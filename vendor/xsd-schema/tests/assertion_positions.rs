//! Where `<xs:assert>` may appear in a complex type definition.
//!
//! Structures 1.1 §3.4.2 (XML Representation of Complex Type Definition Schema
//! Components) gives the content models:
//!
//! - `<complexType>`: "(annotation?, (simpleContent | complexContent |
//!   (openContent?, (group | all | choice | sequence)?, ((attribute |
//!   attributeGroup)*, anyAttribute?), assert*)))";
//! - `<simpleContent>` / `<restriction>`: "(annotation?, (simpleType?,
//!   (minExclusive | … | pattern | assertion | {any with namespace:
//!   ##other})*)?, ((attribute | attributeGroup)*, anyAttribute?), assert*)";
//! - `<simpleContent>` / `<extension>`: "(annotation?, ((attribute |
//!   attributeGroup)*, anyAttribute?), assert*)";
//! - `<complexContent>` / `<restriction>`: "(annotation?, openContent?, (group
//!   | all | choice | sequence)?, ((attribute | attributeGroup)*,
//!   anyAttribute?), assert*)";
//! - `<complexContent>` / `<extension>`: "(annotation?, openContent?, ((group
//!   | all | choice | sequence)?, ((attribute | attributeGroup)*,
//!   anyAttribute?), assert*))".
//!
//! So assertions come last, after the attribute declarations, the attribute
//! group references and the attribute wildcard, and nothing follows them.
//! A simple type's `<restriction>` (§3.16.2) has no `assert` (only the
//! `assertion` facet). XSD 1.0 has no `assert` at all.
//!
//! The assertions placed after `<xs:anyAttribute>` are part of the type's
//! {assertions} (§3.4.2.1: "Assertions corresponding to all the `<assert>`
//! element information items among the [children] of `<complexType>`,
//! `<restriction>` and `<extension>`, if any, in document order") and are
//! checked on every driver.

// The XSD 1.1 position tables are used only with the `xsd11` feature.
#![cfg_attr(not(feature = "xsd11"), allow(dead_code))]

use xsd_schema::{SchemaSetBuilder, XsdVersion};

const XS: &str = r#"xmlns:xs="http://www.w3.org/2001/XMLSchema""#;

/// Named components every probe may refer to.
const SUPPORT: &str = r#"
  <xs:attributeGroup name="AG"><xs:attribute name="g"/></xs:attributeGroup>
  <xs:complexType name="SB">
    <xs:simpleContent><xs:extension base="xs:string">
      <xs:attribute name="a"/><xs:attributeGroup ref="AG"/>
      <xs:anyAttribute processContents="lax"/>
    </xs:extension></xs:simpleContent>
  </xs:complexType>
  <xs:complexType name="CB">
    <xs:sequence><xs:element name="e" minOccurs="0"/></xs:sequence>
    <xs:attribute name="a"/><xs:attributeGroup ref="AG"/>
    <xs:anyAttribute processContents="lax"/>
  </xs:complexType>"#;

/// The parents an `assert` can have, as a template around `{}`.
const PARENTS: &[(&str, &str)] = &[
    (
        "complexType",
        r#"<xs:complexType name="T">{}</xs:complexType>"#,
    ),
    (
        "simpleContent/restriction",
        r#"<xs:complexType name="T"><xs:simpleContent><xs:restriction base="SB">{}</xs:restriction></xs:simpleContent></xs:complexType>"#,
    ),
    (
        "simpleContent/extension",
        r#"<xs:complexType name="T"><xs:simpleContent><xs:extension base="xs:string">{}</xs:extension></xs:simpleContent></xs:complexType>"#,
    ),
    (
        "complexContent/restriction",
        r#"<xs:complexType name="T"><xs:complexContent><xs:restriction base="CB">{}</xs:restriction></xs:complexContent></xs:complexType>"#,
    ),
    (
        "complexContent/extension",
        r#"<xs:complexType name="T"><xs:complexContent><xs:extension base="CB">{}</xs:extension></xs:complexContent></xs:complexType>"#,
    ),
];

const ASSERT: &str = r#"<xs:assert test="true()"/>"#;
const ATTR: &str = r#"<xs:attribute name="b"/>"#;
const GROUP: &str = r#"<xs:attributeGroup ref="AG"/>"#;
const WILD: &str = r#"<xs:anyAttribute processContents="lax"/>"#;
const ANN: &str = r#"<xs:annotation><xs:documentation>d</xs:documentation></xs:annotation>"#;
const SEQ: &str = r#"<xs:sequence><xs:element name="e" minOccurs="0"/></xs:sequence>"#;
const OPEN: &str =
    r###"<xs:openContent><xs:any namespace="##other" processContents="lax"/></xs:openContent>"###;
