//! Differential tests: the 0.2.3 UPA check against the 0.2.2 one
//! (`upa_reference.rs`) on generated content models.
//!
//! - `element_element_check_matches_pairwise_scan`: the element–element check
//!   on random particle lists and random substitution maps — including maps no
//!   schema produces (sets without the head's own name, empty sets, keys whose
//!   set disagrees with the particle's name) — must return the same result:
//!   the same pair, in the same order, with the same error.
//! - `check_upa_matches_reference_on_random_content_models`: random
//!   sequence/choice/repetition models over element references, local
//!   elements and wildcards, with substitution groups (abstract heads and
//!   members, blocked heads, several heads per member) under XSD 1.0 and 1.1
//!   rules. Every NFA state is compared on one shared epsilon closure; the
//!   whole check is compared by verdict, violation kind and state.
//! - `check_all_group_upa_matches_reference`: `xs:all` models, compared
//!   exactly.
//! - `epsilon_closure_matches_reference`: the closure with a reused visited
//!   set reaches exactly the terms of the 0.2.3 closure, and lists them in
//!   increasing state order.
//!
//! Inputs come from a fixed-seed generator, so a failure reproduces.
//! `UPA_DIFF_CASES=<n>` and `UPA_DIFF_SEED=<n>` widen or vary a run; under
//! Miri the default case counts shrink a hundredfold.

use super::reference;
use std::collections::BTreeSet;

use super::{
    check_all_group_upa, check_all_group_upa_with_substitutions, check_element_element_conflicts,
    check_upa, check_upa_with_substitutions, epsilon_closure_with_terms,
    epsilon_closure_with_terms_in, upa_substitution_map, ClosureScratch, ReachableTerm,
};
use crate::compiler::all_group::{AllGroupModel, AllParticle};
use crate::compiler::fragment::{fragment_to_table, FragmentBuilder, NfaFragment};
use crate::compiler::nfa::{NfaTable, NfaTerm, StateId, TransitionKind};
use crate::compiler::particle::MaxOccurs;
use crate::compiler::substitution::{SubstitutableNameSet, SubstitutionGroupMap};
use crate::error::SchemaResult;
use crate::ids::{ElementKey, NameId};
use crate::parser::location::{SourceRef, SourceSpan};
use crate::schema::model::{DerivationSet, SchemaSet, XsdVersion};
use crate::types::complex::{NamespaceConstraint, ProcessContents};

/// SplitMix64: a dependency-free, deterministic generator.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn percent(&mut self, p: usize) -> bool {
        self.below(100) < p
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.below(items.len())]
    }
}

