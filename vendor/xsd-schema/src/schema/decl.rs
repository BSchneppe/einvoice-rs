//! Element and attribute declarations
//!
//! This module defines XSD element and attribute declarations.

use crate::ids::{ElementKey, IdentityConstraintKey, NameId, SimpleTypeKey, TypeKey};
use crate::parser::location::SourceRef;
use crate::schema::model::DerivationSet;

/// Scope of a declaration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclarationScope {
    /// Global declaration (top-level in schema)
    Global,
    /// Local declaration (within complex type)
    Local,
}

/// Value constraint kind
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueConstraint {
    /// Default value (can be overridden)
    Default(String),
    /// Fixed value (cannot be changed)
    Fixed(String),
}

impl ValueConstraint {
    /// Get the value
    pub fn value(&self) -> &str {
        match self {
            ValueConstraint::Default(v) => v,
            ValueConstraint::Fixed(v) => v,
        }
    }

    /// Check if this is a fixed constraint
    pub fn is_fixed(&self) -> bool {
        matches!(self, ValueConstraint::Fixed(_))
    }
}

/// Type reference (can be resolved or unresolved)
#[derive(Debug, Clone)]
pub enum TypeReference {
    /// Resolved type key
    Resolved(TypeKey),
    /// Unresolved reference (namespace + local name)
    Unresolved {
        namespace: Option<NameId>,
        local_name: NameId,
    },
}

/// Element declaration
///
/// Represents an xs:element declaration, either global or local.
#[derive(Debug, Clone)]
pub struct ElementDecl {
    /// Element name
    pub name: NameId,

    /// Target namespace
    pub target_namespace: Option<NameId>,

    /// Source location for error reporting
    pub source: Option<SourceRef>,

    /// Declaration scope (global or local)
    pub scope: DeclarationScope,

    /// Type definition (resolved or reference)
    pub type_def: Option<TypeReference>,

    /// Value constraint (default or fixed)
    pub value_constraint: Option<ValueConstraint>,

    /// Nillable flag (allows xsi:nil="true")
    pub nillable: bool,

    /// Abstract flag (must be substituted)
    pub is_abstract: bool,

    /// Substitution group affiliation
    pub substitution_group: Option<ElementRef>,

    /// Disallowed substitutions (block attribute)
    pub disallowed_substitutions: DerivationSet,

    /// Substitution group exclusions (final attribute)
    pub substitution_group_exclusions: DerivationSet,

    /// Identity constraints defined on this element
    pub identity_constraints: Vec<IdentityConstraintKey>,

    /// ID attribute value
    pub id: Option<String>,

    /// XSD 1.1: Type alternatives (conditional type assignment)
    pub type_alternatives: Vec<TypeAlternative>,

    /// Form (qualified/unqualified) - for local elements
    pub form: Option<FormKind>,
}

/// Reference to an element (for substitution groups)
#[derive(Debug, Clone)]
pub enum ElementRef {
    /// Resolved element key
    Resolved(ElementKey),
    /// Unresolved reference
    Unresolved {
        namespace: Option<NameId>,
        local_name: NameId,
    },
}

/// XSD 1.1: Type alternative
#[derive(Debug, Clone)]
pub struct TypeAlternative {
    /// XPath test expression
    pub test: Option<String>,
    /// Type to use when test passes
    pub type_def: TypeReference,
    /// Source location
    pub source: Option<SourceRef>,
}

/// Form kind (qualified/unqualified)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormKind {
    Qualified,
    Unqualified,
}

impl ElementDecl {
    /// Create a new global element declaration
    pub fn new_global(name: NameId, target_namespace: Option<NameId>) -> Self {
        Self {
            name,
            target_namespace,
            source: None,
            scope: DeclarationScope::Global,
            type_def: None,
            value_constraint: None,
            nillable: false,
            is_abstract: false,
            substitution_group: None,
            disallowed_substitutions: DerivationSet::empty(),
            substitution_group_exclusions: DerivationSet::empty(),
            identity_constraints: Vec::new(),
            id: None,
            type_alternatives: Vec::new(),
            form: None,
        }
    }

    /// Create a new local element declaration
    pub fn new_local(name: NameId, target_namespace: Option<NameId>) -> Self {
        Self {
            name,
            target_namespace,
            source: None,
            scope: DeclarationScope::Local,
            type_def: None,
            value_constraint: None,
            nillable: false,
            is_abstract: false,
            substitution_group: None,
            disallowed_substitutions: DerivationSet::empty(),
            substitution_group_exclusions: DerivationSet::empty(),
            identity_constraints: Vec::new(),
            id: None,
            type_alternatives: Vec::new(),
            form: None,
        }
    }

    /// Check if this is a global element
    pub fn is_global(&self) -> bool {
        self.scope == DeclarationScope::Global
    }

    /// Check if this is a local element
    pub fn is_local(&self) -> bool {
        self.scope == DeclarationScope::Local
    }

    /// Check if this element can be substituted
    pub fn is_substitutable(&self) -> bool {
        !self.is_abstract && self.substitution_group_exclusions.is_empty()
    }
}

/// Attribute declaration
///
/// Represents an xs:attribute declaration, either global or local.
#[derive(Debug, Clone)]
pub struct AttributeDecl {
    /// Attribute name
    pub name: NameId,

    /// Target namespace
    pub target_namespace: Option<NameId>,

