//! Type casting operations for XPath evaluation.
//!
//! This module implements XPath 2.0 type casting rules for converting
//! values between different types.
//!
//! ## Casting Rules
//!
//! - `cast_to`: Explicit cast expression (`value cast as type`)
//! - `treat_as`: Type assertion without conversion (`value treat as type`)
//! - `instance_of`: Type test (`value instance of type`)
//! - `castable`: Castability test (`value castable as type`)

use num_bigint::BigInt;
use rust_decimal::Decimal;

use super::error::XPathError;
use crate::namespace::qname::QualifiedName;
use crate::namespace::table::{well_known, NameTable};
use crate::types::value::{
    DateTimeValue, DateValue, DayTimeDurationValue, DurationValue, GDayValue, GMonthDayValue,
    GMonthValue, GYearMonthValue, GYearValue, TimeValue, XmlAtomicValue, XmlValue, XmlValueKind,
    YearMonthDurationValue,
};
use crate::types::{XmlTypeCode, VALIDATOR_REGISTRY};
use crate::xpath::ast::OccurrenceIndicator;

/// Check if a cast from `source` to `target` is allowed by the XPath 2.0 casting table.
///
/// Implements the casting rules from XPath Functions and Operators Section 17.1.
/// Returns `true` if the cast is permitted, `false` if it should raise XPTY0004.
fn can_cast(source: XmlTypeCode, target: XmlTypeCode) -> bool {
    // Same type is always allowed
    if source == target {
        return true;
    }

    // Casting to NOTATION is never allowed
    if target == XmlTypeCode::Notation {
        return false;
    }

    // untypedAtomic and string can cast to anything (except NOTATION, handled above)
    if source == XmlTypeCode::UntypedAtomic
        || source == XmlTypeCode::String
        || source.is_string_derived()
    {
        return true;
    }

    // Determine the effective source category (map integer subtypes to numeric)
    let source_numeric = source.is_numeric();
    let source_boolean = source == XmlTypeCode::Boolean;
    let source_duration = matches!(
        source,
        XmlTypeCode::Duration | XmlTypeCode::YearMonthDuration | XmlTypeCode::DayTimeDuration
    );
    let source_datetime = source == XmlTypeCode::DateTime || source == XmlTypeCode::DateTimeStamp;
    let source_date = source == XmlTypeCode::Date;
    let source_time = source == XmlTypeCode::Time;
    let source_gyearmonth = source == XmlTypeCode::GYearMonth;
    let source_gyear = source == XmlTypeCode::GYear;
    let source_gmonthday = source == XmlTypeCode::GMonthDay;
    let source_gday = source == XmlTypeCode::GDay;
    let source_gmonth = source == XmlTypeCode::GMonth;
    let source_binary = matches!(source, XmlTypeCode::Base64Binary | XmlTypeCode::HexBinary);
    let source_anyuri = source == XmlTypeCode::AnyUri;

    let target_ua_or_string = target == XmlTypeCode::UntypedAtomic
        || target == XmlTypeCode::String
        || target.is_string_derived();
    let target_numeric = target.is_numeric() || target == XmlTypeCode::Boolean;

    // numeric types → uA, string, float, double, decimal, integer (subtypes), boolean
    if source_numeric {
        return target_ua_or_string || target_numeric;
    }

    // boolean → uA, string, float, double, decimal, integer (subtypes), boolean
    if source_boolean {
        return target_ua_or_string || target_numeric;
    }

    // duration types → uA, string, duration, yearMonthDuration, dayTimeDuration
    if source_duration {
        return target_ua_or_string
            || matches!(
                target,
                XmlTypeCode::Duration
                    | XmlTypeCode::YearMonthDuration
                    | XmlTypeCode::DayTimeDuration
            );
    }

    // dateTime/dateTimeStamp → uA, string, dateTime, dateTimeStamp, date, time, gYearMonth, gYear, gMonthDay, gDay, gMonth
    if source_datetime {
        return target_ua_or_string
            || matches!(
                target,
                XmlTypeCode::DateTime
                    | XmlTypeCode::DateTimeStamp
                    | XmlTypeCode::Date
                    | XmlTypeCode::Time
                    | XmlTypeCode::GYearMonth
                    | XmlTypeCode::GYear
                    | XmlTypeCode::GMonthDay
                    | XmlTypeCode::GDay
                    | XmlTypeCode::GMonth
            );
    }

    // date → uA, string, dateTime, date, gYearMonth, gYear, gMonthDay, gDay, gMonth
    if source_date {
        return target_ua_or_string
            || matches!(
                target,
                XmlTypeCode::DateTime
                    | XmlTypeCode::Date
                    | XmlTypeCode::GYearMonth
                    | XmlTypeCode::GYear
                    | XmlTypeCode::GMonthDay
                    | XmlTypeCode::GDay
                    | XmlTypeCode::GMonth
            );
    }

    // time → uA, string, time
    if source_time {
        return target_ua_or_string || target == XmlTypeCode::Time;
    }

    // gYearMonth → uA, string, gYearMonth
    if source_gyearmonth {
        return target_ua_or_string || target == XmlTypeCode::GYearMonth;
    }

    // gYear → uA, string, gYear
    if source_gyear {
        return target_ua_or_string || target == XmlTypeCode::GYear;
    }

    // gMonthDay → uA, string, gMonthDay
    if source_gmonthday {
        return target_ua_or_string || target == XmlTypeCode::GMonthDay;
    }

    // gDay → uA, string, gDay
    if source_gday {
        return target_ua_or_string || target == XmlTypeCode::GDay;
    }

    // gMonth → uA, string, gMonth
    if source_gmonth {
        return target_ua_or_string || target == XmlTypeCode::GMonth;
    }

    // base64Binary / hexBinary → uA, string, base64Binary, hexBinary
    if source_binary {
        return target_ua_or_string
            || matches!(target, XmlTypeCode::Base64Binary | XmlTypeCode::HexBinary);
    }

    // anyURI → uA, string, anyURI
    if source_anyuri {
        return target_ua_or_string || target == XmlTypeCode::AnyUri;
    }

    // QName / NOTATION → uA, string (F&O §17.1.2 gives the result:
    // `prefix:local`, or the local name); the same type was decided above.
    if matches!(source, XmlTypeCode::QName | XmlTypeCode::Notation) {
        return target_ua_or_string;
    }

    false
}

/// Cast an atomic value to a target type.
///
/// This implements the XPath `cast as` expression for atomic values.
///
/// # Arguments
///
/// * `value` - The value to cast
/// * `target_type` - The target type code
///
/// # Returns
///
/// * `Ok(XmlValue)` - The cast value
/// * `Err(XPathError)` - If casting fails (FORG0001) or types are incompatible (XPTY0004)
pub fn cast_to(value: &XmlValue, target_type: XmlTypeCode) -> Result<XmlValue, XPathError> {
    // Same type - no conversion needed
    if value.type_code == target_type {
        return Ok(value.clone());
    }

    // Check the XPath 2.0 casting table
    if !can_cast(value.type_code, target_type) {
        return Err(XPathError::type_mismatch(
            format!("{:?}", value.type_code),
            format!("{:?}", target_type),
        ));
    }

    let string_val = value.to_string_value();

    match target_type {
        XmlTypeCode::String => Ok(XmlValue::string(string_val)),

        XmlTypeCode::Boolean => cast_to_boolean(value, &string_val),

        XmlTypeCode::Decimal => cast_to_decimal(value, &string_val),

        XmlTypeCode::Integer => cast_to_integer(value, &string_val),

        XmlTypeCode::Float => cast_to_float(value, &string_val),

        XmlTypeCode::Double => cast_to_double(value, &string_val),

        XmlTypeCode::UntypedAtomic => Ok(XmlValue::untyped(string_val)),

        // Integer-derived types: cast to integer first, then validate range
        target if is_integer_derived(target) => cast_to_integer_subtype(value, target),

        // Date/time types: try direct cross-casting first, then parse via VALIDATOR_REGISTRY
        XmlTypeCode::DateTime
        | XmlTypeCode::Date
        | XmlTypeCode::Time
        | XmlTypeCode::Duration
        | XmlTypeCode::YearMonthDuration
        | XmlTypeCode::DayTimeDuration
        | XmlTypeCode::GYearMonth
        | XmlTypeCode::GYear
        | XmlTypeCode::GMonthDay
        | XmlTypeCode::GDay
        | XmlTypeCode::GMonth
        | XmlTypeCode::DateTimeStamp => {
            // Try direct cross-casting between date/time/duration types
            if let Some(result) = cast_datetime_cross(value, target_type) {
                return result;
            }
            let type_name = target_type.local_name().unwrap_or("unknown");
            VALIDATOR_REGISTRY
                .validate(target_type, string_val.trim())
                .map_err(|_| {
                    beyond_value_limits(target_type, string_val.trim()).unwrap_or_else(|| {
                        XPathError::invalid_cast_value(&string_val, format!("xs:{}", type_name))
                    })
                })
        }

        // Binary and URI types: try direct binary cross-casting, then parse via VALIDATOR_REGISTRY
        XmlTypeCode::AnyUri | XmlTypeCode::HexBinary | XmlTypeCode::Base64Binary => {
            // Direct binary cross-casting (base64Binary ↔ hexBinary)
            if let Some(result) = cast_binary_cross(value, target_type) {
                return result;
            }
            let type_name = target_type.local_name().unwrap_or("unknown");
            VALIDATOR_REGISTRY
                .validate(target_type, string_val.trim())
                .map_err(|_| {
                    XPathError::invalid_cast_value(&string_val, format!("xs:{}", type_name))
                })
        }

        // String-derived types: parse via VALIDATOR_REGISTRY
        XmlTypeCode::NormalizedString
        | XmlTypeCode::Token
        | XmlTypeCode::Language
        | XmlTypeCode::NmToken
        | XmlTypeCode::Name
        | XmlTypeCode::NCName
        | XmlTypeCode::Id
        | XmlTypeCode::IdRef
        | XmlTypeCode::Entity => {
            let type_name = target_type.local_name().unwrap_or("unknown");
            VALIDATOR_REGISTRY
                .validate(target_type, &string_val)
                .map_err(|_| {
                    XPathError::invalid_cast_value(&string_val, format!("xs:{}", type_name))
                })
        }

        // QName and NOTATION require special handling (not castable from string)
        XmlTypeCode::QName | XmlTypeCode::Notation => Err(XPathError::type_mismatch(
            format!("{:?}", value.type_code),
            format!("{:?}", target_type),
        )),

        // Unsupported cast
        _ => Err(XPathError::type_mismatch(
            format!("{:?}", value.type_code),
            format!("{:?}", target_type),
        )),
    }
}

