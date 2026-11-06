//! Node operations for XPath evaluation.
//!
//! This module implements XPath 2.0 node comparison and navigation operations:
//! - `is` (node identity)
//! - `<<` (precedes in document order)
//! - `>>` (follows in document order)
//! - Root navigation

use super::error::XPathError;
use super::{DomNavigator, XmlNodeOrder};

/// Check if two nodes are identical (same node).
///
/// This implements the XPath `is` operator.
///
/// # Arguments
///
/// * `a` - First node
/// * `b` - Second node
///
/// # Returns
///
/// `true` if the nodes are the same node, `false` otherwise.
pub fn same_node<N: DomNavigator>(a: &N, b: &N) -> bool {
    match a.compare_position(b) {
        XmlNodeOrder::Same => true,
        XmlNodeOrder::Unknown => b.compare_position(a) == XmlNodeOrder::Same,
        _ => false,
    }
}

/// Check if node `a` precedes node `b` in document order.
///
/// This implements the XPath `<<` operator.
///
/// # Arguments
///
/// * `a` - First node
/// * `b` - Second node
///
/// # Returns
///
/// `true` if `a` precedes `b` in document order.
pub fn preceding_node<N: DomNavigator>(a: &N, b: &N) -> bool {
    a.compare_position(b) == XmlNodeOrder::Before
}

/// Check if node `a` follows node `b` in document order.
///
/// This implements the XPath `>>` operator.
///
/// # Arguments
///
/// * `a` - First node
/// * `b` - Second node
///
/// # Returns
///
/// `true` if `a` follows `b` in document order.
pub fn following_node<N: DomNavigator>(a: &N, b: &N) -> bool {
    a.compare_position(b) == XmlNodeOrder::After
}

/// Get the root node from a given node.
///
/// Navigates to the document root.
///
/// # Arguments
///
/// * `node` - The starting node
///
/// # Returns
///
/// A clone of the navigator positioned at the root.
pub fn get_root<N: DomNavigator>(node: &N) -> N {
    let mut nav = node.clone();
    nav.move_to_root();
    nav
}

/// Get the context node from an optional context.
///
/// # Arguments
///
/// * `context` - Optional context node
///
/// # Returns
///
/// * `Ok(N)` - The context node
/// * `Err(XPathError)` - XPDY0002 if context is undefined
pub fn context_node<N: DomNavigator>(context: Option<&N>) -> Result<N, XPathError> {
    context.cloned().ok_or_else(XPathError::context_undefined)
}

/// Compare two nodes by document order.
///
/// # Returns
///
/// * `std::cmp::Ordering::Less` if `a` precedes `b`
/// * `std::cmp::Ordering::Equal` if they are the same node
/// * `std::cmp::Ordering::Greater` if `a` follows `b`
pub fn compare_document_order<N: DomNavigator>(a: &N, b: &N) -> std::cmp::Ordering {
    match a.compare_position(b) {
        XmlNodeOrder::Before => std::cmp::Ordering::Less,
        XmlNodeOrder::Same => std::cmp::Ordering::Equal,
        XmlNodeOrder::After => std::cmp::Ordering::Greater,
        XmlNodeOrder::Unknown => {
            // Try reverse comparison
            match b.compare_position(a) {
                XmlNodeOrder::Before => std::cmp::Ordering::Greater,
                XmlNodeOrder::After => std::cmp::Ordering::Less,
                _ => std::cmp::Ordering::Equal,
            }
        }
    }
}

/// Check if a node is the document root.
pub fn is_root<N: DomNavigator>(node: &N) -> bool {
    let root = get_root(node);
    same_node(node, &root)
}

/// Check if node `ancestor` is an ancestor of node `descendant`.
pub fn is_ancestor<N: DomNavigator>(ancestor: &N, descendant: &N) -> bool {
    let mut current = descendant.clone();
    while current.move_to_parent() {
        if same_node(&current, ancestor) {
            return true;
        }
    }
    false
}

/// Check if node `descendant` is a descendant of node `ancestor`.
pub fn is_descendant<N: DomNavigator>(descendant: &N, ancestor: &N) -> bool {
    is_ancestor(ancestor, descendant)
}

/// Check if two nodes are siblings (same parent).
pub fn are_siblings<N: DomNavigator>(a: &N, b: &N) -> bool {
    let mut a_parent = a.clone();
    let mut b_parent = b.clone();

    if !a_parent.move_to_parent() || !b_parent.move_to_parent() {
        return false;
    }

    same_node(&a_parent, &b_parent)
}

#[cfg(test)]
mod tests {
    // Tests would require a DomNavigator implementation
    // The roxmltree adapter provides this for integration testing
}
