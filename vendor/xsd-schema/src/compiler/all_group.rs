//! All-group content model validation
//!
//! This module implements the `xs:all` content model, which allows particles
//! to appear in any order. Unlike sequence/choice groups, all-groups are not
//! compiled to NFAs due to the exponential state explosion that would result
//! from permutation expansion.
//!
//! # XSD Version Differences
//!
//! | Feature | XSD 1.0 | XSD 1.1 |
//! |---------|---------|---------|
//! | Element particles | Yes | Yes |
//! | Wildcard particles | No | Yes |
//! | Group references | No | Yes |
//! | minOccurs | 0 or 1 | Any value |
//! | maxOccurs | 1 only | Any value |

use crate::ids::NameId;
use crate::parser::frames::{ParticleResult, ParticleTerm};
use crate::parser::location::SourceRef;
use crate::schema::model::XsdVersion;
use crate::types::complex::{not_qnames_exclude, NamespaceConstraint, ProcessContents};

use super::error::{NfaCompileError, NfaCompileResult};
use super::nfa::NfaTerm;
use super::particle::MaxOccurs;
use super::substitution::SubstitutionGroupMap;

/// Compiled all-group content model
///
/// Represents an `xs:all` group compiled for validation. All-groups allow
/// their particles to appear in any order, with each particle subject to
/// its occurrence constraints.
#[derive(Debug, Clone)]
pub struct AllGroupModel {
    /// Particles in the all-group
    pub particles: Vec<AllParticle>,
    /// Open content wildcard (XSD 1.1 only)
    pub open_content: Option<OpenContentWildcard>,
    /// Whether the outer particle has minOccurs=0, making the entire group optional.
    /// When true, the content model is satisfied even if no children are consumed.
    pub outer_optional: bool,
}

/// A particle within an all-group
#[derive(Debug, Clone)]
pub struct AllParticle {
    /// The term that must be matched
    pub term: NfaTerm,
    /// Minimum required occurrences
    pub min_occurs: u32,
    /// Maximum allowed occurrences
    pub max_occurs: MaxOccurs,
    /// Source location for error reporting
    pub source: Option<SourceRef>,
}

/// Open content wildcard for XSD 1.1
#[derive(Debug, Clone)]
pub struct OpenContentWildcard {
    /// Namespace constraint for allowed namespaces
    pub namespace_constraint: NamespaceConstraint,
    /// How to process matched content
    pub process_contents: ProcessContents,
    /// Open content mode
    pub mode: OpenContentMode,
    /// Pre-expanded concrete QName exclusions (XSD 1.1 notQName)
    pub not_qnames: Vec<(Option<NameId>, NameId)>,
}

/// Open content mode for XSD 1.1
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OpenContentMode {
    /// No open content
    #[default]
    None,
    /// Open content can be interleaved
    Interleave,
    /// Open content only at the end
    Suffix,
}

/// Mutable state during all-group validation
///
/// Tracks how many times each particle has been matched (consumed count).
#[derive(Debug, Clone)]
pub struct AllGroupState {
    /// Number of times each particle has been matched (by index)
    consumed: Vec<u32>,
}

impl AllGroupModel {
    /// Create a new all-group model
    pub fn new(particles: Vec<AllParticle>) -> Self {
        Self {
            particles,
            open_content: None,
            outer_optional: false,
        }
    }

    /// Create an all-group model with open content
    pub fn with_open_content(
        particles: Vec<AllParticle>,
        open_content: OpenContentWildcard,
    ) -> Self {
        Self {
            particles,
            open_content: Some(open_content),
            outer_optional: false,
        }
    }

    /// Check if the all-group is empty
    pub fn is_empty(&self) -> bool {
        self.particles.is_empty()
    }

    /// Get the number of particles
    pub fn particle_count(&self) -> usize {
        self.particles.len()
    }

    /// Check if all particles are optional (can match empty sequence)
    pub fn is_optional(&self) -> bool {
        self.particles.iter().all(|p| p.is_optional())
    }

    /// Create a validation state for this model
    pub fn create_state(&self) -> AllGroupState {
        AllGroupState::new(self)
    }
}

impl AllParticle {
    /// Create a new all-particle
    pub fn new(
        term: NfaTerm,
        min_occurs: u32,
        max_occurs: MaxOccurs,
        source: Option<SourceRef>,
    ) -> Self {
        Self {
            term,
            min_occurs,
            max_occurs,
            source,
        }
    }

