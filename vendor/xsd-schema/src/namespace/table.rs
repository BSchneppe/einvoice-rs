//! String interning via NameTable
//!
//! The NameTable provides O(1) string interning using a chained hash table.
//! All names and namespace URIs in the schema model go through this table
//! to ensure deduplication and fast equality checks via NameId.
//!
//! Design per XML_NAME_TABLE.md:
//! - Entry: `{hash, next, text: Box<str>}`
//! - NameId(0) reserved for empty string
//! - Rehashing when entries.len() > buckets.len()
//! - Pre-seed with standard namespaces
//!
//! ## Interior Mutability
//!
//! NameTable uses interior mutability (`RefCell`) to allow adding new strings
//! through a shared reference (`&self`). This enables runtime string interning
//! in contexts where only immutable access to the table is available (e.g.,
//! during XPath function evaluation).

use crate::ids::NameId;
use ahash::RandomState;
use std::cell::{Cell, RefCell};

/// Entry in the name table
///
/// # Invariants (for unsafe `resolve_ref` / `try_resolve_ref`)
///
/// 1. `text` is immutable after insertion — no API replaces or removes it.
/// 2. No future API should remove, compact, or overwrite entries.
///
/// Note: `hash` and `next` *are* mutated during rehashing; only `text` stability matters.
#[derive(Debug)]
struct Entry {
    /// Cached hash value
    hash: u64,
    /// Next entry in chain (-1 = none); mutated during rehashing
    next: i32,
    /// The interned string (immutable after insertion — never replaced or removed)
    text: Box<str>,
}

/// String interning table for names and namespace URIs
///
/// Provides O(1) average-case lookup and insertion.
/// All strings in the schema model should be interned through this table.
///
/// Uses interior mutability to allow adding strings through shared references.
///
/// # Example
///
/// ```
/// use xsd_schema::namespace::NameTable;
///
/// let table = NameTable::new();
/// let id1 = table.add("hello");
/// let id2 = table.add("hello");
/// assert_eq!(id1, id2); // Same string -> same ID
/// assert_eq!(table.resolve(id1), "hello");
/// ```
#[derive(Debug)]
pub struct NameTable {
    /// All entries (indexed by NameId) - uses RefCell for interior mutability
    entries: RefCell<Vec<Entry>>,
    /// Hash buckets (index into entries via first entry in chain)
    buckets: RefCell<Vec<i32>>,
    /// Per-table keyed hasher (ahash). Fast and DoS-resistant: the random seed
    /// is fixed for this table's lifetime (so cached `Entry.hash` stays valid)
    /// but unpredictable across tables, defeating hash-collision attacks on
    /// names interned from untrusted input. `NameId` is assigned by insertion
    /// order, independent of the hash, so interning results stay deterministic.
    hasher: RandomState,
    /// Bytes of interned text, for [`heap_bytes`](Self::heap_bytes).
    text_bytes: Cell<usize>,
}

impl NameTable {
    /// Initial bucket count
    const INITIAL_BUCKETS: usize = 256;

    /// Create a new name table pre-seeded with standard namespaces
    pub fn new() -> Self {
        let table = Self {
            entries: RefCell::new(Vec::with_capacity(Self::INITIAL_BUCKETS)),
            buckets: RefCell::new(vec![-1; Self::INITIAL_BUCKETS]),
            hasher: RandomState::new(),
            text_bytes: Cell::new(0),
        };

        // Pre-seed standard values
        // NameId(0) = empty string (reserved)
        table.add("");

        // Standard namespace URIs
        table.add(XS_NAMESPACE);
        table.add(XSI_NAMESPACE);
        table.add(XML_NAMESPACE);
        table.add(XMLNS_NAMESPACE);

        // Standard prefixes
        table.add("xs");
        table.add("xsd");
        table.add("xsi");
        table.add("xml");
        table.add("xmlns");

        // XSI attribute local names
        table.add("type");
        table.add("nil");
        table.add("schemaLocation");
        table.add("noNamespaceSchemaLocation");

        table
    }

