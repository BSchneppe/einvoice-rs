//! Type validators for XSD atomic types
//!
//! This module provides the `TypeValidator` trait and `ValidatorRegistry` for
//! parsing, validating, and formatting XSD atomic type values.
//!
//! ## Design
//!
//! - `TypeValidator` trait defines the interface for type-specific validation
//! - `ValidatorRegistry` provides lookup and registration of validators
//! - Built-in validators cover all 19 primitive XSD types

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use num_bigint::BigInt;
use once_cell::sync::Lazy;
use rust_decimal::Decimal;

use super::facets::{normalize_whitespace, FacetSet, WhitespaceMode};
use super::value::{
    DateTimeValue, DateValue, DayTimeDurationValue, DurationValue, GDayValue, GMonthDayValue,
    GMonthValue, GYearMonthValue, GYearValue, TimeValue, TimezoneOffset, XmlAtomicValue, XmlValue,
    XmlValueKind, YearMonthDurationValue,
};
use super::value_order::FacetOrder;
use super::{PrimitiveTypeCode, XmlTypeCode};
use crate::error::FacetError;

/// Error type for validation operations
#[derive(Debug, Clone)]
pub enum ValidationError {
    /// Invalid lexical representation
    InvalidLexical {
        value: String,
        type_name: &'static str,
        message: String,
    },
    /// Facet constraint violation
    FacetViolation(FacetError),
    /// Type error (wrong type for operation)
    TypeError {
        expected: XmlTypeCode,
        actual: XmlTypeCode,
    },
    /// Range error (value out of range for type)
    RangeError {
        value: String,
        type_name: &'static str,
    },
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidLexical {
                value,
                type_name,
                message,
            } => {
                write!(f, "Invalid {} value '{}': {}", type_name, value, message)
            }
            Self::FacetViolation(e) => write!(f, "{}", e),
            Self::TypeError { expected, actual } => {
                write!(f, "Type error: expected {:?}, got {:?}", expected, actual)
            }
            Self::RangeError { value, type_name } => {
                write!(f, "Value '{}' out of range for type {}", value, type_name)
            }
        }
    }
}

impl std::error::Error for ValidationError {}

impl From<FacetError> for ValidationError {
    fn from(e: FacetError) -> Self {
        Self::FacetViolation(e)
    }
}

/// Result type for validation operations
pub type ValidationResult<T> = Result<T, ValidationError>;

/// Build the `validate_enum_value_space` closure for a type whose instance
/// value has been parsed. Each enumeration candidate lexical is normalized
/// (whitespace=collapse), parsed with `$parse_fn` (returning `Result` or
/// `Option` with `.ok()`), then compared against `$typed` using `$eq_fn`.
///
/// Used by Float / Double / Duration validators — §cvc-enumeration-valid
/// compares in value space (not lexically) for these types.
macro_rules! enum_matches_value_space {
    ($typed:expr, $parse_fn:expr, $eq_fn:expr) => {
        |s: &str| {
            $parse_fn(&normalize_whitespace(s, WhitespaceMode::Collapse))
                .ok()
                .map(|e| $eq_fn($typed, e))
                .unwrap_or(false)
        }
    };
}

/// [`enum_matches_value_space!`] for the datatypes with seconds: equality in
/// the facet order on the two literals
/// ([`FacetOrder::facet_cmp_literal`]), exact also for seconds with more
/// fraction digits than the typed value holds.
macro_rules! enum_matches_literal {
    ($typed:expr, $lexical:expr, $parse_fn:expr) => {
        |s: &str| {
            $parse_fn(&normalize_whitespace(s, WhitespaceMode::Collapse))
                .ok()
                .is_some_and(|e| {
                    $typed.facet_cmp_literal($lexical, &e, s) == Some(std::cmp::Ordering::Equal)
                })
        }
    };
}

/// Trait for XSD type validators
///
/// Validators are responsible for:
/// - Parsing lexical values into typed values
/// - Applying whitespace normalization
/// - Validating against facets
/// - Formatting typed values back to canonical lexical form
pub trait TypeValidator: Send + Sync {
    /// Get the type name (e.g., "string", "integer")
    fn type_name(&self) -> &'static str;

    /// Get the type code for this validator
    fn type_code(&self) -> XmlTypeCode;

    /// Get the primitive type from which this type derives
    fn primitive_type(&self) -> PrimitiveTypeCode;

    /// Get the whitespace normalization mode for this type
    fn whitespace(&self) -> WhitespaceMode;

    /// Parse and validate a lexical value
    fn validate(&self, value: &str) -> ValidationResult<XmlValue>;

    /// Parse a value and apply facets
    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue>;

    /// Check if a facet is applicable to this type
    fn facet_applicable(&self, facet: &str) -> bool;
}

/// Number of `XmlTypeCode` discriminants, `0..=Error` inclusive. `by_code` has
/// one slot per code, so a lookup is a bounds-checked array index. Stays correct
/// as long as `Error` remains the highest discriminant (it is the bottom type,
/// added last); a new higher variant would need this bumped.
const XML_TYPE_CODE_COUNT: usize = XmlTypeCode::Error as usize + 1;

/// Registry of type validators
pub struct ValidatorRegistry {
    /// Validators by type name — consulted during schema setup, not per value.
    validators: HashMap<&'static str, Arc<dyn TypeValidator>>,
    /// Validators by type code — a **dense array** indexed by the `XmlTypeCode`
    /// discriminant. `get_by_code` runs per simple value (~1.8M/run on a
    /// record-heavy document); array indexing avoids the per-lookup SipHash a
    /// `HashMap<XmlTypeCode, _>` paid on every value (`PERF_PROFILE_VALIDATE_ONLY`).
    by_code: [Option<Arc<dyn TypeValidator>>; XML_TYPE_CODE_COUNT],
}

impl ValidatorRegistry {
    /// Create a new registry with all built-in validators
    pub fn new() -> Self {
        let mut registry = Self {
            validators: HashMap::new(),
            by_code: std::array::from_fn(|_| None),
        };

        // Register all primitive type validators
        registry.register(Arc::new(StringValidator));
        registry.register(Arc::new(BooleanValidator));
        registry.register(Arc::new(DecimalValidator));
        registry.register(Arc::new(FloatValidator));
        registry.register(Arc::new(DoubleValidator));
        registry.register(Arc::new(IntegerValidator));
        registry.register(Arc::new(DurationValidator));
        registry.register(Arc::new(DateTimeValidator));
        registry.register(Arc::new(DateValidator));
        registry.register(Arc::new(TimeValidator));
        registry.register(Arc::new(GYearMonthValidator));
        registry.register(Arc::new(GYearValidator));
        registry.register(Arc::new(GMonthDayValidator));
        registry.register(Arc::new(GDayValidator));
        registry.register(Arc::new(GMonthValidator));
        registry.register(Arc::new(HexBinaryValidator));
        registry.register(Arc::new(Base64BinaryValidator));
        registry.register(Arc::new(AnyUriValidator));

        // Register derived string validators
        registry.register(Arc::new(NormalizedStringValidator));
        registry.register(Arc::new(TokenValidator));
        registry.register(Arc::new(LanguageValidator));
        registry.register(Arc::new(NmTokenValidator));
        registry.register(Arc::new(NameValidator));
        registry.register(Arc::new(NCNameValidator));
        registry.register(Arc::new(IdValidator));
        registry.register(Arc::new(IdRefValidator));
        registry.register(Arc::new(EntityValidator));

        // Register integer hierarchy validators
        registry.register(Arc::new(LongValidator));
        registry.register(Arc::new(IntValidator));
        registry.register(Arc::new(ShortValidator));
        registry.register(Arc::new(ByteValidator));
        registry.register(Arc::new(NonNegativeIntegerValidator));
        registry.register(Arc::new(PositiveIntegerValidator));
        registry.register(Arc::new(NonPositiveIntegerValidator));
        registry.register(Arc::new(NegativeIntegerValidator));
        registry.register(Arc::new(UnsignedLongValidator));
        registry.register(Arc::new(UnsignedIntValidator));
        registry.register(Arc::new(UnsignedShortValidator));
        registry.register(Arc::new(UnsignedByteValidator));

        // Register QName and NOTATION validators
        registry.register(Arc::new(QNameValidator));
        registry.register(Arc::new(NotationValidator));

        // Register list type validators
        registry.register(Arc::new(NmTokensValidator));
        registry.register(Arc::new(IdRefsValidator));
        registry.register(Arc::new(EntitiesValidator));

        // Register XSD 1.1 validators
        registry.register(Arc::new(YearMonthDurationValidator));
        registry.register(Arc::new(DayTimeDurationValidator));
        registry.register(Arc::new(DateTimeStampValidator));

        // The XPath 2.0 data model's xs:untypedAtomic
        registry.register(Arc::new(UntypedAtomicValidator));

        registry
    }

    /// Register a validator
    pub fn register(&mut self, validator: Arc<dyn TypeValidator>) {
        let name = validator.type_name();
        let code = validator.type_code();
        self.validators.insert(name, validator.clone());
        self.by_code[code as usize] = Some(validator);
    }

    /// Get a validator by type name
    pub fn get_by_name(&self, name: &str) -> Option<&dyn TypeValidator> {
        self.validators.get(name).map(|v| v.as_ref())
    }

    /// Get a validator by type code
    pub fn get_by_code(&self, code: XmlTypeCode) -> Option<&dyn TypeValidator> {
        self.by_code
            .get(code as usize)
            .and_then(|slot| slot.as_ref().map(|v| v.as_ref()))
    }

    /// Validate a value using the appropriate validator
    pub fn validate(&self, type_code: XmlTypeCode, value: &str) -> ValidationResult<XmlValue> {
        match self.get_by_code(type_code) {
            Some(validator) => validator.validate(value),
            None => Err(ValidationError::InvalidLexical {
                value: value.to_string(),
                type_name: "unknown",
                message: format!("No validator for type code {:?}", type_code),
            }),
        }
    }
}

impl Default for ValidatorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Global validator registry instance.
///
/// This is a lazily-initialized singleton containing all built-in type validators.
/// Use this instead of creating new `ValidatorRegistry` instances to avoid
/// the overhead of re-registering 50+ validators on each call.
///
/// # Example
///
/// ```ignore
/// use crate::types::validators::VALIDATOR_REGISTRY;
///
/// if let Some(validator) = VALIDATOR_REGISTRY.get_by_code(XmlTypeCode::Integer) {
///     let result = validator.validate("42")?;
/// }
/// ```
pub static VALIDATOR_REGISTRY: Lazy<ValidatorRegistry> = Lazy::new(ValidatorRegistry::new);

// ============================================================================
// String Validators
// ============================================================================

/// Validator for xs:string
pub struct StringValidator;

impl TypeValidator for StringValidator {
    fn type_name(&self) -> &'static str {
        "string"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::String
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::String
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Preserve
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        Ok(XmlValue::string(value))
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let ws = facets
            .whitespace
            .as_ref()
            .map(|w| w.value)
            .unwrap_or(WhitespaceMode::Preserve);
        let normalized = normalize_whitespace(value, ws);
        facets.validate_string(&normalized)?;
        Ok(XmlValue::string(normalized.into_owned()))
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "length" | "minLength" | "maxLength" | "pattern" | "enumeration" | "whitespace"
        )
    }
}

/// Validator for xs:normalizedString
pub struct NormalizedStringValidator;

impl TypeValidator for NormalizedStringValidator {
    fn type_name(&self) -> &'static str {
        "normalizedString"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::NormalizedString
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::String
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Replace
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Replace);
        Ok(XmlValue::new(
            XmlTypeCode::NormalizedString,
            XmlValueKind::Atomic(XmlAtomicValue::String(normalized.into_owned())),
        ))
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        // NormalizedString requires at least Replace mode
        let base_ws = facets
            .whitespace
            .as_ref()
            .map(|w| w.value)
            .unwrap_or(WhitespaceMode::Replace);
        let ws = if matches!(base_ws, WhitespaceMode::Collapse) {
            WhitespaceMode::Collapse
        } else {
            WhitespaceMode::Replace
        };
        let normalized = normalize_whitespace(value, ws);
        facets.validate_string(&normalized)?;
        Ok(XmlValue::new(
            XmlTypeCode::NormalizedString,
            XmlValueKind::Atomic(XmlAtomicValue::String(normalized.into_owned())),
        ))
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "length" | "minLength" | "maxLength" | "pattern" | "enumeration" | "whitespace"
        )
    }
}

/// Validator for xs:token
pub struct TokenValidator;

impl TypeValidator for TokenValidator {
    fn type_name(&self) -> &'static str {
        "token"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::Token
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::String
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        Ok(XmlValue::new(
            XmlTypeCode::Token,
            XmlValueKind::Atomic(XmlAtomicValue::String(normalized.into_owned())),
        ))
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        facets.validate_string(&normalized)?;
        Ok(XmlValue::new(
            XmlTypeCode::Token,
            XmlValueKind::Atomic(XmlAtomicValue::String(normalized.into_owned())),
        ))
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "length" | "minLength" | "maxLength" | "pattern" | "enumeration" | "whitespace"
        )
    }
}

/// Validator for xs:language
pub struct LanguageValidator;

impl TypeValidator for LanguageValidator {
    fn type_name(&self) -> &'static str {
        "language"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::Language
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::String
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        // Language pattern: [a-zA-Z]{1,8}(-[a-zA-Z0-9]{1,8})*
        if !is_valid_language(&normalized) {
            return Err(ValidationError::InvalidLexical {
                value: value.to_string(),
                type_name: "language",
                message: "Invalid language tag format".to_string(),
            });
        }
        Ok(XmlValue::new(
            XmlTypeCode::Language,
            XmlValueKind::Atomic(XmlAtomicValue::String(normalized.into_owned())),
        ))
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        facets.validate_string(&result.to_string_value())?;
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "length" | "minLength" | "maxLength" | "pattern" | "enumeration" | "whitespace"
        )
    }
}

/// Validator for xs:NMTOKEN
pub struct NmTokenValidator;

impl TypeValidator for NmTokenValidator {
    fn type_name(&self) -> &'static str {
        "NMTOKEN"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::NmToken
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::String
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        if normalized.is_empty() || !normalized.chars().all(is_name_char) {
            return Err(ValidationError::InvalidLexical {
                value: value.to_string(),
                type_name: "NMTOKEN",
                message: "Must contain only name characters".to_string(),
            });
        }
        Ok(XmlValue::new(
            XmlTypeCode::NmToken,
            XmlValueKind::Atomic(XmlAtomicValue::String(normalized.into_owned())),
        ))
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        facets.validate_string(&result.to_string_value())?;
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "length" | "minLength" | "maxLength" | "pattern" | "enumeration" | "whitespace"
        )
    }
}

/// Validator for xs:Name
pub struct NameValidator;

