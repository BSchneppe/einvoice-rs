//! Extensible function support for XPath 2.0.
//!
//! This module provides traits and types for extending the XPath function library
//! with custom user-defined functions.
//!
//! ## Architecture
//!
//! The extensibility system uses two main traits:
//! - `FunctionCatalog` - Bind-time function lookup by namespace/name/arity
//! - `FunctionEvaluator` - Eval-time function dispatch
//!
//! The `FunctionHandle` type provides an opaque reference to functions that works
//! for both built-in and custom functions.
//!
//! ## Usage
//!
//! For most use cases, the default `BuiltinCatalog` and `BuiltinEvaluator` provide
//! access to all standard XPath 2.0 functions. To add custom functions, use `FunctionSet`:
//!
//! ```text
//! let mut functions = FunctionSet::with_builtins();
//! functions.register(
//!     DynamicFunctionSignature { ... },
//!     |ctx, args| { /* implementation */ }
//! );
//! ```

use std::sync::Arc;

use super::signature::{FunctionArity, FunctionSignature};
use super::{eval_function, FunctionId, XPathValue, FUNCTION_REGISTRY};
use crate::types::sequence::SequenceType;
use crate::xpath::context::DynamicContext;
use crate::xpath::error::XPathError;
use crate::xpath::DomNavigator;

// ============================================================================
// FunctionHandle - Opaque identifier for function dispatch
// ============================================================================

/// Base value for custom function handles (built-in handles use FunctionId values).
const CUSTOM_HANDLE_BASE: u32 = 0x1000_0000;

/// Opaque handle for function dispatch.
///
/// Replaces `FunctionId` in the AST to support both built-in and custom functions.
/// Built-in functions use handles with values matching their `FunctionId` discriminant.
/// Custom functions use handles starting at `CUSTOM_HANDLE_BASE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FunctionHandle(pub(crate) u32);

impl FunctionHandle {
    /// Check if this handle refers to a built-in function.
    #[inline]
    pub fn is_builtin(&self) -> bool {
        self.0 < CUSTOM_HANDLE_BASE
    }

    /// Check if this handle refers to a custom function.
    #[inline]
    pub fn is_custom(&self) -> bool {
        self.0 >= CUSTOM_HANDLE_BASE
    }

    /// Get the custom function index (only valid for custom handles).
    #[inline]
    pub(crate) fn custom_index(&self) -> Option<usize> {
        if self.is_custom() {
            Some((self.0 - CUSTOM_HANDLE_BASE) as usize)
        } else {
            None
        }
    }
}

impl From<FunctionId> for FunctionHandle {
    fn from(id: FunctionId) -> Self {
        FunctionHandle(id as u32)
    }
}

// ============================================================================
// DynamicFunctionSignature - Owned signature for external registration
// ============================================================================

/// Function signature with owned strings for external registration.
///
/// Unlike `FunctionSignature` which uses `&'static str` for built-in functions,
/// this type owns its strings, allowing dynamic registration of custom functions.
#[derive(Debug, Clone)]
pub struct DynamicFunctionSignature {
    /// The function namespace URI.
    pub namespace: Arc<str>,
    /// The local name of the function.
    pub local_name: Arc<str>,
    /// The arity specification.
    pub arity: FunctionArity,
    /// Parameter types (may be shorter than actual args for variadic functions).
    pub param_types: Vec<SequenceType>,
    /// Return type.
    pub return_type: SequenceType,
}

impl DynamicFunctionSignature {
    /// Create a new dynamic signature with exact arity.
    pub fn new(
        namespace: impl Into<Arc<str>>,
        local_name: impl Into<Arc<str>>,
        param_types: Vec<SequenceType>,
        return_type: SequenceType,
    ) -> Self {
        let arity = FunctionArity::Exact(param_types.len());
        Self {
            namespace: namespace.into(),
            local_name: local_name.into(),
            arity,
            param_types,
            return_type,
        }
    }

    /// Create a variadic function signature.
    pub fn variadic(
        namespace: impl Into<Arc<str>>,
        local_name: impl Into<Arc<str>>,
        min_args: usize,
        param_types: Vec<SequenceType>,
        return_type: SequenceType,
    ) -> Self {
        Self {
            namespace: namespace.into(),
            local_name: local_name.into(),
            arity: FunctionArity::Variadic(min_args),
            param_types,
            return_type,
        }
    }

    /// Create a function signature with range arity.
    pub fn range(
        namespace: impl Into<Arc<str>>,
        local_name: impl Into<Arc<str>>,
        min_args: usize,
        max_args: usize,
        param_types: Vec<SequenceType>,
        return_type: SequenceType,
    ) -> Self {
        Self {
            namespace: namespace.into(),
            local_name: local_name.into(),
            arity: FunctionArity::Range(min_args, max_args),
            param_types,
            return_type,
        }
    }

