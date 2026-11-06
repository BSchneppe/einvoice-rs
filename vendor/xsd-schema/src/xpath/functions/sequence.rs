//! XPath 2.0 sequence functions.
//!
//! This module implements sequence functions from the XPath 2.0 specification:
//! - fn:index-of
//! - fn:remove
//! - fn:insert-before
//! - fn:subsequence
//! - fn:unordered
//! - fn:deep-equal

use num_bigint::BigInt;
use rust_decimal::prelude::ToPrimitive;

use crate::types::value::{TimezoneOffset, XmlAtomicValue, XmlValue};
use crate::types::XmlTypeCode;
use crate::xpath::collation::{ActiveCollation, CollationRef};
use crate::xpath::context::DynamicContext;
use crate::xpath::error::XPathError;
use crate::xpath::iterator::{VecNodeIterator, XmlItem};
use crate::xpath::tree_comparer::NodeComparer;
use crate::xpath::DomNavigator;

use super::numeric::round_half_toward_positive_infinity_f64;
use super::{
    atomize_sequence, atomize_to_single, atomize_to_single_opt, atomize_to_string_opt, convert,
    materialize, XPathValue,
};

/// The collation of a call: its `$collation` argument if it has one, and
/// otherwise the static context's default collation. The argument is taken off
/// the back of `args`.
///
/// `require` decides when a URI nothing supports becomes FOCH0002. These three
/// functions use the collation only "when string comparison is required"
/// (F&O §15.1.5, §15.1.6, §15.3.1), so a *defaulted* collation is carried into
/// the comparison and raises there — `distinct-values((1, 2, 3))` under an
/// unsupported default collation compares no strings and must not fail. An
/// **explicit** argument is different: the caller named a collation for this
/// call, so an unsupported one is reported at once, which is also what
/// `fn:deep-equal` has always done with it.
fn call_collation<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    args: &mut Vec<XPathValue<N>>,
    has_collation_arg: bool,
    require: bool,
) -> Result<ActiveCollation, XPathError> {
    let explicit = if has_collation_arg {
        atomize_to_string_opt(args.pop().unwrap())?
    } else {
        None
    };
    let active = crate::xpath::collation::resolve_collation_cached(context, explicit.as_deref());
    if require {
        return active.require();
    }
    Ok(active)
}

/// Apply the function conversion rules (XPath 2.0 §3.1.5) for an `xs:integer`
/// parameter and hand back the value as an `i64` position.
fn require_integer(value: XmlValue, function: &str) -> Result<i64, XPathError> {
    let value = convert::expect_atomic_as(value, XmlTypeCode::Integer, function)?;
    value
        .as_integer()
        .and_then(|i| i.to_i64())
        .ok_or_else(|| XPathError::XPTY0004 {
            expected: "xs:integer".to_string(),
            found: format!("{:?}", value.type_code),
        })
}

// ============================================================================
// fn:index-of($seq as xs:anyAtomicType*, $search as xs:anyAtomicType,
//             $collation as xs:string?) as xs:integer*
// ============================================================================

/// Implements fn:index-of - returns positions of matching items in a sequence.
///
/// Returns a sequence of positive integers giving the positions of items in $seq
/// that are equal to $search. F&O §15.1.5 compares them under the rules of the
/// `eq` operator and adds that "Values that cannot be compared, i.e. the eq
/// operator is not defined for their types, are considered to be distinct" — so
/// an incomparable pair is a non-match, never an error. `$search` is *not*
/// converted to the type of the sequence: it is an `xs:anyAtomicType`
/// parameter, and under `eq` an `xs:untypedAtomic` operand is cast to
/// `xs:string` (XPath 2.0 §3.5.1), which is what `values_equal` does. An
/// untyped `2` therefore matches no `xs:integer` and `index-of((1, 2, 3), $u)`
/// is the empty sequence.
///
/// The collation — the `$collation` argument, or the static context's default —
/// decides the string comparisons and nothing else, which is F&O §15.1.5's
/// "the collation is used when string comparison is required".
pub fn index_of<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(XPathError::wrong_number_of_arguments(
            "index-of",
            2,
            args.len(),
        ));
    }

    let has_collation_arg = args.len() == 3;
    let collation = call_collation(context, &mut args, has_collation_arg, has_collation_arg)?;
    let tz = context.implicit_timezone_or_host();

    // Get the sequence (arg 0) and search value (arg 1)
    let seq = args.remove(0);
    let search_arg = args.remove(0);

    // Atomize both
    let seq_values = atomize_sequence(seq)?;
    let search_value = match atomize_to_single_opt(search_arg)? {
        None => return Ok(XPathValue::Empty),
        Some(value) => value,
    };

    // Find matching positions (1-based)
    let mut positions = Vec::new();
    for (idx, item) in seq_values.iter().enumerate() {
        if values_equal(item, &search_value, collation.as_ref(), tz)? {
            positions.push(XmlItem::Atomic(XmlValue::integer(BigInt::from(idx + 1))));
        }
    }

    Ok(XPathValue::from_sequence(positions))
}

/// Compare two atomic values for equality (used by index-of and distinct-values).
///
/// F&O §15.1.5 (`fn:index-of`): "The items in the sequence $seqParam are
/// compared with $srchParam under the rules for the eq operator. Values of
/// type xs:untypedAtomic are compared as if they were of type xs:string.
/// Values that cannot be compared, i.e. the eq operator is not defined for
/// their types, are considered to be distinct." So this is the `eq` operator
/// itself ([`value_eq_collated`](crate::xpath::operators::value_eq_collated)),
/// under `collation`, with a pair `eq` is not defined for — an `xs:integer`
/// against an `xs:string`, say — answering `false` instead of raising. Two
/// string-like values compare by their string values whatever their types:
/// `xs:token('a')`, an `xs:NMTOKEN` list member and an `xs:ID` all equal the
/// `xs:string` `'a'`, as they do under `eq`.
///
/// Numeric pairs keep the exact promotion below (decimal↔integer compared as
/// decimals, float and double promoted), which is `eq`'s as well.
///
/// The only error this can raise is therefore FOCH0002 from an unsupported
/// collation, and only for a pair that actually needed one.
fn values_equal(
    left: &XmlValue,
    right: &XmlValue,
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
) -> Result<bool, XPathError> {
    let left_norm = normalize_for_comparison(left);
    let right_norm = normalize_for_comparison(right);

    // Numeric type promotion: compare numerics as doubles
    if left_norm.type_code.is_numeric() && right_norm.type_code.is_numeric() {
        return Ok(numeric_values_equal(&left_norm, &right_norm));
    }

    eq_or_distinct(&left_norm, &right_norm, collation, tz)
}

