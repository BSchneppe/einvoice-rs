//! Turning forms into a document.
//!
//! The [`Emitter`] walks a [`Form`] tree once and pushes it into a
//! [`BufferDocumentBuilder`]. Everything a form left open is decided here:
//! prefixes are resolved, namespace declarations are computed, attribute
//! sequences are atomized and joined, and spliced nodes are copied with the
//! constructor content rules.
//!
//! # Name resolution
//!
//! A prefix is looked up in the declarations the enclosing forms made,
//! innermost first, then in the emitter's own namespace table (the composer's
//! [`with_namespace`](crate::compose::Composer::with_namespace) bindings).
//! `xml` is always bound. An unprefixed element name takes the default element
//! namespace — a form-declared `xmlns` if there is one, otherwise the
//! composer's
//! [`with_default_element_namespace`](crate::compose::Composer::with_default_element_namespace).
//! An unprefixed attribute name is always in no namespace (Namespaces in XML
//! 1.0 §6.2). A prefix nothing binds is
//! [`ComposeError::UnboundPrefix`], reporting the form's path.

use std::collections::HashMap;

use crate::document::{BufferDocument, BufferDocumentBuilder, CopyOptions, NamespaceFixup};
use crate::namespace::{is_ncname, XML_NAMESPACE};
use crate::types::value::XmlValue;
use crate::xpath::atomize::atomize_node;
use crate::xpath::XmlItem;

use super::form::{AttrValue, Content, Form, Name};
use super::{ComposeError, Nav};

/// The `xml` prefix, always bound and never declared.
const XML_PREFIX: &str = "xml";

/// Pushes [`Form`]s into a document builder.
///
/// A host that owns its own builder can drive the emitter directly;
/// [`Composer::build`](crate::compose::Composer::build) and
/// [`Composer::build_sequence`](crate::compose::Composer::build_sequence) are
/// the usual way in.
///
/// ```
/// use bumpalo::Bump;
/// use xsd_schema::compose::{Content, Emitter, Form, Name};
/// use xsd_schema::document::{serialize, BufferDocumentBuilder};
/// use xsd_schema::document::{BufferDocumentOptions, CopyOptions};
/// use xsd_schema::namespace::NameTable;
/// use xsd_schema::navigator::DomNavigator;
///
/// let arena = Bump::new();
/// let names = NameTable::new();
/// let builder =
///     BufferDocumentBuilder::new(&arena, &names, None, BufferDocumentOptions::default())?;
///
/// let mut emitter = Emitter::new(builder, CopyOptions::default());
/// let mut greeting = Form::new(Name::local("greeting"));
/// greeting.push(Content::Text("hello".to_string()));
/// emitter.element(greeting)?;
/// let doc = emitter.finish()?;
///
/// assert_eq!(
///     serialize::to_string(&doc.create_navigator(), &Default::default())?,
///     "<greeting>hello</greeting>",
/// );
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct Emitter<'a> {
    builder: BufferDocumentBuilder<'a>,
    /// The declarations actually written, mirroring the open elements.
    fixup: NamespaceFixup,
    /// How spliced nodes are copied.
    copy: CopyOptions,
    /// The composer's namespace table, outermost first.
    namespaces: Vec<(String, String)>,
    /// The composer's default element namespace.
    default_element_ns: String,
    /// The declarations each open form asked for, outermost first.
    declared: Vec<Vec<(String, String)>>,
    /// The open forms' names, for diagnostics.
    path: Vec<String>,
    /// How many same-named element children each open container has seen.
    counts: Vec<HashMap<String, usize>>,
}

