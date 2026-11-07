//! Main XSD parser event loop
//!
//! This module provides the main parser that processes XSD documents using
//! a frame-based state machine. Each XSD element type is handled by a
//! corresponding frame that validates structure and collects content.
//!
//! # Architecture
//!
//! The parser uses:
//! - `TrackedReader` for XML parsing with byte position tracking
//! - `NamespaceContext` for scoped namespace management
//! - Frame stack for nested element handling
//! - `create_frame` factory for frame instantiation
//!
//! # Example
//!
//! ```
//! use xsd_schema::parser::parse::parse_schema;
//! use xsd_schema::SchemaSet;
//!
//! let mut schema_set = SchemaSet::new();
//! let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
//!     <xs:element name="root" type="xs:string"/>
//! </xs:schema>"#;
//!
//! let doc_id = parse_schema(xsd.as_bytes(), "test.xsd", &mut schema_set)
//!     .expect("parse failed");
//! assert_eq!(doc_id, 0);
//! ```

use std::collections::HashSet;

use quick_xml::events::Event;

use crate::error::{SchemaError, SchemaResult};
use crate::ids::{DocumentId, NameId};
use crate::namespace::{is_ncname, NameTable, NamespaceContext, XS_NAMESPACE};
use crate::parser::assemble::assemble_schema;
use crate::parser::attrs::{categorize_attributes, parse_attributes_in, AttributeMap};
use crate::parser::frames::{
    create_frame, create_frame_recovering, xsd_names, Frame, FrameResult, SchemaFrameResult,
    SkipFrame,
};
use crate::parser::location::{SourceLocation, SourceMap, SourceMapStorage, SourceRef, SourceSpan};
use crate::parser::reader::{split_qname, TrackedReader};
use crate::parser::structure::{
    validate_attribute_group_structure, validate_attribute_structure,
    validate_complex_type_structure, validate_element_structure, validate_extension_structure,
    validate_group_structure, validate_import_structure, validate_include_structure,
    validate_key_unique_structure, validate_keyref_structure, validate_notation_structure,
    validate_redefine_structure, validate_schema_structure, validate_simple_type_structure,
    validate_xsd_version_attribute, validate_xsd_version_element, ValidationContext,
};
use crate::schema::annotation::ForeignAttribute;
use crate::schema::model::XsdVersion;
use crate::SchemaSet;

/// The default nesting limit of schemas: [`ParserConfig::max_depth`] and
/// [`SchemaSet::max_depth`](crate::SchemaSet::max_depth).
///
/// The deepest schema of the W3C XSD test suite nests its elements 16 deep,
/// the deepest of 1,700 real-world WSDL / XSD files 25 deep; the limit leaves
/// a margin of five over that, and keeps the recursion of every schema-time
/// pass well inside a 2 MiB thread stack (the default of spawned threads),
/// in debug builds too.
pub const DEFAULT_MAX_DEPTH: usize = 128;

/// Parser configuration options
#[derive(Debug, Clone)]
pub struct ParserConfig {
    /// Whether to recover from errors and continue parsing
    pub error_recovery: bool,
    /// Whether to collect foreign attributes
    pub collect_foreign_attributes: bool,
    /// The nesting limit of schemas (`0` = unlimited); default
    /// [`DEFAULT_MAX_DEPTH`].
    ///
    /// A schema document whose elements nest deeper is refused when it is
    /// read, with a [`SchemaError::LimitExceeded`] (limit
    /// [`SchemaLimit::ElementDepth`]) at the element that breaks the limit:
    /// `Schema limit ParserConfig::max_depth exceeded at main.xsd:1:2177:
    /// element depth 129, the limit is 128`; the `xs:schema` element is at
    /// depth one. [`SchemaSetBuilder::compile`](crate::SchemaSetBuilder::compile)
    /// and [`load_and_process_schema`](crate::load_and_process_schema) also
    /// bound the nesting of the schema's components through references with
    /// it (see [`SchemaSet::max_depth`](crate::SchemaSet::max_depth)).
    /// Together they bound the recursion of the schema-time passes, so that
    /// a schema nested deeply enough to overflow the stack is refused
    /// instead. Raising the limit needs a larger stack for such schemas;
    /// `0` turns this protection off — the crate's own recursion bounds then
    /// refuse only some shapes, and a deeply nested schema can overflow the
    /// stack and abort the process.
    ///
    /// [`SchemaError::LimitExceeded`]: crate::SchemaError::LimitExceeded
    /// [`SchemaLimit::ElementDepth`]: crate::SchemaLimit::ElementDepth
    pub max_depth: usize,
    /// XSD version mode (1.0 or 1.1).
    /// Derived from `SchemaSet.xsd_version` in `parse_schema_with_config`.
    pub(crate) xsd_version: XsdVersion,
}

impl Default for ParserConfig {
    fn default() -> Self {
        Self {
            error_recovery: true,
            collect_foreign_attributes: true,
            max_depth: DEFAULT_MAX_DEPTH,
            xsd_version: XsdVersion::V1_0,
        }
    }
}

/// Parser state during schema parsing
struct ParserState<'a, 'b, 'c> {
    /// Namespace context for prefix resolution
    ns_context: NamespaceContext<'a>,
    /// Stack of parser frames
    frame_stack: Vec<Box<dyn Frame>>,
    /// Current document ID
    doc_id: DocumentId,
    /// Errors collected during parsing
    errors: Vec<SchemaError>,
    /// Per frame on `frame_stack`: `errors.len()` when it was pushed
    error_marks: Vec<usize>,
    /// Parser configuration
    config: &'b ParserConfig,
    /// XSD namespace ID (cached)
    xsd_ns_id: Option<NameId>,
    /// Source map for location resolution
    source_map: &'c SourceMap,
    /// Completed root schema result (set when root frame finishes)
    root_schema: Option<SchemaFrameResult>,
    /// Collected xs:ID values for document-level uniqueness checking
    id_values: HashSet<String>,
    /// True when the root xs:schema element has a vc: version condition that
    /// excludes this document; all children of xs:schema are then skipped.
    vc_schema_excluded: bool,
    /// Chameleon namespace to adopt (§4.2.3 clause 2.3) when this document
    /// is being included by a schema with a target namespace and itself has
    /// neither a `targetNamespace` attribute nor an explicit `xmlns` default.
    /// Applied to the root `<xs:schema>` scope so that unqualified QName
    /// references inside the included document resolve to the includer's
    /// target namespace (matching how top-level definitions are re-namespaced).
    chameleon_namespace: Option<NameId>,
    /// The version of XML the document is read with (its declaration).
    xml_version: crate::xml::XmlVersion,
    /// The namespace bindings at each `<xs:element>` / `<xs:attribute>` that
    /// carries a `default` or `fixed` value, by start offset (see
    /// `SchemaSet::value_constraint_namespaces`).
    value_constraint_namespaces: Vec<(usize, crate::namespace::NamespaceContextSnapshot)>,
}

impl<'a, 'b, 'c> ParserState<'a, 'b, 'c> {
    fn new(
        name_table: &'a mut NameTable,
        doc_id: DocumentId,
        config: &'b ParserConfig,
        source_map: &'c SourceMap,
        chameleon_namespace: Option<NameId>,
    ) -> Self {
        let ns_context = NamespaceContext::new(name_table);
        Self {
            ns_context,
            frame_stack: Vec::new(),
            doc_id,
            errors: Vec::new(),
            error_marks: Vec::new(),
            config,
            xsd_ns_id: None,
            source_map,
            root_schema: None,
            id_values: HashSet::new(),
            vc_schema_excluded: false,
            chameleon_namespace,
            xml_version: crate::xml::XmlVersion::V1_0,
            value_constraint_namespaces: Vec::new(),
        }
    }

    /// The document's XML version, as quick-xml's normalization takes it.
    fn quick_xml_version(&self) -> quick_xml::XmlVersion {
        match self.xml_version {
            crate::xml::XmlVersion::V1_1 => quick_xml::XmlVersion::Explicit1_1,
            _ => quick_xml::XmlVersion::Implicit1_0,
        }
    }

    /// Get the XSD namespace ID, caching it for efficiency
    fn get_xsd_ns_id(&mut self) -> Option<NameId> {
        if self.xsd_ns_id.is_none() {
            self.xsd_ns_id = self.ns_context.name_table().get(XS_NAMESPACE);
        }
        self.xsd_ns_id
    }

