//! Wildcard subsumption and namespace-set logic, plus the XSD 1.1
//! open-content derivation helpers (§3.10.6, §3.4.6.x open content).

#[cfg(feature = "xsd11")]
use super::normalize::{
    complex_content_particle, effective_base_content_particle, is_effectively_empty,
    normalize_type_particle, NormalizedParticle, NormalizedParticleTerm,
};
use super::normalize::{NormalizedElement, NormalizedWildcard};
#[cfg(feature = "xsd11")]
use super::{format_type_name, type_error_context};
#[cfg(feature = "xsd11")]
use crate::error::{SchemaError, SchemaResult};
use crate::ids::NameId;
#[cfg(feature = "xsd11")]
use crate::ids::{ComplexTypeKey, TypeKey};
#[cfg(feature = "xsd11")]
use crate::parser::frames::{Compositor, DerivationMethod, OpenContentMode, OpenContentResult};
use crate::parser::frames::{ProcessContents, WildcardNamespace, WildcardResult};
#[cfg(feature = "xsd11")]
use crate::schema::model::DerivationSet;
#[cfg(feature = "xsd11")]
use crate::schema::SchemaSet;

pub(super) fn wildcard_restricts(derived: &NormalizedWildcard, base: &NormalizedWildcard) -> bool {
    is_wildcard_ns_subset(
        &derived.wildcard,
        derived.target_namespace,
        &base.wildcard,
        base.target_namespace,
    ) && process_contents_strictness(derived.wildcard.process_contents)
        >= process_contents_strictness(base.wildcard.process_contents)
}

/// Check whether the derived wildcard's admissible (namespace, name) set is
/// covered by the union of the given base wildcards. Returns the indices of
/// the bases that participate in the partition (those that admit at least
/// one (ns, name) admitted by derived); returns `None` if the union doesn't
/// cover derived.
///
/// Implements the partition extension of NSSubset (§3.10.6.2): a single
/// derived wildcard restricts a base content model if every (ns, name)
/// admitted by derived is admitted by some base wildcard. Used for cases
/// like wild049 where the base type's xs:all has multiple wildcards
/// partitioning the namespace space, and the derived sequence has a single
/// wildcard whose admissions span both base wildcards.
pub(super) fn wildcard_subset_of_union(
    derived: &NormalizedWildcard,
    bases: &[&NormalizedWildcard],
) -> Option<Vec<usize>> {
    use crate::parser::frames::NotQNameItem;

    let derived_target = derived.target_namespace;

    // Collect all explicit namespace witnesses from both sides.
    let mut explicit_namespaces: Vec<Option<NameId>> = Vec::new();
    let push_ns = |ns: Option<NameId>, out: &mut Vec<Option<NameId>>| {
        if !out.contains(&ns) {
            out.push(ns);
        }
    };

    let collect_namespaces = |wc: &NormalizedWildcard, out: &mut Vec<Option<NameId>>| {
        let target = wc.target_namespace;
        match &wc.wildcard.namespace {
            WildcardNamespace::TargetNamespace => {
                if !out.contains(&target) {
                    out.push(target);
                }
            }
            WildcardNamespace::Local => {
                if !out.contains(&None) {
                    out.push(None);
                }
            }
            WildcardNamespace::List(tokens) => {
                for t in tokens {
                    let ns = t.resolve(target);
                    if !out.contains(&ns) {
                        out.push(ns);
                    }
                }
            }
            _ => {}
        }
        for t in &wc.wildcard.not_namespace {
            let ns = t.resolve(target);
            if !out.contains(&ns) {
                out.push(ns);
            }
        }
        for item in &wc.wildcard.not_qname {
            if let NotQNameItem::QName { namespace, .. } = item {
                if !out.contains(namespace) {
                    out.push(*namespace);
                }
            }
        }
    };

    collect_namespaces(derived, &mut explicit_namespaces);
    for base in bases {
        collect_namespaces(base, &mut explicit_namespaces);
    }
    push_ns(derived_target, &mut explicit_namespaces);
    for base in bases {
        push_ns(base.target_namespace, &mut explicit_namespaces);
    }
    push_ns(None, &mut explicit_namespaces);

    let mut spanned: Vec<usize> = Vec::new();

    let check_namespace = |ns: Option<NameId>,
                           is_explicit_ns: bool,
                           bases: &[&NormalizedWildcard],
                           spanned: &mut Vec<usize>|
     -> bool {
        if !wildcard_admits_ns(derived, ns) {
            return true;
        }
        // Find admitting bases (their indices in `bases`).
        let admitting: Vec<usize> = (0..bases.len())
            .filter(|&i| wildcard_admits_ns(bases[i], ns))
            .collect();
        if admitting.is_empty() {
            return false;
        }
        // For each name witness in this namespace, check coverage.
        let mut name_witnesses: Vec<NameId> = Vec::new();
        let push_name = |n: NameId, out: &mut Vec<NameId>| {
            if !out.contains(&n) {
                out.push(n);
            }
        };
        for item in &derived.wildcard.not_qname {
            if let NotQNameItem::QName {
                namespace,
                local_name,
            } = item
            {
                if *namespace == ns {
                    push_name(*local_name, &mut name_witnesses);
                }
            }
        }
        for &i in &admitting {
            for item in &bases[i].wildcard.not_qname {
                if let NotQNameItem::QName {
                    namespace,
                    local_name,
                } = item
                {
                    if *namespace == ns {
                        push_name(*local_name, &mut name_witnesses);
                    }
                }
            }
        }
        for name in &name_witnesses {
            if !wildcard_admits_qname(derived, ns, *name) {
                continue;
            }
            let any_admits = admitting
                .iter()
                .any(|&i| wildcard_admits_qname(bases[i], ns, *name));
            if !any_admits {
                return false;
            }
        }
        // "Any other name" witness: derived admits some name not in the
        // explicit witness set if and only if no `##defined`/`##definedSibling`
        // entry catches it. If derived admits this symbolic case, at least
        // one base must too.
        let derived_admits_other = wildcard_admits_qname_symbolic_other(derived, ns);
        if derived_admits_other {
            let any_admits_other = admitting
                .iter()
                .any(|&i| wildcard_admits_qname_symbolic_other(bases[i], ns));
            if !any_admits_other {
                return false;
            }
        }
        // Record participating bases. For explicit-namespace witnesses, we
        // know derived admits at least one name in this namespace, so each
        // admitting base participates; for the symbolic "any-other-namespace"
        // sentinel we'd over-count, so the caller marks those separately.
        if is_explicit_ns {
            for &i in &admitting {
                if !spanned.contains(&i) {
                    spanned.push(i);
                }
            }
        }
        true
    };

    // Check each explicit namespace witness.
    for ns in &explicit_namespaces {
        if !check_namespace(*ns, true, bases, &mut spanned) {
            return None;
        }
    }

    // Symbolic "fresh namespace" witness: a namespace not in any explicit
    // set. Use a sentinel NameId guaranteed not to appear (NameId::MAX).
    let fresh_ns = Some(NameId(u32::MAX));
    let derived_admits_fresh =
        wildcard_admits_ns(derived, fresh_ns) || wildcard_admits_ns(derived, None);
    if derived_admits_fresh {
        // The "fresh ns" sentinel covers any unmentioned namespace; we need
        // at least one base to admit it for partition coverage to work.
        let mut fresh_bases: Vec<usize> = (0..bases.len())
            .filter(|&i| wildcard_admits_ns(bases[i], fresh_ns))
            .collect();
        if wildcard_admits_ns(derived, fresh_ns) && fresh_bases.is_empty() {
            return None;
        }
        for &i in &fresh_bases {
            if !spanned.contains(&i) {
                spanned.push(i);
            }
        }
        fresh_bases.clear();
    }

    if spanned.is_empty() {
        // Derived admits nothing — degenerate, treat as covered by no bases.
        return Some(spanned);
    }
    Some(spanned)
}

