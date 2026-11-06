//! Benchmark for the general comparison `A = B` (XPath 2.0 §3.5.2).
//!
//! Run the full sweep with
//!
//! ```text
//! cargo bench --bench general_compare --features xsd11
//! ```
//!
//! Without `--bench` (for instance under `cargo test --all-targets`) it does a
//! small smoke pass instead, so that the target is still type-checked and
//! exercised without spending minutes on it.
//!
//! The sweep covers the four shapes that matter:
//!
//! * `a` — two disjoint sequences of `xs:string`: the worst case, no true pair,
//!   so nothing can exit early;
//! * `b` — two disjoint sequences of `xs:untypedAtomic` taken from attribute
//!   nodes of a real document, evaluated through the public XPath API;
//! * `c` — two disjoint sequences of `xs:integer`;
//! * `d` — sequences whose only true pair is the very last one.
//!
//! plus a small-operand micro pass (1×1, 3×3, 10×10) that must not regress.
//!
//! The second half measures the **filter** shapes, where one compiled expression
//! is evaluated once but its predicate evaluates the same comparison node once
//! per item of a large sequence:
//!
//! * `f` — `$big[. = $c]` and `$big[not(. = $c)]` over `xs:integer`, `xs:string`
//!   and `xs:untypedAtomic` attribute nodes, for every combination of
//!   |big| ∈ {10⁴, 10⁵, 10⁶} and |c| ∈ {10², 10⁴, 10⁵};
//! * `g` — the guards that must not regress: a comparison evaluated once, a
//!   3 × 3 comparison evaluated a million times, a singleton invariant operand,
//!   and two operands that both vary;
//! * `t` — the crossover that sets `compare_cache::MIN_INVARIANT_ITEMS`.
//!
//! Every filter row also reports a **control**: the same run with `count()` in
//! place of the predicate, which is what binding the two variables costs on its
//! own (the sequences are cloned into the dynamic context on every run). Subtract
//! it to see the comparison's own cost.

use std::time::{Duration, Instant};

use xsd_schema::namespace::table::NameTable;
use xsd_schema::types::value::XmlValue;
use xsd_schema::xpath::api::XPathExpr;
use xsd_schema::xpath::collation::{Collation, CollationResolver};
use xsd_schema::xpath::iterator::VecNodeIterator;
use xsd_schema::xpath::operators::general_eq_iter;
use xsd_schema::xpath::{RoXmlNavigator, XPathContext, XPathValue, XmlItem};

type Nav = RoXmlNavigator<'static>;

/// Stop growing a shape once a single evaluation costs more than this, so that
/// the quadratic baseline can be measured at the sizes where it still returns.
const GIVE_UP_AFTER: Duration = Duration::from_secs(5);

fn sequence(values: Vec<XmlValue>) -> VecNodeIterator<Nav> {
    VecNodeIterator::new(values.into_iter().map(XmlItem::Atomic).collect())
}

fn strings(prefix: &str, count: usize) -> Vec<XmlValue> {
    (0..count)
        .map(|i| XmlValue::string(format!("{prefix}-{i:07}")))
        .collect()
}

fn integers(offset: i64, count: usize) -> Vec<XmlValue> {
    (0..count)
        .map(|i| XmlValue::integer(num_bigint::BigInt::from(offset + i as i64)))
        .collect()
}

