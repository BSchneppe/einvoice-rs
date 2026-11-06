//! Simple type definitions
//!
//! This module implements XSD simple type definitions: atomic, list, and union types.

use super::facets::FacetSet;
use super::{PrimitiveTypeCode, XmlTypeCode};
use crate::ids::{NameId, SimpleTypeKey, TypeKey};
use crate::parser::location::SourceRef;
use crate::schema::model::DerivationSet;

/// Simple type variety (atomic, list, or union)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimpleTypeVariety {
    /// Atomic type (single value)
    Atomic,
    /// List type (whitespace-separated values of item type)
    List,
    /// Union type (value satisfies one of member types)
    Union,
}

/// Derivation method for simple types
///
/// This enum specifies how a simple type was derived from its base type.
/// Note: This is different from complex type derivation which uses
/// Restriction/Extension. Simple types use Restriction/List/Union.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SimpleTypeDerivationMethod {
    /// Derived by restriction (constraining base type with facets)
    #[default]
    Restriction,
    /// Derived as a list type (whitespace-separated values)
    List,
    /// Derived as a union type (one of multiple member types)
    Union,
}

/// Reference to a simple type (by key or built-in name)
#[derive(Debug, Clone)]
pub enum SimpleTypeRef {
    /// Reference to a defined simple type
    Resolved(SimpleTypeKey),
    /// Unresolved reference (QName to be resolved later)
    Unresolved {
        namespace: Option<NameId>,
        local_name: NameId,
    },
    /// Built-in type reference
    BuiltIn(BuiltInType),
}

/// Built-in XSD simple types
///
/// This enum represents all built-in simple types in XSD 1.0 and 1.1.
/// Use [`XmlTypeCode`] for a more comprehensive type code enumeration
/// that includes node types and complex types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltInType {
    // Special abstract types
    /// xs:anySimpleType - base of all simple types
    AnySimpleType,
    /// xs:anyAtomicType - base of all atomic types (XSD 1.1)
    AnyAtomicType,
    /// xs:untypedAtomic - untyped atomic value (XSD 1.1)
    UntypedAtomic,

    // Primitive types (19 types)
    /// xs:string
    String,
    /// xs:boolean
    Boolean,
    /// xs:decimal
    Decimal,
    /// xs:float
    Float,
    /// xs:double
    Double,
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
    /// xs:hexBinary
    HexBinary,
    /// xs:base64Binary
    Base64Binary,
    /// xs:anyURI
    AnyURI,
    /// xs:QName
    QName,
    /// xs:NOTATION
    NOTATION,

    // Derived string types
    /// xs:normalizedString
    NormalizedString,
    /// xs:token
    Token,
    /// xs:language
    Language,
    /// xs:NMTOKEN
    NMTOKEN,
    /// xs:NMTOKENS (list type)
    NMTOKENS,
    /// xs:Name
    Name,
    /// xs:NCName
    NCName,
    /// xs:ID
    ID,
    /// xs:IDREF
    IDREF,
    /// xs:IDREFS (list type)
    IDREFS,
    /// xs:ENTITY
    ENTITY,
    /// xs:ENTITIES (list type)
    ENTITIES,

    // Derived numeric types (integer hierarchy)
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

    // XSD 1.1 derived types
    /// xs:yearMonthDuration (XSD 1.1)
    YearMonthDuration,
    /// xs:dayTimeDuration (XSD 1.1)
    DayTimeDuration,
    /// xs:dateTimeStamp (XSD 1.1)
    DateTimeStamp,
    /// xs:error - the bottom type (union of no members); has no valid values (XSD 1.1)
    XsError,
}

