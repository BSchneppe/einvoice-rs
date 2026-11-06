//! Element ids, the is-idrefs property and `fn:idref` on typed trees.
//!
//! XDM 1.0 §6.2.4 (element is-id): "If the element has a complex type with
//! element-only content, the is-id property is false. Otherwise, if the
//! typed-value of the element consists of exactly one atomic value and that
//! value is of type xs:ID, or a type derived from xs:ID, the is-id property is
//! true"; F&O 1.0 §15.5.2 `fn:id` selects an element when "The is-id property
//! … of the element node is true, and the typed value of the element node is
//! equal to V". XDM §6.2.4 / §6.3.4 (is-idrefs): "if any of the atomic values
//! in the typed-value of the element [attribute] is of type xs:IDREF or
//! xs:IDREFS, or a type derived from one of those types, the is-idrefs
//! property is true". F&O 1.0 §15.5.3 `fn:idref` returns "every element or
//! attribute node $N" whose is-idrefs property is true and whose
//! `fn:tokenize(fn:normalize-space(fn:string($N)), ' ')` contains one of the
//! candidate ID values, "in document order with duplicates eliminated".
//!
//! Every typed case runs in XSD 1.0 and 1.1 mode, with and without PSVI typed
//! values.
#![cfg(feature = "xsd11")]

use bumpalo::Bump;
use xsd_schema::document::{
    build_typed_document_with, Annotations, BufferDocNavigator, BufferDocument,
    BufferDocumentBuilder, BufferDocumentOptions, CopyOptions, NodeSchemaBinding,
    SilentValidationSink, TypedBuildOptions,
};
use xsd_schema::namespace::context::NamespaceContextSnapshot;
use xsd_schema::namespace::NameTable;
use xsd_schema::navigator::{DomNavigator, DomNodeType};
use xsd_schema::validation::{NoopHandler, SchemaValidator, SchemaValidity, ValidationFlags};
use xsd_schema::xpath::api::XPathExpr;
use xsd_schema::xpath::{RoXmlNavigator, XPathContext, XmlItem};
use xsd_schema::{SchemaSet, SchemaSetBuilder, TypeKey, XsdVersion};

const VERSIONS: [XsdVersion; 2] = [XsdVersion::V1_0, XsdVersion::V1_1];

fn compile(version: XsdVersion, xsd: &str) -> SchemaSet {
    SchemaSetBuilder::with_version(version)
        .add_source(xsd, "file:///id_idrefs.xsd")
        .expect("schema source accepted")
        .compile()
        .unwrap_or_else(|e| panic!("{version:?}: {e}"))
        .into_schema_set()
}

