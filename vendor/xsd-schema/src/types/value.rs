//! XSD value types for typed atomic values
//!
//! This module provides the `XmlValue` type for representing typed XSD values,
//! parsed and formatted by the datatype layer of [`crate::types`].
//!
//! ## Design
//!
//! - `XmlValue` is a typed container for XSD values
//! - `XmlAtomicValue` holds the actual parsed value
//! - QName and NOTATION values are a [`QNameValue`]: the namespace-resolved
//!   [`QualifiedName`] together with its lexical form
//! - List values hold one `XmlValue` per member, each with its own type
//! - The `PartialEq` / `PartialOrd` of the date/time and duration structs are
//!   the XSD equality and order, computed exactly: a date/time value without a
//!   time zone offset is incomparable with one that has an offset unless 14
//!   hours either way give the same order, and durations are ordered by the
//!   four reference dateTimes (`P1M` and `P30D` are incomparable)
//!
//! ## Implementation limits (Datatypes 1.1 §5.4)
//!
//! The date/time and duration values hold a year in `i32` (dateTime, date,
//! gYearMonth, gYear), each duration field but the seconds in `u32`, and the
//! seconds in a `rust_decimal::Decimal` (a 96-bit mantissa, at most 28
//! fraction digits). A literal whose year, duration field or whole seconds
//! exceed these is refused as not valid. A seconds number with more fraction
//! digits than the `Decimal` holds is valid: its typed value keeps the
//! fraction truncated toward zero (never rounded up to the next second); the
//! validator compares the literal as written for the facets, value
//! constraints and identity constraints, and reports the warning
//! `xs-seconds-capacity`. The order and equality of the values never overflow
//! (whole seconds in `i128`, the fraction apart).

use std::fmt;
use std::sync::Arc;

use num_bigint::BigInt;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;

use super::{PrimitiveTypeCode, XmlTypeCode};
use crate::ids::SimpleTypeKey;
use crate::namespace::qname::QualifiedName;
use crate::namespace::table::NameTable;

/// A typed XSD value with type information.
///
/// This is the primary value type for XPath2/XQuery operations.
/// It carries both the value and its type information.
#[derive(Debug, Clone, PartialEq)]
pub struct XmlValue {
    /// The type code identifying the value's type
    pub type_code: XmlTypeCode,
    /// Optional reference to a schema-defined type
    pub schema_type: Option<SimpleTypeKey>,
    /// The actual value
    pub value: XmlValueKind,
}

impl XmlValue {
    /// Create a new XmlValue with the given type code and value
    pub fn new(type_code: XmlTypeCode, value: XmlValueKind) -> Self {
        Self {
            type_code,
            schema_type: None,
            value,
        }
    }

    /// Create a new XmlValue with schema type reference
    pub fn with_schema_type(
        type_code: XmlTypeCode,
        schema_type: SimpleTypeKey,
        value: XmlValueKind,
    ) -> Self {
        Self {
            type_code,
            schema_type: Some(schema_type),
            value,
        }
    }

    /// Create an untyped atomic value
    pub fn untyped(s: impl Into<String>) -> Self {
        Self {
            type_code: XmlTypeCode::UntypedAtomic,
            schema_type: None,
            value: XmlValueKind::UntypedAtomic(s.into()),
        }
    }

    /// Create a string value
    pub fn string(s: impl Into<String>) -> Self {
        Self {
            type_code: XmlTypeCode::String,
            schema_type: None,
            value: XmlValueKind::Atomic(XmlAtomicValue::String(s.into())),
        }
    }

    /// Create a boolean value
    pub fn boolean(b: bool) -> Self {
        Self {
            type_code: XmlTypeCode::Boolean,
            schema_type: None,
            value: XmlValueKind::Atomic(XmlAtomicValue::Boolean(b)),
        }
    }

    /// Create a decimal value
    pub fn decimal(d: Decimal) -> Self {
        Self {
            type_code: XmlTypeCode::Decimal,
            schema_type: None,
            value: XmlValueKind::Atomic(XmlAtomicValue::Decimal(d)),
        }
    }

    /// Create an integer value
    pub fn integer(i: BigInt) -> Self {
        Self {
            type_code: XmlTypeCode::Integer,
            schema_type: None,
            value: XmlValueKind::Atomic(XmlAtomicValue::Integer(i)),
        }
    }

    /// Create a float value
    pub fn float(f: f32) -> Self {
        Self {
            type_code: XmlTypeCode::Float,
            schema_type: None,
            value: XmlValueKind::Atomic(XmlAtomicValue::Float(f)),
        }
    }

    /// Create a double value
    pub fn double(d: f64) -> Self {
        Self {
            type_code: XmlTypeCode::Double,
            schema_type: None,
            value: XmlValueKind::Atomic(XmlAtomicValue::Double(d)),
        }
    }

    /// Check if this is an atomic value
    pub fn is_atomic(&self) -> bool {
        matches!(
            self.value,
            XmlValueKind::Atomic(_) | XmlValueKind::UntypedAtomic(_)
        )
    }

    /// Check if this is a list value
    pub fn is_list(&self) -> bool {
        matches!(self.value, XmlValueKind::List { .. })
    }

    /// Check if this is a union value
    pub fn is_union(&self) -> bool {
        matches!(self.value, XmlValueKind::Union(_))
    }

    /// Check if this is an untyped atomic value
    pub fn is_untyped(&self) -> bool {
        matches!(self.value, XmlValueKind::UntypedAtomic(_))
    }

    /// Get the primitive type code for this value
    pub fn primitive_type(&self) -> Option<PrimitiveTypeCode> {
        PrimitiveTypeCode::from_type_code(self.type_code)
    }

    /// Get the string value (canonical representation)
    pub fn to_string_value(&self) -> String {
        match &self.value {
            XmlValueKind::Atomic(atom) => atom.to_string(),
            XmlValueKind::List { items, .. } => items
                .iter()
                .map(XmlValue::to_string_value)
                .collect::<Vec<_>>()
                .join(" "),
            XmlValueKind::Union(inner) => inner.to_string_value(),
            XmlValueKind::UntypedAtomic(s) => s.clone(),
        }
    }

    /// Try to get as boolean
    pub fn as_boolean(&self) -> Option<bool> {
        match &self.value {
            XmlValueKind::Atomic(XmlAtomicValue::Boolean(b)) => Some(*b),
            _ => None,
        }
    }

    /// Try to get as string
    pub fn as_string(&self) -> Option<&str> {
        match &self.value {
            XmlValueKind::Atomic(XmlAtomicValue::String(s)) => Some(s),
            XmlValueKind::UntypedAtomic(s) => Some(s),
            _ => None,
        }
    }

