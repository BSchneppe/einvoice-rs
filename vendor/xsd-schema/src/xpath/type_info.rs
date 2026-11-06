//! Type information utilities for XPath evaluation.
//!
//! This module provides utilities for working with XPath/XSD type information.

use crate::types::value::XmlValue;
use crate::types::XmlTypeCode;

/// XPath 2.0 result type classification.
///
/// Used to categorize the result of XPath expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XPath2ResultType {
    /// String result
    String,
    /// Boolean result
    Boolean,
    /// Numeric result (integer, decimal, float, double)
    Number,
    /// Node set result
    NodeSet,
    /// Single navigator/node result
    Navigator,
    /// DateTime result
    DateTime,
    /// Duration result
    Duration,
    /// QName result
    QName,
    /// AnyUri result
    AnyUri,
    /// Other atomic type
    Other,
    /// Any/unknown type
    Any,
}

/// Get the XPath2 result type for an XmlValue.
pub fn get_value_result_type(value: &XmlValue) -> XPath2ResultType {
    get_result_type(value.type_code)
}

/// Get the XPath2 result type for a type code.
pub fn get_result_type(type_code: XmlTypeCode) -> XPath2ResultType {
    match type_code {
        // String types
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
        | XmlTypeCode::UntypedAtomic => XPath2ResultType::String,

        // Boolean
        XmlTypeCode::Boolean => XPath2ResultType::Boolean,

        // Numeric types
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
        | XmlTypeCode::PositiveInteger => XPath2ResultType::Number,

        // DateTime types
        XmlTypeCode::DateTime
        | XmlTypeCode::Date
        | XmlTypeCode::Time
        | XmlTypeCode::GYear
        | XmlTypeCode::GYearMonth
        | XmlTypeCode::GMonth
        | XmlTypeCode::GMonthDay
        | XmlTypeCode::GDay => XPath2ResultType::DateTime,

        // Duration types
        XmlTypeCode::Duration | XmlTypeCode::YearMonthDuration | XmlTypeCode::DayTimeDuration => {
            XPath2ResultType::Duration
        }

        // QName
        XmlTypeCode::QName | XmlTypeCode::Notation => XPath2ResultType::QName,

        // AnyUri
        XmlTypeCode::AnyUri => XPath2ResultType::AnyUri,

        // Node types
        XmlTypeCode::Node
        | XmlTypeCode::Document
        | XmlTypeCode::Element
        | XmlTypeCode::Attribute
        | XmlTypeCode::Namespace
        | XmlTypeCode::ProcessingInstruction
        | XmlTypeCode::Comment
        | XmlTypeCode::Text => XPath2ResultType::Navigator,

        // Abstract/special types
        XmlTypeCode::Item | XmlTypeCode::AnyType | XmlTypeCode::AnySimpleType => {
            XPath2ResultType::Any
        }

        // Catch-all
        _ => XPath2ResultType::Other,
    }
}

/// Check if a type code represents a numeric type.
pub fn is_numeric_type(type_code: XmlTypeCode) -> bool {
    type_code.is_numeric()
}

/// Check if a type code represents a string type.
pub fn is_string_type(type_code: XmlTypeCode) -> bool {
    type_code.is_string_derived() || type_code == XmlTypeCode::UntypedAtomic
}

/// Check if a type code represents a date/time type.
pub fn is_datetime_type(type_code: XmlTypeCode) -> bool {
    matches!(
        type_code,
        XmlTypeCode::DateTime
            | XmlTypeCode::Date
            | XmlTypeCode::Time
            | XmlTypeCode::GYear
            | XmlTypeCode::GYearMonth
            | XmlTypeCode::GMonth
            | XmlTypeCode::GMonthDay
            | XmlTypeCode::GDay
    )
}

/// Check if a type code represents a duration type.
pub fn is_duration_type(type_code: XmlTypeCode) -> bool {
    matches!(
        type_code,
        XmlTypeCode::Duration | XmlTypeCode::YearMonthDuration | XmlTypeCode::DayTimeDuration
    )
}

/// Check if a type code represents a node type.
pub fn is_node_type(type_code: XmlTypeCode) -> bool {
    type_code.is_node()
}

/// Get the base primitive type for a derived type.
pub fn get_base_primitive(type_code: XmlTypeCode) -> XmlTypeCode {
    match type_code {
        // String derivatives
        XmlTypeCode::NormalizedString
        | XmlTypeCode::Token
        | XmlTypeCode::Language
        | XmlTypeCode::NmToken
        | XmlTypeCode::Name
        | XmlTypeCode::NCName
        | XmlTypeCode::Id
        | XmlTypeCode::IdRef
        | XmlTypeCode::Entity => XmlTypeCode::String,

        // Integer derivatives
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
        | XmlTypeCode::PositiveInteger => XmlTypeCode::Integer,

        // Duration derivatives
        XmlTypeCode::YearMonthDuration | XmlTypeCode::DayTimeDuration => XmlTypeCode::Duration,

        // Already primitive or not applicable
        _ => type_code,
    }
}

