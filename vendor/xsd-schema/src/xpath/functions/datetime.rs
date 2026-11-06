//! XPath 2.0 datetime functions.
//!
//! This module implements datetime/duration functions from the XPath 2.0 specification:
//! - Current time functions (current-dateTime, current-date, current-time, implicit-timezone)
//! - Duration component extraction (years/months/days/hours/minutes/seconds-from-duration)
//! - DateTime component extraction (year/month/day/hours/minutes/seconds/timezone-from-dateTime)
//! - Date component extraction (year/month/day/timezone-from-date)
//! - Time component extraction (hours/minutes/seconds/timezone-from-time)
//! - DateTime constructor (fn:dateTime)
//! - Timezone adjustment (adjust-dateTime/date/time-to-timezone)

use num_bigint::BigInt;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;

use crate::types::value::{
    DateTimeValue, DateValue, DayTimeDurationValue, DurationValue, TimeValue, TimezoneOffset,
    XmlAtomicValue, XmlValue, XmlValueKind, YearMonthDurationValue,
};
use crate::types::XmlTypeCode;
use crate::xpath::context::DynamicContext;
use crate::xpath::error::XPathError;
use crate::xpath::DomNavigator;

use super::{atomize_to_single_opt, convert, XPathValue};

// ============================================================================
// Helper Functions
// ============================================================================

/// Convert a TimezoneOffset to a DayTimeDurationValue.
fn timezone_to_day_time_duration(tz: TimezoneOffset) -> DayTimeDurationValue {
    let total_minutes = tz.0.unsigned_abs() as u32;
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;
    DayTimeDurationValue {
        negative: tz.0 < 0,
        days: 0,
        hours,
        minutes,
        seconds: Decimal::ZERO,
    }
}

/// Extract a DayTimeDurationValue as a TimezoneOffset.
/// Returns FODT0003 if:
/// - The offset is outside the valid range (+-14:00)
/// - The duration has non-zero day components
/// - The duration has non-zero or fractional seconds
fn day_time_duration_to_timezone(dur: &DayTimeDurationValue) -> Result<TimezoneOffset, XPathError> {
    // XPath 2.0 spec: timezone must be an integral number of minutes with no days
    // and must be in the range -PT14H to PT14H inclusive.

    // Reject durations with days component
    if dur.days != 0 {
        return Err(XPathError::FODT0003 {
            value: format_day_time_duration(dur),
        });
    }

    // Reject durations with non-zero seconds (including fractional)
    if dur.seconds != Decimal::ZERO {
        return Err(XPathError::FODT0003 {
            value: format_day_time_duration(dur),
        });
    }

    // Calculate total minutes (now we know there are no days or seconds)
    let total_minutes = dur.hours as i64 * 60 + dur.minutes as i64;
    let signed_minutes = if dur.negative {
        -total_minutes
    } else {
        total_minutes
    };

    // Validate range: must be within +-14:00 (+-840 minutes)
    if !(-840..=840).contains(&signed_minutes) {
        return Err(XPathError::FODT0003 {
            value: format_day_time_duration(dur),
        });
    }

    Ok(TimezoneOffset(signed_minutes as i16))
}

/// Format a DayTimeDurationValue for error messages.
fn format_day_time_duration(dur: &DayTimeDurationValue) -> String {
    let mut s = String::new();
    if dur.negative {
        s.push('-');
    }
    s.push_str("PT");
    if dur.days != 0 {
        s.push_str(&format!("{}D", dur.days));
    }
    if dur.hours != 0 {
        s.push_str(&format!("{}H", dur.hours));
    }
    if dur.minutes != 0 {
        s.push_str(&format!("{}M", dur.minutes));
    }
    if dur.seconds != Decimal::ZERO {
        s.push_str(&format!("{}S", dur.seconds));
    }
    if s.len() == 2 || (s.len() == 3 && s.starts_with('-')) {
        s.push_str("0S");
    }
    s
}

/// Validate that a timezone offset is in the valid range (+-14:00).
fn validate_timezone_offset(minutes: i16) -> Result<(), XPathError> {
    if !(-840..=840).contains(&minutes) {
        return Err(XPathError::FODT0003 {
            value: format!("{}:{:02}", minutes / 60, (minutes % 60).abs()),
        });
    }
    Ok(())
}

/// Extract datetime value from XmlValue.
fn as_datetime(value: &XmlValue) -> Option<&DateTimeValue> {
    match &value.value {
        XmlValueKind::Atomic(XmlAtomicValue::DateTime(v)) => Some(v),
        _ => None,
    }
}

/// Extract date value from XmlValue.
fn as_date(value: &XmlValue) -> Option<&DateValue> {
    match &value.value {
        XmlValueKind::Atomic(XmlAtomicValue::Date(v)) => Some(v),
        _ => None,
    }
}

/// Extract time value from XmlValue.
fn as_time(value: &XmlValue) -> Option<&TimeValue> {
    match &value.value {
        XmlValueKind::Atomic(XmlAtomicValue::Time(v)) => Some(v),
        _ => None,
    }
}

/// Extract duration value from XmlValue (xs:duration).
fn as_duration(value: &XmlValue) -> Option<&DurationValue> {
    match &value.value {
        XmlValueKind::Atomic(XmlAtomicValue::Duration(v)) => Some(v),
        _ => None,
    }
}

/// Extract yearMonthDuration value from XmlValue.
fn as_year_month_duration(value: &XmlValue) -> Option<&YearMonthDurationValue> {
    match &value.value {
        XmlValueKind::Atomic(XmlAtomicValue::YearMonthDuration(v)) => Some(v),
        _ => None,
    }
}

/// Extract dayTimeDuration value from XmlValue.
fn as_day_time_duration(value: &XmlValue) -> Option<&DayTimeDurationValue> {
    match &value.value {
        XmlValueKind::Atomic(XmlAtomicValue::DayTimeDuration(v)) => Some(v),
        _ => None,
    }
}

/// Apply the function conversion rules (XPath 2.0 §3.1.5) for an `xs:dateTime`
/// parameter and hand back the value's dateTime payload.
fn require_datetime(value: XmlValue, function: &str) -> Result<DateTimeValue, XPathError> {
    let value = convert::expect_atomic_as(value, XmlTypeCode::DateTime, function)?;
    as_datetime(&value)
        .cloned()
        .ok_or_else(|| XPathError::internal("xs:dateTime without a dateTime payload"))
}

/// Apply the function conversion rules (XPath 2.0 §3.1.5) for an `xs:date`
/// parameter and hand back the value's date payload.
fn require_date(value: XmlValue, function: &str) -> Result<DateValue, XPathError> {
    let value = convert::expect_atomic_as(value, XmlTypeCode::Date, function)?;
    as_date(&value)
        .cloned()
        .ok_or_else(|| XPathError::internal("xs:date without a date payload"))
}

/// Apply the function conversion rules (XPath 2.0 §3.1.5) for an `xs:time`
/// parameter and hand back the value's time payload.
fn require_time(value: XmlValue, function: &str) -> Result<TimeValue, XPathError> {
    let value = convert::expect_atomic_as(value, XmlTypeCode::Time, function)?;
    as_time(&value)
        .cloned()
        .ok_or_else(|| XPathError::internal("xs:time without a time payload"))
}

/// Apply the function conversion rules (XPath 2.0 §3.1.5) for an
/// `xs:dayTimeDuration` parameter and hand back the value's duration payload.
fn require_day_time_duration(
    value: XmlValue,
    function: &str,
) -> Result<DayTimeDurationValue, XPathError> {
    let value = convert::expect_atomic_as(value, XmlTypeCode::DayTimeDuration, function)?;
    as_day_time_duration(&value)
        .cloned()
        .ok_or_else(|| XPathError::internal("xs:dayTimeDuration without a duration payload"))
}

/// Create an XmlValue containing an integer.
fn xml_integer(i: i64) -> XmlValue {
    XmlValue {
        type_code: XmlTypeCode::Integer,
        schema_type: None,
        value: XmlValueKind::Atomic(XmlAtomicValue::Integer(BigInt::from(i))),
    }
}

