//! Attribute parsing and validation
//!
//! This module handles parsing and validation of XSD element attributes.

use quick_xml::events::attributes::Attribute;
use quick_xml::XmlVersion;

use crate::error::{SchemaError, SchemaResult};
use crate::ids::NameId;
use crate::namespace::{NameTable, NamespaceContext, XS_NAMESPACE};
use crate::parser::location::SourceRef;
use crate::schema::annotation::ForeignAttribute;

/// Parsed attribute value
#[derive(Debug, Clone)]
pub struct ParsedAttribute {
    /// Namespace (None for unqualified attributes)
    pub namespace: Option<NameId>,
    /// Local name
    pub local_name: NameId,
    /// Prefix (for QName reconstruction)
    pub prefix: Option<NameId>,
    /// Value as string
    pub value: String,
    /// Source location
    pub source: Option<SourceRef>,
}

impl ParsedAttribute {
    /// Check if this is a namespace declaration (xmlns or xmlns:prefix)
    pub fn is_namespace_decl(&self, xmlns_prefix_id: NameId, xmlns_ns_id: NameId) -> bool {
        // xmlns:foo or xmlns
        self.prefix == Some(xmlns_prefix_id)
            || self.local_name == xmlns_prefix_id
            || self.namespace == Some(xmlns_ns_id)
    }

    /// Check if this is an XSD attribute
    pub fn is_xsd_attribute(&self, xsd_ns_id: NameId) -> bool {
        // XSD attributes are either unqualified or in the XSD namespace
        self.namespace.is_none() || self.namespace == Some(xsd_ns_id)
    }
}

/// Parse attributes from a quick-xml BytesStart (values normalized with the
/// XML 1.0 rules)
pub fn parse_attributes<'a>(
    attrs: impl Iterator<Item = Result<Attribute<'a>, quick_xml::events::attributes::AttrError>>,
    ns_context: &mut NamespaceContext,
    source: Option<SourceRef>,
) -> SchemaResult<Vec<ParsedAttribute>> {
    parse_attributes_in(attrs, ns_context, source, XmlVersion::Implicit1_0)
}

/// [`parse_attributes`] with the values normalized for the document's XML
/// `version` (XML 1.1 also reads NEL, CR NEL and LINE SEPARATOR as line
/// ends).
pub(crate) fn parse_attributes_in<'a>(
    attrs: impl Iterator<Item = Result<Attribute<'a>, quick_xml::events::attributes::AttrError>>,
    ns_context: &mut NamespaceContext,
    source: Option<SourceRef>,
    version: XmlVersion,
) -> SchemaResult<Vec<ParsedAttribute>> {
    let mut result = Vec::new();

    for attr_result in attrs {
        let attr = attr_result.map_err(|e| SchemaError::XmlError {
            message: format!("Attribute error: {}", e),
            location: None,
        })?;

        let name = attr.key.as_ref();
        // XML attribute-value normalization (XML 1.0 / 1.1 §3.3.3) plus
        // predefined-entity resolution; works without quick-xml's `encoding`
        // feature.
        let value = attr
            .normalized_value(version)
            .map_err(|e| SchemaError::XmlError {
                message: format!("Attribute value decode error: {}", e),
                location: None,
            })?
            .into_owned();

        // Split into prefix and local name
        let (local_name_bytes, prefix_bytes) = crate::parser::reader::split_qname(name);

        let name_table = ns_context.name_table_mut();
        let local_name_str =
            std::str::from_utf8(local_name_bytes).map_err(|e| SchemaError::XmlError {
                message: format!("Invalid UTF-8 in attribute name: {}", e),
                location: None,
            })?;
        let local_name = name_table.add(local_name_str);

        let prefix = match prefix_bytes {
            Some(p) => {
                let prefix_str = std::str::from_utf8(p).map_err(|e| SchemaError::XmlError {
                    message: format!("Invalid UTF-8 in prefix: {}", e),
                    location: None,
                })?;
                Some(name_table.add(prefix_str))
            }
            None => None,
        };

        // Resolve namespace from prefix
        let namespace = match prefix {
            Some(prefix_id) => ns_context.lookup_namespace_by_id(prefix_id),
            None => None, // Unqualified attributes have no namespace
        };

        result.push(ParsedAttribute {
            namespace,
            local_name,
            prefix,
            value,
            source: source.clone(),
        });
    }

    Ok(result)
}

