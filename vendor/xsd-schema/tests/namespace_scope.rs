//! The namespace scope of a `BufferDocument` element: what its namespace axis,
//! `fn:in-scope-prefixes`, the serializer, a copy and a typed value see, and
//! what a walk over a deep tree costs.
//!
//! The bindings in scope at an element are its own declarations followed by
//! those of its ancestors, innermost first; the first declaration of a prefix
//! wins. Under `NamespaceAxisScope::All` (the XDM namespace nodes) an
//! undeclaration — `xmlns=""`, or XML 1.1's `xmlns:p=""` — hides an outer
//! binding and makes no node of its own, and the `xml` binding is always there;
//! `ExcludeXml` keeps undeclarations and drops `xml`; `Local` is the element's
//! own declarations. [`expected_in_scope`] states that rule independently of
//! the document, and every test checks the document against it.
//!
//! The last tests are complexity checks rather than timings: a walk over a
//! tree twice as deep must take about twice as long, not four times.

#![cfg(feature = "xsd11")]

use std::time::Instant;

use bumpalo::Bump;
use xsd_schema::document::serialize;
use xsd_schema::document::{
    build_typed_document, BufferDocument, BufferDocumentBuilder, BufferDocumentOptions,
    CopyNamespaces, CopyOptions, SerializeOptions,
};
use xsd_schema::namespace::NameTable;
use xsd_schema::navigator::{DomNavigator, DomNodeType, NamespaceAxisScope};
use xsd_schema::pipeline::load_and_process_schema;
use xsd_schema::validation::{
    drive_buffer_document, CollectingValidationSink, SchemaValidator, ValidationFlags,
};
use xsd_schema::xpath::{XPathContext, XPathExpr, XmlItem};
use xsd_schema::SchemaSet;

const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";

/// A `(prefix, namespace URI)` pair.
type Binding = (String, String);

// ── Reading a document ──────────────────────────────────────────────────

/// Every element of the tree `nav` is in, in document order.
fn elements<N: DomNavigator>(nav: &N) -> Vec<N> {
    let mut found = Vec::new();
    let mut cur = nav.clone();
    cur.move_to_root();
    let mut depth = 0usize;
    loop {
        if cur.node_type() == DomNodeType::Element {
            found.push(cur.clone());
        }
        let mut child = cur.clone();
        if child.move_to_first_child() {
            cur = child;
            depth += 1;
            continue;
        }
        loop {
            if depth == 0 {
                return found;
            }
            if cur.move_to_next_sibling() {
                break;
            }
            cur.move_to_parent();
            depth -= 1;
        }
    }
}

/// The namespace axis of the element at `nav` under `scope`.
fn axis<N: DomNavigator>(nav: &N, scope: NamespaceAxisScope) -> Vec<Binding> {
    let mut found = Vec::new();
    let mut ns = nav.clone();
    if ns.move_to_first_namespace(scope) {
        loop {
            assert_eq!(ns.node_type(), DomNodeType::Namespace);
            found.push((ns.local_name().to_string(), ns.value()));
            if !ns.move_to_next_namespace(scope) {
                break;
            }
        }
        assert!(
            ns.move_to_parent(),
            "a namespace node has its element as parent"
        );
        assert!(ns.is_same_position(nav), "and it is this element");
    }
    found
}

/// The element at `nav` and its element ancestors, innermost first.
fn ancestors_or_self<N: DomNavigator>(nav: &N) -> Vec<N> {
    let mut chain = vec![nav.clone()];
    let mut up = nav.clone();
    while up.move_to_parent() && up.node_type() == DomNodeType::Element {
        chain.push(up.clone());
    }
    chain
}

