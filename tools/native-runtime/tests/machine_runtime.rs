mod support;
use einvoice_native_runtime::machine::{EntryFocus, ItemRef, Prepared, Run, SourceNode};
use einvoice_native_runtime::{Error, Failure, Fault, Resource};
use einvoice_source::{NodeKind, SourceDocument};
use support::*;
use validation_ir::{Binding, BindingValue, Expr, SequenceType};

fn declaration(id: &str, tree: Expr, global: bool, ty: Option<(&str, Option<&str>)>) -> Binding {
    Binding {
        id: id.into(),
        name: name("", id),
        declared_owner: "independent probe".into(),
        effective_scope: if global {
            validation_ir::Scope::Xslt2SchemaGlobal
        } else {
            validation_ir::Scope::RuleLocal
        },
        initialization_context: "lexical-dynamic-context".into(),
        source: location(),
        sequence_type: ty.map(|(n, o)| SequenceType {
            name: name(XS, n),
            occurrence: o.map(str::to_owned),
        }),
        value: BindingValue::Select {
            expression: expression(tree),
        },
    }
}
fn values(run: &mut Run<'_, '_, '_>) -> einvoice_native_runtime::Result<Vec<String>> {
    let c = run.cursor()?;
    let mut v = vec![];
    while let Some(i) = run.next(c)? {
        v.push(run.render(i)?.as_str().into());
    }
    Ok(v)
}
fn doc() -> std::sync::Arc<SourceDocument> {
    SourceDocument::parse(include_bytes!("evaluator-fixtures/document.xml")).unwrap()
}
#[test]
fn globals_capture_root_and_tuples_preserve_entry_focus() {
    let d = doc();
    let a = d
        .root_element()
        .children()
        .find(|n| n.name().is_some_and(|n| n.local() == "a"))
        .unwrap();
    let bindings = vec![declaration("g", Expr::Context, true, None)];
    let e = expression(seq(vec![
        call("count", vec![bin("/", var("g"), step("child", "r"))]),
        bind(
            "for",
            vec![("x", seq(vec![n("1"), n("2")]))],
            seq(vec![
                call("position", vec![]),
                call("last", vec![]),
                bin("is", var("g"), Expr::Root),
            ]),
        ),
    ]));
    let p = Prepared::new(&e, &bindings, machine_limits()).unwrap();
    assert!(std::ptr::eq(p.expression(), &e));
    let mut r = Run::new(
        &p,
        &d,
        EntryFocus::Node(SourceNode::Node(a)),
        atomic_limits(),
        machine_limits(),
        1_000_000,
        1_000_000,
    )
    .unwrap();
    assert_eq!(
        values(&mut r).unwrap(),
        ["1", "1", "1", "true", "1", "1", "true"]
    );
}
#[test]
fn local_select_initializer_keeps_its_captured_focus_when_first_demanded_in_predicate() {
    let d = doc();
    let bindings = vec![declaration("v", call("position", vec![]), false, None)];
    let e = expression(filter(
        seq(vec![n("10"), n("20")]),
        cond(
            bin("=", call("position", vec![]), n("2")),
            bin("=", var("v"), n("1")),
            call("false", vec![]),
        ),
    ));
    let p = Prepared::new(&e, &bindings, machine_limits()).unwrap();
    let mut r = Run::new(
        &p,
        &d,
        EntryFocus::Document,
        atomic_limits(),
        machine_limits(),
        1_000_000,
        1_000_000,
    )
    .unwrap();
    assert_eq!(values(&mut r).unwrap(), ["20"]);
}
#[test]
fn lazy_selects_and_typed_stars_have_replayable_prefixes() {
    for ty in [None, Some(("integer", Some("*")))] {
        let bindings = vec![
            declaration("g", seq(vec![n("1"), bad()]), true, ty),
            declaration("unused", bad(), false, None),
        ];
        let e = expression(seq(vec![
            call("exists", vec![var("g")]),
            filter(var("g"), n("1")),
            filter(var("g"), n("1")),
        ]));
        let p = Prepared::new(&e, &bindings, machine_limits()).unwrap();
        let d = doc();
        let mut r = Run::new(
            &p,
            &d,
            EntryFocus::Document,
            atomic_limits(),
            machine_limits(),
            1_000_000,
            1_000_000,
        )
        .unwrap();
        assert_eq!(values(&mut r).unwrap(), ["true", "1", "1"]);
        let e = expression(call("count", vec![var("g")]));
        let p = Prepared::new(&e, &bindings, machine_limits()).unwrap();
        let mut r = Run::new(
            &p,
            &d,
            EntryFocus::Document,
            atomic_limits(),
            machine_limits(),
            1_000_000,
            1_000_000,
        )
        .unwrap();
        assert_eq!(values(&mut r), Err(Error::XPath(Fault::Forg0001)));
    }
    let bindings = vec![declaration("g", seq(vec![n("1"), n("2")]), true, None)];
    let e = expression(seq(vec![
        call("sum", vec![var("g")]),
        call("sum", vec![var("g")]),
    ]));
    let p = Prepared::new(&e, &bindings, machine_limits()).unwrap();
    let d = doc();
    let mut r = Run::new(
        &p,
        &d,
        EntryFocus::Document,
        atomic_limits(),
        machine_limits(),
        1_000_000,
        1_000_000,
    )
    .unwrap();
    assert_eq!(values(&mut r).unwrap(), ["3", "3"]);
}
#[test]
fn declared_function_conversion_distinguishes_casting_subtyping_and_cardinality() {
    let d = doc();
    for (tree, ty, expected) in [
        (
            path(&["r", "d"]),
            ("decimal", Some("*")),
            Ok(vec!["1.25", "2.5"]),
        ),
        (n("1"), ("decimal", None), Ok(vec!["1"])),
        (
            s("1"),
            ("integer", None),
            Err(Error::XPath(Fault::Xtte0570)),
        ),
        (
            seq(vec![n("1"), n("2")]),
            ("integer", None),
            Err(Error::XPath(Fault::Xtte0570)),
        ),
        (
            seq(vec![]),
            ("integer", Some("+")),
            Err(Error::XPath(Fault::Xtte0570)),
        ),
        (seq(vec![]), ("string", Some("?")), Ok(vec![])),
    ] {
        let b = vec![declaration("v", tree, true, Some(ty))];
        let e = expression(var("v"));
        let p = Prepared::new(&e, &b, machine_limits()).unwrap();
        let mut r = Run::new(
            &p,
            &d,
            EntryFocus::Document,
            atomic_limits(),
            machine_limits(),
            1_000_000,
            1_000_000,
        )
        .unwrap();
        assert_eq!(
            values(&mut r),
            expected.map(|v| v.into_iter().map(str::to_owned).collect())
        );
    }
}
#[test]
fn typed_select_operand_errors_and_fatals_do_not_become_castable_false() {
    let d = doc();
    let b = vec![declaration("v", bad(), true, Some(("integer", None)))];
    let e = expression(typeop("castable", var("v"), "string", None));
    let p = Prepared::new(&e, &b, machine_limits()).unwrap();
    let mut r = Run::new(
        &p,
        &d,
        EntryFocus::Document,
        atomic_limits(),
        machine_limits(),
        1_000_000,
        1_000_000,
    )
    .unwrap();
    assert_eq!(values(&mut r), Err(Error::XPath(Fault::Forg0001)));
    let e = expression(typeop("castable", path(&["r", "n"]), "integer", None));
    let p = Prepared::new(&e, &[], machine_limits()).unwrap();
    let mut r = Run::new(
        &p,
        &d,
        EntryFocus::Document,
        atomic_limits(),
        machine_limits(),
        1,
        1,
    )
    .unwrap();
    let c = r.cursor().unwrap();
    let error = r.next(c).unwrap_err();
    assert!(matches!(
        error,
        Error::Fatal(Failure::Resource(Resource::SourceQuery))
    ));
    assert_eq!(r.next(c), Err(error));
}
#[test]
fn original_attributes_namespace_nodes_text_and_qnames_keep_identity() {
    let d = doc();
    let element = bin(
        "/",
        path(&["r"]),
        Expr::Step {
            axis: "child".into(),
            test: validation_ir::NodeTest::Name {
                name: name("urn:p", "e"),
            },
        },
    );
    let attr = bin(
        "/",
        element,
        Expr::Step {
            axis: "attribute".into(),
            test: validation_ir::NodeTest::Name {
                name: name("urn:p", "x"),
            },
        },
    );
    let e = expression(attr);
    let p = Prepared::new(&e, &[], machine_limits()).unwrap();
    let mut r = Run::new(
        &p,
        &d,
        EntryFocus::Document,
        atomic_limits(),
        machine_limits(),
        1_000_000,
        1_000_000,
    )
    .unwrap();
    let c = r.cursor().unwrap();
    let i = r.next(c).unwrap().unwrap();
    let ItemRef::Node(SourceNode::Attribute(a)) = r.item_ref(i).unwrap() else {
        panic!("attribute identity")
    };
    assert_eq!(a.name().prefix(), Some("p"));
    assert_eq!(a.value(), "v");
    assert_ne!(a.identity(), a.owner().identity());
    let original = a.identity();
    let c2 = r.cursor().unwrap();
    let j = r.next(c2).unwrap().unwrap();
    let ItemRef::Node(n) = r.item_ref(j).unwrap() else {
        panic!()
    };
    assert_eq!(n.identity(), original);
    assert_eq!(r.render(i).unwrap().as_str(), "v");
    let e = expression(step("namespace", "*"));
    let p = Prepared::new(&e, &[], machine_limits()).unwrap();
    let mut r = Run::new(
        &p,
        &d,
        EntryFocus::Node(SourceNode::Node(d.root_element())),
        atomic_limits(),
        machine_limits(),
        1_000_000,
        1_000_000,
    )
    .unwrap();
    let c = r.cursor().unwrap();
    let mut found = vec![];
    while let Some(i) = r.next(c).unwrap() {
        let ItemRef::Node(SourceNode::Namespace(n)) = r.item_ref(i).unwrap() else {
            panic!()
        };
        found.push((n.prefix(), n.uri(), n.identity()));
        assert_ne!(n.identity(), n.owner().identity());
    }
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].0, "p");
    assert_ne!(found[0].2, found[1].2);
    assert!(d.original_bytes().starts_with(b"<r xmlns:p="));
}
#[test]
fn foreign_document_and_run_handles_fail_and_arc_aliases_preserve_identity() {
    let d = doc();
    let other = doc();
    let e = expression(Expr::Context);
    let p = Prepared::new(&e, &[], machine_limits()).unwrap();
    assert!(matches!(
        Run::new(
            &p,
            &d,
            EntryFocus::Node(SourceNode::Node(other.document())),
            atomic_limits(),
            machine_limits(),
            1000,
            1000
        ),
        Err(Error::Fatal(Failure::ForeignScope))
    ));
    let alias = d.clone();
    let mut a = Run::new(
        &p,
        &d,
        EntryFocus::Node(SourceNode::Node(alias.document())),
        atomic_limits(),
        machine_limits(),
        1000,
        1000,
    )
    .unwrap();
    let ca = a.cursor().unwrap();
    let item = a.next(ca).unwrap().unwrap();
    let mut b = Run::new(
        &p,
        &other,
        EntryFocus::Document,
        atomic_limits(),
        machine_limits(),
        1000,
        1000,
    )
    .unwrap();
    assert_eq!(b.next(ca), Err(Error::Fatal(Failure::ForeignScope)));
    assert!(matches!(
        b.item_ref(item),
        Err(Error::Fatal(Failure::ForeignScope))
    ));
}
#[test]
fn absent_focus_root_and_axis_faults_are_distinct() {
    let d = doc();
    for tree in [
        Expr::Context,
        Expr::Root,
        call("position", vec![]),
        call("last", vec![]),
        call("string", vec![]),
    ] {
        let e = expression(tree);
        let p = Prepared::new(&e, &[], machine_limits()).unwrap();
        let mut r = Run::new(
            &p,
            &d,
            EntryFocus::Absent,
            atomic_limits(),
            machine_limits(),
            1000,
            1000,
        )
        .unwrap();
        assert_eq!(values(&mut r), Err(Error::XPath(Fault::Xpdy0002)));
    }
    assert_eq!(
        evaluate(bin("/", n("1"), step("child", "*"))),
        Err(Error::XPath(Fault::Xpty0019))
    );
    assert_eq!(
        evaluate(bin("/", path(&["r"]), bin("/", n("1"), Expr::Context))),
        Err(Error::XPath(Fault::Xpty0019))
    );
}
#[test]
fn cached_prefix_and_error_terminal_replay_with_original_failure_identity() {
    let e = expression(seq(vec![n("7"), bad()]));
    let p = Prepared::new(&e, &[], machine_limits()).unwrap();
    let d = doc();
    let mut r = Run::new(
        &p,
        &d,
        EntryFocus::Document,
        atomic_limits(),
        machine_limits(),
        1_000_000,
        1_000_000,
    )
    .unwrap();
    let c = r.cursor().unwrap();
    assert!(r.next(c).unwrap().is_some());
    assert_eq!(r.next(c), Err(Error::XPath(Fault::Forg0001)));
    let construct = r.error_origin().unwrap().construct as *const Expr;
    assert!(std::ptr::eq(r.error_origin().unwrap().expression, &e));
    let c2 = r.cursor().unwrap();
    assert!(r.next(c2).unwrap().is_some());
    assert_eq!(r.next(c2), Err(Error::XPath(Fault::Forg0001)));
    assert_eq!(
        r.error_origin().unwrap().construct as *const Expr,
        construct
    );
}
#[test]
fn numeric_scope_and_source_budget_are_persistent_and_new_runs_reset() {
    let e = expression(seq(vec![
        bin("*", n("12"), n("13")),
        bin("*", n("14"), n("15")),
    ]));
    let p = Prepared::new(&e, &[], machine_limits()).unwrap();
    let d = doc();
    let mut r = Run::new(
        &p,
        &d,
        EntryFocus::Document,
        atomic_limits(),
        machine_limits(),
        1_000_000,
        1_000_000,
    )
    .unwrap();
    let c = r.cursor().unwrap();
    let before = r.atomic_usage().numeric_work_remaining;
    r.next(c).unwrap();
    let mid = r.atomic_usage().numeric_work_remaining;
    r.next(c).unwrap();
    let after = r.atomic_usage().numeric_work_remaining;
    assert!(before > mid && mid > after);
    let mut fresh = Run::new(
        &p,
        &d,
        EntryFocus::Document,
        atomic_limits(),
        machine_limits(),
        1_000_000,
        1_000_000,
    )
    .unwrap();
    assert_eq!(fresh.atomic_usage().numeric_work_remaining, before);
    assert_eq!(values(&mut fresh).unwrap(), ["156", "210"]);
    let e = expression(seq(vec![
        call("data", vec![path(&["r", "bad"])]),
        call("data", vec![path(&["r", "bad"])]),
    ]));
    let p = Prepared::new(&e, &[], machine_limits()).unwrap();
    let mut r = Run::new(
        &p,
        &d,
        EntryFocus::Document,
        atomic_limits(),
        machine_limits(),
        1_000_000,
        4,
    )
    .unwrap();
    let c = r.cursor().unwrap();
    assert!(r.next(c).unwrap().is_some());
    assert_eq!(
        r.next(c),
        Err(Error::Fatal(Failure::Resource(Resource::SourceQuery)))
    );
}
#[test]
fn circular_globals_are_xpath_faults_without_rc_cycles() {
    let b = vec![
        declaration("a", var("b"), true, None),
        declaration("b", var("a"), true, None),
    ];
    let e = expression(var("a"));
    let p = Prepared::new(&e, &b, machine_limits()).unwrap();
    let d = doc();
    let mut r = Run::new(
        &p,
        &d,
        EntryFocus::Document,
        atomic_limits(),
        machine_limits(),
        1000,
        1000,
    )
    .unwrap();
    assert_eq!(values(&mut r), Err(Error::XPath(Fault::Xtde0640)));
}
#[test]
fn capability_admission_rejects_unused_dead_branches_unknown_bindings_and_types() {
    for tree in [
        cond(
            call("true", vec![]),
            n("1"),
            call("matches", vec![s("a"), s("a")]),
        ),
        call("count", vec![]),
        bin("except", seq(vec![]), seq(vec![])),
        typeop("cast", n("1"), "int", None),
        var("missing"),
        Expr::Call {
            name: name("urn:helper", "body"),
            arguments: vec![],
        },
    ] {
        let e = expression(tree);
        assert!(matches!(
            Prepared::new(&e, &[], machine_limits()),
            Err(Error::Fatal(Failure::Capability(_)))
        ));
    }
    let b = vec![declaration(
        "unused",
        call("upper-case", vec![s("a")]),
        true,
        None,
    )];
    let e = expression(n("1"));
    assert!(matches!(
        Prepared::new(&e, &b, machine_limits()),
        Err(Error::Fatal(Failure::Capability(_)))
    ));
    // A global initializer cannot capture a rule-local declaration.
    let b = vec![
        declaration("g", var("local"), true, None),
        declaration("local", n("1"), false, None),
    ];
    assert!(matches!(
        Prepared::new(&e, &b, machine_limits()),
        Err(Error::Fatal(Failure::Capability(_)))
    ));
}
#[test]
fn huge_ranges_grow_only_on_demand_then_refuse_without_truncation() {
    let e = expression(bin(
        "to",
        n("1"),
        n("9999999999999999999999999999999999999999"),
    ));
    let p = Prepared::new(&e, &[], machine_limits()).unwrap();
    let d = doc();
    let mut limits = machine_limits();
    limits.objects = 128;
    let mut r = Run::new(
        &p,
        &d,
        EntryFocus::Document,
        atomic_limits(),
        limits,
        1000,
        1000,
    )
    .unwrap();
    let c = r.cursor().unwrap();
    assert!(r.next(c).unwrap().is_some());
    assert!(r.usage().objects < 30);
    let mut produced = 1;
    let error = loop {
        match r.next(c) {
            Ok(Some(_)) => produced += 1,
            Ok(None) => panic!("truncated range"),
            Err(e) => break e,
        }
    };
    assert!(produced > 1 && produced < 128);
    assert!(matches!(error, Error::Fatal(Failure::Resource(_))));
    assert_eq!(r.next(c), Err(error));
    assert_eq!(
        evaluate(bin("to", n("3"), n("1"))).unwrap(),
        Vec::<String>::new()
    );
    assert_eq!(
        evaluate(bin("to", decimal("1.0"), n("3"))),
        Err(Error::XPath(Fault::Xpty0004))
    );
}
#[test]
fn independent_replay_uses_cache_but_repeated_pulls_still_spend_fuel() {
    let e = expression(seq(vec![n("1"), n("2")]));
    let p = Prepared::new(&e, &[], machine_limits()).unwrap();
    let d = doc();
    let mut limits = machine_limits();
    limits.work = 200;
    let mut r = Run::new(
        &p,
        &d,
        EntryFocus::Document,
        atomic_limits(),
        limits,
        1000,
        1000,
    )
    .unwrap();
    let a = r.cursor().unwrap();
    let b = r.cursor().unwrap();
    let x = r.next(a).unwrap().unwrap();
    let numeric = r.atomic_usage().numeric_work_remaining;
    let fuel = r.usage().work_remaining;
    let y = r.next(b).unwrap().unwrap();
    assert_eq!(r.atomic_usage().numeric_work_remaining, numeric);
    assert!(r.usage().work_remaining < fuel);
    let (ItemRef::Atomic(x), ItemRef::Atomic(y)) = (r.item_ref(x).unwrap(), r.item_ref(y).unwrap())
    else {
        panic!()
    };
    assert!(std::ptr::eq(x, y));
    while r.next(a).is_ok() {}
    assert!(matches!(r.next(a), Err(Error::Fatal(Failure::Resource(_)))));
}
#[test]
fn deep_preparation_execution_and_drop_use_heap_continuations() {
    let mut tree = n("7");
    for _ in 0..20_000 {
        tree = group(tree);
    }
    let e = expression(tree);
    let p = Prepared::new(&e, &[], machine_limits()).unwrap();
    let d = doc();
    let mut r = Run::new(
        &p,
        &d,
        EntryFocus::Document,
        atomic_limits(),
        machine_limits(),
        1000,
        1000,
    )
    .unwrap();
    assert_eq!(values(&mut r).unwrap(), ["7"]);
    drop(r);
    drop(p);
    // Caller owns the hostile AST. Dismantle it iteratively too.
    let mut tree = e.tree;
    while let Expr::Group { operand } = tree {
        tree = *operand;
    }
}
#[test]
fn preparation_admission_refuses_strings_depth_and_storage_before_growth() {
    let e = expression(s(&"x".repeat(1000)));
    let mut l = machine_limits();
    l.work = 30;
    assert!(matches!(
        Prepared::new(&e, &[], l),
        Err(Error::Fatal(Failure::Resource(_)))
    ));
    let mut tree = n("1");
    for _ in 0..100 {
        tree = group(tree);
    }
    let e = expression(tree);
    let mut l = machine_limits();
    l.frames = 10;
    assert!(matches!(
        Prepared::new(&e, &[], l),
        Err(Error::Fatal(Failure::Resource(_)))
    ));
    let mut tree = e.tree;
    while let Expr::Group { operand } = tree {
        tree = *operand;
    }
    let e = expression(seq(vec![n("1"), n("2")]));
    let mut l = machine_limits();
    l.live_bytes = 1;
    assert!(matches!(
        Prepared::new(&e, &[], l),
        Err(Error::Fatal(Failure::Resource(_)))
    ));
}
#[test]
fn good_error_good_runs_do_not_share_bindings_focus_or_failures() {
    let e = expression(cast("integer", path(&["r", "bad"])));
    let p = Prepared::new(&e, &[], machine_limits()).unwrap();
    for (xml, expected) in [
        ("<r><bad>7</bad></r>", Ok(vec!["7".into()])),
        ("<r><bad>bad</bad></r>", Err(Error::XPath(Fault::Forg0001))),
        ("<r><bad>9</bad></r>", Ok(vec!["9".into()])),
    ] {
        let d = SourceDocument::parse(xml.as_bytes()).unwrap();
        let mut r = Run::new(
            &p,
            &d,
            EntryFocus::Document,
            atomic_limits(),
            machine_limits(),
            1000,
            1000,
        )
        .unwrap();
        assert_eq!(values(&mut r), expected);
    }
    assert_eq!(doc().document().kind(), NodeKind::Document);
}

