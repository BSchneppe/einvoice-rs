//! Simple-type derivation: variety dispatch, applicable-facet and
//! facet-value checks against the base type, and the restriction / list /
//! union rules (`cos-st-restricts`, `cos-list-of-atomic`,
//! `cos-union-memberTypes`).

use super::{format_type_name, type_error_context, DerivationStats};
use crate::error::{SchemaError, SchemaResult};
use crate::ids::{SimpleTypeKey, TypeKey};
use crate::parser::frames::SimpleTypeVariety;
use crate::parser::location::SourceLocation;
use crate::schema::SchemaSet;
use crate::types::facets::{FacetKind, FacetSet};

/// Validate a simple type definition
pub(super) fn validate_simple_type(
    schema_set: &SchemaSet,
    key: SimpleTypeKey,
    stats: &mut DerivationStats,
) -> SchemaResult<()> {
    let type_def = schema_set
        .arenas
        .simple_types
        .get(key)
        .ok_or_else(|| SchemaError::internal("Simple type not found in arena"))?;

    stats.simple_types_validated += 1;

    // cos-applicable-facets: Check that facets are applicable to the type variety
    validate_applicable_facets(schema_set, type_def)?;

    match type_def.variety {
        SimpleTypeVariety::Atomic => {
            // Atomic types are derived by restriction
            validate_simple_restriction(schema_set, type_def, stats)?;
        }
        SimpleTypeVariety::List => {
            stats.list_types_validated += 1;
            validate_simple_list(schema_set, type_def)?;
            validate_facets_against_resolved_base(schema_set, type_def)?;
        }
        SimpleTypeVariety::Union => {
            stats.union_types_validated += 1;
            validate_simple_union(schema_set, type_def)?;
            validate_facets_against_resolved_base(schema_set, type_def)?;
        }
    }

    Ok(())
}

/// Run `FacetSet::merge_with_base` against the resolved base of a list or
/// union simple type, then validate that local facet values fall in the
/// base type's value space. Atomic types perform both checks inline in
/// `validate_simple_restriction`; list (length/minLength/maxLength/whiteSpace)
/// and union (pattern/enumeration/assertions) varieties share the same
/// {facets} derivation semantics for the facets they are allowed to carry,
/// so the merge must run for them too. A list or union value is not a value
/// of one built-in datatype, so the merge compares no enumeration values:
/// each derived enumeration value is validated against the base type itself
/// instead (`validate_facet_values_against_base_type`), which compares it with
/// the base's enumeration in the value space (Datatypes §4.3.5.5).
fn validate_facets_against_resolved_base(
    schema_set: &SchemaSet,
    type_def: &crate::arenas::SimpleTypeDefData,
) -> SchemaResult<()> {
    let Some(base_key) = type_def.resolved_base_type else {
        return Ok(());
    };
    if let Some(mut base_facets) = get_type_facets(schema_set, base_key)? {
        base_facets.enumeration = None;
        type_def
            .facets
            .merge_with_base(&base_facets, crate::types::XmlTypeCode::AnySimpleType)
            .map_err(|e| {
                let (location, type_name) = type_error_context(schema_set, type_def);
                SchemaError::structural(
                    "cos-st-restricts",
                    format!("Simple type '{}' has invalid restriction: {}", type_name, e),
                    location,
                )
            })?;
    }
    validate_facet_values_against_base_type(schema_set, type_def, base_key)?;
    Ok(())
}

