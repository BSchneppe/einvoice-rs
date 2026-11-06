//! Tracked XML reader
//!
//! Wraps quick-xml Reader with byte position tracking for source mapping.

use quick_xml::events::Event;
use quick_xml::Reader;
use std::io::BufRead;

use crate::error::{SchemaError, SchemaResult};
use crate::parser::location::SourceSpan;
use crate::xml::reader::{WfError, WfReader};
use crate::xml::ParseLimits;

/// XML event with source span
#[derive(Debug)]
pub struct TrackedEvent<'a> {
    /// The XML event
    pub event: Event<'a>,
    /// Byte span in source
    pub span: SourceSpan,
}

impl<'a> TrackedEvent<'a> {
    /// Create a new tracked event
    pub fn new(event: Event<'a>, span: SourceSpan) -> Self {
        Self { event, span }
    }

    /// Check if this is a start element event
    pub fn is_start(&self) -> bool {
        matches!(self.event, Event::Start(_))
    }

    /// Check if this is an empty element event
    pub fn is_empty(&self) -> bool {
        matches!(self.event, Event::Empty(_))
    }

    /// Check if this is an end element event
    pub fn is_end(&self) -> bool {
        matches!(self.event, Event::End(_))
    }

    /// Check if this is a text event
    pub fn is_text(&self) -> bool {
        matches!(self.event, Event::Text(_))
    }

    /// Check if this is an EOF event
    pub fn is_eof(&self) -> bool {
        matches!(self.event, Event::Eof)
    }
}

/// Tracked XML reader that wraps quick-xml with position tracking
///
/// This reader provides byte spans for all XML events, enabling accurate
/// source location tracking for error messages.
///
/// The text is read through the crate's well-formedness layer
/// ([`crate::xml`]): a document that is not well-formed is an
/// [`SchemaError::XmlError`] naming the rule and the byte position (also
/// available from [`error_position`](Self::error_position)) — input that ends
/// inside an element included. The rules are XML 1.1's for a document
/// declared `version="1.1"` ([`xml_version`](Self::xml_version)). White space
/// before character data and markup is skipped and text is trimmed at both
/// ends, as before. Spans are byte offsets into the input, a leading BOM
/// included.
///
/// Pass the same buffer to every [`read_event`](Self::read_event): the case
/// of a document type declaration's keyword is checked on its markup, still
/// in the buffer, at the next read (a caller that clears the buffer in
/// between skips that one check).
pub struct TrackedReader<R> {
    /// The underlying quick-xml reader, behind the well-formedness layer
    reader: WfReader<R>,
    /// Start of the last event returned (see [`read_event`](Self::read_event))
    last_position: usize,
    /// Input offset of the last well-formedness error
    error_position: Option<usize>,
}

impl<'a> TrackedReader<&'a [u8]> {
    /// Create a new reader from a byte slice
    pub fn from_bytes(bytes: &'a [u8]) -> Self {
        TrackedReader::from_reader(bytes)
    }

    /// Create a reader from a byte slice that refuses a document nested
    /// deeper than `limits.max_depth` — the schema-document parser's
    /// `ParserConfig::max_depth` ([`SchemaError::LimitExceeded`]).
    pub(crate) fn from_bytes_with_limits(bytes: &'a [u8], limits: &ParseLimits) -> Self {
        Self {
            reader: WfReader::new(bytes, limits, true),
            last_position: 0,
            error_position: None,
        }
    }
}

impl<R: BufRead> TrackedReader<R> {
    /// Create a new reader from a BufRead source
    pub fn from_reader(reader: R) -> Self {
        // quick-xml skips the white space before text and markup; the end of
        // a text event is trimmed in `read_event`.
        Self {
            reader: WfReader::new(reader, &ParseLimits::none(), true),
            last_position: 0,
            error_position: None,
        }
    }

    /// Read the next XML event with its source span
    ///
    /// Offsets are input offsets, a leading byte-order mark counted. The span
    /// starts where the event starts: the `<` of a start or empty-element
    /// tag, an XML declaration, comment, processing instruction or CDATA
    /// section, the first character of a text run (after the white space
    /// `trim_text` skips), the `&` of a reference. An end tag's or a
    /// document type declaration's span starts where the previous event
    /// ended — before white space the read skipped — as their length does
    /// not tell where their `<` is.
    pub fn read_event<'b>(&mut self, buf: &'b mut Vec<u8>) -> SchemaResult<TrackedEvent<'b>> {
        let mut event = match self.reader.read_event_into(buf) {
            Ok(event) => event,
            Err(WfError::Io(e)) => {
                return Err(SchemaError::XmlError {
                    message: format!("I/O error: {e}"),
                    location: None, // Will be filled in by caller with proper source mapping
                });
            }
            Err(WfError::NotWellFormed(e)) => {
                self.error_position = Some(e.position() as usize);
                return Err(SchemaError::xml(e.to_string(), None));
            }
            Err(WfError::Limit(e)) => {
                self.error_position = Some(e.position() as usize);
                // Only the schema-document parser sets limits on this reader
                // (`from_bytes_with_limits`), and only the depth limit, from
                // `ParserConfig::max_depth`.
                if e.limit() == crate::xml::ParseLimit::Depth {
                    return Err(SchemaError::limit(
                        crate::error::SchemaLimit::ElementDepth,
                        e.max(),
                        e.found(),
                        format!("element depth {}", e.found()),
                        None,
                    ));
                }
                return Err(SchemaError::xml(e.to_string(), None));
            }
        };
        // `trim_text`: white space at the end of a text run (the start was
        // skipped by the reader; a run of white space only never comes).
        if let Event::Text(t) = &mut event {
            t.inplace_trim_end();
        }

        let end = self.reader.buffer_position() as usize;
        // The layer records where it found the event (this reader never
        // enables `expand_empty_elements`, which would report `<a/>` as a
        // Start).
        let start = self.reader.event_start() as usize;
        self.last_position = start;
        let span = SourceSpan { start, end };

        Ok(TrackedEvent::new(event, span))
    }

    /// Get the current buffer position
    pub fn buffer_position(&self) -> usize {
        self.reader.buffer_position() as usize
    }

    /// The start of the last event returned: its span's `start`
    pub fn last_position(&self) -> usize {
        self.last_position
    }

    /// The input offset (a leading byte-order mark included) of the
    /// violation behind the last well-formedness error
    /// [`read_event`](Self::read_event) returned, if any.
    pub fn error_position(&self) -> Option<usize> {
        self.error_position
    }

    /// The version of XML whose rules the document is read with (known once
    /// its XML declaration has been read).
    pub fn xml_version(&self) -> crate::xml::XmlVersion {
        self.reader.xml_version()
    }

    /// Get a reference to the underlying reader for decoding
    pub fn inner(&self) -> &Reader<R> {
        self.reader.inner()
    }
}