    /// Check if a namespace URI is the XSD namespace
    fn is_in_xsd_namespace(&mut self, namespace: Option<NameId>) -> bool {
        match (namespace, self.get_xsd_ns_id()) {
            (Some(ns), Some(xsd_ns)) => ns == xsd_ns,
            (None, _) => false, // Unqualified elements are not XSD elements
            _ => false,
        }
    }

    /// Push a namespace scope
    fn push_scope(&mut self) {
        self.ns_context.push_scope();
    }

    /// Pop a namespace scope
    fn pop_scope(&mut self) {
        self.ns_context.pop_scope();
    }

    /// Get current frame
    fn current_frame(&self) -> Option<&dyn Frame> {
        self.frame_stack.last().map(|b| b.as_ref())
    }

    /// Get current frame mutably
    fn current_frame_mut(&mut self) -> Option<&mut Box<dyn Frame>> {
        self.frame_stack.last_mut()
    }

    /// Add an error
    fn add_error(&mut self, error: SchemaError) {
        self.errors.push(error);
    }

    /// In error-recovery mode, collect the error and continue; otherwise fail.
    fn recover_or_fail(&mut self, error: SchemaError) -> SchemaResult<()> {
        if self.config.error_recovery {
            self.add_error(error);
            Ok(())
        } else {
            Err(error)
        }
    }

    /// Create a source reference for the given span
    fn source_ref(&self, span: SourceSpan) -> SourceRef {
        SourceRef::new(self.doc_id, span)
    }

    /// Create validation context for structural checks.
    /// Elements are top-level if their parent frame reports `children_are_top_level`
    /// (schema, redefine, and override frames).
    fn validation_context(&self, source: Option<SourceRef>) -> ValidationContext {
        let is_top_level = self
            .frame_stack
            .last()
            .map(|f| f.children_are_top_level())
            .unwrap_or(false);
        // Walk the frame stack to detect a `<complexType>` lexical ancestor.
        // Stop walking when we hit a frame whose children are top-level
        // (schema/redefine/override) — there's nothing more to look at.
        let inside_complex_type = self
            .frame_stack
            .iter()
            .rev()
            .any(|f| f.children_inside_complex_type());
        ValidationContext {
            xsd_version: self.config.xsd_version,
            is_top_level,
            inside_complex_type,
            source,
        }
    }
}

/// Parse an XSD schema document
///
/// This is the main entry point for parsing XSD documents.
///
/// # Arguments
///
/// * `xml` - Raw XML bytes of the schema document
/// * `base_uri` - Base URI for this document (for error messages and include resolution)
/// * `schema_set` - Schema set to add the parsed document to
///
/// # Returns
///
/// The document ID of the parsed schema, or an error if parsing failed.
pub fn parse_schema(
    xml: &[u8],
    base_uri: &str,
    schema_set: &mut SchemaSet,
) -> SchemaResult<DocumentId> {
    let config = ParserConfig::default();
    parse_schema_with_config(xml, base_uri, schema_set, &config)
}

/// Parse an XSD schema document with custom configuration.
///
/// The XSD version is always derived from `schema_set.xsd_version`,
/// regardless of what `config.xsd_version` contains.
pub fn parse_schema_with_config(
    xml: &[u8],
    base_uri: &str,
    schema_set: &mut SchemaSet,
    config: &ParserConfig,
) -> SchemaResult<DocumentId> {
    parse_schema_with_chameleon(xml, base_uri, schema_set, config, None)
}

/// Parse an XSD schema document with chameleon namespace support.
///
/// If `chameleon_namespace` is `Some` and the parsed document has no
/// `targetNamespace`, the chameleon namespace is adopted per §4.2.3
/// clause 2.3 (chameleon include pre-processing).
pub fn parse_schema_with_chameleon(
    xml: &[u8],
    base_uri: &str,
    schema_set: &mut SchemaSet,
    config: &ParserConfig,
    chameleon_namespace: Option<NameId>,
) -> SchemaResult<DocumentId> {
    // Override parser version from the single source of truth
    let mut config = config.clone();
    config.xsd_version = schema_set.xsd_version;

    // Create source map - keep local reference for location resolution during parsing
    let source_text = String::from_utf8_lossy(xml).into_owned();
    let source_map = SourceMap::new(base_uri.to_string(), source_text);

    // Pre-assign the document ID: the index this document will take in
    // `documents`. Every `SourceRef` of the document carries it, and
    // `documents[doc_id]` is how the document is found again (for example the
    // target namespace a wildcard's `##other` refers to). It is not taken from
    // `source_maps`, which keeps nothing once `SourceMapStorage::drop_all`
    // has run.
    let doc_id = schema_set.documents.len() as DocumentId;

    // Create parser state with reference to source_map
    let mut state = ParserState::new(
        &mut schema_set.name_table,
        doc_id,
        &config,
        &source_map,
        chameleon_namespace,
    );

    // Create XML reader; the nesting limit is checked on the raw markup.
    let mut reader = if config.max_depth == 0 {
        TrackedReader::from_bytes(xml)
    } else {
        TrackedReader::from_bytes_with_limits(
            xml,
            &crate::xml::ParseLimits::none().with_max_depth(config.max_depth),
        )
    };
    let mut buf = Vec::new();

    // Track if we've seen the root schema element
    let mut seen_root = false;

    // Main event loop. The reader clears `buf` itself.
    loop {
        let tracked_event = match reader.read_event(&mut buf) {
            Ok(event) => event,
            // A well-formedness error names its byte position: locate it.
            Err(e) => {
                return Err(match reader.error_position() {
                    Some(pos) => e.with_location(source_map.locate(pos)),
                    None => e,
                })
            }
        };
        let span = tracked_event.span;
        state.xml_version = reader.xml_version();

        match tracked_event.event {
            Event::Start(ref e) => {
                handle_start_element(&mut state, e, span, &mut seen_root)?;
            }
            Event::Empty(ref e) => {
                // Empty elements are treated as Start + End
                handle_start_element(&mut state, e, span, &mut seen_root)?;
                handle_end_element(&mut state, span)?;
            }
            Event::End(_) => {
                handle_end_element(&mut state, span)?;
            }
            Event::Text(ref e) => {
                handle_text(&mut state, e, span)?;
            }
            Event::GeneralRef(ref e) => {
                handle_general_ref(&mut state, e, span)?;
            }
            Event::CData(ref e) => {
                handle_cdata(&mut state, e, span)?;
            }
            Event::Comment(_) => {
                // Ignore comments
            }
            Event::PI(_) => {
                // Ignore processing instructions
            }
            Event::Decl(_) => {
                // Ignore XML declaration
            }
            Event::DocType(_) => {
                // Ignore DOCTYPE
            }
            Event::Eof => break,
        }
    }

    // Check for incomplete parsing
    if !state.frame_stack.is_empty() {
        return Err(SchemaError::structural(
            "src-resolve",
            "Schema document ended with unclosed elements",
            None,
        ));
    }

    // Store any collected parsing errors on the schema set so they can be
    // surfaced later (e.g. when process_loaded_schemas runs).
    let parsing_errors = std::mem::take(&mut state.errors);

    let mut root_schema = state
        .root_schema
        .take()
        .ok_or_else(|| SchemaError::internal("No schema result produced during parsing"))?;
    let value_constraint_namespaces = std::mem::take(&mut state.value_constraint_namespaces);
    drop(state);

    // Record the declared targetNamespace before chameleon adoption.
    let declared_target_namespace = root_schema.target_namespace;

    // src-include §4.2.3 clause 2.1: when included via `<xs:include>` from a
    // schema with a `targetNamespace`, the included document's declared
    // `targetNamespace`, when present, must equal the includer's.
    if let Some(includer_ns) = chameleon_namespace {
        if let Some(declared) = declared_target_namespace {
            if declared != includer_ns {
                return Err(SchemaError::structural(
                    "src-include",
                    format!(
                        "Included schema's targetNamespace '{}' does not match \
                         including schema's targetNamespace '{}'",
                        schema_set.name_table.resolve(declared),
                        schema_set.name_table.resolve(includer_ns),
                    ),
                    None,
                ));
            }
        }
    }

    // Chameleon pre-processing (§4.2.3 clause 2.3): if the parsed document
    // has no targetNamespace and the includer specifies one, adopt it.
    if root_schema.target_namespace.is_none() {
        if let Some(ns) = chameleon_namespace {
            root_schema.target_namespace = Some(ns);
        }
    }

    // Add the source map to storage now that parsing is complete; assembly
    // locates its errors through it. A storage that retains source maps
    // keeps them parallel to `documents`, so the map lands at `doc_id`.
    let retains_maps = !matches!(schema_set.source_maps, SourceMapStorage::None);
    let added_id = schema_set.source_maps.add(source_map);
    debug_assert!(
        !retains_maps || added_id == doc_id,
        "Document ID mismatch: source map {added_id}, document {doc_id}"
    );

    // A document that fails to assemble must leave no trace: the assembler
    // has already withdrawn its components, and its source map goes too.
    // A source map left behind would hand the next document an id one past
    // the end of `documents` (`source_maps` and `documents` are parallel
    // tables indexed by `DocumentId`).
    let mut doc = match assemble_schema(schema_set, doc_id, base_uri, root_schema) {
        Ok(doc) => doc,
        Err(e) => {
            schema_set.source_maps.truncate(doc_id as usize);
            return Err(e);
        }
    };
    doc.declared_target_namespace = declared_target_namespace;
    schema_set.documents.push(doc);
    schema_set.record_value_constraint_namespaces(doc_id, value_constraint_namespaces);
    // Deferred (error-recovery) parsing errors are kept only for a document
    // that made it into the set.
    schema_set.parsing_errors.extend(parsing_errors);

    Ok(doc_id)
}

