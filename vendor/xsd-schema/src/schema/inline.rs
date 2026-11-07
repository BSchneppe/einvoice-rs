//! Inline type assembly pass
//!
//! This module implements Phase 3 inline type resolution as specified in
//! XSD_INLINE_TYPE_RESOLUTION_DESIGN.md. It assembles inline type definitions
//! (TypeRefResult::Inline and inline_type fields) into arena TypeKey values.
//!
//! # Overview
//!
//! Inline types are anonymous type definitions that appear directly within
//! element declarations, attribute declarations, or as base/item/member types
//! in type derivation. These need to be allocated in the type arenas before
//! the reference resolution phase can complete.
//!
//! # Work Queue Approach
//!
//! To avoid borrow conflicts and handle nested inline types, we use a two-pass
//! work queue approach:
//!
//! 1. **Scan Pass**: Collect all inline types into `InlineTypeJob` records.
//!    Each job records the owner component and the inline type AST (cloned).
//!
//! 2. **Assembly Pass**: For each job, assemble the inline type into the
//!    arena and update the owner's resolved_* field with the TypeKey.

use crate::arenas::{
    ComplexTypeDefData, ElementDeclData, IdentityConstraintData, ResolvedAttributeUse,
    SimpleTypeDefData,
};
use std::collections::{HashMap, HashSet};

use crate::error::{SchemaError, SchemaResult};
use crate::ids::*;
use crate::parser::frames::{
    ComplexContentResult, ElementFrameResult, IdentityKind, ParticleResult, ParticleTerm, QNameRef,
    TypeFrameResult, TypeRefResult,
};
use crate::parser::location::SourceRef;
use crate::schema::model::DerivationSet;
use crate::schema::SchemaSet;

/// Resolve a parsed `block`/`final` attribute to an effective `DerivationSet`.
/// `Some(set)` → explicit (including `""` = empty, which overrides the default).
/// `None` → absent; inherit the source document's default via `pick_default`,
/// following `schema_defaults_doc` for override-copied components so they see
/// the override document's defaults rather than the origin schema's.
fn resolve_derivation_default(
    parsed: Option<DerivationSet>,
    source: Option<&SourceRef>,
    schema_set: &SchemaSet,
    pick_default: impl Fn(&crate::schema::model::SchemaDocument) -> DerivationSet,
) -> DerivationSet {
    parsed.unwrap_or_else(|| {
        source
            .and_then(|s| {
                let doc_id = s.schema_defaults_doc.unwrap_or(s.doc_id);
                schema_set.documents.get(doc_id as usize)
            })
            .map(pick_default)
            .unwrap_or_default()
    })
}

fn resolve_block(
    block: Option<DerivationSet>,
    source: Option<&SourceRef>,
    schema_set: &SchemaSet,
) -> DerivationSet {
    resolve_derivation_default(block, source, schema_set, |d| d.block_default)
}

fn resolve_final(
    final_derivation: Option<DerivationSet>,
    source: Option<&SourceRef>,
    schema_set: &SchemaSet,
) -> DerivationSet {
    resolve_derivation_default(final_derivation, source, schema_set, |d| d.final_default)
}

/// Statistics from the inline type assembly pass
#[derive(Debug, Default)]
pub struct InlineAssemblyStats {
    /// Number of element inline types assembled
    pub element_inline_types: usize,
    /// Number of attribute inline types assembled
    pub attribute_inline_types: usize,
    /// Number of simple type base/item/member inline types assembled
    pub simple_type_inline_derivations: usize,
    /// Number of complex type base type inline types assembled
    pub complex_type_inline_derivations: usize,
    /// Number of inline types in model group particles assembled
    pub model_group_inline_types: usize,
    /// Number of inline types in attribute groups assembled
    pub attribute_group_inline_types: usize,
    /// Number of inline types in complex type attributes assembled
    pub complex_type_attribute_inline_types: usize,
    /// Total inline types assembled
    pub total_inline_types: usize,
}

/// Role of an inline type within its owner
#[derive(Debug, Clone, Copy)]
enum InlineRole {
    /// Element's type (inline_type field)
    ElementType,
    /// Attribute's type (inline_type field)
    AttributeType,
    /// Simple type restriction base (base_type: Inline)
    SimpleTypeBase,
    /// List type item type (item_type: Inline)
    ListItemType,
    /// Union member type (member_types[index]: Inline)
    UnionMemberType(usize),
    /// Complex type derivation base (base_type: Inline)
    ComplexTypeBase,
    /// Attribute use inline type within complex type (attributes[index])
    ComplexTypeAttribute(usize),
    /// Particle inline type in model group (particles[index])
    ModelGroupParticle(usize),
    /// Attribute use inline type in attribute group (attributes[index])
    AttributeGroupAttribute(usize),
    /// Inline type on element at index in content particle's model group
    ContentParticleType(usize),
    /// Inline type on alternative at index in element's alternatives vec (XSD 1.1)
    #[cfg(feature = "xsd11")]
    AlternativeType(usize),
    /// Inline `<xs:simpleType>` inside simpleContent/restriction
    /// (§3.4.2.2 clause 1.1 — the B simple type definition)
    SimpleContentInlineType,
}

/// Owner of an inline type
#[derive(Debug, Clone, Copy)]
enum InlineOwner {
    Element(ElementKey),
    Attribute(AttributeKey),
    SimpleType(SimpleTypeKey),
    ComplexType(ComplexTypeKey),
    ModelGroup(ModelGroupKey),
    AttributeGroup(AttributeGroupKey),
}

/// A job representing an inline type to be assembled
struct InlineTypeJob {
    owner: InlineOwner,
    role: InlineRole,
    type_frame: TypeFrameResult,
    target_namespace: Option<NameId>,
}

/// Assemble all inline types in the schema set
///
/// This function walks all components, collects inline type definitions,
/// assembles them into the arenas, and updates the resolved_* fields.
///
/// This should be called after top-level component assembly but before
/// reference resolution.
pub fn assemble_inline_types(schema_set: &mut SchemaSet) -> SchemaResult<InlineAssemblyStats> {
    let mut stats = InlineAssemblyStats::default();

    // Collect all inline type jobs
    let mut jobs = collect_inline_type_jobs(schema_set);

    // Process jobs iteratively (nested inline types may add more jobs)
    while !jobs.is_empty() {
        let current_jobs: Vec<_> = std::mem::take(&mut jobs);

        for job in current_jobs {
            let type_key = assemble_inline_type(schema_set, &job.type_frame, job.target_namespace)?;
            update_owner(schema_set, &job, type_key, &mut stats)?;

            // Check if the newly assembled type has nested inline types
            collect_nested_inline_types(schema_set, type_key, job.target_namespace, &mut jobs);
        }
    }

    Ok(stats)
}

