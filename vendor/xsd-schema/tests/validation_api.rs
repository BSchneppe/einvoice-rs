//! The host-facing validation API, end to end through the public drivers:
//!
//! * `SchemaValidator::new` honours `ValidationFlags::PROCESS_ASSERTIONS`
//!   (inline assertion evaluation, as `new_fragment_buffer`).
//! * Errors and warnings name their subject and a namespace-aware positional
//!   path (`Q{uri}local[n]` steps, `@Q{uri}local` for an attribute); the
//!   local-name `element_path()` and `Display` are what they were.
//! * The typed build composes: the caller's validator, sink and event handler,
//!   one pass over the input (`build_typed_document_with`,
//!   `TypedBuilderHandler`).
//! * A handler can tell the driver to skip validating an attribute
//!   (`ValidationEventHandler::skip_attribute`).

use xsd_schema::validation::{
    drive_navigator, drive_quick_xml, CollectingValidationSink, ExpandedName, InstancePath,
    PathStep, SchemaValidator, SchemaValidity, ValidationError, ValidationFlags, ValidationSubject,
    ValidationWarning,
};
use xsd_schema::{RoXmlNavigator, SchemaSet, SchemaSetBuilder, XsdVersion};

// ── Harness ───────────────────────────────────────────────────────────────