#[test]
fn selected_binding_asts_match_fresh_pinned_xslt_observations() {
    let fixture = include_str!("evaluator-fixtures/observations.tsv");
    let ids = include_str!("evaluator-fixtures/select-bindings.tsv");
    for line in ids.lines() {
        let id = line.split('\t').next().unwrap();
        let initializer = if id.contains("sequence") {
            seq(vec![n("1"), bad()])
        } else {
            bad()
        };
        let entry = match id {
            "global-unused-error" | "local-unused-error" => n("7"),
            "global-dead-error" => cond(call("false", vec![]), var("g"), n("7")),
            "global-sequence-prefix" | "global-typed-sequence-prefix" => {
                call("exists", vec![var("g")])
            }
            "global-sequence-demand" => call("count", vec![var("g")]),
            "global-demand-error" => var("g"),
            _ => panic!("unrepresented selected binding fixture"),
        };
        let bindings = vec![declaration(
            "g",
            initializer,
            id != "local-unused-error",
            if id == "global-typed-sequence-prefix" {
                Some(("integer", Some("*")))
            } else {
                None
            },
        )];
        let e = expression(entry);
        let p = Prepared::new(&e, &bindings, machine_limits()).unwrap();
        let d = doc();
        let mut r = Run::new(
            &p,
            &d,
            EntryFocus::Document,
            atomic_limits(),
            machine_limits(),
            1_000_000,
            1_000_000,
        )
        .unwrap();
        let observed = match values(&mut r) {
            Ok(v) => format!(
                "TEXT\t{}",
                v.join("|")
                    .as_bytes()
                    .iter()
                    .fold(String::new(), |mut s, b| {
                        use std::fmt::Write;
                        write!(&mut s, "{b:02x}").unwrap();
                        s
                    })
            ),
            Err(Error::XPath(f)) => format!("ERROR\t{}", f.local_name()),
            Err(e) => panic!("{id}: {e}"),
        };
        let expected = fixture
            .lines()
            .find_map(|l| l.strip_prefix(&format!("{id}\t")))
            .unwrap();
        assert_eq!(observed, expected, "{id}");
    }
}

