//! XPath static and dynamic context definitions.
//!
//! This module provides:
//! - `XPathContext` - Static context for expression binding and evaluation
//! - `DynamicContext` - Runtime context for XPath evaluation
//! - `VarStore` - Variable storage (Vec-based arena indexed by VarSlotId)
//! - `NameBinder` - Compile-time variable slot allocation

use std::sync::atomic::{AtomicU64, Ordering};

use crate::ids::NameId;
use crate::namespace::context::NamespaceContextSnapshot;
use crate::namespace::qname::QualifiedName;
use crate::namespace::table::NameTable;
use crate::schema::SchemaSet;
use crate::types::value::{DateTimeValue, TimezoneOffset};

use super::collation::CollationResolver;
use super::functions::{BuiltinCatalog, BuiltinEvaluator, FunctionCatalog, FunctionEvaluator};
use super::iterator::XmlItem;
use super::DomNavigator;
use super::XPathMode;

// ============================================================================
// XPathContext (static context for bind-time and eval-time)
// ============================================================================

/// XPath 2.0 static context for expression binding and evaluation.
///
/// The static context provides information needed during expression compilation
/// and evaluation:
/// - Namespace prefix resolution
/// - Function registry access
/// - Default namespaces
/// - Schema type information
#[derive(Debug, Clone)]
pub struct XPathContext<'a> {
    /// Name table for string interning
    pub names: &'a NameTable,
    /// Schema set for type information
    pub schema_set: Option<&'a SchemaSet>,
    /// Namespace bindings for prefix resolution
    pub namespaces: NamespaceContextSnapshot,
    /// Default namespace for unprefixed element names
    pub default_element_ns: Option<NameId>,
    /// Default namespace for unprefixed function names (fn: namespace)
    pub default_function_ns: Option<&'static str>,
    /// Implicit timezone
    pub implicit_timezone: Option<TimezoneOffset>,
    /// Base URI for relative URI resolution
    pub base_uri: Option<String>,
    /// XPath language mode (1.0 or 2.0)
    pub mode: XPathMode,
    /// Enable fn:trace() output to stderr (disabled by default)
    pub trace_enabled: bool,
    /// Function catalog for bind-time lookup (None = use builtins).
    function_catalog: Option<&'a dyn FunctionCatalog>,
    /// Owned default function namespace, set by
    /// [`with_default_function_ns_owned`](Self::with_default_function_ns_owned).
    ///
    /// Takes precedence over the `default_function_ns` field when set; see
    /// [`default_function_namespace`](Self::default_function_namespace).
    default_function_ns_owned: Option<String>,
    /// XPath 1.0 compatibility mode, set by
    /// [`with_xpath10_compatibility`](Self::with_xpath10_compatibility).
    xpath10_compatibility: bool,
    /// Host collations, set by
    /// [`with_collation_resolver`](Self::with_collation_resolver). `None` means
    /// the codepoint collation is the only one this context supports.
    collation_resolver: Option<&'a dyn CollationResolver>,
    /// The static context's *default collation* property, set by
    /// [`with_default_collation`](Self::with_default_collation).
    ///
    /// `None` is the codepoint collation, and the codepoint URI is normalised
    /// to `None` when it is set explicitly — so every hot path decides "is the
    /// default collation the codepoint one" with a discriminant test and never
    /// a string comparison.
    default_collation: Option<String>,
    /// Whether the atomic types of the schema set are targets of `cast as` /
    /// `castable as` and have constructor functions, set off by
    /// [`without_schema_casts`](Self::without_schema_casts).
    schema_casts: bool,
    /// Whether the in-scope schema definitions are only the components present
    /// in every schema by definition, set by
    /// [`with_built_in_components_only`](Self::with_built_in_components_only).
    built_in_components_only: bool,
}

impl<'a> XPathContext<'a> {
    /// Create a new static context with the given name table.
    pub fn new(names: &'a NameTable) -> Self {
        Self {
            names,
            schema_set: None,
            namespaces: NamespaceContextSnapshot::default(),
            default_element_ns: None,
            default_function_ns: Some(super::functions::FN_NAMESPACE),
            implicit_timezone: None,
            base_uri: None,
            mode: XPathMode::XPath20,
            trace_enabled: false,
            function_catalog: None,
            default_function_ns_owned: None,
            xpath10_compatibility: false,
            collation_resolver: None,
            default_collation: None,
            schema_casts: true,
            built_in_components_only: false,
        }
    }

    /// Set the schema set: its named types join the *in-scope schema types* of
    /// the static context, and its top-level element and attribute declarations
    /// the in-scope element and attribute declarations (XPath 2.0 §2.1.1).
    ///
    /// Names are looked up in the schema set by the ids of this context's
    /// [`names`](Self::names), so that should be the schema set's own
    /// [`name_table`](SchemaSet::name_table). An expression compiled with the
    /// schema set keeps its types as keys of that set, so evaluate it with a
    /// context carrying the same set.
    ///
    /// With a schema set attached, an atomic simple type of the set
    ///
    /// * is an `AtomicType` of `instance of` and `treat as`, matched by
    ///   derivation (§2.5.4.2);
    /// * is a target of `cast as` and `castable as` (§3.10.2, §3.10.3): the
    ///   value is cast to the type's nearest built-in type and then checked
    ///   against the type's facets by the schema validator — on the string of
    ///   a string or `xs:untypedAtomic` source, otherwise on the value's
    ///   `xs:string` cast — `FORG0001` when one is not satisfied, and the
    ///   result carries the type as its
    ///   [`schema_type`](crate::types::XmlValue::schema_type);
    /// * has a constructor function of its own name (§3.10.4) — `my:t($arg)` is
    ///   `$arg cast as my:t?` — unless the function catalog has a function of
    ///   that name and arity, which takes precedence. A type in no namespace
    ///   is reached by its constructor only when the default function
    ///   namespace is none.
    ///
    /// A list or union type is not an atomic type: it is neither a target of
    /// `cast as` (`XPST0051`) nor a constructor (`XPST0017`). The XPath of an
    /// XSD 1.1 assertion is evaluated without the casts and constructors of
    /// the schema's own types (Structures §3.13.6.2 clause 2.2.5 puts only the
    /// built-in types in its static context).
    ///
    /// ```
    /// use xsd_schema::namespace::context::NamespaceContextSnapshot;
    /// use xsd_schema::xpath::{RoXmlNavigator, XPathContext, XPathExpr};
    /// use xsd_schema::{SchemaSetBuilder, XsdVersion};
    ///
    /// let set = SchemaSetBuilder::with_version(XsdVersion::V1_1)
    ///     .add_source(
    ///         r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
    ///                       targetNamespace="urn:hats">
    ///              <xs:simpleType name="size">
    ///                <xs:restriction base="xs:integer">
    ///                  <xs:minInclusive value="1"/><xs:maxInclusive value="20"/>
    ///                </xs:restriction>
    ///              </xs:simpleType>
    ///            </xs:schema>"#,
    ///         "file:///hats.xsd",
    ///     )
    ///     .unwrap()
    ///     .compile()
    ///     .unwrap()
    ///     .into_schema_set();
    ///
    /// let mut namespaces = NamespaceContextSnapshot::default();
    /// namespaces
    ///     .bindings
    ///     .push((set.name_table.add("h"), set.name_table.add("urn:hats")));
    /// let ctx = XPathContext::new(&set.name_table)
    ///     .with_namespaces(namespaces)
    ///     .with_schema_set(&set);
    ///
    /// let run = |expr: &str| {
    ///     XPathExpr::compile(expr, &ctx)?
    ///         .evaluator(&ctx)
    ///         .run_string::<RoXmlNavigator<'static>>()
    /// };
    /// assert_eq!(run("h:size('7') instance of h:size").unwrap(), "true");
    /// assert_eq!(run("'12' castable as h:size").unwrap(), "true");
    /// assert_eq!(run("h:size('7') + 1").unwrap(), "8");
    /// assert_eq!(run("'21' cast as h:size").unwrap_err().error_code(), Some("FORG0001"));
    /// ```
    pub fn with_schema_set(mut self, schema_set: &'a SchemaSet) -> Self {
        self.schema_set = Some(schema_set);
        self
    }

    /// Set the namespace bindings
    pub fn with_namespaces(mut self, namespaces: NamespaceContextSnapshot) -> Self {
        self.namespaces = namespaces;
        self
    }

    /// Set the default element namespace
    pub fn with_default_element_ns(mut self, ns: NameId) -> Self {
        self.default_element_ns = Some(ns);
        self
    }

    /// Set the default function namespace
    pub fn with_default_function_ns(mut self, ns: &'static str) -> Self {
        self.default_function_ns = Some(ns);
        self
    }

    /// Set the default function namespace from an owned string.
    ///
    /// This is the counterpart of [`with_default_function_ns`](Self::with_default_function_ns)
    /// for namespaces that are only known at runtime — for example one read from a
    /// configuration file or a host document — and that therefore
    /// cannot be a `&'static str` without leaking. The string is stored in a private field,
    /// so the public `default_function_ns` field and its `&'static str` setter keep working
    /// unchanged.
    ///
    /// # Precedence
    ///
    /// [`default_function_namespace`](Self::default_function_namespace) resolves, in order:
    ///
    /// 1. in [`XPathMode::XPath10`] the empty namespace
    ///    (unchanged: XPath 1.0 core functions live in no namespace, and neither setter
    ///    overrides that);
    /// 2. the value set here, if any;
    /// 3. the public `default_function_ns` field, if `Some`;
    /// 4. the `fn:` namespace.
    ///
    /// # Example
    ///
    /// ```
    /// use xsd_schema::namespace::table::NameTable;
    /// use xsd_schema::xpath::XPathContext;
    ///
    /// // A namespace computed at runtime, not a literal.
    /// let host_ns = format!("http://example.com/fn/v{}", 2);
    ///
    /// let names = NameTable::new();
    /// let ctx = XPathContext::new(&names).with_default_function_ns_owned(host_ns);
    ///
    /// assert_eq!(ctx.default_function_namespace(), "http://example.com/fn/v2");
    /// ```
    pub fn with_default_function_ns_owned(mut self, ns: impl Into<String>) -> Self {
        self.default_function_ns_owned = Some(ns.into());
        self
    }

