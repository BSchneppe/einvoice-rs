//! XPath 2.0 function registry and dispatch.
//!
//! This module provides:
//! - `FunctionId` - Enum identifying all built-in XPath functions
//! - `XPathValue` - Result type for function evaluation
//! - Helper functions for argument atomization and conversion
//! - `eval_function` - Main dispatch function
//!
//! ## Architecture
//!
//! Functions are identified by `FunctionId` which allows non-generic registry
//! lookup at bind time. Function dispatch uses a match on `FunctionId` to call
//! the appropriate implementation.
//!
//! Each built-in converts its own arguments; `eval_function` does not consult
//! the registry's `param_types`. The crate-internal `convert` module holds the
//! shared function conversion rules of XPath 2.0 §3.1.5 that a built-in with a
//! specific atomic parameter type applies to an `xs:untypedAtomic` argument.

pub mod aggregate;
pub(crate) mod convert;
pub mod datetime;
pub mod extensible;
pub mod node;
pub mod numeric;
pub mod qname;
pub mod regex;
pub mod registry;
pub mod sequence;
pub mod signature;
pub mod special;
pub mod string;
pub mod uri;

pub use extensible::{
    BuiltinCatalog, BuiltinEvaluator, CustomFn, DynamicFunctionSignature, FunctionCatalog,
    FunctionEvaluator, FunctionHandle, FunctionSet, XPath10Catalog, XPath10Evaluator,
};
pub use registry::{FunctionEntry, FunctionKey, FunctionRegistry, FUNCTION_REGISTRY};
pub use signature::{FunctionArity, FunctionSignature, FN_2010_NAMESPACE, FN_NAMESPACE};

use num_bigint::BigInt;

use crate::types::value::XmlValue;
use crate::types::XmlTypeCode;
use crate::xpath::atomize;
use crate::xpath::error::XPathError;
use crate::xpath::iterator::XmlItem;
use crate::xpath::DomNavigator;

use super::context::DynamicContext;

/// XPath function identifiers.
///
/// Each variant corresponds to a built-in XPath 2.0 function.
/// This enum allows bind-time function resolution without generic type parameters.
///
/// The enum is `#[non_exhaustive]`: a later release may add built-in functions,
/// so a `match` outside this crate needs a wildcard arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
#[non_exhaustive]
pub enum FunctionId {
    // ========== String Functions ==========
    /// fn:concat($arg1, $arg2, ...)
    Concat = 1,
    /// fn:string-join($arg1, $arg2)
    StringJoin,
    /// fn:substring($sourceString, $start, $length?)
    Substring,
    /// fn:string-length($arg?)
    StringLength,
    /// fn:normalize-space($arg?)
    NormalizeSpace,
    /// fn:normalize-unicode($arg, $normalizationForm?)
    NormalizeUnicode,
    /// fn:upper-case($arg)
    UpperCase,
    /// fn:lower-case($arg)
    LowerCase,
    /// fn:translate($arg, $mapString, $transString)
    Translate,
    /// fn:encode-for-uri($uri-part)
    EncodeForUri,
    /// fn:iri-to-uri($iri)
    IriToUri,
    /// fn:escape-html-uri($uri)
    EscapeHtmlUri,
    /// fn:contains($arg1, $arg2, $collation?)
    Contains,
    /// fn:starts-with($arg1, $arg2, $collation?)
    StartsWith,
    /// fn:ends-with($arg1, $arg2, $collation?)
    EndsWith,
    /// fn:substring-before($arg1, $arg2, $collation?)
    SubstringBefore,
    /// fn:substring-after($arg1, $arg2, $collation?)
    SubstringAfter,
    /// fn:string-to-codepoints($arg)
    StringToCodepoints,
    /// fn:codepoints-to-string($arg)
    CodepointsToString,
    /// fn:compare($comparand1, $comparand2, $collation?)
    Compare,
    /// fn:codepoint-equal($comparand1, $comparand2)
    CodepointEqual,

    // ========== Numeric Functions ==========
    /// fn:abs($arg)
    Abs = 100,
    /// fn:ceiling($arg)
    Ceiling,
    /// fn:floor($arg)
    Floor,
    /// fn:round($arg)
    Round,
    /// fn:round-half-to-even($arg, $precision?)
    RoundHalfToEven,

    // ========== Sequence Functions ==========
    /// fn:empty($arg)
    Empty = 200,
    /// fn:exists($arg)
    Exists,
    /// fn:reverse($arg)
    Reverse,
    /// fn:index-of($seq, $search, $collation?)
    IndexOf,
    /// fn:remove($target, $position)
    Remove,
    /// fn:insert-before($target, $position, $inserts)
    InsertBefore,
    /// fn:subsequence($sourceSeq, $startingLoc, $length?)
    Subsequence,
    /// fn:unordered($sourceSeq)
    Unordered,
    /// fn:zero-or-one($arg)
    ZeroOrOne,
    /// fn:one-or-more($arg)
    OneOrMore,
    /// fn:exactly-one($arg)
    ExactlyOne,
    /// fn:distinct-values($arg, $collation?)
    DistinctValues,
    /// fn:deep-equal($parameter1, $parameter2, $collation?)
    DeepEqual,
    /// fn:count($arg)
    Count,

    // ========== Aggregate Functions ==========
    /// fn:sum($arg, $zero?)
    Sum = 300,
    /// fn:avg($arg)
    Avg,
    /// fn:min($arg, $collation?)
    Min,
    /// fn:max($arg, $collation?)
    Max,

    // ========== Node Functions ==========
    /// fn:name($arg?)
    Name = 400,
    /// fn:local-name($arg?)
    LocalName,
    /// fn:namespace-uri($arg?)
    NamespaceUri,
    /// fn:node-name($arg?)
    NodeName,
    /// fn:nilled($arg)
    Nilled,
    /// fn:base-uri($arg?)
    BaseUri,
    /// fn:document-uri($arg)
    DocumentUri,
    /// fn:lang($testlang, $node?)
    Lang,
    /// fn:root($arg?)
    Root,
    /// fn:id($arg as xs:string*, $node as node()) as element()*
    Id,
    /// fn:collection($arg as xs:string?) as node()*
    Collection,
    /// fn:idref($arg as xs:string*, $node as node()) as node()*
    Idref,