    /// Check if this signature matches the given arity.
    pub fn matches_arity(&self, count: usize) -> bool {
        self.arity.matches(count)
    }
}

impl From<&FunctionSignature> for DynamicFunctionSignature {
    fn from(sig: &FunctionSignature) -> Self {
        Self {
            namespace: Arc::from(sig.namespace),
            local_name: Arc::from(sig.local_name),
            arity: sig.arity,
            param_types: sig.param_types.clone(),
            return_type: sig.return_type.clone(),
        }
    }
}

// ============================================================================
// FunctionCatalog - Bind-time function lookup trait
// ============================================================================

/// Trait for bind-time function lookup.
///
/// Implementors provide function resolution by namespace, local name, and arity.
/// The returned `FunctionHandle` is stored in the AST for later evaluation.
pub trait FunctionCatalog: std::fmt::Debug {
    /// Look up a function by namespace URI, local name, and arity.
    ///
    /// Returns `Some(handle)` if a matching function is found, `None` otherwise.
    fn lookup(&self, namespace: &str, local_name: &str, arity: usize) -> Option<FunctionHandle>;

    /// Get the signature for a function handle.
    ///
    /// Returns `None` if the handle is invalid.
    fn get_signature(&self, handle: FunctionHandle) -> Option<DynamicFunctionSignature>;
}

// ============================================================================
// FunctionEvaluator - Eval-time function dispatch trait
// ============================================================================

/// Trait for eval-time function dispatch.
///
/// Implementors execute functions identified by `FunctionHandle`.
pub trait FunctionEvaluator<N: DomNavigator> {
    /// Evaluate a function with the given arguments.
    ///
    /// # Arguments
    /// * `handle` - The function handle from bind-time lookup
    /// * `ctx` - The dynamic evaluation context
    /// * `args` - The evaluated argument values
    ///
    /// # Errors
    /// Returns an error if the function execution fails.
    fn eval(
        &self,
        handle: FunctionHandle,
        ctx: &mut DynamicContext<'_, N>,
        args: Vec<XPathValue<N>>,
    ) -> Result<XPathValue<N>, XPathError>;
}

// ============================================================================
// BuiltinCatalog - Static wrapper for FUNCTION_REGISTRY
// ============================================================================

/// Catalog wrapper for the static `FUNCTION_REGISTRY`.
///
/// Provides `FunctionCatalog` implementation using only built-in XPath functions.
/// This is the default catalog when no custom functions are registered.
#[derive(Debug, Clone, Copy, Default)]
pub struct BuiltinCatalog;

impl FunctionCatalog for BuiltinCatalog {
    fn lookup(&self, namespace: &str, local_name: &str, arity: usize) -> Option<FunctionHandle> {
        FUNCTION_REGISTRY
            .lookup(namespace, local_name, arity)
            .map(|entry| FunctionHandle::from(entry.id))
    }

    fn get_signature(&self, handle: FunctionHandle) -> Option<DynamicFunctionSignature> {
        if !handle.is_builtin() {
            return None;
        }
        // Convert handle back to FunctionId and look up
        FUNCTION_REGISTRY
            .by_id(handle_to_function_id(handle).ok()?)
            .map(|entry| DynamicFunctionSignature::from(&entry.signature))
    }
}

// ============================================================================
// BuiltinEvaluator - Static wrapper for eval_function
// ============================================================================

/// Evaluator wrapper for the static `eval_function` dispatch.
///
/// Provides `FunctionEvaluator` implementation using only built-in XPath functions.
/// This is the default evaluator when no custom functions are registered.
#[derive(Debug, Clone, Copy, Default)]
pub struct BuiltinEvaluator;

impl<N: DomNavigator> FunctionEvaluator<N> for BuiltinEvaluator {
    fn eval(
        &self,
        handle: FunctionHandle,
        ctx: &mut DynamicContext<'_, N>,
        args: Vec<XPathValue<N>>,
    ) -> Result<XPathValue<N>, XPathError> {
        let id = handle_to_function_id(handle)?;
        eval_function(id, ctx, args)
    }
}

// ============================================================================
// Helper functions
// ============================================================================

/// Convert a FunctionHandle back to a FunctionId.
///
/// Returns an error if the handle is not a valid built-in function handle.
pub(crate) fn handle_to_function_id(handle: FunctionHandle) -> Result<FunctionId, XPathError> {
    if !handle.is_builtin() {
        return Err(XPathError::Internal(format!(
            "Cannot convert custom handle {:?} to FunctionId",
            handle
        )));
    }

    // The handle value is the FunctionId discriminant
    // We need to find the matching FunctionId
    let value = handle.0 as u16;

    // Use the registry to find the function with matching ID
    // This is safe because we only create builtin handles from FunctionIds
    let entry = FUNCTION_REGISTRY
        .by_id_value(value)
        .ok_or_else(|| XPathError::Internal(format!("Invalid function handle: {}", value)))?;

    Ok(entry.id)
}

