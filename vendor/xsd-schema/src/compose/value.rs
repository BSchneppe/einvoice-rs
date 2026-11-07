//! Query results, and the Rust values that can be bound into a query.
//!
//! [`Value`] is what an evaluation hands back: an XDM sequence, unchanged,
//! with the conversions a host actually asks for on top of it.
//! [`IntoXPathValue`] is the way in: it turns Rust scalars, navigators,
//! documents and sequences into the engine's value type so they can be bound
//! to an external variable.

use std::fmt;

use num_bigint::BigInt;
use rust_decimal::Decimal;

use crate::types::value::XmlValue;
use crate::xpath::atomize::{atomize_node, node_atoms, single_atom, to_number, Atoms};
use crate::xpath::functions::effective_boolean_value;
use crate::xpath::{XPathError, XPathValue, XmlItem};

use super::{ComposeError, Doc, Nav};

/// The result of one evaluation.
///
/// A `Value` is a thin wrapper around the engine's own
/// [`XPathValue`]: nothing is atomized, copied or
/// flattened until a method asks for it. The accessors are the ones a host
/// needs — the nodes, the atomic values, a boolean, a string, a number, an
/// order-by key — and each says exactly when it refuses.
///
/// ```
/// use bumpalo::Bump;
/// use xsd_schema::compose::Composer;
/// use xsd_schema::namespace::NameTable;
///
/// let arena = Bump::new();
/// let names = NameTable::new();
/// let c = Composer::new(&arena, &names);
/// let doc = c.load_str("<a><b>1</b><b>2</b></a>")?;
///
/// let bs = c.eval("//b", &[], Some(doc.root()), Vec::new())?;
/// assert_eq!(bs.len(), 2);
/// assert_eq!(bs.nodes()?.len(), 2);
///
/// let total = c.eval("sum(//b)", &[], Some(doc.root()), Vec::new())?;
/// assert_eq!(total.number()?, 3.0);
/// assert_eq!(total.string()?, "3");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone)]
pub struct Value<'a>(XPathValue<Nav<'a>>);