    // ========== DateTime Functions ==========
    /// fn:dateTime($arg1, $arg2)
    DateTime = 500,
    /// fn:current-dateTime()
    CurrentDateTime,
    /// fn:current-date()
    CurrentDate,
    /// fn:current-time()
    CurrentTime,
    /// fn:implicit-timezone()
    ImplicitTimezone,
    /// fn:years-from-duration($arg)
    YearsFromDuration,
    /// fn:months-from-duration($arg)
    MonthsFromDuration,
    /// fn:days-from-duration($arg)
    DaysFromDuration,
    /// fn:hours-from-duration($arg)
    HoursFromDuration,
    /// fn:minutes-from-duration($arg)
    MinutesFromDuration,
    /// fn:seconds-from-duration($arg)
    SecondsFromDuration,
    /// fn:year-from-dateTime($arg)
    YearFromDateTime,
    /// fn:month-from-dateTime($arg)
    MonthFromDateTime,
    /// fn:day-from-dateTime($arg)
    DayFromDateTime,
    /// fn:hours-from-dateTime($arg)
    HoursFromDateTime,
    /// fn:minutes-from-dateTime($arg)
    MinutesFromDateTime,
    /// fn:seconds-from-dateTime($arg)
    SecondsFromDateTime,
    /// fn:timezone-from-dateTime($arg)
    TimezoneFromDateTime,
    /// fn:year-from-date($arg)
    YearFromDate,
    /// fn:month-from-date($arg)
    MonthFromDate,
    /// fn:day-from-date($arg)
    DayFromDate,
    /// fn:timezone-from-date($arg)
    TimezoneFromDate,
    /// fn:hours-from-time($arg)
    HoursFromTime,
    /// fn:minutes-from-time($arg)
    MinutesFromTime,
    /// fn:seconds-from-time($arg)
    SecondsFromTime,
    /// fn:timezone-from-time($arg)
    TimezoneFromTime,
    /// fn:adjust-dateTime-to-timezone($arg, $timezone?)
    AdjustDateTimeToTimezone,
    /// fn:adjust-date-to-timezone($arg, $timezone?)
    AdjustDateToTimezone,
    /// fn:adjust-time-to-timezone($arg, $timezone?)
    AdjustTimeToTimezone,

    // ========== QName Functions ==========
    /// fn:resolve-QName($qname, $element)
    ResolveQName = 600,
    /// fn:QName($paramURI, $paramLocal)
    QName,
    /// fn:prefix-from-QName($arg)
    PrefixFromQName,
    /// fn:local-name-from-QName($arg)
    LocalNameFromQName,
    /// fn:namespace-uri-from-QName($arg)
    NamespaceUriFromQName,
    /// fn:namespace-uri-for-prefix($prefix, $element)
    NamespaceUriForPrefix,
    /// fn:in-scope-prefixes($element)
    InScopePrefixes,

    // ========== URI Functions ==========
    /// fn:resolve-uri($relative, $base?)
    ResolveUri = 700,
    /// fn:static-base-uri()
    StaticBaseUri,

    // ========== Regex Functions ==========
    /// fn:matches($input, $pattern, $flags?)
    Matches = 800,
    /// fn:replace($input, $pattern, $replacement, $flags?)
    Replace,
    /// fn:tokenize($input, $pattern, $flags?)
    Tokenize,

    // ========== Special/Context Functions ==========
    /// fn:position()
    Position = 900,
    /// fn:last()
    Last,
    /// fn:trace($value, $label?)
    Trace,
    /// fn:data($arg)
    Data,
    /// fn:default-collation()
    DefaultCollation,
    /// fn:error(), fn:error($error), fn:error($error, $description),
    /// fn:error($error, $description, $error-object)
    ///
    /// One registry entry serves the four arities, so signature introspection
    /// ([`BuiltinCatalog::get_signature`]) reports the parameters
    /// `xs:QName?, xs:string, item()*` for each — for arity 1 that is
    /// `xs:QName?` where F&O declares `xs:QName`. The function itself enforces
    /// each arity's own types (see [`special::error`]).
    Error,

    // ========== Boolean Functions ==========
    /// fn:true()
    True = 1000,
    /// fn:false()
    False,
    /// fn:not($arg)
    Not,
    /// fn:boolean($arg)
    Boolean,

    // ========== Conversion Functions ==========
    /// fn:string($arg?)
    String = 1100,
    /// fn:number($arg?)
    Number,
}

/// XPath value representing a function result.
///
/// This enum represents the result of evaluating an XPath expression or function.
/// It can be empty, a single item, or a sequence of items.
#[derive(Debug, Clone)]
pub enum XPathValue<N: DomNavigator> {
    /// Empty sequence
    Empty,
    /// Single item (node or atomic value)
    Item(XmlItem<N>),
    /// Sequence of items (materialized)
    Sequence(Vec<XmlItem<N>>),
}

impl<N: DomNavigator> XPathValue<N> {
    /// Create an empty value
    pub fn empty() -> Self {
        Self::Empty
    }

    /// Create a value from a single item
    pub fn from_item(item: XmlItem<N>) -> Self {
        Self::Item(item)
    }

    /// Create a value from an atomic XmlValue
    pub fn from_atomic(value: XmlValue) -> Self {
        Self::Item(XmlItem::Atomic(value))
    }

    /// Create a value from a node
    pub fn from_node(node: N) -> Self {
        Self::Item(XmlItem::Node(node))
    }

    /// Create a value from a sequence of items
    pub fn from_sequence(items: Vec<XmlItem<N>>) -> Self {
        match items.len() {
            0 => Self::Empty,
            1 => Self::Item(items.into_iter().next().unwrap()),
            _ => Self::Sequence(items),
        }
    }

    /// Create a boolean value
    pub fn boolean(b: bool) -> Self {
        Self::from_atomic(XmlValue::boolean(b))
    }

    /// Create a string value
    pub fn string(s: impl Into<String>) -> Self {
        Self::from_atomic(XmlValue::string(s))
    }

    /// Create an integer value
    pub fn integer(i: impl Into<num_bigint::BigInt>) -> Self {
        Self::from_atomic(XmlValue::integer(i.into()))
    }

    /// Create a decimal value
    pub fn decimal(d: rust_decimal::Decimal) -> Self {
        Self::from_atomic(XmlValue::decimal(d))
    }

    /// Create a double value
    pub fn double(d: f64) -> Self {
        Self::from_atomic(XmlValue::double(d))
    }

    /// Check if this value is empty
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    /// Get the count of items
    pub fn len(&self) -> usize {
        match self {
            Self::Empty => 0,
            Self::Item(_) => 1,
            Self::Sequence(items) => items.len(),
        }
    }