/// Validate cos-applicable-facets: only certain facets are applicable to certain type varieties
///
/// - List types: length, minLength, maxLength, pattern, enumeration, whiteSpace
/// - Union types (XSD 1.0): pattern, enumeration
/// - Union types (XSD 1.1): pattern, enumeration, assertions
fn validate_applicable_facets(
    schema_set: &SchemaSet,
    type_def: &crate::arenas::SimpleTypeDefData,
) -> SchemaResult<()> {
    let facets = &type_def.facets;

    match type_def.variety {
        SimpleTypeVariety::List => {
            // List types: only length, minLength, maxLength, pattern, enumeration, whiteSpace
            let has_inapplicable = facets.min_inclusive.is_some()
                || facets.max_inclusive.is_some()
                || facets.min_exclusive.is_some()
                || facets.max_exclusive.is_some()
                || facets.total_digits.is_some()
                || facets.fraction_digits.is_some()
                || facets.explicit_timezone.is_some();

            if has_inapplicable {
                let (location, type_name) = type_error_context(schema_set, type_def);
                let inapplicable = list_inapplicable_facets_for_list(facets);
                return Err(SchemaError::structural(
                    "cos-applicable-facets",
                    format!(
                        "List type '{}' has inapplicable facet(s): {}",
                        type_name, inapplicable
                    ),
                    location,
                ));
            }
        }
        SimpleTypeVariety::Union => {
            // Union types: only pattern, enumeration (and assertions in XSD 1.1)
            let has_inapplicable = facets.length.is_some()
                || facets.min_length.is_some()
                || facets.max_length.is_some()
                || facets.whitespace.is_some()
                || facets.min_inclusive.is_some()
                || facets.max_inclusive.is_some()
                || facets.min_exclusive.is_some()
                || facets.max_exclusive.is_some()
                || facets.total_digits.is_some()
                || facets.fraction_digits.is_some()
                || facets.explicit_timezone.is_some();

            if has_inapplicable {
                let (location, type_name) = type_error_context(schema_set, type_def);
                let inapplicable = list_inapplicable_facets_for_union(facets);
                return Err(SchemaError::structural(
                    "cos-applicable-facets",
                    format!(
                        "Union type '{}' has inapplicable facet(s): {}",
                        type_name, inapplicable
                    ),
                    location,
                ));
            }
        }
        SimpleTypeVariety::Atomic => {
            // Atomic types: applicability depends on the primitive ancestor
            // (§4.1.5 / Table F.1 of Datatypes). Walk up the base chain to the
            // closest built-in primitive and check each facet kind against it.
            if let Some(primitive_code) = primitive_type_code(schema_set, type_def) {
                use crate::types::facets::{
                    facet_applicable_for_type, ExplicitTimezone, FacetApplicability, FacetKind,
                    WhitespaceMode,
                };
                let facets = &type_def.facets;
                let mut bad: Vec<&'static str> = Vec::new();
                let mut check = |present: bool, kind: FacetKind| {
                    if present
                        && matches!(
                            facet_applicable_for_type(kind, primitive_code),
                            FacetApplicability::NotApplicable
                        )
                    {
                        bad.push(kind.name());
                    }
                };
                check(facets.length.is_some(), FacetKind::Length);
                check(facets.min_length.is_some(), FacetKind::MinLength);
                check(facets.max_length.is_some(), FacetKind::MaxLength);
                check(facets.whitespace.is_some(), FacetKind::Whitespace);
                check(facets.min_inclusive.is_some(), FacetKind::MinInclusive);
                check(facets.max_inclusive.is_some(), FacetKind::MaxInclusive);
                check(facets.min_exclusive.is_some(), FacetKind::MinExclusive);
                check(facets.max_exclusive.is_some(), FacetKind::MaxExclusive);
                check(facets.total_digits.is_some(), FacetKind::TotalDigits);
                check(facets.fraction_digits.is_some(), FacetKind::FractionDigits);
                check(
                    facets.explicit_timezone.is_some(),
                    FacetKind::ExplicitTimezone,
                );
                if let Some(ws) = &facets.whitespace {
                    if !matches!(
                        primitive_code,
                        crate::types::XmlTypeCode::String
                            | crate::types::XmlTypeCode::NormalizedString
                            | crate::types::XmlTypeCode::Token
                            | crate::types::XmlTypeCode::Language
                            | crate::types::XmlTypeCode::NmToken
                            | crate::types::XmlTypeCode::Name
                            | crate::types::XmlTypeCode::NCName
                            | crate::types::XmlTypeCode::Id
                            | crate::types::XmlTypeCode::IdRef
                            | crate::types::XmlTypeCode::Entity
                    ) && ws.value != WhitespaceMode::Collapse
                    {
                        bad.push(FacetKind::Whitespace.name());
                    }
                }
                if let Some(tz) = &facets.explicit_timezone {
                    if primitive_code == crate::types::XmlTypeCode::DateTimeStamp
                        && tz.value != ExplicitTimezone::Required
                    {
                        bad.push(FacetKind::ExplicitTimezone.name());
                    }
                }
                if !bad.is_empty() {
                    let (location, type_name) = type_error_context(schema_set, type_def);
                    return Err(SchemaError::structural(
                        "cos-applicable-facets",
                        format!(
                            "Atomic type '{}' has inapplicable facet(s) for primitive '{}': {}",
                            type_name,
                            primitive_code.local_name().unwrap_or("<unnamed>"),
                            bad.join(", ")
                        ),
                        location,
                    ));
                }
            }
        }
    }

    Ok(())
}

/// Walk a simple type's base chain to the closest built-in with an `XmlTypeCode`.
///
/// Depth is capped at 64 — the XSD primitive hierarchy is shallow and the
/// dependency graph already rejects cycles before this runs, so the bound is
/// purely a defence against a malformed arena state.
fn primitive_type_code(
    schema_set: &SchemaSet,
    type_def: &crate::arenas::SimpleTypeDefData,
) -> Option<crate::types::XmlTypeCode> {
    let builtin = schema_set.builtin_types();
    let mut current_base = type_def.resolved_base_type;
    for _ in 0..64 {
        let Some(TypeKey::Simple(k)) = current_base else {
            return None;
        };
        if let Some(code) = builtin.get_type_code(k) {
            return Some(code);
        }
        current_base = schema_set
            .arenas
            .simple_types
            .get(k)
            .and_then(|t| t.resolved_base_type);
    }
    None
}

/// Emit a `cos-st-restricts`-family error when `simple_key` names
/// `xs:anyAtomicType`, which XSD 1.1 bug 11103 declared abstract — it must
/// not appear as a restriction base, list item type, or union member.
fn reject_any_atomic_type(
    schema_set: &SchemaSet,
    type_def: &crate::arenas::SimpleTypeDefData,
    simple_key: SimpleTypeKey,
    constraint: &'static str,
    role: &'static str,
) -> SchemaResult<()> {
    if !schema_set.builtin_types().is_any_atomic_type(simple_key) {
        return Ok(());
    }
    let (location, type_name) = type_error_context(schema_set, type_def);
    Err(SchemaError::structural(
        constraint,
        format!(
            "Simple type '{}' cannot {} xs:anyAtomicType (abstract per XSD 1.1 bug 11103)",
            type_name, role
        ),
        location,
    ))
}

/// List inapplicable facet names for list types
fn list_inapplicable_facets_for_list(facets: &FacetSet) -> String {
    let mut names = Vec::new();
    if facets.min_inclusive.is_some() {
        names.push("minInclusive");
    }
    if facets.max_inclusive.is_some() {
        names.push("maxInclusive");
    }
    if facets.min_exclusive.is_some() {
        names.push("minExclusive");
    }
    if facets.max_exclusive.is_some() {
        names.push("maxExclusive");
    }
    if facets.total_digits.is_some() {
        names.push("totalDigits");
    }
    if facets.fraction_digits.is_some() {
        names.push("fractionDigits");
    }
    if facets.explicit_timezone.is_some() {
        names.push("explicitTimezone");
    }
    names.join(", ")
}