/// Cast a value to boolean.
fn cast_to_boolean(value: &XmlValue, string_val: &str) -> Result<XmlValue, XPathError> {
    let result = match &value.value {
        XmlValueKind::Atomic(XmlAtomicValue::Boolean(b)) => *b,
        XmlValueKind::Atomic(XmlAtomicValue::Integer(i)) => *i != BigInt::from(0),
        XmlValueKind::Atomic(XmlAtomicValue::Decimal(d)) => !d.is_zero(),
        XmlValueKind::Atomic(XmlAtomicValue::Float(f)) => *f != 0.0 && !f.is_nan(),
        XmlValueKind::Atomic(XmlAtomicValue::Double(d)) => *d != 0.0 && !d.is_nan(),
        // An xs:decimal carried in lexical form (beyond `rust_decimal`).
        _ if crate::types::exact_decimal::lexical_decimal(value).is_some() => {
            crate::types::exact_decimal::ExactDecimal::from_value(value)
                .is_some_and(|d| !d.is_zero())
        }
        _ => {
            let s = string_val.trim();
            match s {
                "true" | "1" => true,
                "false" | "0" => false,
                _ => {
                    return Err(XPathError::invalid_cast_value(string_val, "xs:boolean"));
                }
            }
        }
    };
    Ok(XmlValue::boolean(result))
}

/// Cast a value to decimal.
fn cast_to_decimal(value: &XmlValue, string_val: &str) -> Result<XmlValue, XPathError> {
    let result = match &value.value {
        XmlValueKind::Atomic(XmlAtomicValue::Decimal(d)) => *d,
        // F&O §17.1.1: "In casting to numerics, if the value is too large or
        // too small to be accurately represented by the implementation, it is
        // handled as an overflow or underflow as defined in 6.2" — FOAR0002.
        XmlValueKind::Atomic(XmlAtomicValue::Integer(i)) => i
            .to_string()
            .parse::<Decimal>()
            .map_err(|_| XPathError::FOAR0002)?,
        XmlValueKind::Atomic(XmlAtomicValue::Float(f)) => {
            if f.is_nan() || f.is_infinite() {
                return Err(special_float_value(string_val));
            }
            if *f == 0.0 {
                Decimal::ZERO
            } else {
                decimal_from_binary(Decimal::try_from(*f).ok(), f64::from(*f), string_val)?
            }
        }
        XmlValueKind::Atomic(XmlAtomicValue::Double(d)) => {
            if d.is_nan() || d.is_infinite() {
                return Err(special_float_value(string_val));
            }
            if *d == 0.0 {
                Decimal::ZERO
            } else {
                decimal_from_binary(Decimal::try_from(*d).ok(), *d, string_val)?
            }
        }
        XmlValueKind::Atomic(XmlAtomicValue::Boolean(b)) => {
            if *b {
                Decimal::ONE
            } else {
                Decimal::ZERO
            }
        }
        _ => decimal_from_lexical(string_val)?,
    };
    Ok(XmlValue::decimal(result))
}

/// F&O §17.1.3.3 (casting to `xs:decimal`) and §17.1.3.4 (to `xs:integer`):
/// "If SV is one of the special xs:float or xs:double values NaN, INF, or
/// -INF, an error is raised \[err:FOCA0002\]."
fn special_float_value(lexical: &str) -> XPathError {
    XPathError::FOCA0002 {
        value: lexical.to_string(),
    }
}

/// The `xs:decimal` a finite, nonzero `xs:float` / `xs:double` value `value`
/// casts to, given `converted`, `rust_decimal`'s conversion of it (`None`
/// when it failed).
///
/// F&O 1.0 §17.1.3.3: "TV is the xs:decimal value, within the set of
/// xs:decimal values that the implementation is capable of representing,
/// that is numerically closest to SV. If two values are equally close, then
/// the one that is closest to zero is chosen. If SV is too large to be
/// accommodated as an xs:decimal … an error is raised \[err:FOCA0001\]."
/// The conversion fails only beyond the 96-bit mantissa (FOCA0001), and it
/// gives 0 for every magnitude below 10⁻²⁸, the smallest positive value held
/// — although above 5 × 10⁻²⁹ the numerically closest held value is
/// ±10⁻²⁸. Elsewhere the conversion's result is kept: it is the shortest
/// decimal that round-trips to the same binary value (`xs:decimal(0.1e0)` is
/// `0.1`), within half a unit of the 28th fraction digit.
fn decimal_from_binary(
    converted: Option<Decimal>,
    value: f64,
    lexical: &str,
) -> Result<Decimal, XPathError> {
    match converted {
        Some(d) if !d.is_zero() => Ok(crate::types::exact_decimal::unsigned_zero(d)),
        Some(_) => Ok(if above_half_smallest_decimal(value) {
            // ±10⁻²⁸.
            Decimal::new(value.signum() as i64, 28)
        } else {
            Decimal::ZERO
        }),
        None => Err(XPathError::decimal_too_large(lexical)),
    }
}

/// Whether the magnitude of the finite `value` is above 5 × 10⁻²⁹ — closer
/// to 10⁻²⁸ than to 0 — compared exactly: `value` is `m × 2^e`, so `|value|
/// > 5 × 10⁻²⁹` is `m × 10²⁹ × 2^max(e, 0) > 5 × 2^max(−e, 0)`.
fn above_half_smallest_decimal(value: f64) -> bool {
    use num_bigint::BigUint;
    let bits = value.abs().to_bits();
    let exponent_bits = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1u64 << 52) - 1);
    let (mantissa, exponent) = if exponent_bits == 0 {
        (fraction, -1074)
    } else {
        (fraction | (1u64 << 52), exponent_bits - 1075)
    };
    let mut left = BigUint::from(mantissa) * BigUint::from(10u32).pow(29);
    let mut right = BigUint::from(5u32);
    if exponent >= 0 {
        left <<= exponent as usize;
    } else {
        right <<= (-exponent) as usize;
    }
    left > right
}