    /// Set the implicit timezone
    pub fn with_implicit_timezone(mut self, tz: TimezoneOffset) -> Self {
        self.implicit_timezone = Some(tz);
        self
    }

    /// Set the base URI
    pub fn with_base_uri(mut self, base_uri: impl Into<String>) -> Self {
        self.base_uri = Some(base_uri.into());
        self
    }

    /// Set the XPath language mode.
    pub fn with_mode(mut self, mode: XPathMode) -> Self {
        self.mode = mode;
        self
    }

    /// Enable fn:trace() output to stderr.
    pub fn with_trace_enabled(mut self, enabled: bool) -> Self {
        self.trace_enabled = enabled;
        self
    }

    /// Turn *XPath 1.0 compatibility mode* on or off.
    ///
    /// This is the static-context property XPath 2.0 calls "XPath 1.0
    /// compatibility mode", and it is **not** the same thing as
    /// [`XPathMode::XPath10`]. The mode selects the *language*: its lexer and
    /// parser reject XPath 2.0 syntax outright (sequence expressions, `for`,
    /// `instance of`, double literals, …). The flag set here keeps the full
    /// XPath 2.0 syntax and changes only the *semantics* that XPath 2.0 itself
    /// defines differently when the flag is true:
    ///
    /// * the operands of `+`, `-`, `*`, `div` and `mod` are converted with the
    ///   1.0 number rules;
    /// * general comparisons (`=`, `!=`, `<`, …) follow the compatibility
    ///   rules of XPath 2.0 §3.5.2: an operand compared with a single boolean is
    ///   converted to its effective boolean value, and `<`, `<=`, `>`, `>=`
    ///   compare with `fn:number`;
    /// * an argument whose declared type is a single item takes the first item
    ///   of the supplied sequence (the function conversion rules of §3.1.5).
    ///
    /// The effective boolean value itself is not affected: a sequence of two or
    /// more atomic values is `FORG0006` with the flag as without it (§2.4.3).
    ///
    /// Hosts embedding the XPath engine need this when they must run
    /// expressions written for a 1.0-era host language while still accepting
    /// 2.0 syntax in the same document. Setting
    /// [`with_mode`](Self::with_mode) to [`XPathMode::XPath10`] implies these
    /// semantics as well, so the flag is only meaningful in
    /// [`XPathMode::XPath20`].
    ///
    /// # Example
    ///
    /// ```
    /// use xsd_schema::namespace::table::NameTable;
    /// use xsd_schema::xpath::{RoXmlNavigator, XPathContext, XPathExpr};
    ///
    /// let names = NameTable::new();
    /// let ctx = XPathContext::new(&names).with_xpath10_compatibility(true);
    ///
    /// // 2.0 syntax still parses …
    /// let expr = XPathExpr::compile("(1, 2, 3)[1]", &ctx).unwrap();
    /// // … while `1 div 0` follows the 1.0 rule of yielding INF, not an error.
    /// let inf = XPathExpr::compile("1 div 0", &ctx).unwrap();
    /// let value = inf.evaluator(&ctx).run_number::<RoXmlNavigator<'static>>().unwrap();
    /// assert!(value.is_infinite());
    /// let _ = expr;
    /// ```
    pub fn with_xpath10_compatibility(mut self, enabled: bool) -> Self {
        self.xpath10_compatibility = enabled;
        self
    }

    /// Whether *XPath 1.0 compatibility mode* is on — either because
    /// [`with_xpath10_compatibility`](Self::with_xpath10_compatibility) set it
    /// or because the language mode is [`XPathMode::XPath10`], which implies
    /// it.
    #[inline]
    pub fn xpath10_compatibility(&self) -> bool {
        self.xpath10_compatibility || self.mode == XPathMode::XPath10
    }

    /// Get the XPath language mode.
    pub fn mode(&self) -> XPathMode {
        self.mode
    }

    /// Keep the atomic types of the schema set out of `cast as`, `castable
    /// as` and the constructor functions, as before they were added: a cast
    /// to such a type raises XPST0051 when it is evaluated, `castable as` is
    /// false, and a call of its constructor is XPST0017. `instance of` and
    /// `treat as` still see the types.
    ///
    /// The static context of an XSD 1.1 assertion, whose in-scope schema
    /// definitions are only the built-in ones (Structures §3.13.6.2 clause
    /// 2.2.5: "those components that are present in every schema by
    /// definition").
    pub(crate) fn without_schema_casts(mut self) -> Self {
        self.schema_casts = false;
        self
    }

    /// Whether the atomic types of the schema set are cast targets and have
    /// constructor functions (see [`without_schema_casts`](Self::without_schema_casts)).
    pub(crate) fn schema_casts(&self) -> bool {
        self.schema_casts
    }

    /// Keep only the components present in every schema by definition in the
    /// in-scope schema definitions (Structures 1.1 §3.13.6.2 clause 2.2.5):
    /// the built-in types — whose names resolve without a schema set — and,
    /// of the attached schema set's declarations, the four built-in `xsi:`
    /// attribute declarations (§3.2.7). Its named types and its element and
    /// other attribute declarations stay out: naming one is the static error
    /// it is without a schema set. Implies
    /// [`without_schema_casts`](Self::without_schema_casts).
    ///
    /// The static context in which an XSD 1.1 assertion, assertion facet or
    /// type alternative must be XPath Valid when the schema is built.
    pub(crate) fn with_built_in_components_only(mut self) -> Self {
        self.built_in_components_only = true;
        self.without_schema_casts()
    }

    /// The schema set whose named types and top-level declarations are in the
    /// in-scope schema definitions: the attached one, or none under
    /// [`with_built_in_components_only`](Self::with_built_in_components_only).
    pub(crate) fn user_components(&self) -> Option<&'a SchemaSet> {
        self.schema_set.filter(|_| !self.built_in_components_only)
    }

    /// Set the function catalog for custom function support.
    pub fn with_function_catalog(mut self, catalog: &'a dyn FunctionCatalog) -> Self {
        self.function_catalog = Some(catalog);
        self
    }

    /// Get the function catalog, using built-in functions as default.
    ///
    /// Returns a reference to the configured catalog, or `BuiltinCatalog` if none set.
    pub fn function_catalog(&self) -> &dyn FunctionCatalog {
        static BUILTIN: BuiltinCatalog = BuiltinCatalog;
        self.function_catalog.unwrap_or(&BUILTIN)
    }

    /// Install the host's collations.
    ///
    /// Without a resolver the only collation this context supports is the
    /// Unicode codepoint collation
    /// ([`CODEPOINT_COLLATION_URI`](crate::xpath::collation::CODEPOINT_COLLATION_URI)),
    /// and every other collation URI is `FOCH0002` where it is used. With one,
    /// the `$collation` argument of `fn:compare`, `fn:contains`,
    /// `fn:starts-with`, `fn:ends-with`, `fn:substring-before`,
    /// `fn:substring-after`, `fn:index-of`, `fn:distinct-values`,
    /// `fn:deep-equal`, `fn:min` and `fn:max` — and the default collation set by
    /// [`with_default_collation`](Self::with_default_collation) — are looked up
    /// through it.
    ///
    /// The resolver is never asked about the codepoint URI, which this crate
    /// implements itself, and it is asked only when a collation is needed to
    /// compare strings, so an expression that compares none never reaches it.
    ///
    /// See [`collation`](crate::xpath::collation) for a worked example.
    pub fn with_collation_resolver(mut self, resolver: &'a dyn CollationResolver) -> Self {
        self.collation_resolver = Some(resolver);
        self
    }

    /// Set the static context's *default collation* property.
    ///
    /// This is the collation the value and general comparisons (`eq`, `lt`,
    /// `=`, `<`, …) use for strings, and the one every collation-aware function
    /// uses when it is called without a `$collation` argument. It is also what
    /// `fn:default-collation()` returns. Unset, it is the Unicode codepoint
    /// collation.
    ///
    /// The URI is taken as given: XPath 2.0 §2.1.1 makes the default collation a
    /// static-context property that is already an absolute URI, so — unlike a
    /// `$collation` *argument* — it is not resolved against the base URI. It is
    /// not validated here either: an unsupported default collation raises
    /// `FOCH0002` where a string comparison needs it, and never disturbs an
    /// expression that compares no strings.
    pub fn with_default_collation(mut self, uri: impl Into<String>) -> Self {
        let uri = uri.into();
        // The codepoint collation is the absence of a default collation, so
        // that the hot paths never compare this URI at all.
        self.default_collation =
            (uri != crate::xpath::collation::CODEPOINT_COLLATION_URI).then_some(uri);
        self
    }

    /// The static context's default collation URI — the codepoint collation
    /// when [`with_default_collation`](Self::with_default_collation) has not
    /// set another one.
    pub fn default_collation(&self) -> &str {
        self.default_collation
            .as_deref()
            .unwrap_or(crate::xpath::collation::CODEPOINT_COLLATION_URI)
    }

    /// The default collation URI, or `None` when it is the codepoint collation.
    ///
    /// The internal counterpart of [`default_collation`](Self::default_collation):
    /// `None` is the answer the comparison paths want, because it needs no
    /// resolution at all.
    #[inline]
    pub(crate) fn default_collation_uri(&self) -> Option<&str> {
        self.default_collation.as_deref()
    }

    /// The host's collation resolver, if one was installed.
    #[inline]
    pub(crate) fn collation_resolver(&self) -> Option<&'a dyn CollationResolver> {
        self.collation_resolver
    }

    /// Resolve a prefix to a namespace URI.
    ///
    /// Returns the namespace URI for the given prefix, or None if not found.
    pub fn resolve_prefix(&self, prefix: &str) -> Option<String> {
        if prefix.is_empty() {
            // Empty prefix: use default element namespace
            self.default_element_ns
                .and_then(|id| self.names.try_resolve(id))
        } else if let Some(prefix_id) = self.names.get(prefix) {
            self.namespaces
                .resolve_prefix(prefix_id)
                .and_then(|ns_id| self.names.try_resolve(ns_id))
        } else {
            None
        }
    }

    /// Resolve a prefix to a namespace URI using NameId.
    pub fn resolve_prefix_id(&self, prefix_id: NameId) -> Option<NameId> {
        self.namespaces.resolve_prefix(prefix_id)
    }

    /// Get the default function namespace.
    ///
    /// In XPath 1.0 mode, core functions live in no namespace (empty string).
    /// In XPath 2.0 mode, the default is the fn: namespace.
    ///
    /// # Precedence
    ///
    /// 1. [`XPathMode::XPath10`] always yields `""` — XPath
    ///    1.0 core functions live in no namespace, and neither of the two setters overrides
    ///    that.
    /// 2. Otherwise the owned namespace set by
    ///    [`with_default_function_ns_owned`](Self::with_default_function_ns_owned), if any.
    /// 3. Otherwise the public `default_function_ns` field (set by
    ///    [`with_default_function_ns`](Self::with_default_function_ns)), if `Some`.
    /// 4. Otherwise the `fn:` namespace.
    pub fn default_function_namespace(&self) -> &str {
        match self.mode {
            XPathMode::XPath10 => "",
            XPathMode::XPath20 => match self.default_function_ns_owned.as_deref() {
                Some(ns) => ns,
                None => self
                    .default_function_ns
                    .unwrap_or(super::functions::FN_NAMESPACE),
            },
        }
    }

    /// Resolve a name from the name table.
    pub fn resolve_name(&self, id: NameId) -> Option<String> {
        self.names.try_resolve(id)
    }
}

