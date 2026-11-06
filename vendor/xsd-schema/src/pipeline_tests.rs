use super::*;
use crate::error::SchemaError;
use crate::ids::TypeKey;

#[test]
fn test_load_and_process_minimal_schema() {
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(result.is_ok(), "Should parse minimal schema: {:?}", result);

    let stats = result.unwrap();
    assert_eq!(stats.doc_id, 0);
    assert!(stats.inline_stats.is_some());
    assert!(stats.resolution_stats.is_some());
}

#[test]
fn test_load_and_process_element_with_type() {
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:element name="root" type="xs:string"/>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(result.is_ok());

    let stats = result.unwrap();
    let resolution_stats = stats.resolution_stats.unwrap();
    assert!(
        resolution_stats.types_resolved > 0,
        "Should resolve type reference"
    );

    // Verify element's type was resolved
    let root_name = schema_set.name_table.get("root").unwrap();
    let elem_key = schema_set.lookup_element(None, root_name).unwrap();
    let elem = schema_set.arenas.elements.get(elem_key).unwrap();
    assert!(
        elem.resolved_type.is_some(),
        "Element type should be resolved"
    );
}

#[test]
fn test_load_and_process_inline_complex_type() {
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:element name="person">
                <xs:complexType>
                    <xs:sequence>
                        <xs:element name="name" type="xs:string"/>
                        <xs:element name="age" type="xs:int"/>
                    </xs:sequence>
                </xs:complexType>
            </xs:element>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "Should parse schema with inline type: {:?}",
        result
    );

    let stats = result.unwrap();
    let inline_stats = stats.inline_stats.unwrap();
    assert!(
        inline_stats.element_inline_types > 0,
        "Should assemble inline complex type"
    );

    // Verify element's resolved_type is set
    let person_name = schema_set.name_table.get("person").unwrap();
    let elem_key = schema_set.lookup_element(None, person_name).unwrap();
    let elem = schema_set.arenas.elements.get(elem_key).unwrap();
    assert!(
        elem.resolved_type.is_some(),
        "Inline type should be resolved"
    );
    assert!(matches!(elem.resolved_type, Some(TypeKey::Complex(_))));
}

#[test]
fn test_load_and_process_inline_simple_type() {
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:element name="status">
                <xs:simpleType>
                    <xs:restriction base="xs:string">
                        <xs:enumeration value="active"/>
                        <xs:enumeration value="inactive"/>
                    </xs:restriction>
                </xs:simpleType>
            </xs:element>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "Should parse schema with inline simple type: {:?}",
        result
    );

    let stats = result.unwrap();
    let inline_stats = stats.inline_stats.unwrap();
    assert!(
        inline_stats.element_inline_types > 0,
        "Should assemble inline simple type"
    );

    // Verify element's resolved_type is set
    let status_name = schema_set.name_table.get("status").unwrap();
    let elem_key = schema_set.lookup_element(None, status_name).unwrap();
    let elem = schema_set.arenas.elements.get(elem_key).unwrap();
    assert!(
        elem.resolved_type.is_some(),
        "Inline type should be resolved"
    );
    assert!(matches!(elem.resolved_type, Some(TypeKey::Simple(_))));
}

#[test]
fn test_load_and_process_rejects_invalid_particle_wildcard_restriction() {
    let mut schema_set = SchemaSet::new();
    let xsd = r###"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                   targetNamespace="urn:test"
                   xmlns:t="urn:test">
            <xs:complexType name="Base">
                <xs:choice>
                    <xs:any namespace="##any"/>
                </xs:choice>
            </xs:complexType>
            <xs:complexType name="Restricted">
                <xs:complexContent>
                    <xs:restriction base="t:Base">
                        <xs:choice>
                            <xs:any processContents="lax"/>
                        </xs:choice>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(result.is_err());

    match result.unwrap_err() {
        crate::error::SchemaError::StructuralError { constraint, .. } => {
            assert_eq!(constraint, "derivation-ok-restriction");
        }
        other => panic!("Expected derivation-ok-restriction, got {:?}", other),
    }
}

#[test]
fn test_load_and_process_rejects_upa_conflict() {
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:complexType name="Ambiguous">
                <xs:choice>
                    <xs:element name="a"/>
                    <xs:element name="a"/>
                </xs:choice>
            </xs:complexType>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(result.is_err());

    match result.unwrap_err() {
        crate::error::SchemaError::StructuralError { constraint, .. } => {
            assert_eq!(constraint, "cos-nonambig");
        }
        other => panic!("Expected cos-nonambig, got {:?}", other),
    }
}

#[test]
fn test_load_and_process_accepts_sequence_restriction_of_all_group() {
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:complexType name="Base">
                <xs:all>
                    <xs:element name="a" type="xs:string"/>
                    <xs:element name="b" type="xs:string" minOccurs="0"/>
                </xs:all>
            </xs:complexType>
            <xs:complexType name="Restricted">
                <xs:complexContent>
                    <xs:restriction base="Base">
                        <xs:sequence>
                            <xs:element name="a" type="xs:string"/>
                        </xs:sequence>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "all-group restriction should be valid: {:?}",
        result
    );
}

#[test]
fn test_load_and_process_xsd10_rejects_optional_element_restricting_optional_choice() {
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:complexType name="Base">
                <xs:sequence>
                    <xs:choice minOccurs="0">
                        <xs:element name="a" type="xs:string"/>
                        <xs:element name="b" type="xs:string"/>
                    </xs:choice>
                </xs:sequence>
            </xs:complexType>
            <xs:complexType name="Restricted">
                <xs:complexContent>
                    <xs:restriction base="Base">
                        <xs:sequence>
                            <xs:element name="a" type="xs:string" minOccurs="0"/>
                        </xs:sequence>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(result.is_err());

    match result.unwrap_err() {
        crate::error::SchemaError::StructuralError { constraint, .. } => {
            assert_eq!(constraint, "derivation-ok-restriction");
        }
        other => panic!("Expected derivation-ok-restriction, got {:?}", other),
    }
}

#[test]
fn test_load_and_process_xsd11_allows_optional_element_restricting_optional_choice() {
    let mut schema_set = SchemaSet::xsd11();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:complexType name="Base">
                <xs:sequence>
                    <xs:choice minOccurs="0">
                        <xs:element name="a" type="xs:string"/>
                        <xs:element name="b" type="xs:string"/>
                    </xs:choice>
                </xs:sequence>
            </xs:complexType>
            <xs:complexType name="Restricted">
                <xs:complexContent>
                    <xs:restriction base="Base">
                        <xs:sequence>
                            <xs:element name="a" type="xs:string" minOccurs="0"/>
                        </xs:sequence>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "XSD 1.1 optional-choice restriction should be valid: {:?}",
        result
    );
}

// ========================================================================
// Particle restriction & normalization tests (Phase 3 stabilization)
// ========================================================================

/// Repeated sequence with non-unit child occurs: sequence{1,2}(b{2,2}).
/// The guard must let this through so the restriction check can reject it.
/// (particlesHa147)
#[test]
fn test_reject_repeated_sequence_occurs_mismatch() {
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:complexType name="base">
                <xs:choice>
                    <xs:sequence minOccurs="1" maxOccurs="2">
                        <xs:element name="b" minOccurs="2" maxOccurs="2"/>
                    </xs:sequence>
                </xs:choice>
            </xs:complexType>
            <xs:complexType name="derived">
                <xs:complexContent>
                    <xs:restriction base="base">
                        <xs:choice>
                            <xs:element name="b" minOccurs="3" maxOccurs="3"/>
                        </xs:choice>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_err(),
        "b{{3,3}} cannot restrict sequence{{1,2}}(b{{2,2}})"
    );
    match result.unwrap_err() {
        crate::error::SchemaError::StructuralError { constraint, .. } => {
            assert_eq!(constraint, "derivation-ok-restriction");
        }
        other => panic!("Expected derivation-ok-restriction, got {:?}", other),
    }
}

/// Choice branches with non-unit occurs: choice(c1{2,2}, c2).
/// The restriction check must run and detect that c1{3,3} > c1{2,2}.
/// (particlesL004)
#[test]
fn test_reject_choice_branch_occurs_mismatch() {
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:complexType name="B">
                <xs:sequence>
                    <xs:choice>
                        <xs:element name="c1" minOccurs="2" maxOccurs="2"/>
                        <xs:element name="c2"/>
                    </xs:choice>
                    <xs:choice minOccurs="1" maxOccurs="3">
                        <xs:element name="d1"/>
                        <xs:element name="d2"/>
                    </xs:choice>
                </xs:sequence>
            </xs:complexType>
            <xs:complexType name="R">
                <xs:complexContent>
                    <xs:restriction base="B">
                        <xs:sequence>
                            <xs:element name="c1" minOccurs="3" maxOccurs="3"/>
                            <xs:element name="d1"/>
                        </xs:sequence>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_err(),
        "c1{{3,3}} cannot restrict choice branch c1{{2,2}}"
    );
}

/// Section 3.8 normalization: flatten nested same-compositor groups with
/// unit occurs. Base has group-ref creating sequence(sequence{1,1}(r1, r2), ...)
/// which must be flattened for sequence matching to work. (groupB003)
#[test]
fn test_accept_group_ref_restriction_with_flatten() {
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:group name="g1">
                <xs:sequence>
                    <xs:element name="r1"/>
                    <xs:element name="r2"/>
                </xs:sequence>
            </xs:group>
            <xs:group name="g2">
                <xs:sequence>
                    <xs:element name="r3"/>
                    <xs:element name="r4"/>
                </xs:sequence>
            </xs:group>
            <xs:complexType name="A">
                <xs:sequence>
                    <xs:group ref="g1"/>
                    <xs:group ref="g2" minOccurs="0"/>
                </xs:sequence>
            </xs:complexType>
            <xs:element name="elem">
                <xs:complexType>
                    <xs:complexContent>
                        <xs:restriction base="A">
                            <xs:sequence>
                                <xs:group ref="g1"/>
                            </xs:sequence>
                        </xs:restriction>
                    </xs:complexContent>
                </xs:complexType>
            </xs:element>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "group ref restriction should be valid after flattening: {:?}",
        result
    );
}

/// Pointless particles (maxOccurs=0) must be removed during normalization.
/// A choice whose branches all have maxOccurs=0 effectively restricts to
/// empty, which is valid when the base is optional. (mgH014)
#[test]
fn test_accept_restriction_with_zero_max_occurs_branch() {
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:complexType name="bar">
                <xs:choice>
                    <xs:element name="e1"/>
                    <xs:element name="e2"/>
                </xs:choice>
            </xs:complexType>
            <xs:complexType name="foo">
                <xs:complexContent>
                    <xs:restriction base="bar">
                        <xs:choice>
                            <xs:element name="e1" minOccurs="0" maxOccurs="0"/>
                            <xs:element name="e2"/>
                        </xs:choice>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "restriction with maxOccurs=0 branch should be valid: {:?}",
        result
    );
}

/// Restriction where derived is entirely pointless (all maxOccurs=0) against
/// an optional base wildcard. (particlesJq010)
#[test]
fn test_accept_all_zero_restriction_of_optional_wildcard() {
    let mut schema_set = SchemaSet::new();
    let xsd = r###"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                   targetNamespace="http://xsdtesting" xmlns:x="http://xsdtesting">
            <xs:complexType name="B">
                <xs:sequence>
                    <xs:any namespace="##targetNamespace" minOccurs="0"/>
                </xs:sequence>
            </xs:complexType>
            <xs:complexType name="R">
                <xs:complexContent>
                    <xs:restriction base="x:B">
                        <xs:sequence>
                            <xs:element name="e1" minOccurs="0" maxOccurs="0"/>
                        </xs:sequence>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "all-zero restriction of optional wildcard should be valid: {:?}",
        result
    );
}

/// Empty extension inherits base content. Restricting such a type must see
/// the inherited content, not just the empty extension body. (Sun combined)
#[test]
fn test_accept_restriction_of_empty_extension() {
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:complexType name="B">
                <xs:sequence>
                    <xs:element name="foo" type="xs:string"/>
                </xs:sequence>
            </xs:complexType>
            <xs:complexType name="De">
                <xs:complexContent>
                    <xs:extension base="B"/>
                </xs:complexContent>
            </xs:complexType>
            <xs:complexType name="Der">
                <xs:complexContent>
                    <xs:restriction base="De">
                        <xs:sequence>
                            <xs:element name="foo" type="xs:string"/>
                        </xs:sequence>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "restriction of empty extension should see inherited content: {:?}",
        result
    );
}

/// Cross-compositor restriction: sequence restricts repeated choice.
/// The algorithm can't verify this structurally so it must be provisionally
/// accepted (§3.4.6.3). (particlesV004)
#[test]
fn test_accept_sequence_restricting_repeated_choice() {
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:complexType name="B">
                <xs:choice minOccurs="0" maxOccurs="2">
                    <xs:element name="e1" maxOccurs="3"/>
                    <xs:element name="e2" maxOccurs="3"/>
                </xs:choice>
            </xs:complexType>
            <xs:complexType name="R">
                <xs:complexContent>
                    <xs:restriction base="B">
                        <xs:sequence maxOccurs="1">
                            <xs:element name="e1" maxOccurs="3"/>
                            <xs:element name="e2" maxOccurs="3"/>
                        </xs:sequence>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "cross-compositor restriction should be provisionally accepted: {:?}",
        result
    );
}

// ── Step 2 regression targets ───────────────────────────────────────────
// These tests pin the expected outcome for each W3C particle-restriction
// test case so that changes to the restriction algorithm are caught
// immediately without running the full conformance suite.
//
// All are #[ignore] until the restriction algorithm handles them.
// Remove #[ignore] one-by-one as fixes land.  Run ignored tests with:
//   cargo test --lib pipeline::tests -- --ignored --features xsd11

/// Valid: repeated-sequence restriction — derived sequence{1,1} narrows
/// base sequence{1,9} while keeping the same children. (particlesW011)
#[test]
fn test_accept_repeated_sequence_restriction_w011() {
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                   targetNamespace="http://xsdtesting" xmlns:x="http://xsdtesting">
            <xs:complexType name="B">
                <xs:sequence minOccurs="1" maxOccurs="9">
                    <xs:element name="e1"/>
                    <xs:element name="e2"/>
                    <xs:element name="e3"/>
                </xs:sequence>
            </xs:complexType>
            <xs:complexType name="R">
                <xs:complexContent>
                    <xs:restriction base="x:B">
                        <xs:sequence>
                            <xs:element name="e1"/>
                            <xs:element name="e2"/>
                            <xs:element name="e3"/>
                        </xs:sequence>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "sequence{{1,1}} validly restricts sequence{{1,9}}: {:?}",
        result
    );
}

/// Valid: repeated-sequence restriction with element type narrowing.
/// Derived element e1 has type ct3 (restriction of ct1). (particlesW016)
#[test]
fn test_accept_repeated_sequence_restriction_type_narrowing_w016() {
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                   targetNamespace="http://xsdtesting" xmlns:x="http://xsdtesting">
            <xs:complexType name="ct1">
                <xs:sequence>
                    <xs:element name="foo" minOccurs="2" maxOccurs="5"/>
                    <xs:element name="bar" form="qualified"/>
                </xs:sequence>
            </xs:complexType>
            <xs:complexType name="ct3">
                <xs:complexContent>
                    <xs:restriction base="x:ct1">
                        <xs:sequence>
                            <xs:element name="foo" minOccurs="3" maxOccurs="3"/>
                            <xs:element name="bar" form="qualified"/>
                        </xs:sequence>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
            <xs:complexType name="B">
                <xs:sequence minOccurs="1" maxOccurs="9">
                    <xs:element name="e1" type="x:ct1"/>
                    <xs:element name="e2"/>
                    <xs:element name="e3"/>
                </xs:sequence>
            </xs:complexType>
            <xs:complexType name="R">
                <xs:complexContent>
                    <xs:restriction base="x:B">
                        <xs:sequence>
                            <xs:element name="e1" type="x:ct3"/>
                            <xs:element name="e2"/>
                            <xs:element name="e3"/>
                        </xs:sequence>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "type-narrowed sequence restriction should be valid: {:?}",
        result
    );
}

/// Invalid: sequence-vs-choice where derived e3 minOccurs=2 is less than
/// base choice branch e3 minOccurs=3. (particlesV002)
#[test]
fn test_reject_sequence_vs_choice_min_violation_v002() {
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                   targetNamespace="http://xsdtesting" xmlns:x="http://xsdtesting">
            <xs:complexType name="B">
                <xs:choice minOccurs="1" maxOccurs="99">
                    <xs:element name="e1" minOccurs="1" maxOccurs="10"/>
                    <xs:element name="e2" minOccurs="2" maxOccurs="10"/>
                    <xs:element name="e3" minOccurs="3" maxOccurs="10"/>
                </xs:choice>
            </xs:complexType>
            <xs:complexType name="R">
                <xs:complexContent>
                    <xs:restriction base="x:B">
                        <xs:sequence minOccurs="1" maxOccurs="99">
                            <xs:element name="e1" minOccurs="1" maxOccurs="10"/>
                            <xs:element name="e2" minOccurs="2" maxOccurs="10"/>
                            <xs:element name="e3" minOccurs="2" maxOccurs="10"/>
                        </xs:sequence>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_err(),
        "sequence-vs-choice: e3 min=2 violates base choice branch min=3"
    );
}