impl<'a> Emitter<'a> {
    /// An emitter over `builder`, with no namespaces bound.
    ///
    /// ```
    /// use bumpalo::Bump;
    /// use xsd_schema::compose::Emitter;
    /// use xsd_schema::document::{serialize, BufferDocumentBuilder};
    /// use xsd_schema::document::{BufferDocumentOptions, CopyOptions};
    /// use xsd_schema::namespace::NameTable;
    ///
    /// let arena = Bump::new();
    /// let names = NameTable::new();
    /// let builder =
    ///     BufferDocumentBuilder::new(&arena, &names, None, BufferDocumentOptions::default())?;
    ///
    /// // An emitter that has written nothing finishes as an empty document.
    /// let doc = Emitter::new(builder, CopyOptions::default()).finish()?;
    /// assert_eq!(
    ///     serialize::to_string(&doc.create_navigator(), &Default::default())?,
    ///     "",
    /// );
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn new(builder: BufferDocumentBuilder<'a>, copy: CopyOptions) -> Self {
        Self {
            builder,
            fixup: NamespaceFixup::new(),
            copy,
            namespaces: Vec::new(),
            default_element_ns: String::new(),
            declared: Vec::new(),
            path: Vec::new(),
            counts: vec![HashMap::new()],
        }
    }

    /// Binds a prefix for the forms this emitter writes.
    ///
    /// A later binding of the same prefix wins, and a form's own declaration
    /// wins over both.
    ///
    /// ```
    /// use bumpalo::Bump;
    /// use xsd_schema::compose::{Emitter, Form, Name};
    /// use xsd_schema::document::{serialize, BufferDocumentBuilder};
    /// use xsd_schema::document::{BufferDocumentOptions, CopyOptions};
    /// use xsd_schema::namespace::NameTable;
    /// use xsd_schema::navigator::DomNavigator;
    ///
    /// let arena = Bump::new();
    /// let names = NameTable::new();
    /// let builder =
    ///     BufferDocumentBuilder::new(&arena, &names, None, BufferDocumentOptions::default())?;
    ///
    /// let mut emitter =
    ///     Emitter::new(builder, CopyOptions::default()).with_namespace("p", "urn:x");
    /// emitter.element(Form::new(Name::prefixed("p", "root")))?;
    /// let doc = emitter.finish()?;
    ///
    /// assert_eq!(
    ///     serialize::to_string(&doc.create_navigator(), &Default::default())?,
    ///     r#"<p:root xmlns:p="urn:x"/>"#,
    /// );
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn with_namespace(mut self, prefix: &str, uri: &str) -> Self {
        self.namespaces.push((prefix.to_string(), uri.to_string()));
        self
    }

    /// Sets the namespace an unprefixed element name takes.
    ///
    /// ```
    /// use bumpalo::Bump;
    /// use xsd_schema::compose::{Emitter, Form, Name};
    /// use xsd_schema::document::{serialize, BufferDocumentBuilder};
    /// use xsd_schema::document::{BufferDocumentOptions, CopyOptions};
    /// use xsd_schema::namespace::NameTable;
    /// use xsd_schema::navigator::DomNavigator;
    ///
    /// let arena = Bump::new();
    /// let names = NameTable::new();
    /// let builder =
    ///     BufferDocumentBuilder::new(&arena, &names, None, BufferDocumentOptions::default())?;
    ///
    /// let mut emitter = Emitter::new(builder, CopyOptions::default())
    ///     .with_default_element_namespace("urn:d");
    /// emitter.element(Form::new(Name::local("root")))?;
    /// let doc = emitter.finish()?;
    ///
    /// assert_eq!(
    ///     serialize::to_string(&doc.create_navigator(), &Default::default())?,
    ///     r#"<root xmlns="urn:d"/>"#,
    /// );
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn with_default_element_namespace(mut self, uri: &str) -> Self {
        self.default_element_ns = uri.to_string();
        self
    }

