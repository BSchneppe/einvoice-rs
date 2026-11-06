//! Content model state dispatch for instance validation
//!
//! Wraps NFA and AllGroup content model states into a unified enum,
//! providing a common interface for advancing the content model
//! and checking completion.

use std::sync::Arc;

use crate::compiler::{
    epsilon_closure, term_matches_with_substitution, ActiveStates, AllGroupModel, AllGroupState,
    ContentModelLimitExceeded, ContentModelMatcher, NfaTable, NfaTerm,
    OpenContentMode as AllGroupOpenContentMode, StateSet, SubstitutionGroupMap, TermMatchResult,
};
use crate::ids::{ElementKey, NameId, TypeKey};
use crate::schema::model::XsdVersion;
use crate::types::complex::{
    not_qnames_exclude, NamespaceConstraint, OpenContentMode as TypesOpenContentMode,
    ProcessContents,
};

/// Open content information carried through validation
#[derive(Debug, Clone)]
pub struct OpenContentInfo {
    /// Open content mode
    pub mode: TypesOpenContentMode,
    /// Namespace constraint for allowed namespaces
    pub namespace_constraint: NamespaceConstraint,
    /// How to process matched content
    pub process_contents: ProcessContents,
    /// QNames excluded by notQName (pre-expanded concrete pairs)
    pub not_qnames: Vec<(Option<NameId>, NameId)>,
}

/// Information about a matched element from the content model
#[derive(Debug, Clone, Copy)]
pub struct ElementMatchInfo {
    /// The element key from the matching NFA term (if any)
    pub element_key: Option<ElementKey>,
    /// The resolved type for local elements (if any)
    pub resolved_type: Option<TypeKey>,
    /// Process contents mode from open content wildcard (if matched via open content)
    pub process_contents: Option<ProcessContents>,
}

/// Unified content model validation state
///
/// Immutable, shareable compiled content model for one complex type.
///
/// Built once per type at [`SchemaValidator`](crate::validation::SchemaValidator)
/// construction and stored in its per-type map. The heavy automaton tables are
/// wrapped in `Arc` so that constructing a per-element [`ContentValidatorState`]
/// only bumps reference counts and allocates the small mutable execution state
/// (active NFA states / all-group counters) — instead of deep-cloning the whole
/// `NfaTable` (nested `Vec`s) for every element instance.
#[derive(Debug, Clone)]
pub enum CompiledContentModel {
    /// Standard NFA content model.
    ///
    /// `initial` is the start-state epsilon closure, computed once here so that
    /// each per-element [`ContentValidatorState`] clones it (a cheap `StateSet`
    /// copy for the common counter-free case) instead of re-running the closure
    /// DFS on every element instance.
    Nfa {
        nfa: Arc<NfaTable>,
        initial: ActiveStates,
    },
    /// NFA content model carrying an open-content wildcard (XSD 1.1).
    NfaWithOpenContent {
        nfa: Arc<NfaTable>,
        initial: ActiveStates,
        open_content: Option<OpenContentInfo>,
    },
    /// All-group content model (unordered particles).
    AllGroup(Arc<AllGroupModel>),
}

impl CompiledContentModel {
    /// Wrap a freshly compiled [`ContentModelMatcher`] into the shareable form,
    /// moving its automaton tables behind `Arc`s. Called once per complex type.
    ///
    /// # Panics
    ///
    /// If the model's initial configuration set exceeds an execution limit
    /// (see [`ContentModelLimitExceeded`]). Use
    /// [`try_from_matcher`](Self::try_from_matcher) to get that as an error;
    /// the validator's load-time preparation does.
    pub fn from_matcher(matcher: ContentModelMatcher) -> Self {
        Self::try_from_matcher(matcher).unwrap_or_else(|e| {
            panic!("CompiledContentModel::from_matcher: {e}; use try_from_matcher")
        })
    }

    /// Fallible form of [`from_matcher`](Self::from_matcher).
    pub fn try_from_matcher(
        matcher: ContentModelMatcher,
    ) -> Result<Self, ContentModelLimitExceeded> {
        Ok(match matcher {
            ContentModelMatcher::Nfa(nfa) => {
                let initial = ActiveStates::try_from_nfa(&nfa)?;
                Self::Nfa {
                    nfa: Arc::new(nfa),
                    initial,
                }
            }
            ContentModelMatcher::AllGroup(model) => Self::AllGroup(Arc::new(model)),
            ContentModelMatcher::WithOpenContent {
                nfa,
                mode,
                wildcard,
            } => {
                let open_content = wildcard.map(|w| OpenContentInfo {
                    mode,
                    namespace_constraint: w.namespace_constraint,
                    process_contents: w.process_contents,
                    not_qnames: w.not_qnames,
                });
                let initial = ActiveStates::try_from_nfa(&nfa)?;
                Self::NfaWithOpenContent {
                    nfa: Arc::new(nfa),
                    initial,
                    open_content,
                }
            }
        })
    }
}