    /// Add a string to the table, returning its NameId
    ///
    /// If the string already exists, returns the existing NameId.
    /// Otherwise, creates a new entry and returns a new NameId.
    ///
    /// Uses interior mutability, so this can be called through a shared reference.
    pub fn add(&self, value: &str) -> NameId {
        let hash = self.hash_str(value);

        // Check if already present (read-only borrow)
        if let Some(id) = self.find(value, hash) {
            return id;
        }

        // Need to insert (mutable borrow)
        self.insert(value, hash)
    }

    /// Get the NameId for a string if it exists
    pub fn get(&self, value: &str) -> Option<NameId> {
        let hash = self.hash_str(value);
        self.find(value, hash)
    }

    /// Resolve a NameId to a string slice (zero-allocation).
    ///
    /// Prefer this over `resolve()` in performance-critical paths
    /// (e.g., DomNavigator implementations).
    ///
    /// # Safety (internal)
    ///
    /// Uses `RefCell::as_ptr()` to bypass the borrow guard. This is sound because:
    /// - `Entry.text` is `Box<str>` — heap-stable allocation
    /// - `Entry.text` is never replaced or removed after insertion
    ///   (other Entry fields like `next` may be mutated during rehashing,
    ///   but only `text` stability matters here)
    /// - The returned `&str` points to the Box's heap data, not the Vec buffer,
    ///   so it remains valid even if the Vec reallocates during a later `add()`
    ///
    /// # Panics
    ///
    /// Panics if the NameId is out of bounds.
    pub fn resolve_ref(&self, id: NameId) -> &str {
        unsafe { &(&(*self.entries.as_ptr()))[id.0 as usize].text }
    }

    /// Try to resolve a NameId to a string slice (zero-allocation).
    ///
    /// Returns `None` if the NameId is out of bounds.
    pub fn try_resolve_ref(&self, id: NameId) -> Option<&str> {
        unsafe {
            (&(*self.entries.as_ptr()))
                .get(id.0 as usize)
                .map(|e| e.text.as_ref())
        }
    }

    /// Resolve a NameId to its string value
    ///
    /// Returns a copy of the string. For zero-allocation access, use `resolve_ref()`.
    ///
    /// # Panics
    ///
    /// Panics if the NameId is invalid (out of bounds).
    pub fn resolve(&self, id: NameId) -> String {
        self.resolve_ref(id).to_string()
    }

    /// Try to resolve a NameId to its string value
    ///
    /// Returns a copy of the string. For zero-allocation access, use `try_resolve_ref()`.
    pub fn try_resolve(&self, id: NameId) -> Option<String> {
        self.try_resolve_ref(id).map(|s| s.to_string())
    }

    /// Get the number of interned strings
    pub fn len(&self) -> usize {
        self.entries.borrow().len()
    }

    /// The heap memory the table holds, in bytes, in constant time: the
    /// interned text, the entry and bucket arrays (their capacity), and one
    /// heap block per entry's text. An estimate of what the table costs —
    /// the allocator adds its own rounding — so that a host that validates
    /// documents against a long-lived schema set can watch how much the
    /// names of those documents grow it (every distinct name is interned for
    /// good) and rebuild the set past a budget.
    ///
    /// ```
    /// use xsd_schema::namespace::NameTable;
    ///
    /// let table = NameTable::new();
    /// let before = table.heap_bytes();
    /// table.add(&"x".repeat(1000));
    /// assert!(table.heap_bytes() >= before + 1000);
    /// ```
    pub fn heap_bytes(&self) -> usize {
        let entries = self.entries.borrow().capacity();
        let buckets = self.buckets.borrow().capacity();
        self.text_bytes.get()
            + entries * std::mem::size_of::<Entry>()
            + buckets * std::mem::size_of::<i32>()
    }

    /// Check if the table is empty
    pub fn is_empty(&self) -> bool {
        self.entries.borrow().is_empty()
    }

    /// Hash a string with this table's keyed `ahash` hasher.
    ///
    /// `ahash` is much faster than SipHash but, unlike `FxHash`, keeps
    /// hash-collision DoS resistance via a random per-table seed — important
    /// because names are interned from untrusted XML/schema input. The seed is
    /// fixed for the table's lifetime so cached `Entry.hash` values stay valid.
    fn hash_str(&self, value: &str) -> u64 {
        self.hasher.hash_one(value)
    }

