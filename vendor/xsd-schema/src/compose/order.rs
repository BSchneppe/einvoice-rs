//! `order by`, as a sort over host rows.
//!
//! The comparison is XDM's, not Rust's: keys are compared with
//! [`value_lt`] and
//! [`value_eq`] under the codepoint
//! collation, an `xs:untypedAtomic` key is cast to `xs:string` first, and two
//! keys of incomparable types are a type error rather than an arbitrary order.
//! The engine refuses such a pair as "operator not defined"; that refusal is
//! reported here as `XPTY0004`, the code XQuery assigns to a bad `order by`
//! key, with the engine's own words and the two type codes in the message.
//!
//! # Where empty keys and `NaN` go
//!
//! XQuery 1.0 §3.8.3 is not available locally, so this is the placement as
//! best understood, pinned by the unit tests in this module: the empty
//! sequence is less than every other key under [`EmptyOrder::Least`] and
//! greater than every other key under [`EmptyOrder::Greatest`], and `NaN`
//! sits immediately next to it — just after the empty key for `Least`, just
//! before it for `Greatest`, so `NaN` precedes every real number in both.
//! [`Direction::Descending`] reverses the whole ordering, empty key included.
//!
//! # Stability
//!
//! Both functions are stable and evaluate each key exactly once: keys are
//! computed up front, a permutation of row indices is sorted, and the rows are
//! then moved into place (decorate, sort, undecorate). `stable` is therefore
//! the only behaviour offered, and the only one needed.

use std::cmp::Ordering;

use crate::types::value::XmlValue;
use crate::types::PrimitiveTypeCode;
use crate::xpath::operators::{value_eq, value_lt};
use crate::xpath::{timsort_slice_by, XPathError};

use super::ComposeError;

/// Which way a key orders.
///
/// ```
/// use xsd_schema::compose::order::Direction;
///
/// assert_ne!(Direction::Ascending, Direction::Descending);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Smallest key first.
    Ascending,
    /// Largest key first.
    Descending,
}

/// Where a row whose key is the empty sequence goes.
///
/// ```
/// use xsd_schema::compose::order::EmptyOrder;
///
/// assert_ne!(EmptyOrder::Least, EmptyOrder::Greatest);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmptyOrder {
    /// An empty key is less than every other key (`empty least`).
    Least,
    /// An empty key is greater than every other key (`empty greatest`).
    Greatest,
}

/// A key selector: one row in, at most one atomic key out.
///
/// The empty answer is `None`, which is XQuery's empty sequence as an
/// `order by` key; where it sorts is [`EmptyOrder`]'s business.
///
/// ```
/// use xsd_schema::compose::order::KeyFn;
/// use xsd_schema::types::value::XmlValue;
///
/// // Any closure of this shape is a key.
/// let mut by_length: Box<KeyFn<'_, &str>> =
///     Box::new(|row: &&str| Ok(Some(XmlValue::integer(row.len().into()))));
/// assert_eq!(by_length(&"abc").unwrap().unwrap().to_string_value(), "3");
/// ```
pub type KeyFn<'f, T> = dyn FnMut(&T) -> Result<Option<XmlValue>, ComposeError> + 'f;

/// One ordering criterion of a multi-key sort.
///
/// ```
/// use xsd_schema::compose::order::{sort_by_keys, Direction, EmptyOrder, KeySpec};
/// use xsd_schema::types::value::XmlValue;
///
/// let mut rows = vec![("b", 1), ("a", 2), ("a", 1)];
/// let mut specs = [
///     KeySpec::new(Direction::Ascending, EmptyOrder::Least, |r: &(&str, i32)| {
///         Ok(Some(XmlValue::string(r.0)))
///     }),
///     KeySpec::new(Direction::Descending, EmptyOrder::Least, |r: &(&str, i32)| {
///         Ok(Some(XmlValue::integer(r.1.into())))
///     }),
/// ];
///
/// sort_by_keys(&mut rows, &mut specs)?;
/// assert_eq!(rows, [("a", 2), ("a", 1), ("b", 1)]);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct KeySpec<'f, T> {
    direction: Direction,
    empty: EmptyOrder,
    key: Box<KeyFn<'f, T>>,
}

