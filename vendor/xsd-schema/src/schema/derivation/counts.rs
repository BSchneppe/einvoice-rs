//! Exact occurrence counts of a repeated particle term.
//!
//! A group `G{M,N}(X{m,n})` repeats `X`'s term `c` times for every `c` in
//! `⋃ₖ [k·m, k·n]`, `M ≤ k ≤ N` (Structures 1.1 §3.9.4.1.1). With `m ≥ 2`
//! consecutive intervals can leave counts out — one or two iterations of
//! exactly two give {2, 4}, never 3 — so folding the group into `X{M·m, N·n}`
//! fills gaps in. [`Counts`] keeps such a set exactly, so that a restriction
//! check can compare counts on both sides without the fill-in.
//!
//! **Arithmetic.** Counts are [`Count`] (`u128`) values up to [`LIMIT`]
//! (2¹²⁰), exact for every product of three `u32` bounds (`< 2⁹⁶`) and far
//! beyond. A set that needs a larger number is not built: the constructors
//! return `None`, and each caller keeps the group as written or uses an
//! approximation in its safe direction ([`Counts::repeated_over`] for the
//! restricting side, a smaller subset or none for the base). Every value a
//! comparison forms from sets within `LIMIT` stays below `8·LIMIT < 2¹²⁸`,
//! and every operation is checked besides, so nothing saturates or wraps.
//!
//! **Loops.** [`Counts::contains_range`] answers in at most four steps
//! (proof at the loop). [`Counts::within`] and the progression check walk
//! the separate runs of a set one by one; they stop after [`COUNT_STEPS`]
//! runs with "not shown", which rejects the restriction — sound, and only
//! reached by a per-iteration range `[p, q]` with `p > 2¹⁶·(q − p)` repeated
//! more than 2¹⁶ times.

/// One count of occurrences.
pub(super) type Count = u128;

/// The largest count a [`Counts`] holds (see the module documentation).
const LIMIT: Count = 1 << 120;

/// How many separate runs of a set [`Counts::within`] and the progression
/// check examine before they answer "not shown" (a conservative rejection).
const COUNT_STEPS: Count = 1 << 16;

/// A set of counts: the numbers of occurrences of a term that `k`
/// iterations produce, `a ≤ k ≤ b`, when each iteration produces `lo` to
/// `hi` of them — `⋃ₖ [k·lo, k·hi]`, with `k = 0` giving 0 and `None`
/// unbounded.
///
/// Every value is in one of three canonical shapes, which [`Counts::new`]
/// establishes: the empty set (`a = 1, b = 0, lo = hi = 1`); a contiguous
/// range `[a, b]` (`lo = hi = 1`, `a ≤ b`); or a set with gaps (`lo ≥ 2`,
/// `hi ≥ lo` or unbounded, two or more values of `k`). A zero per-iteration
/// maximum or no iterations at all give the range `[0, 0]`, never an
/// unbounded one. Every bound, `a·lo`, and the finite products `a·hi`,
/// `b·hi` are at most [`LIMIT`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Counts {
    a: Count,
    b: Option<Count>,
    lo: Count,
    hi: Option<Count>,
}

impl Counts {
    /// No count at all: a particle whose range is empty.
    const EMPTY: Counts = Counts {
        a: 1,
        b: Some(0),
        lo: 1,
        hi: Some(1),
    };

    /// The count 0 only.
    const ZERO: Counts = Counts {
        a: 0,
        b: Some(0),
        lo: 1,
        hi: Some(1),
    };

    /// Every count in `[min, max]`.
    pub(super) fn range(min: u32, max: Option<u32>) -> Self {
        Self::span(Count::from(min), max.map(Count::from)).expect("u32 bounds are within LIMIT")
    }

    /// The range `[min, max]`, or where it reaches beyond [`LIMIT`] a
    /// larger one (the minimum lowered to `LIMIT`, the maximum unbounded):
    /// for a restricting side's computed range, where admitting more is safe.
    pub(super) fn covering(min: Count, max: Option<Count>) -> Self {
        Self::span(min.min(LIMIT), max.filter(|&max| max <= LIMIT)).expect("within LIMIT")
    }

    /// Every count in `[min, max]`; `None` beyond [`LIMIT`].
    fn span(min: Count, max: Option<Count>) -> Option<Self> {
        if min > LIMIT || max.is_some_and(|max| max > LIMIT) {
            return None;
        }
        Some(match max {
            Some(max) if max < min => Self::EMPTY,
            _ => Counts {
                a: min,
                b: max,
                lo: 1,
                hi: Some(1),
            },
        })
    }

    /// `⋃ₖ [k·lo, k·hi]`, `a ≤ k ≤ b`, in its canonical shape; `None` when
    /// it needs a number beyond [`LIMIT`].
    fn new(a: Count, b: Option<Count>, lo: Count, hi: Option<Count>) -> Option<Self> {
        if b.is_some_and(|b| a > b) {
            return Some(Self::EMPTY);
        }
        // An iteration cannot produce anything: only k = 0 counts.
        if hi.is_some_and(|hi| lo > hi) {
            return Some(if a == 0 { Self::ZERO } else { Self::EMPTY });
        }
        // Zero stays zero: no iterations, or none of the term in each.
        if b == Some(0) || hi == Some(0) {
            return Some(Self::ZERO);
        }
        let min = a.checked_mul(lo)?;
        let max = match (b, hi) {
            (Some(b), Some(hi)) => Some(b.checked_mul(hi)?),
            _ => None,
        };
        // At most one of the term per iteration, one number of iterations,
        // or unbounded iterations from one on: one contiguous run.
        if lo <= 1 || b == Some(a) || hi.is_none() && a >= 1 {
            return Self::span(min, max);
        }
        let first_hi = match hi {
            Some(hi) => a.checked_mul(hi)?,
            None => 0,
        };
        if a > LIMIT
            || lo > LIMIT
            || min > LIMIT
            || first_hi > LIMIT
            || b.is_some_and(|b| b > LIMIT)
            || hi.is_some_and(|hi| hi > LIMIT)
            || max.is_some_and(|max| max > LIMIT)
        {
            return None;
        }
        let set = Counts { a, b, lo, hi };
        Some(if set.gapped_contains_range(min, max) {
            Self::span(min, max)?
        } else {
            set
        })
    }

