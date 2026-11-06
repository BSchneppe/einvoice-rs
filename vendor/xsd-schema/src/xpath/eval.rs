//! XPath AST evaluation phase.
//!
//! This module provides the `eval_node()` function which evaluates a bound
//! XPath AST at runtime. The AST must be bound using `bind_node()` before
//! evaluation.
//!
//! ## Supported Node Types
//!
//! Currently implemented:
//! - `Value` - Literal values (string, integer, double, boolean, empty)
//! - `ContextItem` - Context item reference (`.`)
//! - `VarRef` - Variable references
//! - `Expr` - Sequence expressions
//! - `If` - Conditional expressions
//! - `FunctionCall` - Function calls (dispatched via `eval_function`)
//!
//! Other node types return `not_implemented` errors for now.

use crate::ids::NameId;
use crate::types::{
    ItemType, NameTest as RuntimeNameTest, SequenceType, XmlTypeCardinality, XmlTypeCode,
};
use crate::xpath::arena::{AstArena, AstNodeId};
use crate::xpath::ast::{
    AstNode, Axis, BinaryOpKind, FilterExprNode, ForBinding, ForNode, ItemTypeNode,
    NodeTest as AstNodeTest, OccurrenceIndicator, PathExprNode, PathStepNode, QuantifiedNode,
    QuantifierKind, TypeExprKind, TypeExprNode, ValueNode,
};
use crate::xpath::axis_iterators::{
    AncestorAxis, AttributeAxis, ChildAxis, DescendantNodeIterator, FollowingNodeIterator,
    FollowingSiblingAxis, NamespaceAxis, ParentAxis, PrecedingNodeIterator, PrecedingSiblingAxis,
    SelfAxis, SequentialAxisNodeIterator,
};
use crate::xpath::cast::occurrence_allows_count;
use crate::xpath::context::{DynamicContext, XPathContext};
use crate::xpath::error::XPathError;
use crate::xpath::functions::{
    atomize_sequence, atomize_to_double, atomize_to_single_opt, effective_boolean_value,
    effective_boolean_value_10, string_of_single, XPathValue,
};
use crate::xpath::iterator::{
    DocumentOrderNodeIterator, VecNodeIterator, XmlItem, XmlNodeIterator,
};
use crate::xpath::node_ops::{following_node, get_root, preceding_node, same_node};
use crate::xpath::node_test::{
    matches_item_type_node, name_test_for_principal_kind, principal_node_kind, NodeTest,
};
use crate::xpath::operators::{
    eval_numeric_binary_10, eval_range, eval_unary, general_eq_iter_collated,
    general_ge_iter_collated, general_gt_iter_collated, general_le_iter_collated,
    general_lt_iter_collated, general_ne_iter_collated,
};
use crate::xpath::schema_cast::{cast_value, resolve_cast_target, CastTarget};
use crate::xpath::sequence_ops::{except_nodes, intersect_nodes, union_nodes};
use crate::xpath::DomNodeType;
use crate::xpath::{DomNavigator, XPathMode};

