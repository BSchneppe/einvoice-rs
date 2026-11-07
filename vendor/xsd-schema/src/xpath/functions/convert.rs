//! Function conversion rules for built-in function arguments.
//!
//! XPath 2.0 §3.1.5 "Function Calls" requires that *every* argument value be
//! converted to the declared type of the corresponding parameter before the
//! function body runs:
//!
//! > A function call is evaluated as follows:
//! >
//! > 1. Argument expressions are evaluated, producing argument values. […]
//! > 2. Each argument value is converted by applying the function conversion
//! >    rules listed below.
//! > 3. The function is evaluated using the converted argument values.
//!
//! and, for a parameter whose declared type is an atomic type:
//!
//! > If the expected type is a sequence of an atomic type (possibly with an
//! > occurrence indicator `*`, `+`, or `?`), the following conversions are
//! > applied:
//! >
//! > 1. Atomization is applied to the given value, resulting in a sequence of
//! >    atomic values.
//! > 2. Each item in the atomic sequence that is of type `xs:untypedAtomic` is
//! >    cast to the expected atomic type. For built-in functions where the
//! >    expected type is specified as numeric, arguments of type
//! >    `xs:untypedAtomic` are cast to `xs:double`.
//! > 3. For each numeric item in the atomic sequence that can be promoted to
//! >    the expected atomic type using numeric promotion as described in B.1
//! >    Type Promotion, the promotion is done.
//! > 4. For each item of type `xs:anyURI` in the atomic sequence that can be
//! >    promoted to the expected atomic type using URI promotion as described
//! >    in B.1 Type Promotion, the promotion is done.
//! >
//! > If, after the above conversions, the resulting value does not match the
//! > expected type according to the rules for SequenceType Matching, a type
//! > error is raised [err:XPTY0004].
//!
//! Step 1 is already performed by the `atomize_to_*` helpers in the parent
//! module. This module supplies step 2 ([`cast_untyped_as`]) and the closing
//! XPTY0004 rule ([`expect_atomic_as`]) for the built-ins whose declared
//! parameter type is a *specific* atomic type — dates, times, durations,
//! integers — which would otherwise reject an untyped node from a document
//! that carries no schema.
//!
//! Steps 3 and 4 are not implemented here: no built-in routed through these
//! helpers declares `xs:float`, `xs:double` or `xs:string` for the parameter in
//! question (the ones that do, `fn:substring` and `fn:subsequence`, convert
//! through `atomize_to_double`, which already applies `fn:number` semantics),
//! and neither numeric nor URI promotion can reach `xs:integer`, `xs:date`,
//! `xs:time`, `xs:dateTime`, `xs:duration` or `xs:dayTimeDuration`.
//!
//! Conversion stays a per-built-in step rather than a signature-driven pass in
//! `eval_function`: the registry's `param_types` are not consulted at
//! evaluation time, so their precision is unverified, and making them
//! load-bearing would change every built-in's argument handling at once.

use crate::types::value::XmlValue;
use crate::types::XmlTypeCode;
use crate::xpath::cast;
use crate::xpath::error::XPathError;

/// Step 2 of the function conversion rules, on one already-atomized item:
///
/// > Each item in the atomic sequence that is of type `xs:untypedAtomic` is
/// > cast to the expected atomic type.
///
/// The cast is the ordinary `cast as` of [`cast::cast_to`], so an input whose
/// lexical form is not valid for `expected` raises the dynamic error `FORG0001`
/// naming the offending lexical value and the function. A type that
/// `xs:untypedAtomic` cannot be cast to at all — `xs:QName` and
/// `xs:NOTATION`, which may only be cast from a string literal — raises the
/// type error `XPTY0004` instead, which is the closing rule of §3.1.5.
///
/// Any item that is *not* `xs:untypedAtomic` is returned untouched: this
/// function performs no type check of its own, so a caller that has its own
/// (possibly laxer) notion of an acceptable argument keeps it. Callers that
/// want the closing SequenceType check as well use [`expect_atomic_as`].
pub(crate) fn cast_untyped_as(
    value: XmlValue,
    expected: XmlTypeCode,
    function: &str,
) -> Result<XmlValue, XPathError> {
    if value.type_code != XmlTypeCode::UntypedAtomic {
        return Ok(value);
    }
    match cast::cast_to(&value, expected) {
        Ok(converted) => Ok(converted),
        // A reachable target type with an invalid lexical form: dynamic error.
        Err(XPathError::FORG0001 { value, target_type }) => Err(XPathError::FORG0001 {
            value,
            target_type: format!("{target_type} (argument of fn:{function})"),
        }),
        // Not castable from xs:untypedAtomic at all: type error.
        Err(_) => Err(type_error(expected, XmlTypeCode::UntypedAtomic)),
    }
}

