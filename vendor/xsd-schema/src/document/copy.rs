//! Copying nodes into a document under construction, with constructor
//! semantics.
//!
//! [`BufferDocumentBuilder`] can build a tree
//! from scratch, and [`serialize`](super::serialize) can write one out. This
//! module adds the step in between: taking nodes that already exist — in
//! another `BufferDocument`, in a `roxmltree` document, in any navigator a host
//! supplies — and appending them to the document being built, as a *new* node
//! with a new identity rather than a reference to the old one.
//!
//! ```
//! use bumpalo::Bump;
//! use xsd_schema::document::{serialize, BufferDocument, BufferDocumentBuilder};
//! use xsd_schema::document::{BufferDocumentOptions, CopyOptions};
//! use xsd_schema::namespace::NameTable;
//! use xsd_schema::navigator::DomNavigator;
//!
//! let arena = Bump::new();
//! let names = NameTable::new();
//! let source = BufferDocument::from_reader_default(
//!     r#"<doc xmlns:p="urn:x"><p:keep k="1">text</p:keep></doc>"#.as_bytes(),
//!     &arena,
//!     &names,
//! )?;
//!
//! // The node to copy: the one child of the document element.
//! let mut keep = source.create_navigator();
//! keep.move_to_first_child();
//! keep.move_to_first_child();
//!
//! let target_arena = Bump::new();
//! let mut builder =
//!     BufferDocumentBuilder::new(&target_arena, &names, None, BufferDocumentOptions::default())?;
//! builder.start_element("wrapper", "", "", &[])?;
//! builder.end_of_attributes();
//! builder.copy_subtree(&keep, CopyOptions::default())?;
//! builder.end_element()?;
//! let target = builder.finalize()?;
//!
//! // The prefix `p` was declared on an ancestor in the source; the copy
//! // carries the declaration it needs itself.
//! assert_eq!(
//!     serialize::to_string(&target.create_navigator(), &Default::default())?,
//!     r#"<wrapper><p:keep xmlns:p="urn:x" k="1">text</p:keep></wrapper>"#,
//! );
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # What "constructor semantics" means here
//!
//! The rules are the ones an XQuery element constructor applies to its content
//! sequence — the constructor content rules (XQuery 1.0 §3.7.1.3), quoted
//! here as the design states them:
//!
//! > Any atomic value in the sequence is cast to a string. […] Any consecutive
//! > sequence of strings within the result sequence is converted to a single
//! > text node, whose string value contains the content of each of the strings
//! > in turn, with a single space (#x20) used as a separator between successive
//! > strings. Any document node within the result sequence is replaced by a
//! > sequence containing each of its children, in document order. Zero-length
//! > text nodes within the result sequence are removed. Adjacent text nodes
//! > within the result sequence are merged into a single text node.
//!
//! [`append_content`](BufferDocumentBuilder::append_content) implements that
//! table item by item, and two further rules of XQuery 1.0 §3.7.1:
//!
//! * an attribute that follows a child in the sequence is an error
//!   ([`CopyError::AttributeAfterContent`], XQuery XQTY0024), as is an
//!   attribute with no element to attach it to
//!   ([`CopyError::AttributeOutsideElement`]);
//! * two attributes with the same expanded name cannot both survive. XQuery
//!   raises XQDY0025; this builder keeps the **later** value, which needs no
//!   lookahead over the sequence. Refusing the duplicate instead is future
//!   work.
//!
//! # Namespaces
//!
//! A copied element's names have to keep resolving in their new home, where
//! the ancestors are different ones. [`NamespaceFixup`] works out, for each
//! element, the declarations it must carry — XQuery 1.0 §3.7.4 calls this
//! namespace fixup — and generates prefixes (`ns0`, `ns1`, …) where a prefix
//! the source used is already bound to something else. This is what lets
//! [`serialize`](super::serialize) be strict: it refuses to write a name whose
//! prefix is unbound, and a tree built through this module never contains one.
//!
//! [`CopyOptions::namespaces`] and [`CopyOptions::inherit`] are the two words
//! of XQuery's `copy-namespaces` declaration: whether declarations that the
//! source element carried but does not need are preserved, and whether the
//! copy inherits the declarations in force at the insertion point.
//!
//! # Type annotations
//!
//! By default a copy is untyped: the new nodes have no schema binding, so they
//! atomize as `xs:untypedAtomic`, whatever the source was.
//! [`Annotations::Preserve`] carries the source's bindings over, but only when
//! source and target are bound to the *same* [`SchemaSet`] — a type key means
//! nothing in another set, so that case is
//! [`CopyError::SchemaMismatch`] rather than a
//! silently wrong annotation.

use std::ptr;

use crate::namespace::XML_NAMESPACE;
use crate::navigator::{DomNavigator, DomNodeType, NamespaceAxisScope, TypedValue};
use crate::schema::SchemaSet;
use crate::types::value::XmlValue;
use crate::xpath::{XPathError, XmlItem};

use super::navigator::BufferDocNavigator;
use super::{BufferDocumentBuilder, BufferDocumentError, NodeSchemaBinding};

/// The one prefix that is bound without a declaration and can never be
/// declared (Namespaces in XML 1.0 §3).
const XML_PREFIX: &str = "xml";

// ── Options ───────────────────────────────────────────────────────────

/// Whether a copied element keeps the namespace declarations it carried.
///
/// This is the first word of XQuery's `copy-namespaces` declaration. It is
/// only ever visible for a declaration the subtree does not *need*: one it
/// needs is added by the fixup either way.
///
/// ```
/// use xsd_schema::document::CopyNamespaces;
///
/// assert_eq!(CopyNamespaces::default(), CopyNamespaces::Preserve);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CopyNamespaces {
    /// Carry over the declarations the source element made itself, used or not.
    #[default]
    Preserve,
    /// Declare only what the copied names actually need.
    NoPreserve,
}

/// Whether a copy carries the source's schema type annotations.
///
/// ```
/// use xsd_schema::document::Annotations;
///
/// assert_eq!(Annotations::default(), Annotations::Strip);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Annotations {
    /// Copy nodes untyped, whatever the source was: no binding, no nil flag.
    #[default]
    Strip,
    /// Replay the source's bindings and nil flags, when both documents share a
    /// [`SchemaSet`].
    Preserve,
}

/// How a subtree, an attribute or a content sequence is copied.
///
/// The default is XQuery's own default construction mode: declarations
/// preserved, the insertion point's namespaces inherited, annotations
/// stripped.
///
/// ```
/// use xsd_schema::document::{Annotations, CopyNamespaces, CopyOptions};
///
/// let opts = CopyOptions::default();
/// assert_eq!(opts.namespaces, CopyNamespaces::Preserve);
/// assert!(opts.inherit);
/// assert_eq!(opts.annotations, Annotations::Strip);
///
/// // A subtree that should stand entirely on its own:
/// let standalone = CopyOptions {
///     namespaces: CopyNamespaces::NoPreserve,
///     inherit: false,
///     ..CopyOptions::default()
/// };
/// assert!(!standalone.inherit);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CopyOptions {
    /// Whether an element keeps declarations it does not need.
    pub namespaces: CopyNamespaces,
    /// Whether the copy inherits the namespaces in scope at the insertion
    /// point (`true`, the default) or re-declares what it needs on its own top
    /// element (`false`).
    pub inherit: bool,
    /// Whether schema type annotations are carried over.
    pub annotations: Annotations,
}

impl Default for CopyOptions {
    fn default() -> Self {
        Self {
            namespaces: CopyNamespaces::default(),
            inherit: true,
            annotations: Annotations::default(),
        }
    }
}

// ── Errors ────────────────────────────────────────────────────────────

/// Why a node could not be copied.
///
/// ```
/// use bumpalo::Bump;
/// use xsd_schema::document::{BufferDocument, BufferDocumentBuilder};
/// use xsd_schema::document::{BufferDocumentOptions, CopyError, CopyOptions};
/// use xsd_schema::namespace::NameTable;
/// use xsd_schema::navigator::DomNavigator;
///
/// let arena = Bump::new();
/// let names = NameTable::new();
/// let source = BufferDocument::from_reader_default(r#"<a k="v"/>"#.as_bytes(), &arena, &names)?;
/// let mut attr = source.create_navigator();
/// attr.move_to_first_child();
/// attr.move_to_first_attribute();
///
/// // Nothing is open, so there is no element to put the attribute on.
/// let mut builder =
///     BufferDocumentBuilder::new(&arena, &names, None, BufferDocumentOptions::default())?;
/// match builder.copy_attribute(&attr, CopyOptions::default()) {
///     Err(CopyError::AttributeOutsideElement { name }) => assert_eq!(name, "k"),
///     other => panic!("expected a refusal, got {other:?}"),
/// }
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, thiserror::Error)]
pub enum CopyError {
    /// An attribute was appended to an element that already has a child
    /// (XQuery XQTY0024).
    #[error("attribute {name:?} cannot be added after the element already has content")]
    AttributeAfterContent {
        /// The attribute's qualified name, as the source spells it.
        name: String,
    },

