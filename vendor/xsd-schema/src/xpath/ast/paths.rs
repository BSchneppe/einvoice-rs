// ============================================================================
// Path Expressions
// ============================================================================

use super::{AstNodeId, SourceSpan};

/// XPath axis specifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// `child::` (default for element names)
    Child,
    /// `descendant::`
    Descendant,
    /// `attribute::` (abbreviated `@`)
    Attribute,
    /// `self::`
    SelfAxis,
    /// `descendant-or-self::`
    DescendantOrSelf,
    /// `following-sibling::`
    FollowingSibling,
    /// `following::`
    Following,
    /// `parent::` (abbreviated `..`)
    Parent,
    /// `ancestor::`
    Ancestor,
    /// `preceding-sibling::`
    PrecedingSibling,
    /// `preceding::`
    Preceding,
    /// `ancestor-or-self::`
    AncestorOrSelf,
    /// `namespace::`
    Namespace,
}

impl Axis {
    /// Check if this is a reverse axis (traverses in reverse document order).
    pub fn is_reverse(&self) -> bool {
        matches!(
            self,
            Axis::Parent
                | Axis::Ancestor
                | Axis::PrecedingSibling
                | Axis::Preceding
                | Axis::AncestorOrSelf
        )
    }

    /// Check if this is a forward axis.
    pub fn is_forward(&self) -> bool {
        !self.is_reverse()
    }
}

/// Node test in a path step.
#[derive(Debug, Clone)]
pub enum NodeTest {
    /// Name test (QName with optional wildcards).
    Name(NameTest),
    /// Kind test (`node()`, `element()`, etc.).
    Kind(KindTest),
}

/// Name test with optional wildcards.
#[derive(Debug, Clone)]
pub struct NameTest {
    /// Namespace prefix (None = wildcard `*:local`, Some("") = no prefix).
    pub prefix: Option<String>,
    /// Local name (None = wildcard `prefix:*` or `*`).
    pub local_name: Option<String>,
}

impl NameTest {
    /// Match any node: `*`.
    pub fn any() -> Self {
        Self {
            prefix: None,
            local_name: None,
        }
    }

    /// Match any local name in a namespace: `prefix:*`.
    pub fn any_in_ns(prefix: String) -> Self {
        Self {
            prefix: Some(prefix),
            local_name: None,
        }
    }

    /// Match any namespace with a specific local name: `*:local`.
    pub fn any_ns(local_name: String) -> Self {
        Self {
            prefix: None,
            local_name: Some(local_name),
        }
    }

    /// Match a specific QName.
    pub fn qname(prefix: String, local_name: String) -> Self {
        Self {
            prefix: Some(prefix),
            local_name: Some(local_name),
        }
    }
}

/// Kind test (`node()`, `text()`, `element()`, etc.).
#[derive(Debug, Clone)]
pub enum KindTest {
    /// `node()` - matches any node.
    AnyKind,
    /// `text()` - matches text nodes.
    Text,
    /// `comment()` - matches comment nodes.
    Comment,
    /// `processing-instruction()` or `processing-instruction('name')`.
    ProcessingInstruction(Option<String>),
    /// `document-node()` or `document-node(element(...))`.
    Document(Option<Box<KindTest>>),
    /// `element()` or `element(name)` or `element(name, type)`.
    Element(ElementTest),
    /// `attribute()` or `attribute(name)` or `attribute(name, type)`.
    Attribute(AttributeTest),
    /// `schema-element(name)`.
    SchemaElement(String),
    /// `schema-attribute(name)`.
    SchemaAttribute(String),
}

/// Element test: `element()`, `element(name)`, or `element(name, type)`.
#[derive(Debug, Clone, Default)]
pub struct ElementTest {
    /// Element name (None = wildcard).
    pub name: Option<QName>,
    /// Type annotation (None = any type).
    pub type_name: Option<QName>,
    /// Whether the type allows nilled elements.
    pub nillable: bool,
}

/// Attribute test: `attribute()`, `attribute(name)`, or `attribute(name, type)`.
#[derive(Debug, Clone, Default)]
pub struct AttributeTest {
    /// Attribute name (None = wildcard).
    pub name: Option<QName>,
    /// Type annotation (None = any type).
    pub type_name: Option<QName>,
}

/// Qualified name (prefix:local or just local).
#[derive(Debug, Clone)]
pub struct QName {
    /// Namespace prefix (empty string if none).
    pub prefix: String,
    /// Local part.
    pub local: String,
}

impl QName {
    pub fn new(prefix: String, local: String) -> Self {
        Self { prefix, local }
    }

    pub fn local_only(local: String) -> Self {
        Self {
            prefix: String::new(),
            local,
        }
    }
}

/// The lexical form: `prefix:local`, or `local` with no prefix.
impl std::fmt::Display for QName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.prefix.is_empty() {
            f.write_str(&self.local)
        } else {
            write!(f, "{}:{}", self.prefix, self.local)
        }
    }
}