fn schema(version: XsdVersion, body: &str) -> SchemaSet {
    let xsd =
        format!(r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">{body}</xs:schema>"#);
    SchemaSetBuilder::with_version(version)
        .add_source(&xsd, "file:///validation_api.xsd")
        .expect("schema source accepted")
        .compile()
        .expect("schema compiles")
        .into_schema_set()
}

/// Stream `xml` through `validator`; the root validity and every diagnostic.
fn stream(
    validator: &SchemaValidator<'_>,
    ss: &SchemaSet,
    xml: &str,
) -> (
    Option<SchemaValidity>,
    Vec<ValidationError>,
    Vec<ValidationWarning>,
) {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let outcome = {
        let mut runtime = validator.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        drive_quick_xml(xml.as_bytes(), &mut runtime, ss).expect("drive completes")
    };
    (outcome.root_validity, errors, warnings)
}

// ── B9: `SchemaValidator::new` and `PROCESS_ASSERTIONS` ──────────────────

#[cfg(feature = "xsd11")]
const FALSE_ASSERTION: &str = r#"
    <xs:element name="root">
        <xs:complexType>
            <xs:sequence><xs:element name="a" type="xs:string"/></xs:sequence>
            <xs:assert test="false()"/>
        </xs:complexType>
    </xs:element>"#;

/// Structures §3.13.4.1 Assertion Satisfied: "1 … the {test} … evaluates to
/// true"; `false()` never does, so the instance is invalid once assertions
/// are processed.
#[cfg(feature = "xsd11")]
#[test]
fn new_with_process_assertions_evaluates_assertions() {
    let ss = schema(XsdVersion::V1_1, FALSE_ASSERTION);
    let flags = ValidationFlags::default() | ValidationFlags::PROCESS_ASSERTIONS;
    let validator = SchemaValidator::new(&ss, flags);
    let (validity, errors, _) = stream(&validator, &ss, "<root><a>x</a></root>");
    assert_eq!(validity, Some(SchemaValidity::Invalid));
    let codes: Vec<_> = errors.iter().map(|e| e.constraint).collect();
    assert_eq!(codes, ["cvc-assertion"], "{errors:?}");

    // The same verdict as the dedicated constructor.
    let validator = SchemaValidator::new_fragment_buffer(&ss, ValidationFlags::default());
    let (validity, errors, _) = stream(&validator, &ss, "<root><a>x</a></root>");
    assert_eq!(validity, Some(SchemaValidity::Invalid));
    assert_eq!(errors.len(), 1, "{errors:?}");
}

/// Without the flag `::new` still does not evaluate assertions.
#[cfg(feature = "xsd11")]
#[test]
fn new_without_process_assertions_skips_assertions() {
    let ss = schema(XsdVersion::V1_1, FALSE_ASSERTION);
    let validator = SchemaValidator::new(&ss, ValidationFlags::default());
    let (validity, errors, _) = stream(&validator, &ss, "<root><a>x</a></root>");
    assert_eq!(validity, Some(SchemaValidity::Valid));
    assert!(errors.is_empty(), "{errors:?}");
}

/// `new_main_document` evaluates no assertion (its documented behaviour): a
/// `false()` assertion validates.
#[cfg(feature = "xsd11")]
#[test]
fn new_main_document_evaluates_no_assertion() {
    let ss = schema(XsdVersion::V1_1, FALSE_ASSERTION);
    let flags = ValidationFlags::default() | ValidationFlags::PROCESS_ASSERTIONS;
    let validator = SchemaValidator::new_main_document(&ss, flags);
    let (validity, errors, _) = stream(&validator, &ss, "<root><a>x</a></root>");
    assert_eq!(validity, Some(SchemaValidity::Valid));
    assert!(errors.is_empty(), "{errors:?}");
}

// ── B8: structured subject and path ───────────────────────────────────────

const ORDER: &str = r#"
    <xs:element name="order">
        <xs:complexType>
            <xs:sequence>
                <xs:element name="note" type="xs:string" minOccurs="0" maxOccurs="unbounded"/>
                <xs:element name="item" maxOccurs="unbounded">
                    <xs:complexType>
                        <xs:simpleContent>
                            <xs:extension base="xs:int">
                                <xs:attribute name="qty" type="xs:positiveInteger"/>
                                <xs:attribute name="id" type="xs:ID"/>
                                <xs:attribute name="ref" type="xs:IDREF"/>
                            </xs:extension>
                        </xs:simpleContent>
                    </xs:complexType>
                </xs:element>
                <xs:element name="note" type="xs:string" minOccurs="0" maxOccurs="unbounded"/>
            </xs:sequence>
        </xs:complexType>
    </xs:element>"#;

/// `ORDER` in the target namespace `urn:po`, element form qualified.
fn order_schema() -> SchemaSet {
    let xsd = format!(
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                      targetNamespace="urn:po" elementFormDefault="qualified">{ORDER}</xs:schema>"#
    );
    SchemaSetBuilder::with_version(XsdVersion::V1_0)
        .add_source(&xsd, "file:///validation_api_po.xsd")
        .expect("schema source accepted")
        .compile()
        .expect("schema compiles")
        .into_schema_set()
}

fn po(local: &str) -> ExpandedName {
    ExpandedName::new(Some("urn:po"), local)
}

fn elem(local: &str, position: usize) -> PathStep {
    PathStep::element(po(local), position)
}

fn attr(local: &str) -> PathStep {
    PathStep::attribute(ExpandedName::new(None, local))
}

/// Validate `xml` against `ORDER` with both drivers; the errors must agree.
fn order_errors(xml: &str) -> Vec<ValidationError> {
    let ss = order_schema();
    let validator = SchemaValidator::new(&ss, ValidationFlags::default());
    let (_, streamed, _) = stream(&validator, &ss, xml);

    let doc = roxmltree::Document::parse(xml).expect("well-formed");
    let mut walked = Vec::new();
    let mut warnings = Vec::new();
    {
        let mut runtime = validator.start_run(CollectingValidationSink {
            errors: &mut walked,
            warnings: &mut warnings,
        });
        drive_navigator(&RoXmlNavigator::new(&doc), &mut runtime, &ss).expect("walk completes");
    }
    let key = |e: &ValidationError| {
        (
            e.constraint,
            e.subject().cloned(),
            e.instance_path().cloned(),
            e.element_path().map(str::to_string),
        )
    };
    assert_eq!(
        streamed.iter().map(key).collect::<Vec<_>>(),
        walked.iter().map(key).collect::<Vec<_>>(),
        "the streaming and the tree-walking driver disagree"
    );
    streamed
}

/// An element's own value: the subject is the element, the path counts it
/// among its same-named siblings only (the `note` before it does not count).
#[test]
fn element_error_names_the_element_and_its_position() {
    let errors = order_errors(
        r#"<p:order xmlns:p="urn:po"><p:note/><p:item>1</p:item><p:item>x</p:item></p:order>"#,
    );
    assert_eq!(errors.len(), 1, "{errors:?}");
    let e = &errors[0];
    assert_eq!(e.subject(), Some(&ValidationSubject::Element(po("item"))));
    assert_eq!(
        e.instance_path(),
        Some(&InstancePath::new(vec![elem("order", 1), elem("item", 2)]))
    );
    assert_eq!(
        e.instance_path().unwrap().to_string(),
        "/Q{urn:po}order[1]/Q{urn:po}item[2]"
    );
    // The local-name path and the Display form are unchanged.
    assert_eq!(e.element_path(), Some("/order/item"));
    assert!(e.to_string().ends_with(" (/order/item)"), "{e}");
    assert!(
        e.to_string().starts_with(&format!("[{}] ", e.constraint)),
        "{e}"
    );
}

/// An attribute's value: the subject is the attribute, the path ends in an
/// attribute step written `@Q{}local` for a name in no namespace.
#[test]
fn attribute_error_names_the_attribute() {
    let errors = order_errors(
        r#"<p:order xmlns:p="urn:po"><p:item>1</p:item><p:item qty="0">2</p:item></p:order>"#,
    );
    assert_eq!(errors.len(), 1, "{errors:?}");
    let e = &errors[0];
    assert_eq!(
        e.subject(),
        Some(&ValidationSubject::Attribute(ExpandedName::new(
            None, "qty"
        )))
    );
    assert_eq!(
        e.instance_path().map(ToString::to_string).as_deref(),
        Some("/Q{urn:po}order[1]/Q{urn:po}item[2]/@Q{}qty")
    );
    assert!(e.instance_path().unwrap().steps().eq([
        &elem("order", 1),
        &elem("item", 2),
        &attr("qty")
    ]));
    // The local-name path stays the element's.
    assert_eq!(e.element_path(), Some("/order/item"));
}

/// A child the content model rejects: `cvc-complex-type.2.4` is a constraint
/// on the parent (Structures §3.4.4.2 Element Locally Valid (Complex Type)),
/// so the parent is the subject; the message names the child.
#[test]
fn content_model_error_is_about_the_parent() {
    let errors =
        order_errors(r#"<p:order xmlns:p="urn:po"><p:item>1</p:item><p:bogus/></p:order>"#);
    let e = errors
        .iter()
        .find(|e| e.constraint == "cvc-complex-type.2.4")
        .unwrap_or_else(|| panic!("{errors:?}"));
    assert_eq!(e.subject(), Some(&ValidationSubject::Element(po("order"))));
    assert_eq!(e.instance_path().unwrap().to_string(), "/Q{urn:po}order[1]");
    assert!(e.message.contains("bogus"), "{e}");
}

/// An IDREF is checked at the end of the run; its error still names the
/// attribute that carried it, with the path as it was when it was read.
#[test]
fn unresolved_idref_names_its_attribute() {
    let errors = order_errors(
        r#"<p:order xmlns:p="urn:po"><p:item id="a">1</p:item><p:item ref="b">2</p:item></p:order>"#,
    );
    let e = errors
        .iter()
        .find(|e| e.constraint == "cvc-id.1")
        .unwrap_or_else(|| panic!("{errors:?}"));
    assert_eq!(
        e.subject(),
        Some(&ValidationSubject::Attribute(ExpandedName::new(
            None, "ref"
        )))
    );
    assert_eq!(
        e.instance_path().unwrap().to_string(),
        "/Q{urn:po}order[1]/Q{urn:po}item[2]/@Q{}ref"
    );
    assert_eq!(e.element_path(), Some("/order/item"));
}

/// No-namespace names are written `Q{}local`; positions restart under each
/// parent.
#[test]
fn positions_restart_under_each_parent() {
    let ss = schema(
        XsdVersion::V1_0,
        r#"<xs:element name="r">
             <xs:complexType><xs:sequence>
               <xs:element name="g" maxOccurs="unbounded">
                 <xs:complexType><xs:sequence>
                   <xs:element name="v" type="xs:int" maxOccurs="unbounded"/>
                 </xs:sequence></xs:complexType>
               </xs:element>
             </xs:sequence></xs:complexType>
           </xs:element>"#,
    );
    let validator = SchemaValidator::new(&ss, ValidationFlags::default());
    let (_, errors, _) = stream(
        &validator,
        &ss,
        "<r><g><v>1</v><v>2</v><v>3</v></g><g><v>1</v><v>x</v></g></r>",
    );
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(
        errors[0].instance_path().unwrap().to_string(),
        "/Q{}r[1]/Q{}g[2]/Q{}v[2]"
    );
    assert_eq!(
        errors[0].subject(),
        Some(&ValidationSubject::Element(ExpandedName::new(None, "v")))
    );
}

/// Warnings carry the subject and the path too.
#[test]
fn warning_names_its_subject() {
    let ss = schema(
        XsdVersion::V1_0,
        r#"<xs:element name="r"><xs:complexType><xs:sequence>
             <xs:element name="d" type="xs:decimal" maxOccurs="unbounded"/>
           </xs:sequence></xs:complexType></xs:element>"#,
    );
    let validator = SchemaValidator::new(&ss, ValidationFlags::default());
    let (validity, errors, warnings) = stream(
        &validator,
        &ss,
        "<r><d>1</d><d>123456789012345678901234567890123456</d></r>",
    );
    assert_eq!(validity, Some(SchemaValidity::Valid));
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    let w = &warnings[0];
    assert_eq!(
        w.subject(),
        Some(&ValidationSubject::Element(ExpandedName::new(None, "d")))
    );
    assert_eq!(w.instance_path().unwrap().to_string(), "/Q{}r[1]/Q{}d[2]");
}

/// A host-built error: the free constructors leave subject and path empty.
#[test]
fn constructed_errors_have_no_subject() {
    let e = xsd_schema::validation::error("cvc-elt.1", "message", None);
    assert!(e.subject().is_none() && e.instance_path().is_none() && e.element_path().is_none());
    assert_eq!(e.to_string(), "[cvc-elt.1] message");
    let w = ValidationWarning::new("code", "message", None);
    assert!(w.subject().is_none() && w.instance_path().is_none());
    assert_eq!(w.to_string(), "[code] message");
    assert_eq!(InstancePath::default().to_string(), "/");

    // The builders set what the runtime would.
    let subject = ValidationSubject::Attribute(ExpandedName::new(Some(""), "id"));
    let path = InstancePath::new(vec![
        PathStep::element(ExpandedName::new(Some("urn:a"), "r"), 1),
        PathStep::attribute(ExpandedName::new(None, "id")),
    ]);
    let e = e
        .with_subject(subject.clone())
        .with_instance_path(path.clone());
    assert_eq!(e.subject(), Some(&subject));
    assert_eq!(
        e.instance_path().map(ToString::to_string).as_deref(),
        Some("/Q{urn:a}r[1]/@Q{}id")
    );
    assert_eq!(
        subject.name().namespace(),
        None,
        "an empty namespace is no namespace"
    );
    assert_eq!(subject.to_string(), "@Q{}id");
    let w = w.with_subject(subject.clone()).with_instance_path(path);
    assert_eq!(w.subject(), Some(&subject));
    assert_eq!(w.to_string(), "[code] message", "Display is unchanged");
}

/// `ExpandedName` is opaque and canonical: an empty namespace is no
/// namespace, for equality and hashing too.
#[test]
fn expanded_names_are_canonical() {
    use std::collections::HashSet;
    let none = ExpandedName::new(None, "a");
    let empty = ExpandedName::new(Some(""), "a");
    assert_eq!(none, empty);
    assert_eq!(HashSet::from([none.clone(), empty]).len(), 1);
    assert_eq!(none.namespace(), None);
    assert_eq!(none.namespace_uri(), "");
    assert_eq!(none.local_name(), "a");
    assert_eq!(none.to_string(), "Q{}a");
    let ns = ExpandedName::new(Some("urn:x"), "b");
    assert_eq!(ns.namespace(), Some("urn:x"));
    assert_eq!(ns.to_string(), "Q{urn:x}b");
    assert_ne!(ns, ExpandedName::new(None, "b"));
    assert_eq!(
        ValidationSubject::Element(ns.clone()).to_string(),
        "Q{urn:x}b"
    );
}

/// Diagnostics stay `Send + Sync` (they may be handed to another thread).
#[test]
fn diagnostics_are_send_and_sync() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<ValidationError>();
    send_sync::<ValidationWarning>();
    send_sync::<InstancePath>();
    send_sync::<ExpandedName>();
    send_sync::<ValidationSubject>();
}

/// The path API: steps outermost first, the last step, equality by steps,
/// accessors on the steps.
#[test]
fn instance_path_api() {
    let a = ExpandedName::new(Some("urn:a"), "r");
    let path = InstancePath::new(vec![
        PathStep::element(a.clone(), 1),
        PathStep::element(ExpandedName::new(None, "c"), 3),
        PathStep::attribute(ExpandedName::new(None, "id")),
    ]);
    assert_eq!(path.len(), 3);
    assert!(!path.is_empty());
    // An iterator, outermost first, that knows its length and runs both ways.
    let mut steps = path.steps();
    assert_eq!(steps.len(), 3);
    let first = steps.next().unwrap();
    assert_eq!(first.name(), &a);
    assert_eq!(first.position(), Some(1));
    assert_eq!(steps.len(), 2);
    assert_eq!(steps.next_back().map(PathStep::position), Some(None));
    assert_eq!(steps.next().map(PathStep::position), Some(Some(3)));
    assert!(steps.next().is_none() && steps.next_back().is_none());
    assert_eq!(
        path.steps()
            .rev()
            .map(|s| s.name().local_name())
            .collect::<Vec<_>>(),
        ["id", "c", "r"]
    );
    assert_eq!(path.last().map(|s| s.name().local_name()), Some("id"));
    assert_eq!(path.to_string(), "/Q{urn:a}r[1]/Q{}c[3]/@Q{}id");
    let same = InstancePath::from(vec![
        PathStep::element(a, 1),
        PathStep::element(ExpandedName::new(Some(""), "c"), 3),
        PathStep::attribute(ExpandedName::new(None, "id")),
    ]);
    assert_eq!(path, same);
    assert_ne!(path, InstancePath::default());
    assert!(InstancePath::default().is_empty());
    assert_eq!(InstancePath::default().steps().len(), 0);

    // Past the inline buffer (deeper than 32 steps) the order is the same.
    let deep = InstancePath::new(
        (1..=100)
            .map(|n| PathStep::element(ExpandedName::new(None, "e"), n))
            .collect(),
    );
    assert!(deep.steps().map(|s| s.position().unwrap()).eq(1..=100));
    assert!(deep
        .steps()
        .rev()
        .map(|s| s.position().unwrap())
        .eq((1..=100).rev()));
    assert_eq!(deep.steps().len(), 100);
}

/// A very deep path is built, compared, written and dropped without
/// recursion (a small thread stack would overflow otherwise).
#[test]
fn a_very_deep_path_is_handled_iteratively() {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let name = ExpandedName::new(None, "a");
            let deep = |n: usize| {
                InstancePath::new(
                    (1..=n)
                        .map(|_| PathStep::element(name.clone(), 1))
                        .collect(),
                )
            };
            let (p, q) = (deep(300_000), deep(300_000));
            assert_eq!(p.len(), 300_000);
            assert_eq!(p, q);
            assert_eq!(p.to_string().len(), 300_000 * "/Q{}a[1]".len());
            drop(p);
            drop(q);
        })
        .unwrap()
        .join()
        .unwrap();
}