/// Collect all inline type jobs from schema components
fn collect_inline_type_jobs(schema_set: &SchemaSet) -> Vec<InlineTypeJob> {
    let mut jobs = Vec::new();

    // Scan elements
    for (key, elem) in schema_set.arenas.elements.iter() {
        let target_ns = elem.target_namespace;
        if let Some(inline_type) = &elem.inline_type {
            jobs.push(InlineTypeJob {
                owner: InlineOwner::Element(key),
                role: InlineRole::ElementType,
                type_frame: (**inline_type).clone(),
                target_namespace: target_ns,
            });
        }
    }

    // Scan element alternatives (XSD 1.1)
    #[cfg(feature = "xsd11")]
    for (key, elem) in schema_set.arenas.elements.iter() {
        let target_ns = elem.target_namespace;
        for (idx, alt) in elem.alternatives.iter().enumerate() {
            if let Some(inline_type) = &alt.inline_type {
                jobs.push(InlineTypeJob {
                    owner: InlineOwner::Element(key),
                    role: InlineRole::AlternativeType(idx),
                    type_frame: (**inline_type).clone(),
                    target_namespace: target_ns,
                });
            }
        }
    }

    // Scan attributes
    for (key, attr) in schema_set.arenas.attributes.iter() {
        let target_ns = attr.target_namespace;
        if let Some(inline_type) = &attr.inline_type {
            jobs.push(InlineTypeJob {
                owner: InlineOwner::Attribute(key),
                role: InlineRole::AttributeType,
                type_frame: TypeFrameResult::Simple(Box::new((**inline_type).clone())),
                target_namespace: target_ns,
            });
        }
    }

    // Scan simple types
    for (key, simple) in schema_set.arenas.simple_types.iter() {
        let target_ns = simple.target_namespace;

        // Base type inline
        if let Some(TypeRefResult::Inline(inline_type)) = &simple.base_type {
            jobs.push(InlineTypeJob {
                owner: InlineOwner::SimpleType(key),
                role: InlineRole::SimpleTypeBase,
                type_frame: (**inline_type).clone(),
                target_namespace: target_ns,
            });
        }

        // Item type inline (for list types)
        if let Some(TypeRefResult::Inline(inline_type)) = &simple.item_type {
            jobs.push(InlineTypeJob {
                owner: InlineOwner::SimpleType(key),
                role: InlineRole::ListItemType,
                type_frame: (**inline_type).clone(),
                target_namespace: target_ns,
            });
        }

        // Member types inline (for union types)
        for (idx, member) in simple.member_types.iter().enumerate() {
            if let TypeRefResult::Inline(inline_type) = member {
                jobs.push(InlineTypeJob {
                    owner: InlineOwner::SimpleType(key),
                    role: InlineRole::UnionMemberType(idx),
                    type_frame: (**inline_type).clone(),
                    target_namespace: target_ns,
                });
            }
        }
    }

    // Scan complex types
    for (key, complex) in schema_set.arenas.complex_types.iter() {
        let target_ns = complex.target_namespace;

        // Base type inline
        if let Some(TypeRefResult::Inline(inline_type)) = &complex.base_type {
            jobs.push(InlineTypeJob {
                owner: InlineOwner::ComplexType(key),
                role: InlineRole::ComplexTypeBase,
                type_frame: (**inline_type).clone(),
                target_namespace: target_ns,
            });
        }

        // Attribute uses with inline types
        for (idx, attr_use) in complex.attributes.iter().enumerate() {
            if let Some(inline_type) = &attr_use.attribute.inline_type {
                jobs.push(InlineTypeJob {
                    owner: InlineOwner::ComplexType(key),
                    role: InlineRole::ComplexTypeAttribute(idx),
                    type_frame: TypeFrameResult::Simple(Box::new((**inline_type).clone())),
                    target_namespace: target_ns,
                });
            }
        }

        // Content particles (in complex content)
        collect_content_inline_types(&complex.content, key, target_ns, &mut jobs);
    }

    // Scan model groups (recursively, including nested inline groups)
    for (key, group) in schema_set.arenas.model_groups.iter() {
        let target_ns = group.target_namespace;
        let mut flat_idx = 0;
        collect_model_group_inline_types_recursive(
            &group.particles,
            key,
            target_ns,
            &mut flat_idx,
            &mut jobs,
        );
    }

    // Scan attribute groups
    for (key, group) in schema_set.arenas.attribute_groups.iter() {
        let target_ns = group.target_namespace;

        for (idx, attr_use) in group.attributes.iter().enumerate() {
            if let Some(inline_type) = &attr_use.attribute.inline_type {
                jobs.push(InlineTypeJob {
                    owner: InlineOwner::AttributeGroup(key),
                    role: InlineRole::AttributeGroupAttribute(idx),
                    type_frame: TypeFrameResult::Simple(Box::new((**inline_type).clone())),
                    target_namespace: target_ns,
                });
            }
        }
    }

    jobs
}

/// Collect inline types from complex content (recursive helper)
fn collect_content_inline_types(
    content: &ComplexContentResult,
    owner_key: ComplexTypeKey,
    target_ns: Option<NameId>,
    jobs: &mut Vec<InlineTypeJob>,
) {
    match content {
        ComplexContentResult::Complex(complex_content) => {
            // Check base type inline
            if let Some(TypeRefResult::Inline(inline_type)) = &complex_content.base_type {
                jobs.push(InlineTypeJob {
                    owner: InlineOwner::ComplexType(owner_key),
                    role: InlineRole::ComplexTypeBase,
                    type_frame: (**inline_type).clone(),
                    target_namespace: target_ns,
                });
            }

            // Check attributes in complex content
            for (idx, attr_use) in complex_content.attributes.iter().enumerate() {
                if let Some(inline_type) = &attr_use.attribute.inline_type {
                    jobs.push(InlineTypeJob {
                        owner: InlineOwner::ComplexType(owner_key),
                        role: InlineRole::ComplexTypeAttribute(idx),
                        type_frame: TypeFrameResult::Simple(Box::new((**inline_type).clone())),
                        target_namespace: target_ns,
                    });
                }
            }

            // Scan content particle for inline types on element children
            if let Some(particle) = &complex_content.particle {
                collect_particle_inline_types(particle, owner_key, target_ns, jobs);
            }
        }
        ComplexContentResult::Simple(simple_content) => {
            // Check base type inline
            if let Some(TypeRefResult::Inline(inline_type)) = &simple_content.base_type {
                jobs.push(InlineTypeJob {
                    owner: InlineOwner::ComplexType(owner_key),
                    role: InlineRole::ComplexTypeBase,
                    type_frame: (**inline_type).clone(),
                    target_namespace: target_ns,
                });
            }

            // Check attributes in simple content
            for (idx, attr_use) in simple_content.attributes.iter().enumerate() {
                if let Some(inline_type) = &attr_use.attribute.inline_type {
                    jobs.push(InlineTypeJob {
                        owner: InlineOwner::ComplexType(owner_key),
                        role: InlineRole::ComplexTypeAttribute(idx),
                        type_frame: TypeFrameResult::Simple(Box::new((**inline_type).clone())),
                        target_namespace: target_ns,
                    });
                }
            }

            // §3.4.2.2 clause 1.1: inline <xs:simpleType> inside
            // simpleContent/restriction is the content type definition B.
            if let Some(inline_st) = &simple_content.content_type {
                jobs.push(InlineTypeJob {
                    owner: InlineOwner::ComplexType(owner_key),
                    role: InlineRole::SimpleContentInlineType,
                    type_frame: TypeFrameResult::Simple(Box::new((**inline_st).clone())),
                    target_namespace: target_ns,
                });
            }
        }
        ComplexContentResult::Empty => {}
    }
}

/// Collect inline types from a content particle's model group elements (recursive)
fn collect_particle_inline_types(
    particle: &ParticleResult,
    owner_key: ComplexTypeKey,
    target_ns: Option<NameId>,
    jobs: &mut Vec<InlineTypeJob>,
) {
    if let ParticleTerm::Group(group_def) = &particle.term {
        let mut flat_idx = 0;
        collect_group_elements_recursive(
            &group_def.particles,
            owner_key,
            target_ns,
            &mut flat_idx,
            jobs,
        );
    }
}

/// Recursive helper: walk particles in depth-first order, assigning flat element indices
fn collect_group_elements_recursive(
    particles: &[ParticleResult],
    owner_key: ComplexTypeKey,
    target_ns: Option<NameId>,
    flat_idx: &mut usize,
    jobs: &mut Vec<InlineTypeJob>,
) {
    for particle in particles {
        match &particle.term {
            ParticleTerm::Element(elem) => {
                if let Some(inline_type) = &elem.inline_type {
                    jobs.push(InlineTypeJob {
                        owner: InlineOwner::ComplexType(owner_key),
                        role: InlineRole::ContentParticleType(*flat_idx),
                        type_frame: (**inline_type).clone(),
                        target_namespace: target_ns,
                    });
                }
                *flat_idx += 1;
            }
            ParticleTerm::Group(group_def) if group_def.ref_name.is_none() => {
                // Inline group (no ref) - recurse into its particles
                collect_group_elements_recursive(
                    &group_def.particles,
                    owner_key,
                    target_ns,
                    flat_idx,
                    jobs,
                );
            }
            _ => {} // Skip group refs and wildcards
        }
    }
}

/// Recursive helper: walk model group particles in depth-first order, collecting inline types
/// with flat element indices (mirroring collect_group_elements_recursive for complex types)
fn collect_model_group_inline_types_recursive(
    particles: &[ParticleResult],
    owner_key: ModelGroupKey,
    target_ns: Option<NameId>,
    flat_idx: &mut usize,
    jobs: &mut Vec<InlineTypeJob>,
) {
    for particle in particles {
        match &particle.term {
            ParticleTerm::Element(elem) => {
                if let Some(inline_type) = &elem.inline_type {
                    jobs.push(InlineTypeJob {
                        owner: InlineOwner::ModelGroup(owner_key),
                        role: InlineRole::ModelGroupParticle(*flat_idx),
                        type_frame: (**inline_type).clone(),
                        target_namespace: target_ns,
                    });
                }
                *flat_idx += 1;
            }
            ParticleTerm::Group(group_def) if group_def.ref_name.is_none() => {
                // Inline group (no ref) - recurse into its particles
                collect_model_group_inline_types_recursive(
                    &group_def.particles,
                    owner_key,
                    target_ns,
                    flat_idx,
                    jobs,
                );
            }
            _ => {} // Skip group refs and wildcards
        }
    }
}

