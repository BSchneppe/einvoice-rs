// Microsoft Public License (Ms-PL)
// See the file License.rtf or License.txt for the license details.

// Copyright (c) 2011, Semyon A. Chertkov (semyonc@gmail.com)
// All rights reserved.

//! ItemSet - A dynamic collection for XPath items with sorting support.
//!
//! This module provides a collection type similar to C#'s ItemSet class,
//! designed to store and manage items with efficient sorting capabilities.

use std::cmp::Ordering;
use std::ops::{Index, IndexMut};
use std::slice;

use super::error::XPathError;
use super::iterator::XmlItem;
use super::timsort::{timsort_slice_with_comparer, IComparer};
use super::{DomNavigator, XmlNodeOrder};

/// A dynamic, resizable collection for storing items with sorting support.
///
/// `ItemSet<T>` is a Vec-backed collection that provides:
/// - Dynamic array storage with automatic capacity management
/// - Efficient sorting using TimSort with custom comparers
/// - Iterator support for traversing items
/// - A completion flag for tracking collection state
///
/// This is a Rust port of the C# ItemSet class used in XPath 2.0 operations.
#[derive(Debug, Clone)]
pub struct ItemSet<T> {
    items: Vec<T>,
    completed: bool,
}

impl<T> Default for ItemSet<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> ItemSet<T> {
    /// Creates a new empty `ItemSet`.
    pub fn new() -> Self {
        ItemSet {
            items: Vec::new(),
            completed: false,
        }
    }

    /// Creates a new `ItemSet` with the specified capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        ItemSet {
            items: Vec::with_capacity(capacity),
            completed: false,
        }
    }

    /// Returns the number of items in the collection.
    #[inline]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Returns `true` if the collection is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Returns the current capacity of the collection.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.items.capacity()
    }

    /// Sets the capacity of the collection.
    ///
    /// # Panics
    ///
    /// Panics if `capacity` is less than the current length.
    pub fn set_capacity(&mut self, capacity: usize) {
        if capacity < self.items.len() {
            panic!("Capacity cannot be less than the current length");
        }
        if capacity > self.items.capacity() {
            self.items.reserve(capacity - self.items.capacity());
        }
    }

    /// Ensures the collection has at least the specified capacity.
    fn ensure_capacity(&mut self, min: usize) {
        if self.items.capacity() < min {
            let new_capacity = if self.items.capacity() == 0 {
                4
            } else {
                self.items.capacity() * 2
            };
            let new_capacity = new_capacity.max(min);
            self.items.reserve(new_capacity - self.items.capacity());
        }
    }

    /// Returns `true` if the collection has been marked as completed.
    #[inline]
    pub fn completed(&self) -> bool {
        self.completed
    }

    /// Sets the completion flag.
    #[inline]
    pub fn set_completed(&mut self, value: bool) {
        self.completed = value;
    }

    /// Returns a reference to the item at the specified index.
    #[inline]
    pub fn get(&self, index: usize) -> Option<&T> {
        self.items.get(index)
    }

    /// Returns a mutable reference to the item at the specified index.
    #[inline]
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.items.get_mut(index)
    }

    /// Adds an item to the end of the collection.
    pub fn add(&mut self, item: T) {
        self.ensure_capacity(self.items.len() + 1);
        self.items.push(item);
    }

    /// Removes all items from the collection.
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Returns an iterator over the items.
    #[inline]
    pub fn iter(&self) -> ItemSetIter<'_, T> {
        ItemSetIter {
            inner: self.items.iter(),
        }
    }

    /// Returns a mutable iterator over the items.
    #[inline]
    pub fn iter_mut(&mut self) -> ItemSetIterMut<'_, T> {
        ItemSetIterMut {
            inner: self.items.iter_mut(),
        }
    }

    /// Returns the underlying items as a slice.
    #[inline]
    pub fn as_slice(&self) -> &[T] {
        &self.items
    }

    /// Returns the underlying items as a mutable slice.
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.items
    }
}

impl<T: Clone> ItemSet<T> {
    /// Sorts the items using the provided comparer.
    ///
    /// Uses TimSort algorithm which is stable and efficient for partially sorted data.
    pub fn sort_with<C: IComparer<T>>(&mut self, comparer: &C) {
        timsort_slice_with_comparer(&mut self.items, comparer);
    }
}

impl<T> Index<usize> for ItemSet<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        &self.items[index]
    }
}

impl<T> IndexMut<usize> for ItemSet<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.items[index]
    }
}