    /// Try to get as decimal
    ///
    /// `None` also for an `xs:decimal` value beyond what `rust_decimal`
    /// holds exactly — one that is not `m / 10^s` with an integer
    /// `|m| ≤ 2^96 − 1` (79228162514264337593543950335) and `s ≤ 28`, such as
    /// a magnitude above that bound or more than 28 fraction digits. Such a
    /// value is valid; its typed value is its canonical representation
    /// (Datatypes §3.3.3.2), an [`XmlAtomicValue::String`] with the type code
    /// [`XmlTypeCode::Decimal`], and [`XmlValue::to_string_value`] returns
    /// that form. It is never rounded to a nearby `Decimal`.
    pub fn as_decimal(&self) -> Option<Decimal> {
        match &self.value {
            XmlValueKind::Atomic(XmlAtomicValue::Decimal(d)) => Some(*d),
            XmlValueKind::Atomic(XmlAtomicValue::Integer(i)) => {
                // Try to convert BigInt to Decimal
                i.to_string().parse().ok()
            }
            _ => None,
        }
    }

    /// Try to get as integer
    pub fn as_integer(&self) -> Option<&BigInt> {
        match &self.value {
            XmlValueKind::Atomic(XmlAtomicValue::Integer(i)) => Some(i),
            _ => None,
        }
    }

    /// Try to get as double
    pub fn as_double(&self) -> Option<f64> {
        match &self.value {
            XmlValueKind::Atomic(XmlAtomicValue::Double(d)) => Some(*d),
            XmlValueKind::Atomic(XmlAtomicValue::Float(f)) => Some(*f as f64),
            XmlValueKind::Atomic(XmlAtomicValue::Decimal(d)) => d.to_string().parse().ok(),
            XmlValueKind::Atomic(XmlAtomicValue::Integer(i)) => i.to_string().parse().ok(),
            // An `xs:decimal` carried in lexical form (see `as_decimal`): the
            // nearest double, as casting `xs:decimal` to `xs:double` gives it.
            XmlValueKind::Atomic(XmlAtomicValue::String(s))
                if self.type_code == XmlTypeCode::Decimal =>
            {
                crate::types::exact_decimal::ExactDecimal::parse(s).map(|d| d.to_f64())
            }
            _ => None,
        }
    }

    /// Try to get as QName: the expanded name of an
    /// [`XmlAtomicValue::QName`] (see [`QNameValue::name`]).
    pub fn as_qname(&self) -> Option<&QualifiedName> {
        match &self.value {
            XmlValueKind::Atomic(XmlAtomicValue::QName(qn)) => Some(qn.name()),
            _ => None,
        }
    }

    /// Convert this `XmlValue` to an `XPathValue`, for instance as `$value`
    /// in assertion evaluation.
    ///
    /// - **Atomic/UntypedAtomic** → single `XPathValue::Item`
    /// - **List** → `XPathValue::Sequence` of its members, each with the type
    ///   and `schema_type` it was validated with
    /// - **Union** → recursively converts the member value (so a union whose
    ///   chosen member type is a list type gives that list's members)
    #[cfg(feature = "xsd11")]
    pub fn to_xpath_value<N: crate::xpath::DomNavigator>(&self) -> crate::xpath::XPathValue<N> {
        use crate::xpath::iterator::XmlItem;
        use crate::xpath::XPathValue;

        match &self.value {
            XmlValueKind::Atomic(_) | XmlValueKind::UntypedAtomic(_) => {
                XPathValue::from_atomic(self.clone())
            }
            XmlValueKind::List { items, .. } => {
                XPathValue::from_sequence(items.iter().cloned().map(XmlItem::Atomic).collect())
            }
            XmlValueKind::Union(inner) => inner.to_xpath_value(),
        }
    }
}

/// Value kind discriminant for XmlValue
#[derive(Debug, Clone, PartialEq)]
pub enum XmlValueKind {
    /// A single atomic value
    Atomic(XmlAtomicValue),
    /// A list value (e.g., NMTOKENS): the typed value of a list type.
    List {
        /// The type code of the list's declared item type: the item type's
        /// built-in type code, or that of the built-in type it is derived
        /// from (`xs:integer` for a restriction of it). An item type that is
        /// a union has no single one: its code is `xs:anySimpleType`, the
        /// base type of every union. Each member carries its own type.
        item_type: XmlTypeCode,
        /// The members, in order: one atomic value each
        /// ([`XmlValueKind::Atomic`], or [`XmlValueKind::UntypedAtomic`]
        /// for an item type of `xs:anySimpleType`), with the `type_code` and
        /// `schema_type` of the type that validated it — the item type, or,
        /// when the item type is a union, the member type of the union that
        /// validated that member (XPath 2.0 §2.5.2). A member is never a
        /// list, and never wrapped in [`XmlValueKind::Union`].
        items: Vec<XmlValue>,
    },
    /// A union value (actual type determined at runtime)
    Union(Box<XmlValue>),
    /// An untyped atomic value (raw string)
    UntypedAtomic(String),
}

/// Atomic XSD value types
///
/// This enum holds the actual parsed values for atomic XSD types.
/// For complex types like date/time, we use structured representations.
#[derive(Debug, Clone, PartialEq)]
pub enum XmlAtomicValue {
    // String types
    /// xs:string and derived types
    String(String),

    // Boolean type
    /// xs:boolean
    Boolean(bool),

    // Numeric types
    /// xs:decimal, when `rust_decimal::Decimal` holds the value exactly: the
    /// value is `m / 10^s` for integers `m` and `s` with
    /// `|m| ≤ 2^96 − 1` (79228162514264337593543950335 — a 96-bit mantissa)
    /// and `0 ≤ s ≤ 28`. Every value that a `totalDigits` facet of 28
    /// accepts qualifies: Datatypes §4.3.11.3 makes it `i / 10^n` with
    /// `|i| < 10^28 < 2^96` and `0 ≤ n ≤ 28`. A count of significant digits
    /// does not decide it: `0.00000000000000000000000000001` (10^−29) and
    /// `100000000000000000000000000000` (10^29) have one each and do not
    /// qualify.
    ///
    /// The value space of `xs:decimal` is unbounded. A larger or more precise
    /// valid value is validated exactly — digits, bounds, enumeration and
    /// equality are decided on the exact value — but its typed value is its
    /// canonical representation (Datatypes §3.3.3.2: no sign for a positive
    /// value, no leading or trailing zeros, no decimal point for an integer):
    /// an [`XmlAtomicValue::String`] inside
    /// an [`XmlValue`] whose `type_code` is [`XmlTypeCode::Decimal`]. The
    /// validator reports such a value with a warning (code
    /// `xs-decimal-capacity`), and XPath arithmetic on it raises `FOAR0002`
    /// instead of computing with a rounded number.
    Decimal(Decimal),
    /// xs:integer and derived integer types
    Integer(BigInt),
    /// xs:float
    Float(f32),
    /// xs:double
    Double(f64),

    // Date/time types
    /// xs:dateTime
    DateTime(DateTimeValue),
    /// xs:date
    Date(DateValue),
    /// xs:time
    Time(TimeValue),
    /// xs:duration
    Duration(DurationValue),
    /// xs:gYearMonth
    GYearMonth(GYearMonthValue),
    /// xs:gYear
    GYear(GYearValue),
    /// xs:gMonthDay
    GMonthDay(GMonthDayValue),
    /// xs:gDay
    GDay(GDayValue),
    /// xs:gMonth
    GMonth(GMonthValue),
    /// xs:yearMonthDuration (XSD 1.1)
    YearMonthDuration(YearMonthDurationValue),
    /// xs:dayTimeDuration (XSD 1.1)
    DayTimeDuration(DayTimeDurationValue),

