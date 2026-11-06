#![allow(dead_code)] // Shared by native regressions and the raw-byte comparison executable.
use xsd_schema::pipeline::load_and_process_schema;
use xsd_schema::validation::errors::ValidationError;
use xsd_schema::validation::*;
use xsd_schema::{SchemaSet, XsdVersion};

pub const XSD: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
 <xs:simpleType name="Positive"><xs:restriction base="xs:decimal"><xs:minInclusive value="0"/></xs:restriction></xs:simpleType>
 <xs:complexType name="Body"><xs:sequence><xs:element name="d" type="Positive" minOccurs="2" maxOccurs="2"/><xs:element name="z" type="xs:string"/></xs:sequence><xs:attribute name="req" type="xs:int" use="required"/><xs:attribute name="fixed" type="xs:string" fixed="ok"/><xs:attribute name="no" use="prohibited"/></xs:complexType>
 <xs:element name="r" type="Body"/>
 <xs:element name="n" type="xs:int" nillable="true"/>
 <xs:element name="nf" type="xs:int" nillable="true" fixed="1"/>
 <xs:element name="ref" type="xs:IDREF"/>
 <xs:element name="refs"><xs:complexType><xs:sequence><xs:element name="i" type="xs:IDREF" maxOccurs="unbounded"/></xs:sequence></xs:complexType></xs:element>
 <xs:element name="decimals"><xs:complexType><xs:sequence><xs:element name="d" type="xs:decimal" maxOccurs="unbounded"/></xs:sequence></xs:complexType></xs:element>
</xs:schema>"#;
pub const VALID: &str = r#"<r req="1"><d>1</d><d>2</d><z/></r>"#;
pub const IDREFS: &str = "<refs><i>absent-a</i><i>absent-b</i><i>absent-c</i></refs>";
pub const CAPACITY: &str = "<decimals><d>10000000000000000000000000000000000000000</d><d>10000000000000000000000000000000000000000</d><d>10000000000000000000000000000000000000000</d></decimals>";
pub const CASES: &[(&str, &str)] = &[
    ("valid", VALID),
    (
        "duplicate-bad",
        r#"<r req="1"><d>bad</d><d>bad</d><z/></r>"#,
    ),
    ("facet", r#"<r req="1"><d>-1</d><d>2</d><z/></r>"#),
    ("extra", r#"<r req="1"><d>1</d><d>2</d><z/><extra/></r>"#),
    ("order", r#"<r req="1"><z/><d>1</d><d>2</d></r>"#),
    ("missing-child", r#"<r req="1"><d>1</d></r>"#),
    ("missing-attribute", r#"<r><d>1</d><d>2</d><z/></r>"#),
    (
        "extra-attribute",
        r#"<r req="1" extra="x"><d>1</d><d>2</d><z/></r>"#,
    ),
    ("bad-attribute", r#"<r req="bad"><d>1</d><d>2</d><z/></r>"#),
    (
        "fixed-attribute",
        r#"<r req="1" fixed="no"><d>1</d><d>2</d><z/></r>"#,
    ),
    (
        "prohibited-attribute",
        r#"<r req="1" no="x"><d>1</d><d>2</d><z/></r>"#,
    ),
    ("unknown-root", "<other/>"),
    (
        "xsi-nil",
        r#"<r xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" req="1" xsi:nil="true"><d>1</d><d>2</d><z/></r>"#,
    ),
    (
        "nil-content",
        r#"<n xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:nil="true">1</n>"#,
    ),
    (
        "nil-fixed",
        r#"<nf xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:nil="true"/>"#,
    ),
    (
        "nil-lexical",
        r#"<n xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:nil="bad"/>"#,
    ),
    (
        "xsi-type-unresolved",
        r#"<n xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:type="missing">1</n>"#,
    ),
    (
        "xsi-type-prefix",
        r#"<n xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:type="u:T">1</n>"#,
    ),
    (
        "xsi-type-derivation",
        r#"<n xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:xs="http://www.w3.org/2001/XMLSchema" xsi:type="xs:string">1</n>"#,
    ),
    ("idrefs", IDREFS),
    ("warning", CAPACITY),
];

pub fn schema(source: &str) -> SchemaSet {
    let mut schema = SchemaSet::new();
    schema.xsd_version = XsdVersion::V1_0;
    load_and_process_schema(source.as_bytes(), "authored.xsd", &mut schema, None).unwrap();
    assert!(!schema.has_parsing_errors());
    schema
}

pub fn capture(
    schema: &SchemaSet,
    xml: &str,
) -> (DriveOutcome, Vec<ValidationError>, Vec<ValidationWarning>) {
    let validator = SchemaValidator::new(schema, ValidationFlags::default());
    assert!(validator.content_model_failures().is_empty());
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let sink = CollectingValidationSink {
        errors: &mut errors,
        warnings: &mut warnings,
    };
    let mut runtime = validator.start_run(sink);
    let result = drive_quick_xml(xml.as_bytes(), &mut runtime, schema).unwrap();
    assert!(runtime.operational_failure().is_none());
    (result, errors, warnings)
}

pub fn case(name: &str) -> Vec<ValidationError> {
    let schema = schema(XSD);
    let xml = CASES.iter().find(|(n, _)| *n == name).unwrap().1;
    capture(&schema, xml).1
}
