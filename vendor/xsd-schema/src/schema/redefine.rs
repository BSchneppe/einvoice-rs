//! xs:redefine processing
//!
//! Redefine allows extending/restricting types and groups from an included schema.
//! The redefining component must:
//! - Have the same name as the original
//! - Reference itself as the base type (for type redefinitions)
//! - Reference itself within the group (for group redefinitions)
//!
//! # XSD 1.0 Constraints
//!
//! For simpleType/complexType redefinitions:
//! - The redefinition must derive from the original type by restriction or extension
//! - The base type reference must use the same name as the type being redefined (self-reference)
//!
//! For group/attributeGroup redefinitions:
//! - The redefinition must contain exactly one reference to the original group
//! - This reference is replaced with the original group's content

use crate::error::{SchemaError, SchemaResult};
use crate::ids::*;
use crate::parser::frames::{ParticleResult, ParticleTerm};
use crate::schema::composition::{
    record_provenance, redefined_action, ComponentIdentity, ComponentKey, ComponentKind,
};
use crate::schema::model::RedefineDirective;
use crate::schema::SchemaSet;
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Transitive original lookup
// ---------------------------------------------------------------------------
//
// `src-redefine` requires the redefined component's original to be found in
// "the schema corresponding to" the redefined document — i.e. the *effective
// view* of that document, which per §4.2.1/§4.2.3 includes everything its own
// `xs:include` and `xs:redefine` directives pull in transitively, not merely
// its own top-level declarations.
//
// The lookups below therefore start at the redefine target document and walk
// its `redefines` + `includes` edges (via `resolved_doc_id`) depth-first,
// guarded by a `HashSet<DocumentId>` so include/redefine cycles — which are
// legal, see `topologically_ordered_redefines` — terminate. Each helper is
// kind-scoped so a name collision across component kinds cannot satisfy the
// lookup, and the walk never leaves the composition closure of the redefined
// document, so it cannot "find" an original that lives in an unrelated schema.

fn lookup_simple_original(
    schema_set: &SchemaSet,
    doc_id: DocumentId,
    namespace: Option<NameId>,
    name: NameId,
    visited: &mut HashSet<DocumentId>,
) -> Option<TypeKey> {
    if !visited.insert(doc_id) {
        return None;
    }
    let found = schema_set.documents.get(doc_id as usize).and_then(|doc| {
        doc.component_index
            .lookup_simple_type(namespace, name)
            .map(TypeKey::Simple)
            .or_else(|| {
                composition_targets(doc).find_map(|target| {
                    lookup_simple_original(schema_set, target, namespace, name, visited)
                })
            })
    });
    visited.remove(&doc_id);
    found
}

fn lookup_complex_original(
    schema_set: &SchemaSet,
    doc_id: DocumentId,
    namespace: Option<NameId>,
    name: NameId,
    visited: &mut HashSet<DocumentId>,
) -> Option<TypeKey> {
    if !visited.insert(doc_id) {
        return None;
    }
    let found = schema_set.documents.get(doc_id as usize).and_then(|doc| {
        doc.component_index
            .lookup_complex_type(namespace, name)
            .map(TypeKey::Complex)
            .or_else(|| {
                composition_targets(doc).find_map(|target| {
                    lookup_complex_original(schema_set, target, namespace, name, visited)
                })
            })
    });
    visited.remove(&doc_id);
    found
}

fn lookup_model_group_original(
    schema_set: &SchemaSet,
    doc_id: DocumentId,
    namespace: Option<NameId>,
    name: NameId,
    visited: &mut HashSet<DocumentId>,
) -> Option<ModelGroupKey> {
    if !visited.insert(doc_id) {
        return None;
    }
    let found = schema_set.documents.get(doc_id as usize).and_then(|doc| {
        doc.component_index
            .lookup_model_group(namespace, name)
            .or_else(|| {
                composition_targets(doc).find_map(|target| {
                    lookup_model_group_original(schema_set, target, namespace, name, visited)
                })
            })
    });
    visited.remove(&doc_id);
    found
}

fn lookup_attribute_group_original(
    schema_set: &SchemaSet,
    doc_id: DocumentId,
    namespace: Option<NameId>,
    name: NameId,
    visited: &mut HashSet<DocumentId>,
) -> Option<AttributeGroupKey> {
    if !visited.insert(doc_id) {
        return None;
    }
    let found = schema_set.documents.get(doc_id as usize).and_then(|doc| {
        doc.component_index
            .lookup_attribute_group(namespace, name)
            .or_else(|| {
                composition_targets(doc).find_map(|target| {
                    lookup_attribute_group_original(schema_set, target, namespace, name, visited)
                })
            })
    });
    visited.remove(&doc_id);
    found
}

/// Documents that contribute to `doc`'s effective view: its redefine targets
/// followed by its include targets. Unresolved directives are skipped.
fn composition_targets(
    doc: &crate::schema::model::SchemaDocument,
) -> impl Iterator<Item = DocumentId> + '_ {
    doc.redefines
        .iter()
        .filter_map(|r| r.resolved_doc_id)
        .chain(doc.includes.iter().filter_map(|i| i.resolved_doc_id))
}

/// Apply a redefine directive to the schema set.
///
/// This replaces the original components with the redefined versions,
/// after validating the redefinition constraints.
///
/// Uses document-scoped lookup via `resolved_doc_id` when available,
/// falling back to global namespace table lookup for backward compatibility.
pub fn apply_redefine(
    schema_set: &mut SchemaSet,
    redefine: &RedefineDirective,
) -> SchemaResult<()> {
    let target_doc_id = redefine.resolved_doc_id;
    // The redefining document is the one that contains the xs:redefine element
    let redefining_doc_id = redefine.source.as_ref().map(|s| s.doc_id);

    for simple_key in &redefine.simple_types {
        apply_simple_type_redefine(schema_set, *simple_key, target_doc_id, redefining_doc_id)?;
    }

    for complex_key in &redefine.complex_types {
        apply_complex_type_redefine(schema_set, *complex_key, target_doc_id, redefining_doc_id)?;
    }

    for group_key in &redefine.groups {
        apply_model_group_redefine(schema_set, *group_key, target_doc_id, redefining_doc_id)?;
    }

    for attr_group_key in &redefine.attribute_groups {
        apply_attribute_group_redefine(
            schema_set,
            *attr_group_key,
            target_doc_id,
            redefining_doc_id,
        )?;
    }

    Ok(())
}

