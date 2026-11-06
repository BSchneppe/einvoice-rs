//! Wildcard specifications
//!
//! This module defines wildcards for xs:any and xs:anyAttribute elements.

use crate::ids::NameId;
use crate::parser::location::SourceRef;

/// Namespace constraint for wildcards
///
/// Specifies which namespaces are allowed by a wildcard.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum NamespaceConstraint {
    /// Any namespace allowed (##any)
    #[default]
    Any,

    /// Other namespaces allowed (##other) - excludes the target namespace
    /// and absent (no namespace)
    Other,

    /// Specific set of namespaces
    /// None in the set represents "no namespace" (##local)
    Enumeration(Vec<Option<NameId>>),

    /// Not these namespaces (XSD 1.1 notNamespace)
    Not(Vec<Option<NameId>>),
}

impl NamespaceConstraint {
    /// Create a constraint for "##any"
    pub fn any() -> Self {
        NamespaceConstraint::Any
    }

    /// Create a constraint for "##other"
    pub fn other() -> Self {
        NamespaceConstraint::Other
    }

    /// Create a constraint for "##targetNamespace"
    pub fn target_namespace(ns: Option<NameId>) -> Self {
        NamespaceConstraint::Enumeration(vec![ns])
    }

    /// Create a constraint for "##local"
    pub fn local() -> Self {
        NamespaceConstraint::Enumeration(vec![None])
    }

    /// Create a constraint from a list of namespaces
    pub fn list(namespaces: Vec<Option<NameId>>) -> Self {
        NamespaceConstraint::Enumeration(namespaces)
    }

    /// Check if this constraint allows a given namespace. No constraint kind
    /// depends on the XSD version (`##other` is the same in both,
    /// [`other_matches_namespace`](crate::types::complex::other_matches_namespace)).
    pub fn allows(&self, ns: Option<NameId>, target_ns: Option<NameId>) -> bool {
        match self {
            NamespaceConstraint::Any => true,
            NamespaceConstraint::Other => {
                crate::types::complex::other_matches_namespace(ns, target_ns)
            }
            NamespaceConstraint::Enumeration(allowed) => allowed.contains(&ns),
            NamespaceConstraint::Not(disallowed) => !disallowed.contains(&ns),
        }
    }
}

/// Process contents directive
///
/// Specifies how wildcard content should be validated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProcessContents {
    /// Strictly validate - schema must be available, content must be valid
    #[default]
    Strict,

    /// Laxly validate - validate if schema available, skip otherwise
    Lax,

    /// Skip validation entirely
    Skip,
}

impl std::str::FromStr for ProcessContents {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "strict" => Ok(ProcessContents::Strict),
            "lax" => Ok(ProcessContents::Lax),
            "skip" => Ok(ProcessContents::Skip),
            _ => Err(()),
        }
    }
}

impl ProcessContents {
    /// Convert to string
    pub fn as_str(&self) -> &'static str {
        match self {
            ProcessContents::Strict => "strict",
            ProcessContents::Lax => "lax",
            ProcessContents::Skip => "skip",
        }
    }
}

/// Element wildcard (xs:any)
///
/// Specifies a wildcard allowing any elements from specified namespaces.
#[derive(Debug, Clone)]
pub struct ElementWildcard {
    /// Namespace constraint
    pub namespace_constraint: NamespaceConstraint,

    /// Process contents directive
    pub process_contents: ProcessContents,

    /// Minimum occurrences
    pub min_occurs: u32,

    /// Maximum occurrences (None = unbounded)
    pub max_occurs: Option<u32>,

    /// Source location for error reporting
    pub source: Option<SourceRef>,

    /// ID attribute value
    pub id: Option<String>,

    /// XSD 1.1: notQName exclusions (populated by compiler, checked at validation)
    pub not_qnames: Vec<QNameDisallowed>,
}

impl ElementWildcard {
    /// Create a new element wildcard
    pub fn new() -> Self {
        Self {
            namespace_constraint: NamespaceConstraint::Any,
            process_contents: ProcessContents::Strict,
            min_occurs: 1,
            max_occurs: Some(1),
            source: None,
            id: None,
            not_qnames: Vec::new(),
        }
    }

    /// Create a wildcard with ##any namespace and lax processing
    pub fn any_lax() -> Self {
        Self {
            namespace_constraint: NamespaceConstraint::Any,
            process_contents: ProcessContents::Lax,
            min_occurs: 0,
            max_occurs: None,
            source: None,
            id: None,
            not_qnames: Vec::new(),
        }
    }

    /// Check if this wildcard is optional
    pub fn is_optional(&self) -> bool {
        self.min_occurs == 0
    }

