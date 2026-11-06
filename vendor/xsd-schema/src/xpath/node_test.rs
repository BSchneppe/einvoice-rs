//! XPath node test matching helpers.
//!
//! Provides a unified node test type that can be used by axis iterators
//! and type-based filters, aligning with `XPATH_ITERATOR_PORT_PLAN.md`.

use crate::ids::{NameId, TypeKey};
use crate::namespace::qname::QualifiedName;
use crate::types::sequence::TypeTest;
use crate::types::{ItemType, NameTest, SequenceType, XmlTypeCode};
use crate::xpath::ast::{Axis, ItemTypeNode, KindTest};
use crate::xpath::error::XPathError;
use crate::xpath::iterator::XmlItem;

use super::context::XPathContext;
use super::{DomNavigator, DomNodeType};

/// The principal node kind of `axis` (XPath 2.0 §3.2.1.1).
///
/// "If an axis can contain elements, then the principal node kind is element;
/// otherwise, it is the kind of nodes that the axis can contain." Thus it is
/// attribute for the `attribute::` axis, namespace for `namespace::`, and
/// element for every other axis.
pub fn principal_node_kind(axis: Axis) -> DomNodeType {
    match axis {
        Axis::Attribute => DomNodeType::Attribute,
        Axis::Namespace => DomNodeType::Namespace,
        _ => DomNodeType::Element,
    }
}

/// Build the runtime node test for a **name test** used on an axis whose
/// principal node kind is `principal`.
///
/// XPath 2.0 §3.2.1.2: "A name test is true if and only if the kind of the
/// node is the principal node kind for the step axis and the expanded QName
/// of the node is equal (as defined by the `eq` operator) to the expanded
/// QName specified by the name test." Without this restriction `self::*` and
/// `ancestor-or-self::*` would select attribute nodes, which are never of the
/// principal node kind of those axes.
pub fn name_test_for_principal_kind(test: NameTest, principal: DomNodeType) -> NodeTest {
    match principal {
        DomNodeType::Attribute => {
            NodeTest::Type(SequenceType::one(ItemType::attribute(Some(test), None)))
        }
        // A namespace node's name is its prefix, and it is the only kind the
        // `namespace::` axis yields; `matches_name_test` applies that rule.
        DomNodeType::Namespace => NodeTest::Name(test),
        _ => NodeTest::Type(SequenceType::one(ItemType::element(
            Some(test),
            None,
            false,
        ))),
    }
}

/// Unified node test for axis iterators.
#[derive(Debug, Clone)]
pub enum NodeTest {
    /// Name test (`*`, `*:local`, `prefix:*`, or QName).
    Name(NameTest),
    /// Sequence type test (`node()`, `element(...)`, etc.).
    Type(SequenceType),
}

impl NodeTest {
    pub fn matches<N: DomNavigator>(&self, nav: &N, ctx: &XPathContext<'_>) -> bool {
        match self {
            NodeTest::Name(test) => matches_name_test(test, nav, ctx),
            NodeTest::Type(seq) => matches_sequence_type(seq, nav, ctx),
        }
    }
}

pub fn matches_name_test<N: DomNavigator>(
    test: &NameTest,
    nav: &N,
    ctx: &XPathContext<'_>,
) -> bool {
    // A namespace node has a name too: its prefix, in no namespace (XDM §6.4).
    // A name test on the `namespace::` axis therefore selects by prefix, and
    // `*` selects every namespace node — the principal node kind of that axis
    // is namespace, not element.
    if nav.node_type() == DomNodeType::Namespace {
        return matches_namespace_name_test(test, nav, ctx);
    }
    if nav.node_type() != DomNodeType::Element && nav.node_type() != DomNodeType::Attribute {
        return false;
    }

    match test {
        NameTest::Wildcard => true,
        NameTest::NamespaceWildcard(local_id) => {
            // *:local - match any namespace with specific local name
            match ctx.names.try_resolve_ref(*local_id) {
                Some(local) => nav.local_name() == local,
                None => false,
            }
        }
        NameTest::LocalWildcard(ns_id) => {
            // prefix:* - match any local name in specific namespace
            match ctx.names.try_resolve_ref(*ns_id) {
                Some(ns) => nav.namespace_uri() == ns,
                None => false,
            }
        }
        NameTest::QName(qname) => qname_matches(qname, nav, ctx),
    }
}

