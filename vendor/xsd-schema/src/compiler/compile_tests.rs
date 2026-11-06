use super::*;
use crate::parser::frames::OpenContentMode;
use crate::parser::location::{SourceRef, SourceSpan};
use crate::schema::model::{
    DefaultOpenContent, OpenContentMode as SchemaOpenContentMode, XsdVersion,
};
use crate::schema::wildcard::ElementWildcard;
use crate::schema::SchemaDocument;

fn make_element_particle(name: NameId, min: u32, max: Option<u32>) -> ParticleResult {
    ParticleResult {
        term: ParticleTerm::Element(ElementFrameResult {
            name: Some(name),
            ref_name: None,
            target_namespace: None,
            type_ref: None,
            inline_type: None,
            substitution_group: vec![],
            default_value: None,
            fixed_value: None,
            nillable: false,
            is_abstract: false,
            min_occurs: 1,
            max_occurs: Some(1),
            block: Default::default(),
            final_derivation: Default::default(),
            form: None,
            id: None,
            alternatives: vec![],
            identity_constraints: vec![],
            identity_constraint_refs: vec![],
            annotation: None,
            source: None,
        }),
        min_occurs: min,
        max_occurs: max,
        source: None,
    }
}

fn make_sequence_particle(particles: Vec<ParticleResult>) -> ParticleResult {
    ParticleResult {
        term: ParticleTerm::Group(ModelGroupDefResult {
            name: None,
            ref_name: None,
            compositor: Some(Compositor::Sequence),
            particles,
            min_occurs: 1,
            max_occurs: Some(1),
            id: None,
            annotation: None,
            source: None,
        }),
        min_occurs: 1,
        max_occurs: Some(1),
        source: None,
    }
}

fn make_choice_particle(particles: Vec<ParticleResult>) -> ParticleResult {
    ParticleResult {
        term: ParticleTerm::Group(ModelGroupDefResult {
            name: None,
            ref_name: None,
            compositor: Some(Compositor::Choice),
            particles,
            min_occurs: 1,
            max_occurs: Some(1),
            id: None,
            annotation: None,
            source: None,
        }),
        min_occurs: 1,
        max_occurs: Some(1),
        source: None,
    }
}

