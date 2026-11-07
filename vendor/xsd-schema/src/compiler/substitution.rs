//! Substitution group helpers for validation and UPA checks.

use std::collections::{HashMap, HashSet};

use ahash::RandomState;

use crate::ids::{ElementKey, NameId, TypeKey};
use crate::schema::model::{DerivationSet, SchemaSet};

/// Set of `(local-name, namespace)` substitutable element identities.
///
/// Keys are interned integer `NameId`s, not strings, so this uses `ahash`
/// (fast, keyed) instead of SipHash — mirroring [`AttrNameSet`] in
/// `validation::context` and the UPA conflict sets that clone these. The UPA
/// overlap check (`check_element_element_conflicts`) does O(n²) `contains`
/// lookups over these, so the default-SipHash cost was a top compiler hot spot.
///
/// [`AttrNameSet`]: crate::validation::context::AttrNameSet
pub type SubstitutableNameSet = HashSet<(NameId, Option<NameId>), RandomState>;

/// Map from substitution group head to all substitutable element names.
///
/// Keys are interned slotmap `ElementKey`s (not attacker-controlled bytes) and
/// this map is `.get()`-probed per candidate NFA `Element` term in
/// `term_matches` on the per-child validation hot path, so it uses keyed
/// `ahash` rather than the default SipHash — same rationale as the inner
/// `SubstitutableNameSet`.
pub type SubstitutionGroupMap = HashMap<ElementKey, SubstitutableNameSet, RandomState>;

/// Build a substitution group membership map for the schema set.
pub fn build_substitution_group_map(schema_set: &SchemaSet) -> SubstitutionGroupMap {
    build_substitution_group_map_inner(schema_set, false)
}

/// Build a substitution group membership map that includes abstract members.
///
/// Per XSD 1.1 (W3C Bugzilla 4337), abstract elements participate in the
/// substitution group for schema-time UPA / cos-element-consistent (EDC)
/// constraints, even though they cannot appear in instances. This variant is
/// used by UPA/EDC schema-time validation under XSD 1.1.
pub fn build_substitution_group_map_with_abstract(schema_set: &SchemaSet) -> SubstitutionGroupMap {
    build_substitution_group_map_inner(schema_set, true)
}

fn build_substitution_group_map_inner(
    schema_set: &SchemaSet,
    include_abstract: bool,
) -> SubstitutionGroupMap {
    let mut member_map: HashMap<ElementKey, Vec<ElementKey>> = HashMap::new();
    for (member_key, elem) in schema_set.arenas.elements.iter() {
        for head_key in &elem.resolved_substitution_groups {
            member_map.entry(*head_key).or_default().push(member_key);
        }
    }

    let mut result = SubstitutionGroupMap::default();
    let mut seen_heads = HashSet::new();
    for (head_key, _) in schema_set.arenas.elements.iter() {
        let head_key = resolve_element_key(schema_set, head_key);
        if !seen_heads.insert(head_key) {
            continue;
        }

        let head_elem = match schema_set.arenas.elements.get(head_key) {
            Some(elem) => elem,
            None => continue,
        };
        let mut names = SubstitutableNameSet::default();
        if let Some(name) = head_elem.name {
            if !head_elem.is_abstract || include_abstract {
                names.insert((name, head_elem.target_namespace));
            }
        }

        let (effective_block, effective_final) =
            effective_element_constraints(schema_set, head_elem);
        if !effective_block.contains_substitution() {
            let head_type = head_elem.resolved_type;
            // Clause 2.3 item (2): fold in the head type's {prohibited
            // substitutions}.
            let type_block = head_type_prohibited_substitutions(schema_set, head_type);
            let exclude = derivation_exclusions(effective_block | type_block, effective_final);

            let mut stack = member_map.get(&head_key).cloned().unwrap_or_default();
            let mut visited = HashSet::new();
            while let Some(member_key) = stack.pop() {
                if !visited.insert(member_key) {
                    continue;
                }
                if let Some(member) = resolved_element(schema_set, member_key) {
                    if let Some(name) = member.name {
                        if (!member.is_abstract || include_abstract)
                            && is_substitutable(
                                schema_set,
                                head_type,
                                exclude,
                                member.resolved_type,
                            )
                        {
                            names.insert((name, member.target_namespace));
                        }
                    }
                }
                if let Some(nested) = member_map.get(&member_key) {
                    for &next in nested {
                        if !visited.contains(&next) {
                            stack.push(next);
                        }
                    }
                }
            }
        }

        // Prune the trivial entry: a head whose substitutable set is exactly
        // its own name adds no information — every consumer of this map
        // (`term_matches`, `term_matches_with_substitution`, EDC's member
        // expansion, UPA's `element_substitutable_names`) falls back to the
        // same direct self-match when the entry is absent. Pruning makes the
        // map *empty* for the common substitution-free schema, letting
        // `SchemaValidator::new` drop it to `None` so the per-child hash
        // probes in `term_matches` disappear entirely. Entries that differ
        // from the trivial self-match must stay: abstract heads (their set
        // omits the self-name — that omission is what blocks abstract
        // elements from matching in instances) and heads with substitutable
        // members.
        let is_trivial_self_match = head_elem
            .name
            .is_some_and(|n| names.len() == 1 && names.contains(&(n, head_elem.target_namespace)));
        if !is_trivial_self_match {
            result.insert(head_key, names);
        }
    }

    result
}

