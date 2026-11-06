//! Schema component model
//!
//! This module contains the schema object model including elements, types, groups, etc.
//!
//! ## Module Structure
//!
//! - `model` - SchemaSet, SchemaDocument, NamespaceTable
//! - `decl` - Element and attribute declarations
//! - `group` - Model groups and attribute groups
//! - `wildcard` - Wildcard specifications
//! - `annotation` - Annotations and documentation
//! - `resolver` - Reference resolution (QName → component ID)
//! - `inline` - Inline type assembly (TypeRefResult::Inline → TypeKey)
//! - `dependencies` - Dependency graph for type compilation order
//! - `derivation` - Type derivation validation
//! - `redefine` - xs:redefine processing
//! - `override_dir` - xs:override processing (XSD 1.1)
//! - `value_constraint` - an element declaration's {value constraint} for hosts

pub mod annotation;
pub mod composition;
#[cfg(feature = "xsd11")]
pub mod cta;
pub mod decl;
pub mod dependencies;
pub mod derivation;
#[cfg(feature = "xsd11")]
pub mod edc;
pub mod group;
pub mod inline;
pub mod model;
pub(crate) mod nesting;
#[cfg(feature = "xsd11")]
pub mod override_dir;
pub mod redefine;
pub mod resolver;
pub mod value_constraint;
pub mod wildcard;

// Re-exports from model
pub use model::{
    DefaultOpenContent, DerivationSet, FormChoice, ImportDirective, IncludeDirective,
    NamespaceTable, OpenContentMode, OverrideDirective, RedefineDirective, SchemaDocument,
    SchemaSet, XsdVersion,
};

// Re-exports from decl
pub use decl::{
    AttributeDecl, DeclarationScope, ElementDecl, FormKind, NotationDecl, TypeReference,
    ValueConstraint,
};

// Re-exports from value_constraint
pub use value_constraint::{ElementValueConstraint, ValueConstraintVariety};

// Re-exports from group
pub use group::{AttributeGroupDef, AttributeGroupRef, ModelGroupDef, ModelGroupRef, Occurrence};

// Re-exports from wildcard
pub use wildcard::{ElementWildcard, NamespaceConstraint, ProcessContents};

// Re-exports from composition
pub use composition::{
    ComponentIdentity, ComponentKey, ComponentKind, ComponentOrigin, CompositionAction,
    CompositionEdge, CompositionEdgeKind, DocumentComponentIndex, EffectiveComponent,
};

// Re-exports from annotation
pub use annotation::{
    Annotation, AnnotationItem, AppInfoElement, DocumentationElement, ForeignAttribute, XmlFragment,
};

// Re-exports from resolver
pub use resolver::{
    finalize_pending_ic_refs, resolve_all_references, ReferenceResolver, ResolutionStats,
    ResolvedReferences,
};

// Re-exports from inline
pub use inline::{
    allocate_content_particle_elements, allocate_model_group_particle_elements,
    assemble_inline_types, InlineAssemblyStats,
};

// Re-exports from dependencies
pub use dependencies::{build_dependency_graph, DependencyGraph, DependencyStats};

// Re-exports from derivation
#[cfg(feature = "xsd11")]
pub use cta::validate_assertion_xpath;
#[cfg(feature = "xsd11")]
pub use cta::validate_cta_substitutability;
#[cfg(feature = "xsd11")]
pub use cta::validate_cta_xpath;
#[cfg(feature = "xsd11")]
pub use derivation::validate_element_type_alternatives;
#[cfg(feature = "xsd11")]
pub use derivation::validate_local_element_type_table_consistency;
#[cfg(feature = "xsd11")]
pub use derivation::validate_restriction_local_element_type_table_consistency;
#[cfg(feature = "xsd11")]
pub use derivation::validate_wildcard_disallowed_names;
#[cfg(feature = "xsd11")]
pub use derivation::validate_wildcard_element_type_table_consistency;
pub use derivation::{
    validate_all_derivations, validate_attribute_id_constraints,
    validate_attribute_value_constraints, validate_complex_type_attribute_uniqueness,
    validate_element_value_constraints, validate_local_decl_target_namespace,
    validate_no_xsi_attribute_declarations, validate_substitution_group_element_consistency,
    validate_xsd10_annotation_source_anyuri, DerivationStats as DerivationValidationStats,
};

