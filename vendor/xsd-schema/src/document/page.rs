use std::cell::Cell;

use bumpalo::Bump;

use super::error::BufferDocumentError;
use super::node::{page_of, slot_of, Node, PAGE_SIZE};

/// Page-based flat node array using `Cell<Node>` for interior mutability.
///
/// Nodes are allocated sequentially and addressed by flat `u32` indices.
/// Pages of [`PAGE_SIZE`] nodes each are arena-allocated on demand.
/// `Cell<Node>` allows mutation through a shared reference, which is
/// needed when the builder holds `&mut` to its own state but only `&`
/// to the pages for sibling/parent fixup.
pub struct NodePages<'a> {
    arena: &'a Bump,
    pages: Vec<&'a [Cell<Node>]>,
    len: u32,
}

impl<'a> NodePages<'a> {
    /// Creates a new `NodePages` with the first page pre-allocated.
    pub fn new(arena: &'a Bump) -> Self {
        let first_page = Self::make_page(arena);
        Self {
            arena,
            pages: vec![first_page],
            len: 0,
        }
    }

    /// Allocates the next node slot and returns its flat index.
    ///
    /// A new page is allocated when the current page is full.
    ///
    /// # Errors
    ///
    /// Returns [`BufferDocumentError::Overflow`] if the index would exceed `u32::MAX - 1`.
    pub fn alloc(&mut self) -> Result<u32, BufferDocumentError> {
        let idx = self.len;
        // Reserve u32::MAX as NULL sentinel
        if idx == u32::MAX {
            return Err(BufferDocumentError::Overflow);
        }
        self.len = idx + 1;
        // If we just crossed into a new page (and it's not the very first allocation),
        // allocate the page.
        if idx > 0 && slot_of(idx) == 0 {
            self.allocate_page();
        }
        Ok(idx)
    }

    /// Reads the node at the given flat index.
    #[inline]
    pub fn get(&self, node_ref: u32) -> Node {
        let page = page_of(node_ref) as usize;
        let slot = slot_of(node_ref) as usize;
        self.pages[page][slot].get()
    }

    /// Writes a node at the given flat index (interior mutability via `Cell`).
    #[inline]
    pub fn set(&self, node_ref: u32, node: Node) {
        let page = page_of(node_ref) as usize;
        let slot = slot_of(node_ref) as usize;
        self.pages[page][slot].set(node);
    }

    /// Read-modify-write a node at the given flat index.
    #[inline]
    pub fn update<F: FnOnce(&mut Node)>(&self, node_ref: u32, f: F) {
        let page = page_of(node_ref) as usize;
        let slot = slot_of(node_ref) as usize;
        let mut node = self.pages[page][slot].get();
        f(&mut node);
        self.pages[page][slot].set(node);
    }

    /// Returns the total number of allocated nodes.
    #[inline]
    pub fn len(&self) -> u32 {
        self.len
    }

    /// Returns `true` if no nodes have been allocated.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Allocates a new page in the arena and appends it.
    fn allocate_page(&mut self) {
        let page = Self::make_page(self.arena);
        self.pages.push(page);
    }

    /// Arena-allocates a single page of `Cell<Node>`.
    fn make_page(arena: &Bump) -> &[Cell<Node>] {
        arena.alloc_slice_fill_with(PAGE_SIZE as usize, |_| Cell::new(Node::default()))
    }
}

#[cfg(test)]
mod tests {
    use super::super::node::{NodeType, NULL, PAGE_SIZE};
    use super::*;

    #[test]
    fn first_alloc_returns_zero() {
        let arena = Bump::new();
        let mut pages = NodePages::new(&arena);
        assert_eq!(pages.alloc().unwrap(), 0);
    }

    #[test]
    fn sequential_alloc() {
        let arena = Bump::new();
        let mut pages = NodePages::new(&arena);
        for expected in 0..10 {
            assert_eq!(pages.alloc().unwrap(), expected);
        }
    }

