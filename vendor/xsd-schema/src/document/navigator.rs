//! Cursor-based [`DomNavigator`] over [`BufferDocument`].
//!
//! [`BufferDocNavigator`] is a lightweight, cloneable cursor that enables
//! XPath 2.0 evaluation over the flat node array.  It implements a
//! three-state cursor model:
//!
//! | State | `virtual_parent` | `current_ns` | Meaning |
//! |-------|------------------|--------------|---------|
//! | Real node | `NULL` | `NsRef::NULL` | Main array node |
//! | Attribute | element ref | `NsRef::NULL` | Attribute virtual node |
//! | Namespace | element ref | non-NULL | Namespace virtual node |

use std::borrow::Cow;
use std::collections::HashSet;

use crate::ids::{NameId, SimpleTypeKey, TypeKey};
use crate::navigator::{
    DomNavigator, DomNodeType, NamespaceAxisScope, NavigatorError, TypedValue, XmlNodeOrder,
};
use crate::schema::SchemaSet;
use crate::validation::info::ContentType;
use crate::validation::simple::validate_simple_type_in;

use super::buffer::BufferDocument;
use super::node::{Node, NodeType};
use super::type_remap::NodeSchemaBinding;
use super::{NsRef, NULL};

/// Lightweight cursor for XPath navigation over [`BufferDocument`].
#[derive(Clone)]
pub struct BufferDocNavigator<'a> {
    doc: &'a BufferDocument<'a>,
    /// Current node position in the main node array.
    current: u32,
    /// Hide the synthetic root's children for XSD assertion absolute paths.
    assertion_absolute_root: bool,
    /// In assertion scope, the asserter element (the visible "fragment root").
    /// `NULL` outside assertion scope. Axis iterators that have to bound a
    /// traversal at the top of the tree (e.g. `preceding`, which walks back
    /// towards it) use this so they stay inside the visible subtree instead
    /// of running into the synthetic root, whose children
    /// `move_to_first_child` deliberately hides.
    assertion_fragment_root: u32,
    /// Non-NULL when positioned on an attribute or namespace (= owning element).
    virtual_parent: u32,
    /// Non-NULL when positioned on a namespace node.
    current_ns: NsRef,
    /// The node this navigator presents as having no parent, or `NULL`.
    /// See [`BufferDocNavigator::new_orphan`].
    orphan_root: u32,
    /// Sub-index for document-order comparison of virtual nodes.
    attr_index: u16,
    /// Collected namespaces for All/ExcludeXml traversal.
    ns_list: Vec<NsRef>,
}

impl<'a> BufferDocNavigator<'a> {
    /// Creates a navigator positioned at the given node.
    pub fn new(doc: &'a BufferDocument<'a>, node: u32) -> Self {
        Self {
            doc,
            current: node,
            assertion_absolute_root: false,
            assertion_fragment_root: NULL,
            orphan_root: NULL,
            virtual_parent: NULL,
            current_ns: NsRef::NULL,
            attr_index: 0,
            ns_list: Vec::new(),
        }
    }

    /// Creates a navigator for XSD 1.1 assertion evaluation.
    ///
    /// XSD 1.1 §3.13.4.1 clause 1.3 builds the assertion's data model instance
    /// from the asserted element `E` alone: "The root node of the \[XDM\]
    /// instance is constructed from E; the data model instance contains only
    /// that node and nodes constructed from the \[attributes\], \[children\], and
    /// descendants of E", with the Note "It is a consequence of this
    /// construction that attempts to refer, in an assertion, to the siblings or
    /// ancestors of E, or to any part of the input document outside of E
    /// itself, will be unsuccessful."
    ///
    /// So the assertion context item is `E`, relative paths such as `.//x`
    /// traverse its subtree, and every step that would leave that subtree
    /// yields nothing: `parent::`, `ancestor::`, `ancestor-or-self::` beyond
    /// `E`, `following-sibling::`, `preceding-sibling::`, `following::` and
    /// `preceding::` are all cut at `E` (see
    /// the private `is_tree_top`). For the same reason
    /// [`find_element_by_id`](DomNavigator::find_element_by_id) — and with it
    /// `fn:id` — answers only with an element inside `E`.
    ///
    /// `fn:root()` and the absolute paths built on it are the one place this
    /// differs from [`new_orphan`](Self::new_orphan): they still land on the
    /// document node, whose child axis is then hidden, so `/x` and `//x` select
    /// **nothing** rather than reaching into `E`. That is what the W3C XSD 1.1
    /// test suite requires — `ibmMeta/assertion.testSet` groups
    /// `d4_3_15ii31` and `d4_3_15ii32`, categorised
    /// `xsd1_1-Assertions-StayInSubtree`, are documented as
    /// *"`//` returns empty sequence"* and expect an instance to be **invalid**
    /// because `count(//ele1) eq 1` and `count(//@attr1) eq 1` are *false*
    /// inside the very subtree that contains one of each.
    pub fn new_assertion(doc: &'a BufferDocument<'a>, node: u32) -> Self {
        Self {
            assertion_absolute_root: true,
            assertion_fragment_root: node,
            ..Self::new(doc, node)
        }
    }

    /// Creates a navigator for a node that must appear to have **no parent**.
    ///
    /// `BufferDocument` always has a document node at the root of the tree,
    /// but the XDM allows a parentless element, attribute, comment,
    /// processing-instruction or text node, and a host that constructs such
    /// nodes has to build them somewhere. Under this constructor `node` is
    /// that parentless node: `move_to_parent` returns `false` there,
    /// `move_to_root` and `move_to_visible_root` land on it rather than on the
    /// document node that physically holds it, and it has no siblings. Its
    /// descendants behave normally and reach it with `move_to_parent`.
    ///
    /// So `parent::`, `ancestor::`, the sibling axes, `following::` and
    /// `preceding::` all stop at `node`, and `fn:root()` and `..` agree that it
    /// is the root of its own tree. The scope of an id is that tree as well:
    /// [`find_element_by_id`](DomNavigator::find_element_by_id) answers with
    /// the first element *inside the subtree of `node`* that has the id.
    ///
    /// [`new_assertion`](Self::new_assertion) cuts the same upward and
    /// sideways links at the asserted element; the two differ only in
    /// `fn:root()` and the absolute paths built on it, which under
    /// `new_assertion` deliberately still reach the hidden document node (and
    /// select nothing below it). Here `fn:root()` lands on `node`, so a leading
    /// `/` or `//` — `fn:root(self::node()) treat as document-node()`, XPath 2.0
    /// §3.2 — raises `XPDY0050` when `node` is not a document node.
    ///
    /// The document node already has no parent and no siblings, so
    /// `new_orphan(doc, doc.root())` is the ordinary navigator
    /// ([`new`](Self::new)).
    ///
    /// # Panics
    ///
    /// Panics if `node` is not a node of `doc`.
    pub fn new_orphan(doc: &'a BufferDocument<'a>, node: u32) -> Self {
        assert!(
            doc.has_node(node),
            "new_orphan: {node} is not a node of this document"
        );
        if node == doc.root || doc.nodes.get(node).node_type() == NodeType::Root {
            return Self::new(doc, node);
        }
        Self {
            assertion_absolute_root: true,
            assertion_fragment_root: node,
            orphan_root: node,
            ..Self::new(doc, node)
        }
    }

    /// A navigator on the attribute node `attr_ref` (the attribute's index
    /// among its element's attributes set, for document order).
    fn at_attribute(doc: &'a BufferDocument<'a>, attr_ref: u32) -> Self {
        let owner = doc.nodes.get(attr_ref).parent;
        let mut index: u16 = 0;
        let mut cursor = owner + 1;
        while cursor != attr_ref && cursor != NULL {
            cursor = doc.nodes.get(cursor).next_sibling;
            index += 1;
        }
        Self {
            virtual_parent: owner,
            attr_index: index,
            ..Self::new(doc, attr_ref)
        }
    }

    /// Returns the underlying document.
    #[inline]
    pub fn document(&self) -> &'a BufferDocument<'a> {
        self.doc
    }

    /// Returns the current flat node index.
    #[inline]
    pub fn current_ref(&self) -> u32 {
        self.current
    }

    // ── Internal helpers ──────────────────────────────────────────────

    #[inline]
    fn is_on_namespace(&self) -> bool {
        !self.current_ns.is_null()
    }

    #[inline]
    fn is_on_attribute(&self) -> bool {
        self.virtual_parent != NULL && self.current_ns.is_null()
    }

    /// Whether this cursor sits on the node that has no parent and no siblings
    /// in the tree this navigator presents: the node declared parentless by
    /// [`new_orphan`](Self::new_orphan), or the asserted element under
    /// [`new_assertion`](Self::new_assertion).
    ///
    /// XSD 1.1 §3.13.4.1 clause 1.3 builds an assertion's data model instance
    /// so that it "contains only that node and nodes constructed from the
    /// [attributes], [children], and descendants of E", with the Note
    /// "attempts to refer, in an assertion, to the siblings or ancestors of E,
    /// … will be unsuccessful". The upward and sideways links of E are
    /// therefore cut exactly as an orphan's are; only `fn:root()` and the
    /// absolute paths that build on it differ between the two constructors
    /// (see [`new_assertion`](Self::new_assertion)).
    ///
    /// An attribute or namespace cursor keeps `current` on its **owning
    /// element**, so `current` equals the boundary node for that element's
    /// attribute and namespace nodes as well as for the node itself. They are
    /// different nodes, and the parent of such a node *is* the boundary node —
    /// the cut applies only to the boundary node's own upward links.
    #[inline]
    fn is_tree_top(&self) -> bool {
        if self.virtual_parent != NULL {
            return false;
        }
        if self.orphan_root != NULL && self.current == self.orphan_root {
            return true;
        }
        self.assertion_fragment_root != NULL && self.current == self.assertion_fragment_root
    }

    #[inline]
    fn node(&self) -> Node {
        self.doc.nodes.get(self.current)
    }

    fn ns_node(&self) -> super::NamespaceNode {
        self.doc.namespace_pages.get(self.current_ns)
    }

    fn clear_virtual(&mut self) {
        self.virtual_parent = NULL;
        self.current_ns = NsRef::NULL;
        self.attr_index = 0;
        self.ns_list.clear();
    }

    /// Restore cursor state from saved values (for position-unchanged-on-false).
    fn restore_cursor(
        &mut self,
        current: u32,
        virtual_parent: u32,
        current_ns: NsRef,
        attr_index: u16,
    ) {
        self.current = current;
        self.virtual_parent = virtual_parent;
        self.current_ns = current_ns;
        self.attr_index = attr_index;
        self.ns_list.clear();
    }

    /// Document-order key for position comparison.
    ///
    /// Within an element: namespaces (1) < attributes (2) < real children (0 at higher index).
    fn order_key(&self) -> (u32, u8, u16) {
        if self.is_on_namespace() {
            (self.virtual_parent, 1, self.attr_index)
        } else if self.is_on_attribute() {
            (self.virtual_parent, 2, self.attr_index)
        } else {
            (self.current, 0, 0)
        }
    }

    fn matches_kind(&self, kind: DomNodeType) -> bool {
        kind == DomNodeType::All || self.node_type() == kind
    }

    fn past_end(&self, end: Option<&Self>) -> bool {
        if let Some(end) = end {
            self.compare_position(end) != XmlNodeOrder::Before
        } else {
            false
        }
    }

    /// Returns `true` when the current element has at least one child element.
    ///
    /// For non-element nodes (attributes, text, etc.) this always returns `false`.
    fn has_element_children(&self) -> bool {
        if self.is_on_namespace() || self.is_on_attribute() {
            return false;
        }
        if let Some(child) = self.doc.first_content_child_of(self.current) {
            let end = self.doc.subtree_end(self.current);
            let mut i = child;
            while i < end {
                let n = self.doc.nodes.get(i);
                if n.node_type() == NodeType::Element {
                    return true;
                }
                i += 1;
            }
        }
        false
    }

    /// Concatenates all descendant text content (XPath string-value for elements/root).
    fn compute_element_value(&self) -> String {
        let end = self.doc.subtree_end(self.current);
        let mut result = String::new();
        let mut i = self.current + 1;
        while i < end {
            let node = self.doc.nodes.get(i);
            match node.node_type() {
                NodeType::Text | NodeType::Whitespace | NodeType::SignificantWhitespace => {
                    result.push_str(self.doc.strings.get(node.value));
                }
                _ => {}
            }
            i += 1;
        }
        result
    }