#[test]
fn atomic_path_prefix_replays_and_mixed_terminal_fault_is_demanded() {
    let e = expression(bin(
        "/",
        path(&["r", "a"]),
        cond(
            bin("=", step("attribute", "id"), s("one")),
            n("7"),
            Expr::Context,
        ),
    ));
    let p = Prepared::new(&e, &[], machine_limits()).unwrap();
    let d = doc();
    let mut r = Run::new(
        &p,
        &d,
        EntryFocus::Document,
        atomic_limits(),
        machine_limits(),
        1_000_000,
        1_000_000,
    )
    .unwrap();
    let c = r.cursor().unwrap();
    let first = r.next(c).unwrap().unwrap();
    assert_eq!(r.render(first).unwrap().as_str(), "7");
    assert_eq!(r.next(c), Err(Error::XPath(Fault::Xpty0018)));
    let c = r.cursor().unwrap();
    assert!(r.next(c).unwrap().is_some());
    assert_eq!(r.next(c), Err(Error::XPath(Fault::Xpty0018)));
}

#[test]
fn frame_and_cumulative_cache_admission_fail_terminally() {
    let mut tree = n("1");
    for _ in 0..20 {
        tree = group(tree);
    }
    let e = expression(tree);
    let p = Prepared::new(&e, &[], machine_limits()).unwrap();
    let d = doc();
    let mut limits = machine_limits();
    limits.frames = 5;
    let mut r = Run::new(
        &p,
        &d,
        EntryFocus::Document,
        atomic_limits(),
        limits,
        1000,
        1000,
    )
    .unwrap();
    let c = r.cursor().unwrap();
    let err = r.next(c).unwrap_err();
    assert_eq!(
        err,
        Error::Fatal(Failure::Resource(Resource::Machine("frames")))
    );
    assert_eq!(r.next(c), Err(err));
    drop(r);
    drop(p);
    let mut tree = e.tree;
    while let Expr::Group { operand } = tree {
        tree = *operand;
    }
    let e = expression(bin("to", n("1"), n("1000000")));
    let p = Prepared::new(&e, &[], machine_limits()).unwrap();
    let mut limits = machine_limits();
    limits.total_bytes = 20_000;
    let mut r = Run::new(
        &p,
        &d,
        EntryFocus::Document,
        atomic_limits(),
        limits,
        1000,
        1000,
    )
    .unwrap();
    let c = r.cursor().unwrap();
    let error = loop {
        match r.next(c) {
            Ok(Some(_)) => {}
            Ok(None) => panic!("truncated success"),
            Err(e) => break e,
        }
    };
    assert_eq!(
        error,
        Error::Fatal(Failure::Resource(Resource::Machine("storage")))
    );
    assert!(r.usage().total_bytes <= limits.total_bytes);
    assert_eq!(r.next(c), Err(error));
}

