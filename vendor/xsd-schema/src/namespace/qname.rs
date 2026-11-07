//! QName parsing and validation
//!
//! Provides QName parsing with NCName validation following XPath2/XSD semantics.
//! - InvalidLexical (FORG0001): Malformed QName syntax
//! - UndefinedPrefix (XPST0081): Prefix not in scope

use super::context::{NamespaceContext, NamespaceContextSnapshot};
use super::table::NameTable;
use crate::ids::NameId;
use std::fmt;

/// Qualified name with interned strings via NameTable
///
/// A QName consists of:
/// - Optional prefix (e.g., "xs" in "xs:string")
/// - Local name (e.g., "string" in "xs:string")
/// - Resolved namespace URI (e.g., XSD namespace)
#[derive(Debug, Clone)]
pub struct QualifiedName {
    /// Namespace URI (None = no namespace)
    pub namespace_uri: Option<NameId>,
    /// Local name part
    pub local_name: NameId,
    /// Original prefix (None = unprefixed)
    pub prefix: Option<NameId>,
}

/// QName equality is defined by namespace URI + local name only (per XML Namespaces).
/// The prefix is a syntactic artifact and does not affect identity.
impl PartialEq for QualifiedName {
    fn eq(&self, other: &Self) -> bool {
        self.namespace_uri == other.namespace_uri && self.local_name == other.local_name
    }
}

impl Eq for QualifiedName {}

impl std::hash::Hash for QualifiedName {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.namespace_uri.hash(state);
        self.local_name.hash(state);
    }
}

impl QualifiedName {
    /// Create a new QualifiedName
    pub fn new(namespace_uri: Option<NameId>, local_name: NameId, prefix: Option<NameId>) -> Self {
        Self {
            namespace_uri,
            local_name,
            prefix,
        }
    }

    /// Create a QualifiedName with no namespace
    pub fn local(local_name: NameId) -> Self {
        Self {
            namespace_uri: None,
            local_name,
            prefix: None,
        }
    }

    /// Check if this QName has a namespace
    pub fn has_namespace(&self) -> bool {
        self.namespace_uri.is_some()
    }

    /// Check if this QName is prefixed
    pub fn is_prefixed(&self) -> bool {
        self.prefix.is_some()
    }
}

/// Error type for QName parsing
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QNameError {
    /// Invalid lexical form (FORG0001)
    InvalidLexical(String),
    /// Undefined prefix (XPST0081)
    UndefinedPrefix(String),
    /// Empty local name
    EmptyLocalName,
}

impl fmt::Display for QNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QNameError::InvalidLexical(s) => write!(f, "Invalid QName syntax: '{}'", s),
            QNameError::UndefinedPrefix(p) => write!(f, "Undefined prefix: '{}'", p),
            QNameError::EmptyLocalName => write!(f, "Empty local name in QName"),
        }
    }
}

impl std::error::Error for QNameError {}

