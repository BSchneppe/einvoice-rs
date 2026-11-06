//! Atomization operations for XPath evaluation.
//!
//! This module implements XPath 2.0 atomization rules for converting
//! values to their atomic representations.
//!
//! ## Atomization Rules
//!
//! Atomization extracts atomic values from items:
//!
//! - For atomic values, returns the value itself
//! - For nodes, returns the typed value of the node
//! - For empty sequences, returns None
//! - For sequences with more than one item, raises XPTY0004

use super::error::XPathError;
use super::functions::XPathValue;
use super::iterator::XmlItem;
use super::{DomNavigator, DomNodeType};
use crate::navigator::TypedValue;
use crate::types::value::{XmlAtomicValue, XmlValue, XmlValueKind};
use crate::types::XmlTypeCode;

/// Atomize a navigator node to its XDM atomic value.
///
/// Interprets [`TypedValue`] with proper error handling:
/// - `Value(v)` → `Ok(Some(v))`, a union's value as the value of its member
///   type (below)
/// - `Untyped` → `Ok(Some(untypedAtomic(string-value)))` (or `xs:string` for a
///   comment, processing instruction or namespace node — XPath 2.0 §I.2)
/// - `Nilled` → `Ok(None)` (empty sequence)
/// - `Absent` → `Err(FOTY0012)`
///
/// The typed value of a node whose type is a list type comes back as it is
/// stored: one value of kind [`XmlValueKind::List`] holding every member. The
/// XPath operators and functions — `fn:data`, the comparisons, arithmetic, the
/// function conversion rules — see it as the sequence of its members instead,
/// one atomic value per member, each of the type that validated it: the list's
/// item type, or, for an item type that is a union, the member type of the
/// union that accepted that member (XDM 1.0 §3.3.1.2; XPath 2.0 §2.5.2: "The
/// typed value of a node is never treated as an instance of a named list
/// type").
///
/// The typed value of a node whose type is a union type comes back from the
/// navigator as the union's value holding the value of the member type that
/// validated it ([`XmlValueKind::Union`]); XPath sees the member's value
/// itself, with the member type as its type annotation. XDM 1.0 §3.3.1.2: the
/// typed value is M applied to the string value "as an instance of the
/// appropriate value type, where the appropriate value type is the [member
/// type definition] if T is a union type"; XPath 2.0 §2.5.2: "when the type
/// annotation of a node is a union type, the typed value of the node will be
/// an instance of one of the member types of the union". So `data($n)
/// instance of my:member` holds for a user-defined member type, as it does
/// for a built-in one.
pub fn atomize_node<N: DomNavigator>(nav: &N) -> Result<Option<XmlValue>, XPathError> {
    match nav.typed_value() {
        TypedValue::Value(v) => Ok(Some(unwrap_union_owned(v))),
        TypedValue::Untyped => {
            let v = match nav.node_type() {
                // XPath 2.0 §I.2 (Incompatibilities when Compatibility Mode is
                // false): "The typed value of a comment node, processing
                // instruction node, or namespace node under XPath 2.0 is of
                // type xs:string, not xs:untypedAtomic." None of these three
                // kinds can carry a type annotation, so `Untyped` here means
                // "has no annotation", not "annotated xs:untyped".
                DomNodeType::Comment
                | DomNodeType::ProcessingInstruction
                | DomNodeType::Namespace => XmlValue::string(nav.value()),
                _ => XmlValue::untyped(nav.value()),
            };
            Ok(Some(v))
        }
        TypedValue::Nilled => Ok(None),
        TypedValue::Absent => Err(XPathError::no_typed_value()),
    }
}

// ============================================================================
// The sequence of atomic values a value stands for
// ============================================================================

/// The members of a packed list value, one atomic value each, in order.
///
/// A typed value of kind [`XmlValueKind::List`] is how the validator stores
/// the typed value of a node whose type is a list type; XPath sees it as the
/// sequence of its members (XDM 1.0 §3.3.1.2, XPath 2.0 §2.5.2). Each member
/// is the value the list stores: of the type that validated it.
#[derive(Debug)]
pub(crate) struct ListMembers {
    items: std::vec::IntoIter<XmlValue>,
}