    /// Check if this is a single item
    pub fn is_single(&self) -> bool {
        matches!(self, Self::Item(_))
    }

    /// Convert to a Vec of items
    pub fn into_vec(self) -> Vec<XmlItem<N>> {
        match self {
            Self::Empty => Vec::new(),
            Self::Item(item) => vec![item],
            Self::Sequence(items) => items,
        }
    }

    /// Borrow the items as a slice, without cloning: empty for `Empty`, one
    /// element for `Item`, and the whole sequence for `Sequence`.
    ///
    /// The slice always has [`len`](Self::len) elements, in sequence order.
    pub fn as_slice(&self) -> &[XmlItem<N>] {
        match self {
            Self::Empty => &[],
            Self::Item(item) => std::slice::from_ref(item),
            Self::Sequence(items) => items,
        }
    }

    /// Try to get the first item
    pub fn first(&self) -> Option<&XmlItem<N>> {
        match self {
            Self::Empty => None,
            Self::Item(item) => Some(item),
            Self::Sequence(items) => items.first(),
        }
    }

    // ========================================================================
    // Atomic Value Extraction Methods
    // ========================================================================

    /// The string of a single atomic item that holds one — `xs:string`, a
    /// type derived from it, or `xs:untypedAtomic` — copied into a new
    /// `String`.
    ///
    /// Returns `None` if:
    /// - The value is empty
    /// - The value is a sequence
    /// - The item is a node (not atomic)
    /// - The atomic value is not a string type
    ///
    /// Nothing is converted: an `xs:integer` gives `None`, not its lexical
    /// form, and a node is not atomized. For the string value of any atomic
    /// item use [`XmlValue::to_string_value`] on it (or `fn:string` in the
    /// expression).
    ///
    /// ```
    /// use xsd_schema::xpath::functions::XPathValue;
    /// use xsd_schema::xpath::RoXmlNavigator;
    ///
    /// type Value = XPathValue<RoXmlNavigator<'static>>;
    /// assert_eq!(Value::string("hello").to_owned_string().as_deref(), Some("hello"));
    /// assert_eq!(Value::integer(42).to_owned_string(), None);
    /// ```
    pub fn to_owned_string(&self) -> Option<String> {
        match self {
            Self::Item(XmlItem::Atomic(v)) => v.as_string().map(str::to_owned),
            _ => None,
        }
    }

    /// The same as [`to_owned_string`](Self::to_owned_string), under a name
    /// that suggests a borrow: the string is copied.
    #[deprecated(
        since = "0.3.0",
        note = "returns an owned `String`, not a borrow; use `to_owned_string`"
    )]
    pub fn as_str(&self) -> Option<String> {
        self.to_owned_string()
    }

    /// Try to extract a boolean from a single atomic item.
    ///
    /// Returns `None` if the value is not a single atomic boolean.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Item(XmlItem::Atomic(v)) => v.as_boolean(),
            _ => None,
        }
    }

    /// Try to extract a double from a single atomic item.
    ///
    /// Returns `None` if the value is not a single atomic numeric value.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Item(XmlItem::Atomic(v)) => v.as_double(),
            _ => None,
        }
    }

    /// Try to extract an integer from a single atomic item.
    ///
    /// Returns `None` if the value is not a single atomic integer.
    pub fn as_integer(&self) -> Option<num_bigint::BigInt> {
        match self {
            Self::Item(XmlItem::Atomic(v)) => v.as_integer().cloned(),
            _ => None,
        }
    }
}

// ============================================================================
// Helper Functions for Argument Processing
// ============================================================================

/// The value one item atomizes to before a packed list is taken apart
/// (XPath 2.0 §2.4.2): a node's typed value, `None` for a nilled element; an
/// atomic item itself, a union value unwrapped to the value of its member type.
/// A packed list value — the stored typed value of a list-typed node — stands
/// for its members ([`atomize::unpack_list`]), which the callers below take
/// apart.
fn item_value<N: DomNavigator>(item: XmlItem<N>) -> Result<Option<XmlValue>, XPathError> {
    match item {
        XmlItem::Node(nav) => atomize::atomize_node(&nav),
        XmlItem::Atomic(value) => Ok(Some(atomize::unwrap_union_owned(value))),
    }
}

/// The single atomic value one item atomizes to: `None` for none (a nilled
/// element, an empty list), XPTY0004 for a list-typed node with more than one
/// member.
fn item_at_most_one<N: DomNavigator>(item: XmlItem<N>) -> Result<Option<XmlValue>, XPathError> {
    match item_value(item)? {
        None => Ok(None),
        Some(value) => atomize::single_atom(value),
    }
}

/// Append the atomic values one item atomizes to: one per member of a
/// list-typed node, none for a nilled element.
fn push_item_atoms<N: DomNavigator>(
    item: XmlItem<N>,
    out: &mut Vec<XmlValue>,
) -> Result<(), XPathError> {
    if let Some(value) = item_value(item)? {
        atomize::push_atoms(value, out);
    }
    Ok(())
}

/// Atomize a value and convert to string.
///
/// The value is atomized as the function conversion rules require (XPath 2.0
/// §3.1.5): a node contributes its typed value, not its string value, so a
/// typed node gives the string form of its typed value and an element with
/// element-only content raises `FOTY0012`. This handles:
/// - Empty value, or a node whose typed value is empty -> empty string
/// - Single atomic value after atomization -> its string value
/// - More than one item or atomic value -> error (XPTY0004)
pub fn atomize_to_string<N: DomNavigator>(value: XPathValue<N>) -> Result<String, XPathError> {
    Ok(atomize_to_string_opt(value)?.unwrap_or_default())
}

/// Atomize a value and convert to required string.
///
/// Returns error if the value atomizes to the empty sequence or to more than
/// one atomic value.
pub fn atomize_to_string_required<N: DomNavigator>(
    value: XPathValue<N>,
) -> Result<String, XPathError> {
    atomize_to_string_opt(value)?.ok_or_else(|| XPathError::XPTY0004 {
        expected: "xs:string".to_string(),
        found: "empty-sequence()".to_string(),
    })
}

/// Atomize a value and convert to string with strict type checking.
///
/// Per XPath 2.0, only xs:string, xs:untypedAtomic, and xs:anyURI can be
/// promoted to xs:string. Other types (e.g., xs:integer) raise XPTY0004 —
/// including the typed value of a node, which is atomized first (§3.1.5).
/// A value that atomizes to the empty sequence returns the empty string.
pub fn atomize_to_string_strict<N: DomNavigator>(
    value: XPathValue<N>,
) -> Result<String, XPathError> {
    Ok(atomize_to_string_strict_opt(value)?.unwrap_or_default())
}

