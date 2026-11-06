use super::*;
use crate::namespace::table::NameTable;
use crate::xpath::arena::SourceSpan;
use crate::xpath::ast::{
    BinaryOpNode, ExprNode, FunctionCallNode, IfNode, RangeNode, UnaryOpKind, UnaryOpNode,
    ValueNode,
};
use crate::xpath::bind::bind_node;
use crate::xpath::context::{NameBinder, XPathContext};
use crate::xpath::RoXmlNavigator;

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

/// Helper to bind and eval a manually constructed AST
fn bind_and_eval(
    arena: &mut AstArena,
    root: AstNodeId,
) -> Result<XPathValue<RoXmlNavigator<'static>>, XPathError> {
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut binder = NameBinder::new();

    bind_node(arena, root, &ctx, &mut binder)?;

    let mut dyn_ctx: DynamicContext<'_, RoXmlNavigator<'static>> =
        DynamicContext::new(&ctx, binder.len());

    eval_node(arena, root, &mut dyn_ctx)
}

#[test]
fn test_eval_true_false() {
    // Test true()
    let mut arena = AstArena::new();
    let func_id = make_function_call(&mut arena, "", "true", vec![]);
    let root = wrap_in_expr(&mut arena, func_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_boolean(), Some(true));
        }
        _ => panic!("Expected boolean true"),
    }

    // Test false()
    let mut arena = AstArena::new();
    let func_id = make_function_call(&mut arena, "", "false", vec![]);
    let root = wrap_in_expr(&mut arena, func_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_boolean(), Some(false));
        }
        _ => panic!("Expected boolean false"),
    }
}

#[test]
fn test_eval_concat() {
    let mut arena = AstArena::new();
    let arg1 = arena.add(AstNode::Value(ValueNode::String("Hello".to_string())));
    let arg2 = arena.add(AstNode::Value(ValueNode::String(" ".to_string())));
    let arg3 = arena.add(AstNode::Value(ValueNode::String("World".to_string())));
    let func_id = make_function_call(&mut arena, "", "concat", vec![arg1, arg2, arg3]);
    let root = wrap_in_expr(&mut arena, func_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_string(), Some("Hello World"));
        }
        _ => panic!("Expected string"),
    }
}

#[test]
fn test_eval_nested_function() {
    let mut arena = AstArena::new();
    // Create concat('a', 'b')
    let arg1 = arena.add(AstNode::Value(ValueNode::String("a".to_string())));
    let arg2 = arena.add(AstNode::Value(ValueNode::String("b".to_string())));
    let inner_func = make_function_call(&mut arena, "", "concat", vec![arg1, arg2]);
    // Create upper-case(concat('a', 'b'))
    let outer_func = make_function_call(&mut arena, "", "upper-case", vec![inner_func]);
    let root = wrap_in_expr(&mut arena, outer_func);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_string(), Some("AB"));
        }
        _ => panic!("Expected string"),
    }
}

#[test]
fn test_eval_integer_literal() {
    let mut arena = AstArena::new();
    let val = arena.add(AstNode::Value(ValueNode::Integer("42".to_string())));
    let root = wrap_in_expr(&mut arena, val);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(
                v.as_integer().map(|i| i.to_string()),
                Some("42".to_string())
            );
        }
        _ => panic!("Expected integer"),
    }
}

#[test]
fn test_eval_double_literal() {
    let mut arena = AstArena::new();
    let val = arena.add(AstNode::Value(ValueNode::Double("2.5".to_string())));
    let root = wrap_in_expr(&mut arena, val);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert!(v.as_double().is_some());
        }
        _ => panic!("Expected double"),
    }
}

#[test]
fn test_eval_string_literal() {
    let mut arena = AstArena::new();
    let val = arena.add(AstNode::Value(ValueNode::String("hello".to_string())));
    let root = wrap_in_expr(&mut arena, val);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_string(), Some("hello"));
        }
        _ => panic!("Expected string"),
    }
}

#[test]
fn test_eval_empty_sequence() {
    let mut arena = AstArena::new();
    let val = arena.add(AstNode::Value(ValueNode::Empty));
    let root = wrap_in_expr(&mut arena, val);

    let result = bind_and_eval(&mut arena, root).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_eval_if_true() {
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

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_string(), Some("yes"));
        }
        _ => panic!("Expected string 'yes'"),
    }
}

#[test]
fn test_eval_if_false() {
    let mut arena = AstArena::new();
    // Create false() for test condition
    let test_func = make_function_call(&mut arena, "", "false", vec![]);
    // Create then/else branches
    let then_val = arena.add(AstNode::Value(ValueNode::String("yes".to_string())));
    let else_val = arena.add(AstNode::Value(ValueNode::String("no".to_string())));
    // Create if node
    let span = SourceSpan::new(0, 30);
    let if_node = IfNode::new(test_func, then_val, else_val, span);
    let if_id = arena.add(AstNode::If(if_node));
    let root = wrap_in_expr(&mut arena, if_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_string(), Some("no"));
        }
        _ => panic!("Expected string 'no'"),
    }
}

#[test]
fn test_eval_not() {
    // Test not(true())
    let mut arena = AstArena::new();
    let true_func = make_function_call(&mut arena, "", "true", vec![]);
    let not_func = make_function_call(&mut arena, "", "not", vec![true_func]);
    let root = wrap_in_expr(&mut arena, not_func);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_boolean(), Some(false));
        }
        _ => panic!("Expected boolean false"),
    }

    // Test not(false())
    let mut arena = AstArena::new();
    let false_func = make_function_call(&mut arena, "", "false", vec![]);
    let not_func = make_function_call(&mut arena, "", "not", vec![false_func]);
    let root = wrap_in_expr(&mut arena, not_func);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_boolean(), Some(true));
        }
        _ => panic!("Expected boolean true"),
    }
}

#[test]
fn test_eval_position_last() {
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut binder = NameBinder::new();

    // Test position()
    let mut arena = AstArena::new();
    let func_id = make_function_call(&mut arena, "", "position", vec![]);
    let root = wrap_in_expr(&mut arena, func_id);

    bind_node(&mut arena, root, &ctx, &mut binder).unwrap();

    let mut dyn_ctx: DynamicContext<'_, RoXmlNavigator<'static>> =
        DynamicContext::new(&ctx, binder.len()).with_position(3, 10);

    let result = eval_node(&arena, root, &mut dyn_ctx).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_integer().map(|i| i.to_string()), Some("3".to_string()));
        }
        _ => panic!("Expected integer 3"),
    }

    // Test last()
    let mut arena = AstArena::new();
    let func_id = make_function_call(&mut arena, "", "last", vec![]);
    let root = wrap_in_expr(&mut arena, func_id);

    bind_node(&mut arena, root, &ctx, &mut binder).unwrap();

    let result = eval_node(&arena, root, &mut dyn_ctx).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(
                v.as_integer().map(|i| i.to_string()),
                Some("10".to_string())
            );
        }
        _ => panic!("Expected integer 10"),
    }
}

#[test]
fn test_eval_count() {
    // count(()) with empty sequence
    let mut arena = AstArena::new();
    let empty = arena.add(AstNode::Value(ValueNode::Empty));
    let func_id = make_function_call(&mut arena, "", "count", vec![empty]);
    let root = wrap_in_expr(&mut arena, func_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_integer().map(|i| i.to_string()), Some("0".to_string()));
        }
        _ => panic!("Expected integer 0"),
    }
}

#[test]
fn test_eval_empty_exists() {
    // Test empty(())
    let mut arena = AstArena::new();
    let empty = arena.add(AstNode::Value(ValueNode::Empty));
    let func_id = make_function_call(&mut arena, "", "empty", vec![empty]);
    let root = wrap_in_expr(&mut arena, func_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_boolean(), Some(true));
        }
        _ => panic!("Expected boolean true"),
    }

    // Test exists(())
    let mut arena = AstArena::new();
    let empty = arena.add(AstNode::Value(ValueNode::Empty));
    let func_id = make_function_call(&mut arena, "", "exists", vec![empty]);
    let root = wrap_in_expr(&mut arena, func_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_boolean(), Some(false));
        }
        _ => panic!("Expected boolean false"),
    }
}

#[test]
fn test_full_pipeline() {
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut binder = NameBinder::new();

    // Build AST for: concat('Hello, ', 'World!')
    let mut arena = AstArena::new();
    let arg1 = arena.add(AstNode::Value(ValueNode::String("Hello, ".to_string())));
    let arg2 = arena.add(AstNode::Value(ValueNode::String("World!".to_string())));
    let func_id = make_function_call(&mut arena, "", "concat", vec![arg1, arg2]);
    let root = wrap_in_expr(&mut arena, func_id);

    // Bind
    bind_node(&mut arena, root, &ctx, &mut binder).expect("bind failed");

    // Eval
    let mut dyn_ctx: DynamicContext<'_, RoXmlNavigator<'static>> =
        DynamicContext::new(&ctx, binder.len());
    let result = eval_node(&arena, root, &mut dyn_ctx).expect("eval failed");

    // Verify
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_string(), Some("Hello, World!"));
        }
        _ => panic!("Expected string 'Hello, World!'"),
    }
}

#[test]
fn test_eval_arithmetic_add() {
    let mut arena = AstArena::new();
    let left = arena.add(AstNode::Value(ValueNode::Integer("5".to_string())));
    let right = arena.add(AstNode::Value(ValueNode::Integer("3".to_string())));
    let span = SourceSpan::new(0, 5);
    let bin_op = BinaryOpNode::new(BinaryOpKind::Add, left, right, span);
    let bin_id = arena.add(AstNode::BinaryOp(bin_op));
    let root = wrap_in_expr(&mut arena, bin_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_integer().map(|i| i.to_string()), Some("8".to_string()));
        }
        _ => panic!("Expected integer 8"),
    }
}

#[test]
fn test_eval_arithmetic_sub() {
    let mut arena = AstArena::new();
    let left = arena.add(AstNode::Value(ValueNode::Integer("5".to_string())));
    let right = arena.add(AstNode::Value(ValueNode::Integer("3".to_string())));
    let span = SourceSpan::new(0, 5);
    let bin_op = BinaryOpNode::new(BinaryOpKind::Sub, left, right, span);
    let bin_id = arena.add(AstNode::BinaryOp(bin_op));
    let root = wrap_in_expr(&mut arena, bin_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_integer().map(|i| i.to_string()), Some("2".to_string()));
        }
        _ => panic!("Expected integer 2"),
    }
}

#[test]
fn test_eval_arithmetic_mul() {
    let mut arena = AstArena::new();
    let left = arena.add(AstNode::Value(ValueNode::Integer("5".to_string())));
    let right = arena.add(AstNode::Value(ValueNode::Integer("3".to_string())));
    let span = SourceSpan::new(0, 5);
    let bin_op = BinaryOpNode::new(BinaryOpKind::Mul, left, right, span);
    let bin_id = arena.add(AstNode::BinaryOp(bin_op));
    let root = wrap_in_expr(&mut arena, bin_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(
                v.as_integer().map(|i| i.to_string()),
                Some("15".to_string())
            );
        }
        _ => panic!("Expected integer 15"),
    }
}

#[test]
fn test_eval_logical_and_short_circuit() {
    let mut arena = AstArena::new();
    let left = arena.add(AstNode::Value(ValueNode::Boolean(false)));
    let right = arena.add(AstNode::Value(ValueNode::Boolean(true)));
    let span = SourceSpan::new(0, 10);
    let bin_op = BinaryOpNode::new(BinaryOpKind::And, left, right, span);
    let bin_id = arena.add(AstNode::BinaryOp(bin_op));
    let root = wrap_in_expr(&mut arena, bin_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_boolean(), Some(false));
        }
        _ => panic!("Expected boolean false"),
    }
}

#[test]
fn test_eval_logical_or_short_circuit() {
    let mut arena = AstArena::new();
    let left = arena.add(AstNode::Value(ValueNode::Boolean(true)));
    let right = arena.add(AstNode::Value(ValueNode::Boolean(false)));
    let span = SourceSpan::new(0, 10);
    let bin_op = BinaryOpNode::new(BinaryOpKind::Or, left, right, span);
    let bin_id = arena.add(AstNode::BinaryOp(bin_op));
    let root = wrap_in_expr(&mut arena, bin_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_boolean(), Some(true));
        }
        _ => panic!("Expected boolean true"),
    }
}

#[test]
fn test_unary_negate() {
    // -5 → -5
    let mut arena = AstArena::new();
    let val = arena.add(AstNode::Value(ValueNode::Integer("5".to_string())));
    let span = SourceSpan::new(0, 2);
    let unary_op = UnaryOpNode::new(UnaryOpKind::Negate, val, span);
    let unary_id = arena.add(AstNode::UnaryOp(unary_op));
    let root = wrap_in_expr(&mut arena, unary_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(
                v.as_integer().map(|i| i.to_string()),
                Some("-5".to_string())
            );
        }
        _ => panic!("Expected integer -5"),
    }

    // --5 → 5 (double negation)
    let mut arena = AstArena::new();
    let val = arena.add(AstNode::Value(ValueNode::Integer("5".to_string())));
    let span = SourceSpan::new(0, 3);
    let inner_unary = UnaryOpNode::new(UnaryOpKind::Negate, val, span);
    let inner_id = arena.add(AstNode::UnaryOp(inner_unary));
    let outer_unary = UnaryOpNode::new(UnaryOpKind::Negate, inner_id, span);
    let outer_id = arena.add(AstNode::UnaryOp(outer_unary));
    let root = wrap_in_expr(&mut arena, outer_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_integer().map(|i| i.to_string()), Some("5".to_string()));
        }
        _ => panic!("Expected integer 5"),
    }

    // -1.5 → -1.5 (double)
    let mut arena = AstArena::new();
    let val = arena.add(AstNode::Value(ValueNode::Double("1.5".to_string())));
    let span = SourceSpan::new(0, 4);
    let unary_op = UnaryOpNode::new(UnaryOpKind::Negate, val, span);
    let unary_id = arena.add(AstNode::UnaryOp(unary_op));
    let root = wrap_in_expr(&mut arena, unary_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            let d = v.as_double().expect("Expected double");
            assert!((d - (-1.5)).abs() < f64::EPSILON);
        }
        _ => panic!("Expected double -1.5"),
    }
}

#[test]
fn test_unary_identity() {
    // +5 → 5
    let mut arena = AstArena::new();
    let val = arena.add(AstNode::Value(ValueNode::Integer("5".to_string())));
    let span = SourceSpan::new(0, 2);
    let unary_op = UnaryOpNode::new(UnaryOpKind::Identity, val, span);
    let unary_id = arena.add(AstNode::UnaryOp(unary_op));
    let root = wrap_in_expr(&mut arena, unary_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_integer().map(|i| i.to_string()), Some("5".to_string()));
        }
        _ => panic!("Expected integer 5"),
    }

    // +-5 → -5 (identity then negate in value)
    let mut arena = AstArena::new();
    let val = arena.add(AstNode::Value(ValueNode::Integer("-5".to_string())));
    let span = SourceSpan::new(0, 3);
    let unary_op = UnaryOpNode::new(UnaryOpKind::Identity, val, span);
    let unary_id = arena.add(AstNode::UnaryOp(unary_op));
    let root = wrap_in_expr(&mut arena, unary_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(
                v.as_integer().map(|i| i.to_string()),
                Some("-5".to_string())
            );
        }
        _ => panic!("Expected integer -5"),
    }
}

#[test]
fn test_unary_empty_sequence() {
    // -() → ()
    let mut arena = AstArena::new();
    let val = arena.add(AstNode::Value(ValueNode::Empty));
    let span = SourceSpan::new(0, 3);
    let unary_op = UnaryOpNode::new(UnaryOpKind::Negate, val, span);
    let unary_id = arena.add(AstNode::UnaryOp(unary_op));
    let root = wrap_in_expr(&mut arena, unary_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    assert!(result.is_empty());
}

// ========================================================================
// General Comparison Operator Tests
// ========================================================================

#[test]
fn test_general_eq_single() {
    // 1 = 1 → true
    let mut arena = AstArena::new();
    let left = arena.add(AstNode::Value(ValueNode::Integer("1".to_string())));
    let right = arena.add(AstNode::Value(ValueNode::Integer("1".to_string())));
    let span = SourceSpan::new(0, 5);
    let bin_op = BinaryOpNode::new(BinaryOpKind::GeneralEq, left, right, span);
    let bin_id = arena.add(AstNode::BinaryOp(bin_op));
    let root = wrap_in_expr(&mut arena, bin_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_boolean(), Some(true));
        }
        _ => panic!("Expected boolean true"),
    }

    // 1 = 2 → false
    let mut arena = AstArena::new();
    let left = arena.add(AstNode::Value(ValueNode::Integer("1".to_string())));
    let right = arena.add(AstNode::Value(ValueNode::Integer("2".to_string())));
    let span = SourceSpan::new(0, 5);
    let bin_op = BinaryOpNode::new(BinaryOpKind::GeneralEq, left, right, span);
    let bin_id = arena.add(AstNode::BinaryOp(bin_op));
    let root = wrap_in_expr(&mut arena, bin_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_boolean(), Some(false));
        }
        _ => panic!("Expected boolean false"),
    }
}

#[test]
fn test_general_eq_sequence() {
    // (1, 2, 3) = 2 → true (exists a pair)
    let mut arena = AstArena::new();
    let v1 = arena.add(AstNode::Value(ValueNode::Integer("1".to_string())));
    let v2 = arena.add(AstNode::Value(ValueNode::Integer("2".to_string())));
    let v3 = arena.add(AstNode::Value(ValueNode::Integer("3".to_string())));
    let span = SourceSpan::new(0, 10);
    let left_seq = ExprNode::sequence(vec![v1, v2, v3], span);
    let left = arena.add(AstNode::Expr(left_seq));
    let right = arena.add(AstNode::Value(ValueNode::Integer("2".to_string())));
    let bin_op = BinaryOpNode::new(BinaryOpKind::GeneralEq, left, right, span);
    let bin_id = arena.add(AstNode::BinaryOp(bin_op));
    let root = wrap_in_expr(&mut arena, bin_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_boolean(), Some(true));
        }
        _ => panic!("Expected boolean true"),
    }

    // (1, 2, 3) = 4 → false
    let mut arena = AstArena::new();
    let v1 = arena.add(AstNode::Value(ValueNode::Integer("1".to_string())));
    let v2 = arena.add(AstNode::Value(ValueNode::Integer("2".to_string())));
    let v3 = arena.add(AstNode::Value(ValueNode::Integer("3".to_string())));
    let span = SourceSpan::new(0, 10);
    let left_seq = ExprNode::sequence(vec![v1, v2, v3], span);
    let left = arena.add(AstNode::Expr(left_seq));
    let right = arena.add(AstNode::Value(ValueNode::Integer("4".to_string())));
    let bin_op = BinaryOpNode::new(BinaryOpKind::GeneralEq, left, right, span);
    let bin_id = arena.add(AstNode::BinaryOp(bin_op));
    let root = wrap_in_expr(&mut arena, bin_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_boolean(), Some(false));
        }
        _ => panic!("Expected boolean false"),
    }
}

#[test]
fn test_general_ne() {
    // 1 != 2 → true
    let mut arena = AstArena::new();
    let left = arena.add(AstNode::Value(ValueNode::Integer("1".to_string())));
    let right = arena.add(AstNode::Value(ValueNode::Integer("2".to_string())));
    let span = SourceSpan::new(0, 6);
    let bin_op = BinaryOpNode::new(BinaryOpKind::GeneralNe, left, right, span);
    let bin_id = arena.add(AstNode::BinaryOp(bin_op));
    let root = wrap_in_expr(&mut arena, bin_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_boolean(), Some(true));
        }
        _ => panic!("Expected boolean true"),
    }

    // 1 != 1 → false
    let mut arena = AstArena::new();
    let left = arena.add(AstNode::Value(ValueNode::Integer("1".to_string())));
    let right = arena.add(AstNode::Value(ValueNode::Integer("1".to_string())));
    let span = SourceSpan::new(0, 6);
    let bin_op = BinaryOpNode::new(BinaryOpKind::GeneralNe, left, right, span);
    let bin_id = arena.add(AstNode::BinaryOp(bin_op));
    let root = wrap_in_expr(&mut arena, bin_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_boolean(), Some(false));
        }
        _ => panic!("Expected boolean false"),
    }
}

#[test]
fn test_general_lt() {
    // 1 < 2 → true
    let mut arena = AstArena::new();
    let left = arena.add(AstNode::Value(ValueNode::Integer("1".to_string())));
    let right = arena.add(AstNode::Value(ValueNode::Integer("2".to_string())));
    let span = SourceSpan::new(0, 5);
    let bin_op = BinaryOpNode::new(BinaryOpKind::GeneralLt, left, right, span);
    let bin_id = arena.add(AstNode::BinaryOp(bin_op));
    let root = wrap_in_expr(&mut arena, bin_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_boolean(), Some(true));
        }
        _ => panic!("Expected boolean true"),
    }

    // 2 < 1 → false
    let mut arena = AstArena::new();
    let left = arena.add(AstNode::Value(ValueNode::Integer("2".to_string())));
    let right = arena.add(AstNode::Value(ValueNode::Integer("1".to_string())));
    let span = SourceSpan::new(0, 5);
    let bin_op = BinaryOpNode::new(BinaryOpKind::GeneralLt, left, right, span);
    let bin_id = arena.add(AstNode::BinaryOp(bin_op));
    let root = wrap_in_expr(&mut arena, bin_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_boolean(), Some(false));
        }
        _ => panic!("Expected boolean false"),
    }
}

#[test]
fn test_general_le_gt_ge() {
    // 1 <= 2 → true
    let mut arena = AstArena::new();
    let left = arena.add(AstNode::Value(ValueNode::Integer("1".to_string())));
    let right = arena.add(AstNode::Value(ValueNode::Integer("2".to_string())));
    let span = SourceSpan::new(0, 6);
    let bin_op = BinaryOpNode::new(BinaryOpKind::GeneralLe, left, right, span);
    let bin_id = arena.add(AstNode::BinaryOp(bin_op));
    let root = wrap_in_expr(&mut arena, bin_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_boolean(), Some(true));
        }
        _ => panic!("Expected boolean true"),
    }

    // 2 > 1 → true
    let mut arena = AstArena::new();
    let left = arena.add(AstNode::Value(ValueNode::Integer("2".to_string())));
    let right = arena.add(AstNode::Value(ValueNode::Integer("1".to_string())));
    let span = SourceSpan::new(0, 5);
    let bin_op = BinaryOpNode::new(BinaryOpKind::GeneralGt, left, right, span);
    let bin_id = arena.add(AstNode::BinaryOp(bin_op));
    let root = wrap_in_expr(&mut arena, bin_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_boolean(), Some(true));
        }
        _ => panic!("Expected boolean true"),
    }

    // 2 >= 2 → true
    let mut arena = AstArena::new();
    let left = arena.add(AstNode::Value(ValueNode::Integer("2".to_string())));
    let right = arena.add(AstNode::Value(ValueNode::Integer("2".to_string())));
    let span = SourceSpan::new(0, 6);
    let bin_op = BinaryOpNode::new(BinaryOpKind::GeneralGe, left, right, span);
    let bin_id = arena.add(AstNode::BinaryOp(bin_op));
    let root = wrap_in_expr(&mut arena, bin_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_boolean(), Some(true));
        }
        _ => panic!("Expected boolean true"),
    }
}

#[test]
fn test_general_comparisons_empty() {
    // () = 1 → false
    let mut arena = AstArena::new();
    let left = arena.add(AstNode::Value(ValueNode::Empty));
    let right = arena.add(AstNode::Value(ValueNode::Integer("1".to_string())));
    let span = SourceSpan::new(0, 6);
    let bin_op = BinaryOpNode::new(BinaryOpKind::GeneralEq, left, right, span);
    let bin_id = arena.add(AstNode::BinaryOp(bin_op));
    let root = wrap_in_expr(&mut arena, bin_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_boolean(), Some(false));
        }
        _ => panic!("Expected boolean false"),
    }

    // 1 = () → false
    let mut arena = AstArena::new();
    let left = arena.add(AstNode::Value(ValueNode::Integer("1".to_string())));
    let right = arena.add(AstNode::Value(ValueNode::Empty));
    let span = SourceSpan::new(0, 6);
    let bin_op = BinaryOpNode::new(BinaryOpKind::GeneralEq, left, right, span);
    let bin_id = arena.add(AstNode::BinaryOp(bin_op));
    let root = wrap_in_expr(&mut arena, bin_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_boolean(), Some(false));
        }
        _ => panic!("Expected boolean false"),
    }
}

// ========================================================================
// Node Comparison Operator Tests
// ========================================================================

#[test]
fn test_node_is_same() {
    // Test that $node is $node → true
    // We use the context item for this test since we can set it up easily
    use crate::xpath::context::NameBinder;

    let doc = roxmltree::Document::parse("<root><a/><b/></root>").expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child(); // root
    nav.move_to_first_child(); // a

    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut binder = NameBinder::new();

    // Build AST for: . is .
    let mut arena = AstArena::new();
    let span = SourceSpan::new(0, 6);
    let left = arena.add(AstNode::ContextItem(
        crate::xpath::ast::ContextItemNode::new(span),
    ));
    let right = arena.add(AstNode::ContextItem(
        crate::xpath::ast::ContextItemNode::new(span),
    ));
    let bin_op = BinaryOpNode::new(BinaryOpKind::Is, left, right, span);
    let bin_id = arena.add(AstNode::BinaryOp(bin_op));
    let root = wrap_in_expr(&mut arena, bin_id);

    bind_node(&mut arena, root, &ctx, &mut binder).unwrap();

    let mut dyn_ctx =
        DynamicContext::new(&ctx, binder.len()).with_context_item(XmlItem::Node(nav.clone()));

    let result = eval_node(&arena, root, &mut dyn_ctx).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_boolean(), Some(true));
        }
        _ => panic!("Expected boolean true"),
    }
}

