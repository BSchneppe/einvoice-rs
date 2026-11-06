//! Effective attribute uses (§3.4.2.2), the §3.6.2.2 effective attribute
//! wildcard with its §3.10.6.4 intersection, and the §3.4.6.3 clause-3
//! attribute restriction check.

use super::wildcard::process_contents_strictness;
use super::{format_type_name, is_type_derived_from, type_error_context};
use crate::error::{SchemaError, SchemaResult};
use crate::ids::{AttributeGroupKey, ComplexTypeKey, NameId, TypeKey};
use crate::parser::frames::{
    AttributeUseKind, ComplexContentResult, DerivationMethod, ProcessContents, WildcardNamespace,
    WildcardResult,
};
use crate::schema::SchemaSet;

/// Resolved effective attribute use for comparison during restriction validation.
/// Attribute identity is (target_namespace, name) per §3.2.6.
pub(super) struct EffectiveAttributeUse {
    pub(super) name: NameId,
    pub(super) target_namespace: Option<NameId>,
    pub(super) use_kind: AttributeUseKind,
    pub(super) resolved_type: Option<TypeKey>,
    pub(super) fixed_value: Option<String>,
    /// The declaration the fixed value was written on — the use's own
    /// `<attribute>`, or the referenced declaration (its namespace bindings
    /// read a QName value).
    pub(super) fixed_source: Option<crate::parser::location::SourceRef>,
    pub(super) default_value: Option<String>,
    /// XSD 1.1 §3.5.1 / §3.2.2.3: the use-level `{inheritable}` is the local
    /// `inheritable` attribute when present; for `ref`-based uses without an
    /// own `inheritable` it falls back to the resolved declaration's
    /// `{inheritable}`. Used by §3.4.6.3 derivation-ok-restriction to enforce
    /// `G.{inheritable} = S.{inheritable}` (subsumes clause 5.3).
    pub(super) inheritable: bool,
}

/// Resolve a single attribute use + its parallel resolved data into an
/// `EffectiveAttributeUse`.  Returns `None` when the attribute name
/// cannot be determined (malformed data).
pub(super) fn resolve_single_attribute_use(
    schema_set: &SchemaSet,
    attr_use: &crate::parser::frames::AttributeUseResult,
    resolved: Option<&crate::arenas::ResolvedAttributeUse>,
) -> Option<EffectiveAttributeUse> {
    let (name, target_namespace) = if let Some(ref_name) = &attr_use.attribute.ref_name {
        if let Some(resolved_attr) = resolved.and_then(|r| r.resolved_ref) {
            let decl = schema_set.arenas.attributes.get(resolved_attr);
            (
                decl.and_then(|d| d.name)?,
                decl.and_then(|d| d.target_namespace),
            )
        } else {
            (ref_name.local_name, ref_name.namespace)
        }
    } else {
        let n = attr_use.attribute.name?;
        // For inline (non-ref) attributes, compute effective namespace
        // using form + attributeFormDefault per §3.2.2.
        let ns = schema_set.effective_local_attribute_namespace(
            attr_use.attribute.target_namespace,
            attr_use.attribute.form.as_deref(),
            attr_use.attribute.source.as_ref(),
            None,
        );
        (n, ns)
    };

    // Prefer the use's resolved_type; fall back to the global declaration's type.
    let resolved_type = resolved.and_then(|r| r.resolved_type).or_else(|| {
        resolved
            .and_then(|r| r.resolved_ref)
            .and_then(|ref_key| schema_set.arenas.attributes.get(ref_key))
            .and_then(|decl| decl.resolved_type)
    });

    // For fixed_value: use the inline fixed, or the resolved global decl's fixed.
    let (fixed_value, fixed_source) = match &attr_use.attribute.fixed_value {
        Some(fixed) => (Some(fixed.clone()), attr_use.attribute.source.clone()),
        None => resolved
            .and_then(|r| r.resolved_ref)
            .and_then(|ref_key| schema_set.arenas.attributes.get(ref_key))
            .map(|decl| (decl.fixed_value.clone(), decl.source.clone()))
            .unwrap_or((None, None)),
    };
    // For default_value: use the inline default, or the resolved global decl's default.
    let default_value = attr_use.attribute.default_value.clone().or_else(|| {
        resolved
            .and_then(|r| r.resolved_ref)
            .and_then(|ref_key| schema_set.arenas.attributes.get(ref_key))
            .and_then(|decl| decl.default_value.clone())
    });

    // {inheritable} per §3.2.2.2 / §3.2.2.3: the actual value of the use's
    // `inheritable` attribute, if present, otherwise false for a local
    // declaration and the referenced declaration's {inheritable} for a ref.
    let inheritable = attr_use.use_inheritable(
        resolved
            .and_then(|r| r.resolved_ref)
            .and_then(|ref_key| schema_set.arenas.attributes.get(ref_key)),
    );

    Some(EffectiveAttributeUse {
        name,
        target_namespace,
        use_kind: attr_use.use_kind,
        resolved_type,
        fixed_value,
        fixed_source,
        default_value,
        inheritable,
    })
}

/// The attribute uses a complex type declares itself — §3.4.2.4 (Mapping
/// Rule for Attribute Uses Property) clauses 1 and 2: "1 The set of attribute
/// uses corresponding to the <attribute> \[children\], if any. 2 The
/// {attribute uses} of the attribute groups ·resolved· to by the ·actual
/// value·s of the ref \[attribute\] of the <attributeGroup> \[children\], if
/// any." — each name once, the type's own first.
///
/// An `<attribute use="prohibited">` corresponds to no attribute use
/// (§3.2.2.2 / §3.2.2.3: "unless use='prohibited', in which case the item
/// corresponds to nothing at all"; XSD 1.0 §3.2.2 alike). An attribute
/// group's is therefore dropped. The type's own is kept, after the uses and
/// only for a name none of them has, as a marker (`use_kind` `Prohibited`):
/// in a restriction it keeps the base's use of that name from being inherited
/// (clause 3.2.2: "what would have been an attribute use corresponding to an
/// <attribute> \[child\], if the <attribute> had not had use = prohibited";
/// XSD 1.0 clause 3.2: "the relevant <attribute> among the \[children\] of
/// <restriction>"), in an extension it is ignored (clause 3.1). This is the
/// rule the validation runtime's walk of the {attribute uses} follows
/// (`ValidationRuntime::for_each_attribute_use`).
///
/// Attributes are always on `type_def.attributes` (moved from sc/cc at parse time).
fn collect_effective_attribute_uses(
    schema_set: &SchemaSet,
    type_def: &crate::arenas::ComplexTypeDefData,
) -> Vec<EffectiveAttributeUse> {
    fn has(list: &[EffectiveAttributeUse], eau: &EffectiveAttributeUse) -> bool {
        list.iter()
            .any(|a| a.name == eau.name && a.target_namespace == eau.target_namespace)
    }
    let mut result = Vec::new();
    let mut prohibited = Vec::new();

    for (i, attr_use) in type_def.attributes.iter().enumerate() {
        let resolved = type_def.resolved_attributes.get(i);
        if let Some(eau) = resolve_single_attribute_use(schema_set, attr_use, resolved) {
            if eau.use_kind == AttributeUseKind::Prohibited {
                prohibited.push(eau);
            } else if !has(&result, &eau) {
                result.push(eau);
            }
        }
    }

    let mut group_uses = Vec::new();
    for &ag_key in &type_def.resolved_attribute_groups {
        collect_attribute_group_uses(schema_set, ag_key, &mut group_uses, 0);
    }
    for eau in group_uses {
        if eau.use_kind != AttributeUseKind::Prohibited && !has(&result, &eau) {
            result.push(eau);
        }
    }

    for eau in prohibited {
        if !has(&result, &eau) {
            result.push(eau);
        }
    }

    result
}