/// A name test applied to a namespace node, whose name is its prefix.
fn matches_namespace_name_test<N: DomNavigator>(
    test: &NameTest,
    nav: &N,
    ctx: &XPathContext<'_>,
) -> bool {
    match test {
        NameTest::Wildcard => true,
        // `*:local` — a namespace node is in no namespace, so this is just the
        // prefix test.
        NameTest::NamespaceWildcard(local_id) => match ctx.names.try_resolve_ref(*local_id) {
            Some(local) => nav.local_name() == local,
            None => false,
        },
        // `prefix:*` — no namespace node is in a namespace.
        NameTest::LocalWildcard(_) => false,
        // A QName selects the namespace node whose prefix it is; a prefixed
        // name never matches, because the name is in no namespace.
        NameTest::QName(qname) => match ctx.names.try_resolve_ref(qname.local_name) {
            Some(local) => qname.namespace_uri.is_none() && nav.local_name() == local,
            None => false,
        },
    }
}

pub fn matches_sequence_type<N: DomNavigator>(
    sequence: &SequenceType,
    nav: &N,
    ctx: &XPathContext<'_>,
) -> bool {
    matches_item_type(&sequence.item_type, nav, ctx)
}

/// The node matcher of every [`ItemType`] — XPath 2.0 §2.5.4.2 – §2.5.4.6. A
/// step node test, `instance of`, `treat as` and [`ItemType::matches_item`]
/// all run it.
pub(crate) fn matches_item_type<N: DomNavigator>(
    item_type: &ItemType,
    nav: &N,
    ctx: &XPathContext<'_>,
) -> bool {
    match item_type {
        ItemType::AnyItem | ItemType::AnyNode => true,
        ItemType::Document(None) => nav.node_type() == DomNodeType::Root,
        ItemType::Document(Some(inner)) => match_document_with_inner(inner, nav, ctx),
        ItemType::Element {
            name,
            type_test,
            nillable,
        } => matches_element_test(nav, name.as_ref(), type_test.as_ref(), *nillable, ctx),
        ItemType::Attribute { name, type_test } => {
            matches_attribute_test(nav, name.as_ref(), type_test.as_ref(), ctx)
        }
        ItemType::SchemaElement(name) => matches_schema_element(nav, name, ctx),
        ItemType::SchemaAttribute(name) => matches_schema_attribute(nav, name, ctx),
        ItemType::Text => nav.node_type().is_text_like(),
        ItemType::Comment => nav.node_type() == DomNodeType::Comment,
        ItemType::ProcessingInstruction(target) => {
            nav.node_type() == DomNodeType::ProcessingInstruction
                && target.as_ref().is_none_or(|name| nav.local_name() == name)
        }
        ItemType::NamespaceNode => nav.node_type() == DomNodeType::Namespace,
        ItemType::AtomicType(_) | ItemType::SchemaAtomicType(_) => false,
    }
}

/// `element()`, `element(N)`, `element(N, T)` and `element(N, T?)` — XPath 2.0
/// §2.5.4.3, the one implementation every spelling of the test runs.
///
/// > `element(ElementName, TypeName)` matches an element node whose name is
/// > ElementName if derives-from(AT, TypeName) is true, where AT is the type
/// > annotation of the element node, and the nilled property of the node is
/// > false.
///
/// and with `TypeName ?`, "The nilled property of the node may be either true
/// or false". Without a `TypeName` the test matches "regardless of its type
/// annotation or nilled property". "Note that substitution groups do not
/// affect the semantics of ElementTest": the name is compared as it is.
pub(crate) fn matches_element_test<N: DomNavigator>(
    nav: &N,
    name: Option<&NameTest>,
    type_test: Option<&TypeTest>,
    nillable: bool,
    ctx: &XPathContext<'_>,
) -> bool {
    if nav.node_type() != DomNodeType::Element {
        return false;
    }
    if let Some(test) = name {
        if !matches_name_test(test, nav, ctx) {
            return false;
        }
    }
    let Some(type_test) = type_test else {
        return true;
    };
    if !nillable && matches!(nav.typed_value(), crate::xpath::TypedValue::Nilled) {
        return false;
    }
    derives_from_type_test(nav, type_test, true, ctx)
}

