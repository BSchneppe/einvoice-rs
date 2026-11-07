//! Arena storage for schema components
//!
//! All schema components are stored in arenas to avoid reference cycles.
//! Each component type has its own SlotMap with typed keys for type safety.
//!
//! Uses slotmap for O(1) insertion, lookup, and removal with generation tracking.

use std::ops::Deref;
use std::sync::{Arc, RwLock};

use slotmap::{SecondaryMap, SlotMap};

use crate::ids::*;
#[cfg(feature = "xsd11")]
use crate::parser::frames::AssertResult;
use crate::parser::frames::{
    AlternativeResult, AttributeUseResult, ComplexContentResult, Compositor, DerivationMethod,
    FieldResult, IdentityKind, OpenContentResult, ParticleResult, QNameRef, SelectorResult,
    SimpleTypeResult, SimpleTypeVariety, TypeFrameResult, TypeRefResult, WildcardResult,
};
use crate::parser::location::SourceRef;
use crate::schema::annotation::Annotation;
use crate::schema::model::DerivationSet;
use crate::types::facets::{EnumerationIndex, FacetSet};

// Forward declarations for types that will be defined later
// These are placeholders until we define the actual types

/// `src-resolve` reference miss deferred from schema compilation to
/// instance validation. The schema compiles; the error fires only if the
/// declaration carrying this payload is selected for validating an
/// instance. Used in XSD 1.0 for explicit element `type` and
/// `xs:list itemType` references whose target component does not exist.
#[derive(Debug, Clone)]
pub struct DeferredSrcResolve {
    pub message: String,
    pub source: Option<SourceRef>,
}

/// Placeholder for SimpleTypeDef (defined in types/simple.rs)
#[derive(Debug)]
pub struct SimpleTypeDefData {
    pub name: Option<NameId>,
    pub target_namespace: Option<NameId>,
    pub variety: SimpleTypeVariety,
    pub base_type: Option<TypeRefResult>,
    pub item_type: Option<TypeRefResult>,
    pub member_types: Vec<TypeRefResult>,
    pub facets: FacetSet,
    pub final_derivation: DerivationSet,
    pub id: Option<String>,
    pub derivation_id: Option<String>,
    pub annotation: Option<Annotation>,
    pub source: Option<SourceRef>,

    // Resolved references (populated after reference resolution phase)
    /// Resolved base type key (for restriction derivation)
    pub resolved_base_type: Option<TypeKey>,
    /// Resolved item type key (for list types)
    pub resolved_item_type: Option<TypeKey>,
    /// Resolved member type keys (for union types)
    pub resolved_member_types: Vec<TypeKey>,

    /// Original simple type key before redefine (for base-type resolution)
    pub redefine_original: Option<SimpleTypeKey>,

    /// Deferred `src-resolve` error for an `xs:list itemType` whose target
    /// is missing. Reported when this list type is used for validation.
    pub deferred_item_type_error: Option<DeferredSrcResolve>,
}

/// Resolved attribute use - stores resolved keys for attribute use references
#[derive(Debug, Clone)]
pub struct ResolvedAttributeUse {
    /// Resolved type key (from type_ref or inline type)
    pub resolved_type: Option<TypeKey>,
    /// Resolved attribute reference (for attribute refs)
    pub resolved_ref: Option<AttributeKey>,
}

/// Placeholder for ComplexTypeDef (defined in types/complex.rs)
#[derive(Debug)]
pub struct ComplexTypeDefData {
    pub name: Option<NameId>,
    pub target_namespace: Option<NameId>,
    pub base_type: Option<TypeRefResult>,
    pub derivation_method: Option<DerivationMethod>,
    pub content: ComplexContentResult,
    pub open_content: Option<OpenContentResult>,
    pub attributes: Vec<AttributeUseResult>,
    pub attribute_groups: Vec<QNameRef>,
    pub attribute_wildcard: Option<WildcardResult>,
    pub mixed: bool,
    pub is_abstract: bool,
    pub final_derivation: DerivationSet,
    pub block: DerivationSet,
    pub default_attributes_apply: bool,
    pub id: Option<String>,
    #[cfg(feature = "xsd11")]
    pub assertions: Vec<AssertResult>,
    #[cfg(feature = "xsd11")]
    pub xpath_default_namespace: Option<String>,
    pub annotation: Option<Annotation>,
    pub source: Option<SourceRef>,

