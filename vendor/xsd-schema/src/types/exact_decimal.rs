//! Exact `xs:decimal` values for validity decisions.
//!
//! XSD 1.1 Part 2 §3.3.3 defines the value space of `xs:decimal` as "the set
//! of numbers that can be obtained by dividing an integer by a non-negative
//! power of ten", and adds: "Precision is not reflected in this value space;
//! the number 2.0 is not distinct from the number 2.00." The space is
//! unbounded, while the crate's typed value, `rust_decimal::Decimal`, holds a
//! 96-bit mantissa with a scale of at most 28.
//!
//! [`ExactDecimal`] is the value itself, kept as its decimal digits, with no
//! limit. Every value-based check on `xs:decimal` and the types derived from
//! it — `totalDigits`, `fractionDigits`, the four bound facets, enumeration,
//! fixed/default value equality, identity-constraint field equality — is
//! decided on it whenever the `Decimal` cannot carry the value exactly, so a
//! value beyond that capacity is never rounded into a different verdict.
//!
//! A valid value that `Decimal` cannot hold exactly is carried as its
//! canonical lexical representation (Datatypes §3.3.3.2): an [`XmlValue`] whose
//! `type_code` is [`XmlTypeCode::Decimal`] and whose payload is
//! [`XmlAtomicValue::String`]. [`lexical_decimal`] recognises that shape. Datatypes §5.4 requires that such a value "must not be treated
//! as invalid, and the unsupported value must not be quietly changed to a
//! supported value", and that the processor "must indicate to the user and/or
//! downstream application that it cannot process the input data with assured
//! correctness" — the validator reports [`DECIMAL_CAPACITY_WARNING`] for it.

use std::cmp::Ordering;
use std::fmt;

use num_bigint::BigInt;
use rust_decimal::Decimal;

use super::value::{XmlAtomicValue, XmlValue, XmlValueKind};
use super::XmlTypeCode;

/// The warning code the validator reports, through
/// [`ValidationSink::on_warning`](crate::validation::ValidationSink::on_warning),
/// for an `xs:decimal` value it validated exactly but carries in lexical form.
pub(crate) const DECIMAL_CAPACITY_WARNING: &str = "xs-decimal-capacity";

/// Digits any `rust_decimal::Decimal` holds exactly: a mantissa of at most 28
/// digits is below 10^28 < 2^96, and a scale of at most 28 is in range.
pub(crate) const EXACT_DIGITS: usize = 28;

/// The largest scale `rust_decimal::Decimal` supports.
const MAX_SCALE: usize = 28;

/// The largest mantissa `rust_decimal::Decimal` supports, 2^96 − 1.
const MAX_MANTISSA: u128 = (1u128 << 96) - 1;

/// The digits of `Decimal::MAX` (2^96 − 1), the largest magnitude it holds.
const MAX_DECIMAL_DIGITS: &str = "79228162514264337593543950335";

/// An `xs:decimal` value of any size and precision.
///
/// Normalized, so derived equality is value equality: `int` has no leading
/// zeros (it is empty when the magnitude is below one), `frac` has no trailing
/// zeros, and zero is never negative.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExactDecimal {
    negative: bool,
    int: String,
    frac: String,
}

/// Why a string cannot become a `rust_decimal::Decimal` exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DecimalParseError {
    /// Not in the `xs:decimal` lexical space.
    Invalid,
    /// The integer part exceeds `Decimal::MAX`.
    TooLarge,
    /// The value is in range but needs more digits than `Decimal` holds.
    TooPrecise,
}