impl BuiltInType {
    /// Get the local name of this built-in type.
    ///
    /// Delegates to [`XmlTypeCode::local_name`] for consistency.
    #[inline]
    pub fn local_name(&self) -> &'static str {
        // Delegate to XmlTypeCode which has the canonical names
        self.type_code()
            .local_name()
            .expect("BuiltInType always has a local name")
    }

    /// Get the corresponding [`XmlTypeCode`] for this built-in type.
    #[inline]
    pub fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::from(*self)
    }

    /// Get the primitive type code for this built-in type.
    ///
    /// Returns `None` for abstract types (AnySimpleType, AnyAtomicType)
    /// and list types (NMTOKENS, IDREFS, ENTITIES).
    #[inline]
    pub fn primitive_type_code(&self) -> Option<PrimitiveTypeCode> {
        PrimitiveTypeCode::from_type_code(self.type_code())
    }

    /// Check if this is a primitive type (one of the 19 fundamental types).
    #[inline]
    pub fn is_primitive(&self) -> bool {
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
                | Self::AnyURI
                | Self::QName
                | Self::NOTATION
        )
    }

    /// Check if this is a list type (NMTOKENS, IDREFS, ENTITIES).
    #[inline]
    pub fn is_list(&self) -> bool {
        matches!(self, Self::NMTOKENS | Self::IDREFS | Self::ENTITIES)
    }

    /// Check if this is an XSD 1.1 type.
    #[inline]
    pub fn is_xsd11(&self) -> bool {
        matches!(
            self,
            Self::AnyAtomicType
                | Self::UntypedAtomic
                | Self::YearMonthDuration
                | Self::DayTimeDuration
                | Self::DateTimeStamp
                | Self::XsError
        )
    }

    /// Parse a built-in type from its local name.
    ///
    /// Delegates to [`XmlTypeCode::from_local_name`] for consistency.
    pub fn from_local_name(name: &str) -> Option<BuiltInType> {
        XmlTypeCode::from_local_name(name).and_then(|code| Self::try_from(code).ok())
    }

    /// Returns an iterator over all built-in types.
    pub fn all() -> impl Iterator<Item = BuiltInType> {
        [
            Self::AnySimpleType,
            Self::AnyAtomicType,
            Self::UntypedAtomic,
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
            Self::AnyURI,
            Self::QName,
            Self::NOTATION,
            Self::NormalizedString,
            Self::Token,
            Self::Language,
            Self::NMTOKEN,
            Self::NMTOKENS,
            Self::Name,
            Self::NCName,
            Self::ID,
            Self::IDREF,
            Self::IDREFS,
            Self::ENTITY,
            Self::ENTITIES,
            Self::Integer,
            Self::NonPositiveInteger,
            Self::NegativeInteger,
            Self::Long,
            Self::Int,
            Self::Short,
            Self::Byte,
            Self::NonNegativeInteger,
            Self::UnsignedLong,
            Self::UnsignedInt,
            Self::UnsignedShort,
            Self::UnsignedByte,
            Self::PositiveInteger,
            Self::YearMonthDuration,
            Self::DayTimeDuration,
            Self::DateTimeStamp,
            Self::XsError,
        ]
        .into_iter()
    }
}

// ============================================================================
// Conversion between BuiltInType and XmlTypeCode
// ============================================================================

impl From<BuiltInType> for XmlTypeCode {
    fn from(builtin: BuiltInType) -> Self {
        match builtin {
            BuiltInType::AnySimpleType => XmlTypeCode::AnySimpleType,
            BuiltInType::AnyAtomicType => XmlTypeCode::AnyAtomicType,
            BuiltInType::UntypedAtomic => XmlTypeCode::UntypedAtomic,
            BuiltInType::String => XmlTypeCode::String,
            BuiltInType::Boolean => XmlTypeCode::Boolean,
            BuiltInType::Decimal => XmlTypeCode::Decimal,
            BuiltInType::Float => XmlTypeCode::Float,
            BuiltInType::Double => XmlTypeCode::Double,
            BuiltInType::Duration => XmlTypeCode::Duration,
            BuiltInType::DateTime => XmlTypeCode::DateTime,
            BuiltInType::Time => XmlTypeCode::Time,
            BuiltInType::Date => XmlTypeCode::Date,
            BuiltInType::GYearMonth => XmlTypeCode::GYearMonth,
            BuiltInType::GYear => XmlTypeCode::GYear,
            BuiltInType::GMonthDay => XmlTypeCode::GMonthDay,
            BuiltInType::GDay => XmlTypeCode::GDay,
            BuiltInType::GMonth => XmlTypeCode::GMonth,
            BuiltInType::HexBinary => XmlTypeCode::HexBinary,
            BuiltInType::Base64Binary => XmlTypeCode::Base64Binary,
            BuiltInType::AnyURI => XmlTypeCode::AnyUri,
            BuiltInType::QName => XmlTypeCode::QName,
            BuiltInType::NOTATION => XmlTypeCode::Notation,
            BuiltInType::NormalizedString => XmlTypeCode::NormalizedString,
            BuiltInType::Token => XmlTypeCode::Token,
            BuiltInType::Language => XmlTypeCode::Language,
            BuiltInType::NMTOKEN => XmlTypeCode::NmToken,
            BuiltInType::NMTOKENS => XmlTypeCode::NmTokens,
            BuiltInType::Name => XmlTypeCode::Name,
            BuiltInType::NCName => XmlTypeCode::NCName,
            BuiltInType::ID => XmlTypeCode::Id,
            BuiltInType::IDREF => XmlTypeCode::IdRef,
            BuiltInType::IDREFS => XmlTypeCode::IdRefs,
            BuiltInType::ENTITY => XmlTypeCode::Entity,
            BuiltInType::ENTITIES => XmlTypeCode::Entities,
            BuiltInType::Integer => XmlTypeCode::Integer,
            BuiltInType::NonPositiveInteger => XmlTypeCode::NonPositiveInteger,
            BuiltInType::NegativeInteger => XmlTypeCode::NegativeInteger,
            BuiltInType::Long => XmlTypeCode::Long,
            BuiltInType::Int => XmlTypeCode::Int,
            BuiltInType::Short => XmlTypeCode::Short,
            BuiltInType::Byte => XmlTypeCode::Byte,
            BuiltInType::NonNegativeInteger => XmlTypeCode::NonNegativeInteger,
            BuiltInType::UnsignedLong => XmlTypeCode::UnsignedLong,
            BuiltInType::UnsignedInt => XmlTypeCode::UnsignedInt,
            BuiltInType::UnsignedShort => XmlTypeCode::UnsignedShort,
            BuiltInType::UnsignedByte => XmlTypeCode::UnsignedByte,
            BuiltInType::PositiveInteger => XmlTypeCode::PositiveInteger,
            BuiltInType::YearMonthDuration => XmlTypeCode::YearMonthDuration,
            BuiltInType::DayTimeDuration => XmlTypeCode::DayTimeDuration,
            BuiltInType::DateTimeStamp => XmlTypeCode::DateTimeStamp,
            BuiltInType::XsError => XmlTypeCode::Error,
        }
    }
}

