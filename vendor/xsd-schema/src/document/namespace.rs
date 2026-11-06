//! Namespace chain storage for the document buffer.
//!
//! [`NamespaceNode`] is a 16-byte linked-list node holding a single namespace
//! binding (prefix → namespace URI).  Chains are stored in a page-based arena
//! ([`NamespacePageFactory`]) that mirrors the [`NodePages`](super::page::NodePages)
//! pattern.

use std::cell::Cell;

use bumpalo::Bump;

use crate::ids::NameId;

// ── NsRef ─────────────────────────────────────────────────────────────

/// Opaque index into a [`NamespacePageFactory`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NsRef(pub u32);

impl Default for NsRef {
    /// Default is the NULL sentinel, not slot 0.
    fn default() -> Self {
        Self::NULL
    }
}

impl NsRef {
    /// Sentinel value that marks end-of-chain or "no namespace".
    pub const NULL: NsRef = NsRef(u32::MAX);

    /// Returns `true` if this reference is the NULL sentinel.
    #[inline]
    pub fn is_null(self) -> bool {
        self == Self::NULL
    }
}

// ── Page-addressing constants ─────────────────────────────────────────

/// Page size exponent: 2^12 = 4096 namespace nodes per page.
pub const NS_PAGE_SHIFT: u32 = 12;

/// Namespace nodes per page (4096).
pub const NS_PAGE_SIZE: u32 = 1 << NS_PAGE_SHIFT;

/// Bitmask for extracting the slot within a page.
pub const NS_PAGE_MASK: u32 = NS_PAGE_SIZE - 1; // 0xFFF

// ── NamespaceNode ─────────────────────────────────────────────────────

/// A single namespace binding in a linked chain — 12 bytes.
///
/// Each element that declares namespace bindings stores an [`NsRef`] head
/// that points to the first `NamespaceNode` in a chain.  The `next` field
/// links to the following binding (or [`NsRef::NULL`] for end-of-chain).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NamespaceNode {
    pub prefix: NameId,
    pub namespace_uri: NameId,
    pub next: NsRef,
}

impl Default for NamespaceNode {
    fn default() -> Self {
        Self {
            prefix: NameId(0),
            namespace_uri: NameId(0),
            next: NsRef::NULL,
        }
    }
}

impl NamespaceNode {
    /// Creates a new namespace node.
    pub fn new(prefix: NameId, namespace_uri: NameId, next: NsRef) -> Self {
        Self {
            prefix,
            namespace_uri,
            next,
        }
    }
}

// ── NamespacePageFactory ──────────────────────────────────────────────

/// Arena-backed page array of [`Cell<NamespaceNode>`].
///
/// Mirrors [`NodePages`](super::page::NodePages) but for namespace bindings.
pub struct NamespacePageFactory<'a> {
    arena: &'a Bump,
    pages: Vec<&'a [Cell<NamespaceNode>]>,
    len: u32,
}

/// Returns the page number for a flat namespace index.
#[inline]
fn page_of(ns_ref: NsRef) -> u32 {
    ns_ref.0 >> NS_PAGE_SHIFT
}

/// Returns the slot (offset within page) for a flat namespace index.
#[inline]
fn slot_of(ns_ref: NsRef) -> u32 {
    ns_ref.0 & NS_PAGE_MASK
}

impl<'a> NamespacePageFactory<'a> {
    /// Creates a new factory with the first page pre-allocated.
    pub fn new(arena: &'a Bump) -> Self {
        let first_page = Self::make_page(arena);
        Self {
            arena,
            pages: vec![first_page],
            len: 0,
        }
    }

    /// Allocates the next namespace slot and returns its [`NsRef`].
    ///
    /// Returns `None` if the index would reach `u32::MAX` (reserved as NULL).
    pub fn alloc(&mut self) -> Option<NsRef> {
        let idx = self.len;
        if idx == u32::MAX {
            return None;
        }
        self.len = idx + 1;
        if idx > 0 && slot_of(NsRef(idx)) == 0 {
            self.allocate_page();
        }
        Some(NsRef(idx))
    }

