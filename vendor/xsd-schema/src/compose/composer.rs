//! The composer: an arena, a name table, a namespace context, and a cache of
//! compiled expressions.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::io::{self, BufRead, BufReader};
use std::path::Path;
use std::rc::Rc;

use bumpalo::Bump;

use crate::document::{
    serialize, BufferDocument, BufferDocumentBuilder, BufferDocumentOptions, CopyOptions,
    SerializeOptions,
};
use crate::namespace::{NameTable, NamespaceContextSnapshot, XSI_NAMESPACE, XS_NAMESPACE};
use crate::navigator::{DomNavigator, DomNodeType};
use crate::xpath::functions::FN_NAMESPACE;
use crate::xpath::{RegexCache, XPathContext, XPathExpr, XPathValue};
use crate::SchemaSet;

use super::emit::Emitter;
use super::form::Form;
use super::{ComposeError, Nav, Value};

/// What a compiled expression is cached under: the expression text and the
/// external variable names it was compiled with.
///
/// Both halves are compared and hashed by their *content*, not by where the
/// literals happen to live, so two call sites spelling the same expression
/// with the same variables share one compilation.
#[derive(Clone, Copy)]
struct ExprKey {
    source: &'static str,
    vars: &'static [&'static str],
}

impl PartialEq for ExprKey {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source && self.vars == other.vars
    }
}

impl Eq for ExprKey {}

impl Hash for ExprKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.source.hash(state);
        self.vars.hash(state);
    }
}

/// A finished document, held in the composer's arena.
///
/// `Doc` is a `Copy` handle: passing it around costs nothing and every
/// navigator taken from it shares the composer's lifetime, so a node of one
/// document can be spliced into another without any lifetime work.
///
/// ```
/// use bumpalo::Bump;
/// use xsd_schema::compose::Composer;
/// use xsd_schema::document::SerializeOptions;
/// use xsd_schema::namespace::NameTable;
/// use xsd_schema::navigator::DomNavigator;
///
/// let arena = Bump::new();
/// let names = NameTable::new();
/// let c = Composer::new(&arena, &names);
///
/// let doc = c.load_str("<!--first--><a><b/></a>")?;
/// assert_eq!(doc.children().len(), 2);
/// assert_eq!(doc.document_element().unwrap().local_name(), "a");
/// assert_eq!(doc.to_xml(&SerializeOptions::default())?, "<!--first--><a><b/></a>");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Copy)]
pub struct Doc<'a>(&'a BufferDocument<'a>);

impl<'a> Doc<'a> {
    /// A navigator on the document node.
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
    /// let doc = c.load_str("<a><b/></a>")?;
    ///
    /// use xsd_schema::navigator::{DomNavigator, DomNodeType};
    /// assert_eq!(doc.root().node_type(), DomNodeType::Root);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn root(&self) -> Nav<'a> {
        self.0.create_navigator()
    }

    /// The first top-level element, if the document has one.
    ///
    /// A document built with
    /// [`build_sequence`](Composer::build_sequence) may have none, or several;
    /// use [`children`](Self::children) for those.
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
    /// let doc = c.load_str("<!--lead--><a/>")?;
    ///
    /// use xsd_schema::navigator::DomNavigator;
    /// assert_eq!(doc.document_element().unwrap().local_name(), "a");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn document_element(&self) -> Option<Nav<'a>> {
        self.children()
            .into_iter()
            .find(|node| node.node_type() == DomNodeType::Element)
    }

    /// Every top-level node, in document order.
    ///
    /// This is how a sequence of elements becomes queryable: build it with
    /// [`build_sequence`](Composer::build_sequence), then bind `children()` to
    /// a variable.
    ///
    /// ```
    /// use bumpalo::Bump;
    /// use xsd_schema::compose::{Composer, Content, Form, IntoXPathValue, Name};
    /// use xsd_schema::namespace::NameTable;
    ///
    /// let arena = Bump::new();
    /// let names = NameTable::new();
    /// let c = Composer::new(&arena, &names);
    ///
    /// let mut count = Form::new(Name::local("nbids"));
    /// count.push(Content::Text("2".to_string()));
    /// let mut first = Form::new(Name::local("bid_count"));
    /// first.push(Content::Element(count));
    ///
    /// let mut count = Form::new(Name::local("nbids"));
    /// count.push(Content::Text("5".to_string()));
    /// let mut second = Form::new(Name::local("bid_count"));
    /// second.push(Content::Element(count));
    ///
    /// let seq = c.build_sequence(vec![first, second])?;
    /// let total = c.eval(
    ///     "sum($s/nbids)",
    ///     &["s"],
    ///     None,
    ///     vec![("s", seq.children().into_xpath_value())],
    /// )?;
    /// assert_eq!(total.string()?, "7");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn children(&self) -> Vec<Nav<'a>> {
        let mut out = Vec::new();
        let mut node = self.root();
        if node.move_to_first_child() {
            loop {
                out.push(node.clone());
                if !node.move_to_next_sibling() {
                    break;
                }
            }
        }
        out
    }

    /// Writes the document as XML.
    ///
    /// ```
    /// use bumpalo::Bump;
    /// use xsd_schema::compose::Composer;
    /// use xsd_schema::document::SerializeOptions;
    /// use xsd_schema::namespace::NameTable;
    ///
    /// let arena = Bump::new();
    /// let names = NameTable::new();
    /// let c = Composer::new(&arena, &names);
    /// let doc = c.load_str("<a><b>1</b></a>")?;
    ///
    /// // Compact by default; `indent` asks for layout instead.
    /// assert_eq!(doc.to_xml(&SerializeOptions::default())?, "<a><b>1</b></a>");
    /// let formatted = SerializeOptions { indent: Some(2), ..SerializeOptions::default() };
    /// assert_eq!(doc.to_xml(&formatted)?, "<a>\n  <b>1</b>\n</a>");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn to_xml(&self, opts: &SerializeOptions) -> Result<String, ComposeError> {
        Ok(serialize::to_string(&self.root(), opts)?)
    }

    /// Writes the document as XML into `writer`, without an intermediate
    /// `String`.
    ///
    /// ```
    /// use bumpalo::Bump;
    /// use xsd_schema::compose::Composer;
    /// use xsd_schema::document::SerializeOptions;
    /// use xsd_schema::namespace::NameTable;
    ///
    /// let arena = Bump::new();
    /// let names = NameTable::new();
    /// let c = Composer::new(&arena, &names);
    /// let doc = c.load_str("<a/>")?;
    ///
    /// let mut out: Vec<u8> = Vec::new();
    /// doc.write_xml(&mut out, &SerializeOptions::default())?;
    /// assert_eq!(String::from_utf8(out)?, "<a/>");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn write_xml<W: io::Write>(
        &self,
        writer: W,
        opts: &SerializeOptions,
    ) -> Result<(), ComposeError> {
        Ok(serialize::serialize_document(&self.root(), writer, opts)?)
    }

    /// The underlying document.
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
    /// let doc = c.load_str("<a/>")?;
    ///
    /// // No schema set was configured, so the document is bound to none.
    /// assert!(doc.inner().schema_set().is_none());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn inner(&self) -> &'a BufferDocument<'a> {
        self.0
    }
}