/// Validate element-specific structural constraints
///
/// Dispatches to the appropriate validation function based on element name.
/// This enforces constraints like name/ref exclusivity, required attributes, etc.
fn validate_element_attributes(
    local_name: &str,
    attrs: &AttributeMap,
    name_table: &NameTable,
    ctx: &ValidationContext,
) -> SchemaResult<()> {
    match local_name {
        xsd_names::ELEMENT => validate_element_structure(attrs, name_table, ctx),
        xsd_names::ATTRIBUTE => validate_attribute_structure(attrs, name_table, ctx),
        xsd_names::SIMPLE_TYPE => validate_simple_type_structure(attrs, name_table, ctx),
        xsd_names::COMPLEX_TYPE => validate_complex_type_structure(attrs, name_table, ctx),
        xsd_names::GROUP => validate_group_structure(attrs, name_table, ctx),
        xsd_names::ATTRIBUTE_GROUP => validate_attribute_group_structure(attrs, name_table, ctx),
        xsd_names::NOTATION => validate_notation_structure(attrs, name_table, ctx),
        xsd_names::INCLUDE => validate_include_structure(attrs, name_table),
        xsd_names::IMPORT => validate_import_structure(attrs, name_table),
        xsd_names::REDEFINE => validate_redefine_structure(attrs, name_table),
        xsd_names::SCHEMA => validate_schema_structure(attrs, name_table),
        xsd_names::KEY | xsd_names::UNIQUE => validate_key_unique_structure(attrs, name_table),
        xsd_names::KEYREF => validate_keyref_structure(attrs, name_table),
        xsd_names::EXTENSION => validate_extension_structure(attrs, name_table),
        // Note: restriction and list/union validation requires child info (has_inline_type),
        // so they're validated at frame finish time, not here
        _ => Ok(()),
    }
}

fn intern_attribute_values(local_name: &str, attrs: &AttributeMap, name_table: &mut NameTable) {
    fn add_if_present(attrs: &AttributeMap, name_table: &mut NameTable, attr: &str) {
        if let Some(value) = attrs.get_value_by_name(name_table, attr) {
            name_table.add(value);
        }
    }

    match local_name {
        xsd_names::SCHEMA => {
            add_if_present(attrs, name_table, "targetNamespace");
            add_if_present(attrs, name_table, "defaultAttributes");
        }
        xsd_names::SIMPLE_TYPE | xsd_names::COMPLEX_TYPE => {
            add_if_present(attrs, name_table, "name");
        }
        xsd_names::ELEMENT | xsd_names::ATTRIBUTE => {
            add_if_present(attrs, name_table, "name");
            add_if_present(attrs, name_table, "targetNamespace");
        }
        xsd_names::GROUP | xsd_names::ATTRIBUTE_GROUP | xsd_names::NOTATION => {
            add_if_present(attrs, name_table, "name");
        }
        xsd_names::KEY | xsd_names::KEYREF | xsd_names::UNIQUE => {
            add_if_present(attrs, name_table, "name");
        }
        _ => {}
    }
}