    /// Writes one form and everything under it.
    ///
    /// With no element open this appends a top-level node, so several calls
    /// produce a sequence of top-level elements.
    ///
    /// An `Err` abandons the document: the element it was writing stays open
    /// and the partial tree is not worth finishing, so drop the emitter rather
    /// than calling it again.
    ///
    /// ```
    /// use bumpalo::Bump;
    /// use xsd_schema::compose::{Emitter, Form, Name};
    /// use xsd_schema::document::{serialize, BufferDocumentBuilder};
    /// use xsd_schema::document::{BufferDocumentOptions, CopyOptions};
    /// use xsd_schema::namespace::NameTable;
    ///
    /// let arena = Bump::new();
    /// let names = NameTable::new();
    /// let builder =
    ///     BufferDocumentBuilder::new(&arena, &names, None, BufferDocumentOptions::default())?;
    ///
    /// let mut emitter = Emitter::new(builder, CopyOptions::default());
    /// emitter.element(Form::new(Name::local("one")))?;
    /// emitter.element(Form::new(Name::local("two")))?;
    /// let doc = emitter.finish()?;
    ///
    /// assert_eq!(
    ///     serialize::to_string(&doc.create_navigator(), &Default::default())?,
    ///     "<one/><two/>",
    /// );
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn element(&mut self, form: Form<'a>) -> Result<(), ComposeError> {
        let (name, declared, attrs, content) = form.into_parts();

        let segment = self.path_segment(&name);
        self.path.push(segment);
        // The form's declarations are in scope for its own name too.
        self.declared.push(declared);

        let outcome = self.write_element(&name, attrs, content);

        self.path.pop();
        self.declared.pop();
        outcome
    }

