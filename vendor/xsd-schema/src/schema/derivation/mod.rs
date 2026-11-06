//! Type derivation validation
//!
//! This module validates type derivation rules according to the XSD specification.
//! It is run after reference resolution (Task 3.1) and dependency graph construction
//! (Task 3.2), using the topological order to process types in correct order.
//!
//! # Validation Rules
//!
//! ## Simple Type Derivation
//!
//! - **Restriction**: Derived facets must be more restrictive than base facets
//! - **List**: Item type must be atomic (not list or union of lists)
//! - **Union**: Member types must be simple types
//!
//! ## Complex Type Derivation
//!
//! - **Extension**: Base type content + new content must be valid
//! - **Restriction**: Content model must be valid restriction of base content model
//!
//! # XSD Constraint IDs
//!
//! - `cos-st-restricts` - Derivation Valid (Restriction, Simple)
//! - `cos-list-of-atomic` - List item type must be atomic
//! - `cos-union-memberTypes` - Union member types must be simple
//! - `cos-ct-extends` - Complex Type Derivation OK (Extension)
//! - `derivation-ok-restriction` - Complex Type Derivation OK (Restriction)

mod attributes;
mod complex;
mod constraints;
mod counts;
mod normalize;
mod particle;
mod redefine;
mod simple;
mod type_table;
mod wildcard;

#[cfg(test)]
mod tests;

pub(crate) use attributes::{
    compute_runtime_attribute_wildcard, effective_attribute_wildcard,
    effective_wildcard_allows_attribute, EffectiveAttributeWildcard,
};
pub use constraints::{
    validate_attribute_id_constraints, validate_attribute_value_constraints,
    validate_complex_type_attribute_uniqueness, validate_element_value_constraints,
    validate_local_decl_target_namespace, validate_no_xsi_attribute_declarations,
    validate_substitution_group_element_consistency, validate_xsd10_annotation_source_anyuri,
};
#[cfg(feature = "xsd11")]
pub use type_table::{
    validate_element_type_alternatives, validate_local_element_type_table_consistency,
    validate_restriction_local_element_type_table_consistency, validate_wildcard_disallowed_names,
    validate_wildcard_element_type_table_consistency,
};
// `CanonicalNs` and `wildcard_result_union` are re-exported only to preserve
// the pre-split `crate::schema::derivation::{CanonicalNs, wildcard_result_union}`
// paths; nothing outside their defining submodule names them today.
#[allow(unused_imports)]
pub(crate) use attributes::CanonicalNs;
#[cfg(feature = "xsd11")]
#[allow(unused_imports)]
pub(crate) use wildcard::wildcard_result_union;

use self::complex::validate_complex_type;
use self::redefine::{
    validate_all_redefine_attribute_group_restrictions, validate_all_redefine_group_restrictions,
};
use self::simple::validate_simple_type;
use crate::error::{SchemaError, SchemaResult};
use crate::ids::{AttributeGroupKey, NameId, TypeKey};
use crate::parser::location::{SourceLocation, SourceRef};
use crate::schema::dependencies::DependencyGraph;
use crate::schema::model::DerivationSet;
use crate::schema::SchemaSet;

/// Statistics from derivation validation
#[derive(Debug, Default)]
pub struct DerivationStats {
    /// Number of simple types validated
    pub simple_types_validated: usize,
    /// Number of complex types validated
    pub complex_types_validated: usize,
    /// Number of list types validated
    pub list_types_validated: usize,
    /// Number of union types validated
    pub union_types_validated: usize,
    /// Number of restriction derivations validated
    pub restrictions_validated: usize,
    /// Number of extension derivations validated
    pub extensions_validated: usize,
    /// Number of errors encountered
    pub errors: usize,
}

