//! §3.8 / §3.9.6 particle normalization: the `NormalizedParticle` model
//! and the rewrites (pointless-particle removal, same-compositor
//! flattening, single-child-group collapsing) applied before a particle
//! restriction check.

use super::counts::Counts;
use crate::error::{SchemaError, SchemaResult};
use crate::ids::{ElementKey, NameId, TypeKey};
use crate::parser::frames::{
    ComplexContentResult, Compositor, DerivationMethod, ElementFrameResult, ModelGroupDefResult,
    ParticleResult, ParticleTerm, WildcardResult,
};
use crate::parser::location::SourceRef;
use crate::schema::model::DerivationSet;
use crate::schema::SchemaSet;

#[derive(Debug, Clone)]
pub(super) struct NormalizedParticle {
    pub(super) term: NormalizedParticleTerm,
    pub(super) min_occurs: u32,
    pub(super) max_occurs: Option<u32>,
    pub(super) source: Option<SourceRef>,
    /// Compositor of the single-child group that `collapse_single_child_groups`
    /// folded away to produce this particle, if any.  §3.9.6's rules are
    /// structural, so the restriction check has to be able to put that group
    /// back; see the re-expansion step in `particle_restricts`.
    pub(super) collapsed_from: Option<Compositor>,
    /// A non-unit group occurrence range was folded into this particle.
    /// Unlike `collapsed_from`, this survives nested unit wrappers, allowing
    /// XSD 1.0 restriction checks to distinguish repeated groups from elements.
    pub(super) folded_group_occurs: bool,
    /// The exact counts of `term` this particle admits, when they are not
    /// every count in `[min_occurs, max_occurs]` — the fold of a repeated
    /// group whose child needs two or more occurrences: `choice{1,2}(b{2,2})`
    /// folded into `b{2,4}` admits two or four `b`s, never three. `None`:
    /// every count in the range. Restriction checks compare these sets, not
    /// the ranges ([`NormalizedParticle::occurrences`]).
    pub(super) counts: Option<Counts>,
    /// The single-child group that `collapse_single_child_groups` folded
    /// into this particle, as it was before the fold — group and child
    /// with their own ranges. XSD 1.0 compares a choice that §3.9.6 clause
    /// 2.2 does not make pointless in this form (`particle_restricts`).
    pub(super) as_written: Option<Box<NormalizedParticle>>,
}

impl NormalizedParticle {
    /// The counts of the term this particle admits: `counts`, or the range.
    pub(super) fn occurrences(&self) -> Counts {
        self.counts
            .unwrap_or_else(|| Counts::range(self.min_occurs, self.max_occurs))
    }

    /// Sets the counts of the term, and the range to their hull (saturated
    /// to `u32`). The set is kept where the range does not say it exactly:
    /// gaps, or bounds beyond `u32`. Gaps are dropped when the term accepts
    /// the empty sequence: `L(T)^c ⊆ L(T)^{c+1}`, so the hull admits the
    /// same sequences.
    pub(super) fn set_occurrences(&mut self, counts: Counts) {
        let counts = if super::particle::term_is_emptiable(&self.term) {
            counts.hull_set()
        } else {
            counts
        };
        (self.min_occurs, self.max_occurs) = counts.hull();
        self.counts = (!counts.is_u32_range()).then_some(counts);
    }
}

#[derive(Debug, Clone)]
pub(super) enum NormalizedParticleTerm {
    Element(NormalizedElement),
    Wildcard(Box<NormalizedWildcard>),
    Group(NormalizedGroup),
}

#[derive(Debug, Clone)]
pub(super) struct NormalizedElement {
    pub(super) name: NameId,
    pub(super) namespace: Option<NameId>,
    pub(super) type_key: TypeKey,
    pub(super) element_key: Option<ElementKey>,
    pub(super) block: DerivationSet,
    pub(super) nillable: bool,
    pub(super) fixed_value: Option<String>,
    /// The declaration the fixed value was written on (its namespace
    /// bindings read a QName value).
    pub(super) fixed_source: Option<SourceRef>,
}