/// The F&O error for a lexically valid date/time or duration literal whose
/// numbers the value structs cannot hold, or `None` when the literal is not
/// lexically valid for `target` at all (then the cast fails with FORG0001).
///
/// F&O 1.0 §17.1.1: "In casting to xs:date, xs:dateTime, xs:gYear, or
/// xs:gYearMonth (or types derived from these), if the value is too large or
/// too small to be represented by the implementation, error \[err:FODT0001\]
/// is raised. In casting to a duration value, if the value is too large or too
/// small to be represented by the implementation, error \[err:FODT0002\] is
/// raised." The limits are those of `types::value`: a year in `i32`, the
/// year, month, day, hour and minute numbers of a duration in `u32`, whole
/// seconds below 2⁹⁶. A literal beyond them is recognised by putting held
/// numbers in their places — a year with the same leap-year status, `0` for a
/// duration's numbers — and validating the result.
fn beyond_value_limits(target: XmlTypeCode, lexical: &str) -> Option<XPathError> {
    use num_bigint::BigInt;
    match target {
        XmlTypeCode::Date
        | XmlTypeCode::DateTime
        | XmlTypeCode::DateTimeStamp
        | XmlTypeCode::GYear
        | XmlTypeCode::GYearMonth => {
            let (negative, digits) = match lexical.strip_prefix('-') {
                Some(rest) => (true, rest),
                None => (false, lexical),
            };
            let end = digits
                .bytes()
                .position(|b| !b.is_ascii_digit())
                .unwrap_or(digits.len());
            let (year, rest) = digits.split_at(end);
            // A year has four digits or more, and no leading zero beyond four.
            if year.len() < 4 || (year.len() > 4 && year.starts_with('0')) {
                return None;
            }
            let value: BigInt = year.parse().ok()?;
            let value = if negative { -value } else { value };
            if i32::try_from(&value).is_ok() {
                return None;
            }
            // A year of the same leap-year status (the Gregorian cycle is 400
            // years), positive and of four digits.
            let cycle: BigInt = ((value % 400) + 400) % 400;
            let substitute = format!("{}{rest}", 2000 + cycle.to_string().parse::<i32>().ok()?);
            VALIDATOR_REGISTRY
                .validate(target, &substitute)
                .ok()
                .map(|_| XPathError::FODT0001)
        }
        XmlTypeCode::Duration | XmlTypeCode::YearMonthDuration | XmlTypeCode::DayTimeDuration => {
            let mut too_large = false;
            let mut substitute = String::with_capacity(lexical.len());
            let mut chars = lexical.char_indices().peekable();
            while let Some((start, c)) = chars.next() {
                if !c.is_ascii_digit() {
                    substitute.push(c);
                    continue;
                }
                let mut end = start + 1;
                while let Some(&(i, d)) = chars.peek() {
                    if d.is_ascii_digit() {
                        end = i + 1;
                        chars.next();
                    } else {
                        break;
                    }
                }
                let number = &lexical[start..end];
                let designator = lexical[end..].chars().next();
                let fraction = designator == Some('.');
                let limit: BigInt =
                    if fraction || designator == Some('S') || lexical[..start].ends_with('.') {
                        // Seconds: whole seconds below 2⁹⁶; the fraction is
                        // truncated to the digits that fit, never refused.
                        BigInt::from(u128::MAX >> 32)
                    } else {
                        BigInt::from(u32::MAX)
                    };
                let held = lexical[..start].ends_with('.')
                    || number.parse::<BigInt>().map_or(true, |n| n <= limit);
                if held {
                    substitute.push_str(number);
                } else {
                    too_large = true;
                    substitute.push('0');
                }
            }
            if !too_large {
                return None;
            }
            VALIDATOR_REGISTRY
                .validate(target, &substitute)
                .ok()
                .map(|_| XPathError::FODT0002)
        }
        _ => None,
    }
}

/// The `xs:decimal` a string or `xs:untypedAtomic` value casts to, and the
/// value of an XPath decimal literal (XPath 2.0 §3.1.1 casts those the same
/// way).
///
/// F&O §17.1.1 maps the string with the XSD lexical mapping — narrower than
/// what `rust_decimal`'s parser accepts (no exponent, no `_`), so outside it
/// the cast fails with FORG0001. Then: "In casting to numerics, if the value
/// is too large or too small to be accurately represented by the
/// implementation, it is handled as an overflow or underflow as defined in
/// 6.2", where "For xs:decimal operations, overflow behavior ·must· raise an
/// error \[err:FOAR0002\]. On underflow, 0.0 must be returned." So a
/// magnitude beyond `rust_decimal`'s 96-bit mantissa raises FOAR0002, and a
/// non-zero magnitude below 10^−28 — the smallest positive value held — is 0
/// (never a negative zero). Otherwise "if the value is not too large or too
/// small but nevertheless cannot be represented accurately with the number of
/// decimal digits available to the implementation, the implementation may
/// round to the nearest representable value or may raise a dynamic error
/// \[err:FOCA0006\]" — this crate raises FOCA0006 (more than 28 fraction
/// digits, or a mantissa beyond 96 bits, at a magnitude of 10^−28 or more)
/// rather than round.
pub(crate) fn decimal_from_lexical(text: &str) -> Result<Decimal, XPathError> {
    use crate::types::exact_decimal::{parse_decimal_exact, DecimalParseError, ExactDecimal};
    let trimmed = text.trim();
    if !crate::types::validators::is_valid_xsd_decimal_lexical(trimmed) {
        return Err(XPathError::invalid_cast_value(text, "xs:decimal"));
    }
    match parse_decimal_exact(trimmed) {
        Ok(d) => Ok(d),
        Err(DecimalParseError::TooLarge) => Err(XPathError::FOAR0002),
        Err(DecimalParseError::TooPrecise)
            if ExactDecimal::parse(trimmed).is_some_and(|d| d.is_below_smallest_decimal()) =>
        {
            Ok(Decimal::ZERO)
        }
        Err(DecimalParseError::TooPrecise) => Err(XPathError::decimal_too_precise(trimmed)),
        Err(DecimalParseError::Invalid) => Err(XPathError::invalid_cast_value(text, "xs:decimal")),
    }
}

/// Cast a value to integer.
fn cast_to_integer(value: &XmlValue, string_val: &str) -> Result<XmlValue, XPathError> {
    let result = match &value.value {
        XmlValueKind::Atomic(XmlAtomicValue::Integer(i)) => i.clone(),
        XmlValueKind::Atomic(XmlAtomicValue::Decimal(d)) => {
            // Truncate decimal to integer
            let truncated = d.trunc();
            truncated
                .to_string()
                .parse::<BigInt>()
                .map_err(|_| XPathError::invalid_cast_value(string_val, "xs:integer"))?
        }
        XmlValueKind::Atomic(XmlAtomicValue::Float(f)) => {
            if f.is_nan() || f.is_infinite() {
                return Err(special_float_value(string_val));
            }
            let truncated = f.trunc() as f64;
            // Use string round-trip to handle values outside i64 range
            let s = format!("{:.0}", truncated);
            s.parse::<BigInt>().map_err(|_| XPathError::FOCA0003 {
                message: format!("Value {} is too large for xs:integer", string_val),
            })?
        }
        XmlValueKind::Atomic(XmlAtomicValue::Double(d)) => {
            if d.is_nan() || d.is_infinite() {
                return Err(special_float_value(string_val));
            }
            // Use string round-trip to handle values outside i64 range
            let s = format!("{:.0}", d.trunc());
            s.parse::<BigInt>().map_err(|_| XPathError::FOCA0003 {
                message: format!("Value {} is too large for xs:integer", string_val),
            })?
        }
        XmlValueKind::Atomic(XmlAtomicValue::Boolean(b)) => BigInt::from(if *b { 1 } else { 0 }),
        // An xs:decimal carried in lexical form: F&O §17.1.3.4, "SV with the
        // fractional part discarded", exactly.
        _ if crate::types::exact_decimal::lexical_decimal(value).is_some() => {
            crate::types::exact_decimal::ExactDecimal::from_value(value)
                .map(|d| d.trunc_to_bigint())
                .ok_or_else(|| XPathError::invalid_cast_value(string_val, "xs:integer"))?
        }
        _ => string_val
            .trim()
            .parse::<BigInt>()
            .map_err(|_| XPathError::invalid_cast_value(string_val, "xs:integer"))?,
    };
    Ok(XmlValue::integer(result))
}

/// Cast a value to float.
fn cast_to_float(value: &XmlValue, string_val: &str) -> Result<XmlValue, XPathError> {
    let result = match &value.value {
        XmlValueKind::Atomic(XmlAtomicValue::Float(f)) => *f,
        XmlValueKind::Atomic(XmlAtomicValue::Double(d)) => *d as f32,
        XmlValueKind::Atomic(XmlAtomicValue::Decimal(d)) => d
            .to_string()
            .parse::<f32>()
            .map_err(|_| XPathError::invalid_cast_value(string_val, "xs:float"))?,
        XmlValueKind::Atomic(XmlAtomicValue::Integer(i)) => i
            .to_string()
            .parse::<f32>()
            .map_err(|_| XPathError::invalid_cast_value(string_val, "xs:float"))?,
        XmlValueKind::Atomic(XmlAtomicValue::Boolean(b)) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        _ => parse_float_with_special(string_val.trim())
            .map_err(|_| XPathError::invalid_cast_value(string_val, "xs:float"))?,
    };
    Ok(XmlValue::float(result))
}

/// Cast a value to double.
fn cast_to_double(value: &XmlValue, string_val: &str) -> Result<XmlValue, XPathError> {
    let result = match &value.value {
        XmlValueKind::Atomic(XmlAtomicValue::Double(d)) => *d,
        XmlValueKind::Atomic(XmlAtomicValue::Float(f)) => *f as f64,
        XmlValueKind::Atomic(XmlAtomicValue::Decimal(d)) => d
            .to_string()
            .parse::<f64>()
            .map_err(|_| XPathError::invalid_cast_value(string_val, "xs:double"))?,
        XmlValueKind::Atomic(XmlAtomicValue::Integer(i)) => i
            .to_string()
            .parse::<f64>()
            .map_err(|_| XPathError::invalid_cast_value(string_val, "xs:double"))?,
        XmlValueKind::Atomic(XmlAtomicValue::Boolean(b)) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        _ => parse_double_with_special(string_val.trim())
            .map_err(|_| XPathError::invalid_cast_value(string_val, "xs:double"))?,
    };
    Ok(XmlValue::double(result))
}