impl TypeValidator for NameValidator {
    fn type_name(&self) -> &'static str {
        "Name"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::Name
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::String
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        if !is_valid_name(&normalized) {
            return Err(ValidationError::InvalidLexical {
                value: value.to_string(),
                type_name: "Name",
                message: "Invalid XML Name".to_string(),
            });
        }
        Ok(XmlValue::new(
            XmlTypeCode::Name,
            XmlValueKind::Atomic(XmlAtomicValue::String(normalized.into_owned())),
        ))
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        facets.validate_string(&result.to_string_value())?;
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "length" | "minLength" | "maxLength" | "pattern" | "enumeration" | "whitespace"
        )
    }
}

/// Validator for xs:NCName
pub struct NCNameValidator;

impl TypeValidator for NCNameValidator {
    fn type_name(&self) -> &'static str {
        "NCName"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::NCName
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::String
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        if !is_valid_ncname(&normalized) {
            return Err(ValidationError::InvalidLexical {
                value: value.to_string(),
                type_name: "NCName",
                message: "Invalid NCName (Name without colons)".to_string(),
            });
        }
        Ok(XmlValue::new(
            XmlTypeCode::NCName,
            XmlValueKind::Atomic(XmlAtomicValue::String(normalized.into_owned())),
        ))
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        facets.validate_string(&result.to_string_value())?;
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "length" | "minLength" | "maxLength" | "pattern" | "enumeration" | "whitespace"
        )
    }
}

/// Validator for xs:ID
pub struct IdValidator;

impl TypeValidator for IdValidator {
    fn type_name(&self) -> &'static str {
        "ID"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::Id
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::String
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        if !is_valid_ncname(&normalized) {
            return Err(ValidationError::InvalidLexical {
                value: value.to_string(),
                type_name: "ID",
                message: "Invalid ID (must be NCName)".to_string(),
            });
        }
        Ok(XmlValue::new(
            XmlTypeCode::Id,
            XmlValueKind::Atomic(XmlAtomicValue::String(normalized.into_owned())),
        ))
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        facets.validate_string(&result.to_string_value())?;
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "length" | "minLength" | "maxLength" | "pattern" | "enumeration" | "whitespace"
        )
    }
}

/// Validator for xs:IDREF
pub struct IdRefValidator;

impl TypeValidator for IdRefValidator {
    fn type_name(&self) -> &'static str {
        "IDREF"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::IdRef
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::String
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        if !is_valid_ncname(&normalized) {
            return Err(ValidationError::InvalidLexical {
                value: value.to_string(),
                type_name: "IDREF",
                message: "Invalid IDREF (must be NCName)".to_string(),
            });
        }
        Ok(XmlValue::new(
            XmlTypeCode::IdRef,
            XmlValueKind::Atomic(XmlAtomicValue::String(normalized.into_owned())),
        ))
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        facets.validate_string(&result.to_string_value())?;
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "length" | "minLength" | "maxLength" | "pattern" | "enumeration" | "whitespace"
        )
    }
}

/// Validator for xs:ENTITY
pub struct EntityValidator;

impl TypeValidator for EntityValidator {
    fn type_name(&self) -> &'static str {
        "ENTITY"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::Entity
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::String
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        if !is_valid_ncname(&normalized) {
            return Err(ValidationError::InvalidLexical {
                value: value.to_string(),
                type_name: "ENTITY",
                message: "Invalid ENTITY (must be NCName)".to_string(),
            });
        }
        Ok(XmlValue::new(
            XmlTypeCode::Entity,
            XmlValueKind::Atomic(XmlAtomicValue::String(normalized.into_owned())),
        ))
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        facets.validate_string(&result.to_string_value())?;
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "length" | "minLength" | "maxLength" | "pattern" | "enumeration" | "whitespace"
        )
    }
}

// ============================================================================
// Boolean Validator
// ============================================================================

/// Validator for xs:boolean
pub struct BooleanValidator;

impl TypeValidator for BooleanValidator {
    fn type_name(&self) -> &'static str {
        "boolean"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::Boolean
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::Boolean
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        match normalized.as_ref() {
            "true" | "1" => Ok(XmlValue::boolean(true)),
            "false" | "0" => Ok(XmlValue::boolean(false)),
            _ => Err(ValidationError::InvalidLexical {
                value: value.to_string(),
                type_name: "boolean",
                message: "Expected 'true', 'false', '1', or '0'".to_string(),
            }),
        }
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        // §cvc-pattern-valid: pattern applies to lexical representation, not canonical form.
        // "1" must match pattern "[1]{1}" even though canonical form is "true".
        facets.validate_patterns_only(value)?;
        // §cvc-enumeration-valid: compare in value space ("1" == "true" == true)
        if let XmlValueKind::Atomic(XmlAtomicValue::Boolean(b)) = result.value {
            facets.validate_enum_value_space(
                |s| match normalize_whitespace(s, WhitespaceMode::Collapse).as_ref() {
                    "true" | "1" => b,
                    "false" | "0" => !b,
                    _ => false,
                },
                value,
            )?;
        }
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(facet, "pattern" | "enumeration")
    }
}

// ============================================================================
// Numeric Validators
// ============================================================================

/// Validate strict XSD lexical form for `xs:decimal`.
///
/// Datatypes 1.1 §3.3.3 (`decimalLexicalRep ::= decimalPtNumeral |
/// noDecimalPtNumeral`, productions \[45\]–\[51\]) admits only
/// `('+'|'-')? (digits ('.' digits?)? | '.' digits)` — no exponent and no
/// `INF`/`NaN` (those enter the grammar at `scientificNotationNumeral`,
/// which decimal's lexical mapping never references). `rust_decimal`'s
/// `from_str` accepts additional forms (scientific notation as of 1.42,
/// underscore separators) — we pre-validate to reject them so conformance
/// does not track that crate's parser behavior.
pub fn is_valid_xsd_decimal_lexical(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let mut i = 0;
    if bytes[i] == b'+' || bytes[i] == b'-' {
        i += 1;
    }
    let mut has_int_digits = false;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        has_int_digits = true;
        i += 1;
    }
    let mut has_frac_digits = false;
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            has_frac_digits = true;
            i += 1;
        }
    }
    if !has_int_digits && !has_frac_digits {
        return false;
    }
    i == bytes.len()
}

/// The typed value of an `xs:decimal` lexical form (`raw` before, `normalized`
/// after whitespace collapse) — see [`DecimalValidator::validate`].
fn decimal_value(raw: &str, normalized: &str) -> ValidationResult<XmlValue> {
    if !is_valid_xsd_decimal_lexical(normalized) {
        return Err(ValidationError::InvalidLexical {
            value: raw.to_string(),
            type_name: "decimal",
            message: format!("Invalid xs:decimal lexical form: '{}'", normalized),
        });
    }
    Ok(super::exact_decimal::decimal_typed_value(normalized))
}

/// Validator for xs:decimal
///
/// Values are validated exactly at any size and precision. Exactly
/// representable values — those `rust_decimal::Decimal` holds: `m / 10^s` with
/// an integer `|m| ≤ 2^96 − 1` (79228162514264337593543950335, a 96-bit
/// mantissa) and `0 ≤ s ≤ 28` — are typed as [`XmlAtomicValue::Decimal`];
/// larger or more precise values are carried as their canonical string (see
/// [`DecimalValidator::validate`]).
pub struct DecimalValidator;

impl TypeValidator for DecimalValidator {
    fn type_name(&self) -> &'static str {
        "decimal"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::Decimal
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::Decimal
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    /// Parse an `xs:decimal` lexical form into its typed value.
    ///
    /// The value space of `xs:decimal` is unbounded (Datatypes 1.1 §3.3.3).
    /// A value that `rust_decimal::Decimal` holds exactly — `m / 10^s` with an
    /// integer `|m| ≤ 2^96 − 1 = 79228162514264337593543950335` and
    /// `0 ≤ s ≤ 28`, trailing fraction zeros dropped — is an
    /// [`XmlAtomicValue::Decimal`], never rounded. Any other valid value is
    /// still valid: its typed value is its canonical representation
    /// (Datatypes §3.3.3.2, e.g. `1000…0.5` for `+01000…0.500`) as
    /// [`XmlAtomicValue::String`] with the [`XmlTypeCode::Decimal`] type
    /// code (so [`XmlValue::as_decimal`] returns `None` for it). Facets are
    /// checked on the exact value either way, and the instance validator
    /// reports such a value as a warning with code `xs-decimal-capacity`
    /// (Datatypes 1.1 §5.4 Partial Implementation of Infinite Datatypes).
    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        decimal_value(value, &normalized)
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        let result = decimal_value(value, &normalized)?;
        facets.validate_decimal_family(&result, &normalized)?;
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "totalDigits"
                | "fractionDigits"
                | "minInclusive"
                | "maxInclusive"
                | "minExclusive"
                | "maxExclusive"
                | "pattern"
                | "enumeration"
        )
    }
}

/// Validator for xs:integer
pub struct IntegerValidator;

impl TypeValidator for IntegerValidator {
    fn type_name(&self) -> &'static str {
        "integer"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::Integer
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::Decimal
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        normalized
            .parse::<BigInt>()
            .map(XmlValue::integer)
            .map_err(|e| ValidationError::InvalidLexical {
                value: value.to_string(),
                type_name: "integer",
                message: e.to_string(),
            })
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        facets.validate_decimal_family(&result, &normalized)?;
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "totalDigits"
                | "fractionDigits"
                | "minInclusive"
                | "maxInclusive"
                | "minExclusive"
                | "maxExclusive"
                | "pattern"
                | "enumeration"
        )
    }
}

/// Validator for xs:float
pub struct FloatValidator;

impl TypeValidator for FloatValidator {
    fn type_name(&self) -> &'static str {
        "float"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::Float
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::Float
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        parse_float(&normalized)
            .map(XmlValue::float)
            .map_err(|msg| ValidationError::InvalidLexical {
                value: value.to_string(),
                type_name: "float",
                message: msg,
            })
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        if let XmlValueKind::Atomic(XmlAtomicValue::Float(f)) = result.value {
            facets.validate_float(f)?;
            // §cvc-enumeration-valid: compare in value space, not lexically
            facets.validate_enum_value_space(
                enum_matches_value_space!(f, parse_float, float_eq),
                value,
            )?;
        }
        facets.validate_patterns_only(value)?;
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "minInclusive"
                | "maxInclusive"
                | "minExclusive"
                | "maxExclusive"
                | "pattern"
                | "enumeration"
        )
    }
}

/// Validator for xs:double
pub struct DoubleValidator;

impl TypeValidator for DoubleValidator {
    fn type_name(&self) -> &'static str {
        "double"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::Double
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::Double
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        parse_double(&normalized)
            .map(XmlValue::double)
            .map_err(|msg| ValidationError::InvalidLexical {
                value: value.to_string(),
                type_name: "double",
                message: msg,
            })
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        if let XmlValueKind::Atomic(XmlAtomicValue::Double(d)) = result.value {
            facets.validate_double(d)?;
            // §cvc-enumeration-valid: compare in value space, not lexically
            facets.validate_enum_value_space(
                enum_matches_value_space!(d, parse_double, double_eq),
                value,
            )?;
        }
        facets.validate_patterns_only(value)?;
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "minInclusive"
                | "maxInclusive"
                | "minExclusive"
                | "maxExclusive"
                | "pattern"
                | "enumeration"
        )
    }
}

/// Validate strict XSD lexical form for `xs:float` / `xs:double`.
///
/// Datatypes 1.1 §3.3.16 / §3.3.17 enumerate the legal special values as
/// `INF`, `+INF`, `-INF`, and `NaN` (case-sensitive). XSD 1.0 omits the
/// `+INF` form (Datatypes 1.0 §3.2.4 / §3.2.5); the version-specific
/// rejection of `+INF` is handled post-validation in `validate_atomic_type`.
/// Anything else must match the numeric production
/// `('+'|'-')? (digits ('.' digits?)? | '.' digits) ([eE] ('+'|'-')? digits)?`.
/// Rust's `f32::from_str` / `f64::from_str` accept additional forms
/// (`inf`, `nan`, `infinity`, `+NaN`, etc.) — we pre-validate to reject them.
pub fn is_valid_xsd_float_lexical(s: &str) -> bool {
    if s == "INF" || s == "-INF" || s == "+INF" || s == "NaN" {
        return true;
    }
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let mut i = 0;
    if bytes[i] == b'+' || bytes[i] == b'-' {
        i += 1;
    }
    let mut has_int_digits = false;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        has_int_digits = true;
        i += 1;
    }
    let mut has_frac_digits = false;
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            has_frac_digits = true;
            i += 1;
        }
    }
    if !has_int_digits && !has_frac_digits {
        return false;
    }
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        i += 1;
        if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
            i += 1;
        }
        let mut has_exp_digits = false;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            has_exp_digits = true;
            i += 1;
        }
        if !has_exp_digits {
            return false;
        }
    }
    i == bytes.len()
}

/// Parse XSD float with special value handling
pub(crate) fn parse_float(s: &str) -> Result<f32, String> {
    if !is_valid_xsd_float_lexical(s) {
        return Err(format!("Invalid xs:float lexical form: '{}'", s));
    }
    match s {
        "INF" | "+INF" => Ok(f32::INFINITY),
        "-INF" => Ok(f32::NEG_INFINITY),
        "NaN" => Ok(f32::NAN),
        _ => s
            .parse()
            .map_err(|e: std::num::ParseFloatError| e.to_string()),
    }
}

/// Parse XSD double with special value handling
pub(crate) fn parse_double(s: &str) -> Result<f64, String> {
    if !is_valid_xsd_float_lexical(s) {
        return Err(format!("Invalid xs:double lexical form: '{}'", s));
    }
    match s {
        "INF" | "+INF" => Ok(f64::INFINITY),
        "-INF" => Ok(f64::NEG_INFINITY),
        "NaN" => Ok(f64::NAN),
        _ => s
            .parse()
            .map_err(|e: std::num::ParseFloatError| e.to_string()),
    }
}

use crate::types::equality::{double_eq, float_eq};

/// Value-space equality of two `xs:date` values for `enumeration`: equal in
/// the order the facets use ([`FacetOrder`]), so one instant written with two
/// time zone offsets is one value, and a value without a time zone offset
/// equals no value with one. The datatypes with seconds compare their
/// literals ([`enum_matches_literal!`]): `xs:duration` values are equal when
/// the four reference sums agree — the same signed months and seconds, and
/// also pairs such as `P400Y` / `P146097D` (decision 11).
fn date_value_eq(a: &DateValue, b: &DateValue) -> bool {
    a.facet_cmp(b) == Some(std::cmp::Ordering::Equal)
}

/// Value-space equality of two values of a g* datatype for `enumeration`, in
/// the same order (Datatypes 1.1 §3.3.13.1: "---15−11:00 = ---16+13:00").
fn g_value_eq<T: FacetOrder>(a: &T, b: &T) -> bool {
    a.facet_cmp(b) == Some(std::cmp::Ordering::Equal)
}

fn year_month_duration_value_eq(a: &YearMonthDurationValue, b: &YearMonthDurationValue) -> bool {
    a.partial_cmp(b) == Some(std::cmp::Ordering::Equal)
}

// ---------------------------------------------------------------------------
// Date/time bound checking helper
// ---------------------------------------------------------------------------