#[derive(Debug, Clone)]
pub(super) struct NormalizedWildcard {
    pub(super) wildcard: WildcardResult,
    pub(super) target_namespace: Option<NameId>,
}

#[derive(Debug, Clone)]
pub(super) struct NormalizedGroup {
    pub(super) compositor: Compositor,
    pub(super) particles: Vec<NormalizedParticle>,
}

struct ParticleNormalizer<'a> {
    schema_set: &'a SchemaSet,
    target_namespace: Option<NameId>,
    resolved_types: &'a [Option<TypeKey>],
    flat_index: usize,
    depth: usize,
}

const MAX_PARTICLE_RESTRICTION_DEPTH: usize = 100;

impl<'a> ParticleNormalizer<'a> {
    fn new(
        schema_set: &'a SchemaSet,
        target_namespace: Option<NameId>,
        resolved_types: &'a [Option<TypeKey>],
    ) -> Self {
        Self {
            schema_set,
            target_namespace,
            resolved_types,
            flat_index: 0,
            depth: 0,
        }
    }

    fn normalize_particle(
        &mut self,
        particle: &ParticleResult,
    ) -> SchemaResult<NormalizedParticle> {
        if self.depth
            >= self
                .schema_set
                .recursion_bound(MAX_PARTICLE_RESTRICTION_DEPTH)
        {
            let bound = self
                .schema_set
                .recursion_bound(MAX_PARTICLE_RESTRICTION_DEPTH);
            return Err(SchemaError::limit(
                crate::error::SchemaLimit::ComponentDepth,
                bound,
                self.depth + 1,
                format!(
                    "particle restriction normalization nests {} levels deep (its recursion \
                     bound; SchemaSet::max_depth is {})",
                    self.depth + 1,
                    self.schema_set.max_depth()
                ),
                None,
            ));
        }

        self.depth += 1;
        let term = match &particle.term {
            ParticleTerm::Element(elem) => {
                let source = particle.source.as_ref().or(elem.source.as_ref());
                NormalizedParticleTerm::Element(self.normalize_element(elem, source)?)
            }
            ParticleTerm::Any(wildcard) => {
                NormalizedParticleTerm::Wildcard(Box::new(NormalizedWildcard {
                    wildcard: wildcard.clone(),
                    target_namespace: self.target_namespace,
                }))
            }
            ParticleTerm::Group(group) => {
                NormalizedParticleTerm::Group(self.normalize_group(group)?)
            }
        };
        self.depth -= 1;

        Ok(collapse_single_child_groups(NormalizedParticle {
            term,
            min_occurs: particle.min_occurs,
            max_occurs: particle.max_occurs,
            source: particle.source.clone(),
            collapsed_from: None,
            folded_group_occurs: false,
            counts: None,
            as_written: None,
        }))
    }

    fn normalize_element(
        &mut self,
        elem: &ElementFrameResult,
        source: Option<&SourceRef>,
    ) -> SchemaResult<NormalizedElement> {
        if let Some(ref_name) = &elem.ref_name {
            let elem_key = self
                .schema_set
                .lookup_element(ref_name.namespace, ref_name.local_name);
            let (type_key, block, nillable, fixed_value, fixed_source) = elem_key
                .and_then(|key| self.schema_set.arenas.elements.get(key))
                .map(|decl| {
                    let (eff_block, _) =
                        crate::compiler::substitution::effective_element_constraints(
                            self.schema_set,
                            decl,
                        );
                    let tk = decl
                        .resolved_type
                        .unwrap_or_else(|| TypeKey::Complex(self.schema_set.any_type_key()));
                    (
                        tk,
                        eff_block,
                        decl.nillable,
                        decl.fixed_value.clone(),
                        decl.source.clone(),
                    )
                })
                .unwrap_or_else(|| {
                    (
                        TypeKey::Complex(self.schema_set.any_type_key()),
                        DerivationSet::empty(),
                        false,
                        None,
                        None,
                    )
                });
            return Ok(NormalizedElement {
                name: ref_name.local_name,
                namespace: ref_name.namespace,
                type_key,
                element_key: elem_key,
                block,
                nillable,
                fixed_value,
                fixed_source,
            });
        }

        let name = elem
            .name
            .ok_or_else(|| SchemaError::internal("element particle missing name and ref"))?;
        let index = self.flat_index;
        self.flat_index += 1;

        let namespace = self.schema_set.effective_local_element_namespace(
            elem.target_namespace,
            elem.form.as_deref(),
            source,
            self.target_namespace,
        );
        let type_key = self
            .resolved_types
            .get(index)
            .copied()
            .flatten()
            .or_else(|| resolve_element_type_ref(self.schema_set, elem))
            .unwrap_or_else(|| TypeKey::Complex(self.schema_set.any_type_key()));

        // Compute effective block for local element
        // block=None means absent → inherit blockDefault; Some(b) means explicit (including "").
        let block = match elem.block {
            Some(b) => b,
            None => source
                .and_then(|s| {
                    let doc_id = s.schema_defaults_doc.unwrap_or(s.doc_id);
                    self.schema_set
                        .documents
                        .get(doc_id as usize)
                        .map(|d| d.block_default)
                })
                .unwrap_or_default(),
        };

        Ok(NormalizedElement {
            name,
            namespace,
            type_key,
            element_key: None,
            block,
            nillable: elem.nillable,
            fixed_value: elem.fixed_value.clone(),
            fixed_source: elem.source.clone().or_else(|| source.cloned()),
        })
    }

