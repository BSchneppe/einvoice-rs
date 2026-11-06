//! An extension whose explicit content is empty takes the base's content type.
//!
//! Structures §3.4.2.3.3 clause 2 makes the ·explicit content· of a
//! `<complexContent><extension>` empty not only when it has no particle
//! (2.1.1) but also for an `<all>` or `<sequence>` with no children (2.1.2),
//! a `<choice minOccurs="0">` with no children (2.1.3) and a particle with
//! `maxOccurs="0"` (2.1.4). Clause 4.2.2 then gives the extension the base's
//! content type: "If the {base type definition} is a complex type definition
//! whose {content type}.{variety} = element-only or mixed and the ·effective
//! content· is empty, then {base type definition}.{content type}".
//!
//! Only clause 2.1.1 used to be recognized, so extending `xs:anyType` (or any
//! mixed type) with `<xs:sequence/>` was rejected as element-only content over
//! a mixed base (`cos-ct-extends`), `<xs:all/>` over a non-empty base as an
//! all group nested in a sequence, and so on. Every case runs in XSD 1.0 and
//! XSD 1.1 mode; the XSD 1.0 rule (§3.4.2, "the effective content is empty …
//! then the {content type} of the {base type definition}") is the same here.

use xsd_schema::validation::{
    drive_quick_xml, CollectingValidationSink, SchemaValidator, SchemaValidity, ValidationFlags,
};
use xsd_schema::{SchemaResult, SchemaSet, SchemaSetBuilder};

const MODES: [bool; 2] = [false, true];

fn compile(xsd: &str, xsd11: bool) -> SchemaResult<SchemaSet> {
    let builder = if xsd11 {
        SchemaSetBuilder::xsd11()
    } else {
        SchemaSetBuilder::new()
    };
    builder
        .add_source(xsd, "file:///empty_extension_content.xsd")?
        .compile()
        .map(|compiled| compiled.into_schema_set())
}

fn validity(schema_set: &SchemaSet, xml: &str) -> Option<SchemaValidity> {
    let validator = SchemaValidator::new(schema_set, ValidationFlags::default());
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let sink = CollectingValidationSink {
        errors: &mut errors,
        warnings: &mut warnings,
    };
    let mut runtime = validator.start_run(sink);
    drive_quick_xml(xml.as_bytes(), &mut runtime, schema_set)
        .expect("driver completes")
        .root_validity
}

/// A schema whose type `C` extends `base` with `content` as the extension's
/// children, plus a global element `c` of type `C`. `extra` holds further
/// top-level declarations.
fn extension(base: &str, content: &str, extra: &str) -> String {
    format!(
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:t="urn:t" targetNamespace="urn:t">
  {extra}
  <xs:complexType name="C"><xs:complexContent><xs:extension base="{base}">{content}</xs:extension></xs:complexContent></xs:complexType>
  <xs:element name="c" type="t:C"/>
</xs:schema>"#
    )
}

/// Every spelling of empty explicit content (clauses 2.1.1–2.1.4).
const EMPTY_EXPLICIT_CONTENT: [&str; 7] = [
    "",
    "<xs:sequence/>",
    "<xs:sequence><xs:annotation><xs:documentation>none</xs:documentation></xs:annotation></xs:sequence>",
    "<xs:all/>",
    r#"<xs:choice minOccurs="0"/>"#,
    r#"<xs:sequence minOccurs="0" maxOccurs="0"><xs:element name="x"/></xs:sequence>"#,
    r#"<xs:choice minOccurs="0" maxOccurs="0"><xs:element name="x" type="xs:int"/><xs:element name="y"/></xs:choice>"#,
];