/// Evaluate an AST node and return the result.
///
/// This function recursively evaluates the AST, dispatching to appropriate
/// handlers based on node type. The AST must have been bound using `bind_node()`
/// before evaluation.
///
/// # Arguments
/// * `arena` - The AST arena containing all nodes
/// * `id` - The ID of the node to evaluate
/// * `ctx` - The dynamic context for evaluation
///
/// # Returns
/// * `Ok(XPathValue)` containing the evaluation result
/// * `Err(XPathError)` if evaluation fails
///
/// # Errors
/// * `XPDY0002` - Context item is undefined when required
/// * `XPST0008` - Variable is not bound
/// * Various function-specific errors
pub fn eval_node<N: DomNavigator>(
    arena: &AstArena,
    id: AstNodeId,
    ctx: &mut DynamicContext<'_, N>,
) -> Result<XPathValue<N>, XPathError> {
    let node = arena.get(id);

    match node {
        AstNode::Expr(expr) => {
            // Evaluate all items and concatenate results
            if expr.items.is_empty() {
                return Ok(XPathValue::empty());
            }

            if expr.items.len() == 1 {
                // Single item - no concatenation needed
                return eval_node(arena, expr.items[0], ctx);
            }

            // XPath 1.0: comma-separated sequences are not allowed
            // (the comma operator only appears in function args, which use a separate grammar production)
            if ctx.static_context.mode() == XPathMode::XPath10 {
                return Err(XPathError::XPST0003 {
                    message: "Sequence expressions (comma operator) are not available in XPath 1.0"
                        .to_string(),
                });
            }

            // Multiple items - collect all results
            let mut results: Vec<XmlItem<N>> = Vec::new();
            for item_id in &expr.items {
                let value = eval_node(arena, *item_id, ctx)?;
                results.extend(value.into_vec());
            }
            Ok(XPathValue::from_sequence(results))
        }

        AstNode::Value(value_node) => {
            // XPath 1.0: reject constructs that slipped past the lexer
            if ctx.static_context.mode() == XPathMode::XPath10 {
                if matches!(value_node, ValueNode::Empty) {
                    return Err(XPathError::XPST0003 {
                        message: "Empty sequence () is not available in XPath 1.0".to_string(),
                    });
                }
                if matches!(value_node, ValueNode::Double(_)) {
                    return Err(XPathError::XPST0003 {
                        message: "Double literals (e.g. 1e10) are not available in XPath 1.0"
                            .to_string(),
                    });
                }
            }
            // Convert ValueNode to XPathValue
            eval_value(value_node, ctx.static_context.mode())
        }

        AstNode::ContextItem(_) => {
            // Return the context item, or error if undefined
            match &ctx.context_item {
                Some(item) => Ok(XPathValue::from_item(item.clone())),
                None => Err(XPathError::XPDY0002 {
                    message: "Context item is undefined".to_string(),
                }),
            }
        }

        AstNode::VarRef(var_ref) => {
            // Get the variable value from the context
            let slot = var_ref
                .slot
                .ok_or_else(|| XPathError::Internal("Variable reference not bound".to_string()))?;

            ctx.get_variable(slot)
                .cloned()
                .ok_or_else(|| XPathError::XPDY0002 {
                    message: format!("Variable ${} is not set", var_ref.local_name),
                })
        }

        AstNode::If(if_node) => {
            // Evaluate condition and return appropriate branch
            let test_value = eval_node(arena, if_node.test, ctx)?;
            let condition = effective_boolean_value(&test_value)?;

            if condition {
                eval_node(arena, if_node.then_branch, ctx)
            } else {
                eval_node(arena, if_node.else_branch, ctx)
            }
        }

        AstNode::FunctionCall(func_call) => {
            // Get the resolved function handle
            let handle = func_call
                .function_handle
                .ok_or_else(|| XPathError::Internal("Function call not bound".to_string()))?;

            // Evaluate all arguments
            let mut args: Vec<XPathValue<N>> = Vec::with_capacity(func_call.args.len());
            for arg_id in &func_call.args {
                args.push(eval_node(arena, *arg_id, ctx)?);
            }

            // XPath 2.0 §3.1.5: in XPath 1.0 compatibility mode the function
            // conversion rules start with three extra steps.
            if ctx.static_context.xpath10_compatibility() {
                apply_function_conversion_10(
                    arena,
                    ctx.static_context,
                    handle,
                    &func_call.args,
                    &mut args,
                )?;
            }

            // Dispatch via the context's eval_function method (supports custom functions)
            ctx.eval_function(handle, args)
        }

        AstNode::For(for_node) => eval_for_expression(arena, for_node, ctx),

        AstNode::Quantified(quant_node) => eval_quantified_expression(arena, quant_node, ctx),

        AstNode::PathExpr(path_expr) => eval_path_expr(arena, path_expr, ctx),

        AstNode::FilterExpr(filter_expr) => eval_filter_expr(arena, filter_expr, ctx),

        AstNode::Range(range) => {
            let start_val = eval_node(arena, range.start, ctx)?;
            let end_val = eval_node(arena, range.end, ctx)?;

            // §3.3.1 gives `to` the operand type `xs:integer?`, so in XPath 1.0
            // compatibility mode the first §3.1.5 step applies and an operand of
            // more than one item is truncated to its first item instead of
            // raising a type error. The `fn:string` / `fn:number` steps do not
            // apply, because the expected type is neither `xs:string` nor
            // `xs:double`.
            let compat = ctx.static_context.xpath10_compatibility();
            let start_opt = if compat {
                first_atomized_item(start_val)?
            } else {
                atomize_operand(start_val, "xs:integer?")?
            };
            let end_opt = if compat {
                first_atomized_item(end_val)?
            } else {
                atomize_operand(end_val, "xs:integer?")?
            };

            match (start_opt, end_opt) {
                (None, _) | (_, None) => Ok(XPathValue::empty()),
                (Some(start), Some(end)) => {
                    // XPath 2.0 §3.3.1: the operands of `to` are `xs:integer?`,
                    // so the function conversion rules apply and an
                    // `xs:untypedAtomic` operand is cast to `xs:integer`.
                    let start = to_integer_operand(start)?;
                    let end = to_integer_operand(end)?;
                    let values = eval_range(&start, &end)?;
                    let items: Vec<XmlItem<N>> = values.into_iter().map(XmlItem::Atomic).collect();
                    Ok(XPathValue::from_sequence(items))
                }
            }
        }

        AstNode::UnaryOp(unary_op) => {
            let operand_val = eval_node(arena, unary_op.operand, ctx)?;

            if ctx.static_context.xpath10_compatibility() {
                // §3.4: in compatibility mode an empty operand makes the whole
                // arithmetic expression NaN, so there is no empty result.
                let xpath10_language = ebv_is_xpath10(ctx.static_context);
                let result = match arithmetic_operand_10(operand_val, xpath10_language)? {
                    Some(operand) => eval_unary(unary_op.kind, &operand)?,
                    None => crate::types::XmlValue::double(f64::NAN),
                };
                return Ok(XPathValue::from_atomic(result));
            }

            let opt = atomize_operand(operand_val, "a single numeric value")?;

            match opt {
                None => Ok(XPathValue::empty()),
                Some(operand) => {
                    let result = eval_unary(unary_op.kind, &operand)?;
                    Ok(XPathValue::from_atomic(result))
                }
            }
        }

        AstNode::BinaryOp(bin_op) => {
            match bin_op.kind {
                // Logical operators - short-circuit evaluation
                BinaryOpKind::And => {
                    let left_val = eval_node(arena, bin_op.left, ctx)?;
                    let left_bool = if ebv_is_xpath10(ctx.static_context) {
                        effective_boolean_value_10(&left_val)?
                    } else {
                        effective_boolean_value(&left_val)?
                    };
                    if !left_bool {
                        return Ok(XPathValue::boolean(false));
                    }
                    let right_val = eval_node(arena, bin_op.right, ctx)?;
                    let right_bool = if ebv_is_xpath10(ctx.static_context) {
                        effective_boolean_value_10(&right_val)?
                    } else {
                        effective_boolean_value(&right_val)?
                    };
                    Ok(XPathValue::boolean(right_bool))
                }
                BinaryOpKind::Or => {
                    let left_val = eval_node(arena, bin_op.left, ctx)?;
                    let left_bool = if ebv_is_xpath10(ctx.static_context) {
                        effective_boolean_value_10(&left_val)?
                    } else {
                        effective_boolean_value(&left_val)?
                    };
                    if left_bool {
                        return Ok(XPathValue::boolean(true));
                    }
                    let right_val = eval_node(arena, bin_op.right, ctx)?;
                    let right_bool = if ebv_is_xpath10(ctx.static_context) {
                        effective_boolean_value_10(&right_val)?
                    } else {
                        effective_boolean_value(&right_val)?
                    };
                    Ok(XPathValue::boolean(right_bool))
                }

                // Arithmetic and value comparison operators - atomize to single values
                BinaryOpKind::Add
                | BinaryOpKind::Sub
                | BinaryOpKind::Mul
                | BinaryOpKind::Div
                | BinaryOpKind::IDiv
                | BinaryOpKind::Mod
                | BinaryOpKind::ValueEq
                | BinaryOpKind::ValueNe
                | BinaryOpKind::ValueLt
                | BinaryOpKind::ValueLe
                | BinaryOpKind::ValueGt
                | BinaryOpKind::ValueGe => {
                    let left_val = eval_node(arena, bin_op.left, ctx)?;
                    let right_val = eval_node(arena, bin_op.right, ctx)?;

                    let is_arithmetic = matches!(
                        bin_op.kind,
                        BinaryOpKind::Add
                            | BinaryOpKind::Sub
                            | BinaryOpKind::Mul
                            | BinaryOpKind::Div
                            | BinaryOpKind::IDiv
                            | BinaryOpKind::Mod
                    );

                    // §3.4: in XPath 1.0 compatibility mode an arithmetic
                    // operand is truncated to its first item, an empty operand
                    // makes the expression NaN, and a boolean, string, decimal,
                    // float or untypedAtomic operand is converted with
                    // `fn:number`. The rule is specific to arithmetic; the value
                    // comparisons (which XPath 1.0 does not have) keep the 2.0
                    // rules.
                    if is_arithmetic && ctx.static_context.xpath10_compatibility() {
                        let xpath10_language = ebv_is_xpath10(ctx.static_context);
                        let left = arithmetic_operand_10(left_val, xpath10_language)?;
                        let right = arithmetic_operand_10(right_val, xpath10_language)?;
                        let (Some(left), Some(right)) = (left, right) else {
                            // "If the atomized operand is an empty sequence, the
                            // result of the arithmetic expression is the
                            // xs:double value NaN."
                            return Ok(XPathValue::from_atomic(crate::types::XmlValue::double(
                                f64::NAN,
                            )));
                        };
                        // After the conversion every numeric operand is an
                        // xs:double. §3.4's list deliberately leaves the date,
                        // time and duration types alone, so an operand of one of
                        // those keeps the XPath 2.0 operator mapping — as does
                        // `idiv`, which has no XPath 1.0 counterpart and must
                        // keep its integer result type.
                        let numeric_pair =
                            left.type_code.is_numeric() && right.type_code.is_numeric();
                        let result = if numeric_pair && !matches!(bin_op.kind, BinaryOpKind::IDiv) {
                            eval_numeric_binary_10(bin_op.kind, &left, &right)?
                        } else {
                            let mut truncated = false;
                            let result = crate::xpath::operators::eval_binary_noted(
                                bin_op.kind,
                                &left,
                                &right,
                                crate::xpath::collation::CollationRef::Codepoint,
                                ctx.implicit_timezone_or_host(),
                                &mut truncated,
                            )?;
                            if truncated {
                                warn_truncated_seconds(ctx, arena, id, bin_op.kind, &result);
                            }
                            result
                        };
                        return Ok(XPathValue::from_atomic(result));
                    }

                    let left_opt = atomize_operand(left_val, "a single atomic value")?;
                    let right_opt = atomize_operand(right_val, "a single atomic value")?;

                    match (left_opt, right_opt) {
                        (None, _) | (_, None) => Ok(XPathValue::empty()),
                        (Some(left), Some(right)) => {
                            // XPath 2.0 §2.1.1 makes the static context's
                            // default collation "the collation to be used by
                            // functions and operators for comparing and
                            // ordering values of type xs:string and xs:anyURI
                            // (and types derived from them)", and §B.2 spells
                            // `A eq B` for two `xs:string` values as
                            // `op:numeric-equal(fn:compare(A, B), 0)`. Only the
                            // comparison operators read it; the arithmetic ones
                            // ignore it. It is resolved only if two strings
                            // are actually compared (and then through the
                            // per-run memo), and costs a discriminant test when
                            // it is the codepoint collation. A date/time
                            // operand without a timezone is read in the
                            // implicit timezone of this dynamic context (F&O
                            // §10.4), in comparisons and in subtraction.
                            let tz = ctx.implicit_timezone_or_host();
                            let deferred = ctx.deferred_default_collation();
                            let mut truncated = false;
                            let result = crate::xpath::operators::eval_binary_noted(
                                bin_op.kind,
                                &left,
                                &right,
                                deferred.as_ref(),
                                tz,
                                &mut truncated,
                            )?;
                            drop(deferred);
                            if truncated {
                                warn_truncated_seconds(ctx, arena, id, bin_op.kind, &result);
                            }
                            Ok(XPathValue::from_atomic(result))
                        }
                    }
                }

                // General comparisons - use Cartesian product semantics
                BinaryOpKind::GeneralEq
                | BinaryOpKind::GeneralNe
                | BinaryOpKind::GeneralLt
                | BinaryOpKind::GeneralLe
                | BinaryOpKind::GeneralGt
                | BinaryOpKind::GeneralGe => {
                    // `=` and `!=` outside XPath 1.0 compatibility mode go
                    // through `compare_cache`, which may reuse the index of an
                    // operand that does not change during this run. With nothing
                    // to reuse it evaluates both operands in source order and
                    // calls the very same `general_eq_iter`/`general_ne_iter` code.
                    if !ctx.static_context.xpath10_compatibility()
                        && matches!(
                            bin_op.kind,
                            BinaryOpKind::GeneralEq | BinaryOpKind::GeneralNe
                        )
                    {
                        let result = crate::xpath::compare_cache::eval_general_eq_ne(
                            arena,
                            id,
                            bin_op.left,
                            bin_op.right,
                            matches!(bin_op.kind, BinaryOpKind::GeneralEq),
                            ctx,
                        )?;
                        return Ok(XPathValue::boolean(result));
                    }

                    let left_val = eval_node(arena, bin_op.left, ctx)?;
                    let right_val = eval_node(arena, bin_op.right, ctx)?;

                    if ctx.static_context.xpath10_compatibility() {
                        let left_is_bool = is_boolean_value(&left_val);
                        let right_is_bool = is_boolean_value(&right_val);
                        let mode10 = ebv_is_xpath10(ctx.static_context);
                        // XPath 1.0 (`XPathMode::XPath10`) §3.4 converts only a
                        // *node-set* compared with a boolean as a whole; other
                        // operands are converted pair by pair below, and `<` …
                        // `>=` convert both sides to numbers. XPath 2.0 §3.5.2,
                        // in compatibility mode: "If either operand is a single
                        // atomic value that is an instance of xs:boolean, then
                        // the other operand is converted to xs:boolean by taking
                        // its effective boolean value" — whatever it is, for all
                        // six operators, and with the 2.0 effective boolean
                        // value (§2.4.3), which is FORG0006 for a sequence of
                        // two or more atomic values.
                        let convert_other = if mode10 {
                            (left_is_bool && has_nodes_or_empty(&right_val))
                                || (right_is_bool && has_nodes_or_empty(&left_val))
                        } else {
                            left_is_bool || right_is_bool
                        };

                        if convert_other {
                            let (l, r) = if mode10 {
                                (
                                    effective_boolean_value_10(&left_val)?,
                                    effective_boolean_value_10(&right_val)?,
                                )
                            } else {
                                (
                                    effective_boolean_value(&left_val)?,
                                    effective_boolean_value(&right_val)?,
                                )
                            };
                            let result = match bin_op.kind {
                                BinaryOpKind::GeneralEq => l == r,
                                BinaryOpKind::GeneralNe => l != r,
                                BinaryOpKind::GeneralLt
                                | BinaryOpKind::GeneralLe
                                | BinaryOpKind::GeneralGt
                                | BinaryOpKind::GeneralGe => {
                                    let ln = if l { 1.0_f64 } else { 0.0 };
                                    let rn = if r { 1.0_f64 } else { 0.0 };
                                    match bin_op.kind {
                                        BinaryOpKind::GeneralLt => ln < rn,
                                        BinaryOpKind::GeneralLe => ln <= rn,
                                        BinaryOpKind::GeneralGt => ln > rn,
                                        BinaryOpKind::GeneralGe => ln >= rn,
                                        _ => unreachable!(),
                                    }
                                }
                                _ => unreachable!(),
                            };
                            return Ok(XPathValue::boolean(result));
                        }
                    }

                    let left_iter = VecNodeIterator::new(left_val.into_vec());
                    let right_iter = VecNodeIterator::new(right_val.into_vec());

                    let result = if ctx.static_context.xpath10_compatibility() {
                        // §3.5.2 compares the converted operands with `eq`,
                        // `ne`, …, so two strings are compared under the
                        // default collation, as the value comparisons above
                        // compare them — resolved only if two strings meet.
                        //
                        // In the XPath 1.0 language (`XPathMode::XPath10`) the
                        // pairs follow XPath 1.0 §3.4; an XPath 2.0 expression
                        // in compatibility mode follows §3.5.2 after
                        // atomization, with the static context for its casts.
                        let static_context = ctx.static_context;
                        let compat = (!ebv_is_xpath10(static_context)).then_some(static_context);
                        let tz = ctx.implicit_timezone_or_host();
                        let deferred = ctx.deferred_default_collation();
                        crate::xpath::operators::general_compare_iter_10(
                            bin_op.kind,
                            &left_iter,
                            &right_iter,
                            deferred.as_ref(),
                            tz,
                            compat,
                        )?
                    } else {
                        // The default collation is resolved only if two strings
                        // are compared, and then through the per-run memo.
                        // F&O §10.4: the implicit timezone of the dynamic
                        // context, for date/time values without one.
                        let static_context = ctx.static_context;
                        let tz = ctx.implicit_timezone_or_host();
                        let deferred = ctx.deferred_default_collation();
                        let collation = deferred.as_ref();
                        let (l, r) = (&left_iter, &right_iter);
                        match bin_op.kind {
                            BinaryOpKind::GeneralEq => {
                                general_eq_iter_collated(static_context, l, r, collation, tz)?
                            }
                            BinaryOpKind::GeneralNe => {
                                general_ne_iter_collated(static_context, l, r, collation, tz)?
                            }
                            BinaryOpKind::GeneralLt => {
                                general_lt_iter_collated(static_context, l, r, collation, tz)?
                            }
                            BinaryOpKind::GeneralLe => {
                                general_le_iter_collated(static_context, l, r, collation, tz)?
                            }
                            BinaryOpKind::GeneralGt => {
                                general_gt_iter_collated(static_context, l, r, collation, tz)?
                            }
                            BinaryOpKind::GeneralGe => {
                                general_ge_iter_collated(static_context, l, r, collation, tz)?
                            }
                            _ => unreachable!(),
                        }
                    };
                    Ok(XPathValue::boolean(result))
                }

                // Node comparisons - use node identity/document order
                BinaryOpKind::Is | BinaryOpKind::Before | BinaryOpKind::After => {
                    let left_val = eval_node(arena, bin_op.left, ctx)?;
                    let right_val = eval_node(arena, bin_op.right, ctx)?;

                    let left_node = extract_single_node(left_val)?;
                    let right_node = extract_single_node(right_val)?;

                    // Per XPath 2.0 spec: if either operand is empty, result is empty sequence
                    match (left_node, right_node) {
                        (Some(left), Some(right)) => {
                            let result = match bin_op.kind {
                                BinaryOpKind::Is => same_node(&left, &right),
                                BinaryOpKind::Before => preceding_node(&left, &right),
                                BinaryOpKind::After => following_node(&left, &right),
                                _ => unreachable!(),
                            };
                            Ok(XPathValue::boolean(result))
                        }
                        _ => Ok(XPathValue::empty()),
                    }
                }

                // Sequence operators - node-only, return document order with duplicates removed
                BinaryOpKind::Union | BinaryOpKind::Intersect | BinaryOpKind::Except => {
                    let left_val = eval_node(arena, bin_op.left, ctx)?;
                    let right_val = eval_node(arena, bin_op.right, ctx)?;

                    let left_vec = left_val.into_vec();
                    let right_vec = right_val.into_vec();

                    let result = match bin_op.kind {
                        BinaryOpKind::Union => union_nodes(left_vec, right_vec)?,
                        BinaryOpKind::Intersect => intersect_nodes(left_vec, right_vec)?,
                        BinaryOpKind::Except => except_nodes(left_vec, right_vec)?,
                        _ => unreachable!(),
                    };
                    Ok(XPathValue::from_sequence(result))
                }
            }
        }

        AstNode::PathStep(_) => {
            // PathStep should not be evaluated directly - it's processed via eval_path_step
            Err(XPathError::Internal(
                "PathStep should not be evaluated directly".to_string(),
            ))
        }

        AstNode::TypeExpr(type_expr) => eval_type_expr(arena, type_expr, ctx),
    }
}

