//! XSD 1.1 type-table / type-alternative consistency checks and the
//! `{disallowed names}` wildcard rules (§3.10.6.1, §3.4.6.3
//! `cos-element-consistent`).

#[cfg(feature = "xsd11")]
use super::format_type_name;
#[cfg(feature = "xsd11")]
use super::wildcard::wildcard_namespace_matches;
#[cfg(feature = "xsd11")]
use crate::error::{SchemaError, SchemaResult};
#[cfg(feature = "xsd11")]
use crate::ids::{ElementKey, NameId, TypeKey};
#[cfg(feature = "xsd11")]
use crate::parser::frames::{
    ComplexContentResult, ParticleResult, ParticleTerm, ProcessContents, WildcardResult,
};
#[cfg(feature = "xsd11")]
use crate::parser::location::SourceRef;
#[cfg(feature = "xsd11")]
use crate::schema::SchemaSet;

/// XSD 1.1 §3.3.2 Schema Representation Constraint: Type Alternative
/// Representation OK (`src-type-alternative`).
///
/// Among an element's sequence of `<xs:alternative>` children, only the last
/// alternative is allowed to omit the `test` attribute (acting as a default
/// fallback). An alternative without `@test` in a non-final position is a
/// schema error.
#[cfg(feature = "xsd11")]
pub fn validate_element_type_alternatives(schema_set: &SchemaSet) -> SchemaResult<()> {
    if !schema_set.is_xsd11() {
        return Ok(());
    }
    for (_key, elem) in schema_set.arenas.elements.iter() {
        let alts = &elem.alternatives;
        if alts.len() < 2 {
            continue;
        }
        for alt in &alts[..alts.len() - 1] {
            if alt.test.is_none() {
                let name = elem
                    .name
                    .map(|n| schema_set.name_table.resolve_ref(n))
                    .unwrap_or("(anonymous)");
                let location = schema_set.locate(elem.source.as_ref());
                return Err(SchemaError::structural(
                    "src-type-alternative",
                    format!(
                        "Element '{}': <xs:alternative> without a 'test' attribute is only \
                         permitted as the last alternative",
                        name
                    ),
                    location,
                ));
            }
        }
    }
    Ok(())
}