impl Iterator for ListMembers {
    type Item = XmlValue;

    #[inline]
    fn next(&mut self) -> Option<XmlValue> {
        self.items.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.items.size_hint()
    }
}

impl ExactSizeIterator for ListMembers {}

impl ListMembers {
    /// At most one member: `None` for an empty list, the member for one, and
    /// the type error `XPTY0004` for more — the rule of every operand and
    /// parameter that takes an optional single atomic value (XPath 2.0 §3.1.5,
    /// §3.4, §3.5.1).
    pub(crate) fn at_most_one(mut self) -> Result<Option<XmlValue>, XPathError> {
        let count = self.len();
        if count > 1 {
            return Err(XPathError::type_mismatch(
                "a single atomic value",
                format!("a typed value of {count} list members"),
            ));
        }
        Ok(self.next())
    }
}

/// Split a packed list value into its members.
///
/// This is the single place where XPath atomization unpacks a list, so every
/// path that atomizes — `fn:data`, the general and value comparisons and their
/// hash index, arithmetic, the function conversion rules, `fn:deep-equal` —
/// sees the same members. `Ok` for a value of kind [`XmlValueKind::List`], and
/// for a union value whose chosen member type is a list type (a `List` inside
/// [`XmlValueKind::Union`]); every other value is handed back unchanged in
/// `Err`, after a single discriminant test for an atomic value.
#[inline]
pub(crate) fn unpack_list(value: XmlValue) -> Result<ListMembers, XmlValue> {
    match value {
        XmlValue {
            value: XmlValueKind::List { items, .. },
            ..
        } => Ok(ListMembers {
            items: items.into_iter(),
        }),
        XmlValue {
            value: XmlValueKind::Union(inner),
            ..
        } if holds_list(&inner) => unpack_list(*inner),
        other => Err(other),
    }
}

/// Whether `value` is a packed list, possibly inside unions — i.e. whether
/// [`unpack_list`] would take it apart. One discriminant test for an atomic
/// value, so a hot loop can keep its single-value path exactly as it was.
#[inline]
pub(crate) fn is_packed_list(value: &XmlValue) -> bool {
    match &value.value {
        XmlValueKind::Atomic(_) | XmlValueKind::UntypedAtomic(_) => false,
        _ => holds_list(value),
    }
}

/// Whether `value` is a packed list, possibly inside unions.
fn holds_list(value: &XmlValue) -> bool {
    match &value.value {
        XmlValueKind::List { .. } => true,
        XmlValueKind::Union(inner) => holds_list(inner),
        _ => false,
    }
}

/// The atomic values one item atomizes to (XPath 2.0 §2.4.2): none (a nilled
/// element, an empty list), one, or the members of a list.
#[derive(Debug)]
pub(crate) enum Atoms {
    /// No value, or exactly one value that is not a list.
    One(Option<XmlValue>),
    /// The members of a list.
    Members(ListMembers),
}

impl Atoms {
    /// The atomic values `value` stands for: its members if it is a packed
    /// list (see [`unpack_list`]), otherwise `value` itself.
    #[inline]
    pub(crate) fn of(value: XmlValue) -> Self {
        match unpack_list(value) {
            Ok(members) => Atoms::Members(members),
            Err(value) => Atoms::One(Some(value)),
        }
    }
}

impl Iterator for Atoms {
    type Item = XmlValue;

    #[inline]
    fn next(&mut self) -> Option<XmlValue> {
        match self {
            Atoms::One(value) => value.take(),
            Atoms::Members(members) => members.next(),
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            Atoms::One(value) => {
                let n = usize::from(value.is_some());
                (n, Some(n))
            }
            Atoms::Members(members) => members.size_hint(),
        }
    }
}

impl ExactSizeIterator for Atoms {}

