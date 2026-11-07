//! Bounded XML 1.0 source construction, independent of invoice validation.
//!
//! Original bytes and decoded lexical evidence survive normalization. The SAX
//! parser remains the authority for structure, expanded names and references.
//! XML 1.1 is an explicit open capability gate. No schema or rule is executed.
#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod budget;
mod decode;
mod document;
mod error;
mod lexical;
mod parse;
mod render;

pub use budget::{QueryBudget, SourceLimits};
pub use document::{
    Attribute, Children, Declaration, DocumentOrder, Encoding, EvidenceSpan, LedgerEntry,
    LedgerKind, LedgerOwner, Name, Namespace, Namespaces, Node, NodeIdentity, NodeKind,
    OriginalScalarSpan, OriginalUtf8Span, SourceDocument, SourceLocation,
};
pub use error::{Resource, SourceError};