// ============================================================================
// NameBinder (compile-time variable slot allocation)
// ============================================================================

/// Variable slot identifier for indexing into VarStore.
pub type VarSlotId = u32;

/// Reference to a variable slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VarRef {
    pub slot: VarSlotId,
}

/// Entry in the NameBinder stack.
#[derive(Debug, Clone)]
pub struct NameSlot {
    pub name: QualifiedName,
    pub slot: VarSlotId,
}

/// Compile-time variable binder for slot allocation.
///
/// Provides stack-based scoping with slot IDs into a data pool.
/// Used during expression binding to assign variable slots.
///
/// External variables (those declared before `mark_external_boundary()` is called)
/// are tracked separately and can be retrieved via `external_vars()`.
#[derive(Debug, Default)]
pub struct NameBinder {
    next_slot: VarSlotId,
    stack: Vec<NameSlot>,
    /// Count of external variables (pushed before mark_external_boundary)
    external_var_count: usize,
}

impl NameBinder {
    /// Create a new empty name binder.
    pub fn new() -> Self {
        Self {
            next_slot: 0,
            stack: Vec::new(),
            external_var_count: 0,
        }
    }

    /// Get the total number of slots allocated.
    ///
    /// Use this after bind() to determine VarStore size.
    pub fn len(&self) -> usize {
        self.next_slot as usize
    }

    /// Check if any slots have been allocated.
    pub fn is_empty(&self) -> bool {
        self.next_slot == 0
    }

    /// Mark the current stack position as the boundary between external variables
    /// and internally-bound variables.
    ///
    /// Call this after pushing all external variables (those provided by the API user)
    /// and before binding the expression (which may introduce for/let/quantified variables).
    pub fn mark_external_boundary(&mut self) {
        self.external_var_count = self.stack.len();
    }

    /// Iterate over external variables (those pushed before `mark_external_boundary()`).
    ///
    /// Returns an iterator of (name, slot) pairs for all external variables.
    pub fn external_vars(&self) -> impl Iterator<Item = (&QualifiedName, VarSlotId)> {
        self.stack
            .iter()
            .take(self.external_var_count)
            .map(|slot| (&slot.name, slot.slot))
    }

    /// Get the number of external variables.
    pub fn external_var_count(&self) -> usize {
        self.external_var_count
    }

    /// Push a new variable binding onto the stack.
    ///
    /// Allocates a new slot and returns a VarRef to it.
    pub fn push_var(&mut self, name: QualifiedName) -> VarRef {
        let slot = self.next_slot;
        self.next_slot += 1;
        self.stack.push(NameSlot { name, slot });
        VarRef { slot }
    }

    /// Pop the most recent variable binding.
    ///
    /// Used by for/some/every expressions after binding their body.
    pub fn pop_var(&mut self) {
        self.stack.pop();
    }

    /// Resolve a variable name to its slot.
    ///
    /// Searches from the top of the stack (most recent binding first).
    /// Returns XPST0008 if the variable is not bound.
    pub fn resolve(&self, name: &QualifiedName) -> Result<VarRef, super::error::XPathError> {
        // Walk stack from end to beginning (last-in, first-out scoping)
        for entry in self.stack.iter().rev() {
            if entry.name == *name {
                return Ok(VarRef { slot: entry.slot });
            }
        }
        // Format QName for error - we don't have access to NameTable here,
        // so we use the raw NameId values in the message
        Err(super::error::XPathError::XPST0008 {
            qname: format!("$var(local={})", name.local_name.0),
        })
    }

    /// Resolve a variable name to its slot, with NameTable for error messages.
    ///
    /// Same as `resolve()` but provides better error messages.
    pub fn resolve_with_names(
        &self,
        name: &QualifiedName,
        names: &NameTable,
    ) -> Result<VarRef, super::error::XPathError> {
        // Walk stack from end to beginning (last-in, first-out scoping)
        for entry in self.stack.iter().rev() {
            if entry.name == *name {
                return Ok(VarRef { slot: entry.slot });
            }
        }
        // Format QName for error using NameTable
        let local = names
            .try_resolve(name.local_name)
            .unwrap_or_else(|| "<unknown>".to_string());
        let qname_str = if let Some(prefix_id) = name.prefix {
            let prefix = names
                .try_resolve(prefix_id)
                .unwrap_or_else(|| "<unknown>".to_string());
            format!("{}:{}", prefix, local)
        } else {
            local.to_string()
        };
        Err(super::error::XPathError::XPST0008 { qname: qname_str })
    }
}

// ============================================================================
// VarStore (variable storage - Vec-based arena)
// ============================================================================

/// Variable storage for XPath evaluation.
///
/// Stores variable values indexed by VarSlotId.
/// Size is determined by NameBinder::len() after binding.
#[derive(Debug)]
pub struct VarStore<V> {
    /// Variable values indexed by slot ID
    values: Vec<Option<V>>,
    /// How often each slot has been written. A consumer that caches something
    /// derived from a slot's value uses this to notice a rebinding in O(1),
    /// whatever the new value happens to be allocated at; see
    /// [`generation`](Self::generation).
    generations: Vec<u64>,
    /// Which store the generations count in; see [`serial`](Self::serial).
    serial: u64,
}

/// Source of [`VarStore::serial`].
static NEXT_VAR_STORE_SERIAL: AtomicU64 = AtomicU64::new(1);

impl<V: Clone> Clone for VarStore<V> {
    /// A clone is written independently of the original, so its generations
    /// count in a store of their own.
    fn clone(&self) -> Self {
        Self {
            values: self.values.clone(),
            generations: self.generations.clone(),
            serial: NEXT_VAR_STORE_SERIAL.fetch_add(1, Ordering::Relaxed),
        }
    }
}

