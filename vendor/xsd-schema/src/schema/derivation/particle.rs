//! §3.9.6 Particle Valid (Restriction): the particle subsumption core
//! that decides whether a derived particle restricts a base particle.

use super::counts::{Count, Counts};
use super::format_type_name;
use super::normalize::{
    collapse_single_child_groups, complex_content_particle, effective_base_content_particle,
    flatten_nested_sequences, flatten_same_compositor_groups, fold_single_child_group,
    folded_occurrences, folded_occurrences_under, is_effectively_empty, normalize_type_particle,
    normalized_effective_base_particle, occurs_is_unit, remove_pointless_particles,
    NormalizedElement, NormalizedGroup, NormalizedParticle, NormalizedParticleTerm,
    NormalizedWildcard,
};
use super::wildcard::{
    process_contents_strictness, wildcard_allows_element, wildcard_restricts,
    wildcard_subset_of_union,
};
use crate::error::{SchemaError, SchemaResult};
use crate::parser::frames::{ComplexContentResult, Compositor};
use crate::schema::model::DerivationSet;
use crate::schema::SchemaSet;

pub(super) fn validate_content_particle_restriction(
    schema_set: &SchemaSet,
    derived: &crate::arenas::ComplexTypeDefData,
    base: &crate::arenas::ComplexTypeDefData,
) -> SchemaResult<()> {
    let derived_particle = complex_content_particle(&derived.content);
    // For the base, resolve effective content by walking up the extension
    // chain: an empty extension inherits its base's content model, and a
    // non-empty one is `sequence(inherited, own)` per §3.4.2.3.
    let (effective_base, _) = effective_base_content_particle(schema_set, base);
    // That sequence, like a nested `<sequence>` written in the base or
    // reached through a model-group reference, is a pointless sequence of
    // XSD 1.0 §3.9.6 clause 2.2. The parts were normalized before the chain
    // was assembled (and XSD 1.1 never flattens), so flatten the whole base.
    let base_particle =
        normalized_effective_base_particle(schema_set, base, 0)?.map(flatten_nested_sequences);

    let location = derived
        .source
        .as_ref()
        .and_then(|s| schema_set.source_maps.locate(s));
    let type_name = format_type_name(schema_set, derived.name, derived.target_namespace);
    let base_name = format_type_name(schema_set, base.name, base.target_namespace);

    match (derived_particle, base_particle) {
        (None, None) => Ok(()),
        (Some(derived_particle), None) => {
            let derived_particle = normalize_type_particle(schema_set, derived, derived_particle)?;
            // Empty derived particle can restrict an empty-particle base, but not simpleContent (different violation).
            if !matches!(effective_base.content, ComplexContentResult::Simple(_))
                && is_effectively_empty(&derived_particle)
            {
                Ok(())
            } else {
                Err(SchemaError::structural(
                    "derivation-ok-restriction",
                    format!(
                        "Complex type '{}' adds particle content while restricting '{}' which has empty content",
                        type_name, base_name
                    ),
                    location,
                ))
            }
        }
        (None, Some(base_particle)) => {
            if particle_is_emptiable(&base_particle) {
                Ok(())
            } else {
                Err(SchemaError::structural(
                    "derivation-ok-restriction",
                    format!(
                        "Complex type '{}' removes required particle content from base type '{}'",
                        type_name, base_name
                    ),
                    location,
                ))
            }
        }
        (Some(derived_particle), Some(base_particle)) => {
            let derived_particle = normalize_type_particle(schema_set, derived, derived_particle)?;

            // XSD 1.1: a derived particle that accepts no sequence at all (a
            // required choice whose branches all map to no component) has an
            // empty language, a subset of any (§3.4.6.4 clause 1). XSD 1.0
            // takes an empty group as empty content, as before.
            if schema_set.is_xsd11() && particle_accepts_nothing(&derived_particle) {
                return Ok(());
            }
            if is_effectively_empty(&derived_particle) {
                if particle_is_emptiable(&base_particle) {
                    return Ok(());
                } else {
                    return Err(SchemaError::structural(
                        "derivation-ok-restriction",
                        format!(
                            "Complex type '{}' removes required particle content from base type '{}'",
                            type_name, base_name
                        ),
                        location,
                    ));
                }
            }

            // All compositor combinations are now handled by
            // particle_restricts: same-compositor, sequence→choice,
            // sequence→all, choice expansion, and the catch-all rejection
            // for structurally forbidden pairs like all→choice.

            if particle_restricts(schema_set, &derived_particle, &base_particle) {
                Ok(())
            } else {
                Err(SchemaError::structural(
                    "derivation-ok-restriction",
                    format!(
                        "Content model of '{}' is not a valid restriction of base type '{}'",
                        type_name, base_name
                    ),
                    location,
                ))
            }
        }
    }
}

/// Does `derived` restrict `base` (§3.9.6 for XSD 1.0, §3.4.6.4 for 1.1)?
///
/// **Termination.** Every recursive call compares a strictly smaller pair:
/// a fold (`fold_single_child_group`, `under_folded_base`, `over_folded`,
/// a folded branch) removes a group node from one side, the structural
/// rules recurse into children, and the mixing rules compare a derived
/// branch (a child) with a base or a base copy with new counts but no new
/// nodes. Two steps add a node: the XSD 1.1 re-expansion into the base's
/// compositor (never applied to a group that already has it, and followed
/// by the base's children), and the XSD 1.0 comparison of a single-child
/// group as written ([`unpointless_group_as_written`]), which goes one fold
/// back each time along a finite chain and then descends structurally. The
/// loops here run over the children of finite groups; the count comparisons
/// themselves are bounded (`counts.rs`).
pub(super) fn particle_restricts(
    schema_set: &SchemaSet,
    derived: &NormalizedParticle,
    base: &NormalizedParticle,
) -> bool {
    // XSD 1.1: a derived particle that accepts the empty sequence and
    // nothing else (an empty group kept as a choice's empty alternative, a
    // group whose term accepts nothing and may occur zero times) restricts
    // exactly the bases that accept it; one that accepts nothing restricts
    // any (§3.4.6.4 clause 1).
    if schema_set.is_xsd11() {
        match degenerate_language(derived) {
            Some(Degenerate::OnlyEmpty) => return particle_is_emptiable(base),
            Some(Degenerate::Nothing) => return true,
            None => {}
        }
    }
    // XSD 1.1 intensional restriction: fold single-child sequence/all groups
    // symmetrically on both sides so they are compared in the same normal form.
    // The fold keeps the exact counts (`sequence{1,2}(b{2,2})` admits two or
    // four `b`s, never three — particlesHa147), and every comparison of
    // occurrences below compares those sets.
    if schema_set.is_xsd11() {
        if let Some(folded) = fold_single_child_group(derived) {
            // A fold with gaps or beyond `u32`, into a child whose term
            // accepts the empty sequence, or of one group into another loses
            // structure that an identical or narrowed base group would
            // match: try both.
            let lossy = folded.counts.is_some()
                || matches!(&derived.term, NormalizedParticleTerm::Group(group)
                if group.particles.first().is_some_and(|child| {
                    term_is_emptiable(&child.term)
                        || matches!(child.term, NormalizedParticleTerm::Group(_))
                }));
            return particle_restricts(schema_set, &folded, base)
                || lossy && particle_restricts_unfolded(schema_set, derived, base);
        }
        if is_unfoldable_group(derived, &[Compositor::Sequence, Compositor::All]) {
            // A derived group whose counts are not one set: as it is (an
            // identical or narrowed group), against the base folded, then
            // folded with more counts than it admits.
            return particle_restricts_unfolded(schema_set, derived, base)
                || fold_single_child_group(base)
                    .or_else(|| under_folded_base(base))
                    .is_some_and(|folded_base| {
                        particle_restricts(schema_set, derived, &folded_base)
                    })
                || over_folded(derived)
                    .is_some_and(|over| particle_restricts(schema_set, &over, base));
        }
        if let Some(folded_base) = fold_single_child_group(base) {
            return particle_restricts(schema_set, derived, &folded_base);
        }
        // A base group whose counts are not one set: folded with fewer
        // counts than it admits, then as it is.
        if under_folded_base(base)
            .is_some_and(|folded_base| particle_restricts(schema_set, derived, &folded_base))
        {
            return true;
        }
    }

    if particle_restricts_unfolded(schema_set, derived, base) {
        return true;
    }
    // A derived single-branch choice that normalization could not collapse
    // exactly (its counts are not one set): collapsed with more counts.
    is_unfoldable_group(derived, &[Compositor::Choice])
        && over_folded(derived).is_some_and(|over| particle_restricts(schema_set, &over, base))
}

/// A single-child group with one of `compositors` that is not folded into
/// its child: its counts are not one [`Counts`] set (a child with gaps
/// repeated).
fn is_unfoldable_group(particle: &NormalizedParticle, compositors: &[Compositor]) -> bool {
    let NormalizedParticleTerm::Group(group) = &particle.term else {
        return false;
    };
    let [child] = group.particles.as_slice() else {
        return false;
    };
    compositors.contains(&group.compositor) && folded_occurrences(particle, child, false).is_none()
}

