//! XPath 2.0 Parser module.
//!
//! This module provides the public API for parsing XPath 2.0 expressions.
//! It uses the LALRPOP-generated parser with a custom stateful lexer.

use crate::xpath::arena::{AstArena, AstNodeId, SourceSpan};
use crate::xpath::ast::AstNode;
use crate::xpath::error::XPathError;
use crate::xpath::lexer::{Lexer, LexerError, Token};
use crate::xpath::{XPathMode, XPathParseOptions};
use std::fmt;

// The LALRPOP-generated parser.
// This uses the lalrpop_mod! macro to include the generated code.
// The grammar is defined in src/xpath/parser.lalrpop
lalrpop_util::lalrpop_mod!(
    #[allow(clippy::all)]
    #[allow(unused)]
    #[allow(dead_code)]
    pub xpath_grammar,
    "/xpath/parser.rs"
);

/// Error type for XPath parsing.
///
/// `#[non_exhaustive]`: a `match` needs a wildcard arm; later variants are
/// additive.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ParseError {
    /// Lexer error (tokenization failed).
    Lexer(LexerError),
    /// Parser error (grammar mismatch).
    Parser {
        message: String,
        location: Option<usize>,
    },
    /// Unexpected end of input.
    UnexpectedEof,
    /// The expression nests deeper than
    /// [`MAX_EXPRESSION_DEPTH`](crate::xpath::depth::MAX_EXPRESSION_DEPTH):
    /// a limit of this implementation (XPath 2.0 §2.3.1), not a syntax error.
    /// It converts to [`XPathError::LimitExceeded`].
    #[non_exhaustive]
    LimitExceeded {
        /// The limit, [`MAX_EXPRESSION_DEPTH`](crate::xpath::depth::MAX_EXPRESSION_DEPTH).
        max: usize,
        /// The nesting depth of the expression (greater than `max`).
        found: usize,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Lexer(e) => write!(f, "Lexer error: {}", e),
            ParseError::Parser { message, location } => {
                if let Some(loc) = location {
                    write!(f, "Parse error at position {}: {}", loc, message)
                } else {
                    write!(f, "Parse error: {}", message)
                }
            }
            ParseError::UnexpectedEof => write!(f, "Unexpected end of input"),
            ParseError::LimitExceeded { max, found } => write!(
                f,
                "XPath limit xpath::depth::MAX_EXPRESSION_DEPTH exceeded: nesting depth {found}, \
                 the limit is {max}"
            ),
        }
    }
}

impl std::error::Error for ParseError {}

impl From<LexerError> for ParseError {
    fn from(e: LexerError) -> Self {
        ParseError::Lexer(e)
    }
}

/// Refuse an expression nested deeper than the limit, before anything walks
/// it recursively (see [`crate::xpath::depth`]).
fn check_depth(arena: &AstArena, root: AstNodeId) -> Result<(), ParseError> {
    let max = crate::xpath::depth::MAX_EXPRESSION_DEPTH;
    let found = crate::xpath::depth::expression_depth(arena, root);
    if found > max {
        return Err(ParseError::LimitExceeded { max, found });
    }
    Ok(())
}

/// Map a LALRPOP parse error to our `ParseError` type.
fn map_lalrpop_error(e: lalrpop_util::ParseError<usize, Token, LexerError>) -> ParseError {
    match e {
        lalrpop_util::ParseError::InvalidToken { location } => ParseError::Parser {
            message: "Invalid token".to_string(),
            location: Some(location),
        },
        lalrpop_util::ParseError::UnrecognizedEof { location, expected } => ParseError::Parser {
            message: format!("Unexpected end of input, expected one of: {:?}", expected),
            location: Some(location),
        },
        lalrpop_util::ParseError::UnrecognizedToken { token, expected } => ParseError::Parser {
            message: format!(
                "Unexpected token {:?}, expected one of: {:?}",
                token.1, expected
            ),
            location: Some(token.0),
        },
        lalrpop_util::ParseError::ExtraToken { token } => ParseError::Parser {
            message: format!("Extra token: {:?}", token.1),
            location: Some(token.0),
        },
        lalrpop_util::ParseError::User { error } => ParseError::Lexer(error),
    }
}

