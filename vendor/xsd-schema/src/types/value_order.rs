//! The order relation of the date/time and duration value spaces — the one
//! rule the bounding facets, `enumeration`, value constraints (`fixed`),
//! identity constraints and the schema-time facet checks share.
//!
//! - The date/time datatypes of the seven-property model are ordered on the
//!   time line, partially when exactly one of two values has a time zone
//!   offset (Datatypes 1.1 §D.2.1; XSD 1.0 §3.2.7.4).
//! - `xs:duration` is partially ordered by the four reference dateTimes of
//!   Datatypes 1.1 §3.3.6.1 (XSD 1.0 §3.2.6.2).
//!
//! The value structs' own `PartialOrd` / `PartialEq` are this order and its
//! equality (`types::value`); this module adds the comparisons over
//! `XmlAtomicValue`, the XSD 1.0 year numbering and identity. The XPath engine
//! compares date/time values with its implicit time zone instead, through its
//! own functions.

use std::cmp::Ordering;

use rust_decimal::Decimal;

use super::value::{
    DateTimeValue, DateValue, DayTimeDurationValue, DurationValue, GDayValue, GMonthDayValue,
    GMonthValue, GYearMonthValue, GYearValue, TimeValue, XmlAtomicValue, YearMonthDurationValue,
};

/// The order relation the bounding facets and `enumeration` compare the
/// values of a datatype in — `None` for two ·incomparable· values, which no
/// bound admits and which are not equal.
pub(crate) trait FacetOrder {
    fn facet_cmp(&self, other: &Self) -> Option<Ordering>;

    /// [`facet_cmp`](Self::facet_cmp) of two values read from the literals
    /// `lexical` and `other_lexical`: for the datatypes with seconds, exact
    /// also when a literal's seconds have more fraction digits than the value
    /// holds ([`literal_cmp_in`]).
    fn facet_cmp_literal(
        &self,
        lexical: &str,
        other: &Self,
        other_lexical: &str,
    ) -> Option<Ordering> {
        let _ = (lexical, other_lexical);
        self.facet_cmp(other)
    }
}

/// The value structs' own `PartialOrd` is this order (the seven-property
/// types on the time line with the ±14:00 imputation, `xs:duration` by the
/// four reference dateTimes, the two totally ordered duration types by months
/// and by seconds).
macro_rules! partial_ord_facet_order {
    ($($ty:ty),*) => {$(
        impl FacetOrder for $ty {
            fn facet_cmp(&self, other: &Self) -> Option<Ordering> {
                self.partial_cmp(other)
            }
        }
    )*};
}
partial_ord_facet_order!(
    DateValue,
    GYearMonthValue,
    GYearValue,
    GMonthDayValue,
    GDayValue,
    GMonthValue,
    YearMonthDurationValue
);

/// The datatypes with seconds: `facet_cmp` is their `PartialOrd`, and two
/// values read from literals whose seconds have more fraction digits than a
/// `Decimal` holds are compared with the fractions as written.
macro_rules! seconds_facet_order {
    ($($ty:ty => $variant:ident),*) => {$(
        impl FacetOrder for $ty {
            fn facet_cmp(&self, other: &Self) -> Option<Ordering> {
                self.partial_cmp(other)
            }

            fn facet_cmp_literal(
                &self,
                lexical: &str,
                other: &Self,
                other_lexical: &str,
            ) -> Option<Ordering> {
                if !beyond_capacity(lexical) && !beyond_capacity(other_lexical) {
                    return self.partial_cmp(other);
                }
                literal_cmp_in(
                    false,
                    &XmlAtomicValue::$variant(self.clone()),
                    lexical,
                    &XmlAtomicValue::$variant(other.clone()),
                    other_lexical,
                )
            }
        }
    )*};
}
seconds_facet_order!(
    DateTimeValue => DateTime,
    TimeValue => Time,
    DurationValue => Duration,
    DayTimeDurationValue => DayTimeDuration
);

/// The strict order `order_at(offset)` gives when the value without a time
/// zone offset is given +14:00 and when it is given −14:00 (in minutes), if
/// both give the same one; otherwise the two values are ·incomparable·.
pub(crate) fn same_order_at_both_extreme_offsets(
    order_at: impl Fn(i16) -> Option<Ordering>,
) -> Option<Ordering> {
    const MAX_OFFSET: i16 = 14 * 60;
    match (order_at(MAX_OFFSET), order_at(-MAX_OFFSET)) {
        (Some(a), Some(b)) if a == b && a != Ordering::Equal => Some(a),
        _ => None,
    }
}

