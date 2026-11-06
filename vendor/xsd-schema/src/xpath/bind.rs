//! XPath AST binding phase.
//!
//! This module provides the `bind_node()` function which performs static analysis
//! on a parsed XPath AST. During binding:
//!
//! - Function calls are resolved to `FunctionId` via the global registry
//! - Variable references are resolved to slot indices via `NameBinder`
//! - Namespace prefixes are resolved to namespace URIs
//! - Name tests are resolved to interned QNames
//! - Type expressions are resolved to interned atomic type QNames
//!
//! Binding must complete successfully before evaluation can proceed.

use crate::ids::NameId;
use crate::namespace::qname::QualifiedName;
use crate::namespace::table::XS_NAMESPACE;
use crate::types::{NameTest as ResolvedNameTest, XmlTypeCode};
use crate::xpath::arena::{AstArena, AstNodeId};
use crate::xpath::ast::{
    AstNode, FunctionCallNode, ItemTypeNode, NameTest, NodeTest, OccurrenceIndicator, QName,
    SequenceTypeNode, TypeExprKind, TypeExprNode,
};
use crate::xpath::context::{NameBinder, XPathContext};
use crate::xpath::error::XPathError;
use crate::xpath::XPathMode;

/// Bind an AST node and all its children.
///
/// This function performs static analysis on the AST:
/// - Resolves function calls to `FunctionId`
/// - Resolves variable references to slot indices
/// - Validates namespace prefixes
///
/// The results are stored on the nodes (`PathStepNode::resolved_test`,
/// `TypeExprNode::resolved_atomic_type`, and a crate-private resolved item
/// type on `instance of` / `treat as` nodes). They describe the tree as it was
/// bound: after editing a bound node, bind it again.
///
/// # Arguments
/// * `arena` - The AST arena containing all nodes
/// * `id` - The ID of the node to bind
/// * `ctx` - The static context for namespace resolution
/// * `binder` - The name binder for variable slot allocation
///
/// # Returns
/// * `Ok(())` if binding succeeds
/// * `Err(XPathError)` with appropriate error code if binding fails
///
/// # Errors
/// * `XPST0081` - Undefined namespace prefix
/// * `XPST0017` - Function not found
/// * `XPST0008` - Undefined variable
pub fn bind_node(
    arena: &mut AstArena,
    id: AstNodeId,
    ctx: &XPathContext<'_>,
    binder: &mut NameBinder,
) -> Result<(), XPathError> {
    // Clone the node to avoid borrow conflicts
    let node = arena.get(id).clone();

    match node {
        AstNode::Expr(expr) => {
            // Bind all items in the expression sequence
            for item_id in &expr.items {
                bind_node(arena, *item_id, ctx, binder)?;
            }
        }

        AstNode::Value(_) => {
            // Literal values need no binding
        }

        AstNode::ContextItem(_) => {
            // Context item needs no binding
        }

        AstNode::VarRef(var_ref) => {
            // Resolve the variable to a slot
            let name = resolve_var_qname(&var_ref.prefix, &var_ref.local_name, ctx)?;
            let var = binder.resolve_with_names(&name, ctx.names)?;

            // Update the node with the resolved slot
            if let AstNode::VarRef(ref mut node) = arena.get_mut(id) {
                node.slot = Some(var.slot);
            }
        }

        AstNode::If(if_node) => {
            // Bind test, then, and else branches
            bind_node(arena, if_node.test, ctx, binder)?;
            bind_node(arena, if_node.then_branch, ctx, binder)?;
            bind_node(arena, if_node.else_branch, ctx, binder)?;
        }

        AstNode::For(for_node) => {
            // For expressions introduce variables into scope
            // Each binding's in_expr is evaluated in the outer scope,
            // then the variable is pushed for the next binding and return_expr
            for binding_idx in 0..for_node.bindings.len() {
                let binding = &for_node.bindings[binding_idx];
                // Bind the in_expr in current scope
                bind_node(arena, binding.in_expr, ctx, binder)?;

                // Push the variable into scope
                let name = resolve_var_qname(&binding.prefix, &binding.local_name, ctx)?;
                let var = binder.push_var(name);

                // Update the binding with the resolved slot (by index, not name)
                if let AstNode::For(ref mut node) = arena.get_mut(id) {
                    node.bindings[binding_idx].slot = Some(var.slot);
                }
            }

            // Bind the return expression with all variables in scope
            bind_node(arena, for_node.return_expr, ctx, binder)?;

            // Pop all the variables (in reverse order)
            for _ in &for_node.bindings {
                binder.pop_var();
            }
        }

        AstNode::Quantified(quant_node) => {
            // Similar to for expressions
            for binding_idx in 0..quant_node.bindings.len() {
                let binding = &quant_node.bindings[binding_idx];
                bind_node(arena, binding.in_expr, ctx, binder)?;

                let name = resolve_var_qname(&binding.prefix, &binding.local_name, ctx)?;
                let var = binder.push_var(name);

                if let AstNode::Quantified(ref mut node) = arena.get_mut(id) {
                    node.bindings[binding_idx].slot = Some(var.slot);
                }
            }

            bind_node(arena, quant_node.satisfies, ctx, binder)?;

            for _ in &quant_node.bindings {
                binder.pop_var();
            }
        }

        AstNode::FunctionCall(func_call) => {
            // First bind all argument expressions
            for arg_id in &func_call.args {
                bind_node(arena, *arg_id, ctx, binder)?;
            }

            // Resolve the function namespace
            let namespace = if func_call.prefix.is_empty() {
                // Empty prefix -> use default function namespace
                ctx.default_function_namespace().to_string()
            } else {
                // Resolve the prefix to a namespace URI
                ctx.resolve_prefix(&func_call.prefix)
                    .ok_or_else(|| XPathError::undefined_prefix(&func_call.prefix))?
                    .to_string()
            };

            // Check if this is an XPath 2.0 constructor function (e.g. xs:integer(...))
            if let Some(type_expr) = try_bind_constructor_function(&func_call, &namespace, ctx)? {
                *arena.get_mut(id) = AstNode::TypeExpr(type_expr);
                return Ok(());
            }

            // Look up the function via the catalog (supports custom functions)
            let arity = func_call.args.len();
            let Some(handle) =
                ctx.function_catalog()
                    .lookup(&namespace, &func_call.local_name, arity)
            else {
                // The constructor function of an atomic type of the schema set
                // (§3.10.4). A function of the same name and arity in the
                // catalog — one a host registered — has taken precedence above.
                if let Some(type_expr) = try_bind_schema_constructor(&func_call, &namespace, ctx) {
                    *arena.get_mut(id) = AstNode::TypeExpr(type_expr);
                    return Ok(());
                }
                return Err(XPathError::function_not_found(
                    &func_call.local_name,
                    arity,
                    &namespace,
                ));
            };

            // Store the resolved function handle
            if let AstNode::FunctionCall(ref mut node) = arena.get_mut(id) {
                node.function_handle = Some(handle);
            }
        }

        AstNode::PathExpr(path_expr) => {
            // Bind all steps in the path
            for step_id in &path_expr.steps {
                bind_node(arena, *step_id, ctx, binder)?;
            }
        }

        AstNode::FilterExpr(filter_expr) => {
            // Bind the base expression and all predicates
            bind_node(arena, filter_expr.base, ctx, binder)?;
            for pred_id in &filter_expr.predicates {
                bind_node(arena, *pred_id, ctx, binder)?;
            }
        }

        AstNode::Range(range_node) => {
            bind_node(arena, range_node.start, ctx, binder)?;
            bind_node(arena, range_node.end, ctx, binder)?;
        }

        AstNode::UnaryOp(unary_op) => {
            bind_node(arena, unary_op.operand, ctx, binder)?;
        }

        AstNode::BinaryOp(binary_op) => {
            bind_node(arena, binary_op.left, ctx, binder)?;
            bind_node(arena, binary_op.right, ctx, binder)?;
        }

        AstNode::PathStep(path_step) => {
            // Bind predicates in the step
            for pred_id in &path_step.predicates {
                bind_node(arena, *pred_id, ctx, binder)?;
            }

            // Resolve the name test if present.
            //
            // The default-element-namespace only applies to element name
            // tests. For attribute and namespace axes the expanded QName
            // of an unprefixed local name is always (no-namespace, local).
            // §2.5.2 of the XPath 2.0 spec — see also W3C cta0044, where
            // a schema with `xpathDefaultNamespace="abc"` declaring
            // unqualified attributes would otherwise see `@switch`
            // resolved into the default namespace and miss the actual
            // unprefixed attribute on the instance.
            let axis = path_step.axis;
            let is_attribute_axis = matches!(
                axis,
                super::ast::Axis::Attribute | super::ast::Axis::Namespace
            );
            let resolved = resolve_node_test_with_axis(&path_step.test, ctx, is_attribute_axis)?;
            if let AstNode::PathStep(ref mut node) = arena.get_mut(id) {
                node.resolved_test = resolved;
            }
        }

        AstNode::TypeExpr(type_expr) => {
            bind_node(arena, type_expr.operand, ctx, binder)?;

            // Resolve atomic type QName if present
            match type_expr.target_type.item_type {
                Some(ItemTypeNode::Atomic(ref qname)) => {
                    let resolved = if type_expr.from_constructor {
                        resolve_constructor_type_qname(qname, ctx)?
                    } else {
                        resolve_atomic_type_qname(qname, ctx)?
                    };
                    check_atomic_type_name(&resolved, qname, ctx)?;
                    let cast_target = if matches!(
                        type_expr.kind,
                        TypeExprKind::CastAs | TypeExprKind::CastableAs
                    ) {
                        check_cast_target(&resolved, qname, ctx)?;
                        // A target that does not resolve — a user type in a
                        // context that keeps them out of casts — is resolved,
                        // and rejected with XPST0051, when it is evaluated.
                        crate::xpath::schema_cast::resolve_cast_target(&resolved, ctx).ok()
                    } else {
                        None
                    };
                    if let AstNode::TypeExpr(ref mut node) = arena.get_mut(id) {
                        node.resolved_atomic_type = Some(resolved);
                        node.resolved_cast_target = cast_target;
                    }
                }
                // A kind test used as an ItemType spells the same QNames a step
                // node test does, and its prefixes are statically known names
                // in exactly the same way (XPST0081).
                Some(ItemTypeNode::Kind(ref kind)) => check_kind_test_prefixes(kind, ctx)?,
                Some(ItemTypeNode::Item) | None => {}
            }

            // `instance of` / `treat as`: resolve the item type once, here,
            // rather than for every item the expression tests.
            if matches!(
                type_expr.kind,
                TypeExprKind::InstanceOf | TypeExprKind::TreatAs
            ) {
                if let AstNode::TypeExpr(ref mut node) = arena.get_mut(id) {
                    node.resolved_item_type = match &node.target_type.item_type {
                        Some(item_type) => {
                            Some(crate::xpath::node_test::item_type_of_item_type_node(
                                item_type,
                                node.resolved_atomic_type.as_ref(),
                                ctx,
                            )?)
                        }
                        None => None,
                    };
                }
            }
        }
    }

    Ok(())
}

