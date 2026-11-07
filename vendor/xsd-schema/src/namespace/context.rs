//! Namespace context with scoped prefix mappings
//!
//! Provides hierarchical namespace management for XML parsing.
//! Each element can push a new scope, and scopes are popped when elements close.

use super::table::{well_known, NameTable};
use crate::ids::NameId;
use std::collections::HashMap;

/// Scoped prefix-to-namespace mapping
///
/// Maintains a stack of scopes for namespace resolution during parsing.
/// Each scope can add new prefix bindings that shadow outer scopes.
///
/// # Example
///
/// ```
/// use xsd_schema::namespace::{NameTable, NamespaceContext};
///
/// let mut table = NameTable::new();
/// let mut ctx = NamespaceContext::new(&mut table);
///
/// // Root scope with XSD namespace
/// ctx.push_scope();
/// ctx.add_namespace("xs", "http://www.w3.org/2001/XMLSchema");
///
/// // Inner scope can shadow
/// ctx.push_scope();
/// ctx.add_namespace("xs", "http://different/namespace");
///
/// ctx.pop_scope(); // Back to original binding
/// ctx.pop_scope(); // Root scope removed
/// ```
pub struct NamespaceContext<'a> {
    /// Reference to the name table for string interning
    name_table: &'a mut NameTable,
    /// Stack of scopes, each mapping prefix NameId -> namespace NameId.
    /// A prefix mapped to [`well_known::EMPTY`] is undeclared in that scope
    /// (Namespaces in XML 1.1 `xmlns:p=""`).
    scopes: Vec<HashMap<NameId, NameId>>,
    /// Default namespace (unprefixed elements)
    default_namespace: Option<NameId>,
    /// Stack of default namespace values (for scoped changes)
    default_ns_stack: Vec<Option<NameId>>,
}

impl<'a> NamespaceContext<'a> {
    /// Create a new namespace context with standard bindings
    ///
    /// Pre-binds:
    /// - xml -> <http://www.w3.org/XML/1998/namespace>
    /// - xmlns -> <http://www.w3.org/2000/xmlns/>
    pub fn new(name_table: &'a mut NameTable) -> Self {
        let mut ctx = Self {
            name_table,
            scopes: Vec::new(),
            default_namespace: None,
            default_ns_stack: Vec::new(),
        };

        // Start with root scope containing standard bindings
        ctx.push_scope();

        // Bind xml and xmlns prefixes (these are always in scope per XML spec)
        ctx.scopes[0].insert(well_known::XML_PREFIX, well_known::XML_NAMESPACE);
        ctx.scopes[0].insert(well_known::XMLNS_PREFIX, well_known::XMLNS_NAMESPACE);

        ctx
    }

    /// Get a reference to the name table
    pub fn name_table(&self) -> &NameTable {
        self.name_table
    }

    /// Get a mutable reference to the name table
    pub fn name_table_mut(&mut self) -> &mut NameTable {
        self.name_table
    }

