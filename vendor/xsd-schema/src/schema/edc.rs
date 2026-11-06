//! XSD 1.1 dynamic Element Declarations Consistent (§3.4.6.4 / cvc-complex-type rule 5).
//!
//! When a wildcard accepts an element and resolves it (via lax/strict process
//! contents) to a global element declaration, that *governing* declaration must
//! be consistent with any locally declared element binding for the same QName
//! in the same content model — including bindings inherited from base types.
//! This is the "dynamic EDC" check.
//!
//! Schema-time EDC (cos-element-consistent) is a separate concern; this module
//! covers only the runtime obligation that fires after a wildcard match.

#![cfg(feature = "xsd11")]

use std::collections::HashMap;

use crate::compiler::SubstitutionGroupMap;
use crate::ids::{ComplexTypeKey, ElementKey, NameId, TypeKey};
use crate::parser::frames::{ComplexContentResult, ParticleResult, ParticleTerm};
use crate::schema::model::{DerivationSet, SchemaSet};

/// Default binding tracked for a QName in a content model.
#[derive(Debug, Clone, Copy)]
pub enum DefaultBinding {
    /// A local element declaration governs this QName. The element key may be
    /// the head of a substitution group when the binding was reached through
    /// substitution propagation.
    Element {
        key: ElementKey,
        /// Resolved type of the binding, captured eagerly because the local
        /// arena entry sometimes leaves `resolved_type` empty for inline
        /// shapes that resolve through `resolved_content_particle_types`.
        resolved_type: Option<TypeKey>,
    },
    /// A wildcard with `processContents = strict` covers this binding.
    #[allow(dead_code)]
    Strict,
    /// A wildcard with `processContents = lax`.
    #[allow(dead_code)]
    Lax,
    /// A wildcard with `processContents = skip`.
    #[allow(dead_code)]
    Skip,
}

/// Result of a dynamic EDC check.
#[derive(Debug, Clone)]
pub enum EdcOutcome {
    /// No local binding for this QName — wildcard match is unconstrained.
    NoLocalBinding,
    /// Local binding subsumes the governing decl/type — match is valid.
    Subsumes,
    /// Local binding does NOT subsume — emit `cvc-complex-type.5`.
    Mismatch { reason: String },
}

/// Dynamic EDC check for a wildcard-matched element.
pub fn check_dynamic_edc(
    schema_set: &SchemaSet,
    subst_groups: Option<&SubstitutionGroupMap>,
    parent_ct: ComplexTypeKey,
    qname: (Option<NameId>, NameId),
    governing_type: Option<TypeKey>,
    governing_decl: Option<ElementKey>,
) -> EdcOutcome {
    let bindings = collect_local_bindings(schema_set, subst_groups, parent_ct);
    let Some(local) = bindings.get(&qname).copied() else {
        return EdcOutcome::NoLocalBinding;
    };

    if let DefaultBinding::Element { resolved_type, .. } = local {
        let Some(local_type) = resolved_type else {
            return EdcOutcome::Subsumes;
        };
        let gov_type = governing_type
            .or_else(|| governing_decl.and_then(|k| schema_set.arenas.elements[k].resolved_type));
        let Some(gov_type) = gov_type else {
            return EdcOutcome::Subsumes;
        };
        if !schema_set.is_type_derived_from(gov_type, local_type, DerivationSet::empty()) {
            return EdcOutcome::Mismatch {
                reason: "governing type is not validly substitutable for the local element's type"
                    .to_string(),
            };
        }
    }
    EdcOutcome::Subsumes
}

/// Build a (namespace, name) → DefaultBinding map for a complex type's
/// content model, walking up the base-type chain so that bindings inherited
/// from a restricted/extended base are visible.
pub fn collect_local_bindings(
    schema_set: &SchemaSet,
    subst_groups: Option<&SubstitutionGroupMap>,
    ct_key: ComplexTypeKey,
) -> HashMap<(Option<NameId>, NameId), DefaultBinding> {
    let mut out = HashMap::new();
    let mut visited = std::collections::HashSet::new();
    let mut current = Some(ct_key);
    while let Some(k) = current {
        if !visited.insert(k) {
            break;
        }
        let ct = &schema_set.arenas.complex_types[k];
        let target_ns = ct.target_namespace;
        if let ComplexContentResult::Complex(content) = &ct.content {
            if let Some(particle) = content.particle.as_ref() {
                let mut flat_idx = 0usize;
                walk_particle(
                    schema_set,
                    subst_groups,
                    particle,
                    target_ns,
                    &ct.resolved_content_particle_elements,
                    &ct.resolved_content_particle_types,
                    &mut flat_idx,
                    &mut out,
                    0,
                );
            }
        }
        // Walk up the base type chain (extension or restriction).
        current = match ct.resolved_base_type {
            Some(TypeKey::Complex(parent)) => Some(parent),
            _ => None,
        };
    }
    out
}

