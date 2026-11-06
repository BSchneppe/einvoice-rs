//! XPath2 item and node iterator abstractions.
//!
//! Mirrors the design in `XML_NODE_ITERATOR_DESIGN.md`.

use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Rc;

use num_bigint::BigInt;

use crate::types::XmlValue;

use super::error::XPathError;
use super::item_set::{ItemSet, XPathComparer};
use super::DomNavigator;

/// XPath item (node or atomic value).
#[derive(Debug, Clone)]
pub enum XmlItem<N: DomNavigator> {
    Node(N),
    Atomic(XmlValue),
}

impl<N: DomNavigator> XmlItem<N> {
    /// Check if this item is a node.
    pub fn is_node(&self) -> bool {
        matches!(self, XmlItem::Node(_))
    }

    /// Check if this item is an atomic value.
    pub fn is_atomic(&self) -> bool {
        matches!(self, XmlItem::Atomic(_))
    }

    /// Try to get a reference to the node.
    pub fn as_node(&self) -> Option<&N> {
        match self {
            XmlItem::Node(n) => Some(n),
            _ => None,
        }
    }

    /// Try to get a reference to the atomic value.
    pub fn as_atomic(&self) -> Option<&XmlValue> {
        match self {
            XmlItem::Atomic(v) => Some(v),
            _ => None,
        }
    }

    /// The string of an atomic item that holds one — `xs:string`, a type
    /// derived from it, or `xs:untypedAtomic` — copied into a new `String`;
    /// `None` for a node or an atomic value of another type (nothing is
    /// converted, as with [`XPathValue::to_owned_string`]).
    ///
    /// [`XPathValue::to_owned_string`]: crate::xpath::functions::XPathValue::to_owned_string
    pub fn to_owned_string(&self) -> Option<String> {
        self.as_atomic()
            .and_then(|v| v.as_string().map(str::to_owned))
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

    /// Try to extract a boolean from an atomic item.
    pub fn as_bool(&self) -> Option<bool> {
        self.as_atomic().and_then(|v| v.as_boolean())
    }

    /// Try to extract a double from an atomic item.
    pub fn as_f64(&self) -> Option<f64> {
        self.as_atomic().and_then(|v| v.as_double())
    }

    /// Try to extract an integer from an atomic item.
    pub fn as_integer(&self) -> Option<BigInt> {
        self.as_atomic().and_then(|v| v.as_integer().cloned())
    }
}

// ============================================================================
// From Trait Implementations for XmlItem
// ============================================================================

impl<N: DomNavigator> From<N> for XmlItem<N> {
    fn from(node: N) -> Self {
        XmlItem::Node(node)
    }
}

impl<N: DomNavigator> From<XmlValue> for XmlItem<N> {
    fn from(value: XmlValue) -> Self {
        XmlItem::Atomic(value)
    }
}

/// Borrowed view of an XPath item.
#[derive(Debug, Clone, Copy)]
pub enum XmlItemRef<'a, N: DomNavigator> {
    Node(&'a N),
    Atomic(&'a XmlValue),
}

impl<'a, N: DomNavigator> XmlItemRef<'a, N> {
    pub fn is_node(&self) -> bool {
        matches!(self, XmlItemRef::Node(_))
    }

    pub fn from_item(item: &'a XmlItem<N>) -> Self {
        match item {
            XmlItem::Node(node) => XmlItemRef::Node(node),
            XmlItem::Atomic(value) => XmlItemRef::Atomic(value),
        }
    }
}

/// Iterator over XPath items (nodes + atomic values).
///
/// This mirrors the .NET XPathNodeIterator shape: a cloneable cursor with
/// `current` and `move_next` semantics.
pub trait XmlNodeIterator: Clone {
    type Navigator: DomNavigator;

    /// Current item (None before first move_next or after end).
    fn current(&self) -> Option<XmlItemRef<'_, Self::Navigator>>;

    /// 0-based position of the current item, or None if not started/finished.
    fn current_position(&self) -> Option<usize>;

    /// Advance to next item; returns false at end of sequence.
    fn move_next(&mut self) -> Result<bool, XPathError>;

    /// 1-based sequential position for axis iteration.
    fn sequential_position(&self) -> Option<usize> {
        self.current_position().map(|pos| pos + 1)
    }

    /// Reset sequential position tracking (used by position filters).
    fn reset_sequential_position(&mut self) {}
}

fn clone_item_ref<N: DomNavigator>(item: XmlItemRef<'_, N>) -> XmlItem<N> {
    match item {
        XmlItemRef::Node(node) => XmlItem::Node(node.clone()),
        XmlItemRef::Atomic(value) => XmlItem::Atomic(value.clone()),
    }
}

/// Vector-backed iterator for simple tests and adapters.
#[derive(Debug, Clone)]
pub struct VecNodeIterator<N: DomNavigator> {
    items: Vec<XmlItem<N>>,
    index: Option<usize>,
}

impl<N: DomNavigator> VecNodeIterator<N> {
    pub fn new(items: Vec<XmlItem<N>>) -> Self {
        Self { items, index: None }
    }
}

impl<N: DomNavigator> XmlNodeIterator for VecNodeIterator<N> {
    type Navigator = N;

    fn current(&self) -> Option<XmlItemRef<'_, Self::Navigator>> {
        self.index.and_then(|i| match self.items.get(i) {
            Some(XmlItem::Node(node)) => Some(XmlItemRef::Node(node)),
            Some(XmlItem::Atomic(value)) => Some(XmlItemRef::Atomic(value)),
            None => None,
        })
    }

    fn current_position(&self) -> Option<usize> {
        self.index
    }

    fn move_next(&mut self) -> Result<bool, XPathError> {
        let next = match self.index {
            None => 0,
            Some(i) => i + 1,
        };

        if next < self.items.len() {
            self.index = Some(next);
            Ok(true)
        } else {
            self.index = None;
            Ok(false)
        }
    }
}

/// Iterator that yields no items.
#[derive(Debug, Clone, Copy, Default)]
pub struct EmptyIterator<N: DomNavigator> {
    _marker: PhantomData<N>,
}

impl<N: DomNavigator> EmptyIterator<N> {
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<N: DomNavigator> XmlNodeIterator for EmptyIterator<N> {
    type Navigator = N;

    fn current(&self) -> Option<XmlItemRef<'_, Self::Navigator>> {
        None
    }

