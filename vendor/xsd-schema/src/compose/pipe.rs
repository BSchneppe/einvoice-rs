//! Fallible iterator pipelines over host rows.
//!
//! A [`Pipe`] is an adapter over ordinary Rust iteration, not a query plan: it
//! yields `Result<T, ComposeError>`, so a stage that evaluates an expression
//! can fail and the failure travels to the consumer instead of turning into a
//! missing row. `Iterator::filter` wants a `bool` and `Iterator::flatten`
//! drops an `Err` on the floor — these adapters are the same shapes with the
//! `Result` kept.
//!
//! # The FLWOR vocabulary
//!
//! | Query clause | Adapter |
//! |---|---|
//! | `for $x in …` | [`nodes`], [`items`], [`from`] |
//! | `let $y := …` | [`try_map`](Pipe::try_map) carrying a tuple |
//! | `where …` | [`try_filter`](Pipe::try_filter) |
//! | nested `for` | [`try_flat_map`](Pipe::try_flat_map) |
//! | `order by …` | [`order_by`](Pipe::order_by) |
//! | `return …` | [`try_map`](Pipe::try_map) building a form |
//!
//! # What is and is not lazy
//!
//! The adapters are pulled on demand, and callbacks run in written order with
//! no reordering and no parallelism. What is *not* lazy: the expression that
//! creates a source has already been evaluated when the source is built, and
//! its sequence is already materialized; [`order_by`](Pipe::order_by) is an
//! explicit materialization boundary; and a splice into a form consumes its
//! iterator immediately. A pipe removes the intermediate vectors between
//! stages — it does not make XML streaming.
//!
//! # After the first error
//!
//! A pipe is fused: once it has yielded an `Err` it yields `None` for ever,
//! and no later predicate, projection, inner iterator or upstream `next` is
//! invoked.
//!
//! ```
//! use bumpalo::Bump;
//! use xsd_schema::compose::{pipe, Composer};
//! use xsd_schema::namespace::NameTable;
//! use xsd_schema::navigator::DomNavigator;
//!
//! let arena = Bump::new();
//! let names = NameTable::new();
//! let c = Composer::new(&arena, &names);
//! let doc = c.load_str("<a><b n='1'/><b n='2'/><b n='3'/></a>")?;
//!
//! let kept: Vec<String> = pipe::nodes(c.eval("//b", &[], Some(doc.root()), Vec::new())?)
//!     .try_filter(|b| c.eval("@n > 1", &[], Some(b.clone()), Vec::new())?.boolean())
//!     .try_map(|b| c.eval("string(@n)", &[], Some(b), Vec::new())?.string())
//!     .collect::<Result<Vec<_>, _>>()?;
//!
//! assert_eq!(kept, ["2", "3"]);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::vec;

use crate::types::value::XmlValue;
use crate::xpath::XmlItem;

use super::order::{self, Direction, EmptyOrder};
use super::{ComposeError, Nav, Value};

/// A fallible iterator of host rows.
///
/// `Pipe` implements [`Iterator`] with `Item = Result<T, ComposeError>`, so
/// `for`, `collect::<Result<Vec<_>, _>>()` and every ordinary adapter work on
/// it as well as the `try_` adapters below.
///
/// ```
/// use xsd_schema::compose::{pipe, ComposeError};
///
/// let doubled: Vec<i32> = pipe::from(1..4)
///     .try_map(|n| Ok(n * 2))
///     .collect::<Result<Vec<_>, ComposeError>>()?;
/// assert_eq!(doubled, [2, 4, 6]);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct Pipe<I> {
    inner: I,
    /// Set when the pipe has yielded an error or run out; both stop the pull.
    done: bool,
}

impl<I> Pipe<I> {
    /// Wraps an iterator of results.
    fn wrap(inner: I) -> Self {
        Self { inner, done: false }
    }
}

impl<I, T> Iterator for Pipe<I>
where
    I: Iterator<Item = Result<T, ComposeError>>,
{
    type Item = Result<T, ComposeError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        match self.inner.next() {
            None => {
                self.done = true;
                None
            }
            Some(Err(error)) => {
                self.done = true;
                Some(Err(error))
            }
            row => row,
        }
    }
}

