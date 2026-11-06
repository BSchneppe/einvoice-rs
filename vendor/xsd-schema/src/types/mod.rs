//! XSD type definitions and facets
//!
//! This module contains type definitions, facets, and the type system.
//!
//! ## Module Structure
//!
//! - `facets` - Constraining facets (length, pattern, enumeration, etc.)
//! - `simple` - Simple type definitions (atomic, list, union)
//! - `complex` - Complex type definitions with content models
//!
//! ## Type Code Enums
//!
//! The module provides three core enums for type identification:
//!
//! - `XmlTypeCode` - Complete type codes matching .NET XmlTypeCode (60+ codes)
//! - `PrimitiveTypeCode` - The 19 primitive XSD types for validator dispatch
//! - `ValueKind` - Runtime value kind for type discrimination

pub mod builtin;
pub mod complex;
pub mod convert;
pub mod equality;
pub(crate) mod exact_decimal;
pub mod facets;
#[cfg(feature = "xsd11")]
pub mod sequence;
pub mod simple;
pub mod validators;
pub mod value;
pub(crate) mod value_order;

// Re-exports
pub use builtin::BuiltinTypes;
pub use complex::{
    AttributeUse, AttributeUseKind, AttributeWildcard, ComplexTypeContent, ComplexTypeDef,
    Compositor, ContentKind, ContentParticle, ContentTerm, DerivationMethod, ModelGroupDef,
    NamespaceConstraint, ProcessContents,
};
pub use convert::{ConversionError, ConversionResult, IntoXmlValue, TypeConverter};
pub use facets::{
    facet_applicable, facet_applicable_for_type, normalize_whitespace, AssertionFacet,
    EnumerationFacet, ExplicitTimezone, ExplicitTimezoneFacet, FacetApplicability, FacetFixed,
    FacetKind, FacetSet, FractionDigitsFacet, LengthFacet, MaxExclusiveFacet, MaxInclusiveFacet,
    MaxLengthFacet, MinExclusiveFacet, MinInclusiveFacet, MinLengthFacet, PatternFacet,
    TotalDigitsFacet, WhitespaceFacet, WhitespaceMode,
};
#[cfg(feature = "xsd11")]
pub use sequence::{
    resolve_list_item_schema_type, ItemType, NameTest, SequenceType, TypeTest, XmlTypeCardinality,
};
pub use simple::{
    BuiltInType, SimpleTypeDef, SimpleTypeDerivationMethod, SimpleTypeRef, SimpleTypeVariety,
};
pub use validators::{
    TypeValidator, ValidationError, ValidationResult, ValidatorRegistry, VALIDATOR_REGISTRY,
};
pub use value::{
    DateTimeValue, DateValue, DayTimeDurationValue, DurationValue, GDayValue, GMonthDayValue,
    GMonthValue, GYearMonthValue, GYearValue, QNameValue, TimeValue, TimezoneOffset,
    XmlAtomicValue, XmlValue, XmlValueKind, YearMonthDurationValue,
};

// ============================================================================
// XmlTypeCode - Complete type codes matching .NET XmlTypeCode
// ============================================================================

/// XSD type codes for type identification and dispatch.
///
/// Ordered to match .NET `XmlTypeCode` for interoperability.
/// See XSD_TYPE_DESIGN.md §3.2 for full specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
#[derive(Default)]
pub enum XmlTypeCode {
    // Special types (0-9)
    /// No type information
    #[default]
    None = 0,
    /// Any item (XPath2)
    Item = 1,
    /// Any node
    Node = 2,
    /// Document node
    Document = 3,
    /// Element node
    Element = 4,
    /// Attribute node
    Attribute = 5,
    /// Namespace node
    Namespace = 6,
    /// Processing instruction node
    ProcessingInstruction = 7,
    /// Comment node
    Comment = 8,
    /// Text node
    Text = 9,

    // Schema abstract types (10-13)
    /// xs:anyType - the ur-type
    AnyType = 10,
    /// xs:anySimpleType - base of all simple types
    AnySimpleType = 11,
    /// xs:anyAtomicType - base of all atomic types (XSD 1.1)
    AnyAtomicType = 12,
    /// xs:untypedAtomic - untyped atomic value
    UntypedAtomic = 13,

    // String types (14-23)
    /// xs:string
    String = 14,
    /// xs:normalizedString
    NormalizedString = 15,
    /// xs:token
    Token = 16,
    /// xs:language
    Language = 17,
    /// xs:NMTOKEN
    NmToken = 18,
    /// xs:Name
    Name = 19,
    /// xs:NCName
    NCName = 20,
    /// xs:ID
    Id = 21,
    /// xs:IDREF
    IdRef = 22,
    /// xs:ENTITY
    Entity = 23,

