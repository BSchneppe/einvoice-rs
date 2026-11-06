//! High-level, ergonomic XPath evaluation API.
//!
//! This module provides a user-friendly interface for compiling and evaluating
//! XPath expressions with Rust-idiomatic patterns:
//!
//! - **Separation of compilation and execution**: Compile once, evaluate many times
//! - **Builder pattern**: Fluent API for setting variables and options
//! - **`From` traits**: Ergonomic value conversion (use `42` instead of `XPathValue::integer(42)`)
//! - **No lifetimes in compiled expressions**: Store `XPathExpr` anywhere
//!
//! # Example
//!
//! ```no_run
//! use xsd_schema::xpath::api::{XPathExpr, EvalValue};
//! use xsd_schema::xpath::{XPathContext, RoXmlNavigator};
//! use xsd_schema::namespace::table::NameTable;
//!
//! // Setup context
//! let names = NameTable::new();
//! let ctx = XPathContext::new(&names);
//!
//! // Compile once with external variables
//! let expr = XPathExpr::compile_with_vars("$x + $y", &ctx, &["x", "y"]).unwrap();
//!
//! // Evaluate with fluent builder
//! let result = expr.evaluator(&ctx)
//!     .with_variable("x", 10).unwrap()
//!     .with_variable("y", 32).unwrap()
//!     .run::<RoXmlNavigator<'static>>().unwrap();
//!
//! // Convenience methods for common return types
//! let sum = expr.evaluator(&ctx)
//!     .with_variable("x", 20).unwrap()
//!     .with_variable("y", 22).unwrap()
//!     .run_number::<RoXmlNavigator<'static>>().unwrap();
//! assert_eq!(sum, 42.0);
//! ```

use std::cell::RefCell;

use num_bigint::BigInt;

use crate::namespace::qname::QualifiedName;

use super::arena::{AstArena, AstNodeId, SourceSpan};
use super::bind::bind_node;
use super::context::{DynamicContext, NameBinder, VarSlotId, XPathContext};
use super::deps::{analyze, ExprDependencies, FunctionCallRef};
use super::error::{XPathError, XPathWarning};
use super::eval::eval_node;
use super::functions::{effective_boolean_value, XPathValue};
use super::iterator::XmlItem;
use super::parser::{parse, parse_with_mode};
use super::regex_cache::RegexCache;
use super::DomNavigator;

// ============================================================================
// ExternalVar - Information about a declared external variable
// ============================================================================

/// Information about an external variable declared at compile time.
///
/// External variables are those declared by the user (via `compile_with_vars`)
/// as opposed to variables introduced by the expression itself (like `for $x in ...`).
#[derive(Debug, Clone)]
pub struct ExternalVar {
    /// The qualified name of the variable
    pub name: QualifiedName,
    /// The slot ID for storing the variable's value
    pub slot: VarSlotId,
}

// ============================================================================
// XPathExpr - Compiled XPath expression (owns its AST)
// ============================================================================

/// A compiled XPath expression that can be evaluated multiple times.
///
/// `XPathExpr` owns its AST and contains no lifetimes, so it can be stored
/// in structs, sent across threads (if using appropriate synchronization),
/// or cached for repeated evaluation.
///
/// # Compilation vs Evaluation
///
/// Compilation (`compile()` or `compile_with_vars()`) parses and binds the expression,
/// resolving function names, variable slots, and namespace prefixes. This is the
/// expensive step.
///
/// Evaluation (`evaluator().run()`) executes the compiled AST, which is much faster.
/// You can evaluate the same compiled expression many times with different variable
/// values or context nodes.
///
/// # External Variables
///
/// Use `compile_with_vars()` to declare variables that will be provided at evaluation time:
///
/// ```no_run
/// # use xsd_schema::xpath::api::XPathExpr;
/// # use xsd_schema::xpath::XPathContext;
/// # use xsd_schema::namespace::table::NameTable;
/// let names = NameTable::new();
/// let ctx = XPathContext::new(&names);
///
/// // Declare $x and $y as external variables
/// let expr = XPathExpr::compile_with_vars("$x + $y", &ctx, &["x", "y"]).unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct XPathExpr {
    /// Original source expression
    source: String,
    /// The AST arena containing all nodes
    arena: AstArena,
    /// Root node ID of the expression
    root: AstNodeId,
    /// Source span of the entire expression
    span: SourceSpan,
    /// Total number of variable slots needed
    var_slots: usize,
    /// External variables declared at compile time
    external_vars: Vec<ExternalVar>,
    /// Compile-time dependency metadata (referenced externals, focus use,
    /// function calls). Computed once after binding; see [`super::deps`].
    deps: ExprDependencies,
}

impl XPathExpr {
    /// Compile an XPath expression without external variables.
    ///
    /// Use this when your expression doesn't reference any variables, or when
    /// all variables are provided by the expression itself (e.g., `for $x in 1 to 10`).
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The expression has syntax errors
    /// - A function is not found
    /// - A variable is referenced but not defined
    /// - A namespace prefix is not bound
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use xsd_schema::xpath::api::XPathExpr;
    /// # use xsd_schema::xpath::XPathContext;
    /// # use xsd_schema::namespace::table::NameTable;
    /// let names = NameTable::new();
    /// let ctx = XPathContext::new(&names);
    /// let expr = XPathExpr::compile("1 + 2 * 3", &ctx).unwrap();
    /// ```
    pub fn compile(expr: &str, ctx: &XPathContext<'_>) -> Result<Self, XPathError> {
        Self::compile_with_vars(expr, ctx, &[])
    }

    /// Compile an XPath expression with declared external variables.
    ///
    /// External variables must be provided at evaluation time via `with_variable()`.
    /// Variable names should be provided without the `$` prefix.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The expression has syntax errors
    /// - A function is not found
    /// - A variable is referenced that wasn't declared
    /// - A namespace prefix is not bound
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use xsd_schema::xpath::api::XPathExpr;
    /// # use xsd_schema::xpath::XPathContext;
    /// # use xsd_schema::namespace::table::NameTable;
    /// let names = NameTable::new();
    /// let ctx = XPathContext::new(&names);
    ///
    /// // Compile expression that uses $x and $y
    /// let expr = XPathExpr::compile_with_vars("$x + $y", &ctx, &["x", "y"]).unwrap();
    /// ```
    pub fn compile_with_vars(
        expr: &str,
        ctx: &XPathContext<'_>,
        vars: &[&str],
    ) -> Result<Self, XPathError> {
        // Parse the expression using the mode from context (ParseError → XPathError via From)
        let parsed = if ctx.mode() == super::XPathMode::XPath20 {
            parse(expr)?
        } else {
            parse_with_mode(expr, ctx.mode())?
        };

        let mut arena = parsed.arena;
        let root = parsed.root;
        let span = parsed.span;

        // Create name binder and push external variables
        let mut binder = NameBinder::new();
        let mut external_vars = Vec::with_capacity(vars.len());

        for var_name in vars {
            // Parse variable name - support both "local" and "prefix:local" formats
            let qname = parse_variable_name(var_name, ctx)?;

            // Check for duplicate variable declarations
            if external_vars.iter().any(|v: &ExternalVar| v.name == qname) {
                return Err(XPathError::XPST0003 {
                    message: format!("Duplicate external variable declaration: ${}", var_name),
                });
            }

            let var_ref = binder.push_var(qname.clone());
            external_vars.push(ExternalVar {
                name: qname,
                slot: var_ref.slot,
            });
        }

        // Mark boundary between external vars and expression-internal vars
        binder.mark_external_boundary();

        // Bind the expression (resolves functions, variables, namespaces)
        bind_node(&mut arena, root, ctx, &mut binder)?;

        let var_slots = binder.len();

        // External slots are allocated first, so they occupy 0..count and no
        // expression-internal (for/some/every) variable can collide with them.
        let external_slot_count = external_vars.len();
        debug_assert!(external_vars
            .iter()
            .enumerate()
            .all(|(i, v)| v.slot as usize == i));

        // Compile-time only: collect referenced externals, focus use and the
        // function-call inventory in a single post-bind walk.
        let deps = analyze(&arena, root, ctx, external_slot_count);

        Ok(Self {
            source: expr.to_string(),
            arena,
            root,
            span,
            var_slots,
            external_vars,
            deps,
        })
    }