/// Result of parsing an XPath expression.
#[derive(Debug)]
pub struct ParsedXPath {
    /// The arena containing all AST nodes.
    pub arena: AstArena,
    /// The root node ID of the parsed expression.
    pub root: AstNodeId,
    /// Source span of the entire expression.
    pub span: SourceSpan,
}

impl ParsedXPath {
    /// Get a reference to the root AST node.
    pub fn root_node(&self) -> &AstNode {
        self.arena.get(self.root)
    }

    /// Get a reference to any node by ID.
    pub fn get_node(&self, id: AstNodeId) -> &AstNode {
        self.arena.get(id)
    }

    /// Get the number of nodes in the AST.
    pub fn node_count(&self) -> usize {
        self.arena.len()
    }
}

/// Parse an XPath 2.0 expression string.
///
/// Returns a `ParsedXPath` containing the AST arena and root node ID.
///
/// # Example
///
/// ```
/// use xsd_schema::xpath::parser::parse;
///
/// let result = parse("/a/b/c").unwrap();
/// println!("Parsed {} nodes", result.node_count());
/// ```
pub fn parse(input: &str) -> Result<ParsedXPath, ParseError> {
    let mut arena = AstArena::new();
    let lexer = Lexer::new(input);

    let root = xpath_grammar::ExprParser::new()
        .parse(&mut arena, lexer)
        .map_err(map_lalrpop_error)?;
    check_depth(&arena, root)?;

    Ok(ParsedXPath {
        arena,
        root,
        span: SourceSpan::new(0, input.len()),
    })
}

/// Parse an XPath expression and return just the root node ID.
///
/// This is a convenience function when you only need the arena and root.
pub fn parse_expr(input: &str, arena: &mut AstArena) -> Result<AstNodeId, ParseError> {
    let lexer = Lexer::new(input);

    let root = xpath_grammar::ExprParser::new()
        .parse(arena, lexer)
        .map_err(map_lalrpop_error)?;
    check_depth(arena, root)?;
    Ok(root)
}

/// Parse an XPath expression with a specific mode (XPath 1.0 or 2.0).
pub fn parse_with_mode(input: &str, mode: XPathMode) -> Result<ParsedXPath, ParseError> {
    let mut arena = AstArena::new();
    let lexer = Lexer::new_with_mode(input, mode);

    let root = xpath_grammar::ExprParser::new()
        .parse(&mut arena, lexer)
        .map_err(map_lalrpop_error)?;
    check_depth(&arena, root)?;

    Ok(ParsedXPath {
        arena,
        root,
        span: SourceSpan::new(0, input.len()),
    })
}

/// Parse an XPath expression with a specific mode and return just the root node ID.
pub fn parse_expr_with_mode(
    input: &str,
    mode: XPathMode,
    arena: &mut AstArena,
) -> Result<AstNodeId, ParseError> {
    let lexer = Lexer::new_with_mode(input, mode);

    let root = xpath_grammar::ExprParser::new()
        .parse(arena, lexer)
        .map_err(map_lalrpop_error)?;
    check_depth(arena, root)?;
    Ok(root)
}