// ============================================================================
// FunctionSet - Combined catalog and evaluator with custom function support
// ============================================================================

use std::collections::HashMap;

/// Type alias for custom function implementation.
///
/// Custom functions receive the dynamic context and evaluated arguments,
/// and return an XPath value or error.
pub type CustomFn<N> = Arc<
    dyn Fn(&mut DynamicContext<'_, N>, Vec<XPathValue<N>>) -> Result<XPathValue<N>, XPathError>
        + Send
        + Sync,
>;

/// Entry for a custom function in FunctionSet.
struct CustomFunctionEntry<N: DomNavigator> {
    signature: DynamicFunctionSignature,
    implementation: CustomFn<N>,
}

/// A function set that combines built-in and custom functions.
///
/// `FunctionSet<N>` implements both `FunctionCatalog` and `FunctionEvaluator<N>`,
/// allowing users to register custom XPath functions alongside the built-in ones.
///
/// ## Example
///
/// ```text
/// use xsd_schema::xpath::functions::{FunctionSet, DynamicFunctionSignature, XPathValue};
/// use xsd_schema::types::sequence::SequenceType;
///
/// let mut functions = FunctionSet::with_builtins();
///
/// // Register a custom function
/// let sig = DynamicFunctionSignature::new(
///     "http://example.com/ext",
///     "my-upper",
///     vec![SequenceType::string()],
///     SequenceType::string(),
/// );
///
/// functions.register(sig, |_ctx, mut args| {
///     let s = args.remove(0);
///     // ... implementation
///     Ok(XPathValue::string("RESULT"))
/// });
/// ```
pub struct FunctionSet<N: DomNavigator> {
    /// Custom function entries (indexed by custom handle offset)
    custom_functions: Vec<CustomFunctionEntry<N>>,
    /// Lookup map: (namespace, local_name, arity) -> handle
    lookup: HashMap<(Arc<str>, Arc<str>, usize), FunctionHandle>,
    /// Variadic lookup: (namespace, local_name) -> (handle, min_arity)
    variadic_lookup: HashMap<(Arc<str>, Arc<str>), (FunctionHandle, usize)>,
}

impl<N: DomNavigator> FunctionSet<N> {
    /// Create an empty function set with no built-in functions.
    ///
    /// Use `with_builtins()` to include standard XPath 2.0 functions.
    pub fn new() -> Self {
        Self {
            custom_functions: Vec::new(),
            lookup: HashMap::new(),
            variadic_lookup: HashMap::new(),
        }
    }

    /// Create a function set with all built-in XPath 2.0 functions.
    ///
    /// Built-in functions are looked up via the global `FUNCTION_REGISTRY`.
    /// Custom functions registered with `register()` will take precedence
    /// over built-in functions with the same signature.
    pub fn with_builtins() -> Self {
        // We don't need to populate the lookup maps with builtins;
        // we fall back to FUNCTION_REGISTRY for those.
        Self::new()
    }

    /// Register a custom function.
    ///
    /// The function will be available for lookup by its namespace, local name,
    /// and arity. If a function with the same signature already exists (either
    /// built-in or previously registered), the new function takes precedence.
    ///
    /// Returns the `FunctionHandle` for the registered function.
    ///
    /// ## Example
    ///
    /// ```text
    /// let sig = DynamicFunctionSignature::new(
    ///     "http://example.com/ext",
    ///     "double",
    ///     vec![SequenceType::double()],
    ///     SequenceType::double(),
    /// );
    ///
    /// functions.register(sig, |_ctx, mut args| {
    ///     let val = args.remove(0);
    ///     let d = val.as_f64().unwrap_or(0.0);
    ///     Ok(XPathValue::double(d * 2.0))
    /// });
    /// ```
    pub fn register<F>(
        &mut self,
        signature: DynamicFunctionSignature,
        implementation: F,
    ) -> FunctionHandle
    where
        F: Fn(&mut DynamicContext<'_, N>, Vec<XPathValue<N>>) -> Result<XPathValue<N>, XPathError>
            + Send
            + Sync
            + 'static,
    {
        let index = self.custom_functions.len();
        let handle = FunctionHandle(CUSTOM_HANDLE_BASE + index as u32);

        // Register in lookup maps based on arity
        let ns = signature.namespace.clone();
        let local = signature.local_name.clone();

        match signature.arity {
            FunctionArity::Exact(n) => {
                self.lookup.insert((ns.clone(), local.clone(), n), handle);
            }
            FunctionArity::Range(min, max) => {
                for arity in min..=max {
                    self.lookup
                        .insert((ns.clone(), local.clone(), arity), handle);
                }
            }
            FunctionArity::Variadic(min) => {
                self.variadic_lookup
                    .insert((ns.clone(), local.clone()), (handle, min));
            }
        }

        // Store the entry
        self.custom_functions.push(CustomFunctionEntry {
            signature,
            implementation: Arc::new(implementation),
        });

        handle
    }

    /// Get the number of custom functions registered.
    pub fn custom_count(&self) -> usize {
        self.custom_functions.len()
    }

    /// Check if this set has any custom functions.
    pub fn has_custom_functions(&self) -> bool {
        !self.custom_functions.is_empty()
    }
}

impl<N: DomNavigator> Default for FunctionSet<N> {
    fn default() -> Self {
        Self::with_builtins()
    }
}

impl<N: DomNavigator> std::fmt::Debug for FunctionSet<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FunctionSet")
            .field("custom_count", &self.custom_functions.len())
            .field("lookup_count", &self.lookup.len())
            .finish()
    }
}