impl ExactDecimal {
    /// The value of an `xs:decimal` lexical form (Datatypes §3.3.3.1,
    /// `(\+|-)?([0-9]+(\.[0-9]*)?|\.[0-9]+)`), or `None` when `lexical` is not
    /// one. No whitespace is accepted; collapse it first.
    pub(crate) fn parse(lexical: &str) -> Option<Self> {
        let (negative, rest) = match lexical.as_bytes().first() {
            Some(b'-') => (true, &lexical[1..]),
            Some(b'+') => (false, &lexical[1..]),
            _ => (false, lexical),
        };
        let (int_part, frac_part) = match rest.find('.') {
            Some(dot) => (&rest[..dot], &rest[dot + 1..]),
            None => (rest, ""),
        };
        if int_part.is_empty() && frac_part.is_empty() {
            return None;
        }
        if !int_part.bytes().all(|b| b.is_ascii_digit())
            || !frac_part.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        Some(Self::from_parts(
            negative,
            int_part.trim_start_matches('0'),
            frac_part.trim_end_matches('0'),
        ))
    }

    fn from_parts(negative: bool, int: &str, frac: &str) -> Self {
        let is_zero = int.is_empty() && frac.is_empty();
        Self {
            negative: negative && !is_zero,
            int: int.to_owned(),
            frac: frac.to_owned(),
        }
    }

    /// The value of a `rust_decimal::Decimal`.
    pub(crate) fn from_decimal(value: &Decimal) -> Self {
        let digits = value.mantissa().unsigned_abs().to_string();
        let scale = value.scale() as usize;
        let negative = value.is_sign_negative();
        if scale == 0 {
            return Self::from_parts(negative, digits.trim_start_matches('0'), "");
        }
        if digits.len() > scale {
            let (int, frac) = digits.split_at(digits.len() - scale);
            Self::from_parts(
                negative,
                int.trim_start_matches('0'),
                frac.trim_end_matches('0'),
            )
        } else {
            let mut frac = "0".repeat(scale - digits.len());
            frac.push_str(&digits);
            Self::from_parts(negative, "", frac.trim_end_matches('0'))
        }
    }

    /// The value of an `xs:integer`.
    pub(crate) fn from_bigint(value: &BigInt) -> Self {
        let text = value.to_string();
        let (negative, digits) = match text.strip_prefix('-') {
            Some(digits) => (true, digits),
            None => (false, text.as_str()),
        };
        Self::from_parts(negative, digits.trim_start_matches('0'), "")
    }

    /// The exact value of a typed value of the `xs:decimal` family — a
    /// `Decimal` or `Integer` payload, or a decimal carried in lexical form
    /// (see [`lexical_decimal`]) — looking through union wrappers. `None` for
    /// anything else.
    pub(crate) fn from_value(value: &XmlValue) -> Option<Self> {
        match &value.value {
            XmlValueKind::Atomic(atom) => Self::from_atomic(value.type_code, atom),
            XmlValueKind::Union(inner) => Self::from_value(inner),
            _ => None,
        }
    }

    /// [`ExactDecimal::from_value`] for one atomic payload typed `type_code`
    /// (the payload of a value whose type code is known apart from it).
    pub(crate) fn from_atomic(type_code: XmlTypeCode, atom: &XmlAtomicValue) -> Option<Self> {
        match atom {
            XmlAtomicValue::Decimal(d) => Some(Self::from_decimal(d)),
            XmlAtomicValue::Integer(i) => Some(Self::from_bigint(i)),
            XmlAtomicValue::String(s) if type_code == XmlTypeCode::Decimal => Self::parse(s),
            _ => None,
        }
    }

    /// Whether the value is zero.
    pub(crate) fn is_zero(&self) -> bool {
        self.int.is_empty() && self.frac.is_empty()
    }

    /// Whether the value is not zero but its magnitude is below 10^−28, the
    /// smallest positive value a `rust_decimal::Decimal` holds — the first
    /// non-zero digit comes after the 28th fraction digit. 10^−28 itself is
    /// held.
    pub(crate) fn is_below_smallest_decimal(&self) -> bool {
        // `frac` has no trailing zeros, so a non-zero value whose first 28
        // fraction digits are zeros has a 29th digit or later that is not.
        self.int.is_empty()
            && self.frac.len() > MAX_SCALE
            && self.frac.as_bytes()[..MAX_SCALE].iter().all(|&b| b == b'0')
    }

