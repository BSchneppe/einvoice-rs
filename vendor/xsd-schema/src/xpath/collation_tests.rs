//! Tests for the host collation callback.
//!
//! Three fixtures do most of the work:
//!
//! * [`AsciiCaseless`] — an ASCII case-insensitive collation with a
//!   [`sort_key`](Collation::sort_key) *and* a [`find`](Collation::find), so it
//!   exercises the hash-indexed path and all five substring functions;
//! * [`OrderOnly`] — the same ordering with neither, so it exercises the
//!   pairwise fallback of the indexes and FOCH0004 from the substring functions;
//! * [`Recording`] — a resolver that counts what it is asked for, so that "the
//!   codepoint collation never reaches the resolver" is asserted and not merely
//!   asserted about.
//!
//! Everything runs through the public API — `XPathExpr::compile` plus
//! `evaluator(&ctx).run()` — so the wiring under test is the one a host gets.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::rc::Rc;

use super::*;
use crate::namespace::table::NameTable;
use crate::xpath::api::XPathExpr;
use crate::xpath::{RoXmlNavigator, XPathValue};

type Nav = RoXmlNavigator<'static>;

const CASELESS: &str = "http://example.com/collation/ascii-caseless";
const ORDER_ONLY: &str = "http://example.com/collation/order-only";
const UNKNOWN: &str = "http://example.com/collation/nobody-knows-this";

// ============================================================================
// Fixtures
// ============================================================================

/// ASCII case-insensitive, with a sort key and collation units.
struct AsciiCaseless;

fn lowered(s: &str) -> Vec<u8> {
    s.bytes().map(|b| b.to_ascii_lowercase()).collect()
}

impl Collation for AsciiCaseless {
    fn compare(&self, a: &str, b: &str) -> Ordering {
        lowered(a).cmp(&lowered(b))
    }

    fn sort_key(&self, s: &str) -> Option<Vec<u8>> {
        Some(lowered(s))
    }

    fn find(&self, haystack: &str, needle: &str) -> Option<Option<(usize, usize)>> {
        // ASCII case folding is length-preserving on the byte level for ASCII
        // letters and leaves every other byte alone, so a byte offset into the
        // folded string is a byte offset into the original.
        let (folded_haystack, folded_needle) = (lowered(haystack), lowered(needle));
        let found = folded_haystack
            .windows(folded_needle.len().max(1))
            .position(|window| window == folded_needle.as_slice())
            .or(if folded_needle.is_empty() {
                Some(0)
            } else {
                None
            });
        Some(found.map(|start| (start, start + folded_needle.len())))
    }
}

/// The same ordering, with no sort key and no collation units: the fallback
/// fixture.
struct OrderOnly;

impl Collation for OrderOnly {
    fn compare(&self, a: &str, b: &str) -> Ordering {
        lowered(a).cmp(&lowered(b))
    }
}

/// A resolver that records every URI it is asked about.
#[derive(Default)]
struct Recording {
    asked: RefCell<Vec<String>>,
}

impl std::fmt::Debug for Recording {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Recording")
    }
}

impl CollationResolver for Recording {
    fn resolve(&self, uri: &str) -> Option<Rc<dyn Collation>> {
        self.asked.borrow_mut().push(uri.to_string());
        match uri {
            CASELESS => Some(Rc::new(AsciiCaseless) as Rc<dyn Collation>),
            ORDER_ONLY => Some(Rc::new(OrderOnly) as Rc<dyn Collation>),
            _ => None,
        }
    }
}

impl Recording {
    fn asked(&self) -> Vec<String> {
        self.asked.borrow().clone()
    }
}

// ============================================================================
// Harness
// ============================================================================

/// Evaluate `src` against a context built by `configure`.
fn eval_with(
    names: &NameTable,
    resolver: Option<&Recording>,
    default_collation: Option<&str>,
    base_uri: Option<&str>,
    src: &str,
) -> Result<XPathValue<Nav>, XPathError> {
    // The `xs` prefix is bound so that constructor functions can be exercised.
    let mut namespaces = crate::namespace::context::NamespaceContextSnapshot::default();
    namespaces
        .bindings
        .push((names.add("xs"), names.add(crate::namespace::XS_NAMESPACE)));
    let mut ctx = XPathContext::new(names).with_namespaces(namespaces);
    if let Some(resolver) = resolver {
        ctx = ctx.with_collation_resolver(resolver);
    }
    if let Some(uri) = default_collation {
        ctx = ctx.with_default_collation(uri);
    }
    if let Some(base) = base_uri {
        ctx = ctx.with_base_uri(base);
    }
    XPathExpr::compile(src, &ctx)?.evaluator(&ctx).run::<Nav>()
}

/// Evaluate `src` with the recording resolver installed and no default
/// collation, i.e. under the codepoint collation.
fn eval(resolver: &Recording, src: &str) -> Result<XPathValue<Nav>, XPathError> {
    let names = NameTable::new();
    eval_with(&names, Some(resolver), None, None, src)
}

/// Evaluate `src` with the recording resolver installed and `CASELESS` as the
/// default collation.
fn eval_caseless(resolver: &Recording, src: &str) -> Result<XPathValue<Nav>, XPathError> {
    let names = NameTable::new();
    eval_with(&names, Some(resolver), Some(CASELESS), None, src)
}

fn code(err: &XPathError) -> Option<&'static str> {
    err.error_code()
}

/// The error of a failed evaluation. `Result::unwrap_err` is unavailable here:
/// `XPathValue<N>` is `Debug` only when `N` is, and no navigator is.
fn expect_err(result: Result<XPathValue<Nav>, XPathError>, what: &str) -> XPathError {
    match result {
        Err(err) => err,
        Ok(_) => panic!("{what} was expected to raise"),
    }
}

/// The integer result of an evaluation, as an `i64`.
fn integer(value: &XPathValue<Nav>) -> Option<i64> {
    use num_bigint::BigInt;
    value
        .as_integer()
        .and_then(|i: BigInt| i64::try_from(i).ok())
}

// ============================================================================
// The API itself
// ============================================================================

#[test]
fn the_default_collation_is_codepoint_and_survives_being_set_explicitly() {
    let names = NameTable::new();
    assert_eq!(
        XPathContext::new(&names).default_collation(),
        CODEPOINT_COLLATION_URI
    );
    assert_eq!(
        XPathContext::new(&names)
            .with_default_collation(CODEPOINT_COLLATION_URI)
            .default_collation(),
        CODEPOINT_COLLATION_URI
    );
    assert_eq!(
        XPathContext::new(&names)
            .with_default_collation(CASELESS)
            .default_collation(),
        CASELESS
    );
}

