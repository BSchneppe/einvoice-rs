//! Tests for the general-comparison index that is reused across evaluations.
//!
//! Two layers:
//!
//! * a **differential** layer, which drives [`indexed_eq`] / [`indexed_ne`]
//!   directly — so the index is built on the first use and at any size, the
//!   "threshold 0" setting the thresholds below never allow in production — and
//!   compares every answer against the Cartesian-product loop
//!   (`general_eq_iter_pairwise` / `general_ne_iter_pairwise`), error value
//!   included. The whole corpus of the previous wave is reused: all atomic types,
//!   numeric-/date-/non-numeric-looking `xs:untypedAtomic`, `NaN`, `±0`, integers
//!   beyond `xs:double`'s precision and beyond `xs:decimal`'s range, decimals with
//!   trailing zeros, timezoned and untimezoned dates and times, the three duration
//!   types, QNames, list-typed and union-typed values, and empty sequences. Each
//!   case runs a whole **stream** of varying operands against one index, because
//!   reuse is what is on trial here.
//!
//! * an **end-to-end** layer, which evaluates real expressions through
//!   `eval_node` with the production thresholds, so the lazy path — first
//!   evaluation untouched, index built on the second, probed afterwards — and the
//!   invariance classification are exercised as they ship.

use super::*;
use crate::namespace::context::NamespaceContextSnapshot;
use crate::namespace::qname::QualifiedName;
use crate::namespace::NameTable;
use crate::types::sequence::SequenceType;
use crate::types::value::TimezoneOffset;
use crate::xpath::ast::{AstNode, BinaryOpKind};
use crate::xpath::bind::bind_node;
use crate::xpath::context::NameBinder;
use crate::xpath::context::VarStore;
use crate::xpath::functions::{DynamicFunctionSignature, FunctionSet};
use crate::xpath::general_compare::tests::{draw, families, Rng};
use crate::xpath::iterator::BufferedNodeIterator;
use crate::xpath::operators::{general_eq_iter_pairwise, general_ne_iter_pairwise};
use crate::xpath::parser::parse;
use crate::xpath::RoXmlNavigator;
use num_bigint::BigInt;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

type Nav = RoXmlNavigator<'static>;

// ============================================================================
// Differential layer
// ============================================================================

fn iter_of(values: &[XmlValue]) -> VecNodeIterator<Nav> {
    VecNodeIterator::new(values.iter().cloned().map(XmlItem::Atomic).collect())
}

/// What a path produced, rendered so that two runs can be compared exactly — the
/// error value included, not only its code.
fn render(result: &Result<bool, XPathError>) -> String {
    format!("{:?}", result)
}

/// The Cartesian-product answer, which everything here must reproduce.
fn pairwise(
    context: &XPathContext,
    left: &[XmlValue],
    right: &[XmlValue],
    is_eq: bool,
) -> Result<bool, XPathError> {
    let left_iter = iter_of(left);
    let right_buf = BufferedNodeIterator::preload(iter_of(right)).unwrap();
    if is_eq {
        general_eq_iter_pairwise(context, &left_iter, &right_buf)
    } else {
        general_ne_iter_pairwise(context, &left_iter, &right_buf)
    }
}

/// The values the production path hands to the index for an operand made of
/// `values`: its items atomized by `general_compare::atomize_items`, exactly as
/// `install` and `probe` atomize them — so a packed list value (the stored
/// typed value of a list-typed node) arrives as its members, as the pairwise
/// loop sees it.
fn atomized(values: &[XmlValue]) -> Vec<XmlValue> {
    let items: Vec<XmlItem<Nav>> = values.iter().cloned().map(XmlItem::Atomic).collect();
    general_compare::atomize_items(&items).expect("atomic items always atomize")
}

/// Run a whole stream of varying operands against **one** cached index and check
/// every answer against the pairwise loop. Returns `(decided, total)`.
fn check_stream(
    context: &XPathContext,
    side: Side,
    invariant: &[XmlValue],
    stream: &[Vec<XmlValue>],
    is_eq: bool,
) -> (usize, usize) {
    let Some(mut operand) = CachedOperand::new(atomized(invariant)) else {
        // A value whose comparison class is not modelled; no index is possible,
        // which the production path expresses by never installing one.
        return (0, 0);
    };
    let mut decided = 0usize;
    let mut total = 0usize;
    for varying in stream {
        let (left, right) = side.order(varying.as_slice(), invariant);
        let expected = pairwise(context, left, right, is_eq);
        let active = crate::xpath::collation::resolve_default(context);
        let probe_values = atomized(varying);
        let outcome = if is_eq {
            indexed_eq(
                context,
                side,
                &mut operand,
                &probe_values,
                active.as_ref(),
                crate::xpath::operators::static_timezone(context),
            )
        } else {
            indexed_ne(
                context,
                side,
                &mut operand,
                &probe_values,
                active.as_ref(),
                crate::xpath::operators::static_timezone(context),
            )
        };
        total += 1;
        let got: Result<bool, XPathError> = match outcome {
            FastOutcome::Decided(result) => Ok(result),
            FastOutcome::Raise(err) => Err(err),
            FastOutcome::Fallback => continue,
        };
        decided += 1;
        assert_eq!(
            render(&expected),
            render(&got),
            "{} diverged for side {:?}\n  invariant = {:?}\n  varying   = {:?}",
            if is_eq { "`=`" } else { "`!=`" },
            side,
            invariant,
            varying
        );
    }
    (decided, total)
}

const BOTH_SIDES: [Side; 2] = [Side::Left, Side::Right];

#[test]
fn the_cached_index_agrees_with_the_pairwise_path_on_equality() {
    let names = NameTable::new();
    let context = XPathContext::new(&names);
    let families = families(&names);
    let mut rng = Rng(0x243F_6A88_85A3_08D3);

    let mut decided = 0usize;
    let mut total = 0usize;
    for _ in 0..700 {
        let invariant = draw(&mut rng, &families);
        let stream: Vec<Vec<XmlValue>> = (0..10).map(|_| draw(&mut rng, &families)).collect();
        for side in BOTH_SIDES {
            let (d, t) = check_stream(&context, side, &invariant, &stream, true);
            decided += d;
            total += t;
        }
    }
    println!("`=` streams: the cached index decided {decided} of {total}");
    assert!(
        decided * 4 >= total,
        "the cached index decided only {decided} of {total} cases"
    );
}

