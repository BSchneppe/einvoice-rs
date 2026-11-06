//! A `default` / `fixed` value of type `xs:QName` / `xs:NOTATION` is read at
//! schema load with the namespace bindings of the schema element it is
//! written on; a prefix bound nowhere there gives it no value, and the schema
//! is in error.
//!
//! Structures 1.1 §3.3.6.1 Element Declaration Properties Correct: "2 If E
//! has a ·non-absent· {value constraint}, then E.{value constraint} is a
//! valid default with respect to E.{type definition} as defined in Element
//! Default Valid (Immediate) (§3.3.6.2)"; §3.2.6.1 Attribute Declaration
//! Properties Correct: "2 if there is a {value constraint}, then it is a valid
//! default with respect to the {type definition} as defined in Simple Default
//! Valid (§3.2.6.2)"; §3.5.6.1 Attribute Use Correct: "2 If U.{value
//! constraint} is not ·absent·, then it is a valid default with respect to
//! U.{attribute declaration}.{type definition} as defined in Simple Default
//! Valid (§3.2.6.2)". Simple Default Valid: "1 V.{lexical form} is ·valid·
//! with respect to T as defined by Datatype Valid … 2 V.{lexical form} maps
//! to V.{value} in the value space of T." Datatypes §3.3.18: "The mapping
//! from lexical space to value space for a particular QName ·literal· depends
//! on the namespace bindings in scope where the literal occurs. When QNames
//! appear in an XML context, the bindings to be used in the ·lexical mapping·
//! are those in the [in-scope namespaces] property of the relevant element."
//! XSD 1.0 §3.2.2 / §3.3.2 make the {value constraint} "a pair consisting of
//! the ·actual value· (with respect to the {type definition}) of that
//! [attribute]" and §3.2.6 / §3.3.6 require "the canonical lexical
//! representation of its value" to be valid: a literal without a value fails
//! there too.

use xsd_schema::{SchemaSetBuilder, XsdVersion};

const VERSIONS: [XsdVersion; 2] = [XsdVersion::V1_0, XsdVersion::V1_1];

/// `{BODY}` is each case's declarations; `p` is bound to `urn:p` on the
/// schema element.
const SCHEMA: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
    xmlns:p="urn:p" xmlns:t="urn:t" targetNamespace="urn:t">
  <xs:simpleType name="qnames"><xs:list itemType="xs:QName"/></xs:simpleType>
  <xs:simpleType name="qnameOrInt"><xs:union memberTypes="xs:QName xs:int"/></xs:simpleType>
  <xs:simpleType name="qnameOrString"><xs:union memberTypes="xs:QName xs:string"/></xs:simpleType>
  <xs:notation name="png" public="image/png"/>
  <xs:simpleType name="pic"><xs:restriction base="xs:NOTATION">
    <xs:enumeration value="t:png"/>
  </xs:restriction></xs:simpleType>
  <xs:attribute name="decl" type="xs:QName"/>
  {BODY}
</xs:schema>"#;