/// `attribute()`, `attribute(N)` and `attribute(N, T)` — XPath 2.0 §2.5.4.5:
/// "`attribute(AttributeName, TypeName)` matches an attribute node whose name
/// is AttributeName if derives-from(AT, TypeName ) is true, where AT is the
/// type annotation of the attribute node." An attribute has no nilled
/// property.
pub(crate) fn matches_attribute_test<N: DomNavigator>(
    nav: &N,
    name: Option<&NameTest>,
    type_test: Option<&TypeTest>,
    ctx: &XPathContext<'_>,
) -> bool {
    if nav.node_type() != DomNodeType::Attribute {
        return false;
    }
    if let Some(test) = name {
        if !matches_name_test(test, nav, ctx) {
            return false;
        }
    }
    match type_test {
        Some(type_test) => derives_from_type_test(nav, type_test, false, ctx),
        None => true,
    }
}

/// `derives-from(AT, ET)` (XPath 2.0 §2.5.4) for an element (`is_element`) or
/// attribute node and the expected type of a [`TypeTest`].
///
/// `AT` is [`DomNavigator::type_annotation`], so a **complex** annotation takes
/// part. A node with no annotation is untyped: its `AT` is `xs:untyped` for an
/// element and `xs:untypedAtomic` for an attribute (§2.5.1), and those derive
/// from nothing but `xs:anyType` — and, for `xs:untypedAtomic`,
/// `xs:anyAtomicType` and `xs:anySimpleType` — so `element(*, xs:integer)` does
/// not match an untyped element. An annotated node is never untyped.
///
/// An annotated node needs the schema set its annotation came from to answer
/// for a type it names by code; without one in the static context the match
/// fails (the "unknown type" case of §2.5.4, which leaves the answer
/// implementation-dependent).
fn derives_from_type_test<N: DomNavigator>(
    nav: &N,
    type_test: &TypeTest,
    is_element: bool,
    ctx: &XPathContext<'_>,
) -> bool {
    let Some(actual) = nav.type_annotation() else {
        // The node is untyped; which predefined type does ET stand for?
        let code = match type_test {
            TypeTest::Untyped => return is_element,
            TypeTest::BuiltIn(code) => *code,
            TypeTest::Schema(key) => match ctx.schema_set.and_then(|s| built_in_code(s, *key)) {
                Some(code) => code,
                // A user-defined type: nothing untyped derives from it.
                None => return false,
            },
        };
        return if is_element {
            code == XmlTypeCode::AnyType
        } else {
            matches!(
                code,
                XmlTypeCode::UntypedAtomic
                    | XmlTypeCode::AnyAtomicType
                    | XmlTypeCode::AnySimpleType
                    | XmlTypeCode::AnyType
            )
        };
    };
    match type_test {
        TypeTest::Untyped => false,
        TypeTest::Schema(expected) => match ctx.schema_set {
            Some(schema_set) => schema_set.xpath_derives_from(actual, *expected),
            // The key and the annotation come from the same schema set, which
            // the static context does not name: only the same type matches.
            None => actual == *expected,
        },
        TypeTest::BuiltIn(code) => {
            let Some(schema_set) = ctx.schema_set else {
                return false;
            };
            match built_in_key(schema_set, *code) {
                // Every set has the types of the data model, an XSD 1.0 set
                // too (`SchemaSet::xpath_type`), with their places in its
                // hierarchy: every atomic type derives from
                // `xs:anyAtomicType` ("xs:anyAtomicType is an atomic type that
                // includes all atomic values", §2.5.1).
                Some(expected) => schema_set.xpath_derives_from(actual, expected),
                // A predefined type the set does not have (`xs:dateTimeStamp`
                // in an XSD 1.0 set): no node of the set has it.
                None => false,
            }
        }
    }
}

