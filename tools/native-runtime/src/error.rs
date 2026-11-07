//! XPath faults are distinct from failures of the processing machinery.

/// Namespace of every fault supported here.
pub const XQT_ERRORS: &str = "http://www.w3.org/2005/xqt-errors";

/// A typed XPath error QName, independent of a future expression/source location.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Fault {
    /// Invalid lexical value for a cast.
    Forg0001,
    /// Invalid effective boolean value.
    Forg0006,
    /// Required dynamic focus is absent.
    Xpdy0002,
    /// Axis/path operand is not a node.
    Xpty0020,
    /// A path returned both nodes and atomic values.
    Xpty0018,
    /// Left path operand contains an atomic value.
    Xpty0019,
    /// A lazy binding depends on itself.
    Xtde0640,
    /// Declared select binding type/cardinality conversion failed.
    Xtte0570,
    /// Non-finite numeric cannot be cast to decimal/integer.
    Foca0002,
    /// Division by zero for decimal/integer or integer division.
    Foar0001,
    /// Invalid binary integer division (NaN or infinite dividend).
    Foar0002,
    /// Incompatible atomic types.
    Xpty0004,
}
impl Fault {
    /// Local part of the error QName; namespace is [`XQT_ERRORS`].
    pub fn local_name(self) -> &'static str {
        match self {
            Self::Forg0001 => "FORG0001",
            Self::Forg0006 => "FORG0006",
            Self::Xpdy0002 => "XPDY0002",
            Self::Xpty0020 => "XPTY0020",
            Self::Xpty0018 => "XPTY0018",
            Self::Xpty0019 => "XPTY0019",
            Self::Xtde0640 => "XTDE0640",
            Self::Xtte0570 => "XTTE0570",
            Self::Foca0002 => "FOCA0002",
            Self::Foar0001 => "FOAR0001",
            Self::Foar0002 => "FOAR0002",
            Self::Xpty0004 => "XPTY0004",
        }
    }
}

/// Explicit resource refusals, never an XPath false/zero/failed cast.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Resource {
    /// Invalid or platform-unsafe caller limits.
    InvalidLimits,
    /// Input string or lexical input is too large.
    InputBytes,
    /// Cumulative string allocation allowance.
    StringBytes,
    /// Cumulative rendering allowance.
    OutputBytes,
    /// Live value count or conservative live storage slots.
    LiveValues,
    /// Runtime work exhausted (numeric fuel is separately reserved).
    Work,
    /// Checked size arithmetic failed.
    SizeOverflow,
    /// Exact kernel resource refusal, retaining its original cause.
    Numeric(einvoice_numeric::ResourceCause),
    /// Expression preparation/execution dimension.
    Machine(&'static str),
    /// Immutable source query refused its cumulative budget.
    SourceQuery,
}

/// Processing failures cannot be caught as XPath cast failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Failure {
    /// Explicit resource allowance exhausted.
    Resource(Resource),
    /// Requested feature is not implemented by this layer.
    Capability(&'static str),
    /// Values belong to another caller scope.
    ForeignScope,
    /// Broken implementation invariant.
    Internal(&'static str),
}

/// Result channel for this layer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// An XPath dynamic/type fault.
    XPath(Fault),
    /// A fatal processing failure.
    Fatal(Failure),
}
impl From<einvoice_numeric::Error> for Error {
    fn from(error: einvoice_numeric::Error) -> Self {
        use einvoice_numeric::Error as N;
        match error {
            N::InvalidLexical => Self::XPath(Fault::Forg0001),
            N::DivideByZero => Self::XPath(Fault::Foar0001),
            N::Resource(cause) => resource(Resource::Numeric(cause)),
            N::InexactRendering => Self::Fatal(Failure::Internal("inexact exact rendering")),
        }
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::XPath(fault) => write!(f, "Q{{{XQT_ERRORS}}}{}", fault.local_name()),
            Self::Fatal(failure) => write!(f, "atomic processing failure: {failure:?}"),
        }
    }
}
impl std::error::Error for Error {}
/// Checked operation result.
pub type Result<T> = std::result::Result<T, Error>;
pub(crate) fn resource(cause: Resource) -> Error {
    Error::Fatal(Failure::Resource(cause))
}
pub(crate) fn size_overflow() -> Error {
    resource(Resource::SizeOverflow)
}
