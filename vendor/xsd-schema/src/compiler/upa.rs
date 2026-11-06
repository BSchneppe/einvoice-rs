//! Unique Particle Attribution (UPA) constraint checking
//!
//! UPA ensures that during validation, each element can be unambiguously attributed
//! to exactly one particle in the content model. This module implements UPA checking
//! for compiled NFA content models.
//!
//! # XSD Version Behavior
//!
//! | Conflict Type        | XSD 1.0 | XSD 1.1            |
//! |---------------------|---------|---------------------|
//! | Element-Element     | Error   | Error               |
//! | Element-Wildcard    | Error   | Allowed (elem wins) |
//! | Wildcard-Wildcard   | Error   | Error               |

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::hash::BuildHasherDefault;

use ahash::AHasher;
use thiserror::Error;

use crate::error::{SchemaError, SchemaResult};
use crate::ids::NameId;
use crate::parser::location::SourceRef;
use crate::schema::model::{SchemaSet, XsdVersion};
use crate::types::complex::{not_qnames_exclude, other_matches_namespace, NamespaceConstraint};

use super::all_group::AllGroupModel;
use super::nfa::{NfaTable, NfaTerm, StateId, TransitionKind};
use super::substitution::{
    build_substitution_group_map, build_substitution_group_map_with_abstract, SubstitutableNameSet,
    SubstitutionGroupMap,
};

/// Result type for internal UPA checking operations
type UpaResult<T> = Result<T, Box<UpaError>>;

/// Errors that can occur during UPA checking
#[derive(Error, Debug, Clone)]
#[allow(clippy::enum_variant_names)]
enum UpaError {
    /// Two element particles can match the same element
    #[error(
        "UPA violation: elements '{first_name}' and '{second_name}' conflict at state {state_id}"
    )]
    ElementElementConflict {
        first_name: String,
        second_name: String,
        first_name_id: NameId,
        first_namespace: Option<NameId>,
        second_name_id: NameId,
        second_namespace: Option<NameId>,
        state_id: StateId,
        first_location: Option<SourceRef>,
        second_location: Option<SourceRef>,
    },

    /// An element particle and wildcard can match the same element
    #[error("UPA violation: element '{element_name}' conflicts with wildcard at state {state_id}")]
    ElementWildcardConflict {
        element_name: String,
        element_name_id: NameId,
        element_namespace: Option<NameId>,
        wildcard_constraint: NamespaceConstraint,
        state_id: StateId,
        element_location: Option<SourceRef>,
        wildcard_location: Option<SourceRef>,
    },

    /// Two wildcards can match overlapping namespaces
    #[error("UPA violation: wildcards conflict at state {state_id}")]
    WildcardWildcardConflict {
        first_constraint: NamespaceConstraint,
        second_constraint: NamespaceConstraint,
        state_id: StateId,
        first_location: Option<SourceRef>,
        second_location: Option<SourceRef>,
    },
}

impl UpaError {
    /// Get the first source location if available
    pub fn first_location(&self) -> Option<&SourceRef> {
        match self {
            UpaError::ElementElementConflict { first_location, .. } => first_location.as_ref(),
            UpaError::ElementWildcardConflict {
                element_location, ..
            } => element_location.as_ref(),
            UpaError::WildcardWildcardConflict { first_location, .. } => first_location.as_ref(),
        }
    }

    /// Get the second source location if available
    pub fn second_location(&self) -> Option<&SourceRef> {
        match self {
            UpaError::ElementElementConflict {
                second_location, ..
            } => second_location.as_ref(),
            UpaError::ElementWildcardConflict {
                wildcard_location, ..
            } => wildcard_location.as_ref(),
            UpaError::WildcardWildcardConflict {
                second_location, ..
            } => second_location.as_ref(),
        }
    }
}

fn upa_error_to_schema_error(schema_set: &SchemaSet, error: Box<UpaError>) -> SchemaError {
    let primary = error
        .first_location()
        .and_then(|source| schema_set.source_maps.locate(source));
    let secondary = error
        .second_location()
        .and_then(|source| schema_set.source_maps.locate(source));
    let mut message = error.to_string();
    if let Some(location) = secondary {
        message.push_str(&format!(" (see also {})", location));
    }
    SchemaError::structural("cos-nonambig", message, primary)
}