    /// An attribute was appended with no element open, i.e. at document level
    /// (XQuery XQTY0024).
    #[error("attribute {name:?} cannot be added outside an element")]
    AttributeOutsideElement {
        /// The attribute's qualified name, as the source spells it.
        name: String,
    },

    /// [`Annotations::Preserve`] was asked for, but the source document is
    /// bound to a different [`SchemaSet`] than the document being built, so
    /// its type keys mean nothing here.
    #[error("type annotations cannot be preserved across two different schema sets")]
    SchemaMismatch,

    /// A node kind this copy cannot represent: a namespace node used as
    /// content, or a node that is not an attribute passed to
    /// [`copy_attribute`](BufferDocumentBuilder::copy_attribute).
    #[error("{0}")]
    Unsupported(&'static str),

    /// The builder refused the node (the document is full, or an element was
    /// closed that was never opened).
    #[error("the document builder failed: {0}")]
    Document(#[from] BufferDocumentError),

    /// An atomic item could not be atomized to a string value.
    #[error("an atomic value could not be atomized: {0}")]
    Atomize(#[from] XPathError),
}

// ── CopySource ────────────────────────────────────────────────────────

/// A navigator that can also report the PSVI facts a copy may carry over.
///
/// Every [`DomNavigator`] is a `CopySource`: the default bodies say "nothing to
/// preserve", which is the right answer for a navigator over a document that
/// was never validated. A schema-aware navigator overrides them, and then
/// [`Annotations::Preserve`] has something to replay.
///
/// ```
/// use xsd_schema::document::CopySource;
/// use xsd_schema::navigator::RoXmlNavigator;
///
/// let doc = roxmltree::Document::parse("<a/>")?;
/// let nav = RoXmlNavigator::new(&doc);
///
/// // An untyped source: a copy from it is untyped whatever the options say.
/// assert!(nav.schema_binding().is_none());
/// assert!(CopySource::schema_set(&nav).is_none());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub trait CopySource: DomNavigator {
    /// The schema binding of the node at the cursor, if it carries one.
    fn schema_binding(&self) -> Option<NodeSchemaBinding> {
        None
    }

    /// The schema set the node's document is bound to, if any.
    ///
    /// A copy preserves annotations only when this is the very same set the
    /// target document is bound to, compared by address.
    fn schema_set(&self) -> Option<&SchemaSet> {
        None
    }
}

impl CopySource for BufferDocNavigator<'_> {
    fn schema_binding(&self) -> Option<NodeSchemaBinding> {
        // The inherent accessor borrows out of the remap table; a binding is
        // `Copy`, and the copy has to outlive the navigator's borrow anyway.
        BufferDocNavigator::schema_binding(self).copied()
    }

    fn schema_set(&self) -> Option<&SchemaSet> {
        self.document().schema_set()
    }
}

impl CopySource for crate::navigator::RoXmlNavigator<'_> {}

// ── NamespaceFixup ────────────────────────────────────────────────────

/// The namespace declarations a constructed element has to carry.
///
/// A `NamespaceFixup` mirrors the open elements of the document being built:
/// one scope per element, innermost last. For each element it answers the only
/// question namespace fixup (XQuery 1.0 §3.7.4) asks — which declarations does
/// this element need so that its own name and its attributes' names resolve? —
/// and rewrites an attribute's prefix when the one it came with is taken.
///
/// [`declarations_for`](Self::declarations_for) **pushes** the scope it
/// computes, so the caller mirrors the element's end with
/// [`pop_scope`](Self::pop_scope):
///
/// ```
/// use xsd_schema::document::NamespaceFixup;
///
/// let mut fixup = NamespaceFixup::new();
/// fixup.push_scope(&[("p".to_string(), "urn:outer".to_string())]);
/// assert_eq!(fixup.in_scope("p"), Some("urn:outer"));
///
/// // An element in urn:inner with a `p:k` attribute in urn:other: the
/// // element declares its own prefix, and the attribute cannot borrow the
/// // conflicting `p`, so it gets a generated one.
/// let (decls, attrs) =
///     fixup.declarations_for(("q", "urn:inner"), &[("p", "urn:other")], &[]);
/// assert_eq!(
///     decls,
///     vec![
///         ("q".to_string(), "urn:inner".to_string()),
///         ("ns0".to_string(), "urn:other".to_string()),
///     ],
/// );
/// assert_eq!(attrs, vec![("ns0".to_string(), "urn:other".to_string())]);
///
/// fixup.pop_scope();
/// assert_eq!(fixup.in_scope("q"), None);
/// ```
#[derive(Clone, Debug, Default)]
pub struct NamespaceFixup {
    /// Every binding of every open scope, innermost last. An entry with an
    /// empty prefix and an empty URI is an undeclared default namespace.
    bindings: Vec<(String, String)>,
    /// Where each open scope starts in `bindings`.
    marks: Vec<usize>,
}

impl NamespaceFixup {
    /// A fixup with nothing in scope.
    ///
    /// ```
    /// use xsd_schema::document::NamespaceFixup;
    ///
    /// let fixup = NamespaceFixup::new();
    /// assert_eq!(fixup.in_scope("p"), None);
    /// // The empty prefix always resolves: to no namespace.
    /// assert_eq!(fixup.in_scope(""), Some(""));
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens a scope holding `decls`, as an element start does.
    ///
    /// ```
    /// use xsd_schema::document::NamespaceFixup;
    ///
    /// let mut fixup = NamespaceFixup::new();
    /// fixup.push_scope(&[(String::new(), "urn:d".to_string())]);
    /// assert_eq!(fixup.in_scope(""), Some("urn:d"));
    /// ```
    pub fn push_scope(&mut self, decls: &[(String, String)]) {
        self.marks.push(self.bindings.len());
        self.bindings.extend_from_slice(decls);
    }

    /// Closes the innermost scope, as an element end does. A no-op when no
    /// scope is open.
    ///
    /// ```
    /// use xsd_schema::document::NamespaceFixup;
    ///
    /// let mut fixup = NamespaceFixup::new();
    /// fixup.push_scope(&[("p".to_string(), "urn:x".to_string())]);
    /// fixup.pop_scope();
    /// assert_eq!(fixup.in_scope("p"), None);
    /// fixup.pop_scope(); // harmless
    /// ```
    pub fn pop_scope(&mut self) {
        if let Some(mark) = self.marks.pop() {
            self.bindings.truncate(mark);
        }
    }

    /// The namespace URI `prefix` resolves to here, or `None` when the prefix
    /// is not usable.
    ///
    /// The empty prefix always resolves — to the empty URI when no default
    /// namespace is in scope — and `xml` resolves without a declaration. A
    /// prefix bound to the empty URI is an XML 1.1 undeclaration, which XML
    /// 1.0 cannot write, so it reads as unusable rather than as "no
    /// namespace".
    ///
    /// ```
    /// use xsd_schema::document::NamespaceFixup;
    ///
    /// let mut fixup = NamespaceFixup::new();
    /// fixup.push_scope(&[("p".to_string(), "urn:x".to_string())]);
    /// assert_eq!(fixup.in_scope("p"), Some("urn:x"));
    /// assert_eq!(fixup.in_scope("q"), None);
    /// assert_eq!(fixup.in_scope("xml"), Some("http://www.w3.org/XML/1998/namespace"));
    /// ```
    pub fn in_scope(&self, prefix: &str) -> Option<&str> {
        if prefix == XML_PREFIX {
            return Some(XML_NAMESPACE);
        }
        match self
            .bindings
            .iter()
            .rev()
            .find(|(p, _)| p == prefix)
            .map(|(_, u)| u.as_str())
        {
            Some("") if !prefix.is_empty() => None,
            Some(uri) => Some(uri),
            None if prefix.is_empty() => Some(""),
            None => None,
        }
    }

