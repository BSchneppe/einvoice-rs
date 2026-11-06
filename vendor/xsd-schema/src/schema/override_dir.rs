//! xs:override processing (XSD 1.1)
//!
//! Override is simpler than redefine - it simply replaces components
//! without requiring self-derivation/self-reference constraints.
//!
//! # XSD 1.1 Semantics
//!
//! The xs:override element allows any schema top-level component to be
//! replaced with a new definition. Unlike xs:redefine:
//! - No self-derivation constraint for types
//! - No self-reference constraint for groups
//! - Supports all schemaTop elements (types, elements, attributes, groups, notations)
//!
//! The overriding component completely replaces the original.
//!
//! When `resolved_doc_id` is available, override only replaces components
//! that actually exist in the target document's component index. Components
//! not found in the target document are silently skipped per §4.2.5.

use std::collections::HashSet;

use crate::error::{SchemaError, SchemaResult};
use crate::ids::*;
use crate::parser::frames::{
    ComplexContentResult, ComplexTypeResult, ElementFrameResult, ParticleResult, ParticleTerm,
    TypeFrameResult,
};
use crate::schema::composition::{
    overridden_action, record_provenance, ComponentKey, ComponentKind, CompositionEdgeKind,
};
use crate::schema::model::{DerivationSet, OverrideComponent, OverrideDirective};
use crate::schema::SchemaSet;

/// Extract the `(kind, namespace, name)` identity tuple for an override
/// component. Returns `None` for anonymous components (legal for local
/// inline types but never valid as direct children of `<xs:override>`,
/// where every component is required to carry a `name`). The existing
/// `override_*` helpers enforce the name requirement with an explicit
/// `src-override` error; this helper is intentionally lenient so it can
/// be reused by the pre-pass without duplicating error emission.
fn override_component_identity(
    schema_set: &SchemaSet,
    component: &OverrideComponent,
) -> Option<(ComponentKind, Option<NameId>, NameId)> {
    match *component {
        OverrideComponent::SimpleType(k) => {
            let t = schema_set.arenas.simple_types.get(k)?;
            Some((ComponentKind::SimpleType, t.target_namespace, t.name?))
        }
        OverrideComponent::ComplexType(k) => {
            let t = schema_set.arenas.complex_types.get(k)?;
            Some((ComponentKind::ComplexType, t.target_namespace, t.name?))
        }
        OverrideComponent::Group(k) => {
            let g = schema_set.arenas.model_groups.get(k)?;
            Some((ComponentKind::ModelGroup, g.target_namespace, g.name?))
        }
        OverrideComponent::AttributeGroup(k) => {
            let g = schema_set.arenas.attribute_groups.get(k)?;
            Some((ComponentKind::AttributeGroup, g.target_namespace, g.name?))
        }
        OverrideComponent::Element(k) => {
            let e = schema_set.arenas.elements.get(k)?;
            Some((ComponentKind::Element, e.target_namespace, e.name?))
        }
        OverrideComponent::Attribute(k) => {
            let a = schema_set.arenas.attributes.get(k)?;
            Some((ComponentKind::Attribute, a.target_namespace, a.name?))
        }
        OverrideComponent::Notation(k) => {
            let n = schema_set.arenas.notations.get(k)?;
            Some((ComponentKind::Notation, n.target_namespace, n.name))
        }
    }
}