/// The order of two atomic values of the date/time and duration datatypes in
/// [`FacetOrder`] — two values of the same seven-property datatype, or two
/// durations of any of the three duration datatypes (restrictions do not
/// create new values: an `xs:yearMonthDuration` is an `xs:duration`); `None`
/// for incomparable values and for every other pair.
pub(crate) fn date_time_or_duration_cmp(
    a: &XmlAtomicValue,
    b: &XmlAtomicValue,
) -> Option<Ordering> {
    use XmlAtomicValue as V;
    match (a, b) {
        (V::DateTime(x), V::DateTime(y)) => x.facet_cmp(y),
        (V::Date(x), V::Date(y)) => x.facet_cmp(y),
        (V::Time(x), V::Time(y)) => x.facet_cmp(y),
        (V::GYearMonth(x), V::GYearMonth(y)) => x.facet_cmp(y),
        (V::GYear(x), V::GYear(y)) => x.facet_cmp(y),
        (V::GMonthDay(x), V::GMonthDay(y)) => x.facet_cmp(y),
        (V::GDay(x), V::GDay(y)) => x.facet_cmp(y),
        (V::GMonth(x), V::GMonth(y)) => x.facet_cmp(y),
        _ => duration_cmp(&duration_parts(a)?, &duration_parts(b)?),
    }
}

/// [`date_time_or_duration_cmp`] in the year numbering of the schema's XSD
/// version. XSD 1.0 §3.2.7: "'-0001' is the lexical representation of the
/// year 1 Before Common Era (1 BCE …)", and '0000' is disallowed, so a 1.0
/// year −n is the 1.1 (astronomical) year 1 − n; Datatypes 1.1 §3.3.7:
/// "'0000' … 1 BCE", '-0001' 2 BCE. In 1.0 mode the negative years are moved
/// up by one before the 1.1 order compares them.
pub(crate) fn date_time_or_duration_cmp_in(
    xsd10: bool,
    a: &XmlAtomicValue,
    b: &XmlAtomicValue,
) -> Option<Ordering> {
    if xsd10 && (negative_year(a) || negative_year(b)) {
        date_time_or_duration_cmp(&xsd10_year_numbering(a), &xsd10_year_numbering(b))
    } else {
        date_time_or_duration_cmp(a, b)
    }
}

/// Whether a date/time or duration literal has seconds with more fraction
/// digits than its typed value holds — the typed value holds them truncated
/// (`validators::parse_seconds`), and the comparisons that see the literal
/// use the fraction as written ([`literal_cmp_in`]).
fn beyond_capacity(lexical: &str) -> bool {
    super::validators::seconds_beyond_capacity(lexical)
}

/// [`date_time_or_duration_cmp_in`] of two values read from the literals
/// `a_lexical` and `b_lexical` — exact for every literal the lexical space
/// admits and the typed value represents: a seconds number with more
/// fraction digits than a `Decimal` holds (at most 28, fewer for a large
/// integral part) is kept truncated in the typed value, and a pair with such
/// a literal is compared here with the whole fraction as written (Datatypes
/// 1.1 §5.4: "the unsupported value must not be quietly changed to a
/// supported value"). The whole seconds are those of the typed value, which
/// truncation does not touch.
pub(crate) fn literal_cmp_in(
    xsd10: bool,
    a: &XmlAtomicValue,
    a_lexical: &str,
    b: &XmlAtomicValue,
    b_lexical: &str,
) -> Option<Ordering> {
    use std::borrow::Cow;
    use XmlAtomicValue as V;
    if !beyond_capacity(a_lexical) && !beyond_capacity(b_lexical) {
        return date_time_or_duration_cmp_in(xsd10, a, b);
    }
    let fa = ExactFraction::from_digits(literal_fraction(a_lexical));
    let fb = ExactFraction::from_digits(literal_fraction(b_lexical));
    let (a, b) = if xsd10 {
        (xsd10_year_numbering(a), xsd10_year_numbering(b))
    } else {
        (Cow::Borrowed(a), Cow::Borrowed(b))
    };
    match (&*a, &*b) {
        (V::DateTime(x), V::DateTime(y)) => exact_time_line_cmp(
            |impute| (x.time_line_whole(impute), &fa),
            x.timezone.is_some(),
            |impute| (y.time_line_whole(impute), &fb),
            y.timezone.is_some(),
        ),
        (V::Time(x), V::Time(y)) => exact_time_line_cmp(
            |impute| (x.time_line_whole(impute), &fa),
            x.timezone.is_some(),
            |impute| (y.time_line_whole(impute), &fb),
            y.timezone.is_some(),
        ),
        (x, y) => match (exact_duration_parts(x, &fa), exact_duration_parts(y, &fb)) {
            (Some(x), Some(y)) => duration_cmp(&x, &y),
            _ => date_time_or_duration_cmp(x, y),
        },
    }
}