    /// Borrowing fast path for the element/root string-value.
    ///
    /// When the subtree has at most one text-class descendant — the common case
    /// for simple-content leaves like `<id>42</id>` — its string-value *is* that
    /// single interned `StringStore` slice, so it can be borrowed with no
    /// concatenation. Returns:
    ///
    /// * `Some(slice)` — exactly one text node; borrow it.
    /// * `Some("")` — no text content at all.
    /// * `None` — two or more text nodes; the caller must concatenate via
    ///   [`compute_element_value`](Self::compute_element_value).
    fn single_text_value(&self) -> Option<&str> {
        let end = self.doc.subtree_end(self.current);
        let mut found: Option<&str> = None;
        let mut i = self.current + 1;
        while i < end {
            let node = self.doc.nodes.get(i);
            match node.node_type() {
                NodeType::Text | NodeType::Whitespace | NodeType::SignificantWhitespace => {
                    if found.is_some() {
                        // ≥2 text runs → genuine concatenation required.
                        return None;
                    }
                    found = Some(self.doc.strings.get(node.value));
                }
                _ => {}
            }
            i += 1;
        }
        Some(found.unwrap_or(""))
    }

    /// Walks ancestors looking for `xml:base` attribute.
    fn resolve_base_uri(&self) -> &'a str {
        let mut nav = self.clone();
        loop {
            let node = nav.node();
            if node.node_type() == NodeType::Element && node.has_flag(Node::HAS_ATTRIBUTE) {
                let mut attr = nav.current + 1;
                loop {
                    let attr_node = self.doc.nodes.get(attr);
                    if attr_node.node_type() != NodeType::Attribute {
                        break;
                    }
                    let qname = self.doc.qname_table.get(attr_node.value);
                    let local = self.doc.names.resolve_ref(qname.local_name);
                    if local == "base" {
                        let ns = self.doc.names.resolve_ref(qname.namespace_uri);
                        if ns == "http://www.w3.org/XML/1998/namespace" {
                            let val_node = self.doc.nodes.get(attr + 1);
                            return self.doc.strings.get(val_node.value);
                        }
                    }
                    if attr_node.next_sibling == NULL {
                        break;
                    }
                    attr = attr_node.next_sibling;
                }
            }
            if !nav.move_to_parent() {
                break;
            }
        }
        // No xml:base found while walking up. CTA fragment evaluation
        // installs the instance file URI as `fragment_base_uri` so
        // `fn:base-uri(.)` reports the instance URI even though the
        // synthetic XDM tree has no `xml:base` to anchor it (§3.12.4).
        self.doc.fragment_base_uri.unwrap_or("")
    }

    /// Finds the NsRef boundary for Local namespace scope on `elem`: the
    /// chain in scope at its parent (the chain head of the nearest ancestor
    /// with `HAS_NMSP_DECLS`), where `elem`'s own declarations end.  Returns
    /// `NsRef::NULL` if no ancestor has namespace declarations.
    fn find_local_ns_boundary(&self, elem: u32) -> NsRef {
        match self.doc.nodes.get(elem).parent {
            NULL => NsRef::NULL,
            parent => self.doc.namespace_scope(parent),
        }
    }

    /// Collects namespace NsRefs for the given scope.
    ///
    /// - **Local**: Only namespace nodes declared on `elem` itself (stops at
    ///   the nearest ancestor's namespace chain boundary).
    /// - **All**: All in-scope namespaces including inherited, plus the
    ///   implicit `xml:` binding.
    /// - **ExcludeXml**: Like All but without the `xml:` namespace.
    ///
    /// `All` and `ExcludeXml` read one chain, the one in scope at `elem`
    /// (`BufferDocument::namespace_scope`): it holds the declarations of
    /// `elem` and of every ancestor, innermost first, so the first node of
    /// each prefix is the binding in force. Finding the chain and walking it
    /// take the same time at any depth (see `NamespaceScopes`).
    fn collect_namespaces(&self, elem: u32, scope: NamespaceAxisScope) -> Vec<NsRef> {
        match scope {
            NamespaceAxisScope::Local => {
                let node = self.doc.nodes.get(elem);
                if !node.has_flag(Node::HAS_NMSP_DECLS) {
                    return Vec::new();
                }
                let head = match self.doc.element_namespaces.get(&elem) {
                    Some(&h) => h,
                    None => return Vec::new(),
                };
                let boundary = self.find_local_ns_boundary(elem);
                let mut result = Vec::new();
                let mut ns_ref = head;
                while !ns_ref.is_null() && ns_ref != boundary {
                    result.push(ns_ref);
                    ns_ref = self.doc.namespace_pages.get(ns_ref).next;
                }
                result
            }
            NamespaceAxisScope::All | NamespaceAxisScope::ExcludeXml => {
                // Lazily allocate the result Vec and the dedup set: an element
                // with no declarations on any ancestor-or-self collects nothing,
                // so for `ExcludeXml` both stay unallocated and we return empty
                // — the common case on the validation hot path (which uses
                // `ExcludeXml`, see `build_ns_snapshot`). `All` follows the XDM
                // contract and *always* surfaces the implicit `xml:` binding
                // (appended below), so it allocates a one-element Vec even on a
                // namespace-free element — that is fine, `All` is only reached by
                // the (deprecated) `namespace::` axis, not validation.
                let mut result: Vec<NsRef> = Vec::new();
                let mut seen_prefixes: Option<HashSet<NameId>> = None;

                for (ns_ref, ns_node) in self.doc.scope_walk(self.doc.namespace_scope(elem)) {
                    let seen = seen_prefixes.get_or_insert_with(HashSet::new);
                    if !seen.insert(ns_node.prefix) {
                        continue;
                    }
                    if scope == NamespaceAxisScope::ExcludeXml
                        && self.doc.names.resolve_ref(ns_node.prefix) == "xml"
                    {
                        continue;
                    }
                    // A zero-length URI is a namespace *undeclaration*
                    // (`xmlns=""`). It cancels an outer binding — it must
                    // stay in `seen` so the outer one is shadowed — but it
                    // is the absence of a binding, so under `All` (the XDM
                    // `dm:namespace-nodes` accessor behind the `namespace::`
                    // axis and `fn:in-scope-prefixes`) no namespace node
                    // exists for it. `Local` and `ExcludeXml` are views of
                    // the *declarations* — serialization, shallow copy and
                    // the namespace context that resolves QNames during
                    // validation — and must keep it.
                    if scope == NamespaceAxisScope::All
                        && self.doc.names.resolve_ref(ns_node.namespace_uri).is_empty()
                    {
                        continue;
                    }
                    result.push(ns_ref);
                }

                // XDM contract: `All` always exposes the implicit `xml:` binding,
                // regardless of whether any real namespace is in scope. Dedup
                // against an explicitly declared `xml` prefix.
                if scope == NamespaceAxisScope::All {
                    let xml_ns = self.doc.xml_namespace;
                    if !xml_ns.is_null() {
                        let xml_node = self.doc.namespace_pages.get(xml_ns);
                        let seen = seen_prefixes.get_or_insert_with(HashSet::new);
                        if seen.insert(xml_node.prefix) {
                            result.push(xml_ns);
                        }
                    }
                }

                result
            }
        }
    }

    // ── Schema binding accessors ──────────────────────────────────────

    /// Returns the full [`TypeKey`] from the current node's schema binding.
    ///
    /// Returns `None` when positioned on a namespace virtual node.
    pub fn element_type_key(&self) -> Option<TypeKey> {
        if self.is_on_namespace() {
            return None;
        }
        let idx = self.node().binding_index();
        self.doc.binding_remap.get(idx).map(|b| b.type_key)
    }

    /// Returns the full [`NodeSchemaBinding`] for the current node.
    ///
    /// Returns `None` when positioned on a namespace virtual node.
    pub fn schema_binding(&self) -> Option<&NodeSchemaBinding> {
        if self.is_on_namespace() {
            return None;
        }
        let idx = self.node().binding_index();
        self.doc.binding_remap.get(idx)
    }

    /// Post-process a typed value for QName/Notation: resolve the lexical
    /// form against the navigator's in-scope namespaces if the inner atomic
    /// is still stored as a `String` (the simple-type validator has no
    /// namespace context, so QName/Notation values come back lexically) —
    /// the value itself, a union's member value, and every member of a list
    /// (of `xs:QName`, or of a union with a QName or NOTATION member type).
    ///
    /// Returns the value unchanged for any other type or shape, and a lexical
    /// form that does not resolve unchanged.
    fn resolve_qname_typed_value(
        &self,
        value: crate::types::value::XmlValue,
    ) -> crate::types::value::XmlValue {
        use crate::types::value::{XmlValue, XmlValueKind};

        if !crate::validation::simple::holds_lexical_qname(&value) {
            return value;
        }
        let XmlValue {
            type_code,
            schema_type,
            value: kind,
        } = value;
        let kind = match kind {
            XmlValueKind::Union(inner) => {
                XmlValueKind::Union(Box::new(self.resolve_qname_typed_value(*inner)))
            }
            XmlValueKind::List { item_type, items } => XmlValueKind::List {
                item_type,
                items: items
                    .into_iter()
                    .map(|member| self.resolve_qname_typed_value(member))
                    .collect(),
            },
            XmlValueKind::Atomic(crate::types::value::XmlAtomicValue::String(lexical)) => {
                match self.resolve_lexical_qname(&lexical, type_code) {
                    Some(atom) => XmlValueKind::Atomic(atom),
                    None => {
                        XmlValueKind::Atomic(crate::types::value::XmlAtomicValue::String(lexical))
                    }
                }
            }
            other => other,
        };
        XmlValue {
            type_code,
            schema_type,
            value: kind,
        }
    }

    /// The `xs:QName` (or, for `type_code` `xs:NOTATION`, `xs:NOTATION`)
    /// value `lexical` denotes in the navigator's in-scope namespaces; `None`
    /// when it is not a QName or its prefix is not in scope.
    fn resolve_lexical_qname(
        &self,
        lexical: &str,
        type_code: crate::types::XmlTypeCode,
    ) -> Option<crate::types::value::XmlAtomicValue> {
        use crate::namespace::qname::QualifiedName;
        use crate::types::value::{QNameValue, XmlAtomicValue};
        use crate::xpath::functions::qname::parse_lexical_qname;

        let (prefix, local_name) = parse_lexical_qname(lexical).ok()?;
        let ns_id = match prefix.as_deref() {
            Some(p) => Some(self.resolve_prefix_in_scope(Some(p))?),
            None => self.resolve_prefix_in_scope(None),
        };
        let names = self.doc.names;
        let local_id = names.add(&local_name);
        let prefix_id = prefix.as_deref().map(|p| names.add(p));
        let qn = QNameValue::new(QualifiedName::new(ns_id, local_id, prefix_id), names);
        Some(if type_code == crate::types::XmlTypeCode::Notation {
            XmlAtomicValue::Notation(qn)
        } else {
            XmlAtomicValue::QName(qn)
        })
    }

    /// The namespace an `xs:QName` / `xs:NOTATION` lexical form's prefix is
    /// bound to at this node, as simple-type validation asks it
    /// (`validation::simple::PrefixResolver`): for a prefix, `Some` of its
    /// namespace or `None` when it is bound nowhere; for no prefix, `Some` of
    /// the default namespace (`Some(None)` without one).
    pub(crate) fn prefix_binding(&self, prefix: Option<&str>) -> Option<Option<NameId>> {
        match prefix {
            Some(p) if !p.is_empty() => self.resolve_prefix_in_scope(Some(p)).map(Some),
            _ => Some(self.resolve_prefix_in_scope(None)),
        }
    }

    /// Resolve a prefix to its in-scope namespace URI's `NameId`.
    ///
    /// Reads the chain in scope at the owning element (or the current node if
    /// not on a virtual node): its first node of the prefix is the binding in
    /// force. Returns `None` if the prefix is
    /// undeclared or bound to an empty URI (XML 1.1 namespace undeclaration).
    /// `prefix.is_none()` requests the default namespace; `Some("")` is
    /// equivalent. The implicit `xml` prefix always resolves.
    fn resolve_prefix_in_scope(&self, prefix: Option<&str>) -> Option<NameId> {
        let target_prefix = prefix.unwrap_or("");
        let names = self.doc.names;

        if target_prefix == "xml" {
            return Some(names.add("http://www.w3.org/XML/1998/namespace"));
        }

        let start = if self.virtual_parent != NULL {
            self.virtual_parent
        } else {
            self.current
        };

        for (_, ns_node) in self.doc.scope_walk(self.doc.namespace_scope(start)) {
            if names.resolve_ref(ns_node.prefix) == target_prefix {
                if names.resolve_ref(ns_node.namespace_uri).is_empty() {
                    return None;
                }
                return Some(ns_node.namespace_uri);
            }
        }

        None
    }
}

