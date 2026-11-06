//! The description of a result element, before any document exists.
//!
//! A [`Form`] is an owned, eager tree: a name, the namespace declarations the
//! author asked for, attributes, and content. Nothing in it is a closure and
//! nothing in it is lazy — every Rust value that goes into a form has already
//! been evaluated — so a form can be returned from a function, stored in a
//! `Vec`, and handed to [`Composer::build`](crate::compose::Composer::build)
//! later.

use std::fmt;

use crate::namespace::is_ncname;
use crate::types::value::XmlValue;
use crate::xpath::{XPathValue, XmlItem};

use super::value::describe_items;
use super::{ComposeError, IntoXPathValue, Nav, Value};

/// An element or attribute name as the author wrote it.
///
/// A `Name` is a prefix and a local part, not a resolved name: the prefix is
/// looked up when the document is built, against the form's own declarations
/// first and the composer's namespace table second. An empty prefix on an
/// element takes the composer's default element namespace; an empty prefix on
/// an attribute means no namespace, as Namespaces in XML 1.0 §6.2 requires.
///
/// ```
/// use xsd_schema::compose::Name;
///
/// assert_eq!(Name::local("item").to_string(), "item");
/// assert_eq!(Name::prefixed("p", "item").to_string(), "p:item");
/// assert_eq!(Name::parse("p:item")?.prefix(), "p");
/// assert!(Name::parse("not a name").is_err());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Name {
    prefix: String,
    local: String,
}

impl Name {
    /// A name with no prefix.
    ///
    /// ```
    /// use xsd_schema::compose::Name;
    ///
    /// let n = Name::local("high_bid");
    /// assert_eq!(n.prefix(), "");
    /// assert_eq!(n.local_name(), "high_bid");
    /// ```
    pub fn local(local: &str) -> Self {
        Self {
            prefix: String::new(),
            local: local.to_string(),
        }
    }

    /// A name with a prefix to resolve at build time.
    ///
    /// ```
    /// use xsd_schema::compose::Name;
    ///
    /// let n = Name::prefixed("xml", "lang");
    /// assert_eq!(n.prefix(), "xml");
    /// assert_eq!(n.local_name(), "lang");
    /// ```
    pub fn prefixed(prefix: &str, local: &str) -> Self {
        Self {
            prefix: prefix.to_string(),
            local: local.to_string(),
        }
    }

    /// Parses `local` or `prefix:local`, checking both halves are `NCName`s.
    ///
    /// ```
    /// use xsd_schema::compose::{ComposeError, Name};
    ///
    /// assert_eq!(Name::parse("bid-count")?.local_name(), "bid-count");
    /// assert_eq!(Name::parse("p:bid")?.prefix(), "p");
    /// assert!(matches!(Name::parse("a:b:c"), Err(ComposeError::InvalidName(_))));
    /// assert!(matches!(Name::parse(""), Err(ComposeError::InvalidName(_))));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn parse(text: &str) -> Result<Self, ComposeError> {
        let invalid = || ComposeError::InvalidName(text.to_string());
        match text.split_once(':') {
            None => {
                if is_ncname(text) {
                    Ok(Self::local(text))
                } else {
                    Err(invalid())
                }
            }
            Some((prefix, local)) => {
                if is_ncname(prefix) && is_ncname(local) {
                    Ok(Self::prefixed(prefix, local))
                } else {
                    Err(invalid())
                }
            }
        }
    }

    /// The prefix, empty when there is none.
    ///
    /// ```
    /// use xsd_schema::compose::Name;
    ///
    /// assert_eq!(Name::prefixed("p", "a").prefix(), "p");
    /// assert_eq!(Name::local("a").prefix(), "");
    /// ```
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// The local part.
    ///
    /// ```
    /// use xsd_schema::compose::Name;
    ///
    /// assert_eq!(Name::prefixed("p", "a").local_name(), "a");
    /// ```
    pub fn local_name(&self) -> &str {
        &self.local
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.prefix.is_empty() {
            f.write_str(&self.local)
        } else {
            write!(f, "{}:{}", self.prefix, self.local)
        }
    }
}