impl<'a> Value<'a> {
    /// The node items, in order.
    ///
    /// An atomic item is [`ComposeError::NotANode`]: dropping it silently
    /// would hide a mistake in the expression.
    /// [`pipe::nodes`](crate::compose::pipe::nodes) is the streaming
    /// counterpart.
    ///
    /// ```
    /// use bumpalo::Bump;
    /// use xsd_schema::compose::{ComposeError, Composer};
    /// use xsd_schema::namespace::NameTable;
    /// use xsd_schema::navigator::DomNavigator;
    ///
    /// let arena = Bump::new();
    /// let names = NameTable::new();
    /// let c = Composer::new(&arena, &names);
    /// let doc = c.load_str("<a><b/></a>")?;
    ///
    /// let nodes = c.eval("//b", &[], Some(doc.root()), Vec::new())?.nodes()?;
    /// assert_eq!(nodes[0].local_name(), "b");
    ///
    /// // A string is not a node.
    /// let atomic = c.eval("'x'", &[], None, Vec::new())?;
    /// assert!(matches!(atomic.nodes(), Err(ComposeError::NotANode)));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn nodes(&self) -> Result<Vec<Nav<'a>>, ComposeError> {
        let mut out = Vec::with_capacity(self.len());
        for item in self.iter() {
            match item {
                XmlItem::Node(node) => out.push(node.clone()),
                XmlItem::Atomic(_) => return Err(ComposeError::NotANode),
            }
        }
        Ok(out)
    }

    /// Every item atomized, in order.
    ///
    /// A node is atomized with
    /// [`atomize_node`], so an untyped
    /// element yields its `xs:untypedAtomic` string value and a typed one its
    /// typed value: one value per member when its type is a list type
    /// (XDM §3.3.1.2). A nilled element contributes nothing.
    ///
    /// ```
    /// use bumpalo::Bump;
    /// use xsd_schema::compose::Composer;
    /// use xsd_schema::namespace::NameTable;
    ///
    /// let arena = Bump::new();
    /// let names = NameTable::new();
    /// let c = Composer::new(&arena, &names);
    /// let doc = c.load_str("<a><b>1</b><b>2</b></a>")?;
    ///
    /// let atomics = c.eval("//b", &[], Some(doc.root()), Vec::new())?.atomics()?;
    /// let text: Vec<String> = atomics.iter().map(|v| v.to_string_value()).collect();
    /// assert_eq!(text, ["1", "2"]);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn atomics(&self) -> Result<Vec<XmlValue>, ComposeError> {
        let mut out = Vec::with_capacity(self.len());
        for item in self.iter() {
            match item {
                XmlItem::Atomic(value) => out.extend(Atoms::of(value.clone())),
                XmlItem::Node(node) => out.extend(node_atoms(node)?),
            }
        }
        Ok(out)
    }

    /// The effective boolean value (XPath 2.0 §2.4.3).
    ///
    /// ```
    /// use bumpalo::Bump;
    /// use xsd_schema::compose::Composer;
    /// use xsd_schema::namespace::NameTable;
    ///
    /// let arena = Bump::new();
    /// let names = NameTable::new();
    /// let c = Composer::new(&arena, &names);
    /// let doc = c.load_str("<a><b>x</b></a>")?;
    ///
    /// assert!(c.eval("//b", &[], Some(doc.root()), Vec::new())?.boolean()?);
    /// assert!(!c.eval("//zzz", &[], Some(doc.root()), Vec::new())?.boolean()?);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn boolean(&self) -> Result<bool, ComposeError> {
        Ok(effective_boolean_value(&self.0)?)
    }

    /// The string value of the single item, `""` for the empty sequence.
    ///
    /// More than one item is [`ComposeError::NotSingleton`]; use
    /// [`atomics`](Self::atomics) to join a sequence yourself.
    ///
    /// ```
    /// use bumpalo::Bump;
    /// use xsd_schema::compose::{ComposeError, Composer};
    /// use xsd_schema::namespace::NameTable;
    ///
    /// let arena = Bump::new();
    /// let names = NameTable::new();
    /// let c = Composer::new(&arena, &names);
    /// let doc = c.load_str("<a><b>one</b><b>two</b></a>")?;
    ///
    /// assert_eq!(c.eval("//b[1]", &[], Some(doc.root()), Vec::new())?.string()?, "one");
    /// assert_eq!(c.eval("()", &[], None, Vec::new())?.string()?, "");
    /// assert!(matches!(
    ///     c.eval("//b", &[], Some(doc.root()), Vec::new())?.string(),
    ///     Err(ComposeError::NotSingleton { len: 2 }),
    /// ));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn string(&self) -> Result<String, ComposeError> {
        match self.single()? {
            None => Ok(String::new()),
            Some(XmlItem::Atomic(value)) => Ok(value.to_string_value()),
            Some(XmlItem::Node(node)) => Ok(crate::navigator::DomNavigator::value(node)),
        }
    }

    /// `fn:number()` of the single item: `NaN` when it does not convert, and
    /// `NaN` for the empty sequence.
    ///
    /// More than one item is [`ComposeError::NotSingleton`].
    ///
    /// ```
    /// use bumpalo::Bump;
    /// use xsd_schema::compose::Composer;
    /// use xsd_schema::namespace::NameTable;
    ///
    /// let arena = Bump::new();
    /// let names = NameTable::new();
    /// let c = Composer::new(&arena, &names);
    ///
    /// assert_eq!(c.eval("2 + 3", &[], None, Vec::new())?.number()?, 5.0);
    /// assert!(c.eval("'nope'", &[], None, Vec::new())?.number()?.is_nan());
    /// assert!(c.eval("()", &[], None, Vec::new())?.number()?.is_nan());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn number(&self) -> Result<f64, ComposeError> {
        match self.atomize_single()? {
            None => Ok(f64::NAN),
            Some(value) => Ok(to_number(&value)),
        }
    }

    /// The `order by` key: the single atomized item, or `None` for the empty
    /// sequence.
    ///
    /// More than one item is `XPTY0004`, reported as
    /// [`ComposeError::XPath`] — an order-by key must be at most one value
    /// (XQuery 1.0 §3.8.3).
    ///
    /// ```
    /// use bumpalo::Bump;
    /// use xsd_schema::compose::Composer;
    /// use xsd_schema::namespace::NameTable;
    ///
    /// let arena = Bump::new();
    /// let names = NameTable::new();
    /// let c = Composer::new(&arena, &names);
    /// let doc = c.load_str("<a><b>7</b></a>")?;
    ///
    /// let key = c.eval("b", &[], Some(doc.document_element().unwrap()), Vec::new())?.key()?;
    /// assert_eq!(key.unwrap().to_string_value(), "7");
    /// assert!(c.eval("()", &[], None, Vec::new())?.key()?.is_none());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn key(&self) -> Result<Option<XmlValue>, ComposeError> {
        let len = self.len();
        if len > 1 {
            return Err(ComposeError::XPath {
                source: XPathError::type_mismatch(
                    "xs:anyAtomicType?",
                    format!("a sequence of {len} items"),
                ),
                expr: String::new(),
            });
        }
        self.atomize_single()
    }

    /// How many items the sequence holds.
    ///
    /// ```
    /// use bumpalo::Bump;
    /// use xsd_schema::compose::Composer;
    /// use xsd_schema::namespace::NameTable;
    ///
    /// let arena = Bump::new();
    /// let names = NameTable::new();
    /// let c = Composer::new(&arena, &names);
    ///
    /// assert_eq!(c.eval("()", &[], None, Vec::new())?.len(), 0);
    /// assert_eq!(c.eval("1", &[], None, Vec::new())?.len(), 1);
    /// assert_eq!(c.eval("(1, 2, 3)", &[], None, Vec::new())?.len(), 3);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the sequence is empty.
    ///
    /// ```
    /// use bumpalo::Bump;
    /// use xsd_schema::compose::Composer;
    /// use xsd_schema::namespace::NameTable;
    ///
    /// let arena = Bump::new();
    /// let names = NameTable::new();
    /// let c = Composer::new(&arena, &names);
    ///
    /// assert!(c.eval("()", &[], None, Vec::new())?.is_empty());
    /// assert!(!c.eval("1", &[], None, Vec::new())?.is_empty());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Borrows the items, in order.
    ///
    /// A one-item sequence is one item here: the engine stores a singleton
    /// without a backing vector, and this iterator yields it all the same.
    ///
    /// ```
    /// use bumpalo::Bump;
    /// use xsd_schema::compose::Composer;
    /// use xsd_schema::namespace::NameTable;
    ///
    /// let arena = Bump::new();
    /// let names = NameTable::new();
    /// let c = Composer::new(&arena, &names);
    ///
    /// assert_eq!(c.eval("1", &[], None, Vec::new())?.iter().count(), 1);
    /// assert_eq!(c.eval("(1, 2, 3)", &[], None, Vec::new())?.iter().count(), 3);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn iter(&self) -> impl Iterator<Item = &XmlItem<Nav<'a>>> + '_ {
        self.0.as_slice().iter()
    }

    /// The wrapped engine value.
    ///
    /// ```
    /// use bumpalo::Bump;
    /// use xsd_schema::compose::Composer;
    /// use xsd_schema::namespace::NameTable;
    ///
    /// let arena = Bump::new();
    /// let names = NameTable::new();
    /// let c = Composer::new(&arena, &names);
    ///
    /// let sequence = c.eval("(1, 2)", &[], None, Vec::new())?.into_inner();
    /// assert_eq!(sequence.into_vec().len(), 2);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn into_inner(self) -> XPathValue<Nav<'a>> {
        self.0
    }

    /// Borrows the wrapped engine value.
    ///
    /// ```
    /// use bumpalo::Bump;
    /// use xsd_schema::compose::Composer;
    /// use xsd_schema::namespace::NameTable;
    ///
    /// let arena = Bump::new();
    /// let names = NameTable::new();
    /// let c = Composer::new(&arena, &names);
    ///
    /// let value = c.eval("(1, 2)", &[], None, Vec::new())?;
    /// assert_eq!(value.inner().len(), 2);
    /// // …and the `Value` is still usable afterwards.
    /// assert_eq!(value.len(), 2);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn inner(&self) -> &XPathValue<Nav<'a>> {
        &self.0
    }

    /// The one item, `None` when empty, [`ComposeError::NotSingleton`] beyond
    /// one.
    fn single(&self) -> Result<Option<&XmlItem<Nav<'a>>>, ComposeError> {
        match self.len() {
            0 => Ok(None),
            1 => Ok(self.0.first()),
            len => Err(ComposeError::NotSingleton { len }),
        }
    }

    /// [`single`](Self::single), atomized: `None` when the item atomizes to
    /// the empty sequence (a nilled element, an empty list), `XPTY0004` when it
    /// atomizes to more than one value (a list-typed node with several
    /// members).
    fn atomize_single(&self) -> Result<Option<XmlValue>, ComposeError> {
        match self.single()? {
            None => Ok(None),
            Some(XmlItem::Atomic(value)) => Ok(single_atom(value.clone())?),
            Some(XmlItem::Node(node)) => match atomize_node(node)? {
                None => Ok(None),
                Some(value) => Ok(single_atom(value)?),
            },
        }
    }
}