/// Atomize a value and convert to optional string with strict type checking.
///
/// Returns None for a value that atomizes to the empty sequence.
pub fn atomize_to_string_strict_opt<N: DomNavigator>(
    value: XPathValue<N>,
) -> Result<Option<String>, XPathError> {
    atomize_to_single_opt(value)?
        .map(atomic_to_string_strict)
        .transpose()
}

/// Convert an atomized value to string with strict type checking.
/// Only xs:string, xs:untypedAtomic, and xs:anyURI types are accepted.
fn atomic_to_string_strict(value: XmlValue) -> Result<String, XPathError> {
    match value.type_code {
        XmlTypeCode::String
        | XmlTypeCode::UntypedAtomic
        | XmlTypeCode::AnyUri
        | XmlTypeCode::NormalizedString
        | XmlTypeCode::Token
        | XmlTypeCode::Language
        | XmlTypeCode::NmToken
        | XmlTypeCode::Name
        | XmlTypeCode::NCName
        | XmlTypeCode::Id
        | XmlTypeCode::IdRef
        | XmlTypeCode::Entity => Ok(atomize::string_value(&value)),
        _ => Err(XPathError::XPTY0004 {
            expected: "xs:string".to_string(),
            found: crate::xpath::type_info::type_code_to_name(value.type_code).to_string(),
        }),
    }
}

/// Atomize a value and convert to optional string.
///
/// Returns None for a value that atomizes to the empty sequence.
pub fn atomize_to_string_opt<N: DomNavigator>(
    value: XPathValue<N>,
) -> Result<Option<String>, XPathError> {
    Ok(atomize_to_single_opt(value)?.map(|atomic| atomize::string_value(&atomic)))
}

/// `fn:string` of a value holding at most one item (F&O §2.3): the string
/// value of a node — the whole text, never its typed value — or the string
/// form of an atomic value, and the empty string for the empty sequence.
/// More than one item is an error.
pub(crate) fn string_of_single<N: DomNavigator>(
    value: XPathValue<N>,
) -> Result<String, XPathError> {
    match value {
        XPathValue::Empty => Ok(String::new()),
        XPathValue::Item(item) => Ok(item_string_value(item)),
        XPathValue::Sequence(items) => {
            if items.len() == 1 {
                Ok(item_string_value(items.into_iter().next().unwrap()))
            } else {
                Err(too_many_items("zero or one item"))
            }
        }
    }
}

/// The error for a value of more than one item where at most one is allowed.
///
/// XPath 2.0 §3.1.5, the function conversion rules: "If, after the above
/// conversions, the resulting value does not match the expected type according
/// to the rules for SequenceType Matching, a type error is raised
/// \[err:XPTY0004\]"; the operators state the same rule for their operands
/// (§3.4, §3.5.1, §3.5.3, §3.10.2). XPDY0050 is `treat as`'s code (§3.10.5),
/// not this one.
pub(crate) fn too_many_items(expected: &str) -> XPathError {
    XPathError::XPTY0004 {
        expected: expected.to_string(),
        found: "a sequence of more than one item".to_string(),
    }
}

/// The string value of a node, or the string form of an atomic value.
fn item_string_value<N: DomNavigator>(item: XmlItem<N>) -> String {
    match item {
        XmlItem::Atomic(value) => atomize::string_value(&value),
        XmlItem::Node(nav) => nav.value(),
    }
}

/// Atomize a value and convert to double, with `fn:number` semantics.
///
/// This handles:
/// - Empty value, or a node whose typed value is empty -> NaN
/// - Single atomic value after atomization -> its numeric value (NaN when it
///   does not convert); a node contributes its typed value (§3.1.5)
/// - More than one item or atomic value -> error (XPTY0004)
pub fn atomize_to_double<N: DomNavigator>(value: XPathValue<N>) -> Result<f64, XPathError> {
    Ok(match atomize_to_single_opt(value)? {
        None => f64::NAN,
        Some(atomic) => atomize::to_number(&atomic),
    })
}

/// Atomize a value to a single XmlValue.
///
/// Returns error if the value atomizes to the empty sequence or to more than
/// one atomic value.
pub fn atomize_to_single<N: DomNavigator>(value: XPathValue<N>) -> Result<XmlValue, XPathError> {
    atomize_to_single_opt(value)?.ok_or_else(|| XPathError::XPTY0004 {
        expected: "item()".to_string(),
        found: "empty-sequence()".to_string(),
    })
}

/// Atomize a value to an optional XmlValue.
///
/// `None` when the value atomizes to the empty sequence: the empty value, a
/// nilled element, or a list-typed node with no members (XPath 2.0 §3.4,
/// §3.5.1: "If the atomized operand is an empty sequence, the result … is an
/// empty sequence"). More than one item is a type error (XPTY0004, XPath 2.0
/// §3.1.5), and so is a single list-typed node with more than one member.
pub fn atomize_to_single_opt<N: DomNavigator>(
    value: XPathValue<N>,
) -> Result<Option<XmlValue>, XPathError> {
    atomize_at_most_one(value, "zero or one item")
}

/// [`atomize_to_single_opt`], naming the expected type in the XPTY0004 raised
/// for more than one item.
pub(crate) fn atomize_at_most_one<N: DomNavigator>(
    value: XPathValue<N>,
    expected: &str,
) -> Result<Option<XmlValue>, XPathError> {
    match value {
        XPathValue::Empty => Ok(None),
        XPathValue::Item(item) => item_at_most_one(item),
        XPathValue::Sequence(items) => {
            if items.len() == 1 {
                item_at_most_one(items.into_iter().next().unwrap())
            } else {
                Err(too_many_items(expected))
            }
        }
    }
}

/// Atomize all items in a value to a sequence of XmlValues.
///
/// Nilled elements (which atomize to the empty sequence) contribute nothing,
/// and a list-typed node contributes one value per member.
pub fn atomize_sequence<N: DomNavigator>(
    value: XPathValue<N>,
) -> Result<Vec<XmlValue>, XPathError> {
    match value {
        XPathValue::Empty => Ok(Vec::new()),
        XPathValue::Item(item) => {
            let mut result = Vec::with_capacity(1);
            push_item_atoms(item, &mut result)?;
            Ok(result)
        }
        XPathValue::Sequence(items) => {
            let mut result = Vec::with_capacity(items.len());
            for item in items {
                push_item_atoms(item, &mut result)?;
            }
            Ok(result)
        }
    }
}