    /// Get the original source expression.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Get the source span of the expression.
    pub fn span(&self) -> SourceSpan {
        self.span
    }

    /// Get the external variables declared for this expression.
    pub fn external_vars(&self) -> &[ExternalVar] {
        &self.external_vars
    }

    /// The external variables this expression actually **references**.
    ///
    /// A subset of [`external_vars()`](Self::external_vars), which keeps
    /// listing every name *supplied* to
    /// [`compile_with_vars`](Self::compile_with_vars). Variables the
    /// expression introduces itself (`for $i in …`, `some $i in …`) are never
    /// reported, and a binding that shadows a declared external does not mark
    /// that external as referenced.
    ///
    /// Callers that bind variable values per evaluation can use this to do
    /// O(referenced) work instead of O(in-scope).
    ///
    /// # Example
    ///
    /// ```
    /// # use xsd_schema::xpath::api::XPathExpr;
    /// # use xsd_schema::xpath::XPathContext;
    /// # use xsd_schema::namespace::table::NameTable;
    /// let names = NameTable::new();
    /// let ctx = XPathContext::new(&names);
    /// let expr = XPathExpr::compile_with_vars("$a + 1", &ctx, &["a", "b", "c"]).unwrap();
    ///
    /// // All three stay declared …
    /// assert_eq!(expr.external_vars().len(), 3);
    /// // … but only $a is referenced.
    /// assert_eq!(expr.referenced_external_vars().count(), 1);
    /// let slot_of_a = expr.external_vars()[0].slot;
    /// let slot_of_b = expr.external_vars()[1].slot;
    /// assert!(expr.references_external_var(slot_of_a));
    /// assert!(!expr.references_external_var(slot_of_b));
    /// ```
    pub fn referenced_external_vars(&self) -> impl Iterator<Item = &ExternalVar> + '_ {
        self.external_vars
            .iter()
            .filter(|var| self.deps.references_var(var.slot))
    }

    /// Whether the external variable in `slot` is referenced by this
    /// expression. O(1) — backed by a bitset over the external slots.
    ///
    /// Returns `false` for any slot that is not an external variable of this
    /// expression (including slots of its own `for`/`some`/`every` variables).
    /// See [`referenced_external_vars()`](Self::referenced_external_vars) for
    /// an example.
    pub fn references_external_var(&self, slot: VarSlotId) -> bool {
        self.deps.references_var(slot)
    }

    /// Whether evaluation reads the **initial** context item — the focus in
    /// effect when the expression is entered (XPath 2.0 §2.1.2).
    ///
    /// Foci created *inside* the expression do not count: a predicate is
    /// evaluated with the item being tested as its context item (§3.2.2), and
    /// every path step after the first is evaluated against the items of the
    /// previous step (§3.2). `for`, `some`/`every`, `if`, function arguments,
    /// operators and comma sequences keep the enclosing focus.
    ///
    /// `false` means an evaluator may skip setting up a context item
    /// altogether. A call to a custom (catalog-registered) function is
    /// conservatively treated as reading the context item, since this crate
    /// cannot inspect its body.
    ///
    /// # Example
    ///
    /// ```
    /// # use xsd_schema::xpath::api::XPathExpr;
    /// # use xsd_schema::xpath::XPathContext;
    /// # use xsd_schema::namespace::table::NameTable;
    /// let names = NameTable::new();
    /// let ctx = XPathContext::new(&names);
    ///
    /// // A relative first step reads the context node …
    /// let expr = XPathExpr::compile_with_vars("a[position() = 1]", &ctx, &[]).unwrap();
    /// assert!(expr.uses_focus());
    /// // … but the predicate runs in an inner focus.
    /// assert!(!expr.uses_position());
    ///
    /// // Filtering a variable needs no focus at all.
    /// let expr = XPathExpr::compile_with_vars("$x[position() = 1]", &ctx, &["x"]).unwrap();
    /// assert!(!expr.uses_focus());
    /// assert!(!expr.uses_position());
    /// ```
    pub fn uses_focus(&self) -> bool {
        self.deps.uses_focus()
    }

    /// Whether `fn:position()` is evaluated against the **initial** focus.
    ///
    /// `position()` inside a predicate or a non-first path step refers to that
    /// inner focus and is not reported. Implies
    /// [`uses_focus()`](Self::uses_focus), since the context position is part
    /// of the focus (§2.1.2).
    ///
    /// # Example
    ///
    /// ```
    /// # use xsd_schema::xpath::api::XPathExpr;
    /// # use xsd_schema::xpath::XPathContext;
    /// # use xsd_schema::namespace::table::NameTable;
    /// let names = NameTable::new();
    /// let ctx = XPathContext::new(&names);
    /// let expr = XPathExpr::compile_with_vars("position() = 1", &ctx, &[]).unwrap();
    /// assert!(expr.uses_position());
    /// assert!(expr.uses_focus());
    /// ```
    pub fn uses_position(&self) -> bool {
        self.deps.uses_position()
    }

    /// Whether `fn:last()` is evaluated against the **initial** focus.
    ///
    /// As with [`uses_position()`](Self::uses_position), a `last()` inside a
    /// predicate or a later path step belongs to that inner focus:
    /// `count(//a[last()])` reports `uses_last() == false` (and
    /// `uses_focus() == true`, because `//` starts from the root of the context
    /// node).
    ///
    /// # Example
    ///
    /// ```
    /// # use xsd_schema::xpath::api::XPathExpr;
    /// # use xsd_schema::xpath::XPathContext;
    /// # use xsd_schema::namespace::table::NameTable;
    /// let names = NameTable::new();
    /// let ctx = XPathContext::new(&names);
    /// let expr = XPathExpr::compile_with_vars("last()", &ctx, &[]).unwrap();
    /// assert!(expr.uses_last());
    ///
    /// let expr = XPathExpr::compile_with_vars("count(//a[last()])", &ctx, &[]).unwrap();
    /// assert!(!expr.uses_last());
    /// assert!(expr.uses_focus());
    /// ```
    pub fn uses_last(&self) -> bool {
        self.deps.uses_last()
    }

    /// The distinct functions called anywhere in this expression, including
    /// inside predicates, `for` bodies and other nested foci.
    ///
    /// One [`FunctionCallRef`] per distinct `(namespace, local_name, arity)`
    /// triple, in the order the calls were first seen. The namespace is the one
    /// the call actually **bound** to (the resolved function's own namespace),
    /// so an unprefixed `count(...)` is reported in the `fn:` namespace in
    /// XPath 1.0 mode as well. Constructor functions (`xs:integer(…)`) are
    /// rewritten into `cast as` expressions during binding and are therefore
    /// not function calls here.
    ///
    /// This is the hook for deciding impurity yourself: look for `fn:doc`,
    /// `fn:collection`, `fn:unparsed-text`, a host language's `document()` or
    /// `key()`, and so on.
    ///
    /// # Example
    ///
    /// ```
    /// # use xsd_schema::xpath::api::XPathExpr;
    /// # use xsd_schema::xpath::XPathContext;
    /// # use xsd_schema::namespace::table::NameTable;
    /// const FN: &str = "http://www.w3.org/2005/xpath-functions";
    /// let names = NameTable::new();
    /// let ctx = XPathContext::new(&names);
    /// let expr =
    ///     XPathExpr::compile_with_vars("count(distinct-values($s)) + string-length()", &ctx, &["s"])
    ///         .unwrap();
    ///
    /// let calls: Vec<(&str, &str, usize)> = expr
    ///     .function_calls()
    ///     .iter()
    ///     .map(|c| (c.namespace.as_str(), c.local_name.as_str(), c.arity))
    ///     .collect();
    /// assert_eq!(
    ///     calls,
    ///     vec![
    ///         (FN, "count", 1),
    ///         (FN, "distinct-values", 1),
    ///         (FN, "string-length", 0),
    ///     ]
    /// );
    /// ```
    pub fn function_calls(&self) -> &[FunctionCallRef] {
        self.deps.function_calls()
    }

    /// Borrow the bound AST arena of this expression.
    ///
    /// Useful for callers that want to inspect the compiled tree
    /// (e.g. CTA schema-time validation walking type expressions).
    pub fn arena(&self) -> &AstArena {
        &self.arena
    }

    /// Create an evaluator for this expression.
    ///
    /// The evaluator uses a builder pattern to set variables and other options
    /// before running the expression.
    ///
    /// `ctx` should be the static context the expression was compiled with, or
    /// one with the same name table and schema set: binding resolved the
    /// expression's names and schema types against those (a cast target or an
    /// `instance of` type is kept as a key of that schema set), and evaluation
    /// reads them against the schema set of `ctx`.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use xsd_schema::xpath::api::XPathExpr;
    /// # use xsd_schema::xpath::{XPathContext, RoXmlNavigator};
    /// # use xsd_schema::namespace::table::NameTable;
    /// let names = NameTable::new();
    /// let ctx = XPathContext::new(&names);
    /// let expr = XPathExpr::compile_with_vars("$x * 2", &ctx, &["x"]).unwrap();
    ///
    /// let result = expr.evaluator(&ctx)
    ///     .with_variable("x", 21).unwrap()
    ///     .run_number::<RoXmlNavigator<'static>>().unwrap();
    /// assert_eq!(result, 42.0);
    /// ```
    pub fn evaluator<'a, 'ctx>(
        &'a self,
        ctx: &'ctx XPathContext<'ctx>,
    ) -> XPathEvaluator<'a, 'ctx> {
        XPathEvaluator::new(self, ctx)
    }
}

