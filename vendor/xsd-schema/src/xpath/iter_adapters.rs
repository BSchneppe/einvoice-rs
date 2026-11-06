//! Iterator adapters for XPath evaluation.
//!
//! This module provides iterator adapters for common XPath operations.
//! These are building blocks for the full XPath iterator infrastructure.

use crate::types::value::XmlValue;

/// An iterator that yields codepoints from a string.
///
/// Implements fn:string-to-codepoints.
pub struct CodepointIterator {
    chars: std::vec::IntoIter<char>,
}

impl CodepointIterator {
    /// Create a new codepoint iterator from a string.
    pub fn new(s: &str) -> Self {
        Self {
            chars: s.chars().collect::<Vec<_>>().into_iter(),
        }
    }
}

impl Iterator for CodepointIterator {
    type Item = XmlValue;

    fn next(&mut self) -> Option<Self::Item> {
        self.chars
            .next()
            .map(|c| XmlValue::integer(num_bigint::BigInt::from(c as u32)))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.chars.size_hint()
    }
}

impl ExactSizeIterator for CodepointIterator {}

/// An iterator that filters items based on a predicate.
pub struct FilterIterator<I, F> {
    inner: I,
    predicate: F,
}

impl<I, F> FilterIterator<I, F>
where
    I: Iterator,
    F: FnMut(&I::Item) -> bool,
{
    /// Create a new filter iterator.
    pub fn new(inner: I, predicate: F) -> Self {
        Self { inner, predicate }
    }
}

impl<I, F> Iterator for FilterIterator<I, F>
where
    I: Iterator,
    F: FnMut(&I::Item) -> bool,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.by_ref().find(&mut self.predicate)
    }
}

/// An iterator that maps items through a transformation function.
pub struct MapIterator<I, F> {
    inner: I,
    transform: F,
}

impl<I, F, T> MapIterator<I, F>
where
    I: Iterator,
    F: FnMut(I::Item) -> T,
{
    /// Create a new map iterator.
    pub fn new(inner: I, transform: F) -> Self {
        Self { inner, transform }
    }
}

impl<I, F, T> Iterator for MapIterator<I, F>
where
    I: Iterator,
    F: FnMut(I::Item) -> T,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(&mut self.transform)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

/// An iterator that yields items from a slice with position tracking.
pub struct PositionalIterator<'a, T> {
    items: std::slice::Iter<'a, T>,
    position: usize,
    count: usize,
}

impl<'a, T> PositionalIterator<'a, T> {
    /// Create a new positional iterator.
    pub fn new(items: &'a [T]) -> Self {
        Self {
            items: items.iter(),
            position: 0,
            count: items.len(),
        }
    }

    /// Get the current position (1-based, XPath style).
    pub fn position(&self) -> usize {
        self.position
    }

    /// Get the total count of items.
    pub fn count(&self) -> usize {
        self.count
    }

    /// Check if this is the last item.
    pub fn is_last(&self) -> bool {
        self.position == self.count
    }
}

impl<'a, T> Iterator for PositionalIterator<'a, T> {
    type Item = (usize, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        self.items.next().map(|item| {
            self.position += 1;
            (self.position, item)
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.items.size_hint()
    }
}

impl<'a, T> ExactSizeIterator for PositionalIterator<'a, T> {}

/// An iterator that reverses another iterator.
pub struct ReverseIterator<I: DoubleEndedIterator> {
    inner: std::iter::Rev<I>,
}

impl<I: DoubleEndedIterator> ReverseIterator<I> {
    /// Create a new reverse iterator.
    pub fn new(inner: I) -> Self {
        Self { inner: inner.rev() }
    }
}

impl<I: DoubleEndedIterator> Iterator for ReverseIterator<I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

/// An iterator that takes at most N items.
pub struct TakeIterator<I> {
    inner: I,
    remaining: usize,
}

impl<I: Iterator> TakeIterator<I> {
    /// Create a new take iterator.
    pub fn new(inner: I, count: usize) -> Self {
        Self {
            inner,
            remaining: count,
        }
    }
}

impl<I: Iterator> Iterator for TakeIterator<I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            None
        } else {
            self.remaining -= 1;
            self.inner.next()
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (lower, upper) = self.inner.size_hint();
        (
            lower.min(self.remaining),
            upper.map(|u| u.min(self.remaining)),
        )
    }
}