#[test]
fn the_cached_index_agrees_with_the_pairwise_path_on_inequality() {
    let names = NameTable::new();
    let context = XPathContext::new(&names);
    let families = families(&names);
    let mut rng = Rng(0x13198A2E_03707344);

    let mut decided = 0usize;
    let mut total = 0usize;
    for _ in 0..700 {
        let invariant = draw(&mut rng, &families);
        let stream: Vec<Vec<XmlValue>> = (0..10).map(|_| draw(&mut rng, &families)).collect();
        for side in BOTH_SIDES {
            let (d, t) = check_stream(&context, side, &invariant, &stream, false);
            decided += d;
            total += t;
        }
    }
    println!("`!=` streams: the cached index decided {decided} of {total}");
    assert!(
        decided * 5 >= total,
        "the cached index decided only {decided} of {total} cases"
    );
}

/// The workload's own shape: a large invariant operand of one family, probed with
/// every single corpus value in turn. Large enough that the hash chains are real.
#[test]
fn every_corpus_value_probes_a_large_index_correctly() {
    let names = NameTable::new();
    let context = XPathContext::new(&names);
    let families = families(&names);
    let all: Vec<XmlValue> = families.iter().flatten().cloned().collect();
    // Every corpus value as its own singleton probe, plus the empty sequence.
    let mut stream: Vec<Vec<XmlValue>> = all.iter().map(|v| vec![v.clone()]).collect();
    stream.push(Vec::new());

    let mut decided = 0usize;
    let mut total = 0usize;
    for family in &families {
        // Repeat the family until the index is well past the production
        // threshold, so that collision chains and multi-class operands are both
        // exercised.
        let invariant: Vec<XmlValue> = family
            .iter()
            .chain(family.iter())
            .chain(family.iter())
            .cloned()
            .collect();
        for side in BOTH_SIDES {
            for &is_eq in &[true, false] {
                let (d, t) = check_stream(&context, side, &invariant, &stream, is_eq);
                decided += d;
                total += t;
            }
        }
    }
    // The whole corpus as one operand: every class pair at once.
    for side in BOTH_SIDES {
        for &is_eq in &[true, false] {
            let (d, t) = check_stream(&context, side, &all, &stream, is_eq);
            decided += d;
            total += t;
        }
    }
    println!("large-index probes: the cached index decided {decided} of {total}");
    assert!(decided * 4 >= total);
}

/// A probe that raises, sitting between probes that do not: the index must answer
/// each of them exactly as the pairwise loop does, and the raising one must not
/// leave the index in a state that spoils the next.
#[test]
fn a_raising_probe_does_not_disturb_the_probes_around_it() {
    let names = NameTable::new();
    let context = XPathContext::new(&names);
    let invariant: Vec<XmlValue> = (0..40).map(|i| XmlValue::string(format!("s{i}"))).collect();
    let stream = vec![
        vec![XmlValue::string("s7")],                           // true
        vec![XmlValue::string("nope")],                         // false
        vec![XmlValue::boolean(true)],                          // raises: boolean vs string
        vec![XmlValue::string("s0")],                           // true again
        vec![XmlValue::integer(BigInt::from(1))],               // raises: numeric vs string
        vec![XmlValue::string("s39")],                          // true again
        vec![],                                                 // empty
        vec![XmlValue::boolean(false), XmlValue::string("s3")], // true wins over the raise
    ];
    for side in BOTH_SIDES {
        for &is_eq in &[true, false] {
            let (_, total) = check_stream(&context, side, &invariant, &stream, is_eq);
            assert_eq!(total, stream.len());
        }
    }
}

/// Reuse must not make an answer depend on what was probed before it: the same
/// stream in reverse, against a fresh index, has to give the same answers.
#[test]
fn an_answer_does_not_depend_on_the_order_of_the_stream() {
    let names = NameTable::new();
    let context = XPathContext::new(&names);
    let families = families(&names);
    let all: Vec<XmlValue> = families.iter().flatten().cloned().collect();
    let mut rng = Rng(0xA409_3822_299F_31D0);

    for _ in 0..200 {
        let invariant: Vec<XmlValue> = (0..30).map(|_| all[rng.below(all.len())].clone()).collect();
        let stream: Vec<Vec<XmlValue>> = (0..8).map(|_| draw(&mut rng, &families)).collect();
        let Some(mut forwards) = CachedOperand::new(invariant.clone()) else {
            continue;
        };
        let Some(mut backwards) = CachedOperand::new(invariant.clone()) else {
            continue;
        };
        let side = Side::Right;
        let ahead: Vec<String> = stream
            .iter()
            .map(|varying| {
                describe(indexed_eq(
                    &context,
                    side,
                    &mut forwards,
                    varying,
                    CollationRef::Codepoint,
                    TimezoneOffset::UTC,
                ))
            })
            .collect();
        let mut behind: Vec<String> = stream
            .iter()
            .rev()
            .map(|varying| {
                describe(indexed_eq(
                    &context,
                    side,
                    &mut backwards,
                    varying,
                    CollationRef::Codepoint,
                    TimezoneOffset::UTC,
                ))
            })
            .collect();
        behind.reverse();
        assert_eq!(ahead, behind, "invariant = {invariant:?}");
    }
}

fn describe(outcome: FastOutcome) -> String {
    match outcome {
        FastOutcome::Decided(result) => format!("decided {result}"),
        FastOutcome::Raise(err) => format!("raise {err:?}"),
        FastOutcome::Fallback => "fallback".to_string(),
    }
}

// ============================================================================
// The invariance classification
// ============================================================================

/// Parse and bind `source` with `vars` as external variables.
struct Compiled {
    arena: AstArena,
    root: AstNodeId,
    slots: usize,
    vars: Vec<VarSlotId>,
}

fn compile(names: &NameTable, ctx: &XPathContext<'_>, source: &str, vars: &[&str]) -> Compiled {
    let parsed = parse(source).unwrap_or_else(|e| panic!("parse of {source:?} failed: {e:?}"));
    let mut arena = parsed.arena;
    let root = parsed.root;
    let mut binder = NameBinder::new();
    let mut slots = Vec::new();
    for var in vars {
        slots.push(binder.push_var(QualifiedName::local(names.add(var))).slot);
    }
    binder.mark_external_boundary();
    bind_node(&mut arena, root, ctx, &mut binder)
        .unwrap_or_else(|e| panic!("bind of {source:?} failed: {e:?}"));
    Compiled {
        arena,
        root,
        slots: binder.len(),
        vars: slots,
    }
}