/// Parse a QName string into its components
///
/// # Arguments
///
/// * `qname` - The QName string to parse (e.g., "xs:string" or "localName")
/// * `ns_context` - Namespace context for prefix resolution (mutable for string interning)
/// * `use_default_ns` - Whether to use default namespace for unprefixed names
///
/// # Returns
///
/// A `QualifiedName` with resolved namespace, or an error.
///
/// # Errors
///
/// - `InvalidLexical` if the QName syntax is invalid
/// - `UndefinedPrefix` if the prefix is not bound in the namespace context
pub fn parse_qname(
    qname: &str,
    ns_context: &mut NamespaceContext,
    use_default_ns: bool,
) -> Result<QualifiedName, QNameError> {
    let qname = qname.trim();

    if qname.is_empty() {
        return Err(QNameError::EmptyLocalName);
    }

    // Split on ':' to find prefix
    let (prefix_str, local_str) = match qname.find(':') {
        Some(pos) => {
            if pos == 0 {
                return Err(QNameError::InvalidLexical(qname.to_string()));
            }
            let prefix = &qname[..pos];
            let local = &qname[pos + 1..];

            // Check for multiple colons
            if local.contains(':') {
                return Err(QNameError::InvalidLexical(qname.to_string()));
            }

            (Some(prefix), local)
        }
        None => (None, qname),
    };

    // Validate local name
    if local_str.is_empty() {
        return Err(QNameError::EmptyLocalName);
    }

    if !is_ncname(local_str) {
        return Err(QNameError::InvalidLexical(qname.to_string()));
    }

    // Validate and resolve prefix
    let (namespace_uri, prefix_id) = match prefix_str {
        Some(prefix) => {
            if !is_ncname(prefix) {
                return Err(QNameError::InvalidLexical(qname.to_string()));
            }

            let prefix_id = ns_context.name_table_mut().add(prefix);
            match ns_context.lookup_namespace_by_id(prefix_id) {
                Some(ns_id) => (Some(ns_id), Some(prefix_id)),
                None => return Err(QNameError::UndefinedPrefix(prefix.to_string())),
            }
        }
        None => {
            // Unprefixed name - use default namespace if requested
            let namespace_uri = if use_default_ns {
                ns_context.default_namespace()
            } else {
                None
            };
            (namespace_uri, None)
        }
    };

    let local_id = ns_context.name_table_mut().add(local_str);

    Ok(QualifiedName::new(namespace_uri, local_id, prefix_id))
}

/// Parse a QName string using an immutable `NamespaceContextSnapshot` + `NameTable`
///
/// Same parsing/validation logic as [`parse_qname`] but works with a snapshot
/// of namespace bindings instead of a mutable `NamespaceContext`. This is useful
/// during validation where only a snapshot is available (e.g., xsi:type resolution).
///
/// Uses `name_table.add()` to intern names, since instance values may contain
/// names not yet present in the table.
///
/// # Arguments
///
/// * `qname` - The QName string to parse (e.g., "xs:string" or "localName")
/// * `ns_snapshot` - Snapshot of namespace bindings for prefix resolution
/// * `name_table` - Name table for string interning
/// * `use_default_ns` - Whether to use default namespace for unprefixed names
///
/// # Errors
///
/// - `InvalidLexical` if the QName syntax is invalid
/// - `UndefinedPrefix` if the prefix is not bound in the snapshot
pub fn parse_qname_with_snapshot(
    qname: &str,
    ns_snapshot: &NamespaceContextSnapshot,
    name_table: &NameTable,
    use_default_ns: bool,
) -> Result<QualifiedName, QNameError> {
    let qname = qname.trim();

    if qname.is_empty() {
        return Err(QNameError::EmptyLocalName);
    }

    // Split on ':' to find prefix
    let (prefix_str, local_str) = match qname.find(':') {
        Some(pos) => {
            if pos == 0 {
                return Err(QNameError::InvalidLexical(qname.to_string()));
            }
            let prefix = &qname[..pos];
            let local = &qname[pos + 1..];

            // Check for multiple colons
            if local.contains(':') {
                return Err(QNameError::InvalidLexical(qname.to_string()));
            }

            (Some(prefix), local)
        }
        None => (None, qname),
    };

    // Validate local name
    if local_str.is_empty() {
        return Err(QNameError::EmptyLocalName);
    }

    if !is_ncname(local_str) {
        return Err(QNameError::InvalidLexical(qname.to_string()));
    }

    // Validate and resolve prefix
    let (namespace_uri, prefix_id) = match prefix_str {
        Some(prefix) => {
            if !is_ncname(prefix) {
                return Err(QNameError::InvalidLexical(qname.to_string()));
            }

            let prefix_id = name_table.add(prefix);
            match ns_snapshot.resolve_prefix(prefix_id) {
                Some(ns_id) => (Some(ns_id), Some(prefix_id)),
                None => return Err(QNameError::UndefinedPrefix(prefix.to_string())),
            }
        }
        None => {
            // Unprefixed name - use default namespace if requested
            let namespace_uri = if use_default_ns {
                ns_snapshot.default_namespace()
            } else {
                None
            };
            (namespace_uri, None)
        }
    };

    let local_id = name_table.add(local_str);

    Ok(QualifiedName::new(namespace_uri, local_id, prefix_id))
}