/// Allocation job for a local element in a content particle
struct ContentParticleElementJob {
    complex_type_key: ComplexTypeKey,
    flat_idx: usize,
    elem: ElementFrameResult,
    target_namespace: Option<NameId>,
}

/// Walk content particles recursively and collect local element allocation jobs
fn collect_content_particle_elements_recursive(
    particles: &[ParticleResult],
    complex_type_key: ComplexTypeKey,
    target_ns: Option<NameId>,
    flat_idx: &mut usize,
    jobs: &mut Vec<ContentParticleElementJob>,
) {
    for particle in particles {
        match &particle.term {
            ParticleTerm::Element(elem) if elem.ref_name.is_none() => {
                jobs.push(ContentParticleElementJob {
                    complex_type_key,
                    flat_idx: *flat_idx,
                    elem: elem.clone(),
                    target_namespace: target_ns,
                });
                *flat_idx += 1;
            }
            ParticleTerm::Element(_) => {
                // Ref element - skip allocation but still increment counter
                *flat_idx += 1;
            }
            ParticleTerm::Group(group_def) if group_def.ref_name.is_none() => {
                collect_content_particle_elements_recursive(
                    &group_def.particles,
                    complex_type_key,
                    target_ns,
                    flat_idx,
                    jobs,
                );
            }
            _ => {} // Skip group refs and wildcards
        }
    }
}

/// Resolve an XSD 1.1 identity constraint @ref to the referenced IC key.
///
/// §3.11.2: the corresponding schema component is the identity-constraint
/// definition resolved to by the actual value of the ref [attribute].
/// §3.11.6 clause 5: the referenced IC's category must match the element tag.
pub(crate) fn resolve_ic_ref(
    kind: IdentityKind,
    ref_name: &QNameRef,
    source: Option<&SourceRef>,
    target_namespace: Option<NameId>,
    schema_set: &crate::schema::SchemaSet,
) -> crate::error::SchemaResult<IdentityConstraintKey> {
    let ref_ns = ref_name.namespace.or(target_namespace);
    let ref_local = ref_name.local_name;

    // §3.17.6.2 clause 4 — per-document QName visibility gate.
    // Feed the *resolved* `ref_ns` (after target-namespace fallback) so an
    // unprefixed IC ref doesn't accidentally bypass the gate as "absent".
    crate::schema::resolver::check_namespace_visible_ns(
        schema_set,
        ref_ns,
        ref_local,
        source,
        "Identity constraint",
    )?;

    // Look up in namespace tables
    let target_key = schema_set
        .namespaces
        .get(&ref_ns)
        .and_then(|nt| nt.identity_constraints.get(&ref_local))
        .copied();

    let target_key = match target_key {
        Some(k) => k,
        None => {
            // Search arena as fallback (IC may not be in namespace table yet)
            let mut found = None;
            for (key, ic_data) in &schema_set.arenas.identity_constraints {
                if ic_data.name != ref_local {
                    continue;
                }
                let ic_ns = ic_data
                    .source
                    .as_ref()
                    .and_then(|s| schema_set.documents.get(s.doc_id as usize))
                    .and_then(|d| d.target_namespace);
                if ic_ns == ref_ns {
                    found = Some(key);
                    break;
                }
            }
            found.ok_or_else(|| {
                let ref_display = crate::schema::resolver::format_resolved_qname(
                    &schema_set.name_table,
                    ref_ns,
                    ref_local,
                );
                let location = source.and_then(|s| schema_set.source_maps.locate(s));
                crate::error::SchemaError::structural(
                    "src-resolve",
                    format!("Identity constraint ref target '{}' not found", ref_display),
                    location,
                )
            })?
        }
    };

    // §3.11.6 clause 5: kind must match
    let target = &schema_set.arenas.identity_constraints[target_key];
    if target.kind != kind {
        let ref_display = crate::schema::resolver::format_resolved_qname(
            &schema_set.name_table,
            ref_ns,
            ref_local,
        );
        let location = source.and_then(|s| schema_set.source_maps.locate(s));
        return Err(crate::error::SchemaError::structural(
            "src-identity-constraint.5",
            format!(
                "Identity constraint ref '{}': referenced constraint is {:?} but expected {:?}",
                ref_display, target.kind, kind
            ),
            location,
        ));
    }

    Ok(target_key)
}

/// Validate every keyref's `refer` (§3.11.2 {referenced key}, §3.11.6
/// Identity-constraint Definition Properties Correct clause 2): it must name a
/// key or unique, with as many fields as the keyref has.
///
/// Run once every identity-constraint definition of the schema is in the
/// arena — top-level and local elements, every schema document — because the
/// definitions form one symbol space (§3.11.1): a `refer` may name a
/// definition that comes later in its document, sits on a local element
/// allocated later, or lives in another schema document of the namespace.
/// An unqualified `refer` without a default namespace falls back to the
/// keyref's target namespace, as the runtime's lookup does.
pub(crate) fn validate_all_keyref_refers(
    schema_set: &crate::schema::SchemaSet,
) -> SchemaResult<()> {
    let doc_ns = |source: Option<&SourceRef>| {
        source
            .and_then(|s| schema_set.documents.get(s.doc_id as usize))
            .and_then(|d| d.target_namespace)
    };
    for (_key, ic) in schema_set.arenas.identity_constraints.iter() {
        if ic.kind != IdentityKind::Keyref {
            continue;
        }
        let Some(refer) = &ic.refer else {
            continue;
        };
        let refer_ns = refer.namespace.or(doc_ns(ic.source.as_ref()));
        let target = schema_set
            .arenas
            .identity_constraints
            .values()
            .find(|other| {
                other.name == refer.local_name && doc_ns(other.source.as_ref()) == refer_ns
            });
        let location = ic
            .source
            .as_ref()
            .and_then(|s| schema_set.source_maps.locate(s));
        let ic_name = schema_set.name_table.resolve_ref(ic.name);
        let refer_name = schema_set.name_table.resolve_ref(refer.local_name);
        match target {
            None => {
                return Err(SchemaError::structural(
                    "src-identity-constraint",
                    format!(
                        "Keyref '{}': refer target '{}' not found among identity constraints",
                        ic_name, refer_name
                    ),
                    location,
                ));
            }
            Some(target) if target.kind == IdentityKind::Keyref => {
                return Err(SchemaError::structural(
                    "src-identity-constraint",
                    format!(
                        "Keyref '{}': refer target '{}' is a keyref, not a key or unique",
                        ic_name, refer_name
                    ),
                    location,
                ));
            }
            Some(target) if target.fields.len() != ic.fields.len() => {
                return Err(SchemaError::structural(
                    "src-identity-constraint",
                    format!(
                        "Keyref '{}': has {} field(s) but refer target '{}' has {} field(s)",
                        ic_name,
                        ic.fields.len(),
                        refer_name,
                        target.fields.len()
                    ),
                    location,
                ));
            }
            Some(_) => {}
        }
    }
    Ok(())
}

