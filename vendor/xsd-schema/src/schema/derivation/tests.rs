//! Unit tests for type-derivation validation.
//!
//! This module holds the single inline `mod tests` block that the former
//! `derivation.rs` carried; its cases span several of the sibling modules.

use super::attributes::*;
use super::complex::*;
use super::normalize::*;
use super::particle::*;
use super::redefine::*;
use super::simple::*;
#[cfg(feature = "xsd11")]
use super::wildcard::*;
use super::*;
use crate::arenas::{ComplexTypeDefData, SimpleTypeDefData};
use crate::error::SchemaError;
use crate::ids::{NameId, TypeKey};
use crate::parser::frames::{
    ComplexContentResult, Compositor, DerivationMethod, ProcessContents, SimpleTypeVariety,
    WildcardNamespace, WildcardResult,
};
use crate::schema::dependencies::DependencyGraph;
use crate::schema::model::DerivationSet;
use crate::schema::SchemaSet;
use crate::types::facets::FacetSet;

fn create_simple_type_data(name: Option<NameId>, variety: SimpleTypeVariety) -> SimpleTypeDefData {
    SimpleTypeDefData {
        name,
        target_namespace: None,
        variety,
        base_type: None,
        item_type: None,
        member_types: Vec::new(),
        facets: FacetSet::new(),
        final_derivation: DerivationSet::empty(),
        id: None,
        derivation_id: None,
        annotation: None,
        source: None,
        resolved_base_type: None,
        resolved_item_type: None,
        resolved_member_types: Vec::new(),
        redefine_original: None,
        deferred_item_type_error: None,
    }
}

fn create_complex_type_data(name: Option<NameId>) -> ComplexTypeDefData {
    ComplexTypeDefData {
        name,
        target_namespace: None,
        base_type: None,
        derivation_method: None,
        content: ComplexContentResult::Empty,
        open_content: None,
        attributes: Vec::new(),
        attribute_groups: Vec::new(),
        attribute_wildcard: None,
        mixed: false,
        is_abstract: false,
        final_derivation: DerivationSet::empty(),
        block: DerivationSet::empty(),
        default_attributes_apply: true,
        id: None,
        #[cfg(feature = "xsd11")]
        assertions: Vec::new(),
        #[cfg(feature = "xsd11")]
        xpath_default_namespace: None,
        annotation: None,
        source: None,
        resolved_base_type: None,
        resolved_attribute_groups: Vec::new(),
        resolved_attributes: Vec::new(),
        resolved_content_particle_types: Vec::new(),
        resolved_content_particle_elements: Vec::new(),
        resolved_simple_content_type: None,
        redefine_original: None,
    }
}

#[test]
fn test_derivation_stats_default() {
    let stats = DerivationStats::default();
    assert_eq!(stats.simple_types_validated, 0);
    assert_eq!(stats.complex_types_validated, 0);
    assert_eq!(stats.errors, 0);
}

#[test]
fn test_validate_empty_schema() {
    let schema_set = SchemaSet::new();
    let dep_graph = DependencyGraph::new();

    let result = validate_all_derivations(&schema_set, &dep_graph);
    assert!(result.is_ok());

    let stats = result.unwrap();
    assert_eq!(stats.simple_types_validated, 0);
    assert_eq!(stats.complex_types_validated, 0);
}

#[test]
fn test_validate_atomic_type_no_base() {
    let mut schema_set = SchemaSet::new();
    let type_data = create_simple_type_data(None, SimpleTypeVariety::Atomic);
    let key = schema_set.arenas.alloc_simple_type(type_data);

    let mut stats = DerivationStats::default();
    let result = validate_simple_type(&schema_set, key, &mut stats);

    assert!(result.is_ok());
    assert_eq!(stats.simple_types_validated, 1);
}

#[test]
fn test_validate_list_type_no_item() {
    let mut schema_set = SchemaSet::new();
    let type_data = create_simple_type_data(None, SimpleTypeVariety::List);
    let key = schema_set.arenas.alloc_simple_type(type_data);

    let mut stats = DerivationStats::default();
    let result = validate_simple_type(&schema_set, key, &mut stats);

    assert!(result.is_ok());
    assert_eq!(stats.list_types_validated, 1);
}

#[test]
fn test_validate_union_type_no_members() {
    let mut schema_set = SchemaSet::new();
    let type_data = create_simple_type_data(None, SimpleTypeVariety::Union);
    let key = schema_set.arenas.alloc_simple_type(type_data);

    let mut stats = DerivationStats::default();
    let result = validate_simple_type(&schema_set, key, &mut stats);

    assert!(result.is_ok());
    assert_eq!(stats.union_types_validated, 1);
}

#[test]
fn test_validate_list_of_atomic() {
    let mut schema_set = SchemaSet::new();

    // Create an atomic item type
    let item_type_data = create_simple_type_data(None, SimpleTypeVariety::Atomic);
    let item_key = schema_set.arenas.alloc_simple_type(item_type_data);

    // Create a list type with atomic item type
    let mut list_type_data = create_simple_type_data(None, SimpleTypeVariety::List);
    list_type_data.resolved_item_type = Some(TypeKey::Simple(item_key));
    let list_key = schema_set.arenas.alloc_simple_type(list_type_data);

    let mut stats = DerivationStats::default();
    let result = validate_simple_type(&schema_set, list_key, &mut stats);

    assert!(result.is_ok());
}

#[test]
fn test_validate_list_of_list_error() {
    let mut schema_set = SchemaSet::new();

    // Create a list item type (invalid)
    let inner_list_data = create_simple_type_data(None, SimpleTypeVariety::List);
    let inner_key = schema_set.arenas.alloc_simple_type(inner_list_data);

    // Create a list type with list item type (should fail)
    let mut outer_list_data = create_simple_type_data(None, SimpleTypeVariety::List);
    outer_list_data.resolved_item_type = Some(TypeKey::Simple(inner_key));
    let outer_key = schema_set.arenas.alloc_simple_type(outer_list_data);

    let mut stats = DerivationStats::default();
    let result = validate_simple_type(&schema_set, outer_key, &mut stats);

    assert!(result.is_err());
    if let Err(SchemaError::StructuralError { constraint, .. }) = result {
        assert_eq!(constraint, "cos-list-of-atomic");
    } else {
        panic!("Expected structural error with cos-list-of-atomic constraint");
    }
}

#[test]
fn test_validate_union_with_complex_member_error() {
    let mut schema_set = SchemaSet::new();

    // Create a complex type (invalid for union member)
    let complex_data = create_complex_type_data(None);
    let complex_key = schema_set.arenas.alloc_complex_type(complex_data);

    // Create a union type with complex member (should fail)
    let mut union_data = create_simple_type_data(None, SimpleTypeVariety::Union);
    union_data.resolved_member_types = vec![TypeKey::Complex(complex_key)];
    let union_key = schema_set.arenas.alloc_simple_type(union_data);

    let mut stats = DerivationStats::default();
    let result = validate_simple_type(&schema_set, union_key, &mut stats);

    assert!(result.is_err());
    if let Err(SchemaError::StructuralError { constraint, .. }) = result {
        assert_eq!(constraint, "cos-union-memberTypes");
    } else {
        panic!("Expected structural error with cos-union-memberTypes constraint");
    }
}

#[test]
fn test_validate_complex_type_no_base() {
    let mut schema_set = SchemaSet::new();
    let type_data = create_complex_type_data(None);
    let key = schema_set.arenas.alloc_complex_type(type_data);

    let mut stats = DerivationStats::default();
    let result = validate_complex_type(&schema_set, key, &mut stats);

    assert!(result.is_ok());
    assert_eq!(stats.complex_types_validated, 1);
}

#[test]
fn test_validate_complex_extension() {
    let mut schema_set = SchemaSet::new();

    // Create base complex type
    let base_data = create_complex_type_data(None);
    let base_key = schema_set.arenas.alloc_complex_type(base_data);

    // Create derived type with extension
    let mut derived_data = create_complex_type_data(None);
    derived_data.derivation_method = Some(DerivationMethod::Extension);
    derived_data.resolved_base_type = Some(TypeKey::Complex(base_key));
    let derived_key = schema_set.arenas.alloc_complex_type(derived_data);

    let mut stats = DerivationStats::default();
    let result = validate_complex_type(&schema_set, derived_key, &mut stats);

    assert!(result.is_ok());
    assert_eq!(stats.extensions_validated, 1);
}

