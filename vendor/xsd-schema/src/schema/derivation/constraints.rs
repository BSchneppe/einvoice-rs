//! Schema-wide constraint checks that run after reference resolution:
//! attribute/element value constraints, attribute-use uniqueness, xsi
//! attribute declarations, local declaration target namespaces and
//! substitution-group element consistency.

use super::attributes::{resolve_single_attribute_use, EffectiveAttributeUse};
use super::complex::effective_simple_content_type_key;
use super::format_type_name;
use crate::error::{SchemaError, SchemaResult};
use crate::ids::{AttributeGroupKey, AttributeKey, ComplexTypeKey, ElementKey, NameId, TypeKey};
use crate::parser::frames::{AttributeUseKind, DerivationMethod};
use crate::parser::location::SourceRef;
use crate::schema::SchemaSet;

/// XSD 1.0 §3.2.6 constraint 2 (`cos-attribute-decl`): if an attribute's type
/// is or derives from xs:ID, it must not have a value constraint (default or
/// fixed). XSD 1.1 relaxes this restriction. Called from the pipeline after
/// reference resolution.
pub fn validate_attribute_id_constraints(schema_set: &SchemaSet) -> SchemaResult<()> {
    use crate::types::XmlTypeCode;

    if !schema_set.is_xsd10() {
        return Ok(());
    }

    let id_key = match schema_set.builtin_types().get_by_type_code(XmlTypeCode::Id) {
        Some(k) => k,
        None => return Ok(()),
    };

    for (_key, attr_data) in schema_set.arenas.attributes.iter() {
        if attr_data.default_value.is_none() && attr_data.fixed_value.is_none() {
            continue;
        }
        if let Some(TypeKey::Simple(st_key)) = attr_data.resolved_type {
            if schema_set.derives_from(st_key, id_key) {
                let attr_name = attr_data
                    .name
                    .map(|n| schema_set.name_table.resolve(n).to_string())
                    .unwrap_or_else(|| "(anonymous)".to_string());
                let constraint = if attr_data.default_value.is_some() {
                    "default"
                } else {
                    "fixed"
                };
                return Err(SchemaError::structural(
                    "cos-attribute-decl",
                    format!(
                        "Attribute '{}' has type xs:ID (or derived) and must not have a {} value constraint",
                        attr_name, constraint
                    ),
                    schema_set.locate(attr_data.source.as_ref()),
                ));
            }
        }
    }

    for (_key, ct_data) in schema_set.arenas.complex_types.iter() {
        for (i, attr_use) in ct_data.attributes.iter().enumerate() {
            if attr_use.use_kind == AttributeUseKind::Prohibited {
                continue;
            }
            let resolved = ct_data.resolved_attributes.get(i);
            let ref_decl = resolved
                .and_then(|r| r.resolved_ref)
                .and_then(|k| schema_set.arenas.attributes.get(k));

            let has_constraint = attr_use.attribute.default_value.is_some()
                || attr_use.attribute.fixed_value.is_some()
                || ref_decl.is_some_and(|d| d.default_value.is_some() || d.fixed_value.is_some());
            if !has_constraint {
                continue;
            }

            let attr_type = resolved
                .and_then(|r| r.resolved_type)
                .or_else(|| ref_decl.and_then(|d| d.resolved_type));
            if let Some(TypeKey::Simple(st_key)) = attr_type {
                if schema_set.derives_from(st_key, id_key) {
                    let attr_name = attr_use
                        .attribute
                        .name
                        .map(|n| schema_set.name_table.resolve(n).to_string())
                        .or_else(|| {
                            ref_decl
                                .and_then(|d| d.name)
                                .map(|n| schema_set.name_table.resolve(n).to_string())
                        })
                        .unwrap_or_else(|| "(anonymous)".to_string());
                    let constraint = if attr_use.attribute.default_value.is_some()
                        || ref_decl.and_then(|d| d.default_value.as_ref()).is_some()
                    {
                        "default"
                    } else {
                        "fixed"
                    };
                    let location = attr_use
                        .attribute
                        .source
                        .as_ref()
                        .or(ct_data.source.as_ref())
                        .and_then(|s| schema_set.source_maps.locate(s));
                    return Err(SchemaError::structural(
                        "cos-attribute-decl",
                        format!(
                            "Attribute '{}' has type xs:ID (or derived) and must not have a {} value constraint",
                            attr_name, constraint
                        ),
                        location,
                    ));
                }
            }
        }
    }

    Ok(())
}