/// Resolve a variable QName from prefix and local name.
///
/// This function interns the local name and prefix into the NameTable using `add()`,
/// which always succeeds (returning existing NameId or creating new one).
/// Only namespace prefix resolution can fail if the prefix is not bound.
fn resolve_var_qname(
    prefix: &str,
    local_name: &str,
    ctx: &XPathContext<'_>,
) -> Result<QualifiedName, XPathError> {
    // Intern the local name (always succeeds)
    let local_id = ctx.names.add(local_name);

    if prefix.is_empty() {
        Ok(QualifiedName::local(local_id))
    } else {
        // Intern the prefix
        let prefix_id = ctx.names.add(prefix);

        // Resolve prefix to namespace - THIS can still fail legitimately
        let ns_id = ctx
            .resolve_prefix_id(prefix_id)
            .ok_or_else(|| XPathError::undefined_prefix(prefix))?;

        Ok(QualifiedName::new(Some(ns_id), local_id, Some(prefix_id)))
    }
}

/// Resolve a NodeTest to a ResolvedNameTest with axis-aware namespace
/// handling. When `is_attribute_axis` is true (attribute or namespace
/// axis), an unprefixed local name resolves to the no-namespace QName
/// instead of using the default-element-namespace.
fn resolve_node_test_with_axis(
    test: &NodeTest,
    ctx: &XPathContext<'_>,
    is_attribute_axis: bool,
) -> Result<Option<ResolvedNameTest>, XPathError> {
    match test {
        NodeTest::Name(name_test) => {
            let resolved = resolve_name_test_with_axis(name_test, ctx, is_attribute_axis)?;
            Ok(Some(resolved))
        }
        NodeTest::Kind(kind) => {
            // A kind test carries no *name test* to resolve — the QNames inside
            // `element(N, T)` and friends are expanded at evaluation time,
            // against the same default-element-namespace rule the step uses.
            // Their prefixes are still statically known names, though, and the
            // type and declaration names must be in the static context, so
            // they are checked here; see [`check_kind_test_prefixes`].
            check_kind_test_prefixes(kind, ctx)?;
            Ok(None)
        }
    }
}

