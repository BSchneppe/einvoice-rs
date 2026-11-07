//! `<xs:anyAttribute namespace="##other"/>` never admits unqualified attributes.
//!
//! Structures §3.10.2.2 maps `namespace="##other"` to {variety} "not" with
//! {namespaces} "a set consisting of ·absent· and, if the targetNamespace
//! [attribute] of the `<schema>` ancestor element information item is
//! present, its ·actual value·" — a mapping shared by `<any>` and
//! `<anyAttribute>`. By §3.10.4.3 clause 2 a namespace name (or ·absent·)
//! is allowed by such a constraint only if it "is not identical to any of
//! the members of C.{namespaces}". XSD 1.0 reads `##other` the same way.
//!
//! The XSD 1.1 attribute-wildcard path used to drop ·absent· from that set,
//! so in XSD 1.1 mode an `##other` attribute wildcard accepted attributes
//! without a namespace, and the attribute-wildcard union, intersection and
//! subset checks worked on the wrong set. Every case runs in XSD 1.0 and
//! XSD 1.1 mode where the schema is valid in both.

use xsd_schema::validation::{
    drive_quick_xml, CollectingValidationSink, SchemaValidator, SchemaValidity, ValidationFlags,
};
use xsd_schema::{SchemaResult, SchemaSet, SchemaSetBuilder};

/// Compile schema documents given as `(base URI, text)`; documents import
/// each other's namespace without a schema location.
fn compile(documents: &[(&str, &str)], xsd11: bool) -> SchemaResult<SchemaSet> {
    let mut builder = if xsd11 {
        SchemaSetBuilder::xsd11()
    } else {
        SchemaSetBuilder::new()
    };
    for (uri, text) in documents {
        builder = builder.add_source(text, uri)?;
    }
    builder.compile().map(|compiled| compiled.into_schema_set())
}