/// Parse a float string, handling special values like INF and NaN.
fn parse_float_with_special(s: &str) -> Result<f32, ()> {
    parse_xsd_float(s).ok_or(())
}

/// Parse a double string, handling special values like INF and NaN.
fn parse_double_with_special(s: &str) -> Result<f64, ()> {
    parse_xsd_double(s).ok_or(())
}

/// The `xs:double` whose lexical form is `lexical`, or `None` when it is not
/// one.
///
/// The lexical space (XSD 1.1 Part 2 §3.3.5): a decimal mantissa with an
/// optional sign and an optional exponent, or one of `INF`, `+INF`, `-INF`,
/// `NaN` — spelled exactly so. Surrounding XML whitespace is collapsed away
/// first (the type's `whiteSpace` facet is `collapse`). Rust's own `f64` parser
/// is not used on its own because it also accepts `inf`, `infinity`, `nan`,
/// `-NaN` and friends in any case; XPath 2.0 Appendix I.1 is explicit that
/// "the strings Infinity and -Infinity … cause a dynamic error when
/// compatibility mode is set to false" (and convert to NaN under `fn:number`).
///
/// Every conversion of a string or `xs:untypedAtomic` to `xs:double` in the
/// engine goes through this — `cast`, `fn:number`, arithmetic, comparisons,
/// `fn:sum`/`fn:avg`/`fn:min`/`fn:max` — so they all agree on what is a number.
pub(crate) fn parse_xsd_double(lexical: &str) -> Option<f64> {
    let s = trim_xml_whitespace(lexical);
    match s {
        "INF" | "+INF" => Some(f64::INFINITY),
        "-INF" => Some(f64::NEG_INFINITY),
        "NaN" => Some(f64::NAN),
        _ if is_xsd_numeral(s) => s.parse().ok(),
        _ => None,
    }
}

/// [`parse_xsd_double`] for `xs:float`, which has the same lexical space.
pub(crate) fn parse_xsd_float(lexical: &str) -> Option<f32> {
    let s = trim_xml_whitespace(lexical);
    match s {
        "INF" | "+INF" => Some(f32::INFINITY),
        "-INF" => Some(f32::NEG_INFINITY),
        "NaN" => Some(f32::NAN),
        _ if is_xsd_numeral(s) => s.parse().ok(),
        _ => None,
    }
}

fn trim_xml_whitespace(s: &str) -> &str {
    s.trim_matches(|c| matches!(c, ' ' | '\t' | '\r' | '\n'))
}