/// The `(node, left, right)` of the expression's only general comparison.
fn only_general_comparison(arena: &AstArena, source: &str) -> (AstNodeId, AstNodeId, AstNodeId) {
    let mut found = None;
    for (id, node) in arena.iter() {
        if let AstNode::BinaryOp(op) = node {
            if matches!(op.kind, BinaryOpKind::GeneralEq | BinaryOpKind::GeneralNe) {
                assert!(found.is_none(), "{source:?} has more than one `=`/`!=`");
                found = Some((id, op.left, op.right));
            }
        }
    }
    found.unwrap_or_else(|| panic!("{source:?} has no general comparison"))
}

/// `(left operand invariant, right operand invariant)` for the general comparison
/// of `source`.
fn invariance(source: &str, vars: &[&str]) -> (bool, bool) {
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let compiled = compile(&names, &ctx, source, vars);
    let range_vars = deps::range_var_slots(&compiled.arena);
    let (_, left, right) = only_general_comparison(&compiled.arena, source);
    (
        deps::is_run_invariant(&compiled.arena, left, &ctx, &range_vars),
        deps::is_run_invariant(&compiled.arena, right, &ctx, &range_vars),
    )
}

#[test]
fn an_operand_bound_before_the_run_is_invariant() {
    let vars = &["a", "b", "big", "c", "xs", "ys"];
    // (expression, left invariant, right invariant)
    let cases: &[(&str, bool, bool)] = &[
        // --- the workload -------------------------------------------------
        ("$big[. = $c]", false, true),
        ("$big[not(. = $c)]", false, true),
        ("$big[. != $c]", false, true),
        ("$big[@id = $c]", false, true),
        // --- variables bound before the run, literals, ranges -------------
        ("$a = $b", true, true),
        ("$a = 'x'", true, true),
        ("$a = 1", true, true),
        ("$a = (1 to 100)", true, true),
        ("$a = ('x', 'y', $b)", true, true),
        ("$a = $b + 1", true, true),
        ("$a = (if ($b) then $a else 'x')", true, true),
        // A predicate over an invariant base runs in an inner focus, so the
        // operand as a whole still does not read the focus it is entered with.
        ("$a = $b[. > 1]", true, true),
        ("$a = $b/x/y", true, true),
        ("$a = $b//x", true, true),
        // --- the focus is not invariant -----------------------------------
        ("$a = .", true, false),
        (". = $a", false, true),
        ("$a = x", true, false),
        ("$a = x/y", true, false),
        ("$a = /x", true, false),
        ("$a = //x", true, false),
        ("$a = ..", true, false),
        ("$a = @id", true, false),
        // --- no function call is trusted ----------------------------------
        ("$a = count($b)", true, false),
        ("$a = string($b)", true, false),
        ("$a = position()", true, false),
        ("$a = last()", true, false),
        ("$a = true()", true, false),
        ("$a = current-dateTime()", true, false),
        // Even inside a predicate, where the call is deterministic.
        ("$a = $b[position() eq 1]", true, false),
        // --- range variables ----------------------------------------------
        ("for $x in $xs return $x = $ys", false, true),
        ("for $x in $xs return $ys = $x", true, false),
        ("some $x in $xs satisfies $x = $ys", false, true),
        ("every $x in $xs satisfies $ys = $x", true, false),
        // A `for` inside the operand is rejected too: conservative, and it can
        // never mistake an outer binding for an inner one.
        ("$a = (for $x in $b return $x)", true, false),
        ("(for $x in $a return $x) = $b", false, true),
    ];
    for &(source, left, right) in cases {
        assert_eq!(
            invariance(source, vars),
            (left, right),
            "invariance of {source:?}"
        );
    }
}

/// A nested `for` whose "invariant" operand is rebound by the outer iteration:
/// both operands of the inner comparison must be rejected.
#[test]
fn a_variable_rebound_by_an_outer_iteration_is_not_invariant() {
    let vars = &["xs", "ys"];
    assert_eq!(
        invariance("for $x in $xs return (for $y in $ys return $x = $y)", vars),
        (false, false)
    );
    assert_eq!(
        invariance(
            "for $x in $xs return (some $y in $ys satisfies $y = $x)",
            vars
        ),
        (false, false)
    );
}

// ============================================================================
// End to end, with the production thresholds
// ============================================================================

fn sequence(values: &[XmlValue]) -> XPathValue<Nav> {
    XPathValue::from_sequence(values.iter().cloned().map(XmlItem::Atomic).collect())
}

fn atomics(value: &XPathValue<Nav>) -> Vec<XmlValue> {
    value
        .as_slice()
        .iter()
        .map(|item| match item {
            XmlItem::Atomic(atom) => atom.clone(),
            XmlItem::Node(_) => panic!("unexpected node"),
        })
        .collect()
}

/// The filter `$big[<predicate over $c>]` as the Cartesian-product loop decides
/// it: the first error wins, because `eval_predicates` walks the items in order.
fn expected_filter(
    context: &XPathContext,
    big: &[XmlValue],
    c: &[XmlValue],
    shape: Shape2,
) -> Result<Vec<XmlValue>, XPathError> {
    let mut kept = Vec::new();
    for value in big {
        let item = std::slice::from_ref(value);
        let (left, right) = if shape.reversed { (c, item) } else { (item, c) };
        let hit = pairwise(context, left, right, shape.is_eq)?;
        if hit != shape.negate {
            kept.push(value.clone());
        }
    }
    Ok(kept)
}

/// Which comparison a filter's predicate performs, so that the reference walk
/// compares the same pairs in the same order.
#[derive(Debug, Clone, Copy)]
struct Shape2 {
    is_eq: bool,
    negate: bool,
    /// The invariant operand is written on the left of the comparison.
    reversed: bool,
}

const EQ: Shape2 = Shape2 {
    is_eq: true,
    negate: false,
    reversed: false,
};
const NOT_EQ: Shape2 = Shape2 {
    is_eq: true,
    negate: true,
    reversed: false,
};
const NE: Shape2 = Shape2 {
    is_eq: false,
    negate: false,
    reversed: false,
};
const EQ_REVERSED: Shape2 = Shape2 {
    is_eq: true,
    negate: false,
    reversed: true,
};
const NE_REVERSED: Shape2 = Shape2 {
    is_eq: false,
    negate: false,
    reversed: true,
};

/// Evaluate `source` with `$big` and `$c` bound, through `eval_node`, i.e. with
/// the production thresholds and the lazy build.
fn run_filter(
    names: &NameTable,
    ctx: &XPathContext<'_>,
    source: &str,
    big: &[XmlValue],
    c: &[XmlValue],
) -> (Result<XPathValue<Nav>, XPathError>, Vec<&'static str>) {
    let compiled = compile(names, ctx, source, &["big", "c"]);
    let mut dyn_ctx: DynamicContext<'_, Nav> = DynamicContext::new(ctx, compiled.slots);
    dyn_ctx.set_variable(compiled.vars[0], sequence(big));
    dyn_ctx.set_variable(compiled.vars[1], sequence(c));
    let result = eval_node(&compiled.arena, compiled.root, &mut dyn_ctx);
    let states = states(dyn_ctx.general_compare_cache());
    (result, states)
}

