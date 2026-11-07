//! Sequence operations for XPath evaluation.
//!
//! This module implements XPath 2.0 sequence operations:
//! - Union (|) - node-only, returns nodes in document order
//! - Intersect - node-only, returns nodes in document order
//! - Except - node-only, returns nodes in document order
//!
//! Note: Union, intersect, and except are NODE-ONLY operations in XPath 2.0.
//! Applying them to atomic values results in XPTY0004.

use std::collections::HashSet;

use crate::ids::NameId;
use crate::types::value::{XmlAtomicValue, XmlValue, XmlValueKind};
use crate::types::XmlTypeCode;

use super::error::XPathError;
use super::item_set::{ItemSet, XPathComparer, XPathEqualityComparer};
use super::iterator::XmlItem;
use super::DomNavigator;

// ============================================================================
// Node-only sequence operations (union, intersect, except)
// ============================================================================

/// Compute the union of two node sequences.
///
/// Returns all nodes from both sequences, deduplicated by node identity,
/// sorted in document order.
///
/// # XPath Semantics
///
/// The union operator `|` is defined only for node sequences. The result
/// contains all nodes that are in either operand, in document order, with
/// duplicates removed.
///
/// # Errors
///
/// Returns XPTY0004 if either sequence contains non-node items.
pub fn union_nodes<N: DomNavigator + Clone>(
    left: Vec<XmlItem<N>>,
    right: Vec<XmlItem<N>>,
) -> Result<Vec<XmlItem<N>>, XPathError> {
    let mut result: ItemSet<XmlItem<N>> = ItemSet::with_capacity(left.len() + right.len());
    let eq_comparer = XPathEqualityComparer::new();

    // Add all items from left, checking they are nodes
    for item in left {
        if !matches!(item, XmlItem::Node(_)) {
            return Err(XPathError::type_mismatch(
                "node()*".to_string(),
                "atomic value".to_string(),
            ));
        }
        // Check for duplicates using node identity
        let is_duplicate = result
            .iter()
            .any(|existing| eq_comparer.equals(existing, &item));
        if !is_duplicate {
            result.add(item);
        }
    }

    // Add items from right that aren't already in result
    for item in right {
        if !matches!(item, XmlItem::Node(_)) {
            return Err(XPathError::type_mismatch(
                "node()*".to_string(),
                "atomic value".to_string(),
            ));
        }
        let is_duplicate = result
            .iter()
            .any(|existing| eq_comparer.equals(existing, &item));
        if !is_duplicate {
            result.add(item);
        }
    }

    // Sort in document order
    let comparer = XPathComparer::new();
    result.sort_with(&comparer);

    Ok(result.into_iter().collect())
}

/// Compute the intersection of two node sequences.
///
/// Returns nodes that appear in both sequences, sorted in document order.
///
/// # XPath Semantics
///
/// The intersect operator returns all nodes that are in both operands,
/// in document order.
///
/// # Errors
///
/// Returns XPTY0004 if either sequence contains non-node items.
pub fn intersect_nodes<N: DomNavigator + Clone>(
    left: Vec<XmlItem<N>>,
    right: Vec<XmlItem<N>>,
) -> Result<Vec<XmlItem<N>>, XPathError> {
    // Validate that all items are nodes
    for item in &left {
        if !matches!(item, XmlItem::Node(_)) {
            return Err(XPathError::type_mismatch(
                "node()*".to_string(),
                "atomic value".to_string(),
            ));
        }
    }
    for item in &right {
        if !matches!(item, XmlItem::Node(_)) {
            return Err(XPathError::type_mismatch(
                "node()*".to_string(),
                "atomic value".to_string(),
            ));
        }
    }

    let eq_comparer = XPathEqualityComparer::new();
    let mut result: ItemSet<XmlItem<N>> = ItemSet::new();

    // Find nodes in left that also exist in right
    for left_item in left {
        let in_right = right.iter().any(|r| eq_comparer.equals(&left_item, r));
        if in_right {
            // Check we haven't already added this node
            let already_added = result
                .iter()
                .any(|existing| eq_comparer.equals(existing, &left_item));
            if !already_added {
                result.add(left_item);
            }
        }
    }

    // Sort in document order
    let comparer = XPathComparer::new();
    result.sort_with(&comparer);

    Ok(result.into_iter().collect())
}