/// Apply the function conversion rules to one already-atomized argument whose
/// expected type is a specific atomic type.
///
/// The three outcomes mirror XPath 2.0 §3.1.5:
///
/// * `xs:untypedAtomic` input — cast to `expected` by [`cast_untyped_as`],
///   with its `FORG0001` / `XPTY0004` split.
/// * input already of `expected`, or of a type derived from it (an `xs:short`
///   where `xs:integer` is expected, an `xs:dayTimeDuration` where
///   `xs:duration` is expected) — returned unchanged.
/// * anything else — `XPTY0004`, per "If, after the above conversions, the
///   resulting value does not match the expected type according to the rules
///   for SequenceType Matching, a type error is raised [err:XPTY0004]".
///
/// An empty sequence is *not* this function's concern: a caller whose parameter
/// is optional (`xs:date?`) must keep returning the empty sequence for an empty
/// input before reaching here.
///
/// So an untyped `1999-01-20` taken from a document that carries no schema
/// becomes an `xs:date` under
/// `expect_atomic_as(value, XmlTypeCode::Date, "month-from-date")`, exactly as
/// `end_date >= xs:date('1999-03-01')` already casts it.
pub(crate) fn expect_atomic_as(
    value: XmlValue,
    expected: XmlTypeCode,
    function: &str,
) -> Result<XmlValue, XPathError> {
    let value = cast_untyped_as(value, expected, function)?;
    if accepts(expected, value.type_code) {
        Ok(value)
    } else {
        Err(type_error(expected, value.type_code))
    }
}

/// Apply step 2 of the function conversion rules to an argument whose expected
/// type is the pseudo-type `numeric`:
///
/// > For built-in functions where the expected type is specified as numeric,
/// > arguments of type `xs:untypedAtomic` are cast to `xs:double`.
///
/// Every other input is returned untouched, so the caller's own numeric
/// dispatch keeps producing its usual result type (`fn:abs(-2)` stays an
/// `xs:integer`) and its usual `XPTY0004` for non-numeric input.
pub(crate) fn promote_untyped_to_double(
    value: XmlValue,
    function: &str,
) -> Result<XmlValue, XPathError> {
    cast_untyped_as(value, XmlTypeCode::Double, function)
}

/// SequenceType matching for the item types these helpers deal with: the
/// expected type itself, or any type derived from it by restriction.
///
/// [`cast::type_matches`] covers the `xs:string`, `xs:integer` and `xs:decimal`
/// branches of the built-in hierarchy; the two date/time branches it does not
/// model are added here.
fn accepts(expected: XmlTypeCode, actual: XmlTypeCode) -> bool {
    if cast::type_matches(actual, expected) {
        return true;
    }
    match expected {
        // xs:yearMonthDuration and xs:dayTimeDuration are derived from
        // xs:duration (XSD Datatypes §3.4.26, §3.4.27).
        XmlTypeCode::Duration => matches!(
            actual,
            XmlTypeCode::YearMonthDuration | XmlTypeCode::DayTimeDuration
        ),
        // xs:dateTimeStamp is derived from xs:dateTime (XSD 1.1 Datatypes §3.4.28).
        XmlTypeCode::DateTime => actual == XmlTypeCode::DateTimeStamp,
        _ => false,
    }
}