    /// The declarations an element must carry so that its own name and every
    /// attribute name resolves, plus the — possibly re-prefixed — attribute
    /// names to store.
    ///
    /// `elem` and the entries of `attrs` are `(prefix, namespace_uri)` pairs;
    /// `keep` holds declarations to carry over from a copied source element
    /// (its local namespace axis under [`CopyNamespaces::Preserve`], nothing
    /// under [`CopyNamespaces::NoPreserve`]). The returned attribute names come
    /// back in the order they were given, so a caller pairs them with its own
    /// local names and values by index.
    ///
    /// The rules, in the order they are applied:
    ///
    /// 1. The element's own name is declared unless it already resolves. An
    ///    unprefixed name in no namespace declares `xmlns=""` when a default
    ///    namespace would otherwise apply to it.
    /// 2. `keep` follows, minus anything that already resolves identically and
    ///    minus any entry that would rebind the element's own prefix: the
    ///    element's binding wins.
    /// 3. An attribute in a namespace keeps its prefix when that prefix
    ///    resolves to the same URI, and gets the prefix declared when it is
    ///    free. An empty prefix (which cannot carry a namespaced attribute —
    ///    Namespaces in XML 1.0 §6.2) and a prefix bound to another URI both
    ///    get a generated prefix, `ns0`, `ns1`, … — the first that is neither
    ///    in scope nor already declared on this element.
    ///
    /// The resulting scope is pushed before returning; call
    /// [`pop_scope`](Self::pop_scope) when the element ends.
    ///
    /// ```
    /// use xsd_schema::document::NamespaceFixup;
    ///
    /// let mut fixup = NamespaceFixup::new();
    ///
    /// // A prefixed element with an unprefixed attribute in no namespace:
    /// // one declaration, and the attribute name is unchanged.
    /// let (decls, attrs) = fixup.declarations_for(("p", "urn:x"), &[("", "")], &[]);
    /// assert_eq!(decls, vec![("p".to_string(), "urn:x".to_string())]);
    /// assert_eq!(attrs, vec![(String::new(), String::new())]);
    ///
    /// // Nested inside it, an unused declaration carried over by `keep`.
    /// let (inner, _) = fixup.declarations_for(
    ///     ("p", "urn:x"),
    ///     &[],
    ///     &[("unused".to_string(), "urn:u".to_string())]
    ///         .iter()
    ///         .map(|(p, u)| (p.as_str(), u.as_str()))
    ///         .collect::<Vec<_>>(),
    /// );
    /// // `p` already resolves, so only the carried declaration is new.
    /// assert_eq!(inner, vec![("unused".to_string(), "urn:u".to_string())]);
    /// ```
    // The tuple of two `(prefix, uri)` lists *is* the documented shape of the
    // answer — declarations to write, names to store — and naming either half
    // would hide which is which at the call site.
    #[allow(clippy::type_complexity)]
    pub fn declarations_for(
        &mut self,
        elem: (&str, &str),
        attrs: &[(&str, &str)],
        keep: &[(&str, &str)],
    ) -> (Vec<(String, String)>, Vec<(String, String)>) {
        let (elem_prefix, elem_uri) = elem;
        let mut decls: Vec<(String, String)> = Vec::new();

        // Rule 1 — the element's own name, first, so it wins every conflict.
        // A prefixed name in no namespace cannot be written in XML 1.0 at all;
        // nothing is declared for it and the serializer reports it.
        if elem_prefix != XML_PREFIX
            && (!elem_uri.is_empty() || elem_prefix.is_empty())
            && self.in_scope(elem_prefix) != Some(elem_uri)
        {
            decls.push((elem_prefix.to_string(), elem_uri.to_string()));
        }

        // Rule 2 — declarations carried over from the source element.
        for &(prefix, uri) in keep {
            if prefix == XML_PREFIX
                || (prefix == elem_prefix && uri != elem_uri)
                || (uri.is_empty() && !prefix.is_empty())
            {
                continue;
            }
            if resolve(self, &decls, prefix) != Some(uri) {
                decls.push((prefix.to_string(), uri.to_string()));
            }
        }

        // Rule 3 — attribute names.
        let mut names = Vec::with_capacity(attrs.len());
        for &(prefix, uri) in attrs {
            names.push(self.attribute_name(prefix, uri, &mut decls));
        }

        self.push_scope(&decls);
        (decls, names)
    }

    /// The prefix an attribute in `uri` is stored with, declaring one in
    /// `decls` when it needs declaring.
    fn attribute_name(
        &self,
        prefix: &str,
        uri: &str,
        decls: &mut Vec<(String, String)>,
    ) -> (String, String) {
        // In no namespace: unprefixed, whatever the source called it. The
        // default namespace never applies to an attribute (Namespaces in XML
        // 1.0 §6.2), so this needs no declaration and can never conflict.
        if uri.is_empty() {
            return (String::new(), String::new());
        }
        if uri == XML_NAMESPACE {
            return (XML_PREFIX.to_string(), uri.to_string());
        }
        if !prefix.is_empty() {
            match resolve(self, decls, prefix) {
                Some(bound) if bound == uri => return (prefix.to_string(), uri.to_string()),
                Some(_) => {} // taken by another URI — generate one below
                None => {
                    decls.push((prefix.to_string(), uri.to_string()));
                    return (prefix.to_string(), uri.to_string());
                }
            }
        }
        let generated = self.generated_prefix(decls);
        decls.push((generated.clone(), uri.to_string()));
        (generated, uri.to_string())
    }

    /// The first `ns{k}` that is neither in scope nor already declared here.
    fn generated_prefix(&self, decls: &[(String, String)]) -> String {
        let mut k = 0u32;
        loop {
            let candidate = format!("ns{k}");
            if resolve(self, decls, &candidate).is_none() {
                return candidate;
            }
            k += 1;
        }
    }
}

/// [`NamespaceFixup::in_scope`] extended with the declarations collected for
/// an element that has not been pushed yet.
fn resolve<'r>(
    fixup: &'r NamespaceFixup,
    pending: &'r [(String, String)],
    prefix: &str,
) -> Option<&'r str> {
    if prefix == XML_PREFIX {
        return Some(XML_NAMESPACE);
    }
    match pending.iter().rev().find(|(p, _)| p == prefix) {
        Some((_, uri)) if uri.is_empty() && !prefix.is_empty() => None,
        Some((_, uri)) => Some(uri.as_str()),
        None => fixup.in_scope(prefix),
    }
}

// ── Internals shared by the copy methods ──────────────────────────────

/// What to do with the source's type annotations, once the schema sets have
/// been compared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Annotate {
    Strip,
    Preserve,
}

/// One source attribute, read off before the target element is opened.
struct CopiedAttribute {
    local_name: String,
    namespace_uri: String,
    prefix: String,
    value: String,
    binding: Option<NodeSchemaBinding>,
}

/// The declarations the source element made itself, `xml` excluded.
fn local_declarations<N: DomNavigator>(nav: &N) -> Vec<(String, String)> {
    let mut decls = Vec::new();
    let mut ns = nav.clone();
    if ns.move_to_first_namespace(NamespaceAxisScope::Local) {
        loop {
            let prefix = ns.local_name();
            if prefix != XML_PREFIX {
                decls.push((prefix.to_string(), ns.value_ref().into_owned()));
            }
            if !ns.move_to_next_namespace(NamespaceAxisScope::Local) {
                break;
            }
        }
    }
    decls
}

/// The string an atomic item contributes to the content.
///
/// XDM sequences hold atomic values, so the value is atomized first: that is
/// the identity for an atomic value and unwraps a union. A list value is a
/// *sequence* of atomic values, and the constructor content rules join such a
/// run with a single space — exactly what its string value already is.
fn atomic_text(value: &XmlValue) -> Result<String, XPathError> {
    if value.is_atomic() || value.is_list() {
        Ok(value.to_string_value())
    } else {
        Ok(crate::xpath::atomize::atomize(value)?.to_string_value())
    }
}

/// The name to report in an attribute error, as the source spells it.
fn qualified_name<N: DomNavigator>(nav: &N) -> String {
    nav.name().to_string()
}

// ── Builder methods ───────────────────────────────────────────────────

