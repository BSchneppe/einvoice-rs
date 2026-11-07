//! IEEE operations belong only to XPath binary types and their required conversions.
use crate::error::size_overflow;
use crate::value::{Integral, Value};
use crate::{Error, Failure, Fault, Result, Scope};
use einvoice_numeric::{Decimal, RoundingMode};
use std::fmt::Write;

pub(crate) struct Stack {
    bytes: [u8; 128],
    len: usize,
}
impl Stack {
    pub fn new() -> Self {
        Self {
            bytes: [0; 128],
            len: 0,
        }
    }
    pub fn as_str(&self) -> Result<&str> {
        std::str::from_utf8(&self.bytes[..self.len])
            .map_err(|_| Error::Fatal(Failure::Internal("non-UTF8 binary formatting")))
    }
}
impl Write for Stack {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let end = self.len.checked_add(s.len()).ok_or(std::fmt::Error)?;
        let dest = self.bytes.get_mut(self.len..end).ok_or(std::fmt::Error)?;
        dest.copy_from_slice(s.as_bytes());
        self.len = end;
        Ok(())
    }
}
fn formatting() -> Error {
    Error::Fatal(Failure::Internal("binary stack formatting capacity"))
}

/// Grammar checked before the bounded standard-library parser. Pinned Saxon parses
/// lexical float through binary64 then narrows, including its double-rounding edge.
pub(crate) fn parse(text: &str, single: bool) -> Result<Value> {
    let s = text.trim_matches([' ', '\t', '\r', '\n']);
    let x = match s {
        "INF" | "+INF" => f64::INFINITY,
        "-INF" => f64::NEG_INFINITY,
        "NaN" => f64::NAN,
        _ => {
            // Pinned StringToDouble's short-input fallback reaches Java's
            // <= U+0020 edge trimming. At nine codepoints it uses the stricter
            // grammar. INF/NaN recognition above uses XML whitespace only.
            let s = if text.chars().take(9).count() < 9 {
                s.trim_matches(|c| c <= '\u{20}')
            } else {
                s
            };
            let s1 = s.strip_prefix(['+', '-']).unwrap_or(s);
            let mut parts = s1.split(['e', 'E']);
            let m = parts.next().unwrap_or("");
            let e = parts.next();
            if parts.next().is_some() {
                return Err(Error::XPath(Fault::Forg0001));
            }
            let mut seen_digit = false;
            let mut seen_point = false;
            for byte in m.bytes() {
                if byte.is_ascii_digit() {
                    seen_digit = true;
                } else if byte == b'.' && !seen_point {
                    seen_point = true;
                } else {
                    return Err(Error::XPath(Fault::Forg0001));
                }
            }
            if !seen_digit {
                return Err(Error::XPath(Fault::Forg0001));
            }
            if let Some(e) = e {
                let e = e.strip_prefix(['+', '-']).unwrap_or(e);
                if e.is_empty() || !e.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(Error::XPath(Fault::Forg0001));
                }
            }
            s.parse::<f64>()
                .map_err(|_| Error::XPath(Fault::Forg0001))?
        }
    };
    Ok(if single {
        Value::Float(x as f32)
    } else {
        Value::Double(x)
    })
}

