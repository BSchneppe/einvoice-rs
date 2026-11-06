//! Reuse of a general-comparison index across evaluations of one comparison.
//!
//! [`general_compare`](crate::xpath::general_compare) answers one `A = B` in
//! `O(|A| + |B|)` instead of `O(|A| · |B|)`, but it rebuilds its index every time
//! the comparison is evaluated. That is not enough for the shape
//!
//! ```text
//!     $big[not(. = $c)]
//! ```
//!
//! where **one** compiled expression is evaluated **once** but its predicate
//! evaluates the **same** comparison node up to millions of times: each
//! evaluation compares a small varying operand (the context item) against an
//! operand that does not change at all (`$c`). Each evaluation is linear in `$c`,
//! so the filter as a whole is quadratic.
//!
//! This module keeps the index of the operand that does not change for the
//! duration of one **run**, so that each later evaluation of that comparison node
//! is a hash probe plus the same confirmation by the real `eq`.
//!
//! # Where the cache lives, and how long
//!
//! In [`DynamicContext`], which is created by
//! [`XPathEvaluator::run`](crate::xpath::XPathEvaluator::run) and dropped when
//! that run returns: one cache per run, never in the compiled expression (which
//! is shared and immutable) and never global. Entries are keyed by the identity
//! of the comparison's AST node — its arena's [serial](AstArena::serial) and its
//! node id, so that a nested evaluation over a different arena can never collide
//! with them. The serial and not the address: a host function that compiles and
//! evaluates expressions of its own on the same `DynamicContext` can drop one
//! arena and get the next one allocated at the very same address.
//!
//! # When an operand counts as invariant
//!
//! Statically, and conservatively:
//! [`deps::run_invariant_vars`](crate::xpath::deps::run_invariant_vars) accepts an
//! operand subtree only when it reads no part of the focus it is entered with,
//! calls no function at all, and references no variable bound by a `for`, `some`
//! or `every` anywhere in the expression. What is left is a pure function of
//! state that cannot change while the expression runs: variables bound before the
//! run, literals, ranges, sequence constructors, arithmetic, `if`, and paths and
//! predicates rooted in any of those.
//!
//! Because such an operand cannot raise an error it did not raise on the first
//! evaluation, and cannot have effects, **not evaluating it** on later
//! evaluations is invisible — which is what makes the saving possible at all: for
//! `$c` the evaluation alone would clone a sequence of 100,000 items.
//!
//! On top of the static rule, every probe re-checks a cheap identity of what the
//! index was built from: for each variable slot the operand reads, the number of
//! times that slot has been written ([`VarStore::generation`](crate::xpath::VarStore),
//! bumped by every write, and the store hands out no mutable reference to a value)
//! together with the address and length of its backing storage. If either
//! differs the index is thrown away and rebuilt on the next evaluation, so a
//! rebinding this analysis did not expect costs speed rather than correctness.
//!
//! # The other operand can write while this one is being read
//!
//! The static rule constrains only the operand that is indexed. The *other* one
//! is unconstrained, and a function call in it reaches a host
//! [`FunctionEvaluator`](crate::xpath::functions::FunctionEvaluator) with
//! `&mut DynamicContext`, which can rebind any variable — including one the
//! invariant operand reads. Both operands are evaluated before an index is
//! installed, so a fingerprint taken at the end would describe a *newer* binding
//! than the value in hand, and later probes would answer from a value the
//! expression can no longer produce. Two rules keep that from happening:
//!
//! * the fingerprint of an operand is taken **at the instant that operand is
//!   evaluated**, and an index is installed only if the fingerprint still holds
//!   once the other operand has run as well;
//! * a probe checks the fingerprint **before either operand is evaluated**, so a
//!   stale index sends the whole comparison back to the untouched path with its
//!   operands evaluated in source order — left first, then right, exactly as
//!   without the cache, so effects and errors happen in the order the expression
//!   asks for.
//!
//! When the varying operand *does* write such a variable while it runs, where
//! the invariant operand sits decides what that means. On the **left** it is
//! evaluated before the varying operand, so the value the index holds is the one
//! this comparison must use and only later evaluations rebuild. On the **right**
//! it is evaluated after, so the value it would now produce is the rebound one
//! and the index must not answer.
//!
//! # Laziness
//!
//! Nothing is built on the first evaluation of a comparison node: it runs the
//! code of the previous wave, byte for byte. The index is built while the second
//! evaluation runs, and only when the invariant operand has at least
//! [`MIN_INVARIANT_ITEMS`] atomized items — below that the bounded pairwise
//! prefix in `operators` already answers the comparison at a cost no index can
//! beat. A comparison evaluated once, or with two small operands, therefore costs
//! what it cost before.
//!
//! # Why the answers stay identical
//!
//! The index answers a comparison only when it can prove both the answer *and*
//! the absence of a hard error, by the same three phases
//! [`general_compare`](crate::xpath::general_compare) documents: every key the
//! product needs is computed with the conversion the comparison itself performs,
//! every candidate pair is confirmed by the real `eq`, and a deferred type error
//! is raised by the very pair the pairwise loop would have raised it for.
//! Anything else — an unmodelled class pair, a failing conversion, an atomization
//! error, a `confirm` that unexpectedly raises — makes it decline, and the
//! declining path evaluates the operand it stood for and runs the untouched
//! comparison. XPath 1.0 compatibility mode never reaches this module.

