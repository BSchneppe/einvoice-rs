//! How a run assesses its validation root: `ValidationRuntime::set_validation_root`.
//!
//! Structures 1.1 §5.2 (Assessing Schema-Validity) names the approaches:
//! *type-driven validation* — "The user or application identifies a type
//! definition from among the type definitions of the schema. If the
//! ·validation root· is an element, then it is validated as described in
//! Schema-Validity Assessment (Element) (§3.3.4.6) (with the stipulated type
//! definition as the ·governing type definition·)"; *element-driven
//! validation* — "with the stipulated element declaration as the ·governing·
//! declaration"; *lax wildcard validation* — "The processor starts from
//! Schema-Validity Assessment (Element) (§3.3.4.6) with no stipulated
//! declaration or definition. If the ·validation root· and the schema
//! determine an element declaration (by the name of the element), an
//! attribute declaration (by the name of the attribute), or a type definition
//! (by xsi:type), then ·strict validation· is performed. If they do not
//! identify any declaration or definition, then ·lax assessment· is
//! performed"; and *strict wildcard validation*, the same with the absence
//! of a declaration reported to the invoking process — this crate's default,
//! `cvc-elt.1`.
//!
//! Every case runs in XSD 1.0 and XSD 1.1 mode, on streaming validation with
//! and without PSVI typed values, on the navigator driver and — with the
//! `xsd11` feature — on the buffer-document driver and the typed build.

use xsd_schema::namespace::context::NamespaceContextSnapshot;
use xsd_schema::validation::{
    drive_navigator, drive_quick_xml, CollectingValidationSink, SchemaInfo, SchemaValidator,
    SchemaValidity, TypeSource, ValidationAttempted, ValidationError, ValidationFlags,
    ValidationRoot, ValidationSink,
};
use xsd_schema::{
    ComplexTypeKey, ElementKey, RoXmlNavigator, SchemaSet, SchemaSetBuilder, TypeKey, XsdVersion,
};

use SchemaValidity::{Invalid, NotKnown, Valid};

const VERSIONS: [XsdVersion; 2] = [XsdVersion::V1_0, XsdVersion::V1_1];

const XSI: &str = r#"xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance""#;

/// The schema every test uses (no target namespace).
const SCHEMA: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:complexType name="T">
    <xs:sequence><xs:element name="n" type="xs:int"/></xs:sequence>
    <xs:attribute name="a" type="xs:int"/>
  </xs:complexType>
  <xs:complexType name="T2"><xs:complexContent><xs:extension base="T">
    <xs:sequence><xs:element name="m" type="xs:string"/></xs:sequence>
  </xs:extension></xs:complexContent></xs:complexType>
  <xs:complexType name="Blocked" block="extension">
    <xs:sequence><xs:element name="n" type="xs:int"/></xs:sequence>
  </xs:complexType>
  <xs:complexType name="BlockedExt"><xs:complexContent><xs:extension base="Blocked">
    <xs:sequence><xs:element name="m" type="xs:string"/></xs:sequence>
  </xs:extension></xs:complexContent></xs:complexType>
  <xs:complexType name="Abstract" abstract="true">
    <xs:sequence><xs:element name="n" type="xs:int" minOccurs="0"/></xs:sequence>
  </xs:complexType>
  <xs:complexType name="Concrete"><xs:complexContent><xs:extension base="Abstract"/></xs:complexContent></xs:complexType>
  <xs:simpleType name="Small"><xs:restriction base="xs:int"><xs:maxInclusive value="9"/></xs:restriction></xs:simpleType>
  <xs:element name="decl" type="xs:string"/>
  <xs:element name="typed" type="T"/>
  <xs:element name="abstractDecl" type="xs:string" abstract="true"/>
  <xs:element name="g" type="xs:int"/>
  <xs:attribute name="ga" type="xs:int"/>
</xs:schema>"#;

fn schema(version: XsdVersion) -> SchemaSet {
    SchemaSetBuilder::with_version(version)
        .add_source(SCHEMA, "file:///validation_root.xsd")
        .expect("schema source accepted")
        .compile()
        .unwrap_or_else(|e| panic!("{version:?}: schema compiles: {e}"))
        .into_schema_set()
}

fn type_key(ss: &SchemaSet, name: &str) -> TypeKey {
    let name = ss.name_table.add(name);
    ss.lookup_type(None, name).expect("a named type")
}