fn compile_one(text: &str, xsd11: bool) -> SchemaResult<SchemaSet> {
    compile(&[("file:///s.xsd", text)], xsd11)
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

fn check(set: &SchemaSet, label: &str, xml: &str, expected: SchemaValidity) {
    assert_eq!(validity(set, xml), Some(expected), "{label}: {xml}");
}

/// Schema document with target namespace `urn:b` (or none) and `body`.
fn doc(target_namespace: Option<&str>, body: &str) -> String {
    let tns = match target_namespace {
        Some(ns) => format!(r###" xmlns:b="{ns}" targetNamespace="{ns}""###),
        None => String::new(),
    };
    format!(
        r###"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"{tns}>
{body}
</xs:schema>"###
    )
}

/// `urn:a` schema document with `body`.
fn doc_a(body: &str) -> String {
    format!(
        r###"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:a="urn:a" targetNamespace="urn:a">
{body}
</xs:schema>"###
    )
}

/// `urn:b` schema document importing `urn:a`, with `body`.
fn doc_b(body: &str) -> String {
    format!(
        r###"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:a="urn:a" xmlns:b="urn:b" targetNamespace="urn:b">
  <xs:import namespace="urn:a"/>
{body}
</xs:schema>"###
    )
}

const MODES: [bool; 2] = [false, true];

/// The reported case: target namespace `urn:b`, `##other` = not(urn:b, absent).
#[test]
fn other_attribute_wildcard_with_target_namespace() {
    for process_contents in ["skip", "lax"] {
        let schema = doc(
            Some("urn:b"),
            &format!(
                r###"  <xs:element name="ra"><xs:complexType>
    <xs:anyAttribute namespace="##other" processContents="{process_contents}"/>
  </xs:complexType></xs:element>"###
            ),
        );
        for xsd11 in MODES {
            let set = compile_one(&schema, xsd11).unwrap_or_else(|e| panic!("xsd11={xsd11}: {e}"));
            let label = format!("xsd11={xsd11} processContents={process_contents}");
            // An unqualified attribute: absent ∈ {namespaces}.
            check(
                &set,
                &label,
                r###"<b:ra xmlns:b="urn:b" x="1"/>"###,
                SchemaValidity::Invalid,
            );
            // An attribute in the target namespace.
            check(
                &set,
                &label,
                r###"<b:ra xmlns:b="urn:b" b:x="1"/>"###,
                SchemaValidity::Invalid,
            );
            // An attribute in a third namespace.
            check(
                &set,
                &label,
                r###"<b:ra xmlns:b="urn:b" xmlns:c="urn:c" c:x="1"/>"###,
                SchemaValidity::Valid,
            );
        }
    }
}

/// No target namespace: `##other` = not(absent).
#[test]
fn other_attribute_wildcard_without_target_namespace() {
    let schema = doc(
        None,
        r###"  <xs:element name="ra"><xs:complexType>
    <xs:anyAttribute namespace="##other" processContents="skip"/>
  </xs:complexType></xs:element>"###,
    );
    for xsd11 in MODES {
        let set = compile_one(&schema, xsd11).unwrap_or_else(|e| panic!("xsd11={xsd11}: {e}"));
        let label = format!("xsd11={xsd11}");
        check(&set, &label, r###"<ra x="1"/>"###, SchemaValidity::Invalid);
        check(
            &set,
            &label,
            r###"<ra xmlns:c="urn:c" c:x="1"/>"###,
            SchemaValidity::Valid,
        );
    }
}

/// An `##other` wildcard contributed by an attribute group, alone and
/// intersected with the type's own wildcard (§3.6.2.2, §3.10.6.4:
/// not(S1) ∩ enumeration(S2) = S2 minus S1).
#[test]
fn other_attribute_wildcard_from_an_attribute_group_and_intersection() {
    let schema = doc(
        Some("urn:b"),
        r###"  <xs:attributeGroup name="AG"><xs:anyAttribute namespace="##other" processContents="skip"/></xs:attributeGroup>
  <xs:element name="rg"><xs:complexType>
    <xs:attributeGroup ref="b:AG"/>
  </xs:complexType></xs:element>
  <xs:element name="ri"><xs:complexType>
    <xs:attributeGroup ref="b:AG"/>
    <xs:anyAttribute namespace="##local urn:c" processContents="skip"/>
  </xs:complexType></xs:element>"###,
    );
    for xsd11 in MODES {
        let set = compile_one(&schema, xsd11).unwrap_or_else(|e| panic!("xsd11={xsd11}: {e}"));
        let label = format!("xsd11={xsd11}");
        check(
            &set,
            &label,
            r###"<b:rg xmlns:b="urn:b" x="1"/>"###,
            SchemaValidity::Invalid,
        );
        check(
            &set,
            &label,
            r###"<b:rg xmlns:b="urn:b" xmlns:d="urn:d" d:x="1"/>"###,
            SchemaValidity::Valid,
        );
        // {absent, urn:c} minus {urn:b, absent} = {urn:c}.
        check(
            &set,
            &label,
            r###"<b:ri xmlns:b="urn:b" x="1"/>"###,
            SchemaValidity::Invalid,
        );
        check(
            &set,
            &label,
            r###"<b:ri xmlns:b="urn:b" xmlns:c="urn:c" c:x="1"/>"###,
            SchemaValidity::Valid,
        );
        check(
            &set,
            &label,
            r###"<b:ri xmlns:b="urn:b" xmlns:d="urn:d" d:x="1"/>"###,
            SchemaValidity::Invalid,
        );
    }
}

/// Extension unions an inherited `##other` with the extension's own
/// wildcard (§3.4.2.5, §3.10.6.3).
#[test]
fn other_attribute_wildcard_in_an_extension_union() {
    // not(urn:a, absent) ∪ not(urn:b, absent) = not(absent) (§3.10.6.3
    // clause 4.2; XSD 1.0 §3.10.6 clause 5 gives the same).
    let a = doc_a(
        r###"  <xs:complexType name="BaseA"><xs:anyAttribute namespace="##other" processContents="skip"/></xs:complexType>"###,
    );
    let b = doc_b(
        r###"  <xs:complexType name="D"><xs:complexContent><xs:extension base="a:BaseA">
    <xs:anyAttribute namespace="##other" processContents="skip"/>
  </xs:extension></xs:complexContent></xs:complexType>
  <xs:element name="r" type="b:D"/>"###,
    );
    for xsd11 in MODES {
        let set = compile(&[("file:///a.xsd", &a), ("file:///b.xsd", &b)], xsd11)
            .unwrap_or_else(|e| panic!("xsd11={xsd11}: {e}"));
        let label = format!("xsd11={xsd11}");
        check(
            &set,
            &label,
            r###"<b:r xmlns:b="urn:b" x="1"/>"###,
            SchemaValidity::Invalid,
        );
        for prefix in ["a", "b", "c"] {
            check(
                &set,
                &label,
                &format!(
                    r###"<b:r xmlns:a="urn:a" xmlns:b="urn:b" xmlns:c="urn:c" {prefix}:x="1"/>"###
                ),
                SchemaValidity::Valid,
            );
        }
    }

    // not(urn:b, absent) ∪ {absent} = not(urn:b) in XSD 1.1 (clause 5.2);
    // XSD 1.0 cannot express that union (msData wildZ013).
    let schema = doc(
        Some("urn:b"),
        r###"  <xs:complexType name="Base"><xs:anyAttribute namespace="##other" processContents="skip"/></xs:complexType>
  <xs:complexType name="D"><xs:complexContent><xs:extension base="b:Base">
    <xs:anyAttribute namespace="##local" processContents="skip"/>
  </xs:extension></xs:complexContent></xs:complexType>
  <xs:element name="r" type="b:D"/>"###,
    );
    let err = compile_one(&schema, false).expect_err("XSD 1.0: union not expressible");
    assert!(err.to_string().contains("cos-aw-union"), "{err}");
    let set = compile_one(&schema, true).unwrap_or_else(|e| panic!("xsd11: {e}"));
    check(
        &set,
        "xsd11",
        r###"<b:r xmlns:b="urn:b" x="1"/>"###,
        SchemaValidity::Valid,
    );
    check(
        &set,
        "xsd11",
        r###"<b:r xmlns:b="urn:b" b:x="1"/>"###,
        SchemaValidity::Invalid,
    );
}

/// `urn:b` document: `D` restricts `Base` (`##other`, lax) with `content`.
fn restriction_of_other(content: &str) -> String {
    doc(
        Some("urn:b"),
        &format!(
            r###"  <xs:complexType name="Base"><xs:anyAttribute namespace="##other" processContents="lax"/></xs:complexType>
  <xs:complexType name="D"><xs:complexContent><xs:restriction base="b:Base">{content}</xs:restriction></xs:complexContent></xs:complexType>"###
        ),
    )
}

fn any(attrs: &str) -> String {
    format!(r###"<xs:anyAttribute {attrs} processContents="lax"/>"###)
}

/// Restriction of an `##other` attribute wildcard: Wildcard Subset
/// (§3.10.6.2) and the attribute clause of derivation-ok-restriction
/// (§3.4.6.3 clause 3) now see ·absent· in the base's {namespaces}.
#[test]
fn other_attribute_wildcard_restriction() {
    let both = [
        // {absent} ⊄ not(urn:b, absent).
        (any(r###"namespace="##local""###), false),
        (any(r###"namespace="##local urn:c""###), false),
        // {urn:c} ⊆ not(urn:b, absent).
        (any(r###"namespace="urn:c""###), true),
        (any(r###"namespace="##other""###), true),
        // An unqualified attribute use the base wildcard does not allow.
        (r###"<xs:attribute name="x"/>"###.to_string(), false),
        (
            format!(
                r###"<xs:attribute name="x"/>{}"###,
                any(r###"namespace="##other""###)
            ),
            false,
        ),
    ];
    for xsd11 in MODES {
        for (content, valid) in &both {
            let result = compile_one(&restriction_of_other(content), xsd11);
            assert_eq!(
                result.is_ok(),
                *valid,
                "xsd11={xsd11} `{content}`: {:?}",
                result.err()
            );
        }
    }
}

/// XSD 1.1 `notNamespace` spells the same constraints explicitly.
#[cfg(feature = "xsd11")]
#[test]
fn other_attribute_wildcard_restriction_by_not_namespace() {
    let cases = [
        // not(urn:b) admits absent.
        (any(r###"notNamespace="##targetNamespace""###), false),
        // not(urn:b, absent) is `##other` itself.
        (any(r###"notNamespace="##targetNamespace ##local""###), true),
        (any(r###"notNamespace="urn:b ##local urn:c""###), true),
    ];
    for (content, valid) in &cases {
        let result = compile_one(&restriction_of_other(content), true);
        assert_eq!(
            result.is_ok(),
            *valid,
            "xsd11 `{content}`: {:?}",
            result.err()
        );
    }

    // ... and the other way round: `##other` restricts its explicit spelling.
    let schema = doc(
        Some("urn:b"),
        r###"  <xs:complexType name="Base"><xs:anyAttribute notNamespace="##targetNamespace ##local" processContents="lax"/></xs:complexType>
  <xs:complexType name="D"><xs:complexContent><xs:restriction base="b:Base">
    <xs:anyAttribute namespace="##other" processContents="lax"/>
  </xs:restriction></xs:complexContent></xs:complexType>"###,
    );
    compile_one(&schema, true).unwrap_or_else(|e| panic!("xsd11: {e}"));
}

/// An `##other` attribute wildcard inherited from, or referenced in, another
/// schema document keeps the namespaces of its own document (§3.10.2.2) and
/// still excludes ·absent·.
#[test]
fn inherited_other_attribute_wildcard_excludes_absent() {
    let a = doc_a(
        r###"  <xs:complexType name="BaseA"><xs:anyAttribute namespace="##other" processContents="skip"/></xs:complexType>
  <xs:attributeGroup name="AG"><xs:anyAttribute namespace="##other" processContents="skip"/></xs:attributeGroup>"###,
    );
    let b = doc_b(
        r###"  <xs:complexType name="DA"><xs:complexContent><xs:extension base="a:BaseA"/></xs:complexContent></xs:complexType>
  <xs:complexType name="DG"><xs:attributeGroup ref="a:AG"/></xs:complexType>
  <xs:element name="ra" type="b:DA"/>
  <xs:element name="rg" type="b:DG"/>"###,
    );
    for xsd11 in MODES {
        let set = compile(&[("file:///a.xsd", &a), ("file:///b.xsd", &b)], xsd11)
            .unwrap_or_else(|e| panic!("xsd11={xsd11}: {e}"));
        let label = format!("xsd11={xsd11}");
        for element in ["ra", "rg"] {
            check(
                &set,
                &label,
                &format!(r###"<b:{element} xmlns:b="urn:b" b:x="1"/>"###),
                SchemaValidity::Valid,
            );
            check(
                &set,
                &label,
                &format!(r###"<b:{element} xmlns:b="urn:b" xmlns:a="urn:a" a:x="1"/>"###),
                SchemaValidity::Invalid,
            );
            check(
                &set,
                &label,
                &format!(r###"<b:{element} xmlns:b="urn:b" x="1"/>"###),
                SchemaValidity::Invalid,
            );
        }
    }

    // A base from a document without a target namespace: not(absent).
    let n = doc(
        None,
        r###"  <xs:complexType name="BaseN"><xs:anyAttribute namespace="##other" processContents="skip"/></xs:complexType>"###,
    );
    let b = r###"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:b="urn:b" targetNamespace="urn:b">
  <xs:import/>
  <xs:complexType name="DN"><xs:complexContent><xs:extension base="BaseN"/></xs:complexContent></xs:complexType>
  <xs:element name="rn" type="b:DN"/>
</xs:schema>"###;
    for xsd11 in MODES {
        let set = compile(&[("file:///n.xsd", &n), ("file:///b.xsd", b)], xsd11)
            .unwrap_or_else(|e| panic!("xsd11={xsd11}: {e}"));
        let label = format!("xsd11={xsd11}");
        check(
            &set,
            &label,
            r###"<b:rn xmlns:b="urn:b" b:x="1"/>"###,
            SchemaValidity::Valid,
        );
        check(
            &set,
            &label,
            r###"<b:rn xmlns:b="urn:b" x="1"/>"###,
            SchemaValidity::Invalid,
        );
    }
}