/// Whether `key` is an atomic simple type: atomic variety, and not
/// `xs:anySimpleType` itself. A restriction of a list or union type inherits
/// its base's variety when references are resolved.
pub(crate) fn is_atomic_simple_type(schema_set: &crate::schema::SchemaSet, key: TypeKey) -> bool {
    let TypeKey::Simple(simple) = key else {
        return false;
    };
    if simple == schema_set.builtin_types().any_simple_type {
        return false;
    }
    schema_set
        .arenas
        .simple_types
        .get(simple)
        .is_some_and(|def| def.variety == crate::parser::frames::SimpleTypeVariety::Atomic)
}

/// The schema set's key for the predefined type with this code — the data
/// model's types included in both XSD versions ([`SchemaSet::xpath_type`](crate::schema::SchemaSet::xpath_type)).
fn built_in_key(schema_set: &crate::schema::SchemaSet, code: XmlTypeCode) -> Option<TypeKey> {
    schema_set.xpath_type(code)
}

/// The code of a predefined type, if `key` is one in this schema set.
fn built_in_code(schema_set: &crate::schema::SchemaSet, key: TypeKey) -> Option<XmlTypeCode> {
    match key {
        TypeKey::Complex(_) if schema_set.is_any_type(key) => Some(XmlTypeCode::AnyType),
        TypeKey::Complex(_) => None,
        TypeKey::Simple(simple) => schema_set.get_type_code(simple),
    }
}

/// The runtime [`ItemType`] of a kind test from the AST, with its names and
/// type expanded against the static context — what a step node test, `instance
/// of` and `treat as` all match with.
///
/// The names follow §2.5.4: "QNames appearing in a sequence type have their
/// prefixes expanded to namespace URIs by means of the statically known
/// namespaces and (where applicable) the default element/type namespace. An
/// unprefixed attribute QName is in no namespace." A `TypeName` is an element
/// or type name, so the default element/type namespace applies to it too; a
/// name in the `xs` namespace becomes [`TypeTest::Untyped`] or
/// [`TypeTest::BuiltIn`], any other name the [`TypeTest::Schema`] key of the
/// schema set in the static context.
///
/// The binder has already rejected a prefix the static context cannot expand
/// (`XPST0081`) and a `TypeName` outside the in-scope schema types
/// (`XPST0008`); an expression tree that was never bound raises the same
/// errors here.
pub(crate) fn item_type_of_kind_test(
    kind: &KindTest,
    ctx: &XPathContext<'_>,
) -> Result<ItemType, XPathError> {
    Ok(match kind {
        KindTest::AnyKind => ItemType::AnyNode,
        KindTest::Text => ItemType::Text,
        KindTest::Comment => ItemType::Comment,
        KindTest::ProcessingInstruction(target) => ItemType::ProcessingInstruction(target.clone()),
        KindTest::Document(inner) => ItemType::Document(match inner {
            Some(inner) => Some(Box::new(item_type_of_kind_test(inner, ctx)?)),
            None => None,
        }),
        KindTest::Element(test) => ItemType::element(
            kind_test_name(test.name.as_ref(), ctx, DomNodeType::Element)?,
            match &test.type_name {
                Some(type_name) => Some(resolve_type_test(type_name, ctx)?),
                None => None,
            },
            test.nillable,
        ),
        KindTest::Attribute(test) => ItemType::attribute(
            kind_test_name(test.name.as_ref(), ctx, DomNodeType::Attribute)?,
            match &test.type_name {
                Some(type_name) => Some(resolve_type_test(type_name, ctx)?),
                None => None,
            },
        ),
        // A name that is not a lexical QName cannot come out of the parser; if
        // one did, it is interned whole as the local name, which no node can
        // have, so the test matches nothing instead of everything.
        KindTest::SchemaElement(name) => ItemType::SchemaElement(
            resolve_schema_test_name(name, ctx, DomNodeType::Element)
                .unwrap_or_else(|| QualifiedName::local(ctx.names.add(name))),
        ),
        KindTest::SchemaAttribute(name) => ItemType::SchemaAttribute(
            resolve_schema_test_name(name, ctx, DomNodeType::Attribute)
                .unwrap_or_else(|| QualifiedName::local(ctx.names.add(name))),
        ),
    })
}