impl<N: DomNavigator> FunctionCatalog for FunctionSet<N> {
    fn lookup(&self, namespace: &str, local_name: &str, arity: usize) -> Option<FunctionHandle> {
        // First check custom functions (exact arity)
        let ns: Arc<str> = Arc::from(namespace);
        let local: Arc<str> = Arc::from(local_name);

        if let Some(&handle) = self.lookup.get(&(ns.clone(), local.clone(), arity)) {
            return Some(handle);
        }

        // Check variadic custom functions
        if let Some(&(handle, min_arity)) = self.variadic_lookup.get(&(ns.clone(), local.clone())) {
            if arity >= min_arity {
                return Some(handle);
            }
        }

        // Fall back to built-in registry
        FUNCTION_REGISTRY
            .lookup(namespace, local_name, arity)
            .map(|entry| FunctionHandle::from(entry.id))
    }

    fn get_signature(&self, handle: FunctionHandle) -> Option<DynamicFunctionSignature> {
        if let Some(index) = handle.custom_index() {
            // Custom function
            self.custom_functions
                .get(index)
                .map(|e| e.signature.clone())
        } else {
            // Built-in function
            FUNCTION_REGISTRY
                .by_id(handle_to_function_id(handle).ok()?)
                .map(|entry| DynamicFunctionSignature::from(&entry.signature))
        }
    }
}

impl<N: DomNavigator> FunctionEvaluator<N> for FunctionSet<N> {
    fn eval(
        &self,
        handle: FunctionHandle,
        ctx: &mut DynamicContext<'_, N>,
        args: Vec<XPathValue<N>>,
    ) -> Result<XPathValue<N>, XPathError> {
        if let Some(index) = handle.custom_index() {
            // Custom function
            let entry = self.custom_functions.get(index).ok_or_else(|| {
                XPathError::Internal(format!("Invalid custom function handle: {:?}", handle))
            })?;
            (entry.implementation)(ctx, args)
        } else {
            // Built-in function
            let id = handle_to_function_id(handle)?;
            eval_function(id, ctx, args)
        }
    }
}

// ============================================================================
// XPath10Catalog - Function catalog restricting to XPath 1.0 core functions
// ============================================================================

/// Static list of XPath 1.0 core function names (27 functions).
const XPATH10_FUNCTIONS: &[&str] = &[
    "last",
    "position",
    "count",
    "id",
    "name",
    "local-name",
    "namespace-uri",
    "lang",
    "string",
    "concat",
    "starts-with",
    "contains",
    "substring-before",
    "substring-after",
    "substring",
    "string-length",
    "normalize-space",
    "translate",
    "boolean",
    "not",
    "true",
    "false",
    "number",
    "sum",
    "floor",
    "ceiling",
    "round",
];

/// Catalog that restricts available functions to the XPath 1.0 core set.
///
/// For empty-namespace lookups (XPath 1.0 mode), only the 27 core functions
/// are allowed, and they are resolved via `FUNCTION_REGISTRY` using `FN_NAMESPACE`.
/// For non-empty namespace lookups, delegates to `BuiltinCatalog` unchanged.
#[derive(Debug, Clone, Copy, Default)]
pub struct XPath10Catalog;

impl FunctionCatalog for XPath10Catalog {
    fn lookup(&self, namespace: &str, local_name: &str, arity: usize) -> Option<FunctionHandle> {
        if namespace.is_empty() {
            // XPath 1.0 mode: only allow core 1.0 functions, resolve via FN_NAMESPACE
            if XPATH10_FUNCTIONS.contains(&local_name) {
                FUNCTION_REGISTRY
                    .lookup(super::signature::FN_NAMESPACE, local_name, arity)
                    .map(|entry| FunctionHandle::from(entry.id))
            } else {
                None
            }
        } else {
            // Non-empty namespace: delegate to builtin catalog
            BuiltinCatalog.lookup(namespace, local_name, arity)
        }
    }