impl<'f, T> KeySpec<'f, T> {
    /// A criterion: a direction, a placement for empty keys, and the key
    /// itself.
    ///
    /// ```
    /// use xsd_schema::compose::order::{Direction, EmptyOrder, KeySpec};
    /// use xsd_schema::types::value::XmlValue;
    ///
    /// let spec = KeySpec::new(Direction::Ascending, EmptyOrder::Greatest, |r: &i32| {
    ///     Ok(Some(XmlValue::integer((*r).into())))
    /// });
    /// assert_eq!(spec.direction(), Direction::Ascending);
    /// assert_eq!(spec.empty_order(), EmptyOrder::Greatest);
    /// ```
    pub fn new(
        direction: Direction,
        empty: EmptyOrder,
        key: impl FnMut(&T) -> Result<Option<XmlValue>, ComposeError> + 'f,
    ) -> Self {
        Self {
            direction,
            empty,
            key: Box::new(key),
        }
    }

    /// The direction this criterion sorts in.
    ///
    /// ```
    /// use xsd_schema::compose::order::{Direction, EmptyOrder, KeySpec};
    /// use xsd_schema::types::value::XmlValue;
    ///
    /// let spec = KeySpec::new(Direction::Descending, EmptyOrder::Least, |r: &i32| {
    ///     Ok(Some(XmlValue::integer((*r).into())))
    /// });
    /// assert_eq!(spec.direction(), Direction::Descending);
    /// ```
    pub fn direction(&self) -> Direction {
        self.direction
    }

    /// Where this criterion puts an empty key.
    ///
    /// ```
    /// use xsd_schema::compose::order::{Direction, EmptyOrder, KeySpec};
    /// use xsd_schema::types::value::XmlValue;
    ///
    /// let spec = KeySpec::new(Direction::Ascending, EmptyOrder::Greatest, |r: &i32| {
    ///     Ok(Some(XmlValue::integer((*r).into())))
    /// });
    /// assert_eq!(spec.empty_order(), EmptyOrder::Greatest);
    /// ```
    pub fn empty_order(&self) -> EmptyOrder {
        self.empty
    }
}

/// Sorts `rows` by one key, stably.
///
/// `key` is called exactly once per row, before any comparison. A key error
/// aborts the sort and leaves `rows` untouched; a comparison error does the
/// same.
///
/// ```
/// use xsd_schema::compose::order::{sort_by_key, Direction, EmptyOrder};
/// use xsd_schema::types::value::XmlValue;
///
/// let mut rows = vec![3, 1, 2];
/// sort_by_key(&mut rows, Direction::Ascending, EmptyOrder::Least, |r| {
///     Ok(Some(XmlValue::integer((*r).into())))
/// })?;
/// assert_eq!(rows, [1, 2, 3]);
///
/// sort_by_key(&mut rows, Direction::Descending, EmptyOrder::Least, |r| {
///     Ok(Some(XmlValue::integer((*r).into())))
/// })?;
/// assert_eq!(rows, [3, 2, 1]);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn sort_by_key<T, F>(
    rows: &mut Vec<T>,
    direction: Direction,
    empty: EmptyOrder,
    key: F,
) -> Result<(), ComposeError>
where
    F: FnMut(&T) -> Result<Option<XmlValue>, ComposeError>,
{
    let mut specs = [KeySpec::new(direction, empty, key)];
    sort_by_keys(rows, &mut specs)
}