    fn current_position(&self) -> Option<usize> {
        None
    }

    fn move_next(&mut self) -> Result<bool, XPathError> {
        Ok(false)
    }
}

struct BufferedState<I: XmlNodeIterator> {
    source: I,
    buffer: Vec<XmlItem<I::Navigator>>,
    exhausted: bool,
}

/// Buffered iterator that can be replayed without re-reading the source.
#[derive(Clone)]
pub struct BufferedNodeIterator<I: XmlNodeIterator> {
    state: Rc<RefCell<BufferedState<I>>>,
    index: Option<usize>,
    current: Option<XmlItem<I::Navigator>>,
}

impl<I: XmlNodeIterator> BufferedNodeIterator<I> {
    pub fn new(source: I) -> Self {
        Self {
            state: Rc::new(RefCell::new(BufferedState {
                source,
                buffer: Vec::new(),
                exhausted: false,
            })),
            index: None,
            current: None,
        }
    }

    pub fn from_ref(source: &I) -> Self {
        Self::new(source.clone())
    }

    pub fn preload(source: I) -> Result<Self, XPathError> {
        let mut iter = Self::new(source);
        iter.fill()?;
        Ok(iter)
    }

    pub fn fill(&mut self) -> Result<(), XPathError> {
        let mut state = self.state.borrow_mut();
        if state.exhausted {
            return Ok(());
        }
        while state.source.move_next()? {
            let next_item = state.source.current().map(clone_item_ref);
            if let Some(item) = next_item {
                state.buffer.push(item);
            } else {
                state.exhausted = true;
                return Ok(());
            }
        }
        state.exhausted = true;
        Ok(())
    }
}

impl<I: XmlNodeIterator> XmlNodeIterator for BufferedNodeIterator<I> {
    type Navigator = I::Navigator;

