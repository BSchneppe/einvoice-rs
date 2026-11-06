//! XPath2 type conversion API
//!
//! This module provides type conversion functions following XPath2/XQuery
//! type promotion and casting rules.
//!
//! ## Conversion Types
//!
//! - **Casting**: Explicit type conversion via `cast as` (strict)
//! - **Type Promotion**: Implicit conversion for numeric operations
//! - **Atomization**: Converting nodes to atomic values
//!
//! ## Reference
//!
//! Based on XPath2 type conversion rules and the C# XPath2Convert implementation.

use num_bigint::BigInt;
use rust_decimal::Decimal;

use super::exact_decimal::{parse_decimal_exact, unsigned_zero, DecimalParseError, ExactDecimal};
#[cfg(feature = "xsd11")]
use super::sequence::{ItemType, SequenceType};
use super::validators::{is_valid_xsd_decimal_lexical, ValidationError, ValidatorRegistry};
use super::value::{XmlAtomicValue, XmlValue, XmlValueKind};
use super::{PrimitiveTypeCode, XmlTypeCode};

/// Error type for conversion operations
#[derive(Debug, Clone)]
pub enum ConversionError {
    /// Type conversion not allowed (XPTY0004)
    TypeMismatch { from: XmlTypeCode, to: XmlTypeCode },
    /// Invalid value for target type (FORG0001)
    InvalidValue {
        value: String,
        target_type: &'static str,
        message: String,
    },
    /// Overflow during conversion (FOAR0002)
    Overflow {
        value: String,
        target_type: &'static str,
    },
    /// Empty sequence where value expected
    EmptySequence,
    /// Validation error during conversion
    Validation(ValidationError),
}

