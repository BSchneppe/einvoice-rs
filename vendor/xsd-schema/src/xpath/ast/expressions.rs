// ============================================================================
// Expression Nodes
// ============================================================================

use super::{AstNodeId, SourceSpan};

/// Comma-separated sequence of expressions.
///
/// In XPath, `(a, b, c)` creates a sequence containing results of a, b, and c.
#[derive(Debug, Clone)]
pub struct ExprNode {
    /// List of expression IDs in the sequence.
    pub items: Vec<AstNodeId>,
    /// Source location.
    pub span: SourceSpan,
}

impl ExprNode {
    /// Create a sequence with a single expression.
    pub fn single(id: AstNodeId, span: SourceSpan) -> Self {
        Self {
            items: vec![id],
            span,
        }
    }

    /// Create a sequence with multiple expressions.
    pub fn sequence(items: Vec<AstNodeId>, span: SourceSpan) -> Self {
        Self { items, span }
    }

    /// Append an expression to the sequence.
    pub fn append(&mut self, id: AstNodeId, end: usize) {
        self.items.push(id);
        self.span.end = end;
    }
}

/// Literal value node.
#[derive(Debug, Clone)]
pub enum ValueNode {
    /// Empty sequence `()`.
    Empty,
    /// String literal.
    String(String),
    /// Boolean literal (used internally, not in XPath syntax).
    Boolean(bool),
    /// Integer literal (stored as string for arbitrary precision).
    Integer(String),
    /// Decimal literal (stored as string for arbitrary precision).
    Decimal(String),
    /// Double literal (stored as string to preserve exact representation).
    Double(String),
    /// Typed value (for runtime computed values).
    Typed(crate::types::value::XmlValue),
}

/// Context item reference (`.`).
#[derive(Debug, Clone)]
pub struct ContextItemNode {
    /// Source location.
    pub span: SourceSpan,
}

impl ContextItemNode {
    pub fn new(span: SourceSpan) -> Self {
        Self { span }
    }
}

/// Variable reference (`$prefix:localname` or `$localname`).
#[derive(Debug, Clone)]
pub struct VarRefNode {
    /// Namespace prefix (empty string if none).
    pub prefix: String,
    /// Local name.
    pub local_name: String,
    /// Resolved variable slot (set during binding phase).
    pub slot: Option<u32>,
    /// Source location.
    pub span: SourceSpan,
}

impl VarRefNode {
    pub fn new(prefix: String, local_name: String, span: SourceSpan) -> Self {
        Self {
            prefix,
            local_name,
            slot: None,
            span,
        }
    }
}
