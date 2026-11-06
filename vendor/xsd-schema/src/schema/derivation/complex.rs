//! Complex-type derivation: `cos-ct-extends` (extension),
//! `derivation-ok-restriction` (restriction), mixed-content parity and
//! simple-content restriction.

use super::attributes::{
    validate_attribute_restriction, validate_xsd10_attribute_wildcard_union_expressible,
};
use super::normalize::normalize_type_particle;
use super::particle::{particle_is_emptiable, validate_content_particle_restriction};
#[cfg(feature = "xsd11")]
use super::wildcard::{
    validate_all_group_restriction_edc, validate_open_content_extension,
    validate_open_content_restriction,
};
use super::{format_type_name, is_type_derived_from, type_error_context, DerivationStats};
use crate::error::{SchemaError, SchemaResult};
use crate::ids::{ComplexTypeKey, TypeKey};
use crate::parser::frames::{
    ComplexContentResult, Compositor, DerivationMethod, ModelGroupDefResult, ParticleTerm,
};
use crate::schema::SchemaSet;

/// Validate a complex type definition
pub(super) fn validate_complex_type(
    schema_set: &SchemaSet,
    key: ComplexTypeKey,
    stats: &mut DerivationStats,
) -> SchemaResult<()> {
    let type_def = schema_set
        .arenas
        .complex_types
        .get(key)
        .ok_or_else(|| SchemaError::internal("Complex type not found in arena"))?;

    stats.complex_types_validated += 1;

    // Check derivation method
    match type_def.derivation_method {
        Some(DerivationMethod::Extension) => {
            stats.extensions_validated += 1;
            validate_complex_extension(schema_set, key, type_def)?;
        }
        Some(DerivationMethod::Restriction) => {
            stats.restrictions_validated += 1;
            validate_complex_restriction(schema_set, type_def)?;
        }
        None => {
            // No explicit derivation - this is a new complex type definition
            // Implicitly derived from xs:anyType by restriction
        }
    }

    Ok(())
}