/// List inapplicable facet names for union types
fn list_inapplicable_facets_for_union(facets: &FacetSet) -> String {
    let mut names = Vec::new();
    if facets.length.is_some() {
        names.push("length");
    }
    if facets.min_length.is_some() {
        names.push("minLength");
    }
    if facets.max_length.is_some() {
        names.push("maxLength");
    }
    if facets.whitespace.is_some() {
        names.push("whiteSpace");
    }
    if facets.min_inclusive.is_some() {
        names.push("minInclusive");
    }
    if facets.max_inclusive.is_some() {
        names.push("maxInclusive");
    }
    if facets.min_exclusive.is_some() {
        names.push("minExclusive");
    }
    if facets.max_exclusive.is_some() {
        names.push("maxExclusive");
    }
    if facets.total_digits.is_some() {
        names.push("totalDigits");
    }
    if facets.fraction_digits.is_some() {
        names.push("fractionDigits");
    }
    if facets.explicit_timezone.is_some() {
        names.push("explicitTimezone");
    }
    names.join(", ")
}

/// Validate simple type restriction derivation
///
/// Constraint: cos-st-restricts (Derivation Valid - Restriction, Simple)
fn validate_simple_restriction(
    schema_set: &SchemaSet,
    type_def: &crate::arenas::SimpleTypeDefData,
    stats: &mut DerivationStats,
) -> SchemaResult<()> {
    // If no base type, this is a primitive type or xs:anySimpleType derivation
    let base_key = match type_def.resolved_base_type {
        Some(key) => key,
        None => return Ok(()), // No base type to validate against
    };

    // cos-st-restricts.1.1: base type must be a simple type definition
    if let TypeKey::Complex(_) = base_key {
        let (location, type_name) = type_error_context(schema_set, type_def);
        return Err(SchemaError::structural(
            "cos-st-restricts",
            format!("Simple type '{}': base type must be a simple type definition (cos-st-restricts.1.1)", type_name),
            location,
        ));
    }

    if let TypeKey::Simple(base_simple_key) = base_key {
        reject_any_atomic_type(
            schema_set,
            type_def,
            base_simple_key,
            "cos-st-restricts",
            "restrict",
        )?;

        // cos-st-restricts.1.1: an atomic restriction's {base type definition}
        // must be an atomic simple type or a built-in primitive.
        // xs:anySimpleType has no {variety}, so a user-defined
        // <xs:restriction base="xs:anySimpleType"/> is invalid (msData
        // addB110, stZ005/stZ006/stZ011). Built-in primitives themselves
        // never reach this check — they carry no resolved base.
        if base_simple_key == schema_set.builtin_types().any_simple_type {
            let (location, type_name) = type_error_context(schema_set, type_def);
            return Err(SchemaError::structural(
                "cos-st-restricts",
                format!(
                    "Simple type '{}' cannot restrict xs:anySimpleType directly \
                     (cos-st-restricts.1.1: base must be atomic or a built-in primitive)",
                    type_name
                ),
                location,
            ));
        }
    }

    stats.restrictions_validated += 1;

    // Check that base type is not final for restriction
    if let TypeKey::Simple(base_simple_key) = base_key {
        if let Some(base_type) = schema_set.arenas.simple_types.get(base_simple_key) {
            if base_type.final_derivation.contains_restriction() {
                let (location, type_name) = type_error_context(schema_set, type_def);
                let base_name =
                    format_type_name(schema_set, base_type.name, base_type.target_namespace);
                return Err(SchemaError::structural(
                    "cos-st-restricts",
                    format!(
                        "Simple type '{}' cannot restrict '{}' because base type is final for restriction",
                        type_name, base_name
                    ),
                    location,
                ));
            }
        }
    }

    // Get base type facets
    let base_facets = get_type_facets(schema_set, base_key)?;

    // An `xs:QName` / `xs:NOTATION` enumeration is a subset of its base's in
    // value space, by expanded name — `validate_facet_values_against_base_type`
    // checks it; `merge_with_base` compares lexical forms.
    let base_facets = base_facets.map(|mut facets| {
        if is_notation_or_qname_base(schema_set, base_key) {
            facets.enumeration = None;
        }
        facets
    });

    // Validate that derived facets are more restrictive
    if let Some(ref base_facets) = base_facets {
        // FacetSet.merge_with_base validates derivation rules; the facet
        // values are compared in the value space of the type's datatype.
        let datatype = primitive_type_code(schema_set, type_def)
            .unwrap_or(crate::types::XmlTypeCode::AnySimpleType);
        type_def
            .facets
            .merge_with_base(base_facets, datatype)
            .map_err(|e| {
                let (location, type_name) = type_error_context(schema_set, type_def);
                SchemaError::structural(
                    "cos-st-restricts",
                    format!("Simple type '{}' has invalid restriction: {}", type_name, e),
                    location,
                )
            })?;
    }

    // Validate that facet values are in the base type's value space
    // (e.g., enumeration values must be valid for xs:float when base is xs:float)
    validate_facet_values_against_base_type(schema_set, type_def, base_key)?;

    Ok(())
}