    /// The counts of a term occurring `[p, q]` times per iteration over
    /// `{m, n}` iterations; with `empty_iteration` any iteration may produce
    /// none of it (an emptiable sibling branch takes it), so `k` starts at 0.
    #[cfg(test)]
    fn iterated(
        (m, n): (u32, Option<u32>),
        (p, q): (u32, Option<u32>),
        empty_iteration: bool,
    ) -> Self {
        Self::iterated_wide(
            (Count::from(m), n.map(Count::from)),
            (Count::from(p), q.map(Count::from)),
            empty_iteration,
        )
        .expect("u32 bounds")
    }

    fn iterated_wide(
        (m, n): (Count, Option<Count>),
        (p, q): (Count, Option<Count>),
        empty_iteration: bool,
    ) -> Option<Self> {
        Self::new(if empty_iteration || p == 0 { 0 } else { m }, n, p, q)
    }

    /// `self` repeated: the counts of a group around a particle with these
    /// counts, the group iterating `iterations` times; with
    /// `empty_iteration` an iteration may also produce none. `None` when the
    /// result is not one such set, or needs a number beyond [`LIMIT`].
    ///
    /// `self` is `⋃ⱼ [j·lo, j·hi]`, `a ≤ j ≤ b`: `j` units of `lo` to `hi`
    /// each. `K` iterations add up to `J` units for every `J` in
    /// `[K·a, K·b]`, so the repetition is `⋃ [J·lo, J·hi]` over the unit
    /// totals `T = ⋃ₖ [K·a, K·b]` — one set whenever `T` is a range (or
    /// `self` is: then the result is `T` itself).
    pub(super) fn repeated(self, iterations: Counts, empty_iteration: bool) -> Option<Self> {
        if self == Self::ZERO || iterations == Self::ZERO {
            return Some(Self::ZERO);
        }
        if self.is_unit() && !empty_iteration {
            return Some(iterations);
        }
        let Some((m, n)) = iterations.as_range() else {
            if empty_iteration {
                // Any of the iterations may be empty: the non-empty ones
                // number anything up to the most iterations.
                return self.repeated(Self::span(0, iterations.max())?, false);
            }
            if self.b.is_none() {
                // Unboundedly many units per iteration: K iterations give
                // every unit total from K·a on, so the totals are everything
                // from the least positive K's, and 0 when K = 0 is allowed.
                let least = iterations.without_zero();
                if least.is_empty() {
                    return Some(if iterations.contains(0) {
                        Self::ZERO
                    } else {
                        Self::EMPTY
                    });
                }
                let start = least.min().checked_mul(self.a)?;
                let totals = if iterations.contains(0) && start > 0 {
                    Self::new(0, None, start, None)?
                } else {
                    Self::span(start, None)?
                };
                if self.is_range() {
                    return Some(totals);
                }
                let (a, b) = totals.as_range()?;
                return Self::new(a, b, self.lo, self.hi);
            }
            return self.repeated_gapped(iterations);
        };
        let totals = Self::iterated_wide((m, n), (self.a, self.b), empty_iteration)?;
        if self.is_range() {
            return Some(totals);
        }
        let (a, b) = totals.as_range()?;
        Self::new(a, b, self.lo, self.hi)
    }

    /// [`Counts::repeated`], or where that is not one set a set that holds
    /// every count of the repetition and more: the unit totals `T` replaced
    /// by their hull `[min T, max T]`, or, where even that needs numbers
    /// beyond [`LIMIT`], every count from a lower bound of the repetition's
    /// least. Only for the restricting side, where admitting more is safe.
    pub(super) fn repeated_over(self, iterations: Counts, empty_iteration: bool) -> Self {
        if let Some(exact) = self.repeated(iterations, empty_iteration) {
            return exact;
        }
        let least = if empty_iteration { 0 } else { iterations.min() };
        let hull = Self::iterated_wide((least, iterations.max()), (self.a, self.b), false)
            .and_then(|totals| Self::new(totals.min(), totals.max(), self.lo, self.hi));
        hull.unwrap_or_else(|| {
            // Every count of the repetition is at least least·min(self).
            let floor = least.saturating_mul(self.min()).min(LIMIT);
            Self::span(floor, None).expect("within LIMIT")
        })
    }

    /// [`Counts::repeated`], or where that is not one set the largest of a
    /// few sets that hold only counts of the repetition: without the zero
    /// iterations, over the contiguous tail of `iterations`, or one iteration.
    /// Only for the base side, where admitting less is safe. `None` when none
    /// applies.
    pub(super) fn repeated_under(self, iterations: Counts, empty_iteration: bool) -> Option<Self> {
        if let Some(exact) = self.repeated(iterations, empty_iteration) {
            return Some(exact);
        }
        let positive = iterations.without_zero();
        if let Some(subset) = self.repeated(positive, false) {
            return Some(subset);
        }
        // A progression never runs together: no tail is contiguous.
        if !positive.is_progression() {
            let tail = Self::span(
                positive.contiguous_from().max(positive.min()),
                positive.max(),
            );
            if let Some(subset) = tail
                .filter(|tail| !tail.is_empty())
                .and_then(|tail| self.repeated(tail, false))
            {
                return Some(subset);
            }
        }
        iterations.contains(1).then_some(self)
    }