fn resolve_element_key(schema_set: &SchemaSet, key: ElementKey) -> ElementKey {
    schema_set
        .arenas
        .elements
        .get(key)
        .and_then(|elem| elem.resolved_ref)
        .unwrap_or(key)
}

fn resolved_element(
    schema_set: &SchemaSet,
    key: ElementKey,
) -> Option<&crate::arenas::ElementDeclData> {
    let key = resolve_element_key(schema_set, key);
    schema_set.arenas.elements.get(key)
}

pub(crate) fn derivation_exclusions(
    effective_block: DerivationSet,
    effective_final: DerivationSet,
) -> DerivationSet {
    // Per §3.3.6.3 / §3.9.6 the exclusion set is built solely from the
    // head *element's* {substitution group exclusions} (element `final`)
    // and, for instance-time checks, its `block` attribute.  The head
    // *type's* {final} is intentionally excluded here: is_type_derived_from
    // walks the full derivation chain and the type's own finality is a
    // property of the type hierarchy, not the element declaration.
    (effective_block | effective_final) & derivation_mask()
}

fn derivation_mask() -> DerivationSet {
    DerivationSet::EXTENSION
        | DerivationSet::RESTRICTION
        | DerivationSet::LIST
        | DerivationSet::UNION
}

pub(crate) fn is_substitutable(
    schema_set: &SchemaSet,
    head_type: Option<TypeKey>,
    exclude: DerivationSet,
    member_type: Option<TypeKey>,
) -> bool {
    let any_type = TypeKey::Complex(schema_set.any_type_key());
    let head_type = head_type.unwrap_or(any_type);
    let member_type = member_type.unwrap_or(any_type);
    schema_set.is_type_derived_from(member_type, head_type, exclude)
}

pub(crate) fn effective_element_constraints(
    _schema_set: &SchemaSet,
    element: &crate::arenas::ElementDeclData,
) -> (DerivationSet, DerivationSet) {
    // Both block and final_derivation are resolved at assembly time
    // (the assembler applies blockDefault/finalDefault to empty/absent entries).
    (element.block, element.final_derivation)
}

/// §3.3.6.3 Substitution Group OK (Transitive) clause 2.3, item (2): the
/// blocking union includes the head's {type definition}.{prohibited
/// substitutions} when that type is complex (sunData disallowedSubst005:
/// `<xs:complexType block="restriction">` on the head's type forbids
/// substitution by restriction-derived members even though the head
/// *element* carries no block).
pub(crate) fn head_type_prohibited_substitutions(
    schema_set: &SchemaSet,
    head_type: Option<TypeKey>,
) -> DerivationSet {
    match head_type {
        Some(TypeKey::Complex(ct_key)) => schema_set
            .arenas
            .complex_types
            .get(ct_key)
            .map(|ct| ct.block)
            .unwrap_or_else(DerivationSet::empty),
        _ => DerivationSet::empty(),
    }
}