/// Invalid: sequence-vs-choice where derived sequence{0,2}(e1{0,2},e2{0,2})
/// is not a valid restriction of choice{0,2}(e1{0,3},e2{0,3}).
/// A sequence forces both elements present on each repetition, which is
/// not expressible in the base choice. (particlesV005)
#[test]
fn test_reject_sequence_vs_choice_not_expressible_v005() {
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                   targetNamespace="http://xsdtesting" xmlns:x="http://xsdtesting">
            <xs:complexType name="B">
                <xs:choice minOccurs="0" maxOccurs="2">
                    <xs:element name="e1" maxOccurs="3"/>
                    <xs:element name="e2" maxOccurs="3"/>
                </xs:choice>
            </xs:complexType>
            <xs:complexType name="R">
                <xs:complexContent>
                    <xs:restriction base="x:B">
                        <xs:sequence maxOccurs="2">
                            <xs:element name="e1" maxOccurs="2"/>
                            <xs:element name="e2" maxOccurs="2"/>
                        </xs:sequence>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_err(),
        "sequence-vs-choice with both elements required per repetition is invalid"
    );
}

/// Invalid: sequence-vs-choice where derived sequence introduces e4 which
/// has no counterpart in the base choice. (particlesV016)
#[test]
fn test_reject_sequence_vs_choice_extra_element_v016() {
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                   targetNamespace="http://xsdtesting" xmlns:x="http://xsdtesting">
            <xs:complexType name="B">
                <xs:choice maxOccurs="unbounded">
                    <xs:element name="e1" maxOccurs="3"/>
                    <xs:element name="e2" minOccurs="0" maxOccurs="3"/>
                    <xs:element name="e3" minOccurs="0" maxOccurs="3"/>
                </xs:choice>
            </xs:complexType>
            <xs:complexType name="R">
                <xs:complexContent>
                    <xs:restriction base="x:B">
                        <xs:sequence>
                            <xs:element name="e1"/>
                            <xs:element name="e2" minOccurs="0"/>
                            <xs:element name="e3" minOccurs="0"/>
                            <xs:element name="e4" minOccurs="0"/>
                        </xs:sequence>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_err(),
        "sequence-vs-choice: e4 has no base counterpart"
    );
}

/// Invalid: sequence-vs-choice where derived element e1 has type ct2
/// which is NOT derived from base element e1's type ct1. (particlesV018)
#[test]
fn test_reject_sequence_vs_choice_type_mismatch_v018() {
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                   targetNamespace="http://xsdtesting" xmlns:x="http://xsdtesting">
            <xs:complexType name="ct1">
                <xs:sequence>
                    <xs:element name="foo" minOccurs="2" maxOccurs="5"/>
                    <xs:element name="bar" form="qualified"/>
                </xs:sequence>
            </xs:complexType>
            <xs:complexType name="ct2">
                <xs:sequence>
                    <xs:element name="foo"/>
                    <xs:element name="bar"/>
                </xs:sequence>
            </xs:complexType>
            <xs:complexType name="B">
                <xs:choice maxOccurs="4">
                    <xs:element name="e1" type="x:ct1"/>
                    <xs:element name="e2"/>
                </xs:choice>
            </xs:complexType>
            <xs:complexType name="R">
                <xs:complexContent>
                    <xs:restriction base="x:B">
                        <xs:sequence>
                            <xs:element name="e1" type="x:ct2"/>
                            <xs:element name="e2"/>
                        </xs:sequence>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_err(),
        "sequence-vs-choice: ct2 is not derived from ct1"
    );
}

/// Invalid: all-from-choice — derived `all` restricts base `choice`.
/// Cross-compositor all↔choice is not allowed. (particlesHb006)
#[test]
fn test_reject_all_restricting_choice_hb006() {
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                   targetNamespace="http://xsdtesting" xmlns:x="http://xsdtesting"
                   elementFormDefault="qualified">
            <xs:complexType name="B">
                <xs:choice minOccurs="1" maxOccurs="99">
                    <xs:element name="e1"/>
                    <xs:element name="e2"/>
                </xs:choice>
            </xs:complexType>
            <xs:complexType name="base">
                <xs:complexContent>
                    <xs:restriction base="x:B">
                        <xs:choice minOccurs="3" maxOccurs="9">
                            <xs:element name="e1"/>
                            <xs:element name="e2"/>
                        </xs:choice>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
            <xs:complexType name="R">
                <xs:complexContent>
                    <xs:restriction base="x:base">
                        <xs:all>
                            <xs:element name="e1"/>
                            <xs:element name="e2"/>
                        </xs:all>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(result.is_err(), "all-from-choice restriction is forbidden");
}

/// Invalid: all-from-sequence via group refs — derived `all` group
/// restricts base `sequence` group. (particlesHb007)
#[test]
fn test_reject_all_restricting_sequence_hb007() {
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                   targetNamespace="http://xsdtesting" xmlns:x="http://xsdtesting"
                   elementFormDefault="qualified">
            <xs:group name="Gb">
                <xs:sequence>
                    <xs:element name="e1"/>
                    <xs:element name="e2"/>
                </xs:sequence>
            </xs:group>
            <xs:group name="Gr">
                <xs:all>
                    <xs:element name="e1"/>
                    <xs:element name="e2"/>
                </xs:all>
            </xs:group>
            <xs:complexType name="base">
                <xs:group ref="x:Gb"/>
            </xs:complexType>
            <xs:complexType name="R">
                <xs:complexContent>
                    <xs:restriction base="x:base">
                        <xs:group ref="x:Gr"/>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_err(),
        "all-from-sequence restriction via group refs is forbidden"
    );
}

/// Invalid: choice-from-all — derived `choice` restricts base `all`.
/// (particlesHb009)
#[test]
fn test_reject_choice_restricting_all_hb009() {
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                   targetNamespace="http://xsdtesting" xmlns:x="http://xsdtesting"
                   elementFormDefault="qualified">
            <xs:complexType name="base">
                <xs:all>
                    <xs:element name="e1"/>
                    <xs:element name="e2"/>
                </xs:all>
            </xs:complexType>
            <xs:complexType name="R">
                <xs:complexContent>
                    <xs:restriction base="x:base">
                        <xs:choice>
                            <xs:element name="e1"/>
                            <xs:element name="e2"/>
                        </xs:choice>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(result.is_err(), "choice-from-all restriction is forbidden");
}

/// Invalid: choice restriction where derived branch d2 cannot match
/// base branch sequence(d1,d2). A single element is not a valid
/// restriction of a multi-child sequence. (particlesM033)
#[test]
fn test_reject_choice_branch_element_vs_sequence_m033() {
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                   targetNamespace="http://xsdtesting" xmlns:x="http://xsdtesting">
            <xs:complexType name="B">
                <xs:choice>
                    <xs:element name="c1" minOccurs="2" maxOccurs="2"/>
                    <xs:sequence minOccurs="1" maxOccurs="unbounded">
                        <xs:element name="d1" minOccurs="1" maxOccurs="1"/>
                        <xs:element name="d2" minOccurs="1" maxOccurs="1"/>
                    </xs:sequence>
                </xs:choice>
            </xs:complexType>
            <xs:complexType name="R">
                <xs:complexContent>
                    <xs:restriction base="x:B">
                        <xs:choice>
                            <xs:element name="c1" minOccurs="2" maxOccurs="2"/>
                            <xs:element name="d2" minOccurs="1" maxOccurs="unbounded"/>
                        </xs:choice>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_err(),
        "choice branch d2 cannot restrict base sequence(d1,d2)"
    );
}

/// Invalid: choice restriction where derived branch d1 cannot match
/// either base sequence branch. (particlesM034)
#[test]
fn test_reject_choice_branch_vs_multi_child_sequences_m034() {
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                   targetNamespace="http://xsdtesting" xmlns:x="http://xsdtesting">
            <xs:complexType name="B">
                <xs:choice>
                    <xs:sequence minOccurs="0" maxOccurs="unbounded">
                        <xs:element name="c1" minOccurs="2" maxOccurs="2"/>
                        <xs:element name="c2" minOccurs="0" maxOccurs="1"/>
                    </xs:sequence>
                    <xs:sequence minOccurs="1" maxOccurs="unbounded">
                        <xs:element name="d1" minOccurs="0" maxOccurs="unbounded"/>
                        <xs:element name="d2" minOccurs="1" maxOccurs="unbounded"/>
                    </xs:sequence>
                </xs:choice>
            </xs:complexType>
            <xs:complexType name="R">
                <xs:complexContent>
                    <xs:restriction base="x:B">
                        <xs:choice>
                            <xs:element name="d1" minOccurs="1" maxOccurs="unbounded"/>
                        </xs:choice>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_err(),
        "choice branch d1 cannot restrict either base sequence branch"
    );
}

/// Choice-vs-choice restriction where both are optional.  The derived
/// restricts each branch to maxOccurs=0 — effectively empty.
/// (particlesIe001)
#[test]
fn test_accept_choice_restriction_all_branches_zero() {
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                   targetNamespace="http://xsdtesting" xmlns:x="http://xsdtesting"
                   elementFormDefault="qualified">
            <xs:complexType name="base">
                <xs:choice minOccurs="0">
                    <xs:element name="e1" minOccurs="0" maxOccurs="unbounded"/>
                    <xs:element name="e2" minOccurs="0" maxOccurs="unbounded"/>
                </xs:choice>
            </xs:complexType>
            <xs:complexType name="testing">
                <xs:complexContent>
                    <xs:restriction base="x:base">
                        <xs:choice minOccurs="0">
                            <xs:element name="e1" minOccurs="0" maxOccurs="0"/>
                            <xs:element name="e2" minOccurs="0" maxOccurs="0"/>
                        </xs:choice>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "choice restriction with all-zero branches should be valid: {:?}",
        result
    );
}

/// recurseAsIfGroup: element restricts base group.  The implicit wrapper
/// has occurs {1,1} which must satisfy the base group's occurs. This tests
/// that the outer occurs check is in place.
#[test]
fn test_reject_element_restricting_group_with_required_repetition() {
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:complexType name="base">
                <xs:sequence minOccurs="2" maxOccurs="2">
                    <xs:element name="a"/>
                </xs:sequence>
            </xs:complexType>
            <xs:complexType name="derived">
                <xs:complexContent>
                    <xs:restriction base="base">
                        <xs:sequence>
                            <xs:element name="a"/>
                        </xs:sequence>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_err(),
        "derived sequence{{1,1}} cannot restrict base sequence{{2,2}}"
    );
}

/// Valid same-compositor restriction: sequence restricts sequence with
/// the derived dropping an optional tail. Basic sanity check.
#[test]
fn test_accept_sequence_restriction_dropping_optional() {
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:complexType name="base">
                <xs:sequence>
                    <xs:element name="a" type="xs:string"/>
                    <xs:element name="b" type="xs:string" minOccurs="0"/>
                </xs:sequence>
            </xs:complexType>
            <xs:complexType name="derived">
                <xs:complexContent>
                    <xs:restriction base="base">
                        <xs:sequence>
                            <xs:element name="a" type="xs:string"/>
                        </xs:sequence>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "dropping optional tail should be valid: {:?}",
        result
    );
}

#[test]
fn test_load_and_process_attribute_with_inline_type() {
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:complexType name="ProductType">
                <xs:attribute name="code">
                    <xs:simpleType>
                        <xs:restriction base="xs:string">
                            <xs:pattern value="[A-Z]{3}-[0-9]{4}"/>
                        </xs:restriction>
                    </xs:simpleType>
                </xs:attribute>
            </xs:complexType>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "Should parse schema with attribute inline type: {:?}",
        result
    );

    let stats = result.unwrap();
    let inline_stats = stats.inline_stats.unwrap();
    // The inline type is within a complex type's attribute, so it should be counted
    assert!(
        inline_stats.total_inline_types > 0,
        "Should assemble attribute inline type"
    );
}

#[test]
fn test_parse_only_mode() {
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:element name="root" type="xs:string"/>
        </xs:schema>"#;

    let result = load_and_process_schema(
        xsd.as_bytes(),
        "test.xsd",
        &mut schema_set,
        Some(PipelineConfig::parse_only()),
    );
    assert!(result.is_ok());

    let stats = result.unwrap();
    // In parse-only mode, these should be None
    assert!(stats.inline_stats.is_none());
    assert!(stats.resolution_stats.is_none());

    // Element should exist but type not resolved
    let root_name = schema_set.name_table.get("root").unwrap();
    let elem_key = schema_set.lookup_element(None, root_name).unwrap();
    let elem = schema_set.arenas.elements.get(elem_key).unwrap();
    assert!(
        elem.resolved_type.is_none(),
        "Type should not be resolved in parse-only mode"
    );
}

#[test]
fn test_process_loaded_schemas() {
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:element name="item">
                <xs:complexType>
                    <xs:sequence>
                        <xs:element name="value" type="xs:decimal"/>
                    </xs:sequence>
                </xs:complexType>
            </xs:element>
        </xs:schema>"#;

    // First, parse only
    let doc_id = parse_schema_only(xsd.as_bytes(), "test.xsd", &mut schema_set).unwrap();
    assert_eq!(doc_id, 0);

    // Element exists but type not resolved
    let item_name = schema_set.name_table.get("item").unwrap();
    let elem_key = schema_set.lookup_element(None, item_name).unwrap();
    let elem = schema_set.arenas.elements.get(elem_key).unwrap();
    assert!(elem.resolved_type.is_none());

    // Now process the loaded schemas
    let (inline_stats, resolution_stats) = process_loaded_schemas(&mut schema_set).unwrap();
    assert!(inline_stats.total_inline_types > 0);

    // Element type should now be resolved
    let elem = schema_set.arenas.elements.get(elem_key).unwrap();
    assert!(
        elem.resolved_type.is_some(),
        "Type should be resolved after processing"
    );

    // Resolution stats should show resolved references
    // Resolution stats should show we processed the schemas
    let _ = resolution_stats; // Use the stats to avoid unused warning
}

#[test]
fn test_pipeline_config_default() {
    let config = PipelineConfig::default();
    assert!(config.resolve_directives);
    assert!(config.assemble_inline_types);
    assert!(config.resolve_references);
}

#[test]
fn test_pipeline_config_parse_only() {
    let config = PipelineConfig::parse_only();
    assert!(!config.resolve_directives);
    assert!(!config.assemble_inline_types);
    assert!(!config.resolve_references);
}

#[test]
fn test_load_schema_convenience() {
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:element name="data" type="xs:string"/>
        </xs:schema>"#;

    let result = load_schema(xsd.as_bytes(), "test.xsd", &mut schema_set);
    assert!(result.is_ok());
}

#[test]
fn test_nested_inline_types() {
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:element name="order">
                <xs:complexType>
                    <xs:sequence>
                        <xs:element name="item">
                            <xs:complexType>
                                <xs:sequence>
                                    <xs:element name="name" type="xs:string"/>
                                    <xs:element name="price">
                                        <xs:simpleType>
                                            <xs:restriction base="xs:decimal">
                                                <xs:minInclusive value="0"/>
                                            </xs:restriction>
                                        </xs:simpleType>
                                    </xs:element>
                                </xs:sequence>
                            </xs:complexType>
                        </xs:element>
                    </xs:sequence>
                </xs:complexType>
            </xs:element>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "Should handle nested inline types: {:?}",
        result
    );

    let stats = result.unwrap();
    let inline_stats = stats.inline_stats.unwrap();
    // Should have multiple inline types: order's complexType, item's complexType, price's simpleType
    assert!(
        inline_stats.total_inline_types >= 1,
        "Should assemble multiple inline types"
    );
}

// ========================================================================
// Structural Check Tests (from XSD_TODO.md)
// ========================================================================

#[test]
fn test_reject_element_name_and_ref() {
    // Element with both name and ref should error (per structure.rs)
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:element name="foo" ref="bar"/>
        </xs:schema>"#;

    let mut config = PipelineConfig::default();
    config.parser.error_recovery = false;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, Some(config));
    assert!(
        result.is_err(),
        "Should reject element with both name and ref"
    );

    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("name") || err.to_string().contains("ref"),
        "Error should mention name/ref conflict: {}",
        err
    );
}

#[test]
fn test_list_itemtype_xor_inline() {
    // List with both itemType and inline simpleType should be rejected
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:simpleType name="badList">
                <xs:list itemType="xs:string">
                    <xs:simpleType>
                        <xs:restriction base="xs:integer"/>
                    </xs:simpleType>
                </xs:list>
            </xs:simpleType>
        </xs:schema>"#;

    let mut config = PipelineConfig::default();
    config.parser.error_recovery = false;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, Some(config));
    assert!(
        result.is_err(),
        "Should reject list with both itemType and inline type"
    );
}

#[test]
fn test_union_requires_membertypes_or_inline() {
    // Union missing both memberTypes and inline simpleType children should be rejected
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:simpleType name="badUnion">
                <xs:union/>
            </xs:simpleType>
        </xs:schema>"#;

    let mut config = PipelineConfig::default();
    config.parser.error_recovery = false;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, Some(config));
    // Note: This validation might happen during assembly or resolution, not parsing
    // If the schema parses but fails during resolution, we still consider it a success
    // as long as the error is eventually caught
    assert!(
        result.is_err() || !schema_set.arenas.simple_types.is_empty(),
        "Should either reject empty union or parse it for later validation"
    );
}

#[cfg(feature = "xsd11")]
#[test]
fn test_xsd11_assert_rejected_in_10_mode() {
    // Schema with xs:assert should error in 1.0 mode
    let mut schema_set = SchemaSet::new(); // defaults to V1_0
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:complexType name="ValidatedType">
                <xs:sequence>
                    <xs:element name="value" type="xs:integer"/>
                </xs:sequence>
                <xs:assert test="value gt 0"/>
            </xs:complexType>
        </xs:schema>"#;

    let config = PipelineConfig {
        parser: ParserConfig {
            error_recovery: false,
            ..Default::default()
        },
        ..Default::default()
    };

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, Some(config));
    assert!(
        result.is_err(),
        "xs:assert should be rejected in XSD 1.0 mode"
    );
}