impl<'a> BufferDocumentBuilder<'a> {
    /// Copies the node at the cursor, and everything below it, as the next
    /// child of the open element — or as a top-level node when no element is
    /// open.
    ///
    /// | Node kind | What is copied |
    /// |---|---|
    /// | element | the subtree: the element with its attributes, the declarations it needs, then its children |
    /// | text, comment, processing instruction | that one node |
    /// | document (`Root`) | its children, in document order — the node itself has no counterpart inside an element |
    /// | attribute | as [`copy_attribute`](Self::copy_attribute) |
    /// | namespace | [`CopyError::Unsupported`]: a namespace node is a property of an element, not content |
    ///
    /// The result is a new subtree with new node identities; nothing is shared
    /// with the source, which may live in another document, another arena or
    /// another navigator implementation entirely.
    ///
    /// ```
    /// use bumpalo::Bump;
    /// use xsd_schema::document::{serialize, BufferDocumentBuilder};
    /// use xsd_schema::document::{BufferDocumentOptions, CopyOptions};
    /// use xsd_schema::namespace::NameTable;
    /// use xsd_schema::navigator::{DomNavigator, RoXmlNavigator};
    ///
    /// // The source need not be a BufferDocument.
    /// let source = roxmltree::Document::parse("<doc><a>1</a><b/></doc>")?;
    /// let mut nav = RoXmlNavigator::new(&source);
    /// nav.move_to_first_child(); // <doc>
    /// nav.move_to_first_child(); // <a>
    ///
    /// let arena = Bump::new();
    /// let names = NameTable::new();
    /// let mut builder =
    ///     BufferDocumentBuilder::new(&arena, &names, None, BufferDocumentOptions::default())?;
    /// builder.start_element("out", "", "", &[])?;
    /// builder.end_of_attributes();
    /// builder.copy_subtree(&nav, CopyOptions::default())?;
    /// nav.move_to_next_sibling(); // <b/>
    /// builder.copy_subtree(&nav, CopyOptions::default())?;
    /// builder.end_element()?;
    /// let doc = builder.finalize()?;
    ///
    /// assert_eq!(
    ///     serialize::to_string(&doc.create_navigator(), &Default::default())?,
    ///     "<out><a>1</a><b/></out>",
    /// );
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn copy_subtree<N: CopySource>(
        &mut self,
        node: &N,
        opts: CopyOptions,
    ) -> Result<(), CopyError> {
        match node.node_type() {
            DomNodeType::Element => {
                let mode = self.annotate_mode(node, opts)?;
                let mut fixup = self.seeded_fixup(opts.inherit);
                self.copy_element_tree(node, opts, mode, &mut fixup)
            }
            DomNodeType::Root => {
                let mut child = node.clone();
                if child.move_to_first_child() {
                    loop {
                        self.copy_subtree(&child, opts)?;
                        if !child.move_to_next_sibling() {
                            break;
                        }
                    }
                }
                Ok(())
            }
            DomNodeType::Attribute => self.copy_attribute(node, opts),
            kind => self.copy_leaf(node, kind),
        }
    }

    /// Copies an attribute node onto the open element.
    ///
    /// The attribute is stored with a prefix that resolves where it lands: the
    /// source's prefix when that is free or already bound to the same URI, a
    /// generated one otherwise, with the declaration added to the open element.
    /// This is the one point where a declaration can appear after
    /// [`start_element`](Self::start_element) has returned, and it is sound
    /// because an attribute may only be added before the element has children.
    ///
    /// Errors when the element already has a child
    /// ([`CopyError::AttributeAfterContent`]) or when no element is open
    /// ([`CopyError::AttributeOutsideElement`]). An attribute whose expanded
    /// name the element already carries replaces that one's value and type
    /// annotation — the later of two duplicates wins, and keeps the earlier
    /// one's prefix. What the earlier one made its element answer to in
    /// `fn:id` goes with it.
    ///
    /// ```
    /// use bumpalo::Bump;
    /// use xsd_schema::document::{serialize, BufferDocument, BufferDocumentBuilder};
    /// use xsd_schema::document::{BufferDocumentOptions, CopyOptions};
    /// use xsd_schema::namespace::NameTable;
    /// use xsd_schema::navigator::DomNavigator;
    ///
    /// let arena = Bump::new();
    /// let names = NameTable::new();
    /// let source = BufferDocument::from_reader_default(
    ///     r#"<a xmlns:p="urn:x" p:k="v"/>"#.as_bytes(),
    ///     &arena,
    ///     &names,
    /// )?;
    /// let mut attr = source.create_navigator();
    /// attr.move_to_first_child();
    /// attr.move_to_first_attribute();
    ///
    /// let mut builder =
    ///     BufferDocumentBuilder::new(&arena, &names, None, BufferDocumentOptions::default())?;
    /// builder.start_element("out", "", "", &[])?;
    /// builder.copy_attribute(&attr, CopyOptions::default())?;
    /// builder.end_of_attributes();
    /// builder.end_element()?;
    /// let doc = builder.finalize()?;
    ///
    /// assert_eq!(
    ///     serialize::to_string(&doc.create_navigator(), &Default::default())?,
    ///     r#"<out xmlns:p="urn:x" p:k="v"/>"#,
    /// );
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn copy_attribute<N: CopySource>(
        &mut self,
        attr: &N,
        opts: CopyOptions,
    ) -> Result<(), CopyError> {
        if attr.node_type() != DomNodeType::Attribute {
            return Err(CopyError::Unsupported(
                "copy_attribute needs an attribute node",
            ));
        }
        if !self.has_open_element() {
            return Err(CopyError::AttributeOutsideElement {
                name: qualified_name(attr),
            });
        }
        if self.content_state().children_started {
            return Err(CopyError::AttributeAfterContent {
                name: qualified_name(attr),
            });
        }

        let mode = self.annotate_mode(attr, opts)?;
        let binding = if mode == Annotate::Preserve {
            attr.schema_binding()
        } else {
            None
        };
        let local_name = attr.local_name();
        let namespace_uri = attr.namespace_uri();
        let value = attr.value_ref();

        // A duplicate keeps its place and its prefix; its value and its
        // annotation are the later one's — including the lack of one: an
        // unannotated attribute must not inherit the earlier one's type.
        if let Some(existing) = self.find_attribute(local_name, namespace_uri) {
            self.set_attribute_value(existing, &value);
            match binding {
                Some(binding) => {
                    self.set_node_binding(existing, binding)?;
                }
                None => self.clear_node_binding(existing),
            }
            return Ok(());
        }

        // Fixup against the scope actually in force at the insertion point:
        // a lone attribute has no subtree to re-declare anything for.
        let fixup = self.seeded_fixup(true);
        let mut decls = Vec::new();
        let (prefix, _) = fixup.attribute_name(attr.prefix(), namespace_uri, &mut decls);
        for (decl_prefix, decl_uri) in &decls {
            self.declare_namespace_on_open_element(decl_prefix, decl_uri)?;
        }

        self.sync_last_attribute();
        let attr_ref = self.attribute(local_name, namespace_uri, &prefix, &value)?;
        if let Some(binding) = binding {
            self.set_node_binding(attr_ref, binding)?;
        }
        Ok(())
    }

    /// Appends a sequence of items as content, with the constructor content
    /// rules (XQuery 1.0 §3.7.1.3).
    ///
    /// Each item is handled by its kind: an atomic value becomes text —
    /// separated from a preceding atomic value by a single space, and from a
    /// node by nothing at all — a document node contributes its children, an
    /// attribute goes onto the open element, and anything else is copied as a
    /// subtree. The "previous item was atomic" state belongs to the open
    /// element, not to the call, so splitting a sequence across two calls makes
    /// no difference to the output.
    ///
    /// ```
    /// use bumpalo::Bump;
    /// use xsd_schema::document::{serialize, BufferDocument, BufferDocumentBuilder};
    /// use xsd_schema::document::{BufferDocumentOptions, CopyOptions};
    /// use xsd_schema::namespace::NameTable;
    /// use xsd_schema::navigator::DomNavigator;
    /// use xsd_schema::types::value::XmlValue;
    /// use xsd_schema::xpath::XmlItem;
    ///
    /// let arena = Bump::new();
    /// let names = NameTable::new();
    /// let source =
    ///     BufferDocument::from_reader_default("<x><em>b</em></x>".as_bytes(), &arena, &names)?;
    /// let mut em = source.create_navigator();
    /// em.move_to_first_child();
    /// em.move_to_first_child();
    ///
    /// let items = vec![
    ///     XmlItem::Atomic(XmlValue::string("a")),
    ///     XmlItem::Node(em),
    ///     XmlItem::Atomic(XmlValue::string("c")),
    ///     XmlItem::Atomic(XmlValue::string("d")),
    /// ];
    ///
    /// let mut builder =
    ///     BufferDocumentBuilder::new(&arena, &names, None, BufferDocumentOptions::default())?;
    /// builder.start_element("p", "", "", &[])?;
    /// builder.end_of_attributes();
    /// builder.append_content(&items, CopyOptions::default())?;
    /// builder.end_element()?;
    /// let doc = builder.finalize()?;
    ///
    /// // No separator next to a node, one space between two atomic values.
    /// assert_eq!(
    ///     serialize::to_string(&doc.create_navigator(), &Default::default())?,
    ///     "<p>a<em>b</em>c d</p>",
    /// );
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn append_content<N: CopySource>(
        &mut self,
        items: &[XmlItem<N>],
        opts: CopyOptions,
    ) -> Result<(), CopyError> {
        for item in items {
            match item {
                XmlItem::Atomic(value) => self.append_atomic(value)?,
                XmlItem::Node(node) => self.copy_subtree(node, opts)?,
            }
        }
        Ok(())
    }

