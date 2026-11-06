//! Top-level `BufferDocument` struct assembling all storage primitives.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use bumpalo::Bump;

use crate::namespace::NameTable;
use crate::schema::SchemaSet;

use super::namespace::{NamespaceScopes, ScopeWalk};
use super::{
    BindingRemapTable, BufferDocumentOptions, DocumentKind, ElementIndex, NamespacePageFactory,
    Node, NodePages, NodeSourceSpans, NodeType, NsRef, QNameTable, StringStore, NULL,
};

/// Monotonic source of [`BufferDocument::serial`] values.
///
/// Starts at 1 so that `0` can be used by callers as a "no document" sentinel.
static NEXT_DOCUMENT_SERIAL: AtomicU64 = AtomicU64::new(1);

/// Hands out the next document creation ordinal.
///
/// Called once per [`BufferDocument`] construction; see
/// [`BufferDocument::serial`] for the guarantees this provides.
pub(crate) fn next_document_serial() -> u64 {
    NEXT_DOCUMENT_SERIAL.fetch_add(1, Ordering::Relaxed)
}

/// The flat index of the node every buffer starts with: the document node the
/// builder allocates first. `BufferDocument::root` normally names it, but
/// [`BufferDocument::set_cta_fragment`] moves the root onto an element, so the
/// two cannot be used interchangeably.
pub(crate) const DOCUMENT_NODE: u32 = 0;

/// The whitespace of XML: space, tab, carriage return and line feed.
///
/// This is the set `fn:normalize-space` and the tokenized attribute types work
/// on, and it is deliberately **not** `char::is_whitespace`, which also matches
/// NEL, NBSP and the Unicode space separators.
#[inline]
fn is_xml_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

/// `value` with XML whitespace stripped at both ends and collapsed inside.
fn normalize_space(value: &str) -> Cow<'_, str> {
    let trimmed = value.trim_matches(is_xml_space);
    if !trimmed.contains(is_xml_space) {
        return Cow::Borrowed(trimmed);
    }
    let mut out = String::with_capacity(trimmed.len());
    let mut pending_space = false;
    for c in trimmed.chars() {
        if is_xml_space(c) {
            pending_space = true;
        } else {
            if pending_space {
                out.push(' ');
                pending_space = false;
            }
            out.push(c);
        }
    }
    Cow::Owned(out)
}

/// The key an id value is filed under, or `None` when it is not usable as one.
///
/// The key is the value normalized as a tokenized attribute type is — XML
/// whitespace stripped at both ends and collapsed inside — which is what makes
/// `xml:id=" d "` answer to `id('d')`. Only the *key* is normalized: the
/// attribute keeps its own value for `string()`, serialization and copying.
///
/// A normalized value that is not a lexical NCName yields `None` and is never
/// indexed, because F&O §15.5.2 says of an is-id node whose value is not a
/// lexical `xs:ID` that "such a node will never be selected".
pub(crate) fn id_index_key(value: &str) -> Option<Cow<'_, str>> {
    let normalized = normalize_space(value);
    match &normalized {
        Cow::Borrowed(s) if crate::namespace::is_ncname(s) => Some(normalized),
        Cow::Owned(s) if crate::namespace::is_ncname(s) => Some(normalized),
        _ => None,
    }
}

/// One element's claim on an id value.
///
/// `attr` is the attribute node that makes `elem` answer to the value — an
/// `xml:id`, or an attribute whose typed value is an `xs:ID` — so that the
/// claim can be withdrawn when that attribute's value or annotation is
/// replaced; it is `elem` itself when the element's own typed value is the
/// `xs:ID`. It is [`NULL`] for a value filed by
/// [`BufferDocumentBuilder::register_xml_id`](super::BufferDocumentBuilder::register_xml_id),
/// which no attribute stands behind and which is never withdrawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IdClaim {
    /// The root of the tree `elem` belongs to (see
    /// [`BufferDocument::tree_root_of`]): the document node in a
    /// [`DocumentKind::Full`] document, the top-level node `elem` descends
    /// from in a [`DocumentKind::Fragment`] buffer.
    pub(crate) tree: u32,
    pub(crate) elem: u32,
    pub(crate) attr: u32,
}

/// Every claim on one id value.
///
/// The scope of an id is one *tree*, and a [`DocumentKind::Fragment`] buffer
/// can hold several top-level trees, so a lookup is by `(tree, value)`. The tree is kept here, beside the
/// element, rather than inside the map key, so that a lookup can borrow a
/// `&str` straight into the map and never allocate.
///
/// Every claim is kept — including a second element's claim on a value its
/// tree already has, which F&O §15.5.2 never selects — because a claim can be
/// withdrawn: when the element that was selected gives the value up, the next
/// one in document order must answer. Lookups therefore take the *smallest*
/// element reference, and node references are handed out in document order, so
/// that is exactly F&O §15.5.2's "the first such element in document order".
#[derive(Debug, Clone)]
pub(crate) enum IdSlot {
    /// One claim — the usual case, and free of indirection.
    One(IdClaim),
    /// Several claims, in the order they were filed. Empty only transiently,
    /// after the last claim was withdrawn; the key is then removed.
    Many(Vec<IdClaim>),
}

impl IdSlot {
    fn claims(&self) -> &[IdClaim] {
        match self {
            IdSlot::One(claim) => std::slice::from_ref(claim),
            IdSlot::Many(claims) => claims,
        }
    }

    /// The first element in document order with this value inside `tree`.
    pub(crate) fn in_tree(&self, tree: u32) -> Option<u32> {
        match self {
            IdSlot::One(claim) => (claim.tree == tree).then_some(claim.elem),
            IdSlot::Many(claims) => claims
                .iter()
                .filter(|claim| claim.tree == tree)
                .map(|claim| claim.elem)
                .min(),
        }
    }

