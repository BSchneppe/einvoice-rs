//! NFA fragment structures and composition helpers
//!
//! This module implements Thompson's construction algorithm for building NFAs
//! from content model particles. Fragments are composable building blocks that
//! can be concatenated, alternated, or repeated.

use crate::parser::location::SourceRef;

use super::nfa::{CounterDef, CounterId, NfaState, NfaTable, NfaTerm, StateId, TransitionKind};

/// A composable NFA fragment with single entry and exit points
///
/// Fragments are the building blocks for constructing complex NFAs using
/// Thompson's construction. They maintain the invariant of having exactly
/// one start state and one end state, which enables easy composition.
#[derive(Debug, Clone)]
pub struct NfaFragment {
    /// All states in this fragment (indices are local to fragment)
    pub states: Vec<NfaState>,
    /// Entry point state index (into states vector)
    pub start: usize,
    /// Exit point state index (into states vector)
    pub end: usize,
    /// Counter definitions for counted loops within this fragment
    pub counter_defs: Vec<CounterDef>,
    /// Whether this fragment can match the empty string (end reachable from
    /// start without consuming any input).  Tracked incrementally through
    /// all composition operations and used to set `CounterDef::body_nullable`.
    pub nullable: bool,
}

impl NfaFragment {
    /// Create a new fragment from states with specified start/end (no counters, not nullable)
    pub fn new(states: Vec<NfaState>, start: usize, end: usize) -> Self {
        debug_assert!(start < states.len(), "start index out of bounds");
        debug_assert!(end < states.len(), "end index out of bounds");
        Self {
            states,
            start,
            end,
            counter_defs: Vec::new(),
            nullable: false,
        }
    }

    /// Create a new fragment with counter definitions
    pub fn with_counters(
        states: Vec<NfaState>,
        start: usize,
        end: usize,
        counter_defs: Vec<CounterDef>,
        nullable: bool,
    ) -> Self {
        debug_assert!(start < states.len(), "start index out of bounds");
        debug_assert!(end < states.len(), "end index out of bounds");
        Self {
            states,
            start,
            end,
            counter_defs,
            nullable,
        }
    }

    /// Verifies the position-based ID invariant: `state.id == index` for all states.
    ///
    /// This invariant is critical for correctness of all composition operations
    /// (`concat`, `alternate`, etc.) which use position-based offset arithmetic.
    /// `FragmentBuilder` guarantees it at construction; combinators preserve it.
    /// Any new fragment creation path MUST maintain this invariant.
    fn assert_ids_normalized(&self) {
        for (pos, state) in self.states.iter().enumerate() {
            debug_assert_eq!(
                state.id, pos as StateId,
                "Fragment ID invariant violated: state at position {} has id {}",
                pos, state.id
            );
        }
    }

    /// Get the start state
    pub fn start_state(&self) -> &NfaState {
        &self.states[self.start]
    }

    /// Get the end state
    pub fn end_state(&self) -> &NfaState {
        &self.states[self.end]
    }

    /// Get a mutable reference to a state by local index
    pub fn get_state_mut(&mut self, index: usize) -> Option<&mut NfaState> {
        self.states.get_mut(index)
    }

    /// Give every origin-less epsilon state in this fragment the location of
    /// the construct that built it.
    ///
    /// Composition invents states that belong to no particle of their own —
    /// the branch and merge states of [`alternate`](Self::alternate) and
    /// [`concat`](Self::concat), the entry and exit of the occurrence
    /// wrappers, the exit of a [`single_term`](FragmentBuilder::single_term)
    /// fragment — so [`FragmentBuilder`] creates them with no origin and the
    /// inspector renders them as `(no origin)`. Calling this right after a
    /// composition attributes them to the model group or particle that caused
    /// it; states already carrying an origin keep it, so the innermost known
    /// construct always wins.
    ///
    /// Term-bearing states are never touched, whether or not they have an
    /// origin. Their origin identifies the particle a term came from, and
    /// `compiler::upa`'s `same_particle_origin` compares exactly those to
    /// decide whether two reachable terms are two views of one particle;
    /// handing two distinct terms one shared location would make a real UPA
    /// conflict look like a particle meeting itself.
    pub fn fill_missing_origin(&mut self, origin: &SourceRef) {
        for state in &mut self.states {
            if state.term.is_none() && state.origin.is_none() {
                state.origin = Some(origin.clone());
            }
        }
    }