/// XSD 1.1 §3.10.6.1 rule 4 (Wildcard Properties Correct): for every QName
/// member in a wildcard's `{disallowed names}`, that QName's namespace name
/// must be admitted by the wildcard's `{namespace constraint}` (the
/// combination of `namespace` and `notNamespace`).
///
/// In other words: the schema cannot list a notQName entry whose namespace
/// the wildcard already excludes — such an entry would be redundant and
/// the spec rules it out as a structural error. Covers W3C saxonData
/// `wild031`..`wild035` (and is the post-parse step that backs the
/// per-entry checks `parse_not_qname` performs at parse time).
///
/// The check needs the resolved target namespace of the wildcard's owner
/// to interpret `##targetNamespace`/`##other`/`##local`, which is only
/// available after assembly — hence this is a separate pipeline pass
/// rather than something `parse_not_qname` can do on its own.
#[cfg(feature = "xsd11")]
pub fn validate_wildcard_disallowed_names(schema_set: &SchemaSet) -> SchemaResult<()> {
    if !schema_set.is_xsd11() {
        return Ok(());
    }

    fn check_wildcard(
        schema_set: &SchemaSet,
        wc: &WildcardResult,
        target_ns: Option<NameId>,
    ) -> SchemaResult<()> {
        use crate::parser::frames::NotQNameItem;

        for item in &wc.not_qname {
            let NotQNameItem::QName {
                namespace: q_ns,
                local_name,
            } = item
            else {
                continue;
            };
            // The QName must satisfy the wildcard's namespace constraint
            // (cvc-wildcard-namespace §3.10.4.3): it must be admitted by
            // the positive constraint AND not be excluded by notNamespace.
            let admitted_by_constraint =
                wildcard_namespace_matches(&wc.namespace, *q_ns, target_ns);
            let excluded_by_not_namespace = wc
                .not_namespace
                .iter()
                .any(|t| t.resolve(target_ns) == *q_ns);
            if !admitted_by_constraint || excluded_by_not_namespace {
                let location = schema_set.locate(wc.source.as_ref());
                let qname_text = match q_ns {
                    Some(ns) => format!(
                        "{{{}}}:{}",
                        schema_set.name_table.resolve_ref(*ns),
                        schema_set.name_table.resolve_ref(*local_name),
                    ),
                    None => schema_set.name_table.resolve_ref(*local_name).to_string(),
                };
                return Err(SchemaError::structural(
                    "w-props-correct",
                    format!(
                        "notQName entry '{}' is not admitted by the wildcard's \
                         namespace constraint (§3.10.6.1 rule 4)",
                        qname_text
                    ),
                    location,
                ));
            }
        }
        Ok(())
    }

    fn check_particle(
        schema_set: &SchemaSet,
        particle: &ParticleResult,
        target_ns: Option<NameId>,
        depth: usize,
    ) -> SchemaResult<()> {
        if depth > schema_set.recursion_bound(100) {
            return Ok(());
        }
        match &particle.term {
            ParticleTerm::Any(wc) => check_wildcard(schema_set, wc, target_ns)?,
            ParticleTerm::Group(group) => {
                for child in &group.particles {
                    check_particle(schema_set, child, target_ns, depth + 1)?;
                }
            }
            ParticleTerm::Element(_) => {}
        }
        Ok(())
    }

    // Complex types: own attribute_wildcard + content particles + any
    // attribute_wildcard hiding inside the SimpleContent / ComplexContent
    // derivation defs.
    for (_key, ct) in schema_set.arenas.complex_types.iter() {
        let target_ns = ct.target_namespace;
        if let Some(wc) = ct.attribute_wildcard.as_ref() {
            check_wildcard(schema_set, wc, target_ns)?;
        }
        match &ct.content {
            ComplexContentResult::Empty => {}
            ComplexContentResult::Simple(sc) => {
                if let Some(wc) = sc.attribute_wildcard.as_ref() {
                    check_wildcard(schema_set, wc, target_ns)?;
                }
            }
            ComplexContentResult::Complex(cc) => {
                if let Some(wc) = cc.attribute_wildcard.as_ref() {
                    check_wildcard(schema_set, wc, target_ns)?;
                }
                if let Some(p) = cc.particle.as_ref() {
                    check_particle(schema_set, p, target_ns, 0)?;
                }
                if let Some(oc) = cc.open_content.as_ref() {
                    if let Some(wc) = oc.wildcard.as_ref() {
                        check_wildcard(schema_set, wc, target_ns)?;
                    }
                }
            }
        }
        if let Some(oc) = ct.open_content.as_ref() {
            if let Some(wc) = oc.wildcard.as_ref() {
                check_wildcard(schema_set, wc, target_ns)?;
            }
        }
    }

    // Attribute groups: own attribute_wildcard.
    for (_key, ag) in schema_set.arenas.attribute_groups.iter() {
        if let Some(wc) = ag.attribute_wildcard.as_ref() {
            check_wildcard(schema_set, wc, ag.target_namespace)?;
        }
    }

    // Model-group definitions: walk content particles for element wildcards.
    for (_key, mg) in schema_set.arenas.model_groups.iter() {
        for child in &mg.particles {
            check_particle(schema_set, child, mg.target_namespace, 0)?;
        }
    }

    Ok(())
}

