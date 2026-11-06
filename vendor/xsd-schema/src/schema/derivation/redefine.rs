//! §src-redefine 6.2.2 / 7.2.2 deferred restriction checks for
//! `<xs:redefine>` (and `<xs:override>`) group and attribute-group
//! redefinitions.

use super::attributes::{
    classify_attribute_wildcard_restriction, collect_attribute_group_uses, EffectiveAttributeUse,
    WildcardRestrictionOutcome,
};
use super::normalize::{is_effectively_empty, normalize_model_group_as_particle};
use super::particle::{particle_accepts_nothing, particle_is_emptiable, particle_restricts};
use super::{
    effective_attribute_wildcard, effective_wildcard_allows_attribute, format_type_name,
    is_type_derived_from, DerivationStats,
};
use crate::error::SchemaError;
use crate::ids::AttributeGroupKey;
use crate::parser::frames::AttributeUseKind;
use crate::schema::SchemaSet;

// ---------------------------------------------------------------------------
// §src-redefine 6.2.2 / 7.2.2 — deferred restriction validation for redefines
// ---------------------------------------------------------------------------
//
// When an `<xs:redefine>` child group (or attribute group) has zero
// self-references, §src-redefine clauses 6.2.2 / 7.2.2 require that the
// redefined component be a *valid restriction* of the original. Composition
// (`schema/redefine.rs`) validates the self-reference shape (clauses 6.1 /
// 7.1) and flags zero-self-ref redefines via
// `redefine_requires_restriction_check`; this module does the deferred
// restriction check after reference resolution is complete.
//
// Spec anchors (source of truth: `structures.html` — W3C XSD 1.1 §src-redefine
// and §3.4.6.3 Derivation Valid (Restriction, Complex)):
//   - §src-redefine 6.2.2: redefined model group must be a valid restriction
//     of the original per §3.9.6 Particle Valid (Restriction).
//   - §src-redefine 7.2.2: redefined attribute group must satisfy clause 3
//     of §3.4.6.3 (clause-3 only, NOT clause-4 local-type-substitution).
//   - §3.8: pointless particles (`maxOccurs=0`) are eliminated before the
//     restriction check.
//   - §3.2.2: a prohibited `<xs:attribute>` is NOT an attribute use.
//
// Scope limitations:
//   - Chained redefines (`orig → v1 → v2`) resolve nested `group-ref`s via
//     the currently bound namespace version; see
//     `normalize_model_group_as_particle` doc comment.

/// Flatten an attribute group's effective attribute uses, filtering out
/// prohibited uses per §3.2.2 (a prohibited `<xs:attribute>` is not an
/// attribute use on either side of a restriction comparison).
pub(super) fn collect_flat_attribute_uses_for_group(
    schema_set: &SchemaSet,
    ag_key: AttributeGroupKey,
) -> Vec<EffectiveAttributeUse> {
    let mut result = Vec::new();
    collect_attribute_group_uses(schema_set, ag_key, &mut result, 0);
    result.retain(|eau| eau.use_kind != AttributeUseKind::Prohibited);
    result
}

/// Construct a §src-redefine 6.2.2 structural error for a model group whose
/// restriction of its original cannot be validated.
fn make_redefine_group_restriction_error(
    schema_set: &SchemaSet,
    derived: &crate::arenas::ModelGroupData,
    detail: &str,
) -> SchemaError {
    let name = format_type_name(schema_set, derived.name, derived.target_namespace);
    let location = derived
        .source
        .as_ref()
        .and_then(|s| schema_set.source_maps.locate(s));
    SchemaError::structural(
        "src-redefine.6.2.2",
        format!(
            "Redefined group '{}' must be a valid restriction of the original \
             (§src-redefine 6.2.2): {}",
            name, detail,
        ),
        location,
    )
}

/// Construct a §src-redefine 7.2.2 structural error for an attribute group
/// whose restriction of its original cannot be validated.
fn make_redefine_attr_group_restriction_error(
    schema_set: &SchemaSet,
    derived: &crate::arenas::AttributeGroupData,
    detail: &str,
) -> SchemaError {
    let name = format_type_name(schema_set, derived.name, derived.target_namespace);
    let location = derived
        .source
        .as_ref()
        .and_then(|s| schema_set.source_maps.locate(s));
    SchemaError::structural(
        "src-redefine.7.2.2",
        format!(
            "Redefined attribute group '{}' must be a valid restriction of the original \
             (§src-redefine 7.2.2): {}",
            name, detail,
        ),
        location,
    )
}