fn build<F>(ss: &SchemaSet, xml: &str, psvi: bool, check: F)
where
    F: FnOnce(&BufferDocument<'_>, Option<SchemaValidity>),
{
    let flags = if psvi {
        ValidationFlags::default()
    } else {
        ValidationFlags::default() & !ValidationFlags::BUILD_PSVI_TYPED_VALUES
    };
    let validator = SchemaValidator::new(ss, flags);
    let mut runtime = validator.start_run(SilentValidationSink);
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

/// Each item of `expr`'s result over the navigator `nav` as a string: a node
/// as `name()` (`@name` for an attribute), an atomic value as its string.
fn eval_at<N: DomNavigator>(
    names: &NameTable,
    ss: Option<&SchemaSet>,
    nav: N,
    expr: &str,
) -> Result<Vec<String>, String> {
    let mut ns = NamespaceContextSnapshot::default();
    ns.bindings.push((
        names.add("xs"),
        names.add("http://www.w3.org/2001/XMLSchema"),
    ));
    let ctx = XPathContext::new(names).with_namespaces(ns);
    let ctx = match ss {
        Some(ss) => ctx.with_schema_set(ss),
        None => ctx,
    };
    let compiled = XPathExpr::compile(expr, &ctx).map_err(|e| e.to_string())?;
    let value = compiled
        .evaluator(&ctx)
        .run_with_node(nav)
        .map_err(|e| e.to_string())?;
    Ok(value
        .into_vec()
        .iter()
        .map(|item| match item {
            XmlItem::Node(n) if n.node_type() == DomNodeType::Attribute => {
                format!("@{}", n.name())
            }
            XmlItem::Node(n) => n.name().to_string(),
            XmlItem::Atomic(v) => v.to_string_value(),
        })
        .collect())
}

fn eval(ss: &SchemaSet, doc: &BufferDocument<'_>, expr: &str) -> Vec<String> {
    eval_at(&ss.name_table, Some(ss), doc.create_navigator(), expr)
        .unwrap_or_else(|e| panic!("{expr}: {e}"))
}

// ── C62: element ids ──────────────────────────────────────────────────

const IDS: &str = r###"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:simpleType name="ids"><xs:list itemType="xs:ID"/></xs:simpleType>
  <xs:simpleType name="intOrId"><xs:union memberTypes="xs:integer xs:ID"/></xs:simpleType>
  <xs:simpleType name="pid"><xs:restriction base="xs:ID">
    <xs:pattern value="P-.*"/>
  </xs:restriction></xs:simpleType>
  <xs:element name="r"><xs:complexType><xs:sequence>
    <xs:element name="ide" type="xs:ID" nillable="true" maxOccurs="unbounded"/>
    <xs:element name="idsc" maxOccurs="unbounded"><xs:complexType><xs:simpleContent>
      <xs:extension base="xs:ID"><xs:attribute name="aid" type="xs:ID"/></xs:extension>
    </xs:simpleContent></xs:complexType></xs:element>
    <xs:element name="idl" type="ids" maxOccurs="unbounded"/>
    <xs:element name="u" type="intOrId" maxOccurs="unbounded"/>
    <xs:element name="pat" type="pid"/>
    <xs:element name="eo"><xs:complexType><xs:sequence>
      <xs:element name="s" type="xs:string"/>
    </xs:sequence></xs:complexType></xs:element>
  </xs:sequence></xs:complexType></xs:element>
</xs:schema>"###;

const ID_DOC: &str = "<r xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\
    <ide>e1</ide><ide xsi:nil=\"true\"/><ide> e2 </ide>\
    <idsc aid=\"a1\">s1</idsc>\
    <idl>l1</idl><idl>l2 l3</idl>\
    <u>12</u><u>k1</u>\
    <pat>P-1</pat>\
    <eo><s>notanid</s></eo></r>";

/// An element whose typed value is a single `xs:ID` — of a simple type, of a
/// complex type with simple content, a list of one `xs:ID`, a union's `xs:ID`
/// member, a restriction of `xs:ID` — is found by `fn:id`; a nilled element, a
/// list of two, a union's other member and element-only content are not.
#[test]
fn an_element_whose_typed_value_is_an_id_is_found_by_id() {
    for version in VERSIONS {
        let ss = compile(version, IDS);
        for psvi in [true, false] {
            build(&ss, ID_DOC, psvi, |doc, validity| {
                let label = format!("{version:?} psvi={psvi}");
                assert_eq!(validity, Some(SchemaValidity::Valid), "{label}");
                for (id, expected) in [
                    ("e1", vec!["ide"]),
                    ("e2", vec!["ide"]),
                    ("s1", vec!["idsc"]),
                    ("a1", vec!["idsc"]),
                    ("l1", vec!["idl"]),
                    ("l2", vec![]),
                    ("12", vec![]),
                    ("k1", vec!["u"]),
                    ("P-1", vec!["pat"]),
                    ("notanid", vec![]),
                ] {
                    assert_eq!(
                        eval(&ss, doc, &format!("id('{id}')")),
                        expected,
                        "{label}: id('{id}')"
                    );
                }
                assert_eq!(
                    eval(&ss, doc, "count(id('e1 e2 s1 a1')) , id('e2')/string()"),
                    ["3", "e2"],
                    "{label}"
                );
                let nav = doc.create_navigator();
                let found = nav.find_element_by_id("k1").unwrap().expect("k1");
                assert_eq!(found.local_name(), "u", "{label}");
                assert!(doc.get_element_by_id("P-1").is_some(), "{label}");
            });
        }
    }
}

/// A copy that keeps the annotations files the element ids of the copied
/// elements, as it files their attribute ids.
#[test]
fn a_copy_with_annotations_keeps_element_ids() {
    for version in VERSIONS {
        let ss = compile(version, IDS);
        build(&ss, ID_DOC, true, |source, _| {
            let mut r = source.create_navigator();
            assert!(r.move_to_first_child());
            let arena = Bump::new();
            let mut builder = BufferDocumentBuilder::new(
                &arena,
                &ss.name_table,
                Some(&ss),
                BufferDocumentOptions::default(),
            )
            .unwrap();
            builder.start_element("w", "", "", &[]).unwrap();
            builder.end_of_attributes();
            builder
                .copy_subtree(
                    &r,
                    CopyOptions {
                        annotations: Annotations::Preserve,
                        ..CopyOptions::default()
                    },
                )
                .unwrap();
            builder.end_element().unwrap();
            let copy = builder.finalize().unwrap();
            assert_eq!(
                eval(&ss, &copy, "id('e1 s1 a1 k1')/name()"),
                ["ide", "idsc", "u"]
            );
            assert_eq!(eval(&ss, &copy, "id('l2')"), Vec::<String>::new());
        });
    }
}

/// A host that builds a tree and binds an element itself gets the same: the
/// element is filed when it ends, from the text it holds then.
#[test]
fn a_host_bound_element_is_filed_when_it_ends() {
    let ss = compile(XsdVersion::V1_1, IDS);
    let id = match ss
        .lookup_type(None, ss.name_table.get("pid").expect("interned"))
        .expect("pid")
    {
        TypeKey::Simple(k) => k,
        TypeKey::Complex(_) => unreachable!(),
    };
    let arena = Bump::new();
    let mut builder =
        BufferDocumentBuilder::new(&arena, &ss.name_table, Some(&ss), Default::default()).unwrap();
    builder.start_element("doc", "", "", &[]).unwrap();
    builder.end_of_attributes();
    let e = builder.start_element("x", "", "", &[]).unwrap();
    builder
        .set_node_binding(
            e,
            NodeSchemaBinding {
                type_key: TypeKey::Simple(id),
                element_decl: None,
                attribute_decl: None,
                content_type: None,
            },
        )
        .unwrap();
    builder.end_of_attributes();
    builder.text("P-");
    builder.text("7");
    builder.end_element().unwrap();
    builder.end_element().unwrap();
    let doc = builder.finalize().unwrap();
    assert_eq!(eval(&ss, &doc, "id('P-7')/name()"), ["x"]);
}

// ── C62: is-idrefs ────────────────────────────────────────────────────

const IDREFS: &str = r###"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:simpleType name="refOrInt"><xs:union memberTypes="xs:integer xs:IDREF"/></xs:simpleType>
  <xs:simpleType name="mixedRefs"><xs:list itemType="refOrInt"/></xs:simpleType>
  <xs:simpleType name="myref"><xs:restriction base="xs:IDREF">
    <xs:pattern value="[a-z]+\d"/>
  </xs:restriction></xs:simpleType>
  <xs:element name="r"><xs:complexType><xs:sequence>
    <xs:element name="item" maxOccurs="unbounded"><xs:complexType><xs:simpleContent>
      <xs:extension base="xs:ID">
        <xs:attribute name="ref" type="xs:IDREF"/>
        <xs:attribute name="refs" type="xs:IDREFS"/>
        <xs:attribute name="u" type="refOrInt"/>
        <xs:attribute name="m" type="mixedRefs"/>
        <xs:attribute name="s" type="xs:string"/>
        <xs:attribute name="nc" type="xs:NCName"/>
      </xs:extension>
    </xs:simpleContent></xs:complexType></xs:element>
    <xs:element name="eref" type="xs:IDREF" maxOccurs="unbounded"/>
    <xs:element name="erefs" type="xs:IDREFS" maxOccurs="unbounded"/>
    <xs:element name="my" type="myref" maxOccurs="unbounded"/>
    <xs:element name="scrs" maxOccurs="unbounded"><xs:complexType><xs:simpleContent>
      <xs:extension base="xs:IDREFS"/>
    </xs:simpleContent></xs:complexType></xs:element>
  </xs:sequence></xs:complexType></xs:element>
</xs:schema>"###;

const IDREF_DOC: &str = "<r>\
    <item ref=\"b1\" refs=\"a1 b1\" u=\"a1\" m=\"5 b1\" s=\"a1\" nc=\"a1\">a1</item>\
    <item u=\"5\" m=\"5 6\">b1</item>\
    <eref>a1</eref><erefs> b1  a1 </erefs><my>b1</my><scrs>a1</scrs></r>";

/// Which nodes have the is-idrefs property: an attribute or element whose
/// typed value has an `xs:IDREF` (or derived) atomic value — `xs:IDREF`,
/// `xs:IDREFS`, a restriction, a union's `xs:IDREF` member, a list of such a
/// union holding one; not a union's other member, `xs:string`, `xs:NCName`.
#[test]
fn the_is_idrefs_property() {
    for version in VERSIONS {
        let ss = compile(version, IDREFS);
        for psvi in [true, false] {
            build(&ss, IDREF_DOC, psvi, |doc, validity| {
                let label = format!("{version:?} psvi={psvi}");
                assert_eq!(validity, Some(SchemaValidity::Valid), "{label}");
                let mut found = Vec::new();
                let mut nav = doc.create_navigator();
                collect_idrefs(&mut nav, &mut found);
                assert_eq!(
                    found,
                    [
                        "item[1]/@ref",
                        "item[1]/@refs",
                        "item[1]/@u",
                        "item[1]/@m",
                        "eref",
                        "erefs",
                        "my",
                        "scrs",
                    ],
                    "{label}"
                );
            });
        }
    }
}

/// Walk the tree in document order, naming each element and attribute with
/// the is-idrefs property (`item[1]/@ref` for an attribute).
fn collect_idrefs(nav: &mut BufferDocNavigator<'_>, out: &mut Vec<String>) {
    fn visit(nav: &BufferDocNavigator<'_>, position: &mut [usize; 2], out: &mut Vec<String>) {
        let mut child = nav.clone();
        if !child.move_to_first_child() {
            return;
        }
        loop {
            if child.node_type() == DomNodeType::Element {
                let name = child.local_name().to_string();
                let label = if name == "item" {
                    position[0] += 1;
                    format!("item[{}]", position[0])
                } else {
                    name
                };
                if child.is_idrefs() {
                    out.push(label.clone());
                }
                let mut attr = child.clone();
                if attr.move_to_first_attribute() {
                    loop {
                        if attr.is_idrefs() {
                            out.push(format!("{label}/@{}", attr.local_name()));
                        }
                        if !attr.move_to_next_attribute() {
                            break;
                        }
                    }
                }
                visit(&child, position, out);
            }
            if !child.move_to_next_sibling() {
                break;
            }
        }
    }
    let mut position = [0, 0];
    visit(nav, &mut position, out);
}

// ── C54: fn:idref ─────────────────────────────────────────────────────

/// F&O 1.0 §15.5.3: the is-idrefs elements and attributes whose string value
/// has the candidate as a token, in document order, each once; candidates that
/// are not NCNames are ignored; the second argument names the tree.
#[test]
fn idref_returns_the_referring_nodes() {
    for version in VERSIONS {
        let ss = compile(version, IDREFS);
        for psvi in [true, false] {
            build(&ss, IDREF_DOC, psvi, |doc, _| {
                let label = format!("{version:?} psvi={psvi}");
                for (expr, expected) in [
                    ("idref('a1')", vec!["@refs", "@u", "eref", "erefs", "scrs"]),
                    ("idref('b1')", vec!["@ref", "@refs", "@m", "erefs", "my"]),
                    (
                        "idref(('b1', 'a1'))",
                        vec!["@ref", "@refs", "@u", "@m", "eref", "erefs", "my", "scrs"],
                    ),
                    (
                        "idref('a1', /r/item[2])",
                        vec!["@refs", "@u", "eref", "erefs", "scrs"],
                    ),
                    ("idref('5')", vec![]),
                    ("idref('a1 b1')", vec![]),
                    ("idref(())", vec![]),
                    ("idref('zz')", vec![]),
                    ("idref('a1')/..", vec!["r", "item"]),
                    (
                        "/r/item[1]/idref('b1')/string()",
                        vec!["b1", "a1 b1", "5 b1", "b1 a1", "b1"],
                    ),
                ] {
                    assert_eq!(eval(&ss, doc, expr), expected, "{label}: {expr}");
                }
            });
        }
    }
}

/// The errors F&O 1.0 §15.5.3 names: no context item (XPDY0002), a context
/// item that is not a node (XPTY0004), a tree whose root is not a document
/// node (FODC0001).
#[test]
fn idref_errors() {
    let ss = compile(XsdVersion::V1_1, IDREFS);
    build(&ss, IDREF_DOC, true, |doc, _| {
        let names = &ss.name_table;
        let ctx = XPathContext::new(names).with_schema_set(&ss);
        let err = XPathExpr::compile("idref('a1')", &ctx)
            .unwrap()
            .evaluator(&ctx)
            .run::<BufferDocNavigator<'_>>()
            .err()
            .expect("no context item is an error")
            .to_string();
        assert!(err.contains("XPDY0002"), "{err}");
        let err =
            eval_at(names, Some(&ss), doc.create_navigator(), "(1)[idref('a1')]").unwrap_err();
        assert!(err.contains("XPTY0004"), "{err}");
        let mut item = doc.create_navigator();
        assert!(item.move_to_first_child() && item.move_to_first_child());
        let orphan = BufferDocNavigator::new_orphan(doc, item.current_ref());
        let err = eval_at(names, Some(&ss), orphan, "idref('a1')").unwrap_err();
        assert!(err.contains("FODC0001"), "{err}");
    });
}

/// A tree without annotations has no is-idrefs node: `fn:idref` is empty — in
/// a buffer document and in a schema-unaware navigator alike (a basic
/// processor offers the function too).
#[test]
fn idref_over_an_untyped_tree_is_empty() {
    let names = NameTable::new();
    let arena = Bump::new();
    let doc = BufferDocument::from_reader_default(
        r#"<r><a ref="x"/><x xml:id="x"/></r>"#.as_bytes(),
        &arena,
        &names,
    )
    .unwrap();
    assert_eq!(
        eval_at(
            &names,
            None,
            doc.create_navigator(),
            "count(idref('x')), count(id('x'))"
        ),
        Ok(vec!["0".to_string(), "1".to_string()])
    );
    let parsed = roxmltree::Document::parse(r#"<r><a ref="x"/></r>"#).unwrap();
    assert_eq!(
        eval_at(
            &names,
            None,
            RoXmlNavigator::new(&parsed),
            "count(idref('x'))"
        ),
        Ok(vec!["0".to_string()])
    );
}

/// A copy that keeps the annotations keeps is-idrefs; a host that copies a
/// tree untyped keeps it with `register_idrefs`, as it keeps an id with
/// `register_xml_id`.
#[test]
fn is_idrefs_is_kept_by_copies() {
    let ss = compile(XsdVersion::V1_1, IDREFS);
    build(&ss, IDREF_DOC, true, |source, _| {
        let mut r = source.create_navigator();
        assert!(r.move_to_first_child());
        let arena = Bump::new();
        let mut builder =
            BufferDocumentBuilder::new(&arena, &ss.name_table, Some(&ss), Default::default())
                .unwrap();
        builder.start_element("w", "", "", &[]).unwrap();
        builder.end_of_attributes();
        builder
            .copy_subtree(
                &r,
                CopyOptions {
                    annotations: Annotations::Preserve,
                    ..CopyOptions::default()
                },
            )
            .unwrap();
        builder.end_element().unwrap();
        let copy = builder.finalize().unwrap();
        assert_eq!(
            eval(&ss, &copy, "idref('a1')/name()"),
            ["refs", "u", "eref", "erefs", "scrs"]
        );
    });

    // Untyped, kept by the host.
    let arena = Bump::new();
    let mut builder =
        BufferDocumentBuilder::new(&arena, &ss.name_table, Some(&ss), Default::default()).unwrap();
    builder.start_element("doc", "", "", &[]).unwrap();
    builder.end_of_attributes();
    builder.start_element("a", "", "", &[]).unwrap();
    let attr = builder.attribute("ref", "", "", " t1  t2 ").unwrap();
    builder.register_idrefs(attr);
    builder.end_of_attributes();
    builder.end_element().unwrap();
    let elem = builder.start_element("e", "", "", &[]).unwrap();
    builder.end_of_attributes();
    builder.text("t2");
    builder.register_idrefs(elem);
    builder.end_element().unwrap();
    builder.end_element().unwrap();
    let doc = builder.finalize().unwrap();
    assert_eq!(eval(&ss, &doc, "idref('t2')/name()"), ["ref", "e"]);
    let mut a = doc.create_navigator();
    assert!(a.move_to_first_child() && a.move_to_first_child());
    assert!(a.move_to_first_attribute());
    assert!(a.is_idrefs());
}
