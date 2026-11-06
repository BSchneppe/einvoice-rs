//! Open content schema-level validation helpers (XSD 1.1)
//!
//! This module provides helpers for validating that open content declarations
//! in the schema are well-formed (schema-level checks). Runtime instance
//! validation of open content (matching additional elements against the
//! wildcard) is implemented in `crate::validation::content`.

use crate::error::{SchemaError, SchemaResult};
use crate::schema::model::{DefaultOpenContent, OpenContentMode};
use crate::SchemaSet;

/// Validate a default open content declaration (cos-valid-default-oc, §3.4.6.5).
///
/// Checks:
/// 1. Mode must be `interleave` or `suffix` (not `none`)
/// 2. Wildcard must be present when mode is `interleave` or `suffix`
fn validate_default_open_content(
    schema_set: &SchemaSet,
    default_oc: &DefaultOpenContent,
) -> SchemaResult<()> {
    let location = default_oc
        .source
        .as_ref()
        .and_then(|s| schema_set.source_maps.locate(s));

    // 1. Mode must be interleave or suffix (not none)
    if default_oc.mode == OpenContentMode::None {
        return Err(SchemaError::structural(
            "cos-valid-default-oc",
            "defaultOpenContent mode must be 'interleave' or 'suffix'",
            location,
        ));
    }

    // 2. Wildcard must be present when mode is interleave or suffix
    if default_oc.wildcard.is_none() {
        return Err(SchemaError::structural(
            "cos-valid-default-oc",
            "defaultOpenContent requires a wildcard (xs:any) child element",
            location,
        ));
    }

    Ok(())
}

/// Validate all default open content declarations across all schema documents.
pub fn validate_all_default_open_content(schema_set: &SchemaSet) -> SchemaResult<()> {
    for doc in &schema_set.documents {
        if let Some(ref default_oc) = doc.default_open_content {
            validate_default_open_content(schema_set, default_oc)?;
        }
    }
    Ok(())
}