    /// The smallest `t` for which the value satisfies a `totalDigits` facet of
    /// value `t` (Datatypes §4.3.11.3): the value is `i / 10^n` with
    /// `|i| < 10^t` and `0 ≤ n ≤ t`. With `n` the number of fraction digits,
    /// that is the digit count of `i` for a value of magnitude one or more,
    /// and `n` itself below one — `0.05` needs two digits, not one.
    pub(crate) fn total_digits(&self) -> usize {
        if self.int.is_empty() {
            self.frac.len().max(1)
        } else {
            self.int.len() + self.frac.len()
        }
    }

    /// The smallest `n` with the value equal to `i / 10^n` for an integer `i`
    /// (Datatypes §4.3.12.3).
    pub(crate) fn fraction_digits(&self) -> usize {
        self.frac.len()
    }

    /// The same value as a `rust_decimal::Decimal`, when one holds it exactly:
    /// `TooLarge` when the magnitude exceeds `Decimal::MAX`, `TooPrecise` when
    /// it is in range but needs a scale above 28 or a mantissa above 2^96 − 1.
    pub(crate) fn to_decimal(&self) -> Result<Decimal, DecimalParseError> {
        let above_max = match self.int.len().cmp(&MAX_DECIMAL_DIGITS.len()) {
            Ordering::Greater => true,
            Ordering::Less => false,
            Ordering::Equal => match self.int.as_str().cmp(MAX_DECIMAL_DIGITS) {
                Ordering::Greater => true,
                Ordering::Equal => !self.frac.is_empty(),
                Ordering::Less => false,
            },
        };
        if above_max {
            return Err(DecimalParseError::TooLarge);
        }
        if self.frac.len() > MAX_SCALE {
            return Err(DecimalParseError::TooPrecise);
        }
        let mut digits = String::with_capacity(self.int.len() + self.frac.len());
        digits.push_str(&self.int);
        digits.push_str(&self.frac);
        let digits = digits.trim_start_matches('0');
        // At most 29 digits fit a u128; a longer mantissa is above 2^96 − 1.
        if digits.len() > MAX_DECIMAL_DIGITS.len() {
            return Err(DecimalParseError::TooPrecise);
        }
        let mantissa = if digits.is_empty() {
            0
        } else {
            digits
                .parse::<u128>()
                .map_err(|_| DecimalParseError::TooPrecise)?
        };
        if mantissa > MAX_MANTISSA {
            return Err(DecimalParseError::TooPrecise);
        }
        // `mantissa` ≤ 2^96 − 1, so it converts to i128 without loss.
        let magnitude = mantissa as i128;
        let signed = if self.negative { -magnitude } else { magnitude };
        Decimal::try_from_i128_with_scale(signed, self.frac.len() as u32)
            .map_err(|_| DecimalParseError::TooPrecise)
    }

    /// The integer part (truncation toward zero), as F&O §17.1.3.4 casts a
    /// decimal to `xs:integer`.
    pub(crate) fn trunc_to_bigint(&self) -> BigInt {
        if self.int.is_empty() {
            return BigInt::from(0);
        }
        // `int` is a non-empty run of ASCII digits, so the parse cannot fail;
        // zero is the harmless fallback all the same.
        let magnitude = self.int.parse::<BigInt>().unwrap_or_default();
        if self.negative {
            -magnitude
        } else {
            magnitude
        }
    }

    /// The nearest `f64` (Rust's float parser rounds correctly), `±INF`
    /// beyond its range — F&O §17.1.3.2 casting `xs:decimal` to `xs:double`.
    pub(crate) fn to_f64(&self) -> f64 {
        self.to_string().parse().unwrap_or(f64::NAN)
    }

    /// The nearest `f32`, `±INF` beyond its range.
    #[cfg_attr(not(feature = "xsd11"), allow(dead_code))] // XPath only
    pub(crate) fn to_f32(&self) -> f32 {
        self.to_string().parse().unwrap_or(f32::NAN)
    }

    fn signum(&self) -> i8 {
        if self.is_zero() {
            0
        } else if self.negative {
            -1
        } else {
            1
        }
    }