#[test]
fn any_type_extension_with_empty_explicit_content_is_any_type() {
    for xsd11 in MODES {
        for content in EMPTY_EXPLICIT_CONTENT {
            let xsd = extension("xs:anyType", content, "");
            let set = compile(&xsd, xsd11)
                .unwrap_or_else(|e| panic!("xsd11={xsd11} `{content}`: must compile: {e}"));
            // The content type is xs:anyType's: mixed, any element (lax).
            assert_eq!(
                validity(
                    &set,
                    r#"<t:c xmlns:t="urn:t">text<anything><t:else/></anything>more</t:c>"#
                ),
                Some(SchemaValidity::Valid),
                "xsd11={xsd11} `{content}`"
            );
        }
    }
}

#[test]
fn mixed_base_extension_with_empty_explicit_content_keeps_the_base_content() {
    let base = r#"<xs:complexType name="B" mixed="true"><xs:sequence><xs:element name="b"/></xs:sequence></xs:complexType>"#;
    for xsd11 in MODES {
        for content in EMPTY_EXPLICIT_CONTENT {
            let xsd = extension("t:B", content, base);
            let set = compile(&xsd, xsd11)
                .unwrap_or_else(|e| panic!("xsd11={xsd11} `{content}`: must compile: {e}"));
            assert_eq!(
                validity(&set, r#"<t:c xmlns:t="urn:t">text<b/>text</t:c>"#),
                Some(SchemaValidity::Valid),
                "xsd11={xsd11} `{content}`: the base's mixed content"
            );
            assert_eq!(
                validity(&set, r#"<t:c xmlns:t="urn:t"><b/><x/></t:c>"#),
                Some(SchemaValidity::Invalid),
                "xsd11={xsd11} `{content}`: nothing is added to the base's particle"
            );
        }
    }
}

#[test]
fn element_only_base_extension_with_empty_explicit_content_keeps_the_base_content() {
    let seq_base = r#"<xs:complexType name="B"><xs:sequence><xs:element name="b"/></xs:sequence></xs:complexType>"#;
    let all_base = r#"<xs:complexType name="B"><xs:all><xs:element name="b"/><xs:element name="d" minOccurs="0"/></xs:all></xs:complexType>"#;
    for xsd11 in MODES {
        for base in [seq_base, all_base] {
            for content in EMPTY_EXPLICIT_CONTENT {
                let xsd = extension("t:B", content, base);
                let set = compile(&xsd, xsd11).unwrap_or_else(|e| {
                    panic!("xsd11={xsd11} base `{base}` `{content}`: must compile: {e}")
                });
                assert_eq!(
                    validity(&set, r#"<t:c xmlns:t="urn:t"><b/></t:c>"#),
                    Some(SchemaValidity::Valid),
                    "xsd11={xsd11} base `{base}` `{content}`"
                );
                assert_eq!(
                    validity(&set, r#"<t:c xmlns:t="urn:t">text<b/></t:c>"#),
                    Some(SchemaValidity::Invalid),
                    "xsd11={xsd11} base `{base}` `{content}`: still element-only"
                );
            }
        }
    }
}

#[test]
fn non_empty_or_mixed_explicit_content_is_still_checked() {
    for xsd11 in MODES {
        // Element-only content over a mixed base: cos-ct-extends 1.4.3.2.2.1.
        let xsd = extension(
            "xs:anyType",
            r#"<xs:sequence><xs:element name="x"/></xs:sequence>"#,
            "",
        );
        let err = compile(&xsd, xsd11).expect_err("element-only over anyType");
        assert!(err.to_string().contains("cos-ct-extends"), "{err}");

        // Effective mixed true makes the empty explicit content an empty
        // sequence (clause 3.1.1): a mixed type over an element-only base.
        let base = r#"<xs:complexType name="B"><xs:sequence><xs:element name="b"/></xs:sequence></xs:complexType>"#;
        let xsd = format!(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:t="urn:t" targetNamespace="urn:t">
  {base}
  <xs:complexType name="C" mixed="true"><xs:complexContent><xs:extension base="t:B"><xs:sequence/></xs:extension></xs:complexContent></xs:complexType>
</xs:schema>"#
        );
        let err = compile(&xsd, xsd11).expect_err("mixed over element-only");
        assert!(err.to_string().contains("cos-ct-extends"), "{err}");
    }
}

/// A simple-content base: XSD 1.0 gives the extension the base's (simple)
/// content type (§3.4.2 clause 3.2.1, the same as for no particle at all);
/// XSD 1.1 maps it per clause 4.2.1 to an empty content type, which
/// cos-ct-extends clause 1.4 rejects.
#[test]
fn simple_content_base_follows_the_no_particle_case() {
    let base = r#"<xs:complexType name="B"><xs:simpleContent><xs:extension base="xs:int"/></xs:simpleContent></xs:complexType>"#;
    for content in ["", "<xs:sequence/>"] {
        let xsd = extension("t:B", content, base);
        let set =
            compile(&xsd, false).unwrap_or_else(|e| panic!("1.0 `{content}`: must compile: {e}"));
        assert_eq!(
            validity(&set, r#"<t:c xmlns:t="urn:t">42</t:c>"#),
            Some(SchemaValidity::Valid)
        );
        assert!(
            compile(&xsd, true).is_err(),
            "1.1 `{content}`: complexContent over simple content"
        );
    }
}

/// Errors inside a `maxOccurs="0"` explicit content are still reported: its
/// QName references must resolve and its local declarations must be
/// correct, exactly as before.
#[test]
fn errors_inside_max_occurs_zero_content_are_still_reported() {
    let cases = [
        (
            r#"<xs:sequence minOccurs="0" maxOccurs="0"><xs:element name="x" type="t:undefined"/></xs:sequence>"#,
            "src-resolve",
        ),
        (
            r#"<xs:sequence minOccurs="0" maxOccurs="0"><xs:element ref="t:undefined"/></xs:sequence>"#,
            "src-resolve",
        ),
        (
            r#"<xs:group ref="t:undefined" minOccurs="0" maxOccurs="0"/>"#,
            // Reported when the content model is compiled, as before.
            "unresolved group reference",
        ),
        (
            r#"<xs:sequence minOccurs="0" maxOccurs="0"><xs:element name="x" type="xs:int" default="abc"/></xs:sequence>"#,
            "e-props-correct",
        ),
        (
            r#"<xs:sequence minOccurs="0" maxOccurs="0"><xs:element name="x" minOccurs="3" maxOccurs="2"/></xs:sequence>"#,
            "p-props-correct",
        ),
        (r#"<xs:sequence maxOccurs="0"/>"#, "p-props-correct"),
    ];
    let base = r#"<xs:complexType name="B"><xs:sequence><xs:element name="b"/></xs:sequence></xs:complexType>"#;
    for xsd11 in MODES {
        for (content, constraint) in cases {
            let xsd = extension("t:B", content, base);
            let err = compile(&xsd, xsd11)
                .expect_err(&format!("xsd11={xsd11} `{content}` must be rejected"));
            assert!(
                err.to_string().contains(constraint),
                "xsd11={xsd11} `{content}`: expected {constraint}, got {err}"
            );
        }
    }
}

/// The components inside a `maxOccurs="0"` explicit content do not exist
/// (§3.8.2: "unless minOccurs=maxOccurs=0, in which case the item corresponds
/// to no component at all"), so they take no part in Element Declarations
/// Consistent (§3.8.6.3) with the base's particle.
#[test]
fn max_occurs_zero_content_takes_no_part_in_element_declarations_consistent() {
    let base = r#"<xs:complexType name="B"><xs:sequence><xs:element name="a"/></xs:sequence></xs:complexType>"#;
    for xsd11 in MODES {
        let xsd = extension(
            "t:B",
            r#"<xs:sequence minOccurs="0" maxOccurs="0"><xs:element name="a" type="xs:int"/></xs:sequence>"#,
            base,
        );
        compile(&xsd, xsd11).unwrap_or_else(|e| panic!("xsd11={xsd11}: must compile: {e}"));
    }
}
