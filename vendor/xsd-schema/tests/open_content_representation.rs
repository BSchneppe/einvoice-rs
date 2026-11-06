//! An `<xs:openContent>` whose mode is not `none` must have an `<xs:any>`
//! child (C25), and an `<xs:defaultOpenContent>` always must.
//!
//! Structures 1.1 §3.4.3, Schema Representation Constraint: Complex Type
//! Definition Representation OK: "3 If <openContent> is present and has mode
//! ≠ 'none', then there must be an <any> among the [children] of
//! <openContent>. 4 If <openContent> is present and has mode = 'none', then
//! there must not be an <any> among the [children] of <openContent>."
//!
//! §3.4.2 XML Representation Summary: `<defaultOpenContent appliesToEmpty =
//! boolean : false id = ID mode = (interleave | suffix) : interleave>
//! Content: (annotation?, any)` — the schema for schemas requires the
//! wildcard.
#![cfg(feature = "xsd11")]

use xsd_schema::{SchemaError, SchemaSetBuilder};

const XS: &str = r#"xmlns:xs="http://www.w3.org/2001/XMLSchema""#;

fn compile(body: &str) -> Result<(), SchemaError> {
    let xsd = format!("<xs:schema {XS}>\n{body}\n</xs:schema>");
    SchemaSetBuilder::xsd11()
        .add_source(&xsd, "file:///open-content/main.xsd")?
        .compile()
        .map(|_| ())
}

fn constraint(error: &SchemaError) -> &str {
    match error {
        SchemaError::StructuralError { constraint, .. } => constraint,
        _ => "",
    }
}

fn complex_type(open_content: &str) -> String {
    format!(
        r#"<xs:complexType name="T">
{open_content}
<xs:sequence><xs:element name="a"/></xs:sequence>
</xs:complexType>"#
    )
}

#[test]
fn open_content_with_a_mode_needs_a_wildcard() {
    for open in [
        r#"<xs:openContent mode="interleave"/>"#,
        r#"<xs:openContent mode="suffix"/>"#,
        r#"<xs:openContent/>"#,
        r#"<xs:openContent mode="interleave"><xs:annotation/></xs:openContent>"#,
    ] {
        let error = compile(&complex_type(open)).expect_err(open);
        assert_eq!(constraint(&error), "src-ct", "{open}: {error}");
        let text = error.to_string();
        assert!(text.contains("main.xsd:3:"), "{open}: located: {text}");
        assert!(text.contains("xs:any"), "{open}: {text}");
    }
}

#[test]
fn open_content_with_a_wildcard_or_mode_none_is_fine() {
    for open in [
        r#"<xs:openContent mode="interleave"><xs:any/></xs:openContent>"#,
        r###"<xs:openContent mode="suffix"><xs:any namespace="##other"/></xs:openContent>"###,
        r#"<xs:openContent><xs:any/></xs:openContent>"#,
        r#"<xs:openContent mode="none"/>"#,
    ] {
        compile(&complex_type(open)).unwrap_or_else(|e| panic!("{open}: {e}"));
    }
}

#[test]
fn default_open_content_needs_a_wildcard() {
    for open in [
        r#"<xs:defaultOpenContent/>"#,
        r#"<xs:defaultOpenContent mode="suffix"/>"#,
        r#"<xs:defaultOpenContent mode="interleave"><xs:annotation/></xs:defaultOpenContent>"#,
    ] {
        let error = compile(&format!("{open}\n<xs:element name=\"e\"/>")).expect_err(open);
        // Refused before this batch already, as cos-valid-default-oc.
        assert_eq!(
            constraint(&error),
            "cos-valid-default-oc",
            "{open}: {error}"
        );
        assert!(
            error.to_string().contains("main.xsd:2:"),
            "{open}: located: {error}"
        );
    }
    compile("<xs:defaultOpenContent><xs:any/></xs:defaultOpenContent>\n<xs:element name=\"e\"/>")
        .expect("with a wildcard");
}
