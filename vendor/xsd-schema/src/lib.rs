//! XML Schema (XSD 1.0 and 1.1) validator with a post-schema-validation
//! infoset (PSVI) and a built-in XPath 2.0 engine.
//!
//! The crate parses and composes schema documents into a [`SchemaSet`],
//! validates instances through a push-based API that event-based parsers
//! (quick-xml, through [`validation::drive_quick_xml`]) and DOM-style sources
//! (any [`DomNavigator`], through [`validation::drive_navigator`]) can drive,
//! and reports the PSVI of every element and attribute. With the `xsd11`
//! feature it adds XSD 1.1 (assertions, type alternatives, open content), the
//! XPath 2.0 engine (`xpath`), a compact schema-aware document store
//! (`document`) and its typed build; `compose` adds XQuery-style composition
//! of XML from Rust.
//!
//! The guides in `doc/` of the repository walk through the API: the
//! Introduction for the entry points, the Architecture Overview, the
//! Extensibility guide, Implementation Limits, and — coming from 0.2.x —
//! What's New in 0.3 and Upgrading to 0.3.
//!
//! # Entry Points
//!
//! | Task | Start here |
//! | --- | --- |
//! | Load one schema | [`load_and_process_schema`] |
//! | Load related schemas, with loaders, a catalog or a [`LocationRewriter`] | [`SchemaSetBuilder`] |
//! | Choose XSD 1.0 or 1.1 semantics | [`SchemaSet::new`] / [`SchemaSet::xsd11`] (the `xsd11` feature only compiles the 1.1 code) |
//! | Find the directives whose documents could not be read | [`SchemaSet::directive_failures`] |
//! | Validate an instance | [`validation::SchemaValidator`], [`validation::ValidationRuntime`], a driver such as [`validation::drive_quick_xml`] |
//! | Validate against a stipulated declaration or type, or laxly | [`validation::ValidationRoot`] |
//! | Bound what an instance may contain | [`xml::ParseLimits`] |
//! | Validate and build a typed document in one pass (`xsd11`) | `document::build_typed_document_with`, `document::TypedBuildOptions` |
//! | Evaluate XPath 2.0 (`xsd11`) | `xpath::XPathExpr`, `xpath::XPathContext` |
//!
//! # Cargo Features
//!
//! Every feature is additive. This documentation is built with all of them,
//! so it shows items a default build does not have:
//!
//! | Feature | Default | Adds |
//! | --- | --- | --- |
//! | `xsd11` | no | XSD 1.1 (assertions, type alternatives, open content), the XPath 2.0 engine (`xpath`), the document store and its typed build (`document`), `drive_buffer_document` |
//! | `compose` | no | XQuery-style composition of XML from Rust (`compose`, the `form!` and `xpath!` macros); implies `xsd11` |
//! | `async` | no | asynchronous schema loading: `AsyncSchemaLoader`, `SchemaSetBuilder::with_async_loader`, `add_async` and `compile_async`, `load_and_process_schema_async`, `load_schema_async` |
//! | `unicode-normalization` | yes | the normalization forms of `fn:normalize-unicode` (without it, a call that asks for one, the one-argument form included, raises an error) |
//!
//! ## Single Schema
//!
//! Use [`load_and_process_schema`] for complete processing of a single schema.
//! XSD version is set on `SchemaSet` — the parser derives it automatically.
//!
//! ```
//! use xsd_schema::{SchemaSet, load_and_process_schema};
//!
//! // Use SchemaSet::new() for XSD 1.0, SchemaSet::xsd11() for XSD 1.1
//! let mut schema_set = SchemaSet::new();
//! let xml = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
//!     <xs:element name="root" type="xs:string"/>
//! </xs:schema>"#;
//!
//! let stats = load_and_process_schema(xml.as_bytes(), "schema.xsd", &mut schema_set, None)
//!     .expect("failed to load schema");
//! assert_eq!(stats.doc_id, 0);
//! ```
//!
//! ## Multiple Related Schemas
//!
//! [`SchemaSetBuilder`] loads documents and the documents their directives
//! name, then compiles them together. For documents you load yourself, use the
//! two-phase approach:
//!
//! ```
//! use xsd_schema::{SchemaSet, parse_schema_only, process_loaded_schemas};
//!
//! let mut schema_set = SchemaSet::new();
//!
//! let schemas = [
//!     (r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
//!                   targetNamespace="urn:schema1">
//!         <xs:element name="item1" type="xs:string"/>
//!     </xs:schema>"#, "schema1.xsd"),
//!     (r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
//!                   targetNamespace="urn:schema2">
//!         <xs:element name="item2" type="xs:int"/>
//!     </xs:schema>"#, "schema2.xsd"),
//! ];
//!
//! // Phase 1: Parse all schemas
//! for (xml, uri) in schemas {
//!     parse_schema_only(xml.as_bytes(), uri, &mut schema_set).expect("parse failed");
//! }
//!
//! // Phase 2: Process all schemas together
//! // (redefine/override application, inline assembly, reference resolution,
//! // and the same schema checks as every other entry point)
//! // Note: all participating schemas — including redefine/override targets —
//! // must be parsed before calling this function.
//! let (inline_stats, resolution_stats) = process_loaded_schemas(&mut schema_set)
//!     .expect("processing failed");
//! ```
//!
//! ## Validating an Instance
//!
//! ```
//! use xsd_schema::validation::{
//!     drive_quick_xml, CollectingValidationSink, SchemaValidator, SchemaValidity, ValidationFlags,
//! };
//! use xsd_schema::SchemaSetBuilder;
//!
//! let schema_set = SchemaSetBuilder::new()
//!     .add_source(
//!         r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
//!              <xs:element name="qty" type="xs:positiveInteger"/>
//!            </xs:schema>"#,
//!         "qty.xsd",
//!     )?
//!     .compile()?
//!     .into_schema_set();
//! let validator = SchemaValidator::new(&schema_set, ValidationFlags::default());
//!
//! let (mut errors, mut warnings) = (Vec::new(), Vec::new());
//! let mut runtime = validator.start_run(CollectingValidationSink {
//!     errors: &mut errors,
//!     warnings: &mut warnings,
//! });
//! let outcome = drive_quick_xml("<qty>0</qty>".as_bytes(), &mut runtime, &schema_set)?;
//! drop(runtime);
//! assert_eq!(outcome.root_validity, Some(SchemaValidity::Invalid));
//! assert_eq!(errors[0].instance_path().unwrap().to_string(), "/Q{}qty[1]");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! ## Advanced: Low-Level Parser
//!
//! For custom pipelines, the low-level parser is available at [`parser::parse_schema`].
//! This only performs Phase 1 (parsing + assembly) - subsequent phases must be run manually.
//!
//! # Architecture
//!
//! The parser uses a state machine approach with typed parser frames for each XSD element type.
//! All schema components are stored in arenas with typed IDs to avoid reference cycles.
//!
//! ## Core Modules
//!
//! - [`xml`] - Reading XML text: well-formedness checks and parse limits for every reader
//! - [`parser`] - XSD document parsing with location tracking, directive resolution
//! - [`namespace`] - String interning and namespace management
//! - [`schema`] - Schema component model (elements, types, groups)
//! - [`types`] - Type definitions, facets and typed values
//! - [`compiler`] - Content-model compilation and Unique Particle Attribution
//! - [`validation`] - Instance validation and the PSVI
//! - `xpath`, `document` (`xsd11`) - the XPath 2.0 engine and the document store
//! - `compose` (`compose`) - XQuery-style composition of XML