// Re-exports from redefine
pub use redefine::apply_redefine;

// Re-exports from override_dir
#[cfg(feature = "xsd11")]
pub use override_dir::apply_override;

use crate::error::{SchemaError, SchemaResult};

/// Compile all deferred pattern facets across every simple type in the schema set.
///
/// Patterns added via `add_pattern_unchecked` during parsing are not compiled until
/// this function runs. Any invalid XSD regex pattern causes a structural error,
/// making the schema invalid.
///
/// Must be called after reference resolution so that all types are fully assembled.
pub fn compile_all_patterns(schema_set: &mut SchemaSet) -> SchemaResult<()> {
    let xsd_version = schema_set.xsd_version;
    let regex_compat = schema_set.regex_compatibility;
    let keys: Vec<_> = schema_set.arenas.simple_types.keys().collect();
    let arenas = schema_set.arenas.entries_mut();
    for key in keys {
        let type_def = &mut arenas.simple_types[key];
        if let Err(facet_err) = type_def.facets.compile_patterns(xsd_version, regex_compat) {
            let source = type_def.source.clone();
            let location = source
                .as_ref()
                .and_then(|src| schema_set.source_maps.locate(src));
            return Err(SchemaError::structural(
                "pattern-valid",
                facet_err.to_string(),
                location,
            ));
        }
    }
    Ok(())
}

/// Apply all redefine and override directives collected from loaded documents,
/// then build effective component provenance records.
///
/// This must be called after all participating schemas (including redefine/override
/// targets) have been parsed and loaded into the schema set, but before inline
/// assembly and reference resolution.
///
/// ## Phases
///
/// 1. **Collect** — gather all declared components from every document's
///    `component_index` and detect composition-time duplicates (`sch-props-correct.2`).
/// 2. **Apply** — run redefine/override directives, which mutate namespace tables
///    and record `Redefined`/`Overridden` provenance.
/// 3. **Store** — save the effective component list on `SchemaSet` for later
///    diagnostic and provenance queries.
pub fn apply_redefine_override(schema_set: &mut SchemaSet) -> SchemaResult<()> {
    // Phase 0: validate that every redefine has a real parse-time original.
    // Cyclic xs:include is legal per §4.2.3 and several W3C/IBM fixtures
    // (`schU1`, `D1→D2→D3` chains) demonstrate that legitimate transitive
    // redefine chains must continue to be accepted. The constraint we
    // enforce here is the narrower §src-redefine rule that the redefining
    // component must reference an *existing* original — not one that is
    // only present because the chained-redefine cross-doc insert (later in
    // this function) has masked its absence.
    validate_redefine_originals_exist(schema_set)?;

    // Phase 1: collect declared components from all documents
    collect_declared_components(schema_set);

    // Phase 2: apply redefines (records Redefined provenance).
    //
    // Redefines must run so that inner/leaf redefines execute before outer
    // ones that depend on their results. Document order alone is not a
    // reliable proxy: breadth-first loading assigns *lower* doc_ids to outer
    // documents, while pre-loaded (`parse_schema_only` + `process_loaded_schemas`)
    // callers typically push dependencies first, giving them *lower* doc_ids.
    // Sort by dependency depth instead: depth(doc) = 0 when the doc redefines
    // nothing, else 1 + max depth of its redefine targets. Processing
    // ascending by depth guarantees each target doc's own redefines are
    // applied before any redefine that uses that target.
    let redefines = topologically_ordered_redefines(schema_set);
    for redefine in redefines {
        apply_redefine(schema_set, &redefine)?;
    }

    // Phase 2a: reject invalid override schemas (§4.2.5) before any
    // override side-effect touches namespace tables or provenance.
    #[cfg(feature = "xsd11")]
    override_dir::validate_override_directives(schema_set)?;

    // Phase 2b: apply overrides in topological order so that outer
    // overrides of the same component always win over inner ones
    // reached through the include/override closure. Flat document-order
    // iteration mis-orders the `over009` double-override shape because
    // the BFS loader assigns outer docs *lower* ids.
    #[cfg(feature = "xsd11")]
    {
        let overrides = topologically_ordered_overrides(schema_set);
        for override_dir in overrides {
            apply_override(schema_set, &override_dir)?;
        }
    }

    Ok(())
}