/// The namespace of a name in a kind test: the prefix's, or for no prefix the
/// default element/type namespace on an element name and none on an attribute
/// name (§2.5.4).
fn kind_test_name_ns(
    prefix: &str,
    ctx: &XPathContext<'_>,
    principal: DomNodeType,
) -> Result<Option<NameId>, XPathError> {
    if prefix.is_empty() {
        return Ok(if principal == DomNodeType::Element {
            ctx.default_element_ns
        } else {
            None
        });
    }
    match ctx.resolve_prefix(prefix) {
        Some(uri) => Ok(Some(ctx.names.add(&uri))),
        None => Err(XPathError::undefined_prefix(prefix)),
    }
}

/// The `ElementName` / `AttributeName` of a kind test as a runtime name test.
fn kind_test_name(
    name: Option<&crate::xpath::ast::QName>,
    ctx: &XPathContext<'_>,
    principal: DomNodeType,
) -> Result<Option<NameTest>, XPathError> {
    let Some(name) = name else {
        return Ok(None);
    };
    let ns = kind_test_name_ns(&name.prefix, ctx, principal)?;
    Ok(Some(NameTest::QName(QualifiedName::new(
        ns,
        ctx.names.add(&name.local),
        None,
    ))))
}

/// The `TypeName` of a kind test as a [`TypeTest`].
fn resolve_type_test(
    type_name: &crate::xpath::ast::QName,
    ctx: &XPathContext<'_>,
) -> Result<TypeTest, XPathError> {
    use crate::namespace::table::well_known;

    let ns = kind_test_name_ns(&type_name.prefix, ctx, DomNodeType::Element)?;
    let undefined = || XPathError::undefined_qname(type_name.to_string());
    if ns == Some(well_known::XS_NAMESPACE) {
        if type_name.local == "untyped" {
            return Ok(TypeTest::Untyped);
        }
        return XmlTypeCode::from_local_name(&type_name.local)
            .map(TypeTest::BuiltIn)
            .ok_or_else(undefined);
    }
    let schema_set = ctx.schema_set.ok_or_else(undefined)?;
    schema_set
        .lookup_type(ns, ctx.names.add(&type_name.local))
        .map(TypeTest::Schema)
        .ok_or_else(undefined)
}

/// `document-node(E)` — XPath 2.0 §2.5.4.2: "document-node(E) matches any
/// document node that contains exactly one element node, optionally
/// accompanied by one or more comment and processing instruction nodes, if E
/// is an ElementTest or SchemaElementTest that matches the element node."
///
/// So a second element child, or a text node among the children, rules the
/// document out whatever `E` says.
fn match_document_with_inner<N: DomNavigator>(
    inner: &ItemType,
    nav: &N,
    ctx: &XPathContext<'_>,
) -> bool {
    if nav.node_type() != DomNodeType::Root {
        return false;
    }

    let mut cursor = nav.clone();
    if !cursor.move_to_first_child() {
        return false;
    }

    let mut element: Option<N> = None;
    loop {
        match cursor.node_type() {
            DomNodeType::Element => {
                if element.is_some() {
                    return false;
                }
                element = Some(cursor.clone());
            }
            DomNodeType::Comment | DomNodeType::ProcessingInstruction => {}
            _ => return false,
        }
        if !cursor.move_to_next_sibling() {
            break;
        }
    }

    element.is_some_and(|element| matches_item_type(inner, &element, ctx))
}