/// `left eq right` under `collation`, where a pair the `eq` operator is not
/// defined for is simply unequal (F&O §15.1.5, §15.1.6: "considered to be
/// distinct"). Any other error — FOCH0002 for an unsupported collation — is
/// raised.
fn eq_or_distinct(
    left: &XmlValue,
    right: &XmlValue,
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
) -> Result<bool, XPathError> {
    match crate::xpath::operators::value_eq_collated(left, right, collation, tz) {
        Ok(equal) => Ok(equal),
        Err(XPathError::BinaryOperatorNotDefined { .. }) => Ok(false),
        Err(err) => Err(err),
    }
}

/// Compare two numeric values for equality using XPath 2.0 type promotion.
/// Promotion: decimal→float→double. Uses the least common type.
/// NaN is not equal to NaN for value comparison per XPath spec.
fn numeric_values_equal(left: &XmlValue, right: &XmlValue) -> bool {
    numeric_values_equal_inner(left, right, false)
}

/// Inner implementation for numeric equality comparison.
/// If `nan_equal` is true, NaN == NaN (used by fn:distinct-values).
fn numeric_values_equal_inner(left: &XmlValue, right: &XmlValue, nan_equal: bool) -> bool {
    let lt = left.type_code;
    let rt = right.type_code;
    let is_decimal_or_int =
        |tc: XmlTypeCode| tc.is_numeric() && tc != XmlTypeCode::Float && tc != XmlTypeCode::Double;

    // If either operand is xs:double, promote both to double
    if lt == XmlTypeCode::Double || rt == XmlTypeCode::Double {
        if let (Some(l), Some(r)) = (left.as_double(), right.as_double()) {
            if nan_equal && l.is_nan() && r.is_nan() {
                return true;
            }
            if l.is_nan() || r.is_nan() {
                return false;
            }
            return l == r;
        }
        return false;
    }

    // If either operand is xs:float, promote the other to float
    if lt == XmlTypeCode::Float || rt == XmlTypeCode::Float {
        // An xs:decimal carried in lexical form (beyond `rust_decimal`) has
        // no `Decimal`; it promotes to its nearest float exactly the same way.
        let as_f32 = |v: &XmlValue| match &v.value {
            crate::types::value::XmlValueKind::Atomic(XmlAtomicValue::Float(f)) => Some(*f),
            _ => v.as_decimal().and_then(|d| d.to_f32()).or_else(|| {
                crate::types::exact_decimal::ExactDecimal::from_value(v).map(|d| d.to_f32())
            }),
        };
        let lf = as_f32(left);
        let rf = as_f32(right);
        if let (Some(l), Some(r)) = (lf, rf) {
            if nan_equal && l.is_nan() && r.is_nan() {
                return true;
            }
            if l.is_nan() || r.is_nan() {
                return false;
            }
            return l == r;
        }
        return false;
    }

    // Both are decimal/integer — compare as Decimal (exact), or by exact
    // value when one is beyond what `Decimal` holds (an xs:decimal carried in
    // lexical form, an xs:integer past `Decimal::MAX`).
    if is_decimal_or_int(lt) && is_decimal_or_int(rt) {
        if let (Some(l), Some(r)) = (left.as_decimal(), right.as_decimal()) {
            return l == r;
        }
        return crate::types::exact_decimal::decimal_values_equal(left, right) == Some(true);
    }

    false
}

/// Compare two values for equality for fn:distinct-values.
///
/// F&O §15.1.6: "Equality must be defined for the type of the items … The
/// values are compared using the eq operator" with "xs:untypedAtomic …
/// compared as if they were of type xs:string", "Values that cannot be compared,
/// i.e. the eq operator is not defined for their types, are considered to be
/// distinct", and — unlike `eq` — NaN equal to NaN. So this is
/// [`values_equal`] except for the NaN rule.
fn distinct_values_equal(
    left: &XmlValue,
    right: &XmlValue,
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
) -> Result<bool, XPathError> {
    let left_norm = normalize_for_comparison(left);
    let right_norm = normalize_for_comparison(right);

    // Numeric type promotion with NaN == NaN for fn:distinct-values
    if left_norm.type_code.is_numeric() && right_norm.type_code.is_numeric() {
        return Ok(numeric_values_equal_inner(&left_norm, &right_norm, true));
    }

    // Everything else is `eq`: durations compare across their subtypes
    // (`P0M eq PT0S`, `P12M eq P1Y`), dates and times by their timeline value,
    // QNames by expanded name, strings under the collation; a date/time value
    // without a timezone in the implicit timezone (F&O §15.1.8 compares "using
    // the eq operator").
    eq_or_distinct(&left_norm, &right_norm, collation, tz)
}

/// Normalize a value for comparison (UntypedAtomic and AnyUri become string).
fn normalize_for_comparison(value: &XmlValue) -> XmlValue {
    match value.type_code {
        XmlTypeCode::UntypedAtomic | XmlTypeCode::AnyUri => {
            XmlValue::string(value.to_string_value())
        }
        _ => value.clone(),
    }
}

// ============================================================================
// fn:reverse($arg as item()*) as item()*
// ============================================================================

/// Implements fn:reverse - reverses the order of items in a sequence.
pub fn reverse<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "reverse",
            1,
            args.len(),
        ));
    }

    let mut items = materialize(args.remove(0));
    items.reverse();
    Ok(XPathValue::from_sequence(items))
}

// ============================================================================
// fn:zero-or-one($arg as item()*) as item()?
// ============================================================================

/// Implements fn:zero-or-one - returns the argument if it contains zero or one items.
///
/// Raises FORG0003 if the argument contains more than one item.
pub fn zero_or_one<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "zero-or-one",
            1,
            args.len(),
        ));
    }

    let arg = args.remove(0);
    if arg.len() > 1 {
        return Err(XPathError::FORG0003);
    }
    Ok(arg)
}