    // Resolved references (populated after reference resolution phase)
    /// Resolved base type key (for extension/restriction derivation)
    pub resolved_base_type: Option<TypeKey>,
    /// Resolved attribute group keys
    pub resolved_attribute_groups: Vec<AttributeGroupKey>,
    /// Resolved attribute uses (parallel to attributes vec)
    pub resolved_attributes: Vec<ResolvedAttributeUse>,
    /// Resolved inline types for content particle elements (flat depth-first element order)
    pub resolved_content_particle_types: Vec<Option<TypeKey>>,
    /// Resolved element keys for local elements in content particles (flat depth-first element order)
    pub resolved_content_particle_elements: Vec<Option<ElementKey>>,
    /// Resolved inline simpleType inside simpleContent/restriction
    /// (§3.4.2.2 clause 1.1 — the B simple type definition).
    /// Only present when simpleContent has an explicit inline `<xs:simpleType>`.
    pub resolved_simple_content_type: Option<TypeKey>,

    /// Original complex type key before redefine (for base-type resolution)
    pub redefine_original: Option<ComplexTypeKey>,
}

/// Placeholder for ElementDecl (defined in schema/decl.rs)
#[derive(Debug)]
pub struct ElementDeclData {
    pub name: Option<NameId>,
    pub target_namespace: Option<NameId>,
    pub ref_name: Option<QNameRef>,
    pub type_ref: Option<TypeRefResult>,
    pub inline_type: Option<Box<TypeFrameResult>>,
    pub substitution_group: Vec<QNameRef>,
    pub default_value: Option<String>,
    pub fixed_value: Option<String>,
    pub nillable: bool,
    pub is_abstract: bool,
    pub min_occurs: u32,
    pub max_occurs: Option<u32>,
    pub block: DerivationSet,
    pub final_derivation: DerivationSet,
    pub form: Option<String>,
    pub id: Option<String>,
    pub alternatives: Vec<AlternativeResult>,
    pub identity_constraints: Vec<IdentityConstraintKey>,
    /// XSD 1.1: pending identity constraint @ref references (resolved in resolve_all_references)
    pub pending_ic_refs: Vec<(IdentityKind, QNameRef, Option<SourceRef>)>,
    pub annotation: Option<Annotation>,
    pub source: Option<SourceRef>,

    // Resolved references (populated after reference resolution phase)
    /// Resolved type key (from type_ref or inline_type)
    pub resolved_type: Option<TypeKey>,
    /// Resolved element reference (for element refs)
    pub resolved_ref: Option<ElementKey>,
    /// Resolved substitution group head elements
    pub resolved_substitution_groups: Vec<ElementKey>,

    /// Deferred `src-resolve` error for an explicit `type` attribute whose
    /// target is missing. Reported when this element declaration is
    /// selected for validation, before any `xs:anyType` fallback.
    pub deferred_type_error: Option<DeferredSrcResolve>,
}

/// Placeholder for AttributeDecl (defined in schema/decl.rs)
#[derive(Debug)]
pub struct AttributeDeclData {
    pub name: Option<NameId>,
    pub target_namespace: Option<NameId>,
    pub ref_name: Option<QNameRef>,
    pub type_ref: Option<TypeRefResult>,
    pub inline_type: Option<Box<SimpleTypeResult>>,
    pub default_value: Option<String>,
    pub fixed_value: Option<String>,
    pub use_kind: Option<String>,
    pub form: Option<String>,
    pub inheritable: bool,
    pub id: Option<String>,
    pub annotation: Option<Annotation>,
    pub source: Option<SourceRef>,

    // Resolved references (populated after reference resolution phase)
    /// Resolved type key (from type_ref or inline_type)
    pub resolved_type: Option<TypeKey>,
    /// Resolved attribute reference (for attribute refs)
    pub resolved_ref: Option<AttributeKey>,
}

/// Placeholder for AttributeGroup (defined in schema/group.rs)
#[derive(Debug)]
pub struct AttributeGroupData {
    pub name: Option<NameId>,
    pub target_namespace: Option<NameId>,
    pub ref_name: Option<QNameRef>,
    pub attributes: Vec<AttributeUseResult>,
    pub attribute_groups: Vec<QNameRef>,
    pub attribute_wildcard: Option<WildcardResult>,
    pub id: Option<String>,
    pub annotation: Option<Annotation>,
    pub source: Option<SourceRef>,