/// Compute the difference of two node sequences (left except right).
///
/// Returns nodes that appear in left but not in right, sorted in document order.
///
/// # XPath Semantics
///
/// The except operator returns all nodes that are in the first operand
/// but not in the second operand, in document order.
///
/// # Errors
///
/// Returns XPTY0004 if either sequence contains non-node items.
pub fn except_nodes<N: DomNavigator + Clone>(
    left: Vec<XmlItem<N>>,
    right: Vec<XmlItem<N>>,
) -> Result<Vec<XmlItem<N>>, XPathError> {
    // Validate that all items are nodes
    for item in &left {
        if !matches!(item, XmlItem::Node(_)) {
            return Err(XPathError::type_mismatch(
                "node()*".to_string(),
                "atomic value".to_string(),
            ));
        }
    }
    for item in &right {
        if !matches!(item, XmlItem::Node(_)) {
            return Err(XPathError::type_mismatch(
                "node()*".to_string(),
                "atomic value".to_string(),
            ));
        }
    }

    let eq_comparer = XPathEqualityComparer::new();
    let mut result: ItemSet<XmlItem<N>> = ItemSet::new();

    // Find nodes in left that do NOT exist in right
    for left_item in left {
        let in_right = right.iter().any(|r| eq_comparer.equals(&left_item, r));
        if !in_right {
            // Check we haven't already added this node
            let already_added = result
                .iter()
                .any(|existing| eq_comparer.equals(existing, &left_item));
            if !already_added {
                result.add(left_item);
            }
        }
    }

    // Sort in document order
    let comparer = XPathComparer::new();
    result.sort_with(&comparer);

    Ok(result.into_iter().collect())
}

// ============================================================================
// Atomic value sequence operations (for fn:distinct-values, etc.)
// ============================================================================
//
// These helpers compare two atomic values by their string values, except that
// an `xs:QName` or `xs:NOTATION` compares by its expanded name (namespace URI
// and local name, ignoring the prefix) and never equals a value of another
// type. That is not the XPath `eq` (`1` and `1.0e0` share a string value;
// `xs:decimal("1.0")` and `"1"` do too): the evaluator's `fn:distinct-values`,
// `fn:index-of` and `fn:deep-equal` do not use them.

/// The key the atomic-value helpers below compare values by.
///
/// An `xs:QName` or `xs:NOTATION` is its expanded name — namespace URI and
/// local name, never the prefix (F&O §11.2.1 `op:QName-equal`: "The prefix
/// parts of $arg1 and $arg2, if any, are ignored"; `op:NOTATION-equal` is the
/// same) — kept apart from every other value, so a QName never equals the
/// string that spells it. Every other value is its string value, as before.
#[derive(Debug, PartialEq, Eq, Hash)]
enum AtomKey {
    Name {
        notation: bool,
        namespace_uri: Option<NameId>,
        local_name: NameId,
    },
    Text(String),
}

fn atom_key(value: &XmlValue) -> AtomKey {
    let mut inner = value;
    while let XmlValueKind::Union(member) = &inner.value {
        inner = member;
    }
    match &inner.value {
        XmlValueKind::Atomic(XmlAtomicValue::QName(name) | XmlAtomicValue::Notation(name)) => {
            AtomKey::Name {
                notation: inner.type_code == XmlTypeCode::Notation,
                namespace_uri: name.name().namespace_uri,
                local_name: name.name().local_name,
            }
        }
        _ => AtomKey::Text(value.to_string_value()),
    }
}

/// Compute the union of two atomic value sequences, without duplicates (by
/// string value; an `xs:QName` or `xs:NOTATION` by expanded name).
///
/// Note: This is NOT the XPath `|` operator (which is node-only).
/// This is for internal use with atomic value sequences.
pub fn union_atomic_values(left: Vec<XmlValue>, right: Vec<XmlValue>) -> Vec<XmlValue> {
    let mut seen = HashSet::new();
    let mut result = Vec::with_capacity(left.len() + right.len());

    for item in left.into_iter().chain(right) {
        let key = atom_key(&item);
        if !seen.contains(&key) {
            seen.insert(key);
            result.push(item);
        }
    }

    result
}

