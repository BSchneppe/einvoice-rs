//! Schema composition graph types
//!
//! Tracks the relationships between schema documents created by
//! `xs:include`, `xs:import`, `xs:redefine`, and `xs:override` directives.
//!
//! These types are populated during directive resolution (Step 1) and
//! consumed by later pipeline stages for document-scoped component lookup,
//! provenance tracking, and override processing order.

use std::collections::HashMap;

use crate::ids::*;
use crate::parser::location::SourceRef;

/// The kind of composition relationship between two schema documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompositionEdgeKind {
    Include,
    Import,
    Redefine,
    #[cfg(feature = "xsd11")]
    Override,
}

/// A directed edge in the schema composition graph.
///
/// Records that `source_doc` references `target_doc` via a composition
/// directive of the given `kind`. The `source` field links back to the
/// directive element in the source document for diagnostics.
///
/// `target_doc` is `None` for cycle edges discovered while the target is
/// still mid-parse (in the resolver's `resolving` set). After directive
/// resolution completes, call [`fixup_composition_edges`](crate::parser::resolver::fixup_composition_edges) to fill in
/// any `None` targets from `loaded_locations`.
#[derive(Debug, Clone)]
pub struct CompositionEdge {
    pub source_doc: DocumentId,
    pub target_doc: Option<DocumentId>,
    /// Resolved URI of the target (for fixup and diagnostics).
    pub resolved_location: String,
    pub kind: CompositionEdgeKind,
    /// Source location of the directive element (for diagnostics).
    pub source: Option<SourceRef>,
    /// Raw `schemaLocation` attribute value (not the resolved URI).
    pub schema_location: String,
}

// ============================================================================
// Forward-looking types for Steps 2–3 (defined now, used later)
// ============================================================================

/// Classification of a top-level schema component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ComponentKind {
    SimpleType,
    ComplexType,
    Element,
    Attribute,
    ModelGroup,
    AttributeGroup,
    Notation,
    IdentityConstraint,
}

impl ComponentKind {
    /// Human-readable label for diagnostic messages
    /// (e.g. "simple type", "attribute group").
    pub fn display_name(self) -> &'static str {
        match self {
            ComponentKind::SimpleType => "simple type",
            ComponentKind::ComplexType => "complex type",
            ComponentKind::Element => "element",
            ComponentKind::Attribute => "attribute",
            ComponentKind::ModelGroup => "model group",
            ComponentKind::AttributeGroup => "attribute group",
            ComponentKind::Notation => "notation",
            ComponentKind::IdentityConstraint => "identity constraint",
        }
    }
}

/// Unique identity of a schema component (kind + QName).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ComponentIdentity {
    pub kind: ComponentKind,
    pub name: NameId,
    pub namespace: Option<NameId>,
}

/// Tracks which document a component originated from.
///
/// `owner_doc` is `None` when the originating document is unknown (e.g.,
/// pre-loaded schemas without directive resolution).
#[derive(Debug, Clone, Copy)]
pub struct ComponentOrigin {
    pub owner_doc: Option<DocumentId>,
    pub identity: ComponentIdentity,
}

/// A typed arena key for any top-level schema component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentKey {
    Type(TypeKey),
    Element(ElementKey),
    Attribute(AttributeKey),
    ModelGroup(ModelGroupKey),
    AttributeGroup(AttributeGroupKey),
    Notation(NotationKey),
    IdentityConstraint(IdentityConstraintKey),
}

/// How a component arrived in the effective schema set.
#[derive(Debug, Clone)]
pub enum CompositionAction {
    /// Declared directly in its owning document.
    Declared,
    /// Included from another document (same namespace merge).
    Included { from_doc: DocumentId },
    /// Replaced via `xs:redefine`. `from_doc` is `None` when the redefining
    /// document is unknown (directive without source location).
    Redefined {
        from_doc: Option<DocumentId>,
        replaced: ComponentOrigin,
    },
    /// Replaced via `xs:override` (XSD 1.1). `from_doc` is `None` when the
    /// overriding document is unknown.
    #[cfg(feature = "xsd11")]
    Overridden {
        from_doc: Option<DocumentId>,
        replaced: ComponentOrigin,
    },
}

/// An effective top-level component with its provenance metadata.
///
/// After composition, every visible component in the namespace tables
/// has an associated `EffectiveComponent` that records which document
/// contributed it and how it arrived (declared, included, redefined, etc.).
#[derive(Debug, Clone)]
pub struct EffectiveComponent {
    /// The arena key of the component.
    pub key: ComponentKey,
    /// Where this component originated.
    pub origin: ComponentOrigin,
    /// How this component entered the effective schema.
    pub action: CompositionAction,
}