    // Resolved references (populated after reference resolution phase)
    /// Resolved attribute group reference (for attributeGroup refs)
    pub resolved_ref: Option<AttributeGroupKey>,
    /// Resolved nested attribute group keys
    pub resolved_attribute_groups: Vec<AttributeGroupKey>,
    /// Resolved attribute uses (parallel to attributes vec)
    pub resolved_attributes: Vec<ResolvedAttributeUse>,

    /// Original attribute group key before redefine (for self-reference resolution)
    pub redefine_original: Option<AttributeGroupKey>,
    /// When this attribute group is a zero-self-reference redefine, the
    /// deferred §src-redefine 7.2.2 restriction check must verify it is a
    /// valid restriction of `redefine_original` after reference resolution
    /// completes. `false` for every non-redefine construction site.
    pub redefine_requires_restriction_check: bool,
}

/// Resolved particle term - stores resolved keys for particle references
#[derive(Debug, Clone)]
pub enum ResolvedParticleTerm {
    /// Element with resolved type and ref
    Element {
        resolved_type: Option<TypeKey>,
        resolved_ref: Option<ElementKey>,
    },
    /// Group with resolved ref
    Group { resolved_ref: Option<ModelGroupKey> },
    /// Wildcard (no resolution needed)
    Any,
}

/// Placeholder for ModelGroup (defined in schema/group.rs)
#[derive(Debug)]
pub struct ModelGroupData {
    pub name: Option<NameId>,
    pub target_namespace: Option<NameId>,
    pub ref_name: Option<QNameRef>,
    pub compositor: Option<Compositor>,
    pub particles: Vec<ParticleResult>,
    pub min_occurs: u32,
    pub max_occurs: Option<u32>,
    pub id: Option<String>,
    pub annotation: Option<Annotation>,
    pub source: Option<SourceRef>,

    // Resolved references (populated after reference resolution phase)
    /// Resolved model group reference (for group refs)
    pub resolved_ref: Option<ModelGroupKey>,
    /// Resolved particle terms (parallel to particles vec)
    pub resolved_particles: Vec<ResolvedParticleTerm>,
    /// Flat depth-first indexed resolved types for all particles (including nested inline groups)
    pub resolved_particle_types: Vec<Option<TypeKey>>,
    /// Flat depth-first indexed resolved element keys for all particles (including nested inline groups)
    pub resolved_particle_elements: Vec<Option<ElementKey>>,

    /// Original model group key before redefine (for self-reference resolution)
    pub redefine_original: Option<ModelGroupKey>,
    /// When this model group is a zero-self-reference redefine, the deferred
    /// §src-redefine 6.2.2 restriction check must verify it is a valid
    /// restriction of `redefine_original` after reference resolution
    /// completes. `false` for every non-redefine construction site.
    pub redefine_requires_restriction_check: bool,
}

/// Placeholder for Notation (defined in schema/decl.rs)
#[derive(Debug)]
pub struct NotationData {
    pub name: NameId,
    pub target_namespace: Option<NameId>,
    pub public: Option<String>,
    pub system: Option<String>,
    pub id: Option<String>,
    pub annotation: Option<Annotation>,
    pub source: Option<SourceRef>,
}

/// Identity constraint (key, unique, keyref) stored in the arena
#[derive(Debug)]
pub struct IdentityConstraintData {
    pub kind: IdentityKind,
    pub name: NameId,
    pub ref_name: Option<QNameRef>,
    pub refer: Option<QNameRef>,
    pub selector: SelectorResult,
    pub fields: Vec<FieldResult>,
    pub id: Option<String>,
    pub annotation: Option<Annotation>,
    pub source: Option<SourceRef>,
}

/// Arena storage for all schema component types
///
/// Components are stored in type-specific SlotMaps and accessed via typed keys.
/// This approach avoids reference cycles and provides O(1) access with generation tracking.
#[derive(Debug, Default)]
pub struct SchemaArenas {
    /// Simple type definitions
    pub simple_types: SlotMap<SimpleTypeKey, SimpleTypeDefData>,
    /// Complex type definitions
    pub complex_types: SlotMap<ComplexTypeKey, ComplexTypeDefData>,
    /// Element declarations
    pub elements: SlotMap<ElementKey, ElementDeclData>,
    /// Attribute declarations
    pub attributes: SlotMap<AttributeKey, AttributeDeclData>,
    /// Attribute groups
    pub attribute_groups: SlotMap<AttributeGroupKey, AttributeGroupData>,
    /// Named model groups
    pub model_groups: SlotMap<ModelGroupKey, ModelGroupData>,
    /// Notations
    pub notations: SlotMap<NotationKey, NotationData>,
    /// Identity constraints
    pub identity_constraints: SlotMap<IdentityConstraintKey, IdentityConstraintData>,
}