use std::collections::HashMap;
use std::hash::BuildHasherDefault;

use ahash::AHasher;

use crate::types::value::{TimezoneOffset, XmlValue};
use crate::xpath::arena::{AstArena, AstNodeId};
use crate::xpath::collation::CollationRef;
use crate::xpath::context::{DynamicContext, VarSlotId, XPathContext};
use crate::xpath::deps::{self, SlotSet};
use crate::xpath::error::XPathError;
use crate::xpath::eval::eval_node;
use crate::xpath::functions::XPathValue;
use crate::xpath::general_compare::{self, Bucket, Class, FastOutcome, KeyHash, PairKind};
use crate::xpath::iterator::{VecNodeIterator, XmlItem};
use crate::xpath::operators;
use crate::xpath::DomNavigator;

/// Hasher for the join tables. A `BuildHasherDefault` is a ZST, so a table costs
/// nothing to seed.
type Hasher = BuildHasherDefault<AHasher>;

/// The evaluation at which a comparison node's invariant operand is indexed.
///
/// `1` would index on the very first evaluation, making a comparison that is
/// evaluated once pay for a table nobody probes. `2` leaves the first evaluation
/// exactly as it was and builds the index while the second one runs.
const BUILD_AT_EVALUATION: u32 = 2;

/// The smallest invariant operand worth indexing, in atomized items.
///
/// `1` — i.e. any non-empty operand — because the crossover measured in
/// `benches/general_compare.rs` is below the smallest size there is: a filter over
/// 200,000 items against a **one**-item invariant operand takes 136 ms without an
/// index and 75 ms with one. Not evaluating the operand at all is most of that;
/// not rebuilding a `VecNodeIterator`, a `BufferedNodeIterator` and an atomized
/// value per evaluation is the rest. An empty operand is still skipped: the
/// comparison is then `false` and the untouched path answers it without touching
/// the other side.
const MIN_INVARIANT_ITEMS: usize = 1;

/// The largest **varying** operand the index is used for, in items.
///
/// The index has to atomize the whole varying operand before it can prove that no
/// hard error hides in the product, while the bounded pairwise prefix in
/// `operators` may answer a comparison from its very first pair. With a small
/// varying operand — the singleton of `$big[. = $c]`, an attribute, a short
/// sequence — that difference is a few atomizations; with a large one it would be
/// a constant factor on top of an already linear evaluation, and a comparison with
/// an early true pair would pay it for nothing. Above this the index declines and
/// the untouched path runs, which is exactly what it cost before.
const MAX_VARYING_ITEMS: usize = 64;

/// How many comparison nodes one run tracks. Expressions have a handful; the cap
/// bounds the linear scan when a generated expression has hundreds.
const MAX_TRACKED_NODES: usize = 32;

/// End of a collision chain.
const NO_SLOT: u32 = u32::MAX;

/// The class pairs of one comparison: the ones that join, each with the bucket it
/// joins in, and the ones that raise, in the comparison's own `(left, right)`
/// order.
type ClassPairs = (Vec<(Class, Class, Bucket)>, Vec<(Class, Class)>);

/// The keys of one class of the varying operand in one bucket: the positions and
/// key hashes of the values that have a key, and the positions of the ones whose
/// key is `Never`.
type ProbeKeys = (Vec<(usize, u64)>, Vec<usize>);

// ============================================================================
// Entry point
// ============================================================================

/// Evaluate `left = right` (`is_eq`) or `left != right` for the comparison node
/// `node`, reusing the index of an operand that does not change during this run.
///
/// This is the whole XPath 2.0 `=` / `!=` path of
/// [`eval_node`](crate::xpath::eval::eval_node): when no index applies it
/// evaluates both operands in source order and calls the same
/// `operators::general_eq_iter` / `general_ne_iter` the evaluator called before.
pub(crate) fn eval_general_eq_ne<N: DomNavigator>(
    arena: &AstArena,
    node: AstNodeId,
    left: AstNodeId,
    right: AstNodeId,
    is_eq: bool,
    ctx: &mut DynamicContext<'_, N>,
) -> Result<bool, XPathError> {
    let arena_id = arena.serial();
    let plan = ctx.general_compare_cache_mut().plan(arena_id, node);

    match plan {
        Plan::Plain => plain(arena, left, right, is_eq, ctx),

        Plan::Consider => {
            // The invariance analysis reads only the AST, so it runs before
            // either operand does: an operand's fingerprint has to be taken at
            // the instant that operand is evaluated, and for the left one that
            // instant is before the right one gets a chance to write.
            let static_context = ctx.static_context;
            let range_vars = ctx.general_compare_cache_mut().range_vars(arena).clone();
            let left_vars = deps::run_invariant_vars(arena, left, static_context, &range_vars);
            let right_vars = deps::run_invariant_vars(arena, right, static_context, &range_vars);

            let left_value = eval_node(arena, left, ctx)?;
            let left_print = left_vars.map(|vars| fingerprint(ctx, &vars));
            let right_value = eval_node(arena, right, ctx)?;
            // Nothing runs between the right operand and here, so its own
            // fingerprint cannot already be out of date. The left one can: a
            // host function in the right operand receives `&mut DynamicContext`
            // and may have rebound a variable the left operand read, in which
            // case the left value in hand is the one from *before* that write
            // and must not be paired with the generations in hand.
            let right_print = right_vars.map(|vars| fingerprint(ctx, &vars));
            let left_print = left_print.filter(|print| unchanged(ctx, print));

            install(
                arena_id,
                node,
                ctx,
                (left_print, &left_value),
                (right_print, &right_value),
            );
            compare_values(ctx, left_value, right_value, is_eq)
        }

        Plan::Probe(side) => probe_plan(arena, arena_id, node, left, right, side, is_eq, ctx),
    }
}

