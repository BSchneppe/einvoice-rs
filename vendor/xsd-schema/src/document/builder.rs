//! Document builder — core push API and quick-xml adapter.
//!
//! [`BufferDocumentBuilder`] constructs a [`BufferDocument`] either through
//! its low-level push API (`start_element`, `attribute`, `text`, …) or via
//! the `build()` method which drives the push API from a quick-xml event stream.

use std::borrow::Cow;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::BufRead;

use bumpalo::Bump;
use quick_xml::events::Event;

use crate::ids::{NameId, SimpleTypeKey, TypeKey};
use crate::namespace::table::XML_NAMESPACE;
use crate::namespace::NameTable;
use crate::parser::frames::SimpleTypeVariety;
use crate::parser::location::SourceSpan;
use crate::schema::SchemaSet;
use crate::types::value::{XmlAtomicValue, XmlValue, XmlValueKind};
use crate::types::XmlTypeCode;
use crate::validation::info::ContentType;
use crate::xml::reader::{AttrKind, WfError, WfReader};
use crate::xml_entity::resolve_general_ref;

use super::buffer::{
    file_id_claim, id_index_key, next_document_serial, withdraw_id_claim, BufferDocument, IdClaim,
    IdSlot,
};
use super::error::BufferDocumentError;
use super::namespace::{prefix_bit, NamespaceScopes};
use super::{
    BindingRemapTable, BufferDocumentOptions, DocumentKind, ElementIndex, NamespaceNode,
    NamespacePageFactory, Node, NodePages, NodeSchemaBinding, NodeSourceSpans, NodeType, NsRef,
    QNameAtom, QNameTable, StringStore, NULL,
};

// ── ElementBuildState ─────────────────────────────────────────────────

/// Tracks per-element state during document construction.
#[derive(Clone, Copy, Debug)]
struct ElementBuildState {
    node_ref: u32,
    #[allow(dead_code)] // used by navigator in Step 7
    has_attrs: bool,
}

// ── ContentState ──────────────────────────────────────────────────────

/// What has already been appended to one open container.
///
/// The constructor content rules (XQuery 1.0 §3.7.1.3) need two facts about
/// the container that is currently being filled: whether it already has a
/// child (an attribute may not follow one) and whether the item appended last
/// was an atomic value (two adjacent atomic values are separated by a single
/// space). Both are per-container, so the builder keeps one of these for every
/// open element plus one for the document level.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ContentState {
    /// A text, element, comment, processing-instruction or atomic item has
    /// been appended to this container.
    pub(crate) children_started: bool,
    /// The item appended last was an atomic value, so the next atomic value
    /// needs a separating space.
    pub(crate) last_atomic: bool,
}

// ── hash_name ─────────────────────────────────────────────────────────

/// Compute a `u32` hash of a local name string (same hasher as `QNameTable`).
fn hash_name(name: &str) -> u32 {
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    hasher.finish() as u32
}

// ── BufferDocumentBuilder ─────────────────────────────────────────────

/// Builds a [`BufferDocument`] incrementally via a push API.
pub struct BufferDocumentBuilder<'a> {
    doc: BufferDocument<'a>,
    parent: u32,
    last_sibling: u32,
    last_attr: u32,
    namespace_stack: Vec<(u32, NsRef)>,
    text_buffer: String,
    text_type: Option<NodeType>,
    current_namespace: NsRef,
    element_stack: Vec<ElementBuildState>,
    /// One entry per open container, the document level first. Never empty.
    content_states: Vec<ContentState>,
    pending_spans: Vec<(u32, usize)>,
    #[allow(dead_code)]
    options: BufferDocumentOptions,
    /// The interned `xml:id` name, so that the check
    /// [`attribute`](Self::attribute) makes on every attribute is two integer
    /// comparisons rather than two string comparisons.
    xml_id_name: (NameId, NameId),
    /// An id value another element of the same tree already held when the
    /// last attribute was added; see
    /// [`dropped_duplicate_id`](Self::dropped_duplicate_id).
    dropped_duplicate_id: Option<Box<str>>,
    /// The is-id class of each binding index, filled on first use — see
    /// [`IdClass`]. Nearly every annotation an attribute carries is
    /// [`IdClass::Never`], and this makes that answer one vector read.
    id_classes: Vec<IdClass>,
    /// The is-idrefs class of each binding index, filled on first use — see
    /// [`IdrefsClass`].
    idrefs_classes: Vec<IdrefsClass>,
    /// How many element and attribute nodes carry a binding, so that
    /// `has_type_annotations` stays exact when a binding is taken away again.
    annotated_nodes: usize,
    /// Limits on the text [`build`](Self::build) reads.
    parse_limits: crate::xml::ParseLimits,
}

impl<'a> BufferDocumentBuilder<'a> {
    // ── Constructor ───────────────────────────────────────────────────

    /// Creates a new builder.
    ///
    /// If `schema_set` is `Some`, uses its `name_table`; otherwise uses `names`.
    pub fn new(
        arena: &'a Bump,
        names: &'a NameTable,
        schema_set: Option<&'a SchemaSet>,
        options: BufferDocumentOptions,
    ) -> Result<Self, BufferDocumentError> {
        let effective_names = schema_set
            .map(|ss| &ss.name_table as &'a NameTable)
            .unwrap_or(names);

        // Namespace pages + implicit xml: binding at slot 0
        let mut namespace_pages = NamespacePageFactory::new(arena);
        let xml_ns_ref = namespace_pages
            .alloc()
            .ok_or(BufferDocumentError::Overflow)?;
        let xml_prefix_id = effective_names.add("xml");
        let xml_uri_id = effective_names.add(XML_NAMESPACE);
        namespace_pages.set(
            xml_ns_ref,
            NamespaceNode::new(xml_prefix_id, xml_uri_id, NsRef::NULL),
        );

        // Node pages + root node at index 0
        let mut nodes = NodePages::new(arena);
        let root_ref = nodes.alloc()?;
        let mut root_node = Node::default();
        root_node.set_node_type(NodeType::Root);
        root_node.parent = NULL;
        root_node.next_sibling = NULL;
        nodes.set(root_ref, root_node);

        let mut ns_scopes = NamespaceScopes::default();
        ns_scopes.allocated(xml_ns_ref, NsRef::NULL);

        let doc = BufferDocument {
            arena,
            serial: next_document_serial(),
            kind: options.kind,
            names: effective_names,
            nodes,
            qname_table: QNameTable::new(),
            strings: StringStore::new(arena),
            binding_remap: BindingRemapTable::new(),
            has_type_annotations: false,
            root: root_ref,
            options,
            namespace_pages,
            xml_namespace: xml_ns_ref,
            element_namespaces: HashMap::new(),
            ns_scopes,
            element_index: ElementIndex::new(),
            source_spans: NodeSourceSpans::new(),
            id_elements: HashMap::new(),
            idrefs: HashMap::new(),
            schema_set,
            fragment_base_uri: None,
            xml_declaration: None,
        };

        Ok(Self {
            doc,
            parent: root_ref,
            last_sibling: NULL,
            last_attr: NULL,
            namespace_stack: Vec::new(),
            text_buffer: String::new(),
            text_type: None,
            current_namespace: NsRef::NULL,
            element_stack: Vec::new(),
            content_states: vec![ContentState::default()],
            pending_spans: Vec::new(),
            options,
            xml_id_name: (effective_names.add("id"), xml_uri_id),
            dropped_duplicate_id: None,
            id_classes: Vec::new(),
            idrefs_classes: Vec::new(),
            annotated_nodes: 0,
            parse_limits: crate::xml::ParseLimits::none(),
        })
    }

    /// This builder with limits on the XML text [`build`](Self::build)
    /// reads (see [`ParseLimits`](crate::xml::ParseLimits)): each is checked
    /// on the raw markup before anything of it is interned or built, and a
    /// document that breaks one is refused with
    /// [`BufferDocumentError::LimitExceeded`]. The default is no limits. The
    /// push API is not limited.
    ///
    /// A typed build reads through the validation driver, which takes its
    /// limits from the validation runtime instead
    /// ([`ValidationRuntime::set_parse_limits`](crate::validation::ValidationRuntime::set_parse_limits)).
    pub fn with_parse_limits(mut self, limits: crate::xml::ParseLimits) -> Self {
        self.parse_limits = limits;
        self
    }

    // ── Core push API ─────────────────────────────────────────────────

    /// Opens an element node.
    ///
    /// `ns_declarations` is a list of `(prefix, namespace_uri)` pairs for xmlns
    /// declarations on this element.
    pub fn start_element(
        &mut self,
        local_name: &str,
        ns_uri: &str,
        prefix: &str,
        ns_declarations: &[(&str, &str)],
    ) -> Result<u32, BufferDocumentError> {
        self.flush_text()?;
        self.note_child_appended();

        // Save previous namespace head
        let prev_namespace = self.current_namespace;
        // The namespace node this element's first declaration gets, if any,
        // and a filter of the prefixes it declares.
        let first_declaration = NsRef(self.doc.namespace_pages.len());
        let mut own_prefixes = 0u64;

        // Process namespace declarations
        for &(ns_prefix, ns_uri_decl) in ns_declarations {
            let prefix = self.handle_namespace_decl(ns_prefix, ns_uri_decl)?;
            own_prefixes |= prefix_bit(prefix);
        }

        // Intern names
        let local_id = self.doc.names.add(local_name);
        let uri_id = self.doc.names.add(ns_uri);
        let prefix_id = self.doc.names.add(prefix);
        let local_hash = hash_name(local_name);

        // Dedup the qname BEFORE materializing its qualified-name string. The
        // qualified name is fully determined by prefix+local_name, so a table hit
        // reuses the first occurrence's stored string. Storing unconditionally
        // (as before) left one orphaned `StringStore` entry per element
        // occurrence — the atom dedups, but the string does not.
        let probe = QNameAtom {
            local_name: local_id,
            namespace_uri: uri_id,
            prefix: prefix_id,
            local_name_hash: local_hash,
            qualified_name_idx: 0,
        };
        let qname_idx = match self.doc.qname_table.lookup(&probe) {
            Some(idx) => idx,
            None => {
                let qualified_name_idx = if prefix.is_empty() {
                    self.doc.strings.store(local_name)
                } else {
                    self.doc.strings.store(&format!("{prefix}:{local_name}"))
                };
                self.doc.qname_table.atomize(QNameAtom {
                    qualified_name_idx,
                    ..probe
                })
            }
        };

        // Allocate element node
        let elem_ref = self.doc.nodes.alloc()?;
        let mut elem_node = Node::default();
        elem_node.set_node_type(NodeType::Element);
        elem_node.parent = self.parent;
        elem_node.next_sibling = NULL;
        elem_node.value = qname_idx;
        self.doc.nodes.set(elem_ref, elem_node);

        // Link from last sibling
        if self.last_sibling != NULL {
            self.doc.nodes.update(self.last_sibling, |n| {
                n.next_sibling = elem_ref;
            });
        }

        // Set HAS_CHILDREN on parent
        self.doc
            .nodes
            .update(self.parent, |n| n.set_flag(Node::HAS_CHILDREN));

        // Element index (Full mode)
        if self.doc.kind == DocumentKind::Full {
            self.doc.element_index.add(local_hash, elem_ref);
        }

        // Namespace scope changed?
        if self.current_namespace != prev_namespace {
            self.namespace_stack.push((elem_ref, prev_namespace));
            self.doc.nodes.update(elem_ref, |n| {
                n.set_flag(Node::HAS_NMSP_DECLS);
            });
            self.doc
                .element_namespaces
                .insert(elem_ref, self.current_namespace);
            self.doc.ns_scopes.declared(
                &self.doc.namespace_pages,
                elem_ref,
                prev_namespace,
                self.current_namespace,
                Some((first_declaration, own_prefixes)),
            );
        }

        // Push element state, descend
        self.element_stack.push(ElementBuildState {
            node_ref: elem_ref,
            has_attrs: false,
        });
        self.content_states.push(ContentState::default());
        self.parent = elem_ref;
        self.last_sibling = NULL;
        self.last_attr = NULL;

        Ok(elem_ref)
    }

    /// Adds an attribute to the current element (two-node pair).
    ///
    /// This is the one place an attribute node is created, so it is also where
    /// an `xml:id` is filed in the document's id index — for a document parsed
    /// from text, for a typed one, for a copy and for a tree a host builds
    /// through this API alike. The cost is two integer comparisons per
    /// attribute (the name is interned either way). An attribute that is an id
    /// by its *type* rather than its name is filed when its annotation is
    /// attached, by [`set_node_binding`](Self::set_node_binding). Only an
    /// attribute of an element makes an id: one added while no element is
    /// open belongs to the document node and is not filed, because `fn:id`
    /// selects elements only.
    ///
    /// The builder does not look for an attribute of the same name already on
    /// the element: calling this twice with one name makes two attribute nodes.
    pub fn attribute(
        &mut self,
        local_name: &str,
        ns_uri: &str,
        prefix: &str,
        value: &str,
    ) -> Result<u32, BufferDocumentError> {
        let local_id = self.doc.names.add(local_name);
        let uri_id = self.doc.names.add(ns_uri);
        let prefix_id = self.doc.names.add(prefix);

        // The namespace is tested first: almost every attribute is in no
        // namespace, so this fails on the first comparison.
        let is_xml_id = uri_id == self.xml_id_name.1 && local_id == self.xml_id_name.0;

        // Same dedup-before-store as element starts: avoid an orphaned
        // `StringStore` entry per attribute occurrence (only the first sighting
        // of a unique qname materializes its qualified-name string).
        let probe = QNameAtom {
            local_name: local_id,
            namespace_uri: uri_id,
            prefix: prefix_id,
            local_name_hash: 0, // attrs not indexed
            qualified_name_idx: 0,
        };
        let qname_idx = match self.doc.qname_table.lookup(&probe) {
            Some(idx) => idx,
            None => {
                let qualified_name_idx = if prefix.is_empty() {
                    self.doc.strings.store(local_name)
                } else {
                    self.doc.strings.store(&format!("{prefix}:{local_name}"))
                };
                self.doc.qname_table.atomize(QNameAtom {
                    qualified_name_idx,
                    ..probe
                })
            }
        };

        // Attribute node
        let attr_ref = self.doc.nodes.alloc()?;
        let mut attr_node = Node::default();
        attr_node.set_node_type(NodeType::Attribute);
        attr_node.parent = self.parent;
        attr_node.next_sibling = NULL;
        attr_node.value = qname_idx;
        self.doc.nodes.set(attr_ref, attr_node);

        // ChildValue node
        let val_idx = self.doc.strings.store(value);
        let cv_ref = self.doc.nodes.alloc()?;
        let mut cv_node = Node::default();
        cv_node.set_node_type(NodeType::ChildValue);
        cv_node.parent = attr_ref; // parent is the Attribute node
        cv_node.next_sibling = NULL;
        cv_node.value = val_idx;
        self.doc.nodes.set(cv_ref, cv_node);

        // Chain attributes
        if self.last_attr != NULL {
            self.doc.nodes.update(self.last_attr, |n| {
                n.next_sibling = attr_ref;
            });
        }
        self.last_attr = attr_ref;

        // Set HAS_ATTRIBUTE on parent
        self.doc
            .nodes
            .update(self.parent, |n| n.set_flag(Node::HAS_ATTRIBUTE));

        // Mark element as having attrs
        if let Some(state) = self.element_stack.last_mut() {
            state.has_attrs = true;
        }

        if is_xml_id {
            if let (Some(owner), Some(key)) = (self.id_owner(attr_ref), id_index_key(value)) {
                let claim = IdClaim {
                    tree: self.tree_of(owner),
                    elem: owner,
                    attr: attr_ref,
                };
                if file_id_claim(&mut self.doc.id_elements, &key, claim) {
                    self.dropped_duplicate_id = Some(key.into());
                }
            }
        }

        Ok(attr_ref)
    }

    /// Marks the end of attributes; subsequent content nodes are children.
    pub fn end_of_attributes(&mut self) {
        self.last_sibling = NULL;
        self.last_attr = NULL;
    }

    /// Accumulates text content; coalesced on the next structural event.
    pub fn text(&mut self, value: &str) {
        self.text_buffer.push_str(value);
        if self.text_type.is_none() {
            self.text_type = Some(NodeType::Text);
        }
        self.note_child_appended();
    }

    /// Whether the open element has no content node yet, so that all its
    /// content so far is the text still pending (not yet a node).
    pub(crate) fn has_only_pending_text(&self) -> bool {
        self.last_sibling == NULL
    }

    /// The text accumulated since the last structural event, not yet a node.
    pub(crate) fn pending_text(&self) -> &str {
        if self.text_type.is_some() {
            &self.text_buffer
        } else {
            ""
        }
    }

    /// Replaces the pending text; an empty `text` leaves no text node.
    pub(crate) fn replace_pending_text(&mut self, text: &str) {
        self.text_buffer.clear();
        if text.is_empty() {
            self.text_type = None;
        } else {
            self.text_buffer.push_str(text);
            self.text_type.get_or_insert(NodeType::Text);
        }
    }

    /// Discards the pending text (it becomes no node), keeping the buffer.
    pub(crate) fn discard_pending_text(&mut self) {
        self.text_type = None;
        self.text_buffer.clear();
    }

    /// Takes the pending text away (it becomes no node).
    pub(crate) fn take_pending_text(&mut self) -> String {
        self.text_type = None;
        std::mem::take(&mut self.text_buffer)
    }

    /// A prefix bound to the namespace `uri` in the scope of the open element
    /// — `xml` for the XML namespace, otherwise the nearest declaration of a
    /// non-empty prefix to `uri` that no nearer declaration of the same prefix
    /// hides — or `None` when there is none.
    pub(crate) fn prefix_in_scope(&self, uri: &str) -> Option<&'a str> {
        if uri == XML_NAMESPACE {
            return Some("xml");
        }
        let names = self.doc.names;
        let uri_id = names.get(uri)?;
        let mut seen: Vec<NameId> = Vec::new();
        for (_, ns) in self.doc.scope_walk(self.current_namespace) {
            let prefix = names.resolve_ref(ns.prefix);
            if !prefix.is_empty() && !seen.contains(&ns.prefix) {
                if ns.namespace_uri == uri_id {
                    return Some(prefix);
                }
                seen.push(ns.prefix);
            }
        }
        None
    }