/// XSD 1.1 §3.8.6.3 / cos-element-consistent (second clause): when a complex
/// type's content model contains both a local element declaration with
/// expanded name Q AND a strict/lax wildcard that admits Q, AND a top-level
/// element declaration G with expanded name Q exists, then the type tables
/// of the local element and G must be either both absent or both present
/// and equivalent.
///
/// Closes wild078/079 (local has no type table, global has one) and wild081
/// (local has a type table, global doesn't).
#[cfg(feature = "xsd11")]
pub fn validate_wildcard_element_type_table_consistency(
    schema_set: &SchemaSet,
) -> SchemaResult<()> {
    use crate::parser::frames::AlternativeResult;

    if !schema_set.is_xsd11() {
        return Ok(());
    }

    /// Walk a particle tree using the parallel `local_keys`/`flat_idx` scheme
    /// from `allocate_content_particle_elements`. For each local element
    /// particle, look up its allocated arena key (where post-resolution
    /// alternatives live) instead of relying on the stale parser-frame copy.
    #[allow(clippy::too_many_arguments)]
    fn walk_collect<'a>(
        particle: &'a ParticleResult,
        target_ns: Option<NameId>,
        schema_set: &'a SchemaSet,
        local_keys: &[Option<ElementKey>],
        flat_idx: &mut usize,
        local_elems: &mut Vec<(Option<NameId>, NameId, ElementKey, Option<SourceRef>)>,
        wildcards: &mut Vec<&'a WildcardResult>,
        depth: usize,
    ) {
        if depth > schema_set.recursion_bound(100) {
            return;
        }
        match &particle.term {
            ParticleTerm::Element(elem) => {
                if let Some(ref_qn) = &elem.ref_name {
                    // ref slot: increment flat_idx but no local key.
                    *flat_idx += 1;
                    let _ = ref_qn;
                } else if let Some(name) = elem.name {
                    let ns = elem.target_namespace.or(target_ns);
                    let idx = *flat_idx;
                    *flat_idx += 1;
                    if let Some(key) = local_keys.get(idx).copied().flatten() {
                        local_elems.push((ns, name, key, elem.source.clone()));
                    }
                }
            }
            ParticleTerm::Any(wc) => {
                wildcards.push(wc);
            }
            ParticleTerm::Group(group) => {
                if let Some(ref_qn) = &group.ref_name {
                    if let Some(group_key) =
                        schema_set.lookup_model_group(ref_qn.namespace, ref_qn.local_name)
                    {
                        let mg = &schema_set.arenas.model_groups[group_key];
                        let mg_ns = mg.target_namespace.or(target_ns);
                        let mut group_flat_idx = 0usize;
                        for child in &mg.particles {
                            walk_collect(
                                child,
                                mg_ns,
                                schema_set,
                                &mg.resolved_particle_elements,
                                &mut group_flat_idx,
                                local_elems,
                                wildcards,
                                depth + 1,
                            );
                        }
                    }
                    // Group refs do not advance the outer flat_idx (mirrors
                    // collect_content_particle_elements_recursive).
                } else {
                    for child in &group.particles {
                        walk_collect(
                            child,
                            target_ns,
                            schema_set,
                            local_keys,
                            flat_idx,
                            local_elems,
                            wildcards,
                            depth + 1,
                        );
                    }
                }
            }
        }
    }

    /// Resolve an alternative's effective type — fall back to looking up the
    /// QName via `schema_set.lookup_type` when the parser-frame copy hasn't
    /// been resolved yet (which is the case for local element alternatives
    /// allocated post-`resolve_all_references`).
    fn alt_effective_type(alt: &AlternativeResult, schema_set: &SchemaSet) -> Option<TypeKey> {
        use crate::parser::frames::TypeRefResult;
        if let Some(t) = alt.resolved_type {
            return Some(t);
        }
        if let Some(TypeRefResult::QName(qname)) = &alt.type_ref {
            return schema_set
                .lookup_type(qname.namespace, qname.local_name)
                .or_else(|| {
                    schema_set.get_built_in_type_by_qname(qname.namespace, qname.local_name)
                });
        }
        None
    }

    fn alternatives_equivalent(
        a: &[AlternativeResult],
        b: &[AlternativeResult],
        schema_set: &SchemaSet,
    ) -> bool {
        if a.len() != b.len() {
            return false;
        }
        for (x, y) in a.iter().zip(b.iter()) {
            if x.test != y.test {
                return false;
            }
            if alt_effective_type(x, schema_set) != alt_effective_type(y, schema_set) {
                return false;
            }
        }
        true
    }

    for (_key, ct) in schema_set.arenas.complex_types.iter() {
        let target_ns = ct.target_namespace;
        let ComplexContentResult::Complex(cc) = &ct.content else {
            continue;
        };
        let Some(particle) = cc.particle.as_ref() else {
            continue;
        };

        let mut local_elems: Vec<(Option<NameId>, NameId, ElementKey, Option<SourceRef>)> =
            Vec::new();
        let mut wildcards: Vec<&WildcardResult> = Vec::new();
        let mut flat_idx = 0usize;
        walk_collect(
            particle,
            target_ns,
            schema_set,
            &ct.resolved_content_particle_elements,
            &mut flat_idx,
            &mut local_elems,
            &mut wildcards,
            0,
        );

        // Open-content wildcards also count.
        if let Some(oc) = cc.open_content.as_ref() {
            if let Some(wc) = oc.wildcard.as_ref() {
                wildcards.push(wc);
            }
        }

        if wildcards.is_empty() {
            continue;
        }

        for (l_ns, l_name, l_key, l_source) in &local_elems {
            let global_key = schema_set.lookup_element(*l_ns, *l_name);
            let Some(g_key) = global_key else {
                continue;
            };
            // If the local declaration is the global itself (e.g., a ref'd
            // element resolving back to the same arena key), skip — there's
            // only one declaration so no inconsistency is possible.
            if *l_key == g_key {
                continue;
            }
            let l_decl = &schema_set.arenas.elements[*l_key];
            let g_decl = &schema_set.arenas.elements[g_key];

            // The wildcard must be lax/strict (skip wildcards bypass the EDC
            // per wild080) AND admit (l_ns, l_name).
            let admitted = wildcards.iter().any(|wc| {
                if matches!(wc.process_contents, ProcessContents::Skip) {
                    return false;
                }
                // A wildcard reached through a model group of another
                // namespace keeps the meaning of `##other` /
                // `##targetNamespace` of its own schema document (§3.10.2.2).
                let wc_ns = schema_set.wildcard_target_namespace(wc.source.as_ref(), target_ns);
                wildcard_result_admits_qname(wc, wc_ns, *l_ns, *l_name)
            });
            if !admitted {
                continue;
            }

            if !alternatives_equivalent(&l_decl.alternatives, &g_decl.alternatives, schema_set) {
                let location = schema_set
                    .locate(l_source.as_ref())
                    .or_else(|| schema_set.locate(ct.source.as_ref()));
                let qname = match l_ns {
                    Some(ns) => format!(
                        "{{{}}}:{}",
                        schema_set.name_table.resolve_ref(*ns),
                        schema_set.name_table.resolve_ref(*l_name),
                    ),
                    None => schema_set.name_table.resolve_ref(*l_name).to_string(),
                };
                return Err(SchemaError::structural(
                    "cos-element-consistent",
                    format!(
                        "Local element '{}' is in the same content model as a strict/lax \
                         wildcard that admits its expanded name; the local element's type \
                         table is not equivalent to that of the top-level declaration of \
                         the same name (§3.8.6.3 / cos-element-consistent)",
                        qname
                    ),
                    location,
                ));
            }
        }
    }

    Ok(())
}

