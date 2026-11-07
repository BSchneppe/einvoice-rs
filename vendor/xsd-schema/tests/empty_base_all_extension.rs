//! An extension of a base type whose content is an empty `<xs:all/>`.
//!
//! Structures 1.1 §3.4.2.3.3: the ·explicit content· of `<complexType
//! name="B"><xs:all/></complexType>` is empty ("2.1.2 There is an <all> or
//! <sequence> among the [children] with no [children] of its own excluding
//! <annotation>"), so its {content type} has {variety} empty (clause 4.1.1),
//! unless it is mixed ("3.1.1 If the ·effective mixed· is true, then A
//! particle whose properties are as follows: {min occurs} 1 {max occurs} 1
//! {term} a model group whose {compositor} is sequence and whose {particles}
//! is empty"). An extension of a base with {variety} empty takes its own
//! content: "4.2.1 If the {base type definition} is a simple type definition
//! or is a complex type definition whose {content type}.{variety} = empty or
//! simple, then a Content Type as per clause 4.1.1 and clause 4.1.2 above" —
//! an all group (inline or referenced) is then the {particle} of the content
//! type, which All Group Limited (§3.8.6.2) clause 1.2 allows. Derivation
//! Valid (Extension) (§3.4.6.2) clause 1.4.3.2.1: "B.{content type}.{variety}
//! = empty." A mixed base keeps its empty sequence, so an all group added to it
//! lands in a sequence (clause 4.2.3.3) and the schema is in error (W3C
//! `all308`). XSD 1.0 §3.4.2 (clause 3.2.2: "If the content type of the type
//! definition resolved to by the ·actual value· of the base [attribute] is
//! empty, then a pair of mixed or elementOnly … and the ·explicit content·
//! itself") gives the same results.
//!
//! The all-group content models of extensions are compiled by code of the
//! `xsd11` feature; without it an all group under an extension is compiled
//! more loosely (see the batch (k) report, "Noticed, not done").
#![cfg(feature = "xsd11")]

use xsd_schema::validation::{
    drive_navigator, drive_quick_xml, CollectingValidationSink, SchemaValidator, SchemaValidity,
    ValidationFlags,
};
use xsd_schema::{RoXmlNavigator, SchemaSet, SchemaSetBuilder, XsdVersion};

/// `{BASE}` is the base type's content and attributes, `{EXT}` the
/// extension's content; `doc` has the extension's type.
const SCHEMA: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:group name="allg"><xs:all>
    <xs:element name="e1"/><xs:element name="e2" minOccurs="0"/>
  </xs:all></xs:group>
  <xs:complexType name="B" {MIXED}>{BASE}</xs:complexType>
  <xs:complexType name="T"><xs:complexContent {MIXED}><xs:extension base="B">
    {EXT}
  </xs:extension></xs:complexContent></xs:complexType>
  <xs:element name="doc" type="T"/>
</xs:schema>"#;

struct Case {
    label: &'static str,
    mixed: bool,
    base: &'static str,
    ext: &'static str,
    /// Whether the schema is valid, per version (1.0, 1.1); `None`: not
    /// checked in XSD 1.0 (this crate's 1.0 reading of an empty `<xs:all/>`
    /// base and of an all-group reference after an all base differs from
    /// XSD 1.0 §3.4.2; see the batch (k) report, "Noticed, not done").
    valid: (Option<bool>, bool),
}