    // Binary types
    /// xs:hexBinary
    HexBinary(Vec<u8>),
    /// xs:base64Binary
    Base64Binary(Vec<u8>),

    // URI type
    /// xs:anyURI
    AnyUri(String),

    // QName types (namespace-resolved)
    /// xs:QName
    QName(QNameValue),
    /// xs:NOTATION
    Notation(QNameValue),
}

/// The value of an `xs:QName` or `xs:NOTATION`
/// ([`XmlAtomicValue::QName`], [`XmlAtomicValue::Notation`]): an expanded
/// name — a [`QualifiedName`] of interned ids — together with the string form
/// of the value, so that it converts to a string with no [`NameTable`] at
/// hand.
///
/// The string form is the one F&O §17.1.2 gives for casting to `xs:string`:
/// "if the qualified name has a prefix, then TV is the concatenation of the
/// prefix of SV, a single colon (:), and the local name of SV. otherwise TV is
/// the local-name." It is what `Display`, [`XmlValue::to_string_value`],
/// `fn:string` and every cast to a string give.
///
/// Equality and hashing follow [`QualifiedName`]: namespace URI and local
/// name, never the prefix (F&O §11.2.1 `op:QName-equal`: "The prefix parts of
/// $arg1 and $arg2, if any, are ignored"). The ids are only comparable with
/// ids of the same [`NameTable`].
///
/// Cloning is cheap (the value is shared).
#[derive(Clone)]
pub struct QNameValue(Arc<QNameParts>);

struct QNameParts {
    name: QualifiedName,
    /// `prefix:local`, or `local` when there is no prefix.
    lexical: Box<str>,
    /// The length of the prefix in `lexical`; 0 when there is none.
    prefix_len: usize,
}

impl QNameValue {
    /// The value of `name`, whose prefix and local name are resolved in
    /// `names` to build the string form. An empty prefix counts as no prefix.
    ///
    /// `names` must be the table `name`'s ids were interned in: with any other
    /// table the string form is wrong — another name's text, or, for an id the
    /// table does not hold, the empty string (a debug build asserts that both
    /// ids are held).
    pub fn new(name: QualifiedName, names: &NameTable) -> Self {
        debug_assert!(
            names.try_resolve_ref(name.local_name).is_some()
                && name
                    .prefix
                    .is_none_or(|p| names.try_resolve_ref(p).is_some()),
            "QNameValue::new: the name's ids are not in this name table"
        );
        let local = names.try_resolve_ref(name.local_name).unwrap_or("");
        let prefix = name
            .prefix
            .and_then(|p| names.try_resolve_ref(p))
            .unwrap_or("");
        let lexical: Box<str> = if prefix.is_empty() {
            local.into()
        } else {
            format!("{prefix}:{local}").into()
        };
        QNameValue(Arc::new(QNameParts {
            name,
            lexical,
            prefix_len: prefix.len(),
        }))
    }

    /// The expanded name: namespace URI, local name and prefix, as ids.
    pub fn name(&self) -> &QualifiedName {
        &self.0.name
    }

    /// The prefix, or `None` when the value has none.
    pub fn prefix(&self) -> Option<&str> {
        match self.0.prefix_len {
            0 => None,
            n => Some(&self.0.lexical[..n]),
        }
    }

    /// The local name.
    pub fn local_name(&self) -> &str {
        match self.0.prefix_len {
            0 => &self.0.lexical,
            n => &self.0.lexical[n + 1..],
        }
    }

    /// The string form: `prefix:local`, or `local` when there is no prefix
    /// (F&O §17.1.2).
    pub fn lexical(&self) -> &str {
        &self.0.lexical
    }
}

impl PartialEq for QNameValue {
    fn eq(&self, other: &Self) -> bool {
        self.0.name == other.0.name
    }
}

impl Eq for QNameValue {}

impl std::hash::Hash for QNameValue {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.name.hash(state);
    }
}

impl fmt::Display for QNameValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.lexical)
    }
}

impl fmt::Debug for QNameValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QNameValue")
            .field("lexical", &self.lexical())
            .field("name", self.name())
            .finish()
    }
}

impl XmlAtomicValue {
    /// Get the type code for this atomic value
    pub fn type_code(&self) -> XmlTypeCode {
        match self {
            Self::String(_) => XmlTypeCode::String,
            Self::Boolean(_) => XmlTypeCode::Boolean,
            Self::Decimal(_) => XmlTypeCode::Decimal,
            Self::Integer(_) => XmlTypeCode::Integer,
            Self::Float(_) => XmlTypeCode::Float,
            Self::Double(_) => XmlTypeCode::Double,
            Self::DateTime(_) => XmlTypeCode::DateTime,
            Self::Date(_) => XmlTypeCode::Date,
            Self::Time(_) => XmlTypeCode::Time,
            Self::Duration(_) => XmlTypeCode::Duration,
            Self::GYearMonth(_) => XmlTypeCode::GYearMonth,
            Self::GYear(_) => XmlTypeCode::GYear,
            Self::GMonthDay(_) => XmlTypeCode::GMonthDay,
            Self::GDay(_) => XmlTypeCode::GDay,
            Self::GMonth(_) => XmlTypeCode::GMonth,
            Self::YearMonthDuration(_) => XmlTypeCode::YearMonthDuration,
            Self::DayTimeDuration(_) => XmlTypeCode::DayTimeDuration,
            Self::HexBinary(_) => XmlTypeCode::HexBinary,
            Self::Base64Binary(_) => XmlTypeCode::Base64Binary,
            Self::AnyUri(_) => XmlTypeCode::AnyUri,
            Self::QName(_) => XmlTypeCode::QName,
            Self::Notation(_) => XmlTypeCode::Notation,
        }
    }

    /// Get the primitive type code for this atomic value
    pub fn primitive_type(&self) -> PrimitiveTypeCode {
        match self {
            Self::String(_) => PrimitiveTypeCode::String,
            Self::Boolean(_) => PrimitiveTypeCode::Boolean,
            Self::Decimal(_) | Self::Integer(_) => PrimitiveTypeCode::Decimal,
            Self::Float(_) => PrimitiveTypeCode::Float,
            Self::Double(_) => PrimitiveTypeCode::Double,
            Self::DateTime(_) => PrimitiveTypeCode::DateTime,
            Self::Date(_) => PrimitiveTypeCode::Date,
            Self::Time(_) => PrimitiveTypeCode::Time,
            Self::Duration(_) | Self::YearMonthDuration(_) | Self::DayTimeDuration(_) => {
                PrimitiveTypeCode::Duration
            }
            Self::GYearMonth(_) => PrimitiveTypeCode::GYearMonth,
            Self::GYear(_) => PrimitiveTypeCode::GYear,
            Self::GMonthDay(_) => PrimitiveTypeCode::GMonthDay,
            Self::GDay(_) => PrimitiveTypeCode::GDay,
            Self::GMonth(_) => PrimitiveTypeCode::GMonth,
            Self::HexBinary(_) => PrimitiveTypeCode::HexBinary,
            Self::Base64Binary(_) => PrimitiveTypeCode::Base64Binary,
            Self::AnyUri(_) => PrimitiveTypeCode::AnyUri,
            Self::QName(_) => PrimitiveTypeCode::QName,
            Self::Notation(_) => PrimitiveTypeCode::Notation,
        }
    }
}