/// The time line order of the seven-property model (Datatypes 1.1 §D.2.1)
/// on exact keys: `key(None)` is a value's position (whole seconds, the
/// fraction apart) with its own time zone offset, or `Z` without one, and
/// `key(Some(minutes))` its position with that offset imputed; a pair with
/// exactly one offset is ordered only if both extreme imputations agree.
fn exact_time_line_cmp<'f>(
    a: impl Fn(Option<i16>) -> (i128, &'f ExactFraction),
    a_zoned: bool,
    b: impl Fn(Option<i16>) -> (i128, &'f ExactFraction),
    b_zoned: bool,
) -> Option<Ordering> {
    match (a_zoned, b_zoned) {
        (true, false) => same_order_at_both_extreme_offsets(|m| Some(a(None).cmp(&b(Some(m))))),
        (false, true) => same_order_at_both_extreme_offsets(|m| Some(a(Some(m)).cmp(&b(None)))),
        _ => Some(a(None).cmp(&b(None))),
    }
}

/// The fraction digits of the seconds in a date/time or duration literal —
/// the digits after its decimal point (only a seconds number has one);
/// empty without a point.
fn literal_fraction(lexical: &str) -> &str {
    let Some((_, after)) = lexical.split_once('.') else {
        return "";
    };
    let end = after
        .bytes()
        .position(|b| !b.is_ascii_digit())
        .unwrap_or(after.len());
    &after[..end]
}

/// A fraction of a second in `[0, 1)`, exactly: its decimal digits after the
/// point, without trailing zeros (none for zero). Two fractions are ordered
/// as their digit strings are — a proper prefix is the smaller number — which
/// is their numeric order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
pub(crate) struct ExactFraction(Box<str>);

impl ExactFraction {
    fn from_digits(digits: &str) -> Self {
        Self(digits.trim_end_matches('0').into())
    }

    fn is_zero(&self) -> bool {
        self.0.is_empty()
    }

    /// `1 − self`, for a fraction above zero: each digit `d` but the last
    /// becomes `9 − d`, the last (never 0) `10 − d`.
    fn complement(&self) -> Self {
        let last = self.0.len() - 1;
        let digits: String = self
            .0
            .bytes()
            .enumerate()
            .map(|(i, b)| {
                let d = b - b'0';
                char::from(b'0' + if i == last { 10 - d } else { 9 - d })
            })
            .collect();
        Self::from_digits(&digits)
    }
}

/// Whether `atom` is a dateTime, date, gYear or gYearMonth value with a
/// negative year, the one place the year numberings of XSD 1.0 and 1.1 differ.
pub(crate) fn negative_year(atom: &XmlAtomicValue) -> bool {
    use XmlAtomicValue as V;
    match atom {
        V::DateTime(v) => v.year < 0,
        V::Date(v) => v.year < 0,
        V::GYear(v) => v.year < 0,
        V::GYearMonth(v) => v.year < 0,
        _ => false,
    }
}

/// `atom` with an XSD 1.0 negative year read in the 1.1 numbering (one up).
fn xsd10_year_numbering(atom: &XmlAtomicValue) -> std::borrow::Cow<'_, XmlAtomicValue> {
    use std::borrow::Cow;
    use XmlAtomicValue as V;
    if !negative_year(atom) {
        return Cow::Borrowed(atom);
    }
    Cow::Owned(match atom {
        V::DateTime(v) => V::DateTime(DateTimeValue {
            year: v.year + 1,
            ..v.clone()
        }),
        V::Date(v) => V::Date(DateValue {
            year: v.year + 1,
            ..v.clone()
        }),
        V::GYear(v) => V::GYear(GYearValue {
            year: v.year + 1,
            ..v.clone()
        }),
        V::GYearMonth(v) => V::GYearMonth(GYearMonthValue {
            year: v.year + 1,
            ..v.clone()
        }),
        other => other.clone(),
    })
}