#[test]
fn test_validate_complex_restriction() {
    let mut schema_set = SchemaSet::new();

    // Create base complex type
    let base_data = create_complex_type_data(None);
    let base_key = schema_set.arenas.alloc_complex_type(base_data);

    // Create derived type with restriction
    let mut derived_data = create_complex_type_data(None);
    derived_data.derivation_method = Some(DerivationMethod::Restriction);
    derived_data.resolved_base_type = Some(TypeKey::Complex(base_key));
    let derived_key = schema_set.arenas.alloc_complex_type(derived_data);

    let mut stats = DerivationStats::default();
    let result = validate_complex_type(&schema_set, derived_key, &mut stats);

    assert!(result.is_ok());
    assert_eq!(stats.restrictions_validated, 1);
}

#[test]
fn test_validate_extension_of_final_type_error() {
    let mut schema_set = SchemaSet::new();

    // Create base complex type with final="extension"
    let mut base_data = create_complex_type_data(None);
    base_data.final_derivation = DerivationSet::extension();
    let base_key = schema_set.arenas.alloc_complex_type(base_data);

    // Create derived type with extension (should fail)
    let mut derived_data = create_complex_type_data(None);
    derived_data.derivation_method = Some(DerivationMethod::Extension);
    derived_data.resolved_base_type = Some(TypeKey::Complex(base_key));
    let derived_key = schema_set.arenas.alloc_complex_type(derived_data);

    let mut stats = DerivationStats::default();
    let result = validate_complex_type(&schema_set, derived_key, &mut stats);

    assert!(result.is_err());
    if let Err(SchemaError::StructuralError { constraint, .. }) = result {
        assert_eq!(constraint, "cos-ct-extends");
    } else {
        panic!("Expected structural error with cos-ct-extends constraint");
    }
}

#[test]
fn test_validate_extension_of_final_default_type_error() {
    // Assembly would apply finalDefault to types without an explicit final.
    // This test simulates that: base.final_derivation = extension (inherited from finalDefault).
    let mut schema_set = SchemaSet::new();

    let mut base_data = create_complex_type_data(None);
    base_data.final_derivation = DerivationSet::extension();
    let base_key = schema_set.arenas.alloc_complex_type(base_data);

    // Create derived type with extension (should fail).
    let mut derived_data = create_complex_type_data(None);
    derived_data.derivation_method = Some(DerivationMethod::Extension);
    derived_data.resolved_base_type = Some(TypeKey::Complex(base_key));
    let derived_key = schema_set.arenas.alloc_complex_type(derived_data);

    let mut stats = DerivationStats::default();
    let result = validate_complex_type(&schema_set, derived_key, &mut stats);

    assert!(result.is_err());
    if let Err(SchemaError::StructuralError { constraint, .. }) = result {
        assert_eq!(constraint, "cos-ct-extends");
    } else {
        panic!("Expected structural error with cos-ct-extends constraint");
    }
}

#[test]
fn test_validate_restriction_of_final_type_error() {
    let mut schema_set = SchemaSet::new();

    // Create base complex type with final="restriction"
    let mut base_data = create_complex_type_data(None);
    base_data.final_derivation = DerivationSet::restriction();
    let base_key = schema_set.arenas.alloc_complex_type(base_data);

    // Create derived type with restriction (should fail)
    let mut derived_data = create_complex_type_data(None);
    derived_data.derivation_method = Some(DerivationMethod::Restriction);
    derived_data.resolved_base_type = Some(TypeKey::Complex(base_key));
    let derived_key = schema_set.arenas.alloc_complex_type(derived_data);

    let mut stats = DerivationStats::default();
    let result = validate_complex_type(&schema_set, derived_key, &mut stats);

    assert!(result.is_err());
    if let Err(SchemaError::StructuralError { constraint, .. }) = result {
        assert_eq!(constraint, "derivation-ok-restriction");
    } else {
        panic!("Expected structural error with derivation-ok-restriction constraint");
    }
}

#[test]
fn test_format_type_name_anonymous() {
    let schema_set = SchemaSet::new();
    let name = format_type_name(&schema_set, None, None);
    assert_eq!(name, "(anonymous)");
}

#[test]
fn test_format_type_name_with_namespace() {
    let schema_set = SchemaSet::new();
    let name_id = schema_set.name_table.add("myType");
    let ns_id = schema_set.name_table.add("http://example.com");
    let name = format_type_name(&schema_set, Some(name_id), Some(ns_id));
    assert_eq!(name, "{http://example.com}myType");
}

#[test]
fn test_format_type_name_no_namespace() {
    let schema_set = SchemaSet::new();
    let name_id = schema_set.name_table.add("myType");
    let name = format_type_name(&schema_set, Some(name_id), None);
    assert_eq!(name, "myType");
}

// ====================================================================
// XSD 1.1: Open-content derivation tests
// ====================================================================

#[cfg(feature = "xsd11")]
fn make_open_content(
    mode: crate::parser::frames::OpenContentMode,
    namespace: crate::parser::frames::WildcardNamespace,
    pc: crate::parser::frames::ProcessContents,
) -> crate::parser::frames::OpenContentResult {
    crate::parser::frames::OpenContentResult {
        mode,
        wildcard: Some(crate::parser::frames::WildcardResult {
            namespace,
            process_contents: pc,
            not_namespace: Vec::new(),
            not_qname: Vec::new(),
            id: None,
            annotation: None,
            source: None,
        }),
        id: None,
        annotation: None,
        source: None,
    }
}

#[cfg(feature = "xsd11")]
#[test]
fn test_extension_suffix_cannot_extend_interleave() {
    use crate::parser::frames::{OpenContentMode, ProcessContents, WildcardNamespace};

    let mut schema_set = SchemaSet::new();

    let mut base_data = create_complex_type_data(None);
    base_data.open_content = Some(make_open_content(
        OpenContentMode::Interleave,
        WildcardNamespace::Any,
        ProcessContents::Lax,
    ));
    let base_key = schema_set.arenas.alloc_complex_type(base_data);

    let mut derived_data = create_complex_type_data(None);
    derived_data.derivation_method = Some(DerivationMethod::Extension);
    derived_data.resolved_base_type = Some(TypeKey::Complex(base_key));
    derived_data.open_content = Some(make_open_content(
        OpenContentMode::Suffix,
        WildcardNamespace::Any,
        ProcessContents::Lax,
    ));
    let derived_key = schema_set.arenas.alloc_complex_type(derived_data);

    let mut stats = DerivationStats::default();
    let result = validate_complex_type(&schema_set, derived_key, &mut stats);

    assert!(result.is_err());
    if let Err(SchemaError::StructuralError { constraint, .. }) = result {
        assert_eq!(constraint, "cos-ct-extends");
    } else {
        panic!("Expected cos-ct-extends error");
    }
}

#[cfg(feature = "xsd11")]
#[test]
fn test_extension_interleave_extends_interleave_valid() {
    use crate::parser::frames::{OpenContentMode, ProcessContents, WildcardNamespace};

    let mut schema_set = SchemaSet::new();

    let mut base_data = create_complex_type_data(None);
    base_data.open_content = Some(make_open_content(
        OpenContentMode::Interleave,
        WildcardNamespace::Any,
        ProcessContents::Lax,
    ));
    let base_key = schema_set.arenas.alloc_complex_type(base_data);

    let mut derived_data = create_complex_type_data(None);
    derived_data.derivation_method = Some(DerivationMethod::Extension);
    derived_data.resolved_base_type = Some(TypeKey::Complex(base_key));
    derived_data.open_content = Some(make_open_content(
        OpenContentMode::Interleave,
        WildcardNamespace::Any,
        ProcessContents::Lax,
    ));
    let derived_key = schema_set.arenas.alloc_complex_type(derived_data);

    let mut stats = DerivationStats::default();
    let result = validate_complex_type(&schema_set, derived_key, &mut stats);
    assert!(result.is_ok());
}

