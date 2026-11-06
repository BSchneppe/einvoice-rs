//! Schema-aware document builder.
//!
//! A typed build reads an XML document once, validating it and building a
//! [`BufferDocument`] in the same pass, so that each element and attribute
//! node carries a [`NodeSchemaBinding`] (type key, declaration keys, content
//! type) from the validation:
//!
//! * [`build_typed_document_with`] — the composable build: the caller chooses
//!   the [`SchemaValidator`] (for example with XSD 1.1 assertions), supplies
//!   the [`ValidationSink`] that receives the diagnostics, and can run its own
//!   [`ValidationEventHandler`] alongside the builder in the same pass.
//! * [`TypedBuilderHandler`] — the building block underneath: the event
//!   handler that mirrors validator events into a [`BufferDocumentBuilder`],
//!   composed with an inner handler, for callers that drive
//!   [`drive_quick_xml_with`] themselves.
//! * [`build_typed_document`] — the turn-key build: default flags, no
//!   diagnostics ([`SilentValidationSink`]), no handler.
//!
//! The tree is the XDM instance of the PSVI (XDM 1.0 §3.3, §6): a valid node of
//! a simple type or with simple content has its [schema normalized value] as
//! its string value; defaulted attributes are attribute nodes and an empty
//! element that takes its declaration's default has a text node with it
//! ([`TypedBuilderHandler::with_defaults_as_nodes`]); white space in
//! element-only content is no text node (§6.7.4). What a validation run
//! reports about each node reaches the inner handler unchanged.

use std::io::BufRead;

use bumpalo::Bump;

use crate::parser::location::SourceSpan;
use crate::schema::SchemaSet;
use crate::validation::errors::ValidationError;
use crate::validation::info::{SchemaInfo, ValidationAttempted, ValidationFlags};
use crate::validation::quick_xml_driver::{
    drive_quick_xml_with, AttributeView, DoctypeView, DriveOutcome, DriveWithError,
    ElementStartView, EndElementInfo, EndOfAttributesView, NoopHandler, TextKind,
    ValidationEventHandler,
};
use crate::validation::runtime::ValidationRuntime;
use crate::validation::validator::{ValidationSink, ValidationWarning};
use crate::validation::SchemaValidator;

use super::buffer::BufferDocument;
use super::builder::BufferDocumentBuilder;
use super::error::BufferDocumentError;
use super::type_remap::NodeSchemaBinding;
use super::BufferDocumentOptions;

// ── SilentValidationSink ──────────────────────────────────────────────

/// A [`ValidationSink`] that silently discards all errors and warnings.
///
/// Used by [`build_typed_document`] when the caller only needs schema
/// bindings on nodes and does not care about validation diagnostics.
pub struct SilentValidationSink;

impl ValidationSink for SilentValidationSink {
    fn on_error(&mut self, _error: ValidationError) {}
    fn on_warning(&mut self, _warning: ValidationWarning) {}
}

// ── TypedBuildError ───────────────────────────────────────────────────

/// Why a typed build stopped: the document could not be read or built, the
/// caller's [`ValidationEventHandler`] returned an error, or the runtime was
/// for another schema set.
///
/// Validation diagnostics are not errors of the build: they go to the
/// runtime's [`ValidationSink`], and the build completes.
///
/// `Display` shows the document error, or `handler error: ` and the
/// handler's error. It is a [`std::error::Error`] when `E` is one; its
/// `source()` is then the handler's error (for `Document`, the document
/// error's own source).
#[derive(Debug)]
#[non_exhaustive]
pub enum TypedBuildError<E> {
    /// The XML could not be read, or the document could not be built: text
    /// that is not well-formed (input that ends inside an element included),
    /// a parse limit of the runtime broken, an I/O error, a duplicate
    /// `xml:id`, a node-count limit.
    Document(BufferDocumentError),
    /// The caller's handler returned this error; the build stopped there.
    Handler(E),
    /// The runtime does not validate against the schema set the document is
    /// built for (`build_typed_document_with`'s `schema_set`): its bindings
    /// would point into another set's components. Nothing was read.
    SchemaSetMismatch,
}

impl<E> From<BufferDocumentError> for TypedBuildError<E> {
    fn from(e: BufferDocumentError) -> Self {
        TypedBuildError::Document(e)
    }
}

impl<E: std::fmt::Display> std::fmt::Display for TypedBuildError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypedBuildError::Document(e) => write!(f, "{e}"),
            TypedBuildError::Handler(e) => write!(f, "handler error: {e}"),
            TypedBuildError::SchemaSetMismatch => {
                f.write_str("the validation runtime is for another schema set than the document's")
            }
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for TypedBuildError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TypedBuildError::Document(e) => e.source(),
            TypedBuildError::Handler(e) => Some(e),
            TypedBuildError::SchemaSetMismatch => None,
        }
    }
}

// ── Handler ───────────────────────────────────────────────────────────

/// The typed-document builder as a [`ValidationEventHandler`]: it mirrors
/// the validator events of one pass into a [`BufferDocumentBuilder`], and
/// forwards every event to an inner handler `H` (by default the no-op
/// [`NoopHandler`]).
///
/// Every hook first does the builder's work, then calls the same hook of the
/// inner handler with the same arguments — so the inner handler sees each
/// element's [`SchemaInfo`] after the node has its binding, and an error it
/// returns stops the pass as [`TypedBuildError::Handler`]. Builder failures
/// come back as [`TypedBuildError::Document`].
///
/// Drive it with [`drive_quick_xml_with`] over a runtime from the caller's
/// [`SchemaValidator`], then [`finalize`](BufferDocumentBuilder::finalize)
/// the builder — or let [`build_typed_document_with`] do both. The builder
/// must have been created for the same [`SchemaSet`] as the validator
/// (`BufferDocumentBuilder::new(arena, &set.name_table, Some(&set), options)`).
///
/// Internally it keeps the per-element bookkeeping of the build: the
/// element-ref stack (so text/end events correlate to the open element), the
/// slot for the attribute being processed, the per-element queue of
/// attributes whose binding waits for type-alternative selection, the
/// source-span tracking, and the content of simple-content elements, whose
/// text becomes the [schema normalized value] once the element is known to be
/// valid (XDM 1.0 §6.2.4; an attribute's value likewise, §6.3.4).
///
/// The tree differs from the events in three more ways, all XDM's
/// construction from a PSVI:
///
/// * the defaulted attributes of each element
///   ([`EndOfAttributesView::default_attributes`]) are added as attribute
///   nodes, and a valid empty element that takes its declaration's `default` /
///   `fixed` value gets a text node with it — see
///   [`with_defaults_as_nodes`](Self::with_defaults_as_nodes), which can turn
///   both off;
/// * a text node of an element whose governing type has element-only content
///   (or empty content, when the element is nilled) that is all XML white
///   space is not built (XDM 1.0 §6.7.4: "if the resulting Text Node consists
///   entirely of whitespace and … that parent is an element and its {content
///   type} is not “mixed”, then the content of the Text Node is the
///   zero-length string … an empty Text Node will be discarded"). The
///   decision is made by the governing type when the text would become a node
///   — a text node that is not all white space, which makes such content
///   invalid, is kept as read with the white space in it; an element found
///   invalid only later (a required child missing) has lost its white space
///   already, consistently with the element-only annotation it keeps. The
///   inner handler still sees every text event;
/// * an element no declaration or type governs (laxly assessed: under a lax
///   wildcard, or the undeclared root of a `ValidationRoot::Lax` run) is
///   unbound — `xs:untyped` — when nothing in it was assessed, and bound to
///   `xs:anyType` when its `[validation attempted]` is partial: an attribute
///   (an `xsi:*` one included, which its built-in declaration governs) or a
///   descendant was assessed (XDM 1.0 §3.3.1.1). That is known when the
///   element ends ([`EndElementInfo::validation_attempted`]), so the binding
///   is set then.
///
/// Deliberately, the `xs:anyType` rule is applied to unbound elements only.
/// XDM 1.0 §3.3.1.1 reads "The \[validity\] property exists and is "invalid",
/// or the \[validation attempted\] property exists and is "partial", the schema
/// type of an element is xs:anyType and the type of an attribute is
/// xs:anySimpleType" — taken literally, also a *strictly assessed* element
/// whose own `[validation attempted]` is partial (a valid element with a child
/// a lax or skip wildcard left unassessed, say) or which is invalid, and an
/// invalid attribute. Such a node keeps the binding its declaration or type
/// gives it: its type is the one validation used, and a declared element
/// with an open wildcard would otherwise lose its type to `xs:anyType`. The
/// tree's `nilled` property follows XDM §6.2.4: an element is nilled only if
/// it is valid and its declaration nils it.
pub struct TypedBuilderHandler<'b, 'a, H = NoopHandler> {
    builder: &'b mut BufferDocumentBuilder<'a>,
    inner: H,
    track: bool,
    pending_start_offset: Option<usize>,
    /// Currently open elements' refs. Pushed at `before_element`, popped at
    /// `after_end_element`.
    elem_ref_stack: Vec<u32>,
    /// (elem_ref, start_byte) for each currently open element when
    /// `track` is on. Popped at `on_element_end_offset` to set the span.
    pending_spans: Vec<(u32, usize)>,
    /// Single-slot scratch for the attribute currently between
    /// `before_attribute` and `after_attribute`.
    current_attr_ref: Option<u32>,
    /// Per-element queue of attribute refs whose binding is deferred until
    /// CTA reselection. Drained in `after_end_of_attributes`.
    deferred_attr_refs: Vec<u32>,
    /// Element-decl key for the currently open element (used so we can
    /// preserve `element_decl` when refreshing the binding after CTA).
    element_decl_stack: Vec<Option<crate::ids::ElementKey>>,
    /// Per open element: whether its start `SchemaInfo` nils it. The tree's
    /// nilled property is set when it ends, and only if it is valid.
    nilled_stack: Vec<bool>,
    /// Per open element: whether it has a binding (a governing type, from its
    /// start or a type alternative).
    bound_stack: Vec<bool>,
    /// Per open element: whether its content is a simple type's, and what is
    /// held back of it (see [`SimpleContent`]).
    simple_stack: Vec<SimpleContent>,
    /// The whiteSpace of the types met so far (`None`: a union's, which
    /// depends on the member type) — a few per document, looked up linearly.
    whitespace_of: Vec<(
        crate::ids::TypeKey,
        Option<crate::types::facets::WhitespaceMode>,
    )>,
    /// Whether defaulted attributes and element defaults become nodes
    /// ([`with_defaults_as_nodes`](Self::with_defaults_as_nodes)).
    defaults_as_nodes: bool,
}

