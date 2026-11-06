//! The UPA element–element check as released in 0.2.2, kept verbatim as the
//! reference for the differential tests (`upa_differential_tests.rs`).
//!
//! 0.2.3 changed two things, both for compile time only:
//!
//! - `check_upa` / `check_all_group_upa` built the whole substitution-group
//!   map on every call, i.e. once per complex type of the schema set; the
//!   pipeline now builds it once per UPA pass (`upa_substitution_map`).
//! - `check_element_element_conflicts` compared every pair of reachable
//!   element particles per NFA state; it now finds the same first pair through
//!   a name index (`first_element_element_conflict`).
//!
//! The verdicts and the reported errors must not change, so the tests compare
//! the code below with the current code. Only the parts that changed are
//! copied; the element–wildcard and wildcard–wildcard checks and the error
//! conversion are shared.
//!
//! The epsilon closure changed after 0.2.3 (P15): it reuses its visited set
//! and collects the terms in state order instead of a `HashSet`'s order, which
//! fixes the pair a conflict reports. The 0.2.3 closure is copied below; with
//! it the reported pair depends on hashing, so the tests compare it with the
//! current one by the terms reached, and whole checks by verdict, violation
//! kind and state.

use std::collections::HashSet;

use super::{
    check_element_wildcard_conflicts, check_wildcard_wildcard_conflicts, same_particle_origin,
    upa_error_to_schema_error, ReachableTerm, ReachableTerms, UpaError, UpaResult,
};
use crate::compiler::all_group::AllGroupModel;
use crate::compiler::nfa::{NfaTable, NfaTerm, StateId, TransitionKind};
use crate::compiler::substitution::{
    build_substitution_group_map, build_substitution_group_map_with_abstract, SubstitutableNameSet,
    SubstitutionGroupMap,
};
use crate::error::SchemaResult;
use crate::ids::NameId;
use crate::schema::model::{SchemaSet, XsdVersion};

/// Check if two elements have the same qualified name (name + namespace)
pub(super) fn elements_overlap(
    name1: NameId,
    ns1: Option<NameId>,
    name2: NameId,
    ns2: Option<NameId>,
) -> bool {
    name1 == name2 && ns1 == ns2
}

fn element_substitutable_names(
    term: &NfaTerm,
    substitution_sets: &SubstitutionGroupMap,
) -> SubstitutableNameSet {
    match term {
        NfaTerm::Element {
            name,
            namespace,
            element_key,
            ..
        } => {
            if let Some(key) = element_key {
                if let Some(names) = substitution_sets.get(key) {
                    return names.clone();
                }
            }

            let mut names = SubstitutableNameSet::default();
            names.insert((*name, *namespace));
            names
        }
        NfaTerm::Wildcard { .. } => SubstitutableNameSet::default(),
    }
}

fn name_sets_overlap(names1: &SubstitutableNameSet, names2: &SubstitutableNameSet) -> bool {
    if names1.len() > names2.len() {
        return names2.iter().any(|name| names1.contains(name));
    }
    names1.iter().any(|name| names2.contains(name))
}

/// Check element-element conflicts in reachable terms
pub(super) fn check_element_element_conflicts(
    elements: &[ReachableTerm],
    from_state_id: StateId,
    schema_set: &SchemaSet,
    substitution_sets: &SubstitutionGroupMap,
) -> UpaResult<()> {
    let element_sets: Vec<SubstitutableNameSet> = elements
        .iter()
        .map(|elem| element_substitutable_names(&elem.term, substitution_sets))
        .collect();

    for i in 0..elements.len() {
        for j in (i + 1)..elements.len() {
            let elem1 = &elements[i];
            let elem2 = &elements[j];

            // Skip copies of the same particle from occurrence unrolling
            if same_particle_origin(&elem1.origin, &elem2.origin) {
                continue;
            }

            if let (
                NfaTerm::Element {
                    name: name1,
                    namespace: ns1,
                    ..
                },
                NfaTerm::Element {
                    name: name2,
                    namespace: ns2,
                    ..
                },
            ) = (&elem1.term, &elem2.term)
            {
                if elements_overlap(*name1, *ns1, *name2, *ns2)
                    || name_sets_overlap(&element_sets[i], &element_sets[j])
                {
                    let first_name = schema_set
                        .name_table
                        .try_resolve(*name1)
                        .unwrap_or_else(|| "<unknown>".to_string());
                    let second_name = schema_set
                        .name_table
                        .try_resolve(*name2)
                        .unwrap_or_else(|| "<unknown>".to_string());

                    return Err(Box::new(UpaError::ElementElementConflict {
                        first_name,
                        second_name,
                        first_name_id: *name1,
                        first_namespace: *ns1,
                        second_name_id: *name2,
                        second_namespace: *ns2,
                        state_id: from_state_id,
                        first_location: elem1.origin.clone(),
                        second_location: elem2.origin.clone(),
                    }));
                }
            }
        }
    }
    Ok(())
}