    /// Concatenate two fragments: self followed by other
    ///
    /// Creates an epsilon transition from self's end state to other's start state.
    /// The resulting fragment starts at self's start and ends at other's end.
    pub fn concat(mut self, mut other: NfaFragment) -> NfaFragment {
        let nullable = self.nullable && other.nullable;

        self.assert_ids_normalized();
        other.assert_ids_normalized();
        self.debug_assert_isolated_boundaries();
        other.debug_assert_isolated_boundaries();

        let state_offset = self.states.len();
        let counter_offset = self.counter_defs.len() as CounterId;

        // Offset all state IDs and counter IDs in other fragment
        for state in &mut other.states {
            state.id += state_offset as StateId;
            for trans in &mut state.transitions {
                trans.target += state_offset as StateId;
                trans.kind = trans.kind.offset_counter(counter_offset);
            }
        }

        // Add epsilon transition from self.end to other.start
        let other_start = other.start + state_offset;
        self.states[self.end].add_epsilon(other_start as StateId);

        // Merge states and counter defs
        let new_end = other.end + state_offset;
        self.states.extend(other.states);
        self.counter_defs.extend(other.counter_defs);

        NfaFragment::with_counters(
            self.states,
            self.start,
            new_end,
            self.counter_defs,
            nullable,
        )
    }

    /// Alternate two fragments: self | other
    ///
    /// Creates a new start state with epsilon transitions to both fragments,
    /// and a new end state that both fragments converge to.
    pub fn alternate(mut self, mut other: NfaFragment) -> NfaFragment {
        let nullable = self.nullable || other.nullable;

        self.assert_ids_normalized();
        other.assert_ids_normalized();
        self.debug_assert_isolated_boundaries();
        other.debug_assert_isolated_boundaries();

        // Create new start and end states
        let new_start_id = (self.states.len() + other.states.len()) as StateId;
        let new_end_id = new_start_id + 1;

        let mut new_start = NfaState::epsilon(new_start_id, None);
        let new_end = NfaState::epsilon(new_end_id, None);

        // Offset other fragment's state IDs and counter IDs
        let other_state_offset = self.states.len();
        let counter_offset = self.counter_defs.len() as CounterId;
        for state in &mut other.states {
            state.id += other_state_offset as StateId;
            for trans in &mut state.transitions {
                trans.target += other_state_offset as StateId;
                trans.kind = trans.kind.offset_counter(counter_offset);
            }
        }

        // Add epsilon from new start to both fragment starts
        new_start.add_epsilon(self.start as StateId);
        new_start.add_epsilon((other.start + other_state_offset) as StateId);

        // Add epsilon from both fragment ends to new end
        self.states[self.end].add_epsilon(new_end_id);
        other.states[other.end].add_epsilon(new_end_id);

        // Merge all states and counter defs
        let mut states = self.states;
        states.extend(other.states);
        states.push(new_start);
        states.push(new_end);

        let mut counter_defs = self.counter_defs;
        counter_defs.extend(other.counter_defs);

        NfaFragment::with_counters(
            states,
            new_start_id as usize,
            new_end_id as usize,
            counter_defs,
            nullable,
        )
    }

    /// Make fragment optional: self?
    ///
    /// Adds an epsilon transition from start to end, allowing the fragment
    /// to be skipped entirely.
    pub fn optional(mut self) -> NfaFragment {
        self.assert_ids_normalized();
        self.debug_assert_isolated_boundaries();
        // Add epsilon from start to end
        let end_id = self.end as StateId;
        self.states[self.start].add_epsilon(end_id);
        self.nullable = true;
        self
    }

