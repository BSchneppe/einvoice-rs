//! The [nil] property of an element with `xsi:nil="true"`.
//!
//! Structures 1.1 §3.3.5.3 (Element Declaration Information Set Contribution):
//! "[nil] true if clause 3.2.3 of Element Locally Valid (Element) (§3.3.4.3)
//! above is satisfied, otherwise false". §3.3.4.3 clause 3.2.3: "E has xsi:nil
//! = true (that is, E is ·nilled·), and all of the following are true: 3.2.3.1
//! E has no character or element information item [children]. 3.2.3.2 D has
//! no {value constraint} with {variety} = fixed." §3.3.1: "An element
//! information item E is nilled with respect to some element declaration D if
//! and only if all of the following are true: 1 E has xsi:nil = true. 2
//! D.{nillable} = true." XSD 1.0 §3.3.4 / §3.3.5 say the same.
//!
//! So an element of a nillable declaration with `xsi:nil="true"` and content
//! is ·nilled· — its content is not checked against its type, its `[validity]`
//! is invalid (`cvc-elt.3.2.1`) — but its [nil] is false. The push API reports
//! [nil] in the `SchemaInfo` of the element's end (`validate_end_element`); the
//! `SchemaInfo` of its start, before any content, says whether it is ·nilled·.
//! The streaming and navigator drivers report the error and the invalid root;
//! a typed tree does not nil the element (XDM 1.0 §6.2.4: nilled needs
//! `[validity]` valid).

use xsd_schema::namespace::context::NamespaceContextSnapshot;
use xsd_schema::validation::{
    drive_navigator, drive_quick_xml, CollectingValidationSink, SchemaInfo, SchemaValidator,
    SchemaValidity, ValidationError, ValidationFlags,
};
use xsd_schema::{RoXmlNavigator, SchemaSet, SchemaSetBuilder, XsdVersion};

const VERSIONS: [XsdVersion; 2] = [XsdVersion::V1_0, XsdVersion::V1_1];
const XSI: &str = "http://www.w3.org/2001/XMLSchema-instance";

const SCHEMA: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:complexType name="T"><xs:sequence>
    <xs:element name="n" type="xs:int" minOccurs="0"/>
  </xs:sequence></xs:complexType>
  <xs:element name="bag">
    <xs:complexType><xs:sequence>
      <xs:element name="c" type="T" nillable="true" minOccurs="0"/>
      <xs:element name="s" type="xs:string" nillable="true" minOccurs="0"/>
    </xs:sequence></xs:complexType>
  </xs:element>
</xs:schema>"#;

fn schema(version: XsdVersion) -> SchemaSet {
    SchemaSetBuilder::with_version(version)
        .add_source(SCHEMA, "file:///nil_property.xsd")
        .expect("schema source accepted")
        .compile()
        .unwrap_or_else(|e| panic!("{version:?}: schema compiles: {e}"))
        .into_schema_set()
}

fn flags(psvi: bool) -> ValidationFlags {
    let mut f = ValidationFlags::default();
    if !psvi {
        f &= !ValidationFlags::BUILD_PSVI_TYPED_VALUES;
    }
    f
}

fn codes(errors: &[ValidationError]) -> Vec<String> {
    errors.iter().map(|e| e.constraint.to_string()).collect()
}

/// What `<bag><NAME xsi:nil="true">CONTENT</NAME></bag>` gives through the
/// push API: the start and the end `SchemaInfo` of NAME, and the codes.
/// `child` adds an `n` element child, `text` a text child.
fn push(
    ss: &SchemaSet,
    name: &str,
    child: bool,
    text: Option<&str>,
    psvi: bool,
) -> (SchemaInfo, SchemaInfo, Vec<String>) {
    let v = SchemaValidator::new(ss, flags(psvi));
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    let ns = NamespaceContextSnapshot {
        default_ns: None,
        bindings: Vec::new(),
    };
    let (start, end) = {
        let mut rt = v.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        rt.validate_element("bag", "", None, None, &ns);
        rt.validate_end_of_attributes();
        let start = rt.validate_element(name, "", None, Some("true"), &ns);
        rt.validate_attribute("nil", XSI, "true");
        rt.validate_end_of_attributes();
        if child {
            rt.validate_element("n", "", None, None, &ns);
            rt.validate_end_of_attributes();
            rt.validate_text("1");
            rt.validate_end_element();
        }
        if let Some(text) = text {
            rt.validate_text(text);
        }
        let end = rt.validate_end_element();
        rt.validate_end_element();
        let _ = rt.end_validation();
        (start, end)
    };
    (start, end, codes(&errors))
}