    /// Appends one atomic value as text, with the separator rule of
    /// [`append_content`](Self::append_content).
    ///
    /// A single space goes in first when the item appended last to this
    /// element was also an atomic value. An explicit [`text`](Self::text) call
    /// is not an atomic item and breaks that run, so text and an atomic value
    /// meet without a separator.
    ///
    /// ```
    /// use bumpalo::Bump;
    /// use xsd_schema::document::{serialize, BufferDocumentBuilder, BufferDocumentOptions};
    /// use xsd_schema::namespace::NameTable;
    /// use xsd_schema::types::value::XmlValue;
    ///
    /// let arena = Bump::new();
    /// let names = NameTable::new();
    /// let mut builder =
    ///     BufferDocumentBuilder::new(&arena, &names, None, BufferDocumentOptions::default())?;
    /// builder.start_element("p", "", "", &[])?;
    /// builder.end_of_attributes();
    /// builder.append_atomic(&XmlValue::string("a"))?;
    /// builder.append_atomic(&XmlValue::boolean(true))?;
    /// builder.end_element()?;
    /// let doc = builder.finalize()?;
    ///
    /// assert_eq!(
    ///     serialize::to_string(&doc.create_navigator(), &Default::default())?,
    ///     "<p>a true</p>",
    /// );
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn append_atomic(&mut self, value: &XmlValue) -> Result<(), CopyError> {
        let text = atomic_text(value)?;
        if self.content_state().last_atomic {
            self.text(" ");
        }
        self.text(&text);
        let state = self.content_state_mut();
        state.children_started = true;
        state.last_atomic = true;
        Ok(())
    }

    // ── Internals ─────────────────────────────────────────────────────

    /// Copies a text, comment or processing-instruction node.
    fn copy_leaf<N: DomNavigator>(&mut self, node: &N, kind: DomNodeType) -> Result<(), CopyError> {
        match kind {
            DomNodeType::Text | DomNodeType::Whitespace | DomNodeType::SignificantWhitespace => {
                self.text(&node.value_ref());
                Ok(())
            }
            DomNodeType::Comment => {
                self.comment(&node.value_ref())?;
                Ok(())
            }
            DomNodeType::ProcessingInstruction => {
                self.processing_instruction(node.local_name(), &node.value_ref())?;
                Ok(())
            }
            DomNodeType::Namespace => Err(CopyError::Unsupported(
                "a namespace node is a property of an element and cannot be copied as content",
            )),
            _ => Err(CopyError::Unsupported(
                "DomNodeType::All is a wildcard, not a node kind that can be copied",
            )),
        }
    }

    /// Walks the source subtree iteratively — one explicit level of `depth`
    /// per open element, so a deep document cannot overflow the Rust stack —
    /// opening, filling and closing the copy as it goes.
    fn copy_element_tree<N: CopySource>(
        &mut self,
        start: &N,
        opts: CopyOptions,
        mode: Annotate,
        fixup: &mut NamespaceFixup,
    ) -> Result<(), CopyError> {
        let mut nav = start.clone();
        let mut depth = 0usize;
        loop {
            let mut descended = false;
            match nav.node_type() {
                DomNodeType::Element => {
                    self.start_copied_element(&nav, opts, mode, fixup)?;
                    let mut child = nav.clone();
                    if child.move_to_first_child() {
                        nav = child;
                        depth += 1;
                        descended = true;
                    } else {
                        self.end_element()?;
                        fixup.pop_scope();
                    }
                }
                // An element's children are elements, text, comments and PIs;
                // its attributes are copied with it, above.
                kind => self.copy_leaf(&nav, kind)?,
            }
            if descended {
                continue;
            }
            // Up and on, closing every element whose children are exhausted.
            loop {
                if depth == 0 {
                    return Ok(());
                }
                if nav.move_to_next_sibling() {
                    break;
                }
                nav.move_to_parent();
                depth -= 1;
                self.end_element()?;
                fixup.pop_scope();
            }
        }
    }

    /// Opens the copy of one element: its declarations and attributes are read
    /// off the source *before* `start_element`, because that is where the
    /// builder takes an element's declarations.
    fn start_copied_element<N: CopySource>(
        &mut self,
        nav: &N,
        opts: CopyOptions,
        mode: Annotate,
        fixup: &mut NamespaceFixup,
    ) -> Result<(), CopyError> {
        let mut attributes: Vec<CopiedAttribute> = Vec::new();
        let mut attr = nav.clone();
        if attr.move_to_first_attribute() {
            loop {
                attributes.push(CopiedAttribute {
                    local_name: attr.local_name().to_string(),
                    namespace_uri: attr.namespace_uri().to_string(),
                    prefix: attr.prefix().to_string(),
                    value: attr.value_ref().into_owned(),
                    binding: if mode == Annotate::Preserve {
                        attr.schema_binding()
                    } else {
                        None
                    },
                });
                if !attr.move_to_next_attribute() {
                    break;
                }
            }
        }

        let keep = if opts.namespaces == CopyNamespaces::Preserve {
            local_declarations(nav)
        } else {
            Vec::new()
        };
        let keep_refs: Vec<(&str, &str)> =
            keep.iter().map(|(p, u)| (p.as_str(), u.as_str())).collect();
        let attr_refs: Vec<(&str, &str)> = attributes
            .iter()
            .map(|a| (a.prefix.as_str(), a.namespace_uri.as_str()))
            .collect();
        let (decls, names) =
            fixup.declarations_for((nav.prefix(), nav.namespace_uri()), &attr_refs, &keep_refs);
        let decl_refs: Vec<(&str, &str)> = decls
            .iter()
            .map(|(p, u)| (p.as_str(), u.as_str()))
            .collect();

        let elem_ref = self.start_element(
            nav.local_name(),
            nav.namespace_uri(),
            nav.prefix(),
            &decl_refs,
        )?;
        for (attribute, (prefix, _)) in attributes.iter().zip(names.iter()) {
            let attr_ref = self.attribute(
                &attribute.local_name,
                &attribute.namespace_uri,
                prefix,
                &attribute.value,
            )?;
            if let Some(binding) = attribute.binding {
                self.set_node_binding(attr_ref, binding)?;
            }
        }
        if mode == Annotate::Preserve {
            if let Some(binding) = nav.schema_binding() {
                self.set_node_binding(elem_ref, binding)?;
            }
            if nav.typed_value() == TypedValue::Nilled {
                self.set_nil(elem_ref);
            }
        }
        self.end_of_attributes();
        Ok(())
    }

    /// Whether the source's annotations can be replayed here, per
    /// [`CopyOptions::annotations`].
    fn annotate_mode<N: CopySource>(
        &self,
        node: &N,
        opts: CopyOptions,
    ) -> Result<Annotate, CopyError> {
        if opts.annotations == Annotations::Strip {
            return Ok(Annotate::Strip);
        }
        match (node.schema_set(), self.schema_set()) {
            // Nothing to preserve: an untyped source copies untyped, as
            // `Strip` would.
            (None, _) => Ok(Annotate::Strip),
            (Some(source), Some(target)) if ptr::eq(source, target) => Ok(Annotate::Preserve),
            (Some(_), _) => Err(CopyError::SchemaMismatch),
        }
    }