impl TryFrom<XmlTypeCode> for BuiltInType {
    type Error = ();

    /// Convert from XmlTypeCode to BuiltInType.
    ///
    /// Returns `Err(())` for node types, AnyType, and other non-simple type codes.
    fn try_from(code: XmlTypeCode) -> Result<Self, Self::Error> {
        match code {
            XmlTypeCode::AnySimpleType => Ok(BuiltInType::AnySimpleType),
            XmlTypeCode::AnyAtomicType => Ok(BuiltInType::AnyAtomicType),
            XmlTypeCode::UntypedAtomic => Ok(BuiltInType::UntypedAtomic),
            XmlTypeCode::String => Ok(BuiltInType::String),
            XmlTypeCode::Boolean => Ok(BuiltInType::Boolean),
            XmlTypeCode::Decimal => Ok(BuiltInType::Decimal),
            XmlTypeCode::Float => Ok(BuiltInType::Float),
            XmlTypeCode::Double => Ok(BuiltInType::Double),
            XmlTypeCode::Duration => Ok(BuiltInType::Duration),
            XmlTypeCode::DateTime => Ok(BuiltInType::DateTime),
            XmlTypeCode::Time => Ok(BuiltInType::Time),
            XmlTypeCode::Date => Ok(BuiltInType::Date),
            XmlTypeCode::GYearMonth => Ok(BuiltInType::GYearMonth),
            XmlTypeCode::GYear => Ok(BuiltInType::GYear),
            XmlTypeCode::GMonthDay => Ok(BuiltInType::GMonthDay),
            XmlTypeCode::GDay => Ok(BuiltInType::GDay),
            XmlTypeCode::GMonth => Ok(BuiltInType::GMonth),
            XmlTypeCode::HexBinary => Ok(BuiltInType::HexBinary),
            XmlTypeCode::Base64Binary => Ok(BuiltInType::Base64Binary),
            XmlTypeCode::AnyUri => Ok(BuiltInType::AnyURI),
            XmlTypeCode::QName => Ok(BuiltInType::QName),
            XmlTypeCode::Notation => Ok(BuiltInType::NOTATION),
            XmlTypeCode::NormalizedString => Ok(BuiltInType::NormalizedString),
            XmlTypeCode::Token => Ok(BuiltInType::Token),
            XmlTypeCode::Language => Ok(BuiltInType::Language),
            XmlTypeCode::NmToken => Ok(BuiltInType::NMTOKEN),
            XmlTypeCode::NmTokens => Ok(BuiltInType::NMTOKENS),
            XmlTypeCode::Name => Ok(BuiltInType::Name),
            XmlTypeCode::NCName => Ok(BuiltInType::NCName),
            XmlTypeCode::Id => Ok(BuiltInType::ID),
            XmlTypeCode::IdRef => Ok(BuiltInType::IDREF),
            XmlTypeCode::IdRefs => Ok(BuiltInType::IDREFS),
            XmlTypeCode::Entity => Ok(BuiltInType::ENTITY),
            XmlTypeCode::Entities => Ok(BuiltInType::ENTITIES),
            XmlTypeCode::Integer => Ok(BuiltInType::Integer),
            XmlTypeCode::NonPositiveInteger => Ok(BuiltInType::NonPositiveInteger),
            XmlTypeCode::NegativeInteger => Ok(BuiltInType::NegativeInteger),
            XmlTypeCode::Long => Ok(BuiltInType::Long),
            XmlTypeCode::Int => Ok(BuiltInType::Int),
            XmlTypeCode::Short => Ok(BuiltInType::Short),
            XmlTypeCode::Byte => Ok(BuiltInType::Byte),
            XmlTypeCode::NonNegativeInteger => Ok(BuiltInType::NonNegativeInteger),
            XmlTypeCode::UnsignedLong => Ok(BuiltInType::UnsignedLong),
            XmlTypeCode::UnsignedInt => Ok(BuiltInType::UnsignedInt),
            XmlTypeCode::UnsignedShort => Ok(BuiltInType::UnsignedShort),
            XmlTypeCode::UnsignedByte => Ok(BuiltInType::UnsignedByte),
            XmlTypeCode::PositiveInteger => Ok(BuiltInType::PositiveInteger),
            XmlTypeCode::YearMonthDuration => Ok(BuiltInType::YearMonthDuration),
            XmlTypeCode::DayTimeDuration => Ok(BuiltInType::DayTimeDuration),
            XmlTypeCode::DateTimeStamp => Ok(BuiltInType::DateTimeStamp),
            XmlTypeCode::Error => Ok(BuiltInType::XsError),
            // Node types, AnyType, None, Item are not simple types
            _ => Err(()),
        }
    }
}

