//! AST types, error handling, and namespace resolution for identity-constraint XPath.
//!
//! This module defines the abstract syntax tree produced by parsing the restricted XPath
//! subset used in XSD `<selector>` and `<field>` expressions. It also provides the
//! `xpathDefaultNamespace` cascade resolution required by XSD 1.1.

#![allow(dead_code)]

use std::fmt;

use crate::ids::NameId;
use crate::namespace::context::NamespaceContextSnapshot;
use crate::namespace::table::NameTable;
use crate::schema::model::XsdVersion;

use super::identity_lexer::IdXPathLexError;

/// Error produced during identity-constraint XPath compilation.
#[derive(Debug, Clone)]
pub enum IdentityXPathError {
    /// Lexer error (invalid character, unsupported syntax).
    Lex(IdXPathLexError),
    /// Parser error (unexpected token, malformed expression).
    Parse { message: String, position: usize },
    /// Unbound namespace prefix.
    UnboundPrefix { prefix: String, position: usize },
    /// Restriction violation (e.g. attribute step in selector, attribute not last).
    Restriction { message: String, position: usize },
}

impl fmt::Display for IdentityXPathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IdentityXPathError::Lex(e) => write!(f, "{e}"),
            IdentityXPathError::Parse { message, position } => {
                write!(
                    f,
                    "identity XPath parse error at position {position}: {message}"
                )
            }
            IdentityXPathError::UnboundPrefix { prefix, position } => {
                write!(
                    f,
                    "identity XPath error at position {position}: unbound prefix `{prefix}`"
                )
            }
            IdentityXPathError::Restriction { message, position } => {
                write!(
                    f,
                    "identity XPath restriction at position {position}: {message}"
                )
            }
        }
    }
}

impl std::error::Error for IdentityXPathError {}

impl From<IdXPathLexError> for IdentityXPathError {
    fn from(e: IdXPathLexError) -> Self {
        IdentityXPathError::Lex(e)
    }
}

/// How an unprefixed element name matches a namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NamespaceMatch {
    /// No namespace (XSD 1.0 default, or `##local`).
    NoNamespace,
    /// An exact namespace URI.
    Exact(NameId),
}

/// A name test in a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NameTest {
    /// `*` — matches any element/attribute.
    Wildcard,
    /// `ns:*` — matches any local name in the given namespace.
    NamespaceWildcard(NameId),
    /// `foo` or `ns:foo` — matches a specific QName.
    QName {
        namespace: NamespaceMatch,
        local_name: NameId,
    },
}

impl NameTest {
    /// Check whether this name test matches a given namespace URI and local name.
    pub(crate) fn matches(&self, namespace_uri: NameId, local_name: NameId) -> bool {
        match self {
            NameTest::Wildcard => true,
            NameTest::NamespaceWildcard(ns) => namespace_uri == *ns,
            NameTest::QName {
                namespace,
                local_name: ln,
            } => {
                *ln == local_name
                    && match namespace {
                        NamespaceMatch::NoNamespace => {
                            // No namespace means empty namespace URI
                            namespace_uri.0 == 0
                        }
                        NamespaceMatch::Exact(ns) => namespace_uri == *ns,
                    }
            }
        }
    }
}

/// A single step in a path expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AstStep {
    /// `.` — the current node.
    SelfNode,
    /// `foo`, `child::foo`, `*`, etc. — child axis.
    Child(NameTest),
    /// `@foo`, `attribute::foo` — attribute axis (field expressions only).
    Attribute(NameTest),
}

/// A single path in a selector/field expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AstPath {
    /// Whether this path starts with `.//` (descendant-or-self).
    pub descendant: bool,
    /// The steps in this path.
    pub steps: Vec<AstStep>,
}

/// Parsed identity-constraint XPath expression (union of paths).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Asttree {
    /// The alternative paths (union branches).
    pub paths: Vec<AstPath>,
}