    /// Check if this particle is optional (minOccurs = 0)
    pub fn is_optional(&self) -> bool {
        self.min_occurs == 0
    }

    /// Check if the given occurrence count satisfies minOccurs
    pub fn is_satisfied(&self, consumed: u32) -> bool {
        consumed >= self.min_occurs
    }
}

impl AllGroupState {
    /// Create a new validation state for an all-group
    pub fn new(model: &AllGroupModel) -> Self {
        Self {
            consumed: vec![0; model.particles.len()],
        }
    }

    /// Reset the state for a new validation run
    pub fn reset(&mut self, model: &AllGroupModel) {
        self.consumed.clear();
        self.consumed.resize(model.particles.len(), 0);
    }

    /// Check if a particle can still accept matches
    pub fn can_accept(&self, model: &AllGroupModel, index: usize) -> bool {
        if let (Some(&count), Some(particle)) =
            (self.consumed.get(index), model.particles.get(index))
        {
            match particle.max_occurs {
                MaxOccurs::Unbounded => true,
                MaxOccurs::Bounded(max) => count < max,
            }
        } else {
            false
        }
    }

    /// Accept a match for the particle at the given index
    ///
    /// Returns true if the match was accepted, false if the particle
    /// cannot accept any more matches.
    pub fn accept(&mut self, model: &AllGroupModel, index: usize) -> bool {
        if self.can_accept(model, index) {
            self.consumed[index] += 1;
            true
        } else {
            false
        }
    }

    /// Get how many times a particle has been matched
    pub fn consumed(&self, index: usize) -> u32 {
        self.consumed.get(index).copied().unwrap_or(0)
    }

    /// Check if all particles have satisfied their minOccurs constraints
    pub fn is_satisfied(&self, model: &AllGroupModel) -> bool {
        for (i, particle) in model.particles.iter().enumerate() {
            if !particle.is_satisfied(self.consumed(i)) {
                return false;
            }
        }
        true
    }

    /// Check if any particle has been consumed at all
    pub fn has_any_consumed(&self) -> bool {
        self.consumed.iter().any(|&c| c > 0)
    }

    /// Get indices of particles that have not satisfied their minOccurs
    pub fn unsatisfied_indices(&self, model: &AllGroupModel) -> Vec<usize> {
        let mut result = Vec::new();
        for (i, particle) in model.particles.iter().enumerate() {
            if !particle.is_satisfied(self.consumed(i)) {
                result.push(i);
            }
        }
        result
    }
}

/// Validate all-group constraints based on XSD version
///
/// XSD 1.0 has strict constraints on what can appear in an all-group:
/// - Only element particles (no wildcards, no group references)
/// - minOccurs must be 0 or 1
/// - maxOccurs must be 0 or 1 (`maxOccurs="0"` is the standard idiom for
///   forbidding an element via restriction; see W3C XSD 1.0 §3.8.6
///   cos-all-limited and the conformance test mgA015)
///
/// XSD 1.1 relaxes these constraints to allow wildcards and arbitrary
/// occurrence values. Group references are allowed but must satisfy
/// cos-all-limited constraints:
/// - **Rule 1.3**: minOccurs = maxOccurs = 1
/// - **Rule 2**: referenced group must have compositor = all
///   (compositor check requires schema resolution, so only the occurrence
///   constraint is validated here; compositor is checked during compilation)
/// - Must be a group reference (`ref_name` set), not an inline group
pub fn validate_all_group_constraints(
    particles: &[ParticleResult],
    xsd_version: XsdVersion,
    source: Option<SourceRef>,
) -> NfaCompileResult<()> {
    match xsd_version {
        XsdVersion::V1_0 => validate_all_group_xsd10(particles, source),
        #[cfg(feature = "xsd11")]
        XsdVersion::V1_1 => validate_all_group_xsd11(particles, source),
        #[cfg(not(feature = "xsd11"))]
        XsdVersion::V1_1 => validate_all_group_xsd10(particles, source),
    }
}

