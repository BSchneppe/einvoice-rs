//! XPath 2.0 special/context functions.
//!
//! This module implements:
//! - fn:position() - return context position
//! - fn:last() - return context size
//! - fn:trace($value, $label?) - debug output and passthrough
//! - fn:data($arg) - atomize a sequence
//! - fn:default-collation() - return default collation URI
//! - fn:error($error?, $description, $error-object?) - raise a dynamic error
//!   (see [`error`] for the four signatures and the argument types it
//!   enforces)

use crate::xpath::context::DynamicContext;
use crate::xpath::error::XPathError;
use crate::xpath::DomNavigator;

use super::{atomize_sequence, XPathValue};
use crate::xpath::iterator::XmlItem;

/// fn:position() as xs:integer
///
/// Returns the context position of the current item within the sequence
/// being processed. This is a 1-based position.
///
/// Raises XPDY0002 if the context item is undefined.
pub fn position<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if !args.is_empty() {
        return Err(XPathError::wrong_number_of_arguments(
            "position",
            0,
            args.len(),
        ));
    }
    // Context must be defined for position()
    if context.context_item.is_none() && context.context_position == 0 {
        return Err(XPathError::XPDY0002 {
            message: "Context is undefined for fn:position()".to_string(),
        });
    }
    Ok(XPathValue::integer(context.context_position as i64))
}

/// fn:last() as xs:integer
///
/// Returns the context size (total number of items in the sequence
/// being processed).
///
/// Raises XPDY0002 if the context item is undefined.
pub fn last<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if !args.is_empty() {
        return Err(XPathError::wrong_number_of_arguments("last", 0, args.len()));
    }
    // Context must be defined for last()
    if context.context_item.is_none() && context.context_size == 0 {
        return Err(XPathError::XPDY0002 {
            message: "Context is undefined for fn:last()".to_string(),
        });
    }
    Ok(XPathValue::integer(context.context_size as i64))
}

/// fn:trace($value as item()*, $label as xs:string?) as item()*
///
/// Returns $value unchanged, after writing $label and $value to trace output.
/// This function is intended for debugging.
///
/// XPath 2.0 requires two arguments, but we support 1-2 for flexibility.
pub fn trace<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.is_empty() || args.len() > 2 {
        return Err(XPathError::wrong_number_of_arguments(
            "trace",
            2,
            args.len(),
        ));
    }

    // Get optional label (second argument)
    let label = if args.len() == 2 {
        let label_arg = args.pop().unwrap();
        super::atomize_to_string_opt(label_arg)?
    } else {
        None
    };

    // Get value (first argument) - we'll return this unchanged
    let value = args.remove(0);

    // Write trace output to stderr only when trace is enabled
    if context.static_context.trace_enabled {
        let value_str = value_to_trace_string(&value);
        if let Some(label) = label {
            eprintln!("[trace] {}: {}", label, value_str);
        } else {
            eprintln!("[trace] {}", value_str);
        }
    }

    // Return value unchanged
    Ok(value)
}

/// Convert an XPathValue to a string for trace output.
fn value_to_trace_string<N: DomNavigator>(value: &XPathValue<N>) -> String {
    match value {
        XPathValue::Empty => "()".to_string(),
        XPathValue::Item(item) => item_to_trace_string(item),
        XPathValue::Sequence(items) => {
            let strs: Vec<String> = items.iter().map(item_to_trace_string).collect();
            format!("({})", strs.join(", "))
        }
    }
}

/// Convert an XmlItem to a string for trace output.
fn item_to_trace_string<N: DomNavigator>(item: &XmlItem<N>) -> String {
    match item {
        XmlItem::Atomic(value) => value.to_string_value(),
        XmlItem::Node(nav) => format!("<{}...>", nav.name()),
    }
}

/// fn:data($arg as item()*) as xs:anyAtomicType*
///
/// Returns the atomized value of each item in $arg.
///
/// For atomic values, returns the value itself.
/// For nodes, returns the typed value of the node.
pub fn data<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments("data", 1, args.len()));
    }

    let arg = args.remove(0);

    // Atomize the entire sequence
    let atomized = atomize_sequence(arg)?;

    // Convert back to XPathValue
    let items: Vec<XmlItem<N>> = atomized.into_iter().map(XmlItem::Atomic).collect();

    Ok(XPathValue::from_sequence(items))
}