/// A base single-child sequence or all group whose counts are not one set,
/// folded into its child with a subset of its counts
/// ([`folded_occurrences_under`]): what restricts the fold restricts the
/// group.
fn under_folded_base(particle: &NormalizedParticle) -> Option<NormalizedParticle> {
    let NormalizedParticleTerm::Group(group) = &particle.term else {
        return None;
    };
    let [child] = group.particles.as_slice() else {
        return None;
    };
    if group.compositor == Compositor::Choice {
        return None;
    }
    let counts = folded_occurrences_under(particle, child, false)?;
    Some(fold_with(particle, child, counts))
}

/// The child of the group `particle`, with `counts`.
fn fold_with(
    particle: &NormalizedParticle,
    child: &NormalizedParticle,
    counts: Counts,
) -> NormalizedParticle {
    let mut folded = NormalizedParticle {
        term: child.term.clone(),
        min_occurs: 0,
        max_occurs: None,
        source: particle.source.clone().or(child.source.clone()),
        collapsed_from: None,
        folded_group_occurs: true,
        counts: None,
        as_written: None,
    };
    folded.set_occurrences(counts);
    folded
}

/// A single-child group folded into its child with a set of counts that
/// holds all of the group's and more ([`Counts::repeated_over`]): for the
/// derived side only — what it restricts, the group restricts.
fn over_folded(particle: &NormalizedParticle) -> Option<NormalizedParticle> {
    let NormalizedParticleTerm::Group(group) = &particle.term else {
        return None;
    };
    let [child] = group.particles.as_slice() else {
        return None;
    };
    let counts = folded_occurrences(particle, child, false).unwrap_or_else(|| {
        child
            .occurrences()
            .repeated_over(particle.occurrences(), false)
    });
    let mut over = NormalizedParticle {
        term: child.term.clone(),
        min_occurs: 0,
        max_occurs: None,
        source: particle.source.clone().or(child.source.clone()),
        collapsed_from: (group.compositor == Compositor::Choice).then_some(Compositor::Choice),
        folded_group_occurs: true,
        counts: None,
        as_written: None,
    };
    over.set_occurrences(counts);
    Some(over)
}

/// Are the derived particle's counts among the base particle's — §3.9.6
/// Occurrence Range OK, with the exact sets ([`NormalizedParticle::counts`])?
fn occurrences_within(
    schema_set: &SchemaSet,
    derived: &NormalizedParticle,
    base: &NormalizedParticle,
) -> bool {
    if derived.counts.is_none() && base.counts.is_none() {
        let (base_min, base_max) = effective_range(schema_set, base);
        return occurs_range_is_subset(derived.min_occurs, derived.max_occurs, base_min, base_max);
    }
    derived
        .occurrences()
        .within(&effective_occurrences(schema_set, base))
}

/// Is every count in `[min, max]` among the base particle's counts?
fn range_within(
    schema_set: &SchemaSet,
    min: u32,
    max: Option<u32>,
    base: &NormalizedParticle,
) -> bool {
    if base.counts.is_none() {
        let (base_min, base_max) = effective_range(schema_set, base);
        return occurs_range_is_subset(min, max, base_min, base_max);
    }
    Counts::range(min, max).within(&effective_occurrences(schema_set, base))
}

/// Does the base particle admit exactly one occurrence of its term?
fn admits_one(schema_set: &SchemaSet, base: &NormalizedParticle) -> bool {
    range_within(schema_set, 1, Some(1), base)
}

/// A base particle's range as far as its language goes: when its term
/// accepts the empty sequence, `L(T)^c ⊆ L(T)^{c+1}`, so fewer occurrences
/// than its minimum add no sequence it lacks — the range starts at 0.
fn effective_range(schema_set: &SchemaSet, base: &NormalizedParticle) -> (u32, Option<u32>) {
    if schema_set.is_xsd11() && term_is_emptiable(&base.term) {
        (0, base.max_occurs)
    } else {
        (base.min_occurs, base.max_occurs)
    }
}

fn effective_occurrences(schema_set: &SchemaSet, base: &NormalizedParticle) -> Counts {
    if schema_set.is_xsd11() && term_is_emptiable(&base.term) {
        base.occurrences().up_to_max()
    } else {
        base.occurrences()
    }
}

/// `particle` without its empty sequence: its counts other than 0.
fn without_empty(particle: &NormalizedParticle) -> NormalizedParticle {
    let mut non_empty = particle.clone();
    non_empty.set_occurrences(particle.occurrences().without_zero());
    non_empty
}

/// What a particle's language reduces to when it is degenerate.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Degenerate {
    /// No sequence at all: a required group whose term accepts none — at
    /// the bottom a required empty choice, whose branches all map to no
    /// component.
    Nothing,
    /// The empty sequence and nothing else.
    OnlyEmpty,
}

/// `Some` when `particle` accepts no sequence or only the empty one.
pub(super) fn degenerate_language(particle: &NormalizedParticle) -> Option<Degenerate> {
    if particle.max_occurs == Some(0) {
        return Some(Degenerate::OnlyEmpty);
    }
    let term = match &particle.term {
        NormalizedParticleTerm::Element(_) | NormalizedParticleTerm::Wildcard(_) => return None,
        NormalizedParticleTerm::Group(group) => {
            let children: Vec<Option<Degenerate>> =
                group.particles.iter().map(degenerate_language).collect();
            match group.compositor {
                Compositor::Sequence | Compositor::All => {
                    if children.contains(&Some(Degenerate::Nothing)) {
                        Degenerate::Nothing
                    } else if children.iter().all(|c| *c == Some(Degenerate::OnlyEmpty)) {
                        Degenerate::OnlyEmpty
                    } else {
                        return None;
                    }
                }
                Compositor::Choice => {
                    if children.iter().all(|c| *c == Some(Degenerate::Nothing)) {
                        Degenerate::Nothing
                    } else if children.iter().all(Option::is_some) {
                        Degenerate::OnlyEmpty
                    } else {
                        return None;
                    }
                }
            }
        }
    };
    Some(match term {
        Degenerate::Nothing if particle.min_occurs > 0 => Degenerate::Nothing,
        _ => Degenerate::OnlyEmpty,
    })
}

/// Does `particle` accept no sequence at all?
pub(super) fn particle_accepts_nothing(particle: &NormalizedParticle) -> bool {
    degenerate_language(particle) == Some(Degenerate::Nothing)
}

/// XSD 1.0: the single-child choice or sequence normalization collapsed
/// into `particle`, as written, when §3.9.6 clause 2.2 does not make it
/// pointless: its particle is not 1..1 (clause 2.2.2.1; a single-child
/// `all` is always pointless). A pointless group is skipped, as the REC
/// ignores it, and the particle it held is looked at instead. Each step goes
/// one fold back, so the walk ends.
fn unpointless_group_as_written(particle: &NormalizedParticle) -> Option<&NormalizedParticle> {
    let mut current = particle;
    loop {
        if !matches!(
            current.collapsed_from,
            Some(Compositor::Choice | Compositor::Sequence)
        ) {
            return None;
        }
        let written = current.as_written.as_deref()?;
        if !occurs_is_unit(written.min_occurs, written.max_occurs) {
            return Some(written);
        }
        let NormalizedParticleTerm::Group(group) = &written.term else {
            return None;
        };
        current = group.particles.first()?;
    }
}

/// `derived` for a comparison with one of the base's children
/// (RecurseAsIfGroup's implicit group, one branch of a base choice, one
/// child of a base sequence): under XSD 1.0 without the group it was
/// collapsed from. [`particle_restricts_unfolded`] has already compared that
/// group as written with the base itself — the level the REC compares it at.
fn compared_whole(schema_set: &SchemaSet, derived: &NormalizedParticle) -> NormalizedParticle {
    let mut element = derived.clone();
    if schema_set.is_xsd10() {
        element.as_written = None;
    }
    element
}

/// XSD 1.0: `particle` with every single-child group that normalization
/// collapsed and §3.9.6 clause 2.2 does not make pointless put back as
/// written, at every depth ([`unpointless_group_as_written`]). The groups
/// clause 2.2 does make pointless stay out: collapsed single-child groups
/// stay collapsed, and the written forms (taken before the rest of the
/// normalization) lose their empty groups and have their unit groups merged
/// into a parent of the same compositor, as the rest of the tree did.
fn written_in_full(particle: &NormalizedParticle) -> NormalizedParticle {
    fn put_back(particle: &NormalizedParticle) -> NormalizedParticle {
        let mut written = unpointless_group_as_written(particle)
            .unwrap_or(particle)
            .clone();
        written.collapsed_from = None;
        written.as_written = None;
        if let NormalizedParticleTerm::Group(group) = &mut written.term {
            // What the rest of the tree dropped stays dropped.
            group.particles.retain(|child| child.max_occurs != Some(0));
            for child in &mut group.particles {
                *child = put_back(child);
            }
        }
        written
    }
    flatten_same_compositor_groups(remove_pointless_particles(put_back(particle), false))
}