fn env_or(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn cases(default: u64) -> u64 {
    let default = if cfg!(miri) { default / 100 } else { default };
    env_or("UPA_DIFF_CASES", default)
}

fn seed(salt: u64) -> Rng {
    Rng(env_or("UPA_DIFF_SEED", 0x5EED_0223) ^ salt)
}

/// Local names and namespaces of one generated schema: few enough that
/// names collide often, across namespaces too.
struct NamePool {
    locals: Vec<NameId>,
    namespaces: Vec<Option<NameId>>,
}

impl NamePool {
    fn new(schema_set: &SchemaSet) -> Self {
        let locals = ["a", "b", "c", "d"]
            .iter()
            .map(|n| schema_set.name_table.add(n))
            .collect();
        let namespaces = vec![
            None,
            Some(schema_set.name_table.add("urn:x")),
            Some(schema_set.name_table.add("urn:y")),
        ];
        Self { locals, namespaces }
    }

    fn qname(&self, rng: &mut Rng) -> (NameId, Option<NameId>) {
        (rng.pick(&self.locals), rng.pick(&self.namespaces))
    }
}

/// An origin: usually a source span from a small pool, so that unrolled
/// copies (same span) and distinct particles both occur; sometimes none.
fn random_origin(rng: &mut Rng) -> Option<SourceRef> {
    if rng.percent(20) {
        return None;
    }
    let doc = rng.below(2) as u32;
    let start = rng.below(5);
    Some(SourceRef::new(doc, SourceSpan::new(start, start + 1)))
}

fn random_wildcard(rng: &mut Rng, pool: &NamePool) -> NfaTerm {
    let constraint = match rng.below(6) {
        0 => NamespaceConstraint::Any,
        1 => NamespaceConstraint::Other,
        2 => NamespaceConstraint::TargetNamespace,
        3 => NamespaceConstraint::Local,
        4 => NamespaceConstraint::List(vec![rng.pick(&pool.namespaces)]),
        _ => NamespaceConstraint::Not(vec![rng.pick(&pool.namespaces)]),
    };
    let not_qnames = if rng.percent(20) {
        let (local, ns) = pool.qname(rng);
        vec![(ns, local)]
    } else {
        Vec::new()
    };
    NfaTerm::wildcard_with_not_qnames(constraint, ProcessContents::Strict, not_qnames)
}

fn dummy_element_keys(schema_set: &mut SchemaSet, count: usize) -> Vec<ElementKey> {
    let name = schema_set.name_table.add("dummy");
    (0..count)
        .map(|_| {
            schema_set
                .arenas
                .alloc_element(super::tests::element_data(name, None))
        })
        .collect()
}

/// Compare two results by their full `Debug` form (every field of the error:
/// names, namespaces, state, both source locations) and by message.
fn assert_same<T: std::fmt::Debug, E: std::fmt::Debug + std::fmt::Display>(
    what: &str,
    case: u64,
    old: &Result<T, E>,
    new: &Result<T, E>,
) {
    let (old_dbg, new_dbg) = (format!("{old:?}"), format!("{new:?}"));
    assert_eq!(old_dbg, new_dbg, "{what}: case {case} differs");
    if let (Err(o), Err(n)) = (old, new) {
        assert_eq!(o.to_string(), n.to_string(), "{what}: case {case} message");
    }
}

#[test]
fn element_element_check_matches_pairwise_scan() {
    let mut schema_set = SchemaSet::new();
    let pool = NamePool::new(&schema_set);
    let keys = dummy_element_keys(&mut schema_set, 6);
    let mut rng = seed(1);
    let total = cases(40_000);
    let (mut conflicts, mut clean, mut particles) = (0u64, 0u64, 0u64);

    for case in 0..total {
        // Coherent cases give each key one expanded name, as a schema does;
        // free cases do not, to cover any map the check could be handed.
        let coherent = rng.percent(60);
        let own: Vec<(NameId, Option<NameId>)> =
            keys.iter().map(|_| pool.qname(&mut rng)).collect();

        let mut map = SubstitutionGroupMap::default();
        for (k, key) in keys.iter().enumerate() {
            if !rng.percent(60) {
                continue;
            }
            let mut names = SubstitutableNameSet::default();
            if coherent && rng.percent(75) {
                names.insert(own[k]);
            }
            for _ in 0..rng.below(4) {
                names.insert(pool.qname(&mut rng));
            }
            map.insert(*key, names);
        }

        let n = rng.below(13);
        let elements: Vec<ReachableTerm> = (0..n)
            .map(|_| {
                let term = if rng.percent(5) {
                    random_wildcard(&mut rng, &pool)
                } else {
                    let key = if rng.percent(25) {
                        None
                    } else {
                        Some(rng.below(keys.len()))
                    };
                    let (local, ns) = match key {
                        Some(k) if coherent => own[k],
                        _ => pool.qname(&mut rng),
                    };
                    NfaTerm::element(local, ns, key.map(|k| keys[k]))
                };
                ReachableTerm {
                    term,
                    origin: random_origin(&mut rng),
                }
            })
            .collect();
        particles += n as u64;

        let state = rng.below(50) as u32;
        let old = reference::check_element_element_conflicts(&elements, state, &schema_set, &map);
        let new = check_element_element_conflicts(&elements, state, &schema_set, &map);
        assert_same("element-element", case, &old, &new);
        if old.is_err() {
            conflicts += 1;
        } else {
            clean += 1;
        }
    }

    eprintln!(
        "element_element_check_matches_pairwise_scan: {total} cases, {particles} particles, \
         {conflicts} conflicts, {clean} conflict-free, 0 differences"
    );
    assert!(
        conflicts > total / 10 && clean > total / 10,
        "degenerate inputs"
    );
}

/// A generated schema: global element declarations with substitution groups.
struct GeneratedSchema {
    schema_set: SchemaSet,
    pool: NamePool,
    globals: Vec<(ElementKey, NameId, Option<NameId>)>,
}

fn generate_schema(rng: &mut Rng, version: XsdVersion) -> GeneratedSchema {
    let mut schema_set = SchemaSet::with_version(version);
    let pool = NamePool::new(&schema_set);
    let mut globals = Vec::new();
    for _ in 0..rng.below(9) {
        let (local, ns) = pool.qname(rng);
        let mut data = super::tests::element_data(local, ns);
        data.is_abstract = rng.percent(20);
        if rng.percent(10) {
            data.block = DerivationSet::SUBSTITUTION;
        }
        let key = schema_set.arenas.alloc_element(data);
        globals.push((key, local, ns));
    }
    // Substitution-group affiliations: up to two heads (XSD 1.1 lists),
    // chains and the odd cycle (the map builder must still terminate).
    for &(member, _, _) in &globals {
        let heads = match rng.below(10) {
            0..=4 => 0,
            5..=8 => 1,
            _ => 2,
        };
        for _ in 0..heads {
            let head = globals[rng.below(globals.len())].0;
            if head != member || rng.percent(10) {
                schema_set
                    .arenas
                    .entries_mut()
                    .elements
                    .get_mut(member)
                    .unwrap()
                    .resolved_substitution_groups
                    .push(head);
            }
        }
    }
    GeneratedSchema {
        schema_set,
        pool,
        globals,
    }
}

fn random_leaf(rng: &mut Rng, schema: &GeneratedSchema, next_span: &mut usize) -> NfaFragment {
    let term = match rng.below(4) {
        0 | 1 if !schema.globals.is_empty() => {
            let (key, local, ns) = schema.globals[rng.below(schema.globals.len())];
            NfaTerm::element(local, ns, Some(key))
        }
        0..=2 => {
            let (local, ns) = schema.pool.qname(rng);
            NfaTerm::element(local, ns, None)
        }
        _ => random_wildcard(rng, &schema.pool),
    };
    let origin = if rng.percent(10) {
        None
    } else {
        *next_span += 1;
        Some(SourceRef::new(
            0,
            SourceSpan::new(*next_span, *next_span + 1),
        ))
    };
    FragmentBuilder::new().single_term(term, origin)
}

fn random_particle(
    rng: &mut Rng,
    schema: &GeneratedSchema,
    depth: usize,
    next_span: &mut usize,
) -> NfaFragment {
    let fragment = if depth == 0 || rng.percent(45) {
        random_leaf(rng, schema, next_span)
    } else {
        let choice = rng.percent(50);
        let mut group = random_particle(rng, schema, depth - 1, next_span);
        for _ in 0..rng.below(4) {
            let next = random_particle(rng, schema, depth - 1, next_span);
            group = if choice {
                group.alternate(next)
            } else {
                group.concat(next)
            };
        }
        group
    };
    // Occurrence bounds as the UPA compilation produces them (capped at 2,
    // counter-free): copies of one particle share its origin.
    match rng.below(20) {
        0..=9 => fragment,
        10..=12 => fragment.optional(),
        13 | 14 => fragment.repeat_star(),
        15 | 16 => fragment.repeat_plus(),
        17 => fragment.repeat_range(2, Some(2)),
        18 => fragment.repeat_range(0, Some(2)),
        _ => fragment.repeat_range(1, Some(2)),
    }
}

/// Verdict, violation kind and state of a whole-model result. The pair an
/// error names depends, in the reference, on the 0.2.3 epsilon closure's
/// hash-set order, which differs between two calls; that part is compared
/// state by state on a shared closure instead.
fn verdict(result: &SchemaResult<()>) -> String {
    match result {
        Ok(()) => "ok".to_string(),
        Err(error) => {
            let message = error.to_string();
            let kind = if message.contains("UPA violation: elements") {
                "element-element"
            } else if message.contains("conflicts with wildcard") {
                "element-wildcard"
            } else if message.contains("UPA violation: wildcards") {
                "wildcard-wildcard"
            } else {
                "other"
            };
            let state = message
                .split(" at state ")
                .nth(1)
                .and_then(|rest| rest.split(|c: char| !c.is_ascii_digit()).next())
                .unwrap_or("?")
                .to_string();
            format!("{kind} at state {state}")
        }
    }
}

#[test]
fn check_upa_matches_reference_on_random_content_models() {
    let mut rng = seed(2);
    let total = cases(3_000);
    let (mut states, mut state_conflicts, mut models_err, mut models_ok) = (0u64, 0u64, 0u64, 0u64);

    for case in 0..total {
        let version = if case % 2 == 0 {
            XsdVersion::V1_0
        } else {
            XsdVersion::V1_1
        };
        let schema = generate_schema(&mut rng, version);
        let schema_set = &schema.schema_set;
        let mut next_span = 0;
        let nfa = fragment_to_table(random_particle(&mut rng, &schema, 3, &mut next_span));
        let target_namespace = rng.pick(&schema.pool.namespaces);

        let old_map = reference::substitution_map_per_call(schema_set);
        let new_map = upa_substitution_map(schema_set);
        assert_eq!(old_map, new_map, "substitution map: case {case}");

        for state in &nfa.states {
            let reachable = epsilon_closure_with_terms(&nfa, state.id);
            let old = reference::check_element_element_conflicts(
                &reachable.elements,
                state.id,
                schema_set,
                &old_map,
            );
            let new = check_element_element_conflicts(
                &reachable.elements,
                state.id,
                schema_set,
                &new_map,
            );
            assert_same("state", case, &old, &new);
            states += 1;
            state_conflicts += u64::from(old.is_err());
        }

        let old = verdict(&reference::check_upa(&nfa, schema_set, target_namespace));
        let new = verdict(&check_upa(&nfa, schema_set, target_namespace));
        let hoisted = verdict(&check_upa_with_substitutions(
            &nfa,
            schema_set,
            target_namespace,
            &new_map,
        ));
        assert_eq!(old, new, "check_upa: case {case}");
        assert_eq!(old, hoisted, "check_upa_with_substitutions: case {case}");
        if old == "ok" {
            models_ok += 1;
        } else {
            models_err += 1;
        }
    }

    eprintln!(
        "check_upa_matches_reference_on_random_content_models: {total} models \
         ({models_err} violate UPA, {models_ok} satisfy it), {states} states \
         ({state_conflicts} with an element-element conflict), 0 differences"
    );
    assert!(
        models_err > total / 10 && models_ok > total / 10,
        "degenerate inputs"
    );
}

#[test]
fn check_all_group_upa_matches_reference() {
    let mut rng = seed(3);
    let total = cases(10_000);
    let (mut conflicts, mut clean) = (0u64, 0u64);

    for case in 0..total {
        let version = if case % 2 == 0 {
            XsdVersion::V1_0
        } else {
            XsdVersion::V1_1
        };
        let schema = generate_schema(&mut rng, version);
        let schema_set = &schema.schema_set;
        let mut next_span = 0;
        let particles = (0..rng.below(8))
            .map(|_| {
                let leaf = random_leaf(&mut rng, &schema, &mut next_span);
                let state = &leaf.states[leaf.start];
                AllParticle {
                    term: state
                        .term
                        .clone()
                        .expect("a leaf fragment starts at its term"),
                    min_occurs: rng.below(2) as u32,
                    max_occurs: MaxOccurs::Bounded(1),
                    source: state.origin.clone(),
                }
            })
            .collect();
        let model = AllGroupModel {
            particles,
            open_content: None,
            outer_optional: false,
        };
        let target_namespace = rng.pick(&schema.pool.namespaces);

        let old = reference::check_all_group_upa(&model, schema_set, target_namespace);
        let new = check_all_group_upa(&model, schema_set, target_namespace);
        let hoisted = check_all_group_upa_with_substitutions(
            &model,
            schema_set,
            target_namespace,
            &upa_substitution_map(schema_set),
        );
        assert_same("all-group", case, &old, &new);
        assert_same("all-group hoisted", case, &old, &hoisted);
        if old.is_err() {
            conflicts += 1;
        } else {
            clean += 1;
        }
    }

    eprintln!(
        "check_all_group_upa_matches_reference: {total} models, {conflicts} violate UPA, \
         {clean} satisfy it, 0 differences"
    );
    assert!(
        conflicts > total / 10 && clean > total / 10,
        "degenerate inputs"
    );
}

/// The terms of one closure, in order, as comparable strings.
fn term_keys(terms: &[ReachableTerm]) -> Vec<String> {
    terms
        .iter()
        .map(|reachable| format!("{:?} {:?}", reachable.term, reachable.origin))
        .collect()
}

/// The epsilon closure of `start` in increasing state order, computed
/// independently of both implementations.
fn closure_in_state_order(nfa: &NfaTable, start: StateId) -> Vec<StateId> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![start];
    while let Some(state_id) = stack.pop() {
        if !seen.insert(state_id) {
            continue;
        }
        if let Some(state) = nfa.get_state(state_id) {
            for transition in &state.transitions {
                if transition.kind == TransitionKind::Epsilon {
                    stack.push(transition.target);
                }
            }
        }
    }
    seen.into_iter().collect()
}