impl fmt::Display for XmlAtomicValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::String(s) => write!(f, "{}", s),
            Self::Boolean(b) => write!(f, "{}", if *b { "true" } else { "false" }),
            Self::Decimal(d) => {
                // XSD canonical form: no trailing zeros for integers, and
                // zero is `0` — xs:decimal has no negative zero, though a
                // `rust_decimal` zero can carry a sign.
                if d.is_zero() {
                    f.write_str("0")
                } else if d.fract().is_zero() {
                    write!(f, "{}", d.trunc())
                } else {
                    write!(f, "{}", d.normalize())
                }
            }
            Self::Integer(i) => write!(f, "{}", i),
            Self::Float(v) => format_float(*v, f),
            Self::Double(v) => format_double(*v, f),
            Self::DateTime(dt) => write!(f, "{}", dt),
            Self::Date(d) => write!(f, "{}", d),
            Self::Time(t) => write!(f, "{}", t),
            Self::Duration(d) => write!(f, "{}", d),
            Self::GYearMonth(v) => write!(f, "{}", v),
            Self::GYear(v) => write!(f, "{}", v),
            Self::GMonthDay(v) => write!(f, "{}", v),
            Self::GDay(v) => write!(f, "{}", v),
            Self::GMonth(v) => write!(f, "{}", v),
            Self::YearMonthDuration(v) => write!(f, "{}", v),
            Self::DayTimeDuration(v) => write!(f, "{}", v),
            Self::HexBinary(bytes) => {
                write!(f, "{}", hex::encode_upper(bytes))
            }
            Self::Base64Binary(bytes) => {
                use base64::Engine;
                write!(
                    f,
                    "{}",
                    base64::engine::general_purpose::STANDARD.encode(bytes)
                )
            }
            Self::AnyUri(uri) => write!(f, "{}", uri),
            // F&O §17.1.2: `prefix:local`, or the local name.
            Self::QName(qn) | Self::Notation(qn) => fmt::Display::fmt(qn, f),
        }
    }
}

/// Format a scientific notation string to ensure the mantissa has a decimal point.
///
/// Rust's `{:E}` may produce `1E7` or `-1E7`; XPath 2.0 requires `1.0E7` or `-1.0E7`.
fn fix_scientific_notation(s: &str) -> String {
    // Find the position of 'E'
    if let Some(e_pos) = s.find('E') {
        let mantissa = &s[..e_pos];
        let exponent = &s[e_pos..]; // includes 'E'
        if !mantissa.contains('.') {
            format!("{}.0{}", mantissa, exponent)
        } else {
            s.to_string()
        }
    } else {
        s.to_string()
    }
}

/// Format float according to XSD canonical representation
fn format_float(v: f32, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    if v.is_nan() {
        write!(f, "NaN")
    } else if v.is_infinite() {
        if v.is_sign_positive() {
            write!(f, "INF")
        } else {
            write!(f, "-INF")
        }
    } else if v == 0.0 {
        // Per XPath 2.0: negative zero serializes as "-0", positive zero as "0"
        if v.is_sign_negative() {
            write!(f, "-0")
        } else {
            write!(f, "0")
        }
    } else if v.abs() >= 1e-6 && v.abs() < 1e6 {
        // Use regular notation for values in this range
        write!(f, "{}", v)
    } else {
        // Use scientific notation with guaranteed decimal point in mantissa
        let s = format!("{:E}", v);
        write!(f, "{}", fix_scientific_notation(&s))
    }
}

/// Format double according to XSD canonical representation
fn format_double(v: f64, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    if v.is_nan() {
        write!(f, "NaN")
    } else if v.is_infinite() {
        if v.is_sign_positive() {
            write!(f, "INF")
        } else {
            write!(f, "-INF")
        }
    } else if v == 0.0 {
        // Per XPath 2.0: negative zero serializes as "-0", positive zero as "0"
        if v.is_sign_negative() {
            write!(f, "-0")
        } else {
            write!(f, "0")
        }
    } else if v.abs() >= 1e-6 && v.abs() < 1e6 {
        // Use regular notation for values in this range
        write!(f, "{}", v)
    } else {
        // Use scientific notation with guaranteed decimal point in mantissa
        let s = format!("{:E}", v);
        write!(f, "{}", fix_scientific_notation(&s))
    }
}

// ============================================================================
// Date/Time Value Types
// ============================================================================

/// Format a year value according to XPath 2.0 rules.
///
/// Negative years must be formatted as sign + 4-digit year (e.g., -12 → "-0012").
/// Positive years use standard 4-digit zero-padded format.
fn format_year(year: i32, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    if year < 0 {
        // `unsigned_abs`: `-year` overflows for `i32::MIN`.
        write!(f, "-{:04}", year.unsigned_abs())
    } else {
        write!(f, "{:04}", year)
    }
}

/// xs:dateTime value
#[derive(Debug, Clone)]
pub struct DateTimeValue {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: Decimal,
    pub timezone: Option<TimezoneOffset>,
}

impl DateTimeValue {
    /// The order of the two values on the time line with a missing time
    /// zone offset read as `Z` — the building block of the XSD order, which
    /// [`PartialOrd`] gives.
    pub(crate) fn timeline_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        let key = |v: &Self| {
            time_line_seconds(
                day_number(v.year, v.month, v.day),
                v.hour,
                v.minute,
                v.second,
                v.timezone,
            )
        };
        Some(key(self).cmp(&key(other)))
    }

    /// The whole seconds of the value's position on the time line (the
    /// first part of the key [`timeline_cmp`](Self::timeline_cmp) orders),
    /// with the time zone offset `impute` (in minutes) for a value without
    /// one, else its own, else `Z`.
    pub(crate) fn time_line_whole(&self, impute: Option<i16>) -> i128 {
        time_line_seconds(
            day_number(self.year, self.month, self.day),
            self.hour,
            self.minute,
            self.second,
            self.timezone.or(impute.map(TimezoneOffset)),
        )
        .0
    }
}

impl fmt::Display for DateTimeValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        format_year(self.year, f)?;
        write!(
            f,
            "-{:02}-{:02}T{:02}:{:02}:",
            self.month, self.day, self.hour, self.minute
        )?;
        format_seconds(self.second, f)?;
        if let Some(tz) = &self.timezone {
            write!(f, "{}", tz)?;
        }
        Ok(())
    }
}

/// xs:date value
#[derive(Debug, Clone)]
pub struct DateValue {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub timezone: Option<TimezoneOffset>,
}

