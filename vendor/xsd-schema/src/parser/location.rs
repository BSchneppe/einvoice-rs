//! Location tracking for XSD parsing
//!
//! This module provides accurate source location tracking using byte offsets from quick-xml.
//! It supports three retention modes to balance memory usage vs. error reporting fidelity:
//!
//! - `Retain`: Keep full source text (default, ~200KB per 200KB schema)
//! - `DropText`: Keep only line starts (~10KB per 200KB schema)
//! - `DropAll`: No location info (minimal memory)
//!
//! Per XSD_PARSER_DESIGN.md:
//! - Use quick-xml's `buffer_position()` for byte offsets
//! - Build line_starts table once per document
//! - Handle CR, LF, and CRLF line endings correctly
//! - Column calculation counts UTF-8 characters, not bytes

use crate::ids::DocumentId;
use std::fmt;

/// Byte range within a document
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

impl SourceSpan {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(&self) -> bool {
        self.start >= self.end
    }
}

/// Reference to a location within a schema document
#[derive(Debug, Clone)]
pub struct SourceRef {
    pub doc_id: DocumentId,
    pub span: SourceSpan,
    /// When set, overrides `doc_id` for schema-document-level defaults
    /// lookup (elementFormDefault, attributeFormDefault, blockDefault,
    /// finalDefault, defaultAttributes). Used for `xs:override` children
    /// that are conceptually placed in the overridden document D2 per
    /// §4.2.5 / F.2 transformation semantics.
    pub schema_defaults_doc: Option<DocumentId>,
}

impl SourceRef {
    pub fn new(doc_id: DocumentId, span: SourceSpan) -> Self {
        Self {
            doc_id,
            span,
            schema_defaults_doc: None,
        }
    }

    /// The document ID to use for schema-level defaults lookup.
    ///
    /// Returns `schema_defaults_doc` if set (override components),
    /// otherwise falls back to `doc_id`.
    pub fn defaults_doc(&self) -> DocumentId {
        self.schema_defaults_doc.unwrap_or(self.doc_id)
    }
}

/// Line/column location for error reporting (1-based)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation {
    pub base_uri: String,
    pub line: usize,   // 1-based
    pub column: usize, // 1-based
}

impl fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.base_uri, self.line, self.column)
    }
}

/// Source buffer retention policy
///
/// Controls memory vs. error reporting trade-off:
/// - `Retain`: Full source text available (~200KB per 200KB schema)
/// - `DropText`: Only line starts (~10KB per 200KB schema, 90% savings)
/// - `DropAll`: No location info (minimal memory)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SourceRetention {
    /// Keep source text for rich errors and XmlFragment access
    #[default]
    Retain,
    /// Drop source text after parsing; keep only line_starts
    DropText,
    /// Drop entire SourceMap after parsing; no location info
    DropAll,
}

/// Per-document source mapping for line/column resolution
///
/// Owns the source text buffer and line start index.
/// Use `build_line_starts()` to construct the line index once.
#[derive(Debug, Clone)]
pub struct SourceMap {
    pub base_uri: String,
    pub text: String,
    pub line_starts: Vec<usize>,
}

impl SourceMap {
    /// Create a new source map from source text
    pub fn new(base_uri: String, text: String) -> Self {
        let line_starts = build_line_starts(text.as_bytes());
        Self {
            base_uri,
            text,
            line_starts,
        }
    }

    /// Convert byte offset to line/column location
    ///
    /// Returns 1-based line and column numbers.
    /// Column is counted in UTF-8 characters, not bytes.
    pub fn locate(&self, offset: usize) -> SourceLocation {
        let (line, line_start) = self.find_line(offset);
        let column = self.count_utf8_chars(line_start, offset) + 1;

        SourceLocation {
            base_uri: self.base_uri.clone(),
            line,
            column,
        }
    }

    /// Find line number (1-based) and line start offset for byte offset
    fn find_line(&self, offset: usize) -> (usize, usize) {
        // Binary search for the line containing this offset
        match self.line_starts.binary_search(&offset) {
            Ok(idx) => (idx + 1, offset), // Exact match at line start
            Err(idx) => {
                if idx == 0 {
                    (1, 0)
                } else {
                    (idx, self.line_starts[idx - 1])
                }
            }
        }
    }

    /// Count UTF-8 characters between start and end byte offsets
    fn count_utf8_chars(&self, start: usize, end: usize) -> usize {
        let end = end.min(self.text.len());
        let start = start.min(end);
        self.text[start..end].chars().count()
    }