/// Simple type definition
///
/// Represents an XSD simple type which can be atomic (restriction of base),
/// list (whitespace-separated items), or union (one of multiple types).
#[derive(Debug, Clone)]
pub struct SimpleTypeDef {
    /// Name (None for anonymous types)
    pub name: Option<NameId>,

    /// Target namespace
    pub target_namespace: Option<NameId>,

    /// Source location for error reporting
    pub source: Option<SourceRef>,

    /// Type variety (atomic, list, or union)
    pub variety: SimpleTypeVariety,

    /// Derivation method (restriction, list, or union)
    pub derivation_method: SimpleTypeDerivationMethod,

    /// Base type definition (for atomic types derived by restriction)
    pub base_type: Option<SimpleTypeRef>,

    /// Item type (for list types)
    pub item_type: Option<SimpleTypeRef>,

    /// Member types (for union types)
    pub member_types: Vec<SimpleTypeRef>,

    /// Constraining facets
    pub facets: FacetSet,

    /// Type code for built-in types (or derived types)
    ///
    /// For built-in types, this is the corresponding XmlTypeCode.
    /// For user-defined types derived from built-in types, this
    /// may be set to the base type's code for quick type checking.
    pub type_code: XmlTypeCode,

    /// Primitive type code for atomic types
    ///
    /// For atomic types, this indicates which primitive type they
    /// ultimately derive from (one of the 19 XSD primitive types).
    /// This is `None` for list types, union types, and abstract types.
    pub primitive_type: Option<PrimitiveTypeCode>,

    /// Final derivation control
    pub final_derivation: DerivationSet,

    /// ID attribute value (for identity)
    pub id: Option<String>,
}

impl SimpleTypeDef {
    /// Create a new simple type with restriction variety
    pub fn new_restriction(
        name: Option<NameId>,
        target_namespace: Option<NameId>,
        base_type: SimpleTypeRef,
    ) -> Self {
        Self {
            name,
            target_namespace,
            source: None,
            variety: SimpleTypeVariety::Atomic,
            derivation_method: SimpleTypeDerivationMethod::Restriction,
            base_type: Some(base_type),
            item_type: None,
            member_types: Vec::new(),
            facets: FacetSet::new(),
            type_code: XmlTypeCode::None,
            primitive_type: None,
            final_derivation: DerivationSet::empty(),
            id: None,
        }
    }

    /// Create a new list type
    pub fn new_list(
        name: Option<NameId>,
        target_namespace: Option<NameId>,
        item_type: SimpleTypeRef,
    ) -> Self {
        Self {
            name,
            target_namespace,
            source: None,
            variety: SimpleTypeVariety::List,
            derivation_method: SimpleTypeDerivationMethod::List,
            base_type: None,
            item_type: Some(item_type),
            member_types: Vec::new(),
            facets: FacetSet::new(),
            type_code: XmlTypeCode::None,
            primitive_type: None, // List types have no primitive type
            final_derivation: DerivationSet::empty(),
            id: None,
        }
    }

    /// Create a new union type
    pub fn new_union(
        name: Option<NameId>,
        target_namespace: Option<NameId>,
        member_types: Vec<SimpleTypeRef>,
    ) -> Self {
        Self {
            name,
            target_namespace,
            source: None,
            variety: SimpleTypeVariety::Union,
            derivation_method: SimpleTypeDerivationMethod::Union,
            base_type: None,
            item_type: None,
            member_types,
            facets: FacetSet::new(),
            type_code: XmlTypeCode::None,
            primitive_type: None, // Union types have no primitive type
            final_derivation: DerivationSet::empty(),
            id: None,
        }
    }

