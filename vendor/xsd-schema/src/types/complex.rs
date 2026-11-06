//! Complex type definitions
//!
//! This module implements XSD complex type definitions with content models,
//! attributes, and derivation mechanisms.

use crate::ids::{
    AttributeGroupKey, AttributeKey, ComplexTypeKey, ElementKey, ModelGroupKey, NameId,
    SimpleTypeKey, TypeKey,
};
use crate::parser::location::SourceRef;
use crate::schema::model::DerivationSet;

/// Complex type content kind
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ContentKind {
    /// Empty content (no child elements or text)
    #[default]
    Empty,
    /// Simple content (text only, possibly with attributes)
    Simple,
    /// Element-only content (child elements, no mixed text)
    ElementOnly,
    /// Mixed content (child elements with interleaved text)
    Mixed,
}

/// Derivation method for complex types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DerivationMethod {
    /// Derived by restriction (constraining base type)
    Restriction,
    /// Derived by extension (adding to base type)
    Extension,
}

/// Reference to a type (simple or complex)
#[derive(Debug, Clone)]
pub enum TypeRef {
    /// Reference to a simple type
    Simple(SimpleTypeRef),
    /// Reference to a complex type
    Complex(ComplexTypeRef),
    /// Unresolved reference (QName)
    Unresolved {
        namespace: Option<NameId>,
        local_name: NameId,
    },
    /// Built-in anyType
    AnyType,
}

/// Reference to a simple type
#[derive(Debug, Clone)]
pub enum SimpleTypeRef {
    /// Resolved reference
    Resolved(SimpleTypeKey),
    /// Built-in type
    BuiltIn(super::simple::BuiltInType),
}

/// Reference to a complex type
#[derive(Debug, Clone)]
pub enum ComplexTypeRef {
    /// Resolved reference
    Resolved(ComplexTypeKey),
    /// Built-in anyType (the ur-type)
    AnyType,
}

/// Content model for complex types
#[derive(Debug, Clone, Default)]
pub enum ComplexTypeContent {
    /// Empty content (no elements or text)
    #[default]
    Empty,

    /// Simple content (text value with attributes)
    Simple(SimpleContentDef),

    /// Complex content (elements with optional mixed text)
    Complex(Box<ComplexContentDef>),
}

/// Simple content definition (text value with attributes)
#[derive(Debug, Clone)]
pub struct SimpleContentDef {
    /// Base type for the text content
    pub base_type: SimpleTypeRef,

    /// Derivation method
    pub derivation: DerivationMethod,

    /// Source location
    pub source: Option<SourceRef>,
}

/// Complex content definition (elements and optionally mixed text)
#[derive(Debug, Clone)]
pub struct ComplexContentDef {
    /// Content model particle (sequence, choice, all, group ref)
    pub particle: Option<ContentParticle>,

    /// Derivation method
    pub derivation: DerivationMethod,

    /// Whether mixed content is allowed
    pub mixed: bool,

    /// Source location
    pub source: Option<SourceRef>,

    /// XSD 1.1: Open content (runtime matching implemented; schema-level validation pending)
    pub open_content: Option<OpenContent>,
}

/// Content particle (term with occurrence constraints)
#[derive(Debug, Clone)]
pub struct ContentParticle {
    /// The term (element, group, wildcard)
    pub term: ContentTerm,

    /// Minimum occurrences (default 1)
    pub min_occurs: u32,

    /// Maximum occurrences (None = unbounded)
    pub max_occurs: Option<u32>,

    /// Source location
    pub source: Option<SourceRef>,
}

impl ContentParticle {
    /// Create a particle with default occurrence (1..1)
    pub fn new(term: ContentTerm) -> Self {
        Self {
            term,
            min_occurs: 1,
            max_occurs: Some(1),
            source: None,
        }
    }

    /// Create a particle with custom occurrence
    pub fn with_occurs(term: ContentTerm, min: u32, max: Option<u32>) -> Self {
        Self {
            term,
            min_occurs: min,
            max_occurs: max,
            source: None,
        }
    }

    /// Check if this particle is optional (minOccurs=0)
    pub fn is_optional(&self) -> bool {
        self.min_occurs == 0
    }