/// The lexical value an element bound with `binding` has its typed value
/// computed from: its string value `value`, or — when that is empty and the
/// element has no element children — its declaration's `default` / `fixed`
/// value (clause 5.1 of Element Locally Valid (Element), Structures §3.3.4.3;
/// the default first, as validation takes it). A typed build that makes
/// defaults nodes has given such an element its default as text already; a
/// tree built as read has not, and gets the same typed value. The builder's id
/// and idrefs indexes compute an element's typed value from the same value.
pub(crate) fn element_value_or_default<'v>(
    value: Cow<'v, str>,
    has_element_children: bool,
    binding: &NodeSchemaBinding,
    schema_set: &'v SchemaSet,
) -> Cow<'v, str> {
    if !value.is_empty() || has_element_children {
        return value;
    }
    let Some(element) = binding
        .element_decl
        .and_then(|key| schema_set.arenas.elements.get(key))
    else {
        return value;
    };
    match element
        .default_value
        .as_deref()
        .or(element.fixed_value.as_deref())
    {
        Some(default) => Cow::Borrowed(default),
        None => value,
    }
}

// ── DomNavigator impl ─────────────────────────────────────────────────

impl<'a> DomNavigator for BufferDocNavigator<'a> {
    /// Node identity, as required by the XPath 2.0 `is` operator.
    ///
    /// XPath 2.0 §3.5.3 *Node Comparisons*: "A comparison with the `is`
    /// operator is true if the two operand nodes have the same identity, and
    /// are thus the same node; otherwise it is `false`."
    ///
    /// Every cursor field of this navigator (`current`, `virtual_parent`,
    /// `current_ns`, `attr_index`) is an index **into one
    /// [`BufferDocument`]** and carries no meaning outside it, so identity
    /// must include the document itself: two navigators sitting on the same
    /// node index of two *different* documents are two different nodes. The
    /// document is compared by address, which is exactly document identity —
    /// a `BufferDocument` cannot move while a navigator borrows it.
    fn is_same_position(&self, other: &Self) -> bool {
        std::ptr::eq(self.doc, other.doc)
            && self.current == other.current
            && self.virtual_parent == other.virtual_parent
            && self.current_ns == other.current_ns
            && self.attr_index == other.attr_index
    }

    /// Document order, including across trees.
    ///
    /// Within one document the flat node layout *is* document order, so the
    /// comparison is on the cursor's `order_key`.
    ///
    /// Across documents, XPath 2.0 §2.4.1 *Document Order* leaves the choice
    /// to the implementation but constrains it: "The relative order of nodes
    /// in distinct trees is stable but implementation-dependent, subject to
    /// the following constraint: If any node in a given tree T1 is before any
    /// node in a different tree T2, then all nodes in tree T1 are before all
    /// nodes in tree T2" — and "Document order is stable, which means that
    /// the relative order of two nodes will not change during the processing
    /// of a given expression".
    ///
    /// Trees are therefore ordered as whole blocks, by
    /// [`BufferDocument::serial`] — the document creation ordinal. A serial
    /// satisfies the constraint (it depends only on the document, not on the
    /// node) and, unlike the heap address used previously, it is
    /// *reproducible*: repeating the same sequence of document constructions
    /// yields the same order, which makes cross-document results
    /// diffable in tests and stable for `generate-id()`-style identifiers.
    /// Serials are unique per process, so `Same` can only be returned for
    /// two cursors on the same document.
    fn compare_position(&self, other: &Self) -> XmlNodeOrder {
        if !std::ptr::eq(self.doc, other.doc) {
            return if self.doc.serial() < other.doc.serial() {
                XmlNodeOrder::Before
            } else {
                XmlNodeOrder::After
            };
        }
        match self.order_key().cmp(&other.order_key()) {
            std::cmp::Ordering::Less => XmlNodeOrder::Before,
            std::cmp::Ordering::Equal => XmlNodeOrder::Same,
            std::cmp::Ordering::Greater => XmlNodeOrder::After,
        }
    }

    /// Moves this cursor onto `other`'s position — including `other`'s
    /// document, so that moving onto a node of another tree lands on that
    /// node rather than on this tree's node of the same index.
    fn move_to(&mut self, other: &Self) -> bool {
        self.doc = other.doc;
        self.current = other.current;
        self.assertion_absolute_root = other.assertion_absolute_root;
        self.assertion_fragment_root = other.assertion_fragment_root;
        // The parentless view travels with the cursor: landing on a node of a
        // tree whose root is an orphan must not restore its hidden ancestors.
        self.orphan_root = other.orphan_root;
        self.virtual_parent = other.virtual_parent;
        self.current_ns = other.current_ns;
        self.attr_index = other.attr_index;
        self.ns_list.clone_from(&other.ns_list);
        true
    }

    fn move_to_root(&mut self) {
        if self.orphan_root != NULL {
            self.current = self.orphan_root;
            self.clear_virtual();
            return;
        }
        self.current = self.doc.root;
        self.clear_virtual();
    }

    fn move_to_visible_root(&mut self) {
        if self.assertion_absolute_root && self.assertion_fragment_root != NULL {
            self.current = self.assertion_fragment_root;
        } else {
            self.current = self.doc.root;
        }
        self.clear_virtual();
    }

    fn move_to_parent(&mut self) -> bool {
        // The top node's own upward link is cut, but an attribute or namespace
        // node *of* it still has it as its parent, and such a cursor also sits
        // on the same `current` — which `is_tree_top` accounts for, so the
        // virtual parent is resolved below.
        if self.is_tree_top() {
            return false;
        }
        if self.virtual_parent != NULL {
            self.current = self.virtual_parent;
            self.clear_virtual();
            return true;
        }
        let parent = self.node().parent;
        if parent == NULL {
            return false;
        }
        self.current = parent;
        true
    }

    fn move_to_first_child(&mut self) -> bool {
        if self.virtual_parent != NULL {
            return false;
        }
        if self.assertion_absolute_root && self.current == self.doc.root {
            return false;
        }
        if let Some(child) = self.doc.first_content_child_of(self.current) {
            self.current = child;
            true
        } else {
            false
        }
    }

    fn move_to_next_sibling(&mut self) -> bool {
        if self.virtual_parent != NULL {
            return false;
        }
        if self.is_tree_top() {
            return false;
        }
        let sib = self.node().next_sibling;
        if sib == NULL {
            return false;
        }
        self.current = sib;
        true
    }

    fn move_to_prev_sibling(&mut self) -> bool {
        if self.virtual_parent != NULL {
            return false;
        }
        if self.is_tree_top() {
            return false;
        }
        let parent_ref = self.node().parent;
        if parent_ref == NULL {
            return false;
        }
        let first = match self.doc.first_content_child_of(parent_ref) {
            Some(f) => f,
            None => return false,
        };
        if first == self.current {
            return false;
        }
        let mut child = first;
        loop {
            let next = self.doc.nodes.get(child).next_sibling;
            if next == self.current {
                self.current = child;
                return true;
            }
            if next == NULL {
                return false;
            }
            child = next;
        }
    }

    fn move_to_first_attribute(&mut self) -> bool {
        if self.virtual_parent != NULL {
            return false;
        }
        let node = self.node();
        if node.node_type() != NodeType::Element || !node.has_flag(Node::HAS_ATTRIBUTE) {
            return false;
        }
        let first_attr = self.current + 1;
        debug_assert_eq!(
            self.doc.nodes.get(first_attr).node_type(),
            NodeType::Attribute,
        );
        self.virtual_parent = self.current;
        self.current = first_attr;
        self.current_ns = NsRef::NULL;
        self.attr_index = 0;
        true
    }

    fn move_to_next_attribute(&mut self) -> bool {
        if !self.is_on_attribute() {
            return false;
        }
        let next = self.node().next_sibling;
        if next == NULL {
            return false;
        }
        if self.doc.nodes.get(next).node_type() != NodeType::Attribute {
            return false;
        }
        self.current = next;
        self.attr_index += 1;
        true
    }

    fn move_to_first_namespace(&mut self, scope: NamespaceAxisScope) -> bool {
        if self.virtual_parent != NULL {
            return false;
        }
        let elem = self.current;
        if self.doc.nodes.get(elem).node_type() != NodeType::Element {
            return false;
        }
        let collected = self.collect_namespaces(elem, scope);
        if collected.is_empty() {
            return false;
        }
        self.virtual_parent = elem;
        self.current = elem;
        self.current_ns = collected[0];
        self.attr_index = 0;
        self.ns_list = collected;
        true
    }

    fn move_to_next_namespace(&mut self, _scope: NamespaceAxisScope) -> bool {
        if !self.is_on_namespace() {
            return false;
        }
        let next_idx = self.attr_index as usize + 1;
        if next_idx >= self.ns_list.len() {
            return false;
        }
        self.attr_index = next_idx as u16;
        self.current_ns = self.ns_list[next_idx];
        true
    }

    fn move_to_following(&mut self, kind: DomNodeType, end: Option<&Self>) -> bool {
        // Save cursor so we can restore on failure (position unchanged on false).
        let saved_current = self.current;
        let saved_virtual_parent = self.virtual_parent;
        let saved_current_ns = self.current_ns;
        let saved_attr_index = self.attr_index;

        // Following axis: skip descendants of the current node.
        // First escape to the next sibling, or walk up to an ancestor's
        // next sibling, then do a depth-first scan from there.
        if self.virtual_parent != NULL {
            // On attribute/namespace — move back to the owning element first
            self.current = self.virtual_parent;
            self.clear_virtual();
        }

        // Escape the current subtree: find next sibling or ancestor's next sibling
        loop {
            if self.move_to_next_sibling() {
                break;
            }
            if !self.move_to_parent() {
                self.restore_cursor(
                    saved_current,
                    saved_virtual_parent,
                    saved_current_ns,
                    saved_attr_index,
                );
                return false;
            }
        }

        // Now do a depth-first walk from here (this node and its descendants
        // are all in the following axis).
        loop {
            if self.matches_kind(kind) {
                if self.past_end(end) {
                    self.restore_cursor(
                        saved_current,
                        saved_virtual_parent,
                        saved_current_ns,
                        saved_attr_index,
                    );
                    return false;
                }
                return true;
            }
            // Depth-first: child, then sibling, then ancestor's sibling
            if self.move_to_first_child() {
                continue;
            }
            if self.move_to_next_sibling() {
                continue;
            }
            loop {
                if !self.move_to_parent() {
                    self.restore_cursor(
                        saved_current,
                        saved_virtual_parent,
                        saved_current_ns,
                        saved_attr_index,
                    );
                    return false;
                }
                if self.move_to_next_sibling() {
                    break;
                }
            }
        }
    }

    // ── Node information ──────────────────────────────────────────────

    fn node_type(&self) -> DomNodeType {
        if self.is_on_namespace() {
            return DomNodeType::Namespace;
        }
        DomNodeType::from(self.node().node_type())
    }

    fn local_name(&self) -> &str {
        if self.is_on_namespace() {
            let ns = self.ns_node();
            return self.doc.names.resolve_ref(ns.prefix);
        }
        let node = self.node();
        match node.node_type() {
            NodeType::Element | NodeType::Attribute => {
                let qname = self.doc.qname_table.get(node.value);
                self.doc.names.resolve_ref(qname.local_name)
            }
            NodeType::ProcessingInstruction => self.doc.strings.get(node.value),
            _ => "",
        }
    }

