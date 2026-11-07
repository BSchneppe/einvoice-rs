//! Writing a navigable XML tree back out as XML text.
//!
//! [`BufferDocument`](super::BufferDocument) and the read-only roxmltree adapter
//! can both be navigated, but neither could be *written*. This module closes that
//! gap for every backend at once: it is generic over
//! [`DomNavigator`] and uses nothing but
//! navigation plus the node's name and string value, so the same code serializes
//! a `BufferDocNavigator`, a `RoXmlNavigator`, and any navigator a host embedding
//! the engine supplies of its own.
//!
//! ```
//! use bumpalo::Bump;
//! use xsd_schema::document::{serialize, BufferDocument};
//! use xsd_schema::namespace::NameTable;
//!
//! let arena = Bump::new();
//! let names = NameTable::new();
//! let doc = BufferDocument::from_reader_default(
//!     r#"<p:a xmlns:p="urn:x"><p:b k="1">t</p:b><!--c--></p:a>"#.as_bytes(),
//!     &arena,
//!     &names,
//! )?;
//!
//! let xml = serialize::to_string(&doc.create_navigator(), &Default::default())?;
//! assert_eq!(xml, r#"<p:a xmlns:p="urn:x"><p:b k="1">t</p:b><!--c--></p:a>"#);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # What the output looks like
//!
//! Output is UTF-8 bytes. By default nothing is added: the serializer writes
//! exactly the nodes of the tree, so `parse → serialize → parse` yields the same
//! tree again. Empty elements are always collapsed to `<a/>`.
//!
//! Escaping follows the rules of *Canonical XML 1.0* §2.3 ("Processing Model",
//! its *Text Nodes* and *Attribute Nodes* items), because those are the rules
//! designed so that a re-parse cannot change the value back: without them XML
//! 1.0's end-of-line handling (§2.11) and attribute-value normalization (§3.3.3)
//! would silently rewrite literal carriage returns and attribute whitespace.
//!
//! | Context | Character | Written as |
//! |---|---|---|
//! | text | `&` `<` `>` | `&amp;` `&lt;` `&gt;` (`>` always, which covers the `]]>` rule of XML 1.0 §2.4 without scanning) |
//! | text | U+000D | `&#xD;` |
//! | attribute value (always `"`-quoted) | `&` `<` `"` | `&amp;` `&lt;` `&quot;` |
//! | attribute value | U+0009 U+000A U+000D | `&#x9;` `&#xA;` `&#xD;` |
//! | comment, PI target, PI data, names | nothing | — |
//!
//! This is an escaping policy, not an implementation of Canonical XML: nothing
//! here sorts attributes, normalizes namespace declarations or expands empty
//! elements.
//!
//! # What is refused
//!
//! A tree parsed from XML cannot hold content that XML cannot express, but a
//! tree built programmatically can. Such content is never dropped or mangled —
//! it is an error:
//!
//! * a character outside the XML 1.0 `Char` production (C0 controls other than
//!   tab, LF and CR; U+FFFE; U+FFFF) anywhere, including in names
//!   ([`SerializeError::InvalidChar`]);
//! * a comment containing `--` or ending in `-`, XML 1.0 §2.5
//!   ([`SerializeError::InvalidComment`]);
//! * a processing instruction whose target is `xml` in any case, or whose data
//!   contains `?>`, XML 1.0 §2.6 ([`SerializeError::InvalidPi`]);
//! * an element or attribute whose prefix is not bound to its namespace URI in
//!   the scope the output creates ([`SerializeError::UnboundName`]).
//!
//! Out of scope for this module: other encodings, CDATA sections and a doctype
//! declaration.
//!
//! # Compact and formatted output
//!
//! [`SerializeOptions::indent`] chooses between the two modes. `None`, the
//! default, is *compact*: no layout whitespace is added — and none is removed,
//! so it is the mode for an exact text round trip. `Some(n)` is *formatted*: a
//! line break plus `n` spaces per level wherever the contract below permits one
//! (`Some(0)` gives the breaks without the spaces).
//!
//! ```
//! use bumpalo::Bump;
//! use xsd_schema::document::{serialize, BufferDocument, SerializeOptions};
//! use xsd_schema::namespace::NameTable;
//!
//! let arena = Bump::new();
//! let names = NameTable::new();
//! let doc = BufferDocument::from_reader_default(
//!     "<catalog><book id=\"b1\"><title>One</title></book></catalog>".as_bytes(),
//!     &arena,
//!     &names,
//! )?;
//! let nav = doc.create_navigator();
//!
//! let formatted = SerializeOptions { indent: Some(2), ..Default::default() };
//! assert_eq!(
//!     serialize::to_string(&nav, &formatted)?,
//!     "<catalog>\n  <book id=\"b1\">\n    <title>One</title>\n  </book>\n</catalog>",
//! );
//!
//! // The same tree, no layout added, nothing rebuilt.
//! assert_eq!(
//!     serialize::to_string(&nav, &Default::default())?,
//!     "<catalog><book id=\"b1\"><title>One</title></book></catalog>",
//! );
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Line breaks are LF on every platform, the outermost element sits at depth 0
//! (a subtree written on its own included), attributes stay on the start-tag
//! line, an empty element stays `<a/>`, and no trailing newline is written.
//! Whether a declaration is written is independent of the mode.
//!
//! ## Whitespace contract
//!
//! Formatted output must not change what the document says, so layout is added
//! conservatively:
//!
//! 1. **Every existing text character survives, in both modes**, whitespace-only
//!    text included. Before formatting a container, *all* of its direct children
//!    are inspected: any text child suppresses added layout throughout that
//!    container's subtree. So `<p>Hello <b>world</b>!</p>` and
//!    `<p><b>world</b>!</p>` both stay on one line, and existing indentation is
//!    kept rather than rewritten.
//! 2. **A break goes only at a child boundary next to an element**: before an
//!    element child at the child's depth, before the closing tag at the
//!    container's depth when the last child is an element, and between siblings
//!    when either one is an element. A run of comments and processing
//!    instructions is never split internally, and nothing is ever added inside
//!    text, a comment, PI data, an attribute value or an empty element.
//! 3. **`xml:space` is honoured** (XML 1.0 §2.10, read by its expanded
//!    XML-namespace name): `preserve` turns added layout off for that element and
//!    is inherited; a descendant `default` turns it back on for its own content,
//!    unless rule 1 still suppresses it. A subtree serialized on its own consults
//!    its ancestors for the inherited value without inventing an attribute.
//! 4. **At document level** the same rules apply at depth 0. A declaration
//!    immediately followed by the document element gets a break between them; a
//!    declaration followed by a comment or PI does not. No leading blank line and
//!    no final newline are invented, and a standalone text, comment or PI node
//!    gets no layout at all.
//!
//! Formatted output therefore adds whitespace *text nodes* to a reparsed
//! document, which can change string values and text-node counts: compact mode
//! is the contract for exact round trips, formatted mode is a presentation
//! choice. Tabs, other line endings and line wrapping are out of scope.
//!
//! # Namespace declarations
//!
//! Declarations are written where they are *introduced*, computed by comparing
//! each element's in-scope bindings (the `namespace::` axis, minus `xml`) with
//! the scope the output has already established — see [`serialize_node`] for
//! what that means for a subtree. A declaration that only repeats what is
//! already in scope is dropped; an element that leaves an inherited default
//! namespace gets `xmlns=""`. On one element the declarations come out in
//! prefix order, the default namespace first, because the axis itself has no
//! order to preserve.

use std::io::Write;

use crate::namespace::XML_NAMESPACE;
use crate::navigator::{DomNavigator, DomNodeType, NamespaceAxisScope};

/// Options controlling XML output.
///
/// The default writes no XML declaration, which is what a host wants when the
/// result is embedded in something larger; turn it on for a standalone file.
///
/// ```
/// use xsd_schema::document::SerializeOptions;
///
/// let embedded = SerializeOptions::default();
/// assert!(!embedded.xml_declaration);
///
/// let file = SerializeOptions {
///     xml_declaration: true,
///     standalone: Some(true),
///     indent: Some(2),
/// };
/// assert_eq!(file.standalone, Some(true));
/// ```
#[derive(Clone, Debug, Default)]
pub struct SerializeOptions {
    /// Write `<?xml version="1.0" encoding="UTF-8"?>` before a document.
    ///
    /// The declaration is written by [`serialize_document`], and by
    /// [`serialize_node`] on a document (`Root`) node; a node written on its own
    /// is a fragment and never gets one.
    pub xml_declaration: bool,
    /// Adds ` standalone="yes"` / ` standalone="no"` to that declaration.
    ///
    /// Ignored when `xml_declaration` is `false`.
    pub standalone: Option<bool>,
    /// `None` — the default — writes no layout whitespace. `Some(n)` writes a
    /// line break plus `n` spaces per level where the whitespace contract
    /// allows one; `Some(0)` writes the line breaks alone.
    ///
    /// `None` is not a minifier: it adds no whitespace and removes none, which
    /// is what makes it the mode for an exact text round trip. Breaks are LF on
    /// every platform, the outermost element sits at depth 0 (a subtree
    /// included), and no trailing newline is written. See the module docs for
    /// the rules on where a break may go.
    pub indent: Option<usize>,
}