/// Validate the min/max inclusive/exclusive bounds in the datatype's
/// [`FacetOrder`]; a value ·incomparable· with a bound violates it.
/// The `parse_fn` parses the bound value string using the same parser as the main value.
fn validate_bounds<T: FacetOrder, F: Fn(&str) -> Option<T>>(
    value: &T,
    facets: &FacetSet,
    _type_name: &str,
    value_str: &str,
    parse_fn: F,
) -> ValidationResult<()> {
    if let Some(ref min) = facets.min_inclusive {
        if let Some(bound) = parse_fn(&min.value) {
            if !matches!(
                value.facet_cmp_literal(value_str, &bound, &min.value),
                Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
            ) {
                return Err(ValidationError::FacetViolation(
                    FacetError::MinInclusiveViolation {
                        value: value_str.to_string(),
                        min: min.value.clone(),
                    },
                ));
            }
        }
    }
    if let Some(ref max) = facets.max_inclusive {
        if let Some(bound) = parse_fn(&max.value) {
            if !matches!(
                value.facet_cmp_literal(value_str, &bound, &max.value),
                Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
            ) {
                return Err(ValidationError::FacetViolation(
                    FacetError::MaxInclusiveViolation {
                        value: value_str.to_string(),
                        max: max.value.clone(),
                    },
                ));
            }
        }
    }
    if let Some(ref min) = facets.min_exclusive {
        if let Some(bound) = parse_fn(&min.value) {
            if value.facet_cmp_literal(value_str, &bound, &min.value)
                != Some(std::cmp::Ordering::Greater)
            {
                return Err(ValidationError::FacetViolation(
                    FacetError::MinExclusiveViolation {
                        value: value_str.to_string(),
                        min: min.value.clone(),
                    },
                ));
            }
        }
    }
    if let Some(ref max) = facets.max_exclusive {
        if let Some(bound) = parse_fn(&max.value) {
            if value.facet_cmp_literal(value_str, &bound, &max.value)
                != Some(std::cmp::Ordering::Less)
            {
                return Err(ValidationError::FacetViolation(
                    FacetError::MaxExclusiveViolation {
                        value: value_str.to_string(),
                        max: max.value.clone(),
                    },
                ));
            }
        }
    }
    Ok(())
}

// Date/Time Validators
// ============================================================================

/// Validator for xs:duration
pub struct DurationValidator;

impl TypeValidator for DurationValidator {
    fn type_name(&self) -> &'static str {
        "duration"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::Duration
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::Duration
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        parse_duration(&normalized).map(|d| {
            XmlValue::new(
                XmlTypeCode::Duration,
                XmlValueKind::Atomic(XmlAtomicValue::Duration(d)),
            )
        })
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        if let XmlValueKind::Atomic(XmlAtomicValue::Duration(ref dur)) = result.value {
            validate_bounds(dur, facets, "duration", value, |s| {
                parse_duration(&normalize_whitespace(s, WhitespaceMode::Collapse)).ok()
            })?;
            // §cvc-enumeration-valid: compare in value space, not lexically
            facets.validate_enum_value_space(
                enum_matches_literal!(dur, value, parse_duration),
                value,
            )?;
        }
        facets.validate_patterns_only(value)?;
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "minInclusive"
                | "maxInclusive"
                | "minExclusive"
                | "maxExclusive"
                | "pattern"
                | "enumeration"
        )
    }
}

/// Validator for xs:dateTime
pub struct DateTimeValidator;

impl TypeValidator for DateTimeValidator {
    fn type_name(&self) -> &'static str {
        "dateTime"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::DateTime
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::DateTime
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        parse_datetime(&normalized).map(|dt| {
            XmlValue::new(
                XmlTypeCode::DateTime,
                XmlValueKind::Atomic(XmlAtomicValue::DateTime(dt)),
            )
        })
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        if let XmlValueKind::Atomic(XmlAtomicValue::DateTime(ref dt)) = result.value {
            facets.validate_explicit_timezone(dt.timezone.is_some())?;
            validate_bounds(dt, facets, "dateTime", value, |s| {
                parse_datetime(&normalize_whitespace(s, WhitespaceMode::Collapse)).ok()
            })?;
            facets.validate_enum_value_space(
                enum_matches_literal!(dt, value, parse_datetime),
                value,
            )?;
        }
        facets.validate_patterns_only(value)?;
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "minInclusive"
                | "maxInclusive"
                | "minExclusive"
                | "maxExclusive"
                | "pattern"
                | "enumeration"
                | "explicitTimezone"
        )
    }
}

/// Validator for xs:date
pub struct DateValidator;

impl TypeValidator for DateValidator {
    fn type_name(&self) -> &'static str {
        "date"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::Date
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::Date
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        parse_date(&normalized).map(|d| {
            XmlValue::new(
                XmlTypeCode::Date,
                XmlValueKind::Atomic(XmlAtomicValue::Date(d)),
            )
        })
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        if let XmlValueKind::Atomic(XmlAtomicValue::Date(ref d)) = result.value {
            facets.validate_explicit_timezone(d.timezone.is_some())?;
            validate_bounds(d, facets, "date", value, |s| {
                parse_date(&normalize_whitespace(s, WhitespaceMode::Collapse)).ok()
            })?;
            facets.validate_enum_value_space(
                enum_matches_value_space!(d, parse_date, |a, b| date_value_eq(a, &b)),
                value,
            )?;
        }
        facets.validate_patterns_only(value)?;
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "minInclusive"
                | "maxInclusive"
                | "minExclusive"
                | "maxExclusive"
                | "pattern"
                | "enumeration"
                | "explicitTimezone"
        )
    }
}

/// Validator for xs:time
pub struct TimeValidator;

impl TypeValidator for TimeValidator {
    fn type_name(&self) -> &'static str {
        "time"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::Time
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::Time
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        parse_time(&normalized).map(|t| {
            XmlValue::new(
                XmlTypeCode::Time,
                XmlValueKind::Atomic(XmlAtomicValue::Time(t)),
            )
        })
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        if let XmlValueKind::Atomic(XmlAtomicValue::Time(ref t)) = result.value {
            facets.validate_explicit_timezone(t.timezone.is_some())?;
            validate_bounds(t, facets, "time", value, |s| {
                parse_time(&normalize_whitespace(s, WhitespaceMode::Collapse)).ok()
            })?;
            facets.validate_enum_value_space(enum_matches_literal!(t, value, parse_time), value)?;
        }
        facets.validate_patterns_only(value)?;
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "minInclusive"
                | "maxInclusive"
                | "minExclusive"
                | "maxExclusive"
                | "pattern"
                | "enumeration"
                | "explicitTimezone"
        )
    }
}

/// Validator for xs:gYearMonth
pub struct GYearMonthValidator;

impl TypeValidator for GYearMonthValidator {
    fn type_name(&self) -> &'static str {
        "gYearMonth"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::GYearMonth
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::GYearMonth
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        parse_gyearmonth(&normalized).map(|v| {
            XmlValue::new(
                XmlTypeCode::GYearMonth,
                XmlValueKind::Atomic(XmlAtomicValue::GYearMonth(v)),
            )
        })
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        let canonical = result.to_string_value();
        if let XmlValueKind::Atomic(XmlAtomicValue::GYearMonth(ref v)) = result.value {
            facets.validate_explicit_timezone(v.timezone.is_some())?;
            validate_bounds(v, facets, "gYearMonth", value, |s| {
                parse_gyearmonth(&normalize_whitespace(s, WhitespaceMode::Collapse)).ok()
            })?;
            // Datatypes §4.3.4: a pattern constrains the lexical space —
            // the normalized value, not the canonical representation.
            facets
                .validate_patterns_only(&normalize_whitespace(value, WhitespaceMode::Collapse))?;
            facets.validate_enum_value_space(
                enum_matches_value_space!(v, parse_gyearmonth, |a, b| g_value_eq(a, &b)),
                &canonical,
            )?;
        }
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "minInclusive"
                | "maxInclusive"
                | "minExclusive"
                | "maxExclusive"
                | "pattern"
                | "enumeration"
                | "explicitTimezone"
        )
    }
}

/// Validator for xs:gYear
pub struct GYearValidator;

impl TypeValidator for GYearValidator {
    fn type_name(&self) -> &'static str {
        "gYear"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::GYear
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::GYear
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        parse_gyear(&normalized).map(|v| {
            XmlValue::new(
                XmlTypeCode::GYear,
                XmlValueKind::Atomic(XmlAtomicValue::GYear(v)),
            )
        })
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        let canonical = result.to_string_value();
        if let XmlValueKind::Atomic(XmlAtomicValue::GYear(ref v)) = result.value {
            facets.validate_explicit_timezone(v.timezone.is_some())?;
            validate_bounds(v, facets, "gYear", value, |s| {
                parse_gyear(&normalize_whitespace(s, WhitespaceMode::Collapse)).ok()
            })?;
            // Datatypes §4.3.4: a pattern constrains the lexical space —
            // the normalized value, not the canonical representation.
            facets
                .validate_patterns_only(&normalize_whitespace(value, WhitespaceMode::Collapse))?;
            facets.validate_enum_value_space(
                enum_matches_value_space!(v, parse_gyear, |a, b| g_value_eq(a, &b)),
                &canonical,
            )?;
        }
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "minInclusive"
                | "maxInclusive"
                | "minExclusive"
                | "maxExclusive"
                | "pattern"
                | "enumeration"
                | "explicitTimezone"
        )
    }
}

/// Validator for xs:gMonthDay
pub struct GMonthDayValidator;

impl TypeValidator for GMonthDayValidator {
    fn type_name(&self) -> &'static str {
        "gMonthDay"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::GMonthDay
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::GMonthDay
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        parse_gmonthday(&normalized).map(|v| {
            XmlValue::new(
                XmlTypeCode::GMonthDay,
                XmlValueKind::Atomic(XmlAtomicValue::GMonthDay(v)),
            )
        })
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        let canonical = result.to_string_value();
        if let XmlValueKind::Atomic(XmlAtomicValue::GMonthDay(ref v)) = result.value {
            facets.validate_explicit_timezone(v.timezone.is_some())?;
            validate_bounds(v, facets, "gMonthDay", value, |s| {
                parse_gmonthday(&normalize_whitespace(s, WhitespaceMode::Collapse)).ok()
            })?;
            // Datatypes §4.3.4: a pattern constrains the lexical space —
            // the normalized value, not the canonical representation.
            facets
                .validate_patterns_only(&normalize_whitespace(value, WhitespaceMode::Collapse))?;
            facets.validate_enum_value_space(
                enum_matches_value_space!(v, parse_gmonthday, |a, b| g_value_eq(a, &b)),
                &canonical,
            )?;
        }
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "minInclusive"
                | "maxInclusive"
                | "minExclusive"
                | "maxExclusive"
                | "pattern"
                | "enumeration"
                | "explicitTimezone"
        )
    }
}

/// Validator for xs:gDay
pub struct GDayValidator;

impl TypeValidator for GDayValidator {
    fn type_name(&self) -> &'static str {
        "gDay"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::GDay
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::GDay
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        parse_gday(&normalized).map(|v| {
            XmlValue::new(
                XmlTypeCode::GDay,
                XmlValueKind::Atomic(XmlAtomicValue::GDay(v)),
            )
        })
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        let canonical = result.to_string_value();
        if let XmlValueKind::Atomic(XmlAtomicValue::GDay(ref v)) = result.value {
            facets.validate_explicit_timezone(v.timezone.is_some())?;
            validate_bounds(v, facets, "gDay", value, |s| {
                parse_gday(&normalize_whitespace(s, WhitespaceMode::Collapse)).ok()
            })?;
            // Datatypes §4.3.4: a pattern constrains the lexical space —
            // the normalized value, not the canonical representation.
            facets
                .validate_patterns_only(&normalize_whitespace(value, WhitespaceMode::Collapse))?;
            facets.validate_enum_value_space(
                enum_matches_value_space!(v, parse_gday, |a, b| g_value_eq(a, &b)),
                &canonical,
            )?;
        }
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "minInclusive"
                | "maxInclusive"
                | "minExclusive"
                | "maxExclusive"
                | "pattern"
                | "enumeration"
                | "explicitTimezone"
        )
    }
}

/// Validator for xs:gMonth
pub struct GMonthValidator;

impl TypeValidator for GMonthValidator {
    fn type_name(&self) -> &'static str {
        "gMonth"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::GMonth
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::GMonth
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        parse_gmonth(&normalized).map(|v| {
            XmlValue::new(
                XmlTypeCode::GMonth,
                XmlValueKind::Atomic(XmlAtomicValue::GMonth(v)),
            )
        })
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        let canonical = result.to_string_value();
        if let XmlValueKind::Atomic(XmlAtomicValue::GMonth(ref v)) = result.value {
            facets.validate_explicit_timezone(v.timezone.is_some())?;
            validate_bounds(v, facets, "gMonth", value, |s| {
                parse_gmonth(&normalize_whitespace(s, WhitespaceMode::Collapse)).ok()
            })?;
            // Datatypes §4.3.4: a pattern constrains the lexical space —
            // the normalized value, not the canonical representation.
            facets
                .validate_patterns_only(&normalize_whitespace(value, WhitespaceMode::Collapse))?;
            facets.validate_enum_value_space(
                enum_matches_value_space!(v, parse_gmonth, |a, b| g_value_eq(a, &b)),
                &canonical,
            )?;
        }
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "minInclusive"
                | "maxInclusive"
                | "minExclusive"
                | "maxExclusive"
                | "pattern"
                | "enumeration"
                | "explicitTimezone"
        )
    }
}

// ============================================================================
// Binary Validators
// ============================================================================

/// Validator for xs:hexBinary
pub struct HexBinaryValidator;

impl TypeValidator for HexBinaryValidator {
    fn type_name(&self) -> &'static str {
        "hexBinary"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::HexBinary
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::HexBinary
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        hex::decode(normalized.as_bytes())
            .map(|bytes| {
                XmlValue::new(
                    XmlTypeCode::HexBinary,
                    XmlValueKind::Atomic(XmlAtomicValue::HexBinary(bytes)),
                )
            })
            .map_err(|e| ValidationError::InvalidLexical {
                value: value.to_string(),
                type_name: "hexBinary",
                message: e.to_string(),
            })
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        if let XmlValueKind::Atomic(XmlAtomicValue::HexBinary(bytes)) = &result.value {
            facets.validate_binary_length(bytes.len() as u64)?;
            facets.validate_patterns_only(&normalized)?;
            // §cvc-enumeration-valid: a value is a sequence of octets
            // (Datatypes §3.3.15.1), so `0A` is the value of `0a`.
            facets.validate_enum_value_space(
                |s| {
                    hex::decode(normalize_whitespace(s, WhitespaceMode::Collapse).as_bytes())
                        .is_ok_and(|octets| &octets == bytes)
                },
                &normalized,
            )?;
        }
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "length" | "minLength" | "maxLength" | "pattern" | "enumeration"
        )
    }
}

/// Validator for xs:base64Binary
pub struct Base64BinaryValidator;