// Core type definitions
pub mod arenas;
pub mod error;
pub mod ids;

// Parser infrastructure
pub mod parser;

// Namespace management
pub mod namespace;

// Schema component model
pub mod schema;
pub mod types;

// DOM navigation (always available)
pub mod navigator;

// XPath 2.0 engine (only with xsd11 feature)
#[cfg(feature = "xsd11")]
pub mod xpath;

// Page-based XML document buffer (only with xsd11 feature)
#[cfg(feature = "xsd11")]
pub mod document;

// XQuery-style XML composition from Rust (only with the compose feature)
#[cfg(feature = "compose")]
pub mod compose;

// Pipeline orchestration
pub mod pipeline;

// Embedded assets
pub mod embedded;

// Builder pattern API
pub mod builder;

// Regex pattern conversion (shared between XSD and XPath)
pub mod regex_convert;
pub(crate) mod regex_xsd_unicode;

/// Resolution of XML general references (`&amp;`, `&#65;`) from the event stream.
pub(crate) mod xml_entity;

// Reading XML text: well-formedness checks and parse limits for every reader
pub mod xml;

// NFA compiler for content models
pub mod compiler;

// Instance validation
pub mod validation;

// Re-export primary types
pub use error::{FacetError, FacetResult, SchemaError, SchemaLimit, SchemaResult};
pub use ids::*;
pub use schema::{SchemaDocument, SchemaSet};

// Re-export resolution and inline assembly
pub use schema::{
    assemble_inline_types, resolve_all_references, InlineAssemblyStats, ResolutionStats,
};

// Re-export XSD version
pub use schema::model::XsdVersion;

// Re-export regex compatibility mode
pub use schema::model::RegexCompat;

