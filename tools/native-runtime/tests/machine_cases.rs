mod support;
use einvoice_native_runtime::machine::{EntryFocus, ItemRef, Prepared, Run};
use einvoice_native_runtime::Error;
use einvoice_source::SourceDocument;
use support::*;
use validation_ir::Expr;

// These are exact authored ASTs, not a test XPath parser. IDs link to committed
// independent XPath inputs and fresh pinned observations.
fn cases() -> Vec<(&'static str, Expr)> {
    let a1 = || filter(path(&["r", "a"]), n("1"));
    let ancestor = || {
        bin(
            "/",
            bin("/", bin("/", a1(), step("child", "c")), step("child", "z")),
            step("ancestor", "*"),
        )
    };
    let preceding = || {
        bin(
            "/",
            bin("/", a1(), step("child", "c")),
            step("preceding-sibling", "*"),
        )
    };
    vec![
        (
            "float-promote",
            bin(
                "eq",
                bin("+", cast("float", s("16777216")), n("1")),
                cast("float", s("16777216")),
            ),
        ),
        (
            "float-double",
            bin("+", cast("float", s("0.1")), cast("double", s("0.2"))),
        ),
        (
            "double-decimal",
            bin(
                "+",
                cast("decimal", s("9007199254740993")),
                cast("double", s("0")),
            ),
        ),
        (
            "untyped-pair",
            bin("=", path(&["r", "n"]), path(&["r", "n"])),
        ),
        (
            "untyped-string",
            bin("=", path(&["r", "n"]), s("9007199254740993")),
        ),
        (
            "untyped-decimal-general",
            bin(
                "=",
                path(&["r", "n"]),
                cast("decimal", s("9007199254740992")),
            ),
        ),
        (
            "untyped-value",
            bin(
                "eq",
                path(&["r", "n"]),
                cast("decimal", s("9007199254740993")),
            ),
        ),
        (
            "atomized-types",
            seq(vec![
                call("data", vec![path(&["r", "bad"])]),
                call("data", vec![path(&["r", "empty"])]),
                call("data", vec![path(&["r", "missing"])]),
                call("data", vec![path(&["r", "split"])]),
            ]),
        ),
        (
            "node-comparison",
            seq(vec![
                bin("is", a1(), a1()),
                bin("<<", a1(), filter(path(&["r", "a"]), n("2"))),
                bin(">>", filter(path(&["r", "a"]), n("2")), a1()),
                bin("is", seq(vec![]), a1()),
            ]),
        ),
        ("value-many", bin("eq", seq(vec![n("1"), n("2")]), n("1"))),
        (
            "general-many",
            seq(vec![
                bin("=", seq(vec![n("1"), n("2")]), seq(vec![n("2"), n("3")])),
                bin("!=", seq(vec![n("1"), n("2")]), seq(vec![n("1"), n("2")])),
                bin("=", seq(vec![]), n("1")),
                bin("eq", seq(vec![]), n("1")),
            ]),
        ),
        (
            "empty-arithmetic",
            seq(vec![
                call("count", vec![bin("+", seq(vec![]), n("1"))]),
                call("count", vec![cast("decimal", seq(vec![]))]),
                call("number", vec![seq(vec![])]),
            ]),
        ),
        (
            "empty-cast",
            typeop("castable", seq(vec![]), "integer", Some("?")),
        ),
        (
            "many-castable",
            typeop("castable", seq(vec![n("1"), n("2")]), "integer", None),
        ),
        (
            "bad-castable",
            seq(vec![
                typeop("castable", path(&["r", "bad"]), "decimal", None),
                typeop("castable", path(&["r", "empty"]), "decimal", None),
                typeop("castable", path(&["r", "missing"]), "decimal", None),
            ]),
        ),
        (
            "cast-string-v-number",
            seq(vec![
                call("boolean", vec![s("false")]),
                cast("boolean", s("false")),
                call("number", vec![s("1e2")]),
                typeop("castable", s("1e2"), "decimal", None),
            ]),
        ),
        ("sum-untyped", call("sum", vec![path(&["r", "d"])])),
        (
            "sum-typed",
            call(
                "sum",
                vec![bind(
                    "for",
                    vec![("x", path(&["r", "d"]))],
                    cast("decimal", var("x")),
                )],
            ),
        ),
        ("sum-bad", call("sum", vec![path(&["r", "bad"])])),
        ("ebv-empty", call("boolean", vec![seq(vec![])])),
        (
            "ebv-node-first",
            call("boolean", vec![seq(vec![path(&["r"]), n("1"), n("2")])]),
        ),
        (
            "ebv-atomic-first",
            call("boolean", vec![seq(vec![n("1"), path(&["r"])])]),
        ),
        (
            "ebv-two-string",
            call("boolean", vec![seq(vec![s(""), s("x")])]),
        ),
        (
            "function-string-cardinality",
            call("string", vec![path(&["r", "a"])]),
        ),
        ("if-dead-error", cond(call("true", vec![]), n("7"), bad())),
        (
            "and-dead-error",
            bin("and", call("false", vec![]), bin("=", bad(), n("1"))),
        ),
        (
            "or-dead-error",
            bin("or", call("true", vec![]), bin("=", bad(), n("1"))),
        ),
        (
            "exists-tail-error",
            call("exists", vec![seq(vec![n("1"), bad()])]),
        ),
        (
            "count-tail-error",
            call("count", vec![seq(vec![n("1"), bad()])]),
        ),
        (
            "filter-head-error",
            filter(seq(vec![n("1"), bad()]), n("1")),
        ),
        (
            "some-tail-error",
            bind(
                "some",
                vec![("x", seq(vec![n("1"), n("2")]))],
                bin("or", bin("=", var("x"), n("1")), bin("=", bad(), n("2"))),
            ),
        ),
        (
            "every-tail-error",
            bind(
                "every",
                vec![("x", seq(vec![n("1"), n("2")]))],
                bin("and", bin("=", var("x"), n("2")), bin("=", bad(), n("2"))),
            ),
        ),
        (
            "some-empty",
            bind("some", vec![("x", seq(vec![]))], bin("=", bad(), n("1"))),
        ),
        (
            "every-empty",
            bind("every", vec![("x", seq(vec![]))], bin("=", bad(), n("1"))),
        ),
        (
            "castable-source-error",
            typeop("castable", bad(), "string", None),
        ),
        (
            "preceding-group",
            call("string", vec![filter(group(preceding()), n("1"))]),
        ),
        (
            "chained-axis",
            call(
                "string",
                vec![bin(
                    "/",
                    bin("/", a1(), step("child", "c")),
                    filter(
                        filter(
                            step("preceding-sibling", "*"),
                            bin("<=", call("position", vec![]), n("2")),
                        ),
                        n("1"),
                    ),
                )],
            ),
        ),
        (
            "chained-group",
            call(
                "string",
                vec![filter(
                    group(bin(
                        "/",
                        bin("/", a1(), step("child", "c")),
                        filter(
                            step("preceding-sibling", "*"),
                            bin("<=", call("position", vec![]), n("2")),
                        ),
                    )),
                    n("1"),
                )],
            ),
        ),
        (
            "path-positions",
            bin(
                "/",
                bin(
                    "/",
                    path(&["r", "a"]),
                    filter(step("child", "b"), call("last", vec![])),
                ),
                call("string", vec![]),
            ),
        ),
        (
            "global-filter",
            bin(
                "/",
                filter(group(path(&["r", "a", "b"])), call("last", vec![])),
                call("string", vec![]),
            ),
        ),
        (
            "mixed-path",
            bin(
                "/",
                path(&["r", "a"]),
                group(cond(
                    bin("=", step("attribute", "id"), s("one")),
                    Expr::Context,
                    s("atom"),
                )),
            ),
        ),
        (
            "dedup-path",
            call(
                "count",
                vec![bin("/", group(seq(vec![a1(), a1()])), step("child", "b"))],
            ),
        ),
        (
            "for-dependent",
            bind(
                "for",
                vec![
                    ("x", seq(vec![n("1"), n("2")])),
                    ("y", group(bin("to", n("1"), var("x")))),
                ],
                bin("+", bin("*", var("x"), n("10")), var("y")),
            ),
        ),
        (
            "numeric-predicate",
            filter(group(seq(vec![n("10"), n("20"), n("30")])), n("2")),
        ),
        (
            "decimal-predicate",
            filter(
                group(seq(vec![n("10"), n("20"), n("30")])),
                cast("decimal", s("2.0")),
            ),
        ),
        (
            "nan-predicate",
            filter(
                group(seq(vec![n("10"), n("20"), n("30")])),
                cast("double", s("NaN")),
            ),
        ),
        (
            "true-predicate",
            filter(
                group(seq(vec![n("10"), n("20"), n("30")])),
                call("true", vec![]),
            ),
        ),
        (
            "machine-ancestor-axis",
            call(
                "string",
                vec![bin(
                    "/",
                    bin("/", bin("/", a1(), step("child", "c")), step("child", "z")),
                    filter(step("ancestor", "*"), n("1")),
                )],
            ),
        ),
        (
            "machine-ancestor-group",
            call("string", vec![filter(group(ancestor()), n("1"))]),
        ),
        (
            "machine-text-values",
            seq(vec![
                call("data", vec![path(&["r", "split"])]),
                call(
                    "data",
                    vec![bin("/", path(&["r", "split"]), step("child", "text()"))],
                ),
            ]),
        ),
        (
            "machine-atomic-path",
            bin("/", path(&["r", "a", "b"]), call("position", vec![])),
        ),
        (
            "machine-for-focus",
            bind(
                "for",
                vec![("x", path(&["r", "a"]))],
                seq(vec![
                    call("position", vec![]),
                    call("last", vec![]),
                    call("string", vec![bin("/", var("x"), step("attribute", "id"))]),
                ]),
            ),
        ),
        (
            "machine-for-shadow",
            bind(
                "for",
                vec![("x", n("7"))],
                bind(
                    "for",
                    vec![("inner", bin("+", var("x"), n("1")))],
                    var("inner"),
                ),
            ),
        ),
        (
            "machine-union",
            call(
                "count",
                vec![bin("|", path(&["r", "a"]), path(&["r", "a"]))],
            ),
        ),
        (
            "machine-abs-round",
            seq(vec![
                call("abs", vec![decimal("-1.25")]),
                call("round", vec![decimal("-0.5")]),
            ]),
        ),
        (
            "machine-attribute-data",
            call(
                "data",
                vec![bin(
                    "/",
                    bin(
                        "/",
                        path(&["r"]),
                        Expr::Step {
                            axis: "child".into(),
                            test: validation_ir::NodeTest::Name {
                                name: name("urn:p", "e"),
                            },
                        },
                    ),
                    Expr::Step {
                        axis: "attribute".into(),
                        test: validation_ir::NodeTest::Name {
                            name: name("urn:p", "x"),
                        },
                    },
                )],
            ),
        ),
        (
            "machine-namespace-data",
            call("data", vec![bin("/", path(&["r"]), step("namespace", "p"))]),
        ),
        (
            "machine-namespace-identity",
            bin(
                "is",
                bin("/", path(&["r"]), step("namespace", "p")),
                bin("/", path(&["r"]), step("namespace", "p")),
            ),
        ),
        (
            "machine-attribute-identity",
            bin(
                "is",
                bin("/", path(&["r", "a"]), step("attribute", "id")),
                path(&["r"]),
            ),
        ),
        (
            "machine-atomic-path-prefix",
            call(
                "exists",
                vec![bin(
                    "/",
                    path(&["r", "a"]),
                    group(cond(
                        bin("=", step("attribute", "id"), s("one")),
                        n("1"),
                        bad(),
                    )),
                )],
            ),
        ),
        (
            "machine-empty-general",
            bin("=", seq(vec![n("1"), bad()]), seq(vec![])),
        ),
        (
            "machine-filter-zero",
            filter(group(seq(vec![bad(), n("1")])), n("0")),
        ),
        ("machine-root-atomic", filter(group(n("1")), Expr::Root)),
    ]
}
#[test]
fn committed_exact_ast_cases_match_fresh_pinned_observations() {
    let fixture = include_str!("evaluator-fixtures/observations.tsv");
    let queries = include_str!("evaluator-fixtures/queries.tsv");
    let doc = SourceDocument::parse(include_bytes!("evaluator-fixtures/document.xml")).unwrap();
    let cases = cases();
    assert_eq!(queries.lines().count(), cases.len());
    for (id, tree) in cases {
        let mut e = expression(tree);
        let query = queries
            .lines()
            .find(|l| l.split('\t').next() == Some(id))
            .unwrap_or_else(|| panic!("missing {id}"));
        e.text = decode(query.split('\t').nth(1).unwrap());
        let p = Prepared::new(&e, &[], machine_limits()).unwrap_or_else(|e| panic!("{id}: {e}"));
        let mut r = Run::new(
            &p,
            &doc,
            EntryFocus::Document,
            atomic_limits(),
            machine_limits(),
            1_000_000,
            1_000_000,
        )
        .unwrap();
        let c = r.cursor().unwrap();
        let mut actual = vec![];
        loop {
            match r.next(c) {
                Ok(Some(i)) => {
                    let kind = match r.item_ref(i).unwrap() {
                        ItemRef::Atomic(a) => format!("{{{XS}}}{}", a.atomic_type().local_name()),
                        ItemRef::Node(_) => panic!("unprojected source probe {id}"),
                    };
                    let value = r.render(i).unwrap().as_str().to_owned();
                    actual.push(format!("ITEM\t{}\t{}", hex(&kind), hex(&value)));
                }
                Ok(None) => {
                    if actual.is_empty() {
                        actual.push("EMPTY".into());
                    }
                    break;
                }
                Err(Error::XPath(f)) => {
                    actual.clear();
                    actual.push(format!("ERROR\t{}", f.local_name()));
                    break;
                }
                Err(e) => panic!("{id}: {e}"),
            }
        }
        let expected: Vec<_> = fixture
            .lines()
            .filter_map(|l| l.strip_prefix(&format!("{id}\t")))
            .collect();
        assert_eq!(actual, expected, "probe {id}: {}", e.text);
    }
}
fn hex(s: &str) -> String {
    use std::fmt::Write;
    s.bytes().fold(String::new(), |mut out, b| {
        write!(out, "{b:02x}").unwrap();
        out
    })
}