fn make_complex_type_data(
    source: Option<SourceRef>,
    content: ComplexContentResult,
) -> ComplexTypeDefData {
    ComplexTypeDefData {
        name: None,
        target_namespace: None,
        base_type: None,
        derivation_method: None,
        content,
        open_content: None,
        attributes: Vec::new(),
        attribute_groups: Vec::new(),
        attribute_wildcard: None,
        mixed: false,
        is_abstract: false,
        final_derivation: Default::default(),
        block: Default::default(),
        default_attributes_apply: true,
        id: None,
        #[cfg(feature = "xsd11")]
        assertions: Vec::new(),
        #[cfg(feature = "xsd11")]
        xpath_default_namespace: None,
        annotation: None,
        source,
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
fn test_compile_single_element() {
    let schema_set = SchemaSet::new();
    let particle = make_element_particle(NameId(1), 1, Some(1));

    let table = compile_particle(&schema_set, &particle, None).unwrap();

    assert!(table.state_count() >= 2); // At least start and end
}

#[test]
fn test_compile_optional_element() {
    let schema_set = SchemaSet::new();
    let particle = make_element_particle(NameId(1), 0, Some(1));

    let table = compile_particle(&schema_set, &particle, None).unwrap();

    // Optional should have epsilon bypass
    let start = table.get_state(table.start_state).unwrap();
    assert!(start.epsilon_transitions().count() > 0);
}

#[test]
fn test_compile_sequence() {
    let schema_set = SchemaSet::new();
    let particle = make_sequence_particle(vec![
        make_element_particle(NameId(1), 1, Some(1)),
        make_element_particle(NameId(2), 1, Some(1)),
    ]);

    let table = compile_particle(&schema_set, &particle, None).unwrap();

    // Sequence of 2 elements should have multiple states
    assert!(table.state_count() >= 4);
}

#[test]
fn test_compile_choice() {
    let schema_set = SchemaSet::new();
    let particle = make_choice_particle(vec![
        make_element_particle(NameId(1), 1, Some(1)),
        make_element_particle(NameId(2), 1, Some(1)),
    ]);

    let table = compile_particle(&schema_set, &particle, None).unwrap();

    // Choice should have branch states
    assert!(table.state_count() >= 4);
}

#[test]
fn test_default_open_content_applies_to_empty_complex_type() {
    let mut schema_set = SchemaSet::with_version(XsdVersion::V1_1);
    let doc_id = schema_set.documents.len() as u32;
    let mut doc = SchemaDocument::new(doc_id, "test.xsd".to_string());
    doc.default_open_content = Some(DefaultOpenContent {
        source: None,
        applies_to_empty: true,
        mode: SchemaOpenContentMode::Suffix,
        wildcard: Some(ElementWildcard::any_lax()),
    });
    schema_set.documents.push(doc);

    let source = SourceRef::new(doc_id, SourceSpan::new(0, 0));
    let ct_key = schema_set.arenas.alloc_complex_type(make_complex_type_data(
        Some(source),
        ComplexContentResult::Empty,
    ));
    let type_def = schema_set.arenas.complex_types.get(ct_key).unwrap();

    let matcher = compile_content_model_matcher(&schema_set, type_def).unwrap();
    match matcher {
        ContentModelMatcher::WithOpenContent { mode, wildcard, .. } => {
            assert_eq!(mode, TypesOpenContentMode::Suffix);
            assert!(wildcard.is_some());
        }
        _ => panic!("expected open content wrapper"),
    }
}

#[test]
fn test_default_open_content_skipped_when_not_applies_to_empty() {
    let mut schema_set = SchemaSet::with_version(XsdVersion::V1_1);
    let doc_id = schema_set.documents.len() as u32;
    let mut doc = SchemaDocument::new(doc_id, "test.xsd".to_string());
    doc.default_open_content = Some(DefaultOpenContent {
        source: None,
        applies_to_empty: false,
        mode: SchemaOpenContentMode::Interleave,
        wildcard: Some(ElementWildcard::any_lax()),
    });
    schema_set.documents.push(doc);

    let source = SourceRef::new(doc_id, SourceSpan::new(0, 0));
    let ct_key = schema_set.arenas.alloc_complex_type(make_complex_type_data(
        Some(source),
        ComplexContentResult::Empty,
    ));
    let type_def = schema_set.arenas.complex_types.get(ct_key).unwrap();

    let matcher = compile_content_model_matcher(&schema_set, type_def).unwrap();
    assert!(matches!(matcher, ContentModelMatcher::Nfa(_)));
}

#[test]
fn test_invalid_occurrence() {
    let schema_set = SchemaSet::new();
    let particle = ParticleResult {
        term: ParticleTerm::Element(ElementFrameResult {
            name: Some(NameId(1)),
            ref_name: None,
            target_namespace: None,
            type_ref: None,
            inline_type: None,
            substitution_group: vec![],
            default_value: None,
            fixed_value: None,
            nillable: false,
            is_abstract: false,
            min_occurs: 1,
            max_occurs: Some(1),
            block: Default::default(),
            final_derivation: Default::default(),
            form: None,
            id: None,
            alternatives: vec![],
            identity_constraints: vec![],
            identity_constraint_refs: vec![],
            annotation: None,
            source: None,
        }),
        min_occurs: 5, // min > max
        max_occurs: Some(3),
        source: None,
    };

    let result = compile_particle(&schema_set, &particle, None);
    assert!(matches!(
        result,
        Err(NfaCompileError::InvalidOccurrence { .. })
    ));
}

#[test]
fn test_element_form_default_applies_to_local_element() {
    let mut schema_set = SchemaSet::new();
    let target_ns = schema_set.name_table.add("http://example.com");
    let name = schema_set.name_table.add("local");

    let doc_id = schema_set.documents.len() as u32;
    let mut doc = SchemaDocument::new(doc_id, "test.xsd".to_string());
    doc.target_namespace = Some(target_ns);
    doc.element_form_default = FormChoice::Qualified;
    schema_set.documents.push(doc);

    let source_ref = SourceRef::new(doc_id, SourceSpan::new(0, 0));
    let particle = ParticleResult {
        term: ParticleTerm::Element(ElementFrameResult {
            name: Some(name),
            ref_name: None,
            target_namespace: None,
            type_ref: None,
            inline_type: None,
            substitution_group: vec![],
            default_value: None,
            fixed_value: None,
            nillable: false,
            is_abstract: false,
            min_occurs: 1,
            max_occurs: Some(1),
            block: Default::default(),
            final_derivation: Default::default(),
            form: None,
            id: None,
            alternatives: vec![],
            identity_constraints: vec![],
            identity_constraint_refs: vec![],
            annotation: None,
            source: Some(source_ref.clone()),
        }),
        min_occurs: 1,
        max_occurs: Some(1),
        source: Some(source_ref),
    };

    let table = compile_particle(&schema_set, &particle, Some(target_ns)).unwrap();
    let term = table
        .states
        .iter()
        .find_map(|state| state.term.as_ref())
        .expect("expected element term");
    match term {
        NfaTerm::Element { namespace, .. } => {
            assert_eq!(*namespace, Some(target_ns));
        }
        _ => panic!("expected element term"),
    }
}

#[test]
fn test_element_form_override_unqualified() {
    let mut schema_set = SchemaSet::new();
    let target_ns = schema_set.name_table.add("http://example.com");
    let name = schema_set.name_table.add("local");

    let doc_id = schema_set.documents.len() as u32;
    let mut doc = SchemaDocument::new(doc_id, "test.xsd".to_string());
    doc.target_namespace = Some(target_ns);
    doc.element_form_default = FormChoice::Qualified;
    schema_set.documents.push(doc);

    let source_ref = SourceRef::new(doc_id, SourceSpan::new(0, 0));
    let particle = ParticleResult {
        term: ParticleTerm::Element(ElementFrameResult {
            name: Some(name),
            ref_name: None,
            target_namespace: None,
            type_ref: None,
            inline_type: None,
            substitution_group: vec![],
            default_value: None,
            fixed_value: None,
            nillable: false,
            is_abstract: false,
            min_occurs: 1,
            max_occurs: Some(1),
            block: Default::default(),
            final_derivation: Default::default(),
            form: Some("unqualified".to_string()),
            id: None,
            alternatives: vec![],
            identity_constraints: vec![],
            identity_constraint_refs: vec![],
            annotation: None,
            source: Some(source_ref.clone()),
        }),
        min_occurs: 1,
        max_occurs: Some(1),
        source: Some(source_ref),
    };

    let table = compile_particle(&schema_set, &particle, Some(target_ns)).unwrap();
    let term = table
        .states
        .iter()
        .find_map(|state| state.term.as_ref())
        .expect("expected element term");
    match term {
        NfaTerm::Element { namespace, .. } => {
            assert_eq!(*namespace, None);
        }
        _ => panic!("expected element term"),
    }
}

fn make_all_particle(particles: Vec<ParticleResult>) -> ParticleResult {
    ParticleResult {
        term: ParticleTerm::Group(ModelGroupDefResult {
            name: None,
            ref_name: None,
            compositor: Some(Compositor::All),
            particles,
            min_occurs: 1,
            max_occurs: Some(1),
            id: None,
            annotation: None,
            source: None,
        }),
        min_occurs: 1,
        max_occurs: Some(1),
        source: None,
    }
}

fn make_all_particle_with_occurs(
    particles: Vec<ParticleResult>,
    min_occurs: u32,
    max_occurs: Option<u32>,
) -> ParticleResult {
    ParticleResult {
        term: ParticleTerm::Group(ModelGroupDefResult {
            name: None,
            ref_name: None,
            compositor: Some(Compositor::All),
            particles,
            min_occurs: 1,
            max_occurs: Some(1),
            id: None,
            annotation: None,
            source: None,
        }),
        min_occurs,
        max_occurs,
        source: None,
    }
}

fn make_complex_type_with_content(content: ComplexContentResult) -> ComplexTypeDefData {
    make_complex_type_data(None, content)
}

#[test]
fn test_all_group_produces_all_group_matcher() {
    use crate::parser::frames::ComplexContentDefResult;

    let schema_set = SchemaSet::new();
    let all_particle = make_all_particle(vec![
        make_element_particle(NameId(1), 1, Some(1)),
        make_element_particle(NameId(2), 0, Some(1)),
    ]);

    let content = ComplexContentResult::Complex(ComplexContentDefResult {
        particle: Some(all_particle),
        derivation: DerivationMethod::Restriction,
        mixed: false,
        mixed_explicit: false,
        base_type: None,
        open_content: None,
        attributes: vec![],
        attribute_groups: vec![],
        attribute_wildcard: None,
        assertions: vec![],
        id: None,
        derivation_id: None,
        source: None,
    });

    let type_def = make_complex_type_with_content(content);
    let matcher = compile_content_model_matcher(&schema_set, &type_def).unwrap();

    match &matcher {
        ContentModelMatcher::AllGroup(model) => {
            assert_eq!(model.particle_count(), 2);
            // First particle required, second optional
            assert!(!model.particles[0].is_optional());
            assert!(model.particles[1].is_optional());
        }
        other => panic!("expected AllGroup matcher, got {:?}", other),
    }
}

#[test]
fn test_sequence_still_produces_nfa() {
    use crate::parser::frames::ComplexContentDefResult;

    let schema_set = SchemaSet::new();
    let seq_particle = make_sequence_particle(vec![
        make_element_particle(NameId(1), 1, Some(1)),
        make_element_particle(NameId(2), 1, Some(1)),
    ]);

    let content = ComplexContentResult::Complex(ComplexContentDefResult {
        particle: Some(seq_particle),
        derivation: DerivationMethod::Restriction,
        mixed: false,
        mixed_explicit: false,
        base_type: None,
        open_content: None,
        attributes: vec![],
        attribute_groups: vec![],
        attribute_wildcard: None,
        assertions: vec![],
        id: None,
        derivation_id: None,
        source: None,
    });

    let type_def = make_complex_type_with_content(content);
    let matcher = compile_content_model_matcher(&schema_set, &type_def).unwrap();
    assert!(matches!(matcher, ContentModelMatcher::Nfa(_)));
}

#[test]
fn test_extension_from_all_group_base_no_own_particles() {
    use crate::parser::frames::ComplexContentDefResult;

    let mut schema_set = SchemaSet::new();

    // Create base type with all-group
    let base_all = make_all_particle(vec![
        make_element_particle(NameId(1), 1, Some(1)),
        make_element_particle(NameId(2), 1, Some(1)),
    ]);
    let base_content = ComplexContentResult::Complex(ComplexContentDefResult {
        particle: Some(base_all),
        derivation: DerivationMethod::Restriction,
        mixed: false,
        mixed_explicit: false,
        base_type: None,
        open_content: None,
        attributes: vec![],
        attribute_groups: vec![],
        attribute_wildcard: None,
        assertions: vec![],
        id: None,
        derivation_id: None,
        source: None,
    });
    let base_ct = make_complex_type_data(None, base_content);
    let base_key = schema_set.arenas.alloc_complex_type(base_ct);

    // Create extension type with no own particle
    let ext_content = ComplexContentResult::Complex(ComplexContentDefResult {
        particle: None,
        derivation: DerivationMethod::Extension,
        mixed: false,
        mixed_explicit: false,
        base_type: None,
        open_content: None,
        attributes: vec![],
        attribute_groups: vec![],
        attribute_wildcard: None,
        assertions: vec![],
        id: None,
        derivation_id: None,
        source: None,
    });
    let mut ext_type = make_complex_type_data(None, ext_content);
    ext_type.derivation_method = Some(DerivationMethod::Extension);
    ext_type.resolved_base_type = Some(TypeKey::Complex(base_key));

    let matcher = compile_content_model_matcher(&schema_set, &ext_type).unwrap();
    // Extension with no own particles should inherit AllGroup from base
    assert!(matches!(matcher, ContentModelMatcher::AllGroup(_)));
}

#[test]
fn test_extension_from_all_group_base_with_own_particles() {
    use crate::parser::frames::ComplexContentDefResult;

    let mut schema_set = SchemaSet::new();

    // Create base type with all-group
    let base_all = make_all_particle(vec![make_element_particle(NameId(1), 1, Some(1))]);
    let base_content = ComplexContentResult::Complex(ComplexContentDefResult {
        particle: Some(base_all),
        derivation: DerivationMethod::Restriction,
        mixed: false,
        mixed_explicit: false,
        base_type: None,
        open_content: None,
        attributes: vec![],
        attribute_groups: vec![],
        attribute_wildcard: None,
        assertions: vec![],
        id: None,
        derivation_id: None,
        source: None,
    });
    let base_ct = make_complex_type_data(None, base_content);
    let base_key = schema_set.arenas.alloc_complex_type(base_ct);

    // Create extension type with its own sequence particle
    let ext_seq = make_sequence_particle(vec![make_element_particle(NameId(3), 1, Some(1))]);
    let ext_content = ComplexContentResult::Complex(ComplexContentDefResult {
        particle: Some(ext_seq),
        derivation: DerivationMethod::Extension,
        mixed: false,
        mixed_explicit: false,
        base_type: None,
        open_content: None,
        attributes: vec![],
        attribute_groups: vec![],
        attribute_wildcard: None,
        assertions: vec![],
        id: None,
        derivation_id: None,
        source: None,
    });
    let mut ext_type = make_complex_type_data(None, ext_content);
    ext_type.derivation_method = Some(DerivationMethod::Extension);
    ext_type.resolved_base_type = Some(TypeKey::Complex(base_key));

    let result = compile_content_model_matcher(&schema_set, &ext_type);
    // XSD 1.0 SchemaSet falls through to the legacy NFA path regardless of
    // cargo feature: the base AllGroup is converted to NFA and concatenated
    // with the extension's own sequence. XSD 1.1 rejection of this shape is
    // covered by `test_extension_all_group_base_with_sequence`.
    assert!(matches!(result.unwrap(), ContentModelMatcher::Nfa(_)));
}

#[test]
fn test_attach_open_content_all_group() {
    use crate::types::complex::{
        NamespaceConstraint, OpenContent, OpenContentMode as TypesOpenContentMode,
        ProcessContents as TypesProcessContents, WildcardRef,
    };

    let schema_set = SchemaSet::with_version(XsdVersion::V1_0);
    let a_name = NameId(1);
    let model = AllGroupModel::new(vec![AllParticle::new(
        NfaTerm::element(a_name, None, None),
        1,
        MaxOccurs::Bounded(1),
        None,
    )]);
    let matcher = ContentModelMatcher::AllGroup(model);
    let oc = OpenContent {
        mode: TypesOpenContentMode::Interleave,
        wildcard: Some(WildcardRef {
            namespace_constraint: NamespaceConstraint::Any,
            process_contents: TypesProcessContents::Lax,
            not_qnames: Vec::new(),
            has_defined_sibling: false,
            source: None,
        }),
        source: None,
    };

    let result = attach_open_content(&schema_set, matcher, Some(oc));
    match result {
        ContentModelMatcher::AllGroup(model) => {
            assert!(
                model.open_content.is_some(),
                "open content should be populated"
            );
            let oc = model.open_content.unwrap();
            assert_eq!(oc.mode, crate::compiler::OpenContentMode::Interleave);
        }
        other => panic!("expected AllGroup, got {:?}", other),
    }
}

#[cfg(feature = "xsd11")]
#[test]
fn test_extension_merged_all_groups() {
    use crate::parser::frames::ComplexContentDefResult;

    let mut schema_set = SchemaSet::with_version(XsdVersion::V1_1);

    // Base type: all(A, B)
    let base_all = make_all_particle(vec![
        make_element_particle(NameId(1), 1, Some(1)),
        make_element_particle(NameId(2), 1, Some(1)),
    ]);
    let base_content = ComplexContentResult::Complex(ComplexContentDefResult {
        particle: Some(base_all),
        derivation: DerivationMethod::Restriction,
        mixed: false,
        mixed_explicit: false,
        base_type: None,
        open_content: None,
        attributes: vec![],
        attribute_groups: vec![],
        attribute_wildcard: None,
        assertions: vec![],
        id: None,
        derivation_id: None,
        source: None,
    });
    let base_ct = make_complex_type_data(None, base_content);
    let base_key = schema_set.arenas.alloc_complex_type(base_ct);

    // Extension type: all(C, D)
    let ext_all = make_all_particle(vec![
        make_element_particle(NameId(3), 1, Some(1)),
        make_element_particle(NameId(4), 0, Some(1)),
    ]);
    let ext_content = ComplexContentResult::Complex(ComplexContentDefResult {
        particle: Some(ext_all),
        derivation: DerivationMethod::Extension,
        mixed: false,
        mixed_explicit: false,
        base_type: None,
        open_content: None,
        attributes: vec![],
        attribute_groups: vec![],
        attribute_wildcard: None,
        assertions: vec![],
        id: None,
        derivation_id: None,
        source: None,
    });
    let mut ext_type = make_complex_type_data(None, ext_content);
    ext_type.derivation_method = Some(DerivationMethod::Extension);
    ext_type.resolved_base_type = Some(TypeKey::Complex(base_key));

    let matcher = compile_content_model_matcher(&schema_set, &ext_type).unwrap();
    // Two all-groups should merge into a single AllGroup with 4 particles
    match &matcher {
        ContentModelMatcher::AllGroup(model) => {
            assert_eq!(model.particle_count(), 4);
        }
        other => panic!("expected AllGroup, got {:?}", other),
    }
}

#[cfg(feature = "xsd11")]
#[test]
fn test_extension_all_group_base_with_sequence() {
    use crate::parser::frames::ComplexContentDefResult;

    let mut schema_set = SchemaSet::with_version(XsdVersion::V1_1);

    // Base type: all(A, B)
    let base_all = make_all_particle(vec![
        make_element_particle(NameId(1), 1, Some(1)),
        make_element_particle(NameId(2), 1, Some(1)),
    ]);
    let base_content = ComplexContentResult::Complex(ComplexContentDefResult {
        particle: Some(base_all),
        derivation: DerivationMethod::Restriction,
        mixed: false,
        mixed_explicit: false,
        base_type: None,
        open_content: None,
        attributes: vec![],
        attribute_groups: vec![],
        attribute_wildcard: None,
        assertions: vec![],
        id: None,
        derivation_id: None,
        source: None,
    });
    let base_ct = make_complex_type_data(None, base_content);
    let base_key = schema_set.arenas.alloc_complex_type(base_ct);

    // Extension type: sequence(C)
    let ext_seq = make_sequence_particle(vec![make_element_particle(NameId(3), 1, Some(1))]);
    let ext_content = ComplexContentResult::Complex(ComplexContentDefResult {
        particle: Some(ext_seq),
        derivation: DerivationMethod::Extension,
        mixed: false,
        mixed_explicit: false,
        base_type: None,
        open_content: None,
        attributes: vec![],
        attribute_groups: vec![],
        attribute_wildcard: None,
        assertions: vec![],
        id: None,
        derivation_id: None,
        source: None,
    });
    let mut ext_type = make_complex_type_data(None, ext_content);
    ext_type.derivation_method = Some(DerivationMethod::Extension);
    ext_type.resolved_base_type = Some(TypeKey::Complex(base_key));

    // XSD 1.1: extending an xs:all base with a non-all particle violates
    // cos-all-limited.1.2 — the result would wrap the all-group inside a
    // sequence, which the spec forbids (§3.4.2.3.3 clause 4.2.3.3).
    let result = compile_content_model_matcher(&schema_set, &ext_type);
    assert!(matches!(
        result,
        Err(crate::compiler::NfaCompileError::InvalidAllGroupOccurs { ref reason, .. })
            if reason.starts_with("cos-all-limited.1:")
    ));
}

// ========================================================================
// End-to-end wildcard conversion tests
// ========================================================================

#[test]
fn test_wildcard_ref_from_result_resolves_target_namespace_in_list() {
    let schema_set = SchemaSet::with_version(XsdVersion::V1_1);
    let target_ns = schema_set.name_table.add("http://target.example.com");
    let other_ns = schema_set.name_table.add("http://other.example.com");

    use crate::parser::frames::NamespaceToken;

    let wildcard = WildcardResult {
        namespace: WildcardNamespace::List(vec![
            NamespaceToken::TargetNamespace,
            NamespaceToken::Uri(other_ns),
            NamespaceToken::Local,
        ]),
        process_contents: ProcessContents::Lax,
        not_namespace: Vec::new(),
        not_qname: Vec::new(),
        id: None,
        annotation: None,
        source: None,
    };

    let wref = wildcard_ref_from_result(&wildcard, &schema_set, Some(target_ns));
    match &wref.namespace_constraint {
        NamespaceConstraint::List(list) => {
            assert_eq!(list.len(), 3);
            assert_eq!(
                list[0],
                Some(target_ns),
                "##targetNamespace should resolve to target_ns"
            );
            assert_eq!(list[1], Some(other_ns));
            assert_eq!(list[2], None, "##local should resolve to None");
        }
        other => panic!("expected List, got {:?}", other),
    }
}

#[test]
fn test_wildcard_ref_from_result_resolves_target_namespace_in_not_namespace() {
    let schema_set = SchemaSet::with_version(XsdVersion::V1_1);
    let target_ns = schema_set.name_table.add("http://target.example.com");

    use crate::parser::frames::NamespaceToken;

    let wildcard = WildcardResult {
        namespace: WildcardNamespace::Any,
        process_contents: ProcessContents::Lax,
        not_namespace: vec![NamespaceToken::TargetNamespace],
        not_qname: Vec::new(),
        id: None,
        annotation: None,
        source: None,
    };

    let wref = wildcard_ref_from_result(&wildcard, &schema_set, Some(target_ns));
    match &wref.namespace_constraint {
        NamespaceConstraint::Not(excluded) => {
            assert_eq!(excluded.len(), 1);
            assert_eq!(
                excluded[0],
                Some(target_ns),
                "##targetNamespace in notNamespace should resolve to target_ns"
            );
        }
        other => panic!("expected Not, got {:?}", other),
    }
}

#[test]
fn test_wildcard_ref_from_result_expands_defined() {
    let mut schema_set = SchemaSet::with_version(XsdVersion::V1_1);
    let ns = schema_set.name_table.add("http://example.com");
    let elem_name = schema_set.name_table.add("foo");

    // Register a globally declared element in the schema
    schema_set
        .namespaces
        .entry(Some(ns))
        .or_default()
        .elements
        .insert(elem_name, Default::default());

    use NotQNameItem;

    let wildcard = WildcardResult {
        namespace: WildcardNamespace::Any,
        process_contents: ProcessContents::Lax,
        not_namespace: Vec::new(),
        not_qname: vec![NotQNameItem::Defined],
        id: None,
        annotation: None,
        source: None,
    };

    let wref = wildcard_ref_from_result(&wildcard, &schema_set, None);
    assert!(
        wref.not_qnames.contains(&(Some(ns), elem_name)),
        "##defined should expand to include globally declared element (ns, foo)"
    );
}

#[test]
fn test_wildcard_ref_from_default_expands_defined() {
    use crate::schema::wildcard::QNameDisallowed;

    let mut schema_set = SchemaSet::with_version(XsdVersion::V1_1);
    let ns = schema_set.name_table.add("http://example.com");
    let elem_name = schema_set.name_table.add("bar");

    // Register a globally declared element
    schema_set
        .namespaces
        .entry(Some(ns))
        .or_default()
        .elements
        .insert(elem_name, Default::default());

    let mut wildcard = ElementWildcard::any_lax();
    wildcard.not_qnames = vec![QNameDisallowed::Defined];

    let wref = wildcard_ref_from_default(&wildcard, &schema_set);
    assert!(
        wref.not_qnames.contains(&(Some(ns), elem_name)),
        "##defined in default open content should expand to include globally declared element"
    );
}

#[test]
fn test_open_content_from_result_e2e_with_not_namespace() {
    // End-to-end: compile a type with explicit open content using notNamespace=##targetNamespace
    let mut schema_set = SchemaSet::with_version(XsdVersion::V1_1);
    let target_ns = schema_set.name_table.add("http://target.example.com");

    let doc_id = schema_set.documents.len() as u32;
    let mut doc = SchemaDocument::new(doc_id, "test.xsd".to_string());
    doc.target_namespace = Some(target_ns);
    schema_set.documents.push(doc);

    use crate::parser::frames::NamespaceToken;

    let oc_result = OpenContentResult {
        mode: OpenContentMode::Interleave,
        wildcard: Some(WildcardResult {
            namespace: WildcardNamespace::Any,
            process_contents: ProcessContents::Lax,
            not_namespace: vec![NamespaceToken::TargetNamespace],
            not_qname: Vec::new(),
            id: None,
            annotation: None,
            source: None,
        }),
        id: None,
        annotation: None,
        source: None,
    };

    let oc = open_content_from_result(&oc_result, &schema_set, Some(target_ns));
    assert!(oc.is_some());
    let oc = oc.unwrap();
    let wildcard = oc.wildcard.unwrap();
    match &wildcard.namespace_constraint {
        NamespaceConstraint::Not(excluded) => {
            assert_eq!(excluded, &vec![Some(target_ns)]);
        }
        other => panic!("expected Not constraint, got {:?}", other),
    }
}

#[test]
fn test_default_open_content_e2e_with_defined() {
    // End-to-end: compile a type using default open content with ##defined notQName
    use crate::schema::wildcard::QNameDisallowed;

    let mut schema_set = SchemaSet::with_version(XsdVersion::V1_1);
    let ns = schema_set.name_table.add("http://example.com");
    let elem_name = schema_set.name_table.add("globalElem");

    // Register a globally declared element
    schema_set
        .namespaces
        .entry(Some(ns))
        .or_default()
        .elements
        .insert(elem_name, Default::default());

    let doc_id = schema_set.documents.len() as u32;
    let mut doc = SchemaDocument::new(doc_id, "test.xsd".to_string());
    let mut wc = ElementWildcard::any_lax();
    wc.not_qnames = vec![QNameDisallowed::Defined];
    doc.default_open_content = Some(DefaultOpenContent {
        source: None,
        applies_to_empty: true,
        mode: SchemaOpenContentMode::Interleave,
        wildcard: Some(wc),
    });
    schema_set.documents.push(doc);

    let source = SourceRef::new(doc_id, SourceSpan::new(0, 0));
    let ct_key = schema_set.arenas.alloc_complex_type(make_complex_type_data(
        Some(source),
        ComplexContentResult::Empty,
    ));
    let type_def = schema_set.arenas.complex_types.get(ct_key).unwrap();

    let matcher = compile_content_model_matcher(&schema_set, type_def).unwrap();
    match matcher {
        ContentModelMatcher::WithOpenContent { wildcard, .. } => {
            let wref = wildcard.expect("wildcard should be present");
            assert!(
                wref.not_qnames.contains(&(Some(ns), elem_name)),
                "##defined should expand to include globally declared element through full compilation path"
            );
        }
        _ => panic!("expected open content wrapper"),
    }
}

#[test]
fn test_collect_sibling_element_qnames_with_ref() {
    // Verify that collect_sibling_element_qnames handles element refs properly
    let schema_set = SchemaSet::with_version(XsdVersion::V1_1);
    let ref_name = schema_set.name_table.add("refElem");
    let ref_ns = schema_set.name_table.add("http://ref.example.com");
    let local_name = schema_set.name_table.add("localElem");

    let ctx = CompileContext::new(&schema_set, None);

    let particles = vec![
        // Element ref
        ParticleResult {
            term: ParticleTerm::Element(ElementFrameResult {
                name: None,
                ref_name: Some(QNameRef {
                    prefix: None,
                    local_name: ref_name,
                    namespace: Some(ref_ns),
                }),
                target_namespace: None,
                type_ref: None,
                inline_type: None,
                substitution_group: vec![],
                default_value: None,
                fixed_value: None,
                nillable: false,
                is_abstract: false,
                min_occurs: 1,
                max_occurs: Some(1),
                block: Default::default(),
                final_derivation: Default::default(),
                form: None,
                id: None,
                alternatives: vec![],
                identity_constraints: vec![],
                identity_constraint_refs: vec![],
                annotation: None,
                source: None,
            }),
            min_occurs: 1,
            max_occurs: Some(1),
            source: None,
        },
        // Local element
        make_element_particle(local_name, 1, Some(1)),
    ];

    let siblings = ctx.collect_sibling_element_qnames(&particles);
    assert_eq!(siblings.len(), 2);
    assert!(
        siblings.contains(&(Some(ref_ns), ref_name)),
        "should include element ref with resolved namespace"
    );
    assert!(
        siblings.contains(&(None, local_name)),
        "should include local element with proper namespace"
    );
}

#[test]
fn test_defined_sibling_expansion_in_sequence() {
    // Verify that ##definedSibling expands to sibling elements in a sequence
    use NotQNameItem;

    let schema_set = SchemaSet::with_version(XsdVersion::V1_1);
    let elem_a = schema_set.name_table.add("a");
    let elem_b = schema_set.name_table.add("b");

    // Build sequence: <a/> <xs:any notQName="##definedSibling"/>
    let wildcard_particle = ParticleResult {
        term: ParticleTerm::Any(WildcardResult {
            namespace: WildcardNamespace::Any,
            process_contents: ProcessContents::Lax,
            not_namespace: Vec::new(),
            not_qname: vec![NotQNameItem::DefinedSibling],
            id: None,
            annotation: None,
            source: None,
        }),
        min_occurs: 0,
        max_occurs: None,
        source: None,
    };

    let sequence = make_sequence_particle(vec![
        make_element_particle(elem_a, 1, Some(1)),
        make_element_particle(elem_b, 1, Some(1)),
        wildcard_particle,
    ]);

    let nfa = compile_particle(&schema_set, &sequence, None).unwrap();

    // The wildcard in the NFA should have not_qnames excluding siblings a and b
    let mut found_wildcard = false;
    for state in &nfa.states {
        if let Some(NfaTerm::Wildcard { not_qnames, .. }) = &state.term {
            found_wildcard = true;
            assert!(
                not_qnames.contains(&(None, elem_a)),
                "##definedSibling should exclude sibling element 'a'"
            );
            assert!(
                not_qnames.contains(&(None, elem_b)),
                "##definedSibling should exclude sibling element 'b'"
            );
        }
    }
    assert!(found_wildcard, "NFA should contain a wildcard state");
}

#[test]
fn test_defined_sibling_open_content_nfa() {
    // ##definedSibling in open content wildcard should expand to sibling elements
    // from the NFA content model when attached via attach_open_content
    use NotQNameItem;

    let schema_set = SchemaSet::with_version(XsdVersion::V1_1);
    let elem_a = schema_set.name_table.add("a");
    let elem_b = schema_set.name_table.add("b");

    // Build a sequence: <a/> <b/>
    let sequence = make_sequence_particle(vec![
        make_element_particle(elem_a, 1, Some(1)),
        make_element_particle(elem_b, 1, Some(1)),
    ]);
    let nfa = compile_particle(&schema_set, &sequence, None).unwrap();
    let matcher = ContentModelMatcher::Nfa(nfa);

    // Build open content with ##definedSibling
    let oc_result = OpenContentResult {
        mode: OpenContentMode::Interleave,
        wildcard: Some(WildcardResult {
            namespace: WildcardNamespace::Any,
            process_contents: ProcessContents::Lax,
            not_namespace: Vec::new(),
            not_qname: vec![NotQNameItem::DefinedSibling],
            id: None,
            annotation: None,
            source: None,
        }),
        id: None,
        annotation: None,
        source: None,
    };
    let oc = open_content_from_result(&oc_result, &schema_set, None).unwrap();

    // has_defined_sibling should be set
    assert!(oc.wildcard.as_ref().unwrap().has_defined_sibling);

    let result = attach_open_content(&schema_set, matcher, Some(oc));
    match result {
        ContentModelMatcher::WithOpenContent { wildcard, .. } => {
            let wref = wildcard.expect("wildcard should be present");
            assert!(
                !wref.has_defined_sibling,
                "has_defined_sibling should be resolved"
            );
            assert!(
                wref.not_qnames.contains(&(None, elem_a)),
                "##definedSibling should exclude sibling element 'a'"
            );
            assert!(
                wref.not_qnames.contains(&(None, elem_b)),
                "##definedSibling should exclude sibling element 'b'"
            );
        }
        _ => panic!("expected WithOpenContent"),
    }
}

#[test]
fn test_defined_sibling_open_content_all_group() {
    // ##definedSibling in open content wildcard should expand to sibling elements
    // from AllGroup content model when attached via attach_open_content
    use NotQNameItem;

    let schema_set = SchemaSet::with_version(XsdVersion::V1_1);
    let elem_x = schema_set.name_table.add("x");
    let elem_y = schema_set.name_table.add("y");

    // Build all-group with elements x and y
    let model = AllGroupModel::new(vec![
        AllParticle::new(
            NfaTerm::element(elem_x, None, None),
            1,
            MaxOccurs::Bounded(1),
            None,
        ),
        AllParticle::new(
            NfaTerm::element(elem_y, None, None),
            1,
            MaxOccurs::Bounded(1),
            None,
        ),
    ]);
    let matcher = ContentModelMatcher::AllGroup(model);

    // Build open content with ##definedSibling
    let oc_result = OpenContentResult {
        mode: OpenContentMode::Suffix,
        wildcard: Some(WildcardResult {
            namespace: WildcardNamespace::Any,
            process_contents: ProcessContents::Lax,
            not_namespace: Vec::new(),
            not_qname: vec![NotQNameItem::DefinedSibling],
            id: None,
            annotation: None,
            source: None,
        }),
        id: None,
        annotation: None,
        source: None,
    };
    let oc = open_content_from_result(&oc_result, &schema_set, None).unwrap();

    let result = attach_open_content(&schema_set, matcher, Some(oc));
    match result {
        ContentModelMatcher::AllGroup(model) => {
            let oc_wc = model.open_content.expect("open content should be present");
            assert!(
                oc_wc.not_qnames.contains(&(None, elem_x)),
                "##definedSibling should exclude sibling element 'x'"
            );
            assert!(
                oc_wc.not_qnames.contains(&(None, elem_y)),
                "##definedSibling should exclude sibling element 'y'"
            );
        }
        _ => panic!("expected AllGroup"),
    }
}

// ========================================================================
// Group refs inside xs:all — cos-all-limited flattening tests
// ========================================================================

/// Helper: register a named model group in the schema and return its key.
/// The group is stored in arenas and registered in namespace lookup.
fn register_model_group(
    schema_set: &mut SchemaSet,
    name: NameId,
    ns: Option<NameId>,
    compositor: Compositor,
    particles: Vec<ParticleResult>,
) -> crate::ids::ModelGroupKey {
    let data = ModelGroupData {
        name: Some(name),
        target_namespace: ns,
        ref_name: None,
        compositor: Some(compositor),
        particles,
        min_occurs: 1,
        max_occurs: Some(1),
        id: None,
        annotation: None,
        source: None,
        resolved_ref: None,
        resolved_particles: vec![],
        resolved_particle_types: vec![],
        resolved_particle_elements: vec![],
        redefine_original: None,
        redefine_requires_restriction_check: false,
    };
    let key = schema_set.arenas.alloc_model_group(data);
    schema_set
        .namespaces
        .entry(ns)
        .or_default()
        .model_groups
        .insert(name, key);
    key
}

/// Helper: make a group reference particle (xs:group ref="...").
fn make_group_ref_particle(
    ref_ns: Option<NameId>,
    ref_local: NameId,
    min: u32,
    max: Option<u32>,
) -> ParticleResult {
    ParticleResult {
        term: ParticleTerm::Group(ModelGroupDefResult {
            name: None,
            ref_name: Some(QNameRef {
                prefix: None,
                local_name: ref_local,
                namespace: ref_ns,
            }),
            compositor: None,
            particles: vec![],
            min_occurs: 1,
            max_occurs: Some(1),
            id: None,
            annotation: None,
            source: None,
        }),
        min_occurs: min,
        max_occurs: max,
        source: None,
    }
}

/// An unresolvable `<xs:group ref="tns:missing"/>` must name the group the way
/// every sibling error site does — the resolved `{namespace}local` QName, never
/// the raw interned ids.
#[test]
fn test_unresolved_group_ref_error_uses_readable_qname() {
    let schema_set = SchemaSet::new();
    let ns = schema_set.name_table.add("http://example.com/tns");
    let missing = schema_set.name_table.add("missing");
    let particle = make_group_ref_particle(Some(ns), missing, 1, Some(1));

    let err = compile_particle(&schema_set, &particle, Some(ns))
        .expect_err("a reference to an undeclared group must not compile");

    let msg = err.to_string();
    assert!(
        msg.contains("{http://example.com/tns}missing"),
        "expected the resolved QName, got {msg:?}"
    );
    assert!(
        !msg.contains("NameId("),
        "error message leaks raw interned ids: {msg:?}"
    );
}

/// Helper: make an inline all-group particle with a group ref inside it,
/// suitable for use as a complex type's top-level particle.
#[cfg(feature = "xsd11")]
fn make_all_with_group_ref(
    direct_particles: Vec<ParticleResult>,
    group_ref_particles: Vec<ParticleResult>,
) -> ParticleResult {
    let mut all_children = direct_particles;
    all_children.extend(group_ref_particles);
    make_all_particle(all_children)
}

/// Helper: build a complex type def with an all-group particle and compile it.
#[cfg(feature = "xsd11")]
fn compile_all_type(
    schema_set: &SchemaSet,
    all_particle: ParticleResult,
) -> NfaCompileResult<ContentModelMatcher> {
    use crate::parser::frames::ComplexContentDefResult;
    let content = ComplexContentResult::Complex(ComplexContentDefResult {
        particle: Some(all_particle),
        derivation: DerivationMethod::Restriction,
        mixed: false,
        mixed_explicit: false,
        base_type: None,
        open_content: None,
        attributes: vec![],
        attribute_groups: vec![],
        attribute_wildcard: None,
        assertions: vec![],
        id: None,
        derivation_id: None,
        source: None,
    });
    let type_def = make_complex_type_with_content(content);
    compile_content_model_matcher(schema_set, &type_def)
}

// --- Valid schemas (XSD 1.1) ---

#[cfg(feature = "xsd11")]
#[test]
fn test_group_ref_to_all_inside_all() {
    // Test 1: G = all(a, b), parent all(group-ref-G, c) → flattened to all(a, b, c)
    let mut schema_set = SchemaSet::xsd11();
    let a = schema_set.name_table.add("a");
    let b = schema_set.name_table.add("b");
    let c = schema_set.name_table.add("c");
    let g_name = schema_set.name_table.add("G");

    register_model_group(
        &mut schema_set,
        g_name,
        None,
        Compositor::All,
        vec![
            make_element_particle(a, 1, Some(1)),
            make_element_particle(b, 1, Some(1)),
        ],
    );

    let all_particle = make_all_with_group_ref(
        vec![make_element_particle(c, 1, Some(1))],
        vec![make_group_ref_particle(None, g_name, 1, Some(1))],
    );

    let matcher = compile_all_type(&schema_set, all_particle).unwrap();
    match &matcher {
        ContentModelMatcher::AllGroup(model) => {
            assert_eq!(
                model.particle_count(),
                3,
                "should flatten to 3 particles: a, b, c"
            );
        }
        other => panic!("expected AllGroup, got {:?}", other),
    }
}

#[cfg(feature = "xsd11")]
#[test]
fn test_nested_group_refs_in_all() {
    // Test 2: G2 = all(b, c), G1 = all(a, group-ref-G2), parent all(group-ref-G1, d)
    // → flattened to all(a, b, c, d)
    let mut schema_set = SchemaSet::xsd11();
    let a = schema_set.name_table.add("a");
    let b = schema_set.name_table.add("b");
    let c = schema_set.name_table.add("c");
    let d = schema_set.name_table.add("d");
    let g1_name = schema_set.name_table.add("G1");
    let g2_name = schema_set.name_table.add("G2");

    register_model_group(
        &mut schema_set,
        g2_name,
        None,
        Compositor::All,
        vec![
            make_element_particle(b, 1, Some(1)),
            make_element_particle(c, 1, Some(1)),
        ],
    );

    register_model_group(
        &mut schema_set,
        g1_name,
        None,
        Compositor::All,
        vec![
            make_element_particle(a, 1, Some(1)),
            make_group_ref_particle(None, g2_name, 1, Some(1)),
        ],
    );

    let all_particle = make_all_with_group_ref(
        vec![make_element_particle(d, 1, Some(1))],
        vec![make_group_ref_particle(None, g1_name, 1, Some(1))],
    );

    let matcher = compile_all_type(&schema_set, all_particle).unwrap();
    match &matcher {
        ContentModelMatcher::AllGroup(model) => {
            assert_eq!(
                model.particle_count(),
                4,
                "should flatten to 4 particles: a, b, c, d"
            );
        }
        other => panic!("expected AllGroup, got {:?}", other),
    }
}

#[cfg(feature = "xsd11")]
#[test]
fn test_group_ref_with_optional_inner_particles() {
    // Test 3: G = all(a[1..1], b[0..1]), parent all(group-ref-G, c)
    let mut schema_set = SchemaSet::xsd11();
    let a = schema_set.name_table.add("a");
    let b = schema_set.name_table.add("b");
    let c = schema_set.name_table.add("c");
    let g_name = schema_set.name_table.add("G");

    register_model_group(
        &mut schema_set,
        g_name,
        None,
        Compositor::All,
        vec![
            make_element_particle(a, 1, Some(1)),
            make_element_particle(b, 0, Some(1)), // optional
        ],
    );

    let all_particle = make_all_with_group_ref(
        vec![make_element_particle(c, 1, Some(1))],
        vec![make_group_ref_particle(None, g_name, 1, Some(1))],
    );

    let matcher = compile_all_type(&schema_set, all_particle).unwrap();
    match &matcher {
        ContentModelMatcher::AllGroup(model) => {
            assert_eq!(model.particle_count(), 3);
            // Check that inner particle b kept its optional nature
            let optional_count = model.particles.iter().filter(|p| p.is_optional()).count();
            assert_eq!(
                optional_count, 1,
                "b should remain optional after flattening"
            );
        }
        other => panic!("expected AllGroup, got {:?}", other),
    }
}

#[cfg(feature = "xsd11")]
#[test]
fn test_group_ref_alongside_wildcard() {
    // Test 4: all(group-ref-G, xs:any)
    let mut schema_set = SchemaSet::xsd11();
    let a = schema_set.name_table.add("a");
    let g_name = schema_set.name_table.add("G");

    register_model_group(
        &mut schema_set,
        g_name,
        None,
        Compositor::All,
        vec![make_element_particle(a, 1, Some(1))],
    );

    let wildcard_particle = ParticleResult {
        term: ParticleTerm::Any(WildcardResult {
            namespace: WildcardNamespace::Any,
            process_contents: ProcessContents::Lax,
            not_namespace: Vec::new(),
            not_qname: Vec::new(),
            id: None,
            annotation: None,
            source: None,
        }),
        min_occurs: 0,
        max_occurs: Some(1),
        source: None,
    };

    let all_particle = make_all_with_group_ref(
        vec![wildcard_particle],
        vec![make_group_ref_particle(None, g_name, 1, Some(1))],
    );

    let matcher = compile_all_type(&schema_set, all_particle).unwrap();
    match &matcher {
        ContentModelMatcher::AllGroup(model) => {
            assert_eq!(model.particle_count(), 2, "wildcard + flattened element a");
            // One should be element, one should be wildcard
            let has_wildcard = model
                .particles
                .iter()
                .any(|p| matches!(p.term, NfaTerm::Wildcard { .. }));
            let has_element = model
                .particles
                .iter()
                .any(|p| matches!(p.term, NfaTerm::Element { .. }));
            assert!(has_wildcard, "should have wildcard particle");
            assert!(has_element, "should have element particle from group ref");
        }
        other => panic!("expected AllGroup, got {:?}", other),
    }
}

#[cfg(feature = "xsd11")]
#[test]
fn test_defined_sibling_includes_group_ref_elements() {
    // Test 7: ##definedSibling wildcard excludes elements from group refs
    use NotQNameItem;

    let mut schema_set = SchemaSet::with_version(XsdVersion::V1_1);
    let a = schema_set.name_table.add("a");
    let b = schema_set.name_table.add("b");
    let g_name = schema_set.name_table.add("G");

    register_model_group(
        &mut schema_set,
        g_name,
        None,
        Compositor::All,
        vec![make_element_particle(b, 1, Some(1))],
    );

    let wildcard_particle = ParticleResult {
        term: ParticleTerm::Any(WildcardResult {
            namespace: WildcardNamespace::Any,
            process_contents: ProcessContents::Lax,
            not_namespace: Vec::new(),
            not_qname: vec![NotQNameItem::DefinedSibling],
            id: None,
            annotation: None,
            source: None,
        }),
        min_occurs: 0,
        max_occurs: Some(1),
        source: None,
    };

    let all_particle = make_all_with_group_ref(
        vec![make_element_particle(a, 1, Some(1)), wildcard_particle],
        vec![make_group_ref_particle(None, g_name, 1, Some(1))],
    );

    let matcher = compile_all_type(&schema_set, all_particle).unwrap();
    match &matcher {
        ContentModelMatcher::AllGroup(model) => {
            // Find the wildcard particle and verify not_qnames
            let wc = model
                .particles
                .iter()
                .find(|p| matches!(p.term, NfaTerm::Wildcard { .. }))
                .expect("should have wildcard particle");
            if let NfaTerm::Wildcard { not_qnames, .. } = &wc.term {
                assert!(
                    not_qnames.contains(&(None, a)),
                    "##definedSibling should exclude element 'a'"
                );
                assert!(
                    not_qnames.contains(&(None, b)),
                    "##definedSibling should exclude element 'b' from group ref"
                );
            }
        }
        other => panic!("expected AllGroup, got {:?}", other),
    }
}

// --- Invalid schemas (compile errors) ---

#[cfg(feature = "xsd11")]
#[test]
fn test_group_ref_in_all_min_occurs_zero_error() {
    // Test 8: Group ref with minOccurs=0 → cos-all-limited.1.3 error
    let mut schema_set = SchemaSet::xsd11();
    let a = schema_set.name_table.add("a");
    let g_name = schema_set.name_table.add("G");

    register_model_group(
        &mut schema_set,
        g_name,
        None,
        Compositor::All,
        vec![make_element_particle(a, 1, Some(1))],
    );

    let all_particle = make_all_particle(vec![
        make_group_ref_particle(None, g_name, 0, Some(1)), // minOccurs=0 — invalid
    ]);

    let result = compile_all_type(&schema_set, all_particle);
    assert!(
        matches!(result, Err(NfaCompileError::InvalidAllGroupOccurs { .. })),
        "minOccurs=0 should produce InvalidAllGroupOccurs error"
    );
}

#[cfg(feature = "xsd11")]
#[test]
fn test_group_ref_in_all_max_occurs_two_error() {
    // Test 9: Group ref with maxOccurs=2 → cos-all-limited.1.3 error
    let mut schema_set = SchemaSet::xsd11();
    let a = schema_set.name_table.add("a");
    let g_name = schema_set.name_table.add("G");

    register_model_group(
        &mut schema_set,
        g_name,
        None,
        Compositor::All,
        vec![make_element_particle(a, 1, Some(1))],
    );

    let all_particle = make_all_particle(vec![
        make_group_ref_particle(None, g_name, 1, Some(2)), // maxOccurs=2 — invalid
    ]);

    let result = compile_all_type(&schema_set, all_particle);
    assert!(
        matches!(result, Err(NfaCompileError::InvalidAllGroupOccurs { .. })),
        "maxOccurs=2 should produce InvalidAllGroupOccurs error"
    );
}

#[cfg(feature = "xsd11")]
#[test]
fn test_group_ref_to_sequence_in_all_error() {
    // Test 10: Group ref to sequence → cos-all-limited.2 error
    let mut schema_set = SchemaSet::xsd11();
    let a = schema_set.name_table.add("a");
    let g_name = schema_set.name_table.add("G");

    register_model_group(
        &mut schema_set,
        g_name,
        None,
        Compositor::Sequence, // not All — invalid
        vec![make_element_particle(a, 1, Some(1))],
    );

    let all_particle = make_all_particle(vec![make_group_ref_particle(None, g_name, 1, Some(1))]);

    let result = compile_all_type(&schema_set, all_particle);
    assert!(
        matches!(result, Err(NfaCompileError::InvalidAllGroupContent { .. })),
        "sequence group ref should produce InvalidAllGroupContent error"
    );
}

#[cfg(feature = "xsd11")]
#[test]
fn test_group_ref_to_choice_in_all_error() {
    // Test 11: Group ref to choice → cos-all-limited.2 error
    let mut schema_set = SchemaSet::xsd11();
    let a = schema_set.name_table.add("a");
    let g_name = schema_set.name_table.add("G");

    register_model_group(
        &mut schema_set,
        g_name,
        None,
        Compositor::Choice, // not All — invalid
        vec![make_element_particle(a, 1, Some(1))],
    );

    let all_particle = make_all_particle(vec![make_group_ref_particle(None, g_name, 1, Some(1))]);

    let result = compile_all_type(&schema_set, all_particle);
    assert!(
        matches!(result, Err(NfaCompileError::InvalidAllGroupContent { .. })),
        "choice group ref should produce InvalidAllGroupContent error"
    );
}

#[test]
fn test_group_ref_in_all_xsd10_error() {
    // Test 12: XSD 1.0 schema set with group ref in all → InvalidAllGroupContent error
    // SchemaSet::new() creates XSD 1.0 — must reject group refs regardless of
    // whether the crate is built with the xsd11 feature.
    let mut schema_set = SchemaSet::new(); // XSD 1.0
    let a = schema_set.name_table.add("a");
    let g_name = schema_set.name_table.add("G");

    register_model_group(
        &mut schema_set,
        g_name,
        None,
        Compositor::All,
        vec![make_element_particle(a, 1, Some(1))],
    );

    // Directly test compile_all_group_model with a group ref particle
    let particles = vec![make_group_ref_particle(None, g_name, 1, Some(1))];
    let mut ctx = CompileContext::new(&schema_set, None);
    ctx.content_flat_idx = Some(0);

    let result = ctx.compile_all_group_model(&particles, None);
    assert!(
        matches!(result, Err(NfaCompileError::InvalidAllGroupContent { .. })),
        "XSD 1.0 schema should reject group refs in xs:all even in xsd11 build"
    );
}

#[cfg(feature = "xsd11")]
#[test]
fn test_inline_group_in_all_error() {
    // Test 13: Inline group (no ref_name) inside xs:all → InvalidAllGroupContent error
    let schema_set = SchemaSet::xsd11();

    // Create an inline group particle (compositor=All but no ref_name)
    let inline_group = ParticleResult {
        term: ParticleTerm::Group(ModelGroupDefResult {
            name: None,
            ref_name: None, // inline, not a reference
            compositor: Some(Compositor::All),
            particles: vec![make_element_particle(NameId(1), 1, Some(1))],
            min_occurs: 1,
            max_occurs: Some(1),
            id: None,
            annotation: None,
            source: None,
        }),
        min_occurs: 1,
        max_occurs: Some(1),
        source: None,
    };

    let particles = vec![inline_group];
    let mut ctx = CompileContext::new(&schema_set, None);
    ctx.content_flat_idx = Some(0);
    let result = ctx.compile_all_group_model(&particles, None);
    assert!(
        matches!(result, Err(NfaCompileError::InvalidAllGroupContent { .. })),
        "inline group (no ref_name) should be rejected"
    );
}

// --- Outer all-group occurrence constraint tests (Ea022-Ea025) ---

#[test]
fn test_all_group_outer_min_occurs_2_rejected() {
    // Ea023 analog: inline all with minOccurs=2 should be rejected in XSD 1.0
    use crate::parser::frames::ComplexContentDefResult;

    let schema_set = SchemaSet::new();
    let all_particle = make_all_particle_with_occurs(
        vec![
            make_element_particle(NameId(1), 1, Some(1)),
            make_element_particle(NameId(2), 1, Some(1)),
        ],
        2, // minOccurs=2
        Some(1),
    );

    let content = ComplexContentResult::Complex(ComplexContentDefResult {
        particle: Some(all_particle),
        derivation: DerivationMethod::Restriction,
        mixed: false,
        mixed_explicit: false,
        base_type: None,
        open_content: None,
        attributes: vec![],
        attribute_groups: vec![],
        attribute_wildcard: None,
        assertions: vec![],
        id: None,
        derivation_id: None,
        source: None,
    });
    let type_def = make_complex_type_with_content(content);
    let result = compile_content_model_matcher(&schema_set, &type_def);
    assert!(
        matches!(result, Err(NfaCompileError::InvalidAllGroupOccurs { .. })),
        "minOccurs=2 on all-group should be rejected: {:?}",
        result
    );
}

#[test]
fn test_all_group_outer_min_gt_max_rejected() {
    // Ea024 analog: minOccurs=1 maxOccurs=0 should be rejected
    use crate::parser::frames::ComplexContentDefResult;

    let schema_set = SchemaSet::new();
    let all_particle = make_all_particle_with_occurs(
        vec![
            make_element_particle(NameId(1), 1, Some(1)),
            make_element_particle(NameId(2), 1, Some(1)),
        ],
        1,       // minOccurs=1
        Some(0), // maxOccurs=0
    );

    let content = ComplexContentResult::Complex(ComplexContentDefResult {
        particle: Some(all_particle),
        derivation: DerivationMethod::Restriction,
        mixed: false,
        mixed_explicit: false,
        base_type: None,
        open_content: None,
        attributes: vec![],
        attribute_groups: vec![],
        attribute_wildcard: None,
        assertions: vec![],
        id: None,
        derivation_id: None,
        source: None,
    });
    let type_def = make_complex_type_with_content(content);
    let result = compile_content_model_matcher(&schema_set, &type_def);
    assert!(
        matches!(result, Err(NfaCompileError::InvalidAllGroupOccurs { .. })),
        "minOccurs=1 maxOccurs=0 on all-group should be rejected: {:?}",
        result
    );
}

#[test]
fn test_all_group_outer_max_occurs_2_rejected() {
    // Ea025 analog: maxOccurs=2 should be rejected in XSD 1.0
    use crate::parser::frames::ComplexContentDefResult;

    let schema_set = SchemaSet::new();
    let all_particle = make_all_particle_with_occurs(
        vec![
            make_element_particle(NameId(1), 1, Some(1)),
            make_element_particle(NameId(2), 1, Some(1)),
        ],
        1,
        Some(2), // maxOccurs=2
    );

    let content = ComplexContentResult::Complex(ComplexContentDefResult {
        particle: Some(all_particle),
        derivation: DerivationMethod::Restriction,
        mixed: false,
        mixed_explicit: false,
        base_type: None,
        open_content: None,
        attributes: vec![],
        attribute_groups: vec![],
        attribute_wildcard: None,
        assertions: vec![],
        id: None,
        derivation_id: None,
        source: None,
    });
    let type_def = make_complex_type_with_content(content);
    let result = compile_content_model_matcher(&schema_set, &type_def);
    assert!(
        matches!(result, Err(NfaCompileError::InvalidAllGroupOccurs { .. })),
        "maxOccurs=2 on all-group should be rejected: {:?}",
        result
    );
}

#[test]
fn test_all_group_outer_optional_accepted() {
    // Ea022 analog: minOccurs=0 should be valid and set outer_optional
    use crate::parser::frames::ComplexContentDefResult;

    let schema_set = SchemaSet::new();
    let all_particle = make_all_particle_with_occurs(
        vec![
            make_element_particle(NameId(1), 1, Some(1)),
            make_element_particle(NameId(2), 1, Some(1)),
        ],
        0, // minOccurs=0
        Some(1),
    );

    let content = ComplexContentResult::Complex(ComplexContentDefResult {
        particle: Some(all_particle),
        derivation: DerivationMethod::Restriction,
        mixed: false,
        mixed_explicit: false,
        base_type: None,
        open_content: None,
        attributes: vec![],
        attribute_groups: vec![],
        attribute_wildcard: None,
        assertions: vec![],
        id: None,
        derivation_id: None,
        source: None,
    });
    let type_def = make_complex_type_with_content(content);
    let matcher = compile_content_model_matcher(&schema_set, &type_def).unwrap();
    match &matcher {
        ContentModelMatcher::AllGroup(model) => {
            assert!(
                model.outer_optional,
                "minOccurs=0 should set outer_optional"
            );
            assert_eq!(model.particle_count(), 2);
        }
        other => panic!("expected AllGroup, got {:?}", other),
    }
}

#[test]
fn test_all_group_default_not_outer_optional() {
    // minOccurs=1 maxOccurs=1 should NOT set outer_optional
    use crate::parser::frames::ComplexContentDefResult;

    let schema_set = SchemaSet::new();
    let all_particle = make_all_particle(vec![
        make_element_particle(NameId(1), 1, Some(1)),
        make_element_particle(NameId(2), 0, Some(1)),
    ]);

    let content = ComplexContentResult::Complex(ComplexContentDefResult {
        particle: Some(all_particle),
        derivation: DerivationMethod::Restriction,
        mixed: false,
        mixed_explicit: false,
        base_type: None,
        open_content: None,
        attributes: vec![],
        attribute_groups: vec![],
        attribute_wildcard: None,
        assertions: vec![],
        id: None,
        derivation_id: None,
        source: None,
    });
    let type_def = make_complex_type_with_content(content);
    let matcher = compile_content_model_matcher(&schema_set, &type_def).unwrap();
    match &matcher {
        ContentModelMatcher::AllGroup(model) => {
            assert!(
                !model.outer_optional,
                "default should not be outer_optional"
            );
        }
        other => panic!("expected AllGroup, got {:?}", other),
    }
}