/// Create an XmlValue containing a decimal.
fn xml_decimal(d: Decimal) -> XmlValue {
    XmlValue {
        type_code: XmlTypeCode::Decimal,
        schema_type: None,
        // The seconds of a negative duration with no seconds are 0, not -0.
        value: XmlValueKind::Atomic(XmlAtomicValue::Decimal(
            crate::types::exact_decimal::unsigned_zero(d),
        )),
    }
}

/// Create an XmlValue containing a dayTimeDuration.
fn xml_day_time_duration(value: DayTimeDurationValue) -> XmlValue {
    XmlValue {
        type_code: XmlTypeCode::DayTimeDuration,
        schema_type: None,
        value: XmlValueKind::Atomic(XmlAtomicValue::DayTimeDuration(value)),
    }
}

/// Create an XmlValue containing a dateTime.
fn xml_datetime(value: DateTimeValue) -> XmlValue {
    XmlValue {
        type_code: XmlTypeCode::DateTime,
        schema_type: None,
        value: XmlValueKind::Atomic(XmlAtomicValue::DateTime(value)),
    }
}

/// Create an XmlValue containing a date.
fn xml_date(value: DateValue) -> XmlValue {
    XmlValue {
        type_code: XmlTypeCode::Date,
        schema_type: None,
        value: XmlValueKind::Atomic(XmlAtomicValue::Date(value)),
    }
}

/// Create an XmlValue containing a time.
fn xml_time(value: TimeValue) -> XmlValue {
    XmlValue {
        type_code: XmlTypeCode::Time,
        schema_type: None,
        value: XmlValueKind::Atomic(XmlAtomicValue::Time(value)),
    }
}

// ============================================================================
// A. Current Time Functions (4 functions)
// ============================================================================

/// Implements fn:current-dateTime() - returns the current date and time.
///
/// F&O 1.0 §16.3: "Returns the current dateTime (with timezone) from the
/// dynamic context … This function is ·stable·." The dynamic context's
/// [`current_datetime`](DynamicContext::current_datetime) when it is set;
/// otherwise the current instant — read once per dynamic context, with the
/// fractional seconds of the system clock — expressed in the implicit
/// timezone ([`DynamicContext::implicit_timezone_or_host`]), and kept in
/// `current_datetime` for the rest of the evaluation.
pub fn current_datetime<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    use chrono::{Datelike, Timelike};

    if !args.is_empty() {
        return Err(XPathError::wrong_number_of_arguments(
            "current-dateTime",
            0,
            args.len(),
        ));
    }

    // Use cached value or create new one
    let dt = if let Some(ref cached) = context.current_datetime {
        cached.clone()
    } else {
        let tz_offset = context.implicit_timezone_or_host();
        // The instant, then its local time in the implicit timezone — not the
        // host's wall-clock time with the implicit timezone's label.
        let local = context.host_clock().now.naive_utc()
            + chrono::Duration::minutes(i64::from(tz_offset.0));
        // A leap second reads as the last nanosecond of its minute.
        let nanos = local.nanosecond().min(999_999_999);
        let second = Decimal::from(local.second()) + Decimal::new(i64::from(nanos), 9);

        let dt = DateTimeValue {
            year: local.year(),
            month: local.month() as u8,
            day: local.day() as u8,
            hour: local.hour() as u8,
            minute: local.minute() as u8,
            second: second.normalize(),
            timezone: Some(tz_offset),
        };
        context.current_datetime = Some(dt.clone());
        dt
    };

    Ok(XPathValue::from_atomic(xml_datetime(dt)))
}

/// Implements fn:current-date() - returns the current date.
pub fn current_date<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if !args.is_empty() {
        return Err(XPathError::wrong_number_of_arguments(
            "current-date",
            0,
            args.len(),
        ));
    }

    // Get current-dateTime and extract date portion
    let dt_value = current_datetime(context, vec![])?;
    let dt = match dt_value {
        XPathValue::Item(item) => {
            if let crate::xpath::iterator::XmlItem::Atomic(v) = item {
                as_datetime(&v).cloned()
            } else {
                None
            }
        }
        _ => None,
    };

    let dt = dt.ok_or_else(|| XPathError::internal("Failed to get current dateTime"))?;

    let date = DateValue {
        year: dt.year,
        month: dt.month,
        day: dt.day,
        timezone: dt.timezone,
    };

    Ok(XPathValue::from_atomic(xml_date(date)))
}

/// Implements fn:current-time() - returns the current time.
pub fn current_time<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if !args.is_empty() {
        return Err(XPathError::wrong_number_of_arguments(
            "current-time",
            0,
            args.len(),
        ));
    }

    // Get current-dateTime and extract time portion
    let dt_value = current_datetime(context, vec![])?;
    let dt = match dt_value {
        XPathValue::Item(item) => {
            if let crate::xpath::iterator::XmlItem::Atomic(v) = item {
                as_datetime(&v).cloned()
            } else {
                None
            }
        }
        _ => None,
    };

    let dt = dt.ok_or_else(|| XPathError::internal("Failed to get current dateTime"))?;

    let time = TimeValue {
        hour: dt.hour,
        minute: dt.minute,
        second: dt.second,
        timezone: dt.timezone,
    };

    Ok(XPathValue::from_atomic(xml_time(time)))
}

/// Implements fn:implicit-timezone() - returns the implicit timezone as a dayTimeDuration.
pub fn implicit_timezone<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if !args.is_empty() {
        return Err(XPathError::wrong_number_of_arguments(
            "implicit-timezone",
            0,
            args.len(),
        ));
    }

    let tz = context.implicit_timezone_or_host();

    let duration = timezone_to_day_time_duration(tz);
    Ok(XPathValue::from_atomic(xml_day_time_duration(duration)))
}

// ============================================================================
// B. Duration Component Extraction (6 functions)
// ============================================================================

/// Helper: Normalize year-month duration to total months, then extract years component.
/// The years-from-duration function computes total_months / 12 for normalized result.
fn normalized_years_from_ym(years: u32, months: u32, negative: bool) -> i64 {
    let total_months = years as i64 * 12 + months as i64;
    let years_component = total_months / 12;
    if negative {
        -years_component
    } else {
        years_component
    }
}

/// Helper: Normalize year-month duration to total months, then extract months component.
/// The months-from-duration function computes total_months % 12 for normalized result.
fn normalized_months_from_ym(years: u32, months: u32, negative: bool) -> i64 {
    let total_months = years as i64 * 12 + months as i64;
    let months_component = total_months % 12;
    if negative {
        -months_component
    } else {
        months_component
    }
}

/// Helper: Normalize day-time duration to total seconds, then extract days component.
fn normalized_days_from_dt(
    days: u32,
    hours: u32,
    minutes: u32,
    seconds: Decimal,
    negative: bool,
) -> i64 {
    let total_seconds = Decimal::from(days) * Decimal::from(86400)
        + Decimal::from(hours) * Decimal::from(3600)
        + Decimal::from(minutes) * Decimal::from(60)
        + seconds;
    // Integer division for days
    let days_component = (total_seconds / Decimal::from(86400))
        .floor()
        .to_i64()
        .unwrap_or(0);
    if negative {
        -days_component
    } else {
        days_component
    }
}

/// Helper: Normalize day-time duration to total seconds, then extract hours component (0-23).
fn normalized_hours_from_dt(
    days: u32,
    hours: u32,
    minutes: u32,
    seconds: Decimal,
    negative: bool,
) -> i64 {
    let total_seconds = Decimal::from(days) * Decimal::from(86400)
        + Decimal::from(hours) * Decimal::from(3600)
        + Decimal::from(minutes) * Decimal::from(60)
        + seconds;
    // Remainder after removing days, then integer divide by 3600
    let remainder_after_days = total_seconds % Decimal::from(86400);
    let hours_component = (remainder_after_days / Decimal::from(3600))
        .floor()
        .to_i64()
        .unwrap_or(0);
    if negative {
        -hours_component
    } else {
        hours_component
    }
}