// ============================================================================
// Type Expression Evaluation
// ============================================================================

/// Evaluate a type expression (`instance of`, `treat as`, `cast as`, `castable as`).
fn eval_type_expr<N: DomNavigator>(
    arena: &AstArena,
    type_expr: &TypeExprNode,
    ctx: &mut DynamicContext<'_, N>,
) -> Result<XPathValue<N>, XPathError> {
    // Evaluate the operand
    let operand = eval_node(arena, type_expr.operand, ctx)?;
    // XPath 2.0 §3.10.2: a cast to `xs:QName` (or a type derived from it or
    // from `xs:NOTATION`) from a string needs a string literal.
    let string_literal = is_string_literal_expression(arena, type_expr.operand);

    match type_expr.kind {
        TypeExprKind::InstanceOf => eval_instance_of(operand, type_expr, ctx),
        TypeExprKind::TreatAs => eval_treat_as(operand, type_expr, ctx),
        TypeExprKind::CastAs => eval_cast_as(operand, type_expr, string_literal, ctx),
        TypeExprKind::CastableAs => eval_castable_as(operand, type_expr, string_literal, ctx),
    }
}

/// Evaluate `expr instance of type`.
///
/// Returns true if the value matches the sequence type (cardinality + item type).
fn eval_instance_of<N: DomNavigator>(
    operand: XPathValue<N>,
    type_expr: &TypeExprNode,
    ctx: &DynamicContext<'_, N>,
) -> Result<XPathValue<N>, XPathError> {
    let items = operand.into_vec();
    let count = items.len();

    // Handle empty-sequence() first (special case - only matches empty)
    if type_expr.target_type.item_type.is_none() {
        return Ok(XPathValue::boolean(count == 0));
    }

    // Check cardinality
    if !occurrence_allows_count(type_expr.target_type.occurrence, count) {
        return Ok(XPathValue::boolean(false));
    }

    // Get item type (we know it's Some from the check above)
    let item_type = type_expr.target_type.item_type.as_ref().unwrap();

    // Check each item matches the item type
    for item in &items {
        if !item_matches_type_expr(item, item_type, type_expr, ctx) {
            return Ok(XPathValue::boolean(false));
        }
    }

    Ok(XPathValue::boolean(true))
}

/// Whether `item` matches the item type of an `instance of` / `treat as`:
/// against the item type the binder resolved, or — for an expression tree that
/// was never bound — by resolving it now.
fn item_matches_type_expr<N: DomNavigator>(
    item: &XmlItem<N>,
    item_type: &ItemTypeNode,
    type_expr: &TypeExprNode,
    ctx: &DynamicContext<'_, N>,
) -> bool {
    match &type_expr.resolved_item_type {
        Some(resolved) => resolved.matches_item(item, ctx.static_context),
        None => matches_item_type_node(
            item,
            item_type,
            type_expr.resolved_atomic_type.as_ref(),
            ctx.static_context,
        ),
    }
}

/// Evaluate `expr treat as type`.
///
/// Returns the value unchanged if it matches the sequence type, otherwise
/// raises `XPDY0050`.
///
/// XPath 2.0 §3.10.5: "If `expr1` matches `type1`, using the rules for
/// SequenceType matching, the `treat` expression returns the value of `expr1`;
/// otherwise, it raises a dynamic error [err:XPDY0050]." That covers every way
/// the match can fail — the wrong cardinality just as much as the wrong item
/// type — so `treat as` never raises a type error of its own. The error says
/// which sequence type was expected and what the operand was.
fn eval_treat_as<N: DomNavigator>(
    operand: XPathValue<N>,
    type_expr: &TypeExprNode,
    ctx: &DynamicContext<'_, N>,
) -> Result<XPathValue<N>, XPathError> {
    let items = operand.into_vec();
    let count = items.len();
    let mismatch = |found: String| Err(XPathError::treat_mismatch(&type_expr.target_type, found));

    // Handle empty-sequence() first: §2.5.4 gives it no OccurrenceIndicator of
    // its own, so it must not be run through the cardinality check.
    let item_type = match &type_expr.target_type.item_type {
        None => {
            // empty-sequence() - only accepts empty
            if count == 0 {
                return Ok(XPathValue::empty());
            } else {
                return mismatch(describe_items(&items));
            }
        }
        Some(it) => it,
    };

    // Check cardinality
    if !occurrence_allows_count(type_expr.target_type.occurrence, count) {
        return mismatch(describe_items(&items));
    }

    // Check each item matches the item type
    for (index, item) in items.iter().enumerate() {
        if !item_matches_type_expr(item, item_type, type_expr, ctx) {
            return mismatch(if count == 1 {
                describe_item(item)
            } else {
                format!(
                    "a sequence of {count} items whose item {} is {}",
                    index + 1,
                    describe_item(item)
                )
            });
        }
    }

    // Return the original value
    Ok(XPathValue::from_sequence(items))
}

/// A short description of a value for an error message: "the empty sequence",
/// "a sequence of 3 items", or the one item.
fn describe_items<N: DomNavigator>(items: &[XmlItem<N>]) -> String {
    match items {
        [] => "the empty sequence".to_string(),
        [item] => describe_item(item),
        _ => format!("a sequence of {} items", items.len()),
    }
}

/// A short description of one item for an error message: "an xs:string
/// value", "an element node named 'a'", "a text node".
fn describe_item<N: DomNavigator>(item: &XmlItem<N>) -> String {
    match item {
        XmlItem::Atomic(value) => match value.type_code.local_name() {
            Some(name) => format!("an xs:{name} value"),
            None => "an atomic value".to_string(),
        },
        XmlItem::Node(nav) => {
            let kind = match nav.node_type() {
                DomNodeType::Root => "a document node",
                DomNodeType::Element => "an element node",
                DomNodeType::Attribute => "an attribute node",
                DomNodeType::Namespace => "a namespace node",
                DomNodeType::ProcessingInstruction => "a processing-instruction node",
                DomNodeType::Comment => "a comment node",
                _ => "a text node",
            };
            let name = nav.name();
            if name.is_empty() {
                kind.to_string()
            } else {
                format!("{kind} named '{name}'")
            }
        }
    }
}

/// Evaluate `expr cast as type`.
///
/// Atomizes the operand and casts to the target atomic type.
fn eval_cast_as<N: DomNavigator>(
    operand: XPathValue<N>,
    type_expr: &TypeExprNode,
    string_literal: bool,
    ctx: &DynamicContext<'_, N>,
) -> Result<XPathValue<N>, XPathError> {
    // Cast only works with atomic types
    let item_type =
        type_expr
            .target_type
            .item_type
            .as_ref()
            .ok_or_else(|| XPathError::XPTY0004 {
                expected: "atomic type".to_string(),
                found: "empty-sequence()".to_string(),
            })?;

    // The item type must be Atomic for cast
    if !matches!(item_type, ItemTypeNode::Atomic(_)) {
        return Err(XPathError::XPTY0004 {
            expected: "atomic type".to_string(),
            found: "non-atomic type".to_string(),
        });
    }

    // Atomize the operand to get at most one atomic value. XPath 2.0 §3.10.2:
    // "If the result of atomization is a sequence of more than one atomic value,
    // a type error is raised [err:XPTY0004]."
    let atomic_opt = atomize_operand(operand, "a single atomic value")?;

    // Check cardinality
    let allows_empty = matches!(
        type_expr.target_type.occurrence,
        OccurrenceIndicator::ZeroOrOne | OccurrenceIndicator::ZeroOrMore
    );

    match atomic_opt {
        None => {
            if allows_empty {
                Ok(XPathValue::empty())
            } else {
                Err(XPathError::XPTY0004 {
                    expected: type_expr.target_type.to_string(),
                    found: "empty-sequence()".to_string(),
                })
            }
        }
        Some(value) => {
            let target = cast_target(type_expr, ctx.static_context)?;
            let type_name = || match &type_expr.target_type.item_type {
                Some(ItemTypeNode::Atomic(name)) => name.to_string(),
                _ => type_expr.target_type.to_string(),
            };
            if !name_cast_source_allowed(&value, target, string_literal) {
                return Err(XPathError::XPTY0004 {
                    expected: format!(
                        "a string literal or a value of the base type of {}",
                        type_name()
                    ),
                    found: format!("a computed {}", describe_type_code(value.type_code)),
                });
            }
            let result = cast_value(ctx.static_context, &value, target, &type_name)?;
            Ok(XPathValue::from_atomic(result))
        }
    }
}