/// Whether the wildcard admits `ns` in its `{namespace constraint}` after
/// applying `notNamespace`.
fn wildcard_admits_ns(wc: &NormalizedWildcard, ns: Option<NameId>) -> bool {
    if !wildcard_namespace_matches(&wc.wildcard.namespace, ns, wc.target_namespace) {
        return false;
    }
    !wc.wildcard
        .not_namespace
        .iter()
        .any(|t| t.resolve(wc.target_namespace) == ns)
}

/// Whether the wildcard admits the QName `(ns, name)` after applying
/// `notNamespace` and `notQName`. `##defined` and `##definedSibling` are
/// treated pessimistically — if either appears, the QName is rejected, since
/// at schema-time we cannot resolve which names they catch.
fn wildcard_admits_qname(wc: &NormalizedWildcard, ns: Option<NameId>, name: NameId) -> bool {
    use crate::parser::frames::NotQNameItem;
    if !wildcard_admits_ns(wc, ns) {
        return false;
    }
    !wc.wildcard.not_qname.iter().any(|item| match item {
        NotQNameItem::QName {
            namespace,
            local_name,
        } => *namespace == ns && *local_name == name,
        NotQNameItem::Defined | NotQNameItem::DefinedSibling => true,
    })
}

/// Whether the wildcard admits a "symbolic other name" in `ns` — i.e., some
/// name that doesn't appear in any explicit `notQName` entry. The check
/// fails only if `##defined`/`##definedSibling` would catch any name, since
/// concrete QName entries match only specific names.
fn wildcard_admits_qname_symbolic_other(wc: &NormalizedWildcard, ns: Option<NameId>) -> bool {
    use crate::parser::frames::NotQNameItem;
    if !wildcard_admits_ns(wc, ns) {
        return false;
    }
    !wc.wildcard
        .not_qname
        .iter()
        .any(|item| matches!(item, NotQNameItem::Defined | NotQNameItem::DefinedSibling))
}

pub(super) fn wildcard_allows_element(
    element: &NormalizedElement,
    wildcard: &NormalizedWildcard,
) -> bool {
    if !wildcard_namespace_matches(
        &wildcard.wildcard.namespace,
        element.namespace,
        wildcard.target_namespace,
    ) {
        return false;
    }

    let excluded_namespace = wildcard
        .wildcard
        .not_namespace
        .iter()
        .map(|token| token.resolve(wildcard.target_namespace))
        .any(|namespace| namespace == element.namespace);
    if excluded_namespace {
        return false;
    }

    !wildcard_not_qname_excludes(
        &wildcard.wildcard.not_qname,
        element.namespace,
        element.name,
    )
}

pub(super) fn wildcard_namespace_matches(
    namespace: &WildcardNamespace,
    element_namespace: Option<NameId>,
    target_namespace: Option<NameId>,
) -> bool {
    match namespace {
        WildcardNamespace::Any => true,
        WildcardNamespace::Other => {
            !other_exclusion_set(target_namespace).contains(&element_namespace)
        }
        WildcardNamespace::TargetNamespace => element_namespace == target_namespace,
        WildcardNamespace::Local => element_namespace.is_none(),
        WildcardNamespace::List(tokens) => tokens
            .iter()
            .map(|token| token.resolve(target_namespace))
            .any(|resolved| resolved == element_namespace),
    }
}

fn wildcard_not_qname_excludes(
    not_qname: &[crate::parser::frames::NotQNameItem],
    namespace: Option<NameId>,
    local_name: NameId,
) -> bool {
    not_qname.iter().any(|item| match item {
        crate::parser::frames::NotQNameItem::QName {
            namespace: excluded_ns,
            local_name: excluded_name,
        } => *excluded_ns == namespace && *excluded_name == local_name,
        crate::parser::frames::NotQNameItem::Defined => true,
        crate::parser::frames::NotQNameItem::DefinedSibling => false,
    })
}

// ---------------------------------------------------------------------------
// XSD 1.1: Open-content derivation helpers
// ---------------------------------------------------------------------------