/// Both operands, evaluated in source order, compared as the evaluator compared
/// them before this module existed.
fn plain<N: DomNavigator>(
    arena: &AstArena,
    left: AstNodeId,
    right: AstNodeId,
    is_eq: bool,
    ctx: &mut DynamicContext<'_, N>,
) -> Result<bool, XPathError> {
    let left_value = eval_node(arena, left, ctx)?;
    let right_value = eval_node(arena, right, ctx)?;
    compare_values(ctx, left_value, right_value, is_eq)
}

/// One evaluation of a comparison node whose invariant operand is already
/// indexed.
#[allow(clippy::too_many_arguments)]
fn probe_plan<N: DomNavigator>(
    arena: &AstArena,
    arena_id: u64,
    node: AstNodeId,
    left: AstNodeId,
    right: AstNodeId,
    side: Side,
    is_eq: bool,
    ctx: &mut DynamicContext<'_, N>,
) -> Result<bool, XPathError> {
    // Asked **before** anything is evaluated, so that a stale index never
    // reorders the operands: the answer is then the untouched path, left first
    // and right second, which is what the expression asks for.
    if !index_is_fresh(arena_id, node, ctx) {
        ctx.general_compare_cache_mut().reconsider(arena_id, node);
        return plain(arena, left, right, is_eq, ctx);
    }

    // Only the varying operand is evaluated — `side` names the one the index
    // stands for. Leaving the invariant operand unevaluated is unobservable: at
    // the fingerprint just checked it would produce the value the index holds,
    // and it can neither raise nor have an effect.
    let (varying, invariant) = match side {
        Side::Right => (left, right),
        Side::Left => (right, left),
    };
    let varying_value = eval_node(arena, varying, ctx)?;

    // Did evaluating it write a variable the index depends on? Where the
    // invariant operand sits decides what that means: on the left it was
    // evaluated before the write, so the index still holds the value source
    // order gives it; on the right it would be evaluated after, so it must not.
    let disturbed = !index_is_fresh(arena_id, node, ctx);
    let answerable = side == Side::Left || !disturbed;

    let outcome = if answerable {
        probe(arena_id, node, is_eq, ctx, &varying_value)
    } else {
        None
    };
    #[cfg(test)]
    if outcome.is_some() {
        ctx.general_compare_cache_mut().answered += 1;
    }

    let result = match outcome {
        Some(result) => result,
        None => {
            // The invariant operand's value, as source order gives it.
            let held = if disturbed && side == Side::Left {
                // Re-evaluating it here would read a write that source order
                // puts *after* it. The index holds the value from before.
                held_value(arena_id, node, ctx)
            } else {
                None
            };
            let other = match held {
                Some(value) => value,
                None => match eval_node(arena, invariant, ctx) {
                    Ok(value) => value,
                    Err(err) => {
                        if disturbed {
                            ctx.general_compare_cache_mut().reconsider(arena_id, node);
                        }
                        return Err(err);
                    }
                },
            };
            let (left_value, right_value) = side.order(varying_value, other);
            compare_values(ctx, left_value, right_value, is_eq)
        }
    };

    if disturbed {
        ctx.general_compare_cache_mut().reconsider(arena_id, node);
    }
    result
}

/// The comparison as the evaluator ran it before this module existed, with the
/// default collation resolved — through the per-run memo — only if two strings
/// are compared.
fn compare_values<N: DomNavigator>(
    ctx: &mut DynamicContext<'_, N>,
    left: XPathValue<N>,
    right: XPathValue<N>,
    is_eq: bool,
) -> Result<bool, XPathError> {
    let context = ctx.static_context;
    let left_iter = VecNodeIterator::new(left.into_vec());
    let right_iter = VecNodeIterator::new(right.into_vec());
    let tz = ctx.implicit_timezone_or_host();
    let deferred = ctx.deferred_default_collation();
    if is_eq {
        operators::general_eq_iter_collated(context, &left_iter, &right_iter, deferred.as_ref(), tz)
    } else {
        operators::general_ne_iter_collated(context, &left_iter, &right_iter, deferred.as_ref(), tz)
    }
}

// ============================================================================
// Which operand is which
// ============================================================================

/// Which operand of the comparison holds the invariant sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Right,
}

impl Side {
    /// Put a probe-side thing and a cached-side thing into the comparison's own
    /// `(left, right)` order.
    #[inline]
    fn order<T>(self, probe: T, cached: T) -> (T, T) {
        match self {
            Side::Right => (probe, cached),
            Side::Left => (cached, probe),
        }
    }
}

