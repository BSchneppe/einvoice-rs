//! The nesting depth of a parsed XPath expression, and its limit.
//!
//! Binding, static analysis and evaluation walk the expression tree
//! recursively, so an expression nested deeply enough — parentheses inside
//! parentheses, predicates inside predicates, a long chain of `or` or `+`
//! (each operator nests the expression before it), `if` inside `else`,
//! unary `-` before unary `-` — would overflow the stack. Every parse
//! function of [`crate::xpath::parser`] (and so [`XPathExpr::compile`]
//! and everything built on it) refuses an expression deeper than
//! [`MAX_EXPRESSION_DEPTH`] right after it is parsed, before any of those
//! walks, with [`ParseError::LimitExceeded`] /
//! [`XPathError::LimitExceeded`]. The parse itself does not recurse (an LR
//! parser with an explicit stack, an arena of nodes), and neither does this
//! measurement. A tree a host builds into an [`AstArena`] itself, not
//! through the parser, is not checked; [`expression_depth`] measures it.
//!
//! The limit protects any use of the engine, not only schemas, and is sized
//! for optimized builds: at the limit, binding and evaluation take well
//! under 1 MiB of stack in a release build (the frames of the recursive
//! evaluator are under 2 KiB a level), so a 2 MiB thread is enough. An
//! unoptimized (debug) build's frames are about twenty times larger (about
//! 40 KiB a level), so a debug build evaluating expressions near the limit
//! needs a larger thread stack.
//!
//! XPath 2.0 §2.3.1: "an implementation may raise a dynamic error for a
//! reason beyond the scope of this specification. For example, limitations
//! may exist on the maximum numbers or sizes of various objects. Any such
//! limitations, and the consequences of exceeding them, are
//! implementation-dependent."
//!
//! [`XPathExpr::compile`]: crate::xpath::api::XPathExpr::compile
//! [`ParseError::LimitExceeded`]: crate::xpath::parser::ParseError::LimitExceeded
//! [`XPathError::LimitExceeded`]: crate::xpath::error::XPathError::LimitExceeded

use crate::error::{SchemaError, SchemaLimit, SchemaResult};
use crate::xpath::arena::{AstArena, AstNodeId, NO_NODE};
use crate::xpath::ast::AstNode;
use crate::xpath::parser::ParseError;

/// The deepest XPath expression the parser accepts: the longest chain of
/// expressions nested in each other in its tree (a parenthesized
/// expression, a predicate, a function argument, an operand, a branch, a
/// `for` / `some` / `every` clause: one level each; the steps of a path and
/// the items of a sequence are side by side, not nested).
///
/// Measured, the deepest expression of the XQuery test suite's XPath
/// queries nests 14 levels, of the W3C XSD test suite's assertions and type
/// alternatives 14, of a 65,000-expression corpus of embedded XPath 24: the
/// limit leaves a margin of ten times the deepest expression seen.
pub const MAX_EXPRESSION_DEPTH: usize = 256;

/// The children of a node, in no particular order.
fn children(node: &AstNode, out: &mut Vec<AstNodeId>) {
    match node {
        AstNode::Expr(n) => out.extend(n.items.iter().copied()),
        AstNode::Value(_) | AstNode::ContextItem(_) | AstNode::VarRef(_) => {}
        AstNode::If(n) => out.extend([n.test, n.then_branch, n.else_branch]),
        AstNode::For(n) => {
            out.extend(n.bindings.iter().map(|b| b.in_expr));
            out.push(n.return_expr);
        }
        AstNode::Quantified(n) => {
            out.extend(n.bindings.iter().map(|b| b.in_expr));
            out.push(n.satisfies);
        }
        AstNode::FunctionCall(n) => out.extend(n.args.iter().copied()),
        AstNode::PathExpr(n) => out.extend(n.steps.iter().copied()),
        AstNode::FilterExpr(n) => {
            out.push(n.base);
            out.extend(n.predicates.iter().copied());
        }
        AstNode::Range(n) => out.extend([n.start, n.end]),
        AstNode::UnaryOp(n) => out.push(n.operand),
        AstNode::BinaryOp(n) => out.extend([n.left, n.right]),
        AstNode::PathStep(n) => out.extend(n.predicates.iter().copied()),
        AstNode::TypeExpr(n) => out.push(n.operand),
    }
}

/// The nesting depth of the expression tree rooted at `root` (a leaf is 1),
/// measured without recursion, as [`MAX_EXPRESSION_DEPTH`] counts it. A
/// child that is out of range (a sentinel) is skipped.
pub fn expression_depth(arena: &AstArena, root: AstNodeId) -> usize {
    let len = arena.len();
    if root == NO_NODE || root as usize >= len {
        return 0;
    }
    // Iterative post-order: (node, its depth so far, whether its children
    // were pushed). Arena trees share no nodes, so no memo is needed.
    let mut best = 0usize;
    let mut stack: Vec<(AstNodeId, usize)> = vec![(root, 1)];
    let mut kids = Vec::new();
    while let Some((id, depth)) = stack.pop() {
        best = best.max(depth);
        kids.clear();
        children(arena.get(id), &mut kids);
        for &kid in &kids {
            if kid != NO_NODE && (kid as usize) < len {
                stack.push((kid, depth + 1));
            }
        }
    }
    best
}

/// Refuse the `test` of a schema's `xs:assert`, `xs:assertion` or
/// `xs:alternative` (`element`) that nests deeper than
/// [`MAX_EXPRESSION_DEPTH`], when the schema document is read: an assertion
/// is otherwise compiled only when an instance is validated. Other parse
/// errors are left to the checks that report them.
pub(crate) fn check_schema_test(test: &str, element: &str) -> SchemaResult<()> {
    match crate::xpath::parser::parse(test) {
        Err(ParseError::LimitExceeded { max, found }) => Err(SchemaError::limit(
            SchemaLimit::XPathDepth,
            max,
            found,
            format!("XPath nesting depth {found} in the test of <xs:{element}>"),
            None,
        )),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xpath::parser::parse;

    fn depth(expr: &str) -> usize {
        let parsed = parse(expr).unwrap_or_else(|e| panic!("{expr}: {e}"));
        expression_depth(&parsed.arena, parsed.root)
    }

    #[test]
    fn depth_counts_nesting_not_length() {
        let flat = depth("1, 2, 3, 4");
        assert_eq!(depth("(1, 2, 3, 4, 5, 6, 7, 8)"), depth("(1, 2)"));
        assert_eq!(depth("a/b/c/d/e/f"), depth("a/b"));
        assert!(depth("((((1))))") > depth("(1)"));
        assert!(depth("1 + 2 + 3 + 4") > depth("1 + 2"));
        assert!(depth("a[b[c]]") > depth("a[b]"));
        assert!(flat >= 1);
    }
}