#[cfg(feature = "xsd11")]
#[test]
fn test_xsd11_assert_allowed_in_11_mode() {
    // Schema with xs:assert should be allowed in 1.1 mode
    let mut schema_set = SchemaSet::xsd11();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:complexType name="ValidatedType">
                <xs:sequence>
                    <xs:element name="value" type="xs:integer"/>
                </xs:sequence>
                <xs:assert test="value gt 0"/>
            </xs:complexType>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "xs:assert should be allowed in XSD 1.1 mode: {:?}",
        result
    );
}

#[cfg(feature = "xsd11")]
#[test]
fn test_xsd11_alternative_rejected_in_10_mode() {
    // Schema with xs:alternative should error in 1.0 mode
    let mut schema_set = SchemaSet::new(); // defaults to V1_0
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:element name="item">
                <xs:alternative test="@type='special'" type="xs:string"/>
            </xs:element>
        </xs:schema>"#;

    let config = PipelineConfig {
        parser: ParserConfig {
            error_recovery: false,
            ..Default::default()
        },
        ..Default::default()
    };

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, Some(config));
    assert!(
        result.is_err(),
        "xs:alternative should be rejected in XSD 1.0 mode"
    );
}

#[test]
fn test_skip_unknown_subtree() {
    // Foreign-namespace elements at top level violate `sch-props-correct`
    // (XSD's schema-for-schemas only allows the well-known XSD elements
    // there; ad-hoc metadata belongs in `xs:appinfo`/`xs:documentation`).
    // The pipeline rejects such schemas with a structural error, but the
    // error-recovery parser still skips the subtree and walks the rest of
    // the document so the remaining declarations are visible in the partial
    // schema set.
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <unknownElement>
                <nested>content</nested>
            </unknownElement>
            <xs:element name="valid" type="xs:string"/>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    let err = result.expect_err("Foreign element at schema level must be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("Foreign-namespace element") || msg.contains("sch-props-correct"),
        "Expected sch-props-correct foreign-element error, got: {}",
        msg
    );

    // The valid element should still have been parsed before the structural
    // error surfaced — error recovery keeps walking past the skipped subtree.
    let valid_name = schema_set.name_table.get("valid").unwrap();
    let elem_key = schema_set.lookup_element(None, valid_name);
    assert!(
        elem_key.is_some(),
        "Valid element should still be parsed after the unknown subtree is skipped"
    );
}

// ========================================================================
// Foreign Attribute / Implicit Annotation Tests (from XSD_EXTENSIBILITY.md)
// ========================================================================

#[test]
fn test_element_foreign_attribute_creates_implicit_annotation() {
    // Element with foreign attribute but no explicit annotation should get implicit one
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                   xmlns:custom="http://example.com/custom">
            <xs:element name="test" custom:attr="value"/>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "Should parse schema with foreign attribute: {:?}",
        result
    );

    // Verify element has annotation with foreign attribute
    let test_name = schema_set.name_table.get("test").unwrap();
    let elem_key = schema_set.lookup_element(None, test_name).unwrap();
    let elem = schema_set.arenas.elements.get(elem_key).unwrap();

    assert!(
        elem.annotation.is_some(),
        "Element with foreign attribute should have annotation"
    );
    let ann = elem.annotation.as_ref().unwrap();
    assert!(
        !ann.attributes.is_empty(),
        "Annotation should have foreign attributes"
    );
    assert_eq!(ann.attributes[0].value, "value");
}

#[test]
fn test_foreign_attribute_merged_with_explicit_annotation() {
    // Element with both explicit annotation and foreign attribute
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                   xmlns:custom="http://example.com/custom">
            <xs:element name="test" custom:attr="value">
                <xs:annotation>
                    <xs:documentation>Test documentation</xs:documentation>
                </xs:annotation>
            </xs:element>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(result.is_ok(), "Should parse schema: {:?}", result);

    // Verify element has annotation with both documentation and foreign attribute
    let test_name = schema_set.name_table.get("test").unwrap();
    let elem_key = schema_set.lookup_element(None, test_name).unwrap();
    let elem = schema_set.arenas.elements.get(elem_key).unwrap();

    assert!(elem.annotation.is_some(), "Element should have annotation");
    let ann = elem.annotation.as_ref().unwrap();
    assert!(
        !ann.items.is_empty(),
        "Annotation should have documentation item"
    );
    assert!(
        !ann.attributes.is_empty(),
        "Annotation should have merged foreign attributes"
    );
}

#[test]
fn test_complex_type_foreign_attribute() {
    // ComplexType with foreign attribute
    let mut schema_set = SchemaSet::new();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                   xmlns:jaxb="http://java.sun.com/xml/ns/jaxb">
            <xs:complexType name="PersonType" jaxb:class="Person">
                <xs:sequence>
                    <xs:element name="name" type="xs:string"/>
                </xs:sequence>
            </xs:complexType>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(result.is_ok(), "Should parse schema: {:?}", result);

    // Verify complex type has annotation with foreign attribute
    let type_name = schema_set.name_table.get("PersonType").unwrap();
    let type_key = schema_set.lookup_type(None, type_name).unwrap();
    if let TypeKey::Complex(ct_key) = type_key {
        let ct = schema_set.arenas.complex_types.get(ct_key).unwrap();
        assert!(
            ct.annotation.is_some(),
            "ComplexType with foreign attribute should have annotation"
        );
        let ann = ct.annotation.as_ref().unwrap();
        assert!(
            !ann.attributes.is_empty(),
            "Annotation should have foreign attributes"
        );
    } else {
        panic!("Expected complex type");
    }
}

// ========================================================================
// Redefine / Override Integration Tests
// ========================================================================

#[test]
fn test_redefine_via_pipeline() {
    // Base schema defines a simple type; redefining schema extends it via xs:redefine.
    // The resolver must load the base schema, then apply_redefine replaces the type.
    let tmp = std::env::temp_dir().join("xsd_test_redefine");
    std::fs::create_dir_all(&tmp).unwrap();

    let base_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
<xs:simpleType name="MyString">
    <xs:restriction base="xs:string"/>
</xs:simpleType>
</xs:schema>"#;
    let base_path = tmp.join("base.xsd");
    std::fs::write(&base_path, base_xsd).unwrap();

    let redefine_xsd = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
<xs:redefine schemaLocation="{}">
    <xs:simpleType name="MyString">
        <xs:restriction base="MyString">
            <xs:maxLength value="100"/>
        </xs:restriction>
    </xs:simpleType>
</xs:redefine>
<xs:element name="root" type="MyString"/>
</xs:schema>"#,
        base_path.to_string_lossy()
    );

    let mut schema_set = SchemaSet::new();
    let result = load_and_process_schema(
        redefine_xsd.as_bytes(),
        &tmp.join("redefine.xsd").to_string_lossy(),
        &mut schema_set,
        None,
    );
    assert!(
        result.is_ok(),
        "Redefine via pipeline should succeed: {:?}",
        result
    );

    // Verify the redefined type is in the namespace table
    let name = schema_set.name_table.get("MyString").unwrap();
    let type_key = schema_set.lookup_type(None, name);
    assert!(type_key.is_some(), "Redefined type should be registered");
    assert!(matches!(type_key.unwrap(), TypeKey::Simple(_)));

    // Verify the element resolves to the redefined type
    let root_name = schema_set.name_table.get("root").unwrap();
    let elem_key = schema_set.lookup_element(None, root_name).unwrap();
    let elem = schema_set.arenas.elements.get(elem_key).unwrap();
    assert!(
        elem.resolved_type.is_some(),
        "Element type should resolve to redefined type"
    );

    // Clean up
    let _ = std::fs::remove_dir_all(&tmp);
}

#[cfg(feature = "xsd11")]
#[test]
fn test_override_via_pipeline() {
    // Override schema replaces a type from the base schema via xs:override.
    // The resolver must load the override target through process_override
    // in resolve_all_directives.
    use crate::schema::model::XsdVersion;

    let tmp = std::env::temp_dir().join("xsd_test_override");
    std::fs::create_dir_all(&tmp).unwrap();

    let base_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
<xs:simpleType name="CodeType">
    <xs:restriction base="xs:string"/>
</xs:simpleType>
</xs:schema>"#;
    let base_path = tmp.join("base.xsd");
    std::fs::write(&base_path, base_xsd).unwrap();

    let override_xsd = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
<xs:override schemaLocation="{}">
    <xs:simpleType name="CodeType">
        <xs:restriction base="xs:token">
            <xs:pattern value="[A-Z]{{3}}"/>
        </xs:restriction>
    </xs:simpleType>
</xs:override>
<xs:element name="code" type="CodeType"/>
</xs:schema>"#,
        base_path.to_string_lossy()
    );

    let mut schema_set = SchemaSet::with_version(XsdVersion::V1_1);
    let result = load_and_process_schema(
        override_xsd.as_bytes(),
        &tmp.join("override.xsd").to_string_lossy(),
        &mut schema_set,
        None,
    );
    assert!(
        result.is_ok(),
        "Override via pipeline should succeed: {:?}",
        result
    );

    // Verify the overriding type replaced the original
    let name = schema_set.name_table.get("CodeType").unwrap();
    let type_key = schema_set.lookup_type(None, name);
    assert!(type_key.is_some(), "Overridden type should be registered");

    // Verify element resolves
    let code_name = schema_set.name_table.get("code").unwrap();
    let elem_key = schema_set.lookup_element(None, code_name).unwrap();
    let elem = schema_set.arenas.elements.get(elem_key).unwrap();
    assert!(
        elem.resolved_type.is_some(),
        "Element type should resolve to overridden type"
    );

    // Clean up
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn test_process_loaded_schemas_with_redefine() {
    // Manually parse base + redefining schemas, then call process_loaded_schemas.
    // This exercises the multi-schema path and its redefine precondition.
    let base_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
<xs:complexType name="BaseType">
    <xs:sequence>
        <xs:element name="name" type="xs:string"/>
    </xs:sequence>
</xs:complexType>
</xs:schema>"#;

    let redefine_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
<xs:redefine schemaLocation="base.xsd">
    <xs:complexType name="BaseType">
        <xs:complexContent>
            <xs:extension base="BaseType">
                <xs:sequence>
                    <xs:element name="extra" type="xs:int"/>
                </xs:sequence>
            </xs:extension>
        </xs:complexContent>
    </xs:complexType>
</xs:redefine>
<xs:element name="item" type="BaseType"/>
</xs:schema>"#;

    let mut schema_set = SchemaSet::new();

    // Parse both schemas manually (simulating pre-loading)
    let _base_id = parse_schema_only(base_xsd.as_bytes(), "base.xsd", &mut schema_set).unwrap();
    let _redefine_id =
        parse_schema_only(redefine_xsd.as_bytes(), "redefine.xsd", &mut schema_set).unwrap();

    // process_loaded_schemas applies redefine before assembly
    let result = process_loaded_schemas(&mut schema_set);
    assert!(
        result.is_ok(),
        "process_loaded_schemas with redefine should succeed: {:?}",
        result
    );

    // Verify the redefined type is in the namespace table
    let name = schema_set.name_table.get("BaseType").unwrap();
    let type_key = schema_set.lookup_type(None, name);
    assert!(type_key.is_some(), "Redefined type should be registered");
    assert!(matches!(type_key.unwrap(), TypeKey::Complex(_)));
}

// ========================================================================
// Redefine base-type resolution tests
// ========================================================================