/// `a-props-correct.3`: validate that attribute `default`/`fixed` values
/// are type-valid for the declared type.
///
/// Walks every globally-declared attribute and every attribute use inside
/// complex types and rejects when the value constraint cannot be parsed
/// against the attribute's declared simple type.
pub fn validate_attribute_value_constraints(schema_set: &SchemaSet) -> SchemaResult<()> {
    // Top-level attribute declarations
    for (_key, attr) in schema_set.arenas.attributes.iter() {
        if attr.source.is_none() {
            // Built-in xsi:* attributes have `source: None`; skip them.
            continue;
        }
        // a-props-correct.1 / src-attribute: the {type definition} of every
        // attribute must be a simple type definition. attD002 (`type="ct"`
        // where `ct` is a complex type with simpleContent).
        if matches!(attr.resolved_type, Some(TypeKey::Complex(_))) {
            let attr_name = attr
                .name
                .map(|n| schema_set.name_table.resolve(n).to_string())
                .unwrap_or_else(|| "(anonymous)".to_string());
            return Err(SchemaError::structural(
                "a-props-correct",
                format!(
                    "Attribute '{}' references a complex type; the type definition of an \
                     attribute must be a simple type",
                    attr_name,
                ),
                schema_set.locate(attr.source.as_ref()),
            ));
        }
        let (value, is_fixed) = match (&attr.default_value, &attr.fixed_value) {
            (Some(v), _) => (v.as_str(), false),
            (_, Some(v)) => (v.as_str(), true),
            (None, None) => continue,
        };
        let Some(type_key @ TypeKey::Simple(_)) = attr.resolved_type else {
            continue;
        };
        // Simple Default Valid (§3.2.6.2): a QName / NOTATION in the value is
        // read with the bindings of the declaration it is written on.
        if !schema_set.value_constraint_literal_has_value(value, type_key, attr.source.as_ref()) {
            let attr_name = attr
                .name
                .map(|n| schema_set.name_table.resolve(n).to_string())
                .unwrap_or_else(|| "(anonymous)".to_string());
            let constraint = if is_fixed { "fixed" } else { "default" };
            return Err(SchemaError::structural(
                "a-props-correct",
                format!(
                    "Attribute '{}' {} value '{}' is not valid for its declared type",
                    attr_name, constraint, value
                ),
                schema_set.locate(attr.source.as_ref()),
            ));
        }
    }

    // Attribute uses inside complex types
    for (_key, ct) in schema_set.arenas.complex_types.iter() {
        for (i, attr_use) in ct.attributes.iter().enumerate() {
            if attr_use.use_kind == AttributeUseKind::Prohibited {
                continue;
            }
            let resolved = ct.resolved_attributes.get(i);
            let ref_decl = resolved
                .and_then(|r| r.resolved_ref)
                .and_then(|k| schema_set.arenas.attributes.get(k));

            // Use the local (override) value-constraint when present;
            // otherwise the global declaration's. au-props-correct.2 says
            // a use's fixed must equal the declaration's, but here we only
            // need to check that *some* effective value-constraint parses.
            // The schema element the value is written on goes with it: its
            // bindings read a QName / NOTATION value.
            let value_constraint: Option<(&str, bool, Option<&SourceRef>)> = attr_use
                .attribute
                .fixed_value
                .as_deref()
                .map(|v| (v, true))
                .or_else(|| {
                    attr_use
                        .attribute
                        .default_value
                        .as_deref()
                        .map(|v| (v, false))
                })
                .map(|(v, fixed)| (v, fixed, attr_use.attribute.source.as_ref()))
                .or_else(|| {
                    ref_decl.and_then(|d| {
                        d.fixed_value
                            .as_deref()
                            .map(|v| (v, true))
                            .or_else(|| d.default_value.as_deref().map(|v| (v, false)))
                            .map(|(v, fixed)| (v, fixed, d.source.as_ref()))
                    })
                });

            // au-props-correct (§3.5.6 clause 3): if the referenced attribute
            // declaration has `fixed`, then a `fixed` on the attribute use
            // "is identical to" the declaration's value (the use's `default`
            // is forbidden when the declaration is fixed) — values, compared
            // in the declaration's type; a form the type rejects (another
            // constraint reports it), or an unresolved type, falls back to
            // the literals after whitespace collapse.
            if let (Some(use_fixed), Some(decl_fixed)) = (
                attr_use.attribute.fixed_value.as_deref(),
                ref_decl.and_then(|d| d.fixed_value.as_deref()),
            ) {
                use crate::types::facets::normalize_whitespace;
                use crate::types::WhitespaceMode;
                let identical = ref_decl
                    .and_then(|d| d.resolved_type)
                    .and_then(|tk| {
                        // Each literal is read with the namespace bindings
                        // of the element it is written on (QName values).
                        crate::validation::simple::fixed_values_identical_in(
                            use_fixed,
                            schema_set
                                .value_constraint_namespaces(attr_use.attribute.source.as_ref()),
                            decl_fixed,
                            schema_set.value_constraint_namespaces(
                                ref_decl.and_then(|d| d.source.as_ref()),
                            ),
                            tk,
                            schema_set,
                        )
                    })
                    .unwrap_or_else(|| {
                        normalize_whitespace(use_fixed, WhitespaceMode::Collapse)
                            == normalize_whitespace(decl_fixed, WhitespaceMode::Collapse)
                    });
                if !identical {
                    let attr_name = ref_decl
                        .and_then(|d| d.name)
                        .map(|n| schema_set.name_table.resolve(n).to_string())
                        .unwrap_or_else(|| "(anonymous)".to_string());
                    let location = attr_use
                        .attribute
                        .source
                        .as_ref()
                        .or(ct.source.as_ref())
                        .and_then(|s| schema_set.source_maps.locate(s));
                    return Err(SchemaError::structural(
                        "au-props-correct",
                        format!(
                            "Attribute use 'fixed' value '{}' on '{}' does not match the \
                             referenced attribute declaration's 'fixed' value '{}'",
                            use_fixed, attr_name, decl_fixed,
                        ),
                        location,
                    ));
                }
            }

            // au-props-correct.2 also forbids a `default` on the use when
            // the declaration has `fixed`.
            if attr_use.attribute.default_value.is_some()
                && ref_decl.and_then(|d| d.fixed_value.as_deref()).is_some()
            {
                let attr_name = ref_decl
                    .and_then(|d| d.name)
                    .map(|n| schema_set.name_table.resolve(n).to_string())
                    .unwrap_or_else(|| "(anonymous)".to_string());
                let location = attr_use
                    .attribute
                    .source
                    .as_ref()
                    .or(ct.source.as_ref())
                    .and_then(|s| schema_set.source_maps.locate(s));
                return Err(SchemaError::structural(
                    "au-props-correct",
                    format!(
                        "Attribute use cannot specify 'default' for '{}' because the \
                         referenced attribute declaration has 'fixed'",
                        attr_name,
                    ),
                    location,
                ));
            }

            let Some((value, is_fixed, value_source)) = value_constraint else {
                continue;
            };

            let attr_type = resolved
                .and_then(|r| r.resolved_type)
                .or_else(|| ref_decl.and_then(|d| d.resolved_type));
            let Some(type_key @ TypeKey::Simple(_)) = attr_type else {
                continue;
            };
            if !schema_set.value_constraint_literal_has_value(value, type_key, value_source) {
                let attr_name = attr_use
                    .attribute
                    .name
                    .map(|n| schema_set.name_table.resolve(n).to_string())
                    .or_else(|| {
                        ref_decl
                            .and_then(|d| d.name)
                            .map(|n| schema_set.name_table.resolve(n).to_string())
                    })
                    .unwrap_or_else(|| "(anonymous)".to_string());
                let constraint = if is_fixed { "fixed" } else { "default" };
                let location = attr_use
                    .attribute
                    .source
                    .as_ref()
                    .or(ct.source.as_ref())
                    .and_then(|s| schema_set.source_maps.locate(s));
                return Err(SchemaError::structural(
                    "a-props-correct",
                    format!(
                        "Attribute '{}' {} value '{}' is not valid for its declared type",
                        attr_name, constraint, value
                    ),
                    location,
                ));
            }
        }
    }

    Ok(())
}