/// The complete `{attribute uses}` of a complex type, including the ones it
/// inherits from its base type definition (§3.4.2.4) — the *base* side of a
/// restriction check, which needs the full picture, or every attribute the
/// base itself inherited looks like an attribute the restriction invented.
/// ([`collect_effective_attribute_uses`] is the *derived* side: the checks
/// below are written around "not re-declared here means inherited
/// unchanged".)
///
/// Clause 3: the uses "inherited" from the {base type definition} — for an
/// extension all of them (3.1), for a restriction all but those whose name is
/// "3.2.1 the expanded name of the {attribute declaration} of an attribute use
/// which has already been included in the set, following the rules in clause
/// 1 or clause 2 above; 3.2.2 the expanded name of the {attribute
/// declaration} of what would have been an attribute use corresponding to an
/// <attribute> \[child\], if the <attribute> had not had use = prohibited."
/// So a prohibition only stops a restriction's inheritance — an extension's
/// is "simply ignored", an attribute group's is no use at all — and the set
/// is cumulative down the base chain to `xs:anyType`, as the clause composes.
/// The same walk as the validation runtime's.
fn collect_inherited_attribute_uses(
    schema_set: &SchemaSet,
    type_def: &crate::arenas::ComplexTypeDefData,
) -> Vec<EffectiveAttributeUse> {
    let any_type = schema_set.any_type_key();
    let mut result = Vec::new();
    // Names a more derived level keeps out of what its base types contribute.
    let mut not_inherited: Vec<(Option<NameId>, NameId)> = Vec::new();
    let mut current = Some(type_def);
    let mut depth = 0;
    while let Some(level) = current {
        if depth > schema_set.recursion_bound(50) {
            break;
        }
        let restriction = level.derivation_method != Some(DerivationMethod::Extension);
        let mut level_names = Vec::new();
        for eau in collect_effective_attribute_uses(schema_set, level) {
            let name = (eau.target_namespace, eau.name);
            if eau.use_kind == AttributeUseKind::Prohibited {
                // Clause 3.2.2 (a restriction); an extension's is ignored.
                if restriction {
                    level_names.push(name);
                }
                continue;
            }
            level_names.push(name);
            if !not_inherited.contains(&name) {
                result.push(eau);
            }
        }
        not_inherited.extend(level_names);
        current = match level.resolved_base_type {
            Some(TypeKey::Complex(base_key)) if base_key != any_type => {
                schema_set.arenas.complex_types.get(base_key)
            }
            _ => None,
        };
        depth += 1;
    }
    result
}

// ---------------------------------------------------------------------------
// §3.6.2.2 Effective Attribute Wildcard + §3.10.6.4 Intersection
// ---------------------------------------------------------------------------
//
// These helpers implement the "Common Rules for Attribute Wildcards"
// (§3.6.2.2) used by both the complex-type restriction path
// (`validate_attribute_restriction`) and the redefine attribute-group
// restriction path (`validate_all_redefine_attribute_group_restrictions`).
//
// The output is an `EffectiveAttributeWildcard` with a canonical namespace
// constraint (`CanonicalNs`) in which all `WildcardNamespace` variants have
// been normalized to either `Any`, an explicit positive set, or a
// complement set, with `not_namespace` exclusions already folded in and
// `##other` resolved to not({target namespace, absent}). Intersection then
// reduces to pure set theory on `HashSet<Option<NameId>>`.
//
// Intentionally private to this module — the canonical form never leaks
// into the arena model.

/// Canonical namespace constraint for attribute wildcards (§3.10.6.4).
///
/// All `WildcardNamespace` variants normalize to one of these three cases,
/// with `not_namespace` exclusions already folded in and `##other`
/// resolved to `Not({target namespace, absent})` (§3.10.2.2; XSD 1.0 and
/// 1.1 alike).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CanonicalNs {
    /// Every namespace is allowed.
    Any,
    /// Positive set of allowed namespaces. `None` represents the absent
    /// (no-namespace) case.
    Enum(std::collections::HashSet<Option<NameId>>),
    /// Complement set: every namespace except those in the set is allowed.
    /// `Not(empty)` is equivalent to `Any` but is preserved as-is for
    /// symmetry; `canonical_ns_subset` handles this case.
    Not(std::collections::HashSet<Option<NameId>>),
}

/// Effective attribute wildcard, the result of §3.6.2.2.
///
/// Target-namespace-free: `namespace` has already been resolved against
/// each contributor's own target namespace during normalization.
#[derive(Debug, Clone)]
pub(crate) struct EffectiveAttributeWildcard {
    pub(crate) namespace: CanonicalNs,
    pub(crate) not_qname: Vec<crate::parser::frames::NotQNameItem>,
    pub(crate) process_contents: ProcessContents,
}

/// Normalize a single `WildcardResult` into canonical form, resolving
/// `##other`, `##targetNamespace`, `##local`, list tokens, and folding in
/// `not_namespace` exclusions against `target_ns`.
pub(super) fn normalize_attribute_wildcard(
    wc: &WildcardResult,
    target_ns: Option<NameId>,
) -> EffectiveAttributeWildcard {
    use std::collections::HashSet;

    // Step 1: resolve the primary namespace constraint.
    let base: CanonicalNs = match &wc.namespace {
        WildcardNamespace::Any => CanonicalNs::Any,
        WildcardNamespace::Other => {
            // §3.10.2.2: `##other` is {variety} not with {namespaces} "a
            // set consisting of ·absent· and, if the targetNamespace
            // [attribute] of the <schema> ancestor element information
            // item is present, its ·actual value·" — in XSD 1.0 and 1.1
            // alike, for `<anyAttribute>` as for `<any>`. With no target
            // namespace the set is {absent} (the HashSet dedupes).
            let mut excl = HashSet::new();
            excl.insert(target_ns);
            excl.insert(None);
            CanonicalNs::Not(excl)
        }
        WildcardNamespace::TargetNamespace => {
            let mut s = HashSet::new();
            s.insert(target_ns);
            CanonicalNs::Enum(s)
        }
        WildcardNamespace::Local => {
            let mut s = HashSet::new();
            s.insert(None);
            CanonicalNs::Enum(s)
        }
        WildcardNamespace::List(tokens) => {
            let mut s = HashSet::new();
            for tok in tokens {
                s.insert(tok.resolve(target_ns));
            }
            CanonicalNs::Enum(s)
        }
    };

    // Step 2: fold `not_namespace` exclusions into the canonical form.
    let not_ns: HashSet<Option<NameId>> = wc
        .not_namespace
        .iter()
        .map(|t| t.resolve(target_ns))
        .collect();

    let namespace = if not_ns.is_empty() {
        base
    } else {
        match base {
            CanonicalNs::Any => CanonicalNs::Not(not_ns),
            CanonicalNs::Enum(set) => {
                let filtered: HashSet<Option<NameId>> =
                    set.into_iter().filter(|ns| !not_ns.contains(ns)).collect();
                CanonicalNs::Enum(filtered)
            }
            CanonicalNs::Not(set) => {
                let mut combined = set;
                combined.extend(not_ns);
                CanonicalNs::Not(combined)
            }
        }
    };

    EffectiveAttributeWildcard {
        namespace,
        not_qname: wc.not_qname.clone(),
        process_contents: wc.process_contents,
    }
}