impl TypeValidator for Base64BinaryValidator {
    fn type_name(&self) -> &'static str {
        "base64Binary"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::Base64Binary
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::Base64Binary
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        use base64::Engine;
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        base64::engine::general_purpose::STANDARD
            .decode(normalized.as_bytes())
            .map(|bytes| {
                XmlValue::new(
                    XmlTypeCode::Base64Binary,
                    XmlValueKind::Atomic(XmlAtomicValue::Base64Binary(bytes)),
                )
            })
            .map_err(|e| ValidationError::InvalidLexical {
                value: value.to_string(),
                type_name: "base64Binary",
                message: e.to_string(),
            })
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        if let XmlValueKind::Atomic(XmlAtomicValue::Base64Binary(bytes)) = &result.value {
            facets.validate_binary_length(bytes.len() as u64)?;
            facets.validate_patterns_only(&normalized)?;
            // §cvc-enumeration-valid: a value is a sequence of octets
            // (Datatypes §3.3.16.1), compared as such.
            facets.validate_enum_value_space(
                |s| {
                    self.validate(s).is_ok_and(|literal| {
                        matches!(&literal.value,
                            XmlValueKind::Atomic(XmlAtomicValue::Base64Binary(octets)) if octets == bytes)
                    })
                },
                &normalized,
            )?;
        }
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "length" | "minLength" | "maxLength" | "pattern" | "enumeration"
        )
    }
}

// ============================================================================
// URI Validator
// ============================================================================

/// Validator for xs:anyURI
pub struct AnyUriValidator;

impl TypeValidator for AnyUriValidator {
    fn type_name(&self) -> &'static str {
        "anyURI"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::AnyUri
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::AnyUri
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        // XSD anyURI is very permissive - just check for illegal characters
        // (actual URI validation is application-specific)
        Ok(XmlValue::new(
            XmlTypeCode::AnyUri,
            XmlValueKind::Atomic(XmlAtomicValue::AnyUri(normalized.into_owned())),
        ))
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        facets.validate_string(&result.to_string_value())?;
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "length" | "minLength" | "maxLength" | "pattern" | "enumeration"
        )
    }
}

/// Check if a string is a valid URI scheme per RFC 2396 §3.1 / RFC 3986
/// (`alpha *( alpha | digit | "+" | "-" | "." )`). Empty is invalid.
pub fn is_valid_uri_scheme(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
}

/// Strict XSD 1.0 lexical check for an `xs:anyURI` value.
///
/// XSD 1.0 §3.2.17 ties `xs:anyURI` to RFC 2396, which forbids a URI
/// reference whose first colon is preceded by anything other than a valid
/// scheme name. The W3C `anyURI_a001_1336` fixture relies on exactly this
/// check: the schema contains `appinfo/@source="9999...anyURI:"` and
/// `documentation/@source="1111...http://foo/bar"`, both of which place a
/// digit in the (would-be) scheme position. XSD 1.1 explicitly relaxed
/// the rule, so callers must gate this on `XsdVersion::V1_0`.
///
/// The check is intentionally narrow — it only catches the malformed-scheme
/// shape — to avoid false positives on the many sloppy-but-spec-legal URIs
/// in the existing conformance corpus (numeric relative segments like `"0"`
/// or `"123"` remain accepted as valid relative-path references).
///
/// `xs:anyURI` declares `whiteSpace = collapse`. The common case of an
/// annotation `source` value has no whitespace at all, so we skip the
/// allocation entirely unless the input actually contains whitespace.
pub fn is_strict_xsd10_anyuri(value: &str) -> bool {
    let collapsed: Cow<str>;
    let value: &str = if value.chars().any(char::is_whitespace) {
        collapsed = normalize_whitespace(value, WhitespaceMode::Collapse);
        &collapsed
    } else {
        value
    };

    if value.is_empty() {
        return true;
    }

    // Find the first character among ':' / '/' / '?' / '#'. Anything before
    // that delimiter is the candidate scheme; if the delimiter is ':' the
    // scheme syntax must be valid.
    let mut scheme_end: Option<usize> = None;
    for (i, c) in value.char_indices() {
        match c {
            ':' => {
                scheme_end = Some(i);
                break;
            }
            // Pure relative reference — scheme rule does not apply.
            '/' | '?' | '#' => return true,
            _ => {}
        }
    }
    // No colon at all — relative reference, always lexically OK.
    let Some(end) = scheme_end else { return true };
    is_valid_uri_scheme(&value[..end])
}

/// Extended XSD 1.0 lexical check for `xs:anyURI` used on enumeration facet
/// values (msData anyURI_a003/b004/b006). On top of the malformed-scheme
/// shape caught by [`is_strict_xsd10_anyuri`], RFC 2396 excludes the
/// backslash and caret characters entirely (§2.4.3 "unwise") and requires
/// every `%` to introduce a two-hex-digit escape (§2.4.1).
///
/// Deliberately NOT applied to instance validation — only to schema-side
/// facet values — to keep the long-standing permissive instance behavior
/// (the corpus contains many sloppy-but-accepted instance URIs).
pub fn is_strict_xsd10_anyuri_enum_value(value: &str) -> bool {
    if !is_strict_xsd10_anyuri(value) {
        return false;
    }
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' | b'^' => return false,
            b'%' => {
                let ok = bytes.len() > i + 2
                    && bytes[i + 1].is_ascii_hexdigit()
                    && bytes[i + 2].is_ascii_hexdigit();
                if !ok {
                    return false;
                }
                i += 3;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    true
}

// ============================================================================
// Date/Time Parsing Helpers
// ============================================================================

/// Parse XSD duration
fn parse_duration(s: &str) -> ValidationResult<DurationValue> {
    // Lexical form per §3.3.6 (Datatypes):
    //   -?P([0-9]+Y)?([0-9]+M)?([0-9]+D)?(T([0-9]+H)?([0-9]+M)?([0-9]+(\.[0-9]+)?S)?)?
    // At least one designator after `P` is required; after `T` at least one
    // time designator is required. Fraction is only legal on the seconds field.
    let bad = |message: String| ValidationError::InvalidLexical {
        value: s.to_string(),
        type_name: "duration",
        message,
    };

    let bytes = s.as_bytes();
    let mut pos = 0usize;
    let negative = bytes.first() == Some(&b'-');
    if negative {
        pos += 1;
    }
    if bytes.get(pos) != Some(&b'P') {
        return Err(bad("Must start with 'P'".to_string()));
    }
    pos += 1;

    let mut out = DurationValue {
        negative,
        years: 0,
        months: 0,
        days: 0,
        hours: 0,
        minutes: 0,
        seconds: Decimal::ZERO,
    };
    // Duplicate-designator detection. Index order matches `Field` below.
    let mut seen = [false; 6];
    let mut in_time = false;

    while pos < bytes.len() {
        if bytes[pos] == b'T' {
            if in_time {
                return Err(bad("Duplicate 'T' separator".to_string()));
            }
            in_time = true;
            pos += 1;
            continue;
        }

        let int_start = pos;
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }
        if pos == int_start {
            return Err(bad(format!(
                "Unexpected character '{}' at position {}",
                bytes[pos] as char, pos
            )));
        }
        let int_end = pos;
        if pos < bytes.len() && bytes[pos] == b'.' {
            pos += 1;
            let frac_start = pos;
            while pos < bytes.len() && bytes[pos].is_ascii_digit() {
                pos += 1;
            }
            if pos == frac_start {
                return Err(bad(
                    "Fractional part must have at least one digit".to_string()
                ));
            }
        }
        let has_fraction = pos != int_end;
        if pos >= bytes.len() {
            return Err(bad("Missing designator after number".to_string()));
        }
        let designator = bytes[pos];
        pos += 1;
        let num_str = &s[int_start..pos - 1];

        // Resolve the designator to a slot; the letter 'M' is overloaded on `T`.
        let (field, name, allow_fraction): (Field, &str, bool) = match (designator, in_time) {
            (b'Y', false) => (Field::Years, "Y", false),
            (b'M', false) => (Field::Months, "M", false),
            (b'D', false) => (Field::Days, "D", false),
            (b'H', true) => (Field::Hours, "H", false),
            (b'M', true) => (Field::Minutes, "M", false),
            (b'S', true) => (Field::Seconds, "S", true),
            (other, _) => {
                return Err(bad(format!(
                    "Unexpected designator '{}' (in_time={})",
                    other as char, in_time
                )));
            }
        };
        if seen[field as usize] {
            return Err(bad(format!("Duplicate '{}' designator", name)));
        }
        seen[field as usize] = true;
        if has_fraction && !allow_fraction {
            return Err(bad(format!("'{}' must be an integer", name)));
        }

        match field {
            Field::Seconds => {
                out.seconds = parse_seconds(num_str)
                    .ok_or_else(|| bad(format!("Invalid seconds value '{}'", num_str)))?;
            }
            _ => {
                let v: u32 = num_str
                    .parse()
                    .map_err(|_| bad(format!("Invalid {} value '{}'", name, num_str)))?;
                match field {
                    Field::Years => out.years = v,
                    Field::Months => out.months = v,
                    Field::Days => out.days = v,
                    Field::Hours => out.hours = v,
                    Field::Minutes => out.minutes = v,
                    Field::Seconds => unreachable!(),
                }
            }
        }
    }

    if !seen.iter().any(|&b| b) {
        return Err(bad(
            "Duration must contain at least one component".to_string()
        ));
    }
    let any_time = seen[Field::Hours as usize]
        || seen[Field::Minutes as usize]
        || seen[Field::Seconds as usize];
    if in_time && !any_time {
        return Err(bad(
            "Time designator 'T' requires at least one time component".to_string(),
        ));
    }
    // Datatypes 1.1 §3.3.6.2: "a durationLexicalRep consists of one or more
    // of a duYearFrag, duMonthFrag, duDayFrag, duHourFrag, duMinuteFrag,
    // and/or duSecondFrag, in order".
    if !duration_fields_in_order(s) {
        return Err(bad(
            "the fields of a duration are in the order Y, M, D, T, H, M, S".to_string(),
        ));
    }

    Ok(out)
}

#[repr(u8)]
#[derive(Copy, Clone)]
enum Field {
    Years = 0,
    Months = 1,
    Days = 2,
    Hours = 3,
    Minutes = 4,
    Seconds = 5,
}

/// The warning code the validator reports, through
/// [`ValidationSink::on_warning`](crate::validation::ValidationSink::on_warning),
/// for a date/time or duration literal whose seconds have more digits than
/// this implementation holds ([`parse_seconds`]).
pub(crate) const SECONDS_CAPACITY_WARNING: &str = "xs-seconds-capacity";

/// A seconds number of a time of day or a duration (`ss`, `ss.s+`, a
/// duration's `n.nS`) as a `Decimal`: exactly when it fits — at most 28
/// fraction digits and a 96-bit mantissa; otherwise its fraction truncated,
/// toward zero, to the digits that fit, never rounded up past a whole second
/// (`59.99…9` stays below 60). Datatypes 1.1 §5.4: a value beyond the capacity
/// of a partial implementation "must not be treated as invalid, and the
/// unsupported value must not be quietly changed to a supported value": the
/// comparisons that see the literal — the bounds, `enumeration`, value
/// constraints, identity constraints, the schema-time facet checks — use its
/// fraction as written
/// (`value_order::literal_cmp_in`), and the runtime reports
/// [`SECONDS_CAPACITY_WARNING`] for an instance literal
/// ([`seconds_beyond_capacity`]), whose typed value holds the truncated
/// seconds. `None` for a malformed number, or one whose integral part alone
/// exceeds the mantissa.
fn parse_seconds(text: &str) -> Option<Decimal> {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if whole.is_empty() || !whole.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if !fraction.bytes().all(|b| b.is_ascii_digit()) || text.ends_with('.') {
        return None;
    }
    // Leading zeros carry nothing; at most 28 fraction digits can be kept.
    let whole = match whole.trim_start_matches('0') {
        "" => "0",
        digits => digits,
    };
    (0..=fraction.len().min(28)).rev().find_map(|keep| {
        let candidate = if keep == 0 {
            whole.to_string()
        } else {
            format!("{whole}.{}", &fraction[..keep])
        };
        Decimal::from_str_exact(&candidate).ok()
    })
}

/// Whether a literal holds a number with a fraction that [`parse_seconds`]
/// could not keep exactly — the case for [`SECONDS_CAPACITY_WARNING`], and
/// for comparing the literal's fraction as written.
pub(crate) fn seconds_beyond_capacity(lexical: &str) -> bool {
    let bytes = lexical.as_bytes();
    bytes.iter().enumerate().any(|(dot, &b)| {
        if b != b'.' {
            return false;
        }
        let start = (0..dot)
            .rev()
            .take_while(|&i| bytes[i].is_ascii_digit())
            .last()
            .unwrap_or(dot);
        let end = (dot + 1..bytes.len())
            .take_while(|&i| bytes[i].is_ascii_digit())
            .last()
            .map_or(dot + 1, |i| i + 1);
        // Leading zeros of the whole part and trailing zeros of the fraction
        // carry nothing: `001.5000…0` is exact.
        let whole = lexical[start..dot].trim_start_matches('0');
        let fraction = lexical[dot + 1..end].trim_end_matches('0');
        if whole.len() + fraction.len() <= 28 {
            // Fewer than 29 digits always fit a 96-bit mantissa.
            return false;
        }
        fraction.len() > 28
            || Decimal::from_str_exact(&format!(
                "{}.{fraction}",
                if whole.is_empty() { "0" } else { whole }
            ))
            .is_err()
    })
}

/// Parse XSD dateTime
fn parse_datetime(s: &str) -> ValidationResult<DateTimeValue> {
    // Format: YYYY-MM-DDTHH:MM:SS(.sss)?(Z|[+-]HH:MM)?
    let (date_time, tz) = split_timezone(s);
    let parts: Vec<&str> = date_time.split('T').collect();
    if parts.len() != 2 {
        return Err(ValidationError::InvalidLexical {
            value: s.to_string(),
            type_name: "dateTime",
            message: "Missing 'T' separator".to_string(),
        });
    }

    let date = parse_date_part(parts[0], "dateTime")?;
    let time = parse_time_part(parts[1], "dateTime")?;

    // Normalize 24:00:00 → 00:00:00 of the next day (XSD Part 2 §3.3.8).
    // The literal `24:00:00` is only valid with minute=0 and second=0.
    let (year, month, day, hour) = if time.0 == 24 {
        if time.1 != 0 || time.2 != Decimal::from(0) {
            return Err(ValidationError::InvalidLexical {
                value: s.to_string(),
                type_name: "dateTime",
                message: "24:00:00 requires minute=0 and second=0".to_string(),
            });
        }
        let (y, m, d) = add_one_day(date.0, date.1, date.2);
        (y, m, d, 0u8)
    } else {
        (date.0, date.1, date.2, time.0)
    };

    Ok(DateTimeValue {
        year,
        month,
        day,
        hour,
        minute: time.1,
        second: time.2,
        timezone: tz,
    })
}