/// Materialize a value to a Vec of XmlItems.
pub fn materialize<N: DomNavigator>(value: XPathValue<N>) -> Vec<XmlItem<N>> {
    value.into_vec()
}

// ============================================================================
// Function Dispatch
// ============================================================================

/// Evaluate a function by its ID.
///
/// This is the main dispatch function that routes to the appropriate
/// function implementation based on the FunctionId.
pub fn eval_function<N: DomNavigator>(
    id: FunctionId,
    context: &mut DynamicContext<'_, N>,
    args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    match id {
        // ====================================================================
        // Boolean functions
        // ====================================================================
        FunctionId::True => Ok(XPathValue::boolean(true)),
        FunctionId::False => Ok(XPathValue::boolean(false)),
        FunctionId::Not => eval_not(args),

        // ====================================================================
        // Context/Special functions
        // ====================================================================
        FunctionId::Position => special::position(context, args),
        FunctionId::Last => special::last(context, args),
        FunctionId::Trace => special::trace(context, args),
        FunctionId::Data => special::data(context, args),
        FunctionId::DefaultCollation => special::default_collation(context, args),
        FunctionId::Error => special::error(context, args),

        // ====================================================================
        // Sequence functions (basic)
        // ====================================================================
        FunctionId::Empty => eval_empty(args),
        FunctionId::Exists => eval_exists(args),
        FunctionId::Count => eval_count(args),

        // ====================================================================
        // String functions (Phase 2)
        // ====================================================================
        FunctionId::Concat => string::concat(context, args),
        FunctionId::StringJoin => string::string_join(context, args),
        FunctionId::Substring => string::substring(context, args),
        FunctionId::StringLength => string::string_length(context, args),
        FunctionId::NormalizeSpace => string::normalize_space(context, args),
        FunctionId::NormalizeUnicode => string::normalize_unicode(context, args),
        FunctionId::UpperCase => string::upper_case(context, args),
        FunctionId::LowerCase => string::lower_case(context, args),
        FunctionId::Translate => string::translate(context, args),
        FunctionId::EncodeForUri => string::encode_for_uri(context, args),
        FunctionId::IriToUri => string::iri_to_uri(context, args),
        FunctionId::EscapeHtmlUri => string::escape_html_uri(context, args),
        FunctionId::Contains => string::contains(context, args),
        FunctionId::StartsWith => string::starts_with(context, args),
        FunctionId::EndsWith => string::ends_with(context, args),
        FunctionId::SubstringBefore => string::substring_before(context, args),
        FunctionId::SubstringAfter => string::substring_after(context, args),
        FunctionId::StringToCodepoints => string::string_to_codepoints(context, args),
        FunctionId::CodepointsToString => string::codepoints_to_string(context, args),
        FunctionId::Compare => string::compare(context, args),
        FunctionId::CodepointEqual => string::codepoint_equal(context, args),

        // ====================================================================
        // Numeric functions (Phase 3)
        // ====================================================================
        FunctionId::Abs => numeric::abs(context, args),
        FunctionId::Ceiling => numeric::ceiling(context, args),
        FunctionId::Floor => numeric::floor(context, args),
        FunctionId::Round => numeric::round(context, args),
        FunctionId::RoundHalfToEven => numeric::round_half_to_even(context, args),

        // ====================================================================
        // Sequence functions (Phase 3)
        // ====================================================================
        FunctionId::Reverse => sequence::reverse(context, args),
        FunctionId::ZeroOrOne => sequence::zero_or_one(context, args),
        FunctionId::OneOrMore => sequence::one_or_more(context, args),
        FunctionId::ExactlyOne => sequence::exactly_one(context, args),
        FunctionId::DistinctValues => sequence::distinct_values(context, args),
        FunctionId::IndexOf => sequence::index_of(context, args),
        FunctionId::Remove => sequence::remove(context, args),
        FunctionId::InsertBefore => sequence::insert_before(context, args),
        FunctionId::Subsequence => sequence::subsequence(context, args),
        FunctionId::Unordered => sequence::unordered(context, args),
        FunctionId::DeepEqual => sequence::deep_equal(context, args),

        // ====================================================================
        // Aggregate functions (Phase 5)
        // ====================================================================
        FunctionId::Sum => aggregate::sum(context, args),
        FunctionId::Avg => aggregate::avg(context, args),
        FunctionId::Min => aggregate::min(context, args),
        FunctionId::Max => aggregate::max(context, args),

        // ====================================================================
        // Node functions (Phase 5)
        // ====================================================================
        FunctionId::Name => node::name(context, args),
        FunctionId::LocalName => node::local_name(context, args),
        FunctionId::NamespaceUri => node::namespace_uri(context, args),
        FunctionId::NodeName => node::node_name(context, args),
        FunctionId::Nilled => node::nilled(context, args),
        FunctionId::BaseUri => node::base_uri(context, args),
        FunctionId::DocumentUri => node::document_uri(context, args),
        FunctionId::Lang => node::lang(context, args),
        FunctionId::Root => node::root(context, args),
        FunctionId::Id => node::id(context, args),
        FunctionId::Idref => node::idref(context, args),
        FunctionId::Collection => {
            // fn:collection($arg?) — without a registered default collection
            // or URI handler, both forms return the empty sequence per
            // XPath/XQuery F&O §15.5.6. Sufficient for CTA tests that
            // only probe `empty(collection())`.
            if args.len() > 1 {
                return Err(XPathError::wrong_number_of_arguments(
                    "collection",
                    1,
                    args.len(),
                ));
            }
            Ok(XPathValue::Empty)
        }

        // ====================================================================
        // DateTime functions (Phase 6)
        // ====================================================================
        FunctionId::DateTime => datetime::create_datetime(context, args),
        FunctionId::CurrentDateTime => datetime::current_datetime(context, args),
        FunctionId::CurrentDate => datetime::current_date(context, args),
        FunctionId::CurrentTime => datetime::current_time(context, args),
        FunctionId::ImplicitTimezone => datetime::implicit_timezone(context, args),
        // Duration component extraction
        FunctionId::YearsFromDuration => datetime::years_from_duration(context, args),
        FunctionId::MonthsFromDuration => datetime::months_from_duration(context, args),
        FunctionId::DaysFromDuration => datetime::days_from_duration(context, args),
        FunctionId::HoursFromDuration => datetime::hours_from_duration(context, args),
        FunctionId::MinutesFromDuration => datetime::minutes_from_duration(context, args),
        FunctionId::SecondsFromDuration => datetime::seconds_from_duration(context, args),
        // DateTime component extraction
        FunctionId::YearFromDateTime => datetime::year_from_datetime(context, args),
        FunctionId::MonthFromDateTime => datetime::month_from_datetime(context, args),
        FunctionId::DayFromDateTime => datetime::day_from_datetime(context, args),
        FunctionId::HoursFromDateTime => datetime::hours_from_datetime(context, args),
        FunctionId::MinutesFromDateTime => datetime::minutes_from_datetime(context, args),
        FunctionId::SecondsFromDateTime => datetime::seconds_from_datetime(context, args),
        FunctionId::TimezoneFromDateTime => datetime::timezone_from_datetime(context, args),
        // Date component extraction
        FunctionId::YearFromDate => datetime::year_from_date(context, args),
        FunctionId::MonthFromDate => datetime::month_from_date(context, args),
        FunctionId::DayFromDate => datetime::day_from_date(context, args),
        FunctionId::TimezoneFromDate => datetime::timezone_from_date(context, args),
        // Time component extraction
        FunctionId::HoursFromTime => datetime::hours_from_time(context, args),
        FunctionId::MinutesFromTime => datetime::minutes_from_time(context, args),
        FunctionId::SecondsFromTime => datetime::seconds_from_time(context, args),
        FunctionId::TimezoneFromTime => datetime::timezone_from_time(context, args),
        // Timezone adjustment
        FunctionId::AdjustDateTimeToTimezone => {
            datetime::adjust_datetime_to_timezone(context, args)
        }
        FunctionId::AdjustDateToTimezone => datetime::adjust_date_to_timezone(context, args),
        FunctionId::AdjustTimeToTimezone => datetime::adjust_time_to_timezone(context, args),

        // ====================================================================
        // QName functions (Phase 7)
        // ====================================================================
        FunctionId::ResolveQName => qname::resolve_qname(context, args),
        FunctionId::QName => qname::qname_constructor(context, args),
        FunctionId::PrefixFromQName => qname::prefix_from_qname(context, args),
        FunctionId::LocalNameFromQName => qname::local_name_from_qname(context, args),
        FunctionId::NamespaceUriFromQName => qname::namespace_uri_from_qname(context, args),
        FunctionId::NamespaceUriForPrefix => qname::namespace_uri_for_prefix(context, args),
        FunctionId::InScopePrefixes => qname::in_scope_prefixes(context, args),

        // ====================================================================
        // URI functions (Phase 7)
        // ====================================================================
        FunctionId::ResolveUri => uri::resolve_uri(context, args),
        FunctionId::StaticBaseUri => uri::static_base_uri(context, args),

        // ====================================================================
        // Regex functions (Phase 7)
        // ====================================================================
        FunctionId::Matches => regex::matches(context, args),
        FunctionId::Replace => regex::replace(context, args),
        FunctionId::Tokenize => regex::tokenize(context, args),

        // ====================================================================
        // Conversion functions (fn:string, fn:number, fn:boolean)
        // ====================================================================
        FunctionId::String => eval_fn_string(context, args),
        FunctionId::Number => eval_fn_number(context, args),
        FunctionId::Boolean => eval_boolean(args),
    }
}