fn element_key(ss: &SchemaSet, name: &str) -> ElementKey {
    let name = ss.name_table.add(name);
    ss.lookup_element(None, name).expect("a global element")
}

fn validator(ss: &SchemaSet, psvi: bool) -> SchemaValidator<'_> {
    let mut flags = ValidationFlags::default();
    if !psvi {
        flags &= !ValidationFlags::BUILD_PSVI_TYPED_VALUES;
    }
    SchemaValidator::new(ss, flags)
}

/// Root validity and reported constraint codes from one driver.
type Outcome = (Option<SchemaValidity>, Vec<String>);

fn codes(errors: &[ValidationError]) -> Vec<String> {
    errors.iter().map(|e| e.constraint.to_string()).collect()
}

fn stream(ss: &SchemaSet, root: ValidationRoot, xml: &str, psvi: bool) -> Outcome {
    let v = validator(ss, psvi);
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    let validity = {
        let mut rt = v.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        rt.set_validation_root(root);
        drive_quick_xml(xml.as_bytes(), &mut rt, ss)
            .expect("stream drive completes")
            .root_validity
    };
    (validity, codes(&errors))
}

fn dom(ss: &SchemaSet, root: ValidationRoot, xml: &str) -> Outcome {
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
    (validity, codes(&errors))
}

#[cfg(feature = "xsd11")]
fn buffer(ss: &SchemaSet, root: ValidationRoot, xml: &str) -> Outcome {
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
    (validity, codes(&errors))
}

#[cfg(feature = "xsd11")]
fn typed_build(ss: &SchemaSet, root: ValidationRoot, xml: &str) -> Outcome {
    let (validity, errors, _) = typed_build_binding(ss, root, xml);
    (validity, errors)
}

/// The root element's binding in a typed build: its type and declaration.
#[cfg(feature = "xsd11")]
type RootBinding = Option<(TypeKey, Option<ElementKey>)>;

/// The typed build, with the root element's binding (type, declaration).
#[cfg(feature = "xsd11")]
fn typed_build_binding(
    ss: &SchemaSet,
    root: ValidationRoot,
    xml: &str,
) -> (Option<SchemaValidity>, Vec<String>, RootBinding) {
    use xsd_schema::document::{build_typed_document_with, TypedBuildOptions};
    use xsd_schema::navigator::DomNavigator;
    use xsd_schema::validation::NoopHandler;
    let v = validator(ss, true);
    let arena = bumpalo::Bump::new();
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    let (validity, binding) = {
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
            &mut NoopHandler,
            TypedBuildOptions::default(),
        )
        .expect("typed build completes");
        rt.end_validation().expect("end_validation");
        let mut nav = built.document.create_navigator();
        assert!(nav.move_to_first_child(), "a document element");
        let binding = nav.schema_binding().map(|b| (b.type_key, b.element_decl));
        (built.outcome.root_validity, binding)
    };
    (validity, codes(&errors), binding)
}

/// Every driver: the root's `[validity]` is `expected`, and an error whose
/// constraint code starts with `code` was reported (`None`: no error at
/// all).
fn check(
    ss: &SchemaSet,
    label: &str,
    root: ValidationRoot,
    xml: &str,
    expected: SchemaValidity,
    code: Option<&str>,
) {
    let runs = [
        ("stream", stream(ss, root, xml, true)),
        ("stream/no-psvi", stream(ss, root, xml, false)),
        ("navigator", dom(ss, root, xml)),
    ];
    #[cfg(feature = "xsd11")]
    let runs = runs.into_iter().chain([
        ("buffer", buffer(ss, root, xml)),
        ("typed build", typed_build(ss, root, xml)),
    ]);
    for (driver, (validity, errors)) in runs {
        assert_eq!(
            validity,
            Some(expected),
            "{label} {root:?} {driver}: root [validity] of {xml} (errors {errors:?})"
        );
        match code {
            Some(code) => assert!(
                errors.iter().any(|e| e.starts_with(code)),
                "{label} {root:?} {driver}: expected a {code} error for {xml}, got {errors:?}"
            ),
            None => assert!(
                errors.is_empty(),
                "{label} {root:?} {driver}: {xml}: errors {errors:?}"
            ),
        }
    }
}

// ── The push API: the root's SchemaInfo ──────────────────────────────────

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