/// Validate simple type list derivation
///
/// Constraint: cos-list-of-atomic (List item type must be atomic)
fn validate_simple_list(
    schema_set: &SchemaSet,
    type_def: &crate::arenas::SimpleTypeDefData,
) -> SchemaResult<()> {
    // Check that the item type is not final for list derivation
    if let Some(TypeKey::Simple(item_simple_key)) = type_def.resolved_item_type {
        if let Some(item_type) = schema_set.arenas.simple_types.get(item_simple_key) {
            if item_type.final_derivation.contains_list() {
                let (location, type_name) = type_error_context(schema_set, type_def);
                let item_name =
                    format_type_name(schema_set, item_type.name, item_type.target_namespace);
                return Err(SchemaError::structural(
                    "cos-st-restricts",
                    format!(
                        "List type '{}' cannot use '{}' as item type because it is final for list",
                        type_name, item_name
                    ),
                    location,
                ));
            }
        }
    }

    // Also check the base type's final for restriction (list types restrict xs:anySimpleType)
    if let Some(TypeKey::Simple(base_simple_key)) = type_def.resolved_base_type {
        if let Some(base_type) = schema_set.arenas.simple_types.get(base_simple_key) {
            if base_type.final_derivation.contains_list() {
                let (location, type_name) = type_error_context(schema_set, type_def);
                let base_name =
                    format_type_name(schema_set, base_type.name, base_type.target_namespace);
                return Err(SchemaError::structural(
                    "cos-st-restricts",
                    format!(
                        "List type '{}' cannot derive from '{}' because it is final for list",
                        type_name, base_name
                    ),
                    location,
                ));
            }
        }
    }

    // Get the item type
    let item_key = match type_def.resolved_item_type {
        Some(key) => key,
        None => {
            // No resolved item type - might be inline or error
            return Ok(());
        }
    };

    // Item type must be atomic (not a list, not a union containing lists)
    match item_key {
        TypeKey::Simple(simple_key) => {
            reject_any_atomic_type(
                schema_set,
                type_def,
                simple_key,
                "cos-list-of-atomic",
                "use as list item type",
            )?;
            if let Some(item_type) = schema_set.arenas.simple_types.get(simple_key) {
                match item_type.variety {
                    SimpleTypeVariety::Atomic => {
                        // Valid - atomic types are OK
                    }
                    SimpleTypeVariety::List => {
                        // Invalid - list of list is not allowed
                        let (location, type_name) = type_error_context(schema_set, type_def);
                        return Err(SchemaError::structural(
                            "cos-list-of-atomic",
                            format!(
                                "List type '{}' has list item type, which is not allowed",
                                type_name
                            ),
                            location,
                        ));
                    }
                    SimpleTypeVariety::Union => {
                        // Must check that union doesn't contain list members
                        if union_contains_list(schema_set, item_type) {
                            let (location, type_name) = type_error_context(schema_set, type_def);
                            return Err(SchemaError::structural(
                                "cos-list-of-atomic",
                                format!(
                                    "List type '{}' has union item type containing list member",
                                    type_name
                                ),
                                location,
                            ));
                        }
                    }
                }
            }
        }
        TypeKey::Complex(_) => {
            // Complex types cannot be list item types
            let (location, type_name) = type_error_context(schema_set, type_def);
            return Err(SchemaError::structural(
                "cos-list-of-atomic",
                format!(
                    "List type '{}' has complex item type, which is not allowed",
                    type_name
                ),
                location,
            ));
        }
    }

    Ok(())
}

/// Check if a union type (or nested unions) contains any list members
fn union_contains_list(
    schema_set: &SchemaSet,
    union_type: &crate::arenas::SimpleTypeDefData,
) -> bool {
    for member_key in &union_type.resolved_member_types {
        if let TypeKey::Simple(simple_key) = member_key {
            if let Some(member) = schema_set.arenas.simple_types.get(*simple_key) {
                match member.variety {
                    SimpleTypeVariety::List => return true,
                    SimpleTypeVariety::Union => {
                        if union_contains_list(schema_set, member) {
                            return true;
                        }
                    }
                    SimpleTypeVariety::Atomic => {}
                }
            }
        }
    }
    false
}

/// Validate simple type union derivation
///
/// Constraint: cos-union-memberTypes (Union member types must be simple types)
fn validate_simple_union(
    schema_set: &SchemaSet,
    type_def: &crate::arenas::SimpleTypeDefData,
) -> SchemaResult<()> {
    // Check that member types are not final for union derivation
    for member_key in &type_def.resolved_member_types {
        if let TypeKey::Simple(simple_key) = member_key {
            if let Some(member_type) = schema_set.arenas.simple_types.get(*simple_key) {
                if member_type.final_derivation.contains_union() {
                    let (location, type_name) = type_error_context(schema_set, type_def);
                    let member_name = format_type_name(
                        schema_set,
                        member_type.name,
                        member_type.target_namespace,
                    );
                    return Err(SchemaError::structural(
                        "cos-st-restricts",
                        format!(
                            "Union type '{}' cannot use '{}' as member type because it is final for union",
                            type_name, member_name
                        ),
                        location,
                    ));
                }
            }
        }
    }

    // All member types must be simple types
    for member_key in &type_def.resolved_member_types {
        match member_key {
            TypeKey::Simple(simple_key) => {
                reject_any_atomic_type(
                    schema_set,
                    type_def,
                    *simple_key,
                    "cos-union-memberTypes",
                    "use as union member type",
                )?;
            }
            TypeKey::Complex(_) => {
                // Invalid - complex types cannot be union members
                let (location, type_name) = type_error_context(schema_set, type_def);
                return Err(SchemaError::structural(
                    "cos-union-memberTypes",
                    format!(
                        "Union type '{}' has complex member type, which is not allowed",
                        type_name
                    ),
                    location,
                ));
            }
        }
    }

    Ok(())
}

/// Check if a type derives from NOTATION or QName.
fn is_notation_or_qname_base(schema_set: &SchemaSet, key: TypeKey) -> bool {
    let TypeKey::Simple(sk) = key else {
        return false;
    };
    let bt = schema_set.builtin_types();
    schema_set.derives_from(sk, bt.notation) || schema_set.derives_from(sk, bt.qname)
}

/// The values of an enumeration facet in a fixed order (sorted), so that the
/// value an error names does not depend on hash order. The facet's {value} is
/// a set (Datatypes 1.1 §4.3.5.1), so no order is implied; sorted is the
/// deterministic one the stored set gives.
fn sorted_values(enumeration: &crate::types::facets::EnumerationFacet) -> Vec<&String> {
    let mut values: Vec<&String> = enumeration.values.iter().collect();
    values.sort_unstable();
    values
}