#[test]
fn fn_default_collation_returns_the_static_context_property() {
    let resolver = Recording::default();
    assert_eq!(
        eval(&resolver, "default-collation()")
            .unwrap()
            .to_owned_string(),
        Some(CODEPOINT_COLLATION_URI.to_string())
    );
    assert_eq!(
        eval_caseless(&resolver, "default-collation()")
            .unwrap()
            .to_owned_string(),
        Some(CASELESS.to_string())
    );
    // Reading the property never asks the resolver for anything.
    assert_eq!(resolver.asked(), Vec::<String>::new());
}

#[test]
fn the_codepoint_collation_never_reaches_the_resolver() {
    let resolver = Recording::default();
    // Every collation-aware function, with the codepoint URI spelled out …
    for src in [
        &format!("compare('a', 'b', '{CODEPOINT_COLLATION_URI}')"),
        &format!("contains('abc', 'b', '{CODEPOINT_COLLATION_URI}')"),
        &format!("starts-with('abc', 'a', '{CODEPOINT_COLLATION_URI}')"),
        &format!("ends-with('abc', 'c', '{CODEPOINT_COLLATION_URI}')"),
        &format!("substring-before('abc', 'b', '{CODEPOINT_COLLATION_URI}')"),
        &format!("substring-after('abc', 'b', '{CODEPOINT_COLLATION_URI}')"),
        &format!("index-of(('a', 'b'), 'b', '{CODEPOINT_COLLATION_URI}')"),
        &format!("distinct-values(('a', 'A'), '{CODEPOINT_COLLATION_URI}')"),
        &format!("deep-equal(('a'), ('a'), '{CODEPOINT_COLLATION_URI}')"),
        &format!("min(('a', 'b'), '{CODEPOINT_COLLATION_URI}')"),
        &format!("max(('a', 'b'), '{CODEPOINT_COLLATION_URI}')"),
        // … and with no argument at all, so the unset default is used.
        &"compare('a', 'b')".to_string(),
        &"('a', 'b') = 'b'".to_string(),
        &"'a' lt 'b'".to_string(),
    ] {
        eval(&resolver, src).unwrap_or_else(|err| panic!("{src}: {err}"));
    }
    assert_eq!(resolver.asked(), Vec::<String>::new());
}

#[test]
fn a_host_collation_is_resolved_once_per_run_and_memoised() {
    let names = NameTable::new();
    let resolver = Recording::default();
    let ctx = XPathContext::new(&names).with_collation_resolver(&resolver);
    // One compiled expression, one run, the same URI used 100 times.
    let expr = XPathExpr::compile(
        &format!("count((1 to 100)[compare('A', 'a', '{CASELESS}') eq 0])"),
        &ctx,
    )
    .unwrap();
    let result = expr.evaluator(&ctx).run::<Nav>().unwrap();
    assert_eq!(integer(&result), Some(100));
    // The predicate ran 100 times; the resolver was asked once.
    assert_eq!(resolver.asked(), vec![CASELESS.to_string()]);
}

// ============================================================================
// FOCH0002 — for every wired function, in both forms
// ============================================================================

/// Every function that takes a `$collation` argument, with a URI nothing
/// supports.
const UNKNOWN_COLLATION_CALLS: &[&str] = &[
    "compare('a', 'b', '{URI}')",
    "contains('abc', 'b', '{URI}')",
    "starts-with('abc', 'a', '{URI}')",
    "ends-with('abc', 'c', '{URI}')",
    "substring-before('abc', 'b', '{URI}')",
    "substring-after('abc', 'b', '{URI}')",
    "index-of(('a', 'b'), 'b', '{URI}')",
    "distinct-values(('a', 'A'), '{URI}')",
    "deep-equal(('a'), ('a'), '{URI}')",
    // min/max only need a collation when they compare strings — these do.
    "min(('a', 'b'), '{URI}')",
    "max(('a', 'b'), '{URI}')",
];

#[test]
fn an_unknown_collation_uri_is_foch0002_in_every_function() {
    let resolver = Recording::default();
    for template in UNKNOWN_COLLATION_CALLS {
        let src = template.replace("{URI}", UNKNOWN);
        let err = expect_err(eval(&resolver, &src), &src);
        assert_eq!(code(&err), Some("FOCH0002"), "{src} gave {err}");
    }
    // The resolver was asked about it every time — it is the only authority.
    assert!(
        resolver.asked().iter().all(|uri| uri == UNKNOWN),
        "{:?}",
        resolver.asked()
    );
}

#[test]
fn an_unknown_collation_uri_is_foch0002_with_no_resolver_at_all() {
    let names = NameTable::new();
    for template in UNKNOWN_COLLATION_CALLS {
        let src = template.replace("{URI}", UNKNOWN);
        let err = expect_err(eval_with(&names, None, None, None, &src), &src);
        assert_eq!(code(&err), Some("FOCH0002"), "{src} gave {err}");
    }
}

#[test]
fn an_unsupported_default_collation_is_foch0002_where_strings_are_compared() {
    let names = NameTable::new();
    let unsupported = |src: &str| eval_with(&names, None, Some(UNKNOWN), None, src);

    // Functions called with no `$collation` argument fall back to the default.
    for src in [
        "compare('a', 'b')",
        "contains('abc', 'b')",
        "starts-with('abc', 'a')",
        "ends-with('abc', 'c')",
        "substring-before('abc', 'b')",
        "substring-after('abc', 'b')",
        "index-of(('a', 'b'), 'b')",
        "distinct-values(('a', 'A'))",
        // `fn:deep-equal` refuses a collation it cannot use even when no
        // string turns up, because its comparison engine answers `bool` and
        // could not report the error later.
        "deep-equal(('a'), ('a'))",
        "deep-equal((1), (1))",
        // The comparison operators, value and general.
        "'a' eq 'b'",
        "'a' lt 'b'",
        "('a', 'b') = 'b'",
        "('a', 'b') != 'b'",
        "('a', 'b') < 'b'",
    ] {
        let err = expect_err(unsupported(src), src);
        assert_eq!(code(&err), Some("FOCH0002"), "{src} gave {err}");
    }

    // … but an expression that compares no strings is undisturbed: the error is
    // raised where the collation is *needed* (F&O §7.3.1), not where it is set.
    assert_eq!(unsupported("1 eq 1").unwrap().as_bool(), Some(true));
    assert_eq!(unsupported("(1, 2) = 2").unwrap().as_bool(), Some(true));
    assert_eq!(unsupported("(1, 2) < 2").unwrap().as_bool(), Some(true));
    assert_eq!(integer(&unsupported("min((3, 1, 2))").unwrap()), Some(1));
    assert_eq!(integer(&unsupported("max((3, 1, 2))").unwrap()), Some(3));
    assert_eq!(unsupported("distinct-values((1, 1, 2))").unwrap().len(), 2);
}

