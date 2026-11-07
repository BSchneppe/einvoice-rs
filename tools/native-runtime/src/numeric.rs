use crate::error::size_overflow;
use crate::value::{Integral, Value};
use crate::{Atomic, AtomicType, Error, Failure, Fault, Result, Scope};
use einvoice_numeric::RoundingMode;
use std::cmp::Ordering;

/// Scalar arithmetic after numeric/untyped arithmetic conversion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Arithmetic {
    /// Addition.
    Add,
    /// Subtraction.
    Subtract,
    /// Multiplication.
    Multiply,
    /// Pinned Saxon division (exact branch produces Decimal).
    Divide,
    /// Truncating integer quotient.
    IntegerDivide,
    /// Dividend-sign remainder.
    Modulo,
}
/// Six numeric relations; NaN is unordered, independently of any sorting policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Comparison {
    /// Equal.
    Equal,
    /// Unequal.
    NotEqual,
    /// Less than.
    Less,
    /// Less than or equal.
    LessEqual,
    /// Greater than.
    Greater,
    /// Greater than or equal.
    GreaterEqual,
}
impl Scope {
    fn arithmetic_value(&mut self, v: &Value) -> Result<Value> {
        match v {
            Value::UntypedAtomic(_) => self.convert_binary(v, false),
            Value::String(_) | Value::Boolean(_) => Err(Error::XPath(Fault::Xpty0004)),
            _ => Ok(v.clone()),
        }
    }
    fn promoted(&mut self, a: &Value, b: &Value) -> Result<(Value, Value)> {
        if matches!(a, Value::Double(_)) || matches!(b, Value::Double(_)) {
            Ok((
                self.convert_binary(a, false)?,
                self.convert_binary(b, false)?,
            ))
        } else if matches!(a, Value::Float(_)) || matches!(b, Value::Float(_)) {
            Ok((self.convert_binary(a, true)?, self.convert_binary(b, true)?))
        } else if a.exact().is_some() && b.exact().is_some() {
            Ok((a.clone(), b.clone()))
        } else {
            Err(Error::XPath(Fault::Xpty0004))
        }
    }
    /// Numeric arithmetic with XML-untyped arithmetic conversion, not string casts.
    /// Integer/Decimal operations exclusively use the merged exact numeric kernel.
    pub fn arithmetic(&mut self, op: Arithmetic, a: &Atomic, b: &Atomic) -> Result<Atomic> {
        self.check(a)?;
        self.check(b)?;
        let lease = self.slot()?; // Reserve result retention before conversion/allocation.
        let a = self.arithmetic_value(&a.value)?;
        let b = self.arithmetic_value(&b.value)?;
        let (a, b) = self.promoted(&a, &b)?;
        let value = match (&a, &b) {
            (Value::Double(a), Value::Double(b)) => self.binary_op(op, *a, *b, false)?,
            (Value::Float(a), Value::Float(b)) => {
                self.binary_op(op, f64::from(*a), f64::from(*b), true)?
            }
            _ => {
                let da = a
                    .exact()
                    .ok_or(Error::Fatal(Failure::Internal("promotion left")))?;
                let db = b
                    .exact()
                    .ok_or(Error::Fatal(Failure::Internal("promotion right")))?;
                let d = match op {
                    Arithmetic::Add => da.add(db, &mut self.numeric)?,
                    Arithmetic::Subtract => da.sub(db, &mut self.numeric)?,
                    Arithmetic::Multiply => da.mul(db, &mut self.numeric)?,
                    Arithmetic::IntegerDivide => da.integer_quotient(db, &mut self.numeric)?,
                    Arithmetic::Modulo => da.remainder(db, &mut self.numeric)?,
                    Arithmetic::Divide => {
                        let scale = da
                            .normalized_scale()
                            .checked_sub(db.normalized_scale())
                            .and_then(|s| s.checked_add(18))
                            .ok_or_else(size_overflow)?
                            .max(18);
                        da.rounded_divide(
                            db,
                            scale,
                            RoundingMode::HalfTowardZero,
                            &mut self.numeric,
                        )?
                    }
                };
                if op == Arithmetic::IntegerDivide
                    || (op != Arithmetic::Divide
                        && a.kind() == AtomicType::Integer
                        && b.kind() == AtomicType::Integer)
                {
                    Value::Integer(Integral(d))
                } else {
                    Value::Decimal(d)
                }
            }
        };
        Ok(Atomic { value, lease })
    }
    fn binary_op(&mut self, op: Arithmetic, a: f64, b: f64, single: bool) -> Result<Value> {
        self.ledger.work(1)?;
        if op == Arithmetic::IntegerDivide {
            // Pinned zero precedence applies even to NaN/INF dividends.
            if b == 0.0 {
                return Err(Error::XPath(Fault::Foar0001));
            }
            if a.is_nan() || b.is_nan() || a.is_infinite() {
                return Err(Error::XPath(Fault::Foar0002));
            }
            let q = if single {
                f64::from((a as f32) / (b as f32))
            } else {
                a / b
            };
            if single && q.is_infinite() {
                // Saxon's float idiv overflow uses decimal values obtained by
                // its binary64 candidate profile, then an exact integer quotient.
                let da = self.saxon_binary_candidate(a)?;
                let db = self.saxon_binary_candidate(b)?;
                return Ok(Value::Integer(Integral(
                    da.integer_quotient(&db, &mut self.numeric)?,
                )));
            }
            return Ok(Value::Integer(Integral(self.binary_integer(q)?)));
        }
        let value = if single {
            let (a, b) = (a as f32, b as f32);
            Value::Float(match op {
                Arithmetic::Add => a + b,
                Arithmetic::Subtract => a - b,
                Arithmetic::Multiply => a * b,
                Arithmetic::Divide => a / b,
                Arithmetic::Modulo => a % b,
                Arithmetic::IntegerDivide => {
                    return Err(Error::Fatal(Failure::Internal("integer division dispatch")))
                }
            })
        } else {
            Value::Double(match op {
                Arithmetic::Add => a + b,
                Arithmetic::Subtract => a - b,
                Arithmetic::Multiply => a * b,
                Arithmetic::Divide => a / b,
                Arithmetic::Modulo => a % b,
                Arithmetic::IntegerDivide => {
                    return Err(Error::Fatal(Failure::Internal("integer division dispatch")))
                }
            })
        };
        Ok(value)
    }
    /// Numeric scalar comparison after caller-owned comparison conversion. Untyped,
    /// string, Boolean, cardinality and node comparison policies are deliberately absent.
    pub fn numeric_compare(&mut self, op: Comparison, a: &Atomic, b: &Atomic) -> Result<bool> {
        self.check(a)?;
        self.check(b)?;
        if [a.atomic_type(), b.atomic_type()].into_iter().any(|t| {
            matches!(
                t,
                AtomicType::String | AtomicType::UntypedAtomic | AtomicType::Boolean
            )
        }) {
            return Err(Error::XPath(Fault::Xpty0004));
        }
        let (a, b) = self.promoted(&a.value, &b.value)?;
        let order = match (&a, &b) {
            (Value::Double(a), Value::Double(b)) => a.partial_cmp(b),
            (Value::Float(a), Value::Float(b)) => a.partial_cmp(b),
            _ => Some(
                a.exact()
                    .ok_or(Error::Fatal(Failure::Internal("comparison left")))?
                    .compare(
                        b.exact()
                            .ok_or(Error::Fatal(Failure::Internal("comparison right")))?,
                        &mut self.numeric,
                    )?,
            ),
        };
        Ok(match op {
            Comparison::Equal => order == Some(Ordering::Equal),
            Comparison::NotEqual => order != Some(Ordering::Equal),
            Comparison::Less => order == Some(Ordering::Less),
            Comparison::LessEqual => matches!(order, Some(Ordering::Less | Ordering::Equal)),
            Comparison::Greater => order == Some(Ordering::Greater),
            Comparison::GreaterEqual => matches!(order, Some(Ordering::Greater | Ordering::Equal)),
        })
    }
    /// Unary numeric negation; untyped arithmetic operands become Double.
    pub fn negate(&mut self, source: &Atomic) -> Result<Atomic> {
        self.unary(source, false)
    }
    /// Numeric absolute value. Binary -0 becomes +0; NaN remains NaN.
    pub fn abs(&mut self, source: &Atomic) -> Result<Atomic> {
        self.unary(source, true)
    }
    fn unary(&mut self, source: &Atomic, abs: bool) -> Result<Atomic> {
        self.check(source)?;
        let lease = self.slot()?;
        let source = if abs {
            // fn:abs accepts numeric, not string/untyped function conversion here.
            match source.value {
                Value::UntypedAtomic(_) => return Err(Error::XPath(Fault::Xpty0004)),
                _ => self.arithmetic_value(&source.value)?,
            }
        } else {
            self.arithmetic_value(&source.value)?
        };
        let value = match source {
            Value::Integer(Integral(d)) => {
                Value::Integer(Integral(if abs { d.abs() } else { d.negated() }))
            }
            Value::Decimal(d) => Value::Decimal(if abs { d.abs() } else { d.negated() }),
            Value::Float(x) => Value::Float(if abs { x.abs() } else { -x }),
            Value::Double(x) => Value::Double(if abs { x.abs() } else { -x }),
            _ => return Err(Error::XPath(Fault::Xpty0004)),
        };
        Ok(Atomic { value, lease })
    }
    /// XPath rounding, nearest with ties toward positive infinity. `precision` is
    /// a checked signed decimal scale. This scalar API performs no argument conversion.
    pub fn round(&mut self, source: &Atomic, precision: i64) -> Result<Atomic> {
        self.check(source)?;
        let lease = self.slot()?;
        let value = match &source.value {
            Value::Integer(Integral(d)) => Value::Integer(Integral(d.quantize(
                precision,
                RoundingMode::HalfTowardPositiveInfinity,
                &mut self.numeric,
            )?)),
            Value::Decimal(d) => Value::Decimal(d.quantize(
                precision,
                RoundingMode::HalfTowardPositiveInfinity,
                &mut self.numeric,
            )?),
            Value::Float(x) => self.round_binary(f64::from(*x), precision, true)?,
            Value::Double(x) => self.round_binary(*x, precision, false)?,
            _ => return Err(Error::XPath(Fault::Xpty0004)),
        };
        Ok(Atomic { value, lease })
    }
    fn round_binary(&mut self, x: f64, precision: i64, single: bool) -> Result<Value> {
        let binary = |x: f64| {
            if single {
                Value::Float(x as f32)
            } else {
                Value::Double(x)
            }
        };
        if !x.is_finite() || x == 0.0 {
            self.ledger.work(1)?;
            return Ok(binary(x));
        }
        let d = self.exact_binary(x)?;
        let rounded = d.quantize(
            precision,
            RoundingMode::HalfTowardPositiveInfinity,
            &mut self.numeric,
        )?;
        if rounded.is_zero() {
            return Ok(binary(if x.is_sign_negative() { -0.0 } else { 0.0 }));
        }
        self.convert_binary(&Value::Decimal(rounded), single)
    }
}
