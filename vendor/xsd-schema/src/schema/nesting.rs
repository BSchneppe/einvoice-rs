//! The nesting limit of the passes over a schema set's components.
//!
//! The schema-time passes — reference resolution, derivation and restriction
//! checks, content-model compilation, the UPA check — recurse along the ways
//! components are nested in each other: a model group in a model group
//! (inline, or through a `<xs:group ref>`), a type in its base type, a list
//! or union type in its item or member types, an attribute group in the
//! attribute groups it references, an element in the head of its
//! substitution group. A schema that nests components deeply
//! enough would overflow the stack of those passes. [`check_nesting_depth`]
//! measures the longest such chain once, without recursion along it, and
//! refuses a schema whose chain is longer than [`SchemaSet::max_depth`],
//! before any pass that recurses along it runs. It also refuses a model group
//! that contains itself, which Structures 1.1 §3.8.6.1 (Model Group Correct)
//! forbids: "2 There are no circular groups. That is, within the {particles}
//! of a group there is no particle at any depth whose {term} is the group
//! itself."
//!
//! Element nesting within one schema document is bounded by the parser
//! (`ParserConfig::max_depth`); this check walks one component's particle
//! tree recursively, which that bound keeps shallow.

use std::collections::HashMap;
use std::hash::BuildHasherDefault;

use ahash::AHasher;

use crate::error::{SchemaError, SchemaResult};
use crate::ids::{
    AttributeGroupKey, ComplexTypeKey, ElementKey, ModelGroupKey, NameId, SimpleTypeKey, TypeKey,
};
use crate::parser::frames::{ComplexContentResult, ParticleResult, ParticleTerm, TypeRefResult};
use crate::parser::location::SourceRef;
use crate::schema::SchemaSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Node {
    Simple(SimpleTypeKey),
    Complex(ComplexTypeKey),
    Group(ModelGroupKey),
    AttributeGroup(AttributeGroupKey),
    Element(ElementKey),
}

#[derive(Debug, Clone, Copy)]
enum State {
    /// On the walk's stack: an edge to it closes a cycle.
    Open,
    Done(usize),
}

/// What one component contributes: its own depth, and the components it
/// nests, each with the depth at which it is nested.
struct Shape {
    local: usize,
    edges: Vec<(Node, usize)>,
}

/// Refuse a schema whose components nest deeper than
/// [`SchemaSet::max_depth`] (no limit when it is `0`), or that has a model
/// group containing itself.
///
/// Runs after inline types are assembled and before references are
/// resolved: named references are looked up by name, as the resolution will
/// resolve them (a redefining component's reference to its own name denotes
/// the component it redefines).
pub(crate) fn check_nesting_depth(schema_set: &SchemaSet) -> SchemaResult<()> {
    let limit = schema_set.max_depth();
    let arenas = &schema_set.arenas;
    let count = arenas.simple_types.len()
        + arenas.complex_types.len()
        + arenas.model_groups.len()
        + arenas.attribute_groups.len();
    let mut states: HashMap<Node, State, BuildHasherDefault<AHasher>> =
        HashMap::with_capacity_and_hasher(count, Default::default());
    let roots: Vec<Node> = schema_set
        .arenas
        .simple_types
        .iter()
        .map(|(k, _)| Node::Simple(k))
        .chain(
            schema_set
                .arenas
                .complex_types
                .iter()
                .map(|(k, _)| Node::Complex(k)),
        )
        .chain(
            schema_set
                .arenas
                .model_groups
                .iter()
                .map(|(k, _)| Node::Group(k)),
        )
        .chain(
            schema_set
                .arenas
                .attribute_groups
                .iter()
                .map(|(k, _)| Node::AttributeGroup(k)),
        )
        .chain(
            schema_set
                .arenas
                .elements
                .iter()
                .filter(|(_, e)| !e.substitution_group.is_empty())
                .map(|(k, _)| Node::Element(k)),
        )
        .collect();

    // An explicit stack: (node, its shape, the next edge, the depth so far).
    let mut stack: Vec<(Node, Shape, usize, usize)> = Vec::new();
    for root in roots {
        if states.contains_key(&root) {
            continue;
        }
        let root_shape = shape(schema_set, root);
        states.insert(root, State::Open);
        let local = root_shape.local;
        stack.push((root, root_shape, 0, local));
        // Whether this walk closed a reference cycle: a chain too deep is
        // then (most likely) a circular definition longer than the limit.
        let mut cycle_seen = false;
        while let Some(top) = stack.last_mut() {
            if let Some(&(target, offset)) = top.1.edges.get(top.2) {
                top.2 += 1;
                match states.get(&target).copied() {
                    Some(State::Done(d)) => top.3 = top.3.max(offset + d),
                    Some(State::Open) => {
                        if let (Node::Group(_), Node::Group(_)) = (top.0, target) {
                            return Err(circular_group(schema_set, target));
                        }
                        // Other cycles (of types, of attribute groups) are
                        // reported by the passes that own them.
                        cycle_seen = true;
                    }
                    None => {
                        let target_shape = shape(schema_set, target);
                        states.insert(target, State::Open);
                        let local = target_shape.local;
                        stack.push((target, target_shape, 0, local));
                    }
                }
                continue;
            }
            let (node, _, _, depth) = stack.pop().expect("non-empty");
            if limit != 0 && depth > limit {
                return Err(too_deep(schema_set, node, depth, limit, cycle_seen));
            }
            states.insert(node, State::Done(depth));
            if let Some(parent) = stack.last_mut() {
                let offset = parent.1.edges[parent.2 - 1].1;
                parent.3 = parent.3.max(offset + depth);
            }
        }
    }
    Ok(())
}