/// Validate that facet values are in the value space of the base type.
///
/// Reuses the existing `validate_simple_type` runtime infrastructure (type code
/// resolution, facet collection, validator dispatch) to check each locally
/// declared facet value against the base type at schema-compile time.
///
/// Implements XSD Part 2 constraints:
/// - `enumeration-valid-restriction`: enumeration values must be in the base type's value space
/// - `minInclusive-valid-restriction`, `maxInclusive-valid-restriction`,
///   `minExclusive-valid-restriction`, `maxExclusive-valid-restriction`:
///   bound values must be in the base type's value space
fn validate_facet_values_against_base_type(
    schema_set: &SchemaSet,
    type_def: &crate::arenas::SimpleTypeDefData,
    base_key: TypeKey,
) -> SchemaResult<()> {
    let (location, type_name) = type_error_context(schema_set, type_def);

    // QName/NOTATION need a runtime namespace context for full value-space
    // validation, so most lexical checks are deferred. We *can* still reject
    // the always-invalid empty literal in enumeration / bound facets — an
    // empty string is not a valid QName or NOTATION lexically (xsd:QName ≡
    // Prefix? ':'? LocalPart, where LocalPart is an NCName, never empty).
    if is_notation_or_qname_base(schema_set, base_key) {
        if let Some(ref enum_facet) = type_def.facets.enumeration {
            for value in &enum_facet.values {
                if value.trim().is_empty() {
                    return Err(SchemaError::structural(
                        "enumeration-valid-restriction",
                        format!(
                            "Enumeration value '' in type '{}' is not in the value space of the base type",
                            type_name
                        ),
                        location.clone(),
                    ));
                }
            }
            // Datatypes §4.3.5.5: the base's value space is its enumeration,
            // the one it inherits through restrictions without their own too
            // (`merge_with_base` sees only the immediate base's own facets).
            // Compared by expanded name — each literal read with the namespace
            // bindings it was written with — as the facet check compares QName
            // and NOTATION values (Datatypes §3.3.18).
            if let TypeKey::Simple(base_sk) = base_key {
                let base_facets = schema_set.effective_facets(base_sk);
                if let (Some(base_enum), Some(base_index)) = (
                    base_facets.enumeration.as_ref(),
                    schema_set.enumeration_index(base_sk),
                ) {
                    if let Some(value) = sorted_values(enum_facet).into_iter().find(|value| {
                        !crate::validation::simple::qname_literal_enumerated(
                            enum_facet,
                            value,
                            base_enum,
                            &base_index,
                            schema_set.derives_from(base_sk, schema_set.builtin_types().notation),
                            &schema_set.name_table,
                        )
                    }) {
                        return Err(SchemaError::structural(
                            "enumeration-valid-restriction",
                            format!(
                                "Enumeration value '{}' in type '{}' is not in the base type's enumeration",
                                value, type_name
                            ),
                            location.clone(),
                        ));
                    }
                }
            }
        }
        return Ok(());
    }

    // XSD 1.0 strict anyURI lexical rules for enumeration facet values
    // (msData anyURI_a003/b004/b006: bad scheme, bare `%`, `^`, `\`).
    // XSD 1.1 dropped the RFC 2396 tie, so this is version-gated; instance
    // values keep the permissive path.
    if schema_set.is_xsd10() {
        if let (TypeKey::Simple(base_sk), Some(enum_facet)) =
            (base_key, type_def.facets.enumeration.as_ref())
        {
            if schema_set.derives_from(base_sk, schema_set.builtin_types().any_uri) {
                for value in sorted_values(enum_facet) {
                    if !crate::types::validators::is_strict_xsd10_anyuri_enum_value(value) {
                        return Err(SchemaError::structural(
                            "enumeration-valid-restriction",
                            format!(
                                "Enumeration value '{}' in type '{}' is not a valid xs:anyURI \
                                 (XSD 1.0 / RFC 2396 lexical rules)",
                                value, type_name
                            ),
                            location.clone(),
                        ));
                    }
                }
            }
        }
    }

    // Validate enumeration values: §4.3.5.5 enumeration valid restriction, "It
    // is an ·error· if any member of {value} is not in the ·value space· of
    // {base type definition}" — the base's facets, its enumeration included
    // (compared by value, also for a list or a union base).
    if let Some(ref enum_facet) = type_def.facets.enumeration {
        for value in sorted_values(enum_facet) {
            if crate::validation::simple::validate_simple_type(value, base_key, schema_set).is_err()
            {
                return Err(SchemaError::structural(
                    "enumeration-valid-restriction",
                    format!(
                        "Enumeration value '{}' in type '{}' is not in the value space of the base type",
                        value, type_name
                    ),
                    location.clone(),
                ));
            }
        }
    }

    // Validate bound facet values. §4.3.8.1 / §4.3.9.1 permit a derived
    // exclusive bound to equal the base's of the same kind (boundary
    // equality), even though that value is not in the base's value space.
    let check_bound =
        |value: &str, constraint: &'static str, kind: FacetKind| -> SchemaResult<()> {
            match crate::validation::simple::validate_simple_type(value, base_key, schema_set) {
                Ok(_) => Ok(()),
                Err(_) if is_bound_self_violation(kind, schema_set, base_key, value) => Ok(()),
                Err(_) => Err(SchemaError::structural(
                    constraint,
                    format!(
                        "{} value '{}' in type '{}' is not in the value space of the base type",
                        kind.name(),
                        value,
                        type_name
                    ),
                    location.clone(),
                )),
            }
        };

    if let Some(ref f) = type_def.facets.min_inclusive {
        check_bound(
            &f.value,
            "minInclusive-valid-restriction",
            FacetKind::MinInclusive,
        )?;
    }
    if let Some(ref f) = type_def.facets.max_inclusive {
        check_bound(
            &f.value,
            "maxInclusive-valid-restriction",
            FacetKind::MaxInclusive,
        )?;
    }
    if let Some(ref f) = type_def.facets.min_exclusive {
        check_bound(
            &f.value,
            "minExclusive-valid-restriction",
            FacetKind::MinExclusive,
        )?;
    }
    if let Some(ref f) = type_def.facets.max_exclusive {
        check_bound(
            &f.value,
            "maxExclusive-valid-restriction",
            FacetKind::MaxExclusive,
        )?;
    }

    validate_typed_bound_consistency(
        schema_set,
        &type_def.facets,
        base_key,
        &type_name,
        &location,
    )?;

    Ok(())
}

