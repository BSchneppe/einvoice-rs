//! Elements assessed without a governing element declaration: their
//! `[validity]` and `[validation attempted]`, `xsi:nil` on them, and their
//! type in a typed document.
//!
//! Structures 1.1 §3.3.4.6 (Schema-Validity Assessment (Element)): "1 If E has
//! a ·governing element declaration· or a ·governing type definition·, then E
//! must be ·strictly assessed·. 2 If E is ·skipped·, then E must not be
//! ·assessed·. 3 Otherwise, E must be ·laxly assessed·." §3.3.5.1 (Assessment
//! Outcome (Element)), `[validity]`: "1 If it was ·strictly assessed·, then …
//! valid … otherwise invalid. 2 otherwise notKnown."; `[validation
//! attempted]`: "1 If it was ·strictly assessed· and neither its [children]
//! nor its [attributes] contains an information item … whose [validation
//! attempted] is not full, then full; 2 If it was not ·strictly assessed· and
//! neither its [children] nor its [attributes] contains an information item
//! … whose [validation attempted] is not none, then none; 3 otherwise
//! partial." XSD 1.0 §3.3.4 / §3.3.5 say the same ("2 otherwise notKnown").
//!
//! §3.3.1: "An element information item E is nilled with respect to some
//! element declaration D if and only if all of the following are true: 1 E has
//! xsi:nil = true. 2 D.{nillable} = true. If E is said to be ·nilled· without
//! the identity of D being clear from the context, then D is assumed to be E's
//! ·governing element declaration·." Without one, `xsi:nil` is an attribute
//! governed by its built-in declaration (§3.2.4.2 note: "attributes named
//! xsi:type, xsi:nil, xsi:schemaLocation, or xsi:noNamespaceSchemaLocation are
//! always governed by their corresponding built-in declarations") and the
//! content is checked against the governing type (Element Locally Valid (Type)
//! §3.3.4.4 clause 3.1.3 "If E is not ·nilled·…"; XSD 1.0 §3.3.4: "If clause
//! 3.2 of Element Locally Valid (Element) (§3.3.4) did not apply").
//!
//! XDM 1.0 §3.3.1.1 (read at w3.org): "The [validity] property exists and is
//! "invalid", or the [validation attempted] property exists and is "partial",
//! the schema type of an element is xs:anyType …"; "The [validity] property
//! exists and is "notKnown", and the [validation attempted] property exists and
//! is "none", the schema type of an element is xs:untyped". §6.2.1: "If the
//! dm:type-name of an Element Node is xs:untyped, then the dm:type-name of all
//! its descendant elements must also be xs:untyped".
//!
//! Every case runs in XSD 1.0 and XSD 1.1 mode — on the streaming driver with
//! and without PSVI typed values (each element's end `[validity]` recorded by
//! a handler), on the navigator driver and, with the `xsd11` feature, on the
//! buffer-document driver and the typed build — and through the push API for
//! the `SchemaInfo` of an element that is not the root.

use std::convert::Infallible;

use xsd_schema::namespace::context::NamespaceContextSnapshot;
use xsd_schema::validation::{
    drive_navigator, drive_quick_xml_with, AttributeView, CollectingValidationSink,
    ElementStartView, EndElementInfo, SchemaInfo, SchemaValidator, SchemaValidity,
    ValidationAttempted, ValidationError, ValidationEventHandler, ValidationFlags, ValidationRoot,
    ValidationSink,
};
use xsd_schema::{RoXmlNavigator, SchemaSet, SchemaSetBuilder, XsdVersion};

use SchemaValidity::{Invalid, NotKnown, Valid};

const VERSIONS: [XsdVersion; 2] = [XsdVersion::V1_0, XsdVersion::V1_1];

const XSI: &str = r#"xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance""#;

/// `bag` admits anything through a lax wildcard, `sbag` through a strict one.
const SCHEMA: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="bag"><xs:complexType><xs:sequence>
    <xs:any processContents="lax" minOccurs="0" maxOccurs="unbounded"/>
  </xs:sequence></xs:complexType></xs:element>
  <xs:element name="sbag"><xs:complexType><xs:sequence>
    <xs:any processContents="strict" minOccurs="0" maxOccurs="unbounded"/>
  </xs:sequence></xs:complexType></xs:element>
  <xs:complexType name="T">
    <xs:sequence><xs:element name="n" type="xs:int"/></xs:sequence>
  </xs:complexType>
  <xs:element name="g" type="xs:int"/>
  <xs:element name="nillable" type="T" nillable="true"/>
  <xs:element name="nn" type="T"/>
  <xs:element name="nns" type="xs:int"/>
  <xs:attribute name="ga" type="xs:int"/>
  <xs:attribute name="gref" type="xs:IDREF"/>
  <xs:attribute name="gid" type="xs:ID"/>
</xs:schema>"#;

fn schema(version: XsdVersion) -> SchemaSet {
    SchemaSetBuilder::with_version(version)
        .add_source(SCHEMA, "file:///lax_assessment.xsd")
        .expect("schema source accepted")
        .compile()
        .unwrap_or_else(|e| panic!("{version:?}: schema compiles: {e}"))
        .into_schema_set()
}

fn validator(ss: &SchemaSet, psvi: bool) -> SchemaValidator<'_> {
    let mut flags = ValidationFlags::default();
    if !psvi {
        flags &= !ValidationFlags::BUILD_PSVI_TYPED_VALUES;
    }
    SchemaValidator::new(ss, flags)
}

fn codes(errors: &[ValidationError]) -> Vec<String> {
    errors.iter().map(|e| e.constraint.to_string()).collect()
}

/// Each element's end `[validity]` and `[validation attempted]`, in end
/// order, by local name; each element's start `is_nil`; each attribute's
/// `[validity]` and error codes, by element and attribute local name.
#[derive(Default)]
struct Ends {
    open: Vec<String>,
    ends: Vec<(String, SchemaValidity, ValidationAttempted)>,
    nils: Vec<(String, bool)>,
    attrs: Vec<AttrVerdict>,
}