/// Compute the intersection of two atomic value sequences.
///
/// Note: This is NOT the XPath `intersect` operator (which is node-only).
pub fn intersect_atomic_values(left: Vec<XmlValue>, right: Vec<XmlValue>) -> Vec<XmlValue> {
    let right_set: HashSet<AtomKey> = right.iter().map(atom_key).collect();

    let mut seen = HashSet::new();
    let mut result = Vec::new();

    for item in left {
        let key = atom_key(&item);
        if right_set.contains(&key) && !seen.contains(&key) {
            seen.insert(key);
            result.push(item);
        }
    }

    result
}

/// Compute the difference of two atomic value sequences.
///
/// Note: This is NOT the XPath `except` operator (which is node-only).
pub fn except_atomic_values(left: Vec<XmlValue>, right: Vec<XmlValue>) -> Vec<XmlValue> {
    let right_set: HashSet<AtomKey> = right.iter().map(atom_key).collect();

    let mut seen = HashSet::new();
    let mut result = Vec::new();

    for item in left {
        let key = atom_key(&item);
        if !right_set.contains(&key) && !seen.contains(&key) {
            seen.insert(key);
            result.push(item);
        }
    }

    result
}

// ============================================================================
// General sequence utilities
// ============================================================================

/// Check if a sequence is empty.
pub fn is_empty(values: &[XmlValue]) -> bool {
    values.is_empty()
}

/// Check if a sequence contains exactly one item.
pub fn is_singleton(values: &[XmlValue]) -> bool {
    values.len() == 1
}

/// Get the first item from a sequence, if any.
pub fn head(values: &[XmlValue]) -> Option<&XmlValue> {
    values.first()
}

/// Get all items except the first from a sequence.
pub fn tail(values: &[XmlValue]) -> &[XmlValue] {
    if values.is_empty() {
        &[]
    } else {
        &values[1..]
    }
}

/// Reverse a sequence.
pub fn reverse(values: Vec<XmlValue>) -> Vec<XmlValue> {
    let mut result = values;
    result.reverse();
    result
}

/// Get the count of items in a sequence.
pub fn count(values: &[XmlValue]) -> usize {
    values.len()
}

/// Check if a sequence contains a specific value (by string value; an
/// `xs:QName` or `xs:NOTATION` by expanded name).
pub fn contains_value(values: &[XmlValue], target: &XmlValue) -> bool {
    let target = atom_key(target);
    values.iter().any(|v| atom_key(v) == target)
}

/// Insert an item at a specific position.
pub fn insert_before(values: Vec<XmlValue>, position: usize, item: XmlValue) -> Vec<XmlValue> {
    let mut result = values;
    let pos = position.min(result.len());
    result.insert(pos, item);
    result
}

/// Remove an item at a specific position.
pub fn remove_at(values: Vec<XmlValue>, position: usize) -> Vec<XmlValue> {
    let mut result = values;
    if position < result.len() {
        result.remove(position);
    }
    result
}

/// Subsequence (slice) of a sequence.
///
/// XPath uses 1-based indexing.
pub fn subsequence(values: &[XmlValue], start: usize, length: Option<usize>) -> Vec<XmlValue> {
    if start == 0 || start > values.len() {
        return Vec::new();
    }

    let start_idx = start - 1; // Convert to 0-based
    let end_idx = match length {
        Some(len) => (start_idx + len).min(values.len()),
        None => values.len(),
    };

    values[start_idx..end_idx].to_vec()
}

/// Get distinct values from a sequence (removes duplicates by string value; an
/// `xs:QName` or `xs:NOTATION` by expanded name).
///
/// A simplified `fn:distinct-values`: the comparison is the string-value rule
/// above, not `eq`.
pub fn distinct_values(values: Vec<XmlValue>) -> Vec<XmlValue> {
    let mut seen = HashSet::new();
    let mut result = Vec::with_capacity(values.len());

    for item in values {
        let key = atom_key(&item);
        if !seen.contains(&key) {
            seen.insert(key);
            result.push(item);
        }
    }

    result
}

/// Find the index of a value in a sequence (1-based, XPath style), by string
/// value; an `xs:QName` or `xs:NOTATION` by expanded name.
///
/// Returns 0 if not found.
pub fn index_of(values: &[XmlValue], target: &XmlValue) -> usize {
    let target = atom_key(target);
    for (i, v) in values.iter().enumerate() {
        if atom_key(v) == target {
            return i + 1; // 1-based
        }
    }
    0
}