/// (label, declarations, `None` = the schema compiles, `Some(constraint)` =
/// refused with it)
const CASES: &[(&str, &str, Option<&str>)] = &[
    // Elements (e-props-correct.2).
    (
        "element default, unbound prefix",
        r#"<xs:element name="e" type="xs:QName" default="nope:x"/>"#,
        Some("e-props-correct.2"),
    ),
    (
        "element fixed, unbound prefix",
        r#"<xs:element name="e" type="xs:QName" fixed="nope:x"/>"#,
        Some("e-props-correct.2"),
    ),
    (
        "element default, a list member with an unbound prefix",
        r#"<xs:element name="e" type="t:qnames" default="p:a nope:b"/>"#,
        Some("e-props-correct.2"),
    ),
    (
        "element default, a union whose QName member has no value",
        r#"<xs:element name="e" type="t:qnameOrInt" default="nope:x"/>"#,
        Some("e-props-correct.2"),
    ),
    (
        "element fixed, NOTATION, unbound prefix",
        r#"<xs:element name="e" type="t:pic" fixed="nope:png"/>"#,
        Some("e-props-correct.2"),
    ),
    (
        "local element default, unbound prefix",
        r#"<xs:element name="r"><xs:complexType><xs:sequence>
             <xs:element name="e" type="xs:QName" default="nope:x"/>
           </xs:sequence></xs:complexType></xs:element>"#,
        Some("e-props-correct.2"),
    ),
    (
        "element of simple content, unbound prefix",
        r#"<xs:complexType name="sc"><xs:simpleContent>
             <xs:extension base="xs:QName"><xs:attribute name="a"/></xs:extension>
           </xs:simpleContent></xs:complexType>
           <xs:element name="e" type="t:sc" default="nope:x"/>"#,
        Some("e-props-correct.2"),
    ),
    // Attribute declarations and uses.
    (
        "attribute declaration default, unbound prefix",
        r#"<xs:attribute name="a" type="xs:QName" default="nope:x"/>"#,
        Some("a-props-correct"),
    ),
    (
        "local attribute use default, unbound prefix",
        r#"<xs:complexType name="c">
             <xs:attribute name="a" type="xs:QName" default="nope:x"/>
           </xs:complexType>"#,
        Some("a-props-correct"),
    ),
    (
        "attribute reference with a fixed value, unbound prefix",
        r#"<xs:complexType name="c">
             <xs:attribute ref="t:decl" fixed="nope:x"/>
           </xs:complexType>"#,
        Some("a-props-correct"),
    ),
    // The neighbours stay valid.
    (
        "element default, prefix bound on the schema element",
        r#"<xs:element name="e" type="xs:QName" default="p:x"/>"#,
        None,
    ),
    (
        "element default, prefix bound on the declaration",
        r#"<xs:element xmlns:q="urn:q" name="e" type="xs:QName" default="q:x"/>"#,
        None,
    ),
    (
        "element default, unprefixed",
        r#"<xs:element name="e" type="xs:QName" default="x"/>"#,
        None,
    ),
    (
        "element default, list of bound prefixes",
        r#"<xs:element name="e" type="t:qnames" default="p:a t:b"/>"#,
        None,
    ),
    (
        "element default, a union whose string member takes it",
        r#"<xs:element name="e" type="t:qnameOrString" default="nope:x"/>"#,
        None,
    ),
    (
        "element fixed, NOTATION, bound prefix",
        r#"<xs:element name="e" type="t:pic" fixed="t:png"/>"#,
        None,
    ),
    (
        "attribute declaration default, prefix bound on the declaration",
        r#"<xs:attribute xmlns:q="urn:q" name="a" type="xs:QName" default="q:x"/>"#,
        None,
    ),
    (
        "attribute reference with a fixed value, bound prefix",
        r#"<xs:complexType name="c">
             <xs:attribute ref="t:decl" fixed="p:x"/>
           </xs:complexType>"#,
        None,
    ),
    (
        "element default, a string",
        r#"<xs:element name="e" type="xs:string" default="nope:x"/>"#,
        None,
    ),
];

#[test]
fn a_qname_value_constraint_is_read_with_its_declarations_bindings() {
    let mut wrong = Vec::new();
    for version in VERSIONS {
        for &(label, body, refused) in CASES {
            let text = SCHEMA.replace("{BODY}", body);
            let result = SchemaSetBuilder::with_version(version)
                .add_source(&text, "file:///value_constraint_bindings.xsd")
                .map_err(|e| e.to_string())
                .and_then(|b| b.compile().map_err(|e| e.to_string()));
            match (result, refused) {
                (Ok(_), None) => {}
                (Ok(_), Some(code)) => {
                    wrong.push(format!("{version:?} {label}: compiles, expected {code}"))
                }
                (Err(e), None) => wrong.push(format!("{version:?} {label}: refused: {e}")),
                (Err(e), Some(code)) => {
                    if !e.contains(&format!("(constraint: {code})")) {
                        wrong.push(format!("{version:?} {label}: refused, not {code}: {e}"));
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
