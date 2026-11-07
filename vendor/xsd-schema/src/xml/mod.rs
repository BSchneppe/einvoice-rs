//! Reading XML text: the well-formedness checks and parse limits shared by
//! every reader of this crate.
//!
//! Every place where the crate reads XML text — the quick-xml validation
//! drivers ([`drive_quick_xml`](crate::validation::drive_quick_xml) and its
//! variants), the typed build (`document::build_typed_document_with`), the
//! document builder (`BufferDocument::from_reader`,
//! `BufferDocumentBuilder::build`) and the schema-document parser — goes
//! through one layer over quick-xml. A document that is not well-formed or
//! not namespace-well-formed is refused with a fatal error that names the
//! rule and the byte position ([`NotWellFormed`]); XML 1.0 §1.2 makes a
//! well-formedness violation a fatal error, so these checks are always on.
//! Every refusal of the text is a [`NotWellFormed`] — also what quick-xml
//! itself detects (mismatched end tags, unclosed markup, duplicate
//! attributes) and input that ends inside an element, which names the
//! innermost open element; an I/O error of the source is reported as the
//! source's own `std::io::Error`.
//!
//! # Versions
//!
//! A document declared `version="1.1"` is read with XML 1.1 (Second Edition)
//! and Namespaces in XML 1.1 (Second Edition): references to the restricted
//! control characters are allowed, the characters themselves are not raw, NEL
//! and LINE SEPARATOR are line ends in character data and attribute values,
//! and a prefix may be undeclared (`xmlns:p=""`). Every other document — no
//! declaration, `1.0`, another `1.x` — is read with XML 1.0 (Fifth Edition)
//! and Namespaces in XML 1.0 (Third Edition) ([`XmlVersion`]). A NEL or LINE
//! SEPARATOR used as white space inside a tag or declaration of an XML 1.1
//! document is not supported ([`WellFormednessRule::Xml11LineEndInMarkup`]).
//!
//! # What is checked
//!
//! * the document structure (\[1\] `document`, \[22\] `prolog`): one root
//!   element, nothing but white space, comments and processing instructions
//!   outside it, the XML declaration only at the very start and in the order
//!   \[23\] `XMLDecl` gives, a document type declaration only in the prolog,
//!   at most once; input that ends inside an element or markup;
//! * names: element, attribute and processing-instruction target names are
//!   XML \[5\] `Name`s and namespace-well-formed (a \[7\] `QName`, an `NCName`
//!   target);
//! * tags: white space between attributes (\[40\] `STag`, \[44\]
//!   `EmptyElemTag`), no attribute twice, no `<` in an attribute value,
//!   every `&` starting a reference to a character or to one of the five
//!   predefined entities (no reader processes a DTD);
//! * characters: only `Char`s, raw and as references of any length; well-formed
//!   UTF-8 throughout; `]]>` not in character data; no `--` in comments;
//! * the namespace constraints: every prefix bound, reserved prefixes and
//!   namespace names, prefix undeclaring (XML 1.0: not allowed), attributes
//!   unique by expanded name;
//! * the document type declaration's own grammar (\[28\] `doctypedecl`: the
//!   name, the external identifier, an internal subset whose markup is
//!   balanced). Its declarations are not processed: a reference to an entity
//!   it declares is still refused, attribute defaults are not applied.
//!
//! # Encodings
//!
//! The readers read UTF-8 (a UTF-8 byte-order mark is skipped), whatever the
//! XML declaration's `encoding` names: text decoded beforehand — by
//! `decode_xml_bytes`, say — keeps its original declaration. The declaration
//! is reported to `ValidationEventHandler::on_xml_declaration` and kept by a
//! `BufferDocument` ([`XmlDeclaration`]), where a host can refuse an encoding
//! it did not decode. Bytes that are not well-formed UTF-8 are refused
//! (XML 1.0 §4.3.3).
//!
//! # Limits
//!
//! The instance readers also take [`ParseLimits`]: bounds on depth, name and
//! namespace-URI length, attributes and namespace declarations per element,
//! namespaces in scope, attribute-value and text size, and the node count.
//! They are checked on the raw markup before anything of it is interned or
//! built, and a violation is a [`LimitExceeded`] naming the limit. The
//! default is no limits. They bound what is interned and built and the node
//! count, not the reader's memory: quick-xml reads a whole tag, text run,
//! comment or declaration into its buffer before it is checked, so bound the
//! input itself too.

mod chars;
mod error;
mod limits;
pub(crate) mod reader;

#[cfg(test)]
mod tests;

pub use error::{NotWellFormed, WellFormednessRule, XmlDeclaration, XmlVersion};
pub use limits::{LimitExceeded, ParseLimit, ParseLimits};