    /// The first element in document order among the claimants that lie in
    /// the node range `[start, end)` — a subtree, for
    /// [`BufferDocument::element_by_id_in_subtree`].
    pub(crate) fn in_range(&self, start: u32, end: u32) -> Option<u32> {
        self.claims()
            .iter()
            .map(|claim| claim.elem)
            .filter(|&elem| elem >= start && elem < end)
            .min()
    }

    /// The first element in document order, across every tree of the buffer.
    fn first(&self) -> u32 {
        self.claims()
            .iter()
            .map(|claim| claim.elem)
            .min()
            .unwrap_or(NULL)
    }

    /// Whether `elem` of `tree` holds a claim of the given kind: made by an
    /// attribute (`manual == false`) or by `register_xml_id` (`manual`).
    pub(crate) fn has_claim(&self, tree: u32, elem: u32, manual: bool) -> bool {
        self.claims()
            .iter()
            .any(|claim| claim.tree == tree && claim.elem == elem && (claim.attr == NULL) == manual)
    }

    /// Records `claim`, and answers whether another element of its tree
    /// already holds the value — i.e. whether the new claim is a duplicate
    /// that lookups will not select while that one stands.
    pub(crate) fn add(&mut self, claim: IdClaim) -> bool {
        let contested = self
            .claims()
            .iter()
            .any(|held| held.tree == claim.tree && held.elem != claim.elem);
        match self {
            IdSlot::One(held) => *self = IdSlot::Many(vec![*held, claim]),
            IdSlot::Many(claims) => claims.push(claim),
        }
        contested
    }

    /// Withdraws the claim `attr` made, if it made one.
    pub(crate) fn withdraw(&mut self, attr: u32) {
        match self {
            IdSlot::One(claim) if claim.attr == attr => *self = IdSlot::Many(Vec::new()),
            IdSlot::One(_) => {}
            IdSlot::Many(claims) => claims.retain(|claim| claim.attr != attr),
        }
    }

    /// Whether no claim is left, so the key can go.
    pub(crate) fn is_empty(&self) -> bool {
        matches!(self, IdSlot::Many(claims) if claims.is_empty())
    }
}

/// Files `claim` under `key`, creating the entry if need be; answers whether
/// another element of the claim's tree already held the value (see
/// [`IdSlot::add`]).
pub(crate) fn file_id_claim(
    index: &mut HashMap<Box<str>, IdSlot>,
    key: &str,
    claim: IdClaim,
) -> bool {
    match index.get_mut(key) {
        Some(slot) => slot.add(claim),
        None => {
            index.insert(key.into(), IdSlot::One(claim));
            false
        }
    }
}

/// Withdraws the claim the attribute node `attr` made under `key`, and drops
/// the key once nothing claims it.
///
/// `key` is what the claim was filed under. A caller recomputes it from the
/// attribute as it stands — its name, its value and its annotation — which is
/// exact: those are the inputs the key was computed from when it was filed, and
/// the builder withdraws the claim *before* it changes any of them.
pub(crate) fn withdraw_id_claim(index: &mut HashMap<Box<str>, IdSlot>, key: &str, attr: u32) {
    if let Some(slot) = index.get_mut(key) {
        slot.withdraw(attr);
        if slot.is_empty() {
            index.remove(key);
        }
    }
}

/// Compact, cache-friendly XML document representation.
///
/// Built on a flat array of 16-byte [`Node`] structs with power-of-2
/// page addressing.  All string data lives in the arena or in the
/// [`StringStore`]; qualified names are deduplicated via [`QNameTable`].
#[allow(dead_code)] // fields used by builder/navigator in later steps
pub struct BufferDocument<'a> {
    pub(crate) arena: &'a Bump,
    /// Creation ordinal, unique within the process — see [`Self::serial`].
    pub(crate) serial: u64,
    pub(crate) kind: DocumentKind,
    pub(crate) names: &'a NameTable,
    pub(crate) nodes: NodePages<'a>,
    pub(crate) qname_table: QNameTable,
    pub(crate) strings: StringStore<'a>,
    pub(crate) binding_remap: BindingRemapTable,
    /// Whether any element or attribute node of this document carries a schema
    /// type annotation — see [`BufferDocument::has_type_annotations`].
    ///
    /// Maintained by
    /// [`BufferDocumentBuilder::set_node_binding`](super::builder::BufferDocumentBuilder::set_node_binding),
    /// the single place a [`NodeSchemaBinding`](super::NodeSchemaBinding) is
    /// ever attached to a node, and by the builder's `clear_node_binding`, the
    /// one place a binding is taken away (the builder counts the annotated
    /// nodes), so it is exact rather than a hint.
    pub(crate) has_type_annotations: bool,
    pub(crate) root: u32,
    pub(crate) options: BufferDocumentOptions,
    // Side tables
    pub(crate) namespace_pages: NamespacePageFactory<'a>,
    pub(crate) xml_namespace: NsRef,
    pub(crate) element_namespaces: HashMap<u32, NsRef>,
    /// Which namespace chain is in scope at each node, and how to walk it
    /// (see [`NamespaceScopes`]); maintained by the builder.
    pub(crate) ns_scopes: NamespaceScopes,
    pub(crate) element_index: ElementIndex,
    pub(crate) source_spans: NodeSourceSpans,
    /// The id index: every element that answers to an id value — through an
    /// `xml:id`, an attribute whose typed value is an `xs:ID`, its own typed
    /// value, or `register_xml_id` — keyed by the normalized value; see
    /// [`id_index_key`] for the key and [`IdSlot`] for the claims and the tree
    /// scope.
    pub(crate) id_elements: HashMap<Box<str>, IdSlot>,
    /// The idrefs index: for each IDREF value, the element and attribute
    /// nodes with the is-idrefs property ([`Node::IS_IDREFS`]) whose string
    /// value has it as a token (F&O 1.0 §15.5.3) — each node once per value,
    /// in filing order (see
    /// [`nodes_by_idref`](Self::nodes_by_idref)).
    pub(crate) idrefs: HashMap<Box<str>, Vec<u32>>,
    pub(crate) schema_set: Option<&'a SchemaSet>,
    /// Document-level base URI surfaced by `BufferDocNavigator::base_uri()`
    /// when no `xml:base` is found and the cursor reaches the document root.
    /// Used by CTA fragment evaluation to expose the instance file URI to
    /// `fn:base-uri(.)` while leaving the static base URI in
    /// `XPathContext::base_uri` free to carry the schema document URI, and by
    /// [`BufferDocument::set_document_base_uri`] for a host that knows where
    /// the document came from.
    pub(crate) fragment_base_uri: Option<&'a str>,
    /// The XML declaration of a document read from text.
    pub(crate) xml_declaration: Option<crate::xml::XmlDeclaration>,
}

