//! A simple-content restriction whose content type restricts a member of the
//! base's union content type.
//!
//! Structures 1.1 §3.4.6.4 Derivation Valid (Restriction, Complex) clause
//! 2.2.2.1: "Let SB be B.{content type}.{simple type definition}, and ST be
//! T.{content type}.{simple type definition}. Then ST is validly derived from
//! SB as defined in Type Derivation OK (Simple) (§3.16.6.3)", whose clause
//! "2.2.4 All of the following are true: 2.2.4.1 B.{variety} = union. 2.2.4.2
//! D is validly ·derived· from a type definition M in B's transitive
//! membership given S, as defined by this constraint. 2.2.4.3 The {facets}
//! property of B and of any intervening union datatypes is empty." XSD 1.0
//! §3.4.6 Derivation Valid (Restriction, Complex) clause 5.2.2.1 ("The
//! {content type} of the {base type definition} must be a simple type
//! definition from which the {content type} is validly derived given the
//! empty set as defined in Type Derivation OK (Simple) (§3.14.6)") and §3.14.6
//! clause "2.2.4 B's {variety} is union and D is validly derived from a type
//! definition in B's {member type definitions} given the subset".

use xsd_schema::validation::{
    drive_navigator, drive_quick_xml, CollectingValidationSink, SchemaValidator, SchemaValidity,
    ValidationFlags,
};
use xsd_schema::{RoXmlNavigator, SchemaSet, SchemaSetBuilder, XsdVersion};

const VERSIONS: [XsdVersion; 2] = [XsdVersion::V1_0, XsdVersion::V1_1];

/// `{INLINE}` is the `<xs:simpleType>` of `T`'s restriction.
const SCHEMA: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
    xmlns:t="urn:t" targetNamespace="urn:t">
  <xs:simpleType name="small"><xs:restriction base="xs:int">
    <xs:maxInclusive value="10"/>
  </xs:restriction></xs:simpleType>
  <xs:simpleType name="u"><xs:union memberTypes="t:small xs:integer xs:string"/></xs:simpleType>
  <xs:simpleType name="uf"><xs:restriction base="t:u"><xs:pattern value=".{1,3}"/></xs:restriction></xs:simpleType>
  <xs:complexType name="S"><xs:simpleContent>
    <xs:extension base="t:u"><xs:attribute name="a"/></xs:extension>
  </xs:simpleContent></xs:complexType>
  <xs:complexType name="SF"><xs:simpleContent>
    <xs:extension base="t:uf"><xs:attribute name="a"/></xs:extension>
  </xs:simpleContent></xs:complexType>
  <xs:complexType name="T"><xs:simpleContent>
    <xs:restriction base="t:{BASE}">{INLINE}</xs:restriction>
  </xs:simpleContent></xs:complexType>
  <xs:element name="e" type="t:T"/>
</xs:schema>"#;

struct Case {
    label: &'static str,
    base: &'static str,
    inline: &'static str,
    /// The schema's validity per version (1.0, 1.1); `None`: not checked.
    valid: (Option<bool>, bool),
    /// Instances (content, valid) for a valid schema.
    instances: &'static [(&'static str, bool)],
}

const CASES: &[Case] = &[
    Case {
        label: "a restriction of the member xs:string",
        base: "S",
        inline: r#"<xs:simpleType><xs:restriction base="xs:string"/></xs:simpleType>"#,
        valid: (Some(true), true),
        instances: &[("abc", true), ("12", true)],
    },
    Case {
        label: "a restriction of the member xs:integer with a facet",
        base: "S",
        inline: r#"<xs:simpleType><xs:restriction base="xs:integer">
                     <xs:maxInclusive value="5"/></xs:restriction></xs:simpleType>"#,
        valid: (Some(true), true),
        instances: &[("3", true), ("7", false), ("x", false)],
    },
    Case {
        label: "a restriction of a type derived from the member xs:integer",
        base: "S",
        inline: r#"<xs:simpleType><xs:restriction base="xs:short"/></xs:simpleType>"#,
        valid: (Some(true), true),
        instances: &[("3", true), ("70000", false)],
    },
    Case {
        label: "a restriction of the user-defined member t:small",
        base: "S",
        inline: r#"<xs:simpleType><xs:restriction base="t:small">
                     <xs:minInclusive value="2"/></xs:restriction></xs:simpleType>"#,
        valid: (Some(true), true),
        instances: &[("3", true), ("1", false), ("11", false)],
    },
    // The neighbours stay refused.
    Case {
        label: "a restriction of xs:decimal (a base type of a member, not a member)",
        base: "S",
        inline: r#"<xs:simpleType><xs:restriction base="xs:decimal"/></xs:simpleType>"#,
        valid: (Some(false), false),
        instances: &[],
    },
    Case {
        label: "a list",
        base: "S",
        inline: r#"<xs:simpleType><xs:list itemType="xs:string"/></xs:simpleType>"#,
        valid: (Some(false), false),
        instances: &[],
    },
    // XSD 1.1 clause 2.2.4.3: a union with facets admits no member type
    // (XSD 1.0 has no such clause; this crate applies 2.2.4.3 in both
    // versions and is not checked for 1.0 here).
    Case {
        label: "a restriction of a member of a union with facets",
        base: "SF",
        inline: r#"<xs:simpleType><xs:restriction base="xs:string"/></xs:simpleType>"#,
        valid: (None, false),
        instances: &[],
    },
];

fn compile(version: XsdVersion, case: &Case) -> Result<SchemaSet, String> {
    let text = SCHEMA
        .replace("{BASE}", case.base)
        .replace("{INLINE}", case.inline);
    SchemaSetBuilder::with_version(version)
        .add_source(&text, "file:///simple_content_union_member.xsd")
        .map_err(|e| e.to_string())?
        .compile()
        .map(|c| c.into_schema_set())
        .map_err(|e| e.to_string())
}

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
fn a_simple_content_restriction_may_restrict_a_union_member() {
    let mut wrong = Vec::new();
    for version in VERSIONS {
        for case in CASES {
            let want = match version {
                XsdVersion::V1_0 => case.valid.0,
                _ => Some(case.valid.1),
            };
            let Some(want) = want else {
                continue;
            };
            let ss = match (compile(version, case), want) {
                (Ok(ss), true) => ss,
                (Err(e), false) => {
                    if !e.contains("derivation-ok-restriction") {
                        wrong.push(format!(
                            "{version:?} {}: refused, not derivation-ok-restriction: {e}",
                            case.label
                        ));
                    }
                    continue;
                }
                (Ok(_), false) => {
                    wrong.push(format!("{version:?} {}: compiles", case.label));
                    continue;
                }
                (Err(e), true) => {
                    wrong.push(format!("{version:?} {}: refused: {e}", case.label));
                    continue;
                }
            };
            for &(content, valid) in case.instances {
                let xml = format!(r#"<t:e xmlns:t="urn:t" a="x">{content}</t:e>"#);
                let want = if valid {
                    SchemaValidity::Valid
                } else {
                    SchemaValidity::Invalid
                };
                for (driver, root) in verdicts(&ss, &xml) {
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