impl DateValue {
    /// The order of the two values on the time line with a missing time
    /// zone offset read as `Z` — the building block of the XSD order, which
    /// [`PartialOrd`] gives.
    pub(crate) fn timeline_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        let key = |v: &Self| {
            time_line_seconds(
                day_number(v.year, v.month, v.day),
                0,
                0,
                Decimal::ZERO,
                v.timezone,
            )
        };
        Some(key(self).cmp(&key(other)))
    }
}

impl fmt::Display for DateValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        format_year(self.year, f)?;
        write!(f, "-{:02}-{:02}", self.month, self.day)?;
        if let Some(tz) = &self.timezone {
            write!(f, "{}", tz)?;
        }
        Ok(())
    }
}

/// xs:time value
#[derive(Debug, Clone)]
pub struct TimeValue {
    pub hour: u8,
    pub minute: u8,
    pub second: Decimal,
    pub timezone: Option<TimezoneOffset>,
}

impl TimeValue {
    /// The order of the two values on the time line with a missing time
    /// zone offset read as `Z` — the building block of the XSD order, which
    /// [`PartialOrd`] gives.
    pub(crate) fn timeline_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        let key = |v: &Self| time_line_seconds(0, v.hour, v.minute, v.second, v.timezone);
        Some(key(self).cmp(&key(other)))
    }

    /// [`DateTimeValue::time_line_whole`] for a time of day.
    pub(crate) fn time_line_whole(&self, impute: Option<i16>) -> i128 {
        time_line_seconds(
            0,
            self.hour,
            self.minute,
            self.second,
            self.timezone.or(impute.map(TimezoneOffset)),
        )
        .0
    }
}

impl fmt::Display for TimeValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}:{:02}:", self.hour, self.minute)?;
        format_seconds(self.second, f)?;
        if let Some(tz) = &self.timezone {
            write!(f, "{}", tz)?;
        }
        Ok(())
    }
}

/// xs:duration value
#[derive(Debug, Clone)]
pub struct DurationValue {
    pub negative: bool,
    pub years: u32,
    pub months: u32,
    pub days: u32,
    pub hours: u32,
    pub minutes: u32,
    pub seconds: Decimal,
}

impl PartialOrd for DurationValue {
    /// The order of the duration value space (Datatypes 1.1 §3.3.6.1, XSD 1.0
    /// §3.2.6.2): by the four reference dateTimes; `None` for ·incomparable·
    /// values (`P1M` and `P30D`).
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        crate::types::value_order::duration_cmp(
            &crate::types::value_order::duration_value_parts(self),
            &crate::types::value_order::duration_value_parts(other),
        )
    }
}

/// Equality in the same order: the four reference sums agree (`PT1M` equals
/// `PT60S`, `P1D` equals `PT24H`, `P400Y` equals `P146097D`).
impl PartialEq for DurationValue {
    fn eq(&self, other: &Self) -> bool {
        self.partial_cmp(other) == Some(std::cmp::Ordering::Equal)
    }
}

impl fmt::Display for DurationValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Normalize year-month part
        // In `u64`: twelve times a `u32` year count does not fit `u32`.
        let total_months = u64::from(self.years) * 12 + u64::from(self.months);
        let years = total_months / 12;
        let months = total_months % 12;

        // Normalize day-time part
        let (days, hours, minutes, seconds) =
            normalize_day_time(self.days, self.hours, self.minutes, self.seconds);

        if self.negative {
            write!(f, "-")?;
        }
        write!(f, "P")?;
        if years > 0 {
            write!(f, "{}Y", years)?;
        }
        if months > 0 {
            write!(f, "{}M", months)?;
        }
        if days > 0 {
            write!(f, "{}D", days)?;
        }
        if hours > 0 || minutes > 0 || !seconds.is_zero() {
            write!(f, "T")?;
            if hours > 0 {
                write!(f, "{}H", hours)?;
            }
            if minutes > 0 {
                write!(f, "{}M", minutes)?;
            }
            if !seconds.is_zero() {
                format_duration_seconds(seconds, f)?;
                write!(f, "S")?;
            }
        }
        // Handle zero duration
        if years == 0 && months == 0 && days == 0 && hours == 0 && minutes == 0 && seconds.is_zero()
        {
            write!(f, "T0S")?;
        }
        Ok(())
    }
}

/// xs:yearMonthDuration (XSD 1.1)
#[derive(Debug, Clone)]
pub struct YearMonthDurationValue {
    pub negative: bool,
    pub years: u32,
    pub months: u32,
}

impl fmt::Display for YearMonthDurationValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Normalize months → years + months
        // In `u64`: twelve times a `u32` year count does not fit `u32`.
        let total_months = u64::from(self.years) * 12 + u64::from(self.months);
        let years = total_months / 12;
        let months = total_months % 12;

        // Negative zero is normalized to positive zero
        if self.negative && (years > 0 || months > 0) {
            write!(f, "-")?;
        }
        write!(f, "P")?;
        if years > 0 {
            write!(f, "{}Y", years)?;
        }
        if months > 0 {
            write!(f, "{}M", months)?;
        }
        if years == 0 && months == 0 {
            write!(f, "0M")?;
        }
        Ok(())
    }
}

/// Equality in its (total) order: `P12M` equals `P1Y`, `PT24H` equals `P1D`.
impl PartialEq for YearMonthDurationValue {
    fn eq(&self, other: &Self) -> bool {
        self.partial_cmp(other) == Some(std::cmp::Ordering::Equal)
    }
}

impl PartialOrd for YearMonthDurationValue {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        let self_months = if self.negative {
            -(self.years as i64 * 12 + self.months as i64)
        } else {
            self.years as i64 * 12 + self.months as i64
        };
        let other_months = if other.negative {
            -(other.years as i64 * 12 + other.months as i64)
        } else {
            other.years as i64 * 12 + other.months as i64
        };
        self_months.partial_cmp(&other_months)
    }
}

/// xs:dayTimeDuration (XSD 1.1)
#[derive(Debug, Clone)]
pub struct DayTimeDurationValue {
    pub negative: bool,
    pub days: u32,
    pub hours: u32,
    pub minutes: u32,
    pub seconds: Decimal,
}

impl fmt::Display for DayTimeDurationValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Normalize seconds → minutes → hours → days
        let (days, hours, minutes, seconds) =
            normalize_day_time(self.days, self.hours, self.minutes, self.seconds);

        // Negative zero is normalized to positive zero
        if self.negative && (days > 0 || hours > 0 || minutes > 0 || !seconds.is_zero()) {
            write!(f, "-")?;
        }
        write!(f, "P")?;
        if days > 0 {
            write!(f, "{}D", days)?;
        }
        if hours > 0 || minutes > 0 || !seconds.is_zero() {
            write!(f, "T")?;
            if hours > 0 {
                write!(f, "{}H", hours)?;
            }
            if minutes > 0 {
                write!(f, "{}M", minutes)?;
            }
            if !seconds.is_zero() {
                format_duration_seconds(seconds, f)?;
                write!(f, "S")?;
            }
        }
        if days == 0 && hours == 0 && minutes == 0 && seconds.is_zero() {
            write!(f, "T0S")?;
        }
        Ok(())
    }
}