/// Increment a date by one day, handling month/year boundaries.
fn add_one_day(year: i32, month: u8, day: u8) -> (i32, u8, u8) {
    let dim = days_in_month(year, month);
    if (day as u32) < dim {
        (year, month, day + 1)
    } else if month < 12 {
        (year, month + 1, 1)
    } else {
        // Year 0 is excluded in XSD 1.0 but valid in XSD 1.1; either way,
        // the next day after 12/31 of year Y is 1/1 of year Y+1.
        (year + 1, 1, 1)
    }
}

/// Days in a (Gregorian) month, accounting for leap years.
fn days_in_month(year: i32, month: u8) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
            if leap {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

/// Parse XSD date
fn parse_date(s: &str) -> ValidationResult<DateValue> {
    let (date_str, tz) = split_timezone(s);
    let (year, month, day) = parse_date_part(date_str, "date")?;
    Ok(DateValue {
        year,
        month,
        day,
        timezone: tz,
    })
}

/// Parse XSD time
fn parse_time(s: &str) -> ValidationResult<TimeValue> {
    let (time_str, tz) = split_timezone(s);
    let (mut hour, minute, second) = parse_time_part(time_str, "time")?;
    // Normalize 24:00:00 → 00:00:00 (XSD Part 2 §3.3.9).
    // The literal `24:00:00` is only valid with minute=0 and second=0.
    if hour == 24 {
        if minute != 0 || second != Decimal::from(0) {
            return Err(ValidationError::InvalidLexical {
                value: s.to_string(),
                type_name: "time",
                message: "24:00:00 requires minute=0 and second=0".to_string(),
            });
        }
        hour = 0;
    }
    Ok(TimeValue {
        hour,
        minute,
        second,
        timezone: tz,
    })
}

/// Validate the lexical form of a yearFrag per XSD Part 2 §3.3.9:
/// `yearFrag ::= '-'? (([1-9] digit digit digitRep) | ('0' digit digit digit))`.
/// In particular: at least four digits, and five-or-more-digit forms must
/// start with a non-zero leading digit.
fn valid_year_lexical(year_str: &str) -> bool {
    let digits = year_str.strip_prefix('-').unwrap_or(year_str);
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    if digits.len() < 4 {
        return false;
    }
    if digits.len() > 4 && digits.starts_with('0') {
        return false;
    }
    true
}

/// Parse a month or day field, which is exactly two ASCII digits per the
/// `monthFrag` / `dayFrag` productions of XSD Part 2 §D.2.2. Ranges are
/// checked by the callers. (`str::parse::<u8>` would also accept `5`, `005`
/// and `+5`.)
fn parse_two_digit_field(s: &str) -> Option<u8> {
    let [d1 @ b'0'..=b'9', d2 @ b'0'..=b'9'] = s.as_bytes() else {
        return None;
    };
    Some((d1 - b'0') * 10 + (d2 - b'0'))
}

/// Parse date part (YYYY-MM-DD)
fn parse_date_part(s: &str, type_name: &'static str) -> ValidationResult<(i32, u8, u8)> {
    let parts: Vec<&str> = s.split('-').collect();
    // A negative year splits into a leading empty segment; any further
    // segment (e.g. a timezone `split_timezone` did not accept) is an error.
    let (year_str, year, month, day) = if s.starts_with('-') && parts.len() == 4 {
        // Negative year
        let year_str = format!("-{}", parts[1]);
        let year: i32 = year_str
            .parse()
            .map_err(|_| ValidationError::InvalidLexical {
                value: s.to_string(),
                type_name,
                message: "Invalid year".to_string(),
            })?;
        let month =
            parse_two_digit_field(parts[2]).ok_or_else(|| ValidationError::InvalidLexical {
                value: s.to_string(),
                type_name,
                message: "Invalid month".to_string(),
            })?;
        let day =
            parse_two_digit_field(parts[3]).ok_or_else(|| ValidationError::InvalidLexical {
                value: s.to_string(),
                type_name,
                message: "Invalid day".to_string(),
            })?;
        (year_str, year, month, day)
    } else if parts.len() == 3 {
        let year_str = parts[0].to_string();
        let year: i32 = year_str
            .parse()
            .map_err(|_| ValidationError::InvalidLexical {
                value: s.to_string(),
                type_name,
                message: "Invalid year".to_string(),
            })?;
        let month =
            parse_two_digit_field(parts[1]).ok_or_else(|| ValidationError::InvalidLexical {
                value: s.to_string(),
                type_name,
                message: "Invalid month".to_string(),
            })?;
        let day =
            parse_two_digit_field(parts[2]).ok_or_else(|| ValidationError::InvalidLexical {
                value: s.to_string(),
                type_name,
                message: "Invalid day".to_string(),
            })?;
        (year_str, year, month, day)
    } else {
        return Err(ValidationError::InvalidLexical {
            value: s.to_string(),
            type_name,
            message: "Invalid date format".to_string(),
        });
    };

    if !valid_year_lexical(&year_str) {
        return Err(ValidationError::InvalidLexical {
            value: s.to_string(),
            type_name,
            message: "Invalid year: must be 4+ digits, no leading zeros except single '0YYY' form"
                .to_string(),
        });
    }

    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err(ValidationError::InvalidLexical {
            value: s.to_string(),
            type_name,
            message: "Date out of range".to_string(),
        });
    }

    // Reject days that don't exist in the given month (e.g. Feb 29 of a
    // non-leap year, Apr 31). Spec §3.3.9 / §3.3.10 require lexical
    // representations to denote dates that exist in the proleptic
    // Gregorian calendar.
    if (day as u32) > days_in_month(year, month) {
        return Err(ValidationError::InvalidLexical {
            value: s.to_string(),
            type_name,
            message: "Day does not exist in the given month".to_string(),
        });
    }

    Ok((year, month, day))
}

/// Parse time part (HH:MM:SS(.sss)?)
///
/// Per Datatypes Part 2 §3.3.8 / §3.3.9 the hour, minute, and integer-second
/// fields must each be exactly two ASCII digits. No leading sign or other
/// non-digit characters are permitted (Rust's `from_str_radix` accepts a
/// leading `+` even for unsigned types — we reject it explicitly here).
fn parse_time_part(s: &str, type_name: &'static str) -> ValidationResult<(u8, u8, Decimal)> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 3 {
        return Err(ValidationError::InvalidLexical {
            value: s.to_string(),
            type_name,
            message: "Invalid time format".to_string(),
        });
    }

    let is_two_ascii_digits = |t: &str| t.len() == 2 && t.bytes().all(|b| b.is_ascii_digit());
    if !is_two_ascii_digits(parts[0]) {
        return Err(ValidationError::InvalidLexical {
            value: s.to_string(),
            type_name,
            message: "Hour must be exactly two ASCII digits".to_string(),
        });
    }
    if !is_two_ascii_digits(parts[1]) {
        return Err(ValidationError::InvalidLexical {
            value: s.to_string(),
            type_name,
            message: "Minute must be exactly two ASCII digits".to_string(),
        });
    }
    let sec_str = parts[2];
    let (sec_int, sec_frac) = match sec_str.find('.') {
        Some(p) => (&sec_str[..p], Some(&sec_str[p + 1..])),
        None => (sec_str, None),
    };
    if !is_two_ascii_digits(sec_int) {
        return Err(ValidationError::InvalidLexical {
            value: s.to_string(),
            type_name,
            message: "Seconds integer part must be exactly two ASCII digits".to_string(),
        });
    }
    if let Some(frac) = sec_frac {
        if frac.is_empty() || !frac.bytes().all(|b| b.is_ascii_digit()) {
            return Err(ValidationError::InvalidLexical {
                value: s.to_string(),
                type_name,
                message: "Seconds fractional part must be one or more ASCII digits".to_string(),
            });
        }
    }

    let hour: u8 = parts[0]
        .parse()
        .map_err(|_| ValidationError::InvalidLexical {
            value: s.to_string(),
            type_name,
            message: "Invalid hour".to_string(),
        })?;
    let minute: u8 = parts[1]
        .parse()
        .map_err(|_| ValidationError::InvalidLexical {
            value: s.to_string(),
            type_name,
            message: "Invalid minute".to_string(),
        })?;
    let second: Decimal =
        parse_seconds(parts[2]).ok_or_else(|| ValidationError::InvalidLexical {
            value: s.to_string(),
            type_name,
            message: "Invalid second".to_string(),
        })?;

    if hour > 24 || minute > 59 || second >= Decimal::from(60) {
        return Err(ValidationError::InvalidLexical {
            value: s.to_string(),
            type_name,
            message: "Time out of range".to_string(),
        });
    }

    Ok((hour, minute, second))
}

/// Split timezone from date/time string
fn split_timezone(s: &str) -> (&str, Option<TimezoneOffset>) {
    if let Some(stripped) = s.strip_suffix('Z') {
        (stripped, Some(TimezoneOffset::UTC))
    } else if let Some(pos) = s.rfind('+') {
        if pos > 0 && pos < s.len() - 1 {
            let tz_str = &s[pos + 1..];
            if let Some(tz) = parse_timezone_offset(tz_str, false) {
                return (&s[..pos], Some(tz));
            }
        }
        (s, None)
    } else if let Some(pos) = s.rfind('-') {
        // Make sure it's a timezone, not part of date
        if pos > 0 && pos < s.len() - 1 {
            let tz_str = &s[pos + 1..];
            if let Some(tz) = parse_timezone_offset(tz_str, true) {
                return (&s[..pos], Some(tz));
            }
        }
        (s, None)
    } else {
        (s, None)
    }
}

/// Parse a timezone offset (HH:MM), bounded by 14:00.
fn parse_timezone_offset(s: &str, negative: bool) -> Option<TimezoneOffset> {
    let [h1 @ b'0'..=b'9', h2 @ b'0'..=b'9', b':', m1 @ b'0'..=b'9', m2 @ b'0'..=b'9'] =
        s.as_bytes()
    else {
        return None;
    };

    let hours = (h1 - b'0') * 10 + (h2 - b'0');
    let minutes = (m1 - b'0') * 10 + (m2 - b'0');

    if hours > 14 || minutes > 59 || (hours == 14 && minutes != 0) {
        return None;
    }

    let offset = i16::from(hours) * 60 + i16::from(minutes);
    Some(TimezoneOffset(if negative { -offset } else { offset }))
}

/// Parse gYearMonth
fn parse_gyearmonth(s: &str) -> ValidationResult<GYearMonthValue> {
    let (date_str, tz) = split_timezone(s);
    let parts: Vec<&str> = date_str.split('-').collect();

    // As in `parse_date_part`, a negative year must leave exactly three
    // segments; anything after the month is an error.
    let (year_str, year, month) = if date_str.starts_with('-') && parts.len() == 3 {
        let year_str = format!("-{}", parts[1]);
        let year: i32 = year_str
            .parse()
            .map_err(|_| ValidationError::InvalidLexical {
                value: s.to_string(),
                type_name: "gYearMonth",
                message: "Invalid year".to_string(),
            })?;
        let month =
            parse_two_digit_field(parts[2]).ok_or_else(|| ValidationError::InvalidLexical {
                value: s.to_string(),
                type_name: "gYearMonth",
                message: "Invalid month".to_string(),
            })?;
        (year_str, year, month)
    } else if parts.len() == 2 {
        let year_str = parts[0].to_string();
        let year: i32 = year_str
            .parse()
            .map_err(|_| ValidationError::InvalidLexical {
                value: s.to_string(),
                type_name: "gYearMonth",
                message: "Invalid year".to_string(),
            })?;
        let month =
            parse_two_digit_field(parts[1]).ok_or_else(|| ValidationError::InvalidLexical {
                value: s.to_string(),
                type_name: "gYearMonth",
                message: "Invalid month".to_string(),
            })?;
        (year_str, year, month)
    } else {
        return Err(ValidationError::InvalidLexical {
            value: s.to_string(),
            type_name: "gYearMonth",
            message: "Invalid format".to_string(),
        });
    };

    if !valid_year_lexical(&year_str) {
        return Err(ValidationError::InvalidLexical {
            value: s.to_string(),
            type_name: "gYearMonth",
            message: "Invalid year lexical form".to_string(),
        });
    }

    if !(1..=12).contains(&month) {
        return Err(ValidationError::InvalidLexical {
            value: s.to_string(),
            type_name: "gYearMonth",
            message: "Month out of range".to_string(),
        });
    }

    Ok(GYearMonthValue {
        year,
        month,
        timezone: tz,
    })
}

/// Parse gYear
fn parse_gyear(s: &str) -> ValidationResult<GYearValue> {
    let (year_str, tz) = split_timezone(s);
    let year: i32 = year_str
        .parse()
        .map_err(|_| ValidationError::InvalidLexical {
            value: s.to_string(),
            type_name: "gYear",
            message: "Invalid year".to_string(),
        })?;
    if !valid_year_lexical(year_str) {
        return Err(ValidationError::InvalidLexical {
            value: s.to_string(),
            type_name: "gYear",
            message: "Invalid year lexical form".to_string(),
        });
    }
    Ok(GYearValue { year, timezone: tz })
}

/// Parse gMonthDay (--MM-DD)
fn parse_gmonthday(s: &str) -> ValidationResult<GMonthDayValue> {
    let (date_str, tz) = split_timezone(s);
    if !date_str.starts_with("--") {
        return Err(ValidationError::InvalidLexical {
            value: s.to_string(),
            type_name: "gMonthDay",
            message: "Must start with '--'".to_string(),
        });
    }

    let rest = &date_str[2..];
    let parts: Vec<&str> = rest.split('-').collect();
    if parts.len() != 2 {
        return Err(ValidationError::InvalidLexical {
            value: s.to_string(),
            type_name: "gMonthDay",
            message: "Invalid format".to_string(),
        });
    }

    let month = parse_two_digit_field(parts[0]).ok_or_else(|| ValidationError::InvalidLexical {
        value: s.to_string(),
        type_name: "gMonthDay",
        message: "Invalid month".to_string(),
    })?;
    let day = parse_two_digit_field(parts[1]).ok_or_else(|| ValidationError::InvalidLexical {
        value: s.to_string(),
        type_name: "gMonthDay",
        message: "Invalid day".to_string(),
    })?;

    if !(1..=12).contains(&month) {
        return Err(ValidationError::InvalidLexical {
            value: s.to_string(),
            type_name: "gMonthDay",
            message: "Month out of range".to_string(),
        });
    }
    // Day must exist in the given month. Year is unspecified, so allow
    // Feb 29 (a leap year is possible) but still reject Feb 30/31, Apr 31, etc.
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => 29,
        _ => unreachable!(),
    };
    if !(1..=max_day).contains(&day) {
        return Err(ValidationError::InvalidLexical {
            value: s.to_string(),
            type_name: "gMonthDay",
            message: "Day does not exist in the given month".to_string(),
        });
    }

    Ok(GMonthDayValue {
        month,
        day,
        timezone: tz,
    })
}