/// Whether `a` and `b` are date/time or duration values that are equal in
/// [`date_time_or_duration_cmp`]: one instant on the time line (a value with
/// a time zone offset equals no value without one), durations whose four
/// reference sums agree. `None` when the pair is not of those datatypes.
pub(crate) fn date_time_or_duration_eq(a: &XmlAtomicValue, b: &XmlAtomicValue) -> Option<bool> {
    use XmlAtomicValue as V;
    let date_time = |v: &XmlAtomicValue| {
        matches!(
            v,
            V::DateTime(_)
                | V::Date(_)
                | V::Time(_)
                | V::GYearMonth(_)
                | V::GYear(_)
                | V::GMonthDay(_)
                | V::GDay(_)
                | V::GMonth(_)
        )
    };
    if date_time(a) || date_time(b) {
        return Some(date_time_or_duration_cmp(a, b) == Some(Ordering::Equal));
    }
    match (duration_parts(a), duration_parts(b)) {
        // Equal in the order of the four reference dateTimes (§3.3.6.1: "the
        // equality or order of two duration values is determined by adding
        // each duration in the pair to each of the following four dateTime
        // values"): the same months and seconds, and also `P400Y` and
        // `P146097D`, whose sums agree at all four although the text's next
        // sentence ("two duration values are equal if and only if they are
        // identical") says otherwise (decision 11).
        (Some(x), Some(y)) => Some(duration_cmp(&x, &y) == Some(Ordering::Equal)),
        _ => None,
    }
}

/// Whether two date/time values have the same seven properties (Datatypes 1.1
/// §D.2.1) — identity (§2.2.1), stricter than their equality: one instant in
/// two time zone offsets is equal but not identical (§3.3.7.1). `None` when
/// the pair is not of one seven-property datatype.
pub(crate) fn same_properties(a: &XmlAtomicValue, b: &XmlAtomicValue) -> Option<bool> {
    use XmlAtomicValue as V;
    Some(match (a, b) {
        (V::DateTime(x), V::DateTime(y)) => {
            (x.year, x.month, x.day, x.hour, x.minute, x.timezone)
                == (y.year, y.month, y.day, y.hour, y.minute, y.timezone)
                && x.second == y.second
        }
        (V::Date(x), V::Date(y)) => {
            (x.year, x.month, x.day, x.timezone) == (y.year, y.month, y.day, y.timezone)
        }
        (V::Time(x), V::Time(y)) => {
            (x.hour, x.minute, x.timezone) == (y.hour, y.minute, y.timezone) && x.second == y.second
        }
        (V::GYearMonth(x), V::GYearMonth(y)) => {
            (x.year, x.month, x.timezone) == (y.year, y.month, y.timezone)
        }
        (V::GYear(x), V::GYear(y)) => (x.year, x.timezone) == (y.year, y.timezone),
        (V::GMonthDay(x), V::GMonthDay(y)) => {
            (x.month, x.day, x.timezone) == (y.month, y.day, y.timezone)
        }
        (V::GDay(x), V::GDay(y)) => (x.day, x.timezone) == (y.day, y.timezone),
        (V::GMonth(x), V::GMonth(y)) => (x.month, x.timezone) == (y.month, y.timezone),
        _ => return None,
    })
}

/// Identity of two values of one datatype with seconds read from the
/// literals `a_lexical` and `b_lexical`, with the fractions as written (see
/// [`literal_cmp_in`]): the seven properties of two date/time values (the
/// whole seconds those of the typed values, which truncation does not
/// touch), the months and seconds of two durations. `false` for any other
/// pair.
pub(crate) fn literal_same_properties(
    a: &XmlAtomicValue,
    a_lexical: &str,
    b: &XmlAtomicValue,
    b_lexical: &str,
) -> bool {
    let fa = ExactFraction::from_digits(literal_fraction(a_lexical));
    let fb = ExactFraction::from_digits(literal_fraction(b_lexical));
    match (exact_duration_parts(a, &fa), exact_duration_parts(b, &fb)) {
        (Some(x), Some(y)) => x == y,
        _ => {
            let whole = |atom: &XmlAtomicValue| -> Option<XmlAtomicValue> {
                use XmlAtomicValue as V;
                Some(match atom {
                    V::DateTime(v) => V::DateTime(DateTimeValue {
                        second: v.second.trunc(),
                        ..v.clone()
                    }),
                    V::Time(v) => V::Time(TimeValue {
                        second: v.second.trunc(),
                        ..v.clone()
                    }),
                    _ => return None,
                })
            };
            match (whole(a), whole(b)) {
                (Some(x), Some(y)) => same_properties(&x, &y) == Some(true) && fa == fb,
                _ => false,
            }
        }
    }
}

