//! Reusable driver that walks an already-parsed [`DomNavigator`] tree into a
//! [`ValidationRuntime`] — validation *without re-reading or re-parsing* the
//! source text.
//!
//! This is the tree-walk analogue of [`crate::validation::quick_xml_driver`].
//! Instead of pulling events from a `quick_xml::Reader`, [`drive_navigator`]
//! walks a [`DomNavigator`] in document order and emits the *same* sequence of
//! `ValidationRuntime` calls the streaming driver emits:
//!
//! 1. `validate_element` (with the in-scope namespace snapshot + `xsi:type` /
//!    `xsi:nil` discovered on the element)
//! 2. `validate_attribute` for each non-`xmlns` attribute, in document order
//! 3. `validate_end_of_attributes` (+ `take_deferred_attribute_results` under
//!    `xsd11`, drained to mirror the streaming driver)
//! 4. body: `validate_text` / `validate_whitespace` per text node, steps 1–5
//!    for each child element in its place
//! 5. `validate_end_element`
//!
//! and finally `validate_end_validation` once via `end_validation`.
//!
//! The walk is iterative — descending into a child element moves the cursor,
//! it does not call — so a tree of any depth is walked on any thread: its depth
//! costs heap, in the runtime's element stack, not stack. A tree the host
//! builds itself is bounded by no parse limit; one parsed from text by the
//! limits of whatever parsed it.
//!
//! # Why
//!
//! The streaming path consumes its input; collecting `xsi:schemaLocation`
//! hints and then re-validating against an enriched [`SchemaSet`] would require
//! a second `quick_xml` parse of the same bytes. With a buffered document
//! (e.g. `BufferDocument`) the instance is
//! parsed once and can be re-validated against any number of schema sets by
//! re-walking the in-memory tree:
//!
//! ```ignore
//! // Parse once.
//! let doc = BufferDocument::from_reader(reader, &arena, &names, opts, Some(&schema_set))?;
//!
//! // Pass 1: validate, collect hints.
//! let mut rt = validator.start_run(sink1);
//! drive_navigator(&doc.create_navigator(), &mut rt, &schema_set)?;
//! let sl = rt.schema_location_hints().to_vec();
//! let nnsl = rt.no_namespace_schema_location_hints().to_vec();
//!
//! // Enrich the schema set from the hints.
//! if let Some(enriched) = enrich_schema_set(&schema_set, &sl, &nnsl).schema_set {
//!     // Pass 2: re-validate the SAME in-memory tree — no second parse.
//!     let mut rt2 = validator2.start_run(sink2);
//!     drive_navigator(&doc.create_navigator(), &mut rt2, &enriched)?;
//! }
//! ```
//!
//! Diagnostics are reported through the runtime's
//! [`ValidationSink`], exactly as
//! with the streaming driver. The returned [`DriveOutcome`] only carries the
//! root validity and observed depth.

use crate::namespace::context::NamespaceContextSnapshot;
use crate::namespace::table::XSI_NAMESPACE;
use crate::navigator::{DomNavigator, DomNodeType, NamespaceAxisScope};
use crate::schema::SchemaSet;
use crate::validation::errors::ValidationError;
use crate::validation::info::SchemaValidity;
use crate::validation::quick_xml_driver::DriveOutcome;
use crate::validation::runtime::ValidationRuntime;
use crate::validation::validator::ValidationSink;

/// Walk `navigator`'s document tree into `runtime`, then call
/// `runtime.end_validation()`.
///
/// `navigator` may be positioned anywhere; it is cloned and reset to the
/// document root before traversal, so the caller's cursor is left untouched.
///
/// Validation diagnostics arrive through the sink the runtime was built with.
/// Comments and processing instructions are ignored, matching the turn-key
/// streaming driver.
///
/// The walk is iterative, so the tree may be of any depth whatever the
/// thread's stack: each open element costs heap in the runtime, as it does in
/// the streaming driver. No depth limit applies to the walk; a host that wants
/// one bounds the trees it builds or parses (for text,
/// [`ParseLimits::max_depth`](crate::xml::ParseLimits::max_depth)).
///
/// Returns the [`DriveOutcome`] (root validity + maximum element depth). The
/// only error path is a failing `end_validation`.
pub fn drive_navigator<N, S>(
    navigator: &N,
    runtime: &mut ValidationRuntime<'_, S>,
    schema_set: &SchemaSet,
) -> Result<DriveOutcome, ValidationError>
where
    N: DomNavigator,
    S: ValidationSink,
{
    let mut nav = navigator.clone();
    nav.move_to_root();

    let mut max_depth: usize = 0;
    let mut root_validity: Option<SchemaValidity> = None;

    // The document element is the first (and only) Element child of the root.
    // Comments / PIs around it are skipped by the node-type filter.
    if nav.move_to_first_child() {
        loop {
            if nav.node_type() == DomNodeType::Element {
                walk_element(
                    &mut nav,
                    runtime,
                    schema_set,
                    &mut max_depth,
                    &mut root_validity,
                );
            }
            if !nav.move_to_next_sibling() {
                break;
            }
        }
    }

    runtime.end_validation()?;

    Ok(DriveOutcome {
        root_validity,
        max_depth,
    })
}

