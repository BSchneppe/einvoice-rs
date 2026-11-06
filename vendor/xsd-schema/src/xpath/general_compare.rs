//! Indexed evaluation of the general comparison operators (XPath 2.0 §3.5.2).
//!
//! A general comparison `A = B` is existentially quantified over the Cartesian
//! product of the two atomized operands, so the straightforward evaluation is
//! `O(|A| · |B|)`. This module decides the same question with a hash index, in
//! `O(|A| + |B|)`, for the operand shapes where doing so is provably equivalent
//! to the pairwise loop — *including* the errors the pairwise loop raises.
//!
//! # Why an index is sound at all
//!
//! §3.5.2 defines the magnitude relationship of a pair `(a, b)` as a set of
//! conversions followed by the value comparison `eq` (§3.5.1). Equality is only
//! defined *within* a comparison class (two numerics, two string-like values,
//! two booleans, …), and inside a class it is decided by a concrete equality
//! test on a derived representation — a promoted `BigInt`/`Decimal`/`f32`/`f64`,
//! a string, a normalized instant, and so on. Whenever a pure function `key`
//! exists with
//!
//! ```text
//!     a eq b   ⟹   key(a) == key(b)
//! ```
//!
//! the pairs that can possibly compare true are exactly the pairs that share a
//! key, so a hash join over the keys finds every candidate. Each candidate is
//! then **confirmed by the real comparison** (`magnitude_relationship_ctx`
//! followed by `value_eq`), which is the same code the pairwise loop runs — so
//! a key collision can never produce a wrong `true`, and the only obligation on
//! `key` is that it must not miss a true pair.
//!
//! # Why the errors stay identical
//!
//! The pairwise loop walks the product in row-major order and
//!
//! * returns `true` at the first pair that compares true;
//! * returns immediately on a *hard* error (a failed cast in
//!   [`magnitude_relationship_ctx`], or an internal conversion failure inside
//!   `eq`);
//! * holds back the **first** `BinaryOperatorNotDefined` error and raises it
//!   only if no pair compares true.
//!
//! §3.5.2 permits returning `true` without looking at the remaining pairs, but
//! this implementation must be bit-for-bit identical to what it did before, so
//! the index path only ever decides a comparison when it has *proved* that no
//! hard error exists anywhere in the product: every key it needs is computed
//! eagerly, with the very conversion functions the comparison itself would use,
//! and a single failure abandons the index and hands the comparison back to the
//! pairwise loop, which then reproduces the original outcome exactly.
//!
//! Once no hard error is possible:
//!
//! * a true pair anywhere ⟹ `true` (a deferred type error can never preempt it);
//! * no true pair and no incomparable class pair ⟹ `false`;
//! * no true pair but incomparable class pairs present ⟹ the deferred error is
//!   the one raised by the **first** incomparable pair in row-major order, and
//!   that pair is located in linear time and then compared for real, so the
//!   error value is produced by the original code path.
//!
//! Anything this module does not model — `xs:untypedAtomic` against a type that
//! is neither numeric nor string-like, union-typed values, an unclassifiable
//! type code — makes it return [`FastOutcome::Fallback`] and changes nothing.
//!
//! # When it runs
//!
//! The callers in `operators` walk a bounded prefix of the product first, and
//! only ask this module when that prefix runs out undecided. Every comparison
//! small enough to fit inside the prefix, and every comparison whose first true
//! pair falls inside it, therefore costs exactly what it cost before: an index
//! is never built for `@id = 'x'`, and never for a large comparison that the
//! nested loop would have answered from its first few pairs.

use std::borrow::Cow;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hash, Hasher as _};

use ahash::AHasher;
use num_bigint::BigInt;
use rust_decimal::Decimal;

use crate::namespace::qname::QualifiedName;
use crate::types::value::{TimezoneOffset, XmlAtomicValue, XmlValue, XmlValueKind};
use crate::types::XmlTypeCode;
use crate::xpath::atomize::push_atoms;
use crate::xpath::collation::CollationRef;
use crate::xpath::context::XPathContext;
use crate::xpath::error::XPathError;
use crate::xpath::iterator::{BufferedNodeIterator, XmlItem, XmlItemRef, XmlNodeIterator};
use crate::xpath::operators::{
    atomize_item, is_date_time_code, is_duration_code, is_gregorian_code, is_string_like,
    is_temporal_type, magnitude_relationship_ctx, numeric_class, value_eq_collated, NumericClass,
};

/// Hasher used for the join tables. A `BuildHasherDefault` is a ZST, so a table
/// costs nothing to seed — which matters because a comparison may build several.
type Hasher = BuildHasherDefault<AHasher>;

/// What the index path concluded about a comparison.
pub(super) enum FastOutcome {
    /// The comparison is decided; this is exactly what the pairwise loop returns.
    Decided(bool),
    /// The comparison raises this error, exactly as the pairwise loop would.
    Raise(XPathError),
    /// The index path does not model this input; run the pairwise loop.
    Fallback,
}

// ============================================================================
// Entry points
// ============================================================================

/// Try to decide `left = right` with a hash index.
///
/// Returns `None` when the index path does not apply, in which case the caller
/// must run the pairwise loop. The right operand is taken already buffered,
/// because the pairwise loop buffers it too and the buffering must happen in
/// the same place to keep iterator errors identical.
pub(super) fn try_indexed_eq<I1, I2>(
    context: &XPathContext,
    left: &I1,
    right: &BufferedNodeIterator<I2>,
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
) -> Option<Result<bool, XPathError>>
where
    I1: XmlNodeIterator,
    I2: XmlNodeIterator,
{
    let (lvals, rvals) = collect_operands(left, right)?;
    match general_eq_indexed(context, &lvals, &rvals, collation, tz) {
        FastOutcome::Decided(result) => Some(Ok(result)),
        FastOutcome::Raise(err) => Some(Err(err)),
        FastOutcome::Fallback => None,
    }
}

/// Try to decide `left != right` with a hash index.
///
/// See [`general_ne_indexed`] for the (much narrower) cases this covers.
pub(super) fn try_indexed_ne<I1, I2>(
    left: &I1,
    right: &BufferedNodeIterator<I2>,
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
) -> Option<Result<bool, XPathError>>
where
    I1: XmlNodeIterator,
    I2: XmlNodeIterator,
{
    let (lvals, rvals) = collect_operands(left, right)?;
    match general_ne_indexed(&lvals, &rvals, collation, tz) {
        FastOutcome::Decided(result) => Some(Ok(result)),
        FastOutcome::Raise(err) => Some(Err(err)),
        FastOutcome::Fallback => None,
    }
}

/// Atomize both operands.
///
/// `None` means atomization raised — and an atomization error must be
/// reproduced by the pairwise loop, which interleaves atomization with
/// comparison and may well return `true` before ever reaching the offending
/// item.
fn collect_operands<I1, I2>(
    left: &I1,
    right: &BufferedNodeIterator<I2>,
) -> Option<(Vec<XmlValue>, Vec<XmlValue>)>
where
    I1: XmlNodeIterator,
    I2: XmlNodeIterator,
{
    let lvals = atomize_all(left)?;
    if lvals.is_empty() {
        // Nothing to index, and the right operand must not be touched: with an
        // empty left operand the pairwise loop never atomizes it either.
        return Some((lvals, Vec::new()));
    }
    let rvals = atomize_all(right)?;
    Some((lvals, rvals))
}

/// Atomize a materialized sequence, dropping nilled items and splitting a
/// list-typed node into its members — the very values, in the very order, the
/// pairwise loop walks. `None` on any error, for the same reason
/// [`atomize_all`] gives.
pub(super) fn atomize_items<N: crate::xpath::DomNavigator>(
    items: &[XmlItem<N>],
) -> Option<Vec<XmlValue>> {
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        match atomize_item(XmlItemRef::from_item(item)) {
            Ok(Some(value)) => push_atoms(value, &mut out),
            Ok(None) => {}
            Err(_) => return None,
        }
    }
    Some(out)
}

/// Atomize a whole sequence, dropping nilled items (which atomize to the empty
/// sequence and take part in no pair) and splitting a list-typed node into
/// its members. `None` on any error.
fn atomize_all<I: XmlNodeIterator>(iter: &I) -> Option<Vec<XmlValue>> {
    let mut cursor = iter.clone();
    let mut out = Vec::new();
    loop {
        match cursor.move_next() {
            Ok(true) => {}
            Ok(false) => break,
            Err(_) => return None,
        }
        let item = cursor.current()?;
        match atomize_item(item) {
            Ok(Some(value)) => push_atoms(value, &mut out),
            Ok(None) => {}
            Err(_) => return None,
        }
    }
    Some(out)
}