/// `particle_restricts` minus the XSD 1.1 single-child fold.
///
/// The re-expansion step below deliberately re-introduces a single-child
/// group; re-entering through `particle_restricts` would fold it straight back
/// out again.
fn particle_restricts_unfolded(
    schema_set: &SchemaSet,
    derived: &NormalizedParticle,
    base: &NormalizedParticle,
) -> bool {
    // XSD 1.1 only: undo `collapse_single_child_groups` for the derived side.
    //
    // Normalization folds a single-child group `<C minOccurs="m"
    // maxOccurs="n">X</C>` down to `X{m,n}`.  That is language-preserving —
    // the two accept exactly the same sequences — but it destroys the
    // structure XSD 1.0's §3.9.6 rules match on, and under XSD 1.0 that is
    // deliberate: the pointless-particle collapse is what makes a
    // single-branch `<choice>` restricting a multi-branch base choice invalid
    // (msData groupH021v, particlesZ024, both marked "invalid in 1.0, valid
    // in 1.1" by the W3C suite).  Under XSD 1.1 the rule is language
    // subsumption (§3.4.6.4), so the folded and unfolded spellings must be
    // treated alike; re-offering the derived particle in its group form
    // admits exactly those restrictions.
    //
    // The same-compositor guard keeps this from recursing: the re-expanded
    // particle is a group with the base's compositor, so it cannot be
    // re-expanded against the same base.
    if let NormalizedParticleTerm::Group(base_group) = &base.term {
        let already_grouped = matches!(
            &derived.term,
            NormalizedParticleTerm::Group(derived_group)
                if derived_group.compositor == base_group.compositor
        );
        let wrappable = match base_group.compositor {
            Compositor::Sequence | Compositor::Choice => true,
            // `all_particles_restrict` only handles element/wildcard children.
            Compositor::All => matches!(
                &derived.term,
                NormalizedParticleTerm::Element(_) | NormalizedParticleTerm::Wildcard(_)
            ),
        };
        // XSD 1.0: a single-child choice or sequence that §3.9.6 clause 2.2
        // does not make pointless is compared as written — the group and its
        // child with their own ranges (RecurseLax maps `a?` onto a base `a`
        // only if `a?`'s range lies within it) — and so is the base, all the
        // way down, so that the comparison is the REC's own.
        if schema_set.is_xsd10() {
            if let Some(written) = unpointless_group_as_written(derived) {
                if particle_restricts_unfolded(
                    schema_set,
                    &written_in_full(written),
                    &written_in_full(base),
                ) {
                    return true;
                }
            }
        }
        if schema_set.is_xsd11()
            && derived.collapsed_from == Some(base_group.compositor)
            && !already_grouped
            && wrappable
        {
            let expanded = NormalizedParticle {
                term: NormalizedParticleTerm::Group(NormalizedGroup {
                    compositor: base_group.compositor,
                    particles: vec![NormalizedParticle {
                        term: derived.term.clone(),
                        min_occurs: 1,
                        max_occurs: Some(1),
                        source: derived.source.clone(),
                        collapsed_from: None,
                        folded_group_occurs: false,
                        counts: None,
                        as_written: None,
                    }],
                }),
                min_occurs: derived.min_occurs,
                max_occurs: derived.max_occurs,
                source: derived.source.clone(),
                collapsed_from: None,
                folded_group_occurs: derived.folded_group_occurs,
                counts: derived.counts,
                as_written: None,
            };
            if particle_restricts_unfolded(schema_set, &expanded, base) {
                return true;
            }
        }
    }

    // XSD 1.0 RecurseAsIfGroup (Elt:Choice) treats the element as the sole
    // child of a choice with range 1..1, checked with RecurseLax. Success is
    // final. On failure, a folded group may still need the occurs-folding fallback.
    // Normalization may have folded a single-child group's occurs into the
    // element (particlesHa165/Ha167/V009), and the W3C suite accepts the lax
    // interpretation for repeated choices (particlesZ001).
    if let (NormalizedParticleTerm::Element(_), NormalizedParticleTerm::Group(base_group)) =
        (&derived.term, &base.term)
    {
        if schema_set.is_xsd10() && base_group.compositor == Compositor::Choice {
            let implicit_range_fits = admits_one(schema_set, base);
            if implicit_range_fits
                && choice_branches_restrict_ordered(
                    schema_set,
                    &[compared_whole(schema_set, derived)],
                    &base_group.particles,
                )
            {
                return true;
            }

            // An element with only unit wrappers really has an implicit 1..1
            // group. Its own occurrences cannot satisfy required group repetition.
            // If normalization folded non-unit group occurrences, let the fallback
            // check that range instead.
            if !implicit_range_fits && !derived.folded_group_occurs {
                return false;
            }
        }
    }

    // Unless accepted above, an XSD 1.0 non-choice optional particle cannot
    // restrict an optional non-repeated multi-branch choice.
    // The expand_choice_branches approach merges choice occurs into
    // branches, which gives wrong results for RecurseLax when max_occurs=1.
    // For repeated choices (max>1), the spec is ambiguous — provisionally accept.
    if schema_set.is_xsd10()
        && derived.min_occurs == 0
        && !matches!(
            &derived.term,
            NormalizedParticleTerm::Group(group) if group.compositor == Compositor::Choice
        )
        && matches!(
            &base.term,
            NormalizedParticleTerm::Group(group)
                if group.compositor == Compositor::Choice
                    && base.min_occurs == 0
                    && base.max_occurs == Some(1)
                    && group.particles.len() > 1
        )
    {
        return false;
    }

    // §3.9.6 Particle Derivation OK (Choice:Choice -- RecurseLax): compare the
    // two choice particles' own occurrence ranges, then map R's *raw*
    // {particles} onto B's raw {particles} order-preservingly.
    //
    // The occurs-folding path below multiplies the parent choice's occurs into
    // every branch, which is unsound as soon as a base branch is a group: the
    // branch particle's range is multiplied but its children's are not.  An
    // optional choice (min=0) then makes every derived branch optional, and an
    // optional branch can no longer map onto a base branch whose first child is
    // required.  Try the literal spec rule first and only fall through to the
    // folding approximation when it does not apply.
    if let (
        NormalizedParticleTerm::Group(derived_choice),
        NormalizedParticleTerm::Group(base_choice),
    ) = (&derived.term, &base.term)
    {
        if derived_choice.compositor == Compositor::Choice
            && base_choice.compositor == Compositor::Choice
        {
            if occurrences_within(schema_set, derived, base)
                && choice_branches_restrict_ordered(
                    schema_set,
                    &derived_choice.particles,
                    &base_choice.particles,
                )
            {
                return true;
            }
            // Under XSD 1.0 RecurseLax *is* the whole rule for choice:choice,
            // so a failure is final.  Falling through to the occurs-folding
            // path would smuggle the choices' own ranges into the branches and
            // accept, for instance, an optional choice restricting a required
            // one whenever every branch happens to be optional.  XSD 1.1
            // compares languages (§3.4.6.4), where that restriction is genuinely
            // valid, so there the laxer path below still applies.
            if schema_set.is_xsd10() {
                return false;
            }
        }
    }

    if let Some(base_branches) = base_choice_branches(schema_set, base) {
        if let Some(derived_branches) = expand_choice_branches(derived) {
            // XSD 1.0 RecurseLax: order-preserving mapping required.
            // XSD 1.1: unordered set-based matching.
            if schema_set.is_xsd10() {
                return choice_branches_restrict_ordered(
                    schema_set,
                    &derived_branches,
                    &base_branches,
                );
            }
            // A repeated derived choice mixes its branches across iterations,
            // which no branch-by-branch check sees (`choice{0,2}(a, b)` admits
            // `(a, b)`). Accept it whole against one base branch (a wildcard
            // branch absorbs the mixtures), or iteration by iteration:
            // RecurseLax without the order, sound because each derived
            // iteration is then one base iteration.
            if let (
                true,
                NormalizedParticleTerm::Group(derived_group),
                NormalizedParticleTerm::Group(base_group),
            ) = (choice_iterations_mix(derived), &derived.term, &base.term)
            {
                // Each derived iteration one base iteration: a derived
                // branch restricts a base branch, or — when the base can
                // iterate emptily (an emptiable branch) — its non-empty form
                // does.
                let base_empty_iteration = base_group.particles.iter().any(particle_is_emptiable);
                let branch_restricts = |branch: &NormalizedParticle| {
                    let restricts_raw = |branch: &NormalizedParticle| {
                        base_group
                            .particles
                            .iter()
                            .any(|candidate| particle_restricts(schema_set, branch, candidate))
                    };
                    restricts_raw(branch)
                        || base_empty_iteration
                            && particle_is_emptiable(branch)
                            && branch.max_occurs != Some(0)
                            && restricts_raw(&without_empty(branch))
                };
                // Or each derived iteration one or more base iterations:
                // branch `Y` restricts the base's term repeated `[1, ∞)`
                // times, and every total the derived iterations can make —
                // `[K·r, ∞)` over their counts `K`, `r = 0` when a branch is
                // emptiable — is a count of the base.
                let absorbed = |derived: &NormalizedParticle| {
                    let mut repeated = base.clone();
                    repeated.set_occurrences(Counts::range(1, None));
                    if !derived_group
                        .particles
                        .iter()
                        .all(|branch| particle_restricts(schema_set, branch, &repeated))
                    {
                        return false;
                    }
                    let low = u32::from(!derived_group.particles.iter().any(particle_is_emptiable));
                    let totals =
                        Counts::range(low, None).repeated_over(derived.occurrences(), false);
                    totals.within(&effective_occurrences(schema_set, base))
                };
                let iterations_restrict = |derived: &NormalizedParticle| {
                    restricts_some_base_branch(schema_set, derived, &base_branches)
                        || occurrences_within(schema_set, derived, base)
                            && derived_group.particles.iter().all(branch_restricts)
                        || absorbed(derived)
                };
                if iterations_restrict(derived) {
                    return true;
                }
                // An optional derived choice whose empty sequence the base
                // accepts (an emptiable branch) only has to show its
                // non-empty iterations: `choice{0,n}` is ε or `choice{1,n}`.
                if derived.min_occurs == 0 && particle_is_emptiable(base) {
                    return iterations_restrict(&without_empty(derived));
                }
                return false;
            }
            // XSD 1.1: each derived branch must restrict some base branch —
            // OR, when the derived branch is emptiable and at least one base
            // branch is emptiable, the derived branch's empty production is
            // covered by that emptiable base branch and the non-empty form
            // (min≥1) must restrict some base branch. This handles cases
            // like addB118 where the derived choice is optional (min=0) but
            // no single base choice branch is emptiable AND accepts the
            // derived's elements — the union of branches covers it.
            let base_has_emptiable_branch = particle_is_emptiable(base);
            return derived_branches.iter().all(|branch| {
                if restricts_some_base_branch(schema_set, branch, &base_branches) {
                    return true;
                }
                if branch.min_occurs == 0 && base_has_emptiable_branch {
                    if branch.max_occurs == Some(0) {
                        // original was min=0,max=0 (empty); empty production
                        // alone is covered, no non-empty form to check.
                        return true;
                    }
                    return restricts_some_base_branch(
                        schema_set,
                        &without_empty(branch),
                        &base_branches,
                    );
                }
                false
            });
        }

        // Sequence-vs-choice: dedicated handler instead of "any branch" check.
        if let NormalizedParticleTerm::Group(derived_group) = &derived.term {
            if derived_group.compositor == Compositor::Sequence {
                // XSD 1.1: try "restricts any single branch" first.
                // Sound because if derived restricts one branch, it restricts
                // a subset of the choice's language.
                if schema_set.is_xsd11()
                    && restricts_some_base_branch(schema_set, derived, &base_branches)
                {
                    return true;
                }
                let NormalizedParticleTerm::Group(base_group) = &base.term else {
                    unreachable!()
                };
                return sequence_restricts_choice(
                    schema_set,
                    derived,
                    derived_group,
                    base,
                    base_group,
                );
            }
        }

        return restricts_some_base_branch(schema_set, derived, &base_branches);
    }

    if let Some(derived_branches) = expand_choice_branches(derived) {
        // A repeated derived choice against a base sequence: its iterations
        // can put the branches in an order the sequence rejects, so the
        // branches are not compared one by one. XSD 1.0 §3.9.6 marks
        // Choice:Sequence Forbidden outright; XSD 1.1 compares the whole
        // choice with the sequence's children — one iteration of the
        // sequence, or one child repeated over its iterations.
        if choice_iterations_mix(derived) {
            if let NormalizedParticleTerm::Group(base_group) = &base.term {
                if base_group.compositor == Compositor::Sequence {
                    return schema_set.is_xsd11()
                        && (restricts_one_sequence_child(schema_set, derived, base, base_group)
                            || restricts_repeated_child(schema_set, derived, base, base_group));
                }
            }
        }
        return derived_branches
            .iter()
            .all(|branch| particle_restricts(schema_set, branch, base));
    }

    match (&derived.term, &base.term) {
        (
            NormalizedParticleTerm::Element(derived_element),
            NormalizedParticleTerm::Element(base_element),
        ) => {
            let names_match = derived_element.name == base_element.name
                && derived_element.namespace == base_element.namespace;
            let subst_match = !names_match
                && match (derived_element.element_key, base_element.element_key) {
                    (Some(d_key), Some(b_key)) => {
                        crate::compiler::substitution::is_element_substitutable_for(
                            schema_set, b_key, d_key,
                        )
                    }
                    _ => false,
                };
            // NameAndTypeOK (§3.9.6):
            // 1. Names match or substitution group
            (names_match || subst_match)
            // 2. Occurrence range subset (exact counts)
            && occurrences_within(schema_set, derived, base)
            // 3. nillable: derived nillable only if base nillable
            && (base_element.nillable || !derived_element.nillable)
            // 4. fixed value: if base is fixed, derived must be fixed with same value (value-space)
            && match (&base_element.fixed_value, &derived_element.fixed_value) {
                (None, _) => true,
                (Some(_), None) => false,
                (Some(base_fixed), Some(derived_fixed)) => {
                    crate::validation::simple::fixed_values_equal_in(
                        derived_fixed,
                        schema_set.value_constraint_namespaces(derived_element.fixed_source.as_ref()),
                        base_fixed,
                        schema_set.value_constraint_namespaces(base_element.fixed_source.as_ref()),
                        Some(derived_element.type_key),
                        schema_set,
                    )
                }
            }
            // TODO: 5. identity-constraint definitions subset (not yet implemented)
            // 6. block superset (masked to element-relevant bits)
            && derived_element.block.element_block_mask()
                .contains(base_element.block.element_block_mask())
            // 7. type derivation
            && schema_set.is_type_derived_from(
                derived_element.type_key,
                base_element.type_key,
                DerivationSet::extension(),
            )
        }
        (
            NormalizedParticleTerm::Element(element),
            NormalizedParticleTerm::Wildcard(base_wildcard),
        ) => {
            occurrences_within(schema_set, derived, base)
                && wildcard_allows_element(element, base_wildcard)
        }
        (
            NormalizedParticleTerm::Wildcard(derived_wildcard),
            NormalizedParticleTerm::Wildcard(base_wildcard),
        ) => {
            occurrences_within(schema_set, derived, base)
                && wildcard_restricts(derived_wildcard, base_wildcard)
        }
        (
            NormalizedParticleTerm::Group(derived_group),
            NormalizedParticleTerm::Wildcard(base_wildcard),
        ) => group_particle_restricts_wildcard(
            schema_set,
            derived,
            derived_group,
            base,
            base_wildcard,
        ),
        (
            NormalizedParticleTerm::Group(derived_group),
            NormalizedParticleTerm::Group(base_group),
        ) if derived_group.compositor == base_group.compositor => {
            if !occurrences_within(schema_set, derived, base) {
                return false;
            }
            match derived_group.compositor {
                Compositor::Sequence => sequence_particles_restrict(
                    schema_set,
                    &derived_group.particles,
                    &base_group.particles,
                ),
                Compositor::All => all_particles_restrict(
                    schema_set,
                    &derived_group.particles,
                    &base_group.particles,
                ),
                Compositor::Choice => unreachable!("choice particles are handled earlier"),
            }
        }
        // Sequence:All — RecurseUnordered (§3.9.6): unordered bipartite
        // matching regardless of XSD version.  The XSD 1.0 ordered fallback
        // in all_particles_restrict is only correct for All:All (Recurse).
        (
            NormalizedParticleTerm::Group(derived_group),
            NormalizedParticleTerm::Group(base_group),
        ) if derived_group.compositor == Compositor::Sequence
            && base_group.compositor == Compositor::All =>
        {
            if !occurrences_within(schema_set, derived, base) {
                return false;
            }
            recurse_unordered(schema_set, &derived_group.particles, &base_group.particles)
        }
        // recurseAsIfGroup: wrap derived element/wildcard in an implicit group{1,1}
        // and check outer occurs before delegating to sequence/all matching.
        (
            NormalizedParticleTerm::Element(_) | NormalizedParticleTerm::Wildcard(_),
            NormalizedParticleTerm::Group(base_group),
        ) if matches!(
            base_group.compositor,
            Compositor::Sequence | Compositor::All
        ) =>
        {
            let match_children = |children: &[NormalizedParticle]| match base_group.compositor {
                Compositor::All => {
                    all_particles_restrict(schema_set, children, &base_group.particles)
                }
                _ => sequence_particles_restrict(schema_set, children, &base_group.particles),
            };

            admits_one(schema_set, base) && match_children(&[compared_whole(schema_set, derived)])
                || schema_set.is_xsd11()
                    && restricts_repeated_child(schema_set, derived, base, base_group)
        }
        _ => false,
    }
}