/// §3.10.6.4 namespace-constraint intersection. Pure set theory on the
/// canonical lattice.
pub(super) fn intersect_canonical_ns(a: &CanonicalNs, b: &CanonicalNs) -> CanonicalNs {
    use std::collections::HashSet;
    match (a, b) {
        // Any ∩ X = X
        (CanonicalNs::Any, other) | (other, CanonicalNs::Any) => other.clone(),

        // Enum ∩ Enum = set intersection
        (CanonicalNs::Enum(s1), CanonicalNs::Enum(s2)) => {
            let inter: HashSet<Option<NameId>> = s1.intersection(s2).copied().collect();
            CanonicalNs::Enum(inter)
        }

        // Enum ∩ Not(N) = Enum \ N
        (CanonicalNs::Enum(s), CanonicalNs::Not(n))
        | (CanonicalNs::Not(n), CanonicalNs::Enum(s)) => {
            let filtered: HashSet<Option<NameId>> =
                s.iter().filter(|ns| !n.contains(ns)).copied().collect();
            CanonicalNs::Enum(filtered)
        }

        // Not(N1) ∩ Not(N2) = Not(N1 ∪ N2)
        (CanonicalNs::Not(n1), CanonicalNs::Not(n2)) => {
            let mut union = n1.clone();
            union.extend(n2.iter().copied());
            CanonicalNs::Not(union)
        }
    }
}

/// §3.10.6.3 cos-aw-union on the canonical namespace lattice.
/// Mirror of `intersect_canonical_ns` for the union side.
/// XSD 1.0 §3.10.6 "Attribute Wildcard Union" clauses 4/5: unlike XSD 1.1,
/// the 1.0 union of a negation with a set that does NOT contain the negated
/// namespace name — or of two negations of different values — is **not
/// expressible**, and a complex-type extension requiring such a union is a
/// schema error (msData wildZ013: base `##other` extended with
/// `##local b c`). XSD 1.1 §3.10.6.3 made every union expressible, so this
/// check is version-gated.
pub(super) fn validate_xsd10_attribute_wildcard_union_expressible(
    schema_set: &SchemaSet,
    derived: &crate::arenas::ComplexTypeDefData,
    base_key: ComplexTypeKey,
) -> SchemaResult<()> {
    if !schema_set.is_xsd10() {
        return Ok(());
    }
    let own_local = own_attribute_wildcard_ref(derived);
    let Some(own) = effective_attribute_wildcard(
        schema_set,
        own_local,
        derived.target_namespace,
        &derived.resolved_attribute_groups,
    )
    .ok()
    .flatten() else {
        return Ok(());
    };
    let Some(base_wc) = compute_runtime_attribute_wildcard_bounded(schema_set, base_key, 0) else {
        return Ok(());
    };

    // Recover the intensionally negated namespace name from the canonical
    // form. Under XSD 1.0 a `Not` set always stems from `##other`
    // (canonicalized as {tns, absent}); the negated *name* is the non-absent
    // member, or absent itself for a no-targetNamespace schema.
    let negated_of = |ns: &CanonicalNs| -> Option<Option<NameId>> {
        match ns {
            CanonicalNs::Not(set) => Some(
                set.iter()
                    .copied()
                    .flatten()
                    .next()
                    .map(Some)
                    .unwrap_or(None),
            ),
            _ => None,
        }
    };

    // §3.10.6 union of not(ns) with a set S (per the wording the W3C suite
    // enforces — wildZ013 vs wildZ013a):
    //   S ∋ ns and S ∋ absent  → any                (expressible)
    //   S ∋ ns, S ∌ absent     → not(absent)        (expressible)
    //   S ∌ ns, S ∋ absent     → NOT EXPRESSIBLE
    //   S ∌ ns, S ∌ absent     → not(ns)            (expressible)
    let neg_vs_set = |neg: Option<NameId>, s: &std::collections::HashSet<Option<NameId>>| -> bool {
        s.contains(&None) && !s.contains(&neg)
    };

    let not_expressible = match (&own.namespace, &base_wc.namespace) {
        (CanonicalNs::Not(_), CanonicalNs::Enum(s)) => {
            neg_vs_set(negated_of(&own.namespace).unwrap(), s)
        }
        (CanonicalNs::Enum(s), CanonicalNs::Not(_)) => {
            neg_vs_set(negated_of(&base_wc.namespace).unwrap(), s)
        }
        _ => false,
    };

    if not_expressible {
        let (location, type_name) = type_error_context(schema_set, derived);
        return Err(SchemaError::structural(
            "cos-aw-union",
            format!(
                "Complex type '{}': the union of the extension's attribute wildcard \
                 with the base's is not expressible in XSD 1.0 (§3.10.6 attribute \
                 wildcard union)",
                type_name
            ),
            location,
        ));
    }
    Ok(())
}

fn union_canonical_ns(a: &CanonicalNs, b: &CanonicalNs) -> CanonicalNs {
    use std::collections::HashSet;
    match (a, b) {
        // Any ∪ X = Any
        (CanonicalNs::Any, _) | (_, CanonicalNs::Any) => CanonicalNs::Any,

        // Enum(s1) ∪ Enum(s2) = set union
        (CanonicalNs::Enum(s1), CanonicalNs::Enum(s2)) => {
            let mut union = s1.clone();
            union.extend(s2.iter().copied());
            CanonicalNs::Enum(union)
        }

        // Enum(s) ∪ Not(n) = Not(n \ s) — every namespace b allows
        // (= everything except n) plus everything in s. The result is
        // "not (n minus s)": elements of n already in s no longer need
        // to be excluded.
        (CanonicalNs::Enum(s), CanonicalNs::Not(n))
        | (CanonicalNs::Not(n), CanonicalNs::Enum(s)) => {
            let filtered: HashSet<Option<NameId>> =
                n.iter().filter(|ns| !s.contains(ns)).copied().collect();
            if filtered.is_empty() {
                CanonicalNs::Any
            } else {
                CanonicalNs::Not(filtered)
            }
        }

        // Not(n1) ∪ Not(n2) = Not(n1 ∩ n2). A namespace is excluded by
        // the union only if it's excluded by both sides.
        (CanonicalNs::Not(n1), CanonicalNs::Not(n2)) => {
            let inter: HashSet<Option<NameId>> = n1.intersection(n2).copied().collect();
            if inter.is_empty() {
                CanonicalNs::Any
            } else {
                CanonicalNs::Not(inter)
            }
        }
    }
}