#[test]
fn comparisons_spend_numeric_string_and_pair_work_without_false_results() {
    let e = expression(bin(
        "=",
        s(&"x".repeat(2048)),
        s(&format!("{}y", "x".repeat(2047))),
    ));
    let p = Prepared::new(&e, &[], machine_limits()).unwrap();
    let d = doc();
    let atomic = einvoice_native_runtime::Limits::new(
        16384,
        65536,
        8192,
        100_000_000_000,
        9000,
        64_000_000,
        16_000_000,
        1000,
        100_000_000,
    )
    .unwrap();
    let mut r = Run::new(
        &p,
        &d,
        EntryFocus::Document,
        atomic,
        machine_limits(),
        1000,
        1000,
    )
    .unwrap();
    let c = r.cursor().unwrap();
    assert_eq!(
        r.next(c),
        Err(Error::Fatal(Failure::Resource(Resource::Work)))
    );
    assert!(matches!(
        r.error_origin().unwrap().construct,
        Expr::Binary { .. }
    ));
    let e = expression(bin(
        "=",
        bin("to", n("1"), n("100")),
        bin("to", n("101"), n("200")),
    ));
    let p = Prepared::new(&e, &[], machine_limits()).unwrap();
    let mut limits = machine_limits();
    limits.work = 5000;
    let mut r = Run::new(
        &p,
        &d,
        EntryFocus::Document,
        atomic_limits(),
        limits,
        1000,
        1000,
    )
    .unwrap();
    assert!(matches!(
        values(&mut r),
        Err(Error::Fatal(Failure::Resource(Resource::Machine("work"))))
    ));
}