// Shared helpers for §3.8.6.3 / §3.4.6.3 cos-element-consistent.

/// Per-local-element record produced by [`collect_local_elements`].
#[cfg(feature = "xsd11")]
type LocalElementEntry = (Option<NameId>, NameId, ElementKey, Option<SourceRef>);

/// Recursively walk a complex type's content model and emit one
/// `LocalElementEntry` per inline local element declaration.
///
/// `flat_idx` tracks the walker's position in the owning CT's
/// `resolved_content_particle_elements`, which is the post-resolution
/// arena lookup that carries CTA alternatives (the parser-frame
/// `AlternativeResult` slice on the `ElementParticle` is stale for
/// inline alternatives resolved later).
#[cfg(feature = "xsd11")]
fn walk_collect_local_elements(
    particle: &ParticleResult,
    target_ns: Option<NameId>,
    local_keys: &[Option<ElementKey>],
    flat_idx: &mut usize,
    out: &mut Vec<LocalElementEntry>,
) {
    if let ParticleTerm::Group(group) = &particle.term {
        walk_group_local_elements(&group.particles, target_ns, local_keys, flat_idx, out);
    }
}

#[cfg(feature = "xsd11")]
fn walk_group_local_elements(
    particles: &[ParticleResult],
    target_ns: Option<NameId>,
    local_keys: &[Option<ElementKey>],
    flat_idx: &mut usize,
    out: &mut Vec<LocalElementEntry>,
) {
    for p in particles {
        match &p.term {
            ParticleTerm::Element(elem) if elem.ref_name.is_none() => {
                if let Some(Some(elem_key)) = local_keys.get(*flat_idx) {
                    let ns = elem.target_namespace.or(target_ns);
                    if let Some(name) = elem.name {
                        out.push((ns, name, *elem_key, elem.source.clone()));
                    }
                }
                *flat_idx += 1;
            }
            ParticleTerm::Element(_) => {
                *flat_idx += 1;
            }
            ParticleTerm::Group(group) if group.ref_name.is_none() => {
                walk_group_local_elements(&group.particles, target_ns, local_keys, flat_idx, out);
            }
            _ => {}
        }
    }
}

