//! Parser frames for XSD element processing
//!
//! Each XSD element type has a corresponding frame that:
//! - Validates allowed child elements
//! - Collects and validates attributes
//! - Builds schema components
//! - Handles phase transitions
//!
//! The parser uses a stack of frames to track nested elements.

use crate::error::{SchemaError, SchemaResult};
use crate::ids::{NameId, TypeKey};
use crate::namespace::context::NamespaceContextSnapshot;
use crate::namespace::{is_ncname, NameTable};
use crate::parser::attrs::{parse_boolean, parse_form, parse_occurs, parse_use, AttributeMap};
use crate::parser::location::SourceRef;
use crate::schema::annotation::{
    merge_foreign_attributes, Annotation, AnnotationItem, AppInfoElement, DocumentationElement,
    ForeignAttribute, XmlFragment,
};
use crate::schema::model::DerivationSet;
use crate::types::facets::{ExplicitTimezone, FacetSet};

include!("frames/xsd_names.rs");
include!("frames/core.rs");
include!("frames/schema.rs");
include!("frames/types.rs");
include!("frames/elements.rs");
#[cfg(feature = "xsd11")]
include!("frames/xsd11.rs");
include!("frames/notation.rs");
#[cfg(feature = "xsd11")]
include!("frames/open_content.rs");
include!("frames/groups.rs");
include!("frames/wildcards.rs");
include!("frames/annotations.rs");
include!("frames/facets.rs");
include!("frames/identity.rs");
include!("frames/directives.rs");
include!("frames/skip.rs");
include!("frames/helpers.rs");
include!("frames/factory.rs");
include!("frames/tests.rs");