/// Parse gDay (---DD)
fn parse_gday(s: &str) -> ValidationResult<GDayValue> {
    let (day_str, tz) = split_timezone(s);
    if !day_str.starts_with("---") {
        return Err(ValidationError::InvalidLexical {
            value: s.to_string(),
            type_name: "gDay",
            message: "Must start with '---'".to_string(),
        });
    }

    let day =
        parse_two_digit_field(&day_str[3..]).ok_or_else(|| ValidationError::InvalidLexical {
            value: s.to_string(),
            type_name: "gDay",
            message: "Invalid day".to_string(),
        })?;

    if !(1..=31).contains(&day) {
        return Err(ValidationError::InvalidLexical {
            value: s.to_string(),
            type_name: "gDay",
            message: "Day out of range".to_string(),
        });
    }

    Ok(GDayValue { day, timezone: tz })
}

/// Parse gMonth (--MM)
fn parse_gmonth(s: &str) -> ValidationResult<GMonthValue> {
    let (month_str, tz) = split_timezone(s);
    if !month_str.starts_with("--") {
        return Err(ValidationError::InvalidLexical {
            value: s.to_string(),
            type_name: "gMonth",
            message: "Must start with '--'".to_string(),
        });
    }

    let month =
        parse_two_digit_field(&month_str[2..]).ok_or_else(|| ValidationError::InvalidLexical {
            value: s.to_string(),
            type_name: "gMonth",
            message: "Invalid month".to_string(),
        })?;

    if !(1..=12).contains(&month) {
        return Err(ValidationError::InvalidLexical {
            value: s.to_string(),
            type_name: "gMonth",
            message: "Month out of range".to_string(),
        });
    }

    Ok(GMonthValue {
        month,
        timezone: tz,
    })
}

/// Whether the designators of a duration literal come in the order of
/// Datatypes 1.1 §3.3.6.2's grammar — `Y`, `M`, `D`, then after `T` `H`, `M`,
/// `S` — checked by [`parse_duration`] for `xs:duration` and its two
/// subtypes alike.
fn duration_fields_in_order(s: &str) -> bool {
    let mut last = 0u8;
    let mut in_time = false;
    for c in s.chars() {
        let rank = match (c, in_time) {
            ('T', _) => {
                in_time = true;
                continue;
            }
            ('Y', false) => 1,
            ('M', false) => 2,
            ('D', false) => 3,
            ('H', true) => 4,
            ('M', true) => 5,
            ('S', true) => 6,
            _ => continue,
        };
        if rank <= last {
            return false;
        }
        last = rank;
    }
    true
}

/// Parse xs:yearMonthDuration: a lexical form of `xs:duration` that matches
/// the pattern `[^DT]*` (Datatypes 1.1 §3.4.26.1; XDM 1.0 §2.6.2 restricts
/// `xs:duration` with the same pattern) — no day field, no time part.
fn parse_year_month_duration(s: &str) -> ValidationResult<YearMonthDurationValue> {
    let bad = |message: &str| ValidationError::InvalidLexical {
        value: s.to_string(),
        type_name: "yearMonthDuration",
        message: message.to_string(),
    };
    let duration = parse_duration(s).map_err(|e| match e {
        ValidationError::InvalidLexical { message, .. } => bad(&message),
        other => other,
    })?;
    if s.contains(['D', 'T']) {
        return Err(bad(
            "yearMonthDuration cannot contain day or time components",
        ));
    }
    Ok(YearMonthDurationValue {
        negative: duration.negative,
        years: duration.years,
        months: duration.months,
    })
}

/// Parse xs:dayTimeDuration: a lexical form of `xs:duration` that matches
/// the pattern `[^YM]*[DT].*` (Datatypes 1.1 §3.4.27.1; XDM 1.0 §2.6.2) — a
/// day field or a time part, and no year or month field.
fn parse_day_time_duration(s: &str) -> ValidationResult<DayTimeDurationValue> {
    let bad = |message: &str| ValidationError::InvalidLexical {
        value: s.to_string(),
        type_name: "dayTimeDuration",
        message: message.to_string(),
    };
    let duration = parse_duration(s).map_err(|e| match e {
        ValidationError::InvalidLexical { message, .. } => bad(&message),
        other => other,
    })?;
    // Year and month fields precede the day field and the time part, whose
    // `M` is minutes.
    let date_part = s.split('T').next().unwrap_or(s);
    if date_part.contains(['Y', 'M']) {
        return Err(bad(
            "dayTimeDuration cannot contain year or month components",
        ));
    }
    if !s.contains(['D', 'T']) {
        return Err(bad("dayTimeDuration needs a day or a time component"));
    }
    Ok(DayTimeDurationValue {
        negative: duration.negative,
        days: duration.days,
        hours: duration.hours,
        minutes: duration.minutes,
        seconds: duration.seconds,
    })
}

// ============================================================================
// XML Name Validation Helpers
// ============================================================================

/// Check if a character is a valid XML name start character
fn is_name_start_char(c: char) -> bool {
    matches!(c,
        'A'..='Z' | 'a'..='z' | '_' | ':' |
        '\u{C0}'..='\u{D6}' | '\u{D8}'..='\u{F6}' | '\u{F8}'..='\u{2FF}' |
        '\u{370}'..='\u{37D}' | '\u{37F}'..='\u{1FFF}' | '\u{200C}'..='\u{200D}' |
        '\u{2070}'..='\u{218F}' | '\u{2C00}'..='\u{2FEF}' | '\u{3001}'..='\u{D7FF}' |
        '\u{F900}'..='\u{FDCF}' | '\u{FDF0}'..='\u{FFFD}' | '\u{10000}'..='\u{EFFFF}'
    )
}

/// Check if a character is a valid XML name character
fn is_name_char(c: char) -> bool {
    is_name_start_char(c)
        || matches!(c,
            '-' | '.' | '0'..='9' | '\u{B7}' |
            '\u{0300}'..='\u{036F}' | '\u{203F}'..='\u{2040}'
        )
}

/// Check if a character is a valid NCName start character (no colon)
fn is_ncname_start_char(c: char) -> bool {
    is_name_start_char(c) && c != ':'
}

/// Check if a character is a valid NCName character (no colon)
fn is_ncname_char(c: char) -> bool {
    is_name_char(c) && c != ':'
}

/// Validate an XML Name
fn is_valid_name(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if is_name_start_char(c) => chars.all(is_name_char),
        _ => false,
    }
}

/// Validate an NCName (Name without colons)
fn is_valid_ncname(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if is_ncname_start_char(c) => chars.all(is_ncname_char),
        _ => false,
    }
}

/// Validate a language tag (RFC 3066 / BCP 47 simplified)
pub fn is_valid_language(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let parts: Vec<&str> = s.split('-').collect();
    if parts.is_empty() {
        return false;
    }
    // First subtag: 1-8 letters
    let first = parts[0];
    if first.is_empty() || first.len() > 8 || !first.chars().all(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    // Subsequent subtags: 1-8 alphanumeric characters
    for part in parts.iter().skip(1) {
        if part.is_empty() || part.len() > 8 || !part.chars().all(|c| c.is_ascii_alphanumeric()) {
            return false;
        }
    }
    true
}

// ============================================================================
// Integer Hierarchy Validators
// ============================================================================

/// Validator for xs:long
pub struct LongValidator;

impl TypeValidator for LongValidator {
    fn type_name(&self) -> &'static str {
        "long"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::Long
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::Decimal
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        normalized
            .parse::<i64>()
            .map(|v| {
                XmlValue::new(
                    XmlTypeCode::Long,
                    XmlValueKind::Atomic(XmlAtomicValue::Integer(BigInt::from(v))),
                )
            })
            .map_err(|e| ValidationError::InvalidLexical {
                value: value.to_string(),
                type_name: "long",
                message: e.to_string(),
            })
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        facets.validate_decimal_family(&result, &normalized)?;
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "totalDigits"
                | "fractionDigits"
                | "minInclusive"
                | "maxInclusive"
                | "minExclusive"
                | "maxExclusive"
                | "pattern"
                | "enumeration"
        )
    }
}

/// Validator for xs:int
pub struct IntValidator;

impl TypeValidator for IntValidator {
    fn type_name(&self) -> &'static str {
        "int"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::Int
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::Decimal
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        normalized
            .parse::<i32>()
            .map(|v| {
                XmlValue::new(
                    XmlTypeCode::Int,
                    XmlValueKind::Atomic(XmlAtomicValue::Integer(BigInt::from(v))),
                )
            })
            .map_err(|e| ValidationError::InvalidLexical {
                value: value.to_string(),
                type_name: "int",
                message: e.to_string(),
            })
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        facets.validate_decimal_family(&result, &normalized)?;
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "totalDigits"
                | "fractionDigits"
                | "minInclusive"
                | "maxInclusive"
                | "minExclusive"
                | "maxExclusive"
                | "pattern"
                | "enumeration"
        )
    }
}

/// Validator for xs:short
pub struct ShortValidator;

impl TypeValidator for ShortValidator {
    fn type_name(&self) -> &'static str {
        "short"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::Short
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::Decimal
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        normalized
            .parse::<i16>()
            .map(|v| {
                XmlValue::new(
                    XmlTypeCode::Short,
                    XmlValueKind::Atomic(XmlAtomicValue::Integer(BigInt::from(v))),
                )
            })
            .map_err(|e| ValidationError::InvalidLexical {
                value: value.to_string(),
                type_name: "short",
                message: e.to_string(),
            })
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        facets.validate_decimal_family(&result, &normalized)?;
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "totalDigits"
                | "fractionDigits"
                | "minInclusive"
                | "maxInclusive"
                | "minExclusive"
                | "maxExclusive"
                | "pattern"
                | "enumeration"
        )
    }
}

/// Validator for xs:byte
pub struct ByteValidator;

impl TypeValidator for ByteValidator {
    fn type_name(&self) -> &'static str {
        "byte"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::Byte
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::Decimal
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        normalized
            .parse::<i8>()
            .map(|v| {
                XmlValue::new(
                    XmlTypeCode::Byte,
                    XmlValueKind::Atomic(XmlAtomicValue::Integer(BigInt::from(v))),
                )
            })
            .map_err(|e| ValidationError::InvalidLexical {
                value: value.to_string(),
                type_name: "byte",
                message: e.to_string(),
            })
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        facets.validate_decimal_family(&result, &normalized)?;
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "totalDigits"
                | "fractionDigits"
                | "minInclusive"
                | "maxInclusive"
                | "minExclusive"
                | "maxExclusive"
                | "pattern"
                | "enumeration"
        )
    }
}

/// Validator for xs:nonNegativeInteger
pub struct NonNegativeIntegerValidator;

impl TypeValidator for NonNegativeIntegerValidator {
    fn type_name(&self) -> &'static str {
        "nonNegativeInteger"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::NonNegativeInteger
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::Decimal
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        let bigint = normalized
            .parse::<BigInt>()
            .map_err(|e| ValidationError::InvalidLexical {
                value: value.to_string(),
                type_name: "nonNegativeInteger",
                message: e.to_string(),
            })?;
        if bigint < BigInt::from(0) {
            return Err(ValidationError::RangeError {
                value: value.to_string(),
                type_name: "nonNegativeInteger",
            });
        }
        Ok(XmlValue::new(
            XmlTypeCode::NonNegativeInteger,
            XmlValueKind::Atomic(XmlAtomicValue::Integer(bigint)),
        ))
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        facets.validate_decimal_family(&result, &normalized)?;
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "totalDigits"
                | "fractionDigits"
                | "minInclusive"
                | "maxInclusive"
                | "minExclusive"
                | "maxExclusive"
                | "pattern"
                | "enumeration"
        )
    }
}

/// Validator for xs:positiveInteger
pub struct PositiveIntegerValidator;

impl TypeValidator for PositiveIntegerValidator {
    fn type_name(&self) -> &'static str {
        "positiveInteger"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::PositiveInteger
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::Decimal
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        let bigint = normalized
            .parse::<BigInt>()
            .map_err(|e| ValidationError::InvalidLexical {
                value: value.to_string(),
                type_name: "positiveInteger",
                message: e.to_string(),
            })?;
        if bigint <= BigInt::from(0) {
            return Err(ValidationError::RangeError {
                value: value.to_string(),
                type_name: "positiveInteger",
            });
        }
        Ok(XmlValue::new(
            XmlTypeCode::PositiveInteger,
            XmlValueKind::Atomic(XmlAtomicValue::Integer(bigint)),
        ))
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        facets.validate_decimal_family(&result, &normalized)?;
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "totalDigits"
                | "fractionDigits"
                | "minInclusive"
                | "maxInclusive"
                | "minExclusive"
                | "maxExclusive"
                | "pattern"
                | "enumeration"
        )
    }
}

/// Validator for xs:nonPositiveInteger
pub struct NonPositiveIntegerValidator;

impl TypeValidator for NonPositiveIntegerValidator {
    fn type_name(&self) -> &'static str {
        "nonPositiveInteger"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::NonPositiveInteger
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::Decimal
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        let bigint = normalized
            .parse::<BigInt>()
            .map_err(|e| ValidationError::InvalidLexical {
                value: value.to_string(),
                type_name: "nonPositiveInteger",
                message: e.to_string(),
            })?;
        if bigint > BigInt::from(0) {
            return Err(ValidationError::RangeError {
                value: value.to_string(),
                type_name: "nonPositiveInteger",
            });
        }
        Ok(XmlValue::new(
            XmlTypeCode::NonPositiveInteger,
            XmlValueKind::Atomic(XmlAtomicValue::Integer(bigint)),
        ))
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        facets.validate_decimal_family(&result, &normalized)?;
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "totalDigits"
                | "fractionDigits"
                | "minInclusive"
                | "maxInclusive"
                | "minExclusive"
                | "maxExclusive"
                | "pattern"
                | "enumeration"
        )
    }
}

/// Validator for xs:negativeInteger
pub struct NegativeIntegerValidator;

impl TypeValidator for NegativeIntegerValidator {
    fn type_name(&self) -> &'static str {
        "negativeInteger"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::NegativeInteger
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::Decimal
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        let bigint = normalized
            .parse::<BigInt>()
            .map_err(|e| ValidationError::InvalidLexical {
                value: value.to_string(),
                type_name: "negativeInteger",
                message: e.to_string(),
            })?;
        if bigint >= BigInt::from(0) {
            return Err(ValidationError::RangeError {
                value: value.to_string(),
                type_name: "negativeInteger",
            });
        }
        Ok(XmlValue::new(
            XmlTypeCode::NegativeInteger,
            XmlValueKind::Atomic(XmlAtomicValue::Integer(bigint)),
        ))
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        facets.validate_decimal_family(&result, &normalized)?;
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "totalDigits"
                | "fractionDigits"
                | "minInclusive"
                | "maxInclusive"
                | "minExclusive"
                | "maxExclusive"
                | "pattern"
                | "enumeration"
        )
    }
}

/// The lexical form `parse::<u64>()` (and the narrower unsigned parses)
/// should read for a collapsed unsigned-integer literal: Datatypes 1.1
/// §3.4.21.1 (unsignedLong; §3.4.22.1 – §3.4.24.1 alike) — "If the sign is
/// present, it must be '+' except for lexical forms denoting zero, which may
/// be preceded by a positive ('+') or a negative ('-') sign". XSD 1.0 has the
/// same lexical space: these types restrict `nonNegativeInteger`, whose zero
/// "may be preceded by a positive ("+") or a negative ("-") sign" (§3.3.20.1),
/// by `maxInclusive` only (§2.4.2: "Constraining the ·value space·
/// consequently constrains the ·lexical space·"). So `-0`, `-00`, … read as
/// `0`; every other form is left to the parse, which takes an optional `+`.
pub(crate) fn unsigned_lexical(normalized: &str) -> &str {
    match normalized.strip_prefix('-') {
        Some(digits) if !digits.is_empty() && digits.bytes().all(|b| b == b'0') => "0",
        _ => normalized,
    }
}

