//! NFA data structures for content model validation
//!
//! This module defines the core NFA (Nondeterministic Finite Automaton) structures
//! used to represent compiled XSD content models.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::OnceLock;

use super::substitution::SubstitutionGroupMap;
use crate::ids::{ElementKey, NameId, TypeKey};
use crate::parser::location::SourceRef;
use crate::schema::model::XsdVersion;
use crate::types::complex::{not_qnames_exclude, NamespaceConstraint, ProcessContents};

/// Unique identifier for NFA states within a table
pub type StateId = u32;

/// Unique identifier for a counter within an NFA.
///
/// u16 is intentional: keeps `TransitionKind` at 4 bytes (vs 8 with u32),
/// halving transition growth. 65K counters is far beyond any real schema.
pub type CounterId = u16;

/// An inclusive range of counter values `[lo, hi]`.
///
/// Used by the `RangedSingle` optimized path to represent a set of
/// possible counter values at a single NFA state, avoiding the O(N)
/// enumeration that scalar counters require for nullable loop bodies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CounterRange {
    pub lo: u32,
    pub hi: u32,
}

impl CounterRange {
    /// Create a range containing a single value.
    pub fn single(v: u32) -> Self {
        Self { lo: v, hi: v }
    }

    /// Create a range `[lo, hi]`. Panics in debug if `lo > hi`.
    pub fn new(lo: u32, hi: u32) -> Self {
        debug_assert!(lo <= hi, "CounterRange::new({lo}, {hi}): lo > hi");
        Self { lo, hi }
    }

    /// True if this range contains no values (`lo > hi`).
    pub fn is_empty(self) -> bool {
        self.lo > self.hi
    }

    /// True if `self` fully contains `other`.
    pub fn subsumes(self, other: Self) -> bool {
        self.lo <= other.lo && self.hi >= other.hi
    }

    /// Widen to include all values in both ranges.
    /// The caller must ensure the ranges are contiguous (adjacent or overlapping).
    pub fn union(self, other: Self) -> Self {
        let result = Self {
            lo: self.lo.min(other.lo),
            hi: self.hi.max(other.hi),
        };
        debug_assert!(result.lo <= result.hi);
        result
    }

    /// Intersect with `[0, max_exclusive - 1]`.  Returns an empty range if
    /// `lo >= max_exclusive`.
    pub fn intersect_below(self, max_exclusive: u32) -> Self {
        if max_exclusive == 0 || self.lo >= max_exclusive {
            // Use lo > hi to signal empty
            return Self { lo: 1, hi: 0 };
        }
        Self {
            lo: self.lo,
            hi: self.hi.min(max_exclusive - 1),
        }
    }

    /// Intersect with `[min_inclusive, u32::MAX]`.  Returns an empty range if
    /// `hi < min_inclusive`.
    pub fn intersect_above(self, min_inclusive: u32) -> Self {
        if self.hi < min_inclusive {
            return Self { lo: 1, hi: 0 };
        }
        Self {
            lo: self.lo.max(min_inclusive),
            hi: self.hi,
        }
    }
}

/// Definition of a counter used by counted NFA loops.
///
/// Each counter tracks how many times a loop body has been completed.
/// `min` is the minimum iterations for the exit guard; `max` is the
/// maximum iterations for the loop guard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CounterDef {
    /// Minimum completed iterations required to exit
    pub min: u32,
    /// Maximum iterations allowed before loop must exit
    pub max: u32,
    /// Whether the loop body can be traversed without consuming any input.
    /// When true and `num_counters == 1`, the `RangedSingle` fast-forward
    /// path is used in epsilon closure.
    pub body_nullable: bool,
}

/// Upper bound on the number of live counter configurations one frontier may
/// hold (`Counted` scalar configs, `Hybrid` keys, `RangedSingle` entries).
///
/// Finite occurrence bounds are enforced exactly (no `maxOccurs` is ever
/// widened to unbounded), so a pathological model — nested nullable counted
/// groups with very large bounds, e.g. `((a?){0,100000}){0,100000}` — can
/// produce a configuration set proportional to the bound. This limit keeps
/// peak memory and per-child work finite. Exceeding it is an **operational
/// failure** ([`ContentModelLimitExceeded`]), never an acceptance decision:
/// the runtime aborts assessment and reports `validation-resource-limit`.
///
/// Sized so that every model the former 10 000 cutoff admitted still fits
/// (one nullable dimension × states ≈ 80 000 configurations at that bound).
pub const MAX_ACTIVE_CONFIGS: usize = 1 << 18;

/// Upper bound on the work (configurations popped from the worklist) a single
/// epsilon closure may perform. Bounds CPU independently of the final
/// configuration count. See [`MAX_ACTIVE_CONFIGS`].
pub const MAX_CLOSURE_WORK: usize = 1 << 22;

/// Which execution limit a counted content model exceeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContentModelLimit {
    /// More than [`MAX_ACTIVE_CONFIGS`] live configurations in one frontier.
    ActiveConfigurations,
    /// More than [`MAX_CLOSURE_WORK`] configuration visits in one closure.
    ClosureWork,
}

/// A counted content model exceeded an execution limit.
///
/// This is an operational failure of the validator, not a validity verdict:
/// the document may or may not be valid, and the runtime must not report
/// either. It is distinct from a schema error and from `cvc-*` violations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContentModelLimitExceeded {
    /// The limit that was exceeded.
    pub limit: ContentModelLimit,
    /// The value observed when the limit tripped.
    pub observed: usize,
    /// The configured maximum.
    pub allowed: usize,
}

impl std::fmt::Display for ContentModelLimitExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let what = match self.limit {
            ContentModelLimit::ActiveConfigurations => "active counter configurations",
            ContentModelLimit::ClosureWork => "epsilon-closure work units",
        };
        write!(
            f,
            "content-model execution limit exceeded: {} {} > {} allowed",
            self.observed, what, self.allowed
        )
    }
}

impl std::error::Error for ContentModelLimitExceeded {}

#[inline]
fn check_config_limit(observed: usize) -> Result<(), ContentModelLimitExceeded> {
    if observed > MAX_ACTIVE_CONFIGS {
        Err(ContentModelLimitExceeded {
            limit: ContentModelLimit::ActiveConfigurations,
            observed,
            allowed: MAX_ACTIVE_CONFIGS,
        })
    } else {
        Ok(())
    }
}

/// Exact dominance pruning for scalar counter configurations (`Counted`).
///
/// Two configurations at the same NFA state differ only in counter values.
/// `x` dominates `y` when, for every counter `c`, either `x[c] == y[c]`, or
/// `x[c] < y[c]` and `x[c] >= min_c`. Then every guard `y` can pass, `x` can
/// pass too (`CounterMinGuard` needs `>= min`, `CounterMaxGuard` needs
/// `< max`), and the relation is preserved by every transition (`+1` on both,
/// reset or canonical exit to `0` on both). Hence every input accepted from
/// `y` is accepted from `x`, through the same states — so dropping `y`
/// changes neither acceptance, nor attribution (the term lives on the state),
/// nor the expected-element diagnostics (the frontier's state set is
/// unchanged).
///
/// Without this, nested non-nullable counted loops such as
/// `choice{1,100000}(sequence{1,10^8}(a+), b)` (W3C `particlesZ036`) keep one
/// configuration per way of partitioning the input into iterations —
/// O(k²) after k children — although all but a bounded few are dominated.
/// The former 10 000 cutoff hid this by turning such bounds into stars.
fn prune_dominated(configs: HashSet<ActiveConfig>, defs: &[CounterDef]) -> HashSet<ActiveConfig> {
    if configs.len() < 2 {
        return configs;
    }
    let mut by_state: HashMap<StateId, Vec<ActiveConfig>> = HashMap::new();
    for cfg in configs {
        by_state.entry(cfg.state_id).or_default().push(cfg);
    }
    let mut out = HashSet::with_capacity(by_state.len());
    for (_, group) in by_state {
        if group.len() == 1 {
            out.extend(group);
            continue;
        }
        let mut keep: Vec<ActiveConfig> = Vec::with_capacity(group.len());
        'next: for cfg in group {
            for k in &keep {
                if dominates(&k.counters, &cfg.counters, defs) {
                    continue 'next;
                }
            }
            keep.retain(|k| !dominates(&cfg.counters, &k.counters, defs));
            keep.push(cfg);
        }
        out.extend(keep);
    }
    out
}

/// `x` dominates `y` — see [`prune_dominated`]. Equal vectors do not.
fn dominates(x: &[u32], y: &[u32], defs: &[CounterDef]) -> bool {
    let mut strictly = false;
    for (c, (&xv, &yv)) in x.iter().zip(y).enumerate() {
        if xv == yv {
            continue;
        }
        if xv < yv && xv >= defs[c].min {
            strictly = true;
        } else {
            return false;
        }
    }
    strictly
}

#[inline]
fn check_work_limit(observed: usize) -> Result<(), ContentModelLimitExceeded> {
    if observed > MAX_CLOSURE_WORK {
        Err(ContentModelLimitExceeded {
            limit: ContentModelLimit::ClosureWork,
            observed,
            allowed: MAX_CLOSURE_WORK,
        })
    } else {
        Ok(())
    }
}

/// Complete NFA table for a content model
///
/// Represents a compiled content model as a state machine. The NFA can be used
/// for validation by tracking active states and matching input elements.
#[derive(Debug, Clone)]
pub struct NfaTable {
    /// All states in the automaton
    pub states: Vec<NfaState>,
    /// Initial state ID
    pub start_state: StateId,
    /// Accepting state ID (single accept state per Thompson's construction)
    pub accept_state: StateId,
    /// Counter definitions for counted loops (empty if no counted repeats)
    pub counter_defs: Vec<CounterDef>,
    /// Per-state successor closures for the counter-free execution path,
    /// built once per table on first use — see
    /// [`successor_closure`](Self::successor_closure). Invalidated by
    /// [`get_state_mut`](Self::get_state_mut); code that mutates `states`
    /// directly after the table has been executed must rebuild the table.
    succ_closure: OnceLock<Box<[StateSet]>>,
}

impl NfaTable {
    /// Create a new NFA table with the given states (no counters)
    pub fn new(states: Vec<NfaState>, start_state: StateId, accept_state: StateId) -> Self {
        Self {
            states,
            start_state,
            accept_state,
            counter_defs: Vec::new(),
            succ_closure: OnceLock::new(),
        }
    }

    /// Create a new NFA table with counters
    pub fn with_counters(
        states: Vec<NfaState>,
        start_state: StateId,
        accept_state: StateId,
        counter_defs: Vec<CounterDef>,
    ) -> Self {
        Self {
            states,
            start_state,
            accept_state,
            counter_defs,
            succ_closure: OnceLock::new(),
        }
    }

    /// Check if this NFA has any counted loops
    pub fn has_counters(&self) -> bool {
        !self.counter_defs.is_empty()
    }

    /// Get a state by ID
    pub fn get_state(&self, id: StateId) -> Option<&NfaState> {
        self.states.get(id as usize)
    }

    /// Get a mutable reference to a state by ID
    pub fn get_state_mut(&mut self, id: StateId) -> Option<&mut NfaState> {
        // Any structural edit invalidates the cached successor closures.
        self.succ_closure.take();
        self.states.get_mut(id as usize)
    }

    /// The epsilon closure of the *consuming* successors of `state`: exactly
    /// the states reachable after `state`'s term has matched one child.
    ///
    /// Because the epsilon closure of a union is the union of the closures,
    /// one child step on a counter-free model is the OR of these precomputed
    /// bitsets over the matching frontier states — no per-child depth-first
    /// search. Built lazily, once per table, and shared by every
    /// element instance through the `Arc<NfaTable>`. Counter-free tables
    /// only; counted models keep their configuration-based closures.
    #[inline]
    pub fn successor_closure(&self, state: StateId) -> &StateSet {
        let table = self
            .succ_closure
            .get_or_init(|| self.build_successor_closures());
        &table[state as usize]
    }

    fn build_successor_closures(&self) -> Box<[StateSet]> {
        debug_assert!(
            !self.has_counters(),
            "successor closures are only defined for counter-free tables"
        );
        self.states
            .iter()
            .map(|s| epsilon_closure(self, s.consuming_transitions()))
            .collect()
    }

    /// Get the number of states
    pub fn state_count(&self) -> usize {
        self.states.len()
    }

    /// Check if a state is the accept state
    pub fn is_accept(&self, state_id: StateId) -> bool {
        state_id == self.accept_state
    }

    /// Concatenate two NFA tables: self followed by other.
    /// Creates an epsilon transition from self's accept state to other's start state.
    /// Offsets both state IDs and counter IDs in the second table.
    pub fn concat(mut self, mut other: NfaTable) -> NfaTable {
        let state_offset = self.states.len() as StateId;
        let counter_offset = self.counter_defs.len() as CounterId;
        for state in &mut other.states {
            state.id += state_offset;
            for trans in &mut state.transitions {
                trans.target += state_offset;
                trans.kind = trans.kind.offset_counter(counter_offset);
            }
        }
        let other_start = other.start_state + state_offset;
        self.states[self.accept_state as usize].add_epsilon(other_start);
        let new_accept = other.accept_state + state_offset;
        self.states.extend(other.states);
        self.counter_defs.extend(other.counter_defs);
        NfaTable::with_counters(self.states, self.start_state, new_accept, self.counter_defs)
    }

    /// Get all transitions from a given state
    pub fn transitions_from(&self, state_id: StateId) -> &[NfaTransition] {
        self.get_state(state_id)
            .map(|s| s.transitions.as_slice())
            .unwrap_or(&[])
    }
}

/// A single state in the NFA
///
/// Each state can optionally have a term that must be matched to consume input
/// when transitioning through this state. States without terms (epsilon states)
/// are used for branching logic.
#[derive(Debug, Clone)]
pub struct NfaState {
    /// Unique identifier for this state
    pub id: StateId,
    /// The term that must be matched to enter this state via a consuming transition.
    /// None for epsilon states (branching/merging points).
    pub term: Option<NfaTerm>,
    /// Outgoing transitions from this state
    pub transitions: Vec<NfaTransition>,
    /// Source location in the schema for error reporting
    pub origin: Option<SourceRef>,
}

impl NfaState {
    /// Create a new state with no term (epsilon state)
    pub fn epsilon(id: StateId, origin: Option<SourceRef>) -> Self {
        Self {
            id,
            term: None,
            transitions: Vec::new(),
            origin,
        }
    }

    /// Create a new state with a term
    pub fn with_term(id: StateId, term: NfaTerm, origin: Option<SourceRef>) -> Self {
        Self {
            id,
            term: Some(term),
            transitions: Vec::new(),
            origin,
        }
    }

    /// Add a transition to this state
    pub fn add_transition(&mut self, target: StateId, kind: TransitionKind) {
        self.transitions.push(NfaTransition { target, kind });
    }

    /// Add an epsilon transition
    pub fn add_epsilon(&mut self, target: StateId) {
        self.add_transition(target, TransitionKind::Epsilon);
    }

    /// Add a consuming transition
    pub fn add_consume(&mut self, target: StateId) {
        self.add_transition(target, TransitionKind::Consume);
    }

    /// Check if this is an epsilon state (no term)
    pub fn is_epsilon(&self) -> bool {
        self.term.is_none()
    }

    /// Get epsilon transitions from this state
    pub fn epsilon_transitions(&self) -> impl Iterator<Item = StateId> + '_ {
        self.transitions
            .iter()
            .filter(|t| t.kind == TransitionKind::Epsilon)
            .map(|t| t.target)
    }

    /// Get consuming transitions from this state
    pub fn consuming_transitions(&self) -> impl Iterator<Item = StateId> + '_ {
        self.transitions
            .iter()
            .filter(|t| t.kind == TransitionKind::Consume)
            .map(|t| t.target)
    }
}

/// A term that can be matched in the NFA
///
/// Terms represent the actual content that must be matched during validation.
/// Each term corresponds to either a specific element or a wildcard pattern.
#[derive(Debug, Clone)]
pub enum NfaTerm {
    /// Match a specific element
    Element {
        /// Element local name (interned)
        name: NameId,
        /// Element namespace (None = no namespace)
        namespace: Option<NameId>,
        /// Resolved element key (for type lookup during validation)
        element_key: Option<ElementKey>,
        /// Resolved type for local elements (without element key)
        resolved_type: Option<TypeKey>,
    },
    /// Match any element satisfying wildcard constraints
    Wildcard {
        /// Namespace constraint for allowed namespaces
        namespace_constraint: NamespaceConstraint,
        /// How to process matched content
        process_contents: ProcessContents,
        /// Pre-expanded concrete QName exclusions (XSD 1.1 notQName)
        not_qnames: Vec<(Option<NameId>, NameId)>,
    },
}

impl NfaTerm {
    /// Create an element term
    pub fn element(
        name: NameId,
        namespace: Option<NameId>,
        element_key: Option<ElementKey>,
    ) -> Self {
        NfaTerm::Element {
            name,
            namespace,
            element_key,
            resolved_type: None,
        }
    }

    /// Create an element term with a resolved type
    pub fn element_with_type(
        name: NameId,
        namespace: Option<NameId>,
        element_key: Option<ElementKey>,
        resolved_type: Option<TypeKey>,
    ) -> Self {
        NfaTerm::Element {
            name,
            namespace,
            element_key,
            resolved_type,
        }
    }

    /// Create a wildcard term
    pub fn wildcard(
        namespace_constraint: NamespaceConstraint,
        process_contents: ProcessContents,
    ) -> Self {
        NfaTerm::Wildcard {
            namespace_constraint,
            process_contents,
            not_qnames: Vec::new(),
        }
    }

    /// Create a wildcard term with QName exclusions (XSD 1.1)
    pub fn wildcard_with_not_qnames(
        namespace_constraint: NamespaceConstraint,
        process_contents: ProcessContents,
        not_qnames: Vec<(Option<NameId>, NameId)>,
    ) -> Self {
        NfaTerm::Wildcard {
            namespace_constraint,
            process_contents,
            not_qnames,
        }
    }

    /// Check if this term is an element
    pub fn is_element(&self) -> bool {
        matches!(self, NfaTerm::Element { .. })
    }

    /// Check if this term is a wildcard
    pub fn is_wildcard(&self) -> bool {
        matches!(self, NfaTerm::Wildcard { .. })
    }
}

/// A transition between NFA states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NfaTransition {
    /// Target state ID
    pub target: StateId,
    /// Transition type (epsilon or consuming)
    pub kind: TransitionKind,
}

impl NfaTransition {
    /// Create a new transition
    pub fn new(target: StateId, kind: TransitionKind) -> Self {
        Self { target, kind }
    }

    /// Create an epsilon transition
    pub fn epsilon(target: StateId) -> Self {
        Self::new(target, TransitionKind::Epsilon)
    }