#[test]
fn test_node_is_empty() {
    // Test that () is $node → empty sequence
    use crate::xpath::context::NameBinder;

    let doc = roxmltree::Document::parse("<root/>").expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child(); // root

    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut binder = NameBinder::new();

    // Build AST for: () is .
    let mut arena = AstArena::new();
    let span = SourceSpan::new(0, 8);
    let left = arena.add(AstNode::Value(ValueNode::Empty));
    let right = arena.add(AstNode::ContextItem(
        crate::xpath::ast::ContextItemNode::new(span),
    ));
    let bin_op = BinaryOpNode::new(BinaryOpKind::Is, left, right, span);
    let bin_id = arena.add(AstNode::BinaryOp(bin_op));
    let root = wrap_in_expr(&mut arena, bin_id);

    bind_node(&mut arena, root, &ctx, &mut binder).unwrap();

    let mut dyn_ctx =
        DynamicContext::new(&ctx, binder.len()).with_context_item(XmlItem::Node(nav.clone()));

    let result = eval_node(&arena, root, &mut dyn_ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_node_is_type_error() {
    // Test that 1 is $node → type error (XPTY0004)
    use crate::xpath::context::NameBinder;

    let doc = roxmltree::Document::parse("<root/>").expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child(); // root

    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut binder = NameBinder::new();

    // Build AST for: 1 is .
    let mut arena = AstArena::new();
    let span = SourceSpan::new(0, 6);
    let left = arena.add(AstNode::Value(ValueNode::Integer("1".to_string())));
    let right = arena.add(AstNode::ContextItem(
        crate::xpath::ast::ContextItemNode::new(span),
    ));
    let bin_op = BinaryOpNode::new(BinaryOpKind::Is, left, right, span);
    let bin_id = arena.add(AstNode::BinaryOp(bin_op));
    let root = wrap_in_expr(&mut arena, bin_id);

    bind_node(&mut arena, root, &ctx, &mut binder).unwrap();

    let mut dyn_ctx =
        DynamicContext::new(&ctx, binder.len()).with_context_item(XmlItem::Node(nav.clone()));

    let result = eval_node(&arena, root, &mut dyn_ctx);
    assert!(matches!(result, Err(XPathError::XPTY0004 { .. })));
}

#[test]
fn test_node_before_after() {
    // Test << and >> operators with nodes from a document
    // /root/a << /root/b → true (a comes before b)
    // /root/b >> /root/a → true (b comes after a)
    use crate::xpath::bind::bind_node;
    use crate::xpath::context::NameBinder;
    use crate::xpath::parser;

    let doc = roxmltree::Document::parse("<root><a/><b/></root>").expect("parse xml");
    let nav = RoXmlNavigator::new(&doc);

    // Helper to eval xpath with document context
    let eval_with_doc = |expr: &str| -> Result<XPathValue<RoXmlNavigator<'_>>, XPathError> {
        let mut parsed =
            parser::parse(expr).map_err(|e| XPathError::syntax_error(e.to_string()))?;

        let names = NameTable::new();
        let ctx = XPathContext::new(&names);
        let mut binder = NameBinder::new();

        bind_node(&mut parsed.arena, parsed.root, &ctx, &mut binder)?;

        let mut dyn_ctx =
            DynamicContext::new(&ctx, binder.len()).with_context_item(XmlItem::Node(nav.clone()));

        eval_node(&parsed.arena, parsed.root, &mut dyn_ctx)
    };

    // Test: /root/a << /root/b → true (a comes before b in document order)
    let result = eval_with_doc("/root/a << /root/b").unwrap();
    assert_eq!(result.as_bool(), Some(true), "a << b should be true");

    // Test: /root/b >> /root/a → true (b comes after a in document order)
    let result = eval_with_doc("/root/b >> /root/a").unwrap();
    assert_eq!(result.as_bool(), Some(true), "b >> a should be true");

    // Test: /root/a >> /root/b → false (a does not come after b)
    let result = eval_with_doc("/root/a >> /root/b").unwrap();
    assert_eq!(result.as_bool(), Some(false), "a >> b should be false");

    // Test: /root/b << /root/a → false (b does not come before a)
    let result = eval_with_doc("/root/b << /root/a").unwrap();
    assert_eq!(result.as_bool(), Some(false), "b << a should be false");
}

// ========================================================================
// Sequence Operator Tests (Union, Intersect, Except)
// ========================================================================

#[test]
fn test_union_operator_with_atomic_values() {
    // Test union with atomic values (should fail with XPTY0004)
    // (1, 2) | (3, 4) → type error
    let mut arena = AstArena::new();
    let v1 = arena.add(AstNode::Value(ValueNode::Integer("1".to_string())));
    let v2 = arena.add(AstNode::Value(ValueNode::Integer("2".to_string())));
    let v3 = arena.add(AstNode::Value(ValueNode::Integer("3".to_string())));
    let v4 = arena.add(AstNode::Value(ValueNode::Integer("4".to_string())));
    let span = SourceSpan::new(0, 15);
    let left_seq = ExprNode::sequence(vec![v1, v2], span);
    let left = arena.add(AstNode::Expr(left_seq));
    let right_seq = ExprNode::sequence(vec![v3, v4], span);
    let right = arena.add(AstNode::Expr(right_seq));
    let bin_op = BinaryOpNode::new(BinaryOpKind::Union, left, right, span);
    let bin_id = arena.add(AstNode::BinaryOp(bin_op));
    let root = wrap_in_expr(&mut arena, bin_id);

    let result = bind_and_eval(&mut arena, root);
    assert!(result.is_err());
}

#[test]
fn test_intersect_operator_with_atomic_values() {
    // Test intersect with atomic values (should fail with XPTY0004)
    // (1, 2) intersect (2, 3) → type error
    let mut arena = AstArena::new();
    let v1 = arena.add(AstNode::Value(ValueNode::Integer("1".to_string())));
    let v2 = arena.add(AstNode::Value(ValueNode::Integer("2".to_string())));
    let v3 = arena.add(AstNode::Value(ValueNode::Integer("2".to_string())));
    let v4 = arena.add(AstNode::Value(ValueNode::Integer("3".to_string())));
    let span = SourceSpan::new(0, 20);
    let left_seq = ExprNode::sequence(vec![v1, v2], span);
    let left = arena.add(AstNode::Expr(left_seq));
    let right_seq = ExprNode::sequence(vec![v3, v4], span);
    let right = arena.add(AstNode::Expr(right_seq));
    let bin_op = BinaryOpNode::new(BinaryOpKind::Intersect, left, right, span);
    let bin_id = arena.add(AstNode::BinaryOp(bin_op));
    let root = wrap_in_expr(&mut arena, bin_id);

    let result = bind_and_eval(&mut arena, root);
    assert!(result.is_err());
}

#[test]
fn test_except_operator_with_atomic_values() {
    // Test except with atomic values (should fail with XPTY0004)
    // (1, 2) except (2, 3) → type error
    let mut arena = AstArena::new();
    let v1 = arena.add(AstNode::Value(ValueNode::Integer("1".to_string())));
    let v2 = arena.add(AstNode::Value(ValueNode::Integer("2".to_string())));
    let v3 = arena.add(AstNode::Value(ValueNode::Integer("2".to_string())));
    let v4 = arena.add(AstNode::Value(ValueNode::Integer("3".to_string())));
    let span = SourceSpan::new(0, 20);
    let left_seq = ExprNode::sequence(vec![v1, v2], span);
    let left = arena.add(AstNode::Expr(left_seq));
    let right_seq = ExprNode::sequence(vec![v3, v4], span);
    let right = arena.add(AstNode::Expr(right_seq));
    let bin_op = BinaryOpNode::new(BinaryOpKind::Except, left, right, span);
    let bin_id = arena.add(AstNode::BinaryOp(bin_op));
    let root = wrap_in_expr(&mut arena, bin_id);

    let result = bind_and_eval(&mut arena, root);
    assert!(result.is_err());
}

#[test]
fn test_union_operator_with_empty_sequences() {
    // Test union with empty sequences
    // () | () → ()
    let mut arena = AstArena::new();
    let left = arena.add(AstNode::Value(ValueNode::Empty));
    let right = arena.add(AstNode::Value(ValueNode::Empty));
    let span = SourceSpan::new(0, 6);
    let bin_op = BinaryOpNode::new(BinaryOpKind::Union, left, right, span);
    let bin_id = arena.add(AstNode::BinaryOp(bin_op));
    let root = wrap_in_expr(&mut arena, bin_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_union_operator_with_nodes() {
    // Test union with actual nodes
    use crate::xpath::context::NameBinder;

    let doc = roxmltree::Document::parse("<root><a/><b/></root>").expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child(); // root
    nav.move_to_first_child(); // a

    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut binder = NameBinder::new();

    // Build AST for: . | .
    // Same node union should return just one node
    let mut arena = AstArena::new();
    let span = SourceSpan::new(0, 6);
    let left = arena.add(AstNode::ContextItem(
        crate::xpath::ast::ContextItemNode::new(span),
    ));
    let right = arena.add(AstNode::ContextItem(
        crate::xpath::ast::ContextItemNode::new(span),
    ));
    let bin_op = BinaryOpNode::new(BinaryOpKind::Union, left, right, span);
    let bin_id = arena.add(AstNode::BinaryOp(bin_op));
    let root = wrap_in_expr(&mut arena, bin_id);

    bind_node(&mut arena, root, &ctx, &mut binder).unwrap();

    let mut dyn_ctx =
        DynamicContext::new(&ctx, binder.len()).with_context_item(XmlItem::Node(nav.clone()));

    let result = eval_node(&arena, root, &mut dyn_ctx).unwrap();
    // Union of same node with itself should give 1 node (deduplicated)
    assert_eq!(result.into_vec().len(), 1);
}

#[test]
fn test_intersect_operator_with_nodes() {
    // Test intersect with actual nodes
    use crate::xpath::context::NameBinder;

    let doc = roxmltree::Document::parse("<root><a/></root>").expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child(); // root
    nav.move_to_first_child(); // a

    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut binder = NameBinder::new();

    // Build AST for: . intersect .
    // Same node intersect should return that node
    let mut arena = AstArena::new();
    let span = SourceSpan::new(0, 15);
    let left = arena.add(AstNode::ContextItem(
        crate::xpath::ast::ContextItemNode::new(span),
    ));
    let right = arena.add(AstNode::ContextItem(
        crate::xpath::ast::ContextItemNode::new(span),
    ));
    let bin_op = BinaryOpNode::new(BinaryOpKind::Intersect, left, right, span);
    let bin_id = arena.add(AstNode::BinaryOp(bin_op));
    let root = wrap_in_expr(&mut arena, bin_id);

    bind_node(&mut arena, root, &ctx, &mut binder).unwrap();

    let mut dyn_ctx =
        DynamicContext::new(&ctx, binder.len()).with_context_item(XmlItem::Node(nav.clone()));

    let result = eval_node(&arena, root, &mut dyn_ctx).unwrap();
    // Intersect of same node with itself should give 1 node
    assert_eq!(result.into_vec().len(), 1);
}

#[test]
fn test_except_operator_with_nodes() {
    // Test except with actual nodes
    use crate::xpath::context::NameBinder;

    let doc = roxmltree::Document::parse("<root><a/></root>").expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child(); // root
    nav.move_to_first_child(); // a

    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut binder = NameBinder::new();

    // Build AST for: . except .
    // Same node except should return empty (node minus itself = empty)
    let mut arena = AstArena::new();
    let span = SourceSpan::new(0, 12);
    let left = arena.add(AstNode::ContextItem(
        crate::xpath::ast::ContextItemNode::new(span),
    ));
    let right = arena.add(AstNode::ContextItem(
        crate::xpath::ast::ContextItemNode::new(span),
    ));
    let bin_op = BinaryOpNode::new(BinaryOpKind::Except, left, right, span);
    let bin_id = arena.add(AstNode::BinaryOp(bin_op));
    let root = wrap_in_expr(&mut arena, bin_id);

    bind_node(&mut arena, root, &ctx, &mut binder).unwrap();

    let mut dyn_ctx =
        DynamicContext::new(&ctx, binder.len()).with_context_item(XmlItem::Node(nav.clone()));

    let result = eval_node(&arena, root, &mut dyn_ctx).unwrap();
    // Except of same node with itself should give empty sequence
    assert!(result.is_empty());
}

// ========================================================================
// Range Expression Tests
// ========================================================================

#[test]
fn test_range_basic() {
    // 1 to 5 -> (1, 2, 3, 4, 5)
    let mut arena = AstArena::new();
    let start = arena.add(AstNode::Value(ValueNode::Integer("1".to_string())));
    let end = arena.add(AstNode::Value(ValueNode::Integer("5".to_string())));
    let span = SourceSpan::new(0, 6);
    let range = RangeNode::new(start, end, span);
    let range_id = arena.add(AstNode::Range(range));
    let root = wrap_in_expr(&mut arena, range_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    let items = result.into_vec();
    assert_eq!(items.len(), 5);
    // Verify values are 1, 2, 3, 4, 5
    for (i, item) in items.iter().enumerate() {
        match item {
            XmlItem::Atomic(v) => {
                assert_eq!(
                    v.as_integer().map(|x| x.to_string()),
                    Some((i + 1).to_string())
                );
            }
            _ => panic!("Expected atomic integer"),
        }
    }
}

#[test]
fn test_range_empty() {
    // 5 to 3 -> () (empty sequence when start > end)
    let mut arena = AstArena::new();
    let start = arena.add(AstNode::Value(ValueNode::Integer("5".to_string())));
    let end = arena.add(AstNode::Value(ValueNode::Integer("3".to_string())));
    let span = SourceSpan::new(0, 6);
    let range = RangeNode::new(start, end, span);
    let range_id = arena.add(AstNode::Range(range));
    let root = wrap_in_expr(&mut arena, range_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_range_single() {
    // 5 to 5 -> (5)
    let mut arena = AstArena::new();
    let start = arena.add(AstNode::Value(ValueNode::Integer("5".to_string())));
    let end = arena.add(AstNode::Value(ValueNode::Integer("5".to_string())));
    let span = SourceSpan::new(0, 6);
    let range = RangeNode::new(start, end, span);
    let range_id = arena.add(AstNode::Range(range));
    let root = wrap_in_expr(&mut arena, range_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    let items = result.into_vec();
    assert_eq!(items.len(), 1);
    match &items[0] {
        XmlItem::Atomic(v) => {
            assert_eq!(v.as_integer().map(|x| x.to_string()), Some("5".to_string()));
        }
        _ => panic!("Expected atomic integer"),
    }
}

#[test]
fn test_range_empty_start_operand() {
    // () to 5 -> ()
    let mut arena = AstArena::new();
    let start = arena.add(AstNode::Value(ValueNode::Empty));
    let end = arena.add(AstNode::Value(ValueNode::Integer("5".to_string())));
    let span = SourceSpan::new(0, 7);
    let range = RangeNode::new(start, end, span);
    let range_id = arena.add(AstNode::Range(range));
    let root = wrap_in_expr(&mut arena, range_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_range_empty_end_operand() {
    // 1 to () -> ()
    let mut arena = AstArena::new();
    let start = arena.add(AstNode::Value(ValueNode::Integer("1".to_string())));
    let end = arena.add(AstNode::Value(ValueNode::Empty));
    let span = SourceSpan::new(0, 7);
    let range = RangeNode::new(start, end, span);
    let range_id = arena.add(AstNode::Range(range));
    let root = wrap_in_expr(&mut arena, range_id);

    let result = bind_and_eval(&mut arena, root).unwrap();
    assert!(result.is_empty());
}

// ========================================================================
// For Expression Tests
// ========================================================================

/// Helper to create a for expression with bindings and return expr
fn make_for_expr(
    arena: &mut AstArena,
    names: &NameTable,
    var_names: &[&str],
    in_exprs: Vec<AstNodeId>,
    return_expr: AstNodeId,
) -> AstNodeId {
    use crate::xpath::ast::ForBinding;

    let span = SourceSpan::new(0, 50);
    let bindings: Vec<ForBinding> = var_names
        .iter()
        .zip(in_exprs)
        .map(|(name, in_expr)| {
            let _ = names.add(name); // Ensure name is in table
            ForBinding::new(String::new(), name.to_string(), in_expr, span)
        })
        .collect();

    let for_node = crate::xpath::ast::ForNode::new(bindings, return_expr, span);
    arena.add(AstNode::For(for_node))
}

/// Helper to create a sequence of integers
fn make_int_sequence(arena: &mut AstArena, values: &[i64]) -> AstNodeId {
    let span = SourceSpan::new(0, 10);
    let items: Vec<AstNodeId> = values
        .iter()
        .map(|v| arena.add(AstNode::Value(ValueNode::Integer(v.to_string()))))
        .collect();
    let expr = ExprNode::sequence(items, span);
    arena.add(AstNode::Expr(expr))
}

/// Helper to create a variable reference
fn make_var_ref(arena: &mut AstArena, name: &str) -> AstNodeId {
    use crate::xpath::ast::VarRefNode;
    let span = SourceSpan::new(0, 5);
    let var_ref = VarRefNode::new(String::new(), name.to_string(), span);
    arena.add(AstNode::VarRef(var_ref))
}

#[test]
fn test_for_single_binding() {
    // for $i in (1, 2, 3) return $i
    // Expected: (1, 2, 3)
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut binder = NameBinder::new();

    let mut arena = AstArena::new();
    let seq = make_int_sequence(&mut arena, &[1, 2, 3]);
    let var_ref = make_var_ref(&mut arena, "i");
    let for_id = make_for_expr(&mut arena, &names, &["i"], vec![seq], var_ref);
    let root = wrap_in_expr(&mut arena, for_id);

    bind_node(&mut arena, root, &ctx, &mut binder).unwrap();
    let mut dyn_ctx: DynamicContext<'_, RoXmlNavigator<'static>> =
        DynamicContext::new(&ctx, binder.len());

    let result = eval_node(&arena, root, &mut dyn_ctx).unwrap();
    let items = result.into_vec();
    assert_eq!(items.len(), 3);

    // Verify values are 1, 2, 3
    for (i, item) in items.iter().enumerate() {
        match item {
            XmlItem::Atomic(v) => {
                assert_eq!(
                    v.as_integer().map(|x| x.to_string()),
                    Some((i as i64 + 1).to_string())
                );
            }
            _ => panic!("Expected atomic integer"),
        }
    }
}

#[test]
fn test_for_empty_sequence() {
    // for $i in () return $i
    // Expected: ()
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut binder = NameBinder::new();

    let mut arena = AstArena::new();
    let empty = arena.add(AstNode::Value(ValueNode::Empty));
    let var_ref = make_var_ref(&mut arena, "i");
    let for_id = make_for_expr(&mut arena, &names, &["i"], vec![empty], var_ref);
    let root = wrap_in_expr(&mut arena, for_id);

    bind_node(&mut arena, root, &ctx, &mut binder).unwrap();
    let mut dyn_ctx: DynamicContext<'_, RoXmlNavigator<'static>> =
        DynamicContext::new(&ctx, binder.len());

    let result = eval_node(&arena, root, &mut dyn_ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_for_multiple_bindings() {
    // for $i in (1, 2), $j in (10, 20) return $i + $j
    // Expected: (11, 21, 12, 22) - Cartesian product order
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut binder = NameBinder::new();

    let mut arena = AstArena::new();
    let seq1 = make_int_sequence(&mut arena, &[1, 2]);
    let seq2 = make_int_sequence(&mut arena, &[10, 20]);
    let var_i = make_var_ref(&mut arena, "i");
    let var_j = make_var_ref(&mut arena, "j");
    let span = SourceSpan::new(0, 10);
    let add = BinaryOpNode::new(BinaryOpKind::Add, var_i, var_j, span);
    let add_id = arena.add(AstNode::BinaryOp(add));
    let for_id = make_for_expr(&mut arena, &names, &["i", "j"], vec![seq1, seq2], add_id);
    let root = wrap_in_expr(&mut arena, for_id);

    bind_node(&mut arena, root, &ctx, &mut binder).unwrap();
    let mut dyn_ctx: DynamicContext<'_, RoXmlNavigator<'static>> =
        DynamicContext::new(&ctx, binder.len());

    let result = eval_node(&arena, root, &mut dyn_ctx).unwrap();
    let items = result.into_vec();
    assert_eq!(items.len(), 4);

    // Verify values: 1+10=11, 1+20=21, 2+10=12, 2+20=22
    let expected = [11i64, 21, 12, 22];
    for (i, item) in items.iter().enumerate() {
        match item {
            XmlItem::Atomic(v) => {
                assert_eq!(
                    v.as_integer().map(|x| x.to_string()),
                    Some(expected[i].to_string())
                );
            }
            _ => panic!("Expected atomic integer"),
        }
    }
}

#[test]
fn test_for_return_empty() {
    // for $i in (1, 2, 3) return ()
    // Expected: ()
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut binder = NameBinder::new();

    let mut arena = AstArena::new();
    let seq = make_int_sequence(&mut arena, &[1, 2, 3]);
    let empty = arena.add(AstNode::Value(ValueNode::Empty));
    let for_id = make_for_expr(&mut arena, &names, &["i"], vec![seq], empty);
    let root = wrap_in_expr(&mut arena, for_id);

    bind_node(&mut arena, root, &ctx, &mut binder).unwrap();
    let mut dyn_ctx: DynamicContext<'_, RoXmlNavigator<'static>> =
        DynamicContext::new(&ctx, binder.len());

    let result = eval_node(&arena, root, &mut dyn_ctx).unwrap();
    assert!(result.is_empty());
}

// ========================================================================
// Quantified Expression Tests
// ========================================================================

/// Helper to create a quantified expression
fn make_quantified_expr(
    arena: &mut AstArena,
    names: &NameTable,
    kind: QuantifierKind,
    var_names: &[&str],
    in_exprs: Vec<AstNodeId>,
    satisfies: AstNodeId,
) -> AstNodeId {
    use crate::xpath::ast::ForBinding;

    let span = SourceSpan::new(0, 50);
    let bindings: Vec<ForBinding> = var_names
        .iter()
        .zip(in_exprs)
        .map(|(name, in_expr)| {
            let _ = names.add(name); // Ensure name is in table
            ForBinding::new(String::new(), name.to_string(), in_expr, span)
        })
        .collect();

    let quant_node = crate::xpath::ast::QuantifiedNode::new(kind, bindings, satisfies, span);
    arena.add(AstNode::Quantified(quant_node))
}

#[test]
fn test_some_true() {
    // some $x in (1, 2, 3) satisfies $x > 2
    // Expected: true (3 > 2)
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut binder = NameBinder::new();

    let mut arena = AstArena::new();
    let seq = make_int_sequence(&mut arena, &[1, 2, 3]);
    let var_x = make_var_ref(&mut arena, "x");
    let two = arena.add(AstNode::Value(ValueNode::Integer("2".to_string())));
    let span = SourceSpan::new(0, 10);
    let gt = BinaryOpNode::new(BinaryOpKind::GeneralGt, var_x, two, span);
    let gt_id = arena.add(AstNode::BinaryOp(gt));
    let quant_id = make_quantified_expr(
        &mut arena,
        &names,
        QuantifierKind::Some,
        &["x"],
        vec![seq],
        gt_id,
    );
    let root = wrap_in_expr(&mut arena, quant_id);

    bind_node(&mut arena, root, &ctx, &mut binder).unwrap();
    let mut dyn_ctx: DynamicContext<'_, RoXmlNavigator<'static>> =
        DynamicContext::new(&ctx, binder.len());

    let result = eval_node(&arena, root, &mut dyn_ctx).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_boolean(), Some(true));
        }
        _ => panic!("Expected boolean true"),
    }
}

#[test]
fn test_some_false() {
    // some $x in (1, 2, 3) satisfies $x > 5
    // Expected: false
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut binder = NameBinder::new();

    let mut arena = AstArena::new();
    let seq = make_int_sequence(&mut arena, &[1, 2, 3]);
    let var_x = make_var_ref(&mut arena, "x");
    let five = arena.add(AstNode::Value(ValueNode::Integer("5".to_string())));
    let span = SourceSpan::new(0, 10);
    let gt = BinaryOpNode::new(BinaryOpKind::GeneralGt, var_x, five, span);
    let gt_id = arena.add(AstNode::BinaryOp(gt));
    let quant_id = make_quantified_expr(
        &mut arena,
        &names,
        QuantifierKind::Some,
        &["x"],
        vec![seq],
        gt_id,
    );
    let root = wrap_in_expr(&mut arena, quant_id);

    bind_node(&mut arena, root, &ctx, &mut binder).unwrap();
    let mut dyn_ctx: DynamicContext<'_, RoXmlNavigator<'static>> =
        DynamicContext::new(&ctx, binder.len());

    let result = eval_node(&arena, root, &mut dyn_ctx).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_boolean(), Some(false));
        }
        _ => panic!("Expected boolean false"),
    }
}

#[test]
fn test_some_empty_sequence() {
    // some $x in () satisfies $x > 0
    // Expected: false (no items to test)
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut binder = NameBinder::new();

    let mut arena = AstArena::new();
    let empty = arena.add(AstNode::Value(ValueNode::Empty));
    let var_x = make_var_ref(&mut arena, "x");
    let zero = arena.add(AstNode::Value(ValueNode::Integer("0".to_string())));
    let span = SourceSpan::new(0, 10);
    let gt = BinaryOpNode::new(BinaryOpKind::GeneralGt, var_x, zero, span);
    let gt_id = arena.add(AstNode::BinaryOp(gt));
    let quant_id = make_quantified_expr(
        &mut arena,
        &names,
        QuantifierKind::Some,
        &["x"],
        vec![empty],
        gt_id,
    );
    let root = wrap_in_expr(&mut arena, quant_id);

    bind_node(&mut arena, root, &ctx, &mut binder).unwrap();
    let mut dyn_ctx: DynamicContext<'_, RoXmlNavigator<'static>> =
        DynamicContext::new(&ctx, binder.len());

    let result = eval_node(&arena, root, &mut dyn_ctx).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_boolean(), Some(false));
        }
        _ => panic!("Expected boolean false"),
    }
}

#[test]
fn test_every_true() {
    // every $x in (1, 2, 3) satisfies $x > 0
    // Expected: true
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut binder = NameBinder::new();

    let mut arena = AstArena::new();
    let seq = make_int_sequence(&mut arena, &[1, 2, 3]);
    let var_x = make_var_ref(&mut arena, "x");
    let zero = arena.add(AstNode::Value(ValueNode::Integer("0".to_string())));
    let span = SourceSpan::new(0, 10);
    let gt = BinaryOpNode::new(BinaryOpKind::GeneralGt, var_x, zero, span);
    let gt_id = arena.add(AstNode::BinaryOp(gt));
    let quant_id = make_quantified_expr(
        &mut arena,
        &names,
        QuantifierKind::Every,
        &["x"],
        vec![seq],
        gt_id,
    );
    let root = wrap_in_expr(&mut arena, quant_id);

    bind_node(&mut arena, root, &ctx, &mut binder).unwrap();
    let mut dyn_ctx: DynamicContext<'_, RoXmlNavigator<'static>> =
        DynamicContext::new(&ctx, binder.len());

    let result = eval_node(&arena, root, &mut dyn_ctx).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_boolean(), Some(true));
        }
        _ => panic!("Expected boolean true"),
    }
}

#[test]
fn test_every_false() {
    // every $x in (1, 2, 3) satisfies $x > 2
    // Expected: false (1 and 2 are not > 2)
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut binder = NameBinder::new();

    let mut arena = AstArena::new();
    let seq = make_int_sequence(&mut arena, &[1, 2, 3]);
    let var_x = make_var_ref(&mut arena, "x");
    let two = arena.add(AstNode::Value(ValueNode::Integer("2".to_string())));
    let span = SourceSpan::new(0, 10);
    let gt = BinaryOpNode::new(BinaryOpKind::GeneralGt, var_x, two, span);
    let gt_id = arena.add(AstNode::BinaryOp(gt));
    let quant_id = make_quantified_expr(
        &mut arena,
        &names,
        QuantifierKind::Every,
        &["x"],
        vec![seq],
        gt_id,
    );
    let root = wrap_in_expr(&mut arena, quant_id);

    bind_node(&mut arena, root, &ctx, &mut binder).unwrap();
    let mut dyn_ctx: DynamicContext<'_, RoXmlNavigator<'static>> =
        DynamicContext::new(&ctx, binder.len());

    let result = eval_node(&arena, root, &mut dyn_ctx).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_boolean(), Some(false));
        }
        _ => panic!("Expected boolean false"),
    }
}

#[test]
fn test_every_empty_vacuous_truth() {
    // every $x in () satisfies $x > 0
    // Expected: true (vacuous truth)
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut binder = NameBinder::new();

    let mut arena = AstArena::new();
    let empty = arena.add(AstNode::Value(ValueNode::Empty));
    let var_x = make_var_ref(&mut arena, "x");
    let zero = arena.add(AstNode::Value(ValueNode::Integer("0".to_string())));
    let span = SourceSpan::new(0, 10);
    let gt = BinaryOpNode::new(BinaryOpKind::GeneralGt, var_x, zero, span);
    let gt_id = arena.add(AstNode::BinaryOp(gt));
    let quant_id = make_quantified_expr(
        &mut arena,
        &names,
        QuantifierKind::Every,
        &["x"],
        vec![empty],
        gt_id,
    );
    let root = wrap_in_expr(&mut arena, quant_id);

    bind_node(&mut arena, root, &ctx, &mut binder).unwrap();
    let mut dyn_ctx: DynamicContext<'_, RoXmlNavigator<'static>> =
        DynamicContext::new(&ctx, binder.len());

    let result = eval_node(&arena, root, &mut dyn_ctx).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_boolean(), Some(true));
        }
        _ => panic!("Expected boolean true (vacuous truth)"),
    }
}

#[test]
fn test_some_multiple_bindings() {
    // some $x in (1, 2), $y in (3, 4) satisfies $x + $y = 5
    // Expected: true (1+4=5 or 2+3=5)
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut binder = NameBinder::new();

    let mut arena = AstArena::new();
    let seq1 = make_int_sequence(&mut arena, &[1, 2]);
    let seq2 = make_int_sequence(&mut arena, &[3, 4]);
    let var_x = make_var_ref(&mut arena, "x");
    let var_y = make_var_ref(&mut arena, "y");
    let five = arena.add(AstNode::Value(ValueNode::Integer("5".to_string())));
    let span = SourceSpan::new(0, 20);
    let add = BinaryOpNode::new(BinaryOpKind::Add, var_x, var_y, span);
    let add_id = arena.add(AstNode::BinaryOp(add));
    let eq = BinaryOpNode::new(BinaryOpKind::GeneralEq, add_id, five, span);
    let eq_id = arena.add(AstNode::BinaryOp(eq));
    let quant_id = make_quantified_expr(
        &mut arena,
        &names,
        QuantifierKind::Some,
        &["x", "y"],
        vec![seq1, seq2],
        eq_id,
    );
    let root = wrap_in_expr(&mut arena, quant_id);

    bind_node(&mut arena, root, &ctx, &mut binder).unwrap();
    let mut dyn_ctx: DynamicContext<'_, RoXmlNavigator<'static>> =
        DynamicContext::new(&ctx, binder.len());

    let result = eval_node(&arena, root, &mut dyn_ctx).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_boolean(), Some(true));
        }
        _ => panic!("Expected boolean true"),
    }
}

// ========================================================================
// Dependent Binding Tests (for/quantified with dependent bindings)
// ========================================================================

/// Helper to create a for expression with dependent bindings.
/// The second binding's in_expr is a function of the first binding's variable.
fn make_dependent_for_expr(
    arena: &mut AstArena,
    names: &NameTable,
    var1_name: &str,
    in_expr1: AstNodeId,
    var2_name: &str,
    in_expr2: AstNodeId, // Can reference var1
    return_expr: AstNodeId,
) -> AstNodeId {
    use crate::xpath::ast::ForBinding;

    let span = SourceSpan::new(0, 50);
    let _ = names.add(var1_name);
    let _ = names.add(var2_name);

    let bindings = vec![
        ForBinding::new(String::new(), var1_name.to_string(), in_expr1, span),
        ForBinding::new(String::new(), var2_name.to_string(), in_expr2, span),
    ];

    let for_node = crate::xpath::ast::ForNode::new(bindings, return_expr, span);
    arena.add(AstNode::For(for_node))
}

#[test]
fn test_for_dependent_bindings() {
    // for $x in (1, 2, 3), $y in ($x + 1) return $y
    // Expected: (2, 3, 4) - $y depends on $x
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut binder = NameBinder::new();

    let mut arena = AstArena::new();
    let seq1 = make_int_sequence(&mut arena, &[1, 2, 3]);

    // $y in ($x + 1) - create addition expression
    let var_x_in_expr = make_var_ref(&mut arena, "x");
    let one = arena.add(AstNode::Value(ValueNode::Integer("1".to_string())));
    let span = SourceSpan::new(0, 10);
    let add = BinaryOpNode::new(BinaryOpKind::Add, var_x_in_expr, one, span);
    let add_id = arena.add(AstNode::BinaryOp(add));

    // return $y
    let var_y = make_var_ref(&mut arena, "y");

    let for_id = make_dependent_for_expr(&mut arena, &names, "x", seq1, "y", add_id, var_y);
    let root = wrap_in_expr(&mut arena, for_id);

    bind_node(&mut arena, root, &ctx, &mut binder).unwrap();
    let mut dyn_ctx: DynamicContext<'_, RoXmlNavigator<'static>> =
        DynamicContext::new(&ctx, binder.len());

    let result = eval_node(&arena, root, &mut dyn_ctx).unwrap();
    let items = result.into_vec();
    assert_eq!(items.len(), 3);

    // Verify values are 2, 3, 4 (corresponding to $x+1 for $x in 1,2,3)
    let expected = [2i64, 3, 4];
    for (i, item) in items.iter().enumerate() {
        match item {
            XmlItem::Atomic(v) => {
                assert_eq!(
                    v.as_integer().map(|x| x.to_string()),
                    Some(expected[i].to_string()),
                    "Expected {} at index {}",
                    expected[i],
                    i
                );
            }
            _ => panic!("Expected atomic integer at index {}", i),
        }
    }
}

#[test]
fn test_for_dependent_range_binding() {
    // for $x in (1 to 3), $y in (1 to $x) return $y
    // When $x=1: $y in (1) -> 1
    // When $x=2: $y in (1,2) -> 1, 2
    // When $x=3: $y in (1,2,3) -> 1, 2, 3
    // Expected: (1, 1, 2, 1, 2, 3)
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut binder = NameBinder::new();

    let mut arena = AstArena::new();

    // $x in (1 to 3)
    let one_a = arena.add(AstNode::Value(ValueNode::Integer("1".to_string())));
    let three = arena.add(AstNode::Value(ValueNode::Integer("3".to_string())));
    let span = SourceSpan::new(0, 10);
    let range1 = RangeNode::new(one_a, three, span);
    let range1_id = arena.add(AstNode::Range(range1));

    // $y in (1 to $x) - range depends on $x
    let one_b = arena.add(AstNode::Value(ValueNode::Integer("1".to_string())));
    let var_x_in_expr = make_var_ref(&mut arena, "x");
    let range2 = RangeNode::new(one_b, var_x_in_expr, span);
    let range2_id = arena.add(AstNode::Range(range2));

    // return $y
    let var_y = make_var_ref(&mut arena, "y");

    let for_id = make_dependent_for_expr(&mut arena, &names, "x", range1_id, "y", range2_id, var_y);
    let root = wrap_in_expr(&mut arena, for_id);

    bind_node(&mut arena, root, &ctx, &mut binder).unwrap();
    let mut dyn_ctx: DynamicContext<'_, RoXmlNavigator<'static>> =
        DynamicContext::new(&ctx, binder.len());

    let result = eval_node(&arena, root, &mut dyn_ctx).unwrap();
    let items = result.into_vec();

    // Expected: (1, 1, 2, 1, 2, 3)
    let expected = [1i64, 1, 2, 1, 2, 3];
    assert_eq!(items.len(), expected.len());

    for (i, item) in items.iter().enumerate() {
        match item {
            XmlItem::Atomic(v) => {
                assert_eq!(
                    v.as_integer().map(|x| x.to_string()),
                    Some(expected[i].to_string()),
                    "Expected {} at index {}",
                    expected[i],
                    i
                );
            }
            _ => panic!("Expected atomic integer at index {}", i),
        }
    }
}

#[test]
fn test_some_dependent_bindings() {
    // some $x in (1, 2), $y in ($x * 2) satisfies $y > 3
    // When $x=1: $y=2, 2>3 is false
    // When $x=2: $y=4, 4>3 is true -> short-circuit, return true
    // Expected: true
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut binder = NameBinder::new();

    let mut arena = AstArena::new();
    let seq = make_int_sequence(&mut arena, &[1, 2]);

    // $y in ($x * 2)
    let var_x_in_expr = make_var_ref(&mut arena, "x");
    let two = arena.add(AstNode::Value(ValueNode::Integer("2".to_string())));
    let span = SourceSpan::new(0, 10);
    let mul = BinaryOpNode::new(BinaryOpKind::Mul, var_x_in_expr, two, span);
    let mul_id = arena.add(AstNode::BinaryOp(mul));

    // satisfies $y > 3
    let var_y = make_var_ref(&mut arena, "y");
    let three = arena.add(AstNode::Value(ValueNode::Integer("3".to_string())));
    let gt = BinaryOpNode::new(BinaryOpKind::GeneralGt, var_y, three, span);
    let gt_id = arena.add(AstNode::BinaryOp(gt));

    // Build bindings manually for quantified expression
    use crate::xpath::ast::ForBinding;
    let _ = names.add("x");
    let _ = names.add("y");
    let bindings = vec![
        ForBinding::new(String::new(), "x".to_string(), seq, span),
        ForBinding::new(String::new(), "y".to_string(), mul_id, span),
    ];
    let quant_node =
        crate::xpath::ast::QuantifiedNode::new(QuantifierKind::Some, bindings, gt_id, span);
    let quant_id = arena.add(AstNode::Quantified(quant_node));
    let root = wrap_in_expr(&mut arena, quant_id);

    bind_node(&mut arena, root, &ctx, &mut binder).unwrap();
    let mut dyn_ctx: DynamicContext<'_, RoXmlNavigator<'static>> =
        DynamicContext::new(&ctx, binder.len());

    let result = eval_node(&arena, root, &mut dyn_ctx).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_boolean(), Some(true));
        }
        _ => panic!("Expected boolean true"),
    }
}

#[test]
fn test_every_dependent_bindings() {
    // every $x in (1, 2), $y in (1 to $x) satisfies $y <= $x
    // When $x=1: $y in (1), 1 <= 1 is true
    // When $x=2: $y in (1,2), 1 <= 2 is true, 2 <= 2 is true
    // All satisfied -> true
    // Expected: true
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut binder = NameBinder::new();

    let mut arena = AstArena::new();
    let seq = make_int_sequence(&mut arena, &[1, 2]);

    // $y in (1 to $x)
    let one = arena.add(AstNode::Value(ValueNode::Integer("1".to_string())));
    let var_x_in_expr = make_var_ref(&mut arena, "x");
    let span = SourceSpan::new(0, 10);
    let range = RangeNode::new(one, var_x_in_expr, span);
    let range_id = arena.add(AstNode::Range(range));

    // satisfies $y <= $x
    let var_y = make_var_ref(&mut arena, "y");
    let var_x = make_var_ref(&mut arena, "x");
    let le = BinaryOpNode::new(BinaryOpKind::GeneralLe, var_y, var_x, span);
    let le_id = arena.add(AstNode::BinaryOp(le));

    // Build bindings manually for quantified expression
    use crate::xpath::ast::ForBinding;
    let _ = names.add("x");
    let _ = names.add("y");
    let bindings = vec![
        ForBinding::new(String::new(), "x".to_string(), seq, span),
        ForBinding::new(String::new(), "y".to_string(), range_id, span),
    ];
    let quant_node =
        crate::xpath::ast::QuantifiedNode::new(QuantifierKind::Every, bindings, le_id, span);
    let quant_id = arena.add(AstNode::Quantified(quant_node));
    let root = wrap_in_expr(&mut arena, quant_id);

    bind_node(&mut arena, root, &ctx, &mut binder).unwrap();
    let mut dyn_ctx: DynamicContext<'_, RoXmlNavigator<'static>> =
        DynamicContext::new(&ctx, binder.len());

    let result = eval_node(&arena, root, &mut dyn_ctx).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_boolean(), Some(true));
        }
        _ => panic!("Expected boolean true"),
    }
}