/// Record a composition provenance entry into the effective components map.
///
/// Used by both `apply_redefine` and `apply_override` to record how a
/// component arrived in the effective schema set.
pub fn record_provenance(
    effective_components: &mut HashMap<ComponentIdentity, EffectiveComponent>,
    key: ComponentKey,
    kind: ComponentKind,
    namespace: Option<NameId>,
    name: NameId,
    acting_doc_id: Option<DocumentId>,
    action: CompositionAction,
) {
    let identity = ComponentIdentity {
        kind,
        name,
        namespace,
    };
    let origin = ComponentOrigin {
        owner_doc: acting_doc_id,
        identity,
    };
    effective_components.insert(
        identity,
        EffectiveComponent {
            key,
            origin,
            action,
        },
    );
}

/// Build a [`CompositionAction::Redefined`] action.
pub fn redefined_action(
    redefining_doc_id: Option<DocumentId>,
    kind: ComponentKind,
    name: NameId,
    namespace: Option<NameId>,
    target_doc_id: Option<DocumentId>,
) -> CompositionAction {
    let identity = ComponentIdentity {
        kind,
        name,
        namespace,
    };
    CompositionAction::Redefined {
        from_doc: redefining_doc_id,
        replaced: ComponentOrigin {
            owner_doc: target_doc_id,
            identity,
        },
    }
}

/// Build a [`CompositionAction::Overridden`] action (XSD 1.1).
#[cfg(feature = "xsd11")]
pub fn overridden_action(
    overriding_doc_id: Option<DocumentId>,
    kind: ComponentKind,
    name: NameId,
    namespace: Option<NameId>,
    target_doc_id: Option<DocumentId>,
) -> CompositionAction {
    let identity = ComponentIdentity {
        kind,
        name,
        namespace,
    };
    CompositionAction::Overridden {
        from_doc: overriding_doc_id,
        replaced: ComponentOrigin {
            owner_doc: target_doc_id,
            identity,
        },
    }
}

/// Per-document index of top-level components declared by a single schema document.
///
/// Populated during assembly (`register_*` calls) and used for document-scoped
/// lookup in `apply_redefine()` and `apply_override()`.
#[derive(Debug, Default)]
pub struct DocumentComponentIndex {
    index: HashMap<ComponentIdentity, ComponentKey>,
}