// ============================================================================
// EvalValue - Ergonomic value type for variable binding
// ============================================================================

/// A value that can be bound to an XPath variable at evaluation time.
///
/// This enum provides ergonomic conversion from Rust types to XPath values
/// via the `From` trait implementations. You can use Rust literals directly:
///
/// ```no_run
/// # use xsd_schema::xpath::api::{XPathExpr, EvalValue};
/// # use xsd_schema::xpath::XPathContext;
/// # use xsd_schema::namespace::table::NameTable;
/// # let names = NameTable::new();
/// # let ctx = XPathContext::new(&names);
/// # let expr = XPathExpr::compile_with_vars("$x", &ctx, &["x"]).unwrap();
/// // All of these work:
/// expr.evaluator(&ctx).with_variable("x", 42);          // i32 -> Integer
/// expr.evaluator(&ctx).with_variable("x", 3.14);        // f64 -> Double
/// expr.evaluator(&ctx).with_variable("x", true);        // bool -> Bool
/// expr.evaluator(&ctx).with_variable("x", "hello");     // &str -> String
/// ```
#[derive(Debug, Clone)]
pub enum EvalValue {
    /// Boolean value
    Bool(bool),
    /// Small integer (converted to BigInt internally)
    Integer(i64),
    /// Big integer
    BigInteger(BigInt),
    /// Double-precision floating point
    Double(f64),
    /// String value
    String(String),
}

impl From<bool> for EvalValue {
    fn from(b: bool) -> Self {
        EvalValue::Bool(b)
    }
}

impl From<i32> for EvalValue {
    fn from(i: i32) -> Self {
        EvalValue::Integer(i as i64)
    }
}

impl From<i64> for EvalValue {
    fn from(i: i64) -> Self {
        EvalValue::Integer(i)
    }
}

impl From<f32> for EvalValue {
    fn from(f: f32) -> Self {
        EvalValue::Double(f as f64)
    }
}

impl From<f64> for EvalValue {
    fn from(f: f64) -> Self {
        EvalValue::Double(f)
    }
}

impl From<String> for EvalValue {
    fn from(s: String) -> Self {
        EvalValue::String(s)
    }
}

impl From<&str> for EvalValue {
    fn from(s: &str) -> Self {
        EvalValue::String(s.to_string())
    }
}

impl From<BigInt> for EvalValue {
    fn from(i: BigInt) -> Self {
        EvalValue::BigInteger(i)
    }
}

// ============================================================================
// PendingValue - Internal type for deferred XPathValue construction
// ============================================================================

/// Internal representation of a value waiting to be converted to XPathValue<N>.
///
/// Since XPathValue is generic over the navigator type N, and we don't know N
/// until `run()` is called, we store values in this intermediate form.
#[derive(Debug, Clone)]
enum PendingValue {
    Bool(bool),
    Integer(i64),
    BigInteger(BigInt),
    Double(f64),
    String(String),
}