    /// Kleene star: self*
    ///
    /// Allows zero or more repetitions of the fragment. The body is wrapped
    /// in a fresh entry and a fresh exit state — entry → body, body → body
    /// (repeat), body → exit — plus an entry → exit bypass, so the loop edge
    /// never touches a state that composition attaches edges to.
    pub fn repeat_star(self) -> NfaFragment {
        let mut frag = self.wrap_loop();
        let exit_id = frag.end as StateId;
        frag.states[frag.start].add_epsilon(exit_id);
        frag.nullable = true;
        frag
    }

    /// Plus repetition: self+
    ///
    /// Requires at least one occurrence, then allows more: the same fresh
    /// entry, exit and loop as [`repeat_star`](Self::repeat_star), without the
    /// bypass. Nullable iff one occurrence can match empty, so
    /// `self.nullable` carries over.
    pub fn repeat_plus(self) -> NfaFragment {
        self.wrap_loop()
    }

    /// Thompson loop: wrap the body in a fresh entry and a fresh exit.
    ///
    /// ```text
    /// entry --ε--> body.start
    /// body.end --ε--> body.start   [repeat]
    /// body.end --ε--> exit         [leave]
    /// ```
    ///
    /// The loop edge must stay strictly inside the fragment. Composition
    /// attaches edges to a fragment's boundary states — `optional` adds
    /// start → end, `concat` links end → next start, an enclosing loop adds
    /// its own edges — so a loop edge drawn between the body's own boundary
    /// states lets those edges enter or leave the loop: `(X, Y*)?` then
    /// accepted `Y` without `X`, because the bypass landed on `Y*`'s end and
    /// the loop edge led from there back into `Y`. See
    /// [`debug_assert_isolated_boundaries`](Self::debug_assert_isolated_boundaries).
    fn wrap_loop(mut self) -> NfaFragment {
        self.assert_ids_normalized();
        let body_start = self.start as StateId;
        let entry_id = self.states.len() as StateId;
        let exit_id = entry_id + 1;

        let mut entry = NfaState::epsilon(entry_id, None);
        entry.add_epsilon(body_start);
        self.states[self.end].add_epsilon(body_start);
        self.states[self.end].add_epsilon(exit_id);
        self.states.push(entry);
        self.states.push(NfaState::epsilon(exit_id, None));

        let frag = NfaFragment::with_counters(
            self.states,
            entry_id as usize,
            exit_id as usize,
            self.counter_defs,
            self.nullable,
        );
        frag.debug_assert_isolated_boundaries();
        frag
    }

    /// Debug check of the invariant every combinator relies on: no
    /// transition inside the fragment enters its start state, and its end
    /// state has no outgoing transition (an epsilon self-loop, which
    /// `optional` creates on a one-state epsilon fragment, is harmless and
    /// allowed).
    ///
    /// `optional`, `concat` and `alternate` draw edges from or to a
    /// fragment's boundary states. They only mean what they say if nothing
    /// inside the fragment can reach the start again or continue past the
    /// end — otherwise a bypass enters a loop body or a link leaves one.
    fn debug_assert_isolated_boundaries(&self) {
        if !cfg!(debug_assertions) {
            return;
        }
        let start = self.start as StateId;
        let end = self.end as StateId;
        for state in &self.states {
            for t in &state.transitions {
                debug_assert!(
                    t.target != start || state.id == start,
                    "fragment start state {start} is entered from state {} inside the fragment",
                    state.id
                );
            }
        }
        debug_assert!(
            self.states[self.end]
                .transitions
                .iter()
                .all(|t| t.target == end),
            "fragment end state {end} has outgoing transitions inside the fragment"
        );
    }