#[test]
fn epsilon_closure_matches_reference() {
    let mut rng = seed(4);
    let total = cases(3_000);
    let (mut states, mut several) = (0u64, 0u64);
    let sorted = |mut keys: Vec<String>| {
        keys.sort();
        keys
    };

    for case in 0..total {
        let version = if case % 2 == 0 {
            XsdVersion::V1_0
        } else {
            XsdVersion::V1_1
        };
        let schema = generate_schema(&mut rng, version);
        let mut next_span = 0;
        let nfa = fragment_to_table(random_particle(&mut rng, &schema, 3, &mut next_span));
        // One working storage for every state, as `check_upa_with_substitutions`
        // uses it.
        let mut scratch = ClosureScratch::new(&nfa);

        for state in &nfa.states {
            let new = epsilon_closure_with_terms_in(&nfa, state.id, &mut scratch);
            let old = reference::epsilon_closure_with_terms(&nfa, state.id);
            assert_eq!(
                sorted(term_keys(&old.elements)),
                sorted(term_keys(&new.elements)),
                "elements: case {case}, state {}",
                state.id
            );
            assert_eq!(
                sorted(term_keys(&old.wildcards)),
                sorted(term_keys(&new.wildcards)),
                "wildcards: case {case}, state {}",
                state.id
            );

            let in_order = |element: bool| -> Vec<String> {
                closure_in_state_order(&nfa, state.id)
                    .into_iter()
                    .filter_map(|id| nfa.get_state(id))
                    .filter_map(|s| match &s.term {
                        Some(term) if matches!(term, NfaTerm::Element { .. }) == element => {
                            Some(format!("{:?} {:?}", term, s.origin))
                        }
                        _ => None,
                    })
                    .collect()
            };
            assert_eq!(
                in_order(true),
                term_keys(&new.elements),
                "element order: case {case}"
            );
            assert_eq!(
                in_order(false),
                term_keys(&new.wildcards),
                "wildcard order: case {case}"
            );

            let fresh = epsilon_closure_with_terms(&nfa, state.id);
            assert_eq!(term_keys(&fresh.elements), term_keys(&new.elements));
            assert_eq!(term_keys(&fresh.wildcards), term_keys(&new.wildcards));

            states += 1;
            several += u64::from(new.elements.len() + new.wildcards.len() > 1);
        }
    }

    eprintln!(
        "epsilon_closure_matches_reference: {total} models, {states} states \
         ({several} reach two or more terms), 0 differences"
    );
    assert!(several > states / 10, "degenerate inputs");
}