/// Helper: Normalize day-time duration to total seconds, then extract minutes component (0-59).
fn normalized_minutes_from_dt(
    days: u32,
    hours: u32,
    minutes: u32,
    seconds: Decimal,
    negative: bool,
) -> i64 {
    let total_seconds = Decimal::from(days) * Decimal::from(86400)
        + Decimal::from(hours) * Decimal::from(3600)
        + Decimal::from(minutes) * Decimal::from(60)
        + seconds;
    // Remainder after removing days and hours, then integer divide by 60
    let remainder_after_hours = total_seconds % Decimal::from(3600);
    let minutes_component = (remainder_after_hours / Decimal::from(60))
        .floor()
        .to_i64()
        .unwrap_or(0);
    if negative {
        -minutes_component
    } else {
        minutes_component
    }
}

/// Helper: Normalize day-time duration to total seconds, then extract seconds component (0-59.xxx).
fn normalized_seconds_from_dt(
    days: u32,
    hours: u32,
    minutes: u32,
    seconds: Decimal,
    negative: bool,
) -> Decimal {
    let total_seconds = Decimal::from(days) * Decimal::from(86400)
        + Decimal::from(hours) * Decimal::from(3600)
        + Decimal::from(minutes) * Decimal::from(60)
        + seconds;
    // Remainder after removing minutes (keep fractional part)
    let seconds_component = total_seconds % Decimal::from(60);
    if negative {
        -seconds_component
    } else {
        seconds_component
    }
}

/// Implements fn:years-from-duration($arg) - extracts years from a duration.
pub fn years_from_duration<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "years-from-duration",
            1,
            args.len(),
        ));
    }

    let arg = args.remove(0);
    let value = match atomize_to_single_opt(arg)? {
        None => return Ok(XPathValue::Empty),
        Some(v) => v,
    };

    let value = convert::expect_atomic_as(value, XmlTypeCode::Duration, "years-from-duration")?;

    // Extract years component with normalization
    let result = if let Some(dur) = as_duration(&value) {
        // For xs:duration, use the year-month portion normalized
        normalized_years_from_ym(dur.years, dur.months, dur.negative)
    } else if let Some(ymd) = as_year_month_duration(&value) {
        normalized_years_from_ym(ymd.years, ymd.months, ymd.negative)
    } else if as_day_time_duration(&value).is_some() {
        // dayTimeDuration has no years component
        0
    } else {
        return Err(XPathError::internal("Unexpected duration type"));
    };

    Ok(XPathValue::from_atomic(xml_integer(result)))
}

/// Implements fn:months-from-duration($arg) - extracts months from a duration.
pub fn months_from_duration<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "months-from-duration",
            1,
            args.len(),
        ));
    }

    let arg = args.remove(0);
    let value = match atomize_to_single_opt(arg)? {
        None => return Ok(XPathValue::Empty),
        Some(v) => v,
    };

    let value = convert::expect_atomic_as(value, XmlTypeCode::Duration, "months-from-duration")?;

    // Extract months component (0-11) with normalization
    let result = if let Some(dur) = as_duration(&value) {
        normalized_months_from_ym(dur.years, dur.months, dur.negative)
    } else if let Some(ymd) = as_year_month_duration(&value) {
        normalized_months_from_ym(ymd.years, ymd.months, ymd.negative)
    } else if as_day_time_duration(&value).is_some() {
        // dayTimeDuration has no months component
        0
    } else {
        return Err(XPathError::internal("Unexpected duration type"));
    };

    Ok(XPathValue::from_atomic(xml_integer(result)))
}

/// Implements fn:days-from-duration($arg) - extracts days from a duration.
pub fn days_from_duration<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "days-from-duration",
            1,
            args.len(),
        ));
    }

    let arg = args.remove(0);
    let value = match atomize_to_single_opt(arg)? {
        None => return Ok(XPathValue::Empty),
        Some(v) => v,
    };

    let value = convert::expect_atomic_as(value, XmlTypeCode::Duration, "days-from-duration")?;

    // Extract days component from day-time portion with normalization
    let result = if let Some(dur) = as_duration(&value) {
        normalized_days_from_dt(dur.days, dur.hours, dur.minutes, dur.seconds, dur.negative)
    } else if as_year_month_duration(&value).is_some() {
        // yearMonthDuration has no days component
        0
    } else if let Some(dtd) = as_day_time_duration(&value) {
        normalized_days_from_dt(dtd.days, dtd.hours, dtd.minutes, dtd.seconds, dtd.negative)
    } else {
        return Err(XPathError::internal("Unexpected duration type"));
    };

    Ok(XPathValue::from_atomic(xml_integer(result)))
}

/// Implements fn:hours-from-duration($arg) - extracts hours from a duration.
pub fn hours_from_duration<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "hours-from-duration",
            1,
            args.len(),
        ));
    }

    let arg = args.remove(0);
    let value = match atomize_to_single_opt(arg)? {
        None => return Ok(XPathValue::Empty),
        Some(v) => v,
    };

    let value = convert::expect_atomic_as(value, XmlTypeCode::Duration, "hours-from-duration")?;

    // Extract hours component (0-23) with normalization
    let result = if let Some(dur) = as_duration(&value) {
        normalized_hours_from_dt(dur.days, dur.hours, dur.minutes, dur.seconds, dur.negative)
    } else if as_year_month_duration(&value).is_some() {
        // yearMonthDuration has no hours component
        0
    } else if let Some(dtd) = as_day_time_duration(&value) {
        normalized_hours_from_dt(dtd.days, dtd.hours, dtd.minutes, dtd.seconds, dtd.negative)
    } else {
        return Err(XPathError::internal("Unexpected duration type"));
    };

    Ok(XPathValue::from_atomic(xml_integer(result)))
}

/// Implements fn:minutes-from-duration($arg) - extracts minutes from a duration.
pub fn minutes_from_duration<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "minutes-from-duration",
            1,
            args.len(),
        ));
    }

    let arg = args.remove(0);
    let value = match atomize_to_single_opt(arg)? {
        None => return Ok(XPathValue::Empty),
        Some(v) => v,
    };

    let value = convert::expect_atomic_as(value, XmlTypeCode::Duration, "minutes-from-duration")?;

    // Extract minutes component (0-59) with normalization
    let result = if let Some(dur) = as_duration(&value) {
        normalized_minutes_from_dt(dur.days, dur.hours, dur.minutes, dur.seconds, dur.negative)
    } else if as_year_month_duration(&value).is_some() {
        // yearMonthDuration has no minutes component
        0
    } else if let Some(dtd) = as_day_time_duration(&value) {
        normalized_minutes_from_dt(dtd.days, dtd.hours, dtd.minutes, dtd.seconds, dtd.negative)
    } else {
        return Err(XPathError::internal("Unexpected duration type"));
    };

    Ok(XPathValue::from_atomic(xml_integer(result)))
}

/// Implements fn:seconds-from-duration($arg) - extracts seconds from a duration.
pub fn seconds_from_duration<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "seconds-from-duration",
            1,
            args.len(),
        ));
    }

    let arg = args.remove(0);
    let value = match atomize_to_single_opt(arg)? {
        None => return Ok(XPathValue::Empty),
        Some(v) => v,
    };

    let value = convert::expect_atomic_as(value, XmlTypeCode::Duration, "seconds-from-duration")?;

    // Extract seconds component (0-59.xxx) with normalization
    let result = if let Some(dur) = as_duration(&value) {
        normalized_seconds_from_dt(dur.days, dur.hours, dur.minutes, dur.seconds, dur.negative)
    } else if as_year_month_duration(&value).is_some() {
        // yearMonthDuration has no seconds component
        Decimal::ZERO
    } else if let Some(dtd) = as_day_time_duration(&value) {
        normalized_seconds_from_dt(dtd.days, dtd.hours, dtd.minutes, dtd.seconds, dtd.negative)
    } else {
        return Err(XPathError::internal("Unexpected duration type"));
    };

    Ok(XPathValue::from_atomic(xml_decimal(result)))
}

// ============================================================================
// C. DateTime Component Extraction (7 functions)
// ============================================================================