    fn current(&self) -> Option<XmlItemRef<'_, Self::Navigator>> {
        self.current.as_ref().map(XmlItemRef::from_item)
    }

    fn current_position(&self) -> Option<usize> {
        self.index
    }

    fn move_next(&mut self) -> Result<bool, XPathError> {
        let next_index = match self.index {
            None => 0,
            Some(i) => i + 1,
        };

        let mut state = self.state.borrow_mut();
        if next_index < state.buffer.len() {
            self.index = Some(next_index);
            self.current = state.buffer.get(next_index).cloned();
            return Ok(true);
        }

        if state.exhausted {
            self.index = None;
            self.current = None;
            return Ok(false);
        }

        if state.source.move_next()? {
            let next_item = state.source.current().map(clone_item_ref);
            if let Some(item) = next_item {
                state.buffer.push(item.clone());
                self.index = Some(next_index);
                self.current = Some(item);
                return Ok(true);
            }
            state.exhausted = true;
            self.index = None;
            self.current = None;
            return Ok(false);
        }

        state.exhausted = true;
        self.index = None;
        self.current = None;
        Ok(false)
    }
}

/// Iterator over an inclusive integer range (XPath `to` expression).
#[derive(Debug, Clone)]
pub struct RangeIterator<N: DomNavigator> {
    min: BigInt,
    max: BigInt,
    current_value: Option<BigInt>,
    current_item: Option<XmlItem<N>>,
    index: Option<usize>,
    done: bool,
    _marker: PhantomData<N>,
}

impl<N: DomNavigator> RangeIterator<N> {
    pub fn new(min: BigInt, max: BigInt) -> Self {
        let done = min > max;
        Self {
            min,
            max,
            current_value: None,
            current_item: None,
            index: None,
            done,
            _marker: PhantomData,
        }
    }

    pub fn from_i64(min: i64, max: i64) -> Self {
        Self::new(BigInt::from(min), BigInt::from(max))
    }
}

impl<N: DomNavigator> XmlNodeIterator for RangeIterator<N> {
    type Navigator = N;

    fn current(&self) -> Option<XmlItemRef<'_, Self::Navigator>> {
        self.current_item.as_ref().map(XmlItemRef::from_item)
    }

    fn current_position(&self) -> Option<usize> {
        self.index
    }

    fn move_next(&mut self) -> Result<bool, XPathError> {
        if self.done {
            self.current_value = None;
            self.current_item = None;
            self.index = None;
            return Ok(false);
        }

        let next_value = match &self.current_value {
            None => self.min.clone(),
            Some(value) => value + 1,
        };

        if next_value > self.max {
            self.done = true;
            self.current_value = None;
            self.current_item = None;
            self.index = None;
            return Ok(false);
        }

        self.current_value = Some(next_value.clone());
        self.current_item = Some(XmlItem::Atomic(XmlValue::integer(next_value)));
        self.index = Some(match self.index {
            None => 0,
            Some(i) => i + 1,
        });
        Ok(true)
    }
}

/// Iterator that enforces document order for node sequences.
#[derive(Debug, Clone)]
pub struct DocumentOrderNodeIterator<N: DomNavigator> {
    items: ItemSet<XmlItem<N>>,
    item_index: usize,
    index: Option<usize>,
    current: Option<XmlItem<N>>,
    last_node: Option<N>,
}

impl<N: DomNavigator> DocumentOrderNodeIterator<N> {
    pub fn new<I: XmlNodeIterator<Navigator = N>>(mut base: I) -> Result<Self, XPathError> {
        let mut is_node: Option<bool> = None;
        let mut items = ItemSet::new();

        while base.move_next()? {
            let item = match base.current() {
                Some(item) => item,
                None => break,
            };
            let item_is_node = matches!(item, XmlItemRef::Node(_));
            if let Some(prev) = is_node {
                if prev != item_is_node {
                    return Err(XPathError::XPTY0018);
                }
            } else {
                is_node = Some(item_is_node);
            }
            items.add(clone_item_ref(item));
        }

        if is_node == Some(true) {
            let comparer = XPathComparer::new();
            items.sort_with(&comparer);
        }

        Ok(Self {
            items,
            item_index: 0,
            index: None,
            current: None,
            last_node: None,
        })
    }
}