#[test]
fn test_every_dependent_bindings_fails() {
    // every $x in (1, 2, 3), $y in (1 to $x) satisfies $y < $x
    // When $x=1: $y in (1), 1 < 1 is FALSE -> short-circuit, return false
    // Expected: false
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let mut binder = NameBinder::new();

    let mut arena = AstArena::new();
    let seq = make_int_sequence(&mut arena, &[1, 2, 3]);

    // $y in (1 to $x)
    let one = arena.add(AstNode::Value(ValueNode::Integer("1".to_string())));
    let var_x_in_expr = make_var_ref(&mut arena, "x");
    let span = SourceSpan::new(0, 10);
    let range = RangeNode::new(one, var_x_in_expr, span);
    let range_id = arena.add(AstNode::Range(range));

    // satisfies $y < $x (strictly less than)
    let var_y = make_var_ref(&mut arena, "y");
    let var_x = make_var_ref(&mut arena, "x");
    let lt = BinaryOpNode::new(BinaryOpKind::GeneralLt, var_y, var_x, span);
    let lt_id = arena.add(AstNode::BinaryOp(lt));

    // Build bindings manually for quantified expression
    use crate::xpath::ast::ForBinding;
    let _ = names.add("x");
    let _ = names.add("y");
    let bindings = vec![
        ForBinding::new(String::new(), "x".to_string(), seq, span),
        ForBinding::new(String::new(), "y".to_string(), range_id, span),
    ];
    let quant_node =
        crate::xpath::ast::QuantifiedNode::new(QuantifierKind::Every, bindings, lt_id, span);
    let quant_id = arena.add(AstNode::Quantified(quant_node));
    let root = wrap_in_expr(&mut arena, quant_id);

    bind_node(&mut arena, root, &ctx, &mut binder).unwrap();
    let mut dyn_ctx: DynamicContext<'_, RoXmlNavigator<'static>> =
        DynamicContext::new(&ctx, binder.len());

    let result = eval_node(&arena, root, &mut dyn_ctx).unwrap();
    match result {
        XPathValue::Item(XmlItem::Atomic(v)) => {
            assert_eq!(v.as_boolean(), Some(false));
        }
        _ => panic!("Expected boolean false"),
    }
}

// ============================================================================
// Integration Tests (Parse -> Bind -> Eval)
// ============================================================================

#[cfg(test)]
mod integration_tests {
    //! Integration tests for the full parse -> bind -> eval pipeline.

    use crate::namespace::table::NameTable;
    use crate::xpath::bind::bind_node;
    use crate::xpath::context::{DynamicContext, NameBinder, XPathContext};
    use crate::xpath::error::XPathError;
    use crate::xpath::eval::eval_node;
    use crate::xpath::functions::XPathValue;
    use crate::xpath::parser;
    use crate::xpath::RoXmlNavigator;

    /// Helper to parse, bind, and evaluate an XPath expression without context item
    fn eval_xpath(expr: &str) -> Result<XPathValue<RoXmlNavigator<'static>>, XPathError> {
        let mut parsed =
            parser::parse(expr).map_err(|e| XPathError::syntax_error(e.to_string()))?;

        let names = NameTable::new();
        let ctx = XPathContext::new(&names);
        let mut binder = NameBinder::new();

        bind_node(&mut parsed.arena, parsed.root, &ctx, &mut binder)?;

        let mut dyn_ctx: DynamicContext<'_, RoXmlNavigator<'static>> =
            DynamicContext::new(&ctx, binder.len());

        eval_node(&parsed.arena, parsed.root, &mut dyn_ctx)
    }

    /// Helper to parse, bind, and evaluate with a variable bound
    fn eval_xpath_with_var(
        expr: &str,
        var_name: &str,
        var_value: i64,
    ) -> Result<XPathValue<RoXmlNavigator<'static>>, XPathError> {
        use crate::namespace::qname::QualifiedName;
        use crate::types::value::XmlValue;
        use crate::xpath::iterator::XmlItem;

        let mut parsed =
            parser::parse(expr).map_err(|e| XPathError::syntax_error(e.to_string()))?;

        let names = NameTable::new();
        let ctx = XPathContext::new(&names);
        let mut binder = NameBinder::new();

        // Push the variable into scope before binding
        let var_name_id = names.add(var_name);
        let qname = QualifiedName::local(var_name_id);
        let var_ref = binder.push_var(qname);

        bind_node(&mut parsed.arena, parsed.root, &ctx, &mut binder)?;

        let mut dyn_ctx: DynamicContext<'_, RoXmlNavigator<'static>> =
            DynamicContext::new(&ctx, binder.len());

        // Set the variable value
        dyn_ctx.set_variable(
            var_ref.slot,
            XPathValue::Item(XmlItem::Atomic(XmlValue::integer(
                num_bigint::BigInt::from(var_value),
            ))),
        );

        eval_node(&parsed.arena, parsed.root, &mut dyn_ctx)
    }

    #[test]
    fn test_parse_bind_eval_if_expression() {
        let result = eval_xpath("if (true()) then 'yes' else 'no'").unwrap();
        assert_eq!(result.to_owned_string(), Some("yes".to_string()));
    }

    #[test]
    fn test_parse_bind_eval_nested_functions() {
        let result = eval_xpath("upper-case(concat('a', 'b'))").unwrap();
        assert_eq!(result.to_owned_string(), Some("AB".to_string()));
    }

    #[test]
    fn test_parse_bind_eval_variable_reference() {
        let result = eval_xpath_with_var("$x + 1", "x", 5).unwrap();
        assert_eq!(
            result.as_integer().map(|i| i.to_string()),
            Some("6".to_string())
        );
    }

    #[test]
    fn test_parse_bind_eval_comparison() {
        assert_eq!(eval_xpath("1 = 1").unwrap().as_bool(), Some(true));
        assert_eq!(eval_xpath("1 = 2").unwrap().as_bool(), Some(false));
        assert_eq!(eval_xpath("1 < 2").unwrap().as_bool(), Some(true));
    }

    #[test]
    fn test_parse_bind_eval_arithmetic() {
        assert_eq!(
            eval_xpath("1 + 2")
                .unwrap()
                .as_integer()
                .map(|i| i.to_string()),
            Some("3".to_string())
        );
        assert_eq!(
            eval_xpath("5 - 3")
                .unwrap()
                .as_integer()
                .map(|i| i.to_string()),
            Some("2".to_string())
        );
        assert_eq!(
            eval_xpath("2 * 3")
                .unwrap()
                .as_integer()
                .map(|i| i.to_string()),
            Some("6".to_string())
        );
    }
}

// ========================================================================
// Type Expression Tests (instance of, treat as, cast as, castable as)
// ========================================================================

mod type_expr_tests {
    use crate::namespace::context::NamespaceContextSnapshot;
    use crate::namespace::table::well_known;
    use crate::namespace::table::NameTable;
    use crate::xpath::arena::{AstArena, AstNodeId, SourceSpan};
    use crate::xpath::ast::{
        AstNode, ExprNode, ItemTypeNode as AstItemTypeNode, OccurrenceIndicator, QName as AstQName,
        SequenceTypeNode, TypeExprKind, TypeExprNode, ValueNode,
    };
    use crate::xpath::bind::bind_node;
    use crate::xpath::context::{DynamicContext, NameBinder, XPathContext};
    use crate::xpath::error::XPathError;
    use crate::xpath::eval::eval_node;
    use crate::xpath::functions::XPathValue;
    use crate::xpath::iterator::XmlItem;
    use crate::xpath::RoXmlNavigator;

    /// Helper to wrap a node in an Expr
    fn wrap_in_expr(arena: &mut AstArena, node_id: AstNodeId) -> AstNodeId {
        let span = SourceSpan::new(0, 10);
        let expr = ExprNode::single(node_id, span);
        arena.add(AstNode::Expr(expr))
    }

    /// Helper to bind and eval a manually constructed AST
    fn bind_and_eval(
        arena: &mut AstArena,
        root: AstNodeId,
    ) -> Result<XPathValue<RoXmlNavigator<'static>>, XPathError> {
        let names = NameTable::new();
        // Set up namespace context with "xs" prefix bound to XSD namespace
        let xs_prefix = names.add("xs");
        let ns_snapshot = NamespaceContextSnapshot {
            default_ns: None,
            bindings: vec![(xs_prefix, well_known::XS_NAMESPACE)],
        };
        let ctx = XPathContext::new(&names).with_namespaces(ns_snapshot);
        let mut binder = NameBinder::new();

        bind_node(arena, root, &ctx, &mut binder)?;

        let mut dyn_ctx: DynamicContext<'_, RoXmlNavigator<'static>> =
            DynamicContext::new(&ctx, binder.len());

        eval_node(arena, root, &mut dyn_ctx)
    }

    /// Helper to create a TypeExpr node with atomic target type
    fn make_type_expr(
        arena: &mut AstArena,
        kind: TypeExprKind,
        operand: AstNodeId,
        type_name: &str,
        occurrence: OccurrenceIndicator,
    ) -> AstNodeId {
        let span = SourceSpan::new(0, 20);
        let qname = AstQName::new("xs".to_string(), type_name.to_string());
        let item_type = AstItemTypeNode::Atomic(qname);
        let target_type = SequenceTypeNode::single(item_type, occurrence, span);
        let type_expr = TypeExprNode::new(kind, operand, target_type, span);
        arena.add(AstNode::TypeExpr(type_expr))
    }

    /// Helper to create a TypeExpr node with item() target type
    fn make_type_expr_item(
        arena: &mut AstArena,
        kind: TypeExprKind,
        operand: AstNodeId,
        occurrence: OccurrenceIndicator,
    ) -> AstNodeId {
        let span = SourceSpan::new(0, 20);
        let item_type = AstItemTypeNode::Item;
        let target_type = SequenceTypeNode::single(item_type, occurrence, span);
        let type_expr = TypeExprNode::new(kind, operand, target_type, span);
        arena.add(AstNode::TypeExpr(type_expr))
    }

    /// Helper to create an empty-sequence() type expression
    fn make_type_expr_empty_seq(
        arena: &mut AstArena,
        kind: TypeExprKind,
        operand: AstNodeId,
    ) -> AstNodeId {
        let span = SourceSpan::new(0, 20);
        let target_type = SequenceTypeNode::empty(span);
        let type_expr = TypeExprNode::new(kind, operand, target_type, span);
        arena.add(AstNode::TypeExpr(type_expr))
    }

    #[test]
    fn test_instance_of_atomic_matching() {
        // 42 instance of xs:integer -> true
        let mut arena = AstArena::new();
        let val = arena.add(AstNode::Value(ValueNode::Integer("42".to_string())));
        let type_expr = make_type_expr(
            &mut arena,
            TypeExprKind::InstanceOf,
            val,
            "integer",
            OccurrenceIndicator::One,
        );
        let root = wrap_in_expr(&mut arena, type_expr);

        let result = bind_and_eval(&mut arena, root).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(v.as_boolean(), Some(true));
            }
            _ => panic!("Expected boolean true"),
        }
    }

    #[test]
    fn test_instance_of_atomic_not_matching() {
        // 42 instance of xs:string -> false
        let mut arena = AstArena::new();
        let val = arena.add(AstNode::Value(ValueNode::Integer("42".to_string())));
        let type_expr = make_type_expr(
            &mut arena,
            TypeExprKind::InstanceOf,
            val,
            "string",
            OccurrenceIndicator::One,
        );
        let root = wrap_in_expr(&mut arena, type_expr);

        let result = bind_and_eval(&mut arena, root).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(v.as_boolean(), Some(false));
            }
            _ => panic!("Expected boolean false"),
        }
    }

    #[test]
    fn test_instance_of_string() {
        // "hello" instance of xs:string -> true
        let mut arena = AstArena::new();
        let val = arena.add(AstNode::Value(ValueNode::String("hello".to_string())));
        let type_expr = make_type_expr(
            &mut arena,
            TypeExprKind::InstanceOf,
            val,
            "string",
            OccurrenceIndicator::One,
        );
        let root = wrap_in_expr(&mut arena, type_expr);

        let result = bind_and_eval(&mut arena, root).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(v.as_boolean(), Some(true));
            }
            _ => panic!("Expected boolean true"),
        }
    }

    #[test]
    fn test_instance_of_cardinality_too_many() {
        // (1, 2) instance of xs:integer -> false (too many items)
        let mut arena = AstArena::new();
        let v1 = arena.add(AstNode::Value(ValueNode::Integer("1".to_string())));
        let v2 = arena.add(AstNode::Value(ValueNode::Integer("2".to_string())));
        let span = SourceSpan::new(0, 5);
        let seq = ExprNode::sequence(vec![v1, v2], span);
        let seq_id = arena.add(AstNode::Expr(seq));
        let type_expr = make_type_expr(
            &mut arena,
            TypeExprKind::InstanceOf,
            seq_id,
            "integer",
            OccurrenceIndicator::One,
        );
        let root = wrap_in_expr(&mut arena, type_expr);

        let result = bind_and_eval(&mut arena, root).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(v.as_boolean(), Some(false));
            }
            _ => panic!("Expected boolean false"),
        }
    }

    #[test]
    fn test_instance_of_cardinality_star() {
        // (1, 2) instance of xs:integer* -> true
        let mut arena = AstArena::new();
        let v1 = arena.add(AstNode::Value(ValueNode::Integer("1".to_string())));
        let v2 = arena.add(AstNode::Value(ValueNode::Integer("2".to_string())));
        let span = SourceSpan::new(0, 5);
        let seq = ExprNode::sequence(vec![v1, v2], span);
        let seq_id = arena.add(AstNode::Expr(seq));
        let type_expr = make_type_expr(
            &mut arena,
            TypeExprKind::InstanceOf,
            seq_id,
            "integer",
            OccurrenceIndicator::ZeroOrMore,
        );
        let root = wrap_in_expr(&mut arena, type_expr);

        let result = bind_and_eval(&mut arena, root).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(v.as_boolean(), Some(true));
            }
            _ => panic!("Expected boolean true"),
        }
    }

    #[test]
    fn test_instance_of_empty_sequence() {
        // () instance of xs:integer? -> true
        let mut arena = AstArena::new();
        let empty = arena.add(AstNode::Value(ValueNode::Empty));
        let type_expr = make_type_expr(
            &mut arena,
            TypeExprKind::InstanceOf,
            empty,
            "integer",
            OccurrenceIndicator::ZeroOrOne,
        );
        let root = wrap_in_expr(&mut arena, type_expr);

        let result = bind_and_eval(&mut arena, root).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(v.as_boolean(), Some(true));
            }
            _ => panic!("Expected boolean true"),
        }

        // () instance of xs:integer -> false
        let mut arena = AstArena::new();
        let empty = arena.add(AstNode::Value(ValueNode::Empty));
        let type_expr = make_type_expr(
            &mut arena,
            TypeExprKind::InstanceOf,
            empty,
            "integer",
            OccurrenceIndicator::One,
        );
        let root = wrap_in_expr(&mut arena, type_expr);

        let result = bind_and_eval(&mut arena, root).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(v.as_boolean(), Some(false));
            }
            _ => panic!("Expected boolean false"),
        }
    }

    #[test]
    fn test_instance_of_item() {
        // 42 instance of item() -> true
        let mut arena = AstArena::new();
        let val = arena.add(AstNode::Value(ValueNode::Integer("42".to_string())));
        let type_expr = make_type_expr_item(
            &mut arena,
            TypeExprKind::InstanceOf,
            val,
            OccurrenceIndicator::One,
        );
        let root = wrap_in_expr(&mut arena, type_expr);

        let result = bind_and_eval(&mut arena, root).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(v.as_boolean(), Some(true));
            }
            _ => panic!("Expected boolean true"),
        }
    }

    #[test]
    fn test_instance_of_empty_sequence_type() {
        // () instance of empty-sequence() -> true
        let mut arena = AstArena::new();
        let empty = arena.add(AstNode::Value(ValueNode::Empty));
        let type_expr = make_type_expr_empty_seq(&mut arena, TypeExprKind::InstanceOf, empty);
        let root = wrap_in_expr(&mut arena, type_expr);

        let result = bind_and_eval(&mut arena, root).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(v.as_boolean(), Some(true));
            }
            _ => panic!("Expected boolean true"),
        }

        // 42 instance of empty-sequence() -> false
        let mut arena = AstArena::new();
        let val = arena.add(AstNode::Value(ValueNode::Integer("42".to_string())));
        let type_expr = make_type_expr_empty_seq(&mut arena, TypeExprKind::InstanceOf, val);
        let root = wrap_in_expr(&mut arena, type_expr);

        let result = bind_and_eval(&mut arena, root).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(v.as_boolean(), Some(false));
            }
            _ => panic!("Expected boolean false"),
        }
    }

    #[test]
    fn test_treat_as_success() {
        // "hello" treat as xs:string -> "hello"
        let mut arena = AstArena::new();
        let val = arena.add(AstNode::Value(ValueNode::String("hello".to_string())));
        let type_expr = make_type_expr(
            &mut arena,
            TypeExprKind::TreatAs,
            val,
            "string",
            OccurrenceIndicator::One,
        );
        let root = wrap_in_expr(&mut arena, type_expr);

        let result = bind_and_eval(&mut arena, root).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(v.as_string(), Some("hello"));
            }
            _ => panic!("Expected string 'hello'"),
        }
    }

    #[test]
    fn test_treat_as_failure() {
        // 42 treat as xs:string -> XPDY0050. XPath 2.0 §3.10.5: a value that
        // does not match the sequence type of a `treat` expression raises a
        // *dynamic* error, not a type error.
        let mut arena = AstArena::new();
        let val = arena.add(AstNode::Value(ValueNode::Integer("42".to_string())));
        let type_expr = make_type_expr(
            &mut arena,
            TypeExprKind::TreatAs,
            val,
            "string",
            OccurrenceIndicator::One,
        );
        let root = wrap_in_expr(&mut arena, type_expr);

        let result = bind_and_eval(&mut arena, root);
        assert!(matches!(result, Err(XPathError::XPDY0050 { .. })));
    }

    #[test]
    fn test_treat_as_empty_optional() {
        // () treat as xs:integer? -> ()
        let mut arena = AstArena::new();
        let empty = arena.add(AstNode::Value(ValueNode::Empty));
        let type_expr = make_type_expr(
            &mut arena,
            TypeExprKind::TreatAs,
            empty,
            "integer",
            OccurrenceIndicator::ZeroOrOne,
        );
        let root = wrap_in_expr(&mut arena, type_expr);

        let result = bind_and_eval(&mut arena, root).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_cast_as_string_to_integer() {
        // "42" cast as xs:integer -> 42
        let mut arena = AstArena::new();
        let val = arena.add(AstNode::Value(ValueNode::String("42".to_string())));
        let type_expr = make_type_expr(
            &mut arena,
            TypeExprKind::CastAs,
            val,
            "integer",
            OccurrenceIndicator::One,
        );
        let root = wrap_in_expr(&mut arena, type_expr);

        let result = bind_and_eval(&mut arena, root).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(
                    v.as_integer().map(|i| i.to_string()),
                    Some("42".to_string())
                );
            }
            _ => panic!("Expected integer 42"),
        }
    }

    #[test]
    fn test_cast_as_double_to_integer() {
        // 42.7 cast as xs:integer -> 42 (truncated)
        let mut arena = AstArena::new();
        let val = arena.add(AstNode::Value(ValueNode::Double("42.7".to_string())));
        let type_expr = make_type_expr(
            &mut arena,
            TypeExprKind::CastAs,
            val,
            "integer",
            OccurrenceIndicator::One,
        );
        let root = wrap_in_expr(&mut arena, type_expr);

        let result = bind_and_eval(&mut arena, root).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(
                    v.as_integer().map(|i| i.to_string()),
                    Some("42".to_string())
                );
            }
            _ => panic!("Expected integer 42"),
        }
    }

    #[test]
    fn test_cast_as_empty_optional() {
        // () cast as xs:integer? -> ()
        let mut arena = AstArena::new();
        let empty = arena.add(AstNode::Value(ValueNode::Empty));
        let type_expr = make_type_expr(
            &mut arena,
            TypeExprKind::CastAs,
            empty,
            "integer",
            OccurrenceIndicator::ZeroOrOne,
        );
        let root = wrap_in_expr(&mut arena, type_expr);

        let result = bind_and_eval(&mut arena, root).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_cast_as_empty_required() {
        // () cast as xs:integer -> XPTY0004 error
        let mut arena = AstArena::new();
        let empty = arena.add(AstNode::Value(ValueNode::Empty));
        let type_expr = make_type_expr(
            &mut arena,
            TypeExprKind::CastAs,
            empty,
            "integer",
            OccurrenceIndicator::One,
        );
        let root = wrap_in_expr(&mut arena, type_expr);

        let result = bind_and_eval(&mut arena, root);
        assert!(matches!(result, Err(XPathError::XPTY0004 { .. })));
    }

    #[test]
    fn test_cast_as_invalid() {
        // "abc" cast as xs:integer -> FORG0001 error
        let mut arena = AstArena::new();
        let val = arena.add(AstNode::Value(ValueNode::String("abc".to_string())));
        let type_expr = make_type_expr(
            &mut arena,
            TypeExprKind::CastAs,
            val,
            "integer",
            OccurrenceIndicator::One,
        );
        let root = wrap_in_expr(&mut arena, type_expr);

        let result = bind_and_eval(&mut arena, root);
        assert!(matches!(result, Err(XPathError::FORG0001 { .. })));
    }

    #[test]
    fn test_castable_as_success() {
        // "42" castable as xs:integer -> true
        let mut arena = AstArena::new();
        let val = arena.add(AstNode::Value(ValueNode::String("42".to_string())));
        let type_expr = make_type_expr(
            &mut arena,
            TypeExprKind::CastableAs,
            val,
            "integer",
            OccurrenceIndicator::One,
        );
        let root = wrap_in_expr(&mut arena, type_expr);

        let result = bind_and_eval(&mut arena, root).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(v.as_boolean(), Some(true));
            }
            _ => panic!("Expected boolean true"),
        }
    }

    #[test]
    fn test_castable_as_failure() {
        // "abc" castable as xs:integer -> false
        let mut arena = AstArena::new();
        let val = arena.add(AstNode::Value(ValueNode::String("abc".to_string())));
        let type_expr = make_type_expr(
            &mut arena,
            TypeExprKind::CastableAs,
            val,
            "integer",
            OccurrenceIndicator::One,
        );
        let root = wrap_in_expr(&mut arena, type_expr);

        let result = bind_and_eval(&mut arena, root).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(v.as_boolean(), Some(false));
            }
            _ => panic!("Expected boolean false"),
        }
    }

    #[test]
    fn test_castable_as_empty_optional() {
        // () castable as xs:integer? -> true
        let mut arena = AstArena::new();
        let empty = arena.add(AstNode::Value(ValueNode::Empty));
        let type_expr = make_type_expr(
            &mut arena,
            TypeExprKind::CastableAs,
            empty,
            "integer",
            OccurrenceIndicator::ZeroOrOne,
        );
        let root = wrap_in_expr(&mut arena, type_expr);

        let result = bind_and_eval(&mut arena, root).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(v.as_boolean(), Some(true));
            }
            _ => panic!("Expected boolean true"),
        }
    }

    #[test]
    fn test_castable_as_empty_required() {
        // () castable as xs:integer -> false
        let mut arena = AstArena::new();
        let empty = arena.add(AstNode::Value(ValueNode::Empty));
        let type_expr = make_type_expr(
            &mut arena,
            TypeExprKind::CastableAs,
            empty,
            "integer",
            OccurrenceIndicator::One,
        );
        let root = wrap_in_expr(&mut arena, type_expr);

        let result = bind_and_eval(&mut arena, root).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(v.as_boolean(), Some(false));
            }
            _ => panic!("Expected boolean false"),
        }
    }

    /// N2: the item type the binder resolves for `instance of` / `treat as` is
    /// a cache inside the node. A node a host builds and evaluates unbound has
    /// none and is resolved on the fly; a bound node whose `target_type` the
    /// host edits is right again once it is bound again.
    #[test]
    fn instance_of_matches_unbound_and_rebound_nodes() {
        use crate::xpath::ast::{ElementTest, KindTest};

        let names = NameTable::new();
        let ctx = XPathContext::new(&names);
        let span = SourceSpan::new(0, 20);
        let mut arena = AstArena::new();
        let value = arena.add(AstNode::Value(ValueNode::String("a".to_string())));
        let item = SequenceTypeNode::single(AstItemTypeNode::Item, OccurrenceIndicator::One, span);
        let id = arena.add(AstNode::TypeExpr(TypeExprNode::new(
            TypeExprKind::InstanceOf,
            value,
            item,
            span,
        )));
        let eval = |arena: &AstArena| -> Option<bool> {
            let mut dynamic: DynamicContext<'_, RoXmlNavigator<'static>> =
                DynamicContext::new(&ctx, 0);
            match eval_node(arena, id, &mut dynamic).expect("evaluate") {
                XPathValue::Item(XmlItem::Atomic(v)) => v.as_boolean(),
                _ => None,
            }
        };
        // Unbound: resolved on the fly.
        assert_eq!(eval(&arena), Some(true));
        // Bound, then edited in place to `element()`.
        bind_node(&mut arena, id, &ctx, &mut NameBinder::new()).expect("bind");
        assert_eq!(eval(&arena), Some(true));
        if let AstNode::TypeExpr(node) = arena.get_mut(id) {
            node.target_type = SequenceTypeNode::single(
                AstItemTypeNode::Kind(KindTest::Element(ElementTest::default())),
                OccurrenceIndicator::One,
                span,
            );
        }
        bind_node(&mut arena, id, &ctx, &mut NameBinder::new()).expect("bind again");
        assert_eq!(eval(&arena), Some(false));
        // A fresh node built by the host, never bound, for the edited type.
        let rebuilt = TypeExprNode::new(
            TypeExprKind::InstanceOf,
            value,
            SequenceTypeNode::single(
                AstItemTypeNode::Kind(KindTest::Element(ElementTest::default())),
                OccurrenceIndicator::One,
                span,
            ),
            span,
        );
        *arena.get_mut(id) = AstNode::TypeExpr(rebuilt);
        assert_eq!(eval(&arena), Some(false));
    }
} // end type_expr_tests

// ============================================================================
// Path Expression Tests
// ============================================================================

mod path_expr_tests {
    use super::*;
    use crate::xpath::ast::{
        Axis, ExprNode, FilterExprNode, KindTest, NameTest as AstNameTest, NodeTest as AstNodeTest,
        PathExprNode, PathStepNode,
    };
    use crate::xpath::bind::bind_node;
    use crate::xpath::context::NameBinder;

    /// Helper to create a path step
    fn make_path_step(arena: &mut AstArena, axis: Axis, test: AstNodeTest) -> AstNodeId {
        let span = SourceSpan::new(0, 10);
        let step = PathStepNode::new(axis, test, span);
        arena.add(AstNode::PathStep(step))
    }

    /// Helper to create a name test for any element
    fn wildcard_name_test() -> AstNodeTest {
        AstNodeTest::Name(AstNameTest::any())
    }

    /// Helper to create a kind test for node()
    fn node_kind_test() -> AstNodeTest {
        AstNodeTest::Kind(KindTest::AnyKind)
    }

    #[test]
    fn test_path_root_only() {
        // Test "/" - returns document root
        let doc = roxmltree::Document::parse("<root><a/><b/></root>").expect("parse xml");
        let mut nav = RoXmlNavigator::new(&doc);
        nav.move_to_first_child(); // root
        nav.move_to_first_child(); // a

        let names = NameTable::new();
        let ctx = XPathContext::new(&names);
        let mut binder = NameBinder::new();

        let mut arena = AstArena::new();
        let span = SourceSpan::new(0, 1);
        // Create root-only path "/"
        let path = PathExprNode::root_only(span);
        let path_id = arena.add(AstNode::PathExpr(path));
        let root = wrap_in_expr(&mut arena, path_id);

        bind_node(&mut arena, root, &ctx, &mut binder).unwrap();

        let mut dyn_ctx =
            DynamicContext::new(&ctx, binder.len()).with_context_item(XmlItem::Node(nav.clone()));

        let result = eval_node(&arena, root, &mut dyn_ctx).unwrap();
        match result {
            XPathValue::Item(XmlItem::Node(n)) => {
                assert!(
                    matches!(n.node_type(), crate::xpath::DomNodeType::Root),
                    "Expected document root"
                );
            }
            _ => panic!("Expected single node result"),
        }
    }

    #[test]
    fn test_path_absolute_child() {
        // Test "/child::*" - returns root element
        let doc = roxmltree::Document::parse("<root><a/><b/></root>").expect("parse xml");
        let mut nav = RoXmlNavigator::new(&doc);
        nav.move_to_first_child(); // position at root
        nav.move_to_first_child(); // position at a

        let names = NameTable::new();
        let ctx = XPathContext::new(&names);
        let mut binder = NameBinder::new();

        let mut arena = AstArena::new();
        let span = SourceSpan::new(0, 10);

        // Create step: child::*
        let step_id = make_path_step(&mut arena, Axis::Child, wildcard_name_test());

        // Create absolute path with that step
        let path = PathExprNode::absolute(vec![step_id], span);
        let path_id = arena.add(AstNode::PathExpr(path));
        let root = wrap_in_expr(&mut arena, path_id);

        bind_node(&mut arena, root, &ctx, &mut binder).unwrap();

        let mut dyn_ctx =
            DynamicContext::new(&ctx, binder.len()).with_context_item(XmlItem::Node(nav.clone()));

        let result = eval_node(&arena, root, &mut dyn_ctx).unwrap();
        match result {
            XPathValue::Item(XmlItem::Node(n)) => {
                assert_eq!(n.local_name(), "root");
            }
            _ => panic!("Expected single node result for /*"),
        }
    }

    #[test]
    fn test_path_relative_child() {
        // Test "child::*" from <root> - should return <a> and <b>
        let doc = roxmltree::Document::parse("<root><a/><b/></root>").expect("parse xml");
        let mut nav = RoXmlNavigator::new(&doc);
        nav.move_to_first_child(); // position at root

        let names = NameTable::new();
        let ctx = XPathContext::new(&names);
        let mut binder = NameBinder::new();

        let mut arena = AstArena::new();
        let span = SourceSpan::new(0, 10);

        // Create step: child::*
        let step_id = make_path_step(&mut arena, Axis::Child, wildcard_name_test());

        // Create relative path
        let path = PathExprNode::relative(vec![step_id], span);
        let path_id = arena.add(AstNode::PathExpr(path));
        let root = wrap_in_expr(&mut arena, path_id);

        bind_node(&mut arena, root, &ctx, &mut binder).unwrap();

        let mut dyn_ctx =
            DynamicContext::new(&ctx, binder.len()).with_context_item(XmlItem::Node(nav.clone()));

        let result = eval_node(&arena, root, &mut dyn_ctx).unwrap();
        let items = result.into_vec();
        assert_eq!(items.len(), 2);
        match (&items[0], &items[1]) {
            (XmlItem::Node(a), XmlItem::Node(b)) => {
                assert_eq!(a.local_name(), "a");
                assert_eq!(b.local_name(), "b");
            }
            _ => panic!("Expected two nodes"),
        }
    }