#[cfg(feature = "xsd11")]
#[test]
fn test_extension_base_has_oc_derived_has_none() {
    use crate::parser::frames::{OpenContentMode, ProcessContents, WildcardNamespace};

    // Per §3.4.2.3 clause 6.1 and §3.4.6.2 clause 1.4.3.2.2:
    // when the derivation declares no <xs:openContent>, the effective
    // {open content} of the derived type (EOT) inherits the base's
    // (BOT).  That trivially satisfies clauses 1.4.3.2.2.3 and
    // 1.4.3.2.2.4, so extension is valid.  (saxonData/Open/open027.)
    let mut schema_set = SchemaSet::new();

    let mut base_data = create_complex_type_data(None);
    base_data.open_content = Some(make_open_content(
        OpenContentMode::Interleave,
        WildcardNamespace::Any,
        ProcessContents::Lax,
    ));
    let base_key = schema_set.arenas.alloc_complex_type(base_data);

    let mut derived_data = create_complex_type_data(None);
    derived_data.derivation_method = Some(DerivationMethod::Extension);
    derived_data.resolved_base_type = Some(TypeKey::Complex(base_key));
    // No open_content on derived — inherits from base per clause 6.1.
    let derived_key = schema_set.arenas.alloc_complex_type(derived_data);

    let mut stats = DerivationStats::default();
    let result = validate_complex_type(&schema_set, derived_key, &mut stats);

    assert!(
        result.is_ok(),
        "derived inherits BOT per clause 6.1: {:?}",
        result
    );
}

#[cfg(feature = "xsd11")]
#[test]
fn test_extension_base_no_oc_derived_adds_oc_valid() {
    use crate::parser::frames::{OpenContentMode, ProcessContents, WildcardNamespace};

    let mut schema_set = SchemaSet::new();

    // Base has no open content
    let base_data = create_complex_type_data(None);
    let base_key = schema_set.arenas.alloc_complex_type(base_data);

    let mut derived_data = create_complex_type_data(None);
    derived_data.derivation_method = Some(DerivationMethod::Extension);
    derived_data.resolved_base_type = Some(TypeKey::Complex(base_key));
    derived_data.open_content = Some(make_open_content(
        OpenContentMode::Interleave,
        WildcardNamespace::Any,
        ProcessContents::Lax,
    ));
    let derived_key = schema_set.arenas.alloc_complex_type(derived_data);

    let mut stats = DerivationStats::default();
    let result = validate_complex_type(&schema_set, derived_key, &mut stats);
    assert!(result.is_ok());
}

#[cfg(feature = "xsd11")]
#[test]
fn test_restriction_adds_oc_when_base_has_none() {
    use crate::parser::frames::{OpenContentMode, ProcessContents, WildcardNamespace};

    let mut schema_set = SchemaSet::new();

    // Base has no open content
    let base_data = create_complex_type_data(None);
    let base_key = schema_set.arenas.alloc_complex_type(base_data);

    let mut derived_data = create_complex_type_data(None);
    derived_data.derivation_method = Some(DerivationMethod::Restriction);
    derived_data.resolved_base_type = Some(TypeKey::Complex(base_key));
    derived_data.open_content = Some(make_open_content(
        OpenContentMode::Interleave,
        WildcardNamespace::Any,
        ProcessContents::Lax,
    ));
    let derived_key = schema_set.arenas.alloc_complex_type(derived_data);

    let mut stats = DerivationStats::default();
    let result = validate_complex_type(&schema_set, derived_key, &mut stats);

    assert!(result.is_err());
    if let Err(SchemaError::StructuralError { constraint, .. }) = result {
        assert_eq!(constraint, "derivation-ok-restriction");
    } else {
        panic!("Expected derivation-ok-restriction error");
    }
}

#[cfg(feature = "xsd11")]
#[test]
fn test_restriction_removes_oc_valid() {
    use crate::parser::frames::{OpenContentMode, ProcessContents, WildcardNamespace};

    let mut schema_set = SchemaSet::new();

    let mut base_data = create_complex_type_data(None);
    base_data.open_content = Some(make_open_content(
        OpenContentMode::Interleave,
        WildcardNamespace::Any,
        ProcessContents::Lax,
    ));
    let base_key = schema_set.arenas.alloc_complex_type(base_data);

    let mut derived_data = create_complex_type_data(None);
    derived_data.derivation_method = Some(DerivationMethod::Restriction);
    derived_data.resolved_base_type = Some(TypeKey::Complex(base_key));
    // No open_content — restriction removes it
    let derived_key = schema_set.arenas.alloc_complex_type(derived_data);

    let mut stats = DerivationStats::default();
    let result = validate_complex_type(&schema_set, derived_key, &mut stats);
    assert!(result.is_ok());
}

#[cfg(feature = "xsd11")]
#[test]
fn test_restriction_empty_derived_allows_interleave_over_suffix() {
    // Per §3.4.6.4 (language containment), an empty derived particle
    // emits only wildcard content, so the OC mode choice is irrelevant —
    // interleave and suffix accept the same empty-particle language.
    // Mirrors W3C saxonData/Open/open020/open021 which expect VALID.
    use crate::parser::frames::{OpenContentMode, ProcessContents, WildcardNamespace};

    let mut schema_set = SchemaSet::new();

    let mut base_data = create_complex_type_data(None);
    base_data.open_content = Some(make_open_content(
        OpenContentMode::Suffix,
        WildcardNamespace::Any,
        ProcessContents::Lax,
    ));
    let base_key = schema_set.arenas.alloc_complex_type(base_data);

    let mut derived_data = create_complex_type_data(None);
    derived_data.derivation_method = Some(DerivationMethod::Restriction);
    derived_data.resolved_base_type = Some(TypeKey::Complex(base_key));
    derived_data.open_content = Some(make_open_content(
        OpenContentMode::Interleave,
        WildcardNamespace::Any,
        ProcessContents::Lax,
    ));
    let derived_key = schema_set.arenas.alloc_complex_type(derived_data);

    let mut stats = DerivationStats::default();
    let result = validate_complex_type(&schema_set, derived_key, &mut stats);
    assert!(
        result.is_ok(),
        "empty derived content should accept interleave over suffix, got {:?}",
        result.err(),
    );
}

#[cfg(feature = "xsd11")]
#[test]
fn test_restriction_suffix_restricts_interleave_valid() {
    use crate::parser::frames::{OpenContentMode, ProcessContents, WildcardNamespace};

    let mut schema_set = SchemaSet::new();

    let mut base_data = create_complex_type_data(None);
    base_data.open_content = Some(make_open_content(
        OpenContentMode::Interleave,
        WildcardNamespace::Any,
        ProcessContents::Lax,
    ));
    let base_key = schema_set.arenas.alloc_complex_type(base_data);

    let mut derived_data = create_complex_type_data(None);
    derived_data.derivation_method = Some(DerivationMethod::Restriction);
    derived_data.resolved_base_type = Some(TypeKey::Complex(base_key));
    derived_data.open_content = Some(make_open_content(
        OpenContentMode::Suffix,
        WildcardNamespace::Any,
        ProcessContents::Lax,
    ));
    let derived_key = schema_set.arenas.alloc_complex_type(derived_data);

    let mut stats = DerivationStats::default();
    let result = validate_complex_type(&schema_set, derived_key, &mut stats);
    assert!(result.is_ok());
}

// ====================================================================
// cos-ns-subset: ##other exclusion-set tests (§3.10.1, §3.10.6.2)
//
// ##other maps to not({target namespace}, absent), so it always
// excludes both the target namespace AND absent.
// ====================================================================

#[cfg(feature = "xsd11")]
#[test]
fn test_ns_subset_local_not_subset_of_other() {
    // Base ##other with target ns urn:a excludes {Some(urn:a), None}.
    // Derived ##local allows {None}.
    // None is in base's exclusion set → NOT a subset.
    use crate::parser::frames::WildcardNamespace;

    let schema_set = SchemaSet::new();
    let urn_a = schema_set.name_table.add("urn:a");

    let result = is_namespace_subset(
        &WildcardNamespace::Local,
        None,
        &WildcardNamespace::Other,
        Some(urn_a),
    );
    assert!(
        !result,
        "##local must NOT be a subset of ##other (absent is excluded)"
    );
}

