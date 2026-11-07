//! Boolean operations for XPath evaluation.
//!
//! This module implements the XPath 2.0 effective boolean value (EBV) rules
//! as defined in the XPath 2.0 specification section 2.4.3.
//!
//! ## Effective Boolean Value Rules
//!
//! The effective boolean value of a value is determined as follows:
//!
//! - If the value is an empty sequence, EBV is `false`
//! - If the value is a single node, EBV is `true`
//! - If the value is a singleton xs:boolean, EBV is the value
//! - If the value is a singleton xs:string/xs:untypedAtomic/xs:anyURI, EBV is `!value.is_empty()`
//! - If the value is a singleton numeric type, EBV is `value != 0 && !value.is_nan()`
//! - For other atomic types or sequences of length > 1, a type error is raised

use num_bigint::BigInt;

use super::error::XPathError;
use crate::types::{
    value::{XmlAtomicValue, XmlValue, XmlValueKind},
    XmlTypeCode,
};

/// Compute the effective boolean value of an atomic XmlValue.
///
/// This implements the core EBV logic for atomic values. For sequences,
/// use `effective_boolean_value_sequence` or the iterator-based version.
///
/// # Arguments
///
/// * `value` - The atomic value to evaluate
///
/// # Returns
///
/// * `Ok(bool)` - The effective boolean value
/// * `Err(XPathError)` - FORG0006 if the type doesn't support EBV
///
/// # Examples
///
/// ```
/// use xsd_schema::xpath::boolean::effective_boolean_value;
/// use xsd_schema::types::XmlValue;
///
/// assert_eq!(effective_boolean_value(&XmlValue::boolean(true)).unwrap(), true);
/// assert_eq!(effective_boolean_value(&XmlValue::string("")).unwrap(), false);
/// assert_eq!(effective_boolean_value(&XmlValue::string("hello")).unwrap(), true);
/// ```
pub fn effective_boolean_value(value: &XmlValue) -> Result<bool, XPathError> {
    match &value.value {
        // Boolean: use the value directly
        XmlValueKind::Atomic(XmlAtomicValue::Boolean(b)) => Ok(*b),

        // An xs:decimal carried in lexical form (beyond `rust_decimal`) is a
        // number: non-zero is true.
        XmlValueKind::Atomic(XmlAtomicValue::String(_))
            if crate::types::exact_decimal::lexical_decimal(value).is_some() =>
        {
            Ok(crate::types::exact_decimal::ExactDecimal::from_value(value)
                .is_some_and(|d| !d.is_zero()))
        }

        // String types: non-empty is true
        XmlValueKind::Atomic(XmlAtomicValue::String(s)) => Ok(!s.is_empty()),
        XmlValueKind::UntypedAtomic(s) => Ok(!s.is_empty()),
        XmlValueKind::Atomic(XmlAtomicValue::AnyUri(s)) => Ok(!s.is_empty()),

        // Float: non-zero and non-NaN is true
        XmlValueKind::Atomic(XmlAtomicValue::Float(f)) => Ok(!f.is_nan() && *f != 0.0),

        // Double: non-zero and non-NaN is true
        XmlValueKind::Atomic(XmlAtomicValue::Double(d)) => Ok(!d.is_nan() && *d != 0.0),

        // Decimal: non-zero is true
        XmlValueKind::Atomic(XmlAtomicValue::Decimal(d)) => Ok(!d.is_zero()),

        // Integer: non-zero is true
        XmlValueKind::Atomic(XmlAtomicValue::Integer(i)) => Ok(*i != BigInt::from(0)),

        // Union: unwrap and evaluate
        XmlValueKind::Union(inner) => effective_boolean_value(inner),

        // List values: error (multiple items)
        XmlValueKind::List { .. } => Err(XPathError::invalid_argument_type(
            "fn:boolean",
            format_type_for_error(value.type_code),
        )),

        // Other atomic types: error
        _ => Err(XPathError::invalid_argument_type(
            "fn:boolean",
            format_type_for_error(value.type_code),
        )),
    }
}

/// Compute the effective boolean value with support for optional values.
///
/// This handles the case where a value may be absent (empty sequence).
///
/// # Arguments
///
/// * `value` - Optional value to evaluate
///
/// # Returns
///
/// * `Ok(false)` if value is None (empty sequence)
/// * `Ok(bool)` the EBV of the value
/// * `Err(XPathError)` if the type doesn't support EBV
pub fn effective_boolean_value_opt(value: Option<&XmlValue>) -> Result<bool, XPathError> {
    match value {
        None => Ok(false), // Empty sequence is false
        Some(v) => effective_boolean_value(v),
    }
}