/// Collect every local element declaration in `ct`'s content model.
#[cfg(feature = "xsd11")]
fn collect_local_elements(ct: &crate::arenas::ComplexTypeDefData) -> Vec<LocalElementEntry> {
    let mut out = Vec::new();
    let ComplexContentResult::Complex(cc) = &ct.content else {
        return out;
    };
    let Some(particle) = cc.particle.as_ref() else {
        return out;
    };
    let mut flat_idx = 0usize;
    walk_collect_local_elements(
        particle,
        ct.target_namespace,
        &ct.resolved_content_particle_elements,
        &mut flat_idx,
        &mut out,
    );
    out
}

/// Resolve an alternative's effective `TypeKey`, falling back to the
/// schema-set's name lookup or built-in registry when the parser-frame
/// `resolved_type` is absent.
#[cfg(feature = "xsd11")]
fn alt_effective_type(
    alt: &crate::parser::frames::AlternativeResult,
    schema_set: &SchemaSet,
) -> Option<TypeKey> {
    use crate::parser::frames::TypeRefResult;
    if let Some(t) = alt.resolved_type {
        return Some(t);
    }
    if let Some(TypeRefResult::QName(qname)) = &alt.type_ref {
        return schema_set
            .lookup_type(qname.namespace, qname.local_name)
            .or_else(|| schema_set.get_built_in_type_by_qname(qname.namespace, qname.local_name));
    }
    None
}

/// Two alternative lists are equivalent when they have the same length,
/// pairwise-equal `@test` strings, and pairwise-equal effective types.
#[cfg(feature = "xsd11")]
fn alternatives_equivalent(
    a: &[crate::parser::frames::AlternativeResult],
    b: &[crate::parser::frames::AlternativeResult],
    schema_set: &SchemaSet,
) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b.iter()).all(|(x, y)| {
        x.test == y.test && alt_effective_type(x, schema_set) == alt_effective_type(y, schema_set)
    })
}

/// XSD 1.1 §3.8.6.3 / cos-element-consistent: two local element declarations
/// with the same expanded QName in the same complex-type content model must
/// have equivalent type tables (or both have no type table).
#[cfg(feature = "xsd11")]
pub fn validate_local_element_type_table_consistency(schema_set: &SchemaSet) -> SchemaResult<()> {
    use std::collections::HashMap;

    if !schema_set.is_xsd11() {
        return Ok(());
    }

    for (_key, ct) in schema_set.arenas.complex_types.iter() {
        let local_elems = collect_local_elements(ct);
        if local_elems.len() < 2 {
            continue;
        }

        let mut by_name: HashMap<(Option<NameId>, NameId), Vec<usize>> = HashMap::new();
        for (i, (ns, name, _, _)) in local_elems.iter().enumerate() {
            by_name.entry((*ns, *name)).or_default().push(i);
        }

        for (qname, indices) in &by_name {
            if indices.len() < 2 {
                continue;
            }
            let first_idx = indices[0];
            let first_decl = &schema_set.arenas.elements[local_elems[first_idx].2];
            for &idx in &indices[1..] {
                let other_decl = &schema_set.arenas.elements[local_elems[idx].2];
                if alternatives_equivalent(
                    &first_decl.alternatives,
                    &other_decl.alternatives,
                    schema_set,
                ) {
                    continue;
                }
                let qn_str = match qname.0 {
                    Some(ns) => format!(
                        "{{{}}}{}",
                        schema_set.name_table.resolve_ref(ns),
                        schema_set.name_table.resolve_ref(qname.1),
                    ),
                    None => schema_set.name_table.resolve_ref(qname.1).to_string(),
                };
                let location = schema_set
                    .locate(local_elems[idx].3.as_ref())
                    .or_else(|| schema_set.locate(ct.source.as_ref()));
                return Err(SchemaError::structural(
                    "cos-element-consistent",
                    format!(
                        "Two local element declarations of '{}' appear in the same \
                         content model but their type tables are not equivalent \
                         (§3.8.6.3 / cos-element-consistent)",
                        qn_str
                    ),
                    location,
                ));
            }
        }
    }

    Ok(())
}

