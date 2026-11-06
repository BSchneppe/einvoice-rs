#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]

mod budget;
mod value;

pub use budget::{Budget, Limits};
pub use value::Decimal;

/// Explicit rounding at a caller-selected decimal scale.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoundingMode {
    /// Nearest; exact halfway cases increase the magnitude.
    HalfAwayFromZero,
    /// Nearest; exact halfway cases choose the value toward positive infinity.
    HalfTowardPositiveInfinity,
    /// Nearest; exact halfway cases choose an even retained coefficient.
    HalfEven,
    /// Nearest; exact halfway cases decrease the magnitude.
    HalfTowardZero,
    /// Discard the fractional quotient, toward zero.
    TowardZero,
}

/// A resource check that failed. These must remain processing errors at call sites.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceCause {
    /// Invalid or platform-unsafe limit configuration.
    InvalidLimits,
    /// Lexical input exceeds the input byte allowance.
    InputBytes,
    /// A coefficient or conservative temporary estimate exceeds its bit allowance.
    CoefficientBits,
    /// A lexical, requested, intermediate, or normalized scale exceeds its allowance.
    Scale,
    /// A rendering or its conservative scratch estimate exceeds its byte allowance.
    OutputBytes,
    /// Cumulative abstract work is exhausted.
    Work,
    /// Checked size or scale arithmetic cannot represent an estimate.
    SizeOverflow,
}

/// Numeric failures; invalid text never becomes zero and division has no implicit context.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// Text does not satisfy the requested integer/decimal lexical grammar.
    InvalidLexical,
    /// Divisor is zero.
    DivideByZero,
    /// Fixed-scale output would require rounding.
    InexactRendering,
    /// Explicit resource policy refused the operation.
    Resource(ResourceCause),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidLexical => f.write_str("invalid numeric lexical input"),
            Self::DivideByZero => f.write_str("division by zero"),
            Self::InexactRendering => {
                f.write_str("fixed-scale rendering requires explicit rounding")
            }
            Self::Resource(cause) => write!(f, "numeric resource limit: {cause:?}"),
        }
    }
}

impl std::error::Error for Error {}

/// Result of a checked numeric operation.
pub type Result<T> = std::result::Result<T, Error>;
