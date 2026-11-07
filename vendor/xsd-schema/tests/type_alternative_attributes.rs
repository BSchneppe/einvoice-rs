//! The attributes an XSD 1.1 type alternative's test sees.
//!
//! Structures 1.1 §3.12.4: "1 An instance of the [XDM] data model is
//! constructed as follows: 1.1 An information set is constructed by copying
//! the base information set properties … of the following information items:
//! 1.1.1 E itself. 1.1.2 E's [attributes] (but not its [children]). 1.1.3 E's
//! [inherited attributes] which do not have the same expanded names as any of
//! E's [attributes]." E's [attributes] are all of them, the `xsi:` attributes
//! (`xsi:type`, `xsi:nil`, `xsi:schemaLocation`,
//! `xsi:noNamespaceSchemaLocation`, §3.2.7) and, when the validator is told
//! to accept them unvalidated (`ValidationFlags::ALLOW_XML_ATTRIBUTES`),
//! the `xml:` ones included. They are untyped there ("The element E and its
//! [attributes] will be represented in the data model instance by nodes
//! labeled as untyped").
//!
//! Each case runs on the push API, the streaming driver with and without PSVI
//! typed values, the navigator driver and the typed build.
#![cfg(feature = "xsd11")]

use xsd_schema::namespace::context::NamespaceContextSnapshot;
use xsd_schema::validation::{
    drive_navigator, drive_quick_xml, CollectingValidationSink, SchemaValidator, SchemaValidity,
    ValidationFlags,
};
use xsd_schema::{RoXmlNavigator, SchemaSet, SchemaSetBuilder, TypeKey, XsdVersion};

const XSI: &str = "http://www.w3.org/2001/XMLSchema-instance";

/// `{TEST}` is the first alternative's test: it selects `t:Yes` (content
/// "yes"), the default alternative `t:No` (content "no"). `lang` is
/// inheritable, carried by `p` and allowed on `e`.
const SCHEMA: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:t="urn:t"
    xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
    targetNamespace="urn:t" elementFormDefault="qualified">
  <xs:complexType name="B"><xs:simpleContent>
    <xs:extension base="xs:string">
      <xs:attribute name="a"/>
      <xs:attribute name="lang" inheritable="true"/>
    </xs:extension>
  </xs:simpleContent></xs:complexType>
  <xs:complexType name="Yes"><xs:simpleContent>
    <xs:restriction base="t:B"><xs:enumeration value="yes"/></xs:restriction>
  </xs:simpleContent></xs:complexType>
  <xs:complexType name="No"><xs:simpleContent>
    <xs:restriction base="t:B"><xs:enumeration value="no"/></xs:restriction>
  </xs:simpleContent></xs:complexType>
  <xs:element name="e" type="t:B" nillable="true">
    <xs:alternative test="{TEST}" type="t:Yes"/>
    <xs:alternative type="t:No"/>
  </xs:element>
  <xs:element name="p">
    <xs:complexType>
      <xs:sequence><xs:element ref="t:e"/></xs:sequence>
      <xs:attribute name="lang" type="xs:string" inheritable="true"/>
    </xs:complexType>
  </xs:element>
</xs:schema>"#;

fn compile(test: &str) -> SchemaSet {
    SchemaSetBuilder::with_version(XsdVersion::V1_1)
        .add_source(
            &SCHEMA.replace("{TEST}", test),
            "file:///type_alternative_attributes.xsd",
        )
        .expect("schema source accepted")
        .compile()
        .unwrap_or_else(|e| panic!("{test}: schema compiles: {e}"))
        .into_schema_set()
}

fn flags(psvi: bool, xml_attributes: bool) -> ValidationFlags {
    let mut f = ValidationFlags::default();
    if !psvi {
        f &= !ValidationFlags::BUILD_PSVI_TYPED_VALUES;
    }
    if xml_attributes {
        f |= ValidationFlags::ALLOW_XML_ATTRIBUTES;
    }
    f
}

/// One element `<t:e ATTRS>CONTENT</t:e>`, optionally inside `<t:p
/// lang="en">`.
struct Case {
    test: &'static str,
    attrs: &'static str,
    in_p: bool,
    /// Whether the first alternative (`t:Yes`) is selected.
    yes: bool,
}