/// The start and end `SchemaInfo` of a root `name` with `xsi:type` /
/// `xsi:nil` (resolved with no namespace bindings) and one child `child`
/// holding `text`, validated with `root`; plus the codes reported.
fn push_root(
    ss: &SchemaSet,
    root: ValidationRoot,
    name: &str,
    xsi_type: Option<&str>,
    xsi_nil: Option<&str>,
    child: Option<(&str, &str)>,
) -> (SchemaInfo, SchemaInfo, Vec<String>) {
    let v = validator(ss, true);
    let mut rt = v.start_run(Sink(Vec::new()));
    rt.set_validation_root(root);
    assert_eq!(rt.validation_root(), root);
    let ns = no_ns();
    let start = rt.validate_element(name, "", xsi_type, xsi_nil, &ns);
    rt.validate_end_of_attributes();
    if let Some((child, text)) = child {
        rt.validate_element(child, "", None, None, &ns);
        rt.validate_end_of_attributes();
        rt.validate_text(text);
        rt.validate_end_element();
    }
    let end = rt.validate_end_element();
    let _ = rt.end_validation();
    let errors = codes(&rt.sink.0);
    (start, end, errors)
}

// ── Type-driven validation ────────────────────────────────────────────────

/// The stipulated type is the governing type, whatever the root's name — a
/// top-level declaration of that name included (§3.3.4.6: no ·governing
/// element declaration· when "4.2 the processor has stipulated a type
/// definition for E").
#[test]
fn a_stipulated_type_governs_the_root() {
    for version in VERSIONS {
        let ss = schema(version);
        let label = format!("{version:?}");
        let root = ValidationRoot::Type(type_key(&ss, "T"));
        check(&ss, &label, root, "<any><n>1</n></any>", Valid, None);
        // `decl` is declared `xs:string`; the stipulated type wins.
        check(
            &ss,
            &label,
            root,
            "<decl a='2'><n>1</n></decl>",
            Valid,
            None,
        );
        check(
            &ss,
            &label,
            root,
            "<any><n>x</n></any>",
            Invalid,
            Some("cvc-datatype-valid"),
        );
        check(
            &ss,
            &label,
            root,
            "<any a='x'><n>1</n></any>",
            Invalid,
            Some("cvc-datatype-valid"),
        );
        check(
            &ss,
            &label,
            root,
            "<any/>",
            Invalid,
            Some("cvc-complex-type.2.4"),
        );

        let (start, end, errors) = push_root(&ss, root, "decl", None, None, Some(("n", "7")));
        assert!(errors.is_empty(), "{label}: {errors:?}");
        for info in [&start, &end] {
            assert_eq!(info.schema_type, Some(type_key(&ss, "T")), "{label}");
            assert_eq!(info.element_decl, None, "{label}");
            assert_eq!(info.type_source, Some(TypeSource::Stipulated), "{label}");
        }
        assert_eq!(start.validity, Valid, "{label}");
        assert_eq!(end.validity, Valid, "{label}");
        assert_eq!(
            end.validation_attempted,
            ValidationAttempted::Full,
            "{label}"
        );
        assert!(!end.is_nil, "{label}");
    }
}

/// §3.3.4.6, governing type definition clause "1 An ·instance-specified type
/// definition· which ·overrides· a type definition stipulated by the
/// processor"; §3.3.4.2: it overrides when ·validly substitutable·, "subject
/// to the blocking keywords in the union of K and T.{prohibited
/// substitutions}" (XSD 1.0 §3.3.4 clause 1.2.1.2.4: "validly derived from
/// that type definition given its {prohibited substitutions}"). An
/// `xsi:type` that does not is reported (`cvc-elt.4.1` / `.4.2` / `.4.3`,
/// as for a declared element) and the stipulated type stays governing.
#[test]
fn xsi_type_under_a_stipulated_type() {
    for version in VERSIONS {
        let ss = schema(version);
        let label = format!("{version:?}");
        let root = ValidationRoot::Type(type_key(&ss, "T"));
        check(
            &ss,
            &label,
            root,
            &format!("<r {XSI} xsi:type='T2'><n>1</n><m>s</m></r>"),
            Valid,
            None,
        );
        // Without `m`, T2's content is incomplete.
        check(
            &ss,
            &label,
            root,
            &format!("<r {XSI} xsi:type='T2'><n>1</n></r>"),
            Invalid,
            Some("cvc-complex-type.2.4"),
        );
        check(
            &ss,
            &label,
            root,
            &format!("<r {XSI} xsi:type='xs:string' xmlns:xs='http://www.w3.org/2001/XMLSchema'><n>1</n></r>"),
            Invalid,
            Some("cvc-elt.4.2"),
        );
        check(
            &ss,
            &label,
            root,
            &format!("<r {XSI} xsi:type='Nowhere'><n>1</n></r>"),
            Invalid,
            Some("cvc-elt.4.1"),
        );
        // T's {prohibited substitutions} = {extension}: blocked.
        let blocked = ValidationRoot::Type(type_key(&ss, "Blocked"));
        check(
            &ss,
            &label,
            blocked,
            &format!("<r {XSI} xsi:type='BlockedExt'><n>1</n><m>s</m></r>"),
            Invalid,
            Some("cvc-elt.4.3"),
        );
        check(&ss, &label, blocked, "<r><n>1</n></r>", Valid, None);

        let (start, end, errors) = push_root(&ss, root, "r", Some("T2"), None, Some(("n", "1")));
        // `m` is missing: an error, but the governing type is T2.
        assert!(
            errors.iter().any(|e| e == "cvc-complex-type.2.4"),
            "{label}: {errors:?}"
        );
        for info in [&start, &end] {
            assert_eq!(info.schema_type, Some(type_key(&ss, "T2")), "{label}");
            assert_eq!(info.type_source, Some(TypeSource::XsiType), "{label}");
            assert_eq!(info.element_decl, None, "{label}");
        }
    }
}