/// An iterator that skips the first N items.
pub struct SkipIterator<I> {
    inner: I,
    to_skip: usize,
}

impl<I: Iterator> SkipIterator<I> {
    /// Create a new skip iterator.
    pub fn new(inner: I, count: usize) -> Self {
        Self {
            inner,
            to_skip: count,
        }
    }
}

impl<I: Iterator> Iterator for SkipIterator<I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        while self.to_skip > 0 {
            self.to_skip -= 1;
            self.inner.next()?;
        }
        self.inner.next()
    }
}

/// An iterator that chains two iterators together.
pub struct ChainIterator<I1, I2>
where
    I1: Iterator,
    I2: Iterator<Item = I1::Item>,
{
    first: Option<I1>,
    second: I2,
}

impl<I1, I2> ChainIterator<I1, I2>
where
    I1: Iterator,
    I2: Iterator<Item = I1::Item>,
{
    /// Create a new chain iterator.
    pub fn new(first: I1, second: I2) -> Self {
        Self {
            first: Some(first),
            second,
        }
    }
}

impl<I1, I2> Iterator for ChainIterator<I1, I2>
where
    I1: Iterator,
    I2: Iterator<Item = I1::Item>,
{
    type Item = I1::Item;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(ref mut first) = self.first {
            if let Some(item) = first.next() {
                return Some(item);
            }
            self.first = None;
        }
        self.second.next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_codepoint_iterator() {
        let iter = CodepointIterator::new("ABC");
        let codepoints: Vec<_> = iter.collect();
        assert_eq!(codepoints.len(), 3);
        assert_eq!(codepoints[0].to_string_value(), "65");
        assert_eq!(codepoints[1].to_string_value(), "66");
        assert_eq!(codepoints[2].to_string_value(), "67");
    }

    #[test]
    fn test_positional_iterator() {
        let items = vec![1, 2, 3];
        let mut iter = PositionalIterator::new(&items);

        let (pos, val) = iter.next().unwrap();
        assert_eq!(pos, 1);
        assert_eq!(*val, 1);

        let (pos, val) = iter.next().unwrap();
        assert_eq!(pos, 2);
        assert_eq!(*val, 2);
    }

    #[test]
    fn test_take_iterator() {
        let items = vec![1, 2, 3, 4, 5];
        let iter = TakeIterator::new(items.into_iter(), 3);
        let result: Vec<_> = iter.collect();
        assert_eq!(result, vec![1, 2, 3]);
    }

    #[test]
    fn test_skip_iterator() {
        let items = vec![1, 2, 3, 4, 5];
        let iter = SkipIterator::new(items.into_iter(), 2);
        let result: Vec<_> = iter.collect();
        assert_eq!(result, vec![3, 4, 5]);
    }

    #[test]
    fn test_filter_iterator() {
        let items = vec![1, 2, 3, 4, 5];
        let iter = FilterIterator::new(items.into_iter(), |&x| x % 2 == 0);
        let result: Vec<_> = iter.collect();
        assert_eq!(result, vec![2, 4]);
    }

    #[test]
    fn test_map_iterator() {
        let items = vec![1, 2, 3];
        let iter = MapIterator::new(items.into_iter(), |x| x * 2);
        let result: Vec<_> = iter.collect();
        assert_eq!(result, vec![2, 4, 6]);
    }

    #[test]
    fn test_chain_iterator() {
        let first = vec![1, 2];
        let second = vec![3, 4];
        let iter = ChainIterator::new(first.into_iter(), second.into_iter());
        let result: Vec<_> = iter.collect();
        assert_eq!(result, vec![1, 2, 3, 4]);
    }
}