/// One error per level of a deep document: every error has its own, right
/// path (the runtime shares the ancestor steps between them).
#[test]
fn every_level_of_a_deep_document_gets_its_path() {
    let ss = schema(
        XsdVersion::V1_0,
        r#"<xs:element name="a" type="AT"/>
           <xs:complexType name="AT"><xs:sequence>
             <xs:element name="a" type="AT" minOccurs="0"/>
           </xs:sequence></xs:complexType>"#,
    );
    let depth = 2_000;
    let xml = "<a x='1'>".repeat(depth) + &"</a>".repeat(depth);
    let validator = SchemaValidator::new(&ss, ValidationFlags::default());
    let (_, errors, _) = stream(&validator, &ss, &xml);
    assert_eq!(errors.len(), depth);
    for (level, e) in errors.iter().enumerate() {
        let path = e.instance_path().expect("a path");
        assert_eq!(path.len(), level + 2, "the elements down to it, then @x");
        assert_eq!(
            path.last(),
            Some(&PathStep::attribute(ExpandedName::new(None, "x")))
        );
        assert_eq!(e.element_path().map(str::len), Some(2 * (level + 1)));
    }
}

// ── C14: the composable typed build ──────────────────────────────────────

#[cfg(feature = "xsd11")]
mod typed_build {
    use super::*;
    use bumpalo::Bump;
    use std::convert::Infallible;
    use xsd_schema::document::{
        build_typed_document, build_typed_document_with, serialize, BufferDocumentBuilder,
        BufferDocumentError, BufferDocumentOptions, TypedBuildError, TypedBuildOptions,
        TypedBuilderHandler,
    };
    use xsd_schema::navigator::{DomNavigator, TypedValue};
    use xsd_schema::validation::{
        drive_quick_xml_with, ElementStartView, EndElementInfo, NoopHandler, SchemaInfo,
        ValidationEventHandler,
    };