// ============================================================================
// fn:one-or-more($arg as item()*) as item()+
// ============================================================================

/// Implements fn:one-or-more - returns the argument if it contains one or more items.
///
/// Raises FORG0004 if the argument is an empty sequence.
pub fn one_or_more<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "one-or-more",
            1,
            args.len(),
        ));
    }

    let arg = args.remove(0);
    if arg.is_empty() {
        return Err(XPathError::FORG0004);
    }
    Ok(arg)
}

// ============================================================================
// fn:exactly-one($arg as item()*) as item()
// ============================================================================

/// Implements fn:exactly-one - returns the argument if it contains exactly one item.
///
/// Raises FORG0005 if the argument does not contain exactly one item.
pub fn exactly_one<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "exactly-one",
            1,
            args.len(),
        ));
    }

    let arg = args.remove(0);
    if arg.len() != 1 {
        return Err(XPathError::FORG0005);
    }
    Ok(arg)
}

// ============================================================================
// fn:distinct-values($arg as xs:anyAtomicType*, $collation as xs:string?) as xs:anyAtomicType*
// ============================================================================

/// Implements fn:distinct-values - returns unique values from a sequence.
///
/// Returns the values that appear in the argument with duplicates removed.
/// Uses value equality with numeric type promotion.
///
/// The collation — the `$collation` argument, or the static context's default —
/// decides which strings count as duplicates, and governs nothing else
/// (F&O §15.1.6).
pub fn distinct_values<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.is_empty() || args.len() > 2 {
        return Err(XPathError::wrong_number_of_arguments(
            "distinct-values",
            1,
            args.len(),
        ));
    }

    let has_collation_arg = args.len() == 2;
    let collation = call_collation(context, &mut args, has_collation_arg, has_collation_arg)?;
    let tz = context.implicit_timezone_or_host();
    let seq = args.remove(0);

    // Atomize the sequence
    let values = atomize_sequence(seq)?;

    if values.is_empty() {
        return Ok(XPathValue::Empty);
    }

    // Remove duplicates using distinct_values_equal for comparison
    // (treats NaN as equal to NaN, unlike value comparison)
    let mut distinct: Vec<XmlValue> = Vec::new();
    for value in values {
        let mut is_duplicate = false;
        for existing in &distinct {
            if distinct_values_equal(existing, &value, collation.as_ref(), tz)? {
                is_duplicate = true;
                break;
            }
        }
        if !is_duplicate {
            distinct.push(value);
        }
    }

    // Convert back to XPathValue
    let items: Vec<XmlItem<N>> = distinct.into_iter().map(XmlItem::Atomic).collect();

    Ok(XPathValue::from_sequence(items))
}

// ============================================================================
// fn:remove($target as item()*, $position as xs:integer) as item()*
// ============================================================================

/// Implements fn:remove - removes an item from a sequence at a given position.
///
/// Returns a new sequence with the item at the specified position removed.
/// If $position is less than 1 or greater than the length of $target,
/// the sequence is returned unchanged.
pub fn remove<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 2 {
        return Err(XPathError::wrong_number_of_arguments(
            "remove",
            2,
            args.len(),
        ));
    }

    let target = args.remove(0);
    let position_arg = args.remove(0);

    // Get position as integer (function conversion rules, XPath 2.0 §3.1.5)
    let position = require_integer(atomize_to_single(position_arg)?, "remove")?;

    // Materialize target sequence
    let mut items = materialize(target);

    // If position is out of range, return sequence unchanged
    if position < 1 || position as usize > items.len() {
        return Ok(XPathValue::from_sequence(items));
    }

    // Remove item at position (convert 1-based to 0-based)
    items.remove((position - 1) as usize);

    Ok(XPathValue::from_sequence(items))
}

// ============================================================================
// fn:insert-before($target as item()*, $position as xs:integer,
//                  $inserts as item()*) as item()*
// ============================================================================

/// Implements fn:insert-before - inserts items into a sequence.
///
/// Returns a new sequence with $inserts inserted before the item at $position.
/// If $position < 1, inserts at the beginning.
/// If $position > length + 1, inserts at the end.
pub fn insert_before<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 3 {
        return Err(XPathError::wrong_number_of_arguments(
            "insert-before",
            3,
            args.len(),
        ));
    }

    let target = args.remove(0);
    let position_arg = args.remove(0);
    let inserts = args.remove(0);

    // Get position as integer (function conversion rules, XPath 2.0 §3.1.5)
    let position = require_integer(atomize_to_single(position_arg)?, "insert-before")?;

    // Materialize both sequences
    let mut target_items = materialize(target);
    let insert_items = materialize(inserts);

    // Adjust position: if < 1, use 1; if > len+1, use len+1
    let len = target_items.len();
    let adjusted_pos = if position < 1 {
        0
    } else if position as usize > len {
        len
    } else {
        (position - 1) as usize
    };

    // Build result: items[0..pos] + inserts + items[pos..]
    let mut result = Vec::with_capacity(target_items.len() + insert_items.len());
    result.extend(target_items.drain(..adjusted_pos));
    result.extend(insert_items);
    result.extend(target_items);

    Ok(XPathValue::from_sequence(result))
}

/// The `xs:double` of an argument declared `as xs:double`: one item, the
/// empty sequence being XPTY0004.
fn required_double<N: DomNavigator>(
    value: XPathValue<N>,
    parameter: &str,
) -> Result<f64, XPathError> {
    match atomize_to_single_opt(value)? {
        Some(atomic) => Ok(crate::xpath::atomize::to_number(&atomic)),
        None => Err(XPathError::XPTY0004 {
            expected: format!("xs:double for {parameter} of fn:subsequence"),
            found: "empty-sequence()".to_string(),
        }),
    }
}

// ============================================================================
// fn:subsequence($sourceSeq as item()*, $startingLoc as xs:double,
//                $length as xs:double?) as item()*
// ============================================================================

