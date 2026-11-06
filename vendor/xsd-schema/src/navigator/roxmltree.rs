//! roxmltree adapter for DomNavigator
//!
//! Provides [`RoXmlNavigator`] - an untyped navigator over roxmltree documents.
//! Since roxmltree is a read-only, schema-unaware parser, typed value hooks
//! always return `None`.

use std::borrow::Cow;
use std::collections::HashSet;

use ::roxmltree::{Document, Node, NodeType};

use crate::ids::SimpleTypeKey;

use super::{DomNavigator, DomNodeType, NamespaceAxisScope, TypedValue, XmlNodeOrder};

/// The data of a processing instruction, which is its XDM string value.
///
/// `Node::text()` returns `None` for a PI — roxmltree keeps the data in
/// `Node::pi()` instead — so a PI's value has to be read from there.
fn pi_data<'a>(node: Node<'a, 'a>) -> &'a str {
    node.pi().and_then(|pi| pi.value).unwrap_or("")
}

/// Internal cursor state for RoXmlNavigator
#[derive(Clone)]
enum RoCursor<'a> {
    /// Positioned on a regular node (root, element, text, comment, PI)
    Node(Node<'a, 'a>),
    /// Positioned on an attribute of an element
    Attribute { owner: Node<'a, 'a>, index: usize },
    /// Positioned on a namespace declaration of an element
    Namespace {
        owner: Node<'a, 'a>,
        index: usize,
        /// Cached list of namespaces for this owner (based on scope)
        /// Tuple of (prefix_option, uri)
        namespaces: Vec<(Option<String>, String)>,
    },
}

/// Navigator adapter for roxmltree documents
///
/// This is an untyped navigator - `schema_type()` and `typed_value()`
/// always return `None` by default. For testing purposes, a schema type
/// can be set via `with_schema_type()`.
#[derive(Clone)]
pub struct RoXmlNavigator<'a> {
    /// Reference to the source document
    doc: &'a Document<'a>,
    /// Current cursor position
    cursor: RoCursor<'a>,
    /// Base URI for the document (optional, provided by caller)
    base_uri: String,
    /// Cached qualified name (prefix:local)
    name_cache: String,
    /// Schema type override for testing (normally None)
    #[cfg(test)]
    schema_type_override: Option<SimpleTypeKey>,
}

impl<'a> RoXmlNavigator<'a> {
    /// Create a navigator positioned at the document root
    pub fn new(doc: &'a Document<'a>) -> Self {
        Self {
            doc,
            cursor: RoCursor::Node(doc.root()),
            base_uri: String::new(),
            name_cache: String::new(),
            #[cfg(test)]
            schema_type_override: None,
        }
    }

    /// Create a navigator with a base URI
    pub fn with_base_uri(doc: &'a Document<'a>, base_uri: impl Into<String>) -> Self {
        Self {
            doc,
            cursor: RoCursor::Node(doc.root()),
            base_uri: base_uri.into(),
            name_cache: String::new(),
            #[cfg(test)]
            schema_type_override: None,
        }
    }

    /// Create a navigator positioned at a specific node
    pub fn at_node(doc: &'a Document<'a>, node: Node<'a, 'a>) -> Self {
        Self {
            doc,
            cursor: RoCursor::Node(node),
            base_uri: String::new(),
            name_cache: String::new(),
            #[cfg(test)]
            schema_type_override: None,
        }
    }

    /// Set a schema type for testing purposes
    #[cfg(test)]
    pub fn with_schema_type(mut self, schema_type: SimpleTypeKey) -> Self {
        self.schema_type_override = Some(schema_type);
        self
    }

    /// Get the underlying roxmltree node (if positioned on a node)
    pub fn as_node(&self) -> Option<Node<'a, 'a>> {
        match &self.cursor {
            RoCursor::Node(n) => Some(*n),
            _ => None,
        }
    }

    /// Get the owning element for attribute/namespace cursors, or the node itself
    fn owner_node(&self) -> Node<'a, 'a> {
        match &self.cursor {
            RoCursor::Node(n) => *n,
            RoCursor::Attribute { owner, .. } => *owner,
            RoCursor::Namespace { owner, .. } => *owner,
        }
    }

    /// Get document order key for comparison
    /// Returns (node_id, cursor_kind, sub_index)
    /// cursor_kind: 0 = node, 1 = namespace, 2 = attribute
    fn order_key(&self) -> (u32, u8, usize) {
        match &self.cursor {
            RoCursor::Node(n) => (n.id().get(), 0, 0),
            RoCursor::Namespace { owner, index, .. } => (owner.id().get(), 1, *index),
            RoCursor::Attribute { owner, index } => (owner.id().get(), 2, *index),
        }
    }