/// Whether `nav`'s expanded name is `qname` — compared without allocating, as
/// this runs once per candidate node.
fn qname_matches<N: DomNavigator>(qname: &QualifiedName, nav: &N, ctx: &XPathContext<'_>) -> bool {
    let Some(local) = ctx.names.try_resolve_ref(qname.local_name) else {
        return false;
    };
    let ns = match qname.namespace_uri {
        Some(id) => match ctx.names.try_resolve_ref(id) {
            Some(ns) => ns,
            None => return false,
        },
        None => "",
    };

    nav.local_name() == local && nav.namespace_uri() == ns
}

// ============================================================================
// schema-element(N) / schema-attribute(N)
// ============================================================================

/// `schema-element(N)` matching — XPath 2.0 §2.5.4.4.
///
/// This is the **single** implementation of the rule. Every spelling of the
/// test routes here: `ItemType::SchemaElement` (a step node test, and the
/// `SequenceType` form after resolution), `KindTest::SchemaElement` (the AST
/// form used by `instance of` / `treat as`) and `ItemType::matches_node`.
///
/// > A `SchemaElementTest` matches a candidate element node if all three of the
/// > following conditions are satisfied:
/// >
/// > 1. The name of the candidate node matches the specified `ElementName` or
/// >    matches the name of an element in a substitution group headed by an
/// >    element named `ElementName`.
/// > 2. `derives-from(AT, ET)` is true, where `AT` is the type annotation of the
/// >    candidate node and `ET` is the schema type declared for element
/// >    `ElementName` in the in-scope element declarations.
/// > 3. If the element declaration for `ElementName` in the in-scope element
/// >    declarations is not nillable, then the nilled property of the candidate
/// >    node is false.
///
/// §2.5.4.4 also makes an `ElementName` that is *not* in the in-scope element
/// declarations a static error (`XPST0008`). The binder raises it when the
/// expression is compiled — also when the static context has no schema set, and
/// so no declarations at all — so a compiled expression reaches this matcher
/// only with a declared name. An [`ItemType::SchemaElement`] a host builds
/// itself is not checked: an undeclared name matches nothing, and without a
/// schema set in the context the test degrades to a name-only match.
pub(crate) fn matches_schema_element<N: DomNavigator>(
    nav: &N,
    name: &QualifiedName,
    ctx: &XPathContext<'_>,
) -> bool {
    if nav.node_type() != DomNodeType::Element {
        return false;
    }
    let Some(schema_set) = ctx.schema_set else {
        // No in-scope element declarations: name-only match.
        return qname_matches(name, nav, ctx);
    };
    let Some(head_key) = schema_set.lookup_element(name.namespace_uri, name.local_name) else {
        // `ElementName` is not declared; nothing matches it.
        return false;
    };
    let Some(head) = schema_set.arenas.elements.get(head_key) else {
        return false;
    };

    // Clause 1: the candidate's own name, or a member of the group `name` heads.
    if !qname_matches(name, nav, ctx) {
        let Some(candidate_key) = lookup_node_element_decl(nav, schema_set) else {
            return false;
        };
        if !crate::compiler::substitution::is_substitution_group_member(
            schema_set,
            head_key,
            candidate_key,
        ) {
            return false;
        }
    }

    // Clause 3: only a nillable declaration accepts a nilled candidate.
    if !head.nillable && matches!(nav.typed_value(), crate::xpath::TypedValue::Nilled) {
        return false;
    }

    // Clause 2: derives-from(AT, ET).
    derives_from_declared_type(nav, head.resolved_type, schema_set)
}