/// Implements fn:subsequence - returns a contiguous subsequence.
///
/// Returns items from $sourceSeq starting at position $startingLoc
/// and continuing for $length items (or to the end if $length is omitted).
///
/// F&O §15.1.10 defines the result as the items whose position `p` satisfies
/// `p >= fn:round($startingLoc)` and `p < fn:round($startingLoc) +
/// fn:round($length)`, so both arguments are rounded by `fn:round` — half
/// towards positive infinity, the same helper `fn:round` itself uses. A
/// fractional `$startingLoc` of -1.5 therefore begins at -1, not -2.
///
/// The remaining rules on the arguments, which are `xs:double`:
/// - NaN startingLoc or length -> empty
/// - +Infinity startingLoc -> empty
/// - -Infinity length -> empty
/// - the empty sequence for either -> XPTY0004 (outside XPath 1.0
///   compatibility mode, whose function conversion rules make it NaN)
pub fn subsequence<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.is_empty() || args.len() > 3 {
        return Err(XPathError::wrong_number_of_arguments(
            "subsequence",
            2,
            args.len(),
        ));
    }

    let source = args.remove(0);
    let starting_loc_arg = args.remove(0);
    let length_arg = if !args.is_empty() {
        Some(args.remove(0))
    } else {
        None
    };

    // F&O §15.1.10: `$startingLoc as xs:double` and `$length as xs:double`
    // take exactly one value — the empty sequence is a type error (XPath 2.0
    // §3.1.5, XPTY0004). In XPath 1.0 compatibility mode the function
    // conversion rules have already applied `fn:number`, so an empty argument
    // arrives here as NaN.
    let starting_loc = required_double(starting_loc_arg, "$startingLoc")?;

    // Get optional length as double
    let length = match length_arg {
        Some(arg) => Some(required_double(arg, "$length")?),
        None => None,
    };

    // Materialize source sequence
    let items = materialize(source);

    // F&O §15.1.10 defines the result as the items whose position `p`
    // satisfies `fn:round($startingLoc) <= p` and
    // `p < fn:round($startingLoc) + fn:round($length)`, with both comparisons
    // evaluated in xs:double arithmetic. Keeping the arithmetic in f64 makes
    // the infinite and NaN arguments fall out of the same two comparisons:
    // `-INF` to `+INF` sums to NaN, and every comparison with NaN is false, so
    // the result is empty.
    let start = round_half_toward_positive_infinity_f64(starting_loc);
    let end_exclusive = length.map(|len| start + round_half_toward_positive_infinity_f64(len));

    if start.is_nan() || end_exclusive.is_some_and(f64::is_nan) {
        return Ok(XPathValue::Empty);
    }

    let count = items.len() as f64;
    // Both bounds are integer-valued (or infinite), so `p < end` is
    // `p <= end - 1` over the integer positions.
    let first_position = start.max(1.0);
    let last_position = match end_exclusive {
        Some(end) => (end - 1.0).min(count),
        None => count,
    };
    if first_position > count || first_position > last_position {
        return Ok(XPathValue::Empty);
    }
    let start_idx = (first_position - 1.0) as usize;
    let end_idx = last_position as usize;

    // Extract subsequence
    let result: Vec<XmlItem<N>> = items
        .into_iter()
        .skip(start_idx)
        .take(end_idx.saturating_sub(start_idx))
        .collect();

    Ok(XPathValue::from_sequence(result))
}

// ============================================================================
// fn:unordered($sourceSeq as item()*) as item()*
// ============================================================================

/// Implements fn:unordered - returns the sequence in implementation-defined order.
///
/// This is an optimization hint; implementations may return items in any order.
/// Our implementation simply returns the input unchanged.
pub fn unordered<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "unordered",
            1,
            args.len(),
        ));
    }

    // Simply return the input unchanged
    Ok(args.remove(0))
}

// ============================================================================
// fn:deep-equal($parameter1 as item()*, $parameter2 as item()*,
//               $collation as xs:string?) as xs:boolean
// ============================================================================