/// Validator for xs:unsignedLong
pub struct UnsignedLongValidator;

impl TypeValidator for UnsignedLongValidator {
    fn type_name(&self) -> &'static str {
        "unsignedLong"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::UnsignedLong
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::Decimal
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        unsigned_lexical(&normalized)
            .parse::<u64>()
            .map(|v| {
                XmlValue::new(
                    XmlTypeCode::UnsignedLong,
                    XmlValueKind::Atomic(XmlAtomicValue::Integer(BigInt::from(v))),
                )
            })
            .map_err(|e| ValidationError::InvalidLexical {
                value: value.to_string(),
                type_name: "unsignedLong",
                message: e.to_string(),
            })
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        facets.validate_decimal_family(&result, &normalized)?;
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "totalDigits"
                | "fractionDigits"
                | "minInclusive"
                | "maxInclusive"
                | "minExclusive"
                | "maxExclusive"
                | "pattern"
                | "enumeration"
        )
    }
}

/// Validator for xs:unsignedInt
pub struct UnsignedIntValidator;

impl TypeValidator for UnsignedIntValidator {
    fn type_name(&self) -> &'static str {
        "unsignedInt"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::UnsignedInt
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::Decimal
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        unsigned_lexical(&normalized)
            .parse::<u32>()
            .map(|v| {
                XmlValue::new(
                    XmlTypeCode::UnsignedInt,
                    XmlValueKind::Atomic(XmlAtomicValue::Integer(BigInt::from(v))),
                )
            })
            .map_err(|e| ValidationError::InvalidLexical {
                value: value.to_string(),
                type_name: "unsignedInt",
                message: e.to_string(),
            })
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        facets.validate_decimal_family(&result, &normalized)?;
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "totalDigits"
                | "fractionDigits"
                | "minInclusive"
                | "maxInclusive"
                | "minExclusive"
                | "maxExclusive"
                | "pattern"
                | "enumeration"
        )
    }
}

/// Validator for xs:unsignedShort
pub struct UnsignedShortValidator;

impl TypeValidator for UnsignedShortValidator {
    fn type_name(&self) -> &'static str {
        "unsignedShort"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::UnsignedShort
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::Decimal
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        unsigned_lexical(&normalized)
            .parse::<u16>()
            .map(|v| {
                XmlValue::new(
                    XmlTypeCode::UnsignedShort,
                    XmlValueKind::Atomic(XmlAtomicValue::Integer(BigInt::from(v))),
                )
            })
            .map_err(|e| ValidationError::InvalidLexical {
                value: value.to_string(),
                type_name: "unsignedShort",
                message: e.to_string(),
            })
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        facets.validate_decimal_family(&result, &normalized)?;
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "totalDigits"
                | "fractionDigits"
                | "minInclusive"
                | "maxInclusive"
                | "minExclusive"
                | "maxExclusive"
                | "pattern"
                | "enumeration"
        )
    }
}

/// Validator for xs:unsignedByte
pub struct UnsignedByteValidator;

impl TypeValidator for UnsignedByteValidator {
    fn type_name(&self) -> &'static str {
        "unsignedByte"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::UnsignedByte
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::Decimal
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        unsigned_lexical(&normalized)
            .parse::<u8>()
            .map(|v| {
                XmlValue::new(
                    XmlTypeCode::UnsignedByte,
                    XmlValueKind::Atomic(XmlAtomicValue::Integer(BigInt::from(v))),
                )
            })
            .map_err(|e| ValidationError::InvalidLexical {
                value: value.to_string(),
                type_name: "unsignedByte",
                message: e.to_string(),
            })
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        facets.validate_decimal_family(&result, &normalized)?;
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "totalDigits"
                | "fractionDigits"
                | "minInclusive"
                | "maxInclusive"
                | "minExclusive"
                | "maxExclusive"
                | "pattern"
                | "enumeration"
        )
    }
}

// ============================================================================
// QName and NOTATION Validators
// ============================================================================

/// Validator for xs:QName
/// Note: Full QName validation requires namespace context. This validator
/// validates the lexical form (prefix:localname or just localname) and stores
/// the validated string. Namespace resolution must be done separately.
pub struct QNameValidator;

impl TypeValidator for QNameValidator {
    fn type_name(&self) -> &'static str {
        "QName"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::QName
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::QName
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        // QName format: prefix:localname or just localname
        // Both parts must be NCNames
        let (prefix, local) = if let Some(colon_pos) = normalized.find(':') {
            let prefix = &normalized[..colon_pos];
            let local = &normalized[colon_pos + 1..];
            (Some(prefix), local)
        } else {
            (None, normalized.as_ref())
        };

        // Validate prefix is NCName (if present)
        if let Some(p) = prefix {
            if !is_valid_ncname(p) {
                return Err(ValidationError::InvalidLexical {
                    value: value.to_string(),
                    type_name: "QName",
                    message: format!("Invalid prefix '{}' (must be NCName)", p),
                });
            }
        }

        // Validate localname is NCName
        if !is_valid_ncname(local) {
            return Err(ValidationError::InvalidLexical {
                value: value.to_string(),
                type_name: "QName",
                message: format!("Invalid local name '{}' (must be NCName)", local),
            });
        }

        // Store as string for now - namespace resolution requires NamespaceContext
        // which is not available at basic validation time
        Ok(XmlValue::new(
            XmlTypeCode::QName,
            XmlValueKind::Atomic(XmlAtomicValue::String(normalized.into_owned())),
        ))
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        // §cvc-length-valid §1.3 / §cvc-minLength-valid §1.3 / §cvc-maxLength-valid §1.3:
        // For QName any value is always facet-valid w.r.t. length/minLength/maxLength.
        // Only pattern and enumeration constrain the value.
        facets.validate_string_patterns_enums(&result.to_string_value())?;
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "length" | "minLength" | "maxLength" | "pattern" | "enumeration"
        )
    }
}

/// Validator for xs:NOTATION
/// Note: NOTATION validation is similar to QName but the notation must be declared in the schema.
/// This validator validates the lexical form only; notation declaration checking must be done separately.
pub struct NotationValidator;

impl TypeValidator for NotationValidator {
    fn type_name(&self) -> &'static str {
        "NOTATION"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::Notation
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::Notation
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        // NOTATION format: same as QName
        let (prefix, local) = if let Some(colon_pos) = normalized.find(':') {
            let prefix = &normalized[..colon_pos];
            let local = &normalized[colon_pos + 1..];
            (Some(prefix), local)
        } else {
            (None, normalized.as_ref())
        };

        if let Some(p) = prefix {
            if !is_valid_ncname(p) {
                return Err(ValidationError::InvalidLexical {
                    value: value.to_string(),
                    type_name: "NOTATION",
                    message: format!("Invalid prefix '{}' (must be NCName)", p),
                });
            }
        }

        if !is_valid_ncname(local) {
            return Err(ValidationError::InvalidLexical {
                value: value.to_string(),
                type_name: "NOTATION",
                message: format!("Invalid local name '{}' (must be NCName)", local),
            });
        }

        // Store as string for now - notation declaration checking requires schema context
        Ok(XmlValue::new(
            XmlTypeCode::Notation,
            XmlValueKind::Atomic(XmlAtomicValue::String(normalized.into_owned())),
        ))
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        // §cvc-length-valid §1.3 / §cvc-minLength-valid §1.3 / §cvc-maxLength-valid §1.3:
        // For NOTATION any value is always facet-valid w.r.t. length/minLength/maxLength.
        // Only pattern and enumeration constrain the value.
        facets.validate_string_patterns_enums(&result.to_string_value())?;
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "length" | "minLength" | "maxLength" | "pattern" | "enumeration"
        )
    }
}

// ============================================================================
// List Type Validators
// ============================================================================

/// Validator for xs:NMTOKENS (list of NMTOKEN)
pub struct NmTokensValidator;

impl TypeValidator for NmTokensValidator {
    fn type_name(&self) -> &'static str {
        "NMTOKENS"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::NmTokens
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::String
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        if normalized.is_empty() {
            return Err(ValidationError::InvalidLexical {
                value: value.to_string(),
                type_name: "NMTOKENS",
                message: "NMTOKENS must contain at least one token".to_string(),
            });
        }

        let mut items = Vec::new();
        for token in normalized.split_whitespace() {
            if !token.chars().all(is_name_char) {
                return Err(ValidationError::InvalidLexical {
                    value: value.to_string(),
                    type_name: "NMTOKENS",
                    message: format!("Invalid NMTOKEN: '{}'", token),
                });
            }
            items.push(XmlValue::new(
                XmlTypeCode::NmToken,
                XmlValueKind::Atomic(XmlAtomicValue::String(token.to_string())),
            ));
        }

        Ok(XmlValue::new(
            XmlTypeCode::NmTokens,
            XmlValueKind::List {
                item_type: XmlTypeCode::NmToken,
                items,
            },
        ))
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        if let XmlValueKind::List { items, .. } = &result.value {
            facets.validate_list_length(items.len() as u64)?;
        }
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "length" | "minLength" | "maxLength" | "pattern" | "enumeration"
        )
    }
}

/// Validator for xs:IDREFS (list of IDREF)
pub struct IdRefsValidator;

impl TypeValidator for IdRefsValidator {
    fn type_name(&self) -> &'static str {
        "IDREFS"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::IdRefs
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::String
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        if normalized.is_empty() {
            return Err(ValidationError::InvalidLexical {
                value: value.to_string(),
                type_name: "IDREFS",
                message: "IDREFS must contain at least one IDREF".to_string(),
            });
        }

        let mut items = Vec::new();
        for token in normalized.split_whitespace() {
            if !is_valid_ncname(token) {
                return Err(ValidationError::InvalidLexical {
                    value: value.to_string(),
                    type_name: "IDREFS",
                    message: format!("Invalid IDREF: '{}' (must be NCName)", token),
                });
            }
            items.push(XmlValue::new(
                XmlTypeCode::IdRef,
                XmlValueKind::Atomic(XmlAtomicValue::String(token.to_string())),
            ));
        }

        Ok(XmlValue::new(
            XmlTypeCode::IdRefs,
            XmlValueKind::List {
                item_type: XmlTypeCode::IdRef,
                items,
            },
        ))
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        if let XmlValueKind::List { items, .. } = &result.value {
            facets.validate_list_length(items.len() as u64)?;
        }
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "length" | "minLength" | "maxLength" | "pattern" | "enumeration"
        )
    }
}

/// Validator for xs:ENTITIES (list of ENTITY)
pub struct EntitiesValidator;

impl TypeValidator for EntitiesValidator {
    fn type_name(&self) -> &'static str {
        "ENTITIES"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::Entities
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::String
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        if normalized.is_empty() {
            return Err(ValidationError::InvalidLexical {
                value: value.to_string(),
                type_name: "ENTITIES",
                message: "ENTITIES must contain at least one ENTITY".to_string(),
            });
        }

        let mut items = Vec::new();
        for token in normalized.split_whitespace() {
            if !is_valid_ncname(token) {
                return Err(ValidationError::InvalidLexical {
                    value: value.to_string(),
                    type_name: "ENTITIES",
                    message: format!("Invalid ENTITY: '{}' (must be NCName)", token),
                });
            }
            items.push(XmlValue::new(
                XmlTypeCode::Entity,
                XmlValueKind::Atomic(XmlAtomicValue::String(token.to_string())),
            ));
        }

        Ok(XmlValue::new(
            XmlTypeCode::Entities,
            XmlValueKind::List {
                item_type: XmlTypeCode::Entity,
                items,
            },
        ))
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        if let XmlValueKind::List { items, .. } = &result.value {
            facets.validate_list_length(items.len() as u64)?;
        }
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "length" | "minLength" | "maxLength" | "pattern" | "enumeration"
        )
    }
}

// ============================================================================
// XSD 1.1 Duration Validators
// ============================================================================

/// Validator for xs:yearMonthDuration (XSD 1.1)
pub struct YearMonthDurationValidator;

impl TypeValidator for YearMonthDurationValidator {
    fn type_name(&self) -> &'static str {
        "yearMonthDuration"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::YearMonthDuration
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::Duration
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        parse_year_month_duration(&normalized).map(|d| {
            XmlValue::new(
                XmlTypeCode::YearMonthDuration,
                XmlValueKind::Atomic(XmlAtomicValue::YearMonthDuration(d)),
            )
        })
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        if let XmlValueKind::Atomic(XmlAtomicValue::YearMonthDuration(ref dur)) = result.value {
            validate_bounds(dur, facets, "yearMonthDuration", value, |s| {
                parse_year_month_duration(&normalize_whitespace(s, WhitespaceMode::Collapse)).ok()
            })?;
            facets.validate_enum_value_space(
                enum_matches_value_space!(dur, parse_year_month_duration, |a, b| {
                    year_month_duration_value_eq(a, &b)
                }),
                value,
            )?;
        }
        facets.validate_patterns_only(value)?;
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "minInclusive"
                | "maxInclusive"
                | "minExclusive"
                | "maxExclusive"
                | "pattern"
                | "enumeration"
        )
    }
}

/// Validator for xs:dayTimeDuration (XSD 1.1)
pub struct DayTimeDurationValidator;

impl TypeValidator for DayTimeDurationValidator {
    fn type_name(&self) -> &'static str {
        "dayTimeDuration"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::DayTimeDuration
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::Duration
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        parse_day_time_duration(&normalized).map(|d| {
            XmlValue::new(
                XmlTypeCode::DayTimeDuration,
                XmlValueKind::Atomic(XmlAtomicValue::DayTimeDuration(d)),
            )
        })
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        if let XmlValueKind::Atomic(XmlAtomicValue::DayTimeDuration(ref dur)) = result.value {
            validate_bounds(dur, facets, "dayTimeDuration", value, |s| {
                parse_day_time_duration(&normalize_whitespace(s, WhitespaceMode::Collapse)).ok()
            })?;
            facets.validate_enum_value_space(
                enum_matches_literal!(dur, value, parse_day_time_duration),
                value,
            )?;
        }
        facets.validate_patterns_only(value)?;
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "minInclusive"
                | "maxInclusive"
                | "minExclusive"
                | "maxExclusive"
                | "pattern"
                | "enumeration"
        )
    }
}

/// Validator for `xs:untypedAtomic`, the XPath 2.0 data model's type of
/// "untyped atomic data, such as text that has not been assigned a more
/// specific type" (XDM 1.0 §2.6.2): every string is a value of it, unchanged —
/// §2.6.4: "When xs:untypedAtomic is specified as the type, no validation takes
/// place". Its typed value is the string as an `xs:untypedAtomic`. The type
/// has no facets of its own, so no white space is normalized; a facet a
/// restriction of it adds applies to the string.
pub struct UntypedAtomicValidator;