    /// Get source text for a span (returns None if span invalid)
    pub fn get_text(&self, span: &SourceSpan) -> Option<&str> {
        if span.end <= self.text.len() && span.start <= span.end {
            Some(&self.text[span.start..span.end])
        } else {
            None
        }
    }

    /// Convert to compact source map (drops text, keeps line_starts)
    pub fn into_compact(self) -> CompactSourceMap {
        CompactSourceMap {
            base_uri: self.base_uri,
            line_starts: self.line_starts,
            text_len: self.text.len(),
        }
    }
}

/// Compact source map when text is dropped (DropText mode)
///
/// Retains line mapping but not source text.
/// Provides line/column location but cannot extract source text spans.
#[derive(Debug, Clone)]
pub struct CompactSourceMap {
    pub base_uri: String,
    pub line_starts: Vec<usize>,
    pub text_len: usize, // for bounds checking
}

impl CompactSourceMap {
    /// Convert byte offset to line/column location
    ///
    /// Column calculation is approximate (byte offset from line start)
    /// since we don't have the original text to count UTF-8 characters.
    pub fn locate(&self, offset: usize) -> SourceLocation {
        let (line, line_start) = self.find_line(offset);
        let column = offset.saturating_sub(line_start) + 1;

        SourceLocation {
            base_uri: self.base_uri.clone(),
            line,
            column,
        }
    }

    fn find_line(&self, offset: usize) -> (usize, usize) {
        match self.line_starts.binary_search(&offset) {
            Ok(idx) => (idx + 1, offset),
            Err(idx) => {
                if idx == 0 {
                    (1, 0)
                } else {
                    (idx, self.line_starts[idx - 1])
                }
            }
        }
    }
}

/// Centralized source map storage with configurable retention
///
/// Owned by SchemaSet to manage source buffers for all documents.
/// See XSD.md (Source Buffer Storage section) for memory management.
#[derive(Debug, Default)]
pub enum SourceMapStorage {
    /// Full source text retained (default)
    Full(Vec<SourceMap>),
    /// Text dropped; only line mapping kept
    Compact(Vec<CompactSourceMap>),
    /// No source info retained
    #[default]
    None,
}

impl SourceMapStorage {
    /// Create new storage in Full mode
    pub fn new() -> Self {
        SourceMapStorage::Full(Vec::new())
    }

    /// Add a source map
    pub fn add(&mut self, map: SourceMap) -> DocumentId {
        match self {
            SourceMapStorage::Full(maps) => {
                let id = maps.len() as DocumentId;
                maps.push(map);
                id
            }
            SourceMapStorage::Compact(maps) => {
                let id = maps.len() as DocumentId;
                maps.push(map.into_compact());
                id
            }
            SourceMapStorage::None => 0, // Discarded
        }
    }

    /// Drop the source maps of documents `len..`, keeping the first `len`.
    ///
    /// Used to withdraw the source map of a document that failed to
    /// assemble, so the source maps stay parallel to `SchemaSet::documents`.
    pub(crate) fn truncate(&mut self, len: usize) {
        match self {
            SourceMapStorage::Full(maps) => maps.truncate(len),
            SourceMapStorage::Compact(maps) => maps.truncate(len),
            SourceMapStorage::None => {}
        }
    }

    /// Resolve SourceRef to SourceLocation
    pub fn locate(&self, source_ref: &SourceRef) -> Option<SourceLocation> {
        match self {
            SourceMapStorage::Full(maps) => {
                let map = maps.get(source_ref.doc_id as usize)?;
                Some(map.locate(source_ref.span.start))
            }
            SourceMapStorage::Compact(maps) => {
                let map = maps.get(source_ref.doc_id as usize)?;
                Some(map.locate(source_ref.span.start))
            }
            SourceMapStorage::None => None,
        }
    }

    /// Get source text slice for XmlFragment (requires Full mode)
    pub fn get_text(&self, doc_id: DocumentId, span: &SourceSpan) -> Option<&str> {
        match self {
            SourceMapStorage::Full(maps) => {
                let map = maps.get(doc_id as usize)?;
                map.get_text(span)
            }
            _ => None,
        }
    }

    /// Compact storage by dropping source text (Full -> Compact)
    ///
    /// Saves ~90% memory but loses ability to extract source text spans.
    pub fn compact(&mut self) {
        if let SourceMapStorage::Full(maps) = self {
            let compact_maps = maps.drain(..).map(|map| map.into_compact()).collect();
            *self = SourceMapStorage::Compact(compact_maps);
        }
    }

    /// Drop all source info (any -> None)
    ///
    /// Minimal memory but no location info in errors.
    pub fn drop_all(&mut self) {
        *self = SourceMapStorage::None;
    }