#[cfg(feature = "xsd11")]
#[test]
fn test_ns_subset_other_no_tns_not_subset_of_other_with_tns() {
    // Base ##other with tns=urn:a excludes {Some(urn:a), None}.
    // Derived ##other with tns=None excludes {None}.
    // Derived still allows urn:a, which base excludes → NOT a subset.
    use crate::parser::frames::WildcardNamespace;

    let schema_set = SchemaSet::new();
    let urn_a = schema_set.name_table.add("urn:a");

    let result = is_namespace_subset(
        &WildcardNamespace::Other,
        None,
        &WildcardNamespace::Other,
        Some(urn_a),
    );
    assert!(
        !result,
        "##other(tns=None) must NOT be a subset of ##other(tns=urn:a)"
    );
}

#[cfg(feature = "xsd11")]
#[test]
fn test_ns_subset_other_with_tns_is_subset_of_other_no_tns() {
    // Base ##other with tns=None excludes {None}.
    // Derived ##other with tns=urn:a excludes {Some(urn:a), None}.
    // Derived excludes a superset → IS a subset.
    use crate::parser::frames::WildcardNamespace;

    let schema_set = SchemaSet::new();
    let urn_a = schema_set.name_table.add("urn:a");

    let result = is_namespace_subset(
        &WildcardNamespace::Other,
        Some(urn_a),
        &WildcardNamespace::Other,
        None,
    );
    assert!(
        result,
        "##other(tns=urn:a) MUST be a subset of ##other(tns=None)"
    );
}

#[cfg(feature = "xsd11")]
#[test]
fn test_ns_subset_list_with_tns_uri_not_subset_of_other() {
    // Base ##other with tns=urn:a excludes {Some(urn:a), None}.
    // Derived list contains explicit urn:a URI.
    // urn:a is in base's exclusion set → NOT a subset.
    use crate::parser::frames::{NamespaceToken, WildcardNamespace};

    let schema_set = SchemaSet::new();
    let urn_a = schema_set.name_table.add("urn:a");
    let urn_b = schema_set.name_table.add("urn:b");

    let result = is_namespace_subset(
        &WildcardNamespace::List(vec![NamespaceToken::Uri(urn_a), NamespaceToken::Uri(urn_b)]),
        None,
        &WildcardNamespace::Other,
        Some(urn_a),
    );
    assert!(
        !result,
        "List containing base's target ns must NOT be a subset of ##other"
    );
}

// -----------------------------------------------------------------
// §src-redefine 6.2.2 / 7.2.2 — focused pin tests
//
// Broad end-to-end rejection coverage (schR5/attgC028/mgO013) and
// positive-coverage guards (annotA019, attgC017, schH1, schU1, …)
// are already exercised by the W3C conformance suite. The tests
// below pin the subtle invariants that are NOT directly covered by
// conformance: (a) the `wildcard_allows_attribute` helper's
// `##defined` correctness — which is the whole reason the helper
// exists, and (b) the `all{required_e1}` vs `all{}` particle shape
// that `mgO013` ultimately relies on.
// -----------------------------------------------------------------

fn default_wildcard(ns: WildcardNamespace) -> WildcardResult {
    WildcardResult {
        namespace: ns,
        process_contents: ProcessContents::Strict,
        not_namespace: Vec::new(),
        not_qname: Vec::new(),
        id: None,
        annotation: None,
        source: None,
    }
}

/// Normalize `w` against `target_ns` and ask whether the resulting
/// effective wildcard admits `(attr_ns, attr_name)`. Used by the
/// spec-invariant pin tests below so they exercise the production
/// canonical-form path.
fn admits(
    schema_set: &SchemaSet,
    w: &WildcardResult,
    target_ns: Option<NameId>,
    attr_ns: Option<NameId>,
    attr_name: NameId,
) -> bool {
    let eff = normalize_attribute_wildcard(w, target_ns);
    effective_wildcard_allows_attribute(schema_set, &eff, attr_ns, attr_name)
}

#[test]
fn test_effective_wildcard_any_admits_anything() {
    let schema_set = SchemaSet::new();
    let name = schema_set.name_table.add("foo");
    let w = default_wildcard(WildcardNamespace::Any);
    assert!(admits(&schema_set, &w, None, None, name));
}

#[test]
fn test_effective_wildcard_other_excludes_target_ns() {
    // ##other must exclude the target namespace itself.
    let schema_set = SchemaSet::new();
    let ns = schema_set.name_table.add("urn:foo");
    let name = schema_set.name_table.add("bar");
    let w = default_wildcard(WildcardNamespace::Other);
    assert!(
        !admits(&schema_set, &w, Some(ns), Some(ns), name),
        "##other must NOT admit the target namespace"
    );
}

#[test]
fn test_effective_wildcard_other_admits_different_ns() {
    let schema_set = SchemaSet::new();
    let tns = schema_set.name_table.add("urn:foo");
    let other_ns = schema_set.name_table.add("urn:bar");
    let name = schema_set.name_table.add("qux");
    let w = default_wildcard(WildcardNamespace::Other);
    assert!(
        admits(&schema_set, &w, Some(tns), Some(other_ns), name),
        "##other must admit a namespace different from the target"
    );
}

#[test]
fn test_effective_wildcard_other_absent_ns_xsd10_and_xsd11() {
    // §3.10.2.2 / §3.10.4.3 clause 2: `##other` is not({target namespace,
    // absent}) in XSD 1.0 and XSD 1.1 alike, so the absent namespace is
    // never admitted.
    for schema_set in [SchemaSet::new(), SchemaSet::xsd11()] {
        let tns = schema_set.name_table.add("urn:foo");
        let name = schema_set.name_table.add("local_attr");
        let w = default_wildcard(WildcardNamespace::Other);
        assert!(
            !admits(&schema_set, &w, Some(tns), None, name),
            "xsd11={}: ##other must NOT admit the absent namespace",
            schema_set.is_xsd11()
        );
    }
}

#[test]
fn test_effective_wildcard_defined_excludes_declared_only() {
    // §3.10.4 `##defined` excludes ONLY attributes that are globally
    // declared — not all attributes unconditionally.
    use crate::arenas::AttributeDeclData;
    use crate::parser::frames::NotQNameItem;

    let mut schema_set = SchemaSet::new();
    let declared_name = schema_set.name_table.add("declared_attr");
    let undeclared_name = schema_set.name_table.add("undeclared_attr");

    let attr_data = AttributeDeclData {
        name: Some(declared_name),
        target_namespace: None,
        ref_name: None,
        type_ref: None,
        inline_type: None,
        default_value: None,
        fixed_value: None,
        use_kind: None,
        form: None,
        inheritable: false,
        id: None,
        annotation: None,
        source: None,
        resolved_type: None,
        resolved_ref: None,
    };
    let attr_key = schema_set.arenas.alloc_attribute(attr_data);
    schema_set
        .get_or_create_namespace(None)
        .register_attribute(declared_name, attr_key);

    let mut w = default_wildcard(WildcardNamespace::Any);
    w.not_qname = vec![NotQNameItem::Defined];

    assert!(
        !admits(&schema_set, &w, None, None, declared_name),
        "##defined MUST exclude globally-declared attributes"
    );
    assert!(
        admits(&schema_set, &w, None, None, undeclared_name),
        "##defined MUST NOT exclude attributes that are not globally declared"
    );
}

#[test]
fn test_effective_wildcard_not_qname_literal_excludes() {
    use crate::parser::frames::NotQNameItem;

    let schema_set = SchemaSet::new();
    let blocked = schema_set.name_table.add("blocked");
    let allowed = schema_set.name_table.add("allowed");

    let mut w = default_wildcard(WildcardNamespace::Any);
    w.not_qname = vec![NotQNameItem::QName {
        namespace: None,
        local_name: blocked,
    }];

    assert!(!admits(&schema_set, &w, None, None, blocked));
    assert!(admits(&schema_set, &w, None, None, allowed));
}