    /// Find an existing entry
    fn find(&self, value: &str, hash: u64) -> Option<NameId> {
        let entries = self.entries.borrow();
        let buckets = self.buckets.borrow();

        let bucket_idx = (hash as usize) % buckets.len();
        let mut entry_idx = buckets[bucket_idx];

        while entry_idx >= 0 {
            let entry = &entries[entry_idx as usize];
            if entry.hash == hash && entry.text.as_ref() == value {
                return Some(NameId(entry_idx as u32));
            }
            entry_idx = entry.next;
        }

        None
    }

    /// Insert a new entry
    fn insert(&self, value: &str, hash: u64) -> NameId {
        let mut entries = self.entries.borrow_mut();
        let mut buckets = self.buckets.borrow_mut();

        // Check if we need to rehash
        if entries.len() >= buckets.len() {
            Self::rehash_internal(&mut entries, &mut buckets);
        }

        let id = NameId(entries.len() as u32);
        let bucket_idx = (hash as usize) % buckets.len();

        // Insert at head of chain
        let next = buckets[bucket_idx];
        entries.push(Entry {
            hash,
            next,
            text: value.into(),
        });
        buckets[bucket_idx] = id.0 as i32;
        self.text_bytes.set(self.text_bytes.get() + value.len());

        id
    }

    /// Rehash the table (double bucket count) - internal version
    #[allow(clippy::ptr_arg)]
    fn rehash_internal(entries: &mut Vec<Entry>, buckets: &mut Vec<i32>) {
        let new_size = buckets.len() * 2;
        *buckets = vec![-1; new_size];

        // Re-insert all entries into new buckets
        for (idx, entry) in entries.iter_mut().enumerate() {
            let bucket_idx = (entry.hash as usize) % new_size;
            entry.next = buckets[bucket_idx];
            buckets[bucket_idx] = idx as i32;
        }
    }
}

impl Default for NameTable {
    fn default() -> Self {
        Self::new()
    }
}

// Well-known namespace URIs
pub const XS_NAMESPACE: &str = "http://www.w3.org/2001/XMLSchema";
pub const XSI_NAMESPACE: &str = "http://www.w3.org/2001/XMLSchema-instance";
pub const XML_NAMESPACE: &str = "http://www.w3.org/XML/1998/namespace";
pub const XMLNS_NAMESPACE: &str = "http://www.w3.org/2000/xmlns/";

/// Well-known NameIds (pre-seeded in NameTable::new())
pub mod well_known {
    use crate::ids::NameId;

    /// Empty string
    pub const EMPTY: NameId = NameId(0);

    /// XSD namespace URI
    pub const XS_NAMESPACE: NameId = NameId(1);

    /// XSD instance namespace URI
    pub const XSI_NAMESPACE: NameId = NameId(2);

    /// XML namespace URI
    pub const XML_NAMESPACE: NameId = NameId(3);

    /// XMLNS namespace URI
    pub const XMLNS_NAMESPACE: NameId = NameId(4);

    /// "xs" prefix
    pub const XS_PREFIX: NameId = NameId(5);

    /// "xsd" prefix
    pub const XSD_PREFIX: NameId = NameId(6);

    /// "xsi" prefix
    pub const XSI_PREFIX: NameId = NameId(7);

    /// "xml" prefix
    pub const XML_PREFIX: NameId = NameId(8);

    /// "xmlns" prefix
    pub const XMLNS_PREFIX: NameId = NameId(9);

    /// XSI local name "type"
    pub const XSI_TYPE: NameId = NameId(10);

    /// XSI local name "nil"
    pub const XSI_NIL: NameId = NameId(11);

    /// XSI local name "schemaLocation"
    pub const XSI_SCHEMA_LOCATION: NameId = NameId(12);

    /// XSI local name "noNamespaceSchemaLocation"
    pub const XSI_NO_NAMESPACE_SCHEMA_LOCATION: NameId = NameId(13);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_string_is_id_zero() {
        let table = NameTable::new();
        assert_eq!(table.resolve(NameId(0)), "");
    }

    #[test]
    fn test_add_and_resolve() {
        let table = NameTable::new();
        let id = table.add("hello");
        assert_eq!(table.resolve(id), "hello");
    }