/// Apply a simple type redefinition.
fn apply_simple_type_redefine(
    schema_set: &mut SchemaSet,
    new_key: SimpleTypeKey,
    target_doc_id: Option<DocumentId>,
    redefining_doc_id: Option<DocumentId>,
) -> SchemaResult<()> {
    let new_type = schema_set
        .arenas
        .simple_types
        .get(new_key)
        .ok_or_else(|| SchemaError::internal("Redefine: new simple type not found"))?;

    let name = new_type.name.ok_or_else(|| {
        SchemaError::structural(
            "src-redefine",
            "Redefined simple type must have a name",
            None,
        )
    })?;
    let namespace = new_type.target_namespace;

    // Kind-specific lookup scoped to the redefined document's *effective view*
    // (itself plus everything it includes/redefines transitively); global
    // fallback only when resolved_doc_id is None (pre-loaded schemas without
    // resolution).
    let original_key = match target_doc_id {
        Some(id) => lookup_simple_original(schema_set, id, namespace, name, &mut HashSet::new()),
        None => schema_set.lookup_type(namespace, name),
    }
    .ok_or_else(|| {
        SchemaError::structural(
            "src-redefine",
            format!(
                "Original simple type '{}' not found for redefinition in {}",
                schema_set.name_table.resolve(name),
                target_doc_id
                    .and_then(|id| schema_set.documents.get(id as usize))
                    .map(|d| d.base_uri.as_str())
                    .unwrap_or("schema"),
            ),
            None,
        )
    })?;

    validate_self_derivation_simple(schema_set, new_key, name, namespace)?;

    // Store original type key for base-type redirection during resolution
    if let TypeKey::Simple(orig_key) = original_key {
        if let Some(st) = schema_set
            .arenas
            .entries_mut()
            .simple_types
            .get_mut(new_key)
        {
            st.redefine_original = Some(orig_key);
        }
    }

    let ns_table = schema_set.get_or_create_namespace(namespace);
    ns_table.register_type(name, TypeKey::Simple(new_key));

    // Make the redefined type visible in the redefining document's
    // component_index so chained redefines can find it.
    if let Some(doc_id) = redefining_doc_id {
        if let Some(doc) = schema_set.documents.get_mut(doc_id as usize) {
            doc.component_index.insert(
                ComponentIdentity {
                    kind: ComponentKind::SimpleType,
                    name,
                    namespace,
                },
                ComponentKey::Type(TypeKey::Simple(new_key)),
            );
        }
    }

    record_provenance(
        &mut schema_set.effective_components,
        ComponentKey::Type(TypeKey::Simple(new_key)),
        ComponentKind::SimpleType,
        namespace,
        name,
        redefining_doc_id,
        redefined_action(
            redefining_doc_id,
            ComponentKind::SimpleType,
            name,
            namespace,
            target_doc_id,
        ),
    );

    Ok(())
}

/// Apply a complex type redefinition.
fn apply_complex_type_redefine(
    schema_set: &mut SchemaSet,
    new_key: ComplexTypeKey,
    target_doc_id: Option<DocumentId>,
    redefining_doc_id: Option<DocumentId>,
) -> SchemaResult<()> {
    let new_type = schema_set
        .arenas
        .complex_types
        .get(new_key)
        .ok_or_else(|| SchemaError::internal("Redefine: new complex type not found"))?;

    let name = new_type.name.ok_or_else(|| {
        SchemaError::structural(
            "src-redefine",
            "Redefined complex type must have a name",
            None,
        )
    })?;
    let namespace = new_type.target_namespace;

    let original_key = match target_doc_id {
        Some(id) => lookup_complex_original(schema_set, id, namespace, name, &mut HashSet::new()),
        None => schema_set.lookup_type(namespace, name),
    }
    .ok_or_else(|| {
        SchemaError::structural(
            "src-redefine",
            format!(
                "Original complex type '{}' not found for redefinition in {}",
                schema_set.name_table.resolve(name),
                target_doc_id
                    .and_then(|id| schema_set.documents.get(id as usize))
                    .map(|d| d.base_uri.as_str())
                    .unwrap_or("schema"),
            ),
            None,
        )
    })?;

    validate_self_derivation_complex(schema_set, new_key, name, namespace)?;

    // Store original type key for base-type redirection during resolution
    if let TypeKey::Complex(orig_key) = original_key {
        if let Some(ct) = schema_set
            .arenas
            .entries_mut()
            .complex_types
            .get_mut(new_key)
        {
            ct.redefine_original = Some(orig_key);
        }
    }

    let ns_table = schema_set.get_or_create_namespace(namespace);
    ns_table.register_type(name, TypeKey::Complex(new_key));

    if let Some(doc_id) = redefining_doc_id {
        if let Some(doc) = schema_set.documents.get_mut(doc_id as usize) {
            doc.component_index.insert(
                ComponentIdentity {
                    kind: ComponentKind::ComplexType,
                    name,
                    namespace,
                },
                ComponentKey::Type(TypeKey::Complex(new_key)),
            );
        }
    }

    record_provenance(
        &mut schema_set.effective_components,
        ComponentKey::Type(TypeKey::Complex(new_key)),
        ComponentKind::ComplexType,
        namespace,
        name,
        redefining_doc_id,
        redefined_action(
            redefining_doc_id,
            ComponentKind::ComplexType,
            name,
            namespace,
            target_doc_id,
        ),
    );

    Ok(())
}

/// Apply a model group redefinition.
fn apply_model_group_redefine(
    schema_set: &mut SchemaSet,
    new_key: ModelGroupKey,
    target_doc_id: Option<DocumentId>,
    redefining_doc_id: Option<DocumentId>,
) -> SchemaResult<()> {
    let new_group = schema_set
        .arenas
        .model_groups
        .get(new_key)
        .ok_or_else(|| SchemaError::internal("Redefine: new model group not found"))?;

    let name = new_group.name.ok_or_else(|| {
        SchemaError::structural(
            "src-redefine",
            "Redefined model group must have a name",
            None,
        )
    })?;
    let namespace = new_group.target_namespace;

    let original_key = match target_doc_id {
        Some(id) => {
            lookup_model_group_original(schema_set, id, namespace, name, &mut HashSet::new())
        }
        None => schema_set.lookup_model_group(namespace, name),
    }
    .ok_or_else(|| {
        SchemaError::structural(
            "src-redefine",
            format!(
                "Original group '{}' not found for redefinition in {}",
                schema_set.name_table.resolve(name),
                target_doc_id
                    .and_then(|id| schema_set.documents.get(id as usize))
                    .map(|d| d.base_uri.as_str())
                    .unwrap_or("schema"),
            ),
            None,
        )
    })?;

    let has_self_ref = validate_self_reference_group(schema_set, new_key, name)?;

    // Store original key so self-references can be redirected during resolution.
    // When the redefine has zero self-references, flag it for the deferred
    // §src-redefine 6.2.2 restriction check in `validate_all_derivations`.
    if let Some(group) = schema_set
        .arenas
        .entries_mut()
        .model_groups
        .get_mut(new_key)
    {
        group.redefine_original = Some(original_key);
        group.redefine_requires_restriction_check = !has_self_ref;
    }

    let ns_table = schema_set.get_or_create_namespace(namespace);
    ns_table.register_model_group(name, new_key);

    if let Some(doc_id) = redefining_doc_id {
        if let Some(doc) = schema_set.documents.get_mut(doc_id as usize) {
            doc.component_index.insert(
                ComponentIdentity {
                    kind: ComponentKind::ModelGroup,
                    name,
                    namespace,
                },
                ComponentKey::ModelGroup(new_key),
            );
        }
    }

    record_provenance(
        &mut schema_set.effective_components,
        ComponentKey::ModelGroup(new_key),
        ComponentKind::ModelGroup,
        namespace,
        name,
        redefining_doc_id,
        redefined_action(
            redefining_doc_id,
            ComponentKind::ModelGroup,
            name,
            namespace,
            target_doc_id,
        ),
    );

    Ok(())
}