/// Allocate arena element declarations for local elements in content particles.
///
/// This enables the validator to look up nillable, fixed_value, default_value
/// and other properties for local elements via their ElementKey.
///
/// Must be called after inline type assembly and reference resolution.
pub fn allocate_content_particle_elements(schema_set: &mut SchemaSet) -> SchemaResult<()> {
    // Collection pass: walk all complex types and collect jobs.
    //
    // Skip elements whose slot in `resolved_content_particle_elements` is
    // already populated, so the function is safe to re-run (e.g., after
    // a second inline-type assembly pass for alternatives discovered on
    // local elements). The flat_idx still advances on every particle so
    // newly added complex types collected in the same pass keep correct
    // indexing.
    let mut jobs = Vec::new();
    for (key, complex) in schema_set.arenas.complex_types.iter() {
        let target_ns = complex.target_namespace;
        if let ComplexContentResult::Complex(def) = &complex.content {
            if let Some(particle) = &def.particle {
                if let ParticleTerm::Group(group_def) = &particle.term {
                    let mut flat_idx = 0;
                    collect_content_particle_elements_recursive(
                        &group_def.particles,
                        key,
                        target_ns,
                        &mut flat_idx,
                        &mut jobs,
                    );
                }
            }
        }
    }
    // Drop jobs whose slot is already filled in the owning complex type.
    jobs.retain(|job| {
        match schema_set
            .arenas
            .complex_types
            .get(job.complex_type_key)
            .and_then(|ct| ct.resolved_content_particle_elements.get(job.flat_idx))
        {
            Some(Some(_)) => false, // already allocated
            _ => true,
        }
    });

    // Build per-document sets of already-known identity constraint names for uniqueness checking
    // XSD §4.2.1: IC names must be unique per schema document, not globally
    let mut ic_names_by_doc: HashMap<DocumentId, HashSet<NameId>> = HashMap::new();
    for ic in schema_set.arenas.identity_constraints.values() {
        if let Some(source) = &ic.source {
            ic_names_by_doc
                .entry(source.doc_id)
                .or_default()
                .insert(ic.name);
        }
    }

    // Allocation pass: create element declarations and store keys
    for job in jobs {
        // §3.17.6.2 clause 4 — per-document QName visibility gate.
        // This fast path bypasses `resolve_type_ref` so it must gate the QName
        // itself; otherwise un-imported local-element type refs slip through.
        if let Some(TypeRefResult::QName(qname)) = &job.elem.type_ref {
            crate::schema::resolver::check_namespace_visible(
                schema_set,
                qname,
                job.elem.source.as_ref(),
                "Type",
            )?;
        }

        let resolved_type = schema_set
            .arenas
            .complex_types
            .get(job.complex_type_key)
            .and_then(|ct| {
                ct.resolved_content_particle_types
                    .get(job.flat_idx)
                    .copied()
                    .flatten()
            })
            .or_else(|| {
                // Try QName resolution
                match &job.elem.type_ref {
                    Some(TypeRefResult::QName(qname)) => schema_set
                        .lookup_type(qname.namespace, qname.local_name)
                        .or_else(|| {
                            schema_set.get_built_in_type_by_qname(qname.namespace, qname.local_name)
                        }),
                    _ => None,
                }
            });

        // §3.3.2 / §3.3.1: when a local element has neither `type=` nor
        // an inline `<simpleType>` / `<complexType>` (and resolved_type is
        // therefore not yet set), the {type definition} property defaults to
        // xs:anyType. Otherwise content-model initialization in
        // `init_content_model` falls back to (Simple, TextOnly), causing
        // valid mixed/lax-wildcard children to be rejected (xsd002.v01).
        let resolved_type = resolved_type.or_else(|| match &job.elem.type_ref {
            // Only fall back when no type was specified at all. If a QName
            // type-ref was given but didn't resolve, src-resolve below
            // surfaces the error.
            None => Some(TypeKey::Complex(schema_set.any_type_key())),
            _ => None,
        });

        // src-resolve §3.3.6: a local element's QName-typed type-ref must
        // resolve. Falling back silently masks bad schemas (s3_12si02).
        if resolved_type.is_none() {
            if let Some(TypeRefResult::QName(qname)) = &job.elem.type_ref {
                let display = crate::schema::resolver::format_resolved_qname(
                    &schema_set.name_table,
                    qname.namespace,
                    qname.local_name,
                );
                let location = schema_set.locate(job.elem.source.as_ref());
                return Err(SchemaError::structural(
                    "src-resolve",
                    format!("Type reference '{}' on local element not found", display),
                    location,
                ));
            }
        }

        let effective_ns = schema_set.effective_local_element_namespace(
            job.elem.target_namespace,
            job.elem.form.as_deref(),
            job.elem.source.as_ref(),
            job.target_namespace,
        );
        let mut identity_constraint_keys = Vec::with_capacity(job.elem.identity_constraints.len());
        for ic in job.elem.identity_constraints {
            // Check per-document uniqueness using the IC's source document
            if let Some(source) = &ic.source {
                let doc_names = ic_names_by_doc.entry(source.doc_id).or_default();
                if !doc_names.insert(ic.name) {
                    let location = schema_set.source_maps.locate(source);
                    let name_str = schema_set.name_table.resolve(ic.name);
                    return Err(SchemaError::structural(
                        "ic-unique",
                        format!(
                            "Duplicate identity constraint name '{}' in schema document",
                            name_str
                        ),
                        location,
                    ));
                }
            }
            let ic_name = ic.name;
            let ic_key = schema_set
                .arenas
                .alloc_identity_constraint(IdentityConstraintData {
                    kind: ic.kind,
                    name: ic.name,
                    ref_name: ic.ref_name,
                    refer: ic.refer,
                    selector: ic.selector,
                    fields: ic.fields,
                    id: ic.id,
                    annotation: ic.annotation,
                    source: ic.source,
                });
            // Register in namespace table for @ref resolution
            let ns_table = schema_set.get_or_create_namespace(job.target_namespace);
            ns_table.identity_constraints.insert(ic_name, ic_key);
            identity_constraint_keys.push(ic_key);
        }
        // Resolve XSD 1.1 @ref identity constraint references
        for ic_ref in &job.elem.identity_constraint_refs {
            let target_key = resolve_ic_ref(
                ic_ref.kind,
                &ic_ref.ref_name,
                ic_ref.source.as_ref(),
                job.target_namespace,
                schema_set,
            )?;
            identity_constraint_keys.push(target_key);
        }

        let block = resolve_block(job.elem.block, job.elem.source.as_ref(), schema_set);
        let final_derivation = resolve_final(
            job.elem.final_derivation,
            job.elem.source.as_ref(),
            schema_set,
        );
        let elem_data = ElementDeclData {
            name: job.elem.name,
            target_namespace: effective_ns,
            ref_name: None,
            type_ref: job.elem.type_ref.clone(),
            inline_type: job.elem.inline_type.clone(),
            substitution_group: Vec::new(),
            default_value: job.elem.default_value.clone(),
            fixed_value: job.elem.fixed_value.clone(),
            nillable: job.elem.nillable,
            is_abstract: job.elem.is_abstract,
            min_occurs: job.elem.min_occurs,
            max_occurs: job.elem.max_occurs,
            block,
            final_derivation,
            form: job.elem.form.clone(),
            id: job.elem.id.clone(),
            alternatives: job.elem.alternatives.clone(),
            identity_constraints: identity_constraint_keys,
            pending_ic_refs: vec![],
            annotation: job.elem.annotation.clone(),
            source: job.elem.source.clone(),
            resolved_type,
            resolved_ref: None,
            resolved_substitution_groups: Vec::new(),
            deferred_type_error: None,
        };

        let elem_key = schema_set.arenas.alloc_element(elem_data);

        if let Some(ct) = schema_set
            .arenas
            .entries_mut()
            .complex_types
            .get_mut(job.complex_type_key)
        {
            while ct.resolved_content_particle_elements.len() <= job.flat_idx {
                ct.resolved_content_particle_elements.push(None);
            }
            ct.resolved_content_particle_elements[job.flat_idx] = Some(elem_key);
        }
    }

    Ok(())
}

/// XSD 1.1: Assemble inline `<xs:alternative>` types attached to *local*
/// element declarations.
///
/// The first inline-type assembly pass only walks `arenas.elements`,
/// which at that point contains global elements only. Local elements
/// declared inside complex-type particles are allocated later by
/// [`allocate_content_particle_elements`], and any inline alternative
/// types they carry would otherwise stay unresolved.
///
/// Run this after [`allocate_content_particle_elements`]. It:
///
/// 1. Walks every element declaration whose alternatives have an
///    `inline_type` but no `resolved_type`.
/// 2. Calls `assemble_inline_type` to add the inline complex/simple
///    type to the arena and stores the resulting `TypeKey` on the
///    alternative.
/// 3. Returns the list of complex-type keys that were newly added so
///    the caller can re-run reference resolution and content-particle
///    allocation on them.
///
/// The caller must also call [`resolve_all_references`](crate::schema::resolver::resolve_all_references) and
/// [`allocate_content_particle_elements`] again so the new types get
/// their `resolved_base_type`, `resolved_content_particle_types`, and
/// `resolved_content_particle_elements` populated. Both passes are
/// idempotent for already-resolved entries.
#[cfg(feature = "xsd11")]
pub fn resolve_local_element_alternatives(
    schema_set: &mut SchemaSet,
) -> SchemaResult<Vec<ComplexTypeKey>> {
    use crate::ids::ComplexTypeKey;

    // Pass 1: Collect QName type_refs that need resolution (no inline type).
    // These are alternatives like `<xs:alternative test=… type="Quadrilateral"/>`
    // attached to local element declarations. The first
    // `resolve_all_references` pass walked global elements only; local
    // element alternatives copied during `allocate_content_particle_elements`
    // still carry an unresolved QName here.
    let mut qname_pending: Vec<(ElementKey, usize, QNameRef, Option<SourceRef>)> = Vec::new();
    for (key, elem) in schema_set.arenas.elements.iter() {
        for (idx, alt) in elem.alternatives.iter().enumerate() {
            if alt.resolved_type.is_some() {
                continue;
            }
            if let Some(TypeRefResult::QName(qname)) = &alt.type_ref {
                qname_pending.push((key, idx, qname.clone(), alt.source.clone()));
            }
        }
    }
    for (elem_key, alt_idx, qname, src) in qname_pending {
        let resolver = crate::schema::resolver::ReferenceResolver::new(schema_set);
        let type_key = resolver.resolve_type_ref(&qname, src.as_ref())?;
        if let Some(elem) = schema_set.arenas.entries_mut().elements.get_mut(elem_key) {
            if let Some(alt) = elem.alternatives.get_mut(alt_idx) {
                alt.resolved_type = Some(type_key);
            }
        }
    }

    // Pass 2: Collect (element_key, alt_idx, type_frame) triples first to
    // avoid mutating arenas while iterating them. Handles inline types.
    let mut pending: Vec<(ElementKey, usize, TypeFrameResult, Option<NameId>)> = Vec::new();
    for (key, elem) in schema_set.arenas.elements.iter() {
        for (idx, alt) in elem.alternatives.iter().enumerate() {
            if alt.resolved_type.is_some() {
                continue;
            }
            let Some(inline_type) = &alt.inline_type else {
                continue;
            };
            pending.push((key, idx, (**inline_type).clone(), elem.target_namespace));
        }
    }

    let mut new_complex_keys: Vec<ComplexTypeKey> = Vec::new();
    for (elem_key, alt_idx, type_frame, target_ns) in pending {
        let type_key = assemble_inline_type(schema_set, &type_frame, target_ns)?;
        if let TypeKey::Complex(ct_key) = type_key {
            new_complex_keys.push(ct_key);
        }
        if let Some(elem) = schema_set.arenas.entries_mut().elements.get_mut(elem_key) {
            if let Some(alt) = elem.alternatives.get_mut(alt_idx) {
                alt.resolved_type = Some(type_key);
            }
        }
    }

    Ok(new_complex_keys)
}

