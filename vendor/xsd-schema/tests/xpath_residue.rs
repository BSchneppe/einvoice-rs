#![cfg(feature = "xsd11")]
//! XPath engine residue: `fn:subsequence` argument types (C12), the substring
//! functions and collations (C13), unclosed comments (C6), `fn:base-uri` (C7),
//! XPath 1.0 compatibility mode (C8) and `fn:number` in the XPath 1.0
//! language mode (C9).

use xsd_schema::namespace::context::NamespaceContextSnapshot;
use xsd_schema::namespace::table::NameTable;
use xsd_schema::xpath::functions::{XPath10Catalog, XPath10Evaluator};
use xsd_schema::xpath::{DomNavigator, RoXmlNavigator, XPathContext, XPathExpr, XPathMode};
use xsd_schema::XmlItem;

const XS: &str = "http://www.w3.org/2001/XMLSchema";

fn render<N: DomNavigator>(value: xsd_schema::xpath::XPathValue<N>) -> String {
    value
        .into_vec()
        .into_iter()
        .map(|item| match item {
            XmlItem::Atomic(a) => a.to_string_value(),
            XmlItem::Node(n) => format!("<{}>", n.name()),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn namespaces(names: &NameTable) -> NamespaceContextSnapshot {
    let mut ns = NamespaceContextSnapshot::default();
    ns.bindings.push((names.add("xs"), names.add(XS)));
    ns
}

/// Evaluate in XPath 2.0 (`compat`: with XPath 1.0 compatibility mode).
fn eval_in(compat: bool, expr: &str) -> String {
    let names = NameTable::new();
    let ctx = XPathContext::new(&names)
        .with_namespaces(namespaces(&names))
        .with_xpath10_compatibility(compat);
    let compiled = match XPathExpr::compile(expr, &ctx) {
        Ok(c) => c,
        Err(e) => return format!("ERR:{}", e.error_code().unwrap_or("?")),
    };
    match compiled.evaluator(&ctx).run::<RoXmlNavigator<'static>>() {
        Ok(v) => render(v),
        Err(e) => format!("ERR:{}", e.error_code().unwrap_or("?")),
    }
}

fn eval(expr: &str) -> String {
    eval_in(false, expr)
}

/// Evaluate in the XPath 1.0 language mode, with a context node from `xml`
/// when given.
fn eval_xpath10(xml: Option<&str>, expr: &str) -> String {
    let names = NameTable::new();
    let catalog = XPath10Catalog;
    let ctx = XPathContext::new(&names)
        .with_mode(XPathMode::XPath10)
        .with_function_catalog(&catalog);
    let compiled = match XPathExpr::compile(expr, &ctx) {
        Ok(c) => c,
        Err(e) => return format!("ERR:{}", e.error_code().unwrap_or("?")),
    };
    let evaluator = XPath10Evaluator;
    let doc = xml.map(|x| roxmltree::Document::parse(x).expect("well-formed"));
    let node = doc.as_ref().map(RoXmlNavigator::new);
    match compiled
        .evaluator(&ctx)
        .run_with_node_and_setup(node, |ev| {
            ev.context().set_function_evaluator(Some(&evaluator));
        }) {
        Ok(v) => render(v),
        Err(e) => format!("ERR:{}", e.error_code().unwrap_or("?")),
    }
}

fn check(rows: &[(&str, &str)], eval: impl Fn(&str) -> String) {
    let failures: Vec<String> = rows
        .iter()
        .filter_map(|(expr, expected)| {
            let got = eval(expr);
            (got != *expected).then(|| format!("{expr}\n    expected {expected}, got {got}"))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// ---------------------------------------------------------------------------
// C12: fn:subsequence($sourceSeq as item()*, $startingLoc as xs:double)
// ---------------------------------------------------------------------------

/// F&O 1.0 §15.1.10: `$startingLoc as xs:double` and `$length as xs:double`
/// take exactly one value; the empty sequence is a type error (XPath 2.0
/// §3.1.5, XPTY0004), outside XPath 1.0 compatibility mode, where the
/// function conversion rules apply `fn:number` (NaN, so the result is empty).
#[test]
fn subsequence_needs_its_numeric_arguments() {
    check(
        &[
            ("subsequence((1, 2, 3), ())", "ERR:XPTY0004"),
            ("subsequence((1, 2, 3), 1, ())", "ERR:XPTY0004"),
            ("subsequence((), ())", "ERR:XPTY0004"),
            ("subsequence((1, 2, 3), (1, 2))", "ERR:XPTY0004"),
            ("subsequence((1, 2, 3), 2)", "2 3"),
            ("subsequence((1, 2, 3), 2, 1)", "2"),
            ("subsequence((1, 2, 3), xs:untypedAtomic('2'))", "2 3"),
            ("subsequence((1, 2, 3), 1.6, 1)", "2"),
            ("subsequence((), 1)", ""),
        ],
        eval,
    );
    check(
        &[
            ("subsequence((1, 2, 3), ())", ""),
            ("subsequence((1, 2, 3), 1, ())", ""),
            ("subsequence((1, 2, 3), (2, 1))", "2 3"),
        ],
        |e| eval_in(true, e),
    );
}

// ---------------------------------------------------------------------------
// C13: the substring functions
// ---------------------------------------------------------------------------

/// F&O 1.0 §7.3.1: an explicit collation that "is not supported by the
/// implementation" raises FOCH0002, so the zero-length cases of §7.5 never
/// hide an unsupported URI.
#[test]
fn an_unsupported_collation_is_foch0002_before_the_zero_length_cases() {
    let uri = "http://example.com/no-such-collation";
    let rows: Vec<String> = [
        "contains('abc', '', '{U}')",
        "contains('', 'a', '{U}')",
        "contains((), (), '{U}')",
        "starts-with('abc', '', '{U}')",
        "starts-with((), 'a', '{U}')",
        "ends-with('abc', (), '{U}')",
        "substring-before('abc', '', '{U}')",
        "substring-after('', '', '{U}')",
    ]
    .iter()
    .map(|t| t.replace("{U}", uri))
    .collect();
    let rows: Vec<(&str, &str)> = rows.iter().map(|e| (e.as_str(), "ERR:FOCH0002")).collect();
    check(&rows, eval);
}

// ---------------------------------------------------------------------------
// C6: comments
// ---------------------------------------------------------------------------

/// XPath 2.0 §A.2.4 / §3.1.5 `Comment ::= "(:" (CommentContents | Comment)* ":)"`:
/// a comment ends at its `:)`, comments nest, and an unclosed one is a syntax
/// error (XPST0003), not the rest of the expression swallowed. `(:` inside a
/// string literal is part of the string.
#[test]
fn an_unclosed_comment_is_a_syntax_error() {
    check(
        &[
            ("1 (: unterminated", "ERR:XPST0003"),
            ("1 (:", "ERR:XPST0003"),
            ("(: never closed 1", "ERR:XPST0003"),
            ("1 (: outer (: inner :) still open", "ERR:XPST0003"),
            ("1 (: a :) + (: b", "ERR:XPST0003"),
            ("1 (: a :)", "1"),
            ("(: a :) 1", "1"),
            ("1 (: outer (: inner :) outer :) + 1", "2"),
            ("1 (: a :) + (: b :) 2", "3"),
            ("'(: not a comment'", "(: not a comment"),
            ("concat('(:', ':)')", "(::)"),
            ("concat(\"a (: b\", '')", "a (: b"),
            ("string-length('(:')", "2"),
            ("1 (: ':)' :)", "ERR:XPST0003"),
        ],
        eval,
    );
}

// ---------------------------------------------------------------------------
// C7: fn:base-uri
// ---------------------------------------------------------------------------

/// XDM 1.0 §5.2 / XML Base: an element's base URI is its `xml:base` resolved
/// against its parent's base URI (RFC 3986 §5.2, which removes dot segments,
/// §5.2.4), down from the document's base URI.
#[test]
fn base_uri_resolves_xml_base_with_dot_segments_removed() {
    let xml = r#"<r xml:base="../c/./d/"><e xml:base="e/../f.xml"><g/></e><h xml:base="/abs/./x/../y/"/><i xml:base="http://other.example/p/q/../r"/></r>"#;
    let doc = roxmltree::Document::parse(xml).expect("well-formed");
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let run = |base: &str, expr: &str| {
        let nav = RoXmlNavigator::with_base_uri(&doc, base);
        render(
            XPathExpr::compile(expr, &ctx)
                .expect("compiles")
                .evaluator(&ctx)
                .run_with_node(nav)
                .expect("evaluates"),
        )
    };
    let base = "http://example.com/a/b/doc.xml";
    for (expr, expected) in [
        ("string(base-uri(/))", "http://example.com/a/b/doc.xml"),
        ("string(base-uri(/r))", "http://example.com/a/c/d/"),
        ("string(base-uri(/r/e))", "http://example.com/a/c/d/f.xml"),
        ("string(base-uri(/r/e/g))", "http://example.com/a/c/d/f.xml"),
        ("string(base-uri(/r/h))", "http://example.com/abs/y/"),
        ("string(base-uri(/r/i))", "http://other.example/p/r"),
        // An attribute has its parent's base URI (XDM 1.0 §6.4.2).
        (
            "string(base-uri(/r/e/@xml:base))",
            "http://example.com/a/c/d/f.xml",
        ),
    ] {
        assert_eq!(run(base, expr), expected, "{expr}");
    }
    // The document's own URI with dot segments.
    assert_eq!(
        run(
            "http://example.com/a/./b/../doc.xml",
            "string(base-uri(/r))"
        ),
        "http://example.com/c/d/"
    );
}

/// Without a document base URI, `xml:base` resolves against the static base
/// URI (XPath 2.0 §C.1).
#[test]
fn base_uri_falls_back_to_the_static_base_uri() {
    let xml = r#"<r xml:base="../x/./y/"><e/></r>"#;
    let doc = roxmltree::Document::parse(xml).expect("well-formed");
    let names = NameTable::new();
    let ctx = XPathContext::new(&names).with_base_uri("http://example.com/a/b/c");
    let value = XPathExpr::compile("string(base-uri(/r/e))", &ctx)
        .expect("compiles")
        .evaluator(&ctx)
        .run_with_node(RoXmlNavigator::new(&doc))
        .expect("evaluates");
    assert_eq!(render(value), "http://example.com/a/x/y/");
}

/// The same on a `BufferDocument`, whose navigator reports the `xml:base` in
/// scope as written — `fn:base-uri` resolves it.
#[test]
fn base_uri_on_a_buffer_document() {
    use bumpalo::Bump;
    use xsd_schema::document::BufferDocument;
    let arena = Bump::new();
    let names = NameTable::new();
    let mut doc = BufferDocument::from_reader_default(
        br#"<r xml:base="../c/./d/"><e xml:base="e/../f.xml"><g/></e></r>"#.as_slice(),
        &arena,
        &names,
    )
    .expect("builds");
    doc.set_document_base_uri(Some("http://example.com/a/b/doc.xml"));
    let ctx = XPathContext::new(&names);
    for (expr, expected) in [
        ("string(base-uri(/r))", "http://example.com/a/c/d/"),
        ("string(base-uri(/r/e/g))", "http://example.com/a/c/d/f.xml"),
    ] {
        let value = XPathExpr::compile(expr, &ctx)
            .expect("compiles")
            .evaluator(&ctx)
            .run_with_node(doc.create_navigator())
            .expect("evaluates");
        assert_eq!(render(value), expected, "{expr}");
    }
}

// ---------------------------------------------------------------------------
// C8: XPath 1.0 compatibility mode, general comparisons
// ---------------------------------------------------------------------------

/// XPath 2.0 §3.5.2 with XPath 1.0 compatibility mode true: after the
/// boolean rule (a *single atomic* boolean operand converts the other operand
/// to its effective boolean value) and atomization, a pair is compared as
/// numbers when either value is numeric, as strings when both are
/// `xs:untypedAtomic`, with an `xs:untypedAtomic` cast to the other value's
/// type otherwise — and then with `eq`, `ne`, …, so two values of
/// incomparable types are a type error, as without compatibility mode.
#[test]
fn compatibility_mode_general_comparisons_follow_3_5_2() {
    let compat = |e: &str| eval_in(true, e);
    check(
        &[
            // The boolean rule applies to a single atomic operand.
            ("xs:boolean('true') = 'false'", "true"),
            ("'false' = xs:boolean('true')", "true"),
            ("xs:boolean('false') = ()", "true"),
            // After atomization, a boolean in a longer sequence is an atomic
            // value like any other.
            (
                "(xs:boolean('true'), 2) = xs:untypedAtomic('false')",
                "false",
            ),
            ("(xs:boolean('true'), 2) = xs:untypedAtomic('true')", "true"),
            ("(xs:boolean('true'), 2) = 'false'", "ERR:XPTY0004"),
            ("(xs:boolean('true'), 2) = 2", "true"),
            // Numbers: fn:number of the other value.
            ("(1, 2) = xs:untypedAtomic('2.0')", "true"),
            ("(1, 2) = '2'", "true"),
            ("(1, 2) = 'two'", "false"),
            ("('10', '9') > 5", "true"),
            ("('a', 'b') < 1", "false"),
            // Typed non-numeric values keep their types.
            (
                "(xs:date('2000-01-01Z'), 1) = xs:untypedAtomic('2000-01-01+00:00')",
                "true",
            ),
            (
                "(xs:date('2000-01-01'), 1) = xs:untypedAtomic('2000-01-01')",
                "true",
            ),
            ("(xs:date('2000-01-01'), 1) = '2000-01-01'", "ERR:XPTY0004"),
            ("(xs:date('2000-01-01'), 1) = (1, '2000-01-01')", "true"),
            (
                "(xs:dayTimeDuration('PT1H'), 1) = xs:untypedAtomic('PT60M')",
                "true",
            ),
            // Two untypedAtomic values: strings.
            ("xs:untypedAtomic('1.0') = xs:untypedAtomic('1')", "false"),
            ("xs:untypedAtomic('a') != xs:untypedAtomic('b')", "true"),
            // Strings under the default collation, as before.
            ("('a', 'b') = 'b'", "true"),
            ("('a', 'b') != 'a'", "true"),
        ],
        compat,
    );
}

// ---------------------------------------------------------------------------
// C9 (its last clause): fn:number in the XPath 1.0 language mode
// ---------------------------------------------------------------------------

/// XPath 1.0 §4.4: "a string that consists of optional whitespace followed by
/// an optional minus sign followed by a Number followed by whitespace is
/// converted to the IEEE 754 number that is nearest … to the mathematical
/// value represented by the string; any other string is converted to NaN",
/// with `Number ::= Digits ('.' Digits?)? | '.' Digits` (§3.7) — no exponent,
/// no `+`, no `INF`.
#[test]
fn xpath10_number_follows_xpath_1_0() {
    check(
        &[
            ("number('12')", "12"),
            ("number('  -1.5  ')", "-1.5"),
            ("number('.5')", "0.5"),
            ("number('5.')", "5"),
            ("number('-.5')", "-0.5"),
            ("number('1e3')", "NaN"),
            ("number('1E3')", "NaN"),
            ("number('+5')", "NaN"),
            ("number('inf')", "NaN"),
            ("number('INF')", "NaN"),
            ("number('-INF')", "NaN"),
            ("number('NaN')", "NaN"),
            ("number('')", "NaN"),
            ("number('- 5')", "NaN"),
            ("number('5 5')", "NaN"),
            ("number('0x10')", "NaN"),
            // The conversions that apply fn:number.
            ("'1e3' + 0", "NaN"),
            ("'12' + 0", "12"),
            ("'1e3' < 2000", "false"),
            ("'+5' = 5", "false"),
            ("' 5 ' = 5", "true"),
        ],
        |e| eval_xpath10(None, e),
    );
    // A node's string value.
    check(
        &[
            ("number(/r/a)", "NaN"),
            ("number(/r/b)", "42"),
            ("number(/r/c)", "NaN"),
            ("/r/a + 0", "NaN"),
            ("/r/b + 0", "42"),
            ("/r/a > 999", "false"),
            ("sum(/r/b)", "42"),
        ],
        |e| eval_xpath10(Some("<r><a>1e3</a><b> 42 </b><c>INF</c></r>"), e),
    );
}

/// XPath 2.0 keeps its own `fn:number` (a cast to `xs:double`), with and
/// without compatibility mode.
#[test]
fn xpath20_number_is_the_double_cast() {
    for compat in [false, true] {
        check(
            &[
                ("number('1e3')", "1000"),
                ("number('INF')", "INF"),
                ("number('inf')", "NaN"),
            ],
            |e| eval_in(compat, e),
        );
    }
}