/// Parse an XPath expression with structured options, returning `XPathError` on failure.
///
/// This is the primary entry point for the parser API. It selects the lexer mode
/// based on `opts.mode` and returns `XPathError` (not `ParseError`), making it
/// suitable for use alongside bind and eval phases that also return `XPathError`.
///
/// # Example
///
/// ```
/// use xsd_schema::xpath::parser::parse_with_options;
/// use xsd_schema::xpath::{XPathParseOptions, XPathMode};
///
/// let opts = XPathParseOptions { mode: XPathMode::XPath10 };
/// let result = parse_with_options("/a/b", &opts).unwrap();
/// println!("Parsed {} nodes", result.node_count());
/// ```
pub fn parse_with_options(
    input: &str,
    opts: &XPathParseOptions,
) -> Result<ParsedXPath, XPathError> {
    let parsed = parse_with_mode(input, opts.mode)?; // ParseError → XPathError via From
                                                     // XPath 1.0 validation: the lexer blocks all major 2.0-only constructs by suppressing
                                                     // their tokens. The 3 remaining edge cases (comma sequences, empty parens, double literals)
                                                     // are caught at eval time in eval_node(). No separate AST validator is needed.
    Ok(parsed)
}

/// Parse an XPath expression in XPath 1.0 mode, returning `XPathError` on failure.
///
/// Convenience wrapper around [`parse_with_options`] with `XPathMode::XPath10`.
///
/// # Example
///
/// ```
/// use xsd_schema::xpath::parser::parse_xpath10;
///
/// let result = parse_xpath10("/a/b").unwrap();
/// println!("Parsed {} nodes", result.node_count());
/// ```
pub fn parse_xpath10(input: &str) -> Result<ParsedXPath, XPathError> {
    parse_with_options(
        input,
        &XPathParseOptions {
            mode: XPathMode::XPath10,
        },
    )
}