    fn get_signature(&self, handle: FunctionHandle) -> Option<DynamicFunctionSignature> {
        BuiltinCatalog.get_signature(handle)
    }
}

// ============================================================================
// XPath10Evaluator - Evaluator applying XPath 1.0 semantics
// ============================================================================

/// Convert an XPathValue result to double if it's an integer.
///
/// XPath 1.0 has no integer type — all numeric results are doubles.
fn wrap_as_double<N: DomNavigator>(result: XPathValue<N>) -> XPathValue<N> {
    // Check integer first — as_f64() also succeeds for integers via conversion
    if let Some(i) = result.as_integer() {
        return XPathValue::double(i.to_string().parse::<f64>().unwrap_or(f64::NAN));
    }
    result
}

/// Evaluator that applies XPath 1.0 semantics to function results.
///
/// Intercepts specific functions to:
/// - Use first-node-string-value rule for `fn:string`
/// - Use first-node-to-number rule for `fn:number`
/// - Convert integer results to double for `count`, `string-length`, `last`, `position`
/// - Ensure `sum`, `floor`, `ceiling`, `round` return doubles
///
/// All other functions delegate to `BuiltinEvaluator` unchanged.
#[derive(Debug, Clone, Copy, Default)]
pub struct XPath10Evaluator;