    /// Create a consuming transition
    pub fn consume(target: StateId) -> Self {
        Self::new(target, TransitionKind::Consume)
    }
}

/// Type of transition between NFA states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransitionKind {
    /// Epsilon transition (no input consumed, always available)
    Epsilon,
    /// Consuming transition (requires matching the target state's term)
    Consume,
    /// Reset a counter to 0 (entering a counted region)
    CounterReset(CounterId),
    /// Increment a counter by 1 (completed one loop iteration)
    CounterIncrement(CounterId),
    /// Guard: pass only if counter < def.max (loop back)
    CounterMaxGuard(CounterId),
    /// Guard: pass only if counter >= def.min (exit loop)
    CounterMinGuard(CounterId),
}

impl TransitionKind {
    /// Check if this transition is epsilon-like (does not consume input).
    /// All counter transitions are epsilon-like; only `Consume` is not.
    pub fn is_epsilon_like(&self) -> bool {
        !matches!(self, TransitionKind::Consume)
    }

    /// Offset counter IDs by the given amount (for fragment/table composition).
    /// Returns self unchanged for non-counter transitions.
    pub fn offset_counter(self, offset: CounterId) -> Self {
        if offset == 0 {
            return self;
        }
        match self {
            TransitionKind::CounterReset(c) => TransitionKind::CounterReset(c + offset),
            TransitionKind::CounterIncrement(c) => TransitionKind::CounterIncrement(c + offset),
            TransitionKind::CounterMaxGuard(c) => TransitionKind::CounterMaxGuard(c + offset),
            TransitionKind::CounterMinGuard(c) => TransitionKind::CounterMinGuard(c + offset),
            other => other,
        }
    }
}

/// A set of NFA `StateId`s backed by a bitset.
///
/// `StateId` is a dense `u32` (`0..state_count`), so a bitset replaces the old
/// `HashSet<StateId>` on the counter-free (`Simple`) content-model hot path,
/// removing the per-element heap allocation, the per-`StateId` SipHash, and the
/// rehash memcpys. The common case (≤256 NFA states) is stored inline with **no
/// heap allocation**; larger automata spill to a heap `Vec<u64>`.
///
/// Pure safe Rust (no `unsafe`) so it stays trivial under Miri. Iteration order
/// is ascending by `StateId` (deterministic), unlike the previous `HashSet`.
#[derive(Debug, Clone)]
pub enum StateSet {
    /// Up to 256 states stored inline (4 × 64-bit words), zero allocation.
    Inline([u64; STATE_SET_INLINE_WORDS]),
    /// More than 256 states: all words live on the heap.
    Heap(Vec<u64>),
}

/// Inline capacity of [`StateSet`] in 64-bit words (256 states).
pub const STATE_SET_INLINE_WORDS: usize = 4;

impl Default for StateSet {
    fn default() -> Self {
        StateSet::Inline([0; STATE_SET_INLINE_WORDS])
    }
}

impl StateSet {
    /// An empty set with inline storage.
    pub fn new() -> Self {
        StateSet::default()
    }

    /// An empty set pre-sized to hold every state of an NFA of `state_count`
    /// states without growing during insertion (keeps the closure DFS inner
    /// loop free of inline→heap promotion).
    pub fn with_capacity_for(state_count: usize) -> Self {
        let words = state_count.div_ceil(64);
        if words <= STATE_SET_INLINE_WORDS {
            StateSet::Inline([0; STATE_SET_INLINE_WORDS])
        } else {
            StateSet::Heap(vec![0; words])
        }
    }

    /// A singleton set containing just `id`.
    pub fn single(id: StateId) -> Self {
        let mut set = StateSet::with_capacity_for(id as usize + 1);
        set.insert(id);
        set
    }

    /// Read-only view of the backing words.
    #[inline]
    fn words(&self) -> &[u64] {
        match self {
            StateSet::Inline(w) => w,
            StateSet::Heap(w) => w,
        }
    }

    /// Ensure word index `word` is addressable, promoting Inline→Heap if needed,
    /// and return the mutable word slice.
    #[inline]
    fn ensure_words(&mut self, word: usize) -> &mut [u64] {
        // Promote inline → heap first when the word is out of inline range, so
        // the final borrow below is conflict-free.
        if let StateSet::Inline(w) = self {
            if word >= STATE_SET_INLINE_WORDS {
                let mut v = w.to_vec();
                v.resize(word + 1, 0);
                *self = StateSet::Heap(v);
            }
        }
        match self {
            StateSet::Inline(w) => w.as_mut_slice(),
            StateSet::Heap(v) => {
                if word >= v.len() {
                    v.resize(word + 1, 0);
                }
                v.as_mut_slice()
            }
        }
    }

    /// Insert `id`; returns `true` if it was newly added (mirrors
    /// `HashSet::insert`, used to prune the closure DFS).
    #[inline]
    pub fn insert(&mut self, id: StateId) -> bool {
        let word = id as usize / 64;
        let bit = 1u64 << (id as usize % 64);
        let words = self.ensure_words(word);
        let already = words[word] & bit != 0;
        words[word] |= bit;
        !already
    }

    /// `self |= other`: word-wise OR. Promotes to the heap representation only
    /// if `other` has a set bit beyond the inline capacity.
    #[inline]
    pub fn union_with(&mut self, other: &StateSet) {
        let ow = other.words();
        let mut last = ow.len();
        while last > 0 && ow[last - 1] == 0 {
            last -= 1;
        }
        if last == 0 {
            return;
        }
        let words = self.ensure_words(last - 1);
        for (w, &o) in words.iter_mut().zip(&ow[..last]) {
            *w |= o;
        }
    }

    /// Test membership.
    #[inline]
    pub fn contains(&self, id: StateId) -> bool {
        let word = id as usize / 64;
        let bit = 1u64 << (id as usize % 64);
        self.words().get(word).is_some_and(|w| w & bit != 0)
    }

    /// True if no states are set.
    pub fn is_empty(&self) -> bool {
        self.words().iter().all(|&w| w == 0)
    }

    /// Iterate the set states in ascending `StateId` order.
    pub fn iter(&self) -> impl Iterator<Item = StateId> + '_ {
        self.words().iter().enumerate().flat_map(|(wi, &word)| {
            let base = (wi * 64) as StateId;
            StateSetWordIter { word, base }
        })
    }
}

/// Iterator over the set bits of one 64-bit word (ascending), used by
/// [`StateSet::iter`]. Repeatedly extracts the lowest set bit.
struct StateSetWordIter {
    word: u64,
    base: StateId,
}

impl Iterator for StateSetWordIter {
    type Item = StateId;
    #[inline]
    fn next(&mut self) -> Option<StateId> {
        if self.word == 0 {
            return None;
        }
        let bit = self.word.trailing_zeros();
        self.word &= self.word - 1; // clear lowest set bit
        Some(self.base + bit)
    }
}

impl PartialEq for StateSet {
    /// Representation-agnostic: equal iff the same bits are set. Compares the
    /// logical bit content, treating any surplus high words as zero — required
    /// because a pre-sized `Heap` operand and an `Inline` literal can hold the
    /// same set with different word counts.
    fn eq(&self, other: &Self) -> bool {
        let a = self.words();
        let b = other.words();
        let n = a.len().max(b.len());
        (0..n).all(|i| a.get(i).copied().unwrap_or(0) == b.get(i).copied().unwrap_or(0))
    }
}

impl Eq for StateSet {}

thread_local! {
    /// Reusable DFS work-stack for [`epsilon_closure`], so the content-model hot
    /// path does not allocate a fresh `Vec` per call. Cleared on entry. It is
    /// never borrowed re-entrantly: the closure walk only touches `NfaTable` data
    /// and the result `StateSet`, none of which re-enter `epsilon_closure`.
    static EPSILON_STACK: RefCell<Vec<StateId>> = const { RefCell::new(Vec::new()) };
}

/// Compute the epsilon closure for a set of start states.
///
/// Only follows `TransitionKind::Epsilon` transitions. For NFAs with counter
/// transitions, use `ActiveStates` methods instead — this function will miss
/// states reachable via counter transitions.
pub fn epsilon_closure(
    nfa: &NfaTable,
    start_states: impl IntoIterator<Item = StateId>,
) -> StateSet {
    debug_assert!(
        !nfa.has_counters(),
        "epsilon_closure called on counted NFA; use ActiveStates instead"
    );
    let mut closure = StateSet::with_capacity_for(nfa.state_count());
    EPSILON_STACK.with(|cell| {
        let mut stack = cell.borrow_mut();
        stack.clear();
        stack.extend(start_states);

        while let Some(state_id) = stack.pop() {
            if !closure.insert(state_id) {
                continue;
            }
            if let Some(state) = nfa.get_state(state_id) {
                for target in state.epsilon_transitions() {
                    if !closure.contains(target) {
                        stack.push(target);
                    }
                }
            }
        }
    });

    closure
}

/// Match an element name against an NfaTerm with optional substitution groups.
///
/// `_xsd_version` is not used: no wildcard namespace constraint depends on the
/// XSD version (`NamespaceConstraint::matches`).
pub fn term_matches(
    term: &NfaTerm,
    element_name: NameId,
    element_namespace: Option<NameId>,
    target_namespace: Option<NameId>,
    substitution_groups: Option<&SubstitutionGroupMap>,
    _xsd_version: XsdVersion,
) -> bool {
    match term {
        NfaTerm::Element {
            name,
            namespace,
            element_key,
            ..
        } => {
            if let (Some(map), Some(key)) = (substitution_groups, element_key) {
                if let Some(names) = map.get(key) {
                    return names.contains(&(element_name, element_namespace));
                }
            }
            *name == element_name && *namespace == element_namespace
        }
        NfaTerm::Wildcard {
            namespace_constraint,
            not_qnames,
            ..
        } => {
            if !namespace_constraint.matches(element_namespace, target_namespace) {
                return false;
            }
            !not_qnames_exclude(not_qnames, element_namespace, element_name)
        }
    }
}

/// Advance NFA states by matching an element and applying epsilon closure.
///
/// Only handles plain epsilon transitions. For NFAs with counter transitions,
/// use `ActiveStates::advance` instead.
pub fn advance_states(
    nfa: &NfaTable,
    start_states: impl IntoIterator<Item = StateId>,
    element_name: NameId,
    element_namespace: Option<NameId>,
    target_namespace: Option<NameId>,
    substitution_groups: Option<&SubstitutionGroupMap>,
    xsd_version: XsdVersion,
) -> StateSet {
    let closure = epsilon_closure(nfa, start_states);
    let mut next = StateSet::with_capacity_for(nfa.state_count());

    for state_id in closure.iter() {
        let state = match nfa.get_state(state_id) {
            Some(state) => state,
            None => continue,
        };
        let term = match state.term.as_ref() {
            Some(term) => term,
            None => continue,
        };

        if term_matches(
            term,
            element_name,
            element_namespace,
            target_namespace,
            substitution_groups,
            xsd_version,
        ) {
            next.union_with(nfa.successor_closure(state_id));
        }
    }

    next
}

/// Advance NFA states with element-over-wildcard priority (XSD 1.1).
///
/// Only handles plain epsilon transitions. For NFAs with counter transitions,
/// use `ActiveStates::advance_with_priority` instead.
pub fn advance_with_priority(
    nfa: &NfaTable,
    start_states: impl IntoIterator<Item = StateId>,
    element_name: NameId,
    element_namespace: Option<NameId>,
    target_namespace: Option<NameId>,
    substitution_groups: Option<&SubstitutionGroupMap>,
    xsd_version: XsdVersion,
) -> StateSet {
    let closure = epsilon_closure(nfa, start_states);
    let mut element_targets = StateSet::with_capacity_for(nfa.state_count());
    let mut wildcard_targets = StateSet::with_capacity_for(nfa.state_count());

    for state_id in closure.iter() {
        let state = match nfa.get_state(state_id) {
            Some(state) => state,
            None => continue,
        };
        let term = match state.term.as_ref() {
            Some(term) => term,
            None => continue,
        };

        if !term_matches(
            term,
            element_name,
            element_namespace,
            target_namespace,
            substitution_groups,
            xsd_version,
        ) {
            continue;
        }

        match term {
            NfaTerm::Element { .. } => {
                element_targets.union_with(nfa.successor_closure(state_id));
            }
            NfaTerm::Wildcard { .. } => {
                wildcard_targets.union_with(nfa.successor_closure(state_id));
            }
        }
    }

    if !element_targets.is_empty() {
        element_targets
    } else {
        wildcard_targets
    }
}

// ---------------------------------------------------------------------------
// Counter-aware runtime types
// ---------------------------------------------------------------------------

/// A single NFA configuration with counter values.
///
/// Represents one "thread" in the NFA simulation where each counter
/// has a specific value. Two configs at the same state but different
/// counter values are distinct.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct ActiveConfig {
    pub state_id: StateId,
    pub counters: Box<[u32]>,
}

impl ActiveConfig {
    /// Create an initial config at the given state with all counters at 0.
    pub fn initial(state_id: StateId, num_counters: usize) -> Self {
        Self {
            state_id,
            counters: vec![0; num_counters].into_boxed_slice(),
        }
    }

    /// Clone this config with a different state ID.
    fn with_state(&self, new_state: StateId) -> Self {
        Self {
            state_id: new_state,
            counters: self.counters.clone(),
        }
    }

    /// Clone this config with a different state ID and a modified counter.
    fn with_counter_set(&self, new_state: StateId, counter_id: CounterId, value: u32) -> Self {
        let mut new = self.with_state(new_state);
        new.counters[counter_id as usize] = value;
        new
    }
}

/// Key for the `Hybrid` active-state variant.
///
/// Structurally identical to `ActiveConfig` but the slot at `ranged_counter_idx`
/// is **always 0** (sentinel).  All constructors and setters enforce this invariant,
/// so derived `Hash`/`Eq` work correctly without a custom impl.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct HybridKey {
    state_id: StateId,
    counters: Box<[u32]>,
}

impl HybridKey {
    /// Create a key with all counters at 0 (including the ranged slot).
    fn initial(state_id: StateId, num_counters: usize) -> Self {
        Self {
            state_id,
            counters: vec![0; num_counters].into_boxed_slice(),
        }
    }

    /// Clone with a different state, same counter values.
    fn with_state(&self, new_state: StateId) -> Self {
        Self {
            state_id: new_state,
            counters: self.counters.clone(),
        }
    }

    /// Clone with a modified **scalar** counter.
    ///
    /// Panics (debug) if `counter_id` is the ranged counter.
    fn with_scalar_counter(
        &self,
        new_state: StateId,
        counter_id: CounterId,
        value: u32,
        ranged_counter_idx: usize,
    ) -> Self {
        debug_assert!(
            counter_id as usize != ranged_counter_idx,
            "with_scalar_counter called on ranged counter {counter_id}"
        );
        let mut new = self.with_state(new_state);
        new.counters[counter_id as usize] = value;
        new
    }

    /// Read a counter slot value.
    fn counter(&self, counter_id: CounterId) -> u32 {
        self.counters[counter_id as usize]
    }

    /// Repack for dynamic counter switching.
    ///
    /// Precondition: old ranged counter is dead (`[0,0]` in all configs).
    /// Writes 0 into the old ranged slot, zeros the new ranged slot (sentinel).
    /// Returns `(new_key, extracted_scalar_value)` where `extracted_scalar_value`
    /// is the value that was in the new ranged slot (to become a `CounterRange`).
    fn repacked(&self, old_ranged_idx: usize, new_ranged_idx: usize) -> (Self, u32) {
        let scalar_val = self.counters[new_ranged_idx];
        let mut new_counters = self.counters.clone();
        new_counters[old_ranged_idx] = 0;
        new_counters[new_ranged_idx] = 0; // new sentinel
        (
            Self {
                state_id: self.state_id,
                counters: new_counters,
            },
            scalar_val,
        )
    }
}

/// Ranged epsilon closure for single-counter NFAs with nullable body.
///
/// Uses a `HashMap<StateId, CounterRange>` merge-map where each state has
/// exactly one contiguous counter range.  On `CounterIncrement`, the fast-
/// forward sets `hi = counter_def.max`, so the closure converges in O(states)
/// worklist iterations instead of O(max).
///
/// **Contiguity invariant**: every counter range at a state is a single
/// contiguous interval.  Reset→[0,0], increment shifts +1, guards clip one
/// side, fast-forward extends hi — all preserve contiguity.  Ranges arriving
/// at the same state from different paths are always adjacent or overlapping
/// (because the counter increments by exactly 1 per iteration), so union
/// preserves contiguity.
fn ranged_single_epsilon_closure(
    nfa: &NfaTable,
    seeds: HashMap<StateId, CounterRange>,
    counter_def: CounterDef,
) -> Result<ActiveStates, ContentModelLimitExceeded> {
    let mut map: HashMap<StateId, CounterRange> = HashMap::new();
    let mut worklist: VecDeque<(StateId, CounterRange)> = seeds.into_iter().collect();
    let mut work: usize = 0;

    while let Some((state_id, range)) = worklist.pop_front() {
        work += 1;
        check_work_limit(work)?;
        // Check subsumption: if existing range already covers this one, skip.
        if let Some(&existing) = map.get(&state_id) {
            if existing.subsumes(range) {
                continue;
            }
            // Merge (union) — contiguity invariant guarantees this is safe.
            let merged = existing.union(range);
            debug_assert!(
                merged.lo <= merged.hi,
                "contiguity invariant violated at state {state_id}"
            );
            map.insert(state_id, merged);
        } else {
            map.insert(state_id, range);
        }

        // Process transitions from this state.
        if let Some(state) = nfa.get_state(state_id) {
            for trans in &state.transitions {
                let next = match trans.kind {
                    TransitionKind::Epsilon => Some(range),
                    TransitionKind::CounterReset(_) => Some(CounterRange::single(0)),
                    TransitionKind::CounterIncrement(_) => {
                        // Fast-forward: body is nullable, so the counter can
                        // reach max via repeated empty body traversals. The
                        // increment always follows a MaxGuard (`lo < max`), so
                        // `lo + 1 <= max`; the checked form is defensive.
                        range
                            .lo
                            .checked_add(1)
                            .filter(|lo| *lo <= counter_def.max)
                            .map(|lo| CounterRange::new(lo, counter_def.max))
                    }
                    TransitionKind::CounterMaxGuard(_) => {
                        let clamped = range.intersect_below(counter_def.max);
                        if clamped.is_empty() {
                            None
                        } else {
                            Some(clamped)
                        }
                    }
                    TransitionKind::CounterMinGuard(_) => {
                        let passed = range.intersect_above(counter_def.min);
                        if passed.is_empty() {
                            None
                        } else {
                            // Canonicalize: counter is dead after exit.
                            Some(CounterRange::single(0))
                        }
                    }
                    TransitionKind::Consume => None,
                };
                if let Some(next_range) = next {
                    // Only push if not already subsumed by existing map entry.
                    let dominated = map
                        .get(&trans.target)
                        .is_some_and(|r| r.subsumes(next_range));
                    if !dominated {
                        worklist.push_back((trans.target, next_range));
                    }
                }
            }
        }
    }

    Ok(ActiveStates::RangedSingle {
        state_ranges: map,
        counter_def,
    })
}