/// `schema-attribute(N)` matching — XPath 2.0 §2.5.4.6.
///
/// > A `SchemaAttributeTest` matches a candidate attribute node if both of the
/// > following conditions are satisfied:
/// >
/// > 1. The name of the candidate node matches the specified `AttributeName`.
/// > 2. `derives-from(AT, ET)` is true, where `AT` is the type annotation of the
/// >    candidate node and `ET` is the schema type declared for attribute
/// >    `AttributeName` in the in-scope attribute declarations.
///
/// There is no substitution group for attributes, and no nilled property, so
/// the rule is the element one without clauses 1's second half and 3. The
/// `XPST0008` and no-schema-set remarks of [`matches_schema_element`] apply
/// unchanged.
pub(crate) fn matches_schema_attribute<N: DomNavigator>(
    nav: &N,
    name: &QualifiedName,
    ctx: &XPathContext<'_>,
) -> bool {
    if nav.node_type() != DomNodeType::Attribute {
        return false;
    }
    if !qname_matches(name, nav, ctx) {
        return false;
    }
    let Some(schema_set) = ctx.schema_set else {
        return true;
    };
    let Some(attr_key) = schema_set.lookup_attribute(name.namespace_uri, name.local_name) else {
        return false;
    };
    let Some(attr) = schema_set.arenas.attributes.get(attr_key) else {
        return false;
    };
    derives_from_declared_type(nav, attr.resolved_type, schema_set)
}

/// Expand the lexical `ElementName` / `AttributeName` of a
/// `schema-element(N)` / `schema-attribute(N)` test into the interned
/// [`QualifiedName`] the matcher takes.
///
/// The namespace rule is the one §3.2.1.2 gives a name test on an axis of that
/// principal node kind: an unprefixed `ElementName` picks up the default
/// element namespace, while an unprefixed `AttributeName` is in no namespace.
/// A prefix that the static context cannot expand cannot occur — the binder
/// rejects it with `XPST0081` — but the name is re-resolved here rather than
/// carried, so an unexpandable one yields `None` and matches nothing.
///
/// Returns `None` for a name that is not a lexical QName at all.
pub(crate) fn resolve_schema_test_name(
    name: &str,
    ctx: &XPathContext<'_>,
    principal: DomNodeType,
) -> Option<QualifiedName> {
    let (prefix, local) = crate::xpath::functions::qname::parse_lexical_qname(name).ok()?;
    let ns_id = match &prefix {
        Some(prefix) => Some(ctx.names.add(&ctx.resolve_prefix(prefix)?)),
        // An unprefixed attribute name is always in no namespace.
        None if principal == DomNodeType::Attribute => None,
        None => ctx.default_element_ns,
    };
    Some(QualifiedName::new(ns_id, ctx.names.add(&local), None))
}

/// The global element declaration whose expanded name is the one `nav` carries,
/// if the schema set has one.
fn lookup_node_element_decl<N: DomNavigator>(
    nav: &N,
    schema_set: &crate::schema::SchemaSet,
) -> Option<crate::ids::ElementKey> {
    let local_id = schema_set.name_table.get(nav.local_name())?;
    let ns = nav.namespace_uri();
    let ns_id = if ns.is_empty() {
        None
    } else {
        Some(schema_set.name_table.get(ns)?)
    };
    schema_set.lookup_element(ns_id, local_id)
}

/// `derives-from(AT, ET)` for a candidate node and the type a declaration
/// declares, where `None` for `ET` means the declaration constrains no type.
///
/// `AT` is read from [`DomNavigator::type_annotation`], not from
/// [`DomNavigator::schema_type`], so a node annotated with a **complex** type
/// participates: the declared type lives in the schema set here, not in a
/// `SimpleTypeKey` slot of the [`ItemType`], so nothing restricts it to the
/// simple types. A node with no annotation at all cannot derive from a declared
/// type and never matches.
fn derives_from_declared_type<N: DomNavigator>(
    nav: &N,
    declared: Option<TypeKey>,
    schema_set: &crate::schema::SchemaSet,
) -> bool {
    let Some(expected) = declared else {
        // The declaration constrains no type: the name match is the whole test.
        return true;
    };
    let Some(actual) = nav.type_annotation() else {
        return false;
    };
    schema_set.xpath_derives_from(actual, expected)
}

// ============================================================================
// AST KindTest and ItemTypeNode Matching
// ============================================================================

