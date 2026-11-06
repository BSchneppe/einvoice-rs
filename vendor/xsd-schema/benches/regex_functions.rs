//! Benchmark for the XPath 2.0 regular-expression functions `fn:matches`,
//! `fn:replace` and `fn:tokenize` (XPath 2.0 §3.5, F&O §7.6).
//!
//! Run the full sweep with
//!
//! ```text
//! cargo bench --bench regex_functions --features xsd11
//! ```
//!
//! Without `--bench` (for instance under `cargo test --all-targets`) it does a
//! small smoke pass instead, so that the target is still type-checked and
//! exercised without spending minutes on it.
//!
//! What is on trial is the **compilation** of the pattern, not the matching.
//! Each of the three functions turns its `$pattern` and `$flags` arguments into
//! a compiled program, and in the commonest shape of all —
//!
//! ```text
//!     $items[matches(., '\p{Ll}')]
//! ```
//!
//! — that pattern is a literal that cannot change, while the predicate is
//! evaluated once per item. A pattern that names a Unicode category also copies
//! that category's code-point set into its program (the engine builds each set
//! once per process), which makes the compile dearer than the match itself on a
//! short string. The rows:
//!
//! * `(a)` a constant pattern through a predicate over 10⁴ and 10⁵ items, for a
//!   cheap ASCII pattern and for a category pattern;
//! * `(b)` the guards that must not regress: one `matches()`, one `replace()`
//!   and one `tokenize()` per run, plus the empty control that shows what the
//!   run itself costs;
//! * `(c)` the churn guard: a pattern taken from the item, so that a run sees
//!   far more distinct patterns than any bounded cache can hold;
//! * `(d)` `replace()` and `tokenize()` with a constant pattern in a loop.
//!
//! Row `(a)` reports a **control** as well: the same loop with the `matches()`
//! call replaced by a comparison of the item against a string, which is what
//! binding and walking the sequence costs on its own.

use std::time::{Duration, Instant};

use xsd_schema::namespace::table::NameTable;
use xsd_schema::types::value::XmlValue;
use xsd_schema::xpath::api::XPathExpr;
use xsd_schema::xpath::{RoXmlNavigator, XPathContext, XPathValue, XmlItem};

type Nav = RoXmlNavigator<'static>;

/// Time one closure, repeating it until at least `floor` has elapsed, and
/// report the mean duration of a single call.
fn time<F: FnMut()>(floor: Duration, mut body: F) -> Duration {
    let start = Instant::now();
    body();
    let first = start.elapsed();
    if first >= floor {
        return first;
    }
    let mut runs = 1u32;
    let start = Instant::now();
    while start.elapsed() < floor {
        body();
        runs += 1;
    }
    start.elapsed() / runs
}

fn atomic_value(values: Vec<XmlValue>) -> XPathValue<Nav> {
    XPathValue::from_sequence(values.into_iter().map(XmlItem::Atomic).collect())
}

/// `count` short mixed-case strings with a digit run and a comma in them, so
/// that every pattern below has something to find.
fn subjects(count: usize) -> XPathValue<Nav> {
    atomic_value(
        (0..count)
            .map(|i| XmlValue::string(format!("Item-{i:05},tail")))
            .collect(),
    )
}

/// `count` distinct patterns, each matching a different literal.
fn patterns(count: usize) -> XPathValue<Nav> {
    atomic_value(
        (0..count)
            .map(|i| XmlValue::string(format!("Item-{i:05}")))
            .collect(),
    )
}

/// Evaluate `source` once per timed iteration with `$big` bound to `big`.
fn time_expr(
    context: &XPathContext,
    source: &str,
    big: &XPathValue<Nav>,
    floor: Duration,
) -> Duration {
    let expr = XPathExpr::compile_with_vars(source, context, &["big"]).unwrap();
    time(floor, || {
        let value = expr
            .evaluator(context)
            .run_with::<Nav, _>(|eval| {
                eval.set_variable_by_name("big", big.clone()).unwrap();
            })
            .unwrap();
        std::hint::black_box(value.len());
    })
}

fn row(label: &str, size: usize, elapsed: Duration) {
    println!("{label:<46} n = {size:>7}   {elapsed:>12.3?}");
}

fn row_with_control(label: &str, size: usize, elapsed: Duration, control: Duration) {
    println!("{label:<46} n = {size:>7}   {elapsed:>12.3?}   (control {control:>10.3?})");
}

