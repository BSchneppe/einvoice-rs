//! Tests for XPath AST binding phase.
//!
//! This file contains:
//! - Unit tests for binding individual AST nodes
//! - Integration tests for the full parse -> bind -> eval pipeline

use super::*;
use crate::namespace::table::NameTable;
use crate::xpath::arena::SourceSpan;
use crate::xpath::ast::{ExprNode, FunctionCallNode, IfNode, ValueNode};
use crate::xpath::context::{DynamicContext, NameBinder, XPathContext};
use crate::xpath::error::XPathError;
use crate::xpath::eval::eval_node;
use crate::xpath::functions::{FunctionHandle, FunctionId, XPathValue};
use crate::xpath::parser;
use crate::xpath::RoXmlNavigator;

// ============================================================================
// Unit Tests (manually constructed AST)
// ============================================================================

/// Helper to create a test arena with a function call
fn make_function_call(
    arena: &mut AstArena,
    prefix: &str,
    local_name: &str,
    args: Vec<AstNodeId>,
) -> AstNodeId {
    let span = SourceSpan::new(0, 10);
    let func = FunctionCallNode::new(prefix.to_string(), local_name.to_string(), args, span);
    arena.add(AstNode::FunctionCall(func))
}

/// Helper to wrap a node in an Expr
fn wrap_in_expr(arena: &mut AstArena, node_id: AstNodeId) -> AstNodeId {
    let span = SourceSpan::new(0, 10);
    let expr = ExprNode::single(node_id, span);
    arena.add(AstNode::Expr(expr))
}

#[test]
fn test_bind_function_call_default_ns() {
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut binder = NameBinder::new();

    let mut arena = AstArena::new();
    // Create string literal arguments
    let arg1 = arena.add(AstNode::Value(ValueNode::String("a".to_string())));
    let arg2 = arena.add(AstNode::Value(ValueNode::String("b".to_string())));
    // Create concat('a', 'b') function call
    let func_id = make_function_call(&mut arena, "", "concat", vec![arg1, arg2]);
    let root = wrap_in_expr(&mut arena, func_id);

    bind_node(&mut arena, root, &ctx, &mut binder).expect("bind failed");

    // Verify the function call has function_handle set
    if let AstNode::FunctionCall(func) = arena.get(func_id) {
        assert_eq!(
            func.function_handle,
            Some(FunctionHandle::from(FunctionId::Concat))
        );
    } else {
        panic!("Expected FunctionCall node");
    }
}

#[test]
fn test_bind_true_false() {
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut binder = NameBinder::new();

    let mut arena = AstArena::new();
    // Create true() function call
    let func_id = make_function_call(&mut arena, "", "true", vec![]);
    let root = wrap_in_expr(&mut arena, func_id);

    bind_node(&mut arena, root, &ctx, &mut binder).expect("bind failed");

    if let AstNode::FunctionCall(func) = arena.get(func_id) {
        assert_eq!(
            func.function_handle,
            Some(FunctionHandle::from(FunctionId::True))
        );
    } else {
        panic!("Expected FunctionCall node");
    }

    // Test false()
    let mut arena = AstArena::new();
    let func_id = make_function_call(&mut arena, "", "false", vec![]);
    let root = wrap_in_expr(&mut arena, func_id);

    bind_node(&mut arena, root, &ctx, &mut binder).expect("bind failed");

    if let AstNode::FunctionCall(func) = arena.get(func_id) {
        assert_eq!(
            func.function_handle,
            Some(FunctionHandle::from(FunctionId::False))
        );
    } else {
        panic!("Expected FunctionCall node");
    }
}

#[test]
fn test_bind_function_not_found() {
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut binder = NameBinder::new();

    let mut arena = AstArena::new();
    // Create unknown() function call
    let func_id = make_function_call(&mut arena, "", "unknown", vec![]);
    let root = wrap_in_expr(&mut arena, func_id);

    let result = bind_node(&mut arena, root, &ctx, &mut binder);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.error_code(), Some("XPST0017"));
}

#[test]
fn test_bind_undefined_prefix() {
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut binder = NameBinder::new();

    let mut arena = AstArena::new();
    // Create foo:bar() function call with undefined prefix
    let func_id = make_function_call(&mut arena, "foo", "bar", vec![]);
    let root = wrap_in_expr(&mut arena, func_id);

    let result = bind_node(&mut arena, root, &ctx, &mut binder);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.error_code(), Some("XPST0081"));
}