/// Validate complex type extension
///
/// Constraint: cos-ct-extends (Complex Type Derivation OK - Extension)
fn validate_complex_extension(
    schema_set: &SchemaSet,
    #[cfg_attr(not(feature = "xsd11"), allow(unused_variables))] derived_key: ComplexTypeKey,
    type_def: &crate::arenas::ComplexTypeDefData,
) -> SchemaResult<()> {
    // Get base type
    let base_key = match type_def.resolved_base_type {
        Some(key) => key,
        None => return Ok(()), // No base type
    };

    // Check that base type exists and is accessible
    match base_key {
        TypeKey::Simple(base_simple_key) => {
            // Extension from simple type is valid only with simpleContent
            // complexContent extension from simple type is invalid
            if matches!(type_def.content, ComplexContentResult::Complex(_)) {
                let (location, type_name) = type_error_context(schema_set, type_def);
                return Err(SchemaError::structural(
                    "cos-ct-extends",
                    format!(
                        "Complex type '{}' cannot use complexContent extension from a simple type",
                        type_name,
                    ),
                    location,
                ));
            }

            // Check that simple base type is not final for extension
            if let Some(base_type) = schema_set.arenas.simple_types.get(base_simple_key) {
                if base_type.final_derivation.contains_extension() {
                    let (location, type_name) = type_error_context(schema_set, type_def);
                    let base_name =
                        format_type_name(schema_set, base_type.name, base_type.target_namespace);
                    return Err(SchemaError::structural(
                        "cos-ct-extends",
                        format!(
                            "Complex type '{}' cannot extend simple type '{}' because it is final for extension",
                            type_name, base_name,
                        ),
                        location,
                    ));
                }
            }
        }
        TypeKey::Complex(base_complex_key) => {
            if let Some(base_type) = schema_set.arenas.complex_types.get(base_complex_key) {
                // Check that base type is not final for extension
                if base_type.final_derivation.contains_extension() {
                    let (location, type_name) = type_error_context(schema_set, type_def);
                    let base_name =
                        format_type_name(schema_set, base_type.name, base_type.target_namespace);
                    return Err(SchemaError::structural(
                        "cos-ct-extends",
                        format!(
                            "Complex type '{}' cannot extend '{}' because base type is final for extension",
                            type_name, base_name
                        ),
                        location,
                    ));
                }

                // src-ct.2 (§3.4.6.2): when the derived type uses <xs:simpleContent>
                // and the <xs:extension> alternative, the base's {content type}
                // must be either a simple type (clause 2.1.3 requires a simple-type
                // base, handled above via TypeKey::Simple) or a complex type whose
                // {content type} is a simple type definition (clause 2.1.1).
                // A base with element-only or mixed complex content is rejected.
                if matches!(type_def.content, ComplexContentResult::Simple(_))
                    && !matches!(base_type.content, ComplexContentResult::Simple(_))
                {
                    let (location, type_name) = type_error_context(schema_set, type_def);
                    let base_name =
                        format_type_name(schema_set, base_type.name, base_type.target_namespace);
                    return Err(SchemaError::structural(
                        "src-ct",
                        format!(
                            "Complex type '{}' uses xs:simpleContent extension but base '{}' \
                             does not have a simple {{content type}} (src-ct.2.1.1)",
                            type_name, base_name,
                        ),
                        location,
                    ));
                }

                // cos-ct-extends: Cannot use complexContent extension to add particles
                // to a base type with simpleContent.
                // XSD 1.0: only rejected when a particle is actually added.
                // XSD 1.1 cos-ct-extends clause 1.4: content variety must match;
                //   simpleContent base + complexContent derived is always invalid.
                if matches!(base_type.content, ComplexContentResult::Simple(_)) {
                    if let ComplexContentResult::Complex(ref complex) = type_def.content {
                        if complex.particle.is_some() || schema_set.is_xsd11() {
                            let (location, type_name) = type_error_context(schema_set, type_def);
                            let base_name = format_type_name(
                                schema_set,
                                base_type.name,
                                base_type.target_namespace,
                            );
                            return Err(SchemaError::structural(
                                "cos-ct-extends",
                                format!(
                                    "Complex type '{}' cannot use complexContent to extend '{}' which has simpleContent{}",
                                    type_name, base_name,
                                    if complex.particle.is_some() { " with element content" }
                                    else { " (XSD 1.1 cos-ct-extends clause 1.4)" },
                                ),
                                location,
                            ));
                        }
                    }
                }

                validate_extension_mixed_parity(schema_set, type_def, base_type)?;

                validate_xsd10_attribute_wildcard_union_expressible(
                    schema_set,
                    type_def,
                    base_complex_key,
                )?;

                // cos-ct-extends / cos-particle-extend: Cannot extend non-empty
                // non-all content with an all compositor.  The effective content
                // type of an extension is sequence(base, extension) per §3.4.2.3.3.
                // cos-particle-extend §3.9.6.2 only allows: (1) same particle,
                // (2) E is a sequence wrapping B, or (3) both are all groups.
                // An all group nested inside a sequence also violates
                // cos-all-limited.1 (placement constraint).
                //
                // Exception: XSD 1.1 allows all-over-all extensions (clause 3 of
                // cos-particle-extend). If both base and extension are all groups,
                // skip this check.
                if let ComplexContentResult::Complex(ref base_complex) = base_type.content {
                    if let Some(ref base_particle) = base_complex.particle {
                        if let ComplexContentResult::Complex(ref derived_complex) = type_def.content
                        {
                            if let Some(ref ext_particle) = derived_complex.particle {
                                let ext_compositor = match &ext_particle.term {
                                    ParticleTerm::Group(mg) => mg.compositor,
                                    _ => None,
                                };
                                let base_is_all = matches!(
                                    base_particle.term,
                                    ParticleTerm::Group(ModelGroupDefResult {
                                        compositor: Some(Compositor::All),
                                        ..
                                    })
                                );

                                // cos-particle-extend §3.9.6.2: over a non-empty base,
                                // the only valid extension shapes are:
                                // (1) No extension particle  (handled by outer if-let)
                                // (2) Extension particle is a sequence
                                // (3) XSD 1.1: all-over-all
                                match ext_compositor {
                                    Some(Compositor::Sequence) => {
                                        // OK: sequence extension is always valid
                                    }
                                    Some(Compositor::All)
                                        if base_is_all
                                            && schema_set.xsd_version
                                                == crate::schema::model::XsdVersion::V1_1 =>
                                    {
                                        // OK: XSD 1.1 all-over-all
                                    }
                                    Some(Compositor::Choice) if !base_is_all => {
                                        // OK: the effective content type is
                                        // sequence(base, extension) per §3.4.2.3.3
                                        // clause 4.2.3.3, so cos-particle-extend
                                        // clause 2 is satisfied regardless of the
                                        // extension particle's compositor — as long
                                        // as the base particle is not xs:all (which
                                        // would get nested inside the sequence and
                                        // violate cos-all-limited.1).
                                    }
                                    Some(compositor @ (Compositor::All | Compositor::Choice)) => {
                                        let location = type_def
                                            .source
                                            .as_ref()
                                            .and_then(|s| schema_set.source_maps.locate(s));
                                        let type_name = format_type_name(
                                            schema_set,
                                            type_def.name,
                                            type_def.target_namespace,
                                        );
                                        let base_name = format_type_name(
                                            schema_set,
                                            base_type.name,
                                            base_type.target_namespace,
                                        );
                                        let (comp_name, reason) = match compositor {
                                            Compositor::All => (
                                                "all",
                                                "the resulting content model would \
                                                 violate cos-all-limited placement \
                                                 constraints",
                                            ),
                                            Compositor::Choice => (
                                                "choice",
                                                "the base type's xs:all particle would \
                                                 be nested inside a sequence, \
                                                 violating cos-all-limited.1",
                                            ),
                                            Compositor::Sequence => unreachable!(),
                                        };
                                        return Err(SchemaError::structural(
                                            "cos-ct-extends",
                                            format!(
                                                "Complex type '{}' cannot extend '{}' with \
                                                 an xs:{} compositor because the base type \
                                                 has non-empty content; {}",
                                                type_name, base_name, comp_name, reason,
                                            ),
                                            location,
                                        ));
                                    }
                                    None => {
                                        // Bare element or wildcard term (no model group
                                        // wrapper).  The effective content type mapping
                                        // wraps it in a sequence with the base, so this
                                        // is equivalent to a sequence extension — OK.
                                    }
                                }
                            }
                        }
                    }
                }

                // XSD 1.1: Validate open-content compatibility
                #[cfg(feature = "xsd11")]
                validate_open_content_extension(
                    schema_set,
                    derived_key,
                    type_def,
                    base_complex_key,
                    base_type,
                )?;

                // ct-props-correct.4 is enforced globally by
                // `validate_complex_type_attribute_uniqueness` (run from
                // `pipeline.rs` after reference resolution); no extension-
                // local check needed here.
            }
        }
    }

    Ok(())
}