impl<I, T> Pipe<I>
where
    I: Iterator<Item = Result<T, ComposeError>>,
{
    /// Keeps the rows the predicate accepts.
    ///
    /// A rejected row is skipped, an accepted row is passed through unchanged,
    /// and a predicate error ends the pipe.
    ///
    /// ```
    /// use xsd_schema::compose::{pipe, ComposeError};
    ///
    /// let odd: Vec<i32> = pipe::from(1..6)
    ///     .try_filter(|n| Ok(n % 2 == 1))
    ///     .collect::<Result<Vec<_>, ComposeError>>()?;
    /// assert_eq!(odd, [1, 3, 5]);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn try_filter<P>(self, predicate: P) -> Pipe<TryFilter<Self, P>>
    where
        P: FnMut(&T) -> Result<bool, ComposeError>,
    {
        Pipe::wrap(TryFilter {
            inner: self,
            predicate,
        })
    }

    /// Turns each row into exactly one new row.
    ///
    /// This is both `let` — return a tuple that carries the extra value
    /// alongside the row — and `return`, which projects a row into a
    /// [`Form`](crate::compose::Form).
    ///
    /// ```
    /// use xsd_schema::compose::{pipe, ComposeError};
    ///
    /// let pairs: Vec<(i32, i32)> = pipe::from([1, 2])
    ///     .try_map(|n| Ok((n, n * n)))
    ///     .collect::<Result<Vec<_>, ComposeError>>()?;
    /// assert_eq!(pairs, [(1, 1), (2, 4)]);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn try_map<U, F>(self, project: F) -> Pipe<TryMap<Self, F>>
    where
        F: FnMut(T) -> Result<U, ComposeError>,
    {
        Pipe::wrap(TryMap {
            inner: self,
            project,
        })
    }

    /// Expands each row into a sequence of rows, exhausting each inner
    /// sequence before advancing the outer one.
    ///
    /// This is the nested `for`: the inner sequence is created per outer row,
    /// so the pairs come out in the query's own nested order.
    ///
    /// ```
    /// use xsd_schema::compose::{pipe, ComposeError};
    ///
    /// let pairs: Vec<(i32, i32)> = pipe::from([1, 2])
    ///     .try_flat_map(|outer| {
    ///         Ok(pipe::from([10, 20]).try_map(move |inner| Ok((outer, inner))))
    ///     })
    ///     .collect::<Result<Vec<_>, ComposeError>>()?;
    /// assert_eq!(pairs, [(1, 10), (1, 20), (2, 10), (2, 20)]);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn try_flat_map<U, J, F>(self, expand: F) -> Pipe<TryFlatMap<Self, F, J>>
    where
        F: FnMut(T) -> Result<J, ComposeError>,
        J: IntoIterator<Item = Result<U, ComposeError>>,
    {
        Pipe::wrap(TryFlatMap {
            inner: self,
            expand,
            current: None,
        })
    }

    /// Consumes the pipe, sorts the rows, and hands back a pipe over the
    /// sorted rows.
    ///
    /// This is the explicit materialization boundary: every upstream row is
    /// pulled, `key` is evaluated once per row, and the sort follows
    /// [`order`] — XDM comparison, stable, with the
    /// documented placement for empty keys and `NaN`. An upstream error, a key
    /// error and a comparison error all surface here rather than later.
    ///
    /// ```
    /// use xsd_schema::compose::order::{Direction, EmptyOrder};
    /// use xsd_schema::compose::{pipe, ComposeError};
    /// use xsd_schema::types::value::XmlValue;
    ///
    /// let sorted: Vec<i32> = pipe::from([3, 1, 2])
    ///     .order_by(Direction::Ascending, EmptyOrder::Least, |n| {
    ///         Ok(Some(XmlValue::integer((*n).into())))
    ///     })?
    ///     .collect::<Result<Vec<_>, ComposeError>>()?;
    /// assert_eq!(sorted, [1, 2, 3]);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[allow(clippy::type_complexity)]
    pub fn order_by<F>(
        self,
        direction: Direction,
        empty: EmptyOrder,
        key: F,
    ) -> Result<Pipe<vec::IntoIter<Result<T, ComposeError>>>, ComposeError>
    where
        F: FnMut(&T) -> Result<Option<XmlValue>, ComposeError>,
    {
        let mut rows: Vec<T> = Vec::new();
        for row in self {
            rows.push(row?);
        }
        order::sort_by_key(&mut rows, direction, empty, key)?;
        let ordered: Vec<Result<T, ComposeError>> = rows.into_iter().map(Ok).collect();
        Ok(Pipe::wrap(ordered.into_iter()))
    }
}