impl DocumentComponentIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a component into the index.
    pub fn insert(&mut self, identity: ComponentIdentity, key: ComponentKey) {
        self.index.insert(identity, key);
    }

    /// Raw lookup by kind, namespace, and name.
    fn get(
        &self,
        kind: ComponentKind,
        namespace: Option<NameId>,
        name: NameId,
    ) -> Option<&ComponentKey> {
        self.index.get(&ComponentIdentity {
            kind,
            name,
            namespace,
        })
    }

    /// Look up a type by namespace and local name (both simple and complex).
    pub fn lookup_type(&self, namespace: Option<NameId>, name: NameId) -> Option<TypeKey> {
        self.lookup_simple_type(namespace, name)
            .map(TypeKey::Simple)
            .or_else(|| {
                self.lookup_complex_type(namespace, name)
                    .map(TypeKey::Complex)
            })
    }

    /// Look up a simple type by namespace and local name.
    pub fn lookup_simple_type(
        &self,
        namespace: Option<NameId>,
        name: NameId,
    ) -> Option<SimpleTypeKey> {
        match self.get(ComponentKind::SimpleType, namespace, name) {
            Some(&ComponentKey::Type(TypeKey::Simple(key))) => Some(key),
            _ => None,
        }
    }

    /// Look up a complex type by namespace and local name.
    pub fn lookup_complex_type(
        &self,
        namespace: Option<NameId>,
        name: NameId,
    ) -> Option<ComplexTypeKey> {
        match self.get(ComponentKind::ComplexType, namespace, name) {
            Some(&ComponentKey::Type(TypeKey::Complex(key))) => Some(key),
            _ => None,
        }
    }

    /// Look up an element by namespace and local name.
    pub fn lookup_element(&self, namespace: Option<NameId>, name: NameId) -> Option<ElementKey> {
        match self.get(ComponentKind::Element, namespace, name) {
            Some(&ComponentKey::Element(key)) => Some(key),
            _ => None,
        }
    }

    /// Look up an attribute by namespace and local name.
    pub fn lookup_attribute(
        &self,
        namespace: Option<NameId>,
        name: NameId,
    ) -> Option<AttributeKey> {
        match self.get(ComponentKind::Attribute, namespace, name) {
            Some(&ComponentKey::Attribute(key)) => Some(key),
            _ => None,
        }
    }

    /// Look up a model group by namespace and local name.
    pub fn lookup_model_group(
        &self,
        namespace: Option<NameId>,
        name: NameId,
    ) -> Option<ModelGroupKey> {
        match self.get(ComponentKind::ModelGroup, namespace, name) {
            Some(&ComponentKey::ModelGroup(key)) => Some(key),
            _ => None,
        }
    }

    /// Look up an attribute group by namespace and local name.
    pub fn lookup_attribute_group(
        &self,
        namespace: Option<NameId>,
        name: NameId,
    ) -> Option<AttributeGroupKey> {
        match self.get(ComponentKind::AttributeGroup, namespace, name) {
            Some(&ComponentKey::AttributeGroup(key)) => Some(key),
            _ => None,
        }
    }

    /// Look up a notation by namespace and local name.
    pub fn lookup_notation(&self, namespace: Option<NameId>, name: NameId) -> Option<NotationKey> {
        match self.get(ComponentKind::Notation, namespace, name) {
            Some(&ComponentKey::Notation(key)) => Some(key),
            _ => None,
        }
    }

    /// Returns `true` if the index is empty.
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// Returns the number of components in the index.
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// Iterate over all `(identity, key)` pairs in the index.
    pub fn iter(&self) -> impl Iterator<Item = (&ComponentIdentity, &ComponentKey)> {
        self.index.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_composition_edge_kind_equality() {
        assert_eq!(CompositionEdgeKind::Include, CompositionEdgeKind::Include);
        assert_ne!(CompositionEdgeKind::Include, CompositionEdgeKind::Import);
        assert_ne!(CompositionEdgeKind::Import, CompositionEdgeKind::Redefine);
    }

    #[test]
    fn test_composition_edge_construction() {
        let edge = CompositionEdge {
            source_doc: 0,
            target_doc: Some(1),
            resolved_location: "/path/to/types.xsd".to_string(),
            kind: CompositionEdgeKind::Include,
            source: None,
            schema_location: "types.xsd".to_string(),
        };
        assert_eq!(edge.source_doc, 0);
        assert_eq!(edge.target_doc, Some(1));
        assert_eq!(edge.kind, CompositionEdgeKind::Include);
        assert!(edge.source.is_none());
        assert_eq!(edge.schema_location, "types.xsd");
    }

    #[test]
    fn test_composition_edge_cycle_none_target() {
        let edge = CompositionEdge {
            source_doc: 1,
            target_doc: None,
            resolved_location: "/path/to/cyclic.xsd".to_string(),
            kind: CompositionEdgeKind::Include,
            source: None,
            schema_location: "cyclic.xsd".to_string(),
        };
        assert!(edge.target_doc.is_none());
        assert_eq!(edge.resolved_location, "/path/to/cyclic.xsd");
    }

    #[test]
    fn test_composition_edge_with_source() {
        use crate::parser::location::{SourceRef, SourceSpan};
        let edge = CompositionEdge {
            source_doc: 0,
            target_doc: Some(1),
            resolved_location: "/path/to/base.xsd".to_string(),
            kind: CompositionEdgeKind::Redefine,
            source: Some(SourceRef::new(0, SourceSpan::new(100, 200))),
            schema_location: "base.xsd".to_string(),
        };
        let src = edge.source.unwrap();
        assert_eq!(src.doc_id, 0);
        assert_eq!(src.span.start, 100);
    }

    #[test]
    fn test_component_kind_variants() {
        let kinds = [
            ComponentKind::SimpleType,
            ComponentKind::ComplexType,
            ComponentKind::Element,
            ComponentKind::Attribute,
            ComponentKind::ModelGroup,
            ComponentKind::AttributeGroup,
            ComponentKind::Notation,
            ComponentKind::IdentityConstraint,
        ];
        // All variants are distinct
        for (i, a) in kinds.iter().enumerate() {
            for (j, b) in kinds.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b);
                }
            }
        }
    }

    #[test]
    fn test_component_identity_equality_and_copy() {
        let id1 = ComponentIdentity {
            kind: ComponentKind::Element,
            name: NameId(1),
            namespace: Some(NameId(2)),
        };
        // Copy trait: assignment copies value
        let id2 = id1;
        assert_eq!(id1, id2);

        let id3 = ComponentIdentity {
            kind: ComponentKind::Attribute,
            name: NameId(1),
            namespace: Some(NameId(2)),
        };
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_component_origin_construction_and_copy() {
        let origin = ComponentOrigin {
            owner_doc: Some(42),
            identity: ComponentIdentity {
                kind: ComponentKind::SimpleType,
                name: NameId(5),
                namespace: None,
            },
        };
        // Copy trait: assignment copies value
        let origin2 = origin;
        assert_eq!(origin.owner_doc, Some(42));
        assert_eq!(origin.owner_doc, origin2.owner_doc);
        assert_eq!(origin.identity.kind, ComponentKind::SimpleType);
        assert_eq!(origin.identity.name, NameId(5));
        assert_eq!(origin.identity.namespace, None);
    }

    #[cfg(feature = "xsd11")]
    #[test]
    fn test_override_edge_kind() {
        assert_eq!(CompositionEdgeKind::Override, CompositionEdgeKind::Override);
        assert_ne!(CompositionEdgeKind::Override, CompositionEdgeKind::Include);
    }
}
