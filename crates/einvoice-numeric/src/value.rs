use crate::budget::{difference, overflow, product, sum};
use crate::{Budget, Error, ResourceCause, Result, RoundingMode};
use num_bigint::BigInt;
use num_traits::{ToPrimitive, Zero};
use std::cmp::Ordering;
use std::sync::Arc;

/// An exact normalized coefficient × 10^(-scale).
///
/// Coefficients are private, nonnegative and shared; the sign is separate so cloning,
/// negation and absolute value never copy a BigInt. Zero has scale zero and no sign.
/// Equality is allocation-free. Ordering deliberately requires an explicit budget.
#[derive(Clone, Eq, PartialEq)]
pub struct Decimal {
    magnitude: Arc<BigInt>,
    negative: bool,
    scale: i64,
}

impl std::fmt::Debug for Decimal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Decimal")
            .field("coefficient_bits", &self.magnitude.bits())
            .field("negative", &self.negative)
            .field("scale", &self.scale)
            .finish_non_exhaustive()
    }
}

impl Decimal {
    /// Parse `[+-]?([0-9]+(\.[0-9]*)?|\.[0-9]+)`, allowing XML whitespace at edges.
    pub fn parse_decimal(text: &str, budget: &mut Budget) -> Result<Self> {
        Self::parse(text, false, budget)
    }

    /// Parse `[+-]?[0-9]+`, allowing XML whitespace at edges.
    pub fn parse_integer(text: &str, budget: &mut Budget) -> Result<Self> {
        Self::parse(text, true, budget)
    }

    fn parse(text: &str, integer: bool, budget: &mut Budget) -> Result<Self> {
        if text.len() > budget.limits.input {
            return Err(Error::Resource(ResourceCause::InputBytes));
        }
        // Includes trimming, grammar validation and the later significant-digit scans.
        budget.charge(product(text.len() as u64, 8)?)?;
        let text = text.trim_matches([' ', '\t', '\r', '\n']);
        let negative = text.starts_with('-');
        let text = text.strip_prefix(['+', '-']).unwrap_or(text);
        let mut point = false;
        let mut fraction = 0u64;
        let mut digits = 0u64;
        let mut first = None;
        let mut last = 0;
        for (i, byte) in text.bytes().enumerate() {
            if byte == b'.' && !integer && !point {
                point = true;
            } else if byte.is_ascii_digit() {
                digits += 1;
                fraction += u64::from(point);
                if byte != b'0' {
                    first.get_or_insert(i);
                    last = i;
                }
            } else {
                return Err(Error::InvalidLexical);
            }
        }
        if digits == 0 {
            return Err(Error::InvalidLexical);
        }
        let lexical_scale = i64::try_from(fraction).map_err(|_| overflow())?;
        budget.scale(lexical_scale)?;
        let Some(first) = first else {
            return Self::zero(budget);
        };
        let trailing = text[last + 1..].bytes().filter(u8::is_ascii_digit).count();
        let scale =
            i64::try_from(i128::from(lexical_scale) - trailing as i128).map_err(|_| overflow())?;
        budget.scale(scale)?;
        let significant = text[first..=last]
            .bytes()
            .filter(u8::is_ascii_digit)
            .count();
        let bits = product(significant as u64, 4)?;
        budget.bits(bits)?;
        budget.charge(product(bits, bits)?)?;
        // The substrate's radix parser uses floating-point allocation estimates.
        // Build monotonically instead: every intermediate is below the preflight bound.
        let mut magnitude = BigInt::zero();
        for digit in text[first..=last].bytes().filter(u8::is_ascii_digit) {
            magnitude *= 10u8;
            magnitude += digit - b'0';
        }
        Ok(Self {
            magnitude: Arc::new(magnitude),
            negative,
            scale,
        })
    }

    /// Construct canonical zero, charging the small value allocation.
    pub fn zero(budget: &mut Budget) -> Result<Self> {
        budget.charge(1)?;
        Ok(Self {
            magnitude: Arc::new(BigInt::zero()),
            negative: false,
            scale: 0,
        })
    }

    /// Whether this value is zero.
    pub fn is_zero(&self) -> bool {
        self.magnitude.is_zero()
    }

    /// Sign as -1, 0, or 1.
    pub fn sign(&self) -> i8 {
        if self.is_zero() {
            0
        } else if self.negative {
            -1
        } else {
            1
        }
    }