impl PendingValue {
    /// Convert this pending value to an XPathValue for the given navigator type.
    fn into_xpath_value<N: DomNavigator>(self) -> XPathValue<N> {
        match self {
            PendingValue::Bool(b) => XPathValue::boolean(b),
            PendingValue::Integer(i) => XPathValue::integer(BigInt::from(i)),
            PendingValue::BigInteger(i) => XPathValue::integer(i),
            PendingValue::Double(d) => XPathValue::double(d),
            PendingValue::String(s) => XPathValue::string(s),
        }
    }
}

impl From<EvalValue> for PendingValue {
    fn from(v: EvalValue) -> Self {
        match v {
            EvalValue::Bool(b) => PendingValue::Bool(b),
            EvalValue::Integer(i) => PendingValue::Integer(i),
            EvalValue::BigInteger(i) => PendingValue::BigInteger(i),
            EvalValue::Double(d) => PendingValue::Double(d),
            EvalValue::String(s) => PendingValue::String(s),
        }
    }
}

// ============================================================================
// XPathEvaluator - Builder for expression evaluation
// ============================================================================

/// Builder for evaluating a compiled XPath expression.
///
/// Use this to set variables, context nodes, and other evaluation options
/// before running the expression. The builder pattern allows fluent API usage:
///
/// ```no_run
/// # use xsd_schema::xpath::api::XPathExpr;
/// # use xsd_schema::xpath::{XPathContext, RoXmlNavigator};
/// # use xsd_schema::namespace::table::NameTable;
/// # let names = NameTable::new();
/// # let ctx = XPathContext::new(&names);
/// # let expr = XPathExpr::compile_with_vars("$x + $y", &ctx, &["x", "y"]).unwrap();
/// let result = expr.evaluator(&ctx)
///     .with_variable("x", 10).unwrap()
///     .with_variable("y", 32).unwrap()
///     .run::<RoXmlNavigator<'static>>().unwrap();
/// ```
pub struct XPathEvaluator<'expr, 'ctx> {
    /// The compiled expression to evaluate
    expr: &'expr XPathExpr,
    /// The static context
    static_ctx: &'ctx XPathContext<'ctx>,
    /// Variables to set before evaluation (slot -> value)
    pending_vars: Vec<(VarSlotId, PendingValue)>,
    /// The host's cache of compiled regular expressions, lent to the run.
    regex_cache: Option<&'ctx RegexCache>,
    /// Where the run's warnings go; see [`with_warnings`](Self::with_warnings).
    warnings: Option<&'ctx RefCell<Vec<XPathWarning>>>,
}

impl<'expr, 'ctx> XPathEvaluator<'expr, 'ctx> {
    /// Create a new evaluator for the given expression and context.
    fn new(expr: &'expr XPathExpr, static_ctx: &'ctx XPathContext<'ctx>) -> Self {
        Self {
            expr,
            static_ctx,
            pending_vars: Vec::new(),
            regex_cache: None,
            warnings: None,
        }
    }

    /// Set an external variable's value.
    ///
    /// The variable must have been declared when compiling the expression
    /// (via `compile_with_vars()`). Variable names should not include the `$` prefix.
    ///
    /// # Errors
    ///
    /// Returns `XPST0008` if the variable was not declared at compile time.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use xsd_schema::xpath::api::XPathExpr;
    /// # use xsd_schema::xpath::XPathContext;
    /// # use xsd_schema::namespace::table::NameTable;
    /// # let names = NameTable::new();
    /// # let ctx = XPathContext::new(&names);
    /// let expr = XPathExpr::compile_with_vars("$price * $qty", &ctx, &["price", "qty"]).unwrap();
    ///
    /// let eval = expr.evaluator(&ctx)
    ///     .with_variable("price", 19.99).unwrap()
    ///     .with_variable("qty", 3).unwrap();
    /// ```
    pub fn with_variable(
        mut self,
        name: &str,
        value: impl Into<EvalValue>,
    ) -> Result<Self, XPathError> {
        let slot = find_external_var(name, &self.expr.external_vars, self.static_ctx)?;
        self.pending_vars.push((slot, value.into().into()));
        Ok(self)
    }

    /// Lend the run a cache of compiled regular expressions that the host keeps
    /// across runs.
    ///
    /// A run compiles each distinct pattern of `fn:matches`, `fn:replace` and
    /// `fn:tokenize` once and keeps it for as long as the run lasts. A host that
    /// runs the same expression many times — once per item, once per node —
    /// keeps a [`RegexCache`] instead and passes it to every run, so that each
    /// pattern is compiled once for the life of the cache. See [`RegexCache`]
    /// for an example; inside a [`run_with`](Self::run_with) callback,
    /// [`DynamicContext::set_regex_cache`] does the same (and, being applied
    /// later, wins).
    pub fn with_regex_cache(mut self, cache: &'ctx RegexCache) -> Self {
        self.regex_cache = Some(cache);
        self
    }

    /// Collect the run's warnings ([`XPathWarning`]) into `warnings`, after
    /// the run, whether it succeeded or failed; without a collector they are
    /// dropped. Each site of the expression warns once per run.
    ///
    /// ```
    /// use std::cell::RefCell;
    /// use xsd_schema::namespace::context::NamespaceContextSnapshot;
    /// use xsd_schema::namespace::table::NameTable;
    /// use xsd_schema::xpath::{RoXmlNavigator, XPathContext, XPathExpr};
    ///
    /// let names = NameTable::new();
    /// let mut namespaces = NamespaceContextSnapshot::default();
    /// namespaces.bindings.push((names.add("xs"), names.add("http://www.w3.org/2001/XMLSchema")));
    /// let ctx = XPathContext::new(&names).with_namespaces(namespaces);
    /// // 59 seconds and a fraction of 28 digits: 30 digits, more than the
    /// // seconds field holds; the fraction keeps 27 digits.
    /// let expr = XPathExpr::compile(
    ///     "xs:dayTimeDuration('PT0.1234567890123456789012345678S') + xs:dayTimeDuration('PT59S')",
    ///     &ctx,
    /// )
    /// .unwrap();
    /// let warnings = RefCell::new(Vec::new());
    /// let value = expr
    ///     .evaluator(&ctx)
    ///     .with_warnings(&warnings)
    ///     .run_string::<RoXmlNavigator<'static>>()
    ///     .unwrap();
    /// assert_eq!(value, "PT59.123456789012345678901234567S");
    /// assert_eq!(warnings.borrow()[0].code(), "xs-seconds-capacity");
    /// ```
    pub fn with_warnings(mut self, warnings: &'ctx RefCell<Vec<XPathWarning>>) -> Self {
        self.warnings = Some(warnings);
        self
    }

    /// Evaluate the expression and return the full result.
    ///
    /// This is the most flexible method, returning the raw `XPathValue` which
    /// can be empty, a single item, or a sequence.
    ///
    /// # Type Parameter
    ///
    /// - `N`: The navigator type (e.g., `RoXmlNavigator<'doc>`)
    ///
    /// # Errors
    ///
    /// Returns an error if evaluation fails (e.g., type errors, undefined context).
    pub fn run<N: DomNavigator>(self) -> Result<XPathValue<N>, XPathError> {
        self.run_internal(None)
    }