/// Validate that every `<xs:redefine>` directive's redefined components
/// have a real *parse-time* original in the target schema, reachable via a
/// non-cyclic chain of redefines and includes.
///
/// ## Why this exists
///
/// `apply_*_redefine` already performs a per-component lookup against the
/// target document's `component_index`, but that index is mutated by the
/// chained-redefine cross-doc insert at the end of each apply call. In a
/// cyclic redefine pair like the IBM `s4_2_4si01b` fixture (`01b` redefines
/// `01`, `01` redefines `01b`, only `01b` declares `c1` at top level),
/// applying `01`'s redefine first inserts the new `c1` into `01`'s index;
/// `01b`'s subsequent redefine of `01` then "finds" `c1` in `01` only
/// because of that insert. The original `c1` never existed in `01` at parse
/// time, so the redefine should be rejected as `src-redefine` invalid.
///
/// We catch the case here, *before* any redefine runs, by walking the chain
/// from the target document toward a top-level declaration **without ever
/// stepping back through the redefining document**. The exclusion is what
/// distinguishes a cyclic non-anchored case from a legitimate transitive
/// chain `D₁ → D₂ → D₃` where only `D₃` declares the component at top level
/// (which is valid: `D₁`'s redefine of `D₂`'s c finds `c` via `D₂ → D₃`,
/// and the path `D₂ → D₃` does not loop back through `D₁`).
fn validate_redefine_originals_exist(schema_set: &SchemaSet) -> SchemaResult<()> {
    use crate::ids::{DocumentId, NameId};
    use crate::schema::composition::ComponentKind;
    use std::collections::HashSet;

    // DFS over the composition subgraph (redefine *and* include edges — the
    // original may be declared any number of include hops below the redefined
    // document, and §4.2.1/§4.2.3 make those declarations part of the schema
    // corresponding to it). `visiting` is a stack-scoped cycle break: callers
    // seed it with documents that must NOT be revisited (typically the
    // redefining document itself), and the walker inserts/removes its own
    // ancestors.
    fn lookup_via_composition_chain(
        schema_set: &SchemaSet,
        start_doc: DocumentId,
        kind: ComponentKind,
        namespace: Option<NameId>,
        name: NameId,
        visiting: &mut HashSet<DocumentId>,
    ) -> bool {
        if !visiting.insert(start_doc) {
            return false;
        }
        let found = (|| {
            let Some(doc) = schema_set.documents.get(start_doc as usize) else {
                return false;
            };
            let direct = match kind {
                ComponentKind::SimpleType => doc
                    .component_index
                    .lookup_simple_type(namespace, name)
                    .is_some(),
                ComponentKind::ComplexType => doc
                    .component_index
                    .lookup_complex_type(namespace, name)
                    .is_some(),
                ComponentKind::ModelGroup => doc
                    .component_index
                    .lookup_model_group(namespace, name)
                    .is_some(),
                ComponentKind::AttributeGroup => doc
                    .component_index
                    .lookup_attribute_group(namespace, name)
                    .is_some(),
                _ => false,
            };
            if direct {
                return true;
            }
            for redefine in &doc.redefines {
                if let Some(target) = redefine.resolved_doc_id {
                    if lookup_via_composition_chain(
                        schema_set, target, kind, namespace, name, visiting,
                    ) {
                        return true;
                    }
                }
            }
            for include in &doc.includes {
                if let Some(target) = include.resolved_doc_id {
                    if lookup_via_composition_chain(
                        schema_set, target, kind, namespace, name, visiting,
                    ) {
                        return true;
                    }
                }
            }
            false
        })();
        visiting.remove(&start_doc);
        found
    }

    let make_err = |schema_set: &SchemaSet,
                    redefining_doc: &crate::schema::model::SchemaDocument,
                    directive: &crate::schema::model::RedefineDirective,
                    kind_label: &str,
                    name: NameId| {
        let target_label = directive
            .resolved_doc_id
            .and_then(|id| schema_set.documents.get(id as usize))
            .map(|d| d.base_uri.as_str())
            .unwrap_or(directive.schema_location.as_str());
        let location = directive
            .source
            .as_ref()
            .and_then(|s| schema_set.source_maps.locate(s));
        SchemaError::structural(
            "src-redefine",
            format!(
                "Original {} '{}' not found at parse time in '{}' for redefinition \
                 from '{}' (the redefine chain has no non-cyclic anchor)",
                kind_label,
                schema_set.name_table.resolve(name),
                target_label,
                redefining_doc.base_uri,
            ),
            location,
        )
    };

    for doc in &schema_set.documents {
        for redefine in &doc.redefines {
            let Some(target_doc_id) = redefine.resolved_doc_id else {
                continue;
            };

            // Each redefined component contributes one
            // (kind, namespace, name, label) tuple. The label is for
            // diagnostics; the rest drives the chain walk.
            let simples = redefine.simple_types.iter().filter_map(|&k| {
                let st = schema_set.arenas.simple_types.get(k)?;
                Some((
                    ComponentKind::SimpleType,
                    st.target_namespace,
                    st.name?,
                    "simple type",
                ))
            });
            let complexes = redefine.complex_types.iter().filter_map(|&k| {
                let ct = schema_set.arenas.complex_types.get(k)?;
                Some((
                    ComponentKind::ComplexType,
                    ct.target_namespace,
                    ct.name?,
                    "complex type",
                ))
            });
            let groups = redefine.groups.iter().filter_map(|&k| {
                let g = schema_set.arenas.model_groups.get(k)?;
                Some((
                    ComponentKind::ModelGroup,
                    g.target_namespace,
                    g.name?,
                    "model group",
                ))
            });
            let attr_groups = redefine.attribute_groups.iter().filter_map(|&k| {
                let ag = schema_set.arenas.attribute_groups.get(k)?;
                Some((
                    ComponentKind::AttributeGroup,
                    ag.target_namespace,
                    ag.name?,
                    "attribute group",
                ))
            });

            // Pre-seed the visiting set with the redefining document so
            // cyclic self-references through the chain cannot satisfy the
            // lookup. The DFS walker inserts/removes its own ancestors and
            // restores `visiting` to this seeded state on return, so we can
            // reuse the same set across all components.
            let mut visiting: HashSet<DocumentId> = HashSet::with_capacity(4);
            visiting.insert(doc.id);

            for (kind, namespace, name, label) in
                simples.chain(complexes).chain(groups).chain(attr_groups)
            {
                if !lookup_via_composition_chain(
                    schema_set,
                    target_doc_id,
                    kind,
                    namespace,
                    name,
                    &mut visiting,
                ) {
                    return Err(make_err(schema_set, doc, redefine, label, name));
                }
            }
        }
    }

    Ok(())
}