fn validate_typed_bound_consistency(
    schema_set: &SchemaSet,
    facets: &FacetSet,
    base_key: TypeKey,
    type_name: &str,
    location: &Option<SourceLocation>,
) -> SchemaResult<()> {
    let check_pair = |lower: Option<&str>,
                      upper: Option<&str>,
                      lower_name: &'static str,
                      upper_name: &'static str,
                      allow_equal: bool|
     -> SchemaResult<()> {
        let (Some(lower), Some(upper)) = (lower, upper) else {
            return Ok(());
        };
        let Some(cmp) = compare_bound_literals(schema_set, base_key, lower, upper) else {
            return Ok(());
        };
        let valid = if allow_equal {
            matches!(cmp, std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
        } else {
            cmp == std::cmp::Ordering::Less
        };
        if valid {
            return Ok(());
        }
        Err(SchemaError::structural(
            "cos-st-restricts",
            format!(
                "{} value '{}' is not below {} value '{}' in type '{}'",
                lower_name, lower, upper_name, upper, type_name
            ),
            location.clone(),
        ))
    };

    check_pair(
        facets.min_inclusive.as_ref().map(|f| f.value.as_str()),
        facets.max_inclusive.as_ref().map(|f| f.value.as_str()),
        "minInclusive",
        "maxInclusive",
        true,
    )?;
    // §4.3.8.4 "minExclusive <= maxExclusive": only greater is an error.
    check_pair(
        facets.min_exclusive.as_ref().map(|f| f.value.as_str()),
        facets.max_exclusive.as_ref().map(|f| f.value.as_str()),
        "minExclusive",
        "maxExclusive",
        true,
    )?;
    check_pair(
        facets.min_inclusive.as_ref().map(|f| f.value.as_str()),
        facets.max_exclusive.as_ref().map(|f| f.value.as_str()),
        "minInclusive",
        "maxExclusive",
        false,
    )?;
    check_pair(
        facets.min_exclusive.as_ref().map(|f| f.value.as_str()),
        facets.max_inclusive.as_ref().map(|f| f.value.as_str()),
        "minExclusive",
        "maxInclusive",
        false,
    )
}

fn compare_bound_literals(
    schema_set: &SchemaSet,
    base_key: TypeKey,
    lower: &str,
    upper: &str,
) -> Option<std::cmp::Ordering> {
    let parse_base = bound_comparison_base(schema_set, base_key);
    let lower_value =
        crate::validation::simple::validate_simple_type(lower, parse_base, schema_set).ok()?;
    let upper_value =
        crate::validation::simple::validate_simple_type(upper, parse_base, schema_set).ok()?;
    compare_xml_values(
        schema_set.is_xsd10(),
        (&lower_value.typed_value, lower),
        (&upper_value.typed_value, upper),
    )
}

fn bound_comparison_base(schema_set: &SchemaSet, key: TypeKey) -> TypeKey {
    let mut current = key;
    for _ in 0..100 {
        let TypeKey::Simple(sk) = current else {
            return current;
        };
        let Some(st) = schema_set.arenas.simple_types.get(sk) else {
            return current;
        };
        let has_bounds_or_enum = st.facets.enumeration.is_some()
            || st.facets.min_inclusive.is_some()
            || st.facets.min_exclusive.is_some()
            || st.facets.max_inclusive.is_some()
            || st.facets.max_exclusive.is_some();
        if !has_bounds_or_enum {
            return current;
        }
        let Some(base) = st.resolved_base_type else {
            return current;
        };
        current = base;
    }
    current
}

/// The values with the literals they were read from (see
/// [`compare_xml_atomic_values`]).
fn compare_xml_values(
    xsd10: bool,
    lower: (&crate::types::value::XmlValue, &str),
    upper: (&crate::types::value::XmlValue, &str),
) -> Option<std::cmp::Ordering> {
    use crate::types::value::XmlValueKind;
    match (&lower.0.value, &upper.0.value) {
        (XmlValueKind::Atomic(a), XmlValueKind::Atomic(b)) => {
            compare_xml_atomic_values(xsd10, (a, lower.1), (b, upper.1))
        }
        (XmlValueKind::Union(a), _) => compare_xml_values(xsd10, (a, lower.1), upper),
        (_, XmlValueKind::Union(b)) => compare_xml_values(xsd10, lower, (b, upper.1)),
        _ => None,
    }
}

/// The order of two bound values in the datatype's order relation
/// (`types::value_order`): the date/time datatypes on the time line, partially
/// when one value has a time zone offset and the other has none (Datatypes 1.1
/// §D.2.1; XSD 1.0 §3.2.7.4), durations by the four reference dateTimes
/// (§3.3.6.1; XSD 1.0 §3.2.6.2). `None` for ·incomparable· values, which are
/// not out of order, and for the other datatypes (their bounds are ordered by
/// `FacetSet::merge_with_base`). The years are numbered as the schema's XSD
/// version numbers them (XSD 1.0 has no year 0000), and seconds with more
/// fraction digits than the typed value holds are compared as written in the
/// literals (`value_order::literal_cmp_in`).
fn compare_xml_atomic_values(
    xsd10: bool,
    lower: (&crate::types::value::XmlAtomicValue, &str),
    upper: (&crate::types::value::XmlAtomicValue, &str),
) -> Option<std::cmp::Ordering> {
    crate::types::value_order::literal_cmp_in(xsd10, lower.0, lower.1, upper.0, upper.1)
}

/// The {facets} of a simple base type: its own and every facet it inherits
/// (Structures §3.16.6.4: "The {facets} of R are the {facets} of B
/// ·overlaid· with S"), so a restriction is checked against a `fixed` facet,
/// a length or a digit facet of any ancestor, not only of its immediate base
/// (Datatypes §4.3: "{facets} of {base type definition}"). The `enumeration`
/// is the base's own: an enumeration inherited from further up is checked by
/// validating each value against the base type
/// (`validate_facet_values_against_base_type`, enumeration-valid-restriction),
/// as before.
fn get_type_facets(schema_set: &SchemaSet, type_key: TypeKey) -> SchemaResult<Option<FacetSet>> {
    match type_key {
        TypeKey::Simple(key) => {
            if let Some(type_def) = schema_set.arenas.simple_types.get(key) {
                let mut facets = schema_set.effective_facets(key).as_ref().clone();
                facets.enumeration = type_def.facets.enumeration.clone();
                Ok(Some(facets))
            } else {
                Ok(None)
            }
        }
        TypeKey::Complex(_) => {
            // Complex types don't have direct facets
            // (simpleContent types have facets in their content definition)
            Ok(None)
        }
    }
}

/// Whether a bound literal that is not a value of the base type is still
/// allowed: Datatypes 1.1 §4.3.8.1 (maxExclusive; §4.3.9.1 minExclusive
/// alike, XSD 1.0 §4.3.8 / §4.3.9 too) — its value "must be equal to some
/// value in the ·value space· of the ·base type· or be equal to {value} in
/// {base type definition}". The base's {facets} hold the bound of the same
/// kind whether it was set on the base or inherited from further up, and the
/// alternative is unconditional; the inclusive bounds have none.
fn is_bound_self_violation(
    kind: FacetKind,
    schema_set: &SchemaSet,
    base_key: TypeKey,
    value: &str,
) -> bool {
    let Some(content) = content_facets(schema_set, base_key, 0) else {
        return false;
    };
    let base_bound = match kind {
        FacetKind::MaxExclusive => content.facets.max_exclusive.as_ref().map(|f| &f.value),
        FacetKind::MinExclusive => content.facets.min_exclusive.as_ref().map(|f| &f.value),
        _ => return false,
    };
    base_bound.is_some_and(|bound| {
        crate::types::facets::literal_values_equal(content.datatype, value, bound)
    })
}

/// The facets of a complex type's `<simpleContent><restriction>` restrict a
/// simple type definition (Structures 1.1 §3.4.2.2: the content type is "a
/// simple type definition which restricts the simple type definition
/// identified in clause 4.1 (or clause 4.2) with a set of facet components
/// corresponding to the appropriate element information items among the
/// <restriction>'s [children] …, as defined in Simple Type Restriction
/// (Facets) (§3.16.6.4)") — the inline `<simpleType>` when there is one,
/// otherwise the base's content type. So the Datatypes constraints on
/// restricted facets hold against it, as for a simple type: `fixed` facets,
/// lengths, digits and bounds of every step of the base's chain
/// (`FacetSet::merge_with_base`, `cos-st-restricts`), each enumeration value in
/// the base's value space (§4.3.5.5 enumeration valid restriction) and each
/// bound value in it (§4.3.7.2 – §4.3.10.2: "{value} must be equal to some
/// value in the ·value space· of {base type definition}"; an exclusive bound
/// may equal the base's of the same kind), and the bounds in order in the
/// value space of date/time and duration types.
pub(super) fn validate_simple_content_restriction_facets(
    schema_set: &SchemaSet,
    type_def: &crate::arenas::ComplexTypeDefData,
    base_key: crate::ids::ComplexTypeKey,
) -> SchemaResult<()> {
    use crate::parser::frames::{ComplexContentResult, DerivationMethod};
    let ComplexContentResult::Simple(sc) = &type_def.content else {
        return Ok(());
    };
    if sc.derivation != DerivationMethod::Restriction || sc.facets.is_empty() {
        return Ok(());
    }
    let facet_base = type_def
        .resolved_simple_content_type
        .unwrap_or(TypeKey::Complex(base_key));
    let Some(ContentFacets {
        mut facets,
        datatype,
        atomic,
        variety,
    }) = content_facets(schema_set, facet_base, 0)
    else {
        return Ok(());
    };
    // cos-applicable-facets: the facets restrict the content's simple type,
    // so they must be applicable to its variety and datatype (Datatypes 1.1
    // §4.1.5; a plain `<simpleType>` restriction is checked the same way in
    // `validate_applicable_facets`).
    let inapplicable = inapplicable_facets(&sc.facets, variety, datatype);
    if !inapplicable.is_empty() {
        let (location, type_name) = type_error_context(schema_set, type_def);
        return Err(SchemaError::structural(
            "cos-applicable-facets",
            format!(
                "Complex type '{}': facet(s) {} are not applicable to its simple content type",
                type_name,
                inapplicable.join(", ")
            ),
            location,
        ));
    }
    // QName / NOTATION values need the schema's namespace bindings; they keep
    // the check they had (the instance comparison).
    if crate::validation::simple::type_has_qname_values(facet_base, schema_set) {
        return Ok(());
    }
    let (location, type_name) = type_error_context(schema_set, type_def);
    // A list or union value is not a value of one built-in datatype: its
    // enumeration values are checked against the base itself, below.
    if !atomic {
        facets.enumeration = None;
    }
    sc.facets.merge_with_base(&facets, datatype).map_err(|e| {
        SchemaError::structural(
            "cos-st-restricts",
            format!(
                "Complex type '{}' has an invalid simple content restriction: {}",
                type_name, e
            ),
            location.clone(),
        )
    })?;

    let in_base = |value: &str| {
        crate::validation::simple::validate_simple_type(value, facet_base, schema_set)
    };
    if let Some(enumeration) = &sc.facets.enumeration {
        if let Some(value) = sorted_values(enumeration)
            .into_iter()
            .find(|value| in_base(value).is_err())
        {
            return Err(SchemaError::structural(
                "enumeration-valid-restriction",
                format!(
                    "Enumeration value '{}' in type '{}' is not in the value space of the base content type",
                    value, type_name
                ),
                location,
            ));
        }
    }
    let bounds = [
        (
            sc.facets.min_inclusive.as_ref().map(|f| &f.value),
            FacetKind::MinInclusive,
            None,
        ),
        (
            sc.facets.max_inclusive.as_ref().map(|f| &f.value),
            FacetKind::MaxInclusive,
            None,
        ),
        (
            sc.facets.min_exclusive.as_ref().map(|f| &f.value),
            FacetKind::MinExclusive,
            facets.min_exclusive.as_ref().map(|f| &f.value),
        ),
        (
            sc.facets.max_exclusive.as_ref().map(|f| &f.value),
            FacetKind::MaxExclusive,
            facets.max_exclusive.as_ref().map(|f| &f.value),
        ),
    ];
    for (value, kind, base_bound) in bounds {
        let Some(value) = value else { continue };
        if in_base(value).is_ok() {
            continue;
        }
        // An exclusive bound equal to the base's of the same kind is allowed
        // whatever else the base's facets say (§4.3.8.1, §4.3.9.1: "or be
        // equal to {value} in {base type definition}").
        let at_base_bound = base_bound.is_some_and(|bound| {
            crate::types::facets::literal_values_equal(datatype, value, bound)
        });
        if !at_base_bound {
            return Err(SchemaError::structural(
                match kind {
                    FacetKind::MinInclusive => "minInclusive-valid-restriction",
                    FacetKind::MaxInclusive => "maxInclusive-valid-restriction",
                    FacetKind::MinExclusive => "minExclusive-valid-restriction",
                    _ => "maxExclusive-valid-restriction",
                },
                format!(
                    "{} value '{}' in type '{}' is not in the value space of the base content type",
                    kind.name(),
                    value,
                    type_name
                ),
                location,
            ));
        }
    }
    validate_typed_bound_consistency(schema_set, &sc.facets, facet_base, &type_name, &location)
}

/// The facets of `facets` that do not apply to a simple type of `variety`
/// whose nearest built-in type is `datatype` (Datatypes 1.1 §4.1.5): for a
/// list the length facets, `pattern`, `enumeration`, `whiteSpace` (and
/// assertions); for a union `pattern` and `enumeration` (and assertions); for
/// an atomic type the facets its primitive takes.
fn inapplicable_facets(
    facets: &FacetSet,
    variety: SimpleTypeVariety,
    datatype: crate::types::XmlTypeCode,
) -> Vec<&'static str> {
    use crate::types::facets::{facet_applicable_for_type, FacetApplicability};
    let present = [
        (facets.length.is_some(), FacetKind::Length),
        (facets.min_length.is_some(), FacetKind::MinLength),
        (facets.max_length.is_some(), FacetKind::MaxLength),
        (facets.whitespace.is_some(), FacetKind::Whitespace),
        (facets.min_inclusive.is_some(), FacetKind::MinInclusive),
        (facets.max_inclusive.is_some(), FacetKind::MaxInclusive),
        (facets.min_exclusive.is_some(), FacetKind::MinExclusive),
        (facets.max_exclusive.is_some(), FacetKind::MaxExclusive),
        (facets.total_digits.is_some(), FacetKind::TotalDigits),
        (facets.fraction_digits.is_some(), FacetKind::FractionDigits),
        (
            facets.explicit_timezone.is_some(),
            FacetKind::ExplicitTimezone,
        ),
    ];
    present
        .into_iter()
        .filter(|(here, _)| *here)
        .filter(|(_, kind)| match variety {
            SimpleTypeVariety::List => !matches!(
                kind,
                FacetKind::Length
                    | FacetKind::MinLength
                    | FacetKind::MaxLength
                    | FacetKind::Whitespace
            ),
            SimpleTypeVariety::Union => true,
            SimpleTypeVariety::Atomic => {
                datatype != crate::types::XmlTypeCode::AnySimpleType
                    && matches!(
                        facet_applicable_for_type(*kind, datatype),
                        FacetApplicability::NotApplicable
                    )
            }
        })
        .map(|(_, kind)| kind.name())
        .collect()
}

