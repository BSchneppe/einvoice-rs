//! All Group Limited: where an `all` model group may appear, and how often.
//!
//! Structures 1.1 §3.8.6.2: "When a model group has {compositor} all, then
//! all of the following must be true: 1 It appears only as the value of one
//! or more of the following properties: 1.1 the {model group} property of a
//! model group definition. 1.2 the {term} property of a Particle with {max
//! occurs} = 1 which is the {particle} of the {content type} of a complex
//! type definition. 1.3 the {term} property of a Particle P with {min occurs}
//! = {max occurs} = 1, where P is among the {particles} of a Model Group whose
//! {compositor} is all. 2 For every particle P in its {particles}, if
//! P.{term} is a model group, then P.{term}.{compositor} = all." §3.8.2:
//! `<all maxOccurs = (0 | 1) : 1 minOccurs = (0 | 1) : 1>`.
//!
//! XSD 1.0 §3.8.6 (`cos-all-limited`): "1 It appears as the model group of a
//! model group definition. 2 It appears in a particle with {min occurs}={max
//! occurs}=1, and that particle must be part of a pair which constitutes the
//! {content type} of a complex type definition"; §3.8.2: `<all maxOccurs = 1
//! : 1 minOccurs = (0 | 1) : 1>`. XSD 1.1 relaxed what an all group contains
//! (wildcards, `maxOccurs > 1` members, all-group references), not where it
//! appears nor its own occurrence.

use xsd_schema::{SchemaSetBuilder, XsdVersion};

/// `{BODY}` is each case's components; `allg` is an all group.
const SCHEMA: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:group name="allg"><xs:all>
    <xs:element name="a"/><xs:element name="b"/>
  </xs:all></xs:group>
  <xs:group name="allh"><xs:all>
    <xs:element name="c"/>
  </xs:all></xs:group>
  <xs:group name="seqWithAll"><xs:sequence>
    <xs:element name="x"/><xs:group ref="allg"/>
  </xs:sequence></xs:group>
  {BODY}
</xs:schema>"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    /// Compiles.
    Ok,
    /// Refused with `cos-all-limited`.
    Limited,
}
use Verdict::{Limited, Ok};