/// The content of an open element whose governing type is a simple type or a
/// complex type with simple content. XDM 1.0 §6.2.4: "its string-value is the
/// [schema normalized value] of the node", which exists when the element is
/// valid (Structures §3.3.5.4); the children may be its comments and
/// processing instructions "plus an optional single Text Node whose string
/// value is the [schema normalized value]", absent when that is empty. An
/// invalid element has no such value (its type is `xs:anyType`, XDM §3.3.1.1)
/// and keeps its content as read.
enum SimpleContent {
    /// Not simple content (or not assessed): built as read.
    No,
    /// Nilled (`xsi:nil="true"`): no content to normalize.
    Nilled,
    /// Element-only content (complex type, element-only content; or empty
    /// content of a nilled element, which accepts white space too): a text
    /// node that is all white space is no node (XDM 1.0 §6.7.4: "if the
    /// resulting Text Node consists entirely of whitespace and … that parent
    /// is an element and its {content type} is not “mixed”, then the content
    /// of the Text Node is the zero-length string … an empty Text Node will be
    /// discarded when its parent is constructed"). Decided when the text would
    /// become a node, by the governing type: the element's validity is known
    /// only at its end, after its children are built.
    ElementOnly,
    /// Simple content of `type_key`. Until a comment or processing
    /// instruction comes, its text waits in the builder's pending text;
    /// after one, the content is held here, in order, until the element ends.
    /// `saw_text`: a text event came (character \[children\], which keep an
    /// empty element from taking its declaration's default, Structures
    /// §3.3.4.3 clause 5.1).
    Yes {
        type_key: crate::ids::TypeKey,
        held: Option<Vec<HeldContent>>,
        saw_text: bool,
    },
}

impl SimpleContent {
    /// The slot of an element governed by `schema_type` with `content_type`,
    /// nilled or not.
    fn of(
        schema_type: Option<crate::ids::TypeKey>,
        content_type: Option<crate::validation::info::ContentType>,
        nilled: bool,
    ) -> Self {
        use crate::validation::info::ContentType;
        match (schema_type, content_type) {
            (Some(crate::ids::TypeKey::Complex(_)), Some(ContentType::ElementOnly)) => {
                SimpleContent::ElementOnly
            }
            (Some(crate::ids::TypeKey::Complex(_)), Some(ContentType::Empty)) if nilled => {
                SimpleContent::ElementOnly
            }
            _ if nilled => SimpleContent::Nilled,
            (Some(type_key), Some(ContentType::TextOnly)) => SimpleContent::Yes {
                type_key,
                held: None,
                saw_text: false,
            },
            _ => SimpleContent::No,
        }
    }
}

