use bumpalo::Bump;

/// Strings shorter than or equal to this threshold are arena-allocated.
const SHORT_THRESHOLD: usize = 64;

/// Internal representation: short strings live in the arena, long ones on the heap.
enum StringValue<'a> {
    Short(&'a str),
    Long(Box<str>),
}

impl<'a> StringValue<'a> {
    fn as_str(&self) -> &str {
        match self {
            StringValue::Short(s) => s,
            StringValue::Long(s) => s,
        }
    }
}

/// Arena-allocated string pool with heap fallback for long strings.
///
/// Index 0 is a sentinel representing the empty string (no entry is stored for it).
/// All stored strings receive 1-based indices.
pub struct StringStore<'a> {
    arena: &'a Bump,
    values: Vec<StringValue<'a>>,
}

impl<'a> StringStore<'a> {
    /// Creates a new empty string store backed by the given arena.
    pub fn new(arena: &'a Bump) -> Self {
        Self {
            arena,
            values: Vec::new(),
        }
    }

    /// Stores a string and returns its 1-based index.
    ///
    /// Strings up to `SHORT_THRESHOLD` bytes are copied into the arena;
    /// longer strings are heap-allocated.
    pub fn store(&mut self, s: &str) -> u32 {
        let val = if s.len() <= SHORT_THRESHOLD {
            let copied = self.arena.alloc_str(s);
            StringValue::Short(copied)
        } else {
            StringValue::Long(s.into())
        };
        self.values.push(val);
        self.values.len() as u32 // 1-based
    }

    /// Releases spare capacity in the index vector once the document is fully
    /// built. The `Vec<StringValue>` grows geometrically during the build, so it
    /// can hold up to ~2× the entries it actually needs; this hands the slack
    /// back to the allocator. The arena (the string bytes) is untouched.
    pub fn shrink_to_fit(&mut self) {
        self.values.shrink_to_fit();
    }

    /// Returns the string at the given index.
    ///
    /// Index 0 returns `""` (empty-string sentinel).
    /// Other indices are 1-based into the internal storage.
    ///
    /// # Panics
    ///
    /// Panics if `idx` is out of range (greater than the number of stored strings).
    pub fn get(&self, idx: u32) -> &str {
        if idx == 0 {
            return "";
        }
        self.values[(idx - 1) as usize].as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_and_retrieve_short_string() {
        let arena = Bump::new();
        let mut store = StringStore::new(&arena);
        let idx = store.store("hello");
        assert_eq!(idx, 1);
        assert_eq!(store.get(idx), "hello");
    }

    #[test]
    fn store_and_retrieve_long_string() {
        let arena = Bump::new();
        let mut store = StringStore::new(&arena);
        let long = "x".repeat(100);
        let idx = store.store(&long);
        assert_eq!(idx, 1);
        assert_eq!(store.get(idx), long);
    }

    #[test]
    fn index_zero_returns_empty() {
        let arena = Bump::new();
        let store = StringStore::new(&arena);
        assert_eq!(store.get(0), "");
    }

    #[test]
    fn sequential_one_based_indices() {
        let arena = Bump::new();
        let mut store = StringStore::new(&arena);
        let i1 = store.store("a");
        let i2 = store.store("bb");
        let i3 = store.store("ccc");
        assert_eq!(i1, 1);
        assert_eq!(i2, 2);
        assert_eq!(i3, 3);
        assert_eq!(store.get(1), "a");
        assert_eq!(store.get(2), "bb");
        assert_eq!(store.get(3), "ccc");
    }

    #[test]
    fn store_empty_string() {
        let arena = Bump::new();
        let mut store = StringStore::new(&arena);
        let idx = store.store("");
        assert_eq!(idx, 1);
        assert_eq!(store.get(idx), "");
    }

    #[test]
    fn boundary_short_long() {
        let arena = Bump::new();
        let mut store = StringStore::new(&arena);
        // Exactly at threshold — should be Short
        let at_threshold = "a".repeat(SHORT_THRESHOLD);
        let idx1 = store.store(&at_threshold);
        assert_eq!(store.get(idx1), at_threshold);
        // One byte over — should be Long
        let over_threshold = "a".repeat(SHORT_THRESHOLD + 1);
        let idx2 = store.store(&over_threshold);
        assert_eq!(store.get(idx2), over_threshold);
    }
}