/// Convenience wrapper that validates a whole [`BufferDocument`](crate::document::buffer::BufferDocument) without
/// re-parsing — the tree-walk counterpart to `drive_quick_xml` for the
/// page-based document buffer.
#[cfg(feature = "xsd11")]
pub fn drive_buffer_document<S>(
    doc: &crate::document::BufferDocument<'_>,
    runtime: &mut ValidationRuntime<'_, S>,
    schema_set: &SchemaSet,
) -> Result<DriveOutcome, ValidationError>
where
    S: ValidationSink,
{
    drive_navigator(&doc.create_navigator(), runtime, schema_set)
}

/// Validate the top-level element the cursor is positioned on, and its whole
/// subtree.
///
/// The walk is iterative: it keeps one cursor and the depth of the element
/// under it, and descending into a child element is a cursor move, not a call,
/// so the Rust stack does not grow with the depth of the tree — a tree of any
/// depth costs heap (the runtime's element stack), not stack. The calls into
/// `runtime` come in document order, exactly as a recursive walk makes them:
/// an element's start, its attributes and the end of its attributes, then its
/// content, a child element's whole subtree in its place, then the element's
/// end.
///
/// On entry and exit `nav` is positioned on the same element node, so the
/// caller's sibling walk is unaffected.
fn walk_element<N, S>(
    nav: &mut N,
    runtime: &mut ValidationRuntime<'_, S>,
    schema_set: &SchemaSet,
    max_depth: &mut usize,
    root_validity: &mut Option<SchemaValidity>,
) where
    N: DomNavigator,
    S: ValidationSink,
{
    // The depth of the element under the cursor; the top-level element is 1.
    let mut depth: usize = 1;
    'element: loop {
        // The cursor is on an element that is not open yet.
        if depth > *max_depth {
            *max_depth = depth;
        }
        open_element(nav, runtime, schema_set);

        // Its content up to its first child element, which is opened next.
        if nav.move_to_first_child() {
            loop {
                if nav.node_type() == DomNodeType::Element {
                    depth += 1;
                    continue 'element;
                }
                validate_character_data(nav, runtime);
                if !nav.move_to_next_sibling() {
                    break;
                }
            }
            nav.move_to_parent();
        }

        // The cursor is on an element whose content is done: close it, then
        // go on with the rest of its parent's content, closing the parent too
        // when that is done, and so on up.
        loop {
            // 5. End element. The DOM driver discards every element's PSVI
            //    except the root's validity, so use the value-free path (no
            //    per-element SchemaInfo).
            let end_validity = runtime.validate_end_element_novalue();
            if depth == 1 {
                *root_validity = Some(end_validity);
                return;
            }
            while nav.move_to_next_sibling() {
                if nav.node_type() == DomNodeType::Element {
                    continue 'element;
                }
                validate_character_data(nav, runtime);
            }
            nav.move_to_parent();
            depth -= 1;
        }
    }
}