/// `e-props-correct.2` and `e-props-correct.4`: validate that element
/// `default`/`fixed` values are type-valid for the declared type.
///
/// - `e-props-correct.2`: the value must be valid for the element's type.
/// - `e-props-correct.4`: if the type is (or derives from) xs:ID, no value
///   constraint is allowed.
pub fn validate_element_value_constraints(schema_set: &SchemaSet) -> SchemaResult<()> {
    use crate::parser::frames::ComplexContentResult;
    use crate::types::XmlTypeCode;

    let id_key = schema_set.builtin_types().get_by_type_code(XmlTypeCode::Id);
    let any_type_key = TypeKey::Complex(schema_set.any_type_key());

    for (_key, elem) in schema_set.arenas.elements.iter() {
        let (value, is_fixed) = match (&elem.default_value, &elem.fixed_value) {
            (Some(v), _) => (v.as_str(), false),
            (_, Some(v)) => (v.as_str(), true),
            (None, None) => continue,
        };

        // Element refs inherit constraints from the referenced element
        if elem.resolved_ref.is_some() {
            continue;
        }

        let type_key = match elem.resolved_type {
            Some(tk) if tk != any_type_key => tk,
            _ => continue,
        };

        let elem_name = || {
            elem.name
                .map(|n| schema_set.name_table.resolve_ref(n))
                .unwrap_or("(anonymous)")
        };
        let location = || schema_set.locate(elem.source.as_ref());
        let constraint = if is_fixed { "fixed" } else { "default" };

        // src-element §3.3.3 clause 3.2: when an element declaration has a
        // `default` or `fixed` value, the type definition must be either a
        // simple type, a complex type with simple content, or a complex
        // type with mixed=true. Anything else (complex non-simple, non-mixed
        // content) is invalid.
        if let TypeKey::Complex(ct_key) = type_key {
            if let Some(ct) = schema_set.arenas.complex_types.get(ct_key) {
                let simple_content = matches!(ct.content, ComplexContentResult::Simple(_));
                if !simple_content && !ct.mixed {
                    return Err(SchemaError::structural(
                        "src-element",
                        format!(
                            "Element '{}' has '{}' value but its type is a complex type \
                             with non-mixed, non-simple content",
                            elem_name(),
                            constraint
                        ),
                        location(),
                    ));
                }
            }
        }

        // e-props-correct.4: xs:ID (or derived) cannot have a value constraint.
        // XSD 1.1 §3.3.6.1 removes this restriction (it has no analogous clause);
        // only apply in XSD 1.0 mode.
        if !schema_set.is_xsd11() {
            if let (Some(id_simple_key), TypeKey::Simple(st_key)) = (id_key, type_key) {
                if schema_set.derives_from(st_key, id_simple_key) {
                    return Err(SchemaError::structural(
                        "e-props-correct.4",
                        format!(
                            "Element '{}' has type xs:ID (or derived) and must not have a {} value constraint",
                            elem_name(), constraint
                        ),
                        location(),
                    ));
                }
            }
        }

        // e-props-correct.2: value must be valid for the declared type — for
        // a complex type with simple content, its {content type}'s simple type
        // with the facets of its restrictions (Structures §3.4.2.2), which
        // `validate_simple_type` applies to a complex type key.
        let effective_type = match type_key {
            TypeKey::Simple(_) => Some(type_key),
            TypeKey::Complex(ck) => schema_set.arenas.complex_types.get(ck).and_then(|ct| {
                if matches!(ct.content, ComplexContentResult::Simple(_)) {
                    Some(type_key)
                } else {
                    effective_simple_content_type_key(schema_set, ct)
                }
            }),
        };

        // A QName / NOTATION in the value is read with the bindings of the
        // declaration it is written on (Datatypes §3.3.18), as the {value}
        // is (`SchemaSet::element_value_constraint`).
        if let Some(st_key) = effective_type {
            if !schema_set.value_constraint_literal_has_value(value, st_key, elem.source.as_ref()) {
                return Err(SchemaError::structural(
                    "e-props-correct.2",
                    format!(
                        "Element '{}' {} value '{}' is not valid for its declared type",
                        elem_name(),
                        constraint,
                        value
                    ),
                    location(),
                ));
            }
        }
    }

    Ok(())
}