/// Whether `value` may be cast to `target` as far as the rule for the names
/// goes. XPath 2.0 §3.10.2: "If the target type of a cast expression is
/// xs:QName, or is a type that is derived from xs:QName or xs:NOTATION, and
/// if the base type of the input is not the same as the base type of the
/// target type, then the input expression must be a string literal
/// \[err:XPTY0004\]"; §3.10.4 applies it to the constructor functions, and
/// F&O 1.0 §17.1.1: "If the argument to such a cast is computed dynamically,
/// \[err:XPTY0004\] is raised if the value is of any type other than xs:QName
/// or xs:NOTATION respectively (including the case where it is an
/// xs:string)". So the value is an `xs:QName` for an `xs:QName` target, an
/// `xs:NOTATION` for a `xs:NOTATION` one (their type codes), or the operand
/// is a string literal; every other target is unaffected.
fn name_cast_source_allowed(
    value: &crate::types::XmlValue,
    target: CastTarget,
    string_literal: bool,
) -> bool {
    let builtin = match target {
        CastTarget::BuiltIn(code) => code,
        CastTarget::Schema { builtin, .. } => builtin,
    };
    match builtin {
        XmlTypeCode::QName | XmlTypeCode::Notation => {
            value.type_code == builtin || (string_literal && value.type_code == XmlTypeCode::String)
        }
        _ => true,
    }
}

/// `xs:<name>` of a type code, for an error message.
fn describe_type_code(code: XmlTypeCode) -> String {
    match code.local_name() {
        Some(name) => format!("xs:{name}"),
        None => "atomic value".to_string(),
    }
}

/// The target type of a bound `cast as` / `castable as`: the binder's
/// resolution, or — for a node bound before it cached one — the resolution of
/// its expanded name now.
fn cast_target(type_expr: &TypeExprNode, ctx: &XPathContext<'_>) -> Result<CastTarget, XPathError> {
    if let Some(target) = type_expr.resolved_cast_target {
        return Ok(target);
    }
    let qname = type_expr
        .resolved_atomic_type
        .as_ref()
        .ok_or_else(|| XPathError::Internal("Cast target type not resolved".to_string()))?;
    resolve_cast_target(qname, ctx)
}

/// Evaluate `expr castable as type`.
///
/// Returns true if the cast would succeed, false otherwise.
fn eval_castable_as<N: DomNavigator>(
    operand: XPathValue<N>,
    type_expr: &TypeExprNode,
    string_literal: bool,
    ctx: &DynamicContext<'_, N>,
) -> Result<XPathValue<N>, XPathError> {
    // Cast only works with atomic types
    let item_type = type_expr.target_type.item_type.as_ref();
    if !matches!(item_type, Some(ItemTypeNode::Atomic(_))) {
        return Ok(XPathValue::boolean(false));
    }

    // Atomize the operand
    let atomic_opt = match atomize_to_single_opt(operand) {
        Ok(opt) => opt,
        Err(_) => return Ok(XPathValue::boolean(false)), // More than one item
    };

    // Check cardinality
    let allows_empty = matches!(
        type_expr.target_type.occurrence,
        OccurrenceIndicator::ZeroOrOne | OccurrenceIndicator::ZeroOrMore
    );

    match atomic_opt {
        None => {
            // Empty sequence - allowed if occurrence allows it
            Ok(XPathValue::boolean(allows_empty))
        }
        Some(value) => {
            let Ok(target) = cast_target(type_expr, ctx.static_context) else {
                return Ok(XPathValue::boolean(false));
            };
            // §3.10.3: "If the target type of a castable expression is
            // xs:QName, or is a type that is derived from xs:QName or
            // xs:NOTATION, and the input argument of the expression is of type
            // xs:string but it is not a literal string, the result of the
            // castable expression is false."
            let is_castable = name_cast_source_allowed(&value, target, string_literal)
                && cast_value(ctx.static_context, &value, target, &String::new).is_ok();
            Ok(XPathValue::boolean(is_castable))
        }
    }
}

/// Atomize an operand of an operator that takes at most one atomic value.
///
/// XPath 2.0 states the same rule for arithmetic (§3.4), value comparisons
/// (§3.5.1), `to` (§3.3.1, via the function conversion rules) and `cast`
/// (§3.10.2): "If the atomized operand is a sequence of length greater than one,
/// a type error is raised [err:XPTY0004]." `expected` names the operand type in
/// that error.
fn atomize_operand<N: DomNavigator>(
    value: XPathValue<N>,
    expected: &str,
) -> Result<Option<crate::types::XmlValue>, XPathError> {
    crate::xpath::functions::atomize_at_most_one(value, expected)
}

/// The first item of an atomized value, discarding the rest.
///
/// This is the first step the function conversion rules take in XPath 1.0
/// compatibility mode (§3.1.5): "If the expected type calls for a single item or
/// optional single item …, then the value V is effectively replaced by V[1]."
fn first_atomized_item<N: DomNavigator>(
    value: XPathValue<N>,
) -> Result<Option<crate::types::XmlValue>, XPathError> {
    Ok(atomize_sequence(value)?.into_iter().next())
}

/// Apply the XPath 1.0 compatibility-mode operand rules to an arithmetic
/// operand (§3.4).
///
/// "Atomization is applied to the operand. … If the atomized operand is an empty
/// sequence, the result of the arithmetic expression is the `xs:double` value
/// `NaN` … If the atomized operand is a sequence of length greater than one, any
/// items after the first item in the sequence are discarded. If the atomized
/// operand is now an instance of type `xs:boolean`, `xs:string`, `xs:decimal`
/// (including `xs:integer`), `xs:float`, or `xs:untypedAtomic`, then it is
/// converted to the type `xs:double` by applying the `fn:number` function."
///
/// An operand of any other type — a date, a duration, an `xs:double` — is left
/// alone, so the XPath 2.0 operators over those types keep working in
/// compatibility mode.
///
/// `None` means the operand atomized to the empty sequence, which makes the
/// whole arithmetic expression `NaN`.
///
/// `xpath10_language` is the XPath 1.0 language (`XPathMode::XPath10`), whose
/// `number()` reads a string as XPath 1.0 §4.4 does; an XPath 2.0 expression in
/// compatibility mode uses the XPath 2.0 `fn:number`.
fn arithmetic_operand_10<N: DomNavigator>(
    value: XPathValue<N>,
    xpath10_language: bool,
) -> Result<Option<crate::types::XmlValue>, XPathError> {
    let Some(first) = first_atomized_item(value)? else {
        return Ok(None);
    };
    let code = first.type_code;
    let needs_number = code == XmlTypeCode::Boolean
        || code == XmlTypeCode::UntypedAtomic
        || code.is_string_derived()
        || (code.is_numeric() && code != XmlTypeCode::Double);
    if needs_number {
        let number = if xpath10_language {
            crate::xpath::atomize::to_number_xpath10(&first)
        } else {
            crate::xpath::atomize::to_number(&first)
        };
        return Ok(Some(crate::types::XmlValue::double(number)));
    }
    Ok(Some(first))
}

/// Apply the three XPath 1.0 compatibility-mode steps of the function conversion
/// rules to the arguments of a function call (§3.1.5).
///
/// "If XPath 1.0 compatibility mode is `true` **and an argument is not of the
/// expected type**, then the following conversions are applied sequentially to
/// the argument value V: If the expected type calls for a single item or optional
/// single item …, then the value V is effectively replaced by V[1]. If the
/// expected type is `xs:string` or `xs:string?`, then the value V is effectively
/// replaced by `fn:string(V)`. If the expected type is `xs:double` or
/// `xs:double?`, then the value V is effectively replaced by `fn:number(V)`."
///
/// The emphasised condition gates all three steps together, so an argument that
/// already matches its parameter's declared sequence type is passed through
/// untouched — `fn:compare((), '')` keeps its empty sequence, because `()` is a
/// value of type `xs:string?`, and returns the empty sequence rather than `0`.
///
/// The expected types come from the function's signature in the static context;
/// a function whose signature is unavailable, or an argument beyond the declared
/// parameters of a variadic function, is left untouched.
///
/// # What "of the expected type" means here
///
/// The condition is about the argument's **static type**, not about the value
/// the argument happened to produce: the three steps are part of the *static*
/// function conversion rules, applied to the operation tree before evaluation
/// (§2.2.3.1 step SQ6 assigns each expression a static type, and the conversion
/// rules convert "to the declared type of the function parameter").
///
/// §2.2.3.1: "If the Static Typing Feature is not supported, the static types
/// that are assigned are **implementation-dependent**." This engine does not
/// implement that feature, so the choice below is its own, and it is the
/// narrowest one that keeps every observable case right:
///
/// * an argument value that is **not** the empty sequence is judged by
///   [SequenceType matching](SequenceType::matches_sequence) on the value as
///   supplied — before the atomization and casting of the rules that follow, so
///   a node where `xs:string?` is expected is *not* of the expected type (and
///   becomes `fn:string(V)`), and neither is an `xs:untypedAtomic` nor an
///   `xs:integer`;
/// * an argument value that **is** the empty sequence counts as being of the
///   expected type only when the argument *expression* is the literal empty
///   sequence, whose static type is `empty-sequence()`. §2.3.4 singles that
///   expression out — "if the static type assigned to an expression other than
///   `()` or `data(())` is `empty-sequence()`, a static error is raised
///   \[err:XPST0005\]" — so no other expression may be given that static type.
///
/// So `fn:compare((), '')` keeps its empty sequence and returns the empty
/// sequence rather than `0`, while `fn:round(doc/none)` over a path that selects
/// no nodes still converts, and is `NaN`: the static type of a path expression
/// is a node sequence, which is not `xs:double?`, however few nodes it yields.
fn apply_function_conversion_10<N: DomNavigator>(
    arena: &AstArena,
    ctx: &XPathContext<'_>,
    handle: crate::xpath::functions::FunctionHandle,
    arg_exprs: &[AstNodeId],
    args: &mut [XPathValue<N>],
) -> Result<(), XPathError> {
    let Some(signature) = ctx.function_catalog().get_signature(handle) else {
        return Ok(());
    };

    for (idx, arg) in args.iter_mut().enumerate() {
        let Some(expected) = signature.param_types.get(idx) else {
            break;
        };
        let single = matches!(
            expected.cardinality,
            XmlTypeCardinality::One | XmlTypeCardinality::ZeroOrOne
        );
        // None of the three steps can fire for an expected type that is not a
        // single or optional single item: step 1 says so outright, and steps 2
        // and 3 name only `xs:string`/`xs:string?` and `xs:double`/`xs:double?`.
        if !single {
            continue;
        }
        // "… and an argument is not of the expected type".
        let statically_empty = arg_exprs
            .get(idx)
            .is_some_and(|id| is_empty_sequence_expression(arena, *id));
        if (statically_empty || !arg.is_empty()) && value_matches_sequence_type(arg, expected, ctx)
        {
            continue;
        }

        let taken = std::mem::replace(arg, XPathValue::empty());
        *arg = match expected.item_type {
            // `fn:string` of the empty sequence is the zero-length string, and
            // of a single item its string value — for a node, its string value
            // rather than its typed value.
            ItemType::AtomicType(XmlTypeCode::String) => {
                XPathValue::string(string_of_single(first_item(taken))?)
            }
            // `fn:number` of the empty sequence is NaN.
            ItemType::AtomicType(XmlTypeCode::Double) => {
                XPathValue::double(atomize_to_double(first_item(taken))?)
            }
            _ => first_item(taken),
        };
    }

    Ok(())
}

