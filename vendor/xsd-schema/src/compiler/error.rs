//! Error types for NFA compilation
//!
//! All errors include source locations when available for developer-friendly messages.

use crate::parser::location::SourceRef;
use thiserror::Error;

/// Result type for NFA compilation operations
pub type NfaCompileResult<T> = Result<T, NfaCompileError>;

/// Errors that can occur during NFA compilation
#[derive(Error, Debug, Clone)]
pub enum NfaCompileError {
    /// Group reference could not be resolved
    #[error("unresolved group reference: {name}")]
    UnresolvedGroupRef {
        name: String,
        location: Option<SourceRef>,
    },

    /// Element reference could not be resolved
    #[error("unresolved element reference: {name}")]
    UnresolvedElementRef {
        name: String,
        location: Option<SourceRef>,
    },

    /// Invalid occurrence constraint (minOccurs > maxOccurs)
    #[error("invalid occurrence: minOccurs ({min}) > maxOccurs ({max})")]
    InvalidOccurrence {
        min: u32,
        max: u32,
        location: Option<SourceRef>,
    },

    /// All-group contains invalid content (XSD 1.0 restriction)
    #[error("xs:all group can only contain element particles in XSD 1.0")]
    InvalidAllGroupContent { location: Option<SourceRef> },

    /// All-group has invalid occurrence constraints (XSD 1.0 restriction)
    #[error("invalid xs:all occurrence constraint: {reason}")]
    InvalidAllGroupOccurs {
        reason: String,
        location: Option<SourceRef>,
    },

    /// Recursion limit exceeded during compilation
    #[error("recursion limit exceeded while compiling content model")]
    RecursionLimitExceeded { location: Option<SourceRef> },

    /// Empty content model (no particles to compile)
    #[error("empty content model")]
    EmptyContentModel { location: Option<SourceRef> },
}

impl NfaCompileError {
    /// Create an unresolved group reference error
    pub fn unresolved_group(name: impl Into<String>, location: Option<SourceRef>) -> Self {
        NfaCompileError::UnresolvedGroupRef {
            name: name.into(),
            location,
        }
    }

    /// Create an unresolved element reference error
    pub fn unresolved_element(name: impl Into<String>, location: Option<SourceRef>) -> Self {
        NfaCompileError::UnresolvedElementRef {
            name: name.into(),
            location,
        }
    }

    /// Create an invalid occurrence error
    pub fn invalid_occurrence(min: u32, max: u32, location: Option<SourceRef>) -> Self {
        NfaCompileError::InvalidOccurrence { min, max, location }
    }

    /// Create an invalid all-group content error
    pub fn invalid_all_group(location: Option<SourceRef>) -> Self {
        NfaCompileError::InvalidAllGroupContent { location }
    }

    /// Create an invalid all-group occurrence error
    pub fn invalid_all_group_occurs(
        reason: impl Into<String>,
        location: Option<SourceRef>,
    ) -> Self {
        NfaCompileError::InvalidAllGroupOccurs {
            reason: reason.into(),
            location,
        }
    }

    /// Create a recursion limit exceeded error
    pub fn recursion_exceeded(location: Option<SourceRef>) -> Self {
        NfaCompileError::RecursionLimitExceeded { location }
    }

    /// Create an empty content model error
    pub fn empty_content(location: Option<SourceRef>) -> Self {
        NfaCompileError::EmptyContentModel { location }
    }

    /// Get the source location if available
    pub fn location(&self) -> Option<&SourceRef> {
        match self {
            NfaCompileError::UnresolvedGroupRef { location, .. } => location.as_ref(),
            NfaCompileError::UnresolvedElementRef { location, .. } => location.as_ref(),
            NfaCompileError::InvalidOccurrence { location, .. } => location.as_ref(),
            NfaCompileError::InvalidAllGroupContent { location } => location.as_ref(),
            NfaCompileError::InvalidAllGroupOccurs { location, .. } => location.as_ref(),
            NfaCompileError::RecursionLimitExceeded { location } => location.as_ref(),
            NfaCompileError::EmptyContentModel { location } => location.as_ref(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_messages() {
        let err = NfaCompileError::unresolved_group("myGroup", None);
        assert!(err.to_string().contains("myGroup"));

        let err = NfaCompileError::invalid_occurrence(5, 3, None);
        assert!(err.to_string().contains("5"));
        assert!(err.to_string().contains("3"));
    }

    #[test]
    fn test_location_accessor() {
        let err = NfaCompileError::unresolved_element("elem", None);
        assert!(err.location().is_none());
    }
}