    /// Collect namespaces for a node based on scope
    fn collect_namespaces(
        &self,
        node: Node<'a, 'a>,
        scope: NamespaceAxisScope,
    ) -> Vec<(Option<String>, String)> {
        let mut result = Vec::new();

        match scope {
            NamespaceAxisScope::Local => {
                // Only locally declared namespaces
                // roxmltree's namespaces() returns all in-scope namespaces including inherited
                // We need to find which ones are NOT present in the parent
                let parent_prefixes: HashSet<Option<&str>> = if let Some(parent) = node.parent() {
                    if parent.is_element() {
                        parent.namespaces().map(|ns| ns.name()).collect()
                    } else {
                        HashSet::new()
                    }
                } else {
                    HashSet::new()
                };

                for ns in node.namespaces() {
                    let prefix = ns.name();
                    // If this prefix exists in parent, check if URI changed (redeclaration)
                    // If prefix doesn't exist in parent, it's a new local declaration
                    let is_local = if parent_prefixes.contains(&prefix) {
                        // Check if this is a redeclaration with different URI
                        if let Some(parent) = node.parent() {
                            parent
                                .namespaces()
                                .find(|p_ns| p_ns.name() == prefix)
                                .map(|p_ns| p_ns.uri() != ns.uri())
                                .unwrap_or(true)
                        } else {
                            true
                        }
                    } else {
                        // Prefix not in parent, so it's locally declared
                        true
                    };

                    if is_local {
                        result.push((prefix.map(String::from), ns.uri().to_string()));
                    }
                }
            }
            NamespaceAxisScope::All | NamespaceAxisScope::ExcludeXml => {
                // All in-scope namespaces - roxmltree already provides this
                let mut seen_prefixes: HashSet<Option<&str>> = HashSet::new();

                for ns in node.namespaces() {
                    let prefix = ns.name();
                    if seen_prefixes.insert(prefix) {
                        // A zero-length URI is a namespace *undeclaration*
                        // (`xmlns=""`). It cancels an outer binding rather than
                        // adding one, so under `All` — the XDM
                        // `dm:namespace-nodes` accessor, which backs the
                        // `namespace::` axis and `fn:in-scope-prefixes` — no
                        // namespace node exists for it. The other scopes are
                        // internal views of the *declarations* (serialization,
                        // shallow copy, and the namespace context used to
                        // resolve QNames while validating), where the
                        // undeclaration must stay visible.
                        if scope == NamespaceAxisScope::All && ns.uri().is_empty() {
                            continue;
                        }
                        result.push((prefix.map(String::from), ns.uri().to_string()));
                    }
                }

                match scope {
                    NamespaceAxisScope::ExcludeXml => {
                        // Drop the xml prefix; the implicit binding is not
                        // surfaced as a namespace node under this scope.
                        result.retain(|(prefix, _)| prefix.as_deref() != Some("xml"));
                    }
                    NamespaceAxisScope::All => {
                        // XDM contract: `All` always exposes the implicit `xml:`
                        // binding. roxmltree's `Node::namespaces()` seeds `xml`
                        // internally and does NOT emit it here, so inject it when
                        // absent — matching `BufferDocNavigator` so both backends
                        // agree on the `namespace::` axis.
                        let has_xml = result.iter().any(|(p, _)| p.as_deref() == Some("xml"));
                        if !has_xml {
                            result.push((
                                Some("xml".to_string()),
                                "http://www.w3.org/XML/1998/namespace".to_string(),
                            ));
                        }
                    }
                    NamespaceAxisScope::Local => unreachable!("handled above"),
                }
            }
        }

        result
    }

    /// Try to move to a matching node in document order
    fn try_move_to_matching(
        &mut self,
        start: Node<'a, 'a>,
        kind: DomNodeType,
        end_id: Option<u32>,
    ) -> bool {
        // Check if we've reached the end boundary
        if let Some(end) = end_id {
            if start.id().get() >= end {
                return false;
            }
        }

        // Check if current node matches
        let node_type = match start.node_type() {
            NodeType::Root => DomNodeType::Root,
            NodeType::Element => DomNodeType::Element,
            NodeType::Text => DomNodeType::Text,
            NodeType::Comment => DomNodeType::Comment,
            NodeType::PI => DomNodeType::ProcessingInstruction,
        };

        if kind == DomNodeType::All || kind == node_type {
            self.cursor = RoCursor::Node(start);
            return true;
        }

        // Check descendants
        for descendant in start.descendants().skip(1) {
            if let Some(end) = end_id {
                if descendant.id().get() >= end {
                    return false;
                }
            }

            let desc_type = match descendant.node_type() {
                NodeType::Root => DomNodeType::Root,
                NodeType::Element => DomNodeType::Element,
                NodeType::Text => DomNodeType::Text,
                NodeType::Comment => DomNodeType::Comment,
                NodeType::PI => DomNodeType::ProcessingInstruction,
            };

            if kind == DomNodeType::All || kind == desc_type {
                self.cursor = RoCursor::Node(descendant);
                return true;
            }
        }

        false
    }
}

/// Read a lexical QName out of a document's source text, starting at `offset`.
///
/// The scan stops at the first character that cannot be part of a name in that
/// position — whitespace, `/`, `>`, `<`, `=` or a quote — which is enough to
/// delimit an element name inside its start tag and an attribute name before
/// its `=`. Returns `None` when there is no name there, so the caller can fall
/// back to the expanded name.
///
/// "Whitespace" here is XML's, which is exactly the four characters of XML 1.0
/// §2.3 production `S ::= (#x20 | #x9 | #xD | #xA)+`, and not Unicode's: the
/// same section admits `[#x37F-#x1FFF]` into `NameStartChar`, so codepoints
/// such as U+1680 OGHAM SPACE MARK — whitespace to
/// [`char::is_whitespace`], a name character to XML — must not end the scan.
fn qname_at(text: &str, offset: usize) -> Option<&str> {
    let rest = text.get(offset..)?;
    let end = rest
        .find(|c: char| {
            matches!(
                c,
                ' ' | '\t' | '\r' | '\n' | '/' | '>' | '<' | '=' | '"' | '\''
            )
        })
        .unwrap_or(rest.len());
    if end == 0 {
        None
    } else {
        Some(&rest[..end])
    }
}

impl<'a> DomNavigator for RoXmlNavigator<'a> {
    /// Node identity, as required by the XPath 2.0 `is` operator.
    ///
    /// XPath 2.0 §3.5.3 *Node Comparisons*: "A comparison with the `is`
    /// operator is true if the two operand nodes have the same identity, and
    /// are thus the same node; otherwise it is `false`."
    ///
    /// roxmltree's `NodeId` is an index into *one* `Document`'s node vector,
    /// and an attribute/namespace index is relative to its owner, so none of
    /// the cursor fields identifies a node on their own. Identity therefore
    /// starts with the document, compared by address (a `Document` cannot
    /// move while a navigator borrows it): cursors on two different documents
    /// are never at the same position, however their ids line up.
    fn is_same_position(&self, other: &Self) -> bool {
        if !std::ptr::eq(self.doc, other.doc) {
            return false;
        }
        match (&self.cursor, &other.cursor) {
            (RoCursor::Node(a), RoCursor::Node(b)) => a.id() == b.id(),
            (
                RoCursor::Attribute { owner: a, index: i },
                RoCursor::Attribute { owner: b, index: j },
            ) => a.id() == b.id() && i == j,
            (
                RoCursor::Namespace {
                    owner: a, index: i, ..
                },
                RoCursor::Namespace {
                    owner: b, index: j, ..
                },
            ) => a.id() == b.id() && i == j,
            _ => false,
        }
    }