/// Handle a start element event
fn handle_start_element(
    state: &mut ParserState,
    element: &quick_xml::events::BytesStart,
    span: SourceSpan,
    seen_root: &mut bool,
) -> SchemaResult<()> {
    // Push namespace scope for this element
    state.push_scope();

    // Parse element name
    let name = element.name();
    let name_bytes = name.as_ref();
    let (local_name_bytes, prefix_bytes) = split_qname(name_bytes);

    let local_name = std::str::from_utf8(local_name_bytes).map_err(|e| {
        SchemaError::xml(
            format!("Invalid UTF-8 in element name: {}", e),
            Some(state.source_ref(span).to_location(state.source_map)),
        )
    })?;

    // First, process namespace declarations from attributes
    for attr_result in element.attributes() {
        let attr =
            attr_result.map_err(|e| SchemaError::xml(format!("Attribute error: {}", e), None))?;

        let attr_name = attr.key.as_ref();
        // Normalized as the reader normalized it for its namespace checks.
        let attr_value = attr
            .normalized_value(state.quick_xml_version())
            .map_err(|e| SchemaError::xml(format!("Attribute value error: {}", e), None))?;

        // Check for xmlns declarations
        if attr_name == b"xmlns" {
            // Default namespace
            state.ns_context.add_namespace("", &attr_value);
        } else if attr_name.starts_with(b"xmlns:") {
            // Prefixed namespace
            let prefix = std::str::from_utf8(&attr_name[6..]).unwrap_or("");
            state.ns_context.add_namespace(prefix, &attr_value);
        }
    }

    // Now resolve the element's namespace
    let element_ns = if let Some(prefix) = prefix_bytes {
        let prefix_str = std::str::from_utf8(prefix).unwrap_or("");
        state.ns_context.lookup_namespace(prefix_str)
    } else {
        state.ns_context.default_namespace()
    };

    // Check if this is the root schema element
    if !*seen_root {
        *seen_root = true;

        // Must be xs:schema
        if local_name != xsd_names::SCHEMA || !state.is_in_xsd_namespace(element_ns) {
            return Err(SchemaError::structural(
                "sch-props-correct",
                format!("Root element must be xs:schema, found '{}'", local_name),
                Some(state.source_map.locate(span.start)),
            ));
        }
    }

    // Parse and categorize attributes
    let source_ref = Some(state.source_ref(span));
    let version = state.quick_xml_version();
    let parsed_attrs = parse_attributes_in(
        element.attributes(),
        &mut state.ns_context,
        source_ref.clone(),
        version,
    )?;
    // sch-props-correct: attributes on XSD-namespace elements must be
    // unqualified — an explicit `xsd:`/`xs:` prefix on an XSD attribute
    // (e.g. `xsd:targetNamespace`) is not the same lexical attribute as the
    // unqualified one, and the schema-for-schemas content model only
    // declares the unqualified attribute. addB070a (test64756.xsd):
    // `<xsd:schema ... xsd:targetNamespace="http://foobar">` must be rejected.
    if state.is_in_xsd_namespace(element_ns) {
        let xsd_ns = state.get_xsd_ns_id();
        for attr in &parsed_attrs {
            if attr.prefix.is_some() && attr.namespace == xsd_ns {
                let attr_name = state.ns_context.name_table().resolve(attr.local_name);
                let location = attr
                    .source
                    .as_ref()
                    .map(|s| s.to_location(state.source_map));
                state.recover_or_fail(SchemaError::structural(
                    "sch-props-correct",
                    format!(
                        "XSD attribute '{}' on element '{}' must be unqualified, not in \
                         the XSD namespace",
                        attr_name, local_name,
                    ),
                    location,
                ))?;
            }
        }
    }
    let (xsd_attrs, foreign_attrs) =
        categorize_attributes(parsed_attrs, state.ns_context.name_table());
    let attr_map = AttributeMap::new(xsd_attrs);

    // Chameleon include (§4.2.3 clause 2.3): if this is the root `<xs:schema>`
    // of a document being chameleon-included and the document declares neither
    // its own `targetNamespace` nor an explicit `xmlns` default, install the
    // includer's target namespace as the default for QName resolution. Unqualified
    // type/element/group/attribute references inside the document will then
    // resolve to the includer's namespace — matching the fact that top-level
    // definitions in the included schema are re-namespaced into the includer's
    // target namespace (see `parse_schema_with_chameleon` below).
    if state.frame_stack.is_empty()
        && local_name == xsd_names::SCHEMA
        && state.is_in_xsd_namespace(element_ns)
    {
        if let Some(chameleon_ns) = state.chameleon_namespace {
            let has_own_tns = attr_map
                .get_value_by_name(state.ns_context.name_table(), "targetNamespace")
                .is_some();
            let default_is_null = state.ns_context.default_namespace().is_none();
            if !has_own_tns && default_is_null {
                state
                    .ns_context
                    .set_default_namespace_id(Some(chameleon_ns));
            }
        }
    }

    // §F (XSD 1.1 Appendix F): conditional inclusion via vc:* attributes.
    let vc_excluded = if foreign_attrs.is_empty() {
        false
    } else {
        let ns_snapshot = state.ns_context.snapshot();
        should_skip_for_vc(
            &foreign_attrs,
            state.ns_context.name_table(),
            &ns_snapshot,
            state.config.xsd_version,
        )?
    };
    if state.frame_stack.is_empty() {
        if vc_excluded {
            state.vc_schema_excluded = true;
        }
    } else if vc_excluded || state.vc_schema_excluded {
        push_skip_frame(state, source_ref, foreign_attrs)?;
        return Ok(());
    }

    // Check if this is an XSD element (must do before borrowing frame)
    let is_in_xsd_ns = state.is_in_xsd_namespace(element_ns);

    // Check if current frame allows this child and handle skip frames
    let (allows_child, has_frame, in_skip_frame, accepts_foreign) = {
        if let Some(frame) = state.current_frame() {
            let mut allowed = frame.allows(local_name, state.ns_context.name_table());
            // Reject duplicate annotations: each XSD element allows at most one annotation
            if allowed && local_name == xsd_names::ANNOTATION && frame.has_annotation() {
                allowed = false;
            }
            (
                allowed,
                true,
                frame.is_skip_frame(),
                frame.accepts_foreign_children(),
            )
        } else {
            (true, false, false, false)
        }
    };

    if has_frame {
        // If we're inside a skip frame, absorb all children without creating new frames
        if in_skip_frame {
            // Just notify the skip frame (increments depth) and return
            if let Some(mut frame) = state.frame_stack.pop() {
                frame.on_child_start(local_name, state.ns_context.name_table());
                state.frame_stack.push(frame);
            }
            return Ok(());
        }

        // `xs:appinfo` / `xs:documentation` (and any frame opting into
        // `accepts_foreign_children`) treats its content as opaque XML —
        // including child elements that happen to be in the XSD namespace.
        // Without this gate a `<xs:complexType>` literal embedded in
        // documentation prose (addB194) would be parsed as a real
        // schema-level complexType and fail src-ct's "inline complexType
        // cannot have name" check.
        if accepts_foreign {
            push_skip_frame(state, source_ref, foreign_attrs)?;
            return Ok(());
        }

        if !is_in_xsd_ns {
            // Non-XSD child element. The schema-for-schemas content model
            // forbids foreign elements (sch-props-correct). Surface a
            // structural error and (in error recovery mode) skip the
            // subtree so subsequent valid content can still be parsed.
            let location = source_ref.as_ref().map(|s| s.to_location(state.source_map));
            state.recover_or_fail(SchemaError::structural(
                "s4s-elt-invalid-content",
                format!(
                    "Foreign-namespace element '{}' is not allowed here",
                    local_name
                ),
                location,
            ))?;
            push_skip_frame(state, source_ref, foreign_attrs)?;
            return Ok(());
        }

        if !allows_child {
            let error = SchemaError::structural(
                "s4s-elt-invalid-content",
                format!("Unexpected element '{}' in current context", local_name),
                source_ref.as_ref().map(|s| s.to_location(state.source_map)),
            );
            if state.config.error_recovery {
                // Push a skip frame for error recovery
                state.add_error(error);
                push_skip_frame(state, source_ref, foreign_attrs)?;
                return Ok(());
            } else {
                return Err(error);
            }
        }

        // Notify current frame about child start
        // Pop the frame, call method, push it back to avoid borrow issues
        if let Some(mut frame) = state.frame_stack.pop() {
            frame.on_child_start(local_name, state.ns_context.name_table());
            state.frame_stack.push(frame);
        }
    }

    // Validate XSD version compatibility
    let validation_ctx = state.validation_context(source_ref.clone());
    if let Err(e) = validate_xsd_version_element(local_name, &validation_ctx) {
        if state.config.error_recovery {
            state.add_error(e);
            push_skip_frame(state, source_ref, foreign_attrs)?;
            return Ok(());
        } else {
            return Err(e);
        }
    }

    // Perform element-specific structural validation
    if let Err(e) = validate_element_attributes(
        local_name,
        &attr_map,
        state.ns_context.name_table(),
        &validation_ctx,
    ) {
        // Located at the element, unless the check located it itself.
        let e = match &source_ref {
            Some(src) => e.with_location(state.source_map.locate(src.span.start)),
            None => e,
        };
        state.recover_or_fail(e)?;
    }

    // §3.13.2 / xs:annotation: xml:lang on documentation/appinfo, when present,
    // must be a valid xs:language value. Empty / whitespace-only values are
    // rejected by the language regex.
    if matches!(local_name, xsd_names::DOCUMENTATION | xsd_names::APPINFO) {
        let xml_ns = state
            .ns_context
            .name_table()
            .get(crate::namespace::XML_NAMESPACE);
        let lang_local = state.ns_context.name_table().get("lang");
        if let (Some(xml_ns), Some(lang_local)) = (xml_ns, lang_local) {
            for fa in &foreign_attrs {
                if fa.namespace == Some(xml_ns)
                    && fa.local_name == lang_local
                    && !crate::types::validators::is_valid_language(
                        &crate::types::facets::normalize_whitespace(
                            &fa.value,
                            crate::types::facets::WhitespaceMode::Collapse,
                        ),
                    )
                {
                    state.recover_or_fail(SchemaError::structural(
                        "s4s-att-invalid-value",
                        format!(
                            "'{}' xml:lang value '{}' is not a valid xs:language",
                            local_name, fa.value
                        ),
                        source_ref.as_ref().map(|s| s.to_location(state.source_map)),
                    ))?;
                }
            }
        }
    }

    // Validate XSD version for individual attributes
    if is_in_xsd_ns {
        for attr_name_id in attr_map.names() {
            let attr_name = state.ns_context.name_table().resolve(attr_name_id);
            if let Err(e) = validate_xsd_version_attribute(&attr_name, local_name, &validation_ctx)
            {
                state.recover_or_fail(e)?;
            }
        }
    }

    // Validate xs:ID attribute (NCName format + document-level uniqueness).
    // Skip xs:appinfo and xs:documentation — they don't define `id` in the XSD spec.
    if !matches!(local_name, xsd_names::APPINFO | xsd_names::DOCUMENTATION) {
        if let Some(id_val) = attr_map.get_value_by_name(state.ns_context.name_table(), "id") {
            if !is_ncname(id_val) {
                state.recover_or_fail(SchemaError::structural(
                    "s4s-att-invalid-value",
                    format!(
                        "'{}' attribute 'id' has invalid value '{}': not a valid xs:ID",
                        local_name, id_val
                    ),
                    source_ref.as_ref().map(|s| s.to_location(state.source_map)),
                ))?;
            } else if !state.id_values.insert(id_val.to_string()) {
                state.recover_or_fail(SchemaError::structural(
                    "s4s-att-invalid-value",
                    format!(
                        "Duplicate xs:ID value '{}' on element '{}'",
                        id_val, local_name
                    ),
                    source_ref.as_ref().map(|s| s.to_location(state.source_map)),
                ))?;
            }
        }
    }

    // Intern attribute values that are represented as NameId in frame results
    if is_in_xsd_ns {
        intern_attribute_values(local_name, &attr_map, state.ns_context.name_table_mut());
    }

    // Create namespace snapshot for QName resolution during frame construction
    let ns_snapshot = state.ns_context.snapshot();

    // The bindings a `default` / `fixed` value of a declaration is read with
    // (an `xs:QName` value resolves its prefix with them).
    if is_in_xsd_ns && matches!(local_name, xsd_names::ELEMENT | xsd_names::ATTRIBUTE) {
        let names = state.ns_context.name_table();
        let has_value_constraint = attr_map.get_value_by_name(names, "default").is_some()
            || attr_map.get_value_by_name(names, "fixed").is_some();
        if let (true, Some(source)) = (has_value_constraint, source_ref.as_ref()) {
            state
                .value_constraint_namespaces
                .push((source.span.start, ns_snapshot.clone()));
        }
    }

    // Create the new frame. An error from reading the element's attributes
    // (an unresolvable QName, say) is located at the element.
    let locate = |e: SchemaError, state: &ParserState| match &source_ref {
        Some(src) => e.with_location(state.source_map.locate(src.span.start)),
        None => e,
    };
    let frame = if state.config.error_recovery {
        let reported = state.errors.len();
        let mut frame = create_frame_recovering(
            local_name,
            &attr_map,
            state.ns_context.name_table(),
            source_ref.clone(),
            &ns_snapshot,
            &mut state.errors,
        );
        let new: Vec<SchemaError> = state.errors.drain(reported..).collect();
        for e in new {
            let e = locate(e, state);
            state.errors.push(e);
        }
        frame.set_foreign_attributes(foreign_attrs);
        // Set namespace context for annotation content frames
        if matches!(local_name, xsd_names::APPINFO | xsd_names::DOCUMENTATION) {
            frame.set_namespaces(ns_snapshot.clone());
        }
        frame
    } else {
        let mut frame = create_frame(
            local_name,
            &attr_map,
            state.ns_context.name_table(),
            source_ref.clone(),
            &ns_snapshot,
        )
        .map_err(|e| locate(e, state))?;
        frame.set_foreign_attributes(foreign_attrs);
        // Set namespace context for annotation content frames
        if matches!(local_name, xsd_names::APPINFO | xsd_names::DOCUMENTATION) {
            frame.set_namespaces(ns_snapshot.clone());
        }
        frame
    };

    // Push frame onto stack
    state.error_marks.push(state.errors.len());
    state.frame_stack.push(frame);

    Ok(())
}