    /// A range `[p, q]` repeated a gapped number of times `K ∈ I`,
    /// `I = ⋃ⱼ [j·L, j·H]`: `⋃ [K·p, K·q]`, which is `⋃ⱼ [j·L·p, j·H·q]`
    /// when the `K` of each `j` give touching intervals — `p ≤ 1`, one `K`
    /// per `j` (`L = H`), or every `K` past the point where consecutive
    /// intervals touch. `None` otherwise, when `self` has gaps too, or beyond
    /// [`LIMIT`].
    fn repeated_gapped(self, iterations: Counts) -> Option<Self> {
        let (p, q) = self.as_range()?;
        let Counts { a, b, lo, hi } = iterations;
        let first_k = a.max(1).checked_mul(lo)?;
        let touching = p <= 1
            || hi == Some(lo)
            || q.is_none_or(|q| q > p && first_k >= (p - 1).div_ceil(q - p));
        if !touching {
            return None;
        }
        let hi = match (hi, q) {
            (Some(0), _) | (_, Some(0)) => Some(0),
            (Some(hi), Some(q)) => Some(hi.checked_mul(q)?),
            _ => None,
        };
        Self::new(a, b, lo.checked_mul(p)?, hi)
    }

    /// `[min T, max T]` of the repetition of `self` over `iterations`
    /// (`empty_iteration`: from 0): the counts of an emptiable term, whose
    /// language depends only on the largest (`L(T)^c ⊆ L(T)^{c+1}`). `None`
    /// beyond [`LIMIT`].
    pub(super) fn repeated_hull(self, iterations: Counts, empty_iteration: bool) -> Option<Self> {
        let min = if empty_iteration {
            0
        } else {
            self.min().checked_mul(iterations.min())?
        };
        let max = match (self.max(), iterations.max()) {
            (Some(0), _) | (_, Some(0)) => Some(0),
            (Some(x), Some(y)) => Some(x.checked_mul(y)?),
            _ => None,
        };
        Self::span(min, max)
    }

    /// The smallest count (1 for the empty set).
    pub(super) fn min(&self) -> Count {
        // a·lo ≤ LIMIT by the invariant.
        self.a * self.lo
    }

    /// The largest count (`None`: unbounded; 0 for the empty set).
    pub(super) fn max(&self) -> Option<Count> {
        match (self.b, self.hi) {
            (Some(0), _) | (_, Some(0)) => Some(0),
            // b·hi ≤ LIMIT by the invariant.
            (Some(b), Some(hi)) => Some(b * hi),
            _ => None,
        }
    }

    /// `[min, max]` as `u32` occurrence bounds, saturated
    /// ([`Counts::is_u32_range`] tells whether they are exact).
    pub(super) fn hull(&self) -> (u32, Option<u32>) {
        let clamp = |v: Count| u32::try_from(v).unwrap_or(u32::MAX);
        (clamp(self.min()), self.max().map(clamp))
    }

    /// Every count in `[min, max]`, both within `u32`: the set is exactly
    /// the occurrence range [`Counts::hull`] gives.
    pub(super) fn is_u32_range(&self) -> bool {
        let fits = |v: Count| v <= Count::from(u32::MAX);
        self.is_range() && fits(self.min()) && self.max().is_none_or(fits)
    }

    /// The range `[0, max]`: the counts of an emptiable base term as far as
    /// its language goes.
    pub(super) fn up_to_max(&self) -> Self {
        Self::span(0, self.max()).expect("within LIMIT")
    }

    /// `Some((min, max))` when the set is one contiguous range.
    pub(super) fn as_range(&self) -> Option<(Count, Option<Count>)> {
        self.is_range().then_some((self.a, self.b))
    }

    /// The hull `[min, max]` as a set.
    pub(super) fn hull_set(&self) -> Self {
        if self.is_empty() {
            return *self;
        }
        Self::span(self.min(), self.max()).expect("within LIMIT")
    }

    fn is_range(&self) -> bool {
        self.lo == 1 && self.hi == Some(1)
    }

    fn is_unit(&self) -> bool {
        *self == Self::range(1, Some(1))
    }

    fn is_empty(&self) -> bool {
        *self == Self::EMPTY
    }

    /// The set without the count 0 (the non-empty sequences).
    pub(super) fn without_zero(self) -> Self {
        // Raising `a` to 1 keeps every product within the invariant.
        Self::new(self.a.max(1), self.b, self.lo, self.hi).expect("within LIMIT")
    }

    /// Every iteration contributes exactly `lo ≥ 2`: the counts `k·lo` never
    /// run together.
    fn is_progression(&self) -> bool {
        self.hi == Some(self.lo) && self.lo >= 2
    }

    pub(super) fn contains(&self, count: Count) -> bool {
        self.contains_range(count, Some(count))
    }

    /// Is every count in `[r, s]` in the set?
    pub(super) fn contains_range(&self, r: Count, s: Option<Count>) -> bool {
        if s.is_some_and(|s| s < r) {
            return true;
        }
        if self.is_empty() {
            return false;
        }
        if self.is_range() {
            return self.a <= r
                && match (s, self.b) {
                    (_, None) => true,
                    (Some(s), Some(b)) => s <= b,
                    (None, Some(_)) => false,
                };
        }
        self.gapped_contains_range(r, s)
    }

