// ============================================================================
// Type Expressions
// ============================================================================

use super::{AstNodeId, KindTest, QName, SourceSpan};

/// Kind of type expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeExprKind {
    /// `expr instance of type`
    InstanceOf,
    /// `expr treat as type`
    TreatAs,
    /// `expr cast as type`
    CastAs,
    /// `expr castable as type`
    CastableAs,
}

/// Type expression (`instance of`, `treat as`, `cast as`, `castable as`).
///
/// Build one with [`TypeExprNode::new`]: the node also holds a crate-private
/// binding cache, so it has no struct-literal form outside this crate.
#[derive(Debug, Clone)]
pub struct TypeExprNode {
    /// Kind of type expression.
    pub kind: TypeExprKind,
    /// Operand expression.
    pub operand: AstNodeId,
    /// Target type (AST form with raw strings).
    pub target_type: SequenceTypeNode,
    /// Source location.
    pub span: SourceSpan,
    /// Resolved atomic type QName (populated during binding for Atomic item types).
    /// Uses interned NameIds and resolved namespace URIs.
    pub resolved_atomic_type: Option<crate::namespace::qname::QualifiedName>,
    /// The target item type resolved against the static context — names
    /// expanded, type names looked up — for `instance of` and `treat as`.
    /// Populated during binding, so evaluation matches every item against it
    /// without resolving the item type again; `None` before binding (and for
    /// `cast as` / `castable as`, which use `resolved_atomic_type`).
    ///
    /// Crate-private: it is the binder's cache, not part of the node a host
    /// builds or reads. Evaluation resolves `target_type` itself when it is
    /// `None`, so a node built with [`TypeExprNode::new`] is matched correctly
    /// unbound; after editing `target_type` of a bound node, bind it again
    /// (as for `resolved_atomic_type`), which recomputes it.
    pub(crate) resolved_item_type: Option<crate::types::sequence::ItemType>,
    /// The target of a `cast as` / `castable as` resolved against the static
    /// context — a built-in type, or an atomic type of the schema set —
    /// populated during binding (the binder's cache, like
    /// `resolved_item_type`); evaluation resolves `resolved_atomic_type` itself
    /// when it is `None`.
    pub(crate) resolved_cast_target: Option<crate::xpath::schema_cast::CastTarget>,
    /// Whether the binder made this node from a constructor function call
    /// (XPath 2.0 §3.10.4), whose type name is expanded with the default
    /// *function* namespace rather than the default element/type namespace —
    /// so that binding the node again resolves the same type.
    pub(crate) from_constructor: bool,
}

impl TypeExprNode {
    pub fn new(
        kind: TypeExprKind,
        operand: AstNodeId,
        target_type: SequenceTypeNode,
        span: SourceSpan,
    ) -> Self {
        Self {
            kind,
            operand,
            target_type,
            span,
            resolved_atomic_type: None,
            resolved_item_type: None,
            resolved_cast_target: None,
            from_constructor: false,
        }
    }
}

/// Occurrence indicator for sequence types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OccurrenceIndicator {
    /// Exactly one (no indicator).
    #[default]
    One,
    /// Zero or one (`?`).
    ZeroOrOne,
    /// Zero or more (`*`).
    ZeroOrMore,
    /// One or more (`+`).
    OneOrMore,
}

/// Sequence type (`item-type occurrence-indicator?`).
#[derive(Debug, Clone)]
pub struct SequenceTypeNode {
    /// Item type (None = `empty-sequence()`).
    pub item_type: Option<ItemTypeNode>,
    /// Occurrence indicator.
    pub occurrence: OccurrenceIndicator,
    /// Source location.
    pub span: SourceSpan,
}

impl SequenceTypeNode {
    /// `empty-sequence()`
    pub fn empty(span: SourceSpan) -> Self {
        Self {
            item_type: None,
            occurrence: OccurrenceIndicator::One,
            span,
        }
    }

    /// Single item type with optional occurrence.
    pub fn single(
        item_type: ItemTypeNode,
        occurrence: OccurrenceIndicator,
        span: SourceSpan,
    ) -> Self {
        Self {
            item_type: Some(item_type),
            occurrence,
            span,
        }
    }
}

/// Item type in a sequence type.
#[derive(Debug, Clone)]
pub enum ItemTypeNode {
    /// `item()` - any item.
    Item,
    /// Atomic type (QName like `xs:integer`).
    Atomic(QName),
    /// Kind test (`node()`, `element()`, etc.).
    Kind(KindTest),
}

/// The indicator as the grammar spells it: nothing, `?`, `*` or `+`.
impl std::fmt::Display for OccurrenceIndicator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            OccurrenceIndicator::One => "",
            OccurrenceIndicator::ZeroOrOne => "?",
            OccurrenceIndicator::ZeroOrMore => "*",
            OccurrenceIndicator::OneOrMore => "+",
        })
    }
}

/// The item type as the expression spells it: `item()`, a QName, or a kind
/// test.
impl std::fmt::Display for ItemTypeNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ItemTypeNode::Item => f.write_str("item()"),
            ItemTypeNode::Atomic(qname) => write!(f, "{qname}"),
            ItemTypeNode::Kind(kind) => write!(f, "{kind}"),
        }
    }
}

/// The sequence type as the expression spells it, e.g. `xs:integer?` or
/// `empty-sequence()` (XPath 2.0 §2.5.3).
impl std::fmt::Display for SequenceTypeNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.item_type {
            None => f.write_str("empty-sequence()"),
            Some(item_type) => write!(f, "{item_type}{}", self.occurrence),
        }
    }
}