/// Whether `candidate_key` is a member, directly or transitively, of the
/// substitution group headed by `head_key`.
///
/// This is *declared membership only* — the `{substitution group affiliation}`
/// chain of XSD §3.3.1 — with no `block`, `final` or type-derivation check.
/// [`is_element_substitutable_for`] adds those; XPath 2.0 §2.5.4.4's
/// "matches the name of an element in a substitution group headed by an element
/// named `ElementName`" wants the bare membership relation.
///
/// An element is **not** a member of the group it heads itself; callers that
/// need "the head or one of its members" test the head separately.
pub(crate) fn is_substitution_group_member(
    schema_set: &SchemaSet,
    head_key: ElementKey,
    candidate_key: ElementKey,
) -> bool {
    let Some(candidate_elem) = schema_set.arenas.elements.get(candidate_key) else {
        return false;
    };
    let mut visited = HashSet::new();
    let mut stack: Vec<ElementKey> = candidate_elem.resolved_substitution_groups.clone();
    while let Some(sg_head) = stack.pop() {
        if !visited.insert(sg_head) {
            continue;
        }
        if sg_head == head_key {
            return true;
        }
        if let Some(sg_elem) = schema_set.arenas.elements.get(sg_head) {
            stack.extend_from_slice(&sg_elem.resolved_substitution_groups);
        }
    }
    false
}

/// Check if `candidate_key` is validly substitutable for `head_key`
/// per XSD §3.3.6.3 / §3.9.6 NameAndTypeOK.
pub(crate) fn is_element_substitutable_for(
    schema_set: &SchemaSet,
    head_key: ElementKey,
    candidate_key: ElementKey,
) -> bool {
    let Some(head_elem) = schema_set.arenas.elements.get(head_key) else {
        return false;
    };
    let Some(candidate_elem) = schema_set.arenas.elements.get(candidate_key) else {
        return false;
    };

    // Check declared substitution group membership (direct or transitive).
    if !is_substitution_group_member(schema_set, head_key, candidate_key) {
        return false;
    }

    // Check block constraints on the head element
    let (effective_block, effective_final) = effective_element_constraints(schema_set, head_elem);
    if effective_block.contains_substitution() {
        return false;
    }

    // Check type derivation with exclusion mask (clause 2.3 items (1)+(2):
    // head element block ∪ head type {prohibited substitutions}).
    let type_block = head_type_prohibited_substitutions(schema_set, head_elem.resolved_type);
    let exclude = derivation_exclusions(effective_block | type_block, effective_final);
    is_substitutable(
        schema_set,
        head_elem.resolved_type,
        exclude,
        candidate_elem.resolved_type,
    )
}

/// Check e-props-correct.4: member type must be validly substitutable for
/// head type subject to the head's `{substitution group exclusions}` (= `final`).
///
/// Unlike `is_element_substitutable_for`, this does NOT check `block` because
/// `block` controls instance-time substitution, not affiliation legality.
fn check_substitution_group_affiliation(
    schema_set: &SchemaSet,
    head_key: ElementKey,
    member_key: ElementKey,
) -> bool {
    let Some(head_elem) = schema_set.arenas.elements.get(head_key) else {
        return false;
    };
    let Some(member_elem) = schema_set.arenas.elements.get(member_key) else {
        return false;
    };
    let (_, effective_final) = effective_element_constraints(schema_set, head_elem);
    let exclude = derivation_exclusions(DerivationSet::empty(), effective_final);
    is_substitutable(
        schema_set,
        head_elem.resolved_type,
        exclude,
        member_elem.resolved_type,
    )
}