/// Hybrid epsilon closure for multi-counter NFAs with one ranged counter.
///
/// Combines the `ranged_single_epsilon_closure` fast-forward for the ranged
/// counter with scalar operations for all other counters.  The `HybridKey`
/// captures (state, scalar counter values) while the ranged counter is stored
/// as a `CounterRange` map value.
///
/// **Contiguity invariant** holds per-`HybridKey`: for a fixed set of scalar
/// counter values, the ranged counter progresses identically to `RangedSingle`.
fn hybrid_epsilon_closure(
    nfa: &NfaTable,
    seeds: HashMap<HybridKey, CounterRange>,
    ranged_counter_idx: usize,
    num_counters: usize,
) -> Result<ActiveStates, ContentModelLimitExceeded> {
    let ranged_id = ranged_counter_idx as CounterId;
    let ranged_def = nfa.counter_defs[ranged_counter_idx];

    let mut map: HashMap<HybridKey, CounterRange> = HashMap::new();
    let mut worklist: VecDeque<(HybridKey, CounterRange)> = seeds.into_iter().collect();
    let mut work: usize = 0;

    while let Some((key, range)) = worklist.pop_front() {
        work += 1;
        check_work_limit(work)?;
        // Subsumption check: if existing range already covers this one, skip.
        if let Some(&existing) = map.get(&key) {
            if existing.subsumes(range) {
                continue;
            }
            let merged = existing.union(range);
            debug_assert!(
                merged.lo <= merged.hi,
                "contiguity invariant violated at state {}",
                key.state_id
            );
            map.insert(key.clone(), merged);
        } else {
            map.insert(key.clone(), range);
            check_config_limit(map.len())?;
        }

        if let Some(state) = nfa.get_state(key.state_id) {
            for trans in &state.transitions {
                let next: Option<(HybridKey, CounterRange)> = match trans.kind {
                    TransitionKind::Epsilon => Some((key.with_state(trans.target), range)),

                    // --- Ranged counter operations (range arithmetic) ---
                    TransitionKind::CounterReset(c) if c == ranged_id => {
                        Some((key.with_state(trans.target), CounterRange::single(0)))
                    }
                    TransitionKind::CounterIncrement(c) if c == ranged_id => {
                        // Fast-forward: body is nullable, so counter can reach max.
                        // `lo < max` is guaranteed by the preceding MaxGuard.
                        range
                            .lo
                            .checked_add(1)
                            .filter(|lo| *lo <= ranged_def.max)
                            .map(|lo| {
                                (
                                    key.with_state(trans.target),
                                    CounterRange::new(lo, ranged_def.max),
                                )
                            })
                    }
                    TransitionKind::CounterMaxGuard(c) if c == ranged_id => {
                        let clamped = range.intersect_below(ranged_def.max);
                        if clamped.is_empty() {
                            None
                        } else {
                            Some((key.with_state(trans.target), clamped))
                        }
                    }
                    TransitionKind::CounterMinGuard(c) if c == ranged_id => {
                        let passed = range.intersect_above(ranged_def.min);
                        if passed.is_empty() {
                            None
                        } else {
                            // Canonicalize: ranged counter is dead after exit.
                            Some((key.with_state(trans.target), CounterRange::single(0)))
                        }
                    }

                    // --- Scalar counter operations ---
                    TransitionKind::CounterReset(c) => Some((
                        key.with_scalar_counter(trans.target, c, 0, ranged_counter_idx),
                        range,
                    )),
                    TransitionKind::CounterIncrement(c) => {
                        key.counter(c).checked_add(1).map(|val| {
                            (
                                key.with_scalar_counter(trans.target, c, val, ranged_counter_idx),
                                range,
                            )
                        })
                    }
                    TransitionKind::CounterMaxGuard(c) => {
                        if key.counter(c) < nfa.counter_defs[c as usize].max {
                            Some((key.with_state(trans.target), range))
                        } else {
                            None
                        }
                    }
                    TransitionKind::CounterMinGuard(c) => {
                        if key.counter(c) >= nfa.counter_defs[c as usize].min {
                            // Canonicalize scalar counter on exit.
                            Some((
                                key.with_scalar_counter(trans.target, c, 0, ranged_counter_idx),
                                range,
                            ))
                        } else {
                            None
                        }
                    }

                    TransitionKind::Consume => None,
                };

                if let Some((next_key, next_range)) = next {
                    let dominated = map.get(&next_key).is_some_and(|r| r.subsumes(next_range));
                    if !dominated {
                        worklist.push_back((next_key, next_range));
                    }
                }
            }
        }
    }

    let result = ActiveStates::Hybrid {
        configs: map,
        ranged_counter_idx,
        num_counters,
    };
    Ok(result.maybe_switch_ranged_counter(nfa))
}

/// Quad-path active state set for NFA simulation.
///
/// **Invariant**: values are always closure-closed (epsilon closure has been
/// applied). All constructors and advance methods enforce this.
///
/// - `Simple`: counter-free NFA — stores reachable states in a [`StateSet`] bitset.
/// - `Counted`: NFA with multiple counters — tracks `HashSet<ActiveConfig>` (scalar).
/// - `RangedSingle`: single counter with nullable body — stores one `CounterRange`
///   per reachable state, converging epsilon closure in O(states) instead of O(N).
/// - `Hybrid`: multiple counters, one ranged (fast-forward) + others scalar.
///   Collapses the ranged counter's dimension; cost proportional to remaining scalar dims.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActiveStates {
    /// Fast path: no counters in this NFA — reachable states in a bitset.
    Simple(StateSet),
    /// Scalar counted path: configurations carry individual counter values
    Counted {
        configs: HashSet<ActiveConfig>,
        num_counters: usize,
    },
    /// Optimized path for single-counter NFAs with nullable loop body.
    /// Each reachable state maps to one `CounterRange` (contiguity invariant).
    RangedSingle {
        state_ranges: HashMap<StateId, CounterRange>,
        counter_def: CounterDef,
    },
    /// Hybrid path: one counter ranged (fast-forward), others scalar.
    /// The ranged counter's slot in every `HybridKey` is always 0 (sentinel);
    /// the actual ranged value lives in the `CounterRange` map value.
    Hybrid {
        configs: HashMap<HybridKey, CounterRange>,
        ranged_counter_idx: usize,
        num_counters: usize,
    },
}

impl ActiveStates {
    /// Fused "find match info + advance" for the content-model hot path.
    ///
    /// Replaces the previous two passes over the active set (`find_match_info`
    /// then `advance_from`/`advance_with_priority_from`) with a **single
    /// traversal** for the `Simple` (counter-free) variant. Returns:
    /// - the element/wildcard [`MatchInfo`] using element-over-wildcard priority,
    ///   which is **version-independent** (mirrors `find_match_info_in_states`), and
    /// - the next frontier, whose target selection **is** version-dependent:
    ///   V1.0 unions element+wildcard targets (mirrors `advance_states`), V1.1
    ///   prefers element targets over wildcard (mirrors `advance_with_priority`).
    ///
    /// Counter-bearing variants delegate to the existing methods with no
    /// behavior change.
    pub(crate) fn advance_element_with_info(
        &self,
        nfa: &NfaTable,
        name: NameId,
        namespace: Option<NameId>,
        target_ns: Option<NameId>,
        subst_groups: Option<&SubstitutionGroupMap>,
        xsd_version: XsdVersion,
    ) -> Result<(MatchInfo, ActiveStates), ContentModelLimitExceeded> {
        let states = match self {
            ActiveStates::Simple(states) => states,
            // Counter-bearing variants keep the existing two-call behavior.
            _ => {
                let info = self.find_match_info(
                    nfa,
                    name,
                    namespace,
                    target_ns,
                    subst_groups,
                    xsd_version,
                );
                let next = match xsd_version {
                    XsdVersion::V1_0 => self.advance_from(
                        nfa,
                        name,
                        namespace,
                        target_ns,
                        subst_groups,
                        xsd_version,
                    ),
                    XsdVersion::V1_1 => self.advance_with_priority_from(
                        nfa,
                        name,
                        namespace,
                        target_ns,
                        subst_groups,
                        xsd_version,
                    ),
                }?;
                return Ok((info, next));
            }
        };

        // Single pass over the (already closure-closed) Simple active set,
        // capturing MatchInfo and collecting consuming targets at once.
        let mut element_info: Option<MatchInfo> = None;
        let mut wildcard_info: Option<MatchInfo> = None;
        let mut element_targets = StateSet::with_capacity_for(nfa.state_count());
        let mut wildcard_targets = StateSet::with_capacity_for(nfa.state_count());

        for state_id in states.iter() {
            let state = match nfa.get_state(state_id) {
                Some(state) => state,
                None => continue,
            };
            let term = match state.term.as_ref() {
                Some(term) => term,
                None => continue,
            };
            if !term_matches(term, name, namespace, target_ns, subst_groups, xsd_version) {
                continue;
            }
            match term {
                NfaTerm::Element {
                    name: term_name,
                    namespace: term_ns,
                    element_key,
                    resolved_type,
                } => {
                    element_targets.union_with(nfa.successor_closure(state_id));
                    // First matching element wins (element-over-wildcard priority).
                    if element_info.is_none() {
                        element_info = Some(if *term_name == name && *term_ns == namespace {
                            // Direct match — return the term's declaration info.
                            MatchInfo {
                                element_key: *element_key,
                                resolved_type: *resolved_type,
                                process_contents: None,
                            }
                        } else {
                            // Substitution match — runtime resolves the actual member.
                            MatchInfo::default()
                        });
                    }
                }
                NfaTerm::Wildcard {
                    process_contents, ..
                } => {
                    wildcard_targets.union_with(nfa.successor_closure(state_id));
                    // Keep the first matching wildcard as the fallback.
                    if wildcard_info.is_none() {
                        wildcard_info = Some(MatchInfo {
                            element_key: None,
                            resolved_type: None,
                            process_contents: Some(*process_contents),
                        });
                    }
                }
            }
        }

        let match_info = element_info.or(wildcard_info).unwrap_or_default();

        // Target selection mirrors the free advance functions exactly.
        let chosen = match xsd_version {
            XsdVersion::V1_0 => {
                // Union element + wildcard targets (mirrors `advance_states`).
                let mut all = element_targets;
                all.union_with(&wildcard_targets);
                all
            }
            // Element targets, else wildcard (mirrors `advance_with_priority`).
            XsdVersion::V1_1 => {
                if !element_targets.is_empty() {
                    element_targets
                } else {
                    wildcard_targets
                }
            }
        };
        // `chosen` is a union of precomputed closures, hence already closed.
        let next = ActiveStates::Simple(chosen);
        Ok((match_info, next))
    }

    /// Borrowed transition helper used by content validation hot paths to
    /// compute the next frontier without cloning the current one first.
    pub(crate) fn advance_from(
        &self,
        nfa: &NfaTable,
        element_name: NameId,
        element_namespace: Option<NameId>,
        target_namespace: Option<NameId>,
        substitution_groups: Option<&SubstitutionGroupMap>,
        xsd_version: XsdVersion,
    ) -> Result<Self, ContentModelLimitExceeded> {
        match self {
            ActiveStates::Simple(states) => Ok(ActiveStates::Simple(advance_states(
                nfa,
                states.iter(),
                element_name,
                element_namespace,
                target_namespace,
                substitution_groups,
                xsd_version,
            ))),
            ActiveStates::Counted {
                configs,
                num_counters,
            } => {
                let mut next_configs = HashSet::new();
                for config in configs {
                    if let Some(state) = nfa.get_state(config.state_id) {
                        if let Some(ref term_val) = state.term {
                            if term_matches(
                                term_val,
                                element_name,
                                element_namespace,
                                target_namespace,
                                substitution_groups,
                                xsd_version,
                            ) {
                                for trans in &state.transitions {
                                    if trans.kind == TransitionKind::Consume {
                                        next_configs.insert(config.with_state(trans.target));
                                    }
                                }
                            }
                        }
                    }
                }
                let result = ActiveStates::Counted {
                    configs: next_configs,
                    num_counters: *num_counters,
                };
                result.try_epsilon_closure(nfa)
            }
            ActiveStates::RangedSingle {
                state_ranges,
                counter_def,
            } => {
                let mut next_seeds: HashMap<StateId, CounterRange> = HashMap::new();
                for (&state_id, &range) in state_ranges {
                    if let Some(state) = nfa.get_state(state_id) {
                        if let Some(ref term_val) = state.term {
                            if term_matches(
                                term_val,
                                element_name,
                                element_namespace,
                                target_namespace,
                                substitution_groups,
                                xsd_version,
                            ) {
                                for trans in &state.transitions {
                                    if trans.kind == TransitionKind::Consume {
                                        next_seeds
                                            .entry(trans.target)
                                            .and_modify(|r| *r = r.union(range))
                                            .or_insert(range);
                                    }
                                }
                            }
                        }
                    }
                }
                let result = ActiveStates::RangedSingle {
                    state_ranges: next_seeds,
                    counter_def: *counter_def,
                };
                result.try_epsilon_closure(nfa)
            }
            ActiveStates::Hybrid {
                configs,
                ranged_counter_idx,
                num_counters,
            } => {
                let mut next_configs: HashMap<HybridKey, CounterRange> = HashMap::new();
                for (key, &range) in configs {
                    if let Some(state) = nfa.get_state(key.state_id) {
                        if let Some(ref term_val) = state.term {
                            if term_matches(
                                term_val,
                                element_name,
                                element_namespace,
                                target_namespace,
                                substitution_groups,
                                xsd_version,
                            ) {
                                for trans in &state.transitions {
                                    if trans.kind == TransitionKind::Consume {
                                        let new_key = key.with_state(trans.target);
                                        next_configs
                                            .entry(new_key)
                                            .and_modify(|r| *r = r.union(range))
                                            .or_insert(range);
                                    }
                                }
                            }
                        }
                    }
                }
                let result = ActiveStates::Hybrid {
                    configs: next_configs,
                    ranged_counter_idx: *ranged_counter_idx,
                    num_counters: *num_counters,
                };
                result.try_epsilon_closure(nfa)
            }
        }
    }

    /// Borrowed priority transition helper used by content validation hot
    /// paths to avoid cloning the current frontier before matching.
    pub(crate) fn advance_with_priority_from(
        &self,
        nfa: &NfaTable,
        element_name: NameId,
        element_namespace: Option<NameId>,
        target_namespace: Option<NameId>,
        substitution_groups: Option<&SubstitutionGroupMap>,
        xsd_version: XsdVersion,
    ) -> Result<Self, ContentModelLimitExceeded> {
        match self {
            ActiveStates::Simple(states) => Ok(ActiveStates::Simple(advance_with_priority(
                nfa,
                states.iter(),
                element_name,
                element_namespace,
                target_namespace,
                substitution_groups,
                xsd_version,
            ))),
            ActiveStates::Counted {
                configs,
                num_counters,
            } => {
                let mut element_configs = HashSet::new();
                let mut wildcard_configs = HashSet::new();

                for config in configs {
                    if let Some(state) = nfa.get_state(config.state_id) {
                        if let Some(ref term_val) = state.term {
                            if term_matches(
                                term_val,
                                element_name,
                                element_namespace,
                                target_namespace,
                                substitution_groups,
                                xsd_version,
                            ) {
                                let target_set = match term_val {
                                    NfaTerm::Element { .. } => &mut element_configs,
                                    NfaTerm::Wildcard { .. } => &mut wildcard_configs,
                                };
                                for trans in &state.transitions {
                                    if trans.kind == TransitionKind::Consume {
                                        target_set.insert(config.with_state(trans.target));
                                    }
                                }
                            }
                        }
                    }
                }

                let next = if !element_configs.is_empty() {
                    element_configs
                } else {
                    wildcard_configs
                };

                let result = ActiveStates::Counted {
                    configs: next,
                    num_counters: *num_counters,
                };
                result.try_epsilon_closure(nfa)
            }
            ActiveStates::RangedSingle {
                state_ranges,
                counter_def,
            } => {
                let mut element_seeds: HashMap<StateId, CounterRange> = HashMap::new();
                let mut wildcard_seeds: HashMap<StateId, CounterRange> = HashMap::new();

                for (&state_id, &range) in state_ranges {
                    if let Some(state) = nfa.get_state(state_id) {
                        if let Some(ref term_val) = state.term {
                            if term_matches(
                                term_val,
                                element_name,
                                element_namespace,
                                target_namespace,
                                substitution_groups,
                                xsd_version,
                            ) {
                                let target_map = match term_val {
                                    NfaTerm::Element { .. } => &mut element_seeds,
                                    NfaTerm::Wildcard { .. } => &mut wildcard_seeds,
                                };
                                for trans in &state.transitions {
                                    if trans.kind == TransitionKind::Consume {
                                        target_map
                                            .entry(trans.target)
                                            .and_modify(|r| *r = r.union(range))
                                            .or_insert(range);
                                    }
                                }
                            }
                        }
                    }
                }

                let next = if !element_seeds.is_empty() {
                    element_seeds
                } else {
                    wildcard_seeds
                };

                let result = ActiveStates::RangedSingle {
                    state_ranges: next,
                    counter_def: *counter_def,
                };
                result.try_epsilon_closure(nfa)
            }
            ActiveStates::Hybrid {
                configs,
                ranged_counter_idx,
                num_counters,
            } => {
                let mut element_configs: HashMap<HybridKey, CounterRange> = HashMap::new();
                let mut wildcard_configs: HashMap<HybridKey, CounterRange> = HashMap::new();

                for (key, &range) in configs {
                    if let Some(state) = nfa.get_state(key.state_id) {
                        if let Some(ref term_val) = state.term {
                            if term_matches(
                                term_val,
                                element_name,
                                element_namespace,
                                target_namespace,
                                substitution_groups,
                                xsd_version,
                            ) {
                                let target_map = match term_val {
                                    NfaTerm::Element { .. } => &mut element_configs,
                                    NfaTerm::Wildcard { .. } => &mut wildcard_configs,
                                };
                                for trans in &state.transitions {
                                    if trans.kind == TransitionKind::Consume {
                                        let new_key = key.with_state(trans.target);
                                        target_map
                                            .entry(new_key)
                                            .and_modify(|r| *r = r.union(range))
                                            .or_insert(range);
                                    }
                                }
                            }
                        }
                    }
                }

                let next = if !element_configs.is_empty() {
                    element_configs
                } else {
                    wildcard_configs
                };

                let result = ActiveStates::Hybrid {
                    configs: next,
                    ranged_counter_idx: *ranged_counter_idx,
                    num_counters: *num_counters,
                };
                result.try_epsilon_closure(nfa)
            }
        }
    }