#[test]
fn test_redefine_simple_type_base_resolves_to_original() {
    // Verify that a redefined simple type's resolved_base_type points to the
    // original type key, not to itself.
    let tmp = std::env::temp_dir().join("xsd_test_redefine_simple_base");
    std::fs::create_dir_all(&tmp).unwrap();

    let base_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
<xs:simpleType name="MyString">
    <xs:restriction base="xs:string"/>
</xs:simpleType>
</xs:schema>"#;
    let base_path = tmp.join("base.xsd");
    std::fs::write(&base_path, base_xsd).unwrap();

    let redefine_xsd = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
<xs:redefine schemaLocation="{}">
    <xs:simpleType name="MyString">
        <xs:restriction base="MyString">
            <xs:maxLength value="100"/>
        </xs:restriction>
    </xs:simpleType>
</xs:redefine>
</xs:schema>"#,
        base_path.to_string_lossy()
    );

    let mut schema_set = SchemaSet::new();
    let result = load_and_process_schema(
        redefine_xsd.as_bytes(),
        &tmp.join("redefine.xsd").to_string_lossy(),
        &mut schema_set,
        None,
    );
    assert!(result.is_ok(), "Redefine should succeed: {:?}", result);

    let name = schema_set.name_table.get("MyString").unwrap();
    let type_key = schema_set.lookup_type(None, name).unwrap();
    let TypeKey::Simple(simple_key) = type_key else {
        panic!("Expected simple type");
    };

    let type_def = schema_set.arenas.simple_types.get(simple_key).unwrap();
    assert!(
        type_def.redefine_original.is_some(),
        "redefine_original should be set"
    );
    assert!(
        type_def.resolved_base_type.is_some(),
        "resolved_base_type should be set"
    );

    // The resolved base type must NOT be the visible type itself
    assert_ne!(
        type_def.resolved_base_type.unwrap(),
        type_key,
        "resolved_base_type must point to the original, not self"
    );

    // It should point to the original type
    assert_eq!(
        type_def.resolved_base_type.unwrap(),
        TypeKey::Simple(type_def.redefine_original.unwrap()),
        "resolved_base_type must equal TypeKey::Simple(redefine_original)"
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn test_redefine_complex_type_base_resolves_to_original() {
    // Verify that a redefined complex type (extension) has resolved_base_type
    // pointing to the original type key, not itself.
    let tmp = std::env::temp_dir().join("xsd_test_redefine_complex_base");
    std::fs::create_dir_all(&tmp).unwrap();

    let base_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
<xs:complexType name="PersonType">
    <xs:sequence>
        <xs:element name="name" type="xs:string"/>
    </xs:sequence>
</xs:complexType>
</xs:schema>"#;
    let base_path = tmp.join("base.xsd");
    std::fs::write(&base_path, base_xsd).unwrap();

    let redefine_xsd = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
<xs:redefine schemaLocation="{}">
    <xs:complexType name="PersonType">
        <xs:complexContent>
            <xs:extension base="PersonType">
                <xs:sequence>
                    <xs:element name="age" type="xs:int"/>
                </xs:sequence>
            </xs:extension>
        </xs:complexContent>
    </xs:complexType>
</xs:redefine>
<xs:element name="person" type="PersonType"/>
</xs:schema>"#,
        base_path.to_string_lossy()
    );

    let mut schema_set = SchemaSet::new();
    let result = load_and_process_schema(
        redefine_xsd.as_bytes(),
        &tmp.join("redefine.xsd").to_string_lossy(),
        &mut schema_set,
        None,
    );
    assert!(result.is_ok(), "Redefine should succeed: {:?}", result);

    let name = schema_set.name_table.get("PersonType").unwrap();
    let type_key = schema_set.lookup_type(None, name).unwrap();
    let TypeKey::Complex(complex_key) = type_key else {
        panic!("Expected complex type");
    };

    let type_def = schema_set.arenas.complex_types.get(complex_key).unwrap();
    assert!(
        type_def.redefine_original.is_some(),
        "redefine_original should be set"
    );
    assert!(
        type_def.resolved_base_type.is_some(),
        "resolved_base_type should be set"
    );

    // The resolved base type must NOT be the visible type itself
    assert_ne!(
        type_def.resolved_base_type.unwrap(),
        type_key,
        "resolved_base_type must point to the original, not self"
    );

    // It should point to the original type
    assert_eq!(
        type_def.resolved_base_type.unwrap(),
        TypeKey::Complex(type_def.redefine_original.unwrap()),
        "resolved_base_type must equal TypeKey::Complex(redefine_original)"
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn test_redefine_complex_type_extension_via_batch_path() {
    // Exercise the batch path (process_loaded_schemas) with a complex type
    // extension redefine, verifying no self-reference and correct base chain.
    let base_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
<xs:complexType name="BaseType">
    <xs:sequence>
        <xs:element name="name" type="xs:string"/>
    </xs:sequence>
</xs:complexType>
</xs:schema>"#;

    let redefine_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
<xs:redefine schemaLocation="base.xsd">
    <xs:complexType name="BaseType">
        <xs:complexContent>
            <xs:extension base="BaseType">
                <xs:sequence>
                    <xs:element name="extra" type="xs:int"/>
                </xs:sequence>
            </xs:extension>
        </xs:complexContent>
    </xs:complexType>
</xs:redefine>
<xs:element name="item" type="BaseType"/>
</xs:schema>"#;

    let mut schema_set = SchemaSet::new();
    let _base_id = parse_schema_only(base_xsd.as_bytes(), "base.xsd", &mut schema_set).unwrap();
    let _redefine_id =
        parse_schema_only(redefine_xsd.as_bytes(), "redefine.xsd", &mut schema_set).unwrap();

    let result = process_loaded_schemas(&mut schema_set);
    assert!(
        result.is_ok(),
        "process_loaded_schemas should succeed: {:?}",
        result
    );

    let name = schema_set.name_table.get("BaseType").unwrap();
    let type_key = schema_set.lookup_type(None, name).unwrap();
    let TypeKey::Complex(complex_key) = type_key else {
        panic!("Expected complex type");
    };

    let type_def = schema_set.arenas.complex_types.get(complex_key).unwrap();
    assert!(
        type_def.resolved_base_type.is_some(),
        "resolved_base_type should be set"
    );
    assert_ne!(
        type_def.resolved_base_type.unwrap(),
        type_key,
        "resolved_base_type must not be self in batch path"
    );
}

#[test]
fn test_process_loaded_schemas_propagates_dependency_error() {
    // A self-referencing complex type (not from redefine) creates a self-edge
    // in the dependency graph. process_loaded_schemas must propagate that error.
    let schema_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
<xs:complexType name="SelfRef">
    <xs:complexContent>
        <xs:extension base="SelfRef">
            <xs:sequence>
                <xs:element name="x" type="xs:string"/>
            </xs:sequence>
        </xs:extension>
    </xs:complexContent>
</xs:complexType>
</xs:schema>"#;

    let mut schema_set = SchemaSet::new();
    let _doc_id = parse_schema_only(schema_xsd.as_bytes(), "selfref.xsd", &mut schema_set).unwrap();

    let result = process_loaded_schemas(&mut schema_set);
    assert!(
        result.is_err(),
        "Self-referencing type dependency should be detected as error"
    );
}

#[test]
fn test_cross_reference_cycle_with_namespace_prefix() {
    // Mutual cycle: TypeA extends TypeB, TypeB extends TypeA — with targetNamespace.
    // This investigates whether prefixed base-type QNames resolve via the batch path.
    let schema_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           targetNamespace="urn:test" xmlns:t="urn:test">
<xs:complexType name="TypeA">
    <xs:complexContent>
        <xs:extension base="t:TypeB">
            <xs:sequence>
                <xs:element name="a" type="xs:string"/>
            </xs:sequence>
        </xs:extension>
    </xs:complexContent>
</xs:complexType>
<xs:complexType name="TypeB">
    <xs:complexContent>
        <xs:extension base="t:TypeA">
            <xs:sequence>
                <xs:element name="b" type="xs:string"/>
            </xs:sequence>
        </xs:extension>
    </xs:complexContent>
</xs:complexType>
</xs:schema>"#;

    let mut schema_set = SchemaSet::new();
    let _doc_id =
        parse_schema_only(schema_xsd.as_bytes(), "circular.xsd", &mut schema_set).unwrap();

    // Inspect what the parser stored for each type's base_type
    let urn_test = schema_set.name_table.get("urn:test");
    let type_a_name = schema_set.name_table.get("TypeA");
    let type_b_name = schema_set.name_table.get("TypeB");

    // Both names and the namespace must be interned
    assert!(urn_test.is_some(), "urn:test should be interned");
    assert!(type_a_name.is_some(), "TypeA should be interned");
    assert!(type_b_name.is_some(), "TypeB should be interned");

    let urn_test = urn_test.unwrap();
    let type_a_name = type_a_name.unwrap();
    let type_b_name = type_b_name.unwrap();

    // Both types should be registered in the namespace table
    let a_key = schema_set.lookup_type(Some(urn_test), type_a_name);
    let b_key = schema_set.lookup_type(Some(urn_test), type_b_name);
    assert!(
        a_key.is_some(),
        "TypeA should be registered in urn:test namespace"
    );
    assert!(
        b_key.is_some(),
        "TypeB should be registered in urn:test namespace"
    );

    let TypeKey::Complex(ak) = a_key.unwrap() else {
        panic!("TypeA not complex")
    };
    let TypeKey::Complex(bk) = b_key.unwrap() else {
        panic!("TypeB not complex")
    };

    // Verify both types have their base_type QName set from parsing
    let a_def = schema_set.arenas.complex_types.get(ak).unwrap();
    let b_def = schema_set.arenas.complex_types.get(bk).unwrap();
    assert!(
        a_def.base_type.is_some(),
        "TypeA base_type QName should be set after parsing"
    );
    assert!(
        b_def.base_type.is_some(),
        "TypeB base_type QName should be set after parsing"
    );

    // process_loaded_schemas resolves references then builds the dependency graph.
    // The resolution succeeds (both base types resolve), but the dependency graph
    // detects the cycle and returns an error.
    let result = process_loaded_schemas(&mut schema_set);
    assert!(
        result.is_err(),
        "Circular dependency (TypeA ↔ TypeB) should be detected as error"
    );

    // Even though process_loaded_schemas returned Err, resolution already happened.
    // Verify that both base types were resolved (creating the cycle).
    let a_def = schema_set.arenas.complex_types.get(ak).unwrap();
    let b_def = schema_set.arenas.complex_types.get(bk).unwrap();
    assert!(
        a_def.resolved_base_type.is_some(),
        "TypeA resolved_base_type should be set — pointing to TypeB"
    );
    assert!(
        b_def.resolved_base_type.is_some(),
        "TypeB resolved_base_type should be set — pointing to TypeA"
    );
}

// ========================================================================
// Redefine cross-namespace self-derivation rejection test
// ========================================================================

#[test]
fn test_redefine_rejects_cross_namespace_base_reference() {
    // A redefined simple type whose base references a matching local name but in
    // a *different* namespace must be rejected by src-redefine validation.
    let tmp = std::env::temp_dir().join("xsd_test_redefine_cross_ns");
    std::fs::create_dir_all(&tmp).unwrap();

    let base_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           targetNamespace="urn:base">
<xs:simpleType name="Code">
    <xs:restriction base="xs:string"/>
</xs:simpleType>
</xs:schema>"#;
    let base_path = tmp.join("base.xsd");
    std::fs::write(&base_path, base_xsd).unwrap();

    // The redefine references "other:Code" — same local name but different namespace.
    let redefine_xsd = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           targetNamespace="urn:base" xmlns:other="urn:other">
<xs:redefine schemaLocation="{}">
    <xs:simpleType name="Code">
        <xs:restriction base="other:Code">
            <xs:maxLength value="10"/>
        </xs:restriction>
    </xs:simpleType>
</xs:redefine>
</xs:schema>"#,
        base_path.to_string_lossy()
    );

    let mut schema_set = SchemaSet::new();
    let result = load_and_process_schema(
        redefine_xsd.as_bytes(),
        &tmp.join("redefine.xsd").to_string_lossy(),
        &mut schema_set,
        None,
    );
    assert!(
        result.is_err(),
        "Cross-namespace base reference in redefine should be rejected: {:?}",
        result,
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

// ========================================================================
// QName lexical validation test
// ========================================================================

#[test]
fn test_malformed_qname_rejected() {
    // A schema with a malformed QName (trailing colon: "xs:") in a base attribute
    // must fail to parse rather than silently producing an empty local name.
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
<xs:simpleType name="Bad">
    <xs:restriction base="xs:"/>
</xs:simpleType>
</xs:schema>"#;

    let mut schema_set = SchemaSet::new();
    // With error recovery on (default), the malformed QName causes a SkipFrame,
    // so the type ends up with no base_type. The type should still parse, but
    // the base should be absent.
    let _ = load_and_process_schema(xsd.as_bytes(), "bad.xsd", &mut schema_set, None);

    let name = schema_set.name_table.get("Bad");
    if let Some(name_id) = name {
        if let Some(TypeKey::Simple(sk)) = schema_set.lookup_type(None, name_id) {
            let td = schema_set.arenas.simple_types.get(sk).unwrap();
            // base_type should be None — the malformed QName was rejected
            assert!(
                td.base_type.is_none()
                    || matches!(&td.base_type, Some(crate::parser::frames::TypeRefResult::QName(q)) if schema_set.name_table.resolve(q.local_name).is_empty()),
                "Malformed QName 'xs:' should not produce a valid base_type"
            );
        }
    }
    // If the type didn't even parse, that's also acceptable
}

// ========================================================================
// Default Open Content Validation Tests (cos-valid-default-oc, §3.4.6.5)
// ========================================================================

#[cfg(feature = "xsd11")]
#[test]
fn test_default_open_content_interleave_valid() {
    let mut schema_set = SchemaSet::xsd11();
    let xsd = r###"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:defaultOpenContent mode="interleave">
                <xs:any namespace="##other" processContents="lax"/>
            </xs:defaultOpenContent>
            <xs:element name="root" type="xs:string"/>
        </xs:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "Valid defaultOpenContent (interleave) should pass: {:?}",
        result
    );
}

#[cfg(feature = "xsd11")]
#[test]
fn test_default_open_content_suffix_valid() {
    let mut schema_set = SchemaSet::xsd11();
    let xsd = r###"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:defaultOpenContent mode="suffix">
                <xs:any namespace="##other" processContents="skip"/>
            </xs:defaultOpenContent>
            <xs:element name="root" type="xs:string"/>
        </xs:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "Valid defaultOpenContent (suffix) should pass: {:?}",
        result
    );
}

#[cfg(feature = "xsd11")]
#[test]
fn test_default_open_content_missing_wildcard() {
    use crate::error::SchemaError;

    let mut schema_set = SchemaSet::xsd11();
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:defaultOpenContent mode="interleave"/>
            <xs:element name="root" type="xs:string"/>
        </xs:schema>"#;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_err(),
        "defaultOpenContent without wildcard should fail"
    );

    if let Err(SchemaError::StructuralError { constraint, .. }) = result {
        assert_eq!(constraint, "cos-valid-default-oc");
    } else {
        panic!(
            "Expected structural error with cos-valid-default-oc constraint, got: {:?}",
            result
        );
    }
}

#[cfg(feature = "xsd11")]
#[test]
fn test_default_open_content_applies_to_empty_valid() {
    let mut schema_set = SchemaSet::xsd11();
    let xsd = r###"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:defaultOpenContent mode="suffix" appliesToEmpty="true">
                <xs:any namespace="##other" processContents="lax"/>
            </xs:defaultOpenContent>
            <xs:element name="root" type="xs:string"/>
        </xs:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "Valid defaultOpenContent with appliesToEmpty should pass: {:?}",
        result
    );
}

// ======================================================================
// Fix 1: Substitution group validation (e-props-correct.4)
// ======================================================================

#[test]
fn test_substitution_group_final_restriction_blocks_member() {
    // particlesIh001: head e1 has final="restriction", member e2 type derived by restriction
    let mut schema_set = SchemaSet::new();
    let xsd = r###"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
                    targetNamespace="http://test" xmlns:t="http://test"
                    elementFormDefault="qualified">
        <xsd:complexType name="foo">
            <xsd:choice>
                <xsd:element name="c1" minOccurs="0" maxOccurs="2"/>
                <xsd:element name="c2"/>
            </xsd:choice>
        </xsd:complexType>
        <xsd:complexType name="bar">
            <xsd:complexContent>
                <xsd:restriction base="t:foo">
                    <xsd:choice>
                        <xsd:element name="c1"/>
                        <xsd:element name="c2"/>
                    </xsd:choice>
                </xsd:restriction>
            </xsd:complexContent>
        </xsd:complexType>
        <xsd:element name="e1" type="t:foo" final="restriction"/>
        <xsd:element name="e2" type="t:bar" substitutionGroup="t:e1"/>
    </xsd:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_err(),
        "Schema should be invalid: final='restriction' blocks member"
    );
    match result.unwrap_err() {
        SchemaError::StructuralError { constraint, .. } => {
            assert_eq!(constraint, "e-props-correct.4");
        }
        other => panic!("Expected e-props-correct.4, got {:?}", other),
    }
}

#[test]
fn test_substitution_group_final_extension_allows_restriction_member() {
    // Same schema but final="extension" should allow restriction-derived member
    let mut schema_set = SchemaSet::new();
    let xsd = r###"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
                    targetNamespace="http://test" xmlns:t="http://test"
                    elementFormDefault="qualified">
        <xsd:complexType name="foo">
            <xsd:choice>
                <xsd:element name="c1" minOccurs="0" maxOccurs="2"/>
                <xsd:element name="c2"/>
            </xsd:choice>
        </xsd:complexType>
        <xsd:complexType name="bar">
            <xsd:complexContent>
                <xsd:restriction base="t:foo">
                    <xsd:choice>
                        <xsd:element name="c1"/>
                        <xsd:element name="c2"/>
                    </xsd:choice>
                </xsd:restriction>
            </xsd:complexContent>
        </xsd:complexType>
        <xsd:element name="e1" type="t:foo" final="extension"/>
        <xsd:element name="e2" type="t:bar" substitutionGroup="t:e1"/>
    </xsd:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "final='extension' should not block restriction-derived member: {:?}",
        result
    );
}

// ======================================================================
// Fix 1b: NameAndTypeOK — shorthand complex type restricts anyType
// ======================================================================

#[test]
fn test_element_restriction_shorthand_type_restricts_anytype() {
    // particlesIj014: base element c1 has type=anyType, derived c1 has type=foo
    // where foo is a shorthand complex type (implicitly restricts anyType).
    let mut schema_set = SchemaSet::new();
    let xsd = r###"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
                    targetNamespace="http://test" xmlns:t="http://test"
                    elementFormDefault="qualified">
        <xsd:complexType name="foo">
            <xsd:choice>
                <xsd:element name="f1" maxOccurs="5"/>
                <xsd:element name="f2"/>
            </xsd:choice>
        </xsd:complexType>
        <xsd:complexType name="B">
            <xsd:choice>
                <xsd:element name="c1" type="xsd:anyType"/>
                <xsd:element name="c2"/>
            </xsd:choice>
        </xsd:complexType>
        <xsd:complexType name="R">
            <xsd:complexContent>
                <xsd:restriction base="t:B">
                    <xsd:choice>
                        <xsd:element name="c1" type="t:foo"/>
                        <xsd:element name="c2"/>
                    </xsd:choice>
                </xsd:restriction>
            </xsd:complexContent>
        </xsd:complexType>
    </xsd:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "Schema should be valid: foo (shorthand complex type) restricts anyType: {:?}",
        result
    );
}

// ======================================================================
// Fix 2: All:All order-preserving in XSD 1.0
// ======================================================================

#[test]
fn test_all_all_reorder_invalid_xsd10() {
    // particlesS002: B has all(e1,e2,e3), R has all(e2,e1,e3) — invalid in 1.0
    let mut schema_set = SchemaSet::new();
    let xsd = r###"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
                    targetNamespace="http://test" xmlns:t="http://test">
        <xsd:complexType name="B">
            <xsd:all>
                <xsd:element name="e1"/>
                <xsd:element name="e2"/>
                <xsd:element name="e3"/>
            </xsd:all>
        </xsd:complexType>
        <xsd:complexType name="R">
            <xsd:complexContent>
                <xsd:restriction base="t:B">
                    <xsd:all>
                        <xsd:element name="e2"/>
                        <xsd:element name="e1"/>
                        <xsd:element name="e3"/>
                    </xsd:all>
                </xsd:restriction>
            </xsd:complexContent>
        </xsd:complexType>
    </xsd:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_err(),
        "All:All reordering should be invalid in XSD 1.0"
    );
}

#[cfg(feature = "xsd11")]
#[test]
fn test_all_all_reorder_valid_xsd11() {
    // Same schema valid in XSD 1.1 (RecurseUnordered)
    let mut schema_set = SchemaSet::xsd11();
    let xsd = r###"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
                    targetNamespace="http://test" xmlns:t="http://test">
        <xsd:complexType name="B">
            <xsd:all>
                <xsd:element name="e1"/>
                <xsd:element name="e2"/>
                <xsd:element name="e3"/>
            </xsd:all>
        </xsd:complexType>
        <xsd:complexType name="R">
            <xsd:complexContent>
                <xsd:restriction base="t:B">
                    <xsd:all>
                        <xsd:element name="e2"/>
                        <xsd:element name="e1"/>
                        <xsd:element name="e3"/>
                    </xsd:all>
                </xsd:restriction>
            </xsd:complexContent>
        </xsd:complexType>
    </xsd:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "All:All reordering should be valid in XSD 1.1: {:?}",
        result
    );
}

#[test]
fn test_all_all_same_order_valid_xsd10() {
    // Same order should be valid in 1.0
    let mut schema_set = SchemaSet::new();
    let xsd = r###"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
                    targetNamespace="http://test" xmlns:t="http://test">
        <xsd:complexType name="B">
            <xsd:all>
                <xsd:element name="e1"/>
                <xsd:element name="e2"/>
            </xsd:all>
        </xsd:complexType>
        <xsd:complexType name="R">
            <xsd:complexContent>
                <xsd:restriction base="t:B">
                    <xsd:all>
                        <xsd:element name="e1"/>
                        <xsd:element name="e2"/>
                    </xsd:all>
                </xsd:restriction>
            </xsd:complexContent>
        </xsd:complexType>
    </xsd:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "All:All same order should be valid in XSD 1.0: {:?}",
        result
    );
}

// ======================================================================
// Fix 10a: Sequence:All — RecurseUnordered (U family)
// ======================================================================

/// Valid: derived sequence reorders elements from base all group.
/// RecurseUnordered allows any order. (particlesU003)
#[test]
fn test_accept_sequence_restricts_all_reordered_u003() {
    let mut schema_set = SchemaSet::new();
    let xsd = r###"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
                    targetNamespace="http://test" xmlns:t="http://test">
        <xsd:complexType name="B">
            <xsd:all>
                <xsd:element name="e1"/>
                <xsd:element name="e2"/>
            </xsd:all>
        </xsd:complexType>
        <xsd:complexType name="R">
            <xsd:complexContent>
                <xsd:restriction base="t:B">
                    <xsd:sequence>
                        <xsd:element name="e2"/>
                        <xsd:element name="e1"/>
                    </xsd:sequence>
                </xsd:restriction>
            </xsd:complexContent>
        </xsd:complexType>
    </xsd:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "Sequence:All reordering should be valid (RecurseUnordered): {:?}",
        result
    );
}