    /// Document order, including across trees.
    ///
    /// Cross-document ordering uses the documents' heap addresses. XPath 2.0
    /// §2.4.1 *Document Order* allows this: "The relative order of nodes in
    /// distinct trees is stable but implementation-dependent, subject to the
    /// following constraint: If any node in a given tree T1 is before any
    /// node in a different tree T2, then all nodes in tree T1 are before all
    /// nodes in tree T2." Ordering whole trees by an address that cannot
    /// change while the navigators borrow them satisfies both the constraint
    /// and the stability requirement for the duration of an expression. It is
    /// not reproducible across runs; the schema-aware `BufferDocNavigator`
    /// (`xsd11` feature) uses a document serial instead, for callers that
    /// need that.
    fn compare_position(&self, other: &Self) -> XmlNodeOrder {
        if !std::ptr::eq(self.doc, other.doc) {
            let self_ptr = self.doc as *const _ as usize;
            let other_ptr = other.doc as *const _ as usize;
            return if self_ptr < other_ptr {
                XmlNodeOrder::Before
            } else {
                XmlNodeOrder::After
            };
        }

        let self_key = self.order_key();
        let other_key = other.order_key();

        match self_key.cmp(&other_key) {
            std::cmp::Ordering::Less => XmlNodeOrder::Before,
            std::cmp::Ordering::Greater => XmlNodeOrder::After,
            std::cmp::Ordering::Equal => XmlNodeOrder::Same,
        }
    }

    /// Moves this cursor onto `other`'s position — including `other`'s
    /// document, so that moving onto a node of another tree lands on that
    /// node rather than on this tree's node of the same id. The base URI
    /// follows the document, since it describes the document and not the
    /// cursor.
    fn move_to(&mut self, other: &Self) -> bool {
        if !std::ptr::eq(self.doc, other.doc) {
            self.doc = other.doc;
            self.base_uri.clone_from(&other.base_uri);
        }
        self.cursor = other.cursor.clone();
        self.name_cache.clear();
        true
    }

    fn move_to_root(&mut self) {
        self.cursor = RoCursor::Node(self.doc.root());
        self.name_cache.clear();
    }

    fn move_to_parent(&mut self) -> bool {
        match &self.cursor {
            RoCursor::Node(n) => {
                if let Some(parent) = n.parent() {
                    self.cursor = RoCursor::Node(parent);
                    self.name_cache.clear();
                    true
                } else {
                    false
                }
            }
            RoCursor::Attribute { owner, .. } | RoCursor::Namespace { owner, .. } => {
                self.cursor = RoCursor::Node(*owner);
                self.name_cache.clear();
                true
            }
        }
    }