#[test]
fn min_and_max_ignore_the_collation_for_numbers_but_not_for_strings() {
    let names = NameTable::new();
    let numeric = |src: &str| eval_with(&names, None, None, None, src);
    let call = |src: &str| eval_with(&names, None, None, None, src);

    // An unsupported URI with a numeric sequence: the collation is never used,
    // so there is nothing to fail (F&O §15.4.3 — it applies to xs:string only).
    assert_eq!(
        integer(&numeric(&format!("min((3, 1, 2), '{UNKNOWN}')")).unwrap()),
        Some(1)
    );
    assert_eq!(
        integer(&numeric(&format!("max((3, 1, 2), '{UNKNOWN}')")).unwrap()),
        Some(3)
    );
    // The same URI with strings does raise.
    assert_eq!(
        code(&expect_err(
            call(&format!("min(('a', 'b'), '{UNKNOWN}')")),
            "min over strings with an unsupported collation"
        )),
        Some("FOCH0002")
    );
}

// ============================================================================
// FOCH0004 — a collation with no collation units
// ============================================================================

#[test]
fn a_collation_without_collation_units_is_foch0004_in_the_five_substring_functions() {
    let resolver = Recording::default();
    for template in [
        "contains('abc', 'b', '{URI}')",
        "starts-with('abc', 'a', '{URI}')",
        "ends-with('abc', 'c', '{URI}')",
        "substring-before('abc', 'b', '{URI}')",
        "substring-after('abc', 'b', '{URI}')",
        // A zero-length *first* argument still needs the collation units of
        // the second: under a collation with ignorable units it may be
        // equivalent to the zero-length string (F&O §7.5.1).
        "contains('', 'b', '{URI}')",
        "starts-with((), 'b', '{URI}')",
    ] {
        let src = template.replace("{URI}", ORDER_ONLY);
        let err = expect_err(eval(&resolver, &src), &src);
        assert_eq!(code(&err), Some("FOCH0004"), "{src} gave {err}");
        // The code travels as an error QName, and the message names the URI.
        let raised = err.raised_error().expect("an error QName");
        assert_eq!(raised.local_name, "FOCH0004");
        assert_eq!(
            raised.namespace_uri,
            crate::xpath::error::XQT_ERRORS_NAMESPACE
        );
        assert!(raised.description.unwrap().contains(ORDER_ONLY));
    }
}

/// F&O 1.0 §7.5.1 – §7.5.5 decide a zero-length second argument whatever the
/// collation: "If the value of $arg2 is the zero-length string, then the
/// function returns true" (`contains`, `starts-with`, `ends-with`), "returns the
/// zero-length string" (`substring-before`) and "returns the value of $arg1"
/// (`substring-after`). Only then: "If the specified collation does not support
/// collation units an error ·may· be raised \[err:FOCH0004\]" — so a collation
/// without collation units answers these, and an unsupported one is still
/// FOCH0002 first (F&O §7.3.1).
#[test]
fn the_zero_length_cases_come_before_foch0004() {
    let resolver = Recording::default();
    for (template, expected) in [
        ("string(contains('abc', '', '{URI}'))", "true"),
        ("string(contains('abc', (), '{URI}'))", "true"),
        ("string(contains((), '', '{URI}'))", "true"),
        ("string(starts-with('abc', '', '{URI}'))", "true"),
        ("string(ends-with('abc', (), '{URI}'))", "true"),
        ("substring-before('abc', '', '{URI}')", ""),
        ("substring-after('abc', '', '{URI}')", "abc"),
        ("substring-after((), (), '{URI}')", ""),
    ] {
        let src = template.replace("{URI}", ORDER_ONLY);
        let value = eval(&resolver, &src).unwrap_or_else(|e| panic!("{src} raised {e}"));
        let text = value
            .into_vec()
            .into_iter()
            .map(|item| match item {
                crate::xpath::iterator::XmlItem::Atomic(a) => a.to_string_value(),
                crate::xpath::iterator::XmlItem::Node(_) => "<node>".to_string(),
            })
            .collect::<String>();
        assert_eq!(text, expected, "{src}");
    }
    for template in [
        "contains('abc', '', '{URI}')",
        "substring-after('abc', '', '{URI}')",
    ] {
        let src = template.replace("{URI}", UNKNOWN);
        let err = expect_err(eval(&resolver, &src), &src);
        assert_eq!(code(&err), Some("FOCH0002"), "{src} gave {err}");
    }
}

#[test]
fn a_collation_without_collation_units_still_serves_the_ordering_functions() {
    let resolver = Recording::default();
    let order_only = |src: &str| {
        let names = NameTable::new();
        eval_with(&names, Some(&resolver), Some(ORDER_ONLY), None, src)
    };
    assert_eq!(
        integer(&order_only("compare('ABC', 'abc')").unwrap()),
        Some(0)
    );
    assert_eq!(order_only("'ABC' eq 'abc'").unwrap().as_bool(), Some(true));
    assert_eq!(
        order_only("('a', 'q') = 'Q'").unwrap().as_bool(),
        Some(true)
    );
    assert_eq!(
        order_only("min(('B', 'a'))").unwrap().to_owned_string(),
        Some("a".into())
    );
    assert_eq!(
        order_only("distinct-values(('a', 'A', 'b'))")
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        integer(&order_only("index-of(('a', 'B'), 'b')").unwrap()),
        Some(2)
    );
    assert_eq!(
        order_only("deep-equal(('A'), ('a'))").unwrap().as_bool(),
        Some(true)
    );
}

// ============================================================================
// Every wired function under a real host collation
// ============================================================================

/// `(expression, answer as a string)` under the ASCII case-insensitive
/// collation. Each is run twice: with the URI as an explicit argument and with
/// the same collation installed as the default and no argument at all.
fn caseless_cases() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        // (call with a {URI} placeholder, the same call without it, expected)
        ("compare('ABC', 'abc'{, URI})", "", "0"),
        ("compare('ABC', 'abd'{, URI})", "", "-1"),
        ("contains('Hello World', 'LO W'{, URI})", "", "true"),
        ("contains('Hello', 'z'{, URI})", "", "false"),
        ("starts-with('Hello', 'HE'{, URI})", "", "true"),
        ("starts-with('Hello', 'el'{, URI})", "", "false"),
        ("ends-with('Hello', 'LO'{, URI})", "", "true"),
        ("ends-with('Hello', 'll'{, URI})", "", "false"),
        ("substring-before('Hello', 'LL'{, URI})", "", "He"),
        ("substring-after('Hello', 'LL'{, URI})", "", "o"),
        ("substring-before('Hello', 'z'{, URI})", "", ""),
        ("substring-after('Hello', 'z'{, URI})", "", ""),
        ("index-of(('a', 'B', 'c'), 'b'{, URI})", "", "2"),
        ("count(distinct-values(('a', 'A', 'b'){, URI}))", "", "2"),
        ("deep-equal(('A', 'b'), ('a', 'B'){, URI})", "", "true"),
        ("min(('B', 'a'){, URI})", "", "a"),
        ("max(('B', 'a'){, URI})", "", "B"),
    ]
}