#[test]
fn test_bind_nested_function() {
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut binder = NameBinder::new();

    let mut arena = AstArena::new();
    // Create concat('a', 'b')
    let arg1 = arena.add(AstNode::Value(ValueNode::String("a".to_string())));
    let arg2 = arena.add(AstNode::Value(ValueNode::String("b".to_string())));
    let inner_func = make_function_call(&mut arena, "", "concat", vec![arg1, arg2]);
    // Create upper-case(concat('a', 'b'))
    let outer_func = make_function_call(&mut arena, "", "upper-case", vec![inner_func]);
    let root = wrap_in_expr(&mut arena, outer_func);

    bind_node(&mut arena, root, &ctx, &mut binder).expect("bind failed");

    // The outer function should be upper-case
    if let AstNode::FunctionCall(func) = arena.get(outer_func) {
        assert_eq!(
            func.function_handle,
            Some(FunctionHandle::from(FunctionId::UpperCase))
        );
    }

    // The inner function should be concat
    if let AstNode::FunctionCall(func) = arena.get(inner_func) {
        assert_eq!(
            func.function_handle,
            Some(FunctionHandle::from(FunctionId::Concat))
        );
    }
}

#[test]
fn test_bind_if_expression() {
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut binder = NameBinder::new();

    let mut arena = AstArena::new();
    // Create true() for test condition
    let test_func = make_function_call(&mut arena, "", "true", vec![]);
    // Create then/else branches
    let then_val = arena.add(AstNode::Value(ValueNode::String("yes".to_string())));
    let else_val = arena.add(AstNode::Value(ValueNode::String("no".to_string())));
    // Create if node
    let span = SourceSpan::new(0, 30);
    let if_node = IfNode::new(test_func, then_val, else_val, span);
    let if_id = arena.add(AstNode::If(if_node));
    let root = wrap_in_expr(&mut arena, if_id);

    bind_node(&mut arena, root, &ctx, &mut binder).expect("bind failed");

    // Verify the condition function was bound
    if let AstNode::FunctionCall(func) = arena.get(test_func) {
        assert_eq!(
            func.function_handle,
            Some(FunctionHandle::from(FunctionId::True))
        );
    }
}

#[test]
fn test_bind_literal() {
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut binder = NameBinder::new();

    let mut arena = AstArena::new();
    let val = arena.add(AstNode::Value(ValueNode::Integer("42".to_string())));
    let root = wrap_in_expr(&mut arena, val);

    // Binding literals should succeed (no-op)
    bind_node(&mut arena, root, &ctx, &mut binder).expect("bind failed");
}

// ============================================================================
// Integration Tests (Parse -> Bind -> Eval)
// ============================================================================

/// Helper to parse, bind, and evaluate an XPath expression without context item
fn eval_xpath(expr: &str) -> Result<XPathValue<RoXmlNavigator<'static>>, XPathError> {
    let mut parsed = parser::parse(expr).map_err(|e| XPathError::syntax_error(e.to_string()))?;

    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut binder = NameBinder::new();

    bind_node(&mut parsed.arena, parsed.root, &ctx, &mut binder)?;

    let mut dyn_ctx: DynamicContext<'_, RoXmlNavigator<'static>> =
        DynamicContext::new(&ctx, binder.len());

    eval_node(&parsed.arena, parsed.root, &mut dyn_ctx)
}

#[test]
fn test_parse_bind_eval_concat() {
    let result = eval_xpath("concat('a', 'b')").unwrap();
    assert_eq!(result.to_owned_string(), Some("ab".to_string()));
}

#[test]
fn test_parse_bind_eval_string_length() {
    let result = eval_xpath("string-length('hello')").unwrap();
    assert_eq!(
        result.as_integer().map(|i| i.to_string()),
        Some("5".to_string())
    );
}

#[test]
fn test_parse_bind_eval_substring() {
    let result = eval_xpath("substring('hello', 2, 3)").unwrap();
    assert_eq!(result.to_owned_string(), Some("ell".to_string()));
}

#[test]
fn test_parse_bind_eval_upper_case() {
    let result = eval_xpath("upper-case('hello')").unwrap();
    assert_eq!(result.to_owned_string(), Some("HELLO".to_string()));
}

#[test]
fn test_parse_bind_eval_boolean_functions() {
    assert_eq!(eval_xpath("true()").unwrap().as_bool(), Some(true));
    assert_eq!(eval_xpath("false()").unwrap().as_bool(), Some(false));
    assert_eq!(eval_xpath("not(true())").unwrap().as_bool(), Some(false));
}