    fn move_to_first_child(&mut self) -> bool {
        match &self.cursor {
            RoCursor::Node(n) => {
                if let Some(child) = n.first_child() {
                    self.cursor = RoCursor::Node(child);
                    self.name_cache.clear();
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn move_to_next_sibling(&mut self) -> bool {
        match &self.cursor {
            RoCursor::Node(n) => {
                if let Some(sibling) = n.next_sibling() {
                    self.cursor = RoCursor::Node(sibling);
                    self.name_cache.clear();
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn move_to_prev_sibling(&mut self) -> bool {
        match &self.cursor {
            RoCursor::Node(n) => {
                if let Some(sibling) = n.prev_sibling() {
                    self.cursor = RoCursor::Node(sibling);
                    self.name_cache.clear();
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn move_to_first_attribute(&mut self) -> bool {
        match &self.cursor {
            RoCursor::Node(n) if n.is_element() && n.attributes().len() > 0 => {
                self.cursor = RoCursor::Attribute {
                    owner: *n,
                    index: 0,
                };
                self.name_cache.clear();
                true
            }
            _ => false,
        }
    }

    fn move_to_next_attribute(&mut self) -> bool {
        match &self.cursor {
            RoCursor::Attribute { owner, index } => {
                let next_index = index + 1;
                if next_index < owner.attributes().len() {
                    self.cursor = RoCursor::Attribute {
                        owner: *owner,
                        index: next_index,
                    };
                    self.name_cache.clear();
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn move_to_first_namespace(&mut self, scope: NamespaceAxisScope) -> bool {
        match &self.cursor {
            RoCursor::Node(n) if n.is_element() => {
                let namespaces = self.collect_namespaces(*n, scope);
                if !namespaces.is_empty() {
                    self.cursor = RoCursor::Namespace {
                        owner: *n,
                        index: 0,
                        namespaces,
                    };
                    self.name_cache.clear();
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn move_to_next_namespace(&mut self, _scope: NamespaceAxisScope) -> bool {
        match &self.cursor {
            RoCursor::Namespace {
                owner,
                index,
                namespaces,
            } => {
                let next_index = index + 1;
                if next_index < namespaces.len() {
                    self.cursor = RoCursor::Namespace {
                        owner: *owner,
                        index: next_index,
                        namespaces: namespaces.clone(),
                    };
                    self.name_cache.clear();
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn move_to_following(&mut self, kind: DomNodeType, end: Option<&Self>) -> bool {
        // Start from the current node (or owner if on attribute/namespace)
        let start = self.owner_node();
        let end_id = end.map(|e| e.owner_node().id().get());

        // First try siblings of current node
        let mut current = start;

        loop {
            // Try next sibling and its descendants
            if let Some(next) = current.next_sibling() {
                if self.try_move_to_matching(next, kind, end_id) {
                    return true;
                }
                current = next;
            } else if let Some(parent) = current.parent() {
                // No more siblings, go up to parent and try its next sibling
                current = parent;
            } else {
                // Reached root without finding a match
                break;
            }
        }

        false
    }

    fn node_type(&self) -> DomNodeType {
        match &self.cursor {
            RoCursor::Node(n) => match n.node_type() {
                NodeType::Root => DomNodeType::Root,
                NodeType::Element => DomNodeType::Element,
                NodeType::Text => DomNodeType::Text,
                NodeType::Comment => DomNodeType::Comment,
                NodeType::PI => DomNodeType::ProcessingInstruction,
            },
            RoCursor::Attribute { .. } => DomNodeType::Attribute,
            RoCursor::Namespace { .. } => DomNodeType::Namespace,
        }
    }

    fn local_name(&self) -> &str {
        match &self.cursor {
            RoCursor::Node(n) => {
                // For PI nodes, the local name is the target
                if let Some(pi) = n.pi() {
                    pi.target
                } else {
                    n.tag_name().name()
                }
            }
            RoCursor::Attribute { owner, index } => owner
                .attributes()
                .nth(*index)
                .map(|a| a.name())
                .unwrap_or(""),
            RoCursor::Namespace {
                namespaces, index, ..
            } => namespaces
                .get(*index)
                .and_then(|(prefix, _)| prefix.as_deref())
                .unwrap_or(""), // Empty string for default namespace
        }
    }

    fn name(&self) -> &str {
        match &self.cursor {
            RoCursor::Node(n) => {
                // For PI nodes, the name is the target.
                if let Some(pi) = n.pi() {
                    pi.target
                } else if n.is_element() {
                    // roxmltree exposes the *expanded* name only, and this
                    // accessor has to hand back a borrowed `&str`, so the
                    // lexical QName is read back out of the document's own
                    // source text: an element's range starts at its `<`, and
                    // its QName follows immediately.
                    qname_at(self.doc.input_text(), n.range().start + 1)
                        .unwrap_or_else(|| n.tag_name().name())
                } else {
                    // Text, comment and document nodes have no name.
                    n.tag_name().name()
                }
            }
            RoCursor::Attribute { owner, index } => owner
                .attributes()
                .nth(*index)
                .map(|a| {
                    // An attribute's position is the offset of its name.
                    qname_at(self.doc.input_text(), a.position()).unwrap_or_else(|| a.name())
                })
                .unwrap_or(""),
            // A namespace node's name is its prefix, which carries no prefix
            // of its own.
            RoCursor::Namespace {
                namespaces, index, ..
            } => namespaces
                .get(*index)
                .and_then(|(prefix, _)| prefix.as_deref())
                .unwrap_or(""),
        }
    }

    fn namespace_uri(&self) -> &str {
        match &self.cursor {
            RoCursor::Node(n) => n.tag_name().namespace().unwrap_or(""),
            RoCursor::Attribute { owner, index } => owner
                .attributes()
                .nth(*index)
                .and_then(|a| a.namespace())
                .unwrap_or(""),
            RoCursor::Namespace { .. } => {
                // Namespace nodes themselves don't have a namespace URI
                ""
            }
        }
    }

    fn prefix(&self) -> &str {
        match &self.cursor {
            RoCursor::Node(n) => {
                if n.is_element() {
                    // Reverse-lookup: find the prefix for this element's namespace URI
                    if let Some(uri) = n.tag_name().namespace() {
                        n.lookup_prefix(uri).unwrap_or("")
                    } else {
                        ""
                    }
                } else {
                    ""
                }
            }
            RoCursor::Attribute { owner, index } => {
                // Reverse-lookup: find the prefix for this attribute's namespace URI
                if let Some(attr) = owner.attributes().nth(*index) {
                    if let Some(ns_uri) = attr.namespace() {
                        owner.lookup_prefix(ns_uri).unwrap_or("")
                    } else {
                        ""
                    }
                } else {
                    ""
                }
            }
            RoCursor::Namespace { .. } => "",
        }
    }

    fn value(&self) -> String {
        match &self.cursor {
            RoCursor::Node(n) => match n.node_type() {
                NodeType::Text | NodeType::Comment => n.text().unwrap_or("").to_string(),
                NodeType::PI => {
                    // The string value of a processing instruction is its data
                    // (XDM). `Node::text()` does not report it; `pi()` does.
                    pi_data(*n).to_string()
                }
                NodeType::Element | NodeType::Root => {
                    // String value is concatenation of all text descendants
                    let mut result = String::new();
                    for descendant in n.descendants() {
                        if descendant.is_text() {
                            if let Some(text) = descendant.text() {
                                result.push_str(text);
                            }
                        }
                    }
                    result
                }
            },
            RoCursor::Attribute { owner, index } => owner
                .attributes()
                .nth(*index)
                .map(|a| a.value().to_string())
                .unwrap_or_default(),
            RoCursor::Namespace {
                namespaces, index, ..
            } => namespaces
                .get(*index)
                .map(|(_, uri)| uri.clone())
                .unwrap_or_default(),
        }
    }

    fn value_ref(&self) -> Cow<'_, str> {
        // Mirror of `value()`: roxmltree returns document-lifetime `&'a str` for
        // text/PI/comment nodes and attribute values, so borrow them; only the
        // element/root descendant concatenation must allocate.
        match &self.cursor {
            RoCursor::Node(n) => match n.node_type() {
                NodeType::Text | NodeType::Comment => Cow::Borrowed(n.text().unwrap_or("")),
                // A PI's string value is its data, which `text()` never reports.
                NodeType::PI => Cow::Borrowed(pi_data(*n)),
                NodeType::Element | NodeType::Root => Cow::Owned(self.value()),
            },
            RoCursor::Attribute { owner, index } => Cow::Borrowed(
                owner
                    .attributes()
                    .nth(*index)
                    .map(|a| a.value())
                    .unwrap_or(""),
            ),
            RoCursor::Namespace {
                namespaces, index, ..
            } => Cow::Borrowed(
                namespaces
                    .get(*index)
                    .map(|(_, uri)| uri.as_str())
                    .unwrap_or(""),
            ),
        }
    }

    fn base_uri(&self) -> &str {
        &self.base_uri
    }

    fn schema_type(&self) -> Option<SimpleTypeKey> {
        // roxmltree is schema-unaware, but can be overridden for testing
        #[cfg(test)]
        {
            if self.schema_type_override.is_some() {
                return self.schema_type_override;
            }
        }
        None
    }

    /// `DomNavigator` states that `schema_type()` is the simple-type projection
    /// of `type_annotation()`, so the test-only override has to answer both or
    /// a node would report a simple type through one accessor and none through
    /// the other. Outside tests this navigator is schema-unaware and the answer
    /// is `None`, exactly as the trait's default provides.
    fn type_annotation(&self) -> Option<crate::ids::TypeKey> {
        #[cfg(test)]
        {
            if let Some(key) = self.schema_type_override {
                return Some(crate::ids::TypeKey::Simple(key));
            }
        }
        None
    }

    fn typed_value(&self) -> TypedValue {
        // roxmltree is schema-unaware — all nodes are untyped
        TypedValue::Untyped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(xml: &str) -> Document<'_> {
        Document::parse(xml).unwrap()
    }

    #[test]
    fn pi_value_is_the_data() {
        // XDM: the string value of a processing instruction is its data.
        let doc = parse("<root><?go now ?></root>");
        let mut nav = RoXmlNavigator::new(&doc);
        assert!(nav.move_to_first_child());
        assert!(nav.move_to_first_child());
        assert_eq!(nav.node_type(), DomNodeType::ProcessingInstruction);
        assert_eq!(nav.local_name(), "go");
        assert_eq!(nav.value(), "now ");
        assert_eq!(nav.value_ref(), "now ");

        let doc = parse("<root><?go?></root>");
        let mut nav = RoXmlNavigator::new(&doc);
        assert!(nav.move_to_first_child());
        assert!(nav.move_to_first_child());
        assert_eq!(nav.value(), "");
        assert_eq!(nav.value_ref(), "");
    }

    #[test]
    fn test_navigator_at_root() {
        let doc = parse("<root/>");
        let nav = RoXmlNavigator::new(&doc);
        assert_eq!(nav.node_type(), DomNodeType::Root);
    }

    #[test]
    fn test_move_to_first_child() {
        let doc = parse("<root><child/></root>");
        let mut nav = RoXmlNavigator::new(&doc);

        // Move from root to document element
        assert!(nav.move_to_first_child());
        assert_eq!(nav.node_type(), DomNodeType::Element);
        assert_eq!(nav.local_name(), "root");

        // Move to child element
        assert!(nav.move_to_first_child());
        assert_eq!(nav.local_name(), "child");
    }

    #[test]
    fn test_move_to_parent() {
        let doc = parse("<root><child/></root>");
        let mut nav = RoXmlNavigator::new(&doc);

        nav.move_to_first_child(); // to root element
        nav.move_to_first_child(); // to child

        assert!(nav.move_to_parent());
        assert_eq!(nav.local_name(), "root");

        assert!(nav.move_to_parent());
        assert_eq!(nav.node_type(), DomNodeType::Root);
    }

    #[test]
    fn test_sibling_navigation() {
        let doc = parse("<root><a/><b/><c/></root>");
        let mut nav = RoXmlNavigator::new(&doc);

        nav.move_to_first_child(); // to root
        nav.move_to_first_child(); // to a

        assert_eq!(nav.local_name(), "a");
        assert!(nav.move_to_next_sibling());
        assert_eq!(nav.local_name(), "b");
        assert!(nav.move_to_next_sibling());
        assert_eq!(nav.local_name(), "c");
        assert!(!nav.move_to_next_sibling());

        assert!(nav.move_to_prev_sibling());
        assert_eq!(nav.local_name(), "b");
    }

    #[test]
    fn test_attribute_navigation() {
        let doc = parse(r#"<root attr1="v1" attr2="v2"/>"#);
        let mut nav = RoXmlNavigator::new(&doc);

        nav.move_to_first_child(); // to root

        assert!(nav.move_to_first_attribute());
        assert_eq!(nav.node_type(), DomNodeType::Attribute);
        assert_eq!(nav.local_name(), "attr1");
        assert_eq!(nav.value(), "v1");

        assert!(nav.move_to_next_attribute());
        assert_eq!(nav.local_name(), "attr2");
        assert_eq!(nav.value(), "v2");

        assert!(!nav.move_to_next_attribute());

        // Return to parent element
        assert!(nav.move_to_parent());
        assert_eq!(nav.node_type(), DomNodeType::Element);
    }

    #[test]
    fn test_namespace_navigation_local() {
        let doc = parse(r#"<root xmlns="http://default" xmlns:p="http://prefixed"/>"#);
        let mut nav = RoXmlNavigator::new(&doc);

        nav.move_to_first_child(); // to root

        assert!(nav.move_to_first_namespace(NamespaceAxisScope::Local));
        assert_eq!(nav.node_type(), DomNodeType::Namespace);

        // Collect all namespaces
        let mut uris = HashSet::new();
        uris.insert(nav.value());

        while nav.move_to_next_namespace(NamespaceAxisScope::Local) {
            uris.insert(nav.value());
        }

        // Should have two namespaces
        assert!(uris.contains("http://default"));
        assert!(uris.contains("http://prefixed"));
    }

    #[test]
    fn test_element_string_value() {
        let doc = parse("<root>Hello <b>World</b>!</root>");
        let mut nav = RoXmlNavigator::new(&doc);

        nav.move_to_first_child();
        assert_eq!(nav.value(), "Hello World!");
    }

    #[test]
    fn test_is_same_position() {
        let doc = parse("<root><a/><b/></root>");
        let mut nav1 = RoXmlNavigator::new(&doc);
        let mut nav2 = RoXmlNavigator::new(&doc);

        assert!(nav1.is_same_position(&nav2));

        nav1.move_to_first_child();
        assert!(!nav1.is_same_position(&nav2));

        nav2.move_to_first_child();
        assert!(nav1.is_same_position(&nav2));
    }

    #[test]
    fn test_compare_position() {
        let doc = parse("<root><a/><b/></root>");
        let mut nav1 = RoXmlNavigator::new(&doc);
        let mut nav2 = RoXmlNavigator::new(&doc);

        nav1.move_to_first_child(); // to root
        nav1.move_to_first_child(); // to a

        nav2.move_to_first_child(); // to root
        nav2.move_to_first_child(); // to a
        nav2.move_to_next_sibling(); // to b

        assert_eq!(nav1.compare_position(&nav2), XmlNodeOrder::Before);
        assert_eq!(nav2.compare_position(&nav1), XmlNodeOrder::After);
        assert_eq!(nav1.compare_position(&nav1), XmlNodeOrder::Same);
    }

    #[test]
    fn test_move_to() {
        let doc = parse("<root><a/><b/></root>");
        let mut nav1 = RoXmlNavigator::new(&doc);
        let mut nav2 = RoXmlNavigator::new(&doc);

        nav2.move_to_first_child();
        nav2.move_to_first_child();
        nav2.move_to_next_sibling(); // nav2 at <b/>

        assert!(nav1.move_to(&nav2));
        assert!(nav1.is_same_position(&nav2));
        assert_eq!(nav1.local_name(), "b");
    }

    #[test]
    fn test_clone() {
        let doc = parse("<root><a/></root>");
        let mut nav = RoXmlNavigator::new(&doc);
        nav.move_to_first_child();

        let nav_clone = nav.clone();
        assert!(nav.is_same_position(&nav_clone));

        nav.move_to_first_child();
        assert!(!nav.is_same_position(&nav_clone));
    }

    #[test]
    fn test_typed_value_returns_untyped() {
        let doc = parse("<root>text</root>");
        let nav = RoXmlNavigator::new(&doc);

        assert!(nav.schema_type().is_none());
        assert_eq!(nav.typed_value(), TypedValue::Untyped);
    }

    #[test]
    fn test_namespaced_element() {
        let doc = parse(r#"<root xmlns:ns="http://example.com"><ns:child/></root>"#);
        let mut nav = RoXmlNavigator::new(&doc);

        nav.move_to_first_child(); // root
        nav.move_to_first_child(); // ns:child

        assert_eq!(nav.local_name(), "child");
        assert_eq!(nav.namespace_uri(), "http://example.com");
    }

    #[test]
    fn test_text_node() {
        let doc = parse("<root>hello</root>");
        let mut nav = RoXmlNavigator::new(&doc);

        nav.move_to_first_child(); // root
        nav.move_to_first_child(); // text node

        assert_eq!(nav.node_type(), DomNodeType::Text);
        assert_eq!(nav.value(), "hello");
    }

    #[test]
    fn test_comment_node() {
        let doc = parse("<root><!-- comment --></root>");
        let mut nav = RoXmlNavigator::new(&doc);

        nav.move_to_first_child(); // root
        nav.move_to_first_child(); // comment node

        assert_eq!(nav.node_type(), DomNodeType::Comment);
        assert_eq!(nav.value(), " comment ");
    }

    #[test]
    fn test_processing_instruction() {
        let doc = parse("<root><?target data?></root>");
        let mut nav = RoXmlNavigator::new(&doc);

        nav.move_to_first_child(); // root
        nav.move_to_first_child(); // PI node

        assert_eq!(nav.node_type(), DomNodeType::ProcessingInstruction);
        assert_eq!(nav.local_name(), "target");
    }

    #[test]
    fn test_no_attributes() {
        let doc = parse("<root/>");
        let mut nav = RoXmlNavigator::new(&doc);

        nav.move_to_first_child(); // root

        assert!(!nav.move_to_first_attribute());
    }

    #[test]
    fn test_no_namespaces() {
        let doc = parse("<root/>");
        let mut nav = RoXmlNavigator::new(&doc);

        nav.move_to_first_child(); // root

        assert!(!nav.move_to_first_namespace(NamespaceAxisScope::Local));
    }

    #[test]
    fn test_all_axis_exposes_implicit_xml_on_bare_element() {
        // XDM contract (A): `All` always exposes the implicit `xml:` binding,
        // even on an element with no declarations. roxmltree's `namespaces()`
        // omits it, so RoXmlNavigator injects it — matching BufferDocNavigator.
        let doc = parse("<root/>");
        let mut nav = RoXmlNavigator::new(&doc);
        nav.move_to_first_child(); // root

        assert!(nav.move_to_first_namespace(NamespaceAxisScope::All));
        let mut uris = std::collections::HashSet::new();
        uris.insert(nav.value());
        while nav.move_to_next_namespace(NamespaceAxisScope::All) {
            uris.insert(nav.value());
        }
        assert!(
            uris.contains("http://www.w3.org/XML/1998/namespace"),
            "All scope must expose the implicit xml namespace on a bare element"
        );

        // ExcludeXml on the same bare element is empty.
        assert!(!nav.move_to_first_namespace(NamespaceAxisScope::ExcludeXml));
    }

    #[test]
    fn test_has_attributes_helper() {
        let doc = parse(r#"<root attr="val"/>"#);
        let mut nav = RoXmlNavigator::new(&doc);

        nav.move_to_first_child(); // root

        assert!(nav.has_attributes());
        // Should be back at element
        assert_eq!(nav.node_type(), DomNodeType::Element);
    }

    #[test]
    fn test_has_children_helper() {
        let doc = parse("<root><child/></root>");
        let mut nav = RoXmlNavigator::new(&doc);

        nav.move_to_first_child(); // root

        assert!(nav.has_children());
        // Should be back at element
        assert_eq!(nav.node_type(), DomNodeType::Element);
        assert_eq!(nav.local_name(), "root");
    }

    #[test]
    fn test_move_to_child_kind() {
        let doc = parse("<root>text<child/></root>");
        let mut nav = RoXmlNavigator::new(&doc);

        nav.move_to_first_child(); // root

        // Find element child
        assert!(nav.move_to_child_kind(DomNodeType::Element));
        assert_eq!(nav.local_name(), "child");

        nav.move_to_parent();

        // Find text child
        assert!(nav.move_to_child_kind(DomNodeType::Text));
        assert_eq!(nav.value(), "text");
    }

    #[test]
    fn test_move_to_child_name() {
        let doc = parse("<root><a/><b/><c/></root>");
        let mut nav = RoXmlNavigator::new(&doc);

        nav.move_to_first_child(); // root

        assert!(nav.move_to_child_name("b", ""));
        assert_eq!(nav.local_name(), "b");
    }

    #[test]
    fn test_different_documents_compare() {
        let doc1 = parse("<root1/>");
        let doc2 = parse("<root2/>");

        let nav1 = RoXmlNavigator::new(&doc1);
        let nav2 = RoXmlNavigator::new(&doc2);

        // Cross-document ordering is implementation-defined but must be consistent
        let order = nav1.compare_position(&nav2);
        assert!(order == XmlNodeOrder::Before || order == XmlNodeOrder::After);
        // Reverse comparison must be opposite
        let reverse = nav2.compare_position(&nav1);
        assert_ne!(order, reverse);
    }

    #[test]
    fn test_inherited_namespaces() {
        let doc = parse(
            r#"<root xmlns:ns="http://example.com"><child xmlns:local="http://local.com"/></root>"#,
        );
        let mut nav = RoXmlNavigator::new(&doc);

        nav.move_to_first_child(); // root
        nav.move_to_first_child(); // child

        // Child should see inherited namespace with All scope
        // Both ns and local should be visible
        assert!(nav.move_to_first_namespace(NamespaceAxisScope::All));

        // Collect all namespace URIs in All scope
        let mut all_uris = HashSet::new();
        all_uris.insert(nav.value());
        while nav.move_to_next_namespace(NamespaceAxisScope::All) {
            all_uris.insert(nav.value());
        }

        // Should have both inherited (ns) and local (local) namespaces
        assert!(
            all_uris.contains("http://example.com"),
            "Should see inherited namespace"
        );
        assert!(
            all_uris.contains("http://local.com"),
            "Should see local namespace"
        );

        // Now test Local scope - should only have local namespace
        let mut nav2 = RoXmlNavigator::new(&doc);
        nav2.move_to_first_child(); // root
        nav2.move_to_first_child(); // child

        assert!(nav2.move_to_first_namespace(NamespaceAxisScope::Local));

        // Collect all local namespace URIs
        let mut local_uris = HashSet::new();
        local_uris.insert(nav2.value());
        while nav2.move_to_next_namespace(NamespaceAxisScope::Local) {
            local_uris.insert(nav2.value());
        }

        // Should only have the local namespace, not inherited
        assert!(
            local_uris.contains("http://local.com"),
            "Should see local namespace"
        );
        // The inherited namespace from root should NOT be visible in Local scope
        assert!(
            !local_uris.contains("http://example.com"),
            "Should NOT see inherited namespace in Local scope"
        );
    }
    // ── Cross-document node identity ─────────────────────────────────

    /// XPath 2.0 §3.5.3 *Node Comparisons*: "A comparison with the `is`
    /// operator is true if the two operand nodes have the same identity, and
    /// are thus the same node; otherwise it is `false`."
    ///
    /// roxmltree hands out `NodeId`s per `Document`, so two documents with
    /// identical content give their corresponding nodes identical ids — the
    /// document has to take part in the identity test.
    #[test]
    fn is_same_position_is_false_across_documents() {
        let doc_a = parse(r#"<root x="1" y="2" xmlns:n="http://example.com/"><a/><b/></root>"#);
        let doc_b = parse(r#"<root x="1" y="2" xmlns:n="http://example.com/"><a/><b/></root>"#);

        // Root cursors.
        let nav_a = RoXmlNavigator::new(&doc_a);
        let nav_b = RoXmlNavigator::new(&doc_b);
        assert!(!nav_a.is_same_position(&nav_b));
        assert!(!nav_b.is_same_position(&nav_a));

        // Element cursors — same NodeId in both documents.
        let mut elem_a = RoXmlNavigator::new(&doc_a);
        let mut elem_b = RoXmlNavigator::new(&doc_b);
        assert!(elem_a.move_to_first_child());
        assert!(elem_b.move_to_first_child());
        assert_eq!(
            elem_a.as_node().unwrap().id(),
            elem_b.as_node().unwrap().id()
        );
        assert!(!elem_a.is_same_position(&elem_b));

        // Attribute cursors — same owner id, same index.
        let mut attr_a = elem_a.clone();
        let mut attr_b = elem_b.clone();
        assert!(attr_a.move_to_first_attribute());
        assert!(attr_b.move_to_first_attribute());
        assert!(!attr_a.is_same_position(&attr_b));
        assert!(attr_a.move_to_next_attribute());
        assert!(attr_b.move_to_next_attribute());
        assert!(!attr_a.is_same_position(&attr_b));

        // Namespace cursors — same owner id, same index.
        let mut ns_a = elem_a.clone();
        let mut ns_b = elem_b.clone();
        assert!(ns_a.move_to_first_namespace(NamespaceAxisScope::All));
        assert!(ns_b.move_to_first_namespace(NamespaceAxisScope::All));
        assert!(!ns_a.is_same_position(&ns_b));

        // Within one document nothing changes.
        assert!(elem_a.is_same_position(&elem_a.clone()));
        assert!(attr_a.is_same_position(&attr_a.clone()));
        assert!(ns_a.is_same_position(&ns_a.clone()));
        assert!(!elem_a.is_same_position(&attr_a));
        assert!(RoXmlNavigator::new(&doc_a).is_same_position(&RoXmlNavigator::new(&doc_a)));
    }

    #[test]
    fn move_to_carries_the_document() {
        let doc_a = parse("<root><a/></root>");
        let doc_b = parse("<root><b/></root>");

        let mut nav_a = RoXmlNavigator::with_base_uri(&doc_a, "http://example.com/a.xml");
        let mut nav_b = RoXmlNavigator::with_base_uri(&doc_b, "http://example.com/b.xml");
        assert!(nav_b.move_to_first_child());
        assert!(nav_b.move_to_first_child()); // <b/>

        assert!(nav_a.move_to(&nav_b));
        assert_eq!(nav_a.local_name(), "b");
        assert!(nav_a.is_same_position(&nav_b));
        assert_eq!(nav_a.compare_position(&nav_b), XmlNodeOrder::Same);
        assert_eq!(nav_a.base_uri(), "http://example.com/b.xml");
    }

    /// XPath 2.0 §2.4.1 *Document Order*: "If any node in a given tree T1 is
    /// before any node in a different tree T2, then all nodes in tree T1 are
    /// before all nodes in tree T2."
    #[test]
    fn compare_position_across_documents_is_a_consistent_block_order() {
        let doc_a = parse("<root><a/><b/></root>");
        let doc_b = parse("<root><a/><b/></root>");

        let root_a = RoXmlNavigator::new(&doc_a);
        let root_b = RoXmlNavigator::new(&doc_b);
        let first = root_a.compare_position(&root_b);
        assert_ne!(first, XmlNodeOrder::Same);
        assert_eq!(
            root_b.compare_position(&root_a),
            if first == XmlNodeOrder::Before {
                XmlNodeOrder::After
            } else {
                XmlNodeOrder::Before
            }
        );

        // Whichever tree wins, every one of its nodes wins too.
        let mut cursor_a = RoXmlNavigator::new(&doc_a);
        while cursor_a.move_to_first_child() {
            let mut cursor_b = RoXmlNavigator::new(&doc_b);
            loop {
                assert_eq!(cursor_a.compare_position(&cursor_b), first);
                if !cursor_b.move_to_first_child() {
                    break;
                }
            }
        }
    }
    /// `fn:name`'s lexical QName keeps the prefix as the document wrote it.
    #[test]
    fn name_returns_the_qualified_name() {
        let xml = r#"<a:r xmlns:a="http://a/" a:att="v" plain="w"><a:c/><plain/><?pi data?><!--c--></a:r>"#;
        let doc = Document::parse(xml).unwrap();
        let mut nav = RoXmlNavigator::new(&doc);

        // Document node has no name.
        assert_eq!(nav.name(), "");

        assert!(nav.move_to_first_child());
        assert_eq!(nav.name(), "a:r");
        assert_eq!(nav.local_name(), "r");
        assert_eq!(nav.prefix(), "a");

        // Attributes, prefixed and not.
        let mut attrs = nav.clone();
        assert!(attrs.move_to_first_attribute());
        assert_eq!(attrs.name(), "a:att");
        assert_eq!(attrs.local_name(), "att");
        assert!(attrs.move_to_next_attribute());
        assert_eq!(attrs.name(), "plain");

        // A namespace node's name is its prefix.
        let mut ns = nav.clone();
        assert!(ns.move_to_first_namespace(crate::navigator::NamespaceAxisScope::All));
        assert_eq!(ns.name(), "a");

        // Children: prefixed element, unprefixed element, PI, comment.
        let mut child = nav.clone();
        assert!(child.move_to_first_child());
        assert_eq!(child.name(), "a:c");
        assert!(child.move_to_next_sibling());
        assert_eq!(child.name(), "plain");
        assert!(child.move_to_next_sibling());
        assert_eq!(child.name(), "pi");
        assert!(child.move_to_next_sibling());
        assert_eq!(child.name(), "");
    }

    /// A start tag may be spelled with any amount of trailing whitespace, and
    /// an empty element closes with `/>`; the scan must stop at both.
    #[test]
    fn name_handles_every_start_tag_shape() {
        for (xml, expected) in [
            ("<r/>", "r"),
            ("<r />", "r"),
            ("<r></r>", "r"),
            ("<r\n  a='1'/>", "r"),
            (r#"<p:r xmlns:p="http://p/"/>"#, "p:r"),
            (r#"<p:r xmlns:p="http://p/"   >x</p:r>"#, "p:r"),
        ] {
            let doc = Document::parse(xml).unwrap();
            let mut nav = RoXmlNavigator::new(&doc);
            assert!(nav.move_to_first_child());
            assert_eq!(nav.name(), expected, "{xml}");
        }
    }

    /// The name scan uses XML's whitespace, not Unicode's.
    ///
    /// XML 1.0 (5th ed.) §2.3 gives `S ::= (#x20 | #x9 | #xD | #xA)+` and, in
    /// the same production group, admits `[#x37F-#x1FFF]` into `NameStartChar`
    /// (and so into `NameChar`). U+1680 OGHAM SPACE MARK falls in that range:
    /// it is a legal name character that [`char::is_whitespace`] nevertheless
    /// reports as whitespace, so a Unicode scan would truncate the name.
    #[test]
    fn a_name_character_that_unicode_calls_whitespace_does_not_end_the_name() {
        // U+1680 is inside NameStartChar's [#x37F-#x1FFF] range, and Unicode
        // classifies it as whitespace — the two disagree, which is the point.
        assert!('\u{1680}'.is_whitespace());

        let xml = "<a\u{1680}b a\u{1680}b='v'/>";
        let doc = Document::parse(xml).unwrap();
        let mut nav = RoXmlNavigator::new(&doc);
        assert!(nav.move_to_first_child());
        assert_eq!(nav.name(), "a\u{1680}b");
        assert_eq!(nav.local_name(), "a\u{1680}b");

        let mut attr = nav.clone();
        assert!(attr.move_to_first_attribute());
        assert_eq!(attr.name(), "a\u{1680}b");
        assert_eq!(attr.local_name(), "a\u{1680}b");

        // The prefixed forms, where truncation would also lose the colon.
        let xml = "<p:a\u{1680}b xmlns:p=\"http://p/\" p:a\u{1680}b=\"v\"/>";
        let doc = Document::parse(xml).unwrap();
        let mut nav = RoXmlNavigator::new(&doc);
        assert!(nav.move_to_first_child());
        assert_eq!(nav.name(), "p:a\u{1680}b");
        assert_eq!(nav.local_name(), "a\u{1680}b");
        assert_eq!(nav.prefix(), "p");

        let mut attr = nav.clone();
        assert!(attr.move_to_first_attribute());
        assert_eq!(attr.name(), "p:a\u{1680}b");
        assert_eq!(attr.local_name(), "a\u{1680}b");

        // A prefix of its own may hold one too.
        let xml = "<p\u{1680}q:r xmlns:p\u{1680}q=\"http://p/\"/>";
        let doc = Document::parse(xml).unwrap();
        let mut nav = RoXmlNavigator::new(&doc);
        assert!(nav.move_to_first_child());
        assert_eq!(nav.name(), "p\u{1680}q:r");
        assert_eq!(nav.prefix(), "p\u{1680}q");
    }

    /// The four characters of XML 1.0 §2.3 production `S` all end the scan,
    /// and no other Unicode space character does.
    #[test]
    fn only_the_four_xml_whitespace_characters_end_a_name() {
        for ws in [' ', '\t', '\r', '\n'] {
            let xml = format!("<r{ws}a='1'/>");
            let doc = Document::parse(&xml).unwrap();
            let mut nav = RoXmlNavigator::new(&doc);
            assert!(nav.move_to_first_child());
            assert_eq!(nav.name(), "r", "{:?}", ws);
        }
        // Other Unicode whitespace inside the [#x37F-#x1FFF] NameChar range.
        for ws in ['\u{1680}', '\u{180E}'] {
            let xml = format!("<a{ws}b/>");
            let doc = Document::parse(&xml).unwrap();
            let mut nav = RoXmlNavigator::new(&doc);
            assert!(nav.move_to_first_child());
            assert_eq!(nav.name(), format!("a{ws}b"), "{:?}", ws);
        }
    }
}