fn states(cache: &GeneralCompareCache) -> Vec<&'static str> {
    cache
        .entries
        .iter()
        .map(|entry| match entry.state {
            State::Counting(_) => "counting",
            State::Off => "off",
            State::Ready(_) => "ready",
        })
        .collect()
}

/// A corpus of `$big` items wide enough that the predicate hits every route:
/// decided true, decided false, and declined.
fn big_corpus() -> Vec<XmlValue> {
    let mut values = Vec::new();
    for i in 0..60i64 {
        values.push(XmlValue::integer(BigInt::from(i)));
    }
    for i in 0..20 {
        values.push(XmlValue::string(format!("s{i}")));
        values.push(XmlValue::untyped(format!("{i}")));
    }
    values.push(XmlValue::double(f64::NAN));
    values.push(XmlValue::double(-0.0));
    values.push(XmlValue::decimal("1.00".parse().unwrap()));
    values.push(XmlValue::float(0.1));
    values.push(XmlValue::untyped("not-a-number"));
    values
}

#[test]
fn the_lazy_path_filters_exactly_as_the_pairwise_path_does() {
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let big = big_corpus();
    // Comfortably past MIN_INVARIANT_ITEMS, and mixed enough to need several
    // buckets at once.
    let mut c: Vec<XmlValue> = (0..30i64)
        .map(|i| XmlValue::integer(BigInt::from(i * 2)))
        .collect();
    c.push(XmlValue::string("s3"));
    c.push(XmlValue::untyped("7"));
    c.push(XmlValue::decimal("1".parse().unwrap()));

    for (source, shape) in [
        ("$big[. = $c]", EQ),
        ("$big[not(. = $c)]", NOT_EQ),
        ("$big[. != $c]", NE),
        ("$big[$c = .]", EQ_REVERSED),
        ("$big[$c != .]", NE_REVERSED),
    ] {
        let (actual, states) = run_filter(&names, &ctx, source, &big, &c);
        let expected = expected_filter(&ctx, &big, &c, shape);
        match (&actual, &expected) {
            (Ok(got), Ok(want)) => assert_eq!(&atomics(got), want, "{source}"),
            (Err(got), Err(want)) => {
                assert_eq!(format!("{got:?}"), format!("{want:?}"), "{source}")
            }
            (Ok(got), Err(want)) => {
                panic!("{source}: got {:?}, expected error {want:?}", atomics(got))
            }
            (Err(got), Ok(want)) => panic!("{source}: got error {got:?}, expected {want:?}"),
        }
        assert_eq!(
            states,
            vec!["ready"],
            "{source}: the index was not installed"
        );
    }
}

/// The same, with a `$big` whose items raise against `$c` — the error and the
/// items filtered before it must be the ones the pairwise loop produces.
#[test]
fn the_lazy_path_raises_the_same_error_at_the_same_item() {
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut big: Vec<XmlValue> = (0..10i64)
        .map(|i| XmlValue::integer(BigInt::from(i)))
        .collect();
    // A boolean is incomparable with an integer, so this item raises.
    big.push(XmlValue::boolean(true));
    big.extend((10..20i64).map(|i| XmlValue::integer(BigInt::from(i))));
    let c: Vec<XmlValue> = (0..30i64)
        .map(|i| XmlValue::integer(BigInt::from(i * 2)))
        .collect();

    for (source, shape) in [
        ("$big[. = $c]", EQ),
        ("$big[not(. = $c)]", NOT_EQ),
        ("$big[. != $c]", NE),
    ] {
        let (actual, _) = run_filter(&names, &ctx, source, &big, &c);
        let expected = expected_filter(&ctx, &big, &c, shape);
        assert!(expected.is_err(), "{source}: the corpus should raise");
        assert_eq!(
            format!("{:?}", actual.map(|value| atomics(&value))),
            format!("{expected:?}"),
            "{source}"
        );
    }
}

/// An operand the threshold or the classification rejects is never indexed — and
/// the answers are the same either way.
#[test]
fn an_empty_or_varying_operand_is_never_indexed() {
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    // Integers only, so that nothing raises and the comparison's answer is all
    // this test is about.
    let big: Vec<XmlValue> = (0..60i64)
        .map(|i| XmlValue::integer(BigInt::from(i)))
        .collect();

    // An empty operand is below `MIN_INVARIANT_ITEMS`: the comparison is false
    // whatever the other side holds, and the untouched path says so without
    // looking at it.
    let empty: Vec<XmlValue> = Vec::new();
    assert!(empty.len() < MIN_INVARIANT_ITEMS);
    let (actual, empty_states) = run_filter(&names, &ctx, "$big[. = $c]", &big, &empty);
    assert!(atomics(&actual.unwrap()).is_empty());
    assert_eq!(empty_states, vec!["off"]);

    // Both operands vary with the focus: nothing to cache.
    let long: Vec<XmlValue> = (0..40i64)
        .map(|i| XmlValue::integer(BigInt::from(i)))
        .collect();
    let compiled = compile(&names, &ctx, "$big[. = .]", &["big", "c"]);
    let mut dyn_ctx: DynamicContext<'_, Nav> = DynamicContext::new(&ctx, compiled.slots);
    dyn_ctx.set_variable(compiled.vars[0], sequence(&long));
    let result = eval_node(&compiled.arena, compiled.root, &mut dyn_ctx).unwrap();
    assert_eq!(atomics(&result).len(), long.len());
    assert_eq!(states(dyn_ctx.general_compare_cache()), vec!["off"]);
}

/// A varying operand larger than `MAX_VARYING_ITEMS` sends every evaluation back
/// to the untouched path, index or no index.
#[test]
fn a_large_varying_operand_is_left_to_the_untouched_path() {
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let big: Vec<XmlValue> = (0..20i64)
        .map(|i| XmlValue::integer(BigInt::from(i)))
        .collect();
    let wide: Vec<XmlValue> = (0..(MAX_VARYING_ITEMS as i64 + 1))
        .map(|i| XmlValue::integer(BigInt::from(i * 3)))
        .collect();

    // `$c = $c` makes both operands invariant and both of them too wide to probe
    // with, so the index is installed and then never usable.
    let compiled = compile(&names, &ctx, "$big[$c = $c]", &["big", "c"]);
    let mut dyn_ctx: DynamicContext<'_, Nav> = DynamicContext::new(&ctx, compiled.slots);
    dyn_ctx.set_variable(compiled.vars[0], sequence(&big));
    dyn_ctx.set_variable(compiled.vars[1], sequence(&wide));
    let result = eval_node(&compiled.arena, compiled.root, &mut dyn_ctx).unwrap();
    assert_eq!(atomics(&result), big);
    assert_eq!(states(dyn_ctx.general_compare_cache()), vec!["ready"]);
}