/// What an attribute's value is made of.
///
/// ```
/// use xsd_schema::compose::AttrValue;
///
/// // A Rust value formatted with `Display` …
/// let literal = AttrValue::text(42);
/// assert!(matches!(literal, AttrValue::Text(ref s) if s == "42"));
///
/// // … or an XDM sequence, atomized and space-joined when the document is built.
/// let computed: AttrValue<'_> = AttrValue::value("x");
/// assert!(matches!(computed, AttrValue::Value(_)));
/// ```
#[derive(Clone)]
pub enum AttrValue<'a> {
    /// Literal text, written exactly as given.
    Text(String),
    /// A sequence: every item is atomized and their string values are joined
    /// with single spaces (XQuery 1.0 §3.7.1.1).
    Value(XPathValue<Nav<'a>>),
}

impl<'a> AttrValue<'a> {
    /// An attribute value from anything that formats itself.
    ///
    /// ```
    /// use xsd_schema::compose::AttrValue;
    ///
    /// assert!(matches!(AttrValue::text("b1"), AttrValue::Text(ref s) if s == "b1"));
    /// ```
    pub fn text(value: impl fmt::Display) -> Self {
        Self::Text(value.to_string())
    }

    /// An attribute value from a query result or any bindable Rust value.
    ///
    /// ```
    /// use bumpalo::Bump;
    /// use xsd_schema::compose::{AttrValue, Composer};
    /// use xsd_schema::namespace::NameTable;
    ///
    /// let arena = Bump::new();
    /// let names = NameTable::new();
    /// let c = Composer::new(&arena, &names);
    ///
    /// let ids = c.eval("(1, 2, 3)", &[], None, Vec::new())?;
    /// let attr = AttrValue::value(ids);
    /// assert!(matches!(attr, AttrValue::Value(_)));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn value(value: impl IntoXPathValue<'a>) -> Self {
        Self::Value(value.into_xpath_value())
    }
}

/// One item of an element's content.
///
/// ```
/// use xsd_schema::compose::{Content, Form, Name};
///
/// let mut form = Form::new(Name::local("p"));
/// form.push(Content::Text("hello".to_string()));
/// form.push(Content::Comment(" note ".to_string()));
/// assert_eq!(form.content().len(), 2);
/// ```
#[derive(Clone)]
pub enum Content<'a> {
    /// Literal text. Two adjacent texts concatenate with no separator.
    Text(String),
    /// A sequence, emitted with the constructor content rules: atomic values
    /// are space-separated from each other, nodes are copied.
    Value(XPathValue<Nav<'a>>),
    /// A child element.
    Element(Form<'a>),
    /// A comment.
    Comment(String),
    /// A processing instruction: target, then data.
    Pi(String, String),
}

impl fmt::Debug for AttrValue<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(text) => f.debug_tuple("Text").field(text).finish(),
            Self::Value(value) => f
                .debug_tuple("Value")
                .field(&describe_items(value))
                .finish(),
        }
    }
}

impl fmt::Debug for Content<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(text) => f.debug_tuple("Text").field(text).finish(),
            Self::Value(value) => f
                .debug_tuple("Value")
                .field(&describe_items(value))
                .finish(),
            Self::Element(form) => f.debug_tuple("Element").field(form).finish(),
            Self::Comment(text) => f.debug_tuple("Comment").field(text).finish(),
            Self::Pi(target, data) => f.debug_tuple("Pi").field(target).field(data).finish(),
        }
    }
}

/// A result element, described but not yet built.
///
/// ```
/// use bumpalo::Bump;
/// use xsd_schema::compose::{AttrValue, Composer, Content, Form, Name};
/// use xsd_schema::document::SerializeOptions;
/// use xsd_schema::namespace::NameTable;
///
/// let arena = Bump::new();
/// let names = NameTable::new();
/// let c = Composer::new(&arena, &names);
///
/// let mut title = Form::new(Name::local("title"));
/// title.push(Content::Text("One".to_string()));
///
/// let mut book = Form::new(Name::local("book"));
/// book.attr(Name::local("id"), AttrValue::text("b1"));
/// book.push(Content::Element(title));
///
/// let doc = c.build(book)?;
/// assert_eq!(
///     doc.to_xml(&SerializeOptions::default())?,
///     r#"<book id="b1"><title>One</title></book>"#,
/// );
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone)]
pub struct Form<'a> {
    name: Name,
    decls: Vec<(String, String)>,
    attrs: Vec<(Name, AttrValue<'a>)>,
    content: Vec<Content<'a>>,
}