/// Report, once per evaluation of the operator node `site`, that the
/// date/time or duration `result` of `op` keeps its seconds truncated toward
/// zero (`xs-seconds-capacity`; see
/// [`eval_binary_noted`](crate::xpath::operators::eval_binary_noted)).
fn warn_truncated_seconds<N: DomNavigator>(
    ctx: &mut DynamicContext<'_, N>,
    arena: &AstArena,
    site: AstNodeId,
    op: BinaryOpKind,
    result: &crate::types::XmlValue,
) {
    let symbol = match op {
        BinaryOpKind::Add => "+",
        BinaryOpKind::Sub => "-",
        BinaryOpKind::Mul => "*",
        BinaryOpKind::Div => "div",
        _ => "an operator",
    };
    let site = crate::xpath::context::WarningSite::Node {
        expression: std::ptr::from_ref(arena) as usize,
        node: site,
    };
    ctx.warn_once(site, || {
        crate::xpath::operators::seconds_capacity_warning(
            &format!("the operator `{symbol}`"),
            result,
        )
    });
}

/// Whether an expression is a string literal (XPath 2.0 §3.1.1), seen through
/// the wrappers the grammar's step chain adds around a primary expression — a
/// single-step relative `PathExpr` over an unfiltered `FilterExpr`, a
/// parenthesized expression of one operand — exactly as
/// [`is_empty_sequence_expression`] sees through them.
fn is_string_literal_expression(arena: &AstArena, id: AstNodeId) -> bool {
    match arena.try_get(id) {
        Some(AstNode::Value(ValueNode::String(_))) => true,
        Some(AstNode::Expr(expr)) => match expr.items.as_slice() {
            [only] => is_string_literal_expression(arena, *only),
            _ => false,
        },
        Some(AstNode::PathExpr(path)) => match path.steps.as_slice() {
            [only] if !path.is_absolute => is_string_literal_expression(arena, *only),
            _ => false,
        },
        Some(AstNode::FilterExpr(filter)) if filter.predicates.is_empty() => {
            is_string_literal_expression(arena, filter.base)
        }
        _ => false,
    }
}

/// Whether an expression is the literal empty sequence — the one expression
/// XPath 2.0 §2.3.4 allows to have the static type `empty-sequence()`.
///
/// `()` parses to [`ValueNode::Empty`], and a sequence-construction expression
/// with no operands is the same expression. Either may be wrapped by the
/// grammar's step chain: a primary expression used as a function argument
/// arrives as a single-step relative `PathExpr` over an unfiltered
/// `FilterExpr`, and those wrappers add nothing to the expression's type, so the
/// scan looks through them. It does **not** look through anything that can
/// select nodes (an axis step, an absolute path, a predicate), because the
/// static type of such an expression is a node sequence whatever it yields.
fn is_empty_sequence_expression(arena: &AstArena, id: AstNodeId) -> bool {
    match arena.try_get(id) {
        Some(AstNode::Value(ValueNode::Empty)) => true,
        Some(AstNode::Expr(expr)) => match expr.items.as_slice() {
            [] => true,
            [only] => is_empty_sequence_expression(arena, *only),
            _ => false,
        },
        // `(…)` as a step: one relative step and no `/`, so the path contributes
        // nothing of its own.
        Some(AstNode::PathExpr(path)) => match path.steps.as_slice() {
            [only] if !path.is_absolute => is_empty_sequence_expression(arena, *only),
            _ => false,
        },
        // A primary expression with no predicates; a predicate makes it a
        // filtered node sequence.
        Some(AstNode::FilterExpr(filter)) if filter.predicates.is_empty() => {
            is_empty_sequence_expression(arena, filter.base)
        }
        _ => false,
    }
}

/// SequenceType matching for a whole [`XPathValue`], without materialising it:
/// [`XPathValue::as_slice`] borrows every shape, the single item included.
fn value_matches_sequence_type<N: DomNavigator>(
    value: &XPathValue<N>,
    expected: &SequenceType,
    ctx: &XPathContext<'_>,
) -> bool {
    expected.cardinality.matches_count(value.len())
        && value
            .as_slice()
            .iter()
            .all(|item| expected.item_type.matches_item(item, ctx))
}

/// `V[1]`: the first item of a value, or the empty sequence.
fn first_item<N: DomNavigator>(value: XPathValue<N>) -> XPathValue<N> {
    match value {
        XPathValue::Empty => XPathValue::empty(),
        item @ XPathValue::Item(_) => item,
        XPathValue::Sequence(items) => match items.into_iter().next() {
            Some(first) => XPathValue::from_item(first),
            None => XPathValue::empty(),
        },
    }
}

/// Applies the function conversion rules to an operand of the `to` operator.
fn to_integer_operand(value: crate::types::XmlValue) -> Result<crate::types::XmlValue, XPathError> {
    if value.type_code == XmlTypeCode::UntypedAtomic {
        return crate::xpath::cast::cast_to(&value, XmlTypeCode::Integer);
    }
    Ok(value)
}

// ============================================================================
// Path Expression Evaluation
// ============================================================================

/// The node a leading `/` or `//` starts from.
///
/// XPath 2.0 §3.2: "A "/" at the beginning of a path expression is an
/// abbreviation for the initial step (fn:root(self::node()) treat as
/// document-node())/ … At evaluation time, if the root node above the context
/// node is not a document node, a dynamic error is raised \[err:XPDY0050\]" —
/// and the same for "//". A navigator whose tree is rooted at a parentless
/// element (or any other node that is not a document node) therefore has no
/// `/`. An assertion navigator keeps answering it: its root is the document
/// node, whose children it hides, so `//x` there is the empty sequence.
fn document_root<N: DomNavigator>(context_node: &N) -> Result<N, XPathError> {
    let root = get_root(context_node);
    if root.node_type() != DomNodeType::Root {
        return Err(XPathError::root_not_a_document());
    }
    Ok(root)
}

/// Evaluate a path expression.
///
/// Implements XPath 2.0 path expression semantics:
/// - Root-only path (`/`): Returns the document root
/// - Absolute paths (`/a/b`): Start from document root
/// - Relative paths (`a/b`): Start from context node
/// - Paths are evaluated left-to-right, chaining steps
fn eval_path_expr<N: DomNavigator>(
    arena: &AstArena,
    path_expr: &PathExprNode,
    ctx: &mut DynamicContext<'_, N>,
) -> Result<XPathValue<N>, XPathError> {
    // Handle root-only path: "/"
    if path_expr.is_absolute && path_expr.steps.is_empty() {
        let root = document_root(ctx.require_context_node()?)?;
        return Ok(XPathValue::from_node(root));
    }

    // Check if the first step is a "primary expression" that doesn't require context nodes.
    // This includes function calls, literals, parenthesized expressions, variable references,
    // type expressions (constructor functions like xs:int(...) that the binder
    // converts from FunctionCall to TypeExpr/CastAs in-place), and ContextItem.
    // ContextItem (`.`) accesses the dynamic context item directly (which may be atomic),
    // so it must not go through require_context_node() which demands a node.
    // Only PathStep (axis steps) truly require a node context.
    let first_is_primary = path_expr.steps.first().is_some_and(|&step_id| {
        matches!(
            arena.get(step_id),
            AstNode::FilterExpr(_)
                | AstNode::FunctionCall(_)
                | AstNode::Value(_)
                | AstNode::Expr(_)
                | AstNode::VarRef(_)
                | AstNode::TypeExpr(_)
                | AstNode::ContextItem(_)
        )
    });

    // Determine the starting nodes based on path type
    let starting_nodes: Vec<N> = if path_expr.is_absolute {
        // Absolute path: start from document root
        vec![document_root(ctx.require_context_node()?)?]
    } else if first_is_primary {
        // First step is a primary expression - no initial context nodes needed
        Vec::new()
    } else {
        // Relative path: start from context node
        let context_node = ctx.require_context_node()?;
        vec![context_node.clone()]
    };

    // Process steps sequentially. Every `/` is the XPath 2.0 §3.2 operation
    // `E1/E2`, so each step is evaluated once per item of the sequence that
    // reaches it and the per-item results are combined.
    let mut current_nodes: Vec<XmlItem<N>> =
        starting_nodes.into_iter().map(XmlItem::Node).collect();
    let mut shape = SequenceShape::SINGLE_NODE;
    let step_count = path_expr.steps.len();

    for (step_idx, &step_id) in path_expr.steps.iter().enumerate() {
        // §3.2: the combined result of every `E1/E2` is "returned in document
        // order" with "duplicate nodes ... eliminated". That normalization is
        // only *observable* at the end of the path or by a step that can see
        // the order, the size or the duplicates of its input. An axis step
        // cannot: it evaluates its axis and its predicates once per input node,
        // and its predicates take their focus from the step's own result, so
        // reordering or de-duplicating its input cannot change its result set.
        // Deferring the sort to the last step that *can* observe it keeps a
        // plain path such as `//a/b/c` at a single sort, as before.
        let normalize = step_idx + 1 == step_count
            || !matches!(
                arena.get(path_expr.steps[step_idx + 1]),
                AstNode::PathStep(_)
            );

        current_nodes = match arena.get(step_id) {
            AstNode::PathStep(path_step) => {
                let axis = path_step.axis;
                // A forward axis over an input that is already in document
                // order without duplicates needs no sort at all.
                let already_ordered = axis_step_keeps_document_order(axis, shape);
                let items = eval_axis_step(arena, path_step, current_nodes, ctx)?;
                let sorted = if normalize && !already_ordered {
                    sort_document_order(items)?
                } else {
                    items
                };
                shape = axis_step_result_shape(axis, shape, already_ordered || normalize);
                sorted
            }
            _ if step_idx == 0 && current_nodes.is_empty() => {
                // The path starts with a primary expression and there is no
                // `/` to its left, so it is evaluated once with the outer
                // focus.
                shape = SequenceShape::UNKNOWN;
                eval_node(arena, step_id, ctx)?.into_vec()
            }
            _ => {
                // `E1/E2` with an arbitrary E2: the inner focus is one item of
                // E1, at its position in E1, with E1's size (§2.1.2).
                let items = eval_expr_step(arena, step_id, current_nodes, ctx)?;
                shape = SequenceShape::UNKNOWN;
                combine_expr_step_results(items, normalize)?
            }
        };

        // Early exit if sequence becomes empty
        if current_nodes.is_empty() {
            return Ok(XPathValue::empty());
        }
    }

    Ok(XPathValue::from_sequence(current_nodes))
}

