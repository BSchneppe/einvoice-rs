//! XSD document parsing
//!
//! This module contains the XSD parser implementation using quick-xml
//! with location tracking for accurate error reporting.
//!
//! ## Module Structure
//!
//! - `location` - Source location tracking (spans, line/column mapping)
//! - `reader` - Tracked XML reader wrapping quick-xml
//! - `attrs` - Attribute parsing and validation
//! - `frames` - Parser state machine frames
//! - `parse` - Main parser event loop
//! - `structure` - Structural validation rules
//! - `resolver` - Schema resolution (include/import/redefine)

pub mod assemble;
pub mod attrs;
pub mod frames;
pub mod location;
pub mod parse;
pub mod reader;
pub mod resolver;
pub mod structure;
pub(crate) mod uri;

// Re-exports from location
pub use location::{SourceLocation, SourceMap, SourceRef, SourceRetention, SourceSpan};

// Re-exports from reader
pub use reader::{split_qname, ReaderConfig, TrackedEvent, TrackedReader};

// Re-exports from attrs
pub use attrs::{
    categorize_attributes, parse_attributes, parse_boolean, parse_form, parse_occurs,
    parse_process_contents, parse_use, AttributeMap, ParsedAttribute,
};

// Re-exports from parse
//
// Note: `parse_schema` and `parse_schema_with_config` are low-level APIs that only perform
// Phase 1 (parsing + assembly). For typical use cases, prefer:
// - `crate::load_and_process_schema` for single schemas with full processing
// - `crate::parse_schema_only` + `crate::process_loaded_schemas` for multiple schemas
pub use parse::{parse_schema, parse_schema_with_config, ParserConfig, DEFAULT_MAX_DEPTH};

// Re-exports from assemble
pub use assemble::{assemble_schema, parse_form_choice};

// Re-exports from structure
pub use structure::{
    validate_attribute_group_structure, validate_attribute_structure,
    validate_complex_type_structure, validate_element_structure, validate_extension_structure,
    validate_group_structure, validate_import_structure, validate_include_structure,
    validate_key_unique_structure, validate_keyref_structure, validate_list_structure,
    validate_notation_structure, validate_redefine_structure, validate_restriction_structure,
    validate_simple_type_structure, validate_union_structure, validate_xsd_version_attribute,
    validate_xsd_version_element, ValidationContext,
};

// Re-exports from resolver
pub use resolver::{
    fixup_composition_edges,
    resolve_all_directives,
    CatalogEntry,
    DirectiveFailure,
    EmbeddedLoader,
    FileSystemLoader,
    LoaderChain,
    LocationRequest,
    LocationRewriter,
    ResolutionResult,
    ResolverConfig,
    SchemaCatalog,
    // Loader trait and implementations
    SchemaLoader,
    SchemaResolver,
};