/// (label, components, XSD 1.0 verdict, XSD 1.1 verdict)
const CASES: &[(&str, &str, Verdict, Verdict)] = &[
    // The all group's own occurrence (clause 1.2; §3.8.2).
    (
        "<xs:all maxOccurs=\"2\"> as the content type (mgAb004)",
        r#"<xs:complexType name="t"><xs:all maxOccurs="2"><xs:element name="a"/></xs:all></xs:complexType>"#,
        Limited,
        Limited,
    ),
    (
        "<xs:all maxOccurs=\"unbounded\"> (mgAb007)",
        r#"<xs:complexType name="t"><xs:all maxOccurs="unbounded"><xs:element name="a"/></xs:all></xs:complexType>"#,
        Limited,
        Limited,
    ),
    (
        "<xs:all minOccurs=\"0\" maxOccurs=\"2\"> (mgO003)",
        r#"<xs:complexType name="t"><xs:all minOccurs="0" maxOccurs="2"><xs:element name="a"/></xs:all></xs:complexType>"#,
        Limited,
        Limited,
    ),
    (
        "a reference to an all group with maxOccurs=\"2\" (particlesEa025)",
        r#"<xs:complexType name="t"><xs:group ref="allg" minOccurs="1" maxOccurs="2"/></xs:complexType>"#,
        Limited,
        Limited,
    ),
    (
        "<xs:all maxOccurs=\"2\"> extending an empty base (mgP042)",
        r#"<xs:complexType name="base"/>
           <xs:complexType name="t"><xs:complexContent><xs:extension base="base">
             <xs:all maxOccurs="2"><xs:element name="a"/></xs:all>
           </xs:extension></xs:complexContent></xs:complexType>"#,
        Limited,
        Limited,
    ),
    // Its placement (clause 1).
    (
        "a reference to an all group inside a sequence (mgA020)",
        r#"<xs:complexType name="t"><xs:sequence>
             <xs:element name="x"/><xs:group ref="allg"/>
           </xs:sequence></xs:complexType>"#,
        Limited,
        Limited,
    ),
    (
        "a reference to an all group inside a choice",
        r#"<xs:complexType name="t"><xs:choice>
             <xs:element name="x"/><xs:group ref="allg"/>
           </xs:choice></xs:complexType>"#,
        Limited,
        Limited,
    ),
    (
        "an all group reached through a sequence group",
        r#"<xs:complexType name="t"><xs:group ref="seqWithAll"/></xs:complexType>"#,
        Limited,
        Limited,
    ),
    // The neighbours.
    (
        "<xs:all> as the content type",
        r#"<xs:complexType name="t"><xs:all><xs:element name="a"/></xs:all></xs:complexType>"#,
        Ok,
        Ok,
    ),
    (
        "<xs:all minOccurs=\"0\">",
        r#"<xs:complexType name="t"><xs:all minOccurs="0"><xs:element name="a"/></xs:all></xs:complexType>"#,
        Ok,
        Ok,
    ),
    (
        "<xs:all maxOccurs=\"1\" minOccurs=\"1\">",
        r#"<xs:complexType name="t"><xs:all maxOccurs="1" minOccurs="1"><xs:element name="a"/></xs:all></xs:complexType>"#,
        Ok,
        Ok,
    ),
    (
        "a reference to an all group as the content type",
        r#"<xs:complexType name="t"><xs:group ref="allg"/></xs:complexType>"#,
        Ok,
        Ok,
    ),
    (
        "a reference to an all group with minOccurs=\"0\"",
        r#"<xs:complexType name="t"><xs:group ref="allg" minOccurs="0"/></xs:complexType>"#,
        Ok,
        Ok,
    ),
    (
        "a sequence of elements and a sequence group without an all group",
        r#"<xs:group name="s"><xs:sequence><xs:element name="y"/></xs:sequence></xs:group>
           <xs:complexType name="t"><xs:sequence>
             <xs:element name="x"/><xs:group ref="s" maxOccurs="2"/>
           </xs:sequence></xs:complexType>"#,
        Ok,
        Ok,
    ),
    // XSD 1.1 only: an all group inside an all group (clause 1.3).
    (
        "a reference to an all group inside an all group",
        r#"<xs:complexType name="t"><xs:all>
             <xs:element name="x"/><xs:group ref="allh"/>
           </xs:all></xs:complexType>"#,
        Limited,
        Ok,
    ),
    (
        "a reference to an all group inside an all group, maxOccurs=\"2\"",
        r#"<xs:complexType name="t"><xs:all>
             <xs:element name="x"/><xs:group ref="allh" maxOccurs="2"/>
           </xs:all></xs:complexType>"#,
        Limited,
        Limited,
    ),
    // Fix round 2: a particle with minOccurs = maxOccurs = 0 is no component
    // (1.1 §3.7.2: "Otherwise, the <group> has minOccurs=maxOccurs=0, in which
    // case it maps to no component at all"; 1.0 §3.7.2 the same), so it places
    // no all group anywhere.
    (
        "a zero-occurrence reference to an all group inside a sequence",
        r#"<xs:complexType name="t"><xs:sequence>
             <xs:element name="x"/><xs:group ref="allg" minOccurs="0" maxOccurs="0"/>
           </xs:sequence></xs:complexType>"#,
        Ok,
        Ok,
    ),
    (
        "a zero-occurrence reference to an all group inside a choice",
        r#"<xs:complexType name="t"><xs:choice>
             <xs:element name="x"/><xs:group ref="allg" minOccurs="0" maxOccurs="0"/>
           </xs:choice></xs:complexType>"#,
        Ok,
        Ok,
    ),
    (
        "an optional reference to an all group inside a sequence",
        r#"<xs:complexType name="t"><xs:sequence>
             <xs:element name="x"/><xs:group ref="allg" minOccurs="0"/>
           </xs:sequence></xs:complexType>"#,
        Limited,
        Limited,
    ),
    (
        "a zero-occurrence reference to an all group as the content type",
        r#"<xs:complexType name="t"><xs:group ref="allg" minOccurs="0" maxOccurs="0"/></xs:complexType>"#,
        Ok,
        Ok,
    ),
    // `<all>` itself: 1.0 §3.8.2 `maxOccurs = 1 : 1`, 1.1 `maxOccurs = (0 | 1)`.
    (
        "<xs:all minOccurs=\"0\" maxOccurs=\"0\"> as the content type",
        r#"<xs:complexType name="t"><xs:all minOccurs="0" maxOccurs="0"><xs:element name="a"/></xs:all></xs:complexType>"#,
        Limited,
        Ok,
    ),
];