fn group_particle_restricts_wildcard(
    schema_set: &SchemaSet,
    derived: &NormalizedParticle,
    group: &NormalizedGroup,
    base: &NormalizedParticle,
    wildcard: &NormalizedWildcard,
) -> bool {
    let (derived_min, derived_max) = particle_total_occurrence_range(derived);
    if !Counts::covering(derived_min, derived_max).within(&effective_occurrences(schema_set, base))
    {
        return false;
    }

    group_particles_fit_wildcard(&group.particles, wildcard)
}

fn occurs_range_is_subset(
    derived_min: u32,
    derived_max: Option<u32>,
    base_min: u32,
    base_max: Option<u32>,
) -> bool {
    if derived_min < base_min {
        return false;
    }

    match (derived_max, base_max) {
        (_, None) => true,
        (Some(derived), Some(base)) => derived <= base,
        (None, Some(_)) => false,
    }
}

/// The branches of the derived choice `particle`, each with the choice's
/// range folded in: `choice{m,n}(…, X{p,q}, …)` gives `X` with the counts
/// `⋃ₖ [k·p, k·q]`, exact where they are one set and a superset
/// ([`Counts::repeated_over`]) otherwise — a derived particle may admit more
/// than the branch does, never less (each branch alone; see `choice_iterations_mix` for the mixtures).
fn expand_choice_branches(particle: &NormalizedParticle) -> Option<Vec<NormalizedParticle>> {
    let NormalizedParticleTerm::Group(group) = &particle.term else {
        return None;
    };
    if group.compositor != Compositor::Choice {
        return None;
    }

    Some(
        group
            .particles
            .iter()
            .map(|child| {
                let counts = folded_occurrences(particle, child, false).unwrap_or_else(|| {
                    child
                        .occurrences()
                        .repeated_over(particle.occurrences(), false)
                });
                folded_branch(particle, child, counts)
            })
            .collect(),
    )
}

