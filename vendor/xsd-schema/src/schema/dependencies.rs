//! Dependency graph for type compilation order
//!
//! This module builds a dependency graph from type derivations and provides
//! topological sorting for determining compilation order. It also detects
//! circular dependencies which are invalid in XSD.
//!
//! # Dependency Types
//!
//! Types depend on their base types:
//! - Simple type restriction: depends on base type
//! - Simple type list: depends on item type
//! - Simple type union: depends on all member types
//! - Complex type extension: depends on base type
//! - Complex type restriction: depends on base type
//!
//! # Compilation Order
//!
//! Types must be compiled in topological order so that base types are
//! available when compiling derived types. Built-in types are always
//! available and don't need to be in the graph.

use crate::error::{SchemaError, SchemaResult};
use crate::ids::{ComplexTypeKey, SimpleTypeKey, TypeKey};
use crate::schema::SchemaSet;
use std::collections::{HashMap, HashSet, VecDeque};

/// Dependency graph for type compilation order
///
/// Tracks dependencies between types and provides topological sorting
/// to determine the correct compilation order.
#[derive(Debug)]
pub struct DependencyGraph {
    /// Type → types it depends on (base types, item types, member types)
    dependencies: HashMap<TypeKey, Vec<TypeKey>>,

    /// Type → types that depend on it (reverse edges for in-degree calculation)
    dependents: HashMap<TypeKey, Vec<TypeKey>>,

    /// All types in the graph
    all_types: HashSet<TypeKey>,

    /// All types in the order they were first added — the order every pass
    /// over the graph follows, so that the compilation order (and with it the
    /// first error reported among independent types) does not depend on hash
    /// order
    order: Vec<TypeKey>,

    /// Topologically sorted type keys (dependencies first)
    /// Populated after calling sort()
    sorted_types: Vec<TypeKey>,

    /// Whether the graph has been sorted
    is_sorted: bool,
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl DependencyGraph {
    /// Create a new empty dependency graph
    pub fn new() -> Self {
        Self {
            dependencies: HashMap::new(),
            dependents: HashMap::new(),
            all_types: HashSet::new(),
            order: Vec::new(),
            sorted_types: Vec::new(),
            is_sorted: false,
        }
    }

    /// Add a type to the graph (without dependencies)
    pub fn add_type(&mut self, type_key: TypeKey) {
        self.insert_type(type_key);
        self.is_sorted = false;
    }

    fn insert_type(&mut self, type_key: TypeKey) {
        if self.all_types.insert(type_key) {
            self.order.push(type_key);
        }
    }

    /// Add a type dependency (derived_type depends on base_type)
    ///
    /// This means base_type must be compiled before derived_type.
    pub fn add_dependency(&mut self, derived: TypeKey, base: TypeKey) {
        // Add both types to the graph
        self.insert_type(derived);
        self.insert_type(base);

        // Add forward edge: derived -> base (derived depends on base)
        self.dependencies.entry(derived).or_default().push(base);

        // Add reverse edge: base -> derived (derived is a dependent of base)
        self.dependents.entry(base).or_default().push(derived);

        self.is_sorted = false;
    }

    /// Get the number of types in the graph
    pub fn type_count(&self) -> usize {
        self.all_types.len()
    }

    /// Get the number of dependency edges in the graph
    pub fn edge_count(&self) -> usize {
        self.dependencies.values().map(|v| v.len()).sum()
    }

    /// Check if a type has any dependencies
    pub fn has_dependencies(&self, type_key: TypeKey) -> bool {
        self.dependencies
            .get(&type_key)
            .map(|deps| !deps.is_empty())
            .unwrap_or(false)
    }

    /// Get the dependencies of a type
    pub fn get_dependencies(&self, type_key: TypeKey) -> &[TypeKey] {
        self.dependencies
            .get(&type_key)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Perform topological sort and cycle detection using Kahn's algorithm
    ///
    /// After successful sorting, use `compilation_order()` to get types
    /// in the order they should be compiled (dependencies first).
    ///
    /// # Errors
    ///
    /// Returns an error if a circular dependency is detected.
    pub fn sort(&mut self) -> SchemaResult<()> {
        if self.is_sorted {
            return Ok(());
        }

        // Calculate in-degrees (number of dependencies for each type)
        let mut in_degree: HashMap<TypeKey, usize> = HashMap::new();
        for type_key in &self.order {
            let deps = self
                .dependencies
                .get(type_key)
                .map(|v| v.len())
                .unwrap_or(0);
            in_degree.insert(*type_key, deps);
        }

        // Queue of types with no dependencies (in-degree 0), in the order
        // the types were added: independent types keep that order.
        let mut queue: VecDeque<TypeKey> = self
            .order
            .iter()
            .copied()
            .filter(|key| in_degree.get(key) == Some(&0))
            .collect();

        let mut sorted = Vec::with_capacity(self.all_types.len());

        while let Some(type_key) = queue.pop_front() {
            sorted.push(type_key);

            // For each type that depends on this one, decrement its in-degree
            if let Some(deps) = self.dependents.get(&type_key) {
                for dependent in deps {
                    if let Some(deg) = in_degree.get_mut(dependent) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(*dependent);
                        }
                    }
                }
            }
        }

        // If we couldn't process all types, there's a cycle
        if sorted.len() != self.all_types.len() {
            // Find the cycle for error reporting
            let cycle = self.find_cycle()?;
            return Err(SchemaError::structural(
                "ct-props-correct.3",
                format!("Circular type dependency detected: {}", cycle),
                None,
            ));
        }

        self.sorted_types = sorted;
        self.is_sorted = true;
        Ok(())
    }