// ============================================================================
// The index of one invariant operand
// ============================================================================

/// A hash join table over the values of one comparison class of the invariant
/// operand, in one comparison bucket.
struct BucketIndex {
    /// For every indexed value: its position in the operand, and its key hash.
    slots: Vec<(u32, u64)>,
    /// Key hash → the last slot carrying it; `next` chains the earlier ones, so
    /// the table costs two allocations instead of one per distinct key.
    head: HashMap<u64, u32, Hasher>,
    next: Vec<u32>,
    /// Positions of the values of this class whose key is `Never` (`NaN`): they
    /// compare equal to nothing, and comparing them raises nothing.
    never: Vec<u32>,
}

impl BucketIndex {
    /// Index every value of `class` in `bucket`, or `None` when a key conversion
    /// fails — a hard error is then possible and this class pair must never be
    /// answered from an index.
    fn build(
        values: &[XmlValue],
        classes: &[Class],
        class: Class,
        bucket: Bucket,
        collation: CollationRef<'_>,
        tz: TimezoneOffset,
    ) -> Option<Self> {
        let mut slots: Vec<(u32, u64)> = Vec::new();
        let mut never: Vec<u32> = Vec::new();
        for (index, value) in values.iter().enumerate() {
            if classes[index] != class {
                continue;
            }
            match general_compare::key_hash(value, class, bucket, collation, tz) {
                KeyHash::Found(hash) => slots.push((index as u32, hash)),
                KeyHash::Never => never.push(index as u32),
                KeyHash::Failed => return None,
            }
        }
        let mut head: HashMap<u64, u32, Hasher> =
            HashMap::with_capacity_and_hasher(slots.len(), Hasher::default());
        let mut next = vec![NO_SLOT; slots.len()];
        for (slot, &(_, hash)) in slots.iter().enumerate() {
            if let Some(previous) = head.insert(hash, slot as u32) {
                next[slot] = previous;
            }
        }
        Some(Self {
            slots,
            head,
            next,
            never,
        })
    }

    /// The position of a value whose key hash is **not** `hash`, if the bucket
    /// holds one. `None` when every indexed value carries `hash`.
    fn any_other_than(&self, hash: u64) -> Option<usize> {
        // At most two entries have to be looked at: if a second distinct hash
        // exists, one of the first two is it.
        self.head
            .iter()
            .find(|(&key, _)| key != hash)
            .map(|(_, &slot)| self.slots[slot as usize].0 as usize)
    }

    /// The position of any indexed value, if the bucket holds one.
    fn any(&self) -> Option<usize> {
        self.slots.first().map(|&(index, _)| index as usize)
    }
}

/// The atomized invariant operand of one comparison node, plus the join tables
/// built for it so far.
struct CachedOperand {
    values: Vec<XmlValue>,
    classes: Vec<Class>,
    /// The distinct classes present, in first-occurrence order, each with the
    /// position of that first occurrence — which is what locating the first
    /// incomparable pair in row-major order needs.
    first_of_class: Vec<(Class, usize)>,
    /// Built on demand, because the bucket a class is compared in depends on the
    /// *other* operand's class. `None` records a class/bucket pair whose keys
    /// cannot all be computed.
    tables: HashMap<(Class, Bucket), Option<BucketIndex>, Hasher>,
}

impl CachedOperand {
    /// `None` when a value's comparison class is not modelled.
    fn new(values: Vec<XmlValue>) -> Option<Self> {
        let classes = general_compare::classify_all(&values)?;
        let mut first_of_class: Vec<(Class, usize)> = Vec::new();
        for (index, &class) in classes.iter().enumerate() {
            if !first_of_class.iter().any(|(seen, _)| *seen == class) {
                first_of_class.push((class, index));
            }
        }
        Some(Self {
            values,
            classes,
            first_of_class,
            tables: HashMap::default(),
        })
    }

    /// Ensure the table for `(class, bucket)` exists; `false` when it cannot be
    /// built.
    fn ensure_table(
        &mut self,
        class: Class,
        bucket: Bucket,
        collation: CollationRef<'_>,
        tz: TimezoneOffset,
    ) -> bool {
        if !self.tables.contains_key(&(class, bucket)) {
            let built =
                BucketIndex::build(&self.values, &self.classes, class, bucket, collation, tz);
            self.tables.insert((class, bucket), built);
        }
        matches!(self.tables.get(&(class, bucket)), Some(Some(_)))
    }

    /// The table for `(class, bucket)`, which [`ensure_table`](Self::ensure_table)
    /// has already reported as buildable.
    fn table(&self, class: Class, bucket: Bucket) -> Option<&BucketIndex> {
        self.tables.get(&(class, bucket)).and_then(Option::as_ref)
    }

    /// A rough byte count of what this entry holds, for the record.
    #[cfg(test)]
    fn footprint(&self) -> usize {
        let values = self.values.len() * std::mem::size_of::<XmlValue>();
        let classes = self.classes.len() * std::mem::size_of::<Class>();
        let tables: usize = self
            .tables
            .values()
            .flatten()
            .map(|table| {
                table.slots.len() * (std::mem::size_of::<(u32, u64)>() + 4)
                    + table.head.capacity() * std::mem::size_of::<(u64, u32)>()
            })
            .sum();
        values + classes + tables
    }
}