impl<N: DomNavigator> XmlNodeIterator for DocumentOrderNodeIterator<N> {
    type Navigator = N;

    fn current(&self) -> Option<XmlItemRef<'_, Self::Navigator>> {
        self.current.as_ref().map(XmlItemRef::from_item)
    }

    fn current_position(&self) -> Option<usize> {
        self.index
    }

    fn move_next(&mut self) -> Result<bool, XPathError> {
        while self.item_index < self.items.len() {
            let item = self.items[self.item_index].clone();
            self.item_index += 1;

            if let XmlItem::Node(nav) = &item {
                if let Some(last) = self.last_node.as_ref() {
                    if last.is_same_position(nav) {
                        continue;
                    }
                }
                self.last_node = Some(nav.clone());
            }

            self.current = Some(item);
            let next_index = match self.index {
                None => 0,
                Some(i) => i + 1,
            };
            self.index = Some(next_index);
            return Ok(true);
        }

        self.index = None;
        self.current = None;
        Ok(false)
    }
}

/// Iterator that returns the item at a specific sequential position.
#[derive(Debug, Clone)]
pub struct PositionFilterNodeIterator<I: XmlNodeIterator> {
    position: usize,
    iter: I,
    index: Option<usize>,
    current: Option<XmlItem<I::Navigator>>,
    done: bool,
}

impl<I: XmlNodeIterator> PositionFilterNodeIterator<I> {
    pub fn new(position: usize, iter: I) -> Self {
        Self {
            position,
            iter,
            index: None,
            current: None,
            done: false,
        }
    }
}

impl<I: XmlNodeIterator> XmlNodeIterator for PositionFilterNodeIterator<I> {
    type Navigator = I::Navigator;

    fn current(&self) -> Option<XmlItemRef<'_, Self::Navigator>> {
        self.current.as_ref().map(XmlItemRef::from_item)
    }

    fn current_position(&self) -> Option<usize> {
        self.index
    }

    fn move_next(&mut self) -> Result<bool, XPathError> {
        if self.done {
            self.index = None;
            self.current = None;
            return Ok(false);
        }

        while self.iter.move_next()? {
            let seq_pos = match self.iter.sequential_position() {
                Some(pos) => pos,
                None => continue,
            };
            if seq_pos == self.position {
                self.iter.reset_sequential_position();
                self.current = self.iter.current().map(clone_item_ref);
                self.index = Some(0);
                self.done = true;
                return Ok(self.current.is_some());
            }
        }

        self.done = true;
        self.index = None;
        self.current = None;
        Ok(false)
    }
}

/// Iterator that returns atomic items and errors on nodes.
#[derive(Debug, Clone)]
pub struct ItemIterator<I: XmlNodeIterator> {
    iter: I,
    started: bool,
    index: Option<usize>,
    current: Option<XmlItem<I::Navigator>>,
}

impl<I: XmlNodeIterator> ItemIterator<I> {
    pub fn new(iter: I) -> Self {
        Self {
            iter,
            started: false,
            index: None,
            current: None,
        }
    }
}

impl<I: XmlNodeIterator> XmlNodeIterator for ItemIterator<I> {
    type Navigator = I::Navigator;