    /// Get types in compilation order (dependencies first)
    ///
    /// Must call `sort()` first or this will return an empty slice.
    pub fn compilation_order(&self) -> &[TypeKey] {
        &self.sorted_types
    }

    /// Find a cycle in the graph for error reporting
    ///
    /// Uses DFS with path tracking to find a cycle.
    fn find_cycle(&self) -> SchemaResult<String> {
        let mut visited = HashSet::new();
        let mut in_stack = HashSet::new();
        let mut path = Vec::new();

        for &start in &self.order {
            if !visited.contains(&start) {
                if let Some(cycle) =
                    self.dfs_find_cycle(start, &mut visited, &mut in_stack, &mut path)
                {
                    return Ok(cycle);
                }
            }
        }

        // Fallback if we couldn't find the cycle (shouldn't happen)
        Ok("unknown cycle".to_string())
    }

    /// DFS helper to find a cycle
    fn dfs_find_cycle(
        &self,
        node: TypeKey,
        visited: &mut HashSet<TypeKey>,
        in_stack: &mut HashSet<TypeKey>,
        path: &mut Vec<TypeKey>,
    ) -> Option<String> {
        visited.insert(node);
        in_stack.insert(node);
        path.push(node);

        if let Some(deps) = self.dependencies.get(&node) {
            for &dep in deps {
                if !visited.contains(&dep) {
                    if let Some(cycle) = self.dfs_find_cycle(dep, visited, in_stack, path) {
                        return Some(cycle);
                    }
                } else if in_stack.contains(&dep) {
                    // Found a cycle! Build the cycle description
                    let cycle_start = path.iter().position(|&t| t == dep).unwrap();
                    let cycle_path: Vec<String> = path[cycle_start..]
                        .iter()
                        .map(|t| format!("{:?}", t))
                        .collect();
                    return Some(format!("{} -> {:?}", cycle_path.join(" -> "), dep));
                }
            }
        }

        path.pop();
        in_stack.remove(&node);
        None
    }

    /// Check if the graph contains a cycle
    pub fn has_cycle(&self) -> bool {
        let mut visited = HashSet::new();
        let mut in_stack = HashSet::new();
        let mut path = Vec::new();

        for &start in &self.order {
            if !visited.contains(&start)
                && self
                    .dfs_find_cycle(start, &mut visited, &mut in_stack, &mut path)
                    .is_some()
            {
                return true;
            }
        }

        false
    }
}

/// Statistics from building the dependency graph
#[derive(Debug, Default)]
pub struct DependencyStats {
    /// Number of simple types in the graph
    pub simple_types: usize,
    /// Number of complex types in the graph
    pub complex_types: usize,
    /// Number of dependency edges
    pub dependencies: usize,
    /// Number of types with no dependencies (root types)
    pub root_types: usize,
    /// Maximum depth of the dependency tree
    pub max_depth: usize,
}