/// The bindings in scope under `scope` (`All` or `ExcludeXml`), given the
/// declarations of an element and of each of its ancestors, innermost first,
/// each element's in its local-axis order.
fn expected_in_scope(locals: &[Vec<Binding>], scope: NamespaceAxisScope) -> Vec<Binding> {
    let mut seen: Vec<&str> = Vec::new();
    let mut found = Vec::new();
    for (prefix, uri) in locals.iter().flatten() {
        if seen.contains(&prefix.as_str()) {
            continue;
        }
        seen.push(prefix);
        let keep = match scope {
            NamespaceAxisScope::All => !uri.is_empty(),
            NamespaceAxisScope::ExcludeXml => prefix != "xml",
            NamespaceAxisScope::Local => unreachable!("the local axis is the declarations"),
        };
        if keep {
            found.push((prefix.clone(), uri.clone()));
        }
    }
    if scope == NamespaceAxisScope::All && !seen.contains(&"xml") {
        found.push(("xml".to_string(), XML_NS.to_string()));
    }
    found
}

/// The single item of `expr` evaluated at `nav`, as a string.
fn eval1<N: DomNavigator>(names: &NameTable, nav: &N, expr: &str) -> String {
    let ctx = XPathContext::new(names);
    let compiled = XPathExpr::compile(expr, &ctx).unwrap_or_else(|e| panic!("{expr}: {e}"));
    let items = compiled
        .evaluator(&ctx)
        .run_with_node(nav.clone())
        .unwrap_or_else(|e| panic!("{expr}: {e}"))
        .into_vec();
    assert_eq!(items.len(), 1, "{expr}");
    match &items[0] {
        XmlItem::Node(node) => node.value(),
        XmlItem::Atomic(value) => value.to_string_value(),
    }
}

/// Every element of `doc` answers [`expected_in_scope`] of its own and its
/// ancestors' local axes, and `fn:in-scope-prefixes` lists the `All` axis
/// (after `xml`, which it puts first).
fn assert_scopes_consistent(doc: &BufferDocument<'_>) {
    for element in elements(&doc.create_navigator()) {
        let locals: Vec<Vec<Binding>> = ancestors_or_self(&element)
            .iter()
            .map(|e| axis(e, NamespaceAxisScope::Local))
            .collect();
        let all = axis(&element, NamespaceAxisScope::All);
        for scope in [NamespaceAxisScope::All, NamespaceAxisScope::ExcludeXml] {
            assert_eq!(
                axis(&element, scope),
                expected_in_scope(&locals, scope),
                "{scope:?} at <{}>",
                element.name()
            );
        }
        let mut prefixes = vec!["xml".to_string()];
        prefixes.extend(all.into_iter().map(|(p, _)| p).filter(|p| p != "xml"));
        assert_eq!(
            eval1(
                doc.names(),
                &element,
                "string-join(in-scope-prefixes(.), ' ')"
            ),
            prefixes.join(" "),
            "in-scope-prefixes at <{}>",
            element.name()
        );
    }
}