/// Apply an attribute group redefinition.
fn apply_attribute_group_redefine(
    schema_set: &mut SchemaSet,
    new_key: AttributeGroupKey,
    target_doc_id: Option<DocumentId>,
    redefining_doc_id: Option<DocumentId>,
) -> SchemaResult<()> {
    let new_group = schema_set
        .arenas
        .attribute_groups
        .get(new_key)
        .ok_or_else(|| SchemaError::internal("Redefine: new attribute group not found"))?;

    let name = new_group.name.ok_or_else(|| {
        SchemaError::structural(
            "src-redefine",
            "Redefined attribute group must have a name",
            None,
        )
    })?;
    let namespace = new_group.target_namespace;

    let original_key = match target_doc_id {
        Some(id) => {
            lookup_attribute_group_original(schema_set, id, namespace, name, &mut HashSet::new())
        }
        None => schema_set.lookup_attribute_group(namespace, name),
    }
    .ok_or_else(|| {
        SchemaError::structural(
            "src-redefine",
            format!(
                "Original attribute group '{}' not found for redefinition in {}",
                schema_set.name_table.resolve(name),
                target_doc_id
                    .and_then(|id| schema_set.documents.get(id as usize))
                    .map(|d| d.base_uri.as_str())
                    .unwrap_or("schema"),
            ),
            None,
        )
    })?;

    let has_self_ref = validate_self_reference_attribute_group(schema_set, new_key, name)?;

    // Store original key so self-references can be redirected during resolution.
    // When the redefine has zero self-references, flag it for the deferred
    // §src-redefine 7.2.2 restriction check in `validate_all_derivations`.
    if let Some(group) = schema_set
        .arenas
        .entries_mut()
        .attribute_groups
        .get_mut(new_key)
    {
        group.redefine_original = Some(original_key);
        group.redefine_requires_restriction_check = !has_self_ref;
    }

    let ns_table = schema_set.get_or_create_namespace(namespace);
    ns_table.register_attribute_group(name, new_key);

    if let Some(doc_id) = redefining_doc_id {
        if let Some(doc) = schema_set.documents.get_mut(doc_id as usize) {
            doc.component_index.insert(
                ComponentIdentity {
                    kind: ComponentKind::AttributeGroup,
                    name,
                    namespace,
                },
                ComponentKey::AttributeGroup(new_key),
            );
        }
    }

    record_provenance(
        &mut schema_set.effective_components,
        ComponentKey::AttributeGroup(new_key),
        ComponentKind::AttributeGroup,
        namespace,
        name,
        redefining_doc_id,
        redefined_action(
            redefining_doc_id,
            ComponentKind::AttributeGroup,
            name,
            namespace,
            target_doc_id,
        ),
    );

    Ok(())
}

/// Validate that a simple type redefines itself (self-derivation constraint).
fn validate_self_derivation_simple(
    schema_set: &SchemaSet,
    type_key: SimpleTypeKey,
    expected_name: NameId,
    expected_namespace: Option<NameId>,
) -> SchemaResult<()> {
    use crate::parser::frames::TypeRefResult;

    let type_def = schema_set
        .arenas
        .simple_types
        .get(type_key)
        .ok_or_else(|| SchemaError::internal("Type not found"))?;

    // Check that base_type references the same name and namespace (self-reference).
    // Unprefixed QNames (namespace == None) are accepted — they resolve to the
    // target namespace during the reference-resolution phase.
    if let Some(TypeRefResult::QName(ref qname)) = type_def.base_type {
        if qname.local_name != expected_name {
            return Err(SchemaError::structural(
                "src-redefine",
                "Redefined simple type must derive from the original type (self-reference)",
                type_def
                    .source
                    .as_ref()
                    .and_then(|s| schema_set.source_maps.locate(s)),
            ));
        }
        // If the reference is explicitly namespace-qualified, it must match
        if let Some(ref_ns) = qname.namespace {
            if Some(ref_ns) != expected_namespace {
                return Err(SchemaError::structural(
                    "src-redefine",
                    "Redefined simple type base references a different namespace than the original",
                    type_def
                        .source
                        .as_ref()
                        .and_then(|s| schema_set.source_maps.locate(s)),
                ));
            }
        }
    } else {
        return Err(SchemaError::structural(
            "src-redefine",
            "Redefined simple type must have a base type reference",
            type_def
                .source
                .as_ref()
                .and_then(|s| schema_set.source_maps.locate(s)),
        ));
    }

    Ok(())
}

/// Validate that a complex type redefines itself (self-derivation constraint).
fn validate_self_derivation_complex(
    schema_set: &SchemaSet,
    type_key: ComplexTypeKey,
    expected_name: NameId,
    expected_namespace: Option<NameId>,
) -> SchemaResult<()> {
    use crate::parser::frames::TypeRefResult;

    let type_def = schema_set
        .arenas
        .complex_types
        .get(type_key)
        .ok_or_else(|| SchemaError::internal("Type not found"))?;

    // Check that base_type references the same name and namespace (self-reference).
    // Unprefixed QNames (namespace == None) are accepted — they resolve to the
    // target namespace during the reference-resolution phase.
    if let Some(TypeRefResult::QName(ref qname)) = type_def.base_type {
        if qname.local_name != expected_name {
            return Err(SchemaError::structural(
                "src-redefine",
                "Redefined complex type must derive from the original type (self-reference)",
                type_def
                    .source
                    .as_ref()
                    .and_then(|s| schema_set.source_maps.locate(s)),
            ));
        }
        // If the reference is explicitly namespace-qualified, it must match
        if let Some(ref_ns) = qname.namespace {
            if Some(ref_ns) != expected_namespace {
                return Err(SchemaError::structural(
                    "src-redefine",
                    "Redefined complex type base references a different namespace than the original",
                    type_def
                        .source
                        .as_ref()
                        .and_then(|s| schema_set.source_maps.locate(s)),
                ));
            }
        }
    } else {
        return Err(SchemaError::structural(
            "src-redefine",
            "Redefined complex type must have a base type reference",
            type_def
                .source
                .as_ref()
                .and_then(|s| schema_set.source_maps.locate(s)),
        ));
    }

    Ok(())
}

/// Scratch state for the §src-redefine 6.1 self-reference walker.
///
/// `count` is the only always-meaningful field. `min_occurs`/`max_occurs`
/// are the bounds of the most recently visited matching self-ref, and are
/// only meaningful when `count == 1` — §6.1.2 only constrains *the* one
/// self-referencing `<group ref>` in the valid case. When `count > 1`,
/// §6.1.1 already errors without consulting the bounds.
struct GroupSelfRefScan {
    count: u32,
    min_occurs: u32,
    max_occurs: Option<u32>,
}

