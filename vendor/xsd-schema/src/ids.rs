//! Typed IDs for arena-based storage
//!
//! All schema components are stored in arenas and referenced by typed IDs.
//! This approach avoids reference cycles and provides type safety.
//!
//! Uses slotmap for type-safe arena keys with generation tracking.

use slotmap::new_key_type;
use std::fmt;

/// Document ID for source map indexing
pub type DocumentId = u32;

/// Interned string identifier for names and namespace URIs
///
/// See XML_NAME_TABLE.md for NameTable design.
/// NameId(0) is reserved for empty string.
///
/// Note: NameId is NOT a slotmap key - it's a simple index into the NameTable
/// which uses a custom chained hash table for string interning.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct NameId(pub u32);

impl fmt::Debug for NameId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NameId({})", self.0)
    }
}

impl fmt::Display for NameId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "${}", self.0)
    }
}

// Define typed keys for slotmap arenas
// Each key type is unique and cannot be used with other SlotMaps

new_key_type! {
    /// Simple type definition key
    pub struct SimpleTypeKey;

    /// Complex type definition key
    pub struct ComplexTypeKey;

    /// Element declaration key
    pub struct ElementKey;

    /// Attribute declaration key
    pub struct AttributeKey;

    /// Attribute group key
    pub struct AttributeGroupKey;

    /// Model group key (named groups like <xs:group name="...">)
    pub struct ModelGroupKey;

    /// Notation declaration key
    pub struct NotationKey;

    /// Identity constraint key (key, unique, keyref)
    pub struct IdentityConstraintKey;
}

/// Type definition reference (simple or complex)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeKey {
    Simple(SimpleTypeKey),
    Complex(ComplexTypeKey),
}

impl TypeKey {
    pub fn as_simple(&self) -> Option<SimpleTypeKey> {
        match self {
            TypeKey::Simple(key) => Some(*key),
            _ => None,
        }
    }

    pub fn as_complex(&self) -> Option<ComplexTypeKey> {
        match self {
            TypeKey::Complex(key) => Some(*key),
            _ => None,
        }
    }
}

impl From<SimpleTypeKey> for TypeKey {
    fn from(key: SimpleTypeKey) -> Self {
        TypeKey::Simple(key)
    }
}

impl From<ComplexTypeKey> for TypeKey {
    fn from(key: ComplexTypeKey) -> Self {
        TypeKey::Complex(key)
    }
}