/// Element Locally Valid (Type) (§3.3.4.4) "2 If T is a complex type
/// definition, then T.{abstract} = false": an abstract stipulated type needs
/// an `xsi:type` that overrides it.
#[test]
fn an_abstract_stipulated_type_needs_xsi_type() {
    for version in VERSIONS {
        let ss = schema(version);
        let label = format!("{version:?}");
        let root = ValidationRoot::Type(type_key(&ss, "Abstract"));
        check(&ss, &label, root, "<r/>", Invalid, Some("cvc-type.2"));
        check(
            &ss,
            &label,
            root,
            &format!("<r {XSI} xsi:type='Concrete'/>"),
            Valid,
            None,
        );
    }
}

/// ·Nilled· is defined "with respect to some element declaration D" (§3.3.1:
/// "2 D.{nillable} = true"): with a stipulated type there is none, so
/// `xsi:nil` is only an `xs:boolean` attribute and the content is checked.
#[test]
fn a_stipulated_type_root_is_never_nilled() {
    for version in VERSIONS {
        let ss = schema(version);
        let label = format!("{version:?}");
        let root = ValidationRoot::Type(type_key(&ss, "T"));
        check(
            &ss,
            &label,
            root,
            &format!("<r {XSI} xsi:nil='true'/>"),
            Invalid,
            Some("cvc-complex-type.2.4"),
        );
        check(
            &ss,
            &label,
            root,
            &format!("<r {XSI} xsi:nil='true'><n>1</n></r>"),
            Valid,
            None,
        );
        let (start, end, _) = push_root(&ss, root, "r", None, Some("true"), Some(("n", "1")));
        assert!(!start.is_nil && !end.is_nil, "{label}");
    }
}

/// A simple stipulated type: Element Locally Valid (Type) clause 3.1 — no
/// attributes but `xsi:*`, no element children, the text String Valid; the
/// typed build types the root with it.
#[test]
fn a_simple_stipulated_type() {
    for version in VERSIONS {
        let ss = schema(version);
        let label = format!("{version:?}");
        let small = type_key(&ss, "Small");
        let root = ValidationRoot::Type(small);
        check(&ss, &label, root, "<r>5</r>", Valid, None);
        check(&ss, &label, root, "<r>50</r>", Invalid, Some("cvc-"));
        check(&ss, &label, root, "<r a='1'>5</r>", Invalid, Some("cvc-"));
        check(&ss, &label, root, "<r><n>1</n></r>", Invalid, Some("cvc-"));
        let (_, end, errors) = push_root(&ss, root, "r", None, None, None);
        // The empty string is not an xs:int.
        assert!(!errors.is_empty(), "{label}");
        assert_eq!(end.schema_type, Some(small), "{label}");
        #[cfg(feature = "xsd11")]
        {
            let (validity, _, binding) = typed_build_binding(&ss, root, "<r>5</r>");
            assert_eq!(validity, Some(Valid), "{label}");
            assert_eq!(binding, Some((small, None)), "{label}");
        }
    }
}