#[test]
fn focus_independent_numeric_predicates_finish_after_their_selected_position() {
    let bindings = vec![declaration("p", n("1"), true, None)];
    for predicate in [var("p"), cast("integer", s("1"))] {
        let e = expression(call(
            "count",
            vec![filter(seq(vec![n("1"), bad()]), predicate)],
        ));
        let p = Prepared::new(&e, &bindings, machine_limits()).unwrap();
        let d = doc();
        let mut r = Run::new(
            &p,
            &d,
            EntryFocus::Document,
            atomic_limits(),
            machine_limits(),
            1_000_000,
            1_000_000,
        )
        .unwrap();
        assert_eq!(values(&mut r).unwrap(), ["1"]);
    }
    // Numeric position() changes with focus; a Boolean literal selects every item.
    assert_eq!(
        evaluate(filter(seq(vec![n("1"), n("2")]), call("position", vec![]))).unwrap(),
        ["1", "2"]
    );
    assert_eq!(
        evaluate(filter(seq(vec![n("1"), n("2")]), call("true", vec![]))).unwrap(),
        ["1", "2"]
    );
}

#[test]
fn descendant_shortcut_name_tests_and_union_use_original_document_order() {
    assert_eq!(
        evaluate(bin("//", Expr::Root, step("child", "b"))).unwrap(),
        ["10", "20", "30"]
    );
    assert_eq!(
        evaluate(bin("/", path(&["r"]), step("descendant", "b"))).unwrap(),
        ["10", "20", "30"]
    );
    assert_eq!(
        evaluate(bin(
            "|",
            filter(path(&["r", "a"]), n("2")),
            filter(path(&["r", "a"]), n("1"))
        ))
        .unwrap(),
        ["1020", "30"]
    );
    for test in [
        validation_ir::NodeTest::Local { local: "e".into() },
        validation_ir::NodeTest::Namespace {
            prefix: "p".into(),
            namespace: "urn:p".into(),
        },
    ] {
        assert_eq!(
            evaluate(call(
                "count",
                vec![bin(
                    "/",
                    path(&["r"]),
                    Expr::Step {
                        axis: "child".into(),
                        test
                    }
                )]
            ))
            .unwrap(),
            ["1"]
        );
    }
    assert_eq!(
        evaluate(bin(
            "/",
            filter(path(&["r", "a", "b"]), n("1")),
            step("following-sibling", "b")
        ))
        .unwrap(),
        ["20"]
    );
}