impl Scope {
    pub(crate) fn small_integer(&mut self, n: u64) -> Result<Decimal> {
        self.ledger.work(128)?;
        let mut s = Stack::new();
        write!(s, "{n}").map_err(|_| formatting())?;
        Ok(Decimal::parse_integer(s.as_str()?, &mut self.numeric)?)
    }
    fn pow2(&mut self, mut n: u32) -> Result<Decimal> {
        let mut a = self.small_integer(1)?;
        let mut f = self.small_integer(2)?;
        while n > 0 {
            self.ledger.work(1)?;
            if n & 1 != 0 {
                a = a.mul(&f, &mut self.numeric)?;
            }
            n >>= 1;
            if n > 0 {
                f = f.mul(&f, &mut self.numeric)?;
            }
        }
        Ok(a)
    }
    /// Decompose binary64 exactly. A promoted binary32 is exact in binary64 too.
    pub(crate) fn exact_binary(&mut self, x: f64) -> Result<Decimal> {
        self.ledger.work(1)?;
        if !x.is_finite() {
            return Err(Error::XPath(Fault::Foca0002));
        }
        if x == 0.0 {
            return Ok(Decimal::zero(&mut self.numeric)?);
        }
        let bits = x.to_bits();
        let field = ((bits >> 52) & 2047) as i32;
        let frac = bits & ((1u64 << 52) - 1);
        let (mantissa, exponent) = if field == 0 {
            (frac, -1074)
        } else {
            (frac | (1u64 << 52), field - 1023 - 52)
        };
        let a = self.small_integer(mantissa)?;
        let a = if x.is_sign_negative() { a.negated() } else { a };
        if exponent >= 0 {
            Ok(a.mul(&self.pow2(exponent as u32)?, &mut self.numeric)?)
        } else {
            let p = self.pow2((-exponent) as u32)?;
            Ok(a.rounded_divide(
                &p,
                i64::from(-exponent),
                RoundingMode::TowardZero,
                &mut self.numeric,
            )?)
        }
    }
    /// Pinned Java/Saxon conversion profile, distinct from an exact xs:decimal
    /// cast. Generate rounded decimal candidates from exact bits, and select the
    /// nearest shortest candidate that round-trips to binary64 (midpoints allowed).
    /// No binary display spelling is parsed into a Decimal.
    pub(crate) fn saxon_binary_candidate(&mut self, x: f64) -> Result<Decimal> {
        if !x.is_finite() {
            return Err(Error::XPath(Fault::Foca0002));
        }
        if x == 0.0 {
            return Ok(Decimal::zero(&mut self.numeric)?);
        }
        self.ledger.work(128)?;
        let exact = self.exact_binary(x)?;
        let mut seed = Stack::new();
        write!(seed, "{x:e}").map_err(|_| formatting())?;
        let exponent: i64 = seed
            .as_str()?
            .split_once('e')
            .ok_or_else(formatting)?
            .1
            .parse()
            .map_err(|_| formatting())?;
        for digits in 2..=17 {
            self.ledger.work(1)?;
            let scale = i64::from(digits) - 1 - exponent;
            let candidate = exact.quantize(scale, RoundingMode::HalfEven, &mut self.numeric)?;
            let value = self.convert_binary(&Value::Decimal(candidate.clone()), false)?;
            if let Value::Double(back) = value {
                if back.to_bits() == x.to_bits() {
                    return Ok(candidate);
                }
            }
        }
        Err(Error::Fatal(Failure::Capability(
            "binary integer profile needs more digits",
        )))
    }
    /// Saxon IntegerValue.fromDouble has an inclusive signed-64 threshold and
    /// Java's saturating long cast, then a shortest-decimal large-integer profile.
    pub(crate) fn binary_integer(&mut self, x: f64) -> Result<Decimal> {
        if !x.is_finite() {
            return Err(Error::XPath(Fault::Foca0002));
        }
        if (-9223372036854775808.0..=9223372036854775808.0).contains(&x) {
            let n = x as i64;
            let d = self.small_integer(n.unsigned_abs())?;
            return Ok(if n < 0 { d.negated() } else { d });
        }
        let d = self.saxon_binary_candidate(x)?;
        Ok(d.quantize(0, RoundingMode::TowardZero, &mut self.numeric)?)
    }
    /// Reserve a conservative kernel text buffer before its allocation. This is
    /// intentionally conservative because the kernel exposes no coefficient limbs.
    pub(crate) fn decimal_text(&mut self, d: &Decimal) -> Result<String> {
        let digits = self
            .ledger
            .limits
            .numeric_slot
            .checked_mul(3)
            .ok_or_else(size_overflow)?;
        let scale =
            usize::try_from(d.normalized_scale().unsigned_abs()).map_err(|_| size_overflow())?;
        let bound = digits
            .checked_add(scale)
            .and_then(|n| n.checked_add(4))
            .ok_or_else(size_overflow)?;
        self.ledger.string(bound)?;
        Ok(d.to_canonical(&mut self.numeric)?)
    }
    pub(crate) fn convert_binary(&mut self, v: &Value, single: bool) -> Result<Value> {
        match v {
            Value::Float(x) => Ok(if single {
                Value::Float(*x)
            } else {
                Value::Double(f64::from(*x))
            }),
            Value::Double(x) => Ok(if single {
                Value::Float(*x as f32)
            } else {
                Value::Double(*x)
            }),
            Value::Integer(Integral(d)) | Value::Decimal(d) => {
                // Pinned decimal->float goes through binary64, not a direct f32 parse.
                let s = self.decimal_text(d)?;
                parse(&s, single)
            }
            Value::Boolean(b) => Ok(if single {
                Value::Float(u8::from(*b) as f32)
            } else {
                Value::Double(f64::from(u8::from(*b)))
            }),
            Value::String(s) | Value::UntypedAtomic(s) => {
                self.ledger.input(s.len())?;
                parse(s, single)
            }
        }
    }