/// One line per element: its name and its three namespace axes.
fn listing(doc: &BufferDocument<'_>) -> String {
    let show = |bindings: Vec<Binding>| {
        bindings
            .iter()
            .map(|(p, u)| format!("{p}={u}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let mut text = String::new();
    for element in elements(&doc.create_navigator()) {
        text.push_str(&format!(
            "{} | all: {} | excl: {} | local: {}\n",
            element.name(),
            show(axis(&element, NamespaceAxisScope::All)),
            show(axis(&element, NamespaceAxisScope::ExcludeXml)),
            show(axis(&element, NamespaceAxisScope::Local)),
        ));
    }
    text
}

fn parse<'a>(text: &str, arena: &'a Bump, names: &'a NameTable) -> BufferDocument<'a> {
    BufferDocument::from_reader_default(text.as_bytes(), arena, names).expect("well-formed")
}

fn builder<'a>(arena: &'a Bump, names: &'a NameTable) -> BufferDocumentBuilder<'a> {
    BufferDocumentBuilder::new(arena, names, None, BufferDocumentOptions::default())
        .expect("builder")
}

// ── Trees built from a model ────────────────────────────────────────────

/// A small deterministic generator (xorshift64*), so a failure reproduces.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 33) % n
    }
}

/// An element of a model tree: its declarations, in the order given to the
/// builder, and its children.
struct Spec {
    decls: Vec<(&'static str, &'static str)>,
    children: Vec<Spec>,
}

/// A random declaration: a prefix bound, rebound or undeclared (the empty
/// URI), now and then the `xml` prefix bound to its own namespace.
fn declaration(rng: &mut Rng) -> (&'static str, &'static str) {
    const PREFIXES: [&str; 4] = ["", "p", "q", "r"];
    const URIS: [&str; 4] = ["", "urn:a", "urn:b", "urn:c"];
    if rng.below(16) == 0 {
        return ("xml", XML_NS);
    }
    (PREFIXES[rng.below(4) as usize], URIS[rng.below(4) as usize])
}

fn random_spec(rng: &mut Rng, depth: usize) -> Spec {
    let declares = rng.below(2) == 0;
    let decls = if declares {
        // One to three, a prefix now and then twice on one element.
        (0..1 + rng.below(3)).map(|_| declaration(rng)).collect()
    } else {
        Vec::new()
    };
    let children = if depth == 0 {
        Vec::new()
    } else if rng.below(5) == 0 {
        // A run of levels without declarations between two that have some.
        let mut inner = random_spec(rng, depth - 1);
        for _ in 0..rng.below(12) {
            inner = Spec {
                decls: Vec::new(),
                children: vec![inner],
            };
        }
        vec![inner]
    } else {
        (0..rng.below(4))
            .map(|_| random_spec(rng, depth - 1))
            .collect()
    };
    Spec { decls, children }
}

fn build_spec(builder: &mut BufferDocumentBuilder<'_>, spec: &Spec) {
    builder
        .start_element("e", "", "", &spec.decls)
        .expect("start");
    builder.end_of_attributes();
    for child in &spec.children {
        build_spec(builder, child);
    }
    builder.end_element().expect("end");
}

/// The local axis each element of `spec` must answer, in document order,
/// with its ancestors' (innermost first) — from the model alone.
fn model_locals(spec: &Spec, outer: &mut Vec<Vec<Binding>>, out: &mut Vec<Vec<Vec<Binding>>>) {
    // The builder chains each declaration in front of the ones before it, so
    // the local axis lists an element's declarations last to first.
    let local: Vec<Binding> = spec
        .decls
        .iter()
        .rev()
        .map(|(p, u)| (p.to_string(), u.to_string()))
        .collect();
    outer.insert(0, local);
    out.push(outer.clone());
    for child in &spec.children {
        model_locals(child, outer, out);
    }
    outer.remove(0);
}

#[test]
fn random_trees_answer_their_declarations() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for _ in 0..200 {
        let spec = random_spec(&mut rng, 6);
        let arena = Bump::new();
        let names = NameTable::new();
        let mut b = builder(&arena, &names);
        build_spec(&mut b, &spec);
        let doc = b.finalize().expect("finalize");

        let mut expected = Vec::new();
        model_locals(&spec, &mut Vec::new(), &mut expected);
        let found = elements(&doc.create_navigator());
        assert_eq!(found.len(), expected.len());
        for (element, locals) in found.iter().zip(&expected) {
            assert_eq!(axis(element, NamespaceAxisScope::Local), locals[0]);
            for scope in [NamespaceAxisScope::All, NamespaceAxisScope::ExcludeXml] {
                assert_eq!(axis(element, scope), expected_in_scope(locals, scope));
            }
        }
        assert_scopes_consistent(&doc);
    }
}