    /// Repeat exactly n times: self{n}
    ///
    /// Creates n concatenated copies of the fragment.
    /// For n=0, returns an epsilon fragment.
    pub fn repeat_exact(self, n: u32) -> NfaFragment {
        if n == 0 {
            return FragmentBuilder::new().epsilon_fragment();
        }

        // nullable: all n copies must be nullable → self.nullable
        // (concat propagates: a.nullable && b.nullable)
        let mut result = self.clone();
        for _ in 1..n {
            result = result.concat(self.clone());
        }
        result
    }

    /// Counted repeat: uses counter transitions for self{min,max}.
    ///
    /// Produces a compact loop structure with 3 extra states (entry, guard, exit)
    /// regardless of min/max values. Counter tracks completed iterations.
    ///
    /// Structure:
    /// ```text
    /// entry --CounterReset(c)--> body_start
    /// body_end --CounterIncrement(c)--> guard
    /// guard --CounterMaxGuard(c)--> body_start   [loop if count < max]
    /// guard --CounterMinGuard(c)--> exit          [exit if count >= min]
    /// [if min == 0: entry --Epsilon--> exit]      [bypass]
    /// ```
    pub fn repeat_counted(mut self, min: u32, max: u32) -> NfaFragment {
        debug_assert!(min <= max, "repeat_counted: min ({min}) > max ({max})");

        // Capture body nullability *before* adding counter infrastructure.
        let body_nullable = self.nullable;

        self.assert_ids_normalized();

        // Allocate counter
        let counter_id = self.counter_defs.len() as CounterId;
        self.counter_defs.push(CounterDef {
            min,
            max,
            body_nullable,
        });

        // Allocate new states: entry, guard, exit
        let entry_idx = self.states.len();
        let guard_idx = entry_idx + 1;
        let exit_idx = entry_idx + 2;

        let entry_id = entry_idx as StateId;
        let guard_id = guard_idx as StateId;
        let exit_id = exit_idx as StateId;
        let body_start_id = self.start as StateId;

        // entry → CounterReset → body_start
        let mut entry = NfaState::epsilon(entry_id, None);
        entry.add_transition(body_start_id, TransitionKind::CounterReset(counter_id));

        // Optional bypass: entry → exit (if min == 0)
        if min == 0 {
            entry.add_epsilon(exit_id);
        }

        // body_end → CounterIncrement → guard
        self.states[self.end]
            .add_transition(guard_id, TransitionKind::CounterIncrement(counter_id));

        // guard → CounterMaxGuard → body_start (loop back)
        // guard → CounterMinGuard → exit (exit loop)
        let mut guard = NfaState::epsilon(guard_id, None);
        guard.add_transition(body_start_id, TransitionKind::CounterMaxGuard(counter_id));
        guard.add_transition(exit_id, TransitionKind::CounterMinGuard(counter_id));

        let exit = NfaState::epsilon(exit_id, None);

        self.states.push(entry);
        self.states.push(guard);
        self.states.push(exit);

        // The counted loop is nullable if min==0 (bypass edge) or body is nullable
        // (all min iterations can complete without consuming input).
        let nullable = min == 0 || body_nullable;

        NfaFragment::with_counters(
            self.states,
            entry_idx,
            exit_idx,
            self.counter_defs,
            nullable,
        )
    }

    /// Repeat between min and max times: self{min,max}
    ///
    /// Creates min mandatory copies followed by (max-min) optional copies.
    /// If max is None, creates min copies followed by a star.
    pub fn repeat_range(self, min: u32, max: Option<u32>) -> NfaFragment {
        match (min, max) {
            (0, Some(0)) => FragmentBuilder::new().epsilon_fragment(),
            (0, Some(1)) => self.optional(),
            (0, None) => self.repeat_star(),
            (1, Some(1)) => self,
            (1, None) => self.repeat_plus(),
            (n, Some(m)) if n == m => self.repeat_exact(n),
            (n, Some(m)) => {
                // n mandatory + (m-n) optional
                let mut result = self.clone().repeat_exact(n);
                for _ in n..m {
                    result = result.concat(self.clone().optional());
                }
                result
            }
            (n, None) => {
                // n mandatory + star
                let mandatory = self.clone().repeat_exact(n);
                mandatory.concat(self.repeat_star())
            }
        }
    }
}

