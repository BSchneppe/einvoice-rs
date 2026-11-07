//! Everything that can go wrong while composing a document.

use std::io;

use crate::document::{BufferDocumentError, CopyError, SerializeError};
use crate::xpath::XPathError;

/// Why a composition step failed.
///
/// Every variant either wraps an error from the layer that produced it —
/// the XPath engine, the document builder, the subtree copy, the serializer,
/// the file system — or reports a mistake in the composition itself: a prefix
/// nothing binds, a name that is not an XML name, a value of the wrong shape.
///
/// `From` conversions exist for each wrapped error, so `?` works throughout a
/// composition body. The two variants that carry context —
/// [`XPath`](ComposeError::XPath) and [`Copy`](ComposeError::Copy) — get it
/// filled in by the code that has it: the conversion leaves the expression
/// text and the form path empty, and
/// [`Composer::eval`](crate::compose::Composer::eval) and the emitter set
/// them.
///
/// ```
/// use xsd_schema::compose::ComposeError;
///
/// let err = ComposeError::NotSingleton { len: 3 };
/// assert!(err.to_string().contains('3'));
/// ```
#[derive(Debug, thiserror::Error)]
pub enum ComposeError {
    /// An expression could not be compiled or evaluated.
    ///
    /// `expr` is the expression text, so a failure names the call site rather
    /// than just the error code.
    ///
    /// ```
    /// use bumpalo::Bump;
    /// use xsd_schema::compose::{ComposeError, Composer};
    /// use xsd_schema::namespace::NameTable;
    ///
    /// let arena = Bump::new();
    /// let names = NameTable::new();
    /// let c = Composer::new(&arena, &names);
    ///
    /// // `$missing` was never declared.
    /// match c.eval("$missing", &[], None, Vec::new()) {
    ///     Err(ComposeError::XPath { expr, .. }) => assert_eq!(expr, "$missing"),
    ///     other => panic!("expected a refusal, got {other:?}"),
    /// }
    /// ```
    #[error("the expression {expr:?} failed: {source}")]
    XPath {
        /// What the engine reported.
        #[source]
        source: XPathError,
        /// The expression text, empty when the error was converted with `?`
        /// before reaching a call site that knows it.
        expr: String,
    },

    /// The document builder refused a node, or a document could not be parsed.
    #[error("the document could not be built: {0}")]
    Document(#[from] BufferDocumentError),

    /// A spliced value could not be copied into the result.
    ///
    /// `at` is the path of the form whose content failed, spelled like an
    /// XPath step list — `result/item_tuple[3]`.
    #[error("the content of {at} could not be copied: {source}")]
    Copy {
        /// What the copy reported.
        #[source]
        source: CopyError,
        /// The failing form's path, empty when the error was converted with
        /// `?` outside the emitter.
        at: String,
    },

    /// The result could not be written as XML.
    #[error("the document could not be written as XML: {0}")]
    Serialize(#[from] SerializeError),

    /// A file could not be read, or a writer failed.
    #[error("input/output failed: {0}")]
    Io(#[from] io::Error),

    /// A form used a prefix that neither the form nor the composer binds.
    #[error("no namespace is bound to the prefix {prefix:?}, used at {at}")]
    UnboundPrefix {
        /// The unbound prefix.
        prefix: String,
        /// The path of the form that used it.
        at: String,
    },

    /// A literal name is not an XML `NCName` or `prefix:NCName`.
    #[error("{0:?} is not an XML name")]
    InvalidName(String),

    /// An atomic value turned up where a node was required.
    #[error("an atomic value appeared where a node was required")]
    NotANode,

    /// A sequence of several items turned up where one was required.
    #[error("{len} items appeared where a single item was required")]
    NotSingleton {
        /// How many items there were.
        len: usize,
    },

    /// [`Composer::build`](crate::compose::Composer::build) was given
    /// something other than exactly one top-level element.
    ///
    /// Use [`Composer::build_sequence`](crate::compose::Composer::build_sequence)
    /// for a result with none, or several.
    #[error("a document needs exactly one top-level element, this build has {top_level}")]
    BuildShape {
        /// How many top-level elements the build would produce.
        top_level: usize,
    },
}

impl From<XPathError> for ComposeError {
    /// Wraps an engine error with no expression text; the evaluating call site
    /// attaches it.
    fn from(source: XPathError) -> Self {
        Self::XPath {
            source,
            expr: String::new(),
        }
    }
}

impl From<CopyError> for ComposeError {
    /// Wraps a copy error with no form path; the emitter attaches it.
    fn from(source: CopyError) -> Self {
        Self::Copy {
            source,
            at: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xpath_conversion_leaves_the_expression_text_empty() {
        let err: ComposeError = XPathError::more_than_one_item().into();
        match err {
            ComposeError::XPath { expr, .. } => assert!(expr.is_empty()),
            other => panic!("expected an XPath error, got {other:?}"),
        }
    }

    #[test]
    fn copy_conversion_leaves_the_path_empty() {
        let err: ComposeError = CopyError::Unsupported("nope").into();
        match err {
            ComposeError::Copy { at, .. } => assert!(at.is_empty()),
            other => panic!("expected a copy error, got {other:?}"),
        }
    }

    #[test]
    fn messages_name_the_offending_value() {
        assert!(ComposeError::NotSingleton { len: 2 }
            .to_string()
            .contains('2'));
        assert!(ComposeError::BuildShape { top_level: 0 }
            .to_string()
            .contains('0'));
        assert!(ComposeError::InvalidName("a b".to_string())
            .to_string()
            .contains("a b"));
        assert!(ComposeError::UnboundPrefix {
            prefix: "p".to_string(),
            at: "result/item".to_string(),
        }
        .to_string()
        .contains("result/item"));
    }
}