    /// A fixup seeded with the scope a copy starts from.
    ///
    /// With `inherit` the copy sees the namespaces in force at the insertion
    /// point and re-declares nothing they already provide. Without it the copy
    /// starts from an empty scope, so its top element re-declares everything
    /// its names need — except that an inherited **default** namespace stays
    /// visible: XML 1.0 cannot undeclare a prefix, but an unprefixed name in no
    /// namespace does need `xmlns=""` under one, and leaving that out would
    /// change the copied element's name.
    fn seeded_fixup(&self, inherit: bool) -> NamespaceFixup {
        let mut fixup = NamespaceFixup::new();
        let bindings = self.in_scope_bindings();
        if inherit {
            fixup.push_scope(&bindings);
        } else if let Some(default) = bindings.iter().find(|(p, u)| p.is_empty() && !u.is_empty()) {
            fixup.push_scope(std::slice::from_ref(default));
        }
        fixup
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{serialize, BufferDocument, BufferDocumentOptions};
    use crate::namespace::NameTable;
    use crate::navigator::RoXmlNavigator;
    use crate::xpath::TreeComparer;
    use bumpalo::Bump;

    // ── Fixtures ──────────────────────────────────────────────────────

    fn parse<'a>(xml: &str, arena: &'a Bump, names: &'a NameTable) -> BufferDocument<'a> {
        BufferDocument::from_reader_default(xml.as_bytes(), arena, names)
            .expect("the fixture parses")
    }

    fn new_builder<'a>(arena: &'a Bump, names: &'a NameTable) -> BufferDocumentBuilder<'a> {
        BufferDocumentBuilder::new(arena, names, None, BufferDocumentOptions::default())
            .expect("a builder")
    }

    /// The element reached from the document root by following `path` as child
    /// indices (`[0]` is the document element).
    fn element_at<'d>(doc: &'d BufferDocument<'d>, path: &[usize]) -> BufferDocNavigator<'d> {
        let mut nav = doc.create_navigator();
        for &index in path {
            assert!(nav.move_to_first_child(), "no children at {path:?}");
            for _ in 0..index {
                assert!(nav.move_to_next_sibling(), "no sibling at {path:?}");
            }
        }
        nav
    }

    fn xml_of(doc: &BufferDocument<'_>) -> String {
        serialize::to_string(&doc.create_navigator(), &Default::default()).expect("serializes")
    }

    /// Copies `path` out of `source` into a target element opened with
    /// `target_declarations`, and returns the serialized result.
    fn copy_into(
        source: &str,
        path: &[usize],
        target_declarations: &[(&str, &str)],
        target_uri: &str,
        opts: CopyOptions,
    ) -> String {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = parse(source, &arena, &names);
        let node = element_at(&doc, path);

        let mut builder = new_builder(&arena, &names);
        builder
            .start_element("out", target_uri, "", target_declarations)
            .expect("starts");
        builder.end_of_attributes();
        builder.copy_subtree(&node, opts).expect("copies");
        builder.end_element().expect("ends");
        xml_of(&builder.finalize().expect("finalizes"))
    }

    // ── Identity ──────────────────────────────────────────────────────

    #[test]
    fn a_copy_is_a_new_node_with_the_same_content() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = parse(r#"<r><a k="1">t<b/></a></r>"#, &arena, &names);
        let source = element_at(&doc, &[0, 0]);

        let mut builder = new_builder(&arena, &names);
        builder
            .copy_subtree(&source, CopyOptions::default())
            .unwrap();
        let copied = builder.finalize().unwrap();
        let copy = element_at(&copied, &[0]);

        assert!(!source.is_same_position(&copy), "the copy is a new node");
        assert_eq!(source.local_name(), copy.local_name());
        assert!(
            TreeComparer::new().deep_equal(&source, &copy),
            "the copy has the same content",
        );
        assert_eq!(xml_of(&copied), r#"<a k="1">t<b/></a>"#);
    }

    /// A copied subtree is a new tree, and it carries its `xml:id`s: the copy
    /// goes through `BufferDocumentBuilder::attribute`, which is the one place
    /// an id is registered.
    #[test]
    fn a_copied_subtree_carries_its_ids() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = parse(r#"<r><d xml:id="keep"><t>x</t></d></r>"#, &arena, &names);
        let source = element_at(&doc, &[0, 0]);
        assert_eq!(source.local_name(), "d");

        let mut builder = new_builder(&arena, &names);
        builder
            .copy_subtree(&source, CopyOptions::default())
            .unwrap();
        let copied = builder.finalize().unwrap();

        let found = copied
            .get_element_by_id("keep")
            .expect("the copy has the id");
        let mut nav = copied.create_navigator();
        assert!(nav.move_to_first_child());
        assert_eq!(nav.current_ref(), found, "the id points at the copy's root");
    }

    // ── Content rules ─────────────────────────────────────────────────

    /// Appends `items` to a `<p>` element and returns the serialized result.
    fn content_of(items: &[XmlItem<BufferDocNavigator<'_>>]) -> String {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = new_builder(&arena, &names);
        builder.start_element("p", "", "", &[]).unwrap();
        builder.end_of_attributes();
        builder
            .append_content(items, CopyOptions::default())
            .unwrap();
        builder.end_element().unwrap();
        xml_of(&builder.finalize().unwrap())
    }

    #[test]
    fn two_atomic_values_are_joined_with_one_space() {
        let items = vec![
            XmlItem::Atomic(XmlValue::string("a")),
            XmlItem::Atomic(XmlValue::string("b")),
        ];
        assert_eq!(content_of(&items), "<p>a b</p>");
    }

    #[test]
    fn an_atomic_value_next_to_a_node_gets_no_separator() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = parse("<r><x/></r>", &arena, &names);
        let items = vec![
            XmlItem::Atomic(XmlValue::string("a")),
            XmlItem::Node(element_at(&doc, &[0, 0])),
            XmlItem::Atomic(XmlValue::string("b")),
        ];
        assert_eq!(content_of(&items), "<p>a<x/>b</p>");
    }

    #[test]
    fn the_atomic_run_survives_across_two_calls() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = new_builder(&arena, &names);
        builder.start_element("p", "", "", &[]).unwrap();
        builder.end_of_attributes();
        let first: Vec<XmlItem<BufferDocNavigator<'_>>> =
            vec![XmlItem::Atomic(XmlValue::string("a"))];
        let second: Vec<XmlItem<BufferDocNavigator<'_>>> =
            vec![XmlItem::Atomic(XmlValue::string("b"))];
        builder
            .append_content(&first, CopyOptions::default())
            .unwrap();
        builder
            .append_content(&second, CopyOptions::default())
            .unwrap();
        builder.end_element().unwrap();
        assert_eq!(xml_of(&builder.finalize().unwrap()), "<p>a b</p>");
    }

    #[test]
    fn explicit_text_is_not_an_atomic_item() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = new_builder(&arena, &names);
        builder.start_element("p", "", "", &[]).unwrap();
        builder.end_of_attributes();
        builder.text("a");
        builder.append_atomic(&XmlValue::string("b")).unwrap();
        builder.end_element().unwrap();
        assert_eq!(xml_of(&builder.finalize().unwrap()), "<p>ab</p>");
    }