/// Extract XSD attributes and foreign attributes from parsed attributes
pub fn categorize_attributes(
    attrs: Vec<ParsedAttribute>,
    name_table: &NameTable,
) -> (Vec<ParsedAttribute>, Vec<ForeignAttribute>) {
    let xsd_ns = name_table.get(XS_NAMESPACE);

    let mut xsd_attrs = Vec::new();
    let mut foreign_attrs = Vec::new();

    for attr in attrs {
        // Skip namespace declarations
        let xmlns_id = name_table.get("xmlns");
        let xmlns_ns_id = name_table.get(crate::namespace::XMLNS_NAMESPACE);

        if let (Some(xmlns), Some(xmlns_ns)) = (xmlns_id, xmlns_ns_id) {
            if attr.is_namespace_decl(xmlns, xmlns_ns) {
                continue;
            }
        }

        // Categorize as XSD or foreign
        match (attr.namespace, xsd_ns) {
            (None, _) => {
                // Unqualified attribute - could be XSD attribute
                xsd_attrs.push(attr);
            }
            (Some(ns), Some(xsd)) if ns == xsd => {
                // Explicitly in XSD namespace
                xsd_attrs.push(attr);
            }
            _ => {
                // Foreign attribute
                foreign_attrs.push(ForeignAttribute {
                    namespace: attr.namespace,
                    local_name: attr.local_name,
                    prefix: attr.prefix,
                    value: attr.value,
                    source: attr.source,
                });
            }
        }
    }

    (xsd_attrs, foreign_attrs)
}

/// Attribute lookup helper
pub struct AttributeMap {
    attrs: Vec<ParsedAttribute>,
}

impl AttributeMap {
    /// Create from parsed attributes (XSD attributes only)
    pub fn new(attrs: Vec<ParsedAttribute>) -> Self {
        Self { attrs }
    }

    /// Get an attribute by local name
    pub fn get(&self, name_id: NameId) -> Option<&ParsedAttribute> {
        self.attrs.iter().find(|a| a.local_name == name_id)
    }

    /// Get an attribute value by local name
    pub fn get_value(&self, name_id: NameId) -> Option<&str> {
        self.get(name_id).map(|a| a.value.as_str())
    }

    /// Get an attribute value by local name string (looks up in name table)
    pub fn get_value_by_name(&self, name_table: &NameTable, name: &str) -> Option<&str> {
        let name_id = name_table.get(name)?;
        self.get_value(name_id)
    }

    /// Check if an attribute exists
    pub fn has(&self, name_id: NameId) -> bool {
        self.attrs.iter().any(|a| a.local_name == name_id)
    }

    /// Get all attribute names
    pub fn names(&self) -> impl Iterator<Item = NameId> + '_ {
        self.attrs.iter().map(|a| a.local_name)
    }

    /// Remove an attribute and return it
    pub fn take(&mut self, name_id: NameId) -> Option<ParsedAttribute> {
        if let Some(pos) = self.attrs.iter().position(|a| a.local_name == name_id) {
            Some(self.attrs.remove(pos))
        } else {
            None
        }
    }

    /// Get remaining attributes (for detecting unknown attributes)
    pub fn remaining(&self) -> &[ParsedAttribute] {
        &self.attrs
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.attrs.is_empty()
    }
}

/// Parse a boolean attribute value.
///
/// `xs:boolean` has a fixed `whiteSpace=collapse` facet (XSD Part 2 §3.3.2),
/// so `" 1 "` must parse as `true`. We do **not** use [`str::trim`] because
/// it strips the full Unicode whitespace set, but §4.3.6 defines the
/// whiteSpace facet over only `#x20 #x9 #xA #xD`. Values padded with
/// non-XML whitespace (e.g. NBSP `U+00A0`) must still be rejected.
pub fn parse_boolean(value: &str) -> Result<bool, String> {
    let is_xml_ws = |c: char| matches!(c, ' ' | '\t' | '\n' | '\r');
    match value.trim_matches(is_xml_ws) {
        "true" | "1" => Ok(true),
        "false" | "0" => Ok(false),
        _ => Err(format!("Invalid boolean value: '{}'", value)),
    }
}

/// Parse an occurrence count (minOccurs/maxOccurs)
///
/// XSD `nonNegativeInteger` has no upper bound, so literals larger than
/// `u32::MAX` are schema-valid (the W3C suite uses e.g.
/// `maxOccurs="79228162514264337593543950335"`). Occurrence bounds are
/// represented as `u32`, so such a literal **saturates to `u32::MAX`**
/// (4 294 967 295) and is then enforced exactly at that value. This is a
/// documented implementation limit — the bound is never widened to
/// `unbounded` — and it is only observable on a document with more than
/// `u32::MAX` sibling occurrences. Every literal that fits in `u32` is exact.
pub fn parse_occurs(value: &str) -> Result<Option<u32>, String> {
    if value == "unbounded" {
        Ok(None)
    } else {
        // An `xs:nonNegativeInteger`: zero "may be preceded by a positive
        // ("+") or a negative ("-") sign" (XSD 1.0 §3.3.20.1).
        let value = crate::types::validators::unsigned_lexical(value);
        match value.parse::<u32>() {
            Ok(n) => Ok(Some(n)),
            Err(_) => {
                // Accept valid non-negative integers that overflow u32:
                // saturate to the representable maximum (documented above).
                if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) {
                    Ok(Some(u32::MAX))
                } else {
                    Err(format!("Invalid occurrence value: '{}'", value))
                }
            }
        }
    }
}