/// Validate all type derivations in a schema set
///
/// Uses the dependency graph to process types in topological order,
/// ensuring base types are validated before derived types.
///
/// # Arguments
///
/// * `schema_set` - The schema set with resolved references
/// * `dep_graph` - The dependency graph with sorted types
///
/// # Errors
///
/// Returns the first error encountered. All errors have source locations.
pub fn validate_all_derivations(
    schema_set: &SchemaSet,
    dep_graph: &DependencyGraph,
) -> SchemaResult<DerivationStats> {
    let mut stats = DerivationStats::default();
    let mut errors: Vec<SchemaError> = Vec::new();

    // Process types in compilation order (dependencies first)
    for &type_key in dep_graph.compilation_order() {
        match type_key {
            TypeKey::Simple(key) => {
                if let Err(e) = validate_simple_type(schema_set, key, &mut stats) {
                    errors.push(e);
                    stats.errors += 1;
                }
            }
            TypeKey::Complex(key) => {
                if let Err(e) = validate_complex_type(schema_set, key, &mut stats) {
                    errors.push(e);
                    stats.errors += 1;
                }
            }
        }
    }

    // §src-redefine 6.2.2 / 7.2.2 deferred restriction checks — must run
    // after reference resolution and type-derivation passes, because
    // `resolved_particle_types` / `resolved_attributes` on the flagged
    // groups are only populated post-resolve.
    validate_all_redefine_group_restrictions(schema_set, &mut errors, &mut stats);
    validate_all_redefine_attribute_group_restrictions(schema_set, &mut errors, &mut stats);

    // src-attribute_group circularity: an attribute group cannot transitively
    // reference itself in XSD 1.0. XSD 1.1 explicitly relaxed this — circular
    // attribute groups are permitted (W3C Bugzilla 15795). Walks `resolved_ref`
    // and `resolved_attribute_groups` for each group via DFS and flags any
    // back-edge in XSD 1.0 mode only.
    if schema_set.is_xsd10() {
        validate_attribute_group_no_circular(schema_set, &mut errors);
    } else {
        // XSD 1.1: circular attribute groups are allowed in general, but a
        // schema-level `defaultAttributes` group cannot itself participate in
        // a cycle — `resolve_all_references` injects the resolved group into
        // every applicable complex type, so a cycle here would imply the
        // schema-for-schemas validity rule §3.6.3 (no cycles via the
        // defaulting closure). Targeted DFS only on each document's selected
        // defaultAttributes group avoids re-enabling the global ban.
        validate_default_attribute_groups_no_circular(schema_set, &mut errors);
    }

    // Return first error if any
    if let Some(first_error) = errors.into_iter().next() {
        return Err(first_error);
    }

    Ok(stats)
}

/// Walk the attribute-group reference DAG and report cycles.
///
/// Each `xs:attributeGroup` definition references zero or more nested
/// attribute groups (including self via `ref` or `<attributeGroup ref=...>`
/// children). The schema-for-schemas does not allow circular references —
/// `src-attribute_group` constraint 3 in XSD 1.0 / §3.6.3 in XSD 1.1 forbid
/// any group from transitively referencing itself.
fn validate_attribute_group_no_circular(schema_set: &SchemaSet, errors: &mut Vec<SchemaError>) {
    use std::collections::HashSet;

    // Iterate every attribute group key once.
    let keys: Vec<_> = schema_set.arenas.attribute_groups.keys().collect();

    for start in keys {
        let mut path: Vec<AttributeGroupKey> = Vec::new();
        let mut visited: HashSet<AttributeGroupKey> = HashSet::new();
        if let Some(cycle_key) =
            find_attribute_group_cycle(schema_set, start, &mut path, &mut visited)
        {
            let location = schema_set
                .arenas
                .attribute_groups
                .get(cycle_key)
                .and_then(|ag| ag.source.as_ref())
                .and_then(|s| schema_set.source_maps.locate(s));
            errors.push(SchemaError::structural(
                "src-attribute_group",
                "Circular attribute group reference detected",
                location,
            ));
        }
    }
}

/// DFS helper for `validate_attribute_group_no_circular`. Returns the key
/// involved in the cycle (the first repeated node on the stack) when one is
/// found.
fn find_attribute_group_cycle(
    schema_set: &SchemaSet,
    key: AttributeGroupKey,
    path: &mut Vec<AttributeGroupKey>,
    visited: &mut std::collections::HashSet<AttributeGroupKey>,
) -> Option<AttributeGroupKey> {
    if path.contains(&key) {
        return Some(key);
    }
    if !visited.insert(key) {
        return None;
    }
    path.push(key);

    let result = if let Some(ag) = schema_set.arenas.attribute_groups.get(key) {
        let mut found: Option<AttributeGroupKey> = None;
        if let Some(ref_key) = ag.resolved_ref {
            if let Some(c) = find_attribute_group_cycle(schema_set, ref_key, path, visited) {
                found = Some(c);
            }
        }
        if found.is_none() {
            for &nested_key in &ag.resolved_attribute_groups {
                if let Some(c) = find_attribute_group_cycle(schema_set, nested_key, path, visited) {
                    found = Some(c);
                    break;
                }
            }
        }
        found
    } else {
        None
    };

    path.pop();
    result
}