/// Validate all declared substitution group memberships.
///
/// Reports `e-props-correct.4` if a member element's type is not validly
/// substitutable for its head element's type (respecting `final` constraints).
///
/// Note: This uses only the head's `{substitution group exclusions}` (= `final`),
/// NOT the head's `block` attribute. The `block` attribute controls instance-time
/// substitution, not schema-level affiliation legality.
pub fn validate_all_substitution_groups(schema_set: &SchemaSet) -> crate::SchemaResult<()> {
    for (member_key, elem) in schema_set.arenas.elements.iter() {
        for &head_key in &elem.resolved_substitution_groups {
            if !check_substitution_group_affiliation(schema_set, head_key, member_key) {
                let member_name = elem
                    .name
                    .map(|n| schema_set.name_table.resolve(n).to_string())
                    .unwrap_or_else(|| "<anonymous>".to_string());
                let head_name = schema_set
                    .arenas
                    .elements
                    .get(head_key)
                    .and_then(|h| h.name)
                    .map(|n| schema_set.name_table.resolve(n).to_string())
                    .unwrap_or_else(|| "<anonymous>".to_string());
                let location = elem
                    .source
                    .as_ref()
                    .and_then(|s| schema_set.source_maps.locate(s));
                return Err(crate::error::SchemaError::structural(
                    "e-props-correct.4",
                    format!(
                        "Element '{}' is not a valid member of the substitution group \
                         headed by '{}': type derivation is blocked by 'final' constraint",
                        member_name, head_name
                    ),
                    location,
                ));
            }
        }
    }

    // §3.3.6.1.5: substitution group affiliation must be acyclic. Walk each
    // element's resolved substitution-group chain and reject any back-edge.
    for (start_key, _) in schema_set.arenas.elements.iter() {
        let mut visited = HashSet::new();
        let mut stack = vec![start_key];
        while let Some(current) = stack.pop() {
            if !visited.insert(current) {
                continue;
            }
            let Some(decl) = schema_set.arenas.elements.get(current) else {
                continue;
            };
            for &head in &decl.resolved_substitution_groups {
                if head == start_key {
                    let elem = &schema_set.arenas.elements[start_key];
                    let elem_name = elem
                        .name
                        .map(|n| schema_set.name_table.resolve(n).to_string())
                        .unwrap_or_else(|| "<anonymous>".to_string());
                    let location = elem
                        .source
                        .as_ref()
                        .and_then(|s| schema_set.source_maps.locate(s));
                    return Err(crate::error::SchemaError::structural(
                        "e-props-correct",
                        format!(
                            "Substitution group cycle detected involving element '{}' \
                             (§3.3.6.1.5)",
                            elem_name
                        ),
                        location,
                    ));
                }
                stack.push(head);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::location::SourceRef;

    fn element_data(
        name: NameId,
        type_key: TypeKey,
        source: Option<SourceRef>,
    ) -> crate::arenas::ElementDeclData {
        crate::arenas::ElementDeclData {
            name: Some(name),
            target_namespace: None,
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
            source,
            resolved_type: Some(type_key),
            resolved_ref: None,
            resolved_substitution_groups: Vec::new(),
            deferred_type_error: None,
        }
    }

    /// Substitution-free schemas must produce an *empty* map: the builder
    /// prunes trivial self-match entries, and `SchemaValidator::new` relies on
    /// emptiness to drop the map to `None` (skipping the per-child hash probes
    /// in `term_matches` entirely).
    #[test]
    fn test_substitution_group_map_empty_without_substitution() {
        let mut schema_set = SchemaSet::new();
        let a_name = schema_set.name_table.add("a");
        let b_name = schema_set.name_table.add("b");
        let ty = TypeKey::Simple(schema_set.builtin_types().decimal);
        schema_set
            .arenas
            .alloc_element(element_data(a_name, ty, None));
        schema_set
            .arenas
            .alloc_element(element_data(b_name, ty, None));

        let map = build_substitution_group_map(&schema_set);
        assert!(map.is_empty());
    }

    /// An abstract element's entry must NOT be pruned even when it has no
    /// members: its set omits the self-name, and that omission is what stops
    /// `term_matches` from accepting the abstract element in instances. An
    /// absent entry would fall back to the direct self-match and wrongly
    /// accept it.
    #[test]
    fn test_substitution_group_abstract_head_entry_kept() {
        let mut schema_set = SchemaSet::new();
        let head_name = schema_set.name_table.add("head");
        let head_type = TypeKey::Simple(schema_set.builtin_types().decimal);

        let mut head = element_data(head_name, head_type, None);
        head.is_abstract = true;
        let head_key = schema_set.arenas.alloc_element(head);

        let map = build_substitution_group_map(&schema_set);
        let names = map.get(&head_key).expect("abstract entry must be kept");
        assert!(!names.contains(&(head_name, None)));

        // Under the with-abstract variant (schema-time UPA/EDC) the entry IS
        // the trivial self-match, so it is pruned there — equivalent to the
        // consumers' direct-match fallback.
        let map = build_substitution_group_map_with_abstract(&schema_set);
        assert!(!map.contains_key(&head_key));
    }

    #[test]
    fn test_substitution_group_type_derivation_allows_member() {
        let mut schema_set = SchemaSet::new();
        let head_name = schema_set.name_table.add("head");
        let member_name = schema_set.name_table.add("member");
        let head_type = TypeKey::Simple(schema_set.builtin_types().decimal);
        let member_type = TypeKey::Simple(schema_set.builtin_types().int);

        let head_key = schema_set
            .arenas
            .alloc_element(element_data(head_name, head_type, None));
        let member_key =
            schema_set
                .arenas
                .alloc_element(element_data(member_name, member_type, None));
        schema_set
            .arenas
            .entries_mut()
            .elements
            .get_mut(member_key)
            .unwrap()
            .resolved_substitution_groups
            .push(head_key);

        let map = build_substitution_group_map(&schema_set);
        let names = map.get(&head_key).unwrap();
        assert!(names.contains(&(member_name, None)));
    }

    #[test]
    fn test_substitution_group_element_final_blocks_member() {
        let mut schema_set = SchemaSet::new();
        let head_name = schema_set.name_table.add("head");
        let member_name = schema_set.name_table.add("member");
        let head_type = TypeKey::Simple(schema_set.builtin_types().decimal);
        let member_type = TypeKey::Simple(schema_set.builtin_types().int);

        let mut head = element_data(head_name, head_type, None);
        head.final_derivation = DerivationSet::RESTRICTION;
        let head_key = schema_set.arenas.alloc_element(head);
        let member_key =
            schema_set
                .arenas
                .alloc_element(element_data(member_name, member_type, None));
        schema_set
            .arenas
            .entries_mut()
            .elements
            .get_mut(member_key)
            .unwrap()
            .resolved_substitution_groups
            .push(head_key);

        let map = build_substitution_group_map(&schema_set);
        // The member is blocked, so the head's substitutable set collapses to
        // the trivial self-match — which the builder prunes (an absent entry
        // means "matches only its own name" to every consumer).
        assert!(!map.contains_key(&head_key));
    }

    // Per §3.3.4 the {substitution group exclusions} are derived solely from the
    // *element* declaration's `final` attribute.  The head *type's* {final} does
    // not gate substitution group membership.  This test verifies that setting
    // final_derivation on the head type alone does NOT block the member.
    #[test]
    fn test_substitution_group_type_final_does_not_block_member() {
        let mut schema_set = SchemaSet::new();
        let head_name = schema_set.name_table.add("head");
        let member_name = schema_set.name_table.add("member");
        let head_type = schema_set.builtin_types().decimal;
        let member_type = TypeKey::Simple(schema_set.builtin_types().int);

        // Mark the head *type* as final for restriction — this must not block the member.
        if let Some(type_def) = schema_set
            .arenas
            .entries_mut()
            .simple_types
            .get_mut(head_type)
        {
            type_def.final_derivation = DerivationSet::RESTRICTION;
        }

        let head_key = schema_set.arenas.alloc_element(element_data(
            head_name,
            TypeKey::Simple(head_type),
            None,
        ));
        let member_key =
            schema_set
                .arenas
                .alloc_element(element_data(member_name, member_type, None));
        schema_set
            .arenas
            .entries_mut()
            .elements
            .get_mut(member_key)
            .unwrap()
            .resolved_substitution_groups
            .push(head_key);

        let map = build_substitution_group_map(&schema_set);
        let names = map.get(&head_key).unwrap();
        // The type's {final} must NOT gate membership; only the element's final does.
        assert!(names.contains(&(member_name, None)));
    }

    #[test]
    fn test_substitution_group_block_substitution_keeps_head_only() {
        let mut schema_set = SchemaSet::new();
        let head_name = schema_set.name_table.add("head");
        let member_name = schema_set.name_table.add("member");
        let head_type = TypeKey::Simple(schema_set.builtin_types().decimal);
        let member_type = TypeKey::Simple(schema_set.builtin_types().int);

        let mut head = element_data(head_name, head_type, None);
        head.block = DerivationSet::SUBSTITUTION;
        let head_key = schema_set.arenas.alloc_element(head);
        let member_key =
            schema_set
                .arenas
                .alloc_element(element_data(member_name, member_type, None));
        schema_set
            .arenas
            .entries_mut()
            .elements
            .get_mut(member_key)
            .unwrap()
            .resolved_substitution_groups
            .push(head_key);

        let map = build_substitution_group_map(&schema_set);
        // The member is blocked, so the head's substitutable set collapses to
        // the trivial self-match — which the builder prunes (an absent entry
        // means "matches only its own name" to every consumer).
        assert!(!map.contains_key(&head_key));
    }

    #[test]
    fn test_substitution_group_block_default_blocks_member() {
        // Assembly would apply blockDefault to elements without an explicit block.
        // This test simulates that: head.block = SUBSTITUTION (inherited from blockDefault).
        let mut schema_set = SchemaSet::new();
        let head_name = schema_set.name_table.add("head");
        let member_name = schema_set.name_table.add("member");
        let head_type = TypeKey::Simple(schema_set.builtin_types().decimal);
        let member_type = TypeKey::Simple(schema_set.builtin_types().int);

        let mut head = element_data(head_name, head_type, None);
        head.block = DerivationSet::SUBSTITUTION;
        let head_key = schema_set.arenas.alloc_element(head);
        let member_key =
            schema_set
                .arenas
                .alloc_element(element_data(member_name, member_type, None));
        schema_set
            .arenas
            .entries_mut()
            .elements
            .get_mut(member_key)
            .unwrap()
            .resolved_substitution_groups
            .push(head_key);

        let map = build_substitution_group_map(&schema_set);
        // The member is blocked, so the head's substitutable set collapses to
        // the trivial self-match — which the builder prunes (an absent entry
        // means "matches only its own name" to every consumer).
        assert!(!map.contains_key(&head_key));
    }

    #[test]
    fn test_substitution_group_final_default_blocks_member() {
        // Assembly would apply finalDefault to elements without an explicit final.
        // This test simulates that: head.final_derivation = RESTRICTION (inherited from finalDefault).
        let mut schema_set = SchemaSet::new();
        let head_name = schema_set.name_table.add("head");
        let member_name = schema_set.name_table.add("member");
        let head_type = TypeKey::Simple(schema_set.builtin_types().decimal);
        let member_type = TypeKey::Simple(schema_set.builtin_types().int);

        let mut head = element_data(head_name, head_type, None);
        head.final_derivation = DerivationSet::RESTRICTION;
        let head_key = schema_set.arenas.alloc_element(head);
        let member_key =
            schema_set
                .arenas
                .alloc_element(element_data(member_name, member_type, None));
        schema_set
            .arenas
            .entries_mut()
            .elements
            .get_mut(member_key)
            .unwrap()
            .resolved_substitution_groups
            .push(head_key);

        let map = build_substitution_group_map(&schema_set);
        // The member is blocked, so the head's substitutable set collapses to
        // the trivial self-match — which the builder prunes (an absent entry
        // means "matches only its own name" to every consumer).
        assert!(!map.contains_key(&head_key));
    }
}
