// ============================================================================
// Control Flow Nodes
// ============================================================================

use super::{AstNodeId, SourceSpan};

/// Conditional expression (`if (test) then expr else expr`).
#[derive(Debug, Clone)]
pub struct IfNode {
    /// Test/condition expression.
    pub test: AstNodeId,
    /// Then branch expression.
    pub then_branch: AstNodeId,
    /// Else branch expression.
    pub else_branch: AstNodeId,
    /// Source location.
    pub span: SourceSpan,
}

impl IfNode {
    pub fn new(
        test: AstNodeId,
        then_branch: AstNodeId,
        else_branch: AstNodeId,
        span: SourceSpan,
    ) -> Self {
        Self {
            test,
            then_branch,
            else_branch,
            span,
        }
    }
}

/// Variable binding in a for/quantified expression.
#[derive(Debug, Clone)]
pub struct ForBinding {
    /// Variable name prefix.
    pub prefix: String,
    /// Variable local name.
    pub local_name: String,
    /// Expression producing the sequence to iterate.
    pub in_expr: AstNodeId,
    /// Resolved variable slot (set during binding phase).
    pub slot: Option<u32>,
    /// Source location.
    pub span: SourceSpan,
}

impl ForBinding {
    pub fn new(prefix: String, local_name: String, in_expr: AstNodeId, span: SourceSpan) -> Self {
        Self {
            prefix,
            local_name,
            in_expr,
            slot: None,
            span,
        }
    }
}

/// For expression (`for $x in expr return expr`).
///
/// Multiple bindings: `for $x in xs, $y in ys return ...`
#[derive(Debug, Clone)]
pub struct ForNode {
    /// List of variable bindings.
    pub bindings: Vec<ForBinding>,
    /// Return expression.
    pub return_expr: AstNodeId,
    /// Source location.
    pub span: SourceSpan,
}

impl ForNode {
    pub fn new(bindings: Vec<ForBinding>, return_expr: AstNodeId, span: SourceSpan) -> Self {
        Self {
            bindings,
            return_expr,
            span,
        }
    }
}

/// Kind of quantified expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuantifierKind {
    /// `some $x in ... satisfies`
    Some,
    /// `every $x in ... satisfies`
    Every,
}

/// Quantified expression (`some`/`every $x in expr satisfies expr`).
#[derive(Debug, Clone)]
pub struct QuantifiedNode {
    /// Quantifier kind (some or every).
    pub kind: QuantifierKind,
    /// List of variable bindings.
    pub bindings: Vec<ForBinding>,
    /// Satisfies expression.
    pub satisfies: AstNodeId,
    /// Source location.
    pub span: SourceSpan,
}

impl QuantifiedNode {
    pub fn new(
        kind: QuantifierKind,
        bindings: Vec<ForBinding>,
        satisfies: AstNodeId,
        span: SourceSpan,
    ) -> Self {
        Self {
            kind,
            bindings,
            satisfies,
            span,
        }
    }
}