/// A zero-occurrence all group (or reference to one) is no component: the
/// type's content is empty, on every driver (streaming with and without PSVI
/// typed values, navigator), in the versions where the schema is valid.
#[test]
fn a_zero_occurrence_all_group_leaves_the_content_empty() {
    use xsd_schema::validation::{
        drive_navigator, drive_quick_xml, CollectingValidationSink, SchemaValidator,
        SchemaValidity, ValidationFlags,
    };
    use xsd_schema::RoXmlNavigator;
    let cases: [(&str, &str, &[XsdVersion]); 4] = [
        (
            "zero-occurrence reference in a sequence",
            r#"<xs:sequence><xs:element name="x" minOccurs="0"/><xs:group ref="allg" minOccurs="0" maxOccurs="0"/></xs:sequence>"#,
            &[XsdVersion::V1_0, XsdVersion::V1_1],
        ),
        (
            "zero-occurrence reference in a choice",
            r#"<xs:choice minOccurs="0"><xs:element name="x"/><xs:group ref="allg" minOccurs="0" maxOccurs="0"/></xs:choice>"#,
            &[XsdVersion::V1_0, XsdVersion::V1_1],
        ),
        (
            "zero-occurrence reference as the content type",
            r#"<xs:group ref="allg" minOccurs="0" maxOccurs="0"/>"#,
            &[XsdVersion::V1_0, XsdVersion::V1_1],
        ),
        (
            "zero-occurrence all group as the content type",
            r#"<xs:all minOccurs="0" maxOccurs="0"><xs:element name="a"/></xs:all>"#,
            &[XsdVersion::V1_1],
        ),
    ];
    let mut wrong = Vec::new();
    for (label, content, versions) in cases {
        let body = format!(
            r#"<xs:complexType name="t">{content}</xs:complexType><xs:element name="doc" type="t"/>"#
        );
        let text = SCHEMA.replace("{BODY}", &body);
        for &version in versions {
            let ss = match SchemaSetBuilder::with_version(version)
                .add_source(&text, "file:///all_group_limited.xsd")
                .and_then(|b| b.compile())
            {
                Result::Ok(c) => c.into_schema_set(),
                Err(e) => {
                    wrong.push(format!("{version:?} {label}: refused: {e}"));
                    continue;
                }
            };
            for (xml, valid) in [("<doc/>", true), ("<doc><a/><b/></doc>", false)] {
                let want = Some(if valid {
                    SchemaValidity::Valid
                } else {
                    SchemaValidity::Invalid
                });
                let mut roots = Vec::new();
                for psvi in [true, false] {
                    let mut flags = ValidationFlags::default();
                    if !psvi {
                        flags &= !ValidationFlags::BUILD_PSVI_TYPED_VALUES;
                    }
                    let v = SchemaValidator::new(&ss, flags);
                    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
                    let mut rt = v.start_run(CollectingValidationSink {
                        errors: &mut errors,
                        warnings: &mut warnings,
                    });
                    let root = drive_quick_xml(xml.as_bytes(), &mut rt, &ss)
                        .expect("drive")
                        .root_validity;
                    roots.push((if psvi { "stream" } else { "stream/no-psvi" }, root));
                }
                let v = SchemaValidator::new(&ss, ValidationFlags::default());
                let doc = roxmltree::Document::parse(xml).expect("well-formed");
                let nav = RoXmlNavigator::new(&doc);
                let (mut errors, mut warnings) = (Vec::new(), Vec::new());
                let mut rt = v.start_run(CollectingValidationSink {
                    errors: &mut errors,
                    warnings: &mut warnings,
                });
                let root = drive_navigator(&nav, &mut rt, &ss)
                    .expect("drive")
                    .root_validity;
                roots.push(("navigator", root));
                for (driver, root) in roots {
                    if root != want {
                        wrong.push(format!("{version:?} {label} {xml} ({driver}): {root:?}"));
                    }
                }
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

#[test]
fn an_all_group_appears_only_where_all_group_limited_allows() {
    let mut wrong = Vec::new();
    for (version, column) in [(XsdVersion::V1_0, 0), (XsdVersion::V1_1, 1)] {
        for &(label, body, v10, v11) in CASES {
            // An all group inside an all group (clause 1.3) is compiled by
            // code of the `xsd11` feature only.
            if column == 1 && label.contains("inside an all group") && !cfg!(feature = "xsd11") {
                continue;
            }
            let want = if column == 0 { v10 } else { v11 };
            let text = SCHEMA.replace("{BODY}", body);
            let result = SchemaSetBuilder::with_version(version)
                .add_source(&text, "file:///all_group_limited.xsd")
                .map_err(|e| e.to_string())
                .and_then(|b| b.compile().map_err(|e| e.to_string()));
            match (result, want) {
                (Result::Ok(_), Ok) => {}
                (Result::Ok(_), Limited) => wrong.push(format!(
                    "{version:?} {label}: compiles, expected cos-all-limited"
                )),
                (Err(e), Ok) => wrong.push(format!("{version:?} {label}: refused: {e}")),
                (Err(e), Limited) => {
                    if !e.contains("cos-all-limited") {
                        wrong.push(format!(
                            "{version:?} {label}: refused, not cos-all-limited: {e}"
                        ));
                    }
                }
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

/// Fix round 3: the content type of content that maps to no particle.
///
/// Structures 1.1 §3.4.2.3.3: "2 Let the explicit content be … 2.1 If at least
/// one of the following is true … 2.1.2 There is an <all> or <sequence> among
/// the [children] with no [children] of its own excluding <annotation>; 2.1.3
/// There is among the [children] a <choice> element whose minOccurs
/// [attribute] has the ·actual value· 0 and which has no [children] of its own
/// except for <annotation>; 2.1.4 The <group>, <all>, <choice> or <sequence>
/// element among the [children] has a maxOccurs [attribute] with an ·actual
/// value· of 0; then empty"; with the effective mixed false, the {variety} is
/// empty (clause 4.1.1) — no element and no character [children], white space
/// included (§3.4.4.2 clause 1.1). XSD 1.0 §3.4.2 has 2.1.1 – 2.1.3; its
/// <group> with minOccurs = maxOccurs = 0 "corresponds to no component at all"
/// (§3.7.2), so there is no particle either. A sequence or choice whose
/// children are all zero-occurrence particles is not empty explicit content
/// (it has [children]): its particle has no particles — element-only, white
/// space allowed; such a choice matches nothing unless it is optional.
///
/// Each case runs on the streaming driver with and without PSVI typed values,
/// the navigator driver and (`xsd11`) the typed build.
#[test]
fn zero_occurrence_content_has_the_content_type_of_the_spec() {
    use xsd_schema::validation::{
        drive_navigator, drive_quick_xml, CollectingValidationSink, SchemaValidator,
        SchemaValidity, ValidationFlags,
    };
    use xsd_schema::RoXmlNavigator;
    let zero = r#"<xs:group ref="allg" minOccurs="0" maxOccurs="0"/>"#;
    // (label, complexType attributes, content, [(instance, valid)])
    type Case = (
        &'static str,
        &'static str,
        String,
        Vec<(&'static str, bool)>,
    );
    let cases: Vec<Case> = vec![
        (
            "only a zero-occurrence reference (the reviewer's case)",
            "",
            zero.to_string(),
            vec![
                ("<doc/>", true),
                ("<doc> </doc>", false),
                ("<doc><a/></doc>", false),
            ],
        ),
        (
            "a zero-occurrence sequence",
            "",
            r#"<xs:sequence minOccurs="0" maxOccurs="0"><xs:element name="a"/></xs:sequence>"#
                .to_string(),
            vec![
                ("<doc/>", true),
                ("<doc> </doc>", false),
                ("<doc><a/></doc>", false),
            ],
        ),
        (
            "a sequence of zero-occurrence particles",
            "",
            format!(
                r#"<xs:sequence>{zero}<xs:element name="a" minOccurs="0" maxOccurs="0"/></xs:sequence>"#
            ),
            vec![
                ("<doc/>", true),
                ("<doc> </doc>", true),
                ("<doc><a/></doc>", false),
            ],
        ),
        (
            "a choice of zero-occurrence particles",
            "",
            format!(r#"<xs:choice>{zero}</xs:choice>"#),
            vec![
                ("<doc/>", false),
                ("<doc> </doc>", false),
                ("<doc><a/></doc>", false),
            ],
        ),
        (
            "an optional choice of zero-occurrence particles",
            "",
            format!(r#"<xs:choice minOccurs="0">{zero}</xs:choice>"#),
            vec![
                ("<doc/>", true),
                ("<doc> </doc>", true),
                ("<doc><a/></doc>", false),
            ],
        ),
        (
            "a mixed type with only a zero-occurrence reference",
            r#"mixed="true""#,
            zero.to_string(),
            vec![
                ("<doc/>", true),
                ("<doc> x </doc>", true),
                ("<doc><a/></doc>", false),
            ],
        ),
    ];
    let mut wrong = Vec::new();
    for (label, attrs, content, instances) in &cases {
        let body = format!(
            r#"<xs:complexType name="t" {attrs}>{content}</xs:complexType><xs:element name="doc" type="t"/>"#
        );
        let text = SCHEMA.replace("{BODY}", &body);
        for version in [XsdVersion::V1_0, XsdVersion::V1_1] {
            let ss = match SchemaSetBuilder::with_version(version)
                .add_source(&text, "file:///all_group_limited.xsd")
                .and_then(|b| b.compile())
            {
                Result::Ok(c) => c.into_schema_set(),
                Err(e) => {
                    wrong.push(format!("{version:?} {label}: refused: {e}"));
                    continue;
                }
            };
            for &(xml, valid) in instances {
                let want = Some(if valid {
                    SchemaValidity::Valid
                } else {
                    SchemaValidity::Invalid
                });
                let mut roots = Vec::new();
                for psvi in [true, false] {
                    let mut flags = ValidationFlags::default();
                    if !psvi {
                        flags &= !ValidationFlags::BUILD_PSVI_TYPED_VALUES;
                    }
                    let v = SchemaValidator::new(&ss, flags);
                    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
                    let mut rt = v.start_run(CollectingValidationSink {
                        errors: &mut errors,
                        warnings: &mut warnings,
                    });
                    let root = drive_quick_xml(xml.as_bytes(), &mut rt, &ss)
                        .expect("drive")
                        .root_validity;
                    roots.push((if psvi { "stream" } else { "stream/no-psvi" }, root));
                }
                let v = SchemaValidator::new(&ss, ValidationFlags::default());
                let doc = roxmltree::Document::parse(xml).expect("well-formed");
                let nav = RoXmlNavigator::new(&doc);
                let (mut errors, mut warnings) = (Vec::new(), Vec::new());
                let mut rt = v.start_run(CollectingValidationSink {
                    errors: &mut errors,
                    warnings: &mut warnings,
                });
                let root = drive_navigator(&nav, &mut rt, &ss)
                    .expect("drive")
                    .root_validity;
                roots.push(("navigator", root));
                drop(rt);
                #[cfg(feature = "xsd11")]
                {
                    use xsd_schema::document::{build_typed_document_with, TypedBuildOptions};
                    use xsd_schema::validation::NoopHandler;
                    let arena = bumpalo::Bump::new();
                    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
                    let mut rt = v.start_run(CollectingValidationSink {
                        errors: &mut errors,
                        warnings: &mut warnings,
                    });
                    let root = build_typed_document_with(
                        xml.as_bytes(),
                        &arena,
                        &ss,
                        &mut rt,
                        &mut NoopHandler,
                        TypedBuildOptions::default(),
                    )
                    .map(|built| built.outcome.root_validity)
                    .unwrap_or(Some(SchemaValidity::Invalid));
                    drop(rt);
                    roots.push(("typed build", root));
                }
                for (driver, root) in roots {
                    if root != want {
                        wrong.push(format!("{version:?} {label} {xml} ({driver}): {root:?}"));
                    }
                }
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