    /// Finalizes the document.
    ///
    /// ```
    /// use bumpalo::Bump;
    /// use xsd_schema::compose::{Emitter, Form, Name};
    /// use xsd_schema::document::{serialize, BufferDocumentBuilder};
    /// use xsd_schema::document::{BufferDocumentOptions, CopyOptions};
    /// use xsd_schema::namespace::NameTable;
    ///
    /// let arena = Bump::new();
    /// let names = NameTable::new();
    /// let builder =
    ///     BufferDocumentBuilder::new(&arena, &names, None, BufferDocumentOptions::default())?;
    ///
    /// let mut emitter = Emitter::new(builder, CopyOptions::default());
    /// emitter.element(Form::new(Name::local("done")))?;
    ///
    /// let doc = emitter.finish()?;
    /// assert_eq!(
    ///     serialize::to_string(&doc.create_navigator(), &Default::default())?,
    ///     "<done/>",
    /// );
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn finish(self) -> Result<BufferDocument<'a>, ComposeError> {
        Ok(self.builder.finalize()?)
    }

    // ── Internals ─────────────────────────────────────────────────────

    /// The body of [`element`](Self::element), with the diagnostic stacks
    /// already set up so an early return still unwinds them.
    fn write_element(
        &mut self,
        name: &Name,
        attrs: Vec<(Name, AttrValue<'a>)>,
        content: Vec<Content<'a>>,
    ) -> Result<(), ComposeError> {
        let at = self.path.join("/");
        check_name(name)?;
        let elem_uri = self.element_namespace(name.prefix(), &at)?;

        // Attribute names and values, resolved before the element opens: the
        // fixup needs every name at once to generate prefixes deterministically.
        let mut resolved: Vec<(Name, String, String)> = Vec::with_capacity(attrs.len());
        for (attr_name, value) in attrs {
            check_name(&attr_name)?;
            let uri = self.attribute_namespace(attr_name.prefix(), &at)?;
            let text = self.attribute_text(value, &at)?;
            resolved.push((attr_name, uri, text));
        }

        let attr_names: Vec<(&str, &str)> = resolved
            .iter()
            .map(|(attr_name, uri, _)| (attr_name.prefix(), uri.as_str()))
            .collect();
        let keep: Vec<(&str, &str)> = self
            .declared
            .last()
            .expect("a declaration frame is open")
            .iter()
            .map(|(prefix, uri)| (prefix.as_str(), uri.as_str()))
            .collect();

        // `declarations_for` pushes the scope it computes, so the matching
        // `pop_scope` below belongs with `end_element`.
        let (written, stored) =
            self.fixup
                .declarations_for((name.prefix(), &elem_uri), &attr_names, &keep);
        let decl_refs: Vec<(&str, &str)> = written
            .iter()
            .map(|(prefix, uri)| (prefix.as_str(), uri.as_str()))
            .collect();

        self.builder
            .start_element(name.local_name(), &elem_uri, name.prefix(), &decl_refs)?;
        for ((attr_name, _, text), (prefix, uri)) in resolved.iter().zip(stored.iter()) {
            self.builder
                .attribute(attr_name.local_name(), uri, prefix, text)?;
        }
        self.builder.end_of_attributes();

        self.counts.push(HashMap::new());
        let outcome = self.write_content(content);
        self.counts.pop();

        outcome?;
        self.builder.end_element()?;
        self.fixup.pop_scope();
        Ok(())
    }

    /// Writes an element's content, in order.
    fn write_content(&mut self, content: Vec<Content<'a>>) -> Result<(), ComposeError> {
        for item in content {
            match item {
                Content::Text(text) => self.builder.text(&text),
                Content::Value(value) => {
                    // The items carry the element's own adjacency state, so
                    // two consecutive sequences of atomic values still meet
                    // with a single space and an empty one changes nothing.
                    let items = value.into_vec();
                    let at = self.path.join("/");
                    self.builder
                        .append_content(&items, self.copy)
                        .map_err(|source| ComposeError::Copy { source, at })?;
                }
                Content::Element(form) => self.element(form)?,
                Content::Comment(text) => self.builder.comment(&text)?,
                Content::Pi(target, data) => self.builder.processing_instruction(&target, &data)?,
            }
        }
        Ok(())
    }

    /// An attribute value's text: literal, or the sequence atomized and joined
    /// with single spaces (XQuery 1.0 §3.7.1.1).
    fn attribute_text(&self, value: AttrValue<'a>, at: &str) -> Result<String, ComposeError> {
        match value {
            AttrValue::Text(text) => Ok(text),
            AttrValue::Value(sequence) => {
                let mut parts: Vec<String> = Vec::with_capacity(sequence.len());
                for item in sequence.into_vec() {
                    match item {
                        XmlItem::Atomic(atomic) => parts.push(atomic.to_string_value()),
                        XmlItem::Node(node) => {
                            if let Some(atomic) = self.atomize(&node, at)? {
                                parts.push(atomic.to_string_value());
                            }
                        }
                    }
                }
                Ok(parts.join(" "))
            }
        }
    }

    /// Atomizes a node for an attribute value, naming the form on failure.
    fn atomize(&self, node: &Nav<'a>, at: &str) -> Result<Option<XmlValue>, ComposeError> {
        atomize_node(node).map_err(|source| ComposeError::XPath {
            source,
            expr: format!("the attribute content of {at}"),
        })
    }

    /// The namespace an element name is in.
    fn element_namespace(&self, prefix: &str, at: &str) -> Result<String, ComposeError> {
        if prefix.is_empty() {
            // A form-declared default namespace shadows the composer's.
            for frame in self.declared.iter().rev() {
                if let Some((_, uri)) = frame.iter().rev().find(|(p, _)| p.is_empty()) {
                    return Ok(uri.clone());
                }
            }
            return Ok(self.default_element_ns.clone());
        }
        self.resolve(prefix)
            .ok_or_else(|| ComposeError::UnboundPrefix {
                prefix: prefix.to_string(),
                at: at.to_string(),
            })
    }

    /// The namespace an attribute name is in: none unless it is prefixed.
    fn attribute_namespace(&self, prefix: &str, at: &str) -> Result<String, ComposeError> {
        if prefix.is_empty() {
            return Ok(String::new());
        }
        self.resolve(prefix)
            .ok_or_else(|| ComposeError::UnboundPrefix {
                prefix: prefix.to_string(),
                at: at.to_string(),
            })
    }

    /// Resolves a non-empty prefix: form declarations innermost first, then the
    /// emitter's table.
    fn resolve(&self, prefix: &str) -> Option<String> {
        if prefix == XML_PREFIX {
            return Some(XML_NAMESPACE.to_string());
        }
        for frame in self.declared.iter().rev() {
            if let Some((_, uri)) = frame.iter().rev().find(|(p, _)| p == prefix) {
                // An XML 1.1 undeclaration cannot be written, so a prefix bound
                // to nothing reads as unusable rather than as "no namespace".
                return if uri.is_empty() {
                    None
                } else {
                    Some(uri.clone())
                };
            }
        }
        self.namespaces
            .iter()
            .rev()
            .find(|(p, _)| p == prefix)
            .and_then(|(_, uri)| {
                if uri.is_empty() {
                    None
                } else {
                    Some(uri.clone())
                }
            })
    }

    /// The path step for a form: its name, with a one-based position when it is
    /// not the first same-named child of its container.
    fn path_segment(&mut self, name: &Name) -> String {
        let key = name.to_string();
        let frame = self.counts.last_mut().expect("a counter frame is open");
        let seen = frame.entry(key.clone()).or_insert(0);
        *seen += 1;
        if *seen == 1 {
            key
        } else {
            format!("{key}[{seen}]")
        }
    }
}