#[test]
fn every_wired_function_uses_an_explicit_collation_argument() {
    let resolver = Recording::default();
    for (template, _, expected) in caseless_cases() {
        let src = template.replace("{, URI}", &format!(", '{CASELESS}'"));
        let value = eval(&resolver, &src).unwrap_or_else(|err| panic!("{src}: {err}"));
        assert_eq!(render(&value), expected, "{src}");
    }
}

#[test]
fn every_wired_function_uses_the_default_collation_when_given_no_argument() {
    let resolver = Recording::default();
    for (template, _, expected) in caseless_cases() {
        let src = template.replace("{, URI}", "");
        let value = eval_caseless(&resolver, &src).unwrap_or_else(|err| panic!("{src}: {err}"));
        assert_eq!(render(&value), expected, "{src}");
    }
}

/// The string form of a single-item result, for the table above.
fn render(value: &XPathValue<Nav>) -> String {
    use crate::xpath::XmlItem;
    match value {
        XPathValue::Empty => String::new(),
        XPathValue::Item(XmlItem::Atomic(atom)) => atom.to_string_value(),
        other => other.to_owned_string().unwrap_or_default(),
    }
}

// ============================================================================
// Value and general comparisons under a default collation
// ============================================================================

#[test]
fn value_and_general_comparisons_use_the_default_collation() {
    let resolver = Recording::default();
    let yes = |src: &str| {
        assert_eq!(
            eval_caseless(&resolver, src).unwrap().as_bool(),
            Some(true),
            "{src}"
        )
    };
    let no = |src: &str| {
        assert_eq!(
            eval_caseless(&resolver, src).unwrap().as_bool(),
            Some(false),
            "{src}"
        )
    };

    // Value comparisons (XPath 2.0 §3.5.1 / §B.2: `fn:compare` under the
    // default collation).
    yes("'A' eq 'a'");
    no("'A' ne 'a'");
    yes("'a' lt 'B'"); // codepoint would say 'B' (0x42) < 'a' (0x61)
    yes("'B' gt 'a'");
    yes("'A' le 'a'");
    yes("'A' ge 'a'");

    // General comparisons inherit it (§3.5.2 defers to `eq`).
    yes("('a', 'b') = 'B'");
    yes("('a', 'b') != 'Z'");
    no("('a', 'b') = 'Z'");
    yes("('a') < ('B')");
    yes("('Z') > ('a')");

    // xs:anyURI is promoted to xs:string, so it is collated too (§B.1).
    yes("xs:anyURI('ABC') eq 'abc'");

    // Untyped values are cast to xs:string by §3.5.2 and then collated.
    let doc = roxmltree::Document::parse("<r a='ABC'/>").unwrap();
    let names = NameTable::new();
    let ctx = XPathContext::new(&names)
        .with_collation_resolver(&resolver)
        .with_default_collation(CASELESS);
    let expr = XPathExpr::compile("/r/@a = 'abc'", &ctx).unwrap();
    let nav = RoXmlNavigator::new(&doc);
    let result = expr.evaluator(&ctx).run_with_node(nav).unwrap();
    assert_eq!(result.as_bool(), Some(true));
}

#[test]
fn codepoint_equal_ignores_the_default_collation() {
    let resolver = Recording::default();
    // F&O §7.3.3 pins `fn:codepoint-equal` to the codepoint collation.
    assert_eq!(
        eval_caseless(&resolver, "codepoint-equal('A', 'a')")
            .unwrap()
            .as_bool(),
        Some(false)
    );
    assert_eq!(
        eval_caseless(&resolver, "codepoint-equal('a', 'a')")
            .unwrap()
            .as_bool(),
        Some(true)
    );
}

// ============================================================================
// XPath 1.0 compatibility mode
// ============================================================================

/// Evaluate `src` in XPath 1.0 compatibility mode, over `xml` when one is
/// given, with `default_collation` as the default collation.
fn eval_compat(
    resolver: &Recording,
    default_collation: &str,
    xml: Option<&str>,
    src: &str,
) -> Result<Option<bool>, XPathError> {
    let names = NameTable::new();
    let ctx = XPathContext::new(&names)
        .with_collation_resolver(resolver)
        .with_default_collation(default_collation)
        .with_xpath10_compatibility(true);
    let expr = XPathExpr::compile(src, &ctx)?;
    let result = match xml {
        Some(xml) => {
            let doc = roxmltree::Document::parse(xml).unwrap();
            expr.evaluator(&ctx)
                .run_with_node(RoXmlNavigator::new(&doc))?
                .as_bool()
        }
        None => expr.evaluator(&ctx).run::<Nav>()?.as_bool(),
    };
    Ok(result)
}

/// XPath 2.0 §3.5.2: in XPath 1.0 compatibility mode the operands are
/// converted and then "compared using one of the value comparison operators
/// eq, ne, lt, le, gt, or ge", and a value comparison of two strings uses the
/// default collation (§3.5.1, §B.2). So `=` and `!=` between strings collate,
/// exactly as `eq` and `ne` do.
#[test]
fn compatibility_mode_general_comparisons_use_the_default_collation() {
    let resolver = Recording::default();
    let check = |src: &str, expected: bool| {
        assert_eq!(
            eval_compat(&resolver, CASELESS, None, src).unwrap(),
            Some(expected),
            "{src}"
        );
    };

    // The review's reproduction: `=` must agree with `eq`.
    check("'ABC' eq 'abc'", true);
    check("'ABC' = 'abc'", true);
    check("'ABC' != 'abc'", false);
    check("'ABC' != 'abd'", true);
    check("('x', 'ABC') = 'abc'", true);

    // `<`, `<=`, `>`, `>=` convert every operand with `fn:number` first
    // (§3.5.2 rule 3), so no string is ever compared and the collation plays
    // no part: 'a' and 'B' are both NaN, and '10' / '9' compare as numbers.
    check("'a' < 'B'", false);
    check("'B' > 'a'", false);
    check("'10' < '9'", false);
    check("'9' < '10'", true);
    check("'9' <= '9.0'", true);

    // An untyped attribute value is compared as a string, under the collation.
    assert_eq!(
        eval_compat(&resolver, CASELESS, Some("<r a='ABC'/>"), "/r/@a = 'abc'").unwrap(),
        Some(true)
    );
}

/// A default collation nobody supplies is FOCH0002 wherever compatibility
/// mode compares two strings — it must not turn into `false` — and nowhere
/// else (F&O §7.3.1: the error belongs where the collation is *needed*).
#[test]
fn an_unsupported_default_collation_is_foch0002_in_compatibility_mode() {
    let resolver = Recording::default();
    for src in ["'a' = 'b'", "'a' != 'b'", "('x', 'y') = 'z'"] {
        let err = expect_err(
            eval_compat(&resolver, UNKNOWN, None, src).map(|_| XPathValue::<Nav>::Empty),
            src,
        );
        assert_eq!(code(&err), Some("FOCH0002"), "{src} gave {err}");
    }

    // Numbers, booleans and relational operators compare no strings.
    for (src, expected) in [
        ("1 = 1", true),
        ("1 = '1'", true),
        ("1 != 2", true),
        ("true() = 'x'", true),
        ("'a' < 'b'", false),
        ("'1' < '2'", true),
    ] {
        assert_eq!(
            eval_compat(&resolver, UNKNOWN, None, src).unwrap(),
            Some(expected),
            "{src}"
        );
    }
}