    /// Normalized signed decimal scale (zero always has scale zero).
    pub fn normalized_scale(&self) -> i64 {
        self.scale
    }

    /// Whether the exact value is integral. Truncating quotients always satisfy this.
    pub fn is_integral(&self) -> bool {
        self.scale <= 0
    }

    /// Share the coefficient, reversing the sign of nonzero values without allocation.
    pub fn negated(&self) -> Self {
        Self {
            negative: !self.negative && !self.is_zero(),
            ..self.clone()
        }
    }

    /// Share the coefficient with a nonnegative sign, without allocation.
    pub fn abs(&self) -> Self {
        Self {
            negative: false,
            ..self.clone()
        }
    }

    fn check(&self, budget: &mut Budget) -> Result<()> {
        budget.bits(self.magnitude.bits())?;
        budget.scale(self.scale)?;
        budget.charge(self.magnitude.bits())
    }

    fn normalized(
        mut magnitude: BigInt,
        negative: bool,
        mut scale: i64,
        budget: &mut Budget,
    ) -> Result<Self> {
        budget.bits(magnitude.bits())?;
        budget.scale(scale)?;
        if magnitude.is_zero() {
            return Self::zero(budget);
        }
        loop {
            budget.charge(product(magnitude.bits().max(1), 2)?)?;
            if !(&magnitude % 10u8).is_zero() {
                break;
            }
            scale = scale.checked_sub(1).ok_or_else(overflow)?;
            budget.scale(scale)?;
            magnitude /= 10u8;
        }
        Ok(Self {
            magnitude: Arc::new(magnitude),
            negative,
            scale,
        })
    }

    fn aligned(&self, other: &Self, budget: &mut Budget) -> Result<(BigInt, BigInt, i64)> {
        let scale = self.scale.max(other.scale);
        let a = scaled(&self.magnitude, difference(scale, self.scale)?, budget)?;
        let b = scaled(&other.magnitude, difference(scale, other.scale)?, budget)?;
        Ok((a, b, scale))
    }

    /// Exact sum, with no decimal context or intermediate rounding.
    pub fn add(&self, other: &Self, budget: &mut Budget) -> Result<Self> {
        self.combine(other, false, budget)
    }

    /// Exact difference, with no decimal context or intermediate rounding.
    pub fn sub(&self, other: &Self, budget: &mut Budget) -> Result<Self> {
        self.combine(other, true, budget)
    }

    fn combine(&self, other: &Self, subtract: bool, budget: &mut Budget) -> Result<Self> {
        self.check(budget)?;
        other.check(budget)?;
        if other.is_zero() {
            return Ok(self.clone());
        }
        if self.is_zero() {
            return Ok(if subtract {
                other.negated()
            } else {
                other.clone()
            });
        }
        let (a, b, scale) = self.aligned(other, budget)?;
        let other_negative = other.negative ^ subtract;
        let adding = self.negative == other_negative;
        budget.bits(sum(a.bits().max(b.bits()), u64::from(adding))?)?;
        budget.charge(sum(a.bits(), b.bits())?)?;
        let (magnitude, negative) = if adding {
            (a + b, self.negative)
        } else if a >= b {
            (a - b, self.negative)
        } else {
            (b - a, other_negative)
        };
        Self::normalized(magnitude, negative, scale, budget)
    }

    /// Exact product, with checked signed scale arithmetic.
    pub fn mul(&self, other: &Self, budget: &mut Budget) -> Result<Self> {
        self.check(budget)?;
        other.check(budget)?;
        if self.is_zero() || other.is_zero() {
            return Self::zero(budget);
        }
        let scale = self.scale.checked_add(other.scale).ok_or_else(overflow)?;
        budget.scale(scale)?;
        budget.bits(sum(self.magnitude.bits(), other.magnitude.bits())?)?;
        budget.charge(product(self.magnitude.bits(), other.magnitude.bits())?)?;
        Self::normalized(
            &*self.magnitude * &*other.magnitude,
            self.negative ^ other.negative,
            scale,
            budget,
        )
    }

    /// Exact numeric ordering. May refuse an alignment exceeding the caller's budget.
    pub fn compare(&self, other: &Self, budget: &mut Budget) -> Result<Ordering> {
        self.check(budget)?;
        other.check(budget)?;
        if self.sign() != other.sign() {
            return Ok(self.sign().cmp(&other.sign()));
        }
        if self == other {
            return Ok(Ordering::Equal);
        }
        let (a, b, _) = self.aligned(other, budget)?;
        budget.charge(sum(a.bits(), b.bits())?)?;
        let order = a.cmp(&b);
        Ok(if self.negative {
            order.reverse()
        } else {
            order
        })
    }