/// Validation Root Valid (ID/IDREF) (§3.3.4.5) and identity constraints of
/// the children still apply under a stipulated type; every root of a run is
/// assessed with it.
#[test]
fn every_root_of_a_run_is_assessed_with_the_stipulated_type() {
    let ss = schema(XsdVersion::V1_0);
    let v = validator(&ss, true);
    let mut rt = v.start_run(Sink(Vec::new()));
    rt.set_validation_root(ValidationRoot::Type(type_key(&ss, "T")));
    let ns = no_ns();
    for (name, text) in [("one", "1"), ("two", "x")] {
        rt.validate_element(name, "", None, None, &ns);
        rt.validate_end_of_attributes();
        rt.validate_element("n", "", None, None, &ns);
        rt.validate_end_of_attributes();
        rt.validate_text(text);
        rt.validate_end_element();
        let end = rt.validate_end_element();
        assert_eq!(end.schema_type, Some(type_key(&ss, "T")), "{name}");
        assert_eq!(
            end.validity,
            if text == "1" { Valid } else { Invalid },
            "{name}"
        );
    }
    rt.end_validation().expect("end_validation");
    assert!(codes(&rt.sink.0)
        .iter()
        .all(|c| c.starts_with("cvc-datatype-valid")));
}

/// A stipulated component the run's schema set does not hold aborts the run
/// (an operational failure, not a verdict on the instance).
#[test]
fn an_unknown_stipulated_component_aborts_the_run() {
    let ss = schema(XsdVersion::V1_0);
    let v = validator(&ss, true);
    for root in [
        ValidationRoot::Type(TypeKey::Complex(ComplexTypeKey::default())),
        ValidationRoot::Element(ElementKey::default()),
    ] {
        let mut rt = v.start_run(Sink(Vec::new()));
        rt.set_validation_root(root);
        rt.validate_element("r", "", None, None, &no_ns());
        let err = rt.end_validation().expect_err("operational failure");
        assert_eq!(err.constraint, "validation-root-unknown", "{root:?}");
    }
}

// ── Element-driven validation ─────────────────────────────────────────────

/// The stipulated declaration governs the root (§3.3.4.6 governing element
/// declaration clause "1 A declaration stipulated by the processor"); Element
/// Locally Valid (Element) (§3.3.4.3) "1 D is not ·absent· and E and D have
/// the same expanded name" (XSD 1.0 §5.2: the application "checks that its
/// {name} and {target namespace} match").
#[test]
fn a_stipulated_declaration_governs_the_root() {
    for version in VERSIONS {
        let ss = schema(version);
        let label = format!("{version:?}");
        let typed = element_key(&ss, "typed");
        let root = ValidationRoot::Element(typed);
        check(&ss, &label, root, "<typed><n>1</n></typed>", Valid, None);
        check(
            &ss,
            &label,
            root,
            "<typed><n>x</n></typed>",
            Invalid,
            Some("cvc-datatype-valid"),
        );
        // Another name: the declaration still governs, and the root is not
        // locally valid with respect to it.
        check(
            &ss,
            &label,
            root,
            "<decl><n>1</n></decl>",
            Invalid,
            Some("cvc-elt.1"),
        );
        check(
            &ss,
            &label,
            ValidationRoot::Element(element_key(&ss, "abstractDecl")),
            "<abstractDecl>s</abstractDecl>",
            Invalid,
            Some("cvc-elt.2"),
        );
        let (start, end, errors) = push_root(&ss, root, "typed", None, None, Some(("n", "1")));
        assert!(errors.is_empty(), "{label}: {errors:?}");
        for info in [&start, &end] {
            assert_eq!(info.element_decl, Some(typed), "{label}");
            assert_eq!(info.schema_type, Some(type_key(&ss, "T")), "{label}");
            assert_eq!(info.type_source, Some(TypeSource::Declaration), "{label}");
        }
        assert_eq!(
            end.validation_attempted,
            ValidationAttempted::Full,
            "{label}"
        );
        let (_, end, _) = push_root(&ss, root, "decl", None, None, Some(("n", "1")));
        assert_eq!(end.element_decl, Some(typed), "{label}");
        assert_eq!(end.validity, Invalid, "{label}");
    }
}

// ── Lax wildcard validation ───────────────────────────────────────────────