/// Check every namespace prefix a kind test spells, raising `XPST0081` for one
/// that the statically known namespaces cannot expand.
///
/// XPath 2.0, Appendix G: "err:XPST0081 It is a static error if a QName used in
/// an expression contains a namespace prefix that cannot be expanded into a
/// namespace URI by using the statically known namespaces." That covers the
/// element/attribute name and the type name of `element(N, T)` /
/// `attribute(N, T)`, the `ElementName` of `schema-element(N)` and the
/// `AttributeName` of `schema-attribute(N)`, and the same again inside a
/// `document-node(...)`.
///
/// An *unprefixed* name is not checked: it expands through the default element
/// namespace (or, for an attribute name, to no namespace), which never fails.
///
/// The names that must be in the static context are checked here as well
/// (`XPST0008`): the `TypeName` of `element(N, T)` / `attribute(N, T)` must be
/// in the in-scope schema types ([`check_type_name_in_scope`]), and the name of
/// `schema-element(N)` / `schema-attribute(N)` in the in-scope declarations
/// ([`check_schema_declaration_in_scope`]).
fn check_kind_test_prefixes(
    kind: &crate::xpath::ast::KindTest,
    ctx: &XPathContext<'_>,
) -> Result<(), XPathError> {
    use crate::xpath::ast::KindTest;

    match kind {
        KindTest::AnyKind
        | KindTest::Text
        | KindTest::Comment
        | KindTest::ProcessingInstruction(_) => Ok(()),
        KindTest::Document(inner) => match inner {
            Some(inner) => check_kind_test_prefixes(inner, ctx),
            None => Ok(()),
        },
        KindTest::Element(test) => {
            check_optional_qname_prefix(test.name.as_ref(), ctx)?;
            check_optional_qname_prefix(test.type_name.as_ref(), ctx)?;
            check_type_name_in_scope(test.type_name.as_ref(), ctx)
        }
        KindTest::Attribute(test) => {
            check_optional_qname_prefix(test.name.as_ref(), ctx)?;
            check_optional_qname_prefix(test.type_name.as_ref(), ctx)?;
            check_type_name_in_scope(test.type_name.as_ref(), ctx)
        }
        KindTest::SchemaElement(name) => {
            check_lexical_qname_prefix(name, ctx)?;
            check_schema_declaration_in_scope(name, ctx, crate::xpath::DomNodeType::Element)
        }
        KindTest::SchemaAttribute(name) => {
            check_lexical_qname_prefix(name, ctx)?;
            check_schema_declaration_in_scope(name, ctx, crate::xpath::DomNodeType::Attribute)
        }
    }
}