/// Get the XSD type name for a type code.
pub fn type_code_to_name(type_code: XmlTypeCode) -> &'static str {
    match type_code {
        XmlTypeCode::None => "none",
        XmlTypeCode::Item => "item()",
        XmlTypeCode::Node => "node()",
        XmlTypeCode::Document => "document-node()",
        XmlTypeCode::Element => "element()",
        XmlTypeCode::Attribute => "attribute()",
        XmlTypeCode::Namespace => "namespace-node()",
        XmlTypeCode::ProcessingInstruction => "processing-instruction()",
        XmlTypeCode::Comment => "comment()",
        XmlTypeCode::Text => "text()",
        XmlTypeCode::AnyType => "xs:anyType",
        XmlTypeCode::AnySimpleType => "xs:anySimpleType",
        XmlTypeCode::AnyAtomicType => "xs:anyAtomicType",
        XmlTypeCode::UntypedAtomic => "xs:untypedAtomic",
        XmlTypeCode::String => "xs:string",
        XmlTypeCode::NormalizedString => "xs:normalizedString",
        XmlTypeCode::Token => "xs:token",
        XmlTypeCode::Language => "xs:language",
        XmlTypeCode::NmToken => "xs:NMTOKEN",
        XmlTypeCode::Name => "xs:Name",
        XmlTypeCode::NCName => "xs:NCName",
        XmlTypeCode::Id => "xs:ID",
        XmlTypeCode::IdRef => "xs:IDREF",
        XmlTypeCode::Entity => "xs:ENTITY",
        XmlTypeCode::Boolean => "xs:boolean",
        XmlTypeCode::Decimal => "xs:decimal",
        XmlTypeCode::Float => "xs:float",
        XmlTypeCode::Double => "xs:double",
        XmlTypeCode::Integer => "xs:integer",
        XmlTypeCode::NonPositiveInteger => "xs:nonPositiveInteger",
        XmlTypeCode::NegativeInteger => "xs:negativeInteger",
        XmlTypeCode::Long => "xs:long",
        XmlTypeCode::Int => "xs:int",
        XmlTypeCode::Short => "xs:short",
        XmlTypeCode::Byte => "xs:byte",
        XmlTypeCode::NonNegativeInteger => "xs:nonNegativeInteger",
        XmlTypeCode::UnsignedLong => "xs:unsignedLong",
        XmlTypeCode::UnsignedInt => "xs:unsignedInt",
        XmlTypeCode::UnsignedShort => "xs:unsignedShort",
        XmlTypeCode::UnsignedByte => "xs:unsignedByte",
        XmlTypeCode::PositiveInteger => "xs:positiveInteger",
        XmlTypeCode::Duration => "xs:duration",
        XmlTypeCode::DateTime => "xs:dateTime",
        XmlTypeCode::Time => "xs:time",
        XmlTypeCode::Date => "xs:date",
        XmlTypeCode::GYearMonth => "xs:gYearMonth",
        XmlTypeCode::GYear => "xs:gYear",
        XmlTypeCode::GMonthDay => "xs:gMonthDay",
        XmlTypeCode::GDay => "xs:gDay",
        XmlTypeCode::GMonth => "xs:gMonth",
        XmlTypeCode::HexBinary => "xs:hexBinary",
        XmlTypeCode::Base64Binary => "xs:base64Binary",
        XmlTypeCode::AnyUri => "xs:anyURI",
        XmlTypeCode::QName => "xs:QName",
        XmlTypeCode::Notation => "xs:NOTATION",
        XmlTypeCode::YearMonthDuration => "xs:yearMonthDuration",
        XmlTypeCode::DayTimeDuration => "xs:dayTimeDuration",
        _ => "unknown",
    }
}