/// Return the effective open content, treating `mode=None` as absent.
///
/// `compile.rs::open_content_from_result` collapses `mode=None` to `None`,
/// so derivation validation must agree: a raw `OpenContentResult` with
/// `mode=None` is semantically equivalent to no open content.
#[cfg(feature = "xsd11")]
fn effective_open_content(oc: Option<&OpenContentResult>) -> Option<&OpenContentResult> {
    oc.filter(|o| o.mode != OpenContentMode::None)
}

/// Map processContents to a strictness level (Strict=2, Lax=1, Skip=0).
pub(super) fn process_contents_strictness(pc: ProcessContents) -> u8 {
    match pc {
        ProcessContents::Strict => 2,
        ProcessContents::Lax => 1,
        ProcessContents::Skip => 0,
    }
}

/// Compute the exclusion set for `##other` per the spec (§3.10.1):
/// `namespace="##other"` maps to `not({target namespace}, absent)`.
/// The result always contains `None` (absent) and, if the target namespace
/// is present, also contains `Some(target_ns)`.
fn other_exclusion_set(target_ns: Option<NameId>) -> Vec<Option<NameId>> {
    match target_ns {
        Some(ns) => vec![Some(ns), None],
        None => vec![None],
    }
}

/// Resolve a `WildcardNamespace` to a set of effective `Option<NameId>` values
/// that the wildcard **allows** (positive set).
///
/// Returns `None` for unbounded / complement constraints (`Any`, `Other`)
/// that cannot be represented as a finite positive set — callers must handle
/// those structurally.
fn resolve_ns_set(
    wns: &WildcardNamespace,
    target_ns: Option<NameId>,
) -> Option<Vec<Option<NameId>>> {
    match wns {
        WildcardNamespace::Any | WildcardNamespace::Other => None,
        WildcardNamespace::TargetNamespace => Some(vec![target_ns]),
        WildcardNamespace::Local => Some(vec![None]),
        WildcardNamespace::List(tokens) => {
            Some(tokens.iter().map(|t| t.resolve(target_ns)).collect())
        }
    }
}

/// Check whether `derived` namespace constraint is a subset of `base`
/// (cos-ns-subset, §3.10.6.2).
///
/// Both constraints are resolved against their respective target namespaces
/// so that `##targetNamespace` and an explicit URI equal to the target
/// namespace are treated as equivalent.
///
/// Key spec detail: `##other` maps to `not({target namespace}, absent)`,
/// i.e. it **always** excludes both the target namespace and the absent
/// namespace (§3.10.1).
///
/// processContents is checked separately by the open-content derivation
/// validators.
pub(super) fn is_namespace_subset(
    derived: &WildcardNamespace,
    derived_target_ns: Option<NameId>,
    base: &WildcardNamespace,
    base_target_ns: Option<NameId>,
) -> bool {
    match base {
        WildcardNamespace::Any => true,

        WildcardNamespace::Other => {
            // base = not({base_target_ns, absent}).
            // Derived ⊆ base iff every namespace derived allows is also
            // allowed by base, i.e. is not in base's exclusion set.
            let base_excluded = other_exclusion_set(base_target_ns);

            match derived {
                WildcardNamespace::Any => false,

                WildcardNamespace::Other => {
                    // derived = not({derived_target_ns, absent}).
                    // Derived ⊆ base iff base_excluded ⊆ derived_excluded,
                    // i.e. derived excludes at least everything base excludes.
                    let derived_excluded = other_exclusion_set(derived_target_ns);
                    base_excluded.iter().all(|ns| derived_excluded.contains(ns))
                }

                _ => {
                    // Finite positive set — every allowed ns must not be in
                    // base's exclusion set.
                    match resolve_ns_set(derived, derived_target_ns) {
                        Some(resolved) => resolved.iter().all(|ns| !base_excluded.contains(ns)),
                        None => false,
                    }
                }
            }
        }

        WildcardNamespace::TargetNamespace
        | WildcardNamespace::Local
        | WildcardNamespace::List(_) => {
            // Base is a finite positive set — resolve both sides and check
            // set inclusion.
            let Some(base_set) = resolve_ns_set(base, base_target_ns) else {
                return false;
            };
            match derived {
                WildcardNamespace::Any | WildcardNamespace::Other => false,
                _ => {
                    let Some(derived_set) = resolve_ns_set(derived, derived_target_ns) else {
                        return false;
                    };
                    derived_set.iter().all(|ns| base_set.contains(ns))
                }
            }
        }
    }
}

/// Check whether `derived` wildcard's namespace constraint is a subset of
/// `base` wildcard's, also considering notNamespace and notQName exclusions.
///
/// Implements cos-ns-subset (§3.10.6.2) — a pure namespace-constraint
/// relation.  processContents is NOT checked here; callers handle it
/// separately for extension vs restriction semantics.
///
/// `derived_target_ns` / `base_target_ns` are the effective target namespaces
/// of the schema documents that contain the derived / base types.
fn is_wildcard_ns_subset(
    derived: &WildcardResult,
    derived_target_ns: Option<NameId>,
    base: &WildcardResult,
    base_target_ns: Option<NameId>,
) -> bool {
    // Namespace constraint must be a subset
    if !is_namespace_subset(
        &derived.namespace,
        derived_target_ns,
        &base.namespace,
        base_target_ns,
    ) {
        return false;
    }

    // notNamespace: for every namespace that base excludes, derived must
    // not allow it.  The naive "derived.not_namespace ⊇ base.not_namespace"
    // check over-rejects when derived's positive `{namespace constraint}`
    // already excludes the namespace by construction (e.g. derived is a
    // finite List whose members don't overlap base's notNamespace set).
    for base_excl in &base.not_namespace {
        let base_ns = base_excl.resolve(base_target_ns);
        let derived_allows =
            wildcard_namespace_matches(&derived.namespace, base_ns, derived_target_ns)
                && !derived
                    .not_namespace
                    .iter()
                    .any(|d| d.resolve(derived_target_ns) == base_ns);
        if derived_allows {
            return false;
        }
    }

    // notQName: derived must exclude at least everything base excludes that
    // derived's namespace constraint actually admits. If derived's
    // {namespace constraint} ∪ notNamespace already excludes the QName's
    // namespace, base's exclusion is moot for the subset check.
    for item in &base.not_qname {
        match item {
            crate::parser::frames::NotQNameItem::QName { namespace, .. } => {
                let derived_admits_ns =
                    wildcard_namespace_matches(&derived.namespace, *namespace, derived_target_ns)
                        && !derived
                            .not_namespace
                            .iter()
                            .any(|t| t.resolve(derived_target_ns) == *namespace);
                if derived_admits_ns && !derived.not_qname.contains(item) {
                    return false;
                }
            }
            crate::parser::frames::NotQNameItem::Defined
            | crate::parser::frames::NotQNameItem::DefinedSibling => {
                if !derived.not_qname.contains(item) {
                    return false;
                }
            }
        }
    }

    true
}