    #[test]
    fn test_path_descendant_or_self() {
        // Test "descendant-or-self::node()" from <root> - should include root and descendants
        let doc = roxmltree::Document::parse("<root><a><c/></a><b/></root>").expect("parse xml");
        let mut nav = RoXmlNavigator::new(&doc);
        nav.move_to_first_child(); // position at root

        let names = NameTable::new();
        let ctx = XPathContext::new(&names);
        let mut binder = NameBinder::new();

        let mut arena = AstArena::new();
        let span = SourceSpan::new(0, 10);

        // Create step: descendant-or-self::node()
        let step_id = make_path_step(&mut arena, Axis::DescendantOrSelf, node_kind_test());

        // Create relative path
        let path = PathExprNode::relative(vec![step_id], span);
        let path_id = arena.add(AstNode::PathExpr(path));
        let root = wrap_in_expr(&mut arena, path_id);

        bind_node(&mut arena, root, &ctx, &mut binder).unwrap();

        let mut dyn_ctx =
            DynamicContext::new(&ctx, binder.len()).with_context_item(XmlItem::Node(nav.clone()));

        let result = eval_node(&arena, root, &mut dyn_ctx).unwrap();
        let items = result.into_vec();
        // Should include: root, a, c, b
        assert_eq!(items.len(), 4);
    }

    #[test]
    fn test_path_parent_axis() {
        // Test "parent::*" from <a> - should return <root>
        let doc = roxmltree::Document::parse("<root><a/><b/></root>").expect("parse xml");
        let mut nav = RoXmlNavigator::new(&doc);
        nav.move_to_first_child(); // position at root
        nav.move_to_first_child(); // position at a

        let names = NameTable::new();
        let ctx = XPathContext::new(&names);
        let mut binder = NameBinder::new();

        let mut arena = AstArena::new();
        let span = SourceSpan::new(0, 10);

        // Create step: parent::*
        let step_id = make_path_step(&mut arena, Axis::Parent, wildcard_name_test());

        // Create relative path
        let path = PathExprNode::relative(vec![step_id], span);
        let path_id = arena.add(AstNode::PathExpr(path));
        let root = wrap_in_expr(&mut arena, path_id);

        bind_node(&mut arena, root, &ctx, &mut binder).unwrap();

        let mut dyn_ctx =
            DynamicContext::new(&ctx, binder.len()).with_context_item(XmlItem::Node(nav.clone()));

        let result = eval_node(&arena, root, &mut dyn_ctx).unwrap();
        match result {
            XPathValue::Item(XmlItem::Node(n)) => {
                assert_eq!(n.local_name(), "root");
            }
            _ => panic!("Expected single node result for parent::*"),
        }
    }

    #[test]
    fn test_path_with_predicate_position() {
        // Test "child::*[1]" - returns first child only
        let doc = roxmltree::Document::parse("<root><a/><b/><c/></root>").expect("parse xml");
        let mut nav = RoXmlNavigator::new(&doc);
        nav.move_to_first_child(); // position at root

        let names = NameTable::new();
        let ctx = XPathContext::new(&names);
        let mut binder = NameBinder::new();

        let mut arena = AstArena::new();
        let span = SourceSpan::new(0, 10);

        // Create predicate: 1
        let pred = arena.add(AstNode::Value(ValueNode::Integer("1".to_string())));

        // Create step: child::* with predicate [1]
        let step =
            PathStepNode::with_predicates(Axis::Child, wildcard_name_test(), vec![pred], span);
        let step_id = arena.add(AstNode::PathStep(step));

        // Create relative path
        let path = PathExprNode::relative(vec![step_id], span);
        let path_id = arena.add(AstNode::PathExpr(path));
        let root = wrap_in_expr(&mut arena, path_id);

        bind_node(&mut arena, root, &ctx, &mut binder).unwrap();

        let mut dyn_ctx =
            DynamicContext::new(&ctx, binder.len()).with_context_item(XmlItem::Node(nav.clone()));

        let result = eval_node(&arena, root, &mut dyn_ctx).unwrap();
        match result {
            XPathValue::Item(XmlItem::Node(n)) => {
                assert_eq!(n.local_name(), "a");
            }
            _ => panic!("Expected single node result for *[1]"),
        }
    }

    #[test]
    fn test_path_with_predicate_boolean() {
        // Test "child::*[true()]" - returns all children
        let doc = roxmltree::Document::parse("<root><a/><b/></root>").expect("parse xml");
        let mut nav = RoXmlNavigator::new(&doc);
        nav.move_to_first_child(); // position at root

        let names = NameTable::new();
        let ctx = XPathContext::new(&names);
        let mut binder = NameBinder::new();

        let mut arena = AstArena::new();
        let span = SourceSpan::new(0, 10);

        // Create predicate: true()
        let pred = make_function_call(&mut arena, "", "true", vec![]);

        // Create step: child::* with predicate [true()]
        let step =
            PathStepNode::with_predicates(Axis::Child, wildcard_name_test(), vec![pred], span);
        let step_id = arena.add(AstNode::PathStep(step));

        // Create relative path
        let path = PathExprNode::relative(vec![step_id], span);
        let path_id = arena.add(AstNode::PathExpr(path));
        let root = wrap_in_expr(&mut arena, path_id);

        bind_node(&mut arena, root, &ctx, &mut binder).unwrap();

        let mut dyn_ctx =
            DynamicContext::new(&ctx, binder.len()).with_context_item(XmlItem::Node(nav.clone()));

        let result = eval_node(&arena, root, &mut dyn_ctx).unwrap();
        let items = result.into_vec();
        assert_eq!(items.len(), 2, "Expected all children with [true()]");
    }

    #[test]
    fn test_path_multi_step() {
        // Test "child::*/child::*" - returns grandchildren
        let doc = roxmltree::Document::parse("<root><a><x/><y/></a><b><z/></b></root>")
            .expect("parse xml");
        let mut nav = RoXmlNavigator::new(&doc);
        nav.move_to_first_child(); // position at root

        let names = NameTable::new();
        let ctx = XPathContext::new(&names);
        let mut binder = NameBinder::new();

        let mut arena = AstArena::new();
        let span = SourceSpan::new(0, 10);

        // Create two steps: child::* / child::*
        let step1 = make_path_step(&mut arena, Axis::Child, wildcard_name_test());
        let step2 = make_path_step(&mut arena, Axis::Child, wildcard_name_test());

        // Create relative path with both steps
        let path = PathExprNode::relative(vec![step1, step2], span);
        let path_id = arena.add(AstNode::PathExpr(path));
        let root = wrap_in_expr(&mut arena, path_id);

        bind_node(&mut arena, root, &ctx, &mut binder).unwrap();

        let mut dyn_ctx =
            DynamicContext::new(&ctx, binder.len()).with_context_item(XmlItem::Node(nav.clone()));

        let result = eval_node(&arena, root, &mut dyn_ctx).unwrap();
        let items = result.into_vec();
        assert_eq!(items.len(), 3, "Expected 3 grandchildren: x, y, z");
        let names: Vec<String> = items
            .iter()
            .filter_map(|item| match item {
                XmlItem::Node(n) => Some(n.local_name().to_string()),
                _ => None,
            })
            .collect();
        assert!(names.contains(&"x".to_string()));
        assert!(names.contains(&"y".to_string()));
        assert!(names.contains(&"z".to_string()));
    }

    #[test]
    fn test_filter_expr() {
        // Test "(1, 2, 3)[2]" - returns 2
        let names = NameTable::new();
        let ctx = XPathContext::new(&names);
        let mut binder = NameBinder::new();

        let mut arena = AstArena::new();
        let span = SourceSpan::new(0, 10);

        // Create sequence (1, 2, 3)
        let v1 = arena.add(AstNode::Value(ValueNode::Integer("1".to_string())));
        let v2 = arena.add(AstNode::Value(ValueNode::Integer("2".to_string())));
        let v3 = arena.add(AstNode::Value(ValueNode::Integer("3".to_string())));
        let expr = ExprNode::sequence(vec![v1, v2, v3], span);
        let base_id = arena.add(AstNode::Expr(expr));

        // Create predicate: 2
        let pred = arena.add(AstNode::Value(ValueNode::Integer("2".to_string())));

        // Create filter expression
        let filter = FilterExprNode::new(base_id, vec![pred], span);
        let filter_id = arena.add(AstNode::FilterExpr(filter));
        let root = wrap_in_expr(&mut arena, filter_id);

        bind_node(&mut arena, root, &ctx, &mut binder).unwrap();

        let mut dyn_ctx: DynamicContext<'_, RoXmlNavigator<'static>> =
            DynamicContext::new(&ctx, binder.len());

        let result = eval_node(&arena, root, &mut dyn_ctx).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(v.as_integer().map(|i| i.to_string()), Some("2".to_string()));
            }
            _ => panic!("Expected single integer result"),
        }
    }

    #[test]
    fn test_path_self_axis() {
        // Test "self::*" from <root> - should return <root>
        let doc = roxmltree::Document::parse("<root><a/></root>").expect("parse xml");
        let mut nav = RoXmlNavigator::new(&doc);
        nav.move_to_first_child(); // position at root

        let names = NameTable::new();
        let ctx = XPathContext::new(&names);
        let mut binder = NameBinder::new();

        let mut arena = AstArena::new();
        let span = SourceSpan::new(0, 10);

        // Create step: self::*
        let step_id = make_path_step(&mut arena, Axis::SelfAxis, wildcard_name_test());

        // Create relative path
        let path = PathExprNode::relative(vec![step_id], span);
        let path_id = arena.add(AstNode::PathExpr(path));
        let root = wrap_in_expr(&mut arena, path_id);

        bind_node(&mut arena, root, &ctx, &mut binder).unwrap();

        let mut dyn_ctx =
            DynamicContext::new(&ctx, binder.len()).with_context_item(XmlItem::Node(nav.clone()));

        let result = eval_node(&arena, root, &mut dyn_ctx).unwrap();
        match result {
            XPathValue::Item(XmlItem::Node(n)) => {
                assert_eq!(n.local_name(), "root");
            }
            _ => panic!("Expected single node result for self::*"),
        }
    }
}

// ============================================================================
// XPath 1.0 Mode Integration Tests
// ============================================================================

mod xpath10_eval_tests {
    use crate::namespace::table::NameTable;
    use crate::xpath::bind::bind_node;
    use crate::xpath::context::{DynamicContext, NameBinder, XPathContext};
    use crate::xpath::error::XPathError;
    use crate::xpath::eval::eval_node;
    use crate::xpath::functions::XPathValue;
    use crate::xpath::functions::{XPath10Catalog, XPath10Evaluator};
    use crate::xpath::iterator::XmlItem;
    use crate::xpath::parser;
    use crate::xpath::parser::parse_with_mode;
    use crate::xpath::RoXmlNavigator;
    use crate::xpath::XPathMode;

    /// Helper to parse, bind, and evaluate an XPath 1.0 expression without context item
    fn eval_xpath10(expr: &str) -> Result<XPathValue<RoXmlNavigator<'static>>, XPathError> {
        let mut parsed = parse_with_mode(expr, XPathMode::XPath10)
            .map_err(|e| XPathError::syntax_error(e.to_string()))?;

        let names = NameTable::new();
        let catalog = XPath10Catalog;
        let ctx = XPathContext::new(&names)
            .with_mode(XPathMode::XPath10)
            .with_function_catalog(&catalog);
        let mut binder = NameBinder::new();

        bind_node(&mut parsed.arena, parsed.root, &ctx, &mut binder)?;

        let evaluator = XPath10Evaluator;
        let mut dyn_ctx: DynamicContext<'_, RoXmlNavigator<'static>> =
            DynamicContext::new(&ctx, binder.len()).with_function_evaluator(&evaluator);

        eval_node(&parsed.arena, parsed.root, &mut dyn_ctx)
    }

    /// Helper to parse, bind, and evaluate an XPath 2.0 expression without context item
    fn eval_xpath20(expr: &str) -> Result<XPathValue<RoXmlNavigator<'static>>, XPathError> {
        let mut parsed =
            parser::parse(expr).map_err(|e| XPathError::syntax_error(e.to_string()))?;

        let names = NameTable::new();
        let ctx = XPathContext::new(&names);
        let mut binder = NameBinder::new();

        bind_node(&mut parsed.arena, parsed.root, &ctx, &mut binder)?;

        let mut dyn_ctx: DynamicContext<'_, RoXmlNavigator<'static>> =
            DynamicContext::new(&ctx, binder.len());

        eval_node(&parsed.arena, parsed.root, &mut dyn_ctx)
    }

    #[test]
    fn test_xpath10_arithmetic_returns_double() {
        // In XPath 1.0, 1 + 2 = 3.0 (double), not 3 (integer)
        let result = eval_xpath10("1 + 2").unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(v.type_code, crate::types::XmlTypeCode::Double);
                assert_eq!(v.as_double(), Some(3.0));
            }
            _ => panic!("Expected atomic double"),
        }
    }

    #[test]
    fn test_xpath10_arithmetic_mul() {
        let result = eval_xpath10("3 * 4").unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(v.type_code, crate::types::XmlTypeCode::Double);
                assert_eq!(v.as_double(), Some(12.0));
            }
            _ => panic!("Expected atomic double"),
        }
    }

    #[test]
    fn test_xpath10_comparison_eq_boolean_priority() {
        // "1" = true() → XPath 1.0: both to boolean → true = true → true
        let result = eval_xpath10("'1' = true()").unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(v.as_boolean(), Some(true));
            }
            _ => panic!("Expected boolean true"),
        }
    }

    #[test]
    fn test_xpath10_comparison_lt_numeric_coercion() {
        // 3 < 10 → numeric comparison → true
        let result = eval_xpath10("3 < 10").unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(v.as_boolean(), Some(true));
            }
            _ => panic!("Expected boolean true"),
        }
    }

    #[test]
    fn test_xpath10_comparison_eq_string() {
        // 'abc' = 'abc' → string comparison → true
        let result = eval_xpath10("'abc' = 'abc'").unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(v.as_boolean(), Some(true));
            }
            _ => panic!("Expected boolean true"),
        }
    }

    #[test]
    fn test_xpath10_comparison_eq_string_ne() {
        // 'abc' = 'def' → string comparison → false
        let result = eval_xpath10("'abc' = 'def'").unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(v.as_boolean(), Some(false));
            }
            _ => panic!("Expected boolean false"),
        }
    }

    #[test]
    fn test_xpath10_rejects_sequence_expression() {
        // (1, 2, 3) uses the comma operator which is not available in XPath 1.0
        let result = eval_xpath10("(1, 2, 3)");
        match result {
            Err(ref e) => assert_eq!(e.error_code(), Some("XPST0003")),
            Ok(_) => panic!("Expected XPST0003 error for comma sequence in 1.0 mode"),
        }
    }

    #[test]
    fn test_xpath20_predicate_no_rounding() {
        // (1, 2, 3)[2.5] in XPath 2.0 → 2.5 matches no position → empty
        let result = eval_xpath20("(1, 2, 3)[2.5]").unwrap();
        assert!(
            matches!(result, XPathValue::Empty),
            "Expected empty sequence for [2.5] predicate in 2.0 mode",
        );
    }

    #[test]
    fn test_xpath10_and_with_ebv() {
        // true() and true() → true
        let result = eval_xpath10("true() and true()").unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(v.as_boolean(), Some(true));
            }
            _ => panic!("Expected boolean true"),
        }
    }

    #[test]
    fn test_xpath10_or_with_ebv() {
        // false() or true() → true
        let result = eval_xpath10("false() or true()").unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(v.as_boolean(), Some(true));
            }
            _ => panic!("Expected boolean true"),
        }
    }
}

/// XPath 2.0 §3.3.1: the operands of `to` are `xs:integer?`, so an
/// `xs:untypedAtomic` operand is cast rather than rejected.
#[test]
fn range_operands_follow_the_function_conversion_rules() {
    use crate::namespace::table::NameTable;
    use crate::xpath::api::XPathExpr;
    use crate::xpath::RoXmlNavigator;

    use crate::namespace::context::NamespaceContextSnapshot;
    let names = NameTable::new();
    let mut namespaces = NamespaceContextSnapshot::default();
    namespaces.bindings.push((
        names.add("xs"),
        names.add("http://www.w3.org/2001/XMLSchema"),
    ));
    let ctx = XPathContext::new(&names).with_namespaces(namespaces);
    let count = |expr: &str| {
        XPathExpr::compile(expr, &ctx)
            .expect("compile")
            .evaluator(&ctx)
            .run::<RoXmlNavigator<'static>>()
            .map(|v| v.len())
    };
    assert_eq!(count("xs:untypedAtomic('5') to 7").unwrap(), 3);
    assert_eq!(count("1 to xs:untypedAtomic('3')").unwrap(), 3);
    // A value that is not an integer is still an error.
    assert!(count("xs:untypedAtomic('x') to 3").is_err());
    assert!(count("1.5 to 3").is_err());
}

// ============================================================================
// Axis Semantics (XPath 2.0 §3.2.1.1, §3.2.1.2, §3.2.4)
// ============================================================================

#[cfg(test)]
mod axis_semantics_tests {
    use crate::namespace::table::NameTable;
    use crate::xpath::api::XPathExpr;
    use crate::xpath::iterator::XmlItem;
    use crate::xpath::{DomNavigator, DomNodeType, RoXmlNavigator, XPathContext};

    /// One readable line per result item: `kind(detail)` for nodes, the
    /// lexical form for atomic values.
    fn describe<N: DomNavigator>(item: &XmlItem<N>) -> String {
        match item {
            XmlItem::Atomic(value) => value.to_string_value(),
            XmlItem::Node(nav) => match nav.node_type() {
                DomNodeType::Root => "document".to_string(),
                DomNodeType::Element => format!("element({})", nav.local_name()),
                DomNodeType::Attribute => {
                    format!("attribute({}={})", nav.local_name(), nav.value())
                }
                DomNodeType::Namespace => {
                    format!("namespace({}={})", nav.local_name(), nav.value())
                }
                DomNodeType::Comment => format!("comment({})", nav.value()),
                DomNodeType::ProcessingInstruction => format!("pi({})", nav.local_name()),
                other => format!("{other:?}({})", nav.value()),
            },
        }
    }

    /// Evaluate `expr` over `xml` with the document node as context item.
    fn items(expr: &str, xml: &str) -> Vec<String> {
        let names = NameTable::new();
        let ctx = XPathContext::new(&names);
        let compiled = XPathExpr::compile(expr, &ctx).expect("compile");
        let doc = roxmltree::Document::parse(xml).expect("parse xml");
        let value = compiled
            .evaluator(&ctx)
            .run_with_node(RoXmlNavigator::new(&doc))
            .expect("evaluate");
        value.into_vec().iter().map(describe).collect()
    }

    /// Evaluate an expression whose result is a single atomic value.
    fn one(expr: &str, xml: &str) -> String {
        let result = items(expr, xml);
        assert_eq!(result.len(), 1, "{expr}: expected one item, got {result:?}");
        result.into_iter().next().unwrap()
    }

    // ---- §3.2.4: the default axis of an abbreviated forward step ----------

    #[test]
    fn abbrev_step_with_attribute_test_uses_attribute_axis() {
        // "If the axis name is omitted from an axis step, the default axis is
        // child unless the axis step contains an AttributeTest or
        // SchemaAttributeTest; in that case, the default axis is attribute."
        let xml = r#"<doc a="1" b="2"><num>1</num></doc>"#;
        assert_eq!(one("count(/doc/attribute())", xml), "2");
        assert_eq!(one("count(/doc/attribute(a))", xml), "1");
        assert_eq!(items("/doc/attribute(a)", xml), ["attribute(a=1)"]);
        // The explicit axis keeps working, and yields the same nodes.
        assert_eq!(one("count(/doc/attribute::attribute())", xml), "2");
    }

    #[test]
    fn abbrev_step_attribute_test_in_every_position() {
        let xml = r#"<doc a="1"><num b="2"/></doc>"#;
        // `//` expansion: descendant-or-self::node()/attribute()
        assert_eq!(one("count(//attribute())", xml), "2");
        assert_eq!(
            one("//attribute() instance of empty-sequence()", xml),
            "false"
        );
        // Inside a predicate.
        assert_eq!(one("count(/doc/num[attribute()])", xml), "1");
        // Relative path with no leading `/`.
        assert_eq!(one("count(doc/attribute())", xml), "1");
    }

    #[test]
    fn abbrev_step_other_node_tests_keep_the_child_axis() {
        let xml = r#"<doc a="1"><num>1</num></doc>"#;
        assert_eq!(one("count(/doc/node())", xml), "1");
        assert_eq!(one("count(/doc/element())", xml), "1");
        assert_eq!(one("count(/doc/num)", xml), "1");
        assert_eq!(one("count(/doc/*)", xml), "1");
        // An explicit axis is never overridden by the node test.
        assert_eq!(one("count(/doc/child::attribute())", xml), "0");
    }

    // ---- §3.2.1.1 / §3.2.1.2: principal node kind -------------------------

    #[test]
    fn name_test_matches_only_the_principal_node_kind() {
        // "A name test is true if and only if the kind of the node is the
        // principal node kind for the step axis and the expanded QName of the
        // node is equal ... to the expanded QName specified by the name test."
        // For every axis but attribute:: and namespace:: that kind is element.
        let xml = r#"<doc><a x="1"/></doc>"#;
        assert_eq!(one("count(/doc/a/@x/self::*)", xml), "0");
        assert_eq!(one("count(/doc/a/@x/self::x)", xml), "0");
        assert_eq!(one("count(/doc/a/@x/ancestor-or-self::*)", xml), "2");
        assert_eq!(one("count(/doc/a/@x/descendant-or-self::*)", xml), "0");
        // Kind tests are unaffected: they select on node kind, not on the
        // axis's principal node kind.
        assert_eq!(one("count(/doc/a/@x/self::node())", xml), "1");
        assert_eq!(one("count(/doc/a/@x/ancestor-or-self::node())", xml), "4");
        // An element context still matches.
        assert_eq!(one("count(/doc/a/self::*)", xml), "1");
        assert_eq!(one("count(/doc/a/ancestor-or-self::*)", xml), "2");
    }

    #[test]
    fn name_test_on_attribute_and_namespace_axes() {
        // The attribute axis's principal node kind is attribute...
        let xml = r#"<doc xmlns:p="urn:x" a="1"><num/></doc>"#;
        assert_eq!(one("count(/doc/@*)", xml), "1");
        assert_eq!(items("/doc/@a", xml), ["attribute(a=1)"]);
        // ... and the namespace axis's is namespace (a name test there
        // selects by prefix).
        assert_eq!(one("count(/doc/namespace::*)", xml), "2");
        assert_eq!(items("/doc/namespace::p", xml), ["namespace(p=urn:x)"]);
        assert_eq!(one("count(/doc/namespace::nosuch)", xml), "0");
    }

    // ---- §3.2.1.1: the `following` axis -----------------------------------

    #[test]
    fn following_axis_covers_the_whole_rest_of_the_document() {
        // "all nodes that are descendants of the root of the tree ..., are not
        // descendants of the context node, and occur after the context node in
        // document order".
        let xml = "<doc><a><x/>t1</a><b><y/>t2</b></doc>";
        assert_eq!(one("count(/doc/a/following::node())", xml), "3");
        assert_eq!(
            items("/doc/a/following::node()", xml),
            ["element(b)", "element(y)", "Text(t2)"]
        );
        // From a node inside the first subtree the axis also reaches the
        // descendants of the following siblings.
        assert_eq!(one("count(/doc/a/x/following::node())", xml), "4");
        assert_eq!(one("count(/doc/a/x/following::*)", xml), "2");
        // The context node's own descendants are excluded.
        assert_eq!(one("count(/doc/following::node())", xml), "0");
        assert_eq!(one("count(/following::node())", xml), "0");
    }

    #[test]
    fn following_axis_from_attribute_and_namespace_nodes() {
        // An attribute precedes its owner's children in document order and has
        // no descendants, so the axis starts at the owner's first child.
        let xml = r#"<doc><a p="1"><x/>t1</a><b/></doc>"#;
        assert_eq!(one("count(/doc/a/@p/following::node())", xml), "3");
        assert_eq!(
            items("/doc/a/@p/following::node()", xml),
            ["element(x)", "Text(t1)", "element(b)"]
        );
        let ns_xml = r#"<doc><a xmlns:p="urn:x"><x/></a><b/></doc>"#;
        assert_eq!(
            items("/doc/a/namespace::p/following::node()", ns_xml),
            ["element(x)", "element(b)"]
        );
    }

    // ---- §3.2.1.1: the `preceding` axis -----------------------------------

    #[test]
    fn preceding_axis_is_delivered_in_reverse_document_order() {
        // A reverse axis presents its nodes in reverse document order, which
        // is the order a positional predicate's focus counts in; the path
        // expression as a whole still returns document order (§3.2).
        let xml = "<doc><a><x/>t1</a><b/></doc>";
        assert_eq!(one("count(/doc/b/preceding::node())", xml), "3");
        assert_eq!(
            items("/doc/b/preceding::node()", xml),
            ["element(a)", "element(x)", "Text(t1)"]
        );
        assert_eq!(items("/doc/b/preceding::node()[1]", xml), ["Text(t1)"]);
        assert_eq!(items("/doc/b/preceding::*[1]", xml), ["element(x)"]);
        assert_eq!(items("/doc/b/preceding::*[last()]", xml), ["element(a)"]);
        assert_eq!(
            items("/doc/b/preceding::node()[position() > 1]", xml),
            ["element(a)", "element(x)"]
        );
        // The forward axes keep counting in document order.
        assert_eq!(items("/doc/a/x/following::node()[1]", xml), ["Text(t1)"]);
        assert_eq!(
            items("/doc/a/x/following::node()[last()]", xml),
            ["element(b)"]
        );
    }

    #[test]
    fn preceding_axis_excludes_ancestors_and_is_empty_at_the_root() {
        let xml = "<doc><a><x/></a><b/></doc>";
        // Ancestors are never on the preceding axis.
        assert_eq!(
            items("/doc/a/x/preceding::node()", xml),
            Vec::<String>::new()
        );
        // The root of the tree has nothing before it.
        assert_eq!(one("count(/preceding::node())", xml), "0");
        assert_eq!(one("count(/doc/preceding::node())", xml), "0");
        // Nodes outside the document element are reachable.
        let pi_xml = "<?target data?><!--c--><doc><a/></doc>";
        assert_eq!(
            items("/doc/a/preceding::node()", pi_xml),
            ["pi(target)", "comment(c)"]
        );
        assert_eq!(items("/doc/a/preceding::node()[1]", pi_xml), ["comment(c)"]);
    }

    #[test]
    fn preceding_axis_from_attribute_and_namespace_nodes() {
        let xml = r#"<doc><a/><b c="1"/></doc>"#;
        assert_eq!(items("/doc/b/@c/preceding::node()", xml), ["element(a)"]);
        let ns_xml = r#"<doc><a/><b xmlns:p="urn:x"/></doc>"#;
        assert_eq!(
            items("/doc/b/namespace::p/preceding::node()", ns_xml),
            ["element(a)"]
        );
    }

    // ---- XDM: a namespace undeclaration is not a namespace node -----------

    #[test]
    fn namespace_undeclaration_is_not_a_namespace_node() {
        // `xmlns=""` removes the binding for the default prefix; it is the
        // absence of a binding, not a namespace node with a zero-length URI.
        let xml = r#"<chap xmlns="http://c/"><para xmlns=""/></chap>"#;
        assert_eq!(one("count(/*/namespace::*)", xml), "2");
        assert_eq!(one("count(/*/*/namespace::*)", xml), "1");
        assert_eq!(
            items("/*/*/namespace::*", xml),
            ["namespace(xml=http://www.w3.org/XML/1998/namespace)"]
        );
        assert_eq!(one("string-join(in-scope-prefixes(/*/*),',')", xml), "xml");
        // No default namespace is in scope on the inner element...
        assert_eq!(
            one("string(namespace-uri-for-prefix('', /*/*)) = ''", xml),
            "true"
        );
        // ... while the outer element keeps its binding.
        assert_eq!(one("namespace-uri-for-prefix('', /*)", xml), "http://c/");
        assert_eq!(one("string-join(in-scope-prefixes(/*),',')", xml), "xml,");
        // A prefixed undeclaration (Namespaces 1.1) is equally absent.
        let prefixed = r#"<chap xmlns:p="urn:x"><para xmlns:p=""/></chap>"#;
        assert_eq!(one("count(/*/namespace::*)", prefixed), "2");
        assert_eq!(one("count(/*/*/namespace::*)", prefixed), "1");
    }
}

/// Helpers shared by the specification-driven evaluator tests below.
mod spec_helpers {
    use crate::namespace::table::NameTable;
    use crate::xpath::api::XPathExpr;
    use crate::xpath::iterator::XmlItem;
    use crate::xpath::{DomNavigator, DomNodeType, RoXmlNavigator, XPathContext};

    /// One readable line per result item: `kind(detail)` for nodes, the lexical
    /// form for atomic values.
    fn describe<N: DomNavigator>(item: &XmlItem<N>) -> String {
        match item {
            XmlItem::Atomic(value) => value.to_string_value(),
            XmlItem::Node(nav) => match nav.node_type() {
                DomNodeType::Root => "document".to_string(),
                DomNodeType::Element => format!("element({})", nav.local_name()),
                DomNodeType::Attribute => {
                    format!("attribute({}={})", nav.local_name(), nav.value())
                }
                DomNodeType::Namespace => {
                    format!("namespace({}={})", nav.local_name(), nav.value())
                }
                DomNodeType::Comment => format!("comment({})", nav.value()),
                DomNodeType::ProcessingInstruction => format!("pi({})", nav.local_name()),
                other => format!("{other:?}({})", nav.value()),
            },
        }
    }

    /// Evaluate `expr` over `xml` with the document node as context item.
    pub(super) fn items(expr: &str, xml: &str) -> Vec<String> {
        try_items(expr, xml).unwrap_or_else(|e| panic!("{expr}: {e}"))
    }

    /// Evaluate an expression whose result is a single item.
    pub(super) fn one(expr: &str, xml: &str) -> String {
        let result = items(expr, xml);
        assert_eq!(result.len(), 1, "{expr}: expected one item, got {result:?}");
        result.into_iter().next().unwrap()
    }

    /// Evaluate `expr`, returning either the described items or the error's
    /// specification code (`"<no code>"` for an error without one).
    pub(super) fn try_items(expr: &str, xml: &str) -> Result<Vec<String>, String> {
        let names = NameTable::new();
        // The `xs` prefix is bound so the type expressions below can name the
        // built-in types the way an ordinary host document would.
        let mut namespaces = crate::namespace::context::NamespaceContextSnapshot::default();
        namespaces.bindings.push((
            names.add("xs"),
            names.add("http://www.w3.org/2001/XMLSchema"),
        ));
        let ctx = XPathContext::new(&names).with_namespaces(namespaces);
        let compiled = XPathExpr::compile(expr, &ctx).map_err(code_of)?;
        let doc = roxmltree::Document::parse(xml).expect("parse xml");
        let value = compiled
            .evaluator(&ctx)
            .run_with_node(RoXmlNavigator::new(&doc))
            .map_err(code_of)?;
        Ok(value.into_vec().iter().map(describe).collect())
    }

    /// The specification code an expression fails with, or a panic if it
    /// succeeds.
    pub(super) fn error_code(expr: &str, xml: &str) -> String {
        match try_items(expr, xml) {
            Ok(items) => panic!("{expr}: expected an error, got {items:?}"),
            Err(code) => code,
        }
    }

    fn code_of(e: crate::xpath::XPathError) -> String {
        e.error_code()
            .map(|c| c.to_string())
            .unwrap_or_else(|| format!("<no code> {e}"))
    }
}

/// XPath 2.0 §3.2 — the semantics of the `/` operator: a per-item inner focus,
/// results combined in document order without duplicates, and no mixing of
/// nodes with atomic values.
mod path_operator_tests {
    use super::spec_helpers::{error_code, items, one};

    /// Two `o` elements with two `i` children each.
    const NESTED: &str =
        r#"<r><o id="1"><i>a1</i><i>a2</i></o><o id="2"><i>b1</i><i>b2</i></o></r>"#;