/// Build a dependency graph from a schema set
///
/// Uses the resolved references from Task 3.1 to build the graph.
/// Built-in types are not added to the graph since they're always available.
///
/// # Arguments
///
/// * `schema_set` - The schema set with resolved references
///
/// # Returns
///
/// A tuple of (DependencyGraph, DependencyStats)
pub fn build_dependency_graph(
    schema_set: &SchemaSet,
) -> SchemaResult<(DependencyGraph, DependencyStats)> {
    let mut graph = DependencyGraph::new();
    let mut stats = DependencyStats::default();

    // Collect all user-defined type keys
    let simple_type_keys: Vec<SimpleTypeKey> = schema_set.arenas.simple_types.keys().collect();
    let complex_type_keys: Vec<ComplexTypeKey> = schema_set.arenas.complex_types.keys().collect();

    // Track which types are built-in (we don't add them to the graph)
    let builtin_types = schema_set.builtin_types();

    // Add simple types and their dependencies
    for key in simple_type_keys {
        let type_key = TypeKey::Simple(key);

        // Skip built-in types
        if is_builtin_simple_type(key, builtin_types) {
            continue;
        }

        // Add type to graph
        graph.add_type(type_key);
        stats.simple_types += 1;

        if let Some(type_def) = schema_set.arenas.simple_types.get(key) {
            // Add dependency on base type (for restriction)
            if let Some(base_key) = type_def.resolved_base_type {
                if !is_builtin_type(base_key, builtin_types) {
                    graph.add_dependency(type_key, base_key);
                    stats.dependencies += 1;
                }
            }

            // Add dependency on item type (for list)
            if let Some(item_key) = type_def.resolved_item_type {
                if !is_builtin_type(item_key, builtin_types) {
                    graph.add_dependency(type_key, item_key);
                    stats.dependencies += 1;
                }
            }

            // Add dependencies on member types (for union)
            for member_key in &type_def.resolved_member_types {
                if !is_builtin_type(*member_key, builtin_types) {
                    graph.add_dependency(type_key, *member_key);
                    stats.dependencies += 1;
                }
            }
        }
    }

    // Add complex types and their dependencies
    for key in complex_type_keys {
        let type_key = TypeKey::Complex(key);

        if is_builtin_type(type_key, builtin_types) {
            continue;
        }

        // Add type to graph
        graph.add_type(type_key);
        stats.complex_types += 1;

        if let Some(type_def) = schema_set.arenas.complex_types.get(key) {
            // Add dependency on base type (for extension/restriction)
            if let Some(base_key) = type_def.resolved_base_type {
                if !is_builtin_type(base_key, builtin_types) {
                    graph.add_dependency(type_key, base_key);
                    stats.dependencies += 1;
                }
            }
        }
    }

    // Calculate root types (types with no dependencies)
    for type_key in graph.all_types.iter() {
        if !graph.has_dependencies(*type_key) {
            stats.root_types += 1;
        }
    }

    // Perform topological sort (this also validates there are no cycles)
    graph.sort()?;

    // Calculate max depth
    stats.max_depth = calculate_max_depth(&graph);

    Ok((graph, stats))
}

