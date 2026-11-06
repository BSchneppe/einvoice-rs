//! Error types for XSD parsing and validation
//!
//! All errors include source locations when available for developer-friendly messages.

use crate::parser::location::SourceLocation;
use thiserror::Error;

/// Result type for schema operations
pub type SchemaResult<T> = Result<T, SchemaError>;

/// Result type for facet operations
pub type FacetResult<T> = Result<T, FacetError>;

/// Facet-related errors (validation and derivation)
#[derive(Error, Debug, Clone)]
pub enum FacetError {
    /// Value violates length constraint
    #[error("length constraint violation: {message}")]
    LengthViolation { message: String },

    /// Value violates minLength constraint
    #[error("minLength constraint violation: value length {actual} is less than minimum {min}")]
    MinLengthViolation { actual: u64, min: u64 },

    /// Value violates maxLength constraint
    #[error("maxLength constraint violation: value length {actual} exceeds maximum {max}")]
    MaxLengthViolation { actual: u64, max: u64 },

    /// Value doesn't match pattern
    #[error("pattern constraint violation: value '{value}' does not match pattern '{pattern}'")]
    PatternViolation { value: String, pattern: String },

    /// Value not in enumeration
    #[error("enumeration constraint violation: value '{value}' is not in the allowed set")]
    EnumerationViolation { value: String },

    /// Value violates minInclusive constraint
    #[error("minInclusive constraint violation: value '{value}' is less than minimum '{min}'")]
    MinInclusiveViolation { value: String, min: String },

    /// Value violates maxInclusive constraint
    #[error("maxInclusive constraint violation: value '{value}' is greater than maximum '{max}'")]
    MaxInclusiveViolation { value: String, max: String },

    /// Value violates minExclusive constraint
    #[error("minExclusive constraint violation: value '{value}' is not greater than '{min}'")]
    MinExclusiveViolation { value: String, min: String },

    /// Value violates maxExclusive constraint
    #[error("maxExclusive constraint violation: value '{value}' is not less than '{max}'")]
    MaxExclusiveViolation { value: String, max: String },

    /// Value violates totalDigits constraint
    #[error("totalDigits constraint violation: value has {actual} digits, maximum is {max}")]
    TotalDigitsViolation { actual: u32, max: u32 },

    /// Value violates fractionDigits constraint
    #[error(
        "fractionDigits constraint violation: value has {actual} fraction digits, maximum is {max}"
    )]
    FractionDigitsViolation { actual: u32, max: u32 },

    /// Value violates explicitTimezone constraint
    #[error("explicitTimezone constraint violation: {message}")]
    ExplicitTimezoneViolation { message: String },

    /// Invalid pattern regex
    #[error("invalid pattern regex '{pattern}': {message}")]
    InvalidPattern { pattern: String, message: String },

    /// Facet derivation error - derived type is not more restrictive
    #[error("derivation restriction violation: {message}")]
    DerivationRestriction { message: String },

    /// Facet derivation error - fixed facet cannot be overridden
    #[error("fixed facet violation: cannot override fixed {facet_name} value '{base_value}' with '{derived_value}'")]
    FixedFacetViolation {
        facet_name: String,
        base_value: String,
        derived_value: String,
    },

    /// Facet derivation error - conflicting facets
    #[error("conflicting facets: {message}")]
    ConflictingFacets { message: String },

    /// Facet not applicable to this type
    #[error("facet '{facet}' is not applicable to type '{type_name}'")]
    NotApplicable { facet: String, type_name: String },
}

impl FacetError {
    /// Create a length violation error
    pub fn length(message: impl Into<String>) -> Self {
        FacetError::LengthViolation {
            message: message.into(),
        }
    }

    /// Create a pattern violation error
    pub fn pattern(value: impl Into<String>, pattern: impl Into<String>) -> Self {
        FacetError::PatternViolation {
            value: value.into(),
            pattern: pattern.into(),
        }
    }

    /// Create an enumeration violation error
    pub fn enumeration(value: impl Into<String>) -> Self {
        FacetError::EnumerationViolation {
            value: value.into(),
        }
    }

    /// Create a derivation restriction error
    pub fn derivation(message: impl Into<String>) -> Self {
        FacetError::DerivationRestriction {
            message: message.into(),
        }
    }

    /// Create a fixed facet violation error
    pub fn fixed_violation(
        facet_name: impl Into<String>,
        base_value: impl Into<String>,
        derived_value: impl Into<String>,
    ) -> Self {
        FacetError::FixedFacetViolation {
            facet_name: facet_name.into(),
            base_value: base_value.into(),
            derived_value: derived_value.into(),
        }
    }

    /// Create a conflicting facets error
    pub fn conflicting(message: impl Into<String>) -> Self {
        FacetError::ConflictingFacets {
            message: message.into(),
        }
    }
}

