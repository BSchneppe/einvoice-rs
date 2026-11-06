//! A typed document is the XDM instance of its PSVI: defaulted attributes and
//! element defaults are nodes, white space in element-only content is not a
//! text node, and the typed value of a union-typed node is the value of the
//! member type that validated it.
//!
//! XDM 1.0 §6.2.4 (Construction from a PSVI, element `attributes`): "Default
//! and fixed attributes provided by XML Schema processing are added to the
//! [attributes] and are therefore included in the data model attributes of an
//! element"; `children`: "where a fixed or default value for an element is
//! defined in the schema, and the element takes this default value, a text
//! node will be created to contain the value". Structures §3.4.5.1 adds each
//! ·defaulted attribute· to the element's [attributes], with "·namespace
//! fixup·" when no prefix is bound to its namespace, and §3.3.5.4 gives an
//! element for which clause 5.1 of Element Locally Valid (Element) applied the
//! {lexical form} of its {value constraint} as its [schema normalized value].
//!
//! XDM 1.0 §6.7.4: "if the resulting Text Node consists entirely of whitespace
//! and the character information items used to construct this node have a
//! parent and that parent is an element and its {content type} is not
//! “mixed”, then the content of the Text Node is the zero-length string … an
//! empty Text Node will be discarded when its parent is constructed".
//!
//! XDM 1.0 §3.3.1.2: the typed value is M applied to the string value "as an
//! instance of the appropriate value type, where the appropriate value type is
//! the [member type definition] if T is a union type"; XPath 2.0 §2.5.2: "when
//! the type annotation of a node is a union type, the typed value of the node
//! will be an instance of one of the member types of the union".
//!
//! Every case runs in XSD 1.0 and 1.1 mode, with and without PSVI typed values.
#![cfg(feature = "xsd11")]

use std::cell::RefCell;

use bumpalo::Bump;
use xsd_schema::document::{
    build_typed_document_with, BufferDocNavigator, BufferDocument, BufferDocumentBuilder,
    BufferDocumentOptions, SilentValidationSink, TypedBuildOptions, TypedBuilderHandler,
};
use xsd_schema::namespace::context::NamespaceContextSnapshot;
use xsd_schema::navigator::{DomNavigator, DomNodeType, TypedValue};
use xsd_schema::types::value::XmlValueKind;
use xsd_schema::validation::{
    drive_quick_xml_with, AttributeView, EndOfAttributesView, NoopHandler, SchemaInfo,
    SchemaValidator, SchemaValidity, TextKind, ValidationEventHandler, ValidationFlags,
};
use xsd_schema::xpath::api::XPathExpr;
use xsd_schema::xpath::{XPathContext, XmlItem};
use xsd_schema::{SchemaSet, SchemaSetBuilder, TypeKey, XsdVersion};

const VERSIONS: [XsdVersion; 2] = [XsdVersion::V1_0, XsdVersion::V1_1];

fn compile(version: XsdVersion, documents: &[&str]) -> SchemaSet {
    let mut builder = SchemaSetBuilder::with_version(version);
    for (i, text) in documents.iter().enumerate() {
        builder = builder
            .add_source(text, &format!("file:///typed_tree_xdm_{i}.xsd"))
            .unwrap_or_else(|e| panic!("schema document {i}: {e}"));
    }
    builder
        .compile()
        .unwrap_or_else(|e| panic!("{version:?}: {e}"))
        .into_schema_set()
}

fn flags(psvi: bool) -> ValidationFlags {
    if psvi {
        ValidationFlags::default()
    } else {
        ValidationFlags::default() & !ValidationFlags::BUILD_PSVI_TYPED_VALUES
    }
}