/// Allocation job for a local element in a named model group particle
struct ModelGroupElementJob {
    group_key: ModelGroupKey,
    particle_idx: usize,
    elem: ElementFrameResult,
    target_namespace: Option<NameId>,
}

/// Allocate arena element declarations for local elements in named model group particles.
///
/// This enables the validator to look up nillable, fixed_value, default_value
/// and other properties for local elements inside named groups via their ElementKey.
///
/// Must be called after inline type assembly and reference resolution.
pub fn allocate_model_group_particle_elements(schema_set: &mut SchemaSet) -> SchemaResult<()> {
    // Collection pass: walk all model groups recursively and collect jobs
    let mut jobs = Vec::new();
    for (key, group) in schema_set.arenas.model_groups.iter() {
        let target_ns = group.target_namespace;
        let mut flat_idx = 0;
        collect_model_group_elements_recursive(
            &group.particles,
            key,
            target_ns,
            &mut flat_idx,
            &mut jobs,
        );
    }

    // Build per-document sets of already-known identity constraint names for uniqueness checking
    // XSD §4.2.1: IC names must be unique per schema document, not globally
    let mut ic_names_by_doc: HashMap<DocumentId, HashSet<NameId>> = HashMap::new();
    for ic in schema_set.arenas.identity_constraints.values() {
        if let Some(source) = &ic.source {
            ic_names_by_doc
                .entry(source.doc_id)
                .or_default()
                .insert(ic.name);
        }
    }

    // Allocation pass: create element declarations and store keys
    for job in jobs {
        // §3.17.6.2 clause 4 — per-document QName visibility gate.
        if let Some(TypeRefResult::QName(qname)) = &job.elem.type_ref {
            crate::schema::resolver::check_namespace_visible(
                schema_set,
                qname,
                job.elem.source.as_ref(),
                "Type",
            )?;
        }

        let resolved_type = schema_set
            .arenas
            .model_groups
            .get(job.group_key)
            .and_then(|g| {
                g.resolved_particle_types
                    .get(job.particle_idx)
                    .copied()
                    .flatten()
            })
            .or_else(|| {
                // Try QName resolution
                match &job.elem.type_ref {
                    Some(TypeRefResult::QName(qname)) => schema_set
                        .lookup_type(qname.namespace, qname.local_name)
                        .or_else(|| {
                            schema_set.get_built_in_type_by_qname(qname.namespace, qname.local_name)
                        }),
                    _ => None,
                }
            });

        let effective_ns = schema_set.effective_local_element_namespace(
            job.elem.target_namespace,
            job.elem.form.as_deref(),
            job.elem.source.as_ref(),
            job.target_namespace,
        );
        let mut identity_constraint_keys = Vec::with_capacity(job.elem.identity_constraints.len());
        for ic in job.elem.identity_constraints {
            // Check per-document uniqueness using the IC's source document
            if let Some(source) = &ic.source {
                let doc_names = ic_names_by_doc.entry(source.doc_id).or_default();
                if !doc_names.insert(ic.name) {
                    let location = schema_set.source_maps.locate(source);
                    let name_str = schema_set.name_table.resolve(ic.name);
                    return Err(SchemaError::structural(
                        "ic-unique",
                        format!(
                            "Duplicate identity constraint name '{}' in schema document",
                            name_str
                        ),
                        location,
                    ));
                }
            }
            let ic_name = ic.name;
            let ic_key = schema_set
                .arenas
                .alloc_identity_constraint(IdentityConstraintData {
                    kind: ic.kind,
                    name: ic.name,
                    ref_name: ic.ref_name,
                    refer: ic.refer,
                    selector: ic.selector,
                    fields: ic.fields,
                    id: ic.id,
                    annotation: ic.annotation,
                    source: ic.source,
                });
            // Register in namespace table for @ref resolution
            let ns_table = schema_set.get_or_create_namespace(job.target_namespace);
            ns_table.identity_constraints.insert(ic_name, ic_key);
            identity_constraint_keys.push(ic_key);
        }
        // Resolve XSD 1.1 @ref identity constraint references
        for ic_ref in &job.elem.identity_constraint_refs {
            let target_key = resolve_ic_ref(
                ic_ref.kind,
                &ic_ref.ref_name,
                ic_ref.source.as_ref(),
                job.target_namespace,
                schema_set,
            )?;
            identity_constraint_keys.push(target_key);
        }

        let block = resolve_block(job.elem.block, job.elem.source.as_ref(), schema_set);
        let final_derivation = resolve_final(
            job.elem.final_derivation,
            job.elem.source.as_ref(),
            schema_set,
        );
        let elem_data = ElementDeclData {
            name: job.elem.name,
            target_namespace: effective_ns,
            ref_name: None,
            type_ref: job.elem.type_ref.clone(),
            inline_type: job.elem.inline_type.clone(),
            substitution_group: Vec::new(),
            default_value: job.elem.default_value.clone(),
            fixed_value: job.elem.fixed_value.clone(),
            nillable: job.elem.nillable,
            is_abstract: job.elem.is_abstract,
            min_occurs: job.elem.min_occurs,
            max_occurs: job.elem.max_occurs,
            block,
            final_derivation,
            form: job.elem.form.clone(),
            id: job.elem.id.clone(),
            alternatives: job.elem.alternatives.clone(),
            identity_constraints: identity_constraint_keys,
            pending_ic_refs: vec![],
            annotation: job.elem.annotation.clone(),
            source: job.elem.source.clone(),
            resolved_type,
            resolved_ref: None,
            resolved_substitution_groups: Vec::new(),
            deferred_type_error: None,
        };

        let elem_key = schema_set.arenas.alloc_element(elem_data);

        if let Some(group) = schema_set
            .arenas
            .entries_mut()
            .model_groups
            .get_mut(job.group_key)
        {
            while group.resolved_particle_elements.len() <= job.particle_idx {
                group.resolved_particle_elements.push(None);
            }
            group.resolved_particle_elements[job.particle_idx] = Some(elem_key);
        }
    }

    Ok(())
}

/// Walk model group particles recursively and collect local element allocation jobs
fn collect_model_group_elements_recursive(
    particles: &[ParticleResult],
    group_key: ModelGroupKey,
    target_ns: Option<NameId>,
    flat_idx: &mut usize,
    jobs: &mut Vec<ModelGroupElementJob>,
) {
    for particle in particles {
        match &particle.term {
            ParticleTerm::Element(elem) if elem.ref_name.is_none() => {
                jobs.push(ModelGroupElementJob {
                    group_key,
                    particle_idx: *flat_idx,
                    elem: elem.clone(),
                    target_namespace: target_ns,
                });
                *flat_idx += 1;
            }
            ParticleTerm::Element(_) => {
                // Ref element - skip allocation but still increment counter
                *flat_idx += 1;
            }
            ParticleTerm::Group(group_def) if group_def.ref_name.is_none() => {
                collect_model_group_elements_recursive(
                    &group_def.particles,
                    group_key,
                    target_ns,
                    flat_idx,
                    jobs,
                );
            }
            _ => {} // Skip group refs and wildcards
        }
    }
}