    /// [`Counts::contains_range`] for the shape with gaps (`lo ≥ 2`).
    ///
    /// The intervals `[k·p, k·q]` are swept in increasing `k` (their starts
    /// never decrease), skipping to the first one that reaches the smallest
    /// uncovered count `need`. **At most four steps:** a step that skips
    /// lands on an interval reaching `need`; a step that covers raises
    /// `need` to `k·q + 1` and then either the query is covered, or
    /// consecutive intervals touch from here on (answered at once), or they
    /// do not — then `(k + 1)·p > k·q + 1 = need` (were `(k + 1)·p ≤ k·q + 1`
    /// with `q ≤ p`, `p` would be at most 1), so the next step starts beyond
    /// `need` and answers `false`. The loop is bounded by eight steps
    /// besides, answering `false`. With `r, s ≤ LIMIT` (the callers' counts)
    /// every value formed is below `8·LIMIT`; the operations are checked,
    /// an overflow answering `false` (a smaller set: safe for a base).
    fn gapped_contains_range(&self, r: Count, s: Option<Count>) -> bool {
        let Counts {
            b: n, lo: p, hi: q, ..
        } = *self;
        let mut k = self.a;
        let mut need = r;
        for _ in 0..8 {
            if n.is_some_and(|n| k > n) {
                return false;
            }
            let Some(start) = k.checked_mul(p) else {
                return false;
            };
            if start > need {
                // Later intervals start later still.
                return false;
            }
            let end = match q {
                _ if k == 0 => 0,
                None => return true,
                Some(q) => match k.checked_mul(q) {
                    Some(end) => end,
                    None => return false,
                },
            };
            if end >= need {
                need = end.saturating_add(1);
            }
            if s.is_some_and(|s| need > s) {
                return true;
            }
            // [k·p, k·q] and [(k+1)·p, …] touch, and the gap between
            // consecutive intervals, p − 1 − k·(q − p), only shrinks as k
            // grows: every later interval touches too, so all counts up to
            // n·q are covered.
            if let Some(q) = q {
                let next_start = k.checked_add(1).and_then(|next| next.checked_mul(p));
                if k >= 1 && q > p && next_start.is_some_and(|next| next <= end.saturating_add(1)) {
                    return match (n, s) {
                        (None, _) => true,
                        // n·q ≤ LIMIT by the invariant.
                        (Some(n), Some(s)) => s <= n * q,
                        (Some(_), None) => false,
                    };
                }
            }
            // Intervals that end below `need` cannot help: skip to the first
            // one that reaches it.
            k = match q {
                Some(q) => k.saturating_add(1).max(need.div_ceil(q)),
                None => k.saturating_add(1),
            };
        }
        false
    }

    /// The first count from which a set that is not a progression holds
    /// every count up to its maximum: the start of the first interval from
    /// which consecutive intervals touch (or of the last one). Saturates
    /// (past [`LIMIT`] it is only compared).
    fn contiguous_from(&self) -> Count {
        if self.is_range() {
            return self.a;
        }
        let first = self.a.max(1);
        let touching = match self.hi {
            Some(hi) if hi > self.lo => (self.lo - 1).div_ceil(hi - self.lo),
            _ => 1,
        };
        let k = first.max(touching);
        let k = self.b.map_or(k, |b| k.min(b));
        k.saturating_mul(self.lo)
    }

    /// Is every count `j·step`, `from ≤ j ≤ to`, in the set (`step ≥ 2`)?
    fn contains_progression(&self, from: Count, to: Option<Count>, step: Count) -> bool {
        if to.is_some_and(|to| from > to) {
            return true;
        }
        // from·step and to·step are the least and greatest of a set within
        // LIMIT.
        let (least, greatest) = (from * step, to.map(|to| to * step));
        if self.is_range() || self.is_empty() {
            return self.contains_range(least, greatest);
        }
        if self.is_progression() {
            // The set is {k·p : a ≤ k ≤ b}.
            let p = self.lo;
            if to == Some(from) {
                return least.checked_rem(p) == Some(0)
                    && least / p >= self.a
                    && self.b.is_none_or(|b| least / p <= b);
            }
            // Two consecutive counts differ by `step`, so `p` divides it; the
            // quotients then run from `from·step/p` to `to·step/p`.
            if step.checked_rem(p) != Some(0) {
                return false;
            }
            return least / p >= self.a
                && match (greatest, self.b) {
                    (_, None) => true,
                    (Some(greatest), Some(b)) => greatest / p <= b,
                    (None, Some(_)) => false,
                };
        }
        let contiguous = self.contiguous_from();
        let mut j = from;
        // At most COUNT_STEPS counts below the contiguous tail, one by one.
        while j.saturating_mul(step) < contiguous {
            if to.is_some_and(|to| j > to) {
                return true;
            }
            if j - from >= COUNT_STEPS || !self.contains(j * step) {
                return false;
            }
            j += 1;
        }
        if to.is_some_and(|to| j > to) {
            return true;
        }
        // The remaining counts are all past `contiguous`: inside iff the
        // largest is at most the maximum.
        match (greatest, self.max()) {
            (_, None) => true,
            (Some(greatest), Some(max)) => greatest <= max,
            (None, Some(_)) => false,
        }
    }