    /// Push a new scope (called on element start)
    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
        self.default_ns_stack.push(self.default_namespace);
    }

    /// Pop the current scope (called on element end)
    ///
    /// # Panics
    ///
    /// Panics if there are no scopes to pop.
    pub fn pop_scope(&mut self) {
        self.scopes.pop().expect("No scope to pop");
        self.default_namespace = self.default_ns_stack.pop().flatten();
    }

    /// Add a namespace binding to the current scope
    ///
    /// An empty `uri` undeclares: `xmlns=""` the default namespace, and
    /// `xmlns:p=""` (Namespaces in XML 1.1 §6.1; not namespace-well-formed in
    /// XML 1.0) the prefix `p`, which then has no binding in this scope —
    /// [`lookup_namespace`](Self::lookup_namespace) returns `None` for it and
    /// [`snapshot`](Self::snapshot) leaves it out, until the scope is popped
    /// or the prefix is declared again.
    ///
    /// # Arguments
    ///
    /// * `prefix` - The prefix (empty string for default namespace)
    /// * `uri` - The namespace URI
    pub fn add_namespace(&mut self, prefix: &str, uri: &str) {
        if prefix.is_empty() {
            // Default namespace
            self.default_namespace = if uri.is_empty() {
                None // xmlns="" undeclares default namespace
            } else {
                Some(self.name_table.add(uri))
            };
        } else {
            // `well_known::EMPTY` is "": an undeclared prefix.
            let uri_id = self.name_table.add(uri);
            let prefix_id = self.name_table.add(prefix);
            if let Some(scope) = self.scopes.last_mut() {
                scope.insert(prefix_id, uri_id);
            }
        }
    }

    /// Look up namespace URI for a prefix string
    pub fn lookup_namespace(&self, prefix: &str) -> Option<NameId> {
        if let Some(prefix_id) = self.name_table.get(prefix) {
            self.lookup_namespace_by_id(prefix_id)
        } else {
            None
        }
    }

    /// Look up namespace URI for a prefix NameId (`None` for a prefix that
    /// is not declared, or undeclared by `xmlns:p=""`)
    pub fn lookup_namespace_by_id(&self, prefix_id: NameId) -> Option<NameId> {
        // Search scopes from innermost to outermost
        for scope in self.scopes.iter().rev() {
            if let Some(&ns_id) = scope.get(&prefix_id) {
                return (ns_id != well_known::EMPTY).then_some(ns_id);
            }
        }
        None
    }

    /// Get the default namespace (for unprefixed elements)
    pub fn default_namespace(&self) -> Option<NameId> {
        self.default_namespace
    }

    /// Set the default namespace directly
    pub fn set_default_namespace(&mut self, uri: Option<&str>) {
        self.default_namespace = uri.map(|u| self.name_table.add(u));
    }

    /// Set the default namespace to an already-interned `NameId`.
    ///
    /// Used by the parser to install a chameleon-adopted namespace on the
    /// root `<xs:schema>` scope (§4.2.3 clause 2.3): when an included schema
    /// has no `targetNamespace` and no explicit `xmlns` default, unqualified
    /// QName references inside it must resolve to the includer's target
    /// namespace. Pre-setting the scope's default namespace produces that
    /// resolution naturally during subsequent QName parsing.
    pub fn set_default_namespace_id(&mut self, ns: Option<NameId>) {
        self.default_namespace = ns;
    }

    /// Get all namespace bindings in scope
    ///
    /// # Arguments
    ///
    /// * `scope_filter` - Filter for which namespaces to include
    ///
    /// Returns Vec of (prefix_id, namespace_id) pairs.
    pub fn get_namespaces_in_scope(&self, scope_filter: NamespaceScope) -> Vec<(NameId, NameId)> {
        let mut result = HashMap::new();

        // Collect all bindings (inner scopes override outer)
        for scope in &self.scopes {
            for (&prefix_id, &ns_id) in scope {
                result.insert(prefix_id, ns_id);
            }
        }

        // Filter based on scope type; an undeclared prefix is not in scope.
        result
            .into_iter()
            .filter(|&(_, ns_id)| ns_id != well_known::EMPTY)
            .filter(|&(prefix_id, _)| match scope_filter {
                NamespaceScope::All => true,
                NamespaceScope::ExcludeXml => {
                    prefix_id != well_known::XML_PREFIX && prefix_id != well_known::XMLNS_PREFIX
                }
            })
            .collect()
    }

    /// Get current scope depth
    pub fn depth(&self) -> usize {
        self.scopes.len()
    }

    /// Create a snapshot of current namespace bindings
    pub fn snapshot(&self) -> NamespaceContextSnapshot {
        NamespaceContextSnapshot {
            default_ns: self.default_namespace,
            bindings: self.get_namespaces_in_scope(NamespaceScope::ExcludeXml),
        }
    }
}

/// Filter for get_namespaces_in_scope
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamespaceScope {
    /// Include all namespace bindings
    All,
    /// Exclude xml and xmlns bindings
    ExcludeXml,
}

/// Snapshot of namespace bindings at a point in time
///
/// Used to capture context for annotation processing and QName resolution.
#[derive(Debug, Clone, Default)]
pub struct NamespaceContextSnapshot {
    /// Default namespace at snapshot time
    pub default_ns: Option<NameId>,
    /// All prefix bindings (excluding xml/xmlns)
    pub bindings: Vec<(NameId, NameId)>,
}

impl NamespaceContextSnapshot {
    /// Look up namespace URI for a prefix NameId
    ///
    /// Searches through the captured bindings to find the namespace
    /// associated with the given prefix.
    pub fn resolve_prefix(&self, prefix_id: NameId) -> Option<NameId> {
        for &(p, ns) in &self.bindings {
            if p == prefix_id {
                return Some(ns);
            }
        }
        // Check well-known xml prefix (always in scope per XML spec)
        if prefix_id == well_known::XML_PREFIX {
            return Some(well_known::XML_NAMESPACE);
        }
        None
    }