    /// Check if this wildcard is unbounded
    pub fn is_unbounded(&self) -> bool {
        self.max_occurs.is_none()
    }
}

impl Default for ElementWildcard {
    fn default() -> Self {
        Self::new()
    }
}

/// Attribute wildcard (xs:anyAttribute)
///
/// Specifies a wildcard allowing any attributes from specified namespaces.
#[derive(Debug, Clone)]
pub struct AttributeWildcard {
    /// Namespace constraint
    pub namespace_constraint: NamespaceConstraint,

    /// Process contents directive
    pub process_contents: ProcessContents,

    /// Source location for error reporting
    pub source: Option<SourceRef>,

    /// ID attribute value
    pub id: Option<String>,

    /// XSD 1.1: notQName exclusions (populated by compiler, checked at validation)
    pub not_qnames: Vec<QNameDisallowed>,
}

impl AttributeWildcard {
    /// Create a new attribute wildcard
    pub fn new() -> Self {
        Self {
            namespace_constraint: NamespaceConstraint::Any,
            process_contents: ProcessContents::Strict,
            source: None,
            id: None,
            not_qnames: Vec::new(),
        }
    }

    /// Create a wildcard with ##any namespace and lax processing
    pub fn any_lax() -> Self {
        Self {
            namespace_constraint: NamespaceConstraint::Any,
            process_contents: ProcessContents::Lax,
            source: None,
            id: None,
            not_qnames: Vec::new(),
        }
    }
}

impl Default for AttributeWildcard {
    fn default() -> Self {
        Self::new()
    }
}

/// XSD 1.1: Disallowed QName for notQName constraint
#[derive(Debug, Clone)]
pub enum QNameDisallowed {
    /// Specific QName that is disallowed
    QName {
        namespace: Option<NameId>,
        local_name: NameId,
    },
    /// ##defined - disallow elements defined in schema
    Defined,
    /// ##definedSibling - disallow sibling elements
    DefinedSibling,
}

// The stubbed `WildcardUnion::union` / `::intersection` were removed — the
// canonical §3.10.6.3 cos-aw-union lives in `schema::derivation::wildcard_result_union`
// (added by B1, extended by B3). No production code depended on the stubs.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_namespace_constraint_any() {
        let constraint = NamespaceConstraint::any();
        assert!(constraint.allows(Some(NameId(1)), None));
        assert!(constraint.allows(None, None));
    }

    #[test]
    fn test_namespace_constraint_other() {
        let constraint = NamespaceConstraint::other();
        let target = Some(NameId(1));

        assert!(!constraint.allows(target, target)); // Same as target - not allowed
        assert!(constraint.allows(Some(NameId(2)), target)); // Different - allowed
                                                             // XSD 1.0 and 1.1: absent namespace is NOT allowed by ##other
                                                             // (§3.10.4: when the schema has a target namespace, ##other excludes
                                                             // both the target ns and the absent ns).
        assert!(!constraint.allows(None, target));
        assert!(!constraint.allows(None, target));
    }

    #[test]
    fn test_namespace_constraint_enumeration() {
        let constraint = NamespaceConstraint::list(vec![Some(NameId(1)), Some(NameId(2))]);

        assert!(constraint.allows(Some(NameId(1)), None));
        assert!(constraint.allows(Some(NameId(2)), None));
        assert!(!constraint.allows(Some(NameId(3)), None));
    }

    #[test]
    fn test_process_contents_parsing() {
        assert_eq!("strict".parse(), Ok(ProcessContents::Strict));
        assert_eq!("lax".parse(), Ok(ProcessContents::Lax));
        assert_eq!("skip".parse(), Ok(ProcessContents::Skip));
        assert_eq!("invalid".parse::<ProcessContents>(), Err(()));
    }

    #[test]
    fn test_element_wildcard_default() {
        let wildcard = ElementWildcard::new();
        assert_eq!(wildcard.process_contents, ProcessContents::Strict);
        assert_eq!(wildcard.min_occurs, 1);
        assert_eq!(wildcard.max_occurs, Some(1));
    }

    #[test]
    fn test_element_wildcard_any_lax() {
        let wildcard = ElementWildcard::any_lax();
        assert!(wildcard.is_optional());
        assert!(wildcard.is_unbounded());
        assert_eq!(wildcard.process_contents, ProcessContents::Lax);
    }

    #[test]
    fn test_attribute_wildcard_default() {
        let wildcard = AttributeWildcard::new();
        assert_eq!(wildcard.process_contents, ProcessContents::Strict);
    }
}