    /// XSD 1.1: a group's particles without those that map to no component —
    /// an `<element>`, `<group>`, `<any>` or model group with `minOccurs =
    /// maxOccurs = 0` (Structures §3.3.2, §3.7.2, §3.8.2, §3.10.2). They are
    /// dropped before the group is collapsed or folded, so a choice whose
    /// branches all have `maxOccurs="0"` is an empty choice — no sequence at
    /// all when it must occur — and never an optional branch. XSD 1.0 keeps
    /// them until `remove_pointless_particles`, after the collapse, as its
    /// structural rules were checked against before.
    fn without_absent_particles(
        &self,
        particles: Vec<NormalizedParticle>,
    ) -> Vec<NormalizedParticle> {
        if !self.schema_set.is_xsd11() {
            return particles;
        }
        particles
            .into_iter()
            .filter(|particle| !(particle.min_occurs == 0 && particle.max_occurs == Some(0)))
            .collect()
    }

    fn normalize_group(&mut self, group: &ModelGroupDefResult) -> SchemaResult<NormalizedGroup> {
        if let Some(ref_name) = &group.ref_name {
            let group_key = self
                .schema_set
                .lookup_model_group(ref_name.namespace, ref_name.local_name)
                .ok_or_else(|| SchemaError::internal("model group reference was not resolved"))?;
            let group_data = self
                .schema_set
                .arenas
                .get_model_group(group_key)
                .ok_or_else(|| SchemaError::internal("resolved model group not found"))?;
            let compositor = group_data
                .compositor
                .ok_or_else(|| SchemaError::internal("resolved model group missing compositor"))?;
            let mut nested = ParticleNormalizer::new(
                self.schema_set,
                group_data.target_namespace,
                &group_data.resolved_particle_types,
            );
            nested.depth = self.depth;
            let particles = group_data
                .particles
                .iter()
                .map(|particle| nested.normalize_particle(particle))
                .collect::<SchemaResult<Vec<_>>>()?;
            return Ok(NormalizedGroup {
                compositor,
                particles: nested.without_absent_particles(particles),
            });
        }

        let compositor = group
            .compositor
            .ok_or_else(|| SchemaError::internal("inline model group missing compositor"))?;
        let particles = group
            .particles
            .iter()
            .map(|particle| self.normalize_particle(particle))
            .collect::<SchemaResult<Vec<_>>>()?;
        Ok(NormalizedGroup {
            compositor,
            particles: self.without_absent_particles(particles),
        })
    }
}

fn resolve_element_type_ref(schema_set: &SchemaSet, elem: &ElementFrameResult) -> Option<TypeKey> {
    match &elem.type_ref {
        Some(crate::parser::frames::TypeRefResult::QName(qname)) => schema_set
            .lookup_type(qname.namespace, qname.local_name)
            .or_else(|| schema_set.get_built_in_type_by_qname(qname.namespace, qname.local_name)),
        _ => None,
    }
}