/// Canonical namespace subset: `a ⊆ b`. Tests whether every namespace
/// allowed by `a` is also allowed by `b`.
pub(super) fn canonical_ns_subset(a: &CanonicalNs, b: &CanonicalNs) -> bool {
    match (a, b) {
        // Anything ⊆ Any (Any accepts all namespaces)
        (_, CanonicalNs::Any) => true,

        // Any ⊆ Not(empty) also holds, but only when b is literally `Any`
        // after the above match. Otherwise Any is not a subset of anything
        // finite or complemented.
        (CanonicalNs::Any, _) => false,

        // Enum(s) ⊆ Enum(t) iff s ⊆ t
        (CanonicalNs::Enum(s), CanonicalNs::Enum(t)) => s.iter().all(|ns| t.contains(ns)),

        // Enum(s) ⊆ Not(n) iff s ∩ n = ∅  (no element of s is excluded by n)
        (CanonicalNs::Enum(s), CanonicalNs::Not(n)) => s.iter().all(|ns| !n.contains(ns)),

        // Not(n1) ⊆ Not(n2) iff n2 ⊆ n1  (a's exclusion set must be at
        // least as large as b's; the larger the exclusion, the smaller the
        // allowed set)
        (CanonicalNs::Not(n1), CanonicalNs::Not(n2)) => n2.iter().all(|ns| n1.contains(ns)),

        // Not(n) ⊆ Enum(s): `Not(n)` allows infinitely many namespaces, a
        // finite `Enum(s)` cannot contain them all. False.
        (CanonicalNs::Not(_), CanonicalNs::Enum(_)) => false,
    }
}

/// Intersect two effective attribute wildcards per §3.10.6.4.
///
/// - `namespace`: `intersect_canonical_ns`
/// - `not_qname`: union of both lists (deduplicated). Per §3.10.6.4
///   disallowed_names clause 3, `##defined` is preserved if present on
///   either side.
/// - `process_contents`: takes the LEFT operand's value. Callers must
///   pass the operands in the order required by §3.6.2.2 (clause 3.2.1
///   passes L first, clause 3.2.2 passes W[0] first).
fn intersect_effective_attribute_wildcards(
    a: &EffectiveAttributeWildcard,
    b: &EffectiveAttributeWildcard,
) -> EffectiveAttributeWildcard {
    let namespace = intersect_canonical_ns(&a.namespace, &b.namespace);

    // Union not_qname lists. Items whose namespace is no longer admitted
    // by the intersected constraint are redundant but harmless to keep.
    let mut not_qname = a.not_qname.clone();
    for item in &b.not_qname {
        if !not_qname.contains(item) {
            not_qname.push(item.clone());
        }
    }

    EffectiveAttributeWildcard {
        namespace,
        not_qname,
        process_contents: a.process_contents,
    }
}

/// Structural error for attribute-group reference cycles exceeding the
/// depth guard during §3.6.2.2 walking. These should already be rejected
/// by the resolver — fail loudly rather than silently synthesize `Any`.
fn attribute_group_cycle_error() -> SchemaError {
    SchemaError::structural(
        "derivation-ok-restriction",
        "attribute group reference cycle exceeded max depth while computing \
         effective attribute wildcard (§3.6.2.2)",
        None,
    )
}

/// Combine a local effective wildcard with an ordered sequence of
/// contributed effective wildcards per §3.6.2.2 clauses 3.1/3.2.1/3.2.2:
///
/// * W empty ⇒ `local` (or `None` if neither side is present).
/// * L non-absent ⇒ pc from L, intersect L with every Wi.
/// * L absent, W non-empty ⇒ pc from W[0], intersect every Wi.
fn combine_effective_wildcards(
    local: Option<EffectiveAttributeWildcard>,
    w: Vec<EffectiveAttributeWildcard>,
) -> Option<EffectiveAttributeWildcard> {
    match (local, w.is_empty()) {
        (None, true) => None,
        (Some(l), true) => Some(l),
        (Some(l), false) => Some(w.into_iter().fold(l, |acc, wi| {
            intersect_effective_attribute_wildcards(&acc, &wi)
        })),
        (None, false) => {
            let mut it = w.into_iter();
            let first = it.next().expect("w is non-empty");
            Some(it.fold(first, |acc, wi| {
                intersect_effective_attribute_wildcards(&acc, &wi)
            }))
        }
    }
}

/// §3.6.2.2 Common Rules for Attribute Wildcards.
///
/// Given a local wildcard `local_wc` (optional) and the ordered sequence
/// of resolved referenced attribute groups, compute the effective
/// attribute wildcard. Each referenced group's own effective wildcard is
/// computed recursively (so wildcards inherited through chains of
/// `<xs:attributeGroup ref=...>` references are properly intersected).
///
/// Returns `Err` if the attribute-group reference tree exceeds the depth
/// guard (cycle protection, matching `collect_attribute_group_uses`).
pub(crate) fn effective_attribute_wildcard(
    schema_set: &SchemaSet,
    local_wc: Option<&WildcardResult>,
    local_target_ns: Option<NameId>,
    attribute_groups: &[AttributeGroupKey],
) -> SchemaResult<Option<EffectiveAttributeWildcard>> {
    let local = local_wc.map(|w| normalize_attribute_wildcard(w, local_target_ns));

    let mut w: Vec<EffectiveAttributeWildcard> = Vec::new();
    for &ag_key in attribute_groups {
        collect_effective_group_wildcards(schema_set, ag_key, &mut w, 0)?;
    }

    Ok(combine_effective_wildcards(local, w))
}

/// The type's `{attribute wildcard}`: the runtime entry point for attribute
/// wildcard matching, and the base side of derivation-ok-restriction
/// (`validate_attribute_restriction`), so the two never disagree.
///
/// Returns the type's full effective `{attribute wildcard}` per §3.6.2.2
/// (intersection of own + attribute groups) chained with §3.4.2.5's
/// extension union over the base chain. Restriction picks the derived's
/// own wildcard (§3.6.2.2 "complete wildcard") with no base inheritance
/// — per §3.4.2.5 clause 2.1, a restriction's {attribute wildcard} IS
/// the complete wildcard, which is absent when no local <anyAttribute>
/// or attribute-group wildcard contributes one.
///
/// The return value is target-namespace-free: all `##targetNamespace` /
/// `##other` / list tokens have been resolved against each contributor's
/// origin target namespace, so the runtime can match attributes against
/// `EffectiveAttributeWildcard.namespace` directly.
pub(crate) fn compute_runtime_attribute_wildcard(
    schema_set: &SchemaSet,
    ct_key: ComplexTypeKey,
) -> Option<EffectiveAttributeWildcard> {
    compute_runtime_attribute_wildcard_bounded(schema_set, ct_key, 0)
}