/// Handle an end element event
fn handle_end_element(state: &mut ParserState, _span: SourceSpan) -> SchemaResult<()> {
    // Check if current frame is a skip frame with pending depth
    {
        if let Some(mut frame) = state.frame_stack.pop() {
            if frame.is_skip_frame() {
                // Call on_child_end to decrement depth
                // Returns true if this is the final end element for the skipped element
                if !frame.on_child_end() {
                    // Still inside nested children, put frame back and just pop scope
                    state.frame_stack.push(frame);
                    state.pop_scope();
                    return Ok(());
                }
            }
            // Put frame back for normal processing
            state.frame_stack.push(frame);
        }
    }

    // Pop the current frame and get its result
    let frame = match state.frame_stack.pop() {
        Some(f) => f,
        None => {
            return Err(SchemaError::internal("End element with no frame on stack"));
        }
    };
    let mark = state.error_marks.pop().unwrap_or(state.errors.len());

    // Save source ref before finish() consumes the frame
    let source_ref = frame.source().cloned();

    let result = match frame.finish() {
        Ok(r) => r,
        // In error-recovery mode an element whose child was refused (and
        // skipped) may then fail for lacking that child: report the cause —
        // the first error collected inside the element, as without recovery.
        Err(_) if state.errors.len() > mark => return Err(state.errors.remove(mark)),
        Err(e) => {
            // Add source location to error if available
            let e = if let Some(ref src) = source_ref {
                e.with_location(state.source_map.locate(src.span.start))
            } else {
                e
            };
            return Err(e);
        }
    };

    // Pop namespace scope
    state.pop_scope();

    // Attach result to parent frame. Errors from attach() (e.g. st-props-correct
    // duplicate facet from apply_facet) are attributed to the child frame's
    // source location — that is the offending element per §3.16.2.
    if let Some(parent) = state.current_frame_mut() {
        if let Err(e) = parent.attach(result) {
            let e = if let Some(ref src) = source_ref {
                e.with_location(state.source_map.locate(src.span.start))
            } else {
                e
            };
            return Err(e);
        }
    }
    // If no parent, store the root schema result
    else if let FrameResult::Schema(schema_result) = result {
        state.root_schema = Some(schema_result);
    }
    // In error-recovery mode a root `xs:schema` whose attributes were refused
    // (an attribute it does not allow, an invalid value) was skipped like any
    // other element: report that cause. It was collected when the root
    // started, before anything else, so it is the document's first error.
    else if !state.errors.is_empty() {
        return Err(state.errors.remove(0));
    } else {
        return Err(SchemaError::internal(
            "Root frame did not produce a schema result",
        ));
    }

    Ok(())
}

/// Handle a text event
fn handle_text(
    state: &mut ParserState,
    text: &quick_xml::events::BytesText,
    span: SourceSpan,
) -> SchemaResult<()> {
    let version = state.quick_xml_version();
    let text_content = text
        .xml_content(version)
        .map_err(|e| SchemaError::xml(format!("Text content error: {}", e), None))?;

    apply_text(state, &text_content, span)
}

/// Handle a general reference (`&amp;`, `&#65;`) appearing in character data.
///
/// quick-xml reports references as their own events, so the replacement text is
/// resolved here and then fed through the same checks as a plain text run.
fn handle_general_ref(
    state: &mut ParserState,
    reference: &quick_xml::events::BytesRef,
    span: SourceSpan,
) -> SchemaResult<()> {
    let text_content = crate::xml_entity::resolve_general_ref(reference)
        .map_err(|e| SchemaError::xml(format!("Text content error: {}", e), None))?;

    apply_text(state, &text_content, span)
}

/// Route a resolved run of character data to the current frame.
fn apply_text(state: &mut ParserState, text_content: &str, span: SourceSpan) -> SchemaResult<()> {
    // Pass text to current frame if it accepts text content; otherwise reject
    // any non-whitespace text. XSD elements like xs:notation, xs:complexType,
    // xs:sequence, etc. only allow inter-element whitespace as text content
    // (sch-props-correct / element-specific content models).
    if let Some(mut frame) = state.frame_stack.pop() {
        if frame.accepts_text() {
            frame.on_text(text_content);
        } else if !frame.is_skip_frame() && !text_content.trim().is_empty() {
            let source_ref = state.source_ref(span);
            state.frame_stack.push(frame);
            return state.recover_or_fail(SchemaError::structural(
                "sch-props-correct",
                "Non-whitespace text is not allowed here",
                Some(source_ref.to_location(state.source_map)),
            ));
        }
        state.frame_stack.push(frame);
    }

    Ok(())
}

/// Handle a CDATA section
fn handle_cdata(
    state: &mut ParserState,
    cdata: &quick_xml::events::BytesCData,
    span: SourceSpan,
) -> SchemaResult<()> {
    // CDATA is similar to text, typically in annotations
    if let Some(mut frame) = state.frame_stack.pop() {
        if frame.accepts_text() {
            // Convert CDATA to string
            if let Ok(cdata_str) = std::str::from_utf8(cdata.as_ref()) {
                frame.on_cdata(cdata_str);
            }
        } else if !frame.is_skip_frame() {
            // CDATA content is significant — even whitespace-only CDATA represents
            // intentional text. In an XSD element that doesn't allow text content,
            // this is invalid.
            let cdata_is_whitespace = std::str::from_utf8(cdata.as_ref())
                .map(|s| s.trim().is_empty())
                .unwrap_or(false);
            if !cdata_is_whitespace {
                let source_ref = state.source_ref(span);
                state.frame_stack.push(frame);
                return state.recover_or_fail(SchemaError::structural(
                    "sch-props-correct",
                    "Non-whitespace CDATA is not allowed here",
                    Some(source_ref.to_location(state.source_map)),
                ));
            }
        }
        state.frame_stack.push(frame);
    }
    Ok(())
}