// ============================================================================
// fn:deep-equal — content and attribute values, but not names
// ============================================================================

/// Evaluate `src` over `xml` under the ASCII case-insensitive default
/// collation.
fn deep_equal_over(xml: &str, src: &str) -> bool {
    let doc = roxmltree::Document::parse(xml).unwrap();
    let names = NameTable::new();
    let resolver = Recording::default();
    let ctx = XPathContext::new(&names)
        .with_collation_resolver(&resolver)
        .with_default_collation(CASELESS);
    let expr = XPathExpr::compile(src, &ctx).unwrap();
    let nav = RoXmlNavigator::new(&doc);
    expr.evaluator(&ctx)
        .run_with_node(nav)
        .unwrap()
        .as_bool()
        .unwrap()
}

#[test]
fn deep_equal_collates_element_content_and_attribute_values() {
    // Text content differing only in case (the two elements have the same
    // name, since deep-equal compares names too).
    assert!(deep_equal_over(
        "<r><x><e>Text</e></x><y><e>TEXT</e></y></r>",
        "deep-equal(/r/x/e, /r/y/e)"
    ));
    // An attribute *value* differing only in case.
    assert!(deep_equal_over(
        "<r><x><e a='Val'/></x><y><e a='VAL'/></y></r>",
        "deep-equal(/r/x/e/@a, /r/y/e/@a)"
    ));
    assert!(deep_equal_over(
        "<r><x><e a='Val'/></x><y><e a='VAL'/></y></r>",
        "deep-equal(/r/x/e, /r/y/e)"
    ));
    // Free-standing atomic items.
    assert!(deep_equal_over(
        "<r/>",
        "deep-equal(('A', 'b'), ('a', 'B'))"
    ));
    // A processing-instruction's content, but not its target.
    assert!(deep_equal_over(
        "<r><x><?p Data?></x><y><?p DATA?></y></r>",
        "deep-equal(/r/x/processing-instruction(), /r/y/processing-instruction())"
    ));
}

#[test]
fn deep_equal_does_not_collate_names() {
    // Element names differing only in case are different names.
    assert!(!deep_equal_over(
        "<r><x><e>v</e></x><y><E>v</E></y></r>",
        "deep-equal(/r/x/*, /r/y/*)"
    ));
    // Attribute names likewise.
    assert!(!deep_equal_over(
        "<r><x><e a='v'/></x><y><e A='v'/></y></r>",
        "deep-equal(/r/x/e, /r/y/e)"
    ));
    // And a processing-instruction target.
    assert!(!deep_equal_over(
        "<r><x><?p d?></x><y><?P d?></y></r>",
        "deep-equal(/r/x/processing-instruction(), /r/y/processing-instruction())"
    ));
}

// ============================================================================
// Relative collation URIs
// ============================================================================

#[test]
fn a_relative_collation_uri_is_resolved_against_the_static_base_uri() {
    let names = NameTable::new();
    let resolver = Recording::default();
    // `http://example.com/collation/ascii-caseless` relative to
    // `http://example.com/collation/index.xml` is `ascii-caseless`.
    let value = eval_with(
        &names,
        Some(&resolver),
        None,
        Some("http://example.com/collation/index.xml"),
        "compare('ABC', 'abc', 'ascii-caseless')",
    )
    .unwrap();
    assert_eq!(integer(&value), Some(0));
    // The resolver saw the *absolute* URI, never the relative one.
    assert_eq!(resolver.asked(), vec![CASELESS.to_string()]);
}

#[test]
fn a_relative_collation_uri_with_no_base_uri_is_passed_through() {
    let names = NameTable::new();
    let resolver = Recording::default();
    // No base URI: the argument reaches the resolver as written, which is this
    // crate's documented choice — F&O prescribes no error for it, and a
    // resolver may well recognise a short name.
    let err = expect_err(
        eval_with(
            &names,
            Some(&resolver),
            None,
            None,
            "compare('ABC', 'abc', 'ascii-caseless')",
        ),
        "a relative collation URI with no base URI",
    );
    assert_eq!(code(&err), Some("FOCH0002"));
    assert_eq!(resolver.asked(), vec!["ascii-caseless".to_string()]);
}

#[test]
fn a_resolver_may_recognise_a_relative_uri_verbatim() {
    /// A resolver that knows one short name and nothing else.
    #[derive(Debug)]
    struct ShortName;
    impl CollationResolver for ShortName {
        fn resolve(&self, uri: &str) -> Option<Rc<dyn Collation>> {
            (uri == "caseless").then(|| Rc::new(AsciiCaseless) as Rc<dyn Collation>)
        }
    }

    let names = NameTable::new();
    let resolver = ShortName;
    let ctx = XPathContext::new(&names).with_collation_resolver(&resolver);
    let value = XPathExpr::compile("compare('ABC', 'abc', 'caseless')", &ctx)
        .unwrap()
        .evaluator(&ctx)
        .run::<Nav>()
        .unwrap();
    assert_eq!(integer(&value), Some(0));
}

// ============================================================================
// The hash indexes: index path versus pairwise path
// ============================================================================

/// The general-comparison index only engages past
/// `operators::INDEX_AFTER_PAIRS` (64) pairs, so the differential cases have to
/// be big enough to reach it. Each case is run under three collations, and the
/// answer must be the same whichever path the engine picked.
fn big_string_operands() -> (Vec<String>, Vec<String>) {
    let left: Vec<String> = (0..40).map(|i| format!("Item-{i}")).collect();
    let right: Vec<String> = (20..60).map(|i| format!("item-{i}")).collect();
    (left, right)
}

fn sequence_literal(values: &[String]) -> String {
    let items: Vec<String> = values.iter().map(|v| format!("'{v}'")).collect();
    format!("({})", items.join(", "))
}

#[test]
fn a_large_string_comparison_agrees_under_every_collation() {
    let (left, right) = big_string_operands();
    let (left_lit, right_lit) = (sequence_literal(&left), sequence_literal(&right));
    let resolver = Recording::default();

    for (default_collation, eq_expected, ne_expected) in [
        // Codepoint: `Item-20` and `item-20` differ, so nothing matches.
        (None, false, true),
        // Case-insensitive with a sort key: the index keys by it.
        (Some(CASELESS), true, true),
        // Case-insensitive without one: the index declines and the pairwise
        // loop answers — the same answer.
        (Some(ORDER_ONLY), true, true),
    ] {
        let names = NameTable::new();
        let eq = eval_with(
            &names,
            Some(&resolver),
            default_collation,
            None,
            &format!("{left_lit} = {right_lit}"),
        )
        .unwrap();
        assert_eq!(eq.as_bool(), Some(eq_expected), "{default_collation:?} `=`");

        let ne = eval_with(
            &names,
            Some(&resolver),
            default_collation,
            None,
            &format!("{left_lit} != {right_lit}"),
        )
        .unwrap();
        assert_eq!(
            ne.as_bool(),
            Some(ne_expected),
            "{default_collation:?} `!=`"
        );
    }
}