    /// Source location for error reporting
    pub source: Option<SourceRef>,

    /// Declaration scope (global or local)
    pub scope: DeclarationScope,

    /// Simple type definition (attributes always have simple types)
    pub type_def: Option<SimpleTypeReference>,

    /// Value constraint (default or fixed)
    pub value_constraint: Option<ValueConstraint>,

    /// ID attribute value
    pub id: Option<String>,

    /// Form (qualified/unqualified) - for local attributes
    pub form: Option<FormKind>,

    /// XSD 1.1: Inheritable attribute (§3.2.6, §3.3.5.6)
    pub inheritable: bool,
}

/// Reference to a simple type
#[derive(Debug, Clone)]
pub enum SimpleTypeReference {
    /// Resolved simple type key
    Resolved(SimpleTypeKey),
    /// Built-in type
    BuiltIn(crate::types::simple::BuiltInType),
    /// Unresolved reference
    Unresolved {
        namespace: Option<NameId>,
        local_name: NameId,
    },
}

impl AttributeDecl {
    /// Create a new global attribute declaration
    pub fn new_global(name: NameId, target_namespace: Option<NameId>) -> Self {
        Self {
            name,
            target_namespace,
            source: None,
            scope: DeclarationScope::Global,
            type_def: None,
            value_constraint: None,
            id: None,
            form: None,
            inheritable: false,
        }
    }

    /// Create a new local attribute declaration
    pub fn new_local(name: NameId, target_namespace: Option<NameId>) -> Self {
        Self {
            name,
            target_namespace,
            source: None,
            scope: DeclarationScope::Local,
            type_def: None,
            value_constraint: None,
            id: None,
            form: None,
            inheritable: false,
        }
    }

    /// Check if this is a global attribute
    pub fn is_global(&self) -> bool {
        self.scope == DeclarationScope::Global
    }

    /// Check if this is a local attribute
    pub fn is_local(&self) -> bool {
        self.scope == DeclarationScope::Local
    }

    /// Check if this attribute has a default value
    pub fn has_default(&self) -> bool {
        matches!(self.value_constraint, Some(ValueConstraint::Default(_)))
    }

    /// Check if this attribute has a fixed value
    pub fn has_fixed(&self) -> bool {
        matches!(self.value_constraint, Some(ValueConstraint::Fixed(_)))
    }
}

/// Notation declaration
///
/// Represents an xs:notation declaration.
#[derive(Debug, Clone)]
pub struct NotationDecl {
    /// Notation name
    pub name: NameId,

    /// Target namespace
    pub target_namespace: Option<NameId>,

    /// Public identifier
    pub public: String,

    /// System identifier (optional)
    pub system: Option<String>,

    /// Source location
    pub source: Option<SourceRef>,

    /// ID attribute value
    pub id: Option<String>,
}

impl NotationDecl {
    /// Create a new notation declaration
    pub fn new(name: NameId, public: String) -> Self {
        Self {
            name,
            target_namespace: None,
            public,
            system: None,
            source: None,
            id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_element_decl_global() {
        let elem = ElementDecl::new_global(NameId(1), Some(NameId(2)));
        assert!(elem.is_global());
        assert!(!elem.is_local());
        assert_eq!(elem.scope, DeclarationScope::Global);
    }

    #[test]
    fn test_element_decl_local() {
        let elem = ElementDecl::new_local(NameId(1), None);
        assert!(elem.is_local());
        assert!(!elem.is_global());
        assert_eq!(elem.scope, DeclarationScope::Local);
    }

    #[test]
    fn test_element_substitutable() {
        let mut elem = ElementDecl::new_global(NameId(1), None);
        assert!(elem.is_substitutable());

        elem.is_abstract = true;
        assert!(!elem.is_substitutable());
    }

    #[test]
    fn test_attribute_decl_global() {
        let attr = AttributeDecl::new_global(NameId(1), Some(NameId(2)));
        assert!(attr.is_global());
        assert!(!attr.is_local());
    }

    #[test]
    fn test_attribute_decl_local() {
        let attr = AttributeDecl::new_local(NameId(1), None);
        assert!(attr.is_local());
        assert!(!attr.is_global());
    }

    #[test]
    fn test_value_constraint() {
        let default_val = ValueConstraint::Default("foo".to_string());
        assert!(!default_val.is_fixed());
        assert_eq!(default_val.value(), "foo");

        let fixed_val = ValueConstraint::Fixed("bar".to_string());
        assert!(fixed_val.is_fixed());
        assert_eq!(fixed_val.value(), "bar");
    }

    #[test]
    fn test_attribute_value_constraint() {
        let mut attr = AttributeDecl::new_local(NameId(1), None);
        assert!(!attr.has_default());
        assert!(!attr.has_fixed());

        attr.value_constraint = Some(ValueConstraint::Default("test".to_string()));
        assert!(attr.has_default());
        assert!(!attr.has_fixed());

        attr.value_constraint = Some(ValueConstraint::Fixed("test".to_string()));
        assert!(!attr.has_default());
        assert!(attr.has_fixed());
    }

    #[test]
    fn test_notation_decl() {
        let notation = NotationDecl::new(NameId(1), "http://example.com/public".to_string());
        assert_eq!(notation.name, NameId(1));
        assert_eq!(notation.public, "http://example.com/public");
        assert!(notation.system.is_none());
    }
}