impl fmt::Debug for Doc<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Doc")
            .field("top_level_nodes", &self.children().len())
            .finish()
    }
}

/// The prefixes a composer starts with bound, in declaration order.
///
/// An XQuery processor has `xs`, `xsi`, `fn`, `local` and `xml` in scope
/// before the first character of a query is read, and an expression that casts
/// — `xs:date('1999-01-31')` — is unwritable without at least the first of
/// them. Three of the five are bound here: `local` names a namespace for
/// user-declared functions, and there are none to declare, while `xml` is
/// bound by XML itself and never needs a declaration.
const PREDECLARED_NAMESPACES: [(&str, &str); 3] = [
    ("xs", XS_NAMESPACE),
    ("xsi", XSI_NAMESPACE),
    ("fn", FN_NAMESPACE),
];

/// The home of a composition: one arena, one name table, one namespace
/// context, one expression cache.
///
/// # Lifetime model
///
/// Every document the composer loads or builds is finalized into the arena and
/// handed back as a [`Doc<'a>`], so every navigator, [`Value`] and
/// [`Form`] shares the single lifetime `'a`. Nothing is freed before the
/// composer and its arena go away: the model is one composer per request or
/// per batch.
///
/// # Compile once, evaluate many
///
/// [`eval`](Self::eval) caches the compiled expression under its text and its
/// external variable names, so an expression inside a loop is parsed once and
/// bound per evaluation. Every method takes `&self` — the arena allocates
/// through a shared reference, the name table is interior-mutable, and the
/// cache is a `RefCell` — so a composer can be used freely inside iterator
/// closures and nested loops.
///
/// # Threads
///
/// The name table is not synchronized, so a `Composer` is neither `Send` nor
/// `Sync`: one per thread.
///
/// ```
/// use bumpalo::Bump;
/// use xsd_schema::compose::{Composer, Content, Form, Name};
/// use xsd_schema::document::SerializeOptions;
/// use xsd_schema::namespace::NameTable;
///
/// let arena = Bump::new();
/// let names = NameTable::new();
/// let c = Composer::new(&arena, &names);
///
/// let source = c.load_str("<items><item>bike</item><item>kite</item></items>")?;
/// let count = c.eval("count(//item)", &[], Some(source.root()), Vec::new())?;
///
/// let mut summary = Form::new(Name::local("summary"));
/// summary.push(Content::Text(count.string()?));
///
/// assert_eq!(c.build(summary)?.to_xml(&SerializeOptions::default())?, "<summary>2</summary>");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct Composer<'a> {
    arena: &'a Bump,
    /// The table the caller supplied; a schema set's own table wins over it.
    given_names: &'a NameTable,
    /// The table actually used for interning, evaluation and building.
    names: &'a NameTable,
    schema_set: Option<&'a SchemaSet>,
    /// The prefix bindings, in the order they were added: the predeclared
    /// ones first, then the caller's. A later binding for a prefix wins.
    namespaces: Vec<(String, String)>,
    default_element_ns: Option<String>,
    base_uri: Option<String>,
    ctx: XPathContext<'a>,
    exprs: RefCell<HashMap<ExprKey, Rc<XPathExpr>>>,
    /// The regular expressions `fn:matches`, `fn:replace` and `fn:tokenize`
    /// compiled by any evaluation of this composer, lent to every evaluation —
    /// so an `xpath!` in a loop compiles its pattern once, not once per call.
    /// Independent of the static context, so `rebuild_context` keeps it.
    regexes: RegexCache,
    docs: RefCell<Vec<&'a BufferDocument<'a>>>,
    copy: CopyOptions,
}