#[test]
fn the_index_and_the_pairwise_loop_agree_item_by_item() {
    // Drive `try_indexed_eq` / `try_indexed_ne` against the pairwise loop
    // directly, under each of the three collations, so that the index is
    // exercised rather than merely reachable.
    use crate::types::value::XmlValue;
    use crate::xpath::general_compare;
    use crate::xpath::iterator::{BufferedNodeIterator, VecNodeIterator};
    use crate::xpath::operators::{general_eq_iter_pairwise, general_ne_iter_pairwise};
    use crate::xpath::XmlItem;

    fn iter_of(values: &[XmlValue]) -> VecNodeIterator<Nav> {
        VecNodeIterator::new(values.iter().cloned().map(XmlItem::Atomic).collect())
    }

    let corpus: Vec<Vec<XmlValue>> = vec![
        vec![XmlValue::string("A"), XmlValue::string("b")],
        vec![XmlValue::string("a"), XmlValue::string("B")],
        vec![XmlValue::string("z")],
        vec![XmlValue::string("Z"), XmlValue::string("y")],
        vec![XmlValue::untyped("A"), XmlValue::string("q")],
        vec![XmlValue::string("")],
        vec![],
        // A pair the collation cannot touch, to show it is left alone.
        vec![XmlValue::integer(1.into()), XmlValue::string("a")],
    ];

    let resolver = Recording::default();
    let names = NameTable::new();
    for default_collation in [None, Some(CASELESS), Some(ORDER_ONLY)] {
        let mut ctx = XPathContext::new(&names).with_collation_resolver(&resolver);
        if let Some(uri) = default_collation {
            ctx = ctx.with_default_collation(uri);
        }
        let active = super::resolve_default(&ctx);

        let mut decided_eq = 0;
        for left in &corpus {
            for right in &corpus {
                let left_iter = iter_of(left);
                let right_buf = BufferedNodeIterator::preload(iter_of(right)).unwrap();

                let expected_eq = general_eq_iter_pairwise(&ctx, &left_iter, &right_buf);
                if let Some(got) = general_compare::try_indexed_eq(
                    &ctx,
                    &left_iter,
                    &right_buf,
                    active.as_ref(),
                    crate::xpath::operators::static_timezone(&ctx),
                ) {
                    decided_eq += 1;
                    assert_eq!(
                        format!("{:?}", expected_eq.as_ref().map_err(|e| e.to_string())),
                        format!("{:?}", got.as_ref().map_err(|e| e.to_string())),
                        "`=` diverged under {default_collation:?}: {left:?} vs {right:?}"
                    );
                }

                let expected_ne = general_ne_iter_pairwise(&ctx, &left_iter, &right_buf);
                if let Some(got) = general_compare::try_indexed_ne(
                    &left_iter,
                    &right_buf,
                    active.as_ref(),
                    crate::xpath::operators::static_timezone(&ctx),
                ) {
                    assert_eq!(
                        format!("{:?}", expected_ne.as_ref().map_err(|e| e.to_string())),
                        format!("{:?}", got.as_ref().map_err(|e| e.to_string())),
                        "`!=` diverged under {default_collation:?}: {left:?} vs {right:?}"
                    );
                }
            }
        }
        // The index really did answer some of them — except under the
        // sort-key-less collation, where declining *is* the behaviour on trial.
        match default_collation {
            Some(ORDER_ONLY) => {}
            _ => assert!(
                decided_eq > 0,
                "the index never engaged for {default_collation:?}"
            ),
        }
    }
}

#[test]
fn a_collation_without_a_sort_key_sends_the_index_back_to_the_pairwise_loop() {
    use crate::types::value::XmlValue;
    use crate::xpath::general_compare;
    use crate::xpath::iterator::{BufferedNodeIterator, VecNodeIterator};
    use crate::xpath::XmlItem;

    let values: Vec<XmlValue> = (0..8).map(|i| XmlValue::string(format!("s{i}"))).collect();
    let iter = |vs: &[XmlValue]| -> VecNodeIterator<Nav> {
        VecNodeIterator::new(vs.iter().cloned().map(XmlItem::Atomic).collect())
    };

    let names = NameTable::new();
    let resolver = Recording::default();

    // With a sort key the index decides; without one it declines outright.
    for (uri, decides) in [(CASELESS, true), (ORDER_ONLY, false)] {
        let ctx = XPathContext::new(&names)
            .with_collation_resolver(&resolver)
            .with_default_collation(uri);
        let active = super::resolve_default(&ctx);
        let left_iter = iter(&values);
        let right_buf = BufferedNodeIterator::preload(iter(&values)).unwrap();
        let outcome = general_compare::try_indexed_eq(
            &ctx,
            &left_iter,
            &right_buf,
            active.as_ref(),
            crate::xpath::operators::static_timezone(&ctx),
        );
        assert_eq!(outcome.is_some(), decides, "{uri}");
    }
}

#[test]
fn the_cross_evaluation_index_agrees_with_the_pairwise_loop_under_a_collation() {
    // `compare_cache` keeps an index of the invariant operand across
    // evaluations of one comparison node. Under a collation with a sort key it
    // must key by that sort key; without one it must fall back. Either way the
    // filter has to select the same items as the pairwise semantics.
    let names = NameTable::new();
    let resolver = Recording::default();

    // 200 invariant items — past `compare_cache::MIN_INVARIANT_ITEMS` — and a
    // predicate evaluated once per item of a long sequence, so the cached index
    // is built and then probed many times.
    let invariant: Vec<String> = (0..200).map(|i| format!("K{i}")).collect();
    let probe: Vec<String> = (150..400).map(|i| format!("k{i}")).collect();
    let src = format!(
        "count(for $x in {} return (if ($x = {}) then 1 else ()))",
        sequence_literal(&probe),
        sequence_literal(&invariant)
    );

    // Codepoint: `K150` never equals `k150`, so nothing matches.
    assert_eq!(
        integer(&eval_with(&names, Some(&resolver), None, None, &src).unwrap()),
        Some(0)
    );
    // Case-insensitive with a sort key: 150..199 match.
    assert_eq!(
        integer(&eval_with(&names, Some(&resolver), Some(CASELESS), None, &src).unwrap()),
        Some(50)
    );
    // Case-insensitive without one: the same answer, via the pairwise loop.
    assert_eq!(
        integer(&eval_with(&names, Some(&resolver), Some(ORDER_ONLY), None, &src).unwrap()),
        Some(50)
    );
}