/// Build `xml` typed with `options` and hand the document and the root's
/// validity to `check`.
fn build_with<F>(ss: &SchemaSet, xml: &str, psvi: bool, options: TypedBuildOptions, check: F)
where
    F: FnOnce(&BufferDocument<'_>, Option<SchemaValidity>),
{
    let validator = SchemaValidator::new(ss, flags(psvi));
    let mut runtime = validator.start_run(SilentValidationSink);
    let arena = Bump::new();
    let built = build_typed_document_with(
        xml.as_bytes(),
        &arena,
        ss,
        &mut runtime,
        &mut NoopHandler,
        options.with_document_options(BufferDocumentOptions::full()),
    )
    .expect("the build completes");
    check(&built.document, built.outcome.root_validity);
}

fn build<F>(ss: &SchemaSet, xml: &str, psvi: bool, check: F)
where
    F: FnOnce(&BufferDocument<'_>, Option<SchemaValidity>),
{
    build_with(ss, xml, psvi, TypedBuildOptions::default(), check)
}

/// Evaluate `expr` with the document node as the context item; each item of
/// the result as a string (a node's string value, an atomic value's).
fn eval(ss: &SchemaSet, doc: &BufferDocument<'_>, expr: &str) -> Vec<String> {
    let mut ns = NamespaceContextSnapshot::default();
    ns.bindings.push((
        ss.name_table.add("xs"),
        ss.name_table.add("http://www.w3.org/2001/XMLSchema"),
    ));
    let ctx = XPathContext::new(&ss.name_table)
        .with_schema_set(ss)
        .with_namespaces(ns);
    let compiled = XPathExpr::compile(expr, &ctx).unwrap_or_else(|e| panic!("{expr}: {e}"));
    let value = compiled
        .evaluator(&ctx)
        .run_with_node(doc.create_navigator())
        .unwrap_or_else(|e| panic!("{expr}: {e}"));
    value
        .into_vec()
        .iter()
        .map(|item| match item {
            XmlItem::Node(node) => node.value(),
            XmlItem::Atomic(value) => value.to_string_value(),
        })
        .collect()
}

/// The single item of `expr`'s result as a string, or the error's code.
fn try_eval1(ss: &SchemaSet, doc: &BufferDocument<'_>, expr: &str) -> String {
    let mut ns = NamespaceContextSnapshot::default();
    ns.bindings.push((
        ss.name_table.add("xs"),
        ss.name_table.add("http://www.w3.org/2001/XMLSchema"),
    ));
    let ctx = XPathContext::new(&ss.name_table)
        .with_schema_set(ss)
        .with_namespaces(ns);
    let compiled = match XPathExpr::compile(expr, &ctx) {
        Ok(compiled) => compiled,
        Err(e) => return format!("error {e}"),
    };
    match compiled
        .evaluator(&ctx)
        .run_with_node(doc.create_navigator())
    {
        Ok(value) => value
            .into_vec()
            .iter()
            .map(|item| match item {
                XmlItem::Node(node) => node.value(),
                XmlItem::Atomic(value) => value.to_string_value(),
            })
            .collect::<Vec<_>>()
            .join(" | "),
        Err(e) => format!("error {e}"),
    }
}

fn eval1(ss: &SchemaSet, doc: &BufferDocument<'_>, expr: &str) -> String {
    let items = eval(ss, doc, expr);
    assert_eq!(items.len(), 1, "{expr}: one item expected, got {items:?}");
    items.into_iter().next().unwrap()
}

/// `(name, value)` of every attribute of the element `nav` is on, sorted.
fn attributes(nav: &BufferDocNavigator<'_>) -> Vec<(String, String)> {
    let mut nav = nav.clone();
    let mut out = Vec::new();
    if nav.move_to_first_attribute() {
        loop {
            out.push((nav.name().to_string(), nav.value()));
            if !nav.move_to_next_attribute() {
                break;
            }
        }
    }
    out.sort();
    out
}

fn document_element<'d>(doc: &'d BufferDocument<'d>) -> BufferDocNavigator<'d> {
    let mut nav = doc.create_navigator();
    assert!(nav.move_to_first_child(), "a document element");
    while nav.node_type() != DomNodeType::Element {
        assert!(nav.move_to_next_sibling(), "a document element");
    }
    nav
}

// ── C39: defaulted attributes ─────────────────────────────────────────

const DEFAULTS: &str = r###"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
    xmlns:t="urn:t" targetNamespace="urn:t">
  <xs:attribute name="ga" type="xs:token" default=" g  1 "/>
  <xs:attributeGroup name="grp">
    <xs:attribute name="gr" type="xs:integer" default="007"/>
  </xs:attributeGroup>
  <xs:complexType name="base">
    <xs:attribute name="inh" type="xs:string" default=" from base "/>
  </xs:complexType>
  <xs:element name="r">
    <xs:complexType><xs:complexContent><xs:extension base="t:base">
      <xs:sequence>
        <xs:element name="d" type="xs:string" default="dflt" minOccurs="0" maxOccurs="unbounded"/>
        <xs:element name="f" type="xs:token" fixed="  a  b " minOccurs="0" maxOccurs="unbounded"/>
        <xs:element name="i" type="xs:integer" default="05" minOccurs="0"/>
        <xs:element name="sc" default="12" minOccurs="0" maxOccurs="unbounded">
          <xs:complexType><xs:simpleContent><xs:extension base="xs:integer">
            <xs:attribute name="u" type="xs:string" default="kg"/>
          </xs:extension></xs:simpleContent></xs:complexType>
        </xs:element>
        <xs:element name="m" default="mixed default" minOccurs="0">
          <xs:complexType mixed="true"><xs:sequence>
            <xs:element name="b" minOccurs="0"/>
          </xs:sequence></xs:complexType>
        </xs:element>
        <xs:element name="n" type="xs:string" default="nil default" nillable="true" minOccurs="0"/>
        <xs:element name="v" type="xs:integer" default="1" minOccurs="0"/>
      </xs:sequence>
      <xs:attribute name="loc" type="xs:token" default="  l  1 "/>
      <xs:attribute name="fix" type="xs:string" fixed="F"/>
      <xs:attribute ref="t:ga"/>
      <xs:attribute name="present" type="xs:string" default="unused"/>
      <xs:attribute name="noval" type="xs:string"/>
      <xs:attributeGroup ref="t:grp"/>
    </xs:extension></xs:complexContent></xs:complexType>
  </xs:element>
</xs:schema>"###;

/// Every defaulted attribute — a local use, a fixed one, a reference to a
/// global declaration, an attribute group's, one inherited from the base type
/// — is an attribute node with its value constraint's {lexical form} (the
/// ·normalized value· with respect to its type, Structures §3.2.2), typed, of
/// its declared type. An attribute the element carries is not defaulted, and
/// a use without a value constraint adds nothing.
#[test]
fn defaulted_attributes_are_attribute_nodes() {
    for version in VERSIONS {
        let ss = compile(version, &[DEFAULTS]);
        for psvi in [true, false] {
            build(
                &ss,
                r#"<t:r xmlns:t="urn:t" present="here"/>"#,
                psvi,
                |doc, validity| {
                    let label = format!("{version:?} psvi={psvi}");
                    assert_eq!(validity, Some(SchemaValidity::Valid), "{label}");
                    let r = document_element(doc);
                    assert_eq!(
                        attributes(&r),
                        [
                            ("fix", "F"),
                            ("gr", "007"),
                            ("inh", " from base "),
                            ("loc", "l 1"),
                            ("present", "here"),
                            ("t:ga", "g 1"),
                        ]
                        .map(|(n, v)| (n.to_string(), v.to_string())),
                        "{label}"
                    );
                    let mut attr = r.clone();
                    assert!(attr.move_to_first_attribute());
                    loop {
                        assert!(
                            matches!(attr.type_annotation(), Some(TypeKey::Simple(_))),
                            "{label}: @{} is typed",
                            attr.name()
                        );
                        assert!(
                            matches!(attr.typed_value(), TypedValue::Value(_)),
                            "{label}: @{} has a typed value",
                            attr.name()
                        );
                        if !attr.move_to_next_attribute() {
                            break;
                        }
                    }
                    assert_eq!(eval1(&ss, doc, "count(/*/@*)"), "6", "{label}");
                    assert_eq!(
                        eval1(&ss, doc, "data(/*/@gr) instance of xs:integer"),
                        "true",
                        "{label}"
                    );
                    assert_eq!(eval1(&ss, doc, "/*/@gr + 1"), "8", "{label}");
                    assert_eq!(
                        eval1(&ss, doc, "namespace-uri(/*/@*[local-name() = 'ga'])"),
                        "urn:t",
                        "{label}"
                    );
                },
            );
        }
    }
}

/// Structures §3.4.5.1: a defaulted attribute in a namespace takes "a
/// namespace prefix bound to N in the [in-scope namespaces]", and when there
/// is none "an ·implementation-dependent· prefix", the [in-scope namespaces]
/// being "also augmented" (·namespace fixup·).
#[test]
fn a_defaulted_attribute_in_a_namespace_gets_a_prefix_bound_in_scope() {
    let q = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
        targetNamespace="urn:q"><xs:attribute name="qa" type="xs:string" default="q1"/></xs:schema>"#;
    let main = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:q="urn:q">
        <xs:import namespace="urn:q"/>
        <xs:element name="e"><xs:complexType><xs:sequence>
          <xs:element name="c" minOccurs="0"><xs:complexType>
            <xs:attribute ref="q:qa"/>
          </xs:complexType></xs:element>
        </xs:sequence><xs:attribute ref="q:qa"/></xs:complexType></xs:element>
      </xs:schema>"#;
    for version in VERSIONS {
        let ss = compile(version, &[q, main]);
        for psvi in [true, false] {
            let label = format!("{version:?} psvi={psvi}");
            // A prefix in scope is used.
            build(
                &ss,
                r#"<e xmlns:z="urn:q"><c/></e>"#,
                psvi,
                |doc, validity| {
                    assert_eq!(validity, Some(SchemaValidity::Valid), "{label}");
                    assert_eq!(eval1(&ss, doc, "name(/e/@*)"), "z:qa", "{label}");
                    assert_eq!(eval1(&ss, doc, "name(/e/c/@*)"), "z:qa", "{label}");
                    assert_eq!(eval1(&ss, doc, "string(/e/@*)"), "q1", "{label}");
                },
            );
            // None in scope: a prefix is declared on the element.
            build(&ss, "<e><c/></e>", psvi, |doc, validity| {
                assert_eq!(validity, Some(SchemaValidity::Valid), "{label}");
                assert_eq!(eval1(&ss, doc, "namespace-uri(/e/@*)"), "urn:q", "{label}");
                let prefix = eval1(&ss, doc, "substring-before(name(/e/@*), ':')");
                assert!(!prefix.is_empty(), "{label}: a prefix");
                assert_eq!(
                    eval1(
                        &ss,
                        doc,
                        &format!("namespace-uri-for-prefix('{prefix}', /e)")
                    ),
                    "urn:q",
                    "{label}"
                );
                // The child sees the binding and uses it.
                assert_eq!(
                    eval1(&ss, doc, "name(/e/c/@*)"),
                    format!("{prefix}:qa"),
                    "{label}"
                );
                assert_eq!(eval1(&ss, doc, "count(/e/c/namespace::*)"), "2");
            });
        }
    }
}

// ── C39: element defaults ─────────────────────────────────────────────

const ELEMENTS: &str = r#"<t:r xmlns:t="urn:t" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><d/><d></d><d><!--c--><?p x?></d><d> </d><f/><f>a b</f><i/><sc/><sc u="lb"/><m/><n xsi:nil="true"/></t:r>"#;

/// An empty element that takes its `default` / `fixed` value (clause 5.1 of
/// Element Locally Valid (Element): no element or character [children], not
/// nilled) has a text node with it — the {lexical form}, white space
/// normalized as its type prescribes — beside its comments and processing
/// instructions; an element with character children (`<d> </d>`) takes no
/// default. A nilled element has none; nor has a mixed-content element (XDM
/// leaves that case to the processor: "may represent such a default value by
/// creating a text node, but are not required to do so").
#[test]
fn an_empty_element_takes_its_default_as_a_text_node() {
    for version in VERSIONS {
        let ss = compile(version, &[DEFAULTS]);
        for psvi in [true, false] {
            build(&ss, ELEMENTS, psvi, |doc, validity| {
                let label = format!("{version:?} psvi={psvi}");
                assert_eq!(validity, Some(SchemaValidity::Valid), "{label}");
                let e = |expr: &str| eval1(&ss, doc, expr);
                for d in 1..=2 {
                    assert_eq!(e(&format!("string(/*/d[{d}])")), "dflt", "{label} d[{d}]");
                    assert_eq!(e(&format!("count(/*/d[{d}]/text())")), "1", "{label}");
                }
                assert_eq!(e("string(/*/d[3])"), "dflt", "{label}");
                assert_eq!(e("count(/*/d[3]/node())"), "3", "{label}");
                assert_eq!(e("count(/*/d[3]/comment())"), "1", "{label}");
                assert_eq!(e("count(/*/d[3]/processing-instruction())"), "1");
                assert_eq!(e("string(/*/d[4])"), " ", "{label}");
                assert_eq!(e("string(/*/f[1])"), "a b", "{label}");
                assert_eq!(e("string(/*/f[2])"), "a b", "{label}");
                assert_eq!(e("string(/*/i)"), "05", "{label}");
                assert_eq!(e("data(/*/i) instance of xs:integer"), "true");
                assert_eq!(e("/*/i + 0"), "5", "{label}");
                assert_eq!(e("string(/*/sc[1])"), "12", "{label}");
                assert_eq!(e("string(/*/sc[1]/@u)"), "kg", "{label}");
                assert_eq!(e("string(/*/sc[2])"), "12", "{label}");
                assert_eq!(e("string(/*/sc[2]/@u)"), "lb", "{label}");
                assert_eq!(e("count(/*/m/node())"), "0", "{label}");
                assert_eq!(e("nilled(/*/n)"), "true", "{label}");
                assert_eq!(e("count(/*/n/node())"), "0", "{label}");
                // The navigator's typed value agrees with the text node.
                let mut d = document_element(doc);
                assert!(d.move_to_first_child());
                assert_eq!(d.value(), "dflt");
                match d.typed_value() {
                    TypedValue::Value(v) => assert_eq!(v.to_string_value(), "dflt"),
                    other => panic!("{label}: {other:?}"),
                }
            });
        }
    }
}

/// An element that is not valid has no [schema normalized value] and keeps
/// its content as read: no default text node.
#[test]
fn an_invalid_empty_element_takes_no_default_text() {
    for version in VERSIONS {
        let ss = compile(version, &[DEFAULTS]);
        for psvi in [true, false] {
            build(
                &ss,
                r#"<t:r xmlns:t="urn:t"><v bogus="x"/></t:r>"#,
                psvi,
                |doc, validity| {
                    assert_eq!(validity, Some(SchemaValidity::Invalid));
                    assert_eq!(eval1(&ss, doc, "count(/*/v/node())"), "0");
                },
            );
        }
    }
}

/// `TypedBuildOptions::with_defaults_as_nodes(false)` builds the tree as read:
/// no defaulted attribute, no default text (the navigator still gives an empty
/// defaulted element its default as its typed value, as before).
#[test]
fn defaults_as_nodes_can_be_turned_off() {
    for version in VERSIONS {
        let ss = compile(version, &[DEFAULTS]);
        for psvi in [true, false] {
            build_with(
                &ss,
                ELEMENTS,
                psvi,
                TypedBuildOptions::default().with_defaults_as_nodes(false),
                |doc, validity| {
                    assert_eq!(validity, Some(SchemaValidity::Valid));
                    assert_eq!(eval1(&ss, doc, "count(/*/@*)"), "0");
                    assert_eq!(eval1(&ss, doc, "count(/*/sc[1]/@*)"), "0");
                    assert_eq!(eval1(&ss, doc, "string(/*/d[1])"), "");
                    assert_eq!(eval1(&ss, doc, "data(/*/d[1])"), "dflt");
                },
            );
        }
    }
}

/// XSD 1.1 allows a value constraint on an `xs:ID` attribute (XSD 1.0's
/// a-props-correct.3 does not): the defaulted attribute is an id.
#[test]
fn a_defaulted_id_attribute_is_an_id() {
    let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
        <xs:element name="doc"><xs:complexType><xs:sequence>
          <xs:element name="item"><xs:complexType>
            <xs:attribute name="key" type="xs:ID" default="only"/>
          </xs:complexType></xs:element>
        </xs:sequence></xs:complexType></xs:element>
      </xs:schema>"#;
    let ss = compile(XsdVersion::V1_1, &[xsd]);
    for psvi in [true, false] {
        build(&ss, "<doc><item/></doc>", psvi, |doc, validity| {
            assert_eq!(validity, Some(SchemaValidity::Valid));
            assert_eq!(eval1(&ss, doc, "name(id('only'))"), "item");
        });
    }
}

/// The handler of a pass sees the defaulted attributes of each element in
/// [`EndOfAttributesView::default_attributes`], the same set
/// `get_default_attributes` gives, after the builder created them.
#[test]
fn the_handler_sees_the_defaulted_attributes() {
    #[derive(Default)]
    struct Seen(RefCell<Vec<String>>);
    impl ValidationEventHandler for &Seen {
        type Error = std::convert::Infallible;
        fn after_end_of_attributes(
            &mut self,
            view: EndOfAttributesView<'_>,
        ) -> Result<(), Self::Error> {
            for d in view.default_attributes {
                self.0.borrow_mut().push(d.value.clone());
            }
            Ok(())
        }
    }
    for version in VERSIONS {
        let ss = compile(version, &[DEFAULTS]);
        let validator = SchemaValidator::new(&ss, ValidationFlags::default());
        let mut runtime = validator.start_run(SilentValidationSink);
        let arena = Bump::new();
        let mut builder =
            BufferDocumentBuilder::new(&arena, &ss.name_table, Some(&ss), Default::default())
                .unwrap();
        let seen = Seen::default();
        {
            let mut handler = TypedBuilderHandler::with_handler(&mut builder, &seen);
            drive_quick_xml_with(
                r#"<t:r xmlns:t="urn:t"><sc/></t:r>"#.as_bytes(),
                &mut runtime,
                &ss,
                &mut handler,
            )
            .expect("the pass completes");
        }
        let mut values = seen.0.into_inner();
        values.sort();
        assert_eq!(
            values,
            [
                "  l  1 ",
                " from base ",
                " g  1 ",
                "007",
                "F",
                "kg",
                "unused"
            ],
            "{version:?}: the lexical forms as the schema writes them"
        );
        let doc = builder.finalize().unwrap();
        assert_eq!(eval1(&ss, &doc, "count(/*/@*)"), "6");
        assert_eq!(eval1(&ss, &doc, "string(/*/sc/@u)"), "kg");
    }
}

// ── C67: white space in element-only content ──────────────────────────

const WHITESPACE: &str = r###"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:complexType name="eo"><xs:sequence>
    <xs:element name="a" type="xs:string" minOccurs="0"/>
    <xs:element name="b" type="xs:string" minOccurs="0"/>
  </xs:sequence></xs:complexType>
  <xs:element name="r"><xs:complexType><xs:sequence>
    <xs:element name="eo" type="eo" minOccurs="0"/>
    <xs:element name="mx" minOccurs="0"><xs:complexType mixed="true"><xs:sequence>
      <xs:element name="a" type="xs:string" minOccurs="0"/>
    </xs:sequence></xs:complexType></xs:element>
    <xs:element name="s" type="xs:string" minOccurs="0"/>
    <xs:element name="tk" type="xs:token" minOccurs="0"/>
    <xs:element name="any" minOccurs="0"/>
    <xs:element name="nl" type="eo" nillable="true" minOccurs="0"/>
    <xs:element name="w" minOccurs="0"><xs:complexType><xs:sequence>
      <xs:any processContents="skip"/>
    </xs:sequence></xs:complexType></xs:element>
  </xs:sequence></xs:complexType></xs:element>
  <xs:element name="em"><xs:complexType/></xs:element>
  <xs:element name="req"><xs:complexType><xs:sequence>
    <xs:element name="a" type="xs:string"/>
    <xs:element name="b" type="xs:string"/>
  </xs:sequence></xs:complexType></xs:element>
</xs:schema>"###;

const WS_DOC: &str = "<r xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\n  \
    <eo>\n    <a>1</a>\n    <!-- c -->\n    <b> </b>\n    <?pi d?>\n  </eo>\n  \
    <mx> <a>2</a> </mx>\n  <s> </s>\n  <tk> </tk>\n  \
    <any xsi:type=\"eo\">\n    <a>3</a>\n  </any>\n  \
    <nl xsi:nil=\"true\">\n  </nl>\n  \
    <w>\n    <x>  <y/>  </x>\n  </w>\n</r>";

/// White space of an element whose content is element-only is no text node,
/// between elements, comments and processing instructions alike, also when
/// `xsi:type` gives the element-only type and for a nilled element. Mixed
/// content, simple content (`xs:string` keeps its white space; `xs:token`
/// normalizes it away, C66), and an element no declaration governs keep it.
#[test]
fn whitespace_in_element_only_content_is_no_text_node() {
    for version in VERSIONS {
        let ss = compile(version, &[WHITESPACE]);
        for psvi in [true, false] {
            build(&ss, WS_DOC, psvi, |doc, validity| {
                let label = format!("{version:?} psvi={psvi}");
                assert_eq!(validity, Some(SchemaValidity::Valid), "{label}");
                let e = |expr: &str| eval1(&ss, doc, expr);
                assert_eq!(e("count(/r/text())"), "0", "{label}: r");
                assert_eq!(e("count(/r/eo/text())"), "0", "{label}: eo");
                assert_eq!(e("count(/r/eo/node())"), "4", "{label}: eo");
                assert_eq!(e("string(/r/eo)"), "1 ", "{label}");
                assert_eq!(e("count(/r/mx/text())"), "2", "{label}: mixed");
                assert_eq!(e("string(/r/s)"), " ", "{label}: xs:string");
                assert_eq!(e("count(/r/tk/node())"), "0", "{label}: xs:token");
                assert_eq!(e("count(/r/any/text())"), "0", "{label}: xsi:type");
                assert_eq!(e("count(/r/nl/node())"), "0", "{label}: nilled");
                assert_eq!(e("count(/r/w/text())"), "0", "{label}: w");
                assert_eq!(e("count(/r/w/x/text())"), "2", "{label}: skipped");
                // The navigator walks no white space either.
                let mut r = document_element(doc);
                assert!(r.move_to_first_child());
                assert_eq!(r.node_type(), DomNodeType::Element, "{label}");
                assert_eq!(r.local_name(), "eo", "{label}");
            });
        }
    }
}

/// White space next to other characters in element-only content is part of a
/// text node that is not all white space (the element is invalid,
/// cvc-complex-type.2.3): kept as read. White space in empty content makes the
/// element invalid (cvc-complex-type.2.1): kept as read.
#[test]
fn text_that_makes_element_only_content_invalid_is_kept() {
    for version in VERSIONS {
        let ss = compile(version, &[WHITESPACE]);
        for psvi in [true, false] {
            build(&ss, "<r><eo> x <a>1</a></eo></r>", psvi, |doc, validity| {
                assert_eq!(validity, Some(SchemaValidity::Invalid));
                assert_eq!(eval(&ss, doc, "/r/eo/text()"), [" x "]);
            });
            build(&ss, "<em> </em>", psvi, |doc, validity| {
                assert_eq!(validity, Some(SchemaValidity::Invalid));
                assert_eq!(eval(&ss, doc, "/em/text()"), [" "]);
            });
        }
    }
}

/// The decision is made as the text is read, by the governing type: an
/// element-only element found invalid only later (a required child missing)
/// has lost its white space already — consistent with the annotation the typed
/// build keeps on it, its declared element-only type.
#[test]
fn whitespace_of_an_element_only_element_invalid_for_another_reason_is_dropped() {
    for version in VERSIONS {
        let ss = compile(version, &[WHITESPACE]);
        build(&ss, "<req>\n  <a>1</a>\n</req>", true, |doc, validity| {
            assert_eq!(validity, Some(SchemaValidity::Invalid));
            assert_eq!(eval1(&ss, doc, "count(/req/text())"), "0");
        });
    }
}

/// XSD 1.1: the type a type alternative selects decides.
#[test]
fn whitespace_follows_the_type_a_type_alternative_selects() {
    let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
        <xs:complexType name="eo"><xs:sequence>
          <xs:element name="a" type="xs:string" minOccurs="0"/>
        </xs:sequence><xs:attribute name="k" type="xs:string"/></xs:complexType>
        <xs:complexType name="mixedT" mixed="true"><xs:sequence>
          <xs:element name="a" type="xs:string" minOccurs="0"/>
        </xs:sequence><xs:attribute name="k" type="xs:string"/></xs:complexType>
        <xs:element name="r"><xs:complexType><xs:sequence>
          <xs:element name="c" maxOccurs="unbounded">
            <xs:alternative test="@k = 'eo'" type="eo"/>
            <xs:alternative type="mixedT"/>
          </xs:element>
        </xs:sequence></xs:complexType></xs:element>
      </xs:schema>"#;
    let ss = compile(XsdVersion::V1_1, &[xsd]);
    for psvi in [true, false] {
        build(
            &ss,
            "<r><c k=\"eo\">\n <a/>\n</c><c k=\"m\">\n <a/>\n</c></r>",
            psvi,
            |doc, validity| {
                assert_eq!(validity, Some(SchemaValidity::Valid));
                assert_eq!(eval1(&ss, doc, "count(/r/c[1]/text())"), "0");
                assert_eq!(eval1(&ss, doc, "count(/r/c[2]/text())"), "2");
            },
        );
    }
}

/// The handler of a pass still sees every text event, the white space the
/// tree drops included.
#[test]
fn the_handler_still_sees_the_dropped_whitespace() {
    #[derive(Default)]
    struct Texts(RefCell<usize>);
    impl ValidationEventHandler for &Texts {
        type Error = std::convert::Infallible;
        fn on_text(&mut self, _kind: TextKind, _text: &str) -> Result<(), Self::Error> {
            *self.0.borrow_mut() += 1;
            Ok(())
        }
    }
    for version in VERSIONS {
        let ss = compile(version, &[WHITESPACE]);
        let validator = SchemaValidator::new(&ss, ValidationFlags::default());
        let mut runtime = validator.start_run(SilentValidationSink);
        let arena = Bump::new();
        let mut builder =
            BufferDocumentBuilder::new(&arena, &ss.name_table, Some(&ss), Default::default())
                .unwrap();
        let texts = Texts::default();
        {
            let mut handler = TypedBuilderHandler::with_handler(&mut builder, &texts);
            drive_quick_xml_with(
                "<r>\n<eo>\n<a>1</a>\n</eo>\n</r>".as_bytes(),
                &mut runtime,
                &ss,
                &mut handler,
            )
            .expect("the pass completes");
        }
        assert_eq!(*texts.0.borrow(), 5, "{version:?}");
        let doc = builder.finalize().unwrap();
        assert_eq!(eval1(&ss, &doc, "count(//text())"), "1");
    }
}

// ── C79: the typed value of a union-typed node ────────────────────────

const UNIONS: &str = r###"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:simpleType name="Std"><xs:restriction base="xs:string">
    <xs:pattern value="\d{4}-[A-Z]{3}"/>
  </xs:restriction></xs:simpleType>
  <xs:simpleType name="Gen"><xs:union memberTypes="Std xs:string"/></xs:simpleType>
  <xs:simpleType name="Small"><xs:restriction base="xs:integer">
    <xs:maxInclusive value="99"/>
  </xs:restriction></xs:simpleType>
  <xs:simpleType name="IntOrStr"><xs:union memberTypes="Small xs:string"/></xs:simpleType>
  <xs:simpleType name="Gens"><xs:list itemType="Gen"/></xs:simpleType>
  <xs:complexType name="GenSC"><xs:simpleContent><xs:extension base="Gen">
    <xs:attribute name="g" type="Gen"/>
  </xs:extension></xs:simpleContent></xs:complexType>
  <xs:element name="r"><xs:complexType><xs:sequence>
    <xs:element name="e" type="Gen" maxOccurs="unbounded"/>
    <xs:element name="sc" type="GenSC"/>
    <xs:element name="n" type="IntOrStr" maxOccurs="unbounded"/>
    <xs:element name="l" type="Gens"/>
  </xs:sequence><xs:attribute name="a" type="Gen"/></xs:complexType></xs:element>
</xs:schema>"###;

const UNION_DOC: &str = r#"<r a="1917-MAY"><e>1917-MAY</e><e>sometime</e><sc g="2000-JUN">1990-MAR</sc><n>42</n><n>x</n><l>1917-MAY foo 2000-JAN</l></r>"#;

/// `data($n)` of a union-typed element, attribute or simple-content element
/// is an instance of the member type that validated it — a user-defined
/// restriction included; for a list of a union, each member is.
#[test]
fn the_typed_value_of_a_union_typed_node_is_of_its_member_type() {
    for version in VERSIONS {
        let ss = compile(version, &[UNIONS]);
        for psvi in [true, false] {
            build(&ss, UNION_DOC, psvi, |doc, validity| {
                let label = format!("{version:?} psvi={psvi}");
                assert_eq!(validity, Some(SchemaValidity::Valid), "{label}");
                let e = |expr: &str| try_eval1(&ss, doc, expr);
                let cases = [
                    ("data(/r/e[1]) instance of Std", "true"),
                    ("data(/r/e[1]) instance of xs:string", "true"),
                    ("data(/r/e[2]) instance of Std", "false"),
                    ("data(/r/e[2]) instance of xs:string", "true"),
                    ("data(/r/@a) instance of Std", "true"),
                    ("data(/r/sc) instance of Std", "true"),
                    ("data(/r/sc/@g) instance of Std", "true"),
                    ("data(/r/n[1]) instance of Small", "true"),
                    ("data(/r/n[1]) instance of xs:integer", "true"),
                    ("data(/r/n[1]) + 1", "43"),
                    ("data(/r/n[2]) instance of xs:integer", "false"),
                    ("data(/r/n[2]) instance of xs:string", "true"),
                    ("count(data(/r/l))", "3"),
                    ("data(/r/l)[1] instance of Std", "true"),
                    ("data(/r/l)[2] instance of Std", "false"),
                    ("data(/r/l)[3] instance of Std", "true"),
                    ("(data(/r/e[1]) treat as Std) instance of Std", "true"),
                    ("count(/r/e[data(.) instance of Std])", "1"),
                    // The cast path is unchanged.
                    ("xs:string(/r/e[1])", "1917-MAY"),
                    ("/r/n[1] cast as xs:integer", "42"),
                    ("data(/r/e[1]) cast as xs:string", "1917-MAY"),
                    ("/r/e[2] castable as Std", "false"),
                    ("/r/e[1] castable as Std", "true"),
                ];
                let wrong: Vec<_> = cases
                    .into_iter()
                    .filter_map(|(expr, expected)| {
                        let got = e(expr);
                        (got != expected).then(|| format!("{expr} = {got}"))
                    })
                    .collect();
                assert!(wrong.is_empty(), "{label}: {wrong:#?}");
            });
        }
    }
}

/// The navigator's typed value keeps its form — the union's value holding the
/// member type's — and the streaming PSVI of the same pass names the same
/// member type.
#[test]
fn the_navigator_and_the_psvi_agree_on_the_member_type() {
    #[derive(Default)]
    struct Members(RefCell<Vec<Option<TypeKey>>>);
    impl ValidationEventHandler for &Members {
        type Error = std::convert::Infallible;
        fn after_attribute(
            &mut self,
            _view: AttributeView<'_>,
            info: &SchemaInfo,
        ) -> Result<(), Self::Error> {
            self.0.borrow_mut().push(info.member_type);
            Ok(())
        }
    }
    for version in VERSIONS {
        let ss = compile(version, &[UNIONS]);
        let std_key = ss
            .lookup_type(None, ss.name_table.get("Std").expect("interned"))
            .expect("Std");
        for psvi in [true, false] {
            let label = format!("{version:?} psvi={psvi}");
            let validator = SchemaValidator::new(&ss, flags(psvi));
            let mut runtime = validator.start_run(SilentValidationSink);
            let arena = Bump::new();
            let mut builder =
                BufferDocumentBuilder::new(&arena, &ss.name_table, Some(&ss), Default::default())
                    .unwrap();
            let members = Members::default();
            {
                let mut handler = TypedBuilderHandler::with_handler(&mut builder, &members);
                drive_quick_xml_with(UNION_DOC.as_bytes(), &mut runtime, &ss, &mut handler)
                    .expect("the pass completes");
            }
            let doc = builder.finalize().unwrap();
            assert_eq!(
                members.0.borrow().first().copied().flatten(),
                Some(std_key),
                "{label}: @a's member type"
            );
            let mut a = document_element(&doc);
            assert!(a.move_to_first_attribute());
            match a.typed_value() {
                TypedValue::Value(v) => match &v.value {
                    XmlValueKind::Union(member) => {
                        assert_eq!(member.schema_type.map(TypeKey::Simple), Some(std_key))
                    }
                    other => panic!("{label}: a union's value expected, got {other:?}"),
                },
                other => panic!("{label}: {other:?}"),
            }
        }
    }
}