    /// Create initial active states from an NFA, picking the right variant.
    pub fn from_nfa(nfa: &NfaTable) -> Self {
        Self::try_from_nfa(nfa)
            .unwrap_or_else(|e| panic!("ActiveStates::from_nfa: {e}; use try_from_nfa"))
    }

    /// Fallible form of [`from_nfa`](Self::from_nfa).
    ///
    /// The initial closure of a counted model with nested nullable loops can
    /// itself exceed [`MAX_ACTIVE_CONFIGS`] / [`MAX_CLOSURE_WORK`]; this
    /// returns that as an error instead of panicking. The validation runtime
    /// uses this form and turns the error into an operational failure.
    pub fn try_from_nfa(nfa: &NfaTable) -> Result<Self, ContentModelLimitExceeded> {
        use super::particle::COUNTED_THRESHOLD;

        if nfa.has_counters() {
            // Single counter with nullable body → optimized ranged path
            if nfa.counter_defs.len() == 1 && nfa.counter_defs[0].body_nullable {
                let mut state_ranges = HashMap::new();
                state_ranges.insert(nfa.start_state, CounterRange::single(0));
                let result = ActiveStates::RangedSingle {
                    state_ranges,
                    counter_def: nfa.counter_defs[0],
                };
                return result.try_epsilon_closure(nfa);
            }

            // Multiple counters: check for a nullable counter worth ranging.
            // Pick the nullable counter with the largest max — that collapses
            // the most configurations.  Only use Hybrid if max > COUNTED_THRESHOLD
            // (below that, Counted is cheap enough and we avoid map overhead).
            // On equal max, prefer the lowest index — for sequential loops this
            // ranges the first loop's counter, keeping later counters at scalar 0.
            if nfa.counter_defs.len() > 1 {
                let best_nullable = nfa
                    .counter_defs
                    .iter()
                    .enumerate()
                    .filter(|(_, def)| def.body_nullable)
                    .max_by_key(|(idx, def)| (def.max, std::cmp::Reverse(*idx)));

                if let Some((ranged_idx, def)) = best_nullable {
                    if def.max > COUNTED_THRESHOLD {
                        let num_counters = nfa.counter_defs.len();
                        let initial_key = HybridKey::initial(nfa.start_state, num_counters);
                        let mut configs = HashMap::new();
                        configs.insert(initial_key, CounterRange::single(0));
                        let result = ActiveStates::Hybrid {
                            configs,
                            ranged_counter_idx: ranged_idx,
                            num_counters,
                        };
                        return result.try_epsilon_closure(nfa);
                    }
                }
            }

            // Fall through to scalar Counted path
            let initial = ActiveConfig::initial(nfa.start_state, nfa.counter_defs.len());
            let mut configs = HashSet::new();
            configs.insert(initial);
            let result = ActiveStates::Counted {
                configs,
                num_counters: nfa.counter_defs.len(),
            };
            result.try_epsilon_closure(nfa)
        } else {
            let simple = epsilon_closure(nfa, std::iter::once(nfa.start_state));
            Ok(ActiveStates::Simple(simple))
        }
    }

    /// Check if the active set is empty (no reachable states).
    pub fn is_empty(&self) -> bool {
        match self {
            ActiveStates::Simple(s) => s.is_empty(),
            ActiveStates::Counted { configs, .. } => configs.is_empty(),
            ActiveStates::RangedSingle { state_ranges, .. } => state_ranges.is_empty(),
            ActiveStates::Hybrid { configs, .. } => configs.is_empty(),
        }
    }

    /// Check if any active state is the NFA accept state.
    pub fn contains_accept(&self, nfa: &NfaTable) -> bool {
        match self {
            ActiveStates::Simple(s) => s.contains(nfa.accept_state),
            ActiveStates::Counted { configs, .. } => {
                configs.iter().any(|c| nfa.is_accept(c.state_id))
            }
            ActiveStates::RangedSingle { state_ranges, .. } => {
                state_ranges.contains_key(&nfa.accept_state)
            }
            ActiveStates::Hybrid { configs, .. } => {
                configs.keys().any(|k| nfa.is_accept(k.state_id))
            }
        }
    }

    /// Compute epsilon closure (including counter transitions for Counted path).
    pub fn epsilon_closure(self, nfa: &NfaTable) -> Self {
        self.try_epsilon_closure(nfa).unwrap_or_else(|e| {
            panic!("ActiveStates::epsilon_closure: {e}; use try_epsilon_closure")
        })
    }

    /// Fallible form of [`epsilon_closure`](Self::epsilon_closure): enforces
    /// [`MAX_ACTIVE_CONFIGS`] and [`MAX_CLOSURE_WORK`] on the counted paths
    /// (the counter-free `Simple` path is bounded by the state count and
    /// cannot fail). Limits are checked *while* the configuration set grows,
    /// so peak memory is bounded, not just the final size.
    pub fn try_epsilon_closure(self, nfa: &NfaTable) -> Result<Self, ContentModelLimitExceeded> {
        match self {
            ActiveStates::Simple(states) => {
                Ok(ActiveStates::Simple(epsilon_closure(nfa, states.iter())))
            }
            ActiveStates::Counted {
                configs,
                num_counters,
            } => {
                let mut result: HashSet<ActiveConfig> = HashSet::new();
                let mut stack: Vec<ActiveConfig> = configs.into_iter().collect();
                let mut work: usize = 0;

                while let Some(config) = stack.pop() {
                    work += 1;
                    check_work_limit(work)?;
                    if !result.insert(config.clone()) {
                        continue;
                    }
                    check_config_limit(result.len())?;
                    if let Some(state) = nfa.get_state(config.state_id) {
                        for trans in &state.transitions {
                            let next = match trans.kind {
                                TransitionKind::Epsilon => Some(config.with_state(trans.target)),
                                TransitionKind::CounterReset(c) => {
                                    Some(config.with_counter_set(trans.target, c, 0))
                                }
                                TransitionKind::CounterIncrement(c) => config.counters[c as usize]
                                    .checked_add(1)
                                    .map(|val| config.with_counter_set(trans.target, c, val)),
                                TransitionKind::CounterMaxGuard(c) => {
                                    if config.counters[c as usize]
                                        < nfa.counter_defs[c as usize].max
                                    {
                                        Some(config.with_state(trans.target))
                                    } else {
                                        None
                                    }
                                }
                                TransitionKind::CounterMinGuard(c) => {
                                    if config.counters[c as usize]
                                        >= nfa.counter_defs[c as usize].min
                                    {
                                        // Canonicalize: zero out counter on exit to collapse
                                        // configs that left the loop at different counter values.
                                        Some(config.with_counter_set(trans.target, c, 0))
                                    } else {
                                        None
                                    }
                                }
                                TransitionKind::Consume => None,
                            };
                            if let Some(next_config) = next {
                                if !result.contains(&next_config) {
                                    stack.push(next_config);
                                }
                            }
                        }
                    }
                }
                Ok(ActiveStates::Counted {
                    configs: prune_dominated(result, &nfa.counter_defs),
                    num_counters,
                })
            }
            ActiveStates::RangedSingle {
                state_ranges,
                counter_def,
            } => ranged_single_epsilon_closure(nfa, state_ranges, counter_def),
            ActiveStates::Hybrid {
                configs,
                ranged_counter_idx,
                num_counters,
            } => hybrid_epsilon_closure(nfa, configs, ranged_counter_idx, num_counters),
        }
    }

    /// After epsilon closure, check if the current ranged counter is dead
    /// (`[0,0]` in ALL configs — the canonical exit value from `CounterMinGuard`).
    /// If a better nullable counter exists and the repack produces contiguous
    /// ranges, switch the ranged counter.
    ///
    /// The repacked map is already epsilon-closed — no re-closure needed, since
    /// the same set of (state, counter-values) is preserved, just re-encoded.
    fn maybe_switch_ranged_counter(self, nfa: &NfaTable) -> Self {
        use super::particle::COUNTED_THRESHOLD;

        let ActiveStates::Hybrid {
            configs,
            ranged_counter_idx,
            num_counters,
        } = self
        else {
            return self;
        };

        let dead = CounterRange::single(0);
        if configs.is_empty() || !configs.values().all(|r| *r == dead) {
            return ActiveStates::Hybrid {
                configs,
                ranged_counter_idx,
                num_counters,
            };
        }

        // Single pass: collect per-counter min/max across all configs.
        let nc = nfa.counter_defs.len();
        let mut min_vals = vec![u32::MAX; nc];
        let mut max_vals = vec![0u32; nc];
        for key in configs.keys() {
            for idx in 0..nc {
                let v = key.counter(idx as CounterId);
                min_vals[idx] = min_vals[idx].min(v);
                max_vals[idx] = max_vals[idx].max(v);
            }
        }

        // Pick the best candidate: nullable, large max, widest spread.
        // Spread (max - min + 1) is an upper bound on distinct values;
        // the contiguity guard below rejects non-contiguous cases.
        let mut best: Option<(usize, u32, u32)> = None; // (idx, spread, max)
        for (idx, def) in nfa.counter_defs.iter().enumerate() {
            if idx == ranged_counter_idx || !def.body_nullable || def.max <= COUNTED_THRESHOLD {
                continue;
            }
            if min_vals[idx] == max_vals[idx] {
                continue;
            }
            let spread = max_vals[idx] - min_vals[idx] + 1;
            let dominated =
                best.is_some_and(|(_, bs, bm)| spread < bs || (spread == bs && def.max <= bm));
            if !dominated {
                best = Some((idx, spread, def.max));
            }
        }

        let Some((new_ranged_idx, _, _)) = best else {
            return ActiveStates::Hybrid {
                configs,
                ranged_counter_idx,
                num_counters,
            };
        };

        // Guarded repack: track (range, count) per key, then verify that
        // range width == count (no phantom values in the interval).
        let mut new_configs: HashMap<HybridKey, (CounterRange, u32)> =
            HashMap::with_capacity(configs.len());
        for old_key in configs.keys() {
            let (new_key, scalar_val) = old_key.repacked(ranged_counter_idx, new_ranged_idx);
            let new_range = CounterRange::single(scalar_val);
            new_configs
                .entry(new_key)
                .and_modify(|(r, count)| {
                    *r = r.union(new_range);
                    *count += 1;
                })
                .or_insert((new_range, 1));
        }

        for (range, count) in new_configs.values() {
            if range.hi - range.lo + 1 != *count {
                return ActiveStates::Hybrid {
                    configs,
                    ranged_counter_idx,
                    num_counters,
                };
            }
        }

        let repacked = new_configs.into_iter().map(|(k, (r, _))| (k, r)).collect();

        ActiveStates::Hybrid {
            configs: repacked,
            ranged_counter_idx: new_ranged_idx,
            num_counters,
        }
    }

    /// Advance NFA states by matching an element (XSD 1.0 — no priority).
    pub fn advance(
        self,
        nfa: &NfaTable,
        element_name: NameId,
        element_namespace: Option<NameId>,
        target_namespace: Option<NameId>,
        substitution_groups: Option<&SubstitutionGroupMap>,
        xsd_version: XsdVersion,
    ) -> Self {
        self.try_advance(
            nfa,
            element_name,
            element_namespace,
            target_namespace,
            substitution_groups,
            xsd_version,
        )
        .unwrap_or_else(|e| panic!("ActiveStates::advance: {e}; use try_advance"))
    }

    /// Fallible form of [`advance`](Self::advance): returns
    /// [`ContentModelLimitExceeded`] instead of panicking when a counted
    /// model exceeds an execution limit.
    pub fn try_advance(
        self,
        nfa: &NfaTable,
        element_name: NameId,
        element_namespace: Option<NameId>,
        target_namespace: Option<NameId>,
        substitution_groups: Option<&SubstitutionGroupMap>,
        xsd_version: XsdVersion,
    ) -> Result<Self, ContentModelLimitExceeded> {
        self.advance_from(
            nfa,
            element_name,
            element_namespace,
            target_namespace,
            substitution_groups,
            xsd_version,
        )
    }

    /// Advance NFA states with element-over-wildcard priority (XSD 1.1).
    pub fn advance_with_priority(
        self,
        nfa: &NfaTable,
        element_name: NameId,
        element_namespace: Option<NameId>,
        target_namespace: Option<NameId>,
        substitution_groups: Option<&SubstitutionGroupMap>,
        xsd_version: XsdVersion,
    ) -> Self {
        self.try_advance_with_priority(
            nfa,
            element_name,
            element_namespace,
            target_namespace,
            substitution_groups,
            xsd_version,
        )
        .unwrap_or_else(|e| {
            panic!("ActiveStates::advance_with_priority: {e}; use try_advance_with_priority")
        })
    }

    /// Fallible form of [`advance_with_priority`](Self::advance_with_priority).
    pub fn try_advance_with_priority(
        self,
        nfa: &NfaTable,
        element_name: NameId,
        element_namespace: Option<NameId>,
        target_namespace: Option<NameId>,
        substitution_groups: Option<&SubstitutionGroupMap>,
        xsd_version: XsdVersion,
    ) -> Result<Self, ContentModelLimitExceeded> {
        self.advance_with_priority_from(
            nfa,
            element_name,
            element_namespace,
            target_namespace,
            substitution_groups,
            xsd_version,
        )
    }

    /// Find the matching term info from active states (for content validation).
    ///
    /// Returns the element key and resolved type of the first matching term.
    /// Since ActiveStates is closure-closed, all reachable term states are
    /// already present — no additional epsilon closure needed for Counted path.
    pub fn find_match_info(
        &self,
        nfa: &NfaTable,
        name: NameId,
        namespace: Option<NameId>,
        target_ns: Option<NameId>,
        subst_groups: Option<&SubstitutionGroupMap>,
        xsd_version: XsdVersion,
    ) -> MatchInfo {
        match self {
            ActiveStates::Simple(states) => {
                // States are already closure-closed (the `ActiveStates` invariant),
                // so iterate them directly — no epsilon closure needed.
                find_match_info_in_states(
                    nfa,
                    states.iter(),
                    name,
                    namespace,
                    target_ns,
                    subst_groups,
                    xsd_version,
                )
            }
            ActiveStates::Counted { configs, .. } => {
                // Configs are closure-closed — iterate directly
                find_match_info_in_states(
                    nfa,
                    configs.iter().map(|c| c.state_id),
                    name,
                    namespace,
                    target_ns,
                    subst_groups,
                    xsd_version,
                )
            }
            ActiveStates::RangedSingle { state_ranges, .. } => find_match_info_in_states(
                nfa,
                state_ranges.keys().copied(),
                name,
                namespace,
                target_ns,
                subst_groups,
                xsd_version,
            ),
            ActiveStates::Hybrid { configs, .. } => find_match_info_in_states(
                nfa,
                configs.keys().map(|k| k.state_id),
                name,
                namespace,
                target_ns,
                subst_groups,
                xsd_version,
            ),
        }
    }

    /// Collect expected element terms from reachable states (for error messages).
    ///
    /// Returns (local_name, namespace, element_key) for each reachable Element term.
    pub fn expected_element_terms(
        &self,
        nfa: &NfaTable,
    ) -> Vec<(NameId, Option<NameId>, Option<ElementKey>)> {
        let mut result = Vec::new();
        match self {
            ActiveStates::Simple(states) => {
                // States are already closure-closed — iterate directly.
                for state_id in states.iter() {
                    if let Some(state) = nfa.get_state(state_id) {
                        if let Some(NfaTerm::Element {
                            name,
                            namespace,
                            element_key,
                            ..
                        }) = &state.term
                        {
                            result.push((*name, *namespace, *element_key));
                        }
                    }
                }
            }
            ActiveStates::Counted { configs, .. } => {
                let mut seen = HashSet::new();
                for config in configs {
                    if !seen.insert(config.state_id) {
                        continue; // Skip duplicate state IDs
                    }
                    if let Some(state) = nfa.get_state(config.state_id) {
                        if let Some(NfaTerm::Element {
                            name,
                            namespace,
                            element_key,
                            ..
                        }) = &state.term
                        {
                            result.push((*name, *namespace, *element_key));
                        }
                    }
                }
            }
            ActiveStates::RangedSingle { state_ranges, .. } => {
                for &state_id in state_ranges.keys() {
                    if let Some(state) = nfa.get_state(state_id) {
                        if let Some(NfaTerm::Element {
                            name,
                            namespace,
                            element_key,
                            ..
                        }) = &state.term
                        {
                            result.push((*name, *namespace, *element_key));
                        }
                    }
                }
            }
            ActiveStates::Hybrid { configs, .. } => {
                let mut seen = HashSet::new();
                for key in configs.keys() {
                    if !seen.insert(key.state_id) {
                        continue;
                    }
                    if let Some(state) = nfa.get_state(key.state_id) {
                        if let Some(NfaTerm::Element {
                            name,
                            namespace,
                            element_key,
                            ..
                        }) = &state.term
                        {
                            result.push((*name, *namespace, *element_key));
                        }
                    }
                }
            }
        }
        result
    }
}

/// Match info returned from term lookup in active states.
#[derive(Debug, Clone, Copy, Default)]
pub struct MatchInfo {
    pub element_key: Option<ElementKey>,
    pub resolved_type: Option<TypeKey>,
    pub process_contents: Option<ProcessContents>,
}