/// The facets a simple content type has — its own and its chain's, as
/// `effective_facets` composes a simple type's — with the built-in datatype
/// its values are in, and whether that datatype is atomic.
struct ContentFacets {
    facets: FacetSet,
    datatype: crate::types::XmlTypeCode,
    atomic: bool,
    variety: SimpleTypeVariety,
}

/// [`ContentFacets`] of the simple type or the complex type's content type
/// `key`: a complex type's simple-content restriction contributes its own
/// facets on top of those of its inline `<simpleType>`, or of its base's
/// content type (the order `validate_simple_type` validates in).
fn content_facets(schema_set: &SchemaSet, key: TypeKey, depth: u32) -> Option<ContentFacets> {
    use crate::parser::frames::{ComplexContentResult, DerivationMethod};
    if depth > 64 {
        return None;
    }
    match key {
        TypeKey::Simple(sk) => {
            let st = schema_set.arenas.simple_types.get(sk)?;
            let builtin = schema_set.builtin_types();
            let mut datatype = None;
            let mut current = Some(TypeKey::Simple(sk));
            for _ in 0..64 {
                let Some(TypeKey::Simple(k)) = current else {
                    break;
                };
                if let Some(code) = builtin.get_type_code(k) {
                    datatype = Some(code);
                    break;
                }
                current = schema_set
                    .arenas
                    .simple_types
                    .get(k)
                    .and_then(|t| t.resolved_base_type);
            }
            let atomic = st.variety == SimpleTypeVariety::Atomic;
            Some(ContentFacets {
                facets: schema_set.effective_facets(sk).as_ref().clone(),
                datatype: datatype
                    .filter(|_| atomic)
                    .unwrap_or(crate::types::XmlTypeCode::AnySimpleType),
                atomic,
                variety: st.variety,
            })
        }
        TypeKey::Complex(ck) => {
            let ct = schema_set.arenas.complex_types.get(ck)?;
            let next = ct.resolved_simple_content_type.or(ct.resolved_base_type)?;
            if next == key {
                return None;
            }
            let mut content = content_facets(schema_set, next, depth + 1)?;
            if let ComplexContentResult::Simple(sc) = &ct.content {
                if sc.derivation == DerivationMethod::Restriction {
                    let mut own = sc.facets.clone();
                    own.inherit_from(&content.facets);
                    content.facets = own;
                }
            }
            Some(content)
        }
    }
}