    const SCHEMA: &str = r#"
        <xs:element name="order">
            <xs:complexType>
                <xs:sequence>
                    <xs:element name="qty" type="xs:positiveInteger" maxOccurs="unbounded"/>
                </xs:sequence>
                <xs:attribute name="total" type="xs:int"/>
                <xs:assert test="sum(qty) le @total"/>
            </xs:complexType>
        </xs:element>"#;

    /// A host handler run in the same pass: depth, PIs, element PSVI.
    #[derive(Default)]
    struct Observer {
        max_depth: usize,
        depth: usize,
        pis: Vec<String>,
        started: Vec<(String, Option<SchemaValidity>)>,
        ends: usize,
        fail_at_depth: Option<usize>,
    }

    impl ValidationEventHandler for Observer {
        type Error = String;

        fn before_element(&mut self, view: ElementStartView<'_>) -> Result<(), String> {
            self.depth += 1;
            self.max_depth = self.max_depth.max(self.depth);
            if Some(self.depth) == self.fail_at_depth {
                return Err(format!("too deep at {}", view.local_name));
            }
            Ok(())
        }

        fn after_element(
            &mut self,
            view: ElementStartView<'_>,
            info: &SchemaInfo,
        ) -> Result<(), String> {
            self.started
                .push((view.local_name.to_string(), Some(info.validity)));
            Ok(())
        }