/// Validate every `<xs:override>` directive in the schema set against
/// §4.2.5 constraints, before any directive is applied.
///
/// Enforces two rules and emits `src-override` on failure:
///
/// 1. **Target-namespace compatibility.** The overriding and overridden
///    documents must either both lack a `targetNamespace` or share the
///    same value. The chameleon case (overriding has a namespace,
///    overridden has none) is legal.
///
/// 2. **No conflicting duplicate overrides in a single document.**
///    Two children sharing `(kind, namespace, local-name)` inside one
///    `<xs:override>` block are always rejected; across separate blocks
///    they are only rejected when both would actually match a component
///    in their target set (matching `apply_override`'s silent-skip rule
///    for unmatched children).
pub fn validate_override_directives(schema_set: &SchemaSet) -> SchemaResult<()> {
    for doc in &schema_set.documents {
        for directive in &doc.overrides {
            let Some(target_id) = directive.resolved_doc_id else {
                continue;
            };
            let Some(target_doc) = schema_set.documents.get(target_id as usize) else {
                continue;
            };
            // Use the *effective* post-chameleon `target_namespace` on
            // both sides — an included target may already have been
            // chameleon-adopted into a parent namespace, so the
            // pre-chameleon `declared_target_namespace` would reject
            // compositions the spec considers valid.
            let d1_ns = doc.target_namespace;
            let d2_ns = target_doc.target_namespace;
            let compatible = match (d1_ns, d2_ns) {
                (None, None) => true,
                (Some(_), None) => true, // chameleon case
                (Some(a), Some(b)) => a == b,
                (None, Some(_)) => false,
            };
            if !compatible {
                let location = directive
                    .source
                    .as_ref()
                    .and_then(|s| schema_set.source_maps.locate(s));
                let d1_label = d1_ns
                    .map(|ns| schema_set.name_table.resolve(ns).to_string())
                    .unwrap_or_else(|| "(absent)".to_string());
                let d2_label = d2_ns
                    .map(|ns| schema_set.name_table.resolve(ns).to_string())
                    .unwrap_or_else(|| "(absent)".to_string());
                return Err(SchemaError::structural(
                    "src-override",
                    format!(
                        "Override target-namespace mismatch: overriding schema \
                         '{}' (ns={}) cannot override '{}' (ns={}); both must \
                         share the same namespace or both must lack one (§4.2.5)",
                        doc.base_uri, d1_label, target_doc.base_uri, d2_label,
                    ),
                    location,
                ));
            }
        }

        let mut active_cross_block: HashSet<(ComponentKind, Option<NameId>, NameId)> =
            HashSet::new();
        for directive in &doc.overrides {
            let target_set = match directive.resolved_doc_id {
                Some(id) => compute_target_set(schema_set, id),
                None => HashSet::new(),
            };
            let mut in_block: HashSet<(ComponentKind, Option<NameId>, NameId)> = HashSet::new();
            for component in &directive.components {
                let Some(identity) = override_component_identity(schema_set, component) else {
                    continue;
                };
                if !in_block.insert(identity) {
                    let (kind, _ns, name) = identity;
                    let location = directive
                        .source
                        .as_ref()
                        .and_then(|s| schema_set.source_maps.locate(s));
                    let name_str = schema_set.name_table.resolve(name);
                    return Err(SchemaError::structural(
                        "src-override",
                        format!(
                            "Duplicate override of {} '{}' inside a single \
                             <xs:override> block in schema document '{}' (§4.2.5)",
                            kind.display_name(),
                            name_str,
                            doc.base_uri,
                        ),
                        location,
                    ));
                }
                let (kind, namespace, name) = identity;
                let matches_target =
                    target_set_has_identity(schema_set, &target_set, kind, namespace, name);
                if matches_target && !active_cross_block.insert(identity) {
                    let location = directive
                        .source
                        .as_ref()
                        .and_then(|s| schema_set.source_maps.locate(s));
                    let name_str = schema_set.name_table.resolve(name);
                    return Err(SchemaError::structural(
                        "src-override",
                        format!(
                            "Duplicate override of {} '{}' across separate \
                             <xs:override> blocks in schema document '{}': both \
                             children match and would replace the same target \
                             component (§4.2.5)",
                            kind.display_name(),
                            name_str,
                            doc.base_uri,
                        ),
                        location,
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Check whether the component with the given `(kind, namespace, name)`
/// identity exists in any document in the target set. Returns `true`
/// when the target set is empty (unresolved doc — treated as
/// unconditional replacement, matching [`target_set_has_component`]).
fn target_set_has_identity(
    schema_set: &SchemaSet,
    target_set: &HashSet<DocumentId>,
    kind: ComponentKind,
    namespace: Option<NameId>,
    name: NameId,
) -> bool {
    if target_set.is_empty() {
        return true;
    }
    target_set.iter().any(|&doc_id| {
        let Some(doc) = schema_set.documents.get(doc_id as usize) else {
            return false;
        };
        let idx = &doc.component_index;
        match kind {
            ComponentKind::SimpleType => idx.lookup_simple_type(namespace, name).is_some(),
            ComponentKind::ComplexType => idx.lookup_complex_type(namespace, name).is_some(),
            ComponentKind::Element => idx.lookup_element(namespace, name).is_some(),
            ComponentKind::Attribute => idx.lookup_attribute(namespace, name).is_some(),
            ComponentKind::ModelGroup => idx.lookup_model_group(namespace, name).is_some(),
            ComponentKind::AttributeGroup => idx.lookup_attribute_group(namespace, name).is_some(),
            ComponentKind::Notation => idx.lookup_notation(namespace, name).is_some(),
            ComponentKind::IdentityConstraint => false,
        }
    })
}

/// Compute the target set of an override directive per XSD 1.1 §4.2.5.
///
/// The target set is the transitive closure of include + override edges
/// starting from the directly referenced document. Import and redefine
/// edges are excluded.
pub fn compute_target_set(schema_set: &SchemaSet, start_doc: DocumentId) -> HashSet<DocumentId> {
    let mut target_set = HashSet::new();
    let mut queue = vec![start_doc];

    while let Some(doc_id) = queue.pop() {
        if !target_set.insert(doc_id) {
            continue; // already visited
        }
        // Follow Include and Override edges from this document
        for edge in &schema_set.composition_edges {
            if edge.source_doc == doc_id {
                match edge.kind {
                    CompositionEdgeKind::Include | CompositionEdgeKind::Override => {
                        if let Some(target) = edge.target_doc {
                            if !target_set.contains(&target) {
                                queue.push(target);
                            }
                        }
                    }
                    _ => {} // Import and Redefine excluded from target set
                }
            }
        }
    }

    target_set
}

/// Apply an override directive to the schema set.
///
/// Computes the target set (transitive closure of include + override edges
/// from the directly referenced document per §4.2.5) and applies
/// replacements to all documents in the target set. Components not found
/// in any target set document are silently ignored.
pub fn apply_override(
    schema_set: &mut SchemaSet,
    override_dir: &OverrideDirective,
) -> SchemaResult<()> {
    let target_doc_id = override_dir.resolved_doc_id;
    let overriding_doc_id = override_dir.source.as_ref().map(|s| s.doc_id);

    // Compute the target set: transitive closure of include + override
    // edges from the directly referenced document.
    let target_set = match target_doc_id {
        Some(id) => compute_target_set(schema_set, id),
        None => HashSet::new(), // no target doc → unconditional replacement
    };

    // Apply the overridden document's (D2) schema-level defaults to override
    // children per F.2 semantics.  This covers two categories:
    //
    // 1. blockDefault / finalDefault — override children were assembled with
    //    empty assembler defaults (see convert_override in assemble.rs), so
    //    empty block/final means "not explicitly specified" and should adopt
    //    D2's blockDefault/finalDefault.
    //
    // 2. All other document-level defaults (elementFormDefault,
    //    attributeFormDefault, defaultAttributes) — set schema_defaults_doc
    //    on each component's SourceRef so downstream lookups read D2's
    //    document, not D1's.
    let (d2_block_default, d2_final_default) = target_doc_id
        .and_then(|id| schema_set.documents.get(id as usize))
        .map(|doc| (doc.block_default, doc.final_default))
        .unwrap_or((DerivationSet::empty(), DerivationSet::empty()));

    apply_d2_defaults(
        schema_set,
        &override_dir.components,
        d2_block_default,
        d2_final_default,
        target_doc_id,
    );

    for component in &override_dir.components {
        match component {
            OverrideComponent::SimpleType(key) => {
                override_simple_type(
                    schema_set,
                    *key,
                    target_doc_id,
                    &target_set,
                    overriding_doc_id,
                )?;
            }
            OverrideComponent::ComplexType(key) => {
                override_complex_type(
                    schema_set,
                    *key,
                    target_doc_id,
                    &target_set,
                    overriding_doc_id,
                )?;
            }
            OverrideComponent::Group(key) => {
                override_model_group(
                    schema_set,
                    *key,
                    target_doc_id,
                    &target_set,
                    overriding_doc_id,
                )?;
            }
            OverrideComponent::AttributeGroup(key) => {
                override_attribute_group(
                    schema_set,
                    *key,
                    target_doc_id,
                    &target_set,
                    overriding_doc_id,
                )?;
            }
            OverrideComponent::Element(key) => {
                override_element(
                    schema_set,
                    *key,
                    target_doc_id,
                    &target_set,
                    overriding_doc_id,
                )?;
            }
            OverrideComponent::Attribute(key) => {
                override_attribute(
                    schema_set,
                    *key,
                    target_doc_id,
                    &target_set,
                    overriding_doc_id,
                )?;
            }
            OverrideComponent::Notation(key) => {
                override_notation(
                    schema_set,
                    *key,
                    target_doc_id,
                    &target_set,
                    overriding_doc_id,
                )?;
            }
        }
    }
    Ok(())
}

/// Apply the overridden document's (D2) schema-level defaults to override
/// children per F.2 transformation semantics.
///
/// Handles two categories:
///
/// 1. **blockDefault / finalDefault** — override children are assembled with
///    empty assembler defaults (see `convert_override` in assemble.rs), so
///    empty block/final means "not explicitly specified" and should adopt
///    D2's values.
///
/// 2. **All other document-level defaults** (elementFormDefault,
///    attributeFormDefault, defaultAttributes) — sets `schema_defaults_doc`
///    on each component's `SourceRef` (and on nested content model elements)
///    so downstream lookups read D2's document settings, not D1's.
fn apply_d2_defaults(
    schema_set: &mut SchemaSet,
    components: &[OverrideComponent],
    d2_block_default: DerivationSet,
    d2_final_default: DerivationSet,
    d2_doc_id: Option<DocumentId>,
) {
    // Single mutation borrow through the cache-clearing gate for the whole batch.
    let arenas = schema_set.arenas.entries_mut();
    for component in components {
        match component {
            OverrideComponent::SimpleType(key) => {
                if let Some(st) = arenas.get_simple_type_mut(*key) {
                    if st.final_derivation.is_empty() {
                        st.final_derivation = d2_final_default;
                    }
                    set_defaults_doc(&mut st.source, d2_doc_id);
                }
            }
            OverrideComponent::ComplexType(key) => {
                if let Some(ct) = arenas.get_complex_type_mut(*key) {
                    if ct.final_derivation.is_empty() {
                        ct.final_derivation = d2_final_default;
                    }
                    if ct.block.is_empty() {
                        ct.block = d2_block_default;
                    }
                    set_defaults_doc(&mut ct.source, d2_doc_id);
                    // Walk content model to propagate to nested elements
                    set_defaults_doc_on_content(&mut ct.content, d2_doc_id);
                }
            }
            OverrideComponent::Element(key) => {
                if let Some(elem) = arenas.get_element_mut(*key) {
                    if elem.ref_name.is_none() {
                        if elem.block.is_empty() {
                            elem.block = d2_block_default;
                        }
                        if elem.final_derivation.is_empty() {
                            elem.final_derivation = d2_final_default;
                        }
                    }
                    set_defaults_doc(&mut elem.source, d2_doc_id);
                    if let Some(ref mut inline) = elem.inline_type {
                        set_defaults_doc_on_type_frame(inline, d2_doc_id);
                    }
                }
            }
            OverrideComponent::Attribute(key) => {
                if let Some(attr) = arenas.get_attribute_mut(*key) {
                    set_defaults_doc(&mut attr.source, d2_doc_id);
                }
            }
            OverrideComponent::Group(key) => {
                if let Some(group) = arenas.get_model_group_mut(*key) {
                    set_defaults_doc(&mut group.source, d2_doc_id);
                    set_defaults_doc_on_particles(&mut group.particles, d2_doc_id);
                }
            }
            OverrideComponent::AttributeGroup(key) => {
                if let Some(ag) = arenas.get_attribute_group_mut(*key) {
                    set_defaults_doc(&mut ag.source, d2_doc_id);
                }
            }
            OverrideComponent::Notation(key) => {
                if let Some(n) = arenas.get_notation_mut(*key) {
                    set_defaults_doc(&mut n.source, d2_doc_id);
                }
            }
        }
    }
}

/// Set `schema_defaults_doc` on a single source reference.
fn set_defaults_doc(
    source: &mut Option<crate::parser::location::SourceRef>,
    doc_id: Option<DocumentId>,
) {
    if let (Some(ref mut src), Some(id)) = (source, doc_id) {
        src.schema_defaults_doc = Some(id);
    }
}

/// Recursively set `schema_defaults_doc` on all element source refs within
/// a complex type's content model.  No-op when `doc_id` is `None`.
fn set_defaults_doc_on_content(content: &mut ComplexContentResult, doc_id: Option<DocumentId>) {
    if doc_id.is_none() {
        return;
    }
    if let ComplexContentResult::Complex(ref mut ccd) = content {
        if let Some(ref mut particle) = ccd.particle {
            set_defaults_doc_on_particle(particle, doc_id);
        }
    }
}

fn set_defaults_doc_on_particle(particle: &mut ParticleResult, doc_id: Option<DocumentId>) {
    match &mut particle.term {
        ParticleTerm::Element(ref mut elem) => {
            set_defaults_doc_on_element_frame(elem, doc_id);
        }
        ParticleTerm::Group(ref mut group) => {
            set_defaults_doc_on_particles(&mut group.particles, doc_id);
        }
        ParticleTerm::Any(_) => {}
    }
}

fn set_defaults_doc_on_particles(particles: &mut [ParticleResult], doc_id: Option<DocumentId>) {
    for p in particles {
        set_defaults_doc_on_particle(p, doc_id);
    }
}

fn set_defaults_doc_on_element_frame(elem: &mut ElementFrameResult, doc_id: Option<DocumentId>) {
    set_defaults_doc(&mut elem.source, doc_id);
    if let Some(ref mut inline) = elem.inline_type {
        set_defaults_doc_on_type_frame(inline, doc_id);
    }
}

fn set_defaults_doc_on_type_frame(type_frame: &mut TypeFrameResult, doc_id: Option<DocumentId>) {
    match type_frame {
        TypeFrameResult::Complex(ref mut ct) => {
            set_defaults_doc_on_complex_type_result(ct, doc_id);
        }
        TypeFrameResult::Simple(ref mut st) => {
            set_defaults_doc(&mut st.source, doc_id);
        }
    }
}

fn set_defaults_doc_on_complex_type_result(ct: &mut ComplexTypeResult, doc_id: Option<DocumentId>) {
    set_defaults_doc(&mut ct.source, doc_id);
    if let ComplexContentResult::Complex(ref mut ccd) = ct.content {
        if let Some(ref mut particle) = ccd.particle {
            set_defaults_doc_on_particle(particle, doc_id);
        }
    }
}

/// Check whether a component of the given kind exists in any document in
/// the target set. Returns `true` when the target set is empty (fallback:
/// unconditional replacement for pre-loaded schemas without resolution).
fn target_set_has_component<T>(
    schema_set: &SchemaSet,
    target_set: &HashSet<DocumentId>,
    lookup: impl Fn(&crate::schema::composition::DocumentComponentIndex) -> Option<T>,
) -> bool {
    if target_set.is_empty() {
        return true; // no target set → unconditional replacement
    }
    target_set.iter().any(|&doc_id| {
        schema_set
            .documents
            .get(doc_id as usize)
            .and_then(|doc| lookup(&doc.component_index))
            .is_some()
    })
}

/// Override a simple type.
fn override_simple_type(
    schema_set: &mut SchemaSet,
    new_key: SimpleTypeKey,
    target_doc_id: Option<DocumentId>,
    target_set: &HashSet<DocumentId>,
    overriding_doc_id: Option<DocumentId>,
) -> SchemaResult<()> {
    let new_type = schema_set
        .arenas
        .simple_types
        .get(new_key)
        .ok_or_else(|| SchemaError::internal("Override: simple type not found"))?;

    let name = new_type.name.ok_or_else(|| {
        SchemaError::structural(
            "src-override",
            "Overriding simple type must have a name",
            None,
        )
    })?;
    let namespace = new_type.target_namespace;

    if !target_set_has_component(schema_set, target_set, |idx| {
        idx.lookup_simple_type(namespace, name)
    }) {
        return Ok(()); // silently skip unmatched override per §4.2.5
    }

    let ns_table = schema_set.get_or_create_namespace(namespace);
    ns_table.register_type(name, TypeKey::Simple(new_key));
    record_provenance(
        &mut schema_set.effective_components,
        ComponentKey::Type(TypeKey::Simple(new_key)),
        ComponentKind::SimpleType,
        namespace,
        name,
        overriding_doc_id,
        overridden_action(
            overriding_doc_id,
            ComponentKind::SimpleType,
            name,
            namespace,
            target_doc_id,
        ),
    );

    Ok(())
}

/// Override a complex type.
fn override_complex_type(
    schema_set: &mut SchemaSet,
    new_key: ComplexTypeKey,
    target_doc_id: Option<DocumentId>,
    target_set: &HashSet<DocumentId>,
    overriding_doc_id: Option<DocumentId>,
) -> SchemaResult<()> {
    let new_type = schema_set
        .arenas
        .complex_types
        .get(new_key)
        .ok_or_else(|| SchemaError::internal("Override: complex type not found"))?;

    let name = new_type.name.ok_or_else(|| {
        SchemaError::structural(
            "src-override",
            "Overriding complex type must have a name",
            None,
        )
    })?;
    let namespace = new_type.target_namespace;

    if !target_set_has_component(schema_set, target_set, |idx| {
        idx.lookup_complex_type(namespace, name)
    }) {
        return Ok(());
    }

    let ns_table = schema_set.get_or_create_namespace(namespace);
    ns_table.register_type(name, TypeKey::Complex(new_key));
    record_provenance(
        &mut schema_set.effective_components,
        ComponentKey::Type(TypeKey::Complex(new_key)),
        ComponentKind::ComplexType,
        namespace,
        name,
        overriding_doc_id,
        overridden_action(
            overriding_doc_id,
            ComponentKind::ComplexType,
            name,
            namespace,
            target_doc_id,
        ),
    );

    Ok(())
}

/// Override a model group.
fn override_model_group(
    schema_set: &mut SchemaSet,
    new_key: ModelGroupKey,
    target_doc_id: Option<DocumentId>,
    target_set: &HashSet<DocumentId>,
    overriding_doc_id: Option<DocumentId>,
) -> SchemaResult<()> {
    let new_group = schema_set
        .arenas
        .model_groups
        .get(new_key)
        .ok_or_else(|| SchemaError::internal("Override: model group not found"))?;

    let name = new_group.name.ok_or_else(|| {
        SchemaError::structural(
            "src-override",
            "Overriding model group must have a name",
            None,
        )
    })?;
    let namespace = new_group.target_namespace;

    if !target_set_has_component(schema_set, target_set, |idx| {
        idx.lookup_model_group(namespace, name)
    }) {
        return Ok(());
    }

    let ns_table = schema_set.get_or_create_namespace(namespace);
    ns_table.register_model_group(name, new_key);
    record_provenance(
        &mut schema_set.effective_components,
        ComponentKey::ModelGroup(new_key),
        ComponentKind::ModelGroup,
        namespace,
        name,
        overriding_doc_id,
        overridden_action(
            overriding_doc_id,
            ComponentKind::ModelGroup,
            name,
            namespace,
            target_doc_id,
        ),
    );

    Ok(())
}

/// Override an attribute group.
fn override_attribute_group(
    schema_set: &mut SchemaSet,
    new_key: AttributeGroupKey,
    target_doc_id: Option<DocumentId>,
    target_set: &HashSet<DocumentId>,
    overriding_doc_id: Option<DocumentId>,
) -> SchemaResult<()> {
    let new_group = schema_set
        .arenas
        .attribute_groups
        .get(new_key)
        .ok_or_else(|| SchemaError::internal("Override: attribute group not found"))?;

    let name = new_group.name.ok_or_else(|| {
        SchemaError::structural(
            "src-override",
            "Overriding attribute group must have a name",
            None,
        )
    })?;
    let namespace = new_group.target_namespace;

    if !target_set_has_component(schema_set, target_set, |idx| {
        idx.lookup_attribute_group(namespace, name)
    }) {
        return Ok(());
    }

    let ns_table = schema_set.get_or_create_namespace(namespace);
    ns_table.register_attribute_group(name, new_key);
    record_provenance(
        &mut schema_set.effective_components,
        ComponentKey::AttributeGroup(new_key),
        ComponentKind::AttributeGroup,
        namespace,
        name,
        overriding_doc_id,
        overridden_action(
            overriding_doc_id,
            ComponentKind::AttributeGroup,
            name,
            namespace,
            target_doc_id,
        ),
    );

    Ok(())
}

/// Override an element.
fn override_element(
    schema_set: &mut SchemaSet,
    new_key: ElementKey,
    target_doc_id: Option<DocumentId>,
    target_set: &HashSet<DocumentId>,
    overriding_doc_id: Option<DocumentId>,
) -> SchemaResult<()> {
    let new_elem = schema_set
        .arenas
        .elements
        .get(new_key)
        .ok_or_else(|| SchemaError::internal("Override: element not found"))?;

    let name = new_elem.name.ok_or_else(|| {
        SchemaError::structural("src-override", "Overriding element must have a name", None)
    })?;
    let namespace = new_elem.target_namespace;

    if !target_set_has_component(schema_set, target_set, |idx| {
        idx.lookup_element(namespace, name)
    }) {
        return Ok(());
    }

    let ns_table = schema_set.get_or_create_namespace(namespace);
    ns_table.register_element(name, new_key);
    record_provenance(
        &mut schema_set.effective_components,
        ComponentKey::Element(new_key),
        ComponentKind::Element,
        namespace,
        name,
        overriding_doc_id,
        overridden_action(
            overriding_doc_id,
            ComponentKind::Element,
            name,
            namespace,
            target_doc_id,
        ),
    );

    Ok(())
}

/// Override an attribute.
fn override_attribute(
    schema_set: &mut SchemaSet,
    new_key: AttributeKey,
    target_doc_id: Option<DocumentId>,
    target_set: &HashSet<DocumentId>,
    overriding_doc_id: Option<DocumentId>,
) -> SchemaResult<()> {
    let new_attr = schema_set
        .arenas
        .attributes
        .get(new_key)
        .ok_or_else(|| SchemaError::internal("Override: attribute not found"))?;

    let name = new_attr.name.ok_or_else(|| {
        SchemaError::structural(
            "src-override",
            "Overriding attribute must have a name",
            None,
        )
    })?;
    let namespace = new_attr.target_namespace;

    if !target_set_has_component(schema_set, target_set, |idx| {
        idx.lookup_attribute(namespace, name)
    }) {
        return Ok(());
    }

    let ns_table = schema_set.get_or_create_namespace(namespace);
    ns_table.register_attribute(name, new_key);
    record_provenance(
        &mut schema_set.effective_components,
        ComponentKey::Attribute(new_key),
        ComponentKind::Attribute,
        namespace,
        name,
        overriding_doc_id,
        overridden_action(
            overriding_doc_id,
            ComponentKind::Attribute,
            name,
            namespace,
            target_doc_id,
        ),
    );

    Ok(())
}

/// Override a notation.
fn override_notation(
    schema_set: &mut SchemaSet,
    new_key: NotationKey,
    target_doc_id: Option<DocumentId>,
    target_set: &HashSet<DocumentId>,
    overriding_doc_id: Option<DocumentId>,
) -> SchemaResult<()> {
    let new_notation = schema_set
        .arenas
        .notations
        .get(new_key)
        .ok_or_else(|| SchemaError::internal("Override: notation not found"))?;

    let name = new_notation.name;
    let namespace = new_notation.target_namespace;

    if !target_set_has_component(schema_set, target_set, |idx| {
        idx.lookup_notation(namespace, name)
    }) {
        return Ok(());
    }

    let ns_table = schema_set.get_or_create_namespace(namespace);
    ns_table.register_notation(name, new_key);
    record_provenance(
        &mut schema_set.effective_components,
        ComponentKey::Notation(new_key),
        ComponentKind::Notation,
        namespace,
        name,
        overriding_doc_id,
        overridden_action(
            overriding_doc_id,
            ComponentKind::Notation,
            name,
            namespace,
            target_doc_id,
        ),
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse::parse_schema;
    use crate::schema::model::OverrideDirective;

    /// When resolved_doc_id is Some and the target document does NOT
    /// declare the overridden component, the override must be silently
    /// skipped — it must NOT clobber whatever is in the global namespace
    /// table from another document.
    #[test]
    fn test_override_skips_unmatched_component() {
        let tmp = std::env::temp_dir().join("xsd_test_override_skip_unmatched");
        std::fs::create_dir_all(&tmp).unwrap();

        // doc_a.xsd declares MyElem
        let doc_a_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
    <xs:element name="MyElem" type="xs:string"/>
</xs:schema>"#;
        let doc_a_path = tmp.join("ovr_skip_a.xsd");
        std::fs::write(&doc_a_path, doc_a_xsd).unwrap();

        // doc_b.xsd does NOT declare MyElem
        let doc_b_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
    <xs:element name="OtherElem" type="xs:integer"/>
</xs:schema>"#;
        let doc_b_path = tmp.join("ovr_skip_b.xsd");
        std::fs::write(&doc_b_path, doc_b_xsd).unwrap();

        // Parse both documents
        let mut schema_set = SchemaSet::new();
        let _doc_a_id = parse_schema(
            std::fs::read_to_string(&doc_a_path).unwrap().as_bytes(),
            &doc_a_path.to_string_lossy(),
            &mut schema_set,
        )
        .unwrap();
        let doc_b_id = parse_schema(
            std::fs::read_to_string(&doc_b_path).unwrap().as_bytes(),
            &doc_b_path.to_string_lossy(),
            &mut schema_set,
        )
        .unwrap();

        // Capture the original global entry for MyElem (from doc_a)
        let my_elem_name = schema_set.name_table.get("MyElem").unwrap();
        let original_key = schema_set
            .lookup_element(None, my_elem_name)
            .expect("MyElem should be in global namespace table from doc_a");

        // Create a replacement element in the arena
        let replacement_key = schema_set
            .arenas
            .alloc_element(crate::arenas::ElementDeclData {
                name: Some(my_elem_name),
                target_namespace: None,
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
                block: crate::schema::model::DerivationSet::empty(),
                final_derivation: crate::schema::model::DerivationSet::empty(),
                form: None,
                id: None,
                alternatives: Vec::new(),
                identity_constraints: Vec::new(),
                pending_ic_refs: vec![],
                annotation: None,
                source: None,
                resolved_type: None,
                resolved_ref: None,
                resolved_substitution_groups: Vec::new(),
                deferred_type_error: None,
            });

        // Override targeting doc_b (which does NOT have MyElem)
        let override_dir = OverrideDirective {
            source: None,
            schema_location: doc_b_path.to_string_lossy().to_string(),
            resolved_doc_id: Some(doc_b_id),
            components: vec![OverrideComponent::Element(replacement_key)],
        };

        // Apply override — should succeed (silently skip unmatched)
        apply_override(&mut schema_set, &override_dir).unwrap();

        // The global namespace table must still have the ORIGINAL key from doc_a,
        // NOT the replacement — the override was skipped.
        let current_key = schema_set
            .lookup_element(None, my_elem_name)
            .expect("MyElem should still be in namespace table");
        assert_eq!(
            current_key, original_key,
            "Override must not clobber global entry when target document lacks the component"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Override with transitive target set: base.xsd includes helper.xsd,
    /// main.xsd overrides from base.xsd. The override should be able to
    /// target components declared in helper.xsd (via the transitive
    /// include + override closure).
    #[test]
    fn test_override_transitive_target_set() {
        use crate::parser::parse::parse_schema;
        use crate::parser::resolver::{resolve_all_directives, SchemaResolver};

        let tmp = std::env::temp_dir().join("xsd_test_override_transitive");
        std::fs::create_dir_all(&tmp).unwrap();

        // helper.xsd declares MyElem
        let helper_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
    <xs:element name="MyElem" type="xs:string"/>
</xs:schema>"#;
        let helper_path = tmp.join("ovr_helper.xsd");
        std::fs::write(&helper_path, helper_xsd).unwrap();

        // base.xsd includes helper.xsd
        let base_xsd = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
    <xs:include schemaLocation="{}"/>
    <xs:element name="BaseElem" type="xs:integer"/>
</xs:schema>"#,
            helper_path.to_string_lossy()
        );
        let base_path = tmp.join("ovr_base.xsd");
        std::fs::write(&base_path, &base_xsd).unwrap();

        // main.xsd overrides from base.xsd, replacing MyElem
        // (which is declared in helper.xsd, not base.xsd directly)
        let main_xsd = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
    <xs:override schemaLocation="{}">
        <xs:element name="MyElem" type="xs:integer"/>
    </xs:override>
</xs:schema>"#,
            base_path.to_string_lossy()
        );

        // Must use XSD 1.1 for xs:override support
        let mut schema_set = SchemaSet::xsd11();
        let main_path = tmp.join("ovr_main.xsd").to_string_lossy().to_string();
        let doc_id = parse_schema(main_xsd.as_bytes(), &main_path, &mut schema_set).unwrap();

        let mut resolver = SchemaResolver::new();
        // Resolve main → loads base → loads helper
        let result = resolve_all_directives(doc_id, &mut resolver, &mut schema_set);
        assert!(result.is_ok(), "Resolution should succeed");

        // Resolve base's directives (loads helper via include)
        for loaded_id in result.loaded.clone() {
            let nested = resolve_all_directives(loaded_id, &mut resolver, &mut schema_set);
            assert!(nested.is_ok());
            for nested_id in nested.loaded {
                let _ = resolve_all_directives(nested_id, &mut resolver, &mut schema_set);
            }
        }

        // Verify the target set includes both base and helper
        let main_doc = &schema_set.documents[doc_id as usize];
        let override_target = main_doc.overrides[0].resolved_doc_id;
        assert!(
            override_target.is_some(),
            "Override should have resolved_doc_id"
        );

        let target_set = compute_target_set(&schema_set, override_target.unwrap());
        assert!(
            target_set.len() >= 2,
            "Target set should include base and helper, got {} documents",
            target_set.len()
        );

        // Apply composition — override should replace MyElem even though
        // it's declared in helper.xsd (transitive target)
        crate::schema::apply_redefine_override(&mut schema_set).unwrap();

        // MyElem should still exist in namespace table (replaced, not removed)
        let my_elem_name = schema_set.name_table.get("MyElem").unwrap();
        assert!(
            schema_set.lookup_element(None, my_elem_name).is_some(),
            "MyElem should be in namespace table after transitive override"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Override children must use the overridden document's (D2) blockDefault
    /// and finalDefault, not the overriding document's (D1).
    ///
    /// D1 has blockDefault="#all", D2 has blockDefault="restriction".
    /// An override child element with no explicit `block` must get
    /// D2's "restriction", not D1's "#all".
    #[test]
    fn test_override_uses_d2_block_default() {
        use crate::parser::parse::parse_schema;
        use crate::parser::resolver::{resolve_all_directives, SchemaResolver};

        let tmp = std::env::temp_dir().join("xsd_test_override_d2_block");
        std::fs::create_dir_all(&tmp).unwrap();

        // target.xsd (D2): blockDefault="restriction"
        let target_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           blockDefault="restriction">
    <xs:element name="Foo" type="xs:string"/>
</xs:schema>"#;
        let target_path = tmp.join("ovr_d2_block_target.xsd");
        std::fs::write(&target_path, target_xsd).unwrap();

        // main.xsd (D1): blockDefault="#all", overrides Foo from target
        let main_xsd = format!(
            r##"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           blockDefault="#all">
    <xs:override schemaLocation="{}">
        <xs:element name="Foo" type="xs:integer"/>
    </xs:override>
</xs:schema>"##,
            target_path.to_string_lossy()
        );

        let mut schema_set = SchemaSet::xsd11();
        let main_path = tmp
            .join("ovr_d1_block_main.xsd")
            .to_string_lossy()
            .to_string();
        let doc_id = parse_schema(main_xsd.as_bytes(), &main_path, &mut schema_set).unwrap();

        let mut resolver = SchemaResolver::new();
        let result = resolve_all_directives(doc_id, &mut resolver, &mut schema_set);
        assert!(result.is_ok(), "Resolution should succeed");
        for loaded_id in result.loaded.clone() {
            let _ = resolve_all_directives(loaded_id, &mut resolver, &mut schema_set);
        }

        // Apply composition
        crate::schema::apply_redefine_override(&mut schema_set).unwrap();

        // Look up the resulting Foo element
        let foo_name = schema_set.name_table.get("Foo").unwrap();
        let foo_key = schema_set
            .lookup_element(None, foo_name)
            .expect("Foo should be in namespace table after override");
        let foo = schema_set.arenas.elements.get(foo_key).unwrap();

        // The override child had no explicit block, so it must have D2's
        // blockDefault="restriction", NOT D1's blockDefault="#all".
        assert!(
            foo.block.contains(DerivationSet::RESTRICTION),
            "Override element should have D2's blockDefault (restriction), got {:?}",
            foo.block
        );
        assert!(
            !foo.block.contains(DerivationSet::EXTENSION),
            "Override element should NOT have D1's blockDefault (#all which includes extension), got {:?}",
            foo.block
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Override children must have `schema_defaults_doc` set to D2's doc_id
    /// so that downstream lookups for elementFormDefault, attributeFormDefault,
    /// and defaultAttributes read from D2, not D1.
    #[test]
    fn test_override_schema_defaults_doc_set_to_d2() {
        use crate::parser::parse::parse_schema;
        use crate::parser::resolver::{resolve_all_directives, SchemaResolver};

        let tmp = std::env::temp_dir().join("xsd_test_override_defaults_doc");
        std::fs::create_dir_all(&tmp).unwrap();

        // target.xsd (D2): elementFormDefault="qualified"
        let target_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           elementFormDefault="qualified">
    <xs:complexType name="MyType">
        <xs:sequence>
            <xs:element name="child" type="xs:string"/>
        </xs:sequence>
    </xs:complexType>
</xs:schema>"#;
        let target_path = tmp.join("ovr_defaults_target.xsd");
        std::fs::write(&target_path, target_xsd).unwrap();

        // main.xsd (D1): elementFormDefault="unqualified", overrides MyType
        let main_xsd = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           elementFormDefault="unqualified">
    <xs:override schemaLocation="{}">
        <xs:complexType name="MyType">
            <xs:sequence>
                <xs:element name="child" type="xs:integer"/>
            </xs:sequence>
        </xs:complexType>
    </xs:override>
</xs:schema>"#,
            target_path.to_string_lossy()
        );

        let mut schema_set = SchemaSet::xsd11();
        let main_path = tmp
            .join("ovr_defaults_main.xsd")
            .to_string_lossy()
            .to_string();
        let doc_id = parse_schema(main_xsd.as_bytes(), &main_path, &mut schema_set).unwrap();

        let mut resolver = SchemaResolver::new();
        let result = resolve_all_directives(doc_id, &mut resolver, &mut schema_set);
        assert!(result.is_ok(), "Resolution should succeed");
        for loaded_id in result.loaded.clone() {
            let _ = resolve_all_directives(loaded_id, &mut resolver, &mut schema_set);
        }

        // Get D2's doc_id from the override's resolved_doc_id
        let main_doc = &schema_set.documents[doc_id as usize];
        let d2_doc_id = main_doc.overrides[0]
            .resolved_doc_id
            .expect("Override should have resolved_doc_id");

        // Apply composition
        crate::schema::apply_redefine_override(&mut schema_set).unwrap();

        // Look up the resulting MyType complex type
        let my_type_name = schema_set.name_table.get("MyType").unwrap();
        let type_key = schema_set
            .lookup_type(None, my_type_name)
            .expect("MyType should be in namespace table");

        // Get the complex type and verify schema_defaults_doc points to D2
        if let TypeKey::Complex(ct_key) = type_key {
            let ct = schema_set.arenas.complex_types.get(ct_key).unwrap();
            let src = ct.source.as_ref().expect("Complex type should have source");
            assert_eq!(
                src.schema_defaults_doc,
                Some(d2_doc_id),
                "Override complex type's schema_defaults_doc should be D2's doc_id"
            );
            assert_eq!(
                src.defaults_doc(),
                d2_doc_id,
                "defaults_doc() should return D2's doc_id"
            );

            // Verify that D2 has elementFormDefault=qualified
            let d2 = &schema_set.documents[d2_doc_id as usize];
            assert_eq!(
                d2.element_form_default,
                crate::schema::model::FormChoice::Qualified,
                "D2 should have elementFormDefault=qualified"
            );

            // Verify inline child element also has schema_defaults_doc set
            if let ComplexContentResult::Complex(ref ccd) = ct.content {
                if let Some(ref particle) = ccd.particle {
                    match &particle.term {
                        ParticleTerm::Group(group) => {
                            // sequence/all/choice wraps elements in a group
                            for p in &group.particles {
                                if let ParticleTerm::Element(ref elem) = p.term {
                                    let elem_src = elem
                                        .source
                                        .as_ref()
                                        .expect("Inline element should have source");
                                    assert_eq!(
                                        elem_src.schema_defaults_doc, Some(d2_doc_id),
                                        "Inline element within override complex type should have schema_defaults_doc = D2"
                                    );
                                }
                            }
                        }
                        ParticleTerm::Element(ref elem) => {
                            let elem_src = elem
                                .source
                                .as_ref()
                                .expect("Inline element should have source");
                            assert_eq!(
                                elem_src.schema_defaults_doc, Some(d2_doc_id),
                                "Inline element within override should have schema_defaults_doc = D2"
                            );
                        }
                        _ => {}
                    }
                }
            }
        } else {
            panic!("MyType should be a complex type");
        }

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