impl<T> IntoIterator for ItemSet<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl<'a, T> IntoIterator for &'a ItemSet<T> {
    type Item = &'a T;
    type IntoIter = ItemSetIter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut ItemSet<T> {
    type Item = &'a mut T;
    type IntoIter = ItemSetIterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<T> FromIterator<T> for ItemSet<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let items: Vec<T> = iter.into_iter().collect();
        ItemSet {
            items,
            completed: false,
        }
    }
}

impl<T> Extend<T> for ItemSet<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        self.items.extend(iter);
    }
}

/// An iterator over the items of an `ItemSet`.
#[derive(Debug, Clone)]
pub struct ItemSetIter<'a, T> {
    inner: slice::Iter<'a, T>,
}

impl<'a, T> Iterator for ItemSetIter<'a, T> {
    type Item = &'a T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<T> ExactSizeIterator for ItemSetIter<'_, T> {}

/// A mutable iterator over the items of an `ItemSet`.
#[derive(Debug)]
pub struct ItemSetIterMut<'a, T> {
    inner: slice::IterMut<'a, T>,
}

impl<'a, T> Iterator for ItemSetIterMut<'a, T> {
    type Item = &'a mut T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<T> ExactSizeIterator for ItemSetIterMut<'_, T> {}

// ============================================================================
// XPath-specific Comparers
// ============================================================================

/// A comparer that sorts XPath nodes in document order.
///
/// This comparer implements the XPath document order comparison for nodes.
/// It is used to sort node sequences returned by XPath operations like
/// union, intersect, and except.
///
/// # Panics
///
/// Panics if either item is not a node (i.e., is an atomic value).
#[derive(Debug, Clone, Copy, Default)]
pub struct XPathComparer;

impl XPathComparer {
    /// Creates a new `XPathComparer`.
    pub fn new() -> Self {
        XPathComparer
    }

    /// Fallible comparison of two XmlItems in document order.
    ///
    /// Returns an error if either item is not a node.
    pub fn try_compare<N: DomNavigator>(
        &self,
        x: &XmlItem<N>,
        y: &XmlItem<N>,
    ) -> Result<Ordering, XPathError> {
        match (x, y) {
            (XmlItem::Node(nav1), XmlItem::Node(nav2)) => match nav1.compare_position(nav2) {
                XmlNodeOrder::Before => Ok(Ordering::Less),
                XmlNodeOrder::After => Ok(Ordering::Greater),
                XmlNodeOrder::Same => Ok(Ordering::Equal),
                XmlNodeOrder::Unknown => Ok(Ordering::Equal),
            },
            _ => Err(XPathError::XPTY0004 {
                expected: "node".to_string(),
                found: "atomic value".to_string(),
            }),
        }
    }
}

impl<N: DomNavigator> IComparer<XmlItem<N>> for XPathComparer {
    fn compare(&self, x: &XmlItem<N>, y: &XmlItem<N>) -> Ordering {
        match (x, y) {
            (XmlItem::Node(nav1), XmlItem::Node(nav2)) => {
                match nav1.compare_position(nav2) {
                    XmlNodeOrder::Before => Ordering::Less,
                    XmlNodeOrder::After => Ordering::Greater,
                    XmlNodeOrder::Same => Ordering::Equal,
                    XmlNodeOrder::Unknown => {
                        // Different documents - compare by some stable ordering
                        // In C#, this compares hash codes of document roots.
                        // For now, we treat unknown as equal to maintain stability.
                        // This could be enhanced to use document identifiers if available.
                        Ordering::Equal
                    }
                }
            }
            _ => panic!("Cannot compare non-node items in document order (XPTY0004)"),
        }
    }
}

/// A comparer for checking XPath node equality.
///
/// This comparer checks if two nodes are at the same position in the document.
#[derive(Debug, Clone, Copy, Default)]
pub struct XPathEqualityComparer;

impl XPathEqualityComparer {
    /// Creates a new `XPathEqualityComparer`.
    pub fn new() -> Self {
        XPathEqualityComparer
    }

    /// Checks if two XmlItems are equal (same node position).
    ///
    /// # Panics
    ///
    /// Panics if either item is not a node.
    pub fn equals<N: DomNavigator>(&self, x: &XmlItem<N>, y: &XmlItem<N>) -> bool {
        match (x, y) {
            (XmlItem::Node(nav1), XmlItem::Node(nav2)) => nav1.is_same_position(nav2),
            _ => panic!("Cannot compare non-node items for position equality (XPTY0004)"),
        }
    }

