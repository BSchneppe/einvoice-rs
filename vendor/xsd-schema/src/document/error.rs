use thiserror::Error;

/// Errors that can occur during `BufferDocument` construction or navigation.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum BufferDocumentError {
    /// An I/O error of the source the text was read from (its own
    /// `std::io::Error`, also as [`source`](std::error::Error::source)).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Two elements of one tree answer to the same id value.
    ///
    /// Reported when a [`DocumentKind::Full`](super::DocumentKind::Full)
    /// document read from text, or built by
    /// [`build_typed_document`](super::build_typed_document), repeats an
    /// `xml:id` value, and by
    /// [`BufferDocumentBuilder::register_xml_id`](super::BufferDocumentBuilder::register_xml_id).
    /// Values are compared as the id index keys them — XML whitespace stripped
    /// at both ends and collapsed inside, so `xml:id="a"` and `xml:id=" a"`
    /// collide — and a value that is not a lexical NCName is never an id, so
    /// it is never a duplicate either. The payload is the normalized value.
    #[error("Duplicate ID value: '{0}'")]
    DuplicateId(String),

    #[error("Node allocation overflow (exceeds u32::MAX - 1 nodes)")]
    Overflow,

    #[error("end_element called without matching start_element")]
    UnmatchedEndElement,

    #[error("Internal error: {0}")]
    InternalError(String),

    /// The XML text is not well-formed or not namespace-well-formed (XML
    /// 1.0 / 1.1 and Namespaces in XML; see [`crate::xml`]): every refusal of
    /// the text read — bytes that are not UTF-8, an unbound prefix, input
    /// that ends inside an element (`Truncated`) included — with its rule and
    /// byte position.
    #[error("{0}")]
    NotWellFormed(crate::xml::NotWellFormed),

    /// The XML text broke one of the reader's
    /// [`ParseLimits`](crate::xml::ParseLimits).
    #[error("{0}")]
    LimitExceeded(crate::xml::LimitExceeded),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_duplicate_id() {
        let err = BufferDocumentError::DuplicateId("myId".into());
        assert_eq!(err.to_string(), "Duplicate ID value: 'myId'");
    }

    #[test]
    fn display_overflow() {
        let err = BufferDocumentError::Overflow;
        assert_eq!(
            err.to_string(),
            "Node allocation overflow (exceeds u32::MAX - 1 nodes)"
        );
    }

    #[test]
    fn from_io_error() {
        let err: BufferDocumentError = std::io::Error::other("the disk is on fire").into();
        assert!(matches!(err, BufferDocumentError::Io(_)));
        assert_eq!(err.to_string(), "I/O error: the disk is on fire");
        let source = std::error::Error::source(&err).expect("the I/O error");
        assert_eq!(source.to_string(), "the disk is on fire");
    }
}