    // Numeric types (24-40)
    /// xs:boolean
    Boolean = 24,
    /// xs:decimal
    Decimal = 25,
    /// xs:float
    Float = 26,
    /// xs:double
    Double = 27,
    /// xs:integer
    Integer = 28,
    /// xs:nonPositiveInteger
    NonPositiveInteger = 29,
    /// xs:negativeInteger
    NegativeInteger = 30,
    /// xs:long
    Long = 31,
    /// xs:int
    Int = 32,
    /// xs:short
    Short = 33,
    /// xs:byte
    Byte = 34,
    /// xs:nonNegativeInteger
    NonNegativeInteger = 35,
    /// xs:unsignedLong
    UnsignedLong = 36,
    /// xs:unsignedInt
    UnsignedInt = 37,
    /// xs:unsignedShort
    UnsignedShort = 38,
    /// xs:unsignedByte
    UnsignedByte = 39,
    /// xs:positiveInteger
    PositiveInteger = 40,

    // Date/time types (41-52)
    /// xs:duration
    Duration = 41,
    /// xs:dateTime
    DateTime = 42,
    /// xs:time
    Time = 43,
    /// xs:date
    Date = 44,
    /// xs:gYearMonth
    GYearMonth = 45,
    /// xs:gYear
    GYear = 46,
    /// xs:gMonthDay
    GMonthDay = 47,
    /// xs:gDay
    GDay = 48,
    /// xs:gMonth
    GMonth = 49,
    /// xs:yearMonthDuration (XSD 1.1)
    YearMonthDuration = 50,
    /// xs:dayTimeDuration (XSD 1.1)
    DayTimeDuration = 51,
    /// xs:dateTimeStamp (XSD 1.1)
    DateTimeStamp = 52,

    // Binary types (53-54)
    /// xs:hexBinary
    HexBinary = 53,
    /// xs:base64Binary
    Base64Binary = 54,

    // Other types (55-57)
    /// xs:anyURI
    AnyUri = 55,
    /// xs:QName
    QName = 56,
    /// xs:NOTATION
    Notation = 57,

    // List types (58-60)
    /// xs:NMTOKENS (list of NMTOKEN)
    NmTokens = 58,
    /// xs:IDREFS (list of IDREF)
    IdRefs = 59,
    /// xs:ENTITIES (list of ENTITY)
    Entities = 60,

    // XSD 1.1 bottom type (61)
    /// xs:error - the bottom type (union of no members); has no valid values (XSD 1.1)
    Error = 61,
}

impl XmlTypeCode {
    /// Returns true if this is a node type code.
    #[inline]
    pub fn is_node(&self) -> bool {
        matches!(
            self,
            Self::Node
                | Self::Document
                | Self::Element
                | Self::Attribute
                | Self::Namespace
                | Self::ProcessingInstruction
                | Self::Comment
                | Self::Text
        )
    }

    /// Returns true if this is an atomic type (not node, not list, not ur-type).
    #[inline]
    pub fn is_atomic(&self) -> bool {
        (*self as u8) >= Self::UntypedAtomic as u8
            && !matches!(
                self,
                Self::NmTokens | Self::IdRefs | Self::Entities | Self::Error
            )
    }

    /// Returns true if this is a list type (NMTOKENS, IDREFS, ENTITIES).
    #[inline]
    pub fn is_list(&self) -> bool {
        matches!(self, Self::NmTokens | Self::IdRefs | Self::Entities)
    }

    /// Returns true if this is one of the 19 XSD primitive atomic types
    /// that derive from `xs:anyAtomicType` in XSD 1.1 (§3.16.7.3).
    #[inline]
    pub fn is_primitive_atomic(&self) -> bool {
        matches!(
            self,
            Self::String
                | Self::Boolean
                | Self::Decimal
                | Self::Float
                | Self::Double
                | Self::Duration
                | Self::DateTime
                | Self::Time
                | Self::Date
                | Self::GYearMonth
                | Self::GYear
                | Self::GMonthDay
                | Self::GDay
                | Self::GMonth
                | Self::HexBinary
                | Self::Base64Binary
                | Self::AnyUri
                | Self::QName
                | Self::Notation
        )
    }