/// Check if a string is a valid NCName (non-colonized name)
///
/// NCName = Name - ':'
/// Simplified check: start with letter or '_', followed by letters, digits, '.', '-', '_'
pub fn is_ncname(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    let mut chars = s.chars();

    // First character must be NameStartChar (excluding ':')
    match chars.next() {
        Some(c) if is_name_start_char(c) => {}
        _ => return false,
    }

    // Remaining characters must be NameChar (excluding ':')
    for c in chars {
        if !is_name_char(c) {
            return false;
        }
    }

    true
}

/// Check if a character is a valid NameStartChar (per XML spec, excluding ':')
fn is_name_start_char(c: char) -> bool {
    matches!(c,
        'A'..='Z' |
        '_' |
        'a'..='z' |
        '\u{C0}'..='\u{D6}' |
        '\u{D8}'..='\u{F6}' |
        '\u{F8}'..='\u{2FF}' |
        '\u{370}'..='\u{37D}' |
        '\u{37F}'..='\u{1FFF}' |
        '\u{200C}'..='\u{200D}' |
        '\u{2070}'..='\u{218F}' |
        '\u{2C00}'..='\u{2FEF}' |
        '\u{3001}'..='\u{D7FF}' |
        '\u{F900}'..='\u{FDCF}' |
        '\u{FDF0}'..='\u{FFFD}' |
        '\u{10000}'..='\u{EFFFF}'
    )
}