    /// Pinned decimal interval rendering, with at least two significant positions
    /// in scientific form. Normal values use Saxon's symmetric exponent margin,
    /// even where actual predecessor spacing differs. This compatibility policy
    /// need not round-trip at powers of two. All arithmetic uses the exact kernel.
    pub(crate) fn binary_text(&mut self, x: f64, single: bool) -> Result<String> {
        self.ledger.work(512)?;
        self.ledger.string(128)?;
        if x.is_nan() {
            return Ok("NaN".into());
        }
        if x.is_infinite() {
            return Ok(if x.is_sign_negative() { "-INF" } else { "INF" }.into());
        }
        if x == 0.0 {
            return Ok(if x.is_sign_negative() { "-0" } else { "0" }.into());
        }
        let abs = x.abs();
        let scientific = if single {
            !(1e-6f32..1e6f32).contains(&(abs as f32))
        } else {
            !(1e-6f64..1e6f64).contains(&abs)
        };
        let mut seed = Stack::new();
        if single {
            write!(seed, "{:e}", abs as f32)
        } else {
            write!(seed, "{abs:e}")
        }
        .map_err(|_| formatting())?;
        let mut exponent: i64 = seed
            .as_str()?
            .split_once('e')
            .ok_or_else(formatting)?
            .1
            .parse()
            .map_err(|_| formatting())?;
        let (previous, next) = if single {
            let b = (abs as f32).to_bits();
            (
                f64::from(f32::from_bits(b - 1)),
                f64::from(f32::from_bits(b + 1)),
            )
        } else {
            let b = abs.to_bits();
            (f64::from_bits(b - 1), f64::from_bits(b + 1))
        };
        let exact = self.exact_binary(abs)?;
        let normal = if single {
            (abs as f32).is_normal()
        } else {
            abs.is_normal()
        };
        if !normal {
            // Saxon delegates subnormals to the pinned JDK formatter. Its
            // closest one-or-two-digit selection starts at the exact decade,
            // which a shortest Rust seed can cross by rounding (for example
            // a value just below a decimal power). Charge the exact text and
            // scan; no second arithmetic representation or value table.
            let plain = self.decimal_text(&exact)?;
            self.ledger
                .work(u64::try_from(plain.len()).map_err(|_| size_overflow())?)?;
            let fraction = plain.strip_prefix("0.").ok_or_else(formatting)?;
            let first = fraction
                .bytes()
                .position(|b| b != b'0')
                .ok_or_else(formatting)?;
            exponent = i64::try_from(first)
                .map_err(|_| size_overflow())?
                .checked_add(1)
                .and_then(i64::checked_neg)
                .ok_or_else(size_overflow)?;
        }
        let prev = self.exact_binary(previous)?;
        let two = self.small_integer(2)?;
        let mid = |a: &Decimal, b: &Decimal, scope: &mut Scope| -> Result<Decimal> {
            let sum = a.add(b, &mut scope.numeric)?;
            let scale = sum
                .normalized_scale()
                .max(0)
                .checked_add(1)
                .ok_or_else(size_overflow)?;
            Ok(sum.rounded_divide(&two, scale, RoundingMode::TowardZero, &mut scope.numeric)?)
        };
        let mut lower = mid(&exact, &prev, self)?;
        let upper = if next.is_finite() {
            let next = self.exact_binary(next)?;
            mid(&exact, &next, self)?
        } else {
            let gap = exact.sub(&prev, &mut self.numeric)?;
            let next = exact.add(&gap, &mut self.numeric)?;
            mid(&exact, &next, self)?
        };
        // The pinned normal converter initializes symmetric margins
        // from the exponent's ULP, unlike actual predecessor distance at 2^n.
        if normal {
            let radius = upper.sub(&exact, &mut self.numeric)?;
            lower = exact.sub(&radius, &mut self.numeric)?;
        }
        let rounding = if normal {
            RoundingMode::HalfTowardZero
        } else {
            RoundingMode::HalfEven
        };
        // Subnormal round-trip midpoints belong to an even binary significand.
        let inclusive = !normal
            && if single {
                (abs as f32).to_bits() & 1 == 0
            } else {
                abs.to_bits() & 1 == 0
            };
        for n in if scientific { 2 } else { 1 }..=if single { 9 } else { 17 } {
            self.ledger.work(1)?;
            let scale = i64::from(n) - 1 - exponent;
            let rounded = exact.quantize(scale, rounding, &mut self.numeric)?;
            let lower_order = rounded.compare(&lower, &mut self.numeric)?;
            let upper_order = rounded.compare(&upper, &mut self.numeric)?;
            if (lower_order.is_gt() || inclusive && lower_order.is_eq())
                && (upper_order.is_lt() || inclusive && upper_order.is_eq())
            {
                let plain = self.decimal_text(&rounded)?;
                if !scientific {
                    return Ok(if x.is_sign_negative() {
                        format!("-{plain}")
                    } else {
                        plain
                    });
                }
                // Normal Saxon formatting retains the seed exponent. The JDK
                // subnormal route instead normalizes the selected candidate,
                // including when its rounding crosses a decimal decade.
                self.ledger.string(plain.len())?; // Bounded digit scratch before allocation.
                let digits: Vec<u8> = plain.bytes().filter(|b| *b != b'.').collect();
                let point = plain.find('.').unwrap_or(plain.len()) as i64;
                let first = digits
                    .iter()
                    .position(|b| *b != b'0')
                    .ok_or_else(formatting)? as i64;
                let actual_exponent = point - first - 1;
                let exponent = if normal { exponent } else { actual_exponent };
                // Exclude integer-padding zeros before writing: a bounded
                // final spelling must never transiently copy a 309-digit plain
                // integer into its 128-byte buffer and shrink afterwards.
                let last = digits
                    .iter()
                    .rposition(|b| *b != b'0')
                    .ok_or_else(formatting)?;
                let significant = &digits[first as usize..=last];
                let mut out = Stack::new();
                if x.is_sign_negative() {
                    out.write_char('-').map_err(|_| formatting())?;
                }
                if actual_exponent < exponent {
                    out.write_str("0.").map_err(|_| formatting())?;
                    for _ in 0..exponent - actual_exponent - 1 {
                        out.write_char('0').map_err(|_| formatting())?;
                    }
                    for &b in significant {
                        out.write_char(b as char).map_err(|_| formatting())?;
                    }
                } else {
                    out.write_char(significant[0] as char)
                        .map_err(|_| formatting())?;
                    out.write_char('.').map_err(|_| formatting())?;
                    for &b in &significant[1..] {
                        out.write_char(b as char).map_err(|_| formatting())?;
                    }
                }
                if out.as_str()?.ends_with('.') {
                    out.write_char('0').map_err(|_| formatting())?;
                }
                write!(out, "E{exponent}").map_err(|_| formatting())?;
                return Ok(out.as_str()?.to_owned());
            }
        }
        Err(Error::Fatal(Failure::Capability(
            "binary canonical interval requires more digits",
        )))
    }
}