/// Build the closing `XPTY0004` of §3.1.5, in the message shape the built-ins
/// have always used (`expected 'xs:date', found 'UntypedAtomic'`).
fn type_error(expected: XmlTypeCode, found: XmlTypeCode) -> XPathError {
    XPathError::XPTY0004 {
        expected: format!("xs:{}", expected.local_name().unwrap_or("anyAtomicType")),
        found: format!("{found:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_bigint::BigInt;

    #[test]
    fn cast_untyped_as_leaves_a_typed_item_alone() {
        // No type check of its own: an xs:double stays an xs:double even where
        // xs:integer is expected, so a caller with laxer rules keeps them.
        let value = cast_untyped_as(XmlValue::double(65.0), XmlTypeCode::Integer, "probe")
            .expect("not xs:untypedAtomic");
        assert_eq!(value.type_code, XmlTypeCode::Double);
    }

    #[test]
    fn cast_untyped_as_casts_an_untyped_item() {
        let value = cast_untyped_as(XmlValue::untyped("65"), XmlTypeCode::Integer, "probe")
            .expect("castable lexical form");
        assert_eq!(value.type_code, XmlTypeCode::Integer);
    }

    #[test]
    fn untyped_atomic_is_cast_to_the_expected_type() {
        let value = expect_atomic_as(
            XmlValue::untyped("1999-01-20"),
            XmlTypeCode::Date,
            "month-from-date",
        )
        .expect("castable lexical form");
        assert_eq!(value.type_code, XmlTypeCode::Date);
    }

    #[test]
    fn untyped_atomic_integer_is_cast_to_integer() {
        let value = expect_atomic_as(XmlValue::untyped("65"), XmlTypeCode::Integer, "remove")
            .expect("castable lexical form");
        assert_eq!(value.type_code, XmlTypeCode::Integer);
        assert_eq!(value.as_integer(), Some(&BigInt::from(65)));
    }

    #[test]
    fn uncastable_untyped_atomic_raises_forg0001() {
        let err = expect_atomic_as(
            XmlValue::untyped("not-a-date"),
            XmlTypeCode::Date,
            "month-from-date",
        )
        .expect_err("invalid lexical form");
        assert_eq!(err.error_code(), Some("FORG0001"));
        let message = err.to_string();
        assert!(message.contains("not-a-date"), "{message}");
        assert!(message.contains("month-from-date"), "{message}");
    }

    #[test]
    fn untyped_atomic_to_qname_raises_xpty0004() {
        // Casting to xs:QName is not available to xs:untypedAtomic, so the
        // closing SequenceType-matching rule of §3.1.5 applies.
        let err = expect_atomic_as(
            XmlValue::untyped("foo"),
            XmlTypeCode::QName,
            "prefix-from-QName",
        )
        .expect_err("xs:QName is not castable from xs:untypedAtomic");
        assert_eq!(err.error_code(), Some("XPTY0004"));
    }

    #[test]
    fn exact_type_passes_through_unchanged() {
        let value = expect_atomic_as(
            XmlValue::integer(BigInt::from(7)),
            XmlTypeCode::Integer,
            "remove",
        )
        .expect("exact type");
        assert_eq!(value.type_code, XmlTypeCode::Integer);
        assert_eq!(value.as_integer(), Some(&BigInt::from(7)));
    }

    #[test]
    fn derived_type_passes_through_unchanged() {
        let mut short = XmlValue::integer(BigInt::from(3));
        short.type_code = XmlTypeCode::Short;
        let value = expect_atomic_as(short, XmlTypeCode::Integer, "insert-before")
            .expect("xs:short is derived from xs:integer");
        assert_eq!(value.type_code, XmlTypeCode::Short);
    }

    #[test]
    fn day_time_duration_is_accepted_where_duration_is_expected() {
        let mut duration = XmlValue::untyped("PT1H");
        duration = expect_atomic_as(duration, XmlTypeCode::DayTimeDuration, "probe")
            .expect("castable lexical form");
        let value = expect_atomic_as(duration, XmlTypeCode::Duration, "hours-from-duration")
            .expect("xs:dayTimeDuration is derived from xs:duration");
        assert_eq!(value.type_code, XmlTypeCode::DayTimeDuration);
    }

    #[test]
    fn wrong_type_raises_xpty0004_in_the_established_message_shape() {
        let err = expect_atomic_as(XmlValue::string("x"), XmlTypeCode::Date, "month-from-date")
            .expect_err("xs:string is not an xs:date");
        assert_eq!(err.error_code(), Some("XPTY0004"));
        assert_eq!(
            err.to_string(),
            "[XPTY0004] Type mismatch: expected 'xs:date', found 'String'"
        );
    }

    #[test]
    fn numeric_promotion_casts_untyped_atomic_to_double() {
        let value = promote_untyped_to_double(XmlValue::untyped(" 3.5 "), "abs")
            .expect("castable lexical form");
        assert_eq!(value.type_code, XmlTypeCode::Double);
        assert_eq!(value.as_double(), Some(3.5));
    }

    #[test]
    fn numeric_promotion_leaves_other_types_alone() {
        let value = promote_untyped_to_double(XmlValue::integer(BigInt::from(-2)), "abs")
            .expect("already numeric");
        assert_eq!(value.type_code, XmlTypeCode::Integer);
    }

    #[test]
    fn numeric_promotion_of_a_bad_lexical_form_raises_forg0001() {
        let err = promote_untyped_to_double(XmlValue::untyped("twelve"), "abs")
            .expect_err("invalid lexical form");
        assert_eq!(err.error_code(), Some("FORG0001"));
        assert!(err.to_string().contains("abs"), "{err}");
    }
}