    /// Exact truncating integer quotient. The result has normalized scale <= 0.
    /// This never calls rounded decimal division.
    pub fn integer_quotient(&self, other: &Self, budget: &mut Budget) -> Result<Self> {
        self.divisor(other, budget)?;
        if self.is_zero() {
            return Self::zero(budget);
        }
        let (a, b, _) = self.aligned(other, budget)?;
        division_work(&a, &b, budget)?;
        Self::normalized(a / b, self.negative ^ other.negative, 0, budget)
    }

    /// Exact signed remainder: `self = quotient * other + remainder`.
    /// A nonzero remainder has the dividend's sign and smaller magnitude than the divisor.
    pub fn remainder(&self, other: &Self, budget: &mut Budget) -> Result<Self> {
        self.divisor(other, budget)?;
        if self.is_zero() {
            return Self::zero(budget);
        }
        let (a, b, scale) = self.aligned(other, budget)?;
        division_work(&a, &b, budget)?;
        Self::normalized(a % b, self.negative, scale, budget)
    }

    fn divisor(&self, other: &Self, budget: &mut Budget) -> Result<()> {
        self.check(budget)?;
        other.check(budget)?;
        if other.is_zero() {
            Err(Error::DivideByZero)
        } else {
            Ok(())
        }
    }

    /// Round once to a signed decimal scale; the returned value remains normalized.
    pub fn quantize(&self, scale: i64, mode: RoundingMode, budget: &mut Budget) -> Result<Self> {
        self.check(budget)?;
        budget.scale(scale)?;
        if self.is_zero() || self.scale <= scale {
            return Ok(self.clone());
        }
        let shift = difference(self.scale, scale)?;
        // 10^shift > 2*magnitude follows from this deliberately loose binary bound.
        // Avoid a huge power for a result strictly below every nearest-mode midpoint.
        if shift > sum(self.magnitude.bits(), 1)? {
            return Self::zero(budget);
        }
        let denominator = power(shift, budget)?;
        let q = rounded_quotient(&self.magnitude, &denominator, self.negative, mode, budget)?;
        Self::normalized(q, self.negative, scale, budget)
    }

    /// Divide and round once at an explicit signed output scale and rounding mode.
    /// There is no default precision, Saxon policy, or deferred rational rounding.
    pub fn rounded_divide(
        &self,
        other: &Self,
        scale: i64,
        mode: RoundingMode,
        budget: &mut Budget,
    ) -> Result<Self> {
        self.divisor(other, budget)?;
        budget.scale(scale)?;
        if self.is_zero() {
            return Self::zero(budget);
        }
        let exponent = i128::from(other.scale) - i128::from(self.scale) + i128::from(scale);
        if exponent < 0 && exponent.unsigned_abs() > u128::from(sum(self.magnitude.bits(), 1)?) {
            // The nonzero denominator magnitude is at least one, so this quotient
            // is strictly below half a unit of the requested output scale.
            return Self::zero(budget);
        }
        let shift = u64::try_from(exponent.unsigned_abs()).map_err(|_| overflow())?;
        let (a, b) = if exponent >= 0 {
            (
                scaled(&self.magnitude, shift, budget)?,
                scaled(&other.magnitude, 0, budget)?,
            )
        } else {
            (
                scaled(&self.magnitude, 0, budget)?,
                scaled(&other.magnitude, shift, budget)?,
            )
        };
        let negative = self.negative ^ other.negative;
        let q = rounded_quotient(&a, &b, negative, mode, budget)?;
        Self::normalized(q, negative, scale, budget)
    }

    /// Render the exact plain decimal value, without rounding or exponent notation.
    pub fn to_canonical(&self, budget: &mut Budget) -> Result<String> {
        self.render(self.scale.max(0), budget)
    }

    /// Render at a fixed signed scale, refusing inexact output. Nonnegative scales
    /// retain exactly that many fractional places; negative scales require a multiple
    /// of 10^(-scale) and render it as a plain integer. Quantize explicitly first.
    pub fn to_fixed(&self, scale: i64, budget: &mut Budget) -> Result<String> {
        self.check(budget)?;
        budget.scale(scale)?;
        if !self.is_zero() && self.scale > scale {
            return Err(Error::InexactRendering);
        }
        self.render(scale.max(0), budget)
    }