impl TypeValidator for UntypedAtomicValidator {
    fn type_name(&self) -> &'static str {
        "untypedAtomic"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::UntypedAtomic
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::String
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Preserve
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        Ok(XmlValue::untyped(value))
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let ws = facets
            .whitespace
            .as_ref()
            .map(|w| w.value)
            .unwrap_or(WhitespaceMode::Preserve);
        let normalized = normalize_whitespace(value, ws);
        facets.validate_string(&normalized)?;
        Ok(XmlValue::untyped(normalized.into_owned()))
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(facet, "pattern" | "enumeration" | "whitespace")
    }
}

/// Validator for xs:dateTimeStamp (XSD 1.1)
/// dateTimeStamp is dateTime with required timezone
pub struct DateTimeStampValidator;

impl TypeValidator for DateTimeStampValidator {
    fn type_name(&self) -> &'static str {
        "dateTimeStamp"
    }
    fn type_code(&self) -> XmlTypeCode {
        XmlTypeCode::DateTimeStamp
    }
    fn primitive_type(&self) -> PrimitiveTypeCode {
        PrimitiveTypeCode::DateTime
    }
    fn whitespace(&self) -> WhitespaceMode {
        WhitespaceMode::Collapse
    }

    fn validate(&self, value: &str) -> ValidationResult<XmlValue> {
        let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
        let dt = parse_datetime(&normalized)?;

        // dateTimeStamp requires timezone
        if dt.timezone.is_none() {
            return Err(ValidationError::InvalidLexical {
                value: value.to_string(),
                type_name: "dateTimeStamp",
                message: "dateTimeStamp requires a timezone".to_string(),
            });
        }

        Ok(XmlValue::new(
            XmlTypeCode::DateTimeStamp,
            XmlValueKind::Atomic(XmlAtomicValue::DateTime(dt)),
        ))
    }

    fn validate_with_facets(&self, value: &str, facets: &FacetSet) -> ValidationResult<XmlValue> {
        let result = self.validate(value)?;
        if let XmlValueKind::Atomic(XmlAtomicValue::DateTime(ref dt)) = result.value {
            facets.validate_explicit_timezone(dt.timezone.is_some())?;
            validate_bounds(dt, facets, "dateTimeStamp", value, |s| {
                parse_datetime(&normalize_whitespace(s, WhitespaceMode::Collapse)).ok()
            })?;
            facets.validate_enum_value_space(
                enum_matches_literal!(dt, value, parse_datetime),
                value,
            )?;
        }
        facets.validate_patterns_only(value)?;
        Ok(result)
    }

    fn facet_applicable(&self, facet: &str) -> bool {
        matches!(
            facet,
            "minInclusive"
                | "maxInclusive"
                | "minExclusive"
                | "maxExclusive"
                | "pattern"
                | "enumeration"
                | "explicitTimezone"
        )
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_validator() {
        let v = StringValidator;
        let result = v.validate("hello").unwrap();
        assert_eq!(result.type_code, XmlTypeCode::String);
        assert_eq!(result.to_string_value(), "hello");
    }

    #[test]
    fn test_boolean_validator() {
        let v = BooleanValidator;
        assert_eq!(v.validate("true").unwrap().as_boolean(), Some(true));
        assert_eq!(v.validate("false").unwrap().as_boolean(), Some(false));
        assert_eq!(v.validate("1").unwrap().as_boolean(), Some(true));
        assert_eq!(v.validate("0").unwrap().as_boolean(), Some(false));
        assert!(v.validate("yes").is_err());
    }

    #[test]
    fn test_decimal_validator() {
        let v = DecimalValidator;
        let result = v.validate("123.45").unwrap();
        assert_eq!(result.type_code, XmlTypeCode::Decimal);
        assert!(result.as_decimal().is_some());
    }

    #[test]
    fn test_decimal_lexical_space() {
        // Datatypes 1.1 §3.3.3: sign, digits, optional decimal point — nothing else.
        for valid in ["210", "-1.23", "+100000.00", "0.5", ".5", "-.5", "5.", "+0"] {
            assert!(
                is_valid_xsd_decimal_lexical(valid),
                "expected valid: {valid:?}"
            );
        }
        for invalid in [
            "",
            "+",
            "-",
            ".",
            "+.",
            "1E4",
            "-1E4",
            "123.456E4",
            "1e4",
            "1.2e-3",
            "INF",
            "-INF",
            "NaN",
            "1_000",
            "1.2.3",
            "1 2",
            "0x1F",
        ] {
            assert!(
                !is_valid_xsd_decimal_lexical(invalid),
                "expected invalid: {invalid:?}"
            );
        }
    }

    #[test]
    fn test_decimal_validator_rejects_exponent_forms() {
        // W3C decimal019 / decimal025: the xs:decimal lexical space has no
        // exponent. Must hold regardless of how permissive `rust_decimal`'s
        // own parser is (1.42 started accepting scientific notation).
        let v = DecimalValidator;
        assert!(v.validate("-1E4").is_err());
        assert!(v.validate("123.456E4").is_err());
        assert!(v.validate("1e2").is_err());
        assert!(v.validate("INF").is_err());
        assert!(v.validate("NaN").is_err());
        // Whitespace collapse still applies before the lexical check.
        assert!(v.validate("  -1.23  ").is_ok());
    }

    #[test]
    fn test_integer_validator() {
        let v = IntegerValidator;
        let result = v.validate("12345").unwrap();
        assert_eq!(result.type_code, XmlTypeCode::Integer);
        assert_eq!(result.as_integer(), Some(&BigInt::from(12345)));
    }

    #[test]
    fn test_float_validator() {
        let v = FloatValidator;
        assert!(v.validate("2.5").is_ok());
        assert!(v.validate("INF").is_ok());
        assert!(v.validate("-INF").is_ok());
        assert!(v.validate("NaN").is_ok());
    }

    #[test]
    fn test_double_validator() {
        let v = DoubleValidator;
        let result = v.validate("2.718281828").unwrap();
        assert_eq!(result.type_code, XmlTypeCode::Double);
    }

    #[test]
    fn test_datetime_validator() {
        let v = DateTimeValidator;
        let result = v.validate("2024-03-15T10:30:00Z").unwrap();
        assert_eq!(result.type_code, XmlTypeCode::DateTime);
    }

    #[test]
    fn test_date_validator() {
        let v = DateValidator;
        let result = v.validate("2024-03-15").unwrap();
        assert_eq!(result.type_code, XmlTypeCode::Date);
    }

    #[test]
    fn test_time_validator() {
        let v = TimeValidator;
        let result = v.validate("10:30:00").unwrap();
        assert_eq!(result.type_code, XmlTypeCode::Time);
    }

    #[test]
    fn test_time_validator_timezone_offsets() {
        let validator = TimeValidator;

        for offset in [
            "", "Z", "+00:00", "-00:00", "+01:30", "-01:30", "+13:59", "-13:59", "+14:00", "-14:00",
        ] {
            let input = format!("09:15:00{offset}");
            let result = validator.validate(&input);
            assert!(
                result.is_ok(),
                "expected valid time {input:?}, got {result:?}"
            );
        }

        for offset in [
            "+1:30", "-1:30", "+01:3", "-01:3", "+001:30", "+01:030", "+01:60", "-01:60", "+14:01",
            "-14:01", "+14:59", "-14:59", "+15:00", "-15:00", "+24:00", "-24:00", "+", "-", "+01",
            "+01:", "++01:30", "+01:+30", "+01:-30", "-01:-30",
        ] {
            let input = format!("09:15:00{offset}");
            assert!(
                validator.validate(&input).is_err(),
                "expected invalid time {input:?} to be rejected"
            );
        }
    }

    #[test]
    fn test_negative_year_rejects_trailing_segments() {
        // A negative year must not let an unaccepted timezone (or anything
        // else) after the last field slip through and be dropped.
        let result = DateValidator.validate("-2024-01-15-05:00").unwrap();
        match result.value {
            XmlValueKind::Atomic(XmlAtomicValue::Date(d)) => {
                assert_eq!((d.year, d.month, d.day), (-2024, 1, 15));
                assert_eq!(d.timezone, Some(TimezoneOffset(-300)));
            }
            other => panic!("expected a date, got {other:?}"),
        }
        for input in ["-2024-01-15", "-2024-01-15Z", "-2024-01-15+14:00"] {
            assert!(DateValidator.validate(input).is_ok(), "{input:?}");
        }
        for input in ["-2024-05", "-2024-05Z", "-2024-05-05:00"] {
            assert!(GYearMonthValidator.validate(input).is_ok(), "{input:?}");
        }

        for input in [
            "-2024-01-15-15:00",
            "-2024-01-15-14:01",
            "-2024-01-15-1:30",
            "-2024-01-15-junk",
            "-2024-01-15-01-01",
        ] {
            assert!(DateValidator.validate(input).is_err(), "{input:?}");
        }
        for input in [
            "-2024-05-15:00",
            "-2024-05-14:01",
            "-2024-05-junk",
            "-2024-05-01",
        ] {
            assert!(GYearMonthValidator.validate(input).is_err(), "{input:?}");
        }
    }

    #[test]
    fn test_month_and_day_fields_are_two_digits() {
        assert!(DateValidator.validate("2002-02-15").is_ok());
        assert!(DateTimeValidator.validate("2002-02-15T12:44:05").is_ok());
        assert!(GYearMonthValidator.validate("2002-02").is_ok());
        assert!(GMonthDayValidator.validate("--02-15").is_ok());
        assert!(GDayValidator.validate("---01").is_ok());
        assert!(GMonthValidator.validate("--02").is_ok());

        for input in [
            "2002-002-15",
            "2002-2-15",
            "2002-02-5",
            "2002-02-015",
            "2002-+2-15",
        ] {
            assert!(DateValidator.validate(input).is_err(), "{input:?}");
        }
        assert!(DateValidator.validate("-2002-002-15").is_err());
        assert!(DateTimeValidator.validate("2002-002-15T12:44:05").is_err());
        for input in ["2002-002", "2002-2", "2002-+2", "-2002-002"] {
            assert!(GYearMonthValidator.validate(input).is_err(), "{input:?}");
        }
        for input in ["--002-15", "--2-15", "--02-5", "--02-015", "--+2-15"] {
            assert!(GMonthDayValidator.validate(input).is_err(), "{input:?}");
        }
        for input in ["---001", "---1", "---+1"] {
            assert!(GDayValidator.validate(input).is_err(), "{input:?}");
        }
        for input in ["--002", "--2", "--+2"] {
            assert!(GMonthValidator.validate(input).is_err(), "{input:?}");
        }
    }

    #[test]
    fn test_duration_validator() {
        let v = DurationValidator;
        let result = v.validate("P1Y2M3DT4H5M6S").unwrap();
        assert_eq!(result.type_code, XmlTypeCode::Duration);
    }

    #[test]
    fn test_hex_binary_validator() {
        let v = HexBinaryValidator;
        let result = v.validate("DEADBEEF").unwrap();
        assert_eq!(result.type_code, XmlTypeCode::HexBinary);
    }

    #[test]
    fn test_base64_binary_validator() {
        let v = Base64BinaryValidator;
        let result = v.validate("SGVsbG8=").unwrap();
        assert_eq!(result.type_code, XmlTypeCode::Base64Binary);
    }

    #[test]
    fn test_anyuri_validator() {
        let v = AnyUriValidator;
        let result = v.validate("http://example.com").unwrap();
        assert_eq!(result.type_code, XmlTypeCode::AnyUri);
    }

    #[test]
    fn test_validator_registry() {
        let registry = ValidatorRegistry::new();
        assert!(registry.get_by_name("string").is_some());
        assert!(registry.get_by_name("integer").is_some());
        assert!(registry.get_by_code(XmlTypeCode::Boolean).is_some());
        assert!(registry.get_by_name("nonexistent").is_none());
    }

    #[test]
    fn test_whitespace_normalization() {
        let v = TokenValidator;
        let result = v.validate("  hello   world  ").unwrap();
        assert_eq!(result.to_string_value(), "hello world");
    }

    /// Seconds with more fraction digits than a `Decimal` holds are
    /// truncated toward zero, never rounded up to the next second, and
    /// flagged for the capacity warning; trailing zeros are not excess.
    #[test]
    fn seconds_beyond_capacity_truncate_toward_zero() {
        let d = |s: &str| Decimal::from_str_exact(s).unwrap();
        assert_eq!(parse_seconds("59.5"), Some(d("59.5")));
        assert_eq!(
            // 59 and 27 nines: 29 digits are the most a 96-bit mantissa holds here.
            parse_seconds("59.99999999999999999999999999999"),
            Some(d("59.999999999999999999999999999"))
        );
        assert_eq!(
            parse_seconds("0.000000000000000000000000000001"),
            Some(d("0"))
        );
        assert_eq!(
            parse_seconds("1.50000000000000000000000000000000"),
            Some(d("1.5"))
        );
        assert_eq!(
            parse_seconds("79228162514264337593543950335.5"),
            Some(d("79228162514264337593543950335"))
        );
        assert_eq!(parse_seconds("79228162514264337593543950336"), None);
        assert_eq!(parse_seconds("1.5e3"), None);
        assert_eq!(
            parse_seconds("00000000000000000000000000000000001.5"),
            Some(d("1.5"))
        );
        assert_eq!(parse_seconds("7"), Some(d("7")));
        assert_eq!(parse_seconds("7."), None);
        assert!(DurationValidator.validate("PT.5S").is_err());
        assert!(DurationValidator.validate("PT1.S").is_err());
        assert!(TimeValidator.validate("10:00:.5").is_err());
        assert!(TimeValidator.validate("10:00:1.5").is_err());

        assert!(!seconds_beyond_capacity("PT59.5S"));
        assert!(!seconds_beyond_capacity(
            "PT1.50000000000000000000000000000000S"
        ));
        assert!(!seconds_beyond_capacity(
            "2000-01-01T00:00:00.0000000000000000000000000001Z"
        ));
        assert!(seconds_beyond_capacity(
            "2000-01-01T00:00:00.00000000000000000000000000001Z"
        ));
        assert!(seconds_beyond_capacity(
            "-PT0.000000000000000000000000000001S"
        ));
        assert!(seconds_beyond_capacity(
            "PT79228162514264337593543950335.5S"
        ));
        assert!(!seconds_beyond_capacity(
            "PT00000000000000000000000000000000001.5S"
        ));
        assert!(!seconds_beyond_capacity(
            "PT7.9228162514264337593543950335S"
        ));
        assert!(seconds_beyond_capacity("PT7.9228162514264337593543950336S"));

        let dt = DateTimeValidator
            .validate("2000-01-01T23:59:59.99999999999999999999999999999Z")
            .unwrap();
        assert_eq!(
            dt.to_string_value(),
            "2000-01-01T23:59:59.999999999999999999999999999Z"
        );
        assert!(DurationValidator.validate("PT1_0S").is_err());
        assert!(TimeValidator.validate("10:00:0_1").is_err());
        assert!(TimeValidator.validate("10:00:01._1").is_err());
    }
}