// ── Sources ───────────────────────────────────────────────────────────

/// Every item of a result, in order, untouched.
///
/// Nodes are not copied and atomic values are not converted: this is the XDM
/// sequence itself, one item per row. A one-item result yields one row.
///
/// ```
/// use bumpalo::Bump;
/// use xsd_schema::compose::{pipe, ComposeError, Composer};
/// use xsd_schema::namespace::NameTable;
/// use xsd_schema::xpath::XmlItem;
///
/// let arena = Bump::new();
/// let names = NameTable::new();
/// let c = Composer::new(&arena, &names);
///
/// let items = pipe::items(c.eval("(1, 'two', 3)", &[], None, Vec::new())?)
///     .collect::<Result<Vec<_>, ComposeError>>()?;
/// assert_eq!(items.len(), 3);
/// assert!(matches!(items[1], XmlItem::Atomic(_)));
///
/// // A singleton is one row, not none.
/// assert_eq!(pipe::items(c.eval("42", &[], None, Vec::new())?).count(), 1);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn items(value: Value<'_>) -> Pipe<Items<'_>> {
    Pipe::wrap(Items {
        inner: value.into_inner().into_vec().into_iter(),
    })
}

/// Every node of a result, in order.
///
/// An atomic item is [`ComposeError::NotANode`] *at that position*: the rows
/// before it are yielded first, and the pipe ends there.
///
/// ```
/// use bumpalo::Bump;
/// use xsd_schema::compose::{pipe, ComposeError, Composer};
/// use xsd_schema::namespace::NameTable;
/// use xsd_schema::navigator::DomNavigator;
///
/// let arena = Bump::new();
/// let names = NameTable::new();
/// let c = Composer::new(&arena, &names);
/// let doc = c.load_str("<a><b/><b/></a>")?;
///
/// let names_seen: Vec<String> = pipe::nodes(c.eval("//b", &[], Some(doc.root()), Vec::new())?)
///     .try_map(|n| Ok(n.local_name().to_string()))
///     .collect::<Result<Vec<_>, ComposeError>>()?;
/// assert_eq!(names_seen, ["b", "b"]);
///
/// // An atomic item is refused rather than skipped.
/// let mixed = pipe::nodes(c.eval("(1, 2)", &[], None, Vec::new())?)
///     .collect::<Result<Vec<_>, ComposeError>>();
/// assert!(matches!(mixed, Err(ComposeError::NotANode)));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn nodes(value: Value<'_>) -> Pipe<Nodes<'_>> {
    Pipe::wrap(Nodes {
        inner: value.into_inner().into_vec().into_iter(),
    })
}

/// Adapts an ordinary iterator: every item becomes a successful row.
///
/// ```
/// use xsd_schema::compose::{pipe, ComposeError};
///
/// let rows: Vec<i32> = pipe::from(1..4).collect::<Result<Vec<_>, ComposeError>>()?;
/// assert_eq!(rows, [1, 2, 3]);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn from<I>(iter: I) -> Pipe<FromIter<I::IntoIter>>
where
    I: IntoIterator,
{
    Pipe::wrap(FromIter {
        inner: iter.into_iter(),
    })
}

/// Adapts an iterator that already yields results, keeping the failures.
///
/// ```
/// use xsd_schema::compose::{pipe, ComposeError};
///
/// let rows = pipe::from_results(vec![Ok(1), Err(ComposeError::NotANode), Ok(3)])
///     .collect::<Result<Vec<_>, ComposeError>>();
/// assert!(matches!(rows, Err(ComposeError::NotANode)));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn from_results<I, T>(iter: I) -> Pipe<I::IntoIter>
where
    I: IntoIterator<Item = Result<T, ComposeError>>,
{
    Pipe::wrap(iter.into_iter())
}

// ── Source iterators ──────────────────────────────────────────────────

/// The iterator behind [`items`].
///
/// It is named so the return type of [`items`] can be written down; a host
/// consumes it through [`Pipe`].
///
/// ```
/// use xsd_schema::compose::pipe::{Items, Pipe};
///
/// fn count_items(rows: Pipe<Items<'_>>) -> usize {
///     rows.count()
/// }
/// # let _ = count_items;
/// ```
pub struct Items<'a> {
    inner: vec::IntoIter<XmlItem<Nav<'a>>>,
}