/// The branches of the base choice `particle`, each with the choice's range
/// folded in, admitting only sequences the choice admits:
/// `choice{m,n}(…, X{p,q}, …)` gives `X` with the counts `⋃ₖ [k·p, k·q]`
/// (Structures 1.1 §3.9.4.1.1, §3.8.4.1.2) — under XSD 1.1 including the
/// empty iterations an emptiable sibling branch allows (k from 0). A branch
/// whose counts are not one set is offered with a subset of them
/// ([`folded_occurrences_under`]), or not at all. The folding code compared
/// with `X{m·p, n·q}`, which admits counts no number of iterations produces
/// once `p ≥ 2`.
fn base_choice_branches(
    schema_set: &SchemaSet,
    particle: &NormalizedParticle,
) -> Option<Vec<NormalizedParticle>> {
    let NormalizedParticleTerm::Group(group) = &particle.term else {
        return None;
    };
    if group.compositor != Compositor::Choice {
        return None;
    }
    let emptiable = group
        .particles
        .iter()
        .filter(|child| particle_is_emptiable(child))
        .count();
    Some(
        group
            .particles
            .iter()
            .filter_map(|child| {
                let empty_iteration =
                    schema_set.is_xsd11() && emptiable > usize::from(particle_is_emptiable(child));
                let counts = folded_occurrences_under(particle, child, empty_iteration)?;
                Some(folded_branch(particle, child, counts))
            })
            .collect(),
    )
}

/// The branch `child` of the choice `particle`, with `counts`.
fn folded_branch(
    particle: &NormalizedParticle,
    child: &NormalizedParticle,
    counts: Counts,
) -> NormalizedParticle {
    let mut branch = NormalizedParticle {
        term: child.term.clone(),
        min_occurs: 0,
        max_occurs: None,
        source: particle.source.clone().or(child.source.clone()),
        collapsed_from: None,
        folded_group_occurs: particle.folded_group_occurs
            || child.folded_group_occurs
            || !occurs_is_unit(particle.min_occurs, particle.max_occurs),
        counts: None,
        as_written: None,
    };
    branch.set_occurrences(counts);
    collapse_single_child_groups(branch)
}

/// A choice with two or more branches that may repeat: its iterations can
/// mix branches, so checking the branches one at a time (each with the
/// choice's range folded in) says nothing about the mixtures.
fn choice_iterations_mix(particle: &NormalizedParticle) -> bool {
    matches!(&particle.term, NormalizedParticleTerm::Group(group)
        if group.compositor == Compositor::Choice && group.particles.len() > 1)
        && !matches!(particle.max_occurs, Some(0 | 1))
}

/// Does `derived` restrict one child of the base sequence or all group
/// `base`, repeated as often as `base` iterates, while every other child is
/// emptiable? Each iteration of `base` can then be that child alone, so
/// `L(base)` holds the child repeated over the iterations — counts
/// [`folded_occurrences_under`]. RecurseAsIfGroup over several iterations,
/// for a derived particle that needs more than one — XSD 1.1 only: XSD
/// 1.0's RecurseAsIfGroup asks for the 1..1 range (§3.9.6).
fn restricts_repeated_child(
    schema_set: &SchemaSet,
    derived: &NormalizedParticle,
    base: &NormalizedParticle,
    base_group: &NormalizedGroup,
) -> bool {
    if base_group.particles.len() < 2 {
        return false;
    }
    let mut required = base_group
        .particles
        .iter()
        .filter(|child| !particle_is_emptiable(child));
    let candidates: Vec<&NormalizedParticle> = match (required.next(), required.next()) {
        (None, _) => base_group.particles.iter().collect(),
        (Some(child), None) => vec![child],
        (Some(_), Some(_)) => return false,
    };
    candidates.into_iter().any(|child| {
        folded_occurrences_under(base, child, false).is_some_and(|counts| {
            particle_restricts(schema_set, derived, &fold_with(base, child, counts))
        })
    })
}

/// XSD 1.1: does `derived`, taken whole, restrict one child of the base
/// sequence `base` while every other child is emptiable?
///
/// Then each sequence `derived` accepts is accepted by that child, the other
/// children match the empty sequence, and one iteration of `base` — the rest,
/// if `base` must repeat, empty — accepts it (§3.8.4.1.1, §3.9.4.1.1), as
/// §3.4.6.4 clause 1 requires. A repeated choice that only several children
/// together accept is not shown this way.
fn restricts_one_sequence_child(
    schema_set: &SchemaSet,
    derived: &NormalizedParticle,
    base: &NormalizedParticle,
    base_group: &NormalizedGroup,
) -> bool {
    // One iteration of `base`, or several with all but one empty.
    let one_iteration =
        admits_one(schema_set, base) || term_is_emptiable(&base.term) && base.max_occurs != Some(0);
    if !one_iteration {
        return false;
    }
    let mut required = base_group
        .particles
        .iter()
        .filter(|child| !particle_is_emptiable(child));
    let derived = compared_whole(schema_set, derived);
    match (required.next(), required.next()) {
        (None, _) => base_group
            .particles
            .iter()
            .any(|child| particle_restricts(schema_set, &derived, child)),
        (Some(child), None) => particle_restricts(schema_set, &derived, child),
        (Some(_), Some(_)) => false,
    }
}

/// Does `derived` restrict one of the base choice's branches, each with the
/// choice's range folded in exactly ([`base_choice_branches`])?
fn restricts_some_base_branch(
    schema_set: &SchemaSet,
    derived: &NormalizedParticle,
    base_branches: &[NormalizedParticle],
) -> bool {
    let derived = compared_whole(schema_set, derived);
    base_branches
        .iter()
        .any(|candidate| particle_restricts(schema_set, &derived, candidate))
}

/// The counts of element and wildcard occurrences a derived particle's
/// sequences can have, as a range that holds them all: sums and products
/// exact in [`Count`], a maximum that overflows taken as unbounded.
fn particle_total_occurrence_range(particle: &NormalizedParticle) -> (Count, Option<Count>) {
    let add = |x: Option<Count>, y: Option<Count>| match (x, y) {
        (Some(x), Some(y)) => x.checked_add(y),
        _ => None,
    };
    let (term_min, term_max) = match &particle.term {
        NormalizedParticleTerm::Element(_) | NormalizedParticleTerm::Wildcard(_) => (1, Some(1)),
        NormalizedParticleTerm::Group(group) => match group.compositor {
            Compositor::Sequence | Compositor::All => group.particles.iter().fold(
                (0, Some(0)),
                |(acc_min, acc_max): (Count, Option<Count>), child| {
                    let (child_min, child_max) = particle_total_occurrence_range(child);
                    (acc_min.saturating_add(child_min), add(acc_max, child_max))
                },
            ),
            Compositor::Choice => {
                let mut min_total: Option<Count> = None;
                let mut max_total: Option<Option<Count>> = None;
                for child in &group.particles {
                    let (child_min, child_max) = particle_total_occurrence_range(child);
                    min_total = Some(min_total.map_or(child_min, |current| current.min(child_min)));
                    max_total = Some(match max_total {
                        Some(Some(current)) => child_max.map(|child| current.max(child)),
                        Some(None) => None,
                        None => child_max,
                    });
                }
                (min_total.unwrap_or(0), max_total.unwrap_or(Some(0)))
            }
        },
    };
    let occurrences = particle.occurrences();
    let max = match (occurrences.max(), term_max) {
        (Some(0), _) | (_, Some(0)) => Some(0),
        (Some(x), Some(y)) => x.checked_mul(y),
        _ => None,
    };
    (occurrences.min().saturating_mul(term_min), max)
}