#[test]
fn test_parse_bind_eval_numeric_functions() {
    assert_eq!(eval_xpath("abs(-5)").unwrap().as_f64(), Some(5.0));
    assert_eq!(eval_xpath("ceiling(1.5)").unwrap().as_f64(), Some(2.0));
    assert_eq!(eval_xpath("floor(1.5)").unwrap().as_f64(), Some(1.0));
    assert_eq!(eval_xpath("round(1.5)").unwrap().as_f64(), Some(2.0));
}

#[test]
fn test_parse_bind_eval_sequence_functions() {
    assert_eq!(eval_xpath("empty(())").unwrap().as_bool(), Some(true));
    assert_eq!(eval_xpath("exists(1)").unwrap().as_bool(), Some(true));
    assert_eq!(
        eval_xpath("count((1, 2, 3))")
            .unwrap()
            .as_integer()
            .map(|i| i.to_string()),
        Some("3".to_string())
    );
}

// ============================================================================
// Additional Integration Tests (Phase 7.2-7.4)
// ============================================================================

#[test]
fn test_arithmetic_precedence() {
    // Multiplication has higher precedence than addition
    let result = eval_xpath("1 + 2 * 3").unwrap();
    // Should be 7 (1 + (2*3)), not 9 ((1+2)*3)
    assert_eq!(
        result.as_integer().map(|i| i.to_string()),
        Some("7".to_string())
    );
}

#[test]
fn test_for_expression() {
    let result = eval_xpath("for $i in (1, 2, 3) return $i * 2").unwrap();
    // Result should be sequence (2, 4, 6)
    if let XPathValue::Sequence(seq) = result {
        assert_eq!(seq.len(), 3);
        assert_eq!(
            seq[0].as_integer().map(|i| i.to_string()),
            Some("2".to_string())
        );
        assert_eq!(
            seq[1].as_integer().map(|i| i.to_string()),
            Some("4".to_string())
        );
        assert_eq!(
            seq[2].as_integer().map(|i| i.to_string()),
            Some("6".to_string())
        );
    } else {
        panic!("Expected sequence result");
    }
}

#[test]
fn test_quantified_some() {
    let result = eval_xpath("some $x in (1, 2, 3) satisfies $x > 2").unwrap();
    assert_eq!(result.as_bool(), Some(true));
}

#[test]
fn test_quantified_every() {
    let result = eval_xpath("every $x in (1, 2, 3) satisfies $x > 0").unwrap();
    assert_eq!(result.as_bool(), Some(true));

    let result = eval_xpath("every $x in (1, 2, 3) satisfies $x > 2").unwrap();
    assert_eq!(result.as_bool(), Some(false));
}

#[test]
fn test_parse_error_xpst0003() {
    let result = eval_xpath("1 +");
    assert!(result.is_err());
    let err = result.err().unwrap();
    assert_eq!(err.error_code(), Some("XPST0003"));
}

#[test]
fn test_undefined_variable_xpst0008() {
    let result = eval_xpath("$undefined");
    assert!(result.is_err());
    let err = result.err().unwrap();
    assert_eq!(err.error_code(), Some("XPST0008"));
}

#[test]
fn test_unknown_function_xpst0017() {
    let result = eval_xpath("unknown-function()");
    assert!(result.is_err());
    let err = result.err().unwrap();
    assert_eq!(err.error_code(), Some("XPST0017"));
}

// ============================================================================
// Constructor Function Tests
// ============================================================================

use crate::namespace::context::NamespaceContextSnapshot;
use crate::namespace::table::well_known;
use crate::xpath::ast::TypeExprKind;
use crate::xpath::XPathMode;

/// Helper to create an XPathContext with the xs: prefix bound to the XSD namespace.
fn make_xs_context(names: &NameTable) -> XPathContext<'_> {
    let ns_snapshot = NamespaceContextSnapshot {
        default_ns: None,
        bindings: vec![(well_known::XS_PREFIX, well_known::XS_NAMESPACE)],
    };
    XPathContext::new(names).with_namespaces(ns_snapshot)
}