#[test]
fn test_particle_restricts_all_required_over_empty_all_rejects() {
    // Pin test for the exact shape mgO013 reaches after
    // `remove_pointless_particles`: base `all{}` (e1{0,0} removed)
    // vs derived `all{e1{1,1}}`. The driver must reject — derived
    // adds a required particle to empty content, which is not a
    // valid restriction under §3.9.6.
    let schema_set = SchemaSet::new();
    let e1_name = schema_set.name_table.add("e1");
    let any_type = TypeKey::Complex(schema_set.any_type_key());

    let make_elem = |min_occurs: u32, max_occurs: Option<u32>| NormalizedParticle {
        term: NormalizedParticleTerm::Element(NormalizedElement {
            name: e1_name,
            namespace: None,
            type_key: any_type,
            element_key: None,
            block: DerivationSet::empty(),
            nillable: false,
            fixed_value: None,
            fixed_source: None,
        }),
        min_occurs,
        max_occurs,
        source: None,
        collapsed_from: None,
        folded_group_occurs: false,
        counts: None,
        as_written: None,
    };

    let derived = NormalizedParticle {
        term: NormalizedParticleTerm::Group(NormalizedGroup {
            compositor: Compositor::All,
            particles: vec![make_elem(1, Some(1))],
        }),
        min_occurs: 1,
        max_occurs: Some(1),
        source: None,
        collapsed_from: None,
        folded_group_occurs: false,
        counts: None,
        as_written: None,
    };
    let base_empty_all = NormalizedParticle {
        term: NormalizedParticleTerm::Group(NormalizedGroup {
            compositor: Compositor::All,
            particles: Vec::new(),
        }),
        min_occurs: 1,
        max_occurs: Some(1),
        source: None,
        collapsed_from: None,
        folded_group_occurs: false,
        counts: None,
        as_written: None,
    };

    assert!(
        !particle_restricts(&schema_set, &derived, &base_empty_all),
        "all{{e1{{1,1}}}} must NOT restrict all{{}} — derived adds a required particle"
    );
}

#[test]
fn test_collect_flat_attribute_uses_filters_prohibited() {
    // §3.2.2: prohibited attribute uses do NOT correspond to components
    // and must not appear in either side of a restriction comparison.
    use crate::arenas::AttributeGroupData;
    use crate::parser::frames::{
        AttributeFrameResult, AttributeUseKind as AuK, AttributeUseResult,
    };

    let mut schema_set = SchemaSet::new();
    let grp_name = schema_set.name_table.add("ag");
    let opt_name = schema_set.name_table.add("opt");
    let banned_name = schema_set.name_table.add("banned");

    let make_attr = |name: NameId, kind: AuK| AttributeUseResult {
        attribute: AttributeFrameResult {
            name: Some(name),
            ref_name: None,
            target_namespace: None,
            type_ref: None,
            inline_type: None,
            default_value: None,
            fixed_value: None,
            use_kind: None,
            form: None,
            inheritable: None,
            id: None,
            annotation: None,
            source: None,
        },
        use_kind: kind,
    };

    let ag = AttributeGroupData {
        name: Some(grp_name),
        target_namespace: None,
        ref_name: None,
        attributes: vec![
            make_attr(opt_name, AuK::Optional),
            make_attr(banned_name, AuK::Prohibited),
        ],
        attribute_groups: Vec::new(),
        attribute_wildcard: None,
        id: None,
        annotation: None,
        source: None,
        resolved_ref: None,
        resolved_attribute_groups: Vec::new(),
        resolved_attributes: vec![
            crate::arenas::ResolvedAttributeUse {
                resolved_type: None,
                resolved_ref: None,
            },
            crate::arenas::ResolvedAttributeUse {
                resolved_type: None,
                resolved_ref: None,
            },
        ],
        redefine_original: None,
        redefine_requires_restriction_check: false,
    };
    let ag_key = schema_set.arenas.alloc_attribute_group(ag);

    let uses = collect_flat_attribute_uses_for_group(&schema_set, ag_key);
    // Prohibited must be dropped; only `opt` survives.
    assert_eq!(
        uses.len(),
        1,
        "prohibited attribute uses must be filtered out"
    );
    assert_eq!(uses[0].name, opt_name);
}

// -----------------------------------------------------------------
// §3.6.2.2 effective attribute wildcard + §3.10.6.4 intersection
// -----------------------------------------------------------------

fn wildcard_with_ns(namespace: WildcardNamespace) -> WildcardResult {
    WildcardResult {
        namespace,
        process_contents: ProcessContents::Strict,
        not_namespace: Vec::new(),
        not_qname: Vec::new(),
        id: None,
        annotation: None,
        source: None,
    }
}

#[test]
fn test_normalize_any() {
    let wc = wildcard_with_ns(WildcardNamespace::Any);
    let eff = normalize_attribute_wildcard(&wc, None);
    assert!(matches!(eff.namespace, CanonicalNs::Any));
}

#[test]
fn test_normalize_list_resolves_tokens() {
    use crate::parser::frames::NamespaceToken;
    let schema_set = SchemaSet::new();
    let ns_a = schema_set.name_table.add("http://a");
    let target = schema_set.name_table.add("http://t");

    let wc = wildcard_with_ns(WildcardNamespace::List(vec![
        NamespaceToken::Uri(ns_a),
        NamespaceToken::TargetNamespace,
        NamespaceToken::Local,
    ]));
    let eff = normalize_attribute_wildcard(&wc, Some(target));
    match eff.namespace {
        CanonicalNs::Enum(set) => {
            assert!(set.contains(&Some(ns_a)));
            assert!(set.contains(&Some(target)));
            assert!(set.contains(&None));
            assert_eq!(set.len(), 3);
        }
        _ => panic!("expected Enum"),
    }
}

#[test]
fn test_normalize_other_excludes_target_and_absent() {
    // §3.10.2.2: `##other` maps to {variety} not with {namespaces} "a set
    // consisting of ·absent· and, if the targetNamespace [attribute] of
    // the <schema> ancestor element information item is present, its
    // ·actual value·" — for `<anyAttribute>` in XSD 1.1 as in XSD 1.0.
    // The XSD 1.1 attribute path used to drop absent (C24).
    let schema_set = SchemaSet::new();
    let target = schema_set.name_table.add("http://t");

    let wc = wildcard_with_ns(WildcardNamespace::Other);
    let eff = normalize_attribute_wildcard(&wc, Some(target));

    match eff.namespace {
        CanonicalNs::Not(set) => {
            assert!(set.contains(&Some(target)));
            assert!(set.contains(&None), "##other excludes absent");
            assert_eq!(set.len(), 2);
        }
        other => panic!("expected Not, got {:?}", other),
    }
}

#[test]
fn test_normalize_other_absent_target_namespace() {
    // When the schema has no target namespace, ##other is not({absent}):
    // an empty exclusion set would be Not({}) ≡ Any and accept invalid
    // derivations for no-targetNamespace schemas.
    let wc = wildcard_with_ns(WildcardNamespace::Other);
    let eff = normalize_attribute_wildcard(&wc, None);

    match eff.namespace {
        CanonicalNs::Not(set) => {
            assert!(
                set.contains(&None),
                "##other with absent target MUST exclude the absent namespace",
            );
            assert_eq!(set.len(), 1);
        }
        other => panic!("expected Not, got {:?}", other),
    }
}