/// Validate open-content compatibility for complex type extension (cos-ct-extends).
///
/// Implements §3.4.6.2 clauses 1.4.3.2.2 by comparing the **effective**
/// `{open content}` property of each type (BOT, EOT) per §3.4.2.3 clauses
/// 4–6, rather than the raw `<xs:openContent>` child elements.
///
/// EOT inherits from the base when the derivation omits `<openContent>` or
/// specifies `mode="none"` (clause 6.1); otherwise EOT's wildcard is the
/// union (§3.10.6.3 cos-aw-union) of the derivation's wildcard with the
/// base's (clause 6.2). This lets schemas like saxonData/Open/open027 (base
/// has suffix OC, derived declares none) and open047 (derivation widens the
/// wildcard via notNamespace) pass validation.
#[cfg(feature = "xsd11")]
pub(super) fn validate_open_content_extension(
    schema_set: &SchemaSet,
    derived_key: ComplexTypeKey,
    derived: &crate::arenas::ComplexTypeDefData,
    base_key: ComplexTypeKey,
    base: &crate::arenas::ComplexTypeDefData,
) -> SchemaResult<()> {
    let bot = compute_effective_open_content(schema_set, base_key);
    let eot = compute_effective_open_content(schema_set, derived_key);

    // Clause 1.4.3.2.2.3.1: if BOT is absent, extension is unconstrained wrt OC.
    let Some(bot) = bot else {
        return Ok(());
    };

    let (location, type_name) = type_error_context(schema_set, derived);
    let base_name = format_type_name(schema_set, base.name, base.target_namespace);

    // If BOT is present then EOT must be too (by construction of EOT: if
    // derivation has no OC and no mode=none override, clause 6.1 inherits BOT).
    // Reaching `None` here means the derivation's own `<openContent mode="none"/>`
    // plus an empty explicit content type collapsed EOT to absent in a way the
    // base does not satisfy — or, without that override, that the base chain
    // produced an OC but the derived chain didn't (mismatch).
    let Some(eot) = eot else {
        return Err(SchemaError::structural(
            "cos-ct-extends",
            format!(
                "Complex type '{}' extends '{}' which has open content, \
                 but derived type has no open content",
                type_name, base_name
            ),
            location,
        ));
    };

    // Clause 1.4.3.2.2.3: either EOT.mode = interleave, or both modes = suffix.
    let mode_ok = eot.mode == OpenContentMode::Interleave
        || (bot.mode == OpenContentMode::Suffix && eot.mode == OpenContentMode::Suffix);
    if !mode_ok {
        return Err(SchemaError::structural(
            "cos-ct-extends",
            format!(
                "Complex type '{}' uses suffix open content mode but base type '{}' \
                 uses interleave mode — suffix cannot extend interleave",
                type_name, base_name
            ),
            location,
        ));
    }

    // Clause 1.4.3.2.2.4: BOT.{wildcard}.{namespace constraint} ⊆ EOT.{wildcard}.
    // Each wildcard is read against the namespace its effective open content
    // records for it: EOT inherited from BOT keeps the base's namespace
    // (`##other` is fixed by the wildcard's own schema document, §3.10.2.2).
    if let (Some(bot_wc), Some(eot_wc)) = (bot.wildcard.as_ref(), eot.wildcard.as_ref()) {
        if !is_wildcard_ns_subset(bot_wc, bot.target_namespace, eot_wc, eot.target_namespace) {
            return Err(SchemaError::structural(
                "cos-ct-extends",
                format!(
                    "Open content wildcard of '{}' is not a valid extension \
                     of base type '{}' wildcard",
                    type_name, base_name
                ),
                location,
            ));
        }
    }

    Ok(())
}

/// Effective `{open content}` property per §3.4.2.3 clauses 5–6.
///
/// Represents a non-absent open content: an absent OC is encoded as `None`
/// (returned by `compute_effective_open_content`). `target_namespace` is the
/// context for resolving any unresolved `##targetNamespace` tokens inside
/// `wildcard` — needed because a type's own `<openContent>` child is stored
/// with tokens in parser form.
#[cfg(feature = "xsd11")]
#[derive(Debug, Clone)]
struct EffectiveOpenContent {
    mode: OpenContentMode,
    wildcard: Option<WildcardResult>,
    target_namespace: Option<NameId>,
}

/// Compute the effective `{open content}` of a complex type per §3.4.2.3
/// clauses 5 and 6. Walks the base chain through extension derivations.
///
/// Returns `None` when the type's effective OC is absent.
#[cfg(feature = "xsd11")]
fn compute_effective_open_content(
    schema_set: &SchemaSet,
    key: ComplexTypeKey,
) -> Option<EffectiveOpenContent> {
    compute_effective_open_content_bounded(schema_set, key, 0)
}

