//! The schema for the XML namespace that the crate supplies for a
//! namespace-only `<xs:import namespace="http://www.w3.org/XML/1998/namespace"/>`
//! is the current W3C edition (the one served at
//! `http://www.w3.org/2001/xml.xsd`, dated 2009-01): it declares `xml:id`
//! (`xs:ID`, from the xml:id Recommendation), allows the empty string for
//! `xml:lang` (a union of `xs:language` and `""`), and its `specialAttrs`
//! attribute group holds `xml:base`, `xml:lang`, `xml:space` and `xml:id`.

use xsd_schema::validation::{
    drive_quick_xml, CollectingValidationSink, SchemaValidator, SchemaValidity, ValidationFlags,
};
use xsd_schema::{SchemaSet, SchemaSetBuilder, XsdVersion};

use SchemaValidity::{Invalid, Valid};

const SCHEMA: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
    xmlns:xml="http://www.w3.org/XML/1998/namespace">
  <xs:import namespace="http://www.w3.org/XML/1998/namespace"/>
  <xs:element name="doc">
    <xs:complexType>
      <xs:sequence>
        <xs:element name="p" minOccurs="0" maxOccurs="unbounded">
          <xs:complexType>
            <xs:attributeGroup ref="xml:specialAttrs"/>
          </xs:complexType>
        </xs:element>
      </xs:sequence>
      <xs:attribute ref="xml:id"/>
      <xs:attribute ref="xml:lang"/>
    </xs:complexType>
  </xs:element>
</xs:schema>"#;

fn schema(version: XsdVersion) -> SchemaSet {
    SchemaSetBuilder::with_version(version)
        .add_source(SCHEMA, "file:///xml_namespace_schema.xsd")
        .and_then(|builder| builder.compile())
        .map(|compiled| compiled.into_schema_set())
        .unwrap_or_else(|e| panic!("{version:?}: schema compiles: {e}"))
}

/// Root validity and the error constraint codes of one streaming run.
fn run(ss: &SchemaSet, xml: &str) -> (Option<SchemaValidity>, Vec<String>) {
    let validator = SchemaValidator::new(ss, ValidationFlags::default());
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let validity = {
        let mut rt = validator.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        drive_quick_xml(xml.as_bytes(), &mut rt, ss)
            .expect("stream drive completes")
            .root_validity
    };
    (
        validity,
        errors.iter().map(|e| e.constraint.to_string()).collect(),
    )
}

#[test]
fn xml_id_is_declared_and_xml_lang_may_be_empty() {
    let rows: &[(&str, SchemaValidity, Option<&str>)] = &[
        (r#"<doc xml:id="a" xml:lang=""/>"#, Valid, None),
        (
            r#"<doc xml:lang="en-US"><p xml:id="b" xml:base="x/" xml:space="preserve" xml:lang=""/></doc>"#,
            Valid,
            None,
        ),
        // `xml:id` is an `xs:ID`: an NCName, unique in the document.
        (r#"<doc xml:id="1a"/>"#, Invalid, Some("cvc-")),
        (
            r#"<doc xml:id="a"><p xml:id="a"/></doc>"#,
            Invalid,
            Some("cvc-id.2"),
        ),
        // `xml:lang` is still a language tag (or empty); `xml:space` an enumeration.
        (r#"<doc xml:lang="not a tag"/>"#, Invalid, Some("cvc-")),
        (
            r#"<doc><p xml:space="bogus"/></doc>"#,
            Invalid,
            Some("cvc-"),
        ),
    ];
    let mut wrong = Vec::new();
    for version in [XsdVersion::V1_0, XsdVersion::V1_1] {
        let ss = schema(version);
        for (xml, expected, code) in rows {
            let (validity, errors) = run(&ss, xml);
            let ok = validity == Some(*expected)
                && match code {
                    Some(code) => errors.iter().any(|e| e.starts_with(code)),
                    None => errors.is_empty(),
                };
            if !ok {
                wrong.push(format!("{version:?}: {xml} => {validity:?} {errors:?}"));
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

/// The embedded document is the W3C file as published (8,836 bytes), not a
/// copy edited here.
#[test]
fn the_embedded_document_is_the_current_edition() {
    let text = std::str::from_utf8(xsd_schema::XML_XSD).expect("UTF-8");
    assert_eq!(xsd_schema::XML_XSD.len(), 8_836);
    assert!(text.contains(r#"<xs:attribute name="id" type="xs:ID">"#));
    assert!(text.contains(r#"<xs:attribute ref="xml:id"/>"#));
    assert!(text.contains(r#"<xs:enumeration value=""/>"#));
}