/// `ct-props-correct.4` / `ag-props-correct.2`: every complex type's
/// effective `{attribute uses}` must contain at most one entry per
/// `(target_namespace, name)`. Two distinct attribute declarations with the
/// same expanded name are forbidden by both XSD 1.0 and XSD 1.1.
///
/// The check is keyed by *declaration identity*, not by `(name, namespace)`,
/// so reaching the same declaration along multiple paths (including XSD 1.1
/// circular attribute groups) does not produce a false positive — only
/// genuinely distinct declarations that happen to share an expanded name
/// are flagged. The W3C `attQ011` fixture exercises the cross-attribute-
/// group case where attribute "foo" appears once via a global
/// `<attribute ref="x:foo"/>` reference and once via a redefined
/// `<attributeGroup ref="x:red"/>` whose members include a local
/// `<attribute name="foo"/>`.
pub fn validate_complex_type_attribute_uniqueness(schema_set: &SchemaSet) -> SchemaResult<()> {
    use std::collections::{HashMap, HashSet};

    // Stable identity of an attribute declaration. Variants:
    // - `GlobalRef(k)`        — `<xs:attribute ref="...">` resolving to
    //                            global attribute key `k`.
    // - `InlineGroup(g, i)`   — i-th inline `<xs:attribute>` in
    //                            attribute group `g`.
    // - `InlineComplex(c, i)` — i-th inline `<xs:attribute>` in
    //                            complex type `c`.
    #[derive(Hash, PartialEq, Eq, Clone, Copy)]
    enum AttrDeclId {
        GlobalRef(AttributeKey),
        InlineGroup(AttributeGroupKey, usize),
        InlineComplex(ComplexTypeKey, usize),
    }

    fn walk_attribute_group(
        schema_set: &SchemaSet,
        ag_key: AttributeGroupKey,
        visiting_groups: &mut HashSet<AttributeGroupKey>,
        seen: &mut HashSet<AttrDeclId>,
        out: &mut Vec<EffectiveAttributeUse>,
    ) {
        if !visiting_groups.insert(ag_key) {
            return;
        }
        let Some(ag) = schema_set.arenas.attribute_groups.get(ag_key) else {
            visiting_groups.remove(&ag_key);
            return;
        };
        if let Some(ref_key) = ag.resolved_ref {
            walk_attribute_group(schema_set, ref_key, visiting_groups, seen, out);
            visiting_groups.remove(&ag_key);
            return;
        }

        for (i, attr_use) in ag.attributes.iter().enumerate() {
            let resolved = ag.resolved_attributes.get(i);
            let decl_id = if let Some(global_key) = resolved.and_then(|r| r.resolved_ref) {
                AttrDeclId::GlobalRef(global_key)
            } else {
                AttrDeclId::InlineGroup(ag_key, i)
            };
            if seen.insert(decl_id) {
                if let Some(eau) = resolve_single_attribute_use(schema_set, attr_use, resolved) {
                    out.push(eau);
                }
            }
        }
        for &nested in &ag.resolved_attribute_groups {
            walk_attribute_group(schema_set, nested, visiting_groups, seen, out);
        }

        visiting_groups.remove(&ag_key);
    }

    fn collect_with_dedup(
        schema_set: &SchemaSet,
        type_def: &crate::arenas::ComplexTypeDefData,
        ct_key: ComplexTypeKey,
        depth: usize,
        visiting_groups: &mut HashSet<AttributeGroupKey>,
        seen: &mut HashSet<AttrDeclId>,
        out: &mut Vec<EffectiveAttributeUse>,
    ) {
        if depth > schema_set.recursion_bound(50) {
            return;
        }
        for (i, attr_use) in type_def.attributes.iter().enumerate() {
            let resolved = type_def.resolved_attributes.get(i);
            let decl_id = if let Some(global_key) = resolved.and_then(|r| r.resolved_ref) {
                AttrDeclId::GlobalRef(global_key)
            } else {
                AttrDeclId::InlineComplex(ct_key, i)
            };
            if seen.insert(decl_id) {
                if let Some(eau) = resolve_single_attribute_use(schema_set, attr_use, resolved) {
                    out.push(eau);
                }
            }
        }
        for &ag_key in &type_def.resolved_attribute_groups {
            visiting_groups.clear();
            walk_attribute_group(schema_set, ag_key, visiting_groups, seen, out);
        }
        if type_def.derivation_method == Some(DerivationMethod::Extension) {
            if let Some(TypeKey::Complex(base_key)) = type_def.resolved_base_type {
                if let Some(base) = schema_set.arenas.complex_types.get(base_key) {
                    collect_with_dedup(
                        schema_set,
                        base,
                        base_key,
                        depth + 1,
                        visiting_groups,
                        seen,
                        out,
                    );
                }
            }
        }
    }

    // Reusable scratch buffers, cleared per type to avoid per-iteration
    // allocator traffic on schemas with many complex types.
    let mut seen: HashSet<AttrDeclId> = HashSet::new();
    let mut attrs: Vec<EffectiveAttributeUse> = Vec::new();
    let mut visiting_groups: HashSet<AttributeGroupKey> = HashSet::new();
    let mut by_name: HashMap<(Option<NameId>, NameId), ()> = HashMap::new();

    // ct-props-correct clause 4 is XSD 1.0-only. Hoist the `xs:ID` builtin
    // lookup out of the per-complex-type loop; it's an arena-backed constant
    // for the lifetime of the schema set.
    let id_key_for_xsd10 = if schema_set.is_xsd10() {
        schema_set
            .builtin_types()
            .get_by_type_code(crate::types::XmlTypeCode::Id)
    } else {
        None
    };

    for (key, type_def) in schema_set.arenas.complex_types.iter() {
        seen.clear();
        attrs.clear();
        by_name.clear();
        collect_with_dedup(
            schema_set,
            type_def,
            key,
            0,
            &mut visiting_groups,
            &mut seen,
            &mut attrs,
        );

        // §3.4.6: a prohibited attribute use is NOT an entry in the
        // `{attribute uses}` set, so it cannot collide with a (re-)declared
        // use in a derived type.
        attrs.retain(|eau| eau.use_kind != AttributeUseKind::Prohibited);

        for attr in &attrs {
            if by_name
                .insert((attr.target_namespace, attr.name), ())
                .is_some()
            {
                let attr_name_str = schema_set.name_table.resolve(attr.name);
                let type_name =
                    format_type_name(schema_set, type_def.name, type_def.target_namespace);
                let location = type_def
                    .source
                    .as_ref()
                    .and_then(|s| schema_set.source_maps.locate(s));
                return Err(SchemaError::structural(
                    "ct-props-correct",
                    format!(
                        "Complex type '{}': two distinct attribute declarations \
                         with the same expanded name '{}' (ct-props-correct \
                         clause 4 / ag-props-correct clause 2)",
                        type_name, attr_name_str,
                    ),
                    location,
                ));
            }
        }

        // ct-props-correct clause 4 (XSD 1.0 only): "Two distinct members of
        // the {attribute uses} must not have {type definition}s which are
        // both `xs:ID` or are derived from `xs:ID`." XSD 1.1 dropped this
        // constraint — see saxon's id001 test which explicitly documents
        // that an XSD 1.1 type may declare multiple ID-typed attributes.
        if let Some(id_key) = id_key_for_xsd10 {
            let mut id_attrs = attrs.iter().filter(|attr| match attr.resolved_type {
                Some(TypeKey::Simple(st_key)) => schema_set.derives_from(st_key, id_key),
                _ => false,
            });
            if let (Some(first), Some(second)) = (id_attrs.next(), id_attrs.next()) {
                let first_name = schema_set.name_table.resolve(first.name);
                let second_name = schema_set.name_table.resolve(second.name);
                let type_name =
                    format_type_name(schema_set, type_def.name, type_def.target_namespace);
                let location = type_def
                    .source
                    .as_ref()
                    .and_then(|s| schema_set.source_maps.locate(s));
                return Err(SchemaError::structural(
                    "ct-props-correct",
                    format!(
                        "Complex type '{}': attributes '{}' and '{}' both have \
                         xs:ID-derived types (ct-props-correct clause 4; XSD 1.0 only)",
                        type_name, first_name, second_name,
                    ),
                    location,
                ));
            }
        }
    }
    Ok(())
}

