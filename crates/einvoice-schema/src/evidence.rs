//! Owned, lossless native evidence projection. No reference-engine mapping.
use crate::budget::{Failure, Size};
use crate::model::Name;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum TypeCode {
    // Special types (0-9)
    /// No type information
    None,
    /// Any item (XPath2)
    Item,
    /// Any node
    Node,
    /// Document node
    Document,
    /// Element node
    Element,
    /// Attribute node
    Attribute,
    /// Namespace node
    Namespace,
    /// Processing instruction node
    ProcessingInstruction,
    /// Comment node
    Comment,
    /// Text node
    Text,

    // Schema abstract types (10-13)
    /// xs:anyType - the ur-type
    AnyType,
    /// xs:anySimpleType - base of all simple types
    AnySimpleType,
    /// xs:anyAtomicType - base of all atomic types (XSD 1.1)
    AnyAtomicType,
    /// xs:untypedAtomic - untyped atomic value
    UntypedAtomic,

    // String types (14-23)
    /// xs:string
    String,
    /// xs:normalizedString
    NormalizedString,
    /// xs:token
    Token,
    /// xs:language
    Language,
    /// xs:NMTOKEN
    NmToken,
    /// xs:Name
    Name,
    /// xs:NCName
    NCName,
    /// xs:ID
    Id,
    /// xs:IDREF
    IdRef,
    /// xs:ENTITY
    Entity,

    // Numeric types (24-40)
    /// xs:boolean
    Boolean,
    /// xs:decimal
    Decimal,
    /// xs:float
    Float,
    /// xs:double
    Double,
    /// xs:integer
    Integer,
    /// xs:nonPositiveInteger
    NonPositiveInteger,
    /// xs:negativeInteger
    NegativeInteger,
    /// xs:long
    Long,
    /// xs:int
    Int,
    /// xs:short
    Short,
    /// xs:byte
    Byte,
    /// xs:nonNegativeInteger
    NonNegativeInteger,
    /// xs:unsignedLong
    UnsignedLong,
    /// xs:unsignedInt
    UnsignedInt,
    /// xs:unsignedShort
    UnsignedShort,
    /// xs:unsignedByte
    UnsignedByte,
    /// xs:positiveInteger
    PositiveInteger,

    // Date/time types (41-52)
    /// xs:duration
    Duration,
    /// xs:dateTime
    DateTime,
    /// xs:time
    Time,
    /// xs:date
    Date,
    /// xs:gYearMonth
    GYearMonth,
    /// xs:gYear
    GYear,
    /// xs:gMonthDay
    GMonthDay,
    /// xs:gDay
    GDay,
    /// xs:gMonth
    GMonth,
    /// xs:yearMonthDuration (XSD 1.1)
    YearMonthDuration,
    /// xs:dayTimeDuration (XSD 1.1)
    DayTimeDuration,
    /// xs:dateTimeStamp (XSD 1.1)
    DateTimeStamp,

    // Binary types (53-54)
    /// xs:hexBinary
    HexBinary,
    /// xs:base64Binary
    Base64Binary,

    // Other types (55-57)
    /// xs:anyURI
    AnyUri,
    /// xs:QName
    QName,
    /// xs:NOTATION
    Notation,

    // List types (58-60)
    /// xs:NMTOKENS (list of NMTOKEN)
    NmTokens,
    /// xs:IDREFS (list of IDREF)
    IdRefs,
    /// xs:ENTITIES (list of ENTITY)
    Entities,

    // XSD 1.1 bottom type (61)
    /// xs:error - the bottom type (union of no members); has no valid values (XSD 1.1)
    Error,
}