/// Wraps either an NFA-based or AllGroup-based content model into a single
/// enum so that `SchemaValidator` can advance the content model without
/// caring which underlying representation is in use.
///
/// The compiled automaton tables (`NfaTable`, `AllGroupModel`) are held behind
/// `Arc`s shared with the per-type [`CompiledContentModel`]; only the mutable
/// execution state (`active_states`, all-group `state`) is owned per instance.
#[derive(Debug, Clone)]
pub enum ContentValidatorState {
    /// NFA-based content model (sequence, choice, etc.)
    Nfa {
        nfa: Arc<NfaTable>,
        active_states: ActiveStates,
        open_content: Option<OpenContentInfo>,
    },
    /// All-group content model (unordered particles)
    AllGroup {
        model: Arc<AllGroupModel>,
        state: AllGroupState,
        /// `true` once a suffix open-content element has been matched —
        /// further declared all-group particles are no longer accepted
        /// (§3.10.4 suffix semantics: declared content first, then wildcard).
        suffix_locked: bool,
    },
    /// Simple content (text only, no child elements)
    Simple,
    /// Empty content (no children or text)
    Empty,
}

impl ContentValidatorState {
    /// Create a per-element validator state from a shared, pre-compiled model.
    ///
    /// This is the hot path (one call per element instance). It clones only the
    /// `Arc` handles (reference-count bumps) plus any small open-content
    /// metadata, and creates fresh mutable execution state (active NFA states /
    /// all-group counters) — it never deep-clones the automaton tables.
    pub fn from_compiled(compiled: &CompiledContentModel) -> Self {
        match compiled {
            CompiledContentModel::Nfa { nfa, initial } => ContentValidatorState::Nfa {
                nfa: Arc::clone(nfa),
                active_states: initial.clone(),
                open_content: None,
            },
            CompiledContentModel::NfaWithOpenContent {
                nfa,
                initial,
                open_content,
            } => ContentValidatorState::Nfa {
                nfa: Arc::clone(nfa),
                active_states: initial.clone(),
                open_content: open_content.clone(),
            },
            CompiledContentModel::AllGroup(model) => {
                let state = AllGroupState::new(model);
                ContentValidatorState::AllGroup {
                    model: Arc::clone(model),
                    state,
                    suffix_locked: false,
                }
            }
        }
    }

    /// Create a content validator state from a compiled content model matcher.
    ///
    /// Convenience wrapper used by tests and ad-hoc callers; wraps the matcher
    /// in a [`CompiledContentModel`] then builds the per-element state. The
    /// per-run validation path uses [`Self::from_compiled`] against the
    /// pre-built per-type map instead.
    pub fn from_matcher(matcher: ContentModelMatcher) -> Self {
        Self::from_compiled(&CompiledContentModel::from_matcher(matcher))
    }

    /// Create a content validator state from an NFA table
    ///
    /// Computes the initial epsilon closure from the start state.
    pub fn from_nfa(nfa: NfaTable) -> Self {
        let active_states = ActiveStates::from_nfa(&nfa);
        ContentValidatorState::Nfa {
            nfa: Arc::new(nfa),
            active_states,
            open_content: None,
        }
    }

    /// Create a content validator state from an all-group model
    pub fn from_all_group(model: AllGroupModel) -> Self {
        let state = model.create_state();
        ContentValidatorState::AllGroup {
            model: Arc::new(model),
            state,
            suffix_locked: false,
        }
    }

    /// Advance the content model with a child element
    ///
    /// Returns `None` if the element was rejected.
    /// Returns `Some(ElementMatchInfo)` if accepted, containing the
    /// `ElementKey` and `resolved_type` from the matching NFA term (if any).
    ///
    /// # Panics
    ///
    /// If a counted model exceeds an execution limit
    /// ([`ContentModelLimitExceeded`]). The validation runtime uses
    /// [`try_advance_element`](Self::try_advance_element) instead and turns
    /// the error into an operational failure.
    pub fn advance_element(
        &mut self,
        name: NameId,
        namespace: Option<NameId>,
        target_ns: Option<NameId>,
        xsd_version: XsdVersion,
        subst_groups: Option<&SubstitutionGroupMap>,
    ) -> Option<ElementMatchInfo> {
        self.try_advance_element(name, namespace, target_ns, xsd_version, subst_groups)
            .unwrap_or_else(|e| {
                panic!("ContentValidatorState::advance_element: {e}; use try_advance_element")
            })
    }

    /// Fallible form of [`advance_element`](Self::advance_element).
    ///
    /// `Ok(Some(info))` — accepted; `Ok(None)` — rejected, state unchanged
    /// (existing error-continuation behaviour); `Err(limit)` — a counted
    /// model exceeded an execution limit, state unchanged, no verdict.
    pub fn try_advance_element(
        &mut self,
        name: NameId,
        namespace: Option<NameId>,
        target_ns: Option<NameId>,
        xsd_version: XsdVersion,
        subst_groups: Option<&SubstitutionGroupMap>,
    ) -> Result<Option<ElementMatchInfo>, ContentModelLimitExceeded> {
        let mut limit: Option<ContentModelLimitExceeded> = None;
        let matched = self.advance_element_impl(
            name,
            namespace,
            target_ns,
            xsd_version,
            subst_groups,
            &mut limit,
        );
        match limit {
            Some(e) => Err(e),
            None => Ok(matched),
        }
    }