#[test]
fn foreign_item_inspection_latches_first_fatal_but_keeps_retained_evidence() {
    let e = expression(seq(vec![n("7"), n("8")]));
    let p = Prepared::new(&e, &[], machine_limits()).unwrap();
    let d = doc();
    let make = || {
        Run::new(
            &p,
            &d,
            EntryFocus::Document,
            atomic_limits(),
            machine_limits(),
            1000,
            1000,
        )
        .unwrap()
    };
    let mut a = make();
    let mut b = make();
    let ca = a.cursor().unwrap();
    let cb = b.cursor().unwrap();
    let foreign = a.next(ca).unwrap().unwrap();
    let retained = b.next(cb).unwrap().unwrap();
    let fatal = Error::Fatal(Failure::ForeignScope);
    assert!(matches!(b.item_ref(foreign), Err(e) if e == fatal));
    assert!(matches!(b.cursor(), Err(e) if e == fatal));
    assert_eq!(b.next(cb), Err(fatal));
    assert!(matches!(b.render(retained), Err(e) if e == fatal));
    assert!(matches!(b.item_ref(retained), Ok(ItemRef::Atomic(_))));
    assert!(b.error_origin().is_none());

    let e = expression(seq(vec![n("7"), call("string", vec![Expr::Context])]));
    let p = Prepared::new(&e, &[], machine_limits()).unwrap();
    let mut r = Run::new(
        &p,
        &d,
        EntryFocus::Document,
        atomic_limits(),
        machine_limits(),
        1000,
        1,
    )
    .unwrap();
    let c = r.cursor().unwrap();
    let retained = r.next(c).unwrap().unwrap();
    let first = r.next(c).unwrap_err();
    assert_eq!(
        first,
        Error::Fatal(Failure::Resource(Resource::SourceQuery))
    );
    let origin = r.error_origin().unwrap().construct;
    assert!(matches!(r.item_ref(foreign), Err(e) if e == first));
    assert_eq!(r.next(c), Err(first));
    assert!(matches!(r.render(retained), Err(e) if e == first));
    assert!(std::ptr::eq(r.error_origin().unwrap().construct, origin));
    assert!(matches!(r.item_ref(retained), Ok(ItemRef::Atomic(_))));
}