/// Atomize a node into the atomic values of its typed value: [`atomize_node`]
/// followed by [`Atoms::of`], so a list-typed node yields its members and a
/// nilled element none.
#[inline]
pub(crate) fn node_atoms<N: DomNavigator>(nav: &N) -> Result<Atoms, XPathError> {
    Ok(match atomize_node(nav)? {
        Some(value) => Atoms::of(value),
        None => Atoms::One(None),
    })
}

/// The single atomic value `value` stands for: `value` itself if it is not a
/// packed list, otherwise its only member — `None` for an empty list, and
/// `XPTY0004` for more than one member (see [`ListMembers::at_most_one`]).
#[inline]
pub(crate) fn single_atom(value: XmlValue) -> Result<Option<XmlValue>, XPathError> {
    if !is_packed_list(&value) {
        return Ok(Some(value));
    }
    match unpack_list(value) {
        Ok(members) => members.at_most_one(),
        Err(value) => Ok(Some(value)),
    }
}

/// Append the atomic values `value` stands for to `out`: its members if it is
/// a packed list (see [`unpack_list`]), otherwise `value` itself, pushed
/// directly.
#[inline]
pub(crate) fn push_atoms(value: XmlValue, out: &mut Vec<XmlValue>) {
    if !is_packed_list(&value) {
        out.push(value);
        return;
    }
    match unpack_list(value) {
        Ok(members) => out.extend(members),
        Err(value) => out.push(value),
    }
}

/// Atomize an XmlValue, returning its atomic representation.
///
/// For atomic values, this returns a clone of the value.
/// For union values, this unwraps and atomizes the inner value.
/// For list values, this returns an error (multiple items).
///
/// # Arguments
///
/// * `value` - The value to atomize
///
/// # Returns
///
/// * `Ok(XmlValue)` - The atomized value
/// * `Err(XPathError)` - If atomization fails
pub fn atomize(value: &XmlValue) -> Result<XmlValue, XPathError> {
    match &value.value {
        // Atomic values return themselves
        XmlValueKind::Atomic(_) | XmlValueKind::UntypedAtomic(_) => Ok(value.clone()),

        // Union: unwrap and atomize
        XmlValueKind::Union(inner) => atomize(inner),

        // List values represent multiple items - error
        XmlValueKind::List { items, .. } if items.len() > 1 => {
            Err(XPathError::more_than_one_item())
        }

        // Single-item list: return the item
        XmlValueKind::List { items, .. } if items.len() == 1 => Ok(items[0].clone()),

        // Empty list: conceptually empty sequence
        XmlValueKind::List { .. } => Err(XPathError::type_mismatch("item()", "empty-sequence()")),
    }
}

/// Atomize an optional value.
///
/// Returns None for None (empty sequence), otherwise atomizes the value.
///
/// # Arguments
///
/// * `value` - Optional value to atomize
///
/// # Returns
///
/// * `Ok(None)` - If input is None (empty sequence)
/// * `Ok(Some(XmlValue))` - The atomized value
/// * `Err(XPathError)` - If atomization fails
pub fn atomize_opt(value: Option<&XmlValue>) -> Result<Option<XmlValue>, XPathError> {
    match value {
        None => Ok(None),
        Some(v) => atomize(v).map(Some),
    }
}

/// Atomize a value, requiring a non-empty result.
///
/// This is equivalent to `Atomize<T>` in C# - it requires the result to exist.
///
/// # Arguments
///
/// * `value` - Optional value to atomize
///
/// # Returns
///
/// * `Ok(XmlValue)` - The atomized value
/// * `Err(XPathError)` - XPTY0004 if empty, or other atomization errors
pub fn atomize_required(value: Option<&XmlValue>) -> Result<XmlValue, XPathError> {
    match value {
        None => Err(XPathError::type_mismatch("item()", "empty-sequence()")),
        Some(v) => atomize(v),
    }
}