/// Parse an XPath expression in XPath 2.0 mode, returning `XPathError` on failure.
///
/// Convenience wrapper around [`parse_with_options`] with `XPathMode::XPath20`.
///
/// # Example
///
/// ```
/// use xsd_schema::xpath::parser::parse_xpath20;
///
/// let result = parse_xpath20("for $x in 1 to 10 return $x").unwrap();
/// println!("Parsed {} nodes", result.node_count());
/// ```
pub fn parse_xpath20(input: &str) -> Result<ParsedXPath, XPathError> {
    parse_with_options(
        input,
        &XPathParseOptions {
            mode: XPathMode::XPath20,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xpath::ast::*;

    #[test]
    fn test_parse_arithmetic() {
        let result = parse("1 + 2");
        assert!(result.is_ok(), "Parse failed: {:?}", result.err());
    }

    #[test]
    fn test_parse_path() {
        let result = parse("/a/b/c");
        assert!(result.is_ok(), "Parse failed: {:?}", result.err());
    }

    #[test]
    fn test_parse_variable() {
        let result = parse("$x");
        assert!(result.is_ok(), "Parse failed: {:?}", result.err());
    }

    #[test]
    fn test_parse_function() {
        let result = parse("fn:count(*)");
        assert!(result.is_ok(), "Parse failed: {:?}", result.err());
    }

    #[test]
    fn test_lexer_errors() {
        // Test that lexer errors are properly propagated
        // Most inputs should lex successfully
        let result = parse("'unclosed string");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_xpath10_basic_path() {
        let result = parse_with_mode("/a/b", XPathMode::XPath10);
        assert!(result.is_ok(), "Parse failed: {:?}", result.err());
    }

    #[test]
    fn test_parse_xpath10_keyword_as_element_name() {
        // "union" is a valid element name in XPath 1.0
        let result = parse_with_mode("//union", XPathMode::XPath10);
        assert!(result.is_ok(), "Parse failed: {:?}", result.err());
    }

    #[test]
    fn test_parse_xpath10_unary_minus() {
        // -a|b should parse as -(a|b) in XPath 1.0
        let result = parse_with_mode("-a|b", XPathMode::XPath10);
        assert!(result.is_ok(), "Parse failed: {:?}", result.err());
        let parsed = result.unwrap();
        // Root is Expr wrapping the actual expression
        let root = parsed.root_node();
        let inner_id = match root {
            AstNode::Expr(expr) => {
                assert_eq!(expr.items.len(), 1);
                expr.items[0]
            }
            _ => panic!("Expected Expr root, got {:?}", root),
        };
        // The inner node should be UnaryOp(Negate) wrapping a Union
        let inner = parsed.get_node(inner_id);
        match inner {
            AstNode::UnaryOp(unary) => {
                assert_eq!(unary.kind, UnaryOpKind::Negate);
                // The operand should be a BinaryOp(Union)
                let operand = parsed.get_node(unary.operand);
                match operand {
                    AstNode::BinaryOp(binop) => {
                        assert_eq!(binop.kind, BinaryOpKind::Union);
                    }
                    _ => panic!("Expected Union operand, got {:?}", operand),
                }
            }
            _ => panic!("Expected UnaryOp, got {:?}", inner),
        }
    }

    #[test]
    fn test_parse_xpath20_unary_minus() {
        // -a|b should parse as (-a)|b in XPath 2.0
        let result = parse("-a|b");
        assert!(result.is_ok(), "Parse failed: {:?}", result.err());
        let parsed = result.unwrap();
        // Root is Expr wrapping the actual expression
        let root = parsed.root_node();
        let inner_id = match root {
            AstNode::Expr(expr) => {
                assert_eq!(expr.items.len(), 1);
                expr.items[0]
            }
            _ => panic!("Expected Expr root, got {:?}", root),
        };
        // The inner node should be a BinaryOp(Union) with left = UnaryOp(Negate)
        let inner = parsed.get_node(inner_id);
        match inner {
            AstNode::BinaryOp(binop) => {
                assert_eq!(binop.kind, BinaryOpKind::Union);
                let left = parsed.get_node(binop.left);
                match left {
                    AstNode::UnaryOp(unary) => {
                        assert_eq!(unary.kind, UnaryOpKind::Negate);
                    }
                    _ => panic!("Expected UnaryOp on left of Union, got {:?}", left),
                }
            }
            _ => panic!("Expected BinaryOp(Union), got {:?}", inner),
        }
    }

    #[test]
    fn test_parse_xpath10_unary_plus() {
        // +a|b should parse as +(a|b) in XPath 1.0
        let result = parse_with_mode("+a|b", XPathMode::XPath10);
        assert!(result.is_ok(), "Parse failed: {:?}", result.err());
        let parsed = result.unwrap();
        let root = parsed.root_node();
        let inner_id = match root {
            AstNode::Expr(expr) => {
                assert_eq!(expr.items.len(), 1);
                expr.items[0]
            }
            _ => panic!("Expected Expr root, got {:?}", root),
        };
        let inner = parsed.get_node(inner_id);
        match inner {
            AstNode::UnaryOp(unary) => {
                assert_eq!(unary.kind, UnaryOpKind::Identity);
                let operand = parsed.get_node(unary.operand);
                match operand {
                    AstNode::BinaryOp(binop) => {
                        assert_eq!(binop.kind, BinaryOpKind::Union);
                    }
                    _ => panic!("Expected Union operand, got {:?}", operand),
                }
            }
            _ => panic!("Expected UnaryOp, got {:?}", inner),
        }
    }

    #[test]
    fn test_parse_xpath20_unary_plus() {
        // +a|b should parse as (+a)|b in XPath 2.0
        let result = parse("+a|b");
        assert!(result.is_ok(), "Parse failed: {:?}", result.err());
        let parsed = result.unwrap();
        let root = parsed.root_node();
        let inner_id = match root {
            AstNode::Expr(expr) => {
                assert_eq!(expr.items.len(), 1);
                expr.items[0]
            }
            _ => panic!("Expected Expr root, got {:?}", root),
        };
        let inner = parsed.get_node(inner_id);
        match inner {
            AstNode::BinaryOp(binop) => {
                assert_eq!(binop.kind, BinaryOpKind::Union);
                let left = parsed.get_node(binop.left);
                match left {
                    AstNode::UnaryOp(unary) => {
                        assert_eq!(unary.kind, UnaryOpKind::Identity);
                    }
                    _ => panic!("Expected UnaryOp on left of Union, got {:?}", left),
                }
            }
            _ => panic!("Expected BinaryOp(Union), got {:?}", inner),
        }
    }

    #[test]
    fn test_parse_xpath10_double_unary() {
        // --a|b should parse as -(-(a|b)) in XPath 1.0
        let result = parse_with_mode("--a|b", XPathMode::XPath10);
        assert!(result.is_ok(), "Parse failed: {:?}", result.err());
        let parsed = result.unwrap();
        let root = parsed.root_node();
        let inner_id = match root {
            AstNode::Expr(expr) => {
                assert_eq!(expr.items.len(), 1);
                expr.items[0]
            }
            _ => panic!("Expected Expr root, got {:?}", root),
        };
        // Outer: UnaryOp(Negate)
        let outer = parsed.get_node(inner_id);
        match outer {
            AstNode::UnaryOp(unary_outer) => {
                assert_eq!(unary_outer.kind, UnaryOpKind::Negate);
                // Inner: UnaryOp(Negate)
                let mid = parsed.get_node(unary_outer.operand);
                match mid {
                    AstNode::UnaryOp(unary_inner) => {
                        assert_eq!(unary_inner.kind, UnaryOpKind::Negate);
                        // Innermost: Union
                        let operand = parsed.get_node(unary_inner.operand);
                        match operand {
                            AstNode::BinaryOp(binop) => {
                                assert_eq!(binop.kind, BinaryOpKind::Union);
                            }
                            _ => panic!("Expected Union operand, got {:?}", operand),
                        }
                    }
                    _ => panic!("Expected inner UnaryOp, got {:?}", mid),
                }
            }
            _ => panic!("Expected outer UnaryOp, got {:?}", outer),
        }
    }

    #[test]
    fn test_parse_xpath20_double_unary() {
        // --a|b should parse as (--a)|b in XPath 2.0
        let result = parse("--a|b");
        assert!(result.is_ok(), "Parse failed: {:?}", result.err());
        let parsed = result.unwrap();
        let root = parsed.root_node();
        let inner_id = match root {
            AstNode::Expr(expr) => {
                assert_eq!(expr.items.len(), 1);
                expr.items[0]
            }
            _ => panic!("Expected Expr root, got {:?}", root),
        };
        // Root: BinaryOp(Union)
        let inner = parsed.get_node(inner_id);
        match inner {
            AstNode::BinaryOp(binop) => {
                assert_eq!(binop.kind, BinaryOpKind::Union);
                // Left: UnaryOp(Negate) wrapping UnaryOp(Negate)
                let left = parsed.get_node(binop.left);
                match left {
                    AstNode::UnaryOp(unary_outer) => {
                        assert_eq!(unary_outer.kind, UnaryOpKind::Negate);
                        let inner_unary = parsed.get_node(unary_outer.operand);
                        match inner_unary {
                            AstNode::UnaryOp(unary_inner) => {
                                assert_eq!(unary_inner.kind, UnaryOpKind::Negate);
                            }
                            _ => panic!("Expected inner UnaryOp, got {:?}", inner_unary),
                        }
                    }
                    _ => panic!("Expected UnaryOp on left of Union, got {:?}", left),
                }
            }
            _ => panic!("Expected BinaryOp(Union), got {:?}", inner),
        }
    }

    #[test]
    fn test_parse_xpath10_unary_multi_union() {
        // -a|b|c should parse as -(a|b|c) in XPath 1.0
        // The union chain may be nested as (a|b)|c or a|(b|c)
        let result = parse_with_mode("-a|b|c", XPathMode::XPath10);
        assert!(result.is_ok(), "Parse failed: {:?}", result.err());
        let parsed = result.unwrap();
        let root = parsed.root_node();
        let inner_id = match root {
            AstNode::Expr(expr) => {
                assert_eq!(expr.items.len(), 1);
                expr.items[0]
            }
            _ => panic!("Expected Expr root, got {:?}", root),
        };
        // Root should be UnaryOp(Negate) wrapping a Union
        let inner = parsed.get_node(inner_id);
        match inner {
            AstNode::UnaryOp(unary) => {
                assert_eq!(unary.kind, UnaryOpKind::Negate);
                // The operand should be a Union
                let operand = parsed.get_node(unary.operand);
                match operand {
                    AstNode::BinaryOp(binop) => {
                        assert_eq!(binop.kind, BinaryOpKind::Union);
                    }
                    _ => panic!("Expected Union operand, got {:?}", operand),
                }
            }
            _ => panic!("Expected UnaryOp, got {:?}", inner),
        }
    }

    #[test]
    fn test_parse_xpath20_unary_multi_union() {
        // -a|b|c should parse as (-a)|b|c in XPath 2.0
        let result = parse("-a|b|c");
        assert!(result.is_ok(), "Parse failed: {:?}", result.err());
        let parsed = result.unwrap();
        let root = parsed.root_node();
        let inner_id = match root {
            AstNode::Expr(expr) => {
                assert_eq!(expr.items.len(), 1);
                expr.items[0]
            }
            _ => panic!("Expected Expr root, got {:?}", root),
        };
        // Root should be a Union, with left side containing the unary negate
        let inner = parsed.get_node(inner_id);
        match inner {
            AstNode::BinaryOp(binop) => {
                assert_eq!(binop.kind, BinaryOpKind::Union);
                // Walk left until we find a UnaryOp(Negate) somewhere
                // The structure could be ((- a) | b) | c or (- a) | (b | c)
                fn has_unary_negate(parsed: &ParsedXPath, node_id: AstNodeId) -> bool {
                    match parsed.get_node(node_id) {
                        AstNode::UnaryOp(u) => u.kind == UnaryOpKind::Negate,
                        AstNode::BinaryOp(b) => {
                            has_unary_negate(parsed, b.left) || has_unary_negate(parsed, b.right)
                        }
                        _ => false,
                    }
                }
                assert!(
                    has_unary_negate(&parsed, binop.left),
                    "Expected UnaryOp(Negate) somewhere on the left side of Union"
                );
            }
            _ => panic!("Expected BinaryOp(Union), got {:?}", inner),
        }
    }

    #[test]
    fn test_parse_xpath10_convenience() {
        let result = parse_xpath10("/a/b");
        assert!(result.is_ok(), "parse_xpath10 failed: {:?}", result.err());
    }

    #[test]
    fn test_parse_xpath20_convenience() {
        let result = parse_xpath20("for $x in 1 to 10 return $x");
        assert!(result.is_ok(), "parse_xpath20 failed: {:?}", result.err());
    }

    #[test]
    fn test_parse_with_options_xpath10() {
        let opts = XPathParseOptions {
            mode: XPathMode::XPath10,
        };
        let result = parse_with_options("//union", &opts);
        assert!(
            result.is_ok(),
            "parse_with_options failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_parse_with_options_returns_xpath_error() {
        // Verify that parse_with_options returns XPathError, not ParseError
        let result = parse_with_options("'unclosed string", &XPathParseOptions::default());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.error_code(), Some("XPST0003"));
    }

    #[test]
    fn test_parse_error_to_xpath_error_conversion() {
        // Test From<ParseError> for XPathError
        let parse_err = ParseError::Parser {
            message: "test error".to_string(),
            location: Some(5),
        };
        let xpath_err: crate::xpath::error::XPathError = parse_err.into();
        assert_eq!(xpath_err.error_code(), Some("XPST0003"));
        assert!(xpath_err.to_string().contains("test error"));
    }
}