impl<'a> Form<'a> {
    /// An element with no declarations, attributes or content.
    ///
    /// ```
    /// use xsd_schema::compose::{Form, Name};
    ///
    /// let form = Form::new(Name::local("result"));
    /// assert_eq!(form.name().local_name(), "result");
    /// assert!(form.content().is_empty());
    /// ```
    pub fn new(name: Name) -> Self {
        Self {
            name,
            decls: Vec::new(),
            attrs: Vec::new(),
            content: Vec::new(),
        }
    }

    /// Declares a namespace on this element, in scope for it and its
    /// descendants.
    ///
    /// An empty prefix declares the default namespace; an empty prefix with an
    /// empty URI undeclares it. The declaration shadows the composer's
    /// namespace table for this subtree, and is dropped from the output only
    /// when it is redundant.
    ///
    /// ```
    /// use bumpalo::Bump;
    /// use xsd_schema::compose::{Composer, Form, Name};
    /// use xsd_schema::document::SerializeOptions;
    /// use xsd_schema::namespace::NameTable;
    ///
    /// let arena = Bump::new();
    /// let names = NameTable::new();
    /// let c = Composer::new(&arena, &names);
    ///
    /// let mut form = Form::new(Name::prefixed("p", "root"));
    /// form.declare("p", "urn:x");
    ///
    /// assert_eq!(
    ///     c.build(form)?.to_xml(&SerializeOptions::default())?,
    ///     r#"<p:root xmlns:p="urn:x"/>"#,
    /// );
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn declare(&mut self, prefix: &str, uri: &str) -> &mut Self {
        self.decls.push((prefix.to_string(), uri.to_string()));
        self
    }

    /// Adds an attribute.
    ///
    /// Attributes are written in the order they are added. Two attributes with
    /// the same expanded name are a duplicate, and the later one wins.
    ///
    /// ```
    /// use xsd_schema::compose::{AttrValue, Form, Name};
    ///
    /// let mut form = Form::new(Name::local("book"));
    /// form.attr(Name::local("id"), AttrValue::text("b1"))
    ///     .attr(Name::local("year"), AttrValue::text(1999));
    /// assert_eq!(form.attributes().len(), 2);
    /// ```
    pub fn attr(&mut self, name: Name, value: AttrValue<'a>) -> &mut Self {
        self.attrs.push((name, value));
        self
    }

    /// Appends one content item.
    ///
    /// ```
    /// use xsd_schema::compose::{Content, Form, IntoContent, Name};
    ///
    /// let mut form = Form::new(Name::local("p"));
    /// form.push(Content::Text("a".to_string()))
    ///     .push("b".into_content());
    /// assert_eq!(form.content().len(), 2);
    /// ```
    pub fn push(&mut self, content: Content<'a>) -> &mut Self {
        self.content.push(content);
        self
    }

    /// The element's name.
    ///
    /// ```
    /// use xsd_schema::compose::{Form, Name};
    ///
    /// let form = Form::new(Name::prefixed("p", "root"));
    /// assert_eq!(form.name().to_string(), "p:root");
    /// ```
    pub fn name(&self) -> &Name {
        &self.name
    }

    /// The namespace declarations the author asked for, in order.
    ///
    /// ```
    /// use xsd_schema::compose::{Form, Name};
    ///
    /// let mut form = Form::new(Name::local("root"));
    /// form.declare("p", "urn:x");
    /// assert_eq!(form.declarations(), [("p".to_string(), "urn:x".to_string())]);
    /// ```
    pub fn declarations(&self) -> &[(String, String)] {
        &self.decls
    }