/// A chain 300 levels deep: the default namespace rebound on every level, a
/// prefix declared, undeclared and rebound now and then.
#[test]
fn a_deep_chain_of_redeclarations_answers_its_declarations() {
    let mut chain: Vec<Vec<(&'static str, &'static str)>> = Vec::new();
    for level in 0..300 {
        let mut decls = vec![("", if level % 2 == 0 { "urn:a" } else { "urn:b" })];
        if level % 7 == 0 {
            decls.push(("p", "urn:p"));
        }
        if level % 11 == 0 {
            decls.push(("p", ""));
        }
        if level % 13 == 0 {
            decls.push(("q", "urn:q"));
        }
        chain.push(decls);
    }
    let arena = Bump::new();
    let names = NameTable::new();
    let mut b = builder(&arena, &names);
    for decls in &chain {
        b.start_element("e", "", "", decls).expect("start");
        b.end_of_attributes();
    }
    for _ in &chain {
        b.end_element().expect("end");
    }
    let doc = b.finalize().expect("finalize");

    let found = elements(&doc.create_navigator());
    let mut locals: Vec<Vec<Binding>> = Vec::new();
    for (element, decls) in found.iter().zip(&chain) {
        let local: Vec<Binding> = decls
            .iter()
            .rev()
            .map(|(p, u)| (p.to_string(), u.to_string()))
            .collect();
        locals.insert(0, local);
        assert_eq!(axis(element, NamespaceAxisScope::Local), locals[0]);
        for scope in [NamespaceAxisScope::All, NamespaceAxisScope::ExcludeXml] {
            assert_eq!(axis(element, scope), expected_in_scope(&locals, scope));
        }
    }
}

// ── Parsed trees, the serializer, copies ────────────────────────────────

/// Redeclarations, undeclarations (the default namespace, and an XML 1.1
/// prefix) and an explicit `xml` declaration.
const NESTED: &str = r#"<?xml version="1.1"?><a xmlns="urn:d" xmlns:p="urn:p1" xmlns:q="urn:q"><b xmlns:p="urn:p2" p:k="1"><c xmlns=""><d xmlns:p=""><e xmlns:q="urn:q"><p:f xmlns:p="urn:p3"/></e></d></c></b><g xmlns:xml="http://www.w3.org/XML/1998/namespace" q:k="2"/></a>"#;

#[test]
fn a_parsed_tree_answers_its_declarations() {
    let arena = Bump::new();
    let names = NameTable::new();
    let doc = parse(NESTED, &arena, &names);
    assert_scopes_consistent(&doc);
    assert_eq!(
        listing(&doc),
        "a | all: q=urn:q p=urn:p1 =urn:d xml=http://www.w3.org/XML/1998/namespace \
         | excl: q=urn:q p=urn:p1 =urn:d | local: q=urn:q p=urn:p1 =urn:d\n\
         b | all: p=urn:p2 q=urn:q =urn:d xml=http://www.w3.org/XML/1998/namespace \
         | excl: p=urn:p2 q=urn:q =urn:d | local: p=urn:p2\n\
         c | all: p=urn:p2 q=urn:q xml=http://www.w3.org/XML/1998/namespace \
         | excl: = p=urn:p2 q=urn:q | local: =\n\
         d | all: q=urn:q xml=http://www.w3.org/XML/1998/namespace \
         | excl: p= = q=urn:q | local: p=\n\
         e | all: q=urn:q xml=http://www.w3.org/XML/1998/namespace \
         | excl: q=urn:q p= = | local: q=urn:q\n\
         p:f | all: p=urn:p3 q=urn:q xml=http://www.w3.org/XML/1998/namespace \
         | excl: p=urn:p3 q=urn:q = | local: p=urn:p3\n\
         g | all: xml=http://www.w3.org/XML/1998/namespace q=urn:q p=urn:p1 =urn:d \
         | excl: q=urn:q p=urn:p1 =urn:d | local: xml=http://www.w3.org/XML/1998/namespace\n"
    );
}