#[cfg(feature = "xsd11")]
fn compute_effective_open_content_bounded(
    schema_set: &SchemaSet,
    key: ComplexTypeKey,
    depth: u32,
) -> Option<EffectiveOpenContent> {
    // Guard against pathological cycles — reference resolution should have
    // detected them upstream, but keep a local belt-and-braces cap.
    if depth as usize > schema_set.recursion_bound(100) {
        return None;
    }
    let type_data = schema_set.arenas.complex_types.get(key)?;
    let target_ns = type_data.target_namespace;

    // Clause 5: select the "wildcard element" (the OC source for this type).
    // Clause 5.1 picks the <xs:openContent> child element regardless of its
    // @mode — a literal `mode="none"` still "corresponds" to the element per
    // the spec.  The clause-6.1 mode=none branch below handles that case,
    // short-circuiting the defaultOpenContent fallback that 5.2 would apply.
    let own_oc: Option<EffectiveOpenContent> =
        type_data
            .open_content
            .as_ref()
            .map(|oc| EffectiveOpenContent {
                mode: oc.mode,
                wildcard: oc.wildcard.clone(),
                target_namespace: target_ns,
            });

    let wildcard_element: Option<EffectiveOpenContent> = if own_oc.is_some() {
        // Clause 5.1
        own_oc
    } else if let Some(default) = type_data
        .source
        .as_ref()
        .and_then(|s| schema_set.documents.get(s.defaults_doc() as usize))
        .and_then(|d| d.default_open_content.as_ref())
    {
        // Clause 5.2: schema-level <xs:defaultOpenContent> applies when
        // appliesToEmpty=true OR the explicit content type is non-empty.
        if default.applies_to_empty || !explicit_content_is_empty(schema_set, type_data, 0) {
            default_open_content_to_effective(default, target_ns)
        } else {
            None
        }
    } else {
        None // Clause 5.3
    };

    // Base's effective OC (clause 4.2 inheritance). Only inherited across
    // extension; for restriction or anyType-derivation the base OC does not
    // flow into the derived explicit content type.
    let base_oc: Option<EffectiveOpenContent> = if matches!(
        type_data.derivation_method,
        Some(DerivationMethod::Extension)
    ) {
        match type_data.resolved_base_type {
            Some(TypeKey::Complex(base_key)) => {
                compute_effective_open_content_bounded(schema_set, base_key, depth + 1)
            }
            _ => None,
        }
    } else {
        None
    };

    // Clause 6.1: absent / mode=None wildcard element → inherit base.
    let Some(we) = wildcard_element else {
        return base_oc;
    };
    if we.mode == OpenContentMode::None {
        return base_oc;
    }

    // Clause 6.2: build a new OC record with the unioned wildcard.
    let wildcard = match (base_oc.as_ref(), we.wildcard.as_ref()) {
        (Some(b), Some(w)) => match &b.wildcard {
            Some(bw) => Some(wildcard_result_union(bw, b.target_namespace, w, target_ns)),
            None => Some(w.clone()),
        },
        (None, Some(w)) => Some(w.clone()),
        (Some(b), None) => b.wildcard.clone(),
        (None, None) => None,
    };

    Some(EffectiveOpenContent {
        mode: we.mode,
        wildcard,
        target_namespace: target_ns,
    })
}

/// Determine whether a complex type's *explicit content type* is empty per
/// §3.4.2.3 clause 3 (needed for clause 5.2.2's `appliesToEmpty` gate).
///
/// For extension with no derivation-level particle, the explicit content is
/// the base's — so recurse. For restriction or non-derivation types the
/// explicit content is the derivation's own content.
#[cfg(feature = "xsd11")]
fn explicit_content_is_empty(
    schema_set: &SchemaSet,
    type_data: &crate::arenas::ComplexTypeDefData,
    depth: u32,
) -> bool {
    if depth as usize > schema_set.recursion_bound(100) {
        return true;
    }
    // Use the §3.4.2.3 5.2.2 gate (explicit content type variety = empty),
    // which incorporates the effective-mixed promotion of step 3.1.1.
    if !type_data.content.explicit_content_type_is_empty() {
        return false;
    }
    if matches!(
        type_data.derivation_method,
        Some(DerivationMethod::Extension)
    ) {
        if let Some(TypeKey::Complex(base_key)) = type_data.resolved_base_type {
            if let Some(base_data) = schema_set.arenas.complex_types.get(base_key) {
                return explicit_content_is_empty(schema_set, base_data, depth + 1);
            }
        }
    }
    true
}

/// Convert a `DefaultOpenContent` (schema-model form built from the
/// `<xs:defaultOpenContent>` element) into the `EffectiveOpenContent` form
/// used during derivation validation.
#[cfg(feature = "xsd11")]
fn default_open_content_to_effective(
    default: &crate::schema::model::DefaultOpenContent,
    target_ns: Option<NameId>,
) -> Option<EffectiveOpenContent> {
    let mode = match default.mode {
        crate::schema::model::OpenContentMode::None => OpenContentMode::None,
        crate::schema::model::OpenContentMode::Interleave => OpenContentMode::Interleave,
        crate::schema::model::OpenContentMode::Suffix => OpenContentMode::Suffix,
    };
    if mode == OpenContentMode::None {
        return None;
    }
    let wildcard = default.wildcard.as_ref().map(element_wildcard_to_result);
    Some(EffectiveOpenContent {
        mode,
        wildcard,
        target_namespace: target_ns,
    })
}

/// Convert a schema-model `ElementWildcard` to a parser-form `WildcardResult`
/// so it can share the subset / union helpers below.
#[cfg(feature = "xsd11")]
fn element_wildcard_to_result(ew: &crate::schema::wildcard::ElementWildcard) -> WildcardResult {
    use crate::parser::frames::NotQNameItem;
    use crate::schema::wildcard::{NamespaceConstraint, QNameDisallowed};

    let (namespace, not_namespace) = match &ew.namespace_constraint {
        NamespaceConstraint::Any => (WildcardNamespace::Any, Vec::new()),
        NamespaceConstraint::Other => (WildcardNamespace::Other, Vec::new()),
        NamespaceConstraint::Enumeration(nss) => (
            WildcardNamespace::List(nss.iter().copied().map(ns_token).collect()),
            Vec::new(),
        ),
        NamespaceConstraint::Not(nss) => (
            WildcardNamespace::Any,
            nss.iter().copied().map(ns_token).collect(),
        ),
    };

    let process_contents = match ew.process_contents {
        crate::schema::wildcard::ProcessContents::Strict => ProcessContents::Strict,
        crate::schema::wildcard::ProcessContents::Lax => ProcessContents::Lax,
        crate::schema::wildcard::ProcessContents::Skip => ProcessContents::Skip,
    };

    let not_qname = ew
        .not_qnames
        .iter()
        .map(|q| match q {
            QNameDisallowed::QName {
                namespace,
                local_name,
            } => NotQNameItem::QName {
                namespace: *namespace,
                local_name: *local_name,
            },
            QNameDisallowed::Defined => NotQNameItem::Defined,
            QNameDisallowed::DefinedSibling => NotQNameItem::DefinedSibling,
        })
        .collect();

    WildcardResult {
        namespace,
        process_contents,
        not_namespace,
        not_qname,
        id: ew.id.clone(),
        annotation: None,
        source: ew.source.clone(),
    }
}