/// Whether `s` is `(\+|-)?([0-9]+(\.[0-9]*)?|\.[0-9]+)([Ee](\+|-)?[0-9]+)?` — the
/// numeric (non-special) part of the `xs:double` / `xs:float` lexical space.
fn is_xsd_numeral(s: &str) -> bool {
    let bytes = s.as_bytes();
    let mut i = 0;
    let digits = |i: &mut usize| {
        let start = *i;
        while *i < bytes.len() && bytes[*i].is_ascii_digit() {
            *i += 1;
        }
        *i - start
    };
    if matches!(bytes.first(), Some(b'+' | b'-')) {
        i += 1;
    }
    let integer_digits = digits(&mut i);
    let mut fraction_digits = 0;
    if bytes.get(i) == Some(&b'.') {
        i += 1;
        fraction_digits = digits(&mut i);
    }
    if integer_digits == 0 && fraction_digits == 0 {
        return false;
    }
    if matches!(bytes.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(bytes.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        if digits(&mut i) == 0 {
            return false;
        }
    }
    i == bytes.len()
}

/// Treat a value as a specific type (type assertion without conversion).
///
/// This implements the XPath `treat as` expression. Unlike `cast`, this
/// does not perform any conversion - it just validates that the value
/// already has the expected type.
///
/// # Arguments
///
/// * `value` - The value to check
/// * `target_type` - The expected type code
///
/// # Returns
///
/// * `Ok(XmlValue)` - The original value if it matches
/// * `Err(XPathError)` - XPTY0004 if type doesn't match
pub fn treat_as(value: &XmlValue, target_type: XmlTypeCode) -> Result<XmlValue, XPathError> {
    if type_matches(value.type_code, target_type) {
        Ok(value.clone())
    } else {
        Err(XPathError::type_mismatch(
            format!("{:?}", target_type),
            format!("{:?}", value.type_code),
        ))
    }
}

/// Check if a value is an instance of a type.
///
/// This implements the XPath `instance of` expression.
///
/// # Arguments
///
/// * `value` - The value to check
/// * `target_type` - The type to check against
///
/// # Returns
///
/// `true` if the value matches the type, `false` otherwise
pub fn instance_of(value: &XmlValue, target_type: XmlTypeCode) -> bool {
    type_matches(value.type_code, target_type)
}

/// Check if a value is an instance of a type (optional value version).
///
/// Returns true for None if target type allows empty sequence.
pub fn instance_of_opt(
    value: Option<&XmlValue>,
    target_type: XmlTypeCode,
    allow_empty: bool,
) -> bool {
    match value {
        None => allow_empty,
        Some(v) => instance_of(v, target_type),
    }
}

/// Check if a value can be cast to a type.
///
/// This implements the XPath `castable as` expression.
///
/// # Arguments
///
/// * `value` - The value to check
/// * `target_type` - The target type
///
/// # Returns
///
/// `true` if the cast would succeed, `false` otherwise
pub fn castable(value: &XmlValue, target_type: XmlTypeCode) -> bool {
    cast_to(value, target_type).is_ok()
}

/// Check if a value can be cast to a type (optional value version).
pub fn castable_opt(value: Option<&XmlValue>, target_type: XmlTypeCode, allow_empty: bool) -> bool {
    match value {
        None => allow_empty,
        Some(v) => castable(v, target_type),
    }
}

/// Check if a type code is an integer-derived type.
fn is_integer_derived(code: XmlTypeCode) -> bool {
    matches!(
        code,
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
            | XmlTypeCode::PositiveInteger
    )
}

/// The base type of a built-in atomic type.
///
/// One step up the built-in hierarchy of XSD Datatypes §3, extended with the
/// two types XPath 2.0 adds to it: `xs:untypedAtomic`, which derives directly
/// from `xs:anyAtomicType` (XDM §2.6) and is **not** a string, and
/// `xs:anyAtomicType` itself, which is where the walk stops.
///
/// Returns `None` for `xs:anyAtomicType` and for every code that is not a
/// built-in atomic type (nodes, the ur-types, the list types, `xs:error`).
fn builtin_base_type(code: XmlTypeCode) -> Option<XmlTypeCode> {
    use XmlTypeCode as T;
    Some(match code {
        // §3.3: the 19 primitives, plus xs:untypedAtomic, derive from
        // xs:anyAtomicType directly.
        T::UntypedAtomic
        | T::String
        | T::Boolean
        | T::Decimal
        | T::Float
        | T::Double
        | T::Duration
        | T::DateTime
        | T::Time
        | T::Date
        | T::GYearMonth
        | T::GYear
        | T::GMonthDay
        | T::GDay
        | T::GMonth
        | T::HexBinary
        | T::Base64Binary
        | T::AnyUri
        | T::QName
        | T::Notation => T::AnyAtomicType,

        // §3.4.1-§3.4.9: the xs:string branch.
        T::NormalizedString => T::String,
        T::Token => T::NormalizedString,
        T::Language | T::NmToken | T::Name => T::Token,
        T::NCName => T::Name,
        T::Id | T::IdRef | T::Entity => T::NCName,

        // §3.4.13-§3.4.25: the xs:decimal branch.
        T::Integer => T::Decimal,
        T::NonPositiveInteger | T::Long | T::NonNegativeInteger => T::Integer,
        T::NegativeInteger => T::NonPositiveInteger,
        T::Int => T::Long,
        T::Short => T::Int,
        T::Byte => T::Short,
        T::UnsignedLong | T::PositiveInteger => T::NonNegativeInteger,
        T::UnsignedInt => T::UnsignedLong,
        T::UnsignedShort => T::UnsignedInt,
        T::UnsignedByte => T::UnsignedShort,

        // §3.4.26/§3.4.27: the two duration subtypes; §3.4.28: xs:dateTimeStamp.
        T::YearMonthDuration | T::DayTimeDuration => T::Duration,
        T::DateTimeStamp => T::DateTime,

        _ => return None,
    })
}

/// Check if a source type matches a target type for type checking.
///
/// This is SequenceType matching for an AtomicType item type (XPath 2.0
/// §2.5.4): the value's type must be the named type itself or a type derived
/// from it by restriction. The relation is computed by walking
/// `builtin_base_type` up to `xs:anyAtomicType`, so every branch of the
/// built-in hierarchy is modelled, not just the string and numeric ones.
///
/// Two consequences worth naming, because they used to be wrong:
/// `xs:untypedAtomic` is **not** an `xs:string` (it derives from
/// `xs:anyAtomicType`), and `xs:dayTimeDuration` and `xs:yearMonthDuration`
/// **are** `xs:duration`s.
pub fn type_matches(source: XmlTypeCode, target: XmlTypeCode) -> bool {
    if source == target {
        return true;
    }

    // item() matches every item.
    if target == XmlTypeCode::Item {
        return true;
    }

    // Only an atomic value participates in the built-in atomic hierarchy.
    if !source.is_atomic() {
        return false;
    }

    // xs:anyAtomicType is the root of that hierarchy.
    if target == XmlTypeCode::AnyAtomicType {
        return true;
    }

    let mut current = source;
    while let Some(base) = builtin_base_type(current) {
        if base == target {
            return true;
        }
        current = base;
    }

    false
}

/// Convert a resolved atomic type QualifiedName to XmlTypeCode.
///
/// The QualifiedName should have been resolved during binding phase
/// and must be in the XS (XML Schema) namespace.
///
/// # Errors
///
/// Returns XPST0051 if the type name is not a known atomic type.
pub fn resolved_type_to_type_code(
    qname: &QualifiedName,
    names: &NameTable,
) -> Result<XmlTypeCode, XPathError> {
    // Check namespace is XS_NAMESPACE
    match qname.namespace_uri {
        Some(ns_id) if ns_id == well_known::XS_NAMESPACE => {}
        _ => {
            let local = names.resolve(qname.local_name);
            return Err(XPathError::XPST0051 {
                type_name: local.to_string(),
            });
        }
    }

    // Get local name and convert to type code
    let local_name = names.resolve(qname.local_name);
    XmlTypeCode::from_local_name(&local_name).ok_or_else(|| XPathError::XPST0051 {
        type_name: local_name.to_string(),
    })
}

/// Check if an occurrence indicator allows the given item count.
///
/// This implements XPath 2.0 sequence type cardinality matching:
/// - `One` (no indicator): exactly 1 item
/// - `ZeroOrOne` (`?`): 0 or 1 items
/// - `ZeroOrMore` (`*`): any count
/// - `OneOrMore` (`+`): at least 1 item
pub fn occurrence_allows_count(occ: OccurrenceIndicator, count: usize) -> bool {
    match occ {
        OccurrenceIndicator::One => count == 1,
        OccurrenceIndicator::ZeroOrOne => count <= 1,
        OccurrenceIndicator::ZeroOrMore => true,
        OccurrenceIndicator::OneOrMore => count >= 1,
    }
}

/// Cross-cast between date/time/duration types by directly extracting fields.
/// Returns None if no direct cross-cast applies (fall through to string parsing).
fn cast_datetime_cross(
    value: &XmlValue,
    target: XmlTypeCode,
) -> Option<Result<XmlValue, XPathError>> {
    match (&value.value, target) {
        // DateTime → Date
        (XmlValueKind::Atomic(XmlAtomicValue::DateTime(dt)), XmlTypeCode::Date) => {
            Some(Ok(XmlValue::new(
                XmlTypeCode::Date,
                XmlValueKind::Atomic(XmlAtomicValue::Date(DateValue {
                    year: dt.year,
                    month: dt.month,
                    day: dt.day,
                    timezone: dt.timezone,
                })),
            )))
        }
        // DateTime → Time
        (XmlValueKind::Atomic(XmlAtomicValue::DateTime(dt)), XmlTypeCode::Time) => {
            Some(Ok(XmlValue::new(
                XmlTypeCode::Time,
                XmlValueKind::Atomic(XmlAtomicValue::Time(TimeValue {
                    hour: dt.hour,
                    minute: dt.minute,
                    second: dt.second,
                    timezone: dt.timezone,
                })),
            )))
        }
        // Date → DateTime (time defaults to 00:00:00)
        (XmlValueKind::Atomic(XmlAtomicValue::Date(d)), XmlTypeCode::DateTime) => {
            Some(Ok(XmlValue::new(
                XmlTypeCode::DateTime,
                XmlValueKind::Atomic(XmlAtomicValue::DateTime(DateTimeValue {
                    year: d.year,
                    month: d.month,
                    day: d.day,
                    hour: 0,
                    minute: 0,
                    second: Decimal::ZERO,
                    timezone: d.timezone,
                })),
            )))
        }
        // DateTime → gYearMonth
        (XmlValueKind::Atomic(XmlAtomicValue::DateTime(dt)), XmlTypeCode::GYearMonth) => {
            Some(Ok(XmlValue::new(
                XmlTypeCode::GYearMonth,
                XmlValueKind::Atomic(XmlAtomicValue::GYearMonth(GYearMonthValue {
                    year: dt.year,
                    month: dt.month,
                    timezone: dt.timezone,
                })),
            )))
        }
        // DateTime → gYear
        (XmlValueKind::Atomic(XmlAtomicValue::DateTime(dt)), XmlTypeCode::GYear) => {
            Some(Ok(XmlValue::new(
                XmlTypeCode::GYear,
                XmlValueKind::Atomic(XmlAtomicValue::GYear(GYearValue {
                    year: dt.year,
                    timezone: dt.timezone,
                })),
            )))
        }
        // DateTime → gMonthDay
        (XmlValueKind::Atomic(XmlAtomicValue::DateTime(dt)), XmlTypeCode::GMonthDay) => {
            Some(Ok(XmlValue::new(
                XmlTypeCode::GMonthDay,
                XmlValueKind::Atomic(XmlAtomicValue::GMonthDay(GMonthDayValue {
                    month: dt.month,
                    day: dt.day,
                    timezone: dt.timezone,
                })),
            )))
        }
        // DateTime → gDay
        (XmlValueKind::Atomic(XmlAtomicValue::DateTime(dt)), XmlTypeCode::GDay) => {
            Some(Ok(XmlValue::new(
                XmlTypeCode::GDay,
                XmlValueKind::Atomic(XmlAtomicValue::GDay(GDayValue {
                    day: dt.day,
                    timezone: dt.timezone,
                })),
            )))
        }
        // DateTime → gMonth
        (XmlValueKind::Atomic(XmlAtomicValue::DateTime(dt)), XmlTypeCode::GMonth) => {
            Some(Ok(XmlValue::new(
                XmlTypeCode::GMonth,
                XmlValueKind::Atomic(XmlAtomicValue::GMonth(GMonthValue {
                    month: dt.month,
                    timezone: dt.timezone,
                })),
            )))
        }
        // Date → gYearMonth
        (XmlValueKind::Atomic(XmlAtomicValue::Date(d)), XmlTypeCode::GYearMonth) => {
            Some(Ok(XmlValue::new(
                XmlTypeCode::GYearMonth,
                XmlValueKind::Atomic(XmlAtomicValue::GYearMonth(GYearMonthValue {
                    year: d.year,
                    month: d.month,
                    timezone: d.timezone,
                })),
            )))
        }
        // Date → gYear
        (XmlValueKind::Atomic(XmlAtomicValue::Date(d)), XmlTypeCode::GYear) => {
            Some(Ok(XmlValue::new(
                XmlTypeCode::GYear,
                XmlValueKind::Atomic(XmlAtomicValue::GYear(GYearValue {
                    year: d.year,
                    timezone: d.timezone,
                })),
            )))
        }
        // Date → gMonthDay
        (XmlValueKind::Atomic(XmlAtomicValue::Date(d)), XmlTypeCode::GMonthDay) => {
            Some(Ok(XmlValue::new(
                XmlTypeCode::GMonthDay,
                XmlValueKind::Atomic(XmlAtomicValue::GMonthDay(GMonthDayValue {
                    month: d.month,
                    day: d.day,
                    timezone: d.timezone,
                })),
            )))
        }
        // Date → gDay
        (XmlValueKind::Atomic(XmlAtomicValue::Date(d)), XmlTypeCode::GDay) => {
            Some(Ok(XmlValue::new(
                XmlTypeCode::GDay,
                XmlValueKind::Atomic(XmlAtomicValue::GDay(GDayValue {
                    day: d.day,
                    timezone: d.timezone,
                })),
            )))
        }
        // Date → gMonth
        (XmlValueKind::Atomic(XmlAtomicValue::Date(d)), XmlTypeCode::GMonth) => {
            Some(Ok(XmlValue::new(
                XmlTypeCode::GMonth,
                XmlValueKind::Atomic(XmlAtomicValue::GMonth(GMonthValue {
                    month: d.month,
                    timezone: d.timezone,
                })),
            )))
        }
        // Duration → YearMonthDuration (extract year/month parts)
        (XmlValueKind::Atomic(XmlAtomicValue::Duration(d)), XmlTypeCode::YearMonthDuration) => {
            Some(Ok(XmlValue::new(
                XmlTypeCode::YearMonthDuration,
                XmlValueKind::Atomic(XmlAtomicValue::YearMonthDuration(YearMonthDurationValue {
                    negative: d.negative,
                    years: d.years,
                    months: d.months,
                })),
            )))
        }
        // Duration → DayTimeDuration (extract day/time parts)
        (XmlValueKind::Atomic(XmlAtomicValue::Duration(d)), XmlTypeCode::DayTimeDuration) => {
            Some(Ok(XmlValue::new(
                XmlTypeCode::DayTimeDuration,
                XmlValueKind::Atomic(XmlAtomicValue::DayTimeDuration(DayTimeDurationValue {
                    negative: d.negative,
                    days: d.days,
                    hours: d.hours,
                    minutes: d.minutes,
                    seconds: d.seconds,
                })),
            )))
        }
        // YearMonthDuration → Duration
        (XmlValueKind::Atomic(XmlAtomicValue::YearMonthDuration(ym)), XmlTypeCode::Duration) => {
            Some(Ok(XmlValue::new(
                XmlTypeCode::Duration,
                XmlValueKind::Atomic(XmlAtomicValue::Duration(DurationValue {
                    negative: ym.negative,
                    years: ym.years,
                    months: ym.months,
                    days: 0,
                    hours: 0,
                    minutes: 0,
                    seconds: Decimal::ZERO,
                })),
            )))
        }
        // DayTimeDuration → Duration
        (XmlValueKind::Atomic(XmlAtomicValue::DayTimeDuration(dt)), XmlTypeCode::Duration) => {
            Some(Ok(XmlValue::new(
                XmlTypeCode::Duration,
                XmlValueKind::Atomic(XmlAtomicValue::Duration(DurationValue {
                    negative: dt.negative,
                    years: 0,
                    months: 0,
                    days: dt.days,
                    hours: dt.hours,
                    minutes: dt.minutes,
                    seconds: dt.seconds,
                })),
            )))
        }
        // YearMonthDuration → DayTimeDuration: yields zero per XPath 2.0 F&O §17.1.5.
        // Cast goes through xs:duration as intermediate; yearMonthDuration has no day/time
        // components, so extracting the day-time part always produces PT0S.
        (
            XmlValueKind::Atomic(XmlAtomicValue::YearMonthDuration(_)),
            XmlTypeCode::DayTimeDuration,
        ) => Some(Ok(XmlValue::new(
            XmlTypeCode::DayTimeDuration,
            XmlValueKind::Atomic(XmlAtomicValue::DayTimeDuration(DayTimeDurationValue {
                negative: false,
                days: 0,
                hours: 0,
                minutes: 0,
                seconds: Decimal::ZERO,
            })),
        ))),
        // DayTimeDuration → YearMonthDuration: yields zero per XPath 2.0 F&O §17.1.5.
        // Cast goes through xs:duration as intermediate; dayTimeDuration has no year/month
        // components, so extracting the year-month part always produces P0M.
        (
            XmlValueKind::Atomic(XmlAtomicValue::DayTimeDuration(_)),
            XmlTypeCode::YearMonthDuration,
        ) => Some(Ok(XmlValue::new(
            XmlTypeCode::YearMonthDuration,
            XmlValueKind::Atomic(XmlAtomicValue::YearMonthDuration(YearMonthDurationValue {
                negative: false,
                years: 0,
                months: 0,
            })),
        ))),
        _ => None,
    }
}

/// Cross-cast between binary types (base64Binary ↔ hexBinary).
fn cast_binary_cross(
    value: &XmlValue,
    target: XmlTypeCode,
) -> Option<Result<XmlValue, XPathError>> {
    match (&value.value, target) {
        (XmlValueKind::Atomic(XmlAtomicValue::Base64Binary(bytes)), XmlTypeCode::HexBinary) => {
            Some(Ok(XmlValue::new(
                XmlTypeCode::HexBinary,
                XmlValueKind::Atomic(XmlAtomicValue::HexBinary(bytes.clone())),
            )))
        }
        (XmlValueKind::Atomic(XmlAtomicValue::HexBinary(bytes)), XmlTypeCode::Base64Binary) => {
            Some(Ok(XmlValue::new(
                XmlTypeCode::Base64Binary,
                XmlValueKind::Atomic(XmlAtomicValue::Base64Binary(bytes.clone())),
            )))
        }
        _ => None,
    }
}

/// Cast a numeric value to a specific integer subtype.
///
/// This handles casting to types like xs:int, xs:short, xs:byte, etc.
/// with range checking.
pub fn cast_to_integer_subtype(
    value: &XmlValue,
    target_type: XmlTypeCode,
) -> Result<XmlValue, XPathError> {
    // First cast to integer
    let int_val = cast_to(value, XmlTypeCode::Integer)?;
    let bigint = int_val
        .as_integer()
        .ok_or_else(|| XPathError::internal("Expected integer after cast"))?;

    // Then validate range for the specific subtype. `xs:integer` has no size
    // limit here (`BigInt`), and neither have the four types whose value
    // space is unbounded on one side (Datatypes 1.1 §3.4.14, §3.4.15,
    // §3.4.20, §3.4.25): they have one bound only.
    let (min, max): (Option<i128>, Option<i128>) = match target_type {
        XmlTypeCode::Byte => (Some(i8::MIN.into()), Some(i8::MAX.into())),
        XmlTypeCode::Short => (Some(i16::MIN.into()), Some(i16::MAX.into())),
        XmlTypeCode::Int => (Some(i32::MIN.into()), Some(i32::MAX.into())),
        XmlTypeCode::Long => (Some(i64::MIN.into()), Some(i64::MAX.into())),
        XmlTypeCode::UnsignedByte => (Some(0), Some(u8::MAX.into())),
        XmlTypeCode::UnsignedShort => (Some(0), Some(u16::MAX.into())),
        XmlTypeCode::UnsignedInt => (Some(0), Some(u32::MAX.into())),
        XmlTypeCode::UnsignedLong => (Some(0), Some(u64::MAX.into())),
        XmlTypeCode::PositiveInteger => (Some(1), None),
        XmlTypeCode::NonNegativeInteger => (Some(0), None),
        XmlTypeCode::NegativeInteger => (None, Some(-1)),
        XmlTypeCode::NonPositiveInteger => (None, Some(0)),
        XmlTypeCode::Integer => return Ok(int_val),
        _ => {
            return Err(XPathError::type_mismatch(
                format!("{:?}", value.type_code),
                format!("{:?}", target_type),
            ))
        }
    };

    // Check range
    if min.is_some_and(|min| *bigint < BigInt::from(min))
        || max.is_some_and(|max| *bigint > BigInt::from(max))
    {
        return Err(XPathError::invalid_cast_value(
            bigint.to_string(),
            format!("{:?}", target_type),
        ));
    }

    Ok(XmlValue::new(
        target_type,
        XmlValueKind::Atomic(XmlAtomicValue::Integer(bigint.clone())),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cast_string_to_integer() {
        let value = XmlValue::string("42");
        let result = cast_to(&value, XmlTypeCode::Integer).unwrap();
        assert_eq!(result.type_code, XmlTypeCode::Integer);
        assert_eq!(result.as_integer().unwrap(), &BigInt::from(42));
    }

    #[test]
    fn test_cast_string_to_decimal() {
        let value = XmlValue::string("2.5");
        let result = cast_to(&value, XmlTypeCode::Decimal).unwrap();
        assert_eq!(result.type_code, XmlTypeCode::Decimal);
    }

    #[test]
    fn test_cast_string_to_boolean() {
        assert_eq!(
            cast_to(&XmlValue::string("true"), XmlTypeCode::Boolean)
                .unwrap()
                .as_boolean(),
            Some(true)
        );
        assert_eq!(
            cast_to(&XmlValue::string("false"), XmlTypeCode::Boolean)
                .unwrap()
                .as_boolean(),
            Some(false)
        );
        assert_eq!(
            cast_to(&XmlValue::string("1"), XmlTypeCode::Boolean)
                .unwrap()
                .as_boolean(),
            Some(true)
        );
        assert_eq!(
            cast_to(&XmlValue::string("0"), XmlTypeCode::Boolean)
                .unwrap()
                .as_boolean(),
            Some(false)
        );
    }

    #[test]
    fn test_cast_invalid_string_to_boolean() {
        let result = cast_to(&XmlValue::string("yes"), XmlTypeCode::Boolean);
        assert!(result.is_err());
    }

    #[test]
    fn test_cast_integer_to_double() {
        let value = XmlValue::integer(BigInt::from(42));
        let result = cast_to(&value, XmlTypeCode::Double).unwrap();
        assert_eq!(result.as_double(), Some(42.0));
    }

    #[test]
    fn test_cast_double_to_integer() {
        let value = XmlValue::double(42.7);
        let result = cast_to(&value, XmlTypeCode::Integer).unwrap();
        assert_eq!(result.as_integer().unwrap(), &BigInt::from(42)); // Truncated
    }

    #[test]
    fn test_cast_nan_to_integer_fails() {
        let value = XmlValue::double(f64::NAN);
        let result = cast_to(&value, XmlTypeCode::Integer);
        assert!(result.is_err());
    }

    #[test]
    fn test_cast_inf_to_decimal_fails() {
        let value = XmlValue::double(f64::INFINITY);
        let result = cast_to(&value, XmlTypeCode::Decimal);
        assert!(result.is_err());
    }

    #[test]
    fn test_cast_same_type() {
        let value = XmlValue::string("hello");
        let result = cast_to(&value, XmlTypeCode::String).unwrap();
        assert_eq!(result.to_string_value(), "hello");
    }

    #[test]
    fn test_instance_of() {
        assert!(instance_of(&XmlValue::string("test"), XmlTypeCode::String));
        assert!(instance_of(
            &XmlValue::integer(BigInt::from(1)),
            XmlTypeCode::Integer
        ));
        assert!(!instance_of(
            &XmlValue::string("test"),
            XmlTypeCode::Integer
        ));

        // anyAtomicType should match any atomic
        assert!(instance_of(
            &XmlValue::string("test"),
            XmlTypeCode::AnyAtomicType
        ));
    }

    #[test]
    fn test_castable() {
        assert!(castable(&XmlValue::string("42"), XmlTypeCode::Integer));
        assert!(!castable(
            &XmlValue::string("not a number"),
            XmlTypeCode::Integer
        ));
    }

    #[test]
    fn test_treat_as_matching() {
        let value = XmlValue::string("test");
        let result = treat_as(&value, XmlTypeCode::String);
        assert!(result.is_ok());
    }

    #[test]
    fn test_treat_as_non_matching() {
        let value = XmlValue::string("test");
        let result = treat_as(&value, XmlTypeCode::Integer);
        assert!(result.is_err());
    }

    #[test]
    fn test_cast_special_float_values() {
        let inf = XmlValue::string("INF");
        let result = cast_to(&inf, XmlTypeCode::Float).unwrap();
        assert!(result.as_double().unwrap().is_infinite());

        let nan = XmlValue::string("NaN");
        let result = cast_to(&nan, XmlTypeCode::Double).unwrap();
        assert!(result.as_double().unwrap().is_nan());
    }

    #[test]
    fn test_cast_to_integer_subtype() {
        let value = XmlValue::string("100");

        // Should succeed for byte
        let result = cast_to_integer_subtype(&value, XmlTypeCode::Byte).unwrap();
        assert_eq!(result.type_code, XmlTypeCode::Byte);

        // Should fail for byte (out of range)
        let big = XmlValue::string("500");
        let result = cast_to_integer_subtype(&big, XmlTypeCode::Byte);
        assert!(result.is_err());
    }

    #[test]
    fn test_cast_numeric_to_boolean() {
        // Integer non-zero → true
        assert_eq!(
            cast_to(&XmlValue::integer(BigInt::from(10)), XmlTypeCode::Boolean)
                .unwrap()
                .as_boolean(),
            Some(true)
        );
        // Integer zero → false
        assert_eq!(
            cast_to(&XmlValue::integer(BigInt::from(0)), XmlTypeCode::Boolean)
                .unwrap()
                .as_boolean(),
            Some(false)
        );
        // Double NaN → false
        assert_eq!(
            cast_to(&XmlValue::double(f64::NAN), XmlTypeCode::Boolean)
                .unwrap()
                .as_boolean(),
            Some(false)
        );
        // Double -0 → false
        assert_eq!(
            cast_to(&XmlValue::double(-0.0), XmlTypeCode::Boolean)
                .unwrap()
                .as_boolean(),
            Some(false)
        );
        // Float non-zero → true
        assert_eq!(
            cast_to(&XmlValue::float(1.5), XmlTypeCode::Boolean)
                .unwrap()
                .as_boolean(),
            Some(true)
        );
        // Decimal → true
        assert_eq!(
            cast_to(
                &XmlValue::decimal(Decimal::new(-11234, 4)),
                XmlTypeCode::Boolean
            )
            .unwrap()
            .as_boolean(),
            Some(true)
        );
    }

    #[test]
    fn test_cast_datetime_to_date() {
        let dt = XmlValue::new(
            XmlTypeCode::DateTime,
            XmlValueKind::Atomic(XmlAtomicValue::DateTime(DateTimeValue {
                year: 1999,
                month: 5,
                day: 31,
                hour: 13,
                minute: 20,
                second: Decimal::ZERO,
                timezone: Some(crate::types::value::TimezoneOffset(-300)),
            })),
        );
        let result = cast_to(&dt, XmlTypeCode::Date).unwrap();
        assert_eq!(result.type_code, XmlTypeCode::Date);
        assert_eq!(result.to_string_value(), "1999-05-31-05:00");
    }

    #[test]
    fn test_cast_datetime_to_time() {
        let dt = XmlValue::new(
            XmlTypeCode::DateTime,
            XmlValueKind::Atomic(XmlAtomicValue::DateTime(DateTimeValue {
                year: 1999,
                month: 5,
                day: 31,
                hour: 13,
                minute: 20,
                second: Decimal::ZERO,
                timezone: Some(crate::types::value::TimezoneOffset(-300)),
            })),
        );
        let result = cast_to(&dt, XmlTypeCode::Time).unwrap();
        assert_eq!(result.type_code, XmlTypeCode::Time);
        assert_eq!(result.to_string_value(), "13:20:00-05:00");
    }

    #[test]
    fn test_cast_date_to_datetime() {
        let d = XmlValue::new(
            XmlTypeCode::Date,
            XmlValueKind::Atomic(XmlAtomicValue::Date(DateValue {
                year: 1999,
                month: 5,
                day: 31,
                timezone: Some(crate::types::value::TimezoneOffset::UTC),
            })),
        );
        let result = cast_to(&d, XmlTypeCode::DateTime).unwrap();
        assert_eq!(result.type_code, XmlTypeCode::DateTime);
        assert_eq!(result.to_string_value(), "1999-05-31T00:00:00Z");
    }

    #[test]
    fn test_cast_datetime_to_gyear() {
        let dt = XmlValue::new(
            XmlTypeCode::DateTime,
            XmlValueKind::Atomic(XmlAtomicValue::DateTime(DateTimeValue {
                year: 1999,
                month: 5,
                day: 31,
                hour: 13,
                minute: 20,
                second: Decimal::ZERO,
                timezone: None,
            })),
        );
        let result = cast_to(&dt, XmlTypeCode::GYear).unwrap();
        assert_eq!(result.to_string_value(), "1999");

        let result = cast_to(&dt, XmlTypeCode::GMonth).unwrap();
        assert_eq!(result.to_string_value(), "--05");

        let result = cast_to(&dt, XmlTypeCode::GDay).unwrap();
        assert_eq!(result.to_string_value(), "---31");

        let result = cast_to(&dt, XmlTypeCode::GMonthDay).unwrap();
        assert_eq!(result.to_string_value(), "--05-31");

        let result = cast_to(&dt, XmlTypeCode::GYearMonth).unwrap();
        assert_eq!(result.to_string_value(), "1999-05");
    }

    #[test]
    fn test_cast_duration_to_yearmonth() {
        let dur = XmlValue::new(
            XmlTypeCode::Duration,
            XmlValueKind::Atomic(XmlAtomicValue::Duration(DurationValue {
                negative: false,
                years: 1,
                months: 2,
                days: 3,
                hours: 10,
                minutes: 30,
                seconds: Decimal::new(23, 0),
            })),
        );
        let result = cast_to(&dur, XmlTypeCode::YearMonthDuration).unwrap();
        assert_eq!(result.to_string_value(), "P1Y2M");

        let result = cast_to(&dur, XmlTypeCode::DayTimeDuration).unwrap();
        assert_eq!(result.to_string_value(), "P3DT10H30M23S");
    }

    #[test]
    fn test_cast_binary_cross() {
        // base64Binary → hexBinary
        let b64 = XmlValue::new(
            XmlTypeCode::Base64Binary,
            XmlValueKind::Atomic(XmlAtomicValue::Base64Binary(vec![0xAB, 0xCD])),
        );
        let result = cast_to(&b64, XmlTypeCode::HexBinary).unwrap();
        assert_eq!(result.type_code, XmlTypeCode::HexBinary);
        assert_eq!(result.to_string_value(), "ABCD");

        // hexBinary → base64Binary
        let hex = XmlValue::new(
            XmlTypeCode::HexBinary,
            XmlValueKind::Atomic(XmlAtomicValue::HexBinary(vec![0xFF, 0x00])),
        );
        let result = cast_to(&hex, XmlTypeCode::Base64Binary).unwrap();
        assert_eq!(result.type_code, XmlTypeCode::Base64Binary);
    }

    #[test]
    fn test_cast_string_to_time_with_timezone() {
        let value = XmlValue::string("13:20:00-05:00");
        let result = cast_to(&value, XmlTypeCode::Time).unwrap();
        assert_eq!(result.type_code, XmlTypeCode::Time);
        assert_eq!(result.to_string_value(), "13:20:00-05:00");
    }

    #[test]
    fn test_cast_string_to_yearmonth_duration_zero() {
        let value = XmlValue::string("P0Y0M");
        let result = cast_to(&value, XmlTypeCode::YearMonthDuration).unwrap();
        assert_eq!(result.to_string_value(), "P0M");
    }

    // =========================================================================
    // SequenceType matching over the built-in atomic type hierarchy
    // =========================================================================

    /// Every built-in atomic type, with its expected base type. This is the
    /// XSD Datatypes §3 hierarchy plus the two types XPath 2.0 adds to it
    /// (`xs:untypedAtomic` under `xs:anyAtomicType`, and `xs:anyAtomicType`
    /// itself as the root).
    const ATOMIC_HIERARCHY: &[(XmlTypeCode, Option<XmlTypeCode>)] = &[
        (XmlTypeCode::UntypedAtomic, Some(XmlTypeCode::AnyAtomicType)),
        (XmlTypeCode::String, Some(XmlTypeCode::AnyAtomicType)),
        (XmlTypeCode::Boolean, Some(XmlTypeCode::AnyAtomicType)),
        (XmlTypeCode::Decimal, Some(XmlTypeCode::AnyAtomicType)),
        (XmlTypeCode::Float, Some(XmlTypeCode::AnyAtomicType)),
        (XmlTypeCode::Double, Some(XmlTypeCode::AnyAtomicType)),
        (XmlTypeCode::Duration, Some(XmlTypeCode::AnyAtomicType)),
        (XmlTypeCode::DateTime, Some(XmlTypeCode::AnyAtomicType)),
        (XmlTypeCode::Time, Some(XmlTypeCode::AnyAtomicType)),
        (XmlTypeCode::Date, Some(XmlTypeCode::AnyAtomicType)),
        (XmlTypeCode::GYearMonth, Some(XmlTypeCode::AnyAtomicType)),
        (XmlTypeCode::GYear, Some(XmlTypeCode::AnyAtomicType)),
        (XmlTypeCode::GMonthDay, Some(XmlTypeCode::AnyAtomicType)),
        (XmlTypeCode::GDay, Some(XmlTypeCode::AnyAtomicType)),
        (XmlTypeCode::GMonth, Some(XmlTypeCode::AnyAtomicType)),
        (XmlTypeCode::HexBinary, Some(XmlTypeCode::AnyAtomicType)),
        (XmlTypeCode::Base64Binary, Some(XmlTypeCode::AnyAtomicType)),
        (XmlTypeCode::AnyUri, Some(XmlTypeCode::AnyAtomicType)),
        (XmlTypeCode::QName, Some(XmlTypeCode::AnyAtomicType)),
        (XmlTypeCode::Notation, Some(XmlTypeCode::AnyAtomicType)),
        (XmlTypeCode::NormalizedString, Some(XmlTypeCode::String)),
        (XmlTypeCode::Token, Some(XmlTypeCode::NormalizedString)),
        (XmlTypeCode::Language, Some(XmlTypeCode::Token)),
        (XmlTypeCode::NmToken, Some(XmlTypeCode::Token)),
        (XmlTypeCode::Name, Some(XmlTypeCode::Token)),
        (XmlTypeCode::NCName, Some(XmlTypeCode::Name)),
        (XmlTypeCode::Id, Some(XmlTypeCode::NCName)),
        (XmlTypeCode::IdRef, Some(XmlTypeCode::NCName)),
        (XmlTypeCode::Entity, Some(XmlTypeCode::NCName)),
        (XmlTypeCode::Integer, Some(XmlTypeCode::Decimal)),
        (XmlTypeCode::NonPositiveInteger, Some(XmlTypeCode::Integer)),
        (
            XmlTypeCode::NegativeInteger,
            Some(XmlTypeCode::NonPositiveInteger),
        ),
        (XmlTypeCode::Long, Some(XmlTypeCode::Integer)),
        (XmlTypeCode::Int, Some(XmlTypeCode::Long)),
        (XmlTypeCode::Short, Some(XmlTypeCode::Int)),
        (XmlTypeCode::Byte, Some(XmlTypeCode::Short)),
        (XmlTypeCode::NonNegativeInteger, Some(XmlTypeCode::Integer)),
        (
            XmlTypeCode::UnsignedLong,
            Some(XmlTypeCode::NonNegativeInteger),
        ),
        (XmlTypeCode::UnsignedInt, Some(XmlTypeCode::UnsignedLong)),
        (XmlTypeCode::UnsignedShort, Some(XmlTypeCode::UnsignedInt)),
        (XmlTypeCode::UnsignedByte, Some(XmlTypeCode::UnsignedShort)),
        (
            XmlTypeCode::PositiveInteger,
            Some(XmlTypeCode::NonNegativeInteger),
        ),
        (XmlTypeCode::YearMonthDuration, Some(XmlTypeCode::Duration)),
        (XmlTypeCode::DayTimeDuration, Some(XmlTypeCode::Duration)),
        (XmlTypeCode::DateTimeStamp, Some(XmlTypeCode::DateTime)),
        (XmlTypeCode::AnyAtomicType, None),
    ];

    /// The transitive closure of a type's base chain, including the type itself.
    fn ancestors(code: XmlTypeCode) -> Vec<XmlTypeCode> {
        let mut chain = vec![code];
        let mut current = code;
        loop {
            let base = ATOMIC_HIERARCHY
                .iter()
                .find(|(c, _)| *c == current)
                .and_then(|(_, b)| *b);
            match base {
                Some(b) => {
                    chain.push(b);
                    current = b;
                }
                None => return chain,
            }
        }
    }

    /// `type_matches(source, target)` must be exactly "source is target, or is
    /// derived from target by restriction", for every pair of built-in atomic
    /// types.
    #[test]
    fn type_matches_is_the_builtin_atomic_hierarchy() {
        for (source, _) in ATOMIC_HIERARCHY {
            let expected = ancestors(*source);
            for (target, _) in ATOMIC_HIERARCHY {
                // xs:anyAtomicType is abstract: no value has it as its type, so
                // it is only ever a target here.
                if *source == XmlTypeCode::AnyAtomicType && *target != XmlTypeCode::AnyAtomicType {
                    continue;
                }
                let want = expected.contains(target);
                assert_eq!(
                    type_matches(*source, *target),
                    want,
                    "type_matches({source:?}, {target:?}) should be {want}"
                );
            }
        }
    }

    /// The two entries the old hand-written table got wrong.
    #[test]
    fn untyped_atomic_is_not_a_string_and_durations_are_durations() {
        // XDM: xs:untypedAtomic derives from xs:anyAtomicType, not xs:string.
        assert!(!type_matches(
            XmlTypeCode::UntypedAtomic,
            XmlTypeCode::String
        ));
        assert!(!type_matches(
            XmlTypeCode::UntypedAtomic,
            XmlTypeCode::NormalizedString
        ));
        assert!(type_matches(
            XmlTypeCode::UntypedAtomic,
            XmlTypeCode::AnyAtomicType
        ));
        // …and no string type is an xs:untypedAtomic either.
        assert!(!type_matches(
            XmlTypeCode::String,
            XmlTypeCode::UntypedAtomic
        ));

        // XSD Datatypes §3.4.26/§3.4.27.
        assert!(type_matches(
            XmlTypeCode::DayTimeDuration,
            XmlTypeCode::Duration
        ));
        assert!(type_matches(
            XmlTypeCode::YearMonthDuration,
            XmlTypeCode::Duration
        ));
        assert!(!type_matches(
            XmlTypeCode::Duration,
            XmlTypeCode::DayTimeDuration
        ));
        assert!(!type_matches(
            XmlTypeCode::DayTimeDuration,
            XmlTypeCode::YearMonthDuration
        ));
    }

    /// `item()` still matches everything, and a node type never enters the
    /// atomic hierarchy.
    #[test]
    fn type_matches_outside_the_atomic_hierarchy() {
        for code in [
            XmlTypeCode::Element,
            XmlTypeCode::Attribute,
            XmlTypeCode::Document,
            XmlTypeCode::String,
            XmlTypeCode::UntypedAtomic,
        ] {
            assert!(type_matches(code, XmlTypeCode::Item));
        }
        assert!(!type_matches(XmlTypeCode::Element, XmlTypeCode::String));
        assert!(!type_matches(
            XmlTypeCode::Element,
            XmlTypeCode::AnyAtomicType
        ));
        // The list types are not atomic.
        assert!(!type_matches(
            XmlTypeCode::IdRefs,
            XmlTypeCode::AnyAtomicType
        ));
        assert!(!type_matches(XmlTypeCode::IdRefs, XmlTypeCode::IdRef));
    }
}