impl<'a> From<XPathValue<Nav<'a>>> for Value<'a> {
    fn from(value: XPathValue<Nav<'a>>) -> Self {
        Self(value)
    }
}

/// One line per item, for a `Debug` rendering.
///
/// The navigator type has no `Debug` of its own, and printing a whole subtree
/// would be worse than useless in an assertion message, so a node is named by
/// its kind and its qualified name and an atomic value by its string value.
pub(super) fn describe_items(value: &XPathValue<Nav<'_>>) -> Vec<String> {
    let items: Vec<&XmlItem<Nav<'_>>> = match value {
        XPathValue::Empty => Vec::new(),
        XPathValue::Item(item) => vec![item],
        XPathValue::Sequence(items) => items.iter().collect(),
    };
    items
        .into_iter()
        .map(|item| match item {
            XmlItem::Atomic(atomic) => format!("{:?}", atomic.to_string_value()),
            XmlItem::Node(node) => {
                use crate::navigator::DomNavigator;
                format!("{:?}({})", node.node_type(), node.name())
            }
        })
        .collect()
}

impl fmt::Debug for Value<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Value")
            .field(&describe_items(&self.0))
            .finish()
    }
}

// ── IntoXPathValue ────────────────────────────────────────────────────

/// A Rust value that can be bound to an external XPath variable.
///
/// Binding never parses or serializes anything: a navigator becomes a node
/// item, a document becomes its document node, a `Vec` becomes a sequence,
/// and a scalar becomes the atomic value XDM gives it.
///
/// ```
/// use bumpalo::Bump;
/// use xsd_schema::compose::{Composer, IntoXPathValue};
/// use xsd_schema::namespace::NameTable;
///
/// let arena = Bump::new();
/// let names = NameTable::new();
/// let c = Composer::new(&arena, &names);
/// let doc = c.load_str("<a><b>1</b><b>2</b></a>")?;
///
/// // A document binds as its document node …
/// let n = c.eval(
///     "count($d//b)",
///     &["d"],
///     None,
///     vec![("d", doc.into_xpath_value())],
/// )?;
/// assert_eq!(n.string()?, "2");
///
/// // … and a scalar as an atomic value.
/// let doubled = c.eval("$x * 2", &["x"], None, vec![("x", 21.into_xpath_value())])?;
/// assert_eq!(doubled.string()?, "42");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub trait IntoXPathValue<'a> {
    /// Converts `self` into a bindable sequence.
    fn into_xpath_value(self) -> XPathValue<Nav<'a>>;
}