        fn after_end_element(
            &mut self,
            _info: &EndElementInfo,
            _depth: usize,
        ) -> Result<(), String> {
            self.depth -= 1;
            self.ends += 1;
            Ok(())
        }

        fn on_processing_instruction(&mut self, target: &str, _data: &str) -> Result<(), String> {
            self.pis.push(target.to_string());
            Ok(())
        }
    }

    /// One pass: the caller's validator (assertions on), the caller's sink
    /// (errors kept, with their paths), the caller's handler (sees every
    /// event), and a typed document.
    #[test]
    fn one_pass_with_the_callers_validator_sink_and_handler() {
        let ss = schema(XsdVersion::V1_1, SCHEMA);
        let validator = SchemaValidator::new(
            &ss,
            ValidationFlags::default() | ValidationFlags::PROCESS_ASSERTIONS,
        );
        let xml = r#"<order total="3"><?audit x?><qty>2</qty><qty>0</qty><qty>2</qty></order>"#;

        let arena = Bump::new();
        let (mut errors, mut warnings) = (Vec::new(), Vec::new());
        let mut observer = Observer::default();
        let mut runtime = validator.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        let built = build_typed_document_with(
            xml.as_bytes(),
            &arena,
            &ss,
            &mut runtime,
            &mut observer,
            TypedBuildOptions::default(),
        )
        .expect("the build completes");
        runtime.end_validation().expect("no operational failure");
        drop(runtime);

        assert_eq!(built.outcome.root_validity, Some(SchemaValidity::Invalid));
        assert_eq!(built.outcome.max_depth, 2);
        let found: Vec<_> = errors
            .iter()
            .map(|e| (e.constraint, e.instance_path().map(ToString::to_string)))
            .collect();
        assert!(
            found.contains(&("cvc-assertion", Some("/Q{}order[1]".to_string()))),
            "the assertion was evaluated: {found:?}"
        );
        assert!(
            found
                .iter()
                .any(|(_, p)| p.as_deref() == Some("/Q{}order[1]/Q{}qty[2]")),
            "the facet error on the second qty: {found:?}"
        );

        // The handler saw the same pass.
        assert_eq!(observer.max_depth, 2);
        assert_eq!(observer.ends, 4);
        assert_eq!(observer.pis, ["audit"]);
        assert_eq!(observer.started.len(), 4);

        // The document is typed and complete (the PI is in it too).
        let doc = &built.document;
        let mut nav = doc.create_navigator();
        assert!(nav.move_to_first_child());
        assert!(nav.element_type_key().is_some());
        assert!(nav.move_to_first_attribute());
        assert!(matches!(nav.typed_value(), TypedValue::Value(_)));
        let text = serialize::to_string(&doc.create_navigator(), &Default::default()).unwrap();
        assert_eq!(text, xml);
    }

    /// `build_typed_document` is the composable build with default flags, a
    /// silent sink and no handler: the same document.
    #[test]
    fn build_typed_document_is_the_composable_build_with_defaults() {
        let ss = schema(XsdVersion::V1_1, SCHEMA);
        let xml = r#"<order total="9"><qty>2</qty><qty>3</qty></order>"#;
        let arena = Bump::new();
        let plain = build_typed_document(
            xml.as_bytes(),
            &arena,
            &ss,
            BufferDocumentOptions::default(),
        )
        .expect("build");

        let validator = SchemaValidator::new(&ss, ValidationFlags::default());
        let mut runtime = validator.start_run(xsd_schema::document::SilentValidationSink);
        let built = build_typed_document_with(
            xml.as_bytes(),
            &arena,
            &ss,
            &mut runtime,
            &mut NoopHandler,
            TypedBuildOptions::default(),
        )
        .expect("build");
        assert_eq!(built.outcome.root_validity, Some(SchemaValidity::Valid));

        let (mut a, mut b) = (plain.create_navigator(), built.document.create_navigator());
        assert_eq!(
            serialize::to_string(&a, &Default::default()).unwrap(),
            serialize::to_string(&b, &Default::default()).unwrap()
        );
        a.move_to_first_child();
        b.move_to_first_child();
        assert_eq!(a.element_type_key(), b.element_type_key());
        a.move_to_first_child();
        b.move_to_first_child();
        assert_eq!(a.typed_value(), b.typed_value());
    }

    /// The caller's handler can abort the pass; its error comes back as is.
    #[test]
    fn a_handler_error_aborts_the_build() {
        let ss = schema(XsdVersion::V1_1, SCHEMA);
        let validator = SchemaValidator::new(&ss, ValidationFlags::default());
        let arena = Bump::new();
        let mut observer = Observer {
            fail_at_depth: Some(2),
            ..Observer::default()
        };
        let mut runtime = validator.start_run(xsd_schema::document::SilentValidationSink);
        let result = build_typed_document_with(
            "<order><qty>1</qty></order>".as_bytes(),
            &arena,
            &ss,
            &mut runtime,
            &mut observer,
            TypedBuildOptions::default(),
        );
        match result {
            Err(TypedBuildError::Handler(message)) => assert_eq!(message, "too deep at qty"),
            Err(other) => panic!("unexpected error {other}"),
            Ok(_) => panic!("the handler error was dropped"),
        }

        // The handler's error is shown, and is the source when it is an
        // `Error`.
        let e = TypedBuildError::<std::io::Error>::Handler(std::io::Error::other("my message"));
        assert_eq!(e.to_string(), "handler error: my message");
        assert!(std::error::Error::source(&e).is_some());
    }