// Re-export type system enums
pub use types::{BuiltinTypes, PrimitiveTypeCode, ValueKind, XmlTypeCode};

// Re-export facet types
pub use types::{
    facet_applicable, facet_applicable_for_type, normalize_whitespace, FacetApplicability,
    FacetFixed, FacetKind, FacetSet, WhitespaceMode,
};

/// Clear the process-global compiled-pattern regex cache (see
/// [`types::facets::clear_pattern_cache`]).
pub use types::facets::clear_pattern_cache;

// Re-export navigator types (always available)
pub use navigator::{
    DomNavigator, DomNodeType, NamespaceAxisScope, NavigatorError, RoXmlNavigator, XmlNodeOrder,
};

// Re-export XPath types (only with xsd11 feature)
#[cfg(feature = "xsd11")]
pub use xpath::{
    BufferedNodeIterator, EmptyIterator, EvalValue, ExternalVar, FunctionCallRef, RangeIterator,
    TreeComparer, TypedEvaluator, VecNodeIterator, XPathContext, XPathEvaluator, XPathExpr,
    XmlItem, XmlItemRef, XmlNodeIterator,
};

// Re-export pipeline functions
pub use pipeline::{
    load_and_process_schema, load_schema, parse_schema_only, process_loaded_schemas,
    DirectiveStats, PipelineConfig, PipelineStats,
};

// Re-export async pipeline functions
#[cfg(feature = "async")]
pub use pipeline::{load_and_process_schema_async, load_schema_async};

// Re-export builder types
pub use builder::{CompilationStats, CompiledSchemaSet, SchemaSetBuilder};

// Re-export resolver types
pub use parser::resolver::{
    decode_xml_bytes, decode_xml_to_utf8_bytes, DirectiveFailure, EmbeddedLoader, FileSystemLoader,
    LoaderChain, LocationRequest, LocationRewriter, ResolverConfig, SchemaCatalog, SchemaLoader,
    SchemaResolver,
};

// Re-export async loader trait
#[cfg(feature = "async")]
pub use parser::resolver::AsyncSchemaLoader;

// Re-export embedded assets
pub use embedded::{
    get_embedded_schema, has_embedded_schema, XLINK_NAMESPACE, XLINK_XSD, XML_NAMESPACE, XML_XSD,
};

// Re-export compiler types
pub use compiler::{
    compile_model_group, compile_particle, find_complex_type, fragment_to_table,
    inspect_content_model, CompileContext, ContentModelReport, FragmentBuilder, NfaCompileError,
    NfaCompileResult, NfaFragment, NfaState, NfaTable, NfaTerm, NfaTransition, StateId,
    TransitionKind,
};

// Re-export instance validation types
// Note: ValidationError here is distinct from types::validators::ValidationError
pub use validation::{
    error as validation_error, error_with_path as validation_error_with_path,
    facet_constraint_code, from_facet_error, from_value_error, value_error_constraint_code,
    ValidationError as InstanceValidationError, ValidationResult as InstanceValidationResult,
};

// Re-export hint-driven schema loading
pub use validation::hint_loader::{
    enrich_schema_set, load_hints_into_builder, EnrichmentOutcome, HintLoadResult,
};
pub use validation::info::{NoNamespaceSchemaLocationHint, SchemaLocationHint};

// The guides in `doc/`, compiled and run as doctests by `cargo test --doc`, so
// that their examples cannot drift from the API. `cfg(doctest)` is set only
// when rustdoc collects doctests: a normal build of the crate (a dependent's,
// docs.rs) never reads these files. A guide whose examples all need a feature
// is included only with it; the other guides guard each such example with
// `#[cfg(feature = "...")]`, so they run under the default features as well.
#[cfg(doctest)]
mod doc_guides {
    #[doc = include_str!("../doc/INTRODUCTION.md")]
    struct Introduction;
    #[doc = include_str!("../doc/OVERVIEW.md")]
    struct Overview;
    #[cfg_attr(feature = "xsd11", doc = include_str!("../doc/BUFFER_DOCUMENT_OVERVIEW.md"))]
    struct BufferDocumentOverview;
    #[doc = include_str!("../doc/EXTENSIBILITY.md")]
    struct Extensibility;
    #[doc = include_str!("../doc/IMPLEMENTATION_LIMITS.md")]
    struct ImplementationLimits;
    #[cfg_attr(feature = "compose", doc = include_str!("../doc/COMPOSE.md"))]
    struct Compose;
    #[doc = include_str!("../doc/UNSAFE.md")]
    struct Unsafe;
    #[doc = include_str!("../doc/UPGRADING_0_3.md")]
    struct Upgrading03;
    #[doc = include_str!("../doc/WHATS_NEW.md")]
    struct WhatsNew;
}