impl<'a> Composer<'a> {
    /// A composer over `arena` and `names`.
    ///
    /// # Predeclared prefixes
    ///
    /// Three prefixes are bound before you bind anything:
    ///
    /// | Prefix | Namespace |
    /// | --- | --- |
    /// | `xs` | `http://www.w3.org/2001/XMLSchema` |
    /// | `xsi` | `http://www.w3.org/2001/XMLSchema-instance` |
    /// | `fn` | `http://www.w3.org/2005/xpath-functions` |
    ///
    /// They are the ones every processor of this expression language has in
    /// scope from the start, and without `xs` no cast can be written at all:
    /// `xs:date('1999-01-31')` would be `XPST0081 Prefix 'xs' cannot be
    /// expanded`. The prefixes work everywhere a prefix works — in an
    /// expression, and in a [`form!`](crate::form!) element or attribute name
    /// — and a prefix a form actually uses is declared on the output like any
    /// other. Two more prefixes are predeclared by XQuery 1.0 §4.12 and not
    /// here: `local`, which names the namespace of user-declared functions
    /// (there are none to declare), and `xml`, which XML binds itself and
    /// which is never declared. Any of the three can be rebound with
    /// [`with_namespace`](Self::with_namespace).
    ///
    /// ```
    /// use bumpalo::Bump;
    /// use xsd_schema::compose::Composer;
    /// use xsd_schema::namespace::NameTable;
    ///
    /// let arena = Bump::new();
    /// let names = NameTable::new();
    /// let c = Composer::new(&arena, &names);
    /// assert_eq!(c.eval("1 + 1", &[], None, Vec::new())?.string()?, "2");
    ///
    /// // `xs` needs no declaration of your own.
    /// assert_eq!(
    ///     c.eval("month-from-date(xs:date('1999-01-31'))", &[], None, Vec::new())?.string()?,
    ///     "1",
    /// );
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn new(arena: &'a Bump, names: &'a NameTable) -> Self {
        let mut composer = Self {
            arena,
            given_names: names,
            names,
            schema_set: None,
            namespaces: PREDECLARED_NAMESPACES
                .iter()
                .map(|(prefix, uri)| (prefix.to_string(), uri.to_string()))
                .collect(),
            default_element_ns: None,
            base_uri: None,
            ctx: XPathContext::new(names),
            exprs: RefCell::new(HashMap::new()),
            regexes: RegexCache::new(),
            docs: RefCell::new(Vec::new()),
            copy: CopyOptions::default(),
        };
        composer.rebuild_context();
        composer
    }

