//! Value-space equality helpers shared across validators and XPath.
//!
//! These implement XSD §3.3.6.1 (Value Equality) for numeric types, where
//! `NaN == NaN` and `+0 == -0`. The rule differs from Rust's `PartialEq` for
//! `f32`/`f64` (where `NaN != NaN`), so comparisons that need XSD semantics
//! must go through these functions.
//!
//! Duration equality is *not* here: XPath's `durations_equal` (cross-type
//! zero-duration check, §14.4 fn:distinct-values) and the validators'
//! `duration_eq` (signed total-months / day-time-seconds, §3.3.6.1) solve
//! different problems despite similar names.

/// Value-space equality for `xs:float` — NaN equals NaN, `+0 == -0`.
pub fn float_eq(a: f32, b: f32) -> bool {
    if a.is_nan() && b.is_nan() {
        return true;
    }
    a == b
}

/// Value-space equality for `xs:double` — NaN equals NaN, `+0 == -0`.
pub fn double_eq(a: f64, b: f64) -> bool {
    if a.is_nan() && b.is_nan() {
        return true;
    }
    a == b
}