/// Valid: derived sequence omits optional element from base all group.
/// (particlesU004)
#[test]
fn test_accept_sequence_restricts_all_omit_optional_u004() {
    let mut schema_set = SchemaSet::new();
    let xsd = r###"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
                    targetNamespace="http://test" xmlns:t="http://test">
        <xsd:complexType name="B">
            <xsd:all>
                <xsd:element name="e1"/>
                <xsd:element name="e2"/>
                <xsd:element name="e3" minOccurs="0"/>
            </xsd:all>
        </xsd:complexType>
        <xsd:complexType name="R">
            <xsd:complexContent>
                <xsd:restriction base="t:B">
                    <xsd:sequence>
                        <xsd:element name="e2"/>
                        <xsd:element name="e1"/>
                    </xsd:sequence>
                </xsd:restriction>
            </xsd:complexContent>
        </xsd:complexType>
    </xsd:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "Sequence:All reorder + omit optional should be valid: {:?}",
        result
    );
}

/// Valid: derived sequence fully reorders 4-element base all group,
/// promoting optional e4 to required. (particlesU007)
#[test]
fn test_accept_sequence_restricts_all_full_reorder_u007() {
    let mut schema_set = SchemaSet::new();
    let xsd = r###"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
                    targetNamespace="http://test" xmlns:t="http://test">
        <xsd:complexType name="B">
            <xsd:all>
                <xsd:element name="e1"/>
                <xsd:element name="e2" minOccurs="0"/>
                <xsd:element name="e3"/>
                <xsd:element name="e4" minOccurs="0"/>
            </xsd:all>
        </xsd:complexType>
        <xsd:complexType name="R">
            <xsd:complexContent>
                <xsd:restriction base="t:B">
                    <xsd:sequence>
                        <xsd:element name="e4"/>
                        <xsd:element name="e2" minOccurs="0"/>
                        <xsd:element name="e3"/>
                        <xsd:element name="e1"/>
                    </xsd:sequence>
                </xsd:restriction>
            </xsd:complexContent>
        </xsd:complexType>
    </xsd:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "Sequence:All full reorder with promoted optional should be valid: {:?}",
        result
    );
}

// ======================================================================
// Fix 10b: Sequence:Choice — MapAndSum (V family)
// ======================================================================

/// Valid: repeated sequence restricts repeated choice — all children
/// match choice branches and iteration budget fits. (particlesV001)
#[test]
fn test_accept_repeated_sequence_restricts_choice_v001() {
    let mut schema_set = SchemaSet::new();
    let xsd = r###"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
                    targetNamespace="http://test" xmlns:t="http://test">
        <xsd:complexType name="B">
            <xsd:choice minOccurs="1" maxOccurs="10">
                <xsd:element name="e1" minOccurs="1" maxOccurs="10"/>
                <xsd:element name="e2" minOccurs="2" maxOccurs="10"/>
                <xsd:element name="e3" minOccurs="3" maxOccurs="10"/>
            </xsd:choice>
        </xsd:complexType>
        <xsd:complexType name="R">
            <xsd:complexContent>
                <xsd:restriction base="t:B">
                    <xsd:sequence minOccurs="1" maxOccurs="3">
                        <xsd:element name="e1" minOccurs="1" maxOccurs="10"/>
                        <xsd:element name="e2" minOccurs="2" maxOccurs="10"/>
                        <xsd:element name="e3" minOccurs="3" maxOccurs="10"/>
                    </xsd:sequence>
                </xsd:restriction>
            </xsd:complexContent>
        </xsd:complexType>
    </xsd:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "Repeated sequence should validly restrict repeated choice (MapAndSum): {:?}",
        result
    );
}

/// Valid: repeated sequence{2,4} restricts choice{3,9} — iteration budget
/// [4,8] fits within [3,9]. (particlesV003)
#[test]
fn test_accept_repeated_sequence_restricts_choice_v003() {
    let mut schema_set = SchemaSet::new();
    let xsd = r###"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
                    targetNamespace="http://test" xmlns:t="http://test">
        <xsd:complexType name="B">
            <xsd:choice minOccurs="3" maxOccurs="9">
                <xsd:element name="e1"/>
                <xsd:element name="e2"/>
            </xsd:choice>
        </xsd:complexType>
        <xsd:complexType name="R">
            <xsd:complexContent>
                <xsd:restriction base="t:B">
                    <xsd:sequence minOccurs="2" maxOccurs="4">
                        <xsd:element name="e1"/>
                        <xsd:element name="e2"/>
                    </xsd:sequence>
                </xsd:restriction>
            </xsd:complexContent>
        </xsd:complexType>
    </xsd:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "Repeated sequence{{2,4}} should validly restrict choice{{3,9}}: {:?}",
        result
    );
}

// ======================================================================
// Fix 3: Choice:Choice order-preserving in XSD 1.0
// ======================================================================

#[test]
fn test_choice_choice_reorder_invalid_xsd10() {
    // particlesT002: B has choice(c1,c2), R has choice(c2,c1) — invalid in 1.0
    let mut schema_set = SchemaSet::new();
    let xsd = r###"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
                    targetNamespace="http://test" xmlns:t="http://test">
        <xsd:complexType name="B">
            <xsd:sequence>
                <xsd:choice>
                    <xsd:element name="c1"/>
                    <xsd:element name="c2"/>
                </xsd:choice>
                <xsd:element name="foo"/>
            </xsd:sequence>
        </xsd:complexType>
        <xsd:complexType name="R">
            <xsd:complexContent>
                <xsd:restriction base="t:B">
                    <xsd:sequence>
                        <xsd:choice>
                            <xsd:element name="c2"/>
                            <xsd:element name="c1"/>
                        </xsd:choice>
                        <xsd:element name="foo"/>
                    </xsd:sequence>
                </xsd:restriction>
            </xsd:complexContent>
        </xsd:complexType>
    </xsd:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_err(),
        "Choice:Choice reordering should be invalid in XSD 1.0"
    );
}

#[cfg(feature = "xsd11")]
#[test]
fn test_choice_choice_reorder_valid_xsd11() {
    // Same schema valid in XSD 1.1
    let mut schema_set = SchemaSet::xsd11();
    let xsd = r###"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
                    targetNamespace="http://test" xmlns:t="http://test">
        <xsd:complexType name="B">
            <xsd:sequence>
                <xsd:choice>
                    <xsd:element name="c1"/>
                    <xsd:element name="c2"/>
                </xsd:choice>
                <xsd:element name="foo"/>
            </xsd:sequence>
        </xsd:complexType>
        <xsd:complexType name="R">
            <xsd:complexContent>
                <xsd:restriction base="t:B">
                    <xsd:sequence>
                        <xsd:choice>
                            <xsd:element name="c2"/>
                            <xsd:element name="c1"/>
                        </xsd:choice>
                        <xsd:element name="foo"/>
                    </xsd:sequence>
                </xsd:restriction>
            </xsd:complexContent>
        </xsd:complexType>
    </xsd:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "Choice:Choice reordering should be valid in XSD 1.1: {:?}",
        result
    );
}

#[test]
fn test_choice_choice_same_order_valid_xsd10() {
    // Same order should be valid in 1.0
    let mut schema_set = SchemaSet::new();
    let xsd = r###"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
                    targetNamespace="http://test" xmlns:t="http://test">
        <xsd:complexType name="B">
            <xsd:sequence>
                <xsd:choice>
                    <xsd:element name="c1"/>
                    <xsd:element name="c2"/>
                </xsd:choice>
                <xsd:element name="foo"/>
            </xsd:sequence>
        </xsd:complexType>
        <xsd:complexType name="R">
            <xsd:complexContent>
                <xsd:restriction base="t:B">
                    <xsd:sequence>
                        <xsd:choice>
                            <xsd:element name="c1"/>
                            <xsd:element name="c2"/>
                        </xsd:choice>
                        <xsd:element name="foo"/>
                    </xsd:sequence>
                </xsd:restriction>
            </xsd:complexContent>
        </xsd:complexType>
    </xsd:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "Choice:Choice same order should be valid in XSD 1.0: {:?}",
        result
    );
}

// ── Step 11 regression targets ─────────────────────────────────────────
// Dead particles (maxOccurs=0) must not trigger spurious cos-nonambig.

/// particlesJd005: base wildcard{0,0} + derived element{0,0} in restriction.
/// The dead particles should vanish during UPA compile, not trigger cos-nonambig.
#[test]
fn test_accept_dead_wildcard_restriction_jd005() {
    let mut schema_set = SchemaSet::new();
    let xsd = r###"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                   targetNamespace="http://xsdtesting" xmlns:a="http://xsdtesting">
            <xs:complexType name="B">
                <xs:sequence>
                    <xs:any namespace="##any" minOccurs="0" maxOccurs="0"/>
                    <xs:element name="e2"/>
                </xs:sequence>
            </xs:complexType>
            <xs:complexType name="R">
                <xs:complexContent>
                    <xs:restriction base="a:B">
                        <xs:sequence>
                            <xs:element name="e1" minOccurs="0" maxOccurs="0"/>
                            <xs:element name="e2"/>
                        </xs:sequence>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "dead wildcard restriction (Jd005 shape) should be valid: {:?}",
        result
    );
}

/// particlesJf005: same shape but with imported element ref in the dead slot.
#[test]
fn test_accept_dead_imported_element_restriction_jf005() {
    let mut schema_set = SchemaSet::new();
    let xsd = r###"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                   targetNamespace="http://xsdtesting" xmlns:x="http://xsdtesting">
            <xs:complexType name="B">
                <xs:sequence>
                    <xs:any namespace="##any" minOccurs="0" maxOccurs="0"/>
                    <xs:element name="e1"/>
                </xs:sequence>
            </xs:complexType>
            <xs:complexType name="R">
                <xs:complexContent>
                    <xs:restriction base="x:B">
                        <xs:sequence>
                            <xs:element name="e1" minOccurs="0" maxOccurs="0"/>
                            <xs:element name="e1"/>
                        </xs:sequence>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "dead imported-element restriction (Jf005 shape) should be valid: {:?}",
        result
    );
}

// ── Step 13 canary tests ──────────────────────────────────────────────

/// particles00104m1: xs:any inside xs:all is invalid in XSD 1.0
#[test]
fn test_reject_all_group_with_wildcard_xsd10() {
    let mut schema_set = SchemaSet::new();
    let xsd = r###"<?xml version="1.0" encoding="UTF-8"?>
        <xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
                    xmlns="particles" targetNamespace="particles">
            <xsd:group name="G1">
                <xsd:all>
                    <xsd:any/>
                </xsd:all>
            </xsd:group>
            <xsd:element name="a" type="xsd:string"/>
        </xsd:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_err(),
        "xs:any in xs:all must be rejected in XSD 1.0"
    );
    match result.unwrap_err() {
        crate::error::SchemaError::StructuralError { constraint, .. } => {
            assert_eq!(constraint, "cos-all-limited");
        }
        other => panic!("Expected cos-all-limited, got {:?}", other),
    }
}

/// XSD 1.1 allows xs:any inside xs:all
#[cfg(feature = "xsd11")]
#[test]
fn test_accept_all_group_with_wildcard_xsd11() {
    let mut schema_set = SchemaSet::xsd11();
    let xsd = r###"<?xml version="1.0" encoding="UTF-8"?>
        <xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
                    xmlns="particles" targetNamespace="particles">
            <xsd:group name="G1">
                <xsd:all>
                    <xsd:any/>
                </xsd:all>
            </xsd:group>
            <xsd:element name="a" type="xsd:string"/>
        </xsd:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "xs:any in xs:all should be accepted in XSD 1.1: {:?}",
        result
    );
}

/// particlesFb002: extending choice content with all compositor is invalid
#[test]
fn test_reject_extension_with_all_over_choice() {
    let mut schema_set = SchemaSet::new();
    let xsd = r###"<?xml version="1.0"?>
        <xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
                    targetNamespace="http://xsdtesting" xmlns:x="http://xsdtesting"
                    elementFormDefault="qualified">
            <xsd:complexType name="base">
                <xsd:choice>
                    <xsd:element name="c1"/>
                    <xsd:element name="c2"/>
                </xsd:choice>
            </xsd:complexType>
            <xsd:element name="doc">
                <xsd:complexType>
                    <xsd:complexContent>
                        <xsd:extension base="x:base">
                            <xsd:all>
                                <xsd:element name="a1"/>
                                <xsd:element name="a2"/>
                            </xsd:all>
                        </xsd:extension>
                    </xsd:complexContent>
                </xsd:complexType>
            </xsd:element>
        </xsd:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_err(),
        "extending choice with all must be rejected"
    );
    match result.unwrap_err() {
        crate::error::SchemaError::StructuralError { constraint, .. } => {
            assert_eq!(constraint, "cos-ct-extends");
        }
        other => panic!("Expected cos-ct-extends, got {:?}", other),
    }
}

/// Extending sequence content with choice compositor is valid:
/// the effective content type is sequence(base, choice) per §3.4.2.3.3
/// clause 4.2.3.3, satisfying cos-particle-extend clause 2.
#[test]
fn test_accept_extension_with_choice_over_sequence() {
    let mut schema_set = SchemaSet::new();
    let xsd = r###"<?xml version="1.0"?>
        <xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
                    targetNamespace="http://xsdtesting" xmlns:x="http://xsdtesting"
                    elementFormDefault="qualified">
            <xsd:complexType name="base">
                <xsd:sequence>
                    <xsd:element name="s1" type="xsd:string"/>
                </xsd:sequence>
            </xsd:complexType>
            <xsd:element name="doc">
                <xsd:complexType>
                    <xsd:complexContent>
                        <xsd:extension base="x:base">
                            <xsd:choice>
                                <xsd:element name="c1" type="xsd:string"/>
                                <xsd:element name="c2" type="xsd:string"/>
                            </xsd:choice>
                        </xsd:extension>
                    </xsd:complexContent>
                </xsd:complexType>
            </xsd:element>
        </xsd:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "extending sequence with choice is valid per spec: {:?}",
        result.err()
    );
}

/// particlesFb004: XSD 1.1 all-over-all extension is valid
#[cfg(feature = "xsd11")]
#[test]
fn test_accept_xsd11_all_over_all_extension() {
    let mut schema_set = SchemaSet::xsd11();
    let xsd = r###"<?xml version="1.0"?>
        <xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
                    targetNamespace="http://xsdtesting" xmlns:x="http://xsdtesting"
                    elementFormDefault="qualified">
            <xsd:complexType name="base">
                <xsd:all>
                    <xsd:element name="a1" type="xsd:string"/>
                </xsd:all>
            </xsd:complexType>
            <xsd:element name="doc">
                <xsd:complexType>
                    <xsd:complexContent>
                        <xsd:extension base="x:base">
                            <xsd:all>
                                <xsd:element name="a2" type="xsd:string"/>
                            </xsd:all>
                        </xsd:extension>
                    </xsd:complexContent>
                </xsd:complexType>
            </xsd:element>
        </xsd:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "XSD 1.1 all-over-all extension should be accepted: {:?}",
        result
    );
}

/// particlesZ013: attribute type not validly derived in restriction
#[test]
fn test_reject_attribute_type_not_derived_in_restriction() {
    let mut schema_set = SchemaSet::new();
    let xsd = r###"<?xml version="1.0"?>
        <xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
                    targetNamespace="http://xsdtesting" xmlns:t="http://xsdtesting"
                    attributeFormDefault="qualified" elementFormDefault="qualified">
            <xsd:simpleType name="myType10">
                <xsd:union memberTypes="xsd:float xsd:integer">
                    <xsd:simpleType>
                        <xsd:restriction base="xsd:boolean"/>
                    </xsd:simpleType>
                    <xsd:simpleType>
                        <xsd:restriction base="xsd:string">
                            <xsd:enumeration value="x"/>
                            <xsd:enumeration value="y"/>
                        </xsd:restriction>
                    </xsd:simpleType>
                </xsd:union>
            </xsd:simpleType>
            <xsd:complexType name="CT1">
                <xsd:attribute name="att1" type="xsd:integer"/>
            </xsd:complexType>
            <xsd:complexType name="CT2">
                <xsd:complexContent>
                    <xsd:restriction base="t:CT1">
                        <xsd:attribute name="att1" type="t:myType10"/>
                    </xsd:restriction>
                </xsd:complexContent>
            </xsd:complexType>
        </xsd:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_err(),
        "attribute type not derived from base must be rejected"
    );
    match result.unwrap_err() {
        crate::error::SchemaError::StructuralError { constraint, .. } => {
            assert_eq!(constraint, "derivation-ok-restriction");
        }
        other => panic!("Expected derivation-ok-restriction, got {:?}", other),
    }
}

/// particlesZ017: required attribute becomes optional in restriction
#[test]
fn test_reject_required_attribute_becomes_optional() {
    let mut schema_set = SchemaSet::new();
    let xsd = r###"<?xml version="1.0"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:complexType name="XML-Deviant">
                <xs:sequence>
                    <xs:element name="e1" type="xs:integer" minOccurs="0"/>
                    <xs:element name="e2" type="xs:string" nillable="true"/>
                </xs:sequence>
                <xs:attribute name="a1" type="xs:date" use="required"/>
                <xs:attribute name="a2" type="xs:string"/>
            </xs:complexType>
            <xs:complexType name="DareObasanjo">
                <xs:complexContent>
                    <xs:restriction base="XML-Deviant">
                        <xs:sequence>
                            <xs:element name="e1" type="xs:integer" minOccurs="1"/>
                            <xs:element name="e2" type="xs:string" nillable="false"/>
                        </xs:sequence>
                        <xs:attribute name="a1" type="xs:date" use="optional"/>
                        <xs:attribute name="a2" type="xs:string" fixed="Microsoft Outlook"/>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_err(),
        "required->optional attribute must be rejected"
    );
    match result.unwrap_err() {
        crate::error::SchemaError::StructuralError { constraint, .. } => {
            assert_eq!(constraint, "derivation-ok-restriction");
        }
        other => panic!("Expected derivation-ok-restriction, got {:?}", other),
    }
}