#[test]
fn external_render_failure_has_no_stale_ast_origin_and_preserves_poison_evidence() {
    let d = doc();
    let e = expression(seq(vec![n("7"), bad()]));
    let p = Prepared::new(&e, &[], machine_limits()).unwrap();
    let limited = einvoice_native_runtime::Limits::new(
        16384,
        65536,
        8192,
        100_000_000_000,
        100_000_000,
        64_000_000,
        1,
        100_000,
        500_000_000,
    )
    .unwrap();
    let mut r = Run::new(
        &p,
        &d,
        EntryFocus::Document,
        limited,
        machine_limits(),
        1000,
        1000,
    )
    .unwrap();
    let c = r.cursor().unwrap();
    let retained = r.next(c).unwrap().unwrap();
    assert_eq!(r.next(c), Err(Error::XPath(Fault::Forg0001)));
    assert!(r.error_origin().is_some());
    let failure = r.render(retained).unwrap_err();
    assert_eq!(
        failure,
        Error::Fatal(Failure::Resource(Resource::OutputBytes))
    );
    assert!(r.error_origin().is_none());
    assert_eq!(r.next(c), Err(failure));
    assert!(matches!(r.render(retained), Err(e) if e == failure));
    assert!(r.error_origin().is_none());
    assert!(matches!(r.item_ref(retained), Ok(ItemRef::Atomic(_))));

    let e = expression(Expr::Context);
    let p = Prepared::new(&e, &[], machine_limits()).unwrap();
    let mut r = Run::new(
        &p,
        &d,
        EntryFocus::Document,
        atomic_limits(),
        machine_limits(),
        1000,
        1,
    )
    .unwrap();
    let c = r.cursor().unwrap();
    let retained = r.next(c).unwrap().unwrap();
    assert!(matches!(
        r.render(retained),
        Err(Error::Fatal(Failure::Resource(Resource::SourceQuery)))
    ));
    assert!(r.error_origin().is_none());
    let ItemRef::Node(n) = r.item_ref(retained).unwrap() else {
        panic!("retained original source")
    };
    assert_eq!(n.identity(), d.document().identity());
}