/// Return all collected redefine directives ordered so that every redefine's
/// target document has already had its own redefines applied.
///
/// The order is computed from a per-document *dependency depth*:
///   depth(doc) = 0 when doc has no redefines and no includes, else
///                max(
///                  1 + max(depth(target) for each redefine target),
///                  1 + max(depth(target) for each include target),
///                )
///
/// **Why includes count too.** When an outer schema includes a document
/// that already redefines component X *and* the outer schema also
/// directly redefines X, the spec is famously underspecified about which
/// redefine wins (W3C bug 4136). The IBM/W3C `schU*` test sets resolve
/// the ambiguity by treating the outer caller's redefine as the
/// conventional winner: it must be applied *after* anything dragged in
/// via include. Adding includes to the depth recurrence gives the outer
/// document a strictly higher depth than any document it pulls in, so
/// it sorts after them and its redefine lands last (and wins).
///
/// Documents are then stable-sorted by their depth (ascending). Redefines
/// keep their original document order for equal depths, so within a single
/// document the per-redefine order matches source order.
///
/// Cycles can legally occur via include-of-redefine-of-includer chains
/// (the `schU*` family is exactly this shape). The traversal is
/// defensive: any revisited node returns depth 0 from the cycle guard,
/// breaking the loop without caching the broken value, so the eventual
/// cached depth still reflects the longest *acyclic* path through the
/// graph.
fn topologically_ordered_redefines(
    schema_set: &SchemaSet,
) -> Vec<crate::schema::model::RedefineDirective> {
    use crate::ids::DocumentId;
    use crate::schema::model::{RedefineDirective, SchemaDocument};
    use std::collections::{HashMap, HashSet};

    fn depth(
        doc_id: DocumentId,
        docs: &[SchemaDocument],
        cache: &mut HashMap<DocumentId, usize>,
        visiting: &mut HashSet<DocumentId>,
    ) -> usize {
        if let Some(&d) = cache.get(&doc_id) {
            return d;
        }
        if !visiting.insert(doc_id) {
            // Cycle guard: treat revisited nodes as depth 0.
            return 0;
        }
        let d = docs
            .iter()
            .find(|d| d.id == doc_id)
            .map(|doc| {
                let mut max_dep = 0usize;
                for r in &doc.redefines {
                    if let Some(target) = r.resolved_doc_id {
                        let t = depth(target, docs, cache, visiting) + 1;
                        if t > max_dep {
                            max_dep = t;
                        }
                    }
                }
                // §schU* convention: the includer must be deeper than
                // anything it includes, so its own redefines apply
                // *after* the included document's redefines.
                for inc in &doc.includes {
                    if let Some(target) = inc.resolved_doc_id {
                        let t = depth(target, docs, cache, visiting) + 1;
                        if t > max_dep {
                            max_dep = t;
                        }
                    }
                }
                max_dep
            })
            .unwrap_or(0);
        visiting.remove(&doc_id);
        cache.insert(doc_id, d);
        d
    }

    let mut cache: HashMap<DocumentId, usize> = HashMap::new();
    for doc in &schema_set.documents {
        depth(
            doc.id,
            &schema_set.documents,
            &mut cache,
            &mut HashSet::new(),
        );
    }

    // Tag each redefine with its source doc's depth, then stable-sort ascending.
    let mut tagged: Vec<(usize, RedefineDirective)> = schema_set
        .documents
        .iter()
        .flat_map(|doc| {
            let d = cache.get(&doc.id).copied().unwrap_or(0);
            doc.redefines.iter().cloned().map(move |r| (d, r))
        })
        .collect();
    tagged.sort_by_key(|(d, _)| *d);
    tagged.into_iter().map(|(_, r)| r).collect()
}