    /// Input that ends inside an element is a well-formedness error
    /// (`Truncated`, naming the open element), not an internal error — also
    /// from the turn-key build.
    #[test]
    fn truncated_input_is_an_xml_error() {
        let ss = schema(XsdVersion::V1_1, SCHEMA);
        let validator = SchemaValidator::new(&ss, ValidationFlags::default());
        let arena = Bump::new();
        let missing_end_tag = |e: &BufferDocumentError| {
            matches!(e, BufferDocumentError::NotWellFormed(e)
                if e.rule() == xsd_schema::xml::WellFormednessRule::Truncated
                    && e.message().contains("1 element"))
        };
        let mut runtime = validator.start_run(xsd_schema::document::SilentValidationSink);
        let result = build_typed_document_with(
            "<order><qty>1</qty>".as_bytes(),
            &arena,
            &ss,
            &mut runtime,
            &mut NoopHandler,
            TypedBuildOptions::default(),
        );
        match result {
            Err(TypedBuildError::Document(e)) => assert!(missing_end_tag(&e), "{e:?}"),
            Err(other) => panic!("{other:?}"),
            Ok(_) => panic!("truncated input accepted"),
        }
        match build_typed_document(
            "<order><qty>1</qty>".as_bytes(),
            &arena,
            &ss,
            BufferDocumentOptions::default(),
        ) {
            Err(e) => assert!(missing_end_tag(&e), "{e:?}"),
            Ok(_) => panic!("truncated input accepted"),
        }
    }

    /// The runtime must validate against the schema set the document is
    /// built for; a mismatch is an error in every build.
    #[test]
    fn a_runtime_for_another_schema_set_is_an_error() {
        let ss = schema(XsdVersion::V1_1, SCHEMA);
        let other = schema(XsdVersion::V1_1, SCHEMA);
        let validator = SchemaValidator::new(&other, ValidationFlags::default());
        let arena = Bump::new();
        let mut runtime = validator.start_run(xsd_schema::document::SilentValidationSink);
        let result = build_typed_document_with(
            "<order><qty>1</qty></order>".as_bytes(),
            &arena,
            &ss,
            &mut runtime,
            &mut NoopHandler,
            TypedBuildOptions::default(),
        );
        assert!(matches!(result, Err(TypedBuildError::SchemaSetMismatch)));
    }

    /// Options are extensible: the document options ride inside.
    #[test]
    fn typed_build_options_carry_the_document_options() {
        let ss = schema(XsdVersion::V1_1, SCHEMA);
        let validator = SchemaValidator::new(&ss, ValidationFlags::default());
        let arena = Bump::new();
        let document = BufferDocumentOptions {
            track_source_locations: true,
            ..BufferDocumentOptions::default()
        };
        let options = TypedBuildOptions::default().with_document_options(document);
        assert_eq!(options.document, document);
        let mut runtime = validator.start_run(xsd_schema::document::SilentValidationSink);
        let built = build_typed_document_with(
            "<order><qty>1</qty></order>".as_bytes(),
            &arena,
            &ss,
            &mut runtime,
            &mut NoopHandler,
            options,
        )
        .expect("build");
        assert!(built.document.has_source_spans());
    }

    /// A handler behind `&mut dyn ValidationEventHandler` can be passed.
    #[test]
    fn a_dyn_handler_can_be_passed() {
        let ss = schema(XsdVersion::V1_1, SCHEMA);
        let validator = SchemaValidator::new(&ss, ValidationFlags::default());
        let arena = Bump::new();
        let mut observer = Observer::default();
        let handler: &mut dyn ValidationEventHandler<Error = String> = &mut observer;
        let mut runtime = validator.start_run(xsd_schema::document::SilentValidationSink);
        let built = build_typed_document_with(
            "<order><qty>1</qty></order>".as_bytes(),
            &arena,
            &ss,
            &mut runtime,
            handler,
            TypedBuildOptions::default(),
        )
        .expect("build");
        assert_eq!(built.outcome.root_validity, Some(SchemaValidity::Valid));
        assert_eq!(observer.ends, 2);
    }

    /// The building block: the public handler composed by hand, driven by
    /// the layer-2 driver.
    #[test]
    fn the_handler_composes_with_the_layer_two_driver() {
        let ss = schema(XsdVersion::V1_1, SCHEMA);
        let validator = SchemaValidator::new(&ss, ValidationFlags::default());
        let arena = Bump::new();
        let mut builder = BufferDocumentBuilder::new(
            &arena,
            &ss.name_table,
            Some(&ss),
            BufferDocumentOptions::default(),
        )
        .expect("builder");
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut runtime = validator.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        let observer = {
            let mut handler = TypedBuilderHandler::with_handler(&mut builder, Observer::default());
            let outcome = drive_quick_xml_with(
                "<order><qty>x</qty></order>".as_bytes(),
                &mut runtime,
                &ss,
                &mut handler,
            )
            .expect("drive");
            assert_eq!(outcome.root_validity, Some(SchemaValidity::Invalid));
            assert_eq!(handler.handler().ends, 2);
            handler.into_handler()
        };
        runtime.end_validation().unwrap();
        drop(runtime);
        let doc = builder.finalize().expect("finalize");
        assert_eq!(observer.started.len(), 2);
        assert_eq!(errors.len(), 1, "{errors:?}");
        let mut nav = doc.create_navigator();
        assert!(nav.move_to_first_child());
        assert!(nav.element_type_key().is_some());

        // Without an inner handler: the plain builder handler.
        let mut builder = BufferDocumentBuilder::new(
            &arena,
            &ss.name_table,
            Some(&ss),
            BufferDocumentOptions::default(),
        )
        .expect("builder");
        let mut runtime = validator.start_run(xsd_schema::document::SilentValidationSink);
        let mut handler: TypedBuilderHandler<'_, '_> = TypedBuilderHandler::new(&mut builder);
        let r: Result<_, xsd_schema::validation::DriveWithError<TypedBuildError<Infallible>>> =
            drive_quick_xml_with(
                "<order><qty>1</qty></order>".as_bytes(),
                &mut runtime,
                &ss,
                &mut handler,
            );
        assert!(r.is_ok());
    }
}