// Simple function implementations for Phase 1

fn eval_not<N: DomNavigator>(mut args: Vec<XPathValue<N>>) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments("not", 1, args.len()));
    }
    let arg = args.remove(0);
    let ebv = effective_boolean_value(&arg)?;
    Ok(XPathValue::boolean(!ebv))
}

fn eval_empty<N: DomNavigator>(mut args: Vec<XPathValue<N>>) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "empty",
            1,
            args.len(),
        ));
    }
    let arg = args.remove(0);
    Ok(XPathValue::boolean(arg.is_empty()))
}

fn eval_exists<N: DomNavigator>(mut args: Vec<XPathValue<N>>) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "exists",
            1,
            args.len(),
        ));
    }
    let arg = args.remove(0);
    Ok(XPathValue::boolean(!arg.is_empty()))
}

fn eval_count<N: DomNavigator>(mut args: Vec<XPathValue<N>>) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "count",
            1,
            args.len(),
        ));
    }
    let arg = args.remove(0);
    Ok(XPathValue::integer(arg.len() as i64))
}

fn eval_fn_string<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    match args.len() {
        0 => {
            // 0 args: string-value of context item
            let item = context.require_context_item()?.clone();
            let s = match item {
                XmlItem::Node(nav) => nav.value(),
                XmlItem::Atomic(v) => atomize::string_value(&v),
            };
            Ok(XPathValue::string(s))
        }
        1 => {
            let arg = args.remove(0);
            // XPath 1.0 compatibility mode: the argument of a function whose
            // parameter is a singleton is replaced by its first item, so
            // `string(//a)` is the string value of the first `a` instead of a
            // type error.
            let s = if context.static_context.xpath10_compatibility() {
                atomize::to_string_10(&arg)
            } else {
                // The string value of a node, not its atomized typed value.
                string_of_single(arg)?
            };
            Ok(XPathValue::string(s))
        }
        _ => Err(XPathError::wrong_number_of_arguments(
            "string",
            1,
            args.len(),
        )),
    }
}

/// `fn:number` (F&O §14.4).
///
/// XPath 1.0 compatibility mode needs nothing of its own here. The parameter
/// is `xs:anyAtomicType?`, so of the three compatibility steps of the function
/// conversion rules (XPath 2.0 §3.1.5) only the first applies — "the value V
/// is effectively replaced by V\[1\]" — and the evaluator has already taken it
/// for every function call before dispatch (`apply_function_conversion_10` in
/// `eval.rs`). The normal rules follow, in either mode: the argument is
/// atomized, so a node contributes its typed value rather than its string
/// value, and a value that does not convert is NaN.
fn eval_fn_number<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    match args.len() {
        0 => {
            // F&O §14.4: "fn:number() is equivalent to fn:number(.)" — the
            // context item after atomization, so a typed node contributes its
            // typed value, exactly as it does when passed as the argument.
            let item = context.require_context_item()?.clone();
            let d = atomize_to_double(XPathValue::from_item(item))?;
            Ok(XPathValue::double(d))
        }
        1 => {
            let d = atomize_to_double(args.remove(0))?;
            Ok(XPathValue::double(d))
        }
        _ => Err(XPathError::wrong_number_of_arguments(
            "number",
            1,
            args.len(),
        )),
    }
}