    /// Evaluate the expression with a context node.
    ///
    /// Sets the context item to the given node before evaluation. This is
    /// necessary for expressions that use `.` or axis steps like `child::*`.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use xsd_schema::xpath::api::XPathExpr;
    /// # use xsd_schema::xpath::{XPathContext, RoXmlNavigator, DomNavigator};
    /// # use xsd_schema::namespace::table::NameTable;
    /// # let names = NameTable::new();
    /// # let ctx = XPathContext::new(&names);
    /// let expr = XPathExpr::compile("child::item", &ctx).unwrap();
    ///
    /// // Parse some XML and get a navigator
    /// let doc = roxmltree::Document::parse("<root><item/></root>").unwrap();
    /// let mut nav = RoXmlNavigator::new(&doc);
    /// nav.move_to_first_child(); // move to <root>
    ///
    /// let result = expr.evaluator(&ctx)
    ///     .run_with_node(nav).unwrap();
    /// ```
    pub fn run_with_node<N: DomNavigator>(self, node: N) -> Result<XPathValue<N>, XPathError> {
        self.run_internal(Some(node))
    }

    /// Internal evaluation implementation.
    fn run_internal<N: DomNavigator>(
        self,
        context_node: Option<N>,
    ) -> Result<XPathValue<N>, XPathError> {
        // Create dynamic context
        let mut dyn_ctx = DynamicContext::new(self.static_ctx, self.expr.var_slots);
        dyn_ctx.set_regex_cache(self.regex_cache);

        // Set context node if provided
        if let Some(node) = context_node {
            dyn_ctx = dyn_ctx.with_context_node(node);
        }

        // Set pending variables
        for (slot, pending) in self.pending_vars {
            let value: XPathValue<N> = pending.into_xpath_value();
            dyn_ctx.set_variable(slot, value);
        }

        // Evaluate the expression
        let result = eval_node(&self.expr.arena, self.expr.root, &mut dyn_ctx);
        if let Some(warnings) = self.warnings {
            warnings.borrow_mut().extend(dyn_ctx.take_warnings());
        }
        result
    }

    /// Evaluate and return the result as a boolean.
    ///
    /// Uses the XPath effective boolean value rules:
    /// - Empty sequence → `false`
    /// - Boolean → its value
    /// - String → `false` if empty, `true` otherwise
    /// - Number → `false` if 0 or NaN, `true` otherwise
    /// - Node sequence → `true` if non-empty
    ///
    /// # Errors
    ///
    /// Returns an error if evaluation fails or if effective boolean value
    /// cannot be computed (e.g., sequence of multiple atomic values).
    pub fn run_bool<N: DomNavigator>(self) -> Result<bool, XPathError> {
        let value = self.run::<N>()?;
        effective_boolean_value(&value)
    }

    /// Evaluate and return the result as a string.
    ///
    /// Atomizes the result and converts to string. For sequences, returns
    /// the string value of the first item (or empty string for empty sequence).
    ///
    /// # Errors
    ///
    /// Returns an error if evaluation fails.
    pub fn run_string<N: DomNavigator>(self) -> Result<String, XPathError> {
        let value = self.run::<N>()?;
        Ok(xpath_value_to_string(&value))
    }

    /// Evaluate and return the result as a number (f64).
    ///
    /// Atomizes the result and converts to double. Returns `NaN` for
    /// values that cannot be converted to numbers.
    ///
    /// # Errors
    ///
    /// Returns an error if evaluation fails.
    pub fn run_number<N: DomNavigator>(self) -> Result<f64, XPathError> {
        let value = self.run::<N>()?;
        Ok(xpath_value_to_number(&value))
    }

    /// Evaluate and return the result as a vector of nodes.
    ///
    /// Filters the result to include only nodes, discarding atomic values.
    /// Useful for path expressions that return node sequences.
    ///
    /// # Errors
    ///
    /// Returns an error if evaluation fails.
    pub fn run_nodes<N: DomNavigator>(self) -> Result<Vec<N>, XPathError> {
        let value = self.run::<N>()?;
        let items = value.into_vec();
        let nodes = items
            .into_iter()
            .filter_map(|item| match item {
                XmlItem::Node(n) => Some(n),
                XmlItem::Atomic(_) => None,
            })
            .collect();
        Ok(nodes)
    }

    /// Evaluate with a setup callback for advanced variable binding.
    ///
    /// This method allows binding variables that cannot be represented as `EvalValue`,
    /// such as:
    /// - Node values
    /// - Sequences of items
    /// - Empty sequences
    ///
    /// The setup callback receives a mutable reference to the `DynamicContext`
    /// and can use `set_variable_by_name()` or `context.set_variable()` directly.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use xsd_schema::xpath::api::XPathExpr;
    /// # use xsd_schema::xpath::{XPathContext, RoXmlNavigator, XPathValue};
    /// # use xsd_schema::namespace::table::NameTable;
    /// # let names = NameTable::new();
    /// # let ctx = XPathContext::new(&names);
    /// let expr = XPathExpr::compile_with_vars("count($items)", &ctx, &["items"]).unwrap();
    ///
    /// // Bind a sequence of integers
    /// let result = expr.evaluator(&ctx)
    ///     .run_with::<RoXmlNavigator<'static>, _>(|eval| {
    ///         // Create a sequence value
    ///         let seq = XPathValue::from_sequence(vec![
    ///             xsd_schema::xpath::XmlItem::Atomic(xsd_schema::types::XmlValue::integer(1.into())),
    ///             xsd_schema::xpath::XmlItem::Atomic(xsd_schema::types::XmlValue::integer(2.into())),
    ///             xsd_schema::xpath::XmlItem::Atomic(xsd_schema::types::XmlValue::integer(3.into())),
    ///         ]);
    ///         eval.set_variable_by_name("items", seq).unwrap();
    ///     })
    ///     .unwrap();
    /// ```
    ///
    /// # Callback lifetimes
    ///
    /// Only the `&mut` borrow of the [`TypedEvaluator`] is higher-ranked; the expression
    /// and static-context lifetimes are this evaluator's own `'expr` and `'ctx`. That is
    /// what lets the callback install borrowed state that lives in the caller's frame —
    /// e.g. [`DynamicContext::set_extension`] or
    /// [`DynamicContext::set_function_evaluator`] with a local — instead of only
    /// `'static` data.
    pub fn run_with<N, F>(self, setup: F) -> Result<XPathValue<N>, XPathError>
    where
        N: DomNavigator + 'ctx,
        F: for<'d> FnOnce(&mut TypedEvaluator<'expr, 'ctx, 'd, N>),
    {
        self.run_with_node_and_setup(None, setup)
    }

    /// Evaluate with a context node and setup callback for advanced variable binding.
    ///
    /// Combines `run_with_node` and `run_with` functionality. See
    /// [`run_with`](Self::run_with) for the callback's lifetimes.
    pub fn run_with_node_and_setup<N, F>(
        self,
        context_node: Option<N>,
        setup: F,
    ) -> Result<XPathValue<N>, XPathError>
    where
        N: DomNavigator + 'ctx,
        F: for<'d> FnOnce(&mut TypedEvaluator<'expr, 'ctx, 'd, N>),
    {
        // Create dynamic context
        let mut dyn_ctx = DynamicContext::new(self.static_ctx, self.expr.var_slots);
        dyn_ctx.set_regex_cache(self.regex_cache);

        // Set context node if provided
        if let Some(node) = context_node {
            dyn_ctx = dyn_ctx.with_context_node(node);
        }

        // Set pending variables (from with_variable calls)
        for (slot, pending) in self.pending_vars {
            let value: XPathValue<N> = pending.into_xpath_value();
            dyn_ctx.set_variable(slot, value);
        }

        // Create typed evaluator and run setup callback
        {
            let mut typed_eval = TypedEvaluator {
                expr: self.expr,
                static_ctx: self.static_ctx,
                dyn_ctx: &mut dyn_ctx,
            };
            setup(&mut typed_eval);
        } // typed_eval dropped here, releasing the borrow

        // Evaluate the expression
        let result = eval_node(&self.expr.arena, self.expr.root, &mut dyn_ctx);
        if let Some(warnings) = self.warnings {
            warnings.borrow_mut().extend(dyn_ctx.take_warnings());
        }
        result
    }
}