    /// Check if storage is empty
    pub fn is_empty(&self) -> bool {
        match self {
            SourceMapStorage::Full(maps) => maps.is_empty(),
            SourceMapStorage::Compact(maps) => maps.is_empty(),
            SourceMapStorage::None => true,
        }
    }

    /// Get number of documents stored
    pub fn len(&self) -> usize {
        match self {
            SourceMapStorage::Full(maps) => maps.len(),
            SourceMapStorage::Compact(maps) => maps.len(),
            SourceMapStorage::None => 0,
        }
    }
}

/// Build line start index from source bytes
///
/// Handles CR, LF, and CRLF line endings correctly:
/// - LF (Unix): \n
/// - CRLF (Windows): \r\n (counted as single line end)
/// - CR (Mac Classic): \r
///
/// Returns byte offsets where each line starts (0-indexed).
/// First line always starts at 0.
pub fn build_line_starts(bytes: &[u8]) -> Vec<usize> {
    let mut line_starts = vec![0];
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'\n' => {
                // LF: next line starts after \n
                line_starts.push(i + 1);
                i += 1;
            }
            b'\r' => {
                // CR or CRLF
                if i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
                    // CRLF: next line starts after \r\n
                    line_starts.push(i + 2);
                    i += 2;
                } else {
                    // CR: next line starts after \r
                    line_starts.push(i + 1);
                    i += 1;
                }
            }
            _ => {
                i += 1;
            }
        }
    }

    line_starts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_line_starts_lf() {
        let bytes = b"line1\nline2\nline3";
        let starts = build_line_starts(bytes);
        assert_eq!(starts, vec![0, 6, 12]);
    }

    #[test]
    fn test_build_line_starts_crlf() {
        let bytes = b"line1\r\nline2\r\nline3";
        let starts = build_line_starts(bytes);
        assert_eq!(starts, vec![0, 7, 14]);
    }

    #[test]
    fn test_build_line_starts_cr() {
        let bytes = b"line1\rline2\rline3";
        let starts = build_line_starts(bytes);
        assert_eq!(starts, vec![0, 6, 12]);
    }

    #[test]
    fn test_build_line_starts_mixed() {
        let bytes = b"line1\nline2\r\nline3\rline4";
        let starts = build_line_starts(bytes);
        assert_eq!(starts, vec![0, 6, 13, 19]);
    }

    #[test]
    fn test_source_map_locate() {
        let source = "line1\nline2\nline3".to_string();
        let map = SourceMap::new("test.xsd".to_string(), source);

        // First line, first column
        let loc = map.locate(0);
        assert_eq!(loc.line, 1);
        assert_eq!(loc.column, 1);

        // Second line, first column
        let loc = map.locate(6);
        assert_eq!(loc.line, 2);
        assert_eq!(loc.column, 1);

        // Second line, third column
        let loc = map.locate(8);
        assert_eq!(loc.line, 2);
        assert_eq!(loc.column, 3);
    }

    #[test]
    fn test_source_map_utf8_columns() {
        let source = "Hello 世界\nNext line".to_string();
        let map = SourceMap::new("test.xsd".to_string(), source);

        // "世" is at byte offset 6 but character offset 7 (1-based)
        let loc = map.locate(6);
        assert_eq!(loc.line, 1);
        assert_eq!(loc.column, 7); // UTF-8 character count, not bytes
    }

    #[test]
    fn test_source_map_get_text() {
        let source = "line1\nline2\nline3".to_string();
        let map = SourceMap::new("test.xsd".to_string(), source);

        let span = SourceSpan::new(0, 5);
        assert_eq!(map.get_text(&span), Some("line1"));

        let span = SourceSpan::new(6, 11);
        assert_eq!(map.get_text(&span), Some("line2"));
    }

    #[test]
    fn test_source_map_storage() {
        let mut storage = SourceMapStorage::new();

        let map1 = SourceMap::new("test1.xsd".to_string(), "line1\nline2".to_string());
        let doc_id = storage.add(map1);

        let source_ref = SourceRef::new(doc_id, SourceSpan::new(0, 5));
        let loc = storage.locate(&source_ref).unwrap();
        assert_eq!(loc.line, 1);
        assert_eq!(loc.column, 1);

        // Test compact
        storage.compact();
        let loc = storage.locate(&source_ref).unwrap();
        assert_eq!(loc.line, 1);
        assert!(loc.column > 0); // Approximate in compact mode

        // Cannot get text in compact mode
        assert!(storage.get_text(doc_id, &SourceSpan::new(0, 5)).is_none());
    }
}