/// XSD 1.1 Appendix F — conditional inclusion via vc:* version attributes.
///
/// Returns `Ok(true)` if the element should be excluded (skipped), `Ok(false)`
/// if it should be included. Returns `Err` if any vc:* version attribute has
/// an invalid decimal value (the schema is then structurally invalid).
fn should_skip_for_vc(
    foreign_attrs: &[ForeignAttribute],
    name_table: &NameTable,
    ns_snapshot: &crate::namespace::NamespaceContextSnapshot,
    xsd_version: XsdVersion,
) -> SchemaResult<bool> {
    const VC_NAMESPACE: &str = "http://www.w3.org/2007/XMLSchema-versioning";
    let Some(vc_ns_id) = name_table.get(VC_NAMESPACE) else {
        return Ok(false);
    };
    let current: f64 = match xsd_version {
        XsdVersion::V1_0 => 1.0,
        XsdVersion::V1_1 => 1.1,
    };
    for attr in foreign_attrs {
        if attr.namespace != Some(vc_ns_id) {
            continue;
        }
        let local = name_table.resolve_ref(attr.local_name);
        let include = match local {
            "minVersion" | "maxVersion" | "minVersionExclusive" | "maxVersionExclusive" => {
                let bound = match attr.value.trim().parse::<f64>() {
                    Ok(v) => v,
                    Err(_) => {
                        if xsd_version == XsdVersion::V1_1 {
                            return Err(err_versioning(format!(
                                "Invalid vc:{} value '{}': must be a valid xs:decimal",
                                local,
                                attr.value.trim()
                            )));
                        }
                        // XSD 1.0: vc: attributes are informational only; ignore invalid values
                        continue;
                    }
                };
                match local {
                    "minVersion" => current >= bound,
                    "maxVersion" => current <= bound,
                    "minVersionExclusive" => current > bound,
                    _ => current < bound,
                }
            }
            "typeAvailable" | "typeUnavailable" | "facetAvailable" | "facetUnavailable" => {
                // The W3C XMLSchema-versioning spec says vc: attributes "SHOULD
                // be honored" when recognized. Saxon honors them under both
                // XSD 1.0 and 1.1, and the suite tests (e.g. vc014 under
                // version="1.0 1.1") expect xs:error / XSD-1.1-only types to
                // be considered "unavailable" under XSD 1.0. Evaluate in both
                // modes so VC-driven branches are filtered correctly.
                let is_available_attr = matches!(local, "typeAvailable" | "facetAvailable");
                let is_type_check = matches!(local, "typeAvailable" | "typeUnavailable");
                let mut available_count = 0usize;
                let mut total_count = 0usize;
                for token in attr.value.split_whitespace() {
                    total_count += 1;
                    if vc_token_available(
                        token,
                        local,
                        is_type_check,
                        ns_snapshot,
                        name_table,
                        xsd_version,
                    )? {
                        available_count += 1;
                    }
                }
                if total_count == 0 {
                    continue;
                }
                // typeAvailable/facetAvailable: include iff ALL items available.
                // typeUnavailable/facetUnavailable: include iff ANY item is unavailable.
                if is_available_attr {
                    available_count == total_count
                } else {
                    available_count < total_count
                }
            }
            _ => continue,
        };
        if !include {
            return Ok(true);
        }
    }
    Ok(false)
}

fn err_versioning(msg: String) -> SchemaError {
    SchemaError::structural("src-versioning", msg, None)
}

/// Evaluate a single QName token in a vc:typeAvailable / facetAvailable list.
fn vc_token_available(
    token: &str,
    local: &str,
    is_type_check: bool,
    ns_snapshot: &crate::namespace::NamespaceContextSnapshot,
    name_table: &NameTable,
    xsd_version: XsdVersion,
) -> SchemaResult<bool> {
    use crate::namespace::is_ncname;
    let (prefix_str, local_str) = match token.find(':') {
        Some(pos) => (Some(&token[..pos]), &token[pos + 1..]),
        None => (None, token),
    };
    if !is_ncname(local_str) {
        return Err(err_versioning(format!(
            "Invalid QName '{}' in vc:{}: '{}' is not a valid NCName",
            token, local, local_str
        )));
    }
    let ns_id = match prefix_str {
        Some(p) => {
            if !is_ncname(p) {
                return Err(err_versioning(format!(
                    "Invalid QName '{}' in vc:{}: '{}' is not a valid NCName prefix",
                    token, local, p
                )));
            }
            let p_id = name_table.get(p).ok_or_else(|| {
                err_versioning(format!(
                    "Undeclared prefix '{}' in vc:{} value '{}'",
                    p, local, token
                ))
            })?;
            Some(ns_snapshot.resolve_prefix(p_id).ok_or_else(|| {
                err_versioning(format!(
                    "Undeclared prefix '{}' in vc:{} value '{}'",
                    p, local, token
                ))
            })?)
        }
        None => None,
    };
    if ns_id != Some(crate::namespace::well_known::XS_NAMESPACE) {
        return Ok(false);
    }
    Ok(if is_type_check {
        vc_is_xs_type_available(local_str, xsd_version)
    } else {
        vc_is_xs_facet_available(local_str)
    })
}

/// Whether `xs:{local_name}` is a type ·automatically known· to the
/// processor for `vc:typeAvailable` / `vc:typeUnavailable` (Structures 1.1
/// §4.2.2: an element is ignored when "any item in the ·actual value· T is not
/// the expanded name of some type definition ·automatically known· to the
/// processor", or for `vc:typeUnavailable` when every item is one
/// "·automatically known· to and supported by the processor"): the built-in
/// types a schema document of `xsd_version` can name.
///
/// §3.16.7.4 defines the term: "A type about which a processor possesses
/// prior knowledge, and which the processor can support without any
/// declaration of the type being supplied by the user", which "can be included
/// automatically by that processor in all schemas, but need not be". The
/// XPath 2.0 data model's `xs:untypedAtomic` is not a type definition of any
/// schema here (a schema document naming it is `src-resolve`, Structures 1.1
/// §1.3.1.1), so it is not available in either version — an element guarded
/// by `vc:typeAvailable="xs:untypedAtomic"` is ignored, one guarded by
/// `vc:typeUnavailable` kept — and XSD 1.1's additions are available in 1.1
/// only.
fn vc_is_xs_type_available(local_name: &str, xsd_version: XsdVersion) -> bool {
    match crate::types::XmlTypeCode::from_local_name(local_name) {
        Some(crate::types::XmlTypeCode::UntypedAtomic) => false,
        Some(code) => !code.is_xsd11() || xsd_version == XsdVersion::V1_1,
        None => false,
    }
}

fn vc_is_xs_facet_available(local_name: &str) -> bool {
    matches!(
        local_name,
        "minLength"
            | "maxLength"
            | "length"
            | "pattern"
            | "enumeration"
            | "whiteSpace"
            | "totalDigits"
            | "fractionDigits"
            | "minInclusive"
            | "maxInclusive"
            | "minExclusive"
            | "maxExclusive"
            | "assertion"
            | "explicitTimezone"
    )
}

/// Push a skip frame for error recovery
fn push_skip_frame(
    state: &mut ParserState,
    source: Option<SourceRef>,
    foreign_attrs: Vec<ForeignAttribute>,
) -> SchemaResult<()> {
    let mut frame: Box<dyn Frame> = Box::new(SkipFrame::new(source));
    frame.set_foreign_attributes(foreign_attrs);
    state.error_marks.push(state.errors.len());
    state.frame_stack.push(frame);
    Ok(())
}

/// Helper extension for SourceRef to convert to SourceLocation
trait SourceRefExt {
    fn to_location(&self, source_map: &SourceMap) -> SourceLocation;
}