/// Implements fn:deep-equal - tests whether two sequences are deep-equal.
///
/// Two sequences are deep-equal if they have the same length and each pair
/// of corresponding items are deep-equal.
pub fn deep_equal<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(XPathError::wrong_number_of_arguments(
            "deep-equal",
            2,
            args.len(),
        ));
    }

    // `fn:deep-equal` always requires its collation, defaulted or not: the
    // comparison engine behind it answers `bool` at every level and has nowhere
    // to carry an error to, so a collation it cannot use must be refused before
    // the walk starts rather than silently become the codepoint one.
    let has_collation_arg = args.len() == 3;
    let collation = call_collation(context, &mut args, has_collation_arg, true)?;
    let tz = context.implicit_timezone_or_host();

    let param1 = args.remove(0);
    let param2 = args.remove(0);

    // Materialize both sequences
    let items1 = materialize(param1);
    let items2 = materialize(param2);

    // Create VecNodeIterators for comparison
    let iter1: VecNodeIterator<N> = VecNodeIterator::new(items1);
    let iter2: VecNodeIterator<N> = VecNodeIterator::new(items2);

    // The `fn:deep-equal` content model (F&O §15.3.1), not the stricter
    // comparison the published `TreeComparer` performs: comment and
    // processing-instruction *children* play no part, and an element is
    // compared according to its content — typed value for simple content,
    // child elements only for element-only content, `(*|text())` for mixed.
    // The static context's schema set is what makes the content kind of a
    // complex type readable; with none, every element is `xs:untyped`, hence
    // mixed, which is the unvalidated behaviour.
    //
    // The collation travels with the comparer: F&O §15.3.1 uses it wherever the
    // rule compares two strings — text and comment nodes, processing-instruction
    // contents, typed values of elements and attributes, and free-standing
    // atomic items — "but not when names are compared", so element, attribute
    // and PI *names* stay codepoint comparisons.
    let comparer = NodeComparer::deep_equal_function(
        context.static_context.schema_set,
        collation.as_ref(),
        tz,
    );
    let result = comparer.deep_equal_iter(&iter1, &iter2)?;

    Ok(XPathValue::boolean(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespace::table::NameTable;
    use crate::xpath::context::XPathContext;
    use crate::xpath::iterator::XmlItem;
    use crate::xpath::RoXmlNavigator;

    fn make_context<'a>() -> DynamicContext<'a, RoXmlNavigator<'a>> {
        let table = Box::leak(Box::new(NameTable::new()));
        let xpath_ctx = Box::leak(Box::new(XPathContext::new(table)));
        DynamicContext::new(xpath_ctx, 0)
    }

    fn integer_seq<N: DomNavigator>(values: &[i64]) -> XPathValue<N> {
        let items: Vec<XmlItem<N>> = values
            .iter()
            .map(|&v| XmlItem::Atomic(XmlValue::integer(BigInt::from(v))))
            .collect();
        XPathValue::from_sequence(items)
    }

    fn extract_integers<N: DomNavigator>(value: XPathValue<N>) -> Vec<i64> {
        match value {
            XPathValue::Empty => vec![],
            XPathValue::Item(item) => {
                if let XmlItem::Atomic(v) = item {
                    vec![v.as_integer().and_then(|i| i.to_i64()).unwrap()]
                } else {
                    vec![]
                }
            }
            XPathValue::Sequence(items) => items
                .into_iter()
                .filter_map(|item| {
                    if let XmlItem::Atomic(v) = item {
                        v.as_integer().and_then(|i| i.to_i64())
                    } else {
                        None
                    }
                })
                .collect(),
        }
    }

    fn extract_bool<N: DomNavigator>(value: XPathValue<N>) -> bool {
        match value {
            XPathValue::Item(XmlItem::Atomic(v)) => v.as_boolean().unwrap_or(false),
            _ => false,
        }
    }

    // ========== index-of tests ==========

    #[test]
    fn test_index_of_multiple_matches() {
        let mut ctx = make_context();
        let seq = integer_seq::<RoXmlNavigator>(&[10, 20, 30, 20]);
        let search = XPathValue::integer(20);
        let args = vec![seq, search];
        let result = index_of(&mut ctx, args).unwrap();
        assert_eq!(extract_integers(result), vec![2, 4]);
    }

    #[test]
    fn test_index_of_no_match() {
        let mut ctx = make_context();
        let seq = integer_seq::<RoXmlNavigator>(&[10, 20, 30]);
        let search = XPathValue::integer(40);
        let args = vec![seq, search];
        let result = index_of(&mut ctx, args).unwrap();
        assert_eq!(extract_integers(result), Vec::<i64>::new());
    }

    #[test]
    fn test_index_of_string_matches() {
        let mut ctx = make_context();
        let items: Vec<XmlItem<RoXmlNavigator>> = vec!["a", "b", "c", "b"]
            .into_iter()
            .map(|s| XmlItem::Atomic(XmlValue::string(s)))
            .collect();
        let seq = XPathValue::from_sequence(items);
        let search = XPathValue::string("b");
        let args = vec![seq, search];
        let result = index_of(&mut ctx, args).unwrap();
        assert_eq!(extract_integers(result), vec![2, 4]);
    }

    #[test]
    fn test_index_of_empty_sequence() {
        let mut ctx = make_context();
        let seq = XPathValue::<RoXmlNavigator>::Empty;
        let search = XPathValue::integer(1);
        let args = vec![seq, search];
        let result = index_of(&mut ctx, args).unwrap();
        assert!(result.is_empty());
    }

    // ========== remove tests ==========

    #[test]
    fn test_remove_middle() {
        let mut ctx = make_context();
        let seq = integer_seq::<RoXmlNavigator>(&[1, 2, 3]);
        let pos = XPathValue::integer(2);
        let args = vec![seq, pos];
        let result = remove(&mut ctx, args).unwrap();
        assert_eq!(extract_integers(result), vec![1, 3]);
    }

    #[test]
    fn test_remove_out_of_range_low() {
        let mut ctx = make_context();
        let seq = integer_seq::<RoXmlNavigator>(&[1, 2, 3]);
        let pos = XPathValue::integer(0);
        let args = vec![seq, pos];
        let result = remove(&mut ctx, args).unwrap();
        assert_eq!(extract_integers(result), vec![1, 2, 3]);
    }

    #[test]
    fn test_remove_out_of_range_high() {
        let mut ctx = make_context();
        let seq = integer_seq::<RoXmlNavigator>(&[1, 2, 3]);
        let pos = XPathValue::integer(10);
        let args = vec![seq, pos];
        let result = remove(&mut ctx, args).unwrap();
        assert_eq!(extract_integers(result), vec![1, 2, 3]);
    }

    // ========== insert-before tests ==========

    #[test]
    fn test_insert_before_middle() {
        let mut ctx = make_context();
        let target = integer_seq::<RoXmlNavigator>(&[1, 3]);
        let pos = XPathValue::integer(2);
        let inserts = XPathValue::integer(2);
        let args = vec![target, pos, inserts];
        let result = insert_before(&mut ctx, args).unwrap();
        assert_eq!(extract_integers(result), vec![1, 2, 3]);
    }

    #[test]
    fn test_insert_before_position_less_than_one() {
        let mut ctx = make_context();
        let target = integer_seq::<RoXmlNavigator>(&[2, 3]);
        let pos = XPathValue::integer(0);
        let inserts = XPathValue::integer(1);
        let args = vec![target, pos, inserts];
        let result = insert_before(&mut ctx, args).unwrap();
        assert_eq!(extract_integers(result), vec![1, 2, 3]);
    }

    #[test]
    fn test_insert_before_position_beyond_end() {
        let mut ctx = make_context();
        let target = integer_seq::<RoXmlNavigator>(&[1, 2]);
        let pos = XPathValue::integer(10);
        let inserts = XPathValue::integer(3);
        let args = vec![target, pos, inserts];
        let result = insert_before(&mut ctx, args).unwrap();
        assert_eq!(extract_integers(result), vec![1, 2, 3]);
    }

    // ========== subsequence tests ==========

    #[test]
    fn test_subsequence_with_length() {
        let mut ctx = make_context();
        let seq = integer_seq::<RoXmlNavigator>(&[1, 2, 3, 4, 5]);
        let start = XPathValue::double(2.0);
        let len = XPathValue::double(3.0);
        let args = vec![seq, start, len];
        let result = subsequence(&mut ctx, args).unwrap();
        assert_eq!(extract_integers(result), vec![2, 3, 4]);
    }

    #[test]
    fn test_subsequence_without_length() {
        let mut ctx = make_context();
        let seq = integer_seq::<RoXmlNavigator>(&[1, 2, 3, 4, 5]);
        let start = XPathValue::double(3.0);
        let args = vec![seq, start];
        let result = subsequence(&mut ctx, args).unwrap();
        assert_eq!(extract_integers(result), vec![3, 4, 5]);
    }

    #[test]
    fn test_subsequence_negative_start() {
        let mut ctx = make_context();
        let seq = integer_seq::<RoXmlNavigator>(&[1, 2, 3]);
        let start = XPathValue::double(-1.0);
        let len = XPathValue::double(4.0);
        let args = vec![seq, start, len];
        let result = subsequence(&mut ctx, args).unwrap();
        // start=-1, len=4: positions where -1 <= pos < 3, i.e., pos 1 and 2
        assert_eq!(extract_integers(result), vec![1, 2]);
    }

    #[test]
    fn test_subsequence_nan_start() {
        let mut ctx = make_context();
        let seq = integer_seq::<RoXmlNavigator>(&[1, 2, 3]);
        let start = XPathValue::double(f64::NAN);
        let len = XPathValue::double(2.0);
        let args = vec![seq, start, len];
        let result = subsequence(&mut ctx, args).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_subsequence_rounding() {
        let mut ctx = make_context();
        // subsequence((1,2,3,4,5), 1.5, 2.6) should round to start=2, len=3 -> items 2,3,4
        let seq = integer_seq::<RoXmlNavigator>(&[1, 2, 3, 4, 5]);
        let start = XPathValue::double(1.5);
        let len = XPathValue::double(2.6);
        let args = vec![seq, start, len];
        let result = subsequence(&mut ctx, args).unwrap();
        assert_eq!(extract_integers(result), vec![2, 3, 4]);
    }

    /// Evaluate `fn:subsequence` over `(1, 2, 3, 4, 5)`.
    fn subsequence_of_five(start: f64, length: Option<f64>) -> Vec<i64> {
        let mut ctx = make_context();
        let mut args = vec![
            integer_seq::<RoXmlNavigator>(&[1, 2, 3, 4, 5]),
            XPathValue::double(start),
        ];
        if let Some(length) = length {
            args.push(XPathValue::double(length));
        }
        extract_integers(subsequence(&mut ctx, args).expect("numeric arguments"))
    }

    /// A half `$startingLoc` or `$length` rounds towards positive infinity,
    /// because F&O §15.1.10 rounds both through `fn:round`.
    ///
    /// The spec's own arithmetic for `subsequence((1,2,3,4,5), -1.5, 4.5)`:
    /// `fn:round(-1.5)` is -1 and `fn:round(4.5)` is 5, so the result is the
    /// items whose position satisfies `p >= -1` and `p < 4` — items 1, 2 and 3.
    #[test]
    fn test_subsequence_takes_halves_toward_positive_infinity() {
        assert_eq!(subsequence_of_five(-1.5, Some(4.5)), vec![1, 2, 3]);
        // round(2.5) = 3: positions 3 to the end.
        assert_eq!(subsequence_of_five(2.5, None), vec![3, 4, 5]);
        // round(1.5) = 2 and round(2.5) = 3: 2 <= p < 5.
        assert_eq!(subsequence_of_five(1.5, Some(2.5)), vec![2, 3, 4]);
        // round(-2.5) = -2 and round(4.5) = 5: -2 <= p < 3.
        assert_eq!(subsequence_of_five(-2.5, Some(4.5)), vec![1, 2]);
    }

    /// The integral cases are unchanged, and stay exactly on the §15.1.10
    /// arithmetic.
    #[test]
    fn test_subsequence_integral_positions_are_unchanged() {
        assert_eq!(subsequence_of_five(2.0, Some(3.0)), vec![2, 3, 4]);
        assert_eq!(subsequence_of_five(3.0, None), vec![3, 4, 5]);
        assert_eq!(subsequence_of_five(1.0, None), vec![1, 2, 3, 4, 5]);
        assert_eq!(subsequence_of_five(-1.0, Some(4.0)), vec![1, 2]);
        assert_eq!(subsequence_of_five(6.0, None), Vec::<i64>::new());
        assert_eq!(subsequence_of_five(2.0, Some(0.0)), Vec::<i64>::new());
    }

    /// NaN and the infinities behave as they did: `fn:round` returns them
    /// unchanged, so the argument guards still decide the result.
    #[test]
    fn test_subsequence_nan_and_infinite_arguments() {
        assert_eq!(subsequence_of_five(f64::NAN, None), Vec::<i64>::new());
        assert_eq!(subsequence_of_five(f64::NAN, Some(2.0)), Vec::<i64>::new());
        assert_eq!(subsequence_of_five(2.0, Some(f64::NAN)), Vec::<i64>::new());
        assert_eq!(subsequence_of_five(f64::INFINITY, None), Vec::<i64>::new());
        assert_eq!(
            subsequence_of_five(2.0, Some(f64::NEG_INFINITY)),
            Vec::<i64>::new()
        );
        // -Infinity start with a finite length leaves no room for any position.
        assert_eq!(
            subsequence_of_five(f64::NEG_INFINITY, Some(4.0)),
            Vec::<i64>::new()
        );
        // -Infinity start without a length keeps the whole sequence, and an
        // infinite length runs to the end.
        assert_eq!(
            subsequence_of_five(f64::NEG_INFINITY, None),
            vec![1, 2, 3, 4, 5]
        );
        assert_eq!(
            subsequence_of_five(2.0, Some(f64::INFINITY)),
            vec![2, 3, 4, 5]
        );
    }

    // ========== unordered tests ==========

    #[test]
    fn test_unordered_passthrough() {
        let mut ctx = make_context();
        let seq = integer_seq::<RoXmlNavigator>(&[1, 2, 3]);
        let args = vec![seq];
        let result = unordered(&mut ctx, args).unwrap();
        assert_eq!(extract_integers(result), vec![1, 2, 3]);
    }

    // ========== index-of with numeric type promotion tests ==========

    #[test]
    fn test_index_of_integer_matches_double() {
        let mut ctx = make_context();
        // Sequence of integers
        let seq = integer_seq::<RoXmlNavigator>(&[10, 20, 30]);
        // Search for 20.0 (double)
        let search = XPathValue::double(20.0);
        let args = vec![seq, search];
        let result = index_of(&mut ctx, args).unwrap();
        // Should find 20 at position 2
        assert_eq!(extract_integers(result), vec![2]);
    }

    #[test]
    fn test_index_of_double_matches_integer() {
        let mut ctx = make_context();
        // Sequence with a double
        let items: Vec<XmlItem<RoXmlNavigator>> = vec![
            XmlItem::Atomic(XmlValue::double(10.0)),
            XmlItem::Atomic(XmlValue::double(20.0)),
            XmlItem::Atomic(XmlValue::double(30.0)),
        ];
        let seq = XPathValue::from_sequence(items);
        // Search for 20 (integer)
        let search = XPathValue::integer(20);
        let args = vec![seq, search];
        let result = index_of(&mut ctx, args).unwrap();
        // Should find 20.0 at position 2
        assert_eq!(extract_integers(result), vec![2]);
    }

    #[test]
    fn test_index_of_nan_not_equal() {
        let mut ctx = make_context();
        // Sequence with NaN
        let items: Vec<XmlItem<RoXmlNavigator>> = vec![
            XmlItem::Atomic(XmlValue::double(f64::NAN)),
            XmlItem::Atomic(XmlValue::double(1.0)),
        ];
        let seq = XPathValue::from_sequence(items);
        // Search for NaN
        let search = XPathValue::double(f64::NAN);
        let args = vec![seq, search];
        let result = index_of(&mut ctx, args).unwrap();
        // NaN should not match NaN for value comparison
        assert!(result.is_empty());
    }

    // ========== reverse tests ==========

    #[test]
    fn test_reverse_sequence() {
        let mut ctx = make_context();
        let seq = integer_seq::<RoXmlNavigator>(&[1, 2, 3, 4, 5]);
        let args = vec![seq];
        let result = reverse(&mut ctx, args).unwrap();
        assert_eq!(extract_integers(result), vec![5, 4, 3, 2, 1]);
    }

    #[test]
    fn test_reverse_empty() {
        let mut ctx = make_context();
        let seq = XPathValue::<RoXmlNavigator>::Empty;
        let args = vec![seq];
        let result = reverse(&mut ctx, args).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_reverse_single() {
        let mut ctx = make_context();
        let seq = XPathValue::integer(42);
        let args = vec![seq];
        let result = reverse(&mut ctx, args).unwrap();
        assert_eq!(extract_integers(result), vec![42]);
    }

    // ========== zero-or-one tests ==========

    #[test]
    fn test_zero_or_one_empty() {
        let mut ctx = make_context();
        let seq = XPathValue::<RoXmlNavigator>::Empty;
        let args = vec![seq];
        let result = zero_or_one(&mut ctx, args).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_zero_or_one_single() {
        let mut ctx = make_context();
        let seq = XPathValue::integer(42);
        let args = vec![seq];
        let result = zero_or_one(&mut ctx, args).unwrap();
        assert_eq!(extract_integers(result), vec![42]);
    }

    #[test]
    fn test_zero_or_one_multiple_fails() {
        let mut ctx = make_context();
        let seq = integer_seq::<RoXmlNavigator>(&[1, 2]);
        let args = vec![seq];
        let result = zero_or_one(&mut ctx, args);
        match result {
            Err(e) => assert_eq!(e.error_code(), Some("FORG0003")),
            Ok(_) => panic!("Expected FORG0003 error"),
        }
    }

    // ========== one-or-more tests ==========

    #[test]
    fn test_one_or_more_single() {
        let mut ctx = make_context();
        let seq = XPathValue::integer(42);
        let args = vec![seq];
        let result = one_or_more(&mut ctx, args).unwrap();
        assert_eq!(extract_integers(result), vec![42]);
    }

    #[test]
    fn test_one_or_more_multiple() {
        let mut ctx = make_context();
        let seq = integer_seq::<RoXmlNavigator>(&[1, 2, 3]);
        let args = vec![seq];
        let result = one_or_more(&mut ctx, args).unwrap();
        assert_eq!(extract_integers(result), vec![1, 2, 3]);
    }

    #[test]
    fn test_one_or_more_empty_fails() {
        let mut ctx = make_context();
        let seq = XPathValue::<RoXmlNavigator>::Empty;
        let args = vec![seq];
        let result = one_or_more(&mut ctx, args);
        match result {
            Err(e) => assert_eq!(e.error_code(), Some("FORG0004")),
            Ok(_) => panic!("Expected FORG0004 error"),
        }
    }

    // ========== exactly-one tests ==========

    #[test]
    fn test_exactly_one_single() {
        let mut ctx = make_context();
        let seq = XPathValue::integer(42);
        let args = vec![seq];
        let result = exactly_one(&mut ctx, args).unwrap();
        assert_eq!(extract_integers(result), vec![42]);
    }

    #[test]
    fn test_exactly_one_empty_fails() {
        let mut ctx = make_context();
        let seq = XPathValue::<RoXmlNavigator>::Empty;
        let args = vec![seq];
        let result = exactly_one(&mut ctx, args);
        match result {
            Err(e) => assert_eq!(e.error_code(), Some("FORG0005")),
            Ok(_) => panic!("Expected FORG0005 error"),
        }
    }

    #[test]
    fn test_exactly_one_multiple_fails() {
        let mut ctx = make_context();
        let seq = integer_seq::<RoXmlNavigator>(&[1, 2]);
        let args = vec![seq];
        let result = exactly_one(&mut ctx, args);
        match result {
            Err(e) => assert_eq!(e.error_code(), Some("FORG0005")),
            Ok(_) => panic!("Expected FORG0005 error"),
        }
    }

    // ========== distinct-values tests ==========

    #[test]
    fn test_distinct_values_integers() {
        let mut ctx = make_context();
        let seq = integer_seq::<RoXmlNavigator>(&[1, 2, 1, 3, 2, 1]);
        let args = vec![seq];
        let result = distinct_values(&mut ctx, args).unwrap();
        assert_eq!(extract_integers(result), vec![1, 2, 3]);
    }

    #[test]
    fn test_distinct_values_empty() {
        let mut ctx = make_context();
        let seq = XPathValue::<RoXmlNavigator>::Empty;
        let args = vec![seq];
        let result = distinct_values(&mut ctx, args).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_distinct_values_mixed_numeric() {
        let mut ctx = make_context();
        // Mix of integers and doubles that are equal
        let items: Vec<XmlItem<RoXmlNavigator>> = vec![
            XmlItem::Atomic(XmlValue::integer(BigInt::from(1))),
            XmlItem::Atomic(XmlValue::double(2.0)),
            XmlItem::Atomic(XmlValue::integer(BigInt::from(1))), // duplicate of 1
            XmlItem::Atomic(XmlValue::double(2.0)),              // duplicate of 2.0
            XmlItem::Atomic(XmlValue::integer(BigInt::from(3))),
        ];
        let seq = XPathValue::from_sequence(items);
        let args = vec![seq];
        let result = distinct_values(&mut ctx, args).unwrap();
        // Should have 3 distinct values: 1, 2.0, 3
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_distinct_values_strings() {
        let mut ctx = make_context();
        let items: Vec<XmlItem<RoXmlNavigator>> = vec!["a", "b", "a", "c", "b"]
            .into_iter()
            .map(|s| XmlItem::Atomic(XmlValue::string(s)))
            .collect();
        let seq = XPathValue::from_sequence(items);
        let args = vec![seq];
        let result = distinct_values(&mut ctx, args).unwrap();
        // Should have 3 distinct values: a, b, c
        assert_eq!(result.len(), 3);
    }

    // ========== deep-equal tests ==========

    #[test]
    fn test_deep_equal_same_integers() {
        let mut ctx = make_context();
        let seq1 = integer_seq::<RoXmlNavigator>(&[1, 2, 3]);
        let seq2 = integer_seq::<RoXmlNavigator>(&[1, 2, 3]);
        let args = vec![seq1, seq2];
        let result = deep_equal(&mut ctx, args).unwrap();
        assert!(extract_bool(result));
    }

    #[test]
    fn test_deep_equal_different_integers() {
        let mut ctx = make_context();
        let seq1 = integer_seq::<RoXmlNavigator>(&[1, 2, 3]);
        let seq2 = integer_seq::<RoXmlNavigator>(&[1, 2, 4]);
        let args = vec![seq1, seq2];
        let result = deep_equal(&mut ctx, args).unwrap();
        assert!(!extract_bool(result));
    }

    #[test]
    fn test_deep_equal_nan() {
        let mut ctx = make_context();
        let seq1: XPathValue<RoXmlNavigator> = XPathValue::double(f64::NAN);
        let seq2: XPathValue<RoXmlNavigator> = XPathValue::double(f64::NAN);
        let args = vec![seq1, seq2];
        let result = deep_equal(&mut ctx, args).unwrap();
        // XPath deep-equal treats NaN as equal to NaN
        assert!(extract_bool(result));
    }

    #[test]
    fn test_deep_equal_empty_sequences() {
        let mut ctx = make_context();
        let seq1 = XPathValue::<RoXmlNavigator>::Empty;
        let seq2 = XPathValue::<RoXmlNavigator>::Empty;
        let args = vec![seq1, seq2];
        let result = deep_equal(&mut ctx, args).unwrap();
        assert!(extract_bool(result));
    }

    #[test]
    fn test_deep_equal_different_lengths() {
        let mut ctx = make_context();
        let seq1 = integer_seq::<RoXmlNavigator>(&[1, 2]);
        let seq2 = integer_seq::<RoXmlNavigator>(&[1, 2, 3]);
        let args = vec![seq1, seq2];
        let result = deep_equal(&mut ctx, args).unwrap();
        assert!(!extract_bool(result));
    }

    /// The document element of `doc`, as a one-item argument sequence.
    fn document_element<'d>(doc: &'d roxmltree::Document<'d>) -> XPathValue<RoXmlNavigator<'d>> {
        let mut nav = RoXmlNavigator::new(doc);
        assert!(nav.move_to_first_child(), "a document element");
        XPathValue::from_sequence(vec![XmlItem::Node(nav)])
    }

    /// F&O §15.3.1 compares an element's `(*|text())`, so comment and
    /// processing-instruction children play no part. Checked through the
    /// function itself, not through the comparer.
    #[test]
    fn deep_equal_ignores_comment_and_pi_children() {
        let plain = roxmltree::Document::parse("<a>x</a>").expect("parse xml");

        for xml in ["<a>x<!--c--></a>", "<a><!--c-->x</a>", "<a>x<?p d?></a>"] {
            let with_noise = roxmltree::Document::parse(xml).expect("parse xml");
            let mut ctx = make_context();
            let args = vec![document_element(&with_noise), document_element(&plain)];
            assert!(
                extract_bool(deep_equal(&mut ctx, args).unwrap()),
                "{xml} should be deep-equal to <a>x</a>",
            );
        }
    }

    /// …but such a child still splits the text around it, and text nodes are
    /// never merged, so `<a>x<!--c-->y</a>` has two text children where
    /// `<a>xy</a>` has one.
    #[test]
    fn deep_equal_does_not_merge_text_split_by_a_comment() {
        let split = roxmltree::Document::parse("<a>x<!--c-->y</a>").expect("parse xml");
        let joined = roxmltree::Document::parse("<a>xy</a>").expect("parse xml");

        let mut ctx = make_context();
        let args = vec![document_element(&split), document_element(&joined)];
        assert!(!extract_bool(deep_equal(&mut ctx, args).unwrap()));
    }

    /// A comment or PI that is itself an item of the compared sequences is
    /// compared — by string value, and for a PI by target as well.
    #[test]
    fn deep_equal_compares_comment_and_pi_items() {
        let doc =
            roxmltree::Document::parse("<a><!--c--><!--d--><?p v?><?q v?></a>").expect("parse xml");

        let child = |index: usize| -> XPathValue<RoXmlNavigator<'_>> {
            let mut nav = RoXmlNavigator::new(&doc);
            assert!(nav.move_to_first_child(), "a document element");
            assert!(nav.move_to_first_child(), "a child node");
            for _ in 0..index {
                assert!(nav.move_to_next_sibling(), "a child node");
            }
            XPathValue::from_sequence(vec![XmlItem::Node(nav)])
        };

        let mut ctx = make_context();
        assert!(extract_bool(
            deep_equal(&mut ctx, vec![child(0), child(0)]).unwrap()
        ));
        assert!(!extract_bool(
            deep_equal(&mut ctx, vec![child(0), child(1)]).unwrap()
        ));
        assert!(extract_bool(
            deep_equal(&mut ctx, vec![child(2), child(2)]).unwrap()
        ));
        assert!(!extract_bool(
            deep_equal(&mut ctx, vec![child(2), child(3)]).unwrap()
        ));
    }
}