    fn cmp_magnitude(&self, other: &Self) -> Ordering {
        // No leading zeros in `int`: a longer integer part is a larger number.
        // No trailing zeros in `frac`: a fraction that is a proper prefix of
        // another is the smaller one, which is what `str::cmp` answers.
        self.int
            .len()
            .cmp(&other.int.len())
            .then_with(|| self.int.cmp(&other.int))
            .then_with(|| self.frac.cmp(&other.frac))
    }
}

impl Ord for ExactDecimal {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.signum().cmp(&other.signum()) {
            Ordering::Equal if self.negative => other.cmp_magnitude(self),
            Ordering::Equal => self.cmp_magnitude(other),
            unequal => unequal,
        }
    }
}

impl PartialOrd for ExactDecimal {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for ExactDecimal {
    /// The canonical representation (Datatypes §3.3.3, `decimalCanonicalMap`):
    /// no decimal point for an integer, otherwise at least one digit on each
    /// side of it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.negative {
            f.write_str("-")?;
        }
        f.write_str(if self.int.is_empty() { "0" } else { &self.int })?;
        if !self.frac.is_empty() {
            write!(f, ".{}", self.frac)?;
        }
        Ok(())
    }
}

/// `value`, with a negative zero made zero.
///
/// `xs:decimal` has no negative zero (Datatypes §3.3.3: its value space is the
/// numbers `i / 10^n`, and the canonical form of zero is `0`), but
/// `rust_decimal` keeps the sign of a zero result — `(-0.5).trunc()`, `-0.0`,
/// `Decimal::try_from(-1.0e-40)`, the zero seconds of a negative duration — and
/// prints it as `-0`. Every `xs:decimal` an XPath operation produces goes
/// through this. The scale is kept.
#[inline]
pub(crate) fn unsigned_zero(mut value: Decimal) -> Decimal {
    if value.is_zero() {
        value.set_sign_positive(true);
    }
    value
}

/// Number of digit characters in a decimal lexical form.
#[inline]
fn digit_count(lexical: &str) -> usize {
    lexical.bytes().filter(u8::is_ascii_digit).count()
}

/// The `rust_decimal::Decimal` an `xs:decimal` lexical form denotes, when
/// one holds it exactly — never a rounded neighbour.
///
/// The common case — at most [`EXACT_DIGITS`] digits — is `rust_decimal`'s
/// own parser, which is exact there. A longer form goes through
/// [`ExactDecimal`], so `1.000…0` with forty zeros still becomes `1`, and
/// `0.1234…` with 29 significant digits is `TooPrecise`.
///
/// `lexical` must already be collapsed; it is not checked against the
/// lexical space beyond what [`ExactDecimal::parse`] requires, so callers
/// that need the strict gate apply
/// [`is_valid_xsd_decimal_lexical`](super::validators::is_valid_xsd_decimal_lexical)
/// first.
pub(crate) fn parse_decimal_exact(lexical: &str) -> Result<Decimal, DecimalParseError> {
    if digit_count(lexical) <= EXACT_DIGITS {
        if let Ok(d) = lexical.parse::<Decimal>() {
            return Ok(d);
        }
    }
    ExactDecimal::parse(lexical)
        .ok_or(DecimalParseError::Invalid)?
        .to_decimal()
}