/// A duration as the two properties of its value (Datatypes 1.1 §3.3.6.1:
/// "an integer number of months and a decimal number of seconds"), signed and
/// exact: the seconds as whole seconds (`seconds`, floor) plus a fraction in
/// `[0, 1)` (`fraction`, the decimal digits as parsed). No sum of the fields
/// a duration value holds can overflow: `months` ≤ (2³² − 1) × 13 and
/// `seconds` ≤ (2³² − 1) × 90,060 + 2⁹⁶ in magnitude, far inside `i128`.
///
/// The fraction is a `Decimal` for a typed value, an [`ExactFraction`] for a
/// value read with the digits of its literal ([`literal_cmp_in`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DurationParts<F = Decimal> {
    pub(crate) months: i128,
    pub(crate) seconds: i128,
    pub(crate) fraction: F,
}

/// [`DurationParts`] of a duration value of any of the three duration
/// datatypes; `None` for any other value.
pub(crate) fn duration_parts(atom: &XmlAtomicValue) -> Option<DurationParts> {
    use XmlAtomicValue as V;
    match atom {
        V::Duration(d) => Some(duration_value_parts(d)),
        V::YearMonthDuration(d) => Some(parts(
            d.negative,
            i128::from(d.years) * 12 + i128::from(d.months),
            0,
            0,
            0,
            Decimal::ZERO,
        )),
        V::DayTimeDuration(d) => Some(day_time_duration_parts(d)),
        _ => None,
    }
}

/// [`duration_parts`] of an `xs:dayTimeDuration` value.
pub(crate) fn day_time_duration_parts(d: &DayTimeDurationValue) -> DurationParts {
    parts(d.negative, 0, d.days, d.hours, d.minutes, d.seconds)
}

/// [`duration_parts`] of a duration with the fraction of its seconds as
/// written in its literal (`fraction`), for [`literal_cmp_in`]; `None` for
/// any other value.
fn exact_duration_parts(
    atom: &XmlAtomicValue,
    fraction: &ExactFraction,
) -> Option<DurationParts<ExactFraction>> {
    use XmlAtomicValue as V;
    // The typed value holds the seconds truncated: their whole part is the
    // literal's.
    let (negative, months, days, hours, minutes, seconds) = match atom {
        V::Duration(d) => (
            d.negative,
            i128::from(d.years) * 12 + i128::from(d.months),
            d.days,
            d.hours,
            d.minutes,
            d.seconds,
        ),
        V::DayTimeDuration(d) => (d.negative, 0, d.days, d.hours, d.minutes, d.seconds),
        V::YearMonthDuration(d) => (
            d.negative,
            i128::from(d.years) * 12 + i128::from(d.months),
            0,
            0,
            0,
            Decimal::ZERO,
        ),
        _ => return None,
    };
    let whole = parts(false, months, days, hours, minutes, seconds.trunc());
    Some(match negative {
        false => DurationParts {
            months: whole.months,
            seconds: whole.seconds,
            fraction: fraction.clone(),
        },
        true if fraction.is_zero() => DurationParts {
            months: -whole.months,
            seconds: -whole.seconds,
            fraction: ExactFraction::default(),
        },
        true => DurationParts {
            months: -whole.months,
            seconds: -whole.seconds - 1,
            fraction: fraction.complement(),
        },
    })
}

/// [`duration_parts`] of an `xs:duration` value.
pub(crate) fn duration_value_parts(d: &DurationValue) -> DurationParts {
    parts(
        d.negative,
        i128::from(d.years) * 12 + i128::from(d.months),
        d.days,
        d.hours,
        d.minutes,
        d.seconds,
    )
}

/// The exact parts of a duration from its fields: whole seconds and the
/// fraction kept apart, so that no addition rounds; the sign applied, then
/// the fraction brought into `[0, 1)` (floor).
fn parts(
    negative: bool,
    months: i128,
    days: u32,
    hours: u32,
    minutes: u32,
    seconds: Decimal,
) -> DurationParts {
    use rust_decimal::prelude::ToPrimitive;
    let whole = seconds.trunc();
    // A `Decimal`'s integral part (at most 2⁹⁶) always fits `i128`.
    let whole_seconds = whole.to_i128().unwrap_or_default();
    let mut fraction = seconds - whole;
    let mut total = i128::from(days) * 86_400
        + i128::from(hours) * 3_600
        + i128::from(minutes) * 60
        + whole_seconds;
    let mut months = months;
    if negative {
        months = -months;
        total = -total;
        fraction = -fraction;
    }
    if fraction.is_sign_negative() && !fraction.is_zero() {
        // `fraction + 1` has the same scale and a magnitude below 1: exact.
        total -= 1;
        fraction += Decimal::ONE;
    }
    DurationParts {
        months,
        seconds: total,
        fraction: fraction.normalize(),
    }
}