/// A term reachable from a state via epsilon transitions
#[derive(Debug, Clone)]
struct ReachableTerm {
    term: NfaTerm,
    origin: Option<SourceRef>,
}

/// Check if two reachable terms originate from the same particle.
///
/// When occurrence bounds are unrolled (e.g., `a{1,2}` → two NFA copies of `a`),
/// the copies share the same source location. Conflicts between copies of the
/// same particle are not UPA violations — the validator extends the count of
/// the current particle rather than choosing between distinct particles.
fn same_particle_origin(a: &Option<SourceRef>, b: &Option<SourceRef>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.doc_id == b.doc_id && a.span == b.span,
        _ => false,
    }
}

/// Collection of reachable terms categorized by type
#[derive(Debug, Default)]
struct ReachableTerms {
    elements: Vec<ReachableTerm>,
    wildcards: Vec<ReachableTerm>,
}

/// Working storage for the epsilon closures of one NFA's states, reused from
/// state to state ([`check_upa_with_substitutions`] computes one closure per
/// state).
struct ClosureScratch {
    /// `visited[s]`: state `s` is in the closure being computed. All false
    /// between two closures.
    visited: Vec<bool>,
    stack: Vec<StateId>,
    /// The states of the closure being computed.
    closure: Vec<StateId>,
}

impl ClosureScratch {
    fn new(nfa: &NfaTable) -> Self {
        ClosureScratch {
            visited: vec![false; nfa.states.len()],
            stack: Vec::new(),
            closure: Vec::new(),
        }
    }
}

/// [`epsilon_closure_with_terms_in`] with fresh working storage.
#[cfg(test)]
fn epsilon_closure_with_terms(nfa: &NfaTable, start_state: StateId) -> ReachableTerms {
    epsilon_closure_with_terms_in(nfa, start_state, &mut ClosureScratch::new(nfa))
}