impl SchemaArenas {
    /// Create new empty arenas
    pub fn new() -> Self {
        Self::default()
    }

    // Simple types
    pub fn alloc_simple_type(&mut self, data: SimpleTypeDefData) -> SimpleTypeKey {
        self.simple_types.insert(data)
    }

    pub fn get_simple_type(&self, key: SimpleTypeKey) -> Option<&SimpleTypeDefData> {
        self.simple_types.get(key)
    }

    pub fn get_simple_type_mut(&mut self, key: SimpleTypeKey) -> Option<&mut SimpleTypeDefData> {
        self.simple_types.get_mut(key)
    }

    // Complex types
    pub fn alloc_complex_type(&mut self, data: ComplexTypeDefData) -> ComplexTypeKey {
        self.complex_types.insert(data)
    }

    pub fn get_complex_type(&self, key: ComplexTypeKey) -> Option<&ComplexTypeDefData> {
        self.complex_types.get(key)
    }

    pub fn get_complex_type_mut(&mut self, key: ComplexTypeKey) -> Option<&mut ComplexTypeDefData> {
        self.complex_types.get_mut(key)
    }

    // Elements
    pub fn alloc_element(&mut self, data: ElementDeclData) -> ElementKey {
        self.elements.insert(data)
    }

    pub fn get_element(&self, key: ElementKey) -> Option<&ElementDeclData> {
        self.elements.get(key)
    }

    pub fn get_element_mut(&mut self, key: ElementKey) -> Option<&mut ElementDeclData> {
        self.elements.get_mut(key)
    }

    // Attributes
    pub fn alloc_attribute(&mut self, data: AttributeDeclData) -> AttributeKey {
        self.attributes.insert(data)
    }

    pub fn get_attribute(&self, key: AttributeKey) -> Option<&AttributeDeclData> {
        self.attributes.get(key)
    }

    pub fn get_attribute_mut(&mut self, key: AttributeKey) -> Option<&mut AttributeDeclData> {
        self.attributes.get_mut(key)
    }

    // Attribute groups
    pub fn alloc_attribute_group(&mut self, data: AttributeGroupData) -> AttributeGroupKey {
        self.attribute_groups.insert(data)
    }

    pub fn get_attribute_group(&self, key: AttributeGroupKey) -> Option<&AttributeGroupData> {
        self.attribute_groups.get(key)
    }

    pub fn get_attribute_group_mut(
        &mut self,
        key: AttributeGroupKey,
    ) -> Option<&mut AttributeGroupData> {
        self.attribute_groups.get_mut(key)
    }

    // Model groups
    pub fn alloc_model_group(&mut self, data: ModelGroupData) -> ModelGroupKey {
        self.model_groups.insert(data)
    }

    pub fn get_model_group(&self, key: ModelGroupKey) -> Option<&ModelGroupData> {
        self.model_groups.get(key)
    }

    pub fn get_model_group_mut(&mut self, key: ModelGroupKey) -> Option<&mut ModelGroupData> {
        self.model_groups.get_mut(key)
    }

    // Notations
    pub fn alloc_notation(&mut self, data: NotationData) -> NotationKey {
        self.notations.insert(data)
    }

    pub fn get_notation(&self, key: NotationKey) -> Option<&NotationData> {
        self.notations.get(key)
    }

    pub fn get_notation_mut(&mut self, key: NotationKey) -> Option<&mut NotationData> {
        self.notations.get_mut(key)
    }

    // Identity constraints
    pub fn alloc_identity_constraint(
        &mut self,
        data: IdentityConstraintData,
    ) -> IdentityConstraintKey {
        self.identity_constraints.insert(data)
    }

    pub fn get_identity_constraint(
        &self,
        key: IdentityConstraintKey,
    ) -> Option<&IdentityConstraintData> {
        self.identity_constraints.get(key)
    }