impl<V> VarStore<V> {
    /// Create a new variable store with the given size.
    ///
    /// The size should be NameBinder::len() after binding.
    pub fn new(size: usize) -> Self {
        let mut values = Vec::with_capacity(size);
        values.resize_with(size, || None);
        Self {
            values,
            generations: vec![0; size],
            serial: NEXT_VAR_STORE_SERIAL.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// A number no other store — live or dropped — carries.
    ///
    /// Generations count writes *within one store*, so two stores can show the
    /// same generation for a slot that holds different values. A consumer that
    /// remembers a generation remembers this serial with it: replacing the
    /// store of a [`DynamicContext`] (its `variables` field is public), by a new
    /// store or by a clone, then never looks like "nothing was written".
    #[inline]
    pub(crate) fn serial(&self) -> u64 {
        self.serial
    }

    /// Get a variable value by slot ID.
    pub fn get(&self, slot: VarSlotId) -> Option<&V> {
        self.values.get(slot as usize).and_then(|v| v.as_ref())
    }

    /// Set a variable value.
    pub fn set(&mut self, slot: VarSlotId, value: V) {
        if let Some(cell) = self.values.get_mut(slot as usize) {
            *cell = Some(value);
            self.bump(slot);
        }
    }

    /// Clear a variable slot.
    pub fn clear_slot(&mut self, slot: VarSlotId) {
        if let Some(cell) = self.values.get_mut(slot as usize) {
            *cell = None;
            self.bump(slot);
        }
    }

    /// Clear all variable values.
    pub fn clear(&mut self) {
        for cell in &mut self.values {
            *cell = None;
        }
        for generation in &mut self.generations {
            *generation = generation.wrapping_add(1);
        }
    }

    /// How often `slot` has been written since the store was created.
    ///
    /// Every write goes through [`set`](Self::set), [`clear_slot`](Self::clear_slot)
    /// or [`clear`](Self::clear), and the store hands out no mutable reference to a
    /// value, so an unchanged generation means the slot still holds the very value
    /// it held when the generation was read.
    #[inline]
    pub(crate) fn generation(&self, slot: VarSlotId) -> u64 {
        self.generations.get(slot as usize).copied().unwrap_or(0)
    }

    #[inline]
    fn bump(&mut self, slot: VarSlotId) {
        if let Some(generation) = self.generations.get_mut(slot as usize) {
            *generation = generation.wrapping_add(1);
        }
    }

    /// Get the number of slots.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Check if the store is empty.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

impl<V> Default for VarStore<V> {
    fn default() -> Self {
        Self::new(0)
    }
}

// ============================================================================
// The host's clock
// ============================================================================

/// The current instant and the host's local offset at that instant, read
/// together once per dynamic context.
#[derive(Debug, Clone, Copy)]
pub(crate) struct HostClock {
    /// The current instant.
    pub(crate) now: chrono::DateTime<chrono::Utc>,
    /// The host's local offset at `now`.
    pub(crate) offset: TimezoneOffset,
}

impl HostClock {
    fn now() -> Self {
        let now = chrono::Utc::now();
        Self {
            now,
            offset: host_offset_at(now),
        }
    }
}

/// The host's local offset (`chrono::Local`, the process's `TZ`) at `instant`,
/// in whole minutes.
pub(crate) fn host_offset_at(instant: chrono::DateTime<chrono::Utc>) -> TimezoneOffset {
    use chrono::{Offset, TimeZone};
    let seconds = chrono::Local
        .offset_from_utc_datetime(&instant.naive_utc())
        .fix()
        .local_minus_utc();
    TimezoneOffset((seconds / 60) as i16)
}

// ============================================================================
// DynamicContext (for eval-time)
// ============================================================================

/// XPath 2.0 dynamic context for expression evaluation.
///
/// The dynamic context provides runtime information:
/// - Current context item (node or atomic value)
/// - Context position and size (for predicates)
/// - Variable bindings (indexed by VarSlotId)
/// - Current date/time (stable for duration of query)
/// - Implicit timezone
///
/// # The implicit timezone and the current dateTime
///
/// F&O 1.0 §10.4 takes the implicit timezone of every date/time comparison
/// from the dynamic context: "If either operand to a comparison function on
/// date or time values does not have an (explicit) timezone then, for the
/// purpose of the operation, an implicit timezone, provided by the dynamic
/// context …, is assumed to be present as part of the value." The engine
/// reads [`implicit_timezone`](Self::implicit_timezone) in every comparison,
/// in the subtraction of two date/time values, in the one-argument
/// `fn:adjust-*-to-timezone` functions and in `fn:implicit-timezone`. When it
/// is `None`, the context falls back to the host's local offset (the `TZ` of
/// the process, through `chrono::Local`), read the first time it is needed
/// and kept for the rest of this context's evaluation, together with the
/// instant `fn:current-dateTime` reports — so both are ·stable· (F&O §16.3)
/// even across a daylight-saving change.
/// [`implicit_timezone_or_host`](Self::implicit_timezone_or_host) is the
/// timezone in force.
///
/// [`current_datetime`](Self::current_datetime), when set, is what
/// `fn:current-dateTime` returns; otherwise the function returns the current
/// instant expressed in the implicit timezone, and keeps it. A host that
/// evaluates several expressions as one unit of work sets both
/// fields on every dynamic context it builds, so that they stay the same
/// throughout.
pub struct DynamicContext<'a, N: DomNavigator> {
    /// Reference to the static context
    pub static_context: &'a XPathContext<'a>,
    /// Current context item (if any)
    pub context_item: Option<XmlItem<N>>,
    /// Current context position (1-based)
    pub context_position: usize,
    /// Current context size
    pub context_size: usize,
    /// Current date/time (stable for entire query evaluation)
    pub current_datetime: Option<DateTimeValue>,
    /// Implicit timezone; `None` falls back to the host's local offset (see
    /// [`implicit_timezone_or_host`](Self::implicit_timezone_or_host)).
    pub implicit_timezone: Option<TimezoneOffset>,
    /// Base URI for resolving relative URIs
    pub base_uri: Option<String>,
    /// The host's clock, read once: the instant `fn:current-dateTime` reports
    /// when [`current_datetime`](Self::current_datetime) is not set, and the
    /// host's offset at that instant, the implicit timezone when
    /// [`implicit_timezone`](Self::implicit_timezone) is not set.
    host_clock: std::cell::OnceCell<HostClock>,
    /// Variable bindings (indexed by VarSlotId from NameBinder)
    pub variables: VarStore<super::functions::XPathValue<N>>,
    /// Function evaluator for eval-time dispatch (None = use builtins).
    function_evaluator: Option<&'a dyn FunctionEvaluator<N>>,
    /// Host/engine state for extension functions (None = no state).
    ///
    /// See [`with_extension`](Self::with_extension).
    extension: Option<&'a dyn std::any::Any>,
    /// General-comparison indexes reused across evaluations of one comparison
    /// node during this run; see
    /// [`compare_cache`](crate::xpath::compare_cache). Empty and
    /// allocation-free until the first general comparison is evaluated, and
    /// dropped with the context.
    compare_cache: crate::xpath::compare_cache::GeneralCompareCache,
    /// Compiled regular expressions reused across the `fn:matches`,
    /// `fn:replace` and `fn:tokenize` calls of this run; see
    /// [`regex_cache`](crate::xpath::regex_cache). Empty and allocation-free
    /// until the first such call, and dropped with the context. Not used while
    /// `host_regex_cache` is set.
    regex_cache: crate::xpath::regex_cache::RunCache,
    /// A cache of compiled regular expressions the host keeps across runs and
    /// lends to this one; see [`set_regex_cache`](Self::set_regex_cache).
    host_regex_cache: Option<&'a crate::xpath::regex_cache::RegexCache>,
    /// Collations resolved through the host's
    /// [`CollationResolver`](crate::xpath::collation::CollationResolver) during
    /// this run; see [`collation`](crate::xpath::collation). Empty and
    /// allocation-free unless a non-codepoint collation is used, and dropped
    /// with the context.
    collation_cache: crate::xpath::collation::CollationCache,
    /// The warnings this evaluation raised and nobody took yet; see
    /// [`take_warnings`](Self::take_warnings).
    warnings: Vec<super::error::XPathWarning>,
    /// The sites that have warned in this evaluation: each warns once.
    warned_sites: Vec<WarningSite>,
}

/// Where a warning came from, so that it is raised once per evaluation and
/// site however often the site runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WarningSite {
    /// An operator node of an expression: the address of the expression's
    /// node arena (which tells apart expressions evaluated in one context)
    /// and the node.
    Node {
        expression: usize,
        node: crate::xpath::arena::AstNodeId,
    },
    /// A built-in function, by name.
    Function(&'static str),
}

impl<'a, N: DomNavigator> DynamicContext<'a, N> {
    /// Create a new dynamic context with the given static context.
    ///
    /// The var_count should be NameBinder::len() after binding.
    pub fn new(static_context: &'a XPathContext<'a>, var_count: usize) -> Self {
        Self {
            static_context,
            context_item: None,
            context_position: 0,
            context_size: 0,
            current_datetime: None,
            implicit_timezone: static_context.implicit_timezone,
            base_uri: static_context.base_uri.clone(),
            host_clock: std::cell::OnceCell::new(),
            variables: VarStore::new(var_count),
            function_evaluator: None,
            extension: None,
            compare_cache: Default::default(),
            regex_cache: Default::default(),
            host_regex_cache: None,
            collation_cache: Default::default(),
            warnings: Vec::new(),
            warned_sites: Vec::new(),
        }
    }

    /// Set the context item.
    pub fn with_context_item(mut self, item: XmlItem<N>) -> Self {
        self.context_item = Some(item);
        self.context_position = 1;
        self.context_size = 1;
        self
    }

    /// Set the context node.
    pub fn with_context_node(self, node: N) -> Self {
        self.with_context_item(XmlItem::Node(node))
    }

    /// Set context position and size (for predicate evaluation).
    pub fn with_position(mut self, position: usize, size: usize) -> Self {
        self.context_position = position;
        self.context_size = size;
        self
    }

    /// Set the current date/time.
    pub fn with_current_datetime(mut self, dt: DateTimeValue) -> Self {
        self.current_datetime = Some(dt);
        self
    }

    /// Set the implicit timezone.
    pub fn with_implicit_timezone(mut self, tz: TimezoneOffset) -> Self {
        self.implicit_timezone = Some(tz);
        self
    }

    /// The implicit timezone in force:
    /// [`implicit_timezone`](Self::implicit_timezone) when it is set, else the
    /// host's local offset, read once for this context (see the
    /// [type documentation](Self)).
    ///
    /// ```
    /// use xsd_schema::namespace::table::NameTable;
    /// use xsd_schema::types::value::TimezoneOffset;
    /// use xsd_schema::xpath::{DynamicContext, RoXmlNavigator, XPathContext};
    ///
    /// let names = NameTable::new();
    /// let ctx = XPathContext::new(&names);
    /// let dynamic = DynamicContext::<RoXmlNavigator<'static>>::new(&ctx, 0)
    ///     .with_implicit_timezone(TimezoneOffset::from_hm(5, 30));
    /// assert_eq!(dynamic.implicit_timezone_or_host(), TimezoneOffset::from_hm(5, 30));
    ///
    /// // Without one, the host's offset — the same value every time it is asked.
    /// let host = DynamicContext::<RoXmlNavigator<'static>>::new(&ctx, 0);
    /// assert_eq!(host.implicit_timezone_or_host(), host.implicit_timezone_or_host());
    /// ```
    pub fn implicit_timezone_or_host(&self) -> TimezoneOffset {
        match self.implicit_timezone {
            Some(tz) => tz,
            None => self.host_clock().offset,
        }
    }

    /// The warnings this evaluation has raised since they were last taken
    /// ([`XPathWarning`](super::error::XPathWarning)): each site of the
    /// expression — an operator, a function — warns once per evaluation however
    /// often it runs. An evaluation through
    /// [`XPathEvaluator`](crate::xpath::XPathEvaluator) hands them to the
    /// collector of [`with_warnings`](crate::xpath::XPathEvaluator::with_warnings);
    /// a host that drives [`eval_node`](crate::xpath::eval_node) with its own
    /// dynamic context takes them here. Taking them ends the evaluation for
    /// this purpose: a site warns again afterwards.
    pub fn take_warnings(&mut self) -> Vec<super::error::XPathWarning> {
        self.warned_sites.clear();
        std::mem::take(&mut self.warnings)
    }

    /// Raise the warning `warning` builds, unless `site` has warned in this
    /// evaluation already.
    pub(crate) fn warn_once(
        &mut self,
        site: WarningSite,
        warning: impl FnOnce() -> super::error::XPathWarning,
    ) {
        if !self.warned_sites.contains(&site) {
            self.warned_sites.push(site);
            self.warnings.push(warning());
        }
    }

    /// The host's clock, read the first time it is needed.
    pub(crate) fn host_clock(&self) -> &HostClock {
        self.host_clock.get_or_init(HostClock::now)
    }

    /// Get the context item, returning an error if undefined.
    pub fn require_context_item(&self) -> Result<&XmlItem<N>, super::error::XPathError> {
        self.context_item
            .as_ref()
            .ok_or_else(|| super::error::XPathError::XPDY0002 {
                message: "Context item is undefined".to_string(),
            })
    }

    /// Get the context node, returning an error if undefined or not a node.
    ///
    /// Returns XPTY0020 if the context item is not a node (per XPath 2.0 spec for axis steps).
    pub fn require_context_node(&self) -> Result<&N, super::error::XPathError> {
        match self.context_item.as_ref() {
            Some(XmlItem::Node(node)) => Ok(node),
            Some(XmlItem::Atomic(_)) => Err(super::error::XPathError::XPTY0020),
            None => Err(super::error::XPathError::XPDY0002 {
                message: "Context item is undefined".to_string(),
            }),
        }
    }

    /// The general-comparison index cache of this run.
    #[inline]
    pub(crate) fn general_compare_cache(
        &self,
    ) -> &crate::xpath::compare_cache::GeneralCompareCache {
        &self.compare_cache
    }

    /// The general-comparison index cache of this run, mutably.
    #[inline]
    pub(crate) fn general_compare_cache_mut(
        &mut self,
    ) -> &mut crate::xpath::compare_cache::GeneralCompareCache {
        &mut self.compare_cache
    }

    /// The compiled regular expressions of this run.
    #[inline]
    pub(crate) fn regex_cache_mut(&mut self) -> &mut crate::xpath::regex_cache::RunCache {
        &mut self.regex_cache
    }

    /// The compiled regular expressions of this run. Test-only: the shipped
    /// path never reads the cache without also being allowed to fill it.
    #[cfg(test)]
    #[inline]
    pub(crate) fn regex_cache(&self) -> &crate::xpath::regex_cache::RunCache {
        &self.regex_cache
    }

    /// The cache of compiled regular expressions the host lent this run, if any.
    #[inline]
    pub(crate) fn host_regex_cache(&self) -> Option<&'a crate::xpath::regex_cache::RegexCache> {
        self.host_regex_cache
    }

    /// Lend this run a cache of compiled regular expressions that the host keeps
    /// across runs (builder form).
    ///
    /// The builder counterpart of [`set_regex_cache`](Self::set_regex_cache), for
    /// code that constructs its own `DynamicContext`; see there.
    pub fn with_regex_cache(mut self, cache: &'a crate::xpath::RegexCache) -> Self {
        self.host_regex_cache = Some(cache);
        self
    }

    /// Lend this run a cache of compiled regular expressions that the host keeps
    /// across runs, or — with `None` — go back to the run's own cache.
    ///
    /// Without one, a run keeps the programs `fn:matches`, `fn:replace` and
    /// `fn:tokenize` compile for as long as the run lasts, so a host that runs an
    /// expression once per item compiles its patterns once per item. With one,
    /// the run takes its programs from the host's
    /// [`RegexCache`](crate::xpath::RegexCache) and leaves the ones it compiles
    /// there for the next run; its own cache stays empty. This is the
    /// mutable-reference form, for the
    /// [`run_with`](crate::xpath::XPathEvaluator::run_with) setup callback;
    /// outside one,
    /// [`XPathEvaluator::with_regex_cache`](crate::xpath::XPathEvaluator::with_regex_cache)
    /// does the same.
    ///
    /// The reference is `&'a`, like [`set_extension`](Self::set_extension)'s: the
    /// cache has to outlive this context, which a cache the host keeps across
    /// runs does by definition.
    ///
    /// # Example
    ///
    /// ```
    /// use xsd_schema::namespace::table::NameTable;
    /// use xsd_schema::xpath::{RegexCache, RoXmlNavigator, XPathContext, XPathExpr};
    ///
    /// let names = NameTable::new();
    /// let ctx = XPathContext::new(&names);
    /// let expr = XPathExpr::compile("tokenize('a, b,c', ',\\s*')", &ctx).unwrap();
    ///
    /// let regexes = RegexCache::new();
    /// for _ in 0..3 {
    ///     let tokens = expr
    ///         .evaluator(&ctx)
    ///         .run_with::<RoXmlNavigator<'static>, _>(|eval| {
    ///             eval.context().set_regex_cache(Some(&regexes));
    ///         })
    ///         .unwrap();
    ///     assert_eq!(tokens.into_vec().len(), 3);
    /// }
    /// assert_eq!(regexes.len(), 1);
    /// ```
    pub fn set_regex_cache(&mut self, cache: Option<&'a crate::xpath::RegexCache>) {
        self.host_regex_cache = cache;
    }

    /// The collations resolved during this run.
    #[inline]
    pub(crate) fn collation_cache_mut(&mut self) -> &mut crate::xpath::collation::CollationCache {
        &mut self.collation_cache
    }

    /// The static context's default collation for one comparison: resolved —
    /// through this run's memo — only when that comparison actually compares
    /// two strings. See [`DeferredDefault`](crate::xpath::collation::DeferredDefault).
    #[inline]
    pub(crate) fn deferred_default_collation(
        &mut self,
    ) -> crate::xpath::collation::DeferredDefault<'_> {
        let static_context = self.static_context;
        crate::xpath::collation::DeferredDefault::with_memo(
            static_context,
            &mut self.collation_cache,
        )
    }

