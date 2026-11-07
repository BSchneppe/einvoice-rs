use crate::value::Value;
use crate::{Atomic, AtomicType, Comparison, Error, Fault, Result, Scope};
use std::cmp::Ordering;
impl Scope {
    // Additive machine adapters only. All numeric algorithms remain in Scope/kernel.
    pub(super) fn singleton_ebv(&mut self, a: &Atomic) -> Result<bool> {
        self.check(a)?;
        Ok(match &a.value {
            Value::String(s) | Value::UntypedAtomic(s) => !s.is_empty(),
            _ => self
                .cast(a, AtomicType::Boolean)?
                .boolean()
                .ok_or(Error::XPath(Fault::Forg0006))?,
        })
    }
    pub(super) fn machine_compare(
        &mut self,
        op: Comparison,
        a: &Atomic,
        b: &Atomic,
        general: bool,
    ) -> Result<bool> {
        self.check(a)?;
        self.check(b)?;
        let convert = |scope: &mut Self, a: &Atomic, b: &Atomic| -> Result<Atomic> {
            if a.atomic_type() != AtomicType::UntypedAtomic {
                return scope.duplicate(a);
            }
            let target = if !general {
                AtomicType::String
            } else {
                match b.atomic_type() {
                    AtomicType::Integer
                    | AtomicType::Decimal
                    | AtomicType::Float
                    | AtomicType::Double => AtomicType::Double,
                    AtomicType::Boolean => AtomicType::Boolean,
                    _ => AtomicType::String,
                }
            };
            scope.cast(a, target)
        };
        let left = convert(self, a, b)?;
        let right = convert(self, b, a)?;
        let order = match (&left.value, &right.value) {
            (Value::String(a), Value::String(b)) => {
                self.ledger.work(
                    u64::try_from(a.len().max(b.len()))
                        .map_err(|_| crate::error::size_overflow())?,
                )?;
                Some(a.cmp(b))
            }
            (Value::Boolean(a), Value::Boolean(b)) => Some(a.cmp(b)),
            (Value::String(_) | Value::Boolean(_), _)
            | (_, Value::String(_) | Value::Boolean(_)) => {
                return Err(Error::XPath(Fault::Xpty0004))
            }
            _ => return self.numeric_compare(op, &left, &right),
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
}
pub(super) fn numeric(t: AtomicType) -> bool {
    matches!(
        t,
        AtomicType::Integer | AtomicType::Decimal | AtomicType::Float | AtomicType::Double
    )
}
pub(super) fn comparison(op: &str) -> Comparison {
    match op {
        "=" | "eq" => Comparison::Equal,
        "!=" | "ne" => Comparison::NotEqual,
        "<" | "lt" => Comparison::Less,
        "<=" | "le" => Comparison::LessEqual,
        ">" | "gt" => Comparison::Greater,
        _ => Comparison::GreaterEqual,
    }
}
