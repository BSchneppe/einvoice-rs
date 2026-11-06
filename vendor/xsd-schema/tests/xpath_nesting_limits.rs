//! XPath expressions are bounded in nesting, so that a deeply nested
//! expression — in a host's XPath, an `xs:assert` or an `xs:alternative` —
//! is refused with an error instead of overflowing the stack (C17, fix
//! round F2).
//!
//! The parser measures the nesting depth of every expression it parses and
//! refuses one deeper than `xpath::depth::MAX_EXPRESSION_DEPTH`
//! (`XPathError::LimitExceeded`), before binding or evaluation recurse over
//! it. A schema whose assertion, assertion facet or type-alternative test is
//! deeper is refused when it is read (`SchemaError::LimitExceeded`, limit
//! `SchemaLimit::XPathDepth`) — assertions are otherwise compiled only when an
//! instance is validated. Identity-constraint selectors and fields have no
//! nesting (their grammar is a union of paths). A schema's tests are
//! checked when the schema document is read.
//!
//! Refusals run on a 2 MiB thread. An expression *within* the limit is
//! compiled and evaluated on a larger thread: a debug build's binder and
//! evaluator frames are about 40 KiB per nesting level (a release build's
//! under 2 KiB).
#![cfg(feature = "xsd11")]

use xsd_schema::namespace::table::NameTable;
use xsd_schema::xpath::api::XPathExpr;
use xsd_schema::xpath::depth::MAX_EXPRESSION_DEPTH;
use xsd_schema::xpath::error::XPathError;
use xsd_schema::xpath::XPathContext;
use xsd_schema::{SchemaError, SchemaLimit, SchemaSetBuilder, XsdVersion};

const XS: &str = r#"xmlns:xs="http://www.w3.org/2001/XMLSchema""#;

fn on_stack<T: Send + 'static>(bytes: usize, f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(bytes)
        .spawn(f)
        .unwrap()
        .join()
        .expect("no stack overflow, no panic")
}

/// The shapes of the review (F2): `n` levels of each construct.
fn shape(kind: &str, n: usize) -> String {
    match kind {
        "paren" => format!("{}@x = 1{}", "(".repeat(n), ")".repeat(n)),
        "not" => format!("{}@x{}", "not(".repeat(n), ")".repeat(n)),
        "pred" => format!("{}true(){}", "a[".repeat(n), "]".repeat(n)),
        "if" => format!("{}true()", "if (@x) then true() else ".repeat(n)),
        "or" => format!("{}true()", "false() or ".repeat(n)),
        "plus" => format!("{}1 = {}", "1 + ".repeat(n), n + 1),
        "neg" => format!("{}1 = 1", "- - ".repeat(n / 2)),
        other => panic!("{other}"),
    }
}

const KINDS: [&str; 7] = ["paren", "not", "pred", "if", "or", "plus", "neg"];

fn compile(expr: String) -> Result<(), XPathError> {
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    XPathExpr::compile(&expr, &ctx).map(|_| ())
}

#[test]
fn deep_expressions_are_refused_by_the_parser() {
    for kind in KINDS {
        for n in [1_000, 10_000, 100_000] {
            let expr = shape(kind, n);
            let result = on_stack(2 << 20, move || compile(expr));
            match result {
                Err(XPathError::LimitExceeded { max, found, .. }) => {
                    assert_eq!(max, MAX_EXPRESSION_DEPTH, "{kind} {n}");
                    assert!(found > max, "{kind} {n}: {found}");
                }
                other => panic!("{kind} {n}: {other:?}"),
            }
        }
    }
    let text = compile(shape("paren", 1_000)).unwrap_err().to_string();
    assert!(
        text.contains("MAX_EXPRESSION_DEPTH") && text.contains(&MAX_EXPRESSION_DEPTH.to_string()),
        "{text}"
    );
}

/// Long but flat expressions are not nested: paths and sequences.
#[test]
fn long_flat_expressions_are_not_refused() {
    let path = format!("count({}a)", "a/".repeat(20_000));
    let seq = format!("count(({}1))", "1, ".repeat(20_000));
    for expr in [path, seq] {
        on_stack(2 << 20, move || compile(expr)).expect("flat");
    }
}

