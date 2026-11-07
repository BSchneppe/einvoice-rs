//! The string values of a typed document are the [schema normalized value]s.
//!
//! XDM 1.0 §6.2.4 (Construction from a PSVI, element string-value): "If the
//! element has a simple type or a complex type with simple content: its
//! string-value is the [schema normalized value] of the node", and the
//! element's children may be its comments and processing instructions "plus an
//! optional single Text Node whose string value is the [schema normalized
//! value] … If the [schema normalized value] is the empty string, the Text Node
//! must not be present". §6.3.4 (attributes): "The [schema normalized value]
//! PSVI property if that exists. Otherwise, the [normalized value] property."
//! An element or attribute that is not valid has no [schema normalized value]
//! (Structures §3.3.5.4, §3.2.5): its type is `xs:anyType` / `xs:anySimpleType`
//! (XDM §3.3.1.1) and its string value is its text as written. Mixed and
//! element-only content keep their text: "the concatenation of the
//! string-values of all its Text Node descendants".
//!
//! Every case runs with and without PSVI typed values, in XSD 1.0 and 1.1 mode.
#![cfg(feature = "xsd11")]

use bumpalo::Bump;
use xsd_schema::document::{
    build_typed_document_with, BufferDocument, BufferDocumentOptions, TypedBuildOptions,
};
use xsd_schema::navigator::{DomNavigator, TypedValue};
use xsd_schema::validation::{NoopHandler, SchemaValidator, SchemaValidity, ValidationFlags};
use xsd_schema::{SchemaSet, SchemaSetBuilder, XsdVersion};

const VERSIONS: [XsdVersion; 2] = [XsdVersion::V1_0, XsdVersion::V1_1];

fn schema(version: XsdVersion, body: &str) -> SchemaSet {
    let xsd = format!(
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
             <xs:simpleType name="ints"><xs:list itemType="xs:integer"/></xs:simpleType>
             <xs:simpleType name="intOrString"><xs:union memberTypes="xs:integer xs:string"/></xs:simpleType>
             {body}
           </xs:schema>"#
    );
    SchemaSetBuilder::with_version(version)
        .add_source(&xsd, "file:///typed_string_values.xsd")
        .expect("schema source accepted")
        .compile()
        .expect("schema compiles")
        .into_schema_set()
}

/// Build `xml` as a typed document, with or without PSVI typed values, and
/// hand it (and the root validity) to `check`.
fn build<F: FnMut(&BufferDocument<'_>, Option<SchemaValidity>)>(
    ss: &SchemaSet,
    xml: &str,
    psvi: bool,
    mut check: F,
) {
    let flags = if psvi {
        ValidationFlags::default()
    } else {
        ValidationFlags::default() & !ValidationFlags::BUILD_PSVI_TYPED_VALUES
    };
    let validator = SchemaValidator::new(ss, flags);
    let mut runtime = validator.start_run(xsd_schema::document::SilentValidationSink);
    let arena = Bump::new();
    let built = build_typed_document_with(
        xml.as_bytes(),
        &arena,
        ss,
        &mut runtime,
        &mut NoopHandler,
        TypedBuildOptions::default().with_document_options(BufferDocumentOptions::full()),
    )
    .expect("the build completes");
    check(&built.document, built.outcome.root_validity);
}

/// The string value of the node at `path` (child element names from the
/// document element, `@name` for an attribute of the last one).
fn string_value(doc: &BufferDocument<'_>, path: &[&str]) -> String {
    let mut nav = doc.create_navigator();
    assert!(nav.move_to_first_child(), "a document element");
    for step in path {
        if let Some(attr) = step.strip_prefix('@') {
            assert!(
                nav.move_to_first_attribute(),
                "attributes on the way to {step}"
            );
            while nav.local_name() != attr {
                assert!(nav.move_to_next_attribute(), "attribute {step}");
            }
        } else {
            assert!(nav.move_to_first_child(), "children on the way to {step}");
            while !(nav.node_type() == xsd_schema::navigator::DomNodeType::Element
                && nav.local_name() == *step)
            {
                assert!(nav.move_to_next_sibling(), "element {step}");
            }
        }
    }
    nav.value()
}