/// Parse a use attribute (required/optional/prohibited)
pub fn parse_use(value: &str) -> Result<crate::types::complex::AttributeUseKind, String> {
    use crate::types::complex::AttributeUseKind;
    match value {
        "required" => Ok(AttributeUseKind::Required),
        "optional" => Ok(AttributeUseKind::Optional),
        "prohibited" => Ok(AttributeUseKind::Prohibited),
        _ => Err(format!("Invalid use value: '{}'", value)),
    }
}

/// Parse a processContents attribute
pub fn parse_process_contents(value: &str) -> Result<crate::schema::ProcessContents, String> {
    value
        .parse()
        .map_err(|_| format!("Invalid processContents value: '{}'", value))
}

/// Parse a form attribute (qualified/unqualified)
pub fn parse_form(value: &str) -> Result<crate::schema::FormKind, String> {
    match value {
        "qualified" => Ok(crate::schema::FormKind::Qualified),
        "unqualified" => Ok(crate::schema::FormKind::Unqualified),
        _ => Err(format!("Invalid form value: '{}'", value)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_boolean() {
        assert_eq!(parse_boolean("true"), Ok(true));
        assert_eq!(parse_boolean("1"), Ok(true));
        assert_eq!(parse_boolean("false"), Ok(false));
        assert_eq!(parse_boolean("0"), Ok(false));
        assert!(parse_boolean("yes").is_err());
    }

    #[test]
    fn test_parse_occurs() {
        assert_eq!(parse_occurs("0"), Ok(Some(0)));
        assert_eq!(parse_occurs("1"), Ok(Some(1)));
        assert_eq!(parse_occurs("100"), Ok(Some(100)));
        assert_eq!(parse_occurs("unbounded"), Ok(None));
        assert!(parse_occurs("invalid").is_err());
        // Documented implementation limit: literals beyond u32 saturate to
        // u32::MAX (exact at that value) rather than becoming unbounded.
        assert_eq!(parse_occurs("4294967295"), Ok(Some(u32::MAX)));
        assert_eq!(parse_occurs("4294967296"), Ok(Some(u32::MAX)));
        assert_eq!(
            parse_occurs("79228162514264337593543950335"),
            Ok(Some(u32::MAX))
        );
        assert!(parse_occurs("").is_err());
        assert!(parse_occurs("-1").is_err());
        assert_eq!(parse_occurs("-0"), Ok(Some(0)));
        assert_eq!(parse_occurs("-00"), Ok(Some(0)));
        assert_eq!(parse_occurs("+0"), Ok(Some(0)));
    }

    #[test]
    fn test_parse_use() {
        use crate::types::complex::AttributeUseKind;
        assert_eq!(parse_use("required"), Ok(AttributeUseKind::Required));
        assert_eq!(parse_use("optional"), Ok(AttributeUseKind::Optional));
        assert_eq!(parse_use("prohibited"), Ok(AttributeUseKind::Prohibited));
        assert!(parse_use("invalid").is_err());
    }

    #[test]
    fn test_parse_process_contents() {
        use crate::schema::ProcessContents;
        assert_eq!(
            parse_process_contents("strict"),
            Ok(ProcessContents::Strict)
        );
        assert_eq!(parse_process_contents("lax"), Ok(ProcessContents::Lax));
        assert_eq!(parse_process_contents("skip"), Ok(ProcessContents::Skip));
        assert!(parse_process_contents("invalid").is_err());
    }

    #[test]
    fn test_parse_form() {
        use crate::schema::FormKind;
        assert_eq!(parse_form("qualified"), Ok(FormKind::Qualified));
        assert_eq!(parse_form("unqualified"), Ok(FormKind::Unqualified));
        assert!(parse_form("invalid").is_err());
    }

    #[test]
    fn test_attribute_map() {
        let attrs = vec![
            ParsedAttribute {
                namespace: None,
                local_name: NameId(1),
                prefix: None,
                value: "value1".to_string(),
                source: None,
            },
            ParsedAttribute {
                namespace: None,
                local_name: NameId(2),
                prefix: None,
                value: "value2".to_string(),
                source: None,
            },
        ];

        let map = AttributeMap::new(attrs);
        assert!(map.has(NameId(1)));
        assert!(map.has(NameId(2)));
        assert!(!map.has(NameId(3)));

        assert_eq!(map.get_value(NameId(1)), Some("value1"));
        assert_eq!(map.get_value(NameId(3)), None);
    }

    #[test]
    fn test_attribute_map_take() {
        let attrs = vec![ParsedAttribute {
            namespace: None,
            local_name: NameId(1),
            prefix: None,
            value: "value1".to_string(),
            source: None,
        }];

        let mut map = AttributeMap::new(attrs);
        assert!(!map.is_empty());

        let taken = map.take(NameId(1));
        assert!(taken.is_some());
        assert!(map.is_empty());
    }
}