    #[test]
    fn a_text_node_in_the_sequence_breaks_the_atomic_run() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = parse("<r>t</r>", &arena, &names);
        let mut text = doc.create_navigator();
        assert!(text.move_to_first_child());
        assert!(text.move_to_first_child());
        assert_eq!(text.node_type(), DomNodeType::Text);
        let items = vec![
            XmlItem::Atomic(XmlValue::string("a")),
            XmlItem::Node(text),
            XmlItem::Atomic(XmlValue::string("b")),
        ];
        assert_eq!(content_of(&items), "<p>atb</p>");
    }

    #[test]
    fn a_document_node_contributes_its_children() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = parse("<?work now?><r><a/></r><!--c-->", &arena, &names);
        let items = vec![XmlItem::Node(doc.create_navigator())];
        assert_eq!(content_of(&items), "<p><?work now?><r><a/></r><!--c--></p>",);
    }

    #[test]
    fn an_attribute_after_a_child_is_refused() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = parse(r#"<r k="v"/>"#, &arena, &names);
        let mut attr = element_at(&doc, &[0]);
        assert!(attr.move_to_first_attribute());

        let mut builder = new_builder(&arena, &names);
        builder.start_element("out", "", "", &[]).unwrap();
        builder.end_of_attributes();
        builder.text("already here");
        let items = vec![XmlItem::Node(attr)];
        match builder.append_content(&items, CopyOptions::default()) {
            Err(CopyError::AttributeAfterContent { name }) => assert_eq!(name, "k"),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn an_attribute_at_document_level_is_refused() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = parse(r#"<r p:k="v" xmlns:p="urn:x"/>"#, &arena, &names);
        let mut attr = element_at(&doc, &[0]);
        assert!(attr.move_to_first_attribute());

        let mut builder = new_builder(&arena, &names);
        match builder.copy_attribute(&attr, CopyOptions::default()) {
            Err(CopyError::AttributeOutsideElement { name }) => assert_eq!(name, "p:k"),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn the_later_of_two_duplicate_attributes_wins() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = parse(r#"<r><a k="1"/><b k="2"/></r>"#, &arena, &names);
        let mut first = element_at(&doc, &[0, 0]);
        assert!(first.move_to_first_attribute());
        let mut second = element_at(&doc, &[0, 1]);
        assert!(second.move_to_first_attribute());

        let mut builder = new_builder(&arena, &names);
        builder.start_element("out", "", "", &[]).unwrap();
        builder
            .copy_attribute(&first, CopyOptions::default())
            .unwrap();
        builder
            .copy_attribute(&second, CopyOptions::default())
            .unwrap();
        builder.end_of_attributes();
        builder.end_element().unwrap();
        assert_eq!(xml_of(&builder.finalize().unwrap()), r#"<out k="2"/>"#);
    }

    #[test]
    fn an_attribute_can_follow_end_of_attributes() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = parse(r#"<r a="1" b="2"/>"#, &arena, &names);
        let mut attr = element_at(&doc, &[0]);
        assert!(attr.move_to_first_attribute());
        assert!(attr.move_to_next_attribute());

        let mut builder = new_builder(&arena, &names);
        builder.start_element("out", "", "", &[]).unwrap();
        builder.attribute("a", "", "", "1").unwrap();
        builder.end_of_attributes();
        // Still legal: no child has been added yet.
        builder
            .copy_attribute(&attr, CopyOptions::default())
            .unwrap();
        builder.end_element().unwrap();
        assert_eq!(
            xml_of(&builder.finalize().unwrap()),
            r#"<out a="1" b="2"/>"#
        );
    }

    // ── Namespaces ────────────────────────────────────────────────────

    #[test]
    fn a_copy_carries_a_declaration_it_inherited() {
        // `p` is declared on the source's document element, not on the copied
        // node — the copy has to carry it, or the serializer would refuse the
        // name (`SerializeError::UnboundName`).
        assert_eq!(
            copy_into(
                r#"<r xmlns:p="urn:x"><p:a k="1"/></r>"#,
                &[0, 0],
                &[],
                "",
                CopyOptions::default(),
            ),
            r#"<out><p:a xmlns:p="urn:x" k="1"/></out>"#,
        );
    }

    #[test]
    fn preserve_keeps_an_unused_declaration() {
        assert_eq!(
            copy_into(
                r#"<r xmlns:p="urn:x" xmlns:unused="urn:u"><p:a/></r>"#,
                &[0],
                &[],
                "",
                CopyOptions::default(),
            ),
            r#"<out><r xmlns:p="urn:x" xmlns:unused="urn:u"><p:a/></r></out>"#,
        );
    }

    #[test]
    fn no_preserve_declares_only_what_is_needed() {
        let opts = CopyOptions {
            namespaces: CopyNamespaces::NoPreserve,
            ..CopyOptions::default()
        };
        assert_eq!(
            copy_into(
                r#"<r xmlns:p="urn:x" xmlns:unused="urn:u"><p:a/></r>"#,
                &[0],
                &[],
                "",
                opts,
            ),
            r#"<out><r><p:a xmlns:p="urn:x"/></r></out>"#,
        );
    }

    #[test]
    fn without_inherit_the_copy_redeclares_what_it_needs() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = parse(r#"<r xmlns:p="urn:x"><p:a/></r>"#, &arena, &names);
        let node = element_at(&doc, &[0, 0]);

        // The target already binds `p` to the same URI, so an inheriting copy
        // needs no declaration of its own.
        let mut inheriting = new_builder(&arena, &names);
        inheriting
            .start_element("out", "", "", &[("p", "urn:x")])
            .unwrap();
        inheriting.end_of_attributes();
        inheriting
            .copy_subtree(&node, CopyOptions::default())
            .unwrap();
        inheriting.end_element().unwrap();
        let inherited = inheriting.finalize().unwrap();
        assert!(
            local_declarations(&element_at(&inherited, &[0, 0])).is_empty(),
            "an inheriting copy leans on the scope it landed in",
        );

        let opts = CopyOptions {
            inherit: false,
            ..CopyOptions::default()
        };
        let mut standalone = new_builder(&arena, &names);
        standalone
            .start_element("out", "", "", &[("p", "urn:x")])
            .unwrap();
        standalone.end_of_attributes();
        standalone.copy_subtree(&node, opts).unwrap();
        standalone.end_element().unwrap();
        let alone = standalone.finalize().unwrap();
        assert_eq!(
            local_declarations(&element_at(&alone, &[0, 0])),
            vec![("p".to_string(), "urn:x".to_string())],
            "without inherit the declaration is re-emitted on the copy",
        );

        // Either way the names resolve, so the output is the same: a
        // re-declaration of what is already in scope is redundant.
        assert_eq!(xml_of(&inherited), xml_of(&alone));
        assert_eq!(xml_of(&alone), r#"<out xmlns:p="urn:x"><p:a/></out>"#);
    }

    #[test]
    fn a_conflicting_attribute_prefix_is_generated() {
        let opts = CopyOptions {
            namespaces: CopyNamespaces::NoPreserve,
            ..CopyOptions::default()
        };
        assert_eq!(
            copy_into(
                r#"<r xmlns:p="urn:x"><a p:k="v"/></r>"#,
                &[0, 0],
                &[("p", "urn:other")],
                "",
                opts,
            ),
            r#"<out xmlns:p="urn:other"><a xmlns:ns0="urn:x" ns0:k="v"/></out>"#,
        );
    }

    #[test]
    fn a_namespaced_attribute_without_a_prefix_gets_one() {
        let arena = Bump::new();
        let names = NameTable::new();

        // A parser cannot produce this — an unprefixed attribute is in no
        // namespace — so the source is built by hand.
        let mut source = new_builder(&arena, &names);
        source.start_element("a", "", "", &[]).unwrap();
        source.attribute("k", "urn:a", "", "v").unwrap();
        source.end_of_attributes();
        source.end_element().unwrap();
        let doc = source.finalize().unwrap();

        let mut builder = new_builder(&arena, &names);
        builder.start_element("out", "", "", &[]).unwrap();
        builder.end_of_attributes();
        builder
            .copy_subtree(&element_at(&doc, &[0]), CopyOptions::default())
            .unwrap();
        builder.end_element().unwrap();
        assert_eq!(
            xml_of(&builder.finalize().unwrap()),
            r#"<out><a xmlns:ns0="urn:a" ns0:k="v"/></out>"#,
        );
    }

    #[test]
    fn a_copied_attribute_brings_its_declaration_along() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = parse(r#"<r xmlns:p="urn:x" p:k="v"/>"#, &arena, &names);
        let mut attr = element_at(&doc, &[0]);
        assert!(attr.move_to_first_attribute());

        // The open element binds `p` elsewhere, so the copied attribute takes
        // a generated prefix, declared on the element it lands on.
        let mut builder = new_builder(&arena, &names);
        builder
            .start_element("out", "", "", &[("p", "urn:other")])
            .unwrap();
        builder
            .copy_attribute(&attr, CopyOptions::default())
            .unwrap();
        builder.end_of_attributes();
        builder.end_element().unwrap();
        assert_eq!(
            xml_of(&builder.finalize().unwrap()),
            r#"<out xmlns:ns0="urn:x" xmlns:p="urn:other" ns0:k="v"/>"#,
        );
    }

    #[test]
    fn an_unprefixed_element_undeclares_an_inherited_default_namespace() {
        assert_eq!(
            copy_into(
                "<r><a/></r>",
                &[0, 0],
                &[("", "urn:d")],
                "urn:d",
                CopyOptions::default(),
            ),
            r#"<out xmlns="urn:d"><a xmlns=""/></out>"#,
        );
    }

    #[test]
    fn a_default_namespace_is_carried_over_as_a_default() {
        assert_eq!(
            copy_into(
                r#"<r xmlns="urn:d"><a/></r>"#,
                &[0, 0],
                &[],
                "",
                CopyOptions::default(),
            ),
            r#"<out><a xmlns="urn:d"/></out>"#,
        );
    }

    #[test]
    fn the_xml_prefix_is_never_declared() {
        assert_eq!(
            copy_into(
                r#"<r><a xml:lang="en"/></r>"#,
                &[0, 0],
                &[],
                "",
                CopyOptions::default(),
            ),
            r#"<out><a xml:lang="en"/></out>"#,
        );
    }

    #[test]
    fn both_navigator_backends_copy_alike() {
        let xml = r#"<r xmlns:p="urn:x" xmlns="urn:d"><p:a k="1">t<!--c--><?pi d?></p:a></r>"#;

        let arena = Bump::new();
        let names = NameTable::new();
        let doc = parse(xml, &arena, &names);
        let mut builder = new_builder(&arena, &names);
        builder
            .copy_subtree(&element_at(&doc, &[0, 0]), CopyOptions::default())
            .unwrap();
        let from_buffer = xml_of(&builder.finalize().unwrap());

        let ro_doc = roxmltree::Document::parse(xml).expect("the fixture parses");
        let mut ro_nav = RoXmlNavigator::new(&ro_doc);
        assert!(ro_nav.move_to_first_child());
        assert!(ro_nav.move_to_first_child());
        let mut ro_builder = new_builder(&arena, &names);
        ro_builder
            .copy_subtree(&ro_nav, CopyOptions::default())
            .unwrap();
        let from_roxmltree = xml_of(&ro_builder.finalize().unwrap());

        assert_eq!(from_buffer, from_roxmltree);
        assert_eq!(
            from_buffer,
            r#"<p:a xmlns:p="urn:x" k="1">t<!--c--><?pi d?></p:a>"#,
        );
    }

    #[test]
    fn a_namespace_node_is_not_content() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = parse(r#"<r xmlns:p="urn:x"/>"#, &arena, &names);
        let mut ns = element_at(&doc, &[0]);
        assert!(ns.move_to_first_namespace(NamespaceAxisScope::Local));

        let mut builder = new_builder(&arena, &names);
        match builder.copy_subtree(&ns, CopyOptions::default()) {
            Err(CopyError::Unsupported(_)) => {}
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    // ── Annotations ───────────────────────────────────────────────────

    const NILLABLE_SCHEMA: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
        <xs:element name="r">
          <xs:complexType>
            <xs:sequence>
              <xs:element name="v" type="xs:int" nillable="true"/>
            </xs:sequence>
            <xs:attribute name="k" type="xs:int"/>
          </xs:complexType>
        </xs:element>
      </xs:schema>"#;

    const NILLABLE_INSTANCE: &str =
        r#"<r k="7"><v xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:nil="true"/></r>"#;

    fn load_schema() -> SchemaSet {
        let mut schema_set = SchemaSet::xsd11();
        crate::pipeline::load_and_process_schema(
            NILLABLE_SCHEMA.as_bytes(),
            "test.xsd",
            &mut schema_set,
            None,
        )
        .expect("the schema loads");
        schema_set
    }

    fn preserving() -> CopyOptions {
        CopyOptions {
            annotations: Annotations::Preserve,
            ..CopyOptions::default()
        }
    }

    #[test]
    fn annotations_are_preserved_within_one_schema_set() {
        let schema_set = load_schema();
        let arena = Bump::new();
        let names = NameTable::new();
        let source = crate::document::build_typed_document(
            NILLABLE_INSTANCE.as_bytes(),
            &arena,
            &schema_set,
            BufferDocumentOptions::default(),
        )
        .expect("the instance builds");

        // The source really is typed, or the test would prove nothing.
        let root = element_at(&source, &[0]);
        assert!(root.schema_binding().is_some(), "the source is typed");

        let mut builder = BufferDocumentBuilder::new(
            &arena,
            &names,
            Some(&schema_set),
            BufferDocumentOptions::default(),
        )
        .expect("a builder");
        builder.copy_subtree(&root, preserving()).unwrap();
        let copied = builder.finalize().unwrap();

        let copy = element_at(&copied, &[0]);
        assert_eq!(
            copy.element_type_key(),
            root.element_type_key(),
            "the element keeps its type annotation",
        );
        let mut attr = copy.clone();
        assert!(attr.move_to_first_attribute());
        assert_eq!(attr.local_name(), "k");
        assert!(attr.schema_type().is_some(), "the attribute stays typed");
        let child = element_at(&copied, &[0, 0]);
        assert_eq!(child.typed_value(), TypedValue::Nilled, "nil is replayed");
    }

    #[test]
    fn annotations_are_stripped_by_default() {
        let schema_set = load_schema();
        let arena = Bump::new();
        let names = NameTable::new();
        let source = crate::document::build_typed_document(
            NILLABLE_INSTANCE.as_bytes(),
            &arena,
            &schema_set,
            BufferDocumentOptions::default(),
        )
        .expect("the instance builds");

        let mut builder = BufferDocumentBuilder::new(
            &arena,
            &names,
            Some(&schema_set),
            BufferDocumentOptions::default(),
        )
        .expect("a builder");
        builder
            .copy_subtree(&element_at(&source, &[0]), CopyOptions::default())
            .unwrap();
        let copied = builder.finalize().unwrap();

        let copy = element_at(&copied, &[0]);
        assert!(copy.schema_binding().is_none(), "no annotation is carried");
        assert_eq!(
            element_at(&copied, &[0, 0]).typed_value(),
            TypedValue::Untyped,
            "and no nil flag either",
        );
    }

    #[test]
    fn annotations_from_another_schema_set_are_refused() {
        let source_set = load_schema();
        let target_set = load_schema();
        let arena = Bump::new();
        let names = NameTable::new();
        let source = crate::document::build_typed_document(
            NILLABLE_INSTANCE.as_bytes(),
            &arena,
            &source_set,
            BufferDocumentOptions::default(),
        )
        .expect("the instance builds");

        let mut builder = BufferDocumentBuilder::new(
            &arena,
            &names,
            Some(&target_set),
            BufferDocumentOptions::default(),
        )
        .expect("a builder");
        match builder.copy_subtree(&element_at(&source, &[0]), preserving()) {
            Err(CopyError::SchemaMismatch) => {}
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn an_untyped_source_copies_untyped_even_with_preserve() {
        let schema_set = load_schema();
        let arena = Bump::new();
        let names = NameTable::new();
        // Parsed without a schema set: there is nothing to preserve, so this
        // is not a mismatch.
        let source = parse(r#"<r k="7"/>"#, &arena, &names);

        let mut builder = BufferDocumentBuilder::new(
            &arena,
            &names,
            Some(&schema_set),
            BufferDocumentOptions::default(),
        )
        .expect("a builder");
        builder
            .copy_subtree(&element_at(&source, &[0]), preserving())
            .unwrap();
        let copied = builder.finalize().unwrap();
        assert!(element_at(&copied, &[0]).schema_binding().is_none());
        assert_eq!(xml_of(&copied), r#"<r k="7"/>"#);
    }

    // ── NamespaceFixup on its own ─────────────────────────────────────

    #[test]
    fn in_scope_reads_the_empty_prefix_and_xml_specially() {
        let mut fixup = NamespaceFixup::new();
        assert_eq!(fixup.in_scope(""), Some(""));
        assert_eq!(fixup.in_scope("xml"), Some(XML_NAMESPACE));
        assert_eq!(fixup.in_scope("p"), None);

        fixup.push_scope(&[
            (String::new(), "urn:d".to_string()),
            ("p".to_string(), "urn:x".to_string()),
        ]);
        assert_eq!(fixup.in_scope(""), Some("urn:d"));
        assert_eq!(fixup.in_scope("p"), Some("urn:x"));

        // An inner scope shadows, and popping restores.
        fixup.push_scope(&[("p".to_string(), "urn:y".to_string())]);
        assert_eq!(fixup.in_scope("p"), Some("urn:y"));
        fixup.pop_scope();
        assert_eq!(fixup.in_scope("p"), Some("urn:x"));
    }

    #[test]
    fn generated_prefixes_skip_the_ones_already_taken() {
        let mut fixup = NamespaceFixup::new();
        fixup.push_scope(&[
            ("ns0".to_string(), "urn:zero".to_string()),
            ("p".to_string(), "urn:taken".to_string()),
        ]);

        // Two attributes needing a prefix: `p` is taken by another URI, and
        // `ns0` is in scope, so the generated names start at `ns1`.
        let (decls, attrs) =
            fixup.declarations_for(("", ""), &[("p", "urn:a"), ("", "urn:b")], &[]);
        assert_eq!(
            decls,
            vec![
                ("ns1".to_string(), "urn:a".to_string()),
                ("ns2".to_string(), "urn:b".to_string()),
            ],
        );
        assert_eq!(
            attrs,
            vec![
                ("ns1".to_string(), "urn:a".to_string()),
                ("ns2".to_string(), "urn:b".to_string()),
            ],
        );
    }

    #[test]
    fn the_elements_own_binding_wins_over_a_carried_one() {
        let mut fixup = NamespaceFixup::new();
        let (decls, _) = fixup.declarations_for(
            ("p", "urn:element"),
            &[],
            &[("p", "urn:carried"), ("q", "urn:kept")],
        );
        assert_eq!(
            decls,
            vec![
                ("p".to_string(), "urn:element".to_string()),
                ("q".to_string(), "urn:kept".to_string()),
            ],
        );
    }
}