// ---------------------------------------------------------------------------
// (a) A constant pattern, evaluated once per item
// ---------------------------------------------------------------------------

fn bench_constant_pattern(sizes: &[usize], floor: Duration) {
    let names = NameTable::new();
    let context = XPathContext::new(&names);

    for &size in sizes {
        let big = subjects(size);
        let control = time_expr(&context, "$big[. = 'nothing']", &big, floor);

        let elapsed = time_expr(&context, "$big[matches(., 'Item-0*1')]", &big, floor);
        row_with_control("(a) matches(., 'Item-0*1')", size, elapsed, control);

        let elapsed = time_expr(&context, r"$big[matches(., '\p{Ll}')]", &big, floor);
        row_with_control(r"(a) matches(., '\p{Ll}')", size, elapsed, control);

        let elapsed = time_expr(&context, r"$big[matches(., '\d{3}', 'i')]", &big, floor);
        row_with_control(r"(a) matches(., '\d{3}', 'i')", size, elapsed, control);
    }
}

// ---------------------------------------------------------------------------
// (b) One call per run: nothing here may get slower
// ---------------------------------------------------------------------------

fn bench_single_call(floor: Duration) {
    let names = NameTable::new();
    let context = XPathContext::new(&names);
    let one = atomic_value(vec![XmlValue::string("Item-00001,tail")]);

    let elapsed = time_expr(&context, "count($big)", &one, floor);
    row("(b) control: no regex at all", 1, elapsed);

    let elapsed = time_expr(&context, "matches('abracadabra', 'bra')", &one, floor);
    row("(b) one matches(), ASCII pattern", 1, elapsed);

    let elapsed = time_expr(&context, r"matches('abracadabra', '\p{Ll}')", &one, floor);
    row(r"(b) one matches(), '\p{Ll}'", 1, elapsed);

    let elapsed = time_expr(&context, "replace('abracadabra', 'a', 'X')", &one, floor);
    row("(b) one replace()", 1, elapsed);

    let elapsed = time_expr(&context, "tokenize('a,b,c', ',')", &one, floor);
    row("(b) one tokenize()", 1, elapsed);
}

// ---------------------------------------------------------------------------
// (c) A pattern taken from the item: more distinct patterns than any cache
//     can hold, so the cache must not make this slower or grow without bound
// ---------------------------------------------------------------------------

fn bench_pattern_churn(sizes: &[usize], floor: Duration) {
    let names = NameTable::new();
    let context = XPathContext::new(&names);

    for &size in sizes {
        let pats = patterns(size);
        let elapsed = time_expr(&context, "$big[matches('Item-00007', .)]", &pats, floor);
        row("(c) distinct pattern per item", size, elapsed);
    }
}

// ---------------------------------------------------------------------------
// (d) replace() and tokenize() with a constant pattern, in a loop
// ---------------------------------------------------------------------------

fn bench_replace_tokenize(sizes: &[usize], floor: Duration) {
    let names = NameTable::new();
    let context = XPathContext::new(&names);

    for &size in sizes {
        let big = subjects(size);

        let elapsed = time_expr(
            &context,
            r"count(for $s in $big return replace($s, '\d+', 'N'))",
            &big,
            floor,
        );
        row(r"(d) replace($s, '\d+', 'N')", size, elapsed);

        let elapsed = time_expr(
            &context,
            r"count(for $s in $big return tokenize($s, '\p{P}'))",
            &big,
            floor,
        );
        row(r"(d) tokenize($s, '\p{P}')", size, elapsed);
    }
}

fn main() {
    let full = std::env::args().any(|arg| arg == "--bench");
    let (sizes, small, floor): (&[usize], &[usize], Duration) = if full {
        (
            &[10_000, 100_000],
            &[1_000, 10_000],
            Duration::from_millis(400),
        )
    } else {
        (&[50], &[50], Duration::from_millis(5))
    };

    println!("== a constant pattern, evaluated once per item ==");
    bench_constant_pattern(sizes, floor);

    println!();
    println!("== one call per run ==");
    bench_single_call(floor);

    println!();
    println!("== a distinct pattern per item ==");
    bench_pattern_churn(small, floor);

    println!();
    println!("== replace() and tokenize() in a loop ==");
    bench_replace_tokenize(small, floor);
}