// ============================================================================
// The trait's provided methods
// ============================================================================

/// A collation with an **ignorable** collation unit (`-`), which is the case
/// the default `starts_with` / `ends_with` are written for: the leftmost
/// minimal match cannot answer them.
struct IgnoresHyphens;

fn without_hyphens(s: &str) -> String {
    s.chars().filter(|c| *c != '-').collect()
}

impl Collation for IgnoresHyphens {
    fn compare(&self, a: &str, b: &str) -> Ordering {
        without_hyphens(a).cmp(&without_hyphens(b))
    }

    fn find(&self, haystack: &str, needle: &str) -> Option<Option<(usize, usize)>> {
        // The *minimal* match: the shortest matching substring, and among
        // equally short ones the leftmost. A longer substring that merely
        // contains a match is not minimal, which is exactly why the leftmost
        // minimal match of `a` in `-ab` is `1..2` and not `0..2`.
        let stripped = without_hyphens(needle);
        if stripped.is_empty() {
            return Some(Some((0, 0)));
        }
        let chars: Vec<(usize, char)> = haystack.char_indices().collect();
        let mut best: Option<(usize, usize)> = None;
        for start in 0..chars.len() {
            for end in start..chars.len() {
                let end_byte = chars[end].0 + chars[end].1.len_utf8();
                if without_hyphens(&haystack[chars[start].0..end_byte]) != stripped {
                    continue;
                }
                let candidate = (chars[start].0, end_byte);
                let shorter = |(s, e): (usize, usize)| e - s;
                if best.is_none_or(|current| shorter(candidate) < shorter(current)) {
                    best = Some(candidate);
                }
            }
        }
        Some(best)
    }
}

#[test]
fn the_default_starts_with_and_ends_with_survive_ignorable_collation_units() {
    let collation = IgnoresHyphens;

    // The leftmost minimal match of "a" in "-ab" is 1..2, so a `find`-based
    // "does it start at 0" test would say false — but the prefix `-a` has the
    // collation units of `a`, so `starts-with` is true.
    assert_eq!(collation.find("-ab", "a"), Some(Some((1, 2))));
    assert_eq!(collation.starts_with("-ab", "a"), Some(true));

    // The mirror case: the minimal match of "b" in "ab-" ends at 2, not 3, yet
    // the suffix `b-` has the collation units of `b`.
    assert_eq!(collation.find("ab-", "b"), Some(Some((1, 2))));
    assert_eq!(collation.ends_with("ab-", "b"), Some(true));

    // And the negative cases still say no.
    assert_eq!(collation.starts_with("-ab", "b"), Some(false));
    assert_eq!(collation.ends_with("ab-", "a"), Some(false));
}

#[test]
fn a_collation_with_no_find_answers_none_from_the_provided_methods() {
    let collation = OrderOnly;
    assert_eq!(collation.find("abc", "b"), None);
    assert_eq!(collation.starts_with("abc", "a"), None);
    assert_eq!(collation.ends_with("abc", "c"), None);
    assert_eq!(collation.sort_key("abc"), None);
    // `equals` still works — it comes from `compare`.
    assert!(collation.equals("ABC", "abc"));
}

#[test]
fn a_collation_reporting_a_bad_byte_range_is_an_error_not_a_panic() {
    /// Reports a range that is not a character boundary of the haystack.
    #[derive(Debug)]
    struct Liar;
    impl Collation for Liar {
        fn compare(&self, a: &str, b: &str) -> Ordering {
            a.cmp(b)
        }
        fn find(&self, _haystack: &str, _needle: &str) -> Option<Option<(usize, usize)>> {
            // A range that splits the two bytes of "ä".
            Some(Some((1, 2)))
        }
    }
    impl CollationResolver for Liar {
        fn resolve(&self, _uri: &str) -> Option<Rc<dyn Collation>> {
            Some(Rc::new(Liar) as Rc<dyn Collation>)
        }
    }

    let names = NameTable::new();
    let resolver = Liar;
    let ctx = XPathContext::new(&names).with_collation_resolver(&resolver);
    // "ä" is two bytes, so 1..2 splits it.
    let err = expect_err(
        XPathExpr::compile(&format!("substring-before('ä', 'x', '{UNKNOWN}')"), &ctx)
            .unwrap()
            .evaluator(&ctx)
            .run::<Nav>(),
        "a collation reporting a byte range that splits a character",
    );
    assert!(err.to_string().contains("character range"), "{err}");
}

// ============================================================================
// The per-run memo
// ============================================================================

#[test]
fn the_memo_answers_repeated_lookups_of_the_same_uri() {
    use crate::xpath::context::DynamicContext;

    let names = NameTable::new();
    let resolver = Recording::default();
    let ctx = XPathContext::new(&names).with_collation_resolver(&resolver);
    let mut dyn_ctx: DynamicContext<'_, Nav> = DynamicContext::new(&ctx, 0);

    for _ in 0..10 {
        let active = resolve_collation_cached(&mut dyn_ctx, Some(CASELESS));
        assert!(matches!(active, ActiveCollation::Custom(..)));
    }
    assert_eq!(dyn_ctx.collation_cache().hits(), 9);
    assert_eq!(dyn_ctx.collation_cache().misses(), 1);
    assert_eq!(resolver.asked(), vec![CASELESS.to_string()]);

    // A failure is memoised too — it is as much a function of the URI.
    for _ in 0..5 {
        let active = resolve_collation_cached(&mut dyn_ctx, Some(UNKNOWN));
        assert!(matches!(active, ActiveCollation::Unsupported(_)));
    }
    assert_eq!(dyn_ctx.collation_cache().misses(), 2);
    assert_eq!(resolver.asked().len(), 2);
}

#[test]
fn the_codepoint_collation_never_touches_the_memo() {
    use crate::xpath::context::DynamicContext;

    let names = NameTable::new();
    let resolver = Recording::default();
    let ctx = XPathContext::new(&names).with_collation_resolver(&resolver);
    let mut dyn_ctx: DynamicContext<'_, Nav> = DynamicContext::new(&ctx, 0);

    for _ in 0..10 {
        // Both forms: the explicit codepoint URI and the unset default.
        assert!(matches!(
            resolve_collation_cached(&mut dyn_ctx, Some(CODEPOINT_COLLATION_URI)),
            ActiveCollation::Codepoint
        ));
        assert!(matches!(
            resolve_collation_cached(&mut dyn_ctx, None),
            ActiveCollation::Codepoint
        ));
    }
    assert_eq!(dyn_ctx.collation_cache().hits(), 0);
    assert_eq!(dyn_ctx.collation_cache().misses(), 0);
    assert_eq!(resolver.asked(), Vec::<String>::new());
}

// ============================================================================
// Unit tests for the module's own helpers
// ============================================================================