/// The order of two durations, given as [`DurationParts`] (Datatypes 1.1
/// §3.3.6.1): "the equality or order of two duration values is determined by
/// adding each duration in the pair to each of the following four dateTime
/// values: 1696-09-01T00:00:00Z 1697-02-01T00:00:00Z 1903-03-01T00:00:00Z
/// 1903-07-01T00:00:00Z. If all four resulting dateTime value pairs are
/// ordered the same way (less than, equal, or greater than), then the
/// original pair of duration values is ordered the same way; otherwise the
/// original pair is ·incomparable·." XSD 1.0 §3.2.6.2 uses the same four
/// dateTimes, and §3.2.6.3 has the bounding facets treat an indeterminate
/// comparison as false. Every sum is exact (whole seconds in `i128`, the
/// fraction apart), so the order is defined for every pair and every value is
/// equal to itself.
pub(crate) fn duration_cmp<F: Ord>(a: &DurationParts<F>, b: &DurationParts<F>) -> Option<Ordering> {
    const REFERENCES: [(i128, i128); 4] = [(1696, 9), (1697, 2), (1903, 3), (1903, 7)];
    let mut order = None;
    for (year, month) in REFERENCES {
        let here = plus_duration(year, month, a).cmp(&plus_duration(year, month, b));
        match order {
            None => order = Some(here),
            Some(previous) if previous != here => return None,
            Some(_) => {}
        }
    }
    order
}

/// The instant of `year-month-01T00:00:00Z` plus the duration `d` —
/// ·dateTimePlusDuration· (Datatypes 1.1 §E.3.3; XSD 1.0 Appendix E): the
/// months are added first, and the first day of a month needs no pinning,
/// then the seconds — as whole seconds and the fraction, ordered
/// lexicographically (the fraction is in `[0, 1)`).
fn plus_duration<F>(year: i128, month: i128, d: &DurationParts<F>) -> (i128, &F) {
    let total = year * 12 + (month - 1) + d.months;
    let (year, month) = (total.div_euclid(12), total.rem_euclid(12) + 1);
    (
        days_from_civil(year, month) * 86_400 + d.seconds,
        &d.fraction,
    )
}