fn compute_runtime_attribute_wildcard_bounded(
    schema_set: &SchemaSet,
    ct_key: ComplexTypeKey,
    depth: u32,
) -> Option<EffectiveAttributeWildcard> {
    if depth as usize > schema_set.recursion_bound(100) {
        return None;
    }
    let ct = schema_set.arenas.complex_types.get(ct_key)?;

    // Own §3.6.2.2 result: own xs:anyAttribute combined with all referenced
    // attribute groups via the existing canonical helpers. Errors here
    // (cycle overflow) collapse to "no wildcard" — this is the runtime
    // path; cycles are rejected upstream.
    let own_local = own_attribute_wildcard_ref(ct);
    let own = effective_attribute_wildcard(
        schema_set,
        own_local,
        ct.target_namespace,
        &ct.resolved_attribute_groups,
    )
    .ok()
    .flatten();

    let Some(TypeKey::Complex(base_key)) = ct.resolved_base_type else {
        return own;
    };
    if base_key == ct_key {
        return own;
    }

    match ct.derivation_method {
        // §3.4.2.5 clause 2.2.1.1: the ·base wildcard· is the base's
        // {attribute wildcard} — `xs:anyType`'s too (§3.4.7: {variety} any,
        // {process contents} lax), which its extensions therefore have.
        Some(DerivationMethod::Extension) => {
            let base = compute_runtime_attribute_wildcard_bounded(schema_set, base_key, depth + 1);
            match (own, base) {
                (Some(a), Some(b)) => Some(union_effective_attribute_wildcards(&a, &b)),
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                (None, None) => None,
            }
        }
        // Restriction or no derivation: derived's own wildcard is
        // authoritative per XSD §3.4.2.5 clause 2.1 ("If {derivation
        // method} = restriction, then the complete wildcard"). The
        // base's wildcard is NOT inherited — a restriction with no
        // local <anyAttribute> and no attribute-group ref wildcard
        // has {attribute wildcard} = absent. This matches sunData
        // combined/008 test.10/11.n: an `alias` element restriction
        // of a base with `<anyAttribute namespace="urn:a urn:b"/>`
        // and no own wildcard must reject all foreign-namespace
        // attributes (cvc-complex-type.3.2 / cvc-assess-attr).
        _ => own,
    }
}

/// Pull the type's "own" attribute wildcard out of either the top-level
/// field or the SimpleContent / ComplexContent derivation arm where the
/// `<xs:anyAttribute>` legitimately lives.
fn own_attribute_wildcard_ref(ct: &crate::arenas::ComplexTypeDefData) -> Option<&WildcardResult> {
    if let Some(wc) = ct.attribute_wildcard.as_ref() {
        return Some(wc);
    }
    match &ct.content {
        ComplexContentResult::Empty => None,
        ComplexContentResult::Simple(sc) => sc.attribute_wildcard.as_ref(),
        ComplexContentResult::Complex(cc) => cc.attribute_wildcard.as_ref(),
    }
}

/// §3.4.2.5 extension union of the ·complete wildcard· `a` (the extending
/// type's own) and the ·base wildcard· `b` (the base type's
/// {attribute wildcard}).
///
/// - `namespace`: `union_canonical_ns` (set-theoretic union)
/// - `not_qname`: per §3.10.6.3 cos-aw-union, the union must not admit any
///   name that neither input wildcard admits. A literal QName excluded by
///   one wildcard stays excluded in the union iff the other wildcard
///   doesn't admit it either (whether by namespace or by its own
///   disallowed_names). `##defined` / `##definedSibling` keep the simple
///   intersection rule — each is in the result iff both inputs have it.
/// - `process_contents`: the complete wildcard's — clause 2.2.2.3, "a
///   wildcard whose {process contents} and {annotations} are those of the
///   ·complete wildcard·" (XSD 1.0 §3.4.2 clause 3.2.2.1.2 says the same).
fn union_effective_attribute_wildcards(
    a: &EffectiveAttributeWildcard,
    b: &EffectiveAttributeWildcard,
) -> EffectiveAttributeWildcard {
    use crate::parser::frames::NotQNameItem;

    let namespace = union_canonical_ns(&a.namespace, &b.namespace);

    // Combine disallowed names. For each QName excluded by one side, keep
    // it excluded iff the other side also excludes it (by namespace or by
    // literal QName). For `##defined` / `##definedSibling`, the simple
    // intersection rule applies.
    let mut not_qname: Vec<NotQNameItem> = Vec::new();

    let mut consider = |item: &NotQNameItem, other: &EffectiveAttributeWildcard| match item {
        NotQNameItem::QName {
            namespace,
            local_name,
        } => {
            let admitted_by_other_ns = match &other.namespace {
                CanonicalNs::Any => true,
                CanonicalNs::Enum(set) => set.contains(namespace),
                CanonicalNs::Not(set) => !set.contains(namespace),
            };
            let excluded_by_other_qname = other.not_qname.iter().any(|o| match o {
                NotQNameItem::QName {
                    namespace: ons,
                    local_name: oln,
                } => ons == namespace && oln == local_name,
                NotQNameItem::Defined | NotQNameItem::DefinedSibling => false,
            });
            if (!admitted_by_other_ns || excluded_by_other_qname) && !not_qname.contains(item) {
                not_qname.push(item.clone());
            }
        }
        NotQNameItem::Defined | NotQNameItem::DefinedSibling => {
            if other
                .not_qname
                .iter()
                .any(|o| std::mem::discriminant(o) == std::mem::discriminant(item))
                && !not_qname.contains(item)
            {
                not_qname.push(item.clone());
            }
        }
    };

    for item in &a.not_qname {
        consider(item, b);
    }
    for item in &b.not_qname {
        consider(item, a);
    }

    EffectiveAttributeWildcard {
        namespace,
        not_qname,
        process_contents: a.process_contents,
    }
}

/// Recursive walker for `effective_attribute_wildcard`: follows
/// `resolved_ref` delegation, then iterates `resolved_attribute_groups`
/// in document order. Each referenced group's own effective wildcard is
/// computed and appended to `out` if it is non-absent (per §3.6.2.2
/// step 2 — "non-absent `{attribute wildcard}`s").
///
/// Depth guard matches `collect_attribute_group_uses` (> 20).
fn collect_effective_group_wildcards(
    schema_set: &SchemaSet,
    ag_key: AttributeGroupKey,
    out: &mut Vec<EffectiveAttributeWildcard>,
    depth: usize,
) -> SchemaResult<()> {
    if depth > schema_set.recursion_bound(20) {
        return Err(attribute_group_cycle_error());
    }

    let Some(ag) = schema_set.arenas.attribute_groups.get(ag_key) else {
        return Ok(());
    };

    if let Some(ref_key) = ag.resolved_ref {
        return collect_effective_group_wildcards(schema_set, ref_key, out, depth + 1);
    }

    if let Some(eff) = effective_attribute_wildcard_for_group(schema_set, ag, depth + 1)? {
        out.push(eff);
    }

    Ok(())
}