    /// The attributes, in order.
    ///
    /// ```
    /// use xsd_schema::compose::{AttrValue, Form, Name};
    ///
    /// let mut form = Form::new(Name::local("book"));
    /// form.attr(Name::local("id"), AttrValue::text("b1"));
    /// assert_eq!(form.attributes()[0].0.local_name(), "id");
    /// ```
    pub fn attributes(&self) -> &[(Name, AttrValue<'a>)] {
        &self.attrs
    }

    /// The content items, in order.
    ///
    /// ```
    /// use xsd_schema::compose::{Content, Form, Name};
    ///
    /// let mut form = Form::new(Name::local("p"));
    /// form.push(Content::Text("hi".to_string()));
    /// assert!(matches!(form.content()[0], Content::Text(ref t) if t == "hi"));
    /// ```
    pub fn content(&self) -> &[Content<'a>] {
        &self.content
    }

    /// Takes the form apart for emission.
    #[allow(clippy::type_complexity)]
    pub(super) fn into_parts(
        self,
    ) -> (
        Name,
        Vec<(String, String)>,
        Vec<(Name, AttrValue<'a>)>,
        Vec<Content<'a>>,
    ) {
        (self.name, self.decls, self.attrs, self.content)
    }
}

// ── IntoContent ───────────────────────────────────────────────────────

/// A Rust value that can become element content.
///
/// The conversion never serializes an XDM value into text: a navigator stays a
/// node and is copied when the document is built, an atomic value stays atomic
/// and takes part in the space-separation rule, and only `Display` values
/// become literal text.
///
/// There is deliberately no implementation for `Result`: turning a failure
/// into missing content is exactly the mistake this layer is built to prevent.
/// An `Option` *is* implemented, because "nothing" is a legitimate answer —
/// `None` contributes the empty sequence, which adds no content and does not
/// break the adjacency of what surrounds it.
///
/// ```
/// use bumpalo::Bump;
/// use xsd_schema::compose::{Composer, Content, Form, IntoContent, Name};
/// use xsd_schema::document::SerializeOptions;
/// use xsd_schema::namespace::NameTable;
///
/// let arena = Bump::new();
/// let names = NameTable::new();
/// let c = Composer::new(&arena, &names);
///
/// let mut form = Form::new(Name::local("p"));
/// form.push("plain".into_content())
///     .push(7.into_content())
///     .push(Option::<&str>::None.into_content());
///
/// assert_eq!(
///     c.build(form)?.to_xml(&SerializeOptions::default())?,
///     "<p>plain7</p>",
/// );
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub trait IntoContent<'a> {
    /// Converts `self` into one content item.
    fn into_content(self) -> Content<'a>;
}

impl<'a> IntoContent<'a> for Content<'a> {
    fn into_content(self) -> Content<'a> {
        self
    }
}

impl<'a> IntoContent<'a> for Form<'a> {
    fn into_content(self) -> Content<'a> {
        Content::Element(self)
    }
}

impl<'a> IntoContent<'a> for Value<'a> {
    fn into_content(self) -> Content<'a> {
        Content::Value(self.into_inner())
    }
}

impl<'a> IntoContent<'a> for &Value<'a> {
    fn into_content(self) -> Content<'a> {
        Content::Value(self.inner().clone())
    }
}

impl<'a> IntoContent<'a> for XPathValue<Nav<'a>> {
    fn into_content(self) -> Content<'a> {
        Content::Value(self)
    }
}

impl<'a> IntoContent<'a> for XmlItem<Nav<'a>> {
    fn into_content(self) -> Content<'a> {
        Content::Value(XPathValue::from_item(self))
    }
}

impl<'a> IntoContent<'a> for Nav<'a> {
    fn into_content(self) -> Content<'a> {
        Content::Value(XPathValue::from_node(self))
    }
}

impl<'a> IntoContent<'a> for &Nav<'a> {
    fn into_content(self) -> Content<'a> {
        Content::Value(XPathValue::from_node(self.clone()))
    }
}

impl<'a> IntoContent<'a> for XmlValue {
    fn into_content(self) -> Content<'a> {
        Content::Value(XPathValue::from_atomic(self))
    }
}

impl<'a> IntoContent<'a> for &XmlValue {
    fn into_content(self) -> Content<'a> {
        Content::Value(XPathValue::from_atomic(self.clone()))
    }
}

impl<'a> IntoContent<'a> for String {
    fn into_content(self) -> Content<'a> {
        Content::Text(self)
    }
}

impl<'a> IntoContent<'a> for &str {
    fn into_content(self) -> Content<'a> {
        Content::Text(self.to_string())
    }
}