/// The end `SchemaInfo::is_nil` is [nil]: false when the nilled element has
/// content, true when it has none; the start one says it is ·nilled·.
#[test]
fn nil_is_false_at_the_end_of_a_nilled_element_with_content() {
    let mut wrong = Vec::new();
    for version in VERSIONS {
        let ss = schema(version);
        for psvi in [true, false] {
            let cases: [(&str, &str, bool, Option<&str>, bool); 5] = [
                ("complex, element child", "c", true, None, false),
                ("complex, text", "c", false, Some("x"), false),
                ("simple, text", "s", false, Some("x"), false),
                ("complex, empty", "c", false, None, true),
                ("simple, empty", "s", false, None, true),
            ];
            for (label, name, child, text, nil) in cases {
                let (start, end, errors) = push(&ss, name, child, text, psvi);
                let label = format!("{version:?} psvi={psvi} {label}");
                if !start.is_nil {
                    wrong.push(format!(
                        "{label}: start is_nil false (the element is nilled)"
                    ));
                }
                if end.is_nil != nil {
                    wrong.push(format!(
                        "{label}: end is_nil {}, expected {nil}",
                        end.is_nil
                    ));
                }
                let expected_validity = if nil {
                    SchemaValidity::Valid
                } else {
                    SchemaValidity::Invalid
                };
                if end.validity != expected_validity {
                    wrong.push(format!("{label}: [validity] {:?}", end.validity));
                }
                let reported = errors.iter().any(|c| c == "cvc-elt.3.2.1");
                if reported == nil {
                    wrong.push(format!(
                        "{label}: cvc-elt.3.2.1 reported: {reported} ({errors:?})"
                    ));
                }
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The drivers: `cvc-elt.3.2.1` and an invalid root for the element with
/// content, valid for the empty one.
#[test]
fn the_drivers_report_the_nilled_element_with_content() {
    let with_content = format!(r#"<bag xmlns:xsi="{XSI}"><c xsi:nil="true"><n>1</n></c></bag>"#);
    let with_text = format!(r#"<bag xmlns:xsi="{XSI}"><s xsi:nil="true">x</s></bag>"#);
    let empty = format!(r#"<bag xmlns:xsi="{XSI}"><c xsi:nil="true"/><s xsi:nil="true"/></bag>"#);
    for version in VERSIONS {
        let ss = schema(version);
        for (xml, valid) in [(&with_content, false), (&with_text, false), (&empty, true)] {
            let mut runs = Vec::new();
            for psvi in [true, false] {
                let v = SchemaValidator::new(&ss, flags(psvi));
                let (mut errors, mut warnings) = (Vec::new(), Vec::new());
                let root = {
                    let mut rt = v.start_run(CollectingValidationSink {
                        errors: &mut errors,
                        warnings: &mut warnings,
                    });
                    drive_quick_xml(xml.as_bytes(), &mut rt, &ss)
                        .expect("drive")
                        .root_validity
                };
                runs.push((
                    if psvi { "stream" } else { "stream/no-psvi" },
                    root,
                    codes(&errors),
                ));
            }
            {
                let v = SchemaValidator::new(&ss, flags(true));
                let doc = roxmltree::Document::parse(xml).expect("well-formed");
                let nav = RoXmlNavigator::new(&doc);
                let (mut errors, mut warnings) = (Vec::new(), Vec::new());
                let root = {
                    let mut rt = v.start_run(CollectingValidationSink {
                        errors: &mut errors,
                        warnings: &mut warnings,
                    });
                    drive_navigator(&nav, &mut rt, &ss)
                        .expect("drive")
                        .root_validity
                };
                runs.push(("navigator", root, codes(&errors)));
            }
            for (driver, root, errors) in runs {
                let want = if valid {
                    SchemaValidity::Valid
                } else {
                    SchemaValidity::Invalid
                };
                assert_eq!(root, Some(want), "{version:?} {driver}: {xml}: {errors:?}");
                // (The `n` child of the nilled `c` is also reported as
                // undeclared, `cvc-elt.1` — not this item's concern.)
                assert_eq!(
                    errors.first().map(String::as_str),
                    (!valid).then_some("cvc-elt.3.2.1"),
                    "{version:?} {driver}: {xml}: {errors:?}"
                );
            }
        }
    }
}

/// The typed build nils the empty element only.
#[cfg(feature = "xsd11")]
#[test]
fn a_typed_tree_nils_only_the_empty_element() {
    use xsd_schema::document::{build_typed_document_with, TypedBuildOptions};
    use xsd_schema::navigator::{DomNavigator, DomNodeType, TypedValue};
    use xsd_schema::validation::NoopHandler;
    let xml = format!(
        r#"<bag xmlns:xsi="{XSI}"><c xsi:nil="true"><n>1</n></c><s xsi:nil="true"/></bag>"#
    );
    for version in VERSIONS {
        let ss = schema(version);
        let v = SchemaValidator::new(&ss, flags(true));
        let arena = bumpalo::Bump::new();
        let (mut errors, mut warnings) = (Vec::new(), Vec::new());
        let mut rt = v.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        let built = build_typed_document_with(
            xml.as_bytes(),
            &arena,
            &ss,
            &mut rt,
            &mut NoopHandler,
            TypedBuildOptions::default(),
        )
        .expect("typed build completes");
        let mut nilled = Vec::new();
        let mut stack = vec![built.document.create_navigator()];
        while let Some(mut node) = stack.pop() {
            if node.node_type() == DomNodeType::Element {
                nilled.push((
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
        drop(rt);
        assert_eq!(
            nilled,
            [("bag", false), ("c", false), ("n", false), ("s", true)]
                .map(|(n, b)| (n.to_string(), b))
                .to_vec(),
            "{version:?}"
        );
        assert_eq!(
            codes(&errors).first().map(String::as_str),
            Some("cvc-elt.3.2.1"),
            "{version:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Clause 3.2.3.2: a nilled element whose declaration has a fixed value.
//
// Structures 1.1 §3.3.4.3 clause 3.2.3: "E has xsi:nil = true (that is, E is
// ·nilled·), and all of the following are true: … 3.2.3.2 D has no {value
// constraint} with {variety} = fixed." XSD 1.0 §3.3.4 clause 3.2: "3.2.2 There
// must be no fixed {value constraint}" (`cvc-elt.3.2.2`). Such an element
// stays ·nilled· (§3.3.1), so its content is not checked against its type nor
// its fixed value (clause 5.2.2 needs "E is not ·nilled·"), but it is invalid
// and its [nil] is false (§3.3.5.3). A `default` value constraint does not
// matter (W3C `addB065.i`: a mixed type with `fixed="abc"`).
// ---------------------------------------------------------------------------

const FIXED_SCHEMA: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:complexType name="M" mixed="true">
    <xs:attribute name="id" type="xs:string"/>
  </xs:complexType>
  <xs:complexType name="S"><xs:simpleContent>
    <xs:extension base="xs:string"><xs:attribute name="id" type="xs:string"/></xs:extension>
  </xs:simpleContent></xs:complexType>
  <xs:element name="bag">
    <xs:complexType><xs:sequence>
      <xs:element name="f" type="xs:string" nillable="true" fixed="x" minOccurs="0"/>
      <xs:element name="m" type="M" nillable="true" fixed="abc" minOccurs="0"/>
      <xs:element name="sc" type="S" nillable="true" fixed="x" minOccurs="0"/>
      <xs:element name="d" type="xs:string" nillable="true" default="x" minOccurs="0"/>
    </xs:sequence></xs:complexType>
  </xs:element>
</xs:schema>"#;

fn fixed_schema(version: XsdVersion) -> SchemaSet {
    SchemaSetBuilder::with_version(version)
        .add_source(FIXED_SCHEMA, "file:///nil_fixed.xsd")
        .expect("schema source accepted")
        .compile()
        .unwrap_or_else(|e| panic!("{version:?}: schema compiles: {e}"))
        .into_schema_set()
}

/// The push API: `<bag><NAME xsi:nil="NIL">TEXT</NAME></bag>` — the end
/// `SchemaInfo` of NAME and the codes.
fn push_fixed(
    ss: &SchemaSet,
    name: &str,
    nil: Option<&str>,
    text: Option<&str>,
    psvi: bool,
) -> (SchemaInfo, SchemaInfo, Vec<String>) {
    let v = SchemaValidator::new(ss, flags(psvi));
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    let ns = NamespaceContextSnapshot {
        default_ns: None,
        bindings: Vec::new(),
    };
    let (start, end) = {
        let mut rt = v.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        rt.validate_element("bag", "", None, None, &ns);
        rt.validate_end_of_attributes();
        let start = rt.validate_element(name, "", None, nil, &ns);
        if let Some(nil) = nil {
            rt.validate_attribute("nil", XSI, nil);
        }
        rt.validate_end_of_attributes();
        if let Some(text) = text {
            rt.validate_text(text);
        }
        let end = rt.validate_end_element();
        rt.validate_end_element();
        let _ = rt.end_validation();
        (start, end)
    };
    (start, end, codes(&errors))
}

/// (label, element, xsi:nil, text, valid, end \[nil\], first code)
type NilCase = (
    &'static str,
    &'static str,
    Option<&'static str>,
    Option<&'static str>,
    bool,
    bool,
    Option<&'static str>,
);

/// Push API, both versions, with and without PSVI typed values: a nilled
/// element of a declaration with `fixed` is invalid (`cvc-elt.3.2.2`) and its
/// end [nil] is false; the neighbours (a `default`, `xsi:nil="false"`, no
/// `xsi:nil`) keep their verdicts.
#[test]
fn a_nilled_element_with_a_fixed_value_is_invalid() {
    // (label, element, xsi:nil, text, valid, end [nil], first code)
    let cases: [NilCase; 9] = [
        (
            "simple, fixed, nil",
            "f",
            Some("true"),
            None,
            false,
            false,
            Some("cvc-elt.3.2.2"),
        ),
        (
            "mixed, fixed, nil",
            "m",
            Some("true"),
            None,
            false,
            false,
            Some("cvc-elt.3.2.2"),
        ),
        (
            "simple content, fixed, nil",
            "sc",
            Some("true"),
            None,
            false,
            false,
            Some("cvc-elt.3.2.2"),
        ),
        (
            "simple, fixed, nil=1",
            "f",
            Some("1"),
            None,
            false,
            false,
            Some("cvc-elt.3.2.2"),
        ),
        (
            "simple, default, nil",
            "d",
            Some("true"),
            None,
            true,
            true,
            None,
        ),
        (
            "simple, fixed, nil=false, the value",
            "f",
            Some("false"),
            Some("x"),
            true,
            false,
            None,
        ),
        (
            "simple, fixed, no nil, the value",
            "f",
            None,
            Some("x"),
            true,
            false,
            None,
        ),
        (
            "simple, fixed, no nil, empty",
            "f",
            None,
            None,
            true,
            false,
            None,
        ),
        (
            "simple, fixed, no nil, another value",
            "f",
            None,
            Some("y"),
            false,
            false,
            Some("cvc-elt.5.2.2"),
        ),
    ];
    let mut wrong = Vec::new();
    for version in VERSIONS {
        let ss = fixed_schema(version);
        for psvi in [true, false] {
            for (label, name, nil, text, valid, end_nil, code) in cases {
                let (_start, end, errors) = push_fixed(&ss, name, nil, text, psvi);
                let label = format!("{version:?} psvi={psvi} {label}");
                let want = if valid {
                    SchemaValidity::Valid
                } else {
                    SchemaValidity::Invalid
                };
                if end.validity != want {
                    wrong.push(format!(
                        "{label}: [validity] {:?} ({errors:?})",
                        end.validity
                    ));
                }
                if end.is_nil != end_nil {
                    wrong.push(format!("{label}: end is_nil {}", end.is_nil));
                }
                if errors.first().map(String::as_str) != code {
                    wrong.push(format!("{label}: codes {errors:?}, want first {code:?}"));
                }
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The streaming driver (with and without PSVI typed values) and the
/// navigator driver agree: `cvc-elt.3.2.2` and an invalid root.
#[test]
fn the_drivers_report_a_nilled_element_with_a_fixed_value() {
    let cases = [
        (
            format!(r#"<bag xmlns:xsi="{XSI}"><f xsi:nil="true"/></bag>"#),
            false,
        ),
        (
            format!(r#"<bag xmlns:xsi="{XSI}"><m xsi:nil="true" id="a"/></bag>"#),
            false,
        ),
        (
            format!(r#"<bag xmlns:xsi="{XSI}"><sc xsi:nil="true"/></bag>"#),
            false,
        ),
        (
            format!(r#"<bag xmlns:xsi="{XSI}"><f xsi:nil="false">x</f><d xsi:nil="true"/></bag>"#),
            true,
        ),
        (
            format!(r#"<bag xmlns:xsi="{XSI}"><f>x</f><m>abc</m><sc>x</sc></bag>"#),
            true,
        ),
    ];
    for version in VERSIONS {
        let ss = fixed_schema(version);
        for (xml, valid) in &cases {
            let mut runs = Vec::new();
            for psvi in [true, false] {
                let v = SchemaValidator::new(&ss, flags(psvi));
                let (mut errors, mut warnings) = (Vec::new(), Vec::new());
                let root = {
                    let mut rt = v.start_run(CollectingValidationSink {
                        errors: &mut errors,
                        warnings: &mut warnings,
                    });
                    drive_quick_xml(xml.as_bytes(), &mut rt, &ss)
                        .expect("drive")
                        .root_validity
                };
                runs.push((
                    if psvi { "stream" } else { "stream/no-psvi" },
                    root,
                    codes(&errors),
                ));
            }
            {
                let v = SchemaValidator::new(&ss, flags(true));
                let doc = roxmltree::Document::parse(xml).expect("well-formed");
                let nav = RoXmlNavigator::new(&doc);
                let (mut errors, mut warnings) = (Vec::new(), Vec::new());
                let root = {
                    let mut rt = v.start_run(CollectingValidationSink {
                        errors: &mut errors,
                        warnings: &mut warnings,
                    });
                    drive_navigator(&nav, &mut rt, &ss)
                        .expect("drive")
                        .root_validity
                };
                runs.push(("navigator", root, codes(&errors)));
            }
            for (driver, root, errors) in runs {
                let want = if *valid {
                    SchemaValidity::Valid
                } else {
                    SchemaValidity::Invalid
                };
                assert_eq!(root, Some(want), "{version:?} {driver}: {xml}: {errors:?}");
                assert_eq!(
                    errors,
                    if *valid {
                        Vec::<String>::new()
                    } else {
                        vec!["cvc-elt.3.2.2".to_string()]
                    },
                    "{version:?} {driver}: {xml}"
                );
            }
        }
    }
}

/// The typed build does not nil such an element (XDM 1.0 §6.2.4: nilled
/// needs `[validity]` valid and [nil] true) and reports `cvc-elt.3.2.2`.
#[cfg(feature = "xsd11")]
#[test]
fn a_typed_tree_does_not_nil_an_element_with_a_fixed_value() {
    use xsd_schema::document::{build_typed_document_with, TypedBuildOptions};
    use xsd_schema::navigator::{DomNavigator, DomNodeType, TypedValue};
    use xsd_schema::validation::NoopHandler;
    let xml = format!(r#"<bag xmlns:xsi="{XSI}"><f xsi:nil="true"/><d xsi:nil="true"/></bag>"#);
    for version in VERSIONS {
        let ss = fixed_schema(version);
        let v = SchemaValidator::new(&ss, flags(true));
        let arena = bumpalo::Bump::new();
        let (mut errors, mut warnings) = (Vec::new(), Vec::new());
        let mut rt = v.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        let built = build_typed_document_with(
            xml.as_bytes(),
            &arena,
            &ss,
            &mut rt,
            &mut NoopHandler,
            TypedBuildOptions::default(),
        )
        .expect("typed build completes");
        let mut nilled = Vec::new();
        let mut node = built.document.create_navigator();
        assert!(node.move_to_first_child(), "bag");
        assert!(node.move_to_first_child(), "f");
        loop {
            if node.node_type() == DomNodeType::Element {
                nilled.push((
                    node.local_name().to_string(),
                    matches!(node.typed_value(), TypedValue::Nilled),
                ));
            }
            if !node.move_to_next_sibling() {
                break;
            }
        }
        drop(rt);
        assert_eq!(
            nilled,
            [("f", false), ("d", true)]
                .map(|(n, b)| (n.to_string(), b))
                .to_vec(),
            "{version:?}"
        );
        assert_eq!(codes(&errors), ["cvc-elt.3.2.2"], "{version:?}");
    }
}