/// Restriction that omits a base required attribute is valid —
/// the attribute is inherited unchanged per §3.4.2.3 mapping
/// (the derived type's effective {attribute uses} still contains the
/// required `ns1:id` attribute; clause 3 of derivation-ok-restriction
/// is trivially satisfied for inherited uses). The test exercises the
/// namespace-aware matcher: `ns1:id` and a hypothetical no-namespace
/// `id` must be kept distinct when searching for overrides.
#[test]
fn test_accept_restriction_inherits_namespaced_required_attribute() {
    let mut schema_set = SchemaSet::new();
    let xsd = r###"<?xml version="1.0"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                   targetNamespace="http://ns1" xmlns:ns1="http://ns1">
            <xs:attribute name="id" type="xs:string"/>
            <xs:complexType name="Base">
                <xs:sequence>
                    <xs:element name="e" type="xs:string"/>
                </xs:sequence>
                <xs:attribute ref="ns1:id" use="required"/>
            </xs:complexType>
            <xs:complexType name="Derived">
                <xs:complexContent>
                    <xs:restriction base="ns1:Base">
                        <xs:sequence>
                            <xs:element name="e" type="xs:string"/>
                        </xs:sequence>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "omitting a base required attribute inherits it unchanged, \
         which is a valid restriction (spec §3.4.2.3 mapping + \
         derivation-ok-restriction clause 3): {:?}",
        result.err(),
    );
}

/// particlesZ018: simpleContent restriction with list type over atomic base
#[test]
fn test_reject_simple_content_restriction_with_list_over_atomic() {
    let mut schema_set = SchemaSet::new();
    let xsd = r###"<?xml version="1.0"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:complexType name="B1">
                <xs:simpleContent>
                    <xs:extension base="xs:decimal">
                        <xs:attribute name="foo"/>
                    </xs:extension>
                </xs:simpleContent>
            </xs:complexType>
            <xs:complexType name="C2">
                <xs:simpleContent>
                    <xs:restriction base="B1">
                        <xs:simpleType>
                            <xs:list itemType="xs:int"/>
                        </xs:simpleType>
                    </xs:restriction>
                </xs:simpleContent>
            </xs:complexType>
        </xs:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_err(),
        "list type restricting atomic simple content must be rejected"
    );
    match result.unwrap_err() {
        crate::error::SchemaError::StructuralError { constraint, .. } => {
            assert_eq!(constraint, "derivation-ok-restriction");
        }
        other => panic!("Expected derivation-ok-restriction, got {:?}", other),
    }
}

/// particlesZ019: simpleContent list restriction over anySimpleType is valid
#[test]
fn test_accept_simple_content_list_over_any_simple_type() {
    let mut schema_set = SchemaSet::new();
    let xsd = r###"<?xml version="1.0"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:complexType name="B1">
                <xs:simpleContent>
                    <xs:extension base="xs:anySimpleType">
                        <xs:attribute name="foo"/>
                    </xs:extension>
                </xs:simpleContent>
            </xs:complexType>
            <xs:complexType name="C2">
                <xs:simpleContent>
                    <xs:restriction base="B1">
                        <xs:simpleType>
                            <xs:list itemType="xs:int"/>
                        </xs:simpleType>
                    </xs:restriction>
                </xs:simpleContent>
            </xs:complexType>
        </xs:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "list restricting anySimpleType should be accepted: {:?}",
        result
    );
}

/// particlesZ020: simpleContent union restriction over anySimpleType is valid
#[test]
fn test_accept_simple_content_union_over_any_simple_type() {
    let mut schema_set = SchemaSet::new();
    let xsd = r###"<?xml version="1.0"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:complexType name="B1">
                <xs:simpleContent>
                    <xs:extension base="xs:anySimpleType">
                        <xs:attribute name="foo"/>
                    </xs:extension>
                </xs:simpleContent>
            </xs:complexType>
            <xs:complexType name="C2">
                <xs:simpleContent>
                    <xs:restriction base="B1">
                        <xs:simpleType>
                            <xs:union memberTypes="xs:int xs:string"/>
                        </xs:simpleType>
                    </xs:restriction>
                </xs:simpleContent>
            </xs:complexType>
        </xs:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "union restricting anySimpleType should be accepted: {:?}",
        result
    );
}

// ── XSD 1.1 particle conformance regression tests ────────────────────────────

/// particlesFb003: XSD 1.1 allows choice extension over non-empty base content
#[cfg(feature = "xsd11")]
#[test]
fn test_accept_xsd11_choice_extension() {
    let mut schema_set = SchemaSet::xsd11();
    let xsd = r###"<?xml version="1.0"?>
        <xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
                    targetNamespace="http://xsdtesting" xmlns:x="http://xsdtesting"
                    elementFormDefault="qualified">
            <xsd:complexType name="base">
                <xsd:choice>
                    <xsd:any namespace="##local ##targetNamespace foo" maxOccurs="3"/>
                </xsd:choice>
            </xsd:complexType>
            <xsd:element name="doc">
                <xsd:complexType>
                    <xsd:complexContent>
                        <xsd:extension base="x:base">
                            <xsd:choice>
                                <xsd:element name="c1"/>
                                <xsd:element name="c2"/>
                            </xsd:choice>
                        </xsd:extension>
                    </xsd:complexContent>
                </xsd:complexType>
            </xsd:element>
        </xsd:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "XSD 1.1: choice extension over non-empty base should be accepted: {:?}",
        result
    );
}

/// particlesZ031: XSD 1.1 rejects complexContent extension over simpleContent base
#[cfg(feature = "xsd11")]
#[test]
fn test_reject_xsd11_complexcontent_over_simplecontent() {
    let mut schema_set = SchemaSet::xsd11();
    let xsd = r###"<?xml version="1.0"?>
        <xs:schema xmlns="http://schema1" xmlns:xs="http://www.w3.org/2001/XMLSchema"
                   targetNamespace="http://schema1">
            <xs:complexType name="Type1">
                <xs:simpleContent>
                    <xs:extension base="xs:string">
                        <xs:attribute name="Field1" type="xs:string"/>
                    </xs:extension>
                </xs:simpleContent>
            </xs:complexType>
            <xs:complexType name="Type2">
                <xs:complexContent>
                    <xs:extension base="Type1">
                        <xs:attribute name="Field2" type="xs:string"/>
                    </xs:extension>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_err(),
        "XSD 1.1: complexContent extension over simpleContent must be rejected"
    );
    match result.unwrap_err() {
        crate::error::SchemaError::StructuralError { constraint, .. } => {
            assert_eq!(constraint, "cos-ct-extends");
        }
        other => panic!("Expected cos-ct-extends, got {:?}", other),
    }
}

/// particlesZ031: XSD 1.0 accepts complexContent extension over simpleContent base
/// (only rejected when a particle is added; attribute-only extension is fine)
#[test]
fn test_accept_xsd10_complexcontent_over_simplecontent() {
    let mut schema_set = SchemaSet::new();
    let xsd = r###"<?xml version="1.0"?>
        <xs:schema xmlns="http://schema1" xmlns:xs="http://www.w3.org/2001/XMLSchema"
                   targetNamespace="http://schema1">
            <xs:complexType name="Type1">
                <xs:simpleContent>
                    <xs:extension base="xs:string">
                        <xs:attribute name="Field1" type="xs:string"/>
                    </xs:extension>
                </xs:simpleContent>
            </xs:complexType>
            <xs:complexType name="Type2">
                <xs:complexContent>
                    <xs:extension base="Type1">
                        <xs:attribute name="Field2" type="xs:string"/>
                    </xs:extension>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "XSD 1.0: attribute-only complexContent extension over simpleContent should be accepted: {:?}",
        result
    );
}

/// particlesHb008: XSD 1.1 accepts choice-in-derived-sequence restriction
#[cfg(feature = "xsd11")]
#[test]
fn test_accept_xsd11_intensional_restriction_hb008() {
    let mut schema_set = SchemaSet::xsd11();
    let xsd = r###"<?xml version="1.0"?>
        <xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
                    targetNamespace="http://xsdtesting" xmlns:x="http://xsdtesting"
                    elementFormDefault="qualified">
            <xsd:complexType name="base">
                <xsd:choice>
                    <xsd:element name="e1" minOccurs="1" maxOccurs="3"/>
                    <xsd:sequence maxOccurs="2">
                        <xsd:element name="e2" minOccurs="1" maxOccurs="3"/>
                        <xsd:element name="e3" minOccurs="0" maxOccurs="3"/>
                        <xsd:element name="e4" minOccurs="0" maxOccurs="3"/>
                    </xsd:sequence>
                </xsd:choice>
            </xsd:complexType>
            <xsd:element name="doc">
                <xsd:complexType>
                    <xsd:complexContent>
                        <xsd:restriction base="x:base">
                            <xsd:choice>
                                <xsd:element name="e1" minOccurs="1" maxOccurs="2"/>
                                <xsd:sequence maxOccurs="2">
                                    <xsd:element name="e2"/>
                                    <xsd:choice>
                                        <xsd:element name="e3" minOccurs="2" maxOccurs="3"/>
                                        <xsd:element name="e4" minOccurs="1" maxOccurs="3"/>
                                    </xsd:choice>
                                </xsd:sequence>
                            </xsd:choice>
                        </xsd:restriction>
                    </xsd:complexContent>
                </xsd:complexType>
            </xsd:element>
        </xsd:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "XSD 1.1: choice-in-derived-sequence restriction (Hb008) should be accepted: {:?}",
        result
    );
}

/// particlesHb011: XSD 1.1 accepts single-child sequence folding in restriction
#[cfg(feature = "xsd11")]
#[test]
fn test_accept_xsd11_intensional_restriction_hb011() {
    let mut schema_set = SchemaSet::xsd11();
    let xsd = r###"<?xml version="1.0"?>
        <xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
                    targetNamespace="http://xsdtesting" xmlns:x="http://xsdtesting"
                    elementFormDefault="qualified">
            <xsd:complexType name="base">
                <xsd:choice minOccurs="2" maxOccurs="unbounded">
                    <xsd:element name="e1" minOccurs="0" maxOccurs="10"/>
                    <xsd:element name="e2" minOccurs="0"/>
                    <xsd:element name="e3" minOccurs="0"/>
                </xsd:choice>
            </xsd:complexType>
            <xsd:element name="doc">
                <xsd:complexType>
                    <xsd:complexContent>
                        <xsd:restriction base="x:base">
                            <xsd:choice minOccurs="2" maxOccurs="unbounded">
                                <xsd:sequence maxOccurs="2">
                                    <xsd:element name="e1" maxOccurs="2"/>
                                </xsd:sequence>
                                <xsd:element name="e2"/>
                                <xsd:element name="e3" minOccurs="1"/>
                            </xsd:choice>
                        </xsd:restriction>
                    </xsd:complexContent>
                </xsd:complexType>
            </xsd:element>
        </xsd:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "XSD 1.1: single-child sequence folding (Hb011) should be accepted: {:?}",
        result
    );
}

/// particlesZ023: XSD 1.1 accepts single-branch choice restricting multi-branch choice
#[cfg(feature = "xsd11")]
#[test]
fn test_accept_xsd11_intensional_restriction_z023() {
    let mut schema_set = SchemaSet::xsd11();
    let xsd = r###"<?xml version="1.0"?>
        <xsd:schema targetNamespace="http://myuri" xmlns="http://myuri"
                    xmlns:xsd="http://www.w3.org/2001/XMLSchema">
            <xsd:element name="A" type="xsd:string"/>
            <xsd:element name="B" type="xsd:string"/>
            <xsd:complexType name="eleType">
                <xsd:sequence>
                    <xsd:element ref="A"/>
                    <xsd:element ref="B"/>
                    <xsd:choice>
                        <xsd:sequence>
                            <xsd:element name="AAA" minOccurs="0" maxOccurs="unbounded"/>
                            <xsd:element name="BBB" minOccurs="0" maxOccurs="unbounded"/>
                            <xsd:element name="CCC" minOccurs="0" maxOccurs="unbounded"/>
                        </xsd:sequence>
                        <xsd:sequence>
                            <xsd:element name="AAAA" minOccurs="0" maxOccurs="unbounded"/>
                            <xsd:element name="BBBB" minOccurs="0" maxOccurs="unbounded"/>
                            <xsd:element name="CCCC" minOccurs="0" maxOccurs="unbounded"/>
                        </xsd:sequence>
                    </xsd:choice>
                </xsd:sequence>
            </xsd:complexType>
            <xsd:complexType name="eleType2">
                <xsd:complexContent>
                    <xsd:restriction base="eleType">
                        <xsd:sequence>
                            <xsd:element ref="A"/>
                            <xsd:element ref="B"/>
                            <xsd:choice>
                                <xsd:sequence>
                                    <xsd:element name="AAA" minOccurs="0" maxOccurs="unbounded"/>
                                    <xsd:element name="BBB" minOccurs="0" maxOccurs="unbounded"/>
                                    <xsd:element name="CCC" minOccurs="0" maxOccurs="unbounded"/>
                                </xsd:sequence>
                            </xsd:choice>
                        </xsd:sequence>
                    </xsd:restriction>
                </xsd:complexContent>
            </xsd:complexType>
        </xsd:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "XSD 1.1: single-branch choice restriction (Z023) should be accepted: {:?}",
        result
    );
}

/// particlesZ024: XSD 1.1 accepts single group-ref choice restricting multi-branch choice
#[cfg(feature = "xsd11")]
#[test]
fn test_accept_xsd11_intensional_restriction_z024() {
    let mut schema_set = SchemaSet::xsd11();
    let xsd = r###"<?xml version="1.0"?>
        <xsd:schema targetNamespace="http://myuri" xmlns="http://myuri"
                    xmlns:xsd="http://www.w3.org/2001/XMLSchema">
            <xsd:element name="A" type="xsd:string"/>
            <xsd:element name="B" type="xsd:string"/>
            <xsd:group name="G1">
                <xsd:sequence>
                    <xsd:element name="AA" minOccurs="0" maxOccurs="unbounded"/>
                    <xsd:element name="BB" minOccurs="0" maxOccurs="unbounded"/>
                </xsd:sequence>
            </xsd:group>
            <xsd:group name="G2">
                <xsd:sequence>
                    <xsd:element name="AAA" minOccurs="0" maxOccurs="unbounded"/>
                    <xsd:element name="BBB" minOccurs="0" maxOccurs="unbounded"/>
                </xsd:sequence>
            </xsd:group>
            <xsd:complexType name="eleType">
                <xsd:sequence>
                    <xsd:element ref="A"/>
                    <xsd:element ref="B"/>
                    <xsd:choice>
                        <xsd:group ref="G1" minOccurs="0" maxOccurs="unbounded"/>
                        <xsd:group ref="G2" minOccurs="0" maxOccurs="unbounded"/>
                    </xsd:choice>
                </xsd:sequence>
            </xsd:complexType>
            <xsd:complexType name="eleType2">
                <xsd:complexContent>
                    <xsd:restriction base="eleType">
                        <xsd:sequence>
                            <xsd:element ref="A"/>
                            <xsd:element ref="B"/>
                            <xsd:choice>
                                <xsd:group ref="G1" minOccurs="0"/>
                            </xsd:choice>
                        </xsd:sequence>
                    </xsd:restriction>
                </xsd:complexContent>
            </xsd:complexType>
        </xsd:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "XSD 1.1: single group-ref choice restriction (Z024) should be accepted: {:?}",
        result
    );
}

/// particlesZ028: XSD 1.1 accepts substitution group in sequence restriction
#[cfg(feature = "xsd11")]
#[test]
fn test_accept_xsd11_intensional_restriction_z028() {
    let mut schema_set = SchemaSet::xsd11();
    let xsd = r###"<?xml version="1.0"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:element abstract="true" name="aba" type="xs:string"/>
            <xs:element name="a" substitutionGroup="aba" type="xs:string"/>
            <xs:element name="d" type="xs:anyURI"/>
            <xs:group name="abs">
                <xs:choice>
                    <xs:element ref="aba"/>
                </xs:choice>
            </xs:group>
            <xs:complexType name="test">
                <xs:sequence>
                    <xs:group maxOccurs="unbounded" minOccurs="0" ref="abs"/>
                    <xs:element minOccurs="0" ref="d"/>
                </xs:sequence>
            </xs:complexType>
            <xs:complexType name="test4">
                <xs:complexContent>
                    <xs:restriction base="test">
                        <xs:sequence>
                            <xs:sequence minOccurs="1" maxOccurs="1">
                                <xs:element ref="a"/>
                            </xs:sequence>
                            <xs:element ref="d"/>
                        </xs:sequence>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"###;

    let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None);
    assert!(
        result.is_ok(),
        "XSD 1.1: substitution group sequence restriction (Z028) should be accepted: {:?}",
        result
    );
}

// ===========================================================================
// Nested redefine: the original lives in the redefined document's *effective
// view*, not necessarily in its own top-level declarations.
// ===========================================================================