// ============================================================================
// TypedEvaluator - For advanced variable binding with known navigator type
// ============================================================================

/// A typed evaluator that allows binding arbitrary `XPathValue<N>` values.
///
/// This is used within `run_with` callbacks to set variables that cannot be
/// represented as simple `EvalValue` (like nodes or sequences).
pub struct TypedEvaluator<'expr, 'ctx, 'dyn_ctx, N: DomNavigator> {
    expr: &'expr XPathExpr,
    static_ctx: &'ctx XPathContext<'ctx>,
    dyn_ctx: &'dyn_ctx mut DynamicContext<'ctx, N>,
}

impl<'expr, 'ctx, 'dyn_ctx, N: DomNavigator> TypedEvaluator<'expr, 'ctx, 'dyn_ctx, N> {
    /// Set a variable by name to an arbitrary XPath value.
    ///
    /// This allows binding nodes, sequences, empty sequences, or any other
    /// `XPathValue<N>` to an external variable.
    ///
    /// # Errors
    ///
    /// Returns `XPST0008` if the variable was not declared at compile time.
    pub fn set_variable_by_name(
        &mut self,
        name: &str,
        value: XPathValue<N>,
    ) -> Result<(), XPathError> {
        let slot = find_external_var(name, &self.expr.external_vars, self.static_ctx)?;
        self.dyn_ctx.set_variable(slot, value);
        Ok(())
    }

    /// Set a variable by slot ID directly.
    ///
    /// Use this when you already know the slot ID (e.g., from `ExternalVar::slot`).
    pub fn set_variable(&mut self, slot: VarSlotId, value: XPathValue<N>) {
        self.dyn_ctx.set_variable(slot, value);
    }