    /// Shared body of the two entry points above. A limit error is written to
    /// `limit_out` and the function returns `None` without touching `self`.
    fn advance_element_impl(
        &mut self,
        name: NameId,
        namespace: Option<NameId>,
        target_ns: Option<NameId>,
        xsd_version: XsdVersion,
        subst_groups: Option<&SubstitutionGroupMap>,
        limit_out: &mut Option<ContentModelLimitExceeded>,
    ) -> Option<ElementMatchInfo> {
        match self {
            ContentValidatorState::Nfa {
                nfa,
                active_states,
                open_content,
            } => {
                // Fused single pass: capture the match info and compute the next
                // frontier together (the open-content fallback below still reads
                // the *old* `active_states`, so don't move `next` in until then).
                let (mi, next) = match active_states.advance_element_with_info(
                    nfa,
                    name,
                    namespace,
                    target_ns,
                    subst_groups,
                    xsd_version,
                ) {
                    Ok(step) => step,
                    Err(limit) => {
                        *limit_out = Some(limit);
                        return None;
                    }
                };
                let match_info = ElementMatchInfo {
                    element_key: mi.element_key,
                    resolved_type: mi.resolved_type,
                    process_contents: mi.process_contents,
                };

                if next.is_empty() {
                    // No NFA transition matched — try open content wildcard
                    if let Some(oc) = open_content {
                        let allow = match oc.mode {
                            TypesOpenContentMode::Interleave => true,
                            TypesOpenContentMode::Suffix => active_states.contains_accept(nfa),
                            TypesOpenContentMode::None => false,
                        };
                        if allow
                            && open_content_allows(
                                &oc.namespace_constraint,
                                &oc.not_qnames,
                                name,
                                namespace,
                                target_ns,
                            )
                        {
                            // Suffix mode: lock NFA to accept-only so no declared elements
                            // are accepted after the first open-content element (§3.10.4 suffix semantics).
                            if matches!(oc.mode, TypesOpenContentMode::Suffix) {
                                // Lock to the accept state only. The accept state is a
                                // terminal sink, so the singleton is already closure-closed;
                                // for a counted NFA we must NOT call the counter-free
                                // `epsilon_closure` (it asserts no counters and ignores
                                // counter transitions). Use the explicit closure only on
                                // the counter-free path to keep the invariant defensively.
                                let accept_only = if nfa.has_counters() {
                                    StateSet::single(nfa.accept_state)
                                } else {
                                    epsilon_closure(nfa, std::iter::once(nfa.accept_state))
                                };
                                *active_states = ActiveStates::Simple(accept_only);
                            }
                            return Some(ElementMatchInfo {
                                element_key: None,
                                resolved_type: None,
                                process_contents: Some(oc.process_contents),
                            });
                        }
                    }
                    return None;
                }
                *active_states = next;
                Some(match_info)
            }
            ContentValidatorState::AllGroup {
                model,
                state,
                suffix_locked,
            } => {
                // Once the suffix open-content section has begun, declared
                // all-group particles are no longer eligible to match.
                //
                // Structures §3.8.4.1.3: "if one of the Particles in M
                // ·attributes· an element information item to a ·wildcard
                // particle·, and a ·competing· Particle ·attributes· the same
                // item to an ·element particle·, then the ·element particle·
                // is used for validation." Hence two passes: an element
                // particle that still accepts the item wins over a wildcard,
                // whatever their declaration order. One whose {max occurs}
                // is used up has no ·path· through it any more, so it no
                // longer ·competes·, and a wildcard may take the item (the
                // Note there: in `<xs:all><xs:any/><xs:element name="a"/>`,
                // `<a/><a/>` attributes the first `a` to the element and the
                // second to the wildcard).
                if !*suffix_locked {
                    let candidates = model
                        .particles
                        .iter()
                        .enumerate()
                        .filter(|(_, p)| matches!(p.term, NfaTerm::Element { .. }))
                        .chain(
                            model
                                .particles
                                .iter()
                                .enumerate()
                                .filter(|(_, p)| matches!(p.term, NfaTerm::Wildcard { .. })),
                        );
                    for (i, particle) in candidates {
                        if !state.can_accept(model, i) {
                            continue;
                        }
                        let result = term_matches_with_substitution(
                            &particle.term,
                            name,
                            namespace,
                            target_ns,
                            subst_groups,
                            xsd_version,
                        );
                        if result == TermMatchResult::Match {
                            if state.accept(model, i) {
                                let info = match &particle.term {
                                    NfaTerm::Element {
                                        name: term_name,
                                        namespace: term_ns,
                                        element_key,
                                        resolved_type,
                                    } => {
                                        if *term_name == name && *term_ns == namespace {
                                            // Direct match
                                            ElementMatchInfo {
                                                element_key: *element_key,
                                                resolved_type: *resolved_type,
                                                process_contents: None,
                                            }
                                        } else {
                                            // Substitution match — let runtime resolve
                                            ElementMatchInfo {
                                                element_key: None,
                                                resolved_type: None,
                                                process_contents: None,
                                            }
                                        }
                                    }
                                    NfaTerm::Wildcard {
                                        process_contents, ..
                                    } => ElementMatchInfo {
                                        element_key: None,
                                        resolved_type: None,
                                        process_contents: Some(*process_contents),
                                    },
                                };
                                return Some(info);
                            }
                            return None;
                        }
                    }
                }
                // No declared particle matched (or suffix lock engaged) —
                // try open content wildcard.
                if let Some(oc) = &model.open_content {
                    let allow = match oc.mode {
                        AllGroupOpenContentMode::Interleave => true,
                        AllGroupOpenContentMode::Suffix => state.is_satisfied(model),
                        AllGroupOpenContentMode::None => false,
                    };
                    if allow
                        && open_content_allows(
                            &oc.namespace_constraint,
                            &oc.not_qnames,
                            name,
                            namespace,
                            target_ns,
                        )
                    {
                        // §3.10.4 suffix semantics: once a suffix wildcard
                        // element matches, no declared all-group particle may
                        // match again.
                        if matches!(oc.mode, AllGroupOpenContentMode::Suffix) {
                            *suffix_locked = true;
                        }
                        return Some(ElementMatchInfo {
                            element_key: None,
                            resolved_type: None,
                            process_contents: Some(oc.process_contents),
                        });
                    }
                }
                None
            }
            ContentValidatorState::Simple | ContentValidatorState::Empty => {
                // Simple and Empty content models do not accept child elements
                None
            }
        }
    }