/// What is statically known about the node sequence feeding a path step.
///
/// Used only to decide whether the §3.2 "document order, duplicates removed"
/// normalization of a step's result can be skipped; every field is a
/// conservative "known to be true", never a guess.
#[derive(Debug, Clone, Copy)]
struct SequenceShape {
    /// Known to be in document order with no duplicates.
    ordered: bool,
    /// Known to contain no node that is an ancestor of another node in it.
    peers: bool,
    /// Known to contain at most one node.
    single: bool,
}

impl SequenceShape {
    /// The start of an absolute or relative path: exactly one node.
    const SINGLE_NODE: Self = Self {
        ordered: true,
        peers: true,
        single: true,
    };

    /// Nothing is known (the result of an arbitrary expression).
    const UNKNOWN: Self = Self {
        ordered: false,
        peers: false,
        single: false,
    };
}

/// True when concatenating an axis step's per-node results is guaranteed to
/// yield document order without duplicates, so the §3.2 normalization of that
/// step's result would be a no-op.
fn axis_step_keeps_document_order(axis: Axis, input: SequenceShape) -> bool {
    match axis {
        // A reverse axis delivers reverse document order, which always has to
        // be turned around — except `parent`, which yields at most one node
        // per origin, so a single origin cannot be out of order.
        Axis::Parent => input.single,
        Axis::Ancestor | Axis::AncestorOrSelf | Axis::Preceding | Axis::PrecedingSibling => false,
        // `self` reproduces its input.
        Axis::SelfAxis => input.ordered,
        // These forward axes visit disjoint, document-ordered ranges when the
        // origins are document-ordered peers; a single origin trivially is one.
        Axis::Child
        | Axis::Attribute
        | Axis::Namespace
        | Axis::Descendant
        | Axis::DescendantOrSelf => input.single || (input.ordered && input.peers),
        // Two peers can share a following sibling (or a following node), so
        // only a single origin is duplicate-free here.
        Axis::FollowingSibling | Axis::Following => input.single,
    }
}

/// The shape of an axis step's result, given its input's shape.
///
/// `normalized` says whether the result is known to be in document order
/// without duplicates (either because the axis kept that property or because
/// the step's result was sorted).
fn axis_step_result_shape(axis: Axis, input: SequenceShape, normalized: bool) -> SequenceShape {
    // "peers" is a property of the node *set*, independent of its order.
    let peers = match axis {
        // Attributes and namespace nodes have no descendants.
        Axis::Attribute | Axis::Namespace => true,
        // A child of one node is never an ancestor of a child of a peer.
        Axis::SelfAxis | Axis::Child => input.peers,
        // Siblings of a single node are peers; so is its (single) parent.
        Axis::FollowingSibling | Axis::PrecedingSibling | Axis::Parent => input.single,
        _ => false,
    };
    let single = match axis {
        Axis::SelfAxis | Axis::Parent => input.single,
        _ => false,
    };
    SequenceShape {
        ordered: normalized,
        peers,
        single,
    }
}

/// Sort a node sequence into document order and drop duplicates (§3.2).
///
/// A sequence of fewer than two items is returned untouched.
fn sort_document_order<N: DomNavigator>(
    items: Vec<XmlItem<N>>,
) -> Result<Vec<XmlItem<N>>, XPathError> {
    if items.len() < 2 {
        return Ok(items);
    }
    let iter = VecNodeIterator::new(items);
    let mut doc_order_iter = DocumentOrderNodeIterator::new(iter)?;
    collect_iterator(&mut doc_order_iter)
}

/// Combine the per-item results of an `E1/E2` whose E2 is not an axis step.
///
/// XPath 2.0 §3.2: node results are combined in document order with duplicates
/// removed, atomic results are concatenated in order, and a result holding both
/// a node and an atomic value is a type error [err:XPTY0018].
fn combine_expr_step_results<N: DomNavigator>(
    items: Vec<XmlItem<N>>,
    normalize: bool,
) -> Result<Vec<XmlItem<N>>, XPathError> {
    let mut has_node = false;
    let mut has_atomic = false;
    for item in &items {
        match item {
            XmlItem::Node(_) => has_node = true,
            XmlItem::Atomic(_) => has_atomic = true,
        }
    }
    if has_node && has_atomic {
        return Err(XPathError::XPTY0018);
    }
    if has_node && normalize {
        return sort_document_order(items);
    }
    Ok(items)
}

/// Evaluate an axis step against the sequence that reaches it.
///
/// XPath 2.0 §3.2.2: the step's predicates are applied to the sequence the
/// step produces **for one context node**, so their focus size is that
/// sequence's length and not the length of the concatenation over all context
/// nodes. The axis itself cannot observe a focus, so it is still evaluated in a
/// single pass; the pass records where each context node's own run starts.
fn eval_axis_step<N: DomNavigator>(
    arena: &AstArena,
    step: &PathStepNode,
    input_nodes: Vec<XmlItem<N>>,
    ctx: &mut DynamicContext<'_, N>,
) -> Result<Vec<XmlItem<N>>, XPathError> {
    // Convert input to nodes only (XPTY0019 if atomic values present)
    let nodes: Vec<N> = input_nodes
        .into_iter()
        .map(|item| match item {
            XmlItem::Node(n) => Ok(n),
            XmlItem::Atomic(_) => Err(XPathError::XPTY0019),
        })
        .collect::<Result<Vec<_>, _>>()?;

    if nodes.is_empty() {
        return Ok(Vec::new());
    }

    // Convert the step's node test to runtime NodeTest
    let node_test = step_to_node_test(step, ctx.static_context)?;

    // Create the base iterator from input nodes
    let base_iter = VecNodeIterator::new(nodes.into_iter().map(XmlItem::Node).collect());

    // Apply axis iterator
    let xpath_ctx = ctx.static_context.clone();
    let want_runs = !step.predicates.is_empty();
    // The runtime node test carries the whole kind test — for `element(N, T)`
    // and `attribute(N, T)` the TypeName and the nilled clause too (XPath 2.0
    // §2.5.4.3, §2.5.4.5) — so the predicates below see only the nodes the
    // whole test selects.
    let stepped = apply_axis_iterator(step.axis, node_test, xpath_ctx, base_iter, want_runs)?;

    // Apply predicates if any
    if step.predicates.is_empty() {
        return Ok(stepped.items);
    }

    let mut result = Vec::with_capacity(stepped.items.len());
    for (run, &start) in stepped.run_starts.iter().enumerate() {
        let end = stepped
            .run_starts
            .get(run + 1)
            .copied()
            .unwrap_or(stepped.items.len());
        let run_items = stepped.items[start..end].to_vec();
        result.extend(eval_predicates(arena, &step.predicates, ctx, run_items)?);
    }
    Ok(result)
}

/// Evaluate `E1/E2` where E2 is not an axis step, giving every item of E1 an
/// inner focus (§2.1.2: context item, its position in E1, and E1's size).
fn eval_expr_step<N: DomNavigator>(
    arena: &AstArena,
    step_id: AstNodeId,
    input_nodes: Vec<XmlItem<N>>,
    ctx: &mut DynamicContext<'_, N>,
) -> Result<Vec<XmlItem<N>>, XPathError> {
    // §3.2: "Expression E1 is evaluated, and if the result is not a (possibly
    // empty) sequence of nodes, a type error is raised [err:XPTY0019]."
    if input_nodes
        .iter()
        .any(|item| matches!(item, XmlItem::Atomic(_)))
    {
        return Err(XPathError::XPTY0019);
    }

    let size = input_nodes.len();
    let saved_item = ctx.context_item.take();
    let saved_pos = ctx.context_position;
    let saved_size = ctx.context_size;

    let mut results: Vec<XmlItem<N>> = Vec::new();
    let mut outcome = Ok(());
    for (idx, item) in input_nodes.into_iter().enumerate() {
        ctx.context_item = Some(item);
        ctx.context_position = idx + 1;
        ctx.context_size = size;

        match eval_node(arena, step_id, ctx) {
            Ok(value) => results.extend(value.into_vec()),
            Err(e) => {
                outcome = Err(e);
                break;
            }
        }
    }

    ctx.context_item = saved_item;
    ctx.context_position = saved_pos;
    ctx.context_size = saved_size;

    outcome.map(|()| results)
}

/// Convert a PathStepNode to a runtime NodeTest.
///
/// Fails only for a kind test whose names a bound expression cannot fail to
/// resolve (see `node_test::item_type_of_kind_test`).
fn step_to_node_test(
    step: &PathStepNode,
    ctx: &XPathContext<'_>,
) -> Result<Option<NodeTest>, XPathError> {
    // XPath 2.0 §3.2.1.2: a name test only ever selects nodes of the axis's
    // principal node kind, and an unprefixed QName picks up the default
    // element/type namespace only on an axis whose principal node kind is
    // element.
    let principal = principal_node_kind(step.axis);

    // If we have a resolved_test from binding, use it
    if let Some(ref resolved) = step.resolved_test {
        return Ok(Some(name_test_for_principal_kind(
            resolved.clone(),
            principal,
        )));
    }

    // Otherwise, convert from AST node test
    match &step.test {
        AstNodeTest::Name(name_test) => {
            // Convert AST NameTest to runtime NameTest
            let resolved = match (&name_test.prefix, &name_test.local_name) {
                (None, None) => {
                    // * - wildcard
                    RuntimeNameTest::Wildcard
                }
                (None, Some(local)) => {
                    // *:local - namespace wildcard
                    RuntimeNameTest::NamespaceWildcard(ctx.names.add(local))
                }
                (Some(prefix), None) => {
                    // prefix:* - local wildcard
                    let Some(ns_uri) = ctx.resolve_prefix(prefix) else {
                        // Unknown prefix
                        return Ok(None);
                    };
                    RuntimeNameTest::LocalWildcard(ctx.names.add(&ns_uri))
                }
                (Some(prefix), Some(local)) => {
                    // prefix:local - specific QName
                    let local_id = ctx.names.add(local);
                    let ns_uri = resolve_step_name_ns(prefix, ctx, principal);
                    let qname = crate::namespace::qname::QualifiedName::new(ns_uri, local_id, None);
                    RuntimeNameTest::QName(qname)
                }
            };
            Ok(Some(name_test_for_principal_kind(resolved, principal)))
        }
        AstNodeTest::Kind(kind_test) => Ok(Some(NodeTest::Type(SequenceType::one(
            crate::xpath::node_test::item_type_of_kind_test(kind_test, ctx)?,
        )))),
    }
}