/// Whether `text` is XML white space only (#x20, #x9, #xD, #xA) — the white
/// space of XDM 1.0 §6.7.4 and of element-only content (Structures §3.4.4.2
/// clause 1.3), not Unicode's.
fn is_xml_whitespace(text: &str) -> bool {
    text.bytes()
        .all(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
}

/// Content of a simple-content element held until its validity is known.
enum HeldContent {
    Text(String),
    Comment(String),
    ProcessingInstruction(String, String),
}

impl<'b, 'a> TypedBuilderHandler<'b, 'a, NoopHandler> {
    /// A handler that builds into `builder` and forwards nothing.
    pub fn new(builder: &'b mut BufferDocumentBuilder<'a>) -> Self {
        Self::with_handler(builder, NoopHandler)
    }
}

impl<'b, 'a, H> TypedBuilderHandler<'b, 'a, H> {
    /// A handler that builds into `builder` and forwards every event to
    /// `inner`, in the same pass.
    pub fn with_handler(builder: &'b mut BufferDocumentBuilder<'a>, inner: H) -> Self {
        let track = builder.track_source_locations();
        Self {
            builder,
            inner,
            track,
            pending_start_offset: None,
            elem_ref_stack: Vec::new(),
            pending_spans: Vec::new(),
            current_attr_ref: None,
            deferred_attr_refs: Vec::new(),
            element_decl_stack: Vec::new(),
            nilled_stack: Vec::new(),
            bound_stack: Vec::new(),
            simple_stack: Vec::new(),
            whitespace_of: Vec::new(),
            defaults_as_nodes: true,
        }
    }

    /// Whether the values the schema supplies become nodes of the tree — on
    /// by default, as XDM 1.0 §6.2.4 builds them from the PSVI: each
    /// defaulted attribute of an element (Structures §3.4.5.1) becomes an
    /// attribute node, with a prefix bound to its namespace (declared on the
    /// element when none is in scope, the ·namespace fixup·); and a valid
    /// element of a simple type or with simple content that takes its
    /// declaration's `default` / `fixed` value (no element or character
    /// children, not nilled — clause 5.1 of Element Locally Valid (Element),
    /// §3.3.4.3) gets a text node with it, before its comments and processing
    /// instructions. Either value is the value constraint's {lexical form},
    /// white space normalized as the node's type prescribes. An `xs:QName` /
    /// `xs:NOTATION` in it (alone, in a list, or as a union's member) is the
    /// schema's expanded name — the literal read with the bindings of the
    /// schema element it is written on (Datatypes §3.3.18) — and is written
    /// again with a prefix the instance binds to its namespace: the schema's
    /// own prefix where the instance binds it so, another in-scope prefix, no
    /// prefix under the same default namespace, else `ns0`, `ns1`, … declared on
    /// the element (·namespace fixup·, which Structures §3.4.5.1 and §3.3.5.4
    /// leave to the processor). Only a name in no namespace under a default
    /// namespace cannot be written so (no prefix binds no namespace): it keeps
    /// its lexical form. An element of mixed content takes no default text
    /// (XDM leaves that case to the processor).
    ///
    /// `false` builds the tree as read: no defaulted attribute, no default
    /// text (the navigator still gives an empty defaulted element its default
    /// as its typed value).
    pub fn with_defaults_as_nodes(mut self, defaults_as_nodes: bool) -> Self {
        self.defaults_as_nodes = defaults_as_nodes;
        self
    }

    /// The inner handler.
    pub fn handler(&self) -> &H {
        &self.inner
    }

    /// The inner handler, mutably.
    pub fn handler_mut(&mut self) -> &mut H {
        &mut self.inner
    }

    /// Give the inner handler back (the builder stays borrowed until the
    /// handler is dropped; finalize it afterwards).
    pub fn into_handler(self) -> H {
        self.inner
    }

    // The builder's part of each event.

    fn build_element_start_offset(&mut self, byte_pos: usize) {
        if self.track {
            self.pending_start_offset = Some(byte_pos);
        }
    }

    fn build_before_element(
        &mut self,
        view: ElementStartView<'_>,
    ) -> Result<(), BufferDocumentError> {
        // A child element in simple content (the parent is not valid): the
        // parent's content goes in as read.
        if let Some(parent @ SimpleContent::Yes { .. }) = self.simple_stack.last_mut() {
            if let SimpleContent::Yes {
                held: Some(held), ..
            } = std::mem::replace(parent, SimpleContent::No)
            {
                self.replay(held)?;
            }
        }
        self.drop_element_content_whitespace();
        let elem_ref = self.builder.start_element(
            view.local_name,
            view.namespace_uri,
            view.prefix,
            view.namespace_decls,
        )?;
        self.elem_ref_stack.push(elem_ref);
        if self.track {
            if let Some(start) = self.pending_start_offset.take() {
                self.pending_spans.push((elem_ref, start));
            }
        }
        Ok(())
    }

    fn build_after_element(&mut self, info: &SchemaInfo) -> Result<(), BufferDocumentError> {
        let elem_ref = *self
            .elem_ref_stack
            .last()
            .expect("after_element with empty stack");
        self.element_decl_stack.push(info.element_decl);
        self.nilled_stack.push(info.is_nil);
        self.bound_stack.push(info.schema_type.is_some());
        self.simple_stack.push(SimpleContent::of(
            info.schema_type,
            info.content_type,
            info.is_nil,
        ));
        if let Some(tk) = info.schema_type {
            let binding = NodeSchemaBinding {
                type_key: tk,
                element_decl: info.element_decl,
                attribute_decl: None,
                content_type: info.content_type,
            };
            self.builder.set_node_binding(elem_ref, binding)?;
        }
        Ok(())
    }

    /// Give an attribute its [schema normalized value] (XDM 1.0 §6.3.4: "The
    /// [schema normalized value] PSVI property if that exists. Otherwise, the
    /// [normalized value] property") — it exists when the attribute is valid.
    fn normalize_attribute(
        &mut self,
        attr_ref: u32,
        info: &SchemaInfo,
    ) -> Result<(), BufferDocumentError> {
        let (Some(type_key), crate::validation::info::SchemaValidity::Valid) =
            (info.schema_type, info.validity)
        else {
            return Ok(());
        };
        let mode = self.whitespace(type_key);
        if let Some(normalized) =
            self.normalized(self.builder.attribute_value(attr_ref), type_key, mode)
        {
            self.builder.set_attribute_value(attr_ref, &normalized);
        }
        Ok(())
    }

    /// The whiteSpace of `type_key` ([`content_whitespace`]), remembered.
    ///
    /// [`content_whitespace`]: crate::validation::simple::content_whitespace
    fn whitespace(
        &mut self,
        type_key: crate::ids::TypeKey,
    ) -> Option<crate::types::facets::WhitespaceMode> {
        if let Some((_, mode)) = self.whitespace_of.iter().find(|(key, _)| *key == type_key) {
            return *mode;
        }
        let mode = self.builder.schema_set().and_then(|schema_set| {
            crate::validation::simple::content_whitespace(type_key, schema_set)
        });
        if self.whitespace_of.len() < 32 {
            self.whitespace_of.push((type_key, mode));
        }
        mode
    }

    /// `text`'s [schema normalized value] as a lexical form of `type_key`
    /// (whose whiteSpace is `mode`, `None` for a union), when it differs from
    /// `text`.
    fn normalized(
        &self,
        text: &str,
        type_key: crate::ids::TypeKey,
        mode: Option<crate::types::facets::WhitespaceMode>,
    ) -> Option<String> {
        match mode {
            Some(mode) => crate::validation::simple::whitespace_changes(text, mode),
            None => crate::validation::simple::schema_normalized(
                text,
                type_key,
                self.builder.schema_set()?,
            ),
        }
    }

    /// Put held content into the document, in order, as read.
    fn replay(&mut self, held: Vec<HeldContent>) -> Result<(), BufferDocumentError> {
        for content in held {
            match content {
                HeldContent::Text(text) => self.builder.text(&text),
                HeldContent::Comment(text) => self.builder.comment(&text)?,
                HeldContent::ProcessingInstruction(target, data) => {
                    self.builder.processing_instruction(&target, &data)?
                }
            }
        }
        Ok(())
    }

    /// Hold a comment or processing instruction of a simple-content element;
    /// `false` when the open element has no simple content (build it).
    fn hold(&mut self, content: HeldContent) -> bool {
        let Some(SimpleContent::Yes { held, .. }) = self.simple_stack.last_mut() else {
            return false;
        };
        let held = held.get_or_insert_with(|| {
            let pending = self.builder.take_pending_text();
            if pending.is_empty() {
                Vec::new()
            } else {
                vec![HeldContent::Text(pending)]
            }
        });
        held.push(content);
        true
    }

    fn build_before_attribute(
        &mut self,
        view: AttributeView<'_>,
    ) -> Result<(), BufferDocumentError> {
        let attr_ref =
            self.builder
                .attribute(view.local_name, view.namespace_uri, view.prefix, view.value)?;
        // This driver also reads a real XML document, so a duplicate `xml:id`
        // is reported here exactly as the plain text parser reports it.
        if let Some(taken) = self.builder.dropped_duplicate_id() {
            return Err(BufferDocumentError::DuplicateId(taken.into_string()));
        }
        self.current_attr_ref = Some(attr_ref);
        Ok(())
    }

    fn build_after_attribute(&mut self, info: &SchemaInfo) -> Result<(), BufferDocumentError> {
        let attr_ref = self
            .current_attr_ref
            .take()
            .expect("after_attribute without matching before_attribute");
        // An attribute of an element with type alternatives is bound once, in
        // `after_end_of_attributes`, with the type the selected alternative
        // gives it: whatever this event carries is not final.
        if info.deferred_by_cta {
            self.deferred_attr_refs.push(attr_ref);
            return Ok(());
        }
        self.normalize_attribute(attr_ref, info)?;
        // Binding the attribute is also what files it in the id index when
        // its typed value is an `xs:ID` (`set_node_binding`); an `xml:id` was
        // filed by `attribute()` already.
        if let Some(tk) = info.schema_type {
            let binding = NodeSchemaBinding {
                type_key: tk,
                element_decl: None,
                attribute_decl: info.attribute_decl,
                content_type: None,
            };
            self.builder.set_node_binding(attr_ref, binding)?;
        }
        Ok(())
    }

    fn build_after_end_of_attributes(
        &mut self,
        view: &EndOfAttributesView<'_>,
    ) -> Result<(), BufferDocumentError> {
        let elem_ref = *self
            .elem_ref_stack
            .last()
            .expect("after_end_of_attributes without an open element");
        let element_decl = *self
            .element_decl_stack
            .last()
            .expect("after_end_of_attributes without an open element");

        if let Some(tk) = view.info.schema_type {
            let binding = NodeSchemaBinding {
                type_key: tk,
                element_decl,
                attribute_decl: None,
                content_type: view.info.content_type,
            };
            self.builder.set_node_binding(elem_ref, binding)?;
            if let Some(bound) = self.bound_stack.last_mut() {
                *bound = true;
            }
            // A type alternative selected the governing type: it decides.
            let nilled = self.nilled_stack.last().copied().unwrap_or(false);
            if let Some(slot) = self.simple_stack.last_mut() {
                *slot = SimpleContent::of(Some(tk), view.info.content_type, nilled);
            }
        }

        let deferred_refs = std::mem::take(&mut self.deferred_attr_refs);
        if deferred_refs.len() != view.deferred_attribute_results.len() {
            return Err(BufferDocumentError::InternalError(
                "deferred attribute count mismatch".into(),
            ));
        }
        // The final bindings of the attributes `after_attribute` deferred —
        // and, through `set_node_binding`, their entries in the id index.
        for (attr_ref, attr_info) in deferred_refs
            .iter()
            .zip(view.deferred_attribute_results.iter())
        {
            self.normalize_attribute(*attr_ref, attr_info)?;
            if let Some(tk) = attr_info.schema_type {
                let binding = NodeSchemaBinding {
                    type_key: tk,
                    element_decl: None,
                    attribute_decl: attr_info.attribute_decl,
                    content_type: None,
                };
                self.builder.set_node_binding(*attr_ref, binding)?;
            }
        }
        if self.defaults_as_nodes {
            for default in view.default_attributes {
                self.add_defaulted_attribute(default)?;
            }
        }
        self.builder.end_of_attributes();
        Ok(())
    }

    /// Add a defaulted attribute to the open element (Structures §3.4.5.1):
    /// its {lexical form} normalized for its type, with a prefix bound to its
    /// namespace — one in scope, or one declared on the element (·namespace
    /// fixup·) — and bound to its declared type (`xs:anySimpleType` when the
    /// declaration names none), which also files it in the id index when its
    /// typed value is an `xs:ID`.
    fn add_defaulted_attribute(
        &mut self,
        default: &crate::validation::info::DefaultAttribute,
    ) -> Result<(), BufferDocumentError> {
        let Some(schema_set) = self.builder.schema_set() else {
            return Ok(());
        };
        let names = &schema_set.name_table;
        let local = names.resolve_ref(default.local_name);
        let namespace = default.namespace.map_or("", |ns| names.resolve_ref(ns));
        let prefix = if namespace.is_empty() {
            String::new()
        } else if let Some(prefix) = self.builder.prefix_in_scope(namespace) {
            prefix.to_string()
        } else {
            let prefix = (0..)
                .map(|n| format!("ns{n}"))
                .find(|p| !self.builder.prefix_is_declared(p))
                .expect("an undeclared prefix");
            self.builder
                .declare_namespace_on_open_element(&prefix, namespace)?;
            prefix
        };
        let type_key = default.type_key.or_else(|| {
            schema_set
                .builtin_types()
                .get_by_type_code(crate::types::XmlTypeCode::AnySimpleType)
                .map(crate::ids::TypeKey::Simple)
        });
        let normalized = type_key.and_then(|tk| {
            let mode = self.whitespace(tk);
            self.normalized(&default.value, tk, mode)
        });
        let lexical = normalized.as_deref().unwrap_or(&default.value);
        // A QName / NOTATION in the value is the schema's expanded name (the
        // {value}, read with the schema element's bindings): written again
        // with a prefix the instance binds to its namespace.
        let fixed_up = match default.typed_value.as_ref() {
            Some(typed) => self.qname_lexical_in_scope(typed, lexical)?,
            None => None,
        };
        let value = fixed_up.as_deref().unwrap_or(lexical);
        let attr_ref = self.builder.attribute(local, namespace, &prefix, value)?;
        if let Some(taken) = self.builder.dropped_duplicate_id() {
            return Err(BufferDocumentError::DuplicateId(taken.into_string()));
        }
        if let Some(type_key) = type_key {
            let binding = NodeSchemaBinding {
                type_key,
                element_decl: None,
                attribute_decl: default.attribute_key,
                content_type: None,
            };
            self.builder.set_node_binding(attr_ref, binding)?;
        }
        Ok(())
    }

    /// The `default` / `fixed` value of the open element's declaration, if
    /// it has one (the default first, as validation takes it), with the
    /// declaration it is written on.
    fn declared_default(&self) -> Option<(&'a str, &'a crate::arenas::ElementDeclData)> {
        let decl = (*self.element_decl_stack.last()?)?;
        let element = self.builder.schema_set()?.arenas.elements.get(decl)?;
        let value = element
            .default_value
            .as_deref()
            .or(element.fixed_value.as_deref())?;
        Some((value, element))
    }

    /// The text an element's value constraint `value` (of the declaration
    /// `decl`) becomes in the open element whose governing type is
    /// `type_key`: its {lexical form} — under a type other than the declared
    /// one the form itself, as validation reads it (Structures §3.3.4.3
    /// clause 5.1.2) —, white space normalized for `type_key`, a QName /
    /// NOTATION in it written again for the instance
    /// ([`qname_lexical_in_scope`]).
    ///
    /// [`qname_lexical_in_scope`]: Self::qname_lexical_in_scope
    fn default_text(
        &mut self,
        value: &str,
        decl: &'a crate::arenas::ElementDeclData,
        type_key: crate::ids::TypeKey,
    ) -> Result<String, BufferDocumentError> {
        let source = decl.source.as_ref();
        let overridden = self.builder.schema_set().and_then(|schema_set| {
            let declared = decl
                .resolved_type
                .unwrap_or_else(|| crate::ids::TypeKey::Complex(schema_set.any_type_key()));
            (declared != type_key)
                .then(|| schema_set.value_constraint_lexical_form(decl))
                .flatten()
        });
        let value = overridden.as_deref().unwrap_or(value);
        let mode = self.whitespace(type_key);
        let lexical = self
            .normalized(value, type_key, mode)
            .unwrap_or_else(|| value.to_string());
        let Some(schema_set) = self.builder.schema_set() else {
            return Ok(lexical);
        };
        if !crate::validation::simple::type_has_qname_values(type_key, schema_set) {
            return Ok(lexical);
        }
        let Some(snapshot) = schema_set.value_constraint_namespaces(source) else {
            return Ok(lexical);
        };
        let resolver =
            crate::validation::simple::snapshot_prefixes(snapshot, &schema_set.name_table);
        let Ok(typed) = crate::validation::simple::validate_simple_type_in(
            &lexical,
            type_key,
            schema_set,
            Some(&resolver),
        ) else {
            return Ok(lexical);
        };
        Ok(self
            .qname_lexical_in_scope(&typed.typed_value, &lexical)?
            .unwrap_or(lexical))
    }

    /// The lexical form, in the scope of the open element, of `typed` — a
    /// value of a value constraint whose QNames / NOTATIONs hold the schema's
    /// expanded names — when it holds any (`None` otherwise; `lexical`, its
    /// {lexical form}, is then the text): each such name gets a prefix the
    /// instance binds to its namespace, the other list members keep their
    /// lexical forms. Structures §3.4.5.1 Note: "When a default value of type
    /// QName or NOTATION is applied, it is ·implementation-dependent· whether
    /// ·namespace fixup· occurs; if it does not, the prefix used in the
    /// lexical representation … will not necessarily map to the namespace
    /// name of the value" — this performs it.
    fn qname_lexical_in_scope(
        &mut self,
        typed: &crate::types::value::XmlValue,
        lexical: &str,
    ) -> Result<Option<String>, BufferDocumentError> {
        use crate::types::value::{XmlAtomicValue, XmlValueKind};
        if !crate::validation::simple::holds_qname_value(typed) {
            return Ok(None);
        }
        match &typed.value {
            XmlValueKind::Atomic(XmlAtomicValue::QName(qn) | XmlAtomicValue::Notation(qn)) => {
                self.qname_in_scope(qn).map(Some)
            }
            XmlValueKind::Union(member) => self.qname_lexical_in_scope(member, lexical),
            XmlValueKind::List { items, .. } => {
                let tokens: Vec<&str> = lexical
                    .split([' ', '\t', '\n', '\r'])
                    .filter(|t| !t.is_empty())
                    .collect();
                let mut out: Vec<String> = Vec::with_capacity(items.len());
                for (i, item) in items.iter().enumerate() {
                    match self.qname_lexical_in_scope(item, tokens.get(i).copied().unwrap_or(""))? {
                        Some(written) => out.push(written),
                        None => out.push(match tokens.get(i) {
                            Some(token) => token.to_string(),
                            None => item.to_string_value(),
                        }),
                    }
                }
                Ok(Some(out.join(" ")))
            }
            _ => Ok(None),
        }
    }

    /// `qn` written for the open element: with the schema's own prefix when
    /// the instance binds it to the same namespace; unprefixed when the schema
    /// wrote it so and the instance's default namespace is the same; else with
    /// another prefix bound to the namespace in scope; else unprefixed when the
    /// default namespace is it; else with a prefix declared on the element
    /// (`ns0`, `ns1`, … — the ·namespace fixup·). A name in no namespace is
    /// unprefixed: no prefix can be bound to no namespace, so under a default
    /// namespace it keeps the lexical form, which then reads as a name in that
    /// namespace (the one case the fixup cannot cover).
    fn qname_in_scope(
        &mut self,
        qn: &crate::types::value::QNameValue,
    ) -> Result<String, BufferDocumentError> {
        let Some(schema_set) = self.builder.schema_set() else {
            return Ok(qn.to_string());
        };
        let names = &schema_set.name_table;
        let name = qn.name();
        let local = names.resolve_ref(name.local_name);
        let Some(ns) = name.namespace_uri else {
            return Ok(local.to_string());
        };
        let uri = names.resolve_ref(ns);
        let schema_prefix = name
            .prefix
            .map(|p| names.resolve_ref(p))
            .filter(|p| !p.is_empty());
        let default_ns = self.builder.default_namespace_in_scope();
        match schema_prefix {
            Some(p) if self.builder.namespace_of_prefix(p) == Some(uri) => {
                return Ok(format!("{p}:{local}"));
            }
            None if default_ns == Some(uri) => return Ok(local.to_string()),
            _ => {}
        }
        if let Some(p) = self.builder.prefix_in_scope(uri) {
            return Ok(format!("{p}:{local}"));
        }
        if default_ns == Some(uri) {
            return Ok(local.to_string());
        }
        let prefix = (0..)
            .map(|n| format!("ns{n}"))
            .find(|p| !self.builder.prefix_is_declared(p))
            .expect("an undeclared prefix");
        self.builder
            .declare_namespace_on_open_element(&prefix, uri)?;
        Ok(format!("{prefix}:{local}"))
    }

    /// XDM 1.0 §6.7.4: the text waiting to become a node of the open element
    /// is no node when the element's content is element-only and the text is
    /// all white space.
    fn drop_element_content_whitespace(&mut self) {
        if matches!(self.simple_stack.last(), Some(SimpleContent::ElementOnly)) {
            self.drop_pending_whitespace();
        }
    }

    fn drop_pending_whitespace(&mut self) {
        if is_xml_whitespace(self.builder.pending_text()) {
            self.builder.discard_pending_text();
        }
    }

    fn build_after_end_element(
        &mut self,
        info: &EndElementInfo,
    ) -> Result<(), BufferDocumentError> {
        let valid = info.validity == crate::validation::info::SchemaValidity::Valid;
        // Clause 5.1 of Element Locally Valid (Element): an element with no
        // element or character [children] takes its declaration's value
        // constraint, whose {lexical form} is then its [schema normalized
        // value] (§3.3.5.4 clause 1.1).
        let default = |this: &Self, saw_text: bool| {
            (this.defaults_as_nodes && !saw_text)
                .then(|| this.declared_default())
                .flatten()
        };
        match self.simple_stack.pop() {
            Some(SimpleContent::Yes {
                type_key,
                held: None,
                saw_text,
            }) => {
                if valid && self.builder.has_only_pending_text() {
                    let mode = self.whitespace(type_key);
                    match default(self, saw_text) {
                        Some((value, decl)) => {
                            let text = self.default_text(value, decl, type_key)?;
                            self.builder.replace_pending_text(&text);
                        }
                        None => {
                            if let Some(normalized) =
                                self.normalized(self.builder.pending_text(), type_key, mode)
                            {
                                self.builder.replace_pending_text(&normalized);
                            }
                        }
                    }
                }
            }
            Some(SimpleContent::Yes {
                type_key,
                held: Some(mut held),
                saw_text,
            }) => {
                if valid {
                    // One text node with the normalized value, where the first
                    // text was (or first, for a default); the comments and
                    // instructions in their order.
                    let text: String = match default(self, saw_text) {
                        Some((value, decl)) => {
                            held.insert(0, HeldContent::Text(String::new()));
                            self.default_text(value, decl, type_key)?
                        }
                        None => held
                            .iter()
                            .filter_map(|content| match content {
                                HeldContent::Text(text) => Some(text.as_str()),
                                _ => None,
                            })
                            .collect(),
                    };
                    let mode = self.whitespace(type_key);
                    let normalized = self.normalized(&text, type_key, mode).unwrap_or(text);
                    let mut text = Some(normalized);
                    let mut kept = Vec::with_capacity(held.len());
                    for content in held {
                        match content {
                            HeldContent::Text(_) => {
                                if let Some(text) = text.take().filter(|t| !t.is_empty()) {
                                    kept.push(HeldContent::Text(text));
                                }
                            }
                            other => kept.push(other),
                        }
                    }
                    self.replay(kept)?;
                } else {
                    self.replay(held)?;
                }
            }
            Some(SimpleContent::ElementOnly) => self.drop_pending_whitespace(),
            _ => {}
        }
        // An element no declaration or type governed — laxly assessed — is
        // `xs:untyped` only while nothing in it was assessed: XDM 1.0
        // §3.3.1.1, "The [validity] property exists and is "invalid", or the
        // [validation attempted] property exists and is "partial", the schema
        // type of an element is xs:anyType"; "The [validity] property exists
        // and is "notKnown", and the [validation attempted] property exists
        // and is "none", the schema type of an element is xs:untyped" (and
        // §6.2.1: an `xs:untyped` element has only `xs:untyped` descendants).
        // Known only now, when the element ends.
        let bound = self.bound_stack.pop().unwrap_or(true);
        if !bound && info.validation_attempted == ValidationAttempted::Partial {
            if let (Some(&elem_ref), Some(schema_set)) =
                (self.elem_ref_stack.last(), self.builder.schema_set())
            {
                let binding = NodeSchemaBinding {
                    type_key: crate::ids::TypeKey::Complex(schema_set.any_type_key()),
                    element_decl: None,
                    attribute_decl: None,
                    content_type: Some(crate::validation::info::ContentType::Mixed),
                };
                self.builder.set_node_binding(elem_ref, binding)?;
            }
        }
        // XDM 1.0 §6.2.4 `nilled`: "If the [validity] property exists on an
        // information item and is "valid" then if the [nil] property exists
        // and is true, then the nilled property is "true". In all other cases
        // … the nilled property is "false"" — decided now, when the element's
        // validity is known (a nillable element with `xsi:nil="true"` and
        // content, `cvc-elt.3.2.1`, is not nilled).
        let nilled = self.nilled_stack.pop().unwrap_or(false);
        if nilled && valid {
            if let Some(&elem_ref) = self.elem_ref_stack.last() {
                self.builder.set_nil(elem_ref);
            }
        }
        self.builder.end_element()?;
        self.elem_ref_stack.pop();
        self.element_decl_stack.pop();
        Ok(())
    }

    fn build_element_end_offset(&mut self, byte_pos: usize) {
        if self.track {
            if let Some((elem_ref, start)) = self.pending_spans.pop() {
                self.builder
                    .set_source_span(elem_ref, SourceSpan::new(start, byte_pos));
            }
        }
    }

    fn build_text(&mut self, text: &str) {
        if self.elem_ref_stack.is_empty() {
            return;
        }
        if let Some(SimpleContent::Yes { held, saw_text, .. }) = self.simple_stack.last_mut() {
            *saw_text = true;
            if let Some(held) = held {
                match held.last_mut() {
                    Some(HeldContent::Text(last)) => last.push_str(text),
                    _ => held.push(HeldContent::Text(text.to_string())),
                }
                return;
            }
        }
        self.builder.text(text);
    }
}

impl<H: ValidationEventHandler> ValidationEventHandler for TypedBuilderHandler<'_, '_, H> {
    type Error = TypedBuildError<H::Error>;

    fn on_element_start_offset(&mut self, byte_pos: usize) -> Result<(), Self::Error> {
        self.build_element_start_offset(byte_pos);
        self.inner
            .on_element_start_offset(byte_pos)
            .map_err(TypedBuildError::Handler)
    }

    fn before_element(&mut self, view: ElementStartView<'_>) -> Result<(), Self::Error> {
        self.build_before_element(view)?;
        self.inner
            .before_element(view)
            .map_err(TypedBuildError::Handler)
    }

    fn after_element(
        &mut self,
        view: ElementStartView<'_>,
        info: &SchemaInfo,
    ) -> Result<(), Self::Error> {
        self.build_after_element(info)?;
        self.inner
            .after_element(view, info)
            .map_err(TypedBuildError::Handler)
    }

    fn before_attribute(&mut self, view: AttributeView<'_>) -> Result<(), Self::Error> {
        self.build_before_attribute(view)?;
        self.inner
            .before_attribute(view)
            .map_err(TypedBuildError::Handler)
    }

    fn after_attribute(
        &mut self,
        view: AttributeView<'_>,
        info: &SchemaInfo,
    ) -> Result<(), Self::Error> {
        self.build_after_attribute(info)?;
        self.inner
            .after_attribute(view, info)
            .map_err(TypedBuildError::Handler)
    }

    /// The inner handler decides; the attribute is built either way (a
    /// skipped attribute stays untyped).
    fn skip_attribute(&mut self, view: AttributeView<'_>) -> Result<bool, Self::Error> {
        self.inner
            .skip_attribute(view)
            .map_err(TypedBuildError::Handler)
    }

    fn after_end_of_attributes(
        &mut self,
        view: EndOfAttributesView<'_>,
    ) -> Result<(), Self::Error> {
        self.build_after_end_of_attributes(&view)?;
        self.inner
            .after_end_of_attributes(view)
            .map_err(TypedBuildError::Handler)
    }

    fn after_end_element(
        &mut self,
        info: &EndElementInfo,
        depth: usize,
    ) -> Result<(), Self::Error> {
        self.build_after_end_element(info)?;
        self.inner
            .after_end_element(info, depth)
            .map_err(TypedBuildError::Handler)
    }

    fn on_element_end_offset(&mut self, byte_pos: usize) -> Result<(), Self::Error> {
        self.build_element_end_offset(byte_pos);
        self.inner
            .on_element_end_offset(byte_pos)
            .map_err(TypedBuildError::Handler)
    }

    fn on_text(&mut self, kind: TextKind, text: &str) -> Result<(), Self::Error> {
        self.build_text(text);
        self.inner
            .on_text(kind, text)
            .map_err(TypedBuildError::Handler)
    }

    fn on_comment(&mut self, text: &str) -> Result<(), Self::Error> {
        if !self.hold(HeldContent::Comment(text.to_string())) {
            self.drop_element_content_whitespace();
            self.builder.comment(text)?;
        }
        self.inner
            .on_comment(text)
            .map_err(TypedBuildError::Handler)
    }

    fn on_processing_instruction(&mut self, target: &str, data: &str) -> Result<(), Self::Error> {
        if !self.hold(HeldContent::ProcessingInstruction(
            target.to_string(),
            data.to_string(),
        )) {
            self.drop_element_content_whitespace();
            self.builder.processing_instruction(target, data)?;
        }
        self.inner
            .on_processing_instruction(target, data)
            .map_err(TypedBuildError::Handler)
    }

    /// The document keeps no document type declaration; the inner handler
    /// sees it (and may refuse it).
    fn on_doctype(&mut self, doctype: DoctypeView<'_>) -> Result<(), Self::Error> {
        self.inner
            .on_doctype(doctype)
            .map_err(TypedBuildError::Handler)
    }

    /// The document keeps the declaration; the inner handler sees it (and
    /// may refuse it).
    fn on_xml_declaration(
        &mut self,
        declaration: &crate::xml::XmlDeclaration,
    ) -> Result<(), Self::Error> {
        self.builder.set_xml_declaration(declaration.clone());
        self.inner
            .on_xml_declaration(declaration)
            .map_err(TypedBuildError::Handler)
    }
}

/// Lends a caller's handler to a [`TypedBuilderHandler`] for one pass.
struct LentHandler<'h, H: ?Sized>(&'h mut H);

