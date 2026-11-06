//! Binding remap table for mapping 20-bit node indices to full [`NodeSchemaBinding`] values.
//!
//! Each element/attribute node in the [`BufferDocument`](super) carries a 20-bit
//! `binding_index` field.  The [`BindingRemapTable`] maps those compact indices back
//! to [`NodeSchemaBinding`] records containing the type key and optional declaration keys.

use crate::ids::{AttributeKey, ElementKey, SimpleTypeKey, TypeKey};
use crate::validation::info::ContentType;

use super::error::BufferDocumentError;

/// Full schema binding for a single document node.
///
/// Two bindings are considered equal (for deduplication) when **all** fields match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeSchemaBinding {
    pub type_key: TypeKey,
    pub element_decl: Option<ElementKey>,
    pub attribute_decl: Option<AttributeKey>,
    pub content_type: Option<ContentType>,
}

/// Maps compact 20-bit indices ↔ full [`NodeSchemaBinding`] values.
///
/// Index 0 is an unbound sentinel (returned as `None` by [`get`](Self::get))
/// and is never stored in the entries list.
pub struct BindingRemapTable {
    /// Indexed entries (index 0 = unbound sentinel).
    entries: Vec<NodeSchemaBinding>,
}

/// Maximum entry count: 2^20 − 1 (index 0 is the sentinel).
const MAX_ENTRIES: u32 = (1 << 20) - 1;

impl BindingRemapTable {
    /// Creates a new table with an unbound sentinel at index 0.
    pub fn new() -> Self {
        use slotmap::Key;

        // Dummy binding at index 0 — never returned by `get`.
        let sentinel = NodeSchemaBinding {
            type_key: TypeKey::Simple(SimpleTypeKey::null()),
            element_decl: None,
            attribute_decl: None,
            content_type: None,
        };

        Self {
            entries: vec![sentinel],
        }
    }

    /// Registers a [`NodeSchemaBinding`] and returns its 20-bit index.
    ///
    /// If an identical binding was already registered, returns the existing index.
    /// Uses linear scan for deduplication (expected table size is small).
    ///
    /// Returns [`BufferDocumentError::Overflow`] if the table would exceed 2^20 − 1 entries.
    pub fn register(&mut self, binding: NodeSchemaBinding) -> Result<u32, BufferDocumentError> {
        // Linear scan dedup (skip sentinel at 0).
        for (i, entry) in self.entries.iter().enumerate().skip(1) {
            if *entry == binding {
                return Ok(i as u32);
            }
        }
        let idx = self.entries.len() as u32;
        if idx > MAX_ENTRIES {
            return Err(BufferDocumentError::Overflow);
        }
        self.entries.push(binding);
        Ok(idx)
    }

    /// Returns the [`NodeSchemaBinding`] for the given index, or `None` for index 0.
    pub fn get(&self, idx: u32) -> Option<&NodeSchemaBinding> {
        if idx == 0 {
            return None;
        }
        self.entries.get(idx as usize)
    }
}

impl Default for BindingRemapTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{ComplexTypeKey, SimpleTypeKey, TypeKey};
    use slotmap::SlotMap;

    /// Helper: create a real `SimpleTypeKey` via slotmap.
    fn make_simple_key() -> SimpleTypeKey {
        let mut sm = SlotMap::with_key();
        sm.insert(())
    }

    /// Helper: create a real `ComplexTypeKey` via slotmap.
    fn make_complex_key() -> ComplexTypeKey {
        let mut sm = SlotMap::with_key();
        sm.insert(())
    }

    /// Helper: create a binding with just a type key.
    fn binding_from_type(tk: TypeKey) -> NodeSchemaBinding {
        NodeSchemaBinding {
            type_key: tk,
            element_decl: None,
            attribute_decl: None,
            content_type: None,
        }
    }

    #[test]
    fn index_zero_returns_none() {
        let table = BindingRemapTable::new();
        assert!(table.get(0).is_none());
    }

    #[test]
    fn register_and_get_simple() {
        let mut table = BindingRemapTable::new();
        let sk = make_simple_key();
        let binding = binding_from_type(TypeKey::Simple(sk));

        let idx = table.register(binding).unwrap();
        assert_eq!(idx, 1);
        assert_eq!(table.get(idx), Some(&binding));
    }

    #[test]
    fn register_and_get_complex() {
        let mut table = BindingRemapTable::new();
        let ck = make_complex_key();
        let binding = binding_from_type(TypeKey::Complex(ck));

        let idx = table.register(binding).unwrap();
        assert_eq!(idx, 1);
        assert_eq!(table.get(idx), Some(&binding));
    }

    #[test]
    fn register_same_binding_twice_returns_same_index() {
        let mut table = BindingRemapTable::new();
        let sk = make_simple_key();
        let binding = binding_from_type(TypeKey::Simple(sk));

        let idx1 = table.register(binding).unwrap();
        let idx2 = table.register(binding).unwrap();
        assert_eq!(idx1, idx2);
    }

    #[test]
    fn simple_vs_complex_preserved() {
        let mut table = BindingRemapTable::new();
        let sk = make_simple_key();
        let ck = make_complex_key();
        let b_s = binding_from_type(TypeKey::Simple(sk));
        let b_c = binding_from_type(TypeKey::Complex(ck));

        let idx_s = table.register(b_s).unwrap();
        let idx_c = table.register(b_c).unwrap();
        assert_ne!(idx_s, idx_c);

        match table.get(idx_s).unwrap().type_key {
            TypeKey::Simple(_) => {}
            other => panic!("Expected Simple, got {other:?}"),
        }
        match table.get(idx_c).unwrap().type_key {
            TypeKey::Complex(_) => {}
            other => panic!("Expected Complex, got {other:?}"),
        }
    }

    #[test]
    fn multiple_distinct_bindings_distinct_indices() {
        let mut table = BindingRemapTable::new();

        let mut sm: SlotMap<SimpleTypeKey, ()> = SlotMap::with_key();
        let mut indices = Vec::new();
        for _ in 0..10 {
            let sk = sm.insert(());
            indices.push(
                table
                    .register(binding_from_type(TypeKey::Simple(sk)))
                    .unwrap(),
            );
        }

        let unique: std::collections::HashSet<_> = indices.iter().collect();
        assert_eq!(unique.len(), indices.len());
    }

    #[test]
    fn get_out_of_range_returns_none() {
        let table = BindingRemapTable::new();
        assert!(table.get(999).is_none());
    }

    #[test]
    fn same_type_different_decl_are_distinct() {
        let mut table = BindingRemapTable::new();
        let sk = make_simple_key();
        let tk = TypeKey::Simple(sk);

        let mut sm_elem: SlotMap<ElementKey, ()> = SlotMap::with_key();
        let ek1 = sm_elem.insert(());
        let ek2 = sm_elem.insert(());

        let b1 = NodeSchemaBinding {
            type_key: tk,
            element_decl: Some(ek1),
            attribute_decl: None,
            content_type: None,
        };
        let b2 = NodeSchemaBinding {
            type_key: tk,
            element_decl: Some(ek2),
            attribute_decl: None,
            content_type: None,
        };

        let idx1 = table.register(b1).unwrap();
        let idx2 = table.register(b2).unwrap();
        assert_ne!(
            idx1, idx2,
            "same type with different element_decl must get distinct indices"
        );
        assert_eq!(table.get(idx1), Some(&b1));
        assert_eq!(table.get(idx2), Some(&b2));
    }
}