// ============================================================================
// Comparison classes
// ============================================================================

/// The four numeric representations `eq` can promote a pair to, in promotion
/// order: a pair is compared at the higher of the two operands' groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum NumGroup {
    /// Every `xs:integer`-derived type; compared as an exact `BigInt`.
    Int,
    /// `xs:decimal`; compared as an exact `Decimal`.
    Dec,
    /// `xs:float`; compared as `f32`.
    Flt,
    /// `xs:double`; compared as `f64`.
    Dbl,
}

fn num_group(class: NumericClass) -> NumGroup {
    match class {
        NumericClass::Decimal => NumGroup::Dec,
        NumericClass::Float => NumGroup::Flt,
        NumericClass::Double => NumGroup::Dbl,
        // Every integer class ends up in the same `BigInt` arm of the
        // promotion, whatever its precedence.
        _ => NumGroup::Int,
    }
}

/// The comparison class of an atomized value, *before* the §3.5.2 conversions.
///
/// The arms mirror the dispatch order of the `eq` implementation, so that two
/// values are comparable exactly when their classes form a comparable pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Class {
    /// `xs:untypedAtomic` — converted per §3.5.2 depending on the other value.
    Untyped,
    Num(NumGroup),
    /// `xs:string` and its derived types, plus `xs:anyURI`.
    Str,
    Bool,
    /// `xs:dateTime` and `xs:dateTimeStamp`.
    DateTime,
    Date,
    Time,
    /// `xs:duration`, `xs:yearMonthDuration` and `xs:dayTimeDuration`, which all
    /// compare as a (months, seconds) pair.
    Duration,
    /// One of the gregorian types (`xs:gYear`, …), compared by its starting
    /// instant with values of the same type only (F&O §10.4.15 – §10.4.19).
    Gregorian(XmlTypeCode),
    QName,
    /// `xs:NOTATION` and the types derived from it: `op:NOTATION-equal`
    /// compares expanded names, ignoring the prefixes (XPath 2.0 §3.5.1).
    Notation,
    /// Everything else: `eq` reduces to structural equality of two values that
    /// carry the same type code (`xs:hexBinary`, `xs:base64Binary`, list
    /// types, …).
    Opaque(XmlTypeCode),
}

/// Classify an atomized value, or `None` if this module does not model it.
fn classify(value: &XmlValue) -> Option<Class> {
    // A union-typed value is unwrapped by `eq` but *not* by the §3.5.2
    // conversions, which look at the outer type code. Rather than model that
    // asymmetry, leave it to the pairwise loop.
    if matches!(value.value, XmlValueKind::Union(_)) {
        return None;
    }
    let code = value.type_code;
    if code == XmlTypeCode::UntypedAtomic {
        return Some(Class::Untyped);
    }
    // The order below is the dispatch order of `eq`.
    if is_temporal_type(code) {
        if is_date_time_code(code) {
            return Some(Class::DateTime);
        }
        if code == XmlTypeCode::Date {
            return Some(Class::Date);
        }
        if code == XmlTypeCode::Time {
            return Some(Class::Time);
        }
        if is_duration_code(code) {
            return Some(Class::Duration);
        }
        return None;
    }
    if is_gregorian_code(code) {
        return Some(Class::Gregorian(code));
    }
    if code.is_list() {
        return Some(Class::Opaque(code));
    }
    if code.is_numeric() {
        return numeric_class(code).map(|class| Class::Num(num_group(class)));
    }
    if code == XmlTypeCode::Boolean {
        return Some(Class::Bool);
    }
    if is_string_like(code) {
        return Some(Class::Str);
    }
    if code == XmlTypeCode::QName {
        return Some(Class::QName);
    }
    if code == XmlTypeCode::Notation {
        return Some(Class::Notation);
    }
    Some(Class::Opaque(code))
}

/// The representation a pair of classes is compared at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Bucket {
    Str,
    Num(NumGroup),
    Bool,
    DateTime,
    Date,
    Time,
    Duration,
    Gregorian(XmlTypeCode),
    QName,
    Notation,
    Opaque(XmlTypeCode),
}

/// What happens when a value of class `left` meets a value of class `right`.
pub(super) enum PairKind {
    /// Comparable; both values map into this bucket.
    Comparable(Bucket),
    /// `eq` raises `BinaryOperatorNotDefined` for every such pair.
    Incomparable,
    /// Not modelled here.
    Unsupported,
}

pub(super) fn pair_kind(left: Class, right: Class) -> PairKind {
    use Class::*;
    match (left, right) {
        // §3.5.2: both untyped, or untyped against a string-like value, are
        // compared as `xs:string`; two string-like values compare directly.
        (Untyped, Untyped) | (Untyped, Str) | (Str, Untyped) | (Str, Str) => {
            PairKind::Comparable(Bucket::Str)
        }
        // §3.5.2: untyped against a numeric type is cast to `xs:double`, so the
        // pair is compared as two doubles.
        (Untyped, Num(_)) | (Num(_), Untyped) => PairKind::Comparable(Bucket::Num(NumGroup::Dbl)),
        // Untyped against a duration, a date/time, a boolean, a QName or an
        // opaque type needs the "cast to the primitive base type of T" rule,
        // whose failure modes are left to the pairwise loop.
        (Untyped, _) | (_, Untyped) => PairKind::Unsupported,

        (Num(l), Num(r)) => PairKind::Comparable(Bucket::Num(l.max(r))),
        (Bool, Bool) => PairKind::Comparable(Bucket::Bool),
        (DateTime, DateTime) => PairKind::Comparable(Bucket::DateTime),
        (Date, Date) => PairKind::Comparable(Bucket::Date),
        (Time, Time) => PairKind::Comparable(Bucket::Time),
        (Duration, Duration) => PairKind::Comparable(Bucket::Duration),
        (Gregorian(l), Gregorian(r)) if l == r => PairKind::Comparable(Bucket::Gregorian(l)),
        (QName, QName) => PairKind::Comparable(Bucket::QName),
        // A QName against a NOTATION falls through: no row of the §3.5.1 table.
        (Notation, Notation) => PairKind::Comparable(Bucket::Notation),
        (Opaque(l), Opaque(r)) if l == r => PairKind::Comparable(Bucket::Opaque(l)),

        _ => PairKind::Incomparable,
    }
}

// ============================================================================
// Keys
// ============================================================================

/// A hash key that agrees with `eq` inside one bucket.
#[derive(Debug, PartialEq, Eq, Hash)]
enum Key<'a> {
    Str(Cow<'a, str>),
    /// The [`sort_key`](crate::xpath::collation::Collation::sort_key) of a string value, under a
    /// non-codepoint collation. The obligation on a key —
    /// `a eq b ⟹ key(a) == key(b)` — is exactly what a sort key promises, so
    /// the index stays sound; the codepoint collation never produces this arm
    /// and keeps [`Key::Str`].
    Bytes(Vec<u8>),
    Int(&'a BigInt),
    Dec(Decimal),
    /// `f32` bit pattern, with `-0.0` folded onto `0.0`.
    F32(u32),
    /// `f64` bit pattern, with `-0.0` folded onto `0.0`.
    F64(u64),
    Bool(Option<bool>),
    /// A date/time normalized to the instant `eq` compares: whole seconds and
    /// the fraction of a second, exactly.
    Instant(i128, Decimal),
    /// (months, whole seconds, fraction of a second), the parts `eq` compares
    /// two durations by.
    Duration(i128, i128, Decimal),
    QName(&'a QualifiedName),
}

enum KeyOutcome<'a> {
    Found(Key<'a>),
    /// The value can never compare equal to anything in this bucket (`NaN`),
    /// and comparing it raises nothing either, so it can be dropped.
    Never,
    /// The conversion the comparison itself would perform fails here, so a hard
    /// error is possible and the index path must not decide the comparison.
    Failed,
}

/// The string `eq` compares string-like values by, borrowed where possible.
fn string_key(value: &XmlValue) -> Cow<'_, str> {
    match &value.value {
        XmlValueKind::Atomic(XmlAtomicValue::String(s)) => Cow::Borrowed(s),
        XmlValueKind::Atomic(XmlAtomicValue::AnyUri(s)) => Cow::Borrowed(s),
        XmlValueKind::UntypedAtomic(s) => Cow::Borrowed(s),
        _ => Cow::Owned(value.to_string_value()),
    }
}

fn f32_key(value: f32) -> KeyOutcome<'static> {
    if value.is_nan() {
        KeyOutcome::Never
    } else if value == 0.0 {
        KeyOutcome::Found(Key::F32(0))
    } else {
        KeyOutcome::Found(Key::F32(value.to_bits()))
    }
}