#[test]
fn test_effective_wildcard_restricts_defined_covers_declared_qname() {
    // Regression: per §3.10.6.2 disallowed_names clause 1, a base
    // QName exclusion is satisfied whenever the derived wildcard
    // is "not allowed" for that QName — including via a derived
    // `##defined` when the base QName names a globally declared
    // attribute. An earlier implementation required literal
    // `QName{}` containment and wrongly rejected this pattern.
    use crate::arenas::AttributeDeclData;
    use crate::parser::frames::NotQNameItem;

    let mut schema_set = SchemaSet::new();
    let declared_name = schema_set.name_table.add("declared_attr");

    // Globally declare `declared_attr`.
    let attr_data = AttributeDeclData {
        name: Some(declared_name),
        target_namespace: None,
        ref_name: None,
        type_ref: None,
        inline_type: None,
        default_value: None,
        fixed_value: None,
        use_kind: None,
        form: None,
        inheritable: false,
        id: None,
        annotation: None,
        source: None,
        resolved_type: None,
        resolved_ref: None,
    };
    let attr_key = schema_set.arenas.alloc_attribute(attr_data);
    schema_set
        .get_or_create_namespace(None)
        .register_attribute(declared_name, attr_key);

    // Base excludes the declared attribute by literal QName.
    let base = EffectiveAttributeWildcard {
        namespace: CanonicalNs::Any,
        not_qname: vec![NotQNameItem::QName {
            namespace: None,
            local_name: declared_name,
        }],
        process_contents: ProcessContents::Strict,
    };
    // Derived excludes via ##defined — should cover the base
    // exclusion because declared_attr is globally declared.
    let derived = EffectiveAttributeWildcard {
        namespace: CanonicalNs::Any,
        not_qname: vec![NotQNameItem::Defined],
        process_contents: ProcessContents::Strict,
    };

    assert!(
        effective_attribute_wildcard_restricts(&schema_set, &derived, &base, true).is_ok(),
        "derived ##defined must cover a base literal QName exclusion \
         when the attribute is globally declared"
    );

    // Undeclared attribute: ##defined does NOT cover it.
    let undeclared = schema_set.name_table.add("undeclared_attr");
    let base_undeclared = EffectiveAttributeWildcard {
        namespace: CanonicalNs::Any,
        not_qname: vec![NotQNameItem::QName {
            namespace: None,
            local_name: undeclared,
        }],
        process_contents: ProcessContents::Strict,
    };
    assert!(
        effective_attribute_wildcard_restricts(&schema_set, &derived, &base_undeclared, true)
            .is_err(),
        "derived ##defined must NOT cover a base QName exclusion \
         when the attribute is not globally declared"
    );
}

#[test]
fn test_normalize_folds_not_namespace() {
    use crate::parser::frames::NamespaceToken;
    let schema_set = SchemaSet::new();
    let ns_a = schema_set.name_table.add("http://a");

    // Any wildcard with not_namespace=[ns_a] becomes Not({ns_a}).
    let mut wc = wildcard_with_ns(WildcardNamespace::Any);
    wc.not_namespace = vec![NamespaceToken::Uri(ns_a)];
    let eff = normalize_attribute_wildcard(&wc, None);
    match eff.namespace {
        CanonicalNs::Not(set) => {
            assert_eq!(set.len(), 1);
            assert!(set.contains(&Some(ns_a)));
        }
        _ => panic!("expected Not"),
    }
}

#[test]
fn test_intersect_any_is_identity() {
    let mut s = std::collections::HashSet::new();
    let schema_set = SchemaSet::new();
    let ns_a = schema_set.name_table.add("http://a");
    s.insert(Some(ns_a));

    let enum_a = CanonicalNs::Enum(s.clone());
    let result = intersect_canonical_ns(&CanonicalNs::Any, &enum_a);
    assert_eq!(result, enum_a);
    let result2 = intersect_canonical_ns(&enum_a, &CanonicalNs::Any);
    assert_eq!(result2, enum_a);
}

#[test]
fn test_intersect_enum_enum_is_set_intersection() {
    let schema_set = SchemaSet::new();
    let ns_a = schema_set.name_table.add("http://a");
    let ns_b = schema_set.name_table.add("http://b");
    let ns_c = schema_set.name_table.add("http://c");

    let mut s1 = std::collections::HashSet::new();
    s1.insert(Some(ns_a));
    s1.insert(Some(ns_b));
    let mut s2 = std::collections::HashSet::new();
    s2.insert(Some(ns_b));
    s2.insert(Some(ns_c));

    let result = intersect_canonical_ns(&CanonicalNs::Enum(s1), &CanonicalNs::Enum(s2));
    match result {
        CanonicalNs::Enum(set) => {
            assert_eq!(set.len(), 1);
            assert!(set.contains(&Some(ns_b)));
        }
        _ => panic!("expected Enum"),
    }
}

#[test]
fn test_intersect_enum_not_is_set_difference() {
    let schema_set = SchemaSet::new();
    let ns_a = schema_set.name_table.add("http://a");
    let ns_b = schema_set.name_table.add("http://b");

    let mut s = std::collections::HashSet::new();
    s.insert(Some(ns_a));
    s.insert(Some(ns_b));
    let mut n = std::collections::HashSet::new();
    n.insert(Some(ns_b));

    let result = intersect_canonical_ns(&CanonicalNs::Enum(s), &CanonicalNs::Not(n));
    match result {
        CanonicalNs::Enum(set) => {
            assert_eq!(set.len(), 1);
            assert!(set.contains(&Some(ns_a)));
        }
        _ => panic!("expected Enum"),
    }
}

#[test]
fn test_intersect_not_not_is_union_of_exclusions() {
    let schema_set = SchemaSet::new();
    let ns_a = schema_set.name_table.add("http://a");
    let ns_b = schema_set.name_table.add("http://b");

    let mut n1 = std::collections::HashSet::new();
    n1.insert(Some(ns_a));
    let mut n2 = std::collections::HashSet::new();
    n2.insert(Some(ns_b));

    let result = intersect_canonical_ns(&CanonicalNs::Not(n1), &CanonicalNs::Not(n2));
    match result {
        CanonicalNs::Not(set) => {
            assert_eq!(set.len(), 2);
            assert!(set.contains(&Some(ns_a)));
            assert!(set.contains(&Some(ns_b)));
        }
        _ => panic!("expected Not"),
    }
}

#[test]
fn test_canonical_ns_subset_various() {
    let schema_set = SchemaSet::new();
    let ns_a = schema_set.name_table.add("http://a");
    let ns_b = schema_set.name_table.add("http://b");

    let empty_set = std::collections::HashSet::new();
    let mut s_a = std::collections::HashSet::new();
    s_a.insert(Some(ns_a));
    let mut s_ab = std::collections::HashSet::new();
    s_ab.insert(Some(ns_a));
    s_ab.insert(Some(ns_b));

    // Anything ⊆ Any
    assert!(canonical_ns_subset(&CanonicalNs::Any, &CanonicalNs::Any));
    assert!(canonical_ns_subset(
        &CanonicalNs::Enum(s_a.clone()),
        &CanonicalNs::Any
    ));
    assert!(canonical_ns_subset(
        &CanonicalNs::Not(s_a.clone()),
        &CanonicalNs::Any
    ));

    // Any ⊄ non-Any
    assert!(!canonical_ns_subset(
        &CanonicalNs::Any,
        &CanonicalNs::Enum(s_a.clone())
    ));

    // Enum(s) ⊆ Enum(t) iff s ⊆ t
    assert!(canonical_ns_subset(
        &CanonicalNs::Enum(s_a.clone()),
        &CanonicalNs::Enum(s_ab.clone()),
    ));
    assert!(!canonical_ns_subset(
        &CanonicalNs::Enum(s_ab.clone()),
        &CanonicalNs::Enum(s_a.clone()),
    ));

    // Enum(s) ⊆ Not(n) iff s ∩ n = ∅
    assert!(canonical_ns_subset(
        &CanonicalNs::Enum(s_a.clone()),
        &CanonicalNs::Not(empty_set.clone()),
    ));
    assert!(!canonical_ns_subset(
        &CanonicalNs::Enum(s_a.clone()),
        &CanonicalNs::Not(s_a.clone()),
    ));

    // Not(n1) ⊆ Not(n2) iff n2 ⊆ n1 (derived exclusion must be ≥ base)
    assert!(canonical_ns_subset(
        &CanonicalNs::Not(s_ab.clone()),
        &CanonicalNs::Not(s_a.clone()),
    ));
    assert!(!canonical_ns_subset(
        &CanonicalNs::Not(s_a.clone()),
        &CanonicalNs::Not(s_ab.clone()),
    ));

    // Not(n) ⊄ Enum(s) — infinite cannot fit in finite
    assert!(!canonical_ns_subset(
        &CanonicalNs::Not(empty_set),
        &CanonicalNs::Enum(s_a),
    ));
}

#[test]
fn test_effective_attribute_wildcard_absent_no_groups_returns_none() {
    let schema_set = SchemaSet::new();
    let result = effective_attribute_wildcard(&schema_set, None, None, &[]);
    assert!(matches!(result, Ok(None)));
}

#[test]
fn test_effective_attribute_wildcard_local_only() {
    let schema_set = SchemaSet::new();
    let wc = wildcard_with_ns(WildcardNamespace::Any);
    let result = effective_attribute_wildcard(&schema_set, Some(&wc), None, &[]).unwrap();
    let eff = result.expect("expected Some");
    assert!(matches!(eff.namespace, CanonicalNs::Any));
}