// ── C18: skipping an attribute ───────────────────────────────────────────

mod skip_attribute {
    use super::*;
    use xsd_schema::validation::{
        drive_quick_xml_with, AttributeView, SchemaInfo, ValidationAttempted,
        ValidationEventHandler,
    };

    const PAYLOAD: &str = r#"
        <xs:element name="Body">
            <xs:complexType>
                <xs:sequence>
                    <xs:element name="order" nillable="true">
                        <xs:complexType>
                            <xs:attribute name="no" type="xs:int"/>
                        </xs:complexType>
                    </xs:element>
                </xs:sequence>
            </xs:complexType>
        </xs:element>"#;

    const SIGNED: &str = r#"<Body xmlns:wsu="urn:wsu" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><order wsu:Id="id-1" no="5" xsi:nil="false"/></Body>"#;

    /// Skips `{urn:wsu}Id`, records what it was asked and what the
    /// attributes' PSVI was.
    #[derive(Default)]
    struct SkipWsuId {
        asked: Vec<String>,
        seen: Vec<(String, SchemaValidity, ValidationAttempted)>,
    }

    impl ValidationEventHandler for SkipWsuId {
        type Error = std::convert::Infallible;

        fn skip_attribute(&mut self, view: AttributeView<'_>) -> Result<bool, Self::Error> {
            self.asked
                .push(format!("{{{}}}{}", view.namespace_uri, view.local_name));
            Ok(view.namespace_uri == "urn:wsu" && view.local_name == "Id")
        }

        fn after_attribute(
            &mut self,
            view: AttributeView<'_>,
            info: &SchemaInfo,
        ) -> Result<(), Self::Error> {
            self.seen.push((
                view.local_name.to_string(),
                info.validity,
                info.validation_attempted,
            ));
            Ok(())
        }
    }

    fn run(handler: &mut SkipWsuId, xml: &str) -> (Option<SchemaValidity>, Vec<ValidationError>) {
        let ss = schema(XsdVersion::V1_0, PAYLOAD);
        let validator = SchemaValidator::new(&ss, ValidationFlags::default());
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let outcome = {
            let mut runtime = validator.start_run(CollectingValidationSink {
                errors: &mut errors,
                warnings: &mut warnings,
            });
            let outcome =
                drive_quick_xml_with(xml.as_bytes(), &mut runtime, &ss, handler).expect("drive");
            runtime.end_validation().expect("end");
            outcome
        };
        (outcome.root_validity, errors)
    }

    /// The signed payload validates: the skipped attribute is not an error,
    /// it is reported to `after_attribute` as not assessed, and the others
    /// are validated as usual.
    #[test]
    fn the_driver_skips_what_the_handler_asks() {
        let mut handler = SkipWsuId::default();
        let (validity, errors) = run(&mut handler, SIGNED);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(validity, Some(SchemaValidity::Valid));
        assert_eq!(
            handler.seen[..2],
            [
                (
                    "Id".to_string(),
                    SchemaValidity::NotKnown,
                    ValidationAttempted::None
                ),
                (
                    "no".to_string(),
                    SchemaValidity::Valid,
                    ValidationAttempted::Full
                ),
            ]
        );
        assert_eq!(
            handler.seen.len(),
            3,
            "xsi:nil is validated: {:?}",
            handler.seen
        );
        assert_eq!(handler.seen[2].0, "nil");
        // Not asked about namespace declarations or xsi attributes.
        assert_eq!(handler.asked, ["{urn:wsu}Id", "{}no"]);

        // The default handler validates every attribute.
        let ss = schema(XsdVersion::V1_0, PAYLOAD);
        let validator = SchemaValidator::new(&ss, ValidationFlags::default());
        let (validity, errors, _) = stream(&validator, &ss, SIGNED);
        assert_eq!(validity, Some(SchemaValidity::Invalid));
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(
            errors[0].subject(),
            Some(&ValidationSubject::Attribute(ExpandedName::new(
                Some("urn:wsu"),
                "Id"
            )))
        );
    }