const CASES: &[Case] = &[
    // xsi:nil (the four xsi: attributes are E's [attributes]).
    Case {
        test: "exists(@xsi:nil)",
        attrs: r#"xsi:nil="false""#,
        in_p: false,
        yes: true,
    },
    Case {
        test: "exists(@xsi:nil)",
        attrs: "",
        in_p: false,
        yes: false,
    },
    Case {
        test: "@xsi:nil = 'false'",
        attrs: r#"xsi:nil="false""#,
        in_p: false,
        yes: true,
    },
    // xsi:type: its own value, as an untyped string.
    Case {
        test: "@xsi:type = 't:Yes'",
        attrs: r#"xsi:type="t:Yes""#,
        in_p: false,
        yes: true,
    },
    Case {
        test: "exists(@xsi:schemaLocation)",
        attrs: r#"xsi:schemaLocation="urn:x x.xsd""#,
        in_p: false,
        yes: true,
    },
    Case {
        test: "exists(@xsi:noNamespaceSchemaLocation)",
        attrs: r#"xsi:noNamespaceSchemaLocation="x.xsd""#,
        in_p: false,
        yes: true,
    },
    // Every attribute counts, untyped.
    Case {
        test: "count(@*) = 2",
        attrs: r#"a="x" xsi:nil="false""#,
        in_p: false,
        yes: true,
    },
    Case {
        test: "count(@*) = 2",
        attrs: r#"a="x""#,
        in_p: false,
        yes: false,
    },
    Case {
        test: "every $a in @* satisfies $a instance of attribute(*, xs:untypedAtomic)",
        attrs: r#"a="x" xsi:nil="false""#,
        in_p: false,
        yes: true,
    },
    // Inherited attributes (clause 1.1.3) beside the xsi: ones; E's own
    // attribute of the same name hides the inherited one.
    Case {
        test: "@lang = 'en' and exists(@xsi:nil)",
        attrs: r#"xsi:nil="false""#,
        in_p: true,
        yes: true,
    },
    Case {
        test: "count(@*) = 2",
        attrs: r#"xsi:nil="false""#,
        in_p: true,
        yes: true,
    },
    Case {
        test: "@lang = 'en' and exists(@xsi:nil)",
        attrs: r#"lang="fr" xsi:nil="false""#,
        in_p: true,
        yes: false,
    },
    Case {
        test: "@lang = 'en'",
        attrs: "",
        in_p: true,
        yes: true,
    },
];

