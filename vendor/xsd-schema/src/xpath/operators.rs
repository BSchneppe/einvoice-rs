//! XPath operator evaluation helpers.
//!
//! This module hosts AST-only operator evaluation logic, using `XmlValue`
//! and `XmlTypeCode` directly (no ValueProxy layer). See
//! `XPATH_OPERATORS_DESIGN.md` for the target behavior.

use std::cmp::Ordering;

use num_bigint::BigInt;
use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use rust_decimal::Decimal;

use crate::ids::{SimpleTypeKey, TypeKey};
use crate::namespace::qname::QualifiedName;
use crate::schema::model::SchemaSet;
use crate::types::validators::VALIDATOR_REGISTRY;
use crate::types::value::{day_number, time_line_seconds};
use crate::types::value::{
    DateTimeValue, DateValue, DayTimeDurationValue, QNameValue, TimeValue, TimezoneOffset,
    XmlAtomicValue, XmlValue, XmlValueKind, YearMonthDurationValue,
};
use crate::types::value_order::{day_time_duration_parts, DurationParts};
use crate::types::XmlTypeCode;
use crate::xpath::ast::{BinaryOpKind, UnaryOpKind};
use crate::xpath::atomize::{self, ListMembers};
use crate::xpath::cast::cast_to;
use crate::xpath::collation::{self, CollationRef};
use crate::xpath::context::XPathContext;
use crate::xpath::error::{XPathError, XPathWarning};
use crate::xpath::general_compare;
use crate::xpath::iterator::{BufferedNodeIterator, XmlItemRef, XmlNodeIterator};
use crate::xpath::type_info::type_code_to_name;
use crate::xpath::DomNavigator;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NumericClass {
    Byte,
    UnsignedByte,
    Short,
    UnsignedShort,
    Int,
    UnsignedInt,
    Long,
    UnsignedLong,
    Integer,
    Decimal,
    Float,
    Double,
}