/// Return all collected override directives ordered so that every
/// override's target document has already had its own overrides applied.
///
/// Mirrors [`topologically_ordered_redefines`] but walks override + include
/// edges: an override's target set is the transitive closure of include +
/// override edges (§4.2.5), so an outer document must sort strictly after
/// every document it reaches that way. Depth recurrence:
/// `depth(doc) = 1 + max(depth(target))` over `(overrides ∪ includes)`,
/// zero when neither set is non-empty.
#[cfg(feature = "xsd11")]
fn topologically_ordered_overrides(
    schema_set: &SchemaSet,
) -> Vec<crate::schema::model::OverrideDirective> {
    use crate::ids::DocumentId;
    use crate::schema::model::{OverrideDirective, SchemaDocument};
    use std::collections::{HashMap, HashSet};

    fn depth(
        doc_id: DocumentId,
        docs: &[SchemaDocument],
        cache: &mut HashMap<DocumentId, usize>,
        visiting: &mut HashSet<DocumentId>,
    ) -> usize {
        if let Some(&d) = cache.get(&doc_id) {
            return d;
        }
        if !visiting.insert(doc_id) {
            // Cycle guard: treat revisited nodes as depth 0.
            return 0;
        }
        let d = docs
            .iter()
            .find(|d| d.id == doc_id)
            .map(|doc| {
                let mut max_dep = 0usize;
                for o in &doc.overrides {
                    if let Some(target) = o.resolved_doc_id {
                        let t = depth(target, docs, cache, visiting) + 1;
                        if t > max_dep {
                            max_dep = t;
                        }
                    }
                }
                for inc in &doc.includes {
                    if let Some(target) = inc.resolved_doc_id {
                        let t = depth(target, docs, cache, visiting) + 1;
                        if t > max_dep {
                            max_dep = t;
                        }
                    }
                }
                max_dep
            })
            .unwrap_or(0);
        visiting.remove(&doc_id);
        cache.insert(doc_id, d);
        d
    }

    let mut cache: HashMap<DocumentId, usize> = HashMap::new();
    for doc in &schema_set.documents {
        depth(
            doc.id,
            &schema_set.documents,
            &mut cache,
            &mut HashSet::new(),
        );
    }

    let mut tagged: Vec<(usize, OverrideDirective)> = schema_set
        .documents
        .iter()
        .flat_map(|doc| {
            let d = cache.get(&doc.id).copied().unwrap_or(0);
            doc.overrides.iter().cloned().map(move |o| (d, o))
        })
        .collect();
    tagged.sort_by_key(|(d, _)| *d);
    tagged.into_iter().map(|(_, o)| o).collect()
}