    #[test]
    fn set_get_round_trip() {
        let arena = Bump::new();
        let mut pages = NodePages::new(&arena);
        let idx = pages.alloc().unwrap();

        let mut node = Node::default();
        node.set_node_type(NodeType::Element);
        node.value = 42;
        node.parent = NULL;
        node.next_sibling = NULL;

        pages.set(idx, node);
        let read = pages.get(idx);
        assert_eq!(read.node_type(), NodeType::Element);
        assert_eq!(read.value, 42);
        assert_eq!(read.parent, NULL);
        assert_eq!(read.next_sibling, NULL);
    }

    #[test]
    fn update_modifies_in_place() {
        let arena = Bump::new();
        let mut pages = NodePages::new(&arena);
        let idx = pages.alloc().unwrap();

        let mut node = Node::default();
        node.set_node_type(NodeType::Text);
        node.value = 10;
        pages.set(idx, node);

        pages.update(idx, |n| {
            n.value = 99;
            n.set_flag(Node::HAS_CHILDREN);
        });

        let read = pages.get(idx);
        assert_eq!(read.value, 99);
        assert!(read.has_flag(Node::HAS_CHILDREN));
        assert_eq!(read.node_type(), NodeType::Text); // unchanged
    }

    #[test]
    fn cross_page_allocation() {
        let arena = Bump::new();
        let mut pages = NodePages::new(&arena);

        // Allocate PAGE_SIZE + 1 nodes to force a second page
        let count = PAGE_SIZE + 1;
        for i in 0..count {
            let idx = pages.alloc().unwrap();
            assert_eq!(idx, i);
            // Write a distinguishing value
            pages.set(
                idx,
                Node {
                    value: i,
                    ..Node::default()
                },
            );
        }

        assert_eq!(pages.len(), count);

        // Verify last node on first page
        let last_first = PAGE_SIZE - 1;
        assert_eq!(pages.get(last_first).value, last_first);

        // Verify first node on second page
        assert_eq!(pages.get(PAGE_SIZE).value, PAGE_SIZE);
    }

    #[test]
    fn len_tracks_allocations() {
        let arena = Bump::new();
        let mut pages = NodePages::new(&arena);
        assert_eq!(pages.len(), 0);
        assert!(pages.is_empty());

        pages.alloc().unwrap();
        assert_eq!(pages.len(), 1);
        assert!(!pages.is_empty());

        for _ in 0..9 {
            pages.alloc().unwrap();
        }
        assert_eq!(pages.len(), 10);
    }

    #[test]
    fn default_node_is_nul() {
        let arena = Bump::new();
        let mut pages = NodePages::new(&arena);
        let idx = pages.alloc().unwrap();
        let node = pages.get(idx);
        assert_eq!(node.node_type(), NodeType::Nul);
        assert_eq!(node.value, 0);
        assert_eq!(node.parent, 0);
        assert_eq!(node.next_sibling, 0);
    }

    #[test]
    fn set_with_shared_ref() {
        // Verify that `set` works through `&self` (not `&mut self`).
        let arena = Bump::new();
        let mut pages = NodePages::new(&arena);
        let idx = pages.alloc().unwrap();

        // Use shared reference for set
        let pages_ref: &NodePages = &pages;
        let mut node = Node::default();
        node.set_node_type(NodeType::Comment);
        node.value = 7;
        pages_ref.set(idx, node);

        assert_eq!(pages.get(idx).node_type(), NodeType::Comment);
        assert_eq!(pages.get(idx).value, 7);
    }

    #[test]
    fn alloc_overflow_returns_error() {
        let arena = Bump::new();
        let mut pages = NodePages::new(&arena);
        // Force len to u32::MAX so the next alloc hits the overflow guard.
        pages.len = u32::MAX;
        assert!(matches!(pages.alloc(), Err(BufferDocumentError::Overflow)));
    }
}