#[test]
fn test_bind_constructor_xs_integer() {
    let names = NameTable::new();
    let ctx = make_xs_context(&names);
    let mut binder = NameBinder::new();

    let mut arena = AstArena::new();
    let arg = arena.add(AstNode::Value(ValueNode::String("42".to_string())));
    let func_id = make_function_call(&mut arena, "xs", "integer", vec![arg]);
    let root = wrap_in_expr(&mut arena, func_id);

    bind_node(&mut arena, root, &ctx, &mut binder).expect("bind failed");

    // The function call should have been rewritten to a CastAs TypeExpr
    match arena.get(func_id) {
        AstNode::TypeExpr(te) => {
            assert_eq!(te.kind, TypeExprKind::CastAs);
            assert!(te.resolved_atomic_type.is_some());
        }
        other => panic!("Expected TypeExpr(CastAs), got {:?}", other),
    }
}

#[test]
fn test_bind_constructor_xs_unsigned_short() {
    let names = NameTable::new();
    let ctx = make_xs_context(&names);
    let mut binder = NameBinder::new();

    let mut arena = AstArena::new();
    let arg = arena.add(AstNode::Value(ValueNode::Integer("42".to_string())));
    let func_id = make_function_call(&mut arena, "xs", "unsignedShort", vec![arg]);
    let root = wrap_in_expr(&mut arena, func_id);

    bind_node(&mut arena, root, &ctx, &mut binder).expect("bind failed");

    match arena.get(func_id) {
        AstNode::TypeExpr(te) => {
            assert_eq!(te.kind, TypeExprKind::CastAs);
            assert!(te.resolved_atomic_type.is_some());
        }
        other => panic!("Expected TypeExpr(CastAs), got {:?}", other),
    }
}

/// XPath 2.0 §3.10.4: there is a constructor function "for every atomic type in
/// the in-scope schema types (except xs:NOTATION and xs:anyAtomicType, which are
/// not instantiable)", so a call of either names no function (XPST0017 — XQTS
/// `K-SeqExprCast-65`, `-1480`).
#[test]
fn test_bind_constructor_notation_rejected() {
    for local in ["NOTATION", "anyAtomicType"] {
        let names = NameTable::new();
        let ctx = make_xs_context(&names);
        let mut binder = NameBinder::new();

        let mut arena = AstArena::new();
        let arg = arena.add(AstNode::Value(ValueNode::String("x".to_string())));
        let func_id = make_function_call(&mut arena, "xs", local, vec![arg]);
        let root = wrap_in_expr(&mut arena, func_id);

        let result = bind_node(&mut arena, root, &ctx, &mut binder);
        assert!(result.is_err(), "xs:{local}");
        assert_eq!(
            result.unwrap_err().error_code(),
            Some("XPST0017"),
            "xs:{local}"
        );
    }
}

#[test]
fn test_bind_constructor_wrong_arity() {
    let names = NameTable::new();
    let ctx = make_xs_context(&names);
    let mut binder = NameBinder::new();

    let mut arena = AstArena::new();
    let arg1 = arena.add(AstNode::Value(ValueNode::Integer("1".to_string())));
    let arg2 = arena.add(AstNode::Value(ValueNode::Integer("2".to_string())));
    let func_id = make_function_call(&mut arena, "xs", "integer", vec![arg1, arg2]);
    let root = wrap_in_expr(&mut arena, func_id);

    // Should fail with XPST0017 (not a constructor, falls through to function lookup)
    let result = bind_node(&mut arena, root, &ctx, &mut binder);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().error_code(), Some("XPST0017"));
}

#[test]
fn test_bind_constructor_non_xs_namespace() {
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut binder = NameBinder::new();

    let mut arena = AstArena::new();
    let arg = arena.add(AstNode::Value(ValueNode::Integer("42".to_string())));
    // "fn:" prefix -> default function namespace, not xs:
    let func_id = make_function_call(&mut arena, "", "integer", vec![arg]);
    let root = wrap_in_expr(&mut arena, func_id);

    // Should fail with XPST0017 (fn:integer doesn't exist)
    let result = bind_node(&mut arena, root, &ctx, &mut binder);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().error_code(), Some("XPST0017"));
}

#[test]
fn test_bind_constructor_xpath10_mode_ignored() {
    let names = NameTable::new();
    let ctx = make_xs_context(&names).with_mode(XPathMode::XPath10);
    let mut binder = NameBinder::new();

    let mut arena = AstArena::new();
    let arg = arena.add(AstNode::Value(ValueNode::String("42".to_string())));
    let func_id = make_function_call(&mut arena, "xs", "integer", vec![arg]);
    let root = wrap_in_expr(&mut arena, func_id);

    // In XPath 1.0 mode, constructor functions should NOT be recognized
    let result = bind_node(&mut arena, root, &ctx, &mut binder);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().error_code(), Some("XPST0017"));
}