/// fn:default-collation() as xs:string
///
/// Returns the value of the default collation property from the static context
/// — the URI a host passed to
/// [`XPathContext::with_default_collation`](crate::xpath::XPathContext::with_default_collation),
/// and otherwise the Unicode codepoint collation, which F&O §7.3.1 makes the
/// default when the static context names none.
pub fn default_collation<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if !args.is_empty() {
        return Err(XPathError::wrong_number_of_arguments(
            "default-collation",
            0,
            args.len(),
        ));
    }
    Ok(XPathValue::string(
        context.static_context.default_collation(),
    ))
}

/// `fn:error()`
/// `fn:error($error as xs:QName)`
/// `fn:error($error as xs:QName?, $description as xs:string)`
/// `fn:error($error as xs:QName?, $description as xs:string, $error-object as item()*)`
///
/// Raises a dynamic error identified by `$error`, carrying `$description`.
/// With no arguments, or with an empty `$error`, the error QName is
/// `{http://www.w3.org/2005/xqt-errors}FOER0000` and, for the no-argument form,
/// the description is that QName's own URI.
///
/// The function never returns a value: its declared return type is the empty
/// type `none`, so every call is an error — an
/// [`XPathError::Raised`].
///
/// Read the raised error's QName and description back with
/// [`XPathError::raised_error`]; [`XPathError::error_code`] reports
/// `FOER0000` for the default QName. `$error-object` travels with the error as
/// atomic values ([`RaisedErrorInfo::error_object`]): an atomic item as it is,
/// a node as its string value, typed `xs:untypedAtomic` — the error outlives
/// the documents its nodes belong to.
///
/// [`RaisedErrorInfo::error_object`]: crate::xpath::error::RaisedErrorInfo::error_object
///
/// # Argument types
///
/// The parameter types above are enforced here, in the implementation, and not
/// only by whatever signature a host registers: the engine does not check a
/// registered function's declared parameter types at evaluation time (it checks
/// the arity at bind time), so a loose registration must not loosen the
/// function. Each argument goes through the function conversion rules of XPath
/// 2.0 §3.1.5, and a value the declared type does not match raises the closing
/// `XPTY0004` of those rules:
///
/// * `$error` — exactly one `xs:QName` in the one-argument form, and
///   `xs:QName?` in the other two, so `error(())` is a type error while
///   `error((), 'd')` is the default `FOER0000`. `xs:untypedAtomic` cannot be
///   cast to `xs:QName`, so an untyped argument is a type error too.
/// * `$description` — exactly one `xs:string`. Atomization, the
///   `xs:untypedAtomic` cast and `xs:anyURI` promotion apply; nothing turns an
///   `xs:integer` into a string, so `error((), 42)` is a type error, and so is
///   `error((), ())`.
/// * `$error-object` — `item()*`: anything, including the empty sequence.
///
/// In XPath 1.0 compatibility mode the §3.1.5 conversions run first, over the
/// declared types of the registered signature, so `error((), 42)` there becomes
/// `error((), '42')` and raises `FOER0000` rather than a type error.
///
/// # A built-in
///
/// `error` is a built-in function ([`FunctionId::Error`](super::FunctionId::Error))
/// at arities 0 to 3, so `error()` needs no registration. A host that registers
/// its own `fn:error` — as one had to before 0.3.0 — keeps working: a function
/// a [`FunctionSet`](super::FunctionSet) registers takes precedence over the
/// built-in of the same name and arity. The registration below is the one a
/// host used before; it now changes nothing:
///
/// ```
/// use xsd_schema::namespace::table::NameTable;
/// use xsd_schema::xpath::api::XPathExpr;
/// use xsd_schema::xpath::functions::signature::types::{any, empty, qname, qname_opt, string};
/// use xsd_schema::xpath::functions::{
///     special, DynamicFunctionSignature, FunctionSet, FN_NAMESPACE,
/// };
/// use xsd_schema::xpath::{RoXmlNavigator, XPathContext};
///
/// let mut functions: FunctionSet<RoXmlNavigator<'static>> = FunctionSet::with_builtins();
/// for params in [
///     vec![],
///     vec![qname()],
///     vec![qname_opt(), string()],
///     vec![qname_opt(), string(), any()],
/// ] {
///     functions.register(
///         DynamicFunctionSignature::new(FN_NAMESPACE, "error", params, empty()),
///         special::error,
///     );
/// }
///
/// let names = NameTable::new();
/// let ctx = XPathContext::new(&names).with_function_catalog(&functions);
///
/// let run = |expr: &str| {
///     let compiled = XPathExpr::compile(expr, &ctx).unwrap();
///     let outcome = compiled
///         .evaluator(&ctx)
///         .run_with::<RoXmlNavigator<'static>, _>(|eval| {
///             eval.context().set_function_evaluator(Some(&functions));
///         });
///     let Err(err) = outcome else {
///         panic!("fn:error never returns a value")
///     };
///     err
/// };
///
/// let err = run("error()");
/// assert_eq!(err.error_code(), Some("FOER0000"));
/// assert_eq!(err.raised_error().unwrap().local_name, "FOER0000");
///
/// // The declared types are enforced by the implementation.
/// assert_eq!(run("error(())").error_code(), Some("XPTY0004"));
/// assert_eq!(run("error((), 42)").error_code(), Some("XPTY0004"));
/// assert_eq!(run("error((), ())").error_code(), Some("XPTY0004"));
/// ```
pub fn error<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    let arity = args.len();
    if arity > 3 {
        return Err(XPathError::wrong_number_of_arguments("error", 3, arity));
    }

    // F&O gives `fn:error()` with no arguments the same meaning as
    // `fn:error(fn:QName(…, 'err:FOER0000'), 'http://www.w3.org/2005/xqt-errors#FOER0000')`.
    let no_arguments = arity == 0;

    // $error-object as item()*: no type to enforce. It is carried as atomic
    // values, a node by its string value (see the function's documentation).
    let error_object: Vec<crate::types::value::XmlValue> = if arity == 3 {
        args.pop()
            .unwrap()
            .into_vec()
            .into_iter()
            .map(|item| match item {
                XmlItem::Atomic(value) => value,
                XmlItem::Node(nav) => crate::types::value::XmlValue::untyped(nav.value()),
            })
            .collect()
    } else {
        Vec::new()
    };

    // $description as xs:string: required wherever it is present at all.
    let description = if arity >= 2 {
        Some(description_argument(args.pop().unwrap())?)
    } else {
        None
    };

    // $error as xs:QName in the one-argument form, xs:QName? in the others.
    let qname = match args.pop() {
        None => None,
        Some(arg) => {
            let expected = if arity == 1 { "xs:QName" } else { "xs:QName?" };
            match error_qname_argument(arg, expected)? {
                Some(qname) => Some(qname),
                // "fn:error($error as xs:QName)": the one-argument form has no
                // occurrence indicator, so the empty sequence does not match.
                None if arity == 1 => {
                    return Err(XPathError::XPTY0004 {
                        expected: expected.to_string(),
                        found: "empty-sequence()".to_string(),
                    })
                }
                None => None,
            }
        }
    };

    let names = context.static_context.names;
    let (namespace_uri, local_name) = match qname {
        Some(qname) => (
            qname
                .namespace_uri
                .map(|id| names.resolve(id).to_string())
                .unwrap_or_default(),
            names.resolve(qname.local_name).to_string(),
        ),
        None => (
            crate::xpath::error::XQT_ERRORS_NAMESPACE.to_string(),
            crate::xpath::error::DEFAULT_RAISED_ERROR.to_string(),
        ),
    };

    let default_description = format!(
        "{}#{}",
        crate::xpath::error::XQT_ERRORS_NAMESPACE,
        crate::xpath::error::DEFAULT_RAISED_ERROR
    );
    let description = match (&description, no_arguments) {
        (Some(d), _) => Some(d.as_str()),
        (None, true) => Some(default_description.as_str()),
        (None, false) => None,
    };

    Err(XPathError::raised_with_object(
        &namespace_uri,
        &local_name,
        description,
        error_object,
    ))
}