    /// Skipping does not hide the other attributes' errors.
    #[test]
    fn other_attributes_are_still_validated() {
        let mut handler = SkipWsuId::default();
        let (validity, errors) = run(&mut handler, &SIGNED.replace(r#"no="5""#, r#"no="five""#));
        assert_eq!(validity, Some(SchemaValidity::Invalid));
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(
            errors[0]
                .instance_path()
                .map(ToString::to_string)
                .as_deref(),
            Some("/Q{}Body[1]/Q{}order[1]/@Q{}no")
        );
    }

    /// A handler that asks to skip everything only gets the undeclared
    /// attributes skipped: a declared one is validated by its attribute use
    /// (Structures §3.4.4.2 clause 2.1 before 2.2), and `after_attribute` sees
    /// its real PSVI.
    #[test]
    fn declared_attributes_are_validated_whatever_the_handler_says() {
        struct SkipAll(Vec<(String, SchemaValidity)>);
        impl ValidationEventHandler for SkipAll {
            type Error = std::convert::Infallible;
            fn skip_attribute(&mut self, _view: AttributeView<'_>) -> Result<bool, Self::Error> {
                Ok(true)
            }
            fn after_attribute(
                &mut self,
                view: AttributeView<'_>,
                info: &SchemaInfo,
            ) -> Result<(), Self::Error> {
                self.0.push((view.local_name.to_string(), info.validity));
                Ok(())
            }
        }
        let ss = schema(XsdVersion::V1_0, PAYLOAD);
        let validator = SchemaValidator::new(&ss, ValidationFlags::default());
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut handler = SkipAll(Vec::new());
        let outcome = {
            let mut runtime = validator.start_run(CollectingValidationSink {
                errors: &mut errors,
                warnings: &mut warnings,
            });
            let xml = SIGNED.replace(r#"no="5""#, r#"no="five""#);
            let outcome = drive_quick_xml_with(xml.as_bytes(), &mut runtime, &ss, &mut handler)
                .expect("drive");
            runtime.end_validation().expect("end");
            outcome
        };
        assert_eq!(outcome.root_validity, Some(SchemaValidity::Invalid));
        let codes: Vec<_> = errors.iter().map(|e| e.constraint).collect();
        assert_eq!(codes, ["cvc-datatype-valid"], "{errors:?}");
        assert_eq!(
            handler.0[..2],
            [
                ("Id".to_string(), SchemaValidity::NotKnown),
                ("no".to_string(), SchemaValidity::Invalid),
            ]
        );
    }

    /// With type alternatives the skip is decided after the selection; the
    /// typed build gets one final binding per attribute (no deferred-count
    /// mismatch), the declared attribute typed, the skipped one untyped.
    #[cfg(feature = "xsd11")]
    #[test]
    fn the_typed_build_with_type_alternatives() {
        use bumpalo::Bump;
        use xsd_schema::document::{build_typed_document_with, TypedBuildOptions};
        use xsd_schema::navigator::{DomNavigator, TypedValue};

        let ss = schema(
            XsdVersion::V1_1,
            r#"<xs:complexType name="numType">
                 <xs:attribute name="no" type="xs:integer"/>
               </xs:complexType>
               <xs:element name="order">
                 <xs:complexType><xs:attribute name="no" type="xs:string"/></xs:complexType>
                 <xs:alternative test="@wsu:Id" type="numType" xmlns:wsu="urn:wsu"/>
               </xs:element>"#,
        );
        let validator = SchemaValidator::new(&ss, ValidationFlags::default());
        let arena = Bump::new();
        let (mut errors, mut warnings) = (Vec::new(), Vec::new());
        let mut handler = SkipWsuId::default();
        let mut runtime = validator.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        let built = build_typed_document_with(
            r#"<order xmlns:wsu="urn:wsu" wsu:Id="i" no="7"/>"#.as_bytes(),
            &arena,
            &ss,
            &mut runtime,
            &mut handler,
            TypedBuildOptions::default(),
        )
        .expect("build");
        runtime.end_validation().expect("end");
        drop(runtime);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(built.outcome.root_validity, Some(SchemaValidity::Valid));
        let mut nav = built.document.create_navigator();
        assert!(nav.move_to_first_child() && nav.move_to_first_attribute());
        let mut typed = Vec::new();
        loop {
            typed.push((
                nav.local_name().to_string(),
                matches!(nav.typed_value(), TypedValue::Value(_)),
            ));
            if !nav.move_to_next_attribute() {
                break;
            }
        }
        typed.sort();
        assert_eq!(typed, [("Id".to_string(), false), ("no".to_string(), true)]);
    }

    /// The typed build honours the hook through a composed handler: the
    /// attribute is in the document, untyped.
    #[cfg(feature = "xsd11")]
    #[test]
    fn the_typed_build_honours_it() {
        use bumpalo::Bump;
        use xsd_schema::document::{build_typed_document_with, TypedBuildOptions};
        use xsd_schema::navigator::{DomNavigator, TypedValue};

        let ss = schema(XsdVersion::V1_0, PAYLOAD);
        let validator = SchemaValidator::new(&ss, ValidationFlags::default());
        let arena = Bump::new();
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut handler = SkipWsuId::default();
        let mut runtime = validator.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        let built = build_typed_document_with(
            SIGNED.as_bytes(),
            &arena,
            &ss,
            &mut runtime,
            &mut handler,
            TypedBuildOptions::default(),
        )
        .expect("build");
        runtime.end_validation().expect("end");
        drop(runtime);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(built.outcome.root_validity, Some(SchemaValidity::Valid));
        assert_eq!(handler.asked, ["{urn:wsu}Id", "{}no"]);

        let mut nav = built.document.create_navigator();
        assert!(nav.move_to_first_child() && nav.move_to_first_child()); // Body/order
        assert!(nav.move_to_first_attribute());
        let mut attrs = Vec::new();
        loop {
            attrs.push((
                nav.local_name().to_string(),
                nav.element_type_key().is_some(),
                matches!(nav.typed_value(), TypedValue::Value(_)),
            ));
            if !nav.move_to_next_attribute() {
                break;
            }
        }
        assert!(
            attrs.contains(&("Id".to_string(), false, false)),
            "the skipped attribute is in the document, untyped: {attrs:?}"
        );
        assert!(attrs.contains(&("no".to_string(), true, true)), "{attrs:?}");
    }
}