    pub fn get_identity_constraint_mut(
        &mut self,
        key: IdentityConstraintKey,
    ) -> Option<&mut IdentityConstraintData> {
        self.identity_constraints.get_mut(key)
    }
}

/// Read-transparent, mutation-gated wrapper around [`SchemaArenas`].
///
/// `SchemaSet` stores its arenas behind this guard so the lazily-computed
/// effective-facet cache (the `effective_facets` memo) can never go
/// stale. That cache is keyed solely by `SimpleTypeKey`, so it is only correct
/// as long as no *already-cached* simple type is mutated in place after the
/// entry was filled. The guard makes that invariant **structural** rather than
/// a load-bearing-but-undocumented convention:
///
/// * Shared access derefs straight through to `&SchemaArenas`, so every read
///   (`arenas.simple_types.get(..)`, `arenas.get_element(..)`, …) is unchanged
///   and zero-cost.
/// * There is intentionally no `DerefMut`. The only way to obtain
///   `&mut SchemaArenas` — and therefore to `get_mut`/index-mut an *existing*
///   component — is [`ArenasGuard::entries_mut`], which **clears the
///   effective-facet cache first**. A facet/base-chain edit can never outlive
///   the cache entry it would invalidate.
/// * Allocating a *new* component is always cache-safe (a fresh `SimpleTypeKey`
///   is a cache miss that recomputes correctly), so the `alloc_*` paths are
///   forwarded without clearing the cache.
///
/// This realizes the "move the mutate-phase behind a `&mut` gate" fix: every
/// mutation of existing arena state passes through the cache-clearing gate, so
/// the cache is provably consistent with the live arenas at all times.
#[derive(Debug, Default)]
pub struct ArenasGuard {
    inner: SchemaArenas,
    /// Per-simple-type effective (base-chain-merged) facet sets, memoized on
    /// first access and invalidated by [`entries_mut`](Self::entries_mut).
    /// Computed lazily *per entry* (not a write-once snapshot), so a simple type
    /// added to a reused `SchemaSet` after an earlier validation pass is a cache
    /// miss that gets computed, never a silent empty fallback.
    /// Dense `SecondaryMap` keyed by the `SimpleTypeKey` slotmap index — lookup
    /// is an array index with a generation check, **no hashing** (the key is a
    /// dense generational index, so SipHashing it was the worst possible choice).
    effective_facets_cache: RwLock<SecondaryMap<SimpleTypeKey, Arc<FacetSet>>>,
    /// Per-simple-type index of the effective `enumeration` as expanded names
    /// (QName / NOTATION types), built on first use from the effective facets
    /// and invalidated with them by [`entries_mut`](Self::entries_mut).
    enumeration_index_cache: RwLock<SecondaryMap<SimpleTypeKey, Arc<EnumerationIndex>>>,
}

impl ArenasGuard {
    /// Create an empty guard wrapping empty arenas.
    pub fn new() -> Self {
        Self::default()
    }

    /// Borrow the arenas mutably to mutate *existing* components in place (e.g.
    /// filling `resolved_*` fields during compilation, applying redefine/override),
    /// clearing the effective-facet cache first so no stale entry can survive the
    /// edit. This is the sole source of `&mut SchemaArenas` (see the type docs).
    /// Allocating new components needs no gate — use the `alloc_*` forwarders.
    pub fn entries_mut(&mut self) -> &mut SchemaArenas {
        // `&mut self` already guarantees exclusivity, so `get_mut` needs no lock.
        self.effective_facets_cache.get_mut().unwrap().clear();
        self.enumeration_index_cache.get_mut().unwrap().clear();
        &mut self.inner
    }

    // -- Allocation forwarders (new key => always cache-safe) ---------------

    pub fn alloc_simple_type(&mut self, data: SimpleTypeDefData) -> SimpleTypeKey {
        self.inner.alloc_simple_type(data)
    }

    pub fn alloc_complex_type(&mut self, data: ComplexTypeDefData) -> ComplexTypeKey {
        self.inner.alloc_complex_type(data)
    }

    pub fn alloc_element(&mut self, data: ElementDeclData) -> ElementKey {
        self.inner.alloc_element(data)
    }

    pub fn alloc_attribute(&mut self, data: AttributeDeclData) -> AttributeKey {
        self.inner.alloc_attribute(data)
    }

    pub fn alloc_attribute_group(&mut self, data: AttributeGroupData) -> AttributeGroupKey {
        self.inner.alloc_attribute_group(data)
    }