#[derive(Debug, Clone)]
enum NumericValue {
    Integer(BigInt),
    Decimal(Decimal),
    Float(f32),
    Double(f64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Promote {
    Left,
    Right,
    None,
}

/// XQTS-compatible scale for non-terminating decimal division results.
///
/// XPath 2.0 allows implementation-defined precision for xs:decimal division
/// (with a minimum of 18 fractional digits). We normalize to 18 here to keep
/// stable lexical output across platforms and match expected XQTS baselines.
const XPATH_DECIMAL_DIV_SCALE: u32 = 18;

/// Evaluate a unary operator for a single atomic value.
pub fn eval_unary(op: UnaryOpKind, value: &XmlValue) -> Result<XmlValue, XPathError> {
    match op {
        // XPath 2.0 §3.4 and §B.2: unary `+` is `op:numeric-unary-plus($arg as
        // numeric)`, with the same untypedAtomic-to-xs:double cast and the
        // same XPTY0004 for a non-numeric operand as unary `-`. F&O §6.2.7:
        // "The returned value is equal to $arg, and is an instance of
        // xs:integer, xs:decimal, xs:double, or xs:float depending on the type
        // of $arg."
        UnaryOpKind::Identity => eval_numeric_unary_op(value, false),
        UnaryOpKind::Negate => eval_numeric_unary(value),
    }
}

/// Evaluate a binary operator for two atomic values.
///
/// String operands are compared under the Unicode codepoint collation. To
/// compare them under the static context's default collation instead — which is
/// what the `eq`, `ne`, `lt`, `le`, `gt` and `ge` operators of an expression do
/// — the engine uses the crate-private `eval_binary_collated`.
///
/// A date/time operand without a timezone is read in the host's local offset
/// at the time of the call (F&O 1.0 §10.4 takes the implicit timezone from the
/// dynamic context, and this function has none); use
/// [`eval_binary_with_timezone`] to give the implicit timezone.
pub fn eval_binary(
    op: BinaryOpKind,
    left: &XmlValue,
    right: &XmlValue,
) -> Result<XmlValue, XPathError> {
    eval_binary_with_timezone(op, left, right, host_timezone_for(left, right))
}

/// [`eval_binary`] with `implicit_timezone` as the implicit timezone of the
/// dynamic context: the timezone of a date/time operand that has none, in
/// comparisons (F&O 1.0 §10.4: "If either operand to a comparison function on
/// date or time values does not have an (explicit) timezone then, for the
/// purpose of the operation, an implicit timezone, provided by the dynamic
/// context …, is assumed to be present as part of the value") and in the
/// subtraction of two date/time values (§10.8.1 – §10.8.3).
///
/// A host that evaluates expressions passes the timezone of the dynamic
/// context it evaluates them in —
/// [`DynamicContext::implicit_timezone_or_host`](crate::xpath::DynamicContext::implicit_timezone_or_host)
/// — so that its own comparisons (sort keys, grouping keys) agree with the
/// expressions'.
///
/// ```
/// use xsd_schema::types::value::{TimezoneOffset, XmlValue};
/// use xsd_schema::types::XmlTypeCode;
/// use xsd_schema::types::validators::VALIDATOR_REGISTRY;
/// use xsd_schema::xpath::ast::BinaryOpKind;
/// use xsd_schema::xpath::operators::eval_binary_with_timezone;
///
/// let local = VALIDATOR_REGISTRY.validate(XmlTypeCode::DateTime, "2000-01-01T00:00:00").unwrap();
/// let zoned = VALIDATOR_REGISTRY.validate(XmlTypeCode::DateTime, "2000-01-01T00:00:00+05:00").unwrap();
/// let five = TimezoneOffset::from_hm(5, 0);
/// let eq = eval_binary_with_timezone(BinaryOpKind::ValueEq, &local, &zoned, five).unwrap();
/// assert_eq!(eq.as_boolean(), Some(true));
/// let utc = eval_binary_with_timezone(BinaryOpKind::ValueEq, &local, &zoned, TimezoneOffset::UTC).unwrap();
/// assert_eq!(utc.as_boolean(), Some(false));
/// ```
pub fn eval_binary_with_timezone(
    op: BinaryOpKind,
    left: &XmlValue,
    right: &XmlValue,
    implicit_timezone: TimezoneOffset,
) -> Result<XmlValue, XPathError> {
    eval_binary_collated(op, left, right, CollationRef::Codepoint, implicit_timezone)
}

/// [`eval_binary`] with an explicit collation for string operands and the
/// implicit timezone `tz` for date/time operands.
///
/// `collation` is [`CollationRef::Codepoint`] for the Unicode codepoint
/// collation, which takes the very same code path [`eval_binary`] always took.
///
/// XPath 2.0 §2.1.1 defines the static context's default collation as "the
/// collation to be used by functions and operators for comparing and ordering
/// values of type `xs:string` and `xs:anyURI` (and types derived from them)",
/// and §B.2 Operator Mapping spells the `eq` of two `xs:string` values as
/// `op:numeric-equal(fn:compare(A, B), 0)` — `fn:compare` under the default
/// collation. `xs:anyURI` has the same row, and §B.1 adds the note that
/// "functions and operators that compare strings using the default collation
/// also compare `xs:anyURI` values using the default collation".
pub(crate) fn eval_binary_collated(
    op: BinaryOpKind,
    left: &XmlValue,
    right: &XmlValue,
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
) -> Result<XmlValue, XPathError> {
    eval_binary_noted(op, left, right, collation, tz, &mut false)
}

/// The `xs-seconds-capacity` warning for `what` — an operator or a function —
/// whose date/time or duration result keeps its seconds truncated toward zero
/// (see [`eval_binary_noted`]): the code the validator reports for a literal
/// with more fraction digits than this implementation holds, so that both
/// report alike.
pub(crate) fn seconds_capacity_warning(what: &str, result: &XmlValue) -> XPathWarning {
    XPathWarning::new(
        crate::types::validators::SECONDS_CAPACITY_WARNING,
        format!(
            "{what} gave a value whose seconds have more fraction digits than this \
             implementation holds beside the whole seconds (a 96-bit mantissa); the value \
             keeps them truncated toward zero: {}",
            result.to_string_value()
        ),
    )
}

/// [`eval_binary_collated`], setting `truncated` when a date/time or duration
/// result's seconds field could not hold the whole fraction and keeps it
/// truncated toward zero — for the caller to report `xs-seconds-capacity`
/// (Datatypes 1.1 §5.4). Only `+` and `-` with a `xs:dateTime`, `xs:time` or
/// `xs:dayTimeDuration` result can: their seconds below 60 and a fraction of
/// up to 28 digits may need 30; a date drops its time of day, and `*` / `div`
/// compute in `xs:decimal`, whose result's seconds field always fits.
pub(crate) fn eval_binary_noted(
    op: BinaryOpKind,
    left: &XmlValue,
    right: &XmlValue,
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
    truncated: &mut bool,
) -> Result<XmlValue, XPathError> {
    match op {
        BinaryOpKind::Add | BinaryOpKind::Sub | BinaryOpKind::Mul | BinaryOpKind::Div => {
            if let Some(result) = eval_temporal_binary(op, left, right, tz, truncated)? {
                return Ok(result);
            }
            eval_numeric_binary(op, left, right)
        }
        BinaryOpKind::IDiv | BinaryOpKind::Mod => eval_numeric_binary(op, left, right),
        BinaryOpKind::GeneralEq | BinaryOpKind::ValueEq => {
            Ok(XmlValue::boolean(compare_eq(left, right, collation, tz)?))
        }
        BinaryOpKind::GeneralNe | BinaryOpKind::ValueNe => {
            Ok(XmlValue::boolean(!compare_eq(left, right, collation, tz)?))
        }
        BinaryOpKind::GeneralGt | BinaryOpKind::ValueGt => {
            Ok(XmlValue::boolean(compare_gt(left, right, collation, tz)?))
        }
        BinaryOpKind::GeneralGe | BinaryOpKind::ValueGe => {
            Ok(XmlValue::boolean(compare_ge(left, right, collation, tz)?))
        }
        BinaryOpKind::GeneralLt | BinaryOpKind::ValueLt => {
            Ok(XmlValue::boolean(compare_lt(left, right, collation, tz)?))
        }
        BinaryOpKind::GeneralLe | BinaryOpKind::ValueLe => {
            Ok(XmlValue::boolean(compare_le(left, right, collation, tz)?))
        }
        BinaryOpKind::And | BinaryOpKind::Or => eval_boolean_logic(op, left, right),
        BinaryOpKind::Is | BinaryOpKind::Before | BinaryOpKind::After => {
            Err(unsupported_operator(op, left, right))
        }
        BinaryOpKind::Union | BinaryOpKind::Intersect | BinaryOpKind::Except => {
            Err(unsupported_operator(op, left, right))
        }
    }
}

/// Evaluate an XPath range expression (`expr to expr`).
///
/// Per XPath 2.0 spec, both operands must be of type `xs:integer`.
/// Non-integer types produce XPTY0004 type mismatch errors.
pub fn eval_range(start: &XmlValue, end: &XmlValue) -> Result<Vec<XmlValue>, XPathError> {
    let start_class = numeric_class(start.type_code).ok_or_else(|| {
        XPathError::type_mismatch("xs:integer", type_code_to_name(start.type_code))
    })?;
    let end_class = numeric_class(end.type_code)
        .ok_or_else(|| XPathError::type_mismatch("xs:integer", type_code_to_name(end.type_code)))?;

    if !is_integer_class(start_class) {
        return Err(XPathError::type_mismatch(
            "xs:integer",
            type_code_to_name(start.type_code),
        ));
    }
    if !is_integer_class(end_class) {
        return Err(XPathError::type_mismatch(
            "xs:integer",
            type_code_to_name(end.type_code),
        ));
    }

    let start_val = to_integer_value(start)?;
    let end_val = to_integer_value(end)?;

    if start_val > end_val {
        return Ok(Vec::new());
    }

    let mut result = Vec::new();
    let mut current = start_val;
    let one = BigInt::from(1);
    while current <= end_val {
        result.push(XmlValue {
            type_code: XmlTypeCode::Integer,
            schema_type: None,
            value: XmlValueKind::Atomic(XmlAtomicValue::Integer(current.clone())),
        });
        current += &one;
    }

    Ok(result)
}

/// Check if a type code is numeric for operator dispatch.
pub fn is_numeric(code: XmlTypeCode) -> bool {
    code.is_numeric()
}

fn eval_boolean_logic(
    op: BinaryOpKind,
    left: &XmlValue,
    right: &XmlValue,
) -> Result<XmlValue, XPathError> {
    let left_bool = left
        .as_boolean()
        .ok_or_else(|| XPathError::internal("Boolean operator requires boolean operands"))?;
    let right_bool = right
        .as_boolean()
        .ok_or_else(|| XPathError::internal("Boolean operator requires boolean operands"))?;

    let result = match op {
        BinaryOpKind::And => left_bool && right_bool,
        BinaryOpKind::Or => left_bool || right_bool,
        _ => return Err(XPathError::internal("Unexpected boolean operator")),
    };

    Ok(XmlValue::boolean(result))
}

fn compare_eq(
    left: &XmlValue,
    right: &XmlValue,
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
) -> Result<bool, XPathError> {
    let left = unwrap_union_value(left);
    let right = unwrap_union_value(right);

    if let Some(result) = compare_temporal_eq(left, right, tz)? {
        return Ok(result);
    }

    if left.type_code.is_list() || right.type_code.is_list() {
        if left.type_code != right.type_code {
            return Err(operator_not_defined("op:eq", left, right));
        }
        return Ok(list_values_equal(left, right));
    }

    if left.type_code.is_numeric() && right.type_code.is_numeric() {
        return numeric_eq(left, right);
    }

    if left.type_code == XmlTypeCode::Boolean && right.type_code == XmlTypeCode::Boolean {
        return Ok(left.as_boolean() == right.as_boolean());
    }

    if is_string_like(left.type_code) && is_string_like(right.type_code) {
        let (left_value, right_value) = (left.to_string_value(), right.to_string_value());
        return collation.equals(&left_value, &right_value);
    }

    if left.type_code == right.type_code {
        // Special case: QName equality compares namespace URI + local name only, ignoring prefix
        if left.type_code == XmlTypeCode::QName {
            if let (Some(lq), Some(rq)) = (left.as_qname(), right.as_qname()) {
                return Ok(lq.namespace_uri == rq.namespace_uri && lq.local_name == rq.local_name);
            }
        }
        return Ok(left.value == right.value);
    }

    Err(operator_not_defined("op:eq", left, right))
}

fn compare_gt(
    left: &XmlValue,
    right: &XmlValue,
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
) -> Result<bool, XPathError> {
    let left = unwrap_union_value(left);
    let right = unwrap_union_value(right);

    if let Some(result) = compare_temporal_gt(left, right, tz)? {
        return Ok(result);
    }

    if left.type_code.is_numeric() && right.type_code.is_numeric() {
        return numeric_gt(left, right);
    }

    if left.type_code == XmlTypeCode::Boolean && right.type_code == XmlTypeCode::Boolean {
        let l = left.as_boolean().unwrap_or(false);
        let r = right.as_boolean().unwrap_or(false);
        return Ok(l && !r);
    }

    if is_string_like(left.type_code) && is_string_like(right.type_code) {
        let left_value = left.to_string_value();
        let right_value = right.to_string_value();
        return Ok(
            compare_string_values(&left_value, &right_value, collation)? == Ordering::Greater
        );
    }

    Err(operator_not_defined("op:gt", left, right))
}

fn compare_ge(
    left: &XmlValue,
    right: &XmlValue,
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
) -> Result<bool, XPathError> {
    match compare_eq(left, right, collation, tz) {
        Ok(true) => Ok(true),
        Ok(false) => compare_gt(left, right, collation, tz),
        Err(err) if is_operator_not_defined(&err) => compare_gt(left, right, collation, tz),
        Err(err) => Err(err),
    }
}

fn compare_lt(
    left: &XmlValue,
    right: &XmlValue,
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
) -> Result<bool, XPathError> {
    compare_gt(right, left, collation, tz)
}

fn compare_le(
    left: &XmlValue,
    right: &XmlValue,
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
) -> Result<bool, XPathError> {
    match compare_eq(left, right, collation, tz) {
        Ok(true) => Ok(true),
        Ok(false) => compare_lt(left, right, collation, tz),
        Err(err) if is_operator_not_defined(&err) => compare_lt(left, right, collation, tz),
        Err(err) => Err(err),
    }
}

fn eval_temporal_binary(
    op: BinaryOpKind,
    left: &XmlValue,
    right: &XmlValue,
    tz: TimezoneOffset,
    truncated: &mut bool,
) -> Result<Option<XmlValue>, XPathError> {
    if !is_temporal_type(left.type_code) && !is_temporal_type(right.type_code) {
        return Ok(None);
    }

    let result = match op {
        BinaryOpKind::Add => eval_temporal_add(left, right, truncated),
        BinaryOpKind::Sub => eval_temporal_sub(left, right, tz, truncated),
        BinaryOpKind::Mul => eval_temporal_mul(left, right, truncated),
        BinaryOpKind::Div => eval_temporal_div(left, right, truncated),
        _ => return Ok(None),
    }?;

    Ok(Some(result))
}

/// `eq` on two date/time, duration or gregorian values, with `tz` as the
/// implicit timezone; `None` when neither operand is of those types.
///
/// F&O 1.0 §10.4: "If either operand to a comparison function on date or
/// time values does not have an (explicit) timezone then, for the purpose of
/// the operation, an implicit timezone, provided by the dynamic context …, is
/// assumed to be present as part of the value." The values are compared on
/// exact keys (whole seconds, the fraction apart), and durations by their
/// months and seconds (§10.4.5).
fn compare_temporal_eq(
    left: &XmlValue,
    right: &XmlValue,
    tz: TimezoneOffset,
) -> Result<Option<bool>, XPathError> {
    if is_gregorian_code(left.type_code) || is_gregorian_code(right.type_code) {
        // §10.4.15 – §10.4.19: the starting instants of the two values, for
        // two values of one gregorian type.
        if left.type_code != right.type_code {
            return Err(operator_not_defined("op:eq", left, right));
        }
        let (Some(l), Some(r)) = (gregorian_key(left, tz), gregorian_key(right, tz)) else {
            return Err(XPathError::internal("Expected a gregorian value"));
        };
        return Ok(Some(l == r));
    }

    if !is_temporal_type(left.type_code) && !is_temporal_type(right.type_code) {
        return Ok(None);
    }

    if is_date_time_code(left.type_code) || is_date_time_code(right.type_code) {
        if !(is_date_time_code(left.type_code) && is_date_time_code(right.type_code)) {
            return Err(operator_not_defined("op:eq", left, right));
        }
        let left_dt =
            as_datetime(left).ok_or_else(|| XPathError::internal("Expected dateTime value"))?;
        let right_dt =
            as_datetime(right).ok_or_else(|| XPathError::internal("Expected dateTime value"))?;
        return Ok(Some(
            datetime_key(left_dt, tz) == datetime_key(right_dt, tz),
        ));
    }

    if left.type_code == XmlTypeCode::Date || right.type_code == XmlTypeCode::Date {
        if left.type_code != XmlTypeCode::Date || right.type_code != XmlTypeCode::Date {
            return Err(operator_not_defined("op:eq", left, right));
        }
        let left_date = as_date(left).ok_or_else(|| XPathError::internal("Expected date value"))?;
        let right_date =
            as_date(right).ok_or_else(|| XPathError::internal("Expected date value"))?;
        return Ok(Some(date_key(left_date, tz) == date_key(right_date, tz)));
    }

    if left.type_code == XmlTypeCode::Time || right.type_code == XmlTypeCode::Time {
        if left.type_code != XmlTypeCode::Time || right.type_code != XmlTypeCode::Time {
            return Err(operator_not_defined("op:eq", left, right));
        }
        let left_time = as_time(left).ok_or_else(|| XPathError::internal("Expected time value"))?;
        let right_time =
            as_time(right).ok_or_else(|| XPathError::internal("Expected time value"))?;
        return Ok(Some(time_key(left_time, tz) == time_key(right_time, tz)));
    }

    if is_duration_code(left.type_code) || is_duration_code(right.type_code) {
        if !(is_duration_code(left.type_code) && is_duration_code(right.type_code)) {
            return Err(operator_not_defined("op:eq", left, right));
        }
        let left_parts =
            duration_key(left).ok_or_else(|| XPathError::internal("Expected duration value"))?;
        let right_parts =
            duration_key(right).ok_or_else(|| XPathError::internal("Expected duration value"))?;
        // §10.4.5: "the function returns true if the months and seconds
        // values of the two durations are equal".
        return Ok(Some(left_parts == right_parts));
    }

    Err(operator_not_defined("op:eq", left, right))
}

/// `gt` on two date/time or totally ordered duration values, with `tz` as
/// the implicit timezone; `None` when neither operand is of those types.
fn compare_temporal_gt(
    left: &XmlValue,
    right: &XmlValue,
    tz: TimezoneOffset,
) -> Result<Option<bool>, XPathError> {
    if !is_temporal_type(left.type_code) && !is_temporal_type(right.type_code) {
        return Ok(None);
    }

    if is_date_time_code(left.type_code) || is_date_time_code(right.type_code) {
        if !(is_date_time_code(left.type_code) && is_date_time_code(right.type_code)) {
            return Err(operator_not_defined("op:gt", left, right));
        }
        let left_dt =
            as_datetime(left).ok_or_else(|| XPathError::internal("Expected dateTime value"))?;
        let right_dt =
            as_datetime(right).ok_or_else(|| XPathError::internal("Expected dateTime value"))?;
        return Ok(Some(datetime_key(left_dt, tz) > datetime_key(right_dt, tz)));
    }

    if left.type_code == XmlTypeCode::Date || right.type_code == XmlTypeCode::Date {
        if left.type_code != XmlTypeCode::Date || right.type_code != XmlTypeCode::Date {
            return Err(operator_not_defined("op:gt", left, right));
        }
        let left_date = as_date(left).ok_or_else(|| XPathError::internal("Expected date value"))?;
        let right_date =
            as_date(right).ok_or_else(|| XPathError::internal("Expected date value"))?;
        return Ok(Some(date_key(left_date, tz) > date_key(right_date, tz)));
    }

    if left.type_code == XmlTypeCode::Time || right.type_code == XmlTypeCode::Time {
        if left.type_code != XmlTypeCode::Time || right.type_code != XmlTypeCode::Time {
            return Err(operator_not_defined("op:gt", left, right));
        }
        let left_time = as_time(left).ok_or_else(|| XPathError::internal("Expected time value"))?;
        let right_time =
            as_time(right).ok_or_else(|| XPathError::internal("Expected time value"))?;
        return Ok(Some(time_key(left_time, tz) > time_key(right_time, tz)));
    }

    if left.type_code == XmlTypeCode::YearMonthDuration
        || right.type_code == XmlTypeCode::YearMonthDuration
    {
        if left.type_code != XmlTypeCode::YearMonthDuration
            || right.type_code != XmlTypeCode::YearMonthDuration
        {
            return Err(operator_not_defined("op:gt", left, right));
        }
        let left_duration = as_year_month_duration(left)
            .ok_or_else(|| XPathError::internal("Expected yearMonthDuration value"))?;
        let right_duration = as_year_month_duration(right)
            .ok_or_else(|| XPathError::internal("Expected yearMonthDuration value"))?;
        return Ok(Some(
            year_month_total_months(left_duration) > year_month_total_months(right_duration),
        ));
    }

    if left.type_code == XmlTypeCode::DayTimeDuration
        || right.type_code == XmlTypeCode::DayTimeDuration
    {
        if left.type_code != XmlTypeCode::DayTimeDuration
            || right.type_code != XmlTypeCode::DayTimeDuration
        {
            return Err(operator_not_defined("op:gt", left, right));
        }
        let left_duration = as_day_time_duration(left)
            .ok_or_else(|| XPathError::internal("Expected dayTimeDuration value"))?;
        let right_duration = as_day_time_duration(right)
            .ok_or_else(|| XPathError::internal("Expected dayTimeDuration value"))?;
        return Ok(Some(
            seconds_key(&day_time_duration_parts(left_duration))
                > seconds_key(&day_time_duration_parts(right_duration)),
        ));
    }

    Err(operator_not_defined("op:gt", left, right))
}

fn operator_not_defined(op: &str, left: &XmlValue, right: &XmlValue) -> XPathError {
    XPathError::BinaryOperatorNotDefined {
        operator: op.to_string(),
        left_type: type_code_to_name(left.type_code).to_string(),
        right_type: type_code_to_name(right.type_code).to_string(),
    }
}

fn is_operator_not_defined(err: &XPathError) -> bool {
    matches!(err, XPathError::BinaryOperatorNotDefined { .. })
}

fn unwrap_union_value(value: &XmlValue) -> &XmlValue {
    let mut current = value;
    loop {
        match &current.value {
            XmlValueKind::Union(inner) => current = inner,
            _ => return current,
        }
    }
}

fn list_values_equal(left: &XmlValue, right: &XmlValue) -> bool {
    match (&left.value, &right.value) {
        (
            XmlValueKind::List {
                item_type: left_item_type,
                items: left_items,
            },
            XmlValueKind::List {
                item_type: right_item_type,
                items: right_items,
            },
        ) => {
            // Members compare by type and value; which schema type (a
            // restriction, the member a union chose) validated them does not
            // matter.
            left_item_type == right_item_type
                && left_items.len() == right_items.len()
                && left_items
                    .iter()
                    .zip(right_items)
                    .all(|(l, r)| l.type_code == r.type_code && l.value == r.value)
        }
        _ => false,
    }
}

/// The order two string values are in, under `collation`.
///
/// [`CollationRef::Codepoint`] is `str::cmp` — the only thing this function ever
/// did, and still the only thing it does unless a host has installed a
/// [`CollationResolver`](crate::xpath::collation::CollationResolver) *and* named
/// a different default collation. A URI the host does not support raises
/// FOCH0002 here, which is exactly where F&O §7.3.1 wants it: at the string
/// comparison that needed the collation.
fn compare_string_values(
    left: &str,
    right: &str,
    collation: CollationRef<'_>,
) -> Result<Ordering, XPathError> {
    collation.compare(left, right)
}

fn eval_temporal_add(
    left: &XmlValue,
    right: &XmlValue,
    truncated: &mut bool,
) -> Result<XmlValue, XPathError> {
    if let Some(left_dt) = as_datetime(left) {
        if let Some(duration) = as_year_month_duration(right) {
            let result = add_datetime_year_month(left_dt, year_month_total_months(duration))?;
            return Ok(xml_datetime_value(left.type_code, result));
        }
        if let Some(duration) = as_day_time_duration(right) {
            let result =
                add_datetime_day_time(left_dt, &day_time_duration_parts(duration), truncated)?;
            return Ok(xml_datetime_value(left.type_code, result));
        }
        return Err(unsupported_operator(BinaryOpKind::Add, left, right));
    }

    if let Some(left_date) = as_date(left) {
        if let Some(duration) = as_year_month_duration(right) {
            let result = add_date_year_month(left_date, year_month_total_months(duration))?;
            return Ok(xml_date_value(left.type_code, result));
        }
        if let Some(duration) = as_day_time_duration(right) {
            let result = add_date_day_time(left_date, &day_time_duration_parts(duration))?;
            return Ok(xml_date_value(left.type_code, result));
        }
        return Err(unsupported_operator(BinaryOpKind::Add, left, right));
    }

    if let Some(left_time) = as_time(left) {
        if let Some(duration) = as_day_time_duration(right) {
            let result =
                add_time_day_time(left_time, &day_time_duration_parts(duration), truncated);
            return Ok(xml_time_value(left.type_code, result));
        }
        return Err(unsupported_operator(BinaryOpKind::Add, left, right));
    }

    if let Some(left_duration) = as_year_month_duration(left) {
        if let Some(right_duration) = as_year_month_duration(right) {
            // Both totals are below 13 × 2³², so the sum cannot overflow.
            let total =
                year_month_total_months(left_duration) + year_month_total_months(right_duration);
            return Ok(xml_year_month_duration_value(year_month_from_months(
                total,
            )?));
        }
        if let Some(right_dt) = as_datetime(right) {
            let result = add_datetime_year_month(right_dt, year_month_total_months(left_duration))?;
            return Ok(xml_datetime_value(right.type_code, result));
        }
        if let Some(right_date) = as_date(right) {
            let result = add_date_year_month(right_date, year_month_total_months(left_duration))?;
            return Ok(xml_date_value(right.type_code, result));
        }
        return Err(unsupported_operator(BinaryOpKind::Add, left, right));
    }

    if let Some(left_duration) = as_day_time_duration(left) {
        let left_parts = day_time_duration_parts(left_duration);
        if let Some(right_duration) = as_day_time_duration(right) {
            let (seconds, fraction) = add_seconds(
                seconds_key(&left_parts),
                &day_time_duration_parts(right_duration),
            );
            return Ok(xml_day_time_duration_value(day_time_from_exact(
                seconds,
                fraction,
                XPathError::FODT0002,
                truncated,
            )?));
        }
        if let Some(right_dt) = as_datetime(right) {
            let result = add_datetime_day_time(right_dt, &left_parts, truncated)?;
            return Ok(xml_datetime_value(right.type_code, result));
        }
        if let Some(right_date) = as_date(right) {
            let result = add_date_day_time(right_date, &left_parts)?;
            return Ok(xml_date_value(right.type_code, result));
        }
        if let Some(right_time) = as_time(right) {
            let result = add_time_day_time(right_time, &left_parts, truncated);
            return Ok(xml_time_value(right.type_code, result));
        }
        return Err(unsupported_operator(BinaryOpKind::Add, left, right));
    }

    Err(unsupported_operator(BinaryOpKind::Add, left, right))
}

/// The subtraction operators (F&O §10.6.2, §10.6.7, §10.8.1 – §10.8.3, the
/// `subtract-*-from-*` functions of §10.8), with `tz` as the implicit timezone
/// of the two date/time values one subtracts from the other.
fn eval_temporal_sub(
    left: &XmlValue,
    right: &XmlValue,
    tz: TimezoneOffset,
    truncated: &mut bool,
) -> Result<XmlValue, XPathError> {
    if let Some(left_dt) = as_datetime(left) {
        if let Some(right_dt) = as_datetime(right) {
            let result = time_line_difference(
                datetime_key(left_dt, tz),
                datetime_key(right_dt, tz),
                truncated,
            )?;
            return Ok(xml_day_time_duration_value(result));
        }
        if let Some(duration) = as_year_month_duration(right) {
            let result = add_datetime_year_month(left_dt, -year_month_total_months(duration))?;
            return Ok(xml_datetime_value(left.type_code, result));
        }
        if let Some(duration) = as_day_time_duration(right) {
            let result = add_datetime_day_time(
                left_dt,
                &negated(day_time_duration_parts(duration)),
                truncated,
            )?;
            return Ok(xml_datetime_value(left.type_code, result));
        }
        return Err(unsupported_operator(BinaryOpKind::Sub, left, right));
    }

    if let Some(left_date) = as_date(left) {
        if let Some(right_date) = as_date(right) {
            let result =
                time_line_difference(date_key(left_date, tz), date_key(right_date, tz), truncated)?;
            return Ok(xml_day_time_duration_value(result));
        }
        if let Some(duration) = as_year_month_duration(right) {
            let result = add_date_year_month(left_date, -year_month_total_months(duration))?;
            return Ok(xml_date_value(left.type_code, result));
        }
        if let Some(duration) = as_day_time_duration(right) {
            let result = add_date_day_time(left_date, &negated(day_time_duration_parts(duration)))?;
            return Ok(xml_date_value(left.type_code, result));
        }
        return Err(unsupported_operator(BinaryOpKind::Sub, left, right));
    }

    if let Some(left_time) = as_time(left) {
        if let Some(right_time) = as_time(right) {
            let result =
                time_line_difference(time_key(left_time, tz), time_key(right_time, tz), truncated)?;
            return Ok(xml_day_time_duration_value(result));
        }
        if let Some(duration) = as_day_time_duration(right) {
            let result = add_time_day_time(
                left_time,
                &negated(day_time_duration_parts(duration)),
                truncated,
            );
            return Ok(xml_time_value(left.type_code, result));
        }
        return Err(unsupported_operator(BinaryOpKind::Sub, left, right));
    }

    if let Some(left_duration) = as_year_month_duration(left) {
        if let Some(right_duration) = as_year_month_duration(right) {
            let total =
                year_month_total_months(left_duration) - year_month_total_months(right_duration);
            return Ok(xml_year_month_duration_value(year_month_from_months(
                total,
            )?));
        }
        return Err(unsupported_operator(BinaryOpKind::Sub, left, right));
    }

    if let Some(left_duration) = as_day_time_duration(left) {
        if let Some(right_duration) = as_day_time_duration(right) {
            let (seconds, fraction) = add_seconds(
                seconds_key(&day_time_duration_parts(left_duration)),
                &negated(day_time_duration_parts(right_duration)),
            );
            return Ok(xml_day_time_duration_value(day_time_from_exact(
                seconds,
                fraction,
                XPathError::FODT0002,
                truncated,
            )?));
        }
        return Err(unsupported_operator(BinaryOpKind::Sub, left, right));
    }

    Err(unsupported_operator(BinaryOpKind::Sub, left, right))
}

fn eval_temporal_mul(
    left: &XmlValue,
    right: &XmlValue,
    truncated: &mut bool,
) -> Result<XmlValue, XPathError> {
    if let Some(duration) = as_year_month_duration(left) {
        if right.type_code.is_numeric() {
            return Ok(xml_year_month_duration_value(year_month_mul_numeric(
                duration,
                numeric_factor(right)?,
            )?));
        }
        return Err(unsupported_operator(BinaryOpKind::Mul, left, right));
    }

    if let Some(duration) = as_day_time_duration(left) {
        if right.type_code.is_numeric() {
            return Ok(xml_day_time_duration_value(day_time_mul_numeric_value(
                duration, right, truncated,
            )?));
        }
        return Err(unsupported_operator(BinaryOpKind::Mul, left, right));
    }

    if left.type_code.is_numeric() {
        if let Some(duration) = as_year_month_duration(right) {
            return Ok(xml_year_month_duration_value(year_month_mul_numeric(
                duration,
                numeric_factor(left)?,
            )?));
        }
        if let Some(duration) = as_day_time_duration(right) {
            return Ok(xml_day_time_duration_value(day_time_mul_numeric_value(
                duration, left, truncated,
            )?));
        }
    }

    Err(unsupported_operator(BinaryOpKind::Mul, left, right))
}

fn eval_temporal_div(
    left: &XmlValue,
    right: &XmlValue,
    truncated: &mut bool,
) -> Result<XmlValue, XPathError> {
    if let Some(duration) = as_year_month_duration(left) {
        if right.type_code.is_numeric() {
            return Ok(xml_year_month_duration_value(year_month_div_numeric(
                duration,
                numeric_factor(right)?,
            )?));
        }
        if let Some(right_duration) = as_year_month_duration(right) {
            let ratio = year_month_div_duration(duration, right_duration)?;
            return Ok(XmlValue::decimal(
                crate::types::exact_decimal::unsigned_zero(ratio),
            ));
        }
        return Err(unsupported_operator(BinaryOpKind::Div, left, right));
    }

    if let Some(duration) = as_day_time_duration(left) {
        if right.type_code.is_numeric() {
            return Ok(xml_day_time_duration_value(day_time_div_numeric_value(
                duration, right, truncated,
            )?));
        }
        if let Some(right_duration) = as_day_time_duration(right) {
            let ratio = day_time_div_duration(duration, right_duration)?;
            return Ok(XmlValue::decimal(
                crate::types::exact_decimal::unsigned_zero(ratio),
            ));
        }
        return Err(unsupported_operator(BinaryOpKind::Div, left, right));
    }

    Err(unsupported_operator(BinaryOpKind::Div, left, right))
}

fn numeric_eq(left: &XmlValue, right: &XmlValue) -> Result<bool, XPathError> {
    let (left_val, right_val, class) = match promote_numeric(left, right) {
        Ok(promoted) => promoted,
        Err(err) => return exact_decimal_order(left, right, err).map(|o| o == Ordering::Equal),
    };
    Ok(match class {
        NumericClass::Float => {
            let (l, r) = float_pair(left_val, right_val)?;
            l == r
        }
        NumericClass::Double => {
            let (l, r) = double_pair(left_val, right_val)?;
            l == r
        }
        NumericClass::Decimal => {
            let (l, r) = decimal_pair(left_val, right_val)?;
            l == r
        }
        _ => {
            let (l, r) = integer_pair(left_val, right_val)?;
            l == r
        }
    })
}

fn numeric_gt(left: &XmlValue, right: &XmlValue) -> Result<bool, XPathError> {
    let (left_val, right_val, class) = match promote_numeric(left, right) {
        Ok(promoted) => promoted,
        Err(err) => return exact_decimal_order(left, right, err).map(|o| o == Ordering::Greater),
    };
    Ok(match class {
        NumericClass::Float => {
            let (l, r) = float_pair(left_val, right_val)?;
            l > r
        }
        NumericClass::Double => {
            let (l, r) = double_pair(left_val, right_val)?;
            l > r
        }
        NumericClass::Decimal => {
            let (l, r) = decimal_pair(left_val, right_val)?;
            l > r
        }
        _ => {
            let (l, r) = integer_pair(left_val, right_val)?;
            l > r
        }
    })
}

/// Order two decimal-family operands exactly when promoting them to
/// `xs:decimal` failed for capacity — one is an `xs:decimal` carried in
/// lexical form, or an `xs:integer` beyond `rust_decimal`'s range. The
/// comparison operators are defined on the values (F&O §6.3), and comparing
/// two decimals needs no arithmetic, so the answer is exact rather than an
/// error. Any other failure is returned unchanged.
fn exact_decimal_order(
    left: &XmlValue,
    right: &XmlValue,
    err: XPathError,
) -> Result<Ordering, XPathError> {
    if err.is_decimal_capacity_error() {
        if let Some(order) = crate::types::exact_decimal::decimal_values_cmp(left, right) {
            return Ok(order);
        }
    }
    Err(err)
}

/// The `Decimal` of an `xs:decimal`-family operand, or FOAR0002 when it is
/// beyond what `rust_decimal::Decimal` holds — an `xs:decimal` carried in
/// lexical form, or an `xs:integer` promoted to `xs:decimal` outside its
/// range. F&O §6.2: "For xs:decimal operations, overflow behavior ·must·
/// raise an error \[err:FOAR0002\]."
///
/// Only a value no `Decimal` holds is a capacity error. An `xs:decimal` whose
/// payload is a string that does spell a value `Decimal` holds is converted
/// and used — the validator never makes one, so such a value can only come
/// from a typing mistake, and it must not pass for one beyond capacity (and so
/// let an assertion be skipped). A string that is not a decimal at all is the
/// internal error it always was.
pub(crate) fn decimal_operand(value: &XmlValue) -> Result<Decimal, XPathError> {
    if let Some(d) = value.as_decimal() {
        return Ok(d);
    }
    let Some(exact) = crate::types::exact_decimal::ExactDecimal::from_value(value) else {
        return Err(XPathError::internal("Expected decimal value"));
    };
    if let Ok(d) = exact.to_decimal() {
        return Ok(d);
    }
    if crate::types::exact_decimal::lexical_decimal(value).is_some()
        && MARK_CAPACITY_OPERANDS.with(std::cell::Cell::get)
    {
        Err(XPathError::decimal_capacity_operand())
    } else {
        Err(XPathError::FOAR0002)
    }
}

thread_local! {
    /// Whether [`decimal_operand`] marks the FOAR0002 it raises for an
    /// `xs:decimal` carried as a string; see [`with_decimal_capacity_marks`].
    static MARK_CAPACITY_OPERANDS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Run `evaluate` with the FOAR0002 that arithmetic raises for an
/// `xs:decimal` operand beyond `rust_decimal`'s capacity marked
/// ([`XPathError::decimal_capacity_operand`]).
///
/// The validator evaluates XSD 1.1 assertions this way. A valid value that
/// this implementation carries as a string is one whose processing it "cannot
/// … with assured correctness" (Datatypes 1.1 §5.4); an assertion that fails
/// because it computed with such a value has an unknown outcome, while one
/// that raises any other error — an overflow of in-range operands, a division
/// by zero — is not satisfied (Structures §3.13.4.1). Only the marked error
/// tells the two apart: it is raised exactly where such an operand is
/// converted, so an unrelated value elsewhere in the instance cannot excuse a
/// genuine error. The previous setting is restored on return (and on unwind).
pub(crate) fn with_decimal_capacity_marks<R>(evaluate: impl FnOnce() -> R) -> R {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            MARK_CAPACITY_OPERANDS.with(|mark| mark.set(self.0));
        }
    }
    let _restore = Restore(MARK_CAPACITY_OPERANDS.with(|mark| mark.replace(true)));
    evaluate()
}

fn eval_numeric_unary(value: &XmlValue) -> Result<XmlValue, XPathError> {
    eval_numeric_unary_op(value, true)
}

/// Unary `-` (`negate`) or unary `+` on one atomic operand.
fn eval_numeric_unary_op(value: &XmlValue, negate: bool) -> Result<XmlValue, XPathError> {
    // Per XPath 2.0 spec, UntypedAtomic is cast to xs:double for arithmetic
    let promoted;
    let value = if value.type_code == XmlTypeCode::UntypedAtomic {
        promoted = cast_untyped_to_double(value)?;
        &promoted
    } else {
        value
    };
    // XPath 2.0 §3.4: the unary operators are defined for numeric operands only;
    // anything else is a type error [err:XPTY0004].
    let class = numeric_class(value.type_code).ok_or_else(|| {
        XPathError::unary_operator_not_defined(
            "unary arithmetic operator",
            type_code_to_name(value.type_code),
        )
    })?;

    let result_type = unary_result_type(class);
    let value = to_numeric_value(value, class)?;

    let result = if !negate {
        value
    } else {
        match value {
            NumericValue::Integer(v) => NumericValue::Integer(-v),
            // A negative zero is made 0 in `numeric_to_xml_value`.
            NumericValue::Decimal(v) => NumericValue::Decimal(-v),
            NumericValue::Float(v) => NumericValue::Float(-v),
            NumericValue::Double(v) => NumericValue::Double(-v),
        }
    };

    Ok(numeric_to_xml_value(result, result_type))
}

fn eval_numeric_binary(
    op: BinaryOpKind,
    left: &XmlValue,
    right: &XmlValue,
) -> Result<XmlValue, XPathError> {
    let (left_val, right_val, class) = match promote_numeric(left, right) {
        Ok(promoted) => promoted,
        // F&O §6.2.4–§6.2.6: an xs:decimal or xs:integer division (`div`,
        // `idiv`, `mod`) by zero raises FOAR0001, whatever the dividend. A
        // dividend beyond `rust_decimal`'s capacity fails its conversion
        // first; that must not turn a certain FOAR0001 into the capacity
        // error FOAR0002.
        Err(err)
            if matches!(
                op,
                BinaryOpKind::Div | BinaryOpKind::IDiv | BinaryOpKind::Mod
            ) && err.is_decimal_capacity_error()
                && is_exact_zero(right) =>
        {
            return Err(XPathError::FOAR0001);
        }
        Err(err) => return Err(err),
    };
    let result_type = binary_result_type(op, class);

    match op {
        BinaryOpKind::Add => Ok(numeric_add(left_val, right_val, result_type)?),
        BinaryOpKind::Sub => Ok(numeric_sub(left_val, right_val, result_type)?),
        BinaryOpKind::Mul => Ok(numeric_mul(left_val, right_val, result_type)?),
        BinaryOpKind::Div => Ok(numeric_div(left_val, right_val, class, result_type)?),
        BinaryOpKind::IDiv => Ok(numeric_idiv(left_val, right_val)?),
        BinaryOpKind::Mod => Ok(numeric_mod(left_val, right_val, result_type)?),
        _ => Err(XPathError::internal("Unsupported numeric operator")),
    }
}

fn numeric_add(
    left: NumericValue,
    right: NumericValue,
    result_type: XmlTypeCode,
) -> Result<XmlValue, XPathError> {
    let result = match (left, right) {
        (NumericValue::Integer(l), NumericValue::Integer(r)) => NumericValue::Integer(l + r),
        (NumericValue::Decimal(l), NumericValue::Decimal(r)) => {
            NumericValue::Decimal(l.checked_add(r).ok_or(XPathError::FOAR0002)?)
        }
        (NumericValue::Float(l), NumericValue::Float(r)) => NumericValue::Float(l + r),
        (NumericValue::Double(l), NumericValue::Double(r)) => NumericValue::Double(l + r),
        _ => return Err(XPathError::internal("Numeric add type mismatch")),
    };
    Ok(numeric_to_xml_value(result, result_type))
}

fn numeric_sub(
    left: NumericValue,
    right: NumericValue,
    result_type: XmlTypeCode,
) -> Result<XmlValue, XPathError> {
    let result = match (left, right) {
        (NumericValue::Integer(l), NumericValue::Integer(r)) => NumericValue::Integer(l - r),
        (NumericValue::Decimal(l), NumericValue::Decimal(r)) => {
            NumericValue::Decimal(l.checked_sub(r).ok_or(XPathError::FOAR0002)?)
        }
        (NumericValue::Float(l), NumericValue::Float(r)) => NumericValue::Float(l - r),
        (NumericValue::Double(l), NumericValue::Double(r)) => NumericValue::Double(l - r),
        _ => return Err(XPathError::internal("Numeric sub type mismatch")),
    };
    Ok(numeric_to_xml_value(result, result_type))
}

fn numeric_mul(
    left: NumericValue,
    right: NumericValue,
    result_type: XmlTypeCode,
) -> Result<XmlValue, XPathError> {
    let result = match (left, right) {
        (NumericValue::Integer(l), NumericValue::Integer(r)) => NumericValue::Integer(l * r),
        (NumericValue::Decimal(l), NumericValue::Decimal(r)) => {
            NumericValue::Decimal(l.checked_mul(r).ok_or(XPathError::FOAR0002)?)
        }
        (NumericValue::Float(l), NumericValue::Float(r)) => NumericValue::Float(l * r),
        (NumericValue::Double(l), NumericValue::Double(r)) => NumericValue::Double(l * r),
        _ => return Err(XPathError::internal("Numeric mul type mismatch")),
    };
    Ok(numeric_to_xml_value(result, result_type))
}

fn numeric_div(
    left: NumericValue,
    right: NumericValue,
    class: NumericClass,
    result_type: XmlTypeCode,
) -> Result<XmlValue, XPathError> {
    let result = match (left, right) {
        (NumericValue::Integer(l), NumericValue::Integer(r)) => {
            if r == BigInt::from(0) {
                return Err(XPathError::FOAR0001);
            }
            let left_dec = decimal_from_bigint(&l)?;
            let right_dec = decimal_from_bigint(&r)?;
            let quotient = left_dec
                .checked_div(right_dec)
                .ok_or(XPathError::FOAR0002)?;
            NumericValue::Decimal(quotient.round_dp(XPATH_DECIMAL_DIV_SCALE))
        }
        (NumericValue::Decimal(l), NumericValue::Decimal(r)) => {
            if r.is_zero() {
                return Err(XPathError::FOAR0001);
            }
            let quotient = l.checked_div(r).ok_or(XPathError::FOAR0002)?;
            NumericValue::Decimal(quotient.round_dp(XPATH_DECIMAL_DIV_SCALE))
        }
        (NumericValue::Float(l), NumericValue::Float(r)) => NumericValue::Float(l / r),
        (NumericValue::Double(l), NumericValue::Double(r)) => NumericValue::Double(l / r),
        _ => return Err(XPathError::internal("Numeric div type mismatch")),
    };

    let result_type = match class {
        NumericClass::Decimal | NumericClass::Float | NumericClass::Double => result_type,
        _ => XmlTypeCode::Decimal,
    };

    Ok(numeric_to_xml_value(result, result_type))
}

fn numeric_idiv(left: NumericValue, right: NumericValue) -> Result<XmlValue, XPathError> {
    let result = match (left, right) {
        (NumericValue::Integer(l), NumericValue::Integer(r)) => {
            if r == BigInt::from(0) {
                return Err(XPathError::FOAR0001);
            }
            NumericValue::Integer(l / r)
        }
        (NumericValue::Decimal(l), NumericValue::Decimal(r)) => {
            if r.is_zero() {
                return Err(XPathError::FOAR0001);
            }
            let q = l.checked_div(r).ok_or(XPathError::FOAR0002)?.trunc();
            NumericValue::Integer(decimal_to_bigint(&q)?)
        }
        (NumericValue::Float(l), NumericValue::Float(r)) => {
            NumericValue::Integer(float_idiv(f64::from(l), f64::from(r), |q| {
                f64::from(q as f32)
            })?)
        }
        (NumericValue::Double(l), NumericValue::Double(r)) => {
            NumericValue::Integer(float_idiv(l, r, |q| q)?)
        }
        _ => return Err(XPathError::internal("Numeric idiv type mismatch")),
    };

    Ok(numeric_to_xml_value(result, XmlTypeCode::Integer))
}

/// `idiv` of two `xs:float` or `xs:double` values (both given as `f64`;
/// `round` brings the quotient back to the operands' precision).
///
/// F&O §6.2.5: "If $arg2 is (positive or negative) zero, then an error is
/// raised \[err:FOAR0001\]. If either operand is NaN or if $arg1 is INF or
/// -INF then an error is raised \[err:FOAR0002\]. If $arg2 is INF or -INF (and
/// $arg1 is not) then the result is zero." A quotient too large for the
/// floating-point type is the overflow FOAR0002 names too; any finite quotient
/// becomes the exact xs:integer of its truncation, however large.
fn float_idiv(l: f64, r: f64, round: impl Fn(f64) -> f64) -> Result<BigInt, XPathError> {
    if r == 0.0 {
        return Err(XPathError::FOAR0001);
    }
    if l.is_nan() || r.is_nan() || l.is_infinite() {
        return Err(XPathError::FOAR0002);
    }
    let quotient = round(l / r);
    if quotient.is_infinite() {
        return Err(XPathError::FOAR0002);
    }
    let truncated = quotient.trunc();
    // `{:.0}` prints every digit of a finite integral `f64`.
    format!("{truncated:.0}")
        .parse::<BigInt>()
        .map_err(|_| XPathError::FOAR0002)
}

fn numeric_mod(
    left: NumericValue,
    right: NumericValue,
    result_type: XmlTypeCode,
) -> Result<XmlValue, XPathError> {
    let result = match (left, right) {
        (NumericValue::Integer(l), NumericValue::Integer(r)) => {
            if r == BigInt::from(0) {
                return Err(XPathError::FOAR0001);
            }
            NumericValue::Integer(l % r)
        }
        (NumericValue::Decimal(l), NumericValue::Decimal(r)) => {
            if r.is_zero() {
                return Err(XPathError::FOAR0001);
            }
            NumericValue::Decimal(l.checked_rem(r).ok_or(XPathError::FOAR0002)?)
        }
        (NumericValue::Float(l), NumericValue::Float(r)) => NumericValue::Float(l % r),
        (NumericValue::Double(l), NumericValue::Double(r)) => NumericValue::Double(l % r),
        _ => return Err(XPathError::internal("Numeric mod type mismatch")),
    };
    Ok(numeric_to_xml_value(result, result_type))
}

/// Cast an `UntypedAtomic` value to `xs:double` for arithmetic promotion.
///
/// Per XPath 2.0 Section 3.5.1, `UntypedAtomic` operands in arithmetic
/// expressions are cast to `xs:double`.
fn cast_untyped_to_double(value: &XmlValue) -> Result<XmlValue, XPathError> {
    let s = value.to_string_value();
    let d = crate::xpath::cast::parse_xsd_double(&s)
        .ok_or_else(|| XPathError::invalid_cast_value(&s, "xs:double"))?;
    Ok(XmlValue::double(d))
}

fn promote_numeric(
    left: &XmlValue,
    right: &XmlValue,
) -> Result<(NumericValue, NumericValue, NumericClass), XPathError> {
    // Per XPath 2.0 spec, UntypedAtomic is cast to xs:double for arithmetic
    let left_promoted;
    let left_ref = if left.type_code == XmlTypeCode::UntypedAtomic {
        left_promoted = cast_untyped_to_double(left)?;
        &left_promoted
    } else {
        left
    };
    let right_promoted;
    let right_ref = if right.type_code == XmlTypeCode::UntypedAtomic {
        right_promoted = cast_untyped_to_double(right)?;
        &right_promoted
    } else {
        right
    };

    // XPath 2.0 §3.4: an operand combination that is not valid for the operator
    // is a type error [err:XPTY0004], not an internal failure.
    let not_numeric = || {
        XPathError::binary_operator_not_defined(
            "arithmetic operator",
            type_code_to_name(left_ref.type_code),
            type_code_to_name(right_ref.type_code),
        )
    };
    let left_class = numeric_class(left_ref.type_code).ok_or_else(not_numeric)?;
    let right_class = numeric_class(right_ref.type_code).ok_or_else(not_numeric)?;

    let promotion = numeric_promotion(left_class, right_class);
    let target_class = match promotion {
        Promote::Left => left_class,
        Promote::Right => right_class,
        Promote::None => left_class,
    };

    let left_val = to_numeric_value(left_ref, target_class)?;
    let right_val = to_numeric_value(right_ref, target_class)?;

    Ok((left_val, right_val, target_class))
}

pub(super) fn numeric_class(code: XmlTypeCode) -> Option<NumericClass> {
    match code {
        XmlTypeCode::Byte => Some(NumericClass::Byte),
        XmlTypeCode::UnsignedByte => Some(NumericClass::UnsignedByte),
        XmlTypeCode::Short => Some(NumericClass::Short),
        XmlTypeCode::UnsignedShort => Some(NumericClass::UnsignedShort),
        XmlTypeCode::Int => Some(NumericClass::Int),
        XmlTypeCode::UnsignedInt => Some(NumericClass::UnsignedInt),
        XmlTypeCode::Long => Some(NumericClass::Long),
        XmlTypeCode::UnsignedLong => Some(NumericClass::UnsignedLong),
        XmlTypeCode::Integer
        | XmlTypeCode::NonPositiveInteger
        | XmlTypeCode::NegativeInteger
        | XmlTypeCode::NonNegativeInteger
        | XmlTypeCode::PositiveInteger => Some(NumericClass::Integer),
        XmlTypeCode::Decimal => Some(NumericClass::Decimal),
        XmlTypeCode::Float => Some(NumericClass::Float),
        XmlTypeCode::Double => Some(NumericClass::Double),
        _ => None,
    }
}

fn numeric_precedence(class: NumericClass) -> u8 {
    match class {
        NumericClass::Byte => 0,
        NumericClass::UnsignedByte => 1,
        NumericClass::Short => 2,
        NumericClass::UnsignedShort => 3,
        NumericClass::Int => 4,
        NumericClass::UnsignedInt => 5,
        NumericClass::Long => 6,
        NumericClass::UnsignedLong => 7,
        NumericClass::Integer => 8,
        NumericClass::Decimal => 9,
        NumericClass::Float => 10,
        NumericClass::Double => 11,
    }
}

fn numeric_promotion(left: NumericClass, right: NumericClass) -> Promote {
    let left_rank = numeric_precedence(left);
    let right_rank = numeric_precedence(right);

    match left_rank.cmp(&right_rank) {
        Ordering::Less => Promote::Right,
        Ordering::Greater => Promote::Left,
        Ordering::Equal => Promote::None,
    }
}

fn binary_result_type(op: BinaryOpKind, class: NumericClass) -> XmlTypeCode {
    match op {
        BinaryOpKind::Div => div_result_type(class),
        BinaryOpKind::IDiv => XmlTypeCode::Integer,
        BinaryOpKind::Add | BinaryOpKind::Sub | BinaryOpKind::Mul | BinaryOpKind::Mod => {
            arithmetic_result_type(class)
        }
        _ => arithmetic_result_type(class),
    }
}

fn unary_result_type(class: NumericClass) -> XmlTypeCode {
    match class {
        NumericClass::Byte
        | NumericClass::UnsignedByte
        | NumericClass::Short
        | NumericClass::UnsignedShort => XmlTypeCode::Int,
        NumericClass::UnsignedInt => XmlTypeCode::Long,
        NumericClass::UnsignedLong => XmlTypeCode::Integer,
        NumericClass::Int => XmlTypeCode::Int,
        NumericClass::Long => XmlTypeCode::Long,
        NumericClass::Integer => XmlTypeCode::Integer,
        NumericClass::Decimal => XmlTypeCode::Decimal,
        NumericClass::Float => XmlTypeCode::Float,
        NumericClass::Double => XmlTypeCode::Double,
    }
}

fn arithmetic_result_type(class: NumericClass) -> XmlTypeCode {
    match class {
        NumericClass::Byte
        | NumericClass::UnsignedByte
        | NumericClass::Short
        | NumericClass::UnsignedShort => XmlTypeCode::Int,
        NumericClass::UnsignedInt => XmlTypeCode::UnsignedInt,
        NumericClass::Int => XmlTypeCode::Int,
        NumericClass::Long => XmlTypeCode::Long,
        NumericClass::UnsignedLong => XmlTypeCode::Integer,
        NumericClass::Integer => XmlTypeCode::Integer,
        NumericClass::Decimal => XmlTypeCode::Decimal,
        NumericClass::Float => XmlTypeCode::Float,
        NumericClass::Double => XmlTypeCode::Double,
    }
}

fn div_result_type(class: NumericClass) -> XmlTypeCode {
    match class {
        NumericClass::Decimal => XmlTypeCode::Decimal,
        NumericClass::Float => XmlTypeCode::Float,
        NumericClass::Double => XmlTypeCode::Double,
        _ => XmlTypeCode::Decimal,
    }
}

fn is_integer_class(class: NumericClass) -> bool {
    matches!(
        class,
        NumericClass::Byte
            | NumericClass::UnsignedByte
            | NumericClass::Short
            | NumericClass::UnsignedShort
            | NumericClass::Int
            | NumericClass::UnsignedInt
            | NumericClass::Long
            | NumericClass::UnsignedLong
            | NumericClass::Integer
    )
}

/// Whether an `xs:decimal` or `xs:integer` operand is zero. (An `xs:decimal`
/// carried as a string is never zero: zero always fits a `Decimal`.)
fn is_exact_zero(value: &XmlValue) -> bool {
    match &value.value {
        XmlValueKind::Atomic(XmlAtomicValue::Decimal(d)) => d.is_zero(),
        XmlValueKind::Atomic(XmlAtomicValue::Integer(i)) => i.sign() == num_bigint::Sign::NoSign,
        _ => false,
    }
}

fn to_integer_value(value: &XmlValue) -> Result<BigInt, XPathError> {
    match &value.value {
        XmlValueKind::Atomic(XmlAtomicValue::Integer(v)) => Ok(v.clone()),
        _ => Err(XPathError::internal("Expected integer value")),
    }
}

fn to_numeric_value(value: &XmlValue, class: NumericClass) -> Result<NumericValue, XPathError> {
    match class {
        NumericClass::Decimal => Ok(NumericValue::Decimal(decimal_operand(value)?)),
        NumericClass::Float => {
            let val = value
                .as_double()
                .ok_or_else(|| XPathError::internal("Expected numeric value"))?;
            Ok(NumericValue::Float(val as f32))
        }
        NumericClass::Double => {
            let val = value
                .as_double()
                .ok_or_else(|| XPathError::internal("Expected numeric value"))?;
            Ok(NumericValue::Double(val))
        }
        _ => Ok(NumericValue::Integer(to_integer_value(value)?)),
    }
}

/// An `xs:integer` promoted to `xs:decimal`; FOAR0002 beyond
/// `rust_decimal`'s range (F&O §6.2, decimal overflow).
fn decimal_from_bigint(value: &BigInt) -> Result<Decimal, XPathError> {
    value
        .to_string()
        .parse::<Decimal>()
        .map_err(|_| XPathError::FOAR0002)
}

fn decimal_to_bigint(value: &Decimal) -> Result<BigInt, XPathError> {
    value
        .trunc()
        .to_string()
        .parse::<BigInt>()
        .map_err(|_| XPathError::internal("Failed to convert decimal to integer"))
}

fn numeric_to_xml_value(value: NumericValue, type_code: XmlTypeCode) -> XmlValue {
    let value = match value {
        NumericValue::Integer(v) => XmlValueKind::Atomic(XmlAtomicValue::Integer(v)),
        // xs:decimal has no negative zero (Datatypes §3.3.3): `-0.0` is 0,
        // whose canonical form is "0", not "-0". Float and double keep
        // IEEE's negative zero.
        NumericValue::Decimal(v) => XmlValueKind::Atomic(XmlAtomicValue::Decimal(
            crate::types::exact_decimal::unsigned_zero(v),
        )),
        NumericValue::Float(v) => XmlValueKind::Atomic(XmlAtomicValue::Float(v)),
        NumericValue::Double(v) => XmlValueKind::Atomic(XmlAtomicValue::Double(v)),
    };

    XmlValue {
        type_code,
        schema_type: None,
        value,
    }
}

fn integer_pair(left: NumericValue, right: NumericValue) -> Result<(BigInt, BigInt), XPathError> {
    match (left, right) {
        (NumericValue::Integer(l), NumericValue::Integer(r)) => Ok((l, r)),
        _ => Err(XPathError::internal("Expected integer values")),
    }
}

fn decimal_pair(left: NumericValue, right: NumericValue) -> Result<(Decimal, Decimal), XPathError> {
    match (left, right) {
        (NumericValue::Decimal(l), NumericValue::Decimal(r)) => Ok((l, r)),
        _ => Err(XPathError::internal("Expected decimal values")),
    }
}

fn float_pair(left: NumericValue, right: NumericValue) -> Result<(f32, f32), XPathError> {
    match (left, right) {
        (NumericValue::Float(l), NumericValue::Float(r)) => Ok((l, r)),
        _ => Err(XPathError::internal("Expected float values")),
    }
}

fn double_pair(left: NumericValue, right: NumericValue) -> Result<(f64, f64), XPathError> {
    match (left, right) {
        (NumericValue::Double(l), NumericValue::Double(r)) => Ok((l, r)),
        _ => Err(XPathError::internal("Expected double values")),
    }
}

fn xml_datetime_value(type_code: XmlTypeCode, value: DateTimeValue) -> XmlValue {
    XmlValue {
        type_code,
        schema_type: None,
        value: XmlValueKind::Atomic(XmlAtomicValue::DateTime(value)),
    }
}

fn xml_date_value(type_code: XmlTypeCode, value: DateValue) -> XmlValue {
    XmlValue {
        type_code,
        schema_type: None,
        value: XmlValueKind::Atomic(XmlAtomicValue::Date(value)),
    }
}

fn xml_time_value(type_code: XmlTypeCode, value: TimeValue) -> XmlValue {
    XmlValue {
        type_code,
        schema_type: None,
        value: XmlValueKind::Atomic(XmlAtomicValue::Time(value)),
    }
}

fn xml_year_month_duration_value(value: YearMonthDurationValue) -> XmlValue {
    XmlValue {
        type_code: XmlTypeCode::YearMonthDuration,
        schema_type: None,
        value: XmlValueKind::Atomic(XmlAtomicValue::YearMonthDuration(value)),
    }
}

fn xml_day_time_duration_value(value: DayTimeDurationValue) -> XmlValue {
    XmlValue {
        type_code: XmlTypeCode::DayTimeDuration,
        schema_type: None,
        value: XmlValueKind::Atomic(XmlAtomicValue::DayTimeDuration(value)),
    }
}

pub(super) fn is_temporal_type(code: XmlTypeCode) -> bool {
    matches!(
        code,
        XmlTypeCode::DateTime
            | XmlTypeCode::DateTimeStamp
            | XmlTypeCode::Date
            | XmlTypeCode::Time
            | XmlTypeCode::Duration
            | XmlTypeCode::YearMonthDuration
            | XmlTypeCode::DayTimeDuration
    )
}

pub(super) fn is_date_time_code(code: XmlTypeCode) -> bool {
    matches!(code, XmlTypeCode::DateTime | XmlTypeCode::DateTimeStamp)
}

pub(super) fn is_duration_code(code: XmlTypeCode) -> bool {
    matches!(
        code,
        XmlTypeCode::Duration | XmlTypeCode::YearMonthDuration | XmlTypeCode::DayTimeDuration
    )
}

fn as_datetime(value: &XmlValue) -> Option<&DateTimeValue> {
    match &value.value {
        XmlValueKind::Atomic(XmlAtomicValue::DateTime(v)) => Some(v),
        _ => None,
    }
}

fn as_date(value: &XmlValue) -> Option<&DateValue> {
    match &value.value {
        XmlValueKind::Atomic(XmlAtomicValue::Date(v)) => Some(v),
        _ => None,
    }
}

fn as_time(value: &XmlValue) -> Option<&TimeValue> {
    match &value.value {
        XmlValueKind::Atomic(XmlAtomicValue::Time(v)) => Some(v),
        _ => None,
    }
}

fn as_year_month_duration(value: &XmlValue) -> Option<&YearMonthDurationValue> {
    match &value.value {
        XmlValueKind::Atomic(XmlAtomicValue::YearMonthDuration(v)) => Some(v),
        _ => None,
    }
}

fn as_day_time_duration(value: &XmlValue) -> Option<&DayTimeDurationValue> {
    match &value.value {
        XmlValueKind::Atomic(XmlAtomicValue::DayTimeDuration(v)) => Some(v),
        _ => None,
    }
}

/// The gregorian types, on which only `eq` and `ne` are defined (F&O 1.0
/// §10.1: "only equality is defined on xs:gYearMonth, xs:gYear,
/// xs:gMonthDay, xs:gMonth and xs:gDay values").
pub(super) fn is_gregorian_code(code: XmlTypeCode) -> bool {
    matches!(
        code,
        XmlTypeCode::GYearMonth
            | XmlTypeCode::GYear
            | XmlTypeCode::GMonthDay
            | XmlTypeCode::GMonth
            | XmlTypeCode::GDay
    )
}

// ----------------------------------------------------------------------------
// Exact keys
// ----------------------------------------------------------------------------
//
// Date/time values are compared, and subtracted, on the time line the schema
// facets use (`types::value::time_line_seconds`): whole seconds from
// 1970-01-01T00:00:00Z in `i128` and the fraction of a second in `[0, 1)`
// apart, so no comparison rounds and no sum overflows. Durations are compared
// on `types::value_order::DurationParts` — months and seconds, exactly.
//
// The keys read a value's own timezone, or the implicit timezone `tz` when it
// has none (F&O §10.4), and its year in the XSD 1.0 numbering the XPath
// arithmetic uses (`astronomical_year`).

/// A position on the time line: whole seconds from 1970-01-01T00:00:00Z and
/// the fraction of a second in `[0, 1)`, compared lexicographically.
pub(super) type TimeLineKey = (i128, Decimal);

/// The proleptic Gregorian (astronomical) year of an XPath date/time year.
///
/// The XPath arithmetic numbers years as XSD 1.0 does: `-0001` is the year
/// before `0001` (1 BCE). A year `0000`, which the XSD 1.1 lexical space
/// admits, is read as `0001` (F&O 1.0 §10.1.1: "The results of operations on
/// dates that cross the year 0000 are ·implementation-defined·").
fn astronomical_year(year: i32) -> i32 {
    if year <= 0 {
        year + 1
    } else {
        year
    }
}

/// The XPath year of an astronomical year — FODT0001 beyond the `i32` the
/// value structs hold (F&O §10.1.1: "It ·must· raise an error
/// \[err:FODT0001\] in case of overflow").
fn xpath_year(astronomical: i128) -> Result<i32, XPathError> {
    let year = if astronomical <= 0 {
        astronomical - 1
    } else {
        astronomical
    };
    i32::try_from(year).map_err(|_| XPathError::FODT0001)
}

/// `value`'s instant, with `tz` for a missing timezone.
fn datetime_key(value: &DateTimeValue, tz: TimezoneOffset) -> TimeLineKey {
    time_line_seconds(
        day_number(astronomical_year(value.year), value.month, value.day),
        value.hour,
        value.minute,
        value.second,
        Some(value.timezone.unwrap_or(tz)),
    )
}

/// The starting instant of an `xs:date` (F&O §10.4.9: "the xs:dateTime at
/// time 00:00:00 on that date").
fn date_key(value: &DateValue, tz: TimezoneOffset) -> TimeLineKey {
    time_line_seconds(
        day_number(astronomical_year(value.year), value.month, value.day),
        0,
        0,
        Decimal::ZERO,
        Some(value.timezone.unwrap_or(tz)),
    )
}

/// An `xs:time` on the reference date (F&O §10.4.12; one reference date for
/// every time, so it is day 0 here).
fn time_key(value: &TimeValue, tz: TimezoneOffset) -> TimeLineKey {
    time_line_seconds(
        0,
        value.hour,
        value.minute,
        value.second,
        Some(value.timezone.unwrap_or(tz)),
    )
}

/// The starting instant of a gregorian value (F&O §10.4.15 – §10.4.19): the
/// missing components from a reference dateTime — 1972 (a leap year) for
/// the month and day types, December for `xs:gDay` (31 days) — and the
/// first day of the month. (§10.4.15 and §10.4.18 fill in the last day; any
/// day gives the same equality, since only values of one month are equal.)
fn gregorian_key(value: &XmlValue, tz: TimezoneOffset) -> Option<TimeLineKey> {
    let (days, timezone) = match &value.value {
        XmlValueKind::Atomic(XmlAtomicValue::GYearMonth(v)) => (
            day_number(astronomical_year(v.year), v.month, 1),
            v.timezone,
        ),
        XmlValueKind::Atomic(XmlAtomicValue::GYear(v)) => {
            (day_number(astronomical_year(v.year), 1, 1), v.timezone)
        }
        XmlValueKind::Atomic(XmlAtomicValue::GMonthDay(v)) => {
            (day_number(1972, v.month, v.day), v.timezone)
        }
        XmlValueKind::Atomic(XmlAtomicValue::GMonth(v)) => {
            (day_number(1972, v.month, 1), v.timezone)
        }
        XmlValueKind::Atomic(XmlAtomicValue::GDay(v)) => (day_number(1972, 12, v.day), v.timezone),
        _ => return None,
    };
    Some(time_line_seconds(
        days,
        0,
        0,
        Decimal::ZERO,
        Some(timezone.unwrap_or(tz)),
    ))
}

/// The months and seconds of any duration value (F&O §10.4.5), exactly.
fn duration_key(value: &XmlValue) -> Option<DurationParts> {
    match &value.value {
        XmlValueKind::Atomic(atom) => crate::types::value_order::duration_parts(atom),
        _ => None,
    }
}

/// The seconds of a duration as a time line offset: whole seconds and the
/// fraction in `[0, 1)`.
fn seconds_key(parts: &DurationParts) -> TimeLineKey {
    (parts.seconds, parts.fraction)
}

// ----------------------------------------------------------------------------
// Comparison keys for the indexed general comparison
// ----------------------------------------------------------------------------
//
// Each of these exposes the *exact* key `compare_eq` compares a value of that
// family by, so that `general_compare` can key a hash table on it without
// inventing a second notion of equality. `None` means the value does not hold
// the expected representation, which is the case in which the comparison
// itself would raise.

/// The instant `eq` compares an `xs:dateTime`/`xs:dateTimeStamp` value at.
pub(super) fn datetime_compare_key(value: &XmlValue, tz: TimezoneOffset) -> Option<TimeLineKey> {
    as_datetime(value).map(|v| datetime_key(v, tz))
}

/// The instant `eq` compares an `xs:date` value at.
pub(super) fn date_compare_key(value: &XmlValue, tz: TimezoneOffset) -> Option<TimeLineKey> {
    as_date(value).map(|v| date_key(v, tz))
}

/// The instant `eq` compares an `xs:time` value at.
pub(super) fn time_compare_key(value: &XmlValue, tz: TimezoneOffset) -> Option<TimeLineKey> {
    as_time(value).map(|v| time_key(v, tz))
}

/// The starting instant `eq` compares a gregorian value at.
pub(super) fn gregorian_compare_key(value: &XmlValue, tz: TimezoneOffset) -> Option<TimeLineKey> {
    gregorian_key(value, tz)
}

/// The months and seconds `eq` compares any duration value by.
pub(super) fn duration_compare_key(value: &XmlValue) -> Option<DurationParts> {
    duration_key(value)
}

fn numeric_to_f64(value: &XmlValue) -> Result<f64, XPathError> {
    let val = value
        .as_double()
        .ok_or_else(|| XPathError::internal("Expected numeric value"))?;
    if !val.is_finite() {
        return Err(XPathError::internal("Numeric value is not finite"));
    }
    Ok(val)
}

fn numeric_to_decimal(value: &XmlValue) -> Result<Decimal, XPathError> {
    match value.type_code {
        XmlTypeCode::Float | XmlTypeCode::Double => {
            let val = numeric_to_f64(value)?;
            Decimal::from_f64(val)
                .ok_or_else(|| XPathError::internal("Failed to convert float to decimal"))
        }
        _ => decimal_operand(value),
    }
}

/// The numeric multiplier or divisor of a duration as an `xs:double`, NaN and
/// the infinities included (F&O §10.6.3, §10.6.4, §10.6.8, §10.6.9 name what
/// each of them does).
fn numeric_factor(value: &XmlValue) -> Result<f64, XPathError> {
    value
        .as_double()
        .ok_or_else(|| XPathError::internal("Expected numeric value"))
}

/// The `fn:implicit-timezone` of a context that has none: the host's local
/// offset now (`chrono::Local`).
///
/// Used by the public functions that take no dynamic context
/// ([`value_eq`], [`eval_binary`], …); a dynamic context reads its own once
/// per evaluation ([`DynamicContext::implicit_timezone_or_host`]).
///
/// [`DynamicContext::implicit_timezone_or_host`]: crate::xpath::DynamicContext::implicit_timezone_or_host
pub(crate) fn host_implicit_timezone() -> TimezoneOffset {
    crate::xpath::context::host_offset_at(chrono::Utc::now())
}

/// The host's implicit timezone when `left` or `right` is a value the
/// implicit timezone can apply to (a date/time or gregorian value), else `Z`,
/// which nothing reads — so the context-free functions consult the host's
/// clock only for the comparisons that need it.
fn host_timezone_for(left: &XmlValue, right: &XmlValue) -> TimezoneOffset {
    let zoned = |value: &XmlValue| {
        let code = unwrap_union_value(value).type_code;
        (is_temporal_type(code) && !is_duration_code(code)) || is_gregorian_code(code)
    };
    if zoned(left) || zoned(right) {
        host_implicit_timezone()
    } else {
        TimezoneOffset::UTC
    }
}

/// The implicit timezone of a static context: its own, or the host's.
pub(super) fn static_timezone(context: &XPathContext) -> TimezoneOffset {
    context
        .implicit_timezone
        .unwrap_or_else(host_implicit_timezone)
}

// ----------------------------------------------------------------------------
// Exact date/time and duration arithmetic
// ----------------------------------------------------------------------------
//
// F&O 1.0 §10.1.1: a processor that limits the representation of date/time
// values "·must· raise an error [err:FODT0001] in case of overflow" in the
// arithmetic of §10.8, and one that limits durations "·must· raise an error
// [err:FODT0002] in case of overflow" in that of §10.6. The limits are the
// value structs' own: a year in `i32`, the day, hour and minute fields of a
// duration in `u32`, the months of a `xs:yearMonthDuration` below
// 2³² years, seconds in a `Decimal`. Results are computed exactly (whole
// seconds in `i128`, the fraction apart) and checked against those limits;
// only the fraction of a result's seconds field can exceed what a `Decimal`
// holds next to the whole seconds, and it is then truncated toward zero
// (`seconds_with_fraction`), never rounded up to the next second.

/// `key + parts.seconds` on the time line, exactly (the months of `parts`
/// are not added here).
fn add_seconds(key: TimeLineKey, parts: &DurationParts) -> TimeLineKey {
    let (mut whole, fraction) = key;
    whole += parts.seconds;
    // Two fractions in [0, 1) of at most 28 digits: the sum is exact.
    let mut fraction = fraction + parts.fraction;
    if fraction >= Decimal::ONE {
        fraction -= Decimal::ONE;
        whole += 1;
    }
    (whole, fraction.normalize())
}

/// The negation of a duration's parts, with the fraction back in `[0, 1)`.
fn negated(parts: DurationParts) -> DurationParts {
    if parts.fraction.is_zero() {
        DurationParts {
            months: -parts.months,
            seconds: -parts.seconds,
            fraction: Decimal::ZERO,
        }
    } else {
        DurationParts {
            months: -parts.months,
            seconds: -parts.seconds - 1,
            fraction: Decimal::ONE - parts.fraction,
        }
    }
}

/// `whole + fraction` as the seconds field of a value (`whole` below 60):
/// exact when the sum fits a `Decimal`, else with the fraction truncated
/// toward zero to the digits that fit — and then `truncated` is set, for the
/// caller to report `xs-seconds-capacity` (the fraction is normalized, so any
/// digit dropped is a nonzero one).
fn seconds_with_fraction(whole: u32, fraction: Decimal, truncated: &mut bool) -> Decimal {
    let fraction = fraction.normalize();
    let mut scale = fraction.scale();
    loop {
        let digits = fraction.trunc_with_scale(scale);
        let mantissa = digits.mantissa() * 10i128.pow(scale - digits.scale())
            + i128::from(whole) * 10i128.pow(scale);
        if let Ok(seconds) = Decimal::try_from_i128_with_scale(mantissa, scale) {
            if scale < fraction.scale() {
                *truncated = true;
            }
            return seconds;
        }
        // Scale 0 always fits: `whole` is below 60.
        scale -= 1;
    }
}

/// The `xs:dayTimeDuration` of `seconds + fraction` seconds (`fraction` in
/// `[0, 1)`), or `overflow` when its days exceed what the value struct holds.
fn day_time_from_exact(
    seconds: i128,
    fraction: Decimal,
    overflow: XPathError,
    truncated: &mut bool,
) -> Result<DayTimeDurationValue, XPathError> {
    let negative = seconds < 0;
    let (whole, fraction) = if !negative {
        (seconds, fraction)
    } else if fraction.is_zero() {
        (-seconds, Decimal::ZERO)
    } else {
        (-seconds - 1, Decimal::ONE - fraction)
    };
    let days = u32::try_from(whole / 86_400).map_err(|_| overflow)?;
    let rest = whole % 86_400;
    Ok(DayTimeDurationValue {
        negative,
        days,
        // Below 24, 60 and 60.
        hours: (rest / 3_600) as u32,
        minutes: (rest % 3_600 / 60) as u32,
        seconds: seconds_with_fraction((rest % 60) as u32, fraction, truncated),
    })
}

/// The `xs:dayTimeDuration` between two instants (F&O §10.8.1 – §10.8.3);
/// FODT0001 beyond the value struct (§10.1.1).
fn time_line_difference(
    left: TimeLineKey,
    right: TimeLineKey,
    truncated: &mut bool,
) -> Result<DayTimeDurationValue, XPathError> {
    let mut seconds = left.0 - right.0;
    let mut fraction = left.1 - right.1;
    if fraction.is_sign_negative() && !fraction.is_zero() {
        fraction += Decimal::ONE;
        seconds -= 1;
    }
    day_time_from_exact(seconds, fraction, XPathError::FODT0001, truncated)
}

/// The local (timezone-free) position of a date/time: the fields read as
/// UTC.
fn local_key(year: i32, month: u8, day: u8, hour: u8, minute: u8, second: Decimal) -> TimeLineKey {
    time_line_seconds(
        day_number(astronomical_year(year), month, day),
        hour,
        minute,
        second,
        None,
    )
}

/// The calendar fields of a local position, or FODT0001 when its year is
/// beyond the `i32` the value structs hold.
fn civil_fields(
    key: TimeLineKey,
    truncated: &mut bool,
) -> Result<(i32, u8, u8, u8, u8, Decimal), XPathError> {
    let (whole, fraction) = key;
    let days = whole.div_euclid(86_400);
    let second_of_day = whole.rem_euclid(86_400);
    let (astronomical, month, day) = civil_from_days(days);
    let year = xpath_year(astronomical)?;
    Ok((
        year,
        month,
        day,
        (second_of_day / 3_600) as u8,
        (second_of_day % 3_600 / 60) as u8,
        seconds_with_fraction((second_of_day % 60) as u32, fraction, truncated),
    ))
}

/// F&O §10.8.4, §10.8.5 (and the subtractions): the months are added to the
/// local value, the day pinned to the month's length, the timezone kept.
fn add_datetime_year_month(
    value: &DateTimeValue,
    months: i64,
) -> Result<DateTimeValue, XPathError> {
    let (year, month, day) = add_months_to_date(value.year, value.month, value.day, months)?;
    Ok(DateTimeValue {
        year,
        month,
        day,
        hour: value.hour,
        minute: value.minute,
        second: value.second,
        timezone: value.timezone,
    })
}

fn add_date_year_month(value: &DateValue, months: i64) -> Result<DateValue, XPathError> {
    let (year, month, day) = add_months_to_date(value.year, value.month, value.day, months)?;
    Ok(DateValue {
        year,
        month,
        day,
        timezone: value.timezone,
    })
}

/// F&O §10.8.6: the seconds are added to the local value; the timezone kept.
fn add_datetime_day_time(
    value: &DateTimeValue,
    duration: &DurationParts,
    truncated: &mut bool,
) -> Result<DateTimeValue, XPathError> {
    let key = local_key(
        value.year,
        value.month,
        value.day,
        value.hour,
        value.minute,
        value.second,
    );
    let (year, month, day, hour, minute, second) =
        civil_fields(add_seconds(key, duration), truncated)?;
    Ok(DateTimeValue {
        year,
        month,
        day,
        hour,
        minute,
        second,
        timezone: value.timezone,
    })
}

/// F&O §10.8.7: the date of the starting instant plus the seconds.
fn add_date_day_time(value: &DateValue, duration: &DurationParts) -> Result<DateValue, XPathError> {
    let key = local_key(value.year, value.month, value.day, 0, 0, Decimal::ZERO);
    // A date keeps no time of day (§10.8.7: the date of the instant), so no
    // seconds field is truncated here.
    let (year, month, day, ..) = civil_fields(add_seconds(key, duration), &mut false)?;
    Ok(DateValue {
        year,
        month,
        day,
        timezone: value.timezone,
    })
}

/// F&O §10.8.9: the time of day plus the seconds, modulo a day.
fn add_time_day_time(
    value: &TimeValue,
    duration: &DurationParts,
    truncated: &mut bool,
) -> TimeValue {
    let key = time_line_seconds(0, value.hour, value.minute, value.second, None);
    let (whole, fraction) = add_seconds(key, duration);
    let second_of_day = whole.rem_euclid(86_400);
    TimeValue {
        hour: (second_of_day / 3_600) as u8,
        minute: (second_of_day % 3_600 / 60) as u8,
        second: seconds_with_fraction((second_of_day % 60) as u32, fraction, truncated),
        timezone: value.timezone,
    }
}

fn year_month_total_months(value: &YearMonthDurationValue) -> i64 {
    let total = value.years as i64 * 12 + value.months as i64;
    if value.negative {
        -total
    } else {
        total
    }
}

/// The `xs:yearMonthDuration` of `months` months, or FODT0002 when its years
/// exceed the `u32` the value struct holds (F&O §10.1.1).
fn year_month_from_months(months: i64) -> Result<YearMonthDurationValue, XPathError> {
    let negative = months < 0;
    let abs_months = months.unsigned_abs();
    let years = u32::try_from(abs_months / 12).map_err(|_| XPathError::FODT0002)?;
    Ok(YearMonthDurationValue {
        negative,
        years,
        months: (abs_months % 12) as u32,
    })
}

/// A month count computed in floating point, rounded to the nearest month
/// (F&O §10.6.3: "The result is rounded to the nearest month"), or FODT0002
/// beyond the value struct.
fn year_month_from_rounded(months: f64) -> Result<YearMonthDurationValue, XPathError> {
    const MAX_MONTHS: f64 = u32::MAX as f64 * 12.0 + 11.0;
    let rounded = (months + 0.5).floor();
    if !rounded.is_finite() || rounded.abs() > MAX_MONTHS {
        return Err(XPathError::FODT0002);
    }
    year_month_from_months(rounded as i64)
}

/// F&O §10.6.3: "If $arg2 is positive or negative zero, the result is a
/// zero-length duration. If $arg2 is positive or negative infinity, the
/// result overflows and is handled as discussed in 10.1.1 Limits and
/// Precision. If $arg2 is NaN an error is raised \[err:FOCA0005\]".
fn year_month_mul_numeric(
    value: &YearMonthDurationValue,
    factor: f64,
) -> Result<YearMonthDurationValue, XPathError> {
    if factor.is_nan() {
        return Err(XPathError::FOCA0005);
    }
    if factor.is_infinite() {
        return Err(XPathError::FODT0002);
    }
    year_month_from_rounded(year_month_total_months(value) as f64 * factor)
}

/// F&O §10.6.4: "If $arg2 is positive or negative infinity, the result is a
/// zero-length duration. If $arg2 is positive or negative zero, the result
/// overflows … If $arg2 is NaN an error is raised \[err:FOCA0005\]".
fn year_month_div_numeric(
    value: &YearMonthDurationValue,
    divisor: f64,
) -> Result<YearMonthDurationValue, XPathError> {
    if divisor.is_nan() {
        return Err(XPathError::FOCA0005);
    }
    if divisor == 0.0 {
        return Err(XPathError::FODT0002);
    }
    if divisor.is_infinite() {
        return year_month_from_months(0);
    }
    year_month_from_rounded(year_month_total_months(value) as f64 / divisor)
}

/// F&O §10.6.5: "identical to op:numeric-divide with xs:integer operands" —
/// FOAR0001 for a zero divisor (§6.2.4).
fn year_month_div_duration(
    left: &YearMonthDurationValue,
    right: &YearMonthDurationValue,
) -> Result<Decimal, XPathError> {
    let right_months = year_month_total_months(right);
    if right_months == 0 {
        return Err(XPathError::FOAR0001);
    }
    Decimal::from(year_month_total_months(left))
        .checked_div(Decimal::from(right_months))
        .ok_or(XPathError::FOAR0002)
}

/// The seconds of a dayTimeDuration as one `Decimal` — `overflow` when they
/// exceed what a `Decimal` holds (2⁹⁶ − 1); the fraction rounded when the sum
/// has more digits than it holds.
fn day_time_decimal_seconds(
    value: &DayTimeDurationValue,
    overflow: XPathError,
) -> Result<Decimal, XPathError> {
    let parts = day_time_duration_parts(value);
    Decimal::from_i128(parts.seconds)
        .and_then(|whole| whole.checked_add(parts.fraction))
        .ok_or(overflow)
}

/// The `xs:dayTimeDuration` of a `Decimal` number of seconds; FODT0002
/// beyond the value struct.
fn day_time_from_decimal_seconds(
    seconds: Decimal,
    truncated: &mut bool,
) -> Result<DayTimeDurationValue, XPathError> {
    let whole = seconds.floor();
    let fraction = seconds - whole;
    let whole = whole.to_i128().ok_or(XPathError::FODT0002)?;
    day_time_from_exact(whole, fraction, XPathError::FODT0002, truncated)
}

/// F&O §10.6.8: "If $arg2 is positive or negative zero, the result is a
/// zero-length duration. If $arg2 is positive or negative infinity, the
/// result overflows … If $arg2 is NaN an error is raised \[err:FOCA0005\]".
/// A factor beyond `Decimal`'s range is applied in `f64`.
fn day_time_mul_numeric_value(
    value: &DayTimeDurationValue,
    factor: &XmlValue,
    truncated: &mut bool,
) -> Result<DayTimeDurationValue, XPathError> {
    let as_double = numeric_factor(factor)?;
    if as_double.is_nan() {
        return Err(XPathError::FOCA0005);
    }
    if as_double.is_infinite() {
        return Err(XPathError::FODT0002);
    }
    let seconds = day_time_decimal_seconds(value, XPathError::FODT0002)?;
    let total = match numeric_to_decimal(factor) {
        Ok(d) => seconds.checked_mul(d).ok_or(XPathError::FODT0002)?,
        Err(_) => {
            let total = seconds
                .to_f64()
                .ok_or_else(|| XPathError::internal("Failed to convert seconds to f64"))?;
            Decimal::from_f64(total * as_double).ok_or(XPathError::FODT0002)?
        }
    };
    day_time_from_decimal_seconds(total, truncated)
}

/// F&O §10.6.9: "If $arg2 is positive or negative infinity, the result is a
/// zero-length duration. If $arg2 is positive or negative zero, the result
/// overflows … If $arg2 is NaN an error is raised \[err:FOCA0005\]".
fn day_time_div_numeric_value(
    value: &DayTimeDurationValue,
    divisor: &XmlValue,
    truncated: &mut bool,
) -> Result<DayTimeDurationValue, XPathError> {
    let as_double = numeric_factor(divisor)?;
    if as_double.is_nan() {
        return Err(XPathError::FOCA0005);
    }
    if as_double == 0.0 {
        return Err(XPathError::FODT0002);
    }
    if as_double.is_infinite() {
        return day_time_from_exact(0, Decimal::ZERO, XPathError::FODT0002, truncated);
    }
    let seconds = day_time_decimal_seconds(value, XPathError::FODT0002)?;
    let total = match numeric_to_decimal(divisor) {
        Ok(d) if !d.is_zero() => seconds.checked_div(d).ok_or(XPathError::FODT0002)?,
        _ => {
            let total = seconds
                .to_f64()
                .ok_or_else(|| XPathError::internal("Failed to convert seconds to f64"))?;
            Decimal::from_f64(total / as_double).ok_or(XPathError::FODT0002)?
        }
    };
    day_time_from_decimal_seconds(total, truncated)
}

/// F&O §10.6.10: "identical to op:numeric-divide with xs:decimal operands" —
/// FOAR0001 for a zero divisor, FOAR0002 when an operand's seconds or the
/// quotient exceed what an `xs:decimal` holds here (§6.2).
fn day_time_div_duration(
    left: &DayTimeDurationValue,
    right: &DayTimeDurationValue,
) -> Result<Decimal, XPathError> {
    let right_seconds = day_time_decimal_seconds(right, XPathError::FOAR0002)?;
    if right_seconds.is_zero() {
        return Err(XPathError::FOAR0001);
    }
    day_time_decimal_seconds(left, XPathError::FOAR0002)?
        .checked_div(right_seconds)
        .ok_or(XPathError::FOAR0002)
}

/// The date `delta_months` months from `year-month-day`, the day pinned to
/// the length of the new month; FODT0001 when the year leaves `i32`.
fn add_months_to_date(
    year: i32,
    month: u8,
    day: u8,
    delta_months: i64,
) -> Result<(i32, u8, u8), XPathError> {
    // |delta_months| is below 13 × 2³²: no overflow in i64.
    let total = i64::from(astronomical_year(year)) * 12 + i64::from(month) - 1 + delta_months;
    let year = xpath_year(i128::from(total.div_euclid(12)))?;
    let month = (total.rem_euclid(12) + 1) as u8;
    let day = day.min(days_in_month(year, month)?);
    Ok((year, month, day))
}

fn days_in_month(year: i32, month: u8) -> Result<u8, XPathError> {
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => return Err(XPathError::internal("Invalid month value")),
    };
    Ok(days)
}