/// Assemble an inline type into the arena
///
/// Returns the TypeKey for the newly allocated type.
/// Anonymous types are not registered in namespace tables.
fn assemble_inline_type(
    schema_set: &mut SchemaSet,
    type_frame: &TypeFrameResult,
    target_namespace: Option<NameId>,
) -> SchemaResult<TypeKey> {
    match type_frame {
        TypeFrameResult::Simple(simple) => {
            let final_derivation =
                resolve_final(simple.final_derivation, simple.source.as_ref(), schema_set);
            let data = SimpleTypeDefData {
                name: simple.name, // May be None for anonymous types
                target_namespace,
                variety: simple.variety,
                base_type: simple.base_type.clone(),
                item_type: simple.item_type.clone(),
                member_types: simple.member_types.clone(),
                facets: simple.facets.clone(),
                final_derivation,
                id: simple.id.clone(),
                derivation_id: simple.derivation_id.clone(),
                annotation: simple.annotation.clone(),
                source: simple.source.clone(),
                // Resolved references (to be populated later)
                resolved_base_type: None,
                resolved_item_type: None,
                resolved_member_types: Vec::new(),
                redefine_original: None,
                deferred_item_type_error: None,
            };
            let key = schema_set.arenas.alloc_simple_type(data);
            Ok(TypeKey::Simple(key))
        }
        TypeFrameResult::Complex(complex) => {
            let open_content = match &complex.content {
                ComplexContentResult::Complex(def) => def.open_content.clone(),
                _ => None,
            };
            #[cfg(feature = "xsd11")]
            let assertions = match &complex.content {
                ComplexContentResult::Simple(sc) => sc.assertions.clone(),
                ComplexContentResult::Complex(cc) => cc.assertions.clone(),
                ComplexContentResult::Empty => Vec::new(),
            };
            let block = resolve_block(complex.block, complex.source.as_ref(), schema_set);
            let final_derivation = resolve_final(
                complex.final_derivation,
                complex.source.as_ref(),
                schema_set,
            );
            let data = ComplexTypeDefData {
                name: complex.name, // May be None for anonymous types
                target_namespace,
                base_type: complex.base_type.clone(),
                derivation_method: complex.derivation_method,
                content: complex.content.clone(),
                open_content,
                attributes: complex.attributes.clone(),
                attribute_groups: complex.attribute_groups.clone(),
                attribute_wildcard: complex.attribute_wildcard.clone(),
                mixed: complex.mixed,
                is_abstract: complex.is_abstract,
                final_derivation,
                block,
                default_attributes_apply: complex.default_attributes_apply,
                id: complex.id.clone(),
                #[cfg(feature = "xsd11")]
                assertions,
                #[cfg(feature = "xsd11")]
                xpath_default_namespace: complex.xpath_default_namespace.clone(),
                annotation: complex.annotation.clone(),
                source: complex.source.clone(),
                // Resolved references (to be populated later)
                resolved_base_type: None,
                resolved_attribute_groups: Vec::new(),
                resolved_attributes: Vec::new(),
                resolved_content_particle_types: Vec::new(),
                resolved_content_particle_elements: Vec::new(),
                resolved_simple_content_type: None,
                redefine_original: None,
            };
            let key = schema_set.arenas.alloc_complex_type(data);
            Ok(TypeKey::Complex(key))
        }
    }
}

/// Update the owner component with the resolved type key
fn update_owner(
    schema_set: &mut SchemaSet,
    job: &InlineTypeJob,
    type_key: TypeKey,
    stats: &mut InlineAssemblyStats,
) -> SchemaResult<()> {
    match job.owner {
        InlineOwner::Element(key) => {
            if let Some(elem) = schema_set.arenas.entries_mut().elements.get_mut(key) {
                match job.role {
                    #[cfg(feature = "xsd11")]
                    InlineRole::AlternativeType(idx) => {
                        if let Some(alt) = elem.alternatives.get_mut(idx) {
                            alt.resolved_type = Some(type_key);
                        }
                    }
                    _ => {
                        elem.resolved_type = Some(type_key);
                        stats.element_inline_types += 1;
                    }
                }
                stats.total_inline_types += 1;
            }
        }
        InlineOwner::Attribute(key) => {
            if let Some(attr) = schema_set.arenas.entries_mut().attributes.get_mut(key) {
                attr.resolved_type = Some(type_key);
                stats.attribute_inline_types += 1;
                stats.total_inline_types += 1;
            }
        }
        InlineOwner::SimpleType(key) => {
            if let Some(simple) = schema_set.arenas.entries_mut().simple_types.get_mut(key) {
                match job.role {
                    InlineRole::SimpleTypeBase => {
                        simple.resolved_base_type = Some(type_key);
                    }
                    InlineRole::ListItemType => {
                        simple.resolved_item_type = Some(type_key);
                    }
                    InlineRole::UnionMemberType(idx) => {
                        // For union members, we need to track position
                        // Insert at the correct position in resolved_member_types
                        while simple.resolved_member_types.len() <= idx {
                            // Use a placeholder that will be overwritten
                            // This handles sparse indices from inline types
                            simple.resolved_member_types.push(type_key);
                        }
                        simple.resolved_member_types[idx] = type_key;
                    }
                    _ => {}
                }
                stats.simple_type_inline_derivations += 1;
                stats.total_inline_types += 1;
            }
        }
        InlineOwner::ComplexType(key) => {
            if let Some(complex) = schema_set.arenas.entries_mut().complex_types.get_mut(key) {
                match job.role {
                    InlineRole::ComplexTypeBase => {
                        complex.resolved_base_type = Some(type_key);
                        stats.complex_type_inline_derivations += 1;
                    }
                    InlineRole::ComplexTypeAttribute(idx) => {
                        // Ensure resolved_attributes vec is large enough
                        while complex.resolved_attributes.len() <= idx {
                            complex.resolved_attributes.push(ResolvedAttributeUse {
                                resolved_type: None,
                                resolved_ref: None,
                            });
                        }
                        complex.resolved_attributes[idx].resolved_type = Some(type_key);
                        stats.complex_type_attribute_inline_types += 1;
                    }
                    InlineRole::ContentParticleType(idx) => {
                        while complex.resolved_content_particle_types.len() <= idx {
                            complex.resolved_content_particle_types.push(None);
                        }
                        complex.resolved_content_particle_types[idx] = Some(type_key);
                        stats.complex_type_inline_derivations += 1;
                    }
                    InlineRole::SimpleContentInlineType => {
                        complex.resolved_simple_content_type = Some(type_key);
                        stats.complex_type_inline_derivations += 1;
                    }
                    _ => {}
                }
                stats.total_inline_types += 1;
            }
        }
        InlineOwner::ModelGroup(key) => {
            if let Some(group) = schema_set.arenas.entries_mut().model_groups.get_mut(key) {
                if let InlineRole::ModelGroupParticle(flat_idx) = job.role {
                    // Store in flat-indexed resolved_particle_types
                    while group.resolved_particle_types.len() <= flat_idx {
                        group.resolved_particle_types.push(None);
                    }
                    group.resolved_particle_types[flat_idx] = Some(type_key);
                    stats.model_group_inline_types += 1;
                    stats.total_inline_types += 1;
                }
            }
        }
        InlineOwner::AttributeGroup(key) => {
            if let Some(group) = schema_set
                .arenas
                .entries_mut()
                .attribute_groups
                .get_mut(key)
            {
                if let InlineRole::AttributeGroupAttribute(idx) = job.role {
                    // Ensure resolved_attributes vec is large enough
                    while group.resolved_attributes.len() <= idx {
                        group.resolved_attributes.push(ResolvedAttributeUse {
                            resolved_type: None,
                            resolved_ref: None,
                        });
                    }
                    group.resolved_attributes[idx].resolved_type = Some(type_key);
                    stats.attribute_group_inline_types += 1;
                    stats.total_inline_types += 1;
                }
            }
        }
    }
    Ok(())
}