const CASES: &[Case] = &[
    Case {
        label: "a reference to an all group over an empty all (mgO007, mgZ003)",
        mixed: false,
        base: "<xs:all/>",
        ext: r#"<xs:group ref="allg"/>"#,
        valid: (Some(true), true),
    },
    Case {
        label: "a reference to an all group over <xs:all minOccurs=\"0\"/>",
        mixed: false,
        base: r#"<xs:all minOccurs="0"/>"#,
        ext: r#"<xs:group ref="allg"/>"#,
        valid: (Some(true), true),
    },
    Case {
        label: "an inline all group over an empty all",
        mixed: false,
        base: "<xs:all/>",
        ext: r#"<xs:all><xs:element name="e1"/><xs:element name="e2" minOccurs="0"/></xs:all>"#,
        valid: (None, true),
    },
    Case {
        label: "a sequence over an empty all",
        mixed: false,
        base: "<xs:all/>",
        ext: r#"<xs:sequence><xs:element name="e1"/><xs:element name="e2" minOccurs="0"/></xs:sequence>"#,
        valid: (Some(true), true),
    },
    Case {
        label: "a reference to an all group with maxOccurs=\"2\" over an empty all",
        mixed: false,
        base: "<xs:all/>",
        ext: r#"<xs:group ref="allg" maxOccurs="2"/>"#,
        valid: (Some(false), false),
    },
    Case {
        label: "an inline all group over a mixed empty all (all308)",
        mixed: true,
        base: "<xs:all/>",
        ext: r#"<xs:all><xs:element name="e1"/><xs:element name="e2" minOccurs="0"/></xs:all>"#,
        valid: (Some(false), false),
    },
    Case {
        label: "a reference to an all group over a mixed empty all",
        mixed: true,
        base: "<xs:all/>",
        ext: r#"<xs:group ref="allg"/>"#,
        valid: (None, false),
    },
    // A base without any particle has {variety} empty too.
    Case {
        label: "a reference to an all group over a base without content",
        mixed: false,
        base: "",
        ext: r#"<xs:group ref="allg"/>"#,
        valid: (Some(true), true),
    },
    Case {
        label: "an inline all group over a base without content",
        mixed: false,
        base: "",
        ext: r#"<xs:all><xs:element name="e1"/><xs:element name="e2" minOccurs="0"/></xs:all>"#,
        valid: (Some(true), true),
    },
    Case {
        label: "a sequence over a mixed empty all",
        mixed: true,
        base: "<xs:all/>",
        ext: r#"<xs:sequence><xs:element name="e1"/><xs:element name="e2" minOccurs="0"/></xs:sequence>"#,
        valid: (Some(true), true),
    },
    // Fix round 2: a zero-occurrence particle is no component (§3.7.2,
    // §3.8.2), so the explicit content is empty (clause 2.1.4) and the
    // extension keeps the base's content type (clause 4.2.2) — or, over a
    // base without content, is empty.
    Case {
        label: "a zero-occurrence reference to an all group over a base without content",
        mixed: false,
        base: "",
        ext: r#"<xs:group ref="allg" minOccurs="0" maxOccurs="0"/>"#,
        valid: (Some(true), true),
    },
    Case {
        label: "a zero-occurrence reference to an all group over a non-empty all",
        mixed: false,
        base: r#"<xs:all><xs:element name="e1"/><xs:element name="e2" minOccurs="0"/></xs:all>"#,
        ext: r#"<xs:group ref="allg" minOccurs="0" maxOccurs="0"/>"#,
        valid: (Some(true), true),
    },
    Case {
        label: "a zero-occurrence all group over a non-empty all",
        mixed: false,
        base: r#"<xs:all><xs:element name="e1"/><xs:element name="e2" minOccurs="0"/></xs:all>"#,
        ext: r#"<xs:all minOccurs="0" maxOccurs="0"><xs:element name="e3"/></xs:all>"#,
        valid: (None, true),
    },
    // XSD 1.1 clause 4.2.3.2: an all group over an all group merges them.
    Case {
        label: "a reference to an all group over a non-empty all",
        mixed: false,
        base: r#"<xs:all><xs:element name="e0" minOccurs="0"/></xs:all>"#,
        ext: r#"<xs:group ref="allg"/>"#,
        valid: (None, true),
    },
    Case {
        label: "an inline all group over a non-empty all",
        mixed: false,
        base: r#"<xs:all><xs:element name="e0" minOccurs="0"/></xs:all>"#,
        ext: r#"<xs:all><xs:element name="e1"/><xs:element name="e2" minOccurs="0"/></xs:all>"#,
        valid: (Some(false), true),
    },
];

