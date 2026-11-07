//! Hand-written recursive-descent parser for identity-constraint XPath.
//!
//! Parses the restricted XPath subset used in XSD `<selector>` and `<field>` expressions
//! over pre-lexed tokens from [`IdXPathLexer`].
//!
//! Grammar:
//! ```text
//! Selector ::= Path ( '|' Path )*
//! Field    ::= Path ( '|' Path )*
//! Path     ::= ('.' '//')? Step ( '/' Step )*
//! Step     ::= '.' | '@' NameTest | AxisName '::' NameTest | NameTest
//! NameTest ::= '*' | NCName ':' '*' | NCName ':' NCName | NCName
//! ```

#![allow(dead_code)]

use crate::ids::NameId;
use crate::namespace::context::NamespaceContextSnapshot;
use crate::namespace::table::NameTable;

use super::asttree::{AstPath, AstStep, Asttree, IdentityXPathError, NameTest, NamespaceMatch};
use super::identity_lexer::{IdXPathLexer, IdXPathSpanned, IdXPathToken};

/// Recursive-descent parser for identity-constraint XPath expressions.
pub(crate) struct IdXPathParser<'a> {
    /// Pre-lexed token buffer.
    tokens: Vec<IdXPathSpanned<'a>>,
    /// Current position in the token buffer.
    pos: usize,
    /// Namespace context snapshot (for prefix resolution).
    ns_snapshot: &'a NamespaceContextSnapshot,
    /// Name table (for string interning).
    name_table: &'a NameTable,
    /// Resolved default namespace for unprefixed element names.
    unprefixed_ns: NamespaceMatch,
}