/// Logical NOT operation on an XPath value.
///
/// Returns the negation of the effective boolean value.
///
/// # Arguments
///
/// * `value` - The value to negate
///
/// # Returns
///
/// * `Ok(bool)` - The negated boolean value
/// * `Err(XPathError)` - If EBV cannot be computed
pub fn not(value: &XmlValue) -> Result<bool, XPathError> {
    effective_boolean_value(value).map(|b| !b)
}

/// Logical NOT with optional value support.
pub fn not_opt(value: Option<&XmlValue>) -> Result<bool, XPathError> {
    effective_boolean_value_opt(value).map(|b| !b)
}

/// Check if a type code represents a numeric type.
///
/// Numeric types support EBV via the "non-zero and non-NaN" rule.
pub fn is_numeric_type(type_code: XmlTypeCode) -> bool {
    matches!(
        type_code,
        XmlTypeCode::Decimal
            | XmlTypeCode::Float
            | XmlTypeCode::Double
            | XmlTypeCode::Integer
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

/// Check if a type code represents a string-like type.
///
/// String-like types support EBV via the "non-empty" rule.
pub fn is_string_like_type(type_code: XmlTypeCode) -> bool {
    matches!(
        type_code,
        XmlTypeCode::String
            | XmlTypeCode::NormalizedString
            | XmlTypeCode::Token
            | XmlTypeCode::Language
            | XmlTypeCode::NmToken
            | XmlTypeCode::Name
            | XmlTypeCode::NCName
            | XmlTypeCode::Id
            | XmlTypeCode::IdRef
            | XmlTypeCode::Entity
            | XmlTypeCode::UntypedAtomic
            | XmlTypeCode::AnyUri
    )
}

/// Check if a type code supports effective boolean value.
pub fn supports_ebv(type_code: XmlTypeCode) -> bool {
    type_code == XmlTypeCode::Boolean
        || is_numeric_type(type_code)
        || is_string_like_type(type_code)
}

/// Format a type code for error messages.
fn format_type_for_error(type_code: XmlTypeCode) -> String {
    match type_code {
        XmlTypeCode::None => "none".to_string(),
        XmlTypeCode::Item => "item()".to_string(),
        XmlTypeCode::Node => "node()".to_string(),
        XmlTypeCode::Document => "document-node()".to_string(),
        XmlTypeCode::Element => "element()".to_string(),
        XmlTypeCode::Attribute => "attribute()".to_string(),
        XmlTypeCode::Namespace => "namespace-node()".to_string(),
        XmlTypeCode::ProcessingInstruction => "processing-instruction()".to_string(),
        XmlTypeCode::Comment => "comment()".to_string(),
        XmlTypeCode::Text => "text()".to_string(),
        XmlTypeCode::AnyType => "xs:anyType".to_string(),
        XmlTypeCode::AnySimpleType => "xs:anySimpleType".to_string(),
        XmlTypeCode::AnyAtomicType => "xs:anyAtomicType".to_string(),
        XmlTypeCode::UntypedAtomic => "xs:untypedAtomic".to_string(),
        XmlTypeCode::String => "xs:string".to_string(),
        XmlTypeCode::Boolean => "xs:boolean".to_string(),
        XmlTypeCode::Decimal => "xs:decimal".to_string(),
        XmlTypeCode::Float => "xs:float".to_string(),
        XmlTypeCode::Double => "xs:double".to_string(),
        XmlTypeCode::Integer => "xs:integer".to_string(),
        XmlTypeCode::Duration => "xs:duration".to_string(),
        XmlTypeCode::DateTime => "xs:dateTime".to_string(),
        XmlTypeCode::Time => "xs:time".to_string(),
        XmlTypeCode::Date => "xs:date".to_string(),
        XmlTypeCode::GYearMonth => "xs:gYearMonth".to_string(),
        XmlTypeCode::GYear => "xs:gYear".to_string(),
        XmlTypeCode::GMonthDay => "xs:gMonthDay".to_string(),
        XmlTypeCode::GDay => "xs:gDay".to_string(),
        XmlTypeCode::GMonth => "xs:gMonth".to_string(),
        XmlTypeCode::HexBinary => "xs:hexBinary".to_string(),
        XmlTypeCode::Base64Binary => "xs:base64Binary".to_string(),
        XmlTypeCode::QName => "xs:QName".to_string(),
        XmlTypeCode::Notation => "xs:NOTATION".to_string(),
        XmlTypeCode::YearMonthDuration => "xs:yearMonthDuration".to_string(),
        XmlTypeCode::DayTimeDuration => "xs:dayTimeDuration".to_string(),
        _ => format!("type({})", type_code as u8),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::XmlTypeCode;
    use num_bigint::BigInt;
    use rust_decimal::Decimal;

    #[test]
    fn test_boolean_ebv() {
        assert!(effective_boolean_value(&XmlValue::boolean(true)).unwrap());
        assert!(!effective_boolean_value(&XmlValue::boolean(false)).unwrap());
    }

    #[test]
    fn test_string_ebv() {
        assert!(!effective_boolean_value(&XmlValue::string("")).unwrap());
        assert!(effective_boolean_value(&XmlValue::string("hello")).unwrap());
        assert!(effective_boolean_value(&XmlValue::string(" ")).unwrap()); // Whitespace is non-empty
    }

    #[test]
    fn test_untyped_atomic_ebv() {
        assert!(!effective_boolean_value(&XmlValue::untyped("")).unwrap());
        assert!(effective_boolean_value(&XmlValue::untyped("value")).unwrap());
    }

    #[test]
    fn test_numeric_ebv() {
        // Integer
        assert!(!effective_boolean_value(&XmlValue::integer(BigInt::from(0))).unwrap());
        assert!(effective_boolean_value(&XmlValue::integer(BigInt::from(42))).unwrap());
        assert!(effective_boolean_value(&XmlValue::integer(BigInt::from(-1))).unwrap());

        // Decimal
        assert!(!effective_boolean_value(&XmlValue::decimal(Decimal::ZERO)).unwrap());
        assert!(effective_boolean_value(&XmlValue::decimal(Decimal::new(123, 2))).unwrap());

        // Double
        assert!(!effective_boolean_value(&XmlValue::double(0.0)).unwrap());
        assert!(effective_boolean_value(&XmlValue::double(1.5)).unwrap());
        assert!(!effective_boolean_value(&XmlValue::double(f64::NAN)).unwrap());
        assert!(effective_boolean_value(&XmlValue::double(f64::INFINITY)).unwrap());

        // Float
        assert!(!effective_boolean_value(&XmlValue::float(0.0)).unwrap());
        assert!(effective_boolean_value(&XmlValue::float(1.5)).unwrap());
        assert!(!effective_boolean_value(&XmlValue::float(f32::NAN)).unwrap());
    }

    #[test]
    fn test_empty_sequence_ebv() {
        assert!(!effective_boolean_value_opt(None).unwrap());
    }

    #[test]
    fn test_not() {
        assert!(!not(&XmlValue::boolean(true)).unwrap());
        assert!(not(&XmlValue::boolean(false)).unwrap());
        assert!(not(&XmlValue::string("")).unwrap());
        assert!(!not(&XmlValue::string("x")).unwrap());
    }

    #[test]
    fn test_unsupported_type_error() {
        // DateTime doesn't support EBV
        let dt = XmlValue::new(
            XmlTypeCode::DateTime,
            XmlValueKind::Atomic(XmlAtomicValue::DateTime(
                crate::types::value::DateTimeValue {
                    year: 2024,
                    month: 1,
                    day: 15,
                    hour: 12,
                    minute: 30,
                    second: Decimal::ZERO,
                    timezone: None,
                },
            )),
        );
        let result = effective_boolean_value(&dt);
        assert!(result.is_err());
        if let Err(XPathError::FORG0006Named { function, .. }) = result {
            assert_eq!(function, "fn:boolean");
        } else {
            panic!("Expected FORG0006Named error");
        }
    }

    #[test]
    fn test_is_numeric_type() {
        assert!(is_numeric_type(XmlTypeCode::Integer));
        assert!(is_numeric_type(XmlTypeCode::Decimal));
        assert!(is_numeric_type(XmlTypeCode::Float));
        assert!(is_numeric_type(XmlTypeCode::Double));
        assert!(is_numeric_type(XmlTypeCode::Long));
        assert!(!is_numeric_type(XmlTypeCode::String));
        assert!(!is_numeric_type(XmlTypeCode::Boolean));
    }

    #[test]
    fn test_is_string_like_type() {
        assert!(is_string_like_type(XmlTypeCode::String));
        assert!(is_string_like_type(XmlTypeCode::UntypedAtomic));
        assert!(is_string_like_type(XmlTypeCode::AnyUri));
        assert!(!is_string_like_type(XmlTypeCode::Integer));
        assert!(!is_string_like_type(XmlTypeCode::Boolean));
    }
}
