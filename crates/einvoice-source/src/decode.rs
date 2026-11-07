use crate::budget::Meter;
use crate::document::{DecodedSpan, Encoding, SourceLocation};
use crate::error::{limit, syntax, Result};
use crate::SourceError;
use std::io::Read;
use xml::common::{TextPosition, XmlVersion};
use xml::reader::{ErrorKind, ParserConfig, XmlEvent};

// NamespaceStack::default and each event snapshot own these strings separately.
pub(crate) const IMPLICIT_NAMESPACE_BYTES: usize = xml::namespace::NS_XML_PREFIX.len()
    + xml::namespace::NS_XML_URI.len()
    + xml::namespace::NS_XMLNS_PREFIX.len()
    + xml::namespace::NS_XMLNS_URI.len()
    + xml::namespace::NS_NO_PREFIX.len()
    + xml::namespace::NS_EMPTY_URI.len();

pub(crate) fn admit_parser(m: &mut Meter<'_>) -> Result<()> {
    m.namespace(256)?;
    m.strings(IMPLICIT_NAMESPACE_BYTES)?;
    // StartDocument owns the decoder's short encoding label, even without an
    // explicit declaration. Reserve it separately from namespace initialization.
    m.strings(32)
}

pub(crate) fn reject_dtd(mut bytes: &[u8]) -> Result<()> {
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
            && marker.iter().enumerate().all(|(i, b)| {
                let unit = &candidate[i * width..(i + 1) * width];
                if width == 1 {
                    unit[0] == *b
                } else if little_endian {
                    unit == [*b, 0]
                } else {
                    unit == [0, *b]
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
            return Err(SourceError::UnsupportedDtd);
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

pub(crate) fn config(m: &Meter<'_>) -> xml::reader::ParserConfig2 {
    ParserConfig::new()
        .ignore_comments(false)
        .trim_whitespace(false)
        .whitespace_to_characters(true)
        .cdata_to_characters(false)
        .coalesce_characters(false)
        .ignore_root_level_whitespace(true)
        .ignore_end_of_stream(false)
        .replace_unknown_entity_references(false)
        .allow_multiple_root_elements(false)
        .ignore_invalid_encoding_declarations(false)
        .max_name_length(m.limits.name_bytes)
        .max_attributes(m.limits.attributes)
        .max_attribute_length(m.limits.value_bytes)
        .max_data_length(m.limits.value_bytes)
        .max_entity_expansion_length(m.limits.value_bytes)
        .max_entity_expansion_depth(1)
}

/// Only the first StartDocument is requested, under a byte-limited reader.
/// The prolog guard MUST have run first. No DTD can reach this reader.
pub(crate) fn header(raw: &[u8], m: &mut Meter<'_>) -> Result<(Encoding, usize)> {
    check_declared_labels(raw, m)?;
    let cap = m.limits.header_bytes;
    m.work(raw.len().min(cap))?;
    // StartDocument can be delayed until a PI target or declaration is read.
    // Two UTF-8 payloads, each bounded by twice the admitted source bytes,
    // cover decoding and parser token/name copies for supported encodings.
    // This is logical content admission, not an allocator-capacity estimate.
    let payload = raw
        .len()
        .min(cap)
        .checked_mul(4)
        .ok_or_else(|| limit("string_bytes", m.limits.string_bytes))?;
    m.strings(payload)?;
    admit_parser(m)?;
    let mut input = raw.take(cap as u64);
    let mut reader = config(m).create_reader(&mut input);
    let event = reader.next();
    drop(reader);
    let event = event.map_err(|e| {
        if input.limit() == 0 && raw.len() > cap {
            limit("header_bytes", cap)
        } else {
            parser_error(e, None)
        }
    })?;
    let (version, label) = match event {
        XmlEvent::StartDocument {
            version, encoding, ..
        } => (version, encoding),
        _ => return Err(SourceError::Alignment("header did not emit StartDocument")),
    };
    if version != XmlVersion::Version10 {
        return Err(SourceError::UnsupportedVersion("1.1".into()));
    }
    // xml-rs 0.8.28's label matcher compares zipped bytes without a length
    // comparison. Exact ASCII-insensitive labels close its prefix acceptance.
    let label_encoding = if ["utf-8", "utf8"]
        .iter()
        .any(|s| label.eq_ignore_ascii_case(s))
    {
        Encoding::Utf8
    } else if ["utf-16", "utf16"]
        .iter()
        .any(|s| label.eq_ignore_ascii_case(s))
    {
        if raw.starts_with(&[255, 254]) {
            Encoding::Utf16Le
        } else if raw.starts_with(&[254, 255]) {
            Encoding::Utf16Be
        } else {
            return Err(syntax("UTF-16 requires BOM"));
        }
    } else if ["iso-8859-1", "latin1"]
        .iter()
        .any(|s| label.eq_ignore_ascii_case(s))
    {
        Encoding::Latin1
    } else if ["us-ascii", "ascii"]
        .iter()
        .any(|s| label.eq_ignore_ascii_case(s))
    {
        Encoding::Ascii
    } else {
        return Err(SourceError::UnsupportedEncoding(label));
    };
    let bom = if raw.starts_with(&[255, 254]) || raw.starts_with(&[254, 255]) {
        2
    } else if raw.starts_with(&[239, 187, 191]) {
        3
    } else {
        0
    };
    if (bom == 3 && label_encoding != Encoding::Utf8)
        || (bom == 2 && !matches!(label_encoding, Encoding::Utf16Le | Encoding::Utf16Be))
    {
        return Err(syntax("BOM/declaration encoding conflict"));
    }
    Ok((label_encoding, bom))
}

// A bounded ASCII-only declaration view, tokenized by xmlparser. This is
// metadata admission, not a document parser or a replacement header authority.
// It closes the pinned parser's prefix-label bug even when that bug causes a
// spurious encoding conflict before StartDocument can be emitted.
fn check_declared_labels(raw: &[u8], m: &mut Meter<'_>) -> Result<()> {
    let (raw, width, little) = if raw.starts_with(&[255, 254]) {
        (&raw[2..], 2, true)
    } else if raw.starts_with(&[254, 255]) {
        (&raw[2..], 2, false)
    } else {
        (raw.strip_prefix(&[239, 187, 191]).unwrap_or(raw), 1, true)
    };
    let unit = |at: usize| -> Option<u8> {
        let b = raw.get(at * width..(at + 1) * width)?;
        if width == 1 || (little && b[1] == 0) {
            Some(b[0])
        } else if !little && b[0] == 0 {
            Some(b[1])
        } else {
            None
        }
    };
    let mut prefix = [0u8; 6];
    for (i, b) in prefix.iter_mut().enumerate() {
        let Some(c) = unit(i) else {
            return Ok(());
        };
        *b = c;
    }
    if &prefix[..5] != b"<?xml" || !b" \t\r\n".contains(&prefix[5]) {
        return Ok(());
    }
    let mut declaration = String::new();
    let mut at = 0usize;
    loop {
        if at
            .checked_add(1)
            .and_then(|n| n.checked_mul(width))
            .filter(|n| *n <= m.limits.header_bytes)
            .is_none()
        {
            return Err(limit("header_bytes", m.limits.header_bytes));
        }
        if at >= m.limits.token_bytes {
            return Err(limit("token_bytes", m.limits.token_bytes));
        }
        let Some(c) = unit(at) else {
            return Ok(());
        };
        if !c.is_ascii() {
            return Err(syntax("non-ASCII XML declaration"));
        }
        m.work(1)?;
        append_char(
            &mut declaration,
            char::from(c),
            m.limits.header_bytes,
            "header_bytes",
            m,
        )?;
        at += 1;
        if declaration.ends_with("?>") {
            break;
        }
    }
    declaration.replace_range(5..6, " ");
    let token = xmlparser::Tokenizer::from(declaration.as_str()).next();
    let Some(Ok(xmlparser::Token::Declaration {
        version, encoding, ..
    })) = token
    else {
        return Err(syntax("invalid XML declaration"));
    };
    let version = version.as_str();
    if version != "1.0" {
        if version.len() > 2 && version[2..].bytes().all(|b| b.is_ascii_digit()) {
            return Err(SourceError::UnsupportedVersion(m.copy(version)?));
        }
        return Err(syntax("invalid XML version"));
    }
    if let Some(encoding) = encoding {
        let label = encoding.as_str();
        if !label
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphabetic)
        {
            return Err(syntax("invalid encoding label"));
        }
        if ![
            "utf-8",
            "utf8",
            "utf-16",
            "utf16",
            "iso-8859-1",
            "latin1",
            "us-ascii",
            "ascii",
        ]
        .iter()
        .any(|s| label.eq_ignore_ascii_case(s))
        {
            return Err(SourceError::UnsupportedEncoding(m.copy(label)?));
        }
    }
    Ok(())
}

pub(crate) fn decode(
    raw: &[u8],
    encoding: Encoding,
    bom: usize,
    m: &mut Meter<'_>,
) -> Result<String> {
    let raw = &raw[bom..];
    if encoding == Encoding::Utf8 {
        if raw.len() > m.limits.decoded_bytes {
            return Err(limit("decoded_bytes", m.limits.decoded_bytes));
        }
        m.work(raw.len())?;
        let s = std::str::from_utf8(raw).map_err(|e| syntax(e.to_string()))?;
        return m.copy(s);
    }
    let mut out = String::new();
    let mut push = |c: char| -> Result<()> {
        m.work(1)?;
        append_char(&mut out, c, m.limits.decoded_bytes, "decoded_bytes", m)
    };
    match encoding {
        Encoding::Utf16Le | Encoding::Utf16Be => {
            if raw.len() % 2 != 0 {
                return Err(syntax("incomplete UTF-16 code unit"));
            }
            let units = raw.chunks_exact(2).map(|b| {
                if encoding == Encoding::Utf16Le {
                    u16::from_le_bytes([b[0], b[1]])
                } else {
                    u16::from_be_bytes([b[0], b[1]])
                }
            });
            for c in char::decode_utf16(units) {
                push(c.map_err(|e| syntax(e.to_string()))?)?;
            }
        }
        Encoding::Latin1 | Encoding::Ascii => {
            for b in raw {
                if encoding == Encoding::Ascii && !b.is_ascii() {
                    return Err(syntax("non-ASCII source byte"));
                }
                push(char::from(*b))?;
            }
        }
        Encoding::Utf8 => unreachable!(),
    }
    Ok(out)
}
pub(crate) fn append_char(
    out: &mut String,
    c: char,
    cap: usize,
    dim: &'static str,
    m: &mut Meter<'_>,
) -> Result<()> {
    if out
        .len()
        .checked_add(c.len_utf8())
        .filter(|n| *n <= cap)
        .is_none()
    {
        return Err(limit(dim, cap));
    }
    m.strings(c.len_utf8())?;
    out.try_reserve(c.len_utf8())
        .map_err(|_| limit("allocation", m.limits.storage_bytes))?;
    out.push(c);
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct Checkpoint {
    at: usize,
    scalar: usize,
    location: SourceLocation,
    raw: (u64, u64),
    cr: bool,
}
/// Checkpoints every 64 scalars bound each location/range lookup to 63 scalars.
#[derive(Debug)]
pub(crate) struct Coordinates {
    points: Vec<Checkpoint>,
}
impl Coordinates {
    pub fn build(text: &str, m: &mut Meter<'_>) -> Result<Self> {
        let mut points = Vec::new();
        let mut p = Checkpoint {
            at: 0,
            scalar: 0,
            location: SourceLocation { line: 1, column: 1 },
            raw: (1, 1),
            cr: false,
        };
        for (at, c) in text.char_indices() {
            m.work(c.len_utf8())?;
            p.at = at;
            if p.scalar % 64 == 0 {
                m.push(&mut points, p)?;
            }
            advance_original(&mut p, c);
        }
        p.at = text.len();
        m.push(&mut points, p)?;
        Ok(Self { points })
    }
    fn at(&self, text: &str, at: usize) -> Checkpoint {
        let i = self
            .points
            .partition_point(|p| p.at <= at)
            .saturating_sub(1);
        let mut p = self.points[i];
        for c in text[p.at..at].chars() {
            advance_original(&mut p, c);
        }
        p.at = at;
        p
    }
    pub fn scalar(&self, text: &str, at: usize) -> usize {
        self.at(text, at).scalar
    }
    pub fn location(&self, text: &str, at: usize) -> SourceLocation {
        self.at(text, at).location
    }
    pub fn lexical_location(&self, text: &str, pos: xmlparser::TextPos) -> Option<SourceLocation> {
        let target = (u64::from(pos.row), u64::from(pos.col));
        let i = self
            .points
            .partition_point(|p| p.raw <= target)
            .saturating_sub(1);
        let mut p = self.points[i];
        for c in text[p.at..].chars() {
            if p.raw == target {
                return Some(p.location);
            }
            if p.raw > target {
                return None;
            }
            advance_original(&mut p, c);
        }
        (p.raw == target).then_some(p.location)
    }
}
fn advance_original(p: &mut Checkpoint, c: char) {
    p.scalar += 1;
    if c == '\n' {
        p.raw.0 += 1;
        p.raw.1 = 1;
    } else {
        p.raw.1 += 1;
    }
    match c {
        '\r' => {
            p.location.line += 1;
            p.location.column = 1;
        }
        '\n' => {
            if !p.cr {
                p.location.line += 1;
            }
            p.location.column = 1;
        }
        _ => p.location.column += 1,
    }
    p.cr = c == '\r';
}

#[derive(Debug, Clone, Copy)]
struct NormalPoint {
    at: usize,
    original: usize,
    pos: (u64, u64),
}
pub(crate) struct Normalized {
    pub text: String,
    points: Vec<NormalPoint>,
}
impl Normalized {
    pub fn build(source: &str, values: &[DecodedSpan], m: &mut Meter<'_>) -> Result<Self> {
        let mut text = String::new();
        let mut points = Vec::new();
        let mut chars = source.char_indices().peekable();
        let mut v = 0;
        let mut count = 0;
        let mut pos = (0, 0);
        while let Some((at, c)) = chars.next() {
            m.work(c.len_utf8())?;
            if count % 64 == 0 {
                m.push(
                    &mut points,
                    NormalPoint {
                        at: text.len(),
                        original: at,
                        pos,
                    },
                )?;
            }
            while v < values.len() && values[v].end <= at {
                v += 1;
            }
            let attr = v < values.len() && values[v].start <= at;
            let c = match c {
                '\r' => {
                    if chars.peek().is_some_and(|(_, c)| *c == '\n') {
                        chars.next();
                    }
                    if attr {
                        ' '
                    } else {
                        '\n'
                    }
                }
                '\n' | '\t' if attr => ' ',
                _ => c,
            };
            append_char(
                &mut text,
                c,
                m.limits.normalized_bytes,
                "normalized_bytes",
                m,
            )?;
            advance_parser(&mut pos, c);
            count += 1;
        }
        m.push(
            &mut points,
            NormalPoint {
                at: text.len(),
                original: source.len(),
                pos,
            },
        )?;
        Ok(Self { text, points })
    }
    /// Parser positions count normalized Unicode scalars and LF, not bytes.
    pub fn original_location(
        &self,
        source: &str,
        coordinates: &Coordinates,
        pos: TextPosition,
    ) -> Option<SourceLocation> {
        let target = (pos.row, pos.column);
        let i = self
            .points
            .partition_point(|p| p.pos <= target)
            .saturating_sub(1);
        let mut p = self.points[i];
        let mut original = source[p.original..].char_indices().peekable();
        for c in self.text[p.at..].chars() {
            if p.pos == target {
                return Some(coordinates.location(source, p.original));
            }
            if p.pos > target {
                return None;
            }
            let (at, raw) = original.next()?;
            p.original = self.points[i].original + at + raw.len_utf8();
            if raw == '\r' && original.peek().is_some_and(|(_, c)| *c == '\n') {
                let (at, c) = original.next()?;
                p.original = self.points[i].original + at + c.len_utf8();
            }
            advance_parser(&mut p.pos, c);
        }
        (p.pos == target).then(|| coordinates.location(source, p.original))
    }
    pub fn encode(&self, encoding: Encoding, bom: usize, m: &mut Meter<'_>) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        let cap = m.limits.normalized_bytes;
        let mut extend = |bytes: &[u8]| -> Result<()> {
            m.work(bytes.len())?;
            if out
                .len()
                .checked_add(bytes.len())
                .filter(|n| *n <= cap)
                .is_none()
            {
                return Err(limit("normalized_bytes", cap));
            }
            m.strings(bytes.len())?;
            out.try_reserve(bytes.len())
                .map_err(|_| limit("allocation", m.limits.storage_bytes))?;
            out.extend_from_slice(bytes);
            Ok(())
        };
        match encoding {
            Encoding::Utf8 => {
                if bom != 0 {
                    extend(&[239, 187, 191])?;
                }
                extend(self.text.as_bytes())?;
            }
            Encoding::Utf16Le | Encoding::Utf16Be => {
                extend(if encoding == Encoding::Utf16Le {
                    &[255, 254]
                } else {
                    &[254, 255]
                })?;
                for unit in self.text.encode_utf16() {
                    extend(&if encoding == Encoding::Utf16Le {
                        unit.to_le_bytes()
                    } else {
                        unit.to_be_bytes()
                    })?;
                }
            }
            Encoding::Latin1 | Encoding::Ascii => {
                for c in self.text.chars() {
                    let b = u8::try_from(c as u32).map_err(|_| {
                        SourceError::Alignment("normalized encoding changed repertoire")
                    })?;
                    extend(&[b])?;
                }
            }
        }
        Ok(out)
    }
}
fn advance_parser(pos: &mut (u64, u64), c: char) {
    if c == '\n' {
        pos.0 += 1;
        pos.1 = 0;
    } else {
        pos.1 += 1;
    }
}

pub(crate) fn parser_error(
    error: xml::reader::Error,
    location: Option<SourceLocation>,
) -> SourceError {
    match error.kind(){
        ErrorKind::Syntax(text) if text=="Entity too big"||text=="This document is larger/more complex than allowed by the parser's configuration"=>SourceError::Resource(crate::Resource { dimension: "parser", limit: None }),
        ErrorKind::Syntax(text) if text.starts_with("Unsupported encoding:")=>SourceError::UnsupportedEncoding(error.to_string()),
        _=>SourceError::Syntax{message:error.to_string(),location},
    }
}

#[cfg(test)]
mod review_tests {
    #[test]
    fn parser_initialization_and_snapshot_admit_actual_builtin_strings() {
        let mut reader = xml::reader::EventReader::new(b"<r/>".as_slice());
        reader.next().unwrap();
        let xml::reader::XmlEvent::StartElement { namespace, .. } = reader.next().unwrap() else {
            panic!("expected root event");
        };
        let payload: usize = namespace.0.iter().map(|(p, u)| p.len() + u.len()).sum();
        assert_eq!(super::IMPLICIT_NAMESPACE_BYTES, payload);
        let limits = crate::SourceLimits {
            string_bytes: payload - 1,
            ..Default::default()
        };
        assert!(matches!(
            super::admit_parser(&mut super::Meter::new(&limits, None)),
            Err(crate::SourceError::Resource(_))
        ));
    }

    #[test]
    fn opaque_backend_limit_does_not_invent_a_numeric_maximum() {
        let mut reader = xml::reader::ParserConfig::new()
            .max_data_length(1)
            .create_reader(b"<r><?p     ?></r>".as_slice());
        let error = loop {
            match reader.next() {
                Err(error) => break error,
                Ok(xml::reader::XmlEvent::EndDocument) => panic!("expected actual parser refusal"),
                _ => {}
            }
        };
        assert!(matches!(
            super::parser_error(error, None),
            crate::SourceError::Resource(crate::Resource {
                dimension: "parser",
                limit: None
            })
        ));
    }
}