/// `XPST0008` unless the `ElementName` of a `schema-element(N)` (or the
/// `AttributeName` of a `schema-attribute(N)`) names a declaration in the
/// in-scope element (attribute) declarations.
///
/// XPath 2.0 §2.5.4.4: "If the ElementName specified in the SchemaElementTest
/// is not found in the in-scope element declarations, a static error is raised
/// \[err:XPST0008\]." §2.5.4.6 says the same of an `AttributeName` and the
/// in-scope attribute declarations. Those are part of the static context
/// (§2.1.1, "In-scope element declarations. Each element declaration is
/// identified either by an expanded QName (for a top-level element
/// declaration) or by an implementation-dependent element identifier (for a
/// local element declaration)"): here the top-level declarations of the schema
/// set attached to the static context. A context with no schema set has none,
/// so every such test is `XPST0008` there.
///
/// The name expands as the matcher expands it (an unprefixed `ElementName` in
/// the default element/type namespace, an unprefixed `AttributeName` in no
/// namespace); its prefix has already been checked (XPST0081).
///
/// A context limited to the built-in components
/// ([`XPathContext::with_built_in_components_only`]) has no element
/// declaration in scope and, of the attribute declarations, the four built-in
/// `xsi:` ones (Structures 1.1 §3.2.7: "There are four attribute declarations
/// present in every schema by definition").
fn check_schema_declaration_in_scope(
    name: &str,
    ctx: &XPathContext<'_>,
    principal: crate::xpath::DomNodeType,
) -> Result<(), XPathError> {
    let declared = crate::xpath::node_test::resolve_schema_test_name(name, ctx, principal)
        .zip(ctx.schema_set)
        .is_some_and(|(qname, schema_set)| {
            let user_components = ctx.user_components().is_some();
            if principal == crate::xpath::DomNodeType::Attribute {
                schema_set
                    .lookup_attribute(qname.namespace_uri, qname.local_name)
                    .is_some_and(|key| {
                        user_components || schema_set.builtin_types().is_builtin_attribute(key)
                    })
            } else {
                user_components
                    && schema_set
                        .lookup_element(qname.namespace_uri, qname.local_name)
                        .is_some()
            }
        });
    if declared {
        Ok(())
    } else {
        Err(XPathError::undefined_qname(name))
    }
}