    /// Reads the namespace node at the given reference.
    #[inline]
    pub fn get(&self, ns_ref: NsRef) -> NamespaceNode {
        let page = page_of(ns_ref) as usize;
        let slot = slot_of(ns_ref) as usize;
        self.pages[page][slot].get()
    }

    /// Writes a namespace node at the given reference (interior mutability).
    #[inline]
    pub fn set(&self, ns_ref: NsRef, node: NamespaceNode) {
        let page = page_of(ns_ref) as usize;
        let slot = slot_of(ns_ref) as usize;
        self.pages[page][slot].set(node);
    }

    /// Returns the total number of allocated namespace nodes.
    #[inline]
    pub fn len(&self) -> u32 {
        self.len
    }

    /// Returns `true` if no namespace nodes have been allocated.
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns an iterator over the chain starting at `head` (the raw chain,
    /// hidden bindings included; tests compare readers with it).
    ///
    /// Yields `(prefix, namespace_uri)` pairs.  An [`NsRef::NULL`] head
    /// produces an empty iterator.
    #[cfg(test)]
    pub fn iter_chain(&self, head: NsRef) -> NamespaceChain<'_, 'a> {
        NamespaceChain {
            factory: self,
            cursor: head,
        }
    }

    fn allocate_page(&mut self) {
        let page = Self::make_page(self.arena);
        self.pages.push(page);
    }

    fn make_page(arena: &Bump) -> &[Cell<NamespaceNode>] {
        arena.alloc_slice_fill_with(NS_PAGE_SIZE as usize, |_| {
            Cell::new(NamespaceNode::default())
        })
    }
}

// ── NamespaceChain iterator ───────────────────────────────────────────

/// Iterator over a linked chain of namespace bindings.
#[cfg(test)]
pub struct NamespaceChain<'f, 'a> {
    factory: &'f NamespacePageFactory<'a>,
    cursor: NsRef,
}

#[cfg(test)]
impl Iterator for NamespaceChain<'_, '_> {
    type Item = (NameId, NameId);

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor.is_null() {
            return None;
        }
        let node = self.factory.get(self.cursor);
        self.cursor = node.next;
        Some((node.prefix, node.namespace_uri))
    }
}

// ── NamespaceScopes ───────────────────────────────────────────────────

/// Where the namespace bindings in scope at a node of a document start, and
/// a walk over them that does not revisit what an element hides.
///
/// An element that declares namespaces owns a chain head: its own
/// declarations, last first, followed by the chain in scope at its parent. So
/// one chain holds everything in scope at an element, each prefix's binding in
/// force first. This table answers the two questions a reader of that chain
/// asks, in time that does not grow with the depth of the element:
///
/// * **Which chain is in scope at a node** (`changes`). The builder adds nodes
///   in document order, and the chain in scope changes only where an element
///   that declares namespaces starts (to its own chain) and right after it
///   ends (back to its parent's). The chain in scope at a node is the one of
///   the last change at or before it — found by binary search. A document
///   without declarations has no entry; a declaring element has two at most.
/// * **Where to go next in a walk of the bindings in scope** (`walk_next`, one
///   entry per namespace node). Usually the node's `next`. At an element's
///   last own declaration — where its chain runs into its parent's — the walk
///   skips the leading run of the parent's bindings whose prefixes the element
///   declares itself: they are hidden there, and everywhere below. An element
///   that declares the default namespace again, as some generators do on
///   every element, so passes over all its ancestors' redeclarations at once.
///
/// A skipped node always has a prefix the walk has met already, so a reader
/// that keeps the first binding of each prefix (as every reader of the scope
/// does) sees exactly what it saw on the full chain, in the same order.
#[derive(Debug, Default)]
pub(crate) struct NamespaceScopes {
    /// `(first node, chain head)`, in increasing node order, one entry at most
    /// per node and no two neighbours with the same head.
    changes: Vec<(u32, NsRef)>,
    /// For each namespace node, by index, the node a walk of the bindings in
    /// scope visits next.
    walk_next: Vec<NsRef>,
}