/// XSD 1.1: Validate that the schema-level `defaultAttributes` selected groups
/// are not part of a circular reference chain. Targeted to the defaultAttributes
/// closure only — XSD 1.1 (W3C Bugzilla 15795) permits circular AGs in general,
/// but the defaulting injection in `resolver::resolve_all_references` would
/// loop forever if its starting group were itself cyclic.
fn validate_default_attribute_groups_no_circular(
    schema_set: &SchemaSet,
    errors: &mut Vec<SchemaError>,
) {
    use std::collections::HashSet;
    let mut seen_starts: HashSet<AttributeGroupKey> = HashSet::new();
    for doc in &schema_set.documents {
        let Some(ref qname) = doc.default_attributes else {
            continue;
        };
        let Some(start) = schema_set.lookup_attribute_group(qname.namespace_uri, qname.local_name)
        else {
            continue; // unresolvable defaultAttributes already reported elsewhere
        };
        if !seen_starts.insert(start) {
            continue;
        }
        let mut path: Vec<AttributeGroupKey> = Vec::new();
        let mut visited: HashSet<AttributeGroupKey> = HashSet::new();
        if let Some(cycle_key) =
            find_attribute_group_cycle(schema_set, start, &mut path, &mut visited)
        {
            let location = schema_set
                .arenas
                .attribute_groups
                .get(cycle_key)
                .and_then(|ag| ag.source.as_ref())
                .and_then(|s| schema_set.source_maps.locate(s));
            errors.push(SchemaError::structural(
                "src-attribute_group",
                "Circular attribute group reference detected via defaultAttributes",
                location,
            ));
        }
    }
}

/// Format a type name for error messages
pub(crate) fn format_type_name(
    schema_set: &SchemaSet,
    name: Option<NameId>,
    namespace: Option<NameId>,
) -> String {
    match name {
        Some(name_id) => {
            let local = schema_set.name_table.resolve(name_id);
            match namespace {
                Some(ns_id) => {
                    let ns = schema_set.name_table.resolve(ns_id);
                    if ns.is_empty() {
                        local.to_string()
                    } else {
                        format!("{{{}}}{}", ns, local)
                    }
                }
                None => local.to_string(),
            }
        }
        None => "(anonymous)".to_string(),
    }
}

/// Minimal "type-like component" view used by [`type_error_context`].
/// Implemented by the type-def structs whose errors carry both a source
/// location and a formatted type name.
pub(crate) trait TypeDefForError {
    fn error_name(&self) -> Option<NameId>;
    fn error_target_namespace(&self) -> Option<NameId>;
    fn error_source(&self) -> Option<&SourceRef>;
}

impl TypeDefForError for crate::arenas::SimpleTypeDefData {
    fn error_name(&self) -> Option<NameId> {
        self.name
    }
    fn error_target_namespace(&self) -> Option<NameId> {
        self.target_namespace
    }
    fn error_source(&self) -> Option<&SourceRef> {
        self.source.as_ref()
    }
}

impl TypeDefForError for crate::arenas::ComplexTypeDefData {
    fn error_name(&self) -> Option<NameId> {
        self.name
    }
    fn error_target_namespace(&self) -> Option<NameId> {
        self.target_namespace
    }
    fn error_source(&self) -> Option<&SourceRef> {
        self.source.as_ref()
    }
}

/// Returns `(location, type_name)` for error construction on a type component.
/// Pairs [`SchemaSet::locate`] with [`format_type_name`] since they always
/// co-occur in `SchemaError::structural` calls built for a type.
pub(crate) fn type_error_context<T: TypeDefForError>(
    schema_set: &SchemaSet,
    type_def: &T,
) -> (Option<SourceLocation>, String) {
    (
        schema_set.locate(type_def.error_source()),
        format_type_name(
            schema_set,
            type_def.error_name(),
            type_def.error_target_namespace(),
        ),
    )
}

/// Delegate to `SchemaSet::is_type_derived_from` with no method exclusions.
fn is_type_derived_from(schema_set: &SchemaSet, derived_key: TypeKey, base_key: TypeKey) -> bool {
    schema_set.is_type_derived_from(derived_key, base_key, DerivationSet::empty())
}