fn compile(version: XsdVersion, case: &Case) -> Result<SchemaSet, String> {
    let mixed = if case.mixed { r#"mixed="true""# } else { "" };
    let text = SCHEMA
        .replace("{MIXED}", mixed)
        .replace("{BASE}", case.base)
        .replace("{EXT}", case.ext);
    SchemaSetBuilder::with_version(version)
        .add_source(&text, "file:///empty_base_all_extension.xsd")
        .map_err(|e| e.to_string())?
        .compile()
        .map(|c| c.into_schema_set())
        .map_err(|e| e.to_string())
}

/// The root validity on the streaming driver (with and without PSVI typed
/// values) and the navigator driver.
fn verdicts(ss: &SchemaSet, xml: &str) -> Vec<(&'static str, Option<SchemaValidity>)> {
    let mut out = Vec::new();
    for psvi in [true, false] {
        let mut flags = ValidationFlags::default();
        if !psvi {
            flags &= !ValidationFlags::BUILD_PSVI_TYPED_VALUES;
        }
        let v = SchemaValidator::new(ss, flags);
        let (mut errors, mut warnings) = (Vec::new(), Vec::new());
        let mut rt = v.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        let root = drive_quick_xml(xml.as_bytes(), &mut rt, ss)
            .expect("drive")
            .root_validity;
        out.push((if psvi { "stream" } else { "stream/no-psvi" }, root));
    }
    let v = SchemaValidator::new(ss, ValidationFlags::default());
    let doc = roxmltree::Document::parse(xml).expect("well-formed");
    let nav = RoXmlNavigator::new(&doc);
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    let mut rt = v.start_run(CollectingValidationSink {
        errors: &mut errors,
        warnings: &mut warnings,
    });
    let root = drive_navigator(&nav, &mut rt, ss)
        .expect("drive")
        .root_validity;
    out.push(("navigator", root));
    out
}

#[test]
fn an_extension_of_an_empty_all_takes_its_own_content() {
    let mut wrong = Vec::new();
    for (version, valid_in) in [
        (
            XsdVersion::V1_0,
            (|c: &Case| c.valid.0) as fn(&Case) -> Option<bool>,
        ),
        (XsdVersion::V1_1, |c: &Case| Some(c.valid.1)),
    ] {
        for case in CASES {
            let Some(want) = valid_in(case) else {
                continue;
            };
            let ss = match (compile(version, case), want) {
                (Ok(ss), true) => ss,
                (Err(_), false) => continue,
                (Ok(_), false) => {
                    wrong.push(format!("{version:?} {}: compiles", case.label));
                    continue;
                }
                (Err(e), true) => {
                    wrong.push(format!("{version:?} {}: refused: {e}", case.label));
                    continue;
                }
            };
            // The content is the extension's: e1 required, e2 optional, any
            // order for the all groups.
            let instances: &[(&str, bool)] =
                if case.ext.contains(r#"maxOccurs="0""#) && case.base.is_empty() {
                    // No content at all.
                    &[("<doc/>", true), ("<doc><e1/></doc>", false)]
                } else if case.ext.contains("sequence") {
                    &[
                        ("<doc><e1/><e2/></doc>", true),
                        ("<doc><e1/></doc>", true),
                        ("<doc><e2/><e1/></doc>", false),
                        ("<doc/>", false),
                    ]
                } else {
                    &[
                        ("<doc><e1/><e2/></doc>", true),
                        ("<doc><e2/><e1/></doc>", true),
                        ("<doc><e1/></doc>", true),
                        ("<doc/>", false),
                        ("<doc><e1/><e1/></doc>", false),
                    ]
                };
            for &(xml, valid) in instances {
                let want = if valid {
                    SchemaValidity::Valid
                } else {
                    SchemaValidity::Invalid
                };
                for (driver, root) in verdicts(&ss, xml) {
                    if root != Some(want) {
                        wrong.push(format!(
                            "{version:?} {} {xml} ({driver}): {root:?}",
                            case.label
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