/// A comparison evaluated once never builds an index.
#[test]
fn a_comparison_evaluated_once_builds_nothing() {
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let a: Vec<XmlValue> = (0..40i64)
        .map(|i| XmlValue::integer(BigInt::from(i)))
        .collect();
    let compiled = compile(&names, &ctx, "$big = $c", &["big", "c"]);
    let mut dyn_ctx: DynamicContext<'_, Nav> = DynamicContext::new(&ctx, compiled.slots);
    dyn_ctx.set_variable(compiled.vars[0], sequence(&a));
    dyn_ctx.set_variable(compiled.vars[1], sequence(&a));
    assert!(matches!(
        eval_node(&compiled.arena, compiled.root, &mut dyn_ctx),
        Ok(XPathValue::Item(XmlItem::Atomic(_)))
    ));
    assert_eq!(states(dyn_ctx.general_compare_cache()), vec!["counting"]);
}

/// Rebinding the variable an index was built from invalidates the index, so the
/// next answer is computed from the new value.
#[test]
fn rebinding_the_indexed_variable_rebuilds_the_index() {
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let big: Vec<XmlValue> = (0..60i64)
        .map(|i| XmlValue::integer(BigInt::from(i)))
        .collect();
    let first: Vec<XmlValue> = (0..30i64)
        .map(|i| XmlValue::integer(BigInt::from(i * 2)))
        .collect();
    let second: Vec<XmlValue> = (0..30i64)
        .map(|i| XmlValue::integer(BigInt::from(i * 2 + 1)))
        .collect();

    let compiled = compile(&names, &ctx, "$big[. = $c]", &["big", "c"]);
    let mut dyn_ctx: DynamicContext<'_, Nav> = DynamicContext::new(&ctx, compiled.slots);
    dyn_ctx.set_variable(compiled.vars[0], sequence(&big));

    for values in [&first, &second, &first] {
        dyn_ctx.set_variable(compiled.vars[1], sequence(values));
        let result = eval_node(&compiled.arena, compiled.root, &mut dyn_ctx).unwrap();
        assert_eq!(
            atomics(&result),
            expected_filter(&ctx, &big, values, EQ).unwrap(),
            "the index outlived the binding it was built from"
        );
    }
}

/// Two expressions evaluated through one dynamic context: the entries of the
/// first must never be mistaken for the second's. The two below are the same
/// expression but for the variable they compare against, so their comparison
/// nodes carry the **same** node id in their own arenas, and only the arena's
/// identity tells them apart.
#[test]
fn entries_do_not_leak_between_expressions() {
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let big: Vec<XmlValue> = (0..60i64)
        .map(|i| XmlValue::integer(BigInt::from(i)))
        .collect();
    let even: Vec<XmlValue> = (0..30i64)
        .map(|i| XmlValue::integer(BigInt::from(i * 2)))
        .collect();
    let odd: Vec<XmlValue> = (0..30i64)
        .map(|i| XmlValue::integer(BigInt::from(i * 2 + 1)))
        .collect();

    let vars = &["big", "c", "d"];
    let one = compile(&names, &ctx, "$big[. = $c]", vars);
    let two = compile(&names, &ctx, "$big[. = $d]", vars);
    let (_, one_left, one_right) = only_general_comparison(&one.arena, "one");
    let (_, two_left, two_right) = only_general_comparison(&two.arena, "two");
    assert_eq!(
        (one_left, one_right),
        (two_left, two_right),
        "the two arenas should number their nodes identically"
    );

    let mut dyn_ctx: DynamicContext<'_, Nav> = DynamicContext::new(&ctx, one.slots.max(two.slots));
    dyn_ctx.set_variable(one.vars[0], sequence(&big));
    dyn_ctx.set_variable(one.vars[1], sequence(&even));
    dyn_ctx.set_variable(one.vars[2], sequence(&odd));

    // No variable is written from here on, so nothing but the arena's identity
    // can keep the two expressions' indexes apart.
    for _ in 0..3 {
        let first = eval_node(&one.arena, one.root, &mut dyn_ctx).unwrap();
        assert_eq!(
            atomics(&first),
            expected_filter(&ctx, &big, &even, EQ).unwrap(),
            "the first expression got the second's index"
        );
        let second = eval_node(&two.arena, two.root, &mut dyn_ctx).unwrap();
        assert_eq!(
            atomics(&second),
            expected_filter(&ctx, &big, &odd, EQ).unwrap(),
            "the second expression got the first's index"
        );
    }
}

/// `for` and quantified bodies reuse the index across iterations, and get the
/// same answers as the pairwise loop.
#[test]
fn a_for_body_reuses_the_index_across_iterations() {
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let xs: Vec<XmlValue> = (0..50i64)
        .map(|i| XmlValue::integer(BigInt::from(i)))
        .collect();
    let ys: Vec<XmlValue> = (0..30i64)
        .map(|i| XmlValue::integer(BigInt::from(i * 2)))
        .collect();

    let compiled = compile(
        &names,
        &ctx,
        "for $x in $big return ($x = $c)",
        &["big", "c"],
    );
    let mut dyn_ctx: DynamicContext<'_, Nav> = DynamicContext::new(&ctx, compiled.slots);
    dyn_ctx.set_variable(compiled.vars[0], sequence(&xs));
    dyn_ctx.set_variable(compiled.vars[1], sequence(&ys));
    let result = eval_node(&compiled.arena, compiled.root, &mut dyn_ctx).unwrap();

    let expected: Vec<XmlValue> = xs
        .iter()
        .map(|value| {
            XmlValue::boolean(pairwise(&ctx, std::slice::from_ref(value), &ys, true).unwrap())
        })
        .collect();
    assert_eq!(atomics(&result), expected);
    assert_eq!(states(dyn_ctx.general_compare_cache()), vec!["ready"]);

    let compiled = compile(
        &names,
        &ctx,
        "some $x in $big satisfies ($x = $c)",
        &["big", "c"],
    );
    let mut dyn_ctx: DynamicContext<'_, Nav> = DynamicContext::new(&ctx, compiled.slots);
    dyn_ctx.set_variable(compiled.vars[0], sequence(&xs));
    dyn_ctx.set_variable(compiled.vars[1], sequence(&ys));
    let result = eval_node(&compiled.arena, compiled.root, &mut dyn_ctx).unwrap();
    assert_eq!(atomics(&result), vec![XmlValue::boolean(true)]);
}