impl TypeCode {
    pub(crate) fn project(v: &xsd_schema::types::XmlTypeCode) -> Self {
        use xsd_schema::types::XmlTypeCode as B;
        match v {
            B::None => Self::None,
            B::Item => Self::Item,
            B::Node => Self::Node,
            B::Document => Self::Document,
            B::Element => Self::Element,
            B::Attribute => Self::Attribute,
            B::Namespace => Self::Namespace,
            B::ProcessingInstruction => Self::ProcessingInstruction,
            B::Comment => Self::Comment,
            B::Text => Self::Text,
            B::AnyType => Self::AnyType,
            B::AnySimpleType => Self::AnySimpleType,
            B::AnyAtomicType => Self::AnyAtomicType,
            B::UntypedAtomic => Self::UntypedAtomic,
            B::String => Self::String,
            B::NormalizedString => Self::NormalizedString,
            B::Token => Self::Token,
            B::Language => Self::Language,
            B::NmToken => Self::NmToken,
            B::Name => Self::Name,
            B::NCName => Self::NCName,
            B::Id => Self::Id,
            B::IdRef => Self::IdRef,
            B::Entity => Self::Entity,
            B::Boolean => Self::Boolean,
            B::Decimal => Self::Decimal,
            B::Float => Self::Float,
            B::Double => Self::Double,
            B::Integer => Self::Integer,
            B::NonPositiveInteger => Self::NonPositiveInteger,
            B::NegativeInteger => Self::NegativeInteger,
            B::Long => Self::Long,
            B::Int => Self::Int,
            B::Short => Self::Short,
            B::Byte => Self::Byte,
            B::NonNegativeInteger => Self::NonNegativeInteger,
            B::UnsignedLong => Self::UnsignedLong,
            B::UnsignedInt => Self::UnsignedInt,
            B::UnsignedShort => Self::UnsignedShort,
            B::UnsignedByte => Self::UnsignedByte,
            B::PositiveInteger => Self::PositiveInteger,
            B::Duration => Self::Duration,
            B::DateTime => Self::DateTime,
            B::Time => Self::Time,
            B::Date => Self::Date,
            B::GYearMonth => Self::GYearMonth,
            B::GYear => Self::GYear,
            B::GMonthDay => Self::GMonthDay,
            B::GDay => Self::GDay,
            B::GMonth => Self::GMonth,
            B::YearMonthDuration => Self::YearMonthDuration,
            B::DayTimeDuration => Self::DayTimeDuration,
            B::DateTimeStamp => Self::DateTimeStamp,
            B::HexBinary => Self::HexBinary,
            B::Base64Binary => Self::Base64Binary,
            B::AnyUri => Self::AnyUri,
            B::QName => Self::QName,
            B::Notation => Self::Notation,
            B::NmTokens => Self::NmTokens,
            B::IdRefs => Self::IdRefs,
            B::Entities => Self::Entities,
            B::Error => Self::Error,
        }
    }
    pub(crate) fn measure(v: &xsd_schema::types::XmlTypeCode, s: &mut Size) -> Result<(), Failure> {
        use xsd_schema::types::XmlTypeCode as B;
        s.add(std::mem::size_of::<Self>())?;
        match v {
            B::None => {}
            B::Item => {}
            B::Node => {}
            B::Document => {}
            B::Element => {}
            B::Attribute => {}
            B::Namespace => {}
            B::ProcessingInstruction => {}
            B::Comment => {}
            B::Text => {}
            B::AnyType => {}
            B::AnySimpleType => {}
            B::AnyAtomicType => {}
            B::UntypedAtomic => {}
            B::String => {}
            B::NormalizedString => {}
            B::Token => {}
            B::Language => {}
            B::NmToken => {}
            B::Name => {}
            B::NCName => {}
            B::Id => {}
            B::IdRef => {}
            B::Entity => {}
            B::Boolean => {}
            B::Decimal => {}
            B::Float => {}
            B::Double => {}
            B::Integer => {}
            B::NonPositiveInteger => {}
            B::NegativeInteger => {}
            B::Long => {}
            B::Int => {}
            B::Short => {}
            B::Byte => {}
            B::NonNegativeInteger => {}
            B::UnsignedLong => {}
            B::UnsignedInt => {}
            B::UnsignedShort => {}
            B::UnsignedByte => {}
            B::PositiveInteger => {}
            B::Duration => {}
            B::DateTime => {}
            B::Time => {}
            B::Date => {}
            B::GYearMonth => {}
            B::GYear => {}
            B::GMonthDay => {}
            B::GDay => {}
            B::GMonth => {}
            B::YearMonthDuration => {}
            B::DayTimeDuration => {}
            B::DateTimeStamp => {}
            B::HexBinary => {}
            B::Base64Binary => {}
            B::AnyUri => {}
            B::QName => {}
            B::Notation => {}
            B::NmTokens => {}
            B::IdRefs => {}
            B::Entities => {}
            B::Error => {}
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum FacetCause {
    /// Value violates length constraint
    LengthViolation { message: String },

    /// Value violates minLength constraint
    MinLengthViolation { actual: u64, min: u64 },

    /// Value violates maxLength constraint
    MaxLengthViolation { actual: u64, max: u64 },

    /// Value doesn't match pattern
    PatternViolation { value: String, pattern: String },

    /// Value not in enumeration
    EnumerationViolation { value: String },

    /// Value violates minInclusive constraint
    MinInclusiveViolation { value: String, min: String },

    /// Value violates maxInclusive constraint
    MaxInclusiveViolation { value: String, max: String },

    /// Value violates minExclusive constraint
    MinExclusiveViolation { value: String, min: String },

    /// Value violates maxExclusive constraint
    MaxExclusiveViolation { value: String, max: String },

    /// Value violates totalDigits constraint
    TotalDigitsViolation { actual: u32, max: u32 },

    /// Value violates fractionDigits constraint
    FractionDigitsViolation { actual: u32, max: u32 },

    /// Value violates explicitTimezone constraint
    ExplicitTimezoneViolation { message: String },

    /// Invalid pattern regex
    InvalidPattern { pattern: String, message: String },

    /// Facet derivation error - derived type is not more restrictive
    DerivationRestriction { message: String },

    /// Facet derivation error - fixed facet cannot be overridden
    FixedFacetViolation {
        facet_name: String,
        base_value: String,
        derived_value: String,
    },

    /// Facet derivation error - conflicting facets
    ConflictingFacets { message: String },

    /// Facet not applicable to this type
    NotApplicable { facet: String, type_name: String },
}

impl FacetCause {
    pub(crate) fn project(v: &xsd_schema::error::FacetError) -> Self {
        use xsd_schema::error::FacetError as B;
        match v {
            B::LengthViolation { message } => Self::LengthViolation {
                message: message.clone(),
            },
            B::MinLengthViolation { actual, min } => Self::MinLengthViolation {
                actual: *actual,
                min: *min,
            },
            B::MaxLengthViolation { actual, max } => Self::MaxLengthViolation {
                actual: *actual,
                max: *max,
            },
            B::PatternViolation { value, pattern } => Self::PatternViolation {
                value: value.clone(),
                pattern: pattern.clone(),
            },
            B::EnumerationViolation { value } => Self::EnumerationViolation {
                value: value.clone(),
            },
            B::MinInclusiveViolation { value, min } => Self::MinInclusiveViolation {
                value: value.clone(),
                min: min.clone(),
            },
            B::MaxInclusiveViolation { value, max } => Self::MaxInclusiveViolation {
                value: value.clone(),
                max: max.clone(),
            },
            B::MinExclusiveViolation { value, min } => Self::MinExclusiveViolation {
                value: value.clone(),
                min: min.clone(),
            },
            B::MaxExclusiveViolation { value, max } => Self::MaxExclusiveViolation {
                value: value.clone(),
                max: max.clone(),
            },
            B::TotalDigitsViolation { actual, max } => Self::TotalDigitsViolation {
                actual: *actual,
                max: *max,
            },
            B::FractionDigitsViolation { actual, max } => Self::FractionDigitsViolation {
                actual: *actual,
                max: *max,
            },
            B::ExplicitTimezoneViolation { message } => Self::ExplicitTimezoneViolation {
                message: message.clone(),
            },
            B::InvalidPattern { pattern, message } => Self::InvalidPattern {
                pattern: pattern.clone(),
                message: message.clone(),
            },
            B::DerivationRestriction { message } => Self::DerivationRestriction {
                message: message.clone(),
            },
            B::FixedFacetViolation {
                facet_name,
                base_value,
                derived_value,
            } => Self::FixedFacetViolation {
                facet_name: facet_name.clone(),
                base_value: base_value.clone(),
                derived_value: derived_value.clone(),
            },
            B::ConflictingFacets { message } => Self::ConflictingFacets {
                message: message.clone(),
            },
            B::NotApplicable { facet, type_name } => Self::NotApplicable {
                facet: facet.clone(),
                type_name: type_name.clone(),
            },
        }
    }
    pub(crate) fn measure(v: &xsd_schema::error::FacetError, s: &mut Size) -> Result<(), Failure> {
        use xsd_schema::error::FacetError as B;
        s.add(std::mem::size_of::<Self>())?;
        match v {
            B::LengthViolation { message } => {
                s.add(message.len())?;
            }
            B::MinLengthViolation { .. } => {}
            B::MaxLengthViolation { .. } => {}
            B::PatternViolation { value, pattern } => {
                s.add(value.len())?;
                s.add(pattern.len())?;
            }
            B::EnumerationViolation { value } => {
                s.add(value.len())?;
            }
            B::MinInclusiveViolation { value, min } => {
                s.add(value.len())?;
                s.add(min.len())?;
            }
            B::MaxInclusiveViolation { value, max } => {
                s.add(value.len())?;
                s.add(max.len())?;
            }
            B::MinExclusiveViolation { value, min } => {
                s.add(value.len())?;
                s.add(min.len())?;
            }
            B::MaxExclusiveViolation { value, max } => {
                s.add(value.len())?;
                s.add(max.len())?;
            }
            B::TotalDigitsViolation { .. } => {}
            B::FractionDigitsViolation { .. } => {}
            B::ExplicitTimezoneViolation { message } => {
                s.add(message.len())?;
            }
            B::InvalidPattern { pattern, message } => {
                s.add(pattern.len())?;
                s.add(message.len())?;
            }
            B::DerivationRestriction { message } => {
                s.add(message.len())?;
            }
            B::FixedFacetViolation {
                facet_name,
                base_value,
                derived_value,
            } => {
                s.add(facet_name.len())?;
                s.add(base_value.len())?;
                s.add(derived_value.len())?;
            }
            B::ConflictingFacets { message } => {
                s.add(message.len())?;
            }
            B::NotApplicable { facet, type_name } => {
                s.add(facet.len())?;
                s.add(type_name.len())?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ValueCause {
    /// Invalid lexical representation
    InvalidLexical {
        value: String,
        type_name: &'static str,
        message: String,
    },
    /// Facet constraint violation
    FacetViolation(FacetCause),
    /// Type error (wrong type for operation)
    TypeError {
        expected: TypeCode,
        actual: TypeCode,
    },
    /// Range error (value out of range for type)
    RangeError {
        value: String,
        type_name: &'static str,
    },
}

impl ValueCause {
    pub(crate) fn project(v: &xsd_schema::types::validators::ValidationError) -> Self {
        use xsd_schema::types::validators::ValidationError as B;
        match v {
            B::InvalidLexical {
                value,
                type_name,
                message,
            } => Self::InvalidLexical {
                value: value.clone(),
                type_name,
                message: message.clone(),
            },
            B::FacetViolation(inner) => Self::FacetViolation(FacetCause::project(inner)),
            B::TypeError { expected, actual } => Self::TypeError {
                expected: TypeCode::project(expected),
                actual: TypeCode::project(actual),
            },
            B::RangeError { value, type_name } => Self::RangeError {
                value: value.clone(),
                type_name,
            },
        }
    }
    pub(crate) fn measure(
        v: &xsd_schema::types::validators::ValidationError,
        s: &mut Size,
    ) -> Result<(), Failure> {
        use xsd_schema::types::validators::ValidationError as B;
        s.add(std::mem::size_of::<Self>())?;
        match v {
            B::InvalidLexical {
                value,
                type_name,
                message,
            } => {
                s.add(value.len())?;
                s.add(type_name.len())?;
                s.add(message.len())?;
            }
            B::FacetViolation(inner) => {
                FacetCause::measure(inner, s)?;
            }
            B::TypeError { expected, actual } => {
                TypeCode::measure(expected, s)?;
                TypeCode::measure(actual, s)?;
            }
            B::RangeError { value, type_name } => {
                s.add(value.len())?;
                s.add(type_name.len())?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum NativeCause {
    /// Original typed value-validation error, including its facet payload.
    Value(ValueCause),
    /// Original facet error.
    Facet(FacetCause),
    /// Parent content model rejected this expanded child name.
    UnexpectedChild {
        /// Triggering child; raw diagnostic still names its parent.
        found: Name,
    },
    /// Popped element's content model could not finish.
    IncompleteContent,
    /// Required attribute absent on the existing owner element.
    MissingRequiredAttribute {
        /// Actual required expanded name.
        name: Name,
    },
    /// Prohibited attribute (no exact use-component captured).
    ProhibitedAttribute,
    /// No allowed attribute use or effective wildcard matched.
    AttributeNotAllowed,
    /// Attribute fixed-value comparison failed.
    AttributeFixedValue {
        /// Actual fixed constraint.
        expected: String,
        /// Actual instance lexical value.
        actual: String,
    },
    /// No element declaration found under strict assessment.
    UndeclaredElement,
    /// `xsi:nil` present on a non-nillable declaration.
    NilNotAllowed,
    /// Nilled element also has a fixed constraint.
    NilWithFixedValue,
    /// Nilled element has prohibited child content.
    NilWithContent,
    /// `xsi:type` QName could not be parsed/resolved.
    XsiTypeUnresolved {
        /// Original lexical value.
        value: String,
    },
    /// Resolved `xsi:type` failed derivation or substitution checks.
    XsiTypeDerivation {
        /// Original lexical value.
        value: String,
        /// True for substitution blocked by `block`.
        blocked: bool,
    },
}

impl NativeCause {
    pub(crate) fn project(v: &xsd_schema::validation::errors::NativeCause) -> Self {
        use xsd_schema::validation::errors::NativeCause as B;
        match v {
            B::Value(inner) => Self::Value(ValueCause::project(inner)),
            B::Facet(inner) => Self::Facet(FacetCause::project(inner)),
            B::UnexpectedChild { found } => Self::UnexpectedChild {
                found: Name::project(found),
            },
            B::IncompleteContent => Self::IncompleteContent,
            B::MissingRequiredAttribute { name } => Self::MissingRequiredAttribute {
                name: Name::project(name),
            },
            B::ProhibitedAttribute => Self::ProhibitedAttribute,
            B::AttributeNotAllowed => Self::AttributeNotAllowed,
            B::AttributeFixedValue { expected, actual } => Self::AttributeFixedValue {
                expected: expected.clone(),
                actual: actual.clone(),
            },
            B::UndeclaredElement => Self::UndeclaredElement,
            B::NilNotAllowed => Self::NilNotAllowed,
            B::NilWithFixedValue => Self::NilWithFixedValue,
            B::NilWithContent => Self::NilWithContent,
            B::XsiTypeUnresolved { value } => Self::XsiTypeUnresolved {
                value: value.clone(),
            },
            B::XsiTypeDerivation { value, blocked } => Self::XsiTypeDerivation {
                value: value.clone(),
                blocked: *blocked,
            },
        }
    }
    pub(crate) fn measure(
        v: &xsd_schema::validation::errors::NativeCause,
        s: &mut Size,
    ) -> Result<(), Failure> {
        use xsd_schema::validation::errors::NativeCause as B;
        s.add(std::mem::size_of::<Self>())?;
        match v {
            B::Value(inner) => {
                ValueCause::measure(inner, s)?;
            }
            B::Facet(inner) => {
                FacetCause::measure(inner, s)?;
            }
            B::UnexpectedChild { found } => {
                Name::measure(found, s)?;
            }
            B::IncompleteContent => {}
            B::MissingRequiredAttribute { name } => {
                Name::measure(name, s)?;
            }
            B::ProhibitedAttribute => {}
            B::AttributeNotAllowed => {}
            B::AttributeFixedValue { expected, actual } => {
                s.add(expected.len())?;
                s.add(actual.len())?;
            }
            B::UndeclaredElement => {}
            B::NilNotAllowed => {}
            B::NilWithFixedValue => {}
            B::NilWithContent => {}
            B::XsiTypeUnresolved { value } => {
                s.add(value.len())?;
            }
            B::XsiTypeDerivation { value, .. } => {
                s.add(value.len())?;
            }
        }
        Ok(())
    }
}