impl<'a> BufferDocument<'a> {
    /// The XML declaration of a document read from text (by
    /// `BufferDocument::from_reader`, `BufferDocumentBuilder::build` or the
    /// typed build), if it had one.
    pub fn xml_declaration(&self) -> Option<&crate::xml::XmlDeclaration> {
        self.xml_declaration.as_ref()
    }

    // ── Namespace scope ────────────────────────────────────────────────

    /// The head of the namespace chain in scope at `node` — the chain of the
    /// nearest element at or above it that declares namespaces — or
    /// [`NsRef::NULL`] when none does. Takes the same time at any depth.
    #[inline]
    pub(crate) fn namespace_scope(&self, node: u32) -> NsRef {
        self.ns_scopes.scope_at(node)
    }

    /// The bindings of the chain at `head`, in walk order: each prefix's
    /// binding in force before any other node of that prefix, which the walk
    /// may leave out. Readers keep the first node of each prefix.
    #[inline]
    pub(crate) fn scope_walk(&self, head: NsRef) -> ScopeWalk<'_, 'a> {
        ScopeWalk::new(&self.namespace_pages, &self.ns_scopes, head)
    }

    // ── Accessors ──────────────────────────────────────────────────────

    /// Returns the document kind (full or fragment).
    #[inline]
    pub fn kind(&self) -> DocumentKind {
        self.kind
    }

    /// Returns the construction options.
    #[inline]
    pub fn options(&self) -> &BufferDocumentOptions {
        &self.options
    }

    /// Returns the root node index.
    #[inline]
    pub fn root(&self) -> u32 {
        self.root
    }

    /// Returns this document's **creation ordinal**.
    ///
    /// The serial is assigned once, when the document is constructed, from a
    /// process-wide counter. It is therefore:
    ///
    /// * **unique within the process** — no two live or dead
    ///   `BufferDocument`s of this process share a serial;
    /// * **strictly increasing in creation order** — if `a` was created
    ///   before `b`, then `a.serial() < b.serial()`;
    /// * **not** stable across processes, and **not** derived from the
    ///   document's content.
    ///
    /// It is intended for reproducible cross-document ordering (see
    /// [`BufferDocNavigator::compare_position`], which orders nodes from
    /// distinct trees by serial rather than by heap address) and for
    /// `generate-id()`-style identifiers, where a caller can combine the
    /// serial with a node reference to obtain a cheap unique node name.
    ///
    /// [`BufferDocNavigator::compare_position`]: super::navigator::BufferDocNavigator
    #[inline]
    pub fn serial(&self) -> u64 {
        self.serial
    }

    /// Returns the byte range of `node_ref` in the original XML source.
    ///
    /// Spans are only recorded when the document was built with
    /// [`BufferDocumentOptions::track_source_locations`] enabled; otherwise
    /// this always returns `None`, and only element nodes carry one. The span
    /// *starts* at the element's `<`; it ends after the end tag for an element
    /// written with one, and after the tag itself for an empty-element tag. A
    /// host embedding the parser uses `span.start` to report an error at a line
    /// and column of its own copy of the source text.
    ///
    /// ```no_run
    /// # use xsd_schema::document::{BufferDocument, BufferDocumentOptions};
    /// # use xsd_schema::namespace::NameTable;
    /// # let arena = bumpalo::Bump::new();
    /// # let names = NameTable::new();
    /// let options = BufferDocumentOptions { track_source_locations: true, ..Default::default() };
    /// let doc = BufferDocument::from_reader(b"<a/>".as_slice(), &arena, &names, options, None)?;
    /// let span = doc.source_span(doc.root() + 1);
    /// # Ok::<(), xsd_schema::document::BufferDocumentError>(())
    /// ```
    #[inline]
    pub fn source_span(&self, node_ref: u32) -> Option<crate::parser::location::SourceSpan> {
        self.source_spans.get(node_ref)
    }

    /// Whether this document recorded source spans at all
    /// ([`BufferDocumentOptions::track_source_locations`]).
    #[inline]
    pub fn has_source_spans(&self) -> bool {
        !self.source_spans.is_empty()
    }