/// Parse a type name to a type code.
pub fn name_to_type_code(name: &str) -> Option<XmlTypeCode> {
    // Strip xs: prefix if present
    let name = name.strip_prefix("xs:").unwrap_or(name);

    match name {
        "string" => Some(XmlTypeCode::String),
        "boolean" => Some(XmlTypeCode::Boolean),
        "decimal" => Some(XmlTypeCode::Decimal),
        "float" => Some(XmlTypeCode::Float),
        "double" => Some(XmlTypeCode::Double),
        "integer" => Some(XmlTypeCode::Integer),
        "long" => Some(XmlTypeCode::Long),
        "int" => Some(XmlTypeCode::Int),
        "short" => Some(XmlTypeCode::Short),
        "byte" => Some(XmlTypeCode::Byte),
        "unsignedLong" => Some(XmlTypeCode::UnsignedLong),
        "unsignedInt" => Some(XmlTypeCode::UnsignedInt),
        "unsignedShort" => Some(XmlTypeCode::UnsignedShort),
        "unsignedByte" => Some(XmlTypeCode::UnsignedByte),
        "positiveInteger" => Some(XmlTypeCode::PositiveInteger),
        "nonNegativeInteger" => Some(XmlTypeCode::NonNegativeInteger),
        "negativeInteger" => Some(XmlTypeCode::NegativeInteger),
        "nonPositiveInteger" => Some(XmlTypeCode::NonPositiveInteger),
        "duration" => Some(XmlTypeCode::Duration),
        "dateTime" => Some(XmlTypeCode::DateTime),
        "time" => Some(XmlTypeCode::Time),
        "date" => Some(XmlTypeCode::Date),
        "gYearMonth" => Some(XmlTypeCode::GYearMonth),
        "gYear" => Some(XmlTypeCode::GYear),
        "gMonthDay" => Some(XmlTypeCode::GMonthDay),
        "gDay" => Some(XmlTypeCode::GDay),
        "gMonth" => Some(XmlTypeCode::GMonth),
        "hexBinary" => Some(XmlTypeCode::HexBinary),
        "base64Binary" => Some(XmlTypeCode::Base64Binary),
        "anyURI" => Some(XmlTypeCode::AnyUri),
        "QName" => Some(XmlTypeCode::QName),
        "NOTATION" => Some(XmlTypeCode::Notation),
        "normalizedString" => Some(XmlTypeCode::NormalizedString),
        "token" => Some(XmlTypeCode::Token),
        "language" => Some(XmlTypeCode::Language),
        "NMTOKEN" => Some(XmlTypeCode::NmToken),
        "Name" => Some(XmlTypeCode::Name),
        "NCName" => Some(XmlTypeCode::NCName),
        "ID" => Some(XmlTypeCode::Id),
        "IDREF" => Some(XmlTypeCode::IdRef),
        "ENTITY" => Some(XmlTypeCode::Entity),
        "untypedAtomic" => Some(XmlTypeCode::UntypedAtomic),
        "anyAtomicType" => Some(XmlTypeCode::AnyAtomicType),
        "yearMonthDuration" => Some(XmlTypeCode::YearMonthDuration),
        "dayTimeDuration" => Some(XmlTypeCode::DayTimeDuration),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_bigint::BigInt;

    #[test]
    fn test_get_result_type() {
        assert_eq!(
            get_result_type(XmlTypeCode::String),
            XPath2ResultType::String
        );
        assert_eq!(
            get_result_type(XmlTypeCode::Boolean),
            XPath2ResultType::Boolean
        );
        assert_eq!(
            get_result_type(XmlTypeCode::Integer),
            XPath2ResultType::Number
        );
        assert_eq!(
            get_result_type(XmlTypeCode::DateTime),
            XPath2ResultType::DateTime
        );
        assert_eq!(
            get_result_type(XmlTypeCode::Duration),
            XPath2ResultType::Duration
        );
        assert_eq!(
            get_result_type(XmlTypeCode::Element),
            XPath2ResultType::Navigator
        );
    }

    #[test]
    fn test_get_value_result_type() {
        let value = XmlValue::string("test");
        assert_eq!(get_value_result_type(&value), XPath2ResultType::String);

        let value = XmlValue::integer(BigInt::from(42));
        assert_eq!(get_value_result_type(&value), XPath2ResultType::Number);
    }

    #[test]
    fn test_is_numeric_type() {
        assert!(is_numeric_type(XmlTypeCode::Integer));
        assert!(is_numeric_type(XmlTypeCode::Decimal));
        assert!(is_numeric_type(XmlTypeCode::Double));
        assert!(!is_numeric_type(XmlTypeCode::String));
    }

    #[test]
    fn test_type_code_to_name() {
        assert_eq!(type_code_to_name(XmlTypeCode::String), "xs:string");
        assert_eq!(type_code_to_name(XmlTypeCode::Integer), "xs:integer");
        assert_eq!(type_code_to_name(XmlTypeCode::Element), "element()");
    }

    #[test]
    fn test_name_to_type_code() {
        assert_eq!(name_to_type_code("string"), Some(XmlTypeCode::String));
        assert_eq!(name_to_type_code("xs:integer"), Some(XmlTypeCode::Integer));
        assert_eq!(name_to_type_code("unknown"), None);
    }

    #[test]
    fn test_get_base_primitive() {
        assert_eq!(get_base_primitive(XmlTypeCode::Long), XmlTypeCode::Integer);
        assert_eq!(get_base_primitive(XmlTypeCode::Token), XmlTypeCode::String);
        assert_eq!(get_base_primitive(XmlTypeCode::String), XmlTypeCode::String);
    }
}
