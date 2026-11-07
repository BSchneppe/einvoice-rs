use crate::budget::Lease;
use einvoice_numeric::Decimal;
use std::rc::Rc;

/// Supported primitive tags; Integer and Decimal remain distinct.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AtomicType {
    /// Arbitrary integral exact value.
    Integer,
    /// Arbitrary exact decimal, even when integral.
    Decimal,
    /// IEEE binary32.
    Float,
    /// IEEE binary64.
    Double,
    /// Boolean value.
    Boolean,
    /// Typed UTF-8 string.
    String,
    /// Untyped atomic UTF-8 value, without node semantics.
    UntypedAtomic,
}
impl AtomicType {
    /// XML Schema local name (namespace `http://www.w3.org/2001/XMLSchema`).
    pub fn local_name(self) -> &'static str {
        match self {
            Self::Integer => "integer",
            Self::Decimal => "decimal",
            Self::Float => "float",
            Self::Double => "double",
            Self::Boolean => "boolean",
            Self::String => "string",
            Self::UntypedAtomic => "untypedAtomic",
        }
    }
}

/// Integral invariant is private, with no public bypass constructor.
#[derive(Clone, Debug)]
pub(crate) struct Integral(pub Decimal);
#[derive(Clone, Debug)]
pub(crate) enum Value {
    Integer(Integral),
    Decimal(Decimal),
    Float(f32),
    Double(f64),
    Boolean(bool),
    String(Rc<str>),
    UntypedAtomic(Rc<str>),
}
impl Value {
    pub fn kind(&self) -> AtomicType {
        match self {
            Self::Integer(_) => AtomicType::Integer,
            Self::Decimal(_) => AtomicType::Decimal,
            Self::Float(_) => AtomicType::Float,
            Self::Double(_) => AtomicType::Double,
            Self::Boolean(_) => AtomicType::Boolean,
            Self::String(_) => AtomicType::String,
            Self::UntypedAtomic(_) => AtomicType::UntypedAtomic,
        }
    }
    pub fn exact(&self) -> Option<&Decimal> {
        match self {
            Self::Integer(i) => Some(&i.0),
            Self::Decimal(d) => Some(d),
            _ => None,
        }
    }
}

/// A scope-owned atom. No Clone, Eq, Ord, Hash or Display can bypass metering.
pub struct Atomic {
    pub(crate) value: Value,
    pub(crate) lease: Lease,
}
impl Atomic {
    /// Its primitive type, independently of numeric integrality.
    pub fn atomic_type(&self) -> AtomicType {
        self.value.kind()
    }
    /// Scalar binary32 observation; preserves raw zero sign and NaN payload.
    pub fn float_bits(&self) -> Option<u32> {
        if let Value::Float(v) = self.value {
            Some(v.to_bits())
        } else {
            None
        }
    }
    /// Scalar binary64 observation; preserves raw zero sign and NaN payload.
    pub fn double_bits(&self) -> Option<u64> {
        if let Value::Double(v) = self.value {
            Some(v.to_bits())
        } else {
            None
        }
    }
    /// Boolean observation; this is not effective-boolean-value conversion.
    pub fn boolean(&self) -> Option<bool> {
        if let Value::Boolean(v) = self.value {
            Some(v)
        } else {
            None
        }
    }
}

/// Metered rendered text; retaining it consumes a live slot until dropped.
pub struct Text {
    pub(crate) text: String,
    pub(crate) _lease: Lease,
}
impl Text {
    /// Borrow already-accounted UTF-8; allocating copies is the caller's responsibility.
    pub fn as_str(&self) -> &str {
        &self.text
    }
}

impl std::fmt::Debug for Atomic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Atomic")
            .field("type", &self.atomic_type())
            .finish_non_exhaustive()
    }
}
impl std::fmt::Debug for Text {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Text")
            .field("bytes", &self.text.len())
            .finish_non_exhaustive()
    }
}