impl<H: ValidationEventHandler + ?Sized> ValidationEventHandler for LentHandler<'_, H> {
    type Error = H::Error;

    fn before_element(&mut self, view: ElementStartView<'_>) -> Result<(), H::Error> {
        self.0.before_element(view)
    }
    fn after_element(
        &mut self,
        view: ElementStartView<'_>,
        info: &SchemaInfo,
    ) -> Result<(), H::Error> {
        self.0.after_element(view, info)
    }
    fn before_attribute(&mut self, view: AttributeView<'_>) -> Result<(), H::Error> {
        self.0.before_attribute(view)
    }
    fn after_attribute(
        &mut self,
        view: AttributeView<'_>,
        info: &SchemaInfo,
    ) -> Result<(), H::Error> {
        self.0.after_attribute(view, info)
    }
    fn skip_attribute(&mut self, view: AttributeView<'_>) -> Result<bool, H::Error> {
        self.0.skip_attribute(view)
    }
    fn after_end_of_attributes(&mut self, view: EndOfAttributesView<'_>) -> Result<(), H::Error> {
        self.0.after_end_of_attributes(view)
    }
    fn after_end_element(&mut self, info: &EndElementInfo, depth: usize) -> Result<(), H::Error> {
        self.0.after_end_element(info, depth)
    }
    fn on_text(&mut self, kind: TextKind, text: &str) -> Result<(), H::Error> {
        self.0.on_text(kind, text)
    }
    fn on_comment(&mut self, text: &str) -> Result<(), H::Error> {
        self.0.on_comment(text)
    }
    fn on_processing_instruction(&mut self, target: &str, data: &str) -> Result<(), H::Error> {
        self.0.on_processing_instruction(target, data)
    }
    fn on_doctype(&mut self, doctype: DoctypeView<'_>) -> Result<(), H::Error> {
        self.0.on_doctype(doctype)
    }
    fn on_xml_declaration(
        &mut self,
        declaration: &crate::xml::XmlDeclaration,
    ) -> Result<(), H::Error> {
        self.0.on_xml_declaration(declaration)
    }
    fn on_element_start_offset(&mut self, byte_pos: usize) -> Result<(), H::Error> {
        self.0.on_element_start_offset(byte_pos)
    }
    fn on_element_end_offset(&mut self, byte_pos: usize) -> Result<(), H::Error> {
        self.0.on_element_end_offset(byte_pos)
    }
}