/// Check if a normalized particle is an empty group (all children removed as pointless).
fn is_empty_group(particle: &NormalizedParticle) -> bool {
    matches!(&particle.term, NormalizedParticleTerm::Group(group) if group.particles.is_empty())
}

/// Top-level particle with maxOccurs=0 or fully pruned group — treated as empty content per §3.8.
pub(super) fn is_effectively_empty(particle: &NormalizedParticle) -> bool {
    particle.max_occurs == Some(0) || is_empty_group(particle)
}

pub(super) fn complex_content_particle(content: &ComplexContentResult) -> Option<&ParticleResult> {
    match content {
        ComplexContentResult::Complex(def) => def.particle.as_ref(),
        ComplexContentResult::Empty | ComplexContentResult::Simple(_) => None,
    }
}

/// Walk up the extension chain to find the effective content particle.
/// Empty extensions inherit their base type's content model.
/// Returns (type_def_owning_particle, particle) so the normalizer uses the
/// correct target_namespace and resolved_content_particle_types.
pub(super) fn effective_base_content_particle<'a>(
    schema_set: &'a SchemaSet,
    base: &'a crate::arenas::ComplexTypeDefData,
) -> (
    &'a crate::arenas::ComplexTypeDefData,
    Option<&'a ParticleResult>,
) {
    let mut current = base;
    let mut depth = 0;
    loop {
        if let Some(particle) = complex_content_particle(&current.content) {
            return (current, Some(particle));
        }
        // If this type has no content and was derived by extension, check its base
        if current.derivation_method != Some(DerivationMethod::Extension) {
            return (current, None);
        }
        let Some(TypeKey::Complex(base_key)) = current.resolved_base_type else {
            return (current, None);
        };
        let Some(base_type) = schema_set.arenas.complex_types.get(base_key) else {
            return (current, None);
        };
        depth += 1;
        if depth > schema_set.recursion_bound(50) {
            return (current, None); // safety limit
        }
        current = base_type;
    }
}

/// The base type's *effective* content particle, already normalized.
///
/// §3.4.2.3 (complex content extension): when a type is derived by extension
/// and both the base and the extension contribute a non-empty particle, the
/// resulting {content type} particle is `sequence(base-particle,
/// own-particle)` — not just the extension's own contribution.  A restriction
/// of such a type has to be checked against the whole model, otherwise every
/// element the base contributed looks like an element the restriction invented.
///
/// `effective_base_content_particle` only covers the degenerate case of an
/// extension that adds nothing; this walks the whole extension chain and
/// re-assembles the model.
pub(super) fn normalized_effective_base_particle(
    schema_set: &SchemaSet,
    base: &crate::arenas::ComplexTypeDefData,
    depth: usize,
) -> SchemaResult<Option<NormalizedParticle>> {
    let own = match complex_content_particle(&base.content) {
        Some(particle) => Some(normalize_type_particle(schema_set, base, particle)?),
        None => None,
    };

    if depth > schema_set.recursion_bound(50)
        || base.derivation_method != Some(DerivationMethod::Extension)
    {
        return Ok(own);
    }
    let Some(TypeKey::Complex(base_key)) = base.resolved_base_type else {
        return Ok(own);
    };
    let Some(base_type) = schema_set.arenas.complex_types.get(base_key) else {
        return Ok(own);
    };
    let inherited = normalized_effective_base_particle(schema_set, base_type, depth + 1)?;

    Ok(match (inherited, own) {
        (None, own) => own,
        (inherited, None) => inherited,
        (Some(inherited), Some(own)) => {
            if contributes_only_the_empty_sequence(&inherited) {
                Some(own)
            } else if contributes_only_the_empty_sequence(&own) {
                Some(inherited)
            } else {
                Some(NormalizedParticle {
                    term: NormalizedParticleTerm::Group(NormalizedGroup {
                        compositor: Compositor::Sequence,
                        particles: vec![inherited, own],
                    }),
                    min_occurs: 1,
                    max_occurs: Some(1),
                    source: None,
                    collapsed_from: None,
                    folded_group_occurs: false,
                    counts: None,
                    as_written: None,
                })
            }
        }
    })
}