impl<'a, T: IntoContent<'a>> IntoContent<'a> for Option<T> {
    fn into_content(self) -> Content<'a> {
        match self {
            Some(value) => value.into_content(),
            // The empty sequence: no content, and no effect on the
            // surrounding atomic-value adjacency.
            None => Content::Value(XPathValue::Empty),
        }
    }
}

/// `Display` scalars become literal text.
///
/// Rust's own formatting is used, not the XDM canonical lexical form: `1.0f64`
/// is `1` in XDM and `1` here only by coincidence. Bind the value into an
/// expression, or build an [`XmlValue`], when the canonical form matters.
macro_rules! into_content_via_display {
    ($($t:ty),* $(,)?) => {
        $(
            impl<'a> IntoContent<'a> for $t {
                fn into_content(self) -> Content<'a> {
                    Content::Text(self.to_string())
                }
            }
        )*
    };
}

into_content_via_display!(
    bool, char, i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64,
);

// A `Vec` of convertible values flattens into one content sequence, so a
// sequence built up in Rust splices exactly like one an expression returned.

impl<'a> IntoContent<'a> for Vec<Value<'a>> {
    fn into_content(self) -> Content<'a> {
        let mut items: Vec<XmlItem<Nav<'a>>> = Vec::with_capacity(self.len());
        for value in self {
            items.extend(value.into_inner().into_vec());
        }
        Content::Value(XPathValue::from_sequence(items))
    }
}

impl<'a> IntoContent<'a> for Vec<XPathValue<Nav<'a>>> {
    fn into_content(self) -> Content<'a> {
        let mut items: Vec<XmlItem<Nav<'a>>> = Vec::with_capacity(self.len());
        for value in self {
            items.extend(value.into_vec());
        }
        Content::Value(XPathValue::from_sequence(items))
    }
}

impl<'a> IntoContent<'a> for Vec<XmlItem<Nav<'a>>> {
    fn into_content(self) -> Content<'a> {
        Content::Value(XPathValue::from_sequence(self))
    }
}

impl<'a> IntoContent<'a> for Vec<Nav<'a>> {
    fn into_content(self) -> Content<'a> {
        Content::Value(XPathValue::from_sequence(
            self.into_iter().map(XmlItem::Node).collect(),
        ))
    }
}

impl<'a> IntoContent<'a> for Vec<&Nav<'a>> {
    fn into_content(self) -> Content<'a> {
        Content::Value(XPathValue::from_sequence(
            self.into_iter().map(|n| XmlItem::Node(n.clone())).collect(),
        ))
    }
}

impl<'a> IntoContent<'a> for Vec<XmlValue> {
    fn into_content(self) -> Content<'a> {
        Content::Value(XPathValue::from_sequence(
            self.into_iter().map(XmlItem::Atomic).collect(),
        ))
    }
}

impl<'a> IntoContent<'a> for Vec<&XmlValue> {
    fn into_content(self) -> Content<'a> {
        Content::Value(XPathValue::from_sequence(
            self.into_iter()
                .map(|v| XmlItem::Atomic(v.clone()))
                .collect(),
        ))
    }
}

// ── Macro support ─────────────────────────────────────────────────────
//
// The two functions below exist so the macros stay `macro_rules!` and expand
// to ordinary calls. They are not part of the documented surface.

/// The name a string literal in `form!` stands for.
///
/// The literal is split at its first colon, and the halves are *not* checked
/// here: the emitter checks every name it writes, so a literal that is not an
/// `NCName` or `prefix:NCName` is [`ComposeError::InvalidName`] at build time,
/// where every other name rule is decided too.
#[doc(hidden)]
pub fn __name_from_literal(text: &str) -> Name {
    match text.split_once(':') {
        Some((prefix, local)) => Name::prefixed(prefix, local),
        None => Name::local(text),
    }
}