    /// The general-comparison index cache and the deferred default collation at
    /// once, for a probe that needs both.
    #[inline]
    pub(crate) fn compare_cache_and_default_collation(
        &mut self,
    ) -> (
        &mut crate::xpath::compare_cache::GeneralCompareCache,
        crate::xpath::collation::DeferredDefault<'_>,
    ) {
        let static_context = self.static_context;
        (
            &mut self.compare_cache,
            crate::xpath::collation::DeferredDefault::with_memo(
                static_context,
                &mut self.collation_cache,
            ),
        )
    }

    /// The collations resolved during this run. Test-only, like
    /// [`regex_cache`](Self::regex_cache).
    #[cfg(test)]
    #[inline]
    pub(crate) fn collation_cache(&self) -> &crate::xpath::collation::CollationCache {
        &self.collation_cache
    }

    /// Get a variable value by slot ID.
    pub fn get_variable(&self, slot: VarSlotId) -> Option<&super::functions::XPathValue<N>> {
        self.variables.get(slot)
    }

    /// How often the variable in `slot` has been written; see
    /// [`VarStore::generation`].
    #[inline]
    pub(crate) fn variable_generation(&self, slot: VarSlotId) -> u64 {
        self.variables.generation(slot)
    }

    /// The [serial](VarStore::serial) of the store the variables live in now.
    #[inline]
    pub(crate) fn variable_store_serial(&self) -> u64 {
        self.variables.serial()
    }

    /// Set a variable value.
    pub fn set_variable(&mut self, slot: VarSlotId, value: super::functions::XPathValue<N>) {
        self.variables.set(slot, value);
    }

    /// Set the function evaluator for custom function support.
    pub fn with_function_evaluator(mut self, evaluator: &'a dyn FunctionEvaluator<N>) -> Self {
        self.function_evaluator = Some(evaluator);
        self
    }

    /// Set or clear the function evaluator on an existing dynamic context.
    ///
    /// The mutable-reference counterpart of
    /// [`with_function_evaluator`](Self::with_function_evaluator), for contexts that are
    /// already built. This is what makes custom functions reachable through the high-level
    /// API: [`XPathEvaluator::run_with`](crate::xpath::XPathEvaluator::run_with) and
    /// [`run_with_node_and_setup`](crate::xpath::XPathEvaluator::run_with_node_and_setup)
    /// create the dynamic context themselves and hand it to the setup callback through
    /// [`TypedEvaluator::context`](crate::xpath::TypedEvaluator::context), where only a
    /// `&mut DynamicContext` is available. Passing `None` restores built-in dispatch.
    ///
    /// The evaluator must be the one whose
    /// [`FunctionCatalog`] was used to compile the
    /// expression — handles from one catalog are meaningless to another. With
    /// [`FunctionSet`](crate::xpath::functions::FunctionSet) the same value is both, so pass
    /// it to [`XPathContext::with_function_catalog`] and to this method.
    ///
    /// # Example
    ///
    /// See [`set_extension`](Self::set_extension) for a complete `run_with` example that
    /// installs a `FunctionSet` and the state its function reads.
    pub fn set_function_evaluator(&mut self, evaluator: Option<&'a dyn FunctionEvaluator<N>>) {
        self.function_evaluator = evaluator;
    }