/// A limit of the crate that a schema can break (see
/// [`SchemaError::LimitExceeded`]).
///
/// `Display` (and [`setting`](Self::setting)) is the setting that holds it,
/// e.g. `ParserConfig::max_depth`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SchemaLimit {
    /// The element nesting of a schema document
    /// ([`ParserConfig::max_depth`](crate::parser::parse::ParserConfig::max_depth)).
    ElementDepth,
    /// The nesting of components through references
    /// ([`SchemaSet::max_depth`](crate::SchemaSet::max_depth), set from
    /// `ParserConfig::max_depth`).
    ComponentDepth,
    /// The nesting of `xs:include` / `xs:import` / `xs:redefine` /
    /// `xs:override` directives
    /// ([`ResolverConfig::max_depth`](crate::ResolverConfig::max_depth)).
    DirectiveDepth,
    /// The nesting of an XPath expression of the schema (an assertion, a
    /// type alternative): `xpath::depth::MAX_EXPRESSION_DEPTH`.
    XPathDepth,
}

impl SchemaLimit {
    /// The setting that holds the limit, e.g. `"ParserConfig::max_depth"`.
    pub fn setting(self) -> &'static str {
        match self {
            SchemaLimit::ElementDepth => "ParserConfig::max_depth",
            SchemaLimit::ComponentDepth => "SchemaSet::max_depth",
            SchemaLimit::DirectiveDepth => "ResolverConfig::max_depth",
            SchemaLimit::XPathDepth => "xpath::depth::MAX_EXPRESSION_DEPTH",
        }
    }
}

impl std::fmt::Display for SchemaLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.setting())
    }
}

/// XSD schema error with source location
///
/// `#[non_exhaustive]`: a `match` needs a wildcard arm; later variants are
/// additive.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum SchemaError {
    /// XML parsing error from quick-xml
    #[error("XML parse error{}: {message}", location_str(.location))]
    XmlError {
        message: String,
        location: Option<SourceLocation>,
    },

    /// Structural error (invalid child element, wrong attributes, etc.)
    #[error("Schema structural error{}: {message} (constraint: {constraint})", location_str(.location))]
    StructuralError {
        constraint: &'static str,
        message: String,
        location: Option<SourceLocation>,
    },

    /// Namespace error (undefined prefix, invalid QName, etc.)
    #[error("Namespace error{}: {message}", location_str(.location))]
    NamespaceError {
        message: String,
        location: Option<SourceLocation>,
    },

    /// Feature gate error (XSD 1.1 feature used in 1.0 mode)
    #[error("Feature not supported{}: {message}", location_str(.location))]
    FeatureError {
        message: String,
        location: Option<SourceLocation>,
    },

    /// Schema resolution error (include/import failed)
    #[error("Schema resolution error: {message}")]
    ResolutionError { message: String },

    /// I/O error (file not found, permission denied, etc.)
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// Internal error (should not happen, indicates a bug)
    #[error("Internal error: {0}")]
    Internal(String),

    /// A schema broke one of the crate's limits — the bounds that keep the
    /// schema-time passes inside a thread's stack (see [`SchemaLimit`]). The
    /// schema is refused; it is not invalid by the specification, but too
    /// deep (or too large) for this processor as configured.
    ///
    /// `Display`: `Schema limit ParserConfig::max_depth exceeded at
    /// main.xsd:1:2177: element depth 129, the limit is 128`.
    #[error("Schema limit {limit} exceeded{}: {message}, the limit is {max}", location_str(.location))]
    LimitExceeded {
        /// The limit.
        limit: SchemaLimit,
        /// Its configured maximum.
        max: usize,
        /// What was found (greater than `max`).
        found: usize,
        /// What broke it, e.g. "element depth 129".
        message: String,
        /// Where.
        location: Option<SourceLocation>,
    },
}

impl SchemaError {
    /// Create a structural error with constraint ID
    pub fn structural(
        constraint: &'static str,
        message: impl Into<String>,
        location: Option<SourceLocation>,
    ) -> Self {
        SchemaError::StructuralError {
            constraint,
            message: message.into(),
            location,
        }
    }

    /// Create a limit error.
    pub(crate) fn limit(
        limit: SchemaLimit,
        max: usize,
        found: usize,
        message: impl Into<String>,
        location: Option<SourceLocation>,
    ) -> Self {
        SchemaError::LimitExceeded {
            limit,
            max,
            found,
            message: message.into(),
            location,
        }
    }

    /// Create a namespace error
    pub fn namespace(message: impl Into<String>, location: Option<SourceLocation>) -> Self {
        SchemaError::NamespaceError {
            message: message.into(),
            location,
        }
    }