/// Walk a particle tree, recording each Element particle's binding into `out`.
/// `flat_idx` and the parallel `local_keys` / `local_types` arrays follow the
/// same flat depth-first scheme used by `allocate_content_particle_elements`.
#[allow(clippy::too_many_arguments)]
fn walk_particle(
    schema_set: &SchemaSet,
    subst_groups: Option<&SubstitutionGroupMap>,
    particle: &ParticleResult,
    target_ns: Option<NameId>,
    local_keys: &[Option<ElementKey>],
    local_types: &[Option<TypeKey>],
    flat_idx: &mut usize,
    out: &mut HashMap<(Option<NameId>, NameId), DefaultBinding>,
    depth: usize,
) {
    if depth > schema_set.recursion_bound(64) {
        return;
    }
    match &particle.term {
        ParticleTerm::Element(elem) => {
            let (qname_ns, qname_local, key, resolved_type) = if let Some(ref_qn) = &elem.ref_name {
                let key = schema_set.lookup_element(ref_qn.namespace, ref_qn.local_name);
                let ty = key.and_then(|k| schema_set.arenas.elements[k].resolved_type);
                let idx = *flat_idx;
                *flat_idx += 1;
                let _ = idx; // ref slot is None in local_keys; ignore
                (ref_qn.namespace, ref_qn.local_name, key, ty)
            } else if let Some(name) = elem.name {
                let ns = elem.target_namespace.or(target_ns);
                let idx = *flat_idx;
                *flat_idx += 1;
                let key = local_keys.get(idx).copied().flatten();
                let ty = local_types
                    .get(idx)
                    .copied()
                    .flatten()
                    .or_else(|| key.and_then(|k| schema_set.arenas.elements[k].resolved_type));
                (ns, name, key, ty)
            } else {
                return;
            };

            let Some(binding_key) = key else { return };

            // Insert if absent: derived bindings shadow base bindings.
            out.entry((qname_ns, qname_local))
                .or_insert(DefaultBinding::Element {
                    key: binding_key,
                    resolved_type,
                });

            // Substitution-group members: their own QName resolves to the
            // head's binding (head's resolved_type).
            if let Some(map) = subst_groups {
                if let Some(members) = map.get(&binding_key) {
                    for &(member_name, member_ns) in members.iter() {
                        if (member_ns, member_name) == (qname_ns, qname_local) {
                            continue;
                        }
                        out.entry((member_ns, member_name))
                            .or_insert(DefaultBinding::Element {
                                key: binding_key,
                                resolved_type,
                            });
                    }
                }
            }
        }
        ParticleTerm::Group(mg) => {
            if let Some(ref_qn) = &mg.ref_name {
                if let Some(group_key) =
                    schema_set.lookup_model_group(ref_qn.namespace, ref_qn.local_name)
                {
                    let group = &schema_set.arenas.model_groups[group_key];
                    let mut group_flat_idx = 0usize;
                    for child in &group.particles {
                        walk_particle(
                            schema_set,
                            subst_groups,
                            child,
                            group.target_namespace.or(target_ns),
                            &group.resolved_particle_elements,
                            &group.resolved_particle_types,
                            &mut group_flat_idx,
                            out,
                            depth + 1,
                        );
                    }
                }
                // Group refs do NOT increment our flat_idx (mirrors
                // `collect_content_particle_elements_recursive`).
            } else {
                for child in &mg.particles {
                    walk_particle(
                        schema_set,
                        subst_groups,
                        child,
                        target_ns,
                        local_keys,
                        local_types,
                        flat_idx,
                        out,
                        depth + 1,
                    );
                }
            }
        }
        ParticleTerm::Any(_) => {
            // Wildcards do not increment flat_idx.
        }
    }
}