#[test]
fn test_effective_attribute_wildcard_intersects_across_group_and_local() {
    use crate::arenas::AttributeGroupData;
    use crate::parser::frames::NamespaceToken;

    let mut schema_set = SchemaSet::new();
    let ns_a = schema_set.name_table.add("http://a");
    let ns_b = schema_set.name_table.add("http://b");

    // Referenced group has wildcard List[a, b]
    let group_wc = WildcardResult {
        namespace: WildcardNamespace::List(vec![
            NamespaceToken::Uri(ns_a),
            NamespaceToken::Uri(ns_b),
        ]),
        process_contents: ProcessContents::Strict,
        not_namespace: Vec::new(),
        not_qname: Vec::new(),
        id: None,
        annotation: None,
        source: None,
    };
    let group = AttributeGroupData {
        name: None,
        target_namespace: None,
        ref_name: None,
        attributes: Vec::new(),
        attribute_groups: Vec::new(),
        attribute_wildcard: Some(group_wc),
        id: None,
        annotation: None,
        source: None,
        resolved_ref: None,
        resolved_attribute_groups: Vec::new(),
        resolved_attributes: Vec::new(),
        redefine_original: None,
        redefine_requires_restriction_check: false,
    };
    let group_key = schema_set.arenas.alloc_attribute_group(group);

    // Local wildcard is List[a]. Intersection should be {a}.
    let local = WildcardResult {
        namespace: WildcardNamespace::List(vec![NamespaceToken::Uri(ns_a)]),
        process_contents: ProcessContents::Strict,
        not_namespace: Vec::new(),
        not_qname: Vec::new(),
        id: None,
        annotation: None,
        source: None,
    };
    let result =
        effective_attribute_wildcard(&schema_set, Some(&local), None, &[group_key]).unwrap();
    let eff = result.expect("expected Some");
    match eff.namespace {
        CanonicalNs::Enum(set) => {
            assert_eq!(set.len(), 1);
            assert!(set.contains(&Some(ns_a)));
        }
        other => panic!("expected Enum({{ns_a}}), got {:?}", other),
    }
}

#[test]
fn test_effective_attribute_wildcard_no_local_uses_first_group_pc() {
    use crate::arenas::AttributeGroupData;

    let mut schema_set = SchemaSet::new();
    let group_wc = WildcardResult {
        namespace: WildcardNamespace::Any,
        process_contents: ProcessContents::Lax,
        not_namespace: Vec::new(),
        not_qname: Vec::new(),
        id: None,
        annotation: None,
        source: None,
    };
    let group = AttributeGroupData {
        name: None,
        target_namespace: None,
        ref_name: None,
        attributes: Vec::new(),
        attribute_groups: Vec::new(),
        attribute_wildcard: Some(group_wc),
        id: None,
        annotation: None,
        source: None,
        resolved_ref: None,
        resolved_attribute_groups: Vec::new(),
        resolved_attributes: Vec::new(),
        redefine_original: None,
        redefine_requires_restriction_check: false,
    };
    let group_key = schema_set.arenas.alloc_attribute_group(group);

    // No local ⇒ pc comes from W[0] (Lax).
    let result = effective_attribute_wildcard(&schema_set, None, None, &[group_key]).unwrap();
    let eff = result.expect("expected Some");
    assert_eq!(eff.process_contents, ProcessContents::Lax);
    assert!(matches!(eff.namespace, CanonicalNs::Any));
}

#[test]
fn test_effective_wildcard_allows_attribute_basic() {
    let schema_set = SchemaSet::new();
    let name = schema_set.name_table.add("foo");
    let ns_a = schema_set.name_table.add("http://a");

    let any_eff = EffectiveAttributeWildcard {
        namespace: CanonicalNs::Any,
        not_qname: Vec::new(),
        process_contents: ProcessContents::Strict,
    };
    assert!(effective_wildcard_allows_attribute(
        &schema_set,
        &any_eff,
        Some(ns_a),
        name,
    ));

    let mut s = std::collections::HashSet::new();
    s.insert(Some(ns_a));
    let enum_eff = EffectiveAttributeWildcard {
        namespace: CanonicalNs::Enum(s),
        not_qname: Vec::new(),
        process_contents: ProcessContents::Strict,
    };
    assert!(effective_wildcard_allows_attribute(
        &schema_set,
        &enum_eff,
        Some(ns_a),
        name,
    ));
    assert!(!effective_wildcard_allows_attribute(
        &schema_set,
        &enum_eff,
        None,
        name,
    ));
}

#[test]
fn test_effective_wildcard_restricts_enforces_subset() {
    let schema_set = SchemaSet::new();
    let ns_a = schema_set.name_table.add("http://a");
    let ns_b = schema_set.name_table.add("http://b");

    let mut s_a = std::collections::HashSet::new();
    s_a.insert(Some(ns_a));
    let mut s_ab = std::collections::HashSet::new();
    s_ab.insert(Some(ns_a));
    s_ab.insert(Some(ns_b));

    let derived_narrow = EffectiveAttributeWildcard {
        namespace: CanonicalNs::Enum(s_a.clone()),
        not_qname: Vec::new(),
        process_contents: ProcessContents::Strict,
    };
    let base_wide = EffectiveAttributeWildcard {
        namespace: CanonicalNs::Enum(s_ab.clone()),
        not_qname: Vec::new(),
        process_contents: ProcessContents::Strict,
    };

    assert!(
        effective_attribute_wildcard_restricts(&schema_set, &derived_narrow, &base_wide, true)
            .is_ok()
    );
    assert!(
        effective_attribute_wildcard_restricts(&schema_set, &base_wide, &derived_narrow, true)
            .is_err()
    );
}

#[test]
fn test_effective_wildcard_restricts_enforces_process_contents() {
    let schema_set = SchemaSet::new();
    let skip_eff = EffectiveAttributeWildcard {
        namespace: CanonicalNs::Any,
        not_qname: Vec::new(),
        process_contents: ProcessContents::Skip,
    };
    let strict_eff = EffectiveAttributeWildcard {
        namespace: CanonicalNs::Any,
        not_qname: Vec::new(),
        process_contents: ProcessContents::Strict,
    };
    // Strict restricts Skip (tightening).
    assert!(
        effective_attribute_wildcard_restricts(&schema_set, &strict_eff, &skip_eff, true).is_ok()
    );
    // Skip cannot restrict Strict (loosening).
    assert!(
        effective_attribute_wildcard_restricts(&schema_set, &skip_eff, &strict_eff, true).is_err()
    );
}

#[test]
fn test_validate_attribute_restriction_rejects_added_wildcard() {
    // Base has no wildcard, derived adds Any — invalid restriction.
    let mut schema_set = SchemaSet::new();
    let base = create_complex_type_data(None);
    let base_key = schema_set.arenas.alloc_complex_type(base);

    let mut derived = create_complex_type_data(None);
    derived.attribute_wildcard = Some(wildcard_with_ns(WildcardNamespace::Any));
    derived.derivation_method = Some(DerivationMethod::Restriction);
    derived.resolved_base_type = Some(TypeKey::Complex(base_key));
    let derived_key = schema_set.arenas.alloc_complex_type(derived);

    let derived_ref = schema_set.arenas.complex_types.get(derived_key).unwrap();
    let base_ref = schema_set.arenas.complex_types.get(base_key).unwrap();
    let result = validate_attribute_restriction(&schema_set, derived_ref, base_ref);
    assert!(result.is_err());
    if let Err(SchemaError::StructuralError {
        constraint,
        message,
        ..
    }) = result
    {
        assert_eq!(constraint, "derivation-ok-restriction");
        assert!(
            message.contains("wildcard"),
            "message should mention wildcard, got: {}",
            message
        );
    } else {
        panic!("expected StructuralError");
    }
}