impl std::fmt::Display for ConversionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TypeMismatch { from, to } => {
                write!(f, "XPTY0004: Cannot convert {:?} to {:?}", from, to)
            }
            Self::InvalidValue {
                value,
                target_type,
                message,
            } => {
                write!(
                    f,
                    "FORG0001: Invalid {} value '{}': {}",
                    target_type, value, message
                )
            }
            Self::Overflow { value, target_type } => {
                write!(
                    f,
                    "FOAR0002: Overflow converting '{}' to {}",
                    value, target_type
                )
            }
            Self::EmptySequence => {
                write!(f, "XPTY0004: Empty sequence where value expected")
            }
            Self::Validation(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for ConversionError {}

impl From<ValidationError> for ConversionError {
    fn from(e: ValidationError) -> Self {
        Self::Validation(e)
    }
}

/// Result type for conversion operations
pub type ConversionResult<T> = Result<T, ConversionError>;

/// XPath2-compatible type converter
///
/// Handles type conversions following XPath2/XQuery rules.
pub struct TypeConverter {
    validators: ValidatorRegistry,
}

impl TypeConverter {
    /// Create a new type converter
    pub fn new() -> Self {
        Self {
            validators: ValidatorRegistry::new(),
        }
    }

    /// Create with a custom validator registry
    pub fn with_validators(validators: ValidatorRegistry) -> Self {
        Self { validators }
    }

    /// Cast a value to the target type
    ///
    /// This is the strict casting operation used by `cast as`.
    pub fn cast(&self, value: &XmlValue, target: XmlTypeCode) -> ConversionResult<XmlValue> {
        // Same type - no conversion needed
        if value.type_code == target {
            return Ok(value.clone());
        }

        // Handle untyped atomic - cast via string
        if value.is_untyped() {
            let str_val = value.to_string_value();
            return self
                .validators
                .validate(target, &str_val)
                .map_err(ConversionError::from);
        }

        // Check if casting is allowed
        if !can_cast(value.type_code, target) {
            return Err(ConversionError::TypeMismatch {
                from: value.type_code,
                to: target,
            });
        }

        // Perform the cast
        self.perform_cast(value, target)
    }

    /// Convert value to string
    pub fn to_string(&self, value: &XmlValue) -> String {
        value.to_string_value()
    }

    /// Convert value to boolean (effective boolean value)
    pub fn to_boolean(&self, value: &XmlValue) -> ConversionResult<bool> {
        // An xs:decimal carried in lexical form (beyond `rust_decimal`) is a
        // number, not a string: true when it is not zero.
        if let Some(exact) = lexical_decimal_value(value) {
            return Ok(!exact.is_zero());
        }
        match &value.value {
            XmlValueKind::Atomic(XmlAtomicValue::Boolean(b)) => Ok(*b),
            XmlValueKind::Atomic(XmlAtomicValue::String(s)) => Ok(!s.is_empty()),
            XmlValueKind::Atomic(XmlAtomicValue::Integer(i)) => Ok(*i != BigInt::from(0)),
            XmlValueKind::Atomic(XmlAtomicValue::Decimal(d)) => Ok(!d.is_zero()),
            XmlValueKind::Atomic(XmlAtomicValue::Float(f)) => Ok(*f != 0.0 && !f.is_nan()),
            XmlValueKind::Atomic(XmlAtomicValue::Double(d)) => Ok(*d != 0.0 && !d.is_nan()),
            XmlValueKind::UntypedAtomic(s) => Ok(!s.is_empty()),
            _ => Err(ConversionError::TypeMismatch {
                from: value.type_code,
                to: XmlTypeCode::Boolean,
            }),
        }
    }

    /// Convert value to double (numeric promotion)
    pub fn to_double(&self, value: &XmlValue) -> ConversionResult<f64> {
        // An xs:decimal carried in lexical form: the nearest double, as
        // casting xs:decimal to xs:double gives it (F&O §17.1.3.2).
        if let Some(exact) = lexical_decimal_value(value) {
            return Ok(exact.to_f64());
        }
        match &value.value {
            XmlValueKind::Atomic(XmlAtomicValue::Double(d)) => Ok(*d),
            XmlValueKind::Atomic(XmlAtomicValue::Float(f)) => Ok(*f as f64),
            XmlValueKind::Atomic(XmlAtomicValue::Decimal(d)) => {
                d.to_string()
                    .parse()
                    .map_err(|_| ConversionError::Overflow {
                        value: d.to_string(),
                        target_type: "double",
                    })
            }
            XmlValueKind::Atomic(XmlAtomicValue::Integer(i)) => {
                i.to_string()
                    .parse()
                    .map_err(|_| ConversionError::Overflow {
                        value: i.to_string(),
                        target_type: "double",
                    })
            }
            XmlValueKind::Atomic(XmlAtomicValue::Boolean(b)) => Ok(if *b { 1.0 } else { 0.0 }),
            XmlValueKind::UntypedAtomic(s) => {
                s.trim().parse().map_err(|_| ConversionError::InvalidValue {
                    value: s.clone(),
                    target_type: "double",
                    message: "Not a valid number".to_string(),
                })
            }
            _ => Err(ConversionError::TypeMismatch {
                from: value.type_code,
                to: XmlTypeCode::Double,
            }),
        }
    }

    /// Convert value to decimal
    ///
    /// Never rounds a value it can hold. An `xs:untypedAtomic` is cast as
    /// F&O §17.1.1 casts a string to `xs:decimal` — "if the value is too large
    /// or too small to be accurately represented by the implementation, it is
    /// handled as an overflow or underflow as defined in 6.2", where "overflow
    /// behavior ·must· raise an error \[err:FOAR0002\]. On underflow, 0.0 must
    /// be returned": a magnitude above `Decimal::MAX`
    /// (79228162514264337593543950335) is [`ConversionError::Overflow`], and a
    /// non-zero magnitude below 10^−28, the smallest positive value held,
    /// returns 0. Any other value that is not `m / 10^s` with
    /// `|m| ≤ 2^96 − 1` and `s ≤ 28` (more digits than are held) is
    /// [`ConversionError::Overflow`] too, rather than a rounded value (F&O
    /// allows rounding or `FOCA0006` there; `ConversionError` has no variant of
    /// its own for it). An `xs:decimal` already carried in lexical form and an
    /// `xs:integer` beyond `Decimal`'s range are [`ConversionError::Overflow`]
    /// as well. A float or double converts to the nearest `Decimal` (0 below
    /// 10^−28); zero is never negative.
    pub fn to_decimal(&self, value: &XmlValue) -> ConversionResult<Decimal> {
        if let Some(exact) = lexical_decimal_value(value) {
            // A string payload that spells a value `Decimal` holds is that
            // value (the validator never stores one; only a typing mistake
            // could); one beyond capacity is `Overflow`.
            return exact.to_decimal().map_err(|_| ConversionError::Overflow {
                value: value.to_string_value(),
                target_type: "decimal",
            });
        }
        match &value.value {
            XmlValueKind::Atomic(XmlAtomicValue::Decimal(d)) => Ok(*d),
            XmlValueKind::Atomic(XmlAtomicValue::Integer(i)) => {
                i.to_string()
                    .parse()
                    .map_err(|_| ConversionError::Overflow {
                        value: i.to_string(),
                        target_type: "decimal",
                    })
            }
            XmlValueKind::Atomic(XmlAtomicValue::Float(f)) => {
                if f.is_nan() || f.is_infinite() {
                    return Err(ConversionError::InvalidValue {
                        value: f.to_string(),
                        target_type: "decimal",
                        message: "Cannot convert NaN or Infinity to decimal".to_string(),
                    });
                }
                Decimal::try_from(*f)
                    .map(unsigned_zero)
                    .map_err(|_| ConversionError::Overflow {
                        value: f.to_string(),
                        target_type: "decimal",
                    })
            }
            XmlValueKind::Atomic(XmlAtomicValue::Double(d)) => {
                if d.is_nan() || d.is_infinite() {
                    return Err(ConversionError::InvalidValue {
                        value: d.to_string(),
                        target_type: "decimal",
                        message: "Cannot convert NaN or Infinity to decimal".to_string(),
                    });
                }
                Decimal::try_from(*d)
                    .map(unsigned_zero)
                    .map_err(|_| ConversionError::Overflow {
                        value: d.to_string(),
                        target_type: "decimal",
                    })
            }
            XmlValueKind::Atomic(XmlAtomicValue::Boolean(b)) => {
                Ok(if *b { Decimal::ONE } else { Decimal::ZERO })
            }
            XmlValueKind::UntypedAtomic(s) => {
                // Casting untypedAtomic to xs:decimal uses the XSD lexical
                // mapping (Datatypes 1.1 §3.3.3, productions [45]-[51]), which is
                // narrower than what `rust_decimal`'s parser accepts (scientific
                // notation as of 1.42). Gate explicitly so this API does not
                // track the crate's parser behaviour for downstream consumers,
                // which resolve their own `rust_decimal` version.
                let trimmed = s.trim();
                if !is_valid_xsd_decimal_lexical(trimmed) {
                    return Err(ConversionError::InvalidValue {
                        value: s.clone(),
                        target_type: "decimal",
                        message: "Not a valid decimal".to_string(),
                    });
                }
                // Exact, 0 on underflow, or an error — `rust_decimal`'s
                // parser would round a form with more than 28 fraction digits.
                match parse_decimal_exact(trimmed) {
                    Ok(d) => Ok(d),
                    Err(DecimalParseError::Invalid) => Err(ConversionError::InvalidValue {
                        value: s.clone(),
                        target_type: "decimal",
                        message: "Not a valid decimal".to_string(),
                    }),
                    Err(DecimalParseError::TooPrecise)
                        if ExactDecimal::parse(trimmed)
                            .is_some_and(|d| d.is_below_smallest_decimal()) =>
                    {
                        Ok(Decimal::ZERO)
                    }
                    Err(DecimalParseError::TooLarge | DecimalParseError::TooPrecise) => {
                        Err(ConversionError::Overflow {
                            value: s.clone(),
                            target_type: "decimal",
                        })
                    }
                }
            }
            _ => Err(ConversionError::TypeMismatch {
                from: value.type_code,
                to: XmlTypeCode::Decimal,
            }),
        }
    }

    /// Convert value to integer
    pub fn to_integer(&self, value: &XmlValue) -> ConversionResult<BigInt> {
        // An xs:decimal carried in lexical form: truncated, exactly.
        if let Some(exact) = lexical_decimal_value(value) {
            return Ok(exact.trunc_to_bigint());
        }
        match &value.value {
            XmlValueKind::Atomic(XmlAtomicValue::Integer(i)) => Ok(i.clone()),
            XmlValueKind::Atomic(XmlAtomicValue::Decimal(d)) => {
                // Truncate to integer
                let truncated = d.trunc();
                truncated
                    .to_string()
                    .parse()
                    .map_err(|_| ConversionError::Overflow {
                        value: d.to_string(),
                        target_type: "integer",
                    })
            }
            XmlValueKind::Atomic(XmlAtomicValue::Float(f)) => {
                if f.is_nan() || f.is_infinite() {
                    return Err(ConversionError::InvalidValue {
                        value: f.to_string(),
                        target_type: "integer",
                        message: "Cannot convert NaN or Infinity to integer".to_string(),
                    });
                }
                let truncated = f.trunc();
                (truncated as i64)
                    .to_string()
                    .parse()
                    .map_err(|_| ConversionError::Overflow {
                        value: f.to_string(),
                        target_type: "integer",
                    })
            }
            XmlValueKind::Atomic(XmlAtomicValue::Double(d)) => {
                if d.is_nan() || d.is_infinite() {
                    return Err(ConversionError::InvalidValue {
                        value: d.to_string(),
                        target_type: "integer",
                        message: "Cannot convert NaN or Infinity to integer".to_string(),
                    });
                }
                let truncated = d.trunc();
                (truncated as i64)
                    .to_string()
                    .parse()
                    .map_err(|_| ConversionError::Overflow {
                        value: d.to_string(),
                        target_type: "integer",
                    })
            }
            XmlValueKind::Atomic(XmlAtomicValue::Boolean(b)) => {
                Ok(if *b { BigInt::from(1) } else { BigInt::from(0) })
            }
            XmlValueKind::UntypedAtomic(s) => {
                s.trim().parse().map_err(|_| ConversionError::InvalidValue {
                    value: s.clone(),
                    target_type: "integer",
                    message: "Not a valid integer".to_string(),
                })
            }
            _ => Err(ConversionError::TypeMismatch {
                from: value.type_code,
                to: XmlTypeCode::Integer,
            }),
        }
    }

    /// Check if value matches the given sequence type
    #[cfg(feature = "xsd11")]
    pub fn matches(&self, value: &XmlValue, seq_type: &SequenceType) -> bool {
        match &seq_type.item_type {
            ItemType::AnyItem => true,
            ItemType::AtomicType(XmlTypeCode::AnyAtomicType) => value.is_atomic(),
            ItemType::AtomicType(code) => {
                if value.type_code == *code {
                    return true;
                }
                // Check derivation
                derives_from(value.type_code, *code)
            }
            ItemType::AnyNode => false, // XmlValue doesn't hold nodes
            _ => false,
        }
    }

    /// Apply type promotion for numeric operations
    ///
    /// Returns the promoted type for two operands in an arithmetic operation.
    pub fn promote_numeric(&self, left: XmlTypeCode, right: XmlTypeCode) -> Option<XmlTypeCode> {
        if !left.is_numeric() || !right.is_numeric() {
            return None;
        }

        // Promotion rules:
        // - If either is double, result is double
        // - If either is float, result is float
        // - If either is decimal, result is decimal
        // - Otherwise result is integer
        if left == XmlTypeCode::Double || right == XmlTypeCode::Double {
            Some(XmlTypeCode::Double)
        } else if left == XmlTypeCode::Float || right == XmlTypeCode::Float {
            Some(XmlTypeCode::Float)
        } else if left == XmlTypeCode::Decimal || right == XmlTypeCode::Decimal {
            Some(XmlTypeCode::Decimal)
        } else {
            Some(XmlTypeCode::Integer)
        }
    }

    /// Promote a value to the target numeric type
    pub fn promote_to(&self, value: &XmlValue, target: XmlTypeCode) -> ConversionResult<XmlValue> {
        if value.type_code == target {
            return Ok(value.clone());
        }

        match target {
            XmlTypeCode::Double => {
                let d = self.to_double(value)?;
                Ok(XmlValue::double(d))
            }
            XmlTypeCode::Float => {
                let d = self.to_double(value)?;
                Ok(XmlValue::float(d as f32))
            }
            XmlTypeCode::Decimal => {
                let d = self.to_decimal(value)?;
                Ok(XmlValue::decimal(d))
            }
            XmlTypeCode::Integer => {
                let i = self.to_integer(value)?;
                Ok(XmlValue::integer(i))
            }
            _ => Err(ConversionError::TypeMismatch {
                from: value.type_code,
                to: target,
            }),
        }
    }

    /// Perform the actual cast operation
    fn perform_cast(&self, value: &XmlValue, target: XmlTypeCode) -> ConversionResult<XmlValue> {
        let str_val = value.to_string_value();

        match target {
            // Numeric targets - use conversion methods for better precision
            XmlTypeCode::Double => {
                let d = self.to_double(value)?;
                Ok(XmlValue::double(d))
            }
            XmlTypeCode::Float => {
                let d = self.to_double(value)?;
                Ok(XmlValue::float(d as f32))
            }
            XmlTypeCode::Decimal => {
                let d = self.to_decimal(value)?;
                Ok(XmlValue::decimal(d))
            }
            XmlTypeCode::Integer => {
                let i = self.to_integer(value)?;
                Ok(XmlValue::integer(i))
            }
            XmlTypeCode::Boolean => {
                let b = self.to_boolean(value)?;
                Ok(XmlValue::boolean(b))
            }
            XmlTypeCode::String => Ok(XmlValue::string(str_val)),
            XmlTypeCode::UntypedAtomic => Ok(XmlValue::untyped(str_val)),
            // For other types, use the validator
            _ => self
                .validators
                .validate(target, &str_val)
                .map_err(ConversionError::from),
        }
    }
}

impl Default for TypeConverter {
    fn default() -> Self {
        Self::new()
    }
}

/// The exact value of an `xs:decimal` carried in lexical form (see
/// [`XmlAtomicValue::Decimal`]); `None` for every other value.
fn lexical_decimal_value(value: &XmlValue) -> Option<ExactDecimal> {
    super::exact_decimal::lexical_decimal(value).and_then(ExactDecimal::parse)
}

/// Check if casting from one type to another is allowed
fn can_cast(from: XmlTypeCode, to: XmlTypeCode) -> bool {
    // Same type always allowed
    if from == to {
        return true;
    }

    // Get primitive types
    let from_prim = PrimitiveTypeCode::from_type_code(from);
    let to_prim = PrimitiveTypeCode::from_type_code(to);

    match (from_prim, to_prim) {
        // String can cast to anything (via parsing)
        (Some(PrimitiveTypeCode::String), _) => true,

        // Anything can cast to string
        (_, Some(PrimitiveTypeCode::String)) => true,

        // Numeric types can cast to each other
        (Some(p1), Some(p2)) if p1.is_numeric() && p2.is_numeric() => true,

        // Boolean can cast to numeric and vice versa
        (Some(PrimitiveTypeCode::Boolean), Some(p)) if p.is_numeric() => true,
        (Some(p), Some(PrimitiveTypeCode::Boolean)) if p.is_numeric() => true,

        // Date/time types have limited casting
        (Some(PrimitiveTypeCode::DateTime), Some(PrimitiveTypeCode::Date)) => true,
        (Some(PrimitiveTypeCode::DateTime), Some(PrimitiveTypeCode::Time)) => true,
        (Some(PrimitiveTypeCode::Date), Some(PrimitiveTypeCode::DateTime)) => true,

        // Duration subtypes
        (Some(PrimitiveTypeCode::Duration), Some(PrimitiveTypeCode::Duration)) => true,

        // Binary types can cast to each other
        (Some(PrimitiveTypeCode::HexBinary), Some(PrimitiveTypeCode::Base64Binary)) => true,
        (Some(PrimitiveTypeCode::Base64Binary), Some(PrimitiveTypeCode::HexBinary)) => true,

        // UntypedAtomic can cast to anything
        _ if from == XmlTypeCode::UntypedAtomic => true,

        // Anything can cast to untypedAtomic
        _ if to == XmlTypeCode::UntypedAtomic => true,

        // Derivation within same primitive
        _ if from_prim == to_prim => true,

        _ => false,
    }
}

/// Check if a type derives from another (for type matching)
#[cfg(feature = "xsd11")]
fn derives_from(derived: XmlTypeCode, base: XmlTypeCode) -> bool {
    if derived == base {
        return true;
    }

    // Check derivation hierarchy
    match base {
        XmlTypeCode::AnySimpleType => true,
        XmlTypeCode::AnyAtomicType => {
            derived != XmlTypeCode::AnySimpleType && derived != XmlTypeCode::AnyType
        }

        // String hierarchy
        XmlTypeCode::String => matches!(
            derived,
            XmlTypeCode::NormalizedString
                | XmlTypeCode::Token
                | XmlTypeCode::Language
                | XmlTypeCode::NmToken
                | XmlTypeCode::Name
                | XmlTypeCode::NCName
                | XmlTypeCode::Id
                | XmlTypeCode::IdRef
                | XmlTypeCode::Entity
        ),
        XmlTypeCode::NormalizedString => matches!(
            derived,
            XmlTypeCode::Token
                | XmlTypeCode::Language
                | XmlTypeCode::NmToken
                | XmlTypeCode::Name
                | XmlTypeCode::NCName
                | XmlTypeCode::Id
                | XmlTypeCode::IdRef
                | XmlTypeCode::Entity
        ),
        XmlTypeCode::Token => matches!(
            derived,
            XmlTypeCode::Language
                | XmlTypeCode::NmToken
                | XmlTypeCode::Name
                | XmlTypeCode::NCName
                | XmlTypeCode::Id
                | XmlTypeCode::IdRef
                | XmlTypeCode::Entity
        ),
        XmlTypeCode::Name => matches!(
            derived,
            XmlTypeCode::NCName | XmlTypeCode::Id | XmlTypeCode::IdRef | XmlTypeCode::Entity
        ),
        XmlTypeCode::NCName => matches!(
            derived,
            XmlTypeCode::Id | XmlTypeCode::IdRef | XmlTypeCode::Entity
        ),

        // Numeric hierarchy
        XmlTypeCode::Decimal => matches!(
            derived,
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
        ),
        XmlTypeCode::Integer => matches!(
            derived,
            XmlTypeCode::NonPositiveInteger
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
        ),
        XmlTypeCode::Long => matches!(
            derived,
            XmlTypeCode::Int | XmlTypeCode::Short | XmlTypeCode::Byte
        ),
        XmlTypeCode::Int => matches!(derived, XmlTypeCode::Short | XmlTypeCode::Byte),
        XmlTypeCode::Short => matches!(derived, XmlTypeCode::Byte),
        XmlTypeCode::NonNegativeInteger => matches!(
            derived,
            XmlTypeCode::UnsignedLong
                | XmlTypeCode::UnsignedInt
                | XmlTypeCode::UnsignedShort
                | XmlTypeCode::UnsignedByte
                | XmlTypeCode::PositiveInteger
        ),
        XmlTypeCode::UnsignedLong => matches!(
            derived,
            XmlTypeCode::UnsignedInt | XmlTypeCode::UnsignedShort | XmlTypeCode::UnsignedByte
        ),
        XmlTypeCode::UnsignedInt => {
            matches!(
                derived,
                XmlTypeCode::UnsignedShort | XmlTypeCode::UnsignedByte
            )
        }
        XmlTypeCode::UnsignedShort => matches!(derived, XmlTypeCode::UnsignedByte),
        XmlTypeCode::NonPositiveInteger => matches!(derived, XmlTypeCode::NegativeInteger),

        // Duration hierarchy
        XmlTypeCode::Duration => {
            matches!(
                derived,
                XmlTypeCode::YearMonthDuration | XmlTypeCode::DayTimeDuration
            )
        }

        // DateTime hierarchy
        XmlTypeCode::DateTime => matches!(derived, XmlTypeCode::DateTimeStamp),

        _ => false,
    }
}

/// Convert a Rust value to XmlValue
pub trait IntoXmlValue {
    fn into_xml_value(self) -> XmlValue;
}

impl IntoXmlValue for bool {
    fn into_xml_value(self) -> XmlValue {
        XmlValue::boolean(self)
    }
}

impl IntoXmlValue for i32 {
    fn into_xml_value(self) -> XmlValue {
        XmlValue::integer(BigInt::from(self))
    }
}

impl IntoXmlValue for i64 {
    fn into_xml_value(self) -> XmlValue {
        XmlValue::integer(BigInt::from(self))
    }
}

impl IntoXmlValue for f64 {
    fn into_xml_value(self) -> XmlValue {
        XmlValue::double(self)
    }
}

impl IntoXmlValue for f32 {
    fn into_xml_value(self) -> XmlValue {
        XmlValue::float(self)
    }
}

impl IntoXmlValue for Decimal {
    fn into_xml_value(self) -> XmlValue {
        XmlValue::decimal(self)
    }
}

impl IntoXmlValue for BigInt {
    fn into_xml_value(self) -> XmlValue {
        XmlValue::integer(self)
    }
}

impl IntoXmlValue for String {
    fn into_xml_value(self) -> XmlValue {
        XmlValue::string(self)
    }
}

impl IntoXmlValue for &str {
    fn into_xml_value(self) -> XmlValue {
        XmlValue::string(self)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// An `xs:decimal` beyond `rust_decimal`'s capacity: never rounded,
    /// converted exactly where the result is defined.
    #[test]
    fn test_conversions_of_decimals_beyond_capacity() {
        let converter = TypeConverter::new();
        let huge = "10000000000000000000000000000000000000000";
        let big = crate::types::exact_decimal::decimal_typed_value(huge);
        assert_eq!(big.as_decimal(), None);
        assert!(converter.to_boolean(&big).unwrap());
        assert_eq!(converter.to_double(&big).unwrap(), 1e40);
        assert_eq!(
            converter.to_integer(&big).unwrap(),
            huge.parse::<BigInt>().unwrap()
        );
        assert!(matches!(
            converter.to_decimal(&big),
            Err(ConversionError::Overflow { .. })
        ));
        assert_eq!(converter.to_string(&big), huge);
        let precise =
            crate::types::exact_decimal::decimal_typed_value("-1.00000000000000000000000000009");
        assert_eq!(converter.to_integer(&precise).unwrap(), BigInt::from(-1));
        // untypedAtomic → xs:decimal: exact or an error, never rounded.
        for text in ["0.12345678901234567890123456789", huge] {
            assert!(
                matches!(
                    converter.to_decimal(&XmlValue::untyped(text)),
                    Err(ConversionError::Overflow { .. })
                ),
                "{text}"
            );
        }
        assert_eq!(
            converter
                .to_decimal(&XmlValue::untyped("1.000000000000000000000000000000000"))
                .unwrap(),
            Decimal::ONE
        );
        // The validator-backed cast keeps the value, in lexical form.
        let cast = converter
            .cast(&XmlValue::untyped(huge), XmlTypeCode::Decimal)
            .unwrap();
        assert_eq!(cast.type_code, XmlTypeCode::Decimal);
        assert_eq!(cast.to_string_value(), huge);
    }

    /// F&O §17.1.1 / §6.2 for `to_decimal` of an `xs:untypedAtomic`: a
    /// non-zero magnitude below 10^-28 underflows to 0; one too large, or in
    /// range with more digits than are held, is still `Overflow`.
    #[test]
    fn test_to_decimal_underflow_returns_zero() {
        let converter = TypeConverter::new();
        let z27 = "0".repeat(27);
        let z28 = "0".repeat(28);
        for tiny in [
            format!("0.{z28}1"),
            format!("-0.{z28}5"),
            format!(" .{z28}99999 "),
            format!("0.{z28}100000"),
        ] {
            let untyped = XmlValue::untyped(tiny.as_str());
            let d = converter.to_decimal(&untyped).unwrap();
            assert!(d.is_zero() && !d.is_sign_negative(), "{tiny}: {d}");
            let promoted = converter
                .promote_to(&untyped, XmlTypeCode::Decimal)
                .unwrap();
            assert_eq!(promoted.to_string_value(), "0", "{tiny}");
        }
        // 10^-28 itself is held.
        assert_eq!(
            converter
                .to_decimal(&XmlValue::untyped(format!("-0.{z27}1").as_str()))
                .unwrap(),
            Decimal::new(-1, 28)
        );
        for not_underflow in [
            format!("0.{z27}15"),
            "0.12345678901234567890123456789".to_string(),
            "79228162514264337593543950336".to_string(),
        ] {
            assert!(
                matches!(
                    converter.to_decimal(&XmlValue::untyped(not_underflow.as_str())),
                    Err(ConversionError::Overflow { .. })
                ),
                "{not_underflow}"
            );
        }
        // A decimal already held in lexical form is not a cast from a string.
        let held = crate::types::exact_decimal::decimal_typed_value(&format!("0.{z28}1"));
        assert!(matches!(
            converter.to_decimal(&held),
            Err(ConversionError::Overflow { .. })
        ));
        // `cast` validates the string, as schema validation does: the exact
        // value, carried as its canonical string.
        let cast = converter
            .cast(
                &XmlValue::untyped(format!("0.{z28}10").as_str()),
                XmlTypeCode::Decimal,
            )
            .unwrap();
        assert_eq!(cast.to_string_value(), format!("0.{z28}1"));
        // A float or double: the nearest decimal, never a negative zero.
        for value in [
            XmlValue::double(-1.0e-40),
            XmlValue::double(-0.0),
            XmlValue::float(-1.0e-40),
        ] {
            let d = converter.to_decimal(&value).unwrap();
            assert!(d.is_zero() && !d.is_sign_negative(), "{value:?}");
            let cast = converter.cast(&value, XmlTypeCode::Decimal).unwrap();
            assert_eq!(cast.to_string_value(), "0", "{value:?}");
        }
    }

    #[test]
    fn test_to_boolean() {
        let converter = TypeConverter::new();

        assert!(converter.to_boolean(&XmlValue::boolean(true)).unwrap());
        assert!(!converter.to_boolean(&XmlValue::boolean(false)).unwrap());
        assert!(converter.to_boolean(&XmlValue::string("hello")).unwrap());
        assert!(!converter.to_boolean(&XmlValue::string("")).unwrap());
        assert!(converter
            .to_boolean(&XmlValue::integer(BigInt::from(1)))
            .unwrap());
        assert!(!converter
            .to_boolean(&XmlValue::integer(BigInt::from(0)))
            .unwrap());
        assert!(converter.to_boolean(&XmlValue::double(1.5)).unwrap());
        assert!(!converter.to_boolean(&XmlValue::double(0.0)).unwrap());
        assert!(!converter.to_boolean(&XmlValue::double(f64::NAN)).unwrap());
    }

    #[test]
    fn test_to_double() {
        let converter = TypeConverter::new();

        assert_eq!(converter.to_double(&XmlValue::double(2.5)).unwrap(), 2.5);
        assert_eq!(converter.to_double(&XmlValue::float(2.5)).unwrap(), 2.5);
        assert_eq!(
            converter
                .to_double(&XmlValue::integer(BigInt::from(42)))
                .unwrap(),
            42.0
        );
        assert_eq!(
            converter
                .to_double(&XmlValue::decimal(Decimal::new(123, 1)))
                .unwrap(),
            12.3
        );
        assert_eq!(converter.to_double(&XmlValue::boolean(true)).unwrap(), 1.0);
        assert_eq!(converter.to_double(&XmlValue::boolean(false)).unwrap(), 0.0);
    }

    #[test]
    fn test_to_integer() {
        let converter = TypeConverter::new();

        assert_eq!(
            converter
                .to_integer(&XmlValue::integer(BigInt::from(42)))
                .unwrap(),
            BigInt::from(42)
        );
        assert_eq!(
            converter.to_integer(&XmlValue::double(3.7)).unwrap(),
            BigInt::from(3) // Truncated
        );
        assert_eq!(
            converter
                .to_integer(&XmlValue::decimal(Decimal::new(99, 1)))
                .unwrap(),
            BigInt::from(9) // Truncated from 9.9
        );
    }

    #[test]
    fn test_to_decimal_untyped_lexical_gate() {
        let converter = TypeConverter::new();

        // Accepted: the XSD decimal lexical space, Datatypes 1.1 §3.3.3
        // (`decimalLexicalRep`, productions [45]-[51]).
        for (lexical, expected) in [
            ("123", Decimal::from(123)),
            ("-1.5", Decimal::new(-15, 1)),
            ("+.5", Decimal::new(5, 1)),
            (".5", Decimal::new(5, 1)),
            ("12.", Decimal::from(12)),
            ("  12.5  ", Decimal::new(125, 1)),
        ] {
            assert_eq!(
                converter.to_decimal(&XmlValue::untyped(lexical)).unwrap(),
                expected,
                "expected '{}' to convert to decimal",
                lexical
            );
        }

        // Rejected: forms outside that lexical space which `rust_decimal`'s
        // own parser accepts, or may accept in a future version. Downstream
        // consumers resolve their own `rust_decimal`, so this must not depend
        // on the crate's parser: `1_0` is accepted by 1.39, scientific
        // notation by 1.42.
        for lexical in [
            "1_0", "1E4", "1e4", "1.5E2", "NaN", "INF", "-INF", "", "+", ".", "0x1f", "1 2",
        ] {
            assert!(
                matches!(
                    converter.to_decimal(&XmlValue::untyped(lexical)),
                    Err(ConversionError::InvalidValue { .. })
                ),
                "expected '{}' to be rejected as xs:decimal",
                lexical
            );
        }
    }

    #[test]
    fn test_promote_to_decimal_inherits_lexical_gate() {
        let converter = TypeConverter::new();

        assert_eq!(
            converter
                .promote_to(&XmlValue::untyped("12.5"), XmlTypeCode::Decimal)
                .unwrap()
                .as_decimal(),
            Some(Decimal::new(125, 1))
        );
        for lexical in ["1_0", "1E4"] {
            assert!(
                converter
                    .promote_to(&XmlValue::untyped(lexical), XmlTypeCode::Decimal)
                    .is_err(),
                "expected promote_to to reject '{}' as xs:decimal",
                lexical
            );
        }
        // `cast` reaches the same lexical space through `DecimalValidator`.
        assert!(converter
            .cast(&XmlValue::untyped("1_0"), XmlTypeCode::Decimal)
            .is_err());
        assert_eq!(
            converter
                .cast(&XmlValue::untyped("12.5"), XmlTypeCode::Decimal)
                .unwrap()
                .as_decimal(),
            Some(Decimal::new(125, 1))
        );
    }

    #[test]
    fn test_cast_string_to_integer() {
        let converter = TypeConverter::new();
        let str_val = XmlValue::untyped("42");

        let result = converter.cast(&str_val, XmlTypeCode::Integer).unwrap();
        assert_eq!(result.type_code, XmlTypeCode::Integer);
        assert_eq!(result.as_integer(), Some(&BigInt::from(42)));
    }

    #[test]
    fn test_cast_numeric_promotion() {
        let converter = TypeConverter::new();

        // Integer to double
        let int_val = XmlValue::integer(BigInt::from(42));
        let result = converter.cast(&int_val, XmlTypeCode::Double).unwrap();
        assert_eq!(result.as_double(), Some(42.0));

        // Double to string
        let dbl_val = XmlValue::double(2.5);
        let result = converter.cast(&dbl_val, XmlTypeCode::String).unwrap();
        assert!(result.to_string_value().starts_with("2.5"));
    }

    #[test]
    fn test_promote_numeric() {
        let converter = TypeConverter::new();

        // Integer + Integer = Integer
        assert_eq!(
            converter.promote_numeric(XmlTypeCode::Integer, XmlTypeCode::Integer),
            Some(XmlTypeCode::Integer)
        );

        // Integer + Decimal = Decimal
        assert_eq!(
            converter.promote_numeric(XmlTypeCode::Integer, XmlTypeCode::Decimal),
            Some(XmlTypeCode::Decimal)
        );

        // Decimal + Float = Float
        assert_eq!(
            converter.promote_numeric(XmlTypeCode::Decimal, XmlTypeCode::Float),
            Some(XmlTypeCode::Float)
        );

        // Float + Double = Double
        assert_eq!(
            converter.promote_numeric(XmlTypeCode::Float, XmlTypeCode::Double),
            Some(XmlTypeCode::Double)
        );

        // String + Integer = None (not numeric)
        assert_eq!(
            converter.promote_numeric(XmlTypeCode::String, XmlTypeCode::Integer),
            None
        );
    }

    #[cfg(feature = "xsd11")]
    #[test]
    fn test_derives_from() {
        // Integer hierarchy
        assert!(derives_from(XmlTypeCode::Integer, XmlTypeCode::Decimal));
        assert!(derives_from(XmlTypeCode::Long, XmlTypeCode::Integer));
        assert!(derives_from(XmlTypeCode::Int, XmlTypeCode::Long));
        assert!(derives_from(XmlTypeCode::Short, XmlTypeCode::Int));
        assert!(derives_from(XmlTypeCode::Byte, XmlTypeCode::Short));

        // String hierarchy
        assert!(derives_from(
            XmlTypeCode::NormalizedString,
            XmlTypeCode::String
        ));
        assert!(derives_from(
            XmlTypeCode::Token,
            XmlTypeCode::NormalizedString
        ));
        assert!(derives_from(XmlTypeCode::NCName, XmlTypeCode::Name));

        // Duration hierarchy
        assert!(derives_from(
            XmlTypeCode::YearMonthDuration,
            XmlTypeCode::Duration
        ));
        assert!(derives_from(
            XmlTypeCode::DayTimeDuration,
            XmlTypeCode::Duration
        ));

        // Negative cases
        assert!(!derives_from(XmlTypeCode::String, XmlTypeCode::Integer));
        assert!(!derives_from(XmlTypeCode::Decimal, XmlTypeCode::Integer));
    }

    #[test]
    fn test_can_cast() {
        // Same type
        assert!(can_cast(XmlTypeCode::String, XmlTypeCode::String));

        // String to/from anything
        assert!(can_cast(XmlTypeCode::String, XmlTypeCode::Integer));
        assert!(can_cast(XmlTypeCode::Integer, XmlTypeCode::String));

        // Numeric conversions
        assert!(can_cast(XmlTypeCode::Integer, XmlTypeCode::Double));
        assert!(can_cast(XmlTypeCode::Decimal, XmlTypeCode::Float));

        // Boolean and numeric
        assert!(can_cast(XmlTypeCode::Boolean, XmlTypeCode::Integer));
        assert!(can_cast(XmlTypeCode::Double, XmlTypeCode::Boolean));

        // UntypedAtomic
        assert!(can_cast(XmlTypeCode::UntypedAtomic, XmlTypeCode::Date));
        assert!(can_cast(XmlTypeCode::DateTime, XmlTypeCode::UntypedAtomic));
    }

    #[test]
    fn test_into_xml_value() {
        assert_eq!(true.into_xml_value().as_boolean(), Some(true));
        assert_eq!(42i32.into_xml_value().as_integer(), Some(&BigInt::from(42)));
        assert_eq!(2.5f64.into_xml_value().as_double(), Some(2.5));
        assert_eq!("hello".into_xml_value().as_string(), Some("hello"));
    }
}