/// Why a tree could not be written as XML.
///
/// Every variant but [`Io`](SerializeError::Io) reports content that no
/// well-formed XML document can hold, so it is reported rather than silently
/// repaired; repairing names is the job of the code that builds the tree.
///
/// ```
/// use xsd_schema::document::{serialize, SerializeError};
/// use xsd_schema::navigator::{DomNavigator, RoXmlNavigator};
///
/// // An attribute is not a tree, so it cannot be written on its own.
/// let doc = roxmltree::Document::parse(r#"<a k="v"/>"#)?;
/// let mut attr = RoXmlNavigator::new(&doc);
/// attr.move_to_first_child();
/// attr.move_to_first_attribute();
/// match serialize::to_string(&attr, &Default::default()) {
///     Err(SerializeError::NotSerializable(kind)) => println!("refused {kind:?}"),
///     other => panic!("expected a refusal, got {other:?}"),
/// }
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, thiserror::Error)]
pub enum SerializeError {
    /// The writer failed.
    #[error("failed to write XML: {0}")]
    Io(#[from] std::io::Error),

    /// A character that the XML 1.0 `Char` production does not allow, in a
    /// place where no escape can express it either.
    ///
    /// `context` is one of `"text"`, `"attribute value"`, `"comment"`,
    /// `"processing instruction"` or `"name"`.
    #[error("U+{code_point:04X} is not an XML 1.0 Char and cannot appear in {context}")]
    InvalidChar {
        /// The offending Unicode scalar value.
        code_point: u32,
        /// Where it was found.
        context: &'static str,
    },

    /// Comment content containing `--`, or ending in `-` (XML 1.0 §2.5).
    #[error("comment content is not well-formed (XML 1.0 \u{a7}2.5): {0:?}")]
    InvalidComment(String),

    /// A processing instruction that XML 1.0 §2.6 does not allow.
    #[error("processing instruction {target:?} is not well-formed (XML 1.0 \u{a7}2.6): {reason}")]
    InvalidPi {
        /// The PI target as found in the tree.
        target: String,
        /// What is wrong with it.
        reason: &'static str,
    },

    /// An element or attribute name whose prefix is not bound to its namespace
    /// URI in the scope the output establishes, so the written name would not
    /// re-parse to the same expanded name.
    #[error("prefix {prefix:?} is not bound to namespace {uri:?} in the output scope")]
    UnboundName {
        /// The name's prefix (empty for an unprefixed name).
        prefix: String,
        /// The namespace URI the node claims.
        uri: String,
    },

    /// An attribute or namespace node was asked to serialize itself. Both are
    /// properties of an element, not subtrees, so there is nothing to write.
    #[error("a {0:?} node cannot be serialized on its own")]
    NotSerializable(DomNodeType),
}

/// Writes the document containing `root`, optionally preceded by an XML
/// declaration.
///
/// `root` may be positioned anywhere in the document: the serializer starts from
/// its document root, writing every top-level node — comments and processing
/// instructions around the document element included — in document order.
///
/// ```
/// use bumpalo::Bump;
/// use xsd_schema::document::{serialize, BufferDocument, SerializeOptions};
/// use xsd_schema::namespace::NameTable;
///
/// let arena = Bump::new();
/// let names = NameTable::new();
/// let doc = BufferDocument::from_reader_default(
///     "<?work now?><a/><!--after-->".as_bytes(),
///     &arena,
///     &names,
/// )?;
///
/// let mut out = Vec::new();
/// let opts = SerializeOptions { xml_declaration: true, ..Default::default() };
/// serialize::serialize_document(&doc.create_navigator(), &mut out, &opts)?;
/// assert_eq!(
///     String::from_utf8(out)?,
///     r#"<?xml version="1.0" encoding="UTF-8"?><?work now?><a/><!--after-->"#,
/// );
///
/// // Formatted: a break lands next to the element and nowhere else.
/// let mut out = Vec::new();
/// let opts = SerializeOptions { xml_declaration: true, indent: Some(2), ..Default::default() };
/// serialize::serialize_document(&doc.create_navigator(), &mut out, &opts)?;
/// assert_eq!(
///     String::from_utf8(out)?,
///     "<?xml version=\"1.0\" encoding=\"UTF-8\"?><?work now?>\n<a/>\n<!--after-->",
/// );
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn serialize_document<N: DomNavigator, W: Write>(
    root: &N,
    out: W,
    opts: &SerializeOptions,
) -> Result<(), SerializeError> {
    let mut nav = root.clone();
    nav.move_to_root();
    Serializer::new(out, opts)
        .with_ancestor_layout(&nav)
        .write_document(&nav)
}

/// Writes the node at the cursor.
///
/// * On a document (`Root`) node this is [`serialize_document`].
/// * On an element it writes that subtree. The element receives the namespace
///   declarations it needs, inherited ones included, so the fragment stands on
///   its own.
/// * On a text, comment or processing-instruction node it writes that one node,
///   with no layout around it even in formatted mode.
/// * On an attribute or namespace node it is
///   [`SerializeError::NotSerializable`].
///
/// ```
/// use bumpalo::Bump;
/// use xsd_schema::document::{serialize, BufferDocument};
/// use xsd_schema::namespace::NameTable;
/// use xsd_schema::navigator::DomNavigator;
///
/// let arena = Bump::new();
/// let names = NameTable::new();
/// let doc = BufferDocument::from_reader_default(
///     r#"<a xmlns:p="urn:x"><p:b/></a>"#.as_bytes(),
///     &arena,
///     &names,
/// )?;
///
/// let mut inner = doc.create_navigator();
/// inner.move_to_first_child(); // <a>
/// inner.move_to_first_child(); // <p:b>
///
/// // `xmlns:p` is declared on <a>, and travels with the subtree.
/// let mut out = Vec::new();
/// serialize::serialize_node(&inner, &mut out, &Default::default())?;
/// assert_eq!(String::from_utf8(out)?, r#"<p:b xmlns:p="urn:x"/>"#);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn serialize_node<N: DomNavigator, W: Write>(
    node: &N,
    out: W,
    opts: &SerializeOptions,
) -> Result<(), SerializeError> {
    let kind = node.node_type();
    let mut ser = Serializer::new(out, opts).with_ancestor_layout(node);
    match kind {
        DomNodeType::Root => ser.write_document(node),
        DomNodeType::Attribute | DomNodeType::Namespace | DomNodeType::All => {
            Err(SerializeError::NotSerializable(kind))
        }
        _ => ser.write_subtree(node),
    }
}

/// [`serialize_node`] into a `String`.
///
/// ```
/// use xsd_schema::document::serialize;
/// use xsd_schema::navigator::{DomNavigator, RoXmlNavigator};
///
/// let doc = roxmltree::Document::parse("<a>1 &lt; 2</a>")?;
/// let mut nav = RoXmlNavigator::new(&doc);
/// nav.move_to_first_child();
///
/// assert_eq!(
///     serialize::to_string(&nav, &Default::default())?,
///     "<a>1 &lt; 2</a>",
/// );
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn to_string<N: DomNavigator>(
    node: &N,
    opts: &SerializeOptions,
) -> Result<String, SerializeError> {
    let mut buf: Vec<u8> = Vec::new();
    serialize_node(node, &mut buf, opts)?;
    // Every byte written came from a `&str`, so the buffer is valid UTF-8.
    Ok(String::from_utf8(buf).expect("serializer writes UTF-8"))
}

// ── Implementation ───────────────────────────────────────────────────────

/// The `xml` prefix, which is bound implicitly everywhere and is therefore
/// never declared in the output.
const XML_PREFIX: &str = "xml";

/// True for a Unicode scalar the XML 1.0 (Fifth Edition) `Char` production
/// excludes: `Char ::= #x9 | #xA | #xD | [#x20-#xD7FF] | [#xE000-#xFFFD] |
/// [#x10000-#x10FFFF]`. Surrogates cannot occur in a Rust `str`, so what is
/// left is the C0 controls other than tab/LF/CR plus U+FFFE and U+FFFF.
fn is_forbidden_char(ch: char) -> bool {
    match ch {
        '\t' | '\n' | '\r' => false,
        c if (c as u32) < 0x20 => true,
        '\u{FFFE}' | '\u{FFFF}' => true,
        _ => false,
    }
}

/// Spaces to write an indent from, so a break costs no allocation.
const SPACES: [u8; 64] = [b' '; 64];