    /// Check if this particle allows multiple occurrences
    pub fn is_repeating(&self) -> bool {
        self.max_occurs.is_none_or(|max| max > 1)
    }

    /// Check if this particle is unbounded
    pub fn is_unbounded(&self) -> bool {
        self.max_occurs.is_none()
    }
}

/// Content model term (element, group, or wildcard)
#[derive(Debug, Clone)]
pub enum ContentTerm {
    /// Reference to an element
    Element(ElementRef),

    /// Model group (sequence, choice, all)
    Group(ModelGroupDef),

    /// Reference to a named model group
    GroupRef(ModelGroupKey),

    /// Wildcard (any element)
    Wildcard(WildcardRef),
}

/// Reference to an element
#[derive(Debug, Clone)]
pub enum ElementRef {
    /// Local element declaration
    Local(ElementKey),
    /// Reference to a global element
    Global {
        namespace: Option<NameId>,
        local_name: NameId,
        resolved: Option<ElementKey>,
    },
}

/// Inline model group definition
#[derive(Debug, Clone)]
pub struct ModelGroupDef {
    /// Compositor type
    pub compositor: Compositor,

    /// Child particles
    pub particles: Vec<ContentParticle>,

    /// Source location
    pub source: Option<SourceRef>,
}

/// Model group compositor
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compositor {
    /// Sequence: all children in order
    Sequence,
    /// Choice: exactly one child
    Choice,
    /// All: all children in any order (XSD 1.0: top-level only, each once)
    All,
}

/// Reference to a wildcard
#[derive(Debug, Clone)]
pub struct WildcardRef {
    /// Namespace constraint
    pub namespace_constraint: NamespaceConstraint,

    /// Process contents mode
    pub process_contents: ProcessContents,

    /// Pre-expanded concrete QName exclusions (XSD 1.1 notQName)
    pub not_qnames: Vec<(Option<NameId>, NameId)>,

    /// XSD 1.1: notQName="##definedSibling" was specified but deferred
    /// because sibling context was not yet available (open content wildcards).
    /// Resolved later when attached to a content model.
    pub has_defined_sibling: bool,

    /// Source location
    pub source: Option<SourceRef>,
}

/// Namespace constraint for wildcards
#[derive(Debug, Clone, Default)]
pub enum NamespaceConstraint {
    /// Any namespace (##any)
    #[default]
    Any,
    /// Other namespaces (##other)
    Other,
    /// Target namespace (##targetNamespace)
    TargetNamespace,
    /// Local elements only (##local)
    Local,
    /// Specific namespaces
    List(Vec<Option<NameId>>),
    /// XSD 1.1: Not these namespaces (notNamespace)
    Not(Vec<Option<NameId>>),
}

impl NamespaceConstraint {
    /// Check whether an element namespace matches this wildcard constraint.
    /// No constraint kind depends on the XSD version (`##other` is the same in
    /// both, [`other_matches_namespace`]).
    pub fn matches(
        &self,
        element_namespace: Option<NameId>,
        target_namespace: Option<NameId>,
    ) -> bool {
        match self {
            NamespaceConstraint::Any => true,
            NamespaceConstraint::Other => {
                other_matches_namespace(element_namespace, target_namespace)
            }
            NamespaceConstraint::TargetNamespace => element_namespace == target_namespace,
            NamespaceConstraint::Local => element_namespace.is_none(),
            NamespaceConstraint::List(list) => list.contains(&element_namespace),
            NamespaceConstraint::Not(excluded) => !excluded.contains(&element_namespace),
        }
    }
}

/// `##other` namespace predicate (§3.10.4, XSD 1.0 and 1.1 alike).
///
/// `##other` excludes the target namespace and the absent namespace. When the
/// schema has no target namespace, it still excludes absent — only non-absent
/// namespaces match.
///
pub fn other_matches_namespace(
    element_namespace: Option<NameId>,
    target_namespace: Option<NameId>,
) -> bool {
    element_namespace.is_some() && element_namespace != target_namespace
}

