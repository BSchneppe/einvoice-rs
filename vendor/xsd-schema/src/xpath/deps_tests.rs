//! Tests for compile-time expression dependency metadata.
//!
//! The focus-flag table below is the executable form of the semantics
//! documented on `XPathExpr::uses_focus` / `uses_position` / `uses_last`:
//! only reads of the *initial* focus count, i.e. of the dynamic context that
//! is in effect when the expression is entered.

use crate::namespace::context::NamespaceContextSnapshot;
use crate::namespace::table::{NameTable, XS_NAMESPACE};
use crate::xpath::api::XPathExpr;
use crate::xpath::functions::{XPath10Catalog, FN_2010_NAMESPACE, FN_NAMESPACE};
use crate::xpath::{XPathContext, XPathMode};

/// Compile in XPath 2.0 mode with the given external variable names.
/// The `xs` prefix is bound so constructor functions can be exercised.
fn compile(expr: &str, vars: &[&str]) -> XPathExpr {
    let names = NameTable::new();
    let mut namespaces = NamespaceContextSnapshot::default();
    namespaces
        .bindings
        .push((names.add("xs"), names.add(XS_NAMESPACE)));
    let ctx = XPathContext::new(&names).with_namespaces(namespaces);
    XPathExpr::compile_with_vars(expr, &ctx, vars)
        .unwrap_or_else(|e| panic!("compile of {expr:?} failed: {e:?}"))
}

/// `(uses_focus, uses_position, uses_last)` of an expression using `$x`, `$s`
/// and `$a` as declared externals.
fn flags(expr: &str) -> (bool, bool, bool) {
    let expr = compile(expr, &["x", "s", "a"]);
    (expr.uses_focus(), expr.uses_position(), expr.uses_last())
}

// ============================================================================
// Focus flags
// ============================================================================

#[test]
fn focus_flag_table() {
    // (expression, uses_focus, uses_position, uses_last)
    let cases: &[(&str, bool, bool, bool)] = &[
        // --- predicates run in an inner focus (§3.2.2, §3.3.2) -------------
        ("$x[position() = 1]", false, false, false),
        ("a[position() = 1]", true, false, false),
        ("$x[last()]", false, false, false),
        // --- the initial focus itself ------------------------------------
        ("position() = 1", true, true, false),
        ("last()", true, false, true),
        ("position() = last()", true, true, true),
        // `//` starts from the root of the context node (§3.2), but the
        // predicate of the `a` step is an inner focus.
        ("count(//a[last()])", true, false, false),
        // --- absolute paths read the context node to find its root -------
        ("/a", true, false, false),
        ("/", true, false, false),
        ("//a", true, false, false),
        // --- paths rooted in a variable need no focus --------------------
        ("$x/a[last()]", false, false, false),
        ("$x/a/b", false, false, false),
        ("$x//a", false, false, false),
        // --- for / quantified keep the enclosing focus (§3.7, §3.9) ------
        ("for $i in $s return $i/@x", false, false, false),
        ("for $i in a return $i", true, false, false),
        (
            "for $i in $s return $i/b[position() = 1]",
            false,
            false,
            false,
        ),
        ("some $i in $s satisfies $i = .", true, false, false),
        ("every $i in $s satisfies $i = 1", false, false, false),
        ("some $i in a satisfies $i = 1", true, false, false),
        // --- if keeps the enclosing focus --------------------------------
        ("if (position() = 1) then 'a' else 'b'", true, true, false),
        ("if ($x) then 'a' else 'b'", false, false, false),
        // --- context item and axis steps ---------------------------------
        (".", true, false, false),
        ("self::node()", true, false, false),
        ("@id", true, false, false),
        ("child::a", true, false, false),
        ("..", true, false, false),
        ("descendant::a", true, false, false),
        ("a/b", true, false, false),
        // --- focus-free expressions --------------------------------------
        ("1 + 2", false, false, false),
        ("'x'", false, false, false),
        ("$a", false, false, false),
        ("xs:integer($a)", false, false, false),
        ("()", false, false, false),
        ("1 to 10", false, false, false),
        ("$x instance of element()", false, false, false),
        ("count($s)", false, false, false),
        ("string($x)", false, false, false),
        ("concat('a', 'b')", false, false, false),
    ];

    for &(expr, focus, position, last) in cases {
        assert_eq!(
            flags(expr),
            (focus, position, last),
            "flags for {expr:?} (focus, position, last)"
        );
    }
}