/// Within the limit, an expression compiles and evaluates. (On a large
/// thread: a debug build's binder and evaluator frames are about 40 KiB a
/// nesting level, a release build's under 2 KiB.)
#[test]
fn expressions_within_the_limit_evaluate() {
    on_stack(512 << 20, || {
        for kind in KINDS {
            // The deepest n whose expression is within the limit.
            let n = (1..MAX_EXPRESSION_DEPTH)
                .rev()
                .find(|&n| compile(shape(kind, n)).is_ok())
                .expect("some depth fits");
            assert!(n >= MAX_EXPRESSION_DEPTH / 4, "{kind}: only {n}");
            let expr = shape(kind, n);
            let names = NameTable::new();
            let ctx = XPathContext::new(&names);
            let compiled = XPathExpr::compile(&expr, &ctx).unwrap();
            let doc = roxmltree::Document::parse(r#"<r x="1"><a/></r>"#).unwrap();
            let nav = xsd_schema::RoXmlNavigator::new(&doc);
            compiled
                .evaluator(&ctx)
                .run_with_node(nav)
                .map(|_| ())
                .unwrap_or_else(|e| panic!("{kind} {n}: {e}"));
        }
    });
}

fn schema_with(test_site: &str) -> Result<(), SchemaError> {
    let xsd = format!(r#"<xs:schema {XS}>{test_site}</xs:schema>"#);
    SchemaSetBuilder::with_version(XsdVersion::V1_1)
        .add_source(&xsd, "/xpath-limits/main.xsd")?
        .compile()
        .map(|_| ())
}

fn assert_schema_refused(what: &str, site: String) {
    let result = on_stack(2 << 20, move || schema_with(&site));
    match &result {
        Err(
            e @ SchemaError::LimitExceeded {
                limit: SchemaLimit::XPathDepth,
                max: MAX_EXPRESSION_DEPTH,
                ..
            },
        ) => {
            // Located at the element whose test it is, also through the
            // variant-independent accessor.
            let location = e.location().unwrap_or_else(|| panic!("{what}: {e}"));
            assert!(location.line >= 1, "{what}: {e}");
            let text = e.to_string();
            assert!(
                text.contains("xpath::depth::MAX_EXPRESSION_DEPTH"),
                "{text}"
            );
        }
        other => panic!("{what}: {other:?}"),
    }
}

/// The review's reproducers: an `xs:alternative` test of 1,000 nested
/// parentheses aborted at schema compile time; an `xs:assert` test compiled
/// and aborted when an instance was validated.
#[test]
fn deep_schema_xpath_is_refused_when_the_schema_is_read() {
    for kind in KINDS {
        let e = shape(kind, 1_000);
        assert_schema_refused(
            &format!("alternative {kind}"),
            format!(
                r#"<xs:element name="r" type="xs:anyType"><xs:alternative test="{e}" type="xs:anyType"/></xs:element>"#
            ),
        );
        assert_schema_refused(
            &format!("assert {kind}"),
            format!(
                r#"<xs:complexType name="T"><xs:attribute name="x"/><xs:assert test="{e}"/></xs:complexType>"#
            ),
        );
        assert_schema_refused(
            &format!("assertion facet {kind}"),
            format!(
                r#"<xs:simpleType name="S"><xs:restriction base="xs:string"><xs:assertion test="{e}"/></xs:restriction></xs:simpleType>"#
            ),
        );
    }
}

/// An identity constraint's selector and fields are unions of paths: long,
/// never nested, and accepted.
#[test]
fn identity_constraint_xpath_is_flat() {
    let selector = format!("{}a", "a/".repeat(10_000));
    let site = format!(
        r#"<xs:element name="r"><xs:complexType><xs:sequence><xs:any processContents="skip" minOccurs="0"/></xs:sequence></xs:complexType>
           <xs:key name="k"><xs:selector xpath="{selector}"/><xs:field xpath="@id"/></xs:key></xs:element>"#
    );
    on_stack(2 << 20, move || schema_with(&site)).expect("a long selector");
}
