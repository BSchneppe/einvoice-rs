// ============================================================================
// Core AST Node Enum
// ============================================================================

use super::{
    BinaryOpNode, ContextItemNode, ExprNode, FilterExprNode, ForNode, FunctionCallNode, IfNode,
    PathExprNode, PathStepNode, QuantifiedNode, RangeNode, TypeExprNode, UnaryOpNode, ValueNode,
    VarRefNode,
};

/// The main AST node type, encompassing all possible expression types.
#[derive(Debug, Clone)]
pub enum AstNode {
    /// Comma-separated sequence of expressions.
    Expr(ExprNode),
    /// Literal value (string, number, boolean, empty).
    Value(ValueNode),
    /// Context item reference (`.`).
    ContextItem(ContextItemNode),
    /// Variable reference (`$name`).
    VarRef(VarRefNode),
    /// Conditional expression (`if ... then ... else`).
    If(IfNode),
    /// For expression (`for $x in ... return`).
    For(ForNode),
    /// Quantified expression (`some`/`every ... satisfies`).
    Quantified(QuantifiedNode),
    /// Function call (`name(...)`).
    FunctionCall(FunctionCallNode),
    /// Path expression (absolute or relative).
    PathExpr(PathExprNode),
    /// Filter expression (`expr[predicate]`).
    FilterExpr(FilterExprNode),
    /// Range expression (`expr to expr`).
    Range(RangeNode),
    /// Unary operator (`+` or `-`).
    UnaryOp(UnaryOpNode),
    /// Binary operator (arithmetic, comparison, logical, etc.).
    BinaryOp(BinaryOpNode),
    /// Single path step (axis + node test + predicates).
    PathStep(PathStepNode),
    /// Type expression (instance of, treat as, cast as, castable as).
    TypeExpr(TypeExprNode),
}