/// Validate complex type restriction
///
/// Constraint: derivation-ok-restriction (Complex Type Derivation OK - Restriction)
fn validate_complex_restriction(
    schema_set: &SchemaSet,
    type_def: &crate::arenas::ComplexTypeDefData,
) -> SchemaResult<()> {
    // Get base type
    let base_key = match type_def.resolved_base_type {
        Some(key) => key,
        None => return Ok(()), // No base type (derived from anyType)
    };

    match base_key {
        TypeKey::Simple(base_simple_key) => {
            // ct-props-correct.2: If the base type is a simple type definition,
            // the derivation method must be extension (not restriction).
            let (location, type_name) = type_error_context(schema_set, type_def);
            let base_name =
                if let Some(base_type) = schema_set.arenas.simple_types.get(base_simple_key) {
                    format_type_name(schema_set, base_type.name, base_type.target_namespace)
                } else {
                    "(unknown)".to_string()
                };
            return Err(SchemaError::structural(
                "ct-props-correct",
                format!(
                    "Complex type '{}' cannot restrict simple type '{}'; \
                     derivation from a simple type must use extension",
                    type_name, base_name,
                ),
                location,
            ));
        }
        TypeKey::Complex(base_complex_key) => {
            if let Some(base_type) = schema_set.arenas.complex_types.get(base_complex_key) {
                // Check that base type is not final for restriction
                if base_type.final_derivation.contains_restriction() {
                    let (location, type_name) = type_error_context(schema_set, type_def);
                    let base_name =
                        format_type_name(schema_set, base_type.name, base_type.target_namespace);
                    return Err(SchemaError::structural(
                        "derivation-ok-restriction",
                        format!(
                            "Complex type '{}' cannot restrict '{}' because base type is final for restriction",
                            type_name, base_name
                        ),
                        location,
                    ));
                }

                // §3.4.6.4 clause 5.4.1 mixed-parity (one direction only):
                // restricting an element-only base to a `mixed="true"` derived
                // type would add character data, which is incompatible with
                // restriction. The reverse (mixed → element-only) is the
                // "pointless mixed restriction" tolerated by Saxon/Xerces and
                // exercised by valid-schema tests particlesL012 / mgA015 /
                // idK012, so only the asymmetric error is enforced here.
                // ctF006 (element-only choice + mixed restriction).
                if let (
                    ComplexContentResult::Complex(base_complex),
                    ComplexContentResult::Complex(derived_complex),
                ) = (&base_type.content, &type_def.content)
                {
                    let base_mixed = effective_mixed_of(base_type, base_complex);
                    let derived_mixed = effective_mixed_of(type_def, derived_complex);
                    if derived_mixed && !base_mixed {
                        let (location, type_name) = type_error_context(schema_set, type_def);
                        let base_name = format_type_name(
                            schema_set,
                            base_type.name,
                            base_type.target_namespace,
                        );
                        return Err(SchemaError::structural(
                            "derivation-ok-restriction",
                            format!(
                                "Complex type '{}' cannot restrict element-only base '{}' \
                                 to mixed content (§3.4.6.4 clause 5.4.1)",
                                type_name, base_name,
                            ),
                            location,
                        ));
                    }
                }

                // src-ct.2 (§3.4.6.2): when the derived type uses <xs:simpleContent>
                // and the <xs:restriction> alternative, the base must be either a
                // complex type with a simple {content type} (clause 2.1.1) or a
                // complex type whose {content type} is mixed and whose particle is
                // emptiable (clause 2.1.2). Element-only or mixed-but-not-emptiable
                // bases are rejected.
                if matches!(type_def.content, ComplexContentResult::Simple(_))
                    && !is_valid_simple_content_restriction_base(schema_set, base_type)
                {
                    let (location, type_name) = type_error_context(schema_set, type_def);
                    let base_name =
                        format_type_name(schema_set, base_type.name, base_type.target_namespace);
                    return Err(SchemaError::structural(
                        "src-ct",
                        format!(
                            "Complex type '{}' uses xs:simpleContent restriction but base '{}' \
                             does not have a simple {{content type}} nor mixed+emptiable content \
                             (src-ct.2.1.1 / 2.1.2)",
                            type_name, base_name,
                        ),
                        location,
                    ));
                }

                // cos-applicable-facets: facets in a simpleContent restriction
                // constrain the base's effective simple content type. When that
                // content type is xs:anySimpleType (absent {variety}), no
                // constraining facet is applicable (msData stZ010: minLength
                // over an anySimpleType content type).
                if let ComplexContentResult::Simple(sc) = &type_def.content {
                    if !sc.facets.is_empty() {
                        if let Some(TypeKey::Simple(sk)) =
                            effective_simple_content_type_key(schema_set, base_type)
                        {
                            if sk == schema_set.builtin_types().any_simple_type {
                                let (location, type_name) =
                                    type_error_context(schema_set, type_def);
                                return Err(SchemaError::structural(
                                    "cos-applicable-facets",
                                    format!(
                                        "Complex type '{}': constraining facets are not \
                                         applicable to xs:anySimpleType content \
                                         (absent variety)",
                                        type_name
                                    ),
                                    location,
                                ));
                            }
                        }
                    }
                }

                // src-ct.2.2: when the base qualifies only via clause 2.1.2
                // (mixed content with emptiable particle, e.g. xs:anyType),
                // the <xs:restriction> must supply the simple content type
                // itself via an inline <xs:simpleType> child (msData ctD004).
                if let ComplexContentResult::Simple(sc) = &type_def.content {
                    let base_has_simple_content =
                        matches!(base_type.content, ComplexContentResult::Simple(_));
                    if !base_has_simple_content && sc.content_type.is_none() {
                        let (location, type_name) = type_error_context(schema_set, type_def);
                        let base_name = format_type_name(
                            schema_set,
                            base_type.name,
                            base_type.target_namespace,
                        );
                        return Err(SchemaError::structural(
                            "src-ct",
                            format!(
                                "Complex type '{}': simpleContent restriction of mixed base '{}' \
                                 requires an inline <simpleType> child (src-ct.2.2)",
                                type_name, base_name,
                            ),
                            location,
                        ));
                    }
                }

                validate_content_particle_restriction(schema_set, type_def, base_type)?;

                // XSD 1.1 §3.4.6.4 / cos-element-consistent (extended for
                // all-group restrictions): when a derived all-group restricts
                // a base all-group and removes a base local element, any
                // wildcard in the derived that admits the removed element's
                // QName must resolve to a governing type that's substitutable
                // for the base local's type. This is the schema-time analog of
                // dynamic EDC, applied where the all-group's unordered
                // matching makes the conflict structurally inevitable
                // (wild069 — the corresponding xs:sequence case wild068 is
                // covered by runtime dynamic EDC instead).
                #[cfg(feature = "xsd11")]
                validate_all_group_restriction_edc(schema_set, type_def, base_type)?;

                // XSD 1.1: Validate open-content compatibility
                #[cfg(feature = "xsd11")]
                validate_open_content_restriction(schema_set, type_def, base_type)?;

                // Validate attribute restriction (derivation-ok-restriction clause 3)
                validate_attribute_restriction(schema_set, type_def, base_type)?;

                // Validate simpleContent inline type restriction
                // (derivation-ok-restriction clause 2.2.2.1)
                validate_simple_content_restriction(schema_set, type_def, base_type)?;

                // The simpleContent restriction's own facets restrict the
                // base content type (Datatypes constraints on facets).
                super::simple::validate_simple_content_restriction_facets(
                    schema_set,
                    type_def,
                    base_complex_key,
                )?;
            }
        }
    }

    Ok(())
}