/// Check if a simple type key refers to a built-in type
fn is_builtin_simple_type(
    key: SimpleTypeKey,
    builtin: &crate::types::builtin::BuiltinTypes,
) -> bool {
    // Check if this key matches any of the well-known built-in type keys
    // We only need to check a few common ones since built-in types are
    // created with specific keys during initialization
    key == builtin.any_simple_type
        || key == builtin.string
        || key == builtin.boolean
        || key == builtin.decimal
        || key == builtin.float
        || key == builtin.double
        || key == builtin.duration
        || key == builtin.datetime
        || key == builtin.time
        || key == builtin.date
        || key == builtin.g_year_month
        || key == builtin.g_year
        || key == builtin.g_month_day
        || key == builtin.g_day
        || key == builtin.g_month
        || key == builtin.hex_binary
        || key == builtin.base64_binary
        || key == builtin.any_uri
        || key == builtin.xsi_schema_location_type
        || key == builtin.qname
        || key == builtin.notation
        || key == builtin.normalized_string
        || key == builtin.token
        || key == builtin.language
        || key == builtin.nmtoken
        || key == builtin.nmtokens
        || key == builtin.name
        || key == builtin.ncname
        || key == builtin.id
        || key == builtin.idref
        || key == builtin.idrefs
        || key == builtin.entity
        || key == builtin.entities
        || key == builtin.integer
        || key == builtin.non_positive_integer
        || key == builtin.negative_integer
        || key == builtin.long
        || key == builtin.int
        || key == builtin.short
        || key == builtin.byte
        || key == builtin.non_negative_integer
        || key == builtin.unsigned_long
        || key == builtin.unsigned_int
        || key == builtin.unsigned_short
        || key == builtin.unsigned_byte
        || key == builtin.positive_integer
        // Check optional XSD 1.1 types
        || (builtin.any_atomic_type == Some(key))
        || (builtin.year_month_duration == Some(key))
        || (builtin.day_time_duration == Some(key))
        || (builtin.datetime_stamp == Some(key))
        || (builtin.untyped_atomic == Some(key))
        // The XPath 2.0 data model's types an XSD 1.0 set has for a host
        || builtin.is_xpath_type(key)
}

/// Check if a type key refers to a built-in type
fn is_builtin_type(key: TypeKey, builtin: &crate::types::builtin::BuiltinTypes) -> bool {
    match key {
        TypeKey::Simple(simple_key) => is_builtin_simple_type(simple_key, builtin),
        TypeKey::Complex(complex_key) => complex_key == builtin.any_type,
    }
}