#[test]
fn test_validate_attribute_restriction_accepts_narrower_wildcard() {
    use crate::parser::frames::NamespaceToken;
    let mut schema_set = SchemaSet::new();
    let ns_a = schema_set.name_table.add("http://a");

    let mut base = create_complex_type_data(None);
    base.attribute_wildcard = Some(wildcard_with_ns(WildcardNamespace::Any));
    let base_key = schema_set.arenas.alloc_complex_type(base);

    let mut derived = create_complex_type_data(None);
    derived.attribute_wildcard = Some(wildcard_with_ns(WildcardNamespace::List(vec![
        NamespaceToken::Uri(ns_a),
    ])));
    derived.derivation_method = Some(DerivationMethod::Restriction);
    derived.resolved_base_type = Some(TypeKey::Complex(base_key));
    let derived_key = schema_set.arenas.alloc_complex_type(derived);

    let derived_ref = schema_set.arenas.complex_types.get(derived_key).unwrap();
    let base_ref = schema_set.arenas.complex_types.get(base_key).unwrap();
    assert!(validate_attribute_restriction(&schema_set, derived_ref, base_ref).is_ok());
}

#[test]
fn test_validate_attribute_restriction_allows_removing_wildcard() {
    // Base has Any, derived removes the wildcard — always valid
    // (restriction may remove the wildcard).
    let mut schema_set = SchemaSet::new();
    let mut base = create_complex_type_data(None);
    base.attribute_wildcard = Some(wildcard_with_ns(WildcardNamespace::Any));
    let base_key = schema_set.arenas.alloc_complex_type(base);

    let mut derived = create_complex_type_data(None);
    derived.derivation_method = Some(DerivationMethod::Restriction);
    derived.resolved_base_type = Some(TypeKey::Complex(base_key));
    let derived_key = schema_set.arenas.alloc_complex_type(derived);

    let derived_ref = schema_set.arenas.complex_types.get(derived_key).unwrap();
    let base_ref = schema_set.arenas.complex_types.get(base_key).unwrap();
    assert!(validate_attribute_restriction(&schema_set, derived_ref, base_ref).is_ok());
}

#[test]
fn test_redefine_attribute_group_rejects_broader_wildcard() {
    // Original has Any; redefined "restriction" keeps Any + adds a
    // broader-than-original effective wildcard via added scope —
    // emulated here by giving the redefined side a wildcard that
    // excludes fewer namespaces than the original (via not_namespace).
    use crate::arenas::AttributeGroupData;
    use crate::parser::frames::NamespaceToken;

    let mut schema_set = SchemaSet::new();
    let ns_a = schema_set.name_table.add("http://a");

    // Original wildcard: Any with not_namespace=[ns_a]  ⇒  Not({ns_a})
    let mut original_wc = wildcard_with_ns(WildcardNamespace::Any);
    original_wc.not_namespace = vec![NamespaceToken::Uri(ns_a)];
    let original = AttributeGroupData {
        name: None,
        target_namespace: None,
        ref_name: None,
        attributes: Vec::new(),
        attribute_groups: Vec::new(),
        attribute_wildcard: Some(original_wc),
        id: None,
        annotation: None,
        source: None,
        resolved_ref: None,
        resolved_attribute_groups: Vec::new(),
        resolved_attributes: Vec::new(),
        redefine_original: None,
        redefine_requires_restriction_check: false,
    };
    let original_key = schema_set.arenas.alloc_attribute_group(original);

    // Derived wildcard: plain Any (allows ns_a, which original excludes).
    let derived = AttributeGroupData {
        name: None,
        target_namespace: None,
        ref_name: None,
        attributes: Vec::new(),
        attribute_groups: Vec::new(),
        attribute_wildcard: Some(wildcard_with_ns(WildcardNamespace::Any)),
        id: None,
        annotation: None,
        source: None,
        resolved_ref: None,
        resolved_attribute_groups: Vec::new(),
        resolved_attributes: Vec::new(),
        redefine_original: Some(original_key),
        redefine_requires_restriction_check: true,
    };
    schema_set.arenas.alloc_attribute_group(derived);

    let mut errors = Vec::new();
    let mut stats = DerivationStats::default();
    validate_all_redefine_attribute_group_restrictions(&schema_set, &mut errors, &mut stats);

    assert!(
        !errors.is_empty(),
        "expected a src-redefine.7.2.2 error for broader derived wildcard"
    );
    let msg = match &errors[0] {
        SchemaError::StructuralError {
            constraint,
            message,
            ..
        } => {
            assert_eq!(*constraint, "src-redefine.7.2.2");
            message.clone()
        }
        _ => panic!("expected StructuralError"),
    };
    assert!(
        msg.contains("wildcard") || msg.contains("restriction"),
        "error should mention wildcard restriction, got: {}",
        msg
    );
}

#[test]
fn test_redefine_attribute_group_effective_wildcard_admits_inherited_attr() {
    // Original attribute group references a nested group whose local
    // wildcard is List[ns_a]. The redefined group adds an attribute in
    // ns_a. Without the §3.6.2.2 effective-wildcard fix, this would
    // fail: the original's *local* attribute_wildcard is None, so the
    // old code would reject ns_a even though the inherited wildcard
    // admits it.
    use crate::arenas::{AttributeGroupData, ResolvedAttributeUse};
    use crate::parser::frames::{
        AttributeFrameResult, AttributeUseKind as AuK, AttributeUseResult, NamespaceToken,
    };

    let mut schema_set = SchemaSet::new();
    let ns_a = schema_set.name_table.add("http://a");
    let attr_name = schema_set.name_table.add("foo");

    // Nested group with wildcard List[ns_a].
    let nested_wc = WildcardResult {
        namespace: WildcardNamespace::List(vec![NamespaceToken::Uri(ns_a)]),
        process_contents: ProcessContents::Strict,
        not_namespace: Vec::new(),
        not_qname: Vec::new(),
        id: None,
        annotation: None,
        source: None,
    };
    let nested = AttributeGroupData {
        name: None,
        target_namespace: None,
        ref_name: None,
        attributes: Vec::new(),
        attribute_groups: Vec::new(),
        attribute_wildcard: Some(nested_wc),
        id: None,
        annotation: None,
        source: None,
        resolved_ref: None,
        resolved_attribute_groups: Vec::new(),
        resolved_attributes: Vec::new(),
        redefine_original: None,
        redefine_requires_restriction_check: false,
    };
    let nested_key = schema_set.arenas.alloc_attribute_group(nested);

    // Original group: no local wildcard, references nested.
    let original = AttributeGroupData {
        name: None,
        target_namespace: None,
        ref_name: None,
        attributes: Vec::new(),
        attribute_groups: Vec::new(),
        attribute_wildcard: None,
        id: None,
        annotation: None,
        source: None,
        resolved_ref: None,
        resolved_attribute_groups: vec![nested_key],
        resolved_attributes: Vec::new(),
        redefine_original: None,
        redefine_requires_restriction_check: false,
    };
    let original_key = schema_set.arenas.alloc_attribute_group(original);

    // Redefined group: adds an attribute in ns_a, inherits the nested
    // wildcard through the same reference chain.
    let attr_use = AttributeUseResult {
        attribute: AttributeFrameResult {
            name: Some(attr_name),
            ref_name: None,
            target_namespace: Some(ns_a),
            type_ref: None,
            inline_type: None,
            default_value: None,
            fixed_value: None,
            use_kind: None,
            form: None,
            inheritable: None,
            id: None,
            annotation: None,
            source: None,
        },
        use_kind: AuK::Optional,
    };
    let derived = AttributeGroupData {
        name: None,
        target_namespace: None,
        ref_name: None,
        attributes: vec![attr_use],
        attribute_groups: Vec::new(),
        attribute_wildcard: None,
        id: None,
        annotation: None,
        source: None,
        resolved_ref: None,
        resolved_attribute_groups: vec![nested_key],
        resolved_attributes: vec![ResolvedAttributeUse {
            resolved_type: None,
            resolved_ref: None,
        }],
        redefine_original: Some(original_key),
        redefine_requires_restriction_check: true,
    };
    schema_set.arenas.alloc_attribute_group(derived);

    let mut errors = Vec::new();
    let mut stats = DerivationStats::default();
    validate_all_redefine_attribute_group_restrictions(&schema_set, &mut errors, &mut stats);

    assert!(
        errors.is_empty(),
        "attribute admitted by inherited effective wildcard should not error; got: {:?}",
        errors
            .iter()
            .map(|e| format!("{:?}", e))
            .collect::<Vec<_>>()
    );
}