#[test]
fn implicit_context_functions_use_the_focus() {
    // Every zero-arity (or context-defaulting) form derived from the
    // implementations in `functions/` — see the table on `call_focus_use`.
    let cases = [
        "string()",
        "number()",
        "name()",
        "local-name()",
        "namespace-uri()",
        "root()",
        "base-uri()",
        "string-length()",
        "normalize-space()",
        "lang('en')",
        "id('x')",
    ];
    for expr in cases {
        assert!(
            compile(expr, &[]).uses_focus(),
            "{expr:?} should report uses_focus()"
        );
    }

    // The explicit-argument forms do not touch the focus.
    let explicit = [
        "string($x)",
        "number($x)",
        "name($x)",
        "local-name($x)",
        "namespace-uri($x)",
        "root($x)",
        "base-uri($x)",
        "string-length($x)",
        "normalize-space($x)",
        "lang('en', $x)",
        "id('x', $x)",
    ];
    for expr in explicit {
        assert!(
            !compile(expr, &["x"]).uses_focus(),
            "{expr:?} should not report uses_focus()"
        );
    }
}

#[test]
fn idref_reads_the_focus_only_in_its_one_argument_form() {
    // F&O 1.0 §15.5.3: without `$node` the target document is the context
    // item's, as for fn:id.
    assert!(compile("idref('x')", &[]).uses_focus());
    assert!(!compile("idref('x', $x)", &["x"]).uses_focus());
}

#[test]
fn nested_focus_never_leaks_out() {
    // A focus-using function buried in a predicate or a later step stays there.
    for expr in [
        "a[string-length() > 2]",
        "$x/a[name() = 'b']",
        "$x[. = 1]",
        "$x/b/position()",
        "/a[position() = 1]",
    ] {
        let compiled = compile(expr, &["x"]);
        assert!(
            !compiled.uses_position(),
            "{expr:?} should not report uses_position()"
        );
        assert!(
            !compiled.uses_last(),
            "{expr:?} should not report uses_last()"
        );
    }
}

// ============================================================================
// Referenced external variables
// ============================================================================

#[test]
fn only_referenced_externals_are_reported() {
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let expr = XPathExpr::compile_with_vars("$a + 1", &ctx, &["a", "b", "c"]).unwrap();

    // external_vars() keeps its documented behaviour: every supplied name.
    assert_eq!(expr.external_vars().len(), 3);

    let referenced: Vec<String> = expr
        .referenced_external_vars()
        .map(|v| names.try_resolve(v.name.local_name).unwrap())
        .collect();
    assert_eq!(referenced, vec!["a".to_string()]);

    let slot_of_a = expr.external_vars()[0].slot;
    let slot_of_b = expr.external_vars()[1].slot;
    let slot_of_c = expr.external_vars()[2].slot;
    assert!(expr.references_external_var(slot_of_a));
    assert!(!expr.references_external_var(slot_of_b));
    assert!(!expr.references_external_var(slot_of_c));
    // Slots that are not externals of this expression.
    assert!(!expr.references_external_var(99));
}

#[test]
fn references_across_nested_foci_and_bindings() {
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let expr = XPathExpr::compile_with_vars(
        "$a/b[$c = 1]/for-each-marker[count($d) > 0]",
        &ctx,
        &["a", "b", "c", "d"],
    )
    .unwrap();

    let referenced: Vec<String> = expr
        .referenced_external_vars()
        .map(|v| names.try_resolve(v.name.local_name).unwrap())
        .collect();
    // $b is only a node name here, not a variable reference.
    assert_eq!(
        referenced,
        vec!["a".to_string(), "c".to_string(), "d".to_string()]
    );
}

#[test]
fn expression_local_variables_are_not_externals() {
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);

    // No externals declared at all.
    let expr = XPathExpr::compile("for $i in 1 to 3 return $i + 1", &ctx).unwrap();
    assert_eq!(expr.referenced_external_vars().count(), 0);
    assert!(!expr.references_external_var(0));

    // A `for` variable shadowing a declared external of the same name must not
    // mark that external as referenced: the binder allocates it a fresh slot
    // above the external boundary.
    let expr = XPathExpr::compile_with_vars("for $i in 1 to 3 return $i", &ctx, &["i"]).unwrap();
    assert_eq!(expr.external_vars().len(), 1);
    assert_eq!(expr.referenced_external_vars().count(), 0);
    assert!(!expr.references_external_var(expr.external_vars()[0].slot));

    // Same for quantified expressions.
    let expr =
        XPathExpr::compile_with_vars("some $i in 1 to 3 satisfies $i = 2", &ctx, &["i"]).unwrap();
    assert_eq!(expr.referenced_external_vars().count(), 0);

    // The binding sequence, however, is in the outer scope: here `$i` in
    // `1 to $i` is the external.
    let expr = XPathExpr::compile_with_vars("for $i in (1 to $i) return $i", &ctx, &["i"]).unwrap();
    assert_eq!(expr.referenced_external_vars().count(), 1);
    assert!(expr.references_external_var(expr.external_vars()[0].slot));
}