impl Asttree {
    /// Compile a selector XPath expression.
    ///
    /// Selector expressions may not contain attribute steps.
    /// In XSD 1.0 mode, `xpathDefaultNamespace` is ignored (forced to `NoNamespace`).
    pub fn compile_selector(
        xpath: &str,
        ns_snapshot: &NamespaceContextSnapshot,
        name_table: &NameTable,
        own_xpath_default_ns: Option<&str>,
        schema_xpath_default_ns: Option<NameId>,
        target_namespace: Option<NameId>,
        xsd_version: XsdVersion,
    ) -> Result<Asttree, IdentityXPathError> {
        use super::identity_parser::IdXPathParser;

        // In XSD 1.0 mode, xpathDefaultNamespace is not supported
        let (effective_own, effective_schema) = match xsd_version {
            XsdVersion::V1_0 => (None, None),
            XsdVersion::V1_1 => (own_xpath_default_ns, schema_xpath_default_ns),
        };

        let unprefixed_ns = resolve_effective_default_ns(
            effective_own,
            effective_schema,
            ns_snapshot,
            target_namespace,
            name_table,
        );
        let mut parser = IdXPathParser::new(xpath, ns_snapshot, name_table, unprefixed_ns)?;
        parser.parse_selector()
    }

    /// Compile a field XPath expression.
    ///
    /// Field expressions allow an optional final attribute step.
    /// In XSD 1.0 mode, `xpathDefaultNamespace` is ignored (forced to `NoNamespace`).
    pub fn compile_field(
        xpath: &str,
        ns_snapshot: &NamespaceContextSnapshot,
        name_table: &NameTable,
        own_xpath_default_ns: Option<&str>,
        schema_xpath_default_ns: Option<NameId>,
        target_namespace: Option<NameId>,
        xsd_version: XsdVersion,
    ) -> Result<Asttree, IdentityXPathError> {
        use super::identity_parser::IdXPathParser;

        // In XSD 1.0 mode, xpathDefaultNamespace is not supported
        let (effective_own, effective_schema) = match xsd_version {
            XsdVersion::V1_0 => (None, None),
            XsdVersion::V1_1 => (own_xpath_default_ns, schema_xpath_default_ns),
        };

        let unprefixed_ns = resolve_effective_default_ns(
            effective_own,
            effective_schema,
            ns_snapshot,
            target_namespace,
            name_table,
        );
        let mut parser = IdXPathParser::new(xpath, ns_snapshot, name_table, unprefixed_ns)?;
        parser.parse_field()
    }
}