/// XSD 1.0 §3.2.17 lexical check for `xs:anyURI` source attributes on
/// `xs:appinfo` and `xs:documentation`. The W3C `anyURI_a001_1336` fixture
/// places `source="9999...anyURI:"` and `source="1111...http://foo/bar"`
/// on annotations of an element declaration; both have a colon whose
/// scheme prefix starts with a digit, which is invalid per RFC 2396.
/// XSD 1.1 explicitly relaxed the rule, so this validator is XSD 1.0-only.
///
/// We deliberately scope the check to annotation `source` attributes:
///   - directives' `schemaLocation` values like `"0"` and `"123"` are
///     valid relative URIs per RFC 2396 and survive any reasonable
///     strict lexer;
///   - the same goes for `xs:notation/@public`/`@system` and
///     `xs:anyAttribute/@namespace` numeric values in the same fixture.
///
/// The annotation source values are the only unambiguously-malformed
/// anyURIs in the fixture, and they alone are sufficient to make the
/// schema fail per the W3C "one or more invalid anyURIs" ruling.
pub fn validate_xsd10_annotation_source_anyuri(schema_set: &SchemaSet) -> SchemaResult<()> {
    use crate::schema::annotation::{Annotation, AnnotationItem};
    use crate::types::validators::is_strict_xsd10_anyuri;

    if !schema_set.is_xsd10() {
        return Ok(());
    }

    fn check_annotation(
        schema_set: &SchemaSet,
        annotation: Option<&Annotation>,
    ) -> SchemaResult<()> {
        let Some(annotation) = annotation else {
            return Ok(());
        };
        for item in &annotation.items {
            match item {
                AnnotationItem::AppInfo(ai) => {
                    if let Some(ref src) = ai.source {
                        if !is_strict_xsd10_anyuri(src) {
                            let location = ai
                                .source_ref
                                .as_ref()
                                .and_then(|s| schema_set.source_maps.locate(s));
                            return Err(SchemaError::structural(
                                "cvc-datatype-valid",
                                format!(
                                    "<xs:appinfo source=\"{}\"> is not a valid xs:anyURI \
                                     (XSD 1.0 strict scheme syntax)",
                                    src
                                ),
                                location,
                            ));
                        }
                    }
                }
                AnnotationItem::Documentation(d) => {
                    if let Some(ref src) = d.source {
                        if !is_strict_xsd10_anyuri(src) {
                            let location = d
                                .source_ref
                                .as_ref()
                                .and_then(|s| schema_set.source_maps.locate(s));
                            return Err(SchemaError::structural(
                                "cvc-datatype-valid",
                                format!(
                                    "<xs:documentation source=\"{}\"> is not a valid \
                                     xs:anyURI (XSD 1.0 strict scheme syntax)",
                                    src
                                ),
                                location,
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    // Walk every arena that can carry an annotation. NOTE: anyone adding a
    // new annotatable arena type must extend this list.
    for (_k, ct) in schema_set.arenas.complex_types.iter() {
        check_annotation(schema_set, ct.annotation.as_ref())?;
    }
    for (_k, st) in schema_set.arenas.simple_types.iter() {
        check_annotation(schema_set, st.annotation.as_ref())?;
    }
    for (_k, el) in schema_set.arenas.elements.iter() {
        check_annotation(schema_set, el.annotation.as_ref())?;
    }
    for (_k, at) in schema_set.arenas.attributes.iter() {
        check_annotation(schema_set, at.annotation.as_ref())?;
    }
    for (_k, ag) in schema_set.arenas.attribute_groups.iter() {
        check_annotation(schema_set, ag.annotation.as_ref())?;
    }
    for (_k, mg) in schema_set.arenas.model_groups.iter() {
        check_annotation(schema_set, mg.annotation.as_ref())?;
    }
    for (_k, n) in schema_set.arenas.notations.iter() {
        check_annotation(schema_set, n.annotation.as_ref())?;
    }
    for (_k, ic) in schema_set.arenas.identity_constraints.iter() {
        check_annotation(schema_set, ic.annotation.as_ref())?;
    }
    // Schema-level top-level `<xs:annotation>` elements (a schema can hold
    // several, hence `Vec<Annotation>` rather than `Option<Annotation>`).
    for doc in &schema_set.documents {
        for ann in &doc.annotations {
            check_annotation(schema_set, Some(ann))?;
        }
    }
    Ok(())
}

/// Validate `xsi:` Not Allowed (§3.2.6.4 / `no-xsi`): the `{target namespace}`
/// of a *user-declared* attribute must not match the XML Schema instance
/// namespace. The four pre-defined XSI attributes (`type`, `nil`,
/// `schemaLocation`, `noNamespaceSchemaLocation`) are seeded into the
/// attributes arena with `source: None`; that absence is the marker we use
/// to skip them.
pub fn validate_no_xsi_attribute_declarations(schema_set: &SchemaSet) -> SchemaResult<()> {
    use crate::namespace::table::well_known;

    for (_key, attr) in schema_set.arenas.attributes.iter() {
        if attr.source.is_none() {
            // Built-in xsi:type / xsi:nil / xsi:schemaLocation /
            // xsi:noNamespaceSchemaLocation are seeded in
            // `types::builtin::initialize_xsi_attributes` with no source.
            continue;
        }
        let Some(ns) = attr.target_namespace else {
            continue;
        };
        if ns != well_known::XSI_NAMESPACE {
            continue;
        }
        let attr_name = attr
            .name
            .map(|n| schema_set.name_table.resolve_ref(n).to_string())
            .unwrap_or_else(|| "(anonymous)".to_string());
        let location = schema_set.locate(attr.source.as_ref());
        return Err(SchemaError::structural(
            "no-xsi",
            format!(
                "Attribute declaration '{}' has target namespace \
                 'http://www.w3.org/2001/XMLSchema-instance', which is \
                 reserved (no-xsi, §3.2.6.4)",
                attr_name
            ),
            location,
        ));
    }

    // The same constraint must also apply to inline attribute declarations
    // inside attribute groups and complex types — `<attribute>` children
    // without a `ref=` declare a fresh attribute whose effective namespace
    // is determined by `form` / `attributeFormDefault`. With
    // `attributeFormDefault="qualified"` and `targetNamespace=XSI`
    // (attKb018a), each unqualified attribute in an attribute group is
    // promoted to the XSI namespace and must be rejected.
    //
    // Inline `target_namespace=XSI` on an `<attribute>` child reaches the
    // arena unchanged. The form/default-driven path only routes to XSI
    // when the owner's `target_namespace` is the XSI namespace, which is
    // exceedingly rare; skip the per-use scan when no document declares
    // XSI as its target.
    let any_xsi_owner = schema_set
        .documents
        .iter()
        .any(|d| d.target_namespace == Some(well_known::XSI_NAMESPACE));
    let owners_to_scan = any_xsi_owner;
    for (_key, group) in schema_set.arenas.attribute_groups.iter() {
        check_no_xsi_in_attribute_uses(
            schema_set,
            &group.attributes,
            group.target_namespace,
            owners_to_scan,
        )?;
    }
    for (_key, ct) in schema_set.arenas.complex_types.iter() {
        check_no_xsi_in_attribute_uses(
            schema_set,
            &ct.attributes,
            ct.target_namespace,
            owners_to_scan,
        )?;
    }
    Ok(())
}

/// Apply `no-xsi` (§3.2.6.4) to a slice of attribute uses owned by an
/// attribute group or complex type. Skips `ref=` uses (they delegate to
/// the global declaration, which is already covered by the global pass).
fn check_no_xsi_in_attribute_uses(
    schema_set: &SchemaSet,
    attrs: &[crate::parser::frames::AttributeUseResult],
    owner_target_namespace: Option<NameId>,
    owner_could_be_xsi: bool,
) -> SchemaResult<()> {
    use crate::namespace::table::well_known;

    for attr_use in attrs {
        if attr_use.attribute.ref_name.is_some() {
            continue;
        }
        // Fast-path: when the attribute carries no explicit XSI target and
        // no owner document declares XSI as its target, the
        // form/default-driven effective namespace can never be XSI.
        let explicit_xsi = attr_use.attribute.target_namespace == Some(well_known::XSI_NAMESPACE);
        if !explicit_xsi && !owner_could_be_xsi {
            continue;
        }
        let effective_ns = schema_set.effective_local_attribute_namespace(
            attr_use.attribute.target_namespace,
            attr_use.attribute.form.as_deref(),
            attr_use.attribute.source.as_ref(),
            owner_target_namespace,
        );
        if effective_ns != Some(well_known::XSI_NAMESPACE) {
            continue;
        }
        let attr_name = attr_use
            .attribute
            .name
            .map(|n| schema_set.name_table.resolve_ref(n).to_string())
            .unwrap_or_else(|| "(anonymous)".to_string());
        let location = schema_set.locate(attr_use.attribute.source.as_ref());
        return Err(SchemaError::structural(
            "no-xsi",
            format!(
                "Attribute declaration '{}' has target namespace \
                 'http://www.w3.org/2001/XMLSchema-instance', which is \
                 reserved (no-xsi, §3.2.6.4)",
                attr_name
            ),
            location,
        ));
    }
    Ok(())
}

/// Validate src-element §3.3.3 clause 4.3 / src-attribute §3.2.3 clause 6.3:
/// a local `<element>` or `<attribute>` declaring an explicit
/// `targetNamespace` attribute that differs from the schema's own
/// `targetNamespace` is permitted only when there is a `<restriction>`
/// ancestor (between the local declaration and its nearest `<complexType>`
/// ancestor) whose base does not match `xs:anyType`.
///
/// Implementation: per complex type, treat the declaration's "nearest
/// `<complexType>` ancestor" as this complex type. The clause is satisfied
/// iff the type's `derivation_method` is `Restriction` and its
/// `resolved_base_type` is not `xs:anyType`. In every other case (extension,
/// no derivation, or restriction of `xs:anyType`), any local element /
/// attribute carrying a divergent `targetNamespace` is invalid.
///
/// Closes saxon `target002` (element case) and `target004` (attribute case);
/// `target001`/`target003` (the matching `valid` cases) keep passing because
/// they use `restriction` of a non-`anyType` base.
pub fn validate_local_decl_target_namespace(schema_set: &SchemaSet) -> SchemaResult<()> {
    use crate::parser::frames::{ComplexContentResult, ParticleResult, ParticleTerm};

    fn find_divergent_local_element<'a>(
        schema_set: &'a SchemaSet,
        particle: &'a ParticleResult,
        schema_tns: Option<NameId>,
        depth: usize,
    ) -> Option<(Option<SourceRef>, String)> {
        if depth > schema_set.recursion_bound(100) {
            return None;
        }
        match &particle.term {
            ParticleTerm::Element(elem) => {
                if elem.ref_name.is_some() {
                    return None;
                }
                if let Some(ns) = elem.target_namespace {
                    if Some(ns) != schema_tns {
                        let name_str = elem
                            .name
                            .map(|n| schema_set.name_table.resolve_ref(n).to_string())
                            .unwrap_or_default();
                        return Some((elem.source.clone(), name_str));
                    }
                }
            }
            ParticleTerm::Group(group) => {
                // Only descend into inline groups (no ref_name); a group ref
                // points at a top-level group whose own decls are validated
                // independently when their containing context is examined.
                if group.ref_name.is_none() {
                    for child in &group.particles {
                        if let Some(found) =
                            find_divergent_local_element(schema_set, child, schema_tns, depth + 1)
                        {
                            return Some(found);
                        }
                    }
                }
            }
            ParticleTerm::Any(_) => {}
        }
        None
    }

    for (_, ct) in schema_set.arenas.complex_types.iter() {
        let schema_tns = ct.target_namespace;
        let restriction_of_non_any = match (ct.derivation_method, ct.resolved_base_type) {
            (Some(crate::parser::frames::DerivationMethod::Restriction), Some(base_key)) => {
                !schema_set.is_any_type(base_key)
            }
            _ => false,
        };
        if restriction_of_non_any {
            continue;
        }

        // Walk content particles for local elements with divergent
        // targetNamespace.
        let particle_opt = match &ct.content {
            ComplexContentResult::Complex(def) => def.particle.as_ref(),
            _ => None,
        };
        if let Some(particle) = particle_opt {
            if let Some((src, name)) =
                find_divergent_local_element(schema_set, particle, schema_tns, 0)
            {
                let location = schema_set
                    .locate(src.as_ref())
                    .or_else(|| schema_set.locate(ct.source.as_ref()));
                return Err(SchemaError::structural(
                    "src-element",
                    format!(
                        "Local element '{}' has an explicit targetNamespace differing from the \
                         schema's, but is not inside a <restriction> of a non-anyType base \
                         (src-element §3.3.3 clause 4.3)",
                        name
                    ),
                    location,
                ));
            }
        }

        // Check direct attribute uses for divergent targetNamespace.
        for au in &ct.attributes {
            let attr = &au.attribute;
            if attr.ref_name.is_some() {
                continue;
            }
            let Some(ns) = attr.target_namespace else {
                continue;
            };
            if Some(ns) == schema_tns {
                continue;
            }
            let name = attr
                .name
                .map(|n| schema_set.name_table.resolve_ref(n).to_string())
                .unwrap_or_default();
            let location = schema_set
                .locate(attr.source.as_ref())
                .or_else(|| schema_set.locate(ct.source.as_ref()));
            return Err(SchemaError::structural(
                "src-attribute",
                format!(
                    "Local attribute '{}' has an explicit targetNamespace differing from the \
                     schema's, but is not inside a <restriction> of a non-anyType base \
                     (src-attribute §3.2.3 clause 6.3)",
                    name
                ),
                location,
            ));
        }
    }
    Ok(())
}

/// Validate cos-element-consistent (§3.8.6.3) for the substitution-group
/// case: a content model that contains both a local element with QName Q
/// AND an element ref whose substitution-group expansion includes another
/// declaration with the same QName Q must agree on `{type definition}`.
///
/// The base XSD 1.1 EDC machinery (`validate_local_element_type_table_*`)
/// only compares type *tables*; this pass also covers the type-definition
/// rule that makes saxon `subsgroup901.bad.xsd` invalid (a CT containing
/// local `n: xs:date` plus `<xs:element ref="appendixContent">`, where the
/// global `n: xs:string` substitutes for `appendixContent`).
///
/// Active for both XSD 1.0 and 1.1.
pub fn validate_substitution_group_element_consistency(schema_set: &SchemaSet) -> SchemaResult<()> {
    use crate::parser::frames::{ComplexContentResult, ParticleResult, ParticleTerm};
    use std::collections::HashMap;

    type Entry = (TypeKey, Option<SourceRef>);

    // head → direct substitution members. Built once so the per-ref expansion
    // is O(direct members) instead of an O(elements) arena scan per ref.
    let mut subst_index: HashMap<ElementKey, Vec<ElementKey>> = HashMap::new();
    for (mk, m) in schema_set.arenas.elements.iter() {
        for &head in &m.resolved_substitution_groups {
            subst_index.entry(head).or_default().push(mk);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn walk_particle(
        schema_set: &SchemaSet,
        particle: &ParticleResult,
        target_ns: Option<NameId>,
        local_keys: &[Option<ElementKey>],
        flat_idx: &mut usize,
        subst_index: &HashMap<ElementKey, Vec<ElementKey>>,
        out: &mut HashMap<(Option<NameId>, NameId), Vec<Entry>>,
        depth: usize,
    ) {
        if depth > schema_set.recursion_bound(100) {
            return;
        }
        match &particle.term {
            ParticleTerm::Element(elem) => {
                if let Some(ref_qn) = &elem.ref_name {
                    *flat_idx += 1;
                    // The head itself contributes only when non-abstract;
                    // otherwise only its substitution members can appear.
                    let Some(head_key) =
                        schema_set.lookup_element(ref_qn.namespace, ref_qn.local_name)
                    else {
                        return;
                    };
                    let mut visited: std::collections::HashSet<ElementKey> =
                        std::collections::HashSet::new();
                    let mut stack = vec![head_key];
                    while let Some(current) = stack.pop() {
                        if !visited.insert(current) {
                            continue;
                        }
                        let Some(decl) = schema_set.arenas.elements.get(current) else {
                            continue;
                        };
                        // Per XSD 1.1 (W3C Bugzilla 4337), abstract members participate
                        // in the substitution group for cos-element-consistent purposes;
                        // XSD 1.0 excludes them.
                        if !decl.is_abstract || schema_set.is_xsd11() {
                            if let (Some(name), Some(t)) = (decl.name, decl.resolved_type) {
                                out.entry((decl.target_namespace, name))
                                    .or_default()
                                    .push((t, particle.source.clone()));
                            }
                        }
                        if let Some(members) = subst_index.get(&current) {
                            stack.extend(members.iter().copied());
                        }
                    }
                } else {
                    let idx = *flat_idx;
                    *flat_idx += 1;
                    if let Some(Some(elem_key)) = local_keys.get(idx) {
                        if let Some(decl) = schema_set.arenas.elements.get(*elem_key) {
                            if let (Some(name), Some(t)) = (decl.name, decl.resolved_type) {
                                // Arena's `target_namespace` is already the effective
                                // namespace (form + elementFormDefault applied during
                                // allocate_content_particle_elements), so we use it
                                // directly instead of falling back to the outer CT's
                                // target_ns.
                                let ns = decl.target_namespace;
                                out.entry((ns, name))
                                    .or_default()
                                    .push((t, decl.source.clone()));
                            }
                        }
                    }
                }
            }
            ParticleTerm::Group(group) => {
                if let Some(ref_qn) = &group.ref_name {
                    // Group refs don't advance the outer flat_idx; the
                    // model-group arena owns its own resolved_particle_elements.
                    if let Some(group_key) =
                        schema_set.lookup_model_group(ref_qn.namespace, ref_qn.local_name)
                    {
                        let mg = &schema_set.arenas.model_groups[group_key];
                        let inner_ns = mg.target_namespace.or(target_ns);
                        let mut inner_idx = 0usize;
                        for child in &mg.particles {
                            walk_particle(
                                schema_set,
                                child,
                                inner_ns,
                                &mg.resolved_particle_elements,
                                &mut inner_idx,
                                subst_index,
                                out,
                                depth + 1,
                            );
                        }
                    }
                } else {
                    for child in &group.particles {
                        walk_particle(
                            schema_set,
                            child,
                            target_ns,
                            local_keys,
                            flat_idx,
                            subst_index,
                            out,
                            depth + 1,
                        );
                    }
                }
            }
            ParticleTerm::Any(_) => {}
        }
    }

    for (_key, ct) in schema_set.arenas.complex_types.iter() {
        let ComplexContentResult::Complex(cc) = &ct.content else {
            continue;
        };
        let Some(particle) = cc.particle.as_ref() else {
            continue;
        };
        let mut entries: HashMap<(Option<NameId>, NameId), Vec<Entry>> = HashMap::new();
        let mut flat_idx = 0usize;
        walk_particle(
            schema_set,
            particle,
            ct.target_namespace,
            &ct.resolved_content_particle_elements,
            &mut flat_idx,
            &subst_index,
            &mut entries,
            0,
        );

        // An extension's effective content model prepends the base chain's
        // content, so EDC spans the merge: base `child1: xs:integer` plus an
        // extension's own `child1: xs:date` is one content model with an
        // inconsistent QName (saxon complex017). Walk each extension
        // ancestor's own particle into the same entry map.
        let mut current = ct;
        let mut chain_guard = 0usize;
        while matches!(current.derivation_method, Some(DerivationMethod::Extension)) {
            chain_guard += 1;
            if chain_guard > 100 {
                break;
            }
            let Some(TypeKey::Complex(base_key)) = current.resolved_base_type else {
                break;
            };
            let Some(base_ct) = schema_set.arenas.complex_types.get(base_key) else {
                break;
            };
            if let ComplexContentResult::Complex(base_cc) = &base_ct.content {
                if let Some(base_particle) = base_cc.particle.as_ref() {
                    let mut base_flat_idx = 0usize;
                    walk_particle(
                        schema_set,
                        base_particle,
                        base_ct.target_namespace,
                        &base_ct.resolved_content_particle_elements,
                        &mut base_flat_idx,
                        &subst_index,
                        &mut entries,
                        0,
                    );
                }
            }
            current = base_ct;
        }

        for ((ns, name), list) in &entries {
            if list.len() < 2 {
                continue;
            }
            let first_type = list[0].0;
            for (other_type, other_src) in &list[1..] {
                if *other_type == first_type {
                    continue;
                }
                let qn_str = format_type_name(schema_set, Some(*name), *ns);
                let location = schema_set
                    .locate(other_src.as_ref())
                    .or_else(|| schema_set.locate(list[0].1.as_ref()))
                    .or_else(|| schema_set.locate(ct.source.as_ref()));
                return Err(SchemaError::structural(
                    "cos-element-consistent",
                    format!(
                        "Element declarations for '{}' in the same content model \
                         (counting substitution-group expansion) have different \
                         {{type definition}}s (§3.8.6.3 / cos-element-consistent)",
                        qn_str
                    ),
                    location,
                ));
            }
        }
    }
    Ok(())
}