/// Time one closure, repeating it until at least `floor` has elapsed, and
/// report the mean duration of a single call.
fn time<F: FnMut()>(floor: Duration, mut body: F) -> Duration {
    // One warm-up run, which also tells us whether a single call is already
    // expensive enough that repeating it is pointless.
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

fn row(label: &str, size: usize, elapsed: Duration) {
    println!("{label:<34} n = {size:>7}   {:>12.3?}", elapsed);
}

fn skipped(label: &str, size: usize) {
    println!("{label:<34} n = {size:>7}   {:>12}", "not run");
}

// ---------------------------------------------------------------------------
// Atomic-sequence shapes
// ---------------------------------------------------------------------------

fn bench_atomic_shapes(sizes: &[usize], floor: Duration) {
    let names = NameTable::new();
    let context = XPathContext::new(&names);

    let mut run_strings = true;
    let mut run_integers = true;
    let mut run_tail_hit = true;

    for &size in sizes {
        if run_strings {
            let left = sequence(strings("left", size));
            let right = sequence(strings("right", size));
            let elapsed = time(floor, || {
                let hit = general_eq_iter(&context, &left, &right).unwrap();
                assert!(!hit);
            });
            row("(a) disjoint xs:string", size, elapsed);
            run_strings = elapsed < GIVE_UP_AFTER;
        } else {
            skipped("(a) disjoint xs:string", size);
        }

        if run_integers {
            let left = sequence(integers(0, size));
            let right = sequence(integers(1_000_000_000, size));
            let elapsed = time(floor, || {
                let hit = general_eq_iter(&context, &left, &right).unwrap();
                assert!(!hit);
            });
            row("(c) disjoint xs:integer", size, elapsed);
            run_integers = elapsed < GIVE_UP_AFTER;
        } else {
            skipped("(c) disjoint xs:integer", size);
        }

        if run_tail_hit {
            let mut left_values = strings("left", size);
            let mut right_values = strings("right", size);
            // The only true pair is (last of A, last of B).
            if let (Some(l), Some(r)) = (left_values.last_mut(), right_values.last_mut()) {
                *l = XmlValue::string("needle");
                *r = XmlValue::string("needle");
            }
            let left = sequence(left_values);
            let right = sequence(right_values);
            let elapsed = time(floor, || {
                let hit = general_eq_iter(&context, &left, &right).unwrap();
                assert!(hit);
            });
            row("(d) single hit at the very end", size, elapsed);
            run_tail_hit = elapsed < GIVE_UP_AFTER;
        } else {
            skipped("(d) single hit at the very end", size);
        }
    }
}

// ---------------------------------------------------------------------------
// The same shape under a host collation
// ---------------------------------------------------------------------------

/// An ASCII case-insensitive collation that offers a sort key, so the hash
/// index keys by it.
struct Caseless;

impl Collation for Caseless {
    fn compare(&self, a: &str, b: &str) -> std::cmp::Ordering {
        a.bytes()
            .map(|b| b.to_ascii_lowercase())
            .cmp(b.bytes().map(|b| b.to_ascii_lowercase()))
    }

    fn sort_key(&self, s: &str) -> Option<Vec<u8>> {
        Some(s.bytes().map(|b| b.to_ascii_lowercase()).collect())
    }
}

/// The same ordering with no sort key, so the index declines and the pairwise
/// loop answers.
struct CaselessNoKey;

impl Collation for CaselessNoKey {
    fn compare(&self, a: &str, b: &str) -> std::cmp::Ordering {
        Caseless.compare(a, b)
    }
}

#[derive(Debug)]
struct BenchCollations {
    with_key: bool,
}

impl CollationResolver for BenchCollations {
    fn resolve(&self, _uri: &str) -> Option<std::rc::Rc<dyn Collation>> {
        Some(if self.with_key {
            std::rc::Rc::new(Caseless) as std::rc::Rc<dyn Collation>
        } else {
            std::rc::Rc::new(CaselessNoKey) as std::rc::Rc<dyn Collation>
        })
    }
}

const BENCH_COLLATION: &str = "http://example.com/collation/bench";

/// What a non-codepoint default collation costs on the `(a)` shape — the one
/// the index is built for. The codepoint rows above must not move; these are
/// the price of asking for something else.
fn bench_collations(sizes: &[usize], floor: Duration) {
    let names = NameTable::new();
    let with_key = BenchCollations { with_key: true };
    let without_key = BenchCollations { with_key: false };

    let keyed = XPathContext::new(&names)
        .with_collation_resolver(&with_key)
        .with_default_collation(BENCH_COLLATION);
    let unkeyed = XPathContext::new(&names)
        .with_collation_resolver(&without_key)
        .with_default_collation(BENCH_COLLATION);

    let mut run_keyed = true;
    let mut run_unkeyed = true;

    for &size in sizes {
        if run_keyed {
            let left = sequence(strings("left", size));
            let right = sequence(strings("right", size));
            let elapsed = time(floor, || {
                let hit = general_eq_iter(&keyed, &left, &right).unwrap();
                assert!(!hit);
            });
            row("(e) collation, with sort_key", size, elapsed);
            run_keyed = elapsed < GIVE_UP_AFTER;
        } else {
            skipped("(e) collation, with sort_key", size);
        }

        if run_unkeyed {
            let left = sequence(strings("left", size));
            let right = sequence(strings("right", size));
            let elapsed = time(floor, || {
                let hit = general_eq_iter(&unkeyed, &left, &right).unwrap();
                assert!(!hit);
            });
            row("(e) collation, no sort_key", size, elapsed);
            run_unkeyed = elapsed < GIVE_UP_AFTER;
        } else {
            skipped("(e) collation, no sort_key", size);
        }
    }
}

// ---------------------------------------------------------------------------
// Attribute nodes through the public API
// ---------------------------------------------------------------------------

fn document_text(size: usize) -> String {
    let mut xml = String::with_capacity(size * 40);
    xml.push_str("<d><p>");
    for i in 0..size {
        xml.push_str(&format!("<a v=\"left-{i:07}\"/>"));
    }
    xml.push_str("</p><q>");
    for i in 0..size {
        xml.push_str(&format!("<b v=\"right-{i:07}\"/>"));
    }
    xml.push_str("</q></d>");
    xml
}

fn bench_untyped_attributes(sizes: &[usize], floor: Duration) {
    let names = NameTable::new();
    let context = XPathContext::new(&names);
    let expr = XPathExpr::compile("/d/p/a/@v = /d/q/b/@v", &context).unwrap();

    let mut keep_going = true;
    for &size in sizes {
        if !keep_going {
            skipped("(b) untypedAtomic attributes", size);
            continue;
        }
        let text = document_text(size);
        let doc = roxmltree::Document::parse(&text).unwrap();
        let elapsed = time(floor, || {
            let value = expr
                .evaluator(&context)
                .run_with_node(RoXmlNavigator::new(&doc))
                .unwrap();
            let items = value.into_vec();
            assert_eq!(items.len(), 1);
        });
        row("(b) untypedAtomic attributes", size, elapsed);
        keep_going = elapsed < GIVE_UP_AFTER;
    }
}

// ---------------------------------------------------------------------------
// Small operands
// ---------------------------------------------------------------------------

fn bench_small_operands(floor: Duration) {
    let names = NameTable::new();
    let context = XPathContext::new(&names);

    for size in [1usize, 2, 3, 4, 5, 6, 8, 10, 16] {
        let left = sequence(strings("left", size));
        let right = sequence(strings("right", size));
        let elapsed = time(floor, || {
            let hit = general_eq_iter(&context, &left, &right).unwrap();
            assert!(!hit);
        });
        row("(small) disjoint xs:string", size, elapsed);

        let left = sequence(integers(0, size));
        let right = sequence(integers(1_000_000_000, size));
        let elapsed = time(floor, || {
            let hit = general_eq_iter(&context, &left, &right).unwrap();
            assert!(!hit);
        });
        row("(small) disjoint xs:integer", size, elapsed);
    }
}

// ---------------------------------------------------------------------------
// Filter shapes: one expression, one run, the same comparison node evaluated
// once per item of a large sequence
// ---------------------------------------------------------------------------

/// Bind `$big` and `$c` and evaluate `source`, returning how long one whole run
/// takes on average.
fn time_filter(
    context: &XPathContext,
    source: &str,
    big: &XPathValue<Nav>,
    c: &XPathValue<Nav>,
    floor: Duration,
) -> Duration {
    let expr = XPathExpr::compile_with_vars(source, context, &["big", "c"]).unwrap();
    time(floor, || {
        let value = expr
            .evaluator(context)
            .run_with::<Nav, _>(|eval| {
                eval.set_variable_by_name("big", big.clone()).unwrap();
                eval.set_variable_by_name("c", c.clone()).unwrap();
            })
            .unwrap();
        std::hint::black_box(value.len());
    })
}

fn atomic_value(values: Vec<XmlValue>) -> XPathValue<Nav> {
    XPathValue::from_sequence(values.into_iter().map(XmlItem::Atomic).collect())
}

/// A document with `count` attributes under `/d/p/a`, and the navigators of those
/// attribute nodes — cheap to clone, and atomizing them yields `xs:untypedAtomic`.
fn attribute_nodes(
    context: &XPathContext,
    doc: &'static roxmltree::Document<'static>,
    path: &str,
) -> XPathValue<Nav> {
    let expr = XPathExpr::compile(path, context).unwrap();
    let nodes = expr
        .evaluator(context)
        .run_with_node(RoXmlNavigator::new(doc))
        .unwrap()
        .into_vec();
    XPathValue::from_sequence(nodes)
}

fn leak_document(text: String) -> &'static roxmltree::Document<'static> {
    let text: &'static str = Box::leak(text.into_boxed_str());
    Box::leak(Box::new(roxmltree::Document::parse(text).unwrap()))
}

fn filter_row(label: &str, big: usize, c: usize, elapsed: Duration, control: Duration) {
    println!(
        "{label:<38} |big| = {big:>7}  |c| = {c:>6}   {:>12.3?}   (control {:>10.3?})",
        elapsed, control
    );
}

/// A shape that is measured at growing sizes, and how long the last measurement
/// took, so that a run can stop before an evaluation that would take hours.
///
/// Without an index these shapes cost `|big| · |c|`; with one they are flat in
/// `|c|`. The budget therefore starts out assuming the worst — cost linear in
/// `|c|` — and then uses the growth it actually observes, so a build that has
/// become flat is not skipped on the strength of an extrapolation that no longer
/// holds. `last` is reset for every `|big|`; the observed exponent is not, because
/// it is a property of the implementation rather than of the size.
struct Budget {
    last: Option<(usize, Duration)>,
    exponent: f64,
}

/// How long one evaluation may be projected to take before it is skipped.
const PROJECTION_LIMIT: Duration = Duration::from_secs(20);

/// The two operands of one filter shape, or `None` when that shape is not
/// measured at this size.
type Operands<'a> = Option<(&'a XPathValue<Nav>, &'a XPathValue<Nav>)>;

impl Budget {
    fn new() -> Self {
        Self {
            last: None,
            exponent: 1.0,
        }
    }

    fn restart(&mut self) {
        self.last = None;
    }

    fn allows(&self, c: usize) -> bool {
        match self.last {
            Some((previous, elapsed)) if previous > 0 && c > previous => {
                let growth = (c as f64 / previous as f64).powf(self.exponent);
                elapsed.mul_f64(growth) < PROJECTION_LIMIT
            }
            _ => true,
        }
    }

    fn record(&mut self, c: usize, elapsed: Duration) {
        if let Some((previous, before)) = self.last {
            if c > previous && before.as_secs_f64() > 0.0 {
                let sizes = (c as f64 / previous as f64).ln();
                let times = (elapsed.as_secs_f64() / before.as_secs_f64()).ln();
                if sizes > 0.0 {
                    self.exponent = (times / sizes).clamp(0.0, 1.0);
                }
            }
        }
        self.last = Some((c, elapsed));
    }
}

fn bench_filters(bigs: &[usize], cs: &[usize], floor: Duration, untyped_limit: usize) {
    let names = NameTable::new();
    let context = XPathContext::new(&names);

    // One budget per (kind, predicate), kept across the `|big|` sizes so that a
    // flat implementation is recognised as flat.
    let mut budgets: Vec<Budget> = (0..9).map(|_| Budget::new()).collect();

    for &big_size in bigs {
        for budget in &mut budgets {
            budget.restart();
        }
        let big_integers = atomic_value(integers(0, big_size));
        let big_texts = atomic_value(strings("left", big_size));
        let big_nodes = if big_size <= untyped_limit {
            Some(attribute_nodes(
                &context,
                leak_document(document_text(big_size)),
                "/d/p/a/@v",
            ))
        } else {
            None
        };

        for &c_size in cs {
            if c_size > big_size {
                continue;
            }
            let c_integers = atomic_value(integers(1_000_000_000, c_size));
            let c_texts = atomic_value(strings("right", c_size));
            let c_nodes = big_nodes.as_ref().map(|_| {
                attribute_nodes(&context, leak_document(document_text(c_size)), "/d/q/b/@v")
            });

            let kinds: [(&str, Operands<'_>); 3] = [
                ("(f) xs:integer", Some((&big_integers, &c_integers))),
                ("(f) xs:string", Some((&big_texts, &c_texts))),
                (
                    "(f) untyped attributes",
                    match (big_nodes.as_ref(), c_nodes.as_ref()) {
                        (Some(big), Some(c)) => Some((big, c)),
                        _ => None,
                    },
                ),
            ];
            let predicates = [
                ("$big[. = $c]", ". = $c"),
                ("$big[not(. = $c)]", "not(. = $c)"),
                ("$big[. != $c]", ". != $c"),
            ];

            for (kind, operands) in kinds.iter().enumerate() {
                let Some((big, c)) = operands.1 else {
                    continue;
                };
                let control = time_filter(&context, "count($big) + count($c)", big, c, floor);
                for (index, &(predicate, short)) in predicates.iter().enumerate() {
                    let label = format!("{} `{}`", operands.0, short);
                    let budget = &mut budgets[kind * 3 + index];
                    if !budget.allows(c_size) {
                        println!(
                            "{label:<38} |big| = {big_size:>7}  |c| = {c_size:>6}      not run"
                        );
                        continue;
                    }
                    let elapsed = time_filter(&context, predicate, big, c, floor);
                    budget.record(c_size, elapsed);
                    filter_row(&label, big_size, c_size, elapsed, control);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The guards: none of these may get slower
// ---------------------------------------------------------------------------

fn bench_guards(floor: Duration) {
    let names = NameTable::new();
    let context = XPathContext::new(&names);

    // (g1) one comparison, evaluated exactly once, at a size where an index
    // would pay if it were ever probed again.
    for size in [100usize, 10_000] {
        let left = atomic_value(integers(0, size));
        let right = atomic_value(integers(1_000_000_000, size));
        let control = time_filter(&context, "count($big) + count($c)", &left, &right, floor);
        let elapsed = time_filter(&context, "$big = $c", &left, &right, floor);
        filter_row("(g1) evaluated once", size, size, elapsed, control);
    }

    // (g2) a 3 x 3 comparison of two invariant operands, evaluated once per item
    // of a million: the per-evaluation cost of looking in the cache at all.
    let million = atomic_value(integers(0, 1_000_000));
    let three = atomic_value(integers(5_000_000, 3));
    let control = time_filter(&context, "count($big) + count($c)", &million, &three, floor);
    let elapsed = time_filter(&context, "$big[$c = $c]", &million, &three, floor);
    filter_row("(g2) 3x3, a million times", 1_000_000, 3, elapsed, control);

    // (g3) a singleton invariant operand — the shape of `@id = 'x'`.
    let one = atomic_value(integers(7, 1));
    let control = time_filter(&context, "count($big) + count($c)", &million, &one, floor);
    let elapsed = time_filter(&context, "$big[. = $c]", &million, &one, floor);
    filter_row("(g3) singleton operand", 1_000_000, 1, elapsed, control);

    // (g4) both operands vary with the focus: no index is possible.
    let control = time_filter(&context, "count($big) + count($c)", &million, &three, floor);
    let elapsed = time_filter(&context, "$big[. = .]", &million, &three, floor);
    filter_row("(g4) both operands vary", 1_000_000, 0, elapsed, control);

    // (g5) the commonest predicate shape of all: a literal operand.
    let elapsed = time_filter(&context, "$big[. = 7]", &million, &three, floor);
    filter_row("(g5) integer literal", 1_000_000, 1, elapsed, control);
    let million_strings = atomic_value(strings("left", 1_000_000));
    let control_strings = time_filter(
        &context,
        "count($big) + count($c)",
        &million_strings,
        &three,
        floor,
    );
    let elapsed = time_filter(&context, "$big[. = 'x']", &million_strings, &three, floor);
    filter_row(
        "(g5) string literal",
        1_000_000,
        1,
        elapsed,
        control_strings,
    );

    // (g6) a varying operand past MAX_VARYING_ITEMS, where the index declines.
    let hundred = atomic_value(integers(9_000_000, 100));
    let thousand = atomic_value(integers(0, 1_000));
    let control = time_filter(
        &context,
        "count($big) + count($c)",
        &thousand,
        &hundred,
        floor,
    );
    let elapsed = time_filter(&context, "$big[$c = $c]", &thousand, &hundred, floor);
    filter_row("(g6) 100-item varying side", 1_000, 100, elapsed, control);
}

// ---------------------------------------------------------------------------
// Where indexing an invariant operand starts to pay
// ---------------------------------------------------------------------------

fn bench_threshold(floor: Duration) {
    let names = NameTable::new();
    let context = XPathContext::new(&names);
    let big = atomic_value(integers(0, 200_000));
    for c_size in [1usize, 2, 4, 8, 12, 16, 24, 32, 48, 64, 96, 128] {
        let c = atomic_value(integers(1_000_000_000, c_size));
        let control = time_filter(&context, "count($big) + count($c)", &big, &c, floor);
        let elapsed = time_filter(&context, "$big[. = $c]", &big, &c, floor);
        filter_row("(t) crossover", 200_000, c_size, elapsed, control);
    }
}

fn main() {
    let full = std::env::args().any(|arg| arg == "--bench");
    let (sizes, floor): (&[usize], Duration) = if full {
        (&[100, 1_000, 10_000, 100_000], Duration::from_millis(400))
    } else {
        (&[100], Duration::from_millis(5))
    };

    println!("== general comparison `A = B` ==");
    bench_atomic_shapes(sizes, floor);
    bench_untyped_attributes(sizes, floor);
    bench_small_operands(floor);
    // The codepoint rows above must not move; these show what a host collation
    // costs, with and without a sort key.
    bench_collations(sizes, floor);

    println!();
    println!("== the same comparison node, evaluated once per item ==");
    if full {
        bench_filters(
            &[10_000, 100_000, 1_000_000],
            &[100, 10_000, 100_000],
            floor,
            100_000,
        );
        bench_guards(floor);
        bench_threshold(floor);
    } else {
        bench_filters(&[200], &[20], floor, 200);
        bench_guards(Duration::from_millis(1));
        bench_threshold(Duration::from_millis(1));
    }
}