/// Canonical namespace form: finite allowed set, or finite excluded set
/// (complement in the "namespace universe").
#[cfg(feature = "xsd11")]
#[derive(Debug, Clone)]
enum NsForm {
    Pos(Vec<Option<NameId>>),
    Neg(Vec<Option<NameId>>),
}

/// Normalise a wildcard's `{namespace constraint}` into canonical form,
/// resolving `##targetNamespace`/`##local` tokens and merging `notNamespace`
/// into the excluded set.
#[cfg(feature = "xsd11")]
fn wildcard_to_ns_form(
    ns: &WildcardNamespace,
    not_namespace: &[crate::parser::frames::NamespaceToken],
    target_ns: Option<NameId>,
) -> NsForm {
    let resolved_not: Vec<Option<NameId>> =
        not_namespace.iter().map(|t| t.resolve(target_ns)).collect();
    match ns {
        WildcardNamespace::Any => NsForm::Neg(resolved_not),
        WildcardNamespace::Other => {
            let mut excl = other_exclusion_set(target_ns);
            for r in resolved_not {
                if !excl.contains(&r) {
                    excl.push(r);
                }
            }
            NsForm::Neg(excl)
        }
        WildcardNamespace::TargetNamespace => {
            let base = target_ns;
            if resolved_not.contains(&base) {
                NsForm::Pos(Vec::new())
            } else {
                NsForm::Pos(vec![base])
            }
        }
        WildcardNamespace::Local => {
            if resolved_not.contains(&None) {
                NsForm::Pos(Vec::new())
            } else {
                NsForm::Pos(vec![None])
            }
        }
        WildcardNamespace::List(tokens) => {
            let allowed: Vec<Option<NameId>> = tokens
                .iter()
                .map(|t| t.resolve(target_ns))
                .filter(|r| !resolved_not.contains(r))
                .collect();
            NsForm::Pos(allowed)
        }
    }
}

/// Convert a canonical `NsForm` back into `(WildcardNamespace, not_namespace)`
/// pair suitable for a `WildcardResult`. Any excluded-set result that's empty
/// collapses to `##any`; a non-empty excluded set becomes `##any` with
/// `notNamespace` tokens.
#[cfg(feature = "xsd11")]
fn ns_form_to_wildcard(
    form: NsForm,
) -> (
    WildcardNamespace,
    Vec<crate::parser::frames::NamespaceToken>,
) {
    use crate::parser::frames::NamespaceToken;
    match form {
        NsForm::Pos(list) => {
            let tokens: Vec<NamespaceToken> = list.into_iter().map(ns_token).collect();
            (WildcardNamespace::List(tokens), Vec::new())
        }
        NsForm::Neg(list) if list.is_empty() => (WildcardNamespace::Any, Vec::new()),
        NsForm::Neg(list) => {
            let tokens: Vec<NamespaceToken> = list.into_iter().map(ns_token).collect();
            (WildcardNamespace::Any, tokens)
        }
    }
}

/// Convert a resolved namespace (`Some(id)` = URI, `None` = absent/local) into
/// a parser-form `NamespaceToken`. Used by the open-content derivation helpers
/// to reconstruct parser-form wildcards from canonicalised lists.
#[cfg(feature = "xsd11")]
fn ns_token(ns: Option<NameId>) -> crate::parser::frames::NamespaceToken {
    match ns {
        Some(id) => crate::parser::frames::NamespaceToken::Uri(id),
        None => crate::parser::frames::NamespaceToken::Local,
    }
}

/// Wildcard union per §3.10.6.3 cos-aw-union, restricted to the namespace
/// constraint portion. `notQName` items are intersected (an excluded QName
/// stays excluded only if both wildcards exclude it). `processContents` is
/// inherited from `a` (convention: `a` is the derivation's own `<any>`).
///
/// Tokens in the produced `WildcardResult` are already resolved against the
/// input target namespaces, so the caller does not need to supply one.
#[cfg(feature = "xsd11")]
pub(crate) fn wildcard_result_union(
    a: &WildcardResult,
    a_tns: Option<NameId>,
    b: &WildcardResult,
    b_tns: Option<NameId>,
) -> WildcardResult {
    let form_a = wildcard_to_ns_form(&a.namespace, &a.not_namespace, a_tns);
    let form_b = wildcard_to_ns_form(&b.namespace, &b.not_namespace, b_tns);

    let merged = match (form_a, form_b) {
        (NsForm::Pos(mut pa), NsForm::Pos(pb)) => {
            for item in pb {
                if !pa.contains(&item) {
                    pa.push(item);
                }
            }
            NsForm::Pos(pa)
        }
        (NsForm::Pos(pa), NsForm::Neg(nb)) | (NsForm::Neg(nb), NsForm::Pos(pa)) => {
            NsForm::Neg(nb.into_iter().filter(|ns| !pa.contains(ns)).collect())
        }
        (NsForm::Neg(na), NsForm::Neg(nb)) => {
            NsForm::Neg(na.into_iter().filter(|ns| nb.contains(ns)).collect())
        }
    };

    let (namespace, not_namespace) = ns_form_to_wildcard(merged);

    let not_qname: Vec<crate::parser::frames::NotQNameItem> = a
        .not_qname
        .iter()
        .filter(|item| b.not_qname.contains(item))
        .cloned()
        .collect();

    WildcardResult {
        namespace,
        process_contents: a.process_contents,
        not_namespace,
        not_qname,
        id: None,
        annotation: None,
        source: a.source.clone(),
    }
}

