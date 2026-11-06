//! Namespace-sensitive values in typed trees (Wave B batch (t), fix round 1):
//! the `xs:QName` / `xs:NOTATION` value of a value constraint, and the union
//! member that decides whether a node is an id or has the is-idrefs property.
//!
//! Datatypes §3.3.18: "The mapping from lexical space to value space for a
//! particular QName ·literal· depends on the namespace bindings in scope where
//! the literal occurs. When QNames appear in an XML context, the bindings to be
//! used in the ·lexical mapping· are those in the [in-scope namespaces]
//! property of the relevant element", and "If the host language does not
//! specify otherwise, unqualified names are bound to the default namespace".
//! The literal of a `default` / `fixed` value is written on a declaration of
//! a schema document: Structures §3.2.2 / §3.3.2 map it to a Value Constraint
//! whose {value} is "the ·actual value· (with respect to the {type
//! definition}) of the [attribute]" and whose {lexical form} is its
//! "·normalized value·"; a defaulted attribute's [schema actual value] is "The
//! ·effective value constraint·'s {value}" (§3.4.5.1). Its Note: "When a
//! default value of type QName or NOTATION is applied, it is
//! ·implementation-dependent· whether ·namespace fixup· occurs; if it does
//! not, the prefix used in the lexical representation … will not necessarily
//! map to the namespace name of the value". This build performs the fixup: the
//! node's string value is a lexical form that resolves, in the instance, to
//! the schema's expanded name.
//!
//! The id index and the idrefs index classify a node by its typed value, which
//! for a union is the value of the member that validated it — with the node's
//! in-scope namespaces, as validation and the navigator decide it.
//!
//! Every case runs in XSD 1.0 and 1.1 mode, with and without PSVI typed values.
#![cfg(feature = "xsd11")]

use std::cell::RefCell;

use bumpalo::Bump;
use xsd_schema::document::{
    build_typed_document_with, Annotations, BufferDocNavigator, BufferDocument,
    BufferDocumentBuilder, BufferDocumentOptions, CopyOptions, SilentValidationSink,
    TypedBuildOptions, TypedBuilderHandler,
};
use xsd_schema::namespace::context::NamespaceContextSnapshot;
use xsd_schema::navigator::{DomNavigator, DomNodeType};
use xsd_schema::types::value::{XmlAtomicValue, XmlValue, XmlValueKind};
use xsd_schema::validation::{
    drive_quick_xml_with, CollectingValidationSink, EndOfAttributesView, NoopHandler,
    SchemaValidator, SchemaValidity, ValidationEventHandler, ValidationFlags,
};
use xsd_schema::xpath::api::XPathExpr;
use xsd_schema::xpath::{XPathContext, XmlItem};
use xsd_schema::{SchemaSet, SchemaSetBuilder, XsdVersion};

const VERSIONS: [XsdVersion; 2] = [XsdVersion::V1_0, XsdVersion::V1_1];