impl NamespaceScopes {
    /// Records a namespace node the builder has just allocated and chained to
    /// `next`. Called for every allocation, in allocation order.
    pub(crate) fn allocated(&mut self, ns_ref: NsRef, next: NsRef) {
        debug_assert_eq!(ns_ref.0 as usize, self.walk_next.len());
        self.walk_next.push(next);
    }

    /// The head of the chain in scope at `node`, or [`NsRef::NULL`] when no
    /// declaration is in scope there. An attribute, a text node and anything
    /// else inside an element share the element's scope.
    #[inline]
    pub(crate) fn scope_at(&self, node: u32) -> NsRef {
        match self.changes.partition_point(|&(from, _)| from <= node) {
            0 => NsRef::NULL,
            i => self.changes[i - 1].1,
        }
    }

    /// The node a walk of the bindings in scope visits after `ns_ref`.
    #[inline]
    pub(crate) fn walk_next(&self, ns_ref: NsRef) -> NsRef {
        self.walk_next[ns_ref.0 as usize]
    }

    /// From node `from` on, the chain in scope starts at `head`.
    ///
    /// `from` is never below an earlier change: nodes are only appended. A
    /// change at the same node replaces the earlier one (an element that
    /// starts right after another one ends, an element whose declarations grow
    /// after it started).
    pub(crate) fn set_scope(&mut self, from: u32, head: NsRef) {
        if self.changes.last().is_some_and(|&(at, _)| at == from) {
            self.changes.pop();
        }
        debug_assert!(
            self.changes.last().is_none_or(|&(at, _)| at < from),
            "scope changes are recorded in node order"
        );
        let current = self.changes.last().map_or(NsRef::NULL, |&(_, h)| h);
        if current != head {
            self.changes.push((from, head));
        }
    }

    /// Element `elem` declares namespaces: its chain is `head`, whose own
    /// declarations run up to the one chained to `outer` (the chain in scope
    /// at its parent). Sets the walk's skip at the end of its own declarations
    /// and makes `head` the scope from `elem` on.
    ///
    /// `own`, when the caller knows it, is the element's first declaration
    /// (the first namespace node allocated for it) and the [`prefix_bit`]s of
    /// all its prefixes, which spare the search for the first and, mostly,
    /// the scan of the declarations for a prefix the element does not declare.
    ///
    /// Called again when a declaration is added to an element that has no
    /// children yet: the skip is computed afresh for the larger set.
    pub(crate) fn declared(
        &mut self,
        pages: &NamespacePageFactory<'_>,
        elem: u32,
        outer: NsRef,
        head: NsRef,
        own: Option<(NsRef, u64)>,
    ) {
        let (last, mask) = match own {
            Some((first, mask)) => (first, mask),
            None => (NsRef::NULL, u64::MAX),
        };
        let (last, own_len) = if last.is_null() {
            let (mut at, mut len) = (head, 1usize);
            loop {
                let next = pages.get(at).next;
                if next == outer || next.is_null() {
                    break (at, len);
                }
                at = next;
                len += 1;
            }
        } else {
            // The builder allocates an element's declarations one after the
            // other, so they are the nodes `last ..= head`.
            debug_assert_eq!(pages.get(last).next, outer);
            (last, (head.0 - last.0) as usize + 1)
        };
        // Whether the element declares `prefix` itself. An element rarely
        // declares more than a handful of prefixes, so a scan of its own
        // declarations, without allocating, is the common answer; one that
        // declares very many, and hides some, gets a set.
        let scan = |prefix: NameId| {
            let mut at = head;
            loop {
                let node = pages.get(at);
                if node.prefix == prefix {
                    return true;
                }
                if at == last {
                    return false;
                }
                at = node.next;
            }
        };
        let mut set: Option<std::collections::HashSet<NameId>> = None;
        let mut skip = outer;
        while !skip.is_null() {
            let prefix = pages.get(skip).prefix;
            let hidden = mask & prefix_bit(prefix) != 0
                && match &set {
                    Some(set) => set.contains(&prefix),
                    None => scan(prefix),
                };
            if !hidden {
                break;
            }
            if set.is_none() && own_len > 16 {
                set = Some(
                    std::iter::successors(Some(head), |&at| {
                        (at != last).then(|| pages.get(at).next)
                    })
                    .map(|at| pages.get(at).prefix)
                    .collect(),
                );
            }
            skip = self.walk_next(skip);
        }
        self.walk_next[last.0 as usize] = skip;
        self.set_scope(elem, head);
    }