/// True when the derived complex type's explicit particle is absent or
/// normalizes to an empty group (`<xs:sequence/>`, `<xs:all/>`, or a group
/// whose children all prune away). Used to decide when the stricter
/// open-content restriction checks can be safely relaxed: if the derived
/// particle contributes no elements to the content language, the derived
/// type's language comes entirely from its open content wildcard, so the
/// mode-and-subset checks against the base reduce to a pure wildcard-
/// subset check (cos-ns-subset).
#[cfg(feature = "xsd11")]
fn derived_particle_is_empty(
    schema_set: &SchemaSet,
    derived: &crate::arenas::ComplexTypeDefData,
) -> bool {
    let Some(particle) = complex_content_particle(&derived.content) else {
        return true;
    };
    let Ok(normalized) = normalize_type_particle(schema_set, derived, particle) else {
        return false;
    };
    is_effectively_empty(&normalized)
}

/// Extract a wildcard from a base type's effective content particle when it
/// is a single wildcard (optionally wrapped in a pointless sequence/all
/// group). Returns `None` when the base has element particles that would
/// require the derived type's empty particle to not be a valid restriction.
///
/// This supports §3.4.6.4 clause 1 (language containment) for the narrow
/// case where the base has no `<xs:openContent>` but its content model is
/// a single wildcard — which is language-equivalent to having
/// interleave/suffix open content over that same wildcard.
#[cfg(feature = "xsd11")]
fn base_content_single_wildcard<'a>(
    schema_set: &'a SchemaSet,
    base: &'a crate::arenas::ComplexTypeDefData,
) -> Option<NormalizedWildcard> {
    let (particle_owner, particle) = effective_base_content_particle(schema_set, base);
    let particle = particle?;
    let normalized = normalize_type_particle(schema_set, particle_owner, particle).ok()?;
    match &normalized.term {
        NormalizedParticleTerm::Wildcard(wc) => Some((**wc).clone()),
        NormalizedParticleTerm::Group(group) => {
            if group.particles.len() == 1 {
                if let NormalizedParticleTerm::Wildcard(wc) = &group.particles[0].term {
                    return Some((**wc).clone());
                }
            }
            None
        }
        _ => None,
    }
}

/// XSD 1.1 §3.4.6.4 schema-time EDC for all-group restrictions
/// (cvc-complex-type rule 5 / cos-element-consistent extended). When a
/// derived all-group restricts a base all-group and removes a base local
/// element, the derived's wildcard can structurally admit elements with the
/// removed QName. The "tighter EDC rule" of XSD 1.1 (Saxon test category
/// `xsd1_1-Wildcards-TighterMatchingRuleForEDC`, e.g. wild069) demands the
/// schema be invalid if the wildcard's governing type for that QName is not
/// validly substitutable for the base local's declared type.
///
/// The xs:sequence variant of the same construct (wild068) is intentionally
/// not subject to this check: the position constraint of sequence keeps the
/// conflict from arising structurally, leaving runtime dynamic EDC as the
/// catcher.
#[cfg(feature = "xsd11")]
pub(super) fn validate_all_group_restriction_edc(
    schema_set: &SchemaSet,
    derived: &crate::arenas::ComplexTypeDefData,
    base: &crate::arenas::ComplexTypeDefData,
) -> SchemaResult<()> {
    use crate::parser::frames::ProcessContents;

    let derived_particle = complex_content_particle(&derived.content);
    let (effective_base, base_particle) = effective_base_content_particle(schema_set, base);

    let (Some(derived_p), Some(base_p)) = (derived_particle, base_particle) else {
        return Ok(());
    };

    let derived_norm = match normalize_type_particle(schema_set, derived, derived_p) {
        Ok(n) => n,
        Err(_) => return Ok(()),
    };
    let base_norm = match normalize_type_particle(schema_set, effective_base, base_p) {
        Ok(n) => n,
        Err(_) => return Ok(()),
    };

    // Trigger only when both top-level groups are xs:all.
    if !is_top_all_group(&derived_norm) || !is_top_all_group(&base_norm) {
        return Ok(());
    }

    // Collect derived's local element QNames and wildcards (top-level only —
    // an all-group's particles are flat).
    let mut derived_local_qnames: Vec<(Option<NameId>, NameId)> = Vec::new();
    let mut derived_wildcards: Vec<&NormalizedWildcard> = Vec::new();
    if let NormalizedParticleTerm::Group(group) = &derived_norm.term {
        for p in &group.particles {
            match &p.term {
                NormalizedParticleTerm::Element(elem) => {
                    derived_local_qnames.push((elem.namespace, elem.name));
                }
                NormalizedParticleTerm::Wildcard(wc) => {
                    derived_wildcards.push(wc.as_ref());
                }
                NormalizedParticleTerm::Group(_) => {}
            }
        }
    }

    if derived_wildcards.is_empty() {
        return Ok(());
    }

    // Walk base's local elements (top-level all-group particles).
    let base_locals: Vec<(Option<NameId>, NameId, TypeKey)> =
        if let NormalizedParticleTerm::Group(group) = &base_norm.term {
            group
                .particles
                .iter()
                .filter_map(|p| match &p.term {
                    NormalizedParticleTerm::Element(elem) => {
                        Some((elem.namespace, elem.name, elem.type_key))
                    }
                    _ => None,
                })
                .collect()
        } else {
            Vec::new()
        };

    let (location, type_name) = type_error_context(schema_set, derived);
    let base_name = format_type_name(schema_set, base.name, base.target_namespace);

    for (l_ns, l_name, l_type) in &base_locals {
        // Skip if derived also has this local element.
        if derived_local_qnames.contains(&(*l_ns, *l_name)) {
            continue;
        }

        for wc in &derived_wildcards {
            if !wildcard_admits_qname(wc, *l_ns, *l_name) {
                continue;
            }

            let pc = wc.wildcard.process_contents;
            if matches!(pc, ProcessContents::Skip) {
                continue;
            }

            let global_key = schema_set.lookup_element(*l_ns, *l_name);
            let governing_type = global_key
                .and_then(|k| schema_set.arenas.elements.get(k))
                .and_then(|d| d.resolved_type);

            match (pc, governing_type) {
                (ProcessContents::Strict, None) => {
                    // strict + no global: instance with this QName fails strict
                    // wildcard validation, so derived rejects. No conflict.
                    continue;
                }
                (ProcessContents::Lax, None) => {
                    // lax + no global: wildcard skip-validates, so derived
                    // admits arbitrary content for this QName. Base local
                    // would enforce its type — derived broader → reject.
                }
                (_, Some(gov_type)) => {
                    if schema_set.is_type_derived_from(gov_type, *l_type, DerivationSet::empty()) {
                        continue;
                    }
                }
                _ => continue,
            }

            return Err(SchemaError::structural(
                "cos-element-consistent",
                format!(
                    "Complex type '{}' restricts '{}' (xs:all) by removing local element \
                     while keeping a wildcard that admits the same QName; the wildcard's \
                     governing type is not validly substitutable for the base local element's \
                     type (cvc-complex-type rule 5 / tighter EDC for xs:all restriction)",
                    type_name, base_name,
                ),
                location.clone(),
            ));
        }
    }

    Ok(())
}