fn compile(version: XsdVersion, xsd: &str) -> SchemaSet {
    SchemaSetBuilder::with_version(version)
        .add_source(xsd, "file:///namespace_sensitive_values.xsd")
        .expect("schema source accepted")
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

/// Build `xml` typed; hand the document, the root's validity and the
/// validation errors' constraint codes to `check`.
fn build<F>(ss: &SchemaSet, xml: &str, psvi: bool, check: F)
where
    F: FnOnce(&BufferDocument<'_>, Option<SchemaValidity>, Vec<String>),
{
    let validator = SchemaValidator::new(ss, flags(psvi));
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    let arena = Bump::new();
    let built = {
        let mut runtime = validator.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        let built = build_typed_document_with(
            xml.as_bytes(),
            &arena,
            ss,
            &mut runtime,
            &mut NoopHandler,
            TypedBuildOptions::default().with_document_options(BufferDocumentOptions::full()),
        )
        .expect("the build completes");
        runtime.end_validation().expect("no operational failure");
        built
    };
    let codes = errors.iter().map(|e| e.constraint.to_string()).collect();
    check(&built.document, built.outcome.root_validity, codes);
}

/// Each item of `expr`'s result over `doc` as a string (an attribute node as
/// `@name`, another node as its name, an atomic value as its string), or the
/// error. `xs` and `t` (`urn:t`) are bound.
fn eval(ss: &SchemaSet, doc: &BufferDocument<'_>, expr: &str) -> Vec<String> {
    let mut ns = NamespaceContextSnapshot::default();
    for (prefix, uri) in [("xs", "http://www.w3.org/2001/XMLSchema"), ("t", "urn:t")] {
        ns.bindings
            .push((ss.name_table.add(prefix), ss.name_table.add(uri)));
    }
    let ctx = XPathContext::new(&ss.name_table)
        .with_schema_set(ss)
        .with_namespaces(ns);
    let compiled = XPathExpr::compile(expr, &ctx).unwrap_or_else(|e| panic!("{expr}: {e}"));
    match compiled
        .evaluator(&ctx)
        .run_with_node(doc.create_navigator())
    {
        Ok(value) => value
            .into_vec()
            .iter()
            .map(|item| match item {
                XmlItem::Node(n) if n.node_type() == DomNodeType::Attribute => {
                    format!("@{}", n.name())
                }
                XmlItem::Node(n) => n.name().to_string(),
                XmlItem::Atomic(v) => v.to_string_value(),
            })
            .collect(),
        Err(e) => vec![format!("error {e}")],
    }
}

fn eval1(ss: &SchemaSet, doc: &BufferDocument<'_>, expr: &str) -> String {
    eval(ss, doc, expr).join(" | ")
}

/// `{namespace}local` of an `xs:QName` / `xs:NOTATION` value (a union's
/// member value looked through), or `?` for anything else.
fn expanded(ss: &SchemaSet, value: &XmlValue) -> String {
    match &value.value {
        XmlValueKind::Atomic(XmlAtomicValue::QName(qn) | XmlAtomicValue::Notation(qn)) => {
            let name = qn.name();
            let ns = name
                .namespace_uri
                .map_or("", |n| ss.name_table.resolve_ref(n));
            format!("{{{ns}}}{}", ss.name_table.resolve_ref(name.local_name))
        }
        XmlValueKind::Union(member) => expanded(ss, member),
        XmlValueKind::List { items, .. } => items
            .iter()
            .map(|item| expanded(ss, item))
            .collect::<Vec<_>>()
            .join(" "),
        _ => "?".to_string(),
    }
}

// ── 1. QName / NOTATION values of value constraints ──────────────────

const QNAMES: &str = r###"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
    xmlns="urn:t" xmlns:t="urn:t" xmlns:p="urn:p" targetNamespace="urn:t"
    elementFormDefault="qualified">
  <xs:notation name="gif" public="image/gif"/>
  <xs:simpleType name="fmt"><xs:restriction base="xs:NOTATION">
    <xs:enumeration value="gif"/>
  </xs:restriction></xs:simpleType>
  <xs:simpleType name="qnames"><xs:list itemType="xs:QName"/></xs:simpleType>
  <xs:simpleType name="intOrQName"><xs:union memberTypes="xs:integer xs:QName"/></xs:simpleType>
  <xs:element name="r"><xs:complexType>
    <xs:sequence>
      <xs:element name="qe" type="xs:QName" default="p:value" minOccurs="0"/>
      <xs:element name="qf" type="xs:QName" fixed="p:value" minOccurs="0"/>
      <xs:element name="qd" type="xs:QName" default="value" minOccurs="0"/>
    </xs:sequence>
    <xs:attribute name="qa" type="xs:QName" default="p:value"/>
    <xs:attribute name="qd" type="xs:QName" default="value"/>
    <xs:attribute name="qn" type="xs:QName" default="value" xmlns=""/>
    <xs:attribute name="ql" type="qnames" default="p:a value"/>
    <xs:attribute name="qu" type="intOrQName" default="p:u"/>
    <xs:attribute name="nt" type="fmt" default="gif"/>
  </xs:complexType></xs:element>
</xs:schema>"###;

/// The expanded names the schema gives the defaults: `p:value` with `p` bound
/// to `urn:p` on the schema element; `value` in the schema document's default
/// namespace `urn:t`; `value` on a declaration that undeclares it (no
/// namespace).
const QA: &str = "{urn:p}value";
const QD: &str = "{urn:t}value";
const QN: &str = "{}value";

/// What each node of an instance must hold: `(XPath to the node, string
/// value, expanded name(s) of its typed value)`.
fn check_qname_nodes(
    ss: &SchemaSet,
    doc: &BufferDocument<'_>,
    label: &str,
    expected: &[(&str, &str, &str)],
) {
    let mut wrong = Vec::new();
    for (path, string, names) in expected {
        let got_string = eval1(ss, doc, &format!("string({path})"));
        let got_names = eval1(
            ss,
            doc,
            &format!(
                "string-join(for $q in data({path}) return \
                 concat('{{', namespace-uri-from-QName($q), '}}', local-name-from-QName($q)), ' ')"
            ),
        );
        if got_string != *string || (!names.is_empty() && got_names != *names) {
            wrong.push(format!(
                "{path}: string {got_string:?} (want {string:?}), value {got_names:?} (want {names:?})"
            ));
        }
    }
    assert!(wrong.is_empty(), "{label}: {wrong:#?}");
}

/// The reviewer's case: `p:value` written in the schema with `p` bound to
/// `urn:p`, applied where the instance binds `p` to another namespace — the
/// node holds `{urn:p}value`, written with a prefix the instance binds to
/// `urn:p` (declared on the element when none is). The same for every QName of
/// a list, a union's QName member, element defaults and fixed values. A name
/// the schema puts in its default namespace keeps no prefix where the
/// instance's default namespace is the same.
#[test]
fn a_qname_default_keeps_the_schemas_expanded_name_where_the_instance_rebinds_its_prefix() {
    for version in VERSIONS {
        let ss = compile(version, QNAMES);
        for psvi in [true, false] {
            let label = format!("{version:?} psvi={psvi}");
            build(
                &ss,
                r#"<r xmlns="urn:t" xmlns:p="urn:other"><qe/><qf/><qd/></r>"#,
                psvi,
                |doc, validity, errors| {
                    assert_eq!(validity, Some(SchemaValidity::Valid), "{label}: {errors:?}");
                    let ns0 = eval1(&ss, doc, "namespace-uri-for-prefix('ns0', /*)");
                    assert_eq!(ns0, "urn:p", "{label}: the fixup prefix");
                    check_qname_nodes(
                        &ss,
                        doc,
                        &label,
                        &[
                            ("/*/@qa", "ns0:value", QA),
                            ("/*/@qd", "value", QD),
                            ("/*/@ql", "ns0:a value", "{urn:p}a {urn:t}value"),
                            ("/*/@qu", "ns0:u", "{urn:p}u"),
                            ("/*/@nt", "gif", ""),
                            ("/*/t:qe", "ns0:value", QA),
                            ("/*/t:qf", "ns0:value", QA),
                            ("/*/t:qd", "value", QD),
                        ],
                    );
                    assert_eq!(
                        eval1(&ss, doc, "data(/*/@nt) instance of xs:NOTATION"),
                        "true"
                    );
                },
            );
        }
    }
}

/// The other half of the reviewer's case: the instance binds `p` nowhere.
/// The element default used to fail validation (`cvc-datatype-valid.1.2.1`,
/// its literal resolved with the instance's bindings); it is valid, and every
/// node holds the schema's expanded name. A name in no namespace keeps no
/// prefix where no default namespace is in scope.
#[test]
fn a_qname_default_whose_prefix_the_instance_does_not_bind_is_valid_and_fixed_up() {
    for version in VERSIONS {
        let ss = compile(version, QNAMES);
        for psvi in [true, false] {
            let label = format!("{version:?} psvi={psvi}");
            build(
                &ss,
                r#"<t:r xmlns:t="urn:t"><t:qe/><t:qf/><t:qd/></t:r>"#,
                psvi,
                |doc, validity, errors| {
                    assert_eq!(validity, Some(SchemaValidity::Valid), "{label}: {errors:?}");
                    assert!(errors.is_empty(), "{label}: {errors:?}");
                    check_qname_nodes(
                        &ss,
                        doc,
                        &label,
                        &[
                            ("/*/@qa", "ns0:value", QA),
                            ("/*/@qd", "t:value", QD),
                            ("/*/@qn", "value", QN),
                            ("/*/@ql", "ns0:a t:value", "{urn:p}a {urn:t}value"),
                            ("/*/@qu", "ns0:u", "{urn:p}u"),
                            ("/*/@nt", "t:gif", ""),
                            ("/*/t:qe", "ns0:value", QA),
                            ("/*/t:qf", "ns0:value", QA),
                            ("/*/t:qd", "t:value", QD),
                        ],
                    );
                },
            );
            // The schema's own prefix is kept where the instance binds it
            // to the same namespace.
            build(
                &ss,
                r#"<r xmlns="urn:t" xmlns:p="urn:p"><qe/></r>"#,
                psvi,
                |doc, validity, _| {
                    assert_eq!(validity, Some(SchemaValidity::Valid), "{label}");
                    check_qname_nodes(
                        &ss,
                        doc,
                        &label,
                        &[("/*/@qa", "p:value", QA), ("/*/t:qe", "p:value", QA)],
                    );
                    assert_eq!(eval1(&ss, doc, "count(/*/namespace::*)"), "3", "{label}");
                },
            );
        }
    }
}

/// The one case no prefix can fix: a name in no namespace where the instance
/// has a default namespace in scope (Namespaces in XML: a prefix cannot be
/// bound to no namespace, and undeclaring the default namespace on the
/// element would move the element's own unprefixed name). The node keeps the
/// {lexical form}, so in the tree it reads as a name in the default namespace;
/// the streaming PSVI holds the schema's `{}value` (next test).
#[test]
fn a_no_namespace_qname_default_under_a_default_namespace_keeps_its_lexical_form() {
    for version in VERSIONS {
        let ss = compile(version, QNAMES);
        build(&ss, r#"<r xmlns="urn:t"/>"#, true, |doc, validity, _| {
            assert_eq!(validity, Some(SchemaValidity::Valid));
            check_qname_nodes(&ss, doc, "no fixup", &[("/*/@qn", "value", QD)]);
        });
    }
}

/// The streaming PSVI hands out each defaulted attribute's {value} — the
/// schema's expanded names — beside the lexical form the schema writes, and
/// an element that takes a QName default has the schema's value as its typed
/// value; neither depends on the instance's bindings.
#[test]
fn the_streaming_psvi_has_the_schemas_qname_values() {
    #[derive(Default)]
    struct Seen(RefCell<Vec<(String, String)>>);
    impl ValidationEventHandler for &Seen {
        type Error = std::convert::Infallible;
        fn after_end_of_attributes(
            &mut self,
            view: EndOfAttributesView<'_>,
        ) -> Result<(), Self::Error> {
            for d in view.default_attributes {
                let typed = d.typed_value.as_ref().map(|v| format!("{v:?}"));
                self.0
                    .borrow_mut()
                    .push((d.value.clone(), typed.unwrap_or_default()));
            }
            Ok(())
        }
    }
    for version in VERSIONS {
        let ss = compile(version, QNAMES);
        for psvi in [true, false] {
            let label = format!("{version:?} psvi={psvi}");
            // The defaulted attributes through a typed build's handler.
            let validator = SchemaValidator::new(&ss, flags(psvi));
            let mut runtime = validator.start_run(SilentValidationSink);
            let arena = Bump::new();
            let mut builder =
                BufferDocumentBuilder::new(&arena, &ss.name_table, Some(&ss), Default::default())
                    .unwrap();
            let seen = Seen::default();
            {
                let mut handler = TypedBuilderHandler::with_handler(&mut builder, &seen);
                drive_quick_xml_with(
                    r#"<r xmlns="urn:t" xmlns:p="urn:other"/>"#.as_bytes(),
                    &mut runtime,
                    &ss,
                    &mut handler,
                )
                .expect("the pass completes");
            }
            let seen = seen.0.into_inner();
            let lexical: Vec<&str> = seen.iter().map(|(v, _)| v.as_str()).collect();
            assert_eq!(
                lexical,
                ["p:value", "value", "value", "p:a value", "p:u", "gif"],
                "{label}: the lexical forms as the schema writes them"
            );
            for (lexical, typed) in &seen {
                assert!(!typed.is_empty(), "{label}: {lexical} has a typed value");
            }

            // An element default through the push API, with `p` bound to
            // another namespace, and with `p` unbound.
            for bindings in [
                &[("", "urn:t"), ("p", "urn:other")][..],
                &[("", "urn:t")][..],
            ] {
                let mut ns = NamespaceContextSnapshot::default();
                for (prefix, uri) in bindings {
                    if prefix.is_empty() {
                        ns.default_ns = Some(ss.name_table.add(uri));
                    } else {
                        ns.bindings
                            .push((ss.name_table.add(prefix), ss.name_table.add(uri)));
                    }
                }
                let (mut errors, mut warnings) = (Vec::new(), Vec::new());
                let mut rt = validator.start_run(CollectingValidationSink {
                    errors: &mut errors,
                    warnings: &mut warnings,
                });
                let _ = rt.validate_element("r", "urn:t", None, None, &ns);
                let _ = rt.validate_end_of_attributes();
                let typed: Vec<String> = rt
                    .take_default_attributes()
                    .iter()
                    .map(|d| {
                        d.typed_value
                            .as_ref()
                            .map_or("?".into(), |v| expanded(&ss, v))
                    })
                    .collect();
                assert_eq!(
                    typed,
                    [
                        QA,
                        QD,
                        QN,
                        "{urn:p}a {urn:t}value",
                        "{urn:p}u",
                        "{urn:t}gif"
                    ],
                    "{label} {bindings:?}"
                );
                let _ = rt.validate_element("qe", "urn:t", None, None, &ns);
                let _ = rt.validate_end_of_attributes();
                let qe = rt.validate_end_element();
                let _ = rt.validate_end_element();
                rt.end_validation().expect("no operational failure");
                drop(rt);
                assert!(errors.is_empty(), "{label} {bindings:?}: {errors:?}");
                assert_eq!(qe.validity, SchemaValidity::Valid, "{label} {bindings:?}");
                assert!(qe.is_default, "{label}");
                if psvi {
                    let value = qe.typed_value.expect("a typed value");
                    assert_eq!(expanded(&ss, &value), QA, "{label} {bindings:?}");
                }
            }
        }
    }
}

// ── 2. Which union member makes an id or an IDREF ────────────────────

const IDS: &str = r###"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
    xmlns="urn:t" xmlns:t="urn:t" targetNamespace="urn:t" elementFormDefault="qualified">
  <xs:simpleType name="qe"><xs:restriction base="xs:QName">
    <xs:enumeration value="t:x"/>
  </xs:restriction></xs:simpleType>
  <xs:simpleType name="qOrId"><xs:union memberTypes="qe xs:ID"/></xs:simpleType>
  <xs:simpleType name="qOrIdref"><xs:union memberTypes="qe xs:IDREF"/></xs:simpleType>
  <xs:simpleType name="qOrIdrefs"><xs:list itemType="qOrIdref"/></xs:simpleType>
  <xs:simpleType name="qeList"><xs:list itemType="qe"/></xs:simpleType>
  <xs:simpleType name="qListOrIdrefs"><xs:union memberTypes="qeList xs:IDREFS"/></xs:simpleType>
  <xs:element name="r"><xs:complexType>
    <xs:sequence>
      <xs:element name="eu" type="qOrId"/>
      <xs:element name="ev" type="qOrIdref"/>
    </xs:sequence>
    <xs:attribute name="u" type="qOrId"/>
    <xs:attribute name="v" type="qOrIdref"/>
    <xs:attribute name="lv" type="qOrIdrefs"/>
    <xs:attribute name="w" type="qListOrIdrefs"/>
    <xs:attribute name="du" type="qOrId" default="x"/>
  </xs:complexType></xs:element>
</xs:schema>"###;

/// The reviewer's case: `x` under the default namespace `urn:t` is the QName
/// `{urn:t}x`, which the QName member's enumeration (`t:x`, `t` bound to
/// `urn:t` in the schema) admits — validation types it as a QName, so it is
/// neither an id nor an IDREF. Read without namespaces the enumeration does
/// not admit it, and the `xs:ID` / `xs:IDREF` member would. Also for a list
/// of such a union, a union of a QName list and `xs:IDREFS`, elements, and a
/// defaulted attribute.
const QNAME_DOC: &str = r#"<r xmlns="urn:t" u="x" v="x" lv="x" w="x"><eu>x</eu><ev>x</ev></r>"#;

/// The control: values the QName member rejects are ids and IDREFs.
const ID_DOC: &str = r#"<r xmlns="urn:t" u="y" v="y" lv="y" w="y"><eu>z</eu><ev>y</ev></r>"#;

/// Whether each element / attribute (in document order) has the is-idrefs
/// property, as `name` / `@name`.
fn idrefs_nodes(doc: &BufferDocument<'_>) -> Vec<String> {
    fn visit(nav: &BufferDocNavigator<'_>, out: &mut Vec<String>) {
        let mut child = nav.clone();
        if !child.move_to_first_child() {
            return;
        }
        loop {
            if child.node_type() == DomNodeType::Element {
                if child.is_idrefs() {
                    out.push(child.local_name().to_string());
                }
                let mut attr = child.clone();
                if attr.move_to_first_attribute() {
                    loop {
                        if attr.is_idrefs() {
                            out.push(format!("@{}", attr.local_name()));
                        }
                        if !attr.move_to_next_attribute() {
                            break;
                        }
                    }
                }
                visit(&child, out);
            }
            if !child.move_to_next_sibling() {
                break;
            }
        }
    }
    let mut out = Vec::new();
    visit(&doc.create_navigator(), &mut out);
    out
}

#[test]
fn a_union_value_validation_types_as_a_qname_is_neither_an_id_nor_an_idref() {
    for version in VERSIONS {
        let ss = compile(version, IDS);
        for psvi in [true, false] {
            let label = format!("{version:?} psvi={psvi}");
            build(&ss, QNAME_DOC, psvi, |doc, validity, errors| {
                assert_eq!(validity, Some(SchemaValidity::Valid), "{label}: {errors:?}");
                for (expr, expected) in [
                    ("data(/*/@u) instance of xs:QName", "true"),
                    ("data(/*/t:eu) instance of xs:QName", "true"),
                    ("data(/*/@du) instance of xs:QName", "true"),
                    ("data(/*/@lv) instance of xs:QName", "true"),
                    ("data(/*/@w) instance of xs:QName", "true"),
                    ("string(/*/@du)", "x"),
                    ("count(id('x'))", "0"),
                    ("count(idref('x'))", "0"),
                ] {
                    assert_eq!(eval1(&ss, doc, expr), expected, "{label}: {expr}");
                }
                assert_eq!(idrefs_nodes(doc), Vec::<String>::new(), "{label}");
                assert!(doc.get_element_by_id("x").is_none(), "{label}");
            });
            build(&ss, ID_DOC, psvi, |doc, validity, errors| {
                assert_eq!(validity, Some(SchemaValidity::Valid), "{label}: {errors:?}");
                assert_eq!(eval(&ss, doc, "id('y z')"), ["r", "eu"], "{label}");
                assert_eq!(
                    eval(&ss, doc, "idref('y')"),
                    ["@v", "@lv", "@w", "ev"],
                    "{label}"
                );
                assert_eq!(idrefs_nodes(doc), ["@v", "@lv", "@w", "ev"], "{label}");
                assert_eq!(eval1(&ss, doc, "data(/*/@u) instance of xs:ID"), "true");
            });
        }
    }
}

/// A copy that keeps the annotations classifies its nodes with its own
/// in-scope namespaces, as its navigator types them.
#[test]
fn a_copy_with_annotations_classifies_by_its_namespaces() {
    for version in VERSIONS {
        let ss = compile(version, IDS);
        build(&ss, QNAME_DOC, true, |source, _, _| {
            let mut r = source.create_navigator();
            assert!(r.move_to_first_child());
            let arena = Bump::new();
            let mut builder =
                BufferDocumentBuilder::new(&arena, &ss.name_table, Some(&ss), Default::default())
                    .unwrap();
            builder
                .copy_subtree(
                    &r,
                    CopyOptions {
                        annotations: Annotations::Preserve,
                        ..CopyOptions::default()
                    },
                )
                .unwrap();
            let copy = builder.finalize().unwrap();
            assert_eq!(eval1(&ss, &copy, "count(id('x'))"), "0", "{version:?}");
            assert_eq!(eval1(&ss, &copy, "count(idref('x'))"), "0", "{version:?}");
            assert_eq!(idrefs_nodes(&copy), Vec::<String>::new(), "{version:?}");
        });
    }
}

/// A host that builds a tree through the push API and binds the nodes itself
/// gets the same classification: the in-scope namespaces of the node decide.
#[test]
fn a_host_bound_qname_union_value_is_not_an_id() {
    use xsd_schema::document::NodeSchemaBinding;
    for version in VERSIONS {
        let ss = compile(version, IDS);
        let union = |name: &str| {
            ss.lookup_type(
                Some(ss.name_table.add("urn:t")),
                ss.name_table.get(name).expect("interned"),
            )
            .expect(name)
        };
        let arena = Bump::new();
        let mut builder =
            BufferDocumentBuilder::new(&arena, &ss.name_table, Some(&ss), Default::default())
                .unwrap();
        builder
            .start_element("r", "urn:t", "", &[("", "urn:t")])
            .unwrap();
        for (attr, type_name) in [("u", "qOrId"), ("v", "qOrIdref")] {
            let a = builder.attribute(attr, "", "", "x").unwrap();
            builder
                .set_node_binding(
                    a,
                    NodeSchemaBinding {
                        type_key: union(type_name),
                        element_decl: None,
                        attribute_decl: None,
                        content_type: None,
                    },
                )
                .unwrap();
        }
        builder.end_of_attributes();
        builder.end_element().unwrap();
        let doc = builder.finalize().unwrap();
        assert_eq!(eval1(&ss, &doc, "count(id('x'))"), "0", "{version:?}");
        assert_eq!(eval1(&ss, &doc, "count(idref('x'))"), "0", "{version:?}");
        assert_eq!(
            eval1(&ss, &doc, "data(/*/@u) instance of xs:QName"),
            "true",
            "{version:?}"
        );
    }
}

/// The same root cause in identity constraints (batch (r) C75: a defaulted
/// attribute's value joins a key sequence as its typed value): the default
/// `x`, written under the schema document's default namespace `urn:t`, is the
/// QName `{urn:t}x`, as the written `du="x"` under the instance's default
/// namespace is — the two are equal, so `xs:unique` is violated. Read without
/// the schema's bindings the default became the `xs:ID` "x", unequal to the
/// QName, and the duplicate went unreported.
#[test]
fn a_defaulted_qname_key_value_compares_as_the_schemas_qname() {
    let xsd = r###"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
        xmlns="urn:t" xmlns:t="urn:t" targetNamespace="urn:t" elementFormDefault="qualified">
      <xs:simpleType name="qe"><xs:restriction base="xs:QName">
        <xs:enumeration value="t:x"/>
      </xs:restriction></xs:simpleType>
      <xs:simpleType name="qOrId"><xs:union memberTypes="qe xs:ID"/></xs:simpleType>
      <xs:element name="doc">
        <xs:complexType><xs:sequence>
          <xs:element name="item" maxOccurs="unbounded"><xs:complexType>
            <xs:attribute name="du" type="qOrId" default="x"/>
          </xs:complexType></xs:element>
        </xs:sequence></xs:complexType>
        <xs:unique name="u"><xs:selector xpath="t:item"/><xs:field xpath="@du"/></xs:unique>
      </xs:element>
    </xs:schema>"###;
    for version in VERSIONS {
        let ss = compile(version, xsd);
        for psvi in [true, false] {
            build(
                &ss,
                r#"<doc xmlns="urn:t"><item du="x"/><item/></doc>"#,
                psvi,
                |_, validity, errors| {
                    assert_eq!(
                        validity,
                        Some(SchemaValidity::Invalid),
                        "{version:?} psvi={psvi}: {errors:?}"
                    );
                    assert!(
                        errors
                            .iter()
                            .any(|c| c.starts_with("cvc-identity-constraint")),
                        "{version:?} psvi={psvi}: {errors:?}"
                    );
                },
            );
        }
    }
}