fn is_leap_year(xsd_year: i32) -> bool {
    // The astronomical year, with year 0, for the Gregorian rule.
    let year = i64::from(astronomical_year(xsd_year));
    year.rem_euclid(4) == 0 && (year.rem_euclid(100) != 0 || year.rem_euclid(400) == 0)
}

/// The astronomical year, month and day of a day number counted from
/// 1970-01-01 — the inverse of `types::value::day_number`, for every day an
/// `i128` holds.
fn civil_from_days(days: i128) -> (i128, u8, u8) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    (year, m as u8, d as u8)
}

pub(super) fn is_string_like(code: XmlTypeCode) -> bool {
    code.is_string_derived() || matches!(code, XmlTypeCode::AnyUri | XmlTypeCode::UntypedAtomic)
}

// ============================================================================
// General Comparison Support (for sequence comparisons)
// ============================================================================

/// Perform magnitude relationship promotion for general comparisons.
///
/// When comparing values in a general comparison, UntypedAtomic values are
/// promoted to match the type of the other operand according to XPath 2.0 rules:
///
/// - If the other operand is numeric, UntypedAtomic is promoted to xs:double
/// - If the other operand is string, UntypedAtomic is kept as string
/// - If the other operand is a typed value, UntypedAtomic is cast to that type
///
/// # Arguments
///
/// * `left` - The left operand
/// * `right` - The right operand
///
/// # Returns
///
/// A tuple of (promoted_left, promoted_right) suitable for comparison
pub fn magnitude_relationship(
    left: &XmlValue,
    right: &XmlValue,
) -> Result<(XmlValue, XmlValue), XPathError> {
    let mut left_result = left.clone();
    let mut right_result = right.clone();

    // Promote left if it's UntypedAtomic
    if left.type_code == XmlTypeCode::UntypedAtomic {
        if right.type_code.is_numeric() {
            // Promote to double
            let s = left.to_string_value();
            let d = crate::xpath::cast::parse_xsd_double(&s)
                .ok_or_else(|| XPathError::invalid_cast_value(&s, "xs:double"))?;
            left_result = XmlValue::double(d);
        } else if is_string_like(right.type_code) {
            // Keep as string
            left_result = XmlValue::string(left.to_string_value());
        }
        // For other types, we'd need full cast support - for now, keep as string
    }

    // Promote right if it's UntypedAtomic
    if right.type_code == XmlTypeCode::UntypedAtomic {
        if left_result.type_code.is_numeric() {
            // Promote to double
            let s = right.to_string_value();
            let d = crate::xpath::cast::parse_xsd_double(&s)
                .ok_or_else(|| XPathError::invalid_cast_value(&s, "xs:double"))?;
            right_result = XmlValue::double(d);
        } else if is_string_like(left_result.type_code) {
            // Keep as string
            right_result = XmlValue::string(right.to_string_value());
        }
        // For other types, we'd need full cast support - for now, keep as string
    }

    Ok((left_result, right_result))
}

