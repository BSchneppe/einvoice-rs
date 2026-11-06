//! Instance validation against XSD schemas
//!
//! This module provides validation error types, a push-based `SchemaValidator`,
//! and supporting types for XML instance validation with spec-aligned error codes.
//!
//! - A [`SchemaValidator`] holds the configuration ([`ValidationFlags`]) and
//!   starts a [`ValidationRuntime`] per instance, with a [`ValidationSink`]
//!   that receives every [`ValidationError`] and [`ValidationWarning`]. Each
//!   names the item it is about: [`ValidationError::subject`] and
//!   [`ValidationError::instance_path`].
//! - The runtime's push API (`validate_element`, `validate_attribute`, …,
//!   `end_validation`) is source-agnostic. [`drive_quick_xml`] and its
//!   variants drive it from XML text, through the [`xml`](crate::xml)
//!   well-formedness layer and its [`ParseLimits`](crate::xml::ParseLimits);
//!   [`drive_navigator`] re-validates any [`DomNavigator`](crate::DomNavigator)
//!   tree. A [`ValidationEventHandler`] interleaves its own work with the
//!   events.
//! - [`ValidationRoot`] says how a run assesses its validation root: by the
//!   declaration of its name (the default), against a stipulated element
//!   declaration or type definition, or laxly (Structures 1.1 §5.2).
//! - Every event returns a [`SchemaInfo`]: the declaration, the governing
//!   type and its source ([`TypeSource`]), `[validity]`,
//!   `[validation attempted]`, the normalized and typed values, nil.

pub mod active_axis;
#[cfg(feature = "xsd11")]
pub mod alternatives;
#[cfg(feature = "xsd11")]
pub mod assertions;
pub mod asttree;
pub mod content;
pub mod context;
pub mod errors;
pub(crate) mod expression_warnings;
pub mod hint_loader;
pub mod identity;
pub mod identity_lexer;
pub mod identity_parser;
pub mod info;
pub mod navigator_driver;
pub mod quick_xml_driver;
pub mod runtime;
pub mod simple;
pub mod validator;

pub use errors::{
    error, error_with_path, facet_constraint_code, from_facet_error, from_value_error,
    value_error_constraint_code, ExpandedName, InstancePath, PathStep, ValidationError,
    ValidationResult, ValidationSubject,
};

#[cfg(feature = "xsd11")]
pub use info::{AssertionOutcome, InheritedAttribute};
pub use info::{
    ContentProcessing, ContentType, DefaultAttribute, ExpectedAttribute, ExpectedElement,
    NoNamespaceSchemaLocationHint, NodeIdentity, SchemaInfo, SchemaLocationHint, SchemaValidity,
    TypeSource, ValidationAttempted, ValidationFlags, ValidationRoot,
};

pub use content::{CompiledContentModel, ContentValidatorState, ElementMatchInfo};

pub use context::{ElementValidationState, ValidatorState};

pub use simple::{validate_simple_type, SimpleTypeResult};

pub use hint_loader::{
    enrich_schema_set, load_hints_into_builder, EnrichmentOutcome, HintLoadResult,
};
pub use identity::{KeyFieldValue, KeySequence, KeyTable};
#[cfg(feature = "xsd11")]
pub use navigator_driver::drive_buffer_document;
pub use navigator_driver::drive_navigator;
pub use quick_xml_driver::{
    drive_quick_xml, drive_quick_xml_in, drive_quick_xml_with, drive_quick_xml_with_in,
    AttributeView, DoctypeView, DriveError, DriveOutcome, DriveWithError, ElementStartView,
    EndElementInfo, EndOfAttributesView, NoopHandler, TextKind, ValidationEventHandler,
};
pub use runtime::ValidationRuntime;
pub use validator::{
    CollectingValidationSink, ErrorOnlySink, SchemaValidator, ValidationSink, ValidationWarning,
};