/// The content `form!`'s `@{…}` makes: a Rust value formatted with `Display`.
#[doc(hidden)]
pub fn __display_text<'a>(value: impl fmt::Display) -> Content<'a> {
    Content::Text(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_literal_name_splits_at_its_first_colon() {
        assert_eq!(__name_from_literal("bid-count"), Name::local("bid-count"));
        assert_eq!(__name_from_literal("p:bid"), Name::prefixed("p", "bid"));
        // Unchecked here; the emitter refuses it.
        assert_eq!(__name_from_literal("a:b:c"), Name::prefixed("a", "b:c"));
    }

    #[test]
    fn a_display_value_becomes_text() {
        let content: Content<'_> = __display_text(1.5f64);
        assert!(matches!(content, Content::Text(ref t) if t == "1.5"));
    }

    #[test]
    fn parse_accepts_local_and_prefixed_names() {
        assert_eq!(Name::parse("item").unwrap(), Name::local("item"));
        assert_eq!(Name::parse("p:item").unwrap(), Name::prefixed("p", "item"));
        assert_eq!(Name::parse("bid-count").unwrap().local_name(), "bid-count");
    }

    #[test]
    fn parse_refuses_non_names() {
        for bad in ["", ":x", "x:", "a:b:c", "1a", "a b", "a/b"] {
            assert!(
                matches!(Name::parse(bad), Err(ComposeError::InvalidName(_))),
                "{bad:?} should not parse"
            );
        }
    }

    #[test]
    fn display_spells_the_qualified_name() {
        assert_eq!(Name::local("a").to_string(), "a");
        assert_eq!(Name::prefixed("p", "a").to_string(), "p:a");
    }

    #[test]
    fn builders_are_chainable_and_ordered() {
        let mut form = Form::new(Name::local("e"));
        form.declare("p", "urn:x")
            .attr(Name::local("k"), AttrValue::text("v"))
            .push(Content::Text("t".to_string()));

        assert_eq!(
            form.declarations(),
            [("p".to_string(), "urn:x".to_string())]
        );
        assert_eq!(form.attributes().len(), 1);
        assert_eq!(form.content().len(), 1);
    }

    #[test]
    fn display_impls_become_text() {
        for content in [
            true.into_content(),
            'c'.into_content(),
            1i8.into_content(),
            2i16.into_content(),
            3i32.into_content(),
            4i64.into_content(),
            5i128.into_content(),
            6isize.into_content(),
            7u8.into_content(),
            8u16.into_content(),
            9u32.into_content(),
            10u64.into_content(),
            11u128.into_content(),
            12usize.into_content(),
            1.5f32.into_content(),
            2.5f64.into_content(),
            "s".into_content(),
            String::from("o").into_content(),
        ] {
            assert!(matches!(content, Content::Text(_)));
        }
    }

    #[test]
    fn xdm_impls_stay_sequences() {
        let value = XmlValue::string("a");
        for content in [
            XPathValue::<Nav<'_>>::string("x").into_content(),
            XmlItem::<Nav<'_>>::Atomic(XmlValue::string("y")).into_content(),
            value.clone().into_content(),
            (&value).into_content(),
            vec![XmlValue::string("a"), XmlValue::string("b")].into_content(),
            vec![XmlItem::<Nav<'_>>::Atomic(XmlValue::string("c"))].into_content(),
            vec![XPathValue::<Nav<'_>>::string("d")].into_content(),
            vec![&value].into_content(),
            Some(XmlValue::string("e")).into_content(),
            Option::<XmlValue>::None.into_content(),
        ] {
            assert!(matches!(content, Content::Value(_)));
        }
    }

    #[test]
    fn a_vec_of_values_flattens_into_one_sequence() {
        let a: Value<'_> = XPathValue::from_sequence(vec![
            XmlItem::Atomic(XmlValue::string("1")),
            XmlItem::Atomic(XmlValue::string("2")),
        ])
        .into();
        let b: Value<'_> = XPathValue::string("3").into();

        match vec![a, b].into_content() {
            Content::Value(v) => assert_eq!(v.len(), 3),
            other => panic!("expected a sequence, got {other:?}"),
        }
    }

    #[test]
    fn none_contributes_the_empty_sequence() {
        match Option::<&str>::None.into_content() {
            Content::Value(v) => assert!(v.is_empty()),
            other => panic!("expected the empty sequence, got {other:?}"),
        }
    }

    #[test]
    fn a_form_is_content() {
        let form = Form::new(Name::local("child"));
        assert!(matches!(form.into_content(), Content::Element(_)));
    }
}