#[test]
fn declaration_lexical_fault_is_distinct_from_initializer_fault_with_lazy_replay() {
    let d = doc();
    for ty in ["integer", "decimal", "float", "double", "boolean"] {
        for occurrence in [None, Some("?"), Some("*"), Some("+")] {
            for explicit in [false, true] {
                let initializer = if explicit {
                    cast(ty, path(&["r", "bad"]))
                } else {
                    path(&["r", "bad"])
                };
                let b = vec![declaration("v", initializer, false, Some((ty, occurrence)))];
                let e = expression(var("v"));
                let p = Prepared::new(&e, &b, machine_limits()).unwrap();
                let mut r = Run::new(
                    &p,
                    &d,
                    EntryFocus::Document,
                    atomic_limits(),
                    machine_limits(),
                    1_000_000,
                    1_000_000,
                )
                .unwrap();
                let expected = Error::XPath(if explicit {
                    Fault::Forg0001
                } else {
                    Fault::Xtte0570
                });
                let c = r.cursor().unwrap();
                assert_eq!(
                    r.next(c),
                    Err(expected),
                    "{ty} {occurrence:?} explicit={explicit}"
                );
                let origin = r.error_origin().unwrap().construct;
                assert_eq!(r.next(c), Err(expected));
                let replay = r.cursor().unwrap();
                assert_eq!(r.next(replay), Err(expected));
                assert!(std::ptr::eq(r.error_origin().unwrap().construct, origin));
            }
        }
    }
    let b = vec![declaration(
        "v",
        seq(vec![path(&["r", "n"]), path(&["r", "bad"])]),
        false,
        Some(("integer", Some("*"))),
    )];
    let e = expression(var("v"));
    let p = Prepared::new(&e, &b, machine_limits()).unwrap();
    let mut r = Run::new(
        &p,
        &d,
        EntryFocus::Document,
        atomic_limits(),
        machine_limits(),
        1_000_000,
        1_000_000,
    )
    .unwrap();
    let c = r.cursor().unwrap();
    let first = r.next(c).unwrap().unwrap();
    assert_eq!(r.render(first).unwrap().as_str(), "9007199254740993");
    assert_eq!(r.next(c), Err(Error::XPath(Fault::Xtte0570)));
    let replay = r.cursor().unwrap();
    let first = r.next(replay).unwrap().unwrap();
    assert_eq!(r.render(first).unwrap().as_str(), "9007199254740993");
    assert_eq!(r.next(replay), Err(Error::XPath(Fault::Xtte0570)));
}