    /// Get the default namespace (for unprefixed elements)
    pub fn default_namespace(&self) -> Option<NameId> {
        self.default_ns
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_pop_scope() {
        let mut table = NameTable::new();
        let mut ctx = NamespaceContext::new(&mut table);

        let initial_depth = ctx.depth();
        ctx.push_scope();
        assert_eq!(ctx.depth(), initial_depth + 1);
        ctx.pop_scope();
        assert_eq!(ctx.depth(), initial_depth);
    }

    #[test]
    fn test_add_and_lookup_namespace() {
        let mut table = NameTable::new();
        let mut ctx = NamespaceContext::new(&mut table);

        ctx.push_scope();
        ctx.add_namespace("foo", "http://example.com/foo");

        let ns = ctx.lookup_namespace("foo");
        assert!(ns.is_some());

        // Access name_table through ctx to avoid borrow conflict
        let ns_str = ctx.name_table().resolve(ns.unwrap());
        assert_eq!(ns_str, "http://example.com/foo");
    }

    #[test]
    fn test_scope_shadowing() {
        let mut table = NameTable::new();
        let mut ctx = NamespaceContext::new(&mut table);

        ctx.push_scope();
        ctx.add_namespace("foo", "http://outer.com");

        ctx.push_scope();
        ctx.add_namespace("foo", "http://inner.com");

        // Inner scope shadows outer
        let ns = ctx.lookup_namespace("foo").unwrap();
        assert_eq!(ctx.name_table().resolve(ns), "http://inner.com");

        ctx.pop_scope();

        // Back to outer binding
        let ns = ctx.lookup_namespace("foo").unwrap();
        assert_eq!(ctx.name_table().resolve(ns), "http://outer.com");
    }

    #[test]
    fn test_default_namespace() {
        let mut table = NameTable::new();
        let mut ctx = NamespaceContext::new(&mut table);

        assert!(ctx.default_namespace().is_none());

        ctx.push_scope();
        ctx.add_namespace("", "http://default.com");
        assert!(ctx.default_namespace().is_some());

        ctx.pop_scope();
        assert!(ctx.default_namespace().is_none());
    }

    #[test]
    fn test_undeclare_prefix() {
        let mut table = NameTable::new();
        let mut ctx = NamespaceContext::new(&mut table);

        ctx.push_scope();
        ctx.add_namespace("p", "http://outer.com");
        ctx.push_scope();
        ctx.add_namespace("p", ""); // Namespaces in XML 1.1: undeclare
        assert_eq!(ctx.lookup_namespace("p"), None);
        let p = ctx.name_table().get("p").unwrap();
        assert!(ctx.snapshot().resolve_prefix(p).is_none());
        assert!(ctx
            .get_namespaces_in_scope(NamespaceScope::All)
            .iter()
            .all(|&(prefix, _)| prefix != p));

        ctx.push_scope();
        ctx.add_namespace("p", "http://again.com");
        let ns = ctx.lookup_namespace("p").unwrap();
        assert_eq!(ctx.name_table().resolve(ns), "http://again.com");
        ctx.pop_scope();
        assert_eq!(ctx.lookup_namespace("p"), None);

        ctx.pop_scope();
        let ns = ctx.lookup_namespace("p").unwrap();
        assert_eq!(ctx.name_table().resolve(ns), "http://outer.com");
    }

    #[test]
    fn test_undeclare_default_namespace() {
        let mut table = NameTable::new();
        let mut ctx = NamespaceContext::new(&mut table);

        ctx.push_scope();
        ctx.add_namespace("", "http://default.com");
        assert!(ctx.default_namespace().is_some());

        ctx.push_scope();
        ctx.add_namespace("", ""); // Undeclare
        assert!(ctx.default_namespace().is_none());
    }

    #[test]
    fn test_xml_prefix_always_bound() {
        let mut table = NameTable::new();
        let ctx = NamespaceContext::new(&mut table);

        let ns = ctx.lookup_namespace("xml");
        assert!(ns.is_some());
        assert_eq!(
            ctx.name_table().resolve(ns.unwrap()),
            super::super::table::XML_NAMESPACE
        );
    }

    #[test]
    fn test_snapshot() {
        let mut table = NameTable::new();
        let mut ctx = NamespaceContext::new(&mut table);

        ctx.push_scope();
        ctx.add_namespace("foo", "http://foo.com");
        ctx.add_namespace("", "http://default.com");

        let snapshot = ctx.snapshot();
        assert!(snapshot.default_ns.is_some());
        assert!(!snapshot.bindings.is_empty());
    }
}