/// An attribute's verdict: element, attribute, `[validity]`, its codes.
type AttrVerdict = (String, String, SchemaValidity, Vec<&'static str>);

impl ValidationEventHandler for Ends {
    type Error = Infallible;

    fn after_element(
        &mut self,
        view: ElementStartView<'_>,
        info: &SchemaInfo,
    ) -> Result<(), Infallible> {
        self.open.push(view.local_name.to_string());
        self.nils.push((view.local_name.to_string(), info.is_nil));
        Ok(())
    }

    fn after_attribute(
        &mut self,
        view: AttributeView<'_>,
        info: &SchemaInfo,
    ) -> Result<(), Infallible> {
        let element = self.open.last().cloned().unwrap_or_default();
        self.attrs.push((
            element,
            view.local_name.to_string(),
            info.validity,
            info.schema_error_codes.clone(),
        ));
        Ok(())
    }

    fn after_end_element(
        &mut self,
        info: &EndElementInfo,
        _depth: usize,
    ) -> Result<(), Infallible> {
        let name = self.open.pop().unwrap_or_default();
        self.ends
            .push((name, info.validity, info.validation_attempted));
        Ok(())
    }
}

/// What one driver reports: the root's `[validity]`, the codes, and (the
/// handler-driven drivers: the stream and the typed build) every element's
/// end verdict, start `is_nil` and attribute verdicts.
struct Run {
    root: Option<SchemaValidity>,
    errors: Vec<String>,
    ends: Option<Ends>,
}

fn stream(ss: &SchemaSet, root: ValidationRoot, xml: &str, psvi: bool) -> Run {
    let v = validator(ss, psvi);
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    let mut ends = Ends::default();
    let validity = {
        let mut rt = v.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        rt.set_validation_root(root);
        let validity = drive_quick_xml_with(xml.as_bytes(), &mut rt, ss, &mut ends)
            .expect("stream drive completes")
            .root_validity;
        // `drive_quick_xml_with` leaves the end of the run to its caller:
        // the IDREF resolution (`cvc-id.1`) happens here.
        rt.end_validation().expect("end_validation");
        validity
    };
    Run {
        root: validity,
        errors: codes(&errors),
        ends: Some(ends),
    }
}

fn dom(ss: &SchemaSet, root: ValidationRoot, xml: &str) -> Run {
    let v = validator(ss, true);
    let doc = roxmltree::Document::parse(xml).expect("well-formed instance");
    let nav = RoXmlNavigator::new(&doc);
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    let validity = {
        let mut rt = v.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        rt.set_validation_root(root);
        drive_navigator(&nav, &mut rt, ss)
            .expect("navigator drive completes")
            .root_validity
    };
    Run {
        root: validity,
        errors: codes(&errors),
        ends: None,
    }
}

#[cfg(feature = "xsd11")]
fn buffer(ss: &SchemaSet, root: ValidationRoot, xml: &str) -> Run {
    use xsd_schema::document::{BufferDocument, BufferDocumentOptions};
    use xsd_schema::validation::drive_buffer_document;
    let v = validator(ss, true);
    let arena = bumpalo::Bump::new();
    let doc = BufferDocument::from_reader(
        xml.as_bytes(),
        &arena,
        &ss.name_table,
        BufferDocumentOptions::default(),
        None,
    )
    .expect("buffer build");
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    let validity = {
        let mut rt = v.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        rt.set_validation_root(root);
        drive_buffer_document(&doc, &mut rt, ss)
            .expect("buffer drive completes")
            .root_validity
    };
    Run {
        root: validity,
        errors: codes(&errors),
        ends: None,
    }
}

/// The typed build: the root's `[validity]`, the codes, and the type each
/// element is bound to (`None`: unbound, `xs:untyped`), by local name in
/// document order.
#[cfg(feature = "xsd11")]
fn typed_build(
    ss: &SchemaSet,
    root: ValidationRoot,
    xml: &str,
) -> (Run, Vec<(String, Option<String>)>) {
    use xsd_schema::document::{build_typed_document_with, TypedBuildOptions};
    use xsd_schema::navigator::{DomNavigator, DomNodeType};
    let v = validator(ss, true);
    let arena = bumpalo::Bump::new();
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    let mut ends = Ends::default();
    let (validity, types) = {
        let mut rt = v.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        rt.set_validation_root(root);
        let built = build_typed_document_with(
            xml.as_bytes(),
            &arena,
            ss,
            &mut rt,
            &mut ends,
            TypedBuildOptions::default(),
        )
        .expect("typed build completes");
        rt.end_validation().expect("end_validation");
        let mut types = Vec::new();
        // Preorder walk of the elements.
        let mut stack = vec![built.document.create_navigator()];
        while let Some(mut node) = stack.pop() {
            if node.node_type() == DomNodeType::Element {
                let name = node.local_name().to_string();
                let bound = node.schema_binding().map(|b| type_name(ss, b.type_key));
                types.push((name, bound));
            }
            let mut children = Vec::new();
            if node.move_to_first_child() {
                loop {
                    if node.node_type() == DomNodeType::Element {
                        children.push(node.clone());
                    }
                    if !node.move_to_next_sibling() {
                        break;
                    }
                }
            }
            stack.extend(children.into_iter().rev());
        }
        (built.outcome.root_validity, types)
    };
    (
        Run {
            root: validity,
            errors: codes(&errors),
            ends: Some(ends),
        },
        types,
    )
}

#[cfg(feature = "xsd11")]
fn type_name(ss: &SchemaSet, type_key: xsd_schema::TypeKey) -> String {
    use xsd_schema::TypeKey;
    let name = match type_key {
        TypeKey::Complex(k) => ss.arenas.complex_types.get(k).and_then(|ct| ct.name),
        TypeKey::Simple(k) => ss.arenas.simple_types.get(k).and_then(|st| st.name),
    };
    name.map(|n| ss.name_table.resolve(n).to_string())
        .unwrap_or_else(|| "(anonymous)".to_string())
}

/// Every driver: the root's `[validity]` is `root`, an error whose code
/// starts with `code` was reported (`None`: no error), and — where the driver
/// reports it — each named element ended with the given `[validity]` and
/// `[validation attempted]`.
fn check(
    ss: &SchemaSet,
    label: &str,
    mode: ValidationRoot,
    xml: &str,
    root: SchemaValidity,
    code: Option<&str>,
    ends: &[(&str, SchemaValidity, ValidationAttempted)],
) {
    for (driver, run) in runs(ss, mode, xml) {
        let errors = &run.errors;
        assert_eq!(
            run.root,
            Some(root),
            "{label} {driver}: root [validity] of {xml} (errors {errors:?})"
        );
        match code {
            Some(code) => assert!(
                errors.iter().any(|e| e.starts_with(code)),
                "{label} {driver}: expected a {code} error for {xml}, got {errors:?}"
            ),
            None => assert!(
                errors.is_empty(),
                "{label} {driver}: {xml}: errors {errors:?}"
            ),
        }
        if let Some(seen) = run.ends.as_ref().map(|e| &e.ends) {
            for (name, validity, attempted) in ends {
                let got = seen.iter().find(|(n, _, _)| n == name);
                assert_eq!(
                    got.map(|(_, v, a)| (*v, *a)),
                    Some((*validity, *attempted)),
                    "{label} {driver}: end of <{name}> in {xml} (all: {seen:?})"
                );
            }
        }
    }
}

/// Every driver's report of `xml` under `mode`.
fn runs(ss: &SchemaSet, mode: ValidationRoot, xml: &str) -> Vec<(&'static str, Run)> {
    let runs = vec![
        ("stream", stream(ss, mode, xml, true)),
        ("stream/no-psvi", stream(ss, mode, xml, false)),
        ("navigator", dom(ss, mode, xml)),
    ];
    #[cfg(feature = "xsd11")]
    let runs = {
        let mut runs = runs;
        runs.push(("buffer", buffer(ss, mode, xml)));
        runs.push(("typed build", typed_build(ss, mode, xml).0));
        runs
    };
    runs
}

/// Every driver reported exactly the codes `expected` (in any order).
fn check_codes(ss: &SchemaSet, label: &str, mode: ValidationRoot, xml: &str, expected: &[&str]) {
    let mut expected: Vec<String> = expected.iter().map(|c| c.to_string()).collect();
    expected.sort();
    for (driver, run) in runs(ss, mode, xml) {
        let mut got = run.errors.clone();
        got.sort();
        assert_eq!(got, expected, "{label} {driver}: codes of {xml}");
    }
}

/// The handler-driven drivers (stream, typed build): each named element's
/// start `is_nil` is as given; and, with the `xsd11` feature, the typed
/// tree's nilled property of each named element is as given.
fn check_nils(
    ss: &SchemaSet,
    label: &str,
    mode: ValidationRoot,
    xml: &str,
    start: &[(&str, bool)],
    #[cfg_attr(not(feature = "xsd11"), allow(unused_variables))] tree: &[(&str, bool)],
) {
    for (driver, run) in runs(ss, mode, xml) {
        let Some(seen) = run.ends.as_ref().map(|e| &e.nils) else {
            continue;
        };
        for (name, nil) in start {
            let got = seen.iter().find(|(n, _)| n == name).map(|(_, v)| *v);
            assert_eq!(
                got,
                Some(*nil),
                "{label} {driver}: start is_nil of <{name}> in {xml}"
            );
        }
    }
    #[cfg(feature = "xsd11")]
    {
        let nilled = typed_nilled_in(ss, mode, xml);
        for (name, nil) in tree {
            let got = nilled.iter().find(|(n, _)| n == name).map(|(_, v)| *v);
            assert_eq!(
                got,
                Some(*nil),
                "{label}: typed tree nilled of <{name}> in {xml}"
            );
        }
    }
}

/// The handler-driven drivers: the attribute `attr` of each named element
/// has the given `[validity]`, and its `schema_error_codes` contain `code`
/// (`None`: none).
fn check_attrs(
    ss: &SchemaSet,
    label: &str,
    mode: ValidationRoot,
    xml: &str,
    expected: &[(&str, &str, SchemaValidity, Option<&str>)],
) {
    for (driver, run) in runs(ss, mode, xml) {
        let Some(seen) = run.ends.as_ref().map(|e| &e.attrs) else {
            continue;
        };
        for (element, attr, validity, code) in expected {
            let got = seen
                .iter()
                .find(|(e, a, _, _)| e == element && a == attr)
                .unwrap_or_else(|| {
                    panic!("{label} {driver}: no @{attr} of <{element}> in {seen:?}")
                });
            assert_eq!(
                got.2, *validity,
                "{label} {driver}: [validity] of <{element} @{attr}> in {xml}"
            );
            match code {
                Some(code) => assert!(
                    got.3.contains(code),
                    "{label} {driver}: codes of <{element} @{attr}>: {:?}",
                    got.3
                ),
                None => assert!(
                    got.3.is_empty(),
                    "{label} {driver}: codes of <{element} @{attr}>: {:?}",
                    got.3
                ),
            }
        }
    }
}

// ── The push API ─────────────────────────────────────────────────────────

struct Sink(Vec<ValidationError>);
impl ValidationSink for Sink {
    fn on_error(&mut self, error: ValidationError) {
        self.0.push(error);
    }
    fn on_warning(&mut self, _: xsd_schema::validation::ValidationWarning) {}
}

fn no_ns() -> NamespaceContextSnapshot {
    NamespaceContextSnapshot {
        default_ns: None,
        bindings: Vec::new(),
    }
}

/// `<bag><x …>(<g>text</g>)?</x></bag>` through the push API: the start and
/// end `SchemaInfo` of `x`, the end `SchemaInfo` of `bag`, and the codes.
fn push_child(
    ss: &SchemaSet,
    xsi_type: Option<&str>,
    xsi_nil: Option<&str>,
    attribute: Option<(&str, &str)>,
    g: Option<&str>,
) -> (SchemaInfo, SchemaInfo, SchemaInfo, Vec<String>) {
    let v = validator(ss, true);
    let mut rt = v.start_run(Sink(Vec::new()));
    let ns = no_ns();
    rt.validate_element("bag", "", None, None, &ns);
    rt.validate_end_of_attributes();
    let start = rt.validate_element("x", "", xsi_type, xsi_nil, &ns);
    if let Some(nil) = xsi_nil {
        rt.validate_attribute("nil", "http://www.w3.org/2001/XMLSchema-instance", nil);
    }
    if let Some((name, value)) = attribute {
        rt.validate_attribute(name, "", value);
    }
    rt.validate_end_of_attributes();
    if let Some(text) = g {
        rt.validate_element("g", "", None, None, &ns);
        rt.validate_end_of_attributes();
        rt.validate_text(text);
        rt.validate_end_element();
    }
    let end = rt.validate_end_element();
    let bag = rt.validate_end_element();
    let _ = rt.end_validation();
    let errors = codes(&rt.sink.0);
    (start, end, bag, errors)
}

// ── C81: [validity] of a laxly assessed element ───────────────────────────

/// A lax wildcard's element with no declaration and no `xsi:type` is laxly
/// assessed: `[validity]` notKnown (clause 2), `[validation attempted]` none
/// — at its start and at its end; its strictly assessed parent stays valid
/// (§3.3.5.1 clause 1.1.2 counts only children whose `[validity]` is
/// invalid).
#[test]
fn a_lax_wildcard_element_without_a_declaration_is_not_known() {
    for version in VERSIONS {
        let ss = schema(version);
        let label = format!("{version:?}");
        check(
            &ss,
            &label,
            ValidationRoot::Declaration,
            "<bag><x/><y>text<z/></y></bag>",
            Valid,
            None,
            &[
                ("x", NotKnown, ValidationAttempted::None),
                ("y", NotKnown, ValidationAttempted::None),
                ("z", NotKnown, ValidationAttempted::None),
                ("bag", Valid, ValidationAttempted::Partial),
            ],
        );
        let (start, end, bag, errors) = push_child(&ss, None, None, None, None);
        assert!(errors.is_empty(), "{label}: {errors:?}");
        for info in [&start, &end] {
            assert_eq!(info.validity, NotKnown, "{label}");
            assert_eq!(info.schema_type, None, "{label}");
            assert_eq!(info.element_decl, None, "{label}");
        }
        assert_eq!(
            end.validation_attempted,
            ValidationAttempted::None,
            "{label}"
        );
        assert_eq!(bag.validity, Valid, "{label}");
        assert_eq!(
            bag.validation_attempted,
            ValidationAttempted::Partial,
            "{label}"
        );
    }
}

/// What a laxly assessed element's assessed descendants and attributes
/// change: its `[validation attempted]` becomes partial (clause 3), never its
/// `[validity]` — a declared child that is invalid is reported, and is
/// invalid itself, but leaves the lax element notKnown (clause 2), and so its
/// parent valid; the same for an attribute assessed by its global
/// declaration (`ga`, an `xs:int`).
#[test]
fn assessed_descendants_make_a_lax_element_partial_not_valid() {
    for version in VERSIONS {
        let ss = schema(version);
        let label = format!("{version:?}");
        let decl = ValidationRoot::Declaration;
        check(
            &ss,
            &label,
            decl,
            "<bag><x><g>1</g></x></bag>",
            Valid,
            None,
            &[
                ("g", Valid, ValidationAttempted::Full),
                ("x", NotKnown, ValidationAttempted::Partial),
            ],
        );
        check(
            &ss,
            &label,
            decl,
            "<bag><x><g>x</g></x></bag>",
            Valid,
            Some("cvc-datatype-valid"),
            &[
                ("g", Invalid, ValidationAttempted::Full),
                ("x", NotKnown, ValidationAttempted::Partial),
            ],
        );
        check(
            &ss,
            &label,
            decl,
            "<bag><x ga='1'/></bag>",
            Valid,
            None,
            &[("x", NotKnown, ValidationAttempted::Partial)],
        );
        check(
            &ss,
            &label,
            decl,
            "<bag><x ga='x'/></bag>",
            Valid,
            Some("cvc-datatype-valid"),
            &[("x", NotKnown, ValidationAttempted::Partial)],
        );
        // An attribute no declaration governs is not assessed: none.
        check(
            &ss,
            &label,
            decl,
            "<bag><x a='1'/></bag>",
            Valid,
            None,
            &[("x", NotKnown, ValidationAttempted::None)],
        );
        let (_, end, bag, errors) = push_child(&ss, None, None, None, Some("x"));
        assert!(
            errors.iter().any(|e| e.starts_with("cvc-datatype-valid")),
            "{label}: {errors:?}"
        );
        assert_eq!(end.validity, NotKnown, "{label}");
        assert_eq!(
            end.validation_attempted,
            ValidationAttempted::Partial,
            "{label}"
        );
        assert_eq!(bag.validity, Valid, "{label}");
        let (_, end, _, errors) = push_child(&ss, None, None, Some(("ga", "x")), None);
        assert!(!errors.is_empty(), "{label}");
        assert_eq!(end.validity, NotKnown, "{label}");
    }
}

/// An `xsi:type` that resolves to no type gives no ·instance-specified type
/// definition· (§3.3.4.1 clause 3: "The ·actual value· (a ·QName·) ·resolves·
/// to a type definition"), so the element has no governing type and is
/// laxly assessed — notKnown. The runtime reports the unresolved name
/// (`cvc-elt.4.1`; XSD 1.1 makes the `xsi:type` attribute itself invalid,
/// §3.2.4.1 clause 5). One that resolves makes the element strictly
/// assessed.
#[test]
fn an_unresolved_xsi_type_leaves_a_lax_element_not_known() {
    for version in VERSIONS {
        let ss = schema(version);
        let label = format!("{version:?}");
        let decl = ValidationRoot::Declaration;
        check(
            &ss,
            &label,
            decl,
            &format!("<bag {XSI}><x xsi:type='Nowhere'/></bag>"),
            Valid,
            Some("cvc-elt.4.1"),
            &[("x", NotKnown, ValidationAttempted::Partial)],
        );
        check(
            &ss,
            &label,
            decl,
            &format!("<bag {XSI}><x xsi:type='T'><n>1</n></x></bag>"),
            Valid,
            None,
            &[("x", Valid, ValidationAttempted::Full)],
        );
        let (start, end, _, errors) = push_child(&ss, Some("Nowhere"), None, None, None);
        assert!(
            errors.iter().any(|e| e.starts_with("cvc-elt.4.1")),
            "{label}: {errors:?}"
        );
        assert_eq!(start.validity, NotKnown, "{label}");
        assert_eq!(end.validity, NotKnown, "{label}");
    }
}

/// Guard: under a strict wildcard an undeclared element is an error
/// (`cvc-elt.1`) and its parent invalid (§3.3.5.1 clause 1.1.3); the runtime
/// keeps reporting such an element itself as invalid.
#[test]
fn a_strict_wildcard_element_without_a_declaration_stays_an_error() {
    for version in VERSIONS {
        let ss = schema(version);
        let label = format!("{version:?}");
        check(
            &ss,
            &label,
            ValidationRoot::Declaration,
            "<sbag><x/></sbag>",
            Invalid,
            Some("cvc-elt.1"),
            &[("x", Invalid, ValidationAttempted::None)],
        );
    }
}

/// The `ValidationRoot::Lax` root with no declaration (§5.2 lax wildcard
/// validation: "If they do not identify any declaration or definition, then
/// ·lax assessment· is performed"): notKnown whatever its attributes and
/// descendants — `DriveOutcome::root_validity` included — and partial once
/// something below it was assessed.
#[test]
fn an_undeclared_lax_root_is_not_known() {
    for version in VERSIONS {
        let ss = schema(version);
        let label = format!("{version:?}");
        let lax = ValidationRoot::Lax;
        check(
            &ss,
            &label,
            lax,
            "<u/>",
            NotKnown,
            None,
            &[("u", NotKnown, ValidationAttempted::None)],
        );
        check(
            &ss,
            &label,
            lax,
            "<u><g>1</g></u>",
            NotKnown,
            None,
            &[("u", NotKnown, ValidationAttempted::Partial)],
        );
        check(
            &ss,
            &label,
            lax,
            "<u><g>x</g></u>",
            NotKnown,
            Some("cvc-datatype-valid"),
            &[("u", NotKnown, ValidationAttempted::Partial)],
        );
        check(
            &ss,
            &label,
            lax,
            "<u ga='x'/>",
            NotKnown,
            Some("cvc-datatype-valid"),
            &[("u", NotKnown, ValidationAttempted::Partial)],
        );
        check(
            &ss,
            &label,
            lax,
            &format!("<u {XSI} xsi:type='Nowhere'/>"),
            NotKnown,
            Some("cvc-elt.4.1"),
            &[("u", NotKnown, ValidationAttempted::Partial)],
        );
    }
}

// ── C82: xsi:nil without a declaration ────────────────────────────────────

/// An element with no governing element declaration is never ·nilled·
/// (§3.3.1): `xsi:nil="true"` is no error of its own (Element Locally Valid
/// (Element), clause 3, needs a declaration) and the content is checked
/// against the governing type — an `xsi:type` here — as for any element that
/// is not nilled: `T` needs its `n`, and an `n` is no "nilled content" error.
/// On a laxly assessed element (no `xsi:type`) the content is anything.
#[test]
fn xsi_nil_without_a_declaration_nils_nothing() {
    for version in VERSIONS {
        let ss = schema(version);
        let label = format!("{version:?}");
        let decl = ValidationRoot::Declaration;
        check(
            &ss,
            &label,
            decl,
            &format!("<bag {XSI}><x xsi:type='T' xsi:nil='true'/></bag>"),
            Invalid,
            Some("cvc-complex-type.2.4"),
            &[("x", Invalid, ValidationAttempted::Full)],
        );
        check(
            &ss,
            &label,
            decl,
            &format!("<bag {XSI}><x xsi:type='T' xsi:nil='true'><n>1</n></x></bag>"),
            Valid,
            None,
            &[("x", Valid, ValidationAttempted::Full)],
        );
        check(
            &ss,
            &label,
            decl,
            &format!("<bag {XSI}><x xsi:nil='true'>text<y/></x></bag>"),
            Valid,
            None,
            &[("x", NotKnown, ValidationAttempted::Partial)],
        );
        // The root `xsi:type` promotion and the lax root alike.
        for mode in [ValidationRoot::Declaration, ValidationRoot::Lax] {
            check(
                &ss,
                &label,
                mode,
                &format!("<u {XSI} xsi:type='T' xsi:nil='true'/>"),
                Invalid,
                Some("cvc-complex-type.2.4"),
                &[],
            );
            check(
                &ss,
                &label,
                mode,
                &format!("<u {XSI} xsi:type='T' xsi:nil='true'><n>1</n></u>"),
                Valid,
                None,
                &[],
            );
        }
        // Guard: a nillable declaration nils.
        check(
            &ss,
            &label,
            decl,
            &format!("<nillable {XSI} xsi:nil='true'/>"),
            Valid,
            None,
            &[],
        );
        let (start, end, _, errors) = push_child(&ss, Some("T"), Some("true"), None, None);
        assert!(
            errors.iter().any(|e| e.starts_with("cvc-complex-type.2.4")),
            "{label}: {errors:?}"
        );
        assert!(!start.is_nil, "{label}");
        assert!(!end.is_nil, "{label}");
        let (start, end, _, _) = push_child(&ss, None, Some("true"), None, None);
        assert!(!start.is_nil, "{label}");
        assert!(!end.is_nil, "{label}");
    }
}

/// The typed build: an element no declaration governs is not nilled in the
/// tree either (XDM 1.0 §6.2.4 `nilled`: "If the [validity] property exists
/// on an information item and is "valid" then if the [nil] property exists
/// and is true, then the nilled property is "true". In all other cases … the
/// nilled property is "false""; Structures §3.3.5.3 gives [nil] only with a
/// ·governing element declaration·).
#[cfg(feature = "xsd11")]
#[test]
fn a_typed_tree_nils_only_declared_elements() {
    for version in VERSIONS {
        let ss = schema(version);
        let xml = format!(
            "<bag {XSI}><x xsi:type='T' xsi:nil='true'><n>1</n></x><nillable xsi:nil='true'/></bag>"
        );
        let nilled = typed_nilled(&ss, &xml);
        assert_eq!(
            nilled,
            vec![
                ("bag".to_string(), false),
                ("x".to_string(), false),
                ("n".to_string(), false),
                ("nillable".to_string(), true)
            ],
            "{version:?}"
        );
    }
}

/// Each element's nilled property in a typed build of `xml`, in document
/// order.
#[cfg(feature = "xsd11")]
fn typed_nilled(ss: &SchemaSet, xml: &str) -> Vec<(String, bool)> {
    typed_nilled_in(ss, ValidationRoot::Declaration, xml)
}

/// [`typed_nilled`] under the root mode `mode`.
#[cfg(feature = "xsd11")]
fn typed_nilled_in(ss: &SchemaSet, mode: ValidationRoot, xml: &str) -> Vec<(String, bool)> {
    use xsd_schema::document::{build_typed_document_with, TypedBuildOptions};
    use xsd_schema::navigator::{DomNavigator, DomNodeType, TypedValue};
    use xsd_schema::validation::NoopHandler;
    let v = validator(ss, true);
    let arena = bumpalo::Bump::new();
    let mut rt = v.start_run(Sink(Vec::new()));
    rt.set_validation_root(mode);
    let built = build_typed_document_with(
        xml.as_bytes(),
        &arena,
        ss,
        &mut rt,
        &mut NoopHandler,
        TypedBuildOptions::default(),
    )
    .expect("typed build completes");
    let mut out = Vec::new();
    let mut stack = vec![built.document.create_navigator()];
    while let Some(mut node) = stack.pop() {
        if node.node_type() == DomNodeType::Element {
            out.push((
                node.local_name().to_string(),
                matches!(node.typed_value(), TypedValue::Nilled),
            ));
        }
        let mut children = Vec::new();
        if node.move_to_first_child() {
            loop {
                if node.node_type() == DomNodeType::Element {
                    children.push(node.clone());
                }
                if !node.move_to_next_sibling() {
                    break;
                }
            }
        }
        stack.extend(children.into_iter().rev());
    }
    out
}

// ── C85: the type of a laxly assessed element in a typed tree ─────────────

/// An element's binding in a typed tree: its type's name, `None` when unbound.
#[cfg(feature = "xsd11")]
type Bound = Option<String>;

/// A typed-build case: the run's root mode, the instance, and the binding
/// each named element must have.
#[cfg(feature = "xsd11")]
type BindingCase = (ValidationRoot, String, Vec<(&'static str, Bound)>);

/// XDM 1.0 §3.3.1.1: a laxly assessed element whose `[validation attempted]`
/// is partial — something below it, or an attribute of it, was assessed — is
/// `xs:anyType`; one whose is none stays `xs:untyped` (unbound). A lax
/// wildcard's elements and the `ValidationRoot::Lax` root alike; the
/// `xsi:*` attributes are assessed by their built-in declarations, so they
/// count.
#[cfg(feature = "xsd11")]
#[test]
fn a_typed_tree_types_a_partially_assessed_element_any_type() {
    let any = || Some("anyType".to_string());
    let int = || Some("int".to_string());
    for version in VERSIONS {
        let ss = schema(version);
        let label = format!("{version:?}");
        let lax = ValidationRoot::Lax;
        let decl = ValidationRoot::Declaration;
        let cases: Vec<BindingCase> = vec![
            (lax, "<u/>".into(), vec![("u", None)]),
            (
                lax,
                "<u a='1'><v/></u>".into(),
                vec![("u", None), ("v", None)],
            ),
            (
                lax,
                "<u><g>1</g></u>".into(),
                vec![("u", any()), ("g", int())],
            ),
            (
                lax,
                "<u><v><g>1</g></v><w/></u>".into(),
                vec![("u", any()), ("v", any()), ("g", int()), ("w", None)],
            ),
            (lax, "<u ga='1'/>".into(), vec![("u", any())]),
            (
                lax,
                format!("<u {XSI} xsi:nil='false'/>"),
                vec![("u", any())],
            ),
            (
                decl,
                "<bag><x><g>1</g></x><y/></bag>".into(),
                vec![("bag", None), ("x", any()), ("g", int()), ("y", None)],
            ),
        ];
        for (mode, xml, expected) in cases {
            let (run, types) = typed_build(&ss, mode, &xml);
            assert!(run.errors.is_empty(), "{label} {xml}: {:?}", run.errors);
            for (name, ty) in expected {
                let got = types
                    .iter()
                    .find(|(n, _)| n == name)
                    .map(|(_, t)| t.clone());
                // `bag`'s own type is anonymous: only check it is bound.
                if name == "bag" {
                    assert!(
                        matches!(got, Some(Some(_))),
                        "{label} {xml}: <bag> bound ({types:?})"
                    );
                    continue;
                }
                assert_eq!(got, Some(ty), "{label} {xml}: <{name}> ({types:?})");
            }
        }
    }
}

/// The tree and XPath agree: an `xs:anyType` element is not an
/// `element(*, xs:untyped)`, its typed value is its string value as
/// `xs:untypedAtomic` (a complex type with mixed content), and an unbound one
/// still is `xs:untyped`.
#[cfg(feature = "xsd11")]
#[test]
fn xpath_sees_the_lax_types() {
    use xsd_schema::document::{build_typed_document_with, TypedBuildOptions};
    use xsd_schema::navigator::DomNavigator;
    use xsd_schema::validation::NoopHandler;
    use xsd_schema::xpath::api::XPathExpr;
    use xsd_schema::xpath::{XPathContext, XmlItem};
    for version in VERSIONS {
        let ss = schema(version);
        let v = validator(&ss, true);
        let arena = bumpalo::Bump::new();
        let mut rt = v.start_run(Sink(Vec::new()));
        rt.set_validation_root(ValidationRoot::Lax);
        let built = build_typed_document_with(
            "<u><v><g>7</g></v><w>s</w></u>".as_bytes(),
            &arena,
            &ss,
            &mut rt,
            &mut NoopHandler,
            TypedBuildOptions::default(),
        )
        .expect("typed build completes");
        let mut ns = NamespaceContextSnapshot::default();
        ns.bindings.push((
            ss.name_table.add("xs"),
            ss.name_table.add("http://www.w3.org/2001/XMLSchema"),
        ));
        let ctx = XPathContext::new(&ss.name_table)
            .with_schema_set(&ss)
            .with_namespaces(ns);
        let eval = |expr: &str| -> Vec<String> {
            let compiled = XPathExpr::compile(expr, &ctx).unwrap_or_else(|e| panic!("{expr}: {e}"));
            compiled
                .evaluator(&ctx)
                .run_with_node(built.document.create_navigator())
                .unwrap_or_else(|e| panic!("{expr}: {e}"))
                .into_vec()
                .iter()
                .map(|item| match item {
                    XmlItem::Node(node) => node.value(),
                    XmlItem::Atomic(value) => value.to_string_value(),
                })
                .collect()
        };
        let one = |expr: &str| eval(expr).join(" ");
        let label = format!("{version:?}");
        assert_eq!(
            one("/u instance of element(*, xs:untyped)"),
            "false",
            "{label}"
        );
        assert_eq!(
            one("/u/v instance of element(*, xs:untyped)"),
            "false",
            "{label}"
        );
        assert_eq!(
            one("/u/w instance of element(*, xs:untyped)"),
            "true",
            "{label}"
        );
        assert_eq!(
            one("/u/v/g instance of element(*, xs:int)"),
            "true",
            "{label}"
        );
        assert_eq!(
            one("data(/u) instance of xs:untypedAtomic"),
            "true",
            "{label}"
        );
        assert_eq!(one("string(data(/u))"), "7s", "{label}");
    }
}

// ── Fix round 1 ───────────────────────────────────────────────────────────

/// F6: the streaming rows see the end of the run too — an IDREF on an element
/// a lax wildcard matched (assessed by the global `gref`) that names no ID is
/// `cvc-id.1` on every driver; the validation root `bag` is invalid
/// (Validation Root Valid (ID/IDREF) §3.3.4.5), the lax `x` notKnown.
#[test]
fn an_idref_on_a_lax_element_must_resolve_on_every_driver() {
    for version in VERSIONS {
        let ss = schema(version);
        let label = format!("{version:?}");
        let decl = ValidationRoot::Declaration;
        check(
            &ss,
            &label,
            decl,
            "<bag><x gref='zz'/></bag>",
            Invalid,
            Some("cvc-id.1"),
            &[("x", NotKnown, ValidationAttempted::Partial)],
        );
        check(
            &ss,
            &label,
            decl,
            "<bag><x gref='a'/><y gid='a'/></bag>",
            Valid,
            None,
            &[("x", NotKnown, ValidationAttempted::Partial)],
        );
    }
}

/// F3: a declared element is ·nilled· only when its declaration is nillable
/// (§3.3.1: "1 E has xsi:nil = true. 2 D.{nillable} = true"). On `nn`
/// (`nillable` false) `xsi:nil` is `cvc-elt.3.1` (Element Locally Valid
/// (Element) clause 3.1: "D.{nillable} = false, and E has no xsi:nil
/// attribute"), still reported, and the element is not nilled — at its start
/// and end (push API, handlers) and in the typed tree — so its content is
/// checked against its type: `T` needs `n` (`cvc-complex-type.2.4`), and an
/// `n` is no "nilled content" error (`cvc-elt.3.2.1`). As the root of both
/// modes and under a lax wildcard alike.
#[test]
fn a_declaration_that_is_not_nillable_nils_nothing() {
    for version in VERSIONS {
        let ss = schema(version);
        let label = format!("{version:?}");
        for mode in [ValidationRoot::Declaration, ValidationRoot::Lax] {
            let empty = format!("<nn {XSI} xsi:nil='true'/>");
            check(
                &ss,
                &label,
                mode,
                &empty,
                Invalid,
                Some("cvc-elt.3.1"),
                &[("nn", Invalid, ValidationAttempted::Full)],
            );
            check_codes(
                &ss,
                &label,
                mode,
                &empty,
                &["cvc-elt.3.1", "cvc-complex-type.2.4"],
            );
            check_nils(
                &ss,
                &label,
                mode,
                &empty,
                &[("nn", false)],
                &[("nn", false)],
            );
            let full = format!("<nn {XSI} xsi:nil='true'><n>1</n></nn>");
            check_codes(&ss, &label, mode, &full, &["cvc-elt.3.1"]);
            check_nils(&ss, &label, mode, &full, &[("nn", false)], &[("nn", false)]);
        }
        let nested = format!("<bag {XSI}><nn xsi:nil='true'><n>1</n></nn></bag>");
        check(
            &ss,
            &label,
            ValidationRoot::Declaration,
            &nested,
            Invalid,
            Some("cvc-elt.3.1"),
            &[("nn", Invalid, ValidationAttempted::Full)],
        );
        check_codes(
            &ss,
            &label,
            ValidationRoot::Declaration,
            &nested,
            &["cvc-elt.3.1"],
        );
        // A simple type: the empty content is checked (no `xs:int`).
        let simple = format!("<nns {XSI} xsi:nil='true'/>");
        check(
            &ss,
            &label,
            ValidationRoot::Declaration,
            &simple,
            Invalid,
            Some("cvc-elt.3.1"),
            &[],
        );
        check_nils(
            &ss,
            &label,
            ValidationRoot::Declaration,
            &simple,
            &[("nns", false)],
            &[("nns", false)],
        );

        let v = validator(&ss, true);
        let mut rt = v.start_run(Sink(Vec::new()));
        let ns = no_ns();
        let start = rt.validate_element("nn", "", None, Some("true"), &ns);
        rt.validate_attribute("nil", "http://www.w3.org/2001/XMLSchema-instance", "true");
        rt.validate_end_of_attributes();
        let end = rt.validate_end_element();
        let _ = rt.end_validation();
        assert!(!start.is_nil, "{label}: start");
        assert!(!end.is_nil, "{label}: end");
        assert_eq!(end.validity, Invalid, "{label}");
    }
}

/// F3, the typed tree: XDM 1.0 §6.2.4 `nilled` — "If the [validity] property
/// exists on an information item and is "valid" then if the [nil] property
/// exists and is true, then the nilled property is "true". In all other cases
/// … the nilled property is "false"". A nillable element with `xsi:nil="true"`
/// and content is invalid (`cvc-elt.3.2.1`, clause 3.2.3.1): not nilled in
/// the tree; the valid one is.
#[cfg(feature = "xsd11")]
#[test]
fn a_typed_tree_nils_only_valid_elements() {
    for version in VERSIONS {
        let ss = schema(version);
        let label = format!("{version:?}");
        let xml = format!(
            "<bag {XSI}><nillable xsi:nil='true'><n>1</n></nillable><x><nillable xsi:nil='true'/></x></bag>"
        );
        let nilled = typed_nilled(&ss, &xml);
        assert_eq!(
            nilled,
            vec![
                ("bag".to_string(), false),
                ("nillable".to_string(), false),
                ("n".to_string(), false),
                ("x".to_string(), false),
                ("nillable".to_string(), true)
            ],
            "{label}"
        );
    }
}

/// F5: XSD 1.1 Attribute Locally Valid (§3.2.4.1) "5 If D is the built-in
/// declaration for xsi:type (Attribute Declaration for the 'type' attribute
/// (§3.2.7.1)), then A's ·actual value· ·resolves· to a type definition." An
/// `xsi:type` that names no type is an invalid attribute in XSD 1.1
/// (`cvc-attribute.5` its code); XSD 1.0's Attribute Locally Valid has no
/// such clause (clauses 1–4: a declaration, a type, String Valid, the fixed
/// value), so there it stays valid. The problem is reported once: the element
/// reports it (`cvc-elt.4.1`) when the `xsi:type` reaches its start; when a
/// host gives the element no `xsi:type` and only validates the attribute, the
/// attribute reports `cvc-attribute.5` itself (XSD 1.1). No element verdict
/// changes.
#[test]
fn an_unresolved_xsi_type_attribute_is_invalid_in_xsd_1_1() {
    for version in VERSIONS {
        let ss = schema(version);
        let label = format!("{version:?}");
        let xsd11 = version == XsdVersion::V1_1;
        let (attr, code) = if xsd11 {
            (Invalid, Some("cvc-attribute.5"))
        } else {
            (Valid, None)
        };
        let decl = ValidationRoot::Declaration;
        for xml in [
            format!("<bag {XSI}><x xsi:type='Nowhere'/></bag>"),
            format!("<bag {XSI}><x xsi:type='p:T' xmlns:p='urn:nowhere'/></bag>"),
        ] {
            check_attrs(&ss, &label, decl, &xml, &[("x", "type", attr, code)]);
            check_codes(&ss, &label, decl, &xml, &["cvc-elt.4.1"]);
            check(
                &ss,
                &label,
                decl,
                &xml,
                Valid,
                Some("cvc-elt.4.1"),
                &[("x", NotKnown, ValidationAttempted::Partial)],
            );
        }
        // A declared element: invalid by cvc-elt.4.1 as before.
        let declared = format!("<g {XSI} xsi:type='Nowhere'>1</g>");
        check_attrs(&ss, &label, decl, &declared, &[("g", "type", attr, code)]);
        check_codes(&ss, &label, decl, &declared, &["cvc-elt.4.1"]);
        // Guard: one that resolves is valid in both versions.
        let resolved = format!("<bag {XSI}><x xsi:type='T'><n>1</n></x></bag>");
        check_attrs(&ss, &label, decl, &resolved, &[("x", "type", Valid, None)]);

        // A host that withholds the `xsi:type` from the element event.
        let v = validator(&ss, true);
        let mut rt = v.start_run(Sink(Vec::new()));
        let ns = no_ns();
        rt.validate_element("bag", "", None, None, &ns);
        rt.validate_end_of_attributes();
        rt.validate_element("x", "", None, None, &ns);
        let info = rt.validate_attribute(
            "type",
            "http://www.w3.org/2001/XMLSchema-instance",
            "Nowhere",
        );
        rt.validate_end_of_attributes();
        let x = rt.validate_end_element();
        rt.validate_end_element();
        let _ = rt.end_validation();
        let errors = codes(&rt.sink.0);
        assert_eq!(info.validity, attr, "{label}: withheld");
        assert_eq!(x.validity, NotKnown, "{label}: withheld");
        if xsd11 {
            assert_eq!(errors, vec!["cvc-attribute.5".to_string()], "{label}");
            assert_eq!(info.schema_error_codes, vec!["cvc-attribute.5"], "{label}");
        } else {
            assert!(errors.is_empty(), "{label}: {errors:?}");
        }
    }
}