#[test]
fn serialized_output_is_unchanged() {
    let arena = Bump::new();
    let names = NameTable::new();
    let doc = parse(NESTED, &arena, &names);
    let opts = SerializeOptions::default();
    assert_eq!(
        serialize::to_string(&doc.create_navigator(), &opts).expect("serializes"),
        r#"<a xmlns="urn:d" xmlns:p="urn:p1" xmlns:q="urn:q"><b xmlns:p="urn:p2" p:k="1"><c xmlns=""><d><e><p:f xmlns:p="urn:p3"/></e></d></c></b><g q:k="2"/></a>"#
    );
    let fragments: Vec<String> = elements(&doc.create_navigator())
        .iter()
        .map(|e| serialize::to_string(e, &opts).expect("serializes"))
        .collect();
    assert_eq!(
        fragments,
        [
            r#"<a xmlns="urn:d" xmlns:p="urn:p1" xmlns:q="urn:q"><b xmlns:p="urn:p2" p:k="1"><c xmlns=""><d><e><p:f xmlns:p="urn:p3"/></e></d></c></b><g q:k="2"/></a>"#,
            r#"<b xmlns="urn:d" xmlns:p="urn:p2" xmlns:q="urn:q" p:k="1"><c xmlns=""><d><e><p:f xmlns:p="urn:p3"/></e></d></c></b>"#,
            r#"<c xmlns:p="urn:p2" xmlns:q="urn:q"><d><e><p:f xmlns:p="urn:p3"/></e></d></c>"#,
            r#"<d xmlns:q="urn:q"><e><p:f xmlns:p="urn:p3"/></e></d>"#,
            r#"<e xmlns:q="urn:q"><p:f xmlns:p="urn:p3"/></e>"#,
            r#"<p:f xmlns:p="urn:p3" xmlns:q="urn:q"/>"#,
            r#"<g xmlns="urn:d" xmlns:p="urn:p1" xmlns:q="urn:q" q:k="2"/>"#,
        ]
    );
}

/// Copies of the parsed tree's `b` into a parent whose bindings differ: the
/// default namespace and `p` bound elsewhere, `q` not at all.
#[test]
fn copies_into_a_parent_with_other_bindings() {
    let arena = Bump::new();
    let names = NameTable::new();
    let source = parse(NESTED, &arena, &names);
    let b = elements(&source.create_navigator())[1].clone();
    assert_eq!(b.name(), "b");

    let mut copies = Vec::new();
    for opts in [
        CopyOptions::default(),
        CopyOptions {
            namespaces: CopyNamespaces::NoPreserve,
            inherit: false,
            ..CopyOptions::default()
        },
    ] {
        let mut target = builder(&arena, &names);
        target
            .start_element("w", "urn:w", "", &[("", "urn:w"), ("p", "urn:other")])
            .expect("start w");
        target.end_of_attributes();
        target.copy_subtree(&b, opts).expect("copies");
        target.end_element().expect("end w");
        let copy = target.finalize().expect("finalize");
        assert_scopes_consistent(&copy);
        copies.push((
            listing(&copy),
            serialize::to_string(&copy.create_navigator(), &SerializeOptions::default())
                .expect("serializes"),
        ));
    }
    assert_eq!(
        copies[0].1,
        r#"<w xmlns="urn:w" xmlns:p="urn:other"><b xmlns="urn:d" xmlns:p="urn:p2" p:k="1"><c xmlns=""><d><e xmlns:q="urn:q"><p:f xmlns:p="urn:p3"/></e></d></c></b></w>"#
    );
    assert_eq!(
        copies[1].1,
        r#"<w xmlns="urn:w" xmlns:p="urn:other"><b xmlns="urn:d" xmlns:p="urn:p2" p:k="1"><c xmlns=""><d><e><p:f xmlns:p="urn:p3"/></e></d></c></b></w>"#
    );
    assert_eq!(
        copies[0].0,
        "w | all: p=urn:other =urn:w xml=http://www.w3.org/XML/1998/namespace \
         | excl: p=urn:other =urn:w | local: p=urn:other =urn:w\n\
         b | all: p=urn:p2 =urn:d xml=http://www.w3.org/XML/1998/namespace \
         | excl: p=urn:p2 =urn:d | local: p=urn:p2 =urn:d\n\
         c | all: p=urn:p2 xml=http://www.w3.org/XML/1998/namespace \
         | excl: = p=urn:p2 | local: =\n\
         d | all: p=urn:p2 xml=http://www.w3.org/XML/1998/namespace \
         | excl: = p=urn:p2 | local: \n\
         e | all: q=urn:q p=urn:p2 xml=http://www.w3.org/XML/1998/namespace \
         | excl: q=urn:q = p=urn:p2 | local: q=urn:q\n\
         p:f | all: p=urn:p3 q=urn:q xml=http://www.w3.org/XML/1998/namespace \
         | excl: p=urn:p3 q=urn:q = | local: p=urn:p3\n"
    );
}