const BODY: &str = r###"
  <xs:element name="r"><xs:complexType mixed="false"><xs:sequence>
    <xs:element name="t" type="xs:token" minOccurs="0"/>
    <xs:element name="s" type="xs:string" minOccurs="0"/>
    <xs:element name="n" type="xs:normalizedString" minOccurs="0"/>
    <xs:element name="i" type="xs:integer" minOccurs="0"/>
    <xs:element name="l" type="ints" minOccurs="0"/>
    <xs:element name="u" type="intOrString" minOccurs="0"/>
    <xs:element name="w" type="xs:token" minOccurs="0"/>
    <xs:element name="c" type="xs:token" minOccurs="0"/>
    <xs:element name="sc" minOccurs="0"><xs:complexType><xs:simpleContent>
      <xs:extension base="xs:token"><xs:attribute name="a" type="xs:token"/></xs:extension>
    </xs:simpleContent></xs:complexType></xs:element>
    <xs:element name="m" minOccurs="0"><xs:complexType mixed="true"><xs:sequence>
      <xs:element name="b" type="xs:token" minOccurs="0"/>
    </xs:sequence></xs:complexType></xs:element>
    <xs:element name="nil" type="xs:token" nillable="true" minOccurs="0"/>
  </xs:sequence>
  <xs:attribute name="at" type="xs:token"/>
  <xs:attribute name="as" type="xs:string"/>
  <xs:attribute name="ai" type="xs:integer"/>
  <xs:attribute name="al" type="ints"/>
  <xs:anyAttribute namespace="##other" processContents="skip"/>
  </xs:complexType></xs:element>"###;

const VALID: &str =
    "<r xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xml:space=\"preserve\" \
     at=\"  a   b  \" as=\"  a  \" ai=\" 7 \" al=\" 1  2 \">\
     <t>  a   b  </t><s>  a  </s><n>a&#9;b</n><i> 7 </i><l> 1   2 </l><u> 5 </u>\
     <w>   </w><c> a <!--x--> b <?p d?> c </c><sc a=\" x  y \"> p  q </sc>\
     <m> x <b> y  z </b> w </m><nil xsi:nil=\"true\"/></r>";

/// Simple types and simple content: the [schema normalized value];
/// `xs:string` keeps its white space; `xml:space` plays no part (it is not a
/// schema facet).
#[test]
fn simple_content_and_attributes_carry_the_schema_normalized_value() {
    for version in VERSIONS {
        let ss = schema(version, BODY);
        for psvi in [true, false] {
            build(&ss, VALID, psvi, |doc, validity| {
                let label = format!("{version:?} psvi={psvi}");
                assert_eq!(validity, Some(SchemaValidity::Valid), "{label}");
                let cases: [(&[&str], &str); 16] = [
                    (&["@at"], "a b"),
                    (&["@as"], "  a  "),
                    (&["@ai"], "7"),
                    (&["@al"], "1 2"),
                    (&["t"], "a b"),
                    (&["s"], "  a  "),
                    (&["n"], "a b"),
                    (&["i"], "7"),
                    (&["l"], "1 2"),
                    (&["u"], "5"),
                    (&["w"], ""),
                    (&["c"], "a b c"),
                    (&["sc"], "p q"),
                    (&["sc", "@a"], "x y"),
                    // Mixed content: the text as written, the typed child's
                    // text normalized.
                    (&["m"], " x y z w "),
                    (&["m", "b"], "y z"),
                ];
                for (path, expected) in cases {
                    assert_eq!(string_value(doc, path), expected, "{label} {path:?}");
                }
            });
        }
    }
}

/// The text node a valid simple-content element keeps is one, with the
/// normalized value (none for the empty string); its comments and processing
/// instructions stay, in order; typed values agree; the element's source span
/// still covers the text as written.
#[test]
fn the_text_node_and_the_source_span() {
    for version in VERSIONS {
        let ss = schema(version, BODY);
        for psvi in [true, false] {
            build(&ss, VALID, psvi, |doc, _| {
                let label = format!("{version:?} psvi={psvi}");
                // `c`: text node "a b c", the comment and the PI.
                let mut nav = doc.create_navigator();
                assert!(nav.move_to_first_child());
                assert!(nav.move_to_first_child());
                let mut kinds = Vec::new();
                loop {
                    if nav.local_name() == "c" {
                        let mut child = nav.clone();
                        assert!(child.move_to_first_child());
                        loop {
                            kinds.push((format!("{:?}", child.node_type()), child.value()));
                            if !child.move_to_next_sibling() {
                                break;
                            }
                        }
                        // The element's span is the source text as written.
                        let span = doc.source_span(nav_ref(&nav)).expect("a span");
                        assert_eq!(
                            &VALID[span.start..span.end],
                            "<c> a <!--x--> b <?p d?> c </c>",
                            "{label}"
                        );
                    }
                    if nav.local_name() == "w" {
                        assert!(
                            !nav.clone().move_to_first_child(),
                            "{label}: no text node in w"
                        );
                    }
                    if nav.local_name() == "t" {
                        match nav.typed_value() {
                            TypedValue::Value(v) => {
                                assert_eq!(v.to_string_value(), "a b", "{label}")
                            }
                            other => panic!("{label}: {other:?}"),
                        }
                    }
                    if !nav.move_to_next_sibling() {
                        break;
                    }
                }
                let texts: Vec<_> = kinds.iter().filter(|(k, _)| k == "Text").collect();
                assert_eq!(texts.len(), 1, "{label}: {kinds:?}");
                assert_eq!(texts[0].1, "a b c", "{label}");
                assert!(
                    kinds.iter().any(|(k, v)| k == "Comment" && v == "x"),
                    "{label}: {kinds:?}"
                );
                assert!(
                    kinds.iter().any(|(k, _)| k == "ProcessingInstruction"),
                    "{label}: {kinds:?}"
                );
            });
        }
    }
}