/// The number of days from 1970-01-01 to the first day of `month` in the
/// proleptic Gregorian `year` (Datatypes 1.1 §E.3.4: the year 0000 is a leap
/// year), for any year: floor division throughout, `i128` so that no year a
/// duration or a date can reach overflows.
pub(crate) fn days_from_civil(year: i128, month: i128) -> i128 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts_of(lexical: &str) -> DurationParts {
        match crate::types::validators::VALIDATOR_REGISTRY
            .validate(crate::types::XmlTypeCode::Duration, lexical)
            .expect("a duration")
            .value
        {
            crate::types::value::XmlValueKind::Atomic(XmlAtomicValue::Duration(d)) => {
                duration_value_parts(&d)
            }
            other => panic!("{other:?}"),
        }
    }

    fn cmp(a: &str, b: &str) -> Option<Ordering> {
        duration_cmp(&parts_of(a), &parts_of(b))
    }

    #[test]
    fn days_from_civil_matches_known_dates() {
        assert_eq!(days_from_civil(1970, 1), 0);
        assert_eq!(days_from_civil(2000, 3), 11_017);
        assert_eq!(days_from_civil(1969, 12), -31);
        // 1903-03-01 + P1M: 31 days; 1697-02-01 + P1M: 28 days.
        assert_eq!(days_from_civil(1903, 4) - days_from_civil(1903, 3), 31);
        assert_eq!(days_from_civil(1697, 3) - days_from_civil(1697, 2), 28);
        // 0000 is a leap year, and so is -0004; 1900 is not, 2000 is.
        assert_eq!(days_from_civil(0, 3) - days_from_civil(0, 2), 29);
        assert_eq!(days_from_civil(-4, 3) - days_from_civil(-4, 2), 29);
        assert_eq!(days_from_civil(-5, 3) - days_from_civil(-5, 2), 28);
        assert_eq!(days_from_civil(1900, 3) - days_from_civil(1900, 2), 28);
        assert_eq!(days_from_civil(-1, 1) - days_from_civil(-2, 1), 365);
        // Every year has 365 or 366 days, also far from year 1.
        for year in [
            -2_147_483_648i128,
            -10_001,
            -401,
            -1,
            0,
            1,
            9_999,
            2_147_483_647,
            4_294_967_295 * 13,
        ] {
            let length = days_from_civil(year + 1, 1) - days_from_civil(year, 1);
            assert!(length == 365 || length == 366, "{year}: {length}");
        }
    }

    /// XSD 1.0 §3.2.6.2's table: "P1Y > P364D <> P365D <> P366D < P367D",
    /// "P1M > P27D <> P28D <> P29D <> P30D <> P31D < P32D", "P5M > P149D <>
    /// P150D <> P151D <> P152D <> P153D < P154D".
    #[test]
    fn the_xsd_1_0_table_of_duration_orders() {
        use Ordering::{Greater, Less};
        for (a, b, expected) in [
            ("P1Y", "P364D", Some(Greater)),
            ("P1Y", "P365D", None),
            ("P1Y", "P366D", None),
            ("P1Y", "P367D", Some(Less)),
            ("P1M", "P27D", Some(Greater)),
            ("P1M", "P28D", None),
            ("P1M", "P29D", None),
            ("P1M", "P30D", None),
            ("P1M", "P31D", None),
            ("P1M", "P32D", Some(Less)),
            ("P5M", "P149D", Some(Greater)),
            ("P5M", "P150D", None),
            ("P5M", "P153D", None),
            ("P5M", "P154D", Some(Less)),
            ("PT60S", "PT1M", Some(Ordering::Equal)),
            ("P1D", "PT24H", Some(Ordering::Equal)),
            ("P12M", "P1Y", Some(Ordering::Equal)),
            ("-P1M", "-P27D", Some(Less)),
            ("-P1M", "-P30D", None),
            ("-P1D", "PT0S", Some(Less)),
        ] {
            assert_eq!(cmp(a, b), expected, "{a} vs {b}");
            assert_eq!(cmp(b, a), expected.map(Ordering::reverse), "{b} vs {a}");
        }
    }

    /// The months / days table of XSD 1.0 §3.2.6.2 ("Days Minimum 28 59 89
    /// 120 150 181 212 242 273 303 334 365 393", "Maximum 31 62 92 123 153
    /// 184 215 245 276 306 337 366 397"): n months exceed every number of
    /// days below the minimum, are exceeded by every number above the
    /// maximum, and are incomparable with the days in between — with one
    /// exception. The table spans every start date ("Implementations are free
    /// to optimize … the following table can be used"); the order is defined
    /// by the four reference dateTimes, from which nine months last 273 to 275
    /// days, so `P9M < P276D` (the four sums are 273, 273, 275 and 275 days;
    /// from a 1 May they would be 276).
    #[test]
    fn months_against_days_follow_the_xsd_1_0_table() {
        let minimum = [28, 59, 89, 120, 150, 181, 212, 242, 273, 303, 334, 365, 393];
        let maximum = [31, 62, 92, 123, 153, 184, 215, 245, 275, 306, 337, 366, 397];
        for (i, (lo, hi)) in minimum.iter().zip(maximum.iter()).enumerate() {
            let months = format!("P{}M", i + 1);
            assert_eq!(
                cmp(&months, &format!("P{}D", lo - 1)),
                Some(Ordering::Greater),
                "{months}"
            );
            assert_eq!(
                cmp(&months, &format!("P{}D", hi + 1)),
                Some(Ordering::Less),
                "{months}"
            );
            for days in *lo..=*hi {
                assert_eq!(
                    cmp(&months, &format!("P{days}D")),
                    None,
                    "{months} vs P{days}D"
                );
            }
        }
    }

    /// Every sum is exact: a fraction below 10⁻¹⁸ keeps a duration apart
    /// from zero, and the largest values the fields hold are equal to
    /// themselves and ordered against their negation (fix round 2).
    #[test]
    fn duration_sums_are_exact() {
        use Ordering::{Equal, Greater, Less};
        assert_eq!(cmp("PT0.00000000000000000001S", "PT0S"), Some(Greater));
        assert_eq!(cmp("-PT0.00000000000000000001S", "PT0S"), Some(Less));
        assert_eq!(
            cmp("P10000DT0.0000000000000000000001S", "P10000D"),
            Some(Greater)
        );
        assert_eq!(cmp("PT0.5S", "PT0.50S"), Some(Equal));
        assert_eq!(cmp("-PT1.5S", "-PT1S"), Some(Less));
        assert_eq!(cmp("-PT0.5S", "-PT1S"), Some(Greater));
        const HUGE: &str = "P4294967295Y4294967295M4294967295DT4294967295H4294967295M79228162514264337593543950335S";
        let negative = format!("-{HUGE}");
        assert_eq!(cmp(HUGE, HUGE), Some(Equal));
        assert_eq!(cmp(&negative, &negative), Some(Equal));
        assert_eq!(cmp(&negative, HUGE), Some(Less));
        assert_eq!(cmp(&negative, "PT0S"), Some(Less));
        assert_eq!(parts_of(&negative), parts_of(&negative));
        assert_eq!(parts_of("-PT0.25S").seconds, -1);
        assert_eq!(parts_of("-PT0.25S").fraction, Decimal::new(75, 2));
    }

    fn atom(code: crate::types::XmlTypeCode, lexical: &str) -> XmlAtomicValue {
        match crate::types::validators::VALIDATOR_REGISTRY
            .validate(code, lexical)
            .unwrap_or_else(|e| panic!("{lexical}: {e:?}"))
            .value
        {
            crate::types::value::XmlValueKind::Atomic(a) => a,
            other => panic!("{other:?}"),
        }
    }

    fn literal_cmp(code: crate::types::XmlTypeCode, a: &str, b: &str) -> Option<Ordering> {
        literal_cmp_in(false, &atom(code, a), a, &atom(code, b), b)
    }

    #[test]
    fn exact_fractions_order_and_complement() {
        let f = ExactFraction::from_digits;
        assert!(f("5") > f("49999"));
        assert!(f("5") < f("50001"));
        assert_eq!(f("5000"), f("5"));
        assert!(f("") < f("0000000000000000000000000000001"));
        assert!(f("").is_zero() && f("000").is_zero());
        for (x, one_minus_x) in [
            ("25", "75"),
            ("1", "9"),
            ("0001", "9999"),
            ("999", "001"),
            ("5", "5"),
        ] {
            assert_eq!(f(x).complement(), f(one_minus_x), "1 - 0.{x}");
        }
    }

    /// A pair with a literal whose seconds the typed value truncates is
    /// compared with the fractions as written — also where truncation alone
    /// would reverse the order: `PT60.000…05S` keeps 27 fraction digits (its
    /// 30-digit mantissa does not fit), `PT1M0.000…01S` all 28.
    #[test]
    fn literals_beyond_capacity_compare_as_written() {
        use crate::types::XmlTypeCode::{DateTime, DayTimeDuration, Duration, Time};
        use Ordering::{Equal, Greater, Less};
        const E30: &str = "PT0.000000000000000000000000000001S";
        let rows: &[(crate::types::XmlTypeCode, &str, &str, Option<Ordering>)] = &[
            (Duration, E30, "PT0S", Some(Greater)),
            (
                Duration,
                "-PT0.000000000000000000000000000001S",
                "PT0S",
                Some(Less),
            ),
            (
                Duration,
                "-PT0.000000000000000000000000000001S",
                "-PT0.000000000000000000000000000002S",
                Some(Greater),
            ),
            (
                Duration,
                E30,
                "PT0.0000000000000000000000000000010S",
                Some(Equal),
            ),
            (
                Duration,
                "PT60.0000000000000000000000000005S",
                "PT1M0.0000000000000000000000000001S",
                Some(Greater),
            ),
            (
                Duration,
                "P1MT0.000000000000000000000000000001S",
                "P30D",
                None,
            ),
            (
                Duration,
                "P1MT0.000000000000000000000000000001S",
                "P27D",
                Some(Greater),
            ),
            (
                DayTimeDuration,
                "P1DT0.000000000000000000000000000001S",
                "PT24H",
                Some(Greater),
            ),
            (
                DateTime,
                "2000-01-01T00:00:00.000000000000000000000000000001Z",
                "2000-01-01T00:00:00Z",
                Some(Greater),
            ),
            (
                DateTime,
                "2000-01-01T05:00:00.000000000000000000000000000001+05:00",
                "2000-01-01T00:00:00.0000000000000000000000000000010Z",
                Some(Equal),
            ),
            (
                DateTime,
                "1999-12-31T23:59:59.999999999999999999999999999999Z",
                "2000-01-01T00:00:00Z",
                Some(Less),
            ),
            (
                DateTime,
                "2000-01-01T00:00:00.000000000000000000000000000001Z",
                "2000-01-01T00:00:00",
                None,
            ),
            (
                DateTime,
                "2000-01-01T14:00:00.000000000000000000000000000001Z",
                "2000-01-01T00:00:00",
                Some(Greater),
            ),
            (
                Time,
                "12:00:00.000000000000000000000000000001+05:00",
                "07:00:00Z",
                Some(Greater),
            ),
        ];
        for &(code, a, b, expected) in rows {
            assert_eq!(literal_cmp(code, a, b), expected, "{a} vs {b}");
            assert_eq!(
                literal_cmp(code, b, a),
                expected.map(Ordering::reverse),
                "{b} vs {a}"
            );
            assert_eq!(literal_cmp(code, a, a), Some(Equal), "{a} vs itself");
        }
    }
}