/// Collect nested inline types from a newly assembled type
fn collect_nested_inline_types(
    schema_set: &SchemaSet,
    type_key: TypeKey,
    target_namespace: Option<NameId>,
    jobs: &mut Vec<InlineTypeJob>,
) {
    match type_key {
        TypeKey::Simple(key) => {
            if let Some(simple) = schema_set.arenas.simple_types.get(key) {
                // Check for nested inline types in base/item/member
                if let Some(TypeRefResult::Inline(inline_type)) = &simple.base_type {
                    jobs.push(InlineTypeJob {
                        owner: InlineOwner::SimpleType(key),
                        role: InlineRole::SimpleTypeBase,
                        type_frame: (**inline_type).clone(),
                        target_namespace,
                    });
                }
                if let Some(TypeRefResult::Inline(inline_type)) = &simple.item_type {
                    jobs.push(InlineTypeJob {
                        owner: InlineOwner::SimpleType(key),
                        role: InlineRole::ListItemType,
                        type_frame: (**inline_type).clone(),
                        target_namespace,
                    });
                }
                for (idx, member) in simple.member_types.iter().enumerate() {
                    if let TypeRefResult::Inline(inline_type) = member {
                        jobs.push(InlineTypeJob {
                            owner: InlineOwner::SimpleType(key),
                            role: InlineRole::UnionMemberType(idx),
                            type_frame: (**inline_type).clone(),
                            target_namespace,
                        });
                    }
                }
            }
        }
        TypeKey::Complex(key) => {
            if let Some(complex) = schema_set.arenas.complex_types.get(key) {
                // Check for nested inline types in base type
                if let Some(TypeRefResult::Inline(inline_type)) = &complex.base_type {
                    jobs.push(InlineTypeJob {
                        owner: InlineOwner::ComplexType(key),
                        role: InlineRole::ComplexTypeBase,
                        type_frame: (**inline_type).clone(),
                        target_namespace,
                    });
                }

                // Check for inline types in attributes
                for (idx, attr_use) in complex.attributes.iter().enumerate() {
                    if let Some(inline_type) = &attr_use.attribute.inline_type {
                        jobs.push(InlineTypeJob {
                            owner: InlineOwner::ComplexType(key),
                            role: InlineRole::ComplexTypeAttribute(idx),
                            type_frame: TypeFrameResult::Simple(Box::new((**inline_type).clone())),
                            target_namespace,
                        });
                    }
                }

                // Check content for nested inline types
                collect_content_inline_types(&complex.content, key, target_namespace, jobs);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arenas::ElementDeclData;
    use crate::parser::frames::{SimpleTypeResult, SimpleTypeVariety};
    use crate::schema::model::DerivationSet;
    use crate::types::facets::FacetSet;

    fn create_simple_type_frame(variety: SimpleTypeVariety) -> TypeFrameResult {
        TypeFrameResult::Simple(Box::new(SimpleTypeResult {
            name: None,
            variety,
            base_type: None,
            item_type: None,
            member_types: Vec::new(),
            facets: FacetSet::new(),
            final_derivation: None,
            id: None,
            derivation_id: None,
            annotation: None,
            source: None,
        }))
    }

    #[test]
    fn test_assemble_inline_types_empty_schema() {
        let mut schema_set = SchemaSet::new();
        let result = assemble_inline_types(&mut schema_set);
        assert!(result.is_ok());
        let stats = result.unwrap();
        assert_eq!(stats.total_inline_types, 0);
    }

    #[test]
    fn test_element_with_inline_simple_type() {
        let mut schema_set = SchemaSet::new();

        let elem_name = schema_set.name_table.add("testElement");
        let inline_type = Box::new(create_simple_type_frame(SimpleTypeVariety::Atomic));

        let elem_data = ElementDeclData {
            name: Some(elem_name),
            target_namespace: None,
            ref_name: None,
            type_ref: None,
            inline_type: Some(inline_type),
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
            resolved_type: None,
            resolved_ref: None,
            resolved_substitution_groups: Vec::new(),
            deferred_type_error: None,
        };

        let elem_key = schema_set.arenas.alloc_element(elem_data);

        // Run inline assembly
        let result = assemble_inline_types(&mut schema_set);
        assert!(result.is_ok());
        let stats = result.unwrap();
        assert_eq!(stats.element_inline_types, 1);
        assert_eq!(stats.total_inline_types, 1);

        // Verify resolved_type is set
        let elem = schema_set.arenas.elements.get(elem_key).unwrap();
        assert!(elem.resolved_type.is_some());
        assert!(matches!(elem.resolved_type, Some(TypeKey::Simple(_))));

        // Verify inline_type is preserved
        assert!(elem.inline_type.is_some());
    }

    #[test]
    fn test_simple_type_with_inline_base() {
        let mut schema_set = SchemaSet::new();

        let type_name = schema_set.name_table.add("testType");
        let inline_base = Box::new(create_simple_type_frame(SimpleTypeVariety::Atomic));

        let type_data = SimpleTypeDefData {
            name: Some(type_name),
            target_namespace: None,
            variety: SimpleTypeVariety::Atomic,
            base_type: Some(TypeRefResult::Inline(inline_base)),
            item_type: None,
            member_types: Vec::new(),
            facets: FacetSet::new(),
            final_derivation: DerivationSet::empty(),
            id: None,
            derivation_id: None,
            annotation: None,
            source: None,
            resolved_base_type: None,
            resolved_item_type: None,
            resolved_member_types: Vec::new(),
            redefine_original: None,
            deferred_item_type_error: None,
        };

        let type_key = schema_set.arenas.alloc_simple_type(type_data);

        // Run inline assembly
        let result = assemble_inline_types(&mut schema_set);
        assert!(result.is_ok());
        let stats = result.unwrap();
        assert_eq!(stats.simple_type_inline_derivations, 1);

        // Verify resolved_base_type is set
        let simple = schema_set.arenas.simple_types.get(type_key).unwrap();
        assert!(simple.resolved_base_type.is_some());

        // Verify original base_type is preserved
        assert!(matches!(simple.base_type, Some(TypeRefResult::Inline(_))));
    }

    #[test]
    fn test_list_type_with_inline_item() {
        let mut schema_set = SchemaSet::new();

        let type_name = schema_set.name_table.add("listType");
        let inline_item = Box::new(create_simple_type_frame(SimpleTypeVariety::Atomic));

        let type_data = SimpleTypeDefData {
            name: Some(type_name),
            target_namespace: None,
            variety: SimpleTypeVariety::List,
            base_type: None,
            item_type: Some(TypeRefResult::Inline(inline_item)),
            member_types: Vec::new(),
            facets: FacetSet::new(),
            final_derivation: DerivationSet::empty(),
            id: None,
            derivation_id: None,
            annotation: None,
            source: None,
            resolved_base_type: None,
            resolved_item_type: None,
            resolved_member_types: Vec::new(),
            redefine_original: None,
            deferred_item_type_error: None,
        };

        let type_key = schema_set.arenas.alloc_simple_type(type_data);

        // Run inline assembly
        let result = assemble_inline_types(&mut schema_set);
        assert!(result.is_ok());

        // Verify resolved_item_type is set
        let simple = schema_set.arenas.simple_types.get(type_key).unwrap();
        assert!(simple.resolved_item_type.is_some());
    }

    #[test]
    fn test_union_type_with_inline_members() {
        let mut schema_set = SchemaSet::new();

        let type_name = schema_set.name_table.add("unionType");
        let inline_member1 = Box::new(create_simple_type_frame(SimpleTypeVariety::Atomic));
        let inline_member2 = Box::new(create_simple_type_frame(SimpleTypeVariety::Atomic));

        let type_data = SimpleTypeDefData {
            name: Some(type_name),
            target_namespace: None,
            variety: SimpleTypeVariety::Union,
            base_type: None,
            item_type: None,
            member_types: vec![
                TypeRefResult::Inline(inline_member1),
                TypeRefResult::Inline(inline_member2),
            ],
            facets: FacetSet::new(),
            final_derivation: DerivationSet::empty(),
            id: None,
            derivation_id: None,
            annotation: None,
            source: None,
            resolved_base_type: None,
            resolved_item_type: None,
            resolved_member_types: Vec::new(),
            redefine_original: None,
            deferred_item_type_error: None,
        };

        let type_key = schema_set.arenas.alloc_simple_type(type_data);

        // Run inline assembly
        let result = assemble_inline_types(&mut schema_set);
        assert!(result.is_ok());
        let stats = result.unwrap();
        assert_eq!(stats.simple_type_inline_derivations, 2);

        // Verify resolved_member_types has both members
        let simple = schema_set.arenas.simple_types.get(type_key).unwrap();
        assert_eq!(simple.resolved_member_types.len(), 2);
    }

    #[test]
    fn test_inline_type_in_model_group_resolved() {
        use crate::arenas::ModelGroupData;
        use crate::parser::frames::{Compositor, ElementFrameResult};

        let mut schema_set = SchemaSet::new();

        let elem_name = schema_set.name_table.add("detail");
        let inline_type = Box::new(create_simple_type_frame(SimpleTypeVariety::Atomic));

        // Create a named model group with an element that has an inline type
        let group_data = ModelGroupData {
            name: Some(schema_set.name_table.add("myGroup")),
            target_namespace: None,
            ref_name: None,
            compositor: Some(Compositor::Sequence),
            particles: vec![ParticleResult {
                term: ParticleTerm::Element(ElementFrameResult {
                    name: Some(elem_name),
                    ref_name: None,
                    target_namespace: None,
                    type_ref: None,
                    inline_type: Some(inline_type),
                    substitution_group: vec![],
                    default_value: None,
                    fixed_value: None,
                    nillable: false,
                    is_abstract: false,
                    min_occurs: 1,
                    max_occurs: Some(1),
                    block: Default::default(),
                    final_derivation: Default::default(),
                    form: None,
                    id: None,
                    alternatives: vec![],
                    identity_constraints: vec![],
                    identity_constraint_refs: vec![],
                    annotation: None,
                    source: None,
                }),
                min_occurs: 1,
                max_occurs: Some(1),
                source: None,
            }],
            min_occurs: 1,
            max_occurs: Some(1),
            id: None,
            annotation: None,
            source: None,
            resolved_ref: None,
            resolved_particles: Vec::new(),
            resolved_particle_types: Vec::new(),
            resolved_particle_elements: Vec::new(),
            redefine_original: None,
            redefine_requires_restriction_check: false,
        };

        let _group_key = schema_set.arenas.alloc_model_group(group_data);

        let result = assemble_inline_types(&mut schema_set);
        assert!(result.is_ok());
        let stats = result.unwrap();
        assert_eq!(stats.model_group_inline_types, 1);
        assert_eq!(stats.total_inline_types, 1);

        // Verify resolved_particle_types has the type set
        let group = schema_set.arenas.model_groups.get(_group_key).unwrap();
        assert_eq!(group.resolved_particle_types.len(), 1);
        assert!(
            group.resolved_particle_types[0].is_some(),
            "Expected resolved type at flat index 0"
        );
    }

    #[test]
    fn test_inline_type_in_content_particle_resolved() {
        use crate::parser::frames::{
            ComplexContentDefResult, Compositor, ElementFrameResult, ModelGroupDefResult,
        };

        let mut schema_set = SchemaSet::new();

        let elem_name = schema_set.name_table.add("child");
        let inline_type = Box::new(create_simple_type_frame(SimpleTypeVariety::Atomic));

        // Create a complex type with a content particle containing an element with inline type
        let content_particle = ParticleResult {
            term: ParticleTerm::Group(ModelGroupDefResult {
                name: None,
                ref_name: None,
                compositor: Some(Compositor::Sequence),
                particles: vec![ParticleResult {
                    term: ParticleTerm::Element(ElementFrameResult {
                        name: Some(elem_name),
                        ref_name: None,
                        target_namespace: None,
                        type_ref: None,
                        inline_type: Some(inline_type),
                        substitution_group: vec![],
                        default_value: None,
                        fixed_value: None,
                        nillable: false,
                        is_abstract: false,
                        min_occurs: 1,
                        max_occurs: Some(1),
                        block: Default::default(),
                        final_derivation: Default::default(),
                        form: None,
                        id: None,
                        alternatives: vec![],
                        identity_constraints: vec![],
                        identity_constraint_refs: vec![],
                        annotation: None,
                        source: None,
                    }),
                    min_occurs: 1,
                    max_occurs: Some(1),
                    source: None,
                }],
                min_occurs: 1,
                max_occurs: Some(1),
                id: None,
                annotation: None,
                source: None,
            }),
            min_occurs: 1,
            max_occurs: Some(1),
            source: None,
        };

        let complex_data = ComplexTypeDefData {
            name: Some(schema_set.name_table.add("MyComplexType")),
            target_namespace: None,
            base_type: None,
            derivation_method: None,
            content: ComplexContentResult::Complex(ComplexContentDefResult {
                particle: Some(content_particle),
                derivation: crate::parser::frames::DerivationMethod::Restriction,
                mixed: false,
                mixed_explicit: false,
                base_type: None,
                open_content: None,
                attributes: Vec::new(),
                attribute_groups: Vec::new(),
                attribute_wildcard: None,
                assertions: Vec::new(),
                id: None,
                derivation_id: None,
                source: None,
            }),
            open_content: None,
            attributes: Vec::new(),
            attribute_groups: Vec::new(),
            attribute_wildcard: None,
            mixed: false,
            is_abstract: false,
            final_derivation: DerivationSet::empty(),
            block: DerivationSet::empty(),
            default_attributes_apply: true,
            id: None,
            #[cfg(feature = "xsd11")]
            assertions: Vec::new(),
            #[cfg(feature = "xsd11")]
            xpath_default_namespace: None,
            annotation: None,
            source: None,
            resolved_base_type: None,
            resolved_attribute_groups: Vec::new(),
            resolved_attributes: Vec::new(),
            resolved_content_particle_types: Vec::new(),
            resolved_content_particle_elements: Vec::new(),
            resolved_simple_content_type: None,
            redefine_original: None,
        };

        let ct_key = schema_set.arenas.alloc_complex_type(complex_data);

        let result = assemble_inline_types(&mut schema_set);
        assert!(result.is_ok());
        let stats = result.unwrap();
        assert!(stats.total_inline_types >= 1);

        // Verify resolved_content_particle_types has the type set
        let ct = schema_set.arenas.complex_types.get(ct_key).unwrap();
        assert_eq!(ct.resolved_content_particle_types.len(), 1);
        assert!(ct.resolved_content_particle_types[0].is_some());
    }

    #[test]
    fn test_nested_inline_type_in_model_group() {
        use crate::arenas::ModelGroupData;
        use crate::parser::frames::{Compositor, ElementFrameResult, ModelGroupDefResult};

        let mut schema_set = SchemaSet::new();

        let elem_name = schema_set.name_table.add("nested_elem");
        let inline_type = Box::new(create_simple_type_frame(SimpleTypeVariety::Atomic));

        // Create a named model group:
        //   <xs:group name="G">
        //     <xs:sequence>
        //       <xs:choice>
        //         <xs:element name="nested_elem">
        //           <xs:simpleType>...</xs:simpleType>
        //         </xs:element>
        //       </xs:choice>
        //     </xs:sequence>
        //   </xs:group>
        let group_data = ModelGroupData {
            name: Some(schema_set.name_table.add("G")),
            target_namespace: None,
            ref_name: None,
            compositor: Some(Compositor::Sequence),
            particles: vec![ParticleResult {
                term: ParticleTerm::Group(ModelGroupDefResult {
                    name: None,
                    ref_name: None,
                    compositor: Some(Compositor::Choice),
                    particles: vec![ParticleResult {
                        term: ParticleTerm::Element(ElementFrameResult {
                            name: Some(elem_name),
                            ref_name: None,
                            target_namespace: None,
                            type_ref: None,
                            inline_type: Some(inline_type),
                            substitution_group: vec![],
                            default_value: None,
                            fixed_value: None,
                            nillable: false,
                            is_abstract: false,
                            min_occurs: 1,
                            max_occurs: Some(1),
                            block: Default::default(),
                            final_derivation: Default::default(),
                            form: None,
                            id: None,
                            alternatives: vec![],
                            identity_constraints: vec![],
                            identity_constraint_refs: vec![],
                            annotation: None,
                            source: None,
                        }),
                        min_occurs: 1,
                        max_occurs: Some(1),
                        source: None,
                    }],
                    min_occurs: 1,
                    max_occurs: Some(1),
                    id: None,
                    annotation: None,
                    source: None,
                }),
                min_occurs: 1,
                max_occurs: Some(1),
                source: None,
            }],
            min_occurs: 1,
            max_occurs: Some(1),
            id: None,
            annotation: None,
            source: None,
            resolved_ref: None,
            resolved_particles: Vec::new(),
            resolved_particle_types: Vec::new(),
            resolved_particle_elements: Vec::new(),
            redefine_original: None,
            redefine_requires_restriction_check: false,
        };

        let group_key = schema_set.arenas.alloc_model_group(group_data);

        // Run inline assembly
        let result = assemble_inline_types(&mut schema_set);
        assert!(result.is_ok());
        let stats = result.unwrap();
        assert_eq!(stats.model_group_inline_types, 1);
        assert_eq!(stats.total_inline_types, 1);

        // Verify resolved_particle_types has the type at flat index 0
        let group = schema_set.arenas.model_groups.get(group_key).unwrap();
        assert_eq!(group.resolved_particle_types.len(), 1);
        assert!(
            group.resolved_particle_types[0].is_some(),
            "Expected resolved type for nested element at flat index 0"
        );
        assert!(matches!(
            group.resolved_particle_types[0],
            Some(TypeKey::Simple(_))
        ));
    }
}