/// Validate XSD 1.1 all-group constraints (cos-all-limited)
#[cfg(feature = "xsd11")]
fn validate_all_group_xsd11(
    particles: &[ParticleResult],
    source: Option<SourceRef>,
) -> NfaCompileResult<()> {
    for particle in particles {
        if let ParticleTerm::Group(group) = &particle.term {
            // cos-all-limited 1.3: group ref must have minOccurs = maxOccurs = 1
            if particle.min_occurs != 1 || particle.max_occurs != Some(1) {
                return Err(NfaCompileError::InvalidAllGroupOccurs {
                    reason: "cos-all-limited.1.3: group reference inside xs:all \
                             must have minOccurs = maxOccurs = 1"
                        .into(),
                    location: particle.source.clone().or(source.clone()),
                });
            }
            // Must be a group reference, not an inline group
            if group.ref_name.is_none() {
                return Err(NfaCompileError::InvalidAllGroupContent {
                    location: particle.source.clone().or(source.clone()),
                });
            }
        }
    }
    Ok(())
}

/// Validate XSD 1.0 all-group constraints
fn validate_all_group_xsd10(
    particles: &[ParticleResult],
    source: Option<SourceRef>,
) -> NfaCompileResult<()> {
    for particle in particles {
        // XSD 1.0: Only element particles allowed
        if !matches!(particle.term, ParticleTerm::Element(_)) {
            return Err(NfaCompileError::InvalidAllGroupContent {
                location: particle.source.clone().or(source.clone()),
            });
        }

        // XSD 1.0: minOccurs must be 0 or 1
        if particle.min_occurs > 1 {
            return Err(NfaCompileError::InvalidAllGroupOccurs {
                reason: format!(
                    "minOccurs must be 0 or 1 in XSD 1.0 all-group, found {}",
                    particle.min_occurs
                ),
                location: particle.source.clone().or(source.clone()),
            });
        }

        // XSD 1.0: maxOccurs must be 0 or 1
        match particle.max_occurs {
            Some(0) | Some(1) => {} // OK
            Some(n) => {
                return Err(NfaCompileError::InvalidAllGroupOccurs {
                    reason: format!("maxOccurs must be 0 or 1 in XSD 1.0 all-group, found {}", n),
                    location: particle.source.clone().or(source.clone()),
                });
            }
            None => {
                return Err(NfaCompileError::InvalidAllGroupOccurs {
                    reason: "maxOccurs='unbounded' not allowed in XSD 1.0 all-group".to_string(),
                    location: particle.source.clone().or(source.clone()),
                });
            }
        }
    }

    Ok(())
}

/// Term matching result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TermMatchResult {
    /// Term matched
    Match,
    /// Term did not match
    NoMatch,
}

/// Match an element name against an NfaTerm
pub fn term_matches(
    term: &NfaTerm,
    element_name: NameId,
    element_namespace: Option<NameId>,
    target_namespace: Option<NameId>,
    xsd_version: XsdVersion,
) -> TermMatchResult {
    term_matches_with_substitution(
        term,
        element_name,
        element_namespace,
        target_namespace,
        None,
        xsd_version,
    )
}

