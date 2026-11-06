use super::*;

use crate::namespace::table::NameTable;
use crate::navigator::RoXmlNavigator;
use crate::schema::model::DerivationSet;
use crate::types::XmlTypeCode;
use crate::xpath::context::XPathContext;

// Helper to create ElementDeclData for tests
fn make_element_data(
    name: Option<crate::ids::NameId>,
    target_namespace: Option<crate::ids::NameId>,
) -> crate::arenas::ElementDeclData {
    crate::arenas::ElementDeclData {
        name,
        target_namespace,
        ref_name: None,
        type_ref: None,
        inline_type: None,
        substitution_group: Vec::new(),
        default_value: None,
        fixed_value: None,
        nillable: false,
        is_abstract: false,
        min_occurs: 1,
        max_occurs: Some(1),
        block: DerivationSet::empty(),
        final_derivation: DerivationSet::empty(),
        form: None,
        id: None,
        alternatives: Vec::new(),
        identity_constraints: Vec::new(),
        pending_ic_refs: vec![],
        annotation: None,
        source: None,
        resolved_type: None,
        resolved_ref: None,
        resolved_substitution_groups: Vec::new(),
        deferred_type_error: None,
    }
}

// Helper to create AttributeDeclData for tests
fn make_attribute_data(
    name: Option<crate::ids::NameId>,
    target_namespace: Option<crate::ids::NameId>,
) -> crate::arenas::AttributeDeclData {
    crate::arenas::AttributeDeclData {
        name,
        target_namespace,
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
    }
}

#[test]
fn test_name_test_wildcard() {
    let doc = roxmltree::Document::parse("<root xmlns=\"urn:test\"/>").expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child();
    let table = NameTable::new();
    let ctx = XPathContext::new(&table);
    let test = NameTest::Wildcard;

    assert!(matches_name_test(&test, &nav, &ctx));
}

#[test]
fn test_name_test_local_wildcard() {
    let doc = roxmltree::Document::parse("<root xmlns=\"urn:test\"/>").expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child();
    let table = NameTable::new();
    let ns_id = table.add("urn:test");
    let ctx = XPathContext::new(&table);
    // LocalWildcard takes namespace URI - matches any local name in that namespace
    let test = NameTest::LocalWildcard(ns_id);

    assert!(matches_name_test(&test, &nav, &ctx));
}

#[test]
fn test_name_test_namespace_wildcard() {
    let doc = roxmltree::Document::parse("<root xmlns=\"urn:test\"/>").expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child();
    let table = NameTable::new();
    let local_id = table.add("root");
    let ctx = XPathContext::new(&table);
    // NamespaceWildcard takes local name - matches any namespace with that local name
    let test = NameTest::NamespaceWildcard(local_id);

    assert!(matches_name_test(&test, &nav, &ctx));
}

#[test]
fn test_name_test_qname() {
    let doc = roxmltree::Document::parse("<root xmlns=\"urn:test\"/>").expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child();
    let table = NameTable::new();
    let ns_id = table.add("urn:test");
    let local_id = table.add("root");
    let ctx = XPathContext::new(&table);
    let qname = QualifiedName::new(Some(ns_id), local_id, None);
    let test = NameTest::QName(qname);

    assert!(matches_name_test(&test, &nav, &ctx));
}

#[test]
fn test_sequence_type_element_name() {
    let doc = roxmltree::Document::parse("<root xmlns=\"urn:test\"/>").expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child();
    let table = NameTable::new();
    let ns_id = table.add("urn:test");
    let local_id = table.add("root");
    let ctx = XPathContext::new(&table);
    let qname = QualifiedName::new(Some(ns_id), local_id, None);
    let name_test = NameTest::QName(qname);
    let seq = SequenceType::one(ItemType::element(Some(name_test), None, false));

    assert!(matches_sequence_type(&seq, &nav, &ctx));
}

#[test]
fn test_sequence_type_document_with_element() {
    let doc =
        roxmltree::Document::parse("<root xmlns=\"urn:test\"><child/></root>").expect("parse xml");
    let nav = RoXmlNavigator::new(&doc);
    let table = NameTable::new();
    let ns_id = table.add("urn:test");
    let local_id = table.add("root");
    let ctx = XPathContext::new(&table);
    let qname = QualifiedName::new(Some(ns_id), local_id, None);
    let name_test = NameTest::QName(qname);
    let inner = ItemType::element(Some(name_test), None, false);
    let seq = SequenceType::one(ItemType::Document(Some(Box::new(inner))));

    assert!(matches_sequence_type(&seq, &nav, &ctx));
}

#[test]
fn test_sequence_type_processing_instruction_target() {
    let doc = roxmltree::Document::parse("<?target data?><root/>").expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child();
    let table = NameTable::new();
    let ctx = XPathContext::new(&table);
    let seq = SequenceType::one(ItemType::ProcessingInstruction(Some("target".to_string())));

    assert!(matches_sequence_type(&seq, &nav, &ctx));
}

#[test]
fn test_sequence_type_text_like() {
    let doc = roxmltree::Document::parse("<root>text</root>").expect("parse xml");
    let mut text_nav = RoXmlNavigator::new(&doc);
    text_nav.move_to_first_child();
    text_nav.move_to_first_child();

    let table = NameTable::new();
    let ctx = XPathContext::new(&table);
    let seq = SequenceType::one(ItemType::Text);

    assert!(matches_sequence_type(&seq, &text_nav, &ctx));
}

#[test]
fn test_sequence_type_atomic_rejected_for_node() {
    let doc = roxmltree::Document::parse("<root/>").expect("parse xml");
    let nav = RoXmlNavigator::new(&doc);
    let table = NameTable::new();
    let ctx = XPathContext::new(&table);
    let seq = SequenceType::one(ItemType::AtomicType(XmlTypeCode::String));

    assert!(!matches_sequence_type(&seq, &nav, &ctx));
}

// ============================================================================
// Schema-Aware Tests
// ============================================================================

#[test]
fn test_schema_element_without_schema_context() {
    // Test that schema-element() falls back to name-only match without schema_set
    let doc = roxmltree::Document::parse("<root xmlns=\"urn:test\"/>").expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child();

    let table = NameTable::new();
    let ns_id = table.add("urn:test");
    let local_id = table.add("root");
    let ctx = XPathContext::new(&table); // No schema_set

    let qname = QualifiedName::new(Some(ns_id), local_id, None);
    let seq = SequenceType::one(ItemType::SchemaElement(qname));

    // Should match based on name only (no schema context)
    assert!(matches_sequence_type(&seq, &nav, &ctx));
}

#[test]
fn test_schema_element_name_mismatch() {
    // Test that schema-element() rejects non-matching names
    let doc = roxmltree::Document::parse("<other xmlns=\"urn:test\"/>").expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child();

    let table = NameTable::new();
    let ns_id = table.add("urn:test");
    let local_id = table.add("root"); // Different from "other"
    let ctx = XPathContext::new(&table);

    let qname = QualifiedName::new(Some(ns_id), local_id, None);
    let seq = SequenceType::one(ItemType::SchemaElement(qname));

    // Should not match - different local name
    assert!(!matches_sequence_type(&seq, &nav, &ctx));
}

#[test]
fn test_schema_element_namespace_mismatch() {
    // Test that schema-element() rejects non-matching namespaces
    let doc = roxmltree::Document::parse("<root xmlns=\"urn:other\"/>").expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child();

    let table = NameTable::new();
    let ns_id = table.add("urn:test"); // Different from "urn:other"
    let local_id = table.add("root");
    let ctx = XPathContext::new(&table);

    let qname = QualifiedName::new(Some(ns_id), local_id, None);
    let seq = SequenceType::one(ItemType::SchemaElement(qname));

    // Should not match - different namespace
    assert!(!matches_sequence_type(&seq, &nav, &ctx));
}

#[test]
fn test_schema_element_rejects_attribute() {
    // Test that schema-element() rejects attribute nodes
    let doc = roxmltree::Document::parse("<root attr=\"value\"/>").expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child();
    nav.move_to_first_attribute();

    let table = NameTable::new();
    let local_id = table.add("attr");
    let ctx = XPathContext::new(&table);

    let qname = QualifiedName::new(None, local_id, None);
    let seq = SequenceType::one(ItemType::SchemaElement(qname));

    // Should not match - it's an attribute, not an element
    assert!(!matches_sequence_type(&seq, &nav, &ctx));
}

#[test]
fn test_schema_attribute_without_schema_context() {
    // Test that schema-attribute() falls back to name-only match without schema_set
    let doc = roxmltree::Document::parse("<root attr=\"value\"/>").expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child();
    nav.move_to_first_attribute();

    let table = NameTable::new();
    let local_id = table.add("attr");
    let ctx = XPathContext::new(&table); // No schema_set

    let qname = QualifiedName::new(None, local_id, None);
    let seq = SequenceType::one(ItemType::SchemaAttribute(qname));

    // Should match based on name only (no schema context)
    assert!(matches_sequence_type(&seq, &nav, &ctx));
}

#[test]
fn test_schema_attribute_name_mismatch() {
    // Test that schema-attribute() rejects non-matching names
    let doc = roxmltree::Document::parse("<root other=\"value\"/>").expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child();
    nav.move_to_first_attribute();

    let table = NameTable::new();
    let local_id = table.add("attr"); // Different from "other"
    let ctx = XPathContext::new(&table);

    let qname = QualifiedName::new(None, local_id, None);
    let seq = SequenceType::one(ItemType::SchemaAttribute(qname));

    // Should not match - different local name
    assert!(!matches_sequence_type(&seq, &nav, &ctx));
}

#[test]
fn test_schema_attribute_rejects_element() {
    // Test that schema-attribute() rejects element nodes
    let doc = roxmltree::Document::parse("<root/>").expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child();

    let table = NameTable::new();
    let local_id = table.add("root");
    let ctx = XPathContext::new(&table);

    let qname = QualifiedName::new(None, local_id, None);
    let seq = SequenceType::one(ItemType::SchemaAttribute(qname));

    // Should not match - it's an element, not an attribute
    assert!(!matches_sequence_type(&seq, &nav, &ctx));
}

// ============================================================================
// KindTest Schema-Aware Tests
// ============================================================================