/// Walk a particle list recursively to implement §src-redefine 6.1's
/// "among its contents at some level" scan (structures.html:13729-13741).
///
/// Descends through inline `<sequence>`/`<choice>`/`<all>` compositors,
/// stops at element particles (§6.1 element-ancestor exclusion), and does
/// not follow group references — this is a pre-resolution, purely
/// structural scan.
///
/// The walker mirrors the inline-compositor recursion shape from
/// `ParticleNormalizer::normalize_particle` in
/// `src/schema/derivation.rs`, but explicitly does **not** copy the
/// resolved-group follow-through in `normalize_group`: that post-resolution
/// code dives into a group ref's target, which would produce an incorrect
/// §6.1 interpretation here.
fn count_group_self_refs(
    particles: &[ParticleResult],
    expected_name: NameId,
    scan: &mut GroupSelfRefScan,
) {
    for particle in particles {
        if let ParticleTerm::Group(ref grp) = particle.term {
            match grp.ref_name {
                Some(ref ref_name) => {
                    // Syntactic group reference. Check for self-match;
                    // never recurse into its (empty-by-AST-invariant)
                    // particles vector.
                    if ref_name.local_name == expected_name {
                        scan.count += 1;
                        scan.min_occurs = particle.min_occurs;
                        scan.max_occurs = particle.max_occurs;
                    }
                }
                None => {
                    // Inline compositor (sequence/choice/all) — descend.
                    debug_assert!(
                        grp.name.is_none(),
                        "named group definition unexpectedly nested in a particle list"
                    );
                    count_group_self_refs(&grp.particles, expected_name, scan);
                }
            }
        }
        // ParticleTerm::Element — terminal per §6.1's
        // "does not have an <element> ancestor" exclusion. We intentionally
        // do NOT descend into ElementFrameResult.inline_type.
        // ParticleTerm::Any — terminal (no particles to scan).
    }
}

/// Validate that a redefining model group's self-reference structure is
/// consistent with §src-redefine clause 6
/// (`structures.html:13729-13752`, `#src-redefine`).
///
/// The scan is **recursive** through inline `<sequence>`/`<choice>`/`<all>`
/// compositors per §6.1's *"among its contents at some level"* wording,
/// and stops at any `<element>` ancestor per §6.1's element-ancestor
/// exclusion.
///
/// Returns `Ok(true)` when the redefine contains exactly one well-formed
/// self-reference (§6.1 with §6.1.1 + §6.1.2 satisfied), `Ok(false)` when
/// the redefine contains zero self-references (§6.2 — the caller must
/// schedule a deferred §6.2.2 restriction check), and `Err` for `>1`
/// self-references (§6.1.1 violation) or a single self-ref with
/// `minOccurs`/`maxOccurs != 1` (§6.1.2 violation).
fn validate_self_reference_group(
    schema_set: &SchemaSet,
    group_key: ModelGroupKey,
    expected_name: NameId,
) -> SchemaResult<bool> {
    let group = schema_set
        .arenas
        .model_groups
        .get(group_key)
        .ok_or_else(|| SchemaError::internal("Group not found"))?;

    let mut scan = GroupSelfRefScan {
        count: 0,
        min_occurs: 0,
        max_occurs: None,
    };
    count_group_self_refs(&group.particles, expected_name, &mut scan);

    match scan.count {
        // Clause 6.2 (src-redefine §6.2): the redefining group has no
        // self-reference. Clause 6.2.1 — that the name resolves in S2 — is
        // already enforced by the caller's component lookup against the
        // target document. Clause 6.2.2 — that the new model is a valid
        // restriction of the original — is enforced later by the deferred
        // `validate_all_redefine_group_restrictions` pass once reference
        // resolution is complete. Return `false` so the caller sets the
        // arena flag that schedules that pass.
        0 => Ok(false),
        // Clause 6.1: self-reference present.
        // Clause 6.1.2: minOccurs and maxOccurs must both be 1 on the
        // self-referencing `<group ref>` particle itself (the
        // `ParticleResult` whose `term` is `ParticleTerm::Group(ref)`),
        // not on any enclosing compositor.
        1 => {
            if scan.min_occurs != 1 || scan.max_occurs != Some(1) {
                Err(SchemaError::structural(
                    "src-redefine",
                    format!(
                        "Self-referencing group particle must have \
                         minOccurs=1 and maxOccurs=1, but found \
                         minOccurs={} maxOccurs={}",
                        scan.min_occurs,
                        scan.max_occurs
                            .map(|v| v.to_string())
                            .unwrap_or_else(|| "unbounded".to_string()),
                    ),
                    group
                        .source
                        .as_ref()
                        .and_then(|s| schema_set.source_maps.locate(s)),
                ))
            } else {
                Ok(true)
            }
        }
        _ => Err(SchemaError::structural(
            "src-redefine",
            format!(
                "Redefined group must contain at most one \
                 self-reference (found {})",
                scan.count,
            ),
            group
                .source
                .as_ref()
                .and_then(|s| schema_set.source_maps.locate(s)),
        )),
    }
}