    fn render(&self, fraction: i64, budget: &mut Budget) -> Result<String> {
        self.check(budget)?;
        // log10(2) < 30103/100000; integer-only conservative digit estimate.
        let digit_bound = sum(product(self.magnitude.bits(), 30103)? / 100000, 1)?;
        let integer_bound = (i128::from(digit_bound) - i128::from(self.scale)).max(1);
        let output_bound = u64::try_from(
            integer_bound
                + i128::from(fraction)
                + i128::from(fraction > 0)
                + i128::from(self.negative),
        )
        .map_err(|_| overflow())?;
        let capacity = budget.output(output_bound)?;
        budget.output(digit_bound)?;
        budget.charge(sum(
            product(self.magnitude.bits(), self.magnitude.bits())?,
            output_bound,
        )?)?;
        // Avoid the substrate's radix formatting (float capacity estimates and
        // divide-and-conquer power tables). All conversion intermediates shrink.
        let mut remainder = (*self.magnitude).clone();
        let mut digits = Vec::with_capacity(digit_bound as usize);
        loop {
            let digit = (&remainder % 10u8).to_u8().ok_or_else(overflow)?;
            digits.push(b'0' + digit);
            remainder /= 10u8;
            if remainder.is_zero() {
                break;
            }
        }
        digits.reverse();
        let integer_digits = digits.len() as i128 - i128::from(self.scale);
        let mut out = String::with_capacity(capacity);
        if self.negative {
            out.push('-');
        }
        if integer_digits <= 0 {
            out.push('0');
        } else {
            for i in 0..integer_digits as usize {
                out.push(digits.get(i).copied().unwrap_or(b'0') as char);
            }
        }
        if fraction > 0 {
            out.push('.');
            for i in 0..fraction {
                let index = integer_digits + i128::from(i);
                let byte = usize::try_from(index).ok().and_then(|i| digits.get(i));
                out.push(byte.copied().unwrap_or(b'0') as char);
            }
        }
        Ok(out)
    }
}

fn power(exponent: u64, budget: &mut Budget) -> Result<BigInt> {
    let bits = sum(product(exponent, 4)?, 1)?;
    budget.bits(bits)?;
    budget.charge(product(bits, bits)?)?;
    let exponent = u32::try_from(exponent).map_err(|_| overflow())?;
    Ok(BigInt::from(10u8).pow(exponent))
}

fn scaled(value: &BigInt, exponent: u64, budget: &mut Budget) -> Result<BigInt> {
    if exponent == 0 || value.is_zero() {
        budget.bits(value.bits())?;
        budget.charge(value.bits())?;
        return Ok(value.clone());
    }
    let power_bits = sum(product(exponent, 4)?, 1)?;
    budget.bits(sum(value.bits(), power_bits)?)?;
    // Check multiplication fuel before allocating even the power.
    budget.charge(product(value.bits(), power_bits)?)?;
    let factor = power(exponent, budget)?;
    Ok(value * factor)
}

fn division_work(a: &BigInt, b: &BigInt, budget: &mut Budget) -> Result<()> {
    budget.bits(a.bits().max(b.bits()))?;
    budget.charge(product(a.bits().max(1), b.bits().max(1))?)
}

fn rounded_quotient(
    a: &BigInt,
    b: &BigInt,
    negative: bool,
    mode: RoundingMode,
    budget: &mut Budget,
) -> Result<BigInt> {
    // Reserve a possible carry before division allocations, even if rounding is exact.
    budget.bits(sum(a.bits(), 1)?)?;
    division_work(a, b, budget)?;
    division_work(a, b, budget)?;
    let mut quotient = a / b;
    let remainder = a % b;
    if !remainder.is_zero() && mode != RoundingMode::TowardZero {
        budget.charge(product(b.bits().max(1), 3)?)?;
        // Compare r with b-r rather than allocating 2*r beyond the bit limit.
        let order = remainder.cmp(&(b - &remainder));
        let tie_up = match mode {
            RoundingMode::HalfAwayFromZero => true,
            RoundingMode::HalfTowardPositiveInfinity => !negative,
            RoundingMode::HalfEven => quotient.bit(0),
            RoundingMode::HalfTowardZero | RoundingMode::TowardZero => false,
        };
        if order.is_gt() || (order.is_eq() && tie_up) {
            budget.charge(sum(quotient.bits(), 1)?)?;
            quotient += 1u8;
        }
    }
    Ok(quotient)
}