// ============================================================================
// Schema-aware primitive type resolution for magnitude relationship
// ============================================================================

/// Get the primitive base type for a schema-defined simple type.
///
/// Per XPath 2.0 spec, when casting untypedAtomic in general comparisons,
/// we cast to the **primitive base type** of the other operand's dynamic type.
/// This walks the base type chain to find the built-in primitive type.
///
/// Returns the XmlTypeCode of the primitive base type, or the input type_code
/// if no schema information is available.
fn get_primitive_base_type(
    schema_set: Option<&SchemaSet>,
    schema_type: Option<SimpleTypeKey>,
    type_code: XmlTypeCode,
) -> XmlTypeCode {
    // If we have schema info, resolve the primitive base type
    if let (Some(schema_set), Some(type_key)) = (schema_set, schema_type) {
        let builtin_types = schema_set.builtin_types();

        // Check if the type itself is a built-in
        if let Some(code) = builtin_types.get_type_code(type_key) {
            return get_xsd_primitive_type(code);
        }

        // Walk base type chain to find a built-in type
        if let Some(type_def) = schema_set.arenas.simple_types.get(type_key) {
            let mut current_def = type_def;
            loop {
                if let Some(TypeKey::Simple(base_key)) = current_def.resolved_base_type {
                    if let Some(code) = builtin_types.get_type_code(base_key) {
                        return get_xsd_primitive_type(code);
                    }
                    if let Some(base_def) = schema_set.arenas.simple_types.get(base_key) {
                        current_def = base_def;
                        continue;
                    }
                }
                break;
            }
        }
    }

    // Fallback: use the type_code's primitive base type
    get_xsd_primitive_type(type_code)
}