impl<N: DomNavigator> FunctionEvaluator<N> for XPath10Evaluator {
    fn eval(
        &self,
        handle: FunctionHandle,
        ctx: &mut DynamicContext<'_, N>,
        args: Vec<XPathValue<N>>,
    ) -> Result<XPathValue<N>, XPathError> {
        let id = handle_to_function_id(handle)?;
        match id {
            // fn:string — use XPath 1.0 first-node rule
            FunctionId::String => {
                use crate::xpath::atomize;
                match args.len() {
                    0 => {
                        let item = ctx.require_context_item()?.clone();
                        let s = match item {
                            crate::xpath::iterator::XmlItem::Node(nav) => nav.value(),
                            crate::xpath::iterator::XmlItem::Atomic(v) => atomize::string_value(&v),
                        };
                        Ok(XPathValue::string(s))
                    }
                    1 => {
                        let s = atomize::to_string_10(&args[0]);
                        Ok(XPathValue::string(s))
                    }
                    _ => Err(XPathError::wrong_number_of_arguments(
                        "string",
                        1,
                        args.len(),
                    )),
                }
            }

            // fn:number — use XPath 1.0 first-node rule
            FunctionId::Number => {
                use crate::xpath::atomize;
                match args.len() {
                    0 => {
                        let item = ctx.require_context_item()?.clone();
                        let d = match item {
                            // XPath 1.0 §4.4: the string value read as a
                            // Number, or NaN.
                            crate::xpath::iterator::XmlItem::Node(nav) => {
                                atomize::xpath10_string_to_number(&nav.value())
                            }
                            crate::xpath::iterator::XmlItem::Atomic(v) => {
                                atomize::to_number_xpath10(&v)
                            }
                        };
                        Ok(XPathValue::double(d))
                    }
                    1 => {
                        let d = atomize::to_number_10(&args[0]);
                        Ok(XPathValue::double(d))
                    }
                    _ => Err(XPathError::wrong_number_of_arguments(
                        "number",
                        1,
                        args.len(),
                    )),
                }
            }

            // Functions that return integer in 2.0 but should return double in 1.0
            FunctionId::Count
            | FunctionId::StringLength
            | FunctionId::Last
            | FunctionId::Position => {
                let result = eval_function(id, ctx, args)?;
                Ok(wrap_as_double(result))
            }

            // Numeric functions — ensure double result in 1.0
            FunctionId::Sum | FunctionId::Floor | FunctionId::Ceiling | FunctionId::Round => {
                let result = eval_function(id, ctx, args)?;
                Ok(wrap_as_double(result))
            }

            // All other functions: delegate unchanged
            _ => eval_function(id, ctx, args),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xpath::RoXmlNavigator;

    #[test]
    fn test_function_handle_from_id() {
        let handle = FunctionHandle::from(FunctionId::Count);
        assert!(handle.is_builtin());
        assert!(!handle.is_custom());
    }

    #[test]
    fn test_custom_handle() {
        let handle = FunctionHandle(CUSTOM_HANDLE_BASE);
        assert!(!handle.is_builtin());
        assert!(handle.is_custom());
        assert_eq!(handle.custom_index(), Some(0));

        let handle2 = FunctionHandle(CUSTOM_HANDLE_BASE + 5);
        assert_eq!(handle2.custom_index(), Some(5));
    }

    #[test]
    fn test_builtin_catalog_lookup() {
        let catalog = BuiltinCatalog;
        let handle = catalog.lookup("http://www.w3.org/2005/xpath-functions", "count", 1);
        assert!(handle.is_some());
        assert!(handle.unwrap().is_builtin());
    }

    #[test]
    fn test_builtin_catalog_not_found() {
        let catalog = BuiltinCatalog;
        let handle = catalog.lookup("http://example.com", "my-func", 1);
        assert!(handle.is_none());
    }

    #[test]
    fn test_dynamic_signature_from_static() {
        let catalog = BuiltinCatalog;
        let handle = catalog
            .lookup("http://www.w3.org/2005/xpath-functions", "count", 1)
            .unwrap();
        let sig = catalog.get_signature(handle);
        assert!(sig.is_some());
        let sig = sig.unwrap();
        assert_eq!(&*sig.local_name, "count");
        assert_eq!(sig.arity, FunctionArity::Exact(1));
    }

    // ========================================================================
    // FunctionSet tests
    // ========================================================================

    #[test]
    fn test_function_set_builtins_accessible() {
        let functions: FunctionSet<RoXmlNavigator<'static>> = FunctionSet::with_builtins();

        // Built-in function should be found
        let handle = functions.lookup("http://www.w3.org/2005/xpath-functions", "count", 1);
        assert!(handle.is_some());
        assert!(handle.unwrap().is_builtin());
    }

    #[test]
    fn test_function_set_register_custom() {
        let mut functions: FunctionSet<RoXmlNavigator<'static>> = FunctionSet::with_builtins();

        let sig = DynamicFunctionSignature::new(
            "http://example.com/ext",
            "my-func",
            vec![SequenceType::string()],
            SequenceType::string(),
        );

        let handle = functions.register(sig, |_ctx, _args| Ok(XPathValue::string("custom result")));

        assert!(handle.is_custom());
        assert_eq!(functions.custom_count(), 1);
    }

    #[test]
    fn test_function_set_lookup_custom() {
        let mut functions: FunctionSet<RoXmlNavigator<'static>> = FunctionSet::with_builtins();

        let sig = DynamicFunctionSignature::new(
            "http://example.com/ext",
            "my-upper",
            vec![SequenceType::string()],
            SequenceType::string(),
        );

        let registered_handle = functions.register(sig, |_ctx, mut args| {
            let s = super::super::atomize_to_string(args.remove(0))?;
            Ok(XPathValue::string(s.to_uppercase()))
        });

        // Lookup should find our custom function
        let found_handle = functions.lookup("http://example.com/ext", "my-upper", 1);
        assert!(found_handle.is_some());
        assert_eq!(found_handle.unwrap(), registered_handle);
        assert!(found_handle.unwrap().is_custom());
    }

    #[test]
    fn test_function_set_get_custom_signature() {
        let mut functions: FunctionSet<RoXmlNavigator<'static>> = FunctionSet::with_builtins();

        let sig = DynamicFunctionSignature::new(
            "http://example.com/ext",
            "test-func",
            vec![SequenceType::integer(), SequenceType::integer()],
            SequenceType::integer(),
        );

        let handle = functions.register(sig, |_ctx, _args| Ok(XPathValue::integer(42)));

        let retrieved_sig = functions.get_signature(handle);
        assert!(retrieved_sig.is_some());
        let retrieved_sig = retrieved_sig.unwrap();
        assert_eq!(&*retrieved_sig.namespace, "http://example.com/ext");
        assert_eq!(&*retrieved_sig.local_name, "test-func");
        assert_eq!(retrieved_sig.arity, FunctionArity::Exact(2));
    }

    #[test]
    fn test_function_set_custom_overrides_builtin() {
        let mut functions: FunctionSet<RoXmlNavigator<'static>> = FunctionSet::with_builtins();

        // Register a custom function with the same name as a builtin
        let sig = DynamicFunctionSignature::new(
            "http://www.w3.org/2005/xpath-functions",
            "count",
            vec![SequenceType::any()],
            SequenceType::integer(),
        );

        let custom_handle = functions.register(sig, |_ctx, _args| {
            // Custom count always returns 999
            Ok(XPathValue::integer(999))
        });

        // Lookup should now find the custom function
        let found_handle = functions.lookup("http://www.w3.org/2005/xpath-functions", "count", 1);
        assert!(found_handle.is_some());
        assert_eq!(found_handle.unwrap(), custom_handle);
        assert!(found_handle.unwrap().is_custom());
    }

    #[test]
    fn test_function_set_eval_custom() {
        use crate::namespace::table::NameTable;
        use crate::xpath::context::{DynamicContext, XPathContext};

        let mut functions: FunctionSet<RoXmlNavigator<'static>> = FunctionSet::with_builtins();

        // Register a simple doubling function
        let sig = DynamicFunctionSignature::new(
            "http://example.com/ext",
            "double",
            vec![SequenceType::double()],
            SequenceType::double(),
        );

        let handle = functions.register(sig, |_ctx, mut args| {
            let val = args.remove(0);
            let d = val.as_f64().unwrap_or(0.0);
            Ok(XPathValue::double(d * 2.0))
        });

        // Create a minimal context for evaluation
        let names = NameTable::new();
        let static_ctx = XPathContext::new(&names);
        let mut dyn_ctx: DynamicContext<'_, RoXmlNavigator<'static>> =
            DynamicContext::new(&static_ctx, 0);

        // Evaluate the custom function
        let args = vec![XPathValue::double(21.0)];
        let result = functions.eval(handle, &mut dyn_ctx, args).unwrap();

        assert_eq!(result.as_f64(), Some(42.0));
    }

    #[test]
    fn test_function_set_eval_builtin() {
        use crate::namespace::table::NameTable;
        use crate::types::value::XmlValue;
        use crate::xpath::context::{DynamicContext, XPathContext};
        use crate::xpath::iterator::XmlItem;

        let functions: FunctionSet<RoXmlNavigator<'static>> = FunctionSet::with_builtins();

        // Get handle for builtin count function
        let handle = functions
            .lookup("http://www.w3.org/2005/xpath-functions", "count", 1)
            .unwrap();

        // Create context
        let names = NameTable::new();
        let static_ctx = XPathContext::new(&names);
        let mut dyn_ctx: DynamicContext<'_, RoXmlNavigator<'static>> =
            DynamicContext::new(&static_ctx, 0);

        // Create a sequence of 3 items
        let items = vec![
            XmlItem::Atomic(XmlValue::integer(1.into())),
            XmlItem::Atomic(XmlValue::integer(2.into())),
            XmlItem::Atomic(XmlValue::integer(3.into())),
        ];
        let args = vec![XPathValue::from_sequence(items)];

        // Evaluate
        let result = functions.eval(handle, &mut dyn_ctx, args).unwrap();
        assert_eq!(
            result.as_integer().map(|i| i.to_string()),
            Some("3".to_string())
        );
    }

    #[test]
    fn test_function_set_range_arity() {
        let mut functions: FunctionSet<RoXmlNavigator<'static>> = FunctionSet::with_builtins();

        // Register a function with range arity (1-3 arguments)
        let sig = DynamicFunctionSignature::range(
            "http://example.com/ext",
            "multi",
            1,
            3,
            vec![
                SequenceType::string(),
                SequenceType::string(),
                SequenceType::string(),
            ],
            SequenceType::string(),
        );

        let handle =
            functions.register(sig, |_ctx, args| Ok(XPathValue::integer(args.len() as i64)));

        // Should match arities 1, 2, and 3
        assert_eq!(
            functions.lookup("http://example.com/ext", "multi", 1),
            Some(handle)
        );
        assert_eq!(
            functions.lookup("http://example.com/ext", "multi", 2),
            Some(handle)
        );
        assert_eq!(
            functions.lookup("http://example.com/ext", "multi", 3),
            Some(handle)
        );

        // Should not match arity 0 or 4
        assert!(functions
            .lookup("http://example.com/ext", "multi", 0)
            .is_none());
        assert!(functions
            .lookup("http://example.com/ext", "multi", 4)
            .is_none());
    }

    #[test]
    fn test_function_set_variadic() {
        let mut functions: FunctionSet<RoXmlNavigator<'static>> = FunctionSet::with_builtins();

        // Register a variadic function (min 2 arguments)
        let sig = DynamicFunctionSignature::variadic(
            "http://example.com/ext",
            "varargs",
            2,
            vec![SequenceType::any_atomic()],
            SequenceType::integer(),
        );

        let handle =
            functions.register(sig, |_ctx, args| Ok(XPathValue::integer(args.len() as i64)));

        // Should match arities >= 2
        assert_eq!(
            functions.lookup("http://example.com/ext", "varargs", 2),
            Some(handle)
        );
        assert_eq!(
            functions.lookup("http://example.com/ext", "varargs", 5),
            Some(handle)
        );
        assert_eq!(
            functions.lookup("http://example.com/ext", "varargs", 100),
            Some(handle)
        );

        // Should not match arities < 2
        assert!(functions
            .lookup("http://example.com/ext", "varargs", 0)
            .is_none());
        assert!(functions
            .lookup("http://example.com/ext", "varargs", 1)
            .is_none());
    }

    // ========================================================================
    // XPath10Catalog tests
    // ========================================================================

    #[test]
    fn test_xpath10_catalog_resolves_core_function() {
        let catalog = XPath10Catalog;
        // count is in the 1.0 core set — empty namespace should resolve
        let handle = catalog.lookup("", "count", 1);
        assert!(handle.is_some());
        assert!(handle.unwrap().is_builtin());
    }

    #[test]
    fn test_xpath10_catalog_rejects_non_core_function() {
        let catalog = XPath10Catalog;
        // deep-equal is XPath 2.0 only — should be rejected in empty namespace
        let handle = catalog.lookup("", "deep-equal", 2);
        assert!(handle.is_none());
    }

    #[test]
    fn test_xpath10_catalog_non_empty_ns_delegates() {
        let catalog = XPath10Catalog;
        // Non-empty namespace delegates to BuiltinCatalog unchanged
        let handle = catalog.lookup("http://www.w3.org/2005/xpath-functions", "count", 1);
        assert!(handle.is_some());
    }

    #[test]
    fn test_xpath10_catalog_all_core_functions() {
        let catalog = XPath10Catalog;
        // Verify all 27 core functions resolve
        let functions_with_arity: &[(&str, usize)] = &[
            ("last", 0),
            ("position", 0),
            ("count", 1),
            ("id", 1),
            ("name", 0),
            ("local-name", 0),
            ("namespace-uri", 0),
            ("lang", 1),
            ("string", 0),
            ("concat", 2),
            ("starts-with", 2),
            ("contains", 2),
            ("substring-before", 2),
            ("substring-after", 2),
            ("substring", 2),
            ("string-length", 0),
            ("normalize-space", 0),
            ("translate", 3),
            ("boolean", 1),
            ("not", 1),
            ("true", 0),
            ("false", 0),
            ("number", 0),
            ("sum", 1),
            ("floor", 1),
            ("ceiling", 1),
            ("round", 1),
        ];
        for (name, arity) in functions_with_arity {
            let handle = catalog.lookup("", name, *arity);
            assert!(
                handle.is_some(),
                "XPath 1.0 function '{}' with arity {} not found",
                name,
                arity
            );
        }
    }

    // ========================================================================
    // XPath10Evaluator tests
    // ========================================================================

    #[test]
    fn test_xpath10_evaluator_count_returns_double() {
        use crate::namespace::table::NameTable;
        use crate::types::value::XmlValue;
        use crate::xpath::context::{DynamicContext, XPathContext};
        use crate::xpath::iterator::XmlItem;

        let evaluator = XPath10Evaluator;

        let names = NameTable::new();
        let static_ctx = XPathContext::new(&names);
        let mut dyn_ctx: DynamicContext<'_, RoXmlNavigator<'static>> =
            DynamicContext::new(&static_ctx, 0);

        // Create a sequence of 3 items
        let items = vec![
            XmlItem::Atomic(XmlValue::integer(1.into())),
            XmlItem::Atomic(XmlValue::integer(2.into())),
            XmlItem::Atomic(XmlValue::integer(3.into())),
        ];
        let args = vec![XPathValue::from_sequence(items)];
        let handle = FunctionHandle::from(FunctionId::Count);

        let result = evaluator.eval(handle, &mut dyn_ctx, args).unwrap();
        // In XPath 1.0, count() should return double, not integer
        assert_eq!(result.as_f64(), Some(3.0));
        // Should NOT be an integer
        assert!(result.as_integer().is_none());
    }

    #[test]
    fn test_xpath10_evaluator_string_with_node() {
        let evaluator = XPath10Evaluator;

        let doc = roxmltree::Document::parse("<r><a>first</a><b>second</b></r>").unwrap();
        let mut nav_a = RoXmlNavigator::new(&doc);
        nav_a.move_to_first_child(); // <r>
        nav_a.move_to_first_child(); // <a>
        let mut nav_b = nav_a.clone();
        nav_b.move_to_next_sibling(); // <b>

        use crate::namespace::table::NameTable;
        use crate::xpath::context::{DynamicContext, XPathContext};
        use crate::xpath::iterator::XmlItem;

        let names = NameTable::new();
        let static_ctx = XPathContext::new(&names);
        let mut dyn_ctx = DynamicContext::new(&static_ctx, 0);

        // Sequence of two nodes — XPath 1.0 string() should use first node
        let node_seq = XPathValue::from_sequence(vec![XmlItem::Node(nav_a), XmlItem::Node(nav_b)]);
        let args = vec![node_seq];
        let handle = FunctionHandle::from(FunctionId::String);

        let result = evaluator.eval(handle, &mut dyn_ctx, args).unwrap();
        assert_eq!(result.to_owned_string(), Some("first".to_string()));
    }
}