    /// The namespace the non-empty `prefix` is bound to in the scope of the
    /// open element (`xml` to the XML namespace), or `None` when it is bound
    /// nowhere (or undeclared).
    pub(crate) fn namespace_of_prefix(&self, prefix: &str) -> Option<&'a str> {
        if prefix == "xml" {
            return Some(XML_NAMESPACE);
        }
        let names = self.doc.names;
        for (_, ns) in self.doc.scope_walk(self.current_namespace) {
            if names.resolve_ref(ns.prefix) == prefix {
                let uri = names.resolve_ref(ns.namespace_uri);
                return (!uri.is_empty()).then_some(uri);
            }
        }
        None
    }

    /// The default namespace in the scope of the open element, or `None`
    /// when there is none (or it is undeclared).
    pub(crate) fn default_namespace_in_scope(&self) -> Option<&'a str> {
        let names = self.doc.names;
        for (_, ns) in self.doc.scope_walk(self.current_namespace) {
            if names.resolve_ref(ns.prefix).is_empty() {
                let uri = names.resolve_ref(ns.namespace_uri);
                return (!uri.is_empty()).then_some(uri);
            }
        }
        None
    }

    /// Whether `prefix` is bound (to anything, an undeclaration included) in
    /// the scope of the open element.
    pub(crate) fn prefix_is_declared(&self, prefix: &str) -> bool {
        if prefix == "xml" || prefix == "xmlns" {
            return true;
        }
        let Some(prefix_id) = self.doc.names.get(prefix) else {
            return false;
        };
        self.doc
            .scope_walk(self.current_namespace)
            .any(|(_, ns)| ns.prefix == prefix_id)
    }

    /// The value of the attribute node `attr_ref`.
    pub(crate) fn attribute_value(&self, attr_ref: u32) -> &str {
        debug_assert_eq!(
            self.doc.nodes.get(attr_ref).node_type(),
            NodeType::Attribute
        );
        self.doc.strings.get(self.doc.nodes.get(attr_ref + 1).value)
    }

    /// Adds a comment node.
    pub fn comment(&mut self, value: &str) -> Result<(), BufferDocumentError> {
        self.flush_text()?;
        self.note_child_appended();
        self.add_content_node(NodeType::Comment, value)?;
        Ok(())
    }

    /// Adds a processing instruction (two-node pair: PI + ChildValue).
    pub fn processing_instruction(
        &mut self,
        target: &str,
        data: &str,
    ) -> Result<(), BufferDocumentError> {
        self.flush_text()?;
        self.note_child_appended();

        let target_idx = self.doc.strings.store(target);
        let pi_ref = self.doc.nodes.alloc()?;
        let mut pi_node = Node::default();
        pi_node.set_node_type(NodeType::ProcessingInstruction);
        pi_node.parent = self.parent;
        pi_node.next_sibling = NULL;
        pi_node.value = target_idx;
        self.doc.nodes.set(pi_ref, pi_node);

        let data_idx = self.doc.strings.store(data);
        let cv_ref = self.doc.nodes.alloc()?;
        let mut cv_node = Node::default();
        cv_node.set_node_type(NodeType::ChildValue);
        cv_node.parent = pi_ref;
        cv_node.next_sibling = NULL;
        cv_node.value = data_idx;
        self.doc.nodes.set(cv_ref, cv_node);

        // Link sibling
        if self.last_sibling != NULL {
            self.doc.nodes.update(self.last_sibling, |n| {
                n.next_sibling = pi_ref;
            });
        }
        self.last_sibling = pi_ref;

        // Set HAS_CHILDREN on parent
        self.doc
            .nodes
            .update(self.parent, |n| n.set_flag(Node::HAS_CHILDREN));

        Ok(())
    }

    /// Closes the current element.
    ///
    /// This is where an element that is an id or has the is-idrefs property
    /// by its annotation is filed (its typed value depends on its content,
    /// complete only now) — see
    /// [`set_node_binding`](Self::set_node_binding) — and where an element a
    /// host marked with [`register_idrefs`](Self::register_idrefs) while it
    /// was open is filed in the idrefs index.
    pub fn end_element(&mut self) -> Result<(), BufferDocumentError> {
        self.flush_text()?;
        if let Some(state) = self.element_stack.last() {
            let elem = state.node_ref;
            self.file_element(elem);
        }

        let _state = self
            .element_stack
            .pop()
            .ok_or(BufferDocumentError::UnmatchedEndElement)?;
        // The document-level state is never popped, so the vector stays
        // non-empty for `content_state{,_mut}`.
        if self.content_states.len() > 1 {
            self.content_states.pop();
        }

        // If element has namespace declarations, restore previous scope
        let elem_node = self.doc.nodes.get(self.parent);
        if elem_node.has_flag(Node::HAS_NMSP_DECLS) {
            if let Some((_elem_ref, prev_ns)) = self.namespace_stack.pop() {
                self.current_namespace = prev_ns;
                // Whatever comes next is in the parent's scope again.
                self.doc.ns_scopes.set_scope(self.doc.nodes.len(), prev_ns);
            }
        }

        self.last_sibling = self.parent;
        self.parent = elem_node.parent;
        self.last_attr = NULL;

        Ok(())
    }

    /// Finalizes the document, appending the Nul sentinel.
    pub fn finalize(mut self) -> Result<BufferDocument<'a>, BufferDocumentError> {
        self.flush_text()?;

        // Allocate Nul sentinel
        let nul_ref = self.doc.nodes.alloc()?;
        let nul_node = Node::default(); // NodeType::Nul by default
        self.doc.nodes.set(nul_ref, nul_node);

        // No more strings will be added — return the index vector's
        // geometric-growth slack to the allocator.
        self.doc.strings.shrink_to_fit();

        Ok(self.doc)
    }

    /// Sets the schema binding on a node, returning `true` if the type is complex.
    ///
    /// This is the one place a [`NodeSchemaBinding`] is attached to a node, so
    /// it is also where [`BufferDocument::has_type_annotations`] is maintained:
    /// binding an element or an attribute is exactly what makes
    /// `DomNavigator::type_annotation()` answer `Some` for that node.
    ///
    /// For the same reason it is where an attribute that is an id by its
    /// *type* is filed in the id index. XDM 1.0 §6.3.4 makes an attribute
    /// is-id when its typed value is exactly one atomic value of type `xs:ID`
    /// or a type derived from it — which takes in a union whose value is of
    /// its `xs:ID` member and a list of `xs:ID` of length one, and leaves out a
    /// value the type rejects. The typed value is the one
    /// `DomNavigator::typed_value()` reports for the node, so `fn:id` and
    /// `fn:data` always agree, and a copy that keeps the annotation (a typed
    /// build, a copy with `Annotations::Preserve`, a host binding a node
    /// itself) is indexed exactly as its source. Binding a node again first
    /// withdraws what the previous binding filed. An `xml:id` is an id by its
    /// name whatever its type, and is filed by [`attribute`](Self::attribute).
    ///
    /// An *element* is an id by the same rule (XDM 1.0 §6.2.4: unless its
    /// content is element-only, "if the typed-value of the element consists of
    /// exactly one atomic value and that value is of type xs:ID, or a type
    /// derived from xs:ID, the is-id property is true") — a simple type or a
    /// complex type with simple content; a nilled element has no typed value.
    /// Its typed value depends on its content, so it is filed when the element
    /// ends ([`end_element`](Self::end_element)), or at once when a closed
    /// element is bound. `fn:id` then selects the element itself (F&O 1.0
    /// §15.5.2).
    ///
    /// The is-idrefs property (XDM 1.0 §6.2.4, §6.3.4: "if any of the atomic
    /// values in the typed-value … is of type xs:IDREF or xs:IDREFS, or a type
    /// derived from one of those types") is set here for an attribute and at
    /// the end for an element, from the same typed value, and its string
    /// value's tokens are filed for `fn:idref` (F&O 1.0 §15.5.3); see
    /// [`register_idrefs`](Self::register_idrefs) for a node that keeps the
    /// property without an annotation.
    ///
    /// Returns [`BufferDocumentError::Overflow`] if the binding table is full.
    pub fn set_node_binding(
        &mut self,
        node_ref: u32,
        binding: NodeSchemaBinding,
    ) -> Result<bool, BufferDocumentError> {
        let idx = self.doc.binding_remap.register(binding)?;
        let is_complex = matches!(binding.type_key, crate::ids::TypeKey::Complex(_));
        let node = self.doc.nodes.get(node_ref);
        let kind = node.node_type();
        let previous = node.binding_index();
        if kind == NodeType::Attribute && previous != 0 {
            self.withdraw_typed_id(node_ref, previous);
            self.withdraw_idrefs(node_ref);
        }
        let closed_element = kind == NodeType::Element && !self.is_open(node_ref);
        if closed_element {
            self.withdraw_element(node_ref, previous);
        }
        if matches!(kind, NodeType::Element | NodeType::Attribute) && previous == 0 {
            self.annotated_nodes += 1;
            self.doc.has_type_annotations = true;
        }
        self.doc.nodes.update(node_ref, |n| {
            n.set_binding_index(idx);
            if is_complex {
                n.set_flag(Node::IS_COMPLEX_TYPE);
            } else {
                n.clear_flag(Node::IS_COMPLEX_TYPE);
            }
        });
        if kind == NodeType::Attribute {
            self.file_typed_id(node_ref, idx);
            self.file_typed_idrefs(node_ref, idx);
        }
        if closed_element {
            self.file_element(node_ref);
        }
        Ok(is_complex)
    }

    /// Takes a node's schema binding away again, leaving it unannotated — and,
    /// for an attribute, withdrawing the id its annotation made it.
    ///
    /// Used where a node is replaced by one that carries no annotation: the
    /// later of two duplicate attributes of a constructed element wins with its
    /// value *and* its (absent) annotation.
    pub(crate) fn clear_node_binding(&mut self, node_ref: u32) {
        let node = self.doc.nodes.get(node_ref);
        let previous = node.binding_index();
        if previous == 0 {
            return;
        }
        let kind = node.node_type();
        if kind == NodeType::Attribute {
            self.withdraw_typed_id(node_ref, previous);
            self.withdraw_idrefs(node_ref);
        }
        if kind == NodeType::Element && !self.is_open(node_ref) {
            self.withdraw_element(node_ref, previous);
        }
        if matches!(kind, NodeType::Element | NodeType::Attribute) {
            self.annotated_nodes -= 1;
            self.doc.has_type_annotations = self.annotated_nodes > 0;
        }
        self.doc.nodes.update(node_ref, |n| {
            n.set_binding_index(0);
            n.clear_flag(Node::IS_COMPLEX_TYPE);
        });
    }

    /// Sets the `IS_NIL` flag on a node (xsi:nil="true").
    pub fn set_nil(&mut self, node_ref: u32) {
        self.doc.nodes.update(node_ref, |n| {
            n.set_flag(Node::IS_NIL);
        });
    }

    /// Registers an id value for the given element.
    ///
    /// [`attribute`](Self::attribute) already does this for an `xml:id`, and
    /// [`set_node_binding`](Self::set_node_binding) for an attribute or
    /// element whose typed value is an `xs:ID`, so this is for an id that
    /// comes from somewhere else — an untyped copy that keeps its source's
    /// ids, for example. It is **not** restricted to `Full` documents: the scope
    /// of an id is one tree, and the index is keyed by tree, so a `Fragment`
    /// buffer holding several top-level trees is served correctly. A value
    /// registered here stays registered: no attribute stands behind it, so
    /// replacing an attribute of the element never withdraws it.
    ///
    /// Which elements share a tree depends on the document kind: a
    /// [`DocumentKind::Full`] document is one tree (every top-level node sits
    /// under the one document node), while in a [`DocumentKind::Fragment`]
    /// buffer each child of the document node is the root of its own tree —
    /// see [`BufferDocument::get_element_by_id_in_tree`].
    ///
    /// The value is normalized as an index key (XML whitespace stripped and
    /// collapsed); a value that is not a lexical NCName is ignored, because
    /// such a node can never be selected. So is a node that is not an element:
    /// `fn:id` selects elements only.
    ///
    /// Returns [`BufferDocumentError::DuplicateId`] if another element of the
    /// **same tree** already answers to the normalized value. Registering the
    /// same element twice is accepted and changes nothing: a duplicate is a
    /// second *element* with the value.
    ///
    /// # Panics
    ///
    /// Panics if `elem_ref` is not a node of the document being built.
    pub fn register_xml_id(&mut self, id: &str, elem_ref: u32) -> Result<(), BufferDocumentError> {
        assert!(
            self.doc.has_node(elem_ref),
            "register_xml_id: {elem_ref} is not a node of this document"
        );
        if self.doc.nodes.get(elem_ref).node_type() != NodeType::Element {
            return Ok(());
        }
        let Some(key) = id_index_key(id) else {
            return Ok(());
        };
        let tree = self.doc.tree_root_of(elem_ref);
        let claim = IdClaim {
            tree,
            elem: elem_ref,
            attr: NULL,
        };
        match self.doc.id_elements.get_mut(key.as_ref()) {
            Some(slot) => match slot.in_tree(tree) {
                Some(first) if first != elem_ref => {
                    Err(BufferDocumentError::DuplicateId(key.into_owned()))
                }
                _ => {
                    // Filed as a claim of its own even when an attribute of the
                    // element already made it answer, so that replacing that
                    // attribute does not take this registration along.
                    if !slot.has_claim(tree, elem_ref, true) {
                        slot.add(claim);
                    }
                    Ok(())
                }
            },
            None => {
                self.doc.id_elements.insert(key.into(), IdSlot::One(claim));
                Ok(())
            }
        }
    }

    /// Gives the element or attribute node `node_ref` the is-idrefs property
    /// (XDM 1.0 §5.6), so that `fn:idref` finds it by the tokens of its
    /// string value (F&O 1.0 §15.5.3).
    ///
    /// A node whose annotation has an `xs:IDREF` / `xs:IDREFS` typed value
    /// gets the property from its binding
    /// ([`set_node_binding`](Self::set_node_binding)); this is for a node that
    /// keeps it without one — an untyped copy of such a node, as
    /// [`register_xml_id`](Self::register_xml_id) is for an id. An attribute is
    /// filed under its value as it stands (refiled when the builder replaces
    /// the value); an element when it ends, or at once if it has ended
    /// already. A node of another kind is ignored, and marking a node twice
    /// changes nothing.
    ///
    /// # Panics
    ///
    /// Panics if `node_ref` is not a node of the document being built.
    pub fn register_idrefs(&mut self, node_ref: u32) {
        assert!(
            self.doc.has_node(node_ref),
            "register_idrefs: {node_ref} is not a node of this document"
        );
        let node = self.doc.nodes.get(node_ref);
        if node.has_flag(Node::IS_IDREFS) {
            return;
        }
        match node.node_type() {
            NodeType::Attribute => self.mark_and_file_idrefs(node_ref),
            NodeType::Element if self.is_open(node_ref) => {
                self.doc
                    .nodes
                    .update(node_ref, |n| n.set_flag(Node::IS_IDREFS));
            }
            NodeType::Element => self.mark_and_file_idrefs(node_ref),
            _ => {}
        }
    }

    /// Takes the id value the last [`attribute`](Self::attribute) filed for a
    /// second element of a tree that already had an element with that id.
    ///
    /// F&O §15.5.2 selects "the first such element in document order" when
    /// several elements share an ID value, so such a claim is kept but not
    /// selected, and it is not an error for the index. This is how a driver
    /// that reads a real XML document learns about a duplicate `xml:id` without
    /// paying for a lookup of its own: the index knows, and only it should
    /// decide what a duplicate means. It answers `None` for a `Fragment`
    /// buffer, which has never reported duplicates.
    #[inline]
    pub(crate) fn dropped_duplicate_id(&mut self) -> Option<Box<str>> {
        self.dropped_duplicate_id.as_ref()?;
        let dropped = self.dropped_duplicate_id.take();
        dropped.filter(|_| self.doc.kind == DocumentKind::Full)
    }

    /// Returns `true` when source location tracking is enabled.
    #[inline]
    pub fn track_source_locations(&self) -> bool {
        self.options.track_source_locations
    }

    /// Records a completed source span for a node.
    pub fn set_source_span(&mut self, node_ref: u32, span: SourceSpan) {
        self.doc.source_spans.set(node_ref, span);
    }

    // ── Services for `super::copy` ────────────────────────────────────
    //
    // The copy helpers live in `super::copy` but need a few facts about the
    // half-built document that the push API alone does not expose. They are
    // `pub(crate)` on purpose: none of them is part of the builder's contract.

    /// The content state of the container currently being filled.
    #[inline]
    pub(crate) fn content_state(&self) -> ContentState {
        *self
            .content_states
            .last()
            .expect("the document-level state is never popped")
    }

    /// The content state of the container currently being filled, mutably.
    #[inline]
    pub(crate) fn content_state_mut(&mut self) -> &mut ContentState {
        self.content_states
            .last_mut()
            .expect("the document-level state is never popped")
    }

    /// Records that a child has been appended to the current container: an
    /// attribute may no longer follow, and an atomic run is broken.
    #[inline]
    fn note_child_appended(&mut self) {
        let state = self.content_state_mut();
        state.children_started = true;
        state.last_atomic = false;
    }

    /// Whether an element is open, i.e. whether content is being added to an
    /// element rather than at document level.
    #[inline]
    pub(crate) fn has_open_element(&self) -> bool {
        !self.element_stack.is_empty()
    }

    /// The schema set the document under construction is bound to, if any.
    #[inline]
    pub(crate) fn schema_set(&self) -> Option<&'a SchemaSet> {
        self.doc.schema_set
    }

    /// The `(prefix, namespace_uri)` bindings in scope at the insertion point,
    /// one entry per prefix (the innermost declaration wins).
    ///
    /// An entry whose URI is empty is an undeclared default namespace. The
    /// implicit `xml` binding is not included: it needs no declaration.
    pub(crate) fn in_scope_bindings(&self) -> Vec<(String, String)> {
        let mut bindings: Vec<(String, String)> = Vec::new();
        for (_, ns_node) in self.doc.scope_walk(self.current_namespace) {
            let prefix = self.doc.names.resolve_ref(ns_node.prefix);
            if !bindings.iter().any(|(p, _)| p == prefix) {
                bindings.push((
                    prefix.to_string(),
                    self.doc
                        .names
                        .resolve_ref(ns_node.namespace_uri)
                        .to_string(),
                ));
            }
        }
        bindings
    }

    /// Declares a namespace on the element that is currently open, after
    /// [`start_element`](Self::start_element) has returned.
    ///
    /// Only sound while the element is still in its attribute phase — no child
    /// has been added yet — because a child that was already created captured
    /// the chain head as it was then. Attributes are the one thing that can
    /// need a declaration after the start tag was opened (the builder takes an
    /// element's own declarations up front), and they can only be added before
    /// any child, so that restriction costs nothing.
    pub(crate) fn declare_namespace_on_open_element(
        &mut self,
        prefix: &str,
        uri: &str,
    ) -> Result<(), BufferDocumentError> {
        debug_assert!(self.has_open_element(), "no element is open");
        debug_assert!(
            !self.content_state().children_started,
            "a child was already added to this element"
        );
        let elem_ref = self.parent;
        let previously_declared = self.doc.nodes.get(elem_ref).has_flag(Node::HAS_NMSP_DECLS);
        let prev_namespace = self.current_namespace;
        let _ = self.handle_namespace_decl(prefix, uri)?;
        if !previously_declared {
            // First declaration on this element: it now owns a scope, which
            // `end_element` has to restore.
            self.namespace_stack.push((elem_ref, prev_namespace));
            self.doc.nodes.update(elem_ref, |n| {
                n.set_flag(Node::HAS_NMSP_DECLS);
            });
        }
        self.doc
            .element_namespaces
            .insert(elem_ref, self.current_namespace);
        // The chain in scope at the parent, which the element's own
        // declarations run into.
        let outer = match self.namespace_stack.last() {
            Some(&(owner, outer)) if owner == elem_ref => outer,
            _ => prev_namespace,
        };
        self.doc.ns_scopes.declared(
            &self.doc.namespace_pages,
            elem_ref,
            outer,
            self.current_namespace,
            None,
        );
        Ok(())
    }

    /// Points `last_attr` at the last attribute of the open element, so that a
    /// further [`attribute`](Self::attribute) chains onto it.
    ///
    /// [`end_of_attributes`](Self::end_of_attributes) clears that cursor; a
    /// host that appends an attribute item after it (perfectly legal as long as
    /// the element has no children yet) would otherwise orphan the new node.
    pub(crate) fn sync_last_attribute(&mut self) {
        if self.last_attr != NULL || !self.has_open_element() {
            return;
        }
        if !self
            .doc
            .nodes
            .get(self.parent)
            .has_flag(Node::HAS_ATTRIBUTE)
        {
            return;
        }
        let mut cursor = self.doc.first_child_of(self.parent);
        loop {
            let next = self.doc.nodes.get(cursor).next_sibling;
            if next == NULL || self.doc.nodes.get(next).node_type() != NodeType::Attribute {
                break;
            }
            cursor = next;
        }
        self.last_attr = cursor;
    }

    /// The open element's attribute with this expanded name, if it has one.
    pub(crate) fn find_attribute(&self, local_name: &str, ns_uri: &str) -> Option<u32> {
        if !self.has_open_element()
            || !self
                .doc
                .nodes
                .get(self.parent)
                .has_flag(Node::HAS_ATTRIBUTE)
        {
            return None;
        }
        let local_id = self.doc.names.add(local_name);
        let uri_id = self.doc.names.add(ns_uri);
        let mut cursor = self.doc.first_child_of(self.parent);
        while self.doc.nodes.get(cursor).node_type() == NodeType::Attribute {
            let atom = self.doc.qname_table.get(self.doc.nodes.get(cursor).value);
            if atom.local_name == local_id && atom.namespace_uri == uri_id {
                return Some(cursor);
            }
            let next = self.doc.nodes.get(cursor).next_sibling;
            if next == NULL {
                break;
            }
            cursor = next;
        }
        None
    }

    /// Replaces the value of an existing attribute node.
    ///
    /// The id index follows: whatever the old value made the attribute's
    /// element answer to — as an `xml:id`, or through the attribute's
    /// annotation — is withdrawn before the value changes, and the new value is
    /// filed under the same rules afterwards. The annotation itself is the
    /// caller's business ([`set_node_binding`](Self::set_node_binding) /
    /// [`clear_node_binding`](Self::clear_node_binding)).
    ///
    /// The previous value stays interned in the string store; duplicate
    /// attribute names are rare enough that reclaiming it is not worth a
    /// back-reference.
    pub(crate) fn set_attribute_value(&mut self, attr_ref: u32, value: &str) {
        debug_assert_eq!(
            self.doc.nodes.get(attr_ref).node_type(),
            NodeType::Attribute
        );
        let binding = self.doc.nodes.get(attr_ref).binding_index();
        let is_xml_id = self.is_xml_id_attribute(attr_ref);
        let was_idrefs = self.withdraw_idrefs(attr_ref);
        if is_xml_id {
            let doc = &mut self.doc;
            let old = doc.strings.get(doc.nodes.get(attr_ref + 1).value);
            if let Some(key) = id_index_key(old) {
                withdraw_id_claim(&mut doc.id_elements, &key, attr_ref);
            }
        } else if binding != 0 {
            self.withdraw_typed_id(attr_ref, binding);
        }

        let val_idx = self.doc.strings.store(value);
        self.doc.nodes.update(attr_ref + 1, |n| {
            n.value = val_idx;
        });

        if is_xml_id {
            if let (Some(owner), Some(key)) = (self.id_owner(attr_ref), id_index_key(value)) {
                let claim = IdClaim {
                    tree: self.tree_of(owner),
                    elem: owner,
                    attr: attr_ref,
                };
                file_id_claim(&mut self.doc.id_elements, &key, claim);
            }
        } else if binding != 0 {
            self.file_typed_id(attr_ref, binding);
        }
        if binding != 0 {
            self.file_typed_idrefs(attr_ref, binding);
        } else if was_idrefs {
            self.mark_and_file_idrefs(attr_ref);
        }
    }

    // ── The is-id rule for annotated attributes ──────────────────────

    /// Whether an attribute node is named `xml:id`, which is an id by its name
    /// whatever its type (XDM 1.0 §6.3.4).
    fn is_xml_id_attribute(&self, attr_ref: u32) -> bool {
        let atom = self.doc.qname_table.get(self.doc.nodes.get(attr_ref).value);
        atom.namespace_uri == self.xml_id_name.1 && atom.local_name == self.xml_id_name.0
    }

    /// The root of the tree `elem` belongs to (see
    /// [`BufferDocument::tree_root_of`]) — in a `Fragment` buffer, for the
    /// element that is open, the outermost open element, which costs no parent
    /// walk.
    #[inline]
    fn tree_of(&self, elem: u32) -> u32 {
        match self.element_stack.first() {
            Some(outermost) if elem == self.parent && self.doc.kind == DocumentKind::Fragment => {
                outermost.node_ref
            }
            _ => self.doc.tree_root_of(elem),
        }
    }

    /// The element the attribute `attr_ref` belongs to, or `None` when it was
    /// added while no element was open and so belongs to the document node.
    /// Only an element can answer to an id (`fn:id` returns `element()*`), so
    /// such an attribute files nothing.
    #[inline]
    fn id_owner(&self, attr_ref: u32) -> Option<u32> {
        let owner = self.doc.nodes.get(attr_ref).parent;
        (self.doc.nodes.get(owner).node_type() == NodeType::Element).then_some(owner)
    }

    /// The is-id class of the binding at `binding_idx`, computed once per
    /// binding and remembered.
    fn id_class(&mut self, binding_idx: u32) -> IdClass {
        let slot = binding_idx as usize;
        if let Some(&class) = self.id_classes.get(slot) {
            if class != IdClass::Unknown {
                return class;
            }
        }
        let class = match (self.doc.schema_set, self.doc.binding_remap.get(binding_idx)) {
            (Some(schema_set), Some(binding)) => IdClass::of(binding, schema_set),
            _ => IdClass::Never,
        };
        if self.id_classes.len() <= slot {
            self.id_classes.resize(slot + 1, IdClass::Unknown);
        }
        self.id_classes[slot] = class;
        class
    }

    /// The claim the attribute `attr_ref` makes when it is bound at
    /// `binding_idx`, and the binding's type — or `None` when that binding
    /// cannot make it an id, which for nearly every attribute is one vector
    /// read.
    fn typed_id_candidate(
        &mut self,
        attr_ref: u32,
        binding_idx: u32,
    ) -> Option<(IdClass, TypeKey, &'a SchemaSet, IdClaim)> {
        let class = self.id_class(binding_idx);
        if class == IdClass::Never || self.is_xml_id_attribute(attr_ref) {
            return None;
        }
        let type_key = self.doc.binding_remap.get(binding_idx)?.type_key;
        let schema_set = self.doc.schema_set?;
        let owner = self.id_owner(attr_ref)?;
        let claim = IdClaim {
            tree: self.tree_of(owner),
            elem: owner,
            attr: attr_ref,
        };
        Some((class, type_key, schema_set, claim))
    }

    /// Files the attribute `attr_ref`, bound at `binding_idx`, when that makes
    /// it an id. A duplicate is kept but not selected (see [`IdSlot`]), and is
    /// not reported: ID uniqueness in a validated document is `cvc-id`'s
    /// business, which the validator reports on its own channel.
    fn file_typed_id(&mut self, attr_ref: u32, binding_idx: u32) {
        let Some((class, type_key, schema_set, claim)) =
            self.typed_id_candidate(attr_ref, binding_idx)
        else {
            return;
        };
        let doc = &mut self.doc;
        let value = doc.strings.get(doc.nodes.get(attr_ref + 1).value);
        let scope = InScope::at(doc, attr_ref);
        let key = typed_id_key(class, value, type_key, schema_set, &scope);
        if let Some(key) = key {
            file_id_claim(&mut doc.id_elements, &key, claim);
        }
    }

    /// Withdraws what [`file_typed_id`](Self::file_typed_id) filed for the
    /// attribute while it was bound at `binding_idx`. Called before the value
    /// or the binding changes, so the key recomputes to the one it was filed
    /// under.
    fn withdraw_typed_id(&mut self, attr_ref: u32, binding_idx: u32) {
        let Some((class, type_key, schema_set, _)) = self.typed_id_candidate(attr_ref, binding_idx)
        else {
            return;
        };
        let doc = &mut self.doc;
        let value = doc.strings.get(doc.nodes.get(attr_ref + 1).value);
        let scope = InScope::at(doc, attr_ref);
        let key = typed_id_key(class, value, type_key, schema_set, &scope);
        if let Some(key) = key {
            withdraw_id_claim(&mut doc.id_elements, &key, attr_ref);
        }
    }

    // ── The is-id rule for annotated elements; is-idrefs ──────────────

    /// Whether `elem` is an element still open (being built).
    fn is_open(&self, elem: u32) -> bool {
        self.element_stack.iter().rev().any(|s| s.node_ref == elem)
    }

    /// The string value of the element `elem` as it stands — its text
    /// descendants, concatenated — and whether it has element children.
    fn element_content(&self, elem: u32) -> (Cow<'_, str>, bool) {
        let end = self.doc.subtree_end(elem);
        let mut text: Option<Cow<'_, str>> = None;
        let mut has_elements = false;
        for i in elem + 1..end {
            let node = self.doc.nodes.get(i);
            match node.node_type() {
                NodeType::Text | NodeType::Whitespace | NodeType::SignificantWhitespace => {
                    let s = self.doc.strings.get(node.value);
                    text = Some(match text {
                        None => Cow::Borrowed(s),
                        Some(t) => Cow::Owned(t.into_owned() + s),
                    });
                }
                NodeType::Element => has_elements = true,
                _ => {}
            }
        }
        (text.unwrap_or(Cow::Borrowed("")), has_elements)
    }

    /// The value the typed value of the bound element `elem` is computed
    /// from, exactly as `BufferDocNavigator::typed_value` takes it: its string
    /// value, or its declaration's default when it is empty and has no element
    /// children. `None` for a nilled element (no typed value) or a binding
    /// that is gone.
    fn element_typed_lexical(
        &self,
        elem: u32,
        binding_idx: u32,
    ) -> Option<(String, NodeSchemaBinding, &'a SchemaSet)> {
        if self.doc.nodes.get(elem).has_flag(Node::IS_NIL) {
            return None;
        }
        let binding = *self.doc.binding_remap.get(binding_idx)?;
        let schema_set = self.doc.schema_set?;
        let (value, has_elements) = self.element_content(elem);
        let value =
            super::navigator::element_value_or_default(value, has_elements, &binding, schema_set);
        Some((value.into_owned(), binding, schema_set))
    }

    /// The id index key the element `elem`, bound at `binding_idx`, answers
    /// to — XDM 1.0 §6.2.4: "If the element has a complex type with
    /// element-only content, the is-id property is false. Otherwise, if the
    /// typed-value of the element consists of exactly one atomic value and
    /// that value is of type xs:ID, or a type derived from xs:ID, the is-id
    /// property is true".
    fn element_id_key(&mut self, elem: u32, binding_idx: u32) -> Option<String> {
        let class = self.id_class(binding_idx);
        if class == IdClass::Never {
            return None;
        }
        let (value, binding, schema_set) = self.element_typed_lexical(elem, binding_idx)?;
        let scope = InScope::at(&self.doc, elem);
        typed_id_key(class, &value, binding.type_key, schema_set, &scope).map(Cow::into_owned)
    }

    /// Files the element `elem` as it stands: in the id index when its typed
    /// value is a single `xs:ID`, and in the idrefs index when it has the
    /// is-idrefs property — by its annotation, or because a host marked it.
    /// Called when the element ends, and when a closed element is bound
    /// again.
    fn file_element(&mut self, elem: u32) {
        let node = self.doc.nodes.get(elem);
        let binding_idx = node.binding_index();
        let marked = node.has_flag(Node::IS_IDREFS);
        if binding_idx == 0 && !marked {
            return;
        }
        if binding_idx != 0 {
            if let Some(key) = self.element_id_key(elem, binding_idx) {
                let claim = IdClaim {
                    tree: self.tree_of(elem),
                    elem,
                    attr: elem,
                };
                file_id_claim(&mut self.doc.id_elements, &key, claim);
            }
        }
        let idrefs = marked
            || (self.idrefs_class(binding_idx) == IdrefsClass::ByValue
                && self.element_typed_lexical(elem, binding_idx).is_some_and(
                    |(value, binding, schema_set)| {
                        let scope = InScope::at(&self.doc, elem);
                        typed_is_idrefs(&value, binding.type_key, schema_set, &scope)
                    },
                ));
        if idrefs {
            self.doc.nodes.update(elem, |n| n.set_flag(Node::IS_IDREFS));
            let (value, _) = self.element_content(elem);
            let value = value.into_owned();
            file_idrefs(&mut self.doc.idrefs, elem, &value);
        }
    }

    /// Withdraws what [`file_element`](Self::file_element) filed for the
    /// closed element `elem` while it was bound at `binding_idx`. Called
    /// before its binding changes. An unbound element filed nothing by an
    /// annotation (its is-idrefs flag, if any, a host's mark, stays).
    fn withdraw_element(&mut self, elem: u32, binding_idx: u32) {
        if binding_idx == 0 {
            return;
        }
        if let Some(key) = self.element_id_key(elem, binding_idx) {
            withdraw_id_claim(&mut self.doc.id_elements, &key, elem);
        }
        self.withdraw_idrefs(elem);
    }

    /// The is-idrefs class of the binding at `binding_idx`, computed once per
    /// binding and remembered.
    fn idrefs_class(&mut self, binding_idx: u32) -> IdrefsClass {
        if binding_idx == 0 {
            return IdrefsClass::Never;
        }
        let slot = binding_idx as usize;
        if let Some(&class) = self.idrefs_classes.get(slot) {
            if class != IdrefsClass::Unknown {
                return class;
            }
        }
        let class = match (self.doc.schema_set, self.doc.binding_remap.get(binding_idx)) {
            (Some(schema_set), Some(binding)) => IdrefsClass::of(binding, schema_set),
            _ => IdrefsClass::Never,
        };
        if self.idrefs_classes.len() <= slot {
            self.idrefs_classes.resize(slot + 1, IdrefsClass::Unknown);
        }
        self.idrefs_classes[slot] = class;
        class
    }

    /// Files the attribute `attr_ref`, bound at `binding_idx`, in the idrefs
    /// index when its typed value makes it is-idrefs — XDM 1.0 §6.3.4: "If any
    /// of the atomic values in the typed-value of the attribute is of type
    /// xs:IDREF or xs:IDREFS, or a type derived from one of those types, the
    /// is-idrefs property is true". The typed value is the one
    /// `BufferDocNavigator::typed_value` computes.
    fn file_typed_idrefs(&mut self, attr_ref: u32, binding_idx: u32) {
        if self.idrefs_class(binding_idx) == IdrefsClass::Never || self.id_owner(attr_ref).is_none()
        {
            return;
        }
        let (Some(schema_set), Some(binding)) = (
            self.doc.schema_set,
            self.doc.binding_remap.get(binding_idx).copied(),
        ) else {
            return;
        };
        let value = self.attribute_value(attr_ref);
        let scope = InScope::at(&self.doc, attr_ref);
        if typed_is_idrefs(value, binding.type_key, schema_set, &scope) {
            self.mark_and_file_idrefs(attr_ref);
        }
    }

    /// Gives `node` the is-idrefs flag and files the tokens of its string
    /// value as it stands.
    fn mark_and_file_idrefs(&mut self, node: u32) {
        self.doc.nodes.update(node, |n| n.set_flag(Node::IS_IDREFS));
        let value = match self.doc.nodes.get(node).node_type() {
            NodeType::Attribute => self.attribute_value(node).to_string(),
            _ => self.element_content(node).0.into_owned(),
        };
        file_idrefs(&mut self.doc.idrefs, node, &value);
    }

    /// Takes the is-idrefs flag of `node` away again and withdraws the
    /// entries it made, recomputed from its string value as it stands (it is
    /// called before that changes). Answers whether the node had the flag.
    fn withdraw_idrefs(&mut self, node: u32) -> bool {
        if !self.doc.nodes.get(node).has_flag(Node::IS_IDREFS) {
            return false;
        }
        let value = match self.doc.nodes.get(node).node_type() {
            NodeType::Attribute => self.attribute_value(node).to_string(),
            _ => self.element_content(node).0.into_owned(),
        };
        for token in idref_tokens(&value) {
            if let Some(nodes) = self.doc.idrefs.get_mut(token) {
                nodes.retain(|&n| n != node);
                if nodes.is_empty() {
                    self.doc.idrefs.remove(token);
                }
            }
        }
        self.doc
            .nodes
            .update(node, |n| n.clear_flag(Node::IS_IDREFS));
        true
    }

    // ── Internal helpers ──────────────────────────────────────────────

    /// Flushes accumulated text into a content node.
    fn flush_text(&mut self) -> Result<(), BufferDocumentError> {
        if let Some(nt) = self.text_type.take() {
            let value = std::mem::take(&mut self.text_buffer);
            if !value.is_empty() {
                self.add_content_node(nt, &value)?;
            }
        }
        Ok(())
    }

    /// Allocates a content node (Text, Comment, etc.) and links it.
    fn add_content_node(
        &mut self,
        node_type: NodeType,
        value: &str,
    ) -> Result<u32, BufferDocumentError> {
        let str_idx = self.doc.strings.store(value);
        let node_ref = self.doc.nodes.alloc()?;
        let mut node = Node::default();
        node.set_node_type(node_type);
        node.parent = self.parent;
        node.next_sibling = NULL;
        node.value = str_idx;
        self.doc.nodes.set(node_ref, node);

        if self.last_sibling != NULL {
            self.doc.nodes.update(self.last_sibling, |n| {
                n.next_sibling = node_ref;
            });
        }
        self.last_sibling = node_ref;

        self.doc
            .nodes
            .update(self.parent, |n| n.set_flag(Node::HAS_CHILDREN));

        Ok(node_ref)
    }

    /// Allocates a namespace node and chains it to `current_namespace`;
    /// returns the declared prefix.
    fn handle_namespace_decl(
        &mut self,
        prefix: &str,
        uri: &str,
    ) -> Result<NameId, BufferDocumentError> {
        let prefix_id = self.doc.names.add(prefix);
        let uri_id = self.doc.names.add(uri);

        let ns_ref = self
            .doc
            .namespace_pages
            .alloc()
            .ok_or(BufferDocumentError::Overflow)?;
        self.doc.namespace_pages.set(
            ns_ref,
            NamespaceNode::new(prefix_id, uri_id, self.current_namespace),
        );
        self.doc.ns_scopes.allocated(ns_ref, self.current_namespace);
        self.current_namespace = ns_ref;

        Ok(prefix_id)
    }

    // ── quick-xml adapter ─────────────────────────────────────────────

    /// Builds the document from XML text.
    ///
    /// The text is read through the crate's well-formedness layer
    /// ([`crate::xml`]), with the rules of the version its XML declaration
    /// names: a document that is not well-formed is refused with
    /// [`BufferDocumentError::NotWellFormed`] (input that ends inside an
    /// element included), and the limits set with
    /// [`with_parse_limits`](Self::with_parse_limits) are checked before
    /// anything of the markup that breaks one is interned
    /// ([`BufferDocumentError::LimitExceeded`]). The XML declaration is kept
    /// ([`BufferDocument::xml_declaration`]); a document type declaration in
    /// the prolog is checked and not kept.
    pub fn build<R: BufRead>(
        mut self,
        reader: R,
    ) -> Result<BufferDocument<'a>, BufferDocumentError> {
        let mut xml_reader = WfReader::new(reader, &self.parse_limits, false);

        // Transient prefix → URI mapping for namespace resolution
        let mut prefix_map: HashMap<Box<[u8]>, Vec<String>> = HashMap::new();
        prefix_map
            .entry(b"xml".to_vec().into_boxed_slice())
            .or_default()
            .push(XML_NAMESPACE.to_string());
        prefix_map
            .entry(b"".to_vec().into_boxed_slice())
            .or_default()
            .push(String::new());

        // Per-element declared prefixes for cleanup on close
        let mut scope_decls: Vec<Vec<Box<[u8]>>> = Vec::new();

        let track = self.options.track_source_locations;
        let mut buf = Vec::with_capacity(1024);

        loop {
            let event = xml_reader.read_event_into(&mut buf);
            let event_start = if track {
                xml_reader.event_start() as usize
            } else {
                0
            };

            match event {
                Ok(Event::Start(ref e)) => {
                    let elem_ref = self.handle_start_or_empty(
                        e,
                        &xml_reader,
                        false,
                        &mut prefix_map,
                        &mut scope_decls,
                    )?;
                    if track {
                        self.pending_spans.push((elem_ref, event_start));
                    }
                }
                Ok(Event::Empty(ref e)) => {
                    let elem_ref = self.handle_start_or_empty(
                        e,
                        &xml_reader,
                        true,
                        &mut prefix_map,
                        &mut scope_decls,
                    )?;
                    if track {
                        self.doc.source_spans.set(
                            elem_ref,
                            SourceSpan::new(event_start, xml_reader.buffer_position() as usize),
                        );
                    }
                }
                Ok(Event::End(_)) => {
                    if track {
                        if let Some((elem_ref, start)) = self.pending_spans.pop() {
                            self.doc.source_spans.set(
                                elem_ref,
                                SourceSpan::new(start, xml_reader.buffer_position() as usize),
                            );
                        }
                    }
                    // Pop namespace scope
                    if let Some(decls) = scope_decls.pop() {
                        for prefix_key in &decls {
                            if let Some(stack) = prefix_map.get_mut(prefix_key.as_ref()) {
                                stack.pop();
                            }
                        }
                    }
                    self.end_element()?;
                }
                // Outside the root element only white space reaches here
                // (the reader refuses other character data there).
                Ok(Event::Text(ref e)) => {
                    if !self.element_stack.is_empty() {
                        // XML 1.0 / 1.1 line-end normalization (§2.11).
                        let text = e
                            .xml_content(xml_reader.quick_xml_version())
                            .map_err(|_| document_error(xml_reader.recheck().utf8()))?;
                        self.text(&text);
                    }
                }
                // References are reported apart from the surrounding text; the
                // builder merges consecutive text anyway, so the run is rejoined.
                Ok(Event::GeneralRef(ref e)) => {
                    let text = resolve_general_ref(e)
                        .map_err(|e| document_error(xml_reader.recheck().decoding(e)))?;
                    self.text(&text);
                }
                Ok(Event::CData(ref e)) => {
                    let text = std::str::from_utf8(e)
                        .map_err(|_| document_error(xml_reader.recheck().utf8()))?;
                    self.text(text);
                }
                Ok(Event::Comment(ref e)) => {
                    let text = std::str::from_utf8(e)
                        .map_err(|_| document_error(xml_reader.recheck().utf8()))?;
                    self.comment(text)?;
                }
                Ok(Event::PI(ref e)) => {
                    let raw = std::str::from_utf8(e)
                        .map_err(|_| document_error(xml_reader.recheck().utf8()))?;
                    let (target, data) = parse_pi_content(raw);
                    self.processing_instruction(target, data)?;
                }
                Ok(Event::Decl(_)) => {
                    if let Some(declaration) = xml_reader.declaration() {
                        self.set_xml_declaration(declaration.clone());
                    }
                }
                // Not kept; its keyword is checked at the next read.
                Ok(Event::DocType(_)) => {}
                Ok(Event::Eof) => break,
                Err(e) => return Err(document_error(e)),
            }
        }

        self.finalize()
    }

    /// Records the XML declaration of the document being read.
    pub(crate) fn set_xml_declaration(&mut self, declaration: crate::xml::XmlDeclaration) {
        self.doc.xml_declaration = Some(declaration);
    }

    /// Handles `Event::Start` and `Event::Empty` elements; `spans` are the
    /// tag's attributes, as the reader found them while checking it.
    fn handle_start_or_empty<R: BufRead>(
        &mut self,
        e: &quick_xml::events::BytesStart<'_>,
        xml_reader: &WfReader<R>,
        is_empty: bool,
        prefix_map: &mut HashMap<Box<[u8]>, Vec<String>>,
        scope_decls: &mut Vec<Vec<Box<[u8]>>>,
    ) -> Result<u32, BufferDocumentError> {
        // The tag's attributes, as the reader found them while checking it.
        let spans = xml_reader.attributes();
        let version = xml_reader.quick_xml_version();
        // The reader checked the tag (UTF-8, bound prefixes, references):
        // `at` reports what still fails as the refusal it is (see `Recheck`).
        let at = xml_reader.recheck();
        let utf8 = |_| document_error(at.utf8());
        let tag: &[u8] = e;
        let mut local_decls: Vec<Box<[u8]>> = Vec::new();
        let mut ns_decls_str: Vec<(String, String)> = Vec::new();

        // First pass: collect xmlns declarations
        for span in spans {
            if span.kind == AttrKind::Attribute {
                continue;
            }
            let prefix_bytes = span.prefix(tag);
            let uri = span
                .value(tag, version)
                .map_err(|e| document_error(at.decoding(e)))?
                .into_owned();
            let prefix_key: Box<[u8]> = prefix_bytes.to_vec().into_boxed_slice();
            prefix_map
                .entry(prefix_key.clone())
                .or_default()
                .push(uri.clone());
            local_decls.push(prefix_key);
            let prefix_str = std::str::from_utf8(prefix_bytes).map_err(utf8)?;
            ns_decls_str.push((prefix_str.to_string(), uri));
        }

        scope_decls.push(local_decls);

        // Build ns_declarations slice for start_element
        let ns_decl_refs: Vec<(&str, &str)> = ns_decls_str
            .iter()
            .map(|(p, u)| (p.as_str(), u.as_str()))
            .collect();

        // Resolve element name
        let full_name = e.name();
        let full_name_bytes = full_name.as_ref();
        let (elem_prefix_bytes, elem_local_bytes) = split_prefix_local(full_name_bytes);

        let elem_local = std::str::from_utf8(elem_local_bytes).map_err(utf8)?;
        let elem_prefix_str = std::str::from_utf8(elem_prefix_bytes).map_err(utf8)?;

        // Resolve element namespace
        let elem_ns_uri = match prefix_map.get(elem_prefix_bytes) {
            // An empty binding of a prefix is an XML 1.1 undeclaration: not
            // bound (the reader refuses that before; this is a safeguard).
            Some(stack)
                if !stack.is_empty()
                    && (elem_prefix_bytes.is_empty() || !stack.last().unwrap().is_empty()) =>
            {
                stack.last().unwrap().as_str().to_string()
            }
            _ if elem_prefix_bytes.is_empty() => String::new(),
            _ => return Err(document_error(at.unbound(elem_prefix_bytes))),
        };

        let elem_ref =
            self.start_element(elem_local, &elem_ns_uri, elem_prefix_str, &ns_decl_refs)?;

        // Second pass: non-xmlns attributes
        for span in spans {
            if span.kind != AttrKind::Attribute {
                continue;
            }
            let attr_prefix_bytes = span.prefix(tag);
            let attr_local = std::str::from_utf8(span.local(tag)).map_err(utf8)?;
            let attr_prefix_str = std::str::from_utf8(attr_prefix_bytes).map_err(utf8)?;

            // Resolve attr namespace: unprefixed → empty, prefixed → lookup
            let attr_ns_uri = if attr_prefix_bytes.is_empty() {
                String::new()
            } else {
                match prefix_map.get(attr_prefix_bytes) {
                    Some(stack) if !stack.is_empty() && !stack.last().unwrap().is_empty() => {
                        stack.last().unwrap().as_str().to_string()
                    }
                    _ => return Err(document_error(at.unbound(attr_prefix_bytes))),
                }
            };

            let unescaped = span
                .value(tag, version)
                .map_err(|e| document_error(at.decoding(e)))?;
            self.attribute(attr_local, &attr_ns_uri, attr_prefix_str, &unescaped)?;

            // A duplicate `xml:id` is a defect of the *document*, and a driver
            // reading text is the one place a real document is read, so it is
            // reported here rather than in `attribute()` — which also has to
            // serve a host that constructs trees, where F&O §15.5.2 asks for
            // "the first such element in document order" instead of an error.
            if let Some(taken) = self.dropped_duplicate_id() {
                return Err(BufferDocumentError::DuplicateId(taken.into_string()));
            }
        }

        self.end_of_attributes();

        if is_empty {
            // Pop scope for empty element
            if let Some(decls) = scope_decls.pop() {
                for prefix_key in &decls {
                    if let Some(stack) = prefix_map.get_mut(prefix_key.as_ref()) {
                        stack.pop();
                    }
                }
            }
            self.end_element()?;
        }

        Ok(elem_ref)
    }
}