/// The `$description as xs:string` argument of `fn:error`, under the function
/// conversion rules of XPath 2.0 §3.1.5.
///
/// Atomization runs first; `super::atomize_to_string_strict` then supplies the
/// only two conversions the rules allow into `xs:string` — the
/// `xs:untypedAtomic` cast (which is where an untyped node ends up) and
/// `xs:anyURI` promotion — and raises `XPTY0004` for everything else. The
/// cardinality is checked here, because the declared type has no occurrence
/// indicator: neither the empty sequence nor a longer one matches `xs:string`.
fn description_argument<N: DomNavigator>(value: XPathValue<N>) -> Result<String, XPathError> {
    let atomized = atomize_sequence(value)?;
    if atomized.len() != 1 {
        return Err(XPathError::XPTY0004 {
            expected: "xs:string".to_string(),
            found: describe_count(atomized.len()),
        });
    }
    let item = atomized.into_iter().next().expect("length checked");
    super::atomize_to_string_strict(XPathValue::<N>::from_atomic(item))
}

/// The `$error` argument of `fn:error`, under the same rules.
///
/// `Ok(None)` is the empty sequence, which only the two- and three-argument
/// forms accept; the caller decides. `xs:QName` has no cast from
/// `xs:untypedAtomic` (XPath 2.0 §3.12.3 allows a cast to `xs:QName` only from
/// a string literal), so an argument that atomizes to anything but an
/// `xs:QName` is the closing type error of the rules.
fn error_qname_argument<N: DomNavigator>(
    value: XPathValue<N>,
    expected: &str,
) -> Result<Option<crate::namespace::QualifiedName>, XPathError> {
    let atomized = atomize_sequence(value)?;
    if atomized.len() > 1 {
        return Err(XPathError::XPTY0004 {
            expected: expected.to_string(),
            found: describe_count(atomized.len()),
        });
    }
    match atomized.into_iter().next() {
        None => Ok(None),
        Some(item) => match item.as_qname() {
            Some(qname) => Ok(Some(qname.clone())),
            None => Err(XPathError::XPTY0004 {
                expected: expected.to_string(),
                found: crate::xpath::type_info::type_code_to_name(item.type_code).to_string(),
            }),
        },
    }
}