/// Map an XmlTypeCode to its XSD primitive base type.
///
/// Per XPath 2.0 spec, these are the 19 primitive types plus special handling
/// for dayTimeDuration and yearMonthDuration.
fn get_xsd_primitive_type(code: XmlTypeCode) -> XmlTypeCode {
    match code {
        // Already primitive types - return as-is
        XmlTypeCode::String
        | XmlTypeCode::Boolean
        | XmlTypeCode::Decimal
        | XmlTypeCode::Float
        | XmlTypeCode::Double
        | XmlTypeCode::Duration
        | XmlTypeCode::DateTime
        | XmlTypeCode::Time
        | XmlTypeCode::Date
        | XmlTypeCode::GYearMonth
        | XmlTypeCode::GYear
        | XmlTypeCode::GMonthDay
        | XmlTypeCode::GDay
        | XmlTypeCode::GMonth
        | XmlTypeCode::HexBinary
        | XmlTypeCode::Base64Binary
        | XmlTypeCode::AnyUri
        | XmlTypeCode::QName
        | XmlTypeCode::Notation => code,

        // XPath 2.0 special cases: dayTimeDuration and yearMonthDuration
        // These are their own "primitive" types for comparison purposes
        XmlTypeCode::DayTimeDuration => XmlTypeCode::DayTimeDuration,
        XmlTypeCode::YearMonthDuration => XmlTypeCode::YearMonthDuration,

        // String-derived types → xs:string
        XmlTypeCode::NormalizedString
        | XmlTypeCode::Token
        | XmlTypeCode::Language
        | XmlTypeCode::NmToken
        | XmlTypeCode::Name
        | XmlTypeCode::NCName
        | XmlTypeCode::Id
        | XmlTypeCode::IdRef
        | XmlTypeCode::Entity => XmlTypeCode::String,

        // Integer hierarchy → xs:decimal (the primitive for all integers)
        XmlTypeCode::Integer
        | XmlTypeCode::NonPositiveInteger
        | XmlTypeCode::NegativeInteger
        | XmlTypeCode::Long
        | XmlTypeCode::Int
        | XmlTypeCode::Short
        | XmlTypeCode::Byte
        | XmlTypeCode::NonNegativeInteger
        | XmlTypeCode::UnsignedLong
        | XmlTypeCode::UnsignedInt
        | XmlTypeCode::UnsignedShort
        | XmlTypeCode::UnsignedByte
        | XmlTypeCode::PositiveInteger => XmlTypeCode::Decimal,

        // dateTimeStamp → xs:dateTime
        XmlTypeCode::DateTimeStamp => XmlTypeCode::DateTime,

        // List types - shouldn't reach here after atomization, but fallback to string
        XmlTypeCode::NmTokens | XmlTypeCode::IdRefs | XmlTypeCode::Entities => XmlTypeCode::String,

        // Other/unknown types → xs:string as fallback
        _ => XmlTypeCode::String,
    }
}

/// Cast an UntypedAtomic value to a primitive type.
///
/// Uses `cast_to` for types it supports, falls back to `VALIDATOR_REGISTRY`
/// for other types (date/time/duration/etc). Does NOT apply facets.
fn cast_to_primitive(value: &XmlValue, target_type: XmlTypeCode) -> Result<XmlValue, XPathError> {
    // First try cast_to which handles common types
    match cast_to(value, target_type) {
        Ok(result) => Ok(result),
        Err(_) => {
            // Fallback to VALIDATOR_REGISTRY for date/time/duration types
            let string_val = value.to_string_value();
            VALIDATOR_REGISTRY
                .validate(target_type, &string_val)
                .map_err(|e| XPathError::FORG0001 {
                    value: string_val,
                    target_type: format!("{:?}: {}", target_type, e),
                })
        }
    }
}

/// Cast an UntypedAtomic value to a primitive type with namespace context.
///
/// This handles QName and NOTATION specially by resolving namespace prefixes
/// using the in-scope namespace bindings from the XPath context.
fn cast_to_primitive_ctx(
    context: &XPathContext,
    value: &XmlValue,
    target_type: XmlTypeCode,
) -> Result<XmlValue, XPathError> {
    match target_type {
        XmlTypeCode::QName | XmlTypeCode::Notation => {
            // QName/NOTATION need namespace resolution
            cast_to_qname_with_context(context, value, target_type)
        }
        _ => cast_to_primitive(value, target_type),
    }
}

/// Cast an UntypedAtomic value to QName or NOTATION with namespace resolution.
///
/// Parses the lexical QName and resolves the prefix using the context's namespace bindings.
pub(crate) fn cast_to_qname_with_context(
    context: &XPathContext,
    value: &XmlValue,
    type_code: XmlTypeCode,
) -> Result<XmlValue, XPathError> {
    use crate::xpath::functions::qname::parse_lexical_qname;

    // A value of the target type is already a resolved name: re-reading its
    // lexical form would resolve its prefix against the static context
    // instead of the bindings it was made with.
    if value.type_code == type_code
        && matches!(
            value.value,
            XmlValueKind::Atomic(XmlAtomicValue::QName(_) | XmlAtomicValue::Notation(_))
        )
    {
        return Ok(value.clone());
    }

    let lexical = value.to_string_value();

    // Parse prefix:local or local (reuse existing validated parser)
    let (prefix, local_name) = parse_lexical_qname(&lexical)?;

    // Resolve namespace using context.namespaces
    let namespace_uri = if let Some(ref pfx) = prefix {
        let prefix_id = context
            .names
            .get(pfx)
            .ok_or_else(|| XPathError::undefined_prefix(pfx))?;
        let ns_id = context
            .namespaces
            .resolve_prefix(prefix_id)
            .ok_or_else(|| XPathError::undefined_prefix(pfx))?;
        Some(ns_id)
    } else {
        // Unprefixed QName in casting context - no namespace per XPath spec
        None
    };

    // Build QualifiedName
    let local_id = context.names.add(&local_name);
    let prefix_id = prefix.as_ref().map(|p| context.names.add(p));
    let qn = QNameValue::new(
        QualifiedName::new(namespace_uri, local_id, prefix_id),
        context.names,
    );
    let atom = if type_code == XmlTypeCode::Notation {
        XmlAtomicValue::Notation(qn)
    } else {
        XmlAtomicValue::QName(qn)
    };

    Ok(XmlValue::new(type_code, XmlValueKind::Atomic(atom)))
}

/// Perform magnitude relationship promotion with context-aware casting.
///
/// Per XPath 2.0 spec (general comparison rules):
/// - If both operands are numeric → both cast to xs:double
/// - If both operands are xs:untypedAtomic → both cast to xs:string
/// - If exactly one operand is xs:untypedAtomic:
///   - If other is numeric → cast untyped to xs:double
///   - If other is xs:dayTimeDuration → cast untyped to xs:dayTimeDuration
///   - If other is xs:yearMonthDuration → cast untyped to xs:yearMonthDuration
///   - Otherwise → cast untyped to the **primitive base type** of the other operand
///
/// Note: This does NOT apply facet validation - facets are for schema validation,
/// not XPath general comparisons.
pub fn magnitude_relationship_ctx(
    context: &XPathContext,
    left: &XmlValue,
    right: &XmlValue,
) -> Result<(XmlValue, XmlValue), XPathError> {
    let mut left_result = left.clone();
    let mut right_result = right.clone();

    if left_result.type_code == XmlTypeCode::UntypedAtomic {
        if right.type_code.is_numeric() {
            // Numeric → cast to xs:double
            let s = left_result.to_string_value();
            let d = crate::xpath::cast::parse_xsd_double(&s)
                .ok_or_else(|| XPathError::invalid_cast_value(&s, "xs:double"))?;
            left_result = XmlValue::double(d);
        } else if is_string_like(right.type_code) {
            // String-like → cast to xs:string
            left_result = XmlValue::string(left_result.to_string_value());
        } else if right.type_code != XmlTypeCode::UntypedAtomic {
            // Other typed value → cast to primitive base type
            let primitive_type =
                get_primitive_base_type(context.schema_set, right.schema_type, right.type_code);
            left_result = cast_to_primitive_ctx(context, &left_result, primitive_type)?;
        }
    }

    if right_result.type_code == XmlTypeCode::UntypedAtomic {
        if left_result.type_code.is_numeric() {
            // Numeric → cast to xs:double
            let s = right_result.to_string_value();
            let d = crate::xpath::cast::parse_xsd_double(&s)
                .ok_or_else(|| XPathError::invalid_cast_value(&s, "xs:double"))?;
            right_result = XmlValue::double(d);
        } else if is_string_like(left_result.type_code) {
            // String-like → cast to xs:string
            right_result = XmlValue::string(right_result.to_string_value());
        } else if left_result.type_code != XmlTypeCode::UntypedAtomic {
            // Other typed value → cast to primitive base type
            let primitive_type = get_primitive_base_type(
                context.schema_set,
                left_result.schema_type,
                left_result.type_code,
            );
            right_result = cast_to_primitive_ctx(context, &right_result, primitive_type)?;
        }
    }

    Ok((left_result, right_result))
}

/// The value one item of a general-comparison operand atomizes to, as it is
/// stored: an atomic value as it is, a node's typed value (`None` for a nilled
/// element). A list-typed node's typed value comes back packed; the callers
/// take it apart with [`atomize::unpack_list`] — [`AtomCursor`] for the
/// pairwise loops, `general_compare`'s atomizers for the hash index.
pub(super) fn atomize_item<N: DomNavigator>(
    item: XmlItemRef<'_, N>,
) -> Result<Option<XmlValue>, XPathError> {
    match item {
        XmlItemRef::Atomic(value) => Ok(Some(value.clone())),
        XmlItemRef::Node(node) => atomize::atomize_node(node),
    }
}

/// A cursor over the atomized values of a general-comparison operand, in
/// order (XPath 2.0 §3.5.2: "Atomization is applied to each operand").
///
/// A nilled element contributes no value and a list-typed node one value per
/// member, so the pairwise loops below walk the Cartesian product of the two
/// atomized operands in row-major order. Each item is atomized when the cursor
/// reaches it, never before, so an item that cannot be atomized raises its
/// error exactly where the loop meets it. A value that is not a list costs two
/// discriminant tests more than the item itself: no list in progress, and not
/// a packed list ([`atomize::is_packed_list`]).
struct AtomCursor<I: XmlNodeIterator> {
    items: I,
    /// The members of the list being walked, if the current item was one.
    pending: Option<ListMembers>,
}

impl<I: XmlNodeIterator> AtomCursor<I> {
    fn new(items: I) -> Self {
        Self {
            items,
            pending: None,
        }
    }

    /// The next atomized value, or `None` at the end of the operand.
    #[inline]
    fn next_atom(&mut self) -> Result<Option<XmlValue>, XPathError> {
        if let Some(members) = &mut self.pending {
            if let Some(member) = members.next() {
                return Ok(Some(member));
            }
            self.pending = None;
        }
        while self.items.move_next()? {
            let item = self
                .items
                .current()
                .ok_or_else(|| XPathError::internal("Iterator current missing"))?;
            let Some(value) = atomize_item(item)? else {
                continue; // nilled → no value
            };
            if !atomize::is_packed_list(&value) {
                return Ok(Some(value));
            }
            match atomize::unpack_list(value) {
                Ok(mut members) => {
                    if let Some(first) = members.next() {
                        self.pending = Some(members);
                        return Ok(Some(first));
                    }
                    // An empty list contributes no value.
                }
                Err(value) => return Ok(Some(value)),
            }
        }
        Ok(None)
    }
}

/// Compare two values for equality (value comparison).
///
/// This is the core equality comparison used by both value and general comparisons.
/// For general comparisons, use `magnitude_relationship` first to promote UntypedAtomic values.
///
/// Strings are compared under the Unicode codepoint collation; the collated
/// twins below take the collation the expression is actually evaluated with.
/// A date/time value without a timezone is read in the host's local offset at
/// the time of the call; [`eval_binary_with_timezone`] takes the implicit
/// timezone of a dynamic context instead.
pub fn value_eq(left: &XmlValue, right: &XmlValue) -> Result<bool, XPathError> {
    compare_eq(
        left,
        right,
        CollationRef::Codepoint,
        host_timezone_for(left, right),
    )
}

/// Compare two values for greater-than (value comparison); see [`value_eq`].
pub fn value_gt(left: &XmlValue, right: &XmlValue) -> Result<bool, XPathError> {
    compare_gt(
        left,
        right,
        CollationRef::Codepoint,
        host_timezone_for(left, right),
    )
}

/// Compare two values for greater-than-or-equal (value comparison); see
/// [`value_eq`].
pub fn value_ge(left: &XmlValue, right: &XmlValue) -> Result<bool, XPathError> {
    compare_ge(
        left,
        right,
        CollationRef::Codepoint,
        host_timezone_for(left, right),
    )
}

/// Compare two values for less-than (value comparison); see [`value_eq`].
pub fn value_lt(left: &XmlValue, right: &XmlValue) -> Result<bool, XPathError> {
    compare_lt(
        left,
        right,
        CollationRef::Codepoint,
        host_timezone_for(left, right),
    )
}

/// Compare two values for less-than-or-equal (value comparison); see
/// [`value_eq`].
pub fn value_le(left: &XmlValue, right: &XmlValue) -> Result<bool, XPathError> {
    compare_le(
        left,
        right,
        CollationRef::Codepoint,
        host_timezone_for(left, right),
    )
}

/// [`value_eq`] under `collation`, with the implicit timezone `tz`.
pub(crate) fn value_eq_collated(
    left: &XmlValue,
    right: &XmlValue,
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
) -> Result<bool, XPathError> {
    compare_eq(left, right, collation, tz)
}

/// [`value_gt`] under `collation`, with the implicit timezone `tz`.
pub(crate) fn value_gt_collated(
    left: &XmlValue,
    right: &XmlValue,
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
) -> Result<bool, XPathError> {
    compare_gt(left, right, collation, tz)
}

/// [`value_lt`] under `collation`, with the implicit timezone `tz`.
pub(crate) fn value_lt_collated(
    left: &XmlValue,
    right: &XmlValue,
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
) -> Result<bool, XPathError> {
    compare_lt(left, right, collation, tz)
}

// ============================================================================
// General comparisons for single atomized values
// ============================================================================

/// General equality comparison with magnitude relationship promotion (single values).
///
/// Promotes UntypedAtomic values before comparison.
/// For sequence comparisons, use `general_eq_seq`.
pub fn general_eq(left: &XmlValue, right: &XmlValue) -> Result<bool, XPathError> {
    let (l, r) = magnitude_relationship(left, right)?;
    compare_eq(&l, &r, CollationRef::Codepoint, host_timezone_for(&l, &r))
}

/// General greater-than comparison with magnitude relationship promotion (single values).
pub fn general_gt(left: &XmlValue, right: &XmlValue) -> Result<bool, XPathError> {
    let (l, r) = magnitude_relationship(left, right)?;
    compare_gt(&l, &r, CollationRef::Codepoint, host_timezone_for(&l, &r))
}

/// General not-equal comparison with magnitude relationship promotion (single values).
pub fn general_ne(left: &XmlValue, right: &XmlValue) -> Result<bool, XPathError> {
    general_eq(left, right).map(|eq| !eq)
}

/// General greater-than-or-equal comparison with magnitude relationship promotion (single values).
pub fn general_ge(left: &XmlValue, right: &XmlValue) -> Result<bool, XPathError> {
    let (l, r) = magnitude_relationship(left, right)?;
    compare_ge(&l, &r, CollationRef::Codepoint, host_timezone_for(&l, &r))
}

/// General less-than comparison with magnitude relationship promotion (single values).
pub fn general_lt(left: &XmlValue, right: &XmlValue) -> Result<bool, XPathError> {
    let (l, r) = magnitude_relationship(left, right)?;
    compare_lt(&l, &r, CollationRef::Codepoint, host_timezone_for(&l, &r))
}

/// General less-than-or-equal comparison with magnitude relationship promotion (single values).
pub fn general_le(left: &XmlValue, right: &XmlValue) -> Result<bool, XPathError> {
    let (l, r) = magnitude_relationship(left, right)?;
    compare_le(&l, &r, CollationRef::Codepoint, host_timezone_for(&l, &r))
}

// ============================================================================
// Iterator-based general comparisons (Cartesian product semantics)
// ============================================================================

pub fn general_eq_iter<I1, I2>(
    context: &XPathContext,
    left: &I1,
    right: &I2,
) -> Result<bool, XPathError>
where
    I1: XmlNodeIterator,
    I2: XmlNodeIterator,
{
    // The static context's default collation, resolved only if two strings are
    // actually compared, and then once for the whole comparison rather than
    // once per pair. Under the codepoint collation — the default, and the only
    // collation without a host resolver — this is an `Option::is_none()` and
    // the handle is `Codepoint`, which is the code path every line below took
    // before collations existed.
    let deferred = collation::DeferredDefault::new(context);
    general_eq_iter_collated(
        context,
        left,
        right,
        deferred.as_ref(),
        static_timezone(context),
    )
}

/// [`general_eq_iter`] under the collation `collation` — the evaluator's entry
/// point, which hands in a default collation resolved through the per-run memo.
pub(crate) fn general_eq_iter_collated<I1, I2>(
    context: &XPathContext,
    left: &I1,
    right: &I2,
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
) -> Result<bool, XPathError>
where
    I1: XmlNodeIterator,
    I2: XmlNodeIterator,
{
    let right_buf = BufferedNodeIterator::preload(right.clone())?;

    // Walk a bounded prefix of the Cartesian product first. Every comparison
    // whose product is no larger than the budget, and every comparison whose
    // first true pair falls inside it, is decided at exactly the cost it had
    // before — which is what the small comparisons in predicates need.
    if let Some(result) =
        general_eq_scan(context, left, &right_buf, INDEX_AFTER_PAIRS, collation, tz)?
    {
        return Ok(result);
    }

    // Only a genuinely large product gets here. A hash index answers the same
    // question in O(|A| + |B|) for the operand shapes where that is provably
    // the same answer, errors included; it declines everything else.
    if let Some(result) = general_compare::try_indexed_eq(context, left, &right_buf, collation, tz)
    {
        return result;
    }

    Ok(general_eq_scan(context, left, &right_buf, usize::MAX, collation, tz)?.unwrap_or(false))
}

/// How many pairs of the Cartesian product are compared before an index is
/// considered.
///
/// The budget exists so that the indexed path can never cost more than the
/// pairwise one on a comparison the pairwise one would have finished quickly:
/// `(a, b, c) = (x, y)` is answered without ever atomizing an operand twice,
/// and so is a large comparison whose very first pairs already match.
const INDEX_AFTER_PAIRS: usize = 64;

/// The Cartesian-product evaluation of `A = B`, and the definition of what the
/// indexed path in [`general_compare`] must reproduce exactly. (The production
/// paths run the same [`general_eq_scan`] with an unbounded budget; this is the
/// tests' reference.)
#[cfg(test)]
pub(super) fn general_eq_iter_pairwise<I1, I2>(
    context: &XPathContext,
    left: &I1,
    right_buf: &BufferedNodeIterator<I2>,
) -> Result<bool, XPathError>
where
    I1: XmlNodeIterator,
    I2: XmlNodeIterator,
{
    let deferred = collation::DeferredDefault::new(context);
    Ok(general_eq_scan(
        context,
        left,
        right_buf,
        usize::MAX,
        deferred.as_ref(),
        static_timezone(context),
    )?
    .unwrap_or(false))
}

/// Walk at most `budget` pairs of the product in row-major order.
///
/// `Ok(None)` means the budget ran out before the product did, and nothing has
/// been decided: no pair compared true and none raised a hard error, so the
/// caller is free to answer the comparison any way it likes. With a budget of
/// `usize::MAX` the result is never `None` and this is the original loop.
fn general_eq_scan<I1, I2>(
    context: &XPathContext,
    left: &I1,
    right_buf: &BufferedNodeIterator<I2>,
    budget: usize,
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
) -> Result<Option<bool>, XPathError>
where
    I1: XmlNodeIterator,
    I2: XmlNodeIterator,
{
    let mut left_atoms = AtomCursor::new(left.clone());
    let mut remaining = budget;
    // XPath 2.0 §3.5.2 lets a general comparison return true as soon as it finds
    // a pair with the required magnitude relationship, so an incomparable pair
    // found on the way there may be ignored. If no pair compares true, the
    // comparison of the incomparable pair is what decides the result, and that
    // comparison is a type error (§3.5.1).
    let mut deferred: Option<XPathError> = None;

    while let Some(left_value) = left_atoms.next_atom()? {
        let mut right_atoms = AtomCursor::new(right_buf.clone());

        while let Some(right_value) = right_atoms.next_atom()? {
            if remaining == 0 {
                return Ok(None);
            }
            remaining -= 1;
            let (l, r) = magnitude_relationship_ctx(context, &left_value, &right_value)?;

            match value_eq_collated(&l, &r, collation, tz) {
                Ok(true) => return Ok(Some(true)),
                Ok(false) => continue,
                Err(err) if is_operator_not_defined(&err) => {
                    deferred.get_or_insert(err);
                    continue;
                }
                Err(err) => return Err(err),
            }
        }
    }

    match deferred {
        Some(err) => Err(err),
        None => Ok(Some(false)),
    }
}

pub fn general_ne_iter<I1, I2>(
    context: &XPathContext,
    left: &I1,
    right: &I2,
) -> Result<bool, XPathError>
where
    I1: XmlNodeIterator,
    I2: XmlNodeIterator,
{
    let deferred = collation::DeferredDefault::new(context);
    general_ne_iter_collated(
        context,
        left,
        right,
        deferred.as_ref(),
        static_timezone(context),
    )
}

/// [`general_ne_iter`] under the collation `collation`.
pub(crate) fn general_ne_iter_collated<I1, I2>(
    context: &XPathContext,
    left: &I1,
    right: &I2,
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
) -> Result<bool, XPathError>
where
    I1: XmlNodeIterator,
    I2: XmlNodeIterator,
{
    let right_buf = BufferedNodeIterator::preload(right.clone())?;

    // `!=` already stops at the first unequal pair, so the bounded prefix
    // decides it in practice; the index is only there for the degenerate
    // all-equal product, and it declines everything else.
    if let Some(result) =
        general_ne_scan(context, left, &right_buf, INDEX_AFTER_PAIRS, collation, tz)?
    {
        return Ok(result);
    }

    if let Some(result) = general_compare::try_indexed_ne(left, &right_buf, collation, tz) {
        return result;
    }

    Ok(general_ne_scan(context, left, &right_buf, usize::MAX, collation, tz)?.unwrap_or(false))
}

/// The Cartesian-product evaluation of `A != B` (the tests' reference, like
/// [`general_eq_iter_pairwise`]).
#[cfg(test)]
pub(super) fn general_ne_iter_pairwise<I1, I2>(
    context: &XPathContext,
    left: &I1,
    right_buf: &BufferedNodeIterator<I2>,
) -> Result<bool, XPathError>
where
    I1: XmlNodeIterator,
    I2: XmlNodeIterator,
{
    let deferred = collation::DeferredDefault::new(context);
    Ok(general_ne_scan(
        context,
        left,
        right_buf,
        usize::MAX,
        deferred.as_ref(),
        static_timezone(context),
    )?
    .unwrap_or(false))
}

/// The `!=` counterpart of [`general_eq_scan`]; see it for what `Ok(None)` means.
fn general_ne_scan<I1, I2>(
    context: &XPathContext,
    left: &I1,
    right_buf: &BufferedNodeIterator<I2>,
    budget: usize,
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
) -> Result<Option<bool>, XPathError>
where
    I1: XmlNodeIterator,
    I2: XmlNodeIterator,
{
    let mut left_atoms = AtomCursor::new(left.clone());
    let mut remaining = budget;
    // An incomparable pair is *not* an unequal pair: §3.5.2 defers to the `ne`
    // value comparison, and §3.5.1 makes an incomparable `ne` a type error. The
    // error is held back in case a genuinely unequal pair turns up, which §3.5.2
    // allows to decide the comparison on its own.
    let mut deferred: Option<XPathError> = None;

    while let Some(left_value) = left_atoms.next_atom()? {
        let mut right_atoms = AtomCursor::new(right_buf.clone());

        while let Some(right_value) = right_atoms.next_atom()? {
            if remaining == 0 {
                return Ok(None);
            }
            remaining -= 1;
            let (l, r) = magnitude_relationship_ctx(context, &left_value, &right_value)?;

            match value_eq_collated(&l, &r, collation, tz) {
                Ok(true) => continue,
                Ok(false) => return Ok(Some(true)),
                Err(err) if is_operator_not_defined(&err) => {
                    deferred.get_or_insert(err);
                    continue;
                }
                Err(err) => return Err(err),
            }
        }
    }

    match deferred {
        Some(err) => Err(err),
        None => Ok(Some(false)),
    }
}

pub fn general_lt_iter<I1, I2>(
    context: &XPathContext,
    left: &I1,
    right: &I2,
) -> Result<bool, XPathError>
where
    I1: XmlNodeIterator,
    I2: XmlNodeIterator,
{
    let deferred = collation::DeferredDefault::new(context);
    general_lt_iter_collated(
        context,
        left,
        right,
        deferred.as_ref(),
        static_timezone(context),
    )
}