pub(super) fn normalize_type_particle(
    schema_set: &SchemaSet,
    type_def: &crate::arenas::ComplexTypeDefData,
    particle: &ParticleResult,
) -> SchemaResult<NormalizedParticle> {
    let mut normalizer = ParticleNormalizer::new(
        schema_set,
        type_def.target_namespace,
        &type_def.resolved_content_particle_types,
    );
    let particle = normalizer.normalize_particle(particle)?;
    let particle = remove_pointless_particles(particle, schema_set.is_xsd11());
    // XSD 1.1: skip flattening to preserve structural grouping needed for
    // intensional restriction (e.g. a single-branch choice whose collapsed
    // sequence must match against a multi-branch choice in the base).
    if !schema_set.is_xsd11() {
        let particle = flatten_same_compositor_groups(particle);
        return Ok(particle);
    }
    Ok(particle)
}

/// Normalize a top-level named model group into a `NormalizedParticle` for
/// §src-redefine 6.2.2 restriction comparisons.
///
/// **Chain-of-redefine caveat**: when called on an original whose own
/// particles include `group-ref`s, those refs are resolved via
/// `schema_set.lookup_model_group` inside [`ParticleNormalizer::normalize_group`]
/// — which returns the *currently bound* version. For a chain
/// `orig → v1 → v2`, v1's inner group-refs resolve to whatever the
/// current namespace binding is, not to what v1 saw at creation time. This
/// is a pre-existing limitation shared with the complex-type restriction
/// path; it is not fixed here.
pub(super) fn normalize_model_group_as_particle(
    schema_set: &SchemaSet,
    group_data: &crate::arenas::ModelGroupData,
) -> SchemaResult<NormalizedParticle> {
    let compositor = group_data
        .compositor
        .ok_or_else(|| SchemaError::internal("redefined named model group missing compositor"))?;

    let mut normalizer = ParticleNormalizer::new(
        schema_set,
        group_data.target_namespace,
        &group_data.resolved_particle_types,
    );
    let particles = group_data
        .particles
        .iter()
        .map(|particle| normalizer.normalize_particle(particle))
        .collect::<SchemaResult<Vec<_>>>()?;
    let particles = normalizer.without_absent_particles(particles);

    let wrapper = NormalizedParticle {
        term: NormalizedParticleTerm::Group(NormalizedGroup {
            compositor,
            particles,
        }),
        min_occurs: group_data.min_occurs,
        max_occurs: group_data.max_occurs,
        source: group_data.source.clone(),
        collapsed_from: None,
        folded_group_occurs: false,
        counts: None,
        as_written: None,
    };

    // `normalize_particle` collapses every child; the outer wrapper we
    // built by hand above is *not* collapsed and must be so explicitly so
    // a single-element named group ends up shaped identically to the same
    // content inside a complex type.
    let particle = collapse_single_child_groups(wrapper);
    let particle = remove_pointless_particles(particle, schema_set.is_xsd11());
    // XSD 1.1: skip flattening (same rationale as `normalize_type_particle`).
    if !schema_set.is_xsd11() {
        let particle = flatten_same_compositor_groups(particle);
        return Ok(particle);
    }
    Ok(particle)
}

/// Remove "pointless" particles per Section 3.8 normalization:
/// - Particles with maxOccurs=0 are effectively absent
/// - Groups with no remaining children after removal are also pointless
///
/// Under XSD 1.1 (`keep_empty_branches`) an empty group that accepts the
/// empty sequence stays when it is a branch of a choice: there it is the
/// choice's empty alternative, which the language keeps (§3.8.4.1.2). XSD
/// 1.0 ignores it, as §3.9.6 clause 2.2 calls it pointless.
pub(super) fn remove_pointless_particles(
    mut particle: NormalizedParticle,
    keep_empty_branches: bool,
) -> NormalizedParticle {
    if let NormalizedParticleTerm::Group(group) = &mut particle.term {
        let in_choice = group.compositor == Compositor::Choice;
        group.particles = group
            .particles
            .drain(..)
            .map(|child| remove_pointless_particles(child, keep_empty_branches))
            .filter(|p| {
                p.max_occurs != Some(0)
                    && (!is_pointless_empty_group(p) || keep_empty_branches && in_choice)
            })
            .collect();
    }
    particle
}