#[test]
fn referenced_externals_survive_more_than_64_slots() {
    // The bitset is word-based; make sure slot 64 and beyond are handled.
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let decls: Vec<String> = (0..70).map(|i| format!("v{i}")).collect();
    let decl_refs: Vec<&str> = decls.iter().map(String::as_str).collect();
    let expr = XPathExpr::compile_with_vars("$v0 + $v64 + $v69", &ctx, &decl_refs).unwrap();

    assert_eq!(expr.external_vars().len(), 70);
    assert_eq!(expr.referenced_external_vars().count(), 3);
    assert!(expr.references_external_var(0));
    assert!(expr.references_external_var(64));
    assert!(expr.references_external_var(69));
    assert!(!expr.references_external_var(1));
    assert!(!expr.references_external_var(63));
    assert!(!expr.references_external_var(65));
}

// ============================================================================
// Function call inventory
// ============================================================================

/// `(namespace, local name, arity)` triples of an expression's calls.
fn calls(expr: &XPathExpr) -> Vec<(String, String, usize)> {
    expr.function_calls()
        .iter()
        .map(|c| (c.namespace.clone(), c.local_name.clone(), c.arity))
        .collect()
}

#[test]
fn function_calls_are_distinct_and_nested_foci_included() {
    let expr = compile("count(distinct-values($s)) + string-length()", &["s"]);
    assert_eq!(
        calls(&expr),
        vec![
            (FN_NAMESPACE.to_string(), "count".to_string(), 1),
            (FN_NAMESPACE.to_string(), "distinct-values".to_string(), 1),
            (FN_NAMESPACE.to_string(), "string-length".to_string(), 0),
        ]
    );

    // Calls inside predicates, for bodies and quantified tests are reported,
    // and repeated call sites collapse into one entry; differing arity does not.
    let expr = compile(
        "a[count(b) = 1]/c[count(d) = 2][substring(@e, 1) or substring(@e, 1, 2)]",
        &[],
    );
    assert_eq!(
        calls(&expr),
        vec![
            (FN_NAMESPACE.to_string(), "count".to_string(), 1),
            (FN_NAMESPACE.to_string(), "substring".to_string(), 2),
            (FN_NAMESPACE.to_string(), "substring".to_string(), 3),
        ]
    );

    // Constructor functions are rewritten into `cast as` during binding, so
    // they are type expressions rather than calls.
    let expr = compile("xs:integer($a)", &["a"]);
    assert!(expr.function_calls().is_empty());

    // An expression without calls reports none.
    assert!(compile("$a + 1", &["a"]).function_calls().is_empty());
}

#[test]
fn function_calls_in_xpath10_mode_report_the_bound_namespace() {
    // XPath 1.0 core functions are *looked up* without a namespace (the static
    // context's default function namespace is empty in 1.0 mode) but they
    // *bind* to the fn: entries via XPath10Catalog, and the bound namespace is
    // what gets reported — so `namespace == FN_NAMESPACE` tests written by a
    // caller work in both modes.
    let names = NameTable::new();
    let catalog = XPath10Catalog;
    let ctx = XPathContext::new(&names)
        .with_mode(XPathMode::XPath10)
        .with_function_catalog(&catalog);
    assert_eq!(ctx.default_function_namespace(), "");

    let expr = XPathExpr::compile_with_vars("count(//a) + string-length(.)", &ctx, &[]).unwrap();
    assert_eq!(
        calls(&expr),
        vec![
            (FN_NAMESPACE.to_string(), "count".to_string(), 1),
            (FN_NAMESPACE.to_string(), "string-length".to_string(), 1),
        ]
    );
    // Focus analysis is mode-independent: `//a` reads the context node.
    assert!(expr.uses_focus());
    assert!(!expr.uses_position());

    let expr = XPathExpr::compile_with_vars("position() = last()", &ctx, &[]).unwrap();
    assert!(expr.uses_position());
    assert!(expr.uses_last());
    assert_eq!(
        calls(&expr),
        vec![
            (FN_NAMESPACE.to_string(), "position".to_string(), 0),
            (FN_NAMESPACE.to_string(), "last".to_string(), 0),
        ]
    );

    // Identical triples in XPath 2.0 mode, where the same calls are looked up
    // in fn: directly.
    let expr20 = compile("position() = last()", &[]);
    assert_eq!(calls(&expr20), calls(&expr));
}

#[test]
fn function_calls_normalize_the_fn_namespace_alias() {
    // A call written against the XPath 2010 fn: alias binds to the same
    // registry entry, so it is reported under the canonical fn: namespace.
    let names = NameTable::new();
    let mut namespaces = NamespaceContextSnapshot::default();
    namespaces
        .bindings
        .push((names.add("fn2010"), names.add(FN_2010_NAMESPACE)));
    let ctx = XPathContext::new(&names).with_namespaces(namespaces);
    let expr = XPathExpr::compile("fn2010:count((1, 2))", &ctx).unwrap();

    assert_eq!(
        expr.function_calls()
            .iter()
            .map(|c| (c.namespace.clone(), c.local_name.clone(), c.arity))
            .collect::<Vec<_>>(),
        vec![(FN_NAMESPACE.to_string(), "count".to_string(), 1)]
    );
}