fn f64_key(value: f64) -> KeyOutcome<'static> {
    if value.is_nan() {
        KeyOutcome::Never
    } else if value == 0.0 {
        KeyOutcome::Found(Key::F64(0))
    } else {
        KeyOutcome::Found(Key::F64(value.to_bits()))
    }
}

/// The `xs:double` an `xs:untypedAtomic` value is cast to when it meets a
/// numeric value. This is the same parse the comparison performs.
fn untyped_as_double(value: &XmlValue) -> Option<f64> {
    crate::xpath::cast::parse_xsd_double(&value.to_string_value())
}

/// Compute the key of `value` in `bucket`.
///
/// Every arm uses the accessor the real comparison uses, so a `Failed` here is
/// exactly a comparison that would have raised.
///
/// `collation` is `None` for the Unicode codepoint collation, in which case
/// every arm is what it always was. Under a host collation only `Bucket::Str`
/// changes, and only that bucket may: it is the one bucket whose `eq` the
/// collation redefines. `Bucket::Opaque` compares values structurally whatever
/// the collation, and `to_string_value` remains a sound key proxy for it. A
/// collation that offers no [`sort_key`](crate::xpath::collation::Collation::sort_key) makes the string
/// bucket `Failed`, which sends the whole comparison to the pairwise loop —
/// where the collation is applied by `eq` itself.
fn key_of<'a>(
    value: &'a XmlValue,
    class: Class,
    bucket: Bucket,
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
) -> KeyOutcome<'a> {
    match bucket {
        // A deferred default collation is resolved here, because a string key
        // is exactly where two strings are about to be compared under it.
        Bucket::Str => match collation.resolved() {
            CollationRef::Codepoint => KeyOutcome::Found(Key::Str(string_key(value))),
            CollationRef::Custom(collation) => match collation.sort_key(&string_key(value)) {
                Some(key) => KeyOutcome::Found(Key::Bytes(key)),
                // No sort key: the index has no sound key for this bucket, so
                // the comparison goes back to the pairwise loop, which applies
                // the collation through `eq` itself.
                None => KeyOutcome::Failed,
            },
            // FOCH0002 belongs to the comparison, not to the index; declining
            // sends the pair to the pairwise loop, which raises it there.
            CollationRef::Unsupported(_) => KeyOutcome::Failed,
            // `resolved()` never answers `Deferred`.
            CollationRef::Deferred(_) => KeyOutcome::Failed,
        },
        Bucket::Num(group) => {
            if class == Class::Untyped {
                // Untyped only ever reaches a numeric bucket as `xs:double`.
                return match untyped_as_double(value) {
                    Some(d) => f64_key(d),
                    None => KeyOutcome::Failed,
                };
            }
            match group {
                NumGroup::Int => match value.as_integer() {
                    Some(i) => KeyOutcome::Found(Key::Int(i)),
                    None => KeyOutcome::Failed,
                },
                NumGroup::Dec => match value.as_decimal() {
                    Some(d) => KeyOutcome::Found(Key::Dec(d)),
                    None => KeyOutcome::Failed,
                },
                NumGroup::Flt => match value.as_double() {
                    Some(d) => f32_key(d as f32),
                    None => KeyOutcome::Failed,
                },
                NumGroup::Dbl => match value.as_double() {
                    Some(d) => f64_key(d),
                    None => KeyOutcome::Failed,
                },
            }
        }
        Bucket::Bool => KeyOutcome::Found(Key::Bool(value.as_boolean())),
        // The implicit timezone `tz` places a value without a timezone, as
        // `eq` does (F&O §10.4).
        Bucket::DateTime => match crate::xpath::operators::datetime_compare_key(value, tz) {
            Some((whole, fraction)) => KeyOutcome::Found(Key::Instant(whole, fraction)),
            None => KeyOutcome::Failed,
        },
        Bucket::Date => match crate::xpath::operators::date_compare_key(value, tz) {
            Some((whole, fraction)) => KeyOutcome::Found(Key::Instant(whole, fraction)),
            None => KeyOutcome::Failed,
        },
        Bucket::Time => match crate::xpath::operators::time_compare_key(value, tz) {
            Some((whole, fraction)) => KeyOutcome::Found(Key::Instant(whole, fraction)),
            None => KeyOutcome::Failed,
        },
        Bucket::Gregorian(_) => match crate::xpath::operators::gregorian_compare_key(value, tz) {
            Some((whole, fraction)) => KeyOutcome::Found(Key::Instant(whole, fraction)),
            None => KeyOutcome::Failed,
        },
        Bucket::Duration => match crate::xpath::operators::duration_compare_key(value) {
            Some(parts) => {
                KeyOutcome::Found(Key::Duration(parts.months, parts.seconds, parts.fraction))
            }
            None => KeyOutcome::Failed,
        },
        Bucket::QName => match value.as_qname() {
            Some(qname) => KeyOutcome::Found(Key::QName(qname)),
            None => KeyOutcome::Failed,
        },
        // The expanded name, never the lexical form: two NOTATIONs that differ
        // only in their prefixes are `eq`. A NOTATION still held in lexical
        // form (no namespace context resolved it) is left to the pairwise loop.
        Bucket::Notation => match &value.value {
            XmlValueKind::Atomic(XmlAtomicValue::Notation(name)) => {
                KeyOutcome::Found(Key::QName(name.name()))
            }
            _ => KeyOutcome::Failed,
        },
        // `eq` compares these by structural equality of the value, and the
        // string value of each of these types is a function of exactly what
        // that equality compares (no part of the value is ignored), so equal
        // values always share a key. Unequal values may collide; confirmation
        // settles them. (`xs:QName` and `xs:NOTATION` are not here: their
        // string value carries the prefix, which equality ignores — so a list
        // value holding such members, which atomization never hands over but a
        // host may, is left to the pairwise loop.)
        Bucket::Opaque(_) if holds_a_name(value) => KeyOutcome::Failed,
        Bucket::Opaque(_) => KeyOutcome::Found(Key::Str(string_key(value))),
    }
}

/// Whether a list value has an `xs:QName` or `xs:NOTATION` member.
fn holds_a_name(value: &XmlValue) -> bool {
    match &value.value {
        XmlValueKind::List { items, .. } => items.iter().any(|member| {
            matches!(
                member.value,
                XmlValueKind::Atomic(XmlAtomicValue::QName(_) | XmlAtomicValue::Notation(_))
            )
        }),
        _ => false,
    }
}

/// The outcome of hashing the comparison key of a value in one bucket.
pub(super) enum KeyHash {
    /// The key hashes to this.
    Found(u64),
    /// The value can never compare equal to anything in this bucket (`NaN`), and
    /// comparing it raises nothing either.
    Never,
    /// The conversion the comparison itself would perform fails here, so a hard
    /// error is possible and nothing may be decided from an index.
    Failed,
}

/// Hash the comparison key of `value` in `bucket`.
///
/// The obligation on a key is one-directional — `a eq b ⟹ key(a) == key(b)` —
/// and hashing only weakens it in the harmless direction:
/// `key(a) == key(b) ⟹ hash(a) == hash(b)`, so `hash(a) != hash(b)` still proves
/// `¬(a eq b)`, while a hash collision merely produces one more candidate, and
/// every candidate is confirmed by the real comparison. Keying a table by the
/// hash instead of by [`Key`] is what lets the table **own** its keys rather than
/// borrow from the values it indexes — which is what an index that outlives a
/// single evaluation needs.
pub(super) fn key_hash(
    value: &XmlValue,
    class: Class,
    bucket: Bucket,
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
) -> KeyHash {
    match key_of(value, class, bucket, collation, tz) {
        KeyOutcome::Found(key) => {
            let mut hasher = AHasher::default();
            key.hash(&mut hasher);
            KeyHash::Found(hasher.finish())
        }
        KeyOutcome::Never => KeyHash::Never,
        KeyOutcome::Failed => KeyHash::Failed,
    }
}

// ============================================================================
// `=`
// ============================================================================

/// Run the pair through the real comparison: the §3.5.2 conversions followed by
/// the `eq` value comparison. Every candidate the index finds goes through here.
pub(super) fn confirm(
    context: &XPathContext,
    left: &XmlValue,
    right: &XmlValue,
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
) -> Result<bool, XPathError> {
    let (l, r) = magnitude_relationship_ctx(context, left, right)?;
    value_eq_collated(&l, &r, collation, tz)
}