    /// Whether **any** element or attribute node of this document carries a
    /// schema type annotation.
    ///
    /// This is the O(1) form of the walk an XPath host would otherwise have to
    /// perform — visiting every element and attribute and asking each one for
    /// [`DomNavigator::type_annotation`] — when it has to reject a typed (or an
    /// untyped) tree, or take a different path for one.
    ///
    /// The answer is **exact**, not a hint: it is `true` if and only if at
    /// least one node of the document would report
    /// `DomNavigator::type_annotation() == Some(_)`. It is maintained at the
    /// one place a binding is attached to a node, so no walk can disagree with
    /// it.
    ///
    /// "Carries a type annotation" means the same thing here as it does for
    /// [`DomNavigator::type_annotation`]: a node of a document that was never
    /// schema-validated reports `None`, which is the XDM `xs:untyped` /
    /// `xs:untypedAtomic` default. Those defaults are therefore *not* counted,
    /// and a freshly parsed document answers `false`. Only element and
    /// attribute nodes can be annotated; every other kind reports `None`
    /// unconditionally and is never counted.
    ///
    /// A document built by copying ([`copy_subtree`]) answers according to the
    /// copy's [`Annotations`] mode: `Annotations::Preserve` carries the
    /// source's bindings over and can make this `true`, while
    /// `Annotations::Strip` — the default — always leaves it `false`.
    ///
    /// [`DomNavigator::type_annotation`]: crate::navigator::DomNavigator::type_annotation
    /// [`copy_subtree`]: super::builder::BufferDocumentBuilder::copy_subtree
    /// [`Annotations`]: super::Annotations
    ///
    /// ```
    /// use xsd_schema::document::BufferDocument;
    /// use xsd_schema::namespace::NameTable;
    ///
    /// let arena = bumpalo::Bump::new();
    /// let names = NameTable::new();
    /// let doc = BufferDocument::from_reader_default(b"<a n=\"1\"/>".as_slice(), &arena, &names)?;
    /// // Parsed, never validated: no node carries an annotation.
    /// assert!(!doc.has_type_annotations());
    /// # Ok::<(), xsd_schema::document::BufferDocumentError>(())
    /// ```
    #[inline]
    pub fn has_type_annotations(&self) -> bool {
        self.has_type_annotations
    }

    /// Sets the **document-level base URI**: the base URI a node of this
    /// document reports once the walk up its `xml:base` ancestors reaches the
    /// document node without finding one.
    ///
    /// A parser has no way to know the URI a document was retrieved from — it
    /// is handed bytes — so a document built by [`BufferDocument::from_reader`]
    /// or by a [`BufferDocumentBuilder`] starts with none, and `fn:base-uri`
    /// on its nodes falls back to the static base URI of the expression. A
    /// host that *does* know where the document came from records it here, and
    /// `fn:base-uri` then reports that URI (still overridden by any `xml:base`
    /// attribute on the node or an ancestor, which is resolved against it).
    ///
    /// The URI must live at least as long as the document; allocate it in the
    /// same arena when it is computed at run time.
    ///
    /// [`BufferDocumentBuilder`]: super::builder::BufferDocumentBuilder
    ///
    /// ```
    /// # use xsd_schema::document::BufferDocument;
    /// # use xsd_schema::namespace::NameTable;
    /// # use xsd_schema::navigator::DomNavigator;
    /// # let arena = bumpalo::Bump::new();
    /// # let names = NameTable::new();
    /// let mut doc = BufferDocument::from_reader_default(b"<a/>".as_slice(), &arena, &names)?;
    /// doc.set_document_base_uri(Some("file:///tmp/a.xml"));
    /// assert_eq!(doc.create_navigator().base_uri(), "file:///tmp/a.xml");
    /// # Ok::<(), xsd_schema::document::BufferDocumentError>(())
    /// ```
    #[inline]
    pub fn set_document_base_uri(&mut self, uri: Option<&'a str>) {
        self.fragment_base_uri = uri;
    }