// ============================================================================
// The cache
// ============================================================================

/// A cheap identity of what a variable slot held when the index was built.
///
/// This is the safety net, not the proof: the static rule already guarantees that
/// the engine does not write a non-range slot while the expression runs. The
/// write count is the reliable half — [`VarStore`](crate::xpath::VarStore) bumps it
/// on every write and hands out no mutable reference to a value, so an unchanged
/// count means the slot still holds the very value it held. The shape is a second,
/// free check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ValueId {
    /// The [serial](crate::xpath::VarStore) of the store the slot lives in: a
    /// generation counts writes within one store only, and the store of a
    /// `DynamicContext` can be replaced wholesale.
    store: u64,
    /// How often the slot had been written.
    generation: u64,
    shape: Shape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// Not bound, or bound to the empty sequence.
    Unset,
    /// Bound to a single item.
    Single,
    /// The address and the length of a sequence's backing storage.
    Sequence(usize, usize),
}

fn value_id<N: DomNavigator>(ctx: &DynamicContext<'_, N>, slot: VarSlotId) -> ValueId {
    let shape = match ctx.get_variable(slot) {
        None | Some(XPathValue::Empty) => Shape::Unset,
        Some(XPathValue::Item(_)) => Shape::Single,
        Some(XPathValue::Sequence(sequence)) => {
            Shape::Sequence(sequence.as_ptr() as usize, sequence.len())
        }
    };
    ValueId {
        store: ctx.variable_store_serial(),
        generation: ctx.variable_generation(slot),
        shape,
    }
}

/// The variable slots one operand reads, each with what its slot held at the
/// instant that operand was evaluated.
type Fingerprint = Vec<(VarSlotId, ValueId)>;

/// What the slots of `vars` hold right now.
fn fingerprint<N: DomNavigator>(ctx: &DynamicContext<'_, N>, vars: &[VarSlotId]) -> Fingerprint {
    vars.iter()
        .map(|&slot| (slot, value_id(ctx, slot)))
        .collect()
}

/// Whether every slot of `print` still holds what it held when `print` was taken.
fn unchanged<N: DomNavigator>(ctx: &DynamicContext<'_, N>, print: &[(VarSlotId, ValueId)]) -> bool {
    print.iter().all(|&(slot, id)| value_id(ctx, slot) == id)
}

/// A built index, with what it was built from.
struct Ready {
    side: Side,
    /// The variable slots the invariant operand reads, and what was bound to them
    /// when the index was built.
    fingerprint: Fingerprint,
    operand: CachedOperand,
}

enum State {
    /// Evaluated this many times, not yet analysed.
    Counting(u32),
    /// Statically not cacheable, or the invariant operand is too small: this node
    /// runs the untouched path for the rest of the run.
    Off,
    Ready(Box<Ready>),
}

struct Entry {
    node: AstNodeId,
    state: State,
}

/// What [`GeneralCompareCache::plan`] tells the caller to do.
enum Plan {
    /// Run the untouched path.
    Plain,
    /// Run the untouched path, then decide whether to index an operand.
    Consider,
    /// An index for this side is ready; evaluate only the other operand.
    Probe(Side),
}

/// The general-comparison indexes of one evaluation run.
///
/// Lives in [`DynamicContext`] and dies with it. Empty and allocation-free until
/// the first general comparison of the run is evaluated.
#[derive(Default)]
pub(crate) struct GeneralCompareCache {
    /// [Serial](AstArena::serial) of the AST arena `entries` belong to; `0`
    /// (which no arena carries) before the first comparison of the run.
    arena: u64,
    entries: Vec<Entry>,
    /// The range variables of that arena, computed once on first use.
    range_vars: Option<SlotSet>,
    /// How many evaluations this run answered from an index. Test-only, so that a
    /// test can tell "the fix keeps the cache correct" from "the fix switched the
    /// cache off"; the shipped fast path pays nothing for it.
    #[cfg(test)]
    answered: u32,
}