    /// Get a reference to the dynamic context for advanced manipulation.
    pub fn context(&mut self) -> &mut DynamicContext<'ctx, N> {
        &mut *self.dyn_ctx
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Parse a variable name string into a QualifiedName.
///
/// Supports both simple names ("x") and prefixed names ("prefix:local").
/// The prefix must be bound in the static context.
fn parse_variable_name(name: &str, ctx: &XPathContext<'_>) -> Result<QualifiedName, XPathError> {
    if let Some(colon_pos) = name.find(':') {
        // Prefixed name: "prefix:local"
        let prefix = &name[..colon_pos];
        let local = &name[colon_pos + 1..];

        if prefix.is_empty() || local.is_empty() {
            return Err(XPathError::XPST0003 {
                message: format!("Invalid variable name: '{}'", name),
            });
        }

        let prefix_id = ctx.names.add(prefix);
        let local_id = ctx.names.add(local);

        // Resolve prefix to namespace
        let ns_id = ctx
            .resolve_prefix_id(prefix_id)
            .ok_or_else(|| XPathError::undefined_prefix(prefix))?;

        Ok(QualifiedName::new(Some(ns_id), local_id, Some(prefix_id)))
    } else {
        // Simple name: "x"
        let local_id = ctx.names.add(name);
        Ok(QualifiedName::local(local_id))
    }
}

/// Find an external variable by name, searching from the end to match binder resolution.
///
/// Returns the slot ID if found, or an error if not declared.
fn find_external_var(
    name: &str,
    external_vars: &[ExternalVar],
    ctx: &XPathContext<'_>,
) -> Result<VarSlotId, XPathError> {
    let qname = parse_variable_name(name, ctx)?;

    // Search from the end to match the binder's last-in-first-out resolution
    external_vars
        .iter()
        .rev()
        .find(|v| v.name == qname)
        .map(|v| v.slot)
        .ok_or_else(|| XPathError::XPST0008 {
            qname: format!("${}", name),
        })
}

/// Convert an XPathValue to a string.
fn xpath_value_to_string<N: DomNavigator>(value: &XPathValue<N>) -> String {
    match value {
        XPathValue::Empty => String::new(),
        XPathValue::Item(item) => item_to_string(item),
        XPathValue::Sequence(items) => {
            if let Some(first) = items.first() {
                item_to_string(first)
            } else {
                String::new()
            }
        }
    }
}

/// Convert an XmlItem to a string.
fn item_to_string<N: DomNavigator>(item: &XmlItem<N>) -> String {
    match item {
        XmlItem::Node(nav) => nav.value(),
        XmlItem::Atomic(val) => val.to_string_value(),
    }
}

/// Convert an XPathValue to a number.
fn xpath_value_to_number<N: DomNavigator>(value: &XPathValue<N>) -> f64 {
    match value {
        XPathValue::Empty => f64::NAN,
        XPathValue::Item(item) => item_to_number(item),
        XPathValue::Sequence(items) => {
            if let Some(first) = items.first() {
                item_to_number(first)
            } else {
                f64::NAN
            }
        }
    }
}

/// Convert an XmlItem to a number.
fn item_to_number<N: DomNavigator>(item: &XmlItem<N>) -> f64 {
    match item {
        XmlItem::Node(nav) => nav.value().trim().parse().unwrap_or(f64::NAN),
        XmlItem::Atomic(val) => {
            if let Some(d) = val.as_double() {
                d
            } else if let Some(i) = val.as_integer() {
                // Convert BigInt to f64 (may lose precision for very large numbers)
                i.to_string().parse().unwrap_or(f64::NAN)
            } else {
                // Try string conversion
                val.to_string_value().trim().parse().unwrap_or(f64::NAN)
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespace::table::NameTable;
    use crate::xpath::RoXmlNavigator;

    #[test]
    fn test_compile_simple_expression() {
        let names = NameTable::new();
        let ctx = XPathContext::new(&names);

        let expr = XPathExpr::compile("1 + 2", &ctx);
        assert!(expr.is_ok());

        let expr = expr.unwrap();
        assert_eq!(expr.source(), "1 + 2");
        assert!(expr.external_vars().is_empty());
    }

    #[test]
    fn test_compile_with_variables() {
        let names = NameTable::new();
        let ctx = XPathContext::new(&names);

        let expr = XPathExpr::compile_with_vars("$x + $y", &ctx, &["x", "y"]);
        assert!(expr.is_ok());

        let expr = expr.unwrap();
        assert_eq!(expr.external_vars().len(), 2);
    }

    #[test]
    fn test_eval_simple_arithmetic() {
        let names = NameTable::new();
        let ctx = XPathContext::new(&names);

        let expr = XPathExpr::compile("1 + 2", &ctx).unwrap();
        let result = expr
            .evaluator(&ctx)
            .run_number::<RoXmlNavigator<'static>>()
            .unwrap();

        assert_eq!(result, 3.0);
    }

    #[test]
    fn test_eval_with_variable() {
        let names = NameTable::new();
        let ctx = XPathContext::new(&names);

        let expr = XPathExpr::compile_with_vars("$x + 1", &ctx, &["x"]).unwrap();
        let result = expr
            .evaluator(&ctx)
            .with_variable("x", 41)
            .unwrap()
            .run_number::<RoXmlNavigator<'static>>()
            .unwrap();

        assert_eq!(result, 42.0);
    }

    #[test]
    fn test_eval_with_string_variable() {
        let names = NameTable::new();
        let ctx = XPathContext::new(&names);

        let expr = XPathExpr::compile_with_vars(
            "concat($greeting, ' ', $name)",
            &ctx,
            &["greeting", "name"],
        )
        .unwrap();
        let result = expr
            .evaluator(&ctx)
            .with_variable("greeting", "Hello")
            .unwrap()
            .with_variable("name", "World")
            .unwrap()
            .run_string::<RoXmlNavigator<'static>>()
            .unwrap();

        assert_eq!(result, "Hello World");
    }

    #[test]
    fn test_eval_run_bool() {
        let names = NameTable::new();
        let ctx = XPathContext::new(&names);

        let expr = XPathExpr::compile("1 < 2", &ctx).unwrap();
        let result = expr
            .evaluator(&ctx)
            .run_bool::<RoXmlNavigator<'static>>()
            .unwrap();
        assert!(result);

        let expr = XPathExpr::compile("2 < 1", &ctx).unwrap();
        let result = expr
            .evaluator(&ctx)
            .run_bool::<RoXmlNavigator<'static>>()
            .unwrap();
        assert!(!result);
    }

    #[test]
    fn test_eval_run_number() {
        let names = NameTable::new();
        let ctx = XPathContext::new(&names);

        let expr = XPathExpr::compile("2.5", &ctx).unwrap();
        let result = expr
            .evaluator(&ctx)
            .run_number::<RoXmlNavigator<'static>>()
            .unwrap();
        assert!((result - 2.5).abs() < 0.001);
    }

    #[test]
    fn test_undefined_variable_error() {
        let names = NameTable::new();
        let ctx = XPathContext::new(&names);

        // Try to compile expression with undefined variable
        let result = XPathExpr::compile("$x", &ctx);
        assert!(result.is_err());

        if let Err(XPathError::XPST0008 { qname }) = result {
            assert!(qname.contains("x"));
        } else {
            panic!("Expected XPST0008 error");
        }
    }

    #[test]
    fn test_setting_undeclared_variable_error() {
        let names = NameTable::new();
        let ctx = XPathContext::new(&names);

        // Compile with only $x declared
        let expr = XPathExpr::compile_with_vars("$x", &ctx, &["x"]).unwrap();

        // Try to set undeclared variable $y
        let result = expr
            .evaluator(&ctx)
            .with_variable("x", 1)
            .unwrap()
            .with_variable("y", 2); // $y was not declared

        assert!(result.is_err());
        if let Err(XPathError::XPST0008 { qname }) = result {
            assert!(qname.contains("y"));
        } else {
            panic!("Expected XPST0008 error");
        }
    }

    #[test]
    fn test_eval_with_context_node() {
        let names = NameTable::new();
        let ctx = XPathContext::new(&names);

        let expr = XPathExpr::compile("child::item", &ctx).unwrap();

        let doc = roxmltree::Document::parse("<root><item>value</item></root>").unwrap();
        let mut nav = RoXmlNavigator::new(&doc);
        nav.move_to_first_child(); // move to <root>

        let result = expr.evaluator(&ctx).run_with_node(nav).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_expr_is_clone() {
        let names = NameTable::new();
        let ctx = XPathContext::new(&names);

        let expr1 = XPathExpr::compile("1 + 2", &ctx).unwrap();
        let expr2 = expr1.clone();

        // Both should evaluate to the same result
        let result1 = expr1
            .evaluator(&ctx)
            .run_number::<RoXmlNavigator<'static>>()
            .unwrap();
        let result2 = expr2
            .evaluator(&ctx)
            .run_number::<RoXmlNavigator<'static>>()
            .unwrap();

        assert_eq!(result1, result2);
    }

    #[test]
    fn test_eval_value_conversions() {
        // Test that all From implementations work
        let _: EvalValue = true.into();
        let _: EvalValue = 42i32.into();
        let _: EvalValue = 42i64.into();
        let _: EvalValue = 2.5f32.into();
        let _: EvalValue = 2.5f64.into();
        let _: EvalValue = "hello".into();
        let _: EvalValue = String::from("hello").into();
        let _: EvalValue = BigInt::from(1000000000000i64).into();
    }

    #[test]
    fn test_multiple_evaluations_same_expr() {
        let names = NameTable::new();
        let ctx = XPathContext::new(&names);

        let expr = XPathExpr::compile_with_vars("$x * 2", &ctx, &["x"]).unwrap();

        // Evaluate multiple times with different values
        let result1 = expr
            .evaluator(&ctx)
            .with_variable("x", 5)
            .unwrap()
            .run_number::<RoXmlNavigator<'static>>()
            .unwrap();

        let result2 = expr
            .evaluator(&ctx)
            .with_variable("x", 10)
            .unwrap()
            .run_number::<RoXmlNavigator<'static>>()
            .unwrap();

        let result3 = expr
            .evaluator(&ctx)
            .with_variable("x", 21)
            .unwrap()
            .run_number::<RoXmlNavigator<'static>>()
            .unwrap();

        assert_eq!(result1, 10.0);
        assert_eq!(result2, 20.0);
        assert_eq!(result3, 42.0);
    }

    #[test]
    fn test_duplicate_variable_error() {
        let names = NameTable::new();
        let ctx = XPathContext::new(&names);

        // Duplicate variable names should cause an error
        let result = XPathExpr::compile_with_vars("$x + $x", &ctx, &["x", "x"]);
        assert!(result.is_err());

        if let Err(XPathError::XPST0003 { message }) = result {
            assert!(message.contains("Duplicate"));
        } else {
            panic!("Expected XPST0003 error for duplicate variable");
        }
    }

    #[test]
    fn test_prefixed_variable() {
        let names = NameTable::new();

        // Create a context with a namespace binding
        let my_ns = names.add("http://example.com/my");
        let my_prefix = names.add("my");

        let mut namespaces = crate::namespace::context::NamespaceContextSnapshot::default();
        namespaces.bindings.push((my_prefix, my_ns));

        let ctx = XPathContext::new(&names).with_namespaces(namespaces);

        // Compile with a prefixed variable
        let expr = XPathExpr::compile_with_vars("$my:value + 1", &ctx, &["my:value"]).unwrap();

        // Set the prefixed variable
        let result = expr
            .evaluator(&ctx)
            .with_variable("my:value", 41)
            .unwrap()
            .run_number::<RoXmlNavigator<'static>>()
            .unwrap();

        assert_eq!(result, 42.0);
    }

    #[test]
    fn test_run_with_sequence() {
        use crate::types::XmlValue;

        let names = NameTable::new();
        let ctx = XPathContext::new(&names);

        let expr = XPathExpr::compile_with_vars("count($items)", &ctx, &["items"]).unwrap();

        // Use run_with to bind a sequence
        let result = expr
            .evaluator(&ctx)
            .run_with::<RoXmlNavigator<'static>, _>(|eval| {
                let seq = XPathValue::from_sequence(vec![
                    XmlItem::Atomic(XmlValue::integer(1.into())),
                    XmlItem::Atomic(XmlValue::integer(2.into())),
                    XmlItem::Atomic(XmlValue::integer(3.into())),
                ]);
                eval.set_variable_by_name("items", seq).unwrap();
            })
            .unwrap();

        // count() should return 3
        assert_eq!(
            result.as_integer().map(|i| i.to_string()),
            Some("3".to_string())
        );
    }

    #[test]
    fn test_run_with_empty_sequence() {
        let names = NameTable::new();
        let ctx = XPathContext::new(&names);

        let expr = XPathExpr::compile_with_vars("empty($items)", &ctx, &["items"]).unwrap();

        // Use run_with to bind an empty sequence
        let result = expr
            .evaluator(&ctx)
            .run_with::<RoXmlNavigator<'static>, _>(|eval| {
                eval.set_variable_by_name("items", XPathValue::empty())
                    .unwrap();
            })
            .unwrap();

        // empty() should return true for empty sequence
        assert_eq!(result.as_bool(), Some(true));
    }

    #[test]
    fn test_a_lent_regex_cache_outlives_the_run() {
        let names = NameTable::new();
        let ctx = XPathContext::new(&names);
        let expr = XPathExpr::compile_with_vars(r"matches($s, '^\p{Lu}')", &ctx, &["s"]).unwrap();

        let regexes = RegexCache::new();
        for i in 0..100 {
            let word = if i % 2 == 0 { "Upper" } else { "lower" };
            let result = expr
                .evaluator(&ctx)
                .with_regex_cache(&regexes)
                .with_variable("s", word)
                .unwrap()
                .run_bool::<RoXmlNavigator<'static>>()
                .unwrap();
            assert_eq!(result, i % 2 == 0);
        }
        // One compilation for a hundred runs.
        assert_eq!(regexes.compiles(), 1);
        assert_eq!(regexes.hits(), 99);
        assert_eq!(regexes.len(), 1);
    }

    #[test]
    fn test_the_setup_callback_has_the_last_word_on_the_regex_cache() {
        let names = NameTable::new();
        let ctx = XPathContext::new(&names);
        let expr = XPathExpr::compile("matches('abc', 'b')", &ctx).unwrap();

        let builder_cache = RegexCache::new();
        let callback_cache = RegexCache::new();
        let result = expr
            .evaluator(&ctx)
            .with_regex_cache(&builder_cache)
            .run_with::<RoXmlNavigator<'static>, _>(|eval| {
                eval.context().set_regex_cache(Some(&callback_cache));
            })
            .unwrap();
        assert_eq!(result.as_bool(), Some(true));
        assert!(builder_cache.is_empty());
        assert_eq!(callback_cache.len(), 1);