/// What one index costs, for the record: the atomized values plus one join table.
#[test]
fn the_footprint_of_an_index_is_proportional_to_the_operand() {
    let names = NameTable::new();
    let _ = &names;
    let values: Vec<XmlValue> = (0..1000i64)
        .map(|i| XmlValue::integer(BigInt::from(i)))
        .collect();
    let mut operand = CachedOperand::new(values).unwrap();
    let class = operand.first_of_class[0].0;
    let bucket = match general_compare::pair_kind(class, class) {
        PairKind::Comparable(bucket) => bucket,
        _ => panic!("integers compare with integers"),
    };
    assert!(operand.ensure_table(class, bucket, CollationRef::Codepoint, TimezoneOffset::UTC));
    let per_value = operand.footprint() / 1000;
    println!(
        "index footprint: {} bytes for 1000 xs:integer items, {} per item \
         (XmlValue is {} bytes)",
        operand.footprint(),
        per_value,
        std::mem::size_of::<XmlValue>()
    );
    assert!(per_value < 200, "{per_value} bytes per item is too much");
}

// ============================================================================
// A host function in the other operand writes the variable this one reads
// ============================================================================
//
// The invariance rule constrains only the operand that gets indexed. The other
// one may call anything, and a host function reaches
// [`FunctionEvaluator::eval`] with `&mut DynamicContext`, so it can rebind the
// very variable the invariant operand reads. The cases below pin down what each
// evaluation must then answer, derived from the one rule that decides all of
// them: the untouched path evaluates the **left** operand and then the **right**
// one, so a write the right operand makes is invisible to the left operand of
// the same comparison and visible to every later evaluation.

const MUT_NS: &str = "http://example.com/mutate";

/// The plan `my:mutate()` follows. `FunctionEvaluator::eval` takes `&self` and
/// `register` wants `Send + Sync + 'static`, so the mutable part is atomic and
/// the whole plan travels into the closure in an `Arc`.
struct MutationPlan {
    /// The slot `my:mutate()` reads and rebinds. Binding assigns it, so the test
    /// fills this in after compiling; `NO_SLOT` means "not yet known".
    slot: AtomicU32,
    /// The call, counted from 1, that performs the rebinding. `0` never rebinds.
    at_call: u32,
    /// The integer the rebinding binds.
    binds: i64,
    /// How many copies of the value it read the function returns. `1` is the
    /// ordinary case; more than `MAX_VARYING_ITEMS` makes every probe decline,
    /// which is how the fallback path gets exercised.
    width: usize,
    /// Calls so far.
    calls: AtomicU32,
}

impl MutationPlan {
    fn new(at_call: u32, binds: i64) -> Arc<Self> {
        Arc::new(Self {
            slot: AtomicU32::new(NO_SLOT),
            at_call,
            binds,
            width: 1,
            calls: AtomicU32::new(0),
        })
    }

    fn wide(at_call: u32, binds: i64, width: usize) -> Arc<Self> {
        let plan = Self::new(at_call, binds);
        Arc::new(Self {
            slot: AtomicU32::new(NO_SLOT),
            at_call: plan.at_call,
            binds: plan.binds,
            width,
            calls: AtomicU32::new(0),
        })
    }
}

/// A function set with `my:mutate()`: it returns `width` copies of what the
/// planned slot holds **before** this call, and on call `at_call` it rebinds that
/// slot to `binds`.
fn mutating_functions(plan: Arc<MutationPlan>) -> FunctionSet<Nav> {
    let mut functions: FunctionSet<Nav> = FunctionSet::with_builtins();
    functions.register(
        DynamicFunctionSignature::new(MUT_NS, "mutate", vec![], SequenceType::any_atomic_star()),
        move |ctx, _args| {
            let call = plan.calls.fetch_add(1, Ordering::Relaxed) + 1;
            let slot = plan.slot.load(Ordering::Relaxed);
            let seen: Vec<XmlItem<Nav>> = ctx
                .get_variable(slot)
                .map(|value| value.clone().into_vec())
                .unwrap_or_default();
            if call == plan.at_call {
                ctx.set_variable(slot, XPathValue::integer(plan.binds));
            }
            let mut out = Vec::with_capacity(seen.len() * plan.width);
            for _ in 0..plan.width {
                out.extend(seen.iter().cloned());
            }
            Ok(XPathValue::from_sequence(out))
        },
    );
    functions
}

/// The result of one run of a mutating expression: the filtered items and how
/// many evaluations the index answered.
struct MutRun {
    kept: Vec<i64>,
    answered: u32,
    states: Vec<&'static str>,
}

/// Evaluate `source` with `$c` bound to the single integer `start` and
/// `my:mutate()` available, through `eval_node` — i.e. with the production
/// thresholds and the lazy build.
fn run_mutating(source: &str, start: i64, plan: &Arc<MutationPlan>) -> MutRun {
    let names = NameTable::new();
    let functions = mutating_functions(Arc::clone(plan));
    let mut namespaces = NamespaceContextSnapshot::default();
    namespaces
        .bindings
        .push((names.add("my"), names.add(MUT_NS)));
    let ctx = XPathContext::new(&names)
        .with_namespaces(namespaces)
        .with_function_catalog(&functions);

    let compiled = compile(&names, &ctx, source, &["c"]);
    plan.slot.store(compiled.vars[0], Ordering::Relaxed);

    let mut dyn_ctx: DynamicContext<'_, Nav> =
        DynamicContext::new(&ctx, compiled.slots).with_function_evaluator(&functions);
    dyn_ctx.set_variable(compiled.vars[0], XPathValue::integer(start));
    let result = eval_node(&compiled.arena, compiled.root, &mut dyn_ctx)
        .unwrap_or_else(|e| panic!("{source} raised {e:?}"));
    MutRun {
        kept: atomics(&result)
            .iter()
            .map(|value| {
                value
                    .clone()
                    .as_integer()
                    .unwrap_or_else(|| panic!("integer expected, got {value:?}"))
                    .to_string()
                    .parse()
                    .unwrap()
            })
            .collect(),
        answered: dyn_ctx.general_compare_cache().answered(),
        states: states(dyn_ctx.general_compare_cache()),
    }
}