/// Check if a character is a valid NameChar (per XML spec, excluding ':')
fn is_name_char(c: char) -> bool {
    is_name_start_char(c)
        || matches!(c,
            '-' |
            '.' |
            '0'..='9' |
            '\u{B7}' |
            '\u{0300}'..='\u{036F}' |
            '\u{203F}'..='\u{2040}'
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_ncname_valid() {
        assert!(is_ncname("foo"));
        assert!(is_ncname("_bar"));
        assert!(is_ncname("foo123"));
        assert!(is_ncname("foo-bar"));
        assert!(is_ncname("foo.bar"));
        assert!(is_ncname("foo_bar"));
        assert!(is_ncname("Élément")); // Unicode
    }

    #[test]
    fn test_is_ncname_invalid() {
        assert!(!is_ncname("")); // Empty
        assert!(!is_ncname("123foo")); // Starts with digit
        assert!(!is_ncname("-foo")); // Starts with hyphen
        assert!(!is_ncname(".foo")); // Starts with dot
        assert!(!is_ncname("foo:bar")); // Contains colon
        assert!(!is_ncname("foo bar")); // Contains space
    }

    #[test]
    fn test_qualified_name_local() {
        let local = QualifiedName::local(NameId(1));
        assert!(!local.has_namespace());
        assert!(!local.is_prefixed());
    }

    #[test]
    fn test_qualified_name_prefixed() {
        let qn = QualifiedName::new(Some(NameId(1)), NameId(2), Some(NameId(3)));
        assert!(qn.has_namespace());
        assert!(qn.is_prefixed());
    }

    // --- parse_qname_with_snapshot tests ---

    /// Helper: create a NameTable + NamespaceContextSnapshot with given bindings
    fn make_snapshot(
        prefixes: &[(&str, &str)],
        default_ns: Option<&str>,
    ) -> (NameTable, NamespaceContextSnapshot) {
        use super::super::context::NamespaceContext;
        let mut table = NameTable::new();
        let mut ctx = NamespaceContext::new(&mut table);
        ctx.push_scope();
        for &(prefix, uri) in prefixes {
            ctx.add_namespace(prefix, uri);
        }
        if let Some(uri) = default_ns {
            ctx.add_namespace("", uri);
        }
        let snapshot = ctx.snapshot();
        drop(ctx);
        (table, snapshot)
    }

    #[test]
    fn test_snapshot_prefixed_qname() {
        let (table, snapshot) = make_snapshot(&[("xs", "http://www.w3.org/2001/XMLSchema")], None);
        let result = parse_qname_with_snapshot("xs:string", &snapshot, &table, true).unwrap();
        assert_eq!(table.resolve(result.local_name), "string");
        assert!(result.prefix.is_some());
        assert_eq!(table.resolve(result.prefix.unwrap()), "xs");
        assert!(result.namespace_uri.is_some());
        assert_eq!(
            table.resolve(result.namespace_uri.unwrap()),
            "http://www.w3.org/2001/XMLSchema"
        );
    }

    #[test]
    fn test_snapshot_unprefixed_with_default_ns() {
        let (table, snapshot) = make_snapshot(&[], Some("http://default.com"));
        let result = parse_qname_with_snapshot("localName", &snapshot, &table, true).unwrap();
        assert_eq!(table.resolve(result.local_name), "localName");
        assert!(result.prefix.is_none());
        assert!(result.namespace_uri.is_some());
        assert_eq!(
            table.resolve(result.namespace_uri.unwrap()),
            "http://default.com"
        );
    }

    #[test]
    fn test_snapshot_unprefixed_without_default_ns() {
        let (table, snapshot) = make_snapshot(&[], None);
        let result = parse_qname_with_snapshot("localName", &snapshot, &table, true).unwrap();
        assert_eq!(table.resolve(result.local_name), "localName");
        assert!(result.namespace_uri.is_none());
    }

    #[test]
    fn test_snapshot_unprefixed_default_ns_not_used() {
        let (table, snapshot) = make_snapshot(&[], Some("http://default.com"));
        // use_default_ns = false => namespace should be None
        let result = parse_qname_with_snapshot("localName", &snapshot, &table, false).unwrap();
        assert!(result.namespace_uri.is_none());
    }

    #[test]
    fn test_snapshot_invalid_ncname_local() {
        let (table, snapshot) = make_snapshot(&[("xs", "http://www.w3.org/2001/XMLSchema")], None);
        let err = parse_qname_with_snapshot("xs:123bad", &snapshot, &table, true).unwrap_err();
        assert!(matches!(err, QNameError::InvalidLexical(_)));
    }

    #[test]
    fn test_snapshot_invalid_ncname_prefix() {
        let (table, snapshot) = make_snapshot(&[], None);
        let err = parse_qname_with_snapshot("123:foo", &snapshot, &table, true).unwrap_err();
        assert!(matches!(err, QNameError::InvalidLexical(_)));
    }

    #[test]
    fn test_snapshot_undefined_prefix() {
        let (table, snapshot) = make_snapshot(&[], None);
        let err = parse_qname_with_snapshot("nope:foo", &snapshot, &table, true).unwrap_err();
        assert!(matches!(err, QNameError::UndefinedPrefix(_)));
    }

    #[test]
    fn test_snapshot_empty_input() {
        let (table, snapshot) = make_snapshot(&[], None);
        let err = parse_qname_with_snapshot("", &snapshot, &table, true).unwrap_err();
        assert!(matches!(err, QNameError::EmptyLocalName));
    }

    #[test]
    fn test_snapshot_whitespace_trimmed() {
        let (table, snapshot) = make_snapshot(&[("xs", "http://www.w3.org/2001/XMLSchema")], None);
        let result = parse_qname_with_snapshot("  xs:string  ", &snapshot, &table, true).unwrap();
        assert_eq!(table.resolve(result.local_name), "string");
    }
}