impl<'a> IntoXPathValue<'a> for XPathValue<Nav<'a>> {
    fn into_xpath_value(self) -> XPathValue<Nav<'a>> {
        self
    }
}

impl<'a> IntoXPathValue<'a> for Value<'a> {
    fn into_xpath_value(self) -> XPathValue<Nav<'a>> {
        self.into_inner()
    }
}

impl<'a> IntoXPathValue<'a> for &Value<'a> {
    fn into_xpath_value(self) -> XPathValue<Nav<'a>> {
        self.inner().clone()
    }
}

impl<'a> IntoXPathValue<'a> for Nav<'a> {
    fn into_xpath_value(self) -> XPathValue<Nav<'a>> {
        XPathValue::from_node(self)
    }
}

impl<'a> IntoXPathValue<'a> for &Nav<'a> {
    fn into_xpath_value(self) -> XPathValue<Nav<'a>> {
        XPathValue::from_node(self.clone())
    }
}

impl<'a> IntoXPathValue<'a> for Doc<'a> {
    fn into_xpath_value(self) -> XPathValue<Nav<'a>> {
        XPathValue::from_node(self.root())
    }
}

impl<'a> IntoXPathValue<'a> for &Doc<'a> {
    fn into_xpath_value(self) -> XPathValue<Nav<'a>> {
        XPathValue::from_node(self.root())
    }
}