    /// Create a feature gate error
    pub fn feature(message: impl Into<String>, location: Option<SourceLocation>) -> Self {
        SchemaError::FeatureError {
            message: message.into(),
            location,
        }
    }

    /// Create a resolution error
    pub fn resolution(message: impl Into<String>) -> Self {
        SchemaError::ResolutionError {
            message: message.into(),
        }
    }

    /// Create an XML parse error
    pub fn xml(message: impl Into<String>, location: Option<SourceLocation>) -> Self {
        SchemaError::XmlError {
            message: message.into(),
            location,
        }
    }

    /// Create an internal error
    pub fn internal(message: impl Into<String>) -> Self {
        SchemaError::Internal(message.into())
    }

    /// Add source location to error if it doesn't already have one
    pub fn with_location(self, location: SourceLocation) -> Self {
        match self {
            SchemaError::XmlError {
                message,
                location: None,
            } => SchemaError::XmlError {
                message,
                location: Some(location),
            },
            SchemaError::StructuralError {
                constraint,
                message,
                location: None,
            } => SchemaError::StructuralError {
                constraint,
                message,
                location: Some(location),
            },
            SchemaError::NamespaceError {
                message,
                location: None,
            } => SchemaError::NamespaceError {
                message,
                location: Some(location),
            },
            SchemaError::FeatureError {
                message,
                location: None,
            } => SchemaError::FeatureError {
                message,
                location: Some(location),
            },
            SchemaError::LimitExceeded {
                limit,
                max,
                found,
                message,
                location: None,
            } => SchemaError::LimitExceeded {
                limit,
                max,
                found,
                message,
                location: Some(location),
            },
            // Already has location or doesn't support location - return unchanged
            other => other,
        }
    }

    /// The source location the error carries, if any — whatever its variant,
    /// so that a caller need not match the variants (the enum is
    /// `#[non_exhaustive]`).
    pub fn location(&self) -> Option<&SourceLocation> {
        match self {
            SchemaError::XmlError { location, .. }
            | SchemaError::StructuralError { location, .. }
            | SchemaError::NamespaceError { location, .. }
            | SchemaError::FeatureError { location, .. }
            | SchemaError::LimitExceeded { location, .. } => location.as_ref(),
            SchemaError::ResolutionError { .. }
            | SchemaError::IoError(_)
            | SchemaError::Internal(_) => None,
        }
    }

    /// A copy of this error (`std::io::Error` is not `Clone`: an I/O error
    /// is copied as its kind and message).
    pub(crate) fn duplicate(&self) -> SchemaError {
        match self {
            SchemaError::XmlError { message, location } => SchemaError::XmlError {
                message: message.clone(),
                location: location.clone(),
            },
            SchemaError::StructuralError {
                constraint,
                message,
                location,
            } => SchemaError::StructuralError {
                constraint,
                message: message.clone(),
                location: location.clone(),
            },
            SchemaError::NamespaceError { message, location } => SchemaError::NamespaceError {
                message: message.clone(),
                location: location.clone(),
            },
            SchemaError::FeatureError { message, location } => SchemaError::FeatureError {
                message: message.clone(),
                location: location.clone(),
            },
            SchemaError::ResolutionError { message } => SchemaError::ResolutionError {
                message: message.clone(),
            },
            SchemaError::IoError(e) => {
                SchemaError::IoError(std::io::Error::new(e.kind(), e.to_string()))
            }
            SchemaError::Internal(message) => SchemaError::Internal(message.clone()),
            SchemaError::LimitExceeded {
                limit,
                max,
                found,
                message,
                location,
            } => SchemaError::LimitExceeded {
                limit: *limit,
                max: *max,
                found: *found,
                message: message.clone(),
                location: location.clone(),
            },
        }
    }

    /// Returns `true` for errors that indicate the schema *content* is invalid
    /// (structural, namespace, XML parse, feature-gate errors).
    ///
    /// Returns `false` for resolution/IO errors, which mean the schema could
    /// not be *located* — these are non-fatal during import processing because
    /// `xs:import` schema locations are hints, not requirements.
    pub fn is_schema_content_error(&self) -> bool {
        matches!(
            self,
            SchemaError::StructuralError { .. }
                | SchemaError::NamespaceError { .. }
                | SchemaError::XmlError { .. }
                | SchemaError::FeatureError { .. }
                | SchemaError::LimitExceeded { .. }
        )
    }
}

/// Conversion from quick-xml errors
impl From<quick_xml::Error> for SchemaError {
    fn from(err: quick_xml::Error) -> Self {
        SchemaError::XmlError {
            message: err.to_string(),
            location: None,
        }
    }
}

/// Helper function for formatting optional location
fn location_str(loc: &Option<SourceLocation>) -> String {
    match loc {
        Some(l) => format!(" at {}", l),
        None => String::new(),
    }
}