/// Equality in its (total) order: `P12M` equals `P1Y`, `PT24H` equals `P1D`.
impl PartialEq for DayTimeDurationValue {
    fn eq(&self, other: &Self) -> bool {
        self.partial_cmp(other) == Some(std::cmp::Ordering::Equal)
    }
}

impl PartialOrd for DayTimeDurationValue {
    /// The order of the seconds, a total order (Datatypes 1.1 §3.4.27) —
    /// that of the four reference dateTimes for durations without months —
    /// on the exact sums (whole seconds in `i128`, the fraction apart).
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        crate::types::value_order::duration_cmp(
            &crate::types::value_order::day_time_duration_parts(self),
            &crate::types::value_order::day_time_duration_parts(other),
        )
    }
}

/// xs:gYearMonth value
#[derive(Debug, Clone)]
pub struct GYearMonthValue {
    pub year: i32,
    pub month: u8,
    pub timezone: Option<TimezoneOffset>,
}

impl GYearMonthValue {
    /// The order of the two values on the time line with a missing time
    /// zone offset read as `Z` — the building block of the XSD order, which
    /// [`PartialOrd`] gives.
    pub(crate) fn timeline_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        let tz1 = self.timezone.map_or(0i64, |tz| tz.0 as i64);
        let tz2 = other.timezone.map_or(0i64, |tz| tz.0 as i64);
        let d1 = day_number(self.year, self.month, 1) * 1440 - i128::from(tz1);
        let d2 = day_number(other.year, other.month, 1) * 1440 - i128::from(tz2);
        d1.partial_cmp(&d2)
    }
}

impl fmt::Display for GYearMonthValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        format_year(self.year, f)?;
        write!(f, "-{:02}", self.month)?;
        if let Some(tz) = &self.timezone {
            write!(f, "{}", tz)?;
        }
        Ok(())
    }
}

/// xs:gYear value
#[derive(Debug, Clone)]
pub struct GYearValue {
    pub year: i32,
    pub timezone: Option<TimezoneOffset>,
}

impl GYearValue {
    /// The order of the two values on the time line with a missing time
    /// zone offset read as `Z` — the building block of the XSD order, which
    /// [`PartialOrd`] gives.
    pub(crate) fn timeline_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        let tz1 = self.timezone.map_or(0i64, |tz| tz.0 as i64);
        let tz2 = other.timezone.map_or(0i64, |tz| tz.0 as i64);
        let d1 = day_number(self.year, 1, 1) * 1440 - i128::from(tz1);
        let d2 = day_number(other.year, 1, 1) * 1440 - i128::from(tz2);
        d1.partial_cmp(&d2)
    }
}

impl fmt::Display for GYearValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        format_year(self.year, f)?;
        if let Some(tz) = &self.timezone {
            write!(f, "{}", tz)?;
        }
        Ok(())
    }
}

/// xs:gMonthDay value
#[derive(Debug, Clone)]
pub struct GMonthDayValue {
    pub month: u8,
    pub day: u8,
    pub timezone: Option<TimezoneOffset>,
}

impl GMonthDayValue {
    /// The order of the two values on the time line with a missing time
    /// zone offset read as `Z` — the building block of the XSD order, which
    /// [`PartialOrd`] gives.
    pub(crate) fn timeline_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        let tz1 = self.timezone.map_or(0i64, |tz| tz.0 as i64);
        let tz2 = other.timezone.map_or(0i64, |tz| tz.0 as i64);
        // Use reference year 2000 for gMonthDay comparison
        let d1 = day_number(2000, self.month, self.day) * 1440 - i128::from(tz1);
        let d2 = day_number(2000, other.month, other.day) * 1440 - i128::from(tz2);
        d1.partial_cmp(&d2)
    }
}

impl fmt::Display for GMonthDayValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "--{:02}-{:02}", self.month, self.day)?;
        if let Some(tz) = &self.timezone {
            write!(f, "{}", tz)?;
        }
        Ok(())
    }
}

/// xs:gDay value
#[derive(Debug, Clone)]
pub struct GDayValue {
    pub day: u8,
    pub timezone: Option<TimezoneOffset>,
}

impl GDayValue {
    /// The order of the two values on the time line with a missing time
    /// zone offset read as `Z` — the building block of the XSD order, which
    /// [`PartialOrd`] gives.
    pub(crate) fn timeline_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        let tz1 = self.timezone.map_or(0i64, |tz| tz.0 as i64);
        let tz2 = other.timezone.map_or(0i64, |tz| tz.0 as i64);
        let d1 = (self.day as i64) * 1440 - tz1;
        let d2 = (other.day as i64) * 1440 - tz2;
        d1.partial_cmp(&d2)
    }
}

impl fmt::Display for GDayValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "---{:02}", self.day)?;
        if let Some(tz) = &self.timezone {
            write!(f, "{}", tz)?;
        }
        Ok(())
    }
}

/// xs:gMonth value
#[derive(Debug, Clone)]
pub struct GMonthValue {
    pub month: u8,
    pub timezone: Option<TimezoneOffset>,
}

/// A gMonth is a month of 1972 on the time line (Datatypes 1.1 §E.3.4: "Let yr
/// be 1971 when dt's ·year· is absent", "mo be 12 or dt's ·month·"; XSD 1.0
/// §3.2.14: "the order relation on gMonth is the order relation on their
/// starting instants"), in minutes. Two months are at least 28 days apart, so
/// no pair of time zone offsets (at most 28 hours apart) reorders them. 1.1
/// places a month at its last day, 1.0 at its first: the order is the same.
impl GMonthValue {
    /// The order of the two values on the time line with a missing time
    /// zone offset read as `Z` — the building block of the XSD order, which
    /// [`PartialOrd`] gives.
    pub(crate) fn timeline_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        let tz1 = self.timezone.map_or(0i64, |tz| tz.0 as i64);
        let tz2 = other.timezone.map_or(0i64, |tz| tz.0 as i64);
        let d1 = day_number(1972, self.month, 1) * 1440 - i128::from(tz1);
        let d2 = day_number(1972, other.month, 1) * 1440 - i128::from(tz2);
        d1.partial_cmp(&d2)
    }
}

impl fmt::Display for GMonthValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "--{:02}", self.month)?;
        if let Some(tz) = &self.timezone {
            write!(f, "{}", tz)?;
        }
        Ok(())
    }
}

