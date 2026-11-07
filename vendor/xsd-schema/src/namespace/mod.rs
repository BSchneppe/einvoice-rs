//! Namespace management and string interning
//!
//! This module provides:
//! - `NameTable` - String interning for names and namespace URIs
//! - `NamespaceContext` - Scoped prefix-to-namespace mappings
//! - `QualifiedName` - Parsed QName with resolved namespace
//!
//! All strings in the schema model pass through `NameTable` for deduplication
//! and fast equality checks via `NameId`.

pub mod context;
pub mod qname;
pub mod table;

// Re-exports
pub use context::{NamespaceContext, NamespaceContextSnapshot, NamespaceScope};
pub use qname::{is_ncname, parse_qname, parse_qname_with_snapshot, QNameError, QualifiedName};
pub use table::{
    well_known, NameTable, XMLNS_NAMESPACE, XML_NAMESPACE, XSI_NAMESPACE, XS_NAMESPACE,
};