// ── The is-id rule ────────────────────────────────────────────────────

/// What an attribute's annotation can make of its is-id property (XDM 1.0
/// §6.3.4), decided once per binding.
///
/// The property belongs to the *typed value*: an attribute is-id when that is
/// exactly one atomic value of type `xs:ID` or a type derived from it. For
/// most annotations the answer does not depend on the value at all, and those
/// are the ones worth knowing up front.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IdClass {
    /// Not classified yet.
    Unknown,
    /// No value of this type is an `xs:ID`: no `xs:ID` is reachable through
    /// its derivation, its list item type or its union member types.
    Never,
    /// `xs:ID` itself, or a restriction of it with no facet beyond the
    /// whitespace one every such type has: a value is valid — and its typed
    /// value one `xs:ID` — exactly when it is a lexical NCName once
    /// whitespace-collapsed, which is what the index key already checks.
    PlainId,
    /// It depends on the value: a restriction of `xs:ID` with facets of its
    /// own (a value it rejects has no `xs:ID` typed value), a union with an
    /// `xs:ID` member (it depends which member the value is of), a list of
    /// `xs:ID` (it depends how many items there are).
    ByValue,
}

impl IdClass {
    fn of(binding: &NodeSchemaBinding, schema_set: &SchemaSet) -> Self {
        let Some(id) = schema_set.builtin_types().get_by_type_code(XmlTypeCode::Id) else {
            return IdClass::Never;
        };
        match binding.type_key {
            TypeKey::Simple(sk) => Self::of_simple(sk, id, schema_set, 0),
            // The navigator gives a complex annotation a typed value only for
            // text-only content, and only then could it be an `xs:ID`.
            TypeKey::Complex(_) if binding.content_type == Some(ContentType::TextOnly) => {
                IdClass::ByValue
            }
            TypeKey::Complex(_) => IdClass::Never,
        }
    }