/// The order of the date/time datatypes of the seven-property model
/// (Datatypes 1.1 §D.2.1): values "are ordered the same as their
/// ·timeOnTimeline· values, except that if one value's ·timezoneOffset· is
/// absent and the other's is not, and using maximum and minimum
/// ·timezoneOffset· values for the one whose ·timezoneOffset· is actually
/// absent changes the resulting (strict) inequality, the original two values
/// are incomparable" (XSD 1.0 §3.2.7.4 alike) — `None` then. Equality is the
/// `Equal` of this order: one instant in two offsets is one value; a value
/// with an offset equals no value without one. The year is read with the XSD
/// 1.1 numbering (`0000` is 1 BCE).
macro_rules! seven_property_order {
    ($($ty:ty),*) => {$(
        impl PartialOrd for $ty {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                let impute = |value: &Self, minutes: i16| Self {
                    timezone: Some(TimezoneOffset(minutes)),
                    ..value.clone()
                };
                match (self.timezone.is_some(), other.timezone.is_some()) {
                    (true, false) => crate::types::value_order::same_order_at_both_extreme_offsets(
                        |minutes| self.timeline_cmp(&impute(other, minutes)),
                    ),
                    (false, true) => crate::types::value_order::same_order_at_both_extreme_offsets(
                        |minutes| impute(self, minutes).timeline_cmp(other),
                    ),
                    _ => self.timeline_cmp(other),
                }
            }
        }

        impl PartialEq for $ty {
            fn eq(&self, other: &Self) -> bool {
                self.partial_cmp(other) == Some(std::cmp::Ordering::Equal)
            }
        }
    )*};
}
seven_property_order!(
    DateTimeValue,
    DateValue,
    TimeValue,
    GYearMonthValue,
    GYearValue,
    GMonthDayValue,
    GDayValue,
    GMonthValue
);

/// Timezone offset in minutes from UTC
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimezoneOffset(pub i16);

impl TimezoneOffset {
    /// UTC timezone
    pub const UTC: Self = Self(0);

    /// Create a timezone offset from hours and minutes
    pub fn from_hm(hours: i8, minutes: i8) -> Self {
        Self(hours as i16 * 60 + minutes as i16)
    }

    /// Get hours component
    pub fn hours(&self) -> i8 {
        (self.0 / 60) as i8
    }

    /// Get minutes component
    pub fn minutes(&self) -> i8 {
        (self.0 % 60).abs() as i8
    }
}

impl fmt::Display for TimezoneOffset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 == 0 {
            write!(f, "Z")
        } else {
            let sign = if self.0 > 0 { '+' } else { '-' };
            let hours = (self.0.abs() / 60) as u8;
            let minutes = (self.0.abs() % 60) as u8;
            write!(f, "{}{:02}:{:02}", sign, hours, minutes)
        }
    }
}

/// Normalize day-time duration components.
/// Convert a date to total days from a reference epoch (year 1, month 1, day 1).
/// Used for comparing date/time values.
/// The day of `year-month-day` counted from 1970-01-01 in the proleptic
/// Gregorian calendar of Datatypes 1.1 (§E.3.4: the year 0000 is a leap year),
/// exact for every year (`i128`, floor division).
pub(crate) fn day_number(year: i32, month: u8, day: u8) -> i128 {
    crate::types::value_order::days_from_civil(i128::from(year), i128::from(month))
        + i128::from(day)
        - 1
}

/// A position on the time line, exactly: whole seconds from 1970-01-01T00:00Z
/// (a missing time zone offset read as `Z`), and the fraction of a second in
/// `[0, 1)` as parsed — kept apart so that no sum rounds (Datatypes 1.1 §E.3.4
/// ·timeOnTimeline·). Compared lexicographically. The XPath operators order
/// date/time values on the same key, with their implicit time zone in place
/// of a missing offset.
pub(crate) fn time_line_seconds(
    days: i128,
    hour: u8,
    minute: u8,
    second: Decimal,
    timezone: Option<TimezoneOffset>,
) -> (i128, Decimal) {
    let whole = second.trunc();
    let fraction = (second - whole).normalize();
    let whole = whole.to_i128().unwrap_or_default();
    let offset = timezone.map_or(0, |tz| i128::from(tz.0));
    (
        days * 86_400 + i128::from(hour) * 3_600 + i128::from(minute) * 60 + whole - offset * 60,
        fraction,
    )
}

/// The canonical day-time fields of a duration (Datatypes 1.1 §E.3.2
/// ·durationCanonicalMap·): carries over whole seconds into minutes, minutes
/// into hours, hours into days; only the integer part of seconds is carried,
/// the fractional part stays in seconds. In `u128`, so that every value the
/// fields hold (whole seconds up to 2⁹⁶, `u32` days, hours and minutes)
/// carries without overflow.
fn normalize_day_time(
    days: u32,
    hours: u32,
    minutes: u32,
    seconds: Decimal,
) -> (u128, u128, u128, Decimal) {
    let whole_secs = seconds.trunc();
    let frac_secs = seconds - whole_secs;

    // A non-negative `Decimal`'s integral part (below 2⁹⁶) always fits `u128`.
    let total_secs: u128 = whole_secs.to_u128().unwrap_or(0);
    let mut mins = u128::from(minutes) + total_secs / 60;
    let rem_secs = (total_secs % 60) as u32;
    let mut hrs = u128::from(hours) + mins / 60;
    mins %= 60;
    let d = u128::from(days) + hrs / 24;
    hrs %= 24;

    let out_seconds = Decimal::from(rem_secs) + frac_secs;
    (d, hrs, mins, out_seconds)
}

/// Format seconds with optional fractional part (zero-padded for time-of-day: HH:MM:SS)
fn format_seconds(s: Decimal, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    if s.fract().is_zero() {
        write!(f, "{:02}", s.trunc())
    } else {
        // Format with fractional seconds, trimming trailing zeros
        let formatted = format!("{}", s);
        if let Some(dot_pos) = formatted.find('.') {
            let int_part = &formatted[..dot_pos];
            let frac_part = formatted[dot_pos + 1..].trim_end_matches('0');
            if int_part.len() == 1 {
                write!(f, "0{}.{}", int_part, frac_part)
            } else {
                write!(f, "{}.{}", int_part, frac_part)
            }
        } else {
            write!(f, "{:02}", s)
        }
    }
}