/// `XPST0008` unless the `TypeName` of an `element(N, T)` / `attribute(N, T)`
/// test (if any) names a type in the in-scope schema types.
///
/// XPath 2.0 §2.5.4.3: "TypeName must be present in the in-scope schema types
/// \[err:XPST0008\]" (§2.5.4.5 for attributes). The in-scope schema types are
/// the built-in types of the XML Schema namespace — which include `xs:untyped`
/// and `xs:untypedAtomic` (§2.5.1) — plus, when a schema set is attached, its
/// named types, unless the context is limited to the built-in components
/// ([`XPathContext::with_built_in_components_only`]). §2.5.1 leaves the predefined set to the host; this engine's is
/// every built-in type it knows, the XSD 1.1 ones included, whatever the
/// schema set's version (see `TypeTest::BuiltIn`). An unprefixed `TypeName` is in the default element/type
/// namespace. The prefix has already been checked (XPST0081).
fn check_type_name_in_scope(
    type_name: Option<&QName>,
    ctx: &XPathContext<'_>,
) -> Result<(), XPathError> {
    let Some(type_name) = type_name else {
        return Ok(());
    };
    let ns_id = if type_name.prefix.is_empty() {
        ctx.default_element_ns
    } else {
        ctx.resolve_prefix(&type_name.prefix)
            .map(|uri| ctx.names.add(&uri))
    };
    let is_xs = ns_id == Some(crate::namespace::table::well_known::XS_NAMESPACE);
    let in_scope = if is_xs {
        type_name.local == "untyped" || XmlTypeCode::from_local_name(&type_name.local).is_some()
    } else {
        ctx.user_components().is_some_and(|schema_set| {
            schema_set
                .lookup_type(ns_id, ctx.names.add(&type_name.local))
                .is_some()
        })
    };
    if in_scope {
        return Ok(());
    }
    Err(XPathError::undefined_qname(
        if type_name.prefix.is_empty() {
            type_name.local.clone()
        } else {
            format!("{}:{}", type_name.prefix, type_name.local)
        },
    ))
}

/// `XPST0081` unless `qname` is absent, unprefixed, or carries a prefix the
/// static context can expand.
fn check_optional_qname_prefix(
    qname: Option<&QName>,
    ctx: &XPathContext<'_>,
) -> Result<(), XPathError> {
    let Some(qname) = qname else {
        return Ok(());
    };
    check_prefix(&qname.prefix, ctx)
}

/// The same check for a kind test that keeps its name in lexical form, as
/// `schema-element(N)` and `schema-attribute(N)` do. A name the parser let
/// through but that is not a lexical QName is left to evaluation, which does
/// not match it; only the prefix is this function's concern.
fn check_lexical_qname_prefix(name: &str, ctx: &XPathContext<'_>) -> Result<(), XPathError> {
    let Ok((prefix, _)) = crate::xpath::functions::qname::parse_lexical_qname(name) else {
        return Ok(());
    };
    match prefix {
        Some(prefix) => check_prefix(&prefix, ctx),
        None => Ok(()),
    }
}

/// `XPST0081` unless `prefix` is empty or the static context can expand it.
fn check_prefix(prefix: &str, ctx: &XPathContext<'_>) -> Result<(), XPathError> {
    if prefix.is_empty() {
        return Ok(());
    }
    let prefix_id = ctx.names.add(prefix);
    if ctx.resolve_prefix_id(prefix_id).is_some() {
        return Ok(());
    }
    Err(XPathError::undefined_prefix(prefix))
}