impl<'a> Iterator for Items<'a> {
    type Item = Result<XmlItem<Nav<'a>>, ComposeError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(Ok)
    }
}

/// The iterator behind [`nodes`].
///
/// It is named so the return type of [`nodes`] can be written down; a host
/// consumes it through [`Pipe`].
///
/// ```
/// use xsd_schema::compose::pipe::{Nodes, Pipe};
///
/// fn count_nodes(rows: Pipe<Nodes<'_>>) -> usize {
///     rows.count()
/// }
/// # let _ = count_nodes;
/// ```
pub struct Nodes<'a> {
    inner: vec::IntoIter<XmlItem<Nav<'a>>>,
}

impl<'a> Iterator for Nodes<'a> {
    type Item = Result<Nav<'a>, ComposeError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.inner.next()? {
            XmlItem::Node(node) => Some(Ok(node)),
            XmlItem::Atomic(_) => Some(Err(ComposeError::NotANode)),
        }
    }
}

/// The iterator behind [`from`].
///
/// ```
/// use std::ops::Range;
///
/// use xsd_schema::compose::pipe::{self, FromIter, Pipe};
///
/// let rows: Pipe<FromIter<Range<i32>>> = pipe::from(0..3);
/// assert_eq!(rows.count(), 3);
/// ```
pub struct FromIter<I> {
    inner: I,
}

impl<I: Iterator> Iterator for FromIter<I> {
    type Item = Result<I::Item, ComposeError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(Ok)
    }
}

// ── Adapter iterators ─────────────────────────────────────────────────

/// The iterator behind [`Pipe::try_filter`].
///
/// ```
/// use std::ops::Range;
///
/// use xsd_schema::compose::pipe::{self, FromIter, Pipe, TryFilter};
///
/// let rows: Pipe<TryFilter<Pipe<FromIter<Range<i32>>>, _>> =
///     pipe::from(0..3).try_filter(|n| Ok(*n > 0));
/// assert_eq!(rows.count(), 2);
/// ```
pub struct TryFilter<I, P> {
    inner: I,
    predicate: P,
}

impl<I, P, T> Iterator for TryFilter<I, P>
where
    I: Iterator<Item = Result<T, ComposeError>>,
    P: FnMut(&T) -> Result<bool, ComposeError>,
{
    type Item = Result<T, ComposeError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.inner.next()? {
                Err(error) => return Some(Err(error)),
                Ok(row) => match (self.predicate)(&row) {
                    Err(error) => return Some(Err(error)),
                    Ok(true) => return Some(Ok(row)),
                    Ok(false) => continue,
                },
            }
        }
    }
}

/// The iterator behind [`Pipe::try_map`].
///
/// ```
/// use std::ops::Range;
///
/// use xsd_schema::compose::pipe::{self, FromIter, Pipe, TryMap};
///
/// let rows: Pipe<TryMap<Pipe<FromIter<Range<i32>>>, _>> =
///     pipe::from(0..3).try_map(|n| Ok(n * 2));
/// assert_eq!(rows.count(), 3);
/// ```
pub struct TryMap<I, F> {
    inner: I,
    project: F,
}

impl<I, F, T, U> Iterator for TryMap<I, F>
where
    I: Iterator<Item = Result<T, ComposeError>>,
    F: FnMut(T) -> Result<U, ComposeError>,
{
    type Item = Result<U, ComposeError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.inner.next()? {
            Err(error) => Some(Err(error)),
            Ok(row) => Some((self.project)(row)),
        }
    }
}

/// The iterator behind [`Pipe::try_flat_map`].
///
/// Its third parameter is the inner sequence's type, so the whole type is
/// usually left to inference.
///
/// ```
/// use xsd_schema::compose::pipe;
///
/// let rows = pipe::from(0..2).try_flat_map(|n| Ok(pipe::from([n, n])));
/// assert_eq!(rows.count(), 4);
/// ```
pub struct TryFlatMap<I, F, J: IntoIterator> {
    inner: I,
    expand: F,
    current: Option<J::IntoIter>,
}