    fn of_simple(sk: SimpleTypeKey, id: SimpleTypeKey, schema_set: &SchemaSet, depth: u32) -> Self {
        // Cycle guard, as the other type-graph walks have.
        if depth > 32 {
            return IdClass::Never;
        }
        let Some(data) = schema_set.arenas.simple_types.get(sk) else {
            return IdClass::Never;
        };
        let reaches_id = |member: &TypeKey| match member {
            TypeKey::Simple(member) => {
                Self::of_simple(*member, id, schema_set, depth + 1) != IdClass::Never
            }
            TypeKey::Complex(_) => false,
        };
        match data.variety {
            SimpleTypeVariety::Atomic if schema_set.derives_from(sk, id) => {
                let mut constraining = (*schema_set.effective_facets(sk)).clone();
                constraining.whitespace = None;
                if constraining.is_empty() {
                    IdClass::PlainId
                } else {
                    IdClass::ByValue
                }
            }
            SimpleTypeVariety::Atomic => IdClass::Never,
            SimpleTypeVariety::List if data.resolved_item_type.as_ref().is_some_and(reaches_id) => {
                IdClass::ByValue
            }
            SimpleTypeVariety::Union if data.resolved_member_types.iter().any(reaches_id) => {
                IdClass::ByValue
            }
            SimpleTypeVariety::List | SimpleTypeVariety::Union => IdClass::Never,
        }
    }
}