impl GeneralCompareCache {
    /// Decide what to do with the comparison node `node` of the arena `arena`.
    fn plan(&mut self, arena: u64, node: AstNodeId) -> Plan {
        if self.arena != arena {
            // A different expression is being evaluated with this context: the
            // node ids of the old one mean nothing here.
            self.arena = arena;
            self.entries.clear();
            self.range_vars = None;
        }
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.node == node) {
            return match &mut entry.state {
                State::Off => Plan::Plain,
                State::Ready(ready) => Plan::Probe(ready.side),
                State::Counting(seen) => {
                    *seen += 1;
                    if *seen >= BUILD_AT_EVALUATION {
                        Plan::Consider
                    } else {
                        Plan::Plain
                    }
                }
            };
        }
        if self.entries.len() < MAX_TRACKED_NODES {
            self.entries.push(Entry {
                node,
                state: State::Counting(1),
            });
        }
        Plan::Plain
    }

    /// The range variables of `arena`, computed once per run.
    fn range_vars(&mut self, arena: &AstArena) -> &SlotSet {
        self.range_vars
            .get_or_insert_with(|| deps::range_var_slots(arena))
    }

    fn state_mut(&mut self, arena: u64, node: AstNodeId) -> Option<&mut State> {
        if self.arena != arena {
            return None;
        }
        self.entries
            .iter_mut()
            .find(|entry| entry.node == node)
            .map(|entry| &mut entry.state)
    }

    fn ready(&self, arena: u64, node: AstNodeId) -> Option<&Ready> {
        if self.arena != arena {
            return None;
        }
        self.entries
            .iter()
            .find(|entry| entry.node == node)
            .and_then(|entry| match &entry.state {
                State::Ready(ready) => Some(&**ready),
                _ => None,
            })
    }

    /// Record the outcome of considering a node, whatever it is: a node that
    /// stayed `Counting` would be analysed again on every evaluation.
    fn settle(&mut self, arena: u64, node: AstNodeId, state: State) {
        if let Some(slot) = self.state_mut(arena, node) {
            *slot = state;
        }
    }

    /// Throw an index away and let the **next** evaluation of the node consider
    /// it again, with the bindings in force then. Rebuilding rather than trusting
    /// it costs speed, never correctness.
    fn reconsider(&mut self, arena: u64, node: AstNodeId) {
        self.settle(arena, node, State::Counting(BUILD_AT_EVALUATION - 1));
    }

    /// How many evaluations of this run were answered from an index.
    #[cfg(test)]
    fn answered(&self) -> u32 {
        self.answered
    }
}

/// Whether the index of `node` still stands for what its operand would evaluate
/// to with the bindings in force now.
fn index_is_fresh<N: DomNavigator>(
    arena_id: u64,
    node: AstNodeId,
    ctx: &DynamicContext<'_, N>,
) -> bool {
    match ctx.general_compare_cache().ready(arena_id, node) {
        Some(ready) => unchanged(ctx, &ready.fingerprint),
        None => false,
    }
}

/// The value the index of `node` stands for, as an atomized sequence.
///
/// A general comparison atomizes both its operands, and
/// [`atomize_items`](general_compare::atomize_items) is the very function the
/// untouched path atomizes with, so handing the atomized sequence to
/// [`compare_values`] compares exactly the values the operand's own items would
/// have produced. Used only where re-evaluating the operand would read a write
/// that source order puts after it.
fn held_value<N: DomNavigator>(
    arena_id: u64,
    node: AstNodeId,
    ctx: &DynamicContext<'_, N>,
) -> Option<XPathValue<N>> {
    let ready = ctx.general_compare_cache().ready(arena_id, node)?;
    Some(XPathValue::from_sequence(
        ready
            .operand
            .values
            .iter()
            .cloned()
            .map(XmlItem::Atomic)
            .collect(),
    ))
}

// ============================================================================
// Installing an index
// ============================================================================

/// Decide whether one of the operands of `node` can be indexed for the rest of
/// the run, and index it if so. Called with both operands already evaluated, so
/// that nothing is evaluated twice.
///
/// Each operand comes with `Some(fingerprint)` when it is run invariant *and*
/// that fingerprint still describes the value handed in, and `None` otherwise —
/// which is where an operand whose bindings the other operand has rewritten
/// drops out.
fn install<N: DomNavigator>(
    arena_id: u64,
    node: AstNodeId,
    ctx: &mut DynamicContext<'_, N>,
    left: (Option<Fingerprint>, &XPathValue<N>),
    right: (Option<Fingerprint>, &XPathValue<N>),
) {
    // Consider the longer operand first: with both invariant, indexing the longer
    // one is what saves the most, and a probe over the shorter one is cheap.
    let longer_is_left = left.1.len() > right.1.len();
    let mut candidates = [
        (Side::Right, right.0, right.1),
        (Side::Left, left.0, left.1),
    ];
    if longer_is_left {
        candidates.swap(0, 1);
    }

    for (side, print, value) in candidates {
        let Some(fingerprint) = print else {
            continue;
        };
        if value.len() < MIN_INVARIANT_ITEMS {
            continue;
        }
        // Atomize exactly as the pairwise loop would; an error there means the
        // loop may raise, so the comparison must keep running through it.
        let Some(values) = general_compare::atomize_items(value.as_slice()) else {
            continue;
        };
        if values.len() < MIN_INVARIANT_ITEMS {
            continue;
        }
        let Some(operand) = CachedOperand::new(values) else {
            continue;
        };
        ctx.general_compare_cache_mut().settle(
            arena_id,
            node,
            State::Ready(Box::new(Ready {
                side,
                fingerprint,
                operand,
            })),
        );
        return;
    }

    ctx.general_compare_cache_mut()
        .settle(arena_id, node, State::Off);
}

// ============================================================================
// Probing an index
// ============================================================================