    /// Create a simple type for a built-in type
    ///
    /// This constructor is used when registering built-in types with their
    /// proper type code and primitive type information.
    pub fn new_builtin(
        name: NameId,
        target_namespace: Option<NameId>,
        builtin: BuiltInType,
    ) -> Self {
        let type_code = builtin.type_code();
        let primitive_type = builtin.primitive_type_code();
        let variety = if builtin.is_list() {
            SimpleTypeVariety::List
        } else {
            SimpleTypeVariety::Atomic
        };
        let derivation_method = if builtin.is_list() {
            SimpleTypeDerivationMethod::List
        } else {
            SimpleTypeDerivationMethod::Restriction
        };

        Self {
            name: Some(name),
            target_namespace,
            source: None,
            variety,
            derivation_method,
            base_type: None, // Will be set during registry initialization
            item_type: None, // Will be set for list types
            member_types: Vec::new(),
            facets: default_facets_for_builtin(builtin),
            type_code,
            primitive_type,
            final_derivation: DerivationSet::empty(),
            id: None,
        }
    }

    /// Check if this is an anonymous type
    pub fn is_anonymous(&self) -> bool {
        self.name.is_none()
    }

    /// Check if this is a global (named) type
    pub fn is_global(&self) -> bool {
        self.name.is_some()
    }

    /// Check if this is an atomic type
    pub fn is_atomic(&self) -> bool {
        self.variety == SimpleTypeVariety::Atomic
    }

    /// Check if this is a list type
    pub fn is_list(&self) -> bool {
        self.variety == SimpleTypeVariety::List
    }

    /// Check if this is a union type
    pub fn is_union(&self) -> bool {
        self.variety == SimpleTypeVariety::Union
    }

    /// Check if this type was derived by restriction
    pub fn is_restriction(&self) -> bool {
        self.derivation_method == SimpleTypeDerivationMethod::Restriction
    }

    /// Get the primitive type code for this simple type
    ///
    /// Returns the primitive type from which this type ultimately derives.
    /// Returns `None` for list types, union types, and abstract types.
    pub fn get_primitive_type(&self) -> Option<PrimitiveTypeCode> {
        self.primitive_type
    }

    /// Get the type code for this simple type
    pub fn get_type_code(&self) -> XmlTypeCode {
        self.type_code
    }

    /// Get the TypeKey for this simple type (requires its key)
    pub fn type_key(&self, key: SimpleTypeKey) -> TypeKey {
        TypeKey::Simple(key)
    }
}

/// Default facets for built-in types — delegates to
/// [`effective_arena_facets_for_builtin`] to keep the parser-frame
/// `SimpleTypeDef` and the arena `SimpleTypeDefData` paths in sync.
pub fn default_facets_for_builtin(builtin: BuiltInType) -> FacetSet {
    effective_arena_facets_for_builtin(builtin)
}

/// Effective {facets} for a built-in type after walking the entire derivation
/// chain, per XSD Datatypes Part 2. Bakes whiteSpace, integer-hierarchy
/// fractionDigits=0, bounded-integer min/maxInclusive, and list minLength=1
/// into the arena's `SimpleTypeDefData.facets` so user-derived types see
/// the proper inherited facets through `merge_with_base`. The runtime
/// equivalent for user-derived types is `validation::simple::collect_facets`.
pub fn effective_arena_facets_for_builtin(builtin: BuiltInType) -> FacetSet {
    use super::facets::{FacetFixed, WhitespaceMode};

    let mut facets = FacetSet::new();

    // String/NormalizedString leave whiteSpace unfixed so descendants can
    // tighten preserve→replace→collapse; AnySimpleType/AnyAtomicType/XsError
    // omit it entirely (cos-applicable-facets rejects whiteSpace on unions),
    // and so does the XPath data model's UntypedAtomic, whose values are not
    // normalized (XDM 1.0 §2.6.4: "no validation takes place").
    match builtin {
        BuiltInType::String => {
            facets.set_whitespace(WhitespaceMode::Preserve, FacetFixed::Default, None);
        }
        BuiltInType::NormalizedString => {
            facets.set_whitespace(WhitespaceMode::Replace, FacetFixed::Default, None);
        }
        BuiltInType::AnySimpleType
        | BuiltInType::AnyAtomicType
        | BuiltInType::UntypedAtomic
        | BuiltInType::XsError => {}
        _ => {
            facets.set_whitespace(WhitespaceMode::Collapse, FacetFixed::Fixed, None);
        }
    }

    if let Some((lo, hi)) = integer_hierarchy_bounds(builtin) {
        facets.set_fraction_digits(0, FacetFixed::Fixed, None);
        if let Some(v) = lo {
            facets.set_min_inclusive(v, FacetFixed::Default, None);
        }
        if let Some(v) = hi {
            facets.set_max_inclusive(v, FacetFixed::Default, None);
        }
    }

    if matches!(
        builtin,
        BuiltInType::IDREFS | BuiltInType::NMTOKENS | BuiltInType::ENTITIES
    ) {
        facets.set_min_length(1, FacetFixed::Default, None);
    }

    facets
}

