//! The id index when an attribute is replaced or rebound, when a tree is copied
//! with its annotations, under XSD 1.1 conditional type assignment, and for
//! the typed values of union and list types.
//!
//! The rule under test is XDM §6.3.4's is-id property of an attribute node:
//! always true for `xml:id`; otherwise true exactly when the attribute's typed
//! value is one atomic value of type `xs:ID` or a type derived from it. `fn:id`
//! (F&O §15.5.2) selects, within one tree, the first element in document order
//! that has such an attribute with the wanted value.

use bumpalo::Bump;

use super::IdClass;
use crate::document::{
    build_typed_document, Annotations, BufferDocNavigator, BufferDocument, BufferDocumentBuilder,
    BufferDocumentOptions, CopyOptions, NodeSchemaBinding,
};
use crate::ids::TypeKey;
use crate::namespace::context::NamespaceContextSnapshot;
use crate::namespace::table::XML_NAMESPACE;
use crate::namespace::NameTable;
use crate::navigator::{DomNavigator, TypedValue};
use crate::schema::SchemaSet;
use crate::types::XmlTypeCode;
use crate::xpath::context::XPathContext;
use crate::xpath::{XPathExpr, XmlItem};

// ── Fixtures ──────────────────────────────────────────────────────────

/// Attributes of every flavour the is-id rule has to tell apart.
const ID_SCHEMA: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
    <xs:simpleType name="idOrInteger">
        <xs:union memberTypes="xs:ID xs:integer"/>
    </xs:simpleType>
    <xs:simpleType name="nameOrId">
        <xs:union memberTypes="xs:NCName xs:ID"/>
    </xs:simpleType>
    <xs:simpleType name="idList">
        <xs:list itemType="xs:ID"/>
    </xs:simpleType>
    <xs:simpleType name="shortId">
        <xs:restriction base="xs:ID">
            <xs:maxLength value="5"/>
        </xs:restriction>
    </xs:simpleType>
    <xs:element name="root">
        <xs:complexType>
            <xs:sequence>
                <xs:element name="item" minOccurs="0" maxOccurs="unbounded">
                    <xs:complexType>
                        <xs:attribute name="key" type="xs:ID"/>
                        <xs:attribute name="u" type="idOrInteger"/>
                        <xs:attribute name="n" type="nameOrId"/>
                        <xs:attribute name="l" type="idList"/>
                        <xs:attribute name="s" type="shortId"/>
                        <xs:attribute name="text" type="xs:string"/>
                    </xs:complexType>
                </xs:element>
            </xs:sequence>
        </xs:complexType>
    </xs:element>
</xs:schema>"#;

fn load_schema(xsd: &str) -> SchemaSet {
    let mut schema_set = SchemaSet::xsd11();
    crate::pipeline::load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None)
        .expect("the schema loads");
    schema_set
}

fn typed<'a>(xml: &str, arena: &'a Bump, schema_set: &'a SchemaSet) -> BufferDocument<'a> {
    build_typed_document(
        xml.as_bytes(),
        arena,
        schema_set,
        BufferDocumentOptions::default(),
    )
    .expect("the instance builds")
}

fn parse<'a>(xml: &str, arena: &'a Bump, names: &'a NameTable) -> BufferDocument<'a> {
    BufferDocument::from_reader_default(xml.as_bytes(), arena, names).expect("the fixture parses")
}

fn builder<'a>(
    arena: &'a Bump,
    names: &'a NameTable,
    schema_set: Option<&'a SchemaSet>,
) -> BufferDocumentBuilder<'a> {
    BufferDocumentBuilder::new(arena, names, schema_set, BufferDocumentOptions::default())
        .expect("a builder")
}

fn preserving() -> CopyOptions {
    CopyOptions {
        annotations: Annotations::Preserve,
        ..CopyOptions::default()
    }
}