/// Implements fn:year-from-dateTime($arg) - extracts year from a dateTime.
pub fn year_from_datetime<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "year-from-dateTime",
            1,
            args.len(),
        ));
    }

    let arg = args.remove(0);
    let value = match atomize_to_single_opt(arg)? {
        None => return Ok(XPathValue::Empty),
        Some(v) => v,
    };

    let dt = require_datetime(value, "year-from-dateTime")?;

    Ok(XPathValue::from_atomic(xml_integer(dt.year as i64)))
}

/// Implements fn:month-from-dateTime($arg) - extracts month from a dateTime.
pub fn month_from_datetime<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "month-from-dateTime",
            1,
            args.len(),
        ));
    }

    let arg = args.remove(0);
    let value = match atomize_to_single_opt(arg)? {
        None => return Ok(XPathValue::Empty),
        Some(v) => v,
    };

    let dt = require_datetime(value, "month-from-dateTime")?;

    Ok(XPathValue::from_atomic(xml_integer(dt.month as i64)))
}

/// Implements fn:day-from-dateTime($arg) - extracts day from a dateTime.
pub fn day_from_datetime<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "day-from-dateTime",
            1,
            args.len(),
        ));
    }

    let arg = args.remove(0);
    let value = match atomize_to_single_opt(arg)? {
        None => return Ok(XPathValue::Empty),
        Some(v) => v,
    };

    let dt = require_datetime(value, "day-from-dateTime")?;

    Ok(XPathValue::from_atomic(xml_integer(dt.day as i64)))
}

/// Implements fn:hours-from-dateTime($arg) - extracts hours from a dateTime.
pub fn hours_from_datetime<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "hours-from-dateTime",
            1,
            args.len(),
        ));
    }

    let arg = args.remove(0);
    let value = match atomize_to_single_opt(arg)? {
        None => return Ok(XPathValue::Empty),
        Some(v) => v,
    };

    let dt = require_datetime(value, "hours-from-dateTime")?;

    Ok(XPathValue::from_atomic(xml_integer(dt.hour as i64)))
}

/// Implements fn:minutes-from-dateTime($arg) - extracts minutes from a dateTime.
pub fn minutes_from_datetime<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "minutes-from-dateTime",
            1,
            args.len(),
        ));
    }

    let arg = args.remove(0);
    let value = match atomize_to_single_opt(arg)? {
        None => return Ok(XPathValue::Empty),
        Some(v) => v,
    };

    let dt = require_datetime(value, "minutes-from-dateTime")?;

    Ok(XPathValue::from_atomic(xml_integer(dt.minute as i64)))
}

/// Implements fn:seconds-from-dateTime($arg) - extracts seconds from a dateTime.
pub fn seconds_from_datetime<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "seconds-from-dateTime",
            1,
            args.len(),
        ));
    }

    let arg = args.remove(0);
    let value = match atomize_to_single_opt(arg)? {
        None => return Ok(XPathValue::Empty),
        Some(v) => v,
    };

    let dt = require_datetime(value, "seconds-from-dateTime")?;

    Ok(XPathValue::from_atomic(xml_decimal(dt.second)))
}

/// Implements fn:timezone-from-dateTime($arg) - extracts timezone from a dateTime.
pub fn timezone_from_datetime<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "timezone-from-dateTime",
            1,
            args.len(),
        ));
    }

    let arg = args.remove(0);
    let value = match atomize_to_single_opt(arg)? {
        None => return Ok(XPathValue::Empty),
        Some(v) => v,
    };

    let dt = require_datetime(value, "timezone-from-dateTime")?;

    match dt.timezone {
        Some(tz) => {
            let duration = timezone_to_day_time_duration(tz);
            Ok(XPathValue::from_atomic(xml_day_time_duration(duration)))
        }
        None => Ok(XPathValue::Empty),
    }
}

// ============================================================================
// D. Date Component Extraction (4 functions)
// ============================================================================

/// Implements fn:year-from-date($arg) - extracts year from a date.
pub fn year_from_date<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "year-from-date",
            1,
            args.len(),
        ));
    }

    let arg = args.remove(0);
    let value = match atomize_to_single_opt(arg)? {
        None => return Ok(XPathValue::Empty),
        Some(v) => v,
    };

    let date = require_date(value, "year-from-date")?;

    Ok(XPathValue::from_atomic(xml_integer(date.year as i64)))
}

/// Implements fn:month-from-date($arg) - extracts month from a date.
pub fn month_from_date<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "month-from-date",
            1,
            args.len(),
        ));
    }

    let arg = args.remove(0);
    let value = match atomize_to_single_opt(arg)? {
        None => return Ok(XPathValue::Empty),
        Some(v) => v,
    };

    let date = require_date(value, "month-from-date")?;

    Ok(XPathValue::from_atomic(xml_integer(date.month as i64)))
}

/// Implements fn:day-from-date($arg) - extracts day from a date.
pub fn day_from_date<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "day-from-date",
            1,
            args.len(),
        ));
    }

    let arg = args.remove(0);
    let value = match atomize_to_single_opt(arg)? {
        None => return Ok(XPathValue::Empty),
        Some(v) => v,
    };

    let date = require_date(value, "day-from-date")?;

    Ok(XPathValue::from_atomic(xml_integer(date.day as i64)))
}

/// Implements fn:timezone-from-date($arg) - extracts timezone from a date.
pub fn timezone_from_date<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "timezone-from-date",
            1,
            args.len(),
        ));
    }

    let arg = args.remove(0);
    let value = match atomize_to_single_opt(arg)? {
        None => return Ok(XPathValue::Empty),
        Some(v) => v,
    };

    let date = require_date(value, "timezone-from-date")?;

    match date.timezone {
        Some(tz) => {
            let duration = timezone_to_day_time_duration(tz);
            Ok(XPathValue::from_atomic(xml_day_time_duration(duration)))
        }
        None => Ok(XPathValue::Empty),
    }
}

// ============================================================================
// E. Time Component Extraction (4 functions)
// ============================================================================

/// Implements fn:hours-from-time($arg) - extracts hours from a time.
pub fn hours_from_time<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "hours-from-time",
            1,
            args.len(),
        ));
    }

    let arg = args.remove(0);
    let value = match atomize_to_single_opt(arg)? {
        None => return Ok(XPathValue::Empty),
        Some(v) => v,
    };

    let time = require_time(value, "hours-from-time")?;

    Ok(XPathValue::from_atomic(xml_integer(time.hour as i64)))
}

/// Implements fn:minutes-from-time($arg) - extracts minutes from a time.
pub fn minutes_from_time<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "minutes-from-time",
            1,
            args.len(),
        ));
    }

    let arg = args.remove(0);
    let value = match atomize_to_single_opt(arg)? {
        None => return Ok(XPathValue::Empty),
        Some(v) => v,
    };

    let time = require_time(value, "minutes-from-time")?;

    Ok(XPathValue::from_atomic(xml_integer(time.minute as i64)))
}

/// Implements fn:seconds-from-time($arg) - extracts seconds from a time.
pub fn seconds_from_time<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "seconds-from-time",
            1,
            args.len(),
        ));
    }

    let arg = args.remove(0);
    let value = match atomize_to_single_opt(arg)? {
        None => return Ok(XPathValue::Empty),
        Some(v) => v,
    };

    let time = require_time(value, "seconds-from-time")?;

    Ok(XPathValue::from_atomic(xml_decimal(time.second)))
}

/// Implements fn:timezone-from-time($arg) - extracts timezone from a time.
pub fn timezone_from_time<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "timezone-from-time",
            1,
            args.len(),
        ));
    }

    let arg = args.remove(0);
    let value = match atomize_to_single_opt(arg)? {
        None => return Ok(XPathValue::Empty),
        Some(v) => v,
    };

    let time = require_time(value, "timezone-from-time")?;

    match time.timezone {
        Some(tz) => {
            let duration = timezone_to_day_time_duration(tz);
            Ok(XPathValue::from_atomic(xml_day_time_duration(duration)))
        }
        None => Ok(XPathValue::Empty),
    }
}