    /// Get the function evaluator, using built-in functions as default.
    ///
    /// Returns a reference to the configured evaluator, or `BuiltinEvaluator` if none set.
    pub fn function_evaluator(&self) -> &dyn FunctionEvaluator<N> {
        static BUILTIN: BuiltinEvaluator = BuiltinEvaluator;
        self.function_evaluator.unwrap_or(&BUILTIN)
    }

    /// Check if a custom function evaluator is configured.
    pub fn has_custom_evaluator(&self) -> bool {
        self.function_evaluator.is_some()
    }

    /// Attach host state for extension functions (builder form).
    ///
    /// [`FunctionEvaluator::eval`] takes
    /// `&self`, so a custom function cannot keep mutable state on the evaluator itself. The
    /// extension slot is the supported way to give it per-run engine or host state without
    /// changing that contract: the host stores a `&dyn Any` on the dynamic context, and the
    /// function reads it back with [`extension`](Self::extension), using interior mutability
    /// (`Cell`, `RefCell`, atomics) for whatever it needs to update.
    ///
    /// The slot holds a single value. Hosts that need several pieces of state put them in one
    /// struct — that also keeps the downcast target unambiguous.
    ///
    /// This is the builder form, for code that constructs its own `DynamicContext`. Through
    /// the high-level API the dynamic context is created for you, so use
    /// [`set_extension`](Self::set_extension) inside the
    /// [`run_with`](crate::xpath::XPathEvaluator::run_with) setup callback instead — its
    /// example shows the complete public-API flow.
    ///
    /// # Lifetime
    ///
    /// The reference is `&'a`, the same lifetime parameter as the borrowed
    /// [`XPathContext`], so the state lives *alongside* the static context: it has to
    /// outlive this `DynamicContext`, which means an enclosing scope (or a leak, for
    /// `&'static`) — not a temporary created in the evaluating call. The whole expression
    /// evaluates against one dynamic context — sub-expressions (predicates, path steps,
    /// `for` and quantified bodies, nested calls) reuse it and only save/restore the focus —
    /// so a value set here is visible to every `eval` call the expression makes.
    ///
    /// # Example
    ///
    /// A custom function that counts its own calls through the extension slot, driven from
    /// the low-level `parse`/`bind_node`/`eval_node` path; the expression body runs the
    /// function once per `for` iteration:
    ///
    /// ```
    /// use std::cell::Cell;
    /// use xsd_schema::namespace::context::NamespaceContextSnapshot;
    /// use xsd_schema::namespace::table::NameTable;
    /// use xsd_schema::types::sequence::SequenceType;
    /// use xsd_schema::xpath::functions::{DynamicFunctionSignature, FunctionSet, XPathValue};
    /// use xsd_schema::xpath::{
    ///     bind_node, eval_node, parse, DynamicContext, NameBinder, RoXmlNavigator, XPathContext,
    /// };
    ///
    /// /// Host state: how many times the extension function ran.
    /// struct CallCounter {
    ///     calls: Cell<u32>,
    /// }
    ///
    /// let mut functions: FunctionSet<RoXmlNavigator<'static>> = FunctionSet::with_builtins();
    /// functions.register(
    ///     DynamicFunctionSignature::new(
    ///         "http://example.com/ext",
    ///         "tick",
    ///         vec![],
    ///         SequenceType::integer(),
    ///     ),
    ///     |ctx, _args| {
    ///         // Read the host state back out of the dynamic context.
    ///         let counter = ctx.extension::<CallCounter>().expect("counter installed");
    ///         counter.calls.set(counter.calls.get() + 1);
    ///         Ok(XPathValue::integer(counter.calls.get() as i64))
    ///     },
    /// );
    ///
    /// let names = NameTable::new();
    /// let mut namespaces = NamespaceContextSnapshot::default();
    /// namespaces
    ///     .bindings
    ///     .push((names.add("ext"), names.add("http://example.com/ext")));
    /// let ctx = XPathContext::new(&names)
    ///     .with_namespaces(namespaces)
    ///     .with_function_catalog(&functions);
    ///
    /// // The state lives alongside the context it is attached to.
    /// let counter = CallCounter { calls: Cell::new(0) };
    ///
    /// let mut parsed = parse("for $i in (1, 2, 3) return ext:tick()").unwrap();
    /// let mut binder = NameBinder::new();
    /// bind_node(&mut parsed.arena, parsed.root, &ctx, &mut binder).unwrap();
    ///
    /// let mut dyn_ctx: DynamicContext<'_, RoXmlNavigator<'static>> =
    ///     DynamicContext::new(&ctx, binder.len())
    ///         .with_function_evaluator(&functions)
    ///         .with_extension(&counter);
    /// eval_node(&parsed.arena, parsed.root, &mut dyn_ctx).unwrap();
    ///
    /// assert_eq!(counter.calls.get(), 3);
    /// ```
    pub fn with_extension(mut self, ext: &'a dyn std::any::Any) -> Self {
        self.extension = Some(ext);
        self
    }

    /// Set or clear the extension slot on an existing dynamic context.
    ///
    /// The mutable-reference counterpart of [`with_extension`](Self::with_extension), for
    /// contexts that are already built — for example inside a
    /// [`run_with`](crate::xpath::XPathEvaluator::run_with) setup callback, which hands out
    /// `&mut DynamicContext` through
    /// [`TypedEvaluator::context`](crate::xpath::TypedEvaluator::context). Passing `None`
    /// clears the slot.
    ///
    /// # Example
    ///
    /// The whole flow through the high-level API: one
    /// [`FunctionSet`](crate::xpath::functions::FunctionSet) serves as the compile-time
    /// catalog and the eval-time evaluator, and the state it reads is a plain local — the
    /// setup callback borrows the caller's frame, no `'static` and no leak.
    ///
    /// ```
    /// use std::cell::Cell;
    /// use xsd_schema::namespace::context::NamespaceContextSnapshot;
    /// use xsd_schema::namespace::table::NameTable;
    /// use xsd_schema::types::sequence::SequenceType;
    /// use xsd_schema::xpath::api::XPathExpr;
    /// use xsd_schema::xpath::functions::{DynamicFunctionSignature, FunctionSet, XPathValue};
    /// use xsd_schema::xpath::{RoXmlNavigator, XPathContext};
    ///
    /// /// Host state: how many times the extension function ran.
    /// struct CallCounter {
    ///     calls: Cell<u32>,
    /// }
    ///
    /// let mut functions: FunctionSet<RoXmlNavigator<'static>> = FunctionSet::with_builtins();
    /// functions.register(
    ///     DynamicFunctionSignature::new(
    ///         "http://example.com/ext",
    ///         "tick",
    ///         vec![],
    ///         SequenceType::integer(),
    ///     ),
    ///     |ctx, _args| {
    ///         let counter = ctx.extension::<CallCounter>().expect("counter installed");
    ///         counter.calls.set(counter.calls.get() + 1);
    ///         Ok(XPathValue::integer(counter.calls.get() as i64))
    ///     },
    /// );
    ///
    /// let names = NameTable::new();
    /// let mut namespaces = NamespaceContextSnapshot::default();
    /// namespaces
    ///     .bindings
    ///     .push((names.add("ext"), names.add("http://example.com/ext")));
    /// let ctx = XPathContext::new(&names)
    ///     .with_namespaces(namespaces)
    ///     .with_function_catalog(&functions);
    ///
    /// let expr = XPathExpr::compile("for $i in (1, 2, 3) return ext:tick()", &ctx).unwrap();
    ///
    /// // A local on the stack — not leaked, not `'static`.
    /// let counter = CallCounter { calls: Cell::new(0) };
    ///
    /// let result = expr
    ///     .evaluator(&ctx)
    ///     .run_with::<RoXmlNavigator<'static>, _>(|eval| {
    ///         let dyn_ctx = eval.context();
    ///         dyn_ctx.set_function_evaluator(Some(&functions));
    ///         dyn_ctx.set_extension(Some(&counter));
    ///     })
    ///     .unwrap();
    ///
    /// assert_eq!(result.into_vec().len(), 3);
    /// assert_eq!(counter.calls.get(), 3);
    /// ```
    pub fn set_extension(&mut self, ext: Option<&'a dyn std::any::Any>) {
        self.extension = ext;
    }