/// §3.4.2.3 mapping: the `mixed` flag carried by a `<complexContent>` wrapper
/// overrides the outer `<complexType mixed="…">` attribute.  For complex
/// types authored in the short form (no `<complexContent>` wrapper), the
/// outer attribute applies unchanged.
fn effective_mixed_of(
    type_def: &crate::arenas::ComplexTypeDefData,
    complex: &crate::parser::frames::ComplexContentDefResult,
) -> bool {
    // `complex.mixed` reflects the `<complexContent mixed="…">` attribute.
    // When the complexType was parsed from a short form, the wrapper is
    // synthesized with mixed=false and the outer attribute is preserved on
    // `type_def.mixed` — so we OR the two.  When the wrapper is present,
    // `type_def.mixed` carries the same bit, so the OR is a no-op.
    complex.mixed || type_def.mixed
}

/// cos-ct-extends clause 1.4.3.2.2.4.1 (§3.4.6.2): when the derived type
/// supplies its own particle, the effective mixed of the derived {content
/// type} must match the base's — both element-only, or both mixed. Also
/// fires when the derived has no own particle but explicitly declares a
/// `mixed=` value that disagrees with the base (ctF008): per §3.4.2.3
/// clause 4.1 the {content type} inherits from the base, but Saxon/Xerces
/// (and the W3C suite) treat the contradictory `mixed="true"` declaration
/// as a structural error. The "no-particle and no explicit mixed flag"
/// case is the only scenario that must be skipped, because there is then
/// no inconsistency.
fn validate_extension_mixed_parity(
    schema_set: &SchemaSet,
    type_def: &crate::arenas::ComplexTypeDefData,
    base_type: &crate::arenas::ComplexTypeDefData,
) -> SchemaResult<()> {
    let ComplexContentResult::Complex(ref base_complex) = base_type.content else {
        return Ok(());
    };
    let ComplexContentResult::Complex(ref derived_complex) = type_def.content else {
        return Ok(());
    };
    let base_mixed = effective_mixed_of(base_type, base_complex);
    let derived_mixed = effective_mixed_of(type_def, derived_complex);
    if derived_mixed == base_mixed {
        return Ok(());
    }
    // No own particle and no explicit mixed declaration → parity is
    // trivially inherited from the base; nothing to check.
    if derived_complex.particle.is_none() && !derived_complex.mixed && !type_def.mixed {
        return Ok(());
    }
    let (location, type_name) = type_error_context(schema_set, type_def);
    let base_name = format_type_name(schema_set, base_type.name, base_type.target_namespace);
    Err(SchemaError::structural(
        "cos-ct-extends",
        format!(
            "Complex type '{}' cannot extend '{}' — derived is {} but base is {} \
             (cos-ct-extends clause 1.4.3.2.2.4.1)",
            type_name,
            base_name,
            if derived_mixed {
                "mixed"
            } else {
                "element-only"
            },
            if base_mixed { "mixed" } else { "element-only" },
        ),
        location,
    ))
}