// ============================================================================
// F. DateTime Constructor (1 function)
// ============================================================================

/// Implements fn:dateTime($date, $time) - combines a date and time into a dateTime.
///
/// Error FORG0008 is raised if both arguments have timezones and they differ.
pub fn create_datetime<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 2 {
        return Err(XPathError::wrong_number_of_arguments(
            "dateTime",
            2,
            args.len(),
        ));
    }

    let time_arg = args.remove(1);
    let date_arg = args.remove(0);

    // Get date value
    let date_value = match atomize_to_single_opt(date_arg)? {
        None => return Ok(XPathValue::Empty),
        Some(v) => v,
    };

    // Get time value
    let time_value = match atomize_to_single_opt(time_arg)? {
        None => return Ok(XPathValue::Empty),
        Some(v) => v,
    };

    let date = require_date(date_value, "dateTime")?;

    let time = require_time(time_value, "dateTime")?;

    // Check timezone compatibility
    let timezone = match (date.timezone, time.timezone) {
        (Some(date_tz), Some(time_tz)) => {
            // Both have timezones - they must be equal
            if date_tz.0 != time_tz.0 {
                return Err(XPathError::FORG0008);
            }
            Some(date_tz)
        }
        (Some(tz), None) => Some(tz),
        (None, Some(tz)) => Some(tz),
        (None, None) => None,
    };

    // Normalize 24:00:00 to 00:00:00 (per XQTS expected behavior)
    let hour = if time.hour == 24 { 0 } else { time.hour };

    let result = DateTimeValue {
        year: date.year,
        month: date.month,
        day: date.day,
        hour,
        minute: time.minute,
        second: time.second,
        timezone,
    };

    Ok(XPathValue::from_atomic(xml_datetime(result)))
}

// ============================================================================
// G. Timezone Adjustment (3 functions with 1-arg and 2-arg variants)
// ============================================================================

/// Implements fn:adjust-dateTime-to-timezone($arg, $timezone?) - adjusts a dateTime's timezone.
pub fn adjust_datetime_to_timezone<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.is_empty() || args.len() > 2 {
        return Err(XPathError::wrong_number_of_arguments(
            "adjust-dateTime-to-timezone",
            1,
            args.len(),
        ));
    }

    // Get timezone argument (if provided)
    let tz_arg = if args.len() == 2 {
        Some(args.remove(1))
    } else {
        None
    };

    let dt_arg = args.remove(0);
    let dt_value = match atomize_to_single_opt(dt_arg)? {
        None => return Ok(XPathValue::Empty),
        Some(v) => v,
    };

    let dt = require_datetime(dt_value, "adjust-dateTime-to-timezone")?;

    // Determine target timezone
    let target_tz = if let Some(tz_val) = tz_arg {
        match atomize_to_single_opt(tz_val)? {
            None => {
                // Empty timezone argument - strip timezone
                let result = DateTimeValue {
                    year: dt.year,
                    month: dt.month,
                    day: dt.day,
                    hour: dt.hour,
                    minute: dt.minute,
                    second: dt.second,
                    timezone: None,
                };
                return Ok(XPathValue::from_atomic(xml_datetime(result)));
            }
            Some(v) => {
                let duration = require_day_time_duration(v, "adjust-dateTime-to-timezone")?;
                day_time_duration_to_timezone(&duration)?
            }
        }
    } else {
        // 1-arg form: use implicit timezone
        context.implicit_timezone_or_host()
    };

    validate_timezone_offset(target_tz.0)?;

    // Apply timezone adjustment
    let result = match dt.timezone {
        None => {
            // Input has no timezone - just attach the new one without shifting
            DateTimeValue {
                year: dt.year,
                month: dt.month,
                day: dt.day,
                hour: dt.hour,
                minute: dt.minute,
                second: dt.second,
                timezone: Some(target_tz),
            }
        }
        Some(source_tz) => {
            // Convert from source timezone to target timezone
            let offset_diff = target_tz.0 - source_tz.0;
            adjust_datetime_by_minutes(&dt, offset_diff, target_tz)?
        }
    };

    Ok(XPathValue::from_atomic(xml_datetime(result)))
}

/// Implements fn:adjust-date-to-timezone($arg, $timezone?) - adjusts a date's timezone.
pub fn adjust_date_to_timezone<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.is_empty() || args.len() > 2 {
        return Err(XPathError::wrong_number_of_arguments(
            "adjust-date-to-timezone",
            1,
            args.len(),
        ));
    }

    // Get timezone argument (if provided)
    let tz_arg = if args.len() == 2 {
        Some(args.remove(1))
    } else {
        None
    };

    let date_arg = args.remove(0);
    let date_value = match atomize_to_single_opt(date_arg)? {
        None => return Ok(XPathValue::Empty),
        Some(v) => v,
    };

    let date = require_date(date_value, "adjust-date-to-timezone")?;

    // Determine target timezone
    let target_tz = if let Some(tz_val) = tz_arg {
        match atomize_to_single_opt(tz_val)? {
            None => {
                // Empty timezone argument - strip timezone
                let result = DateValue {
                    year: date.year,
                    month: date.month,
                    day: date.day,
                    timezone: None,
                };
                return Ok(XPathValue::from_atomic(xml_date(result)));
            }
            Some(v) => {
                let duration = require_day_time_duration(v, "adjust-date-to-timezone")?;
                day_time_duration_to_timezone(&duration)?
            }
        }
    } else {
        // 1-arg form: use implicit timezone
        context.implicit_timezone_or_host()
    };

    validate_timezone_offset(target_tz.0)?;

    // Apply timezone adjustment
    let result = match date.timezone {
        None => {
            // Input has no timezone - just attach the new one without shifting
            DateValue {
                year: date.year,
                month: date.month,
                day: date.day,
                timezone: Some(target_tz),
            }
        }
        Some(source_tz) => {
            // Convert from source timezone to target timezone
            // For dates, we need to convert via dateTime at midnight
            let offset_diff = target_tz.0 - source_tz.0;
            adjust_date_by_minutes(&date, offset_diff, target_tz)?
        }
    };

    Ok(XPathValue::from_atomic(xml_date(result)))
}

/// Implements fn:adjust-time-to-timezone($arg, $timezone?) - adjusts a time's timezone.
pub fn adjust_time_to_timezone<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.is_empty() || args.len() > 2 {
        return Err(XPathError::wrong_number_of_arguments(
            "adjust-time-to-timezone",
            1,
            args.len(),
        ));
    }

    // Get timezone argument (if provided)
    let tz_arg = if args.len() == 2 {
        Some(args.remove(1))
    } else {
        None
    };

    let time_arg = args.remove(0);
    let time_value = match atomize_to_single_opt(time_arg)? {
        None => return Ok(XPathValue::Empty),
        Some(v) => v,
    };

    let time = require_time(time_value, "adjust-time-to-timezone")?;

    // Determine target timezone
    let target_tz = if let Some(tz_val) = tz_arg {
        match atomize_to_single_opt(tz_val)? {
            None => {
                // Empty timezone argument - strip timezone
                let result = TimeValue {
                    hour: time.hour,
                    minute: time.minute,
                    second: time.second,
                    timezone: None,
                };
                return Ok(XPathValue::from_atomic(xml_time(result)));
            }
            Some(v) => {
                let duration = require_day_time_duration(v, "adjust-time-to-timezone")?;
                day_time_duration_to_timezone(&duration)?
            }
        }
    } else {
        // 1-arg form: use implicit timezone
        context.implicit_timezone_or_host()
    };

    validate_timezone_offset(target_tz.0)?;

    // Apply timezone adjustment
    let result = match time.timezone {
        None => {
            // Input has no timezone - just attach the new one without shifting
            TimeValue {
                hour: time.hour,
                minute: time.minute,
                second: time.second,
                timezone: Some(target_tz),
            }
        }
        Some(source_tz) => {
            // Convert from source timezone to target timezone
            let offset_diff = target_tz.0 - source_tz.0;
            adjust_time_by_minutes(&time, offset_diff, target_tz)?
        }
    };

    Ok(XPathValue::from_atomic(xml_time(result)))
}