    fn name(&self) -> &str {
        if self.is_on_namespace() {
            let ns = self.ns_node();
            return self.doc.names.resolve_ref(ns.prefix);
        }
        let node = self.node();
        match node.node_type() {
            NodeType::Element | NodeType::Attribute => {
                let qname = self.doc.qname_table.get(node.value);
                self.doc.strings.get(qname.qualified_name_idx)
            }
            NodeType::ProcessingInstruction => self.doc.strings.get(node.value),
            _ => "",
        }
    }

    fn namespace_uri(&self) -> &str {
        if self.is_on_namespace() {
            // Namespace nodes themselves don't have a namespace URI
            return "";
        }
        let node = self.node();
        match node.node_type() {
            NodeType::Element | NodeType::Attribute => {
                let qname = self.doc.qname_table.get(node.value);
                self.doc.names.resolve_ref(qname.namespace_uri)
            }
            _ => "",
        }
    }

    fn prefix(&self) -> &str {
        if self.is_on_namespace() {
            return "";
        }
        let node = self.node();
        match node.node_type() {
            NodeType::Element | NodeType::Attribute => {
                let qname = self.doc.qname_table.get(node.value);
                self.doc.names.resolve_ref(qname.prefix)
            }
            _ => "",
        }
    }

    fn value(&self) -> String {
        if self.is_on_namespace() {
            // Namespace node value = the bound namespace URI
            let ns = self.ns_node();
            return self.doc.names.resolve_ref(ns.namespace_uri).to_string();
        }
        let node = self.node();
        match node.node_type() {
            NodeType::Element | NodeType::Root => self.compute_element_value(),
            NodeType::Attribute | NodeType::ProcessingInstruction => {
                let val_node = self.doc.nodes.get(self.current + 1);
                self.doc.strings.get(val_node.value).to_string()
            }
            NodeType::Text
            | NodeType::Whitespace
            | NodeType::SignificantWhitespace
            | NodeType::Comment => self.doc.strings.get(node.value).to_string(),
            _ => String::new(),
        }
    }