/// The typed value of a valid, collapsed `xs:decimal` lexical form: a
/// `Decimal` payload when one holds the value exactly, otherwise the value's
/// canonical representation with the `xs:decimal` type code (see
/// [`lexical_decimal`]).
///
/// The payload is the value, not the instance's spelling: `+0100.500` and
/// `100.5` give the same typed value, as they do for a `Decimal` payload
/// (Datatypes §3.3.3: "the number 2.0 is not distinct from the number
/// 2.00"). It is `decimalCanonicalMap` of the value (§3.3.3.2) — what F&O
/// §17.1.2 returns when an `xs:decimal` is cast to `xs:string` — so every
/// conversion of the typed value to a string (`fn:string`, `xs:string`,
/// `xs:untypedAtomic`, [`XmlValue::to_string_value`]) is canonical without
/// further work. The instance's own spelling stays the PSVI
/// `[schema normalized value]`.
pub(crate) fn decimal_typed_value(lexical: &str) -> XmlValue {
    if digit_count(lexical) <= EXACT_DIGITS {
        if let Ok(d) = lexical.parse::<Decimal>() {
            return XmlValue::decimal(d);
        }
    }
    let payload = match ExactDecimal::parse(lexical) {
        Some(exact) => match exact.to_decimal() {
            Ok(d) => return XmlValue::decimal(d),
            Err(_) => exact.to_string(),
        },
        // Not a decimal lexical form: callers validate before they ask, so
        // this is not reached; keep the text rather than invent a value.
        None => lexical.to_owned(),
    };
    XmlValue::new(
        XmlTypeCode::Decimal,
        XmlValueKind::Atomic(XmlAtomicValue::String(payload)),
    )
}

/// The canonical representation of an `xs:decimal` value that the crate
/// carries as a string because `rust_decimal::Decimal` cannot hold it exactly
/// (see [`decimal_typed_value`]), looking through union wrappers; `None` for
/// every other value.
pub(crate) fn lexical_decimal(value: &XmlValue) -> Option<&str> {
    match &value.value {
        XmlValueKind::Atomic(XmlAtomicValue::String(s))
            if value.type_code == XmlTypeCode::Decimal =>
        {
            Some(s)
        }
        XmlValueKind::Union(inner) => lexical_decimal(inner),
        _ => None,
    }
}

/// The first `xs:decimal` value inside `value` — the value itself, a union's
/// member value, or a list member — that is carried in lexical form. A list
/// member carries the type that validated it, so a member of a list whose
/// item type is a union counts exactly when the union's decimal member type
/// accepted it.
pub(crate) fn first_lexical_decimal(value: &XmlValue) -> Option<&str> {
    match &value.value {
        XmlValueKind::Atomic(_) => lexical_decimal(value),
        XmlValueKind::Union(inner) => first_lexical_decimal(inner),
        XmlValueKind::List { items, .. } => items.iter().find_map(first_lexical_decimal),
        XmlValueKind::UntypedAtomic(_) => None,
    }
}

/// Value equality of two `xs:decimal`-family values when at least one of them
/// is not a plain `Decimal`/`Integer` pair that `PartialEq` already compares
/// by value; `None` when either is not of the family.
#[cfg_attr(not(feature = "xsd11"), allow(dead_code))] // XPath only
pub(crate) fn decimal_values_equal(a: &XmlValue, b: &XmlValue) -> Option<bool> {
    Some(ExactDecimal::from_value(a)? == ExactDecimal::from_value(b)?)
}

