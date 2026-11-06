use super::reference::elements_overlap;
use super::*;
use crate::compiler::compile::cap_for_upa;
use crate::compiler::{fragment_to_table, FragmentBuilder, NfaTerm};
use crate::error::SchemaError;
use crate::parser::location::{SourceRef, SourceSpan};
use crate::schema::model::DerivationSet;
use crate::types::complex::ProcessContents;

fn assert_cos_nonambig(error: SchemaError) {
    match error {
        SchemaError::StructuralError { constraint, .. } => {
            assert_eq!(constraint, "cos-nonambig");
        }
        _ => panic!("Expected cos-nonambig structural error"),
    }
}

fn create_test_schema_set() -> SchemaSet {
    SchemaSet::new()
}

fn create_test_schema_set_v11() -> SchemaSet {
    SchemaSet::with_version(XsdVersion::V1_1)
}

pub(super) fn element_data(
    name: NameId,
    target_namespace: Option<NameId>,
) -> crate::arenas::ElementDeclData {
    crate::arenas::ElementDeclData {
        name: Some(name),
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

// ========================================================================
// Element-Element conflict tests
// ========================================================================

#[test]
fn test_element_element_same_name_conflict() {
    let schema_set = create_test_schema_set();
    let name_a = schema_set.name_table.add("a");

    // Build a choice between two elements with the same name
    let builder = FragmentBuilder::new();
    let term1 = NfaTerm::element(name_a, None, None);
    let term2 = NfaTerm::element(name_a, None, None);
    let frag1 = builder.single_term(term1, None);
    let frag2 = builder.single_term(term2, None);
    let choice = frag1.alternate(frag2);
    let nfa = fragment_to_table(choice);

    let result = check_upa(&nfa, &schema_set, None);
    assert!(result.is_err());
    assert_cos_nonambig(result.unwrap_err());
}

#[test]
fn test_element_element_different_namespace_no_conflict() {
    let schema_set = create_test_schema_set();
    let name_a = schema_set.name_table.add("a");
    let ns1 = schema_set.name_table.add("http://ns1.example.com");
    let ns2 = schema_set.name_table.add("http://ns2.example.com");

    // Build a choice between two elements with same name but different namespaces
    let builder = FragmentBuilder::new();
    let term1 = NfaTerm::element(name_a, Some(ns1), None);
    let term2 = NfaTerm::element(name_a, Some(ns2), None);
    let frag1 = builder.single_term(term1, None);
    let frag2 = builder.single_term(term2, None);
    let choice = frag1.alternate(frag2);
    let nfa = fragment_to_table(choice);

    let result = check_upa(&nfa, &schema_set, None);
    assert!(result.is_ok());
}

#[test]
fn test_element_element_sequence_no_conflict() {
    let schema_set = create_test_schema_set();
    let name_a = schema_set.name_table.add("a");

    // Build a sequence of two elements with the same name (no conflict in sequence)
    let builder = FragmentBuilder::new();
    let term1 = NfaTerm::element(name_a, None, None);
    let term2 = NfaTerm::element(name_a, None, None);
    let frag1 = builder.single_term(term1, None);
    let frag2 = builder.single_term(term2, None);
    let sequence = frag1.concat(frag2);
    let nfa = fragment_to_table(sequence);

    let result = check_upa(&nfa, &schema_set, None);
    assert!(result.is_ok());
}

#[test]
fn test_element_element_different_names_no_conflict() {
    let schema_set = create_test_schema_set();
    let name_a = schema_set.name_table.add("a");
    let name_b = schema_set.name_table.add("b");

    // Build a choice between two elements with different names
    let builder = FragmentBuilder::new();
    let term1 = NfaTerm::element(name_a, None, None);
    let term2 = NfaTerm::element(name_b, None, None);
    let frag1 = builder.single_term(term1, None);
    let frag2 = builder.single_term(term2, None);
    let choice = frag1.alternate(frag2);
    let nfa = fragment_to_table(choice);

    let result = check_upa(&nfa, &schema_set, None);
    assert!(result.is_ok());
}

// ========================================================================
// Element-Wildcard conflict tests
// ========================================================================

#[test]
fn test_element_wildcard_any_conflict_xsd10() {
    let schema_set = create_test_schema_set();
    let name_a = schema_set.name_table.add("a");

    // Build a choice between an element and a ##any wildcard
    let builder = FragmentBuilder::new();
    let term1 = NfaTerm::element(name_a, None, None);
    let term2 = NfaTerm::wildcard(NamespaceConstraint::Any, ProcessContents::Lax);
    let frag1 = builder.single_term(term1, None);
    let frag2 = builder.single_term(term2, None);
    let choice = frag1.alternate(frag2);
    let nfa = fragment_to_table(choice);

    let result = check_upa(&nfa, &schema_set, None);
    assert!(result.is_err());
    assert_cos_nonambig(result.unwrap_err());
}

#[test]
fn test_element_wildcard_any_allowed_xsd11() {
    let schema_set = create_test_schema_set_v11();
    let name_a = schema_set.name_table.add("a");

    // Build a choice between an element and a ##any wildcard
    let builder = FragmentBuilder::new();
    let term1 = NfaTerm::element(name_a, None, None);
    let term2 = NfaTerm::wildcard(NamespaceConstraint::Any, ProcessContents::Lax);
    let frag1 = builder.single_term(term1, None);
    let frag2 = builder.single_term(term2, None);
    let choice = frag1.alternate(frag2);
    let nfa = fragment_to_table(choice);

    // XSD 1.1 allows element-wildcard conflicts (element takes priority)
    let result = check_upa(&nfa, &schema_set, None);
    assert!(result.is_ok());
}

#[test]
fn test_element_wildcard_other_with_target_no_overlap() {
    let schema_set = create_test_schema_set();
    let name_a = schema_set.name_table.add("a");
    let target_ns = schema_set.name_table.add("http://target.example.com");

    // Build a choice between an element in target ns and ##other wildcard
    let builder = FragmentBuilder::new();
    let term1 = NfaTerm::element(name_a, Some(target_ns), None);
    let term2 = NfaTerm::wildcard(NamespaceConstraint::Other, ProcessContents::Lax);
    let frag1 = builder.single_term(term1, None);
    let frag2 = builder.single_term(term2, None);
    let choice = frag1.alternate(frag2);
    let nfa = fragment_to_table(choice);

    // ##other excludes target namespace, so no overlap
    let result = check_upa(&nfa, &schema_set, Some(target_ns));
    assert!(result.is_ok());
}

// ========================================================================
// Wildcard-Wildcard conflict tests
// ========================================================================

#[test]
fn test_wildcard_wildcard_any_any_conflict() {
    let schema_set = create_test_schema_set();

    // Build a choice between two ##any wildcards
    let builder = FragmentBuilder::new();
    let term1 = NfaTerm::wildcard(NamespaceConstraint::Any, ProcessContents::Lax);
    let term2 = NfaTerm::wildcard(NamespaceConstraint::Any, ProcessContents::Strict);
    let frag1 = builder.single_term(term1, None);
    let frag2 = builder.single_term(term2, None);
    let choice = frag1.alternate(frag2);
    let nfa = fragment_to_table(choice);

    let result = check_upa(&nfa, &schema_set, None);
    assert!(result.is_err());
    assert_cos_nonambig(result.unwrap_err());
}

#[test]
fn test_wildcard_wildcard_target_vs_other_no_conflict() {
    let schema_set = create_test_schema_set();
    let target_ns = schema_set.name_table.add("http://target.example.com");

    // Build a choice between ##targetNamespace and ##other wildcards
    let builder = FragmentBuilder::new();
    let term1 = NfaTerm::wildcard(NamespaceConstraint::TargetNamespace, ProcessContents::Lax);
    let term2 = NfaTerm::wildcard(NamespaceConstraint::Other, ProcessContents::Lax);
    let frag1 = builder.single_term(term1, None);
    let frag2 = builder.single_term(term2, None);
    let choice = frag1.alternate(frag2);
    let nfa = fragment_to_table(choice);

    // ##targetNamespace and ##other are mutually exclusive
    let result = check_upa(&nfa, &schema_set, Some(target_ns));
    assert!(result.is_ok());
}

#[test]
fn test_wildcard_wildcard_list_overlap() {
    let schema_set = create_test_schema_set();
    let ns1 = schema_set.name_table.add("http://ns1.example.com");
    let ns2 = schema_set.name_table.add("http://ns2.example.com");

    // Build a choice between two list wildcards with overlapping namespaces
    let builder = FragmentBuilder::new();
    let term1 = NfaTerm::wildcard(
        NamespaceConstraint::List(vec![Some(ns1), Some(ns2)]),
        ProcessContents::Lax,
    );
    let term2 = NfaTerm::wildcard(
        NamespaceConstraint::List(vec![Some(ns2)]),
        ProcessContents::Lax,
    );
    let frag1 = builder.single_term(term1, None);
    let frag2 = builder.single_term(term2, None);
    let choice = frag1.alternate(frag2);
    let nfa = fragment_to_table(choice);

    let result = check_upa(&nfa, &schema_set, None);
    assert!(result.is_err());
    assert_cos_nonambig(result.unwrap_err());
}

#[test]
fn test_wildcard_wildcard_list_no_overlap() {
    let schema_set = create_test_schema_set();
    let ns1 = schema_set.name_table.add("http://ns1.example.com");
    let ns2 = schema_set.name_table.add("http://ns2.example.com");

    // Build a choice between two list wildcards with non-overlapping namespaces
    let builder = FragmentBuilder::new();
    let term1 = NfaTerm::wildcard(
        NamespaceConstraint::List(vec![Some(ns1)]),
        ProcessContents::Lax,
    );
    let term2 = NfaTerm::wildcard(
        NamespaceConstraint::List(vec![Some(ns2)]),
        ProcessContents::Lax,
    );
    let frag1 = builder.single_term(term1, None);
    let frag2 = builder.single_term(term2, None);
    let choice = frag1.alternate(frag2);
    let nfa = fragment_to_table(choice);

    let result = check_upa(&nfa, &schema_set, None);
    assert!(result.is_ok());
}

// ========================================================================
// Complex pattern tests
// ========================================================================

#[test]
fn test_nested_choice_with_conflict() {
    let schema_set = create_test_schema_set();
    let name_a = schema_set.name_table.add("a");
    let name_b = schema_set.name_table.add("b");

    // Build nested choice: (a | (a | b))
    let builder = FragmentBuilder::new();
    let term_a1 = NfaTerm::element(name_a, None, None);
    let term_a2 = NfaTerm::element(name_a, None, None);
    let term_b = NfaTerm::element(name_b, None, None);

    let frag_a1 = builder.single_term(term_a1, None);
    let frag_a2 = builder.single_term(term_a2, None);
    let frag_b = builder.single_term(term_b, None);

    let inner_choice = frag_a2.alternate(frag_b);
    let outer_choice = frag_a1.alternate(inner_choice);
    let nfa = fragment_to_table(outer_choice);

    let result = check_upa(&nfa, &schema_set, None);
    assert!(result.is_err());
}

#[test]
fn test_optional_element_before_same_element() {
    let schema_set = create_test_schema_set();
    let name_a = schema_set.name_table.add("a");

    // Build: a? a (optional a followed by required a)
    // This creates ambiguity: given input "a", could be (empty, a) or (a, ???)
    let builder = FragmentBuilder::new();
    let term1 = NfaTerm::element(name_a, None, None);
    let term2 = NfaTerm::element(name_a, None, None);

    let frag1 = builder.single_term(term1, None);
    let optional_a = frag1.optional();
    let frag2 = builder.single_term(term2, None);
    let sequence = optional_a.concat(frag2);
    let nfa = fragment_to_table(sequence);

    // This should detect a conflict at the initial state
    let result = check_upa(&nfa, &schema_set, None);
    assert!(result.is_err());
}

// ========================================================================
// Helper function tests
// ========================================================================

#[test]
fn test_elements_overlap_same() {
    let name = NameId(1);
    let ns = Some(NameId(2));
    assert!(elements_overlap(name, ns, name, ns));
}

#[test]
fn test_elements_overlap_different_name() {
    assert!(!elements_overlap(NameId(1), None, NameId(2), None));
}

#[test]
fn test_elements_overlap_different_ns() {
    let name = NameId(1);
    assert!(!elements_overlap(
        name,
        Some(NameId(2)),
        name,
        Some(NameId(3))
    ));
}

#[test]
fn test_element_wildcard_any() {
    assert!(element_wildcard_overlap(
        None,
        &NamespaceConstraint::Any,
        None,
    ));
    assert!(element_wildcard_overlap(
        Some(NameId(1)),
        &NamespaceConstraint::Any,
        None,
    ));
}

#[test]
fn test_substitution_group_head_member_conflict() {
    let mut schema_set = create_test_schema_set();
    let head_name = schema_set.name_table.add("head");
    let member_name = schema_set.name_table.add("member");

    let head_key = schema_set
        .arenas
        .alloc_element(element_data(head_name, None));
    let member_key = schema_set
        .arenas
        .alloc_element(element_data(member_name, None));
    schema_set
        .arenas
        .entries_mut()
        .elements
        .get_mut(member_key)
        .unwrap()
        .resolved_substitution_groups
        .push(head_key);

    let builder = FragmentBuilder::new();
    let term1 = NfaTerm::element(head_name, None, Some(head_key));
    let term2 = NfaTerm::element(member_name, None, Some(member_key));
    let frag1 = builder.single_term(term1, None);
    let frag2 = builder.single_term(term2, None);
    let choice = frag1.alternate(frag2);
    let nfa = fragment_to_table(choice);

    let result = check_upa(&nfa, &schema_set, None);
    assert!(result.is_err());
    assert_cos_nonambig(result.unwrap_err());
}

#[test]
fn test_substitution_group_transitive_conflict() {
    let mut schema_set = create_test_schema_set();
    let head_name = schema_set.name_table.add("head");
    let mid_name = schema_set.name_table.add("mid");
    let leaf_name = schema_set.name_table.add("leaf");

    let head_key = schema_set
        .arenas
        .alloc_element(element_data(head_name, None));
    let mid_key = schema_set
        .arenas
        .alloc_element(element_data(mid_name, None));
    let leaf_key = schema_set
        .arenas
        .alloc_element(element_data(leaf_name, None));

    schema_set
        .arenas
        .entries_mut()
        .elements
        .get_mut(mid_key)
        .unwrap()
        .resolved_substitution_groups
        .push(head_key);
    schema_set
        .arenas
        .entries_mut()
        .elements
        .get_mut(leaf_key)
        .unwrap()
        .resolved_substitution_groups
        .push(mid_key);

    let builder = FragmentBuilder::new();
    let term1 = NfaTerm::element(head_name, None, Some(head_key));
    let term2 = NfaTerm::element(leaf_name, None, Some(leaf_key));
    let frag1 = builder.single_term(term1, None);
    let frag2 = builder.single_term(term2, None);
    let choice = frag1.alternate(frag2);
    let nfa = fragment_to_table(choice);

    let result = check_upa(&nfa, &schema_set, None);
    assert!(result.is_err());
    assert_cos_nonambig(result.unwrap_err());
}

#[test]
fn test_substitution_group_blocked_no_conflict() {
    let mut schema_set = create_test_schema_set();
    let head_name = schema_set.name_table.add("head");
    let member_name = schema_set.name_table.add("member");

    let head_key = schema_set
        .arenas
        .alloc_element(element_data(head_name, None));
    let member_key = schema_set
        .arenas
        .alloc_element(element_data(member_name, None));
    schema_set
        .arenas
        .entries_mut()
        .elements
        .get_mut(member_key)
        .unwrap()
        .resolved_substitution_groups
        .push(head_key);
    schema_set
        .arenas
        .entries_mut()
        .elements
        .get_mut(head_key)
        .unwrap()
        .block = DerivationSet::SUBSTITUTION;

    let builder = FragmentBuilder::new();
    let term1 = NfaTerm::element(head_name, None, Some(head_key));
    let term2 = NfaTerm::element(member_name, None, Some(member_key));
    let frag1 = builder.single_term(term1, None);
    let frag2 = builder.single_term(term2, None);
    let choice = frag1.alternate(frag2);
    let nfa = fragment_to_table(choice);

    let result = check_upa(&nfa, &schema_set, None);
    assert!(result.is_ok());
}

#[test]
fn test_element_wildcard_other() {
    let target_ns = Some(NameId(1));
    let other_ns = Some(NameId(2));

    // Element in target namespace does NOT match ##other
    assert!(!element_wildcard_overlap(
        target_ns,
        &NamespaceConstraint::Other,
        target_ns,
    ));

    // Element in other namespace matches ##other
    assert!(element_wildcard_overlap(
        other_ns,
        &NamespaceConstraint::Other,
        target_ns,
    ));
}

#[test]
fn test_element_wildcard_other_absent_namespace_xsd10() {
    let target_ns = Some(NameId(1));

    assert!(!element_wildcard_overlap(
        None,
        &NamespaceConstraint::Other,
        target_ns,
    ));
}

#[test]
fn test_element_wildcard_other_absent_namespace_xsd11() {
    let target_ns = Some(NameId(1));

    // §3.10.4: when the schema has a target namespace, ##other excludes
    // BOTH the target namespace AND the absent namespace, in both XSD 1.0
    // and 1.1.
    assert!(!element_wildcard_overlap(
        None,
        &NamespaceConstraint::Other,
        target_ns,
    ));
}

#[test]
fn test_element_wildcard_target_namespace() {
    let target_ns = Some(NameId(1));

    // Element in target namespace matches ##targetNamespace
    assert!(element_wildcard_overlap(
        target_ns,
        &NamespaceConstraint::TargetNamespace,
        target_ns,
    ));

    // Element with no namespace does NOT match ##targetNamespace (if target is set)
    assert!(!element_wildcard_overlap(
        None,
        &NamespaceConstraint::TargetNamespace,
        target_ns,
    ));
}

#[test]
fn test_element_wildcard_local() {
    // Element with no namespace matches ##local
    assert!(element_wildcard_overlap(
        None,
        &NamespaceConstraint::Local,
        Some(NameId(1)),
    ));

    // Element with namespace does NOT match ##local
    assert!(!element_wildcard_overlap(
        Some(NameId(2)),
        &NamespaceConstraint::Local,
        Some(NameId(1)),
    ));
}

#[test]
fn test_wildcards_overlap_any() {
    let target = Some(NameId(1));
    assert!(wildcards_overlap(
        &NamespaceConstraint::Any,
        &NamespaceConstraint::Any,
        target,
    ));
    assert!(wildcards_overlap(
        &NamespaceConstraint::Any,
        &NamespaceConstraint::Other,
        target,
    ));
}

#[test]
fn test_wildcards_overlap_target_other() {
    let target = Some(NameId(1));
    // ##targetNamespace and ##other are mutually exclusive
    assert!(!wildcards_overlap(
        &NamespaceConstraint::TargetNamespace,
        &NamespaceConstraint::Other,
        target,
    ));
}

#[test]
fn test_wildcards_overlap_other_local_xsd10() {
    let target = Some(NameId(1));
    assert!(!wildcards_overlap(
        &NamespaceConstraint::Other,
        &NamespaceConstraint::Local,
        target,
    ));
}

#[test]
fn test_wildcards_overlap_other_local_xsd11() {
    let target = Some(NameId(1));
    // ##other never matches absent namespace when the schema has a target
    // namespace (§3.10.4), so it cannot overlap with ##local. Same for
    // both XSD 1.0 and 1.1.
    assert!(!wildcards_overlap(
        &NamespaceConstraint::Other,
        &NamespaceConstraint::Local,
        target,
    ));
}

// ========================================================================
// Not constraint tests
// ========================================================================

#[test]
fn test_element_wildcard_not_excluded() {
    let ns1 = Some(NameId(1));
    // Element in excluded namespace does NOT match Not([ns1])
    assert!(!element_wildcard_overlap(
        ns1,
        &NamespaceConstraint::Not(vec![ns1]),
        None,
    ));
}

#[test]
fn test_element_wildcard_not_allowed() {
    let ns1 = Some(NameId(1));
    let ns2 = Some(NameId(2));
    // Element in non-excluded namespace matches Not([ns1])
    assert!(element_wildcard_overlap(
        ns2,
        &NamespaceConstraint::Not(vec![ns1]),
        None,
    ));
}

#[test]
fn test_element_wildcard_not_absent_excluded() {
    // Not([None]) excludes absent namespace
    assert!(!element_wildcard_overlap(
        None,
        &NamespaceConstraint::Not(vec![None]),
        None,
    ));
}

#[test]
fn test_wildcards_overlap_not_not() {
    let ns1 = Some(NameId(1));
    let ns2 = Some(NameId(2));
    // Not(ns1) vs Not(ns2): both exclude finite sets, infinite remainder overlaps
    assert!(wildcards_overlap(
        &NamespaceConstraint::Not(vec![ns1]),
        &NamespaceConstraint::Not(vec![ns2]),
        None,
    ));
}

#[test]
fn test_wildcards_overlap_not_any() {
    let ns1 = Some(NameId(1));
    assert!(wildcards_overlap(
        &NamespaceConstraint::Not(vec![ns1]),
        &NamespaceConstraint::Any,
        None,
    ));
}

#[test]
fn test_wildcards_overlap_not_target_excluded() {
    let target = Some(NameId(1));
    // Not([target]) vs TargetNamespace: no overlap (target is excluded)
    assert!(!wildcards_overlap(
        &NamespaceConstraint::Not(vec![target]),
        &NamespaceConstraint::TargetNamespace,
        target,
    ));
}

#[test]
fn test_wildcards_overlap_not_target_not_excluded() {
    let target = Some(NameId(1));
    let ns2 = Some(NameId(2));
    // Not([ns2]) vs TargetNamespace: overlaps (target is not excluded)
    assert!(wildcards_overlap(
        &NamespaceConstraint::Not(vec![ns2]),
        &NamespaceConstraint::TargetNamespace,
        target,
    ));
}

#[test]
fn test_wildcards_overlap_not_list_partial() {
    let ns1 = Some(NameId(1));
    let ns2 = Some(NameId(2));
    // Not([ns1]) vs List([ns1, ns2]): overlaps because ns2 is not excluded
    assert!(wildcards_overlap(
        &NamespaceConstraint::Not(vec![ns1]),
        &NamespaceConstraint::List(vec![ns1, ns2]),
        None,
    ));
}

#[test]
fn test_wildcards_overlap_not_list_all_excluded() {
    let ns1 = Some(NameId(1));
    // Not([ns1]) vs List([ns1]): no overlap (all list items excluded)
    assert!(!wildcards_overlap(
        &NamespaceConstraint::Not(vec![ns1]),
        &NamespaceConstraint::List(vec![ns1]),
        None,
    ));
}

// ========================================================================
// notQName UPA tests
// ========================================================================

#[test]
fn test_element_wildcard_no_overlap_via_not_qnames() {
    // When a wildcard's notQName excludes a specific element,
    // they should not be considered overlapping
    let schema_set = create_test_schema_set();
    let name_a = schema_set.name_table.add("a");

    let builder = FragmentBuilder::new();
    let term1 = NfaTerm::element(name_a, None, None);
    // Wildcard that explicitly excludes element "a" (absent ns)
    let term2 = NfaTerm::wildcard_with_not_qnames(
        NamespaceConstraint::Any,
        ProcessContents::Lax,
        vec![(None, name_a)],
    );
    let frag1 = builder.single_term(term1, None);
    let frag2 = builder.single_term(term2, None);
    let choice = frag1.alternate(frag2);
    let nfa = fragment_to_table(choice);

    // XSD 1.0: normally element vs ##any wildcard would conflict,
    // but notQName excludes this element, so no overlap
    let result = check_upa(&nfa, &schema_set, None);
    assert!(result.is_ok());
}

#[test]
fn test_element_wildcard_subst_member_namespace_conflict() {
    // Head element is in target namespace (no overlap with ##other wildcard),
    // but a substitution group member is in another namespace (overlaps).
    // UPA should detect this conflict.
    let mut schema_set = create_test_schema_set();
    let target_ns = schema_set.name_table.add("http://target.example.com");
    let other_ns = schema_set.name_table.add("http://other.example.com");
    let head_name = schema_set.name_table.add("head");
    let member_name = schema_set.name_table.add("member");

    let head_key = schema_set
        .arenas
        .alloc_element(element_data(head_name, Some(target_ns)));
    let member_key = schema_set
        .arenas
        .alloc_element(element_data(member_name, Some(other_ns)));
    schema_set
        .arenas
        .entries_mut()
        .elements
        .get_mut(member_key)
        .unwrap()
        .resolved_substitution_groups
        .push(head_key);

    let builder = FragmentBuilder::new();
    // Element in target namespace with substitution group
    let term1 = NfaTerm::element(head_name, Some(target_ns), Some(head_key));
    // ##other wildcard — matches other_ns but not target_ns
    let term2 = NfaTerm::wildcard(NamespaceConstraint::Other, ProcessContents::Lax);
    let frag1 = builder.single_term(term1, None);
    let frag2 = builder.single_term(term2, None);
    let choice = frag1.alternate(frag2);
    let nfa = fragment_to_table(choice);

    // XSD 1.0: head element is in target_ns (no overlap with ##other),
    // BUT member is in other_ns (DOES overlap with ##other).
    // UPA should detect conflict.
    let result = check_upa(&nfa, &schema_set, Some(target_ns));
    assert!(
        result.is_err(),
        "should detect conflict via substitution member in other namespace"
    );
    assert_cos_nonambig(result.unwrap_err());
}

#[test]
fn test_element_wildcard_subst_member_excluded_by_not_qnames() {
    // Head element is excluded by notQName, but substitution member is NOT excluded.
    // UPA should still detect conflict.
    let mut schema_set = create_test_schema_set();
    let head_name = schema_set.name_table.add("head");
    let member_name = schema_set.name_table.add("member");

    let head_key = schema_set
        .arenas
        .alloc_element(element_data(head_name, None));
    let member_key = schema_set
        .arenas
        .alloc_element(element_data(member_name, None));
    schema_set
        .arenas
        .entries_mut()
        .elements
        .get_mut(member_key)
        .unwrap()
        .resolved_substitution_groups
        .push(head_key);

    let builder = FragmentBuilder::new();
    let term1 = NfaTerm::element(head_name, None, Some(head_key));
    // Wildcard excludes head but NOT member
    let term2 = NfaTerm::wildcard_with_not_qnames(
        NamespaceConstraint::Any,
        ProcessContents::Lax,
        vec![(None, head_name)], // only head excluded
    );
    let frag1 = builder.single_term(term1, None);
    let frag2 = builder.single_term(term2, None);
    let choice = frag1.alternate(frag2);
    let nfa = fragment_to_table(choice);

    // XSD 1.0: head is excluded by notQName, but member is not.
    // The wildcard can still match "member", so UPA conflict exists.
    let result = check_upa(&nfa, &schema_set, None);
    assert!(
        result.is_err(),
        "should detect conflict: member not excluded by notQName"
    );
    assert_cos_nonambig(result.unwrap_err());
}

// ========================================================================
// Element-element conflicts through the name index (0.2.3) — corner cases.
// `upa_differential_tests.rs` compares the index with the 0.2.2 pairwise
// scan on generated inputs; these pin the cases one by one.
// ========================================================================

fn global_element(
    schema_set: &mut SchemaSet,
    name: NameId,
    namespace: Option<NameId>,
    is_abstract: bool,
    heads: &[crate::ids::ElementKey],
) -> crate::ids::ElementKey {
    let mut data = element_data(name, namespace);
    data.is_abstract = is_abstract;
    data.resolved_substitution_groups = heads.to_vec();
    schema_set.arenas.alloc_element(data)
}

fn span(n: usize) -> Option<SourceRef> {
    Some(SourceRef::new(0, SourceSpan::new(n, n + 1)))
}

/// `<xs:choice>` of the given particles, one origin each.
fn choice_of(particles: Vec<(NfaTerm, Option<SourceRef>)>) -> NfaTable {
    let builder = FragmentBuilder::new();
    let mut particles = particles.into_iter();
    let (term, origin) = particles.next().expect("at least one particle");
    let mut choice = builder.single_term(term, origin);
    for (term, origin) in particles {
        choice = choice.alternate(builder.single_term(term, origin));
    }
    fragment_to_table(choice)
}

fn reachable_terms(particles: Vec<(NfaTerm, Option<SourceRef>)>) -> Vec<ReachableTerm> {
    particles
        .into_iter()
        .map(|(term, origin)| ReachableTerm { term, origin })
        .collect()
}

/// The span starts of the two particles an element-element error names
/// (`None` for a particle without an origin), after checking that the 0.2.2
/// pairwise scan reports the identical error.
fn element_conflict_locations(
    elements: &[ReachableTerm],
    schema_set: &SchemaSet,
) -> Option<(Option<usize>, Option<usize>)> {
    let map = upa_substitution_map(schema_set);
    let new = check_element_element_conflicts(elements, 7, schema_set, &map);
    let old = super::reference::check_element_element_conflicts(elements, 7, schema_set, &map);
    assert_eq!(format!("{new:?}"), format!("{old:?}"));
    match new.err().map(|error| *error) {
        None => None,
        Some(UpaError::ElementElementConflict {
            first_location,
            second_location,
            ..
        }) => Some((
            first_location.map(|s| s.span.start),
            second_location.map(|s| s.span.start),
        )),
        Some(other) => panic!("expected an element-element conflict, got {other:?}"),
    }
}

/// A substitution-group member competes by its expanded name: the same local
/// name in another namespace does not compete with it (§3.9.4.1.2 "matches":
/// same expanded name, or substitutable).
#[test]
fn test_substitution_member_same_local_name_other_namespace() {
    for version in [XsdVersion::V1_0, XsdVersion::V1_1] {
        let mut schema_set = SchemaSet::with_version(version);
        let ns1 = Some(schema_set.name_table.add("urn:one"));
        let ns2 = Some(schema_set.name_table.add("urn:two"));
        let head = schema_set.name_table.add("head");
        let m = schema_set.name_table.add("m");
        let head_key = global_element(&mut schema_set, head, ns1, false, &[]);
        global_element(&mut schema_set, m, ns2, false, &[head_key]);

        let head_ref = NfaTerm::element(head, ns1, Some(head_key));
        let other_ns = choice_of(vec![
            (head_ref.clone(), span(1)),
            (NfaTerm::element(m, ns1, None), span(2)),
        ]);
        assert!(
            check_upa(&other_ns, &schema_set, None).is_ok(),
            "{version:?}"
        );

        let member_ns = choice_of(vec![
            (head_ref, span(1)),
            (NfaTerm::element(m, ns2, None), span(2)),
        ]);
        assert_cos_nonambig(check_upa(&member_ns, &schema_set, None).unwrap_err());
    }
}

/// Two heads sharing a member (XSD 1.1 `substitutionGroup` lists) compete
/// although their own names differ.
#[test]
fn test_two_heads_sharing_a_member_compete() {
    let mut schema_set = create_test_schema_set_v11();
    let h1 = schema_set.name_table.add("h1");
    let h2 = schema_set.name_table.add("h2");
    let m = schema_set.name_table.add("m");
    let h1_key = global_element(&mut schema_set, h1, None, false, &[]);
    let h2_key = global_element(&mut schema_set, h2, None, false, &[]);
    global_element(&mut schema_set, m, None, false, &[h1_key, h2_key]);

    let nfa = choice_of(vec![
        (NfaTerm::element(h1, None, Some(h1_key)), span(1)),
        (NfaTerm::element(h2, None, Some(h2_key)), span(2)),
    ]);
    assert_cos_nonambig(check_upa(&nfa, &schema_set, None).unwrap_err());
}

/// Under XSD 1.0 an abstract head's substitutable set omits its own name and,
/// with no members, is empty; two references to it still compete by name.
#[test]
fn test_abstract_head_without_members_competes_by_name() {
    for version in [XsdVersion::V1_0, XsdVersion::V1_1] {
        let mut schema_set = SchemaSet::with_version(version);
        let a = schema_set.name_table.add("a");
        let a_key = global_element(&mut schema_set, a, None, true, &[]);

        let map = upa_substitution_map(&schema_set);
        if version == XsdVersion::V1_0 {
            assert!(
                map[&a_key].is_empty(),
                "abstract head, no members: empty set"
            );
        }

        let nfa = choice_of(vec![
            (NfaTerm::element(a, None, Some(a_key)), span(1)),
            (NfaTerm::element(a, None, Some(a_key)), span(2)),
        ]);
        assert_cos_nonambig(check_upa(&nfa, &schema_set, None).unwrap_err());
    }
}

/// An abstract head competes with its members in both versions.
#[test]
fn test_abstract_head_competes_with_its_member() {
    for version in [XsdVersion::V1_0, XsdVersion::V1_1] {
        let mut schema_set = SchemaSet::with_version(version);
        let a = schema_set.name_table.add("a");
        let m = schema_set.name_table.add("m");
        let a_key = global_element(&mut schema_set, a, None, true, &[]);
        let m_key = global_element(&mut schema_set, m, None, false, &[a_key]);

        let nfa = choice_of(vec![
            (NfaTerm::element(a, None, Some(a_key)), span(1)),
            (NfaTerm::element(m, None, Some(m_key)), span(2)),
        ]);
        assert_cos_nonambig(check_upa(&nfa, &schema_set, None).unwrap_err());
    }
}

/// An abstract member takes part in its head's set for UPA only under XSD 1.1
/// (W3C Bugzilla 4337), so a head and its abstract member compete only there.
#[test]
fn test_abstract_member_competes_only_under_xsd11() {
    for version in [XsdVersion::V1_0, XsdVersion::V1_1] {
        let mut schema_set = SchemaSet::with_version(version);
        let h = schema_set.name_table.add("h");
        let x = schema_set.name_table.add("x");
        let h_key = global_element(&mut schema_set, h, None, false, &[]);
        let x_key = global_element(&mut schema_set, x, None, true, &[h_key]);

        let nfa = choice_of(vec![
            (NfaTerm::element(h, None, Some(h_key)), span(1)),
            (NfaTerm::element(x, None, Some(x_key)), span(2)),
        ]);
        let result = check_upa(&nfa, &schema_set, None);
        match version {
            XsdVersion::V1_0 => assert!(result.is_ok()),
            XsdVersion::V1_1 => assert_cos_nonambig(result.unwrap_err()),
        }
    }
}

/// The UPA map is the plain map under XSD 1.0 and the one with abstract
/// members under XSD 1.1; the pipeline builds it once per pass.
#[test]
fn test_upa_substitution_map_follows_version() {
    for version in [XsdVersion::V1_0, XsdVersion::V1_1] {
        let mut schema_set = SchemaSet::with_version(version);
        let h = schema_set.name_table.add("h");
        let x = schema_set.name_table.add("x");
        let h_key = global_element(&mut schema_set, h, None, false, &[]);
        global_element(&mut schema_set, x, None, true, &[h_key]);

        let plain = build_substitution_group_map(&schema_set);
        let with_abstract = build_substitution_group_map_with_abstract(&schema_set);
        assert_ne!(plain, with_abstract);
        let expected = match version {
            XsdVersion::V1_0 => plain,
            XsdVersion::V1_1 => with_abstract,
        };
        assert_eq!(upa_substitution_map(&schema_set), expected);
    }
}

/// The schema-set pass builds the map once and checks every type with it: a
/// conflict that only the XSD 1.1 map shows (an abstract member) in the
/// *second* complex type is still found, and only under XSD 1.1.
#[test]
fn test_upa_pass_reuses_the_version_map_across_types() {
    let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
        <xs:element name="head" type="xs:string"/>
        <xs:element name="member" type="xs:string" substitutionGroup="head" abstract="true"/>
        <xs:complexType name="First">
            <xs:sequence>
                <xs:element ref="head"/>
                <xs:element name="other" type="xs:string"/>
            </xs:sequence>
        </xs:complexType>
        <xs:complexType name="Second">
            <xs:choice>
                <xs:element ref="head"/>
                <xs:element ref="member"/>
            </xs:choice>
        </xs:complexType>
    </xs:schema>"#;

    let mut xsd10 = SchemaSet::new();
    crate::pipeline::load_and_process_schema(xsd.as_bytes(), "upa.xsd", &mut xsd10, None)
        .expect("XSD 1.0: an abstract member does not compete with its head");

    let mut xsd11 = SchemaSet::with_version(XsdVersion::V1_1);
    let error =
        crate::pipeline::load_and_process_schema(xsd.as_bytes(), "upa.xsd", &mut xsd11, None)
            .expect_err("XSD 1.1: the abstract member competes with its head");
    assert_cos_nonambig(error);
}

/// `xs:all`: element particles compete by name and by substitution group;
/// the error names state 0 (no NFA).
#[test]
fn test_all_group_element_conflicts() {
    use crate::compiler::all_group::AllParticle;
    use crate::compiler::particle::MaxOccurs;

    let mut schema_set = create_test_schema_set_v11();
    let h = schema_set.name_table.add("h");
    let m = schema_set.name_table.add("m");
    let n = schema_set.name_table.add("n");
    let h_key = global_element(&mut schema_set, h, None, false, &[]);
    let m_key = global_element(&mut schema_set, m, None, false, &[h_key]);
    let n_key = global_element(&mut schema_set, n, None, false, &[]);

    let all = |terms: Vec<NfaTerm>| AllGroupModel {
        particles: terms
            .into_iter()
            .enumerate()
            .map(|(i, term)| AllParticle {
                term,
                min_occurs: 1,
                max_occurs: MaxOccurs::Bounded(1),
                source: span(i + 1),
            })
            .collect(),
        open_content: None,
        outer_optional: false,
    };
    let h_ref = NfaTerm::element(h, None, Some(h_key));
    let m_ref = NfaTerm::element(m, None, Some(m_key));
    let n_ref = NfaTerm::element(n, None, Some(n_key));

    let ok = all(vec![h_ref.clone(), n_ref.clone()]);
    assert!(check_all_group_upa(&ok, &schema_set, None).is_ok());

    for model in [
        all(vec![n_ref.clone(), h_ref.clone(), m_ref]),
        all(vec![h_ref, n_ref.clone(), n_ref]),
    ] {
        let error = check_all_group_upa(&model, &schema_set, None).unwrap_err();
        assert!(error.to_string().contains("at state 0"), "{error}");
        assert_cos_nonambig(error);
    }
}

/// Wildcard terms take no part in the element-element check; an element
/// competing with a wildcard is left to the element-wildcard check (an error
/// under XSD 1.0 only).
#[test]
fn test_element_element_check_leaves_wildcards_to_their_own_check() {
    let schema_set = create_test_schema_set();
    let a = schema_set.name_table.add("a");
    let b = schema_set.name_table.add("b");
    let any = || NfaTerm::wildcard(NamespaceConstraint::Any, ProcessContents::Lax);

    let elements = reachable_terms(vec![
        (any(), span(1)),
        (NfaTerm::element(a, None, None), span(2)),
        (any(), span(3)),
        (NfaTerm::element(b, None, None), span(4)),
    ]);
    assert_eq!(element_conflict_locations(&elements, &schema_set), None);

    let nfa = choice_of(vec![
        (NfaTerm::element(a, None, None), span(1)),
        (any(), span(2)),
    ]);
    let error = check_upa(&nfa, &schema_set, None).unwrap_err();
    assert!(
        error.to_string().contains("conflicts with wildcard"),
        "{error}"
    );
    assert!(check_upa(&nfa, &create_test_schema_set_v11(), None).is_ok());
}

/// With several competing pairs the first in pairwise order is reported —
/// the smallest first index, then the smallest second — as 0.2.2 did.
#[test]
fn test_element_element_conflict_reports_first_pair_in_scan_order() {
    let schema_set = create_test_schema_set();
    let a = schema_set.name_table.add("a");
    let b = schema_set.name_table.add("b");
    let elem = |name| NfaTerm::element(name, None, None);

    // Pairs (0,3) on `b` and (1,2) on `a`: (0,3) comes first.
    let elements = reachable_terms(vec![
        (elem(b), span(1)),
        (elem(a), span(2)),
        (elem(a), span(3)),
        (elem(b), span(4)),
    ]);
    assert_eq!(
        element_conflict_locations(&elements, &schema_set),
        Some((Some(1), Some(4)))
    );

    // Index 0 competes with 2 (`a`) and 3 (`a`): the smaller partner wins,
    // whichever key finds it.
    let elements = reachable_terms(vec![
        (elem(a), span(1)),
        (elem(b), span(2)),
        (elem(a), span(3)),
        (elem(a), span(4)),
    ]);
    assert_eq!(
        element_conflict_locations(&elements, &schema_set),
        Some((Some(1), Some(3)))
    );
}

/// Copies of one particle made by occurrence unrolling share its origin and
/// do not compete; particles without an origin always do.
#[test]
fn test_element_element_unrolled_copies_and_sourceless_particles() {
    let schema_set = create_test_schema_set();
    let a = schema_set.name_table.add("a");
    let elem = || NfaTerm::element(a, None, None);

    let copies = reachable_terms(vec![(elem(), span(1)), (elem(), span(1))]);
    assert_eq!(element_conflict_locations(&copies, &schema_set), None);

    let sourceless = reachable_terms(vec![(elem(), None), (elem(), None)]);
    assert_eq!(
        element_conflict_locations(&sourceless, &schema_set),
        Some((None, None))
    );

    // The first particle's copy is skipped, the distinct particle is not.
    let mixed = reachable_terms(vec![
        (elem(), span(1)),
        (elem(), span(1)),
        (elem(), span(2)),
    ]);
    assert_eq!(
        element_conflict_locations(&mixed, &schema_set),
        Some((Some(1), Some(2)))
    );

    // A particle without an origin competes with a copy-carrying one.
    let b = schema_set.name_table.add("b");
    let later = reachable_terms(vec![
        (elem(), span(1)),
        (NfaTerm::element(b, None, None), span(5)),
        (elem(), span(1)),
        (elem(), None),
    ]);
    assert_eq!(
        element_conflict_locations(&later, &schema_set),
        Some((Some(1), None))
    );
}

// ========================================================================
// Occurrence-bound capping tests (cap_for_upa)
// ========================================================================

#[test]
fn test_cap_for_upa_simple_unchanged() {
    assert_eq!(cap_for_upa(0, Some(1)), (0, Some(1)));
    assert_eq!(cap_for_upa(1, Some(1)), (1, Some(1)));
}

#[test]
fn test_cap_for_upa_dead_particle() {
    // Dead particle (maxOccurs=0) must be preserved, not resurrected
    assert_eq!(cap_for_upa(0, Some(0)), (0, Some(0)));
}

#[test]
fn test_dead_wildcard_no_upa_conflict() {
    // sequence(any{0,0}, e1) — dead wildcard should not conflict with e1
    let schema_set = create_test_schema_set();
    let name_e1 = schema_set.name_table.add("e1");

    let builder = FragmentBuilder::new();
    let wc = NfaTerm::wildcard(NamespaceConstraint::Any, ProcessContents::Lax);
    let elem = NfaTerm::element(name_e1, None, None);
    let frag_wc = builder.single_term(wc, None).repeat_range(0, Some(0));
    let frag_elem = builder.single_term(elem, None);
    let sequence = frag_wc.concat(frag_elem);
    let nfa = fragment_to_table(sequence);

    let result = check_upa(&nfa, &schema_set, None);
    assert!(
        result.is_ok(),
        "dead wildcard (maxOccurs=0) should not cause UPA conflict"
    );
}

#[test]
fn test_cap_for_upa_exact_repeat() {
    assert_eq!(cap_for_upa(5, Some(5)), (2, Some(2)));
    assert_eq!(cap_for_upa(2, Some(2)), (2, Some(2)));
    assert_eq!(cap_for_upa(100, Some(100)), (2, Some(2)));
}

#[test]
fn test_cap_for_upa_optional_with_repetition() {
    assert_eq!(cap_for_upa(0, Some(5000)), (0, Some(2)));
    assert_eq!(cap_for_upa(0, None), (0, Some(2)));
    assert_eq!(cap_for_upa(0, Some(2)), (0, Some(2)));
}

#[test]
fn test_cap_for_upa_required_with_repetition() {
    assert_eq!(cap_for_upa(1, Some(5000)), (1, Some(2)));
    assert_eq!(cap_for_upa(3, Some(5000)), (1, Some(2)));
    assert_eq!(cap_for_upa(1, None), (1, Some(2)));
    assert_eq!(cap_for_upa(10, None), (1, Some(2)));
}

// ========================================================================
// Counted ambiguity detection tests
// ========================================================================

#[test]
fn test_counted_element_trailing_sibling_conflict() {
    // sequence(a{1,2}, a) — UPA violation: iteration boundary ambiguity
    // Simulates capped a{1,5000} followed by a
    let schema_set = create_test_schema_set();
    let name_a = schema_set.name_table.add("a");

    let builder = FragmentBuilder::new();
    let term1 = NfaTerm::element(name_a, None, None);
    let term2 = NfaTerm::element(name_a, None, None);
    let frag1 = builder.single_term(term1, None);
    let repeated = frag1.repeat_range(1, Some(2));
    let frag2 = builder.single_term(term2, None);
    let sequence = repeated.concat(frag2);
    let nfa = fragment_to_table(sequence);

    let result = check_upa(&nfa, &schema_set, None);
    assert!(
        result.is_err(),
        "should detect UPA violation for a{{1,2}} followed by a"
    );
    assert_cos_nonambig(result.unwrap_err());
}

#[test]
fn test_nullable_counted_trailing_sibling_conflict() {
    // sequence(a{0,2}, a) — UPA violation: nullable iteration boundary
    let schema_set = create_test_schema_set();
    let name_a = schema_set.name_table.add("a");

    let builder = FragmentBuilder::new();
    let term1 = NfaTerm::element(name_a, None, None);
    let term2 = NfaTerm::element(name_a, None, None);
    let frag1 = builder.single_term(term1, None);
    let repeated = frag1.repeat_range(0, Some(2));
    let frag2 = builder.single_term(term2, None);
    let sequence = repeated.concat(frag2);
    let nfa = fragment_to_table(sequence);

    let result = check_upa(&nfa, &schema_set, None);
    assert!(
        result.is_err(),
        "should detect UPA violation for a{{0,2}} followed by a"
    );
    assert_cos_nonambig(result.unwrap_err());
}

#[test]
fn test_counted_different_elements_no_conflict() {
    // sequence(a{1,2}, b{1,2}) — no UPA violation
    let schema_set = create_test_schema_set();
    let name_a = schema_set.name_table.add("a");
    let name_b = schema_set.name_table.add("b");

    let builder = FragmentBuilder::new();
    let term_a = NfaTerm::element(name_a, None, None);
    let term_b = NfaTerm::element(name_b, None, None);
    let frag_a = builder.single_term(term_a, None);
    let frag_b = builder.single_term(term_b, None);
    let repeated_a = frag_a.repeat_range(1, Some(2));
    let repeated_b = frag_b.repeat_range(1, Some(2));
    let sequence = repeated_a.concat(repeated_b);
    let nfa = fragment_to_table(sequence);

    let result = check_upa(&nfa, &schema_set, None);
    assert!(
        result.is_ok(),
        "sequence(a{{1,2}}, b{{1,2}}) should not violate UPA"
    );
}

#[test]
fn test_single_repeated_element_no_conflict() {
    // a{0,2} alone — no UPA violation (same particle, not two different ones).
    // Uses a shared source origin so same_particle_origin identifies the
    // unrolled copies as the same particle, matching real compiled NFAs.
    let schema_set = create_test_schema_set();
    let name_a = schema_set.name_table.add("a");
    let origin = Some(SourceRef::new(0, SourceSpan::new(0, 10)));

    let builder = FragmentBuilder::new();
    let term = NfaTerm::element(name_a, None, None);
    let frag = builder.single_term(term, origin);
    let repeated = frag.repeat_range(0, Some(2));
    let nfa = fragment_to_table(repeated);

    let result = check_upa(&nfa, &schema_set, None);
    assert!(
        result.is_ok(),
        "single repeated element should not violate UPA"
    );
}

// ========================================================================
// Schema-level UPA compilation
// ========================================================================

/// The UPA path compiles every particle with capped occurrence bounds
/// (`cap_for_upa`), so the model it produces is counter-free and the
/// epsilon-closure analysis below can run on it. `compile_base_all_group`
/// used to reach for the public, non-UPA entry point, compiling the base
/// type of an extension with uncapped bounds while the rest of the same
/// compilation was capped; both halves now run in the same mode.
///
/// `Derived` extends an all-group base (§3.4.2.3.3 clause 4.2.3.2: one merged
/// all group, the base's particles followed by the extension's) and
/// `DerivedSeq` extends a sequence base — the two shapes the UPA path can
/// produce for an extension. Every particle carries `maxOccurs="1000"`, which
/// outside UPA mode compiles to counted NFA transitions.
#[cfg(feature = "xsd11")]
#[test]
fn test_upa_extension_of_all_group_base_is_counter_free() {
    use crate::compiler::ContentModelMatcher;

    let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
        <xs:complexType name="Base">
            <xs:all>
                <xs:element name="a" type="xs:string" maxOccurs="1000"/>
                <xs:element name="b" type="xs:string" maxOccurs="1000"/>
            </xs:all>
        </xs:complexType>
        <xs:complexType name="Derived">
            <xs:complexContent>
                <xs:extension base="Base">
                    <xs:all>
                        <xs:element name="c" type="xs:string" maxOccurs="1000"/>
                    </xs:all>
                </xs:extension>
            </xs:complexContent>
        </xs:complexType>
        <xs:complexType name="SeqBase">
            <xs:sequence maxOccurs="1000">
                <xs:element name="d" type="xs:string" maxOccurs="1000"/>
            </xs:sequence>
        </xs:complexType>
        <xs:complexType name="DerivedSeq">
            <xs:complexContent>
                <xs:extension base="SeqBase">
                    <xs:sequence maxOccurs="1000">
                        <xs:element name="e" type="xs:string" maxOccurs="1000"/>
                    </xs:sequence>
                </xs:extension>
            </xs:complexContent>
        </xs:complexType>
    </xs:schema>"#;

    let mut schema_set = SchemaSet::xsd11();
    crate::pipeline::load_and_process_schema(xsd.as_bytes(), "upa.xsd", &mut schema_set, None)
        .expect("schema should load and pass UPA checking");

    let upa_matcher = |local: &str| {
        let key = crate::compiler::inspect::find_complex_type(&schema_set, None, local)
            .unwrap_or_else(|| panic!("complex type {local} not found"));
        crate::compiler::compile_content_model_for_upa(
            &schema_set,
            &schema_set.arenas.complex_types[key],
        )
        .unwrap_or_else(|e| panic!("UPA compilation of {local} failed: {e}"))
    };

    // The all-group base merges with the extension's all group; an all-group
    // model runs no automaton, so it carries no counters by construction.
    match upa_matcher("Derived") {
        ContentModelMatcher::AllGroup(model) => {
            assert_eq!(
                model.particles.len(),
                3,
                "base particles followed by the extension's"
            );
        }
        other => panic!("expected a merged all-group UPA model, got {other:?}"),
    }

    // The sequence base goes through the NFA path; capping must leave it
    // counter-free, which is what the closure-based UPA analysis requires.
    match upa_matcher("DerivedSeq") {
        ContentModelMatcher::Nfa(nfa) | ContentModelMatcher::WithOpenContent { nfa, .. } => {
            assert!(
                !nfa.has_counters(),
                "UPA model must be counter-free, found {:?}",
                nfa.counter_defs
            );
        }
        other => panic!("expected an NFA UPA model, got {other:?}"),
    }
}

// ========================================================================
// P15: the reported pair does not depend on hashing
// ========================================================================

/// A competing pair is reported in the order the epsilon closure collects the
/// terms, which follows the NFA's state order, not a hash set's: the same
/// schema names the same two particles on every compile, in every process.
///
/// The model is Exchange Web Services' `ArrayOfTransitionsType`: four
/// optional, repeatable element references, the last the head of a
/// substitution group whose members are the other three. The closure of the
/// start state reaches all four, and the head competes with each member.
#[test]
fn test_reported_conflict_pair_is_deterministic() {
    let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
        xmlns:t="urn:ews" targetNamespace="urn:ews" elementFormDefault="qualified">
      <xs:complexType name="TransitionType">
        <xs:sequence><xs:element name="To" type="xs:string"/></xs:sequence>
      </xs:complexType>
      <xs:complexType name="AbsoluteDateTransitionType">
        <xs:complexContent><xs:extension base="t:TransitionType">
          <xs:sequence><xs:element name="DateTime" type="xs:dateTime"/></xs:sequence>
        </xs:extension></xs:complexContent>
      </xs:complexType>
      <xs:element name="Transition" type="t:TransitionType"/>
      <xs:element name="AbsoluteDateTransition" type="t:AbsoluteDateTransitionType"
          substitutionGroup="t:Transition"/>
      <xs:element name="RecurringDayTransition" type="t:AbsoluteDateTransitionType"
          substitutionGroup="t:Transition"/>
      <xs:element name="RecurringDateTransition" type="t:AbsoluteDateTransitionType"
          substitutionGroup="t:Transition"/>
      <xs:complexType name="ArrayOfTransitionsType">
        <xs:sequence>
          <xs:element ref="t:AbsoluteDateTransition" minOccurs="0" maxOccurs="unbounded"/>
          <xs:element ref="t:RecurringDayTransition" minOccurs="0" maxOccurs="unbounded"/>
          <xs:element ref="t:RecurringDateTransition" minOccurs="0" maxOccurs="unbounded"/>
          <xs:element ref="t:Transition" minOccurs="0" maxOccurs="unbounded"/>
        </xs:sequence>
      </xs:complexType>
    </xs:schema>"#;

    for version in [XsdVersion::V1_0, XsdVersion::V1_1] {
        let messages: std::collections::BTreeSet<String> = (0..24)
            .map(|_| {
                let Err(error) = crate::SchemaSetBuilder::with_version(version)
                    .add_source(xsd, "file:///ews-transitions.xsd")
                    .expect("schema source accepted")
                    .compile()
                else {
                    panic!("{version:?}: ArrayOfTransitionsType violates UPA");
                };
                assert!(
                    error.to_string().contains("cos-nonambig"),
                    "{version:?}: expected cos-nonambig, got {error}"
                );
                error.to_string()
            })
            .collect();
        assert_eq!(
            messages.len(),
            1,
            "{version:?}: the reported pair varies between compiles: {messages:#?}"
        );
        let message = messages.into_iter().next().unwrap();
        assert!(
            message.contains("elements 'AbsoluteDateTransition' and 'Transition' conflict"),
            "{version:?}: the first competing pair in state order: {message}"
        );
    }
}