impl<'a> IntoXPathValue<'a> for XmlValue {
    fn into_xpath_value(self) -> XPathValue<Nav<'a>> {
        XPathValue::from_atomic(self)
    }
}

impl<'a> IntoXPathValue<'a> for &XmlValue {
    fn into_xpath_value(self) -> XPathValue<Nav<'a>> {
        XPathValue::from_atomic(self.clone())
    }
}

impl<'a> IntoXPathValue<'a> for bool {
    fn into_xpath_value(self) -> XPathValue<Nav<'a>> {
        XPathValue::boolean(self)
    }
}

impl<'a> IntoXPathValue<'a> for i32 {
    fn into_xpath_value(self) -> XPathValue<Nav<'a>> {
        XPathValue::integer(BigInt::from(self))
    }
}

impl<'a> IntoXPathValue<'a> for i64 {
    fn into_xpath_value(self) -> XPathValue<Nav<'a>> {
        XPathValue::integer(BigInt::from(self))
    }
}

impl<'a> IntoXPathValue<'a> for BigInt {
    fn into_xpath_value(self) -> XPathValue<Nav<'a>> {
        XPathValue::integer(self)
    }
}

impl<'a> IntoXPathValue<'a> for f64 {
    fn into_xpath_value(self) -> XPathValue<Nav<'a>> {
        XPathValue::double(self)
    }
}

impl<'a> IntoXPathValue<'a> for Decimal {
    fn into_xpath_value(self) -> XPathValue<Nav<'a>> {
        XPathValue::decimal(self)
    }
}

impl<'a> IntoXPathValue<'a> for &str {
    fn into_xpath_value(self) -> XPathValue<Nav<'a>> {
        XPathValue::string(self)
    }
}

impl<'a> IntoXPathValue<'a> for String {
    fn into_xpath_value(self) -> XPathValue<Nav<'a>> {
        XPathValue::string(self)
    }
}

impl<'a> IntoXPathValue<'a> for Vec<Nav<'a>> {
    fn into_xpath_value(self) -> XPathValue<Nav<'a>> {
        XPathValue::from_sequence(self.into_iter().map(XmlItem::Node).collect())
    }
}

impl<'a> IntoXPathValue<'a> for &[Nav<'a>] {
    fn into_xpath_value(self) -> XPathValue<Nav<'a>> {
        XPathValue::from_sequence(self.iter().cloned().map(XmlItem::Node).collect())
    }
}

impl<'a> IntoXPathValue<'a> for Vec<XmlItem<Nav<'a>>> {
    fn into_xpath_value(self) -> XPathValue<Nav<'a>> {
        XPathValue::from_sequence(self)
    }
}

impl<'a> IntoXPathValue<'a> for Vec<XmlValue> {
    fn into_xpath_value(self) -> XPathValue<Nav<'a>> {
        XPathValue::from_sequence(self.into_iter().map(XmlItem::Atomic).collect())
    }
}

impl<'a, T: IntoXPathValue<'a>> IntoXPathValue<'a> for Option<T> {
    fn into_xpath_value(self) -> XPathValue<Nav<'a>> {
        match self {
            Some(value) => value.into_xpath_value(),
            None => XPathValue::Empty,
        }
    }
}

impl<'a> IntoXPathValue<'a> for () {
    fn into_xpath_value(self) -> XPathValue<Nav<'a>> {
        XPathValue::Empty
    }
}

// ── IntoContextNode ───────────────────────────────────────────────────

/// A value usable as the context item of an evaluation.
///
/// The context item of a composed query is always a node in v1: a navigator,
/// or a document standing for its document node.
///
/// ```
/// use bumpalo::Bump;
/// use xsd_schema::compose::{Composer, IntoContextNode};
/// use xsd_schema::namespace::NameTable;
///
/// let arena = Bump::new();
/// let names = NameTable::new();
/// let c = Composer::new(&arena, &names);
/// let doc = c.load_str("<a><b>deep</b></a>")?;
///
/// // A document and a navigator reach the same node here.
/// let from_doc = c.eval("string(//b)", &[], Some(doc.into_context_node()), Vec::new())?;
/// let root = doc.root();
/// let from_nav = c.eval("string(//b)", &[], Some((&root).into_context_node()), Vec::new())?;
/// assert_eq!(from_doc.string()?, from_nav.string()?);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub trait IntoContextNode<'a> {
    /// Converts `self` into the navigator to focus on.
    fn into_context_node(self) -> Nav<'a>;
}