/// True when a particle accepts the empty sequence and nothing else, so it can
/// be dropped when assembling `sequence(inherited, own)` for an extension.
///
/// Deliberately narrower than [`is_effectively_empty`]: a *required* empty
/// choice accepts no sequence at all, so dropping it would turn an
/// unsatisfiable content type into a satisfiable one and let a restriction of
/// it be checked against the inherited half alone.
fn contributes_only_the_empty_sequence(particle: &NormalizedParticle) -> bool {
    particle.max_occurs == Some(0) || is_pointless_empty_group(particle)
}

/// A child group particle that contributes nothing to the content model.
///
/// `sequence`/`all` with no particles accepts only the empty sequence however
/// often it is repeated.  An empty `choice` accepts nothing at all, so it is
/// only pointless when it is optional — `<choice minOccurs="1"/>` makes the
/// content model unsatisfiable and must be kept.
fn is_pointless_empty_group(particle: &NormalizedParticle) -> bool {
    match &particle.term {
        NormalizedParticleTerm::Group(group) if group.particles.is_empty() => {
            match group.compositor {
                Compositor::Sequence | Compositor::All => true,
                Compositor::Choice => particle.min_occurs == 0,
            }
        }
        _ => false,
    }
}

/// Flatten nested groups with unit occurs and the same compositor into
/// their parent (Section 3.8 particle normalization).
/// E.g. sequence(sequence{1,1}(a, b), c) → sequence(a, b, c).
pub(super) fn flatten_same_compositor_groups(
    mut particle: NormalizedParticle,
) -> NormalizedParticle {
    if let NormalizedParticleTerm::Group(group) = &mut particle.term {
        // First recurse into children
        group.particles = group
            .particles
            .drain(..)
            .map(flatten_same_compositor_groups)
            .collect();
        // Then flatten children that are same-compositor groups with unit occurs
        let parent_compositor = group.compositor;
        let mut flattened = Vec::with_capacity(group.particles.len());
        for child in group.particles.drain(..) {
            if let NormalizedParticleTerm::Group(ref child_group) = child.term {
                if child_group.compositor == parent_compositor
                    && occurs_is_unit(child.min_occurs, child.max_occurs)
                {
                    flattened.extend(child_group.particles.iter().cloned());
                    continue;
                }
            }
            flattened.push(child);
        }
        group.particles = flattened;
    }
    particle
}

/// Splice every unit-occurs `sequence` child of a `sequence` into its parent,
/// at any depth.
///
/// XSD 1.0 §3.9.6 clause 2.2 ignores such a `<sequence>` when it checks a
/// restriction ("pointless": its particle has {min occurs} and {max occurs}
/// of 1 and is among the {particles} of a `<sequence>`). XSD 1.1 §3.4.6.4
/// compares languages, and a sequence's language is the concatenation of its
/// particles' languages (§3.8.4.1.1), so the splice is sound there too.
///
/// Sequences only: an `all` inside an `all` is left alone (XSD 1.1 matches
/// all-groups unordered, by their own particles).
pub(super) fn flatten_nested_sequences(mut particle: NormalizedParticle) -> NormalizedParticle {
    if let NormalizedParticleTerm::Group(group) = &mut particle.term {
        let children: Vec<_> = group
            .particles
            .drain(..)
            .map(flatten_nested_sequences)
            .collect();
        if group.compositor != Compositor::Sequence {
            group.particles = children;
            return particle;
        }
        for child in children {
            match child.term {
                NormalizedParticleTerm::Group(inner)
                    if inner.compositor == Compositor::Sequence
                        && occurs_is_unit(child.min_occurs, child.max_occurs) =>
                {
                    group.particles.extend(inner.particles);
                }
                term => group.particles.push(NormalizedParticle { term, ..child }),
            }
        }
    }
    particle
}

