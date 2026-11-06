//! Element index by local-name hash for the document buffer.
//!
//! [`ElementIndex`] maps [`QNameAtom::local_name_hash`](super::qname::QNameAtom::local_name_hash)
//! values to lists of node references, giving O(1) lookup by element name.
//! Callers **must** filter results by full QName because hash collisions are expected.

use ahash::RandomState;
use std::collections::HashMap;

/// Index from `local_name_hash` → list of node references.
///
/// Used by the navigator to quickly locate elements by name without a
/// full document scan.  Because different QNames may share the same
/// `local_name_hash`, callers must perform a secondary equality check
/// against the full [`QNameAtom`](super::qname::QNameAtom).
///
/// The key is itself a precomputed `local_name_hash`, so the map uses `ahash`
/// (fast, keyed) rather than SipHash.
#[derive(Debug, Default)]
pub struct ElementIndex {
    map: HashMap<u32, Vec<u32>, RandomState>,
}

impl ElementIndex {
    /// Creates a new empty index.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records `node_ref` under the given `local_name_hash`.
    pub fn add(&mut self, local_name_hash: u32, node_ref: u32) {
        self.map.entry(local_name_hash).or_default().push(node_ref);
    }

    /// Returns all node refs that share `local_name_hash`, or an empty slice.
    pub fn find(&self, local_name_hash: u32) -> &[u32] {
        self.map.get(&local_name_hash).map_or(&[], |v| v.as_slice())
    }

    /// Returns the total number of hash buckets that have at least one entry.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Returns `true` if the index contains no entries.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_index_is_empty() {
        let idx = ElementIndex::new();
        assert!(idx.is_empty());
        assert_eq!(idx.len(), 0);
    }

    #[test]
    fn add_and_find_single() {
        let mut idx = ElementIndex::new();
        idx.add(42, 7);
        assert_eq!(idx.find(42), &[7]);
    }

    #[test]
    fn unknown_hash_returns_empty() {
        let idx = ElementIndex::new();
        assert!(idx.find(999).is_empty());
    }

    #[test]
    fn multiple_elements_same_hash() {
        let mut idx = ElementIndex::new();
        idx.add(42, 1);
        idx.add(42, 5);
        idx.add(42, 9);
        assert_eq!(idx.find(42), &[1, 5, 9]);
    }

    #[test]
    fn different_hashes_independent() {
        let mut idx = ElementIndex::new();
        idx.add(10, 1);
        idx.add(20, 2);
        assert_eq!(idx.find(10), &[1]);
        assert_eq!(idx.find(20), &[2]);
    }

    #[test]
    fn len_counts_buckets() {
        let mut idx = ElementIndex::new();
        idx.add(10, 1);
        idx.add(20, 2);
        idx.add(10, 3); // same bucket as 10
        assert_eq!(idx.len(), 2); // two distinct hash keys
    }
}