        // `None` goes back to the run's own cache: neither host cache is touched.
        let result = expr
            .evaluator(&ctx)
            .with_regex_cache(&builder_cache)
            .run_with::<RoXmlNavigator<'static>, _>(|eval| {
                eval.context().set_regex_cache(None);
            })
            .unwrap();
        assert_eq!(result.as_bool(), Some(true));
        assert!(builder_cache.is_empty());
        assert_eq!(callback_cache.compiles(), 1);
        assert_eq!(callback_cache.hits(), 0);
    }

    #[test]
    fn test_a_nested_run_shares_the_lent_regex_cache() {
        use crate::namespace::context::NamespaceContextSnapshot;
        use crate::types::sequence::SequenceType;
        use crate::xpath::functions::{DynamicFunctionSignature, FunctionSet};

        // `ext:inner()` evaluates another expression, with the cache its own
        // run was lent, while the outer run is still in progress.
        let mut functions: FunctionSet<RoXmlNavigator<'static>> = FunctionSet::with_builtins();
        functions.register(
            DynamicFunctionSignature::new(
                "http://example.com/ext",
                "inner",
                vec![],
                SequenceType::boolean(),
            ),
            |ctx, _args| {
                let cache = ctx.host_regex_cache().expect("the outer run has a cache");
                let inner = XPathExpr::compile("matches('xbx', 'b')", ctx.static_context)?;
                let found = inner
                    .evaluator(ctx.static_context)
                    .with_regex_cache(cache)
                    .run_bool::<RoXmlNavigator<'static>>()?;
                Ok(XPathValue::boolean(found))
            },
        );

        let names = NameTable::new();
        let mut namespaces = NamespaceContextSnapshot::default();
        namespaces
            .bindings
            .push((names.add("ext"), names.add("http://example.com/ext")));
        let ctx = XPathContext::new(&names)
            .with_namespaces(namespaces)
            .with_function_catalog(&functions);
        let expr = XPathExpr::compile("matches('abc', 'b') and ext:inner()", &ctx).unwrap();

        let regexes = RegexCache::new();
        let result = expr
            .evaluator(&ctx)
            .with_regex_cache(&regexes)
            .run_with::<RoXmlNavigator<'static>, _>(|eval| {
                eval.context().set_function_evaluator(Some(&functions));
            })
            .unwrap();
        assert_eq!(result.as_bool(), Some(true));
        // The inner run found the outer run's program.
        assert_eq!(regexes.compiles(), 1);
        assert_eq!(regexes.hits(), 1);
    }
}