/// The kind test as the XPath 2.0 grammar spells it (§2.5.3), e.g.
/// `element(p:a, T?)` or `document-node(schema-element(x))`.
impl std::fmt::Display for KindTest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KindTest::AnyKind => f.write_str("node()"),
            KindTest::Text => f.write_str("text()"),
            KindTest::Comment => f.write_str("comment()"),
            KindTest::ProcessingInstruction(None) => f.write_str("processing-instruction()"),
            KindTest::ProcessingInstruction(Some(target)) => {
                write!(f, "processing-instruction({target})")
            }
            KindTest::Document(None) => f.write_str("document-node()"),
            KindTest::Document(Some(inner)) => write!(f, "document-node({inner})"),
            KindTest::Element(test) => {
                f.write_str("element(")?;
                match (&test.name, &test.type_name) {
                    (None, None) => {}
                    (name, type_name) => {
                        match name {
                            Some(name) => write!(f, "{name}")?,
                            None => f.write_str("*")?,
                        }
                        if let Some(type_name) = type_name {
                            write!(f, ", {type_name}")?;
                            if test.nillable {
                                f.write_str("?")?;
                            }
                        }
                    }
                }
                f.write_str(")")
            }
            KindTest::Attribute(test) => {
                f.write_str("attribute(")?;
                match (&test.name, &test.type_name) {
                    (None, None) => {}
                    (name, type_name) => {
                        match name {
                            Some(name) => write!(f, "{name}")?,
                            None => f.write_str("*")?,
                        }
                        if let Some(type_name) = type_name {
                            write!(f, ", {type_name}")?;
                        }
                    }
                }
                f.write_str(")")
            }
            KindTest::SchemaElement(name) => write!(f, "schema-element({name})"),
            KindTest::SchemaAttribute(name) => write!(f, "schema-attribute({name})"),
        }
    }
}

/// Single step in a path expression.
#[derive(Debug, Clone)]
pub struct PathStepNode {
    /// Axis specifier.
    pub axis: Axis,
    /// Node test (AST form with raw strings).
    pub test: NodeTest,
    /// Predicates (expression IDs).
    pub predicates: Vec<AstNodeId>,
    /// Source location.
    pub span: SourceSpan,
    /// Resolved name test (populated during binding).
    /// Uses interned NameIds and resolved namespace URIs.
    pub resolved_test: Option<crate::types::NameTest>,
}

impl PathStepNode {
    pub fn new(axis: Axis, test: NodeTest, span: SourceSpan) -> Self {
        Self {
            axis,
            test,
            predicates: Vec::new(),
            span,
            resolved_test: None,
        }
    }

    pub fn with_predicates(
        axis: Axis,
        test: NodeTest,
        predicates: Vec<AstNodeId>,
        span: SourceSpan,
    ) -> Self {
        Self {
            axis,
            test,
            predicates,
            span,
            resolved_test: None,
        }
    }

    /// Abbreviated forward step written as a bare `NodeTest` (no axis name).
    ///
    /// XPath 2.0 §3.2.4: "If the axis name is omitted from an axis step, the
    /// default axis is `child` unless the axis step contains an
    /// AttributeTest or SchemaAttributeTest; in that case, the default axis
    /// is `attribute`." So `section/attribute(id)` abbreviates
    /// `child::section/attribute::attribute(id)`, while `section/para`
    /// abbreviates `child::section/child::para`.
    pub fn abbrev_forward(test: NodeTest, span: SourceSpan) -> Self {
        Self::new(default_forward_axis(&test), test, span)
    }

    /// Abbreviated parent step (`..`).
    pub fn abbrev_parent(span: SourceSpan) -> Self {
        Self {
            axis: Axis::Parent,
            test: NodeTest::Kind(KindTest::AnyKind),
            predicates: Vec::new(),
            span,
            resolved_test: None,
        }
    }
}

/// The axis an abbreviated forward step (`NodeTest` with no axis name) runs on.
///
/// XPath 2.0 §3.2.4: `child` unless the node test is an AttributeTest or a
/// SchemaAttributeTest, in which case it is `attribute`.
pub fn default_forward_axis(test: &NodeTest) -> Axis {
    match test {
        NodeTest::Kind(KindTest::Attribute(_)) | NodeTest::Kind(KindTest::SchemaAttribute(_)) => {
            Axis::Attribute
        }
        _ => Axis::Child,
    }
}

/// Path expression (sequence of steps).
#[derive(Debug, Clone)]
pub struct PathExprNode {
    /// Whether the path starts from root (`/`).
    pub is_absolute: bool,
    /// Steps in the path (IDs of PathStep nodes or filter expressions).
    pub steps: Vec<AstNodeId>,
    /// Source location.
    pub span: SourceSpan,
    /// Hint that result order doesn't matter (optimization).
    pub unordered_hint: bool,
}

impl PathExprNode {
    /// Root-only path (`/`).
    pub fn root_only(span: SourceSpan) -> Self {
        Self {
            is_absolute: true,
            steps: Vec::new(),
            span,
            unordered_hint: false,
        }
    }

    /// Absolute path (`/a/b`).
    pub fn absolute(steps: Vec<AstNodeId>, span: SourceSpan) -> Self {
        Self {
            is_absolute: true,
            steps,
            span,
            unordered_hint: false,
        }
    }

    /// Relative path (`a/b`).
    pub fn relative(steps: Vec<AstNodeId>, span: SourceSpan) -> Self {
        Self {
            is_absolute: false,
            steps,
            span,
            unordered_hint: false,
        }
    }
}

/// Filter expression (`primary[predicate][predicate]...`).
#[derive(Debug, Clone)]
pub struct FilterExprNode {
    /// Base/primary expression.
    pub base: AstNodeId,
    /// Predicate expressions.
    pub predicates: Vec<AstNodeId>,
    /// Source location.
    pub span: SourceSpan,
}

impl FilterExprNode {
    pub fn new(base: AstNodeId, predicates: Vec<AstNodeId>, span: SourceSpan) -> Self {
        Self {
            base,
            predicates,
            span,
        }
    }
}