    /// The document-level base URI, if one was recorded.
    ///
    /// See [`set_document_base_uri`](Self::set_document_base_uri).
    #[inline]
    pub fn document_base_uri(&self) -> Option<&'a str> {
        self.fragment_base_uri
    }

    /// Returns the associated schema set, if any.
    #[inline]
    pub fn schema_set(&self) -> Option<&'a SchemaSet> {
        self.schema_set
    }

    /// Returns the shared name table.
    #[inline]
    pub fn names(&self) -> &'a NameTable {
        self.names
    }

    // ── Navigation helpers ─────────────────────────────────────────────

    /// Returns the first child of `parent` (always `parent + 1`).
    ///
    /// This relies on the document-order layout: the first child node
    /// is stored immediately after its parent.
    #[inline]
    pub fn first_child_of(&self, parent: u32) -> u32 {
        parent + 1
    }

    /// Returns the first content (non-attribute) child of `parent`, or
    /// `None` if the element has no children.
    ///
    /// Attribute pairs precede content children in document order.
    /// This method skips over them by walking the `next_sibling` chain
    /// of attribute nodes until a non-attribute child is found.
    pub fn first_content_child_of(&self, parent: u32) -> Option<u32> {
        let node = self.nodes.get(parent);
        if !node.has_flag(Node::HAS_CHILDREN) {
            return None;
        }
        if !node.has_flag(Node::HAS_ATTRIBUTE) {
            return Some(parent + 1);
        }
        // Walk the attribute next_sibling chain.
        // Each attribute is a 2-node pair (Attribute + ChildValue).
        let mut cursor = parent + 1; // first attribute
        loop {
            let attr = self.nodes.get(cursor);
            if attr.next_sibling == NULL {
                // Last attribute pair — content starts after its ChildValue node.
                return Some(cursor + 2);
            }
            cursor = attr.next_sibling;
        }
    }

    /// Returns the flat index one past the last node in the subtree
    /// rooted at `elem`.
    ///
    /// Walks ancestors until a node with a `next_sibling` is found and
    /// returns that sibling.  If the root is reached without finding a
    /// sibling, returns `self.nodes.len()` (end of document).
    pub fn subtree_end(&self, elem: u32) -> u32 {
        let mut cursor = elem;
        loop {
            let node = self.nodes.get(cursor);
            if node.next_sibling != NULL {
                return node.next_sibling;
            }
            if node.parent == NULL {
                return self.nodes.len();
            }
            cursor = node.parent;
        }
    }

    /// The root of the tree `node` belongs to — the node the id index files a
    /// claim under. `node` must be a node of this document.
    ///
    /// The rule follows what the navigator presents:
    ///
    /// * In a [`DocumentKind::Full`] document the whole document is **one
    ///   tree**: the navigator shows every top-level node — elements, comments,
    ///   processing instructions — under the one document node, and `fn:root()`
    ///   reaches it from all of them. The tree root is the document node.
    /// * In a [`DocumentKind::Fragment`] buffer the document node is only the
    ///   holder of nodes that are handed out as parentless
    ///   ([`BufferDocNavigator::new_orphan`],
    ///   [`BufferDocNavigator::new_assertion`]), so **each child of the
    ///   document node is the root of a tree of its own**, and the document
    ///   node is its own tree root. An element
    ///   [`set_cta_fragment`](Self::set_cta_fragment) has re-rooted (its upward
    ///   link is cut) is a tree root too.
    ///
    /// [`BufferDocNavigator::new_orphan`]: super::navigator::BufferDocNavigator::new_orphan
    /// [`BufferDocNavigator::new_assertion`]: super::navigator::BufferDocNavigator::new_assertion
    pub(crate) fn tree_root_of(&self, node: u32) -> u32 {
        if self.kind == DocumentKind::Full {
            // Nothing re-roots a `Full` document — `set_cta_fragment` turns
            // the document into a `Fragment` — so every node descends from the
            // document node.
            return DOCUMENT_NODE;
        }
        let mut cursor = node;
        loop {
            let parent = self.nodes.get(cursor).parent;
            if parent == NULL || parent == DOCUMENT_NODE {
                return cursor;
            }
            cursor = parent;
        }
    }

    /// Whether `node_ref` is a node of this document.
    #[inline]
    pub(crate) fn has_node(&self, node_ref: u32) -> bool {
        node_ref < self.nodes.len()
    }

    /// Looks up an element node by its id: the value of its `xml:id`, of an
    /// attribute whose typed value is a single `xs:ID`, or its own typed value
    /// when that is a single `xs:ID`.
    ///
    /// The value is normalized the same way the index key is: XML whitespace
    /// is stripped at both ends and collapsed inside, and a value that is not
    /// a lexical NCName never matches.
    ///
    /// When several elements answer to the value, the one that comes **first
    /// in document order** is returned. A [`DocumentKind::Full`] document is
    /// one tree; a [`DocumentKind::Fragment`] buffer can hold several (see
    /// [`get_element_by_id_in_tree`](Self::get_element_by_id_in_tree)), and
    /// this looks across all of them. Use `get_element_by_id_in_tree` to stay
    /// inside one tree.
    pub fn get_element_by_id(&self, id: &str) -> Option<u32> {
        let key = id_index_key(id)?;
        self.id_elements.get(key.as_ref()).map(IdSlot::first)
    }

    /// Looks up an element node by its id **within one tree**, answering with
    /// the first such element in document order.
    ///
    /// `node` is any node of the wanted tree — the tree root is resolved from
    /// it — so a caller can pass the node it already holds. Which nodes make up
    /// one tree depends on the [`DocumentKind`]:
    ///
    /// * In a [`DocumentKind::Full`] document the whole document is one tree,
    ///   as the navigator presents it — every top-level node, a comment or
    ///   processing instruction before the document element included, sits
    ///   under the one document node. Any node of the document finds every id
    ///   of the document, exactly as
    ///   [`get_element_by_id`](Self::get_element_by_id) does.
    /// * In a [`DocumentKind::Fragment`] buffer the document node only holds
    ///   nodes that are handed out as parentless (see
    ///   [`BufferDocNavigator::new_orphan`]): each child of the document node
    ///   is the root of its own tree, and `node` finds only the ids of the
    ///   top-level tree it belongs to. A top-level comment, processing
    ///   instruction or text node is a tree that holds no element.
    ///
    /// Passing the document node means "every tree of this buffer", and then
    /// this is [`get_element_by_id`](Self::get_element_by_id). A `node` that is
    /// not a node of this document answers `None`.
    ///
    /// [`BufferDocNavigator::new_orphan`]: super::navigator::BufferDocNavigator::new_orphan
    pub fn get_element_by_id_in_tree(&self, node: u32, id: &str) -> Option<u32> {
        if !self.has_node(node) {
            return None;
        }
        let key = id_index_key(id)?;
        let slot = self.id_elements.get(key.as_ref())?;
        let tree = self.tree_root_of(node);
        if tree == DOCUMENT_NODE {
            Some(slot.first())
        } else {
            slot.in_tree(tree)
        }
    }

    /// The first element in document order that answers to `id` **inside the
    /// subtree rooted at `root`**, `root` itself included.
    ///
    /// This is the lookup for a navigator that presents `root` as the top of
    /// the visible tree ([`BufferDocNavigator::new_orphan`],
    /// [`BufferDocNavigator::new_assertion`]): the scope of an id is then that
    /// subtree, not the physical tree that holds it, and an element outside
    /// the subtree must neither be returned nor hide a claimant inside it —
    /// so the range is applied to the claims, before the first one is chosen.
    /// A `root` that is not an element or document node holds no element, and
    /// one that is not a node of this document finds nothing.
    ///
    /// [`BufferDocNavigator::new_orphan`]: super::navigator::BufferDocNavigator::new_orphan
    /// [`BufferDocNavigator::new_assertion`]: super::navigator::BufferDocNavigator::new_assertion
    pub(crate) fn element_by_id_in_subtree(&self, root: u32, id: &str) -> Option<u32> {
        if !self.has_node(root) {
            return None;
        }
        if !matches!(
            self.nodes.get(root).node_type(),
            NodeType::Element | NodeType::Root
        ) {
            return None;
        }
        let key = id_index_key(id)?;
        let slot = self.id_elements.get(key.as_ref())?;
        slot.in_range(root, self.subtree_end(root))
    }

    /// The element and attribute nodes with the is-idrefs property whose
    /// string value has `id` as a token (F&O 1.0 §15.5.3), in document order,
    /// inside the range `[start, end)` of node references, or in the whole
    /// buffer when `range` is `None`. `id` is looked up as given: the caller
    /// passes an NCName.
    pub(crate) fn nodes_by_idref(&self, id: &str, range: Option<(u32, u32)>) -> Vec<u32> {
        let Some(nodes) = self.idrefs.get(id) else {
            return Vec::new();
        };
        let mut found: Vec<u32> = match range {
            Some((start, end)) => nodes
                .iter()
                .copied()
                .filter(|&n| n >= start && n < end)
                .collect(),
            None => nodes.clone(),
        };
        found.sort_unstable();
        found
    }

    // ── CTA fragment configuration ─────────────────────────────────────

    /// Reconfigure this document so that `elem_ref` is the root of the
    /// XDM tree visible to the navigator (XSD 1.1 §3.12.4 CTA XDM
    /// instance shape).
    ///
    /// After this call, `move_to_parent()` from `elem_ref` returns `false`
    /// (the synthetic Root node at index 0 is severed from the tree),
    /// `move_to_root()` lands on `elem_ref`, and the `following::*` /
    /// `preceding::*` axes cannot escape the subtree anchored at
    /// `elem_ref`. The optional `base_uri` argument is surfaced by
    /// `BufferDocNavigator::base_uri()` when no `xml:base` is found,
    /// so `fn:base-uri(.)` can return the instance file URI even
    /// though the static base URI in the XPath context is set to the
    /// schema document URI.
    pub(crate) fn set_cta_fragment(&mut self, elem_ref: u32, base_uri: Option<&'a str>) {
        // The namespace scope of a node is that of the nearest declaring
        // element above it in the tree as built (`namespace_scope`). Cutting
        // `elem_ref` loose keeps that answer only if nothing above it declares
        // a namespace — true for the one caller, whose element is a child of
        // the document node.
        debug_assert!(
            {
                let parent = self.nodes.get(elem_ref).parent;
                parent == NULL || self.namespace_scope(parent).is_null()
            },
            "the new root is in the scope of a declaration above it"
        );
        self.kind = DocumentKind::Fragment;
        self.root = elem_ref;
        self.nodes.update(elem_ref, |n| n.parent = NULL);
        self.fragment_base_uri = base_uri;
    }

    // ── Navigator factory ─────────────────────────────────────────────

    /// Creates a navigator positioned at the document root.
    pub fn create_navigator(&self) -> super::navigator::BufferDocNavigator<'_> {
        super::navigator::BufferDocNavigator::new(self, self.root)
    }

    /// Creates a navigator positioned at the given node reference.
    pub fn create_navigator_at(&self, node_ref: u32) -> super::navigator::BufferDocNavigator<'_> {
        super::navigator::BufferDocNavigator::new(self, node_ref)
    }

    // ── Parsing helpers ───────────────────────────────────────────────

    /// Parses an XML document from a reader into a `BufferDocument`.
    pub fn from_reader<R: std::io::BufRead>(
        reader: R,
        arena: &'a Bump,
        names: &'a NameTable,
        options: BufferDocumentOptions,
        schema_set: Option<&'a SchemaSet>,
    ) -> Result<Self, super::BufferDocumentError> {
        let builder =
            super::builder::BufferDocumentBuilder::new(arena, names, schema_set, options)?;
        builder.build(reader)
    }

    /// Parses an XML document from a reader with default options.
    pub fn from_reader_default<R: std::io::BufRead>(
        reader: R,
        arena: &'a Bump,
        names: &'a NameTable,
    ) -> Result<Self, super::BufferDocumentError> {
        Self::from_reader(reader, arena, names, BufferDocumentOptions::default(), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Node, NodeType, NULL};

    /// Helper: builds a minimal `BufferDocument` for testing.
    fn make_doc<'a>(arena: &'a Bump, names: &'a NameTable) -> BufferDocument<'a> {
        BufferDocument {
            arena,
            serial: next_document_serial(),
            kind: DocumentKind::default(),
            names,
            nodes: NodePages::new(arena),
            qname_table: QNameTable::new(),
            strings: StringStore::new(arena),
            binding_remap: BindingRemapTable::new(),
            has_type_annotations: false,
            root: 0,
            options: BufferDocumentOptions::default(),
            namespace_pages: NamespacePageFactory::new(arena),
            xml_namespace: NsRef::NULL,
            element_namespaces: HashMap::new(),
            ns_scopes: NamespaceScopes::default(),
            element_index: ElementIndex::new(),
            source_spans: NodeSourceSpans::new(),
            id_elements: HashMap::new(),
            idrefs: HashMap::new(),
            schema_set: None,
            fragment_base_uri: None,
            xml_declaration: None,
        }
    }

    /// Helper: allocate a node and write it.
    fn push_node(doc: &mut BufferDocument<'_>, node: Node) -> u32 {
        let idx = doc.nodes.alloc().unwrap();
        doc.nodes.set(idx, node);
        idx
    }

    /// Helper: create a node with specific type and flags.
    fn make_node(nt: NodeType, parent: u32, next_sibling: u32, flags: u32) -> Node {
        let mut n = Node::default();
        n.set_node_type(nt);
        n.parent = parent;
        n.next_sibling = next_sibling;
        n.props_type |= flags;
        n
    }

    #[test]
    fn first_child_of() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = make_doc(&arena, &names);
        assert_eq!(doc.first_child_of(0), 1);
        assert_eq!(doc.first_child_of(5), 6);
        assert_eq!(doc.first_child_of(100), 101);
    }

    #[test]
    fn first_content_child_of_no_children() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut doc = make_doc(&arena, &names);

        // Element without HAS_CHILDREN
        let elem = make_node(NodeType::Element, NULL, NULL, 0);
        push_node(&mut doc, elem);

        assert_eq!(doc.first_content_child_of(0), None);
    }

    #[test]
    fn first_content_child_of_no_attrs() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut doc = make_doc(&arena, &names);

        // Element with children but no attributes
        let elem = make_node(NodeType::Element, NULL, NULL, Node::HAS_CHILDREN);
        push_node(&mut doc, elem); // 0

        // First child is text
        let text = make_node(NodeType::Text, 0, NULL, 0);
        push_node(&mut doc, text); // 1

        assert_eq!(doc.first_content_child_of(0), Some(1));
    }

    #[test]
    fn first_content_child_of_with_attrs() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut doc = make_doc(&arena, &names);

        // Element with both attributes and children
        let elem = make_node(
            NodeType::Element,
            NULL,
            NULL,
            Node::HAS_CHILDREN | Node::HAS_ATTRIBUTE,
        );
        push_node(&mut doc, elem); // 0

        // Attribute 1 (pair: Attribute + ChildValue)
        // next_sibling points to the next attribute at index 3
        let attr1 = make_node(NodeType::Attribute, 0, 3, 0);
        push_node(&mut doc, attr1); // 1

        let val1 = make_node(NodeType::ChildValue, 0, NULL, 0);
        push_node(&mut doc, val1); // 2

        // Attribute 2 (pair: Attribute + ChildValue)
        // next_sibling = NULL → last attribute
        let attr2 = make_node(NodeType::Attribute, 0, NULL, 0);
        push_node(&mut doc, attr2); // 3

        let val2 = make_node(NodeType::ChildValue, 0, NULL, 0);
        push_node(&mut doc, val2); // 4

        // Content child (text) at index 5
        let text = make_node(NodeType::Text, 0, NULL, 0);
        push_node(&mut doc, text); // 5

        // first_content_child_of should skip both attribute pairs → index 5
        assert_eq!(doc.first_content_child_of(0), Some(5));
    }

    #[test]
    fn subtree_end_with_sibling() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut doc = make_doc(&arena, &names);

        // Root at 0
        let root = make_node(NodeType::Root, NULL, NULL, Node::HAS_CHILDREN);
        push_node(&mut doc, root); // 0

        // Element at 1, has sibling at 2
        let elem = make_node(NodeType::Element, 0, 2, 0);
        push_node(&mut doc, elem); // 1

        // Sibling element at 2
        let sib = make_node(NodeType::Element, 0, NULL, 0);
        push_node(&mut doc, sib); // 2

        assert_eq!(doc.subtree_end(1), 2);
    }

    #[test]
    fn subtree_end_walks_ancestors() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut doc = make_doc(&arena, &names);

        // Root at 0, has sibling at 5 (hypothetical)
        let root = make_node(NodeType::Root, NULL, NULL, Node::HAS_CHILDREN);
        push_node(&mut doc, root); // 0

        // Parent element at 1, has next_sibling at 4
        let parent = make_node(NodeType::Element, 0, 4, Node::HAS_CHILDREN);
        push_node(&mut doc, parent); // 1

        // Nested child element at 2, no sibling
        let child = make_node(NodeType::Element, 1, NULL, 0);
        push_node(&mut doc, child); // 2

        // Text at 3 (unused, just to fill space)
        let text = make_node(NodeType::Text, 1, NULL, 0);
        push_node(&mut doc, text); // 3

        // Sibling of parent at 4
        let uncle = make_node(NodeType::Element, 0, NULL, 0);
        push_node(&mut doc, uncle); // 4

        // child(2) has no sibling → walk to parent(1) which has sibling 4
        assert_eq!(doc.subtree_end(2), 4);
    }

    #[test]
    fn subtree_end_at_document_end() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut doc = make_doc(&arena, &names);

        // Root at 0, no sibling, parent = NULL
        let root = make_node(NodeType::Root, NULL, NULL, Node::HAS_CHILDREN);
        push_node(&mut doc, root); // 0

        // Single child at 1, no sibling
        let elem = make_node(NodeType::Element, 0, NULL, 0);
        push_node(&mut doc, elem); // 1

        // elem(1) has no sibling → walk to root(0) which has parent = NULL → nodes.len()
        assert_eq!(doc.subtree_end(1), doc.nodes.len());
        assert_eq!(doc.subtree_end(1), 2);
    }

    #[test]
    fn get_element_by_id_found() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut doc = make_doc(&arena, &names);
        doc.id_elements.insert(
            "foo".into(),
            IdSlot::One(IdClaim {
                tree: 1,
                elem: 42,
                attr: 43,
            }),
        );

        assert_eq!(doc.get_element_by_id("foo"), Some(42));
        // The lookup normalizes its argument the way the key was normalized.
        assert_eq!(doc.get_element_by_id("  foo\n"), Some(42));
        // A value that is not a lexical NCName can never match.
        assert_eq!(doc.get_element_by_id("fo o"), None);
    }

    #[test]
    fn id_index_key_normalizes_and_filters() {
        assert_eq!(id_index_key("d").as_deref(), Some("d"));
        assert_eq!(id_index_key(" \t d \r\n").as_deref(), Some("d"));
        // Collapsing an inner run can only ever produce a non-NCName.
        assert_eq!(id_index_key("a \t b").as_deref(), None);
        // U+00A0 is not XML whitespace, and not a NameChar either.
        assert_eq!(id_index_key("a\u{a0}b").as_deref(), None);
        assert_eq!(id_index_key("1abc").as_deref(), None);
        assert_eq!(id_index_key("p:q").as_deref(), None);
        assert_eq!(id_index_key("   ").as_deref(), None);
    }

    fn claim(tree: u32, elem: u32, attr: u32) -> IdClaim {
        IdClaim { tree, elem, attr }
    }

    #[test]
    fn an_id_slot_answers_with_the_first_element_of_each_tree() {
        let mut slot = IdSlot::One(claim(1, 1, 2));
        assert!(slot.add(claim(1, 9, 10)), "the tree already has one");
        assert_eq!(slot.in_tree(1), Some(1));
        assert!(!slot.add(claim(5, 5, 6)), "another tree gets its own entry");
        assert_eq!(slot.in_tree(5), Some(5));
        assert_eq!(slot.in_tree(7), None);
        assert_eq!(slot.first(), 1, "first in document order");
        // A second claim of the same element is not a duplicate.
        let mut own = IdSlot::One(claim(1, 1, 2));
        assert!(!own.add(claim(1, 1, NULL)));
        assert!(own.has_claim(1, 1, true) && own.has_claim(1, 1, false));
    }

    #[test]
    fn withdrawing_a_claim_hands_the_value_to_the_next_element() {
        let mut slot = IdSlot::One(claim(1, 3, 4));
        slot.add(claim(1, 9, 10));
        slot.add(claim(1, 7, 8));
        slot.withdraw(4);
        assert_eq!(slot.in_tree(1), Some(7), "next in document order");
        slot.withdraw(8);
        assert_eq!(slot.in_tree(1), Some(9));
        slot.withdraw(99); // no such claim: nothing changes
        assert_eq!(slot.in_tree(1), Some(9));
        slot.withdraw(10);
        assert!(slot.is_empty());

        let mut one = IdSlot::One(claim(1, 3, 4));
        one.withdraw(5);
        assert!(!one.is_empty());
        one.withdraw(4);
        assert!(one.is_empty());
    }

    #[test]
    fn a_withdrawn_last_claim_takes_its_key_along() {
        let mut index = HashMap::new();
        assert!(!file_id_claim(&mut index, "k", claim(1, 3, 4)));
        assert!(file_id_claim(&mut index, "k", claim(1, 5, 6)));
        withdraw_id_claim(&mut index, "k", 4);
        assert_eq!(index.get("k").and_then(|slot| slot.in_tree(1)), Some(5));
        withdraw_id_claim(&mut index, "k", 6);
        assert!(index.is_empty());
        withdraw_id_claim(&mut index, "absent", 6); // harmless
    }

    #[test]
    fn get_element_by_id_not_found() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = make_doc(&arena, &names);

        assert_eq!(doc.get_element_by_id("nonexistent"), None);
    }

    #[test]
    fn source_span_is_readable_when_tracking_is_on() {
        let xml = "<a>\n  <b/>\n</a>";
        let arena = Bump::new();
        let names = NameTable::new();
        let options = BufferDocumentOptions {
            track_source_locations: true,
            ..Default::default()
        };
        let doc =
            BufferDocument::from_reader(xml.as_bytes(), &arena, &names, options, None).unwrap();

        assert!(doc.has_source_spans());
        // The document element is the node right after the root. Its span runs
        // from its `<` to the end of its end tag.
        let a = doc.root() + 1;
        let span_a = doc.source_span(a).expect("the root element has a span");
        assert_eq!(&xml[span_a.start..span_a.end], xml);
        assert!(xml[span_a.start..].starts_with("<a>"));

        // An empty-element tag's span is exactly that tag.
        use crate::navigator::{DomNavigator, DomNodeType};
        let mut nav = doc.create_navigator_at(a);
        assert!(nav.move_to_first_child());
        while nav.node_type() != DomNodeType::Element {
            assert!(nav.move_to_next_sibling());
        }
        let span_b = doc
            .source_span(nav.current_ref())
            .expect("the child element has a span");
        assert!(span_b.start > span_a.start);
        assert_eq!(&xml[span_b.start..span_b.end], "<b/>");
    }

    #[test]
    fn a_document_level_base_uri_is_reported_and_xml_base_still_wins() {
        use crate::navigator::DomNavigator;
        let arena = Bump::new();
        let names = NameTable::new();
        let mut doc = BufferDocument::from_reader_default(
            br#"<a><plain/><based xml:base="sub/"/></a>"#.as_slice(),
            &arena,
            &names,
        )
        .unwrap();

        // Nothing is recorded by default, which is what every document built
        // before this accessor existed reports.
        assert_eq!(doc.document_base_uri(), None);
        assert_eq!(doc.create_navigator().base_uri(), "");

        doc.set_document_base_uri(Some("file:///tmp/a.xml"));
        assert_eq!(doc.document_base_uri(), Some("file:///tmp/a.xml"));

        // The document node and any node without an `xml:base` ancestor
        // report it.
        let mut nav = doc.create_navigator();
        assert_eq!(nav.base_uri(), "file:///tmp/a.xml");
        assert!(nav.move_to_first_child());
        assert!(nav.move_to_first_child());
        assert_eq!(nav.local_name(), "plain");
        assert_eq!(nav.base_uri(), "file:///tmp/a.xml");

        // An `xml:base` attribute still takes precedence at the node itself.
        assert!(nav.move_to_next_sibling());
        assert_eq!(nav.local_name(), "based");
        assert_eq!(nav.base_uri(), "sub/");

        doc.set_document_base_uri(None);
        assert_eq!(doc.create_navigator().base_uri(), "");
    }

    #[test]
    fn source_span_is_absent_without_tracking() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = BufferDocument::from_reader_default(b"<a/>".as_slice(), &arena, &names).unwrap();

        assert!(!doc.has_source_spans());
        assert_eq!(doc.source_span(doc.root() + 1), None);
    }
}