/// Compute the effective wildcard for a single attribute group, walking
/// `resolved_ref` delegation and `resolved_attribute_groups` recursively.
/// Separate from the top-level `effective_attribute_wildcard` so the depth
/// counter propagates correctly through nested calls.
fn effective_attribute_wildcard_for_group(
    schema_set: &SchemaSet,
    ag: &crate::arenas::AttributeGroupData,
    depth: usize,
) -> SchemaResult<Option<EffectiveAttributeWildcard>> {
    if depth > schema_set.recursion_bound(20) {
        return Err(attribute_group_cycle_error());
    }

    if let Some(ref_key) = ag.resolved_ref {
        let Some(target) = schema_set.arenas.attribute_groups.get(ref_key) else {
            return Ok(None);
        };
        return effective_attribute_wildcard_for_group(schema_set, target, depth + 1);
    }

    let local = ag
        .attribute_wildcard
        .as_ref()
        .map(|w| normalize_attribute_wildcard(w, ag.target_namespace));

    let mut w: Vec<EffectiveAttributeWildcard> = Vec::new();
    for &nested_key in &ag.resolved_attribute_groups {
        collect_effective_group_wildcards(schema_set, nested_key, &mut w, depth + 1)?;
    }

    Ok(combine_effective_wildcards(local, w))
}

/// Check that `derived` is a valid restriction of `base` (derived ⊆ base)
/// per cos-ns-subset (§3.10.6.2) on attribute wildcards.
///
/// Verifies:
/// 1. canonical namespace subset (clauses 1-4 of §3.10.6.2 on the
///    namespace constraint),
/// 2. each QName in `base.not_qname` must not be allowed by `derived`
///    (§3.10.6.2 disallowed_names clause 1). "Not allowed" covers any
///    rejection mechanism on the derived side: namespace constraint,
///    literal notQName entry, or `##defined` (which rejects any
///    globally declared attribute). This is delegated to
///    `effective_wildcard_allows_attribute` so all three mechanisms
///    are checked uniformly.
/// 3. if `base.not_qname` contains `##defined`, derived must also
///    (§3.10.6.2 clause 2); same for `##sibling` (clause 3). These
///    two keywords require literal containment, unlike QName members.
/// 4. derived processContents strictness ≥ base strictness (mirrors
///    `validate_open_content_restriction` at derivation.rs:2488-2501),
///    when `compare_process_contents` — not for a restriction of
///    `xs:anyType` (XSD 1.0 clause 4.3: "Unless the {base type definition} is
///    the ·ur-type definition·").
///
/// Takes `schema_set` because `##defined` coverage requires a lookup
/// against `schema_set.lookup_attribute` via
/// `effective_wildcard_allows_attribute`.
///
/// On failure returns `Err(reason)` so callers can build informative
/// error messages.
pub(super) fn effective_attribute_wildcard_restricts(
    schema_set: &SchemaSet,
    derived: &EffectiveAttributeWildcard,
    base: &EffectiveAttributeWildcard,
    compare_process_contents: bool,
) -> Result<(), &'static str> {
    use crate::parser::frames::NotQNameItem;

    if !canonical_ns_subset(&derived.namespace, &base.namespace) {
        return Err("namespace constraint is not a subset of the base wildcard");
    }

    // §3.10.6.2 disallowed_names clause 1: each QName member of base's
    // not_qname must not be admitted by derived. `effective_wildcard_allows_attribute`
    // correctly handles namespace-constraint rejection, literal QName
    // exclusion, and `##defined` (with schema lookup) in one pass.
    for item in &base.not_qname {
        match item {
            NotQNameItem::QName {
                namespace,
                local_name,
            } => {
                if effective_wildcard_allows_attribute(schema_set, derived, *namespace, *local_name)
                {
                    return Err(
                        "notQName exclusions do not cover the base wildcard's disallowed names",
                    );
                }
            }
            // Clause 2: `##defined` requires literal containment.
            NotQNameItem::Defined => {
                if !derived
                    .not_qname
                    .iter()
                    .any(|d| matches!(d, NotQNameItem::Defined))
                {
                    return Err("base wildcard excludes ##defined but derived does not");
                }
            }
            // Clause 3: `##definedSibling` requires literal containment.
            NotQNameItem::DefinedSibling => {
                if !derived
                    .not_qname
                    .iter()
                    .any(|d| matches!(d, NotQNameItem::DefinedSibling))
                {
                    return Err("base wildcard excludes ##definedSibling but derived does not");
                }
            }
        }
    }

    if compare_process_contents
        && process_contents_strictness(derived.process_contents)
            < process_contents_strictness(base.process_contents)
    {
        return Err("processContents is weaker than the base wildcard");
    }

    Ok(())
}

/// Does this effective wildcard admit a specific `(namespace, name)`
/// attribute?
///
/// Mirror of `wildcard_allows_attribute` (derivation.rs:3091) operating
/// on the canonical form. Preserves the load-bearing `NotQNameItem::Defined`
/// semantics documented at derivation.rs:3078-3090 — `##defined` only
/// excludes attributes that are actually globally declared, not an
/// unconditional block.
pub(crate) fn effective_wildcard_allows_attribute(
    schema_set: &SchemaSet,
    wc: &EffectiveAttributeWildcard,
    attr_namespace: Option<NameId>,
    attr_name: NameId,
) -> bool {
    // Namespace constraint check.
    let ns_ok = match &wc.namespace {
        CanonicalNs::Any => true,
        CanonicalNs::Enum(set) => set.contains(&attr_namespace),
        CanonicalNs::Not(set) => !set.contains(&attr_namespace),
    };
    if !ns_ok {
        return false;
    }

    // not_qname exclusions, including ##defined schema lookup.
    for item in &wc.not_qname {
        match item {
            crate::parser::frames::NotQNameItem::QName {
                namespace: qns,
                local_name,
            } => {
                if *qns == attr_namespace && *local_name == attr_name {
                    return false;
                }
            }
            crate::parser::frames::NotQNameItem::Defined => {
                if schema_set
                    .lookup_attribute(attr_namespace, attr_name)
                    .is_some()
                {
                    return false;
                }
            }
            crate::parser::frames::NotQNameItem::DefinedSibling => {
                // Not meaningful for attribute wildcards; ignore (matches
                // `wildcard_allows_attribute`).
            }
        }
    }

    true
}

/// Recursively expand an attribute group into effective attribute uses.
pub(super) fn collect_attribute_group_uses(
    schema_set: &SchemaSet,
    ag_key: AttributeGroupKey,
    result: &mut Vec<EffectiveAttributeUse>,
    depth: usize,
) {
    if depth > schema_set.recursion_bound(20) {
        return;
    }

    let Some(ag) = schema_set.arenas.attribute_groups.get(ag_key) else {
        return;
    };

    if let Some(ref_key) = ag.resolved_ref {
        collect_attribute_group_uses(schema_set, ref_key, result, depth + 1);
        return;
    }

    for (i, attr_use) in ag.attributes.iter().enumerate() {
        let resolved = ag.resolved_attributes.get(i);
        if let Some(eau) = resolve_single_attribute_use(schema_set, attr_use, resolved) {
            result.push(eau);
        }
    }

    for &nested_key in &ag.resolved_attribute_groups {
        collect_attribute_group_uses(schema_set, nested_key, result, depth + 1);
    }
}

