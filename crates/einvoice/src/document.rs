use std::collections::HashSet;
use std::sync::Arc;
use xml::common::Position;
use xml::name::OwnedName;
use xml::reader::{ErrorKind, ParserConfig, XmlEvent};

use crate::{SourceLocation, Target, ValidationReport};

pub(crate) const CII_NS: &str = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100";
pub(crate) const RAM_NS: &str =
    "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100";

/// Import budgets, checked before copying source bytes and while parsing.
/// Parser token/entity budgets are additionally fixed by the pinned XML parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Maximum original source size in bytes (default: 16 MiB).
    pub max_bytes: usize,
    /// Maximum number of simultaneously open elements (default: 128).
    pub max_depth: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_bytes: 16 * 1024 * 1024,
            max_depth: 128,
        }
    }
}

/// A budget exhaustion is not a statement about document conformance.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResourceLimit {
    /// Input exceeds the configured byte budget.
    #[error("source has {actual} bytes; limit is {limit}")]
    InputBytes {
        /// Allowed bytes.
        limit: usize,
        /// Observed bytes.
        actual: usize,
    },
    /// Element nesting exceeds the configured depth budget.
    #[error("element nesting exceeds {limit}")]
    Depth {
        /// Allowed nesting depth.
        limit: usize,
    },
    /// The pinned parser exhausted a token or entity-expansion budget.
    #[error("XML parser budget exhausted: {message}")]
    Parser {
        /// Parser detail, not a stable machine-readable code.
        message: String,
    },
}

/// XML syntax and resource failures, independent of invoice conformance.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    /// The source is not a well-formed XML document supported by the parser.
    #[error("XML syntax error: {message}")]
    XmlSyntax {
        /// Parser explanation.
        message: String,
        /// One-based source position.
        location: SourceLocation,
    },
    /// DTD processing is deliberately unsupported and is stopped before expansion.
    #[error("DTD processing is unsupported")]
    UnsupportedDtd,
    /// The declared encoding requires a decoder the parser does not implement.
    #[error("unsupported XML encoding: {message}")]
    UnsupportedEncoding {
        /// Parser explanation; no claim that the document is invalid.
        message: String,
    },
    /// Parsing stopped due to a budget, not an invoice fault.
    #[error(transparent)]
    ResourceLimit(#[from] ResourceLimit),
}

/// An immutable, syntax-parsed document that owns the exact imported bytes.
///
/// Unknown elements, attributes, encodings, comments and lexical forms are not
/// serialized from a typed model. No editing or complete semantic conversion is
/// offered. Clones share immutable source storage.
#[derive(Debug, Clone)]
pub struct Document {
    bytes: Arc<[u8]>,
    pub(crate) root: OwnedName,
    pub(crate) root_location: SourceLocation,
    pub(crate) profiles: Vec<(String, SourceLocation)>,
}

impl Document {
    /// Parse the entire XML document using default budgets. This is not validation.
    pub fn parse(bytes: impl AsRef<[u8]>) -> Result<Self, ParseError> {
        Self::parse_with_limits(bytes, Limits::default())
    }

    /// Parse with explicit input/depth budgets; never fetches external resources.
    /// DTD processing is unsupported and rejected before entity expansion.
    pub fn parse_with_limits(bytes: impl AsRef<[u8]>, limits: Limits) -> Result<Self, ParseError> {
        let bytes = bytes.as_ref();
        if bytes.len() > limits.max_bytes {
            return Err(ResourceLimit::InputBytes {
                limit: limits.max_bytes,
                actual: bytes.len(),
            }
            .into());
        }
        reject_dtd(bytes)?;
        let mut reader = ParserConfig::new()
            .allow_multiple_root_elements(false)
            .max_name_length(4096)
            .max_attributes(1024)
            .max_attribute_length(1024 * 1024)
            .max_data_length(16 * 1024 * 1024)
            .max_entity_expansion_length(1024 * 1024)
            .max_entity_expansion_depth(10)
            .create_reader(bytes);
        let mut encoding = String::new();
        let mut stack = Vec::new();
        let mut root = None;
        let mut profiles = Vec::new();
        let mut profile: Option<(String, SourceLocation)> = None;
        loop {
            let event = reader.next().map_err(|error| {
                let message = error.to_string();
                // xml-rs 0.8.28 exposes these limits only as Syntax strings.
                // Keep the dependency pinned and these cases covered by tests.
                match error.kind() {
                    ErrorKind::Syntax(text) if text == "Entity too big" || text ==
                        "This document is larger/more complex than allowed by the parser's configuration" => {
                        ParseError::ResourceLimit(ResourceLimit::Parser { message })
                    }
                    ErrorKind::Syntax(text) if text.starts_with("Unsupported encoding:") => {
                        ParseError::UnsupportedEncoding { message }
                    }
                    _ => ParseError::XmlSyntax { message, location: position(error.position()) },
                }
            })?;
            match event {
                XmlEvent::StartDocument {
                    encoding: detected, ..
                } => encoding = detected,
                XmlEvent::StartElement {
                    name, attributes, ..
                } => {
                    if stack.len() >= limits.max_depth {
                        return Err(ResourceLimit::Depth {
                            limit: limits.max_depth,
                        }
                        .into());
                    }
                    let location = position(reader.position());
                    let mut attribute_names = HashSet::new();
                    for attribute in &attributes {
                        if !attribute_names.insert((
                            attribute.name.namespace.as_deref(),
                            attribute.name.local_name.as_str(),
                        )) {
                            return Err(ParseError::XmlSyntax {
                                message: "duplicate attribute expanded name".into(),
                                location,
                            });
                        }
                    }
                    if root.is_none() {
                        root = Some((name.clone(), location.clone()));
                    }
                    stack.push(name);
                    if is_profile_path(&stack) {
                        profile = Some((String::new(), location));
                    }
                }
                XmlEvent::Characters(text) | XmlEvent::CData(text) | XmlEvent::Whitespace(text) => {
                    if let Some((value, _)) = &mut profile {
                        value.push_str(&text);
                    }
                }
                XmlEvent::EndElement { .. } => {
                    if is_profile_path(&stack) {
                        if let Some(value) = profile.take() {
                            profiles.push(value);
                        }
                    }
                    stack.pop();
                }
                XmlEvent::EndDocument => break,
                _ => {}
            }
        }
        crate::syntax::check_namespace_declarations(bytes, &encoding)?;
        // A single root is guaranteed by the XML reader before EndDocument.
        let (root, root_location) = root.ok_or_else(|| ParseError::XmlSyntax {
            message: "missing document element".into(),
            location: SourceLocation { line: 1, column: 1 },
        })?;
        Ok(Self {
            bytes: Arc::from(bytes),
            root,
            root_location,
            profiles,
        })
    }