    pub fn alloc_model_group(&mut self, data: ModelGroupData) -> ModelGroupKey {
        self.inner.alloc_model_group(data)
    }

    pub fn alloc_notation(&mut self, data: NotationData) -> NotationKey {
        self.inner.alloc_notation(data)
    }

    pub fn alloc_identity_constraint(
        &mut self,
        data: IdentityConstraintData,
    ) -> IdentityConstraintKey {
        self.inner.alloc_identity_constraint(data)
    }

    // -- Effective-facet cache ----------------------------------------------

    /// The effective facet set for a simple type — the type's own facets merged
    /// with every base type's facets up the derivation chain (most-derived
    /// wins, matching the inheritance rules `inherit_from` applies).
    ///
    /// Memoized per simple type so the validation hot path does a single map
    /// lookup instead of re-walking the base chain and cloning a `FacetSet` per
    /// value. Filled on demand (miss → compute → insert); wiped wholesale by
    /// [`entries_mut`](Self::entries_mut), so it is always consistent with the
    /// current arena contents.
    pub(crate) fn effective_facets(&self, sk: SimpleTypeKey) -> Arc<FacetSet> {
        if let Some(facets) = self.effective_facets_cache.read().unwrap().get(sk) {
            return Arc::clone(facets);
        }
        let computed = Arc::new(self.compute_effective_facets(sk));
        self.effective_facets_cache
            .write()
            .unwrap()
            .insert(sk, Arc::clone(&computed));
        computed
    }

    /// The expanded-name index of a QName / NOTATION type's effective
    /// enumeration, memoized like [`effective_facets`](Self::effective_facets)
    /// (miss → `build` → insert; wiped by [`entries_mut`](Self::entries_mut)).
    pub(crate) fn enumeration_index(
        &self,
        sk: SimpleTypeKey,
        build: impl FnOnce() -> EnumerationIndex,
    ) -> Arc<EnumerationIndex> {
        if let Some(index) = self.enumeration_index_cache.read().unwrap().get(sk) {
            return Arc::clone(index);
        }
        let built = Arc::new(build());
        self.enumeration_index_cache
            .write()
            .unwrap()
            .insert(sk, Arc::clone(&built));
        built
    }

    /// Compute the effective (base-chain-merged) facet set for one simple type.
    /// Start from the most-derived facets, then `inherit_from` each base (fills
    /// only missing values, so derived facets take priority). Cycle-guarded at
    /// depth 100.
    fn compute_effective_facets(&self, sk: SimpleTypeKey) -> FacetSet {
        let Some(st) = self.inner.simple_types.get(sk) else {
            return FacetSet::new();
        };
        let mut facets = st.facets.clone();
        let mut current_base = st.resolved_base_type;
        for _ in 0..100 {
            match current_base {
                Some(TypeKey::Simple(base_sk)) => match self.inner.simple_types.get(base_sk) {
                    Some(base_data) => {
                        facets.inherit_from(&base_data.facets);
                        current_base = base_data.resolved_base_type;
                    }
                    None => break,
                },
                _ => break,
            }
        }
        facets
    }
}

impl Deref for ArenasGuard {
    type Target = SchemaArenas;