/// §3.4.6.2 src-ct.2: a complex type derived via `<xs:simpleContent>` from
/// another complex base is legal only when the base's `{content type}` is
/// a simple type (clause 2.1.1) or when — for the restriction branch — the
/// base is mixed with an emptiable particle (clause 2.1.2).
/// Returns `true` if the base is acceptable for a simpleContent restriction.
fn is_valid_simple_content_restriction_base(
    schema_set: &SchemaSet,
    base: &crate::arenas::ComplexTypeDefData,
) -> bool {
    match &base.content {
        ComplexContentResult::Simple(_) => true,
        ComplexContentResult::Complex(complex) => {
            // Clause 2.1.2: mixed content with an emptiable particle.  The
            // `mixed` flag on a <complexContent> wrapper overrides the outer
            // <complexType mixed="…"> attribute per §3.4.2.2, so consult it
            // here; the outer flag applies only to the short-form path.
            if !complex.mixed {
                return false;
            }
            match &complex.particle {
                // Absent particle ≡ empty sequence ≡ emptiable.
                None => true,
                Some(particle) => match normalize_type_particle(schema_set, base, particle) {
                    Ok(normalized) => particle_is_emptiable(&normalized),
                    Err(_) => false,
                },
            }
        }
        // Short-form complex type without <simpleContent>/<complexContent>:
        // the {content type} is determined by §3.4.2.2 from the outer
        // `mixed` attribute and any top-level particle.  When `mixed` is
        // true and no particle is present, the type has mixed emptiable
        // content and is a valid base for simpleContent restriction
        // (clause 2.1.2).  When `mixed` is false, the content type is
        // element-only-empty, which is not a valid base.
        ComplexContentResult::Empty => base.mixed,
    }
}