/// Resolve the effective default namespace for unprefixed element names.
///
/// Cascade: `own_raw` > `schema_raw_id` (resolved via `name_table`).
/// Special values:
/// - `##defaultNamespace` → snapshot's default namespace
/// - `##targetNamespace` → schema's target namespace
/// - `##local` → `NoNamespace`
/// - other string → `Exact(name_table.add(uri))`
/// - no value → `NoNamespace` (XSD 1.0 behavior)
fn resolve_effective_default_ns(
    own_raw: Option<&str>,
    schema_raw_id: Option<NameId>,
    ns_snapshot: &NamespaceContextSnapshot,
    target_namespace: Option<NameId>,
    name_table: &NameTable,
) -> NamespaceMatch {
    // Try own-level first, then schema-level
    let effective = if let Some(raw) = own_raw {
        Some(raw.to_string())
    } else {
        schema_raw_id.map(|id| name_table.resolve(id))
    };

    match effective.as_deref() {
        Some("##defaultNamespace") => match ns_snapshot.default_ns {
            Some(ns_id) => NamespaceMatch::Exact(ns_id),
            None => NamespaceMatch::NoNamespace,
        },
        Some("##targetNamespace") => match target_namespace {
            Some(ns_id) => NamespaceMatch::Exact(ns_id),
            None => NamespaceMatch::NoNamespace,
        },
        Some("##local") => NamespaceMatch::NoNamespace,
        Some(uri) => NamespaceMatch::Exact(name_table.add(uri)),
        None => NamespaceMatch::NoNamespace,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespace::context::NamespaceContextSnapshot;
    use crate::namespace::table::NameTable;

    // --- Namespace resolution tests ---

    #[test]
    fn no_default_unprefixed() {
        let table = NameTable::new();
        let snapshot = NamespaceContextSnapshot::default();
        let result = resolve_effective_default_ns(None, None, &snapshot, None, &table);
        assert_eq!(result, NamespaceMatch::NoNamespace);
    }

    #[test]
    fn own_default_namespace() {
        let table = NameTable::new();
        let uri_id = table.add("http://example.com/ns");
        let snapshot = NamespaceContextSnapshot {
            default_ns: Some(uri_id),
            bindings: vec![],
        };
        let result =
            resolve_effective_default_ns(Some("##defaultNamespace"), None, &snapshot, None, &table);
        assert_eq!(result, NamespaceMatch::Exact(uri_id));
    }

    #[test]
    fn own_target_namespace() {
        let table = NameTable::new();
        let tns = table.add("http://example.com/target");
        let snapshot = NamespaceContextSnapshot::default();
        let result = resolve_effective_default_ns(
            Some("##targetNamespace"),
            None,
            &snapshot,
            Some(tns),
            &table,
        );
        assert_eq!(result, NamespaceMatch::Exact(tns));
    }

    #[test]
    fn own_local() {
        let table = NameTable::new();
        let snapshot = NamespaceContextSnapshot::default();
        let result = resolve_effective_default_ns(Some("##local"), None, &snapshot, None, &table);
        assert_eq!(result, NamespaceMatch::NoNamespace);
    }

    #[test]
    fn own_literal_uri() {
        let table = NameTable::new();
        let snapshot = NamespaceContextSnapshot::default();
        let result =
            resolve_effective_default_ns(Some("http://example.com"), None, &snapshot, None, &table);
        let expected_id = table.add("http://example.com");
        assert_eq!(result, NamespaceMatch::Exact(expected_id));
    }

    #[test]
    fn cascade_own_over_schema() {
        let table = NameTable::new();
        let schema_ns = table.add("http://example.com");
        let snapshot = NamespaceContextSnapshot::default();
        // own = ##local wins over schema-level URI
        let result =
            resolve_effective_default_ns(Some("##local"), Some(schema_ns), &snapshot, None, &table);
        assert_eq!(result, NamespaceMatch::NoNamespace);
    }

    #[test]
    fn cascade_schema_fallback() {
        let table = NameTable::new();
        let schema_ns = table.add("http://example.com");
        let snapshot = NamespaceContextSnapshot::default();
        // own = None, falls through to schema-level
        let result = resolve_effective_default_ns(None, Some(schema_ns), &snapshot, None, &table);
        assert_eq!(result, NamespaceMatch::Exact(schema_ns));
    }

    #[test]
    fn default_ns_absent() {
        let table = NameTable::new();
        let snapshot = NamespaceContextSnapshot {
            default_ns: None,
            bindings: vec![],
        };
        let result =
            resolve_effective_default_ns(Some("##defaultNamespace"), None, &snapshot, None, &table);
        assert_eq!(result, NamespaceMatch::NoNamespace);
    }

    #[test]
    fn target_ns_absent() {
        let table = NameTable::new();
        let snapshot = NamespaceContextSnapshot::default();
        let result =
            resolve_effective_default_ns(Some("##targetNamespace"), None, &snapshot, None, &table);
        assert_eq!(result, NamespaceMatch::NoNamespace);
    }

    // --- NameTest::matches tests ---

    #[test]
    fn wildcard_matches_anything() {
        let table = NameTable::new();
        let ns = table.add("http://example.com");
        let ln = table.add("foo");
        assert!(NameTest::Wildcard.matches(ns, ln));
    }

    #[test]
    fn namespace_wildcard_matches_same_ns() {
        let table = NameTable::new();
        let ns = table.add("http://example.com");
        let ln = table.add("foo");
        assert!(NameTest::NamespaceWildcard(ns).matches(ns, ln));
    }

    #[test]
    fn namespace_wildcard_rejects_different_ns() {
        let table = NameTable::new();
        let ns1 = table.add("http://example.com/1");
        let ns2 = table.add("http://example.com/2");
        let ln = table.add("foo");
        assert!(!NameTest::NamespaceWildcard(ns1).matches(ns2, ln));
    }

    #[test]
    fn qname_exact_match() {
        let table = NameTable::new();
        let ns = table.add("http://example.com");
        let ln = table.add("foo");
        let test = NameTest::QName {
            namespace: NamespaceMatch::Exact(ns),
            local_name: ln,
        };
        assert!(test.matches(ns, ln));
    }

    #[test]
    fn qname_no_namespace_match() {
        let table = NameTable::new();
        let ln = table.add("foo");
        let test = NameTest::QName {
            namespace: NamespaceMatch::NoNamespace,
            local_name: ln,
        };
        use crate::namespace::table::well_known;
        // NameId(0) = empty string = no namespace
        assert!(test.matches(well_known::EMPTY, ln));
    }

    #[test]
    fn qname_rejects_wrong_local() {
        let table = NameTable::new();
        let ns = table.add("http://example.com");
        let ln1 = table.add("foo");
        let ln2 = table.add("bar");
        let test = NameTest::QName {
            namespace: NamespaceMatch::Exact(ns),
            local_name: ln1,
        };
        assert!(!test.matches(ns, ln2));
    }

    // --- XSD version gating tests ---

    #[test]
    fn compile_selector_v10_ignores_own_xpath_default_ns() {
        // In XSD 1.0 mode, xpathDefaultNamespace should be ignored,
        // so unprefixed element names resolve to NoNamespace.
        let table = NameTable::new();
        let snapshot = NamespaceContextSnapshot::default();
        let tree = Asttree::compile_selector(
            "foo",
            &snapshot,
            &table,
            Some("http://example.com/default"),
            None,
            None,
            XsdVersion::V1_0,
        )
        .unwrap();
        match &tree.paths[0].steps[0] {
            AstStep::Child(NameTest::QName { namespace, .. }) => {
                assert_eq!(*namespace, NamespaceMatch::NoNamespace);
            }
            other => panic!("expected Child(QName{{NoNamespace, ..}}), got {other:?}"),
        }
    }

    #[test]
    fn compile_selector_v11_applies_own_xpath_default_ns() {
        // In XSD 1.1 mode, xpathDefaultNamespace should be applied.
        let table = NameTable::new();
        let snapshot = NamespaceContextSnapshot::default();
        let ns_id = table.add("http://example.com/default");
        let tree = Asttree::compile_selector(
            "foo",
            &snapshot,
            &table,
            Some("http://example.com/default"),
            None,
            None,
            XsdVersion::V1_1,
        )
        .unwrap();
        match &tree.paths[0].steps[0] {
            AstStep::Child(NameTest::QName { namespace, .. }) => {
                assert_eq!(*namespace, NamespaceMatch::Exact(ns_id));
            }
            other => panic!("expected Child(QName{{Exact, ..}}), got {other:?}"),
        }
    }

    #[test]
    fn compile_selector_v10_ignores_schema_xpath_default_ns() {
        // In XSD 1.0 mode, even schema-level xpathDefaultNamespace is ignored.
        let table = NameTable::new();
        let schema_ns = table.add("http://example.com/schema");
        let snapshot = NamespaceContextSnapshot::default();
        let tree = Asttree::compile_selector(
            "foo",
            &snapshot,
            &table,
            None,
            Some(schema_ns),
            None,
            XsdVersion::V1_0,
        )
        .unwrap();
        match &tree.paths[0].steps[0] {
            AstStep::Child(NameTest::QName { namespace, .. }) => {
                assert_eq!(*namespace, NamespaceMatch::NoNamespace);
            }
            other => panic!("expected Child(QName{{NoNamespace, ..}}), got {other:?}"),
        }
    }

    #[test]
    fn compile_field_v10_ignores_xpath_default_ns() {
        // In XSD 1.0 mode, field compilation ignores xpathDefaultNamespace.
        let table = NameTable::new();
        let snapshot = NamespaceContextSnapshot::default();
        let tree = Asttree::compile_field(
            "foo",
            &snapshot,
            &table,
            Some("http://example.com/default"),
            None,
            None,
            XsdVersion::V1_0,
        )
        .unwrap();
        match &tree.paths[0].steps[0] {
            AstStep::Child(NameTest::QName { namespace, .. }) => {
                assert_eq!(*namespace, NamespaceMatch::NoNamespace);
            }
            other => panic!("expected Child(QName{{NoNamespace, ..}}), got {other:?}"),
        }
    }
}