/// Calculate the maximum depth of the dependency tree
fn calculate_max_depth(graph: &DependencyGraph) -> usize {
    let mut depth_cache: HashMap<TypeKey, usize> = HashMap::new();

    fn get_depth(
        key: TypeKey,
        graph: &DependencyGraph,
        cache: &mut HashMap<TypeKey, usize>,
    ) -> usize {
        if let Some(&cached) = cache.get(&key) {
            return cached;
        }

        let deps = graph.get_dependencies(key);
        let depth = if deps.is_empty() {
            0
        } else {
            1 + deps
                .iter()
                .map(|&d| get_depth(d, graph, cache))
                .max()
                .unwrap_or(0)
        };

        cache.insert(key, depth);
        depth
    }

    graph
        .all_types
        .iter()
        .map(|&key| get_depth(key, graph, &mut depth_cache))
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use slotmap::SlotMap;

    // Helper to create type keys for testing
    fn make_simple_key(_id: u32) -> TypeKey {
        // Create a temporary SlotMap just to generate keys
        // In tests we use these keys symbolically
        thread_local! {
            static SIMPLE_MAP: std::cell::RefCell<SlotMap<SimpleTypeKey, ()>> =
                std::cell::RefCell::new(SlotMap::with_key());
        }
        SIMPLE_MAP.with(|m| TypeKey::Simple(m.borrow_mut().insert(())))
    }

    fn make_complex_key(_id: u32) -> TypeKey {
        thread_local! {
            static COMPLEX_MAP: std::cell::RefCell<SlotMap<ComplexTypeKey, ()>> =
                std::cell::RefCell::new(SlotMap::with_key());
        }
        COMPLEX_MAP.with(|m| TypeKey::Complex(m.borrow_mut().insert(())))
    }

    #[test]
    fn test_empty_graph() {
        let mut graph = DependencyGraph::new();
        assert_eq!(graph.type_count(), 0);
        assert_eq!(graph.edge_count(), 0);
        assert!(graph.sort().is_ok());
        assert!(graph.compilation_order().is_empty());
    }

    #[test]
    fn test_single_type_no_deps() {
        let mut graph = DependencyGraph::new();
        let t1 = make_simple_key(1);
        graph.add_type(t1);

        assert_eq!(graph.type_count(), 1);
        assert_eq!(graph.edge_count(), 0);
        assert!(!graph.has_dependencies(t1));

        assert!(graph.sort().is_ok());
        assert_eq!(graph.compilation_order().len(), 1);
        assert_eq!(graph.compilation_order()[0], t1);
    }

    #[test]
    fn test_simple_chain() {
        // t3 -> t2 -> t1 (t3 depends on t2, t2 depends on t1)
        let mut graph = DependencyGraph::new();
        let t1 = make_simple_key(1);
        let t2 = make_simple_key(2);
        let t3 = make_simple_key(3);

        graph.add_dependency(t2, t1); // t2 depends on t1
        graph.add_dependency(t3, t2); // t3 depends on t2

        assert_eq!(graph.type_count(), 3);
        assert_eq!(graph.edge_count(), 2);

        assert!(graph.sort().is_ok());
        let order = graph.compilation_order();

        // t1 must come before t2, t2 must come before t3
        let pos_t1 = order.iter().position(|&t| t == t1).unwrap();
        let pos_t2 = order.iter().position(|&t| t == t2).unwrap();
        let pos_t3 = order.iter().position(|&t| t == t3).unwrap();

        assert!(pos_t1 < pos_t2);
        assert!(pos_t2 < pos_t3);
    }

    #[test]
    fn test_diamond_dependency() {
        //     t1
        //    /  \
        //   t2   t3
        //    \  /
        //     t4
        // t4 depends on both t2 and t3, which both depend on t1
        let mut graph = DependencyGraph::new();
        let t1 = make_simple_key(1);
        let t2 = make_simple_key(2);
        let t3 = make_simple_key(3);
        let t4 = make_simple_key(4);

        graph.add_dependency(t2, t1);
        graph.add_dependency(t3, t1);
        graph.add_dependency(t4, t2);
        graph.add_dependency(t4, t3);

        assert_eq!(graph.type_count(), 4);
        assert_eq!(graph.edge_count(), 4);

        assert!(graph.sort().is_ok());
        let order = graph.compilation_order();

        // t1 must come before t2 and t3, both must come before t4
        let pos_t1 = order.iter().position(|&t| t == t1).unwrap();
        let pos_t2 = order.iter().position(|&t| t == t2).unwrap();
        let pos_t3 = order.iter().position(|&t| t == t3).unwrap();
        let pos_t4 = order.iter().position(|&t| t == t4).unwrap();

        assert!(pos_t1 < pos_t2);
        assert!(pos_t1 < pos_t3);
        assert!(pos_t2 < pos_t4);
        assert!(pos_t3 < pos_t4);
    }

    #[test]
    fn test_cycle_detection_simple() {
        // t1 -> t2 -> t1 (simple cycle)
        let mut graph = DependencyGraph::new();
        let t1 = make_simple_key(1);
        let t2 = make_simple_key(2);

        graph.add_dependency(t1, t2);
        graph.add_dependency(t2, t1);

        assert!(graph.has_cycle());
        let result = graph.sort();
        assert!(result.is_err());
    }

    #[test]
    fn test_cycle_detection_triangle() {
        // t1 -> t2 -> t3 -> t1 (triangle cycle)
        let mut graph = DependencyGraph::new();
        let t1 = make_simple_key(1);
        let t2 = make_simple_key(2);
        let t3 = make_simple_key(3);

        graph.add_dependency(t1, t2);
        graph.add_dependency(t2, t3);
        graph.add_dependency(t3, t1);

        assert!(graph.has_cycle());
        let result = graph.sort();
        assert!(result.is_err());
    }

    #[test]
    fn test_self_dependency() {
        // t1 -> t1 (self-cycle)
        let mut graph = DependencyGraph::new();
        let t1 = make_simple_key(1);

        graph.add_dependency(t1, t1);

        assert!(graph.has_cycle());
        let result = graph.sort();
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_independent_chains() {
        // Chain 1: t1 -> t2
        // Chain 2: t3 -> t4
        // (independent chains)
        let mut graph = DependencyGraph::new();
        let t1 = make_simple_key(1);
        let t2 = make_simple_key(2);
        let t3 = make_simple_key(3);
        let t4 = make_simple_key(4);

        graph.add_dependency(t2, t1);
        graph.add_dependency(t4, t3);

        assert_eq!(graph.type_count(), 4);
        assert!(!graph.has_cycle());
        assert!(graph.sort().is_ok());

        let order = graph.compilation_order();

        // t1 before t2, t3 before t4
        let pos_t1 = order.iter().position(|&t| t == t1).unwrap();
        let pos_t2 = order.iter().position(|&t| t == t2).unwrap();
        let pos_t3 = order.iter().position(|&t| t == t3).unwrap();
        let pos_t4 = order.iter().position(|&t| t == t4).unwrap();

        assert!(pos_t1 < pos_t2);
        assert!(pos_t3 < pos_t4);
    }

    #[test]
    fn test_mixed_simple_and_complex_types() {
        let mut graph = DependencyGraph::new();
        let s1 = make_simple_key(1);
        let c1 = make_complex_key(1);

        // Complex type c1 depends on simple type s1
        graph.add_dependency(c1, s1);

        assert_eq!(graph.type_count(), 2);
        assert!(graph.sort().is_ok());

        let order = graph.compilation_order();
        let pos_s1 = order.iter().position(|&t| t == s1).unwrap();
        let pos_c1 = order.iter().position(|&t| t == c1).unwrap();

        assert!(pos_s1 < pos_c1);
    }

    #[test]
    fn test_repeated_sort() {
        let mut graph = DependencyGraph::new();
        let t1 = make_simple_key(1);
        let t2 = make_simple_key(2);

        graph.add_dependency(t2, t1);

        // Sort multiple times
        assert!(graph.sort().is_ok());
        let first_order = graph.compilation_order().to_vec();

        assert!(graph.sort().is_ok());
        let second_order = graph.compilation_order().to_vec();

        // Should be the same
        assert_eq!(first_order, second_order);
    }

    #[test]
    fn test_dependency_stats_default() {
        let stats = DependencyStats::default();
        assert_eq!(stats.simple_types, 0);
        assert_eq!(stats.complex_types, 0);
        assert_eq!(stats.dependencies, 0);
        assert_eq!(stats.root_types, 0);
        assert_eq!(stats.max_depth, 0);
    }

    #[test]
    fn test_get_dependencies() {
        let mut graph = DependencyGraph::new();
        let t1 = make_simple_key(1);
        let t2 = make_simple_key(2);
        let t3 = make_simple_key(3);

        graph.add_type(t1);
        graph.add_dependency(t2, t1);
        graph.add_dependency(t3, t1);
        graph.add_dependency(t3, t2);

        assert!(graph.get_dependencies(t1).is_empty());
        assert_eq!(graph.get_dependencies(t2).len(), 1);
        assert_eq!(graph.get_dependencies(t3).len(), 2);
    }

    #[test]
    fn test_build_dependency_graph_empty_schema() {
        let schema_set = SchemaSet::new();
        let result = build_dependency_graph(&schema_set);
        assert!(result.is_ok());

        let (graph, stats) = result.unwrap();
        assert_eq!(stats.simple_types, 0);
        assert_eq!(stats.complex_types, 0);
        assert_eq!(graph.type_count(), 0);
    }

    #[test]
    fn test_build_dependency_graph_with_user_simple_type() {
        use crate::arenas::SimpleTypeDefData;
        use crate::parser::frames::SimpleTypeVariety;
        use crate::schema::model::DerivationSet;
        use crate::types::facets::FacetSet;

        let mut schema_set = SchemaSet::new();

        // Create a user-defined simple type that derives from xs:string
        let type_name = schema_set.name_table.add("myStringType");
        let string_key = schema_set.builtin_types().string;

        let simple_data = SimpleTypeDefData {
            name: Some(type_name),
            target_namespace: None,
            variety: SimpleTypeVariety::Atomic,
            base_type: None,
            item_type: None,
            member_types: Vec::new(),
            facets: FacetSet::new(),
            final_derivation: DerivationSet::empty(),
            id: None,
            derivation_id: None,
            annotation: None,
            source: None,
            // Already resolved base type (as if from resolution phase)
            resolved_base_type: Some(TypeKey::Simple(string_key)),
            resolved_item_type: None,
            resolved_member_types: Vec::new(),
            redefine_original: None,
            deferred_item_type_error: None,
        };

        let _user_type_key = schema_set.arenas.alloc_simple_type(simple_data);

        // Build the dependency graph
        let result = build_dependency_graph(&schema_set);
        assert!(result.is_ok());

        let (graph, stats) = result.unwrap();
        // One user-defined simple type
        assert_eq!(stats.simple_types, 1);
        // The user type depends on the built-in string, but built-in is excluded
        assert_eq!(graph.type_count(), 1);
    }

    #[test]
    fn test_build_dependency_graph_chain_of_user_types() {
        use crate::arenas::SimpleTypeDefData;
        use crate::parser::frames::SimpleTypeVariety;
        use crate::schema::model::DerivationSet;
        use crate::types::facets::FacetSet;

        let mut schema_set = SchemaSet::new();

        // Create type chain: myType2 -> myType1 -> xs:string
        let type1_name = schema_set.name_table.add("myType1");
        let type2_name = schema_set.name_table.add("myType2");
        let string_key = schema_set.builtin_types().string;

        // First type derives from xs:string
        let type1_data = SimpleTypeDefData {
            name: Some(type1_name),
            target_namespace: None,
            variety: SimpleTypeVariety::Atomic,
            base_type: None,
            item_type: None,
            member_types: Vec::new(),
            facets: FacetSet::new(),
            final_derivation: DerivationSet::empty(),
            id: None,
            derivation_id: None,
            annotation: None,
            source: None,
            resolved_base_type: Some(TypeKey::Simple(string_key)),
            resolved_item_type: None,
            resolved_member_types: Vec::new(),
            redefine_original: None,
            deferred_item_type_error: None,
        };
        let type1_key = schema_set.arenas.alloc_simple_type(type1_data);

        // Second type derives from first type
        let type2_data = SimpleTypeDefData {
            name: Some(type2_name),
            target_namespace: None,
            variety: SimpleTypeVariety::Atomic,
            base_type: None,
            item_type: None,
            member_types: Vec::new(),
            facets: FacetSet::new(),
            final_derivation: DerivationSet::empty(),
            id: None,
            derivation_id: None,
            annotation: None,
            source: None,
            resolved_base_type: Some(TypeKey::Simple(type1_key)),
            resolved_item_type: None,
            resolved_member_types: Vec::new(),
            redefine_original: None,
            deferred_item_type_error: None,
        };
        let type2_key = schema_set.arenas.alloc_simple_type(type2_data);

        // Build the dependency graph
        let result = build_dependency_graph(&schema_set);
        assert!(result.is_ok());

        let (graph, stats) = result.unwrap();
        // Two user-defined simple types
        assert_eq!(stats.simple_types, 2);
        // Type2 depends on type1
        assert_eq!(stats.dependencies, 1);

        // Sort should succeed (no cycles)
        assert!(!graph.has_cycle());

        // Verify order: type1 must come before type2
        let order = graph.compilation_order();
        let pos_type1 = order.iter().position(|&t| t == TypeKey::Simple(type1_key));
        let pos_type2 = order.iter().position(|&t| t == TypeKey::Simple(type2_key));

        assert!(pos_type1.is_some() && pos_type2.is_some());
        assert!(pos_type1.unwrap() < pos_type2.unwrap());
    }

    #[test]
    fn test_is_builtin_simple_type_xsd11() {
        use crate::schema::model::XsdVersion;

        let schema_set = SchemaSet::with_version(XsdVersion::V1_1);
        let builtin = schema_set.builtin_types();

        // XSD 1.0 types should be recognized
        assert!(is_builtin_simple_type(builtin.string, builtin));
        assert!(is_builtin_simple_type(builtin.integer, builtin));
        assert!(is_builtin_simple_type(builtin.any_simple_type, builtin));

        // XSD 1.1 types should also be recognized
        if let Some(year_month_duration) = builtin.year_month_duration {
            assert!(is_builtin_simple_type(year_month_duration, builtin));
        }
        if let Some(day_time_duration) = builtin.day_time_duration {
            assert!(is_builtin_simple_type(day_time_duration, builtin));
        }
        if let Some(datetime_stamp) = builtin.datetime_stamp {
            assert!(is_builtin_simple_type(datetime_stamp, builtin));
        }
        if let Some(untyped_atomic) = builtin.untyped_atomic {
            assert!(is_builtin_simple_type(untyped_atomic, builtin));
        }
        if let Some(any_atomic_type) = builtin.any_atomic_type {
            assert!(is_builtin_simple_type(any_atomic_type, builtin));
        }
    }
}