/// Get the string value of an XmlValue.
///
/// For atomic values, this returns the canonical string representation.
/// For union values, this unwraps and gets the string value.
/// For list values, this joins the item strings with spaces.
///
/// # Arguments
///
/// * `value` - The value to convert to string
///
/// # Returns
///
/// The string representation of the value
pub fn string_value(value: &XmlValue) -> String {
    value.to_string_value()
}

/// Get the string value of an optional value.
///
/// Returns empty string for None (empty sequence).
///
/// # Arguments
///
/// * `value` - Optional value to convert
///
/// # Returns
///
/// The string representation, or empty string for None
pub fn string_value_opt(value: Option<&XmlValue>) -> String {
    match value {
        None => String::new(),
        Some(v) => string_value(v),
    }
}

/// Convert a value to a double (numeric).
///
/// Implements the fn:number() behavior:
/// - Returns NaN for invalid conversions
/// - Handles UntypedAtomic by parsing as double
/// - Handles numeric types by conversion
///
/// # Arguments
///
/// * `value` - The value to convert
///
/// # Returns
///
/// The numeric value as f64, or NaN if conversion fails
pub fn to_number(value: &XmlValue) -> f64 {
    match &value.value {
        XmlValueKind::Atomic(atom) => atomic_to_number(atom),
        XmlValueKind::UntypedAtomic(s) => {
            crate::xpath::cast::parse_xsd_double(s).unwrap_or(f64::NAN)
        }
        XmlValueKind::Union(inner) => to_number(inner),
        XmlValueKind::List { .. } => f64::NAN,
    }
}

/// Convert an atomic value to a double.
fn atomic_to_number(atom: &XmlAtomicValue) -> f64 {
    match atom {
        XmlAtomicValue::Double(d) => *d,
        XmlAtomicValue::Float(f) => *f as f64,
        XmlAtomicValue::Decimal(d) => d.to_string().parse().unwrap_or(f64::NAN),
        XmlAtomicValue::Integer(i) => i.to_string().parse().unwrap_or(f64::NAN),
        XmlAtomicValue::Boolean(b) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        XmlAtomicValue::String(s) => crate::xpath::cast::parse_xsd_double(s).unwrap_or(f64::NAN),
        _ => f64::NAN,
    }
}

/// Convert an optional value to a double.
///
/// Returns NaN for None (empty sequence).
pub fn to_number_opt(value: Option<&XmlValue>) -> f64 {
    match value {
        None => f64::NAN,
        Some(v) => to_number(v),
    }
}

/// Check if a value is empty (represents an empty sequence).
///
/// Note: XmlValue itself doesn't have an "empty" variant.
/// This checks for empty lists or None optionals.
pub fn is_empty_list(value: &XmlValue) -> bool {
    matches!(&value.value, XmlValueKind::List { items, .. } if items.is_empty())
}

/// Get the type code of the underlying atomic value.
///
/// For union types, returns the type code of the actual member type.
pub fn effective_type_code(value: &XmlValue) -> XmlTypeCode {
    match &value.value {
        XmlValueKind::Union(inner) => effective_type_code(inner),
        _ => value.type_code,
    }
}

/// Check if a value is a node (in XPath terms).
///
/// Returns true if the type code indicates a node type.
pub fn is_node_type(type_code: XmlTypeCode) -> bool {
    matches!(
        type_code,
        XmlTypeCode::Node
            | XmlTypeCode::Document
            | XmlTypeCode::Element
            | XmlTypeCode::Attribute
            | XmlTypeCode::Namespace
            | XmlTypeCode::ProcessingInstruction
            | XmlTypeCode::Comment
            | XmlTypeCode::Text
    )
}

/// Check if a value represents a node.
pub fn is_node(value: &XmlValue) -> bool {
    is_node_type(effective_type_code(value))
}

/// Unwrap a union value to its member value.
///
/// Recursively unwraps nested unions.
pub fn unwrap_union(value: &XmlValue) -> &XmlValue {
    match &value.value {
        XmlValueKind::Union(inner) => unwrap_union(inner),
        _ => value,
    }
}