/// A declared root is strictly assessed, as by default.
#[test]
fn a_declared_root_under_lax_is_strictly_assessed() {
    for version in VERSIONS {
        let ss = schema(version);
        let label = format!("{version:?}");
        let lax = ValidationRoot::Lax;
        check(&ss, &label, lax, "<decl>s</decl>", Valid, None);
        check(
            &ss,
            &label,
            lax,
            "<typed><n>x</n></typed>",
            Invalid,
            Some("cvc-datatype-valid"),
        );
        check(
            &ss,
            &label,
            lax,
            &format!("<u {XSI} xsi:type='T'><n>1</n></u>"),
            Valid,
            None,
        );
        check(
            &ss,
            &label,
            lax,
            &format!("<u {XSI} xsi:type='T'/>"),
            Invalid,
            Some("cvc-complex-type.2.4"),
        );
    }
}

/// An undeclared root without `xsi:type` is laxly assessed — "E is locally
/// ·validated· with respect to ·xs:anyType·" and its attributes and children
/// are assessed (§3.3.4.6) — instead of `cvc-elt.1`; its `[validity]` is
/// notKnown (§3.3.5.1 clause 2) and its `[validation attempted]` none, or
/// partial when a child or attribute was assessed.
#[test]
fn an_undeclared_root_under_lax_is_laxly_assessed() {
    for version in VERSIONS {
        let ss = schema(version);
        let label = format!("{version:?}");
        let lax = ValidationRoot::Lax;
        check(&ss, &label, lax, "<u/>", NotKnown, None);
        check(
            &ss,
            &label,
            lax,
            "<u a='x'><v><w/></v>text</u>",
            NotKnown,
            None,
        );
        check(&ss, &label, lax, "<u><g>1</g></u>", NotKnown, None);
        // A declared child is strictly assessed, and its errors reported.
        check(
            &ss,
            &label,
            lax,
            "<u><g>x</g></u>",
            NotKnown,
            Some("cvc-datatype-valid"),
        );
        // The root's own attribute, assessed by its global declaration, is
        // invalid and reported; the laxly assessed root stays notKnown
        // (§3.3.5.1 clause 2), as an element a lax wildcard matched does.
        check(
            &ss,
            &label,
            lax,
            "<u ga='x'/>",
            NotKnown,
            Some("cvc-datatype-valid"),
        );
        // An xsi:type that names no type: reported as for a lax wildcard's
        // element, no cvc-elt.1; no ·instance-specified type definition·
        // (§3.3.4.1 clause 3), so the root is laxly assessed — notKnown.
        check(
            &ss,
            &label,
            lax,
            &format!("<u {XSI} xsi:type='Nowhere'/>"),
            NotKnown,
            Some("cvc-elt.4.1"),
        );

        let (start, end, errors) = push_root(&ss, lax, "u", None, None, None);
        assert!(errors.is_empty(), "{label}: {errors:?}");
        for info in [&start, &end] {
            assert_eq!(info.schema_type, None, "{label}");
            assert_eq!(info.element_decl, None, "{label}");
            assert_eq!(info.validity, NotKnown, "{label}");
        }
        assert_eq!(
            end.validation_attempted,
            ValidationAttempted::None,
            "{label}"
        );
        let (_, end, errors) = push_root(&ss, lax, "u", None, None, Some(("g", "1")));
        assert!(errors.is_empty(), "{label}: {errors:?}");
        assert_eq!(
            end.validation_attempted,
            ValidationAttempted::Partial,
            "{label}"
        );
        assert_eq!(end.validity, NotKnown, "{label}");
    }
}

/// The default (`ValidationRoot::Declaration`) is unchanged: an undeclared
/// root is `cvc-elt.1`.
#[test]
fn the_default_reports_an_undeclared_root() {
    for version in VERSIONS {
        let ss = schema(version);
        let label = format!("{version:?}");
        check(
            &ss,
            &label,
            ValidationRoot::Declaration,
            "<u/>",
            Invalid,
            Some("cvc-elt.1"),
        );
        let v = validator(&ss, true);
        let rt = v.start_run(Sink(Vec::new()));
        assert_eq!(rt.validation_root(), ValidationRoot::Declaration);
        assert_eq!(ValidationRoot::default(), ValidationRoot::Declaration);
        let (start, end, errors) = push_root(
            &ss,
            ValidationRoot::Declaration,
            "typed",
            None,
            None,
            Some(("n", "1")),
        );
        assert!(errors.is_empty(), "{label}: {errors:?}");
        for info in [&start, &end] {
            assert_eq!(info.type_source, Some(TypeSource::Declaration), "{label}");
        }
    }
}