/// Outcome of comparing derived and base effective attribute wildcards.
/// Shared by the complex-type restriction path and the redefine
/// attribute-group restriction path.
pub(super) enum WildcardRestrictionOutcome {
    /// Derived has no effective wildcard; any base is valid.
    DerivedAbsent,
    /// Derived has a wildcard but base has none — invalid restriction.
    AddedInDerived,
    /// Both have wildcards and the subset check failed with the given
    /// reason string.
    NotSubset(&'static str),
    /// Both have wildcards and derived is a valid restriction of base.
    Valid,
}

/// Compare two precomputed effective attribute wildcards and classify
/// the restriction relationship. The caller is responsible for deciding
/// how to report each outcome. `compare_process_contents` is false for a
/// restriction of `xs:anyType` (see [`effective_attribute_wildcard_restricts`]).
pub(super) fn classify_attribute_wildcard_restriction(
    schema_set: &SchemaSet,
    derived_eff: Option<&EffectiveAttributeWildcard>,
    base_eff: Option<&EffectiveAttributeWildcard>,
    compare_process_contents: bool,
) -> WildcardRestrictionOutcome {
    match (derived_eff, base_eff) {
        (None, _) => WildcardRestrictionOutcome::DerivedAbsent,
        (Some(_), None) => WildcardRestrictionOutcome::AddedInDerived,
        (Some(d), Some(b)) => {
            match effective_attribute_wildcard_restricts(schema_set, d, b, compare_process_contents)
            {
                Ok(()) => WildcardRestrictionOutcome::Valid,
                Err(reason) => WildcardRestrictionOutcome::NotSubset(reason),
            }
        }
    }
}

/// True when the complex type has no local attribute wildcard AND no
/// attribute groups that could contribute one — the §3.6.2.2 walk is
/// guaranteed to return `None`, so callers can skip the full computation.
fn complex_type_has_no_attribute_wildcard_source(
    type_def: &crate::arenas::ComplexTypeDefData,
) -> bool {
    type_def.attribute_wildcard.is_none() && type_def.resolved_attribute_groups.is_empty()
}

/// Validate attribute uses in a complex type restriction.
///
/// derivation-ok-restriction clause 3 (§3.4.6.3): If E's attributes satisfy
/// T's attribute constraints, they must also satisfy B's.  This means:
/// - Required attributes in the base must remain required in the derived type
///
/// derivation-ok-restriction clause 4: Attribute types in T must be validly
/// substitutable for those in B.
pub(super) fn validate_attribute_restriction(
    schema_set: &SchemaSet,
    derived: &crate::arenas::ComplexTypeDefData,
    base: &crate::arenas::ComplexTypeDefData,
) -> SchemaResult<()> {
    let derived_attrs = collect_effective_attribute_uses(schema_set, derived);
    let base_attrs = collect_inherited_attribute_uses(schema_set, base);

    let location = derived
        .source
        .as_ref()
        .and_then(|s| schema_set.source_maps.locate(s));
    let type_name = format_type_name(schema_set, derived.name, derived.target_namespace);
    let base_name = format_type_name(schema_set, base.name, base.target_namespace);

    // The base's {attribute wildcard} (§3.4.2.5): its ·complete wildcard·,
    // or — when the base is itself an extension — that combined with its
    // own base's {attribute wildcard} (clause 2.2; XSD 1.0 §3.4.2 clause
    // 3.2). It is the wildcard the runtime matches with, computed by the same
    // function, and both the attribute-use half and the wildcard half of
    // derivation-ok-restriction compare against it. Computed on first use.
    let base_key = match derived.resolved_base_type {
        Some(TypeKey::Complex(key)) => Some(key),
        _ => None,
    };
    let base_wildcard_cell = std::cell::OnceCell::new();
    let base_wildcard = || {
        base_wildcard_cell
            .get_or_init(|| {
                base_key.and_then(|k| compute_runtime_attribute_wildcard(schema_set, k))
            })
            .as_ref()
    };

    // Check clause 3: required base attributes must remain required in the
    // derived type's effective {attribute uses}.
    //
    // Per the §3.4.2.4 mapping rule, an attribute use in the base that is NOT
    // matched (by name + target namespace) by a use of the restriction — its
    // own `<attribute>` or one of its attribute groups' — nor by its own
    // prohibited `<attribute>` is inherited into the derived type's
    // {attribute uses} unchanged (an attribute group's prohibited
    // `<attribute>` is no use and removes nothing). The derived type
    // therefore satisfies clause 3 trivially for inherited attribute uses —
    // we only need to reject cases where the derived type *declares* the
    // attribute with a weaker use kind (optional) or prohibits it itself.
    for base_attr in &base_attrs {
        if base_attr.use_kind != AttributeUseKind::Required {
            continue;
        }

        // Find matching derived attribute by expanded name (namespace + local)
        let derived_attr = derived_attrs
            .iter()
            .find(|a| a.name == base_attr.name && a.target_namespace == base_attr.target_namespace);

        match derived_attr {
            // Explicit re-declaration preserves required-ness: OK.
            Some(da) if da.use_kind == AttributeUseKind::Required => {}
            // Not declared in restriction → inherited from base as required: OK.
            None => {}
            // Explicit re-declaration with weaker use (optional / prohibited):
            // the derived type's effective {attribute uses} no longer guarantees
            // presence — reject as invalid restriction.
            Some(_) => {
                let attr_name_str = schema_set.name_table.resolve(base_attr.name);
                return Err(SchemaError::structural(
                    "derivation-ok-restriction",
                    format!(
                        "Complex type '{}' restricting '{}': base type requires attribute '{}' \
                         but the derived type weakens it to optional or prohibited",
                        type_name, base_name, attr_name_str,
                    ),
                    location,
                ));
            }
        }
    }

    // Check clause 4: attribute type derivation
    for derived_attr in &derived_attrs {
        // §3.4.2.3: an <attribute> with use="prohibited" contributes no
        // attribute use to {attribute uses} — it only removes the base's.
        // Its `type` is therefore not part of the derivation at all.
        if derived_attr.use_kind == AttributeUseKind::Prohibited {
            continue;
        }
        let Some(derived_type_key) = derived_attr.resolved_type else {
            continue;
        };

        let base_attr = base_attrs.iter().find(|a| {
            a.name == derived_attr.name && a.target_namespace == derived_attr.target_namespace
        });
        let Some(base_attr) = base_attr else { continue };
        let Some(base_type_key) = base_attr.resolved_type else {
            continue;
        };

        if derived_type_key == base_type_key {
            continue;
        }

        if !is_type_derived_from(schema_set, derived_type_key, base_type_key) {
            let attr_name_str = schema_set.name_table.resolve(derived_attr.name);
            return Err(SchemaError::structural(
                "derivation-ok-restriction",
                format!(
                    "Complex type '{}' restricting '{}': attribute '{}' has a type \
                     that is not validly derived from the base attribute type",
                    type_name, base_name, attr_name_str,
                ),
                location,
            ));
        }
    }

    // §3.4.6.3 derivation-ok-restriction clause 2: an attribute use declared
    // in the restriction with no matching attribute use in the base must be
    // admitted by the base's complete {attribute wildcard} (msData ctO004:
    // unqualified attribute vs a base wildcard of ##other). xs:anyType's
    // wildcard admits everything, so restrictions of anyType are exempt.
    if let Some(base_key) = base_key {
        if base_key != schema_set.any_type_key() {
            for derived_attr in &derived_attrs {
                if derived_attr.use_kind == AttributeUseKind::Prohibited {
                    continue;
                }
                let matches_base = base_attrs.iter().any(|a| {
                    a.name == derived_attr.name
                        && a.target_namespace == derived_attr.target_namespace
                });
                if matches_base {
                    continue;
                }
                let admitted = base_wildcard().is_some_and(|wc| {
                    effective_wildcard_allows_attribute(
                        schema_set,
                        wc,
                        derived_attr.target_namespace,
                        derived_attr.name,
                    )
                });
                if !admitted {
                    let attr_name_str = schema_set.name_table.resolve(derived_attr.name);
                    return Err(SchemaError::structural(
                        "derivation-ok-restriction",
                        format!(
                            "Complex type '{}' restricting '{}': attribute '{}' matches no \
                             attribute use of the base and is not admitted by the base's \
                             attribute wildcard (derivation-ok-restriction.2)",
                            type_name, base_name, attr_name_str,
                        ),
                        location,
                    ));
                }
            }
        }
    }

    // §3.4.6.3 derivation-ok-restriction value-constraint check: when the
    // base attribute use has a `fixed` value, the derived attribute use
    // (when re-declared) must also have a `fixed` value equal to the base's.
    // A derived `default` or no value constraint over a base `fixed` is
    // invalid because it loosens the constraint.
    for base_attr in &base_attrs {
        let Some(base_fixed) = base_attr.fixed_value.as_deref() else {
            continue;
        };
        let base_fixed_ns = schema_set.value_constraint_namespaces(base_attr.fixed_source.as_ref());
        // Find re-declared derived attr matching this base attr.
        let Some(derived_attr) = derived_attrs
            .iter()
            .find(|a| a.name == base_attr.name && a.target_namespace == base_attr.target_namespace)
        else {
            // Inherited unchanged: OK.
            continue;
        };
        // Prohibited: the attribute use is removed, not re-declared with a
        // weaker value constraint (clause 3 above already rejects removing a
        // *required* base attribute).
        if derived_attr.use_kind == AttributeUseKind::Prohibited {
            continue;
        }
        match derived_attr.fixed_value.as_deref() {
            Some(d_fixed)
                if crate::validation::simple::fixed_values_equal_in(
                    d_fixed,
                    schema_set.value_constraint_namespaces(derived_attr.fixed_source.as_ref()),
                    base_fixed,
                    base_fixed_ns,
                    base_attr.resolved_type,
                    schema_set,
                ) => {}
            Some(d_fixed) => {
                let attr_name_str = schema_set.name_table.resolve(derived_attr.name);
                return Err(SchemaError::structural(
                    "derivation-ok-restriction",
                    format!(
                        "Complex type '{}' restricting '{}': attribute '{}' \
                         changes 'fixed' value from '{}' to '{}'",
                        type_name, base_name, attr_name_str, base_fixed, d_fixed,
                    ),
                    location,
                ));
            }
            None => {
                // Derived has no fixed value (either default or nothing) — too
                // loose under base's fixed constraint.
                let attr_name_str = schema_set.name_table.resolve(derived_attr.name);
                let what = if derived_attr.default_value.is_some() {
                    "uses 'default' (cannot weaken base 'fixed')"
                } else {
                    "drops the 'fixed' value constraint"
                };
                return Err(SchemaError::structural(
                    "derivation-ok-restriction",
                    format!(
                        "Complex type '{}' restricting '{}': attribute '{}' {}",
                        type_name, base_name, attr_name_str, what,
                    ),
                    location,
                ));
            }
        }
    }

    // §3.4.6.3 clause 3 / "subsumes" clause 5.3 (XSD 1.1 only):
    // for each attribute use that exists in both the base and the derived
    // type, the base's {inheritable} must equal the derived's. The default
    // binding for an attribute information item subsumes only when the
    // attribute use's inheritability matches; flipping it changes the
    // descendant attribute-inheritance graph, which is not a valid
    // restriction.
    if schema_set.is_xsd11() {
        for derived_attr in &derived_attrs {
            if derived_attr.use_kind == AttributeUseKind::Prohibited {
                continue;
            }
            let Some(base_attr) = base_attrs.iter().find(|a| {
                a.name == derived_attr.name && a.target_namespace == derived_attr.target_namespace
            }) else {
                continue;
            };
            if base_attr.inheritable != derived_attr.inheritable {
                let attr_name_str = schema_set.name_table.resolve(derived_attr.name);
                return Err(SchemaError::structural(
                    "derivation-ok-restriction",
                    format!(
                        "Complex type '{}' restricting '{}': attribute '{}' changes \
                         {{inheritable}} from {} to {}",
                        type_name,
                        base_name,
                        attr_name_str,
                        base_attr.inheritable,
                        derived_attr.inheritable,
                    ),
                    location,
                ));
            }
        }
    }

    // §3.4.6.3 clause 3 (attribute wildcard half; XSD 1.0 §3.4.6 clause 4):
    // the derived type's {attribute wildcard} — a restriction's is its
    // ·complete wildcard· (§3.4.2.5 clause 2.1, the §3.6.2.2 walk) — must
    // be a subset of the base's {attribute wildcard} (above). When the
    // derived type has no wildcard source at all, the §3.6.2.2 walk is
    // guaranteed to return `None` and the restriction is trivially valid —
    // skip both walks in that common case to avoid O(types × groups) arena
    // lookups per compile.
    if !complex_type_has_no_attribute_wildcard_source(derived) {
        let derived_eff = effective_attribute_wildcard(
            schema_set,
            derived.attribute_wildcard.as_ref(),
            derived.target_namespace,
            &derived.resolved_attribute_groups,
        )?;

        // XSD 1.0 clause 4.3 exempts a restriction of the ur-type from the
        // {process contents} comparison: "Unless the {base type definition}
        // is the ·ur-type definition·". In 1.1 an explicit
        // `<xs:restriction base="xs:anyType">` is the same component as the
        // shorthand `<xs:complexType>` (§3.4.2.6), whose wildcard is never
        // compared with anyType's, so the exemption holds there too.
        let base_is_any_type = base_key == Some(schema_set.any_type_key());
        match classify_attribute_wildcard_restriction(
            schema_set,
            derived_eff.as_ref(),
            base_wildcard(),
            !base_is_any_type,
        ) {
            WildcardRestrictionOutcome::DerivedAbsent | WildcardRestrictionOutcome::Valid => {}
            WildcardRestrictionOutcome::AddedInDerived => {
                return Err(SchemaError::structural(
                    "derivation-ok-restriction",
                    format!(
                        "Complex type '{}' restricting '{}': derived type has an attribute \
                         wildcard but the base type does not",
                        type_name, base_name,
                    ),
                    location,
                ));
            }
            WildcardRestrictionOutcome::NotSubset(reason) => {
                return Err(SchemaError::structural(
                    "derivation-ok-restriction",
                    format!(
                        "Complex type '{}' restricting '{}': attribute wildcard is not \
                         a valid restriction of the base wildcard: {}",
                        type_name, base_name, reason,
                    ),
                    location,
                ));
            }
        }
    }

    Ok(())
}