/// One comparable class pair, with the keys of the values it joins.
struct Join<'a> {
    left: Vec<(usize, Key<'a>)>,
    right: Vec<(usize, Key<'a>)>,
}

fn general_eq_indexed(
    context: &XPathContext,
    lvals: &[XmlValue],
    rvals: &[XmlValue],
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
) -> FastOutcome {
    if lvals.is_empty() || rvals.is_empty() {
        return FastOutcome::Decided(false);
    }

    let (lclasses, rclasses) = match (classify_all(lvals), classify_all(rvals)) {
        (Some(l), Some(r)) => (l, r),
        _ => return FastOutcome::Fallback,
    };

    let lset = distinct(&lclasses);
    let rset = distinct(&rclasses);

    // Split the class pairs into the ones that join and the ones that raise.
    let mut joins: Vec<(Class, Class, Bucket)> = Vec::new();
    let mut incomparable: Vec<(Class, Class)> = Vec::new();
    for &lc in &lset {
        for &rc in &rset {
            match pair_kind(lc, rc) {
                PairKind::Comparable(bucket) => joins.push((lc, rc, bucket)),
                PairKind::Incomparable => incomparable.push((lc, rc)),
                PairKind::Unsupported => return FastOutcome::Fallback,
            }
        }
    }

    // Phase 1 — compute every key. A single failure means a hard error is
    // possible somewhere in the product, and a hard error can preempt a true
    // pair, so the whole comparison goes back to the pairwise loop.
    let mut prepared: Vec<Join<'_>> = Vec::with_capacity(joins.len());
    for &(lc, rc, bucket) in &joins {
        let left = match collect_keys(lvals, &lclasses, lc, bucket, collation, tz) {
            Some(keys) => keys,
            None => return FastOutcome::Fallback,
        };
        let right = match collect_keys(rvals, &rclasses, rc, bucket, collation, tz) {
            Some(keys) => keys,
            None => return FastOutcome::Fallback,
        };
        prepared.push(Join { left, right });
    }

    // Phase 2 — hash join. No hard error is possible from here on, so the first
    // confirmed true pair decides the comparison whatever its position.
    for join in &prepared {
        match join_any_true(context, lvals, rvals, join, collation, tz) {
            Ok(true) => return FastOutcome::Decided(true),
            Ok(false) => {}
            Err(_) => return FastOutcome::Fallback,
        }
    }

    // Phase 3 — no pair is true. Either nothing raises, or the deferred error
    // belongs to the first incomparable pair in row-major order.
    if incomparable.is_empty() {
        return FastOutcome::Decided(false);
    }
    match first_incomparable_pair(&lclasses, &rclasses, &incomparable) {
        Some((i, j)) => match confirm(context, &lvals[i], &rvals[j], collation, tz) {
            Err(err) => FastOutcome::Raise(err),
            // The class pair was classified as always-raising, so this is
            // unreachable; falling back is the safe way to say so.
            Ok(_) => FastOutcome::Fallback,
        },
        None => FastOutcome::Fallback,
    }
}

pub(super) fn classify_all(values: &[XmlValue]) -> Option<Vec<Class>> {
    values.iter().map(classify).collect()
}

pub(super) fn distinct(classes: &[Class]) -> Vec<Class> {
    let mut out: Vec<Class> = Vec::new();
    for &class in classes {
        if !out.contains(&class) {
            out.push(class);
        }
    }
    out
}

/// Keys of every value of class `class`, in `bucket`. `None` if any conversion
/// the comparison would perform fails.
fn collect_keys<'a>(
    values: &'a [XmlValue],
    classes: &[Class],
    class: Class,
    bucket: Bucket,
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
) -> Option<Vec<(usize, Key<'a>)>> {
    let mut out = Vec::new();
    for (index, value) in values.iter().enumerate() {
        if classes[index] != class {
            continue;
        }
        match key_of(value, class, bucket, collation, tz) {
            KeyOutcome::Found(key) => out.push((index, key)),
            KeyOutcome::Never => {}
            KeyOutcome::Failed => return None,
        }
    }
    Some(out)
}

/// End of a collision chain.
const NO_SLOT: usize = usize::MAX;

/// A hash join table over one side's keys.
///
/// `head` maps a key to the last slot inserted for it and `next` chains the
/// earlier slots carrying the same key, so the table costs two allocations
/// instead of one per distinct key — which matters at a hundred thousand items.
struct Table<'k, 'a> {
    head: HashMap<&'k Key<'a>, usize, Hasher>,
    next: Vec<usize>,
}

fn build_table<'k, 'a>(keys: &'k [(usize, Key<'a>)]) -> Table<'k, 'a> {
    let mut head: HashMap<&Key<'_>, usize, Hasher> =
        HashMap::with_capacity_and_hasher(keys.len(), Hasher::default());
    let mut next = vec![NO_SLOT; keys.len()];
    for (slot, (_, key)) in keys.iter().enumerate() {
        if let Some(previous) = head.insert(key, slot) {
            next[slot] = previous;
        }
    }
    Table { head, next }
}

/// Does this class pair contain a pair that compares true?
fn join_any_true(
    context: &XPathContext,
    lvals: &[XmlValue],
    rvals: &[XmlValue],
    join: &Join<'_>,
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
) -> Result<bool, XPathError> {
    if join.left.is_empty() || join.right.is_empty() {
        return Ok(false);
    }
    // Index the smaller side; probe with the larger one.
    let indexed_is_left = join.left.len() <= join.right.len();
    let (indexed, probes) = if indexed_is_left {
        (&join.left, &join.right)
    } else {
        (&join.right, &join.left)
    };

    let table = build_table(indexed);
    for (probe_index, key) in probes {
        let mut slot = table.head.get(key).copied().unwrap_or(NO_SLOT);
        while slot != NO_SLOT {
            let hit = indexed[slot].0;
            let (left, right) = if indexed_is_left {
                (&lvals[hit], &rvals[*probe_index])
            } else {
                (&lvals[*probe_index], &rvals[hit])
            };
            if confirm(context, left, right, collation, tz)? {
                return Ok(true);
            }
            slot = table.next[slot];
        }
    }
    Ok(false)
}

/// The first pair in row-major order whose class pair is incomparable.
pub(super) fn first_incomparable_pair(
    lclasses: &[Class],
    rclasses: &[Class],
    incomparable: &[(Class, Class)],
) -> Option<(usize, usize)> {
    // First occurrence of every class on the right.
    let mut first_right: Vec<(Class, usize)> = Vec::new();
    for (index, &class) in rclasses.iter().enumerate() {
        if !first_right.iter().any(|(seen, _)| *seen == class) {
            first_right.push((class, index));
        }
    }

    for (i, lclass) in lclasses.iter().enumerate() {
        let mut best: Option<usize> = None;
        for (lc, rc) in incomparable {
            if lc != lclass {
                continue;
            }
            if let Some((_, j)) = first_right.iter().find(|(class, _)| class == rc) {
                best = Some(match best {
                    Some(current) => current.min(*j),
                    None => *j,
                });
            }
        }
        if let Some(j) = best {
            return Some((i, j));
        }
    }
    None
}

// ============================================================================
// `!=`
// ============================================================================

/// Decide `A != B` — "some pair compares false" — without walking the product.
///
/// The pairwise loop already stops at the first unequal pair, so it is only
/// quadratic when *every* pair is equal. That degenerate case is decided here,
/// but only for the buckets in which `eq` is a genuine equivalence relation, so
/// that "every pair is equal" is the same statement as "all keys are equal":
///
/// * `Str` — equality of the string values under the collation in force
///   (codepoint equality unless a host collation is installed, in which case a
///   collation with no sort key declines the index altogether);
/// * `Bool` — equality of two booleans;
/// * `Num(Int)` — exact `BigInt` equality.
///
/// Numeric buckets that involve `xs:float`/`xs:double` are excluded because
/// promotion makes `eq` non-transitive across them, and everything else falls
/// back.
fn general_ne_indexed(
    lvals: &[XmlValue],
    rvals: &[XmlValue],
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
) -> FastOutcome {
    if lvals.is_empty() || rvals.is_empty() {
        return FastOutcome::Decided(false);
    }

    let (lclasses, rclasses) = match (classify_all(lvals), classify_all(rvals)) {
        (Some(l), Some(r)) => (l, r),
        _ => return FastOutcome::Fallback,
    };

    let bucket = match equivalence_bucket(&lclasses, &rclasses) {
        Some(bucket) => bucket,
        None => return FastOutcome::Fallback,
    };

    // Every pair is equal exactly when every value carries the same key.
    let mut reference: Option<Key<'_>> = None;
    for (values, classes) in [(lvals, &lclasses), (rvals, &rclasses)] {
        for (index, value) in values.iter().enumerate() {
            let key = match key_of(value, classes[index], bucket, collation, tz) {
                KeyOutcome::Found(key) => key,
                // `Never` cannot occur: none of the three buckets is a float
                // bucket. `Failed` means the comparison itself would raise, or
                // that the collation in force cannot produce a sort key.
                KeyOutcome::Never | KeyOutcome::Failed => return FastOutcome::Fallback,
            };
            match &reference {
                None => reference = Some(key),
                Some(first) if *first == key => {}
                Some(_) => return FastOutcome::Decided(true),
            }
        }
    }
    FastOutcome::Decided(false)
}