/// Answer the comparison from the index of node `node`, or `None` to say that the
/// caller must produce the other operand and run the untouched path.
///
/// Whether the index still describes what its operand stands for is the caller's
/// business — [`probe_plan`] asks that before either operand is evaluated, and
/// again afterwards.
fn probe<N: DomNavigator>(
    arena_id: u64,
    node: AstNodeId,
    is_eq: bool,
    ctx: &mut DynamicContext<'_, N>,
    varying: &XPathValue<N>,
) -> Option<Result<bool, XPathError>> {
    if varying.len() > MAX_VARYING_ITEMS {
        return None;
    }
    // The varying operand, atomized exactly as the pairwise loop atomizes it.
    let probe_values = general_compare::atomize_items(varying.as_slice())?;

    let static_context = ctx.static_context;
    // The implicit timezone, which cannot change during a run either (it is
    // read once per dynamic context), so an index keyed on date/time values
    // stays keyed the way this probe reads them.
    let tz = ctx.implicit_timezone_or_host();
    // The static context's default collation, which cannot change during a run,
    // so an index built under it stays keyed the way this probe reads it. It is
    // resolved — through the per-run memo — only when a string key is needed.
    // Under the codepoint collation — the default — this is a discriminant test
    // and every key below is the one it always was.
    let (cache, deferred) = ctx.compare_cache_and_default_collation();
    let collation = deferred.as_ref();
    let state = cache.state_mut(arena_id, node)?;
    let State::Ready(ready) = state else {
        return None;
    };
    let side = ready.side;
    let outcome = if is_eq {
        indexed_eq(
            static_context,
            side,
            &mut ready.operand,
            &probe_values,
            collation,
            tz,
        )
    } else {
        indexed_ne(
            static_context,
            side,
            &mut ready.operand,
            &probe_values,
            collation,
            tz,
        )
    };
    match outcome {
        FastOutcome::Decided(result) => Some(Ok(result)),
        FastOutcome::Raise(err) => Some(Err(err)),
        FastOutcome::Fallback => None,
    }
}

/// The class pairs of this comparison, split into the ones that join and the ones
/// that raise. `None` when a pair is not modelled.
fn class_pairs(side: Side, cached: &CachedOperand, probe_set: &[Class]) -> Option<ClassPairs> {
    let mut joins: Vec<(Class, Class, Bucket)> = Vec::new();
    let mut incomparable: Vec<(Class, Class)> = Vec::new();
    for &probe_class in probe_set {
        for &(cached_class, _) in &cached.first_of_class {
            let (left, right) = side.order(probe_class, cached_class);
            match general_compare::pair_kind(left, right) {
                PairKind::Comparable(bucket) => joins.push((probe_class, cached_class, bucket)),
                PairKind::Incomparable => incomparable.push((left, right)),
                PairKind::Unsupported => return None,
            }
        }
    }
    Some((joins, incomparable))
}

/// The keys of the probe operand's values of class `class`, in `bucket`.
///
/// `None` when a conversion the comparison would perform fails. The second
/// component holds the positions of the values whose key is `Never`.
fn probe_keys(
    values: &[XmlValue],
    classes: &[Class],
    class: Class,
    bucket: Bucket,
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
) -> Option<ProbeKeys> {
    let mut found: Vec<(usize, u64)> = Vec::new();
    let mut never: Vec<usize> = Vec::new();
    for (index, value) in values.iter().enumerate() {
        if classes[index] != class {
            continue;
        }
        match general_compare::key_hash(value, class, bucket, collation, tz) {
            KeyHash::Found(hash) => found.push((index, hash)),
            KeyHash::Never => never.push(index),
            KeyHash::Failed => return None,
        }
    }
    Some((found, never))
}

/// `probe = cached` (in `side`'s order), answered from the cached index.
fn indexed_eq(
    context: &XPathContext,
    side: Side,
    cached: &mut CachedOperand,
    probe: &[XmlValue],
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
) -> FastOutcome {
    if probe.is_empty() || cached.values.is_empty() {
        return FastOutcome::Decided(false);
    }
    let Some(probe_classes) = general_compare::classify_all(probe) else {
        return FastOutcome::Fallback;
    };
    let probe_set = general_compare::distinct(&probe_classes);
    let Some((joins, incomparable)) = class_pairs(side, cached, &probe_set) else {
        return FastOutcome::Fallback;
    };

    // Phase 1 — every key the product needs, computed with the conversion the
    // comparison itself performs. One failure means a hard error is possible
    // somewhere in the product, and a hard error can preempt a true pair, so
    // nothing may be decided here.
    for &(_, cached_class, bucket) in &joins {
        if !cached.ensure_table(cached_class, bucket, collation, tz) {
            return FastOutcome::Fallback;
        }
    }
    let mut keys: Vec<Vec<(usize, u64)>> = Vec::with_capacity(joins.len());
    for &(probe_class, _, bucket) in &joins {
        match probe_keys(probe, &probe_classes, probe_class, bucket, collation, tz) {
            Some((found, _never)) => keys.push(found),
            None => return FastOutcome::Fallback,
        }
    }

    // Phase 2 — hash join. No hard error is possible from here on, so the first
    // confirmed true pair decides the comparison whatever its position.
    let cached: &CachedOperand = cached;
    for (join, &(_, cached_class, bucket)) in joins.iter().enumerate() {
        let Some(table) = cached.table(cached_class, bucket) else {
            return FastOutcome::Fallback;
        };
        for &(probe_index, hash) in &keys[join] {
            let mut slot = table.head.get(&hash).copied().unwrap_or(NO_SLOT);
            while slot != NO_SLOT {
                let cached_index = table.slots[slot as usize].0 as usize;
                let (left, right) = side.order(&probe[probe_index], &cached.values[cached_index]);
                match general_compare::confirm(context, left, right, collation, tz) {
                    Ok(true) => return FastOutcome::Decided(true),
                    Ok(false) => {}
                    // A class pair classified as comparable must not raise. That
                    // it did means the classification is wrong here, so decline
                    // instead of deciding.
                    Err(_) => return FastOutcome::Fallback,
                }
                slot = table.next[slot as usize];
            }
        }
    }

    // Phase 3 — no pair is true. Either nothing raises, or the deferred error
    // belongs to the first incomparable pair in row-major order.
    if incomparable.is_empty() {
        return FastOutcome::Decided(false);
    }
    let (left_classes, right_classes) =
        side.order(probe_classes.as_slice(), cached.classes.as_slice());
    let (left_values, right_values) = side.order(probe, cached.values.as_slice());
    match general_compare::first_incomparable_pair(left_classes, right_classes, &incomparable) {
        Some((i, j)) => {
            match general_compare::confirm(
                context,
                &left_values[i],
                &right_values[j],
                collation,
                tz,
            ) {
                Err(err) => FastOutcome::Raise(err),
                // The class pair was classified as always-raising, so this is
                // unreachable; falling back is the safe way to say so.
                Ok(_) => FastOutcome::Fallback,
            }
        }
        None => FastOutcome::Fallback,
    }
}