    /// Uses `schema_set` for type information, and its name table for
    /// interning.
    ///
    /// Call this before anything else: it changes the table every name is
    /// interned in, so the namespace bindings are re-interned and the
    /// expression cache is dropped.
    ///
    /// ```
    /// use bumpalo::Bump;
    /// use xsd_schema::compose::Composer;
    /// use xsd_schema::namespace::NameTable;
    /// use xsd_schema::{load_and_process_schema, SchemaSet};
    ///
    /// let mut schema_set = SchemaSet::new();
    /// let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
    ///     <xs:element name="n" type="xs:int"/>
    /// </xs:schema>"#;
    /// load_and_process_schema(xsd.as_bytes(), "s.xsd", &mut schema_set, None)
    ///     .expect("the schema loads");
    ///
    /// let arena = Bump::new();
    /// let names = NameTable::new();
    /// let c = Composer::new(&arena, &names).with_schema_set(&schema_set);
    ///
    /// // Everything now interns in the schema set's own table.
    /// let doc = c.load_str("<n>42</n>")?;
    /// assert!(std::ptr::eq(doc.inner().names(), &schema_set.name_table));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn with_schema_set(mut self, schema_set: &'a SchemaSet) -> Self {
        self.schema_set = Some(schema_set);
        self.rebuild_context();
        self
    }

    /// Binds a prefix, for `$p:x` in an expression and `p:x` in a form.
    ///
    /// `xs`, `xsi` and `fn` are bound already ([`new`](Self::new)); this
    /// rebinds any of them, because the last declaration of a prefix wins.
    ///
    /// ```
    /// use bumpalo::Bump;
    /// use xsd_schema::compose::Composer;
    /// use xsd_schema::namespace::NameTable;
    ///
    /// let arena = Bump::new();
    /// let names = NameTable::new();
    /// let c = Composer::new(&arena, &names).with_namespace("h", "urn:h");
    /// let doc = c.load_str(r#"<r xmlns:x="urn:h"><x:a/></r>"#)?;
    ///
    /// assert_eq!(
    ///     c.eval("count(//h:a)", &[], Some(doc.root()), Vec::new())?.string()?,
    ///     "1",
    /// );
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn with_namespace(mut self, prefix: &str, uri: &str) -> Self {
        self.namespaces.push((prefix.to_string(), uri.to_string()));
        self.rebuild_context();
        self
    }

    /// Sets the namespace an unprefixed element name takes, both in
    /// expressions and in forms.
    ///
    /// ```
    /// use bumpalo::Bump;
    /// use xsd_schema::compose::{Composer, Form, Name};
    /// use xsd_schema::document::SerializeOptions;
    /// use xsd_schema::namespace::NameTable;
    ///
    /// let arena = Bump::new();
    /// let names = NameTable::new();
    /// let c = Composer::new(&arena, &names).with_default_element_namespace("urn:d");
    ///
    /// // In a form …
    /// let built = c.build(Form::new(Name::local("root")))?;
    /// assert_eq!(built.to_xml(&SerializeOptions::default())?, r#"<root xmlns="urn:d"/>"#);
    ///
    /// // … and in an expression.
    /// let source = c.load_str(r#"<r xmlns="urn:d"><a/></r>"#)?;
    /// assert_eq!(
    ///     c.eval("count(//a)", &[], Some(source.root()), Vec::new())?.string()?,
    ///     "1",
    /// );
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn with_default_element_namespace(mut self, uri: &str) -> Self {
        self.default_element_ns = Some(uri.to_string());
        self.rebuild_context();
        self
    }

    /// Sets the static base URI for `fn:resolve-uri` and friends.
    ///
    /// ```
    /// use bumpalo::Bump;
    /// use xsd_schema::compose::Composer;
    /// use xsd_schema::namespace::NameTable;
    ///
    /// let arena = Bump::new();
    /// let names = NameTable::new();
    /// let c = Composer::new(&arena, &names).with_base_uri("http://example.com/here/");
    ///
    /// assert_eq!(
    ///     c.eval("string(resolve-uri('there'))", &[], None, Vec::new())?.string()?,
    ///     "http://example.com/here/there",
    /// );
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn with_base_uri(mut self, uri: impl Into<String>) -> Self {
        self.base_uri = Some(uri.into());
        self.rebuild_context();
        self
    }

    /// Sets how spliced nodes are copied.
    ///
    /// The default is XQuery's own default construction mode: declarations
    /// preserved, the insertion point's namespaces inherited, annotations
    /// stripped.
    ///
    /// ```
    /// use bumpalo::Bump;
    /// use xsd_schema::compose::{Composer, Content, Form, IntoContent, Name};
    /// use xsd_schema::document::{CopyNamespaces, CopyOptions, SerializeOptions};
    /// use xsd_schema::namespace::NameTable;
    ///
    /// let arena = Bump::new();
    /// let names = NameTable::new();
    /// let c = Composer::new(&arena, &names).with_copy_options(CopyOptions {
    ///     namespaces: CopyNamespaces::NoPreserve,
    ///     ..CopyOptions::default()
    /// });
    ///
    /// // The source declares a namespace its own name does not use.
    /// let source = c.load_str(r#"<a><b xmlns:unused="urn:u"/></a>"#)?;
    /// let b = c.eval("//b", &[], Some(source.root()), Vec::new())?;
    ///
    /// let mut out = Form::new(Name::local("out"));
    /// out.push(b.into_content());
    /// assert_eq!(c.build(out)?.to_xml(&SerializeOptions::default())?, "<out><b/></out>");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn with_copy_options(mut self, opts: CopyOptions) -> Self {
        self.copy = opts;
        self
    }

    // ── Loading ───────────────────────────────────────────────────────

    /// Parses XML held in a string.
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
    /// let doc = c.load_str("<a><b>1</b></a>")?;
    ///
    /// assert_eq!(
    ///     c.eval("string(//b)", &[], Some(doc.root()), Vec::new())?.string()?,
    ///     "1",
    /// );
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn load_str(&self, xml: &str) -> Result<Doc<'a>, ComposeError> {
        self.load_reader(xml.as_bytes())
    }

    /// Parses XML from a reader.
    ///
    /// ```
    /// use std::io::Cursor;
    ///
    /// use bumpalo::Bump;
    /// use xsd_schema::compose::Composer;
    /// use xsd_schema::document::SerializeOptions;
    /// use xsd_schema::namespace::NameTable;
    ///
    /// let arena = Bump::new();
    /// let names = NameTable::new();
    /// let c = Composer::new(&arena, &names);
    ///
    /// let doc = c.load_reader(Cursor::new(b"<a/>"))?;
    /// assert_eq!(doc.to_xml(&SerializeOptions::default())?, "<a/>");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn load_reader<R: BufRead>(&self, reader: R) -> Result<Doc<'a>, ComposeError> {
        let doc = BufferDocument::from_reader(
            reader,
            self.arena,
            self.names,
            BufferDocumentOptions::default(),
            self.schema_set,
        )?;
        Ok(self.store(doc))
    }

    /// Parses XML from a file.
    ///
    /// ```no_run
    /// use bumpalo::Bump;
    /// use xsd_schema::compose::Composer;
    /// use xsd_schema::namespace::NameTable;
    ///
    /// let arena = Bump::new();
    /// let names = NameTable::new();
    /// let c = Composer::new(&arena, &names);
    ///
    /// let items = c.load_file("items.xml")?;
    /// println!("{}", c.eval("count(//item_tuple)", &[], Some(items.root()), Vec::new())?.string()?);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn load_file(&self, path: impl AsRef<Path>) -> Result<Doc<'a>, ComposeError> {
        let file = std::fs::File::open(path.as_ref())?;
        self.load_reader(BufReader::new(file))
    }

    // ── Building ──────────────────────────────────────────────────────

    /// Builds a document whose content is one element.
    ///
    /// A [`Form`] always describes exactly one element, so this is the usual
    /// entry point; [`ComposeError::BuildShape`] reports a build that would
    /// end up with any other number of top-level elements.
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
    /// let doc = c.build(Form::new(Name::local("result")))?;
    /// assert_eq!(doc.to_xml(&SerializeOptions::default())?, "<result/>");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn build(&self, form: Form<'a>) -> Result<Doc<'a>, ComposeError> {
        self.build_shaped(vec![form], true)
    }

    /// Builds a document with any number of top-level elements.
    ///
    /// This is how a Rust function that returns `element()*` becomes
    /// queryable: build the sequence, then bind
    /// [`Doc::children`] to a variable.
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
    /// let empty = c.build_sequence(Vec::new())?;
    /// assert_eq!(empty.to_xml(&SerializeOptions::default())?, "");
    ///
    /// let two = c.build_sequence(vec![
    ///     Form::new(Name::local("a")),
    ///     Form::new(Name::local("b")),
    /// ])?;
    /// assert_eq!(two.to_xml(&SerializeOptions::default())?, "<a/><b/>");
    /// assert_eq!(two.children().len(), 2);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn build_sequence(&self, forms: Vec<Form<'a>>) -> Result<Doc<'a>, ComposeError> {
        self.build_shaped(forms, false)
    }

    // ── Evaluating ────────────────────────────────────────────────────

    /// Compiles `src` — once per composer — and evaluates it.
    ///
    /// `vars` names the external variables the expression may use; `bind`
    /// supplies their values. `ctx` is the context item, or `None` for an
    /// expression that needs no focus.
    ///
    /// This is the primitive the composition macros expand to. It is public so
    /// a host can call it directly — a variable whose name has a prefix, for
    /// instance — but the documented surface is the macros.
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
    /// // A context item, and no variables.
    /// let first = c.eval("string(b[1])", &[], Some(doc.document_element().unwrap()), Vec::new())?;
    /// assert_eq!(first.string()?, "1");
    ///
    /// // Variables, and no context item.
    /// let sum = c.eval(
    ///     "sum($bs)",
    ///     &["bs"],
    ///     None,
    ///     vec![("bs", c.eval("//b", &[], Some(doc.root()), Vec::new())?.into_xpath_value())],
    /// )?;
    /// assert_eq!(sum.string()?, "3");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[doc(hidden)]
    pub fn eval(
        &self,
        src: &'static str,
        vars: &'static [&'static str],
        ctx: Option<Nav<'a>>,
        bind: Vec<(&'static str, XPathValue<Nav<'a>>)>,
    ) -> Result<Value<'a>, ComposeError> {
        let expr = self.compiled(src, vars)?;
        // `set_variable_by_name` reports an undeclared name, but the setup
        // callback cannot return, so the refusal is carried out of it.
        let mut binding_error = None;
        let outcome = expr
            .evaluator(&self.ctx)
            .with_regex_cache(&self.regexes)
            .run_with_node_and_setup(ctx, |typed| {
                for (name, value) in bind {
                    if let Err(error) = typed.set_variable_by_name(name, value) {
                        binding_error = Some(error);
                        break;
                    }
                }
            });
        if let Some(error) = binding_error {
            return Err(ComposeError::XPath {
                source: error,
                expr: src.to_string(),
            });
        }
        match outcome {
            Ok(value) => Ok(Value::from(value)),
            Err(error) => Err(ComposeError::XPath {
                source: error,
                expr: src.to_string(),
            }),
        }
    }

    // ── Internals ─────────────────────────────────────────────────────

    /// The compiled expression for `(src, vars)`, compiling on first sight.
    fn compiled(
        &self,
        src: &'static str,
        vars: &'static [&'static str],
    ) -> Result<Rc<XPathExpr>, ComposeError> {
        let key = ExprKey { source: src, vars };
        // The borrow ends here, so a nested evaluation inside a loop body can
        // reach the cache while this one is still running.
        let cached = self.exprs.borrow().get(&key).map(Rc::clone);
        if let Some(expr) = cached {
            return Ok(expr);
        }
        let compiled = Rc::new(XPathExpr::compile_with_vars(src, &self.ctx, vars).map_err(
            |source| ComposeError::XPath {
                source,
                expr: src.to_string(),
            },
        )?);
        self.exprs.borrow_mut().insert(key, Rc::clone(&compiled));
        Ok(compiled)
    }

    /// Emits `forms` into a fresh document.
    fn build_shaped(
        &self,
        forms: Vec<Form<'a>>,
        single_element: bool,
    ) -> Result<Doc<'a>, ComposeError> {
        if single_element && forms.len() != 1 {
            return Err(ComposeError::BuildShape {
                top_level: forms.len(),
            });
        }

        let builder = BufferDocumentBuilder::new(
            self.arena,
            self.names,
            self.schema_set,
            BufferDocumentOptions::default(),
        )?;
        let mut emitter = Emitter::new(builder, self.copy);
        for (prefix, uri) in &self.namespaces {
            emitter = emitter.with_namespace(prefix, uri);
        }
        if let Some(uri) = &self.default_element_ns {
            emitter = emitter.with_default_element_namespace(uri);
        }
        for form in forms {
            emitter.element(form)?;
        }
        Ok(self.store(emitter.finish()?))
    }

    /// Moves a finished document into the arena and records it.
    fn store(&self, doc: BufferDocument<'a>) -> Doc<'a> {
        let stored: &'a BufferDocument<'a> = self.arena.alloc(doc);
        self.docs.borrow_mut().push(stored);
        Doc(stored)
    }

    /// Rebuilds the static context from the current configuration.
    ///
    /// Every setter goes through here, and every setter takes `self` by value,
    /// so this only ever runs before the first evaluation; the expression cache
    /// is cleared all the same, because a compiled expression has the old
    /// context's resolved names baked into it.
    fn rebuild_context(&mut self) {
        self.names = match self.schema_set {
            Some(set) => &set.name_table,
            None => self.given_names,
        };

        // A snapshot answers a lookup with the first binding it holds for the
        // prefix, so the bindings go in newest first: a rebinding of a prefix
        // — one of the predeclared ones included — wins over what it rebinds,
        // which is how the emitter reads the same list (innermost last).
        let mut snapshot = NamespaceContextSnapshot::default();
        for (prefix, uri) in self.namespaces.iter().rev() {
            let prefix = self.names.add(prefix);
            if snapshot.bindings.iter().any(|(p, _)| *p == prefix) {
                continue;
            }
            snapshot.bindings.push((prefix, self.names.add(uri)));
        }

        let mut ctx = XPathContext::new(self.names).with_namespaces(snapshot);
        if let Some(set) = self.schema_set {
            ctx = ctx.with_schema_set(set);
        }
        if let Some(uri) = &self.default_element_ns {
            let id = self.names.add(uri);
            ctx = ctx.with_default_element_ns(id);
        }
        if let Some(uri) = &self.base_uri {
            ctx = ctx.with_base_uri(uri.clone());
        }
        self.ctx = ctx;
        self.exprs.borrow_mut().clear();
    }

    /// How many distinct expressions have been compiled.
    ///
    /// Visible to the crate's own tests because the macro tests assert that
    /// one call site compiles one expression however often it runs.
    #[cfg(test)]
    pub(crate) fn cached_expressions(&self) -> usize {
        self.exprs.borrow().len()
    }

    /// How many documents this composer holds.
    #[cfg(test)]
    fn stored_documents(&self) -> usize {
        self.docs.borrow().len()
    }
}