    fn current(&self) -> Option<XmlItemRef<'_, Self::Navigator>> {
        self.current.as_ref().map(XmlItemRef::from_item)
    }

    fn current_position(&self) -> Option<usize> {
        self.index
    }

    fn move_next(&mut self) -> Result<bool, XPathError> {
        if !self.started {
            self.started = true;
            if self.iter.current_position().is_some() {
                let item = match self.iter.current() {
                    Some(item) => item,
                    None => {
                        self.index = None;
                        self.current = None;
                        return Ok(false);
                    }
                };
                if matches!(item, XmlItemRef::Node(_)) {
                    return Err(XPathError::XPTY0018);
                }
                self.current = Some(clone_item_ref(item));
                self.index = Some(0);
                return Ok(true);
            }
        }

        if self.iter.move_next()? {
            let item = match self.iter.current() {
                Some(item) => item,
                None => {
                    self.index = None;
                    self.current = None;
                    return Ok(false);
                }
            };
            if matches!(item, XmlItemRef::Node(_)) {
                return Err(XPathError::XPTY0018);
            }
            self.current = Some(clone_item_ref(item));
            let next_index = match self.index {
                None => 0,
                Some(i) => i + 1,
            };
            self.index = Some(next_index);
            return Ok(true);
        }

        self.index = None;
        self.current = None;
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::navigator::RoXmlNavigator;
    use crate::types::XmlValue;

    fn current_integer<N: DomNavigator>(iter: &impl XmlNodeIterator<Navigator = N>) -> BigInt {
        match iter.current() {
            Some(XmlItemRef::Atomic(value)) => value.as_integer().expect("integer value").clone(),
            _ => panic!("expected integer value"),
        }
    }

    /// `XmlItem::to_owned_string` copies the string of a string-typed or
    /// untyped atomic item, and is `None` for any other item; the deprecated
    /// `as_str` answers the same.
    #[test]
    #[allow(deprecated)]
    fn item_to_owned_string_and_its_deprecated_alias() {
        type Item = XmlItem<RoXmlNavigator<'static>>;
        let doc = roxmltree::Document::parse("<a>text</a>").unwrap();
        let cases: Vec<(XmlItem<RoXmlNavigator<'_>>, Option<&str>)> = vec![
            (Item::Atomic(XmlValue::string("hello")), Some("hello")),
            (Item::Atomic(XmlValue::untyped("raw")), Some("raw")),
            (Item::Atomic(XmlValue::integer(42.into())), None),
            (XmlItem::Node(RoXmlNavigator::new(&doc)), None),
        ];
        for (item, expected) in cases {
            let expected = expected.map(str::to_string);
            assert_eq!(item.to_owned_string(), expected);
            assert_eq!(item.as_str(), expected);
        }
    }

    #[test]
    fn test_empty_iterator() {
        let mut iter: EmptyIterator<RoXmlNavigator<'static>> = EmptyIterator::new();
        assert!(!iter.move_next().unwrap());
        assert!(iter.current().is_none());
        assert!(iter.current_position().is_none());
    }

    #[test]
    fn test_range_iterator_values() {
        let mut iter: RangeIterator<RoXmlNavigator<'static>> = RangeIterator::from_i64(1, 3);
        assert!(iter.move_next().unwrap());
        assert_eq!(current_integer(&iter), BigInt::from(1));
        assert_eq!(iter.current_position(), Some(0));
        assert_eq!(iter.sequential_position(), Some(1));

        assert!(iter.move_next().unwrap());
        assert_eq!(current_integer(&iter), BigInt::from(2));
        assert_eq!(iter.current_position(), Some(1));
        assert_eq!(iter.sequential_position(), Some(2));

        assert!(iter.move_next().unwrap());
        assert_eq!(current_integer(&iter), BigInt::from(3));
        assert_eq!(iter.current_position(), Some(2));
        assert_eq!(iter.sequential_position(), Some(3));

        assert!(!iter.move_next().unwrap());
        assert!(iter.current().is_none());
    }

    #[test]
    fn test_range_iterator_empty() {
        let mut iter: RangeIterator<RoXmlNavigator<'static>> = RangeIterator::from_i64(5, 3);
        assert!(!iter.move_next().unwrap());
        assert!(iter.current().is_none());
    }

    #[test]
    fn test_buffered_iterator_replays() {
        let source: VecNodeIterator<RoXmlNavigator<'static>> = VecNodeIterator::new(vec![
            XmlItem::Atomic(XmlValue::integer(BigInt::from(1))),
            XmlItem::Atomic(XmlValue::integer(BigInt::from(2))),
        ]);

        let mut buffered = BufferedNodeIterator::new(source);
        assert!(buffered.move_next().unwrap());
        assert_eq!(current_integer(&buffered), BigInt::from(1));

        let mut clone = buffered.clone();
        assert_eq!(current_integer(&clone), BigInt::from(1));

        assert!(buffered.move_next().unwrap());
        assert_eq!(current_integer(&buffered), BigInt::from(2));

        assert!(clone.move_next().unwrap());
        assert_eq!(current_integer(&clone), BigInt::from(2));
    }

    #[test]
    fn test_document_order_iterator_dedupes() {
        let doc = roxmltree::Document::parse("<root><a/><a/></root>").expect("parse xml");
        let mut nav = RoXmlNavigator::new(&doc);
        nav.move_to_first_child(); // root
        nav.move_to_first_child(); // a
        let first = nav.clone();
        nav.move_to_next_sibling(); // a
        let second = nav.clone();

        let source: VecNodeIterator<RoXmlNavigator<'_>> = VecNodeIterator::new(vec![
            XmlItem::Node(second),
            XmlItem::Node(first.clone()),
            XmlItem::Node(first),
        ]);

        let mut iter = DocumentOrderNodeIterator::new(source).unwrap();
        let mut names = Vec::new();
        while iter.move_next().unwrap() {
            match iter.current() {
                Some(XmlItemRef::Node(node)) => names.push(node.local_name().to_string()),
                _ => panic!("expected node"),
            }
        }
        assert_eq!(names, vec!["a".to_string(), "a".to_string()]);
    }

    #[test]
    fn test_document_order_iterator_rejects_mixed_sequence() {
        let doc = roxmltree::Document::parse("<root><a/></root>").expect("parse xml");
        let mut nav = RoXmlNavigator::new(&doc);
        nav.move_to_first_child();
        nav.move_to_first_child();
        let source: VecNodeIterator<RoXmlNavigator<'_>> = VecNodeIterator::new(vec![
            XmlItem::Node(nav.clone()),
            XmlItem::Atomic(XmlValue::integer(BigInt::from(1))),
        ]);

        let result = DocumentOrderNodeIterator::new(source);
        assert!(matches!(result, Err(XPathError::XPTY0018)));
    }

    #[test]
    fn test_position_filter_iterator() {
        let source: VecNodeIterator<RoXmlNavigator<'static>> = VecNodeIterator::new(vec![
            XmlItem::Atomic(XmlValue::integer(BigInt::from(1))),
            XmlItem::Atomic(XmlValue::integer(BigInt::from(2))),
        ]);

        let mut iter = PositionFilterNodeIterator::new(2, source);
        assert!(iter.move_next().unwrap());
        assert_eq!(current_integer(&iter), BigInt::from(2));
        assert!(!iter.move_next().unwrap());
    }

    #[test]
    fn test_item_iterator_returns_atomic() {
        let source: VecNodeIterator<RoXmlNavigator<'static>> = VecNodeIterator::new(vec![
            XmlItem::Atomic(XmlValue::integer(BigInt::from(1))),
            XmlItem::Atomic(XmlValue::integer(BigInt::from(2))),
        ]);

        let mut iter = ItemIterator::new(source);
        assert!(iter.move_next().unwrap());
        assert_eq!(current_integer(&iter), BigInt::from(1));
        assert!(iter.move_next().unwrap());
        assert_eq!(current_integer(&iter), BigInt::from(2));
        assert!(!iter.move_next().unwrap());
    }

    #[test]
    fn test_item_iterator_rejects_nodes() {
        let doc = roxmltree::Document::parse("<root><a/></root>").expect("parse xml");
        let mut nav = RoXmlNavigator::new(&doc);
        nav.move_to_first_child();
        nav.move_to_first_child();

        let source: VecNodeIterator<RoXmlNavigator<'_>> =
            VecNodeIterator::new(vec![XmlItem::Node(nav.clone())]);
        let mut iter = ItemIterator::new(source);
        let result = iter.move_next();
        assert!(matches!(result, Err(XPathError::XPTY0018)));
    }
}