/// `probe != cached` (in `side`'s order), answered from the cached index.
///
/// `A != B` is true as soon as **one** pair does not compare equal, and §3.5.2
/// lets that pair decide the comparison on its own — as long as no *hard* error
/// sits earlier in row-major order. Phase 1 rules hard errors out for the whole
/// product exactly as it does for `=`; after that any unequal pair is an answer,
/// and one is found without walking the product:
///
/// * a value whose key is `Never` (`NaN`) is equal to nothing, so it makes any
///   pair it appears in unequal;
/// * two cached values with different key hashes cannot both equal a probe value,
///   so one of them differs from it;
/// * otherwise the bucket holds one key hash, and any probe value with another
///   hash differs from all of it.
///
/// The witness pair is then **confirmed** by the real comparison and has to come
/// back `Ok(false)`, so a wrong key can only cost the fast path, never the answer.
/// When every bucket is key-identical — the degenerate all-equal product — this
/// declines and the untouched path, which has its own linear treatment of that
/// case, takes over.
fn indexed_ne(
    context: &XPathContext,
    side: Side,
    cached: &mut CachedOperand,
    probe: &[XmlValue],
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
) -> FastOutcome {
    if probe.is_empty() || cached.values.is_empty() {
        return FastOutcome::Decided(false);
    }
    let Some(probe_classes) = general_compare::classify_all(probe) else {
        return FastOutcome::Fallback;
    };
    let probe_set = general_compare::distinct(&probe_classes);
    let Some((joins, _incomparable)) = class_pairs(side, cached, &probe_set) else {
        return FastOutcome::Fallback;
    };

    // Phase 1 — as for `=`: unless every key of the product can be computed, a
    // hard error could preempt the unequal pair this would report.
    for &(_, cached_class, bucket) in &joins {
        if !cached.ensure_table(cached_class, bucket, collation, tz) {
            return FastOutcome::Fallback;
        }
    }
    let mut keys: Vec<ProbeKeys> = Vec::with_capacity(joins.len());
    for &(probe_class, _, bucket) in &joins {
        match probe_keys(probe, &probe_classes, probe_class, bucket, collation, tz) {
            Some(entry) => keys.push(entry),
            None => return FastOutcome::Fallback,
        }
    }

    // Phase 2 — find one pair that provably does not compare equal.
    let cached: &CachedOperand = cached;
    for (join, &(_, cached_class, bucket)) in joins.iter().enumerate() {
        let Some(table) = cached.table(cached_class, bucket) else {
            return FastOutcome::Fallback;
        };
        let (found, never) = &keys[join];
        if found.is_empty() && never.is_empty() {
            continue;
        }
        let witness = if let Some(&cached_index) = table.never.first() {
            // A `NaN` on the cached side is unequal to every probe value.
            let probe_index = found
                .first()
                .map(|&(index, _)| index)
                .or_else(|| never.first().copied());
            probe_index.map(|probe_index| (probe_index, cached_index as usize))
        } else if let Some(&probe_index) = never.first() {
            // ... and a `NaN` on the probe side to every cached value.
            table.any().map(|cached_index| (probe_index, cached_index))
        } else {
            found.iter().find_map(|&(probe_index, hash)| {
                table
                    .any_other_than(hash)
                    .map(|cached_index| (probe_index, cached_index))
            })
        };
        let Some((probe_index, cached_index)) = witness else {
            continue;
        };
        let (left, right) = side.order(&probe[probe_index], &cached.values[cached_index]);
        match general_compare::confirm(context, left, right, collation, tz) {
            Ok(false) => return FastOutcome::Decided(true),
            // The keys said these cannot be equal, or the pair was classified as
            // comparable and raised anyway: either way the classification is
            // wrong here, so decline instead of deciding.
            Ok(true) | Err(_) => return FastOutcome::Fallback,
        }
    }

    FastOutcome::Fallback
}

#[cfg(test)]
#[path = "compare_cache_tests.rs"]
mod compare_cache_tests;