/// Builder for constructing NFA fragments with fragment-local state IDs.
///
/// Fragments are created with position-based IDs (`state.id == index`),
/// which is the invariant required by all composition operations.
/// The builder is stateless — each fragment starts with IDs from 0.
#[derive(Debug)]
pub struct FragmentBuilder;

impl FragmentBuilder {
    /// Create a new fragment builder
    pub fn new() -> Self {
        Self
    }

    /// Build a single-term fragment
    ///
    /// Creates a fragment with one term state (id=0) and one epsilon exit
    /// state (id=1). The term state has a consuming transition to the exit.
    pub fn single_term(&self, term: NfaTerm, origin: Option<SourceRef>) -> NfaFragment {
        let mut term_state = NfaState::with_term(0, term, origin);
        let exit_state = NfaState::epsilon(1, None);

        // Add consuming transition from term state to exit
        term_state.add_consume(1);

        NfaFragment::new(vec![term_state, exit_state], 0, 1)
    }

    /// Build an epsilon-only fragment
    ///
    /// Creates a minimal fragment that matches nothing (empty string).
    /// Used for optional content and as base case for empty sequences.
    pub fn epsilon_fragment(&self) -> NfaFragment {
        let state = NfaState::epsilon(0, None);
        let mut frag = NfaFragment::new(vec![state], 0, 0);
        frag.nullable = true;
        frag
    }

    /// Build a fragment that matches nothing — not even the empty sequence.
    ///
    /// Used for an empty `<xs:choice/>`: a choice with no particles is
    /// unsatisfiable (§3.4.2.3 — only a sequence/all with no children, or a
    /// choice with no children *and* minOccurs=0, yields empty content).
    /// When the particle carries minOccurs=0, the occurrence wrapper adds
    /// the epsilon bypass that makes it skippable; otherwise the content
    /// model rejects all input, including empty content (saxon complex022).
    pub fn dead_fragment(&self) -> NfaFragment {
        let start = NfaState::epsilon(0, None);
        let end = NfaState::epsilon(1, None);
        // No transition from start to end — the accept state is unreachable.
        NfaFragment::new(vec![start, end], 0, 1)
    }
}