/// `copy_attribute` declares the prefix it needs on the open element after
/// the element has attributes; the element and every child added later see it.
#[test]
fn a_declaration_after_the_attributes_reaches_the_children() {
    let arena = Bump::new();
    let names = NameTable::new();
    let source = parse(
        r#"<s xmlns:p="urn:x" xmlns:z="urn:z" p:k="v" z:j="w"/>"#,
        &arena,
        &names,
    );
    let mut attr = elements(&source.create_navigator())[0].clone();
    assert!(attr.move_to_first_attribute());

    let mut target = builder(&arena, &names);
    target.start_element("out", "", "", &[]).expect("start out");
    target
        .start_element("in", "", "", &[("p", "urn:taken")])
        .expect("start in");
    target.attribute("plain", "", "", "1").expect("attribute");
    target
        .copy_attribute(&attr, CopyOptions::default())
        .expect("copies p:k");
    assert!(attr.move_to_next_attribute());
    target
        .copy_attribute(&attr, CopyOptions::default())
        .expect("copies z:j");
    target.end_of_attributes();
    for _ in 0..3 {
        target.start_element("kid", "", "", &[]).expect("start kid");
        target.end_of_attributes();
    }
    for _ in 0..3 {
        target.end_element().expect("end kid");
    }
    target.end_element().expect("end in");
    target
        .start_element("after", "", "", &[])
        .expect("start after");
    target.end_of_attributes();
    target.end_element().expect("end after");
    target.end_element().expect("end out");
    let doc = target.finalize().expect("finalize");

    assert_scopes_consistent(&doc);
    assert_eq!(
        listing(&doc),
        "out | all: xml=http://www.w3.org/XML/1998/namespace | excl:  | local: \n\
         in | all: z=urn:z ns0=urn:x p=urn:taken xml=http://www.w3.org/XML/1998/namespace \
         | excl: z=urn:z ns0=urn:x p=urn:taken | local: z=urn:z ns0=urn:x p=urn:taken\n\
         kid | all: z=urn:z ns0=urn:x p=urn:taken xml=http://www.w3.org/XML/1998/namespace \
         | excl: z=urn:z ns0=urn:x p=urn:taken | local: \n\
         kid | all: z=urn:z ns0=urn:x p=urn:taken xml=http://www.w3.org/XML/1998/namespace \
         | excl: z=urn:z ns0=urn:x p=urn:taken | local: \n\
         kid | all: z=urn:z ns0=urn:x p=urn:taken xml=http://www.w3.org/XML/1998/namespace \
         | excl: z=urn:z ns0=urn:x p=urn:taken | local: \n\
         after | all: xml=http://www.w3.org/XML/1998/namespace | excl:  | local: \n"
    );
    assert_eq!(
        serialize::to_string(&doc.create_navigator(), &SerializeOptions::default())
            .expect("serializes"),
        r#"<out><in xmlns:ns0="urn:x" xmlns:p="urn:taken" xmlns:z="urn:z" plain="1" ns0:k="v" z:j="w"><kid><kid><kid/></kid></kid></in><after/></out>"#
    );
}