/// Check if an XmlItem matches an AST ItemTypeNode.
///
/// This is used for `instance of` and `treat as` expressions to check
/// if a value matches the target type specification.
///
/// # Arguments
///
/// * `item` - The item to check (node or atomic value)
/// * `item_type` - The AST item type node to match against
/// * `resolved_atomic_type` - The resolved QualifiedName for atomic types (from binding)
/// * `ctx` - The XPath context for name resolution
///
/// # Returns
///
/// `true` if the item matches the item type, `false` otherwise.
pub fn matches_item_type_node<N: DomNavigator>(
    item: &XmlItem<N>,
    item_type: &ItemTypeNode,
    resolved_atomic_type: Option<&QualifiedName>,
    ctx: &XPathContext<'_>,
) -> bool {
    match item_type_of_item_type_node(item_type, resolved_atomic_type, ctx) {
        Ok(item_type) => item_type.matches_item(item, ctx),
        Err(_) => false,
    }
}

/// The runtime [`ItemType`] of the item type of a SequenceType in the AST —
/// what `instance of` and `treat as` match with. The binder computes it once
/// per expression (`TypeExprNode::resolved_item_type`).
///
/// An `AtomicType` in the `xs` namespace becomes [`ItemType::AtomicType`];
/// any other one names an atomic simple type of the schema set in the static
/// context and becomes [`ItemType::SchemaAtomicType`], which matches a value whose
/// type annotation derives from it — XPath 2.0 §2.5.4.2: "An AtomicType
/// AtomicType matches an atomic value whose actual type is AT if
/// derives-from(AT, AtomicType) is true". `resolved_atomic_type` is the
/// binder's expansion of the name; a name it cannot resolve is `XPST0051`, a
/// kind test fails as [`item_type_of_kind_test`] does.
pub(crate) fn item_type_of_item_type_node(
    item_type: &ItemTypeNode,
    resolved_atomic_type: Option<&QualifiedName>,
    ctx: &XPathContext<'_>,
) -> Result<ItemType, XPathError> {
    use crate::namespace::table::well_known;
    use crate::xpath::cast::resolved_type_to_type_code;

    match item_type {
        ItemTypeNode::Item => Ok(ItemType::AnyItem),
        ItemTypeNode::Kind(kind) => item_type_of_kind_test(kind, ctx),
        ItemTypeNode::Atomic(raw) => {
            let unknown = || XPathError::unknown_type(raw.to_string());
            let qname = resolved_atomic_type.ok_or_else(unknown)?;
            if qname.namespace_uri == Some(well_known::XS_NAMESPACE) {
                return resolved_type_to_type_code(qname, ctx.names).map(ItemType::AtomicType);
            }
            // Only an atomic type is an AtomicType (§2.5.4.2): a union or list
            // type of the schema is XPST0051 like a complex one.
            let schema_set = ctx.schema_set.ok_or_else(unknown)?;
            match schema_set.lookup_type(qname.namespace_uri, qname.local_name) {
                Some(key @ TypeKey::Simple(simple)) if is_atomic_simple_type(schema_set, key) => {
                    Ok(ItemType::SchemaAtomicType(simple))
                }
                _ => Err(unknown()),
            }
        }
    }
}

/// Check if a DOM node matches an AST KindTest.
///
/// The kind test is resolved against the static context into the runtime
/// [`ItemType`] (see `item_type_of_kind_test`) and matched with the one
/// implementation a step node test and a SequenceType also run, so
/// `instance of`, `treat as` and a path step agree on every test. A kind test
/// whose names the static context cannot resolve matches nothing.
pub fn matches_kind_test<N: DomNavigator>(
    nav: &N,
    kind_test: &KindTest,
    ctx: &XPathContext<'_>,
) -> bool {
    match item_type_of_kind_test(kind_test, ctx) {
        Ok(item_type) => matches_item_type(&item_type, nav, ctx),
        Err(_) => false,
    }
}

#[cfg(test)]
#[path = "node_test_tests.rs"]
mod tests;