    /// Read the extension slot, downcast to `T`.
    ///
    /// Returns `None` if no extension is set or if the stored value is not a `T`; see
    /// [`with_extension`](Self::with_extension) for the intended use and lifetime.
    pub fn extension<T: std::any::Any>(&self) -> Option<&'a T> {
        self.extension.and_then(|ext| ext.downcast_ref::<T>())
    }

    /// Evaluate a function using the configured evaluator.
    ///
    /// Calls the evaluator set with
    /// [`with_function_evaluator`](Self::with_function_evaluator) (or the built-in
    /// functions when none is set), passing this context mutably. Prefer it to
    /// `self.function_evaluator().eval(handle, self, args)`, which does not compile:
    /// [`function_evaluator`](Self::function_evaluator) borrows `self` for the
    /// evaluator it returns.
    pub fn eval_function(
        &mut self,
        handle: super::functions::FunctionHandle,
        args: Vec<super::functions::XPathValue<N>>,
    ) -> Result<super::functions::XPathValue<N>, super::error::XPathError> {
        // Route through the custom evaluator (e.g. XPath10Evaluator intercepts builtins).
        // The field holds a `Copy` reference with the lifetime `'a`, not a borrow
        // of `self`, so copying it out leaves `self` free to be passed mutably.
        match self.function_evaluator {
            Some(evaluator) => evaluator.eval(handle, self, args),
            // Built-in handles, and custom handles without a configured evaluator.
            None => BuiltinEvaluator.eval(handle, self, args),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_var_store() {
        let mut store: VarStore<i32> = VarStore::new(3);

        assert!(store.get(0).is_none());
        store.set(0, 42);
        assert_eq!(store.get(0), Some(&42));

        store.set(1, 100);
        assert_eq!(store.get(1), Some(&100));

        store.clear_slot(0);
        assert!(store.get(0).is_none());

        store.clear();
        assert!(store.get(1).is_none());
    }

    #[test]
    fn test_xpath_context_default_function_ns() {
        let names = NameTable::new();
        let ctx = XPathContext::new(&names);
        assert_eq!(
            ctx.default_function_namespace(),
            super::super::functions::FN_NAMESPACE
        );
    }

    #[test]
    fn test_name_binder_push_pop() {
        let names = NameTable::new();
        let mut binder = NameBinder::new();
        assert!(binder.is_empty());
        assert_eq!(binder.len(), 0);

        let x_id = names.add("x");
        let y_id = names.add("y");

        let name1 = QualifiedName::local(x_id);
        let ref1 = binder.push_var(name1.clone());
        assert_eq!(ref1.slot, 0);
        assert_eq!(binder.len(), 1);

        let name2 = QualifiedName::local(y_id);
        let ref2 = binder.push_var(name2.clone());
        assert_eq!(ref2.slot, 1);
        assert_eq!(binder.len(), 2);

        // Resolve should find the variables
        let resolved1 = binder.resolve(&name1).unwrap();
        assert_eq!(resolved1.slot, 0);

        let resolved2 = binder.resolve(&name2).unwrap();
        assert_eq!(resolved2.slot, 1);

        // Pop y, x should still be resolvable
        binder.pop_var();
        let resolved1_again = binder.resolve(&name1).unwrap();
        assert_eq!(resolved1_again.slot, 0);

        // y should not be resolvable after pop
        let err = binder.resolve(&name2);
        assert!(err.is_err());
    }

    #[test]
    fn test_name_binder_shadowing() {
        let names = NameTable::new();
        let mut binder = NameBinder::new();

        let x_id = names.add("x");
        let name = QualifiedName::local(x_id);

        // Push x (slot 0)
        let ref1 = binder.push_var(name.clone());
        assert_eq!(ref1.slot, 0);

        // Push x again (slot 1, shadows slot 0)
        let ref2 = binder.push_var(name.clone());
        assert_eq!(ref2.slot, 1);

        // Resolve should find the shadowing slot
        let resolved = binder.resolve(&name).unwrap();
        assert_eq!(resolved.slot, 1);

        // Pop the shadow, should now resolve to original
        binder.pop_var();
        let resolved_after_pop = binder.resolve(&name).unwrap();
        assert_eq!(resolved_after_pop.slot, 0);
    }

    #[test]
    fn test_name_binder_unbound_error() {
        let names = NameTable::new();
        let binder = NameBinder::new();
        let undefined_id = names.add("undefined");
        let name = QualifiedName::local(undefined_id);
        let result = binder.resolve(&name);
        assert!(matches!(
            result,
            Err(super::super::error::XPathError::XPST0008 { .. })
        ));
    }
}

// ============================================================================
// Tests: DynamicContext extension slot, owned default function namespace
// ============================================================================

#[cfg(test)]
mod extension_slot_tests {
    use super::{DynamicContext, NameBinder, XPathContext};
    use crate::namespace::context::NamespaceContextSnapshot;
    use crate::namespace::table::NameTable;
    use crate::types::sequence::SequenceType;
    use crate::xpath::api::XPathExpr;
    use crate::xpath::bind::bind_node;
    use crate::xpath::eval::eval_node;
    use crate::xpath::functions::{DynamicFunctionSignature, FunctionSet, XPathValue};
    use crate::xpath::parser::parse;
    use crate::xpath::{DomNavigator, RoXmlNavigator, XPathMode};
    use std::cell::Cell;

    type Nav = RoXmlNavigator<'static>;

    const EXT_NS: &str = "http://example.com/ext";

    /// Host state reached through the extension slot: `FunctionEvaluator::eval` takes
    /// `&self`, so the mutable state lives in a `Cell` behind the shared reference.
    #[derive(Default)]
    struct CallCounter {
        calls: Cell<u32>,
    }

    /// A second `Any` type, to check that `extension::<T>()` is type-checked.
    struct OtherState;

    /// A function set with `my:tick()`: increments the counter found in the extension
    /// slot and returns its new value (0 when no counter is installed).
    fn tick_functions<N: DomNavigator>() -> FunctionSet<N> {
        let mut functions: FunctionSet<N> = FunctionSet::with_builtins();
        functions.register(
            DynamicFunctionSignature::new(EXT_NS, "tick", vec![], SequenceType::integer()),
            |ctx, _args| match ctx.extension::<CallCounter>() {
                Some(counter) => {
                    counter.calls.set(counter.calls.get() + 1);
                    Ok(XPathValue::integer(i64::from(counter.calls.get())))
                }
                None => Ok(XPathValue::integer(0)),
            },
        );
        functions
    }

    /// Bind the `my` prefix for `EXT_NS` in a fresh snapshot.
    fn ext_namespaces(names: &NameTable) -> NamespaceContextSnapshot {
        let mut namespaces = NamespaceContextSnapshot::default();
        namespaces
            .bindings
            .push((names.add("my"), names.add(EXT_NS)));
        namespaces
    }

    /// Compile and evaluate `expr` with `my:tick()` available and `counter` in the
    /// extension slot.
    fn eval_with_counter(expr: &str, counter: &CallCounter) -> XPathValue<Nav> {
        let names = NameTable::new();
        let functions: FunctionSet<Nav> = tick_functions();
        let ctx = XPathContext::new(&names)
            .with_namespaces(ext_namespaces(&names))
            .with_function_catalog(&functions);

        let mut parsed = parse(expr).unwrap();
        let mut binder = NameBinder::new();
        bind_node(&mut parsed.arena, parsed.root, &ctx, &mut binder).unwrap();

        let mut dyn_ctx: DynamicContext<'_, Nav> = DynamicContext::new(&ctx, binder.len())
            .with_function_evaluator(&functions)
            .with_extension(counter);

        eval_node(&parsed.arena, parsed.root, &mut dyn_ctx).unwrap()
    }

    #[test]
    fn test_extension_visible_in_for_body() {
        let counter = CallCounter::default();
        let result = eval_with_counter("for $i in (1, 2, 3) return my:tick()", &counter);
        assert_eq!(counter.calls.get(), 3);
        // The slot is the *same* state across iterations, so the values increase.
        let values: Vec<String> = result
            .into_vec()
            .into_iter()
            .map(|item| {
                XPathValue::<Nav>::from_item(item)
                    .as_integer()
                    .expect("integer result")
                    .to_string()
            })
            .collect();
        assert_eq!(values, vec!["1", "2", "3"]);
    }

    #[test]
    fn test_extension_visible_in_predicate() {
        let counter = CallCounter::default();
        let result = eval_with_counter("(1, 2, 3)[my:tick() > 0]", &counter);
        // One predicate evaluation per input item.
        assert_eq!(counter.calls.get(), 3);
        assert_eq!(result.into_vec().len(), 3);
    }

    #[test]
    fn test_extension_visible_in_quantified_body() {
        let counter = CallCounter::default();
        let result = eval_with_counter("some $i in (1, 2) satisfies my:tick() > 0", &counter);
        assert_eq!(result.as_bool(), Some(true));
        // `some` short-circuits on the first true item, so exactly one call happened.
        assert_eq!(counter.calls.get(), 1);

        // `every` has to visit both items.
        let counter = CallCounter::default();
        let result = eval_with_counter("every $i in (1, 2) satisfies my:tick() > 0", &counter);
        assert_eq!(result.as_bool(), Some(true));
        assert_eq!(counter.calls.get(), 2);
    }

    #[test]
    fn test_extension_visible_in_nested_calls() {
        let counter = CallCounter::default();
        let result = eval_with_counter("my:tick() + my:tick()", &counter);
        assert_eq!(counter.calls.get(), 2);
        // 1 + 2 — both calls saw the same counter.
        assert_eq!(result.as_integer().map(|i| i.to_string()), Some("3".into()));
    }

    #[test]
    fn test_extension_without_counter_installed() {
        // The same function set, but no extension in the context: the function sees None.
        let names = NameTable::new();
        let functions: FunctionSet<Nav> = tick_functions();
        let ctx = XPathContext::new(&names)
            .with_namespaces(ext_namespaces(&names))
            .with_function_catalog(&functions);

        let mut parsed = parse("my:tick()").unwrap();
        let mut binder = NameBinder::new();
        bind_node(&mut parsed.arena, parsed.root, &ctx, &mut binder).unwrap();

        let mut dyn_ctx: DynamicContext<'_, Nav> =
            DynamicContext::new(&ctx, binder.len()).with_function_evaluator(&functions);
        assert!(dyn_ctx.extension::<CallCounter>().is_none());

        let result = eval_node(&parsed.arena, parsed.root, &mut dyn_ctx).unwrap();
        assert_eq!(result.as_integer().map(|i| i.to_string()), Some("0".into()));
    }

    #[test]
    fn test_extension_downcast_is_type_checked() {
        let names = NameTable::new();
        let ctx = XPathContext::new(&names);
        let counter = CallCounter::default();

        let dyn_ctx: DynamicContext<'_, Nav> =
            DynamicContext::new(&ctx, 0).with_extension(&counter);

        assert!(dyn_ctx.extension::<CallCounter>().is_some());
        assert!(dyn_ctx.extension::<OtherState>().is_none());
    }

    #[test]
    fn test_set_extension_installs_and_clears() {
        let names = NameTable::new();
        let ctx = XPathContext::new(&names);
        let counter = CallCounter::default();

        let mut dyn_ctx: DynamicContext<'_, Nav> = DynamicContext::new(&ctx, 0);
        assert!(dyn_ctx.extension::<CallCounter>().is_none());

        dyn_ctx.set_extension(Some(&counter));
        assert!(dyn_ctx.extension::<CallCounter>().is_some());

        dyn_ctx.set_extension(None);
        assert!(dyn_ctx.extension::<CallCounter>().is_none());
    }

    #[test]
    fn test_default_function_ns_owned_binds_unprefixed_call() {
        // A namespace that only exists at runtime — no `&'static str` available.
        let runtime_ns = format!("http://example.com/{}/ext", "runtime");

        let mut functions: FunctionSet<Nav> = FunctionSet::with_builtins();
        functions.register(
            DynamicFunctionSignature::new(
                runtime_ns.as_str(),
                "answer",
                vec![],
                SequenceType::integer(),
            ),
            |_ctx, _args| Ok(XPathValue::integer(42)),
        );

        let names = NameTable::new();
        let ctx = XPathContext::new(&names)
            .with_default_function_ns_owned(runtime_ns.clone())
            .with_function_catalog(&functions);
        assert_eq!(ctx.default_function_namespace(), runtime_ns);

        // Unprefixed call binds against the owned default function namespace.
        let mut parsed = parse("answer()").unwrap();
        let mut binder = NameBinder::new();
        bind_node(&mut parsed.arena, parsed.root, &ctx, &mut binder).unwrap();

        let mut dyn_ctx: DynamicContext<'_, Nav> =
            DynamicContext::new(&ctx, binder.len()).with_function_evaluator(&functions);
        let result = eval_node(&parsed.arena, parsed.root, &mut dyn_ctx).unwrap();
        assert_eq!(
            result.as_integer().map(|i| i.to_string()),
            Some("42".into())
        );
    }

    #[test]
    fn test_default_function_ns_owned_wins_over_static_field() {
        let names = NameTable::new();

        let ctx = XPathContext::new(&names)
            .with_default_function_ns("http://example.com/static")
            .with_default_function_ns_owned(format!("http://example.com/{}", "owned"));
        assert_eq!(ctx.default_function_namespace(), "http://example.com/owned");
        // The public field keeps the value it was given.
        assert_eq!(ctx.default_function_ns, Some("http://example.com/static"));

        // Order of the two setters does not matter.
        let ctx = XPathContext::new(&names)
            .with_default_function_ns_owned("http://example.com/owned")
            .with_default_function_ns("http://example.com/static");
        assert_eq!(ctx.default_function_namespace(), "http://example.com/owned");

        // Without the owned value the static field still wins over fn:.
        let ctx = XPathContext::new(&names).with_default_function_ns("http://example.com/static");
        assert_eq!(
            ctx.default_function_namespace(),
            "http://example.com/static"
        );

        // XPath 1.0 mode still yields the empty namespace (neither setter overrides it).
        let ctx = XPathContext::new(&names)
            .with_mode(XPathMode::XPath10)
            .with_default_function_ns_owned("http://example.com/owned");
        assert_eq!(ctx.default_function_namespace(), "");
    }

    #[test]
    fn test_public_api_run_with_evaluator_and_extension() {
        // Everything through the public API: one FunctionSet is both the compile-time
        // catalog and the eval-time evaluator, installed from the setup callback.
        let names = NameTable::new();
        let functions: FunctionSet<Nav> = tick_functions();
        let ctx = XPathContext::new(&names)
            .with_namespaces(ext_namespaces(&names))
            .with_function_catalog(&functions);

        let expr = XPathExpr::compile("for $i in (1, 2, 3) return my:tick()", &ctx).unwrap();

        // Non-`'static` local state: lives on this stack frame, never leaked.
        let counter = CallCounter::default();

        let result = expr
            .evaluator(&ctx)
            .run_with::<Nav, _>(|te| {
                let dyn_ctx = te.context();
                dyn_ctx.set_function_evaluator(Some(&functions));
                dyn_ctx.set_extension(Some(&counter));
            })
            .unwrap();

        assert_eq!(counter.calls.get(), 3);
        assert_eq!(result.into_vec().len(), 3);
    }

    #[test]
    fn test_public_api_run_with_node_and_setup_extension() {
        // Same, through `run_with_node_and_setup`, with a context node and a predicate:
        // the borrowed state also reaches per-item predicate evaluation.
        let doc = roxmltree::Document::parse("<root><item/><item/></root>").unwrap();
        let mut nav = RoXmlNavigator::new(&doc);
        nav.move_to_first_child(); // <root>

        let names = NameTable::new();
        let functions: FunctionSet<RoXmlNavigator<'_>> = tick_functions();
        let ctx = XPathContext::new(&names)
            .with_namespaces(ext_namespaces(&names))
            .with_function_catalog(&functions);

        let expr = XPathExpr::compile("item[my:tick() > 0]", &ctx).unwrap();

        let counter = CallCounter::default();

        let result = expr
            .evaluator(&ctx)
            .run_with_node_and_setup(Some(nav), |te| {
                let dyn_ctx = te.context();
                dyn_ctx.set_function_evaluator(Some(&functions));
                dyn_ctx.set_extension(Some(&counter));
            })
            .unwrap();

        assert_eq!(counter.calls.get(), 2);
        assert_eq!(result.into_vec().len(), 2);
    }

    #[test]
    fn test_set_function_evaluator_installs_and_clears() {
        let names = NameTable::new();
        let ctx = XPathContext::new(&names);
        let functions: FunctionSet<Nav> = tick_functions();

        let mut dyn_ctx: DynamicContext<'_, Nav> = DynamicContext::new(&ctx, 0);
        assert!(!dyn_ctx.has_custom_evaluator());

        dyn_ctx.set_function_evaluator(Some(&functions));
        assert!(dyn_ctx.has_custom_evaluator());

        dyn_ctx.set_function_evaluator(None);
        assert!(!dyn_ctx.has_custom_evaluator());
    }

    #[test]
    fn test_xpath_expr_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        assert_send::<crate::xpath::api::XPathExpr>();
        assert_sync::<crate::xpath::api::XPathExpr>();
    }
}