/// Collect all declared components from every document's component index
/// into `schema_set.effective_components` with provenance metadata.
///
/// Components from included documents are marked `Included`; components
/// declared in root documents are marked `Declared`. When the same identity
/// appears from multiple documents (e.g. via include), the last-registered
/// entry wins, matching current namespace-table behavior.
///
/// Note: duplicate-component detection (`sch-props-correct.2`) is handled
/// at parse time by `register_*` in `assemble.rs`. Composition-time
/// detection will be added when namespace tables are rebuilt from
/// effective components (future step).
fn collect_declared_components(schema_set: &mut SchemaSet) {
    use crate::ids::DocumentId;
    use crate::schema::composition::CompositionEdgeKind;
    use std::collections::HashMap;

    // Build set of (target_doc → source_doc) for Include edges.
    let mut included_from: HashMap<DocumentId, DocumentId> = HashMap::new();
    for edge in &schema_set.composition_edges {
        if edge.kind == CompositionEdgeKind::Include {
            if let Some(target) = edge.target_doc {
                included_from.entry(target).or_insert(edge.source_doc);
            }
        }
    }

    let mut effective: HashMap<ComponentIdentity, EffectiveComponent> = HashMap::new();

    for doc in &schema_set.documents {
        for (&identity, &key) in doc.component_index.iter() {
            let origin = ComponentOrigin {
                owner_doc: Some(doc.id),
                identity,
            };
            let action = if let Some(&from_doc) = included_from.get(&doc.id) {
                CompositionAction::Included { from_doc }
            } else {
                CompositionAction::Declared
            };
            effective.insert(
                identity,
                EffectiveComponent {
                    key,
                    origin,
                    action,
                },
            );
        }
    }

    schema_set.effective_components = effective;
}
