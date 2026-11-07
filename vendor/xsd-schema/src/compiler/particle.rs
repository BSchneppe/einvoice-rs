//! Particle occurrence handling with threshold optimization
//!
//! This module implements occurrence constraint handling for XSD particles
//! (minOccurs/maxOccurs) with optimization for large maxOccurs values.
//!
//! For small maxOccurs (≤ COUNTED_THRESHOLD), NFA fragments are unrolled
//! (cloned). Every larger finite bound uses counted NFA transitions with O(1)
//! extra states and is enforced exactly, up to the representable maximum
//! `u32::MAX` (XSD 1.1 §3.9.4.3 clause 2.2 requires the sequence length to be
//! ≤ `{max occurs}` whenever it is a number; no finite bound is ever widened to
//! unbounded). The former `MAX_COUNTED_OCCURS = 10_000` approximation was
//! removed in 0.2.0.

use super::fragment::NfaFragment;

/// Threshold above which counted NFA is used instead of unrolling.
///
/// Values ≤ this threshold are unrolled (cloned fragments).
/// Values above use counted transitions with O(1) extra states.
pub const COUNTED_THRESHOLD: u32 = 16;

/// MaxOccurs value representation
///
/// Represents the maxOccurs constraint from XSD, which can be either
/// a bounded positive integer or unbounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaxOccurs {
    /// Unbounded (no maximum limit)
    Unbounded,
    /// Bounded to a specific maximum value
    Bounded(u32),
}

impl MaxOccurs {
    /// Create from an Option, where None means unbounded
    pub fn from_option(max: Option<u32>) -> Self {
        match max {
            Some(n) => MaxOccurs::Bounded(n),
            None => MaxOccurs::Unbounded,
        }
    }

    /// Convert to Option for compatibility with fragment methods
    pub fn to_option(&self) -> Option<u32> {
        match self {
            MaxOccurs::Unbounded => None,
            MaxOccurs::Bounded(n) => Some(*n),
        }
    }

    /// Formerly `true` for any finite bound above 10 000, which the compiler
    /// then widened to unbounded. Every finite bound is now enforced exactly,
    /// so this is identical to [`is_unbounded`](Self::is_unbounded).
    #[deprecated(
        since = "0.2.0",
        note = "finite bounds are always exact now; use `is_unbounded`"
    )]
    pub fn is_effectively_unbounded(&self) -> bool {
        self.is_unbounded()
    }

    /// Check if this is explicitly unbounded
    pub fn is_unbounded(&self) -> bool {
        matches!(self, MaxOccurs::Unbounded)
    }
}

impl Default for MaxOccurs {
    fn default() -> Self {
        MaxOccurs::Bounded(1)
    }
}

/// Apply occurrence constraints with threshold-based dispatch.
///
/// - Small bounded (≤ COUNTED_THRESHOLD): unroll via repeat_range
/// - Larger bounded (any value up to `u32::MAX`): counted NFA via
///   repeat_counted — exact, never approximated
/// - Unbounded: Kleene star via repeat_range
///
/// `min > max` never reaches this function: the pipeline rejects it as
/// `p-props-correct` clause 2.1 before compilation.
pub fn apply_occurs(frag: NfaFragment, min: u32, max: MaxOccurs) -> NfaFragment {
    match max.to_option() {
        // Unbounded with large min → counted exact prefix + star tail
        None if min > COUNTED_THRESHOLD => frag
            .clone()
            .repeat_counted(min, min)
            .concat(frag.repeat_star()),
        // Unbounded with small min → existing unroll via repeat_range (star/plus)
        None => frag.repeat_range(min, None),
        // Small bounded → existing unroll via repeat_range
        Some(m) if m <= COUNTED_THRESHOLD => frag.repeat_range(min, Some(m)),
        // Large bounded → counted construction
        Some(m) if min == 0 => frag.repeat_counted(0, m),
        Some(m) if min <= COUNTED_THRESHOLD => frag
            .clone()
            .repeat_exact(min)
            .concat(frag.repeat_counted(0, m - min)),
        Some(m) => frag.repeat_counted(min, m),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_max_occurs_from_option() {
        assert_eq!(MaxOccurs::from_option(Some(5)), MaxOccurs::Bounded(5));
        assert_eq!(MaxOccurs::from_option(None), MaxOccurs::Unbounded);
    }

    #[test]
    fn test_max_occurs_to_option() {
        assert_eq!(MaxOccurs::Bounded(5).to_option(), Some(5));
        assert_eq!(MaxOccurs::Unbounded.to_option(), None);
    }

    #[test]
    #[allow(deprecated)]
    fn test_max_occurs_effectively_unbounded_is_exact() {
        // No finite bound is ever "effectively" unbounded any more — the
        // former 10 000 cutoff was a spec violation (§3.9.4.3 clause 2.2).
        for n in [50, 100, 1000, 10_000, 10_001, 1_000_000, u32::MAX] {
            assert!(!MaxOccurs::Bounded(n).is_effectively_unbounded(), "{n}");
        }
        assert!(MaxOccurs::Unbounded.is_effectively_unbounded());
    }

    /// Every finite bound above the unroll threshold must produce a counted
    /// NFA (exact), including the values the old cutoff used to widen.
    #[test]
    fn test_apply_occurs_large_finite_bounds_are_counted() {
        use crate::compiler::fragment::FragmentBuilder;
        use crate::compiler::nfa::NfaTerm;
        use crate::ids::NameId;
        let builder = FragmentBuilder::new();
        for max in [COUNTED_THRESHOLD + 1, 10_000, 10_001, 1_000_000, u32::MAX] {
            let frag = builder.single_term(NfaTerm::element(NameId(1), None, None), None);
            let out = apply_occurs(frag, 0, MaxOccurs::Bounded(max));
            assert!(
                !out.counter_defs.is_empty(),
                "maxOccurs={max} must compile to a counted (exact) loop"
            );
            assert_eq!(out.counter_defs[0].max, max);
        }
        // Unbounded stays a star: no counter.
        let frag = builder.single_term(NfaTerm::element(NameId(1), None, None), None);
        assert!(apply_occurs(frag, 0, MaxOccurs::Unbounded)
            .counter_defs
            .is_empty());
    }

    #[test]
    fn test_max_occurs_default() {
        assert_eq!(MaxOccurs::default(), MaxOccurs::Bounded(1));
    }
}