/// Check whether a (namespace, name) pair is excluded by a notQName list.
pub fn not_qnames_exclude(
    not_qnames: &[(Option<NameId>, NameId)],
    namespace: Option<NameId>,
    name: NameId,
) -> bool {
    not_qnames
        .iter()
        .any(|&(ns, n)| ns == namespace && n == name)
}

// Re-export ProcessContents from schema::wildcard to avoid duplication.
pub use crate::schema::wildcard::ProcessContents;

/// XSD 1.1: Open content specification
///
/// Runtime matching (interleave + suffix modes) is implemented in `validation/content.rs`.
/// Schema-level validation stubs remain in `compiler/open_content.rs`.
#[derive(Debug, Clone)]
pub struct OpenContent {
    /// Open content mode
    pub mode: OpenContentMode,

    /// Wildcard for open content
    pub wildcard: Option<WildcardRef>,

    /// Source location
    pub source: Option<SourceRef>,
}

/// XSD 1.1: Open content mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OpenContentMode {
    /// No open content
    None,
    /// Open content can appear interleaved
    #[default]
    Interleave,
    /// Open content can appear at the end
    Suffix,
}

impl From<crate::parser::frames::OpenContentMode> for OpenContentMode {
    fn from(m: crate::parser::frames::OpenContentMode) -> Self {
        use crate::parser::frames::OpenContentMode as Src;
        match m {
            Src::None => Self::None,
            Src::Interleave => Self::Interleave,
            Src::Suffix => Self::Suffix,
        }
    }
}

impl From<crate::schema::model::OpenContentMode> for OpenContentMode {
    fn from(m: crate::schema::model::OpenContentMode) -> Self {
        use crate::schema::model::OpenContentMode as Src;
        match m {
            Src::None => Self::None,
            Src::Interleave => Self::Interleave,
            Src::Suffix => Self::Suffix,
        }
    }
}

/// Attribute use (attribute declaration with use constraints)
#[derive(Debug, Clone)]
pub struct AttributeUse {
    /// The attribute declaration
    pub attribute: AttributeRef,

    /// Use requirement
    pub use_kind: AttributeUseKind,

    /// Default value (mutually exclusive with fixed)
    pub default_value: Option<String>,

    /// Fixed value (mutually exclusive with default)
    pub fixed_value: Option<String>,

    /// Inheritable (XSD 1.1 §3.2.6, §3.3.5.6)
    pub inheritable: bool,

    /// Source location
    pub source: Option<SourceRef>,
}

/// Attribute reference
#[derive(Debug, Clone)]
pub enum AttributeRef {
    /// Local attribute declaration
    Local(AttributeKey),
    /// Reference to a global attribute
    Global {
        namespace: Option<NameId>,
        local_name: NameId,
        resolved: Option<AttributeKey>,
    },
}

/// Attribute use requirement
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AttributeUseKind {
    /// Attribute is required
    Required,
    /// Attribute is optional (default)
    #[default]
    Optional,
    /// Attribute is prohibited
    Prohibited,
}

/// Attribute wildcard (anyAttribute)
#[derive(Debug, Clone)]
pub struct AttributeWildcard {
    /// Namespace constraint
    pub namespace_constraint: NamespaceConstraint,

    /// Process contents mode
    pub process_contents: ProcessContents,

    /// Source location
    pub source: Option<SourceRef>,
}

/// Complex type definition
///
/// Represents an XSD complex type with content model and attributes.
#[derive(Debug, Clone)]
pub struct ComplexTypeDef {
    /// Name (None for anonymous types)
    pub name: Option<NameId>,

    /// Target namespace
    pub target_namespace: Option<NameId>,

    /// Source location for error reporting
    pub source: Option<SourceRef>,

    /// Base type (from which this type is derived)
    pub base_type: Option<TypeRef>,

    /// Derivation method (restriction or extension)
    pub derivation_method: Option<DerivationMethod>,

    /// Content model
    pub content: ComplexTypeContent,

    /// Content kind (for quick access)
    pub content_kind: ContentKind,

    /// Attribute uses
    pub attributes: Vec<AttributeUse>,

    /// Attribute group references
    pub attribute_groups: Vec<AttributeGroupKey>,