#[test]
fn test_kind_test_schema_element_without_schema() {
    // Test KindTest::SchemaElement falls back to name-only match
    let doc = roxmltree::Document::parse("<root/>").expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child();

    let table = NameTable::new();
    let ctx = XPathContext::new(&table);

    let kind_test = KindTest::SchemaElement("root".to_string());
    assert!(matches_kind_test(&nav, &kind_test, &ctx));
}

#[test]
fn test_kind_test_schema_element_name_mismatch() {
    // Test KindTest::SchemaElement rejects non-matching names
    let doc = roxmltree::Document::parse("<other/>").expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child();

    let table = NameTable::new();
    let ctx = XPathContext::new(&table);

    let kind_test = KindTest::SchemaElement("root".to_string());
    assert!(!matches_kind_test(&nav, &kind_test, &ctx));
}

#[test]
fn test_kind_test_schema_element_rejects_attribute() {
    // Test KindTest::SchemaElement rejects attribute nodes
    let doc = roxmltree::Document::parse("<root attr=\"value\"/>").expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child();
    nav.move_to_first_attribute();

    let table = NameTable::new();
    let ctx = XPathContext::new(&table);

    let kind_test = KindTest::SchemaElement("attr".to_string());
    assert!(!matches_kind_test(&nav, &kind_test, &ctx));
}

#[test]
fn test_kind_test_schema_attribute_without_schema() {
    // Test KindTest::SchemaAttribute falls back to name-only match
    let doc = roxmltree::Document::parse("<root attr=\"value\"/>").expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child();
    nav.move_to_first_attribute();

    let table = NameTable::new();
    let ctx = XPathContext::new(&table);

    let kind_test = KindTest::SchemaAttribute("attr".to_string());
    assert!(matches_kind_test(&nav, &kind_test, &ctx));
}

#[test]
fn test_kind_test_schema_attribute_name_mismatch() {
    // Test KindTest::SchemaAttribute rejects non-matching names
    let doc = roxmltree::Document::parse("<root other=\"value\"/>").expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child();
    nav.move_to_first_attribute();

    let table = NameTable::new();
    let ctx = XPathContext::new(&table);

    let kind_test = KindTest::SchemaAttribute("attr".to_string());
    assert!(!matches_kind_test(&nav, &kind_test, &ctx));
}

#[test]
fn test_kind_test_schema_attribute_rejects_element() {
    // Test KindTest::SchemaAttribute rejects element nodes
    let doc = roxmltree::Document::parse("<root/>").expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child();

    let table = NameTable::new();
    let ctx = XPathContext::new(&table);

    let kind_test = KindTest::SchemaAttribute("root".to_string());
    assert!(!matches_kind_test(&nav, &kind_test, &ctx));
}

// ============================================================================
// Schema-Aware Tests with SchemaSet
// ============================================================================

#[test]
fn test_schema_element_with_schema_context_declaration_found() {
    use crate::schema::model::SchemaSet;

    // Create a SchemaSet with an element declaration
    let mut schema_set = SchemaSet::new();
    let ns_uri = schema_set.name_table.add("urn:test");
    let elem_name = schema_set.name_table.add("root");

    // Register an element in the schema
    schema_set.get_or_create_namespace(Some(ns_uri));
    let elem_data = make_element_data(Some(elem_name), Some(ns_uri));
    // No resolved_type - so type check will be skipped
    let elem_key = schema_set.arenas.alloc_element(elem_data);
    schema_set
        .namespaces
        .get_mut(&Some(ns_uri))
        .unwrap()
        .elements
        .insert(elem_name, elem_key);

    // Now test
    let doc = roxmltree::Document::parse("<root xmlns=\"urn:test\"/>").expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child();

    // Build context with schema_set reference
    let ctx = XPathContext::new(&schema_set.name_table).with_schema_set(&schema_set);

    let qname = QualifiedName::new(Some(ns_uri), elem_name, None);
    let seq = SequenceType::one(ItemType::SchemaElement(qname));

    // Should match - declaration found but no type constraint
    assert!(matches_sequence_type(&seq, &nav, &ctx));
}

#[test]
fn test_schema_element_with_schema_context_type_requires_annotation() {
    use crate::schema::model::SchemaSet;

    // Create a SchemaSet with an element declaration that has a resolved_type
    let mut schema_set = SchemaSet::new();
    let ns_uri = schema_set.name_table.add("urn:test");
    let elem_name = schema_set.name_table.add("root");
    let string_type = schema_set.builtin_types().string;

    // Register an element with a resolved type
    schema_set.get_or_create_namespace(Some(ns_uri));
    let mut elem_data = make_element_data(Some(elem_name), Some(ns_uri));
    // Set a resolved type - this will require type annotation on node
    elem_data.resolved_type = Some(TypeKey::Simple(string_type));
    let elem_key = schema_set.arenas.alloc_element(elem_data);
    schema_set
        .namespaces
        .get_mut(&Some(ns_uri))
        .unwrap()
        .elements
        .insert(elem_name, elem_key);

    // Now test
    let doc = roxmltree::Document::parse("<root xmlns=\"urn:test\"/>").expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child();

    // Build context with schema_set reference
    let ctx = XPathContext::new(&schema_set.name_table).with_schema_set(&schema_set);

    let qname = QualifiedName::new(Some(ns_uri), elem_name, None);
    let seq = SequenceType::one(ItemType::SchemaElement(qname));

    // Should NOT match - declaration has type but roxmltree navigator has no schema_type
    assert!(!matches_sequence_type(&seq, &nav, &ctx));
}

#[test]
fn test_schema_attribute_with_schema_context_declaration_found() {
    use crate::schema::model::SchemaSet;

    // Create a SchemaSet with an attribute declaration
    let mut schema_set = SchemaSet::new();
    let attr_name = schema_set.name_table.add("attr");

    // Register an attribute in the no-namespace schema
    schema_set.get_or_create_namespace(None);
    let attr_data = make_attribute_data(Some(attr_name), None);
    // No resolved_type - so type check will be skipped
    let attr_key = schema_set.arenas.alloc_attribute(attr_data);
    schema_set
        .namespaces
        .get_mut(&None)
        .unwrap()
        .attributes
        .insert(attr_name, attr_key);

    // Now test
    let doc = roxmltree::Document::parse("<root attr=\"value\"/>").expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child();
    nav.move_to_first_attribute();

    // Build context with schema_set reference
    let ctx = XPathContext::new(&schema_set.name_table).with_schema_set(&schema_set);

    let qname = QualifiedName::new(None, attr_name, None);
    let seq = SequenceType::one(ItemType::SchemaAttribute(qname));

    // Should match - declaration found but no type constraint
    assert!(matches_sequence_type(&seq, &nav, &ctx));
}

#[test]
fn test_schema_attribute_with_schema_context_type_requires_annotation() {
    use crate::schema::model::SchemaSet;

    // Create a SchemaSet with an attribute declaration that has a resolved_type
    let mut schema_set = SchemaSet::new();
    let attr_name = schema_set.name_table.add("attr");
    let string_type = schema_set.builtin_types().string;

    // Register an attribute with a resolved type
    schema_set.get_or_create_namespace(None);
    let mut attr_data = make_attribute_data(Some(attr_name), None);
    // Set a resolved type - this will require type annotation on node
    attr_data.resolved_type = Some(TypeKey::Simple(string_type));
    let attr_key = schema_set.arenas.alloc_attribute(attr_data);
    schema_set
        .namespaces
        .get_mut(&None)
        .unwrap()
        .attributes
        .insert(attr_name, attr_key);

    // Now test
    let doc = roxmltree::Document::parse("<root attr=\"value\"/>").expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child();
    nav.move_to_first_attribute();

    // Build context with schema_set reference
    let ctx = XPathContext::new(&schema_set.name_table).with_schema_set(&schema_set);

    let qname = QualifiedName::new(None, attr_name, None);
    let seq = SequenceType::one(ItemType::SchemaAttribute(qname));

    // Should NOT match - declaration has type but roxmltree navigator has no schema_type
    assert!(!matches_sequence_type(&seq, &nav, &ctx));
}

#[test]
fn test_schema_element_declaration_not_found_rejects() {
    use crate::schema::model::SchemaSet;

    // Create an empty SchemaSet (no declarations)
    let schema_set = SchemaSet::new();
    let ns_uri = schema_set.name_table.add("urn:test");
    let elem_name = schema_set.name_table.add("root");

    // Now test
    let doc = roxmltree::Document::parse("<root xmlns=\"urn:test\"/>").expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child();

    // Build context with schema_set reference
    let ctx = XPathContext::new(&schema_set.name_table).with_schema_set(&schema_set);

    let qname = QualifiedName::new(Some(ns_uri), elem_name, None);
    let seq = SequenceType::one(ItemType::SchemaElement(qname));

    // Should NOT match - declaration not found in schema context
    assert!(!matches_sequence_type(&seq, &nav, &ctx));
}

// ============================================================================
// Schema-Aware Type Derivation Tests (using RoXmlNavigator.with_schema_type)
// ============================================================================

#[test]
fn test_schema_element_type_derivation_match() {
    use crate::schema::model::SchemaSet;

    // Create a SchemaSet with an element declaration that has a resolved_type
    let mut schema_set = SchemaSet::new();
    let ns_uri = schema_set.name_table.add("urn:test");
    let elem_name = schema_set.name_table.add("root");
    let string_type = schema_set.builtin_types().string;

    // Register an element with xs:string type
    schema_set.get_or_create_namespace(Some(ns_uri));
    let mut elem_data = make_element_data(Some(elem_name), Some(ns_uri));
    elem_data.resolved_type = Some(TypeKey::Simple(string_type));
    let elem_key = schema_set.arenas.alloc_element(elem_data);
    schema_set
        .namespaces
        .get_mut(&Some(ns_uri))
        .unwrap()
        .elements
        .insert(elem_name, elem_key);

    // Create a navigator with the same schema type (exact match)
    let doc = roxmltree::Document::parse("<root xmlns=\"urn:test\"/>").expect("parse xml");
    let nav = {
        let mut nav = RoXmlNavigator::new(&doc);
        nav.move_to_first_child();
        nav.with_schema_type(string_type)
    };

    // Build context with schema_set reference
    let ctx = XPathContext::new(&schema_set.name_table).with_schema_set(&schema_set);

    let qname = QualifiedName::new(Some(ns_uri), elem_name, None);
    let seq = SequenceType::one(ItemType::SchemaElement(qname));

    // Should match - type derives from (is same as) declaration type
    assert!(matches_sequence_type(&seq, &nav, &ctx));
}