/// Sorts `rows` by several keys, compared in order, stably.
///
/// Every key of every criterion is evaluated once per row, before any
/// comparison; the criteria are then compared lexicographically, each with its
/// own direction and empty placement.
///
/// ```
/// use xsd_schema::compose::order::{sort_by_keys, Direction, EmptyOrder, KeySpec};
/// use xsd_schema::types::value::XmlValue;
///
/// // Rows whose first key ties keep their input order when nothing breaks it.
/// let mut rows = vec!["bb", "a", "cc", "b"];
/// let mut specs = [KeySpec::new(
///     Direction::Ascending,
///     EmptyOrder::Least,
///     |r: &&str| Ok(Some(XmlValue::integer(r.len().into()))),
/// )];
/// sort_by_keys(&mut rows, &mut specs)?;
/// assert_eq!(rows, ["a", "b", "bb", "cc"]);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn sort_by_keys<T>(
    rows: &mut Vec<T>,
    specs: &mut [KeySpec<'_, T>],
) -> Result<(), ComposeError> {
    if rows.len() < 2 || specs.is_empty() {
        // Still evaluate nothing: a one-row sort has no comparison to make,
        // and no key can change the answer.
        return Ok(());
    }

    // Decorate: every key of every criterion, exactly once per row.
    let mut keys: Vec<Vec<Option<XmlValue>>> = Vec::with_capacity(specs.len());
    for spec in specs.iter_mut() {
        let mut column = Vec::with_capacity(rows.len());
        for row in rows.iter() {
            column.push((spec.key)(row)?.map(cast_untyped_to_string));
        }
        keys.push(column);
    }

    // Sort a permutation, so a row is never cloned and the sort stays stable.
    let mut order: Vec<usize> = (0..rows.len()).collect();
    let mut failure: Option<ComposeError> = None;
    timsort_slice_by(&mut order, |&left, &right| {
        if failure.is_some() {
            return Ordering::Equal;
        }
        for (column, spec) in keys.iter().zip(specs.iter()) {
            match compare_keys(
                column[left].as_ref(),
                column[right].as_ref(),
                spec.direction,
                spec.empty,
            ) {
                Ok(Ordering::Equal) => continue,
                Ok(other) => return other,
                Err(error) => {
                    failure = Some(error);
                    return Ordering::Equal;
                }
            }
        }
        Ordering::Equal
    });
    if let Some(error) = failure {
        return Err(error);
    }

    // Undecorate: move the rows into their new places.
    let mut taken: Vec<Option<T>> = rows.drain(..).map(Some).collect();
    for index in order {
        rows.push(
            taken[index]
                .take()
                .expect("a permutation visits every index once"),
        );
    }
    Ok(())
}

/// An `xs:untypedAtomic` key compares as `xs:string` (XQuery 1.0 §3.8.3).
fn cast_untyped_to_string(key: XmlValue) -> XmlValue {
    if key.is_untyped() {
        XmlValue::string(key.to_string_value())
    } else {
        key
    }
}

/// Where a key sits relative to the empty key and `NaN`.
///
/// Lower is earlier in the ascending order.
fn rank(key: Option<&XmlValue>, empty: EmptyOrder) -> u8 {
    let class = match key {
        None => Class::Empty,
        Some(value) if is_nan(value) => Class::NaN,
        Some(_) => Class::Value,
    };
    match (class, empty) {
        (Class::Empty, EmptyOrder::Least) => 0,
        (Class::NaN, EmptyOrder::Least) => 1,
        (Class::Value, EmptyOrder::Least) => 2,
        (Class::Value, EmptyOrder::Greatest) => 0,
        (Class::NaN, EmptyOrder::Greatest) => 1,
        (Class::Empty, EmptyOrder::Greatest) => 2,
    }
}

enum Class {
    Empty,
    NaN,
    Value,
}

/// Whether a key is `xs:double` or `xs:float` `NaN`.
fn is_nan(value: &XmlValue) -> bool {
    matches!(
        value.primitive_type(),
        Some(PrimitiveTypeCode::Double | PrimitiveTypeCode::Float)
    ) && value.as_double().is_some_and(f64::is_nan)
}

/// The engine's refusal to compare two keys, as the spec's `order by` key type
/// error.
fn incomparable(left: &XmlValue, right: &XmlValue, engine: XPathError) -> ComposeError {
    ComposeError::XPath {
        source: XPathError::type_mismatch(
            "two order-by keys of comparable types",
            format!("{:?} and {:?} ({engine})", left.type_code, right.type_code),
        ),
        expr: String::new(),
    }
}