/// What a container's content is allowed to receive in the way of layout, and
/// what its children inherit.
#[derive(Clone, Copy, Debug, Default)]
struct Layout {
    /// Add breaks at this container's child boundaries.
    enabled: bool,
    /// A text child of this container or of an ancestor. Rule 1 of the
    /// whitespace contract: it suppresses layout for the whole subtree and
    /// cannot be switched back on.
    suppressed: bool,
    /// The effective `xml:space="preserve"`, inherited until a descendant says
    /// `default` (XML 1.0 §2.10).
    preserve: bool,
}

/// One open element: what to unwind when its end tag is written.
struct Frame {
    ns_watermark: usize,
    layout: Layout,
    /// The kind of the last child written in this container; `None` before the
    /// first one, which is what distinguishes the opening boundary.
    prev_child: Option<DomNodeType>,
}

/// The in-scope namespace stack, the layout state and the writer.
struct Serializer<'o, W: Write> {
    out: W,
    opts: &'o SerializeOptions,
    /// In-scope `(prefix, uri)` bindings, innermost last. An entry with an
    /// empty prefix *and* an empty URI is an undeclared default namespace.
    /// This is the serializer's only per-node allocation.
    ns_stack: Vec<(String, String)>,
    /// The layout state the requested node inherits from its ancestors. Only
    /// `suppressed` and `preserve` are read; `enabled` is always recomputed.
    seed: Layout,
}

impl<'o, W: Write> Serializer<'o, W> {
    fn new(out: W, opts: &'o SerializeOptions) -> Self {
        Self {
            out,
            opts,
            ns_stack: Vec::new(),
            seed: Layout::default(),
        }
    }

    /// Seeds the inherited `xml:space` from the node's ancestors, so an
    /// attached subtree honours a `preserve` it sits inside without the
    /// serializer inventing an attribute for it (XML 1.0 §2.10).
    fn with_ancestor_layout<N: DomNavigator>(mut self, node: &N) -> Self {
        if self.opts.indent.is_none() {
            return self;
        }
        let mut ancestor = node.clone();
        while ancestor.move_to_parent() {
            if ancestor.node_type() == DomNodeType::Element {
                if let Some(preserve) = xml_space(&ancestor) {
                    self.seed.preserve = preserve;
                    break;
                }
            }
        }
        self
    }

    // ── Layout ───────────────────────────────────────────────────────

    /// The layout state for an element's content: rule 1's child scan (skipped
    /// when a break could not be written anyway) over rule 3's `xml:space`.
    fn content_layout<N: DomNavigator>(
        &self,
        nav: &N,
        inherited: Layout,
        space: Option<bool>,
    ) -> Layout {
        let indented = self.opts.indent.is_some();
        let preserve = space.unwrap_or(inherited.preserve);
        // `||` short-circuits: an already-suppressed subtree is never scanned,
        // and neither is anything in compact mode.
        let suppressed = inherited.suppressed || (indented && has_text_child(nav));
        Layout {
            enabled: indented && !suppressed && !preserve,
            suppressed,
            preserve,
        }
    }

    /// A line break plus `depth` levels of indentation.
    fn write_break(&mut self, depth: usize) -> Result<(), SerializeError> {
        self.out.write_all(b"\n")?;
        let mut remaining = self.opts.indent.unwrap_or(0) * depth;
        while remaining > 0 {
            let take = remaining.min(SPACES.len());
            self.out.write_all(&SPACES[..take])?;
            remaining -= take;
        }
        Ok(())
    }

    // ── Namespace scope ──────────────────────────────────────────────

    /// The namespace URI `prefix` resolves to in the scope written so far, or
    /// `None` when the prefix is not usable there.
    ///
    /// The empty prefix always resolves (to the empty URI when no default
    /// namespace is in scope); `xml` resolves without a declaration.
    fn in_scope(&self, prefix: &str) -> Option<&str> {
        if prefix == XML_PREFIX {
            return Some(XML_NAMESPACE);
        }
        match self
            .ns_stack
            .iter()
            .rev()
            .find(|(p, _)| p == prefix)
            .map(|(_, u)| u.as_str())
        {
            // A prefixed binding to the empty URI is an XML 1.1 undeclaration;
            // it is never written, so the prefix is not usable here.
            Some("") if !prefix.is_empty() => None,
            Some(uri) => Some(uri),
            None if prefix.is_empty() => Some(""),
            None => None,
        }
    }

    fn is_bound(&self, prefix: &str, uri: &str) -> bool {
        self.in_scope(prefix) == Some(uri)
    }

    /// Whether a non-empty default namespace is in scope, i.e. whether an
    /// element with no default namespace needs `xmlns=""`.
    fn has_default_namespace(&self) -> bool {
        matches!(self.in_scope(""), Some(uri) if !uri.is_empty())
    }

    fn push_binding(&mut self, prefix: &str, uri: &str) {
        self.ns_stack.push((prefix.to_string(), uri.to_string()));
    }

    // ── Node dispatch ────────────────────────────────────────────────