    /// Retrieve original bytes without normalization or regeneration.
    pub fn original_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Serialize an immutable import by copying its exact original bytes.
    /// This is deliberately separate from [`Target::encode`].
    pub fn to_bytes(&self) -> Vec<u8> {
        self.bytes.to_vec()
    }

    /// Validate this source document against one explicit target.
    pub fn validate(&self, target: &Target) -> ValidationReport {
        target.validate_document(self)
    }
}

fn position(pos: xml::common::TextPosition) -> SourceLocation {
    SourceLocation {
        line: pos.row + 1,
        column: pos.column + 1,
    }
}

fn is_profile_path(stack: &[OwnedName]) -> bool {
    stack.len() == 4
        && stack
            .iter()
            .zip([
                (CII_NS, "CrossIndustryInvoice"),
                (CII_NS, "ExchangedDocumentContext"),
                (RAM_NS, "GuidelineSpecifiedDocumentContextParameter"),
                (RAM_NS, "ID"),
            ])
            .all(|(actual, (namespace, local))| {
                actual.namespace.as_deref() == Some(namespace) && actual.local_name == local
            })
}

// xml-rs 0.8.28 only enters DTD processing before the document element, but its
// parameter-entity declarations bypass expansion budgets. Inspect the prolog
// BEFORE invoking it. This guard recognizes only ASCII XML delimiters, not XML
// grammar: the real parser still checks syntax, comments and declarations. DTD
// text inside comments/PIs is skipped. An unterminated comment/PI is left to the
// parser, which cannot reach a DTD from inside that token.
//
// Match whole code units for BOM-marked UTF-16; all other supported encodings are
// ASCII-compatible. Unknown encodings fail in xml-rs before reaching a DTD.
fn reject_dtd(mut bytes: &[u8]) -> Result<(), ParseError> {
    let (width, little_endian) = if bytes.starts_with(&[0xff, 0xfe]) {
        bytes = &bytes[2..];
        (2, true)
    } else if bytes.starts_with(&[0xfe, 0xff]) {
        bytes = &bytes[2..];
        (2, false)
    } else {
        bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
        (1, true)
    };
    let matches = |candidate: &[u8], marker: &[u8]| -> bool {
        candidate.len() >= marker.len() * width
            && marker.iter().enumerate().all(|(i, byte)| {
                let unit = &candidate[i * width..(i + 1) * width];
                if width == 1 {
                    unit[0] == *byte
                } else if little_endian {
                    unit == [*byte, 0]
                } else {
                    unit == [0, *byte]
                }
            })
    };
    loop {
        while [b' ', b'\t', b'\r', b'\n']
            .iter()
            .any(|c| matches(bytes, &[*c]))
        {
            bytes = &bytes[width..];
        }
        if matches(bytes, b"<!DOCTYPE") {
            return Err(ParseError::UnsupportedDtd);
        }
        let (start, end): (&[u8], &[u8]) = if matches(bytes, b"<!--") {
            (b"<!--", b"-->")
        } else if matches(bytes, b"<?") {
            (b"<?", b"?>")
        } else {
            return Ok(());
        };
        bytes = &bytes[start.len() * width..];
        match bytes
            .windows(end.len() * width)
            .step_by(width)
            .position(|s| matches(s, end))
        {
            Some(index) => bytes = &bytes[(index + end.len()) * width..],
            None => return Ok(()),
        }
    }
}