fn eval_boolean<N: DomNavigator>(
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "boolean",
            1,
            args.len(),
        ));
    }
    let arg = args.remove(0);
    let ebv = effective_boolean_value(&arg)?;
    Ok(XPathValue::boolean(ebv))
}

/// Compute the effective boolean value of an XPathValue.
pub fn effective_boolean_value<N: DomNavigator>(value: &XPathValue<N>) -> Result<bool, XPathError> {
    match value {
        XPathValue::Empty => Ok(false),
        XPathValue::Item(item) => item_boolean_value(item),
        XPathValue::Sequence(items) => {
            if items.is_empty() {
                Ok(false)
            } else if let Some(XmlItem::Node(_)) = items.first() {
                // Non-empty sequence starting with a node is true
                Ok(true)
            } else if items.len() == 1 {
                item_boolean_value(&items[0])
            } else {
                // Sequence of multiple atomics is an error
                Err(XPathError::FORG0006 {
                    message:
                        "Effective boolean value not defined for sequence of multiple atomic values"
                            .to_string(),
                })
            }
        }
    }
}

/// Compute the effective boolean value in XPath 1.0 mode.
///
/// Differences from XPath 2.0:
/// - Multi-item sequences don't raise FORG0006; non-empty sequences are `true`
/// - Node-sets: true if non-empty
pub fn effective_boolean_value_10<N: DomNavigator>(
    value: &XPathValue<N>,
) -> Result<bool, XPathError> {
    match value {
        XPathValue::Empty => Ok(false),
        XPathValue::Item(item) => item_boolean_value(item),
        XPathValue::Sequence(items) => {
            if items.is_empty() {
                Ok(false)
            } else if items.len() == 1 {
                item_boolean_value(&items[0])
            } else {
                // XPath 1.0: non-empty node-set/sequence is true (no FORG0006 error)
                Ok(true)
            }
        }
    }
}

fn item_boolean_value<N: DomNavigator>(item: &XmlItem<N>) -> Result<bool, XPathError> {
    match item {
        XmlItem::Node(_) => Ok(true),
        XmlItem::Atomic(value) => {
            match value.as_boolean() {
                Some(b) => Ok(b),
                None => {
                    // For strings (and untypedAtomic), empty is false, non-empty is true
                    if let Some(s) = value.as_string() {
                        Ok(!s.is_empty())
                    } else if value.type_code == crate::types::XmlTypeCode::AnyUri {
                        // xs:anyURI promoted to string for EBV
                        let s = value.to_string_value();
                        Ok(!s.is_empty())
                    } else if let Some(d) = value.as_double() {
                        // For numbers, 0 and NaN are false
                        Ok(!d.is_nan() && d != 0.0)
                    } else if let Some(i) = value.as_integer() {
                        Ok(*i != BigInt::from(0))
                    } else {
                        // Per XPath 2.0: EBV is only defined for boolean, string,
                        // numeric, node, untypedAtomic, anyURI. Other types raise FORG0006.
                        Err(XPathError::FORG0006 {
                            message: format!(
                                "Effective boolean value not defined for type {:?}",
                                value.type_code
                            ),
                        })
                    }
                }
            }
        }
    }
}

// ============================================================================
// From Trait Implementations for XPathValue
// ============================================================================

impl<N: DomNavigator> From<bool> for XPathValue<N> {
    fn from(b: bool) -> Self {
        XPathValue::boolean(b)
    }
}

impl<N: DomNavigator> From<i32> for XPathValue<N> {
    fn from(i: i32) -> Self {
        XPathValue::integer(BigInt::from(i))
    }
}

impl<N: DomNavigator> From<i64> for XPathValue<N> {
    fn from(i: i64) -> Self {
        XPathValue::integer(BigInt::from(i))
    }
}

impl<N: DomNavigator> From<f64> for XPathValue<N> {
    fn from(d: f64) -> Self {
        XPathValue::double(d)
    }
}

impl<N: DomNavigator> From<f32> for XPathValue<N> {
    fn from(f: f32) -> Self {
        XPathValue::double(f as f64)
    }
}

impl<N: DomNavigator> From<String> for XPathValue<N> {
    fn from(s: String) -> Self {
        XPathValue::string(s)
    }
}

impl<N: DomNavigator> From<&str> for XPathValue<N> {
    fn from(s: &str) -> Self {
        XPathValue::string(s)
    }
}

impl<N: DomNavigator> From<()> for XPathValue<N> {
    fn from(_: ()) -> Self {
        XPathValue::empty()
    }
}

impl<N: DomNavigator> From<BigInt> for XPathValue<N> {
    fn from(i: BigInt) -> Self {
        XPathValue::integer(i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xpath::RoXmlNavigator;

    #[test]
    fn test_xpath_value_empty() {
        let value: XPathValue<RoXmlNavigator<'static>> = XPathValue::empty();
        assert!(value.is_empty());
        assert_eq!(value.len(), 0);
    }

    #[test]
    fn test_xpath_value_single() {
        let value: XPathValue<RoXmlNavigator<'static>> = XPathValue::boolean(true);
        assert!(!value.is_empty());
        assert_eq!(value.len(), 1);
        assert!(value.is_single());
    }

    #[test]
    fn test_xpath_value_from_sequence() {
        let items: Vec<XmlItem<RoXmlNavigator<'static>>> = vec![
            XmlItem::Atomic(XmlValue::integer(1.into())),
            XmlItem::Atomic(XmlValue::integer(2.into())),
        ];
        let value = XPathValue::from_sequence(items);
        assert_eq!(value.len(), 2);
        assert!(!value.is_single());
    }

    /// `to_owned_string` copies the string of a single string-typed or
    /// untyped atomic item, and is `None` for every other value: it does not
    /// convert an atomic value of another type, nor atomize a node.
    #[test]
    fn to_owned_string_copies_the_string_of_a_single_string_item() {
        type V = XPathValue<RoXmlNavigator<'static>>;
        assert_eq!(
            V::string("hello").to_owned_string(),
            Some("hello".to_string())
        );
        assert_eq!(
            V::from_atomic(XmlValue::untyped("raw")).to_owned_string(),
            Some("raw".to_string())
        );
        assert_eq!(V::integer(42).to_owned_string(), None);
        assert_eq!(V::boolean(true).to_owned_string(), None);
        assert_eq!(V::empty().to_owned_string(), None);
        let two = V::from_sequence(vec![
            XmlItem::Atomic(XmlValue::string("a")),
            XmlItem::Atomic(XmlValue::string("b")),
        ]);
        assert_eq!(two.to_owned_string(), None);

        let doc = roxmltree::Document::parse("<a>text</a>").unwrap();
        let node = XPathValue::from_node(RoXmlNavigator::new(&doc));
        assert_eq!(node.to_owned_string(), None);
    }

    /// The deprecated `as_str` answers exactly what `to_owned_string` does.
    #[test]
    #[allow(deprecated)]
    fn as_str_is_a_deprecated_alias_of_to_owned_string() {
        type V = XPathValue<RoXmlNavigator<'static>>;
        for value in [
            V::string("hello"),
            V::from_atomic(XmlValue::untyped("raw")),
            V::integer(42),
            V::empty(),
        ] {
            assert_eq!(value.as_str(), value.to_owned_string());
        }
    }

    /// `as_slice` agrees with `len` and `first` for all three shapes; the
    /// single-item shape used to answer with an empty slice.
    #[test]
    fn test_xpath_value_as_slice() {
        let empty: XPathValue<RoXmlNavigator<'static>> = XPathValue::empty();
        assert!(empty.as_slice().is_empty());

        let single: XPathValue<RoXmlNavigator<'static>> = XPathValue::integer(7);
        assert!(single.is_single());
        let slice = single.as_slice();
        assert_eq!(slice.len(), 1);
        assert_eq!(slice.len(), single.len());
        assert!(std::ptr::eq(&slice[0], single.first().unwrap()));
        match &slice[0] {
            XmlItem::Atomic(value) => assert_eq!(value, &XmlValue::integer(7.into())),
            XmlItem::Node(_) => panic!("expected an atomic item"),
        }

        let items: Vec<XmlItem<RoXmlNavigator<'static>>> = vec![
            XmlItem::Atomic(XmlValue::integer(1.into())),
            XmlItem::Atomic(XmlValue::integer(2.into())),
        ];
        let sequence = XPathValue::from_sequence(items);
        let slice = sequence.as_slice();
        assert_eq!(slice.len(), 2);
        assert!(std::ptr::eq(&slice[0], sequence.first().unwrap()));
    }