/// Namespace URI of a QName written in a path step, given the principal node
/// kind of the step's axis.
///
/// XPath 2.0 §3.2.1.2: "An unprefixed QName, when used as a name test on an
/// axis whose principal node kind is element, has the namespace URI of the
/// default element/type namespace in the expression context; otherwise, it
/// has no namespace URI."
fn resolve_step_name_ns(
    prefix: &str,
    ctx: &XPathContext<'_>,
    principal: DomNodeType,
) -> Option<NameId> {
    if prefix.is_empty() {
        if principal == DomNodeType::Element {
            ctx.default_element_ns
        } else {
            None
        }
    } else {
        ctx.resolve_prefix(prefix).map(|s| ctx.names.add(&s))
    }
}

/// The raw items an axis step produced, with the boundaries of the runs the
/// individual input nodes contributed.
struct AxisStepItems<N: DomNavigator> {
    /// Every item the axis yielded, in input-node order then axis order.
    items: Vec<XmlItem<N>>,
    /// Offsets in `items` at which each input node's own run starts; empty when
    /// the caller did not ask for the runs. An input node that yielded nothing
    /// contributes no run.
    run_starts: Vec<usize>,
}

/// Apply an axis iterator to a base iterator.
///
/// When `want_runs` is set, the result records where each input node's own run
/// of results starts, which is what a step's predicates need as their input
/// sequence (§3.2.2).
fn apply_axis_iterator<N: DomNavigator>(
    axis: Axis,
    node_test: Option<NodeTest>,
    ctx: XPathContext<'_>,
    base_iter: VecNodeIterator<N>,
    want_runs: bool,
) -> Result<AxisStepItems<N>, XPathError> {
    match axis {
        Axis::Child => {
            let mut iter =
                SequentialAxisNodeIterator::new(ctx, node_test, false, base_iter, ChildAxis);
            collect_axis_iterator(&mut iter, want_runs)
        }
        Axis::Descendant => {
            let mut iter = DescendantNodeIterator::new(ctx, node_test, false, base_iter);
            collect_axis_iterator(&mut iter, want_runs)
        }
        Axis::DescendantOrSelf => {
            let mut iter = DescendantNodeIterator::new(ctx, node_test, true, base_iter);
            collect_axis_iterator(&mut iter, want_runs)
        }
        Axis::Attribute => {
            let mut iter =
                SequentialAxisNodeIterator::new(ctx, node_test, false, base_iter, AttributeAxis);
            collect_axis_iterator(&mut iter, want_runs)
        }
        Axis::SelfAxis => {
            // SelfAxis returns current node via move_to_first, so match_self=false
            let mut iter =
                SequentialAxisNodeIterator::new(ctx, node_test, false, base_iter, SelfAxis);
            collect_axis_iterator(&mut iter, want_runs)
        }
        Axis::Parent => {
            let mut iter =
                SequentialAxisNodeIterator::new(ctx, node_test, false, base_iter, ParentAxis);
            collect_axis_iterator(&mut iter, want_runs)
        }
        Axis::Ancestor => {
            let mut iter =
                SequentialAxisNodeIterator::new(ctx, node_test, false, base_iter, AncestorAxis);
            collect_axis_iterator(&mut iter, want_runs)
        }
        Axis::AncestorOrSelf => {
            let mut iter =
                SequentialAxisNodeIterator::new(ctx, node_test, true, base_iter, AncestorAxis);
            collect_axis_iterator(&mut iter, want_runs)
        }
        Axis::FollowingSibling => {
            let mut iter = SequentialAxisNodeIterator::new(
                ctx,
                node_test,
                false,
                base_iter,
                FollowingSiblingAxis,
            );
            collect_axis_iterator(&mut iter, want_runs)
        }
        Axis::PrecedingSibling => {
            let mut iter = SequentialAxisNodeIterator::new(
                ctx,
                node_test,
                false,
                base_iter,
                PrecedingSiblingAxis,
            );
            collect_axis_iterator(&mut iter, want_runs)
        }
        Axis::Following => {
            let mut iter = FollowingNodeIterator::new(ctx, node_test, base_iter);
            collect_axis_iterator(&mut iter, want_runs)
        }
        Axis::Preceding => {
            let mut iter = PrecedingNodeIterator::new(ctx, node_test, base_iter);
            collect_axis_iterator(&mut iter, want_runs)
        }
        Axis::Namespace => {
            let mut iter = SequentialAxisNodeIterator::new(
                ctx,
                node_test,
                false,
                base_iter,
                NamespaceAxis::default(),
            );
            collect_axis_iterator(&mut iter, want_runs)
        }
    }
}

/// Collect iterator results into a Vec.
fn collect_iterator<I: XmlNodeIterator>(
    iter: &mut I,
) -> Result<Vec<XmlItem<I::Navigator>>, XPathError> {
    let mut results = Vec::new();
    while iter.move_next()? {
        if let Some(item_ref) = iter.current() {
            let item = match item_ref {
                crate::xpath::iterator::XmlItemRef::Node(n) => XmlItem::Node(n.clone()),
                crate::xpath::iterator::XmlItemRef::Atomic(v) => XmlItem::Atomic(v.clone()),
            };
            results.push(item);
        }
    }
    Ok(results)
}

/// Collect an axis iterator, optionally splitting the output into the runs the
/// individual input nodes produced.
///
/// Every axis iterator in [`crate::xpath::axis_iterators`] restarts
/// `sequential_position()` at 1 when it moves on to the next input node, which
/// is what delimits the runs.
fn collect_axis_iterator<I: XmlNodeIterator>(
    iter: &mut I,
    want_runs: bool,
) -> Result<AxisStepItems<I::Navigator>, XPathError> {
    if !want_runs {
        return Ok(AxisStepItems {
            items: collect_iterator(iter)?,
            run_starts: Vec::new(),
        });
    }

    let mut results = Vec::new();
    let mut run_starts = Vec::new();
    let mut prev_pos = 0usize;
    while iter.move_next()? {
        if let Some(item_ref) = iter.current() {
            let pos = iter.sequential_position();
            debug_assert!(
                pos.is_some(),
                "an axis iterator must report its per-input-node position"
            );
            // An iterator that does not report a position keeps extending the
            // current run, which is the pre-§3.2.2 behaviour rather than a
            // wrong split.
            let pos = pos.unwrap_or(prev_pos + 1);
            if run_starts.is_empty() || pos <= prev_pos {
                run_starts.push(results.len());
            }
            prev_pos = pos;
            let item = match item_ref {
                crate::xpath::iterator::XmlItemRef::Node(n) => XmlItem::Node(n.clone()),
                crate::xpath::iterator::XmlItemRef::Atomic(v) => XmlItem::Atomic(v.clone()),
            };
            results.push(item);
        }
    }
    Ok(AxisStepItems {
        items: results,
        run_starts,
    })
}

// ============================================================================
// Filter Expression Evaluation
// ============================================================================

/// Evaluate a filter expression (`expr[predicate][predicate]...`).
fn eval_filter_expr<N: DomNavigator>(
    arena: &AstArena,
    filter_expr: &FilterExprNode,
    ctx: &mut DynamicContext<'_, N>,
) -> Result<XPathValue<N>, XPathError> {
    // Evaluate the base expression
    let base_value = eval_node(arena, filter_expr.base, ctx)?;

    // If no predicates, return base value directly
    if filter_expr.predicates.is_empty() {
        return Ok(base_value);
    }

    // Apply predicates
    let items = base_value.into_vec();
    let filtered = eval_predicates(arena, &filter_expr.predicates, ctx, items)?;
    Ok(XPathValue::from_sequence(filtered))
}

/// Evaluate predicates on a sequence of items.
///
/// Each predicate is evaluated in order. For each predicate:
/// - If the predicate evaluates to a number, select the item at that position
/// - Otherwise, use effective boolean value to filter
fn eval_predicates<N: DomNavigator>(
    arena: &AstArena,
    predicates: &[AstNodeId],
    ctx: &mut DynamicContext<'_, N>,
    mut items: Vec<XmlItem<N>>,
) -> Result<Vec<XmlItem<N>>, XPathError> {
    for &pred_id in predicates {
        if items.is_empty() {
            break;
        }

        let size = items.len();
        let mut filtered = Vec::new();

        for (idx, item) in items.into_iter().enumerate() {
            let position = idx + 1; // 1-based position

            // Save current context
            let saved_item = ctx.context_item.take();
            let saved_pos = ctx.context_position;
            let saved_size = ctx.context_size;

            // Set predicate context
            ctx.context_item = Some(item.clone());
            ctx.context_position = position;
            ctx.context_size = size;

            // Evaluate predicate
            let pred_result = eval_node(arena, pred_id, ctx)?;

            // Restore context
            ctx.context_item = saved_item;
            ctx.context_position = saved_pos;
            ctx.context_size = saved_size;

            // Check if item should be included
            let is_10 = ebv_is_xpath10(ctx.static_context);
            let include = match &pred_result {
                XPathValue::Item(XmlItem::Atomic(value)) if value.type_code.is_numeric() => {
                    // XPath 1.0 §2.4 and 2.0: exact comparison, no rounding
                    let num = crate::xpath::atomize::to_number(value);
                    if num.is_nan() {
                        false
                    } else {
                        (position as f64) == num
                    }
                }
                _ => {
                    if is_10 {
                        effective_boolean_value_10(&pred_result)?
                    } else {
                        effective_boolean_value(&pred_result)?
                    }
                }
            };

            if include {
                filtered.push(item);
            }
        }

        items = filtered;
    }

    Ok(items)
}

use std::ops::ControlFlow;

// ============================================================================
// For Expression Evaluation
// ============================================================================

/// Evaluate a for expression (`for $x in X, $y in Y return expr`).
///
/// Semantics per XPath 2.0 spec:
/// - Evaluate each binding's `in_expr` to produce a sequence
/// - Iterate through all combinations (Cartesian product for multiple bindings)
/// - For each combination, bind variables and evaluate `return_expr`
/// - Concatenate all results into a single sequence
fn eval_for_expression<N: DomNavigator>(
    arena: &AstArena,
    for_node: &ForNode,
    ctx: &mut DynamicContext<'_, N>,
) -> Result<XPathValue<N>, XPathError> {
    // Collect results from iterating over Cartesian product
    let mut results: Vec<XmlItem<N>> = Vec::new();

    // For the for expression, we use XPathError as the break type since we only
    // break on errors (never short-circuit for other reasons)
    match eval_for_bindings(
        arena,
        &for_node.bindings,
        0,
        for_node.return_expr,
        ctx,
        &mut |result| match result {
            Ok(value) => {
                results.extend(value.into_vec());
                ControlFlow::Continue(())
            }
            Err(e) => ControlFlow::Break(e),
        },
    ) {
        ControlFlow::Continue(()) => {}
        ControlFlow::Break(e) => return Err(e),
    }

    Ok(XPathValue::from_sequence(results))
}