fn group_particles_fit_wildcard(
    particles: &[NormalizedParticle],
    wildcard: &NormalizedWildcard,
) -> bool {
    particles
        .iter()
        .all(|particle| particle_fits_wildcard(particle, wildcard))
}

fn particle_fits_wildcard(particle: &NormalizedParticle, wildcard: &NormalizedWildcard) -> bool {
    if let Some(branches) = expand_choice_branches(particle) {
        return branches
            .iter()
            .all(|branch| particle_fits_wildcard(branch, wildcard));
    }

    match &particle.term {
        NormalizedParticleTerm::Element(element) => wildcard_allows_element(element, wildcard),
        NormalizedParticleTerm::Wildcard(derived_wildcard) => {
            wildcard_restricts(derived_wildcard, wildcard)
        }
        NormalizedParticleTerm::Group(group) => {
            group_particles_fit_wildcard(&group.particles, wildcard)
        }
    }
}

/// Check whether a derived sequence restricts a base choice.
///
/// Two conditions are verified:
///
/// 1. **Per-particle match** — every child of the derived sequence must
///    restrict at least one *raw* base choice branch (name, type, and
///    per-iteration occurs).
///
/// 2. **Iteration budget** — each non-empty derived particle consumes at
///    least one choice iteration.  The total iterations across all sequence
///    repetitions must fit within the base choice's occurs range.
fn sequence_restricts_choice(
    schema_set: &SchemaSet,
    derived: &NormalizedParticle,
    derived_group: &NormalizedGroup,
    base: &NormalizedParticle,
    base_group: &NormalizedGroup,
) -> bool {
    let base_branches = &base_group.particles;

    let mut required_per_iter: u32 = 0;
    let mut total_per_iter: u32 = 0;

    for derived_child in &derived_group.particles {
        // Each derived child must restrict at least one raw base branch.
        let found = base_branches
            .iter()
            .any(|branch| particle_restricts(schema_set, derived_child, branch));
        if !found {
            return false;
        }

        // Count choice-iteration demand per sequence iteration.
        if derived_child.min_occurs > 0 {
            required_per_iter += 1;
        }
        if derived_child.max_occurs != Some(0) {
            total_per_iter += 1;
        }
    }

    // The total choice iterations across all sequence repetitions must fit
    // within the base choice's occurs range: from the derived sequence's
    // exact counts, a product beyond the counts' limit taken as unbounded
    // (more demand, never less).
    let occurrences = derived.occurrences();
    let min_demand = occurrences
        .min()
        .saturating_mul(Count::from(required_per_iter));
    let max_demand = match (occurrences.max(), total_per_iter) {
        (_, 0) => Some(0),
        (Some(max), per_iter) => max.checked_mul(Count::from(per_iter)),
        (None, _) => None,
    };
    Counts::covering(min_demand, max_demand).within(&effective_occurrences(schema_set, base))
}

/// XSD 1.0 RecurseLax: order-preserving matching of choice branches.
/// Each derived branch must map to a base branch at or after the previous match.
/// Unmatched base branches are implicitly skipped (lax, not strict).
fn choice_branches_restrict_ordered(
    schema_set: &SchemaSet,
    derived_branches: &[NormalizedParticle],
    base_branches: &[NormalizedParticle],
) -> bool {
    let mut base_index = 0;
    for derived in derived_branches {
        let mut found = false;
        while base_index < base_branches.len() {
            if particle_restricts(schema_set, derived, &base_branches[base_index]) {
                base_index += 1;
                found = true;
                break;
            }
            base_index += 1;
        }
        if !found {
            return false;
        }
    }
    true
}

fn sequence_particles_restrict(
    schema_set: &SchemaSet,
    derived_particles: &[NormalizedParticle],
    base_particles: &[NormalizedParticle],
) -> bool {
    let mut base_index = 0;
    let mut derived_index = 0;

    while derived_index < derived_particles.len() {
        let mut matched = false;

        while let Some(base) = base_particles.get(base_index) {
            // 1. Direct particle-vs-particle match (the normal greedy step).
            if particle_restricts(schema_set, &derived_particles[derived_index], base) {
                matched = true;
                base_index += 1;
                derived_index += 1;
                break;
            }

            // 2. Atomic sequence-unit match: when the base particle is a
            //    sequence group (e.g. an expanded repetition unit), try to
            //    match a contiguous slice of derived particles against the
            //    unit's children.  This keeps the unit atomic — either the
            //    full slice matches or we fall through. The slice is one
            //    iteration of the unit, so the unit must admit one: its
            //    range holds 1, or its term is emptiable and it may occur
            //    (the other iterations empty).
            if let NormalizedParticleTerm::Group(base_group) = &base.term {
                if base_group.compositor == Compositor::Sequence
                    && !base_group.particles.is_empty()
                    && (admits_one(schema_set, base)
                        || term_is_emptiable(&base.term) && base.max_occurs != Some(0))
                {
                    let unit_len = base_group.particles.len();
                    let remaining = derived_particles.len() - derived_index;
                    if remaining >= unit_len
                        && sequence_particles_restrict(
                            schema_set,
                            &derived_particles[derived_index..derived_index + unit_len],
                            &base_group.particles,
                        )
                    {
                        matched = true;
                        base_index += 1;
                        derived_index += unit_len;
                        break;
                    }
                }
            }

            // 3. XSD 1.1: expand choice in derived sequence.
            //    Each branch must independently work from the current base position
            //    for the entire remaining derived + base suffix. Not for a
            //    choice whose iterations mix its branches: no single branch
            //    stands for `(b, a)` of `choice{0,2}(a, b)`.
            if schema_set.is_xsd11() && !choice_iterations_mix(&derived_particles[derived_index]) {
                if let Some(branches) = expand_choice_branches(&derived_particles[derived_index]) {
                    let all_ok = branches.iter().all(|branch| {
                        let mut remaining = vec![branch.clone()];
                        remaining.extend_from_slice(&derived_particles[derived_index + 1..]);
                        sequence_particles_restrict(
                            schema_set,
                            &remaining,
                            &base_particles[base_index..],
                        )
                    });
                    if all_ok {
                        return true;
                    }
                }
            }

            // 3a. XSD 1.1: inline unit-occurs derived sequence group.
            //    Compensates for the disabled flatten_same_compositor_groups.
            //    sequence{1,1}(a, b, c, ...) at derived can be inlined into
            //    the parent sequence and matched element-by-element against base.
            if schema_set.is_xsd11() {
                if let NormalizedParticleTerm::Group(dg) = &derived_particles[derived_index].term {
                    if dg.compositor == Compositor::Sequence
                        && occurs_is_unit(
                            derived_particles[derived_index].min_occurs,
                            derived_particles[derived_index].max_occurs,
                        )
                        && !dg.particles.is_empty()
                    {
                        let mut inlined = dg.particles.clone();
                        inlined.extend_from_slice(&derived_particles[derived_index + 1..]);
                        if sequence_particles_restrict(
                            schema_set,
                            &inlined,
                            &base_particles[base_index..],
                        ) {
                            return true;
                        }
                    }
                }
            }

            // 4. Skip emptiable base particles.
            if particle_is_emptiable(base) {
                base_index += 1;
                continue;
            }

            return false;
        }

        if !matched {
            return false;
        }
    }

    base_particles[base_index..]
        .iter()
        .all(particle_is_emptiable)
}

/// Merge element particles in an unordered-matching context that share the
/// same expanded name (local + target namespace). The merged particle sums
/// `min_occurs` and `max_occurs` (unbounded on either side stays unbounded).
///
/// Used by `recurse_unordered` to support Sequence→All derivations where the
/// derived sequence lists the same element more than once (saxonData
/// All/all216 is the canonical case). Order within the derived side
/// doesn't affect the base's all-group language, so treating duplicate
/// names as one combined occurrence range is sound.
///
/// Non-element particles (wildcards, nested groups) are passed through
/// unchanged: collapsing them would change their matching semantics.
fn merge_duplicate_elements(particles: &[NormalizedParticle]) -> Vec<NormalizedParticle> {
    let mut merged: Vec<NormalizedParticle> = Vec::with_capacity(particles.len());
    for particle in particles {
        let NormalizedParticleTerm::Element(elem) = &particle.term else {
            merged.push(particle.clone());
            continue;
        };
        let existing = merged.iter_mut().find(|m| {
            matches!(
                &m.term,
                NormalizedParticleTerm::Element(other)
                    if other.name == elem.name && other.namespace == elem.namespace
            )
        });
        if let Some(existing) = existing {
            // The summed range (a derived particle may admit more, never
            // less), exact in `Count`, a maximum that overflows unbounded.
            let (left, right) = (existing.occurrences(), particle.occurrences());
            let max = match (left.max(), right.max()) {
                (Some(x), Some(y)) => x.checked_add(y),
                _ => None,
            };
            existing.set_occurrences(Counts::covering(
                left.min().saturating_add(right.min()),
                max,
            ));
        } else {
            merged.push(particle.clone());
        }
    }
    merged
}