// ── build_typed_document_with ─────────────────────────────────────────

/// Options of a [`build_typed_document_with`] pass.
///
/// `#[non_exhaustive]`, so options can be added later: start from
/// [`TypedBuildOptions::default`] and set what you need.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TypedBuildOptions {
    /// Options of the document built (its kind, source-span tracking).
    pub document: BufferDocumentOptions,
    /// Whether defaulted attributes and element defaults become nodes, as XDM
    /// 1.0 §6.2.4 builds them from the PSVI — `true` by default; see
    /// [`TypedBuilderHandler::with_defaults_as_nodes`].
    pub defaults_as_nodes: bool,
}

impl Default for TypedBuildOptions {
    fn default() -> Self {
        Self {
            document: BufferDocumentOptions::default(),
            defaults_as_nodes: true,
        }
    }
}

impl TypedBuildOptions {
    /// These options with `defaults_as_nodes` set (see
    /// [`TypedBuilderHandler::with_defaults_as_nodes`]).
    pub fn with_defaults_as_nodes(mut self, defaults_as_nodes: bool) -> Self {
        self.defaults_as_nodes = defaults_as_nodes;
        self
    }

    /// These options with `document` as the document options.
    pub fn with_document_options(mut self, document: BufferDocumentOptions) -> Self {
        self.document = document;
        self
    }
}

/// What a [`build_typed_document_with`] pass produced.
#[non_exhaustive]
pub struct TypedBuild<'a> {
    /// The document, with a [`NodeSchemaBinding`] on every element and
    /// attribute the validation typed.
    pub document: BufferDocument<'a>,
    /// The drive outcome: the root element's `[validity]` and the depth.
    pub outcome: DriveOutcome,
}

/// Validate a document and build it, typed, in one pass over `reader`, with
/// the caller's validator, sink and event handler.
///
/// * `runtime` comes from the caller's [`SchemaValidator`] for `schema_set`
///   (`validator.start_run(sink)`): the validator decides the flags — for
///   example `ValidationFlags::PROCESS_ASSERTIONS` to evaluate XSD 1.1
///   assertions — and the sink receives every error and warning, each with
///   its subject and positional path.
/// * `handler` sees every event of the pass, after the builder has handled
///   it (see [`TypedBuilderHandler`]); pass [`NoopHandler`] for none. An
///   error it returns stops the build as [`TypedBuildError::Handler`].
/// * `schema_set` must be the set the runtime's validator was made for
///   ([`TypedBuildError::SchemaSetMismatch`] otherwise); the document borrows
///   it (and `arena`) for `'a`.
/// * `handler` may be a trait object (`&mut dyn ValidationEventHandler<Error
///   = E>`).
/// * `options` carry the document options ([`TypedBuildOptions`]).
///
/// Like [`drive_quick_xml_with`], this does **not** call
/// [`ValidationRuntime::end_validation`]: call it afterwards — it reports
/// the checks that need the whole document (an IDREF without its ID,
/// `cvc-id.1`) to the sink, and returns an operational failure, if any. The
/// runtime also still answers
/// [`schema_location_hints`](ValidationRuntime::schema_location_hints) and
/// the identity-constraint tables.
///
/// Validation diagnostics never stop the build: an invalid document is
/// built and typed as far as the validation typed it.
///
/// A valid attribute, and a valid element of a simple type or a complex type
/// with simple content, has its [schema normalized value] as its string value
/// — its white space normalized as its type prescribes (XDM 1.0 §6.2.4,
/// §6.3.4) — whatever the validator's flags; such an element keeps a single
/// text node (none for an empty value) beside its comments and processing
/// instructions. Defaulted attributes are attribute nodes and an empty
/// element that takes its default has it as a text node, unless
/// [`TypedBuildOptions::defaults_as_nodes`] is `false`; white space in
/// element-only content is no text node (see [`TypedBuilderHandler`]). Mixed
/// content, and a node that is not valid, keeps its text as read; source spans
/// still point at the text as read.
///
/// # Example
///
/// Validate and type a document in one pass, and get the errors:
///
/// ```
/// use bumpalo::Bump;
/// use xsd_schema::document::{build_typed_document_with, TypedBuildOptions};
/// use xsd_schema::navigator::{DomNavigator, TypedValue};
/// use xsd_schema::validation::{
///     CollectingValidationSink, NoopHandler, SchemaValidator, SchemaValidity, ValidationFlags,
/// };
/// use xsd_schema::SchemaSetBuilder;
///
/// let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
///   <xs:element name="order">
///     <xs:complexType><xs:sequence>
///       <xs:element name="qty" type="xs:positiveInteger" maxOccurs="unbounded"/>
///     </xs:sequence></xs:complexType>
///   </xs:element>
/// </xs:schema>"#;
/// let schema_set = SchemaSetBuilder::new()
///     .add_source(xsd, "file:///order.xsd")?
///     .compile()?
///     .into_schema_set();
/// let validator = SchemaValidator::new(&schema_set, ValidationFlags::default());
///
/// let arena = Bump::new();
/// let (mut errors, mut warnings) = (Vec::new(), Vec::new());
/// let mut runtime = validator.start_run(CollectingValidationSink {
///     errors: &mut errors,
///     warnings: &mut warnings,
/// });
/// let built = build_typed_document_with(
///     "<order><qty>2</qty><qty>0</qty></order>".as_bytes(),
///     &arena,
///     &schema_set,
///     &mut runtime,
///     &mut NoopHandler,
///     TypedBuildOptions::default(),
/// )?;
/// runtime.end_validation()?; // the end-of-document checks report here
/// drop(runtime); // releases the sink's borrow of `errors`
///
/// // The errors, each with the item it is about…
/// assert_eq!(built.outcome.root_validity, Some(SchemaValidity::Invalid));
/// assert_eq!(errors.len(), 1);
/// assert_eq!(
///     errors[0].instance_path().unwrap().to_string(),
///     "/Q{}order[1]/Q{}qty[2]"
/// );
///
/// // …and the typed document, from the same pass.
/// let mut nav = built.document.create_navigator();
/// assert!(nav.move_to_first_child() && nav.move_to_first_child()); // order/qty[1]
/// assert!(matches!(nav.typed_value(), TypedValue::Value(_)));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn build_typed_document_with<'a, R, S, H>(
    reader: R,
    arena: &'a Bump,
    schema_set: &'a SchemaSet,
    runtime: &mut ValidationRuntime<'_, S>,
    handler: &mut H,
    options: TypedBuildOptions,
) -> Result<TypedBuild<'a>, TypedBuildError<H::Error>>
where
    R: BufRead,
    S: ValidationSink,
    H: ValidationEventHandler + ?Sized,
{
    if !std::ptr::eq(runtime.schema_set, schema_set) {
        return Err(TypedBuildError::SchemaSetMismatch);
    }
    let mut builder = BufferDocumentBuilder::new(
        arena,
        &schema_set.name_table,
        Some(schema_set),
        options.document,
    )?;
    let driven = {
        let mut typed = TypedBuilderHandler::with_handler(&mut builder, LentHandler(handler))
            .with_defaults_as_nodes(options.defaults_as_nodes);
        drive_quick_xml_with(reader, runtime, schema_set, &mut typed)
    };
    let outcome = driven.map_err(|e| match e {
        DriveWithError::Io(e) => TypedBuildError::Document(BufferDocumentError::Io(e)),
        DriveWithError::NotWellFormed(e) => {
            TypedBuildError::Document(BufferDocumentError::NotWellFormed(e))
        }
        DriveWithError::LimitExceeded(e) => {
            TypedBuildError::Document(BufferDocumentError::LimitExceeded(e))
        }
        DriveWithError::Hook(e) => e,
    })?;
    Ok(TypedBuild {
        document: builder.finalize()?,
        outcome,
    })
}