/// XSD 1.1 §3.4.6.3 / cos-element-consistent (cross-derivation): when a
/// complex type T restricts a base type B and both contain local element
/// declarations with the same expanded QName, the type tables of those
/// declarations must be equivalent (or both absent).
///
/// Complements `validate_local_element_type_table_consistency`, which
/// catches duplicates *within* one content model.
#[cfg(feature = "xsd11")]
pub fn validate_restriction_local_element_type_table_consistency(
    schema_set: &SchemaSet,
) -> SchemaResult<()> {
    use std::collections::HashMap;

    if !schema_set.is_xsd11() {
        return Ok(());
    }

    for (_key, ct) in schema_set.arenas.complex_types.iter() {
        // §3.4.6.2 (extension) only adds particles and never re-issues
        // them, so type-table consistency is automatic there. Restrict to
        // §3.4.6.3 (restriction).
        if ct.derivation_method != Some(crate::parser::frames::DerivationMethod::Restriction) {
            continue;
        }
        let Some(TypeKey::Complex(base_ck)) = ct.resolved_base_type else {
            continue;
        };
        let Some(base_ct) = schema_set.arenas.complex_types.get(base_ck) else {
            continue;
        };

        let derived_locals = collect_local_elements(ct);
        if derived_locals.is_empty() {
            continue;
        }
        let base_locals = collect_local_elements(base_ct);
        if base_locals.is_empty() {
            continue;
        }

        // Multiple base locals with the same name is itself invalid; the
        // in-CT consistency pass will reject the base independently.
        let mut base_by_name: HashMap<(Option<NameId>, NameId), ElementKey> = HashMap::new();
        for (ns, name, ek, _) in &base_locals {
            base_by_name.entry((*ns, *name)).or_insert(*ek);
        }

        for (ns, name, derived_ek, derived_src) in &derived_locals {
            let Some(&base_ek) = base_by_name.get(&(*ns, *name)) else {
                continue;
            };
            let derived_decl = &schema_set.arenas.elements[*derived_ek];
            let base_decl = &schema_set.arenas.elements[base_ek];
            if alternatives_equivalent(
                &derived_decl.alternatives,
                &base_decl.alternatives,
                schema_set,
            ) {
                continue;
            }
            let qn_str = match ns {
                Some(ns_id) => format!(
                    "{{{}}}{}",
                    schema_set.name_table.resolve_ref(*ns_id),
                    schema_set.name_table.resolve_ref(*name),
                ),
                None => schema_set.name_table.resolve_ref(*name).to_string(),
            };
            let location = schema_set
                .locate(derived_src.as_ref())
                .or_else(|| schema_set.locate(ct.source.as_ref()));
            let derived_name = format_type_name(schema_set, ct.name, ct.target_namespace);
            let base_name = format_type_name(schema_set, base_ct.name, base_ct.target_namespace);
            return Err(SchemaError::structural(
                "cos-element-consistent",
                format!(
                    "Complex type '{}' restricting '{}': local element '{}' has a \
                     type table that is not equivalent to the base type's local \
                     element of the same name (§3.4.6.3 / cos-element-consistent)",
                    derived_name, base_name, qn_str,
                ),
                location,
            ));
        }
    }

    Ok(())
}

/// Whether the wildcard's namespace constraint and notQName admit the QName
/// `(ns, name)`. Treats `##defined` and `##definedSibling` pessimistically
/// (rejects).
#[cfg(feature = "xsd11")]
fn wildcard_result_admits_qname(
    wc: &WildcardResult,
    target_ns: Option<NameId>,
    ns: Option<NameId>,
    name: NameId,
) -> bool {
    use crate::parser::frames::NotQNameItem;
    if !wildcard_namespace_matches(&wc.namespace, ns, target_ns) {
        return false;
    }
    if wc.not_namespace.iter().any(|t| t.resolve(target_ns) == ns) {
        return false;
    }
    !wc.not_qname.iter().any(|item| match item {
        NotQNameItem::QName {
            namespace,
            local_name,
        } => *namespace == ns && *local_name == name,
        NotQNameItem::Defined | NotQNameItem::DefinedSibling => true,
    })
}
