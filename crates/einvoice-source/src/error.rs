use crate::SourceLocation;
use std::fmt;

/// A logical budget refusal, never an invoice validity result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resource {
    /// Stable name of the exhausted dimension.
    pub dimension: &'static str,
    /// Configured maximum, or None for the pinned parser's opaque limit refusal.
    /// None is error metadata, never an unlimited construction configuration.
    pub limit: Option<usize>,
}

/// Construction failures, separated by their actual meaning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceError {
    /// Invalid finite configuration (including arithmetic/platform overflow).
    InvalidLimits(&'static str),
    /// Configured logical resource cap or fallible reservation failure.
    Resource(Resource),
    /// Cooperative cancellation observed at a construction checkpoint.
    Cancelled,
    /// XML syntax, including invalid non-lossy source encoding.
    Syntax {
        /// Parser/decoder detail; not a stable machine code.
        message: String,
        /// Original source position, absent when decoding/header mapping is unavailable.
        location: Option<SourceLocation>,
    },
    /// DTD processing is stopped before expansion; no resolver exists.
    UnsupportedDtd,
    /// Encoding outside the supported, strictly matched labels/BOM combinations.
    UnsupportedEncoding(String),
    /// XML 1.1 is not normalized with XML 1.0 rules.
    UnsupportedVersion(String),
    /// Adapter metadata failed its lexical/semantic invariant. Not invalid XML.
    Alignment(&'static str),
    /// A caller supplied output writer failed.
    Output,
}
impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimits(s) => write!(f, "invalid source limit: {s}"),
            Self::Resource(r) => write!(
                f,
                "source resource {} exhausted (limit {:?})",
                r.dimension, r.limit
            ),
            Self::Cancelled => f.write_str("source construction cancelled"),
            Self::Syntax { message, location } => write!(f, "XML syntax: {message} ({location:?})"),
            Self::UnsupportedDtd => f.write_str("DTD processing unsupported"),
            Self::UnsupportedEncoding(s) => write!(f, "unsupported XML encoding: {s}"),
            Self::UnsupportedVersion(s) => write!(f, "unsupported XML version: {s}"),
            Self::Alignment(s) => write!(f, "source engine alignment: {s}"),
            Self::Output => f.write_str("source output writer failed"),
        }
    }
}
impl std::error::Error for SourceError {}
pub(crate) type Result<T> = std::result::Result<T, SourceError>;
pub(crate) fn syntax(message: impl Into<String>) -> SourceError {
    SourceError::Syntax {
        message: message.into(),
        location: None,
    }
}
pub(crate) fn limit(dimension: &'static str, limit: usize) -> SourceError {
    SourceError::Resource(Resource {
        dimension,
        limit: Some(limit),
    })
}