// ── build_typed_document ──────────────────────────────────────────────

/// Build a [`BufferDocument`] with schema bindings on every element and attribute.
///
/// This function mirrors [`BufferDocumentBuilder::build`] but interleaves
/// [`SchemaValidator`] calls so that the resulting document carries
/// [`NodeSchemaBinding`] entries for typed-value and schema-type queries.
///
/// The validator is `SchemaValidator::new(schema_set, ValidationFlags::default())`
/// — XSD 1.1 assertions are not evaluated — and validation errors are
/// silently discarded via [`SilentValidationSink`]. To choose the validator,
/// receive the errors or run a handler of your own in the same pass, use
/// [`build_typed_document_with`], on which this function is built.
pub fn build_typed_document<'a, R: BufRead>(
    reader: R,
    arena: &'a Bump,
    schema_set: &'a SchemaSet,
    options: BufferDocumentOptions,
) -> Result<BufferDocument<'a>, BufferDocumentError> {
    let validator = SchemaValidator::new(schema_set, ValidationFlags::default());
    let mut runtime = validator.start_run(SilentValidationSink);
    let built = build_typed_document_with(
        reader,
        arena,
        schema_set,
        &mut runtime,
        &mut NoopHandler,
        TypedBuildOptions::default().with_document_options(options),
    )
    .map_err(|e| match e {
        TypedBuildError::Document(e) => e,
        TypedBuildError::Handler(never) => match never {},
        TypedBuildError::SchemaSetMismatch => {
            unreachable!("the runtime is made from `schema_set` above")
        }
    })?;
    let _ = runtime.end_validation();
    Ok(built.document)
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::TypeKey;
    use crate::navigator::{DomNavigator, DomNodeType, NamespaceAxisScope, TypedValue};
    use crate::pipeline::load_and_process_schema;
    use crate::validation::info::ContentType;

    fn load_schema(xsd: &str) -> SchemaSet {
        let mut schema_set = SchemaSet::xsd11();
        load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None)
            .expect("failed to load schema");
        schema_set
    }

    fn build_doc<'a>(xml: &str, arena: &'a Bump, schema_set: &'a SchemaSet) -> BufferDocument<'a> {
        build_typed_document(
            xml.as_bytes(),
            arena,
            schema_set,
            BufferDocumentOptions::default(),
        )
        .expect("failed to build typed document")
    }

    // ── Test 1: schema_type() / element_type_key() work ──────────────

    #[test]
    fn typed_document_has_schema_bindings() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root" type="xs:string"/>
            </xs:schema>"#,
        );
        let arena = Bump::new();
        let doc = build_doc("<root>hello</root>", &arena, &schema_set);

        let mut nav = doc.create_navigator();
        assert!(nav.move_to_first_child()); // root element
        assert!(nav.element_type_key().is_some());
        assert!(matches!(nav.element_type_key(), Some(TypeKey::Simple(_))));
    }

    // ── Test 2: typed_value() for xs:integer attribute ────────────────

    #[test]
    fn typed_value_integer_attribute() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root">
                    <xs:complexType>
                        <xs:attribute name="count" type="xs:integer"/>
                    </xs:complexType>
                </xs:element>
            </xs:schema>"#,
        );
        let arena = Bump::new();
        let doc = build_doc(r#"<root count="42"/>"#, &arena, &schema_set);

        let mut nav = doc.create_navigator();
        assert!(nav.move_to_first_child()); // root element
        assert!(nav.move_to_first_attribute());
        assert!(nav.element_type_key().is_some());
        let tv = nav.typed_value();
        assert!(
            matches!(tv, TypedValue::Value(_)),
            "attribute should have typed value"
        );
    }

    // ── Test 3: typed_value() for TextOnly element ────────────────────

    #[test]
    fn typed_value_text_only_element() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root" type="xs:integer"/>
            </xs:schema>"#,
        );
        let arena = Bump::new();
        let doc = build_doc("<root>123</root>", &arena, &schema_set);

        let mut nav = doc.create_navigator();
        assert!(nav.move_to_first_child()); // root element
        let tv = nav.typed_value();
        assert!(
            matches!(tv, TypedValue::Value(_)),
            "simple-typed element should have typed value"
        );
    }

    // ── Test 4: typed_value() returns Absent for ElementOnly ──────────

    #[test]
    fn typed_value_absent_for_element_only() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root">
                    <xs:complexType>
                        <xs:sequence>
                            <xs:element name="child" type="xs:string"/>
                        </xs:sequence>
                    </xs:complexType>
                </xs:element>
            </xs:schema>"#,
        );
        let arena = Bump::new();
        let doc = build_doc("<root><child>hello</child></root>", &arena, &schema_set);

        let mut nav = doc.create_navigator();
        assert!(nav.move_to_first_child()); // root element
        assert!(nav.element_type_key().is_some());
        assert_eq!(
            nav.typed_value(),
            TypedValue::Absent,
            "ElementOnly complex type should produce Absent"
        );
    }

    // ── Test 5: complex type with simpleContent → typed_value works ──

    #[test]
    fn typed_value_simple_content() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root">
                    <xs:complexType>
                        <xs:simpleContent>
                            <xs:extension base="xs:integer">
                                <xs:attribute name="unit" type="xs:string"/>
                            </xs:extension>
                        </xs:simpleContent>
                    </xs:complexType>
                </xs:element>
            </xs:schema>"#,
        );
        let arena = Bump::new();
        let doc = build_doc(r#"<root unit="kg">42</root>"#, &arena, &schema_set);

        let mut nav = doc.create_navigator();
        assert!(nav.move_to_first_child()); // root element
        assert!(matches!(nav.element_type_key(), Some(TypeKey::Complex(_))));
        let binding = nav.schema_binding().unwrap();
        assert_eq!(
            binding.content_type,
            Some(ContentType::TextOnly),
            "simpleContent should have TextOnly content type"
        );
        let tv = nav.typed_value();
        assert!(
            matches!(tv, TypedValue::Value(_)),
            "simpleContent element should have typed_value"
        );
    }

    // ── Test 6: untyped document → binding_index=0, typed_value=Untyped

    #[test]
    fn untyped_document_no_bindings() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="other" type="xs:string"/>
            </xs:schema>"#,
        );
        let arena = Bump::new();
        // Element "root" not declared in schema — unresolved
        let doc = build_doc("<root>hello</root>", &arena, &schema_set);

        let mut nav = doc.create_navigator();
        assert!(nav.move_to_first_child()); // root element
                                            // Unknown element should still build, just no type binding
        assert_eq!(nav.typed_value(), TypedValue::Untyped);
    }

    // ── Test 6b: type_annotation() — the full TypeKey ────────────────

    /// `type_annotation()` is the XDM *type-name* property, so unlike
    /// `schema_type()` it must also report complex types — that is what
    /// `element(*, T)` matching with a complex `T` needs.
    #[test]
    fn type_annotation_reports_complex_simple_and_attribute_types() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root">
                    <xs:complexType>
                        <xs:sequence>
                            <xs:element name="leaf" type="xs:integer"/>
                        </xs:sequence>
                        <xs:attribute name="count" type="xs:integer"/>
                    </xs:complexType>
                </xs:element>
            </xs:schema>"#,
        );
        let arena = Bump::new();
        let doc = build_doc(
            r#"<root count="7"><leaf>42</leaf></root>"#,
            &arena,
            &schema_set,
        );

        // Document root: not an element or attribute → no annotation.
        let mut nav = doc.create_navigator();
        assert_eq!(nav.node_type(), DomNodeType::Root);
        assert_eq!(nav.type_annotation(), None);

        // <root>: annotated with a complex type, which `schema_type()`
        // cannot express.
        assert!(nav.move_to_first_child());
        assert!(
            matches!(nav.type_annotation(), Some(TypeKey::Complex(_))),
            "complex-typed element should report TypeKey::Complex, got {:?}",
            nav.type_annotation()
        );
        assert_eq!(
            nav.schema_type(),
            None,
            "a complex annotation has no simple-type projection"
        );

        // @count: a typed attribute; the annotation agrees with
        // `schema_type()`.
        let mut attr = nav.clone();
        assert!(attr.move_to_first_attribute());
        assert_eq!(attr.local_name(), "count");
        match attr.type_annotation() {
            Some(TypeKey::Simple(k)) => assert_eq!(Some(k), attr.schema_type()),
            other => panic!("expected a simple annotation on @count, got {:?}", other),
        }

        // <leaf>: a simple-typed element; same agreement.
        assert!(nav.move_to_first_child());
        assert_eq!(nav.local_name(), "leaf");
        match nav.type_annotation() {
            Some(TypeKey::Simple(k)) => assert_eq!(Some(k), nav.schema_type()),
            other => panic!("expected a simple annotation on <leaf>, got {:?}", other),
        }

        // The text child of <leaf> is not an element or attribute.
        let mut text = nav.clone();
        assert!(text.move_to_first_child());
        assert_eq!(text.node_type(), DomNodeType::Text);
        assert_eq!(text.type_annotation(), None);

        // The namespace cursor keeps `current` on its owning element, but is
        // not itself an annotated node.
        let mut ns = nav.clone();
        assert!(ns.move_to_first_namespace(NamespaceAxisScope::All));
        assert_eq!(ns.node_type(), DomNodeType::Namespace);
        assert_eq!(ns.type_annotation(), None);
    }

    /// A document whose content the schema does not declare carries no
    /// bindings, so every node is untyped.
    #[test]
    fn type_annotation_is_none_for_untyped_document() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="other" type="xs:string"/>
            </xs:schema>"#,
        );
        let arena = Bump::new();
        let doc = build_doc(r#"<root count="7">hello</root>"#, &arena, &schema_set);

        let mut nav = doc.create_navigator();
        assert!(nav.move_to_first_child()); // undeclared <root>
        assert_eq!(nav.type_annotation(), None);
        assert_eq!(nav.typed_value(), TypedValue::Untyped);
        assert!(nav.move_to_first_attribute());
        assert_eq!(nav.type_annotation(), None);
    }

    /// `RoXmlNavigator` is schema-unaware and keeps the trait's default body.
    #[test]
    fn type_annotation_is_none_for_roxml_navigator() {
        let parsed = roxmltree::Document::parse(r#"<root count="7">hello</root>"#).unwrap();
        let mut nav = crate::navigator::RoXmlNavigator::new(&parsed);
        assert_eq!(nav.type_annotation(), None);
        assert!(nav.move_to_first_child());
        assert_eq!(nav.type_annotation(), None);
        assert!(nav.move_to_first_attribute());
        assert_eq!(nav.type_annotation(), None);
    }

    // ── Test 7: xsi:type override ─────────────────────────────────────

    #[test]
    fn xsi_type_override() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root" type="xs:string"/>
            </xs:schema>"#,
        );
        let arena = Bump::new();
        let doc = build_doc(
            r#"<root xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
                   xmlns:xs="http://www.w3.org/2001/XMLSchema"
                   xsi:type="xs:integer">42</root>"#,
            &arena,
            &schema_set,
        );

        let mut nav = doc.create_navigator();
        assert!(nav.move_to_first_child()); // root element
        assert!(nav.element_type_key().is_some());
        // The type should be resolved (either xs:integer or the declared type)
        let tv = nav.typed_value();
        assert!(
            matches!(tv, TypedValue::Value(_)),
            "xsi:type override should produce typed value"
        );
    }

    // ── Test 8: xsi:nil → IS_NIL, typed_value() returns Nilled ───────

    #[test]
    fn xsi_nil_sets_flag() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root" type="xs:string" nillable="true"/>
            </xs:schema>"#,
        );
        let arena = Bump::new();
        let doc = build_doc(
            r#"<root xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
                   xsi:nil="true"/>"#,
            &arena,
            &schema_set,
        );

        let mut nav = doc.create_navigator();
        assert!(nav.move_to_first_child()); // root element
        assert_eq!(
            nav.typed_value(),
            TypedValue::Nilled,
            "nil element should return Nilled"
        );
    }

    // ── Test 9: NameTable sharing ────────────────────────────────────

    #[test]
    fn name_table_sharing() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root" type="xs:string"/>
            </xs:schema>"#,
        );
        let arena = Bump::new();
        let doc = build_doc("<root>hello</root>", &arena, &schema_set);

        // Builder and validator share schema_set.name_table
        assert!(std::ptr::eq(doc.names(), &schema_set.name_table));
    }

    // ── Test 10: element_type_key() returns both Simple and Complex ──

    #[test]
    fn element_type_key_simple_and_complex() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root">
                    <xs:complexType>
                        <xs:sequence>
                            <xs:element name="val" type="xs:integer"/>
                        </xs:sequence>
                    </xs:complexType>
                </xs:element>
            </xs:schema>"#,
        );
        let arena = Bump::new();
        let doc = build_doc("<root><val>10</val></root>", &arena, &schema_set);

        let mut nav = doc.create_navigator();
        assert!(nav.move_to_first_child()); // root element
        assert!(
            matches!(nav.element_type_key(), Some(TypeKey::Complex(_))),
            "root should have Complex type key"
        );

        // Navigate to <val>
        assert!(nav.move_to_first_child());
        assert!(
            matches!(nav.element_type_key(), Some(TypeKey::Simple(_))),
            "val should have Simple type key"
        );
    }

    // ── Test 11: default value on element → typed_value uses default ─

    #[test]
    fn default_value_on_empty_element() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root" type="xs:integer" default="99"/>
            </xs:schema>"#,
        );
        let arena = Bump::new();
        // Empty element — no text content, should use default "99"
        let doc = build_doc("<root/>", &arena, &schema_set);

        let mut nav = doc.create_navigator();
        assert!(nav.move_to_first_child()); // root element
        let tv = nav.typed_value();
        assert!(
            matches!(tv, TypedValue::Value(_)),
            "empty element with default should produce typed value"
        );
    }

    // ── Test 11b: fixed value on element → typed_value uses fixed ──

    #[test]
    fn fixed_value_on_empty_element() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root" type="xs:integer" fixed="42"/>
            </xs:schema>"#,
        );
        let arena = Bump::new();
        // Empty element — no text content, should use fixed "42"
        let doc = build_doc("<root/>", &arena, &schema_set);

        let mut nav = doc.create_navigator();
        assert!(nav.move_to_first_child()); // root element
        let tv = nav.typed_value();
        assert!(
            matches!(tv, TypedValue::Value(_)),
            "empty element with fixed value should produce typed value, got: {:?}",
            tv
        );
    }

    // ── Test 12: source span tracking works ───────────────────────────

    #[test]
    fn source_span_tracking() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root" type="xs:string"/>
            </xs:schema>"#,
        );
        let arena = Bump::new();
        let doc = build_typed_document(
            "<root>hello</root>".as_bytes(),
            &arena,
            &schema_set,
            BufferDocumentOptions::full(), // track_source_locations = true
        )
        .unwrap();

        // Source spans should be populated
        assert!(
            !doc.source_spans.is_empty(),
            "source spans should be recorded with full() options"
        );
    }

    // ── Test 13: comments and PIs preserved in typed document ────────

    #[test]
    fn comments_and_pis_preserved() {
        use crate::document::node::NodeType;

        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root">
                    <xs:complexType>
                        <xs:sequence>
                            <xs:element name="child" type="xs:string"/>
                        </xs:sequence>
                    </xs:complexType>
                </xs:element>
            </xs:schema>"#,
        );
        let arena = Bump::new();
        let xml = "<root><!-- before --><?p1 d1?><child>hi</child><!-- after --></root>";
        let doc = build_typed_document(
            xml.as_bytes(),
            &arena,
            &schema_set,
            BufferDocumentOptions::default(),
        )
        .unwrap();

        let mut comment_count = 0usize;
        let mut pi_count = 0usize;
        for i in 0..doc.nodes.len() {
            match doc.nodes.get(i).node_type() {
                NodeType::Comment => comment_count += 1,
                NodeType::ProcessingInstruction => pi_count += 1,
                _ => {}
            }
        }
        assert_eq!(comment_count, 2, "two comments should be preserved");
        assert_eq!(pi_count, 1, "one PI should be preserved");
    }

    // ── Fragment mode tests ──────────────────────────────────────────────

    #[test]
    fn fragment_set_node_binding_typed_value() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root" type="xs:integer"/>
            </xs:schema>"#,
        );
        let arena = Bump::new();
        let doc = build_typed_document(
            "<root>42</root>".as_bytes(),
            &arena,
            &schema_set,
            BufferDocumentOptions::fragment(),
        )
        .unwrap();

        let mut nav = doc.create_navigator();
        assert!(nav.move_to_first_child()); // root element
        assert!(nav.element_type_key().is_some());
        let tv = nav.typed_value();
        assert!(
            matches!(tv, TypedValue::Value(_)),
            "fragment typed_value should resolve, got: {:?}",
            tv
        );
    }

    #[test]
    fn fragment_schema_set_propagation() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root" type="xs:string"/>
            </xs:schema>"#,
        );
        let arena = Bump::new();
        let doc = build_typed_document(
            "<root>hello</root>".as_bytes(),
            &arena,
            &schema_set,
            BufferDocumentOptions::fragment(),
        )
        .unwrap();

        assert!(
            doc.schema_set().is_some(),
            "fragment document should carry schema_set reference"
        );
    }

    // ── CTA deferred attribute binding tests (XSD 1.1) ──────────────

    #[cfg(feature = "xsd11")]
    mod cta_deferred_bindings {
        use super::*;

        /// Schema where CTA switches type based on @kind, and the selected type
        /// declares an attribute with a specific type.
        const CTA_ATTR_SCHEMA: &str = r#"
            <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:complexType name="intType">
                    <xs:attribute name="kind" type="xs:string"/>
                    <xs:attribute name="val" type="xs:integer"/>
                </xs:complexType>
                <xs:complexType name="strType">
                    <xs:attribute name="kind" type="xs:string"/>
                    <xs:attribute name="val" type="xs:string"/>
                </xs:complexType>
                <xs:element name="data">
                    <xs:complexType>
                        <xs:attribute name="kind" type="xs:string"/>
                        <xs:attribute name="val" type="xs:string"/>
                    </xs:complexType>
                    <xs:alternative test="@kind='int'" type="intType"/>
                    <xs:alternative test="@kind='str'" type="strType"/>
                </xs:element>
            </xs:schema>"#;

        #[test]
        fn cta_selected_type_binds_attribute() {
            let schema_set = load_schema(CTA_ATTR_SCHEMA);
            let arena = Bump::new();
            let doc = build_doc(r#"<data kind="int" val="42"/>"#, &arena, &schema_set);

            let mut nav = doc.create_navigator();
            assert!(nav.move_to_first_child()); // <data>
                                                // Element should have intType binding
            assert!(nav.element_type_key().is_some());

            // Navigate to 'val' attribute — should have xs:integer type from intType
            assert!(nav.move_to_first_attribute());
            // Attributes are in document order: kind, val
            // Move to second attribute 'val'
            if nav.local_name() == "kind" {
                assert!(nav.move_to_next_attribute());
            }
            assert_eq!(nav.local_name(), "val");
            assert!(
                nav.element_type_key().is_some(),
                "deferred CTA attribute 'val' should have a type binding"
            );
        }

        #[test]
        fn cta_default_type_binds_attribute() {
            let schema_set = load_schema(CTA_ATTR_SCHEMA);
            let arena = Bump::new();
            // kind='str' selects strType — val should be xs:string
            let doc = build_doc(r#"<data kind="str" val="hello"/>"#, &arena, &schema_set);

            let mut nav = doc.create_navigator();
            assert!(nav.move_to_first_child()); // <data>
            assert!(nav.element_type_key().is_some());

            assert!(nav.move_to_first_attribute());
            if nav.local_name() == "kind" {
                assert!(nav.move_to_next_attribute());
            }
            assert_eq!(nav.local_name(), "val");
            assert!(
                nav.element_type_key().is_some(),
                "deferred CTA attribute 'val' should have a type binding for strType"
            );
        }

        #[test]
        fn cta_multiple_deferred_attributes_ordering() {
            // Schema where CTA type has multiple typed attributes
            let schema = r#"
                <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                    <xs:complexType name="fullType">
                        <xs:attribute name="kind" type="xs:string"/>
                        <xs:attribute name="x" type="xs:integer"/>
                        <xs:attribute name="y" type="xs:integer"/>
                    </xs:complexType>
                    <xs:element name="point">
                        <xs:complexType>
                            <xs:attribute name="kind" type="xs:string"/>
                            <xs:attribute name="x" type="xs:string"/>
                            <xs:attribute name="y" type="xs:string"/>
                        </xs:complexType>
                        <xs:alternative test="@kind='full'" type="fullType"/>
                    </xs:element>
                </xs:schema>"#;

            let schema_set = load_schema(schema);
            let arena = Bump::new();
            let doc = build_doc(r#"<point kind="full" x="10" y="20"/>"#, &arena, &schema_set);

            let mut nav = doc.create_navigator();
            assert!(nav.move_to_first_child()); // <point>

            // Check all attributes have bindings
            assert!(nav.move_to_first_attribute());
            let mut bound_count = 0;
            loop {
                if nav.element_type_key().is_some() {
                    bound_count += 1;
                }
                if !nav.move_to_next_attribute() {
                    break;
                }
            }
            assert!(
                bound_count >= 3,
                "all 3 attributes (kind, x, y) should have type bindings, got {}",
                bound_count
            );
        }

        #[test]
        fn cta_nested_elements_no_cross_leakage() {
            // Schema with CTA on two nested elements
            let schema = r#"
                <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                    <xs:complexType name="outerAlt">
                        <xs:sequence>
                            <xs:element ref="inner"/>
                        </xs:sequence>
                        <xs:attribute name="kind" type="xs:string"/>
                        <xs:attribute name="outerAttr" type="xs:integer"/>
                    </xs:complexType>
                    <xs:complexType name="innerAlt">
                        <xs:attribute name="kind" type="xs:string"/>
                        <xs:attribute name="innerAttr" type="xs:integer"/>
                    </xs:complexType>
                    <xs:element name="inner">
                        <xs:complexType>
                            <xs:attribute name="kind" type="xs:string"/>
                            <xs:attribute name="innerAttr" type="xs:string"/>
                        </xs:complexType>
                        <xs:alternative test="@kind='alt'" type="innerAlt"/>
                    </xs:element>
                    <xs:element name="outer">
                        <xs:complexType>
                            <xs:sequence>
                                <xs:element ref="inner"/>
                            </xs:sequence>
                            <xs:attribute name="kind" type="xs:string"/>
                            <xs:attribute name="outerAttr" type="xs:string"/>
                        </xs:complexType>
                        <xs:alternative test="@kind='alt'" type="outerAlt"/>
                    </xs:element>
                </xs:schema>"#;

            let schema_set = load_schema(schema);
            let arena = Bump::new();
            let doc = build_doc(
                r#"<outer kind="alt" outerAttr="99"><inner kind="alt" innerAttr="42"/></outer>"#,
                &arena,
                &schema_set,
            );

            let mut nav = doc.create_navigator();
            assert!(nav.move_to_first_child()); // <outer>
            assert!(
                nav.element_type_key().is_some(),
                "outer should have type binding"
            );

            // Check outerAttr has binding
            assert!(nav.move_to_first_attribute());
            let mut found_outer_attr = false;
            loop {
                if nav.local_name() == "outerAttr" {
                    assert!(
                        nav.element_type_key().is_some(),
                        "outerAttr should have type binding"
                    );
                    found_outer_attr = true;
                }
                if !nav.move_to_next_attribute() {
                    break;
                }
            }
            assert!(found_outer_attr, "should find outerAttr");

            // Navigate to <inner>
            nav.move_to_parent();
            assert!(nav.move_to_first_child()); // <inner>
            assert!(
                nav.element_type_key().is_some(),
                "inner should have type binding"
            );

            // Check innerAttr has binding
            assert!(nav.move_to_first_attribute());
            let mut found_inner_attr = false;
            loop {
                if nav.local_name() == "innerAttr" {
                    assert!(
                        nav.element_type_key().is_some(),
                        "innerAttr should have type binding"
                    );
                    found_inner_attr = true;
                }
                if !nav.move_to_next_attribute() {
                    break;
                }
            }
            assert!(found_inner_attr, "should find innerAttr");
        }

        #[test]
        fn cta_simple_type_selection_no_panic() {
            // CTA selects a simple type — the element then has no attribute
            // declarations. Deferred attributes should produce empty results
            // (no bindings) rather than triggering an InternalError.
            let schema = r#"
                <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                    <xs:element name="data">
                        <xs:complexType>
                            <xs:attribute name="kind" type="xs:string"/>
                            <xs:attribute name="val" type="xs:string"/>
                        </xs:complexType>
                        <xs:alternative test="@kind='simple'" type="xs:string"/>
                    </xs:element>
                </xs:schema>"#;

            let schema_set = load_schema(schema);
            let arena = Bump::new();
            // kind='simple' triggers CTA → xs:string (a simple type).
            // Both attributes were deferred; revalidation must not panic.
            let result = build_typed_document(
                r#"<data kind="simple" val="hello"/>"#.as_bytes(),
                &arena,
                &schema_set,
                BufferDocumentOptions::default(),
            );
            assert!(
                result.is_ok(),
                "CTA selecting simple type should not cause InternalError, got: {:?}",
                result.err()
            );
        }
    }

    // ── `BufferDocument::has_type_annotations` ────────────────────────

    /// Walks the whole tree and reports whether any element or attribute node
    /// carries a type annotation — the O(nodes) answer the O(1) accessor must
    /// agree with exactly.
    fn walk_finds_an_annotation(doc: &BufferDocument<'_>) -> bool {
        fn visit(nav: &crate::document::navigator::BufferDocNavigator<'_>) -> bool {
            if matches!(
                nav.node_type(),
                DomNodeType::Element | DomNodeType::Attribute
            ) && nav.type_annotation().is_some()
            {
                return true;
            }
            if nav.node_type() == DomNodeType::Element {
                let mut attr = nav.clone();
                if attr.move_to_first_attribute() {
                    loop {
                        if attr.type_annotation().is_some() {
                            return true;
                        }
                        if !attr.move_to_next_attribute() {
                            break;
                        }
                    }
                }
            }
            let mut child = nav.clone();
            if child.move_to_first_child() {
                loop {
                    if visit(&child) {
                        return true;
                    }
                    if !child.move_to_next_sibling() {
                        break;
                    }
                }
            }
            false
        }
        visit(&doc.create_navigator())
    }

    fn annotation_schema() -> SchemaSet {
        load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root">
                    <xs:complexType>
                        <xs:sequence>
                            <xs:element name="n" type="xs:integer"/>
                        </xs:sequence>
                        <xs:attribute name="count" type="xs:integer"/>
                    </xs:complexType>
                </xs:element>
            </xs:schema>"#,
        )
    }

    #[test]
    fn has_type_annotations_is_false_for_an_unvalidated_document() {
        let arena = Bump::new();
        let names = crate::namespace::NameTable::new();
        let doc = BufferDocument::from_reader_default(
            r#"<root count="1"><n>2</n><!--c--><?pi t?></root>"#.as_bytes(),
            &arena,
            &names,
        )
        .expect("parses");
        assert!(!doc.has_type_annotations());
        assert!(!walk_finds_an_annotation(&doc));
    }

    #[test]
    fn has_type_annotations_is_true_for_a_typed_document() {
        let schema_set = annotation_schema();
        let arena = Bump::new();
        let doc = build_doc(r#"<root count="1"><n>2</n></root>"#, &arena, &schema_set);
        assert!(doc.has_type_annotations());
        assert!(walk_finds_an_annotation(&doc));
    }

    #[test]
    fn has_type_annotations_follows_the_copys_annotation_mode() {
        use crate::document::{Annotations, CopyOptions};

        let schema_set = annotation_schema();
        let arena = Bump::new();
        let source = build_doc(r#"<root count="1"><n>2</n></root>"#, &arena, &schema_set);
        assert!(source.has_type_annotations());

        let mut node = source.create_navigator();
        assert!(node.move_to_first_child());

        for (annotations, expected) in [(Annotations::Preserve, true), (Annotations::Strip, false)]
        {
            let mut builder = crate::document::BufferDocumentBuilder::new(
                &arena,
                &schema_set.name_table,
                Some(&schema_set),
                BufferDocumentOptions::default(),
            )
            .expect("a builder");
            builder.start_element("out", "", "", &[]).expect("starts");
            builder.end_of_attributes();
            builder
                .copy_subtree(
                    &node,
                    CopyOptions {
                        annotations,
                        ..CopyOptions::default()
                    },
                )
                .expect("copies");
            builder.end_element().expect("ends");
            let copy = builder.finalize().expect("finalizes");
            assert_eq!(
                copy.has_type_annotations(),
                expected,
                "{annotations:?} copy"
            );
            assert_eq!(
                walk_finds_an_annotation(&copy),
                expected,
                "{annotations:?} copy, by walk"
            );
        }
    }

    #[test]
    fn has_type_annotations_agrees_with_a_full_walk() {
        let schema_set = annotation_schema();
        let names = crate::namespace::NameTable::new();
        for xml in [
            "<root/>",
            r#"<root count="1"><n>2</n></root>"#,
            "<root><n>0</n></root>",
            r#"<root count="7"><n>-3</n></root>"#,
        ] {
            // Unvalidated: never annotated.
            let arena = Bump::new();
            let plain = BufferDocument::from_reader_default(xml.as_bytes(), &arena, &names)
                .expect("parses");
            assert_eq!(
                plain.has_type_annotations(),
                walk_finds_an_annotation(&plain),
                "untyped {xml}"
            );
            assert!(!plain.has_type_annotations(), "untyped {xml}");

            // The same markup through the validating builder. `<root/>` is
            // invalid against the schema, so it exercises the path where
            // validation fails and only some nodes get a binding.
            let arena = Bump::new();
            if let Ok(typed) = build_typed_document(
                xml.as_bytes(),
                &arena,
                &schema_set,
                BufferDocumentOptions::default(),
            ) {
                assert_eq!(
                    typed.has_type_annotations(),
                    walk_finds_an_annotation(&typed),
                    "typed {xml}"
                );
            }
        }
    }

    // ── Schema-typed xs:ID attributes are is-id nodes ────────────────

    /// F&O §15.5.2: "E has an attribute node whose is-id property is true and
    /// whose typed value is equal to V". A schema-validated `xs:ID` attribute
    /// has that property, and it is the annotation the validator attaches that
    /// says which attributes do.
    #[test]
    fn a_schema_typed_id_attribute_is_findable() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root">
                    <xs:complexType>
                        <xs:sequence>
                            <xs:element name="item" maxOccurs="unbounded">
                                <xs:complexType>
                                    <xs:attribute name="key" type="xs:ID"/>
                                </xs:complexType>
                            </xs:element>
                        </xs:sequence>
                    </xs:complexType>
                </xs:element>
            </xs:schema>"#,
        );
        let arena = Bump::new();
        let doc = build_doc(
            r#"<root><item key="a"/><item key=" b "/></root>"#,
            &arena,
            &schema_set,
        );

        let found = doc.get_element_by_id("a").expect("the xs:ID is indexed");
        let mut nav = doc.create_navigator_at(found);
        assert_eq!(nav.local_name(), "item");
        assert!(nav.move_to_first_attribute());
        assert_eq!(nav.value(), "a");

        // The key is whitespace-normalized, and so is the attribute's string
        // value: a valid attribute's is its [schema normalized value] (XDM 1.0
        // §6.3.4).
        let padded = doc.get_element_by_id("b").expect("a padded xs:ID too");
        let mut nav = doc.create_navigator_at(padded);
        assert!(nav.move_to_first_attribute());
        assert_eq!(nav.value(), "b", "the schema normalized value");
    }

    /// An attribute of an ordinary type is not an is-id node.
    #[test]
    fn a_plain_attribute_is_not_indexed_as_an_id() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root">
                    <xs:complexType>
                        <xs:attribute name="key" type="xs:NCName"/>
                    </xs:complexType>
                </xs:element>
            </xs:schema>"#,
        );
        let arena = Bump::new();
        let doc = build_doc(r#"<root key="a"/>"#, &arena, &schema_set);
        assert_eq!(doc.get_element_by_id("a"), None);
    }
}