/// The `found` half of an `XPTY0004` raised by a cardinality mismatch.
fn describe_count(count: usize) -> String {
    match count {
        0 => "empty-sequence()".to_string(),
        n => format!("a sequence of {n} items"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespace::table::NameTable;
    use crate::types::value::XmlValue;
    use crate::xpath::context::XPathContext;
    use crate::xpath::RoXmlNavigator;

    fn create_context<'a>(names: &'a NameTable) -> DynamicContext<'a, RoXmlNavigator<'a>> {
        let static_ctx = XPathContext::new(names);
        // Use Box::leak for tests only to get 'a lifetime
        let static_ctx = Box::leak(Box::new(static_ctx));
        DynamicContext::new(static_ctx, 0).with_position(3, 10)
    }

    #[test]
    fn test_position() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);
        ctx.context_position = 5;

        let result = position(&mut ctx, vec![]).unwrap();
        if let XPathValue::Item(XmlItem::Atomic(value)) = result {
            assert_eq!(
                value.as_integer().map(|i| i.to_string()),
                Some("5".to_string())
            );
        } else {
            panic!("Expected integer");
        }
    }

    #[test]
    fn test_last() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);
        ctx.context_size = 10;

        let result = last(&mut ctx, vec![]).unwrap();
        if let XPathValue::Item(XmlItem::Atomic(value)) = result {
            assert_eq!(
                value.as_integer().map(|i| i.to_string()),
                Some("10".to_string())
            );
        } else {
            panic!("Expected integer");
        }
    }

    #[test]
    fn test_default_collation() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        let result = default_collation(&mut ctx, vec![]).unwrap();
        if let XPathValue::Item(XmlItem::Atomic(value)) = result {
            assert_eq!(
                value.as_string(),
                Some(crate::xpath::collation::CODEPOINT_COLLATION_URI)
            );
        } else {
            panic!("Expected string");
        }
    }

    #[test]
    fn test_data_atomic() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        let input = XPathValue::string("hello");
        let result = data(&mut ctx, vec![input]).unwrap();

        if let XPathValue::Item(XmlItem::Atomic(value)) = result {
            assert_eq!(value.as_string(), Some("hello"));
        } else {
            panic!("Expected atomic value");
        }
    }

    #[test]
    fn test_data_sequence() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        let items = vec![
            XmlItem::Atomic(XmlValue::integer(1.into())),
            XmlItem::Atomic(XmlValue::integer(2.into())),
            XmlItem::Atomic(XmlValue::integer(3.into())),
        ];
        let input = XPathValue::Sequence(items);
        let result = data(&mut ctx, vec![input]).unwrap();

        match result {
            XPathValue::Sequence(items) => {
                assert_eq!(items.len(), 3);
            }
            _ => panic!("Expected sequence"),
        }
    }

    #[test]
    fn test_trace_passthrough() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        let input = XPathValue::string("test value");
        let label = XPathValue::string("debug");
        let result = trace(&mut ctx, vec![input, label]).unwrap();

        // trace returns the input unchanged
        if let XPathValue::Item(XmlItem::Atomic(value)) = result {
            assert_eq!(value.as_string(), Some("test value"));
        } else {
            panic!("Expected string");
        }
    }

    #[test]
    fn test_position_wrong_arity() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        let result = position(&mut ctx, vec![XPathValue::string("extra")]);
        assert!(result.is_err());
    }

    // =========================================================================
    // fn:error: the built-in, and a host registration the way the function's
    // rustdoc shows
    // =========================================================================

    mod host_registered_error {
        use crate::namespace::table::NameTable;
        use crate::xpath::api::XPathExpr;
        use crate::xpath::error::{XPathError, DEFAULT_RAISED_ERROR, XQT_ERRORS_NAMESPACE};
        use crate::xpath::functions::signature::types::{any, empty, qname, qname_opt, string};
        use crate::xpath::functions::{
            special, DynamicFunctionSignature, FunctionSet, FN_NAMESPACE,
        };
        use crate::xpath::{RoXmlNavigator, XPathContext};

        /// Register `fn:error` for arities 0 to 3, exactly as the function's
        /// own documentation tells a host to: one signature per arity, so that
        /// each declares the types F&O gives it.
        fn error_functions() -> FunctionSet<RoXmlNavigator<'static>> {
            let mut functions = FunctionSet::with_builtins();
            for params in [
                vec![],
                vec![qname()],
                vec![qname_opt(), string()],
                vec![qname_opt(), string(), any()],
            ] {
                functions.register(
                    DynamicFunctionSignature::new(FN_NAMESPACE, "error", params, empty()),
                    special::error,
                );
            }
            functions
        }

        /// A registration that declares nothing useful about the arguments —
        /// what a careless host might write. The implementation must enforce
        /// the real types anyway.
        fn loosely_registered_error_functions() -> FunctionSet<RoXmlNavigator<'static>> {
            let mut functions = FunctionSet::with_builtins();
            functions.register(
                DynamicFunctionSignature::range(
                    FN_NAMESPACE,
                    "error",
                    0,
                    3,
                    vec![any(), any(), any()],
                    empty(),
                ),
                special::error,
            );
            functions
        }

        /// Compile and evaluate `expr` against `functions`, and return the
        /// error it raises. Panics if the expression yields a value.
        fn raise_with(
            functions: &FunctionSet<RoXmlNavigator<'static>>,
            expr: &str,
            compat: bool,
        ) -> XPathError {
            let names = NameTable::new();
            // Bind `xs` so the tests can name built-in types the way a host
            // document would.
            let mut namespaces = crate::namespace::context::NamespaceContextSnapshot::default();
            namespaces.bindings.push((
                names.add("xs"),
                names.add("http://www.w3.org/2001/XMLSchema"),
            ));
            let ctx = XPathContext::new(&names)
                .with_namespaces(namespaces)
                .with_function_catalog(functions)
                .with_xpath10_compatibility(compat);
            let compiled = match XPathExpr::compile(expr, &ctx) {
                Ok(compiled) => compiled,
                Err(e) => return e,
            };
            let outcome = compiled
                .evaluator(&ctx)
                .run_with::<RoXmlNavigator<'static>, _>(|eval| {
                    eval.context().set_function_evaluator(Some(functions));
                });
            match outcome {
                Err(e) => e,
                Ok(_) => panic!("fn:error must never return a value: {expr}"),
            }
        }

        /// Compile and evaluate `expr` with `fn:error` registered, and return
        /// the error it raises. Panics if the expression yields a value.
        fn raise(expr: &str) -> XPathError {
            raise_with(&error_functions(), expr, false)
        }

        /// Since 0.3.0 `fn:error` is a built-in function: with no host
        /// registration it compiles at every arity it accepts and raises what
        /// the host-registered function raises.
        #[test]
        fn error_is_a_builtin_function() {
            let names = NameTable::new();
            let ctx = XPathContext::new(&names);
            let run = |expr: &str| -> XPathError {
                let compiled =
                    XPathExpr::compile(expr, &ctx).unwrap_or_else(|e| panic!("{expr}: {e}"));
                match compiled.evaluator(&ctx).run::<RoXmlNavigator<'static>>() {
                    Err(e) => e,
                    Ok(_) => panic!("fn:error must never return a value: {expr}"),
                }
            };
            assert_eq!(run("error()").error_code(), Some("FOER0000"));
            assert_eq!(run("error(())").error_code(), Some("XPTY0004"));
            let err = run("error((), 'boom')");
            assert_eq!(err.raised_error().unwrap().description, Some("boom"));
            let err = run("error(QName('urn:x', 'x:e'), 'boom', (42, 'a'))");
            let XPathError::Raised(info) = &err else {
                panic!("expected a raised error, got {err:?}");
            };
            assert_eq!((info.namespace_uri(), info.local_name()), ("urn:x", "e"));
            // The error object travels as atomic values.
            let object: Vec<String> = info
                .error_object()
                .iter()
                .map(|v| v.to_string_value())
                .collect();
            assert_eq!(object, ["42", "a"]);
            // Four arguments is no signature of the function.
            assert_eq!(
                XPathExpr::compile("error((), 'a', 1, 2)", &ctx)
                    .expect_err("arity 4")
                    .error_code(),
                Some("XPST0017")
            );
        }

        /// A node in the error object is carried by its string value, as
        /// `xs:untypedAtomic`.
        #[test]
        fn a_node_in_the_error_object_is_carried_by_its_string_value() {
            let names = NameTable::new();
            let ctx = XPathContext::new(&names);
            let doc = roxmltree::Document::parse("<a>x<b>y</b></a>").expect("parse xml");
            let Err(err) = XPathExpr::compile("error((), 'd', (/a, 1))", &ctx)
                .expect("compile")
                .evaluator(&ctx)
                .run_with_node(RoXmlNavigator::new(&doc))
            else {
                panic!("fn:error must never return a value");
            };
            let XPathError::Raised(info) = &err else {
                panic!("expected a raised error, got {err:?}");
            };
            let object: Vec<(String, crate::types::XmlTypeCode)> = info
                .error_object()
                .iter()
                .map(|v| (v.to_string_value(), v.type_code))
                .collect();
            assert_eq!(
                object,
                [
                    ("xy".to_string(), crate::types::XmlTypeCode::UntypedAtomic),
                    ("1".to_string(), crate::types::XmlTypeCode::Integer),
                ]
            );
        }

        /// A host that registers its own `fn:error` supersedes the built-in.
        #[test]
        fn a_host_registered_error_supersedes_the_built_in() {
            let mut functions: FunctionSet<RoXmlNavigator<'static>> = FunctionSet::with_builtins();
            functions.register(
                DynamicFunctionSignature::new(FN_NAMESPACE, "error", vec![], empty()),
                |_ctx, _args| Err(XPathError::raised("urn:host", "mine", None)),
            );
            let err = raise_with(&functions, "error()", false);
            assert_eq!(err.raised_error().unwrap().namespace_uri, "urn:host");
            // The arities the host did not register stay the built-in's.
            let err = raise_with(&functions, "error((), 'b')", false);
            assert_eq!(err.raised_error().unwrap().local_name, DEFAULT_RAISED_ERROR);
        }

        #[test]
        fn error_with_no_arguments_raises_the_default_qname() {
            let err = raise("error()");
            let raised = err.raised_error().expect("a raised error");
            assert_eq!(raised.namespace_uri, XQT_ERRORS_NAMESPACE);
            assert_eq!(raised.local_name, DEFAULT_RAISED_ERROR);
            // F&O gives the no-argument call the URI of its own error code as
            // the description.
            assert_eq!(
                raised.description,
                Some("http://www.w3.org/2005/xqt-errors#FOER0000")
            );
            assert_eq!(err.error_code(), Some("FOER0000"));
        }

        #[test]
        fn error_with_an_empty_qname_raises_the_default_qname() {
            let err = raise("error((), 'boom')");
            let raised = err.raised_error().expect("a raised error");
            assert_eq!(raised.namespace_uri, XQT_ERRORS_NAMESPACE);
            assert_eq!(raised.local_name, DEFAULT_RAISED_ERROR);
            assert_eq!(raised.description, Some("boom"));
            assert_eq!(err.error_code(), Some("FOER0000"));

            // …and so does the three-argument form.
            let err = raise("error((), 'boom', ())");
            assert_eq!(err.raised_error().unwrap().local_name, DEFAULT_RAISED_ERROR);
        }

        /// F&O gives the one-argument form `fn:error($error as xs:QName)` —
        /// no occurrence indicator — while only the two- and three-argument
        /// forms declare `xs:QName?`. So an empty `$error` alone is the
        /// closing type error of the function conversion rules (XPath 2.0
        /// §3.1.5), not the default `FOER0000`.
        #[test]
        fn a_lone_empty_error_qname_is_a_type_error() {
            let err = raise("error(())");
            assert_eq!(err.error_code(), Some("XPTY0004"));
            assert!(err.raised_error().is_none());
            assert!(err.to_string().contains("xs:QName"), "{err}");

            // The other two forms accept it, as their `?` says.
            assert_eq!(
                raise("error((), 'd')").raised_error().unwrap().local_name,
                DEFAULT_RAISED_ERROR
            );
        }

        /// `$description as xs:string` has no occurrence indicator either, and
        /// the function conversion rules atomize, cast `xs:untypedAtomic` and
        /// promote `xs:anyURI` — but nothing turns an `xs:integer` into a
        /// string.
        #[test]
        fn the_description_must_be_a_single_string() {
            for expr in [
                "error((), 42)",
                "error((), ())",
                "error((), (1, 2))",
                "error((), ('a', 'b'))",
                "error((), xs:date('2000-01-01'))",
                "error((), true())",
                "error(QName('u', 'e'), 42)",
                "error((), 42, 'obj')",
            ] {
                let err = raise(expr);
                assert_eq!(err.error_code(), Some("XPTY0004"), "{expr}: {err}");
                assert!(err.raised_error().is_none(), "{expr}");
            }

            // What the rules *do* convert still works: an xs:anyURI promotes
            // to xs:string, and an untyped node is cast to one.
            let err = raise("error((), xs:anyURI('http://example.com/'))");
            assert_eq!(
                err.raised_error().unwrap().description,
                Some("http://example.com/")
            );
        }

        /// A host may register `fn:error` with loose parameter types — the
        /// engine does not check a registered function's declared parameter
        /// types at evaluation time — so the implementation enforces them.
        #[test]
        fn the_argument_types_do_not_depend_on_the_registered_signature() {
            let loose = loosely_registered_error_functions();
            for expr in ["error(())", "error((), 42)", "error((), ())"] {
                let err = raise_with(&loose, expr, false);
                assert_eq!(err.error_code(), Some("XPTY0004"), "{expr}: {err}");
            }
            // And the calls that are well-typed still raise their error.
            let err = raise_with(&loose, "error((), 'boom')", false);
            assert_eq!(err.raised_error().unwrap().description, Some("boom"));
        }

        /// XPath 2.0 §3.1.5: in XPath 1.0 compatibility mode an argument that
        /// is *not* of the expected type is converted before the function
        /// body runs — `fn:string(42)` where `xs:string` is expected. So the
        /// very same call that is a type error under XPath 2.0 rules raises
        /// the error with a converted description under the flag.
        #[test]
        fn compatibility_mode_converts_the_description_instead_of_rejecting_it() {
            let functions = error_functions();

            let err = raise_with(&functions, "error((), 42)", true);
            let raised = err.raised_error().expect("a raised error");
            assert_eq!(raised.local_name, DEFAULT_RAISED_ERROR);
            assert_eq!(raised.description, Some("42"));

            // `fn:string(())` is the zero-length string.
            let err = raise_with(&functions, "error((), ())", true);
            assert_eq!(err.raised_error().unwrap().description, Some(""));

            // `$error` is xs:QName in the one-argument form, and none of the
            // three compatibility steps produces one, so it stays a type
            // error with the flag on.
            let err = raise_with(&functions, "error(())", true);
            assert_eq!(err.error_code(), Some("XPTY0004"), "{err}");

            // With the flag off, the same three calls behave as XPath 2.0
            // says: two type errors and one type error.
            assert_eq!(
                raise_with(&functions, "error((), 42)", false).error_code(),
                Some("XPTY0004")
            );
            assert_eq!(
                raise_with(&functions, "error((), ())", false).error_code(),
                Some("XPTY0004")
            );
            assert_eq!(
                raise_with(&functions, "error(())", false).error_code(),
                Some("XPTY0004")
            );
        }

        #[test]
        fn error_carries_the_supplied_qname_and_description() {
            let err = raise(
                "error(QName('http://example.com/errors', 'e:myError'), 'something went wrong')",
            );
            let raised = err.raised_error().expect("a raised error");
            assert_eq!(raised.namespace_uri, "http://example.com/errors");
            assert_eq!(raised.local_name, "myError");
            assert_eq!(raised.description, Some("something went wrong"));
            // Not the default QName, so there is no 'static code for it.
            assert_eq!(err.error_code(), None);

            // An error QName in no namespace keeps an empty namespace URI.
            let err = raise("error(QName('', 'bare'), 'd')");
            let raised = err.raised_error().expect("a raised error");
            assert_eq!(raised.namespace_uri, "");
            assert_eq!(raised.local_name, "bare");
        }

        #[test]
        fn error_accepts_and_discards_the_error_object() {
            let err = raise("error((), 'boom', 42)");
            let raised = err.raised_error().expect("a raised error");
            assert_eq!(raised.local_name, DEFAULT_RAISED_ERROR);
            assert_eq!(raised.description, Some("boom"));

            // …whatever the error object is.
            let err = raise("error((), 'boom', (1, 2, 3))");
            assert_eq!(err.raised_error().unwrap().description, Some("boom"));
        }

        #[test]
        fn error_rejects_a_first_argument_that_is_not_a_qname() {
            // $error is xs:QName / xs:QName?, and nothing but a QName (or, in
            // the two- and three-argument forms, the empty sequence) matches
            // it: xs:QName has no cast from xs:untypedAtomic or xs:string.
            let err = raise("error('application error')");
            assert_eq!(err.error_code(), Some("XPTY0004"));
            assert!(err.raised_error().is_none());

            let err = raise("error(42, 'd')");
            assert_eq!(err.error_code(), Some("XPTY0004"));

            // More than one item does not match either occurrence indicator.
            let err = raise("error((QName('u', 'a'), QName('u', 'b')), 'd')");
            assert_eq!(err.error_code(), Some("XPTY0004"), "{err}");
        }

        #[test]
        fn error_accepts_no_more_than_three_arguments() {
            // The registration covers arities 0 to 3 only.
            let err = raise("error((), 'a', (), ())");
            assert_eq!(err.error_code(), Some("XPST0017"));
        }
    }
}