/// The instance of a case with the content its selected type admits when the
/// alternative is the expected one.
fn instance(case: &Case, xml_attr: &str) -> String {
    let content = if case.yes { "yes" } else { "no" };
    let e = format!(
        r#"<t:e xmlns:t="urn:t" xmlns:xsi="{XSI}" {} {xml_attr}>{content}</t:e>"#,
        case.attrs
    );
    if case.in_p {
        format!(r#"<t:p xmlns:t="urn:t" lang="en">{e}</t:p>"#)
    } else {
        e
    }
}

/// The verdicts of every driver: `(driver, valid, errors)`.
fn drivers(ss: &SchemaSet, xml: &str, xml_attributes: bool) -> Vec<(&'static str, bool, String)> {
    let mut out = Vec::new();
    for psvi in [true, false] {
        let v = SchemaValidator::new(ss, flags(psvi, xml_attributes));
        let (mut errors, mut warnings) = (Vec::new(), Vec::new());
        let root = {
            let mut rt = v.start_run(CollectingValidationSink {
                errors: &mut errors,
                warnings: &mut warnings,
            });
            drive_quick_xml(xml.as_bytes(), &mut rt, ss)
                .expect("drive")
                .root_validity
        };
        out.push((
            if psvi { "stream" } else { "stream/no-psvi" },
            root == Some(SchemaValidity::Valid) && errors.is_empty(),
            format!("{errors:?}"),
        ));
    }
    let v = SchemaValidator::new(ss, flags(true, xml_attributes));
    let doc = roxmltree::Document::parse(xml).expect("well-formed");
    let nav = RoXmlNavigator::new(&doc);
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    let root = {
        let mut rt = v.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        drive_navigator(&nav, &mut rt, ss)
            .expect("drive")
            .root_validity
    };
    out.push((
        "navigator",
        root == Some(SchemaValidity::Valid) && errors.is_empty(),
        format!("{errors:?}"),
    ));
    {
        use xsd_schema::document::{build_typed_document_with, TypedBuildOptions};
        use xsd_schema::validation::NoopHandler;
        let arena = bumpalo::Bump::new();
        let (mut errors, mut warnings) = (Vec::new(), Vec::new());
        let mut rt = v.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        let built = build_typed_document_with(
            xml.as_bytes(),
            &arena,
            ss,
            &mut rt,
            &mut NoopHandler,
            TypedBuildOptions::default(),
        )
        .expect("typed build completes");
        let valid = built.outcome.root_validity == Some(SchemaValidity::Valid);
        drop(rt);
        out.push((
            "typed build",
            valid && errors.is_empty(),
            format!("{errors:?}"),
        ));
    }
    out
}

/// The push API: the type `validate_end_of_attributes` reports selected.
fn selected_by_push(ss: &SchemaSet, attrs: &[(&str, &str, &str)]) -> Option<TypeKey> {
    let v = SchemaValidator::new(ss, ValidationFlags::default());
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    let mut rt = v.start_run(CollectingValidationSink {
        errors: &mut errors,
        warnings: &mut warnings,
    });
    let ns = NamespaceContextSnapshot {
        default_ns: None,
        bindings: vec![
            (ss.name_table.add("t"), ss.name_table.add("urn:t")),
            (ss.name_table.add("xsi"), ss.name_table.add(XSI)),
        ],
    };
    let xsi = |name: &str| {
        attrs
            .iter()
            .find(|(ns, local, _)| *ns == XSI && *local == name)
            .map(|(_, _, v)| *v)
    };
    let _ = rt.validate_element("e", "urn:t", xsi("type"), xsi("nil"), &ns);
    for (ns, local, value) in attrs {
        let _ = rt.validate_attribute(local, ns, value);
    }
    let selected = rt.validate_end_of_attributes().schema_type;
    let _ = rt.take_deferred_attribute_results();
    selected
}

#[test]
fn a_type_alternative_sees_the_xsi_attributes() {
    let mut wrong = Vec::new();
    for case in CASES {
        let ss = compile(case.test);
        let xml = instance(case, "");
        for (driver, valid, errors) in drivers(&ss, &xml, false) {
            if !valid {
                wrong.push(format!(
                    "{} on {xml} ({driver}): the {} alternative was not selected: {errors}",
                    case.test,
                    if case.yes { "first" } else { "default" }
                ));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "{} wrong:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

/// The push API reports the selected type at the end of the attributes.
#[test]
fn the_push_api_selects_with_the_xsi_attributes() {
    let ss = compile("exists(@xsi:nil) and @a = 'x'");
    let t = |local: &str| {
        ss.lookup_type(Some(ss.name_table.add("urn:t")), ss.name_table.add(local))
            .expect("a type")
    };
    assert_eq!(
        selected_by_push(&ss, &[("", "a", "x"), (XSI, "nil", "false")]),
        Some(t("Yes"))
    );
    assert_eq!(selected_by_push(&ss, &[("", "a", "x")]), Some(t("No")));
    assert_eq!(
        selected_by_push(&ss, &[(XSI, "nil", "false")]),
        Some(t("No"))
    );
}

/// `xml:` attributes the validator accepts without a declaration
/// (`ALLOW_XML_ATTRIBUTES`) are E's [attributes] too.
#[test]
fn a_type_alternative_sees_accepted_xml_attributes() {
    let ss = compile("exists(@xml:lang)");
    for (attr, yes) in [(r#"xml:lang="en""#, true), ("", false)] {
        let case = Case {
            test: "exists(@xml:lang)",
            attrs: "",
            in_p: false,
            yes,
        };
        let xml = instance(&case, attr);
        for (driver, valid, errors) in drivers(&ss, &xml, true) {
            assert!(valid, "{xml} ({driver}): {errors}");
        }
    }
}