/// The typed value of an `xs:QName` element resolves its prefix in the
/// element's scope: through a redeclaration, an undeclared default namespace
/// and a long run of levels that declare nothing.
#[test]
fn qname_typed_values_resolve_in_scope() {
    let mut schema_set = SchemaSet::new();
    load_and_process_schema(
        br#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                targetNamespace="urn:t" xmlns:t="urn:t" elementFormDefault="qualified">
              <xs:element name="r"><xs:complexType><xs:choice maxOccurs="unbounded">
                <xs:element ref="t:r"/><xs:element name="q" type="xs:QName"/>
              </xs:choice></xs:complexType></xs:element>
            </xs:schema>"#,
        "qnames.xsd",
        &mut schema_set,
        None,
    )
    .expect("schema loads");
    let mut text = String::from(
        r#"<t:r xmlns:t="urn:t" xmlns:p="urn:1" xmlns="urn:d1"><t:q>p:a</t:q><t:q>b</t:q><t:r xmlns:p="urn:2" xmlns=""><t:q>p:c</t:q><t:q>d</t:q>"#,
    );
    for _ in 0..50 {
        text.push_str("<t:r>");
    }
    text.push_str("<t:q>p:e</t:q><t:q>f</t:q>");
    for _ in 0..50 {
        text.push_str("</t:r>");
    }
    text.push_str("</t:r><t:q>p:g</t:q></t:r>");
    let arena = Bump::new();
    let doc = build_typed_document(
        text.as_bytes(),
        &arena,
        &schema_set,
        BufferDocumentOptions::default(),
    )
    .expect("typed build");
    assert_scopes_consistent(&doc);
    let ctx = XPathContext::new(&schema_set.name_table).with_schema_set(&schema_set);
    let expr = "string-join(for $q in //*:q return \
                concat('{', namespace-uri-from-QName(data($q)), '}', local-name-from-QName(data($q))), ' ')";
    let compiled = XPathExpr::compile(expr, &ctx).expect("compiles");
    let items = compiled
        .evaluator(&ctx)
        .run_with_node(doc.create_navigator())
        .expect("evaluates")
        .into_vec();
    let XmlItem::Atomic(value) = &items[0] else {
        panic!("a string")
    };
    assert_eq!(
        value.to_string_value(),
        "{urn:1}a {urn:d1}b {urn:2}c {}d {urn:2}e {}f {urn:1}g"
    );
}

// ── Deep trees: time linear in depth ────────────────────────────────────

/// How the elements of a deep tree declare namespaces.
#[derive(Clone, Copy, Debug)]
enum Shape {
    /// No declarations at all.
    None,
    /// The outermost element declares a default namespace and a prefix.
    Root,
    /// Every element declares the default namespace again (to the same URI),
    /// as some generators write it.
    Every,
}

/// `depth` nested `e` around a `v` holding `12`.
fn deep<'a>(
    arena: &'a Bump,
    names: &'a NameTable,
    shape: Shape,
    depth: usize,
) -> BufferDocument<'a> {
    let mut b = builder(arena, names);
    let ns = match shape {
        Shape::None => "",
        Shape::Root | Shape::Every => "urn:d",
    };
    for level in 0..depth {
        let decls: &[(&str, &str)] = match shape {
            Shape::Root if level == 0 => &[("", "urn:d"), ("p", "urn:p")],
            Shape::Every => &[("", "urn:d")],
            _ => &[],
        };
        b.start_element("e", ns, "", decls).expect("start e");
        b.end_of_attributes();
    }
    b.start_element("v", ns, "", &[]).expect("start v");
    b.end_of_attributes();
    b.text("12");
    b.end_element().expect("end v");
    for _ in 0..depth {
        b.end_element().expect("end e");
    }
    b.finalize().expect("finalize")
}

fn deep_schema(shape: Shape) -> SchemaSet {
    let text = match shape {
        Shape::None => {
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                 <xs:element name="e"><xs:complexType><xs:choice>
                   <xs:element ref="e"/><xs:element name="v" type="xs:int"/>
                 </xs:choice></xs:complexType></xs:element>
               </xs:schema>"#
        }
        Shape::Root | Shape::Every => {
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                   targetNamespace="urn:d" xmlns:d="urn:d" elementFormDefault="qualified">
                 <xs:element name="e"><xs:complexType><xs:choice>
                   <xs:element ref="d:e"/><xs:element name="v" type="xs:int"/>
                 </xs:choice></xs:complexType></xs:element>
               </xs:schema>"#
        }
    };
    let mut schema_set = SchemaSet::new();
    load_and_process_schema(text.as_bytes(), "deep.xsd", &mut schema_set, None)
        .expect("schema loads");
    schema_set
}

/// The walks over a whole tree that read every element's namespaces.
const WALKS: [&str; 5] = [
    "drive",
    "serialize",
    "copy",
    "namespace axis",
    "in-scope-prefixes",
];