/// [`unwrap_union`] by value: the member value of a union value, moved out
/// rather than cloned; any other value is returned as it is.
#[inline]
pub(crate) fn unwrap_union_owned(value: XmlValue) -> XmlValue {
    match value {
        XmlValue {
            value: XmlValueKind::Union(inner),
            ..
        } => unwrap_union_owned(*inner),
        other => other,
    }
}

/// Extract the string value of the first node in an XPathValue (XPath 1.0 rule).
///
/// In XPath 1.0, converting a node-set to string returns the string-value
/// of the first node in document order, or "" if empty.
/// For atomic values, delegates to the standard string conversion.
pub(crate) fn first_node_string_value<N: DomNavigator>(value: &XPathValue<N>) -> String {
    match value {
        XPathValue::Empty => String::new(),
        XPathValue::Item(XmlItem::Node(n)) => n.value(),
        XPathValue::Item(XmlItem::Atomic(v)) => v.to_string_value(),
        XPathValue::Sequence(items) => {
            // Find the document-order-first node in a single pass
            let mut first_node: Option<&N> = None;
            for item in items {
                if let XmlItem::Node(n) = item {
                    if let Some(current) = first_node {
                        if crate::xpath::node_ops::compare_document_order(n, current)
                            == std::cmp::Ordering::Less
                        {
                            first_node = Some(n);
                        }
                    } else {
                        first_node = Some(n);
                    }
                }
            }
            if let Some(n) = first_node {
                return n.value();
            }
            // Fallback: if no nodes, use first atomic's string value
            if let Some(XmlItem::Atomic(v)) = items.first() {
                v.to_string_value()
            } else {
                String::new()
            }
        }
    }
}

/// Convert an XPathValue to string using XPath 1.0 rules.
///
/// Same as `first_node_string_value` — for node-sets, uses first node.
/// For atomics, uses canonical string form.
pub(crate) fn to_string_10<N: DomNavigator>(value: &XPathValue<N>) -> String {
    first_node_string_value(value)
}

/// Convert an XPathValue to number using XPath 1.0 rules.
///
/// Converts to string first (via `to_string_10`), then parses it as XPath 1.0
/// `number()` does ([`xpath10_string_to_number`]).
pub(crate) fn to_number_10<N: DomNavigator>(value: &XPathValue<N>) -> f64 {
    match value {
        XPathValue::Empty => f64::NAN,
        XPathValue::Item(XmlItem::Atomic(v)) => to_number_xpath10(v),
        _ => xpath10_string_to_number(&to_string_10(value)),
    }
}

/// XPath 1.0 `number()` of one atomic value: a number is itself, a boolean
/// `1` or `0`, and anything else is its string value read by
/// [`xpath10_string_to_number`].
pub(crate) fn to_number_xpath10(value: &XmlValue) -> f64 {
    let code = value.type_code;
    if code.is_numeric() || code == crate::types::XmlTypeCode::Boolean {
        to_number(value)
    } else {
        xpath10_string_to_number(&value.to_string_value())
    }
}