/// The index key of an attribute of type `type_key` whose value is `value`,
/// when it is an id — the `xs:ID` value itself, whitespace-collapsed as
/// `xs:ID` is.
///
/// [`IdClass::ByValue`] computes the typed value exactly as
/// `BufferDocNavigator::typed_value` does for an annotated attribute — the
/// same `validate_simple_type` call — so that the index and `fn:data` never
/// disagree, and a copy that keeps the annotation is indexed as its source.
fn typed_id_key<'v>(
    class: IdClass,
    value: &'v str,
    type_key: TypeKey,
    schema_set: &SchemaSet,
    scope: &InScope<'_, '_>,
) -> Option<Cow<'v, str>> {
    match class {
        IdClass::Never | IdClass::Unknown => None,
        IdClass::PlainId => id_index_key(value),
        IdClass::ByValue => {
            let resolver = |prefix: Option<&str>| scope.binding(prefix);
            let typed = crate::validation::simple::validate_simple_type_in(
                value,
                type_key,
                schema_set,
                Some(&resolver),
            )
            .ok()?
            .typed_value;
            let id = single_id(&typed)?;
            id_index_key(id).map(|key| Cow::Owned(key.into_owned()))
        }
    }
}

/// The `xs:ID` value a typed value consists of, when it is exactly one atomic
/// value of type `xs:ID` or a type derived from it.
///
/// Every type derived from `xs:ID` validates to the `xs:ID` type code, since
/// none of the built-in types derives from it. A union value is the value of
/// the member that matched; a list value is its items, and counts only when
/// there is exactly one.
fn single_id(value: &XmlValue) -> Option<&str> {
    match &value.value {
        XmlValueKind::Atomic(XmlAtomicValue::String(id)) if value.type_code == XmlTypeCode::Id => {
            Some(id)
        }
        XmlValueKind::Union(member) => single_id(member),
        XmlValueKind::List { items, .. } => match items.as_slice() {
            [member] => single_id(member),
            _ => None,
        },
        _ => None,
    }
}

/// Whether the values of an annotation can have the is-idrefs property — the
/// one test most annotations need, kept per binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IdrefsClass {
    /// Not classified yet.
    Unknown,
    /// No value of this type holds an `xs:IDREF`: none is reachable through
    /// its derivation, its list item type or its union member types.
    Never,
    /// It depends on the value (and on its validity): the typed value is
    /// computed.
    ByValue,
}

impl IdrefsClass {
    fn of(binding: &NodeSchemaBinding, schema_set: &SchemaSet) -> Self {
        let builtins = schema_set.builtin_types();
        let targets = [
            builtins.get_by_type_code(XmlTypeCode::IdRef),
            builtins.get_by_type_code(XmlTypeCode::IdRefs),
        ];
        let reaches = match binding.type_key {
            TypeKey::Simple(sk) => Self::reaches(sk, &targets, schema_set, 0),
            // The navigator gives a complex annotation a typed value only for
            // text-only content.
            TypeKey::Complex(_) => binding.content_type == Some(ContentType::TextOnly),
        };
        if reaches {
            IdrefsClass::ByValue
        } else {
            IdrefsClass::Never
        }
    }

    fn reaches(
        sk: SimpleTypeKey,
        targets: &[Option<SimpleTypeKey>; 2],
        schema_set: &SchemaSet,
        depth: u32,
    ) -> bool {
        if depth > 32 {
            return false;
        }
        if targets
            .iter()
            .flatten()
            .any(|&target| schema_set.derives_from(sk, target))
        {
            return true;
        }
        let Some(data) = schema_set.arenas.simple_types.get(sk) else {
            return false;
        };
        let reaches = |member: &TypeKey| match member {
            TypeKey::Simple(member) => Self::reaches(*member, targets, schema_set, depth + 1),
            TypeKey::Complex(_) => false,
        };
        match data.variety {
            SimpleTypeVariety::List => data.resolved_item_type.as_ref().is_some_and(reaches),
            SimpleTypeVariety::Union => data.resolved_member_types.iter().any(reaches),
            SimpleTypeVariety::Atomic => false,
        }
    }
}

/// Whether the typed value of `value` under `type_key` holds an atomic value
/// of type `xs:IDREF` or a type derived from it (the members of an
/// `xs:IDREFS` value are) — the same `validate_simple_type` call
/// [`typed_id_key`] makes, so that a value the type rejects has none.
fn typed_is_idrefs(
    value: &str,
    type_key: TypeKey,
    schema_set: &SchemaSet,
    scope: &InScope<'_, '_>,
) -> bool {
    fn holds(value: &XmlValue) -> bool {
        match &value.value {
            XmlValueKind::Atomic(_) => value.type_code == XmlTypeCode::IdRef,
            XmlValueKind::Union(member) => holds(member),
            XmlValueKind::List { items, .. } => items.iter().any(holds),
            XmlValueKind::UntypedAtomic(_) => false,
        }
    }
    let resolver = |prefix: Option<&str>| scope.binding(prefix);
    crate::validation::simple::validate_simple_type_in(value, type_key, schema_set, Some(&resolver))
        .is_ok_and(|result| holds(&result.typed_value))
}

/// The namespace bindings in scope at a node of the document being built, as
/// simple-type validation asks them (`validation::simple::PrefixResolver`):
/// the answers `BufferDocNavigator` gives for the same node, so that the id
/// and idrefs indexes classify a value by the union member its typed value
/// has. Datatypes §3.3.18: the lexical mapping of a QName "depends on the
/// namespace bindings in scope where the literal occurs" — read without them,
/// a QName member whose facets need the expanded name (an `enumeration` of
/// QNames) can fail where validation accepted it, and a later `xs:ID` /
/// `xs:IDREF` member win.
struct InScope<'d, 'a> {
    doc: &'d BufferDocument<'a>,
    /// The chain of the nearest element at or above the node that declares
    /// namespaces.
    head: NsRef,
}

impl<'d, 'a> InScope<'d, 'a> {
    /// The bindings in scope at `node` (an attribute's are its element's).
    fn at(doc: &'d BufferDocument<'a>, node: u32) -> Self {
        Self {
            doc,
            head: doc.namespace_scope(node),
        }
    }

