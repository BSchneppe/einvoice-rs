//! `xs:QName` and `xs:NOTATION` values are compared and seen in their value
//! space during validation.
//!
//! Datatypes §3.3.18: "The ·value space· of QName is the set of tuples
//! {namespace name, local part}", and "The mapping from lexical space to value
//! space for a particular QName ·literal· depends on the namespace bindings in
//! scope where the literal occurs" — for an instance value the [in-scope
//! namespaces] of its element, for a facet or value-constraint literal those of
//! the schema element it is written on; "unqualified names are bound to the
//! default namespace". §3.3.19 gives NOTATION the same lexical mapping. So:
//!
//! - `enumeration` (Datatypes §4.3.5.4: "equal or identical to one of the
//!   values specified in {value}") compares expanded names: `e:Sender` with
//!   `e` bound to the namespace `tns` is bound to in the schema matches
//!   `tns:Sender`, and `tns:Sender` with `tns` bound elsewhere does not;
//! - `pattern` constrains the lexical form;
//! - `$value` of an assertion is "the XDM representation of
//!   E.[schema actual value]" (Structures §3.13.4.1 clause 2.3.1): an
//!   `xs:QName` value;
//! - a `fixed` value constraint compares "the ·actual value· of E" with
//!   "D.{value constraint}.{value}" (Structures §3.3.4.3 clause 5.2.2.2.2);
//! - the schema-time checks that compare enumerations or fixed values do the
//!   same.
//!
//! Instances run on streaming validation with and without PSVI typed values
//! and on the navigator driver, in XSD 1.0 and XSD 1.1 mode; typed values are
//! checked through the push API, the typed document build and the navigator.

#[cfg(feature = "xsd11")]
use bumpalo::Bump;
#[cfg(feature = "xsd11")]
use xsd_schema::document::{build_typed_document, BufferDocumentOptions};
use xsd_schema::namespace::context::NamespaceContextSnapshot;
#[cfg(feature = "xsd11")]
use xsd_schema::navigator::{DomNavigator, TypedValue};
use xsd_schema::types::value::{XmlAtomicValue, XmlValue, XmlValueKind};
use xsd_schema::validation::{
    drive_navigator, drive_quick_xml, CollectingValidationSink, SchemaValidator, SchemaValidity,
    ValidationFlags,
};
use xsd_schema::{RoXmlNavigator, SchemaResult, SchemaSet, SchemaSetBuilder, XsdVersion};

use SchemaValidity::{Invalid, Valid};

const VERSIONS: [XsdVersion; 2] = [XsdVersion::V1_0, XsdVersion::V1_1];
const DRIVERS: [&str; 3] = ["stream", "stream/no-psvi", "navigator"];

// ── Harness ───────────────────────────────────────────────────────────────

/// A schema document for target namespace `ns`, bound to the prefix `prefix`,
/// with `body`.
fn compile_in(version: XsdVersion, prefix: &str, ns: &str, body: &str) -> SchemaResult<SchemaSet> {
    let xsd = format!(
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:{prefix}="{ns}" targetNamespace="{ns}" elementFormDefault="qualified">{body}</xs:schema>"#
    );
    SchemaSetBuilder::with_version(version)
        .add_source(&xsd, "file:///qname_value_space.xsd")?
        .compile()
        .map(|compiled| compiled.into_schema_set())
}

fn schema_in(version: XsdVersion, prefix: &str, ns: &str, body: &str) -> SchemaSet {
    compile_in(version, prefix, ns, body)
        .unwrap_or_else(|e| panic!("{version:?}: schema compiles: {e}"))
}

/// Root validity and error constraint codes of one run on `driver`.
fn run(ss: &SchemaSet, xml: &str, driver: &str) -> (Option<SchemaValidity>, Vec<String>) {
    let flags = match driver {
        "stream/no-psvi" => ValidationFlags::default() & !ValidationFlags::BUILD_PSVI_TYPED_VALUES,
        _ => ValidationFlags::default(),
    };
    #[cfg(feature = "xsd11")]
    let flags = flags | ValidationFlags::PROCESS_ASSERTIONS;
    #[cfg(feature = "xsd11")]
    let v = SchemaValidator::new_fragment_buffer(ss, flags);
    #[cfg(not(feature = "xsd11"))]
    let v = SchemaValidator::new(ss, flags);
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let validity = {
        let mut rt = v.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        if driver == "navigator" {
            let doc = roxmltree::Document::parse(xml).expect("well-formed instance");
            let nav = RoXmlNavigator::new(&doc);
            drive_navigator(&nav, &mut rt, ss)
                .expect("navigator drive completes")
                .root_validity
        } else {
            drive_quick_xml(xml.as_bytes(), &mut rt, ss)
                .expect("stream drive completes")
                .root_validity
        }
    };
    (
        validity,
        errors.iter().map(|e| e.constraint.to_string()).collect(),
    )
}