/// Resolve an AST-level NameTest to a type-system NameTest with interned names.
///
/// Handles all wildcard patterns:
/// - `*` -> Wildcard
/// - `prefix:*` -> LocalWildcard (namespace URI)
/// - `*:local` -> NamespaceWildcard (local name)
/// - `prefix:local` or `local` -> QName
fn resolve_name_test_with_axis(
    name_test: &NameTest,
    ctx: &XPathContext<'_>,
    is_attribute_axis: bool,
) -> Result<ResolvedNameTest, XPathError> {
    match (&name_test.prefix, &name_test.local_name) {
        // * - wildcard matches any name
        (None, None) => Ok(ResolvedNameTest::Wildcard),

        // *:local - any namespace with specific local name
        (None, Some(local)) => {
            let local_id = ctx.names.add(local);
            Ok(ResolvedNameTest::NamespaceWildcard(local_id))
        }

        // prefix:* - any local name in namespace
        (Some(prefix), None) => {
            if prefix.is_empty() {
                // Empty prefix with wildcard local. For element axes
                // the default-element-namespace applies; for attribute
                // and namespace axes an unprefixed name is always in
                // no namespace per XPath 2.0 §2.5.2.
                if !is_attribute_axis {
                    if let Some(ns_id) = ctx.default_element_ns {
                        return Ok(ResolvedNameTest::LocalWildcard(ns_id));
                    }
                }
                let empty_ns = ctx.names.add("");
                Ok(ResolvedNameTest::LocalWildcard(empty_ns))
            } else {
                let prefix_id = ctx.names.add(prefix);
                let ns_id = ctx
                    .resolve_prefix_id(prefix_id)
                    .ok_or_else(|| XPathError::undefined_prefix(prefix))?;
                Ok(ResolvedNameTest::LocalWildcard(ns_id))
            }
        }

        // prefix:local - specific QName
        (Some(prefix), Some(local)) => {
            let local_id = ctx.names.add(local);
            if prefix.is_empty() {
                // Unprefixed local name. Element axes pick up the
                // default-element-namespace; attribute and namespace
                // axes always resolve to no namespace per XPath 2.0
                // §2.5.2 (W3C cta0044 regression).
                let ns_id = if is_attribute_axis {
                    None
                } else {
                    ctx.default_element_ns
                };
                Ok(ResolvedNameTest::QName(QualifiedName::new(
                    ns_id, local_id, None,
                )))
            } else {
                let prefix_id = ctx.names.add(prefix);
                let ns_id = ctx
                    .resolve_prefix_id(prefix_id)
                    .ok_or_else(|| XPathError::undefined_prefix(prefix))?;
                Ok(ResolvedNameTest::QName(QualifiedName::new(
                    Some(ns_id),
                    local_id,
                    Some(prefix_id),
                )))
            }
        }
    }
}

/// Resolve an atomic type QName (e.g., xs:integer) to interned form.
///
/// Atomic types use the XML Schema namespace by default when unprefixed.
fn resolve_atomic_type_qname(
    qname: &QName,
    ctx: &XPathContext<'_>,
) -> Result<QualifiedName, XPathError> {
    let local_id = ctx.names.add(&qname.local);

    if qname.prefix.is_empty() {
        // Unprefixed atomic types: in XPath 2.0, unprefixed type names in
        // cast/instance-of use the default element namespace, not xs:
        // But for compatibility, many implementations treat them as xs: types
        // Use default element namespace if set, otherwise no namespace
        let ns_id = ctx.default_element_ns;
        Ok(QualifiedName::new(ns_id, local_id, None))
    } else {
        let prefix_id = ctx.names.add(&qname.prefix);
        let ns_id = ctx
            .resolve_prefix_id(prefix_id)
            .ok_or_else(|| XPathError::undefined_prefix(&qname.prefix))?;
        Ok(QualifiedName::new(Some(ns_id), local_id, Some(prefix_id)))
    }
}

/// Check that a QName used as an `AtomicType` really names an atomic type.
///
/// XPath 2.0 §2.5.4.2: "An `ItemType` consisting simply of a QName is
/// interpreted as an `AtomicType`. … If a QName that is used as an `AtomicType`
/// is not defined as an atomic type in the in-scope schema types, a static error
/// is raised [err:XPST0051]." §3.10.2 and §3.10.3 impose the same requirement on
/// the target type of `cast as` and `castable as`. The spec's own note spells the
/// consequence out: "The names of non-atomic types such as `xs:IDREFS` are not
/// accepted."
///
/// The in-scope schema types are the built-in types of the XML Schema namespace
/// plus, when a schema set is attached to the static context, its named types
/// (unless the context is limited to the built-in components,
/// [`XPathContext::with_built_in_components_only`]); of those, only the atomic
/// ones qualify — not a union or list type of the
/// schema, nor a restriction of one.
///
/// Note: `xs:NOTATION` and `xs:anyAtomicType` are atomic types and so pass this
/// check; §3.10.2 forbids them as the target of a `cast` with a separate rule
/// and code, which [`check_cast_target`] applies (XPST0080).
fn check_atomic_type_name(
    resolved: &QualifiedName,
    raw: &QName,
    ctx: &XPathContext<'_>,
) -> Result<(), XPathError> {
    let unknown = || XPathError::XPST0051 {
        type_name: if raw.prefix.is_empty() {
            raw.local.clone()
        } else {
            format!("{}:{}", raw.prefix, raw.local)
        },
    };

    let local = ctx
        .names
        .try_resolve(resolved.local_name)
        .ok_or_else(unknown)?;

    let is_xs = resolved
        .namespace_uri
        .and_then(|id| ctx.names.try_resolve(id))
        .is_some_and(|ns| ns == XS_NAMESPACE);

    if is_xs {
        let code = XmlTypeCode::from_local_name(&local).ok_or_else(unknown)?;
        // `xs:anyAtomicType` is the base of the atomic types and is itself a
        // legal `AtomicType`; `is_atomic` groups it with the abstract types, so
        // it is allowed here explicitly. Everything else that is not atomic —
        // `xs:anyType`, `xs:anySimpleType`, the list types, `xs:error` — is not.
        if code == XmlTypeCode::AnyAtomicType || code.is_atomic() {
            return Ok(());
        }
        return Err(unknown());
    }

    // A name outside the XML Schema namespace can only be in the in-scope schema
    // types when a schema set is attached, and must be an *atomic* simple type
    // there: a union or a list type (or a restriction of one) is a simple type
    // but not an atomic type, so it is XPST0051 just like a complex type.
    let schema_set = ctx.user_components().ok_or_else(unknown)?;
    match schema_set.lookup_type(resolved.namespace_uri, resolved.local_name) {
        Some(key) if crate::xpath::node_test::is_atomic_simple_type(schema_set, key) => Ok(()),
        _ => Err(unknown()),
    }
}