/// Compute epsilon closure and collect all reachable terms from a start state.
///
/// Uses DFS traversal following **only** `TransitionKind::Epsilon` transitions.
/// Counter transitions are not followed — the pipeline compiles a separate
/// counter-free NFA for UPA checking by capping occurrence bounds to at most 2
/// (via `compile_content_model_for_upa`), so all transitions in the UPA NFA
/// are epsilon transitions.
///
/// The terms are collected in increasing state order. The checks report the
/// first conflict in this order, so the pair a `cos-nonambig` error names is
/// the same on every run.
fn epsilon_closure_with_terms_in(
    nfa: &NfaTable,
    start_state: StateId,
    scratch: &mut ClosureScratch,
) -> ReachableTerms {
    let mut result = ReachableTerms::default();
    let ClosureScratch {
        visited,
        stack,
        closure,
    } = scratch;
    stack.clear();
    closure.clear();
    stack.push(start_state);

    // Compute epsilon closure
    while let Some(state_id) = stack.pop() {
        // A state outside the table has no transitions and no term.
        let Some(seen) = visited.get_mut(state_id as usize) else {
            continue;
        };
        if *seen {
            continue;
        }
        *seen = true;
        closure.push(state_id);

        if let Some(state) = nfa.get_state(state_id) {
            for transition in &state.transitions {
                if transition.kind == TransitionKind::Epsilon {
                    stack.push(transition.target);
                }
            }
        }
    }
    for &state_id in closure.iter() {
        visited[state_id as usize] = false;
    }
    closure.sort_unstable();

    // Collect terms from states in the closure that have terms
    // These are the states where we can consume input
    for &state_id in closure.iter() {
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

/// Check if an element namespace matches a wildcard constraint
fn element_wildcard_overlap(
    element_namespace: Option<NameId>,
    wildcard: &NamespaceConstraint,
    target_namespace: Option<NameId>,
) -> bool {
    match wildcard {
        NamespaceConstraint::Any => true,

        NamespaceConstraint::Other => {
            // ##other matches any namespace except the target namespace and
            // ·absent· — in both XSD versions (Structures §3.10.2.2)
            other_matches_namespace(element_namespace, target_namespace)
        }

        NamespaceConstraint::TargetNamespace => {
            // ##targetNamespace matches only the target namespace
            element_namespace == target_namespace
        }

        NamespaceConstraint::Local => {
            // ##local matches only elements with no namespace
            element_namespace.is_none()
        }

        NamespaceConstraint::List(namespaces) => {
            // List matches if element namespace is in the list
            // None in the list represents absent namespace
            namespaces.contains(&element_namespace)
        }

        NamespaceConstraint::Not(excluded) => !excluded.contains(&element_namespace),
    }
}

/// Check if two wildcard constraints can match overlapping namespaces
fn wildcards_overlap(
    wc1: &NamespaceConstraint,
    wc2: &NamespaceConstraint,
    target_namespace: Option<NameId>,
) -> bool {
    use NamespaceConstraint::*;

    match (wc1, wc2) {
        // Not(a) vs Not(b): both exclude finite sets, infinite remainder overlaps
        (Not(_), Not(_)) => true,

        // Not(a) vs Any / Any vs Not(a): always overlaps
        (Not(_), Any) | (Any, Not(_)) => true,

        // Not(a) vs Other / Other vs Not(a): both accept "almost everything", overlaps
        (Not(_), Other) | (Other, Not(_)) => true,

        // Not(a) vs TargetNamespace: overlaps if target ns is not excluded
        (Not(a), TargetNamespace) | (TargetNamespace, Not(a)) => !a.contains(&target_namespace),

        // Not(a) vs Local: overlaps if absent ns is not excluded
        (Not(a), Local) | (Local, Not(a)) => !a.contains(&None),

        // Not(a) vs List(b): overlaps if any ns in list is not excluded
        (Not(a), List(b)) | (List(b), Not(a)) => b.iter().any(|ns| !a.contains(ns)),

        // Any matches everything, so overlaps with anything
        (Any, _) | (_, Any) => true,

        // Two ##other constraints overlap (they both match "not target namespace")
        (Other, Other) => true,

        // ##other vs ##targetNamespace: no overlap (mutually exclusive)
        (Other, TargetNamespace) | (TargetNamespace, Other) => false,

        // ##other vs ##local: never overlap — ##other excludes ·absent·, the
        // only namespace ##local admits (Structures §3.10.2.2)
        (Other, Local) | (Local, Other) => other_matches_namespace(None, target_namespace),

        // ##other vs list: overlap if list has any ns other than target
        (Other, List(ns_list)) | (List(ns_list), Other) => ns_list
            .iter()
            .any(|ns| other_matches_namespace(*ns, target_namespace)),

        // ##targetNamespace vs ##targetNamespace: overlap (both match target ns)
        (TargetNamespace, TargetNamespace) => true,

        // ##targetNamespace vs ##local: overlap only if target namespace is None
        (TargetNamespace, Local) | (Local, TargetNamespace) => target_namespace.is_none(),

        // ##targetNamespace vs list: overlap if list contains target namespace
        (TargetNamespace, List(ns_list)) | (List(ns_list), TargetNamespace) => {
            ns_list.contains(&target_namespace)
        }

        // ##local vs ##local: overlap (both match None)
        (Local, Local) => true,

        // ##local vs list: overlap if list contains None
        (Local, List(ns_list)) | (List(ns_list), Local) => ns_list.contains(&None),

        // List vs list: overlap if any namespace is in both lists
        (List(ns_list1), List(ns_list2)) => ns_list1.iter().any(|ns| ns_list2.contains(ns)),
    }
}

/// The two ways two element particles compete (§3.8.6.4 Unique Particle
/// Attribution: "A content model must not contain two element particles which
/// compete with each other"), as keys of the index
/// [`first_element_element_conflict`] builds. An element item matches an
/// element declaration if it has the declaration's expanded name, or if its
/// expanded name resolves to a declaration substitutable for it (§3.9.4.1.2,
/// "matches").
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum CompetitionKey {
    /// The particle's own expanded name `(local name, namespace)`. Two
    /// particles with the same expanded name compete, whatever their
    /// substitution sets (an abstract head's set under XSD 1.0 omits its own
    /// name and can be empty).
    Name(NameId, Option<NameId>),
    /// One expanded name of the particle's substitutable set: the head and
    /// its substitutable members from the [`SubstitutionGroupMap`], or the
    /// particle's own name when the map has no entry for it
    /// ([`element_substitutable_names`]).
    Substitutable(NameId, Option<NameId>),
}

/// Per-key state of [`first_element_element_conflict`]: the first particle
/// that carries the key, and whether a later particle with a different
/// origin has already been paired with it.
#[derive(Clone, Copy)]
struct KeyRun {
    first: usize,
    paired: bool,
}

/// Find the first competing pair of element particles in `elements`.
///
/// Two particles `i < j` compete when they do not come from the same particle
/// (`same_particle_origin`) and either their expanded names are equal or their
/// substitutable sets share a name. "First" is the pairwise order: the
/// smallest `i`, then the smallest `j`, which is the pair the former
/// all-pairs scan reported (O(k²) set intersections per NFA state).
///
/// One pass over the particles in order with an index from key to the first
/// particle that carries it, O(Σ |substitutable set|) per state:
///
/// - Every competing pair shares a key; conversely two particles sharing a key
///   compete unless they come from the same particle.
/// - "Same origin" is an equivalence on particles with a source (copies made
///   by occurrence unrolling); a particle without a source is in a class of
///   its own. So within the particles carrying one key, the first particle
///   `f` competes with a later one iff some later particle's class differs
///   from `f`'s, and the smallest such partner is the first such particle.
/// - If any pair `(i, j)` competes through a key whose first particle `f` is
///   before `i`, then `f` competes too — with `i` when their classes differ,
///   otherwise with `j`. So the smallest competing `i` is the first particle
///   of some key, and its smallest partner is the minimum, over the keys it
///   comes first in, of the first particle in a different class.
///
/// Hence the answer is the minimum of `(first, partner)` over the keys, which
/// the pass records as it finds each key's partner. Neither the order of the
/// map's iteration nor that of a substitutable set enters the result.
fn first_element_element_conflict(
    elements: &[ReachableTerm],
    substitution_sets: &SubstitutionGroupMap,
) -> Option<(usize, usize)> {
    let mut index: HashMap<CompetitionKey, KeyRun, BuildHasherDefault<AHasher>> =
        HashMap::with_capacity_and_hasher(2 * elements.len(), Default::default());
    let mut first_pair: Option<(usize, usize)> = None;

    for (j, elem) in elements.iter().enumerate() {
        let NfaTerm::Element {
            name,
            namespace,
            element_key,
            ..
        } = &elem.term
        else {
            continue;
        };

        let mut visit = |key: CompetitionKey| match index.entry(key) {
            Entry::Vacant(slot) => {
                slot.insert(KeyRun {
                    first: j,
                    paired: false,
                });
            }
            Entry::Occupied(mut slot) => {
                let run = slot.get_mut();
                if !run.paired && !same_particle_origin(&elements[run.first].origin, &elem.origin) {
                    run.paired = true;
                    let pair = (run.first, j);
                    first_pair = Some(first_pair.map_or(pair, |best| best.min(pair)));
                }
            }
        };

        visit(CompetitionKey::Name(*name, *namespace));
        match element_key.and_then(|key| substitution_sets.get(&key)) {
            Some(names) => {
                for &(member_name, member_namespace) in names {
                    visit(CompetitionKey::Substitutable(member_name, member_namespace));
                }
            }
            None => visit(CompetitionKey::Substitutable(*name, *namespace)),
        }
    }

    first_pair
}

/// Check element-element conflicts in reachable terms
fn check_element_element_conflicts(
    elements: &[ReachableTerm],
    from_state_id: StateId,
    schema_set: &SchemaSet,
    substitution_sets: &SubstitutionGroupMap,
) -> UpaResult<()> {
    let Some((i, j)) = first_element_element_conflict(elements, substitution_sets) else {
        return Ok(());
    };
    let (elem1, elem2) = (&elements[i], &elements[j]);
    let (
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
    else {
        unreachable!("first_element_element_conflict pairs element terms only");
    };

    let first_name = schema_set
        .name_table
        .try_resolve(*name1)
        .unwrap_or_else(|| "<unknown>".to_string());
    let second_name = schema_set
        .name_table
        .try_resolve(*name2)
        .unwrap_or_else(|| "<unknown>".to_string());

    Err(Box::new(UpaError::ElementElementConflict {
        first_name,
        second_name,
        first_name_id: *name1,
        first_namespace: *ns1,
        second_name_id: *name2,
        second_namespace: *ns2,
        state_id: from_state_id,
        first_location: elem1.origin.clone(),
        second_location: elem2.origin.clone(),
    }))
}

/// Check element-wildcard conflicts in reachable terms (XSD 1.0 only)
fn check_element_wildcard_conflicts(
    elements: &[ReachableTerm],
    wildcards: &[ReachableTerm],
    from_state_id: StateId,
    target_namespace: Option<NameId>,
    schema_set: &SchemaSet,
    substitution_sets: &SubstitutionGroupMap,
) -> UpaResult<()> {
    for elem in elements {
        if let NfaTerm::Element {
            name,
            namespace: elem_ns,
            ..
        } = &elem.term
        {
            // Collect all QNames this element particle can match
            // (head + substitution group members)
            let matchable_names = element_substitutable_names(&elem.term, substitution_sets);

            for wc in wildcards {
                // Skip copies of the same particle from occurrence unrolling
                if same_particle_origin(&elem.origin, &wc.origin) {
                    continue;
                }

                if let NfaTerm::Wildcard {
                    namespace_constraint,
                    not_qnames,
                    ..
                } = &wc.term
                {
                    // A conflict exists if ANY matchable QName (head or substitution member)
                    // both falls within the wildcard's namespace constraint AND is not
                    // excluded by notQName.
                    let has_conflict = matchable_names.iter().any(|(match_name, match_ns)| {
                        if !element_wildcard_overlap(
                            *match_ns,
                            namespace_constraint,
                            target_namespace,
                        ) {
                            return false;
                        }
                        // Check if this specific QName is excluded by notQName
                        !not_qnames_exclude(not_qnames, *match_ns, *match_name)
                    });
                    if has_conflict {
                        let element_name = schema_set
                            .name_table
                            .try_resolve(*name)
                            .unwrap_or_else(|| "<unknown>".to_string());

                        return Err(Box::new(UpaError::ElementWildcardConflict {
                            element_name,
                            element_name_id: *name,
                            element_namespace: *elem_ns,
                            wildcard_constraint: namespace_constraint.clone(),
                            state_id: from_state_id,
                            element_location: elem.origin.clone(),
                            wildcard_location: wc.origin.clone(),
                        }));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Check wildcard-wildcard conflicts in reachable terms
fn check_wildcard_wildcard_conflicts(
    wildcards: &[ReachableTerm],
    from_state_id: StateId,
    target_namespace: Option<NameId>,
) -> UpaResult<()> {
    for i in 0..wildcards.len() {
        for j in (i + 1)..wildcards.len() {
            let wc1 = &wildcards[i];
            let wc2 = &wildcards[j];

            // Skip copies of the same particle from occurrence unrolling
            if same_particle_origin(&wc1.origin, &wc2.origin) {
                continue;
            }

            if let (
                NfaTerm::Wildcard {
                    namespace_constraint: constraint1,
                    ..
                },
                NfaTerm::Wildcard {
                    namespace_constraint: constraint2,
                    ..
                },
            ) = (&wc1.term, &wc2.term)
            {
                if wildcards_overlap(constraint1, constraint2, target_namespace) {
                    return Err(Box::new(UpaError::WildcardWildcardConflict {
                        first_constraint: constraint1.clone(),
                        second_constraint: constraint2.clone(),
                        state_id: from_state_id,
                        first_location: wc1.origin.clone(),
                        second_location: wc2.origin.clone(),
                    }));
                }
            }
        }
    }
    Ok(())
}

/// Check UPA constraints for an NFA content model
///
/// This function analyzes the NFA to detect ambiguities where the same input
/// element could match multiple particles in the content model.
///
/// # Arguments
///
/// * `nfa` - The compiled NFA content model to check
/// * `schema_set` - The schema set for name resolution and version info
/// * `target_namespace` - The target namespace of the schema containing this content model
///
/// # Returns
///
/// * `Ok(())` if the content model satisfies UPA
/// * `Err(SchemaError)` describing the first conflict found
///
/// # XSD Version Differences
///
/// - XSD 1.0: Element-element, element-wildcard, and wildcard-wildcard conflicts are all errors
/// - XSD 1.1: Element-wildcard conflicts are allowed (element declaration takes priority)
pub fn check_upa(
    nfa: &NfaTable,
    schema_set: &SchemaSet,
    target_namespace: Option<NameId>,
) -> SchemaResult<()> {
    check_upa_with_substitutions(
        nfa,
        schema_set,
        target_namespace,
        &upa_substitution_map(schema_set),
    )
}

/// Build the substitution-group map the UPA check runs with.
///
/// The map depends on the schema set only, so a pass over many content models
/// builds it once and passes it to [`check_upa_with_substitutions`] and
/// [`check_all_group_upa_with_substitutions`]; building it costs a walk of
/// every element declaration.
pub(crate) fn upa_substitution_map(schema_set: &SchemaSet) -> SubstitutionGroupMap {
    // Per XSD 1.1 (W3C Bugzilla 4337), abstract elements participate in the
    // substitution group for cos-nonambig (UPA) purposes even though they
    // can't appear in instances.
    if schema_set.is_xsd11() {
        build_substitution_group_map_with_abstract(schema_set)
    } else {
        build_substitution_group_map(schema_set)
    }
}

/// [`check_upa`] with a substitution-group map built by
/// [`upa_substitution_map`] for the same schema set.
pub(crate) fn check_upa_with_substitutions(
    nfa: &NfaTable,
    schema_set: &SchemaSet,
    target_namespace: Option<NameId>,
    substitution_sets: &SubstitutionGroupMap,
) -> SchemaResult<()> {
    let xsd_version = schema_set.xsd_version;
    let mut scratch = ClosureScratch::new(nfa);

    // Check from each state in the NFA
    for state in &nfa.states {
        let reachable = epsilon_closure_with_terms_in(nfa, state.id, &mut scratch);

        // Check element-element conflicts (always an error)
        if let Err(error) = check_element_element_conflicts(
            &reachable.elements,
            state.id,
            schema_set,
            substitution_sets,
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
                substitution_sets,
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

/// Check UPA constraints for an all-group content model (XSD 1.1).
///
/// XSD 1.0 forbids wildcards in `xs:all`, so only element-element conflicts
/// matter; the no-wildcards check is enforced separately by
/// `validate_all_group_constraints`. XSD 1.1 allows wildcards but requires:
///
/// - Element-element conflicts (same name or substitution group overlap):
///   always an error.
/// - Wildcard-wildcard conflicts (overlapping namespace constraints): always
///   an error.
/// - Element-wildcard conflicts: allowed in XSD 1.1 (element wins by
///   §3.8.6.4 Particle Subsumption priority).
///
/// All-group particles are independent (not part of an NFA), so the check
/// runs over the particle list directly without epsilon-closure analysis.
pub fn check_all_group_upa(
    model: &AllGroupModel,
    schema_set: &SchemaSet,
    target_namespace: Option<NameId>,
) -> SchemaResult<()> {
    check_all_group_upa_with_substitutions(
        model,
        schema_set,
        target_namespace,
        &upa_substitution_map(schema_set),
    )
}

/// [`check_all_group_upa`] with a substitution-group map built by
/// [`upa_substitution_map`] for the same schema set.
pub(crate) fn check_all_group_upa_with_substitutions(
    model: &AllGroupModel,
    schema_set: &SchemaSet,
    target_namespace: Option<NameId>,
    substitution_sets: &SubstitutionGroupMap,
) -> SchemaResult<()> {
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
        substitution_sets,
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

/// The 0.2.2 element–element check (all pairs per state, substitution map
/// built per call), kept verbatim for the differential tests.
#[cfg(test)]
#[path = "upa_reference.rs"]
mod reference;

#[cfg(test)]
#[path = "upa_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "upa_differential_tests.rs"]
mod differential_tests;