impl fmt::Debug for Composer<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Composer")
            .field("namespaces", &self.namespaces)
            .field("default_element_namespace", &self.default_element_ns)
            .field("base_uri", &self.base_uri)
            .field("documents", &self.docs.borrow().len())
            .field("compiled_expressions", &self.exprs.borrow().len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compose::form::{AttrValue, Content, Name};
    use crate::compose::IntoXPathValue;

    fn arena_and_names() -> (Bump, NameTable) {
        (Bump::new(), NameTable::new())
    }

    #[test]
    fn compiled_regular_expressions_outlive_one_evaluation() {
        let (arena, names) = arena_and_names();
        let c = Composer::new(&arena, &names);
        for (word, capitalised) in [("Kite", "true"), ("bike", "false"), ("Sail", "true")] {
            let value = c
                .eval(
                    r"matches($w, '^\p{Lu}')",
                    &["w"],
                    None,
                    vec![("w", XPathValue::string(word))],
                )
                .unwrap();
            assert_eq!(value.string().unwrap(), capitalised);
        }
        // Three evaluations, one compilation.
        assert_eq!(c.regexes.compiles(), 1);
        assert_eq!(c.regexes.hits(), 2);
    }

    // ── The expression cache ──────────────────────────────────────────

    #[test]
    fn the_same_call_site_compiles_once() {
        let (arena, names) = arena_and_names();
        let c = Composer::new(&arena, &names);

        for _ in 0..3 {
            c.eval("1 + 1", &[], None, Vec::new()).unwrap();
        }
        assert_eq!(c.cached_expressions(), 1);
    }

    #[test]
    fn a_different_variable_list_is_a_different_entry() {
        let (arena, names) = arena_and_names();
        let c = Composer::new(&arena, &names);

        c.eval(
            "count($a)",
            &["a"],
            None,
            vec![("a", ().into_xpath_value())],
        )
        .unwrap();
        c.eval(
            "count($a)",
            &["a", "b"],
            None,
            vec![("a", ().into_xpath_value())],
        )
        .unwrap();
        assert_eq!(c.cached_expressions(), 2);
    }

    #[test]
    fn the_cache_is_keyed_by_content_not_by_address() {
        let (arena, names) = arena_and_names();
        let c = Composer::new(&arena, &names);

        // Two separate literals with the same text.
        const A: &str = "2 * 3";
        const B: &str = "2 * 3";
        c.eval(A, &[], None, Vec::new()).unwrap();
        c.eval(B, &[], None, Vec::new()).unwrap();
        assert_eq!(c.cached_expressions(), 1);

        const VARS_A: &[&str] = &["x"];
        const VARS_B: &[&str] = &["x"];
        c.eval("$x", VARS_A, None, vec![("x", 1.into_xpath_value())])
            .unwrap();
        c.eval("$x", VARS_B, None, vec![("x", 1.into_xpath_value())])
            .unwrap();
        assert_eq!(c.cached_expressions(), 2);
    }

    // ── Evaluation ────────────────────────────────────────────────────

    #[test]
    fn a_context_item_is_optional() {
        let (arena, names) = arena_and_names();
        let c = Composer::new(&arena, &names);
        let doc = c.load_str("<a><b>x</b></a>").unwrap();

        let with = c
            .eval(
                "string(b)",
                &[],
                Some(doc.document_element().unwrap()),
                Vec::new(),
            )
            .unwrap();
        assert_eq!(with.string().unwrap(), "x");

        let without = c.eval("string(1)", &[], None, Vec::new()).unwrap();
        assert_eq!(without.string().unwrap(), "1");
    }

    #[test]
    fn a_missing_context_item_is_reported_with_the_expression() {
        let (arena, names) = arena_and_names();
        let c = Composer::new(&arena, &names);

        match c.eval("string(b)", &[], None, Vec::new()) {
            Err(ComposeError::XPath { expr, .. }) => assert_eq!(expr, "string(b)"),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_compile_error_carries_the_expression_text() {
        let (arena, names) = arena_and_names();
        let c = Composer::new(&arena, &names);

        match c.eval("$undeclared + 1", &[], None, Vec::new()) {
            Err(ComposeError::XPath { expr, source }) => {
                assert_eq!(expr, "$undeclared + 1");
                assert!(source.to_string().contains("undeclared"), "{source}");
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
        // A failed compilation is not cached.
        assert_eq!(c.cached_expressions(), 0);
    }

    #[test]
    fn binding_an_undeclared_name_is_reported_with_the_expression() {
        let (arena, names) = arena_and_names();
        let c = Composer::new(&arena, &names);

        match c.eval("1", &[], None, vec![("nope", 1.into_xpath_value())]) {
            Err(ComposeError::XPath { expr, source }) => {
                assert_eq!(expr, "1");
                assert_eq!(source.error_code(), Some("XPST0008"));
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn nested_evaluations_share_the_cache() {
        let (arena, names) = arena_and_names();
        let c = Composer::new(&arena, &names);
        let doc = c.load_str("<a><b>1</b><b>2</b></a>").unwrap();

        // The inner evaluation runs while the outer result is still being used.
        let mut total = 0.0;
        for node in c
            .eval("//b", &[], Some(doc.root()), Vec::new())
            .unwrap()
            .nodes()
            .unwrap()
        {
            total += c
                .eval("number(.)", &[], Some(node), Vec::new())
                .unwrap()
                .number()
                .unwrap();
        }
        assert_eq!(total, 3.0);
        assert_eq!(c.cached_expressions(), 2);
    }

    // ── Loading ───────────────────────────────────────────────────────

    #[test]
    fn every_loaded_and_built_document_is_kept() {
        let (arena, names) = arena_and_names();
        let c = Composer::new(&arena, &names);

        c.load_str("<a/>").unwrap();
        c.load_reader("<b/>".as_bytes()).unwrap();
        c.build(Form::new(Name::local("c"))).unwrap();
        assert_eq!(c.stored_documents(), 3);
    }

    #[test]
    fn a_missing_file_is_an_io_error() {
        let (arena, names) = arena_and_names();
        let c = Composer::new(&arena, &names);
        assert!(matches!(
            c.load_file("this-file-does-not-exist.xml"),
            Err(ComposeError::Io(_))
        ));
    }

    #[test]
    fn malformed_xml_is_a_document_error() {
        let (arena, names) = arena_and_names();
        let c = Composer::new(&arena, &names);
        assert!(matches!(
            c.load_str("<a><b></a>"),
            Err(ComposeError::Document(_))
        ));
    }

    // ── Build shape ───────────────────────────────────────────────────

    #[test]
    fn build_refuses_no_top_level_element() {
        let (arena, names) = arena_and_names();
        let c = Composer::new(&arena, &names);
        assert!(matches!(
            c.build_shaped(Vec::new(), true),
            Err(ComposeError::BuildShape { top_level: 0 })
        ));
    }

    #[test]
    fn build_refuses_two_top_level_elements() {
        let (arena, names) = arena_and_names();
        let c = Composer::new(&arena, &names);
        assert!(matches!(
            c.build_shaped(
                vec![Form::new(Name::local("a")), Form::new(Name::local("b"))],
                true,
            ),
            Err(ComposeError::BuildShape { top_level: 2 })
        ));
    }

    #[test]
    fn build_sequence_accepts_none_and_several() {
        let (arena, names) = arena_and_names();
        let c = Composer::new(&arena, &names);

        let empty = c.build_sequence(Vec::new()).unwrap();
        assert!(empty.children().is_empty());
        assert!(empty.document_element().is_none());
        assert_eq!(empty.to_xml(&SerializeOptions::default()).unwrap(), "");

        let two = c
            .build_sequence(vec![
                Form::new(Name::local("a")),
                Form::new(Name::local("b")),
            ])
            .unwrap();
        assert_eq!(two.children().len(), 2);
        assert_eq!(two.document_element().unwrap().local_name(), "a");
        assert_eq!(
            two.to_xml(&SerializeOptions::default()).unwrap(),
            "<a/><b/>"
        );
    }

    #[test]
    fn a_built_sequence_is_bindable_and_queryable() {
        let (arena, names) = arena_and_names();
        let c = Composer::new(&arena, &names);

        // The shape of a user function returning `element()*`.
        let mut counts = Vec::new();
        for n in ["2", "5", "8"] {
            let mut nbids = Form::new(Name::local("nbids"));
            nbids.push(Content::Text(n.to_string()));
            let mut wrapper = Form::new(Name::local("bid_count"));
            wrapper.push(Content::Element(nbids));
            counts.push(wrapper);
        }
        let sequence = c.build_sequence(counts).unwrap();
        let expected = concat!(
            "<bid_count><nbids>2</nbids></bid_count>",
            "<bid_count><nbids>5</nbids></bid_count>",
            "<bid_count><nbids>8</nbids></bid_count>",
        );
        assert_eq!(
            sequence.to_xml(&SerializeOptions::default()).unwrap(),
            expected
        );

        let total = c
            .eval(
                "count($seq/nbids)",
                &["seq"],
                None,
                vec![("seq", sequence.children().into_xpath_value())],
            )
            .unwrap();
        assert_eq!(total.string().unwrap(), "3");

        let sum = c
            .eval(
                "sum($seq/nbids)",
                &["seq"],
                None,
                vec![("seq", sequence.children().into_xpath_value())],
            )
            .unwrap();
        assert_eq!(sum.string().unwrap(), "15");
    }

    // ── Doc ───────────────────────────────────────────────────────────

    #[test]
    fn a_document_reports_its_top_level_nodes() {
        let (arena, names) = arena_and_names();
        let c = Composer::new(&arena, &names);
        let doc = c.load_str("<!--lead--><a/><?trail x?>").unwrap();

        assert_eq!(doc.children().len(), 3);
        assert_eq!(doc.document_element().unwrap().local_name(), "a");
        assert!(doc.inner().schema_set().is_none());
    }

    #[test]
    fn write_xml_matches_to_xml() {
        let (arena, names) = arena_and_names();
        let c = Composer::new(&arena, &names);
        let doc = c.load_str("<a><b>1 &lt; 2</b></a>").unwrap();

        let opts = SerializeOptions::default();
        let mut bytes: Vec<u8> = Vec::new();
        doc.write_xml(&mut bytes, &opts).unwrap();
        assert_eq!(
            String::from_utf8(bytes).unwrap(),
            doc.to_xml(&opts).unwrap()
        );
    }

    #[test]
    fn a_doc_handle_is_copy() {
        let (arena, names) = arena_and_names();
        let c = Composer::new(&arena, &names);
        let doc = c.load_str("<a/>").unwrap();
        let same = doc;
        assert_eq!(
            same.to_xml(&SerializeOptions::default()).unwrap(),
            doc.to_xml(&SerializeOptions::default()).unwrap(),
        );
        assert!(format!("{doc:?}").contains("Doc"));
    }

    // ── Configuration ─────────────────────────────────────────────────

    #[test]
    fn a_bound_prefix_resolves_in_an_expression() {
        let (arena, names) = arena_and_names();
        let c = Composer::new(&arena, &names).with_namespace("h", "urn:h");
        let doc = c.load_str(r#"<r xmlns:x="urn:h"><x:a/></r>"#).unwrap();

        let found = c
            .eval("count(//h:a)", &[], Some(doc.root()), Vec::new())
            .unwrap();
        assert_eq!(found.string().unwrap(), "1");
    }

    #[test]
    fn a_default_element_namespace_resolves_in_an_expression() {
        let (arena, names) = arena_and_names();
        let c = Composer::new(&arena, &names).with_default_element_namespace("urn:d");
        let doc = c.load_str(r#"<r xmlns="urn:d"><a/></r>"#).unwrap();

        let found = c
            .eval("count(//a)", &[], Some(doc.root()), Vec::new())
            .unwrap();
        assert_eq!(found.string().unwrap(), "1");
    }

    #[test]
    fn a_base_uri_reaches_the_static_context() {
        let (arena, names) = arena_and_names();
        let c = Composer::new(&arena, &names).with_base_uri("http://example.com/here/");

        let resolved = c
            .eval("string(resolve-uri('there'))", &[], None, Vec::new())
            .unwrap();
        assert_eq!(resolved.string().unwrap(), "http://example.com/here/there");
    }

    #[test]
    fn the_debug_output_names_the_configuration() {
        let (arena, names) = arena_and_names();
        let c = Composer::new(&arena, &names).with_namespace("p", "urn:x");
        let text = format!("{c:?}");
        assert!(text.contains("Composer"), "{text}");
        assert!(text.contains("urn:x"), "{text}");
    }

    // ── Predeclared prefixes ──────────────────────────────────────────

    #[test]
    fn a_cast_needs_no_declaration_of_its_own() {
        let (arena, names) = arena_and_names();
        let c = Composer::new(&arena, &names);

        let date = c
            .eval(
                "month-from-date(xs:date('1999-01-31'))",
                &[],
                None,
                Vec::new(),
            )
            .expect("`xs` is bound");
        assert_eq!(date.string().unwrap(), "1");

        let integer = c
            .eval("xs:integer('42') + 1", &[], None, Vec::new())
            .expect("`xs` is bound");
        assert_eq!(integer.string().unwrap(), "43");
    }

    #[test]
    fn the_function_prefix_is_bound() {
        let (arena, names) = arena_and_names();
        let c = Composer::new(&arena, &names);

        let counted = c
            .eval("fn:count((1, 2, 3))", &[], None, Vec::new())
            .expect("`fn` is bound");
        assert_eq!(counted.string().unwrap(), "3");
    }

    #[test]
    fn the_instance_prefix_is_bound() {
        let (arena, names) = arena_and_names();
        let c = Composer::new(&arena, &names);
        let doc = c
            .load_str(r#"<r xmlns:i="http://www.w3.org/2001/XMLSchema-instance" i:nil="true"/>"#)
            .unwrap();

        let found = c
            .eval("count(//@xsi:nil)", &[], Some(doc.root()), Vec::new())
            .expect("`xsi` is bound");
        assert_eq!(found.string().unwrap(), "1");
    }

    #[test]
    fn a_predeclared_prefix_reaches_a_form_and_its_output() {
        let (arena, names) = arena_and_names();
        let c = Composer::new(&arena, &names);

        let mut form = Form::new(Name::prefixed("xs", "schema"));
        form.attr(Name::prefixed("xsi", "type"), AttrValue::text("xs:string"));

        let written = c
            .build(form)
            .unwrap()
            .to_xml(&SerializeOptions::default())
            .unwrap();
        assert_eq!(
            written,
            concat!(
                r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" "#,
                r#"xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" "#,
                r#"xsi:type="xs:string"/>"#,
            ),
        );
    }

    #[test]
    fn an_unused_predeclared_prefix_is_not_declared() {
        let (arena, names) = arena_and_names();
        let c = Composer::new(&arena, &names);

        let written = c
            .build(Form::new(Name::local("plain")))
            .unwrap()
            .to_xml(&SerializeOptions::default())
            .unwrap();
        assert_eq!(written, "<plain/>");
    }

    #[test]
    fn a_rebinding_of_a_predeclared_prefix_wins() {
        let (arena, names) = arena_and_names();
        let c = Composer::new(&arena, &names).with_namespace("xs", "urn:mine");
        let doc = c.load_str(r#"<r xmlns:z="urn:mine"><z:a/></r>"#).unwrap();

        let found = c
            .eval("count(//xs:a)", &[], Some(doc.root()), Vec::new())
            .expect("`xs` resolves to the new namespace");
        assert_eq!(found.string().unwrap(), "1");

        // And the last declaration is the only one: nothing resolves `xs` to
        // the schema namespace any more, so the constructor is gone.
        assert!(c
            .eval("xs:date('1999-01-31')", &[], None, Vec::new())
            .is_err());
    }

    #[test]
    fn predeclaring_leaves_the_cache_empty() {
        let (arena, names) = arena_and_names();
        let c = Composer::new(&arena, &names);
        assert_eq!(c.cached_expressions(), 0, "nothing is compiled up front");

        for _ in 0..3 {
            c.eval("xs:integer('1')", &[], None, Vec::new()).unwrap();
        }
        assert_eq!(c.cached_expressions(), 1, "one call site, one compilation");
    }
}