/// Whether a normalized particle is a top-level xs:all group (with min/max
/// = 1, since xs:all allows minOccurs ∈ {0,1} and maxOccurs = 1).
#[cfg(feature = "xsd11")]
fn is_top_all_group(particle: &NormalizedParticle) -> bool {
    matches!(
        &particle.term,
        NormalizedParticleTerm::Group(group) if group.compositor == Compositor::All
    )
}

/// Validate open-content compatibility for complex type restriction
/// (derivation-ok-restriction).
///
/// Rules:
/// - If base has no OC, derived must not add OC — **unless** the derived
///   particle is empty and the base's content model is a single wildcard
///   that subsumes the derived OC's wildcard (language-equivalent case).
/// - If base has OC but derived doesn't — OK (restriction removes it).
/// - Interleave cannot restrict suffix — **unless** the derived particle
///   is empty, in which case the mode choice is irrelevant because the
///   derived language is the wildcard closure.
/// - Derived wildcard must be a subset of base wildcard.
#[cfg(feature = "xsd11")]
pub(super) fn validate_open_content_restriction(
    schema_set: &SchemaSet,
    derived: &crate::arenas::ComplexTypeDefData,
    base: &crate::arenas::ComplexTypeDefData,
) -> SchemaResult<()> {
    let base_oc = effective_open_content(base.open_content.as_ref());
    let derived_oc = effective_open_content(derived.open_content.as_ref());

    // If base has no open content, derived must not add one — except when
    // the derived particle is empty and the base's single-wildcard particle
    // subsumes the derived OC wildcard (language containment, §3.4.6.4).
    if base_oc.is_none() && derived_oc.is_some() {
        if derived_particle_is_empty(schema_set, derived) {
            let derived_oc_wc = derived_oc.as_ref().and_then(|o| o.wildcard.as_ref());
            if let Some(d_wc) = derived_oc_wc {
                if let Some(base_wc) = base_content_single_wildcard(schema_set, base) {
                    if is_wildcard_ns_subset(
                        d_wc,
                        derived.target_namespace,
                        &base_wc.wildcard,
                        base_wc.target_namespace,
                    ) {
                        return Ok(());
                    }
                }
            }
        }
        let (location, type_name) = type_error_context(schema_set, derived);
        let base_name = format_type_name(schema_set, base.name, base.target_namespace);
        return Err(SchemaError::structural(
            "derivation-ok-restriction",
            format!(
                "Complex type '{}' restricts '{}' which has no open content, \
                 but adds open content — not allowed",
                type_name, base_name
            ),
            location,
        ));
    }

    // If base has OC but derived doesn't — OK (restriction removes it)
    let (Some(base_oc), Some(derived_oc)) = (base_oc, derived_oc) else {
        return Ok(());
    };

    let (location, type_name) = type_error_context(schema_set, derived);
    let base_name = format_type_name(schema_set, base.name, base.target_namespace);

    // Mode: if base is suffix, derived cannot use interleave — unless the
    // derived particle is empty, in which case the derived language is just
    // the wildcard closure and the mode choice is irrelevant.
    if base_oc.mode == OpenContentMode::Suffix
        && derived_oc.mode == OpenContentMode::Interleave
        && !derived_particle_is_empty(schema_set, derived)
    {
        return Err(SchemaError::structural(
            "derivation-ok-restriction",
            format!(
                "Complex type '{}' uses interleave open content mode but base type '{}' \
                 uses suffix mode — interleave cannot restrict suffix",
                type_name, base_name
            ),
            location,
        ));
    }

    // Wildcard: derived must be subset of base
    if let (Some(base_wc), Some(derived_wc)) =
        (base_oc.wildcard.as_ref(), derived_oc.wildcard.as_ref())
    {
        if !is_wildcard_ns_subset(
            derived_wc,
            derived.target_namespace,
            base_wc,
            base.target_namespace,
        ) {
            return Err(SchemaError::structural(
                "derivation-ok-restriction",
                format!(
                    "Open content wildcard of '{}' is not a valid restriction \
                     of base type '{}' wildcard",
                    type_name, base_name
                ),
                location,
            ));
        }

        // processContents: restriction must be at least as strict
        if process_contents_strictness(derived_wc.process_contents)
            < process_contents_strictness(base_wc.process_contents)
        {
            return Err(SchemaError::structural(
                "derivation-ok-restriction",
                format!(
                    "Open content wildcard of '{}' has weaker processContents \
                     than base type '{}' wildcard",
                    type_name, base_name
                ),
                location,
            ));
        }
    }

    Ok(())
}