/// The order of two `xs:decimal`-family values, exactly; `None` when either is
/// not of the family.
#[cfg_attr(not(feature = "xsd11"), allow(dead_code))] // XPath only
pub(crate) fn decimal_values_cmp(a: &XmlValue, b: &XmlValue) -> Option<Ordering> {
    Some(ExactDecimal::from_value(a)?.cmp(&ExactDecimal::from_value(b)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn x(s: &str) -> ExactDecimal {
        ExactDecimal::parse(s).unwrap_or_else(|| panic!("{s} parses"))
    }

    #[test]
    fn parse_normalizes() {
        assert_eq!(x("+001.2300"), x("1.23"));
        assert_eq!(x("-0.0"), x("0"));
        assert_eq!(x(".5"), x("0.5"));
        assert_eq!(x("5."), x("5"));
        assert_eq!(x("-000"), x("+0.000"));
        assert_eq!(x("1.0").to_string(), "1");
        assert_eq!(x("-0.050").to_string(), "-0.05");
        assert_eq!(x("1000").to_string(), "1000");
        for bad in ["", "+", "-", ".", "1e5", "1_0", "1.2.3", " 1", "NaN", "--1"] {
            assert!(ExactDecimal::parse(bad).is_none(), "{bad:?} must not parse");
        }
    }

    #[test]
    fn order_is_numeric() {
        let ordered = [
            "-100000000000000000000000000000000000001",
            "-100000000000000000000000000000000000000",
            "-1.5",
            "-1.05",
            "-0.000000000000000000000000000000001",
            "0",
            "0.000000000000000000000000000000001",
            "0.05",
            "0.5",
            "0.51",
            "1",
            "1.00000000000000000000000000001",
            "79228162514264337593543950335",
            "79228162514264337593543950336",
            "100000000000000000000000000000000000000",
        ];
        for (i, a) in ordered.iter().enumerate() {
            for (j, b) in ordered.iter().enumerate() {
                assert_eq!(x(a).cmp(&x(b)), i.cmp(&j), "{a} vs {b}");
            }
        }
    }

    #[test]
    fn digit_counts_follow_the_facet_definitions() {
        // §4.3.11.3: i / 10^n with |i| < 10^t and 0 ≤ n ≤ t.
        assert_eq!(x("0.05").total_digits(), 2);
        assert_eq!(x("0.5").total_digits(), 1);
        assert_eq!(x("0").total_digits(), 1);
        assert_eq!(x("1000").total_digits(), 4);
        assert_eq!(x("12.340").total_digits(), 4);
        assert_eq!(x("-0.00120").total_digits(), 4);
        assert_eq!(x("12.340").fraction_digits(), 2);
        assert_eq!(x("12").fraction_digits(), 0);
        assert_eq!(x("0.12345678901234567890123456789").fraction_digits(), 29);
    }

    #[test]
    fn to_decimal_is_exact_or_refuses() {
        let max = "79228162514264337593543950335";
        assert_eq!(x(max).to_decimal(), Ok(Decimal::MAX));
        assert_eq!(
            x("-79228162514264337593543950335").to_decimal(),
            Ok(Decimal::MIN)
        );
        assert_eq!(
            x("79228162514264337593543950336").to_decimal(),
            Err(DecimalParseError::TooLarge)
        );
        assert_eq!(
            x("10000000000000000000000000000000000000000").to_decimal(),
            Err(DecimalParseError::TooLarge)
        );
        assert_eq!(
            x("0.12345678901234567890123456789").to_decimal(),
            Err(DecimalParseError::TooPrecise)
        );
        assert_eq!(
            x("1.00000000000000000000000000001").to_decimal(),
            Err(DecimalParseError::TooPrecise)
        );
        assert_eq!(
            x("0.0000000000000000000000000000001").to_decimal(),
            Err(DecimalParseError::TooPrecise)
        );
        assert_eq!(
            x("7922816251426433759354395033.5").to_decimal(),
            Ok(Decimal::from_str("7922816251426433759354395033.5").unwrap())
        );
        assert_eq!(
            x("79228162514264337593543950335.5").to_decimal(),
            Err(DecimalParseError::TooLarge)
        );
        assert_eq!(
            x("79228162514264337593543950334.5").to_decimal(),
            Err(DecimalParseError::TooPrecise)
        );
        assert_eq!(
            x("7922816251426433759354395034.0001").to_decimal(),
            Err(DecimalParseError::TooPrecise)
        );
        assert_eq!(
            x("1.0000000000000000000000000000000000000000").to_decimal(),
            Ok(Decimal::ONE)
        );
        assert_eq!(
            x("0.0000000000000000000000000001").to_decimal(),
            Ok(Decimal::new(1, 28))
        );
        assert_eq!(x("-0").to_decimal(), Ok(Decimal::ZERO));
    }

    #[test]
    fn decimal_round_trip() {
        for s in [
            "0",
            "-1.5",
            "0.0000000000000000000000000001",
            "79228162514264337593543950335",
            "-7.9228162514264337593543950335",
            "123.4500",
        ] {
            let d = Decimal::from_str(s).unwrap();
            assert_eq!(ExactDecimal::from_decimal(&d), x(s), "{s}");
        }
        assert_eq!(ExactDecimal::from_bigint(&BigInt::from(-1200)), x("-1200"));
        assert_eq!(ExactDecimal::from_bigint(&BigInt::from(0)), x("0"));
    }

    #[test]
    fn parse_decimal_exact_never_rounds() {
        assert_eq!(parse_decimal_exact("1.5"), Ok(Decimal::new(15, 1)));
        assert_eq!(
            parse_decimal_exact("0.12345678901234567890123456789"),
            Err(DecimalParseError::TooPrecise)
        );
        assert_eq!(
            parse_decimal_exact("79228162514264337593543950336"),
            Err(DecimalParseError::TooLarge)
        );
        assert_eq!(parse_decimal_exact("1e5"), Err(DecimalParseError::Invalid));
    }

    #[test]
    fn below_the_smallest_decimal() {
        let z27 = "0".repeat(27);
        let z28 = "0".repeat(28);
        for below in [
            format!("0.{z28}1"),
            format!("-0.{z28}1"),
            format!("0.{z28}9999999999"),
            format!(".{z28}1000"),
            format!("0.{z28}00001"),
        ] {
            assert!(x(&below).is_below_smallest_decimal(), "{below}");
        }
        for held_or_not_tiny in [
            "0".to_string(),
            "-0.0".to_string(),
            format!("0.{z27}1"),
            format!("0.{z27}100000"),
            format!("-0.{z27}15"),
            format!("1.{z28}1"),
            "0.12345678901234567890123456789".to_string(),
        ] {
            assert!(
                !x(&held_or_not_tiny).is_below_smallest_decimal(),
                "{held_or_not_tiny}"
            );
        }
        assert_eq!(
            x(&format!("0.{z27}1")).to_decimal(),
            Ok(Decimal::new(1, 28))
        );
    }

    #[test]
    fn typed_value_shapes() {
        let small = decimal_typed_value("1.50");
        assert_eq!(small.as_decimal(), Some(Decimal::new(150, 2)));
        assert_eq!(lexical_decimal(&small), None);
        let big = decimal_typed_value("+10000000000000000000000000000000000000000");
        assert_eq!(big.type_code, XmlTypeCode::Decimal);
        assert_eq!(big.as_decimal(), None);
        // The payload is the canonical representation (Datatypes §3.3.3.2),
        // not the instance's spelling — what F&O §17.1.2 casts to xs:string.
        assert_eq!(
            lexical_decimal(&big),
            Some("10000000000000000000000000000000000000000")
        );
        assert_eq!(
            big.to_string_value(),
            "10000000000000000000000000000000000000000"
        );
        for (spelling, canonical) in [
            (
                "+0010000000000000000000000000000000000000000.500",
                "10000000000000000000000000000000000000000.5",
            ),
            (
                "-.000000000000000000000000000000500",
                "-0.0000000000000000000000000000005",
            ),
            (
                "0.123456789012345678901234567890",
                "0.12345678901234567890123456789",
            ),
            (
                "79228162514264337593543950336.",
                "79228162514264337593543950336",
            ),
        ] {
            let value = decimal_typed_value(spelling);
            assert_eq!(lexical_decimal(&value), Some(canonical), "{spelling}");
            // Structural equality is value equality for these payloads.
            assert_eq!(value, decimal_typed_value(canonical), "{spelling}");
        }
        // A string that merely looks numeric is not an xs:decimal.
        assert_eq!(lexical_decimal(&XmlValue::string("1")), None);
        let same = decimal_typed_value("10000000000000000000000000000000000000000.000");
        assert_eq!(decimal_values_equal(&big, &same), Some(true));
        assert_eq!(
            decimal_values_equal(&big, &decimal_typed_value("1")),
            Some(false)
        );
        assert_eq!(
            decimal_values_cmp(&big, &XmlValue::integer(BigInt::from(1))),
            Some(Ordering::Greater)
        );
        assert_eq!(decimal_values_equal(&big, &XmlValue::string("1")), None);
    }
}