    fn value_ref(&self) -> Cow<'_, str> {
        if self.is_on_namespace() {
            // Namespace node value = the bound namespace URI (interned name).
            return Cow::Borrowed(self.doc.names.resolve_ref(self.ns_node().namespace_uri));
        }
        let node = self.node();
        match node.node_type() {
            // Attribute/PI values live in the following value node's StringStore
            // slot — borrow the contiguous interned bytes, no copy.
            NodeType::Attribute | NodeType::ProcessingInstruction => {
                let val_node = self.doc.nodes.get(self.current + 1);
                Cow::Borrowed(self.doc.strings.get(val_node.value))
            }
            NodeType::Text
            | NodeType::Whitespace
            | NodeType::SignificantWhitespace
            | NodeType::Comment => Cow::Borrowed(self.doc.strings.get(node.value)),
            // Element/Root: borrow the single interned text run when there is
            // one (the simple-content leaf case); only true mixed/multi-text
            // content falls back to allocating a concatenation.
            NodeType::Element | NodeType::Root => match self.single_text_value() {
                Some(s) => Cow::Borrowed(s),
                None => Cow::Owned(self.compute_element_value()),
            },
            _ => Cow::Borrowed(""),
        }
    }

    fn base_uri(&self) -> &str {
        self.resolve_base_uri()
    }

    // ── Schema type hooks ─────────────────────────────────────────────

    fn schema_type(&self) -> Option<SimpleTypeKey> {
        if self.is_on_namespace() {
            return None;
        }
        let idx = self.node().binding_index();
        let binding = self.doc.binding_remap.get(idx)?;
        match binding.type_key {
            TypeKey::Simple(k) => Some(k),
            TypeKey::Complex(_) => None,
        }
    }

    /// The node's type annotation, complex types included.
    ///
    /// Reads the [`NodeSchemaBinding`] the typed builder attached to the
    /// node. Bindings exist only on element and attribute nodes — an
    /// attribute's binding lives on the `Attribute` node of its
    /// name+value pair, which is exactly where the attribute cursor sits, so
    /// no extra indexing is needed — and the unbound sentinel of the remap
    /// table makes every other node kind (and any node of an unvalidated
    /// document) report `None`. The namespace cursor keeps `current` on the
    /// owning element, so it is rejected explicitly.
    fn type_annotation(&self) -> Option<TypeKey> {
        if self.is_on_namespace() {
            return None;
        }
        if !matches!(
            self.node().node_type(),
            NodeType::Element | NodeType::Attribute
        ) {
            return None;
        }
        let idx = self.node().binding_index();
        Some(self.doc.binding_remap.get(idx)?.type_key)
    }

    fn typed_value(&self) -> TypedValue {
        if self.is_on_namespace() {
            return TypedValue::Untyped;
        }
        let node = self.node();
        if node.has_flag(Node::IS_NIL) {
            return TypedValue::Nilled;
        }
        let binding = match self.schema_binding() {
            Some(b) => b,
            None => return TypedValue::Untyped,
        };
        let schema_set = match self.doc.schema_set {
            Some(s) => s,
            None => return TypedValue::Untyped,
        };

        // Complex types (XDM 1.0 §6.2.4): simple content is validated below.
        // "If the element has a complex type with mixed content (including
        // xs:anyType), its typed-value is its dm:string-value as an
        // xs:untypedAtomic"; "If the element has a complex type with empty
        // content, its typed-value is the empty sequence" — an empty list
        // value, which atomizes to no item; element-only content has no typed
        // value at all (FOTY0012 on atomization).
        if let TypeKey::Complex(_) = binding.type_key {
            match binding.content_type {
                Some(ContentType::TextOnly) => {}
                Some(ContentType::Mixed) => return TypedValue::Untyped,
                Some(ContentType::Empty) => {
                    use crate::types::value::{XmlValue, XmlValueKind};
                    use crate::types::XmlTypeCode;
                    return TypedValue::Value(XmlValue::new(
                        XmlTypeCode::UntypedAtomic,
                        XmlValueKind::List {
                            item_type: XmlTypeCode::UntypedAtomic,
                            items: Vec::new(),
                        },
                    ));
                }
                Some(ContentType::ElementOnly) | None => return TypedValue::Absent,
            }
        }

        // Borrow the lexical value (single-text leaves borrow; only true mixed
        // content allocates) so simple-content elements cost no per-element copy.
        let value_str = self.value_ref();
        let has_element_children = value_str.is_empty() && self.has_element_children();
        let effective_value =
            element_value_or_default(value_str, has_element_children, binding, schema_set);

        // QName / NOTATION values resolve their prefixes with the node's
        // in-scope namespaces (Datatypes §3.3.18), as the validation runtime
        // does with the element's, so that the typed value and the verdict
        // agree (enumeration by expanded name, `$value` of an assertion).
        let resolver = |prefix: Option<&str>| self.prefix_binding(prefix);
        match validate_simple_type_in(
            &effective_value,
            binding.type_key,
            schema_set,
            Some(&resolver),
        ) {
            Ok(r) => TypedValue::Value(self.resolve_qname_typed_value(r.typed_value)),
            Err(_) => TypedValue::Untyped,
        }
    }

    /// The is-idrefs property (XDM 1.0 §5.6): set on an element or attribute
    /// whose typed value holds an `xs:IDREF` (or derived) atomic value, or that
    /// a host marked (`BufferDocumentBuilder::register_idrefs`); `false` for
    /// every other node, and for every node of a tree without annotations.
    fn is_idrefs(&self) -> bool {
        if self.is_on_namespace() {
            return false;
        }
        matches!(
            self.node().node_type(),
            NodeType::Element | NodeType::Attribute
        ) && self.node().has_flag(Node::IS_IDREFS)
    }

    /// The nodes with the is-idrefs property whose string value has `id` as a
    /// token, in document order — scoped as
    /// [`find_element_by_id`](DomNavigator::find_element_by_id) is: this
    /// cursor's tree, or the subtree a parentless or asserted node roots.
    fn find_nodes_by_idref(&self, id: &str) -> Result<Vec<Self>, NavigatorError> {
        let scope_root = if self.orphan_root != NULL {
            self.orphan_root
        } else {
            self.assertion_fragment_root
        };
        let range = if scope_root != NULL {
            Some((scope_root, self.doc.subtree_end(scope_root)))
        } else {
            let tree = self.doc.tree_root_of(self.current);
            (tree != super::buffer::DOCUMENT_NODE).then(|| (tree, self.doc.subtree_end(tree)))
        };
        Ok(self
            .doc
            .nodes_by_idref(id, range)
            .into_iter()
            .map(|r| {
                let node = self.doc.nodes.get(r);
                let mut nav = if node.node_type() == NodeType::Attribute {
                    BufferDocNavigator::at_attribute(self.doc, r)
                } else {
                    BufferDocNavigator::new(self.doc, r)
                };
                nav.assertion_absolute_root = self.assertion_absolute_root;
                nav.assertion_fragment_root = self.assertion_fragment_root;
                nav.orphan_root = self.orphan_root;
                nav
            })
            .collect())
    }

    /// The scope of an id is one tree: the search is anchored on this cursor's
    /// own tree (see [`BufferDocument::get_element_by_id_in_tree`] for which
    /// nodes make up one), so a buffer holding several top-level trees never
    /// lets one reach into another.
    ///
    /// Under [`new_orphan`](Self::new_orphan) and
    /// [`new_assertion`](Self::new_assertion) the visible tree is the subtree
    /// of the parentless node or of the asserted element `E` — XSD 1.1
    /// §3.13.4.1 clause 1.3: the assertion's data model instance "contains
    /// only that node and nodes constructed from the \[attributes\],
    /// \[children\], and descendants of E". The lookup is then restricted to
    /// that subtree *before* the first claimant is chosen, so an element
    /// outside it is never returned and never hides one inside it.
    fn find_element_by_id(&self, id: &str) -> Result<Option<Self>, NavigatorError> {
        let scope_root = if self.orphan_root != NULL {
            self.orphan_root
        } else {
            self.assertion_fragment_root
        };
        let found = if scope_root != NULL {
            self.doc.element_by_id_in_subtree(scope_root, id)
        } else {
            self.doc.get_element_by_id_in_tree(self.current, id)
        };
        Ok(found.map(|r| {
            let mut nav = BufferDocNavigator::new(self.doc, r);
            nav.assertion_absolute_root = self.assertion_absolute_root;
            nav.assertion_fragment_root = self.assertion_fragment_root;
            nav.orphan_root = self.orphan_root;
            nav
        }))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespace::NameTable;
    use crate::xpath::context::XPathContext;
    use crate::xpath::XPathExpr;
    use bumpalo::Bump;

    fn build_doc<'a>(xml: &str, arena: &'a Bump, names: &'a NameTable) -> BufferDocument<'a> {
        BufferDocument::from_reader_default(xml.as_bytes(), arena, names).unwrap()
    }

    fn first_ele2<'a>(doc: &'a BufferDocument<'a>) -> u32 {
        let mut nav = BufferDocNavigator::new(doc, doc.root());
        assert!(nav.move_to_first_child()); // root
        assert!(nav.move_to_first_child()); // ele1
        assert!(nav.move_to_first_child()); // subElement1
        assert!(nav.move_to_first_child()); // ele2
        nav.current_ref()
    }

    /// XSD 1.1 §3.13.4.1 clause 1.3: "The root node of the [XDM] instance is
    /// constructed from E; the data model instance contains only that node and
    /// nodes constructed from the [attributes], [children], and descendants of
    /// E." Note: "attempts to refer, in an assertion, to the siblings or
    /// ancestors of E, or to any part of the input document outside of E
    /// itself, will be unsuccessful."
    ///
    /// Every axis that leaves the asserted element therefore yields nothing,
    /// and every axis inside it is unaffected.
    #[test]
    fn assertion_navigator_cuts_every_axis_that_leaves_the_asserted_element() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc(
            "<root><before1/><before2/><E a=\"1\"><in1/><deep><in2/></deep></E><after1/><after2/></root>",
            &arena,
            &names,
        );
        // The `E` element: third child of the document element.
        let e = {
            let mut nav = BufferDocNavigator::new(&doc, doc.root());
            assert!(nav.move_to_first_child()); // root
            assert!(nav.move_to_first_child()); // before1
            assert!(nav.move_to_next_sibling()); // before2
            assert!(nav.move_to_next_sibling()); // E
            assert_eq!(nav.local_name(), "E");
            nav.current_ref()
        };
        let ctx = XPathContext::new(&names);
        let count = |expr: &str| {
            let nav = BufferDocNavigator::new_assertion(&doc, e);
            XPathExpr::compile(expr, &ctx)
                .expect("compile")
                .evaluator(&ctx)
                .run_with_node(nav)
                .expect("evaluate")
                .first()
                .and_then(|item| item.as_atomic().map(|v| v.to_string_value()))
                .unwrap_or_default()
        };

        // Outward: nothing.
        for expr in [
            "count(following::node())",
            "count(preceding::node())",
            "count(following-sibling::node())",
            "count(preceding-sibling::node())",
            "count(parent::node())",
            "count(..)",
            "count(../..)",
            "count(ancestor::node())",
        ] {
            assert_eq!(count(expr), "0", "{expr} must not leave the subtree");
        }
        // Inward: unchanged. `ancestor-or-self::` still has the self step.
        assert_eq!(count("count(ancestor-or-self::node())"), "1");
        assert_eq!(count("count(.//*)"), "3");
        assert_eq!(count("count(descendant-or-self::node())"), "4");
        assert_eq!(count("count(@a)"), "1");
        assert_eq!(count("count(child::*)"), "2");
        // An attribute of E still reaches E through its parent axis: the cut
        // is on E's *own* upward link, not on its attributes'.
        assert_eq!(count("count(@a/parent::E)"), "1");
        assert_eq!(count("count(@a/../..)"), "0");
        // A descendant reaches E, and stops there.
        assert_eq!(count("count(deep/ancestor::*)"), "1");
        assert_eq!(count("local-name(deep/ancestor::*)"), "E");
        assert_eq!(count("count(in1/following::*)"), "2");
        assert_eq!(count("count(deep/preceding::*)"), "1");
    }

    #[test]
    fn assertion_navigator_keeps_absolute_double_slash_empty() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc(
            "<root><ele1 attr1=\"1\"><subElement1><ele2><subElement2><ele1 attr1=\"2\"/></subElement2></ele2></subElement1></ele1></root>",
            &arena,
            &names,
        );
        let ele2 = first_ele2(&doc);
        let ctx = XPathContext::new(&names);
        let nav = BufferDocNavigator::new_assertion(&doc, ele2);
        let expr = XPathExpr::compile(
            "empty(//ele1) and count(.//ele1) eq 1 and empty(//@attr1) and count(.//@attr1) eq 1",
            &ctx,
        )
        .unwrap();

        let result = expr.evaluator(&ctx).run_with_node(nav).unwrap();
        assert_eq!(result.as_bool(), Some(true));
    }

    // ── 1. Basic navigation ──────────────────────────────────────────

    #[test]
    fn root_to_first_child_and_back() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc("<root><child/></root>", &arena, &names);
        let mut nav = doc.create_navigator();

        assert_eq!(nav.node_type(), DomNodeType::Root);
        assert!(nav.move_to_first_child());
        assert_eq!(nav.node_type(), DomNodeType::Element);
        assert_eq!(nav.local_name(), "root");

        assert!(nav.move_to_first_child());
        assert_eq!(nav.local_name(), "child");

        assert!(nav.move_to_parent());
        assert_eq!(nav.local_name(), "root");

        assert!(nav.move_to_parent());
        assert_eq!(nav.node_type(), DomNodeType::Root);

        assert!(!nav.move_to_parent());
    }

    #[test]
    fn sibling_navigation() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc("<root><a/><b/><c/></root>", &arena, &names);
        let mut nav = doc.create_navigator();

        nav.move_to_first_child(); // root
        nav.move_to_first_child(); // a

        assert_eq!(nav.local_name(), "a");
        assert!(nav.move_to_next_sibling());
        assert_eq!(nav.local_name(), "b");
        assert!(nav.move_to_next_sibling());
        assert_eq!(nav.local_name(), "c");
        assert!(!nav.move_to_next_sibling());
    }

    // ── 2. Attribute axis ────────────────────────────────────────────

    #[test]
    fn attribute_navigation() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc(r#"<root attr1="v1" attr2="v2"/>"#, &arena, &names);
        let mut nav = doc.create_navigator();
        nav.move_to_first_child(); // root

        assert!(nav.move_to_first_attribute());
        assert_eq!(nav.node_type(), DomNodeType::Attribute);
        assert_eq!(nav.local_name(), "attr1");
        assert_eq!(nav.value(), "v1");

        assert!(nav.move_to_next_attribute());
        assert_eq!(nav.local_name(), "attr2");
        assert_eq!(nav.value(), "v2");

        assert!(!nav.move_to_next_attribute());

        assert!(nav.move_to_parent());
        assert_eq!(nav.node_type(), DomNodeType::Element);
        assert_eq!(nav.local_name(), "root");
    }

    // ── 3. Namespace axis ────────────────────────────────────────────

    #[test]
    fn namespace_local() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc(
            r#"<root xmlns="http://default" xmlns:p="http://prefixed"/>"#,
            &arena,
            &names,
        );
        let mut nav = doc.create_navigator();
        nav.move_to_first_child(); // root

        assert!(nav.move_to_first_namespace(NamespaceAxisScope::Local));
        assert_eq!(nav.node_type(), DomNodeType::Namespace);

        let mut uris = std::collections::HashSet::new();
        uris.insert(nav.value());
        while nav.move_to_next_namespace(NamespaceAxisScope::Local) {
            uris.insert(nav.value());
        }

        assert!(uris.contains("http://default"));
        assert!(uris.contains("http://prefixed"));
    }

    #[test]
    fn namespace_all_includes_xml() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc(r#"<root xmlns:p="http://prefixed"/>"#, &arena, &names);
        let mut nav = doc.create_navigator();
        nav.move_to_first_child(); // root

        assert!(nav.move_to_first_namespace(NamespaceAxisScope::All));

        let mut uris = std::collections::HashSet::new();
        uris.insert(nav.value());
        while nav.move_to_next_namespace(NamespaceAxisScope::All) {
            uris.insert(nav.value());
        }

        assert!(
            uris.contains("http://prefixed"),
            "Should see declared namespace"
        );
        assert!(
            uris.contains("http://www.w3.org/XML/1998/namespace"),
            "Should see implicit xml namespace"
        );
    }

    #[test]
    fn namespace_all_bare_element_includes_xml() {
        // XDM contract (A): `All` exposes the implicit `xml:` binding even on a
        // namespace-free element. (`ExcludeXml` — used by validation — is empty
        // here, which is what keeps the validation hot path allocation-free.)
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc(r#"<root/>"#, &arena, &names);
        let mut nav = doc.create_navigator();
        nav.move_to_first_child(); // root

        assert!(
            nav.move_to_first_namespace(NamespaceAxisScope::All),
            "All scope must expose the implicit xml namespace on a bare element"
        );
        assert_eq!(
            nav.value(),
            "http://www.w3.org/XML/1998/namespace",
            "the only in-scope namespace on <root/> is the implicit xml binding"
        );
        assert!(!nav.move_to_next_namespace(NamespaceAxisScope::All));

        // ExcludeXml on the same bare element is empty (no per-element alloc).
        assert!(!nav.move_to_first_namespace(NamespaceAxisScope::ExcludeXml));
    }

    #[test]
    fn namespace_exclude_xml() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc(r#"<root xmlns:p="http://prefixed"/>"#, &arena, &names);
        let mut nav = doc.create_navigator();
        nav.move_to_first_child(); // root

        assert!(nav.move_to_first_namespace(NamespaceAxisScope::ExcludeXml));

        let mut uris = std::collections::HashSet::new();
        uris.insert(nav.value());
        while nav.move_to_next_namespace(NamespaceAxisScope::ExcludeXml) {
            uris.insert(nav.value());
        }

        assert!(uris.contains("http://prefixed"));
        assert!(
            !uris.contains("http://www.w3.org/XML/1998/namespace"),
            "Should NOT see xml namespace with ExcludeXml"
        );
    }

    #[test]
    fn namespace_inherited() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc(
            r#"<root xmlns:ns="http://example.com"><child xmlns:local="http://local.com"/></root>"#,
            &arena,
            &names,
        );
        let mut nav = doc.create_navigator();
        nav.move_to_first_child(); // root
        nav.move_to_first_child(); // child

        // All scope: both inherited and local
        assert!(nav.move_to_first_namespace(NamespaceAxisScope::All));
        let mut all_uris = std::collections::HashSet::new();
        all_uris.insert(nav.value());
        while nav.move_to_next_namespace(NamespaceAxisScope::All) {
            all_uris.insert(nav.value());
        }

        assert!(all_uris.contains("http://example.com"), "inherited");
        assert!(all_uris.contains("http://local.com"), "local");

        // Local scope: only local
        nav.move_to_parent(); // back to child
        assert!(nav.move_to_first_namespace(NamespaceAxisScope::Local));
        let mut local_uris = std::collections::HashSet::new();
        local_uris.insert(nav.value());
        while nav.move_to_next_namespace(NamespaceAxisScope::Local) {
            local_uris.insert(nav.value());
        }

        assert!(local_uris.contains("http://local.com"), "local");
        assert!(
            !local_uris.contains("http://example.com"),
            "inherited should not be in Local scope"
        );
    }

    #[test]
    fn namespace_undeclaration_is_not_a_namespace_node() {
        // `xmlns=""` is the absence of a binding for the default prefix, not a
        // binding to the zero-length URI, so the XDM `dm:namespace-nodes`
        // accessor (scope `All`) reports no namespace node for it — while it
        // still shadows the outer default namespace.
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc(
            r#"<chap xmlns="http://c/"><para xmlns=""><deep/></para></chap>"#,
            &arena,
            &names,
        );

        let mut nav = doc.create_navigator();
        nav.move_to_first_child(); // chap
        assert!(nav.move_to_first_namespace(NamespaceAxisScope::All));
        let mut chap = vec![(nav.local_name().to_string(), nav.value())];
        while nav.move_to_next_namespace(NamespaceAxisScope::All) {
            chap.push((nav.local_name().to_string(), nav.value()));
        }
        chap.sort();
        assert_eq!(
            chap,
            vec![
                (String::new(), "http://c/".to_string()),
                (
                    "xml".to_string(),
                    "http://www.w3.org/XML/1998/namespace".to_string()
                ),
            ]
        );

        for depth in 2..=3 {
            let mut nav = doc.create_navigator();
            for _ in 0..depth {
                assert!(nav.move_to_first_child());
            }
            assert!(nav.move_to_first_namespace(NamespaceAxisScope::All));
            let mut seen = vec![(nav.local_name().to_string(), nav.value())];
            while nav.move_to_next_namespace(NamespaceAxisScope::All) {
                seen.push((nav.local_name().to_string(), nav.value()));
            }
            assert_eq!(
                seen,
                vec![(
                    "xml".to_string(),
                    "http://www.w3.org/XML/1998/namespace".to_string()
                )],
                "the undeclaration must shadow http://c/ without becoming a node (depth {depth})"
            );
        }

        // The declaration itself is still visible to the views that model
        // declarations rather than namespace nodes: the serializer and the
        // namespace context that resolves QNames while validating both need
        // `xmlns=""` to stay.
        let mut nav = doc.create_navigator();
        nav.move_to_first_child(); // chap
        nav.move_to_first_child(); // para
        assert!(nav.move_to_first_namespace(NamespaceAxisScope::Local));
        assert_eq!((nav.local_name(), nav.value()), ("", String::new()));
        assert!(!nav.move_to_next_namespace(NamespaceAxisScope::Local));

        let mut nav = doc.create_navigator();
        nav.move_to_first_child(); // chap
        nav.move_to_first_child(); // para
        assert!(nav.move_to_first_namespace(NamespaceAxisScope::ExcludeXml));
        assert_eq!((nav.local_name(), nav.value()), ("", String::new()));
    }

    // ── 4. Element value ─────────────────────────────────────────────

    #[test]
    fn element_value_concatenated_text() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc("<root>Hello <b>World</b>!</root>", &arena, &names);
        let mut nav = doc.create_navigator();
        nav.move_to_first_child();

        assert_eq!(nav.value(), "Hello World!");
    }

    // ── 5. Attribute / PI value ──────────────────────────────────────

    #[test]
    fn attribute_value() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc(r#"<root key="val"/>"#, &arena, &names);
        let mut nav = doc.create_navigator();
        nav.move_to_first_child();
        nav.move_to_first_attribute();

        assert_eq!(nav.value(), "val");
    }

    #[test]
    fn pi_value() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc("<root><?target data?></root>", &arena, &names);
        let mut nav = doc.create_navigator();
        nav.move_to_first_child(); // root
        nav.move_to_first_child(); // PI

        assert_eq!(nav.node_type(), DomNodeType::ProcessingInstruction);
        assert_eq!(nav.local_name(), "target");
        assert_eq!(nav.value(), "data");
    }

    // ── 6. move_to_prev_sibling ──────────────────────────────────────

    #[test]
    fn prev_sibling() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc("<root><a/><b/><c/></root>", &arena, &names);
        let mut nav = doc.create_navigator();
        nav.move_to_first_child(); // root
        nav.move_to_first_child(); // a
        nav.move_to_next_sibling(); // b
        nav.move_to_next_sibling(); // c

        assert_eq!(nav.local_name(), "c");
        assert!(nav.move_to_prev_sibling());
        assert_eq!(nav.local_name(), "b");
        assert!(nav.move_to_prev_sibling());
        assert_eq!(nav.local_name(), "a");
        assert!(!nav.move_to_prev_sibling());
    }

    // ── 7. move_to_following ─────────────────────────────────────────

    #[test]
    fn move_to_following_elements() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc("<root><a><b/></a><c/></root>", &arena, &names);
        let mut nav = doc.create_navigator();
        nav.move_to_first_child(); // root
        nav.move_to_first_child(); // a

        // Following axis from <a> skips descendants of <a> (so skips <b>)
        // and moves to next sibling <c>.
        assert!(nav.move_to_following(DomNodeType::Element, None));
        assert_eq!(nav.local_name(), "c");

        // No more following elements after <c>
        assert!(!nav.move_to_following(DomNodeType::Element, None));
    }

    #[test]
    fn move_to_following_skips_descendants() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc("<root><a><d/><e/></a><b/><c/></root>", &arena, &names);
        let mut nav = doc.create_navigator();
        nav.move_to_first_child(); // root
        nav.move_to_first_child(); // a

        // Following from <a> should skip <d> and <e> (descendants)
        assert!(nav.move_to_following(DomNodeType::Element, None));
        assert_eq!(nav.local_name(), "b");

        assert!(nav.move_to_following(DomNodeType::Element, None));
        assert_eq!(nav.local_name(), "c");

        assert!(!nav.move_to_following(DomNodeType::Element, None));
    }

    #[test]
    fn move_to_following_from_deep_node() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc("<root><a><b><d/></b></a><c/></root>", &arena, &names);
        let mut nav = doc.create_navigator();
        nav.move_to_first_child(); // root
        nav.move_to_first_child(); // a
        nav.move_to_first_child(); // b

        // Following from <b> skips <d> (descendant), walks up to <a>'s sibling <c>
        assert!(nav.move_to_following(DomNodeType::Element, None));
        assert_eq!(nav.local_name(), "c");

        assert!(!nav.move_to_following(DomNodeType::Element, None));
    }

    #[test]
    fn move_to_following_preserves_position_on_false() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc("<root><a/></root>", &arena, &names);
        let mut nav = doc.create_navigator();
        nav.move_to_first_child(); // root
        nav.move_to_first_child(); // a

        let before = nav.clone();
        assert!(!nav.move_to_following(DomNodeType::Element, None));
        // Position must be unchanged after returning false
        assert!(nav.is_same_position(&before));
        assert_eq!(nav.local_name(), "a");
    }

    // ── 8. Document order ────────────────────────────────────────────

    #[test]
    fn compare_position_real_nodes() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc("<root><a/><b/></root>", &arena, &names);
        let mut nav1 = doc.create_navigator();
        let mut nav2 = doc.create_navigator();

        nav1.move_to_first_child(); // root
        nav1.move_to_first_child(); // a

        nav2.move_to_first_child(); // root
        nav2.move_to_first_child(); // a
        nav2.move_to_next_sibling(); // b

        assert_eq!(nav1.compare_position(&nav2), XmlNodeOrder::Before);
        assert_eq!(nav2.compare_position(&nav1), XmlNodeOrder::After);
        assert_eq!(nav1.compare_position(&nav1), XmlNodeOrder::Same);
    }

    #[test]
    fn compare_position_attr_vs_element() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc(r#"<root x="1"><child/></root>"#, &arena, &names);
        let mut nav_attr = doc.create_navigator();
        let mut nav_child = doc.create_navigator();

        nav_attr.move_to_first_child(); // root
        nav_attr.move_to_first_attribute(); // x

        nav_child.move_to_first_child(); // root
        nav_child.move_to_first_child(); // child

        // Attribute of root should come before child of root
        assert_eq!(nav_attr.compare_position(&nav_child), XmlNodeOrder::Before);
    }

    // ── 9. is_same_position / move_to ────────────────────────────────

    #[test]
    fn is_same_position_and_move_to() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc("<root><a/><b/></root>", &arena, &names);
        let mut nav1 = doc.create_navigator();
        let mut nav2 = doc.create_navigator();

        assert!(nav1.is_same_position(&nav2));

        nav2.move_to_first_child();
        nav2.move_to_first_child();
        nav2.move_to_next_sibling(); // b

        assert!(!nav1.is_same_position(&nav2));
        assert!(nav1.move_to(&nav2));
        assert!(nav1.is_same_position(&nav2));
        assert_eq!(nav1.local_name(), "b");
    }

    // ── 10. base_uri ─────────────────────────────────────────────────

    #[test]
    fn base_uri_from_xml_base() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc(
            r#"<root xml:base="http://example.com/"><child/></root>"#,
            &arena,
            &names,
        );
        let mut nav = doc.create_navigator();
        nav.move_to_first_child(); // root
        nav.move_to_first_child(); // child

        assert_eq!(nav.base_uri(), "http://example.com/");
    }

    #[test]
    fn base_uri_empty_when_absent() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc("<root><child/></root>", &arena, &names);
        let mut nav = doc.create_navigator();
        nav.move_to_first_child();

        assert_eq!(nav.base_uri(), "");
    }

    // ── 11. schema_type ──────────────────────────────────────────────

    #[test]
    fn schema_type_untyped() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc("<root/>", &arena, &names);
        let mut nav = doc.create_navigator();
        nav.move_to_first_child();

        assert!(nav.schema_type().is_none());
    }

    #[test]
    fn element_type_key_untyped() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc("<root/>", &arena, &names);
        let mut nav = doc.create_navigator();
        nav.move_to_first_child();

        assert!(nav.element_type_key().is_none());
    }

    #[test]
    fn schema_binding_untyped() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc("<root/>", &arena, &names);
        let mut nav = doc.create_navigator();
        nav.move_to_first_child();

        assert!(nav.schema_binding().is_none());
    }

    #[test]
    fn typed_value_nil_returns_nilled() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = super::super::builder::BufferDocumentBuilder::new(
            &arena,
            &names,
            None,
            super::super::BufferDocumentOptions::default(),
        )
        .unwrap();

        let elem = builder.start_element("root", "", "", &[]).unwrap();
        builder.end_of_attributes();
        builder.set_nil(elem);
        builder.end_element().unwrap();
        let doc = builder.finalize().unwrap();

        let mut nav = doc.create_navigator();
        nav.move_to_first_child();

        assert_eq!(nav.typed_value(), TypedValue::Nilled);
    }

    // ── 12. find_element_by_id ───────────────────────────────────────

    #[test]
    fn find_element_by_id_not_found() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc("<root/>", &arena, &names);
        let nav = doc.create_navigator();

        assert!(nav.find_element_by_id("missing").unwrap().is_none());
    }

    /// Evaluates `expr` with `nav` as the context node.
    fn eval<'d>(
        names: &NameTable,
        nav: BufferDocNavigator<'d>,
        expr: &str,
    ) -> Result<crate::xpath::XPathValue<BufferDocNavigator<'d>>, crate::xpath::XPathError> {
        let ctx = XPathContext::new(names);
        XPathExpr::compile(expr, &ctx)
            .expect("compile")
            .evaluator(&ctx)
            .run_with_node(nav)
    }

    /// The scope of an id is one *tree*. Two top-level trees in one buffer
    /// must not see each other's ids.
    #[test]
    fn find_element_by_id_stays_inside_one_tree() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = crate::document::BufferDocumentBuilder::new(
            &arena,
            &names,
            None,
            crate::document::BufferDocumentOptions::fragment(),
        )
        .unwrap();
        let a = builder.start_element("a", "", "", &[]).unwrap();
        builder
            .attribute("id", crate::namespace::table::XML_NAMESPACE, "xml", "x")
            .unwrap();
        builder.end_of_attributes();
        builder.end_element().unwrap();
        let b = builder.start_element("b", "", "", &[]).unwrap();
        builder
            .attribute("id", crate::namespace::table::XML_NAMESPACE, "xml", "y")
            .unwrap();
        builder.end_of_attributes();
        builder.end_element().unwrap();
        let doc = builder.finalize().unwrap();

        let in_a = BufferDocNavigator::new_orphan(&doc, a);
        assert!(in_a.find_element_by_id("x").unwrap().is_some(), "own tree");
        assert!(
            in_a.find_element_by_id("y").unwrap().is_none(),
            "the sibling tree's id is out of scope"
        );
        let in_b = BufferDocNavigator::new_orphan(&doc, b);
        assert!(in_b.find_element_by_id("y").unwrap().is_some(), "own tree");
        assert!(
            in_b.find_element_by_id("x").unwrap().is_none(),
            "the sibling tree's id is out of scope"
        );
    }

    // ── 12b. fn:id ───────────────────────────────────────────────────

    /// F&O §15.5.2 with the id-index key normalized: the element's `xml:id`
    /// has a trailing space, the argument has a leading one, and they meet.
    #[test]
    fn id_finds_an_element_whose_xml_id_is_padded_with_whitespace() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc(
            r#"<doc><div xml:id="id3 "><title>Expressions</title></div></doc>"#,
            &arena,
            &names,
        );
        let result =
            eval(&names, doc.create_navigator(), "string(id(' id3')/title)").expect("evaluates");
        assert_eq!(result.to_owned_string().as_deref(), Some("Expressions"));
    }

    /// F&O §15.5.2: an is-id node whose value is not a lexical `xs:ID` "will
    /// never be selected".
    #[test]
    fn id_never_selects_a_node_whose_xml_id_is_not_an_ncname() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc(r#"<doc><a xml:id="1abc"/></doc>"#, &arena, &names);
        let result = eval(&names, doc.create_navigator(), "count(id('1abc'))").expect("evaluates");
        assert_eq!(result.as_f64(), Some(0.0));
    }

    /// The candidate IDREFs are `tokenize(normalize-space($s), ' ')`, and
    /// `normalize-space` knows XML whitespace only. U+00A0 is not XML
    /// whitespace, so `"a\u{a0}b"` is a single token — and not an NCName.
    #[test]
    fn id_tokenizes_on_xml_whitespace_only() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc(
            r#"<doc><a xml:id="a"/><b xml:id="b"/></doc>"#,
            &arena,
            &names,
        );
        let result =
            eval(&names, doc.create_navigator(), "count(id('a\u{a0}b'))").expect("evaluates");
        assert_eq!(result.as_f64(), Some(0.0));
        // The same two names separated by real XML whitespace do select both.
        let result =
            eval(&names, doc.create_navigator(), "count(id(' a\t\nb '))").expect("evaluates");
        assert_eq!(result.as_f64(), Some(2.0));
    }

    /// A token that is not an NCName is ignored, not an error, and the rest of
    /// the token list still selects.
    #[test]
    fn id_ignores_a_token_that_is_not_an_ncname() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc(r#"<doc><a xml:id="a"/></doc>"#, &arena, &names);
        let result =
            eval(&names, doc.create_navigator(), "count(id('1abc a x:y'))").expect("evaluates");
        assert_eq!(result.as_f64(), Some(1.0));
    }

    /// F&O §15.5.2: "If the node ... is in a tree whose root is not a document
    /// node, [err:FODC0001] is raised."
    #[test]
    fn id_raises_fodc0001_when_the_tree_root_is_not_a_document_node() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc(r#"<doc><a xml:id="a"/></doc>"#, &arena, &names);
        let elem = {
            let mut nav = doc.create_navigator();
            assert!(nav.move_to_first_child());
            nav.current_ref()
        };
        let Err(err) = eval(
            &names,
            BufferDocNavigator::new_orphan(&doc, elem),
            "id('a')",
        ) else {
            panic!("a parentless tree root is FODC0001");
        };
        assert_eq!(err.error_code(), Some("FODC0001"));
    }

    /// Duplicates: the first in document order wins, and it is not an error.
    ///
    /// The tree is built through the push API, because the text parser refuses
    /// a duplicate `xml:id` outright (see `BufferDocumentError::DuplicateId`).
    #[test]
    fn id_selects_the_first_element_in_document_order() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = crate::document::BufferDocumentBuilder::new(
            &arena,
            &names,
            None,
            crate::document::BufferDocumentOptions::default(),
        )
        .unwrap();
        builder.start_element("doc", "", "", &[]).unwrap();
        builder.end_of_attributes();
        for (name, text) in [("a", "first"), ("b", "second")] {
            builder.start_element(name, "", "", &[]).unwrap();
            builder
                .attribute("id", crate::namespace::table::XML_NAMESPACE, "xml", "dup")
                .unwrap();
            builder.end_of_attributes();
            builder.text(text);
            builder.end_element().unwrap();
        }
        builder.end_element().unwrap();
        let doc = builder.finalize().unwrap();

        let result = eval(
            &names,
            doc.create_navigator(),
            "string-join(id('dup'), '|')",
        )
        .expect("evaluates");
        assert_eq!(result.to_owned_string().as_deref(), Some("first"));
    }

    // ── 13. Virtual parent ───────────────────────────────────────────

    #[test]
    fn attribute_parent_returns_element() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc(r#"<root x="1"/>"#, &arena, &names);
        let mut nav = doc.create_navigator();
        nav.move_to_first_child(); // root
        nav.move_to_first_attribute(); // x

        assert_eq!(nav.node_type(), DomNodeType::Attribute);
        assert!(nav.move_to_parent());
        assert_eq!(nav.node_type(), DomNodeType::Element);
        assert_eq!(nav.local_name(), "root");
    }

    #[test]
    fn namespace_parent_returns_element() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc(r#"<root xmlns:p="http://example.com"/>"#, &arena, &names);
        let mut nav = doc.create_navigator();
        nav.move_to_first_child(); // root
        nav.move_to_first_namespace(NamespaceAxisScope::Local);

        assert_eq!(nav.node_type(), DomNodeType::Namespace);
        assert!(nav.move_to_parent());
        assert_eq!(nav.node_type(), DomNodeType::Element);
        assert_eq!(nav.local_name(), "root");
    }

    // ── 14. name() — qualified name ──────────────────────────────────

    #[test]
    fn qualified_name_with_prefix() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc(
            r#"<ns:root xmlns:ns="http://example.com"/>"#,
            &arena,
            &names,
        );
        let mut nav = doc.create_navigator();
        nav.move_to_first_child();

        assert_eq!(nav.name(), "ns:root");
        assert_eq!(nav.local_name(), "root");
        assert_eq!(nav.prefix(), "ns");
        assert_eq!(nav.namespace_uri(), "http://example.com");
    }

    #[test]
    fn qualified_name_without_prefix() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc("<root/>", &arena, &names);
        let mut nav = doc.create_navigator();
        nav.move_to_first_child();

        assert_eq!(nav.name(), "root");
        assert_eq!(nav.local_name(), "root");
    }

    // ── 15. Empty, text-only, mixed ──────────────────────────────────

    #[test]
    fn empty_element() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc("<root/>", &arena, &names);
        let mut nav = doc.create_navigator();
        nav.move_to_first_child();

        assert!(!nav.move_to_first_child());
        assert_eq!(nav.value(), "");
    }

    #[test]
    fn text_only_element() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc("<root>hello</root>", &arena, &names);
        let mut nav = doc.create_navigator();
        nav.move_to_first_child(); // root

        assert_eq!(nav.value(), "hello");

        nav.move_to_first_child(); // text node
        assert_eq!(nav.node_type(), DomNodeType::Text);
        assert_eq!(nav.value(), "hello");
    }

    #[test]
    fn mixed_content_value() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc("<root>a<b>c</b>d</root>", &arena, &names);
        let mut nav = doc.create_navigator();
        nav.move_to_first_child();

        assert_eq!(nav.value(), "acd");
    }

    #[test]
    fn comment_node() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc("<root><!-- comment --></root>", &arena, &names);
        let mut nav = doc.create_navigator();
        nav.move_to_first_child(); // root
        nav.move_to_first_child(); // comment

        assert_eq!(nav.node_type(), DomNodeType::Comment);
        assert_eq!(nav.value(), " comment ");
    }

    #[test]
    fn clone_semantics() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc("<root><a/></root>", &arena, &names);
        let mut nav = doc.create_navigator();
        nav.move_to_first_child();

        let nav_clone = nav.clone();
        assert!(nav.is_same_position(&nav_clone));

        nav.move_to_first_child();
        assert!(!nav.is_same_position(&nav_clone));
    }

    #[test]
    fn no_attributes_returns_false() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc("<root/>", &arena, &names);
        let mut nav = doc.create_navigator();
        nav.move_to_first_child();

        assert!(!nav.move_to_first_attribute());
    }

    #[test]
    fn no_namespaces_local_returns_false() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc("<root/>", &arena, &names);
        let mut nav = doc.create_navigator();
        nav.move_to_first_child();

        assert!(!nav.move_to_first_namespace(NamespaceAxisScope::Local));
    }

    #[test]
    fn move_to_root_from_deep_node() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc("<root><a><b><c/></b></a></root>", &arena, &names);
        let mut nav = doc.create_navigator();
        nav.move_to_first_child(); // root
        nav.move_to_first_child(); // a
        nav.move_to_first_child(); // b
        nav.move_to_first_child(); // c

        assert_eq!(nav.local_name(), "c");
        nav.move_to_root();
        assert_eq!(nav.node_type(), DomNodeType::Root);
    }

    #[test]
    fn create_navigator_at() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc("<root><child/></root>", &arena, &names);

        // Find the child ref through normal navigation
        let mut nav = doc.create_navigator();
        nav.move_to_first_child(); // root
        nav.move_to_first_child(); // child
        let child_ref = nav.current_ref();

        // Create navigator directly at child
        let nav2 = doc.create_navigator_at(child_ref);
        assert_eq!(nav2.local_name(), "child");
        assert!(nav.is_same_position(&nav2));
    }

    // ── 12. Cross-document identity, order and serials ───────────────

    /// XPath 2.0 §3.5.3 *Node Comparisons*: "A comparison with the `is`
    /// operator is true if the two operand nodes have the same identity, and
    /// are thus the same node; otherwise it is `false`."
    ///
    /// Two documents with byte-identical content produce byte-identical node
    /// indices, so a cursor comparison that ignored the document would report
    /// two distinct nodes as the same node.
    #[test]
    fn is_same_position_is_false_across_documents() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc_a = build_doc(r#"<root x="1"><a/><b/></root>"#, &arena, &names);
        let doc_b = build_doc(r#"<root x="1"><a/><b/></root>"#, &arena, &names);

        // Element cursors: same flat index in both documents.
        let mut nav_a = doc_a.create_navigator();
        let mut nav_b = doc_b.create_navigator();
        assert!(nav_a.move_to_first_child());
        assert!(nav_b.move_to_first_child());
        assert_eq!(nav_a.current_ref(), nav_b.current_ref());
        assert!(!nav_a.is_same_position(&nav_b));
        assert!(!nav_b.is_same_position(&nav_a));

        // Root cursors.
        assert!(!doc_a
            .create_navigator()
            .is_same_position(&doc_b.create_navigator()));

        // Attribute cursors.
        let mut attr_a = nav_a.clone();
        let mut attr_b = nav_b.clone();
        assert!(attr_a.move_to_first_attribute());
        assert!(attr_b.move_to_first_attribute());
        assert!(!attr_a.is_same_position(&attr_b));

        // Within one document nothing changes.
        let other_a = doc_a.create_navigator_at(nav_a.current_ref());
        assert!(nav_a.is_same_position(&other_a));
        assert!(nav_a.is_same_position(&nav_a.clone()));
        assert!(!nav_a.is_same_position(&attr_a));
    }

    #[test]
    fn move_to_carries_the_document() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc_a = build_doc("<root><a/></root>", &arena, &names);
        let doc_b = build_doc("<root><b/></root>", &arena, &names);

        let mut nav_a = doc_a.create_navigator();
        let mut nav_b = doc_b.create_navigator();
        assert!(nav_b.move_to_first_child());
        assert!(nav_b.move_to_first_child()); // <b/>

        assert!(nav_a.move_to(&nav_b));
        assert_eq!(nav_a.local_name(), "b");
        assert!(nav_a.is_same_position(&nav_b));
        assert_eq!(nav_a.compare_position(&nav_b), XmlNodeOrder::Same);
    }

    #[test]
    fn document_serials_are_unique_and_increasing() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc_a = build_doc("<root/>", &arena, &names);
        let doc_b = build_doc("<root/>", &arena, &names);
        let doc_c = build_doc("<root/>", &arena, &names);

        assert!(doc_a.serial() < doc_b.serial());
        assert!(doc_b.serial() < doc_c.serial());
        assert_eq!(doc_a.serial(), doc_a.serial());
    }

    /// XPath 2.0 §2.4.1 *Document Order*: "If any node in a given tree T1 is
    /// before any node in a different tree T2, then all nodes in tree T1 are
    /// before all nodes in tree T2." The serial order decides which tree
    /// comes first, and every node of that tree follows it.
    #[test]
    fn compare_position_orders_documents_by_serial() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc_a = build_doc("<root><a/><b/></root>", &arena, &names);
        let doc_b = build_doc("<root><a/><b/></root>", &arena, &names);
        assert!(doc_a.serial() < doc_b.serial());

        // Every node of the earlier document precedes every node of the
        // later one, in both comparison directions.
        let mut cursor_a = doc_a.create_navigator();
        loop {
            let mut cursor_b = doc_b.create_navigator();
            loop {
                assert_eq!(cursor_a.compare_position(&cursor_b), XmlNodeOrder::Before);
                assert_eq!(cursor_b.compare_position(&cursor_a), XmlNodeOrder::After);
                if !move_to_next_in_subtree(&mut cursor_b) {
                    break;
                }
            }
            if !move_to_next_in_subtree(&mut cursor_a) {
                break;
            }
        }

        // `Same` is reachable only within one document.
        let root_a = doc_a.create_navigator();
        assert_eq!(root_a.compare_position(&root_a), XmlNodeOrder::Same);
        assert_ne!(
            doc_a
                .create_navigator()
                .compare_position(&doc_b.create_navigator()),
            XmlNodeOrder::Same
        );
    }

    /// Depth-first walk used by [`compare_position_orders_documents_by_serial`]
    /// to visit every node of a small document.
    fn move_to_next_in_subtree(nav: &mut BufferDocNavigator<'_>) -> bool {
        if nav.move_to_first_child() {
            return true;
        }
        loop {
            if nav.move_to_next_sibling() {
                return true;
            }
            if !nav.move_to_parent() {
                return false;
            }
        }
    }
    /// `new_orphan` presents a node as the root of its own tree.
    #[test]
    fn an_orphan_node_has_no_parent_and_is_its_own_root() {
        use crate::navigator::DomNavigator;
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = BufferDocument::from_reader_default(
            b"<holder><kid><deep/></kid></holder>".as_slice(),
            &arena,
            &names,
        )
        .unwrap();
        // The element the host wants to hand out as parentless.
        let holder = doc.root() + 1;
        let kid = doc.first_content_child_of(holder).unwrap();

        let mut nav = BufferDocNavigator::new_orphan(doc_ref(&arena, doc), kid);
        assert_eq!(nav.local_name(), "kid");
        assert!(!nav.move_to_parent(), "an orphan has no parent");
        assert!(!nav.move_to_next_sibling());
        assert!(!nav.move_to_prev_sibling());

        // Its descendants are unaffected and reach it again.
        assert!(nav.move_to_first_child());
        assert_eq!(nav.local_name(), "deep");
        assert!(nav.move_to_parent());
        assert_eq!(nav.local_name(), "kid");

        // fn:root() lands on the orphan, not on the document node.
        nav.move_to_root();
        assert_eq!(nav.local_name(), "kid");

        // An ordinary navigator on the same node still sees the whole tree.
        let mut plain = BufferDocNavigator::new(doc_ref(&arena, doc2(&arena, &names)), 0);
        plain.move_to_root();
        assert_eq!(plain.node_type(), DomNodeType::Root);
    }

    fn doc_ref<'a>(arena: &'a Bump, doc: BufferDocument<'a>) -> &'a BufferDocument<'a> {
        arena.alloc(doc)
    }

    fn doc2<'a>(arena: &'a Bump, names: &'a NameTable) -> BufferDocument<'a> {
        BufferDocument::from_reader_default(b"<a/>".as_slice(), arena, names).unwrap()
    }

    /// An orphan's *attributes and namespace nodes* keep it as their parent.
    ///
    /// An attribute or namespace cursor keeps `current` on the owning element,
    /// so the naive "current == orphan_root" test also swallowed the virtual
    /// nodes' parent link.
    #[test]
    fn the_virtual_nodes_of_an_orphan_still_have_it_as_their_parent() {
        use crate::navigator::{DomNavigator, NamespaceAxisScope};
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = BufferDocument::from_reader_default(
            br#"<holder><kid xmlns:p="urn:p" a="1"/></holder>"#.as_slice(),
            &arena,
            &names,
        )
        .unwrap();
        let doc = doc_ref(&arena, doc);
        let holder = doc.root() + 1;
        let kid = doc.first_content_child_of(holder).unwrap();

        // The attribute axis.
        let mut nav = BufferDocNavigator::new_orphan(doc, kid);
        assert!(nav.move_to_first_attribute());
        assert_eq!(nav.local_name(), "a");
        assert!(
            nav.move_to_parent(),
            "an attribute of an orphan has a parent"
        );
        assert_eq!(nav.local_name(), "kid");
        assert!(!nav.move_to_parent(), "and above it the tree still ends");

        // The namespace axis. `local_name()` of a namespace node is its prefix.
        let mut nav = BufferDocNavigator::new_orphan(doc, kid);
        assert!(nav.move_to_first_namespace(NamespaceAxisScope::ExcludeXml));
        assert_eq!(nav.local_name(), "p");
        assert_eq!(nav.node_type(), DomNodeType::Namespace);
        assert!(
            nav.move_to_parent(),
            "a namespace node of an orphan has a parent"
        );
        assert_eq!(nav.local_name(), "kid");

        // The ancestor axis of a namespace node is exactly the orphan.
        let mut nav = BufferDocNavigator::new_orphan(doc, kid);
        assert!(nav.move_to_first_namespace(NamespaceAxisScope::ExcludeXml));
        let mut ancestors = Vec::new();
        while nav.move_to_parent() {
            ancestors.push(nav.local_name().to_string());
        }
        assert_eq!(ancestors, vec!["kid".to_string()]);

        // root() of a namespace node of an orphan is the orphan, not the
        // document node that physically holds it.
        let mut nav = BufferDocNavigator::new_orphan(doc, kid);
        assert!(nav.move_to_first_namespace(NamespaceAxisScope::ExcludeXml));
        nav.move_to_root();
        assert_eq!(nav.node_type(), DomNodeType::Element);
        assert_eq!(nav.local_name(), "kid");
    }

    /// The parentless view travels with `move_to`.
    #[test]
    fn move_to_carries_the_orphan_view() {
        use crate::navigator::DomNavigator;
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = BufferDocument::from_reader_default(
            b"<holder><kid><deep/></kid></holder>".as_slice(),
            &arena,
            &names,
        )
        .unwrap();
        let doc = doc_ref(&arena, doc);
        let holder = doc.root() + 1;
        let kid = doc.first_content_child_of(holder).unwrap();

        let orphan = BufferDocNavigator::new_orphan(doc, kid);
        let mut cursor = doc.create_navigator();
        assert!(cursor.move_to(&orphan));
        assert!(
            !cursor.move_to_parent(),
            "move_to must carry the cut upward link"
        );
    }

    // ── 16. The scope of fn:id under new_assertion / new_orphan ──────

    /// Builds `<outer><sib xml:id=SIB/><inner>[<leaf xml:id=LEAF/>]</inner></outer>`
    /// in a `Fragment` buffer, as the validator's assertion builder does, and
    /// returns the document with the `inner` element.
    fn assertion_fragment<'a>(
        arena: &'a Bump,
        names: &'a NameTable,
        sib: Option<&str>,
        leaf: Option<&str>,
    ) -> (&'a BufferDocument<'a>, u32) {
        use crate::namespace::table::XML_NAMESPACE;
        let mut builder = crate::document::BufferDocumentBuilder::new(
            arena,
            names,
            None,
            crate::document::BufferDocumentOptions::fragment(),
        )
        .unwrap();
        builder.start_element("outer", "", "", &[]).unwrap();
        builder.end_of_attributes();
        builder.start_element("sib", "", "", &[]).unwrap();
        if let Some(id) = sib {
            builder.attribute("id", XML_NAMESPACE, "xml", id).unwrap();
        }
        builder.end_of_attributes();
        builder.end_element().unwrap();
        let inner = builder.start_element("inner", "", "", &[]).unwrap();
        builder.end_of_attributes();
        if let Some(id) = leaf {
            builder.start_element("leaf", "", "", &[]).unwrap();
            builder.attribute("id", XML_NAMESPACE, "xml", id).unwrap();
            builder.end_of_attributes();
            builder.end_element().unwrap();
        }
        builder.end_element().unwrap();
        builder.end_element().unwrap();
        (arena.alloc(builder.finalize().unwrap()), inner)
    }

    /// XSD 1.1 §3.13.4.1 clause 1.3: the assertion's data model instance
    /// "contains only that node and nodes constructed from the \[attributes\],
    /// \[children\], and descendants of E" — so `fn:id` inside an assertion
    /// must not find an element outside the asserted element, here its
    /// preceding sibling.
    #[test]
    fn id_in_an_assertion_does_not_reach_outside_the_asserted_element() {
        let arena = Bump::new();
        let names = NameTable::new();
        let (doc, inner) = assertion_fragment(&arena, &names, Some("x"), None);
        let result = eval(
            &names,
            BufferDocNavigator::new_assertion(doc, inner),
            "empty(id('x'))",
        )
        .expect("evaluates");
        assert_eq!(result.as_bool(), Some(true), "the sibling is outside E");
        assert!(BufferDocNavigator::new_assertion(doc, inner)
            .find_element_by_id("x")
            .unwrap()
            .is_none());
        // The same lookup from the outer element, whose subtree holds `sib`.
        let outer = doc.root() + 1;
        let found = BufferDocNavigator::new_assertion(doc, outer)
            .find_element_by_id("x")
            .unwrap()
            .expect("inside the outer element");
        assert_eq!(found.local_name(), "sib");
    }

    /// An element inside the asserted element is found, even when an element
    /// outside it — earlier in document order — claims the same value: the
    /// restricted lookup answers with the first claimant *inside* the subtree,
    /// not with the first claimant of the buffer filtered afterwards.
    #[test]
    fn id_in_an_assertion_finds_the_first_claimant_inside_the_asserted_element() {
        let arena = Bump::new();
        let names = NameTable::new();
        let (doc, inner) = assertion_fragment(&arena, &names, Some("k"), Some("k"));
        let found = BufferDocNavigator::new_assertion(doc, inner)
            .find_element_by_id("k")
            .unwrap()
            .expect("the leaf inside E claims k");
        assert_eq!(found.local_name(), "leaf");
        let result = eval(
            &names,
            BufferDocNavigator::new_assertion(doc, inner),
            "string(name(id('k')))",
        )
        .expect("evaluates");
        assert_eq!(result.to_owned_string().as_deref(), Some("leaf"));
    }

    /// The same for a parentless node: the orphan's own tree holds `inside`,
    /// and an earlier `outside` with the same id must not hide it.
    #[test]
    fn an_orphan_finds_the_first_claimant_inside_its_own_tree() {
        use crate::namespace::table::XML_NAMESPACE;
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = crate::document::BufferDocumentBuilder::new(
            &arena,
            &names,
            None,
            crate::document::BufferDocumentOptions::default(),
        )
        .unwrap();
        builder.start_element("holder", "", "", &[]).unwrap();
        builder.end_of_attributes();
        builder.start_element("outside", "", "", &[]).unwrap();
        builder.attribute("id", XML_NAMESPACE, "xml", "k").unwrap();
        builder.end_of_attributes();
        builder.end_element().unwrap();
        let kid = builder.start_element("kid", "", "", &[]).unwrap();
        builder.end_of_attributes();
        let inside = builder.start_element("inside", "", "", &[]).unwrap();
        builder.attribute("id", XML_NAMESPACE, "xml", "k").unwrap();
        builder.end_of_attributes();
        builder.end_element().unwrap();
        builder.end_element().unwrap();
        builder.end_element().unwrap();
        let doc = builder.finalize().unwrap();

        let found = BufferDocNavigator::new_orphan(&doc, kid)
            .find_element_by_id("k")
            .unwrap()
            .expect("inside is in the orphan's tree");
        assert_eq!(found.current_ref(), inside);
        // The hit keeps the parentless view.
        let mut up = found.clone();
        assert!(up.move_to_parent());
        assert_eq!(up.current_ref(), kid);
        assert!(!up.move_to_parent(), "the orphan is still the top");
    }

    /// A parentless text, comment or attribute node holds no element, so no id
    /// lookup from it can succeed — even though the range of nodes after an
    /// attribute runs into its owner's children.
    #[test]
    fn a_parentless_attribute_finds_no_id() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc(
            r#"<holder a="1"><kid xml:id="k"/></holder>"#,
            &arena,
            &names,
        );
        let holder = doc.root() + 1;
        let attr = holder + 1;
        assert_eq!(doc.nodes.get(attr).node_type(), NodeType::Attribute);
        assert!(BufferDocNavigator::new_orphan(&doc, attr)
            .find_element_by_id("k")
            .unwrap()
            .is_none());
    }

    /// `new_orphan` on the document node: the document node has no parent and
    /// no siblings already, so the parentless view is the ordinary one and
    /// must not hide the document's children.
    #[test]
    fn new_orphan_on_the_document_node_is_the_ordinary_navigator() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc("<r><x/></r>", &arena, &names);
        let mut nav = BufferDocNavigator::new_orphan(&doc, doc.root());
        assert!(!nav.move_to_parent());
        assert!(nav.move_to_first_child(), "the document element is visible");
        assert_eq!(nav.local_name(), "r");
        for (expr, expected) in [
            ("count(*)", 1.0),
            ("count(node())", 1.0),
            ("count(descendant::x)", 1.0),
            ("count(/r)", 1.0),
            ("count(root()/r/x)", 1.0),
        ] {
            let result = eval(
                &names,
                BufferDocNavigator::new_orphan(&doc, doc.root()),
                expr,
            )
            .expect("evaluates");
            assert_eq!(result.as_f64(), Some(expected), "{expr}");
        }
    }

    /// A reference that is not a node of the document is a caller error,
    /// reported at once rather than on first use.
    #[test]
    #[should_panic(expected = "not a node of this document")]
    fn new_orphan_panics_on_a_node_reference_out_of_range() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc("<r/>", &arena, &names);
        let _ = BufferDocNavigator::new_orphan(&doc, doc.nodes.len());
    }

    /// In a `Full` document the whole document is one tree, so a cursor on a
    /// top-level comment finds the ids of the document element.
    #[test]
    fn find_element_by_id_from_a_top_level_comment() {
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = build_doc(r#"<!--c--><?pi x?><r><a xml:id="x"/></r>"#, &arena, &names);
        let mut nav = doc.create_navigator();
        assert!(nav.move_to_first_child());
        assert_eq!(nav.node_type(), DomNodeType::Comment);
        let found = nav
            .find_element_by_id("x")
            .unwrap()
            .expect("same document, same tree");
        assert_eq!(found.local_name(), "a");
    }
}