/// Check the target type of a `cast as` / `castable as` beyond its being an
/// atomic type.
///
/// XPath 2.0 §3.10.2 (and §3.10.3 in the same words): "The target type must be
/// an atomic type that is in the in-scope schema types \[err:XPST0051\]. In
/// addition, the target type cannot be xs:NOTATION or xs:anyAtomicType
/// \[err:XPST0080\]." Both names pass [`check_atomic_type_name`] — they are
/// atomic types, legal in `instance of` and `treat as` — so this is the second,
/// cast-only rule. The error is static, so it is raised here, while binding.
fn check_cast_target(
    resolved: &QualifiedName,
    raw: &QName,
    ctx: &XPathContext<'_>,
) -> Result<(), XPathError> {
    let is_xs = resolved
        .namespace_uri
        .and_then(|id| ctx.names.try_resolve(id))
        .is_some_and(|ns| ns == XS_NAMESPACE);
    if !is_xs {
        return Ok(());
    }
    let code = ctx
        .names
        .try_resolve(resolved.local_name)
        .and_then(|local| XmlTypeCode::from_local_name(&local));
    if matches!(
        code,
        Some(XmlTypeCode::Notation | XmlTypeCode::AnyAtomicType)
    ) {
        let name = if raw.prefix.is_empty() {
            raw.local.clone()
        } else {
            format!("{}:{}", raw.prefix, raw.local)
        };
        return Err(XPathError::cast_target_not_instantiable(&name));
    }
    Ok(())
}

/// Try to bind a function call as an XPath 2.0 constructor function.
///
/// Constructor functions allow XML Schema type names (e.g. `xs:integer`, `xs:date`)
/// to be used as single-argument function calls for type casting. For example,
/// `xs:unsignedShort(42)` is equivalent to `42 cast as xs:unsignedShort`.
///
/// Returns `Ok(Some(type_expr))` if this is a valid constructor function, or
/// `Ok(None)` if it's not a constructor (fall through to normal lookup, which
/// makes `xs:NOTATION(...)` and `xs:anyAtomicType(...)` XPST0017).
///
/// The type name is the call's expanded name — in the XML Schema namespace,
/// whether through a prefix or through the default function namespace — not
/// the expansion a `cast as` gives an unprefixed name (the default
/// element/type namespace).
fn try_bind_constructor_function(
    func_call: &FunctionCallNode,
    namespace: &str,
    ctx: &XPathContext<'_>,
) -> Result<Option<TypeExprNode>, XPathError> {
    // Constructor functions are an XPath 2.0 feature
    if ctx.mode() != XPathMode::XPath20 {
        return Ok(None);
    }

    // Must be in the XML Schema namespace with exactly 1 argument
    if namespace != XS_NAMESPACE || func_call.args.len() != 1 {
        return Ok(None);
    }

    // Check if the local name matches a known XSD type
    let type_code = match XmlTypeCode::from_local_name(&func_call.local_name) {
        Some(tc) => tc,
        None => return Ok(None),
    };

    // XPath 2.0 §3.10.4: "For every atomic type in the in-scope schema types
    // (except xs:NOTATION and xs:anyAtomicType, which are not instantiable), a
    // constructor function is implicitly defined." So neither of those two,
    // nor a list type or an abstract type, is a constructor function — fall
    // through to normal function lookup, which produces XPST0017.
    if type_code.is_list()
        || matches!(
            type_code,
            XmlTypeCode::AnyType
                | XmlTypeCode::AnySimpleType
                | XmlTypeCode::Notation
                | XmlTypeCode::AnyAtomicType
        )
    {
        return Ok(None);
    }

    let resolved = QualifiedName::new(
        Some(crate::namespace::table::well_known::XS_NAMESPACE),
        ctx.names.add(&func_call.local_name),
        constructor_prefix(func_call, ctx),
    );
    Ok(Some(constructor_type_expr(
        func_call,
        resolved,
        crate::xpath::schema_cast::CastTarget::BuiltIn(type_code),
    )))
}