/// RecurseUnordered: order-independent matching of derived particles against
/// the base all-group's particles. Combines two strategies:
///
/// 1. **Count-based bucket subsumption** (preferred — handles substitution
///    groups, wildcard partition, and choice expansion). Each derived
///    particle is assigned to a base "bucket" (by name match, substitution
///    group head, or wildcard subset). For each bucket the summed derived
///    occurrence range must fit within the base particle's range; unassigned
///    base particles must be emptiable.
///
/// 2. **Bipartite 1-to-1 matching** (fallback for derived particles that
///    contain nested groups not handled by the bucket approach). Each derived
///    particle must restrict some base particle; unmatched base particles
///    must be emptiable.
fn recurse_unordered(
    schema_set: &SchemaSet,
    derived_particles: &[NormalizedParticle],
    base_particles: &[NormalizedParticle],
) -> bool {
    // Try count-based bucket subsumption first — it handles substitution
    // group merging, wildcard partition, and choice distribution.
    if let Some(result) = try_count_based_subsumption(schema_set, derived_particles, base_particles)
    {
        if result {
            return true;
        }
        // Bucket said "no" — but for the failure path we still try bipartite
        // because it can recover via different particle wirings (e.g. when
        // a derived particle could fit either an element or wildcard bucket
        // but the bucket heuristic picked the wrong one).
    }

    // Fallback: bipartite 1-to-1 matching with same-name merge.
    fn backtrack(
        schema_set: &SchemaSet,
        derived_particles: &[NormalizedParticle],
        base_particles: &[NormalizedParticle],
        used: &mut [bool],
        derived_index: usize,
    ) -> bool {
        if derived_index == derived_particles.len() {
            return base_particles
                .iter()
                .enumerate()
                .all(|(index, particle)| used[index] || particle_is_emptiable(particle));
        }

        for (base_index, base_particle) in base_particles.iter().enumerate() {
            if used[base_index]
                || !particle_restricts(schema_set, &derived_particles[derived_index], base_particle)
            {
                continue;
            }
            used[base_index] = true;
            if backtrack(
                schema_set,
                derived_particles,
                base_particles,
                used,
                derived_index + 1,
            ) {
                return true;
            }
            used[base_index] = false;
        }

        false
    }

    // Merge duplicate-name element particles in the derived side: unordered
    // matching counts total occurrences per element, not particle positions.
    let merged_owned;
    let derived_particles = if derived_particles
        .iter()
        .any(|p| matches!(&p.term, NormalizedParticleTerm::Element(_)))
        && has_duplicate_element_names(derived_particles)
    {
        merged_owned = merge_duplicate_elements(derived_particles);
        &merged_owned[..]
    } else {
        derived_particles
    };

    let mut used = vec![false; base_particles.len()];
    backtrack(schema_set, derived_particles, base_particles, &mut used, 0)
}

/// Count-based subsumption: bucket each derived particle by the base particle
/// it maps to, sum derived occurrence ranges per bucket, and check that the
/// summed range fits the base range. Unassigned base particles must be
/// emptiable.
///
/// Returns:
/// - `Some(true)` — the derived particles validly restrict the base under
///   count-based subsumption (substitution groups, wildcard partition, and
///   top-level derived choices are all handled).
/// - `Some(false)` — every derived particle was bucket-able but the
///   per-bucket sum exceeded the base range or an unassigned base particle
///   is not emptiable.
/// - `None` — at least one derived particle is a nested model group; the
///   caller should fall back to bipartite matching.
fn try_count_based_subsumption(
    schema_set: &SchemaSet,
    derived_particles: &[NormalizedParticle],
    base_particles: &[NormalizedParticle],
) -> Option<bool> {
    // Step 1: Expand top-level choices into optional alternatives.
    let expanded = expand_top_level_choices_for_unordered(derived_particles)?;

    // Step 2: Bucket each expanded derived particle to a base index.
    // Each derived particle's counts as a range (a gapped set by its hull:
    // more, never less), exact in `Count`.
    let mut buckets: Vec<Vec<(Count, Option<Count>)>> = vec![Vec::new(); base_particles.len()];
    for derived in &expanded {
        // Nested groups are not bucket-able here.
        if matches!(&derived.term, NormalizedParticleTerm::Group(_)) {
            return None;
        }

        match find_subsumption_bucket(schema_set, derived, base_particles)? {
            BucketAssignment::Single(idx) => {
                let occurrences = derived.occurrences();
                buckets[idx].push((occurrences.min(), occurrences.max()))
            }
            BucketAssignment::Partition(idxs) => {
                // Each base bucket the partition spans must be emptiable
                // (b_min = 0) since derived elements may not land in it,
                // and derived's max must fit each spanned base's max
                // (worst case: all elements land in one bucket).
                for &i in &idxs {
                    let base = &base_particles[i];
                    if base.min_occurs > 0 {
                        return Some(false);
                    }
                    if !count_max_fits(derived.occurrences().max(), base.occurrences().max()) {
                        return Some(false);
                    }
                }
                // Contribute (0, derived.max) to each spanned bucket — the
                // total never exceeds the partition's d_max in any one
                // bucket, but might be 0.
                for &i in &idxs {
                    buckets[i].push((0, derived.occurrences().max()));
                }
            }
            BucketAssignment::None => return Some(false),
        }
    }

    // Step 3: Check each bucket's summed range fits the base range; unmatched
    // base particles must be emptiable.
    for (i, ranges) in buckets.iter().enumerate() {
        let base = &base_particles[i];
        if ranges.is_empty() {
            if !particle_is_emptiable(base) {
                return Some(false);
            }
        } else {
            let (sum_min, sum_max) = ranges.iter().fold(
                (0, Some(0)),
                |(amin, amax): (Count, Option<Count>), &(min, max)| {
                    let amax = match (amax, max) {
                        (Some(x), Some(y)) => x.checked_add(y),
                        _ => None,
                    };
                    (amin.saturating_add(min), amax)
                },
            );
            if !Counts::covering(sum_min, sum_max).within(&effective_occurrences(schema_set, base))
            {
                return Some(false);
            }
        }
    }

    Some(true)
}

/// Expand top-level derived choices into a flat list of element/wildcard
/// particles, treating each branch as an optional alternative. This enables
/// the count-based subsumption to handle cases like all234 where a derived
/// sequence contains a `<xs:choice>` that distributes across base all-group
/// particles.
///
/// Returns `None` if any derived particle contains a nested group whose
/// shape can't be flattened (the caller falls back to bipartite matching).
fn expand_top_level_choices_for_unordered(
    particles: &[NormalizedParticle],
) -> Option<Vec<NormalizedParticle>> {
    let mut result = Vec::with_capacity(particles.len());
    for p in particles {
        match &p.term {
            NormalizedParticleTerm::Group(group) if group.compositor == Compositor::Choice => {
                // Each branch becomes a particle with min=0 (might not be picked)
                // and max = outer_max * branch_max.
                let outer_max = p.occurrences().max();
                for branch in &group.particles {
                    if matches!(&branch.term, NormalizedParticleTerm::Group(_)) {
                        // Nested group inside choice — bail to bipartite.
                        return None;
                    }
                    // Exact in `Count`; a product that overflows unbounded
                    // (a derived particle may admit more, never less).
                    let new_max = match (outer_max, branch.occurrences().max()) {
                        (Some(0), _) | (_, Some(0)) => Some(0),
                        (Some(om), Some(bm)) => om.checked_mul(bm),
                        _ => None,
                    };
                    let mut alternative = NormalizedParticle {
                        term: branch.term.clone(),
                        min_occurs: 0,
                        max_occurs: None,
                        source: branch.source.clone(),
                        collapsed_from: None,
                        folded_group_occurs: p.folded_group_occurs
                            || branch.folded_group_occurs
                            || !occurs_is_unit(p.min_occurs, p.max_occurs),
                        counts: None,
                        as_written: None,
                    };
                    alternative.set_occurrences(Counts::covering(0, new_max));
                    result.push(alternative);
                }
            }
            NormalizedParticleTerm::Group(_) => {
                // Other nested groups (Sequence, All) — bail to bipartite.
                return None;
            }
            _ => result.push(p.clone()),
        }
    }
    Some(result)
}