impl<I, F, J, T, U> Iterator for TryFlatMap<I, F, J>
where
    I: Iterator<Item = Result<T, ComposeError>>,
    F: FnMut(T) -> Result<J, ComposeError>,
    J: IntoIterator<Item = Result<U, ComposeError>>,
{
    type Item = Result<U, ComposeError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(inner) = self.current.as_mut() {
                match inner.next() {
                    Some(Ok(row)) => return Some(Ok(row)),
                    Some(Err(error)) => {
                        self.current = None;
                        return Some(Err(error));
                    }
                    // This inner sequence is exhausted, including when it was
                    // empty; advance the outer one.
                    None => self.current = None,
                }
            }
            match self.inner.next()? {
                Err(error) => return Some(Err(error)),
                Ok(row) => match (self.expand)(row) {
                    Err(error) => return Some(Err(error)),
                    Ok(sequence) => self.current = Some(sequence.into_iter()),
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use bumpalo::Bump;

    use super::*;
    use crate::compose::{Composer, Doc};
    use crate::namespace::NameTable;
    use crate::navigator::DomNavigator;

    /// A source that yields a recorded script of results and counts how often
    /// it was polled.
    struct Script<T> {
        rows: vec::IntoIter<Result<T, ComposeError>>,
        polls: Rc<RefCell<usize>>,
    }

    impl<T> Iterator for Script<T> {
        type Item = Result<T, ComposeError>;

        fn next(&mut self) -> Option<Self::Item> {
            *self.polls.borrow_mut() += 1;
            self.rows.next()
        }
    }

    fn collect<I, T>(pipe: Pipe<I>) -> Result<Vec<T>, ComposeError>
    where
        I: Iterator<Item = Result<T, ComposeError>>,
    {
        pipe.collect()
    }

    fn tracked<T>(rows: Vec<Result<T, ComposeError>>) -> (Pipe<Script<T>>, Rc<RefCell<usize>>) {
        let polls = Rc::new(RefCell::new(0usize));
        let source = Script {
            rows: rows.into_iter(),
            polls: Rc::clone(&polls),
        };
        (from_results(source), polls)
    }

    fn doc<'a>(c: &Composer<'a>, xml: &str) -> Doc<'a> {
        c.load_str(xml).expect("the test XML parses")
    }

    // ── Sources ───────────────────────────────────────────────────────

    #[test]
    fn items_covers_empty_singleton_and_many() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);

        assert_eq!(
            collect(items(c.eval("()", &[], None, Vec::new()).unwrap()))
                .unwrap()
                .len(),
            0
        );
        assert_eq!(
            collect(items(c.eval("42", &[], None, Vec::new()).unwrap()))
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            collect(items(c.eval("(1, 2, 3)", &[], None, Vec::new()).unwrap()))
                .unwrap()
                .len(),
            3
        );
    }

    #[test]
    fn nodes_covers_empty_singleton_and_many() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);
        let d = doc(&c, "<a><b/><b/></a>");

        for (expr, expected) in [("//zzz", 0usize), ("//a", 1), ("//b", 2)] {
            let value = c.eval(expr, &[], Some(d.root()), Vec::new()).unwrap();
            assert_eq!(collect(nodes(value)).unwrap().len(), expected, "{expr}");
        }
    }

    #[test]
    fn nodes_refuses_an_atomic_item_at_its_position() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);
        let d = doc(&c, "<a><b/></a>");

        let value = c
            .eval("(//b, 'oops', //b)", &[], Some(d.root()), Vec::new())
            .unwrap();
        let mut pipe = nodes(value);
        assert!(pipe.next().unwrap().is_ok());
        assert!(matches!(pipe.next(), Some(Err(ComposeError::NotANode))));
        // Fused: the third item is never reached.
        assert!(pipe.next().is_none());
    }

    #[test]
    fn from_and_from_results_adapt_plain_iterators() {
        assert_eq!(collect(from(1..4)).unwrap(), [1, 2, 3]);
        assert_eq!(collect(from(Vec::<i32>::new())).unwrap(), []);
        assert_eq!(
            collect(from_results(vec![Ok(1), Ok(2)])).unwrap(),
            [1i32, 2]
        );
        assert!(matches!(
            collect(from_results(vec![Ok(1), Err(ComposeError::NotANode)])),
            Err(ComposeError::NotANode)
        ));
    }

    // ── Error propagation ─────────────────────────────────────────────

    #[test]
    fn a_source_error_stops_the_pipe() {
        let (pipe, polls) = tracked(vec![Ok(1), Err(ComposeError::NotANode), Ok(3)]);
        let seen = RefCell::new(Vec::new());
        let mut pipe = pipe.try_map(|n| {
            seen.borrow_mut().push(n);
            Ok(n)
        });
        assert_eq!(pipe.next().unwrap().unwrap(), 1);
        assert!(matches!(pipe.next(), Some(Err(ComposeError::NotANode))));
        assert!(pipe.next().is_none());
        // The projection never saw the row after the error, and the source was
        // not polled again either.
        assert_eq!(*seen.borrow(), [1]);
        assert_eq!(*polls.borrow(), 2);
    }

    #[test]
    fn a_predicate_error_stops_the_pipe() {
        let seen = RefCell::new(Vec::new());
        let result = collect(from(1..4).try_filter(|n| {
            seen.borrow_mut().push(*n);
            if *n == 2 {
                Err(ComposeError::NotANode)
            } else {
                Ok(true)
            }
        }));
        assert!(matches!(result, Err(ComposeError::NotANode)));
        assert_eq!(*seen.borrow(), [1, 2]);
    }

    #[test]
    fn a_projection_error_stops_the_pipe() {
        let seen = RefCell::new(Vec::new());
        let result = collect(from(1..4).try_map(|n| {
            seen.borrow_mut().push(n);
            if n == 2 {
                Err(ComposeError::NotANode)
            } else {
                Ok(n)
            }
        }));
        assert!(matches!(result, Err(ComposeError::NotANode)));
        assert_eq!(*seen.borrow(), [1, 2]);
    }

    #[test]
    fn an_inner_iterator_error_stops_the_pipe() {
        let expansions = RefCell::new(Vec::new());
        let result = collect(from(1..4).try_flat_map(|n| {
            expansions.borrow_mut().push(n);
            Ok(from_results(vec![
                Ok(n * 10),
                if n == 2 {
                    Err(ComposeError::NotANode)
                } else {
                    Ok(n * 100)
                },
            ]))
        }));
        assert!(matches!(result, Err(ComposeError::NotANode)));
        // The third outer row is never expanded.
        assert_eq!(*expansions.borrow(), [1, 2]);
    }

    #[test]
    fn an_expansion_error_stops_the_pipe() {
        let result = collect(from(1..4).try_flat_map(|n| {
            if n == 1 {
                Err(ComposeError::NotANode)
            } else {
                Ok(from([n]))
            }
        }));
        assert!(matches!(result, Err(ComposeError::NotANode)));
    }

    #[test]
    fn a_key_error_stops_order_by() {
        let result = from(1..4).order_by(Direction::Ascending, EmptyOrder::Least, |n| {
            if *n == 2 {
                Err(ComposeError::NotANode)
            } else {
                Ok(Some(XmlValue::integer((*n).into())))
            }
        });
        assert!(matches!(result, Err(ComposeError::NotANode)));
    }

    #[test]
    fn a_comparison_error_stops_order_by() {
        let result = from(["a", "1"]).order_by(Direction::Ascending, EmptyOrder::Least, |n| {
            Ok(Some(match *n {
                "a" => XmlValue::string("a"),
                other => XmlValue::integer(other.parse::<i64>().unwrap().into()),
            }))
        });
        match result {
            Err(ComposeError::XPath { source, .. }) => {
                assert_eq!(source.error_code(), Some("XPTY0004"))
            }
            Err(other) => panic!("expected XPTY0004, got {other:?}"),
            Ok(_) => panic!("expected XPTY0004, got a sorted pipe"),
        }
    }

    #[test]
    fn an_upstream_error_stops_order_by() {
        let result = from_results(vec![Ok(1i64), Err(ComposeError::NotANode)]).order_by(
            Direction::Ascending,
            EmptyOrder::Least,
            |n| Ok(Some(XmlValue::integer((*n).into()))),
        );
        assert!(matches!(result, Err(ComposeError::NotANode)));
    }

    // ── Callback order ────────────────────────────────────────────────

    #[test]
    fn callbacks_run_in_the_written_order() {
        let log: RefCell<Vec<&str>> = RefCell::new(Vec::new());
        let rows = collect(
            from(1..3)
                .try_filter(|_| {
                    log.borrow_mut().push("filter");
                    Ok(true)
                })
                .try_map(|n| {
                    log.borrow_mut().push("map");
                    Ok(n)
                }),
        )
        .unwrap();
        assert_eq!(rows, [1, 2]);
        assert_eq!(*log.borrow(), ["filter", "map", "filter", "map"]);
    }

    #[test]
    fn no_callback_runs_after_the_first_error() {
        let log: RefCell<Vec<&str>> = RefCell::new(Vec::new());
        let result = collect(
            from(1..4)
                .try_filter(|n| {
                    log.borrow_mut().push("filter");
                    if *n == 2 {
                        Err(ComposeError::NotANode)
                    } else {
                        Ok(true)
                    }
                })
                .try_map(|n| {
                    log.borrow_mut().push("map");
                    Ok(n)
                }),
        );
        assert!(matches!(result, Err(ComposeError::NotANode)));
        assert_eq!(*log.borrow(), ["filter", "map", "filter"]);
    }

    #[test]
    fn the_pipe_is_fused_after_an_error() {
        let mut pipe = from_results(vec![Err(ComposeError::NotANode), Ok(1)]).try_map(Ok);
        assert!(matches!(pipe.next(), Some(Err(ComposeError::NotANode))));
        for _ in 0..3 {
            assert!(pipe.next().is_none());
        }
    }

    // ── try_flat_map ──────────────────────────────────────────────────

    #[test]
    fn an_empty_inner_sequence_contributes_nothing() {
        let rows = collect(from(1..4).try_flat_map(|n| {
            Ok(if n == 2 {
                from(Vec::<i32>::new())
            } else {
                from(vec![n])
            })
        }))
        .unwrap();
        assert_eq!(rows, [1, 3]);
    }

    #[test]
    fn every_inner_sequence_is_exhausted_before_the_next_outer_row() {
        let rows = collect(
            from([1, 2]).try_flat_map(|outer| Ok(from([10, 20]).try_map(move |i| Ok((outer, i))))),
        )
        .unwrap();
        assert_eq!(rows, [(1, 10), (1, 20), (2, 10), (2, 20)]);
    }

    #[test]
    fn order_and_duplicates_survive() {
        let rows = collect(from(["b", "a", "b"]).try_map(Ok)).unwrap();
        assert_eq!(rows, ["b", "a", "b"]);
    }

    #[test]
    fn navigator_identity_survives_the_pipe() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);
        let d = doc(&c, "<a><b/></a>");

        let original = c
            .eval("//b", &[], Some(d.root()), Vec::new())
            .unwrap()
            .nodes()
            .unwrap();
        let value = c.eval("//b", &[], Some(d.root()), Vec::new()).unwrap();

        // The same node, cloned through two stages and an expansion, is still
        // the same node.
        let through =
            collect(nodes(value).try_map(Ok).try_flat_map(|n| Ok(from(vec![n])))).unwrap();
        assert_eq!(through.len(), 1);
        assert!(through[0].is_same_position(&original[0]));
    }

    // ── order_by ──────────────────────────────────────────────────────

    #[test]
    fn order_by_is_stable_and_evaluates_each_key_once() {
        let calls = RefCell::new(0usize);
        let rows = collect(
            from([("a", 1i64), ("b", 0), ("c", 1), ("d", 0)])
                .order_by(Direction::Ascending, EmptyOrder::Least, |row| {
                    *calls.borrow_mut() += 1;
                    Ok(Some(XmlValue::integer(row.1.into())))
                })
                .unwrap(),
        )
        .unwrap();
        assert_eq!(rows, [("b", 0), ("d", 0), ("a", 1), ("c", 1)]);
        assert_eq!(*calls.borrow(), 4);
    }

    #[test]
    fn order_by_places_empty_keys_and_nan() {
        let rows = collect(
            from(["one", "nan", "empty"])
                .order_by(Direction::Ascending, EmptyOrder::Least, |row| {
                    Ok(match *row {
                        "empty" => None,
                        "nan" => Some(XmlValue::double(f64::NAN)),
                        _ => Some(XmlValue::double(1.0)),
                    })
                })
                .unwrap(),
        )
        .unwrap();
        assert_eq!(rows, ["empty", "nan", "one"]);
    }

    #[test]
    fn order_by_over_an_empty_pipe() {
        let rows = collect(
            from(Vec::<i64>::new())
                .order_by(Direction::Ascending, EmptyOrder::Least, |n| {
                    Ok(Some(XmlValue::integer((*n).into())))
                })
                .unwrap(),
        )
        .unwrap();
        assert!(rows.is_empty());
    }
}