/// XPath 1.0 §4.4 `number()` of a string: "a string that consists of
/// optional whitespace followed by an optional minus sign followed by a Number
/// followed by whitespace is converted to the IEEE 754 number that is nearest
/// (according to the IEEE 754 round-to-nearest rule) to the mathematical value
/// represented by the string; any other string is converted to NaN", with
/// §3.7 `Number ::= Digits ('.' Digits?)? | '.' Digits` and whitespace the
/// four characters of `ExprWhitespace`. No exponent, no `+`, no `INF` — those
/// are XPath 2.0's `xs:double` lexical forms, not XPath 1.0's.
pub(crate) fn xpath10_string_to_number(s: &str) -> f64 {
    let trimmed = s.trim_matches(|c| matches!(c, ' ' | '\t' | '\r' | '\n'));
    let (negative, number) = match trimmed.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, trimmed),
    };
    let mut digits = 0usize;
    let mut points = 0usize;
    for b in number.bytes() {
        match b {
            b'0'..=b'9' => digits += 1,
            b'.' => points += 1,
            _ => return f64::NAN,
        }
    }
    if digits == 0 || points > 1 {
        return f64::NAN;
    }
    // Rust's `f64` parser rounds to nearest, and accepts `5.` and `.5`.
    match number.parse::<f64>() {
        Ok(value) if negative => -value,
        Ok(value) => value,
        Err(_) => f64::NAN,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_bigint::BigInt;
    use rust_decimal::Decimal;

    #[test]
    fn test_atomize_atomic() {
        let value = XmlValue::string("hello");
        let result = atomize(&value).unwrap();
        assert_eq!(result.to_string_value(), "hello");
    }

    #[test]
    fn test_atomize_untyped() {
        let value = XmlValue::untyped("test");
        let result = atomize(&value).unwrap();
        assert_eq!(result.to_string_value(), "test");
    }

    #[test]
    fn test_atomize_opt_none() {
        let result = atomize_opt(None).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_atomize_opt_some() {
        let value = XmlValue::integer(BigInt::from(42));
        let result = atomize_opt(Some(&value)).unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_atomize_required_none() {
        let result = atomize_required(None);
        assert!(result.is_err());
        if let Err(XPathError::XPTY0004 { .. }) = result {
            // Expected
        } else {
            panic!("Expected XPTY0004 error");
        }
    }

    #[test]
    fn test_string_value() {
        assert_eq!(string_value(&XmlValue::string("hello")), "hello");
        assert_eq!(string_value(&XmlValue::boolean(true)), "true");
        assert_eq!(string_value(&XmlValue::integer(BigInt::from(123))), "123");
    }

    #[test]
    fn test_string_value_opt_none() {
        assert_eq!(string_value_opt(None), "");
    }

    #[test]
    fn test_to_number() {
        assert_eq!(to_number(&XmlValue::double(2.5)), 2.5);
        assert_eq!(to_number(&XmlValue::float(2.5)), 2.5);
        assert_eq!(to_number(&XmlValue::integer(BigInt::from(42))), 42.0);
        assert_eq!(to_number(&XmlValue::decimal(Decimal::new(125, 2))), 1.25);
        assert_eq!(to_number(&XmlValue::string("2.5")), 2.5);
        assert!(to_number(&XmlValue::string("not a number")).is_nan());
    }

    #[test]
    fn test_to_number_opt_none() {
        assert!(to_number_opt(None).is_nan());
    }

    #[test]
    fn test_to_number_untyped() {
        assert_eq!(to_number(&XmlValue::untyped("42.5")), 42.5);
        assert_eq!(to_number(&XmlValue::untyped("  2.5  ")), 2.5); // Trimmed
    }

    #[test]
    fn test_effective_type_code() {
        let value = XmlValue::string("test");
        assert_eq!(effective_type_code(&value), XmlTypeCode::String);

        let value = XmlValue::integer(BigInt::from(1));
        assert_eq!(effective_type_code(&value), XmlTypeCode::Integer);
    }

    #[test]
    fn test_is_node_type() {
        assert!(is_node_type(XmlTypeCode::Element));
        assert!(is_node_type(XmlTypeCode::Attribute));
        assert!(is_node_type(XmlTypeCode::Document));
        assert!(!is_node_type(XmlTypeCode::String));
        assert!(!is_node_type(XmlTypeCode::Integer));
    }

    #[test]
    fn test_is_node() {
        // Atomic values are not nodes
        assert!(!is_node(&XmlValue::string("test")));
        assert!(!is_node(&XmlValue::integer(BigInt::from(1))));

        // A value with node type code would be a node
        // (We can't easily create one without a navigator, but we test the type check)
        let node_value = XmlValue::new(
            XmlTypeCode::Element,
            XmlValueKind::UntypedAtomic("element content".to_string()),
        );
        assert!(is_node(&node_value));
    }

    // --- The members of a packed list value ---

    fn id(member: &str) -> XmlValue {
        XmlValue::new(
            XmlTypeCode::Id,
            XmlValueKind::Atomic(XmlAtomicValue::String(member.to_string())),
        )
    }

    fn id_list(members: &[&str]) -> XmlValue {
        XmlValue::new(
            XmlTypeCode::Id,
            XmlValueKind::List {
                item_type: XmlTypeCode::Id,
                items: members.iter().map(|m| id(m)).collect(),
            },
        )
    }

    fn describe(atoms: Atoms) -> Vec<(XmlTypeCode, String)> {
        atoms.map(|v| (v.type_code, v.to_string_value())).collect()
    }

    #[test]
    fn a_packed_list_unpacks_into_its_members() {
        let members = unpack_list(id_list(&["a", "b"])).expect("a list");
        assert_eq!(members.len(), 2);
        let members: Vec<XmlValue> = members.collect();
        assert_eq!(members[0], id("a"));
        assert_eq!(members[1].to_string_value(), "b");

        // A built-in list type carries its own list code; the members carry
        // the item type.
        let nmtokens = XmlValue::new(
            XmlTypeCode::NmTokens,
            XmlValueKind::List {
                item_type: XmlTypeCode::NmToken,
                items: vec![XmlValue::new(
                    XmlTypeCode::NmToken,
                    XmlValueKind::Atomic(XmlAtomicValue::String("p".into())),
                )],
            },
        );
        assert_eq!(
            describe(Atoms::of(nmtokens)),
            [(XmlTypeCode::NmToken, "p".to_string())]
        );
    }

    #[test]
    fn a_union_whose_member_is_a_list_unpacks_too() {
        let union = XmlValue::new(
            XmlTypeCode::Id,
            XmlValueKind::Union(Box::new(id_list(&["x", "y"]))),
        );
        assert_eq!(
            describe(Atoms::of(union)),
            [
                (XmlTypeCode::Id, "x".to_string()),
                (XmlTypeCode::Id, "y".to_string())
            ]
        );
    }

    #[test]
    fn any_other_value_is_handed_back_unchanged() {
        let atomic = XmlValue::integer(BigInt::from(7));
        assert_eq!(unpack_list(atomic.clone()).expect_err("not a list"), atomic);
        let untyped = XmlValue::untyped("a b");
        assert_eq!(
            unpack_list(untyped.clone()).expect_err("not a list"),
            untyped
        );
        let union = XmlValue::new(
            XmlTypeCode::String,
            XmlValueKind::Union(Box::new(XmlValue::string("s"))),
        );
        assert_eq!(unpack_list(union.clone()).expect_err("not a list"), union);
        assert_eq!(Atoms::of(atomic.clone()).collect::<Vec<_>>(), [atomic]);
    }

    #[test]
    fn at_most_one_member() {
        assert_eq!(single_atom(id_list(&[])).unwrap(), None);
        assert_eq!(
            single_atom(id_list(&["c"]))
                .unwrap()
                .map(|v| (v.type_code, v.to_string_value())),
            Some((XmlTypeCode::Id, "c".to_string()))
        );
        assert!(matches!(
            single_atom(id_list(&["a", "b"])),
            Err(XPathError::XPTY0004 { .. })
        ));
        let one = XmlValue::string("s");
        assert_eq!(single_atom(one.clone()).unwrap(), Some(one));
    }

    /// The members of a list whose item type is a union each keep the type
    /// that validated them.
    #[test]
    fn each_member_keeps_its_own_type() {
        let list = XmlValue::new(
            XmlTypeCode::AnySimpleType,
            XmlValueKind::List {
                item_type: XmlTypeCode::AnySimpleType,
                items: vec![XmlValue::string("x"), XmlValue::integer(BigInt::from(1))],
            },
        );
        assert_eq!(
            describe(Atoms::of(list.clone())),
            [
                (XmlTypeCode::String, "x".to_string()),
                (XmlTypeCode::Integer, "1".to_string())
            ]
        );
        assert_eq!(
            atomize(&XmlValue::new(
                XmlTypeCode::AnySimpleType,
                XmlValueKind::List {
                    item_type: XmlTypeCode::AnySimpleType,
                    items: vec![XmlValue::integer(BigInt::from(1))],
                },
            ))
            .unwrap(),
            XmlValue::integer(BigInt::from(1))
        );
    }

    #[test]
    fn unwrap_union_owned_moves_the_member_value_out() {
        let inner = XmlValue::integer(BigInt::from(3));
        let nested = XmlValue::new(
            XmlTypeCode::Integer,
            XmlValueKind::Union(Box::new(XmlValue::new(
                XmlTypeCode::Integer,
                XmlValueKind::Union(Box::new(inner.clone())),
            ))),
        );
        assert_eq!(unwrap_union_owned(nested), inner);
        assert_eq!(unwrap_union_owned(inner.clone()), inner);
    }

    // --- XPath 1.0 conversion tests ---

    use crate::xpath::RoXmlNavigator;

    #[test]
    fn test_first_node_string_value_empty() {
        let value: XPathValue<RoXmlNavigator<'static>> = XPathValue::empty();
        assert_eq!(first_node_string_value(&value), "");
    }

    #[test]
    fn test_first_node_string_value_single_atomic() {
        let value: XPathValue<RoXmlNavigator<'static>> = XPathValue::string("hello");
        assert_eq!(first_node_string_value(&value), "hello");
    }

    #[test]
    fn test_first_node_string_value_single_node() {
        let doc = roxmltree::Document::parse("<root>text content</root>").unwrap();
        let mut nav = RoXmlNavigator::new(&doc);
        nav.move_to_first_child(); // move to <root>
        let value = XPathValue::from_node(nav);
        assert_eq!(first_node_string_value(&value), "text content");
    }

    #[test]
    fn test_first_node_string_value_multi_node_sequence() {
        let doc = roxmltree::Document::parse("<r><a>first</a><b>second</b></r>").unwrap();
        let mut nav_a = RoXmlNavigator::new(&doc);
        nav_a.move_to_first_child(); // <r>
        nav_a.move_to_first_child(); // <a>
        let mut nav_b = nav_a.clone();
        nav_b.move_to_next_sibling(); // <b>
        let value = XPathValue::from_sequence(vec![XmlItem::Node(nav_a), XmlItem::Node(nav_b)]);
        // XPath 1.0: first node's string value
        assert_eq!(first_node_string_value(&value), "first");
    }

    #[test]
    fn test_to_string_10_delegates() {
        let value: XPathValue<RoXmlNavigator<'static>> = XPathValue::string("abc");
        assert_eq!(to_string_10(&value), "abc");
    }

    #[test]
    fn test_to_number_10_empty() {
        let value: XPathValue<RoXmlNavigator<'static>> = XPathValue::empty();
        assert!(to_number_10(&value).is_nan());
    }

    #[test]
    fn test_to_number_10_atomic() {
        let value: XPathValue<RoXmlNavigator<'static>> = XPathValue::double(2.75);
        assert_eq!(to_number_10(&value), 2.75);
    }

    #[test]
    fn test_to_number_10_node_numeric() {
        let doc = roxmltree::Document::parse("<n>42.5</n>").unwrap();
        let mut nav = RoXmlNavigator::new(&doc);
        nav.move_to_first_child(); // <n>
        let value = XPathValue::from_node(nav);
        assert_eq!(to_number_10(&value), 42.5);
    }

    #[test]
    fn test_to_number_10_node_non_numeric() {
        let doc = roxmltree::Document::parse("<n>not a number</n>").unwrap();
        let mut nav = RoXmlNavigator::new(&doc);
        nav.move_to_first_child(); // <n>
        let value = XPathValue::from_node(nav);
        assert!(to_number_10(&value).is_nan());
    }
}