    /// Check whether the content model is in a complete (accepting) state
    ///
    /// For NFA: any active state is the accept state.
    /// For AllGroup: all required particles have been satisfied.
    pub fn is_complete(&self) -> bool {
        match self {
            ContentValidatorState::Nfa {
                nfa, active_states, ..
            } => active_states.contains_accept(nfa),
            ContentValidatorState::AllGroup { model, state, .. } => {
                // If the outer particle is optional (minOccurs=0) and no children
                // have been consumed, the entire group was skipped — trivially satisfied.
                if model.outer_optional && !state.has_any_consumed() {
                    return true;
                }
                state.is_satisfied(model)
            }
            ContentValidatorState::Simple | ContentValidatorState::Empty => true,
        }
    }

    /// Fallible form of [`is_complete`](Self::is_complete).
    ///
    /// Completion is a pure inspection of the already-computed state and never
    /// runs the automaton, so this cannot exceed an execution limit; it is kept
    /// for symmetry with the advancing entry points and always returns `Ok`.
    pub fn try_is_complete(&self) -> Result<bool, ContentModelLimitExceeded> {
        Ok(self.is_complete())
    }

    /// Non-mutating lookahead: would the given element be accepted?
    ///
    /// This does not change the state of the content model.
    pub fn would_accept(
        &self,
        name: NameId,
        namespace: Option<NameId>,
        target_ns: Option<NameId>,
        xsd_version: XsdVersion,
        subst_groups: Option<&SubstitutionGroupMap>,
    ) -> bool {
        match self {
            ContentValidatorState::Nfa {
                nfa,
                active_states,
                open_content,
            } => {
                // Lookahead only: an execution-limit error here is reported as
                // "not accepted"; the mutating advance that follows raises the
                // operational failure.
                let Ok(next) = (match xsd_version {
                    XsdVersion::V1_0 => active_states.advance_from(
                        nfa,
                        name,
                        namespace,
                        target_ns,
                        subst_groups,
                        xsd_version,
                    ),
                    XsdVersion::V1_1 => active_states.advance_with_priority_from(
                        nfa,
                        name,
                        namespace,
                        target_ns,
                        subst_groups,
                        xsd_version,
                    ),
                }) else {
                    return false;
                };
                if !next.is_empty() {
                    return true;
                }
                // Try open content wildcard fallback
                if let Some(oc) = open_content {
                    let allow = match oc.mode {
                        TypesOpenContentMode::Interleave => true,
                        TypesOpenContentMode::Suffix => active_states.contains_accept(nfa),
                        TypesOpenContentMode::None => false,
                    };
                    if allow
                        && open_content_allows(
                            &oc.namespace_constraint,
                            &oc.not_qnames,
                            name,
                            namespace,
                            target_ns,
                        )
                    {
                        return true;
                    }
                }
                false
            }
            ContentValidatorState::AllGroup {
                model,
                state,
                suffix_locked,
            } => {
                if !*suffix_locked {
                    for (i, particle) in model.particles.iter().enumerate() {
                        if !state.can_accept(model, i) {
                            continue;
                        }
                        let result = term_matches_with_substitution(
                            &particle.term,
                            name,
                            namespace,
                            target_ns,
                            subst_groups,
                            xsd_version,
                        );
                        if result == TermMatchResult::Match {
                            return true;
                        }
                    }
                }
                // Try open content wildcard fallback
                if let Some(oc) = &model.open_content {
                    let allow = match oc.mode {
                        AllGroupOpenContentMode::Interleave => true,
                        AllGroupOpenContentMode::Suffix => state.is_satisfied(model),
                        AllGroupOpenContentMode::None => false,
                    };
                    if allow
                        && open_content_allows(
                            &oc.namespace_constraint,
                            &oc.not_qnames,
                            name,
                            namespace,
                            target_ns,
                        )
                    {
                        return true;
                    }
                }
                false
            }
            ContentValidatorState::Simple | ContentValidatorState::Empty => false,
        }
    }
}