// ============================================================================
// Timezone adjustment helpers
// ============================================================================

/// Adjust a dateTime by a number of minutes offset.
fn adjust_datetime_by_minutes(
    dt: &DateTimeValue,
    offset_minutes: i16,
    target_tz: TimezoneOffset,
) -> Result<DateTimeValue, XPathError> {
    // Convert to total minutes from start of day
    let mut total_minutes = dt.hour as i32 * 60 + dt.minute as i32 + offset_minutes as i32;

    // Normalize to 0-1439 range (minutes in a day)
    let mut day_delta = 0i32;
    while total_minutes < 0 {
        total_minutes += 1440;
        day_delta -= 1;
    }
    while total_minutes >= 1440 {
        total_minutes -= 1440;
        day_delta += 1;
    }

    let new_hour = (total_minutes / 60) as u8;
    let new_minute = (total_minutes % 60) as u8;

    // Adjust date if needed
    let (new_year, new_month, new_day) = add_days_to_date(dt.year, dt.month, dt.day, day_delta)?;

    Ok(DateTimeValue {
        year: new_year,
        month: new_month,
        day: new_day,
        hour: new_hour,
        minute: new_minute,
        second: dt.second,
        timezone: Some(target_tz),
    })
}

/// Adjust a date by a number of minutes offset.
/// For dates, midnight (00:00:00) is implied, so we check if we cross day boundaries.
/// The offset can be up to ±1680 minutes (±28 hours), requiring up to ±2 day shifts.
fn adjust_date_by_minutes(
    date: &DateValue,
    offset_minutes: i16,
    target_tz: TimezoneOffset,
) -> Result<DateValue, XPathError> {
    // A date is treated as starting at 00:00:00
    // When adjusting, we're converting from 00:00 in the source timezone to the target timezone
    // The resulting time determines if we cross day boundaries

    let total_minutes = offset_minutes as i32;

    // Calculate the resulting time of day (in minutes from midnight)
    // For dates starting at 00:00, the new time is just the offset
    let resulting_time = total_minutes;

    // Calculate day delta based on how many full days we've shifted
    let day_delta = if resulting_time >= 0 {
        // Positive or zero offset: how many full days forward
        resulting_time / 1440
    } else {
        // Negative offset: ceiling division for days backward
        // e.g., -1 to -1440 = -1 day, -1441 to -2880 = -2 days
        (resulting_time - 1439) / 1440
    };

    let (new_year, new_month, new_day) =
        add_days_to_date(date.year, date.month, date.day, day_delta)?;

    Ok(DateValue {
        year: new_year,
        month: new_month,
        day: new_day,
        timezone: Some(target_tz),
    })
}

/// Adjust a time by a number of minutes offset.
fn adjust_time_by_minutes(
    time: &TimeValue,
    offset_minutes: i16,
    target_tz: TimezoneOffset,
) -> Result<TimeValue, XPathError> {
    let mut total_minutes = time.hour as i32 * 60 + time.minute as i32 + offset_minutes as i32;

    // Normalize to 0-1439 range (minutes in a day)
    while total_minutes < 0 {
        total_minutes += 1440;
    }
    while total_minutes >= 1440 {
        total_minutes -= 1440;
    }

    let new_hour = (total_minutes / 60) as u8;
    let new_minute = (total_minutes % 60) as u8;

    Ok(TimeValue {
        hour: new_hour,
        minute: new_minute,
        second: time.second,
        timezone: Some(target_tz),
    })
}

/// Add days to a date, handling month/year rollovers.
fn add_days_to_date(
    year: i32,
    month: u8,
    day: u8,
    delta: i32,
) -> Result<(i32, u8, u8), XPathError> {
    if delta == 0 {
        return Ok((year, month, day));
    }

    let mut y = year;
    let mut m = month as i32;
    let mut d = day as i32 + delta;

    // Adjust for day overflow/underflow
    loop {
        let days_in_current_month = days_in_month(y, m as u8)?;

        if d > days_in_current_month as i32 {
            d -= days_in_current_month as i32;
            m += 1;
            if m > 12 {
                m = 1;
                // F&O §10.1.1: FODT0001 beyond the year range held (`i32`).
                y = y.checked_add(1).ok_or(XPathError::FODT0001)?;
            }
        } else if d < 1 {
            m -= 1;
            if m < 1 {
                m = 12;
                y = y.checked_sub(1).ok_or(XPathError::FODT0001)?;
            }
            let days_in_prev_month = days_in_month(y, m as u8)?;
            d += days_in_prev_month as i32;
        } else {
            break;
        }
    }

    Ok((y, m as u8, d as u8))
}

/// Get the number of days in a month.
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

/// Check if a year is a leap year.
fn is_leap_year(year: i32) -> bool {
    let year = year as i64;
    year.rem_euclid(4) == 0 && (year.rem_euclid(100) != 0 || year.rem_euclid(400) == 0)
}