/// Steps 1–3 for the element the cursor is on: its start, its attributes and
/// the end of its attributes. The cursor is restored to the element.
fn open_element<N, S>(nav: &mut N, runtime: &mut ValidationRuntime<'_, S>, schema_set: &SchemaSet)
where
    N: DomNavigator,
    S: ValidationSink,
{
    // The namespace / attribute scans both restore the cursor to the element,
    // so the element identity can be read as a borrow *after* them — no clone.
    let ns_ctx = build_ns_snapshot(nav, schema_set);
    let (xsi_type, xsi_nil) = scan_xsi(nav);

    // 1. Element. `local_name()`/`namespace_uri()` are already `&str` (no copy);
    //    the cursor is back on the element after the two scans above. The DOM
    //    driver discards all start-side PSVI, so use the value-free path.
    runtime.validate_element_novalue(
        nav.local_name(),
        nav.namespace_uri(),
        xsi_type.as_deref(),
        xsi_nil.as_deref(),
        &ns_ctx,
    );

    // 2. Attributes (all non-xmlns attributes, including xsi:type / xsi:nil —
    //    the streaming driver forwards those to validate_attribute too).
    //    Attributes have no value-free twin: their SchemaInfo is consumed
    //    *inside* the runtime (IC field matching, ID/IDREF, NOTATION read its
    //    typed_value), so only the returned copy is discarded here.
    if nav.move_to_first_attribute() {
        loop {
            // Names are already `&str`; `value_ref()` borrows the interned bytes
            // for DOM backends. All three are shared borrows of `nav` that end
            // before the next `&mut nav` cursor move (NLL).
            let aval = nav.value_ref();
            runtime.validate_attribute(nav.local_name(), nav.namespace_uri(), &aval);
            if !nav.move_to_next_attribute() {
                break;
            }
        }
        nav.move_to_parent();
    }

    // 3. End of attributes (value-free: the DOM driver discards its PSVI).
    runtime.validate_end_of_attributes_novalue();
    #[cfg(feature = "xsd11")]
    let _ = runtime.take_deferred_attribute_results();
}

/// Step 4 for a content node that is not an element: text is validated as
/// text or white space, comments and PIs carry no validation significance.
fn validate_character_data<N, S>(nav: &N, runtime: &mut ValidationRuntime<'_, S>)
where
    N: DomNavigator,
    S: ValidationSink,
{
    match nav.node_type() {
        DomNodeType::Text | DomNodeType::Whitespace | DomNodeType::SignificantWhitespace => {
            let text = nav.value_ref();
            // Classify by content, exactly as the streaming driver does,
            // rather than trusting the node-type label.
            if text.chars().all(|c| c.is_whitespace()) {
                runtime.validate_whitespace(&text);
            } else {
                runtime.validate_text(&text);
            }
        }
        _ => {}
    }
}

/// Build the in-scope namespace snapshot for the current element by walking the
/// namespace axis, mirroring the streaming driver's `build_ns_context`. The
/// cursor is restored to the element.
///
/// Uses `ExcludeXml` scope deliberately: validation treats the `xml` prefix as
/// implicit and discards it anyway, and `ExcludeXml` never materializes the
/// always-in-scope `xml:` binding — so on a namespace-free document the axis is
/// empty and no per-element `Vec`/`HashSet` is allocated (the common hot-path
/// case). This decouples validation from the (XDM) `All`-axis contract, which
/// the deprecated `namespace::` axis still needs but validation does not.
fn build_ns_snapshot<N>(nav: &mut N, schema_set: &SchemaSet) -> NamespaceContextSnapshot
where
    N: DomNavigator,
{
    let mut snapshot = NamespaceContextSnapshot::default();

    if nav.move_to_first_namespace(NamespaceAxisScope::ExcludeXml) {
        loop {
            // Namespace node: local_name() = prefix ("" for default), value() = URI.
            // Both are consumed immediately (fed to `name_table.add`, which takes
            // `&str`) before the cursor moves, so they borrow without owning.
            let prefix = nav.local_name();
            let uri = nav.value_ref();
            if prefix.is_empty() {
                // Default namespace; skip an empty binding.
                if !uri.is_empty() {
                    snapshot.default_ns = Some(schema_set.name_table.add(&uri));
                }
            } else if prefix != "xmlns" && !uri.is_empty() {
                // `ExcludeXml` already drops the `xml` prefix; the runtime treats
                // any `xmlns` binding as implicit too.
                let prefix_id = schema_set.name_table.add(prefix);
                let uri_id = schema_set.name_table.add(&uri);
                snapshot.bindings.push((prefix_id, uri_id));
            }
            if !nav.move_to_next_namespace(NamespaceAxisScope::ExcludeXml) {
                break;
            }
        }
        nav.move_to_parent();
    }

    snapshot
}

/// Scan the current element's attributes for `xsi:type` / `xsi:nil` lexical
/// values. The cursor is restored to the element.
fn scan_xsi<N>(nav: &mut N) -> (Option<String>, Option<String>)
where
    N: DomNavigator,
{
    let mut xsi_type: Option<String> = None;
    let mut xsi_nil: Option<String> = None;

    if nav.move_to_first_attribute() {
        loop {
            if nav.namespace_uri() == XSI_NAMESPACE {
                match nav.local_name() {
                    "type" => xsi_type = Some(nav.value()),
                    "nil" => xsi_nil = Some(nav.value()),
                    _ => {}
                }
            }
            if !nav.move_to_next_attribute() {
                break;
            }
        }
        nav.move_to_parent();
    }

    (xsi_type, xsi_nil)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::navigator::RoXmlNavigator;
    use crate::pipeline::load_and_process_schema;
    use crate::validation::quick_xml_driver::drive_quick_xml;
    use crate::validation::{
        CollectingValidationSink, SchemaValidator, ValidationFlags, ValidationWarning,
    };

    fn load_schema(xsd: &str) -> SchemaSet {
        let mut ss = SchemaSet::new();
        load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut ss, None).expect("schema parse");
        ss
    }

    /// Validate `instance` against `schema_set` by streaming with the
    /// quick-xml driver. Returns (root validity, sorted error strings).
    fn run_streaming(
        schema_set: &SchemaSet,
        instance: &str,
    ) -> (Option<SchemaValidity>, Vec<String>) {
        let validator = SchemaValidator::new(schema_set, ValidationFlags::default());
        let mut errors = Vec::new();
        let mut warnings: Vec<ValidationWarning> = Vec::new();
        let sink = CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        };
        let mut runtime = validator.start_run(sink);
        let outcome =
            drive_quick_xml(instance.as_bytes(), &mut runtime, schema_set).expect("stream drive");
        let mut errs: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
        errs.sort();
        (outcome.root_validity, errs)
    }

    /// Validate `instance` against `schema_set` by walking a roxmltree-backed
    /// navigator. Returns (root validity, sorted error strings).
    fn run_navigator(
        schema_set: &SchemaSet,
        instance: &str,
    ) -> (Option<SchemaValidity>, Vec<String>) {
        let doc = roxmltree::Document::parse(instance).expect("roxmltree parse");
        let nav = RoXmlNavigator::new(&doc);
        let validator = SchemaValidator::new(schema_set, ValidationFlags::default());
        let mut errors = Vec::new();
        let mut warnings: Vec<ValidationWarning> = Vec::new();
        let sink = CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        };
        let mut runtime = validator.start_run(sink);
        let outcome = drive_navigator(&nav, &mut runtime, schema_set).expect("nav drive");
        let mut errs: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
        errs.sort();
        (outcome.root_validity, errs)
    }

    /// Assert that streaming and navigator validation agree on validity and the
    /// full set of diagnostics — the core correctness guarantee for the new API.
    fn assert_parity(xsd: &str, instance: &str) {
        let schema_set = load_schema(xsd);
        let (sv, se) = run_streaming(&schema_set, instance);
        let (nv, ne) = run_navigator(&schema_set, instance);
        assert_eq!(sv, nv, "root validity mismatch for instance: {instance}");
        assert_eq!(se, ne, "diagnostics mismatch for instance: {instance}");
    }

    #[test]
    fn parity_simple_valid() {
        assert_parity(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root" type="xs:string"/>
            </xs:schema>"#,
            "<root>hello</root>",
        );
    }

    #[test]
    fn parity_simple_invalid_datatype() {
        // Non-integer content for an int-typed element: both paths must report
        // the same datatype error and Invalid validity.
        assert_parity(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="n" type="xs:int"/>
            </xs:schema>"#,
            "<n>not-a-number</n>",
        );
    }

    #[test]
    fn parity_nested_and_attributes() {
        let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:element name="root">
                <xs:complexType>
                    <xs:sequence>
                        <xs:element name="child" type="xs:int"/>
                    </xs:sequence>
                    <xs:attribute name="id" type="xs:string" use="required"/>
                </xs:complexType>
            </xs:element>
        </xs:schema>"#;
        // Valid.
        assert_parity(xsd, r#"<root id="a"><child>1</child></root>"#);
        // Missing required attribute + bad child type.
        assert_parity(xsd, r#"<root><child>x</child></root>"#);
        // Pretty-printed whitespace between elements must not change parity.
        assert_parity(xsd, "<root id=\"a\">\n  <child>1</child>\n</root>");
    }

    #[test]
    fn parity_namespaced() {
        let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                       targetNamespace="urn:ex"
                       xmlns:e="urn:ex"
                       elementFormDefault="qualified">
            <xs:element name="root">
                <xs:complexType>
                    <xs:sequence>
                        <xs:element name="leaf" type="xs:string"/>
                    </xs:sequence>
                </xs:complexType>
            </xs:element>
        </xs:schema>"#;
        assert_parity(
            xsd,
            r#"<e:root xmlns:e="urn:ex"><e:leaf>v</e:leaf></e:root>"#,
        );
        // Default-namespace form exercises the default_ns snapshot path.
        assert_parity(xsd, r#"<root xmlns="urn:ex"><leaf>v</leaf></root>"#);
        // Wrong namespace → invalid in both paths.
        assert_parity(xsd, r#"<root xmlns="urn:wrong"><leaf>v</leaf></root>"#);
    }

    #[test]
    fn parity_xsi_type() {
        // xsi:type discovery must travel through validate_element identically.
        let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:complexType name="base"/>
            <xs:complexType name="derived">
                <xs:complexContent>
                    <xs:extension base="base">
                        <xs:sequence>
                            <xs:element name="x" type="xs:int"/>
                        </xs:sequence>
                    </xs:extension>
                </xs:complexContent>
            </xs:complexType>
            <xs:element name="root" type="base"/>
        </xs:schema>"#;
        assert_parity(
            xsd,
            r#"<root xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:type="derived"><x>3</x></root>"#,
        );
    }

    /// The `drive_buffer_document` path over the page-based `BufferDocument`
    /// (the actual target of the enrichment use case) must agree with the
    /// streaming driver too — covers `BufferDocNavigator`, a different
    /// `DomNavigator` implementation from roxmltree.
    #[cfg(feature = "xsd11")]
    #[test]
    fn parity_buffer_document() {
        use crate::document::{BufferDocument, BufferDocumentOptions};
        use crate::namespace::NameTable;
        use bumpalo::Bump;

        let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:element name="root">
                <xs:complexType>
                    <xs:sequence>
                        <xs:element name="child" type="xs:int"/>
                    </xs:sequence>
                    <xs:attribute name="id" type="xs:string"/>
                </xs:complexType>
            </xs:element>
        </xs:schema>"#;
        let instance = r#"<root id="a"><child>1</child></root>"#;

        let schema_set = load_schema(xsd);
        let (sv, se) = run_streaming(&schema_set, instance);

        // Parse once into the page buffer (untyped is sufficient for the walk).
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = BufferDocument::from_reader(
            instance.as_bytes(),
            &arena,
            &names,
            BufferDocumentOptions::default(),
            None,
        )
        .expect("buffer build");

        let validator = SchemaValidator::new(&schema_set, ValidationFlags::default());
        let mut errors = Vec::new();
        let mut warnings: Vec<ValidationWarning> = Vec::new();
        let sink = CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        };
        let mut runtime = validator.start_run(sink);
        let outcome = drive_buffer_document(&doc, &mut runtime, &schema_set).expect("buffer drive");
        let mut be: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
        be.sort();

        assert_eq!(outcome.root_validity, sv, "buffer vs stream validity");
        assert_eq!(be, se, "buffer vs stream diagnostics");
    }

    /// Re-validating the same parsed tree against a second schema set works
    /// without any re-parse — the property that motivates the API.
    #[test]
    fn revalidate_same_tree_twice() {
        let permissive = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="n" type="xs:string"/>
            </xs:schema>"#,
        );
        let strict = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="n" type="xs:int"/>
            </xs:schema>"#,
        );
        let instance = "<n>abc</n>";
        let doc = roxmltree::Document::parse(instance).unwrap();

        // Pass 1: valid against the permissive schema.
        let (v1, e1) = {
            let nav = RoXmlNavigator::new(&doc);
            let validator = SchemaValidator::new(&permissive, ValidationFlags::default());
            let mut errors = Vec::new();
            let mut warnings: Vec<ValidationWarning> = Vec::new();
            let mut rt = validator.start_run(CollectingValidationSink {
                errors: &mut errors,
                warnings: &mut warnings,
            });
            let o = drive_navigator(&nav, &mut rt, &permissive).unwrap();
            (o.root_validity, errors.len())
        };
        assert_eq!(v1, Some(SchemaValidity::Valid));
        assert_eq!(e1, 0);

        // Pass 2: SAME `doc`, no re-parse — invalid against the strict schema.
        let (v2, e2) = {
            let nav = RoXmlNavigator::new(&doc);
            let validator = SchemaValidator::new(&strict, ValidationFlags::default());
            let mut errors = Vec::new();
            let mut warnings: Vec<ValidationWarning> = Vec::new();
            let mut rt = validator.start_run(CollectingValidationSink {
                errors: &mut errors,
                warnings: &mut warnings,
            });
            let o = drive_navigator(&nav, &mut rt, &strict).unwrap();
            (o.root_validity, errors.len())
        };
        assert_eq!(v2, Some(SchemaValidity::Invalid));
        assert!(e2 > 0);
    }
}