/// [`general_lt_iter`] under the collation `collation`.
pub(crate) fn general_lt_iter_collated<I1, I2>(
    context: &XPathContext,
    left: &I1,
    right: &I2,
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
) -> Result<bool, XPathError>
where
    I1: XmlNodeIterator,
    I2: XmlNodeIterator,
{
    let right_buf = BufferedNodeIterator::preload(right.clone())?;
    let mut left_atoms = AtomCursor::new(left.clone());

    while let Some(left_value) = left_atoms.next_atom()? {
        let mut right_atoms = AtomCursor::new(right_buf.clone());

        while let Some(right_value) = right_atoms.next_atom()? {
            let (l, r) = magnitude_relationship_ctx(context, &left_value, &right_value)?;

            match value_lt_collated(&l, &r, collation, tz) {
                Ok(true) => return Ok(true),
                Ok(false) => continue,
                Err(err) => return Err(err),
            }
        }
    }

    Ok(false)
}

pub fn general_le_iter<I1, I2>(
    context: &XPathContext,
    left: &I1,
    right: &I2,
) -> Result<bool, XPathError>
where
    I1: XmlNodeIterator,
    I2: XmlNodeIterator,
{
    let deferred = collation::DeferredDefault::new(context);
    general_le_iter_collated(
        context,
        left,
        right,
        deferred.as_ref(),
        static_timezone(context),
    )
}

/// [`general_le_iter`] under the collation `collation`.
pub(crate) fn general_le_iter_collated<I1, I2>(
    context: &XPathContext,
    left: &I1,
    right: &I2,
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
) -> Result<bool, XPathError>
where
    I1: XmlNodeIterator,
    I2: XmlNodeIterator,
{
    let right_buf = BufferedNodeIterator::preload(right.clone())?;
    let mut left_atoms = AtomCursor::new(left.clone());

    while let Some(left_value) = left_atoms.next_atom()? {
        let mut right_atoms = AtomCursor::new(right_buf.clone());

        while let Some(right_value) = right_atoms.next_atom()? {
            let (l, r) = magnitude_relationship_ctx(context, &left_value, &right_value)?;

            match value_eq_collated(&l, &r, collation, tz) {
                Ok(true) => return Ok(true),
                Ok(false) => {}
                Err(err) if is_operator_not_defined(&err) => {}
                Err(err) => return Err(err),
            }

            match value_lt_collated(&l, &r, collation, tz) {
                Ok(true) => return Ok(true),
                Ok(false) => continue,
                Err(err) => return Err(err),
            }
        }
    }

    Ok(false)
}

pub fn general_gt_iter<I1, I2>(
    context: &XPathContext,
    left: &I1,
    right: &I2,
) -> Result<bool, XPathError>
where
    I1: XmlNodeIterator,
    I2: XmlNodeIterator,
{
    let deferred = collation::DeferredDefault::new(context);
    general_gt_iter_collated(
        context,
        left,
        right,
        deferred.as_ref(),
        static_timezone(context),
    )
}

/// [`general_gt_iter`] under the collation `collation`.
pub(crate) fn general_gt_iter_collated<I1, I2>(
    context: &XPathContext,
    left: &I1,
    right: &I2,
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
) -> Result<bool, XPathError>
where
    I1: XmlNodeIterator,
    I2: XmlNodeIterator,
{
    let right_buf = BufferedNodeIterator::preload(right.clone())?;
    let mut left_atoms = AtomCursor::new(left.clone());

    while let Some(left_value) = left_atoms.next_atom()? {
        let mut right_atoms = AtomCursor::new(right_buf.clone());

        while let Some(right_value) = right_atoms.next_atom()? {
            let (l, r) = magnitude_relationship_ctx(context, &left_value, &right_value)?;

            match value_gt_collated(&l, &r, collation, tz) {
                Ok(true) => return Ok(true),
                Ok(false) => continue,
                Err(err) => return Err(err),
            }
        }
    }

    Ok(false)
}

pub fn general_ge_iter<I1, I2>(
    context: &XPathContext,
    left: &I1,
    right: &I2,
) -> Result<bool, XPathError>
where
    I1: XmlNodeIterator,
    I2: XmlNodeIterator,
{
    let deferred = collation::DeferredDefault::new(context);
    general_ge_iter_collated(
        context,
        left,
        right,
        deferred.as_ref(),
        static_timezone(context),
    )
}

/// [`general_ge_iter`] under the collation `collation`.
pub(crate) fn general_ge_iter_collated<I1, I2>(
    context: &XPathContext,
    left: &I1,
    right: &I2,
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
) -> Result<bool, XPathError>
where
    I1: XmlNodeIterator,
    I2: XmlNodeIterator,
{
    let right_buf = BufferedNodeIterator::preload(right.clone())?;
    let mut left_atoms = AtomCursor::new(left.clone());

    while let Some(left_value) = left_atoms.next_atom()? {
        let mut right_atoms = AtomCursor::new(right_buf.clone());

        while let Some(right_value) = right_atoms.next_atom()? {
            let (l, r) = magnitude_relationship_ctx(context, &left_value, &right_value)?;

            match value_eq_collated(&l, &r, collation, tz) {
                Ok(true) => return Ok(true),
                Ok(false) => {}
                Err(err) if is_operator_not_defined(&err) => {}
                Err(err) => return Err(err),
            }

            match value_gt_collated(&l, &r, collation, tz) {
                Ok(true) => return Ok(true),
                Ok(false) => continue,
                Err(err) => return Err(err),
            }
        }
    }

    Ok(false)
}

// ============================================================================
// XPath 1.0 comparison helpers
// ============================================================================

/// XPath 1.0 type coercion for comparisons (spec §3.4).
///
/// For `=`/`!=`:
///   1. If either operand is boolean → convert both to boolean
///   2. If either operand is numeric → convert both to number
///   3. Otherwise → compare as strings
///
/// For `<`/`<=`/`>`/`>=`:
///   Always convert both to number
fn coerce_for_comparison_10(
    op: BinaryOpKind,
    left: &XmlValue,
    right: &XmlValue,
) -> (XmlValue, XmlValue) {
    let is_equality = matches!(op, BinaryOpKind::GeneralEq | BinaryOpKind::GeneralNe);

    if is_equality {
        // Rule 1: if either is boolean, convert both to boolean
        if left.type_code == XmlTypeCode::Boolean || right.type_code == XmlTypeCode::Boolean {
            let l_bool = ebv_atomic(left);
            let r_bool = ebv_atomic(right);
            return (XmlValue::boolean(l_bool), XmlValue::boolean(r_bool));
        }

        // Rule 2: if either is numeric, convert both to number
        if left.type_code.is_numeric() || right.type_code.is_numeric() {
            let l_num = crate::xpath::atomize::to_number_xpath10(left);
            let r_num = crate::xpath::atomize::to_number_xpath10(right);
            return (XmlValue::double(l_num), XmlValue::double(r_num));
        }

        // Rule 3: otherwise string comparison
        let l_str = left.to_string_value();
        let r_str = right.to_string_value();
        return (XmlValue::string(l_str), XmlValue::string(r_str));
    }

    // Relational operators: always numeric
    let l_num = crate::xpath::atomize::to_number_xpath10(left);
    let r_num = crate::xpath::atomize::to_number_xpath10(right);
    (XmlValue::double(l_num), XmlValue::double(r_num))
}

/// Effective boolean value of a single atomic value (for XPath 1.0 coercion).
fn ebv_atomic(value: &XmlValue) -> bool {
    if let Some(b) = value.as_boolean() {
        return b;
    }
    if let Some(s) = value.as_string() {
        return !s.is_empty();
    }
    if let Some(d) = value.as_double() {
        return !d.is_nan() && d != 0.0;
    }
    if value.type_code.is_numeric() {
        let d = crate::xpath::atomize::to_number(value);
        return !d.is_nan() && d != 0.0;
    }
    let s = value.to_string_value();
    !s.is_empty()
}

/// A general comparison in XPath 1.0 compatibility mode, after the boolean
/// rule and atomization (XPath 2.0 §3.5.2): the existential comparison of the
/// pairs by [`compare_pair_compat`]. A pair of types `eq` is not defined for
/// is a type error, raised — as without compatibility mode — only when no
/// pair has the required magnitude relationship.
fn general_compare_compat<I1, I2>(
    op: BinaryOpKind,
    context: &XPathContext,
    left: &I1,
    right_buf: &BufferedNodeIterator<I2>,
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
) -> Result<bool, XPathError>
where
    I1: XmlNodeIterator,
    I2: XmlNodeIterator,
{
    let mut left_atoms = AtomCursor::new(left.clone());
    let mut deferred: Option<XPathError> = None;
    while let Some(left_value) = left_atoms.next_atom()? {
        let mut right_atoms = AtomCursor::new(right_buf.clone());
        while let Some(right_value) = right_atoms.next_atom()? {
            match compare_pair_compat(op, context, &left_value, &right_value, collation, tz) {
                Ok(true) => return Ok(true),
                Ok(false) => {}
                Err(err) if is_operator_not_defined(&err) => {
                    deferred.get_or_insert(err);
                }
                Err(err) => return Err(err),
            }
        }
    }
    match deferred {
        Some(err) => Err(err),
        None => Ok(false),
    }
}

/// The magnitude relationship of two atomic values in XPath 1.0
/// compatibility mode (XPath 2.0 §3.5.2): "If the comparison operator is <,
/// <=, >, or >=, then each item in both of the operand sequences is converted
/// to the type xs:double by applying the fn:number function"; otherwise "If at
/// least one of the two atomic values is an instance of a numeric type, then
/// both atomic values are converted to the type xs:double by applying the
/// fn:number function", and an `xs:untypedAtomic` value is cast as without
/// compatibility mode (two of them to `xs:string`, one to the other value's
/// type). "After performing the conversions described above, the atomic
/// values are compared using one of the value comparison operators eq, ne,
/// lt, le, gt, or ge".
fn compare_pair_compat(
    op: BinaryOpKind,
    context: &XPathContext,
    left: &XmlValue,
    right: &XmlValue,
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
) -> Result<bool, XPathError> {
    let relational = matches!(
        op,
        BinaryOpKind::GeneralLt
            | BinaryOpKind::GeneralLe
            | BinaryOpKind::GeneralGt
            | BinaryOpKind::GeneralGe
    );
    let (l, r) = if relational || left.type_code.is_numeric() || right.type_code.is_numeric() {
        (
            XmlValue::double(crate::xpath::atomize::to_number(left)),
            XmlValue::double(crate::xpath::atomize::to_number(right)),
        )
    } else {
        magnitude_relationship_ctx(context, left, right)?
    };
    match op {
        BinaryOpKind::GeneralEq => compare_eq(&l, &r, collation, tz),
        BinaryOpKind::GeneralNe => compare_eq(&l, &r, collation, tz).map(|eq| !eq),
        BinaryOpKind::GeneralLt => compare_lt(&l, &r, collation, tz),
        BinaryOpKind::GeneralLe => compare_le(&l, &r, collation, tz),
        BinaryOpKind::GeneralGt => compare_gt(&l, &r, collation, tz),
        BinaryOpKind::GeneralGe => compare_ge(&l, &r, collation, tz),
        _ => Err(XPathError::internal("not a general comparison operator")),
    }
}

/// XPath 1.0 general equality comparison (iterator-based, Cartesian product).
pub fn general_eq_iter_10<I1, I2>(left: &I1, right: &I2) -> Result<bool, XPathError>
where
    I1: XmlNodeIterator,
    I2: XmlNodeIterator,
{
    general_compare_iter_10(
        BinaryOpKind::GeneralEq,
        left,
        right,
        CollationRef::Codepoint,
        host_implicit_timezone(),
        None,
    )
}

/// XPath 1.0 general not-equal comparison (iterator-based, Cartesian product).
pub fn general_ne_iter_10<I1, I2>(left: &I1, right: &I2) -> Result<bool, XPathError>
where
    I1: XmlNodeIterator,
    I2: XmlNodeIterator,
{
    general_compare_iter_10(
        BinaryOpKind::GeneralNe,
        left,
        right,
        CollationRef::Codepoint,
        host_implicit_timezone(),
        None,
    )
}

/// XPath 1.0 general less-than comparison (iterator-based, Cartesian product).
pub fn general_lt_iter_10<I1, I2>(left: &I1, right: &I2) -> Result<bool, XPathError>
where
    I1: XmlNodeIterator,
    I2: XmlNodeIterator,
{
    general_compare_iter_10(
        BinaryOpKind::GeneralLt,
        left,
        right,
        CollationRef::Codepoint,
        host_implicit_timezone(),
        None,
    )
}

/// XPath 1.0 general less-than-or-equal comparison (iterator-based, Cartesian product).
pub fn general_le_iter_10<I1, I2>(left: &I1, right: &I2) -> Result<bool, XPathError>
where
    I1: XmlNodeIterator,
    I2: XmlNodeIterator,
{
    general_compare_iter_10(
        BinaryOpKind::GeneralLe,
        left,
        right,
        CollationRef::Codepoint,
        host_implicit_timezone(),
        None,
    )
}

/// XPath 1.0 general greater-than comparison (iterator-based, Cartesian product).
pub fn general_gt_iter_10<I1, I2>(left: &I1, right: &I2) -> Result<bool, XPathError>
where
    I1: XmlNodeIterator,
    I2: XmlNodeIterator,
{
    general_compare_iter_10(
        BinaryOpKind::GeneralGt,
        left,
        right,
        CollationRef::Codepoint,
        host_implicit_timezone(),
        None,
    )
}

/// XPath 1.0 general greater-than-or-equal comparison (iterator-based, Cartesian product).
pub fn general_ge_iter_10<I1, I2>(left: &I1, right: &I2) -> Result<bool, XPathError>
where
    I1: XmlNodeIterator,
    I2: XmlNodeIterator,
{
    general_compare_iter_10(
        BinaryOpKind::GeneralGe,
        left,
        right,
        CollationRef::Codepoint,
        host_implicit_timezone(),
        None,
    )
}