/// Walk the complex type extension chain to find the effective simple content
/// type key. Returns `None` if there is no simple content type in the chain.
pub(super) fn effective_simple_content_type_key(
    schema_set: &SchemaSet,
    type_def: &crate::arenas::ComplexTypeDefData,
) -> Option<TypeKey> {
    let mut current_base = type_def.resolved_base_type?;
    for _ in 0..50 {
        match current_base {
            TypeKey::Simple(sk) => return Some(TypeKey::Simple(sk)),
            TypeKey::Complex(ck) => {
                let ct = schema_set.arenas.complex_types.get(ck)?;
                current_base = ct.resolved_base_type?;
            }
        }
    }
    None
}

/// Validate simpleContent restriction inline simpleType.
///
/// derivation-ok-restriction clause 2.2.2.1 (§3.4.6.3): let S_B = B's content
/// type simple type definition and S_T = T's content type simple type definition.
/// S_T must be validly derived from S_B.
fn validate_simple_content_restriction(
    schema_set: &SchemaSet,
    derived: &crate::arenas::ComplexTypeDefData,
    base: &crate::arenas::ComplexTypeDefData,
) -> SchemaResult<()> {
    // Only applies when derived has simpleContent with an inline simpleType
    let ComplexContentResult::Simple(ref sc) = derived.content else {
        return Ok(());
    };

    let Some(ref inline_st) = sc.content_type else {
        return Ok(());
    };

    // Find the base type's effective simple content type
    let Some(base_simple_key) = effective_simple_content_type_key(schema_set, base) else {
        return Ok(());
    };

    // anySimpleType is the ur-type of all simple types — any variety
    // is a valid restriction.  Per §3.14.6 clause 2, inline list/union
    // types automatically derive from anySimpleType.  This function
    // only checks variety compatibility, not facets, so the early
    // return is safe.
    if let TypeKey::Simple(sk) = base_simple_key {
        if sk == schema_set.builtin_types().any_simple_type {
            return Ok(());
        }
    }

    // Get the base simple type's variety
    let base_variety = match base_simple_key {
        TypeKey::Simple(sk) => schema_set.arenas.simple_types.get(sk).map(|st| st.variety),
        TypeKey::Complex(_) => None,
    };

    let Some(base_variety) = base_variety else {
        return Ok(());
    };

    // Check variety compatibility:
    // A list type cannot restrict an atomic type.
    // A union type cannot restrict an atomic type (unless it's a restriction of
    // the base union/atomic via resolved_base_type chain).
    let derived_variety = inline_st.variety;

    if derived_variety != base_variety {
        // Different varieties — check if the inline type's base chain leads to
        // the base simple type (which would mean it's a valid restriction despite
        // variety difference, e.g. restriction of a union member: Type
        // Derivation OK (Simple) clause 2.2.4, "B.{variety} = union … D is
        // validly ·derived· from a type definition M in B's transitive
        // membership", which `is_type_derived_from` applies).
        // For the common case (list restricting atomic, union restricting atomic),
        // this chain walk will NOT find the base type.
        if let Some(inline_resolved_base) = resolve_inline_simple_type_base(schema_set, inline_st) {
            if is_type_derived_from(schema_set, inline_resolved_base, base_simple_key) {
                return Ok(());
            }
        }

        let location = derived
            .source
            .as_ref()
            .and_then(|s| schema_set.source_maps.locate(s));
        let type_name = format_type_name(schema_set, derived.name, derived.target_namespace);
        let base_name = format_type_name(schema_set, base.name, base.target_namespace);
        return Err(SchemaError::structural(
            "derivation-ok-restriction",
            format!(
                "Complex type '{}' restricting '{}': simpleContent inline type \
                 has variety {:?} which is not a valid restriction of the base \
                 type's simple content (variety {:?})",
                type_name, base_name, derived_variety, base_variety,
            ),
            location,
        ));
    }

    Ok(())
}

/// Try to resolve the base type key of an inline SimpleTypeResult.
/// The inline type may have a base_type as a QName that has been resolved,
/// or it may reference a known type directly.
fn resolve_inline_simple_type_base(
    schema_set: &SchemaSet,
    inline_st: &crate::parser::frames::SimpleTypeResult,
) -> Option<TypeKey> {
    // For inline types used in simpleContent/restriction, the base_type
    // is the type the restriction derives from. If it was resolved during
    // assembly, it would be in the arena. We can try to find it by matching
    // the QName if present.
    match &inline_st.base_type {
        // A built-in base (`xs:string`, a member of a union of built-ins) is
        // not among the schema's own types.
        Some(crate::parser::frames::TypeRefResult::QName(qname)) => schema_set
            .lookup_type(qname.namespace, qname.local_name)
            .or_else(|| schema_set.get_built_in_type_by_qname(qname.namespace, qname.local_name)),
        _ => None,
    }
}