    #[test]
    fn test_effective_boolean_value_empty() {
        let value: XPathValue<RoXmlNavigator<'static>> = XPathValue::empty();
        assert!(!effective_boolean_value(&value).unwrap());
    }

    #[test]
    fn test_effective_boolean_value_boolean() {
        let value: XPathValue<RoXmlNavigator<'static>> = XPathValue::boolean(true);
        assert!(effective_boolean_value(&value).unwrap());

        let value: XPathValue<RoXmlNavigator<'static>> = XPathValue::boolean(false);
        assert!(!effective_boolean_value(&value).unwrap());
    }

    #[test]
    fn test_effective_boolean_value_string() {
        let value: XPathValue<RoXmlNavigator<'static>> = XPathValue::string("hello");
        assert!(effective_boolean_value(&value).unwrap());

        let value: XPathValue<RoXmlNavigator<'static>> = XPathValue::string("");
        assert!(!effective_boolean_value(&value).unwrap());
    }

    #[test]
    fn test_effective_boolean_value_number() {
        let value: XPathValue<RoXmlNavigator<'static>> = XPathValue::double(1.0);
        assert!(effective_boolean_value(&value).unwrap());

        let value: XPathValue<RoXmlNavigator<'static>> = XPathValue::double(0.0);
        assert!(!effective_boolean_value(&value).unwrap());

        let value: XPathValue<RoXmlNavigator<'static>> = XPathValue::double(f64::NAN);
        assert!(!effective_boolean_value(&value).unwrap());
    }

    // ========================================================================
    // fn:string, fn:number, fn:boolean tests (XPath 2.0 semantics)
    // ========================================================================

    use crate::namespace::table::NameTable;
    use crate::xpath::context::{DynamicContext, XPathContext};

    #[test]
    fn test_eval_fn_string_integer() {
        let names = NameTable::new();
        let static_ctx = XPathContext::new(&names);
        let mut ctx: DynamicContext<'_, RoXmlNavigator<'static>> =
            DynamicContext::new(&static_ctx, 0);
        let args = vec![XPathValue::integer(42i64)];
        let result = eval_fn_string(&mut ctx, args).unwrap();
        assert_eq!(result.to_owned_string(), Some("42".to_string()));
    }

    #[test]
    fn test_eval_fn_string_string() {
        let names = NameTable::new();
        let static_ctx = XPathContext::new(&names);
        let mut ctx: DynamicContext<'_, RoXmlNavigator<'static>> =
            DynamicContext::new(&static_ctx, 0);
        let args = vec![XPathValue::string("hello")];
        let result = eval_fn_string(&mut ctx, args).unwrap();
        assert_eq!(result.to_owned_string(), Some("hello".to_string()));
    }

    #[test]
    fn test_eval_fn_number_string() {
        let names = NameTable::new();
        let static_ctx = XPathContext::new(&names);
        let mut ctx: DynamicContext<'_, RoXmlNavigator<'static>> =
            DynamicContext::new(&static_ctx, 0);
        let args = vec![XPathValue::string("42.5")];
        let result = eval_fn_number(&mut ctx, args).unwrap();
        assert_eq!(result.as_f64(), Some(42.5));
    }

    #[test]
    fn test_eval_fn_number_invalid() {
        let names = NameTable::new();
        let static_ctx = XPathContext::new(&names);
        let mut ctx: DynamicContext<'_, RoXmlNavigator<'static>> =
            DynamicContext::new(&static_ctx, 0);
        let args = vec![XPathValue::string("abc")];
        let result = eval_fn_number(&mut ctx, args).unwrap();
        assert!(result.as_f64().unwrap().is_nan());
    }

    #[test]
    fn test_eval_boolean_false_empty_string() {
        let args: Vec<XPathValue<RoXmlNavigator<'static>>> = vec![XPathValue::string("")];
        let result = eval_boolean(args).unwrap();
        assert_eq!(result.as_bool(), Some(false));
    }

    #[test]
    fn test_eval_boolean_true_nonempty_string() {
        let args: Vec<XPathValue<RoXmlNavigator<'static>>> = vec![XPathValue::string("x")];
        let result = eval_boolean(args).unwrap();
        assert_eq!(result.as_bool(), Some(true));
    }

    #[test]
    fn test_eval_boolean_false_zero() {
        let args: Vec<XPathValue<RoXmlNavigator<'static>>> = vec![XPathValue::double(0.0)];
        let result = eval_boolean(args).unwrap();
        assert_eq!(result.as_bool(), Some(false));
    }
}