/// The substitution map exactly as 0.2.2's `check_upa` built it on each call.
pub(super) fn substitution_map_per_call(schema_set: &SchemaSet) -> SubstitutionGroupMap {
    // Per XSD 1.1 (W3C Bugzilla 4337), abstract elements participate in the
    // substitution group for cos-nonambig (UPA) purposes even though they
    // can't appear in instances.
    if schema_set.is_xsd11() {
        build_substitution_group_map_with_abstract(schema_set)
    } else {
        build_substitution_group_map(schema_set)
    }
}

/// 0.2.2 `check_upa`.
pub(super) fn check_upa(
    nfa: &NfaTable,
    schema_set: &SchemaSet,
    target_namespace: Option<NameId>,
) -> SchemaResult<()> {
    let xsd_version = schema_set.xsd_version;
    let substitution_sets = substitution_map_per_call(schema_set);

    // Check from each state in the NFA
    for state in &nfa.states {
        let reachable = epsilon_closure_with_terms(nfa, state.id);

        // Check element-element conflicts (always an error)
        if let Err(error) = check_element_element_conflicts(
            &reachable.elements,
            state.id,
            schema_set,
            &substitution_sets,
        ) {
            return Err(upa_error_to_schema_error(schema_set, error));
        }

        // Check element-wildcard conflicts (XSD 1.0 only)
        if xsd_version == XsdVersion::V1_0 {
            if let Err(error) = check_element_wildcard_conflicts(
                &reachable.elements,
                &reachable.wildcards,
                state.id,
                target_namespace,
                schema_set,
                &substitution_sets,
            ) {
                return Err(upa_error_to_schema_error(schema_set, error));
            }
        }

        // Check wildcard-wildcard conflicts (always an error)
        if let Err(error) =
            check_wildcard_wildcard_conflicts(&reachable.wildcards, state.id, target_namespace)
        {
            return Err(upa_error_to_schema_error(schema_set, error));
        }
    }

    Ok(())
}

/// 0.2.2 `check_all_group_upa`.
pub(super) fn check_all_group_upa(
    model: &AllGroupModel,
    schema_set: &SchemaSet,
    target_namespace: Option<NameId>,
) -> SchemaResult<()> {
    let substitution_sets = substitution_map_per_call(schema_set);

    let mut elements: Vec<ReachableTerm> = Vec::new();
    let mut wildcards: Vec<ReachableTerm> = Vec::new();
    for particle in &model.particles {
        let reachable = ReachableTerm {
            term: particle.term.clone(),
            origin: particle.source.clone(),
        };
        match &particle.term {
            NfaTerm::Element { .. } => elements.push(reachable),
            NfaTerm::Wildcard { .. } => wildcards.push(reachable),
        }
    }

    // Sentinel state ID since all-groups don't have NFA states. Used only
    // for error reporting context.
    const ALL_GROUP_STATE_ID: StateId = 0;

    if let Err(error) = check_element_element_conflicts(
        &elements,
        ALL_GROUP_STATE_ID,
        schema_set,
        &substitution_sets,
    ) {
        return Err(upa_error_to_schema_error(schema_set, error));
    }

    if let Err(error) =
        check_wildcard_wildcard_conflicts(&wildcards, ALL_GROUP_STATE_ID, target_namespace)
    {
        return Err(upa_error_to_schema_error(schema_set, error));
    }

    Ok(())
}

/// 0.2.3 `epsilon_closure_with_terms`: the closure in a fresh `HashSet` per
/// state, its terms in that set's iteration order.
pub(super) fn epsilon_closure_with_terms(nfa: &NfaTable, start_state: StateId) -> ReachableTerms {
    let mut result = ReachableTerms::default();
    let mut closure = HashSet::new();
    let mut stack = vec![start_state];

    // Compute epsilon closure
    while let Some(state_id) = stack.pop() {
        if !closure.insert(state_id) {
            continue;
        }

        if let Some(state) = nfa.get_state(state_id) {
            for transition in &state.transitions {
                if transition.kind == TransitionKind::Epsilon {
                    stack.push(transition.target);
                }
            }
        }
    }

    // Collect terms from states in the closure that have terms
    // These are the states where we can consume input
    for &state_id in &closure {
        if let Some(state) = nfa.get_state(state_id) {
            if let Some(term) = &state.term {
                let reachable = ReachableTerm {
                    term: term.clone(),
                    origin: state.origin.clone(),
                };

                match term {
                    NfaTerm::Element { .. } => result.elements.push(reachable),
                    NfaTerm::Wildcard { .. } => result.wildcards.push(reachable),
                }
            }
        }
    }

    result
}
