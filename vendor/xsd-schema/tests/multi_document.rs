#![cfg(feature = "xsd11")]
//! Cross-document node identity, order and set operations.
//!
//! Every navigator cursor in this crate is a set of indices *into one
//! document*, so a node's identity is `(document, cursor)` — never the cursor
//! alone. These tests pin that down from the outside, through compiled XPath
//! expressions with two external variables bound to two **different**
//! documents whose content is byte-identical (so that their corresponding
//! nodes have identical indices, the case a document-blind comparison gets
//! wrong).
//!
//! The behaviour under test comes from three places in XPath 2.0:
//!
//! * §3.5.3 *Node Comparisons* — "A comparison with the `is` operator is true
//!   if the two operand nodes have the same identity, and are thus the same
//!   node; otherwise it is `false`."
//! * §3.3.3 *Combining Node Sequences* — "All these operators eliminate
//!   duplicate nodes from their result sequences based on node identity. The
//!   resulting sequence is returned in document order."
//! * §2.4.1 *Document Order* — "The relative order of nodes in distinct trees
//!   is stable but implementation-dependent, subject to the following
//!   constraint: If any node in a given tree T1 is before any node in a
//!   different tree T2, then all nodes in tree T1 are before all nodes in
//!   tree T2."

use bumpalo::Bump;
use xsd_schema::document::BufferDocument;
use xsd_schema::namespace::NameTable;
use xsd_schema::xpath::api::XPathExpr;
use xsd_schema::xpath::{DomNavigator, RoXmlNavigator, XPathContext, XPathValue};

/// Both documents get this content, so a document-blind identity test would
/// see `$a/root` and `$b/root` as the same node.
const XML: &str = "<root><x/><x/></root>";

/// Evaluate `expr` with `$a` and `$b` bound to the two document nodes, and
/// with `$a`'s document node as the context item.
fn eval<N: DomNavigator>(expr_text: &str, names: &NameTable, a: N, b: N) -> XPathValue<N> {
    let ctx = XPathContext::new(names);
    let expr = XPathExpr::compile_with_vars(expr_text, &ctx, &["a", "b"])
        .unwrap_or_else(|e| panic!("compile `{expr_text}`: {e}"));
    let context_node = a.clone();
    expr.evaluator(&ctx)
        .run_with_node_and_setup(Some(context_node), |typed_eval| {
            typed_eval
                .set_variable_by_name("a", XPathValue::from_node(a))
                .expect("bind $a");
            typed_eval
                .set_variable_by_name("b", XPathValue::from_node(b))
                .expect("bind $b");
        })
        .unwrap_or_else(|e| panic!("evaluate `{expr_text}`: {e}"))
}