/// XPath 1.0 *compatibility mode*: a static-context flag, not a language mode.
#[cfg(test)]
mod xpath10_compatibility_tests {
    use super::*;

    // ── XPath 1.0 compatibility mode (a static-context flag, not a language mode)

    /// The flag is off by default and `XPathMode::XPath10` implies it.
    #[test]
    fn xpath10_compatibility_defaults_to_off_and_the_mode_implies_it() {
        let names = NameTable::new();
        assert!(!XPathContext::new(&names).xpath10_compatibility());
        assert!(XPathContext::new(&names)
            .with_xpath10_compatibility(true)
            .xpath10_compatibility());
        assert!(XPathContext::new(&names)
            .with_mode(XPathMode::XPath10)
            .xpath10_compatibility());
        // The flag does not change the language mode.
        assert_eq!(
            XPathContext::new(&names)
                .with_xpath10_compatibility(true)
                .mode(),
            XPathMode::XPath20
        );
    }

    /// The flag keeps XPath 2.0 syntax, which `XPathMode::XPath10` rejects.
    #[test]
    fn xpath10_compatibility_keeps_xpath20_syntax() {
        use crate::xpath::api::XPathExpr;
        use crate::xpath::RoXmlNavigator;
        let names = NameTable::new();

        let compat = XPathContext::new(&names).with_xpath10_compatibility(true);
        assert!(XPathExpr::compile("(1, 2, 3)", &compat).is_ok());
        assert!(XPathExpr::compile("for $i in 1 to 3 return $i", &compat).is_ok());

        // `XPathMode::XPath10` refuses the same expression.
        let mode10 = XPathContext::new(&names).with_mode(XPathMode::XPath10);
        let rejected = XPathExpr::compile("(1, 2, 3)", &mode10)
            .and_then(|e| e.evaluator(&mode10).run::<RoXmlNavigator<'static>>())
            .err()
            .expect("XPath 1.0 has no sequence expressions");
        assert_eq!(rejected.error_code(), Some("XPST0003"));
    }

    /// The semantics the flag switches: 1.0 arithmetic and the 1.0 first-item
    /// conversion of `fn:string`/`fn:number` — and the one it does not: the
    /// effective boolean value.
    #[test]
    fn xpath10_compatibility_switches_the_semantics() {
        use crate::xpath::api::XPathExpr;
        use crate::xpath::RoXmlNavigator;
        let names = NameTable::new();
        let plain = XPathContext::new(&names);
        let compat = XPathContext::new(&names).with_xpath10_compatibility(true);

        let run = |src: &str, ctx: &XPathContext<'_>| {
            XPathExpr::compile(src, ctx)
                .unwrap()
                .evaluator(ctx)
                .run::<RoXmlNavigator<'static>>()
        };

        // Arithmetic: 1.0 divides by zero to infinity, 2.0 raises FOAR0001
        // for integers.
        assert!(run("1 div 0", &plain).is_err());
        assert!(run("1 div 0", &compat)
            .unwrap()
            .as_f64()
            .unwrap()
            .is_infinite());

        // The effective boolean value is XPath 2.0's (§2.4.3) with the flag
        // either way: a sequence of two atomic values is FORG0006.
        assert!(run("if (('a', 'b')) then 1 else 2", &plain).is_err());
        assert_eq!(
            run("('a', 'b') and true()", &compat)
                .err()
                .and_then(|e| e.error_code()),
            Some("FORG0006")
        );

        // fn:string / fn:number take the first item of a sequence.
        assert!(run("string(('a', 'b'))", &plain).is_err());
        assert_eq!(
            run("string(('a', 'b'))", &compat)
                .unwrap()
                .to_owned_string()
                .as_deref(),
            Some("a")
        );
        assert_eq!(
            run("number(('7', 'x'))", &compat).unwrap().as_f64(),
            Some(7.0)
        );
        // A non-numeric string is NaN in 1.0 rather than an error.
        assert!(run("number('x')", &compat)
            .unwrap()
            .as_f64()
            .unwrap()
            .is_nan());
    }
}