#[cfg(test)]
mod admission_tests {
    #[test]
    fn scientific_buffer_never_grows_beyond_its_admitted_payload() {
        let limits = crate::Limits::new(
            4096,
            65536,
            8192,
            100_000_000_000,
            100_000_000,
            64_000_000,
            16_000_000,
            1024,
            32_000_000,
        )
        .unwrap();
        let mut scope = crate::Scope::new(limits);
        for value in [1e100, -1e100, 1e200, -1e200, 1e308, -1e308] {
            let text = scope.binary_text(value, false).unwrap();
            assert!(text.len() <= 128);
            assert!(
                text.capacity() <= 128,
                "binary output retained a grown scratch buffer"
            );
        }
    }

    #[test]
    fn grammar_uses_presence_without_input_sized_counters() {
        let zeroes = "0".repeat(4096);
        assert!(super::parse(&zeroes, false).is_ok());
        for bad in [".", "0.0.0", "..", "1e", "1e+", "1e2e3"] {
            assert!(matches!(
                super::parse(bad, false),
                Err(crate::Error::XPath(crate::Fault::Forg0001))
            ));
        }
        // Exercise accepted large bounds without allocating their input payload.
        #[cfg(target_pointer_width = "64")]
        for input in [1usize << 31, 1usize << 32] {
            crate::Limits::new(
                input,
                4096,
                8192,
                1_000_000,
                10_000_000_000,
                1_000_000,
                1_000_000,
                4,
                4096,
            )
            .unwrap();
        }
    }
}