/// Returns `Some((minInclusive?, maxInclusive?))` if `builtin` is in the
/// xs:integer derivation chain, `None` otherwise. Bounds derive from the
/// std numeric constants so they cannot drift from the underlying type.
fn integer_hierarchy_bounds(builtin: BuiltInType) -> Option<(Option<String>, Option<String>)> {
    let bounds = match builtin {
        BuiltInType::Integer => (None, None),
        BuiltInType::NonPositiveInteger => (None, Some("0".to_string())),
        BuiltInType::NegativeInteger => (None, Some("-1".to_string())),
        BuiltInType::NonNegativeInteger => (Some("0".to_string()), None),
        BuiltInType::PositiveInteger => (Some("1".to_string()), None),
        BuiltInType::Long => (Some(i64::MIN.to_string()), Some(i64::MAX.to_string())),
        BuiltInType::Int => (Some(i32::MIN.to_string()), Some(i32::MAX.to_string())),
        BuiltInType::Short => (Some(i16::MIN.to_string()), Some(i16::MAX.to_string())),
        BuiltInType::Byte => (Some(i8::MIN.to_string()), Some(i8::MAX.to_string())),
        BuiltInType::UnsignedLong => (Some("0".to_string()), Some(u64::MAX.to_string())),
        BuiltInType::UnsignedInt => (Some("0".to_string()), Some(u32::MAX.to_string())),
        BuiltInType::UnsignedShort => (Some("0".to_string()), Some(u16::MAX.to_string())),
        BuiltInType::UnsignedByte => (Some("0".to_string()), Some(u8::MAX.to_string())),
        _ => return None,
    };
    Some(bounds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_type_names() {
        assert_eq!(BuiltInType::String.local_name(), "string");
        assert_eq!(BuiltInType::Integer.local_name(), "integer");
        assert_eq!(BuiltInType::DateTime.local_name(), "dateTime");
        assert_eq!(BuiltInType::AnyURI.local_name(), "anyURI");
        assert_eq!(BuiltInType::NMTOKEN.local_name(), "NMTOKEN");
        assert_eq!(BuiltInType::UntypedAtomic.local_name(), "untypedAtomic");
        assert_eq!(
            BuiltInType::YearMonthDuration.local_name(),
            "yearMonthDuration"
        );
    }

    #[test]
    fn test_builtin_type_parsing() {
        assert_eq!(
            BuiltInType::from_local_name("string"),
            Some(BuiltInType::String)
        );
        assert_eq!(
            BuiltInType::from_local_name("integer"),
            Some(BuiltInType::Integer)
        );
        assert_eq!(
            BuiltInType::from_local_name("untypedAtomic"),
            Some(BuiltInType::UntypedAtomic)
        );
        assert_eq!(
            BuiltInType::from_local_name("yearMonthDuration"),
            Some(BuiltInType::YearMonthDuration)
        );
        assert_eq!(BuiltInType::from_local_name("nonExistent"), None);
        // anyType is not a simple type
        assert_eq!(BuiltInType::from_local_name("anyType"), None);
    }

    #[test]
    fn test_builtin_is_primitive() {
        assert!(BuiltInType::String.is_primitive());
        assert!(BuiltInType::Decimal.is_primitive());
        assert!(BuiltInType::Duration.is_primitive());
        assert!(!BuiltInType::Integer.is_primitive()); // Derived from decimal
        assert!(!BuiltInType::NCName.is_primitive()); // Derived from Name
        assert!(!BuiltInType::AnySimpleType.is_primitive()); // Abstract
        assert!(!BuiltInType::NMTOKENS.is_primitive()); // List type
    }

    #[test]
    fn test_builtin_is_list() {
        assert!(BuiltInType::NMTOKENS.is_list());
        assert!(BuiltInType::IDREFS.is_list());
        assert!(BuiltInType::ENTITIES.is_list());
        assert!(!BuiltInType::NMTOKEN.is_list());
        assert!(!BuiltInType::String.is_list());
    }

    #[test]
    fn test_builtin_is_xsd11() {
        assert!(BuiltInType::AnyAtomicType.is_xsd11());
        assert!(BuiltInType::UntypedAtomic.is_xsd11());
        assert!(BuiltInType::YearMonthDuration.is_xsd11());
        assert!(BuiltInType::DayTimeDuration.is_xsd11());
        assert!(BuiltInType::DateTimeStamp.is_xsd11());
        assert!(!BuiltInType::String.is_xsd11());
        assert!(!BuiltInType::DateTime.is_xsd11());
    }

    #[test]
    fn test_builtin_type_code() {
        assert_eq!(BuiltInType::String.type_code(), XmlTypeCode::String);
        assert_eq!(BuiltInType::Integer.type_code(), XmlTypeCode::Integer);
        assert_eq!(BuiltInType::NMTOKEN.type_code(), XmlTypeCode::NmToken);
        assert_eq!(BuiltInType::AnyURI.type_code(), XmlTypeCode::AnyUri);
    }

    #[test]
    fn test_builtin_primitive_type_code() {
        assert_eq!(
            BuiltInType::String.primitive_type_code(),
            Some(PrimitiveTypeCode::String)
        );
        assert_eq!(
            BuiltInType::NCName.primitive_type_code(),
            Some(PrimitiveTypeCode::String)
        );
        assert_eq!(
            BuiltInType::Integer.primitive_type_code(),
            Some(PrimitiveTypeCode::Decimal)
        );
        assert_eq!(
            BuiltInType::Duration.primitive_type_code(),
            Some(PrimitiveTypeCode::Duration)
        );
        assert_eq!(
            BuiltInType::YearMonthDuration.primitive_type_code(),
            Some(PrimitiveTypeCode::Duration)
        );
        // Abstract types and list types have no primitive
        assert_eq!(BuiltInType::AnySimpleType.primitive_type_code(), None);
        assert_eq!(BuiltInType::NMTOKENS.primitive_type_code(), None);
    }

    #[test]
    fn test_builtin_to_xml_type_code_conversion() {
        // Test From<BuiltInType> for XmlTypeCode
        assert_eq!(XmlTypeCode::from(BuiltInType::String), XmlTypeCode::String);
        assert_eq!(
            XmlTypeCode::from(BuiltInType::NOTATION),
            XmlTypeCode::Notation
        );
        assert_eq!(XmlTypeCode::from(BuiltInType::AnyURI), XmlTypeCode::AnyUri);
        assert_eq!(
            XmlTypeCode::from(BuiltInType::NMTOKEN),
            XmlTypeCode::NmToken
        );
    }

    #[test]
    fn test_xml_type_code_to_builtin_conversion() {
        // Test TryFrom<XmlTypeCode> for BuiltInType
        assert_eq!(
            BuiltInType::try_from(XmlTypeCode::String),
            Ok(BuiltInType::String)
        );
        assert_eq!(
            BuiltInType::try_from(XmlTypeCode::Notation),
            Ok(BuiltInType::NOTATION)
        );
        assert_eq!(
            BuiltInType::try_from(XmlTypeCode::AnyUri),
            Ok(BuiltInType::AnyURI)
        );
        assert_eq!(
            BuiltInType::try_from(XmlTypeCode::NmToken),
            Ok(BuiltInType::NMTOKEN)
        );

        // Node types and AnyType should fail
        assert!(BuiltInType::try_from(XmlTypeCode::None).is_err());
        assert!(BuiltInType::try_from(XmlTypeCode::Element).is_err());
        assert!(BuiltInType::try_from(XmlTypeCode::AnyType).is_err());
    }

    #[test]
    fn test_builtin_roundtrip_conversion() {
        // All BuiltInTypes should roundtrip through XmlTypeCode
        for builtin in BuiltInType::all() {
            let code = XmlTypeCode::from(builtin);
            let back = BuiltInType::try_from(code).expect("Should convert back");
            assert_eq!(back, builtin, "Roundtrip failed for {:?}", builtin);
        }
    }

    #[test]
    fn test_builtin_all_count() {
        assert_eq!(BuiltInType::all().count(), 51);
    }

    #[test]
    fn test_simple_type_restriction() {
        let st = SimpleTypeDef::new_restriction(
            Some(NameId(1)),
            Some(NameId(2)),
            SimpleTypeRef::BuiltIn(BuiltInType::String),
        );

        assert_eq!(st.variety, SimpleTypeVariety::Atomic);
        assert_eq!(
            st.derivation_method,
            SimpleTypeDerivationMethod::Restriction
        );
        assert!(st.is_global());
        assert!(st.is_atomic());
        assert!(st.is_restriction());
        assert!(st.base_type.is_some());
        assert_eq!(st.type_code, XmlTypeCode::None); // Not set by default
        assert!(st.primitive_type.is_none()); // Not set by default
    }

    #[test]
    fn test_simple_type_list() {
        let st = SimpleTypeDef::new_list(None, None, SimpleTypeRef::BuiltIn(BuiltInType::Integer));

        assert_eq!(st.variety, SimpleTypeVariety::List);
        assert_eq!(st.derivation_method, SimpleTypeDerivationMethod::List);
        assert!(st.is_anonymous());
        assert!(st.is_list());
        assert!(st.item_type.is_some());
        assert!(st.primitive_type.is_none()); // List types have no primitive
    }

    #[test]
    fn test_simple_type_union() {
        let st = SimpleTypeDef::new_union(
            Some(NameId(1)),
            None,
            vec![
                SimpleTypeRef::BuiltIn(BuiltInType::String),
                SimpleTypeRef::BuiltIn(BuiltInType::Integer),
            ],
        );

        assert_eq!(st.variety, SimpleTypeVariety::Union);
        assert_eq!(st.derivation_method, SimpleTypeDerivationMethod::Union);
        assert!(st.is_union());
        assert_eq!(st.member_types.len(), 2);
        assert!(st.primitive_type.is_none()); // Union types have no primitive
    }

    #[test]
    fn test_simple_type_builtin_atomic() {
        let st = SimpleTypeDef::new_builtin(NameId(1), Some(NameId(2)), BuiltInType::String);

        assert_eq!(st.variety, SimpleTypeVariety::Atomic);
        assert_eq!(
            st.derivation_method,
            SimpleTypeDerivationMethod::Restriction
        );
        assert!(st.is_atomic());
        assert_eq!(st.type_code, XmlTypeCode::String);
        assert_eq!(st.primitive_type, Some(PrimitiveTypeCode::String));
    }

    #[test]
    fn test_simple_type_builtin_list() {
        let st = SimpleTypeDef::new_builtin(NameId(1), Some(NameId(2)), BuiltInType::NMTOKENS);

        assert_eq!(st.variety, SimpleTypeVariety::List);
        assert_eq!(st.derivation_method, SimpleTypeDerivationMethod::List);
        assert!(st.is_list());
        assert_eq!(st.type_code, XmlTypeCode::NmTokens);
        assert!(st.primitive_type.is_none()); // List types have no primitive
    }

    #[test]
    fn test_simple_type_builtin_derived() {
        // Test a derived type like integer (derived from decimal)
        let st = SimpleTypeDef::new_builtin(NameId(1), Some(NameId(2)), BuiltInType::Integer);

        assert_eq!(st.type_code, XmlTypeCode::Integer);
        assert_eq!(st.primitive_type, Some(PrimitiveTypeCode::Decimal));
    }

    #[test]
    fn test_simple_type_derivation_method_default() {
        assert_eq!(
            SimpleTypeDerivationMethod::default(),
            SimpleTypeDerivationMethod::Restriction
        );
    }

    #[test]
    fn test_simple_type_variety_copy() {
        let variety = SimpleTypeVariety::Atomic;
        let copy = variety;
        assert_eq!(variety, copy);
    }

    #[test]
    fn test_default_facets() {
        let string_facets = default_facets_for_builtin(BuiltInType::String);
        assert!(string_facets.whitespace.is_some());

        let int_facets = default_facets_for_builtin(BuiltInType::Integer);
        assert!(int_facets.whitespace.is_some());
    }

    #[test]
    fn test_simple_type_helper_methods() {
        let restriction = SimpleTypeDef::new_restriction(
            Some(NameId(1)),
            None,
            SimpleTypeRef::BuiltIn(BuiltInType::String),
        );
        assert!(restriction.is_atomic());
        assert!(!restriction.is_list());
        assert!(!restriction.is_union());
        assert!(restriction.is_restriction());

        let list = SimpleTypeDef::new_list(None, None, SimpleTypeRef::BuiltIn(BuiltInType::String));
        assert!(!list.is_atomic());
        assert!(list.is_list());
        assert!(!list.is_union());

        let union = SimpleTypeDef::new_union(
            None,
            None,
            vec![SimpleTypeRef::BuiltIn(BuiltInType::String)],
        );
        assert!(!union.is_atomic());
        assert!(!union.is_list());
        assert!(union.is_union());
    }

    #[test]
    fn test_simple_type_get_type_code() {
        let st = SimpleTypeDef::new_builtin(NameId(1), None, BuiltInType::DateTime);
        assert_eq!(st.get_type_code(), XmlTypeCode::DateTime);
    }

    #[test]
    fn test_simple_type_get_primitive_type() {
        let st = SimpleTypeDef::new_builtin(NameId(1), None, BuiltInType::NCName);
        assert_eq!(st.get_primitive_type(), Some(PrimitiveTypeCode::String));

        let list_st = SimpleTypeDef::new_builtin(NameId(2), None, BuiltInType::IDREFS);
        assert_eq!(list_st.get_primitive_type(), None);
    }
}