/// Deep equality check between two atomic value sequences (item by item, by
/// string value; an `xs:QName` or `xs:NOTATION` by expanded name).
pub fn deep_equal(left: &[XmlValue], right: &[XmlValue]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    for (l, r) in left.iter().zip(right.iter()) {
        if atom_key(l) != atom_key(r) {
            return false;
        }
    }

    true
}

// ============================================================================
// Backwards compatibility aliases (deprecated)
// ============================================================================

/// Deprecated: Use `union_atomic_values` for atomic sequences or `union_nodes` for nodes.
#[deprecated(note = "Use union_atomic_values for atomic sequences or union_nodes for nodes")]
pub fn union_values(left: Vec<XmlValue>, right: Vec<XmlValue>) -> Vec<XmlValue> {
    union_atomic_values(left, right)
}

/// Deprecated: Use `intersect_atomic_values` for atomic sequences or `intersect_nodes` for nodes.
#[deprecated(
    note = "Use intersect_atomic_values for atomic sequences or intersect_nodes for nodes"
)]
pub fn intersect_values(left: Vec<XmlValue>, right: Vec<XmlValue>) -> Vec<XmlValue> {
    intersect_atomic_values(left, right)
}

/// Deprecated: Use `except_atomic_values` for atomic sequences or `except_nodes` for nodes.
#[deprecated(note = "Use except_atomic_values for atomic sequences or except_nodes for nodes")]
pub fn except_values(left: Vec<XmlValue>, right: Vec<XmlValue>) -> Vec<XmlValue> {
    except_atomic_values(left, right)
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_bigint::BigInt;

    fn int_values(nums: &[i32]) -> Vec<XmlValue> {
        nums.iter()
            .map(|&n| XmlValue::integer(BigInt::from(n)))
            .collect()
    }

    fn str_values(strs: &[&str]) -> Vec<XmlValue> {
        strs.iter().map(|&s| XmlValue::string(s)).collect()
    }

    #[test]
    fn test_union_atomic() {
        let left = int_values(&[1, 2, 3]);
        let right = int_values(&[2, 3, 4]);
        let result = union_atomic_values(left, right);
        assert_eq!(count(&result), 4);
    }

    #[test]
    fn test_intersect_atomic() {
        let left = int_values(&[1, 2, 3]);
        let right = int_values(&[2, 3, 4]);
        let result = intersect_atomic_values(left, right);
        assert_eq!(count(&result), 2);
    }

    #[test]
    fn test_except_atomic() {
        let left = int_values(&[1, 2, 3]);
        let right = int_values(&[2, 3, 4]);
        let result = except_atomic_values(left, right);
        assert_eq!(count(&result), 1);
    }

    #[test]
    fn test_is_empty() {
        assert!(is_empty(&[]));
        assert!(!is_empty(&int_values(&[1])));
    }

    #[test]
    fn test_is_singleton() {
        assert!(!is_singleton(&[]));
        assert!(is_singleton(&int_values(&[1])));
        assert!(!is_singleton(&int_values(&[1, 2])));
    }

    #[test]
    fn test_head_tail() {
        let values = int_values(&[1, 2, 3]);
        assert_eq!(head(&values).unwrap().to_string_value(), "1");
        assert_eq!(tail(&values).len(), 2);

        let empty: Vec<XmlValue> = Vec::new();
        assert!(head(&empty).is_none());
        assert!(tail(&empty).is_empty());
    }

    #[test]
    fn test_reverse() {
        let values = int_values(&[1, 2, 3]);
        let reversed = reverse(values);
        assert_eq!(reversed[0].to_string_value(), "3");
        assert_eq!(reversed[2].to_string_value(), "1");
    }

    #[test]
    fn test_subsequence() {
        let values = int_values(&[1, 2, 3, 4, 5]);

        // Start at 2, length 3 -> [2, 3, 4]
        let sub = subsequence(&values, 2, Some(3));
        assert_eq!(count(&sub), 3);

        // Start at 3, no length -> [3, 4, 5]
        let sub = subsequence(&values, 3, None);
        assert_eq!(count(&sub), 3);

        // Out of bounds
        let sub = subsequence(&values, 10, None);
        assert!(is_empty(&sub));
    }

    #[test]
    fn test_distinct_values() {
        let values = str_values(&["a", "b", "a", "c", "b"]);
        let distinct = distinct_values(values);
        assert_eq!(count(&distinct), 3);
    }

    #[test]
    fn test_index_of() {
        let values = str_values(&["a", "b", "c"]);
        assert_eq!(index_of(&values, &XmlValue::string("b")), 2);
        assert_eq!(index_of(&values, &XmlValue::string("z")), 0);
    }

    #[test]
    fn test_deep_equal() {
        let a = int_values(&[1, 2, 3]);
        let b = int_values(&[1, 2, 3]);
        let c = int_values(&[1, 2, 4]);
        let d = int_values(&[1, 2]);

        assert!(deep_equal(&a, &b));
        assert!(!deep_equal(&a, &c));
        assert!(!deep_equal(&a, &d));
    }

    #[test]
    fn test_insert_before() {
        let values = int_values(&[1, 3]);
        let result = insert_before(values, 1, XmlValue::integer(BigInt::from(2)));
        assert_eq!(count(&result), 3);
        assert_eq!(result[1].to_string_value(), "2");
    }

    #[test]
    fn test_remove_at() {
        let values = int_values(&[1, 2, 3]);
        let result = remove_at(values, 1);
        assert_eq!(count(&result), 2);
    }

    // Tests for node-only operations would require a DomNavigator implementation
    // which is available in integration tests with roxmltree

    /// The atomic-value helpers compare `xs:QName` and `xs:NOTATION` values by
    /// expanded name (F&O §11.2.1 `op:QName-equal`: "The prefix parts of $arg1
    /// and $arg2, if any, are ignored"), never by the lexical form — and never
    /// as equal to a string that spells the same lexical form.
    #[test]
    fn qnames_and_notations_compare_by_expanded_name() {
        use crate::namespace::qname::QualifiedName;
        use crate::namespace::table::NameTable;
        use crate::types::value::{QNameValue, XmlAtomicValue, XmlValueKind};
        use crate::types::XmlTypeCode;

        let names = NameTable::new();
        let ns = names.add("urn:x");
        let local = names.add("l");
        let name = |code: XmlTypeCode, prefix: &str| {
            let value = QNameValue::new(
                QualifiedName::new(Some(ns), local, Some(names.add(prefix))),
                &names,
            );
            let atom = if code == XmlTypeCode::Notation {
                XmlAtomicValue::Notation(value)
            } else {
                XmlAtomicValue::QName(value)
            };
            XmlValue::new(code, XmlValueKind::Atomic(atom))
        };
        let (p, q) = (name(XmlTypeCode::QName, "p"), name(XmlTypeCode::QName, "q"));
        let (np, nq) = (
            name(XmlTypeCode::Notation, "p"),
            name(XmlTypeCode::Notation, "q"),
        );
        let text = XmlValue::string("p:l");

        assert_eq!(distinct_values(vec![p.clone(), q.clone()]).len(), 1);
        assert_eq!(distinct_values(vec![np.clone(), nq.clone()]).len(), 1);
        assert_eq!(distinct_values(vec![p.clone(), np.clone()]).len(), 2);
        assert_eq!(distinct_values(vec![p.clone(), text.clone()]).len(), 2);
        assert_eq!(index_of(&[text.clone(), q.clone()], &p), 2);
        assert_eq!(index_of(std::slice::from_ref(&np), &p), 0);
        assert!(contains_value(std::slice::from_ref(&nq), &np));
        assert!(!contains_value(std::slice::from_ref(&text), &p));
        assert!(deep_equal(
            &[p.clone(), np.clone()],
            &[q.clone(), nq.clone()]
        ));
        assert!(!deep_equal(
            std::slice::from_ref(&p),
            std::slice::from_ref(&text)
        ));
        assert_eq!(
            union_atomic_values(vec![p.clone()], vec![q.clone()]).len(),
            1
        );
        assert_eq!(
            intersect_atomic_values(vec![p.clone()], vec![q.clone()]).len(),
            1
        );
        assert!(except_atomic_values(vec![p.clone()], vec![q.clone()]).is_empty());
        assert_eq!(except_atomic_values(vec![p], vec![text]).len(), 1);
    }
}
