use crate::error::size_overflow;
use crate::ieee;
use crate::value::{Integral, Value};
use crate::{Atomic, AtomicType, Error, Failure, Fault, NumericKind, Result, Scope, Text};
use einvoice_numeric::{Decimal, RoundingMode};
use std::rc::Rc;

impl Scope {
    /// Resolve only implemented XML Schema primitive names. Requested derived,
    /// sequence, node and other types fail explicitly as capability failures.
    pub fn resolve_type(namespace: &str, local: &str) -> Result<AtomicType> {
        if namespace != "http://www.w3.org/2001/XMLSchema" {
            return Err(Error::Fatal(Failure::Capability(
                "non-XML-Schema atomic type",
            )));
        }
        match local {
            "integer" => Ok(AtomicType::Integer),
            "decimal" => Ok(AtomicType::Decimal),
            "float" => Ok(AtomicType::Float),
            "double" => Ok(AtomicType::Double),
            "boolean" => Ok(AtomicType::Boolean),
            "string" => Ok(AtomicType::String),
            "untypedAtomic" => Ok(AtomicType::UntypedAtomic),
            _ => Err(Error::Fatal(Failure::Capability(
                "unimplemented requested atomic type",
            ))),
        }
    }
    /// Construct from the merged IR literal kind; no parser or AST is copied.
    pub fn literal(&mut self, kind: &NumericKind, lexeme: &str) -> Result<Atomic> {
        self.lexical(
            match kind {
                NumericKind::Integer => AtomicType::Integer,
                NumericKind::Decimal => AtomicType::Decimal,
                NumericKind::Double => AtomicType::Double,
            },
            lexeme,
        )
    }
    /// Explicit lexical conversion with pinned type-specific edge whitespace.
    /// String/untypedAtomic retain all bytes, without whitespace normalization.
    pub fn lexical(&mut self, target: AtomicType, text: &str) -> Result<Atomic> {
        self.ledger.input(text.len())?;
        let string = matches!(target, AtomicType::String | AtomicType::UntypedAtomic);
        let lease = if string {
            self.ledger
                .lease(text.len().checked_add(128).ok_or_else(size_overflow)?)?
        } else {
            self.slot()?
        };
        let value = match target {
            AtomicType::Integer => Value::Integer(Integral(Decimal::parse_integer(
                text.trim_matches(|c| c <= '\u{20}'),
                &mut self.numeric,
            )?)),
            AtomicType::Decimal => Value::Decimal(Decimal::parse_decimal(text, &mut self.numeric)?),
            AtomicType::Float | AtomicType::Double => {
                ieee::parse(text, target == AtomicType::Float)?
            }
            AtomicType::Boolean => {
                Value::Boolean(match text.trim_matches([' ', '\t', '\r', '\n']) {
                    "true" | "1" => true,
                    "false" | "0" => false,
                    _ => return Err(Error::XPath(Fault::Forg0001)),
                })
            }
            AtomicType::String | AtomicType::UntypedAtomic => {
                self.ledger.string(text.len())?;
                let s = Rc::from(text);
                if target == AtomicType::String {
                    Value::String(s)
                } else {
                    Value::UntypedAtomic(s)
                }
            }
        };
        Ok(Atomic { value, lease })
    }
    /// Explicit typed cast, separate from function conversion and atomization.
    pub fn cast(&mut self, source: &Atomic, target: AtomicType) -> Result<Atomic> {
        self.check(source)?;
        let lease = if matches!(target, AtomicType::String | AtomicType::UntypedAtomic) {
            let bound = self.text_bound(&source.value)?;
            self.ledger
                .lease(bound.checked_add(128).ok_or_else(size_overflow)?)?
        } else {
            self.slot()?
        };
        let value = self.cast_value(&source.value, target)?;
        Ok(Atomic { value, lease })
    }
    /// Castability of an already supplied singleton atomic operand only. Operand
    /// evaluation/cardinality belong to the future evaluator; fatal failures propagate.
    pub fn castable(&mut self, source: &Atomic, target: AtomicType) -> Result<bool> {
        match self.cast(source, target) {
            Ok(_) => Ok(true),
            Err(Error::XPath(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }
    pub(crate) fn cast_value(&mut self, source: &Value, target: AtomicType) -> Result<Value> {
        self.ledger.work(1)?;
        if source.kind() == target {
            return Ok(source.clone());
        }
        match target {
            AtomicType::Float | AtomicType::Double => {
                self.convert_binary(source, target == AtomicType::Float)
            }
            AtomicType::String | AtomicType::UntypedAtomic => {
                if let Value::String(s) | Value::UntypedAtomic(s) = source {
                    return Ok(if target == AtomicType::String {
                        Value::String(s.clone())
                    } else {
                        Value::UntypedAtomic(s.clone())
                    });
                }
                let text = self.value_text(source)?;
                self.ledger.string(text.len())?; // Rc string copies, bounded before allocation.
                Ok(if target == AtomicType::String {
                    Value::String(Rc::from(text.as_str()))
                } else {
                    Value::UntypedAtomic(Rc::from(text.as_str()))
                })
            }
            AtomicType::Boolean => {
                let b = match source {
                    Value::Integer(Integral(d)) | Value::Decimal(d) => !d.is_zero(),
                    Value::Float(x) => *x != 0.0 && !x.is_nan(),
                    Value::Double(x) => *x != 0.0 && !x.is_nan(),
                    Value::Boolean(x) => *x,
                    Value::String(s) | Value::UntypedAtomic(s) => {
                        self.ledger.input(s.len())?;
                        match s.trim_matches([' ', '\t', '\r', '\n']) {
                            "true" | "1" => true,
                            "false" | "0" => false,
                            _ => return Err(Error::XPath(Fault::Forg0001)),
                        }
                    }
                };
                Ok(Value::Boolean(b))
            }
            AtomicType::Integer | AtomicType::Decimal => {
                let integer = target == AtomicType::Integer;
                let d = match source {
                    Value::Integer(Integral(d)) | Value::Decimal(d) => d.clone(),
                    Value::Float(x) => {
                        if integer {
                            self.binary_integer(f64::from(*x))?
                        } else {
                            self.exact_binary(f64::from(*x))?
                        }
                    }
                    Value::Double(x) => {
                        if integer {
                            self.binary_integer(*x)?
                        } else {
                            self.exact_binary(*x)?
                        }
                    }
                    Value::Boolean(b) => self.small_integer(u64::from(*b))?,
                    Value::String(s) | Value::UntypedAtomic(s) => {
                        self.ledger.input(s.len())?;
                        if integer {
                            Decimal::parse_integer(
                                s.trim_matches(|c| c <= '\u{20}'),
                                &mut self.numeric,
                            )?
                        } else {
                            Decimal::parse_decimal(s, &mut self.numeric)?
                        }
                    }
                };
                if integer {
                    let d = d.quantize(0, RoundingMode::TowardZero, &mut self.numeric)?;
                    if !d.is_integral() {
                        return Err(Error::Fatal(Failure::Internal("nonintegral integer cast")));
                    }
                    Ok(Value::Integer(Integral(d)))
                } else {
                    Ok(Value::Decimal(d))
                }
            }
        }
    }
    /// `fn:number` on a supplied atom, or its empty input. This performs no sequence
    /// or node evaluation. Only specified conversion faults yield NaN; resource,
    /// capability and internal failures always propagate.
    pub fn number(&mut self, source: Option<&Atomic>) -> Result<Atomic> {
        if let Some(source) = source {
            self.check(source)?;
        }
        let lease = self.slot()?;
        let value = match source {
            None => Value::Double(f64::NAN),
            Some(source) => match self.convert_binary(&source.value, false) {
                Ok(v) => v,
                Err(Error::XPath(Fault::Forg0001 | Fault::Xpty0004)) => Value::Double(f64::NAN),
                Err(e) => return Err(e),
            },
        };
        Ok(Atomic { value, lease })
    }
    pub(crate) fn text_bound(&self, value: &Value) -> Result<usize> {
        match value {
            Value::String(s) | Value::UntypedAtomic(s) => Ok(s.len()),
            Value::Integer(Integral(d)) | Value::Decimal(d) => {
                let scale = usize::try_from(d.normalized_scale().unsigned_abs())
                    .map_err(|_| size_overflow())?;
                self.ledger
                    .limits
                    .numeric_slot
                    .checked_mul(3)
                    .and_then(|n| n.checked_add(scale))
                    .and_then(|n| n.checked_add(4))
                    .ok_or_else(size_overflow)
            }
            _ => Ok(128),
        }
    }
    pub(crate) fn value_text(&mut self, value: &Value) -> Result<String> {
        match value {
            Value::Integer(Integral(d)) | Value::Decimal(d) => self.decimal_text(d),
            Value::Float(x) => self.binary_text(f64::from(*x), true),
            Value::Double(x) => self.binary_text(*x, false),
            Value::Boolean(x) => {
                self.ledger.string(5)?;
                Ok(if *x { "true" } else { "false" }.into())
            }
            Value::String(s) | Value::UntypedAtomic(s) => {
                self.ledger.string(s.len())?;
                Ok(s.to_string())
            }
        }
    }
    /// Explicit XPath string form. Budget checks precede allocation; exact decimals
    /// are plain, binary values use pinned scientific thresholds and signed zero.
    pub fn render(&mut self, value: &Atomic) -> Result<Text> {
        self.check(value)?;
        let bound = self.text_bound(&value.value)?;
        let lease = self
            .ledger
            .lease(bound.checked_add(128).ok_or_else(size_overflow)?)?;
        self.ledger.output(bound)?;
        let text = self.value_text(&value.value)?;
        Ok(Text {
            text,
            _lease: lease,
        })
    }
}
