//! Per-node source span tracking for the document buffer.
//!
//! [`NodeSourceSpans`] maps node references to byte ranges in the original
//! XML source, enabling error messages that point to the exact location.

use std::collections::HashMap;

use crate::parser::location::SourceSpan;

/// Mapping from node ref → [`SourceSpan`] (byte range in the source document).
///
/// Only populated when [`BufferDocumentOptions::track_source_locations`](super::BufferDocumentOptions::track_source_locations)
/// is enabled.
#[derive(Debug, Default)]
pub struct NodeSourceSpans {
    map: HashMap<u32, SourceSpan>,
}

impl NodeSourceSpans {
    /// Creates a new empty span table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records (or overwrites) the source span for `node_ref`.
    pub fn set(&mut self, node_ref: u32, span: SourceSpan) {
        self.map.insert(node_ref, span);
    }

    /// Returns the source span for `node_ref`, if recorded.
    pub fn get(&self, node_ref: u32) -> Option<SourceSpan> {
        self.map.get(&node_ref).copied()
    }

    /// Returns the number of nodes with recorded spans.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Returns `true` if no spans have been recorded.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_table_is_empty() {
        let spans = NodeSourceSpans::new();
        assert!(spans.is_empty());
        assert_eq!(spans.len(), 0);
    }

    #[test]
    fn set_get_round_trip() {
        let mut spans = NodeSourceSpans::new();
        let span = SourceSpan::new(10, 25);
        spans.set(3, span);
        assert_eq!(spans.get(3), Some(span));
    }

    #[test]
    fn unrecorded_node_returns_none() {
        let spans = NodeSourceSpans::new();
        assert_eq!(spans.get(42), None);
    }

    #[test]
    fn set_overwrites_previous() {
        let mut spans = NodeSourceSpans::new();
        spans.set(1, SourceSpan::new(0, 10));
        spans.set(1, SourceSpan::new(5, 20));
        assert_eq!(spans.get(1), Some(SourceSpan::new(5, 20)));
    }

    #[test]
    fn multiple_nodes_independent() {
        let mut spans = NodeSourceSpans::new();
        spans.set(0, SourceSpan::new(0, 10));
        spans.set(1, SourceSpan::new(10, 20));
        spans.set(2, SourceSpan::new(20, 30));

        assert_eq!(spans.get(0), Some(SourceSpan::new(0, 10)));
        assert_eq!(spans.get(1), Some(SourceSpan::new(10, 20)));
        assert_eq!(spans.get(2), Some(SourceSpan::new(20, 30)));
        assert_eq!(spans.len(), 3);
    }
}