impl<'a> IdXPathParser<'a> {
    /// Create a new parser by lexing the input string.
    pub fn new(
        input: &'a str,
        ns_snapshot: &'a NamespaceContextSnapshot,
        name_table: &'a NameTable,
        unprefixed_ns: NamespaceMatch,
    ) -> Result<Self, IdentityXPathError> {
        let tokens: Vec<_> = IdXPathLexer::new(input).collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            tokens,
            pos: 0,
            ns_snapshot,
            name_table,
            unprefixed_ns,
        })
    }

    // --- Helpers ---

    /// Peek at the current token.
    fn peek(&self) -> Option<&IdXPathToken<'a>> {
        self.tokens.get(self.pos).map(|(_, tok, _)| tok)
    }

    /// Peek at the token `n` positions ahead of current.
    fn peek_at(&self, n: usize) -> Option<&IdXPathToken<'a>> {
        self.tokens.get(self.pos + n).map(|(_, tok, _)| tok)
    }

    /// Get the byte position of the current token (or end of input).
    fn current_position(&self) -> usize {
        self.tokens
            .get(self.pos)
            .map(|(start, _, _)| *start)
            .unwrap_or_else(|| self.tokens.last().map(|(_, _, end)| *end).unwrap_or(0))
    }

    /// Advance past the current token, returning it.
    fn advance(&mut self) -> Option<IdXPathSpanned<'a>> {
        if self.pos < self.tokens.len() {
            let tok = self.tokens[self.pos];
            self.pos += 1;
            Some(tok)
        } else {
            None
        }
    }

    /// Consume the current token if it matches `expected`, or return an error.
    fn eat(
        &mut self,
        expected: IdXPathToken<'_>,
    ) -> Result<IdXPathSpanned<'a>, IdentityXPathError> {
        if self.peek() == Some(&expected) {
            Ok(self.advance().unwrap())
        } else {
            Err(IdentityXPathError::Parse {
                message: format!("expected `{expected:?}`, found {:?}", self.peek()),
                position: self.current_position(),
            })
        }
    }

    /// Check if we've consumed all tokens.
    fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    /// Resolve a prefix string to a namespace NameId.
    fn resolve_prefix(&self, prefix: &str, pos: usize) -> Result<NameId, IdentityXPathError> {
        let prefix_id = self.name_table.add(prefix);
        self.ns_snapshot.resolve_prefix(prefix_id).ok_or_else(|| {
            IdentityXPathError::UnboundPrefix {
                prefix: prefix.to_string(),
                position: pos,
            }
        })
    }

    // --- Grammar productions ---

    /// Parse a selector expression: `Path ( '|' Path )*`.
    ///
    /// Rejects any attribute steps.
    pub fn parse_selector(&mut self) -> Result<Asttree, IdentityXPathError> {
        if self.at_end() {
            return Err(IdentityXPathError::Parse {
                message: "empty selector expression".into(),
                position: 0,
            });
        }

        let mut paths = vec![self.parse_path(false)?];

        while self.peek() == Some(&IdXPathToken::Pipe) {
            self.advance(); // consume '|'
            paths.push(self.parse_path(false)?);
        }

        if !self.at_end() {
            return Err(IdentityXPathError::Parse {
                message: format!("unexpected token {:?} after expression", self.peek()),
                position: self.current_position(),
            });
        }

        Ok(Asttree { paths })
    }

    /// Parse a field expression: `Path ( '|' Path )*`.
    ///
    /// Allows an optional final attribute step in each path.
    pub fn parse_field(&mut self) -> Result<Asttree, IdentityXPathError> {
        if self.at_end() {
            return Err(IdentityXPathError::Parse {
                message: "empty field expression".into(),
                position: 0,
            });
        }

        let mut paths = vec![self.parse_path(true)?];

        while self.peek() == Some(&IdXPathToken::Pipe) {
            self.advance(); // consume '|'
            paths.push(self.parse_path(true)?);
        }

        if !self.at_end() {
            return Err(IdentityXPathError::Parse {
                message: format!("unexpected token {:?} after expression", self.peek()),
                position: self.current_position(),
            });
        }

        Ok(Asttree { paths })
    }

    /// Parse a single path: `('.' '//')? Step ( '/' Step )*`.
    fn parse_path(&mut self, allow_attr: bool) -> Result<AstPath, IdentityXPathError> {
        let mut descendant = false;
        let mut steps = Vec::new();

        // Check for leading '.' followed by '//' or '/'
        if self.peek() == Some(&IdXPathToken::Dot) {
            if self.peek_at(1) == Some(&IdXPathToken::DoubleSlash) {
                // .// prefix
                self.advance(); // consume '.'
                self.advance(); // consume '//'
                descendant = true;
            } else if self.peek_at(1) == Some(&IdXPathToken::Slash) {
                // ./step — '.' is a self-node step, then '/' + more steps
                let pos = self.current_position();
                self.advance(); // consume '.'
                steps.push(AstStep::SelfNode);
                // The '/' will be consumed in the loop below
                // But first check this isn't a trailing slash
                if self.peek() == Some(&IdXPathToken::Slash) && self.peek_at(1).is_none() {
                    return Err(IdentityXPathError::Parse {
                        message: "trailing `/` in path".into(),
                        position: self.current_position(),
                    });
                }
                _ = pos;
            } else if self.peek_at(1).is_none() || self.peek_at(1) == Some(&IdXPathToken::Pipe) {
                // Bare '.' — self node
                self.advance(); // consume '.'
                steps.push(AstStep::SelfNode);
                return Ok(AstPath { descendant, steps });
            } else {
                return Err(IdentityXPathError::Parse {
                    message: format!("unexpected token {:?} after `.`", self.peek_at(1)),
                    position: self.current_position(),
                });
            }
        }

        // Check for absolute path (leading '/')
        if steps.is_empty()
            && (self.peek() == Some(&IdXPathToken::Slash)
                || self.peek() == Some(&IdXPathToken::DoubleSlash))
        {
            return Err(IdentityXPathError::Parse {
                message: "absolute paths are not allowed in identity-constraint XPath".into(),
                position: self.current_position(),
            });
        }

        // Parse first step (if not already consumed as SelfNode)
        if steps.is_empty() {
            steps.push(self.parse_step(allow_attr)?);
        }

        // Parse remaining steps separated by '/'
        while self.peek() == Some(&IdXPathToken::Slash) {
            // Check for trailing slash
            if self.peek_at(1).is_none() {
                return Err(IdentityXPathError::Parse {
                    message: "trailing `/` in path".into(),
                    position: self.current_position(),
                });
            }
            self.advance(); // consume '/'
            steps.push(self.parse_step(allow_attr)?);
        }

        // Validate attribute placement: attribute step must be last
        for (i, step) in steps.iter().enumerate() {
            if matches!(step, AstStep::Attribute(_)) && i < steps.len() - 1 {
                return Err(IdentityXPathError::Restriction {
                    message: "attribute step must be the last step in a path".into(),
                    position: self.current_position(),
                });
            }
        }

        Ok(AstPath { descendant, steps })
    }

    /// Parse a single step: `.` | `@NameTest` | axis `::` NameTest | NameTest.
    fn parse_step(&mut self, allow_attr: bool) -> Result<AstStep, IdentityXPathError> {
        let pos = self.current_position();

        match self.peek() {
            Some(IdXPathToken::Dot) => {
                self.advance();
                Ok(AstStep::SelfNode)
            }
            Some(IdXPathToken::At) => {
                if !allow_attr {
                    return Err(IdentityXPathError::Restriction {
                        message: "attribute axis is not allowed in selector expressions".into(),
                        position: pos,
                    });
                }
                self.advance(); // consume '@'
                let name_test = self.parse_name_test(true)?;
                Ok(AstStep::Attribute(name_test))
            }
            Some(IdXPathToken::NCName(_)) => {
                // Check for explicit axis: NCName '::'
                if self.peek_at(1) == Some(&IdXPathToken::DoubleColon) {
                    self.parse_explicit_axis(allow_attr)
                } else {
                    let name_test = self.parse_name_test(false)?;
                    Ok(AstStep::Child(name_test))
                }
            }
            Some(IdXPathToken::Star) => {
                let name_test = self.parse_name_test(false)?;
                Ok(AstStep::Child(name_test))
            }
            Some(tok) => Err(IdentityXPathError::Parse {
                message: format!("unexpected token `{tok:?}` at start of step"),
                position: pos,
            }),
            None => Err(IdentityXPathError::Parse {
                message: "unexpected end of expression".into(),
                position: pos,
            }),
        }
    }

    /// Parse an explicit axis step: `child::NameTest` or `attribute::NameTest`.
    fn parse_explicit_axis(&mut self, allow_attr: bool) -> Result<AstStep, IdentityXPathError> {
        let (pos, tok, _) = self.advance().unwrap(); // consume axis name NCName
        let axis_name = match tok {
            IdXPathToken::NCName(name) => name,
            _ => unreachable!(),
        };
        self.advance(); // consume '::'

        match axis_name {
            "child" => {
                let name_test = self.parse_name_test(false)?;
                Ok(AstStep::Child(name_test))
            }
            "attribute" => {
                if !allow_attr {
                    return Err(IdentityXPathError::Restriction {
                        message: "attribute axis is not allowed in selector expressions".into(),
                        position: pos,
                    });
                }
                let name_test = self.parse_name_test(true)?;
                Ok(AstStep::Attribute(name_test))
            }
            other => Err(IdentityXPathError::Parse {
                message: format!(
                    "unsupported axis `{other}` in identity-constraint XPath \
                     (only `child` and `attribute` are allowed)"
                ),
                position: pos,
            }),
        }
    }

    /// Parse a name test: `*` | `NCName:*` | `NCName:NCName` | `NCName`.
    ///
    /// When `is_attribute` is true, unprefixed names are always in no namespace
    /// (per XPath: `xpathDefaultNamespace` only affects element names, not attributes).
    fn parse_name_test(&mut self, is_attribute: bool) -> Result<NameTest, IdentityXPathError> {
        let pos = self.current_position();

        match self.peek() {
            Some(IdXPathToken::Star) => {
                self.advance();
                Ok(NameTest::Wildcard)
            }
            Some(IdXPathToken::NCName(_)) => {
                let (start, tok, ncname_end) = self.advance().unwrap();
                let first_name = match tok {
                    IdXPathToken::NCName(n) => n,
                    _ => unreachable!(),
                };

                // Check for ':' (namespace separator)
                // Reject whitespace between NCName and ':' (e.g., "ns :*" is invalid)
                if self.peek() == Some(&IdXPathToken::Colon) {
                    let (colon_start, _, colon_end) = self.tokens[self.pos];
                    if colon_start != ncname_end {
                        return Err(IdentityXPathError::Parse {
                            message: "whitespace is not allowed between namespace prefix and `:`"
                                .into(),
                            position: ncname_end,
                        });
                    }
                    self.advance(); // consume ':'

                    match self.peek() {
                        Some(IdXPathToken::Star) => {
                            // ns:*
                            let (star_start, _, _) = self.tokens[self.pos];
                            if star_start != colon_end {
                                return Err(IdentityXPathError::Parse {
                                    message: "whitespace is not allowed between `:` and `*`".into(),
                                    position: colon_end,
                                });
                            }
                            self.advance();
                            let ns_id = self.resolve_prefix(first_name, start)?;
                            Ok(NameTest::NamespaceWildcard(ns_id))
                        }
                        Some(IdXPathToken::NCName(_)) => {
                            // ns:local
                            let (local_start, _, _) = self.tokens[self.pos];
                            if local_start != colon_end {
                                return Err(IdentityXPathError::Parse {
                                    message: "whitespace is not allowed between `:` and local name"
                                        .into(),
                                    position: colon_end,
                                });
                            }
                            let (_, tok2, _) = self.advance().unwrap();
                            let local = match tok2 {
                                IdXPathToken::NCName(n) => n,
                                _ => unreachable!(),
                            };
                            let ns_id = self.resolve_prefix(first_name, start)?;
                            let local_id = self.name_table.add(local);
                            Ok(NameTest::QName {
                                namespace: NamespaceMatch::Exact(ns_id),
                                local_name: local_id,
                            })
                        }
                        _ => Err(IdentityXPathError::Parse {
                            message: "expected name or `*` after `:`".into(),
                            position: self.current_position(),
                        }),
                    }
                } else {
                    // Unprefixed name: attributes are always in no namespace,
                    // elements use the xpathDefaultNamespace cascade.
                    let local_id = self.name_table.add(first_name);
                    let ns = if is_attribute {
                        NamespaceMatch::NoNamespace
                    } else {
                        self.unprefixed_ns
                    };
                    Ok(NameTest::QName {
                        namespace: ns,
                        local_name: local_id,
                    })
                }
            }
            _ => Err(IdentityXPathError::Parse {
                message: format!("expected name or `*`, found {:?}", self.peek()),
                position: pos,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespace::context::NamespaceContextSnapshot;
    use crate::namespace::table::NameTable;

    /// Helper: create a snapshot with a single prefix binding.
    fn snapshot_with_prefix(
        table: &NameTable,
        prefix: &str,
        uri: &str,
    ) -> NamespaceContextSnapshot {
        let prefix_id = table.add(prefix);
        let uri_id = table.add(uri);
        NamespaceContextSnapshot {
            default_ns: None,
            bindings: vec![(prefix_id, uri_id)],
        }
    }

    /// Helper: compile a selector with no namespace context.
    fn compile_selector(input: &str) -> Result<Asttree, IdentityXPathError> {
        use crate::schema::model::XsdVersion;
        let table = NameTable::new();
        let snapshot = NamespaceContextSnapshot::default();
        Asttree::compile_selector(input, &snapshot, &table, None, None, None, XsdVersion::V1_1)
    }

    /// Helper: compile a field with no namespace context.
    fn compile_field(input: &str) -> Result<Asttree, IdentityXPathError> {
        use crate::schema::model::XsdVersion;
        let table = NameTable::new();
        let snapshot = NamespaceContextSnapshot::default();
        Asttree::compile_field(input, &snapshot, &table, None, None, None, XsdVersion::V1_1)
    }

    // --- Parser tests ---

    #[test]
    fn simple_child() {
        let tree = compile_selector("foo").unwrap();
        assert_eq!(tree.paths.len(), 1);
        let path = &tree.paths[0];
        assert!(!path.descendant);
        assert_eq!(path.steps.len(), 1);
        match &path.steps[0] {
            AstStep::Child(NameTest::QName { namespace, .. }) => {
                assert_eq!(*namespace, NamespaceMatch::NoNamespace);
            }
            other => panic!("expected Child(QName), got {other:?}"),
        }
    }

    #[test]
    fn multi_step() {
        let tree = compile_selector("foo/bar").unwrap();
        assert_eq!(tree.paths.len(), 1);
        assert_eq!(tree.paths[0].steps.len(), 2);
        assert!(matches!(&tree.paths[0].steps[0], AstStep::Child(_)));
        assert!(matches!(&tree.paths[0].steps[1], AstStep::Child(_)));
    }

    #[test]
    fn descendant_prefix() {
        let tree = compile_selector(".//foo").unwrap();
        assert_eq!(tree.paths.len(), 1);
        assert!(tree.paths[0].descendant);
        assert_eq!(tree.paths[0].steps.len(), 1);
    }

    #[test]
    fn self_then_child() {
        let tree = compile_selector("./foo").unwrap();
        assert_eq!(tree.paths.len(), 1);
        let path = &tree.paths[0];
        assert!(!path.descendant);
        assert_eq!(path.steps.len(), 2);
        assert_eq!(path.steps[0], AstStep::SelfNode);
        assert!(matches!(&path.steps[1], AstStep::Child(_)));
    }

    #[test]
    fn union() {
        let tree = compile_selector("a|b|c").unwrap();
        assert_eq!(tree.paths.len(), 3);
    }

    #[test]
    fn wildcard() {
        let tree = compile_selector("*").unwrap();
        assert_eq!(tree.paths[0].steps.len(), 1);
        assert!(matches!(
            &tree.paths[0].steps[0],
            AstStep::Child(NameTest::Wildcard)
        ));
    }

    #[test]
    fn ns_wildcard() {
        use crate::schema::model::XsdVersion;
        let table = NameTable::new();
        let snapshot = snapshot_with_prefix(&table, "ns", "http://example.com");
        let tree = Asttree::compile_selector(
            "ns:*",
            &snapshot,
            &table,
            None,
            None,
            None,
            XsdVersion::V1_1,
        )
        .unwrap();
        assert!(matches!(
            &tree.paths[0].steps[0],
            AstStep::Child(NameTest::NamespaceWildcard(_))
        ));
    }

    #[test]
    fn prefixed_qname() {
        use crate::schema::model::XsdVersion;
        let table = NameTable::new();
        let snapshot = snapshot_with_prefix(&table, "ns", "http://example.com");
        let ns_id = table.add("http://example.com");
        let tree = Asttree::compile_selector(
            "ns:foo",
            &snapshot,
            &table,
            None,
            None,
            None,
            XsdVersion::V1_1,
        )
        .unwrap();
        match &tree.paths[0].steps[0] {
            AstStep::Child(NameTest::QName {
                namespace: NamespaceMatch::Exact(ns),
                local_name,
            }) => {
                assert_eq!(*ns, ns_id);
                assert_eq!(table.resolve(*local_name), "foo");
            }
            other => panic!("expected Child(QName{{Exact, foo}}), got {other:?}"),
        }
    }

    #[test]
    fn explicit_child_axis() {
        let tree = compile_selector("child::foo").unwrap();
        assert_eq!(tree.paths[0].steps.len(), 1);
        assert!(matches!(&tree.paths[0].steps[0], AstStep::Child(_)));
    }

    #[test]
    fn explicit_attr_field() {
        let tree = compile_field("attribute::bar").unwrap();
        assert_eq!(tree.paths[0].steps.len(), 1);
        assert!(matches!(&tree.paths[0].steps[0], AstStep::Attribute(_)));
    }

    #[test]
    fn attr_shorthand() {
        let tree = compile_field("@bar").unwrap();
        assert_eq!(tree.paths[0].steps.len(), 1);
        assert!(matches!(&tree.paths[0].steps[0], AstStep::Attribute(_)));
    }

    #[test]
    fn field_path_with_attr() {
        let tree = compile_field("foo/@bar").unwrap();
        assert_eq!(tree.paths[0].steps.len(), 2);
        assert!(matches!(&tree.paths[0].steps[0], AstStep::Child(_)));
        assert!(matches!(&tree.paths[0].steps[1], AstStep::Attribute(_)));
    }

    #[test]
    fn complex_field() {
        let tree = compile_field(".//a/b/@c").unwrap();
        let path = &tree.paths[0];
        assert!(path.descendant);
        assert_eq!(path.steps.len(), 3);
        assert!(matches!(&path.steps[0], AstStep::Child(_)));
        assert!(matches!(&path.steps[1], AstStep::Child(_)));
        assert!(matches!(&path.steps[2], AstStep::Attribute(_)));
    }

    // --- Rejection tests ---

    #[test]
    fn reject_attr_in_selector() {
        let err = compile_selector("@foo").unwrap_err();
        assert!(matches!(err, IdentityXPathError::Restriction { .. }));
    }

    #[test]
    fn reject_attr_axis_in_selector() {
        let err = compile_selector("attribute::foo").unwrap_err();
        assert!(matches!(err, IdentityXPathError::Restriction { .. }));
    }

    #[test]
    fn reject_attr_not_last() {
        let err = compile_field("@foo/bar").unwrap_err();
        assert!(matches!(err, IdentityXPathError::Restriction { .. }));
    }

    #[test]
    fn reject_unsupported_axis() {
        let err = compile_selector("parent::foo").unwrap_err();
        assert!(matches!(err, IdentityXPathError::Parse { .. }));
    }

    #[test]
    fn reject_absolute_path() {
        let err = compile_selector("/foo").unwrap_err();
        assert!(matches!(err, IdentityXPathError::Parse { .. }));
    }

    #[test]
    fn reject_empty() {
        let err = compile_selector("").unwrap_err();
        assert!(matches!(err, IdentityXPathError::Parse { .. }));
    }

    #[test]
    fn reject_trailing_slash() {
        let err = compile_selector("foo/").unwrap_err();
        assert!(matches!(err, IdentityXPathError::Parse { .. }));
    }

    #[test]
    fn reject_unbound_prefix() {
        let err = compile_selector("unknown:foo").unwrap_err();
        assert!(matches!(err, IdentityXPathError::UnboundPrefix { .. }));
    }

    #[test]
    fn reject_predicate() {
        let err = compile_selector("foo[1]").unwrap_err();
        assert!(matches!(err, IdentityXPathError::Lex(_)));
    }

    #[test]
    fn reject_parent_axis() {
        let err = compile_selector("foo/..").unwrap_err();
        assert!(matches!(err, IdentityXPathError::Lex(_)));
    }

    // --- Attribute namespace tests ---

    #[test]
    fn unprefixed_attr_ignores_xpath_default_ns() {
        // Even with xpathDefaultNamespace set, unprefixed attribute names
        // must resolve to no-namespace (XPath static context rule).
        use crate::schema::model::XsdVersion;
        let table = NameTable::new();
        let snapshot = NamespaceContextSnapshot::default();
        let tree = Asttree::compile_field(
            "@id",
            &snapshot,
            &table,
            Some("http://example.com/default"),
            None,
            None,
            XsdVersion::V1_1,
        )
        .unwrap();
        match &tree.paths[0].steps[0] {
            AstStep::Attribute(NameTest::QName { namespace, .. }) => {
                assert_eq!(*namespace, NamespaceMatch::NoNamespace);
            }
            other => panic!("expected Attribute(QName{{NoNamespace, ..}}), got {other:?}"),
        }
    }

    #[test]
    fn unprefixed_attr_via_explicit_axis_ignores_xpath_default_ns() {
        use crate::schema::model::XsdVersion;
        let table = NameTable::new();
        let snapshot = NamespaceContextSnapshot::default();
        let tree = Asttree::compile_field(
            "attribute::id",
            &snapshot,
            &table,
            Some("http://example.com/default"),
            None,
            None,
            XsdVersion::V1_1,
        )
        .unwrap();
        match &tree.paths[0].steps[0] {
            AstStep::Attribute(NameTest::QName { namespace, .. }) => {
                assert_eq!(*namespace, NamespaceMatch::NoNamespace);
            }
            other => panic!("expected Attribute(QName{{NoNamespace, ..}}), got {other:?}"),
        }
    }

    #[test]
    fn unprefixed_child_uses_xpath_default_ns() {
        // Child axis should still use xpathDefaultNamespace.
        use crate::schema::model::XsdVersion;
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
}