// ============================================================================
// Tests
// ============================================================================

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

    fn make_datetime_value(
        year: i32,
        month: u8,
        day: u8,
        hour: u8,
        minute: u8,
        second: Decimal,
        timezone: Option<TimezoneOffset>,
    ) -> XPathValue<RoXmlNavigator<'static>> {
        let dt = DateTimeValue {
            year,
            month,
            day,
            hour,
            minute,
            second,
            timezone,
        };
        XPathValue::from_atomic(xml_datetime(dt))
    }

    fn make_date_value(
        year: i32,
        month: u8,
        day: u8,
        timezone: Option<TimezoneOffset>,
    ) -> XPathValue<RoXmlNavigator<'static>> {
        let d = DateValue {
            year,
            month,
            day,
            timezone,
        };
        XPathValue::from_atomic(xml_date(d))
    }

    fn make_time_value(
        hour: u8,
        minute: u8,
        second: Decimal,
        timezone: Option<TimezoneOffset>,
    ) -> XPathValue<RoXmlNavigator<'static>> {
        let t = TimeValue {
            hour,
            minute,
            second,
            timezone,
        };
        XPathValue::from_atomic(xml_time(t))
    }

    fn make_duration_value(
        negative: bool,
        years: u32,
        months: u32,
        days: u32,
        hours: u32,
        minutes: u32,
        seconds: Decimal,
    ) -> XPathValue<RoXmlNavigator<'static>> {
        let d = DurationValue {
            negative,
            years,
            months,
            days,
            hours,
            minutes,
            seconds,
        };
        XPathValue::from_atomic(XmlValue {
            type_code: XmlTypeCode::Duration,
            schema_type: None,
            value: XmlValueKind::Atomic(XmlAtomicValue::Duration(d)),
        })
    }

    fn make_year_month_duration(
        negative: bool,
        years: u32,
        months: u32,
    ) -> XPathValue<RoXmlNavigator<'static>> {
        let d = YearMonthDurationValue {
            negative,
            years,
            months,
        };
        XPathValue::from_atomic(XmlValue {
            type_code: XmlTypeCode::YearMonthDuration,
            schema_type: None,
            value: XmlValueKind::Atomic(XmlAtomicValue::YearMonthDuration(d)),
        })
    }

    fn make_day_time_duration(
        negative: bool,
        days: u32,
        hours: u32,
        minutes: u32,
        seconds: Decimal,
    ) -> XPathValue<RoXmlNavigator<'static>> {
        let d = DayTimeDurationValue {
            negative,
            days,
            hours,
            minutes,
            seconds,
        };
        XPathValue::from_atomic(xml_day_time_duration(d))
    }

    fn get_integer_result<N: DomNavigator>(result: &XPathValue<N>) -> Option<i64> {
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => v.as_integer().and_then(|i| i.to_i64()),
            _ => None,
        }
    }

    fn get_decimal_result<N: DomNavigator>(result: &XPathValue<N>) -> Option<Decimal> {
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => v.as_decimal(),
            _ => None,
        }
    }

    // ========================================================================
    // Current time function tests
    // ========================================================================

    #[test]
    fn test_current_datetime_returns_value() {
        let mut ctx = make_context();
        let result = current_datetime(&mut ctx, vec![]).unwrap();
        assert!(!result.is_empty());
    }

    #[test]
    fn test_current_date_returns_value() {
        let mut ctx = make_context();
        let result = current_date(&mut ctx, vec![]).unwrap();
        assert!(!result.is_empty());
    }

    #[test]
    fn test_current_time_returns_value() {
        let mut ctx = make_context();
        let result = current_time(&mut ctx, vec![]).unwrap();
        assert!(!result.is_empty());
    }

    #[test]
    fn test_implicit_timezone_returns_value() {
        let mut ctx = make_context();
        let result = implicit_timezone(&mut ctx, vec![]).unwrap();
        assert!(!result.is_empty());
    }

    // ========================================================================
    // Duration component extraction tests
    // ========================================================================

    #[test]
    fn test_years_from_duration() {
        let mut ctx = make_context();
        // P1Y2M3DT4H5M6S -> 1 year
        let dur = make_duration_value(false, 1, 2, 3, 4, 5, Decimal::from(6));
        let result = years_from_duration(&mut ctx, vec![dur]).unwrap();
        assert_eq!(get_integer_result(&result), Some(1));
    }

    #[test]
    fn test_years_from_duration_negative() {
        let mut ctx = make_context();
        // -P1Y2M -> -1 year
        let dur = make_duration_value(true, 1, 2, 0, 0, 0, Decimal::ZERO);
        let result = years_from_duration(&mut ctx, vec![dur]).unwrap();
        assert_eq!(get_integer_result(&result), Some(-1));
    }

    #[test]
    fn test_months_from_duration() {
        let mut ctx = make_context();
        // P1Y14M -> 2 months (14 % 12 = 2)
        let dur = make_duration_value(false, 1, 14, 0, 0, 0, Decimal::ZERO);
        let result = months_from_duration(&mut ctx, vec![dur]).unwrap();
        assert_eq!(get_integer_result(&result), Some(2));
    }

    #[test]
    fn test_days_from_duration() {
        let mut ctx = make_context();
        // P5D -> 5 days
        let dur = make_day_time_duration(false, 5, 0, 0, Decimal::ZERO);
        let result = days_from_duration(&mut ctx, vec![dur]).unwrap();
        assert_eq!(get_integer_result(&result), Some(5));
    }

    #[test]
    fn test_hours_from_duration() {
        let mut ctx = make_context();
        // PT10H -> 10 hours
        let dur = make_day_time_duration(false, 0, 10, 0, Decimal::ZERO);
        let result = hours_from_duration(&mut ctx, vec![dur]).unwrap();
        assert_eq!(get_integer_result(&result), Some(10));
    }

    #[test]
    fn test_minutes_from_duration() {
        let mut ctx = make_context();
        // PT45M -> 45 minutes
        let dur = make_day_time_duration(false, 0, 0, 45, Decimal::ZERO);
        let result = minutes_from_duration(&mut ctx, vec![dur]).unwrap();
        assert_eq!(get_integer_result(&result), Some(45));
    }

    #[test]
    fn test_seconds_from_duration() {
        let mut ctx = make_context();
        // PT30.5S -> 30.5 seconds
        let dur = make_day_time_duration(false, 0, 0, 0, Decimal::new(305, 1));
        let result = seconds_from_duration(&mut ctx, vec![dur]).unwrap();
        assert_eq!(get_decimal_result(&result), Some(Decimal::new(305, 1)));
    }

    // ========================================================================
    // DateTime component extraction tests
    // ========================================================================

    #[test]
    fn test_year_from_datetime() {
        let mut ctx = make_context();
        let dt = make_datetime_value(2024, 3, 15, 10, 30, Decimal::from(0), None);
        let result = year_from_datetime(&mut ctx, vec![dt]).unwrap();
        assert_eq!(get_integer_result(&result), Some(2024));
    }

    #[test]
    fn test_month_from_datetime() {
        let mut ctx = make_context();
        let dt = make_datetime_value(2024, 3, 15, 10, 30, Decimal::from(0), None);
        let result = month_from_datetime(&mut ctx, vec![dt]).unwrap();
        assert_eq!(get_integer_result(&result), Some(3));
    }

    #[test]
    fn test_day_from_datetime() {
        let mut ctx = make_context();
        let dt = make_datetime_value(2024, 3, 15, 10, 30, Decimal::from(0), None);
        let result = day_from_datetime(&mut ctx, vec![dt]).unwrap();
        assert_eq!(get_integer_result(&result), Some(15));
    }

    #[test]
    fn test_hours_from_datetime() {
        let mut ctx = make_context();
        let dt = make_datetime_value(2024, 3, 15, 10, 30, Decimal::from(0), None);
        let result = hours_from_datetime(&mut ctx, vec![dt]).unwrap();
        assert_eq!(get_integer_result(&result), Some(10));
    }

    #[test]
    fn test_minutes_from_datetime() {
        let mut ctx = make_context();
        let dt = make_datetime_value(2024, 3, 15, 10, 30, Decimal::from(0), None);
        let result = minutes_from_datetime(&mut ctx, vec![dt]).unwrap();
        assert_eq!(get_integer_result(&result), Some(30));
    }

    #[test]
    fn test_seconds_from_datetime() {
        let mut ctx = make_context();
        let dt = make_datetime_value(2024, 3, 15, 10, 30, Decimal::new(455, 1), None);
        let result = seconds_from_datetime(&mut ctx, vec![dt]).unwrap();
        assert_eq!(get_decimal_result(&result), Some(Decimal::new(455, 1)));
    }

    #[test]
    fn test_timezone_from_datetime_with_tz() {
        let mut ctx = make_context();
        let dt = make_datetime_value(
            2024,
            3,
            15,
            10,
            30,
            Decimal::from(0),
            Some(TimezoneOffset(-300)),
        );
        let result = timezone_from_datetime(&mut ctx, vec![dt]).unwrap();
        assert!(!result.is_empty());
    }

    #[test]
    fn test_timezone_from_datetime_without_tz() {
        let mut ctx = make_context();
        let dt = make_datetime_value(2024, 3, 15, 10, 30, Decimal::from(0), None);
        let result = timezone_from_datetime(&mut ctx, vec![dt]).unwrap();
        assert!(result.is_empty());
    }

    // ========================================================================
    // Date component extraction tests
    // ========================================================================

    #[test]
    fn test_year_from_date() {
        let mut ctx = make_context();
        let d = make_date_value(2024, 6, 20, None);
        let result = year_from_date(&mut ctx, vec![d]).unwrap();
        assert_eq!(get_integer_result(&result), Some(2024));
    }

    #[test]
    fn test_month_from_date() {
        let mut ctx = make_context();
        let d = make_date_value(2024, 6, 20, None);
        let result = month_from_date(&mut ctx, vec![d]).unwrap();
        assert_eq!(get_integer_result(&result), Some(6));
    }

    #[test]
    fn test_day_from_date() {
        let mut ctx = make_context();
        let d = make_date_value(2024, 6, 20, None);
        let result = day_from_date(&mut ctx, vec![d]).unwrap();
        assert_eq!(get_integer_result(&result), Some(20));
    }

    // ========================================================================
    // Time component extraction tests
    // ========================================================================

    #[test]
    fn test_hours_from_time() {
        let mut ctx = make_context();
        let t = make_time_value(14, 35, Decimal::from(0), None);
        let result = hours_from_time(&mut ctx, vec![t]).unwrap();
        assert_eq!(get_integer_result(&result), Some(14));
    }

    #[test]
    fn test_minutes_from_time() {
        let mut ctx = make_context();
        let t = make_time_value(14, 35, Decimal::from(0), None);
        let result = minutes_from_time(&mut ctx, vec![t]).unwrap();
        assert_eq!(get_integer_result(&result), Some(35));
    }

    #[test]
    fn test_seconds_from_time() {
        let mut ctx = make_context();
        let t = make_time_value(14, 35, Decimal::new(125, 1), None);
        let result = seconds_from_time(&mut ctx, vec![t]).unwrap();
        assert_eq!(get_decimal_result(&result), Some(Decimal::new(125, 1)));
    }

    // ========================================================================
    // dateTime constructor tests
    // ========================================================================

    #[test]
    fn test_create_datetime_no_tz() {
        let mut ctx = make_context();
        let d = make_date_value(2024, 3, 15, None);
        let t = make_time_value(10, 30, Decimal::from(0), None);
        let result = create_datetime(&mut ctx, vec![d, t]).unwrap();
        assert!(!result.is_empty());
    }

    #[test]
    fn test_create_datetime_with_matching_tz() {
        let mut ctx = make_context();
        let tz = TimezoneOffset::from_hm(5, 0);
        let d = make_date_value(2024, 3, 15, Some(tz));
        let t = make_time_value(10, 30, Decimal::from(0), Some(tz));
        let result = create_datetime(&mut ctx, vec![d, t]).unwrap();
        assert!(!result.is_empty());
    }

    #[test]
    fn test_create_datetime_mismatched_tz() {
        let mut ctx = make_context();
        let d = make_date_value(2024, 3, 15, Some(TimezoneOffset::from_hm(5, 0)));
        let t = make_time_value(
            10,
            30,
            Decimal::from(0),
            Some(TimezoneOffset::from_hm(-5, 0)),
        );
        let result = create_datetime(&mut ctx, vec![d, t]);
        assert!(matches!(result, Err(XPathError::FORG0008)));
    }

    // ========================================================================
    // Timezone adjustment tests
    // ========================================================================

    #[test]
    fn test_adjust_datetime_to_timezone_no_input_tz() {
        let mut ctx = make_context();
        ctx.implicit_timezone = Some(TimezoneOffset::from_hm(-5, 0));
        let dt = make_datetime_value(2024, 3, 15, 10, 30, Decimal::from(0), None);
        let result = adjust_datetime_to_timezone(&mut ctx, vec![dt]).unwrap();
        // Should attach -05:00 without shifting
        assert!(!result.is_empty());
    }

    #[test]
    fn test_adjust_datetime_to_timezone_strip_tz() {
        let mut ctx = make_context();
        let dt = make_datetime_value(
            2024,
            3,
            15,
            10,
            30,
            Decimal::from(0),
            Some(TimezoneOffset::UTC),
        );
        let result = adjust_datetime_to_timezone(&mut ctx, vec![dt, XPathValue::Empty]).unwrap();
        // Should have no timezone
        assert!(!result.is_empty());
    }

    #[test]
    fn test_adjust_time_to_timezone() {
        let mut ctx = make_context();
        ctx.implicit_timezone = Some(TimezoneOffset::from_hm(0, 0));
        let t = make_time_value(10, 30, Decimal::from(0), None);
        let result = adjust_time_to_timezone(&mut ctx, vec![t]).unwrap();
        assert!(!result.is_empty());
    }

    // ========================================================================
    // Empty sequence tests
    // ========================================================================

    #[test]
    fn test_component_functions_with_empty() {
        let mut ctx = make_context();

        let result = years_from_duration(&mut ctx, vec![XPathValue::Empty]).unwrap();
        assert!(result.is_empty());

        let result = year_from_datetime(&mut ctx, vec![XPathValue::Empty]).unwrap();
        assert!(result.is_empty());

        let result = year_from_date(&mut ctx, vec![XPathValue::Empty]).unwrap();
        assert!(result.is_empty());

        let result = hours_from_time(&mut ctx, vec![XPathValue::Empty]).unwrap();
        assert!(result.is_empty());
    }

    // ========================================================================
    // Duration normalization tests (Issue fix verification)
    // ========================================================================

    #[test]
    fn test_duration_normalization_p14m() {
        // P14M should normalize to years=1, months=2
        let mut ctx = make_context();
        let dur = make_year_month_duration(false, 0, 14);

        let result = years_from_duration(&mut ctx, vec![dur.clone()]).unwrap();
        assert_eq!(get_integer_result(&result), Some(1));

        let result = months_from_duration(&mut ctx, vec![dur]).unwrap();
        assert_eq!(get_integer_result(&result), Some(2));
    }

    #[test]
    fn test_duration_normalization_pt30h() {
        // PT30H should normalize to days=1, hours=6
        let mut ctx = make_context();
        let dur = make_day_time_duration(false, 0, 30, 0, Decimal::ZERO);

        let result = days_from_duration(&mut ctx, vec![dur.clone()]).unwrap();
        assert_eq!(get_integer_result(&result), Some(1));

        let result = hours_from_duration(&mut ctx, vec![dur]).unwrap();
        assert_eq!(get_integer_result(&result), Some(6));
    }

    #[test]
    fn test_duration_normalization_pt90m() {
        // PT90M should normalize to hours=1, minutes=30
        let mut ctx = make_context();
        let dur = make_day_time_duration(false, 0, 0, 90, Decimal::ZERO);

        let result = hours_from_duration(&mut ctx, vec![dur.clone()]).unwrap();
        assert_eq!(get_integer_result(&result), Some(1));

        let result = minutes_from_duration(&mut ctx, vec![dur]).unwrap();
        assert_eq!(get_integer_result(&result), Some(30));
    }

    #[test]
    fn test_duration_normalization_pt3665s() {
        // PT3665S should normalize to hours=1, minutes=1, seconds=5
        let mut ctx = make_context();
        let dur = make_day_time_duration(false, 0, 0, 0, Decimal::from(3665));

        let result = hours_from_duration(&mut ctx, vec![dur.clone()]).unwrap();
        assert_eq!(get_integer_result(&result), Some(1));

        let result = minutes_from_duration(&mut ctx, vec![dur.clone()]).unwrap();
        assert_eq!(get_integer_result(&result), Some(1));

        let result = seconds_from_duration(&mut ctx, vec![dur]).unwrap();
        assert_eq!(get_decimal_result(&result), Some(Decimal::from(5)));
    }

    // ========================================================================
    // Timezone offset validation tests (Issue fix verification)
    // ========================================================================

    #[test]
    fn test_timezone_offset_with_days_rejected() {
        // A timezone with days component should be rejected with FODT0003
        let dur = DayTimeDurationValue {
            negative: false,
            days: 1,
            hours: 0,
            minutes: 0,
            seconds: Decimal::ZERO,
        };
        let result = day_time_duration_to_timezone(&dur);
        assert!(matches!(result, Err(XPathError::FODT0003 { .. })));
    }

    #[test]
    fn test_timezone_offset_with_seconds_rejected() {
        // A timezone with seconds component should be rejected with FODT0003
        let dur = DayTimeDurationValue {
            negative: false,
            days: 0,
            hours: 5,
            minutes: 0,
            seconds: Decimal::from(30),
        };
        let result = day_time_duration_to_timezone(&dur);
        assert!(matches!(result, Err(XPathError::FODT0003 { .. })));
    }

    #[test]
    fn test_timezone_offset_with_fractional_seconds_rejected() {
        // A timezone with fractional seconds should be rejected
        let dur = DayTimeDurationValue {
            negative: false,
            days: 0,
            hours: 5,
            minutes: 0,
            seconds: Decimal::new(5, 1), // 0.5 seconds
        };
        let result = day_time_duration_to_timezone(&dur);
        assert!(matches!(result, Err(XPathError::FODT0003 { .. })));
    }

    #[test]
    fn test_timezone_offset_valid() {
        // Valid timezone: PT5H30M
        let dur = DayTimeDurationValue {
            negative: false,
            days: 0,
            hours: 5,
            minutes: 30,
            seconds: Decimal::ZERO,
        };
        let result = day_time_duration_to_timezone(&dur).unwrap();
        assert_eq!(result.0, 330); // 5*60 + 30 = 330 minutes
    }

    #[test]
    fn test_timezone_offset_out_of_range_rejected() {
        // Timezone > 14:00 should be rejected
        let dur = DayTimeDurationValue {
            negative: false,
            days: 0,
            hours: 15,
            minutes: 0,
            seconds: Decimal::ZERO,
        };
        let result = day_time_duration_to_timezone(&dur);
        assert!(matches!(result, Err(XPathError::FODT0003 { .. })));
    }
}