/// Driver for §src-redefine 6.2.2: for each model group flagged as a
/// zero-self-reference redefine, verify its normalized particle is a valid
/// restriction of the original's normalized particle per §3.9.6 Particle
/// Valid (Restriction).
pub(super) fn validate_all_redefine_group_restrictions(
    schema_set: &SchemaSet,
    errors: &mut Vec<SchemaError>,
    stats: &mut DerivationStats,
) {
    for (_key, derived) in schema_set.arenas.model_groups.iter() {
        if !derived.redefine_requires_restriction_check {
            continue;
        }
        let Some(original_key) = derived.redefine_original else {
            continue;
        };
        let Some(original) = schema_set.arenas.model_groups.get(original_key) else {
            continue;
        };

        let derived_particle = match normalize_model_group_as_particle(schema_set, derived) {
            Ok(p) => p,
            Err(e) => {
                errors.push(e);
                stats.errors += 1;
                continue;
            }
        };
        let base_particle = match normalize_model_group_as_particle(schema_set, original) {
            Ok(p) => p,
            Err(e) => {
                errors.push(e);
                stats.errors += 1;
                continue;
            }
        };

        // Empty-group special case (§3.8 + §3.9.6): when the derived group
        // normalizes to empty content after pointless-particle removal
        // (e.g. its only child had `maxOccurs=0`), `particle_restricts` does
        // not model the "empty content" case correctly — it would reject
        // legal restrictions whenever the base does not normalize to the
        // exact same surviving shape. Mirror the existing short-circuit in
        // `validate_content_particle_restriction` (derivation.rs:1148-1161):
        // empty derived is a valid restriction iff the base is emptiable.
        // A derived group that accepts no sequence at all: see
        // `validate_content_particle_restriction`.
        if schema_set.is_xsd11() && particle_accepts_nothing(&derived_particle) {
            continue;
        }
        if is_effectively_empty(&derived_particle) {
            if !particle_is_emptiable(&base_particle) {
                errors.push(make_redefine_group_restriction_error(
                    schema_set,
                    derived,
                    "removes required content model of the original group",
                ));
                stats.errors += 1;
            }
            continue;
        }

        if !particle_restricts(schema_set, &derived_particle, &base_particle) {
            errors.push(make_redefine_group_restriction_error(
                schema_set,
                derived,
                "content model is not a valid restriction of the original group",
            ));
            stats.errors += 1;
        }
    }
}