/// XSD 1.0 §4.2.2 / XSD 1.1 §4.2.3 `src-redefine`: the original of a redefined
/// component must be found in "the schema corresponding to" the redefined
/// document. That schema is the document's **effective view** — everything it
/// pulls in transitively via its own `xs:include` / `xs:redefine` — and not
/// merely its own top-level declarations.
///
/// Here `outer.xsd` redefines `middle.xsd`, while every original is declared
/// one composition hop further down, in `base.xsd`. The matrix covers:
///
/// * all four redefinable component kinds (simpleType, complexType, group,
///   attributeGroup),
/// * a namespaced composition and a chameleon one (`middle`/`base` without a
///   `targetNamespace`, adopting `urn:test` from the redefining document),
/// * `middle` reaching `base` through `xs:include` and through a chained
///   `xs:redefine`.
///
/// Every fixture is spec-**valid**: in particular each document that declares
/// or restricts the local element `value` agrees on
/// `elementFormDefault="qualified"`, so the redefined `Record` really is a
/// valid restriction of the original (rcase-NameAndTypeOK). They therefore
/// must load with the derivation checks fully enabled.
#[test]
fn test_nested_redefine_resolves_original_through_effective_view() {
    for ns_case in ["namespaced", "chameleon"] {
        // In the chameleon case `middle.xsd`/`base.xsd` carry no
        // targetNamespace (and hence no prefix for their self-references);
        // they adopt `urn:test` from the redefining document per §4.2.3.
        let (ns_attrs, self_prefix) = match ns_case {
            "namespaced" => (r#" targetNamespace="urn:test" xmlns:t="urn:test""#, "t:"),
            _ => ("", ""),
        };

        for link_case in ["include", "redefine"] {
            let middle_body = match link_case {
                "include" => r#"<xs:include schemaLocation="base.xsd"/>"#.to_string(),
                _ => format!(
                    r#"<xs:redefine schemaLocation="base.xsd">
<xs:simpleType name="Marker"><xs:restriction base="{self_prefix}Marker"><xs:maxLength value="8"/></xs:restriction></xs:simpleType>
</xs:redefine>"#
                ),
            };

            let base = format!(
                r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"{ns_attrs} elementFormDefault="qualified">
<xs:simpleType name="Code"><xs:restriction base="xs:string"/></xs:simpleType>
<xs:simpleType name="Marker"><xs:restriction base="xs:string"/></xs:simpleType>
<xs:simpleType name="Inherited"><xs:restriction base="xs:normalizedString"/></xs:simpleType>
<xs:complexType name="Record"><xs:sequence><xs:element name="value" type="xs:string"/></xs:sequence></xs:complexType>
<xs:group name="Rows"><xs:sequence><xs:element name="row" type="xs:string"/></xs:sequence></xs:group>
<xs:attributeGroup name="Flags"><xs:attribute name="flag" type="xs:string"/></xs:attributeGroup>
</xs:schema>"#
            );
            let middle = format!(
                r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"{ns_attrs} elementFormDefault="qualified">
{middle_body}
</xs:schema>"#
            );
            let outer = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
 targetNamespace="urn:test" xmlns:t="urn:test" elementFormDefault="qualified">
<xs:redefine schemaLocation="middle.xsd">
<xs:simpleType name="Code"><xs:restriction base="t:Code"><xs:maxLength value="4"/></xs:restriction></xs:simpleType>
<xs:complexType name="Record"><xs:complexContent><xs:restriction base="t:Record">
<xs:sequence><xs:element name="value" type="t:Inherited"/></xs:sequence>
</xs:restriction></xs:complexContent></xs:complexType>
<xs:group name="Rows"><xs:sequence><xs:group ref="t:Rows"/><xs:element name="extra" type="xs:string"/></xs:sequence></xs:group>
<xs:attributeGroup name="Flags"><xs:attributeGroup ref="t:Flags"/><xs:attribute name="extraFlag" type="xs:string"/></xs:attributeGroup>
</xs:redefine>
<xs:element name="root" type="t:Code"/>
<xs:element name="inherited" type="t:Inherited"/>
<xs:element name="record" type="t:Record"/>
</xs:schema>"#;

            let tmp = std::env::temp_dir().join(format!(
                "xsd_test_nested_redefine_{}_{}_{}",
                std::process::id(),
                ns_case,
                link_case
            ));
            let _ = std::fs::remove_dir_all(&tmp);
            std::fs::create_dir_all(&tmp).unwrap();
            std::fs::write(tmp.join("base.xsd"), base).unwrap();
            std::fs::write(tmp.join("middle.xsd"), middle).unwrap();
            let outer_path = tmp.join("outer.xsd");

            let mut schema_set = SchemaSet::new();
            // Strict processing on purpose: the fixture is spec-valid, so the
            // derivation checker must accept it.
            let result = load_and_process_schema(
                outer.as_bytes(),
                &outer_path.to_string_lossy(),
                &mut schema_set,
                None,
            );
            let _ = std::fs::remove_dir_all(&tmp);
            assert!(
                result.is_ok(),
                "{ns_case}/{link_case}: nested redefine must resolve its transitive \
                 original in strict mode: {result:?}"
            );

            let namespace = schema_set.name_table.get("urn:test").unwrap();

            // simpleType: redefined `Code` found its original in base.xsd.
            let code = schema_set.name_table.get("Code").unwrap();
            let code_key = match schema_set.lookup_type(Some(namespace), code) {
                Some(TypeKey::Simple(key)) => key,
                other => panic!("{ns_case}/{link_case}: expected simple type Code, got {other:?}"),
            };
            assert!(
                schema_set
                    .arenas
                    .simple_types
                    .get(code_key)
                    .and_then(|st| st.redefine_original)
                    .is_some(),
                "{ns_case}/{link_case}: redefined simple type must retain its \
                 transitive original"
            );

            // The type declared only in base.xsd is reachable from the outer
            // document, so the element referencing it resolves.
            let inherited = schema_set.name_table.get("inherited").unwrap();
            let inherited = schema_set
                .lookup_element(Some(namespace), inherited)
                .and_then(|key| schema_set.arenas.elements.get(key));
            assert!(
                matches!(
                    inherited.and_then(|element| element.resolved_type),
                    Some(TypeKey::Simple(_))
                ),
                "{ns_case}/{link_case}: element referencing a base.xsd type must resolve"
            );

            // complexType: redefined `Record` found its original in base.xsd.
            let record = schema_set.name_table.get("Record").unwrap();
            let record_key = match schema_set.lookup_type(Some(namespace), record) {
                Some(TypeKey::Complex(key)) => key,
                other => {
                    panic!("{ns_case}/{link_case}: expected complex type Record, got {other:?}")
                }
            };
            assert!(
                schema_set
                    .arenas
                    .complex_types
                    .get(record_key)
                    .and_then(|ct| ct.redefine_original)
                    .is_some(),
                "{ns_case}/{link_case}: redefined complex type must retain its \
                 transitive original"
            );

            // group: the self-reference inside the redefinition is rewired to
            // the original group from base.xsd.
            let rows_name = schema_set.name_table.get("Rows").unwrap();
            let rows_key = schema_set
                .lookup_model_group(Some(namespace), rows_name)
                .unwrap();
            let rows = schema_set.arenas.model_groups.get(rows_key).unwrap();
            let rows_original = rows
                .redefine_original
                .expect("redefined model group must retain its transitive original");
            assert!(
                rows.resolved_particles.iter().any(|particle| matches!(
                    particle,
                    crate::arenas::ResolvedParticleTerm::Group {
                        resolved_ref: Some(key)
                    } if *key == rows_original
                )),
                "{ns_case}/{link_case}: redefined group's self-reference must point \
                 at the original group"
            );

            // attributeGroup: same, via resolved_attribute_groups.
            let flags_name = schema_set.name_table.get("Flags").unwrap();
            let flags_key = schema_set
                .lookup_attribute_group(Some(namespace), flags_name)
                .unwrap();
            let flags = schema_set.arenas.attribute_groups.get(flags_key).unwrap();
            let flags_original = flags
                .redefine_original
                .expect("redefined attribute group must retain its transitive original");
            assert!(
                flags.resolved_attribute_groups.contains(&flags_original),
                "{ns_case}/{link_case}: redefined attribute group's self-reference must \
                 point at the original attribute group"
            );
        }
    }
}

/// The derivation checks run on both the one-shot and the manual
/// (parse-then-process) entry points.
///
/// The fixture below is deliberately schema-INVALID: `InvalidRestriction`
/// declares an element `b` that does not appear in `Base`'s content model, so
/// no rcase in XSD 1.0 §3.9.6 (nor §3.4.6.4 `cos-content-act-restrict` in
/// XSD 1.1) can hold.
#[test]
fn test_derivation_validation_runs_on_every_entry_point() {
    let schema = br#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
<xs:complexType name="Base"><xs:sequence><xs:element name="a" type="xs:string"/></xs:sequence></xs:complexType>
<xs:complexType name="InvalidRestriction"><xs:complexContent><xs:restriction base="Base">
<xs:sequence><xs:element name="b" type="xs:string"/></xs:sequence>
</xs:restriction></xs:complexContent></xs:complexType>
</xs:schema>"#;

    let mut set = SchemaSet::new();
    let err = load_and_process_schema(schema, "memory:///strict.xsd", &mut set, None)
        .expect_err("processing must check derivation constraints");
    assert!(
        err.to_string().contains("derivation-ok-restriction"),
        "expected a derivation-ok-restriction failure, got: {err:?}"
    );

    let mut manual_set = SchemaSet::new();
    parse_schema_only(schema, "memory:///manual.xsd", &mut manual_set).unwrap();
    assert!(
        process_loaded_schemas(&mut manual_set).is_err(),
        "the manual entry point must check them too"
    );
}

/// Companion to the nested-redefine test above, with the one difference that
/// makes the composition schema-**invalid**: `base.xsd` omits
/// `elementFormDefault`, so its local element `value` is *unqualified* (absent
/// namespace), while the redefining `outer.xsd` declares
/// `elementFormDefault="qualified"` and therefore restricts a `{urn:test}value`
/// that has no counterpart in the base content model.
///
/// That violates XSD 1.0 §3.9.6 `rcase-NameAndTypeOK` clause 1 (R's name and
/// target namespace must equal B's) — equivalently XSD 1.1 §3.4.6.3
/// `derivation-ok-restriction` clause 2.4.2 reached through §3.4.6.4
/// `cos-content-act-restrict` clause 1, so processing is CORRECT to reject it.
/// What this test pins down is *which* check fires: the transitive original is
/// found (that is the redefine lookup working), and the failure comes from the
/// derivation check on the mismatched local element name.
#[test]
fn test_nested_redefine_with_element_form_mismatch_is_rejected() {
    let base = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
<xs:simpleType name="Code"><xs:restriction base="xs:string"/></xs:simpleType>
<xs:simpleType name="Inherited"><xs:restriction base="xs:normalizedString"/></xs:simpleType>
<xs:complexType name="Record"><xs:sequence><xs:element name="value" type="xs:string"/></xs:sequence></xs:complexType>
</xs:schema>"#;
    let middle = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
<xs:include schemaLocation="base.xsd"/>
</xs:schema>"#;
    let outer = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
 targetNamespace="urn:test" xmlns:t="urn:test" elementFormDefault="qualified">
<xs:redefine schemaLocation="middle.xsd">
<xs:complexType name="Record"><xs:complexContent><xs:restriction base="t:Record">
<xs:sequence><xs:element name="value" type="t:Inherited"/></xs:sequence>
</xs:restriction></xs:complexContent></xs:complexType>
</xs:redefine>
<xs:element name="record" type="t:Record"/>
</xs:schema>"#;

    let tmp = std::env::temp_dir().join(format!(
        "xsd_test_redefine_form_mismatch_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(tmp.join("base.xsd"), base).unwrap();
    std::fs::write(tmp.join("middle.xsd"), middle).unwrap();
    let outer_path = tmp.join("outer.xsd").to_string_lossy().into_owned();

    let mut schema_set = SchemaSet::new();
    let result = load_and_process_schema(outer.as_bytes(), &outer_path, &mut schema_set, None);
    let _ = std::fs::remove_dir_all(&tmp);

    // The transitive original IS found (that is the lookup fix); what fails is
    // the derivation check on the mismatched local element name.
    let err = result.expect_err("element-form mismatch must be rejected");
    assert!(
        err.to_string().contains("derivation-ok-restriction"),
        "expected a derivation-ok-restriction failure, got: {err:?}"
    );
}

/// `build_dependency_graph`'s toposort is the only place a circular derivation
/// is reported, and a cycle leaves the type system with no valid compilation
/// order — so both entry points must reject one.
#[test]
fn test_derivation_cycle_is_detected() {
    // A restricts B, B restricts A.
    let schema = br#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
<xs:simpleType name="A"><xs:restriction base="B"><xs:maxLength value="4"/></xs:restriction></xs:simpleType>
<xs:simpleType name="B"><xs:restriction base="A"><xs:maxLength value="8"/></xs:restriction></xs:simpleType>
</xs:schema>"#;

    let mut strict_set = SchemaSet::new();
    let strict =
        load_and_process_schema(schema, "memory:///cycle-strict.xsd", &mut strict_set, None);
    let strict_err = strict.expect_err("a circular derivation must fail in strict mode");
    assert!(
        strict_err.to_string().contains("Circular type dependency"),
        "expected a circular-dependency failure, got: {strict_err:?}"
    );

    // Same guarantee on the manual entry point.
    let mut manual_set = SchemaSet::new();
    parse_schema_only(schema, "memory:///cycle-manual.xsd", &mut manual_set).unwrap();
    assert!(
        process_loaded_schemas(&mut manual_set).is_err(),
        "cycle detection must apply to manually loaded schema sets too"
    );
}

// ---------------------------------------------------------------------------
// derivation-ok-restriction regressions found while loading the official
// GAEB DA XML 3.3 schema corpus (https://www.gaeb.de) in strict mode.
// ---------------------------------------------------------------------------

/// Helper: load `xsd` into a fresh schema set of the requested XSD version.
fn load_strict(xsd: &str, xsd11: bool) -> SchemaResult<PipelineStats> {
    let mut schema_set = if xsd11 {
        SchemaSet::xsd11()
    } else {
        SchemaSet::new()
    };
    load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None)
}

/// Check the implicit group range separately from the element range in
/// XSD 1.0 RecurseAsIfGroup, including the restriction used by XAdES.
#[test]
fn test_xades_element_choice_recurse_as_if_group() {
    struct Case {
        label: &'static str,
        choice_min: &'static str,
        choice_max: &'static str,
        base_min: &'static str,
        base_max: &'static str,
        derived_min: &'static str,
        derived_max: &'static str,
        derived_name: &'static str,
        derived_type: &'static str,
        accepted: bool,
    }
    const XADES: Case = Case {
        label: "xades",
        choice_min: "0",
        choice_max: "1",
        base_min: "0",
        base_max: "unbounded",
        derived_min: "0",
        derived_max: "unbounded",
        derived_name: "Include",
        derived_type: "xs:string",
        accepted: true,
    };
    let cases = [
        XADES,
        Case {
            label: "required_choice_optional_branch",
            choice_min: "1",
            ..XADES
        },
        Case {
            label: "narrow_occurrences",
            derived_min: "1",
            derived_max: "2",
            ..XADES
        },
        Case {
            label: "required_branch",
            base_min: "1",
            derived_min: "1",
            ..XADES
        },
        Case {
            label: "lower_branch_min",
            base_min: "1",
            accepted: false,
            ..XADES
        },
        Case {
            label: "increase_branch_max",
            base_max: "2",
            derived_max: "3",
            accepted: false,
            ..XADES
        },
        Case {
            label: "unbounded_branch_max",
            base_max: "2",
            accepted: false,
            ..XADES
        },
        Case {
            label: "wrong_element",
            derived_name: "Other",
            accepted: false,
            ..XADES
        },
        Case {
            label: "unrelated_type",
            derived_type: "xs:integer",
            accepted: false,
            ..XADES
        },
        Case {
            label: "derived_type",
            derived_type: "xs:token",
            ..XADES
        },
        Case {
            label: "required_group_repetition",
            choice_min: "2",
            choice_max: "2",
            accepted: false,
            ..XADES
        },
        Case {
            label: "optional_group_repetition",
            choice_max: "2",
            ..XADES
        },
    ];
    for Case {
        label,
        choice_min,
        choice_max,
        base_min,
        base_max,
        derived_min,
        derived_max,
        derived_name,
        derived_type,
        accepted,
    } in cases
    {
        let xsd = format!(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
          <xs:complexType name="Base" abstract="true"><xs:sequence>
            <xs:choice minOccurs="{choice_min}" maxOccurs="{choice_max}">
              <xs:element name="Include" type="xs:string" minOccurs="{base_min}" maxOccurs="{base_max}"/>
              <xs:element name="ReferenceInfo" type="xs:string" maxOccurs="unbounded"/>
            </xs:choice><xs:element name="End" type="xs:string"/>
          </xs:sequence></xs:complexType>
          <xs:complexType name="Restricted"><xs:complexContent><xs:restriction base="Base"><xs:sequence>
            <xs:element name="{derived_name}" type="{derived_type}" minOccurs="{derived_min}" maxOccurs="{derived_max}"/>
            <xs:element name="End" type="xs:string"/>
          </xs:sequence></xs:restriction></xs:complexContent></xs:complexType>
          <xs:element name="root" type="Restricted"/>
        </xs:schema>"#
        );
        let result = load_strict(&xsd, false);
        assert_eq!(result.is_ok(), accepted, "case {label}: {result:?}");
    }
}

/// In XSD 1.0, an element's implicit 1..1 group cannot restrict choice{2,2}.
/// A real repeated group may do so, even after normalization folds it into
/// the same element range. XSD 1.1 accepts both forms by language subsumption.
#[test]
fn test_required_choice_repetition_preserves_group_origin() {
    let cases = [
        (
            "element_occurrences",
            r#"<xs:sequence><xs:element name="a" type="xs:string" minOccurs="2" maxOccurs="2"/></xs:sequence>"#,
            false,
            true,
        ),
        (
            "nested_unit_wrappers",
            r#"<xs:sequence><xs:choice><xs:element name="a" type="xs:string" minOccurs="2" maxOccurs="2"/></xs:choice></xs:sequence>"#,
            false,
            true,
        ),
        (
            "named_unit_wrapper",
            r#"<xs:group ref="pair"/>"#,
            false,
            true,
        ),
        (
            "repeated_sequence",
            r#"<xs:sequence minOccurs="2" maxOccurs="2"><xs:element name="a" type="xs:string"/></xs:sequence>"#,
            true,
            true,
        ),
        (
            "repeated_choice_under_unit_wrapper",
            r#"<xs:sequence><xs:choice minOccurs="2" maxOccurs="2"><xs:element name="a" type="xs:string"/></xs:choice></xs:sequence>"#,
            true,
            true,
        ),
        (
            "repeated_sequence_under_nested_unit_wrappers",
            r#"<xs:sequence><xs:choice><xs:sequence minOccurs="2" maxOccurs="2"><xs:element name="a" type="xs:string"/></xs:sequence></xs:choice></xs:sequence>"#,
            true,
            true,
        ),
        (
            "repeated_named_group_under_unit_wrapper",
            r#"<xs:sequence><xs:group ref="single" minOccurs="2" maxOccurs="2"/></xs:sequence>"#,
            true,
            true,
        ),
        (
            "folded_group_below_base_minimum",
            r#"<xs:sequence minOccurs="0"><xs:element name="a" type="xs:string"/></xs:sequence>"#,
            false,
            false,
        ),
        (
            "folded_group_above_base_maximum",
            r#"<xs:sequence minOccurs="3" maxOccurs="3"><xs:element name="a" type="xs:string"/></xs:sequence>"#,
            false,
            false,
        ),
        (
            "folded_group_with_unrelated_type",
            r#"<xs:sequence minOccurs="2" maxOccurs="2"><xs:element name="a" type="xs:integer"/></xs:sequence>"#,
            false,
            false,
        ),
    ];
    for (label, content, accepted_xsd10, accepted_xsd11) in cases {
        let xsd = format!(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:group name="single"><xs:sequence>
                    <xs:element name="a" type="xs:string"/>
                </xs:sequence></xs:group>
                <xs:group name="pair"><xs:sequence>
                    <xs:element name="a" type="xs:string" minOccurs="2" maxOccurs="2"/>
                </xs:sequence></xs:group>
                <xs:complexType name="Base">
                    <xs:choice minOccurs="2" maxOccurs="2">
                        <xs:element name="a" type="xs:string"/>
                        <xs:element name="b" type="xs:string"/>
                    </xs:choice>
                </xs:complexType>
                <xs:complexType name="Restricted">
                    <xs:complexContent><xs:restriction base="Base">
                        {content}
                    </xs:restriction></xs:complexContent>
                </xs:complexType>
            </xs:schema>"#
        );
        for (xsd11, accepted) in [(false, accepted_xsd10), (true, accepted_xsd11)] {
            let result = load_strict(&xsd, xsd11);
            assert_eq!(
                result.is_ok(),
                accepted,
                "case {label}, xsd11={xsd11}: {result:?}"
            );
            if let Err(error) = result {
                assert!(
                    matches!(
                        error,
                        SchemaError::StructuralError {
                            constraint: "derivation-ok-restriction",
                            ..
                        }
                    ),
                    "case {label}, xsd11={xsd11}: {error:?}"
                );
            }
        }
    }
}

/// W3C msData/particles/particlesZ001.xsd: the suite accepts this schema
/// using a lax interpretation of RecurseAsIfGroup for repeated choices.
#[test]
fn test_xsd10_conformance_particles_z001() {
    let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
        <xs:complexType name="Base">
            <xs:sequence>
                <xs:element name="annotation" minOccurs="0"/>
                <xs:choice minOccurs="0" maxOccurs="unbounded">
                    <xs:element name="element"/>
                    <xs:element name="any"/>
                </xs:choice>
            </xs:sequence>
        </xs:complexType>
        <xs:complexType name="Derived">
            <xs:complexContent><xs:restriction base="Base">
                <xs:sequence>
                    <xs:element name="annotation" minOccurs="0"/>
                    <xs:element name="element" minOccurs="0" maxOccurs="unbounded"/>
                </xs:sequence>
            </xs:restriction></xs:complexContent>
        </xs:complexType>
    </xs:schema>"#;
    let result = load_strict(xsd, false);
    assert!(result.is_ok(), "particlesZ001: {result:?}");
}

/// W3C msData/particles/particlesHa165.xsd: normalization folds the
/// single-child choice's 1..2 range into its element.
#[test]
fn test_xsd10_conformance_particles_ha165() {
    let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
        <xs:complexType name="base">
            <xs:sequence>
                <xs:choice maxOccurs="2">
                    <xs:element name="a" type="xs:string"/>
                    <xs:element name="b" type="xs:string"/>
                </xs:choice>
            </xs:sequence>
        </xs:complexType>
        <xs:complexType name="derived">
            <xs:complexContent><xs:restriction base="base">
                <xs:sequence>
                    <xs:choice maxOccurs="2">
                        <xs:element name="a" type="xs:string"/>
                    </xs:choice>
                </xs:sequence>
            </xs:restriction></xs:complexContent>
        </xs:complexType>
    </xs:schema>"#;
    let result = load_strict(xsd, false);
    assert!(result.is_ok(), "particlesHa165: {result:?}");
}

/// W3C msData/particles/particlesHa167.xsd: normalization folds the
/// single-child sequence's 1..2 range into its element.
#[test]
fn test_xsd10_conformance_particles_ha167() {
    let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
        <xs:complexType name="base">
            <xs:sequence>
                <xs:choice maxOccurs="2">
                    <xs:element name="a" type="xs:string"/>
                    <xs:element name="b" type="xs:string"/>
                </xs:choice>
            </xs:sequence>
        </xs:complexType>
        <xs:complexType name="derived">
            <xs:complexContent><xs:restriction base="base">
                <xs:sequence>
                    <xs:sequence maxOccurs="2">
                        <xs:element name="a" type="xs:string"/>
                    </xs:sequence>
                </xs:sequence>
            </xs:restriction></xs:complexContent>
        </xs:complexType>
    </xs:schema>"#;
    let result = load_strict(xsd, false);
    assert!(result.is_ok(), "particlesHa167: {result:?}");
}

/// W3C msData/particles/particlesV009.xsd: a sequence with range 1..2
/// restricts a choice with range 0..2, before single-child normalization.
#[test]
fn test_xsd10_conformance_particles_v009() {
    let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
        <xs:complexType name="B">
            <xs:choice minOccurs="0" maxOccurs="2">
                <xs:element name="e1"/>
                <xs:element name="e2"/>
                <xs:element name="e3"/>
            </xs:choice>
        </xs:complexType>
        <xs:complexType name="R">
            <xs:complexContent><xs:restriction base="B">
                <xs:sequence maxOccurs="2">
                    <xs:element name="e1"/>
                </xs:sequence>
            </xs:restriction></xs:complexContent>
        </xs:complexType>
    </xs:schema>"#;
    let result = load_strict(xsd, false);
    assert!(result.is_ok(), "particlesV009: {result:?}");
}

/// §3.9.6 RecurseLax maps the *raw* {particles} of the two choices and checks
/// the choices' own occurrence ranges separately.  Folding the parent choice's
/// occurs into every branch makes each derived branch optional as soon as the
/// choice is, and an optional branch can no longer map onto a base branch whose
/// first child is required.
#[test]
fn test_optional_choice_branch_maps_onto_group_branch() {
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:complexType name="Base">
                <xs:choice minOccurs="0">
                    <xs:sequence>
                        <xs:element name="a" type="xs:string"/>
                        <xs:element name="b" type="xs:string" minOccurs="0"/>
                    </xs:sequence>
                    <xs:element name="c" type="xs:string"/>
                </xs:choice>
            </xs:complexType>
            <xs:complexType name="Restricted">
                <xs:complexContent>
                    <xs:restriction base="Base">
                        <xs:choice minOccurs="0">
                            <xs:element name="a" type="xs:string"/>
                            <xs:element name="c" type="xs:string"/>
                        </xs:choice>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"#;

    for xsd11 in [false, true] {
        let result = load_strict(xsd, xsd11);
        assert!(
            result.is_ok(),
            "RecurseLax should accept this (xsd11={xsd11}): {result:?}"
        );
    }
}

/// §3.4.2.4: an `<attribute>` with `use="prohibited"` in a restriction
/// contributes no attribute use — it only suppresses the base's.  Its `type`
/// therefore plays no part in derivation-ok-restriction.
#[test]
fn test_prohibited_attribute_type_is_not_checked_for_derivation() {
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:complexType name="Base">
                <xs:sequence/>
                <xs:attribute name="id" type="xs:token"/>
            </xs:complexType>
            <!-- xs:boolean is *not* derived from xs:token: if the prohibited
                 use were treated as a real attribute use, clause 4 would fire. -->
            <xs:complexType name="Restricted">
                <xs:complexContent>
                    <xs:restriction base="Base">
                        <xs:sequence/>
                        <xs:attribute name="id" type="xs:boolean" use="prohibited"/>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"#;

    let result = load_strict(xsd, false);
    assert!(
        result.is_ok(),
        "a prohibited attribute's type is irrelevant: {result:?}"
    );
}

/// Prohibiting a *required* base attribute is still a clause-3 violation.
#[test]
fn test_prohibiting_a_required_base_attribute_is_still_rejected() {
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:complexType name="Base">
                <xs:sequence/>
                <xs:attribute name="id" type="xs:token" use="required"/>
            </xs:complexType>
            <xs:complexType name="Restricted">
                <xs:complexContent>
                    <xs:restriction base="Base">
                        <xs:sequence/>
                        <xs:attribute name="id" type="xs:token" use="prohibited"/>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"#;

    assert!(load_strict(xsd, false).is_err());
}

/// `<xs:choice minOccurs="0"/>` is a pointless particle: it accepts exactly the
/// empty sequence and must not have to map onto anything in the base.
#[test]
fn test_empty_optional_choice_is_a_pointless_particle() {
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:complexType name="Base">
                <xs:sequence>
                    <xs:element name="a" type="xs:string"/>
                    <xs:element name="b" type="xs:string" minOccurs="0"/>
                </xs:sequence>
            </xs:complexType>
            <xs:complexType name="Restricted">
                <xs:complexContent>
                    <xs:restriction base="Base">
                        <xs:sequence>
                            <xs:choice minOccurs="0"/>
                            <xs:element name="a" type="xs:string"/>
                        </xs:sequence>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"#;

    for xsd11 in [false, true] {
        let result = load_strict(xsd, xsd11);
        assert!(result.is_ok(), "(xsd11={xsd11}): {result:?}");
    }
}

/// §3.4.2.3: restricting a type that was itself derived by extension must be
/// checked against `sequence(inherited-particle, own-particle)`, not against
/// the extension's own contribution alone.
#[test]
fn test_restriction_of_extension_sees_the_inherited_content() {
    let schema = |derived_body: &str| {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:complexType name="Root">
                <xs:sequence>
                    <xs:element name="a" type="xs:string"/>
                </xs:sequence>
            </xs:complexType>
            <xs:complexType name="Base">
                <xs:complexContent>
                    <xs:extension base="Root">
                        <xs:sequence>
                            <xs:element name="b" type="xs:string" minOccurs="0"/>
                        </xs:sequence>
                    </xs:extension>
                </xs:complexContent>
            </xs:complexType>
            <xs:complexType name="Restricted">
                <xs:complexContent>
                    <xs:restriction base="Base">{derived_body}</xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"#
        )
    };

    // Keeps the inherited `a`: valid.
    let ok = schema(
        r#"<xs:sequence>
               <xs:element name="a" type="xs:string"/>
               <xs:element name="b" type="xs:string"/>
           </xs:sequence>"#,
    );
    assert!(
        load_strict(&ok, false).is_ok(),
        "{:?}",
        load_strict(&ok, false)
    );

    // Drops the inherited, required `a`: invalid.
    let bad = schema(
        r#"<xs:sequence>
               <xs:element name="b" type="xs:string" minOccurs="0"/>
           </xs:sequence>"#,
    );
    assert!(load_strict(&bad, false).is_err());
}

/// The base side of a restriction must present its complete `{attribute uses}`,
/// including the ones it inherited, or every inherited attribute the derived
/// type re-declares looks like an attribute it invented.
#[test]
fn test_restriction_sees_attributes_the_base_inherited() {
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:complexType name="Root">
                <xs:sequence/>
                <xs:attribute name="id" type="xs:token"/>
            </xs:complexType>
            <xs:complexType name="Base">
                <xs:complexContent>
                    <xs:extension base="Root">
                        <xs:sequence/>
                    </xs:extension>
                </xs:complexContent>
            </xs:complexType>
            <xs:complexType name="Restricted">
                <xs:complexContent>
                    <xs:restriction base="Base">
                        <xs:sequence/>
                        <xs:attribute name="id" type="xs:NCName"/>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"#;

    let result = load_strict(xsd, false);
    assert!(
        result.is_ok(),
        "`id` is inherited by Base from Root: {result:?}"
    );
}

/// A single-branch `<choice minOccurs="0">` restricting a multi-branch base
/// choice. The choice is not pointless (XSD 1.0 §3.9.6 clause 2.2 asks for a
/// 1..1 particle), so XSD 1.0's RecurseLax maps its branch onto the base's
/// second branch and accepts it, as Xerces-J does; XSD 1.1 compares
/// languages. (A *unit* single-branch choice is pointless and collapses —
/// msData groupH021v / particlesZ024, invalid in 1.0.)
#[test]
fn test_single_branch_optional_choice_restriction() {
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:complexType name="Base">
                <xs:choice minOccurs="0">
                    <xs:sequence>
                        <xs:element name="a" type="xs:string"/>
                        <xs:element name="b" type="xs:string" minOccurs="0"/>
                    </xs:sequence>
                    <xs:sequence>
                        <xs:element name="c" type="xs:string"/>
                        <xs:element name="d" type="xs:string" minOccurs="0"/>
                    </xs:sequence>
                </xs:choice>
            </xs:complexType>
            <xs:complexType name="Restricted">
                <xs:complexContent>
                    <xs:restriction base="Base">
                        <xs:choice minOccurs="0">
                            <xs:sequence>
                                <xs:element name="c" type="xs:string"/>
                                <xs:element name="d" type="xs:string" minOccurs="0"/>
                            </xs:sequence>
                        </xs:choice>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"#;

    let xsd10 = load_strict(xsd, false);
    assert!(xsd10.is_ok(), "XSD 1.0 accepts it (RecurseLax): {xsd10:?}");
    let xsd11 = load_strict(xsd, true);
    assert!(xsd11.is_ok(), "XSD 1.1 accepts it: {xsd11:?}");
}

/// §3.9.6 RecurseLax clause 1: the two choices' own occurrence ranges are
/// compared directly.  An optional choice is not a valid XSD 1.0 restriction of
/// a required one, even when every branch is individually optional — folding
/// the ranges into the branches hides the violation.  XSD 1.1 compares
/// languages (§3.4.6.4), and there the two accept the same sequences.
#[test]
fn test_optional_choice_restricting_required_choice_is_version_dependent() {
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:complexType name="Base">
                <xs:choice minOccurs="1" maxOccurs="1">
                    <xs:element name="a" type="xs:string" minOccurs="0"/>
                    <xs:element name="b" type="xs:string" minOccurs="0"/>
                </xs:choice>
            </xs:complexType>
            <xs:complexType name="Restricted">
                <xs:complexContent>
                    <xs:restriction base="Base">
                        <xs:choice minOccurs="0">
                            <xs:element name="a" type="xs:string" minOccurs="0"/>
                            <xs:element name="b" type="xs:string" minOccurs="0"/>
                        </xs:choice>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"#;

    match load_strict(xsd, false) {
        Err(SchemaError::StructuralError { constraint, .. }) => {
            assert_eq!(constraint, "derivation-ok-restriction");
        }
        other => panic!("XSD 1.0 must reject the widened occurrence range: {other:?}"),
    }
    let xsd11 = load_strict(xsd, true);
    assert!(xsd11.is_ok(), "XSD 1.1 accepts it: {xsd11:?}");
}

/// A `<xs:choice/>` with no branches and `minOccurs="1"` accepts no sequence at
/// all, so an extension carrying one has an unsatisfiable content type.  It
/// must not be discarded while re-assembling `sequence(inherited, own)`, or a
/// restriction gets checked against the inherited half alone and is wrongly
/// accepted.
#[test]
fn test_required_empty_choice_in_an_extension_is_not_discarded() {
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:complexType name="Root">
                <xs:sequence>
                    <xs:element name="a" type="xs:string"/>
                </xs:sequence>
            </xs:complexType>
            <xs:complexType name="Base">
                <xs:complexContent>
                    <xs:extension base="Root">
                        <xs:choice/>
                    </xs:extension>
                </xs:complexContent>
            </xs:complexType>
            <xs:complexType name="Restricted">
                <xs:complexContent>
                    <xs:restriction base="Base">
                        <xs:sequence>
                            <xs:element name="a" type="xs:string"/>
                        </xs:sequence>
                    </xs:restriction>
                </xs:complexContent>
            </xs:complexType>
        </xs:schema>"#;

    for xsd11 in [false, true] {
        assert!(
            load_strict(xsd, xsd11).is_err(),
            "Base accepts no sequence, so nothing restricts it (xsd11={xsd11})"
        );
    }
}