#[test]
fn test_schema_element_type_derivation_derived_type() {
    use crate::schema::model::SchemaSet;

    // Create a SchemaSet with an element declaration
    let mut schema_set = SchemaSet::new();
    let ns_uri = schema_set.name_table.add("urn:test");
    let elem_name = schema_set.name_table.add("root");
    // normalizedString derives from string
    let string_type = schema_set.builtin_types().string;
    let normalized_string_type = schema_set.builtin_types().normalized_string;

    // Register an element with xs:string type (base type)
    schema_set.get_or_create_namespace(Some(ns_uri));
    let mut elem_data = make_element_data(Some(elem_name), Some(ns_uri));
    elem_data.resolved_type = Some(TypeKey::Simple(string_type));
    let elem_key = schema_set.arenas.alloc_element(elem_data);
    schema_set
        .namespaces
        .get_mut(&Some(ns_uri))
        .unwrap()
        .elements
        .insert(elem_name, elem_key);

    // Create a navigator with a derived type (normalizedString derives from string)
    let doc = roxmltree::Document::parse("<root xmlns=\"urn:test\"/>").expect("parse xml");
    let nav = {
        let mut nav = RoXmlNavigator::new(&doc);
        nav.move_to_first_child();
        nav.with_schema_type(normalized_string_type)
    };

    // Build context with schema_set reference
    let ctx = XPathContext::new(&schema_set.name_table).with_schema_set(&schema_set);

    let qname = QualifiedName::new(Some(ns_uri), elem_name, None);
    let seq = SequenceType::one(ItemType::SchemaElement(qname));

    // Should match - normalizedString derives from string
    assert!(matches_sequence_type(&seq, &nav, &ctx));
}

#[test]
fn test_schema_element_type_derivation_mismatch() {
    use crate::schema::model::SchemaSet;

    // Create a SchemaSet with an element declaration
    let mut schema_set = SchemaSet::new();
    let ns_uri = schema_set.name_table.add("urn:test");
    let elem_name = schema_set.name_table.add("root");
    // integer does NOT derive from string
    let string_type = schema_set.builtin_types().string;
    let integer_type = schema_set.builtin_types().integer;

    // Register an element with xs:string type
    schema_set.get_or_create_namespace(Some(ns_uri));
    let mut elem_data = make_element_data(Some(elem_name), Some(ns_uri));
    elem_data.resolved_type = Some(TypeKey::Simple(string_type));
    let elem_key = schema_set.arenas.alloc_element(elem_data);
    schema_set
        .namespaces
        .get_mut(&Some(ns_uri))
        .unwrap()
        .elements
        .insert(elem_name, elem_key);

    // Create a navigator with integer type (NOT derived from string)
    let doc = roxmltree::Document::parse("<root xmlns=\"urn:test\"/>").expect("parse xml");
    let nav = {
        let mut nav = RoXmlNavigator::new(&doc);
        nav.move_to_first_child();
        nav.with_schema_type(integer_type)
    };

    // Build context with schema_set reference
    let ctx = XPathContext::new(&schema_set.name_table).with_schema_set(&schema_set);

    let qname = QualifiedName::new(Some(ns_uri), elem_name, None);
    let seq = SequenceType::one(ItemType::SchemaElement(qname));

    // Should NOT match - integer does not derive from string
    assert!(!matches_sequence_type(&seq, &nav, &ctx));
}

#[test]
fn test_schema_attribute_type_derivation_match() {
    use crate::schema::model::SchemaSet;

    // Create a SchemaSet with an attribute declaration
    let mut schema_set = SchemaSet::new();
    let attr_name = schema_set.name_table.add("attr");
    let string_type = schema_set.builtin_types().string;

    // Register an attribute with xs:string type
    schema_set.get_or_create_namespace(None);
    let mut attr_data = make_attribute_data(Some(attr_name), None);
    attr_data.resolved_type = Some(TypeKey::Simple(string_type));
    let attr_key = schema_set.arenas.alloc_attribute(attr_data);
    schema_set
        .namespaces
        .get_mut(&None)
        .unwrap()
        .attributes
        .insert(attr_name, attr_key);

    // Create a navigator with matching schema type
    let doc = roxmltree::Document::parse("<root attr=\"value\"/>").expect("parse xml");
    let nav = {
        let mut nav = RoXmlNavigator::new(&doc);
        nav.move_to_first_child();
        nav.move_to_first_attribute();
        nav.with_schema_type(string_type)
    };

    // Build context with schema_set reference
    let ctx = XPathContext::new(&schema_set.name_table).with_schema_set(&schema_set);

    let qname = QualifiedName::new(None, attr_name, None);
    let seq = SequenceType::one(ItemType::SchemaAttribute(qname));

    // Should match - type matches declaration type
    assert!(matches_sequence_type(&seq, &nav, &ctx));
}

// ============================================================================
// Prefixed QName Tests for KindTest::SchemaElement/SchemaAttribute
// ============================================================================

#[test]
fn test_kind_test_schema_element_prefixed_qname() {
    use crate::namespace::context::NamespaceContextSnapshot;

    // Test KindTest::SchemaElement with prefixed QName (exercises prefix resolution path)
    let doc = roxmltree::Document::parse("<ns:root xmlns:ns=\"urn:test\"/>").expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child();

    let table = NameTable::new();
    let prefix_id = table.add("ns");
    let ns_id = table.add("urn:test");

    // Create namespace context with prefix binding
    let ns_ctx = NamespaceContextSnapshot {
        default_ns: None,
        bindings: vec![(prefix_id, ns_id)],
    };
    let ctx = XPathContext::new(&table).with_namespaces(ns_ctx);

    // Use prefixed QName "ns:root" - this exercises the prefix resolution path
    let kind_test = KindTest::SchemaElement("ns:root".to_string());
    assert!(matches_kind_test(&nav, &kind_test, &ctx));
}

#[test]
fn test_kind_test_schema_element_prefixed_qname_wrong_ns() {
    use crate::namespace::context::NamespaceContextSnapshot;

    // Test that prefixed QName rejects elements in different namespace
    let doc = roxmltree::Document::parse("<ns:root xmlns:ns=\"urn:other\"/>").expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child();

    let table = NameTable::new();
    let prefix_id = table.add("ns");
    let ns_id = table.add("urn:test"); // Different from document's urn:other

    let ns_ctx = NamespaceContextSnapshot {
        default_ns: None,
        bindings: vec![(prefix_id, ns_id)],
    };
    let ctx = XPathContext::new(&table).with_namespaces(ns_ctx);

    let kind_test = KindTest::SchemaElement("ns:root".to_string());
    // Should not match - namespace mismatch
    assert!(!matches_kind_test(&nav, &kind_test, &ctx));
}

#[test]
fn test_kind_test_schema_element_unresolved_prefix() {
    // Test that unresolved prefix returns false (no panic)
    let doc = roxmltree::Document::parse("<ns:root xmlns:ns=\"urn:test\"/>").expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child();

    let table = NameTable::new();
    // No prefix bindings - "unknown" prefix won't resolve
    let ctx = XPathContext::new(&table);

    let kind_test = KindTest::SchemaElement("unknown:root".to_string());
    // Should not match - prefix cannot be resolved
    assert!(!matches_kind_test(&nav, &kind_test, &ctx));
}

#[test]
fn test_kind_test_schema_attribute_prefixed_qname() {
    use crate::namespace::context::NamespaceContextSnapshot;

    // Test KindTest::SchemaAttribute with prefixed QName (exercises prefix resolution path)
    let doc = roxmltree::Document::parse("<root xmlns:ns=\"urn:test\" ns:attr=\"value\"/>")
        .expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child();
    // Find the namespaced attribute (skip xmlns:ns)
    nav.move_to_first_attribute();
    while nav.local_name() != "attr" {
        if !nav.move_to_next_attribute() {
            panic!("ns:attr attribute not found");
        }
    }

    let table = NameTable::new();
    let prefix_id = table.add("ns");
    let ns_id = table.add("urn:test");

    // Create namespace context with prefix binding
    let ns_ctx = NamespaceContextSnapshot {
        default_ns: None,
        bindings: vec![(prefix_id, ns_id)],
    };
    let ctx = XPathContext::new(&table).with_namespaces(ns_ctx);

    // Use prefixed QName "ns:attr" - this exercises the prefix resolution path
    let kind_test = KindTest::SchemaAttribute("ns:attr".to_string());
    assert!(matches_kind_test(&nav, &kind_test, &ctx));
}

#[test]
fn test_kind_test_schema_attribute_prefixed_qname_wrong_ns() {
    use crate::namespace::context::NamespaceContextSnapshot;

    // Test that prefixed QName rejects attributes in different namespace
    let doc = roxmltree::Document::parse("<root xmlns:ns=\"urn:other\" ns:attr=\"value\"/>")
        .expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child();
    nav.move_to_first_attribute();
    while nav.local_name() != "attr" {
        if !nav.move_to_next_attribute() {
            panic!("ns:attr attribute not found");
        }
    }

    let table = NameTable::new();
    let prefix_id = table.add("ns");
    let ns_id = table.add("urn:test"); // Different from document's urn:other

    let ns_ctx = NamespaceContextSnapshot {
        default_ns: None,
        bindings: vec![(prefix_id, ns_id)],
    };
    let ctx = XPathContext::new(&table).with_namespaces(ns_ctx);

    let kind_test = KindTest::SchemaAttribute("ns:attr".to_string());
    // Should not match - namespace mismatch
    assert!(!matches_kind_test(&nav, &kind_test, &ctx));
}

#[test]
fn test_kind_test_schema_attribute_unresolved_prefix() {
    // Test that unresolved prefix returns false (no panic)
    let doc = roxmltree::Document::parse("<root xmlns:ns=\"urn:test\" ns:attr=\"value\"/>")
        .expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child();
    nav.move_to_first_attribute();
    while nav.local_name() != "attr" {
        if !nav.move_to_next_attribute() {
            panic!("ns:attr attribute not found");
        }
    }

    let table = NameTable::new();
    // No prefix bindings - "unknown" prefix won't resolve
    let ctx = XPathContext::new(&table);

    let kind_test = KindTest::SchemaAttribute("unknown:attr".to_string());
    // Should not match - prefix cannot be resolved
    assert!(!matches_kind_test(&nav, &kind_test, &ctx));
}