/// Evaluates `src` with the document node as the context item; `xs` is bound.
fn eval<'d>(
    doc: &'d BufferDocument<'d>,
    src: &str,
) -> crate::xpath::XPathValue<BufferDocNavigator<'d>> {
    let names = doc.names;
    let mut namespaces = NamespaceContextSnapshot::default();
    namespaces
        .bindings
        .push((names.add("xs"), names.add(crate::namespace::XS_NAMESPACE)));
    let ctx = XPathContext::new(names).with_namespaces(namespaces);
    XPathExpr::compile(src, &ctx)
        .expect("compiles")
        .evaluator(&ctx)
        .run_with_node(doc.create_navigator())
        .unwrap_or_else(|e| panic!("{src}: {e}"))
}

/// What `id($id)` selects, as node references.
fn id_refs(doc: &BufferDocument<'_>, id: &str) -> Vec<u32> {
    eval(doc, &format!("id('{id}')"))
        .into_vec()
        .into_iter()
        .map(|item| match item {
            XmlItem::Node(node) => node.current_ref(),
            XmlItem::Atomic(_) => panic!("fn:id returned an atomic value"),
        })
        .collect()
}

fn eval_bool(doc: &BufferDocument<'_>, src: &str) -> bool {
    eval(doc, src)
        .as_bool()
        .unwrap_or_else(|| panic!("{src} is not a boolean"))
}

/// The `n`-th element (0-based) named `name`, in document order.
fn element_named(doc: &BufferDocument<'_>, name: &str, n: usize) -> u32 {
    let refs: Vec<u32> = eval(doc, &format!("//{name}"))
        .into_vec()
        .into_iter()
        .map(|item| match item {
            XmlItem::Node(node) => node.current_ref(),
            XmlItem::Atomic(_) => unreachable!(),
        })
        .collect();
    refs[n]
}

/// A navigator on the attribute `name` of the element `elem`.
fn attribute_of<'d>(doc: &'d BufferDocument<'d>, elem: u32, name: &str) -> BufferDocNavigator<'d> {
    let mut nav = doc.create_navigator_at(elem);
    assert!(nav.move_to_first_attribute(), "the element has attributes");
    while nav.local_name() != name {
        assert!(nav.move_to_next_attribute(), "no attribute {name}");
    }
    nav
}

fn xml_of(doc: &BufferDocument<'_>) -> String {
    crate::document::serialize::to_string(&doc.create_navigator(), &Default::default())
        .expect("serializes")
}

fn simple_binding(schema_set: &SchemaSet, code: XmlTypeCode) -> NodeSchemaBinding {
    NodeSchemaBinding {
        type_key: TypeKey::Simple(
            schema_set
                .builtin_types()
                .get_by_type_code(code)
                .expect("a built-in type"),
        ),
        element_decl: None,
        attribute_decl: None,
        content_type: None,
    }
}

// ── Replacing an attribute ────────────────────────────────────────────