/// Validate self-reference constraints for an attribute group redefine.
///
/// Clause 7.1: with self-reference — exactly one is allowed.
/// Clause 7.2: without self-reference — restriction of original (checked
/// later by `validate_all_redefine_attribute_group_restrictions`).
///
/// Returns `Ok(true)` when the redefine contains exactly one self-reference
/// (§7.1), `Ok(false)` when the redefine contains zero self-references and
/// the caller must schedule a deferred §7.2.2 restriction check. Errors
/// for >1 self-refs.
fn validate_self_reference_attribute_group(
    schema_set: &SchemaSet,
    group_key: AttributeGroupKey,
    expected_name: NameId,
) -> SchemaResult<bool> {
    let group = schema_set
        .arenas
        .attribute_groups
        .get(group_key)
        .ok_or_else(|| SchemaError::internal("Attribute group not found"))?;

    let mut self_refs = 0u32;
    for attr_group_ref in &group.attribute_groups {
        if attr_group_ref.local_name == expected_name {
            self_refs += 1;
        }
    }

    match self_refs {
        // Clause 7.2 (src-redefine §7.2): no self-reference. Clause 7.2.1
        // is enforced by the caller's component lookup; clause 7.2.2 (the
        // restriction check) is deferred to
        // `validate_all_redefine_attribute_group_restrictions` which runs
        // after reference resolution. Return `false` so the caller sets
        // the arena flag that schedules that pass.
        0 => Ok(false),
        // Clause 7.1: exactly one self-reference. Valid.
        1 => Ok(true),
        _ => Err(SchemaError::structural(
            "src-redefine",
            format!(
                "Redefined attribute group must contain at most one \
                 self-reference (found {})",
                self_refs,
            ),
            group
                .source
                .as_ref()
                .and_then(|s| schema_set.source_maps.locate(s)),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arenas::{AttributeGroupData, ModelGroupData};
    use crate::parser::frames::{
        Compositor, ElementFrameResult, ModelGroupDefResult, ParticleResult, ParticleTerm, QNameRef,
    };
    use crate::schema::composition::ComponentKind;
    use crate::schema::model::SchemaDocument;

    /// Helper: set up a schema set with a base document and a named model group.
    fn setup_model_group_redefine() -> (SchemaSet, ModelGroupKey, ModelGroupKey) {
        let mut schema_set = SchemaSet::new();

        // Create base document
        let base_doc_id = schema_set.documents.len() as u32;
        let base_doc = SchemaDocument::new(base_doc_id, "base.xsd".to_string());
        schema_set.documents.push(base_doc);

        let group_name = schema_set.name_table.add("personGroup");

        // Create original group with element "name"
        let name_elem = schema_set.name_table.add("name");
        let original_data = ModelGroupData {
            name: Some(group_name),
            target_namespace: None,
            ref_name: None,
            compositor: Some(Compositor::Sequence),
            particles: vec![ParticleResult {
                term: ParticleTerm::Element(ElementFrameResult {
                    name: Some(name_elem),
                    ref_name: None,
                    target_namespace: None,
                    type_ref: None,
                    inline_type: None,
                    substitution_group: vec![],
                    default_value: None,
                    fixed_value: None,
                    nillable: false,
                    is_abstract: false,
                    min_occurs: 1,
                    max_occurs: Some(1),
                    block: None,
                    final_derivation: None,
                    form: None,
                    id: None,
                    alternatives: vec![],
                    identity_constraints: vec![],
                    identity_constraint_refs: vec![],
                    annotation: None,
                    source: None,
                }),
                min_occurs: 1,
                max_occurs: Some(1),
                source: None,
            }],
            min_occurs: 1,
            max_occurs: Some(1),
            id: None,
            annotation: None,
            source: None,
            resolved_ref: None,
            resolved_particles: Vec::new(),
            resolved_particle_types: Vec::new(),
            resolved_particle_elements: Vec::new(),
            redefine_original: None,
            redefine_requires_restriction_check: false,
        };
        let original_key = schema_set.arenas.alloc_model_group(original_data);
        schema_set
            .get_or_create_namespace(None)
            .register_model_group(group_name, original_key);

        // Create redefining group with self-ref + element "age"
        let age_elem = schema_set.name_table.add("age");
        let new_data = ModelGroupData {
            name: Some(group_name),
            target_namespace: None,
            ref_name: None,
            compositor: Some(Compositor::Sequence),
            particles: vec![
                // Self-reference
                ParticleResult {
                    term: ParticleTerm::Group(ModelGroupDefResult {
                        name: None,
                        ref_name: Some(QNameRef {
                            prefix: None,
                            local_name: group_name,
                            namespace: None,
                        }),
                        compositor: None,
                        particles: vec![],
                        min_occurs: 1,
                        max_occurs: Some(1),
                        id: None,
                        annotation: None,
                        source: None,
                    }),
                    min_occurs: 1,
                    max_occurs: Some(1),
                    source: None,
                },
                // New element
                ParticleResult {
                    term: ParticleTerm::Element(ElementFrameResult {
                        name: Some(age_elem),
                        ref_name: None,
                        target_namespace: None,
                        type_ref: None,
                        inline_type: None,
                        substitution_group: vec![],
                        default_value: None,
                        fixed_value: None,
                        nillable: false,
                        is_abstract: false,
                        min_occurs: 1,
                        max_occurs: Some(1),
                        block: None,
                        final_derivation: None,
                        form: None,
                        id: None,
                        alternatives: vec![],
                        identity_constraints: vec![],
                        identity_constraint_refs: vec![],
                        annotation: None,
                        source: None,
                    }),
                    min_occurs: 1,
                    max_occurs: Some(1),
                    source: None,
                },
            ],
            min_occurs: 1,
            max_occurs: Some(1),
            id: None,
            annotation: None,
            source: None,
            resolved_ref: None,
            resolved_particles: Vec::new(),
            resolved_particle_types: Vec::new(),
            resolved_particle_elements: Vec::new(),
            redefine_original: None,
            redefine_requires_restriction_check: false,
        };
        let new_key = schema_set.arenas.alloc_model_group(new_data);

        (schema_set, original_key, new_key)
    }

    #[test]
    fn test_redefine_model_group_self_reference() {
        let (mut schema_set, original_key, new_key) = setup_model_group_redefine();

        // Apply redefine (no target doc — fallback to global lookup)
        let result = apply_model_group_redefine(&mut schema_set, new_key, None, None);
        assert!(
            result.is_ok(),
            "apply_model_group_redefine failed: {:?}",
            result.err()
        );

        // Verify redefine_original is set
        let group = schema_set.arenas.model_groups.get(new_key).unwrap();
        assert_eq!(
            group.redefine_original,
            Some(original_key),
            "redefine_original should point to the original group"
        );

        // Now resolve references and verify self-ref redirects to original
        let result = crate::schema::resolver::resolve_all_references(&mut schema_set);
        assert!(
            result.is_ok(),
            "resolve_all_references failed: {:?}",
            result.err()
        );

        let group = schema_set.arenas.model_groups.get(new_key).unwrap();
        // First particle is the group ref (self-reference → should resolve to original)
        match &group.resolved_particles[0] {
            crate::arenas::ResolvedParticleTerm::Group {
                resolved_ref: Some(key),
            } => {
                assert_eq!(
                    *key, original_key,
                    "Self-reference should resolve to the original group, not the new one"
                );
            }
            other => panic!("Expected Group particle with resolved_ref, got {:?}", other),
        }
    }

    #[test]
    fn test_redefine_attribute_group_self_reference() {
        let mut schema_set = SchemaSet::new();

        let group_name = schema_set.name_table.add("commonAttrs");

        // Create original attribute group
        let original_data = AttributeGroupData {
            name: Some(group_name),
            target_namespace: None,
            ref_name: None,
            attributes: Vec::new(),
            attribute_groups: Vec::new(),
            attribute_wildcard: None,
            id: None,
            annotation: None,
            source: None,
            resolved_ref: None,
            resolved_attribute_groups: Vec::new(),
            resolved_attributes: Vec::new(),
            redefine_original: None,
            redefine_requires_restriction_check: false,
        };
        let original_key = schema_set.arenas.alloc_attribute_group(original_data);
        schema_set
            .get_or_create_namespace(None)
            .register_attribute_group(group_name, original_key);

        // Create redefining group with self-reference
        let new_data = AttributeGroupData {
            name: Some(group_name),
            target_namespace: None,
            ref_name: None,
            attributes: Vec::new(),
            attribute_groups: vec![QNameRef {
                prefix: None,
                local_name: group_name,
                namespace: None,
            }],
            attribute_wildcard: None,
            id: None,
            annotation: None,
            source: None,
            resolved_ref: None,
            resolved_attribute_groups: Vec::new(),
            resolved_attributes: Vec::new(),
            redefine_original: None,
            redefine_requires_restriction_check: false,
        };
        let new_key = schema_set.arenas.alloc_attribute_group(new_data);

        // Apply redefine
        let result = apply_attribute_group_redefine(&mut schema_set, new_key, None, None);
        assert!(
            result.is_ok(),
            "apply_attribute_group_redefine failed: {:?}",
            result.err()
        );

        // Verify redefine_original
        let group = schema_set.arenas.attribute_groups.get(new_key).unwrap();
        assert_eq!(group.redefine_original, Some(original_key));

        // Resolve references
        let result = crate::schema::resolver::resolve_all_references(&mut schema_set);
        assert!(
            result.is_ok(),
            "resolve_all_references failed: {:?}",
            result.err()
        );

        // Verify self-ref redirected to original
        let group = schema_set.arenas.attribute_groups.get(new_key).unwrap();
        assert_eq!(group.resolved_attribute_groups.len(), 1);
        assert_eq!(
            group.resolved_attribute_groups[0], original_key,
            "Self-reference should resolve to the original attribute group"
        );
    }

    #[test]
    fn test_provenance_note_redefined() {
        let mut schema_set = SchemaSet::new();

        // Create two documents for provenance tracking
        let base_doc_id = schema_set.documents.len() as u32;
        let base_doc = SchemaDocument::new(base_doc_id, "base.xsd".to_string());
        schema_set.documents.push(base_doc);

        let redefining_doc_id = schema_set.documents.len() as u32;
        let redefining_doc = SchemaDocument::new(redefining_doc_id, "main.xsd".to_string());
        schema_set.documents.push(redefining_doc);

        let group_name = schema_set.name_table.add("testGroup");

        // Record provenance as if a redefine occurred
        crate::schema::composition::record_provenance(
            &mut schema_set.effective_components,
            crate::schema::composition::ComponentKey::ModelGroup(
                // dummy key (not used for lookup)
                schema_set.arenas.alloc_model_group(ModelGroupData {
                    name: Some(group_name),
                    target_namespace: None,
                    ref_name: None,
                    compositor: Some(Compositor::Sequence),
                    particles: Vec::new(),
                    min_occurs: 1,
                    max_occurs: Some(1),
                    id: None,
                    annotation: None,
                    source: None,
                    resolved_ref: None,
                    resolved_particles: Vec::new(),
                    resolved_particle_types: Vec::new(),
                    resolved_particle_elements: Vec::new(),
                    redefine_original: None,
                    redefine_requires_restriction_check: false,
                }),
            ),
            ComponentKind::ModelGroup,
            None,
            group_name,
            Some(redefining_doc_id),
            crate::schema::composition::redefined_action(
                Some(redefining_doc_id),
                ComponentKind::ModelGroup,
                group_name,
                None,
                Some(base_doc_id),
            ),
        );

        let note = schema_set.format_provenance_note(ComponentKind::ModelGroup, None, group_name);
        assert!(
            note.contains("base.xsd"),
            "Provenance note should mention the original document: {}",
            note
        );
        assert!(
            note.contains("main.xsd"),
            "Provenance note should mention the redefining document: {}",
            note
        );
        assert!(
            note.contains("redefined"),
            "Provenance note should mention 'redefined': {}",
            note
        );
    }

    #[test]
    fn test_provenance_note_declared() {
        let schema_set = SchemaSet::new();
        let name = schema_set.name_table.get("string").unwrap();

        // No provenance recorded → empty string
        let note = schema_set.format_provenance_note(ComponentKind::SimpleType, None, name);
        assert!(
            note.is_empty(),
            "Provenance note for undeclared component should be empty, got: {}",
            note
        );
    }

    #[test]
    fn test_redefine_model_group_no_self_reference() {
        // Clause 6.2: a redefining group with NO self-reference is valid
        // (pure restriction of the original).
        let (mut schema_set, _original_key, _new_key) = setup_model_group_redefine();

        let group_name = schema_set.name_table.add("personGroup");
        let age_elem = schema_set.name_table.add("age");

        // Create redefining group WITHOUT self-reference (just element "age")
        let new_data = ModelGroupData {
            name: Some(group_name),
            target_namespace: None,
            ref_name: None,
            compositor: Some(Compositor::Sequence),
            particles: vec![ParticleResult {
                term: ParticleTerm::Element(ElementFrameResult {
                    name: Some(age_elem),
                    ref_name: None,
                    target_namespace: None,
                    type_ref: None,
                    inline_type: None,
                    substitution_group: vec![],
                    default_value: None,
                    fixed_value: None,
                    nillable: false,
                    is_abstract: false,
                    min_occurs: 1,
                    max_occurs: Some(1),
                    block: None,
                    final_derivation: None,
                    form: None,
                    id: None,
                    alternatives: vec![],
                    identity_constraints: vec![],
                    identity_constraint_refs: vec![],
                    annotation: None,
                    source: None,
                }),
                min_occurs: 1,
                max_occurs: Some(1),
                source: None,
            }],
            min_occurs: 1,
            max_occurs: Some(1),
            id: None,
            annotation: None,
            source: None,
            resolved_ref: None,
            resolved_particles: Vec::new(),
            resolved_particle_types: Vec::new(),
            resolved_particle_elements: Vec::new(),
            redefine_original: None,
            redefine_requires_restriction_check: false,
        };
        let no_selfref_key = schema_set.arenas.alloc_model_group(new_data);

        let result = apply_model_group_redefine(&mut schema_set, no_selfref_key, None, None);
        assert!(
            result.is_ok(),
            "Group redefine without self-reference should succeed (clause 6.2): {:?}",
            result.err()
        );
    }

    #[test]
    fn test_redefine_model_group_self_ref_wrong_min_occurs() {
        // Clause 6.1.2: self-ref with minOccurs=0 must be rejected.
        let (mut schema_set, _original_key, _new_key) = setup_model_group_redefine();

        let group_name = schema_set.name_table.add("personGroup");
        let age_elem = schema_set.name_table.add("age");

        let new_data = ModelGroupData {
            name: Some(group_name),
            target_namespace: None,
            ref_name: None,
            compositor: Some(Compositor::Sequence),
            particles: vec![
                ParticleResult {
                    term: ParticleTerm::Group(ModelGroupDefResult {
                        name: None,
                        ref_name: Some(QNameRef {
                            prefix: None,
                            local_name: group_name,
                            namespace: None,
                        }),
                        compositor: None,
                        particles: vec![],
                        min_occurs: 1,
                        max_occurs: Some(1),
                        id: None,
                        annotation: None,
                        source: None,
                    }),
                    min_occurs: 0, // ← violates clause 6.1.2
                    max_occurs: Some(1),
                    source: None,
                },
                ParticleResult {
                    term: ParticleTerm::Element(ElementFrameResult {
                        name: Some(age_elem),
                        ref_name: None,
                        target_namespace: None,
                        type_ref: None,
                        inline_type: None,
                        substitution_group: vec![],
                        default_value: None,
                        fixed_value: None,
                        nillable: false,
                        is_abstract: false,
                        min_occurs: 1,
                        max_occurs: Some(1),
                        block: None,
                        final_derivation: None,
                        form: None,
                        id: None,
                        alternatives: vec![],
                        identity_constraints: vec![],
                        identity_constraint_refs: vec![],
                        annotation: None,
                        source: None,
                    }),
                    min_occurs: 1,
                    max_occurs: Some(1),
                    source: None,
                },
            ],
            min_occurs: 1,
            max_occurs: Some(1),
            id: None,
            annotation: None,
            source: None,
            resolved_ref: None,
            resolved_particles: Vec::new(),
            resolved_particle_types: Vec::new(),
            resolved_particle_elements: Vec::new(),
            redefine_original: None,
            redefine_requires_restriction_check: false,
        };
        let bad_key = schema_set.arenas.alloc_model_group(new_data);

        let result = apply_model_group_redefine(&mut schema_set, bad_key, None, None);
        assert!(
            result.is_err(),
            "Self-ref with minOccurs=0 should fail (clause 6.1.2)"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("minOccurs"),
            "Error should mention minOccurs: {}",
            msg
        );
    }

    #[test]
    fn test_redefine_model_group_self_ref_wrong_max_occurs() {
        // Clause 6.1.2: self-ref with maxOccurs=unbounded must be rejected.
        let (mut schema_set, _original_key, _new_key) = setup_model_group_redefine();

        let group_name = schema_set.name_table.add("personGroup");

        let new_data = ModelGroupData {
            name: Some(group_name),
            target_namespace: None,
            ref_name: None,
            compositor: Some(Compositor::Sequence),
            particles: vec![ParticleResult {
                term: ParticleTerm::Group(ModelGroupDefResult {
                    name: None,
                    ref_name: Some(QNameRef {
                        prefix: None,
                        local_name: group_name,
                        namespace: None,
                    }),
                    compositor: None,
                    particles: vec![],
                    min_occurs: 1,
                    max_occurs: Some(1),
                    id: None,
                    annotation: None,
                    source: None,
                }),
                min_occurs: 1,
                max_occurs: None, // ← unbounded, violates clause 6.1.2
                source: None,
            }],
            min_occurs: 1,
            max_occurs: Some(1),
            id: None,
            annotation: None,
            source: None,
            resolved_ref: None,
            resolved_particles: Vec::new(),
            resolved_particle_types: Vec::new(),
            resolved_particle_elements: Vec::new(),
            redefine_original: None,
            redefine_requires_restriction_check: false,
        };
        let bad_key = schema_set.arenas.alloc_model_group(new_data);

        let result = apply_model_group_redefine(&mut schema_set, bad_key, None, None);
        assert!(
            result.is_err(),
            "Self-ref with maxOccurs=unbounded should fail (clause 6.1.2)"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("maxOccurs"),
            "Error should mention maxOccurs: {}",
            msg
        );
    }

    // -----------------------------------------------------------------
    // §src-redefine 6.1 deep self-reference scanning
    // (structures.html:13729-13741, "among its contents at some level")
    // -----------------------------------------------------------------
    //
    // The following four tests exercise the recursive walker
    // `count_group_self_refs`. They cover, in order:
    //   1. a valid self-reference nested one level deep inside an
    //      inline `<sequence>` (§6.1.1 positive case);
    //   2. a nested self-reference whose `minOccurs` violates §6.1.2;
    //   3. two self-references at different nesting levels (§6.1.1
    //      "exactly one such group" violation);
    //   4. a direct walker test proving `ParticleTerm::Element` is
    //      terminal (§6.1 element-ancestor exclusion).

    /// Helper: build a bare `ModelGroupDefResult` with min/maxOccurs=1
    /// and all optional fields defaulted. Shared by the group-ref and
    /// inline-compositor helpers below.
    fn model_group_def(
        ref_name: Option<QNameRef>,
        compositor: Option<Compositor>,
        particles: Vec<ParticleResult>,
    ) -> ModelGroupDefResult {
        ModelGroupDefResult {
            name: None,
            ref_name,
            compositor,
            particles,
            min_occurs: 1,
            max_occurs: Some(1),
            id: None,
            annotation: None,
            source: None,
        }
    }

    /// Helper: build a `ParticleTerm::Group` self-reference wrapped in
    /// an outer `ParticleResult` with the given occurrence bounds.
    fn self_ref_particle(
        group_name: NameId,
        min_occurs: u32,
        max_occurs: Option<u32>,
    ) -> ParticleResult {
        ParticleResult {
            term: ParticleTerm::Group(model_group_def(
                Some(QNameRef {
                    prefix: None,
                    local_name: group_name,
                    namespace: None,
                }),
                None,
                vec![],
            )),
            min_occurs,
            max_occurs,
            source: None,
        }
    }

    /// Helper: build a simple `ParticleTerm::Element` with min/maxOccurs=1
    /// and no inline type. Used to pad a redefine body.
    fn simple_element_particle(name: NameId) -> ParticleResult {
        ParticleResult {
            term: ParticleTerm::Element(ElementFrameResult {
                name: Some(name),
                ref_name: None,
                target_namespace: None,
                type_ref: None,
                inline_type: None,
                substitution_group: vec![],
                default_value: None,
                fixed_value: None,
                nillable: false,
                is_abstract: false,
                min_occurs: 1,
                max_occurs: Some(1),
                block: None,
                final_derivation: None,
                form: None,
                id: None,
                alternatives: vec![],
                identity_constraints: vec![],
                identity_constraint_refs: vec![],
                annotation: None,
                source: None,
            }),
            min_occurs: 1,
            max_occurs: Some(1),
            source: None,
        }
    }

    /// Helper: wrap a list of particles inside an inline compositor
    /// (`ParticleTerm::Group` with `ref_name: None, name: None`).
    fn inline_compositor_particle(
        compositor: Compositor,
        inner: Vec<ParticleResult>,
    ) -> ParticleResult {
        ParticleResult {
            term: ParticleTerm::Group(model_group_def(None, Some(compositor), inner)),
            min_occurs: 1,
            max_occurs: Some(1),
            source: None,
        }
    }

    /// Helper: allocate a new redefining model group whose top-level
    /// particle list is `top_particles`, matching the name of the
    /// original group created by `setup_model_group_redefine`.
    fn alloc_redefining_group(
        schema_set: &mut SchemaSet,
        top_particles: Vec<ParticleResult>,
    ) -> ModelGroupKey {
        let group_name = schema_set.name_table.add("personGroup");
        let data = ModelGroupData {
            name: Some(group_name),
            target_namespace: None,
            ref_name: None,
            compositor: Some(Compositor::Sequence),
            particles: top_particles,
            min_occurs: 1,
            max_occurs: Some(1),
            id: None,
            annotation: None,
            source: None,
            resolved_ref: None,
            resolved_particles: Vec::new(),
            resolved_particle_types: Vec::new(),
            resolved_particle_elements: Vec::new(),
            redefine_original: None,
            redefine_requires_restriction_check: false,
        };
        schema_set.arenas.alloc_model_group(data)
    }

    #[test]
    fn test_redefine_model_group_nested_self_ref_in_sequence() {
        // §src-redefine 6.1 "among its contents at some level":
        // a self-reference nested one level deep inside an inline
        // `<sequence>` must still be recognised as a §6.1 self-reference
        // (satisfying §6.1.1 "exactly one such group" and §6.1.2
        // "minOccurs=1 and maxOccurs=1").
        let (mut schema_set, _original_key, _ignored) = setup_model_group_redefine();

        let group_name = schema_set.name_table.add("personGroup");
        let age_elem = schema_set.name_table.add("age");

        let new_key = alloc_redefining_group(
            &mut schema_set,
            vec![inline_compositor_particle(
                Compositor::Sequence,
                vec![
                    self_ref_particle(group_name, 1, Some(1)),
                    simple_element_particle(age_elem),
                ],
            )],
        );

        let result = apply_model_group_redefine(&mut schema_set, new_key, None, None);
        assert!(
            result.is_ok(),
            "nested self-ref inside <sequence> should be accepted \
             (§src-redefine 6.1 'at some level'): {:?}",
            result.err()
        );

        // The walker found one self-ref → §6.1 path → no deferred §6.2.2
        // restriction check should be scheduled.
        let group = schema_set.arenas.model_groups.get(new_key).unwrap();
        assert!(
            !group.redefine_requires_restriction_check,
            "§6.1 self-ref path must clear the §6.2.2 restriction flag"
        );
    }

    #[test]
    fn test_redefine_model_group_nested_self_ref_wrong_min_occurs() {
        // §src-redefine 6.1.2 applies to the self-referencing `<group ref>`
        // particle's own `minOccurs`/`maxOccurs`, regardless of how deeply
        // nested it is. A nested self-ref with `minOccurs=2` must still be
        // rejected.
        let (mut schema_set, _original_key, _ignored) = setup_model_group_redefine();

        let group_name = schema_set.name_table.add("personGroup");

        let new_key = alloc_redefining_group(
            &mut schema_set,
            vec![inline_compositor_particle(
                Compositor::Sequence,
                vec![self_ref_particle(group_name, 2, Some(2))],
            )],
        );

        let result = apply_model_group_redefine(&mut schema_set, new_key, None, None);
        assert!(
            result.is_err(),
            "nested self-ref with minOccurs=2 should fail (§src-redefine 6.1.2)"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("minOccurs"),
            "error should mention minOccurs: {}",
            msg
        );
    }

    #[test]
    fn test_redefine_model_group_multiple_self_refs_at_different_depths() {
        // §src-redefine 6.1.1 "it has exactly one such group". Two
        // self-references — one at the top level and one nested inside a
        // `<choice>` — must be rejected. The old shallow scan would have
        // missed the nested one and incorrectly accepted this fixture.
        let (mut schema_set, _original_key, _ignored) = setup_model_group_redefine();

        let group_name = schema_set.name_table.add("personGroup");

        let new_key = alloc_redefining_group(
            &mut schema_set,
            vec![
                // Self-ref at the top level — valid min/max.
                self_ref_particle(group_name, 1, Some(1)),
                // A second self-ref nested inside an inline <choice>.
                inline_compositor_particle(
                    Compositor::Choice,
                    vec![self_ref_particle(group_name, 1, Some(1))],
                ),
            ],
        );

        let result = apply_model_group_redefine(&mut schema_set, new_key, None, None);
        assert!(
            result.is_err(),
            "two self-refs at different depths should fail (§src-redefine 6.1.1)"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("at most one") || msg.contains("found 2"),
            "error should mention the count violation: {}",
            msg
        );
    }

    #[test]
    fn test_count_group_self_refs_element_is_terminal() {
        // §src-redefine 6.1 "and that `<group>` does not have an
        // `<element>` ancestor": the walker must treat `ParticleTerm::Element`
        // as terminal and must not descend into its inline type. Implemented
        // simply by never matching `ParticleTerm::Element` in the walker.
        //
        // We verify this sharply by calling the walker directly: a particle
        // list consisting only of an element particle must contribute zero
        // self-references even when queried with a live `expected_name`.
        let schema_set = SchemaSet::new();
        let group_name = schema_set.name_table.add("personGroup");
        let age_elem = schema_set.name_table.add("age");

        let particles = vec![simple_element_particle(age_elem)];

        let mut scan = GroupSelfRefScan {
            count: 0,
            min_occurs: 0,
            max_occurs: None,
        };
        count_group_self_refs(&particles, group_name, &mut scan);
        assert_eq!(
            scan.count, 0,
            "walker must treat ParticleTerm::Element as terminal per \
             §src-redefine 6.1 element-ancestor exclusion"
        );

        // Sanity: the same walker DOES count a top-level self-ref when one
        // is actually present, so the zero-count above is not a vacuous
        // "walker never finds anything" result.
        let particles_with_ref = vec![self_ref_particle(group_name, 1, Some(1))];
        let mut scan = GroupSelfRefScan {
            count: 0,
            min_occurs: 0,
            max_occurs: None,
        };
        count_group_self_refs(&particles_with_ref, group_name, &mut scan);
        assert_eq!(
            scan.count, 1,
            "sanity check: walker detects a top-level self-ref"
        );
    }

    #[test]
    fn test_redefine_attribute_group_no_self_reference() {
        // Clause 7.2: attribute group redefine with no self-reference is valid.
        let mut schema_set = SchemaSet::new();

        let group_name = schema_set.name_table.add("commonAttrs");

        let original_data = AttributeGroupData {
            name: Some(group_name),
            target_namespace: None,
            ref_name: None,
            attributes: Vec::new(),
            attribute_groups: Vec::new(),
            attribute_wildcard: None,
            id: None,
            annotation: None,
            source: None,
            resolved_ref: None,
            resolved_attribute_groups: Vec::new(),
            resolved_attributes: Vec::new(),
            redefine_original: None,
            redefine_requires_restriction_check: false,
        };
        let _original_key = schema_set.arenas.alloc_attribute_group(original_data);
        schema_set
            .get_or_create_namespace(None)
            .register_attribute_group(group_name, _original_key);

        // Redefining group with NO self-reference
        let new_data = AttributeGroupData {
            name: Some(group_name),
            target_namespace: None,
            ref_name: None,
            attributes: Vec::new(),
            attribute_groups: Vec::new(), // empty — no self-ref
            attribute_wildcard: None,
            id: None,
            annotation: None,
            source: None,
            resolved_ref: None,
            resolved_attribute_groups: Vec::new(),
            resolved_attributes: Vec::new(),
            redefine_original: None,
            redefine_requires_restriction_check: false,
        };
        let new_key = schema_set.arenas.alloc_attribute_group(new_data);

        let result = apply_attribute_group_redefine(&mut schema_set, new_key, None, None);
        assert!(
            result.is_ok(),
            "Attribute group redefine without self-reference should succeed (clause 7.2): {:?}",
            result.err()
        );
    }
}