/// A name test on the `namespace::` axis selects by prefix, and `*` selects
/// every namespace node: the principal node kind of that axis is namespace.
#[test]
fn name_tests_select_namespace_nodes_by_prefix() {
    use crate::xpath::XPathExpr;

    let doc =
        roxmltree::Document::parse(r#"<out xmlns:four="http://four.com"/>"#).expect("parse xml");
    let table = NameTable::new();
    let ctx = XPathContext::new(&table);

    let count = |expr: &str| {
        XPathExpr::compile(expr, &ctx)
            .expect("compile")
            .evaluator(&ctx)
            .run_with_node::<RoXmlNavigator<'_>>(RoXmlNavigator::new(&doc))
            .expect("evaluate")
            .first()
            .and_then(|item| item.as_atomic().map(|v| v.to_string_value()))
            .unwrap_or_default()
    };

    // `xmlns:four` plus the implicit `xml` binding.
    assert_eq!(count("count(/out/namespace::*)"), "2");
    assert_eq!(count("count(/out/namespace::four)"), "1");
    assert_eq!(count("count(/out/namespace::nope)"), "0");
    assert_eq!(count("string(/out/namespace::four)"), "http://four.com");
    // A prefixed name test never matches: the name is in no namespace.
    assert_eq!(count("count(/out/namespace::*[name() = 'four'])"), "1");
}

// ── §2.5.4.3 / §2.5.4.5: the TypeName of an element()/attribute() test ──────

/// An untyped node is annotated `xs:untyped` (element) or `xs:untypedAtomic`
/// (attribute), which derives from none of the ordinary schema types.
#[test]
fn element_and_attribute_type_names_reject_an_untyped_node() {
    use crate::xpath::XPathExpr;

    let doc = roxmltree::Document::parse(r#"<doc a="1"/>"#).expect("parse xml");
    let table = NameTable::new();
    let mut namespaces = crate::namespace::context::NamespaceContextSnapshot::default();
    namespaces.bindings.push((
        table.add("xs"),
        table.add("http://www.w3.org/2001/XMLSchema"),
    ));
    let ctx = XPathContext::new(&table).with_namespaces(namespaces);

    let value = |expr: &str| {
        XPathExpr::compile(expr, &ctx)
            .expect("compile")
            .evaluator(&ctx)
            .run_with_node::<RoXmlNavigator<'_>>(RoXmlNavigator::new(&doc))
            .expect("evaluate")
            .first()
            .and_then(|item| item.as_atomic().map(|v| v.to_string_value()))
            .unwrap_or_default()
    };

    // A document that was never validated carries no type annotation.
    assert_eq!(value("/doc instance of element(*, xs:integer)"), "false");
    assert_eq!(value("/doc instance of element(doc, xs:integer)"), "false");
    assert_eq!(
        value("/doc/@a instance of attribute(*, xs:integer)"),
        "false"
    );
    assert_eq!(
        value("/doc/@a instance of attribute(a, xs:string)"),
        "false"
    );
    // `xs:untyped` / `xs:untypedAtomic` and `xs:anyType` do match.
    assert_eq!(value("/doc instance of element(*, xs:untyped)"), "true");
    assert_eq!(value("/doc instance of element(*, xs:anyType)"), "true");
    assert_eq!(
        value("/doc/@a instance of attribute(*, xs:untypedAtomic)"),
        "true"
    );
    assert_eq!(
        value("/doc/@a instance of attribute(*, xs:anyAtomicType)"),
        "true"
    );
    assert_eq!(
        value("/doc/@a instance of attribute(*, xs:anySimpleType)"),
        "true"
    );
    // …and the two untyped annotations are not interchangeable.
    assert_eq!(
        value("/doc instance of element(*, xs:untypedAtomic)"),
        "false"
    );
    assert_eq!(
        value("/doc/@a instance of attribute(*, xs:untyped)"),
        "false"
    );
    // A test without a TypeName is unaffected.
    assert_eq!(value("/doc instance of element()"), "true");
    assert_eq!(value("/doc instance of element(doc)"), "true");
    assert_eq!(value("/doc/@a instance of attribute()"), "true");
    assert_eq!(value("/doc/@a instance of attribute(a)"), "true");
    // The `T?` form only relaxes the nilled requirement, not the type match.
    assert_eq!(value("/doc instance of element(*, xs:untyped?)"), "true");
    assert_eq!(value("/doc instance of element(*, xs:integer?)"), "false");
    // An unresolvable prefix in the TypeName is the static error XPST0081
    // (Appendix G), not a test that quietly matches nothing.
    assert_eq!(
        XPathExpr::compile("/doc instance of element(*, nope:t)", &ctx)
            .expect_err("unbound prefix")
            .error_code(),
        Some("XPST0081")
    );
}

/// On a validated document the annotation is compared with `derives-from`.
#[test]
fn element_and_attribute_type_names_use_the_real_annotation() {
    use crate::document::typed_builder::build_typed_document;
    use crate::document::BufferDocumentOptions;
    use crate::pipeline::load_and_process_schema;
    use crate::schema::SchemaSet;
    use crate::xpath::XPathExpr;
    use bumpalo::Bump;

    let mut schema_set = SchemaSet::xsd11();
    load_and_process_schema(
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
             <xs:simpleType name="count">
               <xs:restriction base="xs:integer"/>
             </xs:simpleType>
             <xs:element name="doc">
               <xs:complexType>
                 <xs:sequence>
                   <xs:element name="n" type="count"/>
                 </xs:sequence>
                 <xs:attribute name="a" type="xs:date"/>
               </xs:complexType>
             </xs:element>
           </xs:schema>"#
            .as_bytes(),
        "test.xsd",
        &mut schema_set,
        None,
    )
    .expect("load schema");

    let arena = Bump::new();
    let doc = build_typed_document(
        r#"<doc a="2024-01-01"><n>7</n></doc>"#.as_bytes(),
        &arena,
        &schema_set,
        BufferDocumentOptions::default(),
    )
    .expect("build typed document");

    let mut namespaces = crate::namespace::context::NamespaceContextSnapshot::default();
    namespaces.bindings.push((
        schema_set.name_table.add("xs"),
        schema_set
            .name_table
            .add("http://www.w3.org/2001/XMLSchema"),
    ));
    let ctx = XPathContext::new(&schema_set.name_table)
        .with_namespaces(namespaces)
        .with_schema_set(&schema_set);

    let value = |expr: &str| {
        XPathExpr::compile(expr, &ctx)
            .expect("compile")
            .evaluator(&ctx)
            .run_with_node(doc.create_navigator())
            .expect("evaluate")
            .first()
            .and_then(|item| item.as_atomic().map(|v| v.to_string_value()))
            .unwrap_or_default()
    };

    // The attribute is annotated xs:date.
    assert_eq!(value("/doc/@a instance of attribute(*, xs:date)"), "true");
    assert_eq!(value("/doc/@a instance of attribute(a, xs:date)"), "true");
    assert_eq!(
        value("/doc/@a instance of attribute(*, xs:integer)"),
        "false"
    );
    // An annotated node is no longer untyped.
    assert_eq!(
        value("/doc/@a instance of attribute(*, xs:untypedAtomic)"),
        "false"
    );
    // `n` is annotated with the user-defined `count`, derived from xs:integer.
    assert_eq!(value("/doc/n instance of element(*, count)"), "true");
    assert_eq!(value("/doc/n instance of element(*, xs:integer)"), "true");
    assert_eq!(value("/doc/n instance of element(*, xs:decimal)"), "true");
    assert_eq!(value("/doc/n instance of element(*, xs:string)"), "false");
    assert_eq!(value("/doc/n instance of element(*, xs:untyped)"), "false");
    // The element itself has a complex type, so no atomic type matches it, but
    // xs:anyType does.
    assert_eq!(value("/doc instance of element(*, xs:anyType)"), "true");
    assert_eq!(value("/doc instance of element(*, xs:string)"), "false");
}

// ── §2.5.4.4 / §2.5.4.6: schema-element(N) and schema-attribute(N) ─────────

/// Build the fixture schema and instance the two `schema-*` tests below use,
/// then evaluate `exprs` over it with the schema set attached.
#[cfg(test)]
fn schema_test_fixture(exprs: &[&str]) -> Vec<String> {
    use crate::document::typed_builder::build_typed_document;
    use crate::document::BufferDocumentOptions;
    use crate::pipeline::load_and_process_schema;
    use crate::schema::SchemaSet;
    use crate::xpath::XPathExpr;
    use bumpalo::Bump;

    let mut schema_set = SchemaSet::xsd11();
    load_and_process_schema(
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
             <xs:element name="head" type="xs:string"/>
             <xs:element name="member" type="xs:token" substitutionGroup="head"/>
             <xs:element name="deep" type="xs:token" substitutionGroup="member"/>
             <xs:element name="other" type="xs:string"/>
             <xs:element name="maybe" type="xs:string" nillable="true"/>
             <xs:attribute name="ga" type="xs:integer"/>
             <xs:element name="doc">
               <xs:complexType>
                 <xs:sequence>
                   <xs:element ref="head" minOccurs="0" maxOccurs="unbounded"/>
                   <xs:element ref="other"/>
                   <xs:element ref="maybe"/>
                 </xs:sequence>
                 <xs:attribute ref="ga"/>
               </xs:complexType>
             </xs:element>
           </xs:schema>"#
            .as_bytes(),
        "test.xsd",
        &mut schema_set,
        None,
    )
    .expect("load schema");

    let arena = Bump::new();
    let doc = build_typed_document(
        concat!(
            r#"<doc ga="3" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">"#,
            "<head>a</head><member>b</member><deep>c</deep>",
            r#"<other>d</other><maybe xsi:nil="true"/></doc>"#
        )
        .as_bytes(),
        &arena,
        &schema_set,
        BufferDocumentOptions::default(),
    )
    .expect("build typed document");

    let mut namespaces = crate::namespace::context::NamespaceContextSnapshot::default();
    namespaces.bindings.push((
        schema_set.name_table.add("xs"),
        schema_set
            .name_table
            .add("http://www.w3.org/2001/XMLSchema"),
    ));
    let ctx = XPathContext::new(&schema_set.name_table)
        .with_namespaces(namespaces)
        .with_schema_set(&schema_set);

    exprs
        .iter()
        .map(|expr| {
            XPathExpr::compile(expr, &ctx)
                .expect("compile")
                .evaluator(&ctx)
                .run_with_node(doc.create_navigator())
                .expect("evaluate")
                .first()
                .and_then(|item| item.as_atomic().map(|v| v.to_string_value()))
                .unwrap_or_default()
        })
        .collect()
}

/// XPath 2.0 §2.5.4.4: a `SchemaElementTest` "matches a candidate element node
/// if all three of the following conditions are satisfied: The name of the
/// candidate node matches the specified `ElementName` or matches the name of an
/// element in a substitution group headed by an element named `ElementName`.
/// `derives-from(AT, ET)` is true … If the element declaration for
/// `ElementName` … is not nillable, then the nilled property of the candidate
/// node is false."
///
/// The same rule has to hold whether the test is used as a **step** node test
/// or inside a **SequenceType**.
#[test]
fn schema_element_test_matches_the_declaration_as_a_step_and_in_a_sequence_type() {
    let exprs = [
        // Step form.
        "count(//schema-element(head))",
        "count(//schema-element(other))",
        "count(//schema-element(maybe))",
        "count(/doc/schema-element(member))",
        "string-join(//schema-element(head)/local-name(), ' ')",
        // SequenceType form: the same answers.
        "/doc/head instance of schema-element(head)",
        "/doc/member instance of schema-element(head)",
        "/doc/deep instance of schema-element(head)",
        "/doc/other instance of schema-element(head)",
        "/doc/head instance of schema-element(member)",
        "/doc/maybe instance of schema-element(maybe)",
        "/doc instance of schema-element(doc)",
    ];
    assert_eq!(
        schema_test_fixture(&exprs),
        [
            // `head`, `member` and `deep` — the head itself plus the two
            // members of the substitution group it heads (transitively).
            "3",
            "1",
            "1",
            // (An ElementName with no declaration is XPST0008:
            // `an_undeclared_schema_element_or_attribute_name_is_xpst0008`.)
            // `member` heads a group containing `deep` only.
            "2",
            "head member deep",
            "true",
            "true",
            "true",
            "false",
            "false",
            // `maybe` is nillable, so the nilled node still matches.
            "true",
            "true",
        ]
    );
}

/// XPath 2.0 §2.5.4.6: a `SchemaAttributeTest` "matches a candidate attribute
/// node if both of the following conditions are satisfied: The name of the
/// candidate node matches the specified `AttributeName`. `derives-from(AT, ET)`
/// is true …" — there is no substitution group for attributes.
#[test]
fn schema_attribute_test_matches_the_declaration_as_a_step_and_in_a_sequence_type() {
    // (An AttributeName with no attribute declaration — `nosuch`, or the
    // element `head` — is XPST0008:
    // `an_undeclared_schema_element_or_attribute_name_is_xpst0008`.)
    let exprs = [
        "count(//schema-attribute(ga))",
        "count(/doc/schema-attribute(ga))",
        "/doc/@ga instance of schema-attribute(ga)",
        "/doc instance of schema-attribute(ga)",
    ];
    assert_eq!(schema_test_fixture(&exprs), ["1", "1", "true", "false"]);
}

// ── Wave B (B1): complex `TypeName`s, `T?`, and the static checks ──────────

/// The schema of the complex-type tests below: a type `base` and its
/// extension `ext`, an unrelated complex type `other`, a simple type `code`;
/// `b` (nillable, type `base`) heads a substitution group with `e` (type
/// `ext`), and the non-nillable `nb` (type `base`) heads one with the nillable
/// `m` (type `ext`).
#[cfg(test)]
const COMPLEX_SCHEMA: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:complexType name="base">
    <xs:sequence><xs:element name="v" type="xs:string" minOccurs="0"/></xs:sequence>
  </xs:complexType>
  <xs:complexType name="ext">
    <xs:complexContent>
      <xs:extension base="base"><xs:attribute name="x" type="xs:integer"/></xs:extension>
    </xs:complexContent>
  </xs:complexType>
  <xs:complexType name="other"><xs:sequence/></xs:complexType>
  <xs:simpleType name="code"><xs:restriction base="xs:token"/></xs:simpleType>
  <xs:element name="b" type="base" nillable="true"/>
  <xs:element name="e" type="ext" substitutionGroup="b" nillable="true"/>
  <xs:element name="nb" type="base"/>
  <xs:element name="m" type="ext" substitutionGroup="nb" nillable="true"/>
  <xs:element name="o" type="other"/>
  <xs:attribute name="c" type="code"/>
  <xs:element name="doc">
    <xs:complexType>
      <xs:sequence>
        <xs:element ref="b" maxOccurs="unbounded"/>
        <xs:element ref="o"/>
        <xs:element ref="nb" maxOccurs="unbounded"/>
      </xs:sequence>
      <xs:attribute ref="c"/>
    </xs:complexType>
  </xs:element>
</xs:schema>"#;

/// The instance: two non-nilled and two nilled members of `b`'s group, then
/// `o`, then `nb` and a nilled and a non-nilled `m`.
#[cfg(test)]
const COMPLEX_INSTANCE: &str = concat!(
    r#"<doc c="k" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">"#,
    r#"<b><v>1</v></b><e x="2"/><b xsi:nil="true"/><e xsi:nil="true"/>"#,
    r#"<o/><nb/><m xsi:nil="true"/><m x="3"/></doc>"#
);

/// Evaluate each of `exprs` over the typed [`COMPLEX_INSTANCE`] with the
/// schema set attached and `xs` bound; a result is the string value of the
/// first item (`""` for the empty sequence) or `error:CODE`.
#[cfg(test)]
fn complex_type_fixture(exprs: &[&str]) -> Vec<String> {
    use crate::document::typed_builder::build_typed_document;
    use crate::document::BufferDocumentOptions;
    use crate::pipeline::load_and_process_schema;
    use crate::schema::SchemaSet;
    use crate::xpath::XPathExpr;
    use bumpalo::Bump;

    let mut schema_set = SchemaSet::xsd11();
    load_and_process_schema(
        COMPLEX_SCHEMA.as_bytes(),
        "complex.xsd",
        &mut schema_set,
        None,
    )
    .expect("load schema");
    let arena = Bump::new();
    let doc = build_typed_document(
        COMPLEX_INSTANCE.as_bytes(),
        &arena,
        &schema_set,
        BufferDocumentOptions::default(),
    )
    .expect("build typed document");

    let mut namespaces = crate::namespace::context::NamespaceContextSnapshot::default();
    namespaces.bindings.push((
        schema_set.name_table.add("xs"),
        schema_set
            .name_table
            .add("http://www.w3.org/2001/XMLSchema"),
    ));
    let ctx = XPathContext::new(&schema_set.name_table)
        .with_namespaces(namespaces)
        .with_schema_set(&schema_set);

    let code = |e: crate::xpath::XPathError| format!("error:{}", e.error_code().unwrap_or("?"));
    exprs
        .iter()
        .map(|expr| match XPathExpr::compile(expr, &ctx) {
            Err(e) => code(e),
            Ok(compiled) => match compiled
                .evaluator(&ctx)
                .run_with_node(doc.create_navigator())
            {
                Err(e) => code(e),
                Ok(value) => value
                    .first()
                    .map(|item| match item {
                        XmlItem::Atomic(v) => v.to_string_value(),
                        XmlItem::Node(n) => n.value(),
                    })
                    .unwrap_or_default(),
            },
        })
        .collect()
}

/// XPath 2.0 §2.5.4.3: "`element(ElementName, TypeName)` matches an element
/// node whose name is ElementName if derives-from(AT, TypeName) is true, where
/// AT is the type annotation of the element node, and the nilled property of
/// the node is false"; with "`TypeName ?` … The nilled property of the node may
/// be either true or false"; and "substitution groups do not affect the
/// semantics of ElementTest". A **complex** `TypeName`, as a step and inside a
/// SequenceType.
#[test]
fn element_test_with_a_complex_type_name_as_a_step_and_in_a_sequence_type() {
    let exprs = [
        // The fixture really is typed.
        "count(//element(*, xs:untyped))",
        // Step form. `base` covers `ext` by extension; nilled nodes need `?`.
        "count(//element(*, base))",
        "count(//element(*, base?))",
        "count(//element(*, ext))",
        "count(//element(*, ext?))",
        "count(//element(*, other))",
        "count(//element(b, base))",
        "count(//element(b, base?))",
        "count(//element(e, base))",
        "string-join(/doc/element(*, base)/local-name(), ' ')",
        "string-join(/doc/element(*, base?)/local-name(), ' ')",
        // Every element derives from xs:anyType; the nilled ones need `?`.
        "count(//element(*, xs:anyType))",
        "count(//element(*, xs:anyType?))",
        // The predicates see only what the whole test selects (§3.2.2).
        "string(/doc/element(*, base?)[3]/local-name())",
        // SequenceType form: the same answers.
        "/doc/b[1] instance of element(*, base)",
        "/doc/e[1] instance of element(*, base)",
        "/doc/e[1] instance of element(b, base)",
        "/doc/e[1] instance of element(e, base)",
        "/doc/b[2] instance of element(b, base)",
        "/doc/b[2] instance of element(b, base?)",
        "/doc/o instance of element(*, base)",
        "/doc/o instance of element(o, other)",
        "count(/doc/b[1] treat as element(*, base))",
        "/doc/o treat as element(*, base)",
        "/doc/b[2] treat as element(*, base)",
    ];
    assert_eq!(
        complex_type_fixture(&exprs),
        [
            "0",
            "4", // b, e, nb, m[2]
            "7", // + b[2], e[2], m[1]
            "2", // e[1], m[2]
            "4",
            "1",
            "1",
            "2",
            "1",
            "b e nb m",
            "b e b e nb m m",
            "7", // doc, b[1], v, e[1], o, nb, m[2]
            "10",
            "b",
            "true",
            "true",
            "false",
            "true",
            "false",
            "true",
            "false",
            "true",
            "1",
            "error:XPDY0050",
            "error:XPDY0050",
        ]
    );
}

/// The derivation of a complex `TypeName` follows the schema, and an
/// `attribute(N, T)` test with a simple type keeps working (regression).
#[test]
fn attribute_tests_with_simple_type_names_keep_working() {
    let exprs = [
        "count(//attribute(*, code))",
        "count(//attribute(*, xs:token))",
        "count(//attribute(c, xs:token))",
        "count(//attribute(x, xs:integer))",
        "count(/doc/attribute(*, xs:untypedAtomic))",
        "/doc/@c instance of attribute(c, code)",
        "/doc/@c instance of attribute(*, xs:integer)",
        "count(/doc/@c treat as attribute(*, xs:anySimpleType))",
    ];
    assert_eq!(
        complex_type_fixture(&exprs),
        ["1", "1", "1", "2", "0", "true", "false", "1"]
    );
}

/// §2.5.4.4 with **complex** declared types: the substitution group, the
/// derived member type, and the nilled clause ("If the element declaration for
/// ElementName in the in-scope element declarations is not nillable, then the
/// nilled property of the candidate node is false").
#[test]
fn schema_element_test_with_complex_declared_types() {
    let exprs = [
        // `b` is nillable: its group b, e, b(nil), e(nil).
        "count(//schema-element(b))",
        "count(//schema-element(e))",
        // `nb` is not nillable: nb and the non-nilled m only.
        "count(//schema-element(nb))",
        // `m` is nillable itself.
        "count(//schema-element(m))",
        "/doc/e[1] instance of schema-element(b)",
        "/doc/m[1] instance of schema-element(nb)",
        "/doc/m[2] instance of schema-element(nb)",
        "/doc/o instance of schema-element(b)",
        "count(/doc/schema-element(b)[2])",
    ];
    assert_eq!(
        complex_type_fixture(&exprs),
        ["4", "2", "2", "2", "true", "false", "true", "false", "1"]
    );
}

/// §2.5.4.4: "If the ElementName specified in the SchemaElementTest is not
/// found in the in-scope element declarations, a static error is raised
/// [err:XPST0008]." §2.5.4.6 the same for attributes. Static: raised when the
/// expression is compiled, also where it would never be evaluated.
#[test]
fn an_undeclared_schema_element_or_attribute_name_is_xpst0008() {
    let exprs = [
        "count(//schema-element(nosuch))",
        "/doc instance of schema-element(nosuch)",
        "if (true()) then 1 else count(//schema-element(nosuch))",
        "count(//schema-attribute(nosuch))",
        // `b` is an element, not an attribute; `c` an attribute, not an element.
        "count(//schema-attribute(b))",
        "count(//schema-element(c))",
        "/doc treat as document-node(schema-element(nosuch))",
        // Declared names still compile.
        "count(//schema-attribute(c))",
        "count(//schema-element(doc))",
    ];
    assert_eq!(
        complex_type_fixture(&exprs),
        [
            "error:XPST0008",
            "error:XPST0008",
            "error:XPST0008",
            "error:XPST0008",
            "error:XPST0008",
            "error:XPST0008",
            "error:XPST0008",
            "1",
            "1",
        ]
    );
}

/// §2.1.1: the in-scope element and attribute declarations are part of the
/// static context. A context with no schema set has none, so every
/// `schema-element(N)` / `schema-attribute(N)` is XPST0008 there.
#[test]
fn schema_element_without_a_schema_set_is_xpst0008() {
    use crate::xpath::XPathExpr;

    let table = NameTable::new();
    let ctx = XPathContext::new(&table);
    for expr in [
        "count(//schema-element(a))",
        "/a instance of schema-element(a)",
        "count(//schema-attribute(b))",
        "/a instance of document-node(schema-element(a))",
    ] {
        let err = XPathExpr::compile(expr, &ctx).expect_err(expr);
        assert_eq!(err.error_code(), Some("XPST0008"), "{expr}: {err}");
    }
}

/// §2.5.4: "QNames appearing in a sequence type have their prefixes expanded
/// to namespace URIs by means of the statically known namespaces and (where
/// applicable) the default element/type namespace. An unprefixed attribute
/// QName is in no namespace." The step form and the SequenceType form agree.
#[test]
fn an_unprefixed_element_name_in_a_sequence_type_uses_the_default_element_namespace() {
    use crate::xpath::XPathExpr;

    let doc = roxmltree::Document::parse(r#"<a xmlns="urn:d" b="1"><c/></a>"#).expect("parse xml");
    let table = NameTable::new();
    let ctx = XPathContext::new(&table).with_default_element_ns(table.add("urn:d"));
    let value = |expr: &str| {
        XPathExpr::compile(expr, &ctx)
            .expect("compile")
            .evaluator(&ctx)
            .run_with_node::<RoXmlNavigator<'_>>(RoXmlNavigator::new(&doc))
            .expect("evaluate")
            .first()
            .and_then(|item| item.as_atomic().map(|v| v.to_string_value()))
            .unwrap_or_default()
    };
    assert_eq!(value("count(/element(a))"), "1");
    assert_eq!(value("/a instance of element(a)"), "true");
    assert_eq!(value("/a/c instance of element(c)"), "true");
    assert_eq!(
        value("/a treat as element(a) instance of element()"),
        "true"
    );
    // An attribute name stays in no namespace.
    assert_eq!(value("/a/@b instance of attribute(b)"), "true");
    assert_eq!(value("count(/a/attribute(b))"), "1");
}

/// B1: the `ItemType` a host builds — e.g. for a function signature — holds a
/// complex `TypeName` ([`TypeTest::Schema`] with a [`TypeKey::Complex`]) and
/// the nillable marker of `element(N, T?)`, and matches with the same rule as
/// the expression forms above.
#[test]
fn item_type_element_test_holds_a_complex_type_and_the_nillable_marker() {
    use crate::document::typed_builder::build_typed_document;
    use crate::document::BufferDocumentOptions;
    use crate::pipeline::load_and_process_schema;
    use crate::schema::SchemaSet;
    use crate::xpath::XPathExpr;
    use bumpalo::Bump;

    let mut schema_set = SchemaSet::xsd11();
    load_and_process_schema(
        COMPLEX_SCHEMA.as_bytes(),
        "complex.xsd",
        &mut schema_set,
        None,
    )
    .expect("load schema");
    let arena = Bump::new();
    let doc = build_typed_document(
        COMPLEX_INSTANCE.as_bytes(),
        &arena,
        &schema_set,
        BufferDocumentOptions::default(),
    )
    .expect("build typed document");
    let ctx = XPathContext::new(&schema_set.name_table).with_schema_set(&schema_set);
    let node = |path: &str| {
        let value = XPathExpr::compile(path, &ctx)
            .expect("compile")
            .evaluator(&ctx)
            .run_with_node(doc.create_navigator())
            .expect("evaluate");
        value.first().cloned().expect("a node")
    };
    let names = &schema_set.name_table;
    let base = schema_set
        .lookup_type(None, names.add("base"))
        .expect("type base");
    let ext = schema_set
        .lookup_type(None, names.add("ext"))
        .expect("type ext");
    assert!(matches!(base, TypeKey::Complex(_)));

    let element = |type_test, nillable| ItemType::element(None, Some(type_test), nillable);
    let b1 = node("/doc/b[1]");
    let e1 = node("/doc/e[1]");
    let b2_nilled = node("/doc/b[2]");
    let o = node("/doc/o");

    // derives-from over complex types: ext extends base.
    assert!(element(TypeTest::Schema(base), false).matches_item(&b1, &ctx));
    assert!(element(TypeTest::Schema(base), false).matches_item(&e1, &ctx));
    assert!(!element(TypeTest::Schema(ext), false).matches_item(&b1, &ctx));
    assert!(!element(TypeTest::Schema(base), false).matches_item(&o, &ctx));
    // The nilled clause and `T?`.
    assert!(!element(TypeTest::Schema(base), false).matches_item(&b2_nilled, &ctx));
    assert!(element(TypeTest::Schema(base), true).matches_item(&b2_nilled, &ctx));
    // A built-in named by its code: xs:anyType covers every element.
    assert!(element(TypeTest::BuiltIn(XmlTypeCode::AnyType), false).matches_item(&o, &ctx));
    // An annotated element is not untyped.
    assert!(!element(TypeTest::Untyped, false).matches_item(&b1, &ctx));
    // The name half: `element(e, base)` and the wildcard.
    let e_name = NameTest::QName(QualifiedName::new(None, names.add("e"), None));
    assert!(
        ItemType::element(Some(e_name.clone()), Some(TypeTest::Schema(base)), false)
            .matches_item(&e1, &ctx)
    );
    assert!(
        !ItemType::element(Some(e_name), Some(TypeTest::Schema(base)), false)
            .matches_item(&b1, &ctx)
    );
    assert!(ItemType::element(Some(NameTest::Wildcard), None, false).matches_item(&b2_nilled, &ctx));
    // Through a SequenceType, as a function signature checks an argument.
    let seq = SequenceType::plus(element(TypeTest::Schema(base), false));
    assert!(seq.matches_sequence(&[b1.clone(), e1.clone()], &ctx));
    assert!(!seq.matches_sequence(&[b1, o], &ctx));

    // An attribute test with a simple type key.
    let code = schema_set
        .lookup_type(None, names.add("code"))
        .expect("type code");
    let c = node("/doc/@c");
    assert!(ItemType::attribute(None, Some(TypeTest::Schema(code))).matches_item(&c, &ctx));
    assert!(
        ItemType::attribute(None, Some(TypeTest::BuiltIn(XmlTypeCode::Token)))
            .matches_item(&c, &ctx)
    );
    assert!(
        !ItemType::attribute(None, Some(TypeTest::BuiltIn(XmlTypeCode::Integer)))
            .matches_item(&c, &ctx)
    );
    // Not an attribute test for an element, nor the other way round.
    assert!(!ItemType::attribute(None, None).matches_item(&e1, &ctx));
    assert!(!ItemType::element(None, None, false).matches_item(&c, &ctx));
}

/// B1: on an untyped document a host-built test sees `xs:untyped` (element)
/// and `xs:untypedAtomic` (attribute) as the annotations, with or without a
/// schema set in the static context (§2.5.1).
#[test]
fn item_type_tests_see_the_untyped_annotations() {
    let doc = roxmltree::Document::parse(r#"<doc a="1"/>"#).expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child();
    let element = XmlItem::Node(nav.clone());
    nav.move_to_first_attribute();
    let attribute = XmlItem::Node(nav);

    let table = NameTable::new();
    let ctx = XPathContext::new(&table);
    let e = |t| ItemType::element(None, Some(t), false).matches_item(&element, &ctx);
    let a = |t| ItemType::attribute(None, Some(t)).matches_item(&attribute, &ctx);

    assert!(e(TypeTest::Untyped));
    assert!(e(TypeTest::BuiltIn(XmlTypeCode::AnyType)));
    assert!(!e(TypeTest::BuiltIn(XmlTypeCode::UntypedAtomic)));
    assert!(!e(TypeTest::BuiltIn(XmlTypeCode::Integer)));
    assert!(a(TypeTest::BuiltIn(XmlTypeCode::UntypedAtomic)));
    assert!(a(TypeTest::BuiltIn(XmlTypeCode::AnyAtomicType)));
    assert!(a(TypeTest::BuiltIn(XmlTypeCode::AnySimpleType)));
    assert!(a(TypeTest::BuiltIn(XmlTypeCode::AnyType)));
    assert!(!a(TypeTest::Untyped));
    assert!(!a(TypeTest::BuiltIn(XmlTypeCode::String)));

    // With a schema set, its keys for the predefined types answer the same.
    let schema_set = crate::schema::SchemaSet::xsd11();
    let ctx = XPathContext::new(&schema_set.name_table).with_schema_set(&schema_set);
    let any_type = TypeKey::Complex(schema_set.any_type_key());
    let untyped_atomic = TypeKey::Simple(schema_set.builtin_types().untyped_atomic.expect("1.1"));
    let string = TypeKey::Simple(schema_set.builtin_types().string);
    let e = |t| ItemType::element(None, Some(t), false).matches_item(&element, &ctx);
    let a = |t| ItemType::attribute(None, Some(t)).matches_item(&attribute, &ctx);
    assert!(e(TypeTest::Schema(any_type)));
    assert!(!e(TypeTest::Schema(string)));
    assert!(a(TypeTest::Schema(untyped_atomic)));
    assert!(a(TypeTest::Schema(any_type)));
    assert!(!a(TypeTest::Schema(string)));
}

/// B1: the kind test of an expression resolves into that `ItemType`:
/// `xs:untyped` and the `xs` built-ins without a schema set, a user type to its
/// key, and `T?` to the nillable marker.
#[test]
fn kind_tests_resolve_to_item_types() {
    use crate::xpath::ast::{ElementTest as AstElementTest, QName as AstQName};

    let schema_set = {
        let mut set = crate::schema::SchemaSet::xsd11();
        crate::pipeline::load_and_process_schema(
            COMPLEX_SCHEMA.as_bytes(),
            "complex.xsd",
            &mut set,
            None,
        )
        .expect("load schema");
        set
    };
    let names = &schema_set.name_table;
    let mut namespaces = crate::namespace::context::NamespaceContextSnapshot::default();
    namespaces.bindings.push((
        names.add("xs"),
        names.add("http://www.w3.org/2001/XMLSchema"),
    ));
    let ctx = XPathContext::new(names)
        .with_namespaces(namespaces)
        .with_schema_set(&schema_set);
    let element = |type_name: &str, prefix: &str, nillable| {
        KindTest::Element(AstElementTest {
            name: None,
            type_name: Some(AstQName::new(prefix.to_string(), type_name.to_string())),
            nillable,
        })
    };
    let resolve = |kind: &KindTest| item_type_of_kind_test(kind, &ctx).expect("resolve");

    let base = schema_set.lookup_type(None, names.add("base")).unwrap();
    assert_eq!(
        resolve(&element("base", "", true)),
        ItemType::element(None, Some(TypeTest::Schema(base)), true)
    );
    assert_eq!(
        resolve(&element("untyped", "xs", false)),
        ItemType::element(None, Some(TypeTest::Untyped), false)
    );
    assert_eq!(
        resolve(&element("integer", "xs", false)),
        ItemType::element(None, Some(TypeTest::BuiltIn(XmlTypeCode::Integer)), false)
    );
    // An unknown type name, or a prefix the context cannot expand, is the
    // binder's static error; an unbound tree gets the same error here.
    assert_eq!(
        item_type_of_kind_test(&element("nosuch", "", false), &ctx)
            .unwrap_err()
            .error_code(),
        Some("XPST0008")
    );
    assert_eq!(
        item_type_of_kind_test(&element("t", "nope", false), &ctx)
            .unwrap_err()
            .error_code(),
        Some("XPST0081")
    );
}

// ── Fix round (review R5–R8) ───────────────────────────────────────────────

/// R5: `element(N)` has one spelling — without a type the nillable marker is
/// meaningless and is normalized away; a schema key of a predefined type is
/// the same test as the type's code, and `TypeTest::canonical` gives the
/// binder's spelling.
#[test]
fn item_type_and_type_test_spellings_are_canonical() {
    let schema_set = crate::schema::SchemaSet::xsd11();
    let names = &schema_set.name_table;
    let n = NameTest::QName(QualifiedName::new(None, names.add("a"), None));
    assert_eq!(
        ItemType::element(Some(n.clone()), None, true),
        ItemType::element(Some(n.clone()), None, false)
    );
    let integer = TypeKey::Simple(schema_set.builtin_types().integer);
    let any_type = TypeKey::Complex(schema_set.any_type_key());
    assert_eq!(
        TypeTest::Schema(integer).canonical(&schema_set),
        TypeTest::BuiltIn(XmlTypeCode::Integer)
    );
    assert_eq!(
        TypeTest::Schema(any_type).canonical(&schema_set),
        TypeTest::BuiltIn(XmlTypeCode::AnyType)
    );
    assert_eq!(TypeTest::Untyped.canonical(&schema_set), TypeTest::Untyped);
    // The two spellings match alike.
    let doc = roxmltree::Document::parse(r#"<a b="1"/>"#).expect("parse xml");
    let mut nav = RoXmlNavigator::new(&doc);
    nav.move_to_first_child();
    let element = XmlItem::Node(nav);
    let ctx = XPathContext::new(names).with_schema_set(&schema_set);
    for (schema, built_in) in [
        (any_type, XmlTypeCode::AnyType),
        (integer, XmlTypeCode::Integer),
    ] {
        assert_eq!(
            ItemType::element(None, Some(TypeTest::Schema(schema)), false)
                .matches_item(&element, &ctx),
            ItemType::element(None, Some(TypeTest::BuiltIn(built_in)), false)
                .matches_item(&element, &ctx),
        );
    }
}

/// Build `xml` typed against `schema` in the given XSD version and evaluate
/// each of `exprs` with the schema set attached; `error:CODE` for an error.
#[cfg(test)]
fn typed_fixture(schema: &str, xml: &str, xsd11: bool, exprs: &[&str]) -> Vec<String> {
    use crate::document::typed_builder::build_typed_document;
    use crate::document::BufferDocumentOptions;
    use crate::pipeline::load_and_process_schema;
    use crate::schema::SchemaSet;
    use crate::xpath::XPathExpr;
    use bumpalo::Bump;

    let mut schema_set = if xsd11 {
        SchemaSet::xsd11()
    } else {
        SchemaSet::new()
    };
    load_and_process_schema(schema.as_bytes(), "t.xsd", &mut schema_set, None).expect("schema");
    let arena = Bump::new();
    let doc = build_typed_document(
        xml.as_bytes(),
        &arena,
        &schema_set,
        BufferDocumentOptions::default(),
    )
    .expect("typed document");
    let mut namespaces = crate::namespace::context::NamespaceContextSnapshot::default();
    namespaces.bindings.push((
        schema_set.name_table.add("xs"),
        schema_set
            .name_table
            .add("http://www.w3.org/2001/XMLSchema"),
    ));
    let ctx = XPathContext::new(&schema_set.name_table)
        .with_namespaces(namespaces)
        .with_schema_set(&schema_set);
    let code = |e: crate::xpath::XPathError| format!("error:{}", e.error_code().unwrap_or("?"));
    exprs
        .iter()
        .map(|expr| match XPathExpr::compile(expr, &ctx) {
            Err(e) => code(e),
            Ok(c) => match c.evaluator(&ctx).run_with_node(doc.create_navigator()) {
                Err(e) => code(e),
                Ok(v) => v
                    .first()
                    .map(|item| match item {
                        XmlItem::Atomic(a) => a.to_string_value(),
                        XmlItem::Node(n) => n.value(),
                    })
                    .unwrap_or_default(),
            },
        })
        .collect()
}

#[cfg(test)]
const ATOMIC_SCHEMA: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:simpleType name="small">
    <xs:restriction base="xs:integer"><xs:maxInclusive value="9"/></xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="ints"><xs:list itemType="xs:integer"/></xs:simpleType>
  <xs:element name="doc">
    <xs:complexType>
      <xs:sequence>
        <xs:element name="sm" type="small"/>
        <xs:element name="ni" type="xs:integer"/>
        <xs:element name="li" type="ints"/>
      </xs:sequence>
      <xs:attribute name="i" type="xs:integer"/>
      <xs:attribute name="l" type="ints"/>
    </xs:complexType>
  </xs:element>
</xs:schema>"#;

#[cfg(test)]
const ATOMIC_INSTANCE: &str = r#"<doc i="4" l="1 2"><sm>3</sm><ni>12</ni><li>5 6</li></doc>"#;

/// R6: §2.5.4 `derives-from(AT, xs:anyAtomicType)` is true for every atomic
/// type, also with an XSD 1.0 schema set (which has no `xs:anyAtomicType`
/// component of its own); a list type is not atomic. `xs:anySimpleType`
/// covers lists too; `xs:untypedAtomic` covers no annotated node.
#[test]
fn any_atomic_type_matches_annotated_atomic_nodes_in_both_xsd_versions() {
    let exprs = [
        "/doc/ni instance of element(*, xs:anyAtomicType)",
        "/doc/sm instance of element(*, xs:anyAtomicType)",
        "/doc/@i instance of attribute(*, xs:anyAtomicType)",
        "/doc/@l instance of attribute(*, xs:anyAtomicType)",
        "/doc/li instance of element(*, xs:anyAtomicType)",
        "/doc instance of element(*, xs:anyAtomicType)",
        "/doc/@l instance of attribute(*, xs:anySimpleType)",
        "/doc/ni instance of element(*, xs:anySimpleType)",
        "/doc/@i instance of attribute(*, xs:untypedAtomic)",
        "count(//attribute(*, xs:anyAtomicType))",
    ];
    let expected = [
        "true", "true", "true", "false", "false", "false", "true", "true", "false", "1",
    ];
    for xsd11 in [false, true] {
        assert_eq!(
            typed_fixture(ATOMIC_SCHEMA, ATOMIC_INSTANCE, xsd11, &exprs),
            expected,
            "xsd11={xsd11}"
        );
    }
}

/// R8: XPath 2.0 §2.5.4.2: "An AtomicType AtomicType matches an atomic value
/// whose actual type is AT if derives-from(AT, AtomicType) is true" — also
/// for a user-defined atomic type.
#[test]
fn instance_of_a_user_defined_atomic_type() {
    let exprs = [
        "data(/doc/sm) instance of small",
        "data(/doc/sm) instance of xs:integer",
        "data(/doc/ni) instance of small",
        "3 instance of small",
        "data(/doc/sm) treat as small",
        "data(/doc/ni) treat as small",
        "count(data(/doc/li)[. instance of xs:integer])",
    ];
    let expected = ["true", "true", "false", "false", "3", "error:XPDY0050", "2"];
    for xsd11 in [false, true] {
        assert_eq!(
            typed_fixture(ATOMIC_SCHEMA, ATOMIC_INSTANCE, xsd11, &exprs),
            expected,
            "xsd11={xsd11}"
        );
    }
}

/// R7: XPath 2.0 §2.5.4.2: "document-node(E) matches any document node that
/// contains exactly one element node, optionally accompanied by one or more
/// comment and processing instruction nodes, if E is an ElementTest or
/// SchemaElementTest that matches the element node" — one rule for a host's
/// `ItemType`, `instance of` and a path step.
#[test]
fn document_node_with_an_element_test_needs_exactly_one_element_child() {
    use crate::document::builder::BufferDocumentBuilder;
    use crate::document::BufferDocumentOptions;
    use crate::xpath::XPathExpr;
    use bumpalo::Bump;

    let names = NameTable::new();
    let arena = Bump::new();
    // Three constructed documents: one element with a comment and a PI, two
    // elements, one element and a text node.
    let build = |shape: &str| {
        let mut b =
            BufferDocumentBuilder::new(&arena, &names, None, BufferDocumentOptions::default())
                .expect("builder");
        match shape {
            "one" => {
                b.comment("c").unwrap();
                b.start_element("a", "", "", &[]).unwrap();
                b.end_of_attributes();
                b.end_element().unwrap();
                b.processing_instruction("p", "d").unwrap();
            }
            "two" => {
                for name in ["a", "b"] {
                    b.start_element(name, "", "", &[]).unwrap();
                    b.end_of_attributes();
                    b.end_element().unwrap();
                }
            }
            _ => {
                b.start_element("a", "", "", &[]).unwrap();
                b.end_of_attributes();
                b.end_element().unwrap();
                b.text("t");
            }
        }
        b.finalize().expect("document")
    };
    let docs = [build("one"), build("two"), build("text")];
    let ctx = XPathContext::new(&names);
    let a = NameTest::QName(QualifiedName::new(None, names.add("a"), None));
    let host = ItemType::Document(Some(Box::new(ItemType::element(Some(a), None, false))));
    let expr = |e: &str| XPathExpr::compile(e, &ctx).expect("compile");
    let instance = expr(". instance of document-node(element(a))");
    let step = expr("count(self::document-node(element(a)))");
    let any_element = expr(". instance of document-node(element())");
    let mut seen = Vec::new();
    for doc in &docs {
        let nav = doc.create_navigator();
        let atom = |c: &XPathExpr| {
            c.evaluator(&ctx)
                .run_with_node(nav.clone())
                .expect("evaluate")
                .first()
                .and_then(|i| i.as_atomic().map(|v| v.to_string_value()))
                .unwrap_or_default()
        };
        seen.push((
            host.matches_item(&XmlItem::Node(nav.clone()), &ctx),
            atom(&instance),
            atom(&step),
            atom(&any_element),
        ));
    }
    let row = |h, i: &str, s: &str, e: &str| (h, i.to_string(), s.to_string(), e.to_string());
    assert_eq!(
        seen,
        [
            row(true, "true", "1", "true"),
            row(false, "false", "0", "false"),
            row(false, "false", "0", "false"),
        ]
    );
}

// ── Fix round 2 (re-review N1, N2) ─────────────────────────────────────────

#[cfg(test)]
const UNION_LIST_SCHEMA: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:simpleType name="ios"><xs:union memberTypes="xs:integer xs:string"/></xs:simpleType>
  <xs:simpleType name="il"><xs:list itemType="xs:integer"/></xs:simpleType>
  <xs:simpleType name="ios2"><xs:restriction base="ios"/></xs:simpleType>
  <xs:simpleType name="small">
    <xs:restriction base="xs:integer"><xs:maxInclusive value="9"/></xs:restriction>
  </xs:simpleType>
  <xs:element name="doc">
    <xs:complexType>
      <xs:sequence>
        <xs:element name="u" type="ios"/>
        <xs:element name="sm" type="small"/>
      </xs:sequence>
      <xs:attribute name="ua" type="ios"/>
      <xs:attribute name="n" type="il"/>
    </xs:complexType>
  </xs:element>
</xs:schema>"#;

/// N1: XPath 2.0 §2.5.4.2: "An ItemType consisting simply of a QName is
/// interpreted as an AtomicType. … If a QName that is used as an AtomicType is
/// not defined as an atomic type in the in-scope schema types, a static error is
/// raised [err:XPST0051]." §3.10.2 / §3.10.3: "The target type must be an atomic
/// type that is in the in-scope schema types [err:XPST0051]." A union or list
/// type of the schema is a simple type but not an atomic one — also a
/// restriction of a union.
#[test]
fn a_union_or_list_type_is_not_an_atomic_type() {
    let exprs = [
        "data(/doc/u) instance of ios",
        "data(/doc/@ua) treat as ios",
        "data(/doc/@n)[1] instance of il",
        "data(/doc/@n) instance of il+",
        "data(/doc/u) instance of ios2",
        "'1' cast as ios",
        "'1 2' cast as il",
        "'1' castable as il",
        "'1' castable as ios2?",
        // Not evaluated, still a static error.
        "if (true()) then 1 else (2 instance of ios)",
        // Atomic types keep working.
        "data(/doc/sm) instance of small",
        "data(/doc/u) instance of xs:integer",
    ];
    let mut expected = vec!["error:XPST0051"; 10];
    expected.extend(["true", "true"]);
    for xsd11 in [false, true] {
        assert_eq!(
            typed_fixture(
                UNION_LIST_SCHEMA,
                r#"<doc ua="7" n="1 2"><u>5</u><sm>3</sm></doc>"#,
                xsd11,
                &exprs
            ),
            expected,
            "xsd11={xsd11}"
        );
    }
}

// ── C69: derives-from does not count union membership ──────────────────────

#[cfg(test)]
const UNION_MEMBER_SCHEMA: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:simpleType name="small">
    <xs:restriction base="xs:integer"><xs:maxInclusive value="9"/></xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="ints"><xs:list itemType="xs:integer"/></xs:simpleType>
  <xs:simpleType name="decOrString"><xs:union memberTypes="xs:decimal xs:string"/></xs:simpleType>
  <xs:simpleType name="smallOrInts"><xs:union memberTypes="small ints"/></xs:simpleType>
  <xs:simpleType name="narrow">
    <xs:restriction base="decOrString"><xs:pattern value="\d+"/></xs:restriction>
  </xs:simpleType>
  <xs:complexType name="decWithUnit">
    <xs:simpleContent>
      <xs:extension base="xs:decimal"><xs:attribute name="unit" type="xs:string"/></xs:extension>
    </xs:simpleContent>
  </xs:complexType>
  <xs:element name="e" type="decOrString"/>
  <xs:element name="doc">
    <xs:complexType>
      <xs:sequence>
        <xs:element name="d" type="xs:decimal"/>
        <xs:element name="s" type="small"/>
        <xs:element name="l" type="ints"/>
        <xs:element name="u" type="decOrString"/>
        <xs:element name="n" type="narrow"/>
        <xs:element name="c" type="decWithUnit"/>
        <xs:element ref="e"/>
        <xs:element ref="e"/>
      </xs:sequence>
      <xs:attribute name="a" type="xs:decimal"/>
      <xs:attribute name="b" type="decOrString"/>
    </xs:complexType>
  </xs:element>
</xs:schema>"#;

/// C69: XPath 2.0 §2.5.4: "derives-from(AT, ET) returns true if ET is a known
/// type and any of the following three conditions is true: AT is a schema type
/// found in the in-scope schema definitions, and is the same as ET or is derived
/// by restriction or extension from ET …; There exists some schema type IT such
/// that derives-from(IT, ET) and derives-from(AT, IT) are true." A member type
/// of a union is neither — the validation-time rule that accepts it (Structures
/// §3.16.6.3 clause 2.2.4, `xsi:type` substitutability) is not derives-from. A
/// restriction of the union still derives from it.
#[test]
fn a_member_type_does_not_derive_from_its_union() {
    let exprs = [
        "/doc/d instance of element(*, decOrString)",
        "/doc/s instance of element(*, smallOrInts)",
        "/doc/l instance of element(*, smallOrInts)",
        "/doc/c instance of element(*, decOrString)",
        "/doc/@a instance of attribute(*, decOrString)",
        "count(/doc/element(*, decOrString))",
        "count(/doc/attribute(*, decOrString))",
        "/doc/d treat as element(*, decOrString)",
        // `e` declares `decOrString`; the second one is `xsi:type="xs:decimal"`,
        // a member type: valid (cvc-elt.4.3), but not derived from the union.
        "count(/doc/schema-element(e))",
        "count(/doc/e[. instance of element(e, decOrString)])",
        // Same type and a restriction of the union still match.
        "/doc/u instance of element(*, decOrString)",
        "/doc/n instance of element(*, decOrString)",
        "/doc/@b instance of attribute(*, decOrString)",
        "/doc/d instance of element(*, xs:decimal)",
        "/doc/s instance of element(*, xs:integer)",
        "/doc/c instance of element(*, xs:decimal)",
        "/doc/l instance of element(*, xs:anySimpleType)",
    ];
    let mut expected = vec!["false", "false", "false", "false", "false", "3", "1"];
    expected.extend(["error:XPDY0050", "1", "1"]);
    expected.extend(["true"; 7]);
    let xml = r#"<doc xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
        xmlns:xs="http://www.w3.org/2001/XMLSchema" a="1.5" b="x"><d>1.5</d><s>3</s><l>1 2</l
        ><u>x</u><n>12</n><c unit="m">2.5</c><e>y</e><e xsi:type="xs:decimal">4.5</e></doc>"#;
    for xsd11 in [false, true] {
        assert_eq!(
            typed_fixture(UNION_MEMBER_SCHEMA, xml, xsd11, &exprs),
            expected,
            "xsd11={xsd11}"
        );
    }
}