/// The single bucket every pair of these operands is compared in, if that
/// bucket is one where `eq` is an equivalence relation.
fn equivalence_bucket(lclasses: &[Class], rclasses: &[Class]) -> Option<Bucket> {
    let mut all_string_like = true;
    let mut all_bool = true;
    let mut all_int = true;
    for class in lclasses.iter().chain(rclasses.iter()) {
        match class {
            Class::Untyped | Class::Str => {
                all_bool = false;
                all_int = false;
            }
            Class::Bool => {
                all_string_like = false;
                all_int = false;
            }
            Class::Num(NumGroup::Int) => {
                all_string_like = false;
                all_bool = false;
            }
            _ => return None,
        }
    }
    if all_string_like {
        Some(Bucket::Str)
    } else if all_bool {
        Some(Bucket::Bool)
    } else if all_int {
        Some(Bucket::Num(NumGroup::Int))
    } else {
        None
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::namespace::NameTable;
    use crate::navigator::RoXmlNavigator;
    use crate::types::value::{
        DateTimeValue, DateValue, DayTimeDurationValue, DurationValue, GYearValue, QNameValue,
        TimeValue, TimezoneOffset, YearMonthDurationValue,
    };
    use crate::xpath::iterator::{VecNodeIterator, XmlItem};
    use crate::xpath::operators::{general_eq_iter_pairwise, general_ne_iter_pairwise};

    type Nav = RoXmlNavigator<'static>;

    fn iter_of(values: &[XmlValue]) -> VecNodeIterator<Nav> {
        VecNodeIterator::new(values.iter().cloned().map(XmlItem::Atomic).collect())
    }

    /// What the two paths produced, rendered so that two runs can be compared
    /// exactly — the error value included, not only its code.
    fn render(result: &Result<bool, XPathError>) -> String {
        format!("{:?}", result)
    }

    /// Run one `=` case through both paths. Returns true when the index path
    /// decided it (as opposed to declining).
    fn check_eq(context: &XPathContext, left: &[XmlValue], right: &[XmlValue]) -> bool {
        let left_iter = iter_of(left);
        let right_iter = iter_of(right);
        let right_buf = BufferedNodeIterator::preload(right_iter).unwrap();

        let expected = general_eq_iter_pairwise(context, &left_iter, &right_buf);

        // The very collation the pairwise loop just used, so that the two
        // paths are compared under the same rule whatever the context says.
        let active = crate::xpath::collation::resolve_default(context);
        let actual = try_indexed_eq(
            context,
            &left_iter,
            &right_buf,
            active.as_ref(),
            crate::xpath::operators::static_timezone(context),
        );

        match actual {
            Some(got) => {
                assert_eq!(
                    render(&expected),
                    render(&got),
                    "`=` diverged\n  left  = {:?}\n  right = {:?}",
                    left,
                    right
                );
                true
            }
            None => false,
        }
    }

    /// Run one `!=` case through both paths.
    fn check_ne(context: &XPathContext, left: &[XmlValue], right: &[XmlValue]) -> bool {
        let left_iter = iter_of(left);
        let right_iter = iter_of(right);
        let right_buf = BufferedNodeIterator::preload(right_iter).unwrap();

        let expected = general_ne_iter_pairwise(context, &left_iter, &right_buf);

        let active = crate::xpath::collation::resolve_default(context);
        let actual = try_indexed_ne(
            &left_iter,
            &right_buf,
            active.as_ref(),
            crate::xpath::operators::static_timezone(context),
        );

        match actual {
            Some(got) => {
                assert_eq!(
                    render(&expected),
                    render(&got),
                    "`!=` diverged\n  left  = {:?}\n  right = {:?}",
                    left,
                    right
                );
                true
            }
            None => false,
        }
    }

    // ------------------------------------------------------------------
    // Corpus
    // ------------------------------------------------------------------

    /// xorshift64*, so the corpus is the same on every run and on every host.
    pub struct Rng(pub u64);

    impl Rng {
        pub fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }

        pub fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
    }

    fn typed(code: XmlTypeCode, atom: XmlAtomicValue) -> XmlValue {
        XmlValue::new(code, XmlValueKind::Atomic(atom))
    }

    fn nmtoken(token: &str) -> XmlValue {
        typed(XmlTypeCode::NmToken, XmlAtomicValue::String(token.into()))
    }

    fn dec(text: &str) -> Decimal {
        text.parse::<Decimal>().unwrap()
    }

    fn strings() -> Vec<XmlValue> {
        vec![
            XmlValue::string(""),
            XmlValue::string("a"),
            XmlValue::string("b"),
            XmlValue::string("1"),
            XmlValue::string("2.0"),
            XmlValue::string("2000-01-01"),
            typed(XmlTypeCode::Token, XmlAtomicValue::String("a".into())),
            typed(XmlTypeCode::NCName, XmlAtomicValue::String("b".into())),
            typed(XmlTypeCode::AnyUri, XmlAtomicValue::AnyUri("a".into())),
            typed(
                XmlTypeCode::AnyUri,
                XmlAtomicValue::AnyUri("http://example.org/".into()),
            ),
        ]
    }

    fn untypeds() -> Vec<XmlValue> {
        vec![
            XmlValue::untyped("a"),
            XmlValue::untyped("1"),
            XmlValue::untyped("2"),
            XmlValue::untyped("2.0"),
            XmlValue::untyped(" 3 "),
            XmlValue::untyped("1e0"),
            XmlValue::untyped("not-a-number"),
            XmlValue::untyped("2000-01-01"),
            XmlValue::untyped("P1Y"),
            XmlValue::untyped("true"),
            XmlValue::untyped(""),
        ]
    }

    fn numerics() -> Vec<XmlValue> {
        vec![
            XmlValue::integer(BigInt::from(0)),
            XmlValue::integer(BigInt::from(1)),
            XmlValue::integer(BigInt::from(2)),
            XmlValue::integer(BigInt::from(-1)),
            // Beyond 2^53: distinct integers that collapse onto the same f64.
            XmlValue::integer(BigInt::from(9007199254740993i64)),
            XmlValue::integer(BigInt::from(9007199254740992i64)),
            // Beyond xs:decimal's range, so the decimal promotion fails.
            XmlValue::integer("123456789012345678901234567890123456".parse().unwrap()),
            typed(XmlTypeCode::Int, XmlAtomicValue::Integer(BigInt::from(1))),
            typed(
                XmlTypeCode::UnsignedByte,
                XmlAtomicValue::Integer(BigInt::from(2)),
            ),
            XmlValue::decimal(dec("1")),
            XmlValue::decimal(dec("1.00")),
            XmlValue::decimal(dec("2.5")),
            XmlValue::decimal(dec("0.1")),
            XmlValue::float(1.0),
            XmlValue::float(0.1),
            XmlValue::float(-0.0),
            XmlValue::float(f32::NAN),
            XmlValue::double(1.0),
            XmlValue::double(0.1),
            XmlValue::double(0.0),
            XmlValue::double(-0.0),
            XmlValue::double(f64::NAN),
            XmlValue::double(f64::INFINITY),
            XmlValue::double(9007199254740992.0),
        ]
    }

    fn booleans() -> Vec<XmlValue> {
        vec![XmlValue::boolean(true), XmlValue::boolean(false)]
    }

    fn date_times() -> Vec<XmlValue> {
        let make = |hour: u8, tz: Option<i16>| {
            typed(
                XmlTypeCode::DateTime,
                XmlAtomicValue::DateTime(DateTimeValue {
                    year: 2000,
                    month: 1,
                    day: 1,
                    hour,
                    minute: 0,
                    second: Decimal::ZERO,
                    timezone: tz.map(TimezoneOffset),
                }),
            )
        };
        vec![
            make(0, Some(0)),
            make(1, Some(60)),
            make(0, None),
            make(5, Some(0)),
            typed(
                XmlTypeCode::DateTimeStamp,
                XmlAtomicValue::DateTime(DateTimeValue {
                    year: 2000,
                    month: 1,
                    day: 1,
                    hour: 0,
                    minute: 0,
                    second: Decimal::ZERO,
                    timezone: Some(TimezoneOffset(0)),
                }),
            ),
        ]
    }

    fn dates() -> Vec<XmlValue> {
        let make = |day: u8, tz: Option<i16>| {
            typed(
                XmlTypeCode::Date,
                XmlAtomicValue::Date(DateValue {
                    year: 2000,
                    month: 1,
                    day,
                    timezone: tz.map(TimezoneOffset),
                }),
            )
        };
        vec![
            make(1, Some(0)),
            make(1, None),
            make(2, Some(0)),
            make(1, Some(-60)),
        ]
    }

    fn times() -> Vec<XmlValue> {
        let make = |hour: u8, tz: Option<i16>| {
            typed(
                XmlTypeCode::Time,
                XmlAtomicValue::Time(TimeValue {
                    hour,
                    minute: 30,
                    second: Decimal::ZERO,
                    timezone: tz.map(TimezoneOffset),
                }),
            )
        };
        vec![make(10, Some(0)), make(11, Some(60)), make(10, None)]
    }

    fn durations() -> Vec<XmlValue> {
        vec![
            typed(
                XmlTypeCode::Duration,
                XmlAtomicValue::Duration(DurationValue {
                    negative: false,
                    years: 1,
                    months: 0,
                    days: 0,
                    hours: 0,
                    minutes: 0,
                    seconds: Decimal::ZERO,
                }),
            ),
            typed(
                XmlTypeCode::Duration,
                XmlAtomicValue::Duration(DurationValue {
                    negative: false,
                    years: 0,
                    months: 12,
                    days: 0,
                    hours: 0,
                    minutes: 0,
                    seconds: Decimal::ZERO,
                }),
            ),
            typed(
                XmlTypeCode::YearMonthDuration,
                XmlAtomicValue::YearMonthDuration(YearMonthDurationValue {
                    negative: false,
                    years: 1,
                    months: 0,
                }),
            ),
            typed(
                XmlTypeCode::YearMonthDuration,
                XmlAtomicValue::YearMonthDuration(YearMonthDurationValue {
                    negative: false,
                    years: 0,
                    months: 13,
                }),
            ),
            typed(
                XmlTypeCode::DayTimeDuration,
                XmlAtomicValue::DayTimeDuration(DayTimeDurationValue {
                    negative: false,
                    days: 1,
                    hours: 0,
                    minutes: 0,
                    seconds: Decimal::ZERO,
                }),
            ),
            typed(
                XmlTypeCode::DayTimeDuration,
                XmlAtomicValue::DayTimeDuration(DayTimeDurationValue {
                    negative: false,
                    days: 0,
                    hours: 24,
                    minutes: 0,
                    seconds: Decimal::ZERO,
                }),
            ),
        ]
    }

    fn opaques(names: &NameTable) -> Vec<XmlValue> {
        let ns = names.add("http://example.org/ns");
        let local_a = names.add("a");
        let local_b = names.add("b");
        let prefix = names.add("p");
        vec![
            typed(
                XmlTypeCode::HexBinary,
                XmlAtomicValue::HexBinary(vec![1, 2]),
            ),
            typed(
                XmlTypeCode::HexBinary,
                XmlAtomicValue::HexBinary(vec![1, 3]),
            ),
            typed(
                XmlTypeCode::Base64Binary,
                XmlAtomicValue::Base64Binary(vec![1, 2]),
            ),
            typed(
                XmlTypeCode::GYear,
                XmlAtomicValue::GYear(GYearValue {
                    year: 2000,
                    timezone: None,
                }),
            ),
            typed(
                XmlTypeCode::GYear,
                XmlAtomicValue::GYear(GYearValue {
                    year: 2001,
                    timezone: Some(TimezoneOffset(0)),
                }),
            ),
            typed(
                XmlTypeCode::QName,
                XmlAtomicValue::QName(QNameValue::new(
                    QualifiedName::new(Some(ns), local_a, Some(prefix)),
                    names,
                )),
            ),
            typed(
                XmlTypeCode::QName,
                XmlAtomicValue::QName(QNameValue::new(
                    QualifiedName::new(Some(ns), local_a, None),
                    names,
                )),
            ),
            typed(
                XmlTypeCode::QName,
                XmlAtomicValue::QName(QNameValue::new(
                    QualifiedName::new(None, local_b, None),
                    names,
                )),
            ),
            typed(
                XmlTypeCode::Notation,
                XmlAtomicValue::Notation(QNameValue::new(
                    QualifiedName::new(Some(ns), local_a, None),
                    names,
                )),
            ),
            // The same NOTATION spelled with a prefix: `eq` to the one above.
            typed(
                XmlTypeCode::Notation,
                XmlAtomicValue::Notation(QNameValue::new(
                    QualifiedName::new(Some(ns), local_a, Some(prefix)),
                    names,
                )),
            ),
            // A node whose typed value is a list.
            XmlValue::new(
                XmlTypeCode::NmTokens,
                XmlValueKind::List {
                    item_type: XmlTypeCode::NmToken,
                    items: vec![nmtoken("a"), nmtoken("b")],
                },
            ),
            XmlValue::new(
                XmlTypeCode::NmTokens,
                XmlValueKind::List {
                    item_type: XmlTypeCode::NmToken,
                    items: vec![nmtoken("a b")],
                },
            ),
            // A union-typed value, which the index path always declines.
            XmlValue::new(
                XmlTypeCode::String,
                XmlValueKind::Union(Box::new(XmlValue::string("a"))),
            ),
        ]
    }

    pub fn families(names: &NameTable) -> Vec<Vec<XmlValue>> {
        vec![
            strings(),
            untypeds(),
            numerics(),
            booleans(),
            date_times(),
            dates(),
            times(),
            durations(),
            opaques(names),
        ]
    }

    /// Draw a sequence of 0..=4 values, mostly from one or two families so that
    /// the comparable class pairs are actually exercised, with an occasional
    /// item from anywhere so the mixed and incomparable shapes are too.
    pub fn draw(rng: &mut Rng, families: &[Vec<XmlValue>]) -> Vec<XmlValue> {
        let len = rng.below(5);
        let home = rng.below(families.len());
        let guest = rng.below(families.len());
        let mut out = Vec::with_capacity(len);
        for _ in 0..len {
            let family = match rng.below(10) {
                0 => &families[rng.below(families.len())],
                1..=3 => &families[guest],
                _ => &families[home],
            };
            out.push(family[rng.below(family.len())].clone());
        }
        out
    }

    // ------------------------------------------------------------------
    // Differential tests
    // ------------------------------------------------------------------

    #[test]
    fn the_indexed_path_agrees_with_the_pairwise_path_on_equality() {
        let names = NameTable::new();
        let context = XPathContext::new(&names);
        let families = families(&names);
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);

        let mut decided = 0usize;
        let total = 6000usize;
        for _ in 0..total {
            let left = draw(&mut rng, &families);
            let right = draw(&mut rng, &families);
            if check_eq(&context, &left, &right) {
                decided += 1;
            }
        }
        // The point of the corpus is to exercise the index path, not only to
        // watch it decline.
        println!("`=` random sequences: index path decided {decided} of {total}");
        assert!(
            decided * 4 >= total,
            "the index path decided only {decided} of {total} cases"
        );
    }

    #[test]
    fn the_indexed_path_agrees_with_the_pairwise_path_on_inequality() {
        let names = NameTable::new();
        let context = XPathContext::new(&names);
        let families = families(&names);
        let mut rng = Rng(0x2545_F491_4F6C_DD1D);

        let mut decided = 0usize;
        let total = 6000usize;
        for _ in 0..total {
            let left = draw(&mut rng, &families);
            let right = draw(&mut rng, &families);
            if check_ne(&context, &left, &right) {
                decided += 1;
            }
        }
        println!("`!=` random sequences: index path decided {decided} of {total}");
        assert!(
            decided * 4 >= total,
            "the index path decided only {decided} of {total} cases"
        );
    }

    /// Every single-value pair in the whole corpus, both ways round: the
    /// smallest products, where a class pair is never masked by a neighbour.
    #[test]
    fn the_indexed_path_agrees_on_every_pair_of_corpus_values() {
        let names = NameTable::new();
        let context = XPathContext::new(&names);
        let all: Vec<XmlValue> = families(&names).into_iter().flatten().collect();
        let mut decided = 0usize;
        let mut total = 0usize;
        for left in &all {
            for right in &all {
                let l = std::slice::from_ref(left);
                let r = std::slice::from_ref(right);
                total += 2;
                decided += usize::from(check_eq(&context, l, r));
                decided += usize::from(check_ne(&context, l, r));
            }
        }
        println!("single-value pairs: index path decided {decided} of {total}");
        assert!(decided * 4 >= total);
    }

    /// Two-value operands against one: enough to make a pair order matter, so
    /// that a deferred type error is raised by the same pair as before.
    #[test]
    fn the_indexed_path_agrees_on_ordered_triples_of_corpus_values() {
        let names = NameTable::new();
        let context = XPathContext::new(&names);
        let all: Vec<XmlValue> = families(&names).into_iter().flatten().collect();
        let mut rng = Rng(0x0123_4567_89AB_CDEF);
        let mut decided = 0usize;
        let mut total = 0usize;
        for _ in 0..8000 {
            let left = vec![
                all[rng.below(all.len())].clone(),
                all[rng.below(all.len())].clone(),
            ];
            let right = vec![
                all[rng.below(all.len())].clone(),
                all[rng.below(all.len())].clone(),
            ];
            total += 2;
            decided += usize::from(check_eq(&context, &left, &right));
            decided += usize::from(check_ne(&context, &left, &right));
        }
        println!("two-by-two products: index path decided {decided} of {total}");
        assert!(decided * 5 >= total);
    }

    // ------------------------------------------------------------------
    // Rule-by-rule unit tests (§3.5.2)
    // ------------------------------------------------------------------

    fn indexed_eq(context: &XPathContext, left: &[XmlValue], right: &[XmlValue]) -> bool {
        let left_iter = iter_of(left);
        let right_buf = BufferedNodeIterator::preload(iter_of(right)).unwrap();
        let active = crate::xpath::collation::resolve_default(context);
        try_indexed_eq(
            context,
            &left_iter,
            &right_buf,
            active.as_ref(),
            crate::xpath::operators::static_timezone(context),
        )
        .expect("the index path should have decided this comparison")
        .expect("the comparison should not raise")
    }

    fn indexed_eq_error(context: &XPathContext, left: &[XmlValue], right: &[XmlValue]) -> String {
        let left_iter = iter_of(left);
        let right_buf = BufferedNodeIterator::preload(iter_of(right)).unwrap();
        let active = crate::xpath::collation::resolve_default(context);
        let result = try_indexed_eq(
            context,
            &left_iter,
            &right_buf,
            active.as_ref(),
            crate::xpath::operators::static_timezone(context),
        );
        format!(
            "{}",
            result
                .expect("the index path should have decided this comparison")
                .expect_err("the comparison should raise")
        )
    }

    #[test]
    fn two_untyped_values_are_compared_as_strings() {
        let names = NameTable::new();
        let context = XPathContext::new(&names);
        // "2" and "2.0" are equal as numbers but not as strings.
        assert!(!indexed_eq(
            &context,
            &[XmlValue::untyped("2"), XmlValue::untyped("x")],
            &[XmlValue::untyped("2.0"), XmlValue::untyped("y")]
        ));
        assert!(indexed_eq(
            &context,
            &[XmlValue::untyped("2"), XmlValue::untyped("x")],
            &[XmlValue::untyped("2"), XmlValue::untyped("y")]
        ));
    }

    #[test]
    fn an_untyped_value_against_a_string_is_compared_as_a_string() {
        let names = NameTable::new();
        let context = XPathContext::new(&names);
        assert!(indexed_eq(
            &context,
            &[XmlValue::untyped("a"), XmlValue::untyped("b")],
            &[XmlValue::string("b"), XmlValue::string("c")]
        ));
        assert!(!indexed_eq(
            &context,
            &[XmlValue::untyped("2"), XmlValue::untyped("b")],
            &[XmlValue::string("2.0"), XmlValue::string("c")]
        ));
    }

    #[test]
    fn an_untyped_value_against_a_numeric_value_is_compared_as_a_double() {
        let names = NameTable::new();
        let context = XPathContext::new(&names);
        assert!(indexed_eq(
            &context,
            &[XmlValue::untyped("2.0"), XmlValue::untyped("9")],
            &[
                XmlValue::integer(BigInt::from(2)),
                XmlValue::integer(BigInt::from(7))
            ]
        ));
    }

    #[test]
    fn an_anyuri_value_is_comparable_with_a_string() {
        let names = NameTable::new();
        let context = XPathContext::new(&names);
        let uri = typed(XmlTypeCode::AnyUri, XmlAtomicValue::AnyUri("a".into()));
        assert!(indexed_eq(
            &context,
            &[uri.clone(), XmlValue::string("z")],
            &[XmlValue::string("a"), XmlValue::string("y")]
        ));
    }

    #[test]
    fn numeric_values_are_compared_after_promotion() {
        let names = NameTable::new();
        let context = XPathContext::new(&names);
        // xs:integer against xs:double promotes to xs:double.
        assert!(indexed_eq(
            &context,
            &[
                XmlValue::integer(BigInt::from(1)),
                XmlValue::integer(BigInt::from(5))
            ],
            &[XmlValue::double(1.0), XmlValue::double(7.0)]
        ));
        // xs:decimal against xs:float promotes to xs:float, where 0.1 and the
        // decimal 0.1 do coincide — they do not as doubles.
        assert!(indexed_eq(
            &context,
            &[XmlValue::decimal(dec("0.1")), XmlValue::decimal(dec("9"))],
            &[XmlValue::float(0.1), XmlValue::float(7.0)]
        ));
        // Trailing zeros do not change an xs:decimal's value.
        assert!(indexed_eq(
            &context,
            &[XmlValue::decimal(dec("1.00")), XmlValue::decimal(dec("9"))],
            &[
                XmlValue::integer(BigInt::from(1)),
                XmlValue::integer(BigInt::from(7))
            ]
        ));
    }

    #[test]
    fn positive_and_negative_zero_are_equal_and_nan_equals_nothing() {
        let names = NameTable::new();
        let context = XPathContext::new(&names);
        assert!(indexed_eq(
            &context,
            &[XmlValue::double(-0.0), XmlValue::double(5.0)],
            &[XmlValue::double(0.0), XmlValue::double(7.0)]
        ));
        assert!(!indexed_eq(
            &context,
            &[XmlValue::double(f64::NAN), XmlValue::double(5.0)],
            &[XmlValue::double(f64::NAN), XmlValue::double(7.0)]
        ));
    }

    #[test]
    fn integers_beyond_double_precision_stay_distinct() {
        let names = NameTable::new();
        let context = XPathContext::new(&names);
        let a = XmlValue::integer(BigInt::from(9007199254740993i64));
        let b = XmlValue::integer(BigInt::from(9007199254740992i64));
        // As xs:integer they differ …
        assert!(!indexed_eq(
            &context,
            &[a.clone(), XmlValue::integer(BigInt::from(1))],
            &[b.clone(), XmlValue::integer(BigInt::from(2))]
        ));
        // … but both equal the same xs:double, because the comparison promotes.
        assert!(indexed_eq(
            &context,
            &[a, XmlValue::integer(BigInt::from(1))],
            &[XmlValue::double(9007199254740992.0), XmlValue::double(2.0)]
        ));
    }

    #[test]
    fn a_timezoned_and_an_untimezoned_date_compare_after_normalization() {
        let names = NameTable::new();
        let context = XPathContext::new(&names);
        let utc = typed(
            XmlTypeCode::DateTime,
            XmlAtomicValue::DateTime(DateTimeValue {
                year: 2000,
                month: 1,
                day: 1,
                hour: 0,
                minute: 0,
                second: Decimal::ZERO,
                timezone: Some(TimezoneOffset(0)),
            }),
        );
        let plus_one = typed(
            XmlTypeCode::DateTime,
            XmlAtomicValue::DateTime(DateTimeValue {
                year: 2000,
                month: 1,
                day: 1,
                hour: 1,
                minute: 0,
                second: Decimal::ZERO,
                timezone: Some(TimezoneOffset(60)),
            }),
        );
        let other = typed(
            XmlTypeCode::DateTime,
            XmlAtomicValue::DateTime(DateTimeValue {
                year: 1999,
                month: 1,
                day: 1,
                hour: 0,
                minute: 0,
                second: Decimal::ZERO,
                timezone: Some(TimezoneOffset(0)),
            }),
        );
        assert!(indexed_eq(
            &context,
            &[utc, other.clone()],
            &[plus_one, other]
        ));
    }

    #[test]
    fn the_three_duration_types_share_one_comparison_class() {
        let names = NameTable::new();
        let context = XPathContext::new(&names);
        let one_year = typed(
            XmlTypeCode::Duration,
            XmlAtomicValue::Duration(DurationValue {
                negative: false,
                years: 1,
                months: 0,
                days: 0,
                hours: 0,
                minutes: 0,
                seconds: Decimal::ZERO,
            }),
        );
        let twelve_months = typed(
            XmlTypeCode::YearMonthDuration,
            XmlAtomicValue::YearMonthDuration(YearMonthDurationValue {
                negative: false,
                years: 0,
                months: 12,
            }),
        );
        let one_day = typed(
            XmlTypeCode::DayTimeDuration,
            XmlAtomicValue::DayTimeDuration(DayTimeDurationValue {
                negative: false,
                days: 1,
                hours: 0,
                minutes: 0,
                seconds: Decimal::ZERO,
            }),
        );
        assert!(indexed_eq(
            &context,
            &[one_year, one_day.clone()],
            &[twelve_months, one_day]
        ));
    }

    /// XPath 2.0 §3.5.1: "A eq B xs:NOTATION xs:NOTATION op:NOTATION-equal(A,
    /// B)", which, like `op:QName-equal`, ignores the prefixes; and a NOTATION
    /// never meets a QName (no row of the table).
    #[test]
    fn notation_equality_ignores_the_prefix() {
        let names = NameTable::new();
        let context = XPathContext::new(&names);
        let ns = names.add("urn:n");
        let gif = names.add("gif");
        let png = names.add("png");
        let jpg = names.add("jpg");
        let (n, m) = (names.add("n"), names.add("m"));
        let notation = |local, prefix| {
            typed(
                XmlTypeCode::Notation,
                XmlAtomicValue::Notation(QNameValue::new(
                    QualifiedName::new(Some(ns), local, Some(prefix)),
                    &names,
                )),
            )
        };
        let mut left = vec![notation(png, n); 70];
        left.push(notation(gif, n));
        let mut right = vec![notation(jpg, m); 70];
        right.push(notation(gif, m));
        assert!(indexed_eq(&context, &left, &right));
        assert!(!indexed_eq(&context, &left[..70], &right));

        let qname = typed(
            XmlTypeCode::QName,
            XmlAtomicValue::QName(QNameValue::new(
                QualifiedName::new(Some(ns), gif, Some(n)),
                &names,
            )),
        );
        let message = indexed_eq_error(&context, &[qname], &[notation(gif, n)]);
        assert!(message.contains("XPTY0004"), "{message}");
    }

    #[test]
    fn qname_equality_ignores_the_prefix() {
        let names = NameTable::new();
        let context = XPathContext::new(&names);
        let ns = names.add("http://example.org/ns");
        let local = names.add("a");
        let other_local = names.add("z");
        let prefix = names.add("p");
        let prefixed = typed(
            XmlTypeCode::QName,
            XmlAtomicValue::QName(QNameValue::new(
                QualifiedName::new(Some(ns), local, Some(prefix)),
                &names,
            )),
        );
        let bare = typed(
            XmlTypeCode::QName,
            XmlAtomicValue::QName(QNameValue::new(
                QualifiedName::new(Some(ns), local, None),
                &names,
            )),
        );
        let other = typed(
            XmlTypeCode::QName,
            XmlAtomicValue::QName(QNameValue::new(
                QualifiedName::new(Some(ns), other_local, None),
                &names,
            )),
        );
        assert!(indexed_eq(
            &context,
            &[prefixed, other.clone()],
            &[bare, other]
        ));
    }

    #[test]
    fn binary_values_compare_within_their_own_type() {
        let names = NameTable::new();
        let context = XPathContext::new(&names);
        let hex_a = typed(
            XmlTypeCode::HexBinary,
            XmlAtomicValue::HexBinary(vec![1, 2]),
        );
        let hex_b = typed(
            XmlTypeCode::HexBinary,
            XmlAtomicValue::HexBinary(vec![3, 4]),
        );
        assert!(indexed_eq(
            &context,
            &[hex_a.clone(), hex_b.clone()],
            &[hex_b.clone(), hex_a.clone()]
        ));
        let b64 = typed(
            XmlTypeCode::Base64Binary,
            XmlAtomicValue::Base64Binary(vec![1, 2]),
        );
        // xs:hexBinary and xs:base64Binary are not comparable.
        let message = indexed_eq_error(&context, &[hex_a, hex_b], &[b64.clone(), b64]);
        assert!(message.contains("op:eq"), "unexpected error: {message}");
    }

    #[test]
    fn booleans_compare_with_booleans_only() {
        let names = NameTable::new();
        let context = XPathContext::new(&names);
        assert!(indexed_eq(
            &context,
            &[XmlValue::boolean(false), XmlValue::boolean(true)],
            &[XmlValue::boolean(true), XmlValue::boolean(true)]
        ));
        let message = indexed_eq_error(
            &context,
            &[XmlValue::boolean(true), XmlValue::boolean(false)],
            &[XmlValue::string("true"), XmlValue::string("false")],
        );
        assert!(message.contains("op:eq"), "unexpected error: {message}");
    }

    #[test]
    fn a_true_pair_wins_over_an_incomparable_pair() {
        let names = NameTable::new();
        let context = XPathContext::new(&names);
        // (boolean, string) is incomparable, but the second row has a true pair.
        assert!(indexed_eq(
            &context,
            &[XmlValue::boolean(true), XmlValue::string("a")],
            &[XmlValue::string("z"), XmlValue::string("a")]
        ));
    }

    #[test]
    fn an_empty_operand_makes_the_comparison_false() {
        let names = NameTable::new();
        let context = XPathContext::new(&names);
        assert!(!indexed_eq(
            &context,
            &[],
            &[XmlValue::string("a"), XmlValue::string("b")]
        ));
        assert!(!indexed_eq(
            &context,
            &[XmlValue::string("a"), XmlValue::string("b")],
            &[]
        ));
    }

    #[test]
    fn an_uncastable_untyped_value_against_a_numeric_value_is_left_to_the_pairwise_path() {
        let names = NameTable::new();
        let context = XPathContext::new(&names);
        let left = [XmlValue::untyped("not-a-number"), XmlValue::untyped("1")];
        let right = [
            XmlValue::integer(BigInt::from(1)),
            XmlValue::integer(BigInt::from(2)),
        ];
        let left_iter = iter_of(&left);
        let right_buf = BufferedNodeIterator::preload(iter_of(&right)).unwrap();
        let active = crate::xpath::collation::resolve_default(&context);
        let result = try_indexed_eq(
            &context,
            &left_iter,
            &right_buf,
            active.as_ref(),
            crate::xpath::operators::static_timezone(&context),
        );
        assert!(
            result.is_none(),
            "a failing cast must hand the comparison back to the pairwise loop"
        );
    }

    /// The public entry points combine a bounded pairwise prefix, the index and
    /// the pairwise fallback. Run sequences long enough to get past the prefix
    /// through both, and check that the combination answers exactly as the
    /// plain Cartesian-product loop does.
    #[test]
    fn the_public_comparison_agrees_with_the_pairwise_path() {
        let names = NameTable::new();
        let context = XPathContext::new(&names);
        let all: Vec<XmlValue> = families(&names).into_iter().flatten().collect();
        let mut rng = Rng(0xDEAD_BEEF_CAFE_F00D);
        for _ in 0..400 {
            // 12 x 12 = 144 pairs, comfortably past the 64-pair prefix.
            let left: Vec<XmlValue> = (0..12).map(|_| all[rng.below(all.len())].clone()).collect();
            let right: Vec<XmlValue> = (0..12).map(|_| all[rng.below(all.len())].clone()).collect();
            let left_iter = iter_of(&left);
            let right_iter = iter_of(&right);
            let right_buf = BufferedNodeIterator::preload(right_iter.clone()).unwrap();

            let expected = general_eq_iter_pairwise(&context, &left_iter, &right_buf);
            let actual =
                crate::xpath::operators::general_eq_iter(&context, &left_iter, &right_iter);
            assert_eq!(render(&expected), render(&actual), "`=` diverged");

            let expected = general_ne_iter_pairwise(&context, &left_iter, &right_buf);
            let actual =
                crate::xpath::operators::general_ne_iter(&context, &left_iter, &right_iter);
            assert_eq!(render(&expected), render(&actual), "`!=` diverged");
        }
    }
}