    #[test]
    fn test_deduplication() {
        let table = NameTable::new();
        let id1 = table.add("hello");
        let id2 = table.add("hello");
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_different_strings_different_ids() {
        let table = NameTable::new();
        let id1 = table.add("hello");
        let id2 = table.add("world");
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_get_existing() {
        let table = NameTable::new();
        let id = table.add("test");
        assert_eq!(table.get("test"), Some(id));
    }

    #[test]
    fn test_get_nonexistent() {
        let table = NameTable::new();
        assert_eq!(table.get("nonexistent"), None);
    }

    #[test]
    fn test_well_known_namespaces() {
        let table = NameTable::new();
        assert_eq!(table.resolve(well_known::XS_NAMESPACE), XS_NAMESPACE);
        assert_eq!(table.resolve(well_known::XSI_NAMESPACE), XSI_NAMESPACE);
        assert_eq!(table.resolve(well_known::XML_NAMESPACE), XML_NAMESPACE);
        assert_eq!(table.resolve(well_known::XMLNS_NAMESPACE), XMLNS_NAMESPACE);
    }

    #[test]
    fn test_well_known_prefixes() {
        let table = NameTable::new();
        assert_eq!(table.resolve(well_known::XS_PREFIX), "xs");
        assert_eq!(table.resolve(well_known::XSD_PREFIX), "xsd");
        assert_eq!(table.resolve(well_known::XSI_PREFIX), "xsi");
        assert_eq!(table.resolve(well_known::XML_PREFIX), "xml");
    }

    #[test]
    fn test_well_known_xsi_local_names() {
        let table = NameTable::new();
        assert_eq!(table.resolve(well_known::XSI_TYPE), "type");
        assert_eq!(table.resolve(well_known::XSI_NIL), "nil");
        assert_eq!(
            table.resolve(well_known::XSI_SCHEMA_LOCATION),
            "schemaLocation"
        );
        assert_eq!(
            table.resolve(well_known::XSI_NO_NAMESPACE_SCHEMA_LOCATION),
            "noNamespaceSchemaLocation"
        );
    }

    #[test]
    fn test_rehashing() {
        let table = NameTable::new();
        // Insert enough entries to trigger rehashing
        for i in 0..1000 {
            let s = format!("string_{}", i);
            table.add(&s);
        }
        // Verify all strings can still be found
        for i in 0..1000 {
            let s = format!("string_{}", i);
            assert!(table.get(&s).is_some(), "Failed to find: {}", s);
        }
    }

    #[test]
    fn test_unicode_strings() {
        let table = NameTable::new();
        let id1 = table.add("日本語");
        let id2 = table.add("日本語");
        assert_eq!(id1, id2);
        assert_eq!(table.resolve(id1), "日本語");
    }

    #[test]
    fn test_interior_mutability() {
        // Test that add works through shared reference
        let table = NameTable::new();
        let table_ref = &table;
        let id1 = table_ref.add("through_ref");
        let id2 = table_ref.add("through_ref");
        assert_eq!(id1, id2);
        assert_eq!(table.resolve(id1), "through_ref");
    }

    #[test]
    fn test_resolve_ref_basic() {
        let table = NameTable::new();
        let id = table.add("hello");
        assert_eq!(table.resolve_ref(id), "hello");
        assert_eq!(table.resolve_ref(NameId(0)), "");
    }

    #[test]
    fn test_try_resolve_ref_basic() {
        let table = NameTable::new();
        let id = table.add("test");
        assert_eq!(table.try_resolve_ref(id), Some("test"));
        assert_eq!(table.try_resolve_ref(NameId(9999)), None);
    }

    #[test]
    fn test_resolve_ref_stability_across_add() {
        let table = NameTable::new();
        let id = table.add("stable_string");
        let resolved: &str = table.resolve_ref(id);
        // Trigger multiple Vec reallocations
        for i in 0..500 {
            table.add(&format!("extra_{i}"));
        }
        assert_eq!(resolved, "stable_string");
    }

    #[test]
    fn test_try_resolve_ref_stability_across_add() {
        let table = NameTable::new();
        let id = table.add("test_str");
        let resolved: Option<&str> = table.try_resolve_ref(id);
        // Trigger multiple Vec reallocations
        for i in 0..500 {
            table.add(&format!("filler_{i}"));
        }
        assert_eq!(resolved, Some("test_str"));
    }
}