    #[test]
    fn step_predicates_see_one_context_node_at_a_time() {
        // §3.2.2: "The context size is the number of items in the input
        // sequence" — the input sequence of a step's predicate is what that
        // step produced for one context node, not the concatenation.
        assert_eq!(one(r#"string-join(/r/o/i[last()],",")"#, NESTED), "a2,b2");
        assert_eq!(
            one(r#"string-join(/r/o/i[position()=1],",")"#, NESTED),
            "a1,b1"
        );
        assert_eq!(one(r#"string-join(/r/o/i[1],",")"#, NESTED), "a1,b1");
        assert_eq!(
            one(r#"string-join(/r/o/i[position()>1],",")"#, NESTED),
            "a2,b2"
        );
        // Parentheses turn the step into a primary expression, and then the
        // predicate applies to the whole sequence.
        assert_eq!(one(r#"string-join((/r/o/i)[last()],",")"#, NESTED), "b2");
        assert_eq!(one(r#"string-join((/r/o/i)[1],",")"#, NESTED), "a1");
    }

    #[test]
    fn a_non_step_right_operand_sees_the_position_in_the_left_operand() {
        // §2.1.2: "the context size in the inner focus for an evaluation of E2
        // is the number of items in the sequence obtained by evaluating E1".
        // E1 is `/r/o/i`, which has four items.
        assert_eq!(
            one(r#"string-join(/r/o/i/string(last()),",")"#, NESTED),
            "4,4,4,4"
        );
        assert_eq!(
            one(r#"string-join(/r/o/i/string(position()),",")"#, NESTED),
            "1,2,3,4"
        );
        // E1 is `/r/o`, which has two.
        assert_eq!(
            one(r#"string-join(/r/o/concat(@id,":",last()),",")"#, NESTED),
            "1:2,2:2"
        );
    }

    #[test]
    fn node_results_are_combined_in_document_order() {
        let doc = r#"<doc><item val="1"/><item val="2"/><item val="3"/><item val="4"/><item val="5"/></doc>"#;
        // §3.2: "The resulting node sequence is returned in document order."
        assert_eq!(
            one(
                r#"string-join(/doc/(item[5],item[3],item[2])/@val,",")"#,
                doc
            ),
            "2,3,5"
        );
        assert_eq!(
            one(
                r#"string-join((/r/o[@id="2"],/r/o[@id="1"])/i,",")"#,
                NESTED
            ),
            "a1,a2,b1,b2"
        );
        // A reverse axis feeding an atomizing step: the reverse-order axis
        // result is put into document order before the step runs.
        assert_eq!(
            one(
                r#"string-join(/doc/a/ancestor::node()/name(),"|")"#,
                "<doc><a/></doc>"
            ),
            "|doc"
        );
        assert_eq!(
            one(
                r#"string-join(/r/o/i/ancestor-or-self::*/name(),"|")"#,
                NESTED
            ),
            "r|o|i|i|o|i|i"
        );
    }

    #[test]
    fn duplicate_nodes_are_eliminated() {
        // §3.2: "duplicate nodes are eliminated based on node identity".
        assert_eq!(one("count((/r/o,/r/o)/i)", NESTED), "4");
        assert_eq!(one("count(./(//i,//i))", NESTED), "4");
        assert_eq!(one("count((/r/o,/r/o)/@id)", NESTED), "2");
        // The axis iterators themselves were never the problem.
        assert_eq!(one("count(//i/ancestor::r)", NESTED), "1");
        assert_eq!(one("count(//i/parent::o)", NESTED), "2");
    }

    #[test]
    fn a_mix_of_nodes_and_atomic_values_is_a_type_error() {
        // §3.2: "If the multiple evaluations of E2 return at least one node and
        // at least one atomic value, a type error is raised [err:XPTY0018]."
        let doc = r#"<doc><item val="1"/><item val="2"/><item val="3"/></doc>"#;
        assert_eq!(
            error_code(
                "count(//item/(if (position()=3) then @* else string(@val)))",
                doc
            ),
            "XPTY0018"
        );
        // A single evaluation that mixes them is the same error.
        assert_eq!(error_code("/doc/(item,'x')", doc), "XPTY0018");
        // All-atomic and all-node results are both fine.
        assert_eq!(one(r#"string-join(/doc/item/@val,",")"#, doc), "1,2,3");
        assert_eq!(one("count(/doc/item)", doc), "3");
        // An empty evaluation counts as a node sequence, not as a mix.
        assert_eq!(one("count(/doc/item/(self::item[@val='2']))", doc), "1");
    }

    #[test]
    fn an_atomic_left_operand_is_a_type_error() {
        // §3.2: "Expression E1 is evaluated, and if the result is not a
        // (possibly empty) sequence of nodes, a type error is raised
        // [err:XPTY0019]."
        let doc = "<doc><a/></doc>";
        assert_eq!(error_code("/doc/name()/a", doc), "XPTY0019");
        assert_eq!(error_code("/doc/name()/string(.)", doc), "XPTY0019");
    }

    #[test]
    fn document_order_holds_for_nested_and_repeated_origins() {
        // Nested elements of the same name: the concatenation of the per-origin
        // child sequences is not in document order, so it has to be sorted.
        let nested = "<doc><a><b>1</b><a><b>2</b></a><b>3</b></a></doc>";
        assert_eq!(one(r#"string-join(//a/b,",")"#, nested), "1,2,3");
        assert_eq!(one("count(//a/b)", nested), "3");
        assert_eq!(one(r#"string-join(//a//b,",")"#, nested), "1,2,3");
        // `following-sibling` from several origins can select the same node
        // twice.
        let flat = "<doc><a/><a/><a/></doc>";
        assert_eq!(one("count(/doc/a/following-sibling::a)", flat), "2");
        assert_eq!(one("count(/doc/a/preceding-sibling::a)", flat), "2");
        assert_eq!(one("count(/doc/a/parent::doc)", flat), "1");
    }

    #[test]
    fn an_absolute_path_starting_with_a_primary_expression_sees_the_root() {
        // A leading "/" is an abbreviation for an initial step that yields the
        // root of the tree (§3.2), so a primary expression written as the first
        // step is evaluated with that root as its context item.
        let doc = "<doc><a/><a/></doc>";
        assert_eq!(items("/(doc)", doc), ["element(doc)"]);
        assert_eq!(one("count(/(doc/a))", doc), "2");
        assert_eq!(items("/(.)", doc), ["document"]);
    }
}

/// XPath 2.0 §3.4 / §3.5.1 / §3.5.2 — the error behaviour of the operators.
mod operator_error_tests {
    use super::spec_helpers::{error_code, items, one};

    #[test]
    fn an_operator_type_error_carries_the_type_error_code() {
        // §3.4: "If the types of the operands, after evaluation, are not a valid
        // combination for the given operator, according to the rules in
        // B.2 Operator Mapping, a type error is raised [err:XPTY0004]."
        assert_eq!(error_code("3 + '2'", "<a/>"), "XPTY0004");
        assert_eq!(error_code("3 - true()", "<a/>"), "XPTY0004");
        assert_eq!(error_code("-'a'", "<a/>"), "XPTY0004");
        // §3.5.1 for the value comparisons.
        assert_eq!(error_code("3 eq '2'", "<a/>"), "XPTY0004");
        assert_eq!(error_code("'a' lt 1", "<a/>"), "XPTY0004");
    }

    #[test]
    fn a_general_comparison_does_not_swallow_a_type_error() {
        // §3.5.2 defers each pair to the corresponding value comparison, and
        // §3.5.1 makes an incomparable pair a type error. `xs:string` against
        // `xs:integer` is not a valid combination, so neither `=` nor `!=` may
        // report a boolean.
        assert_eq!(error_code("'001' = 1", "<a/>"), "XPTY0004");
        assert_eq!(error_code("'001' != 1", "<a/>"), "XPTY0004");
        assert_eq!(error_code("'a' < 1", "<a/>"), "XPTY0004");
        assert_eq!(error_code("'a' >= 1", "<a/>"), "XPTY0004");
    }

    #[test]
    fn a_true_pair_still_short_circuits_a_general_comparison() {
        // §3.5.2: "an implementation may return true as soon as it finds an item
        // in the first operand and an item in the second operand that have the
        // required magnitude relationship."
        assert_eq!(one("(1,'a') = 1", "<a/>"), "true");
        assert_eq!(one("(1,2) = (2,3)", "<a/>"), "true");
        assert_eq!(one("(1,2) != (2,3)", "<a/>"), "true");
        // …but with no true pair the incomparable pair decides.
        assert_eq!(error_code("(1,'a') = 9", "<a/>"), "XPTY0004");
        assert_eq!(error_code("(1,'a') != 1", "<a/>"), "XPTY0004");
    }

    #[test]
    fn an_operand_sequence_of_more_than_one_item_is_a_type_error() {
        // §3.4 and §3.5.1: "If the atomized operand is a sequence of length
        // greater than one, a type error is raised [err:XPTY0004]." The same
        // rule covers `cast` (§3.10.2) and, through the function conversion
        // rules, `to` (§3.3.1).
        assert_eq!(error_code("(2,3,4) eq (2,3)", "<a/>"), "XPTY0004");
        assert_eq!(error_code("(1,2) + 1", "<a/>"), "XPTY0004");
        assert_eq!(error_code("-(1,2)", "<a/>"), "XPTY0004");
        assert_eq!(error_code("(1,2) to 3", "<a/>"), "XPTY0004");
        assert_eq!(error_code("(1,2) cast as xs:integer", "<a/>"), "XPTY0004");
        // An empty operand is still the empty sequence, not an error.
        assert_eq!(items("() eq 1", "<a/>"), Vec::<String>::new());
        assert_eq!(items("() + 1", "<a/>"), Vec::<String>::new());
    }

    #[test]
    fn comparable_operands_are_untouched() {
        // Guard: the type errors above must not leak into valid combinations.
        assert_eq!(one("1 = 1", "<a/>"), "true");
        assert_eq!(one("1 = 2", "<a/>"), "false");
        assert_eq!(one("'a' = 'a'", "<a/>"), "true");
        assert_eq!(one("'a' != 'b'", "<a/>"), "true");
        assert_eq!(one("1 + 1", "<a/>"), "2");
        assert_eq!(one("1.5 lt 2", "<a/>"), "true");
        // An untypedAtomic node value is cast, not rejected (§3.5.2).
        assert_eq!(one("/a/@n = 1", r#"<a n="1"/>"#), "true");
        assert_eq!(one("/a/@n != 2", r#"<a n="1"/>"#), "true");
    }
}

/// XPath 2.0 §3.10.5 — `treat as` raises a dynamic error on every failure path.
mod treat_as_tests {
    use super::spec_helpers::{error_code, items, one};

    #[test]
    fn a_failed_treat_is_a_dynamic_error() {
        // "If expr1 matches type1, using the rules for SequenceType matching,
        // the treat expression returns the value of expr1; otherwise, it raises
        // a dynamic error [err:XPDY0050]."
        assert_eq!(error_code("(23.5) treat as xs:integer", "<a/>"), "XPDY0050");
        assert_eq!(
            error_code("(23,24) treat as xs:decimal", "<a/>"),
            "XPDY0050"
        );
        assert_eq!(error_code("() treat as xs:integer", "<a/>"), "XPDY0050");
        assert_eq!(error_code("/a treat as text()", "<a/>"), "XPDY0050");
        assert_eq!(
            error_code("1 treat as empty-sequence()", "<a/>"),
            "XPDY0050"
        );
        assert_eq!(error_code("(1,2) treat as item()", "<a/>"), "XPDY0050");
        assert_eq!(error_code("() treat as item()+", "<a/>"), "XPDY0050");
    }

    #[test]
    fn a_matching_treat_returns_its_operand() {
        assert_eq!(one("1 treat as xs:integer", "<a/>"), "1");
        assert_eq!(items("/a treat as element()", "<a/>"), ["element(a)"]);
        assert_eq!(
            items("() treat as xs:integer?", "<a/>"),
            Vec::<String>::new()
        );
        // empty-sequence() has no occurrence indicator of its own (§2.5.4).
        assert_eq!(
            items("() treat as empty-sequence()", "<a/>"),
            Vec::<String>::new()
        );
        assert_eq!(one("count((1,2) treat as xs:integer+)", "<a/>"), "2");
    }
}

/// XPath 2.0 §2.5.4.2 / §3.10.2 / §3.10.3 — a QName used as an `AtomicType`
/// must name an atomic type in the in-scope schema types.
mod atomic_type_name_tests {
    use super::spec_helpers::{error_code, one};

    #[test]
    fn an_unknown_type_name_is_a_static_error() {
        // "If a QName that is used as an AtomicType is not defined as an atomic
        // type in the in-scope schema types, a static error is raised
        // [err:XPST0051]."
        assert_eq!(
            error_code("'abc' instance of nosuchtype", "<a/>"),
            "XPST0051"
        );
        assert_eq!(error_code("'abc' treat as nosuchtype", "<a/>"), "XPST0051");
        assert_eq!(error_code("'abc' cast as nosuchtype", "<a/>"), "XPST0051");
        assert_eq!(
            error_code("'abc' castable as nosuchtype", "<a/>"),
            "XPST0051"
        );
        // An unprefixed name is in the default element/type namespace, which is
        // absent here — so `string` is not `xs:string`.
        assert_eq!(error_code("'abc' instance of string", "<a/>"), "XPST0051");
        assert_eq!(error_code("'abc' castable as double", "<a/>"), "XPST0051");
    }

    #[test]
    fn a_non_atomic_schema_type_name_is_a_static_error() {
        // The spec's own note: "The names of non-atomic types such as xs:IDREFS
        // are not accepted."
        assert_eq!(error_code("1 instance of xs:IDREFS", "<a/>"), "XPST0051");
        assert_eq!(error_code("1 instance of xs:NMTOKENS", "<a/>"), "XPST0051");
        assert_eq!(error_code("1 instance of xs:ENTITIES", "<a/>"), "XPST0051");
        assert_eq!(error_code("3 cast as xs:IDREFS", "<a/>"), "XPST0051");
        // `xs:anyType` and `xs:anySimpleType` are not atomic types either.
        assert_eq!(error_code("3 cast as xs:anyType", "<a/>"), "XPST0051");
        assert_eq!(error_code("3 cast as xs:anySimpleType", "<a/>"), "XPST0051");
        assert_eq!(error_code("3 instance of xs:anyType", "<a/>"), "XPST0051");
    }

    #[test]
    fn the_atomic_type_names_keep_working() {
        // Guard: the check must not reject a legitimate AtomicType.
        assert_eq!(one("'abc' instance of xs:string", "<a/>"), "true");
        assert_eq!(one("1 instance of xs:integer", "<a/>"), "true");
        // `xs:integer` derives from `xs:decimal`, so subtype substitution
        // applies (§2.5.4.2).
        assert_eq!(one("1 instance of xs:decimal", "<a/>"), "true");
        assert_eq!(one("1 instance of xs:string", "<a/>"), "false");
        assert_eq!(one("1 castable as xs:string", "<a/>"), "true");
        assert_eq!(one("'3' cast as xs:integer", "<a/>"), "3");
        assert_eq!(one("xs:integer('3')", "<a/>"), "3");
        // `xs:anyAtomicType` is the base of the atomic types, and is itself a
        // legal AtomicType in a SequenceType.
        assert_eq!(one("'abc' instance of xs:anyAtomicType", "<a/>"), "true");
        // `xs:NOTATION` is an atomic type, so naming it is not a static error.
        assert_eq!(one("'abc' instance of xs:NOTATION", "<a/>"), "false");
    }
}

/// XPath 2.0 §3.1.5 and §3.4 — the conversions that XPath 1.0 compatibility
/// mode adds, and the guarantee that they do nothing when the flag is off.
mod xpath10_compatibility_conversion_tests {
    use crate::namespace::table::NameTable;
    use crate::xpath::api::XPathExpr;
    use crate::xpath::{RoXmlNavigator, XPathContext, XPathError};

    /// Evaluate `expr` in XPath 2.0 syntax, with compatibility mode on or off.
    fn eval(expr: &str, compat: bool) -> Result<String, XPathError> {
        let names = NameTable::new();
        let mut namespaces = crate::namespace::context::NamespaceContextSnapshot::default();
        namespaces.bindings.push((
            names.add("xs"),
            names.add("http://www.w3.org/2001/XMLSchema"),
        ));
        let ctx = XPathContext::new(&names)
            .with_namespaces(namespaces)
            .with_xpath10_compatibility(compat);
        let value = XPathExpr::compile(expr, &ctx)?
            .evaluator(&ctx)
            .run::<RoXmlNavigator<'static>>()?;
        Ok(value
            .into_vec()
            .iter()
            .map(|item| match item {
                crate::xpath::iterator::XmlItem::Atomic(v) => v.to_string_value(),
                crate::xpath::iterator::XmlItem::Node(_) => "node".to_string(),
            })
            .collect::<Vec<_>>()
            .join(","))
    }

    /// Evaluate `expr` over `xml` with the document node as context item.
    fn eval_on(expr: &str, xml: &str, compat: bool) -> Result<String, XPathError> {
        let names = NameTable::new();
        let ctx = XPathContext::new(&names).with_xpath10_compatibility(compat);
        let doc = ::roxmltree::Document::parse(xml).expect("parse xml");
        let value = XPathExpr::compile(expr, &ctx)?
            .evaluator(&ctx)
            .run_with_node(RoXmlNavigator::new(&doc))?;
        Ok(value
            .into_vec()
            .iter()
            .map(|item| match item {
                crate::xpath::iterator::XmlItem::Atomic(v) => v.to_string_value(),
                crate::xpath::iterator::XmlItem::Node(_) => "node".to_string(),
            })
            .collect::<Vec<_>>()
            .join(","))
    }

    fn on(expr: &str) -> String {
        eval(expr, true).unwrap_or_else(|e| panic!("{expr}: {e}"))
    }

    fn off(expr: &str) -> Result<String, XPathError> {
        eval(expr, false)
    }

    #[test]
    fn function_arguments_take_the_first_item_and_are_converted() {
        // §3.1.5: "If the expected type calls for a single item or optional
        // single item …, then the value V is effectively replaced by V[1]."
        assert_eq!(on("string-length(( 'abcd', 'xy' ))"), "4");
        assert_eq!(on("upper-case(('a','b'))"), "A");
        // "If the expected type is xs:double or xs:double?, then the value V is
        // effectively replaced by fn:number(V)."
        assert_eq!(on("round(concat('20','.7'))"), "21");
        assert_eq!(on("floor('2.7')"), "2");
        assert_eq!(on("round((3.6, 9))"), "4");
        // "If the expected type is xs:string or xs:string?, then the value V is
        // effectively replaced by fn:string(V)."
        assert_eq!(on("upper-case(12)"), "12");
        assert_eq!(on("string-length(true())"), "4");
        // An argument whose expected type allows a sequence is untouched.
        assert_eq!(on("count((1,2,3))"), "3");
        assert_eq!(on("string-join(('a','b'),'-')"), "a-b");
    }

    #[test]
    fn arithmetic_operands_take_the_first_item_and_become_numbers() {
        // §3.4, compatibility mode: first item, then fn:number, and an empty
        // operand makes the whole expression NaN.
        assert_eq!(on("1 + (6 to 10)"), "7");
        assert_eq!(on("(6 to 10) + 1"), "7");
        assert_eq!(on("1 + ()"), "NaN");
        assert_eq!(on("() + 1"), "NaN");
        assert_eq!(on("() * 3"), "NaN");
        assert_eq!(on("-()"), "NaN");
        assert_eq!(on("-(2,3)"), "-2");
        assert_eq!(on("'3' * '4'"), "12");
        assert_eq!(on("1 + true()"), "2");
        assert_eq!(on("'x' + 1"), "NaN");
        // XPath 1.0 arithmetic is always double, and division by zero is not an
        // error.
        assert_eq!(on("1 div 0"), "INF");
        assert_eq!(on("1 + 2"), "3");
        // The date, time and duration types are not in §3.4's conversion list,
        // so XPath 2.0's operator mapping still applies to them.
        assert_eq!(on("xs:date('2001-01-01') - xs:date('2000-01-01')"), "P366D");
        assert_eq!(
            on("xs:date('2000-01-01') + xs:dayTimeDuration('P1D')"),
            "2000-01-02"
        );
        // `idiv` has no XPath 1.0 counterpart and keeps its integer result.
        assert_eq!(on("7 idiv 2"), "3");
    }

    #[test]
    fn a_range_operand_takes_its_first_item() {
        // §3.3.1 types the operands of `to` as xs:integer?, a single optional
        // item, so only the first §3.1.5 step applies.
        assert_eq!(on("(1,2) to 3"), "1,2,3");
        assert_eq!(on("1 to (3,9)"), "1,2,3");
        // An empty operand still yields the empty sequence, not NaN: the NaN
        // rule belongs to the arithmetic operators.
        assert_eq!(on("() to 3"), "");
        assert_eq!(on("1 to ()"), "");
    }

    /// §3.1.5 gates all three compatibility steps on "**and an argument is not
    /// of the expected type**": an argument that already matches its
    /// parameter's declared sequence type is handed to the function untouched.
    #[test]
    fn an_argument_already_of_the_expected_type_is_not_converted() {
        // `fn:compare($comparand1 as xs:string?, $comparand2 as xs:string?)`:
        // `()` is a value of type `xs:string?`, so no `V[1]`, no `fn:string`,
        // and `fn:compare` sees the empty sequence it is specified to return.
        assert_eq!(on("compare((), '')"), "");
        assert_eq!(on("compare('', ())"), "");
        assert_eq!(on("empty(compare((), ''))"), "true");
        // `fn:resolve-uri($relative as xs:string?, $base as xs:string)`:
        // likewise, and so no base URI comes back out of it.
        assert_eq!(on("resolve-uri((), 'http://example.com/')"), "");
        // An optional numeric parameter behaves the same way.
        assert_eq!(on("round(())"), "");
        assert_eq!(on("floor(())"), "");
        // A single item of the declared type is handed over as it stands.
        assert_eq!(on("string-length('abcd')"), "4");
        assert_eq!(on("round(xs:double('1.5'))"), "2");
    }

    /// The positive half of the same rule: everything that is *not* of the
    /// expected type still goes through the three steps.
    #[test]
    fn an_argument_not_of_the_expected_type_is_still_converted() {
        const XML: &str = "<r><a>7</a></r>";

        // A node where `xs:string?` is expected is not of the expected type —
        // the steps run before atomization — so `fn:string(V)` applies.
        assert_eq!(eval_on("string-length(/r/a)", XML, true).unwrap(), "1");
        assert_eq!(eval_on("upper-case(/r/a)", XML, true).unwrap(), "7");
        // A node where `xs:double?` is expected becomes `fn:number(V)`.
        assert_eq!(eval_on("round(/r/a)", XML, true).unwrap(), "7");
        // More than one item: `V[1]`, then the step for the item type.
        assert_eq!(on("string-length(('abcd','xy'))"), "4");
        assert_eq!(on("compare(('b','zz'), 'b')"), "0");
        // The empty sequence where a *required* single item is expected does
        // not match the cardinality, so it is converted:
        // `fn:string(())` is the zero-length string, and `fn:string-join`
        // joins with it instead of raising XPTY0004 as it does with the flag
        // off.
        assert_eq!(on("string-join(('a','b'), ())"), "ab");
        // … and `fn:number(())` is NaN, which `fn:substring` then reports by
        // selecting no characters.
        assert_eq!(on("substring('hello', ())"), "");
        assert_eq!(on("string-length(substring('hello', ()))"), "0");
        // An xs:untypedAtomic and an xs:integer are not of type `xs:string`
        // either (neither derives from it, and neither promotes to it).
        assert_eq!(on("upper-case(12)"), "12");
        assert_eq!(on("string-length(true())"), "4");
        // An xs:string is not of type `xs:double?`, so `fn:number` applies.
        assert_eq!(on("floor('2.7')"), "2");
        // Nor is an xs:decimal, so XPath 1.0's all-arithmetic-is-double shows
        // through in the result type where XPath 2.0 keeps the decimal.
        assert_eq!(on("round(1.5) instance of xs:double"), "true");
        assert_eq!(off("round(1.5) instance of xs:double").unwrap(), "false");
    }

    /// The condition is about the argument's *static* type, so a path
    /// expression that happens to select **no** nodes is still not of the
    /// expected type: its static type is a node sequence, not `xs:double?`.
    /// The literal `()` is the one expression XPath 2.0 §2.3.4 allows to have
    /// the static type `empty-sequence()`, and that is what makes
    /// `fn:compare((), '')` different from `fn:round(doc/none)`.
    #[test]
    fn an_empty_path_expression_is_not_of_the_expected_type() {
        const XML: &str = "<doc><a>7</a></doc>";

        // `round()` over a path that selects no nodes, and `floor()` over a
        // name that does not exist: both are NaN, which is what a host language
        // that switches this flag on for a 1.0-era expression expects.
        assert_eq!(eval_on("round(doc/none)", XML, true).unwrap(), "NaN");
        assert_eq!(eval_on("floor(nonexistent)", XML, true).unwrap(), "NaN");
        assert_eq!(eval_on("round(/doc/none)", XML, true).unwrap(), "NaN");
        assert_eq!(eval_on("round(doc/a[2])", XML, true).unwrap(), "NaN");
        // The same for a parameter expecting `xs:string?`: an empty path
        // becomes `fn:string(())`, the zero-length string, so `fn:compare`
        // returns a number rather than the empty sequence.
        assert_eq!(eval_on("compare(doc/none, '')", XML, true).unwrap(), "0");
        // …while the literal empty sequence is of the expected type.
        assert_eq!(eval_on("compare((), '')", XML, true).unwrap(), "");
        assert_eq!(eval_on("round(())", XML, true).unwrap(), "");
        // Parentheses around the literal do not change its type.
        assert_eq!(eval_on("round((()))", XML, true).unwrap(), "");
        assert_eq!(eval_on("compare((()), '')", XML, true).unwrap(), "");
        // Under XPath 2.0 rules all four are the empty sequence.
        assert_eq!(eval_on("round(doc/none)", XML, false).unwrap(), "");
        assert_eq!(eval_on("compare(doc/none, '')", XML, false).unwrap(), "");
        assert_eq!(eval_on("round(())", XML, false).unwrap(), "");
        assert_eq!(eval_on("compare((), '')", XML, false).unwrap(), "");
    }

    /// `fn:number` of an untyped node in compatibility mode: `V[1]`, then the
    /// typed value — the string value as `xs:untypedAtomic` — converted as F&O
    /// §14.4 converts it, with the XML Schema lexical forms of `xs:double`.
    #[test]
    fn compatibility_mode_number_of_an_untyped_node() {
        const TWO: &str = "<r><a>7</a><a>8</a></r>";
        let cases: &[(&str, &str, bool, &str)] = &[
            // XML whitespace around the number is ignored.
            ("number(/a)", "<a> 12 </a>", true, "12"),
            ("/a/number()", "<a> 12 </a>", true, "12"),
            // `Infinity` is not an `xs:double` lexical form (`INF` is), and a
            // no-break space is not XML whitespace — with the flag as without.
            ("number(/a)", "<a>Infinity</a>", true, "NaN"),
            ("/a/number()", "<a>Infinity</a>", true, "NaN"),
            ("number(/a)", "<a>\u{a0}12</a>", true, "NaN"),
            ("number(/a)", "<a>INF</a>", true, "INF"),
            ("number(/a)", "<a>Infinity</a>", false, "NaN"),
            ("/a/number()", "<a>Infinity</a>", false, "NaN"),
            ("number(/a)", "<a>\u{a0}12</a>", false, "NaN"),
            ("number(/a)", "<a>INF</a>", false, "INF"),
            // More than one node: the first item…
            ("number(//a)", TWO, true, "7"),
            ("number((//a)[. > 7])", TWO, true, "8"),
            // …in sequence order, not document order.
            ("number(((//a)[2], //a[1]))", TWO, true, "8"),
            // With the flag off, more than one item is a type error.
            ("number(//a)", TWO, false, "error:XPTY0004"),
        ];
        let wrong: Vec<String> = cases
            .iter()
            .filter_map(|&(expr, xml, compat, expected)| {
                let got = outcome(eval_on(expr, xml, compat));
                (got != expected).then(|| {
                    format!(
                        "{expr} over {xml} (compat {compat})  =>  {got}   (expected {expected})"
                    )
                })
            })
            .collect();
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    #[test]
    fn nothing_changes_when_the_flag_is_off() {
        // The same expressions under XPath 2.0 rules.
        assert!(off("round(concat('20','.7'))").is_err());
        assert!(off("1 + (6 to 10)").is_err());
        assert!(off("-(2,3)").is_err());
        assert!(off("(1,2) to 3").is_err());
        assert!(off("'3' * '4'").is_err());
        assert!(off("1 + true()").is_err());
        assert_eq!(off("1 + ()").unwrap(), "");
        assert_eq!(off("() to 3").unwrap(), "");
        assert_eq!(off("string-length(('abcd'))").unwrap(), "4");
        assert_eq!(off("count((1,2,3))").unwrap(), "3");
        assert_eq!(off("1 + 2").unwrap(), "3");
        assert_eq!(off("7 idiv 2").unwrap(), "3");
        assert_eq!(
            off("xs:date('2001-01-01') - xs:date('2000-01-01')").unwrap(),
            "P366D"
        );
        // `upper-case(12)` is a pre-existing leniency in the function library's
        // string coercion, not something compatibility mode introduces: it
        // yields "12" with the flag either way, where §3.1.5 (flag off) calls
        // for XPTY0004 because xs:integer does not promote to xs:string.
        assert_eq!(off("upper-case(12)").unwrap(), "12");
        assert_eq!(on("upper-case(12)"), "12");
        // The arguments that already match their declared type behave under
        // XPath 2.0 exactly as they now do with the flag on, which is the
        // point of the §3.1.5 guard.
        assert_eq!(off("compare((), '')").unwrap(), "");
        assert_eq!(off("resolve-uri((), 'http://example.com/')").unwrap(), "");
        assert_eq!(off("round(())").unwrap(), "");
        assert_eq!(off("floor(())").unwrap(), "");
        assert_eq!(off("string-length('abcd')").unwrap(), "4");
        // …and the ones that do not match still differ: under XPath 2.0 they
        // are type errors, not conversions.
        assert!(off("string-length(('abcd','xy'))").is_err());
        assert!(off("string-join(('a','b'), ())").is_err());
        // `fn:substring`'s own numeric coercion is lenient with the flag
        // either way — another pre-existing leniency of the function library,
        // not something compatibility mode introduces.
        assert_eq!(off("substring('hello', ())").unwrap(), "");
    }

    /// The error code of an evaluation, or its value.
    fn outcome(result: Result<String, XPathError>) -> String {
        match result {
            Ok(value) => value,
            Err(err) => format!("error:{}", err.error_code().unwrap_or("?")),
        }
    }

    /// Review finding X-03. XPath 2.0 §2.4.3 defines the effective boolean value
    /// as "the result of applying the fn:boolean function to the value" — "In
    /// all other cases, fn:boolean raises a type error [err:FORG0006]" — with no
    /// compatibility-mode exception, and lists "General comparisons, in XPath
    /// 1.0 compatibility mode" among its users. Compatibility mode changes the
    /// function conversion rules, arithmetic, general comparisons and the order
    /// of `and`/`or`; it never makes a sequence of two atomic values `true`.
    #[test]
    fn compatibility_mode_keeps_the_xpath20_effective_boolean_value() {
        let xml = "<doc><a>1</a><a>2</a></doc>";
        for expr in [
            "boolean((1,2))",
            "not((1,2))",
            "(1,2) and 1",
            "(1, 2) or false()",
            "(1,2)[(1,2)]",
            "/doc/a[(1, 'x')]",
            "if ((1,2)) then 1 else 0",
            "some $x in (1) satisfies (1,2)",
        ] {
            for compat in [false, true] {
                assert_eq!(
                    outcome(eval_on(expr, xml, compat)),
                    "error:FORG0006",
                    "{expr} (compatibility mode {compat})"
                );
            }
        }
        // §3.5.2 rule 1 takes the *effective boolean value* of the operand
        // compared with a boolean: FORG0006 for two atomic values. (Without the
        // flag the comparison itself is XPTY0004, integer against boolean.)
        for expr in ["(1,2) = true()", "true() != (1, 2)", "(1,2) < true()"] {
            assert_eq!(
                outcome(eval_on(expr, xml, true)),
                "error:FORG0006",
                "{expr}"
            );
            assert_eq!(
                outcome(eval_on(expr, xml, false)),
                "error:XPTY0004",
                "{expr}"
            );
        }
        // What compatibility mode does change stays changed: a single boolean
        // operand converts the other operand by its effective boolean value
        // (§3.5.2) — for a node sequence, an empty sequence, a string, a number…
        for (expr, expected) in [
            ("/doc/a = true()", "true"),
            ("/doc/none = true()", "false"),
            ("/doc/none = false()", "true"),
            ("'abc' = true()", "true"),
            ("0 = false()", "true"),
            ("1 != true()", "false"),
            // … and then `<` … `>=` compare the two as numbers (§3.5.2 rule
            // 3): "true() > number('0.5') is … false in XPath 2.0 even when
            // compatibility mode is set to true" (Appendix I.1).
            ("true() > number('0.5')", "false"),
            ("true() > false()", "true"),
            // A node sequence's effective boolean value is fine with any
            // number of nodes.
            ("boolean(/doc/a)", "true"),
            ("not(/doc/a)", "false"),
            ("/doc/a and 1", "true"),
            ("count(/doc/a[/doc/a])", "2"),
            ("boolean(('', 1))", "error:FORG0006"),
        ] {
            assert_eq!(outcome(eval_on(expr, xml, true)), expected, "{expr}");
        }
    }
}

/// Helper for the expression-level tests below: compile and evaluate `expr`
/// with no context item, and return its string value.
#[cfg(test)]
fn eval_to_string(expr: &str) -> Result<String, XPathError> {
    use crate::namespace::context::NamespaceContextSnapshot;
    use crate::namespace::table::NameTable;
    use crate::xpath::api::XPathExpr;
    use crate::xpath::RoXmlNavigator;

    let names = NameTable::new();
    let mut namespaces = NamespaceContextSnapshot::default();
    namespaces.bindings.push((
        names.add("xs"),
        names.add("http://www.w3.org/2001/XMLSchema"),
    ));
    let ctx = XPathContext::new(&names).with_namespaces(namespaces);
    XPathExpr::compile(expr, &ctx)?
        .evaluator(&ctx)
        .run_string::<RoXmlNavigator<'static>>()
}

/// XPath 2.0 §A.2.1: `DecimalLiteral ::= ("." Digits) | (Digits "." [0-9]*)`.
#[test]
fn decimal_literal_with_a_trailing_period_evaluates() {
    assert_eq!(eval_to_string("5.").unwrap(), "5");
    assert_eq!(eval_to_string("5. + 1").unwrap(), "6");
    assert_eq!(eval_to_string("5. instance of xs:decimal").unwrap(), "true");
    assert_eq!(eval_to_string("0.").unwrap(), "0");
    // The other two forms are unchanged.
    assert_eq!(eval_to_string(".5 + 0.5").unwrap(), "1");
}

/// Datatypes §D.2.2: a timezone offset must match `timezoneFrag` and month and
/// day fields `monthFrag` / `dayFrag`; nothing may follow a negative-year
/// date. A cast from such a string raises FORG0001 through `cast as` and the
/// `xs:*` constructor functions alike, while `castable as` answers `false` —
/// it never raises a cast error.
#[test]
fn casts_reject_malformed_timezones_and_date_fields() {
    for (lexical, ty) in [
        ("09:15:00+15:00", "time"),
        ("09:15:00+01:-30", "time"),
        ("2002-10-10+1:30", "date"),
        ("2002-2-15", "date"),
        ("-2024-01-15-junk", "date"),
        ("2002-10-10T09:15:00+14:01", "dateTime"),
    ] {
        let cast = eval_to_string(&format!("'{lexical}' cast as xs:{ty}"));
        assert!(
            matches!(cast, Err(XPathError::FORG0001 { .. })),
            "'{lexical}' cast as xs:{ty}: {cast:?}"
        );
        let constructor = eval_to_string(&format!("xs:{ty}('{lexical}')"));
        assert!(
            matches!(constructor, Err(XPathError::FORG0001 { .. })),
            "xs:{ty}('{lexical}'): {constructor:?}"
        );
        assert_eq!(
            eval_to_string(&format!("'{lexical}' castable as xs:{ty}")).unwrap(),
            "false",
            "'{lexical}' castable as xs:{ty}"
        );
    }
    // The boundary values stay castable.
    assert_eq!(
        eval_to_string("'09:15:00+14:00' castable as xs:time").unwrap(),
        "true"
    );
    assert_eq!(
        eval_to_string("'2002-10-10-14:00' castable as xs:date").unwrap(),
        "true"
    );
}

/// XPath 2.0 §3.1.1: the value of a string literal is the characters between
/// the delimiters; no XML un-escaping happens inside the XPath processor.
#[test]
fn string_literals_keep_their_characters_verbatim() {
    assert_eq!(eval_to_string("concat('a&b','!')").unwrap(), "a&b!");
    assert_eq!(eval_to_string("concat('a&amp;b','!')").unwrap(), "a&amp;b!");
    assert_eq!(eval_to_string("string-length('&#13;')").unwrap(), "5");
    assert_eq!(eval_to_string("'a&foo;b'").unwrap(), "a&foo;b");
    // A literal carriage return stays a carriage return (codepoint 13).
    assert_eq!(
        eval_to_string(
            "string-join(for $c in string-to-codepoints('a\rb') return string($c), ' ')"
        )
        .unwrap(),
        "97 13 98"
    );
    // A doubled delimiter still stands for one.
    assert_eq!(eval_to_string("'it''s'").unwrap(), "it's");
}

/// Compile and evaluate `expr` with the document element of `xml` as context
/// item, returning the string value or the error code.
#[cfg(test)]
fn eval_on_doc(expr: &str, xml: &str) -> Result<String, Option<&'static str>> {
    use crate::namespace::context::NamespaceContextSnapshot;
    use crate::namespace::table::NameTable;
    use crate::xpath::api::XPathExpr;
    use crate::xpath::RoXmlNavigator;

    let names = NameTable::new();
    let mut namespaces = NamespaceContextSnapshot::default();
    namespaces.bindings.push((
        names.add("xs"),
        names.add("http://www.w3.org/2001/XMLSchema"),
    ));
    namespaces.bindings.push((
        names.add("xsi"),
        names.add("http://www.w3.org/2001/XMLSchema-instance"),
    ));
    let ctx = XPathContext::new(&names).with_namespaces(namespaces);
    let doc = roxmltree::Document::parse(xml).expect("well-formed test document");
    let nav = RoXmlNavigator::new(&doc);
    let compiled = XPathExpr::compile(expr, &ctx).map_err(|e| e.error_code())?;
    compiled
        .evaluator(&ctx)
        .run_with_node(nav)
        .map(|v| {
            v.into_vec()
                .iter()
                .map(|item| match item {
                    XmlItem::Atomic(a) => a.to_string_value(),
                    XmlItem::Node(_) => "<node>".to_string(),
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .map_err(|e| e.error_code())
}

/// `fn:min` and `fn:max` accept every type that has an ordering, which
/// includes the date and time types, not only the numeric ones.
#[test]
fn min_and_max_accept_the_ordered_date_and_time_types() {
    let dates = r#"(xs:date("1996-12-23"), xs:date("1995-12-23"))"#;
    assert_eq!(
        eval_to_string(&format!("max({dates})")).unwrap(),
        "1996-12-23"
    );
    assert_eq!(
        eval_to_string(&format!("min({dates})")).unwrap(),
        "1995-12-23"
    );
    assert_eq!(
        eval_to_string(&format!("max({dates}) instance of xs:date")).unwrap(),
        "true"
    );

    for (expr, expected) in [
        (
            r#"max((xs:dateTime("2001-01-01T12:00:00"), xs:dateTime("2002-01-01T12:00:00")))"#,
            "2002-01-01T12:00:00",
        ),
        (
            r#"min((xs:time("12:00:00"), xs:time("10:00:00")))"#,
            "10:00:00",
        ),
        (
            r#"max((xs:dayTimeDuration("PT1S"), xs:dayTimeDuration("PT2S")))"#,
            "PT2S",
        ),
        (
            r#"max((xs:yearMonthDuration("P1Y"), xs:yearMonthDuration("P2Y")))"#,
            "P2Y",
        ),
        (r#"max(("a", "c", "b"))"#, "c"),
        (r#"min((true(), false()))"#, "false"),
    ] {
        assert_eq!(eval_to_string(expr).unwrap(), expected, "{expr}");
    }

    // A type with no ordering is still rejected.
    assert!(eval_to_string(r#"max((xs:gYear("2000"), xs:gYear("2001")))"#).is_err());
    assert!(eval_to_string(r#"max((xs:duration("P1Y"), xs:duration("P2Y")))"#).is_err());

    // …and so is a sequence that mixes primitive types, because `lt` and `gt`
    // are not defined across them.
    for expr in [
        r#"min((xs:dateTime("1996-12-23T13:13:00"), 23, "brian"))"#,
        r#"max((xs:date("1996-12-23"), 23))"#,
        r#"max((1, "a"))"#,
        r#"max((true(), 1))"#,
        r#"max((xs:dayTimeDuration("PT1S"), xs:yearMonthDuration("P1Y")))"#,
    ] {
        assert!(eval_to_string(expr).is_err(), "{expr}");
    }
    // The numeric types still mix with one another, and xs:anyURI with strings.
    assert_eq!(eval_to_string("max((1, 2.5, 3.0e0))").unwrap(), "3");
    assert_eq!(
        eval_to_string(r#"max((xs:anyURI("http://b/"), "http://a/"))"#).unwrap(),
        "http://b/"
    );
}

/// `fn:sum`'s `$zero` may itself be the empty sequence, which is not the same
/// as omitting it.
#[test]
fn sum_of_an_empty_sequence_returns_the_supplied_zero() {
    assert_eq!(eval_to_string("count(sum((), ()))").unwrap(), "0");
    assert_eq!(eval_to_string("sum(())").unwrap(), "0");
    assert_eq!(eval_to_string("sum((), 42)").unwrap(), "42");
    assert_eq!(eval_to_string(r#"sum((), "none")"#).unwrap(), "none");
}

/// `fn:sum` accumulates with `op:numeric-add`, so a sum of integers is an
/// integer; `fn:avg` divides by the count, and integer ÷ integer is a decimal.
#[test]
fn aggregate_result_types_follow_the_operators() {
    assert_eq!(
        eval_to_string("sum((1,2,3)) instance of xs:integer").unwrap(),
        "true"
    );
    assert_eq!(eval_to_string("sum((1,2,3))").unwrap(), "6");
    assert_eq!(
        eval_to_string("sum((1,2.5)) instance of xs:decimal").unwrap(),
        "true"
    );
    assert_eq!(
        eval_to_string("avg((1,2,3)) instance of xs:decimal").unwrap(),
        "true"
    );
    assert_eq!(
        eval_to_string("min((1,2,3)) instance of xs:integer").unwrap(),
        "true"
    );
    assert_eq!(
        eval_to_string("max((1,2,3)) instance of xs:integer").unwrap(),
        "true"
    );
}

/// `fn:round-half-to-even` decides a tie on the exact value of the argument,
/// not on a scaled binary product, and a zero result keeps the argument's sign.
#[test]
fn round_half_to_even_uses_the_exact_value() {
    for (expr, expected) in [
        // The double nearest 250.025 is above the midpoint, although
        // multiplying it by 100 gives exactly 25002.5.
        ("round-half-to-even(250.0250e0, 2)", "250.03"),
        // The float nearest 150.015 is below the midpoint.
        ("round-half-to-even(xs:float(150.0150e0), 2)", "150.01"),
        // A zero result keeps the sign of the argument.
        ("round-half-to-even(-3.0e0, -2)", "-0"),
        ("round-half-to-even(-0.3e0)", "-0"),
        // Genuine ties go to the even candidate.
        ("round-half-to-even(0.5e0)", "0"),
        ("round-half-to-even(1.5e0)", "2"),
        ("round-half-to-even(2.5e0)", "2"),
        ("round-half-to-even(1.125e0, 2)", "1.12"),
        ("round-half-to-even(35612.25e0, -2)", "35600"),
        ("round-half-to-even(3.567812e+3, 2)", "3567.81"),
        ("round-half-to-even(0.5)", "0"),
        ("round-half-to-even(1.5)", "2"),
        ("round-half-to-even(2.5)", "2"),
        ("round-half-to-even(0)", "0"),
    ] {
        assert_eq!(eval_to_string(expr).unwrap(), expected, "{expr}");
    }
    // NaN and the infinities are returned unchanged.
    assert_eq!(
        eval_to_string("round-half-to-even(1.0e0 div 0.0e0)").unwrap(),
        "INF"
    );
    assert_eq!(
        eval_to_string("round-half-to-even(-1.0e0 div 0.0e0)").unwrap(),
        "-INF"
    );
}

/// `fn:subsequence` keeps the items whose position `p` satisfies
/// `round($startingLoc) <= p` and `p < round($startingLoc) + round($length)`,
/// in xs:double arithmetic — so an infinite or NaN bound falls out of the same
/// two comparisons.
#[test]
fn subsequence_follows_the_position_predicate() {
    for (expr, expected) in [
        ("string-join(subsequence((1 to 10), 4, 3), ',')", "4,5,6"),
        (
            "string-join(subsequence((1 to 10), 4), ',')",
            "4,5,6,7,8,9,10",
        ),
        ("string-join(subsequence((1 to 5), -1, 3), ',')", "1"),
        ("string-join(subsequence((1 to 5), 0), ',')", "1,2,3,4,5"),
        ("count(subsequence((1 to 5), 6))", "0"),
        ("count(subsequence((1 to 5), 2, 0))", "0"),
        ("count(subsequence((1 to 5), 2, -1))", "0"),
        // -INF to +INF sums to NaN, and every comparison with NaN is false.
        (
            "count(subsequence(1 to 20, -1.0e0 div 0.0e0, 1.0e0 div 0.0e0))",
            "0",
        ),
        ("count(subsequence(1 to 20, 1, 1.0e0 div 0.0e0))", "20"),
        ("count(subsequence(1 to 20, 1.0e0 div 0.0e0))", "0"),
        ("count(subsequence(1 to 20, -1.0e0 div 0.0e0))", "20"),
        ("count(subsequence(1 to 20, number('x')))", "0"),
        ("count(subsequence(1 to 20, 1, number('x')))", "0"),
    ] {
        assert_eq!(eval_to_string(expr).unwrap(), expected, "{expr}");
    }
}

/// `fn:resolve-QName` reports FOCA0002 for a value that is not a lexical
/// QName and FONS0004 for a prefix the element does not bind.
#[test]
fn resolve_qname_error_codes() {
    let doc = r#"<e xmlns:pre="http://example.com/"/>"#;
    assert_eq!(
        eval_on_doc(r#"resolve-QName("pre:+thing", /e)"#, doc),
        Err(Some("FOCA0002"))
    );
    assert_eq!(
        eval_on_doc(r#"resolve-QName(":thing", /e)"#, doc),
        Err(Some("FOCA0002"))
    );
    assert_eq!(
        eval_on_doc(r#"resolve-QName("", /e)"#, doc),
        Err(Some("FOCA0002"))
    );
    assert_eq!(
        eval_on_doc(r#"resolve-QName("post:thing", /e)"#, doc),
        Err(Some("FONS0004"))
    );
    // A bound prefix and an unprefixed name still resolve.
    assert_eq!(
        eval_on_doc(
            r#"namespace-uri-from-QName(resolve-QName("pre:thing", /e))"#,
            doc
        )
        .unwrap(),
        "http://example.com/"
    );
    assert_eq!(
        eval_on_doc(r#"local-name-from-QName(resolve-QName("thing", /e))"#, doc).unwrap(),
        "thing"
    );
}

/// "nilled" is the post-schema-validation property, so an element that was
/// never validated is not nilled however it is marked up.
#[test]
fn nilled_is_false_for_an_unvalidated_element() {
    let doc = r#"<doc xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><e xsi:nil="true"/><f/></doc>"#;
    assert_eq!(eval_on_doc("nilled(/doc/e)", doc).unwrap(), "false");
    assert_eq!(eval_on_doc("nilled(/doc/f)", doc).unwrap(), "false");
    // Not an element: the empty sequence.
    assert_eq!(
        eval_on_doc("count(nilled(/doc/e/@xsi:nil))", doc).unwrap(),
        "0"
    );
}

/// XDM §6.4: the typed value of a namespace node is its string value (the
/// namespace URI) as an `xs:string`; it is never `xs:untypedAtomic`, because a
/// namespace node carries no type annotation that could make it untyped.
/// Corroborated by XQTS `K2-NamespaceProp-*` / the `namespace::` accessor
/// cases, which compare the atomized namespace node with a string.
#[test]
fn atomizing_a_namespace_node_yields_xs_string() {
    let doc = r#"<doc xmlns:p="http://example.com/p"/>"#;
    assert_eq!(
        eval_on_doc("data(/doc/namespace::p) instance of xs:string", doc).unwrap(),
        "true"
    );
    assert_eq!(
        eval_on_doc("data(/doc/namespace::p) instance of xs:untypedAtomic", doc).unwrap(),
        "false"
    );
    assert_eq!(
        eval_on_doc("data(/doc/namespace::p)", doc).unwrap(),
        "http://example.com/p"
    );
    // The implicit `xml` binding atomizes the same way.
    assert_eq!(
        eval_on_doc("data(/doc/namespace::xml) instance of xs:string", doc).unwrap(),
        "true"
    );
}

/// The same rule for the two other kinds that have no type annotation: a
/// comment and a processing instruction atomize as `xs:string` (XDM §6.6,
/// §6.7). These two were already right; the test pins them.
#[test]
fn atomizing_a_comment_or_processing_instruction_yields_xs_string() {
    let doc = r#"<doc><!--c--><?pi t?></doc>"#;
    assert_eq!(
        eval_on_doc("data(/doc/comment()) instance of xs:string", doc).unwrap(),
        "true"
    );
    assert_eq!(
        eval_on_doc(
            "data(/doc/processing-instruction()) instance of xs:string",
            doc
        )
        .unwrap(),
        "true"
    );
}

/// `fn:namespace-uri-for-prefix($prefix, $element)`: an empty or absent
/// `$prefix` asks for the element's *default* namespace, and the result is the
/// empty sequence when the prefix (including the empty one) is not bound —
/// never `xs:anyURI("")`.
///
/// From memory of F&O §14 (not checked out locally), corroborated by XQTS:
/// * `fn-namespace-uri-for-prefix-3` — `fn:string(fn:namespace-uri-for-prefix("", $var))`
///   with `declare default element namespace "http://www.example.com/defaultspace"`
///   expects `http://www.example.com/defaultspace`;
/// * `fn-namespace-uri-for-prefix-4` — the same with `()` as the prefix, same result;
/// * `fn-namespace-uri-for-prefix-2`, `-6`, `-16`, `-17` — `fn:count(...)` of an
///   unbound prefix expects `0`, i.e. the empty sequence.
#[test]
fn namespace_uri_for_prefix_returns_the_default_namespace_or_the_empty_sequence() {
    let with_default = r#"<doc xmlns="http://example.com/d" xmlns:p="http://example.com/p"/>"#;
    // The empty prefix and the absent prefix both ask for the default namespace.
    assert_eq!(
        eval_on_doc("namespace-uri-for-prefix('', /*)", with_default).unwrap(),
        "http://example.com/d"
    );
    assert_eq!(
        eval_on_doc("namespace-uri-for-prefix((), /*)", with_default).unwrap(),
        "http://example.com/d"
    );
    assert_eq!(
        eval_on_doc("namespace-uri-for-prefix('p', /*)", with_default).unwrap(),
        "http://example.com/p"
    );

    // No default namespace in scope: the empty sequence, not xs:anyURI("").
    let no_default = r#"<doc xmlns:p="http://example.com/p"/>"#;
    assert_eq!(
        eval_on_doc("count(namespace-uri-for-prefix('', /*))", no_default).unwrap(),
        "0"
    );
    assert_eq!(
        eval_on_doc("count(namespace-uri-for-prefix((), /*))", no_default).unwrap(),
        "0"
    );
    // An unbound non-empty prefix was already the empty sequence.
    assert_eq!(
        eval_on_doc("count(namespace-uri-for-prefix('q', /*))", no_default).unwrap(),
        "0"
    );

    // `xml` is bound in every element's in-scope namespaces.
    assert_eq!(
        eval_on_doc("namespace-uri-for-prefix('xml', /*)", no_default).unwrap(),
        "http://www.w3.org/XML/1998/namespace"
    );

    // The result is an xs:anyURI when there is one.
    assert_eq!(
        eval_on_doc(
            "namespace-uri-for-prefix('p', /*) instance of xs:anyURI",
            no_default
        )
        .unwrap(),
        "true"
    );
}

/// XPath 2.0, Appendix G: "err:XPST0081 It is a static error if a QName used in
/// an expression contains a namespace prefix that cannot be expanded into a
/// namespace URI by using the statically known namespaces." A QName inside a
/// kind test is no exception — `element(p:x)`, `attribute(p:a)`,
/// `element(*, p:T)` and `schema-element(p:x)` are all QNames used in an
/// expression.
mod kind_test_prefix_tests {
    use super::spec_helpers::{error_code, items, try_items};

    #[test]
    fn an_unbound_prefix_in_a_kind_test_is_a_static_error() {
        let doc = "<a/>";
        // Step node tests.
        assert_eq!(error_code("//element(p:x)", doc), "XPST0081");
        assert_eq!(error_code("//attribute(p:a)", doc), "XPST0081");
        assert_eq!(error_code("//element(*, p:T)", doc), "XPST0081");
        assert_eq!(error_code("//element(x, p:T)", doc), "XPST0081");
        assert_eq!(error_code("//attribute(*, p:T)", doc), "XPST0081");
        assert_eq!(error_code("//schema-element(p:x)", doc), "XPST0081");
        assert_eq!(error_code("//schema-attribute(p:a)", doc), "XPST0081");
        // Nested inside document-node().
        assert_eq!(error_code("//document-node(element(p:x))", doc), "XPST0081");
        // The same tests inside a SequenceType.
        assert_eq!(error_code(". instance of element(p:x)", doc), "XPST0081");
        assert_eq!(error_code(". treat as attribute(p:a)", doc), "XPST0081");
        assert_eq!(
            error_code(". instance of schema-element(p:x)", doc),
            "XPST0081"
        );
        assert_eq!(
            error_code(". instance of document-node(element(p:x))", doc),
            "XPST0081"
        );
    }

    #[test]
    fn a_bound_or_absent_prefix_in_a_kind_test_still_compiles() {
        // Guards: nothing that used to compile may stop compiling.
        let doc = r#"<a n="1"><b/></a>"#;
        assert_eq!(items("//element(b)", doc), ["element(b)"]);
        assert_eq!(items("//element()", doc), ["element(a)", "element(b)"]);
        assert_eq!(items("//element(*)", doc), ["element(a)", "element(b)"]);
        assert_eq!(items("//attribute(n)", doc), ["attribute(n=1)"]);
        // A bound prefix on the *type* name compiles (what it then matches is
        // a separate question, settled by SequenceType matching).
        assert!(try_items("//element(*, xs:untyped)", doc).is_ok());
        assert!(try_items("//attribute(*, xs:untypedAtomic)", doc).is_ok());
        // `xml` is in scope in every static context (XML Names).
        assert_eq!(items("//attribute(xml:lang)", doc).len(), 0);
        assert_eq!(items(". instance of element(b)", doc), ["false"]);
        assert_eq!(
            items(". instance of document-node(element(a))", doc),
            ["true"]
        );
    }
}

/// XDM 1.0 §3.3.1.2 and XPath 2.0 §2.4.2 / §2.5.2 — atomizing a node whose type
/// is a list type yields one atomic value per list member, each of the list's
/// item type; the string value of the node stays the whole text.
mod list_atomization_tests {
    use bumpalo::Bump;

    use crate::document::typed_builder::build_typed_document;
    use crate::document::BufferDocumentOptions;
    use crate::namespace::context::NamespaceContextSnapshot;
    use crate::pipeline::load_and_process_schema;
    use crate::schema::SchemaSet;
    use crate::xpath::api::XPathExpr;
    use crate::xpath::iterator::XmlItem;
    use crate::xpath::XPathContext;

    const SCHEMA: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
      <xs:simpleType name="idList"><xs:list itemType="xs:ID"/></xs:simpleType>
      <xs:simpleType name="idrefList"><xs:list itemType="xs:IDREF"/></xs:simpleType>
      <xs:simpleType name="intList"><xs:list itemType="xs:integer"/></xs:simpleType>
      <xs:simpleType name="dblList"><xs:list itemType="xs:double"/></xs:simpleType>
      <xs:simpleType name="intOrString">
        <xs:union memberTypes="xs:integer xs:string"/>
      </xs:simpleType>
      <xs:simpleType name="mixedList"><xs:list itemType="intOrString"/></xs:simpleType>
      <xs:simpleType name="intOrFloat">
        <xs:union memberTypes="xs:integer xs:float"/>
      </xs:simpleType>
      <xs:simpleType name="numList"><xs:list itemType="intOrFloat"/></xs:simpleType>
      <xs:simpleType name="smallInt">
        <xs:restriction base="xs:integer"><xs:maxInclusive value="100"/></xs:restriction>
      </xs:simpleType>
      <xs:simpleType name="smallList"><xs:list itemType="smallInt"/></xs:simpleType>
      <xs:simpleType name="smallOrString">
        <xs:union memberTypes="smallInt xs:string"/>
      </xs:simpleType>
      <xs:simpleType name="smallOrStringList"><xs:list itemType="smallOrString"/></xs:simpleType>
      <xs:complexType name="intListWithKind">
        <xs:simpleContent>
          <xs:extension base="intList">
            <xs:attribute name="kind" type="xs:string"/>
          </xs:extension>
        </xs:simpleContent>
      </xs:complexType>
      <xs:element name="doc">
        <xs:complexType>
          <xs:sequence>
            <xs:element name="l" type="xs:NMTOKENS"/>
            <xs:element name="sc" type="intListWithKind"/>
            <xs:element name="ni" type="xs:integer" nillable="true"/>
            <xs:element name="d" type="dblList" maxOccurs="unbounded"/>
            <xs:element name="p" maxOccurs="unbounded">
              <xs:complexType>
                <xs:attribute name="n" type="intList"/>
                <xs:attribute name="r" type="idrefList"/>
              </xs:complexType>
            </xs:element>
            <xs:element name="lu" type="numList"/>
            <xs:element name="any"/>
            <xs:element name="mx">
              <xs:complexType mixed="true">
                <xs:sequence><xs:element name="i" type="xs:string"/></xs:sequence>
              </xs:complexType>
            </xs:element>
            <xs:element name="emp">
              <xs:complexType><xs:attribute name="k" type="xs:string"/></xs:complexType>
            </xs:element>
            <xs:element name="bo" type="xs:boolean" maxOccurs="2"/>
          </xs:sequence>
          <xs:attribute name="l" type="idList"/>
          <xs:attribute name="one" type="idList"/>
          <xs:attribute name="n" type="intList"/>
          <xs:attribute name="n1" type="intList"/>
          <xs:attribute name="e" type="intList"/>
          <xs:attribute name="r" type="xs:IDREFS"/>
          <xs:attribute name="m" type="mixedList"/>
          <xs:attribute name="m2" type="mixedList"/>
          <xs:attribute name="b" type="xs:boolean"/>
          <xs:attribute name="m3" type="mixedList"/>
          <xs:attribute name="s" type="smallList"/>
          <xs:attribute name="su" type="smallOrStringList"/>
        </xs:complexType>
      </xs:element>
    </xs:schema>"#;

    const INSTANCE: &str = concat!(
        r#"<doc l="a b" one="c" n="1 2 3" n1=" 5 " e="" r="a b" m="1 x" m2="x 1" b="true""#,
        r#" m3="01 x" s="5 7" su="5 x 500""#,
        r#" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">"#,
        r#"<l>p q</l><sc kind="k">4 5</sc><ni xsi:nil="true"/>"#,
        r#"<d>1 NaN</d><d> 1  NaN </d><d>1 2</d>"#,
        r#"<p n="1 2 3" r="a b"/><p n=" 1  2   3 " r="c"/><p n="1 2 4"/><lu>1 1.0e0</lu>"#,
        r#"<any>text</any><mx>a<i>b</i>c</mx><emp k="v"/><bo>true</bo><bo>false</bo>"#,
        r#"</doc>"#
    );

    /// Evaluate every `(expression, expected)` pair over the validated
    /// instance and report all mismatches at once. A result is its items'
    /// string values joined by `|`, `()` for the empty sequence, `<node>` for a
    /// node, and `error:CODE` for an error.
    fn check(cases: &[(&str, &str)], compat: bool) {
        let mut schema_set = SchemaSet::xsd11();
        load_and_process_schema(SCHEMA.as_bytes(), "test.xsd", &mut schema_set, None)
            .expect("the fixture schema loads");
        let arena = Bump::new();
        let doc = build_typed_document(
            INSTANCE.as_bytes(),
            &arena,
            &schema_set,
            BufferDocumentOptions::default(),
        )
        .expect("the fixture document is built");
        let mut namespaces = NamespaceContextSnapshot::default();
        namespaces.bindings.push((
            schema_set.name_table.add("xs"),
            schema_set
                .name_table
                .add("http://www.w3.org/2001/XMLSchema"),
        ));
        let ctx = XPathContext::new(&schema_set.name_table)
            .with_namespaces(namespaces)
            .with_schema_set(&schema_set)
            .with_xpath10_compatibility(compat);

        let code = |e: crate::xpath::XPathError| {
            format!(
                "error:{}",
                e.error_code()
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("<no code> {e}"))
            )
        };
        let mut wrong = Vec::new();
        for (expr, expected) in cases {
            let got = match XPathExpr::compile(expr, &ctx) {
                Err(e) => code(e),
                Ok(compiled) => match compiled
                    .evaluator(&ctx)
                    .run_with_node(doc.create_navigator())
                {
                    Err(e) => code(e),
                    Ok(value) if value.is_empty() => "()".to_string(),
                    Ok(value) => value
                        .into_vec()
                        .iter()
                        .map(|item| match item {
                            XmlItem::Atomic(v) => v.to_string_value(),
                            XmlItem::Node(_) => "<node>".to_string(),
                        })
                        .collect::<Vec<_>>()
                        .join("|"),
                },
            };
            if got != *expected {
                wrong.push(format!("{expr}  =>  {got}   (expected {expected})"));
            }
        }
        assert!(
            wrong.is_empty(),
            "{} of {} cases wrong:\n{}",
            wrong.len(),
            cases.len(),
            wrong.join("\n")
        );
    }

    /// `fn:data` and the typed value: one item per member, each of the type
    /// that validated it (here the list's item type); `fn:string` is untouched.
    #[test]
    fn data_yields_one_item_per_member() {
        check(
            &[
                ("count(data(/doc/@l))", "2"),
                ("data(/doc/@l)", "a|b"),
                ("data(/doc/@l)[1] instance of xs:ID", "true"),
                ("data(/doc/@l)[2] instance of xs:ID", "true"),
                ("data(/doc/@l) instance of xs:ID", "false"),
                ("data(/doc/@l) instance of xs:ID+", "true"),
                ("count(data(/doc/@one))", "1"),
                ("data(/doc/@one) instance of xs:ID", "true"),
                ("data(/doc/@n)[1] instance of xs:integer", "true"),
                ("data(/doc/@n)", "1|2|3"),
                ("string(/doc/@l)", "a b"),
                ("string(/doc/l)", "p q"),
                ("string(/doc/sc)", "4 5"),
                // The empty list: the typed value is the empty sequence.
                ("data(/doc/@e)", "()"),
                ("count(data(/doc/@e))", "0"),
                // A simple-typed list element and a complex type with simple
                // content of a list type.
                ("count(data(/doc/l))", "2"),
                ("data(/doc/l)[2] instance of xs:NMTOKEN", "true"),
                ("count(data(/doc/sc))", "2"),
                ("data(/doc/sc)[2] instance of xs:integer", "true"),
                // The built-in list types.
                ("count(data(/doc/@r))", "2"),
                ("data(/doc/@r)[1] instance of xs:IDREF", "true"),
                ("data(/doc/@r) instance of xs:IDREFS", "error:XPST0051"),
                // A nilled element has the empty sequence as its typed value.
                ("count(data(/doc/ni))", "0"),
            ],
            false,
        );
    }

    /// XPath 2.0 §3.5.2: a general comparison is existential over the atomized
    /// operands, so over the members.
    #[test]
    fn general_comparisons_see_every_member() {
        check(
            &[
                ("/doc/@l = 'b'", "true"),
                ("/doc/@l = 'a b'", "false"),
                ("/doc/@l != 'a'", "true"),
                ("'b' = /doc/@l", "true"),
                ("/doc/@n = 2", "true"),
                ("/doc/@n > 2", "true"),
                ("/doc/@n < 1", "false"),
                ("/doc/@n >= 3", "true"),
                ("/doc/l = 'q'", "true"),
                ("/doc/@r = 'b'", "true"),
                ("/doc/sc = 5", "true"),
                ("/doc/@e = 1", "false"),
                ("/doc/@l = /doc/@r", "true"),
                ("/doc/@n = (7, 8, 3)", "true"),
                ("(7, 8, 3) = /doc/@n", "true"),
            ],
            false,
        );
    }

    /// XPath 2.0 §3.5.1 / §3.4: more than one member is a type error, none is
    /// the empty sequence, and exactly one is that member.
    #[test]
    fn value_comparisons_and_arithmetic_take_at_most_one_member() {
        check(
            &[
                ("/doc/@l eq 'b'", "error:XPTY0004"),
                ("/doc/@one eq 'c'", "true"),
                ("/doc/@n + 1", "error:XPTY0004"),
                ("/doc/@n1 + 1", "6"),
                ("/doc/@n1 eq 5", "true"),
                ("-/doc/@n1", "-5"),
                ("/doc/@e eq 1", "()"),
                ("/doc/@e + 1", "()"),
                ("xs:string(/doc/@l)", "error:XPTY0004"),
                ("/doc/@l castable as xs:string", "false"),
                ("/doc/@one cast as xs:string", "c"),
                ("/doc/@n1 to 6", "5|6"),
            ],
            false,
        );
    }

    /// XPath 2.0 §3.5.1 / §3.4 on a nilled element, whose typed value is the
    /// empty sequence.
    #[test]
    fn a_nilled_element_is_the_empty_sequence_in_value_comparisons_and_arithmetic() {
        check(
            &[
                ("/doc/ni eq 1", "()"),
                ("/doc/ni + 1", "()"),
                ("-/doc/ni", "()"),
                ("/doc/ni castable as xs:integer?", "true"),
                ("/doc/ni castable as xs:integer", "false"),
                ("/doc/ni = 1", "false"),
            ],
            false,
        );
    }

    /// Functions that atomize their arguments see the members.
    #[test]
    fn functions_see_every_member() {
        check(
            &[
                ("sum(data(/doc/@n))", "6"),
                ("sum(/doc/@n)", "6"),
                ("avg(/doc/@n)", "2"),
                ("min(/doc/@n)", "1"),
                ("max(/doc/@n)", "3"),
                ("sum(/doc/sc)", "9"),
                ("distinct-values(/doc/@l)", "a|b"),
                ("index-of(/doc/@l, xs:ID('b'))", "2"),
                ("index-of(/doc/@l, 'b')", "2"),
                ("index-of(/doc/@n, 3)", "3"),
                ("string-join(data(/doc/@l), ',')", "a,b"),
                ("some $x in data(/doc/@n) satisfies $x eq 2", "true"),
                ("deep-equal(data(/doc/@l), ('a', 'b'))", "true"),
                ("abs(/doc/@n1)", "5"),
                ("abs(/doc/@e)", "()"),
                ("exactly-one(data(/doc/@one))", "c"),
                // The effective boolean value of two atomic values is an error
                // (XPath 2.0 §2.4.3); of one string, its non-emptiness.
                ("boolean(data(/doc/@l))", "error:FORG0006"),
                ("boolean(data(/doc/@one))", "true"),
            ],
            false,
        );
    }

    /// The function conversion rules (XPath 2.0 §3.1.5) atomize a node
    /// argument; they do not read its string value.
    #[test]
    fn function_conversion_atomizes_a_typed_node() {
        check(
            &[
                ("number(/doc/@b)", "1"),
                ("number(/doc/@n1)", "5"),
                ("number(/doc/@n)", "error:XPTY0004"),
                ("number(/doc/@e)", "NaN"),
                ("concat(/doc/@n1, '')", "5"),
                ("string-join(/doc/@l, ',')", "a,b"),
                ("upper-case(/doc/@l)", "error:XPTY0004"),
                ("upper-case(/doc/@one)", "C"),
                ("string-length(/doc/@e)", "0"),
                ("compare(/doc/@e, 'a')", "()"),
                ("compare(/doc/ni, 'a')", "()"),
                ("contains(/doc/@one, 'c')", "true"),
                ("substring(/doc/@one, 1)", "c"),
                // An element with element-only content has no typed value
                // (XPath 2.0 §2.4.2, XDM §3.3.1.3).
                ("string-length(/doc)", "error:FOTY0012"),
                // …while fn:string and the zero-argument forms keep reading the
                // string value.
                ("string-length(string(/doc/@l))", "3"),
                ("/doc/@l/string-length()", "3"),
                ("/doc/@l/normalize-space()", "a b"),
            ],
            false,
        );
    }

    /// XPath 2.0 §3.5.2 / §3.4 in XPath 1.0 compatibility mode: the
    /// conversions apply to each member.
    #[test]
    fn compatibility_mode_compares_member_by_member() {
        check(
            &[
                ("/doc/@l = 'b'", "true"),
                ("/doc/@l = 'a b'", "false"),
                ("/doc/@n = 2", "true"),
                ("/doc/@n = '2'", "true"),
                ("/doc/@n > 2", "true"),
                ("/doc/@n < 1", "false"),
                ("/doc/@n + 1", "2"),
                ("/doc/@e + 1", "NaN"),
                ("string(/doc/@l)", "a b"),
            ],
            true,
        );
    }

    /// Review finding: `fn:number` in XPath 1.0 compatibility mode. Its
    /// parameter is `xs:anyAtomicType?`, so of the §3.1.5 compatibility steps
    /// only `V[1]` applies, and the normal rules follow: the node is atomized
    /// to its typed value, which `fn:number` converts. Reading the string value
    /// made a typed `true` NaN.
    #[test]
    fn compatibility_mode_number_atomizes_a_typed_node() {
        check(
            &[
                ("number(/doc/bo[1])", "1"),
                ("number(/doc/bo[2])", "0"),
                ("/doc/bo[1]/number()", "1"),
                ("number(/doc/@b)", "1"),
                ("/doc/@b/number()", "1"),
                ("number(/doc/@n1)", "5"),
                // `V[1]` is the first node, then its typed value.
                ("number(/doc/bo)", "1"),
                ("number((/doc/bo[2], /doc/bo[1]))", "0"),
                // The typed value is empty: NaN.
                ("number(/doc/@e)", "NaN"),
                ("number(/doc/ni)", "NaN"),
                // `V[1]` is the one attribute; its typed value has two members
                // (`@l`) or three (`@n`), which `xs:anyAtomicType?` does not
                // match.
                ("number(/doc/@l)", "error:XPTY0004"),
                ("number(/doc/@n)", "error:XPTY0004"),
                // Element-only content has no typed value.
                ("number(/doc)", "error:FOTY0012"),
            ],
            true,
        );
    }

    /// F&O §15.3.1: typed values are compared item by item, `eq` with NaN equal
    /// to NaN, whatever list type carries them.
    #[test]
    fn deep_equal_compares_list_typed_values_member_by_member() {
        check(
            &[
                // Equal members, different whitespace.
                ("deep-equal(/doc/p[1]/@n, /doc/p[2]/@n)", "true"),
                // Different members.
                ("deep-equal(/doc/p[1]/@n, /doc/p[3]/@n)", "false"),
                // Simple-content elements whose members include NaN.
                ("deep-equal(/doc/d[1], /doc/d[2])", "true"),
                ("deep-equal(/doc/d[1], /doc/d[3])", "false"),
                // xs:IDREFS against a user-defined list of xs:IDREF.
                ("deep-equal(/doc/@r, /doc/p[1]/@r)", "true"),
                ("deep-equal(/doc/@r, /doc/p[2]/@r)", "false"),
            ],
            false,
        );
    }

    /// XDM 1.0 §6.2.4: an element of a complex type with mixed content —
    /// `xs:anyType` included — has its string value as an `xs:untypedAtomic`
    /// typed value, one with empty content has the empty sequence, and only
    /// element-only content has no typed value.
    #[test]
    fn complex_content_typed_values_follow_xdm() {
        check(
            &[
                ("data(/doc/any)", "text"),
                ("data(/doc/any) instance of xs:untypedAtomic", "true"),
                ("/doc/any = 'text'", "true"),
                ("string-length(/doc/any)", "4"),
                ("data(/doc/mx)", "abc"),
                ("/doc/mx = 'abc'", "true"),
                ("data(/doc/emp)", "()"),
                ("count(data(/doc/emp))", "0"),
                ("string-length(/doc/emp)", "0"),
                ("/doc/emp eq 'x'", "()"),
                ("data(/doc)", "error:FOTY0012"),
                ("/doc = 'x'", "error:FOTY0012"),
            ],
            false,
        );
    }

    /// XQTS `Constr-cont-constrmod-6` (XQuery-only, so not in the XPath
    /// selection): `fn:count(fn:data(…/@attr))` of an `xs:IDREFS` attribute
    /// holding `id1 id2` is `2`.
    #[test]
    fn xqts_constr_cont_constrmod_6_essence() {
        check(&[("count(data(/doc/@r))", "2")], false);
    }

    /// A list whose item type is a union: one item per member, each an
    /// instance of the member type that validated it.
    ///
    /// XPath 2.0 §2.5.2: the typed value of a list-typed node is "a sequence
    /// of three values (7, 8, 9), each of type hatsize" (the item type, also a
    /// user-defined one), and "when the type annotation of a node is a union
    /// type, the typed value of the node will be an instance of one of the
    /// member types of the union" — for a list of a union, each member is an
    /// instance of the member type that validated it.
    #[test]
    fn a_list_of_a_union_yields_one_item_per_member() {
        check(
            &[
                ("count(data(/doc/@m))", "2"),
                ("data(/doc/@m)", "1|x"),
                ("/doc/@m = 'x'", "true"),
                ("data(/doc/@m)[1] instance of xs:integer", "true"),
                ("data(/doc/@m)[1] eq 1", "true"),
                ("data(/doc/@m)[2] instance of xs:string", "true"),
                ("deep-equal(data(/doc/@m), (1, 'x'))", "true"),
                ("data(/doc/@m2)", "x|1"),
                ("data(/doc/@m2)[1] instance of xs:string", "true"),
                ("data(/doc/@m2)[1] instance of xs:integer", "false"),
                ("/doc/@m2 = 'x'", "true"),
                ("data(/doc/@m2)[2] instance of xs:integer", "true"),
                ("data(/doc/@m2)[2] + 1", "2"),
                // The typed value, not the spelling: `01` is the integer 1.
                ("data(/doc/@m3)", "1|x"),
                // XQTS `validateexpr-24` (XQuery-only): a list of a union of
                // xs:integer and xs:float, `1 1.0e0`, is an xs:integer and an
                // xs:float.
                ("count(data(/doc/lu))", "2"),
                ("data(/doc/lu)", "1|1"),
                (
                    "(data(/doc/lu)[1] instance of xs:integer, data(/doc/lu)[2] instance of xs:float)",
                    "true|true",
                ),
                ("data(/doc/lu)[2] instance of xs:integer", "false"),
            ],
            false,
        );
    }

    /// A member keeps its type — the item type (XPath 2.0 §2.5.2's
    /// `hatsizelist`: "each of type hatsize"), or the member type a union
    /// chose — with its `schema_type`, a user-defined type included.
    #[test]
    fn list_members_keep_their_user_defined_types() {
        check(
            &[
                ("data(/doc/@s)[2] instance of xs:integer", "true"),
                ("data(/doc/@su)", "5|x|500"),
                ("data(/doc/@su)[1] instance of xs:integer", "true"),
                ("data(/doc/@su)[1] + 1", "6"),
                ("data(/doc/@su)[2] instance of xs:string", "true"),
                // 500 is above smallInt's maxInclusive: xs:string took it.
                ("data(/doc/@su)[3] instance of xs:string", "true"),
                ("data(/doc/@su)[3] instance of xs:integer", "false"),
            ],
            false,
        );

        // The schema types themselves (`instance of` a user-defined atomic
        // type is not decided by the evaluator, so read them directly).
        let mut schema_set = SchemaSet::xsd11();
        load_and_process_schema(SCHEMA.as_bytes(), "test.xsd", &mut schema_set, None)
            .expect("the fixture schema loads");
        let arena = Bump::new();
        let doc = build_typed_document(
            INSTANCE.as_bytes(),
            &arena,
            &schema_set,
            BufferDocumentOptions::default(),
        )
        .expect("the fixture document is built");
        let ctx = XPathContext::new(&schema_set.name_table).with_schema_set(&schema_set);
        let named = |name: &str| {
            let id = schema_set.name_table.get(name).expect("interned");
            schema_set
                .lookup_type(None, id)
                .and_then(|tk| tk.as_simple())
        };
        let string = schema_set.get_built_in_simple_type_by_code(crate::types::XmlTypeCode::String);
        let schema_types = |expr: &str| {
            XPathExpr::compile(expr, &ctx)
                .expect("compiles")
                .evaluator(&ctx)
                .run_with_node(doc.create_navigator())
                .expect("evaluates")
                .into_vec()
                .into_iter()
                .map(|item| match item {
                    XmlItem::Atomic(v) => v.schema_type,
                    XmlItem::Node(_) => panic!("a node"),
                })
                .collect::<Vec<_>>()
        };
        assert!(named("smallInt").is_some() && string.is_some());
        assert_eq!(
            schema_types("data(/doc/@s)"),
            [named("smallInt"), named("smallInt")]
        );
        assert_eq!(
            schema_types("data(/doc/@su)"),
            [named("smallInt"), string, string]
        );
    }

    /// Review finding X-07, on typed values: `fn:index-of` and
    /// `fn:distinct-values` compare under the rules of `eq` (F&O §15.1.5,
    /// §15.1.6), so the members of an `xs:NMTOKENS` or `xs:ID`-list value —
    /// `xs:NMTOKEN`s, `xs:ID`s — equal the `xs:string`s they spell. F&O's own
    /// example: "If @a is an attribute of type xs:NMTOKENS whose string value is
    /// "red green blue" … then fn:index-of(@a, "blue") returns 3".
    #[test]
    fn index_of_and_distinct_values_see_list_members_as_strings() {
        check(
            &[
                ("index-of(/doc/l, 'q')", "2"),
                ("index-of(/doc/l, 'p')", "1"),
                ("index-of(/doc/@l, 'a')", "1"),
                ("index-of(data(/doc/@l), 'b')", "2"),
                ("index-of(/doc/@n, 2)", "2"),
                ("distinct-values((data(/doc/l), 'p'))", "p|q"),
                ("count(distinct-values((data(/doc/@l), 'a', 'b')))", "2"),
                ("distinct-values((data(/doc/@l), data(/doc/l)))", "a|b|p|q"),
            ],
            false,
        );
    }
}

/// `xs:QName` and `xs:NOTATION` values as strings.
///
/// F&O §17.1.2 (casting to xs:string and xs:untypedAtomic): "If ST is
/// xs:QName or xs:NOTATION: if the qualified name has a prefix, then TV is the
/// concatenation of the prefix of SV, a single colon (:), and the local name of
/// SV. otherwise TV is the local-name." §2.3: `fn:string` of an atomic value
/// "returns the same string as is returned by the expression " $arg cast as
/// xs:string "". §11.2.1 `op:QName-equal` ignores the prefixes.
mod qname_string_tests {
    use bumpalo::Bump;

    use crate::document::typed_builder::build_typed_document;
    use crate::document::BufferDocumentOptions;
    use crate::namespace::context::NamespaceContextSnapshot;
    use crate::namespace::table::NameTable;
    use crate::pipeline::load_and_process_schema;
    use crate::schema::SchemaSet;
    use crate::xpath::api::XPathExpr;
    use crate::xpath::iterator::XmlItem;
    use crate::xpath::{DomNavigator, RoXmlNavigator, XPathContext};

    fn render<N: DomNavigator>(items: Vec<XmlItem<N>>) -> String {
        if items.is_empty() {
            return "()".to_string();
        }
        items
            .iter()
            .map(|item| match item {
                XmlItem::Atomic(v) => v.to_string_value(),
                XmlItem::Node(_) => "<node>".to_string(),
            })
            .collect::<Vec<_>>()
            .join("|")
    }

    fn code(e: crate::xpath::XPathError) -> String {
        format!(
            "error:{}",
            e.error_code()
                .map(str::to_string)
                .unwrap_or_else(|| format!("<no code> {e}"))
        )
    }

    fn report(wrong: Vec<String>, total: usize) {
        assert!(
            wrong.is_empty(),
            "{} of {} cases wrong:\n{}",
            wrong.len(),
            total,
            wrong.join("\n")
        );
    }

    /// Evaluate each case over an untyped document with `xs` bound.
    fn check_untyped(xml: &str, cases: &[(&str, &str)]) {
        let names = NameTable::new();
        let mut namespaces = NamespaceContextSnapshot::default();
        namespaces.bindings.push((
            names.add("xs"),
            names.add("http://www.w3.org/2001/XMLSchema"),
        ));
        let ctx = XPathContext::new(&names).with_namespaces(namespaces);
        let doc = roxmltree::Document::parse(xml).expect("parse xml");
        let mut wrong = Vec::new();
        for (expr, expected) in cases {
            let got = match XPathExpr::compile(expr, &ctx) {
                Err(e) => code(e),
                Ok(compiled) => compiled
                    .evaluator(&ctx)
                    .run_with_node(RoXmlNavigator::new(&doc))
                    .map(|v| render(v.into_vec()))
                    .unwrap_or_else(code),
            };
            if got != *expected {
                wrong.push(format!("{expr}  =>  {got}   (expected {expected})"));
            }
        }
        report(wrong, cases.len());
    }

    const SCHEMA: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
      <xs:notation name="png" public="image/png"/>
      <xs:simpleType name="fmt">
        <xs:restriction base="xs:NOTATION"><xs:enumeration value="png"/></xs:restriction>
      </xs:simpleType>
      <xs:simpleType name="qnames"><xs:list itemType="xs:QName"/></xs:simpleType>
      <xs:element name="doc">
        <xs:complexType>
          <xs:sequence><xs:element name="q" type="xs:QName"/></xs:sequence>
          <xs:attribute name="q" type="xs:QName"/>
          <xs:attribute name="u" type="xs:QName"/>
          <xs:attribute name="f" type="fmt"/>
          <xs:attribute name="ql" type="qnames"/>
        </xs:complexType>
      </xs:element>
    </xs:schema>"#;

    const INSTANCE: &str =
        r#"<doc xmlns:p="urn:p" q="p:local" u="plain" f="png" ql="p:a p:b"><q>p:elem</q></doc>"#;

    /// Evaluate each case over the validated instance (typed values from the
    /// schema), with `xs` bound.
    fn check_typed(cases: &[(&str, &str)]) {
        let mut schema_set = SchemaSet::xsd11();
        load_and_process_schema(SCHEMA.as_bytes(), "test.xsd", &mut schema_set, None)
            .expect("the fixture schema loads");
        let arena = Bump::new();
        let doc = build_typed_document(
            INSTANCE.as_bytes(),
            &arena,
            &schema_set,
            BufferDocumentOptions::default(),
        )
        .expect("the fixture document is built");
        let mut namespaces = NamespaceContextSnapshot::default();
        namespaces.bindings.push((
            schema_set.name_table.add("xs"),
            schema_set
                .name_table
                .add("http://www.w3.org/2001/XMLSchema"),
        ));
        let ctx = XPathContext::new(&schema_set.name_table)
            .with_namespaces(namespaces)
            .with_schema_set(&schema_set);
        let mut wrong = Vec::new();
        for (expr, expected) in cases {
            let got = match XPathExpr::compile(expr, &ctx) {
                Err(e) => code(e),
                Ok(compiled) => compiled
                    .evaluator(&ctx)
                    .run_with_node(doc.create_navigator())
                    .map(|v| render(v.into_vec()))
                    .unwrap_or_else(code),
            };
            if got != *expected {
                wrong.push(format!("{expr}  =>  {got}   (expected {expected})"));
            }
        }
        report(wrong, cases.len());
    }

    /// Every string conversion of a QName gives its lexical form.
    #[test]
    fn a_qname_converts_to_its_lexical_form() {
        check_untyped(
            r#"<p:e xmlns:p="urn:p" a="1"/>"#,
            &[
                ("QName('urn:x', 'p:l')", "p:l"),
                ("string(QName('urn:x', 'p:l'))", "p:l"),
                ("string(QName('', 'l'))", "l"),
                ("xs:string(QName('urn:x', 'p:l'))", "p:l"),
                ("QName('urn:x', 'p:l') cast as xs:string", "p:l"),
                ("QName('urn:x', 'p:l') cast as xs:untypedAtomic", "p:l"),
                ("xs:untypedAtomic(QName('', 'l'))", "l"),
                ("QName('urn:x', 'p:l') castable as xs:string", "true"),
                ("concat('[', QName('urn:x', 'p:l'), ']')", "[p:l]"),
                (
                    "string-join(for $q in (QName('urn:x', 'p:a'), QName('', 'b')) return string($q), ',')",
                    "p:a,b",
                ),
                ("xs:QName('xs:integer')", "xs:integer"),
                ("string(xs:QName('xs:integer'))", "xs:integer"),
                ("node-name(/*)", "p:e"),
                ("string(node-name(/*))", "p:e"),
                ("string(node-name(/*/@a))", "a"),
                ("resolve-QName('p:x', /*)", "p:x"),
                ("string(resolve-QName('y', /*))", "y"),
            ],
        );
    }

    /// QName equality stays prefix-blind (F&O §11.2.1), also where values are
    /// hashed.
    #[test]
    fn qname_equality_ignores_the_prefix() {
        check_untyped(
            "<e/>",
            &[
                ("QName('urn:x', 'p:l') eq QName('urn:x', 'q:l')", "true"),
                ("QName('urn:x', 'p:l') eq QName('urn:y', 'p:l')", "false"),
                ("QName('urn:x', 'p:l') = QName('urn:x', 'q:l')", "true"),
                (
                    "count(distinct-values((QName('urn:x', 'p:l'), QName('urn:x', 'q:l'))))",
                    "1",
                ),
                (
                    "index-of((QName('urn:y', 'l'), QName('urn:x', 'q:l')), QName('urn:x', 'p:l'))",
                    "2",
                ),
            ],
        );
    }

    /// The typed value of a QName- or NOTATION-typed node converts to its
    /// lexical form too, members of a list of xs:QName included.
    #[test]
    fn typed_qname_and_notation_values_convert_to_their_lexical_form() {
        check_typed(&[
            ("data(/doc/@q)", "p:local"),
            ("string(data(/doc/@q))", "p:local"),
            ("data(/doc/@q) cast as xs:string", "p:local"),
            ("string(data(/doc/@u))", "plain"),
            ("data(/doc/q)", "p:elem"),
            ("xs:string(data(/doc/q))", "p:elem"),
            ("data(/doc/@f)", "png"),
            ("xs:string(data(/doc/@f))", "png"),
            ("data(/doc/@f) cast as xs:untypedAtomic", "png"),
            ("data(/doc/@q) eq QName('urn:p', 'other:local')", "true"),
            ("local-name-from-QName(data(/doc/@q))", "local"),
            ("prefix-from-QName(data(/doc/@q))", "p"),
            ("namespace-uri-from-QName(data(/doc/@q))", "urn:p"),
            ("data(/doc/@ql)", "p:a|p:b"),
            ("data(/doc/@ql)[2] eq QName('urn:p', 'b')", "true"),
            ("namespace-uri-from-QName(data(/doc/@ql)[1])", "urn:p"),
        ]);
    }
}

/// `xs:NOTATION` values in the indexed general comparison and the compare
/// cache, past the 64-pair prefix the pairwise loop walks first. XPath 2.0
/// §3.5.1: "A eq B xs:NOTATION xs:NOTATION op:NOTATION-equal(A, B)", which
/// ignores the prefixes as `op:QName-equal` does; §3.5.2: `=` is true when some
/// pair compares true, `!=` when some pair compares false.
mod notation_index_tests {
    use bumpalo::Bump;

    use crate::document::typed_builder::build_typed_document;
    use crate::document::BufferDocumentOptions;
    use crate::namespace::context::NamespaceContextSnapshot;
    use crate::pipeline::load_and_process_schema;
    use crate::schema::SchemaSet;
    use crate::xpath::api::XPathExpr;
    use crate::xpath::iterator::XmlItem;
    use crate::xpath::XPathContext;

    /// Two prefixes, `n` and `m`, bound to the target namespace; the
    /// enumeration lists both spellings.
    const SCHEMA: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
        xmlns:n="urn:n" xmlns:m="urn:n" targetNamespace="urn:n">
      <xs:notation name="gif" public="gif"/>
      <xs:notation name="png" public="png"/>
      <xs:notation name="jpg" public="jpg"/>
      <xs:simpleType name="nt"><xs:restriction base="xs:NOTATION">
        <xs:enumeration value="n:gif"/><xs:enumeration value="n:png"/>
        <xs:enumeration value="n:jpg"/><xs:enumeration value="m:gif"/>
        <xs:enumeration value="m:png"/><xs:enumeration value="m:jpg"/>
      </xs:restriction></xs:simpleType>
      <xs:complexType name="v"><xs:attribute name="v" type="n:nt"/></xs:complexType>
      <xs:element name="doc"><xs:complexType><xs:sequence>
        <xs:element name="a" form="unqualified" type="n:v" maxOccurs="unbounded"/>
        <xs:element name="b" form="unqualified" type="n:v" maxOccurs="unbounded"/>
        <xs:element name="c" form="unqualified" type="n:v" maxOccurs="unbounded"/>
        <xs:element name="d" form="unqualified" type="n:v" maxOccurs="unbounded"/>
        <xs:element name="q" form="unqualified">
          <xs:complexType><xs:attribute name="v" type="xs:QName"/></xs:complexType>
        </xs:element>
      </xs:sequence></xs:complexType></xs:element>
    </xs:schema>"#;

    /// 70 + 1 `a` and 70 + 1 `b` (one equal pair, the last); 70 `c` and 70 `d`
    /// that are all the same NOTATION, spelled `n:gif` and `m:gif`.
    fn instance() -> String {
        let mut xml = String::from(r#"<n:doc xmlns:n="urn:n" xmlns:m="urn:n">"#);
        xml.push_str(&r#"<a v="n:png"/>"#.repeat(70));
        xml.push_str(r#"<a v="n:gif"/>"#);
        xml.push_str(&r#"<b v="m:jpg"/>"#.repeat(70));
        xml.push_str(r#"<b v="m:gif"/>"#);
        xml.push_str(&r#"<c v="n:gif"/>"#.repeat(70));
        xml.push_str(&r#"<d v="m:gif"/>"#.repeat(70));
        xml.push_str(r#"<q v="n:gif"/></n:doc>"#);
        xml
    }

    #[test]
    fn notations_with_different_prefixes_pair_in_the_index() {
        let mut schema_set = SchemaSet::xsd11();
        load_and_process_schema(SCHEMA.as_bytes(), "n.xsd", &mut schema_set, None)
            .expect("the fixture schema loads");
        let arena = Bump::new();
        let xml = instance();
        let doc = build_typed_document(
            xml.as_bytes(),
            &arena,
            &schema_set,
            BufferDocumentOptions::default(),
        )
        .expect("the fixture document is built");
        let mut namespaces = NamespaceContextSnapshot::default();
        namespaces.bindings.push((
            schema_set.name_table.add("n"),
            schema_set.name_table.add("urn:n"),
        ));
        let ctx = XPathContext::new(&schema_set.name_table)
            .with_namespaces(namespaces)
            .with_schema_set(&schema_set);
        let cases = [
            (
                "data(/n:doc/a[last()]/@v) eq data(/n:doc/b[last()]/@v)",
                "true",
            ),
            ("/n:doc/a/@v = /n:doc/b/@v", "true"),
            ("data(/n:doc/a/@v) = data(/n:doc/b/@v)", "true"),
            ("/n:doc/a[position() < 71]/@v = /n:doc/b/@v", "false"),
            // The compare cache: the right operand is invariant in the predicate.
            ("count(/n:doc/b[@v = /n:doc/a/@v])", "1"),
            ("count(/n:doc/a[@v = /n:doc/b/@v])", "1"),
            ("/n:doc/c/@v = /n:doc/d/@v", "true"),
            ("/n:doc/c/@v != /n:doc/d/@v", "false"),
            ("count(/n:doc/d[@v != /n:doc/c/@v])", "0"),
            ("count(/n:doc/d[@v = /n:doc/c/@v])", "70"),
            ("count(distinct-values(data(/n:doc/(c|d)/@v)))", "1"),
            // A QName never meets a NOTATION (no row of the §3.5.1 table).
            ("/n:doc/q/@v = /n:doc/c/@v", "error:XPTY0004"),
        ];
        let mut wrong = Vec::new();
        for (expr, expected) in cases {
            let got = match XPathExpr::compile(expr, &ctx) {
                Err(e) => format!("static {e}"),
                Ok(compiled) => match compiled
                    .evaluator(&ctx)
                    .run_with_node(doc.create_navigator())
                {
                    Err(e) => format!("error:{}", e.error_code().unwrap_or("<none>")),
                    Ok(v) => v
                        .into_vec()
                        .iter()
                        .map(|item| match item {
                            XmlItem::Atomic(v) => v.to_string_value(),
                            XmlItem::Node(_) => "<node>".to_string(),
                        })
                        .collect::<Vec<_>>()
                        .join("|"),
                },
            };
            if got != expected {
                wrong.push(format!("{expr}  =>  {got}   (expected {expected})"));
            }
        }
        assert!(
            wrong.is_empty(),
            "{} of {} cases wrong:\n{}",
            wrong.len(),
            cases.len(),
            wrong.join("\n")
        );
    }
}

/// Regression tests for the findings of the 0.2.1 pre-release review of the
/// XPath engine (the `X-nn` numbers are the review's).
mod release_review_tests {
    use crate::namespace::table::NameTable;
    use crate::xpath::api::XPathExpr;
    use crate::xpath::iterator::XmlItem;
    use crate::xpath::{RoXmlNavigator, XPathContext};

    /// Evaluate `expr` (over `xml` when given) with `xs` bound. A result is its
    /// items' string values joined by `|`, `()` for the empty sequence,
    /// `<node>` for a node, and `error:CODE` for an error.
    fn run_on(expr: &str, xml: Option<&str>) -> String {
        let names = NameTable::new();
        let mut namespaces = crate::namespace::context::NamespaceContextSnapshot::default();
        namespaces.bindings.push((
            names.add("xs"),
            names.add("http://www.w3.org/2001/XMLSchema"),
        ));
        let ctx = XPathContext::new(&names).with_namespaces(namespaces);
        let code = |e: crate::xpath::XPathError| {
            format!(
                "error:{}",
                e.error_code()
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("<no code> {e}"))
            )
        };
        let compiled = match XPathExpr::compile(expr, &ctx) {
            Ok(compiled) => compiled,
            Err(e) => return code(e),
        };
        let result = match xml {
            Some(xml) => {
                let doc = roxmltree::Document::parse(xml).expect("parse xml");
                compiled
                    .evaluator(&ctx)
                    .run_with_node(RoXmlNavigator::new(&doc))
                    .map(|value| render(value.into_vec()))
            }
            None => compiled
                .evaluator(&ctx)
                .run::<RoXmlNavigator<'static>>()
                .map(|value| render(value.into_vec())),
        };
        result.unwrap_or_else(code)
    }

    fn render<N: crate::xpath::DomNavigator>(items: Vec<XmlItem<N>>) -> String {
        if items.is_empty() {
            return "()".to_string();
        }
        items
            .iter()
            .map(|item| match item {
                XmlItem::Atomic(v) => v.to_string_value(),
                XmlItem::Node(_) => "<node>".to_string(),
            })
            .collect::<Vec<_>>()
            .join("|")
    }

    /// Check every `(expression, expected)` pair and report all mismatches.
    fn check_on(xml: Option<&str>, cases: &[(&str, &str)]) {
        let wrong: Vec<String> = cases
            .iter()
            .filter_map(|(expr, expected)| {
                let got = run_on(expr, xml);
                (got != *expected).then(|| format!("{expr}  =>  {got}   (expected {expected})"))
            })
            .collect();
        assert!(
            wrong.is_empty(),
            "{} of {} cases wrong:\n{}",
            wrong.len(),
            cases.len(),
            wrong.join("\n")
        );
    }

    fn check(cases: &[(&str, &str)]) {
        check_on(None, cases);
    }

    /// X-10. F&O §15.4.5 (`fn:sum`): "Duration values must either all be
    /// xs:yearMonthDuration values or must all be xs:dayTimeDuration values. …
    /// If the above conditions are not met, a type error is raised
    /// [err:FORG0006]." §15.4.2 (`fn:avg`) says the same.
    #[test]
    fn sum_and_avg_of_a_type_mixture_are_forg0006() {
        check(&[
            ("sum((xs:dayTimeDuration('P1D'), 1))", "error:FORG0006"),
            ("sum((1, xs:dayTimeDuration('P1D')))", "error:FORG0006"),
            ("avg((xs:dayTimeDuration('P1D'), 1))", "error:FORG0006"),
            (
                "avg((xs:yearMonthDuration('P1Y'), xs:dayTimeDuration('P1D')))",
                "error:FORG0006",
            ),
            (
                "sum((xs:yearMonthDuration('P1Y'), xs:dayTimeDuration('P1D')))",
                "error:FORG0006",
            ),
            (
                "sum((xs:dayTimeDuration('P1D'), xs:untypedAtomic('1')))",
                "error:FORG0006",
            ),
            (
                "sum((xs:duration('P1Y'), xs:duration('P1Y')))",
                "error:FORG0006",
            ),
            ("sum(('a', 1))", "error:FORG0006"),
            // What is allowed stays allowed: any mixture of numeric types, and
            // a single duration type.
            ("sum((1, 2.5, xs:float(1), 1e0))", "5.5"),
            ("sum((xs:untypedAtomic('1'), 2))", "3"),
            (
                "sum((xs:dayTimeDuration('P1D'), xs:dayTimeDuration('PT12H')))",
                "P1DT12H",
            ),
            (
                "avg((xs:yearMonthDuration('P1Y'), xs:yearMonthDuration('P2Y')))",
                "P1Y6M",
            ),
            ("avg((1, 2, 3))", "2"),
        ]);
    }

    /// F2 (document API review). XPath 2.0 §3.2: a leading `/` is
    /// `(fn:root(self::node()) treat as document-node())/`, and "if the root node
    /// above the context node is not a document node, a dynamic error is raised
    /// [err:XPDY0050]" — the same for `//`. Under `BufferDocNavigator::new_orphan`
    /// with an element, the root is that element. An assertion navigator's root
    /// is the (hidden) document node, so it keeps answering `/` and `//` — with
    /// the empty sequence below it, which the W3C suite requires.
    #[test]
    fn a_leading_slash_under_a_parentless_element_is_xpdy0050() {
        use crate::document::{BufferDocNavigator, BufferDocument};
        let arena = bumpalo::Bump::new();
        let names = NameTable::new();
        let doc = BufferDocument::from_reader_default(
            b"<holder><kid><d1/><d2>t</d2></kid></holder>".as_slice(),
            &arena,
            &names,
        )
        .unwrap();
        let holder = doc.root() + 1;
        let kid = doc.first_content_child_of(holder).unwrap();
        let ctx = XPathContext::new(&names);
        let run = |nav: BufferDocNavigator<'_>, expr: &str| -> String {
            let code =
                |e: crate::xpath::XPathError| format!("error:{}", e.error_code().unwrap_or("?"));
            match XPathExpr::compile(expr, &ctx) {
                Err(e) => code(e),
                Ok(compiled) => compiled
                    .evaluator(&ctx)
                    .run_with_node(nav)
                    .map(|value| render(value.into_vec()))
                    .unwrap_or_else(code),
            }
        };
        for expr in [
            "count(/)",
            "count(//d2)",
            "count(//node())",
            "/kid",
            "exists(/)",
        ] {
            assert_eq!(
                run(BufferDocNavigator::new_orphan(&doc, kid), expr),
                "error:XPDY0050",
                "{expr} under an orphan element"
            );
        }
        // What does not go through `/` is unaffected.
        assert_eq!(
            run(BufferDocNavigator::new_orphan(&doc, kid), "count(root())"),
            "1"
        );
        assert_eq!(
            run(BufferDocNavigator::new_orphan(&doc, kid), "name(root())"),
            "kid"
        );
        assert_eq!(
            run(
                BufferDocNavigator::new_orphan(&doc, kid),
                "count(descendant::node())"
            ),
            "3"
        );
        // An assertion navigator: `/` is its document node, `//` is empty.
        assert_eq!(
            run(BufferDocNavigator::new_assertion(&doc, kid), "count(/)"),
            "1"
        );
        assert_eq!(
            run(BufferDocNavigator::new_assertion(&doc, kid), "count(//d2)"),
            "0"
        );
        // An ordinary navigator.
        assert_eq!(run(BufferDocNavigator::new(&doc, kid), "count(//d2)"), "1");
        assert_eq!(
            run(BufferDocNavigator::new(&doc, kid), "name(/*)"),
            "holder"
        );
    }

    /// X-02. XPath 2.0 §3.10.2 / §3.10.3: "In addition, the target type cannot
    /// be xs:NOTATION or xs:anyAtomicType [err:XPST0080]" — a static error,
    /// raised when the expression is compiled. Both stay legal in `instance of`
    /// and `treat as`, where they are ordinary atomic types. Since 0.3.0 the
    /// code has a variant of its own.
    #[test]
    fn cast_to_notation_or_any_atomic_type_is_xpst0080() {
        use crate::xpath::error::XPathError;
        let names = NameTable::new();
        let mut namespaces = crate::namespace::context::NamespaceContextSnapshot::default();
        namespaces.bindings.push((
            names.add("xs"),
            names.add("http://www.w3.org/2001/XMLSchema"),
        ));
        let ctx = XPathContext::new(&names).with_namespaces(namespaces);
        for expr in [
            "'a' castable as xs:NOTATION",
            "1 castable as xs:anyAtomicType",
            "1 cast as xs:NOTATION",
            "1 cast as xs:anyAtomicType?",
            "() cast as xs:NOTATION?",
            // Static: raised even where the cast is never evaluated.
            "if (true()) then 1 else (1 cast as xs:anyAtomicType)",
        ] {
            let err = match XPathExpr::compile(expr, &ctx) {
                Ok(_) => panic!("{expr}: compiled, expected XPST0080"),
                Err(err) => err,
            };
            assert_eq!(err.error_code(), Some("XPST0080"), "{expr}: {err}");
            assert!(
                matches!(err, XPathError::XPST0080 { .. }),
                "{expr}: {err:?}"
            );
            assert!(err.raised_error().is_none());
        }
        check(&[
            ("1 instance of xs:anyAtomicType", "true"),
            ("'a' instance of xs:NOTATION", "false"),
            ("1 treat as xs:anyAtomicType", "1"),
            ("'1' cast as xs:integer", "1"),
            ("'a' castable as xs:QName", "true"),
            // Not a type in the in-scope schema types at all: still XPST0051.
            ("1 cast as xs:IDREFS", "error:XPST0051"),
        ]);
    }

    /// X-07. F&O §15.1.5 (`fn:index-of`): "The items in the sequence
    /// $seqParam are compared with $srchParam under the rules for the eq
    /// operator. Values of type xs:untypedAtomic are compared as if they were
    /// of type xs:string. Values that cannot be compared, i.e. the eq operator
    /// is not defined for their types, are considered to be distinct." §15.1.6
    /// (`fn:distinct-values`) the same, with NaN equal to NaN.
    #[test]
    fn index_of_and_distinct_values_compare_with_eq() {
        check(&[
            ("index-of(xs:token('a'), 'a')", "1"),
            ("index-of(xs:ID('b'), 'b')", "1"),
            ("index-of(('a', xs:NCName('a'), xs:anyURI('a')), 'a')", "1|2|3"),
            ("count(distinct-values((xs:token('a'), 'a')))", "1"),
            ("count(distinct-values((xs:untypedAtomic('a'), 'a', xs:anyURI('a'))))", "1"),
            ("index-of((xs:untypedAtomic('a'), 'a'), 'a')", "1|2"),
            // Untyped is a string here, not a number.
            ("index-of((1, 2, 3), xs:untypedAtomic('2'))", "()"),
            // Numeric promotion.
            ("index-of((1, 2.0, 2e0, xs:float(2), 3), 2)", "2|3|4"),
            // Incomparable pairs are unequal, not errors.
            ("index-of(('a', 1, 'b', true()), 1)", "2"),
            ("count(distinct-values(('1', 1, true(), xs:date('2000-01-01'))))", "4"),
            // Dates and times by their point on the timeline.
            (
                "index-of((xs:dateTime('2000-01-01T12:00:00Z'), \
                 xs:dateTime('2000-01-01T13:00:00+01:00')), xs:dateTime('2000-01-01T12:00:00Z'))",
                "1|2",
            ),
            // Durations across their subtypes.
            (
                "count(distinct-values((xs:yearMonthDuration('P12M'), xs:yearMonthDuration('P1Y'), \
                 xs:dayTimeDuration('PT0S'), xs:yearMonthDuration('P0M'))))",
                "2",
            ),
            // QNames by expanded name.
            ("index-of(QName('urn:x', 'p:a'), QName('urn:x', 'q:a'))", "1"),
            // NaN: never `eq`, but one distinct value.
            ("index-of(xs:double('NaN'), xs:double('NaN'))", "()"),
            ("count(distinct-values((xs:double('NaN'), xs:float('NaN'), 0e0 div 0)))", "1"),
        ]);
    }

    /// X-08. XPath 2.0 §3.1.5: "If, after the above conversions, the resulting
    /// value does not match the expected type according to the rules for
    /// SequenceType Matching, a type error is raised [err:XPTY0004]"; §3.5.3:
    /// each operand of `is`, `<<`, `>>` "must be either a single node or an
    /// empty sequence; otherwise a type error is raised [err:XPTY0004]".
    /// XPDY0050 is `treat as`'s code.
    #[test]
    fn a_built_in_argument_of_the_wrong_cardinality_is_xpty0004() {
        let xml = "<doc><a><b/></a><a><b/></a></doc>";
        check_on(
            Some(xml),
            &[
                ("upper-case(('a','b'))", "error:XPTY0004"),
                ("abs((1,2))", "error:XPTY0004"),
                ("string-length(('a','b'))", "error:XPTY0004"),
                ("concat(('a','b'),'c')", "error:XPTY0004"),
                ("number((1,2))", "error:XPTY0004"),
                ("round(/doc/a)", "error:XPTY0004"),
                ("compare(('a','b'),'a')", "error:XPTY0004"),
                ("string-join(('a','b'),('x','y'))", "error:XPTY0004"),
                (
                    "year-from-date((xs:date('2000-01-01'), xs:date('2001-01-01')))",
                    "error:XPTY0004",
                ),
                // `node()?` parameters.
                ("name(/doc/a)", "error:XPTY0004"),
                ("root((/doc, /doc/a))", "error:XPTY0004"),
                ("local-name(1)", "error:XPTY0004"),
                ("(1, 2)[name() = '']", "error:XPTY0004"),
                // Node comparisons.
                ("/doc/a is /doc/a[1]", "error:XPTY0004"),
                ("/doc/a[1] << /doc/a", "error:XPTY0004"),
                // `treat as` keeps XPDY0050.
                ("(1, 2) treat as xs:integer", "error:XPDY0050"),
                // What is allowed stays allowed.
                ("upper-case(())", ""),
                ("name(/doc/none)", ""),
                ("name(/doc/a[1])", "a"),
                ("/doc/a[1] is /doc/a[1]", "true"),
                ("count(/doc/a[1] is ())", "0"),
                ("string-join(('a','b'), ',')", "a,b"),
            ],
        );
    }

    /// X-12. XPath 2.0 §3.4: "If the atomized operand is of type
    /// xs:untypedAtomic, it is cast to xs:double. If the cast fails, a dynamic
    /// error is raised. [err:FORG0001]" and "If the types of the operands, after
    /// evaluation, are not a valid combination for the given operator, according
    /// to the rules in B.2 Operator Mapping, a type error is raised
    /// [err:XPTY0004]"; B.2 maps unary `+` to `op:numeric-unary-plus($arg as
    /// numeric)`, whose result F&O §6.2.7 makes "an instance of xs:integer,
    /// xs:decimal, xs:double, or xs:float depending on the type of $arg".
    #[test]
    fn unary_plus_is_numeric_like_unary_minus() {
        check_on(
            Some("<doc><a>x</a><n>3</n></doc>"),
            &[
                ("+'3'", "error:XPTY0004"),
                ("+true()", "error:XPTY0004"),
                ("+xs:date('2000-01-01')", "error:XPTY0004"),
                ("+xs:untypedAtomic('3') instance of xs:double", "true"),
                ("+xs:untypedAtomic('3')", "3"),
                ("+/doc/n instance of xs:double", "true"),
                ("+/doc/a", "error:FORG0001"),
                ("+()", "()"),
                ("+(1, 2)", "error:XPTY0004"),
                ("+3", "3"),
                ("+3 instance of xs:integer", "true"),
                ("+xs:short(3) instance of xs:integer", "true"),
                ("+xs:short(3) instance of xs:short", "false"),
                ("+1.5 instance of xs:decimal", "true"),
                ("+xs:float(2) instance of xs:float", "true"),
                ("+-2", "-2"),
                // Unary minus, for comparison.
                ("-'3'", "error:XPTY0004"),
                ("-/doc/a", "error:FORG0001"),
            ],
        );
    }

    /// Check `(expression, expected)` pairs over `instance`, validated against
    /// `schema`, with the schema set in the static context.
    fn check_typed(schema: &str, instance: &str, cases: &[(&str, &str)]) {
        use crate::document::typed_builder::build_typed_document;
        use crate::document::BufferDocumentOptions;
        let mut schema_set = crate::schema::SchemaSet::xsd11();
        crate::pipeline::load_and_process_schema(schema.as_bytes(), "t.xsd", &mut schema_set, None)
            .expect("the fixture schema loads");
        let arena = bumpalo::Bump::new();
        let doc = build_typed_document(
            instance.as_bytes(),
            &arena,
            &schema_set,
            BufferDocumentOptions::default(),
        )
        .expect("the fixture document is built");
        let mut namespaces = crate::namespace::context::NamespaceContextSnapshot::default();
        namespaces.bindings.push((
            schema_set.name_table.add("xs"),
            schema_set
                .name_table
                .add("http://www.w3.org/2001/XMLSchema"),
        ));
        let ctx = XPathContext::new(&schema_set.name_table)
            .with_namespaces(namespaces)
            .with_schema_set(&schema_set);
        let code = |e: crate::xpath::XPathError| format!("error:{}", e.error_code().unwrap_or("?"));
        let wrong: Vec<String> = cases
            .iter()
            .filter_map(|(expr, expected)| {
                let got = match XPathExpr::compile(expr, &ctx) {
                    Err(e) => code(e),
                    Ok(compiled) => compiled
                        .evaluator(&ctx)
                        .run_with_node(doc.create_navigator())
                        .map(|value| {
                            let items = value.into_vec();
                            if items.is_empty() {
                                return "()".to_string();
                            }
                            items
                                .iter()
                                .map(|item| match item {
                                    XmlItem::Atomic(v) => v.to_string_value(),
                                    XmlItem::Node(n) => {
                                        use crate::xpath::DomNavigator;
                                        n.local_name().to_string()
                                    }
                                })
                                .collect::<Vec<_>>()
                                .join("|")
                        })
                        .unwrap_or_else(code),
                };
                (got != *expected).then(|| format!("{expr}  =>  {got}   (expected {expected})"))
            })
            .collect();
        assert!(
            wrong.is_empty(),
            "{} of {} cases wrong:\n{}",
            wrong.len(),
            cases.len(),
            wrong.join("\n")
        );
    }

    /// X-04. XPath 2.0 §2.5.4.3: "element(*, TypeName) matches an element node
    /// regardless of its name, if derives-from(AT, TypeName) is true … and the
    /// nilled property of the node is false"; "TypeName must be present in the
    /// in-scope schema types [err:XPST0008]"; §2.5.4.5 for attributes. A step
    /// `element(N, T)` now selects exactly what `instance of element(N, T)`
    /// accepts.
    #[test]
    fn a_typed_element_or_attribute_step_honours_the_type_name() {
        const SCHEMA: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
          <xs:complexType name="ct"><xs:sequence><xs:element name="i" type="xs:string"/></xs:sequence></xs:complexType>
          <xs:element name="doc">
            <xs:complexType>
              <xs:sequence>
                <xs:element name="n" type="xs:integer"/>
                <xs:element name="s" type="xs:string"/>
                <xs:element name="z" type="xs:integer" nillable="true"/>
                <xs:element name="c" type="ct"/>
              </xs:sequence>
              <xs:attribute name="a" type="xs:integer"/>
              <xs:attribute name="b" type="xs:string"/>
            </xs:complexType>
          </xs:element>
        </xs:schema>"#;
        const INSTANCE: &str = concat!(
            r#"<doc a="1" b="x" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">"#,
            r#"<n>1</n><s>x</s><z xsi:nil="true"/><c><i>t</i></c></doc>"#
        );
        check_typed(
            SCHEMA,
            INSTANCE,
            &[
                ("/doc/element(*, xs:integer)", "n"),
                ("/doc/element(*, xs:integer?)", "n|z"),
                ("/doc/s/self::element(*, xs:integer)", "()"),
                ("/doc/z/self::element(*, xs:integer)", "()"),
                ("/doc/element(s, xs:integer)", "()"),
                ("/doc/element(s, xs:string)", "s"),
                ("/doc/element(*, xs:decimal)", "n"),
                ("/doc/element(*, ct)", "c"),
                ("/doc/element(*, xs:anyType)", "n|s|c"),
                ("/doc/element(*, xs:anyType?)", "n|s|z|c"),
                ("/doc/attribute(*, xs:integer)", "a"),
                ("/doc/attribute(*, xs:anySimpleType)", "a|b"),
                ("/doc/@attribute(b, xs:string)", "b"),
                ("/doc/element(*, xs:integer)[1]", "n"),
                ("count(/doc/element(*, xs:string)[last()])", "1"),
                ("/document-node(element(doc, xs:integer))", "()"),
                ("count(/self::document-node(element(doc)))", "1"),
                // The step and `instance of` agree.
                ("/doc/s instance of element(*, xs:integer)", "false"),
                ("/doc/z instance of element(*, xs:integer)", "false"),
                ("/doc/n instance of element(*, xs:integer)", "true"),
                // Unknown type names.
                ("/doc/element(*, nosuchtype)", "error:XPST0008"),
                (
                    "/doc/n instance of element(*, nosuchtype)",
                    "error:XPST0008",
                ),
                ("/doc/attribute(*, xs:nosuchtype)", "error:XPST0008"),
            ],
        );
        // Untyped: only xs:untyped / xs:anyType (elements) and the untyped
        // atomic names (attributes) match, in a step as in `instance of`.
        check_on(
            Some(r#"<doc a="1"><n>1</n></doc>"#),
            &[
                ("count(/doc/element(*, xs:integer))", "0"),
                ("count(/doc/attribute(*, xs:integer))", "0"),
                ("count(/doc/element(*, xs:untyped))", "1"),
                ("count(/doc/attribute(*, xs:untypedAtomic))", "1"),
                ("/doc/n instance of element(*, xs:integer)", "false"),
                // Without a schema set only the built-in types are in scope.
                ("/doc/element(*, my-type)", "error:XPST0008"),
            ],
        );
    }

    /// X-13, the small items.
    #[test]
    fn minor_review_items() {
        check(&[
            // 13.9 — F&O §6.2.5: "If either operand is NaN or if $arg1 is INF
            // or -INF then an error is raised [err:FOAR0002]"; a zero divisor
            // is FOAR0001; an infinite divisor gives zero.
            ("xs:float('INF') idiv 1", "error:FOAR0002"),
            ("xs:double('-INF') idiv 1", "error:FOAR0002"),
            ("xs:double('NaN') idiv 1", "error:FOAR0002"),
            ("1 idiv xs:double('NaN')", "error:FOAR0002"),
            ("1e0 idiv 0", "error:FOAR0001"),
            ("xs:float(1) idiv xs:float(0)", "error:FOAR0001"),
            ("1e0 idiv xs:double('INF')", "0"),
            ("-7e0 idiv 2", "-3"),
            ("xs:float(31) idiv xs:float(7)", "4"),
            // A quotient beyond i64 is still exact.
            ("1e20 idiv 1", "100000000000000000000"),
            // 13.3 — XPath 2.0 A.2.1: DoubleLiteral needs exponent digits.
            ("1e", "error:XPST0003"),
            ("5.0e", "error:XPST0003"),
            ("1e+", "error:XPST0003"),
            ("1e1", "10"),
            (".5E-1", "0.05"),
            // 13.4 — only the XSD lexical forms; `inf`, `Infinity`, `nan` are
            // not numbers (XPath 2.0 Appendix I.1).
            ("xs:double('inf')", "error:FORG0001"),
            ("xs:double('Infinity')", "error:FORG0001"),
            ("xs:float('-inf')", "error:FORG0001"),
            ("xs:double('nan')", "error:FORG0001"),
            ("xs:double('-NaN')", "error:FORG0001"),
            ("xs:double(' INF ')", "INF"),
            ("xs:double('-INF')", "-INF"),
            ("xs:double('NaN')", "NaN"),
            ("xs:double('+1.e2')", "100"),
            ("number('inf')", "NaN"),
            ("number('Infinity')", "NaN"),
            ("number('INF')", "INF"),
            ("xs:untypedAtomic('Infinity') + 1", "error:FORG0001"),
            ("xs:untypedAtomic('inf') = 1", "error:FORG0001"),
            ("'inf' castable as xs:double", "false"),
            ("sum(xs:untypedAtomic('inf'))", "error:FORG0001"),
            // 13.2 — F&O §15.4.3/§15.4.4: the NaN returned is of the least
            // common type of the numbers.
            ("max((xs:float('NaN'), 1)) instance of xs:float", "true"),
            ("min((1, xs:float('NaN'))) instance of xs:float", "true"),
            (
                "max((xs:double('NaN'), xs:float(1))) instance of xs:double",
                "true",
            ),
            ("string(max((xs:float('NaN'), 1)))", "NaN"),
            // 13.7 — F&O §8.1: a base that is a relative reference, is not
            // hierarchic, or has a fragment identifier is FORG0002.
            ("resolve-uri('a', 'http://x/#f')", "error:FORG0002"),
            ("resolve-uri('a', 'mailto:me@x.org')", "error:FORG0002"),
            ("resolve-uri('', '')", "error:FORG0002"),
            ("resolve-uri('a', 'rel/base')", "error:FORG0002"),
            ("resolve-uri('a', 'http://x/b/c')", "http://x/b/a"),
            ("resolve-uri('a', 'file:///d/e')", "file:///d/a"),
            ("resolve-uri('http://y/z', '')", "http://y/z"),
        ]);
    }

    /// C63. F&O §8.1 raises FORG0002 when `$relative` "is not a valid URI
    /// according to the rules of the xs:anyURI data type, or if it is not a
    /// suitable relative reference"; Datatypes §3.3.17.2: "Spaces are, in
    /// principle, allowed in the ·lexical space· of anyURI". A character the
    /// IRI-to-URI escaping takes care of (a space, `<`, `{`, a non-ASCII
    /// letter) resolves, and the result is not percent-encoded (W3C bug 20672);
    /// a reference that stays malformed after escaping (two `#`, an empty
    /// scheme) is still FORG0002.
    #[test]
    fn resolve_uri_resolves_references_with_characters_any_uri_allows() {
        let base = "http://base.example.org/ns/";
        let resolve = |relative: &str| format!("resolve-uri('{relative}', '{base}')");
        check(&[
            (
                &resolve("two organizations"),
                "http://base.example.org/ns/two organizations",
            ),
            (
                &resolve("Hello There"),
                "http://base.example.org/ns/Hello There",
            ),
            (
                &resolve("Az Az , AA 13121-1314"),
                "http://base.example.org/ns/Az Az , AA 13121-1314",
            ),
            (
                &resolve("a<b>{c}|d^e`f\\g"),
                "http://base.example.org/ns/a<b>{c}|d^e`f\\g",
            ),
            (
                &resolve("caf\u{e9} x/../y z"),
                "http://base.example.org/ns/y z",
            ),
            (
                &resolve("a b?q r#f g"),
                "http://base.example.org/ns/a b?q r#f g",
            ),
            // Absolute references are returned unchanged.
            (&resolve("a:_underscore_"), "a:_underscore_"),
            (&resolve("DOM:co"), "DOM:co"),
            // A base with a space is a valid xs:anyURI too.
            ("resolve-uri('c d', 'http://x/a b/')", "http://x/a b/c d"),
            // Still FORG0002: malformed after escaping, or no suitable base.
            (&resolve("##some.uri"), "error:FORG0002"),
            (&resolve(":"), "error:FORG0002"),
            (
                "resolve-uri('a', 'http://localhost/base/##frag')",
                "error:FORG0002",
            ),
            ("resolve-uri('a', 'http://x/a b/#f')", "error:FORG0002"),
            ("resolve-uri('a', 'a b/')", "error:FORG0002"),
        ]);
    }

    /// C64. F&O §11.2.2 `fn:prefix-from-QName($arg as xs:QName?) as
    /// xs:NCName?`, §11.2.3 `fn:local-name-from-QName($arg as xs:QName?) as
    /// xs:NCName?`, §11.2.4 `fn:namespace-uri-from-QName(…) as xs:anyURI?`;
    /// §11.2.6 `fn:in-scope-prefixes`: "For namespaces that have a prefix, it
    /// returns the prefix as an xs:NCName. For the default namespace, which has
    /// no prefix, it returns the zero-length string." `fn:local-name` and
    /// `fn:name` (§14.1, §14.2) return an `xs:string`.
    #[test]
    fn qname_accessors_return_the_types_f_and_o_names() {
        let q = "QName('http://e/', 'p:person')";
        let xml = r#"<a xmlns="urn:d" xmlns:p="urn:p"/>"#;
        check_on(
            Some(xml),
            &[
                (
                    &format!("local-name-from-QName({q}) instance of xs:NCName"),
                    "true",
                ),
                (&format!("local-name-from-QName({q})"), "person"),
                (
                    &format!("prefix-from-QName({q}) instance of xs:NCName"),
                    "true",
                ),
                (&format!("prefix-from-QName({q})"), "p"),
                ("prefix-from-QName(QName('http://e/', 'person'))", "()"),
                ("local-name-from-QName(())", "()"),
                (
                    &format!("namespace-uri-from-QName({q}) instance of xs:anyURI"),
                    "true",
                ),
                (
                    "namespace-uri-for-prefix('p', /*) instance of xs:anyURI",
                    "true",
                ),
                (
                    "in-scope-prefixes(/*)[. = 'p'] instance of xs:NCName",
                    "true",
                ),
                (
                    "in-scope-prefixes(/*)[. = 'xml'] instance of xs:NCName",
                    "true",
                ),
                (
                    "in-scope-prefixes(/*)[. = ''] instance of xs:NCName",
                    "false",
                ),
                (
                    "in-scope-prefixes(/*)[. = ''] instance of xs:string",
                    "true",
                ),
                ("count(in-scope-prefixes(/*))", "3"),
                // The node accessors stay xs:string.
                ("local-name(/*) instance of xs:NCName", "false"),
                ("local-name(/*) instance of xs:string", "true"),
                ("name(/*) instance of xs:NCName", "false"),
            ],
        );
    }
}

/// Wave B (B2, B3): `fn:error` as a built-in, the XPDY0050 audit and its
/// messages.
mod wave_b_error_tests {
    use super::spec_helpers::{error_code, one};
    use crate::namespace::table::NameTable;
    use crate::xpath::api::XPathExpr;
    use crate::xpath::error::{XPathError, DEFAULT_RAISED_ERROR, XQT_ERRORS_NAMESPACE};
    use crate::xpath::{RoXmlNavigator, XPathContext};

    /// Compile and evaluate `expr` over `<a/>`, expecting an error.
    fn raised(expr: &str) -> XPathError {
        let names = NameTable::new();
        let mut namespaces = crate::namespace::context::NamespaceContextSnapshot::default();
        namespaces.bindings.push((
            names.add("xs"),
            names.add("http://www.w3.org/2001/XMLSchema"),
        ));
        let ctx = XPathContext::new(&names).with_namespaces(namespaces);
        let compiled = XPathExpr::compile(expr, &ctx).unwrap_or_else(|e| panic!("{expr}: {e}"));
        let doc = roxmltree::Document::parse("<a/>").expect("parse xml");
        match compiled
            .evaluator(&ctx)
            .run_with_node(RoXmlNavigator::new(&doc))
        {
            Ok(value) => panic!("{expr}: expected an error, got {} items", value.len()),
            Err(e) => e,
        }
    }

    /// F&O 1.0 §3: `fn:error() as none`, `fn:error($error as xs:QName) as
    /// none`, `fn:error($error as xs:QName?, $description as xs:string) as
    /// none`, `fn:error($error as xs:QName?, $description as xs:string,
    /// $error-object as item()*) as none` — a built-in, with no host
    /// registration.
    #[test]
    fn fn_error_is_a_built_in_with_four_signatures() {
        // No arguments: `err:FOER0000`.
        let e = raised("error()");
        assert_eq!(e.error_code(), Some("FOER0000"));
        let r = e.raised_error().expect("an error QName");
        assert_eq!(
            (r.namespace_uri, r.local_name),
            (XQT_ERRORS_NAMESPACE, DEFAULT_RAISED_ERROR)
        );
        // One argument: the QName as given.
        let e = raised("error(QName('urn:x', 'x:boom'))");
        let r = e.raised_error().expect("an error QName");
        assert_eq!((r.namespace_uri, r.local_name), ("urn:x", "boom"));
        assert_eq!(r.description, None);
        assert_eq!(e.error_code(), None);
        // Two arguments; an empty first argument is `err:FOER0000`.
        let e = raised("error((), 'why')");
        assert_eq!(e.error_code(), Some("FOER0000"));
        assert_eq!(e.raised_error().unwrap().description, Some("why"));
        let e = raised("error(QName('', 'local'), 'why')");
        let r = e.raised_error().unwrap();
        assert_eq!((r.namespace_uri, r.local_name), ("", "local"));
        // Three arguments.
        let e = raised("error(QName('urn:x', 'x:boom'), 'why', (1, 'two', /a))");
        assert_eq!(e.raised_error().unwrap().local_name, "boom");
        // The declared argument types are enforced.
        assert_eq!(error_code("error(())", "<a/>"), "XPTY0004");
        assert_eq!(error_code("error((), 42)", "<a/>"), "XPTY0004");
        // Arity 4 is not a signature.
        assert_eq!(error_code("error((), 'a', 1, 2)", "<a/>"), "XPST0017");
        // A call that is never evaluated raises nothing.
        assert_eq!(one("if (true()) then 1 else error()", "<a/>"), "1");
    }

    /// XPath 2.0 §3.10.5: a failed `treat as` "raises a dynamic error
    /// [err:XPDY0050]"; the message says what was expected and what was found.
    #[test]
    fn a_failed_treat_says_what_was_expected_and_found() {
        let e = raised("(1, 2) treat as xs:integer");
        assert_eq!(e.error_code(), Some("XPDY0050"));
        let text = e.to_string();
        assert!(text.contains("xs:integer"), "{text}");
        assert!(text.contains("2 items"), "{text}");

        let e = raised("'a' treat as xs:integer?");
        let text = e.to_string();
        assert!(text.contains("xs:integer?"), "{text}");
        assert!(text.contains("xs:string"), "{text}");

        let e = raised("() treat as element(a)+");
        let text = e.to_string();
        assert!(text.contains("element(a)+"), "{text}");
        assert!(text.contains("empty sequence"), "{text}");

        let e = raised("/a treat as attribute()");
        let text = e.to_string();
        assert!(text.contains("attribute()"), "{text}");
        assert!(text.contains("element"), "{text}");

        let e = raised("1 treat as empty-sequence()");
        assert!(e.to_string().contains("empty-sequence()"), "{e}");
    }

    /// XPath 2.0 §3.10.4: "For every atomic type in the in-scope schema types
    /// (except xs:NOTATION and xs:anyAtomicType, which are not instantiable), a
    /// constructor function is implicitly defined." So `xs:NOTATION(…)` and
    /// `xs:anyAtomicType(…)` name no function at all (XPST0017), while the
    /// `cast` to either is XPST0080 (§3.10.2).
    #[test]
    fn there_is_no_constructor_for_notation_or_any_atomic_type() {
        use super::spec_helpers::try_items;
        assert_eq!(
            [
                try_items("xs:NOTATION('a')", "<a/>"),
                try_items("xs:anyAtomicType('a')", "<a/>"),
                try_items("'a' cast as xs:anyAtomicType", "<a/>"),
                try_items("xs:string('a')", "<a/>"),
            ],
            [
                Err("XPST0017".to_string()),
                Err("XPST0017".to_string()),
                Err("XPST0080".to_string()),
                Ok(vec!["a".to_string()]),
            ]
        );
    }

    /// R13: XPath 2.0 §2.5.1 — "The in-scope schema types in the static
    /// context are initialized with a set of predefined schema types that is
    /// determined by the host language." This engine's predefined types are
    /// the built-in types it knows, the XSD 1.1 `xs:dateTimeStamp` and
    /// `xs:error` included, whatever schema set (or none) is attached — the
    /// same set for an `AtomicType`, a cast, a constructor function and the
    /// `TypeName` of an element or attribute test.
    #[test]
    fn the_predefined_types_include_the_xsd_1_1_built_ins_without_a_schema() {
        use super::spec_helpers::try_items;
        assert_eq!(
            [
                try_items(
                    "xs:dateTimeStamp('2000-01-01T00:00:00Z') instance of xs:dateTimeStamp",
                    "<a/>"
                ),
                try_items(
                    "'2000-01-01T00:00:00Z' castable as xs:dateTimeStamp",
                    "<a/>"
                ),
                try_items("/a instance of element(*, xs:dateTimeStamp)", "<a/>"),
                try_items("/a instance of element(*, xs:error)", "<a/>"),
                try_items("/a instance of element(*, xs:nosuch)", "<a/>"),
            ],
            [
                Ok(vec!["true".to_string()]),
                Ok(vec!["true".to_string()]),
                Ok(vec!["false".to_string()]),
                Ok(vec!["false".to_string()]),
                Err("XPST0008".to_string()),
            ]
        );
    }

    /// "More than one item where one is expected" is XPTY0004 (§3.1.5, §3.4,
    /// §3.10.2), not `treat as`'s XPDY0050.
    #[test]
    fn more_than_one_item_is_a_type_error() {
        assert_eq!(
            XPathError::more_than_one_item().error_code(),
            Some("XPTY0004")
        );
    }
}