#[test]
fn prefixes_are_every_prefix_shortest_first() {
    assert_eq!(prefixes("abc").collect::<Vec<_>>(), ["", "a", "ab", "abc"]);
    assert_eq!(prefixes("").collect::<Vec<_>>(), [""]);
    // Multi-byte characters are never split.
    assert_eq!(prefixes("äb").collect::<Vec<_>>(), ["", "ä", "äb"]);
}

#[test]
fn suffixes_are_every_suffix_longest_first() {
    assert_eq!(suffixes("abc").collect::<Vec<_>>(), ["abc", "bc", "c", ""]);
    assert_eq!(suffixes("").collect::<Vec<_>>(), [""]);
    assert_eq!(suffixes("äb").collect::<Vec<_>>(), ["äb", "b", ""]);
}

#[test]
fn slice_at_reports_a_bad_range_instead_of_panicking() {
    assert_eq!(slice_at("abc", 1..2).unwrap(), "b");
    // Past the end.
    assert!(slice_at("abc", 1..9).is_err());
    // Not a character boundary.
    assert!(slice_at("ä", 1..2).is_err());
}

#[test]
fn foch0004_reports_its_code_through_the_error_qname_machinery() {
    let err = XPathError::collation_no_units(ORDER_ONLY);
    assert_eq!(err.error_code(), Some("FOCH0004"));
    let raised = err.raised_error().unwrap();
    assert_eq!(raised.local_name, "FOCH0004");
    assert!(crate::xpath::error::QNAMED_ERROR_CODES.contains(&"FOCH0004"));
}

// ============================================================================
// When the comparison operators ask the resolver
// ============================================================================

/// Review finding X-11. The `CollationResolver` contract says the resolver "is
/// asked only when a collation is actually needed to compare strings". Under a
/// host default collation the comparison operators used to resolve it on every
/// evaluation, numbers included: 1,000 calls for a 1,000-item integer filter.
/// Now an operator that compares no string never asks, and one that does asks
/// once per run (the per-run memo).
#[test]
fn the_comparison_operators_ask_the_resolver_only_for_strings_and_once_per_run() {
    // A literal of 80 strings: large enough for the cross-evaluation index
    // (it is run invariant) and for the bounded pairwise prefix to run out.
    let strings: Vec<String> = (1..=80).map(|i| format!("'{i}'")).collect();
    let indexed_strings = format!("count((1 to 300)[string(.) = ({})])", strings.join(", "));
    let cases: Vec<(String, i64, usize)> = vec![
        // Numbers only: the resolver is never asked.
        ("count((1 to 1000)[. = 5])".into(), 1, 0),
        ("count((1 to 1000)[. != 5])".into(), 999, 0),
        ("count((1 to 1000)[. < 5])".into(), 4, 0),
        ("count((1 to 1000)[. <= 5])".into(), 5, 0),
        ("count((1 to 1000)[. > 995])".into(), 5, 0),
        ("count((1 to 1000)[. >= 995])".into(), 6, 0),
        ("count((1 to 1000)[. eq 5])".into(), 1, 0),
        ("count((1 to 1000)[. lt 5])".into(), 4, 0),
        ("count((1 to 300)[. = (1 to 80)])".into(), 80, 0),
        (
            "count((1 to 1000)[compare(string(.), 'x') = 0])".into(),
            0,
            1,
        ),
        // Strings: asked once for the whole run, however many evaluations.
        ("count((1 to 1000)[string(.) = 'x'])".into(), 0, 1),
        ("count((1 to 1000)[string(.) != 'x'])".into(), 1000, 1),
        ("count((1 to 1000)[string(.) eq 'x'])".into(), 0, 1),
        ("count((1 to 1000)[string(.) < 'x'])".into(), 1000, 1),
        ("count((1 to 1000)[string(.) ge 'x'])".into(), 0, 1),
        (indexed_strings, 80, 1),
        // And the answers are still collated.
        ("count((1 to 10)[('X', 'y') = 'x'])".into(), 10, 1),
        ("count((1 to 10)['X' eq 'x'])".into(), 10, 1),
    ];
    for (src, expected, calls) in cases {
        let resolver = Recording::default();
        let result = eval_caseless(&resolver, &src).unwrap_or_else(|err| panic!("{src}: {err}"));
        assert_eq!(integer(&result), Some(expected), "{src}");
        assert_eq!(
            resolver.asked().len(),
            calls,
            "{src}: {:?}",
            resolver.asked()
        );
    }
}

/// Compatibility mode resolves the default collation the same way.
#[test]
fn compatibility_mode_comparisons_ask_the_resolver_only_for_strings() {
    for (src, expected, calls) in [
        ("count((1 to 1000)[. = 5]) = 1", true, 0),
        ("count((1 to 1000)[. < 5]) = 4", true, 0),
        ("count((1 to 1000)[string(.) = 'x']) = 0", true, 1),
        ("count((1 to 10)['X' = 'x']) = 10", true, 1),
    ] {
        let resolver = Recording::default();
        assert_eq!(
            eval_compat(&resolver, CASELESS, None, src).unwrap(),
            Some(expected),
            "{src}"
        );
        assert_eq!(
            resolver.asked().len(),
            calls,
            "{src}: {:?}",
            resolver.asked()
        );
    }
}

/// The public iterator functions keep their signatures and resolve the default
/// collation at most once per call — and not at all for numbers.
#[test]
fn the_public_general_comparison_functions_resolve_lazily() {
    use crate::types::value::XmlValue;
    use crate::xpath::iterator::{VecNodeIterator, XmlItem};
    use crate::xpath::operators::{general_eq_iter, general_lt_iter, general_ne_iter};
    let iter = |values: Vec<XmlValue>| -> VecNodeIterator<Nav> {
        VecNodeIterator::new(values.into_iter().map(XmlItem::Atomic).collect())
    };
    let numbers = || iter((0..100).map(|i| XmlValue::integer(i.into())).collect());
    let strings = || iter(vec![XmlValue::string("X"), XmlValue::string("y")]);
    let x = || iter(vec![XmlValue::string("x")]);

    let names = NameTable::new();
    let resolver = Recording::default();
    let ctx = XPathContext::new(&names)
        .with_collation_resolver(&resolver)
        .with_default_collation(CASELESS);
    assert!(general_eq_iter(&ctx, &numbers(), &numbers()).unwrap());
    assert!(general_ne_iter(&ctx, &numbers(), &numbers()).unwrap());
    assert!(general_lt_iter(&ctx, &numbers(), &numbers()).unwrap());
    assert_eq!(resolver.asked().len(), 0);
    assert!(general_eq_iter(&ctx, &strings(), &x()).unwrap());
    assert_eq!(resolver.asked().len(), 1);
    assert!(general_lt_iter(&ctx, &x(), &strings()).unwrap());
    assert_eq!(resolver.asked().len(), 2);
}