    fn write_document<N: DomNavigator>(&mut self, node: &N) -> Result<(), SerializeError> {
        let declaration = self.opts.xml_declaration;
        if declaration {
            self.out
                .write_all(br#"<?xml version="1.0" encoding="UTF-8""#)?;
            match self.opts.standalone {
                Some(true) => self.out.write_all(br#" standalone="yes""#)?,
                Some(false) => self.out.write_all(br#" standalone="no""#)?,
                None => {}
            }
            self.out.write_all(b"?>")?;
        }
        if node.node_type() != DomNodeType::Root {
            // A fragment tree whose visible root is an element (an assertion
            // fragment, say) has no document node to walk into.
            if declaration && self.opts.indent.is_some() {
                self.write_break(0)?;
            }
            return self.write_subtree(node);
        }
        // The document node is a container too, at depth 0, under the same
        // rules; a declaration stands in for its opening boundary.
        let layout = self.content_layout(node, self.seed, None);
        let mut prev_child: Option<DomNodeType> = None;
        let mut child = node.clone();
        if child.move_to_first_child() {
            loop {
                let kind = child.node_type();
                let boundary = match prev_child {
                    // No leading break is invented; a declaration is what
                    // there is to separate the document element from.
                    None => declaration && kind == DomNodeType::Element,
                    Some(prev) => is_element(prev) || is_element(kind),
                };
                if layout.enabled && boundary {
                    self.write_break(0)?;
                }
                prev_child = Some(kind);
                self.write_subtree(&child)?;
                if !child.move_to_next_sibling() {
                    break;
                }
            }
        }
        // No final newline either.
        Ok(())
    }

    /// Writes the subtree at the cursor, iteratively: `frames` holds one entry
    /// per open element — its `ns_stack` watermark and its layout state — so
    /// descending never recurses and a deep document cannot overflow the Rust
    /// stack. `frames.len()` is the depth of the node at the cursor, which is
    /// also its indentation level; the node the caller asked for is at depth 0.
    fn write_subtree<N: DomNavigator>(&mut self, start: &N) -> Result<(), SerializeError> {
        let mut nav = start.clone();
        let mut frames: Vec<Frame> = Vec::new();
        loop {
            let kind = nav.node_type();
            // The boundary before this child, inside its container. A standalone
            // node has no container, hence no layout around it.
            let boundary = frames.last().is_some_and(|frame| {
                frame.layout.enabled
                    && match frame.prev_child {
                        None => is_element(kind),
                        Some(prev) => is_element(prev) || is_element(kind),
                    }
            });
            if boundary {
                self.write_break(frames.len())?;
            }
            if let Some(frame) = frames.last_mut() {
                frame.prev_child = Some(kind);
            }

            let mut descended = false;
            match kind {
                DomNodeType::Element => {
                    let ns_watermark = self.ns_stack.len();
                    let space = self.write_start_tag(&nav)?;
                    let mut child = nav.clone();
                    if child.move_to_first_child() {
                        self.out.write_all(b">")?;
                        let inherited = frames.last().map_or(self.seed, |frame| frame.layout);
                        let layout = self.content_layout(&nav, inherited, space);
                        frames.push(Frame {
                            ns_watermark,
                            layout,
                            prev_child: None,
                        });
                        nav = child;
                        descended = true;
                    } else {
                        // An empty element stays `<a/>`; nothing goes inside it.
                        self.out.write_all(b"/>")?;
                        self.ns_stack.truncate(ns_watermark);
                    }
                }
                kind => self.write_leaf(&nav, kind)?,
            }
            if descended {
                continue;
            }
            // Advance to the next sibling, closing elements on the way up.
            loop {
                if frames.is_empty() {
                    return Ok(());
                }
                if nav.move_to_next_sibling() {
                    break;
                }
                nav.move_to_parent();
                let frame = frames.pop().expect("checked non-empty");
                // The closing boundary, at the container's own depth.
                if frame.layout.enabled && frame.prev_child.is_some_and(is_element) {
                    self.write_break(frames.len())?;
                }
                self.write_end_tag(&nav)?;
                self.ns_stack.truncate(frame.ns_watermark);
            }
        }
    }

    fn write_leaf<N: DomNavigator>(
        &mut self,
        nav: &N,
        kind: DomNodeType,
    ) -> Result<(), SerializeError> {
        match kind {
            DomNodeType::Text | DomNodeType::Whitespace | DomNodeType::SignificantWhitespace => {
                self.write_text(&nav.value_ref())
            }
            DomNodeType::Comment => {
                let value = nav.value_ref();
                check_chars(&value, "comment")?;
                if value.contains("--") || value.ends_with('-') {
                    return Err(SerializeError::InvalidComment(value.into_owned()));
                }
                self.out.write_all(b"<!--")?;
                self.out.write_all(value.as_bytes())?;
                self.out.write_all(b"-->")?;
                Ok(())
            }
            DomNodeType::ProcessingInstruction => {
                let target = nav.local_name();
                check_chars(target, "name")?;
                if target.eq_ignore_ascii_case(XML_PREFIX) {
                    return Err(SerializeError::InvalidPi {
                        target: target.to_string(),
                        reason: "the target `xml` is reserved",
                    });
                }
                let data = nav.value_ref();
                check_chars(&data, "processing instruction")?;
                if data.contains("?>") {
                    return Err(SerializeError::InvalidPi {
                        target: target.to_string(),
                        reason: "the data contains `?>`",
                    });
                }
                self.out.write_all(b"<?")?;
                self.out.write_all(target.as_bytes())?;
                if !data.is_empty() {
                    self.out.write_all(b" ")?;
                    self.out.write_all(data.as_bytes())?;
                }
                self.out.write_all(b"?>")?;
                Ok(())
            }
            other => Err(SerializeError::NotSerializable(other)),
        }
    }

    // ── Elements ─────────────────────────────────────────────────────

    /// Writes `<qname` plus the declarations this element introduces and all of
    /// its attributes, leaving the caller to add `>` or `/>`.
    ///
    /// Returns the element's `xml:space` if it carries one, read off the
    /// attributes as they are written rather than in a second walk.
    fn write_start_tag<N: DomNavigator>(
        &mut self,
        nav: &N,
    ) -> Result<Option<bool>, SerializeError> {
        self.out.write_all(b"<")?;
        self.write_qname(nav.prefix(), nav.local_name())?;
        self.write_namespace_declarations(nav)?;

        // Invariant, not fixup: the name we just wrote must read back the same.
        let (prefix, uri) = (nav.prefix(), nav.namespace_uri());
        if !self.is_bound(prefix, uri) {
            return Err(SerializeError::UnboundName {
                prefix: prefix.to_string(),
                uri: uri.to_string(),
            });
        }

        let watch_space = self.opts.indent.is_some();
        let mut space = None;
        let mut attr = nav.clone();
        if attr.move_to_first_attribute() {
            loop {
                let (prefix, uri) = (attr.prefix(), attr.namespace_uri());
                if watch_space && uri == XML_NAMESPACE && attr.local_name() == "space" {
                    space = xml_space_value(&attr.value_ref());
                }
                // An unprefixed attribute is in no namespace: the default
                // namespace does not apply to it (Namespaces in XML §6.2).
                let bound = if prefix.is_empty() {
                    uri.is_empty()
                } else {
                    self.is_bound(prefix, uri)
                };
                if !bound {
                    return Err(SerializeError::UnboundName {
                        prefix: prefix.to_string(),
                        uri: uri.to_string(),
                    });
                }
                self.out.write_all(b" ")?;
                self.write_qname(prefix, attr.local_name())?;
                self.out.write_all(b"=\"")?;
                self.write_attribute_value(&attr.value_ref())?;
                self.out.write_all(b"\"")?;
                if !attr.move_to_next_attribute() {
                    break;
                }
            }
        }
        Ok(space)
    }

    /// Emits exactly the declarations that change the scope: the element's
    /// in-scope bindings minus what is already bound identically, plus
    /// `xmlns=""` when the element leaves an inherited default namespace.
    ///
    /// The declarations are collected onto the in-scope stack first and written
    /// from there in prefix order (the default namespace first). The
    /// `namespace::` axis has no defined order and the backends disagree on the
    /// one they use, so sorting is what makes the output a function of the tree
    /// rather than of the navigator behind it. It needs no allocation of its
    /// own: the entries are sorted in place, above this element's watermark.
    fn write_namespace_declarations<N: DomNavigator>(
        &mut self,
        nav: &N,
    ) -> Result<(), SerializeError> {
        let watermark = self.ns_stack.len();
        let mut axis_has_default = false;
        let mut ns = nav.clone();
        if ns.move_to_first_namespace(NamespaceAxisScope::ExcludeXml) {
            loop {
                let prefix = ns.local_name();
                let uri = ns.value_ref();
                // `ExcludeXml` already drops it; belt and braces, because the
                // `xml` prefix must never be declared.
                if prefix != XML_PREFIX {
                    if prefix.is_empty() {
                        if uri.is_empty() {
                            // An undeclared default namespace, recorded by the
                            // tree; handled below, where the inherited scope is
                            // known.
                        } else {
                            axis_has_default = true;
                            if !self.is_bound("", &uri) {
                                self.push_binding("", &uri);
                            }
                        }
                    } else if !uri.is_empty() && !self.is_bound(prefix, &uri) {
                        self.push_binding(prefix, &uri);
                    }
                    // A prefixed binding to the empty URI is an XML 1.1
                    // undeclaration, which is never written.
                }
                if !ns.move_to_next_namespace(NamespaceAxisScope::ExcludeXml) {
                    break;
                }
            }
        }
        if !axis_has_default && self.has_default_namespace() {
            self.push_binding("", "");
        }

        self.ns_stack[watermark..].sort_by(|(a, _), (b, _)| a.cmp(b));
        // Disjoint field borrows: the writer and the stack are independent.
        let out = &mut self.out;
        for (prefix, uri) in &self.ns_stack[watermark..] {
            write_declaration(out, prefix, uri)?;
        }
        Ok(())
    }

    fn write_end_tag<N: DomNavigator>(&mut self, nav: &N) -> Result<(), SerializeError> {
        self.out.write_all(b"</")?;
        self.write_qname(nav.prefix(), nav.local_name())?;
        self.out.write_all(b">")?;
        Ok(())
    }

    fn write_qname(&mut self, prefix: &str, local: &str) -> Result<(), SerializeError> {
        write_qname(&mut self.out, prefix, local)
    }

    fn write_text(&mut self, text: &str) -> Result<(), SerializeError> {
        write_text(&mut self.out, text)
    }

    fn write_attribute_value(&mut self, value: &str) -> Result<(), SerializeError> {
        write_attribute_value(&mut self.out, value)
    }
}

// ── Layout helpers, independent of the writer ────────────────────────────

fn is_element(kind: DomNodeType) -> bool {
    kind == DomNodeType::Element
}

/// Whether any direct child of this element is a text node — rule 1 of the
/// whitespace contract, which looks at *all* the children: text after an
/// element suppresses layout just as text before one does.
fn has_text_child<N: DomNavigator>(nav: &N) -> bool {
    let mut child = nav.clone();
    if !child.move_to_first_child() {
        return false;
    }
    loop {
        if child.node_type().is_text_like() {
            return true;
        }
        if !child.move_to_next_sibling() {
            return false;
        }
    }
}

/// `xml:space` on this element, by its expanded XML-namespace name.
fn xml_space<N: DomNavigator>(nav: &N) -> Option<bool> {
    let mut attr = nav.clone();
    if attr.move_to_first_attribute() {
        loop {
            if attr.namespace_uri() == XML_NAMESPACE && attr.local_name() == "space" {
                return xml_space_value(&attr.value_ref());
            }
            if !attr.move_to_next_attribute() {
                break;
            }
        }
    }
    None
}

/// `Some(true)` for `preserve`, `Some(false)` for `default`; any other value is
/// not one XML 1.0 §2.10 defines, so it changes nothing.
fn xml_space_value(value: &str) -> Option<bool> {
    match value {
        "preserve" => Some(true),
        "default" => Some(false),
        _ => None,
    }
}

// ── Writing, independent of the cursor ───────────────────────────────────

fn write_declaration<W: Write>(out: &mut W, prefix: &str, uri: &str) -> Result<(), SerializeError> {
    check_chars(prefix, "name")?;
    if prefix.is_empty() {
        out.write_all(b" xmlns=\"")?;
    } else {
        out.write_all(b" xmlns:")?;
        out.write_all(prefix.as_bytes())?;
        out.write_all(b"=\"")?;
    }
    write_attribute_value(out, uri)?;
    out.write_all(b"\"")?;
    Ok(())
}

fn write_qname<W: Write>(out: &mut W, prefix: &str, local: &str) -> Result<(), SerializeError> {
    check_chars(prefix, "name")?;
    check_chars(local, "name")?;
    if !prefix.is_empty() {
        out.write_all(prefix.as_bytes())?;
        out.write_all(b":")?;
    }
    out.write_all(local.as_bytes())?;
    Ok(())
}

fn write_text<W: Write>(out: &mut W, text: &str) -> Result<(), SerializeError> {
    let mut written = 0;
    for (offset, ch) in text.char_indices() {
        let escape = match ch {
            '&' => "&amp;",
            '<' => "&lt;",
            // Always escaped, which covers the `]]>` prohibition of XML 1.0
            // §2.4 without scanning for it.
            '>' => "&gt;",
            // A literal CR would be turned into LF by the end-of-line handling
            // of XML 1.0 §2.11 on re-parse.
            '\r' => "&#xD;",
            c if is_forbidden_char(c) => {
                return Err(SerializeError::InvalidChar {
                    code_point: c as u32,
                    context: "text",
                })
            }
            _ => continue,
        };
        out.write_all(&text.as_bytes()[written..offset])?;
        out.write_all(escape.as_bytes())?;
        written = offset + ch.len_utf8();
    }
    out.write_all(&text.as_bytes()[written..])?;
    Ok(())
}

fn write_attribute_value<W: Write>(out: &mut W, value: &str) -> Result<(), SerializeError> {
    let mut written = 0;
    for (offset, ch) in value.char_indices() {
        let escape = match ch {
            '&' => "&amp;",
            '<' => "&lt;",
            // The quote character, because values are always `"`-quoted.
            '"' => "&quot;",
            // Whitespace would be collapsed to a space by the attribute-value
            // normalization of XML 1.0 §3.3.3 on re-parse.
            '\t' => "&#x9;",
            '\n' => "&#xA;",
            '\r' => "&#xD;",
            c if is_forbidden_char(c) => {
                return Err(SerializeError::InvalidChar {
                    code_point: c as u32,
                    context: "attribute value",
                })
            }
            _ => continue,
        };
        out.write_all(&value.as_bytes()[written..offset])?;
        out.write_all(escape.as_bytes())?;
        written = offset + ch.len_utf8();
    }
    out.write_all(&value.as_bytes()[written..])?;
    Ok(())
}

/// Rejects content that cannot be written at all, for the contexts that are
/// copied out verbatim (comments, PI targets and data, names).
fn check_chars(text: &str, context: &'static str) -> Result<(), SerializeError> {
    match text.chars().find(|&c| is_forbidden_char(c)) {
        Some(c) => Err(SerializeError::InvalidChar {
            code_point: c as u32,
            context,
        }),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{BufferDocument, BufferDocumentBuilder, BufferDocumentOptions};
    use crate::namespace::NameTable;
    use crate::navigator::RoXmlNavigator;
    use bumpalo::Bump;

    /// Parses `xml` into a `BufferDocument` and returns its serialization.
    fn round(xml: &str) -> String {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = BufferDocument::from_reader_default(xml.as_bytes(), &arena, &names)
            .expect("the fixture parses");
        to_string(&doc.create_navigator(), &SerializeOptions::default()).expect("serializes")
    }

    /// Serializes `xml` through the roxmltree backend instead.
    fn round_roxml(xml: &str) -> String {
        let doc = roxmltree::Document::parse(xml).expect("the fixture parses");
        to_string(&RoXmlNavigator::new(&doc), &SerializeOptions::default()).expect("serializes")
    }

    /// Just the indentation width, everything else default.
    fn indented(width: usize) -> SerializeOptions {
        SerializeOptions {
            indent: Some(width),
            ..SerializeOptions::default()
        }
    }

    /// Parses `xml` and serializes it with the given options.
    fn round_with(xml: &str, opts: &SerializeOptions) -> String {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = BufferDocument::from_reader_default(xml.as_bytes(), &arena, &names)
            .expect("the fixture parses");
        to_string(&doc.create_navigator(), opts).expect("serializes")
    }

    /// The same through the roxmltree backend.
    fn round_roxml_with(xml: &str, opts: &SerializeOptions) -> String {
        let doc = roxmltree::Document::parse(xml).expect("the fixture parses");
        to_string(&RoXmlNavigator::new(&doc), opts).expect("serializes")
    }

    /// Serializes the element reached by `descend` steps of `move_to_first_child`
    /// from the document root — a subtree, still attached to its tree.
    fn round_subtree(xml: &str, descend: usize, opts: &SerializeOptions) -> String {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = BufferDocument::from_reader_default(xml.as_bytes(), &arena, &names)
            .expect("the fixture parses");
        let mut nav = doc.create_navigator();
        for _ in 0..descend {
            assert!(nav.move_to_first_child(), "not that deep");
        }
        to_string(&nav, opts).expect("serializes")
    }

    /// Builds a tree by hand, so that content XML cannot express can be tested.
    fn built<F>(f: F) -> Result<String, SerializeError>
    where
        F: FnOnce(&mut BufferDocumentBuilder<'_>),
    {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder =
            BufferDocumentBuilder::new(&arena, &names, None, BufferDocumentOptions::default())
                .expect("builder");
        f(&mut builder);
        let doc = builder.finalize().expect("finalize");
        to_string(&doc.create_navigator(), &SerializeOptions::default())
    }

    // ── Escaping matrix (§4.2) ───────────────────────────────────────

    #[test]
    fn text_escapes_ampersand_lt_and_gt() {
        // `>` is escaped unconditionally, which is what makes the `]]>`
        // prohibition of XML 1.0 §2.4 impossible to hit.
        assert_eq!(
            round("<a>&amp; &lt; &gt; ]]&gt; ' \"</a>"),
            "<a>&amp; &lt; &gt; ]]&gt; ' \"</a>"
        );
    }

    #[test]
    fn text_escapes_carriage_return_only() {
        // CR must become a reference (XML 1.0 §2.11 would fold it to LF);
        // tab and LF survive as themselves in text.
        let out = built(|b| {
            b.start_element("a", "", "", &[]).unwrap();
            b.end_of_attributes();
            b.text("x\ry\tz\nw");
            b.end_element().unwrap();
        })
        .expect("serializes");
        assert_eq!(out, "<a>x&#xD;y\tz\nw</a>");
    }

    #[test]
    fn attribute_escapes_quote_lt_and_ampersand_but_not_gt() {
        assert_eq!(
            round(r#"<a k="&amp; &lt; &gt; &quot; '"/>"#),
            r#"<a k="&amp; &lt; > &quot; '"/>"#
        );
    }

    #[test]
    fn attribute_escapes_all_three_whitespace_characters() {
        // Written as references because XML 1.0 §3.3.3 would otherwise
        // normalize them to plain spaces on re-parse.
        let out = built(|b| {
            b.start_element("a", "", "", &[]).unwrap();
            b.attribute("k", "", "", "x\ty\nz\rw").unwrap();
            b.end_of_attributes();
            b.end_element().unwrap();
        })
        .expect("serializes");
        assert_eq!(out, r#"<a k="x&#x9;y&#xA;z&#xD;w"/>"#);
        // And the round trip holds: a re-parse gives the value back.
        let doc = roxmltree::Document::parse(&out).expect("output parses");
        assert_eq!(doc.root_element().attribute("k"), Some("x\ty\nz\rw"));
    }

    #[test]
    fn text_and_attribute_escaping_differ_only_where_specified() {
        // One fixture, both contexts: `>` in text vs `>` in an attribute.
        assert_eq!(round(r#"<a k="&gt;">&gt;</a>"#), r#"<a k=">">&gt;</a>"#);
    }

    // ── Refused content ──────────────────────────────────────────────

    #[test]
    fn forbidden_character_in_text_is_an_error() {
        let err = built(|b| {
            b.start_element("a", "", "", &[]).unwrap();
            b.end_of_attributes();
            b.text("x\u{1}y");
            b.end_element().unwrap();
        })
        .expect_err("U+0001 is not an XML 1.0 Char");
        assert!(matches!(
            err,
            SerializeError::InvalidChar {
                code_point: 1,
                context: "text"
            }
        ));
    }

    #[test]
    fn forbidden_character_in_attribute_value_is_an_error() {
        let err = built(|b| {
            b.start_element("a", "", "", &[]).unwrap();
            b.attribute("k", "", "", "\u{FFFF}").unwrap();
            b.end_of_attributes();
            b.end_element().unwrap();
        })
        .expect_err("U+FFFF is not an XML 1.0 Char");
        assert!(matches!(
            err,
            SerializeError::InvalidChar {
                code_point: 0xFFFF,
                context: "attribute value"
            }
        ));
    }

    #[test]
    fn forbidden_character_in_name_is_an_error() {
        let err = built(|b| {
            b.start_element("a\u{b}b", "", "", &[]).unwrap();
            b.end_of_attributes();
            b.end_element().unwrap();
        })
        .expect_err("U+000B is not an XML 1.0 Char");
        assert!(matches!(
            err,
            SerializeError::InvalidChar {
                code_point: 0xB,
                context: "name"
            }
        ));
    }

    #[test]
    fn comment_with_double_hyphen_is_an_error() {
        let err = built(|b| {
            b.start_element("a", "", "", &[]).unwrap();
            b.end_of_attributes();
            b.comment("x--y").unwrap();
            b.end_element().unwrap();
        })
        .expect_err("XML 1.0 §2.5 forbids `--` inside a comment");
        assert!(matches!(err, SerializeError::InvalidComment(s) if s == "x--y"));
    }

    #[test]
    fn comment_ending_in_hyphen_is_an_error() {
        let err = built(|b| {
            b.start_element("a", "", "", &[]).unwrap();
            b.end_of_attributes();
            b.comment("x-").unwrap();
            b.end_element().unwrap();
        })
        .expect_err("XML 1.0 §2.5 forbids a comment ending in `-`");
        assert!(matches!(err, SerializeError::InvalidComment(s) if s == "x-"));
    }

    #[test]
    fn forbidden_character_in_comment_is_an_error() {
        let err = built(|b| {
            b.start_element("a", "", "", &[]).unwrap();
            b.end_of_attributes();
            b.comment("x\u{0}y").unwrap();
            b.end_element().unwrap();
        })
        .expect_err("NUL is not an XML 1.0 Char");
        assert!(matches!(
            err,
            SerializeError::InvalidChar {
                code_point: 0,
                context: "comment"
            }
        ));
    }

    #[test]
    fn reserved_pi_target_is_an_error() {
        for target in ["xml", "XML", "xMl"] {
            let err = built(|b| {
                b.start_element("a", "", "", &[]).unwrap();
                b.end_of_attributes();
                b.processing_instruction(target, "x").unwrap();
                b.end_element().unwrap();
            })
            .expect_err("XML 1.0 §2.6 reserves the target `xml`");
            assert!(
                matches!(&err, SerializeError::InvalidPi { target: t, .. } if t == target),
                "{err:?}"
            );
        }
    }

    #[test]
    fn pi_data_containing_the_close_delimiter_is_an_error() {
        let err = built(|b| {
            b.start_element("a", "", "", &[]).unwrap();
            b.end_of_attributes();
            b.processing_instruction("go", "x?>y").unwrap();
            b.end_element().unwrap();
        })
        .expect_err("PI data cannot contain `?>`");
        assert!(matches!(
            err,
            SerializeError::InvalidPi {
                reason: "the data contains `?>`",
                ..
            }
        ));
    }

    #[test]
    fn unbound_element_prefix_is_an_error() {
        // A name the output scope cannot express: refused, never repaired.
        let err = built(|b| {
            b.start_element("x", "urn:x", "p", &[]).unwrap();
            b.end_of_attributes();
            b.end_element().unwrap();
        })
        .expect_err("prefix `p` is not declared anywhere");
        assert!(matches!(err, SerializeError::UnboundName { prefix, uri }
            if prefix == "p" && uri == "urn:x"));
    }

    #[test]
    fn unbound_attribute_prefix_is_an_error() {
        let err = built(|b| {
            b.start_element("a", "", "", &[]).unwrap();
            b.attribute("k", "urn:x", "p", "v").unwrap();
            b.end_of_attributes();
            b.end_element().unwrap();
        })
        .expect_err("prefix `p` is not declared anywhere");
        assert!(matches!(err, SerializeError::UnboundName { prefix, .. } if prefix == "p"));
    }

    #[test]
    fn unprefixed_attribute_in_a_namespace_is_an_error() {
        // An unprefixed attribute name always re-parses to no namespace
        // (Namespaces in XML §6.2), so this name cannot be written.
        let err = built(|b| {
            b.start_element("a", "urn:d", "", &[("", "urn:d")]).unwrap();
            b.attribute("k", "urn:d", "", "v").unwrap();
            b.end_of_attributes();
            b.end_element().unwrap();
        })
        .expect_err("an unprefixed attribute cannot be in a namespace");
        assert!(matches!(err, SerializeError::UnboundName { prefix, uri }
            if prefix.is_empty() && uri == "urn:d"));
    }

    #[test]
    fn the_xml_prefix_needs_no_declaration() {
        // `xml` is bound everywhere and is never written, but names using it
        // still pass the scope check.
        assert_eq!(round(r#"<a xml:lang="en"/>"#), r#"<a xml:lang="en"/>"#);
    }

    #[test]
    fn attribute_node_is_not_serializable() {
        let doc = roxmltree::Document::parse(r#"<a k="v"/>"#).expect("parses");
        let mut nav = RoXmlNavigator::new(&doc);
        assert!(nav.move_to_first_child());
        assert!(nav.move_to_first_attribute());
        let err = to_string(&nav, &SerializeOptions::default()).expect_err("attribute is refused");
        assert!(matches!(
            err,
            SerializeError::NotSerializable(DomNodeType::Attribute)
        ));
    }

    #[test]
    fn namespace_node_is_not_serializable() {
        let doc = roxmltree::Document::parse(r#"<a xmlns:p="urn:x"/>"#).expect("parses");
        let mut nav = RoXmlNavigator::new(&doc);
        assert!(nav.move_to_first_child());
        assert!(nav.move_to_first_namespace(NamespaceAxisScope::ExcludeXml));
        let err = to_string(&nav, &SerializeOptions::default()).expect_err("namespace is refused");
        assert!(matches!(
            err,
            SerializeError::NotSerializable(DomNodeType::Namespace)
        ));
    }

    // ── Namespace declarations (§4.3) ────────────────────────────────

    #[test]
    fn redundant_redeclaration_is_dropped() {
        // The child re-declares what it already inherits; the output declares
        // it once, where it is introduced.
        assert_eq!(
            round(r#"<p:a xmlns:p="urn:x"><p:b xmlns:p="urn:x"/></p:a>"#),
            r#"<p:a xmlns:p="urn:x"><p:b/></p:a>"#
        );
    }

    #[test]
    fn shadowing_redeclaration_is_kept() {
        assert_eq!(
            round(r#"<p:a xmlns:p="urn:x"><p:b xmlns:p="urn:y"/></p:a>"#),
            r#"<p:a xmlns:p="urn:x"><p:b xmlns:p="urn:y"/></p:a>"#
        );
    }

    #[test]
    fn default_namespace_undeclaration_is_written() {
        assert_eq!(
            round(r#"<a xmlns="urn:d"><b xmlns=""><c/></b></a>"#),
            r#"<a xmlns="urn:d"><b xmlns=""><c/></b></a>"#
        );
    }

    #[test]
    fn undeclaration_is_not_repeated_on_descendants() {
        let out = round(r#"<a xmlns="urn:d"><b xmlns=""><c><d/></c></b></a>"#);
        assert_eq!(out, r#"<a xmlns="urn:d"><b xmlns=""><c><d/></c></b></a>"#);
        assert_eq!(out.matches(r#"xmlns="""#).count(), 1);
    }

    #[test]
    fn undeclaration_is_not_written_without_a_default_namespace() {
        assert_eq!(round("<a><b/></a>"), "<a><b/></a>");
    }

    #[test]
    fn subtree_gains_the_declarations_it_inherits() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = BufferDocument::from_reader_default(
            r#"<a xmlns:p="urn:x" xmlns:q="urn:y" xmlns="urn:d"><p:b q:k="v"/></a>"#.as_bytes(),
            &arena,
            &names,
        )
        .expect("parses");
        let mut inner = doc.create_navigator();
        assert!(inner.move_to_first_child());
        assert!(inner.move_to_first_child());

        let out = to_string(&inner, &SerializeOptions::default()).expect("serializes");
        // Every binding in scope at <p:b> is re-established on it: the two
        // prefixes it uses and the default namespace it sits in.
        let reparsed = roxmltree::Document::parse(&out).expect("output parses");
        let elem = reparsed.root_element();
        assert_eq!(elem.tag_name().namespace(), Some("urn:x"));
        assert_eq!(elem.tag_name().name(), "b");
        assert_eq!(elem.attribute(("urn:y", "k")), Some("v"));
        assert!(out.contains(r#"xmlns:p="urn:x""#), "{out}");
        assert!(out.contains(r#"xmlns:q="urn:y""#), "{out}");
        assert!(out.contains(r#"xmlns="urn:d""#), "{out}");
    }

    #[test]
    fn declarations_come_out_in_prefix_order() {
        // Source order is not preserved — the axis does not have it to give —
        // but the order is a function of the tree, so both backends agree.
        let xml = r#"<a xmlns:z="urn:z" xmlns:b="urn:b" xmlns="urn:d"/>"#;
        let expected = r#"<a xmlns="urn:d" xmlns:b="urn:b" xmlns:z="urn:z"/>"#;
        assert_eq!(round(xml), expected);
        assert_eq!(round_roxml(xml), expected);
    }

    #[test]
    fn declarations_are_scoped_to_their_element() {
        // `q` is introduced on the first child only, so the second child must
        // not carry it.
        assert_eq!(
            round(r#"<a><b xmlns:q="urn:y"/><c/></a>"#),
            r#"<a><b xmlns:q="urn:y"/><c/></a>"#
        );
    }

    // ── Node kinds (§4.4) ────────────────────────────────────────────

    #[test]
    fn empty_element_is_collapsed() {
        assert_eq!(round("<a><b></b></a>"), "<a><b/></a>");
    }

    #[test]
    fn pi_with_empty_data_omits_the_space() {
        assert_eq!(round("<a><?go?></a>"), "<a><?go?></a>");
        assert_eq!(round("<a><?go now?></a>"), "<a><?go now?></a>");
    }

    #[test]
    fn comments_and_pis_around_the_document_element_are_kept() {
        assert_eq!(
            round("<!--before--><?go?><a/><?stop?><!--after-->"),
            "<!--before--><?go?><a/><?stop?><!--after-->"
        );
    }

    #[test]
    fn mixed_content_keeps_node_order() {
        assert_eq!(
            round("<a>t1<b/>t2<!--c-->t3<?pi d?>t4</a>"),
            "<a>t1<b/>t2<!--c-->t3<?pi d?>t4</a>"
        );
    }

    #[test]
    fn attribute_order_is_document_order() {
        assert_eq!(
            round(r#"<a z="1" m="2" a="3"/>"#),
            r#"<a z="1" m="2" a="3"/>"#
        );
    }

    #[test]
    fn a_single_leaf_node_serializes_on_its_own() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc =
            BufferDocument::from_reader_default("<a>text<!--c--></a>".as_bytes(), &arena, &names)
                .expect("parses");
        let mut nav = doc.create_navigator();
        assert!(nav.move_to_first_child());
        assert!(nav.move_to_first_child());
        assert_eq!(
            to_string(&nav, &SerializeOptions::default()).expect("serializes"),
            "text"
        );
        assert!(nav.move_to_next_sibling());
        assert_eq!(
            to_string(&nav, &SerializeOptions::default()).expect("serializes"),
            "<!--c-->"
        );
    }

    // ── The XML declaration ──────────────────────────────────────────

    #[test]
    fn xml_declaration_is_written_on_request() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc =
            BufferDocument::from_reader_default("<a/>".as_bytes(), &arena, &names).expect("parses");
        let nav = doc.create_navigator();

        let mut out = Vec::new();
        let opts = SerializeOptions {
            xml_declaration: true,
            standalone: None,
            indent: None,
        };
        serialize_document(&nav, &mut out, &opts).expect("serializes");
        assert_eq!(
            String::from_utf8(out).unwrap(),
            r#"<?xml version="1.0" encoding="UTF-8"?><a/>"#
        );

        for (standalone, expected) in [(true, "yes"), (false, "no")] {
            let mut out = Vec::new();
            let opts = SerializeOptions {
                xml_declaration: true,
                standalone: Some(standalone),
                indent: None,
            };
            serialize_document(&nav, &mut out, &opts).expect("serializes");
            assert_eq!(
                String::from_utf8(out).unwrap(),
                format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="{expected}"?><a/>"#)
            );
        }
    }

    #[test]
    fn no_xml_declaration_by_default() {
        assert_eq!(round("<a/>"), "<a/>");
    }

    #[test]
    fn serialize_document_starts_from_the_document_root() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = BufferDocument::from_reader_default("<a><b/></a>".as_bytes(), &arena, &names)
            .expect("parses");
        let mut inner = doc.create_navigator();
        assert!(inner.move_to_first_child());
        assert!(inner.move_to_first_child());

        let mut out = Vec::new();
        serialize_document(&inner, &mut out, &SerializeOptions::default()).expect("serializes");
        assert_eq!(String::from_utf8(out).unwrap(), "<a><b/></a>");
    }

    #[test]
    fn serialize_node_on_the_root_writes_the_document() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = BufferDocument::from_reader_default("<a/><!--x-->".as_bytes(), &arena, &names)
            .expect("parses");
        assert_eq!(round("<a/><!--x-->"), "<a/><!--x-->");
        assert_eq!(doc.create_navigator().node_type(), DomNodeType::Root);
    }

    // ── Backend independence ─────────────────────────────────────────

    #[test]
    fn both_backends_produce_identical_output() {
        // The serializer talks to `DomNavigator` only, so the store behind it
        // cannot change the bytes.
        for xml in [
            "<a/>",
            "<a>t1<b/>t2<!--c--><?pi d?></a>",
            r#"<a z="1" m="2"/>"#,
            r#"<p:a xmlns:p="urn:x" xmlns="urn:d"><p:b p:k="v"><c/></p:b></a>"#
                .replace("</a>", "</p:a>")
                .as_str(),
            r#"<a xmlns="urn:d"><b xmlns=""/></a>"#,
            r#"<a xml:lang="en">&amp;&lt;&gt;</a>"#,
            "<!--before--><a/><!--after-->",
        ] {
            assert_eq!(round(xml), round_roxml(xml), "backends differ on {xml}");
        }
    }

    #[test]
    fn deep_nesting_does_not_recurse() {
        // 50_000 levels: an implementation that descended by recursion would
        // overflow the stack here.
        let depth = 50_000;
        let mut xml = String::new();
        for _ in 0..depth {
            xml.push_str("<a>");
        }
        for _ in 0..depth {
            xml.push_str("</a>");
        }
        // The innermost element is childless, so it collapses.
        let expected = format!(
            "{}<a/>{}",
            "<a>".repeat(depth - 1),
            "</a>".repeat(depth - 1)
        );
        assert_eq!(round(&xml), expected);
    }

    #[test]
    fn both_backends_report_pi_data() {
        // Both navigators report a PI's data as its string value, so the data
        // survives on either backend.
        assert_eq!(round("<a><?pi d?></a>"), "<a><?pi d?></a>");
        assert_eq!(round_roxml("<a><?pi d?></a>"), "<a><?pi d?></a>");
        // Trailing whitespace is part of the data (XML 1.0 §2.6).
        assert_eq!(round("<a><?pi d ?></a>"), "<a><?pi d ?></a>");
        assert_eq!(round_roxml("<a><?pi d ?></a>"), "<a><?pi d ?></a>");
    }

    // ── Formatted output (§4.7) ──────────────────────────────────────

    #[test]
    fn compact_is_the_default_and_adds_nothing() {
        assert_eq!(SerializeOptions::default().indent, None);
        assert_eq!(round("<a><b><c/></b></a>"), "<a><b><c/></b></a>");
    }

    #[test]
    fn compact_mode_is_not_a_minifier() {
        // It adds no whitespace and removes none.
        assert_eq!(round("<a>  <b/>  </a>"), "<a>  <b/>  </a>");
        assert_eq!(round("<a>\n  <b/>\n</a>"), "<a>\n  <b/>\n</a>");
    }

    #[test]
    fn the_catalog_example_writes_both_ways() {
        // §4.7's example, built through the builder API.
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder =
            BufferDocumentBuilder::new(&arena, &names, None, BufferDocumentOptions::default())
                .expect("builder");
        builder.start_element("catalog", "", "", &[]).unwrap();
        builder.end_of_attributes();
        for (id, title) in [("b1", "One"), ("b2", "Two")] {
            builder.start_element("book", "", "", &[]).unwrap();
            builder.attribute("id", "", "", id).unwrap();
            builder.end_of_attributes();
            builder.start_element("title", "", "", &[]).unwrap();
            builder.end_of_attributes();
            builder.text(title);
            builder.end_element().unwrap();
            builder.end_element().unwrap();
        }
        builder.end_element().unwrap();
        let doc = builder.finalize().expect("finalize");
        let nav = doc.create_navigator();

        assert_eq!(
            to_string(&nav, &SerializeOptions::default()).expect("serializes"),
            "<catalog><book id=\"b1\"><title>One</title></book>\
             <book id=\"b2\"><title>Two</title></book></catalog>"
        );
        assert_eq!(
            to_string(&nav, &indented(2)).expect("serializes"),
            "<catalog>\n  \
               <book id=\"b1\">\n    \
                 <title>One</title>\n  \
               </book>\n  \
               <book id=\"b2\">\n    \
                 <title>Two</title>\n  \
               </book>\n\
             </catalog>"
        );
    }

    #[test]
    fn indentation_width_zero_writes_breaks_only() {
        assert_eq!(
            round_with("<a><b><c/></b></a>", &indented(0)),
            "<a>\n<b>\n<c/>\n</b>\n</a>"
        );
    }

    #[test]
    fn indentation_width_two_and_four_nest_per_depth() {
        assert_eq!(
            round_with("<a><b><c/></b></a>", &indented(2)),
            "<a>\n  <b>\n    <c/>\n  </b>\n</a>"
        );
        assert_eq!(
            round_with("<a><b><c/></b></a>", &indented(4)),
            "<a>\n    <b>\n        <c/>\n    </b>\n</a>"
        );
    }

    #[test]
    fn empty_elements_stay_collapsed_when_formatted() {
        assert_eq!(
            round_with("<a><b/><c></c></a>", &indented(2)),
            "<a>\n  <b/>\n  <c/>\n</a>"
        );
        assert_eq!(round_with("<a/>", &indented(2)), "<a/>");
    }

    #[test]
    fn formatted_output_with_and_without_the_declaration() {
        let with_declaration = SerializeOptions {
            xml_declaration: true,
            standalone: None,
            indent: Some(2),
        };
        // A declaration in front of the document element gets a break.
        assert_eq!(
            round_with("<a><b/></a>", &with_declaration),
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<a>\n  <b/>\n</a>"
        );
        // In front of a comment it gets none.
        assert_eq!(
            round_with("<!--c--><a/>", &with_declaration),
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><!--c-->\n<a/>"
        );
        // And no leading break is invented without a declaration.
        assert_eq!(round_with("<a><b/></a>", &indented(2)), "<a>\n  <b/>\n</a>");
    }

    #[test]
    fn document_level_comments_and_pis_when_formatted() {
        // Layout only lands next to an element; a run of comments and PIs is
        // never split internally. No final newline.
        assert_eq!(
            round_with("<!--before--><?go?><a/><?stop?><!--after-->", &indented(2)),
            "<!--before--><?go?>\n<a/>\n<?stop?><!--after-->"
        );
    }

    #[test]
    fn comments_and_pis_inside_an_element_when_formatted() {
        assert_eq!(
            round_with("<a><!--c--><b/><?pi d?></a>", &indented(2)),
            "<a><!--c-->\n  <b/>\n  <?pi d?></a>"
        );
        // A container of comments alone gets no layout at all.
        assert_eq!(
            round_with("<a><!--c--><!--d--></a>", &indented(2)),
            "<a><!--c--><!--d--></a>"
        );
    }

    #[test]
    fn standalone_nodes_get_no_layout() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = BufferDocument::from_reader_default(
            "<a>text<!--c--><?go d?></a>".as_bytes(),
            &arena,
            &names,
        )
        .expect("parses");
        let mut nav = doc.create_navigator();
        assert!(nav.move_to_first_child());
        assert!(nav.move_to_first_child());
        for expected in ["text", "<!--c-->", "<?go d?>"] {
            assert_eq!(to_string(&nav, &indented(2)).expect("serializes"), expected);
            nav.move_to_next_sibling();
        }
    }

    #[test]
    fn a_subtree_is_formatted_from_depth_zero() {
        assert_eq!(
            round_subtree("<r><box><a/></box></r>", 2, &indented(2)),
            "<box>\n  <a/>\n</box>"
        );
    }

    #[test]
    fn text_anywhere_in_a_container_suppresses_layout() {
        // Both of these stay on one line; inspecting only the first child would
        // get the second one wrong.
        for xml in [
            "<p>Hello <b>world</b>!</p>",
            "<p><b>world</b>!</p>",
            "<p><b/>tail</p>",
        ] {
            assert_eq!(round_with(xml, &indented(2)), xml, "{xml}");
        }
        // Suppression reaches the whole subtree, not just the mixed container.
        assert_eq!(
            round_with("<p>t<b><c/></b></p>", &indented(2)),
            "<p>t<b><c/></b></p>"
        );
        // A sibling with text does not suppress its neighbours.
        assert_eq!(
            round_with("<r><p>t</p><q><c/></q></r>", &indented(2)),
            "<r>\n  <p>t</p>\n  <q>\n    <c/>\n  </q>\n</r>"
        );
    }

    #[test]
    fn whitespace_only_text_is_preserved_and_suppresses_layout() {
        assert_eq!(round_with("<p> <b/> </p>", &indented(2)), "<p> <b/> </p>");
    }

    #[test]
    fn formatting_does_not_reindent_existing_layout() {
        // An already-indented document has whitespace text children, so its
        // layout is kept exactly as it is rather than rewritten.
        let xml = "<a>\n    <b/>\n</a>";
        assert_eq!(round_with(xml, &indented(2)), xml);
    }

    #[test]
    fn xml_space_preserve_disables_added_layout() {
        let xml = "<pre xml:space=\"preserve\"><a/><b/></pre>";
        assert_eq!(round_with(xml, &indented(2)), xml);
        // And it is inherited by descendants.
        let xml = "<pre xml:space=\"preserve\"><a><b/></a></pre>";
        assert_eq!(round_with(xml, &indented(2)), xml);
    }

    #[test]
    fn a_descendant_xml_space_default_restores_layout() {
        assert_eq!(
            round_with(
                "<r xml:space=\"preserve\"><a><b xml:space=\"default\"><c/></b></a></r>",
                &indented(2)
            ),
            "<r xml:space=\"preserve\"><a><b xml:space=\"default\">\n      <c/>\n    </b></a></r>"
        );
    }

    #[test]
    fn an_attached_subtree_inherits_xml_space_from_its_ancestors() {
        // The ancestors are consulted for the inherited value; no attribute is
        // invented on the subtree's top element.
        assert_eq!(
            round_subtree(
                "<r xml:space=\"preserve\"><box><a/><b/></box></r>",
                2,
                &indented(2)
            ),
            "<box><a/><b/></box>"
        );
        // The nearest ancestor wins, so a `default` in between restores layout.
        assert_eq!(
            round_subtree(
                "<r xml:space=\"preserve\"><m xml:space=\"default\"><box><a/></box></m></r>",
                3,
                &indented(2)
            ),
            "<box>\n  <a/>\n</box>"
        );
        // Without an ancestor setting, the same subtree is formatted.
        assert_eq!(
            round_subtree("<r><box><a/><b/></box></r>", 2, &indented(2)),
            "<box>\n  <a/>\n  <b/>\n</box>"
        );
    }

    #[test]
    fn string_and_writer_output_are_the_same_bytes() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = BufferDocument::from_reader_default(
            "<a><b k=\"v\"><c/></b><!--x--></a>".as_bytes(),
            &arena,
            &names,
        )
        .expect("parses");
        let nav = doc.create_navigator();
        for opts in [SerializeOptions::default(), indented(0), indented(3)] {
            let string = to_string(&nav, &opts).expect("serializes");
            let mut bytes = Vec::new();
            serialize_node(&nav, &mut bytes, &opts).expect("serializes");
            assert_eq!(string.as_bytes(), bytes.as_slice());
            let mut document_bytes = Vec::new();
            serialize_document(&nav, &mut document_bytes, &opts).expect("serializes");
            assert_eq!(string.as_bytes(), document_bytes.as_slice());
        }
    }

    #[test]
    fn serializing_leaves_the_source_tree_unchanged() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc =
            BufferDocument::from_reader_default("<a><b>t</b><c/></a>".as_bytes(), &arena, &names)
                .expect("parses");
        let nav = doc.create_navigator();
        let before = to_string(&nav, &SerializeOptions::default()).expect("serializes");
        let _ = to_string(&nav, &indented(4)).expect("serializes");
        let after = to_string(&nav, &SerializeOptions::default()).expect("serializes");
        assert_eq!(before, after);
        assert_eq!(after, "<a><b>t</b><c/></a>");
    }

    #[test]
    fn formatted_output_reparses_with_its_content_intact() {
        let formatted = round_with(
            "<catalog><book id=\"b1\"><title>One</title></book></catalog>",
            &indented(2),
        );
        let doc = roxmltree::Document::parse(&formatted).expect("output parses");
        let catalog = doc.root_element();
        assert_eq!(catalog.tag_name().name(), "catalog");

        // The added whitespace is exactly one break plus one level, and one
        // break before the closing tag.
        let catalog_text: Vec<&str> = catalog
            .children()
            .filter(|n| n.is_text())
            .map(|n| n.text().unwrap_or(""))
            .collect();
        assert_eq!(catalog_text, vec!["\n  ", "\n"]);

        let book = catalog
            .children()
            .find(|n| n.is_element())
            .expect("the book survived");
        assert_eq!(book.tag_name().name(), "book");
        assert_eq!(book.attribute("id"), Some("b1"));
        let book_text: Vec<&str> = book
            .children()
            .filter(|n| n.is_text())
            .map(|n| n.text().unwrap_or(""))
            .collect();
        assert_eq!(book_text, vec!["\n    ", "\n  "]);

        // Existing text is untouched: no break went inside <title>.
        let title = book
            .children()
            .find(|n| n.is_element())
            .expect("the title survived");
        assert_eq!(title.text(), Some("One"));
        assert_eq!(title.children().count(), 1);
    }

    #[test]
    fn both_backends_format_identically() {
        for xml in [
            "<a><b><c/></b></a>",
            "<a><!--c--><b/><?pi d?></a>",
            "<p>Hello <b>world</b>!</p>",
            "<pre xml:space=\"preserve\"><a/><b/></pre>",
            "<r xml:space=\"preserve\"><a><b xml:space=\"default\"><c/></b></a></r>",
            "<!--before--><a><b/></a><!--after-->",
        ] {
            assert_eq!(
                round_with(xml, &indented(2)),
                round_roxml_with(xml, &indented(2)),
                "backends differ on {xml}"
            );
        }
    }
}