const FACET: &str = r#"<xs:maxLength value="5"/>"#;

/// `Ok(())` when the schema document compiles, the error text otherwise.
fn compile(version: XsdVersion, body: &str) -> Result<(), String> {
    let xsd = format!("<xs:schema {XS}>{SUPPORT}{body}</xs:schema>");
    SchemaSetBuilder::with_version(version)
        .add_source(&xsd, "file:///assertion_positions.xsd")
        .map_err(|e| e.to_string())?
        .compile()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn body(parts: &[&str]) -> String {
    parts.concat()
}

/// The positions after the attribute declarations, attribute group
/// references and the attribute wildcard, in every parent: allowed in 1.1.
fn allowed_everywhere() -> Vec<String> {
    vec![
        body(&[ASSERT]),
        body(&[ASSERT, ASSERT]),
        body(&[ANN, ASSERT]),
        body(&[ATTR, ASSERT]),
        body(&[GROUP, ASSERT]),
        body(&[WILD, ASSERT]),
        body(&[ANN, WILD, ASSERT]),
        body(&[ATTR, WILD, ASSERT]),
        body(&[GROUP, WILD, ASSERT, ASSERT]),
        body(&[ATTR, GROUP, WILD, ASSERT]),
        body(&[GROUP, ATTR, ASSERT]),
    ]
}

/// Orders the content models forbid, in every parent.
fn refused_everywhere() -> Vec<String> {
    vec![
        body(&[ASSERT, ATTR]),
        body(&[ASSERT, GROUP]),
        body(&[ASSERT, WILD]),
        body(&[WILD, ASSERT, ATTR]),
        body(&[WILD, ATTR]),
        body(&[WILD, GROUP]),
        body(&[WILD, WILD]),
        body(&[ASSERT, ANN]),
        body(&[ATTR, ANN]),
    ]
}

/// Parent-specific cases: (parent, body, allowed in 1.1).
fn parent_cases() -> Vec<(&'static str, String, bool)> {
    let mut cases = Vec::new();
    for parent in [
        "complexType",
        "complexContent/restriction",
        "complexContent/extension",
    ] {
        cases.push((parent, body(&[SEQ, ASSERT]), true));
        cases.push((parent, body(&[SEQ, ATTR, GROUP, WILD, ASSERT]), true));
        cases.push((parent, body(&[ANN, SEQ, WILD, ASSERT]), true));
        cases.push((parent, body(&[ASSERT, SEQ]), false));
        cases.push((parent, body(&[ATTR, SEQ]), false));
        cases.push((parent, body(&[WILD, SEQ]), false));
        cases.push((parent, body(&[SEQ, SEQ]), false));
        cases.push((parent, body(&[SEQ, OPEN]), false));
        cases.push((parent, body(&[ASSERT, OPEN]), false));
        cases.push((parent, body(&[WILD, OPEN]), false));
        cases.push((parent, body(&[FACET]), false));
    }
    // openContent before the particle (the restriction keeps CB's language:
    // CB has no open content, so only the extension and complexType get one).
    for parent in ["complexType", "complexContent/extension"] {
        cases.push((parent, body(&[OPEN, SEQ, ASSERT]), true));
        cases.push((parent, body(&[ANN, OPEN, SEQ, WILD, ASSERT]), true));
        cases.push((parent, body(&[OPEN, OPEN, SEQ]), false));
    }
    cases.push(("complexType", body(&[OPEN, ATTR, ASSERT]), true));
    cases.push((
        "complexContent/extension",
        body(&[OPEN, WILD, ASSERT]),
        true,
    ));
    // simpleContent: facets (restriction only) before the attributes.
    let sr = "simpleContent/restriction";
    cases.push((sr, body(&[FACET, ASSERT]), true));
    cases.push((sr, body(&[FACET, ATTR, WILD, ASSERT]), true));
    cases.push((
        sr,
        body(&[
            r#"<xs:simpleType><xs:restriction base="xs:string"/></xs:simpleType>"#,
            FACET,
            WILD,
            ASSERT,
        ]),
        true,
    ));
    cases.push((
        sr,
        body(&[
            r#"<xs:assertion test="string-length($value) lt 9"/>"#,
            GROUP,
            ASSERT,
        ]),
        true,
    ));
    cases.push((sr, body(&[ATTR, FACET]), false));
    cases.push((sr, body(&[WILD, FACET]), false));
    cases.push((sr, body(&[ASSERT, FACET]), false));
    cases.push((
        sr,
        body(&[
            ASSERT,
            r#"<xs:simpleType><xs:restriction base="xs:string"/></xs:simpleType>"#,
        ]),
        false,
    ));
    for parent in ["simpleContent/restriction", "simpleContent/extension"] {
        cases.push((parent, body(&[SEQ, ASSERT]), false));
        cases.push((parent, body(&[OPEN, ASSERT]), false));
    }
    cases.push(("simpleContent/extension", body(&[FACET, ASSERT]), false));
    // complex content has no inline simpleType.
    for parent in [
        "complexContent/restriction",
        "complexContent/extension",
        "complexType",
    ] {
        cases.push((
            parent,
            body(&[
                r#"<xs:simpleType><xs:restriction base="xs:string"/></xs:simpleType>"#,
                ASSERT,
            ]),
            false,
        ));
    }
    cases
}

fn wrap(parent: &str, body: &str) -> String {
    let template = PARENTS
        .iter()
        .find(|(name, _)| *name == parent)
        .map(|(_, t)| *t)
        .expect("known parent");
    // An extension appends its particle to CB's `e?`: a second `e?` would
    // break UPA, so the extension's own element is `f`.
    let body = if parent == "complexContent/extension" {
        body.replace(r#"name="e""#, r#"name="f""#)
    } else {
        body.to_string()
    };
    template.replace("{}", &body)
}

/// Every XSD 1.1 verdict, collected so that one run lists every position the
/// parser gets wrong.
#[cfg(feature = "xsd11")]
#[test]
fn assertions_follow_attributes_and_the_attribute_wildcard_in_xsd_1_1() {
    let mut wrong = Vec::new();
    let mut cases: Vec<(&str, String, bool)> = Vec::new();
    for (parent, _) in PARENTS {
        for b in allowed_everywhere() {
            cases.push((parent, b, true));
        }
        for b in refused_everywhere() {
            cases.push((parent, b, false));
        }
    }
    cases.extend(parent_cases());
    for (parent, b, allowed) in &cases {
        let verdict = compile(XsdVersion::V1_1, &wrap(parent, b));
        if verdict.is_ok() != *allowed {
            wrong.push(format!(
                "{parent}: {b}\n    expected {}, got {verdict:?}",
                if *allowed { "accepted" } else { "refused" }
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} of {} XSD 1.1 positions wrong:\n  {}",
        wrong.len(),
        cases.len(),
        wrong.join("\n  ")
    );
}

/// The support components and the assert-free orders compile in XSD 1.0
/// (the probes above test the parser, not the support schema).
#[test]
fn the_support_schema_compiles_in_both_versions() {
    for version in [XsdVersion::V1_0, XsdVersion::V1_1] {
        for (parent, _) in PARENTS {
            let b = body(&[ATTR, GROUP, WILD]);
            assert_eq!(
                compile(version, &wrap(parent, &b)),
                Ok(()),
                "{version:?} {parent}"
            );
        }
    }
}

/// XSD 1.0 has no `assert`: refused in every position and parent.
#[test]
fn xsd_1_0_refuses_assert_everywhere() {
    let mut wrong = Vec::new();
    for (parent, _) in PARENTS {
        for b in allowed_everywhere() {
            if let Ok(()) = compile(XsdVersion::V1_0, &wrap(parent, &b)) {
                wrong.push(format!("{parent}: {b}"));
            }
        }
    }
    let st = r#"<xs:simpleType name="S"><xs:restriction base="xs:string"><xs:assert test="true()"/></xs:restriction></xs:simpleType>"#;
    if compile(XsdVersion::V1_0, st).is_ok() {
        wrong.push("simpleType/restriction".to_string());
    }
    assert!(
        wrong.is_empty(),
        "XSD 1.0 accepted assert in:\n  {}",
        wrong.join("\n  ")
    );
}

/// A simple type's `<restriction>` has no `assert` (§3.16.2), nor
/// `openContent`; its `assertion` facet stays allowed.
#[cfg(feature = "xsd11")]
#[test]
fn a_simple_type_restriction_has_no_assert() {
    let assert = r#"<xs:simpleType name="S"><xs:restriction base="xs:string"><xs:assert test="true()"/></xs:restriction></xs:simpleType>"#;
    assert!(
        compile(XsdVersion::V1_1, assert).is_err(),
        "assert in simpleType/restriction accepted"
    );
    let open = r#"<xs:simpleType name="S"><xs:restriction base="xs:string"><xs:openContent><xs:any/></xs:openContent></xs:restriction></xs:simpleType>"#;
    assert!(
        compile(XsdVersion::V1_1, open).is_err(),
        "openContent in simpleType/restriction accepted"
    );
    let facet = r#"<xs:simpleType name="S"><xs:restriction base="xs:string"><xs:assertion test="true()"/></xs:restriction></xs:simpleType>"#;
    assert_eq!(compile(XsdVersion::V1_1, facet), Ok(()));
}

#[cfg(feature = "xsd11")]
mod evaluated {
    //! The assertion after `<xs:anyAttribute>` is part of {assertions} and
    //! is checked on every driver.

    use xsd_schema::validation::{
        drive_navigator, drive_quick_xml, CollectingValidationSink, SchemaValidator,
        SchemaValidity, ValidationFlags,
    };
    use xsd_schema::{RoXmlNavigator, SchemaSet, SchemaSetBuilder, XsdVersion};

    const SCHEMA: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
      <xs:element name="r">
        <xs:complexType>
          <xs:anyAttribute processContents="skip"/>
          <xs:assert test="@a = 1"/>
        </xs:complexType>
      </xs:element>
      <xs:element name="s">
        <xs:complexType>
          <xs:simpleContent>
            <xs:extension base="xs:int">
              <xs:anyAttribute processContents="skip"/>
              <xs:assert test="$value lt 10"/>
            </xs:extension>
          </xs:simpleContent>
        </xs:complexType>
      </xs:element>
      <xs:element name="doc">
        <xs:complexType><xs:sequence>
          <xs:element ref="r" minOccurs="0"/><xs:element ref="s" minOccurs="0"/>
        </xs:sequence></xs:complexType>
      </xs:element>
    </xs:schema>"#;

    fn schema() -> SchemaSet {
        SchemaSetBuilder::with_version(XsdVersion::V1_1)
            .add_source(SCHEMA, "file:///assertion_positions_eval.xsd")
            .expect("schema source accepted")
            .compile()
            .unwrap_or_else(|e| panic!("schema compiles: {e}"))
            .into_schema_set()
    }

    fn flags(psvi: bool) -> ValidationFlags {
        let mut f = ValidationFlags::default() | ValidationFlags::PROCESS_ASSERTIONS;
        if !psvi {
            f &= !ValidationFlags::BUILD_PSVI_TYPED_VALUES;
        }
        f
    }

    fn stream(ss: &SchemaSet, xml: &str, psvi: bool) -> (Option<SchemaValidity>, Vec<String>) {
        let v = SchemaValidator::new(ss, flags(psvi));
        let (mut errors, mut warnings) = (Vec::new(), Vec::new());
        let root = {
            let mut rt = v.start_run(CollectingValidationSink {
                errors: &mut errors,
                warnings: &mut warnings,
            });
            let out = drive_quick_xml(xml.as_bytes(), &mut rt, ss).expect("drive");
            out.root_validity
        };
        (
            root,
            errors.iter().map(|e| e.constraint.to_string()).collect(),
        )
    }

    fn navigator(ss: &SchemaSet, xml: &str) -> (Option<SchemaValidity>, Vec<String>) {
        let v = SchemaValidator::new(ss, flags(true));
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
        (
            root,
            errors.iter().map(|e| e.constraint.to_string()).collect(),
        )
    }

    #[test]
    fn an_assertion_after_the_attribute_wildcard_is_checked() {
        let ss = schema();
        for (xml, valid) in [
            ("<doc><r a='1'/></doc>", true),
            ("<doc><r a='2'/></doc>", false),
            ("<doc><r b='1'/></doc>", false),
            ("<doc><s x='y'>3</s></doc>", true),
            ("<doc><s x='y'>30</s></doc>", false),
        ] {
            let runs = [
                ("stream", stream(&ss, xml, true)),
                ("stream/no-psvi", stream(&ss, xml, false)),
                ("navigator", navigator(&ss, xml)),
            ];
            for (driver, (root, codes)) in runs {
                let expected = if valid {
                    SchemaValidity::Valid
                } else {
                    SchemaValidity::Invalid
                };
                assert_eq!(root, Some(expected), "{driver}: {xml}: {codes:?}");
                assert_eq!(
                    codes.iter().any(|c| c.starts_with("cvc-assertion")),
                    !valid,
                    "{driver}: {xml}: {codes:?}"
                );
            }
        }
    }
}