/// Driver for §src-redefine 7.2.2: implementation of §3.4.6.3 clause 3
/// (derivation-ok-restriction, attribute side) applied to redefined
/// attribute groups. Checks:
///  - every derived attribute is present in the base (direct match) or
///    admitted by the base's effective attribute wildcard (§3.6.2.2);
///  - clause 3(b) type tightening on directly-matched pairs;
///  - required-stays-required;
///  - wildcard-vs-wildcard subset: the derived group's effective
///    attribute wildcard must be a valid restriction of the original's.
pub(super) fn validate_all_redefine_attribute_group_restrictions(
    schema_set: &SchemaSet,
    errors: &mut Vec<SchemaError>,
    stats: &mut DerivationStats,
) {
    for (_key, derived) in schema_set.arenas.attribute_groups.iter() {
        if !derived.redefine_requires_restriction_check {
            continue;
        }
        let Some(original_key) = derived.redefine_original else {
            continue;
        };
        let Some(original) = schema_set.arenas.attribute_groups.get(original_key) else {
            continue;
        };

        // Flatten both sides, filtering out Prohibited uses (§3.2.2).
        // The derived key is the key we're iterating on — look it up to
        // get an `AttributeGroupKey` for `collect_flat_attribute_uses_for_group`.
        let derived_attrs = collect_flat_attribute_uses_for_group(schema_set, _key);
        let base_attrs = collect_flat_attribute_uses_for_group(schema_set, original_key);

        // Compute the base's effective attribute wildcard per §3.6.2.2
        // once, outside the per-attribute loop. This is the full
        // intersection across the original group's local wildcard and
        // the wildcards of every referenced nested attribute group.
        let base_effective_wc = match effective_attribute_wildcard(
            schema_set,
            original.attribute_wildcard.as_ref(),
            original.target_namespace,
            &original.resolved_attribute_groups,
        ) {
            Ok(eff) => eff,
            Err(e) => {
                errors.push(e);
                stats.errors += 1;
                continue;
            }
        };

        // Subset check (clause 3, first half): every derived attribute must
        // be valid in the base, either directly by (namespace, name) match
        // or via the base's effective {attribute wildcard}. Also applies the
        // clause 3(b) type-subsumption check for directly-matched pairs.
        let mut failed = false;
        for da in &derived_attrs {
            // (a) Direct match by (namespace, name).
            if let Some(ba) = base_attrs
                .iter()
                .find(|b| b.name == da.name && b.target_namespace == da.target_namespace)
            {
                // Type tightening (clause 3(b)): derived type must equal or
                // be derived from base type when both are resolved.
                if let (Some(dt), Some(bt)) = (da.resolved_type, ba.resolved_type) {
                    if dt != bt && !is_type_derived_from(schema_set, dt, bt) {
                        let attr_name_str = schema_set.name_table.resolve(da.name).to_string();
                        errors.push(make_redefine_attr_group_restriction_error(
                            schema_set,
                            derived,
                            &format!(
                                "attribute '{}' has a type that is not validly derived from the \
                                 base attribute type",
                                attr_name_str,
                            ),
                        ));
                        stats.errors += 1;
                        failed = true;
                        break;
                    }
                }
                // Fixed-value tightening (clause 3, derivation-ok-restriction
                // §3.4.6.3 attribute side): if the base attribute use has
                // {value constraint} = (fixed, V), the derived attribute use
                // must also have {value constraint} = (fixed, V). It cannot
                // be relaxed to (default, V), nor removed entirely. The W3C
                // `schM10` fixture exercises the fixed→default relaxation.
                if let Some(ref base_fixed) = ba.fixed_value {
                    let derived_matches =
                        da.fixed_value.as_ref().is_some_and(|dv| dv == base_fixed);
                    if !derived_matches {
                        let attr_name_str = schema_set.name_table.resolve(da.name).to_string();
                        errors.push(make_redefine_attr_group_restriction_error(
                            schema_set,
                            derived,
                            &format!(
                                "attribute '{}' relaxes or removes the base 'fixed=\"{}\"' \
                                 value constraint",
                                attr_name_str, base_fixed,
                            ),
                        ));
                        stats.errors += 1;
                        failed = true;
                        break;
                    }
                }
                continue;
            }
            // (b) Admitted by the base's *effective* {attribute wildcard}
            // (§3.6.2.2), not just the original group's local wildcard.
            if let Some(ref bwc) = base_effective_wc {
                if effective_wildcard_allows_attribute(
                    schema_set,
                    bwc,
                    da.target_namespace,
                    da.name,
                ) {
                    continue;
                }
            }
            // Neither (a) nor (b) holds — not a valid restriction.
            let attr_name_str = schema_set.name_table.resolve(da.name).to_string();
            errors.push(make_redefine_attr_group_restriction_error(
                schema_set,
                derived,
                &format!(
                    "attribute '{}' is not present in the original and is not admitted by \
                     the original's attribute wildcard",
                    attr_name_str,
                ),
            ));
            stats.errors += 1;
            failed = true;
            break;
        }
        if failed {
            continue;
        }

        // Required-stays-required (clause 3(a)): every base Required attribute
        // must also be Required in the derived side.
        let mut req_failed = false;
        for ba in &base_attrs {
            if ba.use_kind != AttributeUseKind::Required {
                continue;
            }
            let matching = derived_attrs
                .iter()
                .find(|d| d.name == ba.name && d.target_namespace == ba.target_namespace);
            match matching {
                Some(da) if da.use_kind == AttributeUseKind::Required => {}
                _ => {
                    let attr_name_str = schema_set.name_table.resolve(ba.name).to_string();
                    errors.push(make_redefine_attr_group_restriction_error(
                        schema_set,
                        derived,
                        &format!(
                            "base attribute '{}' is required but the redefined group does not \
                             declare it as required",
                            attr_name_str,
                        ),
                    ));
                    stats.errors += 1;
                    req_failed = true;
                    break;
                }
            }
        }
        if req_failed {
            continue;
        }

        // Wildcard-vs-wildcard subset (clause 3, second half of §3.6.2.2):
        // the derived group's effective attribute wildcard must be a
        // valid restriction of the original's. This catches cases where
        // the redefined group broadens an inherited wildcard even when
        // every directly-named attribute already checks out.
        let derived_effective_wc = match effective_attribute_wildcard(
            schema_set,
            derived.attribute_wildcard.as_ref(),
            derived.target_namespace,
            &derived.resolved_attribute_groups,
        ) {
            Ok(eff) => eff,
            Err(e) => {
                errors.push(e);
                stats.errors += 1;
                continue;
            }
        };
        match classify_attribute_wildcard_restriction(
            schema_set,
            derived_effective_wc.as_ref(),
            base_effective_wc.as_ref(),
            true,
        ) {
            WildcardRestrictionOutcome::DerivedAbsent | WildcardRestrictionOutcome::Valid => {}
            WildcardRestrictionOutcome::AddedInDerived => {
                errors.push(make_redefine_attr_group_restriction_error(
                    schema_set,
                    derived,
                    "redefined attribute group declares an attribute wildcard but \
                     the original has none",
                ));
                stats.errors += 1;
            }
            WildcardRestrictionOutcome::NotSubset(reason) => {
                errors.push(make_redefine_attr_group_restriction_error(
                    schema_set,
                    derived,
                    &format!(
                        "attribute wildcard is not a valid restriction of the \
                         original: {}",
                        reason,
                    ),
                ));
                stats.errors += 1;
            }
        }
    }
}