/// Match an element name against an NfaTerm with optional substitution groups.
///
/// `_xsd_version` is not used: no wildcard namespace constraint depends on the
/// XSD version (`NamespaceConstraint::matches`).
pub fn term_matches_with_substitution(
    term: &NfaTerm,
    element_name: NameId,
    element_namespace: Option<NameId>,
    target_namespace: Option<NameId>,
    substitution_groups: Option<&SubstitutionGroupMap>,
    _xsd_version: XsdVersion,
) -> TermMatchResult {
    match term {
        NfaTerm::Element {
            name,
            namespace,
            element_key,
            ..
        } => {
            if let (Some(map), Some(key)) = (substitution_groups, element_key) {
                if let Some(names) = map.get(key) {
                    return if names.contains(&(element_name, element_namespace)) {
                        TermMatchResult::Match
                    } else {
                        TermMatchResult::NoMatch
                    };
                }
            }

            if *name == element_name && *namespace == element_namespace {
                TermMatchResult::Match
            } else {
                TermMatchResult::NoMatch
            }
        }
        NfaTerm::Wildcard {
            namespace_constraint,
            not_qnames,
            ..
        } => {
            if !namespace_constraint.matches(element_namespace, target_namespace) {
                return TermMatchResult::NoMatch;
            }
            if not_qnames_exclude(not_qnames, element_namespace, element_name) {
                return TermMatchResult::NoMatch;
            }
            TermMatchResult::Match
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::build_substitution_group_map;
    use crate::ids::NameId;
    use crate::schema::model::{DerivationSet, SchemaSet};

    fn element_data(
        name: NameId,
        target_namespace: Option<NameId>,
    ) -> crate::arenas::ElementDeclData {
        crate::arenas::ElementDeclData {
            name: Some(name),
            target_namespace,
            ref_name: None,
            type_ref: None,
            inline_type: None,
            substitution_group: Vec::new(),
            default_value: None,
            fixed_value: None,
            nillable: false,
            is_abstract: false,
            min_occurs: 1,
            max_occurs: Some(1),
            block: DerivationSet::empty(),
            final_derivation: DerivationSet::empty(),
            form: None,
            id: None,
            alternatives: Vec::new(),
            identity_constraints: Vec::new(),
            pending_ic_refs: vec![],
            annotation: None,
            source: None,
            resolved_type: None,
            resolved_ref: None,
            resolved_substitution_groups: Vec::new(),
            deferred_type_error: None,
        }
    }

    fn make_element_term(name: u32) -> NfaTerm {
        NfaTerm::element(NameId(name), None, None)
    }

    fn make_particle(name: u32, min: u32, max: MaxOccurs) -> AllParticle {
        AllParticle::new(make_element_term(name), min, max, None)
    }

    #[test]
    fn test_all_group_model_new() {
        let particles = vec![
            make_particle(1, 1, MaxOccurs::Bounded(1)),
            make_particle(2, 0, MaxOccurs::Bounded(1)),
        ];
        let model = AllGroupModel::new(particles);

        assert_eq!(model.particle_count(), 2);
        assert!(!model.is_empty());
        assert!(!model.is_optional()); // First particle is required
    }

    #[test]
    fn test_all_group_model_optional() {
        let particles = vec![
            make_particle(1, 0, MaxOccurs::Bounded(1)),
            make_particle(2, 0, MaxOccurs::Bounded(1)),
        ];
        let model = AllGroupModel::new(particles);

        assert!(model.is_optional()); // All particles are optional
    }

    #[test]
    fn test_all_particle_is_optional() {
        let required = make_particle(1, 1, MaxOccurs::Bounded(1));
        let optional = make_particle(2, 0, MaxOccurs::Bounded(1));

        assert!(!required.is_optional());
        assert!(optional.is_optional());
    }

    #[test]
    fn test_all_particle_is_satisfied() {
        let particle = make_particle(1, 2, MaxOccurs::Bounded(5));

        assert!(!particle.is_satisfied(0));
        assert!(!particle.is_satisfied(1));
        assert!(particle.is_satisfied(2));
        assert!(particle.is_satisfied(3));
    }

    #[test]
    fn test_all_group_state_new() {
        let particles = vec![
            make_particle(1, 1, MaxOccurs::Bounded(2)),
            make_particle(2, 0, MaxOccurs::Bounded(1)),
        ];
        let model = AllGroupModel::new(particles);
        let state = model.create_state();

        assert!(state.can_accept(&model, 0));
        assert!(state.can_accept(&model, 1));
    }

    #[test]
    fn test_all_group_state_accept() {
        let particles = vec![make_particle(1, 1, MaxOccurs::Bounded(2))];
        let model = AllGroupModel::new(particles);
        let mut state = model.create_state();

        assert!(state.can_accept(&model, 0));
        assert!(state.accept(&model, 0));
        assert!(state.can_accept(&model, 0)); // Still has 1 remaining
        assert!(state.accept(&model, 0));
        assert!(!state.can_accept(&model, 0)); // No more remaining
        assert!(!state.accept(&model, 0)); // Should return false
    }

    #[test]
    fn test_all_group_state_accept_unbounded() {
        let particles = vec![make_particle(1, 1, MaxOccurs::Unbounded)];
        let model = AllGroupModel::new(particles);
        let mut state = model.create_state();

        for _ in 0..1000 {
            assert!(state.can_accept(&model, 0));
            assert!(state.accept(&model, 0));
        }
        assert!(state.can_accept(&model, 0)); // Still accepting
    }

    #[test]
    fn test_all_group_state_is_satisfied() {
        let particles = vec![
            make_particle(1, 1, MaxOccurs::Bounded(2)), // Required
            make_particle(2, 0, MaxOccurs::Bounded(1)), // Optional
        ];
        let model = AllGroupModel::new(particles);
        let mut state = model.create_state();

        assert!(!state.is_satisfied(&model)); // First particle not satisfied

        state.accept(&model, 0); // Match first particle once
        assert!(state.is_satisfied(&model)); // Now satisfied
    }

    #[test]
    fn test_all_group_state_unsatisfied_indices() {
        let particles = vec![
            make_particle(1, 1, MaxOccurs::Bounded(1)),
            make_particle(2, 1, MaxOccurs::Bounded(1)),
            make_particle(3, 0, MaxOccurs::Bounded(1)),
        ];
        let model = AllGroupModel::new(particles);
        let mut state = model.create_state();

        let unsatisfied = state.unsatisfied_indices(&model);
        assert_eq!(unsatisfied, vec![0, 1]); // Particles 0 and 1 require matching

        state.accept(&model, 0);
        let unsatisfied = state.unsatisfied_indices(&model);
        assert_eq!(unsatisfied, vec![1]); // Only particle 1 unsatisfied now
    }

    #[test]
    fn test_term_matches_element() {
        let term = NfaTerm::element(NameId(1), Some(NameId(100)), None);

        assert_eq!(
            term_matches(&term, NameId(1), Some(NameId(100)), None, XsdVersion::V1_0),
            TermMatchResult::Match
        );
        assert_eq!(
            term_matches(&term, NameId(2), Some(NameId(100)), None, XsdVersion::V1_0),
            TermMatchResult::NoMatch
        );
        assert_eq!(
            term_matches(&term, NameId(1), Some(NameId(200)), None, XsdVersion::V1_0),
            TermMatchResult::NoMatch
        );
    }

    #[test]
    fn test_term_matches_wildcard_any() {
        let term = NfaTerm::wildcard(NamespaceConstraint::Any, ProcessContents::Lax);

        assert_eq!(
            term_matches(&term, NameId(1), Some(NameId(100)), None, XsdVersion::V1_0),
            TermMatchResult::Match
        );
        assert_eq!(
            term_matches(&term, NameId(999), None, None, XsdVersion::V1_0),
            TermMatchResult::Match
        );
    }

    #[test]
    fn test_term_matches_wildcard_other() {
        let term = NfaTerm::wildcard(NamespaceConstraint::Other, ProcessContents::Lax);
        let target_ns = Some(NameId(100));

        assert_eq!(
            term_matches(
                &term,
                NameId(1),
                Some(NameId(200)),
                target_ns,
                XsdVersion::V1_0
            ),
            TermMatchResult::Match
        );
        assert_eq!(
            term_matches(&term, NameId(1), target_ns, target_ns, XsdVersion::V1_0),
            TermMatchResult::NoMatch
        );
    }

    #[test]
    fn test_term_matches_substitution_group_member() {
        let mut schema_set = SchemaSet::new();
        let head_name = schema_set.name_table.add("head");
        let member_name = schema_set.name_table.add("member");

        let head_key = schema_set
            .arenas
            .alloc_element(element_data(head_name, None));
        let member_key = schema_set
            .arenas
            .alloc_element(element_data(member_name, None));

        schema_set
            .arenas
            .entries_mut()
            .elements
            .get_mut(member_key)
            .unwrap()
            .resolved_substitution_groups
            .push(head_key);

        let map = build_substitution_group_map(&schema_set);
        let term = NfaTerm::element(head_name, None, Some(head_key));

        assert_eq!(
            term_matches_with_substitution(
                &term,
                member_name,
                None,
                None,
                Some(&map),
                XsdVersion::V1_0
            ),
            TermMatchResult::Match
        );
    }

    #[test]
    fn test_term_matches_substitution_group_abstract_head() {
        let mut schema_set = SchemaSet::new();
        let head_name = schema_set.name_table.add("head");
        let member_name = schema_set.name_table.add("member");

        let mut head = element_data(head_name, None);
        head.is_abstract = true;
        let head_key = schema_set.arenas.alloc_element(head);
        let member_key = schema_set
            .arenas
            .alloc_element(element_data(member_name, None));
        schema_set
            .arenas
            .entries_mut()
            .elements
            .get_mut(member_key)
            .unwrap()
            .resolved_substitution_groups
            .push(head_key);

        let map = build_substitution_group_map(&schema_set);
        let term = NfaTerm::element(head_name, None, Some(head_key));

        assert_eq!(
            term_matches_with_substitution(
                &term,
                head_name,
                None,
                None,
                Some(&map),
                XsdVersion::V1_0
            ),
            TermMatchResult::NoMatch
        );
        assert_eq!(
            term_matches_with_substitution(
                &term,
                member_name,
                None,
                None,
                Some(&map),
                XsdVersion::V1_0
            ),
            TermMatchResult::Match
        );
    }
}
