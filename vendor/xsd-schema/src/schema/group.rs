//! Model groups and attribute groups
//!
//! This module defines named model groups (xs:group) and attribute groups (xs:attributeGroup).

use crate::ids::{AttributeGroupKey, ModelGroupKey, NameId};
use crate::parser::location::SourceRef;
use crate::types::complex::{AttributeUse, AttributeWildcard, Compositor, ContentParticle};

/// Named model group definition (xs:group)
///
/// Represents a reusable content model that can be referenced by complex types.
#[derive(Debug, Clone)]
pub struct ModelGroupDef {
    /// Group name (required for global groups)
    pub name: Option<NameId>,

    /// Target namespace
    pub target_namespace: Option<NameId>,

    /// Source location for error reporting
    pub source: Option<SourceRef>,

    /// The compositor (sequence, choice, or all)
    pub compositor: Compositor,

    /// Child particles
    pub particles: Vec<ContentParticle>,

    /// ID attribute value
    pub id: Option<String>,
}

impl ModelGroupDef {
    /// Create a new named model group
    pub fn new(name: NameId, compositor: Compositor) -> Self {
        Self {
            name: Some(name),
            target_namespace: None,
            source: None,
            compositor,
            particles: Vec::new(),
            id: None,
        }
    }

    /// Create a new anonymous model group
    pub fn anonymous(compositor: Compositor) -> Self {
        Self {
            name: None,
            target_namespace: None,
            source: None,
            compositor,
            particles: Vec::new(),
            id: None,
        }
    }

    /// Check if this is a named (global) group
    pub fn is_named(&self) -> bool {
        self.name.is_some()
    }

    /// Check if this group is empty
    pub fn is_empty(&self) -> bool {
        self.particles.is_empty()
    }

    /// Add a particle to this group
    pub fn add_particle(&mut self, particle: ContentParticle) {
        self.particles.push(particle);
    }
}

/// Reference to a model group
#[derive(Debug, Clone)]
pub enum ModelGroupRef {
    /// Resolved reference to a named group
    Resolved(ModelGroupKey),
    /// Unresolved reference
    Unresolved {
        namespace: Option<NameId>,
        local_name: NameId,
    },
}

/// Attribute group definition (xs:attributeGroup)
///
/// Represents a reusable collection of attribute declarations and wildcards.
#[derive(Debug, Clone)]
pub struct AttributeGroupDef {
    /// Group name (required for global groups)
    pub name: Option<NameId>,

    /// Target namespace
    pub target_namespace: Option<NameId>,

    /// Source location for error reporting
    pub source: Option<SourceRef>,

    /// Attribute uses in this group
    pub attributes: Vec<AttributeUse>,

    /// Attribute group references
    pub attribute_group_refs: Vec<AttributeGroupRef>,

    /// Attribute wildcard (anyAttribute)
    pub attribute_wildcard: Option<AttributeWildcard>,

    /// ID attribute value
    pub id: Option<String>,
}

impl AttributeGroupDef {
    /// Create a new named attribute group
    pub fn new(name: NameId) -> Self {
        Self {
            name: Some(name),
            target_namespace: None,
            source: None,
            attributes: Vec::new(),
            attribute_group_refs: Vec::new(),
            attribute_wildcard: None,
            id: None,
        }
    }

    /// Create a new anonymous attribute group
    pub fn anonymous() -> Self {
        Self {
            name: None,
            target_namespace: None,
            source: None,
            attributes: Vec::new(),
            attribute_group_refs: Vec::new(),
            attribute_wildcard: None,
            id: None,
        }
    }

    /// Check if this is a named (global) group
    pub fn is_named(&self) -> bool {
        self.name.is_some()
    }

    /// Check if this group is empty
    pub fn is_empty(&self) -> bool {
        self.attributes.is_empty()
            && self.attribute_group_refs.is_empty()
            && self.attribute_wildcard.is_none()
    }