/// The assertions, run once per navigator backend.
fn assert_cross_document_semantics<N: DomNavigator>(names: &NameTable, a: N, b: N) {
    // §3.5.3: two different nodes, however equal their cursors.
    assert_eq!(
        eval("$a/root is $b/root", names, a.clone(), b.clone()).as_bool(),
        Some(false),
        "`$a/root is $b/root` must be false: they are nodes of different documents"
    );
    // …and a node is still itself.
    assert_eq!(
        eval("$a/root is $a/root", names, a.clone(), b.clone()).as_bool(),
        Some(true),
        "`$a/root is $a/root` must be true"
    );
    assert_eq!(
        eval("$b/root is $b/root", names, a.clone(), b.clone()).as_bool(),
        Some(true),
        "`$b/root is $b/root` must be true"
    );
    // The document nodes themselves are distinct too.
    assert_eq!(
        eval("$a is $b", names, a.clone(), b.clone()).as_bool(),
        Some(false),
        "the two document nodes are distinct"
    );

    // §3.3.3: `|` dedups *by node identity*, so nothing may collapse across
    // the two documents — 2 + 2 = 4.
    assert_eq!(
        eval("count($a/root/x)", names, a.clone(), b.clone()).as_f64(),
        Some(2.0)
    );
    assert_eq!(
        eval("count($a/root/x | $b/root/x)", names, a.clone(), b.clone()).as_f64(),
        Some(4.0),
        "union is by identity: all four `x` elements survive"
    );
    assert_eq!(
        eval("count($a/root/x | $a/root/x)", names, a.clone(), b.clone()).as_f64(),
        Some(2.0),
        "within one document the union still dedups"
    );
    // intersect / except over disjoint documents.
    assert_eq!(
        eval(
            "count($a/root/x intersect $b/root/x)",
            names,
            a.clone(),
            b.clone()
        )
        .as_f64(),
        Some(0.0),
        "no node is in both documents"
    );
    assert_eq!(
        eval(
            "count($a/root/x except $b/root/x)",
            names,
            a.clone(),
            b.clone()
        )
        .as_f64(),
        Some(2.0),
        "`except` may not remove same-index nodes of the other document"
    );

    // Sequence construction keeps both items (no dedup involved, but it would
    // break the same way if identity were document-blind downstream).
    assert_eq!(
        eval("count(($a/root, $b/root))", names, a.clone(), b.clone()).as_f64(),
        Some(2.0)
    );

    // §2.4.1: tree order is a block order — one whole tree, then the other,
    // and it agrees with `<<` / `>>` in both directions.
    let a_first = eval("$a/root << $b/root", names, a.clone(), b.clone())
        .as_bool()
        .expect("`<<` must yield a boolean");
    assert_eq!(
        eval("$b/root >> $a/root", names, a.clone(), b.clone()).as_bool(),
        Some(a_first),
        "`<<` and `>>` must agree"
    );
    let block_order = if a_first {
        "every $n in $a/root/x satisfies (every $m in $b/root/x satisfies $n << $m)"
    } else {
        "every $n in $a/root/x satisfies (every $m in $b/root/x satisfies $n >> $m)"
    };
    assert_eq!(
        eval(block_order, names, a.clone(), b.clone()).as_bool(),
        Some(true),
        "if one node of T1 precedes one node of T2, all of T1 precedes all of T2"
    );

    // Document order over the union is total and duplicate-free: sorting four
    // nodes from two trees must still yield four.
    assert_eq!(
        eval(
            "count(($a/root/x | $b/root/x)[. is ($a/root/x | $b/root/x)[1]])",
            names,
            a.clone(),
            b.clone()
        )
        .as_f64(),
        Some(1.0),
        "the first node of the union is identical to exactly one union member"
    );
}

#[test]
fn buffer_doc_navigator_cross_document_identity() {
    // One arena and one name table for both documents: nothing about node
    // identity may depend on the allocator or on the interning table — only
    // on which `BufferDocument` the node belongs to.
    let arena = Bump::new();
    let names = NameTable::new();
    let doc_a = BufferDocument::from_reader_default(XML.as_bytes(), &arena, &names).unwrap();
    let doc_b = BufferDocument::from_reader_default(XML.as_bytes(), &arena, &names).unwrap();

    // The documents really are indistinguishable by cursor.
    let nav_a = doc_a.create_navigator();
    let nav_b = doc_b.create_navigator();
    assert_eq!(nav_a.current_ref(), nav_b.current_ref());
    assert!(!nav_a.is_same_position(&nav_b));
    // A3: cross-document order follows the creation ordinal.
    assert!(doc_a.serial() < doc_b.serial());

    assert_cross_document_semantics(&names, nav_a, nav_b);
}

#[test]
fn roxml_navigator_cross_document_identity() {
    let parsed_a = roxmltree::Document::parse(XML).unwrap();
    let parsed_b = roxmltree::Document::parse(XML).unwrap();

    let nav_a = RoXmlNavigator::new(&parsed_a);
    let nav_b = RoXmlNavigator::new(&parsed_b);
    assert!(!nav_a.is_same_position(&nav_b));

    assert_cross_document_semantics(&names_table(), nav_a, nav_b);
}

/// `RoXmlNavigator` needs no name table of its own; the XPath static context
/// does.
fn names_table() -> NameTable {
    NameTable::new()
}