/// Recursively iterate over Cartesian product of bindings with lazy evaluation.
///
/// This helper handles the recursive iteration for for/quantified expressions.
/// For each binding, it evaluates the `in_expr` (allowing dependent bindings),
/// iterates over its sequence, and recursively processes remaining bindings.
/// When all bindings are processed, it evaluates the body.
///
/// IMPORTANT: Each binding's `in_expr` is evaluated lazily, AFTER all previous
/// binding variables have been bound. This allows dependent bindings like:
/// `for $x in 1 to 3, $y in $x+1 return $y`
///
/// The function is generic over the break type `B`, allowing callers to use
/// different types for different control flow needs:
/// - For expressions use `B = XPathError` (only break on errors)
/// - Quantified expressions use `B = QuantifiedExit` (break on short-circuit or error)
fn eval_for_bindings<N: DomNavigator, B>(
    arena: &AstArena,
    bindings: &[ForBinding],
    binding_index: usize,
    body_id: AstNodeId,
    ctx: &mut DynamicContext<'_, N>,
    collector: &mut impl FnMut(Result<XPathValue<N>, XPathError>) -> ControlFlow<B>,
) -> ControlFlow<B> {
    if binding_index >= bindings.len() {
        // All bindings processed, evaluate the body
        let result = eval_node(arena, body_id, ctx);
        return collector(result);
    }

    let binding = &bindings[binding_index];
    let slot = match binding.slot {
        Some(s) => s,
        None => {
            return collector(Err(XPathError::Internal(
                "For binding slot not assigned".to_string(),
            )))
        }
    };

    // LAZY EVALUATION: Evaluate in_expr NOW (previous variables are already bound)
    let seq_value = match eval_node(arena, binding.in_expr, ctx) {
        Ok(v) => v,
        Err(e) => return collector(Err(e)),
    };
    let items = seq_value.into_vec();

    // If binding sequence is empty, we simply don't iterate (produces empty result)
    if items.is_empty() {
        return ControlFlow::Continue(());
    }

    // Iterate over each item in the current binding's sequence
    for item in items {
        // Set the variable for this binding
        ctx.set_variable(slot, XPathValue::from_item(item));

        // Recursively process remaining bindings
        if let cf @ ControlFlow::Break(_) =
            eval_for_bindings(arena, bindings, binding_index + 1, body_id, ctx, collector)
        {
            return cf;
        }
    }

    ControlFlow::Continue(())
}

// ============================================================================
// Quantified Expression Evaluation
// ============================================================================

/// Exit type for quantified expression short-circuit evaluation.
///
/// This enum cleanly distinguishes between a legitimate short-circuit exit
/// (when the quantified expression's answer is determined) and an actual error.
enum QuantifiedExit {
    /// Short-circuit: the quantified expression's answer is determined.
    ShortCircuit,
    /// A real error occurred during evaluation.
    Error(XPathError),
}

/// Evaluate a quantified expression (`some/every $x in X satisfies expr`).
///
/// Semantics per XPath 2.0 spec:
/// - `some`: Returns true if at least one combination satisfies the expression
/// - `every`: Returns true if all combinations satisfy (including empty - vacuous truth)
/// - Short-circuit evaluation when result is determined
///
/// NOTE: Bindings are evaluated lazily, allowing dependent bindings like:
/// `some $x in (1,2), $y in ($x*2) satisfies $y > 3`
fn eval_quantified_expression<N: DomNavigator>(
    arena: &AstArena,
    quant_node: &QuantifiedNode,
    ctx: &mut DynamicContext<'_, N>,
) -> Result<XPathValue<N>, XPathError> {
    // Track whether we had any iterations (for vacuous truth handling)
    let mut had_any_iteration = false;
    let mut found_some = false;
    let mut all_satisfied = true;

    // Use QuantifiedExit as the break type to cleanly distinguish between
    // short-circuit (answer found) and real errors
    match eval_for_bindings(
        arena,
        &quant_node.bindings,
        0,
        quant_node.satisfies,
        ctx,
        &mut |result| {
            had_any_iteration = true;
            match result {
                Ok(value) => match effective_boolean_value(&value) {
                    Ok(satisfied) => {
                        match quant_node.kind {
                            QuantifierKind::Some => {
                                if satisfied {
                                    found_some = true;
                                    return ControlFlow::Break(QuantifiedExit::ShortCircuit);
                                }
                            }
                            QuantifierKind::Every => {
                                if !satisfied {
                                    all_satisfied = false;
                                    return ControlFlow::Break(QuantifiedExit::ShortCircuit);
                                }
                            }
                        }
                        ControlFlow::Continue(())
                    }
                    Err(e) => ControlFlow::Break(QuantifiedExit::Error(e)),
                },
                Err(e) => ControlFlow::Break(QuantifiedExit::Error(e)),
            }
        },
    ) {
        ControlFlow::Continue(()) => {} // Completed all iterations
        ControlFlow::Break(QuantifiedExit::ShortCircuit) => {} // Found answer early
        ControlFlow::Break(QuantifiedExit::Error(e)) => return Err(e),
    }

    // Handle vacuous truth: if no iterations occurred (empty binding),
    // "every" is vacuously true, "some" is false
    if !had_any_iteration {
        return match quant_node.kind {
            QuantifierKind::Some => Ok(XPathValue::boolean(false)),
            QuantifierKind::Every => Ok(XPathValue::boolean(true)),
        };
    }

    match quant_node.kind {
        QuantifierKind::Some => Ok(XPathValue::boolean(found_some)),
        QuantifierKind::Every => Ok(XPathValue::boolean(all_satisfied)),
    }
}

/// Evaluate a ValueNode to an XPathValue.
fn eval_value<N: DomNavigator>(
    value: &ValueNode,
    mode: XPathMode,
) -> Result<XPathValue<N>, XPathError> {
    match value {
        ValueNode::Empty => Ok(XPathValue::empty()),

        ValueNode::String(s) => Ok(XPathValue::string(s.clone())),

        ValueNode::Boolean(b) => Ok(XPathValue::boolean(*b)),

        ValueNode::Integer(s) => {
            // Parse integer string to BigInt
            let i: num_bigint::BigInt = s.parse().map_err(|_| XPathError::FORG0001 {
                value: s.clone(),
                target_type: "xs:integer".to_string(),
            })?;
            Ok(XPathValue::integer(i))
        }

        ValueNode::Decimal(s) => {
            if mode == XPathMode::XPath10 {
                // XPath 1.0: all numbers are doubles
                let d: f64 = s.parse().unwrap_or(f64::NAN);
                Ok(XPathValue::double(d))
            } else {
                // XPath 2.0 §3.1.1: the value of a decimal literal is the
                // literal cast to xs:decimal — never a rounded neighbour.
                let d = crate::xpath::cast::decimal_from_lexical(s)?;
                Ok(XPathValue::decimal(d))
            }
        }

        ValueNode::Double(s) => {
            let d: f64 = s.parse().unwrap_or(f64::NAN);
            Ok(XPathValue::double(d))
        }

        ValueNode::Typed(xml_value) => Ok(XPathValue::from_atomic(xml_value.clone())),
    }
}

/// Extract a single node from an XPathValue for node comparison operators.
/// Returns Ok(None) for empty sequence, Ok(Some(node)) for single node,
/// or Err for type errors (non-node or multiple items).
fn extract_single_node<N: DomNavigator>(value: XPathValue<N>) -> Result<Option<N>, XPathError> {
    match value {
        XPathValue::Empty => Ok(None),
        XPathValue::Item(XmlItem::Node(node)) => Ok(Some(node)),
        XPathValue::Item(XmlItem::Atomic(_)) => Err(XPathError::XPTY0004 {
            expected: "node()".to_string(),
            found: "atomic value".to_string(),
        }),
        XPathValue::Sequence(items) => {
            if items.len() == 1 {
                match items.into_iter().next().unwrap() {
                    XmlItem::Node(node) => Ok(Some(node)),
                    XmlItem::Atomic(_) => Err(XPathError::XPTY0004 {
                        expected: "node()".to_string(),
                        found: "atomic value".to_string(),
                    }),
                }
            } else if items.is_empty() {
                Ok(None)
            } else {
                // XPath 2.0 §3.5.3: "Each operand must be either a single node
                // or an empty sequence; otherwise a type error is raised
                // [err:XPTY0004]."
                Err(crate::xpath::functions::too_many_items("node()?"))
            }
        }
    }
}

/// Check if an XPathValue is a single boolean value (XPath 1.0 §3.4 helper).
fn is_boolean_value<N: DomNavigator>(val: &XPathValue<N>) -> bool {
    matches!(
        val.as_slice(),
        [XmlItem::Atomic(v)] if v.type_code == crate::types::XmlTypeCode::Boolean
    )
}

/// Whether the effective boolean value is XPath 1.0's — only in the XPath 1.0
/// *language* mode, [`XPathMode::XPath10`], whose grammar cannot build a
/// sequence of two or more atomic values, so its `boolean()` and XPath 2.0's
/// agree on everything it can express.
///
/// XPath 1.0 *compatibility mode* in an XPath 2.0 expression does not change
/// the effective boolean value: XPath 2.0 §2.4.3 defines it as "the result of
/// applying the fn:boolean function to the value", with no compatibility-mode
/// exception — "In all other cases, fn:boolean raises a type error
/// \[err:FORG0006\]" — and lists "General comparisons, in XPath 1.0
/// compatibility mode" among the places that use that same value. What the mode
/// does change is the function conversion rules (§3.1.5), arithmetic (§3.4),
/// general comparisons (§3.5.2) and the evaluation order of `and`/`or` (§3.6).
#[inline]
fn ebv_is_xpath10(context: &XPathContext<'_>) -> bool {
    context.mode() == XPathMode::XPath10
}

/// Check if an XPathValue contains nodes or is empty (i.e., is a node-set in XPath 1.0 terms).
///
/// In XPath 1.0, the empty result of a path expression is an empty node-set,
/// so `XPathValue::Empty` is treated as a node-set (boolean false).
fn has_nodes_or_empty<N: DomNavigator>(val: &XPathValue<N>) -> bool {
    match val {
        XPathValue::Empty => true,
        XPathValue::Item(XmlItem::Node(_)) => true,
        XPathValue::Sequence(items) => items.iter().any(|i| matches!(i, XmlItem::Node(_))),
        _ => false,
    }
}

#[cfg(test)]
#[path = "eval_tests.rs"]
mod eval_tests;