    /// Attribute wildcard
    pub attribute_wildcard: Option<AttributeWildcard>,

    /// Final derivation control (which derivations are prohibited)
    pub final_derivation: DerivationSet,

    /// Block derivation control (which derivations are blocked for instances)
    pub block: DerivationSet,

    /// Abstract flag (cannot be used directly in instances)
    pub is_abstract: bool,

    /// Mixed content flag
    pub mixed: bool,

    /// ID attribute value
    pub id: Option<String>,

    /// XSD 1.1: Whether schema-level `defaultAttributes` group applies to this type.
    /// Defaults to `true`; set to `false` via `defaultAttributesApply="false"`.
    /// The resolver injects the schema-level attribute group into `resolved_attribute_groups`.
    pub default_attributes_apply: bool,
}

impl ComplexTypeDef {
    /// Create a new complex type with empty content
    pub fn new(name: Option<NameId>, target_namespace: Option<NameId>) -> Self {
        Self {
            name,
            target_namespace,
            source: None,
            base_type: None,
            derivation_method: None,
            content: ComplexTypeContent::Empty,
            content_kind: ContentKind::Empty,
            attributes: Vec::new(),
            attribute_groups: Vec::new(),
            attribute_wildcard: None,
            final_derivation: DerivationSet::empty(),
            block: DerivationSet::empty(),
            is_abstract: false,
            mixed: false,
            id: None,
            default_attributes_apply: true,
        }
    }

    /// Check if this is an anonymous type
    pub fn is_anonymous(&self) -> bool {
        self.name.is_none()
    }

    /// Check if this is a global (named) type
    pub fn is_global(&self) -> bool {
        self.name.is_some()
    }

    /// Check if this type has simple content
    pub fn has_simple_content(&self) -> bool {
        matches!(self.content, ComplexTypeContent::Simple(_))
    }

    /// Check if this type has complex content
    pub fn has_complex_content(&self) -> bool {
        matches!(self.content, ComplexTypeContent::Complex(_))
    }

    /// Check if this type allows mixed content
    pub fn allows_mixed(&self) -> bool {
        self.mixed
    }

    /// Get the TypeKey for this complex type (requires its key)
    pub fn type_key(&self, key: ComplexTypeKey) -> TypeKey {
        TypeKey::Complex(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_complex_type_creation() {
        let ct = ComplexTypeDef::new(Some(NameId(1)), Some(NameId(2)));

        assert!(ct.is_global());
        assert!(!ct.is_anonymous());
        assert!(!ct.is_abstract);
        assert_eq!(ct.content_kind, ContentKind::Empty);
    }

    #[test]
    fn test_anonymous_complex_type() {
        let ct = ComplexTypeDef::new(None, None);
        assert!(ct.is_anonymous());
        assert!(!ct.is_global());
    }

    #[test]
    fn test_content_particle_default() {
        let particle = ContentParticle::new(ContentTerm::Group(ModelGroupDef {
            compositor: Compositor::Sequence,
            particles: vec![],
            source: None,
        }));

        assert_eq!(particle.min_occurs, 1);
        assert_eq!(particle.max_occurs, Some(1));
        assert!(!particle.is_optional());
        assert!(!particle.is_repeating());
    }

    #[test]
    fn test_content_particle_unbounded() {
        let particle = ContentParticle::with_occurs(
            ContentTerm::Group(ModelGroupDef {
                compositor: Compositor::Sequence,
                particles: vec![],
                source: None,
            }),
            0,
            None,
        );

        assert!(particle.is_optional());
        assert!(particle.is_repeating());
        assert!(particle.is_unbounded());
    }

    #[test]
    fn test_compositor_types() {
        assert_eq!(Compositor::Sequence, Compositor::Sequence);
        assert_ne!(Compositor::Sequence, Compositor::Choice);
    }

    #[test]
    fn test_attribute_use_kind() {
        assert_eq!(AttributeUseKind::default(), AttributeUseKind::Optional);
    }

    #[test]
    fn test_process_contents() {
        assert_eq!(ProcessContents::default(), ProcessContents::Strict);
    }
}