impl<'a> IntoContextNode<'a> for Nav<'a> {
    fn into_context_node(self) -> Nav<'a> {
        self
    }
}

impl<'a> IntoContextNode<'a> for &Nav<'a> {
    fn into_context_node(self) -> Nav<'a> {
        self.clone()
    }
}

impl<'a> IntoContextNode<'a> for Doc<'a> {
    fn into_context_node(self) -> Nav<'a> {
        self.root()
    }
}

impl<'a> IntoContextNode<'a> for &Doc<'a> {
    fn into_context_node(self) -> Nav<'a> {
        self.root()
    }
}

#[cfg(test)]
mod tests {
    use bumpalo::Bump;

    use super::*;
    use crate::compose::Composer;
    use crate::namespace::NameTable;
    use crate::navigator::DomNavigator;

    fn composer<'a>(arena: &'a Bump, names: &'a NameTable) -> Composer<'a> {
        Composer::new(arena, names)
    }

    #[test]
    fn iter_keeps_a_singleton() {
        let value: Value<'_> = XPathValue::string("only").into();
        assert_eq!(value.len(), 1);
        assert_eq!(value.iter().count(), 1);
        assert!(!value.is_empty());
    }

    #[test]
    fn iter_walks_empty_and_sequences() {
        let empty: Value<'_> = XPathValue::Empty.into();
        assert_eq!(empty.iter().count(), 0);
        assert!(empty.is_empty());

        let many: Value<'_> = XPathValue::from_sequence(vec![
            XmlItem::Atomic(XmlValue::string("a")),
            XmlItem::Atomic(XmlValue::string("b")),
        ])
        .into();
        assert_eq!(many.iter().count(), 2);
    }

    #[test]
    fn nodes_refuses_an_atomic_item() {
        let value: Value<'_> = XPathValue::string("x").into();
        assert!(matches!(value.nodes(), Err(ComposeError::NotANode)));
    }

    #[test]
    fn accessors_over_a_loaded_document() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = composer(&arena, &names);
        let doc = c.load_str("<a><b>1</b><b>2</b></a>").unwrap();

        let bs = c.eval("//b", &[], Some(doc.root()), Vec::new()).unwrap();
        assert_eq!(bs.nodes().unwrap().len(), 2);
        assert_eq!(bs.atomics().unwrap().len(), 2);
        assert!(bs.boolean().unwrap());
        assert!(matches!(
            bs.string(),
            Err(ComposeError::NotSingleton { len: 2 })
        ));
        assert!(matches!(
            bs.number(),
            Err(ComposeError::NotSingleton { len: 2 })
        ));
    }

    #[test]
    fn string_and_number_of_a_node() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = composer(&arena, &names);
        let doc = c.load_str("<a><b>42</b></a>").unwrap();

        let b = c.eval("//b", &[], Some(doc.root()), Vec::new()).unwrap();
        assert_eq!(b.string().unwrap(), "42");
        assert_eq!(b.number().unwrap(), 42.0);
        assert_eq!(b.key().unwrap().unwrap().to_string_value(), "42");
    }

    #[test]
    fn empty_string_number_and_key() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = composer(&arena, &names);
        let empty = c.eval("()", &[], None, Vec::new()).unwrap();
        assert_eq!(empty.string().unwrap(), "");
        assert!(empty.number().unwrap().is_nan());
        assert!(empty.key().unwrap().is_none());
    }

    #[test]
    fn key_refuses_more_than_one_item() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = composer(&arena, &names);
        let two = c.eval("(1, 2)", &[], None, Vec::new()).unwrap();
        match two.key() {
            Err(ComposeError::XPath { source, .. }) => {
                assert_eq!(source.error_code(), Some("XPTY0004"));
            }
            other => panic!("expected XPTY0004, got {other:?}"),
        }
    }

    /// A packed list value — the stored typed value of a list-typed node —
    /// atomizes to its members, one per member (XDM §3.3.1.2), and a singleton
    /// accessor takes at most one of them.
    #[test]
    fn a_packed_list_value_atomizes_to_its_members() {
        use crate::types::value::XmlValueKind;
        use crate::types::XmlTypeCode;

        let list = |members: &[i64]| -> Value<'_> {
            XPathValue::from_atomic(XmlValue::new(
                XmlTypeCode::Integer,
                XmlValueKind::List {
                    item_type: XmlTypeCode::Integer,
                    items: members
                        .iter()
                        .map(|&m| XmlValue::integer(BigInt::from(m)))
                        .collect(),
                },
            ))
            .into()
        };

        let two = list(&[4, 5]);
        let atomics = two.atomics().unwrap();
        assert_eq!(atomics.len(), 2);
        assert!(atomics.iter().all(|v| v.type_code == XmlTypeCode::Integer));
        assert_eq!(atomics[1].to_string_value(), "5");
        match two.key() {
            Err(ComposeError::XPath { source, .. }) => {
                assert_eq!(source.error_code(), Some("XPTY0004"));
            }
            other => panic!("expected XPTY0004, got {other:?}"),
        }

        assert_eq!(list(&[6]).number().unwrap(), 6.0);
        assert!(list(&[]).number().unwrap().is_nan());
        assert!(list(&[]).key().unwrap().is_none());
        assert!(list(&[]).atomics().unwrap().is_empty());
    }

    #[test]
    fn every_into_xpath_value_impl() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = composer(&arena, &names);
        let doc = c.load_str("<a><b>1</b><b>2</b></a>").unwrap();
        let root = doc.root();
        let bs = c.eval("//b", &[], Some(doc.root()), Vec::new()).unwrap();
        let node_list = bs.nodes().unwrap();

        // Each binding is checked through an expression that only holds for
        // the value that impl is supposed to produce.
        let cases: Vec<(&str, XPathValue<Nav<'_>>, &str)> = vec![
            ("$x", true.into_xpath_value(), "true"),
            ("$x", 7i32.into_xpath_value(), "7"),
            ("$x", 8i64.into_xpath_value(), "8"),
            ("$x", BigInt::from(9).into_xpath_value(), "9"),
            ("$x", 1.5f64.into_xpath_value(), "1.5"),
            ("$x", Decimal::new(25, 1).into_xpath_value(), "2.5"),
            ("$x", "text".into_xpath_value(), "text"),
            ("$x", String::from("owned").into_xpath_value(), "owned"),
            ("$x", XmlValue::string("atom").into_xpath_value(), "atom"),
            ("$x", (&XmlValue::string("ref")).into_xpath_value(), "ref"),
            ("name($x)", root.clone().into_xpath_value(), ""),
            ("name($x)", (&root).into_xpath_value(), ""),
            ("count($x//b)", doc.into_xpath_value(), "2"),
            ("count($x//b)", (&doc).into_xpath_value(), "2"),
            ("count($x)", bs.clone().into_xpath_value(), "2"),
            ("count($x)", (&bs).into_xpath_value(), "2"),
            (
                "count($x)",
                XPathValue::from_sequence(vec![XmlItem::Atomic(XmlValue::string("q"))])
                    .into_xpath_value(),
                "1",
            ),
            ("count($x)", node_list.clone().into_xpath_value(), "2"),
            ("count($x)", node_list.as_slice().into_xpath_value(), "2"),
            (
                "count($x)",
                vec![
                    XmlItem::Atomic(XmlValue::string("a")),
                    XmlItem::Atomic(XmlValue::string("b")),
                ]
                .into_xpath_value(),
                "2",
            ),
            (
                "count($x)",
                vec![XmlValue::string("a"), XmlValue::string("b")].into_xpath_value(),
                "2",
            ),
            ("$x", Some(3i32).into_xpath_value(), "3"),
            ("count($x)", Option::<i32>::None.into_xpath_value(), "0"),
            ("count($x)", ().into_xpath_value(), "0"),
        ];

        for (expr, bound, expected) in cases {
            let got = c
                .eval(expr, &["x"], None, vec![("x", bound)])
                .unwrap()
                .string()
                .unwrap();
            if !expected.is_empty() {
                assert_eq!(got, expected, "binding for {expr}");
            }
        }
    }

    #[test]
    fn into_context_node_impls_agree() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = composer(&arena, &names);
        let doc = c.load_str("<a><b>deep</b></a>").unwrap();
        let root = doc.root();

        for node in [
            doc.into_context_node(),
            (&doc).into_context_node(),
            root.clone().into_context_node(),
            (&root).into_context_node(),
        ] {
            assert!(node.is_same_position(&root));
        }
    }
}