    /// Returns true if this is a numeric type.
    #[inline]
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            Self::Decimal
                | Self::Float
                | Self::Double
                | Self::Integer
                | Self::NonPositiveInteger
                | Self::NegativeInteger
                | Self::Long
                | Self::Int
                | Self::Short
                | Self::Byte
                | Self::NonNegativeInteger
                | Self::UnsignedLong
                | Self::UnsignedInt
                | Self::UnsignedShort
                | Self::UnsignedByte
                | Self::PositiveInteger
        )
    }

    /// Returns true if this is a date/time type.
    #[inline]
    pub fn is_date_time(&self) -> bool {
        matches!(
            self,
            Self::Duration
                | Self::DateTime
                | Self::Time
                | Self::Date
                | Self::GYearMonth
                | Self::GYear
                | Self::GMonthDay
                | Self::GDay
                | Self::GMonth
                | Self::YearMonthDuration
                | Self::DayTimeDuration
                | Self::DateTimeStamp
        )
    }

    /// Returns true if this is a string-derived type.
    #[inline]
    pub fn is_string_derived(&self) -> bool {
        matches!(
            self,
            Self::String
                | Self::NormalizedString
                | Self::Token
                | Self::Language
                | Self::NmToken
                | Self::Name
                | Self::NCName
                | Self::Id
                | Self::IdRef
                | Self::Entity
        )
    }

    /// Returns true if this is an XSD 1.1 type.
    #[inline]
    pub fn is_xsd11(&self) -> bool {
        matches!(
            self,
            Self::AnyAtomicType
                | Self::YearMonthDuration
                | Self::DayTimeDuration
                | Self::DateTimeStamp
                | Self::Error
        )
    }

    /// Returns the item type for list types, or None for non-list types.
    #[inline]
    pub fn list_item_type(&self) -> Option<XmlTypeCode> {
        match self {
            Self::NmTokens => Some(Self::NmToken),
            Self::IdRefs => Some(Self::IdRef),
            Self::Entities => Some(Self::Entity),
            _ => None,
        }
    }

    /// Get the local name of this type code (XSD type name).
    pub fn local_name(&self) -> Option<&'static str> {
        match self {
            Self::None
            | Self::Item
            | Self::Node
            | Self::Document
            | Self::Element
            | Self::Attribute
            | Self::Namespace
            | Self::ProcessingInstruction
            | Self::Comment
            | Self::Text => None,

            Self::AnyType => Some("anyType"),
            Self::AnySimpleType => Some("anySimpleType"),
            Self::AnyAtomicType => Some("anyAtomicType"),
            Self::UntypedAtomic => Some("untypedAtomic"),
            Self::String => Some("string"),
            Self::NormalizedString => Some("normalizedString"),
            Self::Token => Some("token"),
            Self::Language => Some("language"),
            Self::NmToken => Some("NMTOKEN"),
            Self::Name => Some("Name"),
            Self::NCName => Some("NCName"),
            Self::Id => Some("ID"),
            Self::IdRef => Some("IDREF"),
            Self::Entity => Some("ENTITY"),
            Self::Boolean => Some("boolean"),
            Self::Decimal => Some("decimal"),
            Self::Float => Some("float"),
            Self::Double => Some("double"),
            Self::Integer => Some("integer"),
            Self::NonPositiveInteger => Some("nonPositiveInteger"),
            Self::NegativeInteger => Some("negativeInteger"),
            Self::Long => Some("long"),
            Self::Int => Some("int"),
            Self::Short => Some("short"),
            Self::Byte => Some("byte"),
            Self::NonNegativeInteger => Some("nonNegativeInteger"),
            Self::UnsignedLong => Some("unsignedLong"),
            Self::UnsignedInt => Some("unsignedInt"),
            Self::UnsignedShort => Some("unsignedShort"),
            Self::UnsignedByte => Some("unsignedByte"),
            Self::PositiveInteger => Some("positiveInteger"),
            Self::Duration => Some("duration"),
            Self::DateTime => Some("dateTime"),
            Self::Time => Some("time"),
            Self::Date => Some("date"),
            Self::GYearMonth => Some("gYearMonth"),
            Self::GYear => Some("gYear"),
            Self::GMonthDay => Some("gMonthDay"),
            Self::GDay => Some("gDay"),
            Self::GMonth => Some("gMonth"),
            Self::YearMonthDuration => Some("yearMonthDuration"),
            Self::DayTimeDuration => Some("dayTimeDuration"),
            Self::DateTimeStamp => Some("dateTimeStamp"),
            Self::HexBinary => Some("hexBinary"),
            Self::Base64Binary => Some("base64Binary"),
            Self::AnyUri => Some("anyURI"),
            Self::QName => Some("QName"),
            Self::Notation => Some("NOTATION"),
            Self::NmTokens => Some("NMTOKENS"),
            Self::IdRefs => Some("IDREFS"),
            Self::Entities => Some("ENTITIES"),
            Self::Error => Some("error"),
        }
    }

    /// Parse type code from XSD local name.
    pub fn from_local_name(name: &str) -> Option<XmlTypeCode> {
        match name {
            "anyType" => Some(Self::AnyType),
            "anySimpleType" => Some(Self::AnySimpleType),
            "anyAtomicType" => Some(Self::AnyAtomicType),
            "untypedAtomic" => Some(Self::UntypedAtomic),
            "string" => Some(Self::String),
            "normalizedString" => Some(Self::NormalizedString),
            "token" => Some(Self::Token),
            "language" => Some(Self::Language),
            "NMTOKEN" => Some(Self::NmToken),
            "Name" => Some(Self::Name),
            "NCName" => Some(Self::NCName),
            "ID" => Some(Self::Id),
            "IDREF" => Some(Self::IdRef),
            "ENTITY" => Some(Self::Entity),
            "boolean" => Some(Self::Boolean),
            "decimal" => Some(Self::Decimal),
            "float" => Some(Self::Float),
            "double" => Some(Self::Double),
            "integer" => Some(Self::Integer),
            "nonPositiveInteger" => Some(Self::NonPositiveInteger),
            "negativeInteger" => Some(Self::NegativeInteger),
            "long" => Some(Self::Long),
            "int" => Some(Self::Int),
            "short" => Some(Self::Short),
            "byte" => Some(Self::Byte),
            "nonNegativeInteger" => Some(Self::NonNegativeInteger),
            "unsignedLong" => Some(Self::UnsignedLong),
            "unsignedInt" => Some(Self::UnsignedInt),
            "unsignedShort" => Some(Self::UnsignedShort),
            "unsignedByte" => Some(Self::UnsignedByte),
            "positiveInteger" => Some(Self::PositiveInteger),
            "duration" => Some(Self::Duration),
            "dateTime" => Some(Self::DateTime),
            "time" => Some(Self::Time),
            "date" => Some(Self::Date),
            "gYearMonth" => Some(Self::GYearMonth),
            "gYear" => Some(Self::GYear),
            "gMonthDay" => Some(Self::GMonthDay),
            "gDay" => Some(Self::GDay),
            "gMonth" => Some(Self::GMonth),
            "yearMonthDuration" => Some(Self::YearMonthDuration),
            "dayTimeDuration" => Some(Self::DayTimeDuration),
            "dateTimeStamp" => Some(Self::DateTimeStamp),
            "hexBinary" => Some(Self::HexBinary),
            "base64Binary" => Some(Self::Base64Binary),
            "anyURI" => Some(Self::AnyUri),
            "QName" => Some(Self::QName),
            "NOTATION" => Some(Self::Notation),
            "NMTOKENS" => Some(Self::NmTokens),
            "IDREFS" => Some(Self::IdRefs),
            "ENTITIES" => Some(Self::Entities),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

// ============================================================================
// PrimitiveTypeCode - The 19 primitive XSD types
// ============================================================================

/// Primitive type codes identifying the fundamental XSD types.
///
/// Used for validator dispatch and value space identification.
/// These are the 19 primitive types from which all other simple types derive.
/// See XSD_TYPE_DESIGN.md §3.3 for specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrimitiveTypeCode {
    /// xs:string - character sequences
    String,
    /// xs:boolean - true/false values
    Boolean,
    /// xs:decimal - arbitrary precision decimal numbers
    Decimal,
    /// xs:float - IEEE 754 single-precision float
    Float,
    /// xs:double - IEEE 754 double-precision float
    Double,
    /// xs:duration - time duration (PnYnMnDTnHnMnS)
    Duration,
    /// xs:dateTime - date and time
    DateTime,
    /// xs:time - time of day
    Time,
    /// xs:date - calendar date
    Date,
    /// xs:gYearMonth - Gregorian year and month
    GYearMonth,
    /// xs:gYear - Gregorian year
    GYear,
    /// xs:gMonthDay - Gregorian month and day
    GMonthDay,
    /// xs:gDay - Gregorian day
    GDay,
    /// xs:gMonth - Gregorian month
    GMonth,
    /// xs:hexBinary - hex-encoded binary data
    HexBinary,
    /// xs:base64Binary - base64-encoded binary data
    Base64Binary,
    /// xs:anyURI - URI reference
    AnyUri,
    /// xs:QName - qualified name (namespace + local name)
    QName,
    /// xs:NOTATION - notation reference
    Notation,
}

impl PrimitiveTypeCode {
    /// Get the primitive type code for any XmlTypeCode.
    ///
    /// Returns the primitive type from which the given type derives,
    /// or `None` for non-atomic types (nodes, lists, ur-types).
    pub fn from_type_code(code: XmlTypeCode) -> Option<Self> {
        match code {
            // String hierarchy -> String
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
            | XmlTypeCode::UntypedAtomic => Some(Self::String),

            XmlTypeCode::Boolean => Some(Self::Boolean),

            // Decimal hierarchy -> Decimal
            XmlTypeCode::Decimal
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
            | XmlTypeCode::PositiveInteger => Some(Self::Decimal),

            XmlTypeCode::Float => Some(Self::Float),
            XmlTypeCode::Double => Some(Self::Double),

            // Duration hierarchy
            XmlTypeCode::Duration
            | XmlTypeCode::YearMonthDuration
            | XmlTypeCode::DayTimeDuration => Some(Self::Duration),

            // DateTime hierarchy
            XmlTypeCode::DateTime | XmlTypeCode::DateTimeStamp => Some(Self::DateTime),

            XmlTypeCode::Time => Some(Self::Time),
            XmlTypeCode::Date => Some(Self::Date),
            XmlTypeCode::GYearMonth => Some(Self::GYearMonth),
            XmlTypeCode::GYear => Some(Self::GYear),
            XmlTypeCode::GMonthDay => Some(Self::GMonthDay),
            XmlTypeCode::GDay => Some(Self::GDay),
            XmlTypeCode::GMonth => Some(Self::GMonth),
            XmlTypeCode::HexBinary => Some(Self::HexBinary),
            XmlTypeCode::Base64Binary => Some(Self::Base64Binary),
            XmlTypeCode::AnyUri => Some(Self::AnyUri),
            XmlTypeCode::QName => Some(Self::QName),
            XmlTypeCode::Notation => Some(Self::Notation),

            // Non-atomic types have no primitive
            _ => None,
        }
    }

    /// Get the XmlTypeCode for this primitive type.
    pub fn to_type_code(&self) -> XmlTypeCode {
        match self {
            Self::String => XmlTypeCode::String,
            Self::Boolean => XmlTypeCode::Boolean,
            Self::Decimal => XmlTypeCode::Decimal,
            Self::Float => XmlTypeCode::Float,
            Self::Double => XmlTypeCode::Double,
            Self::Duration => XmlTypeCode::Duration,
            Self::DateTime => XmlTypeCode::DateTime,
            Self::Time => XmlTypeCode::Time,
            Self::Date => XmlTypeCode::Date,
            Self::GYearMonth => XmlTypeCode::GYearMonth,
            Self::GYear => XmlTypeCode::GYear,
            Self::GMonthDay => XmlTypeCode::GMonthDay,
            Self::GDay => XmlTypeCode::GDay,
            Self::GMonth => XmlTypeCode::GMonth,
            Self::HexBinary => XmlTypeCode::HexBinary,
            Self::Base64Binary => XmlTypeCode::Base64Binary,
            Self::AnyUri => XmlTypeCode::AnyUri,
            Self::QName => XmlTypeCode::QName,
            Self::Notation => XmlTypeCode::Notation,
        }
    }

    /// Get the local name of this primitive type.
    pub fn local_name(&self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Boolean => "boolean",
            Self::Decimal => "decimal",
            Self::Float => "float",
            Self::Double => "double",
            Self::Duration => "duration",
            Self::DateTime => "dateTime",
            Self::Time => "time",
            Self::Date => "date",
            Self::GYearMonth => "gYearMonth",
            Self::GYear => "gYear",
            Self::GMonthDay => "gMonthDay",
            Self::GDay => "gDay",
            Self::GMonth => "gMonth",
            Self::HexBinary => "hexBinary",
            Self::Base64Binary => "base64Binary",
            Self::AnyUri => "anyURI",
            Self::QName => "QName",
            Self::Notation => "NOTATION",
        }
    }

    /// Returns true if this is a numeric primitive type.
    pub fn is_numeric(&self) -> bool {
        matches!(self, Self::Decimal | Self::Float | Self::Double)
    }

    /// Returns an iterator over all primitive type codes.
    pub fn all() -> impl Iterator<Item = PrimitiveTypeCode> {
        [
            Self::String,
            Self::Boolean,
            Self::Decimal,
            Self::Float,
            Self::Double,
            Self::Duration,
            Self::DateTime,
            Self::Time,
            Self::Date,
            Self::GYearMonth,
            Self::GYear,
            Self::GMonthDay,
            Self::GDay,
            Self::GMonth,
            Self::HexBinary,
            Self::Base64Binary,
            Self::AnyUri,
            Self::QName,
            Self::Notation,
        ]
        .into_iter()
    }
}