    fn deref(&self) -> &SchemaArenas {
        &self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simple_type_data(name: NameId) -> SimpleTypeDefData {
        SimpleTypeDefData {
            name: Some(name),
            target_namespace: None,
            variety: SimpleTypeVariety::Atomic,
            base_type: None,
            item_type: None,
            member_types: Vec::new(),
            facets: FacetSet::new(),
            final_derivation: DerivationSet::empty(),
            id: None,
            derivation_id: None,
            annotation: None,
            source: None,
            // Resolved references
            resolved_base_type: None,
            resolved_item_type: None,
            resolved_member_types: Vec::new(),
            redefine_original: None,
            deferred_item_type_error: None,
        }
    }

    fn element_data(name: NameId, target_namespace: Option<NameId>) -> ElementDeclData {
        ElementDeclData {
            name: Some(name),
            target_namespace,
            ref_name: None,
            type_ref: None,
            inline_type: None,
            substitution_group: Vec::new(),
            default_value: None,
            fixed_value: None,
            nillable: false,
            is_abstract: false,
            min_occurs: 1,
            max_occurs: Some(1),
            block: DerivationSet::empty(),
            final_derivation: DerivationSet::empty(),
            form: None,
            id: None,
            alternatives: Vec::new(),
            identity_constraints: Vec::new(),
            pending_ic_refs: vec![],
            annotation: None,
            source: None,
            // Resolved references
            resolved_type: None,
            resolved_ref: None,
            resolved_substitution_groups: Vec::new(),
            deferred_type_error: None,
        }
    }

    #[test]
    fn test_alloc_and_get_simple_type() {
        let mut arenas = SchemaArenas::new();
        let data = simple_type_data(NameId(1));
        let key = arenas.alloc_simple_type(data);
        let retrieved = arenas.get_simple_type(key).unwrap();
        assert_eq!(retrieved.name, Some(NameId(1)));
    }

    #[test]
    fn test_alloc_and_get_element() {
        let mut arenas = SchemaArenas::new();
        let data = element_data(NameId(2), Some(NameId(3)));
        let key = arenas.alloc_element(data);
        let retrieved = arenas.get_element(key).unwrap();
        assert_eq!(retrieved.name, Some(NameId(2)));
        assert_eq!(retrieved.target_namespace, Some(NameId(3)));
    }

    #[test]
    fn test_multiple_allocations() {
        let mut arenas = SchemaArenas::new();

        let key1 = arenas.alloc_simple_type(simple_type_data(NameId(1)));
        let key2 = arenas.alloc_simple_type(simple_type_data(NameId(2)));

        assert_ne!(key1, key2);
        assert_eq!(arenas.get_simple_type(key1).unwrap().name, Some(NameId(1)));
        assert_eq!(arenas.get_simple_type(key2).unwrap().name, Some(NameId(2)));
    }

    #[test]
    fn test_mutable_access() {
        let mut arenas = SchemaArenas::new();
        let key = arenas.alloc_simple_type(simple_type_data(NameId(1)));

        // Modify through mutable reference
        if let Some(data) = arenas.get_simple_type_mut(key) {
            data.name = Some(NameId(99));
        }

        assert_eq!(arenas.get_simple_type(key).unwrap().name, Some(NameId(99)));
    }

    #[test]
    fn test_slotmap_iteration() {
        let mut arenas = SchemaArenas::new();
        arenas.alloc_simple_type(simple_type_data(NameId(10)));
        arenas.alloc_simple_type(simple_type_data(NameId(20)));
        arenas.alloc_simple_type(simple_type_data(NameId(30)));

        let names: Vec<_> = arenas
            .simple_types
            .values()
            .filter_map(|d| d.name)
            .map(|n| n.0)
            .collect();
        assert_eq!(names.len(), 3);
        assert!(names.contains(&10));
        assert!(names.contains(&20));
        assert!(names.contains(&30));
    }

    #[test]
    fn test_key_with_values() {
        let mut arenas = SchemaArenas::new();
        let key1 = arenas.alloc_element(element_data(NameId(1), None));
        let key2 = arenas.alloc_element(element_data(NameId(2), None));

        // Iterate with keys
        let pairs: Vec<_> = arenas
            .elements
            .iter()
            .map(|(k, v)| (k, v.name.unwrap().0))
            .collect();

        assert_eq!(pairs.len(), 2);
        assert!(pairs.iter().any(|(k, _)| *k == key1));
        assert!(pairs.iter().any(|(k, _)| *k == key2));
    }

    #[test]
    fn test_effective_facets_cached_and_recomputed_after_entries_mut() {
        use crate::types::facets::FacetFixed;

        let mut guard = ArenasGuard::new();
        let key = guard.alloc_simple_type(simple_type_data(NameId(1)));

        // First access: computes from the (empty) facets and caches.
        let first = guard.effective_facets(key);
        assert!(first.min_length.is_none());

        // Second access without mutation returns the *same* cached Arc.
        let second = guard.effective_facets(key);
        assert!(Arc::ptr_eq(&first, &second));

        // Mutate the type's own facets through the cache-clearing gate.
        guard.entries_mut().simple_types[key]
            .facets
            .set_min_length(3, FacetFixed::Default, None);

        // Next access must recompute (cache was cleared by entries_mut), so the
        // new min_length is reflected — never a stale empty/cached fallback.
        let recomputed = guard.effective_facets(key);
        assert!(!Arc::ptr_eq(&first, &recomputed));
        assert_eq!(recomputed.min_length.as_ref().map(|f| f.value), Some(3));
    }
}