/// Format seconds for duration values (no zero-padding)
fn format_duration_seconds(s: Decimal, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    if s.fract().is_zero() {
        write!(f, "{}", s.trunc())
    } else {
        let formatted = format!("{}", s);
        if let Some(dot_pos) = formatted.find('.') {
            let int_part = &formatted[..dot_pos];
            let frac_part = formatted[dot_pos + 1..].trim_end_matches('0');
            write!(f, "{}.{}", int_part, frac_part)
        } else {
            write!(f, "{}", s)
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xml_value_string() {
        let v = XmlValue::string("hello");
        assert_eq!(v.type_code, XmlTypeCode::String);
        assert!(v.is_atomic());
        assert_eq!(v.to_string_value(), "hello");
        assert_eq!(v.as_string(), Some("hello"));
    }

    #[test]
    fn test_xml_value_boolean() {
        let v = XmlValue::boolean(true);
        assert_eq!(v.type_code, XmlTypeCode::Boolean);
        assert_eq!(v.as_boolean(), Some(true));
        assert_eq!(v.to_string_value(), "true");

        let v = XmlValue::boolean(false);
        assert_eq!(v.to_string_value(), "false");
    }

    #[test]
    fn test_xml_value_decimal() {
        let v = XmlValue::decimal(Decimal::new(12345, 2));
        assert_eq!(v.type_code, XmlTypeCode::Decimal);
        assert_eq!(v.as_decimal(), Some(Decimal::new(12345, 2)));
    }

    #[test]
    fn test_xml_value_integer() {
        let v = XmlValue::integer(BigInt::from(42));
        assert_eq!(v.type_code, XmlTypeCode::Integer);
        assert_eq!(v.as_integer(), Some(&BigInt::from(42)));
        assert_eq!(v.to_string_value(), "42");
    }

    #[test]
    fn test_xml_value_double() {
        let v = XmlValue::double(2.5);
        assert_eq!(v.type_code, XmlTypeCode::Double);
        assert_eq!(v.as_double(), Some(2.5));
    }

    #[test]
    fn test_xml_value_untyped() {
        let v = XmlValue::untyped("raw text");
        assert_eq!(v.type_code, XmlTypeCode::UntypedAtomic);
        assert!(v.is_untyped());
        assert_eq!(v.as_string(), Some("raw text"));
    }

    #[test]
    fn test_xml_atomic_value_type_code() {
        assert_eq!(
            XmlAtomicValue::String("test".into()).type_code(),
            XmlTypeCode::String
        );
        assert_eq!(
            XmlAtomicValue::Boolean(true).type_code(),
            XmlTypeCode::Boolean
        );
        assert_eq!(
            XmlAtomicValue::Integer(BigInt::from(1)).type_code(),
            XmlTypeCode::Integer
        );
    }

    #[test]
    fn test_timezone_display() {
        assert_eq!(TimezoneOffset::UTC.to_string(), "Z");
        assert_eq!(TimezoneOffset::from_hm(5, 30).to_string(), "+05:30");
        assert_eq!(TimezoneOffset::from_hm(-8, 0).to_string(), "-08:00");
    }

    #[test]
    fn test_date_display() {
        let d = DateValue {
            year: 2024,
            month: 3,
            day: 15,
            timezone: Some(TimezoneOffset::UTC),
        };
        assert_eq!(d.to_string(), "2024-03-15Z");
    }

    #[test]
    fn test_duration_display() {
        let d = DurationValue {
            negative: false,
            years: 1,
            months: 2,
            days: 3,
            hours: 4,
            minutes: 5,
            seconds: Decimal::new(65, 1), // 6.5 seconds
        };
        // Note: format_seconds may zero-pad to 2 digits
        assert!(d.to_string().starts_with("P1Y2M3DT4H5M"));
        assert!(d.to_string().contains("6.5S"));
    }

    #[test]
    fn test_float_special_values() {
        assert_eq!(format!("{}", XmlAtomicValue::Float(f32::INFINITY)), "INF");
        assert_eq!(
            format!("{}", XmlAtomicValue::Float(f32::NEG_INFINITY)),
            "-INF"
        );
        assert_eq!(format!("{}", XmlAtomicValue::Float(f32::NAN)), "NaN");
    }

    #[test]
    fn test_canonical_durations_of_the_largest_fields() {
        use crate::types::validators::{DurationValidator, TypeValidator};
        let canonical = |lexical: &str| {
            DurationValidator
                .validate(lexical)
                .unwrap()
                .to_string_value()
        };
        // 4294967295 years and months: 55834574835 months; the day-time part
        // carries 2⁹⁶ − 1 seconds and the u32 fields without overflow.
        assert_eq!(canonical("P4294967295Y4294967295M"), "P4652881236Y3M");
        assert_eq!(
            canonical("-P4294967295DT4294967295H4294967295M79228162514264337593543950335S"),
            "-P916992621692878754609936DT18H47M15S"
        );
        assert_eq!(canonical("PT0.5S"), "PT0.5S");
    }

    #[test]
    fn test_hex_binary_display() {
        let v = XmlAtomicValue::HexBinary(vec![0xDE, 0xAD, 0xBE, 0xEF]);
        assert_eq!(format!("{}", v), "DEADBEEF");
    }

    #[test]
    fn test_base64_binary_display() {
        let v = XmlAtomicValue::Base64Binary(b"Hello".to_vec());
        assert_eq!(format!("{}", v), "SGVsbG8=");
    }

    /// F&O §17.1.2: a QName or NOTATION converts to `prefix:local`, or to the
    /// local name when it has no prefix — with no name table at hand.
    #[test]
    fn test_qname_value_string_form() {
        let names = NameTable::new();
        let ns = names.add("urn:x");
        let local = names.add("local");
        let prefix = names.add("p");
        let empty = names.add("");
        let prefixed = QNameValue::new(QualifiedName::new(Some(ns), local, Some(prefix)), &names);
        assert_eq!(prefixed.lexical(), "p:local");
        assert_eq!(prefixed.prefix(), Some("p"));
        assert_eq!(prefixed.local_name(), "local");
        assert_eq!(prefixed.name().namespace_uri, Some(ns));
        let bare = QNameValue::new(QualifiedName::new(Some(ns), local, None), &names);
        assert_eq!((bare.lexical(), bare.prefix()), ("local", None));
        // An empty prefix is no prefix.
        let empty_prefix = QNameValue::new(QualifiedName::new(None, local, Some(empty)), &names);
        assert_eq!(
            (empty_prefix.lexical(), empty_prefix.prefix()),
            ("local", None)
        );

        for (code, atom) in [
            (XmlTypeCode::QName, XmlAtomicValue::QName(prefixed.clone())),
            (
                XmlTypeCode::Notation,
                XmlAtomicValue::Notation(prefixed.clone()),
            ),
        ] {
            assert_eq!(atom.to_string(), "p:local");
            let value = XmlValue::new(code, XmlValueKind::Atomic(atom));
            assert_eq!(value.to_string_value(), "p:local");
        }
        let qname = XmlValue::new(
            XmlTypeCode::QName,
            XmlValueKind::Atomic(XmlAtomicValue::QName(prefixed.clone())),
        );
        assert_eq!(qname.as_qname(), Some(prefixed.name()));
    }

    /// F&O §11.2.1: equality (and so hashing) ignores the prefix.
    #[test]
    fn test_qname_value_equality_ignores_the_prefix() {
        use std::hash::BuildHasher;
        let names = NameTable::new();
        let ns = names.add("urn:x");
        let local = names.add("l");
        let p = QNameValue::new(
            QualifiedName::new(Some(ns), local, Some(names.add("p"))),
            &names,
        );
        let q = QNameValue::new(
            QualifiedName::new(Some(ns), local, Some(names.add("q"))),
            &names,
        );
        let other = QNameValue::new(
            QualifiedName::new(None, local, Some(names.add("p"))),
            &names,
        );
        assert_eq!(p, q);
        assert_ne!(p, other);
        let state = std::collections::hash_map::RandomState::new();
        assert_eq!(state.hash_one(&p), state.hash_one(&q));
    }

    /// A list's string value joins its members' string values with spaces.
    #[test]
    fn test_list_string_value_joins_members() {
        let list = XmlValue::new(
            XmlTypeCode::AnySimpleType,
            XmlValueKind::List {
                item_type: XmlTypeCode::AnySimpleType,
                items: vec![XmlValue::integer(BigInt::from(1)), XmlValue::string("x")],
            },
        );
        assert_eq!(list.to_string_value(), "1 x");
        assert!(list.is_list());
    }
}