/// Runs `walk` once over `doc` and returns what it found, so it is not
/// optimized away.
fn run_walk(walk: &str, doc: &BufferDocument<'_>, schema_set: &SchemaSet) -> usize {
    match walk {
        "drive" => {
            let (mut errors, mut warnings) = (Vec::new(), Vec::new());
            let outcome = {
                let validator = SchemaValidator::new(schema_set, ValidationFlags::default());
                let mut runtime = validator.start_run(CollectingValidationSink {
                    errors: &mut errors,
                    warnings: &mut warnings,
                });
                drive_buffer_document(doc, &mut runtime, schema_set).expect("drive")
            };
            assert!(
                errors.is_empty(),
                "{:?}",
                errors.first().map(ToString::to_string)
            );
            outcome.max_depth
        }
        "serialize" => {
            let mut out = Vec::new();
            serialize::serialize_document(
                &doc.create_navigator(),
                &mut out,
                &SerializeOptions::default(),
            )
            .expect("serializes");
            out.len()
        }
        "copy" => {
            let arena = Bump::new();
            let mut target = builder(&arena, doc.names());
            target
                .start_element("w", "urn:w", "", &[("", "urn:w"), ("p", "urn:other")])
                .expect("start w");
            target.end_of_attributes();
            target
                .copy_subtree(&doc.create_navigator(), CopyOptions::default())
                .expect("copies");
            target.end_element().expect("end w");
            let copy = target.finalize().expect("finalize");
            copy.create_navigator().value().len()
        }
        "namespace axis" => eval1(
            doc.names(),
            &doc.create_navigator(),
            "count(//namespace::*)",
        )
        .parse()
        .expect("a count"),
        "in-scope-prefixes" => eval1(
            doc.names(),
            &doc.create_navigator(),
            "sum(for $e in //* return count(in-scope-prefixes($e)))",
        )
        .parse()
        .expect("a count"),
        other => unreachable!("{other}"),
    }
}

/// The best of three runs of `walk` over a tree `depth` deep, in seconds.
fn best_time(walk: &str, shape: Shape, depth: usize, schema_set: &SchemaSet) -> f64 {
    let arena = Bump::new();
    let doc = deep(&arena, &schema_set.name_table, shape, depth);
    (0..3)
        .map(|_| {
            let start = Instant::now();
            std::hint::black_box(run_walk(walk, &doc, schema_set));
            start.elapsed().as_secs_f64()
        })
        .fold(f64::INFINITY, f64::min)
}

/// Each walk over 20,000 levels takes about twice as long as over 10,000: a
/// linear walk scores 2, one that visits every ancestor of every element 4.
/// The ratio is measured up to three times, so one slow run on a busy machine
/// does not fail the test.
fn assert_linear_in_depth(shape: Shape) {
    let schema_set = deep_schema(shape);
    let mut report = Vec::new();
    for walk in WALKS {
        let mut ratios = Vec::new();
        for _ in 0..3 {
            let small = best_time(walk, shape, 10_000, &schema_set);
            let large = best_time(walk, shape, 20_000, &schema_set);
            let ratio = large / small.max(1e-6);
            ratios.push(format!("{ratio:.2} ({small:.4} s → {large:.4} s)"));
            if ratio < 3.0 {
                break;
            }
            if ratios.len() == 3 {
                report.push(format!("{walk}: {}", ratios.join(", ")));
            }
        }
    }
    assert!(
        report.is_empty(),
        "{shape:?}: time ratio 20,000 / 10,000 levels not below 3:\n{}",
        report.join("\n")
    );
}

#[test]
fn walks_without_declarations_are_linear_in_depth() {
    assert_linear_in_depth(Shape::None);
}

#[test]
fn walks_below_one_declaring_element_are_linear_in_depth() {
    assert_linear_in_depth(Shape::Root);
}

#[test]
fn walks_where_every_element_declares_are_linear_in_depth() {
    assert_linear_in_depth(Shape::Every);
}
