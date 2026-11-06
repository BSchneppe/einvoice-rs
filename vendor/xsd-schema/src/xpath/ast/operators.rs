// ============================================================================
// Operators
// ============================================================================

use super::{AstNodeId, SourceSpan};

/// Operator flags for optimization hints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OpFlags {
    pub atomize: bool,
    pub singleton: bool,
    pub ordered: bool,
}

impl OpFlags {
    pub fn atomized() -> Self {
        Self {
            atomize: true,
            ..Default::default()
        }
    }
    pub fn singleton() -> Self {
        Self {
            singleton: true,
            ..Default::default()
        }
    }
    pub fn ordered() -> Self {
        Self {
            ordered: true,
            ..Default::default()
        }
    }
}

/// Range expression (`expr to expr`).
#[derive(Debug, Clone)]
pub struct RangeNode {
    /// Start of range.
    pub start: AstNodeId,
    /// End of range.
    pub end: AstNodeId,
    /// Source location.
    pub span: SourceSpan,
}

impl RangeNode {
    pub fn new(start: AstNodeId, end: AstNodeId, span: SourceSpan) -> Self {
        Self { start, end, span }
    }
}

/// Unary operator kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOpKind {
    /// Unary minus (`-expr`).
    Negate,
    /// Unary plus / identity (`+expr`).
    Identity,
}

/// Unary operator expression.
#[derive(Debug, Clone)]
pub struct UnaryOpNode {
    /// Operator kind.
    pub kind: UnaryOpKind,
    /// Operand expression.
    pub operand: AstNodeId,
    /// Source location.
    pub span: SourceSpan,
}

impl UnaryOpNode {
    pub fn new(kind: UnaryOpKind, operand: AstNodeId, span: SourceSpan) -> Self {
        Self {
            kind,
            operand,
            span,
        }
    }
}

/// Binary operator kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOpKind {
    // Logical
    /// `or`
    Or,
    /// `and`
    And,

    // General comparisons
    /// `=`
    GeneralEq,
    /// `!=`
    GeneralNe,
    /// `<`
    GeneralLt,
    /// `<=`
    GeneralLe,
    /// `>`
    GeneralGt,
    /// `>=`
    GeneralGe,

    // Value comparisons
    /// `eq`
    ValueEq,
    /// `ne`
    ValueNe,
    /// `lt`
    ValueLt,
    /// `le`
    ValueLe,
    /// `gt`
    ValueGt,
    /// `ge`
    ValueGe,

    // Node comparisons
    /// `is`
    Is,
    /// `<<`
    Before,
    /// `>>`
    After,

    // Arithmetic
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*`
    Mul,
    /// `div`
    Div,
    /// `idiv`
    IDiv,
    /// `mod`
    Mod,

    // Sequence operations
    /// `union` or `|`
    Union,
    /// `intersect`
    Intersect,
    /// `except`
    Except,
}

/// Binary operator expression.
#[derive(Debug, Clone)]
pub struct BinaryOpNode {
    /// Operator kind.
    pub kind: BinaryOpKind,
    /// Left operand.
    pub left: AstNodeId,
    /// Right operand.
    pub right: AstNodeId,
    /// Source location.
    pub span: SourceSpan,
    /// Operator flags for optimization hints.
    pub flags: OpFlags,
}

impl BinaryOpNode {
    pub fn new(kind: BinaryOpKind, left: AstNodeId, right: AstNodeId, span: SourceSpan) -> Self {
        Self {
            kind,
            left,
            right,
            span,
            flags: OpFlags::default(),
        }
    }
}