/// How a derived particle is assigned to base particle bucket(s).
#[derive(Debug, Clone)]
enum BucketAssignment {
    /// derived maps to a single base particle.
    Single(usize),
    /// derived (necessarily a wildcard) partitions across multiple base
    /// wildcards — its admissible (ns, name) set is covered by the union of
    /// the listed base wildcards.
    Partition(Vec<usize>),
    /// derived has no matching base particle (restriction is invalid).
    None,
}

/// Find which base particle bucket(s) the derived particle should be assigned
/// to.
///
/// Returns:
/// - `Some(BucketAssignment::Single(idx))` — derived maps to base particle `idx`.
/// - `Some(BucketAssignment::Partition(idxs))` — derived wildcard partitions
///   across multiple base wildcards (wild049-style restriction).
/// - `Some(BucketAssignment::None)` — derived has no matching base particle.
/// - `None` — derived is a nested group that can't be bucketed (caller
///   should fall back to bipartite matching).
///
/// Search order:
/// 1. Direct element name + namespace match (NameAndTypeOK without occurs).
/// 2. Substitution group head match.
/// 3. Element fitting a base wildcard.
/// 4. Wildcard subset of a base wildcard.
/// 5. Wildcard subset of the union of multiple base wildcards (partition).
fn find_subsumption_bucket(
    schema_set: &SchemaSet,
    derived: &NormalizedParticle,
    base_particles: &[NormalizedParticle],
) -> Option<BucketAssignment> {
    match &derived.term {
        NormalizedParticleTerm::Element(d_elem) => {
            // 1. Direct name match
            for (i, base) in base_particles.iter().enumerate() {
                if let NormalizedParticleTerm::Element(b_elem) = &base.term {
                    if d_elem.name == b_elem.name
                        && d_elem.namespace == b_elem.namespace
                        && name_and_type_ok_no_occurs(schema_set, d_elem, b_elem)
                    {
                        return Some(BucketAssignment::Single(i));
                    }
                }
            }
            // 2. Substitution group head match
            for (i, base) in base_particles.iter().enumerate() {
                if let NormalizedParticleTerm::Element(b_elem) = &base.term {
                    if d_elem.name == b_elem.name && d_elem.namespace == b_elem.namespace {
                        continue; // already tried
                    }
                    if derived_element_substitutes_base(schema_set, d_elem, b_elem)
                        && name_and_type_ok_no_occurs(schema_set, d_elem, b_elem)
                    {
                        return Some(BucketAssignment::Single(i));
                    }
                }
            }
            // 3. Element fits base wildcard
            for (i, base) in base_particles.iter().enumerate() {
                if let NormalizedParticleTerm::Wildcard(b_wc) = &base.term {
                    if wildcard_allows_element(d_elem, b_wc) {
                        return Some(BucketAssignment::Single(i));
                    }
                }
            }
            Some(BucketAssignment::None)
        }
        NormalizedParticleTerm::Wildcard(d_wc) => {
            // 4. Wildcard subset of base wildcard
            for (i, base) in base_particles.iter().enumerate() {
                if let NormalizedParticleTerm::Wildcard(b_wc) = &base.term {
                    if wildcard_restricts(d_wc, b_wc) {
                        return Some(BucketAssignment::Single(i));
                    }
                }
            }
            // 5. Wildcard subset of the union of multiple base wildcards.
            // Collect all base wildcard indices and check coverage. Only
            // consider buckets that share the derived processContents
            // strictness or stronger.
            let candidate_idxs: Vec<usize> = base_particles
                .iter()
                .enumerate()
                .filter_map(|(i, base)| {
                    let NormalizedParticleTerm::Wildcard(b_wc) = &base.term else {
                        return None;
                    };
                    if process_contents_strictness(d_wc.wildcard.process_contents)
                        < process_contents_strictness(b_wc.wildcard.process_contents)
                    {
                        return None;
                    }
                    Some(i)
                })
                .collect();
            if candidate_idxs.len() >= 2 {
                let bases: Vec<&NormalizedWildcard> = candidate_idxs
                    .iter()
                    .map(|&i| match &base_particles[i].term {
                        NormalizedParticleTerm::Wildcard(b_wc) => b_wc.as_ref(),
                        _ => unreachable!(),
                    })
                    .collect();
                if let Some(spanned) = wildcard_subset_of_union(d_wc, &bases) {
                    let idxs: Vec<usize> = spanned
                        .into_iter()
                        .map(|local_idx| candidate_idxs[local_idx])
                        .collect();
                    return Some(BucketAssignment::Partition(idxs));
                }
            }
            Some(BucketAssignment::None)
        }
        NormalizedParticleTerm::Group(_) => None,
    }
}

/// Is `derived_max` at most `base_max` (`None`: unbounded)?
fn count_max_fits(derived_max: Option<Count>, base_max: Option<Count>) -> bool {
    match (derived_max, base_max) {
        (_, None) => true,
        (None, Some(_)) => false,
        (Some(d), Some(b)) => d <= b,
    }
}

/// NameAndTypeOK clauses 3, 4, 6, 7 (everything except clause 2 — the
/// occurrence range subset check). Used by count-based subsumption where
/// the occurrence check is performed at the bucket aggregate level.
fn name_and_type_ok_no_occurs(
    schema_set: &SchemaSet,
    derived: &NormalizedElement,
    base: &NormalizedElement,
) -> bool {
    // Clause 3: derived nillable only if base nillable.
    if derived.nillable && !base.nillable {
        return false;
    }
    // Clause 4: fixed value.
    match (&base.fixed_value, &derived.fixed_value) {
        (None, _) => {}
        (Some(_), None) => return false,
        (Some(base_fixed), Some(derived_fixed)) => {
            if !crate::validation::simple::fixed_values_equal_in(
                derived_fixed,
                schema_set.value_constraint_namespaces(derived.fixed_source.as_ref()),
                base_fixed,
                schema_set.value_constraint_namespaces(base.fixed_source.as_ref()),
                Some(derived.type_key),
                schema_set,
            ) {
                return false;
            }
        }
    }
    // Clause 6: block superset (masked to element bits).
    if !derived
        .block
        .element_block_mask()
        .contains(base.block.element_block_mask())
    {
        return false;
    }
    // Clause 7: type derivation.
    schema_set.is_type_derived_from(derived.type_key, base.type_key, DerivationSet::extension())
}

/// Check whether the derived element is a substitution group member of the
/// base element. Looks up the global element by name when the derived
/// element_key is None (local declaration), per W3C bug 5296 — a local
/// element can match a substitution group via its global namesake.
fn derived_element_substitutes_base(
    schema_set: &SchemaSet,
    derived: &NormalizedElement,
    base: &NormalizedElement,
) -> bool {
    let d_key = derived
        .element_key
        .or_else(|| schema_set.lookup_element(derived.namespace, derived.name));
    let b_key = base
        .element_key
        .or_else(|| schema_set.lookup_element(base.namespace, base.name));
    match (d_key, b_key) {
        (Some(d), Some(b)) => {
            crate::compiler::substitution::is_element_substitutable_for(schema_set, b, d)
        }
        _ => false,
    }
}

/// True when two or more element particles in the slice share the same
/// expanded name — a cheap check to avoid the allocation in
/// `merge_duplicate_elements` for the overwhelming-majority case where
/// every element particle is unique.
fn has_duplicate_element_names(particles: &[NormalizedParticle]) -> bool {
    for (i, a) in particles.iter().enumerate() {
        let NormalizedParticleTerm::Element(a_elem) = &a.term else {
            continue;
        };
        for b in &particles[i + 1..] {
            if let NormalizedParticleTerm::Element(b_elem) = &b.term {
                if a_elem.name == b_elem.name && a_elem.namespace == b_elem.namespace {
                    return true;
                }
            }
        }
    }
    false
}

fn all_particles_restrict(
    schema_set: &SchemaSet,
    derived_particles: &[NormalizedParticle],
    base_particles: &[NormalizedParticle],
) -> bool {
    // XSD 1.0: All:All uses order-preserving Recurse (same as Sequence:Sequence).
    // XSD 1.1: RecurseUnordered allows reordering via backtracking.
    if schema_set.is_xsd10() {
        return sequence_particles_restrict(schema_set, derived_particles, base_particles);
    }
    recurse_unordered(schema_set, derived_particles, base_particles)
}

pub(super) fn particle_is_emptiable(particle: &NormalizedParticle) -> bool {
    particle.min_occurs == 0 || term_is_emptiable(&particle.term)
}

/// Does one occurrence of `term` accept the empty sequence?
pub(super) fn term_is_emptiable(term: &NormalizedParticleTerm) -> bool {
    match term {
        NormalizedParticleTerm::Element(_) | NormalizedParticleTerm::Wildcard(_) => false,
        NormalizedParticleTerm::Group(group) => match group.compositor {
            Compositor::Sequence | Compositor::All => {
                group.particles.iter().all(particle_is_emptiable)
            }
            Compositor::Choice => group.particles.iter().any(particle_is_emptiable),
        },
    }
}