    /// Fallible equality check for two XmlItems.
    ///
    /// Returns an error if either item is not a node.
    pub fn try_equals<N: DomNavigator>(
        &self,
        x: &XmlItem<N>,
        y: &XmlItem<N>,
    ) -> Result<bool, XPathError> {
        match (x, y) {
            (XmlItem::Node(nav1), XmlItem::Node(nav2)) => Ok(nav1.is_same_position(nav2)),
            _ => Err(XPathError::XPTY0004 {
                expected: "node".to_string(),
                found: "atomic value".to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xpath::timsort::{OrdComparer, ReverseComparer};

    #[test]
    fn test_new() {
        let set: ItemSet<i32> = ItemSet::new();
        assert!(set.is_empty());
        assert_eq!(set.len(), 0);
    }

    #[test]
    fn test_with_capacity() {
        let set: ItemSet<i32> = ItemSet::with_capacity(10);
        assert!(set.is_empty());
        assert!(set.capacity() >= 10);
    }

    #[test]
    fn test_add() {
        let mut set = ItemSet::new();
        set.add(1);
        set.add(2);
        set.add(3);
        assert_eq!(set.len(), 3);
        assert_eq!(set[0], 1);
        assert_eq!(set[1], 2);
        assert_eq!(set[2], 3);
    }

    #[test]
    fn test_clear() {
        let mut set = ItemSet::new();
        set.add(1);
        set.add(2);
        set.clear();
        assert!(set.is_empty());
    }

    #[test]
    fn test_get() {
        let mut set = ItemSet::new();
        set.add(42);
        assert_eq!(set.get(0), Some(&42));
        assert_eq!(set.get(1), None);
    }

    #[test]
    fn test_get_mut() {
        let mut set = ItemSet::new();
        set.add(42);
        if let Some(item) = set.get_mut(0) {
            *item = 100;
        }
        assert_eq!(set[0], 100);
    }

    #[test]
    fn test_completed() {
        let mut set: ItemSet<i32> = ItemSet::new();
        assert!(!set.completed());
        set.set_completed(true);
        assert!(set.completed());
    }

    #[test]
    fn test_sort_with_ord_comparer() {
        let mut set = ItemSet::new();
        set.add(3);
        set.add(1);
        set.add(4);
        set.add(1);
        set.add(5);

        let comparer = OrdComparer::<i32>::new();
        set.sort_with(&comparer);

        assert_eq!(set[0], 1);
        assert_eq!(set[1], 1);
        assert_eq!(set[2], 3);
        assert_eq!(set[3], 4);
        assert_eq!(set[4], 5);
    }

    #[test]
    fn test_sort_with_reverse_comparer() {
        let mut set = ItemSet::new();
        set.add(1);
        set.add(2);
        set.add(3);

        let comparer = ReverseComparer::new(OrdComparer::<i32>::new());
        set.sort_with(&comparer);

        assert_eq!(set[0], 3);
        assert_eq!(set[1], 2);
        assert_eq!(set[2], 1);
    }

    #[test]
    fn test_iter() {
        let mut set = ItemSet::new();
        set.add(1);
        set.add(2);
        set.add(3);

        let collected: Vec<_> = set.iter().cloned().collect();
        assert_eq!(collected, vec![1, 2, 3]);
    }

    #[test]
    fn test_into_iter() {
        let mut set = ItemSet::new();
        set.add(1);
        set.add(2);
        set.add(3);

        let collected: Vec<_> = set.into_iter().collect();
        assert_eq!(collected, vec![1, 2, 3]);
    }

    #[test]
    fn test_from_iter() {
        let set: ItemSet<i32> = vec![1, 2, 3].into_iter().collect();
        assert_eq!(set.len(), 3);
        assert_eq!(set[0], 1);
        assert_eq!(set[1], 2);
        assert_eq!(set[2], 3);
    }

    #[test]
    fn test_extend() {
        let mut set = ItemSet::new();
        set.add(1);
        set.extend(vec![2, 3, 4]);
        assert_eq!(set.len(), 4);
    }

    #[test]
    fn test_index() {
        let mut set = ItemSet::new();
        set.add(42);
        assert_eq!(set[0], 42);
    }

    #[test]
    fn test_index_mut() {
        let mut set = ItemSet::new();
        set.add(42);
        set[0] = 100;
        assert_eq!(set[0], 100);
    }

    #[test]
    fn test_default() {
        let set: ItemSet<i32> = ItemSet::default();
        assert!(set.is_empty());
    }
}