/// A node that is not valid has no [schema normalized value]: its text and
/// attribute value stay as written (its comments and text in their order).
#[test]
fn invalid_nodes_keep_their_text() {
    let xml = "<r ai=\" x \"><t>  a   b  </t><i>  x  <!--y--> z </i></r>";
    for version in VERSIONS {
        let ss = schema(version, BODY);
        for psvi in [true, false] {
            build(&ss, xml, psvi, |doc, validity| {
                let label = format!("{version:?} psvi={psvi}");
                assert_eq!(validity, Some(SchemaValidity::Invalid), "{label}");
                assert_eq!(string_value(doc, &["@ai"]), " x ", "{label}");
                assert_eq!(string_value(doc, &["i"]), "  x   z ", "{label}");
                // A valid sibling is still normalized.
                assert_eq!(string_value(doc, &["t"]), "a b", "{label}");
            });
        }
    }
}

/// The node index of the navigator's current node, through the public
/// position API.
fn nav_ref(nav: &xsd_schema::document::navigator::BufferDocNavigator<'_>) -> u32 {
    nav.current_ref()
}

/// An assertion's data model (Structures §3.13.4.1 clause 1: built from the
/// "partial" PSVI of E) has the same string values: an attribute's and a
/// simple-content child's [schema normalized value]; mixed content as read.
#[test]
fn assertions_see_schema_normalized_string_values() {
    use xsd_schema::validation::{drive_navigator, drive_quick_xml, CollectingValidationSink};
    use xsd_schema::RoXmlNavigator;
    let body = r#"
      <xs:element name="r"><xs:complexType><xs:sequence>
        <xs:element name="t" type="xs:token"/>
        <xs:element name="m"><xs:complexType mixed="true"><xs:sequence>
          <xs:element name="b" type="xs:token" minOccurs="0"/>
        </xs:sequence></xs:complexType></xs:element>
      </xs:sequence>
      <xs:attribute name="a" type="xs:token"/>
      <xs:assert test="string(@a) eq 'x y' and string(t) eq 'p q'"/>
      <xs:assert test="string(m) eq ' u v w ' and string(m/b) eq 'v'"/>
      </xs:complexType></xs:element>
      <xs:element name="e"><xs:complexType><xs:simpleContent>
        <xs:extension base="xs:token">
          <xs:assert test="string(.) eq 'k l' and $value eq 'k l'"/>
        </xs:extension>
      </xs:simpleContent></xs:complexType></xs:element>"#;
    let ss = schema(XsdVersion::V1_1, body);
    for xml in [
        r#"<r a="  x   y "><t>  p  q </t><m> u <b>  v </b> w </m></r>"#,
        r#"<e>  k    l </e>"#,
    ] {
        for driver in ["stream", "stream/no-psvi", "navigator"] {
            let flags = match driver {
                "stream/no-psvi" => {
                    ValidationFlags::default() & !ValidationFlags::BUILD_PSVI_TYPED_VALUES
                }
                _ => ValidationFlags::default(),
            } | ValidationFlags::PROCESS_ASSERTIONS;
            let v = SchemaValidator::new_fragment_buffer(&ss, flags);
            let mut errors = Vec::new();
            let mut warnings = Vec::new();
            let validity = {
                let mut rt = v.start_run(CollectingValidationSink {
                    errors: &mut errors,
                    warnings: &mut warnings,
                });
                if driver == "navigator" {
                    let doc = roxmltree::Document::parse(xml).unwrap();
                    drive_navigator(&RoXmlNavigator::new(&doc), &mut rt, &ss)
                        .unwrap()
                        .root_validity
                } else {
                    drive_quick_xml(xml.as_bytes(), &mut rt, &ss)
                        .unwrap()
                        .root_validity
                }
            };
            assert_eq!(
                validity,
                Some(SchemaValidity::Valid),
                "{driver}: {xml}: {errors:?}"
            );
        }
    }
}