    /// For a prefix, the namespace it is bound to (`None` when it is bound
    /// nowhere); for no prefix, the default namespace (`Some(None)` without
    /// one). The `xml` prefix is always bound.
    fn binding(&self, prefix: Option<&str>) -> Option<Option<NameId>> {
        let target = prefix.unwrap_or("");
        let names = self.doc.names;
        if target == "xml" {
            return Some(Some(names.add(XML_NAMESPACE)));
        }
        for (_, ns) in self.doc.scope_walk(self.head) {
            if names.resolve_ref(ns.prefix) == target {
                let bound = !names.resolve_ref(ns.namespace_uri).is_empty();
                return if target.is_empty() {
                    Some(bound.then_some(ns.namespace_uri))
                } else {
                    bound.then_some(Some(ns.namespace_uri))
                };
            }
        }
        target.is_empty().then_some(None)
    }
}

/// The distinct tokens of `value` that are NCNames, the IDREF values an
/// is-idrefs node answers to: F&O 1.0 §15.5.3 matches the candidate against
/// "fn:tokenize(fn:normalize-space(fn:string($N)), ' ')", and "ill-formed
/// IDREF values are ignored".
fn idref_tokens(value: &str) -> Vec<&str> {
    let mut tokens: Vec<&str> = Vec::new();
    for token in value
        .split([' ', '\t', '\n', '\r'])
        .filter(|t| crate::namespace::is_ncname(t))
    {
        if !tokens.contains(&token) {
            tokens.push(token);
        }
    }
    tokens
}

/// Files `node` under each of the IDREF tokens of `value`.
fn file_idrefs(index: &mut HashMap<Box<str>, Vec<u32>>, node: u32, value: &str) {
    for token in idref_tokens(value) {
        match index.get_mut(token) {
            Some(nodes) => {
                if !nodes.contains(&node) {
                    nodes.push(node);
                }
            }
            None => {
                index.insert(token.into(), vec![node]);
            }
        }
    }
}

// ── Free functions ────────────────────────────────────────────────────

/// Splits `b"prefix:local"` into `(b"prefix", b"local")`.
/// If no colon, returns `(b"", full_name)`.
pub(crate) fn split_prefix_local(name: &[u8]) -> (&[u8], &[u8]) {
    match name.iter().position(|&b| b == b':') {
        Some(pos) => (&name[..pos], &name[pos + 1..]),
        None => (b"", name),
    }
}

/// Parses PI content into `(target, data)`.
pub(crate) fn parse_pi_content(raw: &str) -> (&str, &str) {
    // `PI ::= '<?' PITarget (S (Char* - (Char* '?>')))? '?>'` (XML 1.0 §2.6):
    // only the `S` separating the target from the data is not data. Whatever
    // follows it — trailing whitespace included — is the data verbatim.
    let raw = raw.trim_start();
    match raw.find(|c: char| c.is_ascii_whitespace()) {
        Some(pos) => (&raw[..pos], raw[pos..].trim_start()),
        None => (raw, ""),
    }
}