/// Refuses a name whose parts are not `NCName`s.
///
/// [`Name::parse`](crate::compose::Name::parse) checks what it parses, but
/// [`Name::local`](crate::compose::Name::local) and
/// [`Name::prefixed`](crate::compose::Name::prefixed) take the author's word,
/// and so does the string-literal name spelling of `form!`. Every name that
/// reaches the document is therefore checked here, at build time, where the
/// rest of the name rules are decided: an element or attribute name that no
/// well-formed document could carry is
/// [`ComposeError::InvalidName`](crate::compose::ComposeError::InvalidName).
fn check_name(name: &Name) -> Result<(), ComposeError> {
    if is_ncname(name.local_name()) && (name.prefix().is_empty() || is_ncname(name.prefix())) {
        return Ok(());
    }
    Err(ComposeError::InvalidName(name.to_string()))
}

#[cfg(test)]
mod tests {
    use bumpalo::Bump;

    use super::*;
    use crate::compose::{Composer, IntoContent, Value};
    use crate::document::SerializeOptions;
    use crate::namespace::NameTable;
    use crate::xpath::XPathValue;

    fn xml<'a>(form: Form<'a>, c: &Composer<'a>) -> String {
        c.build(form)
            .expect("the form builds")
            .to_xml(&SerializeOptions::default())
            .expect("the document serializes")
    }

    #[test]
    fn an_unprefixed_name_takes_the_default_element_namespace() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names).with_default_element_namespace("urn:d");

        let mut form = Form::new(Name::local("root"));
        form.attr(Name::local("k"), AttrValue::text("v"));
        // The default namespace never reaches the attribute.
        assert_eq!(xml(form, &c), r#"<root xmlns="urn:d" k="v"/>"#);
    }

    #[test]
    fn the_composer_table_resolves_a_prefix() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names).with_namespace("p", "urn:x");

        let mut form = Form::new(Name::prefixed("p", "root"));
        form.attr(Name::prefixed("p", "k"), AttrValue::text("v"));
        assert_eq!(xml(form, &c), r#"<p:root xmlns:p="urn:x" p:k="v"/>"#);
    }

    #[test]
    fn a_form_declaration_shadows_the_composer_table() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names).with_namespace("p", "urn:outer");

        let mut inner = Form::new(Name::prefixed("p", "inner"));
        inner.declare("p", "urn:inner");

        let mut outer = Form::new(Name::prefixed("p", "outer"));
        outer.push(Content::Element(inner));

        assert_eq!(
            xml(outer, &c),
            r#"<p:outer xmlns:p="urn:outer"><p:inner xmlns:p="urn:inner"/></p:outer>"#,
        );
    }

    #[test]
    fn a_form_declaration_is_in_scope_for_descendants() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);

        let child = Form::new(Name::prefixed("p", "child"));
        let mut root = Form::new(Name::local("root"));
        root.declare("p", "urn:x").push(Content::Element(child));

        assert_eq!(xml(root, &c), r#"<root xmlns:p="urn:x"><p:child/></root>"#,);
    }

    #[test]
    fn a_form_can_undeclare_the_default_namespace() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names).with_default_element_namespace("urn:d");

        let mut bare = Form::new(Name::local("bare"));
        bare.declare("", "");

        let mut root = Form::new(Name::local("root"));
        root.push(Content::Element(bare));

        assert_eq!(
            xml(root, &c),
            r#"<root xmlns="urn:d"><bare xmlns=""/></root>"#,
        );
    }

    #[test]
    fn the_xml_prefix_needs_no_declaration() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);

        let mut form = Form::new(Name::local("p"));
        form.attr(Name::prefixed("xml", "lang"), AttrValue::text("en"));
        assert_eq!(xml(form, &c), r#"<p xml:lang="en"/>"#);
    }

    #[test]
    fn an_unbound_prefix_names_the_form_path() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);

        let mut root = Form::new(Name::local("result"));
        root.push(Content::Element(Form::new(Name::local("item_tuple"))))
            .push(Content::Element(Form::new(Name::local("item_tuple"))))
            .push(Content::Element(Form::new(Name::prefixed(
                "nope",
                "item_tuple",
            ))));

        match c.build(root) {
            Err(ComposeError::UnboundPrefix { prefix, at }) => {
                assert_eq!(prefix, "nope");
                assert_eq!(at, "result/nope:item_tuple");
            }
            other => panic!("expected an unbound prefix, got {other:?}"),
        }
    }

    #[test]
    fn an_unbound_attribute_prefix_names_the_form_path() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);

        let mut third = Form::new(Name::local("item_tuple"));
        third.attr(Name::prefixed("q", "k"), AttrValue::text("v"));

        let mut root = Form::new(Name::local("result"));
        root.push(Content::Element(Form::new(Name::local("item_tuple"))))
            .push(Content::Element(Form::new(Name::local("item_tuple"))))
            .push(Content::Element(third));

        match c.build(root) {
            Err(ComposeError::UnboundPrefix { prefix, at }) => {
                assert_eq!(prefix, "q");
                assert_eq!(at, "result/item_tuple[3]");
            }
            other => panic!("expected an unbound prefix, got {other:?}"),
        }
    }

    #[test]
    fn a_generated_prefix_reaches_the_output() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names)
            .with_namespace("p", "urn:a")
            .with_namespace("q", "urn:b");

        // The source binds the same prefix to a different namespace, so the
        // copied attribute cannot keep it.
        let source = c
            .load_str(r#"<s xmlns:p="urn:b" p:k="v"/>"#)
            .expect("the source parses");
        let attr = c
            .eval("//@q:k", &[], Some(source.root()), Vec::new())
            .expect("the attribute is found");
        assert_eq!(attr.len(), 1, "one attribute node");

        let mut form = Form::new(Name::prefixed("p", "out"));
        form.push(Content::Value(attr.into_inner()));

        let written = xml(form, &c);
        assert!(written.contains("ns0:k=\"v\""), "{written}");
        assert!(written.contains("xmlns:ns0=\"urn:b\""), "{written}");
        assert!(written.contains("xmlns:p=\"urn:a\""), "{written}");
    }

    #[test]
    fn an_attribute_after_content_names_the_form_path() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);

        let source = c.load_str(r#"<s k="v"/>"#).expect("the source parses");
        let attr = c
            .eval("//@k", &[], Some(source.root()), Vec::new())
            .expect("the attribute is found");

        let mut inner = Form::new(Name::local("item"));
        inner
            .push(Content::Text("text first".to_string()))
            .push(Content::Value(attr.into_inner()));

        let mut root = Form::new(Name::local("result"));
        root.push(Content::Element(inner));

        match c.build(root) {
            Err(ComposeError::Copy { source, at }) => {
                assert_eq!(at, "result/item");
                assert!(source.to_string().contains("content"), "{source}");
            }
            other => panic!("expected a copy refusal, got {other:?}"),
        }
    }

    #[test]
    fn two_separate_sequences_of_atomic_values_join_with_one_space() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);

        let mut form = Form::new(Name::local("p"));
        form.push(Content::Value(XPathValue::from_atomic(XmlValue::string(
            "a",
        ))))
        .push(Content::Value(XPathValue::from_atomic(XmlValue::string(
            "b",
        ))));

        assert_eq!(xml(form, &c), "<p>a b</p>");
    }

    #[test]
    fn an_empty_sequence_between_two_others_does_not_break_adjacency() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);

        let mut form = Form::new(Name::local("p"));
        form.push(Content::Value(XPathValue::from_atomic(XmlValue::string(
            "a",
        ))))
        .push(Option::<&str>::None.into_content())
        .push(Content::Value(XPathValue::from_atomic(XmlValue::string(
            "b",
        ))));

        assert_eq!(xml(form, &c), "<p>a b</p>");
    }

    #[test]
    fn literal_text_concatenates_and_breaks_an_atomic_run() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);

        let mut form = Form::new(Name::local("p"));
        form.push(Content::Text("a".to_string()))
            .push(Content::Text("b".to_string()))
            .push(Content::Value(XPathValue::from_atomic(XmlValue::string(
                "c",
            ))))
            .push(Content::Text("d".to_string()))
            .push(Content::Value(XPathValue::from_atomic(XmlValue::string(
                "e",
            ))));

        assert_eq!(xml(form, &c), "<p>abcde</p>");
    }

    #[test]
    fn an_attribute_sequence_is_atomized_and_space_joined() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);
        let source = c
            .load_str("<a><b>one</b><b>two</b></a>")
            .expect("the source parses");

        let bs = c
            .eval("//b", &[], Some(source.root()), Vec::new())
            .expect("the nodes are found");

        let mut form = Form::new(Name::local("p"));
        form.attr(Name::local("all"), AttrValue::value(bs)).attr(
            Name::local("mixed"),
            AttrValue::value(vec![XmlValue::string("x"), XmlValue::integer(2.into())]),
        );

        assert_eq!(xml(form, &c), r#"<p all="one two" mixed="x 2"/>"#);
    }

    #[test]
    fn an_empty_attribute_sequence_is_an_empty_value() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);

        let mut form = Form::new(Name::local("p"));
        form.attr(Name::local("k"), AttrValue::value(()));
        assert_eq!(xml(form, &c), r#"<p k=""/>"#);
    }

    #[test]
    fn optional_content_is_present_or_absent() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);
        let present: Option<Form<'_>> = Some(Form::new(Name::local("here")));
        let absent: Option<Form<'_>> = None;

        let mut root = Form::new(Name::local("root"));
        root.push(present.into_content())
            .push(absent.into_content());

        assert_eq!(xml(root, &c), "<root><here/></root>");
    }

    #[test]
    fn comments_and_processing_instructions_are_written() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);

        let mut form = Form::new(Name::local("root"));
        form.push(Content::Comment(" note ".to_string()))
            .push(Content::Pi("work".to_string(), "now".to_string()));

        assert_eq!(xml(form, &c), "<root><!-- note --><?work now?></root>");
    }

    #[test]
    fn a_spliced_node_is_copied_not_shared() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);
        let source = c
            .load_str("<a><b k='v'>text</b></a>")
            .expect("the source parses");

        let b = c
            .eval("//b", &[], Some(source.root()), Vec::new())
            .expect("the node is found");

        let mut form = Form::new(Name::local("out"));
        form.push(Content::Value(b.into_inner()));

        assert_eq!(xml(form, &c), r#"<out><b k="v">text</b></out>"#);
    }

    #[test]
    fn an_empty_value_leaves_an_empty_element() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);

        let empty: Value<'_> = XPathValue::Empty.into();
        let mut high = Form::new(Name::local("high_bid"));
        high.push(empty.into_content());

        assert_eq!(xml(high, &c), "<high_bid/>");
    }

    #[test]
    fn the_emitter_can_be_driven_directly() {
        use crate::document::BufferDocumentOptions;

        let arena = Bump::new();
        let names = NameTable::new();
        let builder =
            BufferDocumentBuilder::new(&arena, &names, None, BufferDocumentOptions::default())
                .expect("a builder");

        let mut emitter = Emitter::new(builder, CopyOptions::default());
        emitter
            .element(Form::new(Name::local("one")))
            .expect("the first element");
        emitter
            .element(Form::new(Name::local("two")))
            .expect("the second element");
        let doc = emitter.finish().expect("the document finalizes");

        assert_eq!(
            crate::document::serialize::to_string(
                &doc.create_navigator(),
                &SerializeOptions::default()
            )
            .expect("serialization"),
            "<one/><two/>",
        );
    }
}