    /// The recorded scope changes.
    #[cfg(test)]
    pub(crate) fn changes_for_test(&self) -> &[(u32, NsRef)] {
        &self.changes
    }
}

/// A prefix's bit in a 64-bit filter of the prefixes an element declares: two
/// prefixes may share a bit, a prefix whose bit is clear is not among them.
#[inline]
pub(crate) fn prefix_bit(prefix: NameId) -> u64 {
    1 << (prefix.0 & 63)
}

/// The bindings in scope from a chain head, in walk order (see
/// [`NamespaceScopes`]): each prefix's binding in force comes before any other
/// node of that prefix, and some of those others are not visited at all.
pub(crate) struct ScopeWalk<'d, 'a> {
    pages: &'d NamespacePageFactory<'a>,
    scopes: &'d NamespaceScopes,
    cursor: NsRef,
}

impl<'d, 'a> ScopeWalk<'d, 'a> {
    pub(crate) fn new(
        pages: &'d NamespacePageFactory<'a>,
        scopes: &'d NamespaceScopes,
        head: NsRef,
    ) -> Self {
        Self {
            pages,
            scopes,
            cursor: head,
        }
    }
}

impl Iterator for ScopeWalk<'_, '_> {
    type Item = (NsRef, NamespaceNode);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor.is_null() {
            return None;
        }
        let at = self.cursor;
        let node = self.pages.get(at);
        self.cursor = self.scopes.walk_next(at);
        Some((at, node))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem;

    #[test]
    fn nsref_null_is_u32_max() {
        assert_eq!(NsRef::NULL.0, u32::MAX);
    }

    #[test]
    fn nsref_default_is_null() {
        assert_eq!(NsRef::default(), NsRef::NULL);
        assert!(NsRef::default().is_null());
    }

    #[test]
    fn nsref_is_null() {
        assert!(NsRef::NULL.is_null());
        assert!(!NsRef(0).is_null());
        assert!(!NsRef(42).is_null());
    }

    #[test]
    fn end_of_chain_is_null() {
        let node = NamespaceNode::new(NameId(1), NameId(2), NsRef::NULL);
        assert!(node.next.is_null());
    }

    #[test]
    fn namespace_node_is_12_bytes() {
        assert_eq!(mem::size_of::<NamespaceNode>(), 12);
    }

    #[test]
    fn sequential_alloc() {
        let arena = Bump::new();
        let mut factory = NamespacePageFactory::new(&arena);
        for expected in 0..10u32 {
            let ns_ref = factory.alloc().unwrap();
            assert_eq!(ns_ref.0, expected);
        }
    }

    #[test]
    fn set_get_round_trip() {
        let arena = Bump::new();
        let mut factory = NamespacePageFactory::new(&arena);
        let r = factory.alloc().unwrap();

        let node = NamespaceNode::new(NameId(10), NameId(20), NsRef::NULL);
        factory.set(r, node);
        assert_eq!(factory.get(r), node);
    }

    #[test]
    fn cross_page_allocation() {
        let arena = Bump::new();
        let mut factory = NamespacePageFactory::new(&arena);

        let count = NS_PAGE_SIZE + 1;
        for i in 0..count {
            let r = factory.alloc().unwrap();
            assert_eq!(r.0, i);
            factory.set(
                r,
                NamespaceNode::new(NameId(i), NameId(i + 1000), NsRef::NULL),
            );
        }
        assert_eq!(factory.len(), count);

        // Verify last node on first page
        let last_first = NsRef(NS_PAGE_SIZE - 1);
        assert_eq!(factory.get(last_first).prefix, NameId(NS_PAGE_SIZE - 1));

        // Verify first node on second page
        let first_second = NsRef(NS_PAGE_SIZE);
        assert_eq!(factory.get(first_second).prefix, NameId(NS_PAGE_SIZE));
    }