/// The review's reproduction: an `xml:id` replaced by a copied `xml:id` of
/// the same name. The serialization shows the new value, so the index must
/// answer to the new value and not to the old one.
#[test]
fn replacing_an_xml_id_moves_its_index_entry() {
    let arena = Bump::new();
    let names = NameTable::new();
    let source = parse(r#"<s xml:id="new"/>"#, &arena, &names);
    let replacement = attribute_of(&source, element_named(&source, "s", 0), "id");

    let mut builder = builder(&arena, &names, None);
    let e = builder.start_element("e", "", "", &[]).unwrap();
    builder
        .attribute("id", XML_NAMESPACE, "xml", "old")
        .unwrap();
    builder
        .copy_attribute(&replacement, CopyOptions::default())
        .unwrap();
    builder.end_of_attributes();
    builder.end_element().unwrap();
    let doc = builder.finalize().unwrap();

    assert_eq!(xml_of(&doc), r#"<e xml:id="new"/>"#);
    assert_eq!(
        id_refs(&doc, "old"),
        Vec::<u32>::new(),
        "the old id is gone"
    );
    assert_eq!(id_refs(&doc, "new"), vec![e], "the new id is filed");
}

/// The replacement is not a lexical NCName: the old mapping goes and nothing
/// takes its place.
#[test]
fn replacing_an_xml_id_with_a_non_ncname_leaves_nothing_filed() {
    let arena = Bump::new();
    let names = NameTable::new();
    let source = parse(r#"<s xml:id="1bad"/>"#, &arena, &names);
    let replacement = attribute_of(&source, element_named(&source, "s", 0), "id");

    let mut builder = builder(&arena, &names, None);
    builder.start_element("e", "", "", &[]).unwrap();
    builder
        .attribute("id", XML_NAMESPACE, "xml", " old ")
        .unwrap();
    builder
        .copy_attribute(&replacement, CopyOptions::default())
        .unwrap();
    builder.end_of_attributes();
    builder.end_element().unwrap();
    let doc = builder.finalize().unwrap();

    assert_eq!(xml_of(&doc), r#"<e xml:id="1bad"/>"#);
    assert_eq!(id_refs(&doc, "old"), Vec::<u32>::new());
    assert_eq!(doc.get_element_by_id("1bad"), None);
    assert!(doc.id_elements.is_empty(), "no stale key is left behind");
}

/// Replacing a value by one that normalizes to the same key keeps the element
/// selected, exactly once.
#[test]
fn replacing_an_xml_id_with_the_same_value_keeps_it() {
    let arena = Bump::new();
    let names = NameTable::new();
    let source = parse(r#"<s xml:id=" same "/>"#, &arena, &names);
    let replacement = attribute_of(&source, element_named(&source, "s", 0), "id");

    let mut builder = builder(&arena, &names, None);
    let e = builder.start_element("e", "", "", &[]).unwrap();
    builder
        .attribute("id", XML_NAMESPACE, "xml", "same")
        .unwrap();
    builder
        .copy_attribute(&replacement, CopyOptions::default())
        .unwrap();
    builder.end_of_attributes();
    builder.end_element().unwrap();
    let doc = builder.finalize().unwrap();

    assert_eq!(id_refs(&doc, "same"), vec![e]);
    assert_eq!(doc.id_elements.len(), 1);
}

/// Two elements share an id; the first gives it up, so the second is the one
/// `fn:id` selects.
#[test]
fn once_the_first_element_gives_up_an_id_the_second_is_found() {
    let arena = Bump::new();
    let names = NameTable::new();
    let source = parse(r#"<s xml:id="other"/>"#, &arena, &names);
    let replacement = attribute_of(&source, element_named(&source, "s", 0), "id");

    let mut builder = builder(&arena, &names, None);
    builder.start_element("r", "", "", &[]).unwrap();
    builder.end_of_attributes();
    let a = builder.start_element("a", "", "", &[]).unwrap();
    builder
        .attribute("id", XML_NAMESPACE, "xml", "dup")
        .unwrap();
    builder
        .copy_attribute(&replacement, CopyOptions::default())
        .unwrap();
    builder.end_of_attributes();
    builder.end_element().unwrap();
    let b = builder.start_element("b", "", "", &[]).unwrap();
    builder
        .attribute("id", XML_NAMESPACE, "xml", "dup")
        .unwrap();
    builder.end_of_attributes();
    builder.end_element().unwrap();
    builder.end_element().unwrap();
    let doc = builder.finalize().unwrap();

    assert_eq!(id_refs(&doc, "dup"), vec![b]);
    assert_eq!(id_refs(&doc, "other"), vec![a]);
}

/// A schema-typed `xs:ID` attribute replaced by another one: the index follows.
#[test]
fn replacing_a_schema_id_attribute_moves_its_index_entry() {
    let schema_set = load_schema(ID_SCHEMA);
    let arena = Bump::new();
    let source = typed(
        r#"<root><item key="alpha"/><item key="beta"/></root>"#,
        &arena,
        &schema_set,
    );
    let first = attribute_of(&source, element_named(&source, "item", 0), "key");
    let second = attribute_of(&source, element_named(&source, "item", 1), "key");

    let mut builder = builder(&arena, &schema_set.name_table, Some(&schema_set));
    let e = builder.start_element("e", "", "", &[]).unwrap();
    builder.copy_attribute(&first, preserving()).unwrap();
    builder.copy_attribute(&second, preserving()).unwrap();
    builder.end_of_attributes();
    builder.end_element().unwrap();
    let doc = builder.finalize().unwrap();

    assert_eq!(xml_of(&doc), r#"<e key="beta"/>"#);
    assert_eq!(id_refs(&doc, "alpha"), Vec::<u32>::new());
    assert_eq!(id_refs(&doc, "beta"), vec![e]);
}

/// The later duplicate wins with its value *and* its annotation. An
/// unannotated replacement must not inherit the earlier one's `xs:ID` type —
/// the replaced node is untyped, so it is not an id at all.
#[test]
fn an_unannotated_replacement_drops_the_id_and_the_old_annotation() {
    let schema_set = load_schema(ID_SCHEMA);
    let arena = Bump::new();
    let source = typed(
        r#"<root><item key="alpha"/><item key="beta"/></root>"#,
        &arena,
        &schema_set,
    );
    let first = attribute_of(&source, element_named(&source, "item", 0), "key");
    let second = attribute_of(&source, element_named(&source, "item", 1), "key");

    let mut builder = builder(&arena, &schema_set.name_table, Some(&schema_set));
    let e = builder.start_element("e", "", "", &[]).unwrap();
    builder.copy_attribute(&first, preserving()).unwrap();
    builder
        .copy_attribute(&second, CopyOptions::default())
        .unwrap();
    builder.end_of_attributes();
    builder.end_element().unwrap();
    let doc = builder.finalize().unwrap();

    assert_eq!(id_refs(&doc, "alpha"), Vec::<u32>::new());
    assert_eq!(id_refs(&doc, "beta"), Vec::<u32>::new());
    let attr = attribute_of(&doc, e, "key");
    assert!(
        attr.schema_binding().is_none(),
        "the old annotation is gone"
    );
    assert_eq!(attr.typed_value(), TypedValue::Untyped);
    assert!(
        !doc.has_type_annotations(),
        "no node of the result carries an annotation"
    );
}

/// Rebinding is a replacement too. Two elements claim one id through
/// schema-typed attributes; when the first one's attribute stops being an
/// `xs:ID`, the second one is selected, and it goes back when it is rebound.
#[test]
fn rebinding_hands_a_shared_id_to_the_next_element_in_document_order() {
    let schema_set = load_schema(ID_SCHEMA);
    let arena = Bump::new();
    let mut builder = builder(&arena, &schema_set.name_table, Some(&schema_set));
    builder.start_element("r", "", "", &[]).unwrap();
    builder.end_of_attributes();
    let a = builder.start_element("a", "", "", &[]).unwrap();
    let a_key = builder.attribute("key", "", "", "dup").unwrap();
    builder.end_of_attributes();
    builder.end_element().unwrap();
    let b = builder.start_element("b", "", "", &[]).unwrap();
    let b_key = builder.attribute("key", "", "", "dup").unwrap();
    builder.end_of_attributes();
    builder.end_element().unwrap();
    builder.end_element().unwrap();

    let id = simple_binding(&schema_set, XmlTypeCode::Id);
    let string = simple_binding(&schema_set, XmlTypeCode::String);
    builder.set_node_binding(a_key, id).unwrap();
    builder.set_node_binding(b_key, id).unwrap();
    assert_eq!(builder.doc.get_element_by_id("dup"), Some(a));
    builder.set_node_binding(a_key, string).unwrap();
    assert_eq!(builder.doc.get_element_by_id("dup"), Some(b));
    builder.set_node_binding(b_key, string).unwrap();
    assert_eq!(builder.doc.get_element_by_id("dup"), None);
    builder.set_node_binding(a_key, id).unwrap();
    let doc = builder.finalize().unwrap();
    assert_eq!(id_refs(&doc, "dup"), vec![a]);
}

/// A value registered through `register_xml_id` is the host's, not an
/// attribute's: replacing the element's `xml:id` does not withdraw it.
#[test]
fn a_registered_id_survives_the_replacement_of_an_xml_id() {
    let arena = Bump::new();
    let names = NameTable::new();
    let source = parse(r#"<s xml:id="other"/>"#, &arena, &names);
    let replacement = attribute_of(&source, element_named(&source, "s", 0), "id");

    let mut builder = builder(&arena, &names, None);
    let e = builder.start_element("e", "", "", &[]).unwrap();
    builder.attribute("id", XML_NAMESPACE, "xml", "k").unwrap();
    builder.register_xml_id("k", e).expect("the same element");
    builder
        .copy_attribute(&replacement, CopyOptions::default())
        .unwrap();
    builder.end_of_attributes();
    builder.end_element().unwrap();
    let doc = builder.finalize().unwrap();

    assert_eq!(id_refs(&doc, "k"), vec![e]);
    assert_eq!(id_refs(&doc, "other"), vec![e]);
}

/// The per-binding classification that keeps the common case cheap: most
/// annotations can never make an id, plain `xs:ID` is decided by the index
/// key alone, and everything else goes by the typed value.
#[test]
fn each_annotation_gets_the_right_is_id_class() {
    let schema_set = load_schema(ID_SCHEMA);
    let arena = Bump::new();
    let source = typed(
        r#"<root><item key="a" u="a" n="a" l="a" s="a" text="a"/></root>"#,
        &arena,
        &schema_set,
    );
    let item = element_named(&source, "item", 0);
    let mut builder = builder(&arena, &schema_set.name_table, Some(&schema_set));
    for (name, expected) in [
        ("key", IdClass::PlainId),
        ("u", IdClass::ByValue),
        ("n", IdClass::ByValue),
        ("l", IdClass::ByValue),
        ("s", IdClass::ByValue),
        ("text", IdClass::Never),
    ] {
        let binding = *attribute_of(&source, item, name)
            .schema_binding()
            .expect("typed");
        let idx = builder.doc.binding_remap.register(binding).unwrap();
        assert_eq!(builder.id_class(idx), expected, "@{name}");
    }
}

// ── Copies with annotations ───────────────────────────────────────────

/// The review's reproduction: a copy that preserves annotations keeps an
/// `xs:ID` attribute typed, so `fn:id` finds it in the copy as in the source.
#[test]
fn a_copy_with_preserved_annotations_carries_its_schema_ids() {
    let schema_set = load_schema(ID_SCHEMA);
    let arena = Bump::new();
    let source = typed(r#"<root><item key="alpha"/></root>"#, &arena, &schema_set);
    let source_item = element_named(&source, "item", 0);
    assert_eq!(id_refs(&source, "alpha"), vec![source_item]);

    let mut builder = builder(&arena, &schema_set.name_table, Some(&schema_set));
    builder
        .copy_subtree(&source.create_navigator_at(source.root()), preserving())
        .unwrap();
    let copy = builder.finalize().unwrap();

    let copied_item = element_named(&copy, "item", 0);
    assert!(
        attribute_of(&copy, copied_item, "key")
            .schema_type()
            .is_some(),
        "the attribute stays typed"
    );
    assert_eq!(id_refs(&copy, "alpha"), vec![copied_item]);
}

/// Without annotations the copied attribute is untyped and not an id.
#[test]
fn a_copy_with_stripped_annotations_carries_no_schema_ids() {
    let schema_set = load_schema(ID_SCHEMA);
    let arena = Bump::new();
    let source = typed(r#"<root><item key="alpha"/></root>"#, &arena, &schema_set);

    let mut builder = builder(&arena, &schema_set.name_table, Some(&schema_set));
    builder
        .copy_subtree(
            &source.create_navigator_at(source.root()),
            CopyOptions::default(),
        )
        .unwrap();
    let copy = builder.finalize().unwrap();
    assert_eq!(id_refs(&copy, "alpha"), Vec::<u32>::new());
}

/// A lone attribute copied with its annotation onto a constructed element.
#[test]
fn copy_attribute_with_preserved_annotations_carries_a_schema_id() {
    let schema_set = load_schema(ID_SCHEMA);
    let arena = Bump::new();
    let source = typed(r#"<root><item key="alpha"/></root>"#, &arena, &schema_set);
    let key = attribute_of(&source, element_named(&source, "item", 0), "key");

    let mut builder = builder(&arena, &schema_set.name_table, Some(&schema_set));
    let e = builder.start_element("e", "", "", &[]).unwrap();
    builder.copy_attribute(&key, preserving()).unwrap();
    builder.end_of_attributes();
    builder.end_element().unwrap();
    let doc = builder.finalize().unwrap();
    assert_eq!(id_refs(&doc, "alpha"), vec![e]);
}

// ── Is-id from the typed value ────────────────────────────────────────

/// The review's reproduction: a union of `xs:ID` and `xs:integer` whose value
/// validates as the `xs:ID` member.
#[test]
fn a_union_value_of_the_id_member_is_an_id() {
    let schema_set = load_schema(ID_SCHEMA);
    let arena = Bump::new();
    let doc = typed(r#"<root><item u="alpha"/></root>"#, &arena, &schema_set);
    assert!(eval_bool(&doc, "data(//item/@u) instance of xs:ID"));
    assert_eq!(id_refs(&doc, "alpha"), vec![element_named(&doc, "item", 0)]);
}

/// The same union, the `xs:integer` member.
#[test]
fn a_union_value_of_the_integer_member_is_not_an_id() {
    let schema_set = load_schema(ID_SCHEMA);
    let arena = Bump::new();
    let doc = typed(r#"<root><item u="12"/></root>"#, &arena, &schema_set);
    assert!(eval_bool(&doc, "data(//item/@u) instance of xs:integer"));
    assert_eq!(id_refs(&doc, "12"), Vec::<u32>::new());
    assert!(doc.id_elements.is_empty());
}

/// A union that *involves* `xs:ID` is recognised only when the actual value is
/// of type `xs:ID` (XDM §6.3.4). Here `xs:NCName` comes first and wins, so the
/// value is an `xs:NCName` and the attribute is not an id — although the very
/// same string would have validated as the `xs:ID` member.
#[test]
fn a_union_value_of_a_non_id_member_is_not_an_id() {
    let schema_set = load_schema(ID_SCHEMA);
    let arena = Bump::new();
    let doc = typed(r#"<root><item n="alpha"/></root>"#, &arena, &schema_set);
    assert!(eval_bool(&doc, "data(//item/@n) instance of xs:NCName"));
    assert!(!eval_bool(&doc, "data(//item/@n) instance of xs:ID"));
    assert_eq!(id_refs(&doc, "alpha"), Vec::<u32>::new());
}

/// The review's reproduction: a list of `xs:ID` of length one.
#[test]
fn a_singleton_list_of_id_is_an_id() {
    let schema_set = load_schema(ID_SCHEMA);
    let arena = Bump::new();
    let doc = typed(r#"<root><item l=" alpha "/></root>"#, &arena, &schema_set);
    assert!(eval_bool(&doc, "data(//item/@l) instance of xs:ID"));
    assert_eq!(id_refs(&doc, "alpha"), vec![element_named(&doc, "item", 0)]);
}

/// A list of two `xs:ID`s is two atomic values, not one: not an id.
#[test]
fn a_two_item_list_of_id_is_not_an_id() {
    let schema_set = load_schema(ID_SCHEMA);
    let arena = Bump::new();
    let doc = typed(
        r#"<root><item l="alpha beta"/></root>"#,
        &arena,
        &schema_set,
    );
    // The typed value is two `xs:ID` values.
    let l = attribute_of(&doc, element_named(&doc, "item", 0), "l");
    match l.typed_value() {
        TypedValue::Value(value) => match value.value {
            crate::types::value::XmlValueKind::List { items, .. } => assert_eq!(items.len(), 2),
            other => panic!("expected a list value, got {other:?}"),
        },
        other => panic!("expected a typed value, got {other:?}"),
    }
    assert_eq!(id_refs(&doc, "alpha"), Vec::<u32>::new());
    assert_eq!(id_refs(&doc, "beta"), Vec::<u32>::new());
    assert!(doc.id_elements.is_empty());
}

/// A type derived from `xs:ID` by restriction is an id type; a value it
/// rejects has no `xs:ID` typed value and is not an id.
#[test]
fn a_restriction_of_id_is_an_id_when_the_value_is_valid() {
    let schema_set = load_schema(ID_SCHEMA);
    let arena = Bump::new();
    let doc = typed(
        r#"<root><item s="abc"/><item s="abcdefgh"/></root>"#,
        &arena,
        &schema_set,
    );
    assert!(eval_bool(&doc, "data(//item[1]/@s) instance of xs:ID"));
    assert_eq!(id_refs(&doc, "abc"), vec![element_named(&doc, "item", 0)]);

    // maxLength 5 rejects it: the typed value is xs:untypedAtomic.
    assert!(eval_bool(
        &doc,
        "data(//item[2]/@s) instance of xs:untypedAtomic"
    ));
    assert_eq!(id_refs(&doc, "abcdefgh"), Vec::<u32>::new());
}

/// Ids from typed values follow `xml:id`'s rules: the first element in
/// document order wins, and the scope is one tree.
#[test]
fn typed_value_ids_select_the_first_element_of_their_own_tree() {
    let schema_set = load_schema(ID_SCHEMA);
    let arena = Bump::new();
    let source = typed(
        r#"<root><item u="dup"/><item l="dup"/></root>"#,
        &arena,
        &schema_set,
    );
    assert_eq!(
        id_refs(&source, "dup"),
        vec![element_named(&source, "item", 0)]
    );

    // Two copies of the second item, as two trees of one fragment buffer.
    let second = source.create_navigator_at(element_named(&source, "item", 1));
    let mut builder = BufferDocumentBuilder::new(
        &arena,
        &schema_set.name_table,
        Some(&schema_set),
        BufferDocumentOptions::fragment(),
    )
    .unwrap();
    builder.copy_subtree(&second, preserving()).unwrap();
    builder.copy_subtree(&second, preserving()).unwrap();
    let doc = builder.finalize().unwrap();
    let mut trees = doc.create_navigator();
    assert!(trees.move_to_first_child());
    let first_tree = trees.current_ref();
    assert!(trees.move_to_next_sibling());
    let second_tree = trees.current_ref();
    assert_eq!(
        doc.get_element_by_id_in_tree(first_tree, "dup"),
        Some(first_tree)
    );
    assert_eq!(
        doc.get_element_by_id_in_tree(second_tree, "dup"),
        Some(second_tree)
    );
}

/// A copy with annotations and its source agree on every flavour.
#[test]
fn a_copy_and_its_source_agree_on_typed_value_ids() {
    let schema_set = load_schema(ID_SCHEMA);
    let arena = Bump::new();
    let source = typed(
        r#"<root><item u="a1"/><item u="7"/><item n="a2"/><item l="a3"/><item l="a4 a5"/><item s="a6"/><item s="a7long"/><item key="a8"/><item text="a9"/></root>"#,
        &arena,
        &schema_set,
    );
    let mut builder = builder(&arena, &schema_set.name_table, Some(&schema_set));
    builder
        .copy_subtree(&source.create_navigator_at(source.root()), preserving())
        .unwrap();
    let copy = builder.finalize().unwrap();

    let found = |doc: &BufferDocument<'_>| -> Vec<String> {
        ["a1", "a2", "a3", "a4", "a5", "a6", "a7long", "a8", "a9"]
            .iter()
            .filter(|id| !id_refs(doc, id).is_empty())
            .map(|id| id.to_string())
            .collect()
    };
    assert_eq!(found(&source), ["a1", "a3", "a6", "a8"]);
    assert_eq!(found(&copy), found(&source));
}

// ── Conditional type assignment (XSD 1.1) ─────────────────────────────

#[cfg(feature = "xsd11")]
mod conditional_type_assignment {
    use super::*;

    /// `e` is declared with `base`, whose `val` is an `xs:NCName`; the
    /// alternative selects `keyed`, a restriction of `base` that narrows
    /// `val` to `xs:ID` (XSD 1.1 e-props-correct.7.1 holds).
    const CTA_TO_ID: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
        <xs:complexType name="base">
            <xs:attribute name="kind" type="xs:string"/>
            <xs:attribute name="val" type="xs:NCName"/>
        </xs:complexType>
        <xs:complexType name="keyed">
            <xs:complexContent>
                <xs:restriction base="base">
                    <xs:attribute name="kind" type="xs:string"/>
                    <xs:attribute name="val" type="xs:ID"/>
                </xs:restriction>
            </xs:complexContent>
        </xs:complexType>
        <xs:element name="root">
            <xs:complexType>
                <xs:sequence>
                    <xs:element name="e" type="base" maxOccurs="unbounded">
                        <xs:alternative test="@kind = 'id'" type="keyed"/>
                    </xs:element>
                </xs:sequence>
            </xs:complexType>
        </xs:element>
    </xs:schema>"#;

    /// The declared type makes `val` an `xs:ID`; the alternative selects a
    /// type in which it is an `xs:string`. Written the way the crate's other
    /// deferred-binding tests are: the alternative's type is not derived from
    /// the declared one, which this crate accepts.
    const CTA_FROM_ID: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
        <xs:complexType name="plain">
            <xs:attribute name="kind" type="xs:string"/>
            <xs:attribute name="val" type="xs:string"/>
        </xs:complexType>
        <xs:element name="root">
            <xs:complexType>
                <xs:sequence>
                    <xs:element name="d" maxOccurs="unbounded">
                        <xs:complexType>
                            <xs:attribute name="kind" type="xs:string"/>
                            <xs:attribute name="val" type="xs:ID"/>
                        </xs:complexType>
                        <xs:alternative test="@kind = 'plain'" type="plain"/>
                    </xs:element>
                </xs:sequence>
            </xs:complexType>
        </xs:element>
    </xs:schema>"#;

    /// The review's reproduction: the final binding of a CTA-deferred
    /// attribute makes it an `xs:ID`.
    #[test]
    fn an_attribute_typed_id_by_type_assignment_is_an_id() {
        let schema_set = load_schema(CTA_TO_ID);
        let arena = Bump::new();
        let doc = typed(
            r#"<root><e kind="id" val="alpha"/><e kind="name" val="beta"/></root>"#,
            &arena,
            &schema_set,
        );
        assert!(eval_bool(&doc, "data(//e[1]/@val) instance of xs:ID"));
        assert_eq!(id_refs(&doc, "alpha"), vec![element_named(&doc, "e", 0)]);
        assert!(!eval_bool(&doc, "data(//e[2]/@val) instance of xs:ID"));
        assert_eq!(id_refs(&doc, "beta"), Vec::<u32>::new());
    }

    /// Only the final type counts: an alternative that turns the declared
    /// `xs:ID` into an `xs:string` leaves nothing filed, while an element no
    /// alternative matches keeps the declared type and its id.
    #[test]
    fn an_attribute_typed_away_from_id_by_type_assignment_is_not_an_id() {
        let schema_set = load_schema(CTA_FROM_ID);
        let arena = Bump::new();
        let doc = typed(
            r#"<root><d kind="plain" val="alpha"/><d kind="other" val="beta"/></root>"#,
            &arena,
            &schema_set,
        );
        assert!(eval_bool(&doc, "data(//d[1]/@val) instance of xs:string"));
        assert!(!eval_bool(&doc, "data(//d[1]/@val) instance of xs:ID"));
        assert_eq!(id_refs(&doc, "alpha"), Vec::<u32>::new());
        assert_eq!(id_refs(&doc, "beta"), vec![element_named(&doc, "d", 1)]);
    }
}