/// Try to bind a function call as the constructor function of an atomic type
/// of the schema set in the static context.
///
/// XPath 2.0 §3.10.4: "For every atomic type in the in-scope schema types
/// (except xs:NOTATION and xs:anyAtomicType, which are not instantiable), a
/// constructor function is implicitly defined. In each case, the name of the
/// constructor function is the same as the name of its target type (including
/// namespace)." So the call's expanded name — through the function namespace
/// rules: a prefix, or the default function namespace, which must be none
/// for a type in no namespace — names the type, and the call takes one
/// argument. F&O §5.3 adds that types derived from `xs:NOTATION` have
/// constructors too; `xs:NOTATION` itself is in the `xs` namespace and has
/// none ([`try_bind_constructor_function`]).
///
/// Called after the function catalog found no function of that name and
/// arity, so a function a host registered under the same name keeps
/// precedence. `None` when the name is no atomic type of the schema set
/// (a list, union or complex type has no constructor: XPST0017).
fn try_bind_schema_constructor(
    func_call: &FunctionCallNode,
    namespace: &str,
    ctx: &XPathContext<'_>,
) -> Option<TypeExprNode> {
    if ctx.mode() != XPathMode::XPath20 || func_call.args.len() != 1 || namespace == XS_NAMESPACE {
        return None;
    }
    if !ctx.schema_casts() {
        return None;
    }
    let schema_set = ctx.schema_set?;
    let namespace_id = (!namespace.is_empty()).then(|| ctx.names.add(namespace));
    let local_id = ctx.names.add(&func_call.local_name);
    let target = crate::xpath::schema_cast::schema_atomic_type(schema_set, namespace_id, local_id)?;
    let resolved = QualifiedName::new(namespace_id, local_id, constructor_prefix(func_call, ctx));
    Some(constructor_type_expr(func_call, resolved, target))
}

/// The interned prefix of a constructor function call, if it has one.
fn constructor_prefix(func_call: &FunctionCallNode, ctx: &XPathContext<'_>) -> Option<NameId> {
    (!func_call.prefix.is_empty()).then(|| ctx.names.add(&func_call.prefix))
}

/// The `($arg) cast as T?` a constructor function call `T($arg)` stands for
/// (§3.10.4: "The semantics of the constructor function call T($arg) are
/// defined to be equivalent to the expression (($arg) cast as T?)"), with its
/// target already resolved.
fn constructor_type_expr(
    func_call: &FunctionCallNode,
    resolved: QualifiedName,
    target: crate::xpath::schema_cast::CastTarget,
) -> TypeExprNode {
    let qname = QName {
        prefix: func_call.prefix.clone(),
        local: func_call.local_name.clone(),
    };
    let target_type = SequenceTypeNode::single(
        ItemTypeNode::Atomic(qname),
        OccurrenceIndicator::ZeroOrOne,
        func_call.span,
    );
    let mut type_expr = TypeExprNode::new(
        TypeExprKind::CastAs,
        func_call.args[0],
        target_type,
        func_call.span,
    );
    type_expr.resolved_atomic_type = Some(resolved);
    type_expr.resolved_cast_target = Some(target);
    type_expr.from_constructor = true;
    type_expr
}

/// Expand the type name of a node made from a constructor function call when
/// it is bound again: with the function namespace rules the call's name was
/// expanded with — a prefix through the statically known namespaces, no
/// prefix through the default function namespace (none: no namespace) — not
/// with the default element/type namespace of a `cast as`.
fn resolve_constructor_type_qname(
    qname: &QName,
    ctx: &XPathContext<'_>,
) -> Result<QualifiedName, XPathError> {
    if !qname.prefix.is_empty() {
        return resolve_atomic_type_qname(qname, ctx);
    }
    let namespace = ctx.default_function_namespace();
    let namespace_id = (!namespace.is_empty()).then(|| ctx.names.add(namespace));
    Ok(QualifiedName::new(
        namespace_id,
        ctx.names.add(&qname.local),
        None,
    ))
}

#[cfg(test)]
#[path = "bind_tests.rs"]
mod bind_tests;