/// The XPath 1.0 compatibility-mode general comparison under `collation`.
///
/// XPath 2.0 §3.5.2 evaluates a general comparison in compatibility mode by
/// converting the operands and then: "After performing the conversions
/// described above, the atomic values are compared using one of the value
/// comparison operators eq, ne, lt, le, gt, or ge, depending on whether the
/// general comparison operator was =, !=, <, <=, >, or >=." A value comparison
/// of two strings uses the default collation (§3.5.1; §B.2 maps it to
/// `fn:compare`), so the engine passes the static context's default collation
/// here — the same collation `eq` uses — and the public `general_*_iter_10`
/// functions, which have no static context, pass the codepoint collation.
///
/// Only `=` and `!=` can reach a string comparison: for `<`, `<=`, `>` and `>=`
/// every operand is converted with `fn:number` first (§3.5.2 rule 3). An
/// unsupported collation is therefore FOCH0002 exactly where two strings are
/// compared, and nowhere else.
///
/// `compat` is the static context of an XPath 2.0 expression evaluated in XPath
/// 1.0 compatibility mode, for which the pairs are compared by §3.5.2 itself
/// ([`compare_pair_compat`]); `None` is the XPath 1.0 language
/// (`XPathMode::XPath10`, and the public `general_*_iter_10` functions), whose
/// §3.4 rules [`coerce_for_comparison_10`] applies.
pub(crate) fn general_compare_iter_10<I1, I2>(
    op: BinaryOpKind,
    left: &I1,
    right: &I2,
    collation: CollationRef<'_>,
    tz: TimezoneOffset,
    compat: Option<&XPathContext>,
) -> Result<bool, XPathError>
where
    I1: XmlNodeIterator,
    I2: XmlNodeIterator,
{
    let right_buf = BufferedNodeIterator::preload(right.clone())?;
    if let Some(context) = compat {
        return general_compare_compat(op, context, left, &right_buf, collation, tz);
    }
    let mut left_atoms = AtomCursor::new(left.clone());

    while let Some(left_value) = left_atoms.next_atom()? {
        let mut right_atoms = AtomCursor::new(right_buf.clone());

        while let Some(right_value) = right_atoms.next_atom()? {
            let (l, r) = coerce_for_comparison_10(op, &left_value, &right_value);

            let compared = match op {
                BinaryOpKind::GeneralEq => compare_eq(&l, &r, collation, tz),
                BinaryOpKind::GeneralNe => compare_eq(&l, &r, collation, tz).map(|eq| !eq),
                BinaryOpKind::GeneralLt => compare_lt(&l, &r, collation, tz),
                BinaryOpKind::GeneralLe => compare_le(&l, &r, collation, tz),
                BinaryOpKind::GeneralGt => compare_gt(&l, &r, collation, tz),
                BinaryOpKind::GeneralGe => compare_ge(&l, &r, collation, tz),
                _ => unreachable!(),
            };
            let satisfied = match compared {
                Ok(satisfied) => satisfied,
                // A collation nobody supplies is FOCH0002 when two strings
                // are compared (F&O §7.3.1); it must not read as "no match".
                Err(err @ XPathError::FOCH0002 { .. }) => return Err(err),
                // After `coerce_for_comparison_10` both operands are booleans,
                // doubles or strings, so no other comparison error can arise;
                // this is the pair-does-not-match reading the compatibility
                // path has had since it was written.
                Err(_) => false,
            };

            if satisfied {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

/// Evaluate a numeric binary operation in XPath 1.0 mode.
///
/// In XPath 1.0, arithmetic always returns double (never integer or decimal).
pub fn eval_numeric_binary_10(
    op: BinaryOpKind,
    left: &XmlValue,
    right: &XmlValue,
) -> Result<XmlValue, XPathError> {
    let l = crate::xpath::atomize::to_number(left);
    let r = crate::xpath::atomize::to_number(right);

    let result = match op {
        BinaryOpKind::Add => l + r,
        BinaryOpKind::Sub => l - r,
        BinaryOpKind::Mul => l * r,
        BinaryOpKind::Div => {
            if r == 0.0 {
                if l == 0.0 || l.is_nan() {
                    f64::NAN
                } else if l > 0.0 {
                    f64::INFINITY
                } else {
                    f64::NEG_INFINITY
                }
            } else {
                l / r
            }
        }
        BinaryOpKind::Mod => {
            if r == 0.0 {
                f64::NAN
            } else {
                l % r
            }
        }
        _ => return Err(XPathError::internal("Unsupported arithmetic operator")),
    };

    Ok(XmlValue::double(result))
}

// ============================================================================
// General comparisons for sequences (Cartesian product semantics)
// ============================================================================

/// General equality comparison for sequences (Cartesian product).
///
/// Returns true if ANY pair (left_item, right_item) from the Cartesian product
/// of the two sequences satisfies the equality condition.
///
/// # XPath 2.0 Semantics
///
/// The general comparison operators (`=`, `!=`, `<`, `<=`, `>`, `>=`) are
/// existentially quantified over their operand sequences:
/// - `A = B` is true if there exist atomized values `a` in `A` and `b` in `B`
///   such that `a eq b` is true (after type promotion).
///
/// # Arguments
///
/// * `left` - Left sequence of atomized values
/// * `right` - Right sequence of atomized values
///
/// # Returns
///
/// `true` if any pair satisfies equality, `false` if no pairs satisfy or
/// either sequence is empty.
pub fn general_eq_seq(left: &[XmlValue], right: &[XmlValue]) -> Result<bool, XPathError> {
    // Empty sequences: result is false (no pairs exist)
    if left.is_empty() || right.is_empty() {
        return Ok(false);
    }

    // Cartesian product: check if ANY pair satisfies the condition
    for l in left {
        for r in right {
            match general_eq(l, r) {
                Ok(true) => return Ok(true),
                Ok(false) => continue,
                Err(_) => continue, // Type errors mean this pair doesn't match
            }
        }
    }

    Ok(false)
}

/// General not-equal comparison for sequences (Cartesian product).
///
/// Returns true if ANY pair (left_item, right_item) satisfies inequality.
pub fn general_ne_seq(left: &[XmlValue], right: &[XmlValue]) -> Result<bool, XPathError> {
    if left.is_empty() || right.is_empty() {
        return Ok(false);
    }

    for l in left {
        for r in right {
            match general_ne(l, r) {
                Ok(true) => return Ok(true),
                Ok(false) => continue,
                Err(_) => continue,
            }
        }
    }

    Ok(false)
}

/// General less-than comparison for sequences (Cartesian product).
///
/// Returns true if ANY pair (left_item, right_item) satisfies left < right.
pub fn general_lt_seq(left: &[XmlValue], right: &[XmlValue]) -> Result<bool, XPathError> {
    if left.is_empty() || right.is_empty() {
        return Ok(false);
    }

    for l in left {
        for r in right {
            match general_lt(l, r) {
                Ok(true) => return Ok(true),
                Ok(false) => continue,
                Err(_) => continue,
            }
        }
    }

    Ok(false)
}

/// General less-than-or-equal comparison for sequences (Cartesian product).
///
/// Returns true if ANY pair (left_item, right_item) satisfies left <= right.
pub fn general_le_seq(left: &[XmlValue], right: &[XmlValue]) -> Result<bool, XPathError> {
    if left.is_empty() || right.is_empty() {
        return Ok(false);
    }

    for l in left {
        for r in right {
            match general_le(l, r) {
                Ok(true) => return Ok(true),
                Ok(false) => continue,
                Err(_) => continue,
            }
        }
    }

    Ok(false)
}

/// General greater-than comparison for sequences (Cartesian product).
///
/// Returns true if ANY pair (left_item, right_item) satisfies left > right.
pub fn general_gt_seq(left: &[XmlValue], right: &[XmlValue]) -> Result<bool, XPathError> {
    if left.is_empty() || right.is_empty() {
        return Ok(false);
    }

    for l in left {
        for r in right {
            match general_gt(l, r) {
                Ok(true) => return Ok(true),
                Ok(false) => continue,
                Err(_) => continue,
            }
        }
    }

    Ok(false)
}

/// General greater-than-or-equal comparison for sequences (Cartesian product).
///
/// Returns true if ANY pair (left_item, right_item) satisfies left >= right.
pub fn general_ge_seq(left: &[XmlValue], right: &[XmlValue]) -> Result<bool, XPathError> {
    if left.is_empty() || right.is_empty() {
        return Ok(false);
    }

    for l in left {
        for r in right {
            match general_ge(l, r) {
                Ok(true) => return Ok(true),
                Ok(false) => continue,
                Err(_) => continue,
            }
        }
    }

    Ok(false)
}

/// The XPath 2.0 §3.4 type error for an operand combination that
/// B.2 Operator Mapping does not cover.
///
/// It uses the dedicated `BinaryOperatorNotDefined` variant, whose error code is
/// `XPTY0004`. The variant is also the "not comparable" signal that
/// [`is_operator_not_defined`] tests, but that signal is only ever consulted on
/// the result of a *comparison* (`compare_ge`/`compare_le` and the general
/// comparisons), never on the result of an arithmetic operator, so the two uses
/// do not interfere.
fn unsupported_operator(op: BinaryOpKind, left: &XmlValue, right: &XmlValue) -> XPathError {
    XPathError::binary_operator_not_defined(
        format!("{op:?}"),
        type_code_to_name(left.type_code),
        type_code_to_name(right.type_code),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespace::qname::QualifiedName;
    use crate::namespace::NameTable;
    use crate::navigator::RoXmlNavigator;
    use crate::types::value::DurationValue;
    use crate::xpath::context::XPathContext;
    use crate::xpath::iterator::{VecNodeIterator, XmlItem};

    fn int_value(type_code: XmlTypeCode, value: i64) -> XmlValue {
        XmlValue {
            type_code,
            schema_type: None,
            value: XmlValueKind::Atomic(XmlAtomicValue::Integer(BigInt::from(value))),
        }
    }

    fn decimal_value(value: &str) -> XmlValue {
        XmlValue {
            type_code: XmlTypeCode::Decimal,
            schema_type: None,
            value: XmlValueKind::Atomic(XmlAtomicValue::Decimal(value.parse::<Decimal>().unwrap())),
        }
    }

    fn datetime_value(
        type_code: XmlTypeCode,
        year: i32,
        month: u8,
        day: u8,
        hour: u8,
        minute: u8,
        second: Decimal,
    ) -> XmlValue {
        XmlValue {
            type_code,
            schema_type: None,
            value: XmlValueKind::Atomic(XmlAtomicValue::DateTime(DateTimeValue {
                year,
                month,
                day,
                hour,
                minute,
                second,
                timezone: None,
            })),
        }
    }

    fn date_value(year: i32, month: u8, day: u8) -> XmlValue {
        XmlValue {
            type_code: XmlTypeCode::Date,
            schema_type: None,
            value: XmlValueKind::Atomic(XmlAtomicValue::Date(DateValue {
                year,
                month,
                day,
                timezone: None,
            })),
        }
    }

    fn time_value(hour: u8, minute: u8, second: Decimal) -> XmlValue {
        XmlValue {
            type_code: XmlTypeCode::Time,
            schema_type: None,
            value: XmlValueKind::Atomic(XmlAtomicValue::Time(TimeValue {
                hour,
                minute,
                second,
                timezone: None,
            })),
        }
    }

    fn time_value_with_tz(
        hour: u8,
        minute: u8,
        second: Decimal,
        timezone: TimezoneOffset,
    ) -> XmlValue {
        XmlValue {
            type_code: XmlTypeCode::Time,
            schema_type: None,
            value: XmlValueKind::Atomic(XmlAtomicValue::Time(TimeValue {
                hour,
                minute,
                second,
                timezone: Some(timezone),
            })),
        }
    }

    fn year_month_duration_value(years: u32, months: u32) -> XmlValue {
        XmlValue {
            type_code: XmlTypeCode::YearMonthDuration,
            schema_type: None,
            value: XmlValueKind::Atomic(XmlAtomicValue::YearMonthDuration(
                YearMonthDurationValue {
                    negative: false,
                    years,
                    months,
                },
            )),
        }
    }

    fn day_time_duration_value(days: u32, hours: u32, minutes: u32, seconds: Decimal) -> XmlValue {
        XmlValue {
            type_code: XmlTypeCode::DayTimeDuration,
            schema_type: None,
            value: XmlValueKind::Atomic(XmlAtomicValue::DayTimeDuration(DayTimeDurationValue {
                negative: false,
                days,
                hours,
                minutes,
                seconds,
            })),
        }
    }

    fn duration_value(
        negative: bool,
        years: u32,
        months: u32,
        days: u32,
        hours: u32,
        minutes: u32,
        seconds: Decimal,
    ) -> XmlValue {
        XmlValue {
            type_code: XmlTypeCode::Duration,
            schema_type: None,
            value: XmlValueKind::Atomic(XmlAtomicValue::Duration(DurationValue {
                negative,
                years,
                months,
                days,
                hours,
                minutes,
                seconds,
            })),
        }
    }

    #[test]
    fn test_add_byte_unsigned_byte_returns_int() {
        let left = int_value(XmlTypeCode::Byte, 1);
        let right = int_value(XmlTypeCode::UnsignedByte, 2);
        let result = eval_binary(BinaryOpKind::Add, &left, &right).unwrap();
        assert_eq!(result.type_code, XmlTypeCode::Int);
        assert_eq!(result.as_integer().unwrap(), &BigInt::from(3));
    }

    #[test]
    fn test_add_int_unsigned_int_returns_unsigned_int() {
        let left = int_value(XmlTypeCode::Int, 3);
        let right = int_value(XmlTypeCode::UnsignedInt, 4);
        let result = eval_binary(BinaryOpKind::Add, &left, &right).unwrap();
        assert_eq!(result.type_code, XmlTypeCode::UnsignedInt);
        assert_eq!(result.as_integer().unwrap(), &BigInt::from(7));
    }

    #[test]
    fn test_div_int_int_returns_decimal() {
        let left = int_value(XmlTypeCode::Int, 3);
        let right = int_value(XmlTypeCode::Int, 2);
        let result = eval_binary(BinaryOpKind::Div, &left, &right).unwrap();
        assert_eq!(result.type_code, XmlTypeCode::Decimal);
        assert_eq!(result.as_decimal().unwrap(), Decimal::new(15, 1));
    }

    #[test]
    fn test_div_short_bounds_rounds_to_xpath_precision() {
        let left = int_value(XmlTypeCode::Short, -32768);
        let right = int_value(XmlTypeCode::Short, 32767);
        let result = eval_binary(BinaryOpKind::Div, &left, &right).unwrap();
        assert_eq!(result.type_code, XmlTypeCode::Decimal);
        assert_eq!(
            result.as_decimal().unwrap().to_string(),
            "-1.000030518509475997"
        );
    }

    #[test]
    fn test_idiv_double_truncates() {
        let left = XmlValue::double(5.9);
        let right = XmlValue::double(2.0);
        let result = eval_binary(BinaryOpKind::IDiv, &left, &right).unwrap();
        assert_eq!(result.type_code, XmlTypeCode::Integer);
        assert_eq!(result.as_integer().unwrap(), &BigInt::from(2));
    }

    #[test]
    fn test_unary_minus_unsigned_int_returns_long() {
        let value = int_value(XmlTypeCode::UnsignedInt, 5);
        let result = eval_unary(UnaryOpKind::Negate, &value).unwrap();
        assert_eq!(result.type_code, XmlTypeCode::Long);
        assert_eq!(result.as_integer().unwrap(), &BigInt::from(-5));
    }

    #[test]
    fn test_string_comparison() {
        let left = XmlValue::string("alpha");
        let right = XmlValue::string("beta");
        let result = eval_binary(BinaryOpKind::GeneralLt, &left, &right).unwrap();
        assert_eq!(result.as_boolean(), Some(true));
    }

    #[test]
    fn test_boolean_eq() {
        let left = XmlValue::boolean(true);
        let right = XmlValue::boolean(false);
        let result = eval_binary(BinaryOpKind::GeneralEq, &left, &right).unwrap();
        assert_eq!(result.as_boolean(), Some(false));
    }

    #[test]
    fn test_range_integer_sequence() {
        let start = int_value(XmlTypeCode::Integer, 1);
        let end = int_value(XmlTypeCode::Integer, 3);
        let result = eval_range(&start, &end).unwrap();
        let values: Vec<_> = result
            .iter()
            .map(|v| v.as_integer().unwrap().clone())
            .collect();
        assert_eq!(
            values,
            vec![BigInt::from(1), BigInt::from(2), BigInt::from(3)]
        );
    }

    #[test]
    fn test_decimal_eq() {
        let left = decimal_value("2.5");
        let right = decimal_value("2.5");
        let result = eval_binary(BinaryOpKind::GeneralEq, &left, &right).unwrap();
        assert_eq!(result.as_boolean(), Some(true));
    }

    #[test]
    fn test_datetime_add_year_month_clamps_day() {
        let left = datetime_value(XmlTypeCode::DateTime, 2023, 1, 31, 10, 0, Decimal::ZERO);
        let right = year_month_duration_value(0, 1);
        let result = eval_binary(BinaryOpKind::Add, &left, &right).unwrap();
        assert_eq!(result.type_code, XmlTypeCode::DateTime);
        match result.value {
            XmlValueKind::Atomic(XmlAtomicValue::DateTime(dt)) => {
                assert_eq!((dt.year, dt.month, dt.day), (2023, 2, 28));
                assert_eq!((dt.hour, dt.minute), (10, 0));
            }
            _ => panic!("Expected dateTime result"),
        }
    }

    #[test]
    fn test_date_sub_date_returns_day_time_duration() {
        let left = date_value(2024, 3, 15);
        let right = date_value(2024, 3, 14);
        let result = eval_binary(BinaryOpKind::Sub, &left, &right).unwrap();
        assert_eq!(result.type_code, XmlTypeCode::DayTimeDuration);
        match result.value {
            XmlValueKind::Atomic(XmlAtomicValue::DayTimeDuration(duration)) => {
                assert!(!duration.negative);
                assert_eq!(duration.days, 1);
                assert_eq!(duration.hours, 0);
                assert_eq!(duration.minutes, 0);
                assert!(duration.seconds.is_zero());
            }
            _ => panic!("Expected dayTimeDuration result"),
        }
    }

    #[test]
    fn test_time_add_day_time_wraps() {
        let left = time_value(23, 0, Decimal::ZERO);
        let right = day_time_duration_value(0, 2, 0, Decimal::ZERO);
        let result = eval_binary(BinaryOpKind::Add, &left, &right).unwrap();
        assert_eq!(result.type_code, XmlTypeCode::Time);
        match result.value {
            XmlValueKind::Atomic(XmlAtomicValue::Time(time)) => {
                assert_eq!((time.hour, time.minute), (1, 0));
            }
            _ => panic!("Expected time result"),
        }
    }

    #[test]
    fn test_time_compare_uses_implicit_timezone() {
        let implicit = host_implicit_timezone();
        let left = time_value(10, 0, Decimal::ZERO);
        let right = time_value_with_tz(10, 0, Decimal::ZERO, implicit);
        let result = eval_binary(BinaryOpKind::GeneralEq, &left, &right).unwrap();
        assert_eq!(result.as_boolean(), Some(true));
    }

    #[test]
    fn test_numeric_mul_year_month_duration() {
        let left = int_value(XmlTypeCode::Int, 2);
        let right = year_month_duration_value(1, 2);
        let result = eval_binary(BinaryOpKind::Mul, &left, &right).unwrap();
        assert_eq!(result.type_code, XmlTypeCode::YearMonthDuration);
        match result.value {
            XmlValueKind::Atomic(XmlAtomicValue::YearMonthDuration(duration)) => {
                assert_eq!((duration.years, duration.months), (2, 4));
            }
            _ => panic!("Expected yearMonthDuration result"),
        }
    }

    #[test]
    fn test_day_time_duration_div_duration_returns_decimal() {
        let left = day_time_duration_value(0, 3, 0, Decimal::ZERO);
        let right = day_time_duration_value(0, 1, 0, Decimal::ZERO);
        let result = eval_binary(BinaryOpKind::Div, &left, &right).unwrap();
        assert_eq!(result.type_code, XmlTypeCode::Decimal);
        assert_eq!(result.as_decimal(), Some(Decimal::from(3)));
    }

    #[test]
    fn test_duration_eq_across_subtypes() {
        let left = duration_value(false, 1, 2, 0, 0, 0, Decimal::ZERO);
        let right = year_month_duration_value(1, 2);
        let result = eval_binary(BinaryOpKind::GeneralEq, &left, &right).unwrap();
        assert_eq!(result.as_boolean(), Some(true));
    }

    #[test]
    fn test_datetime_sub_datetime_returns_day_time_duration() {
        let left = datetime_value(XmlTypeCode::DateTime, 2024, 3, 15, 12, 0, Decimal::ZERO);
        let right = datetime_value(XmlTypeCode::DateTime, 2024, 3, 15, 11, 0, Decimal::ZERO);
        let result = eval_binary(BinaryOpKind::Sub, &left, &right).unwrap();
        assert_eq!(result.type_code, XmlTypeCode::DayTimeDuration);
        match result.value {
            XmlValueKind::Atomic(XmlAtomicValue::DayTimeDuration(duration)) => {
                assert!(!duration.negative);
                assert_eq!((duration.days, duration.hours, duration.minutes), (0, 1, 0));
            }
            _ => panic!("Expected dayTimeDuration result"),
        }
    }

    // ========================================================================
    // General Comparison Tests
    // ========================================================================

    #[test]
    fn test_magnitude_relationship_untyped_to_numeric() {
        // UntypedAtomic compared with integer should promote to double
        let left = XmlValue::untyped("42");
        let right = int_value(XmlTypeCode::Integer, 42);
        let (promoted_left, promoted_right) = magnitude_relationship(&left, &right).unwrap();
        assert_eq!(promoted_left.type_code, XmlTypeCode::Double);
        assert_eq!(promoted_right.type_code, XmlTypeCode::Integer);
    }

    #[test]
    fn test_magnitude_relationship_untyped_to_string() {
        // UntypedAtomic compared with string should become string
        let left = XmlValue::untyped("hello");
        let right = XmlValue::string("world");
        let (promoted_left, promoted_right) = magnitude_relationship(&left, &right).unwrap();
        assert_eq!(promoted_left.type_code, XmlTypeCode::String);
        assert_eq!(promoted_right.type_code, XmlTypeCode::String);
    }

    #[test]
    fn test_magnitude_relationship_both_untyped() {
        // Both UntypedAtomic should both become string
        let left = XmlValue::untyped("abc");
        let right = XmlValue::untyped("def");
        let (promoted_left, promoted_right) = magnitude_relationship(&left, &right).unwrap();
        // When both are untyped, they stay as string-like
        assert!(is_string_like(promoted_left.type_code));
        assert!(is_string_like(promoted_right.type_code));
    }

    #[test]
    fn test_general_eq_with_untyped() {
        // "42" = 42 should be true (UntypedAtomic promoted to double)
        let left = XmlValue::untyped("42");
        let right = int_value(XmlTypeCode::Integer, 42);
        assert!(general_eq(&left, &right).unwrap());
    }

    #[test]
    fn test_general_eq_strings() {
        let left = XmlValue::string("hello");
        let right = XmlValue::string("hello");
        assert!(general_eq(&left, &right).unwrap());

        let right2 = XmlValue::string("world");
        assert!(!general_eq(&left, &right2).unwrap());
    }

    #[test]
    fn test_general_gt_with_untyped() {
        // "50" > 42 should be true
        let left = XmlValue::untyped("50");
        let right = int_value(XmlTypeCode::Integer, 42);
        assert!(general_gt(&left, &right).unwrap());
    }

    #[test]
    fn test_general_ne() {
        let left = XmlValue::string("a");
        let right = XmlValue::string("b");
        assert!(general_ne(&left, &right).unwrap());

        let same = XmlValue::string("a");
        assert!(!general_ne(&left, &same).unwrap());
    }

    #[test]
    fn test_general_comparisons_numeric() {
        let five = int_value(XmlTypeCode::Integer, 5);
        let ten = int_value(XmlTypeCode::Integer, 10);

        assert!(general_lt(&five, &ten).unwrap());
        assert!(general_le(&five, &ten).unwrap());
        assert!(!general_gt(&five, &ten).unwrap());
        assert!(!general_ge(&five, &ten).unwrap());

        assert!(general_ge(&five, &five).unwrap());
        assert!(general_le(&five, &five).unwrap());
    }

    #[test]
    fn test_value_comparisons() {
        let a = XmlValue::string("abc");
        let b = XmlValue::string("xyz");

        assert!(value_lt(&a, &b).unwrap());
        assert!(value_le(&a, &b).unwrap());
        assert!(!value_gt(&a, &b).unwrap());
        assert!(!value_ge(&a, &b).unwrap());
        assert!(!value_eq(&a, &b).unwrap());
    }

    // ========================================================================
    // Sequence General Comparison Tests (Cartesian product)
    // ========================================================================

    #[test]
    fn test_general_eq_seq_finds_match() {
        // (1, 2, 3) = (3, 4, 5) should be true because 3 appears in both
        let left = vec![
            int_value(XmlTypeCode::Integer, 1),
            int_value(XmlTypeCode::Integer, 2),
            int_value(XmlTypeCode::Integer, 3),
        ];
        let right = vec![
            int_value(XmlTypeCode::Integer, 3),
            int_value(XmlTypeCode::Integer, 4),
            int_value(XmlTypeCode::Integer, 5),
        ];
        assert!(general_eq_seq(&left, &right).unwrap());
    }

    #[test]
    fn test_general_eq_seq_no_match() {
        // (1, 2) = (3, 4) should be false because no common values
        let left = vec![
            int_value(XmlTypeCode::Integer, 1),
            int_value(XmlTypeCode::Integer, 2),
        ];
        let right = vec![
            int_value(XmlTypeCode::Integer, 3),
            int_value(XmlTypeCode::Integer, 4),
        ];
        assert!(!general_eq_seq(&left, &right).unwrap());
    }

    #[test]
    fn test_general_eq_seq_empty_is_false() {
        // Empty sequences always return false for general comparisons
        let left: Vec<XmlValue> = vec![];
        let right = vec![int_value(XmlTypeCode::Integer, 1)];
        assert!(!general_eq_seq(&left, &right).unwrap());
        assert!(!general_eq_seq(&right, &left).unwrap());
        assert!(!general_eq_seq(&left, &left).unwrap());
    }

    #[test]
    fn test_general_ne_seq() {
        // (1, 2) != (2, 3) should be true because 1 != 2, 1 != 3, 2 != 3 all true
        let left = vec![
            int_value(XmlTypeCode::Integer, 1),
            int_value(XmlTypeCode::Integer, 2),
        ];
        let right = vec![
            int_value(XmlTypeCode::Integer, 2),
            int_value(XmlTypeCode::Integer, 3),
        ];
        assert!(general_ne_seq(&left, &right).unwrap());

        // (1) != (1) should be false because no pair is not-equal
        let same = vec![int_value(XmlTypeCode::Integer, 1)];
        assert!(!general_ne_seq(&same, &same).unwrap());
    }

    #[test]
    fn test_general_lt_seq() {
        // (1, 2) < (3, 4) should be true because 1 < 3, 1 < 4, 2 < 3, 2 < 4
        let left = vec![
            int_value(XmlTypeCode::Integer, 1),
            int_value(XmlTypeCode::Integer, 2),
        ];
        let right = vec![
            int_value(XmlTypeCode::Integer, 3),
            int_value(XmlTypeCode::Integer, 4),
        ];
        assert!(general_lt_seq(&left, &right).unwrap());

        // (3, 4) < (1, 2) should be false
        assert!(!general_lt_seq(&right, &left).unwrap());

        // (1, 5) < (3, 4) should be true because 1 < 3, 1 < 4
        let mixed = vec![
            int_value(XmlTypeCode::Integer, 1),
            int_value(XmlTypeCode::Integer, 5),
        ];
        assert!(general_lt_seq(&mixed, &right).unwrap());
    }

    #[test]
    fn test_general_gt_seq() {
        // (3, 4) > (1, 2) should be true
        let left = vec![
            int_value(XmlTypeCode::Integer, 3),
            int_value(XmlTypeCode::Integer, 4),
        ];
        let right = vec![
            int_value(XmlTypeCode::Integer, 1),
            int_value(XmlTypeCode::Integer, 2),
        ];
        assert!(general_gt_seq(&left, &right).unwrap());
    }

    #[test]
    fn test_general_le_seq() {
        // (1, 2) <= (2, 3) should be true because 1 <= 2, 1 <= 3, 2 <= 2, 2 <= 3
        let left = vec![
            int_value(XmlTypeCode::Integer, 1),
            int_value(XmlTypeCode::Integer, 2),
        ];
        let right = vec![
            int_value(XmlTypeCode::Integer, 2),
            int_value(XmlTypeCode::Integer, 3),
        ];
        assert!(general_le_seq(&left, &right).unwrap());
    }

    #[test]
    fn test_general_ge_seq() {
        // (2, 3) >= (1, 2) should be true
        let left = vec![
            int_value(XmlTypeCode::Integer, 2),
            int_value(XmlTypeCode::Integer, 3),
        ];
        let right = vec![
            int_value(XmlTypeCode::Integer, 1),
            int_value(XmlTypeCode::Integer, 2),
        ];
        assert!(general_ge_seq(&left, &right).unwrap());
    }

    #[test]
    fn test_general_seq_with_type_promotion() {
        // UntypedAtomic should be promoted: ("42") = (42) should be true
        let left = vec![XmlValue::untyped("42")];
        let right = vec![int_value(XmlTypeCode::Integer, 42)];
        assert!(general_eq_seq(&left, &right).unwrap());
    }

    #[test]
    fn test_general_seq_mixed_types() {
        // Mixed types that can't compare just skip those pairs
        // (1, "hello") = ("hello", 2) should be true because "hello" = "hello"
        let left = vec![
            int_value(XmlTypeCode::Integer, 1),
            XmlValue::string("hello"),
        ];
        let right = vec![
            XmlValue::string("hello"),
            int_value(XmlTypeCode::Integer, 2),
        ];
        assert!(general_eq_seq(&left, &right).unwrap());
    }

    #[test]
    fn test_compare_ge_prefers_eq_over_ordering() {
        // QName ordering isn't defined, but equality is.
        let names = NameTable::new();
        let local = names.add("a");
        let qname = QNameValue::new(QualifiedName::local(local), &names);
        let left = XmlValue::new(
            XmlTypeCode::QName,
            XmlValueKind::Atomic(XmlAtomicValue::QName(qname)),
        );
        let right = left.clone();

        assert!(compare_ge(&left, &right, CollationRef::Codepoint, TimezoneOffset::UTC).unwrap());
        assert!(compare_le(&left, &right, CollationRef::Codepoint, TimezoneOffset::UTC).unwrap());
    }

    fn nmtoken(token: &str) -> XmlValue {
        XmlValue::new(
            XmlTypeCode::NmToken,
            XmlValueKind::Atomic(XmlAtomicValue::String(token.to_string())),
        )
    }

    #[test]
    fn test_list_equality() {
        let left = XmlValue::new(
            XmlTypeCode::NmTokens,
            XmlValueKind::List {
                item_type: XmlTypeCode::NmToken,
                items: vec![nmtoken("a"), nmtoken("b")],
            },
        );
        let right = left.clone();
        let different = XmlValue::new(
            XmlTypeCode::NmTokens,
            XmlValueKind::List {
                item_type: XmlTypeCode::NmToken,
                items: vec![nmtoken("a")],
            },
        );

        assert!(compare_eq(&left, &right, CollationRef::Codepoint, TimezoneOffset::UTC).unwrap());
        assert!(!compare_eq(
            &left,
            &different,
            CollationRef::Codepoint,
            TimezoneOffset::UTC
        )
        .unwrap());
    }

    #[test]
    fn test_union_unwrap_equality() {
        let inner = XmlValue::string("hello");
        let left = XmlValue::new(XmlTypeCode::String, XmlValueKind::Union(Box::new(inner)));
        let right = XmlValue::string("hello");
        assert!(compare_eq(&left, &right, CollationRef::Codepoint, TimezoneOffset::UTC).unwrap());
    }

    #[test]
    fn test_general_eq_iter_finds_match() {
        let names = NameTable::new();
        let context = XPathContext::new(&names);
        let left: VecNodeIterator<RoXmlNavigator<'static>> = VecNodeIterator::new(vec![
            XmlItem::Atomic(XmlValue::integer(BigInt::from(1))),
            XmlItem::Atomic(XmlValue::integer(BigInt::from(2))),
        ]);
        let right: VecNodeIterator<RoXmlNavigator<'static>> =
            VecNodeIterator::new(vec![XmlItem::Atomic(XmlValue::integer(BigInt::from(2)))]);

        assert!(general_eq_iter(&context, &left, &right).unwrap());
    }

    #[test]
    fn test_general_eq_iter_invalid_cast_errors() {
        let names = NameTable::new();
        let context = XPathContext::new(&names);
        let left: VecNodeIterator<RoXmlNavigator<'static>> =
            VecNodeIterator::new(vec![XmlItem::Atomic(XmlValue::untyped("not-a-number"))]);
        let right: VecNodeIterator<RoXmlNavigator<'static>> =
            VecNodeIterator::new(vec![XmlItem::Atomic(XmlValue::integer(BigInt::from(1)))]);

        let result = general_eq_iter(&context, &left, &right);
        assert!(matches!(result, Err(XPathError::FORG0001 { .. })));
    }

    /// A general comparison with no true pair reports the incomparable pair.
    ///
    /// XPath 2.0 §3.5.2 defers the comparison of a pair to the corresponding
    /// value comparison, and §3.5.1 makes an operand combination that no
    /// operator mapping covers a type error. `xs:boolean` against `xs:date` is
    /// such a combination, so the comparison is an error rather than `false` —
    /// the same outcome `test_general_gt_iter_type_mismatch_errors` asserts for
    /// `gt`.
    #[test]
    fn test_general_eq_iter_type_mismatch_errors() {
        let names = NameTable::new();
        let context = XPathContext::new(&names);
        let left: VecNodeIterator<RoXmlNavigator<'static>> =
            VecNodeIterator::new(vec![XmlItem::Atomic(XmlValue::boolean(true))]);
        let right: VecNodeIterator<RoXmlNavigator<'static>> =
            VecNodeIterator::new(vec![XmlItem::Atomic(date_value(2024, 1, 1))]);

        let result = general_eq_iter(&context, &left, &right);
        assert!(matches!(
            result,
            Err(XPathError::BinaryOperatorNotDefined { .. })
        ));
        assert_eq!(result.unwrap_err().error_code(), Some("XPTY0004"));
    }

    /// The same for `!=`: an incomparable pair is not an unequal pair.
    #[test]
    fn test_general_ne_iter_type_mismatch_errors() {
        let names = NameTable::new();
        let context = XPathContext::new(&names);
        let left: VecNodeIterator<RoXmlNavigator<'static>> =
            VecNodeIterator::new(vec![XmlItem::Atomic(XmlValue::boolean(true))]);
        let right: VecNodeIterator<RoXmlNavigator<'static>> =
            VecNodeIterator::new(vec![XmlItem::Atomic(date_value(2024, 1, 1))]);

        let result = general_ne_iter(&context, &left, &right);
        assert_eq!(result.unwrap_err().error_code(), Some("XPTY0004"));
    }

    /// A true pair still short-circuits past an incomparable one (§3.5.2:
    /// "an implementation may return true as soon as it finds an item in the
    /// first operand and an item in the second operand that have the required
    /// magnitude relationship").
    #[test]
    fn test_general_eq_iter_true_pair_wins_over_incomparable_pair() {
        let names = NameTable::new();
        let context = XPathContext::new(&names);
        let left: VecNodeIterator<RoXmlNavigator<'static>> = VecNodeIterator::new(vec![
            XmlItem::Atomic(XmlValue::integer(BigInt::from(1))),
            XmlItem::Atomic(date_value(2024, 1, 1)),
        ]);
        let right: VecNodeIterator<RoXmlNavigator<'static>> =
            VecNodeIterator::new(vec![XmlItem::Atomic(XmlValue::integer(BigInt::from(1)))]);

        assert!(general_eq_iter(&context, &left, &right).unwrap());
    }

    #[test]
    fn test_general_gt_iter_type_mismatch_errors() {
        let names = NameTable::new();
        let context = XPathContext::new(&names);
        let left: VecNodeIterator<RoXmlNavigator<'static>> =
            VecNodeIterator::new(vec![XmlItem::Atomic(XmlValue::boolean(true))]);
        let right: VecNodeIterator<RoXmlNavigator<'static>> =
            VecNodeIterator::new(vec![XmlItem::Atomic(XmlValue::string("false"))]);

        let result = general_gt_iter(&context, &left, &right);
        assert!(matches!(
            result,
            Err(XPathError::BinaryOperatorNotDefined { .. })
        ));
    }

    // =========================================================================
    // Primitive base type resolution tests (XPath 2.0 spec-aligned)
    // =========================================================================

    #[test]
    fn test_magnitude_relationship_ctx_numeric_to_double() {
        // Per XPath spec, UntypedAtomic is promoted to Double when compared with numeric types
        let names = NameTable::new();
        let context = XPathContext::new(&names);

        let untyped = XmlValue::untyped("42");
        let typed = XmlValue::integer(BigInt::from(100));

        let (left, _right) = magnitude_relationship_ctx(&context, &untyped, &typed).unwrap();

        assert_eq!(left.type_code, XmlTypeCode::Double);
        assert!((left.as_double().unwrap() - 42.0).abs() < 0.001);
    }

    #[test]
    fn test_magnitude_relationship_ctx_promotes_untyped() {
        let names = NameTable::new();
        let context = XPathContext::new(&names);

        // Test UntypedAtomic promoted to numeric type
        let untyped = XmlValue::untyped("2.5");
        let typed = XmlValue::double(1.5);

        let (left, right) = magnitude_relationship_ctx(&context, &untyped, &typed).unwrap();

        assert_eq!(left.type_code, XmlTypeCode::Double);
        assert!((left.as_double().unwrap() - 2.5).abs() < 0.001);
        assert_eq!(right.type_code, XmlTypeCode::Double);
    }

    #[test]
    fn test_magnitude_relationship_ctx_string_like() {
        let names = NameTable::new();
        let context = XPathContext::new(&names);

        // Test UntypedAtomic with string-like type stays as string
        let untyped = XmlValue::untyped("test");
        let typed = XmlValue::string("other");

        let (left, right) = magnitude_relationship_ctx(&context, &untyped, &typed).unwrap();

        assert_eq!(left.type_code, XmlTypeCode::String);
        assert_eq!(left.to_string_value(), "test");
        assert_eq!(right.type_code, XmlTypeCode::String);
    }

    #[test]
    fn test_get_xsd_primitive_type_string_derived() {
        // String-derived types should map to xs:string
        assert_eq!(
            get_xsd_primitive_type(XmlTypeCode::NormalizedString),
            XmlTypeCode::String
        );
        assert_eq!(
            get_xsd_primitive_type(XmlTypeCode::Token),
            XmlTypeCode::String
        );
        assert_eq!(
            get_xsd_primitive_type(XmlTypeCode::NCName),
            XmlTypeCode::String
        );
        assert_eq!(get_xsd_primitive_type(XmlTypeCode::Id), XmlTypeCode::String);
    }

    #[test]
    fn test_get_xsd_primitive_type_integer_derived() {
        // Integer-derived types should map to xs:decimal
        assert_eq!(
            get_xsd_primitive_type(XmlTypeCode::Integer),
            XmlTypeCode::Decimal
        );
        assert_eq!(
            get_xsd_primitive_type(XmlTypeCode::Long),
            XmlTypeCode::Decimal
        );
        assert_eq!(
            get_xsd_primitive_type(XmlTypeCode::Int),
            XmlTypeCode::Decimal
        );
        assert_eq!(
            get_xsd_primitive_type(XmlTypeCode::UnsignedInt),
            XmlTypeCode::Decimal
        );
    }

    #[test]
    fn test_get_xsd_primitive_type_duration_special_cases() {
        // dayTimeDuration and yearMonthDuration are their own primitives for XPath
        assert_eq!(
            get_xsd_primitive_type(XmlTypeCode::DayTimeDuration),
            XmlTypeCode::DayTimeDuration
        );
        assert_eq!(
            get_xsd_primitive_type(XmlTypeCode::YearMonthDuration),
            XmlTypeCode::YearMonthDuration
        );
        // But xs:duration is already primitive
        assert_eq!(
            get_xsd_primitive_type(XmlTypeCode::Duration),
            XmlTypeCode::Duration
        );
    }

    #[test]
    fn test_get_xsd_primitive_type_date_time() {
        // Date/time types are already primitive
        assert_eq!(
            get_xsd_primitive_type(XmlTypeCode::DateTime),
            XmlTypeCode::DateTime
        );
        assert_eq!(get_xsd_primitive_type(XmlTypeCode::Date), XmlTypeCode::Date);
        assert_eq!(get_xsd_primitive_type(XmlTypeCode::Time), XmlTypeCode::Time);
        // dateTimeStamp derives from dateTime
        assert_eq!(
            get_xsd_primitive_type(XmlTypeCode::DateTimeStamp),
            XmlTypeCode::DateTime
        );
    }

    #[test]
    fn test_get_primitive_base_type_without_schema() {
        // Without schema info, should use the type_code's primitive
        let primitive = get_primitive_base_type(None, None, XmlTypeCode::Integer);
        assert_eq!(primitive, XmlTypeCode::Decimal);

        let primitive = get_primitive_base_type(None, None, XmlTypeCode::NCName);
        assert_eq!(primitive, XmlTypeCode::String);
    }

    #[test]
    fn test_get_primitive_base_type_with_schema() {
        // With schema info, should resolve from the type definition
        let schema_set = SchemaSet::new();
        let builtin_types = schema_set.builtin_types();

        // xs:integer should resolve to xs:decimal
        let primitive = get_primitive_base_type(
            Some(&schema_set),
            Some(builtin_types.integer),
            XmlTypeCode::Integer,
        );
        assert_eq!(primitive, XmlTypeCode::Decimal);

        // xs:string should stay as xs:string (already primitive)
        let primitive = get_primitive_base_type(
            Some(&schema_set),
            Some(builtin_types.string),
            XmlTypeCode::String,
        );
        assert_eq!(primitive, XmlTypeCode::String);
    }

    #[test]
    fn test_magnitude_relationship_ctx_to_date() {
        // Test that UntypedAtomic is cast to xs:date when compared with date
        let names = NameTable::new();
        let context = XPathContext::new(&names);

        let untyped = XmlValue::untyped("2024-01-15");
        let typed = date_value(2024, 6, 1);

        let (left, right) = magnitude_relationship_ctx(&context, &untyped, &typed).unwrap();

        assert_eq!(left.type_code, XmlTypeCode::Date);
        assert_eq!(right.type_code, XmlTypeCode::Date);
    }

    #[test]
    fn test_magnitude_relationship_ctx_to_boolean() {
        // Test that UntypedAtomic is cast to xs:boolean when compared with boolean
        let names = NameTable::new();
        let context = XPathContext::new(&names);

        let untyped = XmlValue::untyped("true");
        let typed = XmlValue::boolean(false);

        let (left, right) = magnitude_relationship_ctx(&context, &untyped, &typed).unwrap();

        assert_eq!(left.type_code, XmlTypeCode::Boolean);
        assert_eq!(left.as_boolean(), Some(true));
        assert_eq!(right.type_code, XmlTypeCode::Boolean);
    }

    // ========================================================================
    // XPath 1.0 comparison tests
    // ========================================================================

    #[test]
    fn test_xpath10_eq_boolean_priority() {
        // "1" = true() → both convert to boolean → true (string "1" is truthy)
        let left = XmlValue::string("1".to_string());
        let right = XmlValue::boolean(true);
        let (l, r) = coerce_for_comparison_10(BinaryOpKind::GeneralEq, &left, &right);
        assert_eq!(l.type_code, XmlTypeCode::Boolean);
        assert_eq!(r.type_code, XmlTypeCode::Boolean);
        assert!(compare_eq(&l, &r, CollationRef::Codepoint, TimezoneOffset::UTC).unwrap());
    }

    #[test]
    fn test_xpath10_eq_boolean_priority_empty_string() {
        // "" = true() → both to boolean → false = true → false
        let left = XmlValue::string("".to_string());
        let right = XmlValue::boolean(true);
        let (l, r) = coerce_for_comparison_10(BinaryOpKind::GeneralEq, &left, &right);
        assert_eq!(l.as_boolean(), Some(false));
        assert_eq!(r.as_boolean(), Some(true));
        assert!(!compare_eq(&l, &r, CollationRef::Codepoint, TimezoneOffset::UTC).unwrap());
    }

    #[test]
    fn test_xpath10_relational_numeric() {
        // "3" < "10" → 3.0 < 10.0 → true (numeric comparison, not string)
        let left = XmlValue::string("3".to_string());
        let right = XmlValue::string("10".to_string());
        let (l, r) = coerce_for_comparison_10(BinaryOpKind::GeneralLt, &left, &right);
        assert_eq!(l.type_code, XmlTypeCode::Double);
        assert_eq!(r.type_code, XmlTypeCode::Double);
        assert!(compare_lt(&l, &r, CollationRef::Codepoint, TimezoneOffset::UTC).unwrap());
    }

    #[test]
    fn test_xpath10_eq_string_comparison() {
        // "3" = "10" → string compare → false
        let left = XmlValue::string("3".to_string());
        let right = XmlValue::string("10".to_string());
        let (l, r) = coerce_for_comparison_10(BinaryOpKind::GeneralEq, &left, &right);
        assert_eq!(l.type_code, XmlTypeCode::String);
        assert_eq!(r.type_code, XmlTypeCode::String);
        assert!(!compare_eq(&l, &r, CollationRef::Codepoint, TimezoneOffset::UTC).unwrap());
    }

    #[test]
    fn test_xpath10_eq_numeric_priority() {
        // "3" = 3.0 → numeric coercion → 3.0 = 3.0 → true
        let left = XmlValue::string("3".to_string());
        let right = XmlValue::double(3.0);
        let (l, r) = coerce_for_comparison_10(BinaryOpKind::GeneralEq, &left, &right);
        assert_eq!(l.type_code, XmlTypeCode::Double);
        assert_eq!(r.type_code, XmlTypeCode::Double);
        assert!(compare_eq(&l, &r, CollationRef::Codepoint, TimezoneOffset::UTC).unwrap());
    }

    #[test]
    fn test_xpath10_arithmetic_returns_double() {
        // 1 + 2 in XPath 1.0 → 3.0 (double, not integer)
        let left = XmlValue::integer(BigInt::from(1));
        let right = XmlValue::integer(BigInt::from(2));
        let result = eval_numeric_binary_10(BinaryOpKind::Add, &left, &right).unwrap();
        assert_eq!(result.type_code, XmlTypeCode::Double);
        assert_eq!(result.as_double(), Some(3.0));
    }

    #[test]
    fn test_xpath10_arithmetic_div_by_zero() {
        let left = XmlValue::double(1.0);
        let right = XmlValue::double(0.0);
        let result = eval_numeric_binary_10(BinaryOpKind::Div, &left, &right).unwrap();
        assert_eq!(result.as_double(), Some(f64::INFINITY));
    }

    #[test]
    fn test_xpath10_arithmetic_mod() {
        let left = XmlValue::integer(BigInt::from(5));
        let right = XmlValue::integer(BigInt::from(3));
        let result = eval_numeric_binary_10(BinaryOpKind::Mod, &left, &right).unwrap();
        assert_eq!(result.type_code, XmlTypeCode::Double);
        assert_eq!(result.as_double(), Some(2.0));
    }

    #[test]
    fn test_xpath10_general_eq_iter() {
        // Test iterator-based XPath 1.0 comparison: existential over sequences
        type Item = XmlItem<RoXmlNavigator<'static>>;
        let left_items: Vec<Item> = vec![XmlItem::Atomic(XmlValue::string("hello".to_string()))];
        let right_items: Vec<Item> = vec![XmlItem::Atomic(XmlValue::string("hello".to_string()))];
        let left_iter = VecNodeIterator::new(left_items);
        let right_iter = VecNodeIterator::new(right_items);
        assert!(general_eq_iter_10(&left_iter, &right_iter).unwrap());
    }

    #[test]
    fn test_xpath10_general_ne_iter() {
        type Item = XmlItem<RoXmlNavigator<'static>>;
        let left_items: Vec<Item> = vec![XmlItem::Atomic(XmlValue::string("a".to_string()))];
        let right_items: Vec<Item> = vec![XmlItem::Atomic(XmlValue::string("b".to_string()))];
        let left_iter = VecNodeIterator::new(left_items);
        let right_iter = VecNodeIterator::new(right_items);
        assert!(general_ne_iter_10(&left_iter, &right_iter).unwrap());
    }

    #[test]
    fn test_xpath10_general_lt_iter_numeric_coercion() {
        // "3" < "10" in XPath 1.0 → numeric → 3.0 < 10.0 → true
        type Item = XmlItem<RoXmlNavigator<'static>>;
        let left_items: Vec<Item> = vec![XmlItem::Atomic(XmlValue::string("3".to_string()))];
        let right_items: Vec<Item> = vec![XmlItem::Atomic(XmlValue::string("10".to_string()))];
        let left_iter = VecNodeIterator::new(left_items);
        let right_iter = VecNodeIterator::new(right_items);
        assert!(general_lt_iter_10(&left_iter, &right_iter).unwrap());
    }

    // ========================================================================
    // UntypedAtomic arithmetic promotion (XPath 2.0 Section 3.5.1)
    // ========================================================================

    #[test]
    fn test_untyped_atomic_add() {
        // UntypedAtomic("40") + integer(1) → double(41.0)
        let left = XmlValue::untyped("40");
        let right = int_value(XmlTypeCode::Integer, 1);
        let result = eval_binary(BinaryOpKind::Add, &left, &right).unwrap();
        assert_eq!(result.type_code, XmlTypeCode::Double);
        assert_eq!(result.as_double().unwrap(), 41.0);
    }

    #[test]
    fn test_untyped_atomic_both_sides() {
        // UntypedAtomic("40") + UntypedAtomic("40") → double(80.0)
        let left = XmlValue::untyped("40");
        let right = XmlValue::untyped("40");
        let result = eval_binary(BinaryOpKind::Add, &left, &right).unwrap();
        assert_eq!(result.type_code, XmlTypeCode::Double);
        assert_eq!(result.as_double().unwrap(), 80.0);
    }

    #[test]
    fn test_untyped_atomic_div() {
        // UntypedAtomic("40") div integer(2) → double(20.0)
        let left = XmlValue::untyped("40");
        let right = int_value(XmlTypeCode::Integer, 2);
        let result = eval_binary(BinaryOpKind::Div, &left, &right).unwrap();
        assert_eq!(result.type_code, XmlTypeCode::Double);
        assert_eq!(result.as_double().unwrap(), 20.0);
    }

    #[test]
    fn test_untyped_atomic_negate() {
        // -(UntypedAtomic("42")) → double(-42.0)
        let value = XmlValue::untyped("42");
        let result = eval_unary(UnaryOpKind::Negate, &value).unwrap();
        assert_eq!(result.type_code, XmlTypeCode::Double);
        assert_eq!(result.as_double().unwrap(), -42.0);
    }

    #[test]
    fn test_untyped_atomic_non_numeric_fails() {
        // UntypedAtomic("abc") + integer(1) → cast error
        let left = XmlValue::untyped("abc");
        let right = int_value(XmlTypeCode::Integer, 1);
        assert!(eval_binary(BinaryOpKind::Add, &left, &right).is_err());
    }

    /// The XPath comparisons of date/time and duration values agree with the
    /// order of the schema facets (`types::value_order`, batch (v)) once the
    /// implicit timezone is in place: two values with a timezone are totally
    /// ordered there, and `eq` / `gt` must give the same answer. Durations:
    /// `eq` is the identity of the months and seconds (F&O §10.4.5), the two
    /// subtypes are ordered as the value order orders them.
    #[test]
    fn temporal_comparisons_agree_with_the_value_order() {
        use crate::types::value_order::{date_time_or_duration_cmp, duration_parts};
        let parse = |code: XmlTypeCode, lexical: &str| {
            VALIDATOR_REGISTRY
                .validate(code, lexical)
                .unwrap_or_else(|e| panic!("{lexical}: {e:?}"))
        };
        let with_tz = |value: &XmlValue, tz: TimezoneOffset| -> XmlValue {
            let mut value = value.clone();
            if let XmlValueKind::Atomic(atom) = &mut value.value {
                match atom {
                    XmlAtomicValue::DateTime(v) => v.timezone = v.timezone.or(Some(tz)),
                    XmlAtomicValue::Date(v) => v.timezone = v.timezone.or(Some(tz)),
                    XmlAtomicValue::Time(v) => v.timezone = v.timezone.or(Some(tz)),
                    XmlAtomicValue::GYearMonth(v) => v.timezone = v.timezone.or(Some(tz)),
                    XmlAtomicValue::GYear(v) => v.timezone = v.timezone.or(Some(tz)),
                    XmlAtomicValue::GMonthDay(v) => v.timezone = v.timezone.or(Some(tz)),
                    XmlAtomicValue::GDay(v) => v.timezone = v.timezone.or(Some(tz)),
                    XmlAtomicValue::GMonth(v) => v.timezone = v.timezone.or(Some(tz)),
                    _ => {}
                }
            }
            value
        };
        let atom = |value: &XmlValue| match &value.value {
            XmlValueKind::Atomic(a) => a.clone(),
            other => panic!("{other:?}"),
        };
        let pools: &[(XmlTypeCode, &[&str])] = &[
            (
                XmlTypeCode::DateTime,
                &[
                    "2000-01-01T00:00:00Z",
                    "2000-01-01T00:00:00",
                    "2000-01-01T05:00:00+05:00",
                    "1999-12-31T23:00:00-01:00",
                    "2000-01-01T00:00:00.00000000000000000001Z",
                    "2000-01-01T00:00:00.0000000000000000000000000001",
                    "1999-12-31T23:59:59.9999999999999999999999999Z",
                    "2000-01-01T14:00:00",
                    "2000-01-01T00:00:00-14:00",
                    "0001-01-01T00:00:00Z",
                    "2147483647-12-31T23:59:59Z",
                ],
            ),
            (
                XmlTypeCode::Date,
                &[
                    "2000-01-01",
                    "2000-01-01Z",
                    "2000-01-02+14:00",
                    "1999-12-31-10:00",
                    "2000-01-01-14:00",
                    "2147483647-12-31",
                ],
            ),
            (
                XmlTypeCode::Time,
                &[
                    "00:00:00",
                    "00:00:00Z",
                    "05:00:00+05:00",
                    "23:59:59.9999999999999999999999999",
                    "12:00:00.0000000000000000000000001+14:00",
                    "10:00:00-14:00",
                ],
            ),
            (
                XmlTypeCode::GYear,
                &["2000", "2000Z", "2000+14:00", "1999-10:00", "2001"],
            ),
            (
                XmlTypeCode::GYearMonth,
                &["2000-01", "2000-01Z", "2000-02+14:00", "1999-12-10:00"],
            ),
            (
                XmlTypeCode::GMonthDay,
                &["--02-14", "--02-14+12:00", "--02-13-12:00", "--02-29"],
            ),
            (
                XmlTypeCode::GDay,
                &["---02", "---02-14:00", "---03+10:00", "---31", "---01Z"],
            ),
            (
                XmlTypeCode::GMonth,
                &["--05", "--05Z", "--06-14:00", "--04+14:00"],
            ),
        ];
        let timezones = [0, 300, -210, 840, -840].map(TimezoneOffset);
        for (code, lexicals) in pools {
            let values: Vec<XmlValue> = lexicals.iter().map(|l| parse(*code, l)).collect();
            for tz in timezones {
                for (a, la) in values.iter().zip(lexicals.iter()) {
                    for (b, lb) in values.iter().zip(lexicals.iter()) {
                        let order = date_time_or_duration_cmp(
                            &atom(&with_tz(a, tz)),
                            &atom(&with_tz(b, tz)),
                        )
                        .expect("two values with a timezone are ordered");
                        let eq = compare_eq(a, b, CollationRef::Codepoint, tz).unwrap();
                        assert_eq!(eq, order == Ordering::Equal, "{la} eq {lb} at {tz}");
                        if !operators_gregorian(*code) {
                            let gt = compare_gt(a, b, CollationRef::Codepoint, tz).unwrap();
                            assert_eq!(gt, order == Ordering::Greater, "{la} gt {lb} at {tz}");
                        }
                    }
                }
            }
        }
        let durations: &[(XmlTypeCode, &[&str])] = &[
            (
                XmlTypeCode::Duration,
                &[
                    "P1Y", "P12M", "P365D", "P400Y", "P146097D", "PT24H", "P1D",
                    "PT0.00000000000000000001S", "PT0S", "-PT0.5S",
                    "P4294967295Y4294967295M4294967295DT4294967295H4294967295M79228162514264337593543950335S",
                ],
            ),
            (
                XmlTypeCode::YearMonthDuration,
                &["P1Y", "P12M", "P0M", "-P13M", "P4294967295Y11M"],
            ),
            (
                XmlTypeCode::DayTimeDuration,
                &[
                    "P1D", "PT24H", "PT0S", "-PT0.5S", "P10000DT0.0000000000000000000001S",
                    "P10000D", "P4294967295DT4294967295H4294967295M79228162514264337593543950335S",
                ],
            ),
        ];
        for (code, lexicals) in durations {
            let values: Vec<XmlValue> = lexicals.iter().map(|l| parse(*code, l)).collect();
            for (a, la) in values.iter().zip(lexicals.iter()) {
                for (b, lb) in values.iter().zip(lexicals.iter()) {
                    let (pa, pb) = (
                        duration_parts(&atom(a)).unwrap(),
                        duration_parts(&atom(b)).unwrap(),
                    );
                    let eq =
                        compare_eq(a, b, CollationRef::Codepoint, TimezoneOffset::UTC).unwrap();
                    assert_eq!(eq, pa == pb, "{la} eq {lb}");
                    if *code != XmlTypeCode::Duration {
                        let order = date_time_or_duration_cmp(&atom(a), &atom(b))
                            .expect("the two subtypes are totally ordered");
                        let gt =
                            compare_gt(a, b, CollationRef::Codepoint, TimezoneOffset::UTC).unwrap();
                        assert_eq!(gt, order == Ordering::Greater, "{la} gt {lb}");
                        assert_eq!(eq, order == Ordering::Equal, "{la} eq {lb} (order)");
                    }
                }
            }
        }
    }

    fn operators_gregorian(code: XmlTypeCode) -> bool {
        is_gregorian_code(code)
    }
}