/// The document error for an error of the well-formedness layer.
fn document_error(e: WfError) -> BufferDocumentError {
    match e {
        WfError::Io(e) => BufferDocumentError::Io(e),
        WfError::NotWellFormed(e) => BufferDocumentError::NotWellFormed(e),
        WfError::Limit(e) => BufferDocumentError::LimitExceeded(e),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::TypeKey;
    use crate::navigator::DomNavigator;

    #[test]
    fn parse_pi_content_keeps_the_data_verbatim() {
        // XML 1.0 §2.6: only the `S` between target and data is a separator;
        // everything after it, trailing whitespace included, is the data.
        assert_eq!(parse_pi_content("go now "), ("go", "now "));
        assert_eq!(parse_pi_content("go   now\t"), ("go", "now\t"));
        assert_eq!(parse_pi_content("go"), ("go", ""));
        assert_eq!(parse_pi_content("go "), ("go", ""));
        assert_eq!(parse_pi_content("go a?b "), ("go", "a?b "));
    }

    fn make_builder<'a>(arena: &'a Bump, names: &'a NameTable) -> BufferDocumentBuilder<'a> {
        BufferDocumentBuilder::new(arena, names, None, BufferDocumentOptions::default()).unwrap()
    }

    fn make_builder_full<'a>(arena: &'a Bump, names: &'a NameTable) -> BufferDocumentBuilder<'a> {
        BufferDocumentBuilder::new(arena, names, None, BufferDocumentOptions::full()).unwrap()
    }

    // ── Core push API tests ───────────────────────────────────────────

    #[test]
    fn test_empty_document() {
        let arena = Bump::new();
        let names = NameTable::new();
        let builder = make_builder(&arena, &names);
        let doc = builder.finalize().unwrap();

        // Root(0) + Nul(1)
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(doc.nodes.get(0).node_type(), NodeType::Root);
        assert_eq!(doc.nodes.get(1).node_type(), NodeType::Nul);
    }

    #[test]
    fn test_single_element() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = make_builder(&arena, &names);

        let elem = builder.start_element("root", "", "", &[]).unwrap();
        builder.end_of_attributes();
        builder.end_element().unwrap();

        let doc = builder.finalize().unwrap();

        let elem_node = doc.nodes.get(elem);
        assert_eq!(elem_node.node_type(), NodeType::Element);
        assert_eq!(elem_node.parent, 0); // Root
        assert!(doc.nodes.get(0).has_flag(Node::HAS_CHILDREN));
    }

    #[test]
    fn test_element_with_text() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = make_builder(&arena, &names);

        builder.start_element("root", "", "", &[]).unwrap();
        builder.end_of_attributes();
        builder.text("hello world");
        builder.end_element().unwrap();

        let doc = builder.finalize().unwrap();

        // Root(0), Element(1), Text(2), Nul(3)
        assert_eq!(doc.nodes.len(), 4);
        let text_node = doc.nodes.get(2);
        assert_eq!(text_node.node_type(), NodeType::Text);
        assert_eq!(doc.strings.get(text_node.value), "hello world");
    }

    #[test]
    fn test_text_coalescing() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = make_builder(&arena, &names);

        builder.start_element("root", "", "", &[]).unwrap();
        builder.end_of_attributes();
        builder.text("hello ");
        builder.text("world");
        builder.end_element().unwrap();

        let doc = builder.finalize().unwrap();

        // Root(0), Element(1), Text(2), Nul(3) — single coalesced text
        assert_eq!(doc.nodes.len(), 4);
        let text_node = doc.nodes.get(2);
        assert_eq!(text_node.node_type(), NodeType::Text);
        assert_eq!(doc.strings.get(text_node.value), "hello world");
    }

    #[test]
    fn test_element_with_attributes() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = make_builder(&arena, &names);

        builder.start_element("root", "", "", &[]).unwrap();
        let attr1 = builder.attribute("id", "", "", "123").unwrap();
        let attr2 = builder.attribute("name", "", "", "test").unwrap();
        builder.end_of_attributes();
        builder.end_element().unwrap();

        let doc = builder.finalize().unwrap();

        // Root(0), Element(1), Attr1(2), CV1(3), Attr2(4), CV2(5), Nul(6)
        assert_eq!(doc.nodes.len(), 7);

        let a1 = doc.nodes.get(attr1);
        assert_eq!(a1.node_type(), NodeType::Attribute);
        assert_eq!(a1.parent, 1); // Element
        assert_eq!(a1.next_sibling, attr2); // chained

        let cv1 = doc.nodes.get(attr1 + 1);
        assert_eq!(cv1.node_type(), NodeType::ChildValue);
        assert_eq!(cv1.parent, attr1); // parent is attr, not element
        assert_eq!(doc.strings.get(cv1.value), "123");

        let a2 = doc.nodes.get(attr2);
        assert_eq!(a2.node_type(), NodeType::Attribute);
        assert_eq!(a2.next_sibling, NULL);

        assert!(doc.nodes.get(1).has_flag(Node::HAS_ATTRIBUTE));
    }

    #[test]
    fn test_nested_elements() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = make_builder(&arena, &names);

        builder.start_element("a", "", "", &[]).unwrap();
        builder.end_of_attributes();

        let b = builder.start_element("b", "", "", &[]).unwrap();
        builder.end_of_attributes();
        builder.end_element().unwrap();

        builder.end_element().unwrap();

        let doc = builder.finalize().unwrap();

        let b_node = doc.nodes.get(b);
        assert_eq!(b_node.parent, 1); // "a" element
    }

    #[test]
    fn test_sibling_elements() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = make_builder(&arena, &names);

        builder.start_element("root", "", "", &[]).unwrap();
        builder.end_of_attributes();

        let a = builder.start_element("a", "", "", &[]).unwrap();
        builder.end_of_attributes();
        builder.end_element().unwrap();

        let b = builder.start_element("b", "", "", &[]).unwrap();
        builder.end_of_attributes();
        builder.end_element().unwrap();

        builder.end_element().unwrap();

        let doc = builder.finalize().unwrap();

        let a_node = doc.nodes.get(a);
        assert_eq!(a_node.next_sibling, b);

        let b_node = doc.nodes.get(b);
        assert_eq!(b_node.next_sibling, NULL);
    }

    #[test]
    fn test_comment_node() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = make_builder(&arena, &names);

        builder.start_element("root", "", "", &[]).unwrap();
        builder.end_of_attributes();
        builder.comment("a comment").unwrap();
        builder.end_element().unwrap();

        let doc = builder.finalize().unwrap();

        // Root(0), Element(1), Comment(2), Nul(3)
        let comment = doc.nodes.get(2);
        assert_eq!(comment.node_type(), NodeType::Comment);
        assert_eq!(doc.strings.get(comment.value), "a comment");
    }

    #[test]
    fn test_processing_instruction() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = make_builder(&arena, &names);

        builder.start_element("root", "", "", &[]).unwrap();
        builder.end_of_attributes();
        builder
            .processing_instruction("target", "data here")
            .unwrap();
        builder.end_element().unwrap();

        let doc = builder.finalize().unwrap();

        // Root(0), Element(1), PI(2), ChildValue(3), Nul(4)
        let pi = doc.nodes.get(2);
        assert_eq!(pi.node_type(), NodeType::ProcessingInstruction);
        assert_eq!(doc.strings.get(pi.value), "target");

        let cv = doc.nodes.get(3);
        assert_eq!(cv.node_type(), NodeType::ChildValue);
        assert_eq!(cv.parent, 2); // PI node
        assert_eq!(doc.strings.get(cv.value), "data here");
    }

    #[test]
    fn test_namespace_declarations() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = make_builder(&arena, &names);

        builder
            .start_element(
                "root",
                "http://example.com",
                "ex",
                &[("ex", "http://example.com")],
            )
            .unwrap();
        builder.end_of_attributes();
        builder.end_element().unwrap();

        let doc = builder.finalize().unwrap();

        let elem = doc.nodes.get(1);
        assert!(elem.has_flag(Node::HAS_NMSP_DECLS));
        assert!(doc.element_namespaces.contains_key(&1));
    }

    #[test]
    fn test_namespace_scope_restore() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = make_builder(&arena, &names);

        // Outer element declares ns
        builder
            .start_element(
                "outer",
                "http://outer.com",
                "o",
                &[("o", "http://outer.com")],
            )
            .unwrap();
        builder.end_of_attributes();

        // Inner element declares different ns with same prefix
        builder
            .start_element(
                "inner",
                "http://inner.com",
                "o",
                &[("o", "http://inner.com")],
            )
            .unwrap();
        builder.end_of_attributes();
        builder.end_element().unwrap();

        builder.end_element().unwrap();

        let doc = builder.finalize().unwrap();

        // Both elements should have HAS_NMSP_DECLS
        assert!(doc.nodes.get(1).has_flag(Node::HAS_NMSP_DECLS));
        // Inner element at index 2
        assert!(doc.nodes.get(2).has_flag(Node::HAS_NMSP_DECLS));
    }

    /// The scope table records where the chain in scope changes: at an
    /// element that declares, and right after it ends; nothing for a
    /// document that declares nothing.
    #[test]
    fn namespace_scope_changes_at_declaring_elements() {
        let arena = Bump::new();
        let names = NameTable::new();
        let plain = BufferDocument::from_reader_default(
            b"<a><b><c/></b><d/></a>".as_slice(),
            &arena,
            &names,
        )
        .unwrap();
        assert!(plain.ns_scopes.changes_for_test().is_empty());
        for node in 0..plain.nodes.len() {
            assert!(plain.namespace_scope(node).is_null());
        }

        // a=1, b=2, c=3 (declares), x=4 (attribute pair 4, 5), e=6, d=7.
        let doc = BufferDocument::from_reader_default(
            br#"<a><b><c xmlns:p="urn:p" x="1"><e/></c></b><d/></a>"#.as_slice(),
            &arena,
            &names,
        )
        .unwrap();
        let c_head = doc.element_namespaces[&3];
        assert_eq!(
            doc.ns_scopes.changes_for_test(),
            &[(3, c_head), (7, NsRef::NULL)],
            "the scope of c from c on, nothing again from d on"
        );
        let scopes: Vec<bool> = (0..8).map(|n| doc.namespace_scope(n) == c_head).collect();
        assert_eq!(
            scopes,
            [false, false, false, true, true, true, true, false],
            "c, its attribute (name and value node) and its child e are in c's scope"
        );
    }

    /// An element that declares again a prefix its ancestors declared makes
    /// the walk of its scope skip their declarations of it: the walk from the
    /// innermost element visits three nodes however deep the chain — its own
    /// default namespace, then the outermost element's two declarations (the
    /// skip passes over leading runs only; a reader drops the hidden default
    /// namespace, whose prefix it has met).
    #[test]
    fn scope_walk_skips_hidden_redeclarations() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut text = String::from(r#"<e xmlns="urn:d" xmlns:p="urn:p">"#);
        for _ in 0..100 {
            text.push_str(r#"<e xmlns="urn:d">"#);
        }
        for _ in 0..101 {
            text.push_str("</e>");
        }
        let doc = BufferDocument::from_reader_default(text.as_bytes(), &arena, &names).unwrap();
        let innermost = doc.element_namespaces[&101];
        let walked: Vec<(String, String)> = doc
            .scope_walk(innermost)
            .map(|(_, ns)| {
                (
                    names.resolve_ref(ns.prefix).to_string(),
                    names.resolve_ref(ns.namespace_uri).to_string(),
                )
            })
            .collect();
        assert_eq!(
            walked,
            [
                (String::new(), "urn:d".to_string()),
                ("p".to_string(), "urn:p".to_string()),
                (String::new(), "urn:d".to_string()),
            ]
        );
        // The chain itself still holds every declaration, for the local axis.
        assert_eq!(doc.namespace_pages.iter_chain(innermost).count(), 102);
    }

    #[test]
    fn test_element_index_full_mode() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = make_builder_full(&arena, &names);

        let elem = builder.start_element("item", "", "", &[]).unwrap();
        builder.end_of_attributes();
        builder.end_element().unwrap();

        let doc = builder.finalize().unwrap();

        let h = hash_name("item");
        let found = doc.element_index.find(h);
        assert_eq!(found, &[elem]);
    }

    #[test]
    fn test_set_node_binding() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = make_builder(&arena, &names);

        let elem = builder.start_element("root", "", "", &[]).unwrap();
        builder.end_of_attributes();

        // Create a complex type key
        use slotmap::SlotMap;
        let mut sm: SlotMap<crate::ids::ComplexTypeKey, ()> = SlotMap::with_key();
        let ck = sm.insert(());

        let binding = NodeSchemaBinding {
            type_key: TypeKey::Complex(ck),
            element_decl: None,
            attribute_decl: None,
            content_type: None,
        };

        let is_complex = builder.set_node_binding(elem, binding).unwrap();
        assert!(is_complex);

        builder.end_element().unwrap();
        let doc = builder.finalize().unwrap();

        let node = doc.nodes.get(elem);
        assert!(node.has_flag(Node::IS_COMPLEX_TYPE));
        assert!(node.binding_index() > 0);
    }

    #[test]
    fn test_set_nil() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = make_builder(&arena, &names);

        let elem = builder.start_element("root", "", "", &[]).unwrap();
        builder.end_of_attributes();

        builder.set_nil(elem);

        builder.end_element().unwrap();
        let doc = builder.finalize().unwrap();

        let node = doc.nodes.get(elem);
        assert!(node.has_flag(Node::IS_NIL));
    }

    // ── quick-xml adapter tests ───────────────────────────────────────

    fn build_from_str(xml: &str) -> BufferDocument<'_> {
        let arena = Bump::new();
        let names = NameTable::new();
        // We need to leak arena/names for the lifetime to work in tests.
        // Use Box::leak for test convenience.
        let arena = Box::leak(Box::new(arena));
        let names = Box::leak(Box::new(names));
        let builder =
            BufferDocumentBuilder::new(arena, names, None, BufferDocumentOptions::default())
                .unwrap();
        builder.build(xml.as_bytes()).unwrap()
    }

    fn build_from_str_full(xml: &str) -> BufferDocument<'_> {
        let arena = Box::leak(Box::new(Bump::new()));
        let names = Box::leak(Box::new(NameTable::new()));
        let builder =
            BufferDocumentBuilder::new(arena, names, None, BufferDocumentOptions::full()).unwrap();
        builder.build(xml.as_bytes()).unwrap()
    }

    #[test]
    fn test_build_simple() {
        let doc = build_from_str("<root/>");
        // Root(0), Element(1), Nul(2)
        assert_eq!(doc.nodes.len(), 3);
        assert_eq!(doc.nodes.get(1).node_type(), NodeType::Element);
    }

    #[test]
    fn test_build_nested() {
        let doc = build_from_str("<a><b>text</b></a>");
        // Root(0), a(1), b(2), Text(3), Nul(4)
        assert_eq!(doc.nodes.len(), 5);
        assert_eq!(doc.nodes.get(2).parent, 1); // b's parent is a
        let text = doc.nodes.get(3);
        assert_eq!(text.node_type(), NodeType::Text);
        assert_eq!(doc.strings.get(text.value), "text");
    }

    #[test]
    fn test_build_text_with_general_references() {
        // References arrive as their own events; the builder must still produce
        // a single text node holding the whole run.
        let doc = build_from_str("<a>x &amp; y&#33;</a>");
        // Root(0), a(1), Text(2), Nul(3)
        assert_eq!(doc.nodes.len(), 4);
        let text = doc.nodes.get(2);
        assert_eq!(text.node_type(), NodeType::Text);
        assert_eq!(doc.strings.get(text.value), "x & y!");
    }

    #[test]
    fn test_build_rejects_unknown_entity() {
        let arena = Box::leak(Box::new(Bump::new()));
        let names = Box::leak(Box::new(NameTable::new()));
        let builder =
            BufferDocumentBuilder::new(arena, names, None, BufferDocumentOptions::default())
                .unwrap();
        let result = builder.build("<a>&nope;</a>".as_bytes());
        assert!(matches!(result, Err(BufferDocumentError::NotWellFormed(e))
            if e.rule() == crate::xml::WellFormednessRule::EntityDeclared));
    }

    #[test]
    fn test_build_attributes() {
        let doc = build_from_str(r#"<root attr="val"/>"#);
        // Root(0), Element(1), Attr(2), CV(3), Nul(4)
        assert_eq!(doc.nodes.len(), 5);
        assert!(doc.nodes.get(1).has_flag(Node::HAS_ATTRIBUTE));
        let cv = doc.nodes.get(3);
        assert_eq!(doc.strings.get(cv.value), "val");
    }

    #[test]
    fn test_build_namespace_prefixed() {
        let doc = build_from_str(r#"<ns:root xmlns:ns="http://example.com"/>"#);
        let elem = doc.nodes.get(1);
        assert_eq!(elem.node_type(), NodeType::Element);
        assert!(elem.has_flag(Node::HAS_NMSP_DECLS));

        let qname = doc.qname_table.get(elem.value);
        assert_eq!(doc.names.resolve(qname.local_name), "root");
        assert_eq!(doc.names.resolve(qname.namespace_uri), "http://example.com");
        assert_eq!(doc.names.resolve(qname.prefix), "ns");
    }

    #[test]
    fn test_build_default_namespace() {
        let doc = build_from_str(r#"<root xmlns="http://default.com"><child/></root>"#);
        // child inherits default namespace
        let child = doc.nodes.get(2);
        let child_qname = doc.qname_table.get(child.value);
        assert_eq!(
            doc.names.resolve(child_qname.namespace_uri),
            "http://default.com"
        );
    }

    #[test]
    fn test_build_namespace_override() {
        let doc = build_from_str(
            r#"<root xmlns="http://outer.com"><child xmlns="http://inner.com"/></root>"#,
        );
        let root = doc.nodes.get(1);
        let root_qname = doc.qname_table.get(root.value);
        assert_eq!(
            doc.names.resolve(root_qname.namespace_uri),
            "http://outer.com"
        );

        let child = doc.nodes.get(2);
        let child_qname = doc.qname_table.get(child.value);
        assert_eq!(
            doc.names.resolve(child_qname.namespace_uri),
            "http://inner.com"
        );
    }

    #[test]
    fn test_build_cdata_coalescing() {
        let doc = build_from_str("<root>hello <![CDATA[world]]></root>");
        // Text should be coalesced: "hello world"
        let text = doc.nodes.get(2);
        assert_eq!(text.node_type(), NodeType::Text);
        assert_eq!(doc.strings.get(text.value), "hello world");
    }

    #[test]
    fn test_build_comment() {
        let doc = build_from_str("<root><!-- a comment --></root>");
        let comment = doc.nodes.get(2);
        assert_eq!(comment.node_type(), NodeType::Comment);
        assert_eq!(doc.strings.get(comment.value), " a comment ");
    }

    #[test]
    fn test_build_pi() {
        let doc = build_from_str("<root><?target data?></root>");
        let pi = doc.nodes.get(2);
        assert_eq!(pi.node_type(), NodeType::ProcessingInstruction);
        assert_eq!(doc.strings.get(pi.value), "target");

        let cv = doc.nodes.get(3);
        assert_eq!(cv.node_type(), NodeType::ChildValue);
        assert_eq!(doc.strings.get(cv.value), "data");
    }

    #[test]
    fn test_build_mixed_content() {
        let doc = build_from_str("<root>text<!-- comment --><child/>more</root>");
        // Root(0), root(1), Text(2), Comment(3), child(4), Text(5), Nul(6)
        assert_eq!(doc.nodes.get(2).node_type(), NodeType::Text);
        assert_eq!(doc.strings.get(doc.nodes.get(2).value), "text");
        assert_eq!(doc.nodes.get(3).node_type(), NodeType::Comment);
        assert_eq!(doc.nodes.get(4).node_type(), NodeType::Element);
        assert_eq!(doc.nodes.get(5).node_type(), NodeType::Text);
        assert_eq!(doc.strings.get(doc.nodes.get(5).value), "more");
    }

    #[test]
    fn test_build_source_spans() {
        let doc = build_from_str_full("<root><child/></root>");
        // Elements should have spans
        assert!(doc.source_spans.get(1).is_some()); // root
        assert!(doc.source_spans.get(2).is_some()); // child (empty)
    }

    #[test]
    fn test_build_no_source_spans_when_disabled() {
        let doc = build_from_str("<root><child/></root>");
        assert!(doc.source_spans.is_empty());
    }

    #[test]
    fn test_build_xml_id() {
        let doc = build_from_str_full(r#"<root xml:id="myid"/>"#);
        assert_eq!(doc.get_element_by_id("myid"), Some(1));
    }

    #[test]
    fn test_build_xml_id_duplicate_error() {
        let arena = Box::leak(Box::new(Bump::new()));
        let names = Box::leak(Box::new(NameTable::new()));
        let builder =
            BufferDocumentBuilder::new(arena, names, None, BufferDocumentOptions::full()).unwrap();
        let result = builder.build(r#"<root><a xml:id="dup"/><b xml:id="dup"/></root>"#.as_bytes());
        assert!(matches!(result, Err(BufferDocumentError::DuplicateId(_))));
    }

    #[test]
    fn test_build_unbound_prefix_error() {
        let arena = Box::leak(Box::new(Bump::new()));
        let names = Box::leak(Box::new(NameTable::new()));
        let builder =
            BufferDocumentBuilder::new(arena, names, None, BufferDocumentOptions::default())
                .unwrap();
        let result = builder.build(r#"<ns:root/>"#.as_bytes());
        assert!(matches!(result, Err(BufferDocumentError::NotWellFormed(e))
            if e.rule() == crate::xml::WellFormednessRule::PrefixDeclared));
    }

    #[test]
    fn test_build_nul_sentinel() {
        let doc = build_from_str("<root/>");
        let last = doc.nodes.len() - 1;
        assert_eq!(doc.nodes.get(last).node_type(), NodeType::Nul);
    }

    #[test]
    fn test_build_document_level_whitespace_ignored() {
        // Whitespace/text outside the document element is discarded per XPath data model,
        // but comments and PIs at document level are preserved.
        let doc = build_from_str("<!-- prolog -->\n<root/>\n<!-- epilog -->");
        // Root(0), Comment(1), Element(2), Comment(3), Nul(4)
        // The \n between constructs must NOT produce Text nodes.
        assert_eq!(doc.nodes.len(), 5);
        assert_eq!(doc.nodes.get(1).node_type(), NodeType::Comment);
        assert_eq!(doc.nodes.get(2).node_type(), NodeType::Element);
        assert_eq!(doc.nodes.get(3).node_type(), NodeType::Comment);
        assert_eq!(doc.nodes.get(4).node_type(), NodeType::Nul);
    }

    // ── Fragment mode helpers ────────────────────────────────────────────

    fn make_builder_fragment<'a>(
        arena: &'a Bump,
        names: &'a NameTable,
    ) -> BufferDocumentBuilder<'a> {
        BufferDocumentBuilder::new(arena, names, None, BufferDocumentOptions::fragment()).unwrap()
    }

    // ── Fragment mode tests ──────────────────────────────────────────────

    #[test]
    fn fragment_build_navigate() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = make_builder_fragment(&arena, &names);

        let elem = builder.start_element("item", "", "", &[]).unwrap();
        builder.end_of_attributes();
        builder.text("value");
        builder.end_element().unwrap();

        let doc = builder.finalize().unwrap();

        // Navigate: Root → element → text
        let mut nav = doc.create_navigator();
        assert!(nav.move_to_first_child()); // element
        assert_eq!(nav.current_ref(), elem);
        assert!(nav.move_to_first_child()); // text child
    }

    #[test]
    fn fragment_root_is_synthetic() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = make_builder_fragment(&arena, &names);

        builder.start_element("item", "", "", &[]).unwrap();
        builder.end_of_attributes();
        builder.end_element().unwrap();

        let doc = builder.finalize().unwrap();

        // Root node is synthetic
        let root = doc.nodes.get(0);
        assert_eq!(root.node_type(), NodeType::Root);

        // move_to_parent from Root returns false (boundary)
        let mut nav = doc.create_navigator(); // at root
        assert!(!nav.move_to_parent());
    }

    #[test]
    fn fragment_navigation_boundary() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = make_builder_fragment(&arena, &names);

        builder.start_element("item", "", "", &[]).unwrap();
        builder.end_of_attributes();
        builder.end_element().unwrap();

        let doc = builder.finalize().unwrap();

        let mut nav = doc.create_navigator();
        assert!(nav.move_to_first_child()); // element
        assert!(nav.move_to_parent()); // back to Root
        assert!(!nav.move_to_parent()); // boundary — Root has parent=NULL
    }

    #[test]
    fn fragment_skips_element_index() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = make_builder_fragment(&arena, &names);

        builder.start_element("item", "", "", &[]).unwrap();
        builder.end_of_attributes();
        builder.end_element().unwrap();

        let doc = builder.finalize().unwrap();

        let h = hash_name("item");
        assert!(
            doc.element_index.find(h).is_empty(),
            "Fragment mode should not populate element_index"
        );
    }

    /// A fragment document registers ids too.
    ///
    /// This replaces an earlier test that pinned the opposite ("Fragment mode
    /// `register_xml_id` should be no-op"). The scope of an id is one *tree*,
    /// not one buffer, and the index is keyed by tree, so a fragment buffer —
    /// which may hold several top-level trees — no longer has to opt out.
    #[test]
    fn fragment_register_xml_id_is_recorded() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = make_builder_fragment(&arena, &names);

        let elem = builder.start_element("item", "", "", &[]).unwrap();
        builder.end_of_attributes();
        builder.register_xml_id("myid", elem).unwrap();
        builder.end_element().unwrap();

        let doc = builder.finalize().unwrap();
        assert_eq!(
            doc.get_element_by_id("myid"),
            Some(elem),
            "a fragment document holds its ids"
        );
    }

    #[test]
    fn fragment_namespace_inheritance() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = make_builder_fragment(&arena, &names);

        builder
            .start_element(
                "outer",
                "http://example.com",
                "ex",
                &[("ex", "http://example.com")],
            )
            .unwrap();
        builder.end_of_attributes();

        // Child should inherit the namespace
        let child = builder
            .start_element("inner", "http://example.com", "ex", &[])
            .unwrap();
        builder.end_of_attributes();
        builder.end_element().unwrap();

        builder.end_element().unwrap();

        let doc = builder.finalize().unwrap();

        let child_qname = doc.qname_table.get(doc.nodes.get(child).value);
        assert_eq!(
            doc.names.resolve(child_qname.namespace_uri),
            "http://example.com",
            "child should inherit parent namespace in fragment mode"
        );
    }

    #[test]
    fn fragment_push_api_parity() {
        // Build same structure in Full and Fragment mode — node types should match
        let arena_full = Bump::new();
        let names_full = NameTable::new();
        let mut b_full = make_builder(&arena_full, &names_full);

        let arena_frag = Bump::new();
        let names_frag = NameTable::new();
        let mut b_frag = make_builder_fragment(&arena_frag, &names_frag);

        for b in [&mut b_full as &mut BufferDocumentBuilder, &mut b_frag] {
            b.start_element("root", "", "", &[]).unwrap();
            b.attribute("id", "", "", "1").unwrap();
            b.end_of_attributes();
            b.text("hello");
            b.end_element().unwrap();
        }

        let doc_full = b_full.finalize().unwrap();
        let doc_frag = b_frag.finalize().unwrap();

        assert_eq!(doc_full.nodes.len(), doc_frag.nodes.len());
        for i in 0..doc_full.nodes.len() {
            assert_eq!(
                doc_full.nodes.get(i).node_type(),
                doc_frag.nodes.get(i).node_type(),
                "node type mismatch at index {i}"
            );
        }
    }
    // ── DocumentKind::Full with several document-element children ─────

    /// `DocumentKind::Full` does not restrict the root to a single element
    /// child: the builder accepts two document elements, the navigator sees
    /// both as children of the root, and XPath counts both.
    ///
    /// This is what a temporary tree holding a sequence of elements needs;
    /// the XDM root node's `children` property is a sequence, and nothing in
    /// the builder's full-document bookkeeping (element index, `xml:id`
    /// registration) assumes it has length one.
    #[test]
    fn full_document_accepts_two_document_elements() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = make_builder_full(&arena, &names);
        assert_eq!(builder.doc.kind(), DocumentKind::Full);

        builder.start_element("a", "", "", &[]).unwrap();
        builder.end_of_attributes();
        builder.text("first");
        builder.end_element().unwrap();

        builder.start_element("b", "", "", &[]).unwrap();
        builder.end_of_attributes();
        builder.text("second");
        builder.end_element().unwrap();

        let doc = builder.finalize().unwrap();
        assert_eq!(doc.kind(), DocumentKind::Full);

        // The navigator sees both as children of the root.
        let mut nav = doc.create_navigator();
        assert_eq!(nav.node_type(), crate::navigator::DomNodeType::Root);
        assert!(nav.move_to_first_child());
        assert_eq!(nav.local_name(), "a");
        assert_eq!(nav.value(), "first");
        assert!(nav.move_to_next_sibling());
        assert_eq!(nav.local_name(), "b");
        assert_eq!(nav.value(), "second");
        assert!(!nav.move_to_next_sibling());
        assert!(nav.move_to_parent());
        assert_eq!(nav.node_type(), crate::navigator::DomNodeType::Root);

        // And so does XPath, with the root as the context node.
        let ctx = crate::xpath::context::XPathContext::new(&names);
        let expr = crate::xpath::XPathExpr::compile("count(/*)", &ctx).unwrap();
        let result = expr
            .evaluator(&ctx)
            .run_with_node(doc.create_navigator())
            .unwrap();
        assert_eq!(result.as_f64(), Some(2.0));

        let expr = crate::xpath::XPathExpr::compile("string-join(/*/name(), '|')", &ctx).unwrap();
        let result = expr
            .evaluator(&ctx)
            .run_with_node(doc.create_navigator())
            .unwrap();
        assert_eq!(result.to_owned_string().as_deref(), Some("a|b"));
    }

    // ── xml:id registration ───────────────────────────────────────────

    /// The key an `xml:id` is filed under is the value with XML whitespace
    /// stripped at both ends and collapsed inside — the normalization of a
    /// tokenized attribute type.
    #[test]
    fn an_xml_id_is_filed_under_a_whitespace_normalized_key() {
        let doc = build_from_str_full(
            r#"<doc><div xml:id="id3 "><title>Expressions</title></div></doc>"#,
        );
        // Root(0), doc(1), div(2)
        assert_eq!(doc.get_element_by_id("id3"), Some(2));
    }

    /// Only the *key* is normalized: the attribute keeps its own value.
    #[test]
    fn a_normalized_key_does_not_change_the_attribute_value() {
        let doc = build_from_str_full(r#"<doc xml:id=" id3 "/>"#);
        let mut nav = doc.create_navigator();
        assert!(nav.move_to_first_child());
        assert!(nav.move_to_first_attribute());
        assert_eq!(nav.value(), " id3 ", "the visible value is untouched");
    }

    /// F&O §15.5.2: "such a node will never be selected", so a value that is
    /// not a lexical NCName is not indexed at all.
    #[test]
    fn an_xml_id_that_is_not_an_ncname_is_never_registered() {
        let doc = build_from_str_full(r#"<doc xml:id="1abc"/>"#);
        assert_eq!(doc.get_element_by_id("1abc"), None);
    }

    /// A tree built through the push API — what a host that constructs trees
    /// uses — registers its ids like a parsed one.
    #[test]
    fn the_push_api_registers_an_xml_id() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = make_builder(&arena, &names);

        let code = builder.start_element("code", "", "", &[]).unwrap();
        builder.attribute("id", XML_NAMESPACE, "xml", "d").unwrap();
        builder.end_of_attributes();
        builder.text("Damson");
        builder.end_element().unwrap();

        let doc = builder.finalize().unwrap();
        assert_eq!(doc.get_element_by_id("d"), Some(code));
    }

    /// The same through a `Fragment` buffer.
    #[test]
    fn the_push_api_registers_an_xml_id_in_a_fragment() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = make_builder_fragment(&arena, &names);

        let code = builder.start_element("code", "", "", &[]).unwrap();
        builder.attribute("id", XML_NAMESPACE, "xml", "d").unwrap();
        builder.end_of_attributes();
        builder.end_element().unwrap();

        let doc = builder.finalize().unwrap();
        assert_eq!(doc.get_element_by_id("d"), Some(code));
    }

    /// F&O §15.5.2: a well-formed but invalid tree may hold two elements with
    /// the same id; the first in document order is the one selected, and it is
    /// not an error. A host that constructs trees must not be refused one.
    #[test]
    fn a_duplicate_id_in_a_built_tree_keeps_the_first() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = make_builder(&arena, &names);

        builder.start_element("r", "", "", &[]).unwrap();
        builder.end_of_attributes();
        let first = builder.start_element("a", "", "", &[]).unwrap();
        builder
            .attribute("id", XML_NAMESPACE, "xml", "dup")
            .unwrap();
        builder.end_of_attributes();
        builder.end_element().unwrap();
        builder.start_element("b", "", "", &[]).unwrap();
        builder
            .attribute("id", XML_NAMESPACE, "xml", "dup")
            .unwrap();
        builder.end_of_attributes();
        builder.end_element().unwrap();
        builder.end_element().unwrap();

        let doc = builder.finalize().unwrap();
        assert_eq!(doc.get_element_by_id("dup"), Some(first));
    }

    /// The scope of an id is one tree. A buffer may hold several top-level
    /// trees, and the tree-scoped lookup keeps them apart; the unscoped one
    /// answers with whichever comes first in document order.
    #[test]
    fn two_trees_of_one_buffer_do_not_share_ids() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = make_builder_fragment(&arena, &names);

        let tree = |builder: &mut BufferDocumentBuilder<'_>, name: &str, id: &str| {
            let elem = builder.start_element(name, "", "", &[]).unwrap();
            builder.attribute("id", XML_NAMESPACE, "xml", id).unwrap();
            builder.end_of_attributes();
            builder.end_element().unwrap();
            elem
        };
        let a = tree(&mut builder, "a", "x");
        let b = tree(&mut builder, "b", "y");
        // A value that occurs in both trees.
        let c = tree(&mut builder, "c", "shared");
        let d = tree(&mut builder, "d", "shared");
        let doc = builder.finalize().unwrap();

        assert_eq!(doc.get_element_by_id_in_tree(a, "x"), Some(a));
        assert_eq!(doc.get_element_by_id_in_tree(a, "y"), None);
        assert_eq!(doc.get_element_by_id_in_tree(b, "y"), Some(b));
        assert_eq!(doc.get_element_by_id_in_tree(b, "x"), None);

        assert_eq!(doc.get_element_by_id_in_tree(c, "shared"), Some(c));
        assert_eq!(doc.get_element_by_id_in_tree(d, "shared"), Some(d));
        assert_eq!(
            doc.get_element_by_id("shared"),
            Some(c),
            "unscoped: first in document order"
        );
    }

    /// `register_xml_id` still refuses a second element of the same tree, and
    /// accepts a repeat of the same element.
    #[test]
    fn register_xml_id_reports_a_duplicate_within_one_tree() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = make_builder(&arena, &names);

        builder.start_element("r", "", "", &[]).unwrap();
        builder.end_of_attributes();
        let a = builder.start_element("a", "", "", &[]).unwrap();
        builder.end_of_attributes();
        builder.end_element().unwrap();
        let b = builder.start_element("b", "", "", &[]).unwrap();
        builder.end_of_attributes();
        builder.end_element().unwrap();
        builder.end_element().unwrap();

        builder.register_xml_id("k", a).unwrap();
        builder.register_xml_id("k", a).expect("idempotent");
        assert!(matches!(
            builder.register_xml_id("k", b),
            Err(BufferDocumentError::DuplicateId(_))
        ));
        // A value that is not a lexical NCName is ignored, not an error.
        builder.register_xml_id("1abc", b).unwrap();

        let doc = builder.finalize().unwrap();
        assert_eq!(doc.get_element_by_id("k"), Some(a));
        assert_eq!(doc.get_element_by_id("1abc"), None);
    }

    // ── Which nodes make up one tree ──────────────────────────────────

    /// In a `Full` document the whole document is one tree: the navigator
    /// presents every top-level node under the one document node. A second
    /// top-level element registering an id the first already has is a
    /// duplicate, and a lookup anchored on either answers with the first in
    /// document order.
    #[test]
    fn a_full_document_with_two_top_level_elements_is_one_tree() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = make_builder(&arena, &names);
        let top = |builder: &mut BufferDocumentBuilder<'_>, name: &str, id: &str| {
            let elem = builder.start_element(name, "", "", &[]).unwrap();
            builder.attribute("id", XML_NAMESPACE, "xml", id).unwrap();
            builder.end_of_attributes();
            builder.end_element().unwrap();
            elem
        };
        let a = top(&mut builder, "a", "shared");
        let c = top(&mut builder, "c", "shared");
        builder.start_element("e", "", "", &[]).unwrap();
        builder.end_of_attributes();
        let e = builder.parent;
        builder.end_element().unwrap();

        builder.register_xml_id("k", a).unwrap();
        assert!(
            matches!(
                builder.register_xml_id("k", e),
                Err(BufferDocumentError::DuplicateId(_))
            ),
            "one document, one tree"
        );

        let doc = builder.finalize().unwrap();
        assert_eq!(doc.get_element_by_id_in_tree(c, "shared"), Some(a));
        assert_eq!(doc.get_element_by_id_in_tree(a, "shared"), Some(a));
        assert_eq!(doc.get_element_by_id_in_tree(e, "k"), Some(a));
    }

    /// Text parsing reports a duplicate across two top-level elements of a
    /// `Full` document as well.
    #[test]
    fn a_full_document_reports_a_duplicate_across_top_level_elements() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = make_builder(&arena, &names);
        builder.start_element("a", "", "", &[]).unwrap();
        builder.attribute("id", XML_NAMESPACE, "xml", "d").unwrap();
        assert_eq!(builder.dropped_duplicate_id(), None);
        builder.end_of_attributes();
        builder.end_element().unwrap();
        builder.start_element("b", "", "", &[]).unwrap();
        builder.attribute("id", XML_NAMESPACE, "xml", "d").unwrap();
        assert_eq!(builder.dropped_duplicate_id().as_deref(), Some("d"));
    }

    /// A top-level comment or processing instruction of a `Full` document is
    /// in the document's one tree, so a lookup anchored on it finds the
    /// document element's ids. In a `Fragment` buffer each top-level node is
    /// a tree of its own, and a comment's tree holds no element.
    #[test]
    fn a_top_level_comment_is_in_the_tree_of_a_full_document() {
        let arena = Bump::new();
        let names = NameTable::new();
        for (options, expected) in [
            (BufferDocumentOptions::default(), true),
            (BufferDocumentOptions::fragment(), false),
        ] {
            let mut builder = BufferDocumentBuilder::new(&arena, &names, None, options).unwrap();
            builder.comment("c").unwrap();
            let r = builder.start_element("r", "", "", &[]).unwrap();
            builder.end_of_attributes();
            let a = builder.start_element("a", "", "", &[]).unwrap();
            builder.attribute("id", XML_NAMESPACE, "xml", "x").unwrap();
            builder.end_of_attributes();
            builder.end_element().unwrap();
            builder.end_element().unwrap();
            let doc = builder.finalize().unwrap();
            let comment = doc.first_content_child_of(doc.root()).unwrap();
            assert_eq!(doc.nodes.get(comment).node_type(), NodeType::Comment);
            assert_ne!(comment, r);
            assert_eq!(
                doc.get_element_by_id_in_tree(comment, "x"),
                expected.then_some(a),
                "{:?}",
                options.kind
            );
            assert_eq!(doc.get_element_by_id_in_tree(doc.root(), "x"), Some(a));
        }
    }

    /// Only an attribute owned by an element can make an id: an `xml:id`
    /// added while no element is open belongs to the document node, and
    /// `fn:id` returns elements only.
    #[test]
    fn an_xml_id_with_no_open_element_is_not_filed() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = make_builder(&arena, &names);
        builder.attribute("id", XML_NAMESPACE, "xml", "zz").unwrap();
        builder.start_element("r", "", "", &[]).unwrap();
        builder.end_of_attributes();
        builder.end_element().unwrap();
        let doc = builder.finalize().unwrap();
        assert_eq!(doc.get_element_by_id("zz"), None);
        assert_eq!(doc.get_element_by_id_in_tree(doc.root(), "zz"), None);

        let ctx = crate::xpath::context::XPathContext::new(&names);
        let result = crate::xpath::XPathExpr::compile("count(id('zz'))", &ctx)
            .unwrap()
            .evaluator(&ctx)
            .run_with_node(doc.create_navigator())
            .unwrap();
        assert_eq!(result.as_f64(), Some(0.0));
    }

    /// A lookup anchored on a reference that is not a node of the document
    /// answers `None` rather than panicking — also for a reference past the
    /// end that still falls inside the last allocated page.
    #[test]
    fn get_element_by_id_in_tree_with_a_bad_node_reference_is_none() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = make_builder(&arena, &names);
        builder.start_element("r", "", "", &[]).unwrap();
        builder.attribute("id", XML_NAMESPACE, "xml", "q").unwrap();
        builder.end_of_attributes();
        builder.end_element().unwrap();
        let doc = builder.finalize().unwrap();
        assert_eq!(doc.get_element_by_id_in_tree(1_000_000, "q"), None);
        assert_eq!(doc.get_element_by_id_in_tree(doc.nodes.len(), "q"), None);
        assert_eq!(doc.get_element_by_id_in_tree(NULL, "q"), None);
        assert_eq!(doc.get_element_by_id_in_tree(doc.root(), "q"), Some(1));
    }

    /// `register_xml_id` with a reference that is not a node of the document
    /// being built is a caller error and panics, as documented.
    #[test]
    #[should_panic(expected = "not a node of this document")]
    fn register_xml_id_panics_on_a_node_reference_out_of_range() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = make_builder(&arena, &names);
        builder.start_element("r", "", "", &[]).unwrap();
        builder.end_of_attributes();
        builder.end_element().unwrap();
        let _ = builder.register_xml_id("q", 1_000_000);
    }

    /// `register_xml_id` on a node that is not an element files nothing:
    /// only an element has an id.
    #[test]
    fn register_xml_id_on_a_non_element_is_ignored() {
        let arena = Bump::new();
        let names = NameTable::new();
        let mut builder = make_builder(&arena, &names);
        builder.start_element("r", "", "", &[]).unwrap();
        builder.end_of_attributes();
        builder.text("t");
        builder.end_element().unwrap();
        builder.register_xml_id("doc", 0).unwrap();
        let text = 2;
        assert_eq!(builder.doc.nodes.get(text).node_type(), NodeType::Text);
        builder.register_xml_id("txt", text).unwrap();
        let doc = builder.finalize().unwrap();
        assert_eq!(doc.get_element_by_id("doc"), None);
        assert_eq!(doc.get_element_by_id("txt"), None);
    }
}

#[cfg(test)]
#[path = "id_index_tests.rs"]
mod id_index_tests;