fn shape(schema_set: &SchemaSet, node: Node) -> Shape {
    let mut shape = Shape {
        local: 1,
        edges: Vec::new(),
    };
    match node {
        Node::Simple(key) => {
            let Some(st) = schema_set.arenas.simple_types.get(key) else {
                return shape;
            };
            let own = (st.name, st.target_namespace, st.redefine_original);
            if let Some(target) = type_ref(
                schema_set,
                st.base_type.as_ref(),
                st.resolved_base_type,
                own.0,
                own.1,
                own.2.map(TypeKey::Simple),
            ) {
                shape.edges.push((target, 1));
            }
            if let Some(target) = type_ref(
                schema_set,
                st.item_type.as_ref(),
                st.resolved_item_type,
                None,
                None,
                None,
            ) {
                shape.edges.push((target, 1));
            }
            for (i, member) in st.member_types.iter().enumerate() {
                let resolved = st.resolved_member_types.get(i).copied();
                if let Some(target) = type_ref(schema_set, Some(member), resolved, None, None, None)
                {
                    shape.edges.push((target, 1));
                }
            }
        }
        Node::Complex(key) => {
            let Some(ct) = schema_set.arenas.complex_types.get(key) else {
                return shape;
            };
            let own_original = ct.redefine_original.map(TypeKey::Complex);
            let (content_base, content_groups, particle) = match &ct.content {
                ComplexContentResult::Complex(def) => (
                    def.base_type.as_ref(),
                    def.attribute_groups.as_slice(),
                    def.particle.as_ref(),
                ),
                ComplexContentResult::Simple(def) => (
                    def.base_type.as_ref(),
                    def.attribute_groups.as_slice(),
                    None,
                ),
                ComplexContentResult::Empty => (None, &[][..], None),
            };
            let base = ct.base_type.as_ref().or(content_base);
            if let Some(target) = type_ref(
                schema_set,
                base,
                ct.resolved_base_type,
                ct.name,
                ct.target_namespace,
                own_original,
            ) {
                shape.edges.push((target, 1));
            }
            if let Some(TypeKey::Simple(inline)) = ct.resolved_simple_content_type {
                shape.edges.push((Node::Simple(inline), 1));
            }
            for group in ct.attribute_groups.iter().chain(content_groups) {
                if let Some(target) =
                    schema_set.lookup_attribute_group(group.namespace, group.local_name)
                {
                    shape.edges.push((Node::AttributeGroup(target), 1));
                }
            }
            if let Some(particle) = particle {
                particle_shape(schema_set, particle, 1, None, &mut shape);
            }
        }
        Node::Group(key) => {
            let Some(group) = schema_set.arenas.model_groups.get(key) else {
                return shape;
            };
            let redirect = match (group.name, group.redefine_original) {
                (Some(name), Some(original)) => Some((name, group.target_namespace, original)),
                _ => None,
            };
            for particle in &group.particles {
                particle_shape(schema_set, particle, 1, redirect, &mut shape);
            }
        }
        Node::Element(key) => {
            // A substitution group member nests in its heads (the passes
            // that close substitution groups walk the chain).
            let Some(element) = schema_set.arenas.elements.get(key) else {
                return shape;
            };
            for head in &element.substitution_group {
                if let Some(target) = schema_set.lookup_element(head.namespace, head.local_name) {
                    shape.edges.push((Node::Element(target), 1));
                }
            }
        }
        Node::AttributeGroup(key) => {
            let Some(group) = schema_set.arenas.attribute_groups.get(key) else {
                return shape;
            };
            for reference in group.attribute_groups.iter().chain(group.ref_name.as_ref()) {
                let target = match (group.name, group.redefine_original) {
                    (Some(name), Some(original))
                        if reference.local_name == name
                            && reference.namespace == group.target_namespace =>
                    {
                        Some(original)
                    }
                    _ => {
                        schema_set.lookup_attribute_group(reference.namespace, reference.local_name)
                    }
                };
                if let Some(target) = target {
                    shape.edges.push((Node::AttributeGroup(target), 1));
                }
            }
        }
    }
    shape
}