// ============================================================================
// ValueKind - Runtime value kind for type discrimination
// ============================================================================

/// Runtime value kind for type discrimination.
///
/// Used to identify the category of a value at runtime,
/// enabling efficient dispatch in XPath2 operations.
/// See XSD_TYPE_DESIGN.md §3.4 for specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ValueKind {
    /// Atomic value (single indivisible value)
    #[default]
    Atomic,
    /// List value (sequence of atomic values)
    List,
    /// Union value (one of multiple possible types)
    Union,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xml_type_code_ordering() {
        // Verify .NET-compatible ordering
        assert_eq!(XmlTypeCode::None as u8, 0);
        assert_eq!(XmlTypeCode::Item as u8, 1);
        assert_eq!(XmlTypeCode::Node as u8, 2);
        assert_eq!(XmlTypeCode::AnyType as u8, 10);
        assert_eq!(XmlTypeCode::AnySimpleType as u8, 11);
        assert_eq!(XmlTypeCode::AnyAtomicType as u8, 12);
        assert_eq!(XmlTypeCode::UntypedAtomic as u8, 13);
        assert_eq!(XmlTypeCode::String as u8, 14);
        assert_eq!(XmlTypeCode::Boolean as u8, 24);
        assert_eq!(XmlTypeCode::Decimal as u8, 25);
        assert_eq!(XmlTypeCode::Duration as u8, 41);
        assert_eq!(XmlTypeCode::DateTime as u8, 42);
        assert_eq!(XmlTypeCode::HexBinary as u8, 53);
        assert_eq!(XmlTypeCode::AnyUri as u8, 55);
        assert_eq!(XmlTypeCode::QName as u8, 56);
        assert_eq!(XmlTypeCode::Notation as u8, 57);
        assert_eq!(XmlTypeCode::NmTokens as u8, 58);
        assert_eq!(XmlTypeCode::IdRefs as u8, 59);
        assert_eq!(XmlTypeCode::Entities as u8, 60);
    }

    #[test]
    fn test_xml_type_code_is_node() {
        assert!(XmlTypeCode::Node.is_node());
        assert!(XmlTypeCode::Document.is_node());
        assert!(XmlTypeCode::Element.is_node());
        assert!(XmlTypeCode::Attribute.is_node());
        assert!(XmlTypeCode::Text.is_node());
        assert!(XmlTypeCode::Comment.is_node());
        assert!(XmlTypeCode::ProcessingInstruction.is_node());
        assert!(XmlTypeCode::Namespace.is_node());

        assert!(!XmlTypeCode::String.is_node());
        assert!(!XmlTypeCode::Integer.is_node());
        assert!(!XmlTypeCode::AnyType.is_node());
    }

    #[test]
    fn test_xml_type_code_is_atomic() {
        assert!(XmlTypeCode::String.is_atomic());
        assert!(XmlTypeCode::Integer.is_atomic());
        assert!(XmlTypeCode::DateTime.is_atomic());
        assert!(XmlTypeCode::UntypedAtomic.is_atomic());
        assert!(XmlTypeCode::Boolean.is_atomic());

        // List types are not atomic
        assert!(!XmlTypeCode::NmTokens.is_atomic());
        assert!(!XmlTypeCode::IdRefs.is_atomic());
        assert!(!XmlTypeCode::Entities.is_atomic());

        // Abstract/node types are not atomic
        assert!(!XmlTypeCode::None.is_atomic());
        assert!(!XmlTypeCode::Node.is_atomic());
        assert!(!XmlTypeCode::AnyType.is_atomic());
        assert!(!XmlTypeCode::AnySimpleType.is_atomic());
    }

    #[test]
    fn test_xml_type_code_is_list() {
        assert!(XmlTypeCode::NmTokens.is_list());
        assert!(XmlTypeCode::IdRefs.is_list());
        assert!(XmlTypeCode::Entities.is_list());

        assert!(!XmlTypeCode::NmToken.is_list());
        assert!(!XmlTypeCode::IdRef.is_list());
        assert!(!XmlTypeCode::Entity.is_list());
        assert!(!XmlTypeCode::String.is_list());
    }

    #[test]
    fn test_xml_type_code_is_numeric() {
        assert!(XmlTypeCode::Decimal.is_numeric());
        assert!(XmlTypeCode::Integer.is_numeric());
        assert!(XmlTypeCode::Float.is_numeric());
        assert!(XmlTypeCode::Double.is_numeric());
        assert!(XmlTypeCode::Long.is_numeric());
        assert!(XmlTypeCode::UnsignedByte.is_numeric());

        assert!(!XmlTypeCode::String.is_numeric());
        assert!(!XmlTypeCode::Boolean.is_numeric());
        assert!(!XmlTypeCode::DateTime.is_numeric());
    }

    #[test]
    fn test_xml_type_code_is_date_time() {
        assert!(XmlTypeCode::DateTime.is_date_time());
        assert!(XmlTypeCode::Date.is_date_time());
        assert!(XmlTypeCode::Time.is_date_time());
        assert!(XmlTypeCode::Duration.is_date_time());
        assert!(XmlTypeCode::GYear.is_date_time());
        assert!(XmlTypeCode::YearMonthDuration.is_date_time());
        assert!(XmlTypeCode::DayTimeDuration.is_date_time());
        assert!(XmlTypeCode::DateTimeStamp.is_date_time());

        assert!(!XmlTypeCode::String.is_date_time());
        assert!(!XmlTypeCode::Integer.is_date_time());
    }

    #[test]
    fn test_xml_type_code_is_xsd11() {
        assert!(XmlTypeCode::AnyAtomicType.is_xsd11());
        assert!(XmlTypeCode::YearMonthDuration.is_xsd11());
        assert!(XmlTypeCode::DayTimeDuration.is_xsd11());
        assert!(XmlTypeCode::DateTimeStamp.is_xsd11());

        assert!(!XmlTypeCode::String.is_xsd11());
        assert!(!XmlTypeCode::DateTime.is_xsd11());
        assert!(!XmlTypeCode::Duration.is_xsd11());
    }

    #[test]
    fn test_xml_type_code_list_item_type() {
        assert_eq!(
            XmlTypeCode::NmTokens.list_item_type(),
            Some(XmlTypeCode::NmToken)
        );
        assert_eq!(
            XmlTypeCode::IdRefs.list_item_type(),
            Some(XmlTypeCode::IdRef)
        );
        assert_eq!(
            XmlTypeCode::Entities.list_item_type(),
            Some(XmlTypeCode::Entity)
        );
        assert_eq!(XmlTypeCode::String.list_item_type(), None);
    }

    #[test]
    fn test_xml_type_code_local_name() {
        assert_eq!(XmlTypeCode::String.local_name(), Some("string"));
        assert_eq!(XmlTypeCode::Integer.local_name(), Some("integer"));
        assert_eq!(XmlTypeCode::DateTime.local_name(), Some("dateTime"));
        assert_eq!(XmlTypeCode::AnyUri.local_name(), Some("anyURI"));
        assert_eq!(XmlTypeCode::QName.local_name(), Some("QName"));
        assert_eq!(XmlTypeCode::NmToken.local_name(), Some("NMTOKEN"));
        assert_eq!(XmlTypeCode::NmTokens.local_name(), Some("NMTOKENS"));
        assert_eq!(XmlTypeCode::None.local_name(), None);
        assert_eq!(XmlTypeCode::Element.local_name(), None);
    }

    #[test]
    fn test_xml_type_code_from_local_name() {
        assert_eq!(
            XmlTypeCode::from_local_name("string"),
            Some(XmlTypeCode::String)
        );
        assert_eq!(
            XmlTypeCode::from_local_name("integer"),
            Some(XmlTypeCode::Integer)
        );
        assert_eq!(
            XmlTypeCode::from_local_name("dateTime"),
            Some(XmlTypeCode::DateTime)
        );
        assert_eq!(
            XmlTypeCode::from_local_name("anyURI"),
            Some(XmlTypeCode::AnyUri)
        );
        assert_eq!(
            XmlTypeCode::from_local_name("QName"),
            Some(XmlTypeCode::QName)
        );
        assert_eq!(
            XmlTypeCode::from_local_name("NMTOKEN"),
            Some(XmlTypeCode::NmToken)
        );
        assert_eq!(
            XmlTypeCode::from_local_name("NMTOKENS"),
            Some(XmlTypeCode::NmTokens)
        );
        assert_eq!(XmlTypeCode::from_local_name("unknown"), None);
    }

    #[test]
    fn test_xml_type_code_roundtrip() {
        // All XSD types should round-trip through local_name/from_local_name
        for code_val in 10..=60u8 {
            let code: XmlTypeCode = unsafe { std::mem::transmute(code_val) };
            if let Some(name) = code.local_name() {
                assert_eq!(
                    XmlTypeCode::from_local_name(name),
                    Some(code),
                    "Round-trip failed for {:?}",
                    code
                );
            }
        }
    }

    #[test]
    fn test_primitive_type_code_count() {
        // There should be exactly 19 primitive types
        assert_eq!(PrimitiveTypeCode::all().count(), 19);
    }

    #[test]
    fn test_primitive_type_code_from_type_code() {
        // String hierarchy
        assert_eq!(
            PrimitiveTypeCode::from_type_code(XmlTypeCode::String),
            Some(PrimitiveTypeCode::String)
        );
        assert_eq!(
            PrimitiveTypeCode::from_type_code(XmlTypeCode::NormalizedString),
            Some(PrimitiveTypeCode::String)
        );
        assert_eq!(
            PrimitiveTypeCode::from_type_code(XmlTypeCode::Token),
            Some(PrimitiveTypeCode::String)
        );
        assert_eq!(
            PrimitiveTypeCode::from_type_code(XmlTypeCode::NCName),
            Some(PrimitiveTypeCode::String)
        );

        // Decimal hierarchy
        assert_eq!(
            PrimitiveTypeCode::from_type_code(XmlTypeCode::Decimal),
            Some(PrimitiveTypeCode::Decimal)
        );
        assert_eq!(
            PrimitiveTypeCode::from_type_code(XmlTypeCode::Integer),
            Some(PrimitiveTypeCode::Decimal)
        );
        assert_eq!(
            PrimitiveTypeCode::from_type_code(XmlTypeCode::Long),
            Some(PrimitiveTypeCode::Decimal)
        );
        assert_eq!(
            PrimitiveTypeCode::from_type_code(XmlTypeCode::UnsignedInt),
            Some(PrimitiveTypeCode::Decimal)
        );

        // Duration hierarchy
        assert_eq!(
            PrimitiveTypeCode::from_type_code(XmlTypeCode::Duration),
            Some(PrimitiveTypeCode::Duration)
        );
        assert_eq!(
            PrimitiveTypeCode::from_type_code(XmlTypeCode::YearMonthDuration),
            Some(PrimitiveTypeCode::Duration)
        );
        assert_eq!(
            PrimitiveTypeCode::from_type_code(XmlTypeCode::DayTimeDuration),
            Some(PrimitiveTypeCode::Duration)
        );

        // DateTime hierarchy
        assert_eq!(
            PrimitiveTypeCode::from_type_code(XmlTypeCode::DateTime),
            Some(PrimitiveTypeCode::DateTime)
        );
        assert_eq!(
            PrimitiveTypeCode::from_type_code(XmlTypeCode::DateTimeStamp),
            Some(PrimitiveTypeCode::DateTime)
        );

        // Primitives
        assert_eq!(
            PrimitiveTypeCode::from_type_code(XmlTypeCode::Boolean),
            Some(PrimitiveTypeCode::Boolean)
        );
        assert_eq!(
            PrimitiveTypeCode::from_type_code(XmlTypeCode::Float),
            Some(PrimitiveTypeCode::Float)
        );
        assert_eq!(
            PrimitiveTypeCode::from_type_code(XmlTypeCode::Double),
            Some(PrimitiveTypeCode::Double)
        );
        assert_eq!(
            PrimitiveTypeCode::from_type_code(XmlTypeCode::HexBinary),
            Some(PrimitiveTypeCode::HexBinary)
        );
        assert_eq!(
            PrimitiveTypeCode::from_type_code(XmlTypeCode::Base64Binary),
            Some(PrimitiveTypeCode::Base64Binary)
        );
        assert_eq!(
            PrimitiveTypeCode::from_type_code(XmlTypeCode::AnyUri),
            Some(PrimitiveTypeCode::AnyUri)
        );
        assert_eq!(
            PrimitiveTypeCode::from_type_code(XmlTypeCode::QName),
            Some(PrimitiveTypeCode::QName)
        );
        assert_eq!(
            PrimitiveTypeCode::from_type_code(XmlTypeCode::Notation),
            Some(PrimitiveTypeCode::Notation)
        );

        // Non-atomic types return None
        assert_eq!(PrimitiveTypeCode::from_type_code(XmlTypeCode::None), None);
        assert_eq!(PrimitiveTypeCode::from_type_code(XmlTypeCode::Node), None);
        assert_eq!(
            PrimitiveTypeCode::from_type_code(XmlTypeCode::AnyType),
            None
        );
        assert_eq!(
            PrimitiveTypeCode::from_type_code(XmlTypeCode::NmTokens),
            None
        );
    }

    #[test]
    fn test_primitive_type_code_to_type_code() {
        for prim in PrimitiveTypeCode::all() {
            let code = prim.to_type_code();
            assert_eq!(
                PrimitiveTypeCode::from_type_code(code),
                Some(prim),
                "Round-trip failed for {:?}",
                prim
            );
        }
    }

    #[test]
    fn test_primitive_type_code_local_name() {
        assert_eq!(PrimitiveTypeCode::String.local_name(), "string");
        assert_eq!(PrimitiveTypeCode::Boolean.local_name(), "boolean");
        assert_eq!(PrimitiveTypeCode::Decimal.local_name(), "decimal");
        assert_eq!(PrimitiveTypeCode::DateTime.local_name(), "dateTime");
        assert_eq!(PrimitiveTypeCode::AnyUri.local_name(), "anyURI");
        assert_eq!(PrimitiveTypeCode::QName.local_name(), "QName");
        assert_eq!(PrimitiveTypeCode::Notation.local_name(), "NOTATION");
    }

    #[test]
    fn test_value_kind_default() {
        assert_eq!(ValueKind::default(), ValueKind::Atomic);
    }

    #[test]
    fn test_value_kind_variants() {
        // Ensure all variants are distinct
        assert_ne!(ValueKind::Atomic, ValueKind::List);
        assert_ne!(ValueKind::Atomic, ValueKind::Union);
        assert_ne!(ValueKind::List, ValueKind::Union);
    }
}