    #[test]
    fn single_node_chain() {
        let arena = Bump::new();
        let mut factory = NamespacePageFactory::new(&arena);
        let r = factory.alloc().unwrap();
        factory.set(r, NamespaceNode::new(NameId(1), NameId(2), NsRef::NULL));

        let items: Vec<_> = factory.iter_chain(r).collect();
        assert_eq!(items, vec![(NameId(1), NameId(2))]);
    }

    #[test]
    fn two_node_chain() {
        let arena = Bump::new();
        let mut factory = NamespacePageFactory::new(&arena);

        let r1 = factory.alloc().unwrap();
        let r0 = factory.alloc().unwrap();

        // Build chain: r0 -> r1 -> NULL
        factory.set(r1, NamespaceNode::new(NameId(3), NameId(4), NsRef::NULL));
        factory.set(r0, NamespaceNode::new(NameId(1), NameId(2), r1));

        let items: Vec<_> = factory.iter_chain(r0).collect();
        assert_eq!(items, vec![(NameId(1), NameId(2)), (NameId(3), NameId(4))]);
    }

    #[test]
    fn multi_node_chain() {
        let arena = Bump::new();
        let mut factory = NamespacePageFactory::new(&arena);

        let r2 = factory.alloc().unwrap();
        let r1 = factory.alloc().unwrap();
        let r0 = factory.alloc().unwrap();

        factory.set(r2, NamespaceNode::new(NameId(5), NameId(6), NsRef::NULL));
        factory.set(r1, NamespaceNode::new(NameId(3), NameId(4), r2));
        factory.set(r0, NamespaceNode::new(NameId(1), NameId(2), r1));

        let items: Vec<_> = factory.iter_chain(r0).collect();
        assert_eq!(
            items,
            vec![
                (NameId(1), NameId(2)),
                (NameId(3), NameId(4)),
                (NameId(5), NameId(6)),
            ]
        );
    }

    #[test]
    fn null_chain_yields_empty() {
        let arena = Bump::new();
        let factory = NamespacePageFactory::new(&arena);
        let items: Vec<_> = factory.iter_chain(NsRef::NULL).collect();
        assert!(items.is_empty());
    }

    /// One entry per change, the last one at a node winning, none for a
    /// "change" to the chain already in scope.
    #[test]
    fn scope_changes_coalesce() {
        let mut scopes = NamespaceScopes::default();
        assert!(scopes.scope_at(0).is_null());
        scopes.set_scope(5, NsRef(1));
        scopes.set_scope(9, NsRef::NULL);
        // An element that starts where another one ended.
        scopes.set_scope(9, NsRef(2));
        // Back to the chain already in scope.
        scopes.set_scope(12, NsRef(2));
        assert_eq!(scopes.changes, [(5, NsRef(1)), (9, NsRef(2))]);
        assert!(scopes.scope_at(4).is_null());
        assert_eq!(scopes.scope_at(5), NsRef(1));
        assert_eq!(scopes.scope_at(8), NsRef(1));
        assert_eq!(scopes.scope_at(9), NsRef(2));
        assert_eq!(scopes.scope_at(1000), NsRef(2));
        scopes.set_scope(20, NsRef(1));
        scopes.set_scope(20, NsRef(2));
        assert_eq!(scopes.changes.len(), 2);
    }

    #[test]
    fn len_and_is_empty() {
        let arena = Bump::new();
        let mut factory = NamespacePageFactory::new(&arena);
        assert_eq!(factory.len(), 0);
        assert!(factory.is_empty());

        factory.alloc().unwrap();
        assert_eq!(factory.len(), 1);
        assert!(!factory.is_empty());
    }
}