/// The particle tree of one component: each inline model group is one level
/// deeper; a group reference nests the referenced group at its level.
fn particle_shape(
    schema_set: &SchemaSet,
    particle: &ParticleResult,
    level: usize,
    redirect: Option<(NameId, Option<NameId>, ModelGroupKey)>,
    shape: &mut Shape,
) {
    shape.local = shape.local.max(level);
    let ParticleTerm::Group(group) = &particle.term else {
        return;
    };
    if let Some(reference) = &group.ref_name {
        let target = match redirect {
            Some((name, ns, original))
                if reference.local_name == name && reference.namespace == ns =>
            {
                Some(original)
            }
            _ => schema_set.lookup_model_group(reference.namespace, reference.local_name),
        };
        if let Some(target) = target {
            shape.edges.push((Node::Group(target), level));
        }
        return;
    }
    shape.local = shape.local.max(level + 1);
    for child in &group.particles {
        particle_shape(schema_set, child, level + 1, redirect, shape);
    }
}

/// The component a type reference denotes: the resolved key when the
/// reference is already resolved (inline types), else the named type, with a
/// redefining type's reference to its own name redirected to the type it
/// redefines.
fn type_ref(
    schema_set: &SchemaSet,
    reference: Option<&TypeRefResult>,
    resolved: Option<TypeKey>,
    own_name: Option<NameId>,
    own_namespace: Option<NameId>,
    own_original: Option<TypeKey>,
) -> Option<Node> {
    let key = match (resolved, reference) {
        (Some(key), _) => Some(key),
        (None, Some(TypeRefResult::QName(name))) => match (own_name, own_original) {
            (Some(own), Some(original))
                if name.local_name == own && name.namespace == own_namespace =>
            {
                Some(original)
            }
            _ => schema_set.lookup_type(name.namespace, name.local_name),
        },
        _ => None,
    }?;
    Some(match key {
        TypeKey::Simple(k) => Node::Simple(k),
        TypeKey::Complex(k) => Node::Complex(k),
    })
}

fn source_of(schema_set: &SchemaSet, node: Node) -> Option<&SourceRef> {
    match node {
        Node::Simple(k) => schema_set.arenas.simple_types.get(k)?.source.as_ref(),
        Node::Complex(k) => schema_set.arenas.complex_types.get(k)?.source.as_ref(),
        Node::Group(k) => schema_set.arenas.model_groups.get(k)?.source.as_ref(),
        Node::AttributeGroup(k) => schema_set.arenas.attribute_groups.get(k)?.source.as_ref(),
        Node::Element(k) => schema_set.arenas.elements.get(k)?.source.as_ref(),
    }
}

fn describe(schema_set: &SchemaSet, node: Node) -> String {
    let (kind, name, ns) = match node {
        Node::Simple(k) => {
            let d = schema_set.arenas.simple_types.get(k);
            (
                "simple type",
                d.and_then(|d| d.name),
                d.and_then(|d| d.target_namespace),
            )
        }
        Node::Complex(k) => {
            let d = schema_set.arenas.complex_types.get(k);
            (
                "complex type",
                d.and_then(|d| d.name),
                d.and_then(|d| d.target_namespace),
            )
        }
        Node::Group(k) => {
            let d = schema_set.arenas.model_groups.get(k);
            (
                "model group",
                d.and_then(|d| d.name),
                d.and_then(|d| d.target_namespace),
            )
        }
        Node::AttributeGroup(k) => {
            let d = schema_set.arenas.attribute_groups.get(k);
            (
                "attribute group",
                d.and_then(|d| d.name),
                d.and_then(|d| d.target_namespace),
            )
        }
        Node::Element(k) => {
            let d = schema_set.arenas.elements.get(k);
            (
                "element",
                d.and_then(|d| d.name),
                d.and_then(|d| d.target_namespace),
            )
        }
    };
    match name {
        Some(name) => format!(
            "{kind} '{}'",
            crate::schema::resolver::format_resolved_qname(&schema_set.name_table, ns, name)
        ),
        None => format!("an anonymous {kind}"),
    }
}

fn too_deep(
    schema_set: &SchemaSet,
    node: Node,
    depth: usize,
    limit: usize,
    cycle_seen: bool,
) -> SchemaError {
    let what = describe(schema_set, node);
    let message = if cycle_seen {
        format!(
            "{what} is part of a circular definition: it nests components {depth} levels \
             deep through a reference cycle (base, item or member types, attribute groups, \
             substitution group heads)"
        )
    } else {
        format!(
            "{what} nests components {depth} levels deep (model groups, base, item and member \
             types, attribute groups, substitution group heads)"
        )
    };
    SchemaError::limit(
        crate::error::SchemaLimit::ComponentDepth,
        limit,
        depth,
        message,
        schema_set.locate(source_of(schema_set, node)),
    )
}

fn circular_group(schema_set: &SchemaSet, node: Node) -> SchemaError {
    SchemaError::structural(
        "mg-props-correct.2",
        format!(
            "{} contains itself; Model Group Correct (§3.8.6.1) clause 2: \"There are no \
             circular groups. That is, within the {{particles}} of a group there is no \
             particle at any depth whose {{term}} is the group itself.\"",
            describe(schema_set, node)
        ),
        schema_set.locate(source_of(schema_set, node)),
    )
}