/// The review's own reproduction. `my:mutate()` sits in the **right** operand and
/// rebinds `$c` on its second call, i.e. while the evaluation that would install
/// the index is running.
///
/// Source order gives, with `$c` starting at `0` and `my:mutate()` returning what
/// `$c` held when it was entered:
///
/// | item | `$c` read | `my:mutate()` | writes | `=` |
/// |---|---|---|---|---|
/// | 1 | 0 | 0 | — | true |
/// | 2 | 0 | 0 | `$c` := 1 | true |
/// | 3 | 1 | 1 | — | true |
/// | 4 | 1 | 1 | — | true |
///
/// so the filter keeps all four. Pairing the left operand's value — read before
/// the write — with the generations that hold after it would leave an index that
/// answers `0 = 1` for items 3 and 4, and the filter would keep `(1, 2)`.
#[test]
fn a_write_by_the_other_operand_stops_the_index_being_installed() {
    let plan = MutationPlan::new(2, 1);
    let run = run_mutating("(1 to 4)[$c = my:mutate()]", 0, &plan);
    assert_eq!(run.kept, vec![1, 2, 3, 4]);
    // The left operand's fingerprint no longer described its value, and the
    // right operand calls a function, so neither side could be indexed.
    assert_eq!(run.states, vec!["off"]);
    assert_eq!(run.answered, 0);
}

/// The same write, with the operands the other way round: `my:mutate()` is the
/// **left** operand, so its write happens before the invariant right operand of
/// the same comparison is evaluated and is visible to it.
///
/// | item | `my:mutate()` | writes | `$c` read | `=` |
/// |---|---|---|---|---|
/// | 1 | 0 | — | 0 | true |
/// | 2 | 0 | `$c` := 1 | 1 | false |
/// | 3 | 1 | — | 1 | true |
/// | 4 | 1 | — | 1 | true |
#[test]
fn a_write_before_the_invariant_operand_is_visible_to_it() {
    let plan = MutationPlan::new(2, 1);
    let run = run_mutating("(1 to 4)[my:mutate() = $c]", 0, &plan);
    assert_eq!(run.kept, vec![1, 3, 4]);
}

/// The write lands on the **third** call, so an index already exists and is
/// probed. The invariant operand is on the left, i.e. source order evaluates it
/// before the write: evaluation 3 must still compare the value from before.
///
/// | item | `$c` read | `my:mutate()` | writes | `=` |
/// |---|---|---|---|---|
/// | 1 | 0 | 0 | — | true |
/// | 2 | 0 | 0 | — | true |
/// | 3 | 0 | 0 | `$c` := 1 | true |
/// | 4 | 1 | 1 | — | true |
#[test]
fn a_write_during_a_probe_does_not_backdate_the_left_operand() {
    let plan = MutationPlan::new(3, 1);
    let run = run_mutating("(1 to 4)[$c = my:mutate()]", 0, &plan);
    assert_eq!(run.kept, vec![1, 2, 3, 4]);
    // Evaluation 3 was answered from the index — the write does not invalidate
    // the answer, only the index — and evaluation 4 reconsidered the node.
    assert_eq!(run.answered, 1);
}

/// The same write, with the invariant operand on the **right**: source order
/// evaluates it after `my:mutate()`, so evaluation 3 must see the new binding and
/// the index must not answer.
///
/// | item | `my:mutate()` | writes | `$c` read | `=` |
/// |---|---|---|---|---|
/// | 1 | 0 | — | 0 | true |
/// | 2 | 0 | — | 0 | true |
/// | 3 | 0 | `$c` := 1 | 1 | false |
/// | 4 | 1 | — | 1 | true |
#[test]
fn a_write_during_a_probe_is_visible_to_a_right_hand_invariant_operand() {
    let plan = MutationPlan::new(3, 1);
    let run = run_mutating("(1 to 4)[my:mutate() = $c]", 0, &plan);
    assert_eq!(run.kept, vec![1, 2, 4]);
}

/// A write that binds the value the slot already held: the generation moves and
/// the backing storage is a fresh allocation, so the identity check sees a
/// change where the value has none. Rebuilding rather than trusting it costs
/// speed, never the answer.
#[test]
fn a_write_that_restores_the_same_value_is_still_answered_correctly() {
    let plan = MutationPlan::new(2, 0);
    let run = run_mutating("(1 to 4)[$c = my:mutate()]", 0, &plan);
    assert_eq!(run.kept, vec![1, 2, 3, 4]);

    // And the same restoring write while an index exists.
    let plan = MutationPlan::new(3, 0);
    let run = run_mutating("(1 to 4)[$c = my:mutate()]", 0, &plan);
    assert_eq!(run.kept, vec![1, 2, 3, 4]);
}

/// The write happens while an index exists **and** the probe declines, because
/// the varying operand is wider than [`MAX_VARYING_ITEMS`]. The invariant
/// operand still may not be re-evaluated: source order put it before the write,
/// so the comparison runs against the value the index holds.
#[test]
fn a_declined_probe_does_not_re_evaluate_a_left_operand_after_a_write() {
    let plan = MutationPlan::wide(3, 1, MAX_VARYING_ITEMS + 1);
    let run = run_mutating("(1 to 4)[$c = my:mutate()]", 0, &plan);
    // Every evaluation compares `$c` as source order gives it against a wide
    // sequence of copies of the same value, so every item is kept — item 3
    // included, whose `$c` is the `0` from before the write.
    assert_eq!(run.kept, vec![1, 2, 3, 4]);
    // The varying operand is too wide for any probe to answer.
    assert_eq!(run.answered, 0);
}

/// With no write at all the cache must still engage — the fix must not have
/// turned the optimization off.
#[test]
fn without_a_write_the_index_still_answers() {
    let plan = MutationPlan::new(0, 0);
    let run = run_mutating("(1 to 4)[$c = my:mutate()]", 0, &plan);
    assert_eq!(run.kept, vec![1, 2, 3, 4]);
    assert_eq!(run.states, vec!["ready"]);
    // Evaluation 1 ran the untouched path, evaluation 2 installed the index, and
    // evaluations 3 and 4 were answered from it.
    assert_eq!(run.answered, 2);
    assert_eq!(plan.calls.load(Ordering::Relaxed), 4);
}

// ============================================================================
// Identity of the arena and of the variable store
// ============================================================================