/// Find match info from an iterator of state IDs.
///
/// Uses element-over-wildcard priority: if any Element term matches, its info
/// is returned immediately; a Wildcard match is kept as a fallback and only
/// returned when no Element matches.  This mirrors the priority rule in
/// `advance_with_priority`.
fn find_match_info_in_states(
    nfa: &NfaTable,
    state_ids: impl Iterator<Item = StateId>,
    name: NameId,
    namespace: Option<NameId>,
    target_ns: Option<NameId>,
    subst_groups: Option<&SubstitutionGroupMap>,
    xsd_version: XsdVersion,
) -> MatchInfo {
    let mut wildcard_info: Option<MatchInfo> = None;

    for state_id in state_ids {
        if let Some(state) = nfa.get_state(state_id) {
            if let Some(ref term) = state.term {
                if term_matches(term, name, namespace, target_ns, subst_groups, xsd_version) {
                    match term {
                        NfaTerm::Element {
                            name: term_name,
                            namespace: term_ns,
                            element_key,
                            resolved_type,
                        } => {
                            // Element match — highest priority, return immediately
                            return if *term_name == name && *term_ns == namespace {
                                // Direct match — return term's declaration info
                                MatchInfo {
                                    element_key: *element_key,
                                    resolved_type: *resolved_type,
                                    process_contents: None,
                                }
                            } else {
                                // Substitution match — let runtime resolve the actual member
                                MatchInfo {
                                    element_key: None,
                                    resolved_type: None,
                                    process_contents: None,
                                }
                            };
                        }
                        NfaTerm::Wildcard {
                            process_contents, ..
                        } => {
                            // Keep first wildcard as fallback
                            if wildcard_info.is_none() {
                                wildcard_info = Some(MatchInfo {
                                    element_key: None,
                                    resolved_type: None,
                                    process_contents: Some(*process_contents),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    wildcard_info.unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    use crate::compiler::build_substitution_group_map;
    use crate::schema::model::{DerivationSet, SchemaSet};

    pub(super) fn element_data(name: NameId) -> crate::arenas::ElementDeclData {
        crate::arenas::ElementDeclData {
            name: Some(name),
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
        }
    }

    fn make_set(ids: &[StateId]) -> StateSet {
        let mut set = StateSet::new();
        for &id in ids {
            set.insert(id);
        }
        set
    }

    #[test]
    fn test_state_set_basic_ops() {
        let mut s = StateSet::new();
        assert!(s.is_empty());
        assert!(s.insert(3)); // newly set
        assert!(!s.insert(3)); // already present
        assert!(s.insert(70)); // crosses a 64-bit word boundary
        assert!(s.contains(3) && s.contains(70));
        assert!(!s.contains(4));
        assert!(!s.is_empty());
        // Iteration is ascending by StateId.
        assert_eq!(s.iter().collect::<Vec<_>>(), vec![3, 70]);
    }

    #[test]
    fn test_state_set_eq_across_inline_and_heap() {
        // An inline literal vs a pre-sized Heap holding the same bits must compare
        // equal despite different word counts / representations.
        let inline = make_set(&[1, 200]);
        let mut heap = StateSet::with_capacity_for(5000); // forces Heap
        heap.insert(1);
        heap.insert(200);
        assert!(matches!(heap, StateSet::Heap(_)));
        assert!(matches!(inline, StateSet::Inline(_)));
        assert_eq!(inline, heap);
        heap.insert(4096);
        assert_ne!(inline, heap);
    }

    #[test]
    fn test_state_set_spill_promotes_to_heap() {
        // Inserting an id beyond the inline range promotes to Heap and preserves
        // previously-set bits.
        let mut s = StateSet::new();
        s.insert(5);
        assert!(matches!(s, StateSet::Inline(_)));
        s.insert(1000); // beyond 256 inline bits
        assert!(matches!(s, StateSet::Heap(_)));
        assert_eq!(s.iter().collect::<Vec<_>>(), vec![5, 1000]);
        assert_eq!(s, make_set(&[5, 1000]));
    }

    #[test]
    fn test_nfa_state_creation() {
        let state = NfaState::epsilon(0, None);
        assert_eq!(state.id, 0);
        assert!(state.is_epsilon());
        assert!(state.transitions.is_empty());
    }

    #[test]
    fn test_nfa_state_with_term() {
        let term = NfaTerm::element(NameId(1), None, None);
        let state = NfaState::with_term(1, term, None);
        assert_eq!(state.id, 1);
        assert!(!state.is_epsilon());
    }

    #[test]
    fn test_nfa_state_transitions() {
        let mut state = NfaState::epsilon(0, None);
        state.add_epsilon(1);
        state.add_consume(2);

        let epsilons: Vec<_> = state.epsilon_transitions().collect();
        assert_eq!(epsilons, vec![1]);

        let consuming: Vec<_> = state.consuming_transitions().collect();
        assert_eq!(consuming, vec![2]);
    }

    #[test]
    fn test_nfa_table() {
        let states = vec![
            NfaState::epsilon(0, None),
            NfaState::with_term(1, NfaTerm::element(NameId(1), None, None), None),
            NfaState::epsilon(2, None),
        ];
        let table = NfaTable::new(states, 0, 2);

        assert_eq!(table.state_count(), 3);
        assert_eq!(table.start_state, 0);
        assert_eq!(table.accept_state, 2);
        assert!(table.is_accept(2));
        assert!(!table.is_accept(0));
    }

    #[test]
    fn test_nfa_term() {
        let elem = NfaTerm::element(NameId(1), Some(NameId(2)), None);
        assert!(elem.is_element());
        assert!(!elem.is_wildcard());

        let wild = NfaTerm::wildcard(NamespaceConstraint::Any, ProcessContents::Lax);
        assert!(!wild.is_element());
        assert!(wild.is_wildcard());
    }

    #[test]
    fn test_transition_kinds() {
        let eps = NfaTransition::epsilon(1);
        assert_eq!(eps.kind, TransitionKind::Epsilon);

        let cons = NfaTransition::consume(2);
        assert_eq!(cons.kind, TransitionKind::Consume);
    }

    #[test]
    fn test_epsilon_closure_basic() {
        let mut s0 = NfaState::epsilon(0, None);
        s0.add_epsilon(1);
        let mut s1 = NfaState::epsilon(1, None);
        s1.add_epsilon(2);
        let s2 = NfaState::with_term(2, NfaTerm::element(NameId(1), None, None), None);

        let nfa = NfaTable::new(vec![s0, s1, s2], 0, 2);
        let closure = epsilon_closure(&nfa, [0]);

        assert_eq!(closure, make_set(&[0, 1, 2]));
    }

    fn make_priority_nfa() -> NfaTable {
        let mut start = NfaState::epsilon(0, None);
        start.add_epsilon(1);
        start.add_epsilon(2);

        let mut elem_state = NfaState::with_term(1, NfaTerm::element(NameId(1), None, None), None);
        let mut wild_state = NfaState::with_term(
            2,
            NfaTerm::wildcard(NamespaceConstraint::Any, ProcessContents::Lax),
            None,
        );

        elem_state.add_consume(3);
        wild_state.add_consume(4);

        let exit_elem = NfaState::epsilon(3, None);
        let exit_wild = NfaState::epsilon(4, None);

        NfaTable::new(
            vec![start, elem_state, wild_state, exit_elem, exit_wild],
            0,
            3,
        )
    }

    #[test]
    fn test_advance_states_matches_element_and_wildcard() {
        let nfa = make_priority_nfa();
        let next = advance_states(&nfa, [0], NameId(1), None, None, None, XsdVersion::V1_0);
        assert_eq!(next, make_set(&[3, 4]));

        let next = advance_states(&nfa, [0], NameId(2), None, None, None, XsdVersion::V1_0);
        assert_eq!(next, make_set(&[4]));
    }

    #[test]
    fn test_advance_with_priority_prefers_element() {
        let nfa = make_priority_nfa();
        let next = advance_with_priority(&nfa, [0], NameId(1), None, None, None, XsdVersion::V1_1);
        assert_eq!(next, make_set(&[3]));

        let next = advance_with_priority(&nfa, [0], NameId(2), None, None, None, XsdVersion::V1_1);
        assert_eq!(next, make_set(&[4]));
    }

    #[test]
    fn test_borrowed_advance_matches_owned() {
        let simple_nfa = make_priority_nfa();
        let simple = ActiveStates::from_nfa(&simple_nfa);
        assert_eq!(
            simple.advance_from(&simple_nfa, NameId(1), None, None, None, XsdVersion::V1_0),
            simple
                .clone()
                .try_advance(&simple_nfa, NameId(1), None, None, None, XsdVersion::V1_0)
        );
        assert_eq!(
            simple.advance_with_priority_from(
                &simple_nfa,
                NameId(1),
                None,
                None,
                None,
                XsdVersion::V1_1,
            ),
            simple.clone().try_advance_with_priority(
                &simple_nfa,
                NameId(1),
                None,
                None,
                None,
                XsdVersion::V1_1,
            )
        );

        let (counted_nfa, counted_name) = make_counted_element_nfa(3, 5);
        let counted = ActiveStates::from_nfa(&counted_nfa);
        assert_eq!(
            counted.advance_from(
                &counted_nfa,
                counted_name,
                None,
                None,
                None,
                XsdVersion::V1_0,
            ),
            counted.clone().try_advance(
                &counted_nfa,
                counted_name,
                None,
                None,
                None,
                XsdVersion::V1_0,
            )
        );
    }

    #[test]
    fn test_find_match_info_prefers_element_over_wildcard() {
        // make_priority_nfa has element(NameId(1)) at state 1 and wildcard at state 2,
        // both reachable from state 0.  Regardless of HashSet iteration order,
        // find_match_info should prefer the element match.
        let nfa = make_priority_nfa();
        let active = ActiveStates::Simple(epsilon_closure(&nfa, [0]));

        // NameId(1) matches both element and wildcard — element should win
        let mi = active.find_match_info(&nfa, NameId(1), None, None, None, XsdVersion::V1_1);
        assert!(
            mi.process_contents.is_none(),
            "element match should not have process_contents, got {:?}",
            mi.process_contents,
        );

        // NameId(2) matches only the wildcard
        let mi2 = active.find_match_info(&nfa, NameId(2), None, None, None, XsdVersion::V1_1);
        assert!(
            mi2.process_contents.is_some(),
            "wildcard-only match should have process_contents",
        );
    }

    #[test]
    fn test_term_matches_substitution_groups() {
        let mut schema_set = SchemaSet::new();
        let head_name = schema_set.name_table.add("head");
        let member_name = schema_set.name_table.add("member");

        let head_key = schema_set.arenas.alloc_element(element_data(head_name));
        let member_key = schema_set.arenas.alloc_element(element_data(member_name));

        schema_set
            .arenas
            .entries_mut()
            .elements
            .get_mut(member_key)
            .unwrap()
            .resolved_substitution_groups
            .push(head_key);

        let map = build_substitution_group_map(&schema_set);
        let term = NfaTerm::element(head_name, None, Some(head_key));

        assert!(term_matches(
            &term,
            member_name,
            None,
            None,
            Some(&map),
            XsdVersion::V1_0,
        ));
    }

    #[test]
    fn test_find_match_info_substitution_returns_none_for_head_key() {
        // When a member matches via substitution group, find_match_info should
        // return element_key: None (not the head's key) so the runtime resolves
        // the actual member declaration via lookup_element.
        let mut schema_set = SchemaSet::new();
        let head_name = schema_set.name_table.add("head");
        let member_name = schema_set.name_table.add("member");

        let mut head_data = element_data(head_name);
        head_data.is_abstract = true;
        let head_key = schema_set.arenas.alloc_element(head_data);
        let member_key = schema_set.arenas.alloc_element(element_data(member_name));

        schema_set
            .arenas
            .entries_mut()
            .elements
            .get_mut(member_key)
            .unwrap()
            .resolved_substitution_groups
            .push(head_key);

        let map = build_substitution_group_map(&schema_set);

        // Build a simple NFA: start --[head]--> accept
        let builder = crate::compiler::fragment::FragmentBuilder::new();
        let frag = builder.single_term(NfaTerm::element(head_name, None, Some(head_key)), None);
        let nfa = crate::compiler::fragment::fragment_to_table(frag);
        let active = ActiveStates::from_nfa(&nfa);

        // Match with member name — should return element_key: None
        let mi =
            active.find_match_info(&nfa, member_name, None, None, Some(&map), XsdVersion::V1_0);
        assert!(
            mi.element_key.is_none(),
            "substitution match should not return head's element_key"
        );
        assert!(mi.resolved_type.is_none());

        // Abstract head's own name doesn't match (excluded from subst map,
        // and subst map lookup short-circuits before direct name comparison)
        let mi_head =
            active.find_match_info(&nfa, head_name, None, None, Some(&map), XsdVersion::V1_0);
        assert!(
            mi_head.element_key.is_none(),
            "abstract head should not match its own name via subst map"
        );
    }

    #[test]
    fn test_find_match_info_direct_match_returns_element_key() {
        // When a non-abstract head matches directly, find_match_info should
        // return the term's element_key.
        let mut schema_set = SchemaSet::new();
        let head_name = schema_set.name_table.add("head");
        let member_name = schema_set.name_table.add("member");

        // Non-abstract head
        let head_key = schema_set.arenas.alloc_element(element_data(head_name));
        let member_key = schema_set.arenas.alloc_element(element_data(member_name));

        schema_set
            .arenas
            .entries_mut()
            .elements
            .get_mut(member_key)
            .unwrap()
            .resolved_substitution_groups
            .push(head_key);

        let map = build_substitution_group_map(&schema_set);

        let builder = crate::compiler::fragment::FragmentBuilder::new();
        let frag = builder.single_term(NfaTerm::element(head_name, None, Some(head_key)), None);
        let nfa = crate::compiler::fragment::fragment_to_table(frag);
        let active = ActiveStates::from_nfa(&nfa);

        // Direct match with head name — should return head's element_key
        let mi = active.find_match_info(&nfa, head_name, None, None, Some(&map), XsdVersion::V1_0);
        assert_eq!(
            mi.element_key,
            Some(head_key),
            "direct match should return the term's element_key"
        );

        // Substitution match with member name — should return None
        let mi_member =
            active.find_match_info(&nfa, member_name, None, None, Some(&map), XsdVersion::V1_0);
        assert!(
            mi_member.element_key.is_none(),
            "substitution match should not return head's element_key"
        );
    }

    // -----------------------------------------------------------------------
    // Counted NFA tests
    // -----------------------------------------------------------------------

    use crate::compiler::fragment::{fragment_to_table, FragmentBuilder};

    /// Build a counted NFA for element `a{min,max}` and return (nfa, name_id).
    fn make_counted_element_nfa(min: u32, max: u32) -> (NfaTable, NameId) {
        let name = NameId(100);
        let builder = FragmentBuilder::new();
        let frag = builder.single_term(NfaTerm::element(name, None, None), None);
        let counted = frag.repeat_counted(min, max);
        let nfa = fragment_to_table(counted);
        (nfa, name)
    }

    fn advance_n(active: ActiveStates, nfa: &NfaTable, name: NameId, n: usize) -> ActiveStates {
        let mut state = active;
        for _ in 0..n {
            state = state.advance(nfa, name, None, None, None, XsdVersion::V1_0);
        }
        state
    }

    #[test]
    fn test_counted_nfa_has_counters() {
        let (nfa, _) = make_counted_element_nfa(2, 5);
        assert!(nfa.has_counters());
        assert_eq!(nfa.counter_defs.len(), 1);
        assert_eq!(nfa.counter_defs[0].min, 2);
        assert_eq!(nfa.counter_defs[0].max, 5);
        // Counted NFA: body(2 states) + entry + guard + exit = 5 states
        assert_eq!(nfa.state_count(), 5);
    }

    #[test]
    fn test_counted_nfa_compact_state_count() {
        // Large maxOccurs should NOT create many states
        let (nfa, _) = make_counted_element_nfa(0, 1000);
        assert_eq!(nfa.state_count(), 5); // Still just 5 states
    }

    #[test]
    fn test_counted_active_states_is_counted_variant() {
        let (nfa, _) = make_counted_element_nfa(2, 5);
        let active = ActiveStates::from_nfa(&nfa);
        assert!(matches!(active, ActiveStates::Counted { .. }));
    }

    #[test]
    fn test_counted_element_a_3_5() {
        let (nfa, a) = make_counted_element_nfa(3, 5);

        // 2 occurrences: not complete, would accept more
        let s2 = advance_n(ActiveStates::from_nfa(&nfa), &nfa, a, 2);
        assert!(!s2.is_empty());
        assert!(!s2.contains_accept(&nfa));

        // 3 occurrences: complete (min satisfied)
        let s3 = advance_n(ActiveStates::from_nfa(&nfa), &nfa, a, 3);
        assert!(s3.contains_accept(&nfa));

        // 4 occurrences: still complete
        let s4 = advance_n(ActiveStates::from_nfa(&nfa), &nfa, a, 4);
        assert!(s4.contains_accept(&nfa));

        // 5 occurrences: complete (max)
        let s5 = advance_n(ActiveStates::from_nfa(&nfa), &nfa, a, 5);
        assert!(s5.contains_accept(&nfa));

        // 6 occurrences: rejected (past max)
        let s6 = advance_n(ActiveStates::from_nfa(&nfa), &nfa, a, 6);
        assert!(s6.is_empty());
    }

    #[test]
    fn test_counted_element_a_0_100() {
        let (nfa, a) = make_counted_element_nfa(0, 100);

        // 0 occurrences: complete (min=0)
        let s0 = ActiveStates::from_nfa(&nfa);
        assert!(s0.contains_accept(&nfa));

        // 100 occurrences: complete (max)
        let s100 = advance_n(ActiveStates::from_nfa(&nfa), &nfa, a, 100);
        assert!(s100.contains_accept(&nfa));

        // 101 occurrences: rejected
        let s101 = advance_n(ActiveStates::from_nfa(&nfa), &nfa, a, 101);
        assert!(s101.is_empty());
    }

    #[test]
    fn test_counted_element_exact_17() {
        let (nfa, a) = make_counted_element_nfa(17, 17);

        // 16: not complete
        let s16 = advance_n(ActiveStates::from_nfa(&nfa), &nfa, a, 16);
        assert!(!s16.contains_accept(&nfa));

        // 17: complete
        let s17 = advance_n(ActiveStates::from_nfa(&nfa), &nfa, a, 17);
        assert!(s17.contains_accept(&nfa));

        // 18: rejected
        let s18 = advance_n(ActiveStates::from_nfa(&nfa), &nfa, a, 18);
        assert!(s18.is_empty());
    }

    #[test]
    fn test_counted_sequence_a_b() {
        // Build a{2,50} followed by b
        let a = NameId(100);
        let b = NameId(200);
        let builder = FragmentBuilder::new();

        let frag_a = builder.single_term(NfaTerm::element(a, None, None), None);
        let counted_a = frag_a.repeat_counted(2, 50);

        let frag_b = builder.single_term(NfaTerm::element(b, None, None), None);
        let seq = counted_a.concat(frag_b);
        let nfa = fragment_to_table(seq);

        assert!(nfa.has_counters());

        // 1 a + b: should fail (min=2 not satisfied)
        let s = ActiveStates::from_nfa(&nfa);
        let s = s.advance(&nfa, a, None, None, None, XsdVersion::V1_0); // 1 a
        let s = s.advance(&nfa, b, None, None, None, XsdVersion::V1_0); // b
        assert!(!s.contains_accept(&nfa)); // min not satisfied

        // 2 a + b: should succeed
        let s = ActiveStates::from_nfa(&nfa);
        let s = advance_n(s, &nfa, a, 2);
        let s = s.advance(&nfa, b, None, None, None, XsdVersion::V1_0);
        assert!(s.contains_accept(&nfa));

        // 50 a + b: should succeed
        let s = ActiveStates::from_nfa(&nfa);
        let s = advance_n(s, &nfa, a, 50);
        let s = s.advance(&nfa, b, None, None, None, XsdVersion::V1_0);
        assert!(s.contains_accept(&nfa));

        // 51 a: should be rejected
        let s = ActiveStates::from_nfa(&nfa);
        let s = advance_n(s, &nfa, a, 51);
        assert!(s.is_empty());
    }

    #[test]
    fn test_dead_counter_collapse() {
        // (a?){0,200} followed by b
        // After the nullable loop exits, all configs should collapse at b.
        let a = NameId(100);
        let b = NameId(200);
        let builder = FragmentBuilder::new();

        // Build a? (optional element)
        let frag_a = builder.single_term(NfaTerm::element(a, None, None), None);
        let opt_a = frag_a.optional();

        // Counted loop: (a?){0,200}
        let counted = opt_a.repeat_counted(0, 200);

        // Followed by b
        let frag_b = builder.single_term(NfaTerm::element(b, None, None), None);
        let seq = counted.concat(frag_b);
        let nfa = fragment_to_table(seq);

        // Single counter + nullable body → should use RangedSingle
        let initial = ActiveStates::from_nfa(&nfa);
        assert!(
            matches!(&initial, ActiveStates::RangedSingle { .. }),
            "Expected RangedSingle for nullable body counted NFA"
        );

        // Initial state should be complete (min=0 loop can be skipped)
        // After advancing with b (skipping the loop entirely), should accept
        let after_b = initial
            .clone()
            .advance(&nfa, b, None, None, None, XsdVersion::V1_0);
        assert!(after_b.contains_accept(&nfa));

        // With RangedSingle, the state map has O(states) entries, not O(200).
        if let ActiveStates::RangedSingle { state_ranges, .. } = &after_b {
            assert!(
                state_ranges.len() <= 5,
                "Expected O(1) ranged entries after loop exit, got {}",
                state_ranges.len()
            );
        }
    }

    #[test]
    fn test_counted_exact_prefix_plus_star() {
        // a{17,unbounded}: counted exact(17) + star
        // Built via apply_occurs to test the dispatch
        use crate::compiler::particle::{apply_occurs, MaxOccurs};

        let a = NameId(100);
        let builder = FragmentBuilder::new();
        let frag = builder.single_term(NfaTerm::element(a, None, None), None);
        let result = apply_occurs(frag, 17, MaxOccurs::Unbounded);
        let nfa = fragment_to_table(result);

        // Should use counted path (has counters for the prefix)
        assert!(nfa.has_counters());
        // Should be compact — NOT 17*2 = 34 states from unrolling
        assert!(
            nfa.state_count() < 15,
            "expected compact NFA, got {} states",
            nfa.state_count()
        );

        // 16 occurrences: not complete (min=17)
        let s16 = advance_n(ActiveStates::from_nfa(&nfa), &nfa, a, 16);
        assert!(!s16.contains_accept(&nfa));

        // 17 occurrences: complete
        let s17 = advance_n(ActiveStates::from_nfa(&nfa), &nfa, a, 17);
        assert!(s17.contains_accept(&nfa));

        // 100 occurrences: still complete (unbounded)
        let s100 = advance_n(ActiveStates::from_nfa(&nfa), &nfa, a, 100);
        assert!(s100.contains_accept(&nfa));
    }

    #[test]
    fn test_is_epsilon_like() {
        assert!(TransitionKind::Epsilon.is_epsilon_like());
        assert!(!TransitionKind::Consume.is_epsilon_like());
        assert!(TransitionKind::CounterReset(0).is_epsilon_like());
        assert!(TransitionKind::CounterIncrement(0).is_epsilon_like());
        assert!(TransitionKind::CounterMaxGuard(0).is_epsilon_like());
        assert!(TransitionKind::CounterMinGuard(0).is_epsilon_like());
    }

    #[test]
    fn test_offset_counter() {
        assert_eq!(
            TransitionKind::CounterReset(0).offset_counter(3),
            TransitionKind::CounterReset(3)
        );
        assert_eq!(
            TransitionKind::CounterIncrement(2).offset_counter(5),
            TransitionKind::CounterIncrement(7)
        );
        assert_eq!(
            TransitionKind::Epsilon.offset_counter(10),
            TransitionKind::Epsilon
        );
        assert_eq!(
            TransitionKind::Consume.offset_counter(10),
            TransitionKind::Consume
        );
    }

    // -----------------------------------------------------------------------
    // CounterRange unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_counter_range_operations() {
        let r = CounterRange::single(5);
        assert_eq!(r.lo, 5);
        assert_eq!(r.hi, 5);
        assert!(!r.is_empty());
        assert!(r.subsumes(r));

        let r2 = CounterRange::new(3, 7);
        assert!(r2.subsumes(r));
        assert!(!r.subsumes(r2));

        // Union
        let u = CounterRange::single(2).union(CounterRange::new(3, 5));
        assert_eq!(u, CounterRange::new(2, 5));

        // intersect_below
        assert_eq!(
            CounterRange::new(1, 5).intersect_below(4),
            CounterRange::new(1, 3)
        );
        assert!(CounterRange::new(5, 8).intersect_below(5).is_empty());
        assert!(CounterRange::new(0, 10).intersect_below(0).is_empty());

        // intersect_above
        assert_eq!(
            CounterRange::new(1, 5).intersect_above(3),
            CounterRange::new(3, 5)
        );
        assert!(CounterRange::new(1, 3).intersect_above(5).is_empty());
    }

    // -----------------------------------------------------------------------
    // RangedSingle tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_ranged_closure_convergence() {
        // (a?){0,10000} — must use RangedSingle and converge in O(states)
        let a = NameId(100);
        let builder = FragmentBuilder::new();
        let frag = builder.single_term(NfaTerm::element(a, None, None), None);
        let opt = frag.optional();
        let counted = opt.repeat_counted(0, 10_000);
        let nfa = fragment_to_table(counted);

        assert!(nfa.has_counters());
        assert!(nfa.counter_defs[0].body_nullable);

        let initial = ActiveStates::from_nfa(&nfa);
        assert!(matches!(&initial, ActiveStates::RangedSingle { .. }));

        // Should have O(states) entries, not O(10000)
        if let ActiveStates::RangedSingle { state_ranges, .. } = &initial {
            assert!(
                state_ranges.len() <= 10,
                "Expected O(states) entries, got {}",
                state_ranges.len()
            );
        }

        // Should be accepting (min=0)
        assert!(initial.contains_accept(&nfa));

        // Advance with 'a' 5 times — should still be accepting
        let s5 = advance_n(initial, &nfa, a, 5);
        assert!(s5.contains_accept(&nfa));

        // Still RangedSingle throughout
        assert!(matches!(&s5, ActiveStates::RangedSingle { .. }));
    }

    #[test]
    fn test_nullable_body_accepts_range() {
        // (a?){3,5}: accepts "", "a", "aa", "aaa", "aaaa", "aaaaa", rejects "aaaaaa"
        let a = NameId(100);
        let builder = FragmentBuilder::new();
        let frag = builder.single_term(NfaTerm::element(a, None, None), None);
        let opt = frag.optional();
        let counted = opt.repeat_counted(3, 5);
        let nfa = fragment_to_table(counted);

        assert!(nfa.counter_defs[0].body_nullable);
        let initial = ActiveStates::from_nfa(&nfa);
        assert!(matches!(&initial, ActiveStates::RangedSingle { .. }));

        // "" — accepted (body nullable, 3 empty iterations satisfy min)
        assert!(initial.contains_accept(&nfa));

        // "a" through "aaaaa" — all accepted
        for n in 1..=5 {
            let s = advance_n(initial.clone(), &nfa, a, n);
            assert!(s.contains_accept(&nfa), "Expected accepting after {n} a's");
        }

        // "aaaaaa" — rejected (max=5 iterations, each consuming at most 1 a)
        let s6 = advance_n(initial, &nfa, a, 6);
        assert!(s6.is_empty(), "Expected rejection after 6 a's");
    }

    #[test]
    fn test_choice_body_nullable() {
        // (a|b?){0,1000} — body is nullable (b? branch), should use RangedSingle
        let a = NameId(100);
        let b = NameId(200);
        let builder = FragmentBuilder::new();
        let frag_a = builder.single_term(NfaTerm::element(a, None, None), None);
        let frag_b = builder.single_term(NfaTerm::element(b, None, None), None);
        let choice = frag_a.alternate(frag_b.optional());
        let counted = choice.repeat_counted(0, 1000);
        let nfa = fragment_to_table(counted);

        assert!(nfa.counter_defs[0].body_nullable);
        let initial = ActiveStates::from_nfa(&nfa);
        assert!(matches!(&initial, ActiveStates::RangedSingle { .. }));

        if let ActiveStates::RangedSingle { state_ranges, .. } = &initial {
            assert!(
                state_ranges.len() <= 15,
                "Expected O(states) entries, got {}",
                state_ranges.len()
            );
        }

        // Accepts "a", "b", "ab", ""
        assert!(initial.contains_accept(&nfa));
        let s_a = initial
            .clone()
            .advance(&nfa, a, None, None, None, XsdVersion::V1_0);
        assert!(s_a.contains_accept(&nfa));
        let s_b = initial
            .clone()
            .advance(&nfa, b, None, None, None, XsdVersion::V1_0);
        assert!(s_b.contains_accept(&nfa));
    }

    #[test]
    fn test_non_nullable_body_stays_counted() {
        // (a){0,5} — body is NOT nullable, should use Counted (not RangedSingle)
        let (nfa, a) = make_counted_element_nfa(0, 5);
        assert!(!nfa.counter_defs[0].body_nullable);

        let initial = ActiveStates::from_nfa(&nfa);
        assert!(matches!(&initial, ActiveStates::Counted { .. }));

        // Still correct: accepts 0..5 a's
        assert!(initial.contains_accept(&nfa)); // min=0
        for n in 1..=5 {
            let s = advance_n(initial.clone(), &nfa, a, n);
            assert!(s.contains_accept(&nfa), "Expected accepting after {n} a's");
        }
        let s6 = advance_n(initial, &nfa, a, 6);
        assert!(s6.is_empty());
    }

    #[test]
    fn test_multi_counter_stays_counted() {
        // a{0,100} followed by b{0,100} — 2 counters, should use Counted
        let a = NameId(100);
        let b = NameId(200);
        let builder = FragmentBuilder::new();
        let frag_a = builder.single_term(NfaTerm::element(a, None, None), None);
        let counted_a = frag_a.repeat_counted(0, 100);
        let frag_b = builder.single_term(NfaTerm::element(b, None, None), None);
        let counted_b = frag_b.repeat_counted(0, 100);
        let seq = counted_a.concat(counted_b);
        let nfa = fragment_to_table(seq);

        assert_eq!(nfa.counter_defs.len(), 2);
        let initial = ActiveStates::from_nfa(&nfa);
        assert!(
            matches!(&initial, ActiveStates::Counted { .. }),
            "Multi-counter NFA should use Counted, not RangedSingle"
        );
    }

    #[test]
    fn test_nested_nullable_uses_hybrid() {
        // ((a?){0,50}){0,50} — 2 nullable counters → Hybrid
        let a = NameId(100);
        let builder = FragmentBuilder::new();
        let frag = builder.single_term(NfaTerm::element(a, None, None), None);
        let inner = frag.optional().repeat_counted(0, 50);
        let outer = inner.repeat_counted(0, 50);
        let nfa = fragment_to_table(outer);

        assert_eq!(nfa.counter_defs.len(), 2);
        let initial = ActiveStates::from_nfa(&nfa);
        assert!(
            matches!(&initial, ActiveStates::Hybrid { .. }),
            "Nested nullable counted NFA should use Hybrid, got {:?}",
            std::mem::discriminant(&initial)
        );
    }

    // -----------------------------------------------------------------------
    // Hybrid variant tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_nested_nullable_hybrid_correctness() {
        // ((a?){0,100}){0,50}: inner up to 100 per outer iteration, outer up to 50
        let a = NameId(100);
        let builder = FragmentBuilder::new();
        let frag = builder.single_term(NfaTerm::element(a, None, None), None);
        let inner = frag.optional().repeat_counted(0, 100);
        let outer = inner.repeat_counted(0, 50);
        let nfa = fragment_to_table(outer);

        assert_eq!(nfa.counter_defs.len(), 2);
        let initial = ActiveStates::from_nfa(&nfa);
        assert!(matches!(&initial, ActiveStates::Hybrid { .. }));

        // Empty accepted (min=0 on both loops)
        assert!(initial.contains_accept(&nfa));

        // "a"*100: accepted (1 outer iteration, 100 inner = max inner)
        let s100 = advance_n(initial.clone(), &nfa, a, 100);
        assert!(s100.contains_accept(&nfa));

        // "a"*5000: accepted (50 outer * 100 inner)
        let s5000 = advance_n(initial.clone(), &nfa, a, 5000);
        assert!(s5000.contains_accept(&nfa));

        // "a"*5001: rejected (exceeds 50*100)
        let s5001 = advance_n(initial, &nfa, a, 5001);
        assert!(s5001.is_empty(), "Expected rejection after 5001 a's");
    }

    #[test]
    fn test_hybrid_convergence() {
        // ((a?){0,100}){0,50}: verify configs are O(states * outer_max), not O(M*N*states)
        let a = NameId(100);
        let builder = FragmentBuilder::new();
        let frag = builder.single_term(NfaTerm::element(a, None, None), None);
        let inner = frag.optional().repeat_counted(0, 100);
        let outer = inner.repeat_counted(0, 50);
        let nfa = fragment_to_table(outer);

        let initial = ActiveStates::from_nfa(&nfa);
        if let ActiveStates::Hybrid { configs, .. } = &initial {
            // With Hybrid, inner counter is ranged → O(states * outer_max) entries.
            // Without Hybrid (Counted), would be O(states * 100 * 50) = O(5000 * states).
            let max_expected = nfa.state_count() * 51; // outer goes 0..50
            assert!(
                configs.len() <= max_expected,
                "Expected at most {} entries, got {} (state_count={})",
                max_expected,
                configs.len(),
                nfa.state_count()
            );
        } else {
            panic!("Expected Hybrid variant");
        }
    }

    #[test]
    fn test_sequential_nullable_hybrid() {
        // (a?){0,1000} followed by (b?){0,1000}
        let a = NameId(100);
        let b = NameId(200);
        let builder = FragmentBuilder::new();
        let frag_a = builder.single_term(NfaTerm::element(a, None, None), None);
        let counted_a = frag_a.optional().repeat_counted(0, 1000);
        let frag_b = builder.single_term(NfaTerm::element(b, None, None), None);
        let counted_b = frag_b.optional().repeat_counted(0, 1000);
        let seq = counted_a.concat(counted_b);
        let nfa = fragment_to_table(seq);

        assert_eq!(nfa.counter_defs.len(), 2);
        assert!(nfa.counter_defs[0].body_nullable);
        assert!(nfa.counter_defs[1].body_nullable);

        let initial = ActiveStates::from_nfa(&nfa);
        assert!(matches!(&initial, ActiveStates::Hybrid { .. }));

        // Empty: accepted (both min=0)
        assert!(initial.contains_accept(&nfa));

        // "a": accepted
        let s = initial
            .clone()
            .advance(&nfa, a, None, None, None, XsdVersion::V1_0);
        assert!(s.contains_accept(&nfa));

        // "b": accepted
        let s = initial
            .clone()
            .advance(&nfa, b, None, None, None, XsdVersion::V1_0);
        assert!(s.contains_accept(&nfa));

        // "a"*1000 + "b"*1000: accepted
        let s = advance_n(initial.clone(), &nfa, a, 1000);
        assert!(s.contains_accept(&nfa));
        let s = advance_n(s, &nfa, b, 1000);
        assert!(s.contains_accept(&nfa));

        // "a"*1001: rejected (exceeds first loop's max)
        let s = advance_n(initial, &nfa, a, 1001);
        assert!(s.is_empty(), "Expected rejection after 1001 a's");
    }

    #[test]
    fn test_mixed_nullable_nonnullable_hybrid() {
        // (a?){0,500} followed by b{0,500}: one nullable, one not
        let a = NameId(100);
        let b = NameId(200);
        let builder = FragmentBuilder::new();
        let frag_a = builder.single_term(NfaTerm::element(a, None, None), None);
        let counted_a = frag_a.optional().repeat_counted(0, 500);
        let frag_b = builder.single_term(NfaTerm::element(b, None, None), None);
        let counted_b = frag_b.repeat_counted(0, 500);
        let seq = counted_a.concat(counted_b);
        let nfa = fragment_to_table(seq);

        assert_eq!(nfa.counter_defs.len(), 2);
        assert!(nfa.counter_defs[0].body_nullable);
        assert!(!nfa.counter_defs[1].body_nullable);

        let initial = ActiveStates::from_nfa(&nfa);
        assert!(matches!(&initial, ActiveStates::Hybrid { .. }));

        // Verify ranged counter is counter 0 (the nullable one)
        if let ActiveStates::Hybrid {
            ranged_counter_idx, ..
        } = &initial
        {
            assert_eq!(
                *ranged_counter_idx, 0,
                "Should range the nullable counter (index 0)"
            );
        }

        // Empty: accepted (both min=0)
        assert!(initial.contains_accept(&nfa));

        // "a"*500 + "b"*500: accepted
        let s = advance_n(initial.clone(), &nfa, a, 500);
        let s = advance_n(s, &nfa, b, 500);
        assert!(s.contains_accept(&nfa));

        // "b"*501: rejected
        let s = advance_n(initial, &nfa, b, 501);
        assert!(s.is_empty());
    }

    #[test]
    fn test_hybrid_forced_outer_ranged() {
        // ((a?){0,100}){0,50}: manually verify correctness with the algorithm
        // choosing whichever counter it picks, by testing boundary conditions
        // that exercise both counters.
        let a = NameId(100);
        let builder = FragmentBuilder::new();
        let frag = builder.single_term(NfaTerm::element(a, None, None), None);
        let inner = frag.optional().repeat_counted(0, 100);
        let outer = inner.repeat_counted(0, 50);
        let nfa = fragment_to_table(outer);

        let initial = ActiveStates::from_nfa(&nfa);

        // Boundary: exactly 100 a's per iteration, 50 iterations = 5000 total
        let s = advance_n(initial.clone(), &nfa, a, 5000);
        assert!(
            s.contains_accept(&nfa),
            "5000 a's should be accepted (50*100)"
        );

        // Boundary: 101 a's — requires second outer iteration (first handles 100, second 1)
        let s = advance_n(initial.clone(), &nfa, a, 101);
        assert!(
            s.contains_accept(&nfa),
            "101 a's should be accepted (needs 2 outer iterations)"
        );

        // Boundary: 0 a's
        assert!(initial.contains_accept(&nfa), "0 a's should be accepted");

        // Boundary: 5001 — exceeds total capacity
        let s = advance_n(initial, &nfa, a, 5001);
        assert!(s.is_empty(), "5001 a's should be rejected");
    }

    #[test]
    fn test_hybrid_selection_different_maxima() {
        // (a?){0,10} followed by (b?){0,1000}: should range counter 1 (max=1000)
        let a = NameId(100);
        let b = NameId(200);
        let builder = FragmentBuilder::new();
        let frag_a = builder.single_term(NfaTerm::element(a, None, None), None);
        let counted_a = frag_a.optional().repeat_counted(0, 10);
        let frag_b = builder.single_term(NfaTerm::element(b, None, None), None);
        let counted_b = frag_b.optional().repeat_counted(0, 1000);
        let seq = counted_a.concat(counted_b);
        let nfa = fragment_to_table(seq);

        assert_eq!(nfa.counter_defs.len(), 2);
        let initial = ActiveStates::from_nfa(&nfa);
        // counter 0 max=10 (below COUNTED_THRESHOLD=16 — but counter 1 max=1000 qualifies)
        assert!(
            matches!(&initial, ActiveStates::Hybrid { .. }),
            "Expected Hybrid, got {:?}",
            std::mem::discriminant(&initial)
        );

        if let ActiveStates::Hybrid {
            ranged_counter_idx, ..
        } = &initial
        {
            assert_eq!(
                *ranged_counter_idx, 1,
                "Should range counter 1 (max=1000), not counter 0 (max=10)"
            );
        }

        // Correctness: "a"*10 + "b"*1000 accepted
        let s = advance_n(initial.clone(), &nfa, a, 10);
        let s = advance_n(s, &nfa, b, 1000);
        assert!(s.contains_accept(&nfa));

        // "a"*11 rejected
        let s = advance_n(initial, &nfa, a, 11);
        assert!(s.is_empty());
    }

    #[test]
    fn test_hybrid_small_max_stays_counted() {
        // (a?){0,5} followed by (b?){0,5}: both nullable but max ≤ COUNTED_THRESHOLD → Counted
        let a = NameId(100);
        let b = NameId(200);
        let builder = FragmentBuilder::new();
        let frag_a = builder.single_term(NfaTerm::element(a, None, None), None);
        let counted_a = frag_a.optional().repeat_counted(0, 5);
        let frag_b = builder.single_term(NfaTerm::element(b, None, None), None);
        let counted_b = frag_b.optional().repeat_counted(0, 5);
        let seq = counted_a.concat(counted_b);
        let nfa = fragment_to_table(seq);

        assert_eq!(nfa.counter_defs.len(), 2);
        let initial = ActiveStates::from_nfa(&nfa);
        assert!(
            matches!(&initial, ActiveStates::Counted { .. }),
            "Small-max nullable counters should stay Counted, got {:?}",
            std::mem::discriminant(&initial)
        );
    }

    #[test]
    fn test_xsd11_priority_ranged() {
        // Nullable counted body containing element 'a' + wildcard *.
        // advance_with_priority should prefer element over wildcard.
        use crate::types::complex::NamespaceConstraint;
        let a = NameId(100);
        let b = NameId(200);
        let builder = FragmentBuilder::new();

        // Element term for 'a' (mandatory)
        let elem = builder.single_term(NfaTerm::element(a, None, None), None);
        // Wildcard term matching anything (optional → makes body nullable)
        let wc = builder.single_term(
            NfaTerm::Wildcard {
                namespace_constraint: NamespaceConstraint::Any,
                process_contents: ProcessContents::Lax,
                not_qnames: Vec::new(),
            },
            None,
        );
        // (a | *?){0,100} — nullable body via the *? branch
        let choice = elem.alternate(wc.optional());
        let counted = choice.repeat_counted(0, 100);
        let nfa = fragment_to_table(counted);

        assert!(nfa.counter_defs[0].body_nullable);
        let initial = ActiveStates::from_nfa(&nfa);
        assert!(matches!(&initial, ActiveStates::RangedSingle { .. }));

        // advance_with_priority for 'a': element branch exists, so wildcard
        // should not be chosen.  The result should be non-empty.
        let next =
            initial
                .clone()
                .advance_with_priority(&nfa, a, None, None, None, XsdVersion::V1_1);
        assert!(!next.is_empty());
        assert!(next.contains_accept(&nfa));

        // advance_with_priority for 'b': only wildcard matches, should work
        let next_b = initial.advance_with_priority(&nfa, b, None, None, None, XsdVersion::V1_1);
        assert!(!next_b.is_empty());
        assert!(next_b.contains_accept(&nfa));
    }

    // -----------------------------------------------------------------------
    // Dynamic ranged-counter switching tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_initial_tiebreak_prefers_first() {
        // (a?){0,1000} followed by (b?){0,1000}: equal max → should range counter 0
        let a = NameId(100);
        let b = NameId(200);
        let builder = FragmentBuilder::new();
        let frag_a = builder.single_term(NfaTerm::element(a, None, None), None);
        let counted_a = frag_a.optional().repeat_counted(0, 1000);
        let frag_b = builder.single_term(NfaTerm::element(b, None, None), None);
        let counted_b = frag_b.optional().repeat_counted(0, 1000);
        let seq = counted_a.concat(counted_b);
        let nfa = fragment_to_table(seq);

        let initial = ActiveStates::from_nfa(&nfa);
        if let ActiveStates::Hybrid {
            ranged_counter_idx, ..
        } = &initial
        {
            assert_eq!(
                *ranged_counter_idx, 0,
                "Equal max: should prefer first counter (index 0)"
            );
        } else {
            panic!("Expected Hybrid variant");
        }
    }

    #[test]
    fn test_dynamic_switch_sequential_equal_max() {
        // (a?){0,1000} followed by (b?){0,1000}
        // After feeding a's and then b, the ranged counter should switch.
        let a = NameId(100);
        let b = NameId(200);
        let builder = FragmentBuilder::new();
        let frag_a = builder.single_term(NfaTerm::element(a, None, None), None);
        let counted_a = frag_a.optional().repeat_counted(0, 1000);
        let frag_b = builder.single_term(NfaTerm::element(b, None, None), None);
        let counted_b = frag_b.optional().repeat_counted(0, 1000);
        let seq = counted_a.concat(counted_b);
        let nfa = fragment_to_table(seq);

        let initial = ActiveStates::from_nfa(&nfa);
        // Feed 1000 a's then 1 b to move fully into second loop
        let after_a = advance_n(initial, &nfa, a, 1000);
        let after_b = after_a.advance(&nfa, b, None, None, None, XsdVersion::V1_0);

        if let ActiveStates::Hybrid {
            ranged_counter_idx,
            configs,
            ..
        } = &after_b
        {
            assert_eq!(
                *ranged_counter_idx, 1,
                "Should have dynamically switched to counter 1"
            );
            // Config count should be O(states), not O(1000 * states)
            let max_expected = nfa.state_count() * 2;
            assert!(
                configs.len() <= max_expected,
                "Expected <= {} configs after switch, got {}",
                max_expected,
                configs.len()
            );
        } else {
            panic!("Expected Hybrid variant");
        }

        // Correctness: b*999 more (total 1000 b) accepted
        let s = advance_n(after_b.clone(), &nfa, b, 999);
        assert!(s.contains_accept(&nfa));

        // b*1001 total rejected
        let s = advance_n(after_b, &nfa, b, 1000);
        assert!(s.is_empty());
    }

    #[test]
    fn test_dynamic_switch_three_counters() {
        // (a?){0,500} (b?){0,500} (c?){0,500}: cascading switches 0→1→2
        let a = NameId(100);
        let b = NameId(200);
        let c = NameId(300);
        let builder = FragmentBuilder::new();
        let frag_a = builder.single_term(NfaTerm::element(a, None, None), None);
        let counted_a = frag_a.optional().repeat_counted(0, 500);
        let frag_b = builder.single_term(NfaTerm::element(b, None, None), None);
        let counted_b = frag_b.optional().repeat_counted(0, 500);
        let frag_c = builder.single_term(NfaTerm::element(c, None, None), None);
        let counted_c = frag_c.optional().repeat_counted(0, 500);
        let seq = counted_a.concat(counted_b).concat(counted_c);
        let nfa = fragment_to_table(seq);

        assert_eq!(nfa.counter_defs.len(), 3);
        let initial = ActiveStates::from_nfa(&nfa);

        // After 500 a's + 1 b → switch to counter 1
        let after_a = advance_n(initial, &nfa, a, 500);
        assert!(after_a.contains_accept(&nfa));
        let after_b1 = after_a.advance(&nfa, b, None, None, None, XsdVersion::V1_0);
        if let ActiveStates::Hybrid {
            ranged_counter_idx, ..
        } = &after_b1
        {
            assert_eq!(
                *ranged_counter_idx, 1,
                "Should switch to counter 1 after first loop exits"
            );
        }

        // After 500 b's + 1 c → switch to counter 2
        let after_b = advance_n(after_b1, &nfa, b, 499);
        assert!(after_b.contains_accept(&nfa));
        let after_c1 = after_b.advance(&nfa, c, None, None, None, XsdVersion::V1_0);
        if let ActiveStates::Hybrid {
            ranged_counter_idx, ..
        } = &after_c1
        {
            assert_eq!(
                *ranged_counter_idx, 2,
                "Should switch to counter 2 after second loop exits"
            );
        }

        // 500 c's accepted, 501 rejected
        let after_c = advance_n(after_c1.clone(), &nfa, c, 499);
        assert!(after_c.contains_accept(&nfa));
        let over = advance_n(after_c1, &nfa, c, 500);
        assert!(over.is_empty());
    }

    #[test]
    fn test_no_switch_when_ranged_still_active() {
        // (a?){0,1000} followed by (b?){0,1000}
        // After 5 a's, ranged counter is still active — no switch.
        let a = NameId(100);
        let b = NameId(200);
        let builder = FragmentBuilder::new();
        let frag_a = builder.single_term(NfaTerm::element(a, None, None), None);
        let counted_a = frag_a.optional().repeat_counted(0, 1000);
        let frag_b = builder.single_term(NfaTerm::element(b, None, None), None);
        let counted_b = frag_b.optional().repeat_counted(0, 1000);
        let seq = counted_a.concat(counted_b);
        let nfa = fragment_to_table(seq);

        let initial = ActiveStates::from_nfa(&nfa);
        let after_5a = advance_n(initial, &nfa, a, 5);
        if let ActiveStates::Hybrid {
            ranged_counter_idx, ..
        } = &after_5a
        {
            assert_eq!(
                *ranged_counter_idx, 0,
                "Should NOT switch while first loop is still active"
            );
        }
    }

    #[test]
    fn test_no_switch_non_nullable_candidate() {
        // (a?){0,1000} followed by b{0,1000}: second counter is NOT nullable
        let a = NameId(100);
        let b = NameId(200);
        let builder = FragmentBuilder::new();
        let frag_a = builder.single_term(NfaTerm::element(a, None, None), None);
        let counted_a = frag_a.optional().repeat_counted(0, 1000);
        let frag_b = builder.single_term(NfaTerm::element(b, None, None), None);
        let counted_b = frag_b.repeat_counted(0, 1000); // NOT nullable
        let seq = counted_a.concat(counted_b);
        let nfa = fragment_to_table(seq);

        let initial = ActiveStates::from_nfa(&nfa);
        // Feed b's to exit first loop (counter 0 dead) and enter second
        let after_b = advance_n(initial, &nfa, b, 100);
        if let ActiveStates::Hybrid {
            ranged_counter_idx, ..
        } = &after_b
        {
            assert_eq!(
                *ranged_counter_idx, 0,
                "Should NOT switch: counter 1 is not nullable"
            );
        }

        // Correctness: b*1000 accepted
        let s = advance_n(after_b.clone(), &nfa, b, 900);
        assert!(s.contains_accept(&nfa));
    }

    #[test]
    fn test_no_switch_nested_inner_active() {
        // ((a?){0,100}){0,50}: inner counter should not cause a switch
        // while it still has active ranges.
        let a = NameId(100);
        let builder = FragmentBuilder::new();
        let frag = builder.single_term(NfaTerm::element(a, None, None), None);
        let inner = frag.optional().repeat_counted(0, 100);
        let outer = inner.repeat_counted(0, 50);
        let nfa = fragment_to_table(outer);

        // During active processing, the ranged counter should not switch
        let initial = ActiveStates::from_nfa(&nfa);
        let after_a = advance_n(initial.clone(), &nfa, a, 50);
        if let ActiveStates::Hybrid {
            ranged_counter_idx, ..
        } = &after_a
        {
            // The initial ranged counter (inner, max=100) should stay
            let initial_idx = if let ActiveStates::Hybrid {
                ranged_counter_idx, ..
            } = &initial
            {
                *ranged_counter_idx
            } else {
                unreachable!()
            };
            assert_eq!(
                *ranged_counter_idx, initial_idx,
                "Should NOT switch while processing nested loops"
            );
        }
    }

    #[test]
    fn test_switch_config_count_drops() {
        // (a?){0,1000} followed by (b?){0,1000}
        // After first 'b', config count should be small.
        let a = NameId(100);
        let b = NameId(200);
        let builder = FragmentBuilder::new();
        let frag_a = builder.single_term(NfaTerm::element(a, None, None), None);
        let counted_a = frag_a.optional().repeat_counted(0, 1000);
        let frag_b = builder.single_term(NfaTerm::element(b, None, None), None);
        let counted_b = frag_b.optional().repeat_counted(0, 1000);
        let seq = counted_a.concat(counted_b);
        let nfa = fragment_to_table(seq);

        let initial = ActiveStates::from_nfa(&nfa);
        // Feed one 'b': exits first loop, enters second loop
        let after_b = initial.advance(&nfa, b, None, None, None, XsdVersion::V1_0);
        if let ActiveStates::Hybrid {
            configs,
            ranged_counter_idx,
            ..
        } = &after_b
        {
            assert_eq!(
                *ranged_counter_idx, 1,
                "Should switch to counter 1 after 'b'"
            );
            // After switch, configs should be compact: O(states) not O(1000*states)
            assert!(
                configs.len() < nfa.state_count() * 2,
                "Config count {} should be << state_count {} after switch",
                configs.len(),
                nfa.state_count()
            );
        }
    }

    #[test]
    fn test_differential_counted_vs_hybrid_switch() {
        // (a?){0,100} followed by (b?){0,100}
        // Build the same NFA, force one to Counted and the other to Hybrid.
        // Compare accept/reject decisions at every step.
        let a = NameId(100);
        let b = NameId(200);
        let builder = FragmentBuilder::new();
        let frag_a = builder.single_term(NfaTerm::element(a, None, None), None);
        let counted_a = frag_a.optional().repeat_counted(0, 100);
        let frag_b = builder.single_term(NfaTerm::element(b, None, None), None);
        let counted_b = frag_b.optional().repeat_counted(0, 100);
        let seq = counted_a.concat(counted_b);
        let nfa = fragment_to_table(seq);

        // Hybrid path (normal)
        let hybrid = ActiveStates::from_nfa(&nfa);

        // Force Counted path
        let initial_config = ActiveConfig::initial(nfa.start_state, nfa.counter_defs.len());
        let mut counted_configs = HashSet::new();
        counted_configs.insert(initial_config);
        let counted = ActiveStates::Counted {
            configs: counted_configs,
            num_counters: nfa.counter_defs.len(),
        }
        .epsilon_closure(&nfa);

        // Compare: feed a*50, then b*50, checking at each step
        let mut h = hybrid;
        let mut c = counted;
        for _ in 0..50 {
            h = h.advance(&nfa, a, None, None, None, XsdVersion::V1_0);
            c = c.advance(&nfa, a, None, None, None, XsdVersion::V1_0);
            assert_eq!(
                h.contains_accept(&nfa),
                c.contains_accept(&nfa),
                "Hybrid/Counted disagree on accept during a-feeding"
            );
            assert_eq!(
                h.is_empty(),
                c.is_empty(),
                "Hybrid/Counted disagree on empty during a-feeding"
            );
        }
        for _ in 0..50 {
            h = h.advance(&nfa, b, None, None, None, XsdVersion::V1_0);
            c = c.advance(&nfa, b, None, None, None, XsdVersion::V1_0);
            assert_eq!(
                h.contains_accept(&nfa),
                c.contains_accept(&nfa),
                "Hybrid/Counted disagree on accept during b-feeding"
            );
            assert_eq!(
                h.is_empty(),
                c.is_empty(),
                "Hybrid/Counted disagree on empty during b-feeding"
            );
        }
        // Both should accept after a*50 + b*50
        assert!(h.contains_accept(&nfa));
        assert!(c.contains_accept(&nfa));

        // Feed one more b — still accepted (total b=51 ≤ 100)
        h = h.advance(&nfa, b, None, None, None, XsdVersion::V1_0);
        c = c.advance(&nfa, b, None, None, None, XsdVersion::V1_0);
        assert_eq!(h.contains_accept(&nfa), c.contains_accept(&nfa));
    }
}

/// P1 (XSD_COMPILER_REWORK §8): exact finite bounds at and beyond the former
/// 10 000 cutoff, boundary arithmetic at `u32::MAX`, and the execution limits.
#[cfg(test)]
mod exact_bounds_tests {
    use super::*;
    use crate::compiler::fragment::{fragment_to_table, FragmentBuilder};
    use crate::schema::model::XsdVersion;

    const A: NameId = NameId(100);

    fn counted_a(min: u32, max: u32) -> NfaTable {
        let builder = FragmentBuilder::new();
        let frag = builder.single_term(NfaTerm::element(A, None, None), None);
        fragment_to_table(frag.repeat_counted(min, max))
    }

    /// `((a?){0,n}){0,n}` — the nested nullable shape whose configuration set
    /// grows with the bound.
    fn nested_nullable(n: u32) -> NfaTable {
        let builder = FragmentBuilder::new();
        let frag = builder.single_term(NfaTerm::element(A, None, None), None);
        let inner = frag.optional().repeat_counted(0, n);
        fragment_to_table(inner.repeat_counted(0, n))
    }

    fn step(active: ActiveStates, nfa: &NfaTable) -> ActiveStates {
        active
            .try_advance(nfa, A, None, None, None, XsdVersion::V1_0)
            .expect("no execution limit expected")
    }

    /// The reproduced defect: `a{0,10001}` must accept exactly 10 001 and
    /// reject the 10 002nd child. Under the old cutoff the bound was widened
    /// to unbounded and the extra child was accepted.
    #[test]
    fn bound_10001_is_exact() {
        let nfa = counted_a(0, 10_001);
        assert!(
            nfa.has_counters(),
            "bounds above the unroll threshold are counted"
        );
        let mut active = ActiveStates::try_from_nfa(&nfa).unwrap();
        for _ in 0..10_001 {
            active = step(active, &nfa);
            assert!(!active.is_empty());
        }
        assert!(
            active.contains_accept(&nfa),
            "10 001 children satisfy a{{0,10001}}"
        );
        let over = step(active, &nfa);
        assert!(over.is_empty(), "the 10 002nd child must be rejected");
    }

    /// Bounds around every threshold: below/at the minimum, at the maximum,
    /// one past the maximum.
    #[test]
    fn boundary_matrix() {
        for &(min, max) in &[
            (0u32, 17u32),
            (1, 17),
            (17, 17),
            (2, 9_999),
            (0, 10_000),
            (16, 10_000),
            (17, 10_001),
            (0, 100_000),
        ] {
            let nfa = counted_a(min, max);
            let mut active = ActiveStates::try_from_nfa(&nfa).unwrap();
            if min > 0 {
                assert!(
                    !active.contains_accept(&nfa),
                    "{min}..{max}: empty content is invalid"
                );
            }
            for i in 1..=max {
                active = step(active, &nfa);
                assert!(!active.is_empty(), "{min}..{max}: child {i} accepted");
                assert_eq!(
                    active.contains_accept(&nfa),
                    i >= min,
                    "{min}..{max}: completion after {i} children"
                );
            }
            assert!(
                step(active, &nfa).is_empty(),
                "{min}..{max}: child {} rejected",
                max + 1
            );
        }
    }

    /// The representable maximum is enforced exactly and the counter never
    /// overflows: a configuration one below `u32::MAX` accepts one more child
    /// and then rejects the next.
    #[test]
    fn bound_u32_max_arithmetic_is_exact() {
        let nfa = counted_a(0, u32::MAX);
        let term_state = nfa
            .states
            .iter()
            .find(|s| s.term.is_some())
            .map(|s| s.id)
            .expect("term state");
        let at = |count: u32| {
            let mut configs = HashSet::new();
            configs.insert(ActiveConfig {
                state_id: term_state,
                counters: vec![count].into_boxed_slice(),
            });
            ActiveStates::Counted {
                configs,
                num_counters: 1,
            }
        };

        // One below the maximum: the next child completes the maximum, the one after is rejected.
        let active = step(at(u32::MAX - 1), &nfa);
        assert!(active.contains_accept(&nfa));
        assert!(
            step(active, &nfa).is_empty(),
            "u32::MAX + 1 children must be rejected"
        );

        // Two below: two more children fit, the third does not.
        let active = step(at(u32::MAX - 2), &nfa);
        let active = step(active, &nfa);
        assert!(active.contains_accept(&nfa));
        assert!(step(active, &nfa).is_empty());
    }

    /// Every model the former cutoff admitted must still run within the limits.
    #[test]
    fn previously_capped_nested_nullable_shape_fits_limits() {
        let nfa = nested_nullable(10_000);
        let active = ActiveStates::try_from_nfa(&nfa).expect("10 000 nested nullable fits");
        assert!(active.contains_accept(&nfa));
        let active = step(active, &nfa);
        assert!(!active.is_empty());
        assert!(active.contains_accept(&nfa));
    }

    /// A pathological nested nullable model with huge bounds trips an
    /// execution limit instead of hanging or exhausting memory — and the
    /// error is an operational failure, not an acceptance decision.
    #[test]
    fn nested_nullable_huge_bounds_trip_a_limit() {
        let nfa = nested_nullable(1_000_000);
        let err = ActiveStates::try_from_nfa(&nfa).expect_err("must exceed a limit");
        assert!(err.observed > err.allowed, "{err}");
        assert!(
            matches!(
                err.limit,
                ContentModelLimit::ActiveConfigurations | ContentModelLimit::ClosureWork
            ),
            "{err}"
        );
        assert!(err.to_string().contains("execution limit exceeded"));
    }

    /// The legacy infallible entry points panic (loudly) instead of returning
    /// a wrong frontier when a limit is exceeded.
    #[test]
    #[should_panic(expected = "use try_from_nfa")]
    fn legacy_from_nfa_panics_on_limit() {
        let nfa = nested_nullable(1_000_000);
        let _ = ActiveStates::from_nfa(&nfa);
    }

    const B: NameId = NameId(101);

    /// `choice{cmin,cmax}( sequence{smin,smax}( a+ ), b )` — the W3C
    /// `particlesZ036` shape. `counted` forces counter loops regardless of the
    /// unroll threshold; otherwise the bounds are unrolled (counter-free).
    fn choice_seq_model(cmin: u32, cmax: u32, smin: u32, smax: u32, counted: bool) -> NfaTable {
        let builder = FragmentBuilder::new();
        let a_plus = builder
            .single_term(NfaTerm::element(A, None, None), None)
            .repeat_plus();
        let seq = if counted {
            a_plus.repeat_counted(smin, smax)
        } else {
            a_plus.repeat_range(smin, Some(smax))
        };
        let b = builder.single_term(NfaTerm::element(B, None, None), None);
        let choice = seq.alternate(b);
        let outer = if counted {
            choice.repeat_counted(cmin, cmax)
        } else {
            choice.repeat_range(cmin, Some(cmax))
        };
        fragment_to_table(outer)
    }

    fn config_count(active: &ActiveStates) -> usize {
        match active {
            ActiveStates::Counted { configs, .. } => configs.len(),
            ActiveStates::Hybrid { configs, .. } => configs.len(),
            ActiveStates::RangedSingle { state_ranges, .. } => state_ranges.len(),
            ActiveStates::Simple(_) => 0,
        }
    }

    /// Dominance pruning keeps the nested non-nullable counted model bounded:
    /// the W3C `particlesZ036_b` instance (16 660 `a` then `b`s) must run in a
    /// constant-size frontier instead of O(k²) configurations.
    #[test]
    fn dominance_pruning_bounds_nested_nonnullable_counters() {
        let nfa = choice_seq_model(1, 100_000, 1, 100_000_000, true);
        assert_eq!(nfa.counter_defs.len(), 2);
        let mut active = ActiveStates::try_from_nfa(&nfa).unwrap();
        assert!(matches!(active, ActiveStates::Counted { .. }));
        let mut peak = 0;
        let mut at_1000 = 0;
        for i in 0..20_000u32 {
            active = active
                .try_advance(&nfa, A, None, None, None, XsdVersion::V1_0)
                .unwrap();
            assert!(!active.is_empty(), "a #{i} accepted");
            peak = peak.max(config_count(&active));
            if i == 999 {
                at_1000 = config_count(&active);
            }
        }
        let at_20000 = config_count(&active);
        for _ in 0..2 {
            active = active
                .try_advance(&nfa, B, None, None, None, XsdVersion::V1_0)
                .unwrap();
            assert!(!active.is_empty());
            peak = peak.max(config_count(&active));
        }
        assert!(active.contains_accept(&nfa));
        // Steady state: the frontier does not grow with the input (without
        // pruning it would hold ~k²/2 configurations after k children).
        assert_eq!(at_1000, at_20000, "frontier grew with input length");
        assert!(peak <= 64, "frontier must stay bounded, peak was {peak}");
    }

    /// Differential test: the counted automaton (with dominance pruning) must
    /// accept exactly the same prefixes as the unrolled, counter-free
    /// automaton for the same bounds — for every string over {a, b} up to
    /// length 8, including bounds whose minimum exceeds one (the `>= min`
    /// side of the dominance rule) and both XSD versions' advance rules.
    #[test]
    fn counted_with_pruning_matches_unrolled() {
        for &(cmin, cmax, smin, smax) in &[
            (1u32, 3u32, 1u32, 4u32),
            (2, 3, 2, 4),
            (0, 2, 1, 2),
            (1, 2, 0, 3),
        ] {
            let counted = choice_seq_model(cmin, cmax, smin, smax, true);
            let unrolled = choice_seq_model(cmin, cmax, smin, smax, false);
            assert!(counted.has_counters() && !unrolled.has_counters());
            for version in [XsdVersion::V1_0, XsdVersion::V1_1] {
                for len in 0..=8u32 {
                    for bits in 0..(1u32 << len) {
                        let mut c = ActiveStates::try_from_nfa(&counted).unwrap();
                        let mut u = ActiveStates::try_from_nfa(&unrolled).unwrap();
                        let tag = format!(
                            "{cmin},{cmax},{smin},{smax} {version:?} len={len} bits={bits:b}"
                        );
                        assert_eq!(
                            c.contains_accept(&counted),
                            u.contains_accept(&unrolled),
                            "{tag}: empty input"
                        );
                        for i in 0..len {
                            let sym = if bits & (1 << i) == 0 { A } else { B };
                            c = match version {
                                XsdVersion::V1_0 => {
                                    c.try_advance(&counted, sym, None, None, None, version)
                                }
                                XsdVersion::V1_1 => c.try_advance_with_priority(
                                    &counted, sym, None, None, None, version,
                                ),
                            }
                            .unwrap();
                            u = match version {
                                XsdVersion::V1_0 => {
                                    u.try_advance(&unrolled, sym, None, None, None, version)
                                }
                                XsdVersion::V1_1 => u.try_advance_with_priority(
                                    &unrolled, sym, None, None, None, version,
                                ),
                            }
                            .unwrap();
                            assert_eq!(c.is_empty(), u.is_empty(), "{tag}: rejection at {i}");
                            if c.is_empty() {
                                break;
                            }
                            assert_eq!(
                                c.contains_accept(&counted),
                                u.contains_accept(&unrolled),
                                "{tag}: completion at {i}"
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Precomputed successor closures (XSD_COMPILER_REWORK §6.7): the one-child
/// step must equal the reference algorithm — collect the consuming targets of
/// the matching frontier states, then one epsilon-closure DFS — on every
/// counter-free model shape, for every short input, in both XSD versions.
#[cfg(test)]
mod closure_cache_tests {
    use super::tests::element_data;
    use super::*;
    use crate::compiler::fragment::{fragment_to_table, FragmentBuilder};
    use crate::compiler::substitution::build_substitution_group_map;
    use crate::schema::model::XsdVersion;
    use crate::schema::SchemaSet;

    const A: NameId = NameId(100);
    const B: NameId = NameId(101);
    const C: NameId = NameId(102);
    const NS: NameId = NameId(900);

    /// The pre-cache algorithm, kept verbatim as the oracle.
    fn reference_step(
        frontier: &StateSet,
        nfa: &NfaTable,
        name: NameId,
        ns: Option<NameId>,
        subst: Option<&SubstitutionGroupMap>,
        version: XsdVersion,
    ) -> StateSet {
        let mut element_targets = StateSet::with_capacity_for(nfa.state_count());
        let mut wildcard_targets = StateSet::with_capacity_for(nfa.state_count());
        for state_id in frontier.iter() {
            let Some(state) = nfa.get_state(state_id) else {
                continue;
            };
            let Some(term) = state.term.as_ref() else {
                continue;
            };
            if !term_matches(term, name, ns, None, subst, version) {
                continue;
            }
            let targets = match term {
                NfaTerm::Element { .. } => &mut element_targets,
                NfaTerm::Wildcard { .. } => &mut wildcard_targets,
            };
            for t in state.consuming_transitions() {
                targets.insert(t);
            }
        }
        let chosen = match version {
            XsdVersion::V1_0 => {
                let mut all = element_targets;
                for t in wildcard_targets.iter() {
                    all.insert(t);
                }
                all
            }
            XsdVersion::V1_1 => {
                if !element_targets.is_empty() {
                    element_targets
                } else {
                    wildcard_targets
                }
            }
        };
        epsilon_closure(nfa, chosen.iter())
    }

    fn simple(active: &ActiveStates) -> StateSet {
        match active {
            ActiveStates::Simple(s) => s.clone(),
            other => panic!("expected the counter-free path, got {other:?}"),
        }
    }

    /// Enumerate every string over `alphabet` up to `max_len` and compare the
    /// cached step with the oracle after every child.
    fn check_model(
        nfa: &NfaTable,
        alphabet: &[(NameId, Option<NameId>)],
        subst: Option<&SubstitutionGroupMap>,
        max_len: u32,
    ) {
        assert!(!nfa.has_counters());
        let k = alphabet.len() as u32;
        for version in [XsdVersion::V1_0, XsdVersion::V1_1] {
            for len in 0..=max_len {
                for code in 0..k.pow(len) {
                    let mut fast = ActiveStates::try_from_nfa(nfa).unwrap();
                    let mut slow = simple(&fast);
                    let mut c = code;
                    for step in 0..len {
                        let (name, ns) = alphabet[(c % k) as usize];
                        c /= k;
                        let tag = format!("{version:?} len={len} code={code} step={step}");
                        let ref_info = ActiveStates::Simple(slow.clone())
                            .find_match_info(nfa, name, ns, None, subst, version);
                        let (info, next) = fast
                            .advance_element_with_info(nfa, name, ns, None, subst, version)
                            .unwrap();
                        let expected = reference_step(&slow, nfa, name, ns, subst, version);
                        let got = simple(&next);
                        assert!(got == expected, "{tag}: frontier differs");
                        assert_eq!(info.element_key, ref_info.element_key, "{tag}");
                        assert_eq!(info.resolved_type, ref_info.resolved_type, "{tag}");
                        assert_eq!(info.process_contents, ref_info.process_contents, "{tag}");
                        if got.is_empty() {
                            break;
                        }
                        assert_eq!(
                            next.contains_accept(nfa),
                            expected.contains(nfa.accept_state),
                            "{tag}: completion"
                        );
                        fast = next;
                        slow = expected;
                    }
                }
            }
        }
    }

    #[test]
    fn unrolled_choice_of_sequences_matches_reference() {
        // (sequence(a+){1,4} | b){1,3}, fully unrolled (no counters).
        let b = FragmentBuilder::new();
        let a_plus = b
            .single_term(NfaTerm::element(A, None, None), None)
            .repeat_plus();
        let seq = a_plus.repeat_range(1, Some(4));
        let choice = seq.alternate(b.single_term(NfaTerm::element(B, None, None), None));
        let nfa = fragment_to_table(choice.repeat_range(1, Some(3)));
        check_model(&nfa, &[(A, None), (B, None)], None, 7);
    }

    #[test]
    fn nullable_epsilon_cycles_match_reference() {
        // (a? b?)* and ((a?)*)* — dense epsilon cycles through nullable bodies.
        let b = FragmentBuilder::new();
        let ab = b
            .single_term(NfaTerm::element(A, None, None), None)
            .optional()
            .concat(
                b.single_term(NfaTerm::element(B, None, None), None)
                    .optional(),
            )
            .repeat_star();
        check_model(&fragment_to_table(ab), &[(A, None), (B, None)], None, 6);
        let nested = b
            .single_term(NfaTerm::element(A, None, None), None)
            .optional()
            .repeat_star()
            .repeat_star()
            .concat(b.single_term(NfaTerm::element(B, None, None), None));
        check_model(&fragment_to_table(nested), &[(A, None), (B, None)], None, 6);
    }

    #[test]
    fn wildcard_priority_matches_reference_in_both_versions() {
        // (a | any)* , b — element-over-wildcard priority only in XSD 1.1.
        let b = FragmentBuilder::new();
        let any = b.single_term(
            NfaTerm::wildcard(NamespaceConstraint::Any, ProcessContents::Lax),
            None,
        );
        let body = b
            .single_term(NfaTerm::element(A, None, None), None)
            .alternate(any)
            .repeat_star()
            .concat(b.single_term(NfaTerm::element(B, None, None), None));
        let nfa = fragment_to_table(body);
        check_model(&nfa, &[(A, None), (B, None), (C, Some(NS))], None, 5);
    }

    #[test]
    fn substitution_group_members_match_reference() {
        let mut schema_set = SchemaSet::new();
        let head = schema_set.name_table.add("head");
        let m1 = schema_set.name_table.add("m1");
        let m2 = schema_set.name_table.add("m2");
        let head_key = schema_set.arenas.alloc_element(element_data(head));
        for m in [m1, m2] {
            let key = schema_set.arenas.alloc_element(element_data(m));
            schema_set
                .arenas
                .entries_mut()
                .elements
                .get_mut(key)
                .unwrap()
                .resolved_substitution_groups
                .push(head_key);
        }
        let map = build_substitution_group_map(&schema_set);
        assert!(map.contains_key(&head_key));
        // (head? , b)* with the head substitutable by m1/m2.
        let b = FragmentBuilder::new();
        let body = b
            .single_term(NfaTerm::element(head, None, Some(head_key)), None)
            .optional()
            .concat(b.single_term(NfaTerm::element(B, None, None), None))
            .repeat_star();
        let nfa = fragment_to_table(body);
        check_model(
            &nfa,
            &[(head, None), (m1, None), (m2, None), (B, None)],
            Some(&map),
            5,
        );
    }

    #[test]
    fn union_with_promotes_and_ors_words() {
        let mut a = StateSet::new();
        a.insert(3);
        let mut b = StateSet::new();
        b.insert(70);
        a.union_with(&b);
        assert!(a.contains(3) && a.contains(70));
        assert!(matches!(a, StateSet::Inline(_)));
        let mut big = StateSet::new();
        big.insert(300);
        a.union_with(&big);
        assert!(matches!(a, StateSet::Heap(_)));
        assert!(a.contains(3) && a.contains(70) && a.contains(300));
        // Empty operand: no promotion, no change.
        let mut c = StateSet::new();
        c.insert(1);
        c.union_with(&StateSet::with_capacity_for(1000));
        assert!(matches!(c, StateSet::Inline(_)) && c.contains(1));
    }

    #[test]
    fn successor_closure_is_rebuilt_after_mutation() {
        let b = FragmentBuilder::new();
        let mut nfa = fragment_to_table(
            b.single_term(NfaTerm::element(A, None, None), None)
                .concat(b.single_term(NfaTerm::element(B, None, None), None)),
        );
        let a_state = nfa.states.iter().find(|s| s.term.is_some()).unwrap().id;
        let before = nfa.successor_closure(a_state).clone();
        assert!(!before.contains(nfa.accept_state));
        // Add an epsilon edge from a's successor straight to accept.
        let succ = nfa
            .get_state(a_state)
            .unwrap()
            .consuming_transitions()
            .next()
            .unwrap();
        let accept = nfa.accept_state;
        nfa.get_state_mut(succ).unwrap().add_epsilon(accept);
        let after = nfa.successor_closure(a_state);
        assert!(
            after.contains(accept),
            "cache must be invalidated by get_state_mut"
        );
    }
}