    /// Is every count of `self` in `other`?
    pub(super) fn within(&self, other: &Counts) -> bool {
        if self.is_empty() || self == other {
            return true;
        }
        if other.is_empty() {
            return false;
        }
        if self.is_range() {
            return other.contains_range(self.a, self.b);
        }
        // Each interval [k·lo, k·hi] of `self` inside the one of `other`
        // for the same k, over fewer iterations.
        let at_most = |x: Option<Count>, y: Option<Count>| match (x, y) {
            (_, None) => true,
            (Some(x), Some(y)) => x <= y,
            (None, Some(_)) => false,
        };
        if !other.is_range()
            && self.lo >= other.lo
            && at_most(self.hi, other.hi)
            && self.a >= other.a
            && at_most(self.b, other.b)
        {
            return true;
        }
        if other.is_range() {
            return other.contains_range(self.min(), self.max());
        }
        if self.is_progression() {
            return other.contains_progression(self.a, self.b, self.lo);
        }
        // Not a progression: the intervals [k·lo, k·hi] touch from some k on
        // (from k ≥ (lo − 1)/(hi − lo)), and from there they are one run.
        // The runs before it, one by one, at most COUNT_STEPS of them.
        let mut k = self.a;
        loop {
            if self.b.is_some_and(|b| k > b) {
                return true;
            }
            // Past LIMIT nothing is shown: "not shown", a rejection.
            let Some(start) = k.checked_mul(self.lo) else {
                return false;
            };
            let end = match self.hi {
                _ if k == 0 => Some(0),
                Some(hi) => match k.checked_mul(hi) {
                    Some(end) => Some(end),
                    None => return false,
                },
                None => None,
            };
            let touching = k >= 1
                && end.is_none_or(|end| {
                    k.checked_add(1)
                        .and_then(|next| next.checked_mul(self.lo))
                        .is_some_and(|next| next <= end.saturating_add(1))
                });
            if touching {
                return other.contains_range(start, self.max());
            }
            if k - self.a >= COUNT_STEPS || !other.contains_range(start, end) {
                return false;
            }
            k += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Counts;

    /// Brute force: can `k` iterations, `m ≤ k ≤ n`, produce exactly `c`
    /// occurrences when each iteration through the branch gives `[p, q]` and
    /// (with `empty`) any iteration may give none?
    fn producible(
        (m, n): (u32, Option<u32>),
        (p, q): (u32, Option<u32>),
        empty: bool,
        c: u64,
    ) -> bool {
        // Every non-empty iteration gives at least one occurrence once p ≥ 1,
        // and empty ones add nothing new, so k beyond max(m, c) + 1 adds no
        // count.
        let k_max = n.map_or(u64::from(m).max(c) + 1, u64::from);
        (u64::from(m)..=k_max).any(|k| {
            // `j` of the `k` iterations go through the branch.
            let j_min = if empty || p == 0 { 0 } else { k };
            (j_min..=k).any(|j| {
                let lo = j * u64::from(p);
                let hi = if j == 0 {
                    Some(0)
                } else {
                    q.map(|q| j * u64::from(q))
                };
                lo <= c && hi.is_none_or(|hi| c <= hi)
            })
        })
    }

    /// The test of a choice branch's counts: is every count in `[r, s]` one
    /// that `{m, n}` iterations through a branch `{p, q}` produce?
    fn branch_counts_cover(
        m_n: (u32, Option<u32>),
        p_q: (u32, Option<u32>),
        empty: bool,
        (r, s): (u32, Option<u32>),
    ) -> bool {
        Counts::iterated(m_n, p_q, empty).contains_range(u128::from(r), s.map(u128::from))
    }

    /// `Counts::contains_range` agrees with the brute force on every small
    /// range.
    ///
    /// The horizon `H = 60` stands in for an unbounded derived maximum. With
    /// every bound at most 5 a failing count, if any, is below it: a bounded
    /// `n·q` is at most 25; an unbounded `q` makes every count from `k·p` on
    /// producible; and with `q` bounded and `n` unbounded the intervals
    /// `[k·p, k·q]` touch from `k = 4` on when `q > p`, while for `q = p ≥ 2`
    /// the count `k·p + 1` is missing for every `k`.
    #[test]
    fn contains_range_matches_brute_force() {
        const H: u64 = 60;
        let bounds = |from: u32| (from..=5).map(Some).chain([None]).collect::<Vec<_>>();
        let (mut checked, mut covered) = (0u64, 0u64);
        for m in 0..=3u32 {
            for n in bounds(m.max(1)) {
                for p in 0..=3u32 {
                    for q in bounds(p.max(1)) {
                        for empty in [false, true] {
                            for r in 0..=8u32 {
                                for s in bounds(r) {
                                    let top = s.map_or(H, u64::from);
                                    let expected = (u64::from(r)..=top)
                                        .all(|c| producible((m, n), (p, q), empty, c));
                                    let got = branch_counts_cover((m, n), (p, q), empty, (r, s));
                                    assert_eq!(
                                        got, expected,
                                        "choice {{{m},{n:?}}}, branch {{{p},{q:?}}}, \
                                         empty iterations {empty}, derived {{{r},{s:?}}}"
                                    );
                                    checked += 1;
                                    covered += u64::from(got);
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(covered > checked / 10 && covered < checked * 9 / 10);
    }

    /// Bounds near `u32::MAX` take a few steps and do not overflow.
    #[test]
    fn contains_range_large_bounds() {
        let max = u32::MAX;
        // choice{1,max}(a{1,3}, b?) restricted to a{0,max}: every count.
        assert!(branch_counts_cover(
            (1, Some(max)),
            (1, Some(3)),
            true,
            (0, Some(max))
        ));
        // Without the empty iteration the count 0 is not producible.
        assert!(!branch_counts_cover(
            (1, Some(max)),
            (1, Some(3)),
            false,
            (0, Some(max))
        ));
        // choice{0,max}(a{max,max}): only multiples of max.
        assert!(!branch_counts_cover(
            (0, Some(max)),
            (max, Some(max)),
            false,
            (1, Some(1))
        ));
        assert!(branch_counts_cover(
            (0, Some(max)),
            (max, Some(max)),
            false,
            (max, Some(max))
        ));
        assert!(!branch_counts_cover(
            (0, Some(max)),
            (max, Some(max)),
            false,
            (max, None)
        ));
        // choice{0,∞}(a{2,2}): even counts only, unbounded.
        assert!(!branch_counts_cover(
            (0, None),
            (2, Some(2)),
            false,
            (0, None)
        ));
        // choice{max,max}(a{1,max}) restricted to a{max, max·max} up to u32.
        assert!(branch_counts_cover(
            (max, Some(max)),
            (1, Some(max)),
            false,
            (max, Some(max))
        ));
        // choice{1,max}(a{2,3}): every count from 2 on, but not 1.
        assert!(branch_counts_cover(
            (1, Some(max)),
            (2, Some(3)),
            false,
            (2, Some(max))
        ));
        assert!(!branch_counts_cover(
            (1, Some(max)),
            (2, Some(3)),
            false,
            (1, Some(max))
        ));
    }

    /// `Counts::within` — one exact set of counts inside another, gaps on
    /// either side — agrees with the brute force on every pair of small sets.
    ///
    /// Each set is compared as a bitset of its counts up to `H = 96`. With
    /// every bound at most 4 (or unbounded) each set is, from count 16 on,
    /// empty, an unbounded run, or periodic with a period of at most 3, so two
    /// sets that agree on inclusion up to 96 agree everywhere.
    #[test]
    fn within_matches_brute_force() {
        const H: u64 = 96;
        let bounds = |from: u32| (from..=4).map(Some).chain([None]).collect::<Vec<_>>();
        let mut sets = Vec::new();
        for m in 0..=2u32 {
            for n in bounds(m.max(1)) {
                for p in 0..=3u32 {
                    for q in bounds(p.max(1)) {
                        for empty in [false, true] {
                            let bits: Vec<bool> = (0..=H)
                                .map(|c| producible((m, n), (p, q), empty, c))
                                .collect();
                            sets.push((Counts::iterated((m, n), (p, q), empty), bits));
                        }
                    }
                }
            }
        }
        let (mut checked, mut inside) = (0u64, 0u64);
        for (derived, derived_bits) in &sets {
            for (base, base_bits) in &sets {
                let expected = derived_bits.iter().zip(base_bits).all(|(d, b)| !d || *b);
                assert_eq!(
                    derived.within(base),
                    expected,
                    "{derived:?} within {base:?}"
                );
                checked += 1;
                inside += u64::from(expected);
            }
            // The canonical shape holds the brute force's counts, and is a
            // range exactly when they are contiguous.
            for (c, &member) in derived_bits.iter().enumerate() {
                assert_eq!(derived.contains(c as u128), member, "{derived:?}: {c}");
            }
            let top = derived.max().map_or(H, |max| max.min(u128::from(H)) as u64);
            let low = derived.min() as u64;
            let contiguous = (low..=top).all(|c| derived_bits[c as usize]);
            assert_eq!(derived.as_range().is_some(), contiguous, "{derived:?}");
        }
        assert!(inside > checked / 20 && inside < checked * 19 / 20);
    }

    /// `repeated` agrees with the brute force where it answers.
    #[test]
    fn repeated_matches_brute_force() {
        let bounds = |from: u32| (from..=3).map(Some).chain([None]).collect::<Vec<_>>();
        for p in 0..=3u32 {
            for q in bounds(p.max(1)) {
                for m in 0..=2u32 {
                    for n in bounds(m.max(1)) {
                        for empty in [false, true] {
                            let inner = Counts::range(p, q);
                            let outer = inner
                                .repeated(Counts::range(m, n), empty)
                                .expect("a range repeats");
                            for c in 0..=40u64 {
                                assert_eq!(
                                    outer.contains(c.into()),
                                    producible((m, n), (p, q), empty, c),
                                    "{{{p},{q:?}}} repeated {{{m},{n:?}}} empty {empty}: {c}"
                                );
                            }
                        }
                    }
                }
            }
        }
        // A set with gaps repeated: {2, 4} once or twice is {2, 4, 6, 8};
        // twice exactly is {4, 6, 8}; once or not at all is {0, 2, 4}.
        let gapped = Counts::iterated((1, Some(2)), (2, Some(2)), false);
        let expect = |set: Counts, members: &[u64], horizon: u64| {
            for c in 0..=horizon {
                assert_eq!(set.contains(c.into()), members.contains(&c), "{set:?}: {c}");
            }
        };
        expect(
            gapped.repeated(Counts::range(1, Some(1)), false).unwrap(),
            &[2, 4],
            12,
        );
        expect(
            gapped.repeated(Counts::range(1, Some(2)), false).unwrap(),
            &[2, 4, 6, 8],
            12,
        );
        expect(
            gapped.repeated(Counts::range(2, Some(2)), false).unwrap(),
            &[4, 6, 8],
            12,
        );
        expect(
            gapped.repeated(Counts::range(0, Some(1)), false).unwrap(),
            &[0, 2, 4],
            12,
        );
        // Multiples of 3, repeated twice: still the multiples of 3.
        let threes = Counts::iterated((0, None), (3, Some(3)), false);
        let twice = threes.repeated(Counts::range(2, Some(2)), false).unwrap();
        assert!(twice.within(&threes) && threes.within(&twice));
        // A unit term takes the iterations' counts, gaps and all.
        assert_eq!(
            Counts::range(1, Some(1)).repeated(gapped, false),
            Some(gapped)
        );
        // A range repeated a gapped number of times, against a brute force.
        let mut answered = 0;
        for (ia, ib, il, ih) in [
            (0, 1, 2, 3),
            (1, 2, 2, 2),
            (0, 3, 3, 3),
            (2, 3, 2, 4),
            (0, 2, 2, 3),
        ] {
            let iterations = Counts::iterated((ia, Some(ib)), (il, Some(ih)), false);
            for p in 0..=3u32 {
                for q in (p.max(1)..=4).map(Some).chain([None]) {
                    let Some(outer) = Counts::range(p, q).repeated(iterations, false) else {
                        continue;
                    };
                    answered += 1;
                    for c in 0..=60u64 {
                        let brute =
                            (0..=24u64)
                                .filter(|&k| iterations.contains(k.into()))
                                .any(|k| {
                                    // k iterations of [p, q] give [k·p, k·q]; none give 0.
                                    k * u64::from(p) <= c
                                        && q.is_none_or(|q| c <= k * u64::from(q))
                                        && (k > 0 || c == 0)
                                });
                        assert_eq!(
                            outer.contains(c.into()),
                            brute,
                            "[{p},{q:?}] x {iterations:?}: {c}"
                        );
                    }
                }
            }
        }
        assert!(answered > 40, "answered {answered}");
        // `repeated_under` holds only counts of the repetition.
        for (sa, sb, lo, hi) in [(1, 1, 2, 2), (2, 3, 2, 2), (1, 3, 1, 4)] {
            let inner = Counts::iterated((sa, Some(sb)), (lo, Some(hi)), false);
            let members: Vec<u64> = (0..=24).filter(|&x| inner.contains(x.into())).collect();
            for iterations in [
                Counts::iterated((0, Some(1)), (2, None), false),
                Counts::iterated((0, Some(1)), (2, Some(3)), false),
                Counts::iterated((1, Some(3)), (2, Some(2)), false),
            ] {
                let Some(under) = inner.repeated_under(iterations, false) else {
                    continue;
                };
                // Every member is at least 1, so counts up to 40 need at most
                // 40 iterations.
                let mut reachable = [false; 121];
                for k in (0..=40u64).filter(|&k| iterations.contains(k.into())) {
                    let mut sums = vec![0u64];
                    for _ in 0..k {
                        sums = sums
                            .iter()
                            .flat_map(|v| members.iter().map(move |x| v + x))
                            .filter(|&v| v <= 120)
                            .collect();
                        sums.sort_unstable();
                        sums.dedup();
                    }
                    for c in sums {
                        reachable[c as usize] = true;
                    }
                }
                for c in 0..=40u64 {
                    assert!(
                        !under.contains(c.into()) || reachable[c as usize],
                        "{inner:?} x {iterations:?}: {c}"
                    );
                }
            }
        }
        // `repeated_over` holds every count of the repetition.
        for (sa, sb, lo, hi) in [(1, 2, 2, 2), (2, 3, 2, 2), (0, 2, 3, 3), (1, 3, 2, 3)] {
            let inner = Counts::iterated((sa, Some(sb)), (lo, Some(hi)), false);
            let members: Vec<u64> = (0..=24).filter(|&x| inner.contains(x.into())).collect();
            for iterations in [
                Counts::range(0, None),
                Counts::range(2, Some(3)),
                Counts::iterated((0, Some(1)), (2, Some(3)), false),
            ] {
                let over = inner.repeated_over(iterations, false);
                for k in (0..=6u64).filter(|&k| iterations.contains(k.into())) {
                    let mut sums = vec![0u64];
                    for _ in 0..k {
                        sums = sums
                            .iter()
                            .flat_map(|v| members.iter().map(move |x| v + x))
                            .collect();
                        sums.sort_unstable();
                        sums.dedup();
                    }
                    for c in sums {
                        assert!(over.contains(c.into()), "{inner:?} x {iterations:?}: {c}");
                    }
                }
            }
        }
        // Brute force over small gapped sets repeated over small ranges: every
        // repetition `repeated` writes as one set is that set.
        for (sa, sb, lo, hi) in [
            (1, 2, 2, 2),
            (0, 2, 3, 3),
            (1, 3, 2, 3),
            (2, 2, 2, 3),
            (0, 1, 2, 4),
        ] {
            let inner = Counts::iterated((sa, Some(sb)), (lo, Some(hi)), false);
            for m in 0..=2u32 {
                for n in [m.max(1), m + 2] {
                    if let Some(outer) = inner.repeated(Counts::range(m, Some(n)), false) {
                        for c in 0..=60u64 {
                            // c is a sum of k members of `inner`, m ≤ k ≤ n.
                            let members: Vec<u64> =
                                (0..=24).filter(|&x| inner.contains(x.into())).collect();
                            let brute = (m..=n).any(|k| {
                                let mut sums = vec![false; 61];
                                sums[0] = true;
                                for _ in 0..k {
                                    let mut next = vec![false; 61];
                                    for v in 0..=60usize {
                                        if sums[v] {
                                            for &x in &members {
                                                if v as u64 + x <= 60 {
                                                    next[v + x as usize] = true;
                                                }
                                            }
                                        }
                                    }
                                    sums = next;
                                }
                                sums[c as usize]
                            });
                            assert_eq!(
                                outer.contains(c.into()),
                                brute,
                                "{inner:?} repeated {{{m},{n}}}: {c}"
                            );
                        }
                    }
                }
            }
        }
    }

    /// Gapped sets: progressions, gaps that close, bounds near `u32::MAX`.
    #[test]
    fn within_examples() {
        let set = |m, n, p, q| Counts::iterated((m, n), (p, q), false);
        let max = u32::MAX;
        // {2, 4} within {2, 4} and within {2, 4, 6}, not within [2, 4]'s gap.
        assert!(set(1, Some(2), 2, Some(2)).within(&set(1, Some(2), 2, Some(2))));
        assert!(set(1, Some(2), 2, Some(2)).within(&set(1, Some(3), 2, Some(2))));
        assert!(!Counts::range(3, Some(3)).within(&set(1, Some(2), 2, Some(2))));
        assert!(!set(1, Some(2), 2, Some(4)).within(&set(1, Some(2), 2, Some(2))));
        // Multiples of 4 within the even counts, unbounded; not the reverse.
        assert!(set(0, None, 4, Some(4)).within(&set(0, None, 2, Some(2))));
        assert!(!set(0, None, 2, Some(2)).within(&set(0, None, 4, Some(4))));
        // Multiples of 3 from 3 on within [2, 3] ∪ [4, 6] ∪ … = every count from 2.
        assert!(set(1, None, 3, Some(3)).within(&set(1, None, 2, Some(3))));
        // Large bounds take a few steps.
        assert!(set(1, Some(max), 2, Some(2)).within(&set(1, Some(max), 2, Some(2))));
        assert!(set(0, Some(max), 1, Some(3)).within(&set(0, None, 1, Some(1))));
        assert!(!set(1, Some(max), 2, Some(2)).within(&set(1, Some(max - 1), 2, Some(2))));
        assert!(set(1, Some(max / 2), 4, Some(4)).within(&set(1, Some(max), 2, Some(2))));
        assert!(!set(1, Some(max), 4, Some(4)).within(&set(1, Some(max), 2, Some(2))));
        assert!(!set(1, Some(max), 4, Some(4)).within(&set(1, Some(max - 1), 2, Some(3))));
    }

    /// Sets repeated a gapped number of times, with and without empty
    /// iterations: wherever `repeated` answers, the answer is the brute
    /// force's set of sums.
    #[test]
    fn repeated_over_gapped_iterations_matches_brute_force() {
        let inners = [
            Counts::range(0, None),
            Counts::range(2, None),
            Counts::range(1, Some(3)),
            Counts::iterated((1, None), (2, Some(2)), false),
            Counts::iterated((0, None), (3, Some(3)), false),
            Counts::iterated((2, None), (2, Some(3)), false),
        ];
        let iteration_sets = [
            Counts::iterated((1, Some(2)), (2, Some(2)), false),
            Counts::iterated((0, Some(3)), (3, Some(3)), false),
            Counts::iterated((1, None), (2, Some(2)), false),
            Counts::iterated((0, Some(2)), (2, Some(3)), false),
        ];
        const H: u64 = 60;
        let mut answered = 0;
        for inner in inners {
            let members: Vec<u64> = (0..=H).filter(|&x| inner.contains(x.into())).collect();
            for iterations in iteration_sets {
                for empty in [false, true] {
                    let Some(outer) = inner.repeated(iterations, empty) else {
                        continue;
                    };
                    answered += 1;
                    // reachable[k][c]: k iterations can sum to c.
                    let mut reachable = vec![false; H as usize + 1];
                    let mut sums = vec![false; H as usize + 1];
                    sums[0] = true;
                    for k in 0..=H {
                        if iterations.contains(k.into()) {
                            for c in 0..=H as usize {
                                reachable[c] |= sums[c];
                            }
                        }
                        let mut next = vec![false; H as usize + 1];
                        for c in 0..=H as usize {
                            if !sums[c] {
                                continue;
                            }
                            if empty {
                                next[c] = true;
                            }
                            for &x in &members {
                                if c + x as usize <= H as usize {
                                    next[c + x as usize] = true;
                                }
                            }
                        }
                        sums = next;
                    }
                    for c in 0..=H {
                        assert_eq!(
                            outer.contains(c.into()),
                            reachable[c as usize],
                            "{inner:?} x {iterations:?} empty {empty}: {c}"
                        );
                    }
                }
            }
        }
        assert!(answered >= 30, "answered {answered}");
    }

    /// A per-iteration maximum of 0, or no iterations, gives the count 0
    /// however the group repeats — never an unbounded set.
    #[test]
    fn zero_stays_zero() {
        let zero = Counts::range(0, Some(0));
        assert_eq!(zero, Counts::ZERO);
        assert_eq!(zero.max(), Some(0));
        for iterations in [
            Counts::range(1, None),
            Counts::range(0, None),
            Counts::range(3, Some(7)),
            Counts::iterated((1, None), (2, Some(2)), false),
        ] {
            for empty in [false, true] {
                assert_eq!(zero.repeated(iterations, empty), Some(Counts::ZERO));
                assert_eq!(zero.repeated_over(iterations, empty), Counts::ZERO);
                assert_eq!(zero.repeated_under(iterations, empty), Some(Counts::ZERO));
                assert_eq!(
                    Counts::range(2, Some(5)).repeated(Counts::ZERO, empty),
                    Some(Counts::ZERO)
                );
            }
        }
        assert_eq!(
            Counts::iterated((1, None), (0, Some(0)), false),
            Counts::ZERO
        );
        assert_eq!(Counts::new(5, None, 0, Some(0)), Some(Counts::ZERO));
        // An empty per-iteration range: only no iterations would count.
        assert_eq!(Counts::new(5, None, 3, Some(0)), Some(Counts::EMPTY));
        assert_eq!(Counts::new(0, Some(0), 3, None), Some(Counts::ZERO));
        assert!(!Counts::ZERO.contains(1));
        assert!(!Counts::range(1, Some(1)).within(&Counts::ZERO));
    }

    /// Products far beyond `u64` stay exact: `choice{G,∞}(choice{G,∞}(a{G}))`
    /// is the multiples of `G` from `G³` on.
    #[test]
    fn large_products_are_exact() {
        let g: u128 = 3_000_000;
        let from_g = Counts::range(3_000_000, None);
        let inner = Counts::range(3_000_000, Some(3_000_000))
            .repeated(from_g, false)
            .unwrap();
        let outer = inner.repeated(from_g, false).unwrap();
        assert_eq!(outer.min(), g * g * g);
        assert_eq!(outer.max(), None);
        assert!(outer.contains(g * g * g) && outer.contains(g * g * g + g));
        assert!(!outer.contains(g * g * g - g) && !outer.contains(g * g * g + 1));
        assert!(outer.within(&outer) && outer.within(&inner) && !inner.within(&outer));
        assert!(!Counts::range(u32::MAX, Some(u32::MAX)).within(&outer));
        // Three u32::MAX factors: exact; a fourth leaves LIMIT.
        let m = Counts::range(u32::MAX, Some(u32::MAX));
        let m2 = m.repeated(m, false).unwrap();
        let m3 = m.repeated(m2, false).unwrap();
        assert_eq!(m3.min(), u128::from(u32::MAX).pow(3));
        assert_eq!(m.repeated(m3, false), None);
        let over = m.repeated_over(m3, false);
        assert!(over.min() <= u128::from(u32::MAX).pow(4) && over.max().is_none());
    }

    /// Every comparison of sets with large, gapped or unbounded parameters
    /// answers, and quickly: a grid of shapes compared pairwise.
    #[test]
    fn large_shapes_answer_quickly() {
        let started = std::time::Instant::now();
        let m = u128::from(u32::MAX);
        let g = 3_000_000u128;
        let values = [0, 1, 2, 3, g, g + 1, m, m * m];
        let bounds = values.iter().map(|&v| Some(v)).chain([None]);
        let mut sets = Vec::new();
        for &a in &values {
            for b in bounds.clone() {
                for &lo in &values[2..] {
                    for hi in bounds.clone() {
                        if let Some(set) = Counts::new(a, b, lo, hi) {
                            sets.push(set);
                        }
                    }
                }
            }
        }
        sets.sort_by_key(|set| format!("{set:?}"));
        sets.dedup();
        assert!(sets.len() > 300, "{}", sets.len());
        let mut inside = 0;
        for x in &sets {
            assert!(x.within(x), "{x:?}");
            for y in &sets {
                inside += usize::from(x.within(y));
                let _ = y.contains_range(x.min(), x.max());
            }
            let _ = x.repeated_over(*x, false);
            let _ = x.repeated_under(*x, true);
        }
        assert!(inside > sets.len());
        // A hang guard, generous for unoptimized builds.
        let elapsed = started.elapsed();
        assert!(elapsed.as_secs() < 60, "{elapsed:?}");
    }
}