/// A host function `my:eval($source)` that compiles `$source` and evaluates it on
/// the caller's own `DynamicContext` — the natural way to write a dynamic
/// evaluation extension. Every call stores its freshly bound arena in **the same
/// heap slot**, so two calls evaluate two different arenas at one address: the
/// situation in which an address is not an identity. Returns the function set and
/// the addresses the arenas were evaluated at.
fn evaluating_functions() -> (FunctionSet<Nav>, Arc<std::sync::Mutex<Vec<usize>>>) {
    use crate::xpath::functions::signature::types::{any, string};
    let held: Arc<std::sync::Mutex<Box<(AstArena, AstNodeId)>>> =
        Arc::new(std::sync::Mutex::new(Box::new((AstArena::new(), 0))));
    let addresses = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = Arc::clone(&addresses);
    let mut functions: FunctionSet<Nav> = FunctionSet::with_builtins();
    functions.register(
        DynamicFunctionSignature::new(MUT_NS, "eval", vec![string()], any()),
        move |ctx, args| {
            let source = args[0]
                .first()
                .and_then(|item| item.as_atomic())
                .map(|value| value.to_string_value())
                .expect("a string argument");
            let mut parsed = parse(&source).expect("the inner expression parses");
            let mut binder = NameBinder::new();
            bind_node(
                &mut parsed.arena,
                parsed.root,
                ctx.static_context,
                &mut binder,
            )?;
            let mut slot = held.lock().unwrap();
            // Overwrite in place: the old arena is dropped and the new one lands
            // at the very address the old one had.
            **slot = (parsed.arena, parsed.root);
            seen.lock()
                .unwrap()
                .push(&slot.0 as *const AstArena as usize);
            let (arena, root) = &**slot;
            eval_node(arena, *root, ctx)
        },
    );
    (functions, addresses)
}

/// Review finding X-09, the reviewer's own reproduction: the second inner
/// expression must not be answered from the first one's index although both
/// arenas sit at the same address and number their nodes identically. (The
/// literal operand reads no variable, so no fingerprint can tell them apart —
/// only the arena's identity can.)
#[test]
fn an_arena_at_a_recycled_address_is_not_answered_from_the_old_index() {
    let names = NameTable::new();
    let (functions, addresses) = evaluating_functions();
    let mut namespaces = NamespaceContextSnapshot::default();
    namespaces
        .bindings
        .push((names.add("my"), names.add(MUT_NS)));
    let ctx = XPathContext::new(&names)
        .with_namespaces(namespaces)
        .with_function_catalog(&functions);
    let compiled = compile(
        &names,
        &ctx,
        "(my:eval('for $i in 1 to 3 return $i = (1,2,3)'), \
          my:eval('for $i in 1 to 3 return $i = (7,8,9)'))",
        &[],
    );
    // The inner expressions number their `$i` from slot 0 of a binder of their
    // own; give the shared store room for it.
    let mut dyn_ctx: DynamicContext<'_, Nav> =
        DynamicContext::new(&ctx, compiled.slots.max(4)).with_function_evaluator(&functions);
    let result = eval_node(&compiled.arena, compiled.root, &mut dyn_ctx).unwrap();
    let addresses = addresses.lock().unwrap().clone();
    assert_eq!(addresses.len(), 2);
    assert_eq!(
        addresses[0], addresses[1],
        "the test needs both inner arenas at one address"
    );
    assert_eq!(
        atomics(&result),
        [true, true, true, false, false, false]
            .into_iter()
            .map(XmlValue::boolean)
            .collect::<Vec<_>>()
    );
}

/// Review finding X-09, variable side: `DynamicContext::variables` is public, so
/// a host can replace the whole store between two evaluations. Generations count
/// writes within one store, and a single-item binding records no address, so the
/// new store's `$c` — written once, like the old one's — would look unchanged.
#[test]
fn replacing_the_variable_store_rebuilds_the_index() {
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let big: Vec<XmlValue> = (0..20i64)
        .map(|i| XmlValue::integer(BigInt::from(i)))
        .collect();
    let compiled = compile(&names, &ctx, "$big[. = $c]", &["big", "c"]);
    let mut dyn_ctx: DynamicContext<'_, Nav> = DynamicContext::new(&ctx, compiled.slots);
    dyn_ctx.set_variable(compiled.vars[0], sequence(&big));
    dyn_ctx.set_variable(compiled.vars[1], XPathValue::integer(5));
    let first = eval_node(&compiled.arena, compiled.root, &mut dyn_ctx).unwrap();
    assert_eq!(atomics(&first), vec![XmlValue::integer(BigInt::from(5))]);
    assert_eq!(states(dyn_ctx.general_compare_cache()), vec!["ready"]);

    // A fresh store, written exactly as often as the old one was.
    dyn_ctx.variables = VarStore::new(compiled.slots);
    dyn_ctx.set_variable(compiled.vars[0], sequence(&big));
    dyn_ctx.set_variable(compiled.vars[1], XPathValue::integer(7));
    let second = eval_node(&compiled.arena, compiled.root, &mut dyn_ctx).unwrap();
    assert_eq!(atomics(&second), vec![XmlValue::integer(BigInt::from(7))]);

    // A clone of the store, rewritten: the same generations, other values.
    let mut copy = dyn_ctx.variables.clone();
    copy.set(compiled.vars[1], XPathValue::integer(9));
    let mut fresh = VarStore::new(compiled.slots);
    fresh.set(compiled.vars[0], sequence(&big));
    fresh.set(compiled.vars[1], XPathValue::integer(9));
    dyn_ctx.variables = fresh;
    let third = eval_node(&compiled.arena, compiled.root, &mut dyn_ctx).unwrap();
    assert_eq!(atomics(&third), vec![XmlValue::integer(BigInt::from(9))]);
    dyn_ctx.variables = copy;
    let fourth = eval_node(&compiled.arena, compiled.root, &mut dyn_ctx).unwrap();
    assert_eq!(atomics(&fourth), vec![XmlValue::integer(BigInt::from(9))]);
}

/// An arena gets a new serial whenever it can have changed, and a clone is a
/// different arena.
#[test]
fn an_arena_serial_changes_with_every_mutation_and_clone() {
    let mut arena = AstArena::new();
    let created = arena.serial();
    assert_ne!(created, 0, "0 stands for no arena");
    let id = arena.add(AstNode::Value(crate::xpath::ast::ValueNode::Empty));
    let added = arena.serial();
    assert_ne!(added, created);
    let clone = arena.clone();
    assert_ne!(clone.serial(), added);
    let _ = arena.get_mut(id);
    assert_ne!(arena.serial(), added);
    let before_clear = arena.serial();
    arena.clear();
    assert_ne!(arena.serial(), before_clear);
    assert_ne!(AstArena::new().serial(), AstArena::new().serial());
}