/// Check if an open content wildcard allows the given element.
/// Combines namespace matching with notQName exclusion checking.
/// Open content is an XSD 1.1 feature, so V1_1 semantics always apply.
fn open_content_allows(
    ns_constraint: &NamespaceConstraint,
    not_qnames: &[(Option<NameId>, NameId)],
    name: NameId,
    namespace: Option<NameId>,
    target_ns: Option<NameId>,
) -> bool {
    ns_constraint.matches(namespace, target_ns) && !not_qnames_exclude(not_qnames, namespace, name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::{NfaState, NfaTerm};

    /// Build a simple NFA that accepts a single element with given name_id
    fn single_element_nfa(name: NameId, namespace: Option<NameId>) -> NfaTable {
        // State 0: start (epsilon) -> State 1
        // State 1: element term, consume -> State 2
        // State 2: accept (epsilon)
        let mut s0 = NfaState::epsilon(0, None);
        s0.add_epsilon(1);

        let mut s1 = NfaState::with_term(1, NfaTerm::element(name, namespace, None), None);
        s1.add_consume(2);

        let s2 = NfaState::epsilon(2, None);

        NfaTable::new(vec![s0, s1, s2], 0, 2)
    }

    /// Build an NFA that accepts a sequence: elem_a then elem_b
    fn sequence_nfa(
        name_a: NameId,
        ns_a: Option<NameId>,
        name_b: NameId,
        ns_b: Option<NameId>,
    ) -> NfaTable {
        // State 0: start (epsilon) -> State 1
        // State 1: element A, consume -> State 2
        // State 2: epsilon -> State 3
        // State 3: element B, consume -> State 4
        // State 4: accept (epsilon)
        let mut s0 = NfaState::epsilon(0, None);
        s0.add_epsilon(1);

        let mut s1 = NfaState::with_term(1, NfaTerm::element(name_a, ns_a, None), None);
        s1.add_consume(2);

        let mut s2 = NfaState::epsilon(2, None);
        s2.add_epsilon(3);

        let mut s3 = NfaState::with_term(3, NfaTerm::element(name_b, ns_b, None), None);
        s3.add_consume(4);

        let s4 = NfaState::epsilon(4, None);

        NfaTable::new(vec![s0, s1, s2, s3, s4], 0, 4)
    }

    #[test]
    fn test_nfa_single_element_accepted() {
        let name = NameId(1);
        let nfa = single_element_nfa(name, None);
        let mut state = ContentValidatorState::from_nfa(nfa);

        assert!(
            !state.is_complete(),
            "should not be complete before any element"
        );
        assert!(state
            .advance_element(name, None, None, XsdVersion::V1_0, None)
            .is_some());
        assert!(
            state.is_complete(),
            "should be complete after matching element"
        );
    }

    #[test]
    fn test_nfa_single_element_rejected() {
        let name = NameId(1);
        let wrong_name = NameId(2);
        let nfa = single_element_nfa(name, None);
        let mut state = ContentValidatorState::from_nfa(nfa);

        assert!(state
            .advance_element(wrong_name, None, None, XsdVersion::V1_0, None)
            .is_none());
        assert!(!state.is_complete());
    }

    #[test]
    fn test_nfa_sequence() {
        let a = NameId(10);
        let b = NameId(20);
        let nfa = sequence_nfa(a, None, b, None);
        let mut state = ContentValidatorState::from_nfa(nfa);

        assert!(!state.is_complete());
        assert!(state
            .advance_element(a, None, None, XsdVersion::V1_0, None)
            .is_some());
        assert!(!state.is_complete(), "only first element seen");
        assert!(state
            .advance_element(b, None, None, XsdVersion::V1_0, None)
            .is_some());
        assert!(state.is_complete(), "both elements matched");
    }

    #[test]
    fn test_nfa_sequence_wrong_order() {
        let a = NameId(10);
        let b = NameId(20);
        let nfa = sequence_nfa(a, None, b, None);
        let mut state = ContentValidatorState::from_nfa(nfa);

        // Try b first - should be rejected
        assert!(state
            .advance_element(b, None, None, XsdVersion::V1_0, None)
            .is_none());
    }

    #[test]
    fn test_nfa_would_accept() {
        let name = NameId(1);
        let wrong = NameId(2);
        let nfa = single_element_nfa(name, None);
        let state = ContentValidatorState::from_nfa(nfa);

        assert!(state.would_accept(name, None, None, XsdVersion::V1_0, None));
        assert!(!state.would_accept(wrong, None, None, XsdVersion::V1_0, None));
    }

    #[test]
    fn test_all_group_any_order() {
        use crate::compiler::{AllParticle, MaxOccurs};

        let a = NameId(10);
        let b = NameId(20);

        let model = AllGroupModel::new(vec![
            AllParticle::new(
                NfaTerm::element(a, None, None),
                1,
                MaxOccurs::Bounded(1),
                None,
            ),
            AllParticle::new(
                NfaTerm::element(b, None, None),
                1,
                MaxOccurs::Bounded(1),
                None,
            ),
        ]);

        // Order: b, a (reversed) should still work
        let mut state = ContentValidatorState::from_all_group(model);
        assert!(!state.is_complete());
        assert!(state
            .advance_element(b, None, None, XsdVersion::V1_0, None)
            .is_some());
        assert!(
            !state.is_complete(),
            "only one of two required particles matched"
        );
        assert!(state
            .advance_element(a, None, None, XsdVersion::V1_0, None)
            .is_some());
        assert!(state.is_complete(), "both particles satisfied");
    }

    #[test]
    fn test_all_group_missing_required() {
        use crate::compiler::{AllParticle, MaxOccurs};

        let a = NameId(10);
        let b = NameId(20);

        let model = AllGroupModel::new(vec![
            AllParticle::new(
                NfaTerm::element(a, None, None),
                1,
                MaxOccurs::Bounded(1),
                None,
            ),
            AllParticle::new(
                NfaTerm::element(b, None, None),
                1,
                MaxOccurs::Bounded(1),
                None,
            ),
        ]);

        let mut state = ContentValidatorState::from_all_group(model);
        assert!(state
            .advance_element(a, None, None, XsdVersion::V1_0, None)
            .is_some());
        // Don't supply b
        assert!(!state.is_complete(), "b is still required");
    }

    #[test]
    fn test_simple_rejects_elements() {
        let mut state = ContentValidatorState::Simple;
        assert!(state
            .advance_element(NameId(1), None, None, XsdVersion::V1_0, None)
            .is_none());
        assert!(state.is_complete());
    }

    #[test]
    fn test_empty_rejects_elements() {
        let mut state = ContentValidatorState::Empty;
        assert!(state
            .advance_element(NameId(1), None, None, XsdVersion::V1_0, None)
            .is_none());
        assert!(state.is_complete());
    }

    #[test]
    fn test_from_matcher_nfa() {
        let name = NameId(1);
        let nfa = single_element_nfa(name, None);
        let matcher = ContentModelMatcher::Nfa(nfa);
        let mut state = ContentValidatorState::from_matcher(matcher);
        assert!(state
            .advance_element(name, None, None, XsdVersion::V1_0, None)
            .is_some());
        assert!(state.is_complete());
    }

    #[test]
    fn test_from_matcher_all_group() {
        use crate::compiler::{AllParticle, MaxOccurs};

        let a = NameId(5);
        let model = AllGroupModel::new(vec![AllParticle::new(
            NfaTerm::element(a, None, None),
            0,
            MaxOccurs::Bounded(1),
            None,
        )]);
        let matcher = ContentModelMatcher::AllGroup(model);
        let state = ContentValidatorState::from_matcher(matcher);
        // Optional particle, so complete even without matching
        assert!(state.is_complete());
    }

    // -- Open content tests --------------------------------------------------

    use crate::compiler::OpenContentMode as AllGroupOCMode;
    use crate::compiler::{AllParticle, MaxOccurs, OpenContentWildcard};

    fn all_group_with_open_content(
        mode: AllGroupOCMode,
        ns_constraint: NamespaceConstraint,
    ) -> AllGroupModel {
        let a = NameId(10);
        let mut model = AllGroupModel::new(vec![AllParticle::new(
            NfaTerm::element(a, None, None),
            1,
            MaxOccurs::Bounded(1),
            None,
        )]);
        model.open_content = Some(OpenContentWildcard {
            namespace_constraint: ns_constraint,
            process_contents: ProcessContents::Lax,
            mode,
            not_qnames: Vec::new(),
        });
        model
    }

    #[test]
    fn test_all_group_open_content_interleave() {
        let model =
            all_group_with_open_content(AllGroupOCMode::Interleave, NamespaceConstraint::Any);
        let mut state = ContentValidatorState::from_all_group(model);

        let extra = NameId(99);
        let a = NameId(10);

        // Extra element accepted via open content before the declared particle
        let info = state.advance_element(extra, None, None, XsdVersion::V1_1, None);
        assert!(info.is_some(), "interleave should accept extra element");
        assert!(info.unwrap().process_contents.is_some());

        // Declared particle still works
        assert!(state
            .advance_element(a, None, None, XsdVersion::V1_1, None)
            .is_some());
        assert!(state.is_complete());

        // Extra element accepted after declared particle too
        let info2 = state.advance_element(extra, None, None, XsdVersion::V1_1, None);
        assert!(
            info2.is_some(),
            "interleave should accept extra element after satisfaction"
        );
    }

    #[test]
    fn test_all_group_open_content_suffix() {
        let model = all_group_with_open_content(AllGroupOCMode::Suffix, NamespaceConstraint::Any);
        let mut state = ContentValidatorState::from_all_group(model);

        let extra = NameId(99);
        let a = NameId(10);

        // Extra element rejected before the required particle is satisfied
        assert!(
            state
                .advance_element(extra, None, None, XsdVersion::V1_1, None)
                .is_none(),
            "suffix should reject extra element before satisfaction"
        );

        // Satisfy the declared particle
        assert!(state
            .advance_element(a, None, None, XsdVersion::V1_1, None)
            .is_some());
        assert!(state.is_complete());

        // Now extra element should be accepted
        assert!(
            state
                .advance_element(extra, None, None, XsdVersion::V1_1, None)
                .is_some(),
            "suffix should accept extra element after satisfaction"
        );
    }

    #[test]
    fn test_nfa_open_content_interleave() {
        let a = NameId(10);
        let nfa = single_element_nfa(a, None);
        let oc = OpenContentInfo {
            mode: TypesOpenContentMode::Interleave,
            namespace_constraint: NamespaceConstraint::Any,
            process_contents: ProcessContents::Lax,
            not_qnames: Vec::new(),
        };
        let initial = ActiveStates::from_nfa(&nfa);
        let mut state = ContentValidatorState::Nfa {
            nfa: nfa.into(),
            active_states: initial,
            open_content: Some(oc),
        };

        let extra = NameId(99);

        // Extra element accepted via open content before the declared element
        let info = state.advance_element(extra, None, None, XsdVersion::V1_1, None);
        assert!(
            info.is_some(),
            "interleave should accept extra element before NFA match"
        );

        // Declared element still works
        assert!(state
            .advance_element(a, None, None, XsdVersion::V1_1, None)
            .is_some());
        assert!(state.is_complete());
    }

    #[test]
    fn test_nfa_open_content_suffix() {
        let a = NameId(10);
        let nfa = single_element_nfa(a, None);
        let oc = OpenContentInfo {
            mode: TypesOpenContentMode::Suffix,
            namespace_constraint: NamespaceConstraint::Any,
            process_contents: ProcessContents::Lax,
            not_qnames: Vec::new(),
        };
        let initial = ActiveStates::from_nfa(&nfa);
        let mut state = ContentValidatorState::Nfa {
            nfa: nfa.into(),
            active_states: initial,
            open_content: Some(oc),
        };

        let extra = NameId(99);
        let a = NameId(10);

        // Extra element rejected before accept state
        assert!(
            state
                .advance_element(extra, None, None, XsdVersion::V1_1, None)
                .is_none(),
            "suffix should reject extra element before accept state"
        );

        // Match declared element to reach accept state
        assert!(state
            .advance_element(a, None, None, XsdVersion::V1_1, None)
            .is_some());
        assert!(state.is_complete());

        // Now extra element accepted
        assert!(
            state
                .advance_element(extra, None, None, XsdVersion::V1_1, None)
                .is_some(),
            "suffix should accept extra element after accept state"
        );
    }

    #[test]
    fn test_open_content_namespace_constraint() {
        let target_ns = Some(NameId(100));
        let other_ns = Some(NameId(200));

        let model = all_group_with_open_content(
            AllGroupOCMode::Interleave,
            NamespaceConstraint::Other, // Only accept elements from other namespaces
        );
        let mut state = ContentValidatorState::from_all_group(model);

        let extra = NameId(99);

        // Element from target namespace should be rejected by open content
        assert!(
            state
                .advance_element(extra, target_ns, target_ns, XsdVersion::V1_1, None)
                .is_none(),
            "open content with ##other should reject target namespace"
        );

        // Element from other namespace should be accepted
        assert!(
            state
                .advance_element(extra, other_ns, target_ns, XsdVersion::V1_1, None)
                .is_some(),
            "open content with ##other should accept other namespace"
        );
    }

    #[test]
    fn test_would_accept_with_open_content() {
        let model =
            all_group_with_open_content(AllGroupOCMode::Interleave, NamespaceConstraint::Any);
        let state = ContentValidatorState::from_all_group(model);

        let extra = NameId(99);
        let a = NameId(10);

        // Both declared and extra elements should be accepted in lookahead
        assert!(state.would_accept(a, None, None, XsdVersion::V1_1, None));
        assert!(state.would_accept(extra, None, None, XsdVersion::V1_1, None));

        // NFA version
        let nfa = single_element_nfa(a, None);
        let oc = OpenContentInfo {
            mode: TypesOpenContentMode::Interleave,
            namespace_constraint: NamespaceConstraint::Any,
            process_contents: ProcessContents::Lax,
            not_qnames: Vec::new(),
        };
        let initial = ActiveStates::from_nfa(&nfa);
        let state = ContentValidatorState::Nfa {
            nfa: nfa.into(),
            active_states: initial,
            open_content: Some(oc),
        };
        assert!(state.would_accept(a, None, None, XsdVersion::V1_1, None));
        assert!(state.would_accept(extra, None, None, XsdVersion::V1_1, None));
    }

    // -- Not constraint and notQName tests -----------------------------------

    #[test]
    fn test_open_content_not_namespace_constraint() {
        // Open content with Not([ns1]) should reject ns1 but accept others
        let ns1 = Some(NameId(100));
        let ns2 = Some(NameId(200));
        let a = NameId(10);
        let extra = NameId(99);

        let mut model = AllGroupModel::new(vec![AllParticle::new(
            NfaTerm::element(a, None, None),
            1,
            MaxOccurs::Bounded(1),
            None,
        )]);
        model.open_content = Some(OpenContentWildcard {
            namespace_constraint: NamespaceConstraint::Not(vec![ns1]),
            process_contents: ProcessContents::Lax,
            mode: AllGroupOCMode::Interleave,
            not_qnames: Vec::new(),
        });
        let mut state = ContentValidatorState::from_all_group(model);

        // Element from excluded namespace rejected
        assert!(
            state
                .advance_element(extra, ns1, None, XsdVersion::V1_1, None)
                .is_none(),
            "Not([ns1]) should reject elements from ns1"
        );

        // Element from other namespace accepted
        assert!(
            state
                .advance_element(extra, ns2, None, XsdVersion::V1_1, None)
                .is_some(),
            "Not([ns1]) should accept elements from ns2"
        );
    }

    #[test]
    fn test_open_content_not_qnames_exclusion() {
        // Open content with notQName excluding specific element
        let a = NameId(10);
        let excluded = NameId(50);
        let allowed = NameId(60);

        let mut model = AllGroupModel::new(vec![AllParticle::new(
            NfaTerm::element(a, None, None),
            1,
            MaxOccurs::Bounded(1),
            None,
        )]);
        model.open_content = Some(OpenContentWildcard {
            namespace_constraint: NamespaceConstraint::Any,
            process_contents: ProcessContents::Lax,
            mode: AllGroupOCMode::Interleave,
            not_qnames: vec![(None, excluded)], // exclude (absent ns, excluded)
        });
        let mut state = ContentValidatorState::from_all_group(model);

        // Excluded element rejected even though namespace matches
        assert!(
            state
                .advance_element(excluded, None, None, XsdVersion::V1_1, None)
                .is_none(),
            "notQName should reject excluded element"
        );

        // Non-excluded element accepted
        assert!(
            state
                .advance_element(allowed, None, None, XsdVersion::V1_1, None)
                .is_some(),
            "notQName should accept non-excluded element"
        );
    }

    #[test]
    fn test_nfa_open_content_not_qnames_exclusion() {
        // Same test but for NFA path
        let a = NameId(10);
        let excluded = NameId(50);
        let allowed = NameId(60);
        let nfa = single_element_nfa(a, None);
        let oc = OpenContentInfo {
            mode: TypesOpenContentMode::Interleave,
            namespace_constraint: NamespaceConstraint::Any,
            process_contents: ProcessContents::Lax,
            not_qnames: vec![(None, excluded)],
        };
        let initial = ActiveStates::from_nfa(&nfa);
        let mut state = ContentValidatorState::Nfa {
            nfa: nfa.into(),
            active_states: initial,
            open_content: Some(oc),
        };

        // Excluded element rejected
        assert!(
            state
                .advance_element(excluded, None, None, XsdVersion::V1_1, None)
                .is_none(),
            "NFA open content notQName should reject excluded element"
        );

        // Non-excluded element accepted
        assert!(
            state
                .advance_element(allowed, None, None, XsdVersion::V1_1, None)
                .is_some(),
            "NFA open content notQName should accept non-excluded element"
        );
    }

    #[test]
    fn test_would_accept_respects_not_qnames() {
        let a = NameId(10);
        let excluded = NameId(50);
        let allowed = NameId(60);

        let mut model = AllGroupModel::new(vec![AllParticle::new(
            NfaTerm::element(a, None, None),
            1,
            MaxOccurs::Bounded(1),
            None,
        )]);
        model.open_content = Some(OpenContentWildcard {
            namespace_constraint: NamespaceConstraint::Any,
            process_contents: ProcessContents::Lax,
            mode: AllGroupOCMode::Interleave,
            not_qnames: vec![(None, excluded)],
        });
        let state = ContentValidatorState::from_all_group(model);

        assert!(!state.would_accept(excluded, None, None, XsdVersion::V1_1, None));
        assert!(state.would_accept(allowed, None, None, XsdVersion::V1_1, None));
    }
}