/// Extract local name and prefix from a qualified name
pub fn split_qname(qname: &[u8]) -> (&[u8], Option<&[u8]>) {
    match qname.iter().position(|&b| b == b':') {
        Some(pos) => (&qname[pos + 1..], Some(&qname[..pos])),
        None => (qname, None),
    }
}

/// Configuration for XML parsing
#[derive(Debug, Clone)]
pub struct ReaderConfig {
    /// Trim whitespace in text nodes
    pub trim_text: bool,
    /// Check for duplicate attributes
    pub check_duplicates: bool,
}

impl Default for ReaderConfig {
    fn default() -> Self {
        Self {
            trim_text: true,
            check_duplicates: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracked_reader_basic() {
        let xml = b"<root><child/></root>";
        let mut reader = TrackedReader::from_bytes(xml);
        let mut buf = Vec::new();

        // First event: Start element <root>
        let event = reader.read_event(&mut buf).unwrap();
        assert!(event.is_start());

        // Second event: Empty element <child/>
        buf.clear();
        let event = reader.read_event(&mut buf).unwrap();
        assert!(event.is_empty());

        // Third event: End element </root>
        buf.clear();
        let event = reader.read_event(&mut buf).unwrap();
        assert!(event.is_end());

        // Fourth event: EOF
        buf.clear();
        let event = reader.read_event(&mut buf).unwrap();
        assert!(event.is_eof());
    }

    #[test]
    fn test_tracked_reader_spans() {
        let xml = b"<root>text</root>";
        let mut reader = TrackedReader::from_bytes(xml);
        let mut buf = Vec::new();

        // Start element span
        let event = reader.read_event(&mut buf).unwrap();
        assert!(event.span.start == 0);

        // Text span
        buf.clear();
        let event = reader.read_event(&mut buf).unwrap();
        assert!(event.is_text());
        assert!(event.span.start > 0);
    }

    /// An element's span starts at its `<`, not at the end of whatever event
    /// preceded it — the whitespace between the two is consumed by the same
    /// read.
    #[test]
    fn test_element_span_starts_at_open_angle_bracket() {
        let xml = b"<root>\n   <a x=\"1\"/>\n   <b>t</b>\n</root>";
        let mut reader = TrackedReader::from_bytes(xml);
        let mut buf = Vec::new();

        let event = reader.read_event(&mut buf).unwrap();
        assert!(event.is_start());
        assert_eq!((event.span.start, event.span.end), (0, 6), "<root>");

        buf.clear();
        let event = reader.read_event(&mut buf).unwrap();
        assert!(event.is_empty());
        let start = xml.windows(2).position(|w| w == b"<a").unwrap();
        assert_eq!(
            event.span.start, start,
            "empty element must start at its '<', not after </root>'s predecessor"
        );
        assert_eq!(&xml[event.span.start..event.span.end], b"<a x=\"1\"/>");

        buf.clear();
        let event = reader.read_event(&mut buf).unwrap();
        assert!(event.is_start());
        assert_eq!(&xml[event.span.start..event.span.end], b"<b>");
    }

    #[test]
    fn test_split_qname() {
        assert_eq!(split_qname(b"localName"), (&b"localName"[..], None));
        assert_eq!(
            split_qname(b"xs:element"),
            (&b"element"[..], Some(&b"xs"[..]))
        );
        assert_eq!(split_qname(b"xsi:nil"), (&b"nil"[..], Some(&b"xsi"[..])));
    }

    #[test]
    fn test_tracked_event_type_checks() {
        let xml = b"<root/>";
        let mut reader = TrackedReader::from_bytes(xml);
        let mut buf = Vec::new();

        let event = reader.read_event(&mut buf).unwrap();
        assert!(event.is_empty());
        assert!(!event.is_start());
        assert!(!event.is_end());
        assert!(!event.is_text());
        assert!(!event.is_eof());
    }

    #[test]
    fn test_reader_config_default() {
        let config = ReaderConfig::default();
        assert!(config.trim_text);
        assert!(config.check_duplicates);
    }
}