/// Compares two keys under one criterion.
fn compare_keys(
    left: Option<&XmlValue>,
    right: Option<&XmlValue>,
    direction: Direction,
    empty: EmptyOrder,
) -> Result<Ordering, ComposeError> {
    let ascending = match rank(left, empty).cmp(&rank(right, empty)) {
        Ordering::Equal => match (left, right) {
            // Both in the value class, so both are `Some` and comparable by
            // value; two empty keys and two NaNs tie.
            (Some(a), Some(b)) if !is_nan(a) && !is_nan(b) => {
                if value_eq(a, b).map_err(|engine| incomparable(a, b, engine))? {
                    Ordering::Equal
                } else if value_lt(a, b).map_err(|engine| incomparable(a, b, engine))? {
                    Ordering::Less
                } else {
                    Ordering::Greater
                }
            }
            _ => Ordering::Equal,
        },
        other => other,
    };
    Ok(match direction {
        Direction::Ascending => ascending,
        Direction::Descending => ascending.reverse(),
    })
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    fn int(value: i64) -> XmlValue {
        XmlValue::integer(value.into())
    }

    #[test]
    fn ascending_and_descending() {
        let mut rows = vec![3i64, 1, 2];
        sort_by_key(&mut rows, Direction::Ascending, EmptyOrder::Least, |r| {
            Ok(Some(int(*r)))
        })
        .unwrap();
        assert_eq!(rows, [1, 2, 3]);

        sort_by_key(&mut rows, Direction::Descending, EmptyOrder::Least, |r| {
            Ok(Some(int(*r)))
        })
        .unwrap();
        assert_eq!(rows, [3, 2, 1]);
    }

    #[test]
    fn an_untyped_key_compares_as_a_string() {
        // As numbers these would be 9 < 10; as strings "10" < "9".
        let mut rows = vec!["9", "10"];
        sort_by_key(&mut rows, Direction::Ascending, EmptyOrder::Least, |r| {
            Ok(Some(XmlValue::untyped(*r)))
        })
        .unwrap();
        assert_eq!(rows, ["10", "9"]);
    }

    #[test]
    fn empty_least_puts_an_empty_key_first() {
        let mut rows = vec![Some(2i64), None, Some(1)];
        sort_by_key(&mut rows, Direction::Ascending, EmptyOrder::Least, |r| {
            Ok(r.map(int))
        })
        .unwrap();
        assert_eq!(rows, [None, Some(1), Some(2)]);
    }

    #[test]
    fn empty_greatest_puts_an_empty_key_last() {
        let mut rows = vec![Some(2i64), None, Some(1)];
        sort_by_key(&mut rows, Direction::Ascending, EmptyOrder::Greatest, |r| {
            Ok(r.map(int))
        })
        .unwrap();
        assert_eq!(rows, [Some(1), Some(2), None]);
    }

    #[test]
    fn descending_reverses_the_empty_key_too() {
        let mut rows = vec![Some(2i64), None, Some(1)];
        sort_by_key(&mut rows, Direction::Descending, EmptyOrder::Least, |r| {
            Ok(r.map(int))
        })
        .unwrap();
        assert_eq!(rows, [Some(2), Some(1), None]);
    }

    #[test]
    fn nan_sits_next_to_the_empty_key() {
        // `Least`: empty, then NaN, then the numbers.
        let mut rows = vec!["1", "nan", "empty", "0"];
        sort_by_key(&mut rows, Direction::Ascending, EmptyOrder::Least, |r| {
            Ok(match *r {
                "empty" => None,
                "nan" => Some(XmlValue::double(f64::NAN)),
                other => Some(XmlValue::double(other.parse().unwrap())),
            })
        })
        .unwrap();
        assert_eq!(rows, ["empty", "nan", "0", "1"]);

        // `Greatest`: the numbers, then NaN, then empty.
        let mut rows = vec!["1", "nan", "empty", "0"];
        sort_by_key(&mut rows, Direction::Ascending, EmptyOrder::Greatest, |r| {
            Ok(match *r {
                "empty" => None,
                "nan" => Some(XmlValue::double(f64::NAN)),
                other => Some(XmlValue::double(other.parse().unwrap())),
            })
        })
        .unwrap();
        assert_eq!(rows, ["0", "1", "nan", "empty"]);
    }

    #[test]
    fn incomparable_keys_are_a_type_error() {
        let mut rows = vec!["text", "number"];
        let error = sort_by_key(&mut rows, Direction::Ascending, EmptyOrder::Least, |r| {
            Ok(Some(match *r {
                "text" => XmlValue::string("x"),
                _ => int(1),
            }))
        })
        .expect_err("a string and an integer do not compare");
        match error {
            ComposeError::XPath { source, .. } => {
                assert_eq!(source.error_code(), Some("XPTY0004"))
            }
            other => panic!("expected XPTY0004, got {other:?}"),
        }
        // The rows are left as they were.
        assert_eq!(rows, ["text", "number"]);
    }

    #[test]
    fn a_key_error_aborts_the_sort() {
        let mut rows = vec![1i64, 2, 3];
        let error = sort_by_key(&mut rows, Direction::Ascending, EmptyOrder::Least, |r| {
            if *r == 2 {
                Err(ComposeError::NotANode)
            } else {
                Ok(Some(int(*r)))
            }
        })
        .expect_err("the key failed");
        assert!(matches!(error, ComposeError::NotANode));
        assert_eq!(rows, [1, 2, 3]);
    }

    #[test]
    fn each_key_is_evaluated_exactly_once() {
        let calls = RefCell::new(0usize);
        let mut rows = vec![5i64, 3, 4, 1, 2, 9, 8, 7, 6, 0];
        sort_by_key(&mut rows, Direction::Ascending, EmptyOrder::Least, |r| {
            *calls.borrow_mut() += 1;
            Ok(Some(int(*r)))
        })
        .unwrap();
        assert_eq!(rows, [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
        assert_eq!(*calls.borrow(), 10);
    }

    #[test]
    fn the_sort_is_stable() {
        // Equal keys keep their input order.
        let mut rows = vec![("a", 1), ("b", 1), ("c", 1), ("d", 0)];
        sort_by_key(&mut rows, Direction::Ascending, EmptyOrder::Least, |r| {
            Ok(Some(int(r.1)))
        })
        .unwrap();
        assert_eq!(rows, [("d", 0), ("a", 1), ("b", 1), ("c", 1)]);
    }

    #[test]
    fn several_keys_compare_lexicographically() {
        let mut rows = vec![("b", 1i64), ("a", 2), ("a", 1)];
        let mut specs = [
            KeySpec::new(
                Direction::Ascending,
                EmptyOrder::Least,
                |r: &(&str, i64)| Ok(Some(XmlValue::string(r.0))),
            ),
            KeySpec::new(
                Direction::Descending,
                EmptyOrder::Least,
                |r: &(&str, i64)| Ok(Some(int(r.1))),
            ),
        ];
        sort_by_keys(&mut rows, &mut specs).unwrap();
        assert_eq!(rows, [("a", 2), ("a", 1), ("b", 1)]);
    }

    #[test]
    fn a_short_or_keyless_sort_does_nothing() {
        let mut one = vec![1i64];
        sort_by_key(&mut one, Direction::Ascending, EmptyOrder::Least, |_| {
            panic!("no key is needed for one row")
        })
        .unwrap();
        assert_eq!(one, [1]);

        let mut rows = vec![2i64, 1];
        sort_by_keys(&mut rows, &mut []).unwrap();
        assert_eq!(rows, [2, 1]);
    }

    #[test]
    fn key_spec_reports_its_own_settings() {
        let spec = KeySpec::new(Direction::Descending, EmptyOrder::Greatest, |r: &i64| {
            Ok(Some(int(*r)))
        });
        assert_eq!(spec.direction(), Direction::Descending);
        assert_eq!(spec.empty_order(), EmptyOrder::Greatest);
    }
}