impl Default for FragmentBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert a fragment to a complete NFA table.
///
/// Asserts the fragment's ID invariant (`state.id == position`) and
/// wraps the states into an `NfaTable` with the fragment's start/end
/// as start/accept states.
pub fn fragment_to_table(fragment: NfaFragment) -> NfaTable {
    fragment.assert_ids_normalized();
    fragment.debug_assert_isolated_boundaries();

    let start_state = fragment.start as StateId;
    let accept_state = fragment.end as StateId;

    NfaTable::with_counters(
        fragment.states,
        start_state,
        accept_state,
        fragment.counter_defs,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::NameId;

    fn make_element_term(name: u32) -> NfaTerm {
        NfaTerm::element(NameId(name), None, None)
    }

    #[test]
    fn test_single_term_fragment() {
        let builder = FragmentBuilder::new();
        let frag = builder.single_term(make_element_term(1), None);

        assert_eq!(frag.states.len(), 2);
        assert_eq!(frag.start, 0);
        assert_eq!(frag.end, 1);
        assert!(frag.states[0].term.is_some());
        assert!(frag.states[1].term.is_none()); // epsilon exit
    }

    #[test]
    fn test_epsilon_fragment() {
        let builder = FragmentBuilder::new();
        let frag = builder.epsilon_fragment();

        assert_eq!(frag.states.len(), 1);
        assert_eq!(frag.start, 0);
        assert_eq!(frag.end, 0); // Same state
        assert!(frag.states[0].term.is_none());
    }

    #[test]
    fn test_concat() {
        let builder = FragmentBuilder::new();
        let a = builder.single_term(make_element_term(1), None);
        let b = builder.single_term(make_element_term(2), None);

        let concat = a.concat(b);

        // a(2 states) + b(2 states) = 4 states
        assert_eq!(concat.states.len(), 4);
        assert_eq!(concat.start, 0); // a's start
        assert_eq!(concat.end, 3); // b's end (offset by 2)

        // Check epsilon from a's end to b's start
        let a_end = &concat.states[1];
        assert!(a_end.epsilon_transitions().any(|t| t == 2));
    }

    #[test]
    fn test_alternate() {
        let builder = FragmentBuilder::new();
        let a = builder.single_term(make_element_term(1), None);
        let b = builder.single_term(make_element_term(2), None);

        let alt = a.alternate(b);

        // a(2) + b(2) + new_start(1) + new_end(1) = 6 states
        assert_eq!(alt.states.len(), 6);

        // New start should have epsilon to both a.start and b.start
        let new_start = &alt.states[alt.start];
        let eps: Vec<_> = new_start.epsilon_transitions().collect();
        assert_eq!(eps.len(), 2);
        assert!(eps.contains(&0)); // a's start
        assert!(eps.contains(&2)); // b's start (offset by 2)
    }

    #[test]
    fn test_optional() {
        let builder = FragmentBuilder::new();
        let frag = builder.single_term(make_element_term(1), None);
        let opt = frag.optional();

        // Check epsilon from start to end (bypass)
        let start = &opt.states[opt.start];
        assert!(start.epsilon_transitions().any(|t| t == opt.end as StateId));
    }

    #[test]
    fn test_repeat_star() {
        let builder = FragmentBuilder::new();
        let frag = builder.single_term(make_element_term(1), None);
        let star = frag.repeat_star();

        // Body (term state 0, exit 1) wrapped in a fresh entry and exit.
        assert_eq!(star.states.len(), 4);
        let (body_start, body_end) = (0, 1);
        let exit = star.end as StateId;

        let entry = &star.states[star.start];
        assert!(entry.epsilon_transitions().any(|t| t == body_start));
        assert!(entry.epsilon_transitions().any(|t| t == exit)); // bypass

        let end_of_body = &star.states[body_end];
        assert!(end_of_body.epsilon_transitions().any(|t| t == body_start)); // repeat
        assert!(end_of_body.epsilon_transitions().any(|t| t == exit)); // leave

        // The loop stays inside: nothing leaves the exit.
        assert!(star.states[star.end].transitions.is_empty());
        assert!(star.nullable);
    }

    #[test]
    fn test_repeat_plus() {
        let builder = FragmentBuilder::new();
        let frag = builder.single_term(make_element_term(1), None);
        let plus = frag.repeat_plus();

        assert_eq!(plus.states.len(), 4);
        let (body_start, body_end) = (0, 1);
        let exit = plus.end as StateId;

        let entry = &plus.states[plus.start];
        assert!(entry.epsilon_transitions().any(|t| t == body_start));
        // Should NOT have optional bypass
        assert!(!entry.epsilon_transitions().any(|t| t == exit));

        let end_of_body = &plus.states[body_end];
        assert!(end_of_body.epsilon_transitions().any(|t| t == body_start)); // repeat
        assert!(end_of_body.epsilon_transitions().any(|t| t == exit)); // leave

        assert!(plus.states[plus.end].transitions.is_empty());
        assert!(!plus.nullable);
    }

    /// Run `word` (element names) through the table; true when it is accepted.
    fn accepts(table: &NfaTable, word: &[u32]) -> bool {
        use super::super::nfa::ActiveStates;
        use crate::schema::model::XsdVersion;
        let mut states = ActiveStates::from_nfa(table);
        for &name in word {
            states = states.advance(table, NameId(name), None, None, None, XsdVersion::V1_0);
            if states.is_empty() {
                return false;
            }
        }
        states.contains_accept(table)
    }

    /// A loop's boundary edges must not be reachable from the boundary states
    /// other combinators attach to. With the loop edge drawn between the
    /// body's own start and end, `optional()`'s start → end bypass landed on
    /// the star's end and the loop edge led back into the body, so the group
    /// below accepted `Y` without the required `X` before it.
    #[test]
    fn test_loops_inside_optional_and_repeated_groups() {
        const X: u32 = 1;
        const Y: u32 = 2;
        let b = FragmentBuilder::new();
        let x = || b.single_term(make_element_term(X), None);
        let y = || b.single_term(make_element_term(Y), None);

        struct Case {
            label: &'static str,
            frag: NfaFragment,
            valid: Vec<Vec<u32>>,
            invalid: Vec<Vec<u32>>,
        }
        let case = |label, frag, valid, invalid| Case {
            label,
            frag,
            valid,
            invalid,
        };
        let cases = vec![
            case(
                "(X, Y*)?",
                x().concat(y().repeat_star()).optional(),
                vec![vec![], vec![X], vec![X, Y], vec![X, Y, Y]],
                vec![vec![Y], vec![Y, Y], vec![Y, X]],
            ),
            case(
                "(X, Y+)?",
                x().concat(y().repeat_plus()).optional(),
                vec![vec![], vec![X, Y], vec![X, Y, Y]],
                vec![vec![Y], vec![X], vec![Y, Y]],
            ),
            case(
                "(Y*, X)?",
                y().repeat_star().concat(x()).optional(),
                vec![vec![], vec![X], vec![Y, X], vec![Y, Y, X]],
                vec![vec![Y], vec![Y, Y], vec![X, Y]],
            ),
            case(
                "(X, Y*){0,3}",
                x().concat(y().repeat_star()).repeat_range(0, Some(3)),
                vec![vec![], vec![X, Y], vec![X, X, Y, X]],
                vec![vec![Y], vec![Y, X], vec![X, X, X, X]],
            ),
            case(
                "(X, Y*)*",
                x().concat(y().repeat_star()).repeat_star(),
                vec![vec![], vec![X], vec![X, Y, X, Y, Y]],
                vec![vec![Y], vec![Y, X]],
            ),
            case(
                "(Y+)?",
                y().repeat_plus().optional(),
                vec![vec![], vec![Y], vec![Y, Y]],
                vec![vec![X]],
            ),
        ];
        for Case {
            label,
            frag,
            valid,
            invalid,
        } in cases
        {
            let table = fragment_to_table(frag);
            for w in &valid {
                assert!(accepts(&table, w), "{label} must accept {w:?}");
            }
            for w in &invalid {
                assert!(!accepts(&table, w), "{label} must reject {w:?}");
            }
        }
    }

    #[test]
    fn test_repeat_exact() {
        let builder = FragmentBuilder::new();
        let frag = builder.single_term(make_element_term(1), None);
        let exact = frag.repeat_exact(3);

        // 3 copies of 2-state fragment connected = 6 states
        assert_eq!(exact.states.len(), 6);
    }

    #[test]
    fn test_repeat_range() {
        let builder = FragmentBuilder::new();

        // {0,1} = optional
        let frag1 = builder.single_term(make_element_term(1), None);
        let opt = frag1.repeat_range(0, Some(1));
        let start = &opt.states[opt.start];
        assert!(start.epsilon_transitions().any(|t| t == opt.end as StateId));

        // {2,4} = 2 mandatory + 2 optional
        let frag2 = builder.single_term(make_element_term(2), None);
        let range = frag2.repeat_range(2, Some(4));
        // 2*2 mandatory + 2*2 optional = 8 states
        assert_eq!(range.states.len(), 8);
    }

    #[test]
    fn test_fragment_to_table() {
        let builder = FragmentBuilder::new();
        let frag = builder.single_term(make_element_term(1), None);
        let table = fragment_to_table(frag);

        assert_eq!(table.start_state, 0);
        assert_eq!(table.accept_state, 1);
        assert_eq!(table.state_count(), 2);
    }
}