    /// Add an attribute use to this group
    pub fn add_attribute(&mut self, attr_use: AttributeUse) {
        self.attributes.push(attr_use);
    }

    /// Add an attribute group reference
    pub fn add_attribute_group_ref(&mut self, ref_: AttributeGroupRef) {
        self.attribute_group_refs.push(ref_);
    }
}

/// Reference to an attribute group
#[derive(Debug, Clone)]
pub enum AttributeGroupRef {
    /// Resolved reference
    Resolved(AttributeGroupKey),
    /// Unresolved reference
    Unresolved {
        namespace: Option<NameId>,
        local_name: NameId,
    },
}

/// Particle occurrence constraints
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Occurrence {
    /// Minimum occurrences
    pub min: u32,
    /// Maximum occurrences (None = unbounded)
    pub max: Option<u32>,
}

impl Occurrence {
    /// Default occurrence (1..1)
    pub const ONCE: Occurrence = Occurrence {
        min: 1,
        max: Some(1),
    };

    /// Optional occurrence (0..1)
    pub const OPTIONAL: Occurrence = Occurrence {
        min: 0,
        max: Some(1),
    };

    /// Unbounded occurrence (0..unbounded)
    pub const UNBOUNDED: Occurrence = Occurrence { min: 0, max: None };

    /// Required with unbounded max (1..unbounded)
    pub const ONE_OR_MORE: Occurrence = Occurrence { min: 1, max: None };

    /// Create a custom occurrence
    pub fn new(min: u32, max: Option<u32>) -> Self {
        Self { min, max }
    }

    /// Check if this occurrence is optional
    pub fn is_optional(&self) -> bool {
        self.min == 0
    }

    /// Check if this occurrence is unbounded
    pub fn is_unbounded(&self) -> bool {
        self.max.is_none()
    }

    /// Check if this occurrence allows multiple
    pub fn allows_multiple(&self) -> bool {
        self.max.is_none_or(|m| m > 1)
    }

    /// Check if this occurrence is exactly once
    pub fn is_once(&self) -> bool {
        self.min == 1 && self.max == Some(1)
    }
}

impl Default for Occurrence {
    fn default() -> Self {
        Self::ONCE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_group_def() {
        let group = ModelGroupDef::new(NameId(1), Compositor::Sequence);
        assert!(group.is_named());
        assert!(group.is_empty());
        assert_eq!(group.compositor, Compositor::Sequence);
    }

    #[test]
    fn test_anonymous_model_group() {
        let group = ModelGroupDef::anonymous(Compositor::Choice);
        assert!(!group.is_named());
        assert_eq!(group.compositor, Compositor::Choice);
    }

    #[test]
    fn test_attribute_group_def() {
        let group = AttributeGroupDef::new(NameId(1));
        assert!(group.is_named());
        assert!(group.is_empty());
    }

    #[test]
    fn test_anonymous_attribute_group() {
        let group = AttributeGroupDef::anonymous();
        assert!(!group.is_named());
        assert!(group.is_empty());
    }

    #[test]
    fn test_occurrence_constants() {
        assert!(Occurrence::ONCE.is_once());
        assert!(!Occurrence::ONCE.is_optional());
        assert!(!Occurrence::ONCE.is_unbounded());

        assert!(Occurrence::OPTIONAL.is_optional());
        assert!(!Occurrence::OPTIONAL.allows_multiple());

        assert!(Occurrence::UNBOUNDED.is_unbounded());
        assert!(Occurrence::UNBOUNDED.allows_multiple());

        assert!(!Occurrence::ONE_OR_MORE.is_optional());
        assert!(Occurrence::ONE_OR_MORE.is_unbounded());
    }

    #[test]
    fn test_occurrence_custom() {
        let occ = Occurrence::new(2, Some(5));
        assert!(!occ.is_optional());
        assert!(!occ.is_unbounded());
        assert!(occ.allows_multiple());
    }
}