/// Every `(instance, validity, error code)` row on every driver; all
/// mismatches are reported at once. With a code, some error starts with it;
/// without, there is no error at all.
fn check_rows(ss: &SchemaSet, label: &str, rows: &[(&str, SchemaValidity, Option<&str>)]) {
    let mut wrong = Vec::new();
    for (xml, expected, code) in rows {
        for driver in DRIVERS {
            let (validity, errors) = run(ss, xml, driver);
            let ok = validity == Some(*expected)
                && match code {
                    Some(code) => errors.iter().any(|e| e.starts_with(code)),
                    None => errors.is_empty(),
                };
            if !ok {
                wrong.push(format!(
                    "{label} {driver}: {xml} => {validity:?} {errors:?}"
                ));
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

/// The namespace URI and local name of a typed value that is one `xs:QName`
/// (or `xs:NOTATION`) value — itself or as a union's member value.
fn expanded_name(ss: &SchemaSet, value: &XmlValue) -> Option<(String, String)> {
    match &value.value {
        XmlValueKind::Atomic(XmlAtomicValue::QName(qn) | XmlAtomicValue::Notation(qn)) => {
            let name = qn.name();
            Some((
                name.namespace_uri
                    .map(|ns| ss.name_table.resolve(ns).to_string())
                    .unwrap_or_default(),
                qn.local_name().to_string(),
            ))
        }
        XmlValueKind::Union(inner) => expanded_name(ss, inner),
        _ => None,
    }
}

/// The typed value of the document element of `xml`, built as a typed
/// document, as an expanded name (`None` when it is not one QName value).
#[cfg(feature = "xsd11")]
fn document_element_name(ss: &SchemaSet, xml: &str) -> Option<(String, String)> {
    let arena = Bump::new();
    let doc = build_typed_document(xml.as_bytes(), &arena, ss, BufferDocumentOptions::default())
        .expect("typed build");
    let mut nav = doc.create_navigator();
    assert!(nav.move_to_first_child());
    match nav.typed_value() {
        TypedValue::Value(v) => expanded_name(ss, &v),
        _ => None,
    }
}

fn pair(ns: &str, local: &str) -> Option<(String, String)> {
    Some((ns.to_string(), local.to_string()))
}

// ── enumeration: QName ────────────────────────────────────────────────────

/// The shape of a SOAP 1.2 fault code: a restriction of `xs:QName` that
/// enumerates names written with the schema's own prefix.
const CODE_SCHEMA: &str = r#"
  <xs:simpleType name="code"><xs:restriction base="xs:QName">
    <xs:enumeration value="tns:Sender"/>
    <xs:enumeration value="tns:Receiver"/>
  </xs:restriction></xs:simpleType>
  <xs:simpleType name="codeP"><xs:restriction base="tns:code">
    <xs:pattern value="tns:.*"/>
  </xs:restriction></xs:simpleType>
  <xs:element name="Value" type="tns:code"/>
  <xs:element name="PV" type="tns:codeP"/>
  <xs:element name="w"><xs:complexType>
    <xs:attribute name="a" type="tns:code"/>
  </xs:complexType></xs:element>
  <xs:element name="sc"><xs:complexType><xs:simpleContent>
    <xs:extension base="tns:code"><xs:attribute name="x" type="xs:string"/></xs:extension>
  </xs:simpleContent></xs:complexType></xs:element>"#;

#[test]
fn qname_enumeration_compares_expanded_names() {
    const ENUM: Option<&str> = Some("cvc-enumeration-valid");
    for version in VERSIONS {
        let ss = schema_in(version, "tns", "urn:e", CODE_SCHEMA);
        check_rows(
            &ss,
            &format!("{version:?}"),
            &[
                (
                    r#"<tns:Value xmlns:tns="urn:e">tns:Sender</tns:Value>"#,
                    Valid,
                    None,
                ),
                // Another prefix bound to the same namespace.
                (
                    r#"<e:Value xmlns:e="urn:e">e:Sender</e:Value>"#,
                    Valid,
                    None,
                ),
                (
                    r#"<e:Value xmlns:e="urn:e"> e:Receiver </e:Value>"#,
                    Valid,
                    None,
                ),
                // An unprefixed name is in the default namespace.
                (r#"<Value xmlns="urn:e">Sender</Value>"#, Valid, None),
                // The schema's prefix bound to another namespace.
                (
                    r#"<e:Value xmlns:e="urn:e" xmlns:tns="urn:other">tns:Sender</e:Value>"#,
                    Invalid,
                    ENUM,
                ),
                (
                    r#"<e:Value xmlns:e="urn:e">e:Other</e:Value>"#,
                    Invalid,
                    ENUM,
                ),
                // No default namespace: {}Sender.
                (
                    r#"<e:Value xmlns:e="urn:e">Sender</e:Value>"#,
                    Invalid,
                    ENUM,
                ),
                // Attributes.
                (r#"<e:w xmlns:e="urn:e" a="e:Receiver"/>"#, Valid, None),
                (
                    r#"<e:w xmlns:e="urn:e" xmlns:tns="urn:x" a="tns:Receiver"/>"#,
                    Invalid,
                    ENUM,
                ),
                // A complex type with simple content.
                (
                    r#"<e:sc xmlns:e="urn:e" x="1">e:Sender</e:sc>"#,
                    Valid,
                    None,
                ),
                (
                    r#"<e:sc xmlns:e="urn:e" xmlns:tns="urn:x">tns:Sender</e:sc>"#,
                    Invalid,
                    ENUM,
                ),
                // `pattern` constrains the lexical form, the inherited
                // enumeration the value.
                (
                    r#"<tns:PV xmlns:tns="urn:e">tns:Sender</tns:PV>"#,
                    Valid,
                    None,
                ),
                (
                    r#"<e:PV xmlns:e="urn:e">e:Sender</e:PV>"#,
                    Invalid,
                    Some("cvc-pattern-valid"),
                ),
            ],
        );
    }
}

/// The typed value the navigator gives (and `data()` atomizes to) agrees with
/// the drivers: a value they accept is an `xs:QName` value, also when its
/// prefix is not the schema's.
#[cfg(feature = "xsd11")]
#[test]
fn accepted_enumeration_values_have_qname_typed_values() {
    for version in VERSIONS {
        let ss = schema_in(version, "tns", "urn:e", CODE_SCHEMA);
        assert_eq!(
            document_element_name(&ss, r#"<e:Value xmlns:e="urn:e">e:Sender</e:Value>"#),
            pair("urn:e", "Sender"),
            "{version:?}"
        );
        assert_eq!(
            document_element_name(&ss, r#"<e:sc xmlns:e="urn:e">e:Receiver</e:sc>"#),
            pair("urn:e", "Receiver"),
            "{version:?}"
        );
    }
}

// ── enumeration: NOTATION ─────────────────────────────────────────────────

#[test]
fn notation_enumeration_compares_expanded_names() {
    const ENUM: Option<&str> = Some("cvc-enumeration-valid");
    let body = r#"
      <xs:notation name="gif" public="image/gif"/>
      <xs:notation name="png" public="image/png"/>
      <xs:simpleType name="pic"><xs:restriction base="xs:NOTATION">
        <xs:enumeration value="n:gif"/>
        <xs:enumeration xmlns:other="urn:n" value="other:png"/>
      </xs:restriction></xs:simpleType>
      <xs:element name="a"><xs:complexType>
        <xs:attribute name="v" type="n:pic"/>
      </xs:complexType></xs:element>"#;
    for version in VERSIONS {
        let ss = schema_in(version, "n", "urn:n", body);
        check_rows(
            &ss,
            &format!("{version:?}"),
            &[
                (r#"<n:a xmlns:n="urn:n" v="n:gif"/>"#, Valid, None),
                (r#"<n:a xmlns:n="urn:n" v="n:png"/>"#, Valid, None),
                (r#"<p:a xmlns:p="urn:n" v="p:gif"/>"#, Valid, None),
                (
                    r#"<n:a xmlns:n="urn:n" xmlns:z="urn:n" v="z:png"/>"#,
                    Valid,
                    None,
                ),
                (
                    r#"<n:a xmlns:n="urn:n" xmlns:p="urn:x" v="p:gif"/>"#,
                    Invalid,
                    ENUM,
                ),
                (
                    r#"<n:a xmlns:n="urn:n" xmlns:other="urn:x" v="other:png"/>"#,
                    Invalid,
                    ENUM,
                ),
            ],
        );
    }
}

// ── lists and unions ──────────────────────────────────────────────────────

/// A list of `xs:QName` compares each member by expanded name with the member
/// of the enumerated list (Datatypes §2.4.1.2); a union with an `xs:QName`
/// member compares the QName it chose with the enumerated QName.
#[test]
fn lists_and_unions_of_qnames_compare_expanded_names() {
    const ENUM: Option<&str> = Some("cvc-enumeration-valid");
    let body = r#"
      <xs:simpleType name="lq"><xs:restriction>
        <xs:simpleType><xs:list itemType="xs:QName"/></xs:simpleType>
        <xs:enumeration value="t:a t:b"/>
      </xs:restriction></xs:simpleType>
      <xs:simpleType name="uq"><xs:restriction>
        <xs:simpleType><xs:union memberTypes="xs:integer xs:QName"/></xs:simpleType>
        <xs:enumeration value="t:a"/>
        <xs:enumeration value="5"/>
      </xs:restriction></xs:simpleType>
      <xs:element name="r"><xs:complexType><xs:sequence>
        <xs:element name="le" type="t:lq" minOccurs="0"/>
        <xs:element name="ue" type="t:uq" minOccurs="0"/>
      </xs:sequence>
      <xs:attribute name="l" type="t:lq"/>
      <xs:attribute name="u" type="t:uq"/>
      </xs:complexType></xs:element>"#;
    for version in VERSIONS {
        let ss = schema_in(version, "t", "urn:t", body);
        check_rows(
            &ss,
            &format!("{version:?}"),
            &[
                (r#"<t:r xmlns:t="urn:t" l="t:a t:b"/>"#, Valid, None),
                (
                    r#"<t:r xmlns:t="urn:t" xmlns:p="urn:t" l="p:a t:b"/>"#,
                    Valid,
                    None,
                ),
                (
                    r#"<t:r xmlns:t="urn:t" xmlns:p="urn:x" l="p:a t:b"/>"#,
                    Invalid,
                    ENUM,
                ),
                (r#"<t:r xmlns:t="urn:t" l="t:b t:a"/>"#, Invalid, ENUM),
                (
                    r#"<p:r xmlns:p="urn:t"><p:le>p:a p:b</p:le></p:r>"#,
                    Valid,
                    None,
                ),
                (
                    r#"<p:r xmlns:p="urn:t" xmlns:t="urn:x"><p:le>t:a t:b</p:le></p:r>"#,
                    Invalid,
                    ENUM,
                ),
                (
                    r#"<t:r xmlns:t="urn:t" xmlns:p="urn:t" u="p:a"/>"#,
                    Valid,
                    None,
                ),
                (r#"<t:r xmlns:t="urn:t" u="5"/>"#, Valid, None),
                (r#"<t:r xmlns:t="urn:t" u="05"/>"#, Valid, None),
                (
                    r#"<r xmlns="urn:t" xmlns:t="urn:x" u="t:a"/>"#,
                    Invalid,
                    ENUM,
                ),
                (
                    r#"<p:r xmlns:p="urn:t"><p:ue>p:a</p:ue></p:r>"#,
                    Valid,
                    None,
                ),
                (
                    r#"<p:r xmlns:p="urn:t" xmlns:t="urn:x"><p:ue>t:a</p:ue></p:r>"#,
                    Invalid,
                    ENUM,
                ),
            ],
        );
    }
}

// ── assertions (XSD 1.1) ──────────────────────────────────────────────────

/// `$value` is an `xs:QName` value: `namespace-uri-from-QName($value)` works
/// on an element, an attribute, the members of a list, a union's QName member
/// and a complex type's simple content (Structures §3.13.4.1 clause 2.3.1).
#[cfg(feature = "xsd11")]
#[test]
fn assertions_see_qname_values() {
    const ASSERTION: Option<&str> = Some("cvc-assertion");
    let body = r#"
      <xs:simpleType name="inX"><xs:restriction base="xs:QName">
        <xs:assertion test="namespace-uri-from-QName($value) eq 'urn:x'"/>
      </xs:restriction></xs:simpleType>
      <xs:simpleType name="listInX"><xs:restriction>
        <xs:simpleType><xs:list itemType="xs:QName"/></xs:simpleType>
        <xs:assertion test="every $q in $value satisfies namespace-uri-from-QName($q) eq 'urn:x'"/>
      </xs:restriction></xs:simpleType>
      <xs:simpleType name="intOrInX"><xs:restriction>
        <xs:simpleType><xs:union memberTypes="xs:integer xs:QName"/></xs:simpleType>
        <xs:assertion test="$value instance of xs:integer or namespace-uri-from-QName($value) eq 'urn:x'"/>
      </xs:restriction></xs:simpleType>
      <xs:element name="e" type="t:inX"/>
      <xs:element name="l" type="t:listInX"/>
      <xs:element name="u" type="t:intOrInX"/>
      <xs:element name="h"><xs:complexType>
        <xs:attribute name="a" type="t:inX"/>
      </xs:complexType></xs:element>
      <xs:element name="c"><xs:complexType><xs:simpleContent>
        <xs:extension base="xs:QName">
          <xs:assert test="namespace-uri-from-QName($value) eq 'urn:x'"/>
        </xs:extension>
      </xs:simpleContent></xs:complexType></xs:element>"#;
    let ss = schema_in(XsdVersion::V1_1, "t", "urn:t", body);
    check_rows(
        &ss,
        "V1_1",
        &[
            (
                r#"<t:e xmlns:t="urn:t" xmlns:p="urn:x">p:l</t:e>"#,
                Valid,
                None,
            ),
            (
                r#"<t:e xmlns:t="urn:t" xmlns:p="urn:y">p:l</t:e>"#,
                Invalid,
                ASSERTION,
            ),
            (r#"<t:e xmlns:t="urn:t" xmlns="urn:x">l</t:e>"#, Valid, None),
            (
                r#"<t:h xmlns:t="urn:t" xmlns:p="urn:x" a="p:l"/>"#,
                Valid,
                None,
            ),
            (
                r#"<t:h xmlns:t="urn:t" xmlns:p="urn:y" a="p:l"/>"#,
                Invalid,
                ASSERTION,
            ),
            (
                r#"<t:l xmlns:t="urn:t" xmlns:p="urn:x" xmlns:q="urn:x">p:a q:b</t:l>"#,
                Valid,
                None,
            ),
            (
                r#"<t:l xmlns:t="urn:t" xmlns:p="urn:x" xmlns:q="urn:y">p:a q:b</t:l>"#,
                Invalid,
                ASSERTION,
            ),
            (
                r#"<t:u xmlns:t="urn:t" xmlns:p="urn:x">p:a</t:u>"#,
                Valid,
                None,
            ),
            (r#"<t:u xmlns:t="urn:t">5</t:u>"#, Valid, None),
            (
                r#"<t:u xmlns:t="urn:t" xmlns:p="urn:y">p:a</t:u>"#,
                Invalid,
                ASSERTION,
            ),
            (
                r#"<t:c xmlns:t="urn:t" xmlns:p="urn:x">p:a</t:c>"#,
                Valid,
                None,
            ),
            (
                r#"<t:c xmlns:t="urn:t" xmlns:p="urn:y">p:a</t:c>"#,
                Invalid,
                ASSERTION,
            ),
        ],
    );
    // The navigator's typed value agrees.
    assert_eq!(
        document_element_name(&ss, r#"<t:e xmlns:t="urn:t" xmlns:p="urn:x">p:l</t:e>"#),
        pair("urn:x", "l")
    );
}

// ── the PSVI typed value of a QName element ───────────────────────────────

/// The runtime's [schema actual value] of an element of type `xs:QName` is a
/// QName value — its typed value in the end-of-element `SchemaInfo` — as an
/// attribute's already is.
#[test]
fn the_typed_value_of_a_qname_element_is_a_qname() {
    let body = r#"
      <xs:element name="e" type="xs:QName"/>
      <xs:element name="h"><xs:complexType>
        <xs:attribute name="a" type="xs:QName"/>
      </xs:complexType></xs:element>"#;
    for version in VERSIONS {
        let ss = schema_in(version, "t", "urn:t", body);
        let mut ns = NamespaceContextSnapshot::default();
        ns.bindings
            .push((ss.name_table.add("t"), ss.name_table.add("urn:t")));
        ns.bindings
            .push((ss.name_table.add("p"), ss.name_table.add("urn:x")));
        let v = SchemaValidator::new(&ss, ValidationFlags::default());
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut rt = v.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        rt.validate_element("e", "urn:t", None, None, &ns);
        rt.validate_end_of_attributes();
        rt.validate_text("p:l");
        let info = rt.validate_end_element();
        let value = info.typed_value.expect("a typed value");
        assert_eq!(
            expanded_name(&ss, &value),
            pair("urn:x", "l"),
            "{version:?}"
        );

        rt.validate_element("h", "urn:t", None, None, &ns);
        let attr = rt.validate_attribute("a", "", "p:m");
        let value = attr.typed_value.expect("a typed value");
        assert_eq!(
            expanded_name(&ss, &value),
            pair("urn:x", "m"),
            "{version:?}"
        );
        rt.validate_end_of_attributes();
        rt.validate_end_element();
        drop(rt);
        assert!(errors.is_empty(), "{version:?}: {errors:?}");
    }
}

// ── fixed values ──────────────────────────────────────────────────────────

/// Structures §3.3.4.3 clause 5.2.2.2.2: "the ·actual value· of E is equal or
/// identical to D.{value constraint}.{value}" — the fixed literal resolved
/// with the schema's bindings, the instance's with its own. The same for
/// declared attributes and for attributes a strict wildcard admits
/// (Attribute Locally Valid / Attribute Locally Valid (Use)).
#[test]
fn fixed_qname_values_compare_expanded_names() {
    const ELT: Option<&str> = Some("cvc-elt.5.2.2");
    const ATTR: Option<&str> = Some("cvc-a");
    let body = r#"
      <xs:element name="f" type="xs:QName" fixed="t:a"/>
      <xs:element name="g"><xs:complexType>
        <xs:attribute name="a" type="xs:QName" fixed="t:a"/>
      </xs:complexType></xs:element>
      <xs:attribute name="ga" type="xs:QName" fixed="t:a"/>
      <xs:element name="w"><xs:complexType>
        <xs:anyAttribute namespace="urn:t" processContents="strict"/>
      </xs:complexType></xs:element>"#;
    for version in VERSIONS {
        let ss = schema_in(version, "t", "urn:t", body);
        check_rows(
            &ss,
            &format!("{version:?}"),
            &[
                (r#"<t:f xmlns:t="urn:t">t:a</t:f>"#, Valid, None),
                (r#"<p:f xmlns:p="urn:t">p:a</p:f>"#, Valid, None),
                (
                    r#"<p:f xmlns:p="urn:t" xmlns:t="urn:x">t:a</p:f>"#,
                    Invalid,
                    ELT,
                ),
                (r#"<t:f xmlns:t="urn:t">t:b</t:f>"#, Invalid, ELT),
                (r#"<t:f xmlns:t="urn:t"/>"#, Valid, None),
                (r#"<p:g xmlns:p="urn:t" a="p:a"/>"#, Valid, None),
                (
                    r#"<p:g xmlns:p="urn:t" xmlns:t="urn:x" a="t:a"/>"#,
                    Invalid,
                    ATTR,
                ),
                (r#"<p:w xmlns:p="urn:t" p:ga="p:a"/>"#, Valid, None),
                (
                    r#"<p:w xmlns:p="urn:t" xmlns:t="urn:x" p:ga="t:a"/>"#,
                    Invalid,
                    ATTR,
                ),
            ],
        );
    }
}

// ── schema-time checks ────────────────────────────────────────────────────

fn schema_error(version: XsdVersion, body: &str) -> String {
    match compile_in(version, "t", "urn:t", body) {
        Ok(_) => panic!("{version:?}: expected a schema error"),
        Err(e) => e.to_string(),
    }
}

/// A restriction's enumeration must be a subset of its base's (Datatypes
/// §4.3.5.5 enumeration valid restriction, and the value-space subset
/// cos-st-restricts asks for), in value space.
#[test]
fn a_restriction_enumerates_base_values_by_expanded_name() {
    let base = r#"
      <xs:simpleType name="code"><xs:restriction base="xs:QName">
        <xs:enumeration value="t:a"/><xs:enumeration value="t:b"/>
      </xs:restriction></xs:simpleType>"#;
    for version in VERSIONS {
        let ss = schema_in(
            version,
            "t",
            "urn:t",
            &format!(
                r#"{base}
                <xs:simpleType name="sub"><xs:restriction base="t:code">
                  <xs:enumeration xmlns:p="urn:t" value="p:a"/>
                </xs:restriction></xs:simpleType>
                <xs:element name="s" type="t:sub"/>"#
            ),
        );
        check_rows(
            &ss,
            &format!("{version:?}"),
            &[
                (r#"<t:s xmlns:t="urn:t">t:a</t:s>"#, Valid, None),
                (
                    r#"<t:s xmlns:t="urn:t">t:b</t:s>"#,
                    Invalid,
                    Some("cvc-enumeration-valid"),
                ),
            ],
        );
        for derived in [
            r#"<xs:enumeration xmlns:p="urn:t" value="p:c"/>"#,
            r#"<xs:enumeration xmlns:t="urn:x" value="t:a"/>"#,
        ] {
            let err = schema_error(
                version,
                &format!(
                    r#"{base}<xs:simpleType name="sub"><xs:restriction base="t:code">{derived}</xs:restriction></xs:simpleType>"#
                ),
            );
            assert!(
                err.contains("enumeration") || err.contains("cos-st-restricts"),
                "{version:?} {derived}: {err}"
            );
        }
    }
}

/// QName `fixed` values compared at schema time: an attribute use's against
/// its declaration's (Attribute Use Correct clause 3, "identical"), a
/// restriction's attribute use against the base's and a restriction's local
/// element against the base's (derivation-ok-restriction, particle
/// restriction) — each literal resolved with the bindings in scope where it is
/// written.
#[test]
fn schema_time_fixed_qname_values_compare_expanded_names() {
    for version in VERSIONS {
        let same = r#"xmlns:p="urn:t" fixed="p:a""#;
        let other = r#"xmlns:q="urn:x" fixed="q:a""#;
        let use_of = |fixed: &str| {
            format!(
                r#"<xs:attribute name="ga" type="xs:QName" fixed="t:a"/>
                   <xs:complexType name="u"><xs:attribute ref="t:ga" {fixed}/></xs:complexType>"#
            )
        };
        let restricted_attribute = |fixed: &str| {
            format!(
                r#"<xs:complexType name="b"><xs:attribute name="a" type="xs:QName" fixed="t:a"/></xs:complexType>
                   <xs:complexType name="d"><xs:complexContent><xs:restriction base="t:b">
                     <xs:attribute name="a" type="xs:QName" {fixed}/>
                   </xs:restriction></xs:complexContent></xs:complexType>"#
            )
        };
        let restricted_element = |fixed: &str| {
            format!(
                r#"<xs:complexType name="b"><xs:sequence>
                     <xs:element name="c" type="xs:QName" fixed="t:a"/>
                   </xs:sequence></xs:complexType>
                   <xs:complexType name="d"><xs:complexContent><xs:restriction base="t:b">
                     <xs:sequence><xs:element name="c" type="xs:QName" {fixed}/></xs:sequence>
                   </xs:restriction></xs:complexContent></xs:complexType>"#
            )
        };
        for (what, make) in [
            ("attribute use", &use_of as &dyn Fn(&str) -> String),
            ("restricted attribute", &restricted_attribute),
            ("restricted element", &restricted_element),
        ] {
            if let Err(e) = compile_in(version, "t", "urn:t", &make(same)) {
                panic!("{version:?} {what}: the same name must compile: {e}");
            }
            let err = schema_error(version, &make(other));
            assert!(
                err.contains("fixed")
                    || err.contains("au-props-correct")
                    || err.contains("derivation-ok-restriction"),
                "{version:?} {what}: {err}"
            );
        }
    }
}

// ── fix round: simple content, defaults, large enumerations ───────────────

/// A complex type's simple-content restriction may enumerate QName values
/// with its own prefixes (its facets are those of a simple type, Structures
/// §3.4.2.2): `p:a` with `p` bound to the base's namespace is the base's
/// `t:a`. Before this change such an enumeration rejected every instance.
#[test]
fn simple_content_restrictions_enumerate_qnames_by_expanded_name() {
    const ENUM: Option<&str> = Some("cvc-enumeration-valid");
    let body = r#"
      <xs:simpleType name="code"><xs:restriction base="xs:QName">
        <xs:enumeration value="t:a"/><xs:enumeration value="t:b"/>
      </xs:restriction></xs:simpleType>
      <xs:complexType name="sc"><xs:simpleContent>
        <xs:extension base="t:code"><xs:attribute name="x"/></xs:extension>
      </xs:simpleContent></xs:complexType>
      <xs:complexType name="sd"><xs:simpleContent><xs:restriction base="t:sc">
        <xs:enumeration xmlns:p="urn:t" value="p:a"/>
      </xs:restriction></xs:simpleContent></xs:complexType>
      <xs:element name="sd" type="t:sd"/>"#;
    for version in VERSIONS {
        let ss = schema_in(version, "t", "urn:t", body);
        check_rows(
            &ss,
            &format!("{version:?}"),
            &[
                (r#"<t:sd xmlns:t="urn:t">t:a</t:sd>"#, Valid, None),
                (r#"<q:sd xmlns:q="urn:t">q:a</q:sd>"#, Valid, None),
                (r#"<t:sd xmlns:t="urn:t">t:b</t:sd>"#, Invalid, ENUM),
                (
                    r#"<q:sd xmlns:q="urn:t" xmlns:p="urn:x">p:a</q:sd>"#,
                    Invalid,
                    ENUM,
                ),
            ],
        );
    }
}

/// An empty element that takes its value constraint (Structures §3.3.4.3
/// clause 5.1) takes the constraint's {value}: "the ·actual value· … of the
/// [attribute]" on the declaration (§3.3.2), a QName read with the schema
/// element's bindings (Datatypes §3.3.18). The typed build writes it again
/// with a prefix the instance binds (·namespace fixup·, which §3.3.5.4's Note
/// leaves to the processor), so the element is validated as that value
/// (clause 5.1.2) — batch (t) fix round 1, which reverses C33's decision D3
/// (the element's own bindings read the literal). So a default `t:Sender`
/// under an enumeration of `{urn:e}Sender` is valid also on an element that
/// binds `t` elsewhere, and a fixed value taken by an empty element is not
/// compared with the declared one (clause 5.2.2 applies only when 5.1 does
/// not).
#[test]
fn defaults_of_empty_elements_are_read_with_the_schemas_bindings() {
    let body = r#"
      <xs:simpleType name="code"><xs:restriction base="xs:QName">
        <xs:enumeration value="t:Sender"/><xs:enumeration value="t:Receiver"/>
      </xs:restriction></xs:simpleType>
      <xs:element name="r"><xs:complexType><xs:sequence>
        <xs:element name="d" type="t:code" default="t:Sender" minOccurs="0"/>
        <xs:element name="f" type="xs:QName" fixed="t:Sender" minOccurs="0"/>
      </xs:sequence></xs:complexType></xs:element>"#;
    for version in VERSIONS {
        let ss = schema_in(version, "t", "urn:e", body);
        check_rows(
            &ss,
            &format!("{version:?}"),
            &[
                (r#"<t:r xmlns:t="urn:e"><t:d/><t:f/></t:r>"#, Valid, None),
                (
                    r#"<p:r xmlns:p="urn:e" xmlns:t="urn:x"><p:d/></p:r>"#,
                    Valid,
                    None,
                ),
                // The instance binds no `t`: the value is still `{urn:e}Sender`.
                (r#"<p:r xmlns:p="urn:e"><p:d/><p:f/></p:r>"#, Valid, None),
                (
                    r#"<p:r xmlns:p="urn:e" xmlns:t="urn:x"><p:f/></p:r>"#,
                    Valid,
                    None,
                ),
                // Supplied, the fixed value is compared by expanded name.
                (
                    r#"<p:r xmlns:p="urn:e" xmlns:t="urn:x"><p:f>p:Sender</p:f></p:r>"#,
                    Valid,
                    None,
                ),
                (
                    r#"<p:r xmlns:p="urn:e" xmlns:t="urn:x"><p:f>t:Sender</p:f></p:r>"#,
                    Invalid,
                    Some("cvc-elt.5.2.2"),
                ),
            ],
        );
    }
}

/// A large enumeration matches by expanded name and by lexical form exactly
/// as a small one: every enumerated name under any prefix bound to its
/// namespace, nothing else — also after the schema set is used again.
#[test]
fn large_qname_enumerations_match_every_value() {
    let literals: String = (0..300)
        .map(|i| {
            if i % 2 == 0 {
                format!(r#"<xs:enumeration value="t:v{i}"/>"#)
            } else {
                format!(r#"<xs:enumeration xmlns:o="urn:t" value="o:v{i}"/>"#)
            }
        })
        .collect();
    let body = format!(
        r#"<xs:simpleType name="code"><xs:restriction base="xs:QName">{literals}</xs:restriction></xs:simpleType>
           <xs:simpleType name="sub"><xs:restriction base="t:code"><xs:pattern value="[a-z]+:v1.*"/></xs:restriction></xs:simpleType>
           <xs:element name="c" type="t:code"/>
           <xs:element name="s" type="t:sub"/>"#
    );
    for version in VERSIONS {
        let ss = schema_in(version, "t", "urn:t", &body);
        let mut rows: Vec<(String, SchemaValidity, Option<&str>)> = Vec::new();
        for i in [0, 1, 2, 150, 199, 298, 299] {
            rows.push((format!(r#"<p:c xmlns:p="urn:t">p:v{i}</p:c>"#), Valid, None));
            rows.push((format!(r#"<t:c xmlns:t="urn:t">t:v{i}</t:c>"#), Valid, None));
            rows.push((
                format!(r#"<p:c xmlns:p="urn:t" xmlns:t="urn:x">t:v{i}</p:c>"#),
                Invalid,
                Some("cvc-enumeration-valid"),
            ));
        }
        rows.push((
            r#"<t:c xmlns:t="urn:t">t:v300</t:c>"#.to_string(),
            Invalid,
            Some("cvc-enumeration-valid"),
        ));
        rows.push((
            r#"<p:s xmlns:p="urn:t">p:v150</p:s>"#.to_string(),
            Valid,
            None,
        ));
        rows.push((
            r#"<p:s xmlns:p="urn:t">p:v20</p:s>"#.to_string(),
            Invalid,
            Some("cvc-pattern-valid"),
        ));
        let rows: Vec<(&str, SchemaValidity, Option<&str>)> =
            rows.iter().map(|(x, v, c)| (x.as_str(), *v, *c)).collect();
        // Twice: the second pass reads whatever the first one cached.
        check_rows(&ss, &format!("{version:?}"), &rows);
        check_rows(&ss, &format!("{version:?} again"), &rows);
    }
}

// ── fix round 2: union enumeration literals take the union's value ────────

/// Datatypes §4.3.5.2: an enumeration's values are its literals "interpreted
/// as instances of the {base type definition}"; for a union base, §2.4.1.3:
/// "an element or attribute's value is validated against the ·member types·
/// in the order in which they appear in the definition until a match is
/// found" — the first member that accepts a literal decides its value. And
/// "no two values from different ·primitive· ·value spaces· are equal"
/// (§2.2.3, the note on enumeration checks). So with `p:.*` strings before
/// `xs:QName`, the literal `p:a` is the *string* `p:a`: the QName `q:a` is
/// not enumerated even when `q` and `p` name one namespace; with `xs:QName`
/// first it is the QName `{urn:n}a`. The same for NOTATION and list members,
/// and for a literal only one member accepts.
#[test]
fn union_enumeration_literals_take_the_value_their_union_gives() {
    const ENUM: Option<&str> = Some("cvc-enumeration-valid");
    let body = r#"
      <xs:notation name="gif" public="image/gif"/>
      <xs:notation name="png" public="image/png"/>
      <xs:simpleType name="S"><xs:restriction base="xs:string">
        <xs:pattern value="p:.*"/>
      </xs:restriction></xs:simpleType>
      <xs:simpleType name="N"><xs:restriction base="xs:NOTATION">
        <xs:enumeration value="n:gif"/><xs:enumeration value="n:png"/>
      </xs:restriction></xs:simpleType>
      <xs:simpleType name="LQ"><xs:list itemType="xs:QName"/></xs:simpleType>
      <xs:simpleType name="StrFirst"><xs:restriction>
        <xs:simpleType><xs:union memberTypes="n:S xs:QName"/></xs:simpleType>
        <xs:enumeration xmlns:p="urn:n" value="p:a"/>
      </xs:restriction></xs:simpleType>
      <xs:simpleType name="QNameFirst"><xs:restriction>
        <xs:simpleType><xs:union memberTypes="xs:QName n:S"/></xs:simpleType>
        <xs:enumeration xmlns:p="urn:n" value="p:a"/>
      </xs:restriction></xs:simpleType>
      <xs:simpleType name="StrNotation"><xs:restriction>
        <xs:simpleType><xs:union memberTypes="n:S n:N"/></xs:simpleType>
        <xs:enumeration xmlns:p="urn:n" value="p:gif"/>
      </xs:restriction></xs:simpleType>
      <xs:simpleType name="NotationStr"><xs:restriction>
        <xs:simpleType><xs:union memberTypes="n:N n:S"/></xs:simpleType>
        <xs:enumeration xmlns:p="urn:n" value="p:gif"/>
      </xs:restriction></xs:simpleType>
      <xs:simpleType name="StrList"><xs:restriction>
        <xs:simpleType><xs:union memberTypes="n:S n:LQ"/></xs:simpleType>
        <xs:enumeration xmlns:p="urn:n" value="p:a p:b"/>
      </xs:restriction></xs:simpleType>
      <xs:simpleType name="ListStr"><xs:restriction>
        <xs:simpleType><xs:union memberTypes="n:LQ n:S"/></xs:simpleType>
        <xs:enumeration xmlns:p="urn:n" value="p:a p:b"/>
      </xs:restriction></xs:simpleType>
      <xs:simpleType name="IntQName"><xs:restriction>
        <xs:simpleType><xs:union memberTypes="xs:integer xs:QName"/></xs:simpleType>
        <xs:enumeration value="5"/>
        <xs:enumeration xmlns:p="urn:n" value="p:a"/>
      </xs:restriction></xs:simpleType>
      <xs:element name="r"><xs:complexType>
        <xs:attribute name="sf" type="n:StrFirst"/>
        <xs:attribute name="qf" type="n:QNameFirst"/>
        <xs:attribute name="sn" type="n:StrNotation"/>
        <xs:attribute name="ns" type="n:NotationStr"/>
        <xs:attribute name="sl" type="n:StrList"/>
        <xs:attribute name="ls" type="n:ListStr"/>
        <xs:attribute name="iq" type="n:IntQName"/>
      </xs:complexType></xs:element>
      <xs:element name="e" type="n:StrFirst"/>"#;
    for version in VERSIONS {
        let ss = schema_in(version, "n", "urn:n", body);
        check_rows(
            &ss,
            &format!("{version:?}"),
            &[
                // The reproducer: the enumerated value is the string `p:a`.
                (
                    r#"<n:r xmlns:n="urn:n" xmlns:q="urn:n" sf="q:a"/>"#,
                    Invalid,
                    ENUM,
                ),
                (
                    r#"<n:e xmlns:n="urn:n" xmlns:q="urn:n">q:a</n:e>"#,
                    Invalid,
                    ENUM,
                ),
                (r#"<n:r xmlns:n="urn:n" sf="p:a"/>"#, Valid, None),
                (
                    r#"<n:r xmlns:n="urn:n" xmlns:p="urn:x" sf="p:a"/>"#,
                    Valid,
                    None,
                ),
                (r#"<n:e xmlns:n="urn:n">p:a</n:e>"#, Valid, None),
                // QName first: the enumerated value is `{urn:n}a`.
                (
                    r#"<n:r xmlns:n="urn:n" xmlns:q="urn:n" qf="q:a"/>"#,
                    Valid,
                    None,
                ),
                (
                    r#"<n:r xmlns:n="urn:n" xmlns:p="urn:n" qf="p:a"/>"#,
                    Valid,
                    None,
                ),
                (
                    r#"<n:r xmlns:n="urn:n" xmlns:p="urn:x" qf="p:a"/>"#,
                    Invalid,
                    ENUM,
                ),
                // A NOTATION member, either order.
                (
                    r#"<n:r xmlns:n="urn:n" xmlns:q="urn:n" sn="q:gif"/>"#,
                    Invalid,
                    ENUM,
                ),
                (r#"<n:r xmlns:n="urn:n" sn="p:gif"/>"#, Valid, None),
                (
                    r#"<n:r xmlns:n="urn:n" xmlns:q="urn:n" ns="q:gif"/>"#,
                    Valid,
                    None,
                ),
                (
                    r#"<n:r xmlns:n="urn:n" xmlns:p="urn:x" ns="p:gif"/>"#,
                    Invalid,
                    ENUM,
                ),
                // A list member, either order.
                (
                    r#"<n:r xmlns:n="urn:n" xmlns:q="urn:n" sl="q:a q:b"/>"#,
                    Invalid,
                    ENUM,
                ),
                (r#"<n:r xmlns:n="urn:n" sl="p:a p:b"/>"#, Valid, None),
                (
                    r#"<n:r xmlns:n="urn:n" xmlns:q="urn:n" ls="q:a q:b"/>"#,
                    Valid,
                    None,
                ),
                (
                    r#"<n:r xmlns:n="urn:n" xmlns:p="urn:x" ls="p:a p:b"/>"#,
                    Invalid,
                    ENUM,
                ),
                // Literals only one member accepts.
                (r#"<n:r xmlns:n="urn:n" iq="05"/>"#, Valid, None),
                (
                    r#"<n:r xmlns:n="urn:n" xmlns:q="urn:n" iq="q:a"/>"#,
                    Valid,
                    None,
                ),
                (r#"<n:r xmlns:n="urn:n" iq="6"/>"#, Invalid, ENUM),
                (
                    r#"<n:r xmlns:n="urn:n" xmlns:q="urn:n" iq="q:b"/>"#,
                    Invalid,
                    ENUM,
                ),
            ],
        );
    }
}

/// The same rule for members of other primitive types: a literal an earlier
/// member accepts is that member's value, never equal to a value of another
/// primitive type. `05` is enumerated as a `\d+` token, so the integer `+5`
/// (which only the `xs:integer` member accepts) is not enumerated, while the
/// token `05` is.
#[test]
fn union_enumeration_literals_keep_their_primitive_type() {
    const ENUM: Option<&str> = Some("cvc-enumeration-valid");
    let body = r#"
      <xs:simpleType name="digits"><xs:restriction base="xs:token">
        <xs:pattern value="\d+"/>
      </xs:restriction></xs:simpleType>
      <xs:simpleType name="tokInt"><xs:restriction>
        <xs:simpleType><xs:union memberTypes="n:digits xs:integer"/></xs:simpleType>
        <xs:enumeration value="05"/>
      </xs:restriction></xs:simpleType>
      <xs:simpleType name="intTok"><xs:restriction>
        <xs:simpleType><xs:union memberTypes="xs:integer n:digits"/></xs:simpleType>
        <xs:enumeration value="05"/>
      </xs:restriction></xs:simpleType>
      <xs:element name="r"><xs:complexType>
        <xs:attribute name="ti" type="n:tokInt"/>
        <xs:attribute name="it" type="n:intTok"/>
      </xs:complexType></xs:element>"#;
    for version in VERSIONS {
        let ss = schema_in(version, "n", "urn:n", body);
        check_rows(
            &ss,
            &format!("{version:?}"),
            &[
                (r#"<n:r xmlns:n="urn:n" ti="05"/>"#, Valid, None),
                (r#"<n:r xmlns:n="urn:n" ti="+5"/>"#, Invalid, ENUM),
                (r#"<n:r xmlns:n="urn:n" ti="5"/>"#, Invalid, ENUM),
                (r#"<n:r xmlns:n="urn:n" it="5"/>"#, Valid, None),
                (r#"<n:r xmlns:n="urn:n" it="+5"/>"#, Valid, None),
                (r#"<n:r xmlns:n="urn:n" it="005"/>"#, Valid, None),
                (r#"<n:r xmlns:n="urn:n" it="6"/>"#, Invalid, ENUM),
            ],
        );
    }
}