pub(super) fn collapse_single_child_groups(mut particle: NormalizedParticle) -> NormalizedParticle {
    if let NormalizedParticleTerm::Group(group) = &mut particle.term {
        group.particles = group
            .particles
            .drain(..)
            .map(collapse_single_child_groups)
            .collect();
    }

    loop {
        let folded = match &particle.term {
            NormalizedParticleTerm::Group(group)
                if group.particles.len() == 1
                    && can_collapse_single_child_group(
                        group.compositor,
                        particle.min_occurs,
                        particle.max_occurs,
                        &group.particles[0],
                    ) =>
            {
                fold_into_child(&particle)
            }
            _ => None,
        };
        let Some(folded) = folded else {
            return particle;
        };
        let compositor = match &particle.term {
            NormalizedParticleTerm::Group(group) => Some(group.compositor),
            _ => None,
        };
        particle = NormalizedParticle {
            collapsed_from: compositor,
            as_written: Some(Box::new(particle)),
            ..folded
        };
    }
}

fn can_collapse_single_child_group(
    compositor: Compositor,
    group_min_occurs: u32,
    group_max_occurs: Option<u32>,
    child: &NormalizedParticle,
) -> bool {
    if compositor == Compositor::Choice {
        return true;
    }

    occurs_is_unit(group_min_occurs, group_max_occurs) || child.max_occurs == Some(1)
}

/// The counts of `child`'s term in the group particle `group` around it —
/// `{M, N}` iterations, or the group's own exact counts — with
/// `empty_iteration`, an iteration of the group may also produce none of
/// it: `⋃ₖ [k·m, k·n]`, gaps kept. `None` when they are not one [`Counts`]
/// set (a child with gaps repeated) or need counts beyond its limit (2¹²⁰):
/// the group is then not folded.
pub(super) fn folded_occurrences(
    group: &NormalizedParticle,
    child: &NormalizedParticle,
    empty_iteration: bool,
) -> Option<Counts> {
    if super::particle::term_is_emptiable(&child.term) {
        // `L(T)^c ⊆ L(T)^{c+1}`: the hull admits the same sequences.
        return child
            .occurrences()
            .repeated_hull(group.occurrences(), empty_iteration);
    }
    child
        .occurrences()
        .repeated(group.occurrences(), empty_iteration)
}

/// [`folded_occurrences`], or where that is not one set a subset of the
/// counts that is ([`Counts::repeated_under`]) — for the base side only.
pub(super) fn folded_occurrences_under(
    group: &NormalizedParticle,
    child: &NormalizedParticle,
    empty_iteration: bool,
) -> Option<Counts> {
    folded_occurrences(group, child, empty_iteration).or_else(|| {
        child
            .occurrences()
            .repeated_under(group.occurrences(), empty_iteration)
    })
}

/// The single-child group `particle` folded into its child: the child's
/// term with the counts the group gives it ([`folded_occurrences`]), or
/// `None` when they are not one set.
fn fold_into_child(particle: &NormalizedParticle) -> Option<NormalizedParticle> {
    let NormalizedParticleTerm::Group(group) = &particle.term else {
        return None;
    };
    let [child] = group.particles.as_slice() else {
        return None;
    };
    let counts = folded_occurrences(particle, child, false)?;
    let mut folded = NormalizedParticle {
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
    folded.set_occurrences(counts);
    Some(folded)
}

pub(super) fn occurs_is_unit(min_occurs: u32, max_occurs: Option<u32>) -> bool {
    min_occurs == 1 && max_occurs == Some(1)
}

/// XSD 1.1: fold a single-child sequence/all group by multiplying occurs,
/// `sequence{M,N}(e{m,n})` → `e{M*m, N*n}`, with the exact counts kept
/// ([`NormalizedParticle::counts`]). `None` when it is not such a group or
/// its counts are not one set.
pub(super) fn fold_single_child_group(particle: &NormalizedParticle) -> Option<NormalizedParticle> {
    match &particle.term {
        NormalizedParticleTerm::Group(group)
            if group.particles.len() == 1
                && matches!(group.compositor, Compositor::Sequence | Compositor::All) =>
        {
            fold_into_child(particle)
        }
        _ => None,
    }
}