impl SourceRefExt for SourceRef {
    fn to_location(&self, source_map: &SourceMap) -> SourceLocation {
        source_map.locate(self.span.start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::TypeKey;
    use crate::schema::model::FormChoice;

    #[test]
    fn test_parse_minimal_schema() {
        let mut schema_set = SchemaSet::new();
        let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
            <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            </xs:schema>"#;

        let result = parse_schema(xsd.as_bytes(), "test.xsd", &mut schema_set);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_schema_with_element() {
        let mut schema_set = SchemaSet::new();
        let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
            <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root" type="xs:string"/>
            </xs:schema>"#;

        let result = parse_schema(xsd.as_bytes(), "test.xsd", &mut schema_set);
        assert!(result.is_ok());
    }

    /// A declaration's recorded location is its tag's `<`. quick-xml reports
    /// positions after the event it just read, and the read that produces an
    /// element also consumes the whitespace in front of it, so an unadjusted
    /// position lands at the end of the preceding markup — on the line above.
    #[test]
    fn test_element_declaration_locates_at_open_angle_bracket() {
        let mut schema_set = SchemaSet::new();
        // Line 2 is four spaces then `<xs:element name="a" …/>`: column 5.
        let xsd = concat!(
            "<xs:schema xmlns:xs=\"http://www.w3.org/2001/XMLSchema\">\n",
            "    <xs:element name=\"a\" type=\"xs:string\"/>\n",
            "</xs:schema>",
        );

        parse_schema(xsd.as_bytes(), "test.xsd", &mut schema_set).expect("schema should parse");

        let local = schema_set.name_table.get("a").expect("name interned");
        let key = schema_set
            .lookup_element(None, local)
            .expect("element 'a' declared");
        let source = schema_set.arenas.elements[key]
            .source
            .as_ref()
            .expect("element carries a source ref");
        let loc = schema_set
            .source_maps
            .locate(source)
            .expect("source map resolves the span");

        assert_eq!(
            (loc.line, loc.column),
            (2, 5),
            "expected the '<' of <xs:element>, got {loc}"
        );
    }

    #[test]
    fn test_parse_schema_with_complex_type() {
        let mut schema_set = SchemaSet::new();
        let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
            <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:complexType name="PersonType">
                    <xs:sequence>
                        <xs:element name="name" type="xs:string"/>
                        <xs:element name="age" type="xs:int"/>
                    </xs:sequence>
                </xs:complexType>
            </xs:schema>"#;

        let result = parse_schema(xsd.as_bytes(), "test.xsd", &mut schema_set);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_schema_with_simple_type() {
        let mut schema_set = SchemaSet::new();
        let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
            <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:simpleType name="StringList">
                    <xs:list itemType="xs:string"/>
                </xs:simpleType>
            </xs:schema>"#;

        let result = parse_schema(xsd.as_bytes(), "test.xsd", &mut schema_set);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_schema_with_target_namespace() {
        let mut schema_set = SchemaSet::new();
        let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
            <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                       targetNamespace="http://example.com/test">
            </xs:schema>"#;

        let result = parse_schema(xsd.as_bytes(), "test.xsd", &mut schema_set);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_schema_with_import() {
        let mut schema_set = SchemaSet::new();
        let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
            <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:import namespace="http://www.w3.org/XML/1998/namespace"/>
            </xs:schema>"#;

        let result = parse_schema(xsd.as_bytes(), "test.xsd", &mut schema_set);
        assert!(result.is_ok());
    }

    #[cfg(feature = "xsd11")]
    #[test]
    fn test_parse_schema_assembles_arena_fields() {
        use crate::parser::frames::TypeFrameResult;
        use crate::schema::model::OpenContentMode;
        use crate::schema::wildcard::{NamespaceConstraint, ProcessContents};

        let mut schema_set = SchemaSet::xsd11();
        let xsd = r###"<?xml version="1.0" encoding="UTF-8"?>
            <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                       defaultAttributes="common">
                <xs:defaultOpenContent mode="suffix">
                    <xs:any namespace="##other" processContents="lax"/>
                </xs:defaultOpenContent>
                <xs:attributeGroup name="common">
                    <xs:attribute name="lang" type="xs:string"/>
                </xs:attributeGroup>
                <xs:element name="head1" type="xs:string"/>
                <xs:element name="head2" type="xs:string"/>
                <xs:element name="root" substitutionGroup="head1 head2">
                    <xs:complexType>
                        <xs:attribute name="code" type="xs:string"/>
                    </xs:complexType>
                </xs:element>
            </xs:schema>"###;

        let doc_id = parse_schema_with_config(
            xsd.as_bytes(),
            "test.xsd",
            &mut schema_set,
            &ParserConfig::default(),
        )
        .unwrap();

        let doc = &schema_set.documents[doc_id as usize];
        let default_attrs = doc.default_attributes.as_ref().expect("defaultAttributes");
        assert_eq!(
            schema_set.name_table.resolve(default_attrs.local_name),
            "common"
        );
        assert!(default_attrs.namespace_uri.is_none());

        let default_open = doc
            .default_open_content
            .as_ref()
            .expect("defaultOpenContent");
        assert_eq!(default_open.mode, OpenContentMode::Suffix);
        let wildcard = default_open.wildcard.as_ref().expect("wildcard");
        assert!(matches!(
            wildcard.namespace_constraint,
            NamespaceConstraint::Other
        ));
        assert_eq!(wildcard.process_contents, ProcessContents::Lax);

        let common_id = schema_set.name_table.get("common").unwrap();
        let group_key = schema_set
            .lookup_attribute_group(None, common_id)
            .expect("attributeGroup lookup");
        let group = schema_set.arenas.get_attribute_group(group_key).unwrap();
        assert_eq!(group.attributes.len(), 1);
        let lang_id = group.attributes[0].attribute.name.unwrap();
        assert_eq!(schema_set.name_table.resolve(lang_id), "lang");

        let root_id = schema_set.name_table.get("root").unwrap();
        let root_key = schema_set
            .lookup_element(None, root_id)
            .expect("element lookup");
        let root = schema_set.arenas.get_element(root_key).unwrap();
        assert_eq!(root.substitution_group.len(), 2);
        assert_eq!(
            schema_set
                .name_table
                .resolve(root.substitution_group[0].local_name),
            "head1"
        );
        assert_eq!(
            schema_set
                .name_table
                .resolve(root.substitution_group[1].local_name),
            "head2"
        );

        let inline = root.inline_type.as_ref().expect("inline type");
        match inline.as_ref() {
            TypeFrameResult::Complex(ct) => {
                assert_eq!(ct.attributes.len(), 1);
                let code_id = ct.attributes[0].attribute.name.unwrap();
                assert_eq!(schema_set.name_table.resolve(code_id), "code");
            }
            _ => panic!("expected inline complex type"),
        }
    }

    #[test]
    fn test_parse_invalid_root() {
        let mut schema_set = SchemaSet::new();
        let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
            <notSchema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            </notSchema>"#;

        let result = parse_schema(xsd.as_bytes(), "test.xsd", &mut schema_set);
        assert!(result.is_err());
    }

    /// C57: an attribute the schema for schemas does not allow on `xs:schema`
    /// (or an invalid value of one it does) is reported with its own message,
    /// code and location — as on any other XSD element — not as an internal
    /// error about the root frame.
    #[test]
    fn test_bad_attribute_on_schema_root_reports_the_attribute() {
        let cases = [
            (
                "<xs:schema xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" bogus=\"1\">\n  <xs:element name=\"a\"/>\n</xs:schema>",
                "Attribute 'bogus' is not allowed here",
            ),
            (
                "<xs:schema xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" bogus=\"1\"/>",
                "Attribute 'bogus' is not allowed here",
            ),
            (
                "<xs:schema xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" elementFormDefault=\"bogus\"/>",
                "bogus",
            ),
        ];
        for (xsd, expected) in cases {
            let mut schema_set = SchemaSet::new();
            let error = parse_schema(xsd.as_bytes(), "test.xsd", &mut schema_set)
                .expect_err("schema refused");
            let text = error.to_string();
            assert!(
                text.contains(expected) && !text.contains("Internal error"),
                "{xsd}: {text}"
            );
            assert!(
                text.contains("test.xsd:1:1"),
                "{xsd}: located at the root: {text}"
            );
        }
        // The same attribute on another XSD element, for comparison.
        let mut schema_set = SchemaSet::new();
        let text = match parse_schema(
            b"<xs:schema xmlns:xs=\"http://www.w3.org/2001/XMLSchema\"><xs:element name=\"a\" bogus=\"1\"/></xs:schema>",
            "test.xsd",
            &mut schema_set,
        ) {
            Err(error) => error.to_string(),
            Ok(_) => schema_set
                .parsing_errors
                .first()
                .map(|e| e.to_string())
                .unwrap_or_default(),
        };
        assert!(
            text.contains("Attribute 'bogus' is not allowed here"),
            "{text}"
        );
    }

    #[test]
    fn test_parse_form_choice() {
        assert_eq!(
            crate::parser::assemble::parse_form_choice(Some("qualified")),
            FormChoice::Qualified
        );
        assert_eq!(
            crate::parser::assemble::parse_form_choice(Some("unqualified")),
            FormChoice::Unqualified
        );
        assert_eq!(
            crate::parser::assemble::parse_form_choice(None),
            FormChoice::Unqualified
        );
    }

    #[test]
    fn test_parser_config_default() {
        let config = ParserConfig::default();
        assert!(config.error_recovery);
        assert!(config.collect_foreign_attributes);
        assert_eq!(config.max_depth, DEFAULT_MAX_DEPTH);
    }

    #[test]
    fn test_apply_schema_defaults_to_elements_and_types() {
        let mut schema_set = SchemaSet::new();
        let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
            <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                       blockDefault="extension"
                       finalDefault="restriction">
              <xs:element name="head" type="xs:string"/>
              <xs:complexType name="Base"/>
              <xs:simpleType name="Simple">
                <xs:restriction base="xs:string"/>
              </xs:simpleType>
            </xs:schema>"#;

        let result = parse_schema(xsd.as_bytes(), "test.xsd", &mut schema_set);
        assert!(result.is_ok());

        let name_id = schema_set.name_table.get("head").expect("name id for head");
        let ns_table = schema_set
            .namespaces
            .get(&None)
            .expect("default namespace table");
        let elem_key = ns_table.elements.get(&name_id).expect("element key");
        let elem = schema_set
            .arenas
            .elements
            .get(*elem_key)
            .expect("element data");
        assert!(elem.block.contains_extension());
        assert!(elem.final_derivation.contains_restriction());

        let base_id = schema_set.name_table.get("Base").expect("name id for Base");
        let base_key = ns_table.types.get(&base_id).expect("type key for Base");
        match base_key {
            TypeKey::Complex(key) => {
                let base = schema_set
                    .arenas
                    .complex_types
                    .get(*key)
                    .expect("complex type data");
                assert!(base.block.contains_extension());
                assert!(base.final_derivation.contains_restriction());
            }
            _ => panic!("expected complex type for Base"),
        }

        let simple_id = schema_set
            .name_table
            .get("Simple")
            .expect("name id for Simple");
        let simple_key = ns_table.types.get(&simple_id).expect("type key for Simple");
        match simple_key {
            TypeKey::Simple(key) => {
                let simple = schema_set
                    .arenas
                    .simple_types
                    .get(*key)
                    .expect("simple type data");
                assert!(simple.final_derivation.contains_restriction());
            }
            _ => panic!("expected simple type for Simple"),
        }
    }

    /// §3.3.1.2 / §3.4.1: `final=""` on an element/type is an explicit empty override —
    /// it must NOT be replaced by the document-level `finalDefault`. Only absent `final=`
    /// inherits `finalDefault`. This test verifies that the assembler correctly distinguishes
    /// the two cases (T22b fix).
    #[test]
    fn test_final_explicit_empty_overrides_final_default() {
        let mut schema_set = SchemaSet::new();
        let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
            <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                       finalDefault="restriction">
              <!-- final="" is explicit override: no derivation blocked, despite finalDefault -->
              <xs:element name="unlocked" type="xs:string" final=""/>
              <!-- absent final= inherits finalDefault="restriction" -->
              <xs:element name="inherited" type="xs:string"/>
              <xs:complexType name="UnlockedType" final=""/>
              <xs:complexType name="InheritedType"/>
              <xs:simpleType name="UnlockedSimple" final="">
                <xs:restriction base="xs:string"/>
              </xs:simpleType>
              <xs:simpleType name="InheritedSimple">
                <xs:restriction base="xs:string"/>
              </xs:simpleType>
            </xs:schema>"#;

        let result = parse_schema(xsd.as_bytes(), "test.xsd", &mut schema_set);
        assert!(result.is_ok());

        let ns_table = schema_set.namespaces.get(&None).expect("default namespace");

        // Element: final="" → empty (NOT restriction)
        let unlocked_id = schema_set.name_table.get("unlocked").expect("unlocked");
        let unlocked_key = ns_table.elements.get(&unlocked_id).expect("element key");
        let unlocked = schema_set
            .arenas
            .elements
            .get(*unlocked_key)
            .expect("element");
        assert!(
            unlocked.final_derivation.is_empty(),
            "final=\"\" must produce empty set, not inherit finalDefault"
        );

        // Element: absent final → restriction (from finalDefault)
        let inherited_id = schema_set.name_table.get("inherited").expect("inherited");
        let inherited_key = ns_table.elements.get(&inherited_id).expect("element key");
        let inherited = schema_set
            .arenas
            .elements
            .get(*inherited_key)
            .expect("element");
        assert!(
            inherited.final_derivation.contains_restriction(),
            "absent final= must inherit finalDefault=restriction"
        );

        // ComplexType: final="" → empty
        let ut_id = schema_set
            .name_table
            .get("UnlockedType")
            .expect("UnlockedType");
        let ut_key = ns_table.types.get(&ut_id).expect("type key");
        if let crate::ids::TypeKey::Complex(key) = ut_key {
            let ct = schema_set
                .arenas
                .complex_types
                .get(*key)
                .expect("complex type");
            assert!(
                ct.final_derivation.is_empty(),
                "complexType final=\"\" must not inherit finalDefault"
            );
        }

        // ComplexType: absent final → restriction
        let it_id = schema_set
            .name_table
            .get("InheritedType")
            .expect("InheritedType");
        let it_key = ns_table.types.get(&it_id).expect("type key");
        if let crate::ids::TypeKey::Complex(key) = it_key {
            let ct = schema_set
                .arenas
                .complex_types
                .get(*key)
                .expect("complex type");
            assert!(
                ct.final_derivation.contains_restriction(),
                "complexType absent final= must inherit finalDefault"
            );
        }

        // SimpleType: final="" → empty
        let us_id = schema_set
            .name_table
            .get("UnlockedSimple")
            .expect("UnlockedSimple");
        let us_key = ns_table.types.get(&us_id).expect("type key");
        if let crate::ids::TypeKey::Simple(key) = us_key {
            let st = schema_set
                .arenas
                .simple_types
                .get(*key)
                .expect("simple type");
            assert!(
                st.final_derivation.is_empty(),
                "simpleType final=\"\" must not inherit finalDefault"
            );
        }

        // SimpleType: absent final → restriction
        let is_id = schema_set
            .name_table
            .get("InheritedSimple")
            .expect("InheritedSimple");
        let is_key = ns_table.types.get(&is_id).expect("type key");
        if let crate::ids::TypeKey::Simple(key) = is_key {
            let st = schema_set
                .arenas
                .simple_types
                .get(*key)
                .expect("simple type");
            assert!(
                st.final_derivation.contains_restriction(),
                "simpleType absent final= must inherit finalDefault"
            );
        }
    }

    #[test]
    fn test_duplicate_id_detected() {
        let mut schema_set = SchemaSet::new();
        let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:element id="foo123" name="a" type="xs:string"/>
            <xs:element id="foo123" name="b" type="xs:string"/>
        </xs:schema>"#;
        let result = parse_schema(xsd.as_bytes(), "test.xsd", &mut schema_set);
        assert!(result.is_ok());
        assert!(schema_set
            .parsing_errors
            .iter()
            .any(|e| { e.to_string().contains("Duplicate xs:ID value 'foo123'") }));
    }

    #[test]
    fn test_unique_ids_valid() {
        let mut schema_set = SchemaSet::new();
        let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:element id="id1" name="a" type="xs:string"/>
            <xs:element id="id2" name="b" type="xs:string"/>
        </xs:schema>"#;
        let result = parse_schema(xsd.as_bytes(), "test.xsd", &mut schema_set);
        assert!(result.is_ok());
        assert!(schema_set.parsing_errors.is_empty());
    }

    #[test]
    fn test_invalid_id_format() {
        let mut schema_set = SchemaSet::new();
        let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:element id="123bad" name="a" type="xs:string"/>
        </xs:schema>"#;
        let result = parse_schema(xsd.as_bytes(), "test.xsd", &mut schema_set);
        assert!(result.is_ok());
        assert!(schema_set
            .parsing_errors
            .iter()
            .any(|e| { e.to_string().contains("not a valid xs:ID") }));
    }
}
