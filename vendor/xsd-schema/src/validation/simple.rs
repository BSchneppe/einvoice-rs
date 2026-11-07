//! Simple type value validation
//!
//! Validates text content against XSD simple types (atomic, list, union).
//! Reuses `VALIDATOR_REGISTRY` for lexical validation and `FacetSet` for
//! facet constraint checking.

use crate::ids::{NameId, SimpleTypeKey, TypeKey};
use crate::namespace::context::NamespaceContextSnapshot;
use crate::namespace::qname::QualifiedName;
use crate::namespace::table::NameTable;
use crate::parser::frames::{ComplexContentResult, DerivationMethod, SimpleTypeVariety};
use crate::schema::SchemaSet;
use crate::types::facets::{normalize_whitespace, EnumerationFacet, FacetSet, WhitespaceMode};
use crate::types::validators::{ValidationError as TypeValidationError, VALIDATOR_REGISTRY};
use crate::types::value::{QNameValue, XmlAtomicValue, XmlValue, XmlValueKind};
use crate::types::XmlTypeCode;
use rust_decimal::Decimal;

use super::errors::{self, ValidationError};

/// Result of simple type validation
#[derive(Debug)]
pub struct SimpleTypeResult {
    /// The typed value produced by validation
    pub typed_value: XmlValue,
    /// For union types: the member type that matched
    pub member_type: Option<TypeKey>,
    /// The whitespace-normalized value (PSVI `[schema normalized value]`)
    pub normalized_value: Option<String>,
}

/// Validate a string value against a simple type.
///
/// Dispatches to atomic, list, or union validation depending on the type's variety.
/// For complex types with simpleContent, walks the base type chain to find the
/// underlying simple type.
///
/// Without a namespace context an `xs:QName` / `xs:NOTATION` value stays in
/// its lexical form: its typed value is the whitespace-collapsed string under
/// the QName / NOTATION type code, and `enumeration` compares lexical forms.
/// The validation runtime, the typed document's navigator and the casts of
/// the XPath engine resolve such values against the namespace bindings in
/// scope where they occur.
pub fn validate_simple_type(
    value: &str,
    type_key: TypeKey,
    schema_set: &SchemaSet,
) -> Result<SimpleTypeResult, ValidationError> {
    validate_simple_type_inner(value, type_key, schema_set, None)
}

/// How simple-type validation resolves the prefix of an `xs:QName` /
/// `xs:NOTATION` lexical form — Datatypes §3.3.18: "The mapping from lexical
/// space to value space for a particular QName ·literal· depends on the
/// namespace bindings in scope where the literal occurs". Called with the
/// prefix, or `None` for an unprefixed name; returns the namespace bound to
/// it — for `None` the default namespace, itself `None` when there is none
/// ("If the host language does not specify otherwise, unqualified names are
/// bound to the default namespace") — or `None` when the prefix is bound
/// nowhere.
pub(crate) type PrefixResolver<'r> = &'r dyn Fn(Option<&str>) -> Option<Option<NameId>>;

/// A [`PrefixResolver`] over the bindings of a namespace snapshot.
pub(crate) fn snapshot_prefixes<'s>(
    snapshot: &'s NamespaceContextSnapshot,
    names: &'s NameTable,
) -> impl Fn(Option<&str>) -> Option<Option<NameId>> + 's {
    move |prefix| match prefix {
        None | Some("") => Some(snapshot.default_ns),
        Some(prefix) => names
            .get(prefix)
            .and_then(|id| snapshot.resolve_prefix(id))
            .map(Some),
    }
}

/// [`validate_simple_type`] with the namespace bindings in scope where the
/// value occurs: an `xs:QName` / `xs:NOTATION` value — atomic, a member of a
/// list, the value a union's member type gave — whose prefix `resolver` binds
/// is validated in the value space (Datatypes §3.3.18: "The ·value space· of
/// QName is the set of tuples {namespace name, local part}"): its typed value
/// is the QName (NOTATION) value, `enumeration` compares expanded names — each
/// enumerated literal resolved with the bindings of the schema element it was
/// written on — `pattern` constrains the lexical form, and an assertion's
/// `$value` is the QName value. A value whose prefix `resolver` binds nowhere
/// is validated as without a resolver (the caller reports the unbound prefix,
/// `cvc-datatype-valid.1.2.1`).
pub(crate) fn validate_simple_type_in(
    value: &str,
    type_key: TypeKey,
    schema_set: &SchemaSet,
    resolver: Option<PrefixResolver<'_>>,
) -> Result<SimpleTypeResult, ValidationError> {
    validate_simple_type_inner(value, type_key, schema_set, resolver)
}

/// Whether `value` holds an `xs:QName` / `xs:NOTATION` value in the value
/// space (not a lexical form) — itself, as a union's member value, or as a
/// list member.
pub(crate) fn holds_qname_value(value: &XmlValue) -> bool {
    match &value.value {
        XmlValueKind::Atomic(XmlAtomicValue::QName(_) | XmlAtomicValue::Notation(_)) => true,
        XmlValueKind::Union(inner) => holds_qname_value(inner),
        XmlValueKind::List { items, .. } => items.iter().any(holds_qname_value),
        _ => false,
    }
}

/// Whether `value` holds an `xs:QName` or `xs:NOTATION` still in lexical form
/// (an `xs:string` atom under the QName or NOTATION type code) — itself, as a
/// union's member value, or as a list member.
pub(crate) fn holds_lexical_qname(value: &XmlValue) -> bool {
    match &value.value {
        XmlValueKind::Atomic(XmlAtomicValue::String(_)) => {
            matches!(value.type_code, XmlTypeCode::QName | XmlTypeCode::Notation)
        }
        XmlValueKind::Union(inner) => holds_lexical_qname(inner),
        XmlValueKind::List { items, .. } => items.iter().any(holds_lexical_qname),
        _ => false,
    }
}

/// Whether values of `type_key` can be (or contain) `xs:QName` /
/// `xs:NOTATION` values: an atomic type derived from either, a list whose item
/// type can, a union with such a member type, a complex type whose simple
/// content can.
pub(crate) fn type_has_qname_values(type_key: TypeKey, schema_set: &SchemaSet) -> bool {
    fn walk(type_key: TypeKey, schema_set: &SchemaSet, depth: u32) -> bool {
        if depth > 64 {
            return false;
        }
        match type_key {
            TypeKey::Simple(sk) => {
                let Some(st) = schema_set.arenas.simple_types.get(sk) else {
                    return false;
                };
                match st.variety {
                    SimpleTypeVariety::Atomic => matches!(
                        resolve_type_code(sk, schema_set),
                        Some(XmlTypeCode::QName | XmlTypeCode::Notation)
                    ),
                    SimpleTypeVariety::List => st
                        .resolved_item_type
                        .is_some_and(|item| walk(item, schema_set, depth + 1)),
                    SimpleTypeVariety::Union => st
                        .resolved_member_types
                        .iter()
                        .any(|&member| walk(member, schema_set, depth + 1)),
                }
            }
            TypeKey::Complex(ck) => schema_set
                .arenas
                .complex_types
                .get(ck)
                .and_then(|ct| ct.resolved_simple_content_type.or(ct.resolved_base_type))
                .is_some_and(|next| {
                    next != TypeKey::Complex(ck) && walk(next, schema_set, depth + 1)
                }),
        }
    }
    walk(type_key, schema_set, 0)
}

/// Whether values of the simple type `type_key` can be (or contain) `xs:ID`,
/// `xs:IDREF` or `xs:ENTITY` values — the ones the ID/IDREF table
/// (Structures §3.17.5.2: "is or is derived from ID, IDREF or IDREFS") and the
/// ENTITY check take: an atomic type derived from one of them, a list whose
/// item type can (`xs:IDREFS`, `xs:ENTITIES` included), a union with such a
/// member type. Which members a value has is then decided by its typed value.
pub(crate) fn type_has_id_family_values(type_key: TypeKey, schema_set: &SchemaSet) -> bool {
    fn walk(type_key: TypeKey, schema_set: &SchemaSet, depth: u32) -> bool {
        if depth > 64 {
            return false;
        }
        let TypeKey::Simple(sk) = type_key else {
            return false;
        };
        let Some(st) = schema_set.arenas.simple_types.get(sk) else {
            return false;
        };
        match st.variety {
            SimpleTypeVariety::Atomic => matches!(
                resolve_type_code(sk, schema_set),
                Some(
                    XmlTypeCode::Id
                        | XmlTypeCode::IdRef
                        | XmlTypeCode::IdRefs
                        | XmlTypeCode::Entity
                        | XmlTypeCode::Entities
                )
            ),
            SimpleTypeVariety::List => {
                matches!(
                    resolve_type_code(sk, schema_set),
                    Some(XmlTypeCode::IdRefs | XmlTypeCode::Entities)
                ) || st
                    .resolved_item_type
                    .is_some_and(|item| walk(item, schema_set, depth + 1))
            }
            SimpleTypeVariety::Union => st
                .resolved_member_types
                .iter()
                .any(|&member| walk(member, schema_set, depth + 1)),
        }
    }
    walk(type_key, schema_set, 0)
}

/// [`fixed_values_equal`] for two literals each read with the namespace
/// bindings in scope where it was written (`a_ns`, `b_ns`): two
/// `xs:QName` / `xs:NOTATION` values (or lists, unions holding them) are
/// equal when their expanded names are (Datatypes §3.3.18). Without both
/// snapshots, for any other type, or when a literal does not resolve, as
/// [`fixed_values_equal`].
pub(crate) fn fixed_values_equal_in(
    a: &str,
    a_ns: Option<&NamespaceContextSnapshot>,
    b: &str,
    b_ns: Option<&NamespaceContextSnapshot>,
    type_key: Option<TypeKey>,
    schema_set: &SchemaSet,
) -> bool {
    match resolved_pair(a, a_ns, b, b_ns, type_key, schema_set) {
        Some((a, b)) => typed_values_equal(&a, &b),
        None => fixed_values_equal(a, b, type_key, schema_set),
    }
}

/// [`fixed_values_identical`] for two literals each read with its own
/// namespace bindings, as [`fixed_values_equal_in`] does.
pub(crate) fn fixed_values_identical_in(
    a: &str,
    a_ns: Option<&NamespaceContextSnapshot>,
    b: &str,
    b_ns: Option<&NamespaceContextSnapshot>,
    type_key: TypeKey,
    schema_set: &SchemaSet,
) -> Option<bool> {
    match resolved_pair(a, a_ns, b, b_ns, Some(type_key), schema_set) {
        Some((a, b)) => Some(if schema_set.is_xsd10() {
            typed_values_equal(&a, &b)
        } else {
            super::identity::values_identical(&a, &b)
        }),
        None => fixed_values_identical(a, b, type_key, schema_set),
    }
}

/// The typed values of two QName-valued literals, each validated with the
/// bindings it was written with; `None` unless the type has QName values,
/// both snapshots are known and both literals resolve.
fn resolved_pair(
    a: &str,
    a_ns: Option<&NamespaceContextSnapshot>,
    b: &str,
    b_ns: Option<&NamespaceContextSnapshot>,
    type_key: Option<TypeKey>,
    schema_set: &SchemaSet,
) -> Option<(XmlValue, XmlValue)> {
    let (a_ns, b_ns, type_key) = (a_ns?, b_ns?, type_key?);
    if !type_has_qname_values(type_key, schema_set) {
        return None;
    }
    let names = &schema_set.name_table;
    let (ra, rb) = (
        snapshot_prefixes(a_ns, names),
        snapshot_prefixes(b_ns, names),
    );
    let a = validate_simple_type_in(a, type_key, schema_set, Some(&ra)).ok()?;
    let b = validate_simple_type_in(b, type_key, schema_set, Some(&rb)).ok()?;
    (holds_qname_value(&a.typed_value) && holds_qname_value(&b.typed_value))
        .then_some((a.typed_value, b.typed_value))
}

/// [`fixed_matches_typed`] for a fixed literal read with the namespace
/// bindings `fixed_ns` of the declaration it was written on: when the
/// instance's typed value holds `xs:QName` / `xs:NOTATION` values (resolved
/// with the instance's bindings), the fixed literal is resolved with the
/// schema's and the two are compared by expanded name — Structures §3.3.4.3
/// clause 5.2.2.2.2, "the ·actual value· of E is equal or identical to
/// D.{value constraint}.{value}", the {value} being "the ·actual value· …
/// of the [attribute]" on the declaration. Otherwise (or when the literal does
/// not resolve) as [`fixed_matches_typed`].
pub(crate) fn fixed_matches_typed_in(
    instance_raw: &str,
    instance_typed: &XmlValue,
    fixed: &str,
    fixed_ns: Option<&NamespaceContextSnapshot>,
    type_key: Option<TypeKey>,
    schema_set: &SchemaSet,
) -> bool {
    if let (Some(fixed_ns), Some(tk)) = (fixed_ns, type_key) {
        if holds_qname_value(instance_typed) {
            let resolver = snapshot_prefixes(fixed_ns, &schema_set.name_table);
            if let Ok(fixed) = validate_simple_type_in(fixed, tk, schema_set, Some(&resolver)) {
                if holds_qname_value(&fixed.typed_value) {
                    return typed_values_equal(instance_typed, &fixed.typed_value);
                }
            }
        }
    }
    fixed_matches_typed(instance_raw, instance_typed, fixed, type_key, schema_set)
}

/// Whether the enumeration literal `literal` (an `xs:QName` / `xs:NOTATION`
/// lexical form), read with the bindings `snapshot` of the schema element it
/// was written on, denotes the expanded name `{namespace}local`.
fn qname_literal_denotes(
    literal: &str,
    snapshot: &NamespaceContextSnapshot,
    namespace: Option<NameId>,
    local: &str,
    names: &NameTable,
) -> bool {
    let literal = literal.trim();
    let (prefix, literal_local) = match literal.split_once(':') {
        Some((prefix, local)) => (Some(prefix), local),
        None => (None, literal),
    };
    literal_local == local && snapshot_prefixes(snapshot, names)(prefix) == Some(namespace)
}

/// Whether an `enumeration` of a type with QName values admits the QName
/// value `{namespace}local` whose lexical form is `normalized` (Datatypes
/// §4.3.5.4: "equal or identical to one of the values specified in
/// {value}"): a literal with the namespace bindings it was written with
/// (`qname_contexts`) by expanded name, a literal without them (a facet set
/// built by hand) by lexical form.
pub(crate) fn enumeration_admits_qname(
    enumeration: &EnumerationFacet,
    normalized: &str,
    namespace: Option<NameId>,
    local: &str,
    names: &NameTable,
) -> bool {
    enumeration
        .qname_contexts
        .iter()
        .any(|(literal, snapshot)| {
            qname_literal_denotes(literal, snapshot, namespace, local, names)
        })
        || (enumeration.values.contains(normalized)
            && !enumeration
                .qname_contexts
                .iter()
                .any(|(literal, _)| literal == normalized))
}

/// Whether the QName value `name` (lexical form `normalized`) is a value of
/// `enumeration` — through the cached expanded-name index of the simple type
/// `key` whose effective facets hold it, or, for a facet set of no simple type
/// (a complex type's simple-content restriction), by scanning it
/// ([`enumeration_admits_qname`]).
fn qname_enumerated(
    enumeration: &EnumerationFacet,
    key: Option<SimpleTypeKey>,
    normalized: &str,
    name: &QNameValue,
    notation: bool,
    schema_set: &SchemaSet,
) -> bool {
    let qualified = name.name();
    match key.and_then(|sk| schema_set.enumeration_index(sk)) {
        Some(index) => index.admits(
            normalized,
            notation,
            qualified.namespace_uri,
            qualified.local_name,
        ),
        None => enumeration_admits_qname(
            enumeration,
            normalized,
            qualified.namespace_uri,
            name.local_name(),
            &schema_set.name_table,
        ),
    }
}

/// Whether the enumeration literal `literal` of `from` — an `xs:QName` /
/// `xs:NOTATION` lexical form, read with each namespace snapshot recorded for
/// it — is a value of the enumeration `target`, whose expanded names are
/// `target_index` (Datatypes §4.3.5.5 enumeration valid restriction: a
/// restriction's enumeration values are values of its base). A literal
/// without a snapshot, or whose prefix its bindings do not bind, is looked up
/// by lexical form.
pub(crate) fn qname_literal_enumerated(
    from: &EnumerationFacet,
    literal: &str,
    target: &EnumerationFacet,
    target_index: &crate::types::facets::EnumerationIndex,
    notation: bool,
    names: &NameTable,
) -> bool {
    let mut contexts = from
        .qname_contexts
        .iter()
        .filter(|(value, _)| value == literal)
        .peekable();
    if contexts.peek().is_none() {
        return target.values.contains(literal);
    }
    let lexical = literal.trim();
    let (prefix, local) = match lexical.split_once(':') {
        Some((prefix, local)) => (Some(prefix), local),
        None => (None, lexical),
    };
    contexts.all(
        |(_, snapshot)| match snapshot_prefixes(snapshot, names)(prefix) {
            Some(namespace) => names
                .get(local)
                .is_some_and(|local| target_index.has_name(notation, namespace, local)),
            None => target.values.contains(literal),
        },
    )
}

/// Whether some reading of the enumeration literal `literal` satisfies
/// `matches`: one per namespace snapshot recorded for it (`qname_contexts`,
/// each given as a [`PrefixResolver`]), or — without any — a single reading
/// without one.
fn literal_readings(
    enumeration: &EnumerationFacet,
    literal: &str,
    names: &NameTable,
    matches: &dyn Fn(Option<PrefixResolver<'_>>) -> bool,
) -> bool {
    let mut contexts = enumeration
        .qname_contexts
        .iter()
        .filter(|(value, _)| value == literal)
        .peekable();
    if contexts.peek().is_none() {
        return matches(None);
    }
    contexts.any(|(_, snapshot)| {
        let resolver = snapshot_prefixes(snapshot, names);
        matches(Some(&resolver))
    })
}

/// The facets of a type derived from `xs:QName` / `xs:NOTATION` on the QName
/// value `name` of the lexical form `normalized` (whitespace collapsed):
/// `pattern` on the lexical form (Datatypes §4.3.4), `enumeration` by expanded
/// name ([`qname_enumerated`]; `key` is the simple type whose effective facets
/// `facets` are, `None` for a simple-content restriction's own); the length
/// facets admit every value (Datatypes §4.3.1.3, as for a lexical QName). The
/// errors are those of the lexical check.
fn check_qname_facets(
    normalized: &str,
    name: &QNameValue,
    notation: bool,
    facets: &FacetSet,
    key: Option<SimpleTypeKey>,
    schema_set: &SchemaSet,
) -> Result<(), ValidationError> {
    let error = |e: crate::error::FacetError| {
        errors::from_value_error_default(TypeValidationError::FacetViolation(e), None)
    };
    facets.validate_patterns_only(normalized).map_err(error)?;
    if let Some(enumeration) = &facets.enumeration {
        if !qname_enumerated(enumeration, key, normalized, name, notation, schema_set) {
            return Err(error(crate::error::FacetError::enumeration(normalized)));
        }
    }
    Ok(())
}

/// Value-space equality for fixed/default values (§3.3.4.3 clause 5.2.2.2.2, §3.2.4.4).
///
/// Fast-path lexical match; otherwise parses both strings against `type_key`
/// and compares their typed values. Returns `false` when `type_key` is `None`
/// or when either parse fails.
///
/// The comparison is Structures 1.1's "equal or identical" in **both** XSD
/// modes: `NaN` matches `NaN`, `0` matches `-0`, two date/time spellings of
/// one instant in different time zones match, durations compare by months and
/// seconds (`PT60S` matches `PT1M`), decimals by value, and a union value
/// whichever member type each spelling chose; values of different primitive
/// types never match. In XSD 1.0 mode this deliberately departs from the
/// letter of two XSD 1.0 clauses that compare strings — Attribute Locally
/// Valid (Use), "its ·normalized value· must match the canonical lexical
/// representation of the attribute use's {value constraint} value", and
/// derivation-ok-restriction 2.1.3.2, "R's ·effective value constraint· is
/// fixed with the same string as B's" — so values equal in value space but
/// spelled differently are accepted there too; no W3C XSD 1.0 test tells the
/// two readings apart. (The attribute-use-versus-declaration check of the
/// schema compiler compares by identity in XSD 1.1 and by equality in 1.0.)
pub fn fixed_values_equal(
    a: &str,
    b: &str,
    type_key: Option<TypeKey>,
    schema_set: &SchemaSet,
) -> bool {
    if a == b {
        return true;
    }
    let Some(tk) = type_key else {
        return false;
    };
    let Ok(a_result) = validate_simple_type(a, tk, schema_set) else {
        return false;
    };
    let Ok(b_result) = validate_simple_type(b, tk, schema_set) else {
        return false;
    };
    typed_literals_equal(&a_result.typed_value, a, &b_result.typed_value, b)
}

/// Whether two lexical forms of `type_key` denote **identical** values — the
/// comparison Structures 1.1 Attribute Use Correct clause 3 makes between an
/// attribute use's `fixed` value and its declaration's ("is identical to").
/// XSD 1.0 had no separate identity (Datatypes 1.1 §2.2.2: "In the prior
/// version of this specification (1.0), equality was always identity"), so a
/// 1.0 schema set compares them with [`typed_values_equal`]. `None` when
/// either form does not validate against the type (another constraint reports
/// that).
pub(crate) fn fixed_values_identical(
    a: &str,
    b: &str,
    type_key: TypeKey,
    schema_set: &SchemaSet,
) -> Option<bool> {
    if a == b {
        return Some(true);
    }
    let a_result = validate_simple_type(a, type_key, schema_set).ok()?;
    let b_result = validate_simple_type(b, type_key, schema_set).ok()?;
    Some(if schema_set.is_xsd10() {
        typed_literals_equal(&a_result.typed_value, a, &b_result.typed_value, b)
    } else {
        super::identity::literal_values_identical(
            &a_result.typed_value,
            a,
            &b_result.typed_value,
            b,
        )
    })
}

/// Whether two typed values are "equal or identical" — the comparison every
/// value constraint makes: Structures §3.3.4.3 clause 5.2.2.2.2 ("the
/// ·actual value· of E is equal or identical to D.{value constraint}.{value}"),
/// Attribute Locally Valid (Use) ("its ·actual value· must be equal or
/// identical to the {value} of the attribute use's {value constraint}"), and
/// the schema-time checks that compare two fixed values. It is
/// [`values_equal_or_identical`](super::identity::values_equal_or_identical):
/// within one primitive datatype by value (`xs:decimal`-family values exactly,
/// so a decimal carried in lexical form equals every other spelling of the
/// same number; NaN identical to NaN and 0 equal to −0 for `xs:float` /
/// `xs:double`; date/time values by time on the time line; durations by months
/// and seconds), values of different primitive datatypes distinct, lists
/// member for member, and which restriction or union member type validated a
/// value never matters (Datatypes §2.2.1: restrictions "do not create new
/// values"). A structurally equal pair is decided without that walk.
pub(crate) fn typed_values_equal(a: &XmlValue, b: &XmlValue) -> bool {
    a == b || super::identity::values_equal_or_identical(a, b)
}

/// XSD 1.0's Element Locally Valid (Element) clause 5.2.2.2.2 under a
/// governing type other than the declared one: "the ·actual value· of the
/// item must match the canonical lexical representation of the {value
/// constraint} value". The canonical form ([`xsd10_canonical_form`]) only
/// *names* the {value}; the comparison is by value, in the value space of the
/// item's type, and no facet of the governing type (`pattern`, `length`,
/// digits, `enumeration`, …) is applied to any spelling:
///
/// - two atomic values of one primitive datatype other than `string` are
///   compared as XSD 1.1 compares them ([`typed_literals_equal`], equal or
///   identical, the seconds of a date/time or duration as written) — the
///   canonical form of a value, read back in its own value space, is that
///   value;
/// - otherwise — a `string` {value}, values of two primitive datatypes, a list
///   against an atomic value either way — the {value}'s canonical form is
///   read by the governing type's lexical mapping (its white-space
///   normalization, then the built-in datatype of the item's value or of each
///   of its list members; a QName with the bindings of the declaration
///   `value_ns`) and the values compared.
///
/// This is what the 1.0 text adds over 1.1's "equal or identical to D.{value
/// constraint}.{value}": a {value} of another datatype (the string "1" of an
/// `xs:anyType` declaration, the integer 1 or the list `1 2` of a union
/// member, a dateTime member in `Z`) matches the item's value through its
/// canonical form, and a string {value} is read with the governing type's
/// white space. `actual_text` and `value_lexical` are the literals the two
/// values were read from (the element's text, D.{lexical form}).
pub(crate) fn xsd10_fixed_matches(
    actual: &XmlValue,
    actual_text: &str,
    value: &XmlValue,
    value_lexical: &str,
    governing: Option<TypeKey>,
    value_ns: Option<&NamespaceContextSnapshot>,
    schema_set: &SchemaSet,
) -> bool {
    fn member(v: &XmlValue) -> &XmlValue {
        match &v.value {
            XmlValueKind::Union(inner) => member(inner),
            _ => v,
        }
    }
    fn items(v: &XmlValue) -> Option<&[XmlValue]> {
        match &v.value {
            XmlValueKind::List { items, .. } => Some(items),
            _ => None,
        }
    }
    let collapse = |c: &str| normalize_whitespace(c, WhitespaceMode::Collapse).into_owned();
    let (a, v) = (member(actual), member(value));
    let a_texts: Vec<&str> = actual_text.split_ascii_whitespace().collect();
    match (items(a), items(v)) {
        (Some(a_items), Some(v_items)) => {
            let v_texts: Vec<&str> = value_lexical.split_ascii_whitespace().collect();
            a_items.len() == v_items.len()
                && a_texts.len() == a_items.len()
                && v_texts.len() == v_items.len()
                && (0..a_items.len()).all(|i| {
                    atomic_matches_10(
                        member(&a_items[i]),
                        a_texts[i],
                        member(&v_items[i]),
                        v_texts[i],
                        value_ns,
                        schema_set,
                        collapse,
                    )
                })
        }
        // An atomic {value} (a string "1 2") against a list: its canonical
        // form, read as the list reads it (collapsed, member by member).
        (Some(a_items), None) => {
            let canonical = xsd10_canonical_form(v, value_lexical);
            let forms: Vec<&str> = canonical.split_ascii_whitespace().collect();
            a_items.len() == forms.len()
                && a_texts.len() == a_items.len()
                && (0..a_items.len()).all(|i| {
                    read_and_compare_10(
                        member(&a_items[i]),
                        a_texts[i],
                        forms[i],
                        value_ns,
                        schema_set,
                    )
                })
        }
        // A list {value} against an atomic value: the list's canonical form
        // (its members' forms joined by single spaces).
        (None, Some(_)) => {
            let canonical = xsd10_canonical_form(v, value_lexical);
            let normalized = governing_normalized(&canonical, governing, schema_set);
            read_and_compare_10(a, actual_text, &normalized, value_ns, schema_set)
        }
        (None, None) => atomic_matches_10(
            a,
            actual_text,
            v,
            value_lexical,
            value_ns,
            schema_set,
            |c| governing_normalized(c, governing, schema_set),
        ),
    }
}

/// `literal` normalized with the white space of the governing type.
fn governing_normalized(
    literal: &str,
    governing: Option<TypeKey>,
    schema_set: &SchemaSet,
) -> String {
    match governing {
        Some(tk) => normalized_literal(literal, tk, schema_set)
            .map(|n| n.into_owned())
            .unwrap_or_else(|| literal.to_owned()),
        None => literal.to_owned(),
    }
}

/// [`xsd10_fixed_matches`] for two atomic values; `normalize` is the
/// governing type's white-space normalization.
fn atomic_matches_10(
    actual: &XmlValue,
    actual_text: &str,
    value: &XmlValue,
    value_lexical: &str,
    value_ns: Option<&NamespaceContextSnapshot>,
    schema_set: &SchemaSet,
    normalize: impl Fn(&str) -> String,
) -> bool {
    let qname = |v: &XmlValue| {
        holds_qname_value(v) || matches!(v.type_code, XmlTypeCode::QName | XmlTypeCode::Notation)
    };
    let primitive = actual.primitive_type();
    if (qname(actual) && qname(value))
        || (primitive.is_some()
            && primitive == value.primitive_type()
            && primitive != Some(crate::types::PrimitiveTypeCode::String))
    {
        return typed_literals_equal(actual, actual_text, value, value_lexical);
    }
    let canonical = normalize(&xsd10_canonical_form(value, value_lexical));
    read_and_compare_10(actual, actual_text, &canonical, value_ns, schema_set)
}

/// Whether `form`, read by the built-in datatype of the atomic value `actual`
/// (no facet of a derived type), is that value; a QName with the bindings
/// `value_ns` of the declaration the form names a value of.
fn read_and_compare_10(
    actual: &XmlValue,
    actual_text: &str,
    form: &str,
    value_ns: Option<&NamespaceContextSnapshot>,
    schema_set: &SchemaSet,
) -> bool {
    if let XmlValueKind::UntypedAtomic(s) = &actual.value {
        return s == form;
    }
    let Some(builtin) = schema_set
        .builtin_types()
        .get_by_type_code(actual.type_code)
    else {
        return false;
    };
    let names = &schema_set.name_table;
    let resolver = value_ns.map(|ns| snapshot_prefixes(ns, names));
    match validate_simple_type_in(
        form,
        TypeKey::Simple(builtin),
        schema_set,
        resolver.as_ref().map(|r| r as PrefixResolver<'_>),
    ) {
        Ok(read) => typed_literals_equal(actual, actual_text, &read.typed_value, form),
        Err(_) => false,
    }
}

/// The canonical lexical representation XSD 1.0 gives `value`, read from the
/// literal `lexical` — built from the value, with the fraction of a seconds
/// field taken from the literal as written (no digit lost to the stored
/// value's capacity). Datatypes 1.0:
///
/// - §3.2.2.2 boolean `true` / `false`; §3.3.13.2 (and the integer types)
///   "the preceding optional "+" sign is prohibited and leading zeroes are
///   prohibited";
/// - §3.2.3.2 decimal: "The decimal point is required. Leading and trailing
///   zeroes are prohibited subject to the following: there must be at least one
///   digit to the right and to the left of the decimal point which may be a
///   zero";
/// - §3.2.4.2 / §3.2.5.2 float, double: "the exponent must be indicated by "E".
///   Leading zeroes and the preceding optional "+" sign are prohibited in the
///   exponent. If the exponent is zero, it must be indicated by "E0". For the
///   mantissa, the preceding optional "+" sign is prohibited and the decimal
///   point is required … The canonical representation for zero is 0.0E0";
/// - §3.2.7.2 dateTime: "The 2-digit numeral representing the hour must not be
///   '24'; The fractional second string, if present, must not end in '0'; for
///   timezoned values, the timezone must be represented with 'Z'"; §3.2.8.2
///   time: "the time zone must be Coordinated Universal Time (UTC) indicated by
///   a "Z". Additionally, the canonical representation for midnight is
///   00:00:00"; §3.2.9.2 date: "the date portion of the dateTime canonical
///   representation of the interval midpoint … For timezoned values, append the
///   canonical representation of the ·recoverable timezone·" (between -11:59
///   and +12:00, §3.2.9.1);
/// - §3.2.15.2 hexBinary: "the lower case hexadecimal digits ([a-f]) are not
///   allowed";
/// - §2.5.1.2 list: "each item in the ·list· has the canonical lexical
///   representation of its ·itemType·" (joined by single spaces); §2.5.1.3
///   union: "the values have the canonical lexical representation of the
///   appropriate ·memberTypes·".
///
/// Datatypes 1.0 defines none for the other datatypes. A string, `anyURI`,
/// `base64Binary` or Gregorian (`gYear` …) value is written as its string
/// value (its literals map one to one, a time zone `Z` for zero); a duration
/// as its six fields as given (§3.2.6: its value space is six-dimensional),
/// zero fields omitted, `PT0S` for zero; a QName / NOTATION as the literal,
/// whose prefix the declaration's bindings read.
pub(crate) fn xsd10_canonical_form(value: &XmlValue, lexical: &str) -> String {
    fn exponent_form(formatted: String) -> String {
        // Rust's `{:E}` writes the shortest mantissa that reads back as the
        // same value, with a bare exponent ("1E2", "-2.5E-3", "0E0").
        match formatted.split_once('E') {
            Some((mantissa, exponent)) if !mantissa.contains('.') => {
                format!("{mantissa}.0E{exponent}")
            }
            _ => formatted,
        }
    }
    use crate::types::PrimitiveTypeCode as P;
    match &value.value {
        XmlValueKind::Union(inner) => xsd10_canonical_form(inner, lexical),
        XmlValueKind::List { items, .. } => {
            let texts: Vec<&str> = lexical.split_ascii_whitespace().collect();
            items
                .iter()
                .enumerate()
                .map(|(i, item)| xsd10_canonical_form(item, texts.get(i).copied().unwrap_or("")))
                .collect::<Vec<_>>()
                .join(" ")
        }
        XmlValueKind::UntypedAtomic(s) => s.clone(),
        XmlValueKind::Atomic(atom) => match atom {
            XmlAtomicValue::Float(v) if v.is_finite() => exponent_form(format!("{v:E}")),
            XmlAtomicValue::Double(v) if v.is_finite() => exponent_form(format!("{v:E}")),
            XmlAtomicValue::DateTime(dt) => xsd10_date_time_form(dt, lexical),
            XmlAtomicValue::Time(t) => xsd10_time_form(t, lexical),
            XmlAtomicValue::Date(d) => xsd10_date_form(d),
            XmlAtomicValue::Duration(d) => xsd10_duration_form(d, lexical),
            _ => match value.primitive_type() {
                Some(P::QName | P::Notation) => {
                    normalize_whitespace(lexical, WhitespaceMode::Collapse).into_owned()
                }
                Some(P::Decimal) if value.type_code == XmlTypeCode::Decimal => {
                    // The decimal point is required ("1" is "1.0").
                    let form = value.to_string_value();
                    if form.contains('.') {
                        form
                    } else {
                        format!("{form}.0")
                    }
                }
                _ => value.to_string_value(),
            },
        },
    }
}

/// The fraction digits of the seconds of a date/time or duration literal as
/// written, without trailing zeros (empty for none).
fn literal_seconds_fraction(lexical: &str) -> &str {
    // The seconds are the last number of a time or duration literal; a date
    // part has no decimal point.
    let Some(dot) = lexical.rfind('.') else {
        return "";
    };
    let digits = &lexical[dot + 1..];
    let end = digits
        .bytes()
        .position(|b| !b.is_ascii_digit())
        .unwrap_or(digits.len());
    digits[..end].trim_end_matches('0')
}

/// Whole seconds of `second`, and the canonical seconds field: two digits,
/// then the literal's fraction (if any digit is left).
fn seconds_field(second: rust_decimal::Decimal, lexical: &str, width: usize) -> String {
    let whole = second.trunc();
    let fraction = literal_seconds_fraction(lexical);
    let whole = format!("{whole:0width$}");
    if fraction.is_empty() {
        whole
    } else {
        format!("{whole}.{fraction}")
    }
}

/// The days from 1970-01-01 of `year-month-day` with the XSD 1.0 year (no year
/// 0: -0001 precedes 0001) read astronomically.
fn days_10(year: i32, month: u8, day: u8) -> i128 {
    let astronomical = if year < 0 { year + 1 } else { year };
    crate::types::value_order::days_from_civil(i128::from(astronomical), i128::from(month))
        + i128::from(day)
        - 1
}

/// The XSD 1.0 year, month and day of the day `days` from 1970-01-01.
fn civil_10(days: i128) -> (i128, u8, u8) {
    // H. Hinnant's civil_from_days, proleptic Gregorian.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    let year = yoe + era * 400 + i128::from(month <= 2);
    // Back to the 1.0 numbering: astronomical 0 is -0001.
    (if year <= 0 { year - 1 } else { year }, month, day)
}

fn year_field(year: i128) -> String {
    if year < 0 {
        format!("-{:04}", year.unsigned_abs())
    } else {
        format!("{year:04}")
    }
}

fn timezone_field(minutes: i16) -> String {
    crate::types::value::TimezoneOffset(minutes).to_string()
}

/// §3.2.7.2: a timezoned value in `Z` (the instant moved by its offset), the
/// fraction as written without trailing zeros.
fn xsd10_date_time_form(dt: &crate::types::value::DateTimeValue, lexical: &str) -> String {
    let offset = dt.timezone.map_or(0, |tz| i128::from(tz.0));
    let minutes = i128::from(dt.hour) * 60 + i128::from(dt.minute) - offset;
    let days = days_10(dt.year, dt.month, dt.day) + minutes.div_euclid(1440);
    let minutes = minutes.rem_euclid(1440);
    let (year, month, day) = civil_10(days);
    let mut form = format!(
        "{}-{month:02}-{day:02}T{:02}:{:02}:{}",
        year_field(year),
        minutes / 60,
        minutes % 60,
        seconds_field(dt.second, lexical, 2)
    );
    if dt.timezone.is_some() {
        form.push('Z');
    }
    form
}

/// §3.2.8.2: a timezoned time in `Z`, midnight `00:00:00`.
fn xsd10_time_form(t: &crate::types::value::TimeValue, lexical: &str) -> String {
    let offset = t.timezone.map_or(0, |tz| i128::from(tz.0));
    let minutes = (i128::from(t.hour) * 60 + i128::from(t.minute) - offset).rem_euclid(1440);
    let mut form = format!(
        "{:02}:{:02}:{}",
        minutes / 60,
        minutes % 60,
        seconds_field(t.second, lexical, 2)
    );
    if t.timezone.is_some() {
        form.push('Z');
    }
    form
}

/// §3.2.9.2: the date of the interval midpoint with the ·recoverable
/// timezone· (between -11:59 and +12:00: an offset beyond moves the date by a
/// day and the offset by 24 hours).
fn xsd10_date_form(d: &crate::types::value::DateValue) -> String {
    let mut days = days_10(d.year, d.month, d.day);
    let timezone = d.timezone.map(|tz| {
        let mut minutes = tz.0;
        if minutes > 720 {
            days -= 1;
            minutes -= 1440;
        } else if minutes < -719 {
            days += 1;
            minutes += 1440;
        }
        minutes
    });
    let (year, month, day) = civil_10(days);
    let mut form = format!("{}-{month:02}-{day:02}", year_field(year));
    if let Some(minutes) = timezone {
        form.push_str(&timezone_field(minutes));
    }
    form
}

/// A duration as its six fields as given, zero fields omitted (`PT0S` for
/// zero), the seconds' fraction as written.
fn xsd10_duration_form(d: &crate::types::value::DurationValue, lexical: &str) -> String {
    let seconds = seconds_field(d.seconds, lexical, 1);
    let zero_seconds = d.seconds.is_zero() && literal_seconds_fraction(lexical).is_empty();
    let mut form = String::new();
    if d.years > 0 {
        form.push_str(&format!("{}Y", d.years));
    }
    if d.months > 0 {
        form.push_str(&format!("{}M", d.months));
    }
    if d.days > 0 {
        form.push_str(&format!("{}D", d.days));
    }
    let mut time = String::new();
    if d.hours > 0 {
        time.push_str(&format!("{}H", d.hours));
    }
    if d.minutes > 0 {
        time.push_str(&format!("{}M", d.minutes));
    }
    if !zero_seconds {
        time.push_str(&format!("{seconds}S"));
    }
    if !time.is_empty() {
        form.push('T');
        form.push_str(&time);
    }
    if form.is_empty() {
        return "PT0S".to_string();
    }
    if d.negative {
        format!("-P{form}")
    } else {
        format!("P{form}")
    }
}

/// [`typed_values_equal`] of two values read from the literals `a_lexical`
/// and `b_lexical`: a date/time or duration literal whose seconds have more
/// fraction digits than its typed value holds is compared with the fraction
/// as written
/// ([`literal_values_equal_or_identical`](super::identity::literal_values_equal_or_identical)).
pub(crate) fn typed_literals_equal(
    a: &XmlValue,
    a_lexical: &str,
    b: &XmlValue,
    b_lexical: &str,
) -> bool {
    use crate::types::validators::seconds_beyond_capacity;
    if seconds_beyond_capacity(a_lexical) || seconds_beyond_capacity(b_lexical) {
        super::identity::literal_values_equal_or_identical(a, a_lexical, b, b_lexical)
    } else {
        typed_values_equal(a, b)
    }
}

/// The first `xs:decimal` inside a valid typed value that is carried as a
/// string because `rust_decimal::Decimal` cannot hold it — the value itself, a
/// union's member value, or a list member — as its canonical representation;
/// the runtime reports it with the capacity warning (Datatypes 1.1 §5.4).
///
/// Every list member carries the type that validated it — for a list whose
/// item type is a union, the member type of the union that accepted it — so a
/// member is such a decimal exactly when it is one, whatever the other members
/// are.
pub(crate) fn capacity_decimal(value: &XmlValue) -> Option<String> {
    crate::types::exact_decimal::first_lexical_decimal(value).map(str::to_owned)
}

/// The first member literal of a date/time or duration value whose seconds
/// had more fraction digits than this implementation holds, and were
/// truncated to fit
/// ([`seconds_beyond_capacity`](crate::types::validators::seconds_beyond_capacity));
/// `None` when every
/// seconds number was kept exactly. `text` is the literal the value was
/// validated from; a list member is paired with its whitespace-separated
/// token.
pub(crate) fn capacity_seconds(value: &XmlValue, text: &str) -> Option<String> {
    match &value.value {
        XmlValueKind::Atomic(
            XmlAtomicValue::DateTime(_)
            | XmlAtomicValue::Time(_)
            | XmlAtomicValue::Duration(_)
            | XmlAtomicValue::DayTimeDuration(_),
        ) => {
            let text = text.trim();
            crate::types::validators::seconds_beyond_capacity(text).then(|| text.to_owned())
        }
        XmlValueKind::Atomic(_) | XmlValueKind::UntypedAtomic(_) => None,
        XmlValueKind::Union(inner) => capacity_seconds(inner, text),
        XmlValueKind::List { items, .. } => items
            .iter()
            .zip(text.split_ascii_whitespace())
            .find_map(|(item, token)| capacity_seconds(item, token)),
    }
}

/// Value-space equality when the instance's typed value is already computed.
///
/// Avoids re-parsing `instance_raw`; only parses `fixed` on lexical mismatch.
/// Returns `false` when `type_key` is `None` or when parsing `fixed` fails.
/// The comparison, and its deliberate departure from the string-based XSD 1.0
/// clauses in XSD 1.0 mode, is that of [`fixed_values_equal`].
pub fn fixed_matches_typed(
    instance_raw: &str,
    instance_typed: &XmlValue,
    fixed: &str,
    type_key: Option<TypeKey>,
    schema_set: &SchemaSet,
) -> bool {
    if instance_raw == fixed {
        return true;
    }
    let Some(tk) = type_key else {
        return false;
    };
    let Ok(fixed_result) = validate_simple_type(fixed, tk, schema_set) else {
        return false;
    };
    typed_literals_equal(
        instance_typed,
        instance_raw,
        &fixed_result.typed_value,
        fixed,
    )
}

/// PERF_LAZY_PSVI Phase 2 — opt-out fast path.
///
/// When the caller has cleared `BUILD_PSVI_TYPED_VALUES` and no consumer needs
/// the typed value (the runtime checks fixed/default, identity constraints,
/// etc. before calling), validate "accept-any-input" string-family atomic
/// types *without materializing an `XmlValue`*.
///
/// Returns:
/// - `Some(Ok(()))` — validated, no value built;
/// - `Some(Err(_))` — invalid (only the string family; the error is identical to
///   the full path because it reuses the same validator);
/// - `None` — not eligible, *or* a numeric value the fast check could not accept;
///   the caller must fall back to [`validate_simple_type`], which builds the value
///   (needed for lexical-validity of constrained types, value-space facets,
///   ID/QName/list/union, XSD 1.1 assertions) and is the single source of truth
///   for diagnostics.
///
/// Two atomic-type families qualify:
///
/// - **String family** (`xs:string` / `xs:normalizedString` / `xs:token` and
///   restrictions): their lexical space accepts any whitespace-normalized input,
///   so skipping the parse cannot miss a lexical-validity error. Any facets are
///   enforced by reusing the registered validator's facet logic and discarding
///   the produced value — so an emitted error matches the full path exactly.
/// - **Numeric family** (`xs:decimal` and the integer hierarchy — Phase 3): a
///   *definitively valid* value is accepted via an `i128` (integer) or stack
///   `Decimal` parse, *without* heap-allocating a `BigInt` or building an
///   `XmlValue`. Anything the fast check cannot accept (invalid lexical, facet
///   violation, integer past `i128`) returns `None` and defers to the full path,
///   keeping diagnostics byte-identical — the built-in integer validators reject
///   out-of-range values via native parse / `RangeError`, not via bound facets.
///   See [`validate_numeric_lexical`]. Floats/doubles, lists, and unions also
///   fall back.
pub(crate) fn try_validate_lexical_only(
    value: &str,
    type_key: TypeKey,
    schema_set: &SchemaSet,
) -> Option<Result<(), ValidationError>> {
    let TypeKey::Simple(sk) = type_key else {
        return None;
    };
    let st = schema_set.arenas.simple_types.get(sk)?;
    if st.variety != SimpleTypeVariety::Atomic {
        return None;
    }
    let code = resolve_type_code(sk, schema_set)?;
    let is_string = matches!(
        code,
        XmlTypeCode::String | XmlTypeCode::NormalizedString | XmlTypeCode::Token
    );
    let is_numeric = is_numeric_fast_eligible(code);
    if !is_string && !is_numeric {
        return None;
    }
    let facets = schema_set.effective_facets(sk);
    // XSD 1.1 assertions bind `$value`; they need the materialized value.
    #[cfg(feature = "xsd11")]
    if !facets.assertions.is_empty() {
        return None;
    }

    if is_string {
        if facets.is_empty() {
            // No constraints and any input is lexically valid — nothing to do.
            return Some(Ok(()));
        }
        // Reuse the registered validator's facet logic for identical semantics,
        // discarding the produced value.
        let validator = VALIDATOR_REGISTRY.get_by_code(code)?;
        return Some(match validator.validate_with_facets(value, &facets) {
            Ok(_) => Ok(()),
            Err(type_err) => Err(errors::from_value_error_default(type_err, None)),
        });
    }

    // Numeric (Phase 3): accept only a definitively-valid value without building
    // a `BigInt` / `XmlValue`; defer everything else (including errors) to the
    // full path so diagnostics stay identical to the PSVI-on path.
    let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
    validate_numeric_lexical(code, &normalized, &facets).map(|()| Ok(()))
}

/// Integer-hierarchy and `xs:decimal` atomic codes eligible for the Phase 3
/// allocation-free numeric fast path (PERF_LAZY_PSVI). `Float`/`Double` are
/// excluded (already `Copy`/heap-free, and their NaN/INF facet semantics differ).
fn is_numeric_fast_eligible(code: XmlTypeCode) -> bool {
    matches!(
        code,
        XmlTypeCode::Decimal
            | XmlTypeCode::Integer
            | XmlTypeCode::Long
            | XmlTypeCode::Int
            | XmlTypeCode::Short
            | XmlTypeCode::Byte
            | XmlTypeCode::NonNegativeInteger
            | XmlTypeCode::UnsignedLong
            | XmlTypeCode::UnsignedInt
            | XmlTypeCode::UnsignedShort
            | XmlTypeCode::UnsignedByte
            | XmlTypeCode::NonPositiveInteger
            | XmlTypeCode::NegativeInteger
            | XmlTypeCode::PositiveInteger
    )
}

/// Phase 3 numeric fast path: decide whether a whitespace-collapsed
/// integer-hierarchy or `xs:decimal` lexical is *definitively valid* against
/// `facets`, without building a `BigInt` or an `XmlValue`.
///
/// Returns `Some(())` only when the value is valid (so the caller can accept it
/// with nothing materialized). Returns `None` in every other case — invalid
/// lexical, any facet violation, or an integer beyond `i128` — so the caller
/// falls back to [`validate_simple_type`], the single source of truth for the
/// *diagnostic*. Deferring errors rather than synthesizing them keeps the
/// emitted constraint code / message byte-identical to the PSVI-on path: the
/// built-in integer validators reject out-of-range values via native parse /
/// `RangeError` (e.g. `cvc-datatype-valid`), not via the bound facets this fast
/// check consults. The win is on the common valid path; the error path is cold.
fn validate_numeric_lexical(
    code: XmlTypeCode,
    normalized: &str,
    facets: &crate::types::facets::FacetSet,
) -> Option<()> {
    // Integer-lexical gate: `parse::<i128>` accepts exactly the XSD integer
    // lexical space and rejects fractional/garbage input. `Err` is either an
    // invalid lexical or an integer that does not fit `i128`; both defer to the
    // full (`BigInt`) path. (`xs:decimal` is gated separately just below.)
    if code != XmlTypeCode::Decimal && normalized.parse::<i128>().is_err() {
        return None;
    }
    // `xs:decimal` lexical gate: `parse::<Decimal>` below is NOT a lexical
    // check — `rust_decimal` accepts forms outside the XSD decimal lexical
    // space (scientific notation as of 1.42). Defer those to the full path,
    // where `DecimalValidator` rejects them with the proper diagnostic.
    if code == XmlTypeCode::Decimal
        && !crate::types::validators::is_valid_xsd_decimal_lexical(normalized)
    {
        return None;
    }

    // Value-space facets (totalDigits / fractionDigits / min* / max*) need the
    // value as a `Decimal`, and only one that holds it exactly will do: a form
    // of more than 28 digits may need more than `Decimal` carries (it would be
    // rounded), so it defers to the full path, which decides on the exact
    // value — and, for `xs:decimal`, reports the capacity notice. (Never
    // `Decimal::from_i128`, whose num-traits default downcasts through `i64`.)
    if normalized.bytes().filter(u8::is_ascii_digit).count()
        > crate::types::exact_decimal::EXACT_DIGITS
    {
        return None;
    }
    match normalized.parse::<Decimal>() {
        Ok(d) if facets.validate_decimal(&d).is_err() => return None,
        Ok(_) => {}
        Err(_) => return None,
    }

    // Lexical pattern / enumeration facets. A lexically matching enumeration
    // member is also a value match; a mismatch defers to the full path, which
    // compares enumerations by value.
    if facets.validate_string_patterns_enums(normalized).is_err() {
        return None;
    }
    Some(())
}

fn validate_simple_type_impl(
    value: &str,
    type_key: TypeKey,
    schema_set: &SchemaSet,
    resolver: Option<PrefixResolver<'_>>,
) -> Result<SimpleTypeResult, ValidationError> {
    match type_key {
        TypeKey::Simple(sk) => {
            let st_data = match schema_set.arenas.simple_types.get(sk) {
                Some(d) => d,
                None => {
                    return Err(errors::error(
                        "cvc-simple-type",
                        "Simple type definition not found",
                        None,
                    ));
                }
            };
            match st_data.variety {
                SimpleTypeVariety::Atomic => validate_atomic_type(value, sk, schema_set, resolver),
                SimpleTypeVariety::List => validate_list_type(value, sk, schema_set, resolver),
                SimpleTypeVariety::Union => validate_union_type(value, sk, schema_set, resolver),
            }
        }
        TypeKey::Complex(ck) => {
            // §3.4.2.2 clause 1.1: the inline <xs:simpleType> inside
            // simpleContent/restriction is the content type when present;
            // otherwise inherit by walking resolved_base_type.
            let ct_data = match schema_set.arenas.complex_types.get(ck) {
                Some(d) => d,
                None => {
                    return Err(errors::error(
                        "cvc-simple-type",
                        "Complex type definition not found",
                        None,
                    ));
                }
            };
            let restriction = match &ct_data.content {
                ComplexContentResult::Simple(sc)
                    if sc.derivation == DerivationMethod::Restriction =>
                {
                    Some(sc)
                }
                _ => None,
            };
            // String Valid (§3.16.4) clause 1: the ·normalized value· is
            // calculated "using the whiteSpace facet … associated with T", the
            // content type's simple type definition — the most derived
            // whiteSpace, so a restriction's own one comes first, and the base
            // content type and every facet of the chain see the normalized
            // string (a base's whiteSpace is never stronger: whiteSpace valid
            // restriction, Datatypes §4.3.6.4).
            let own_whitespace = restriction
                .and_then(|sc| sc.facets.whitespace.as_ref())
                .map(|w| w.value);
            let value = match own_whitespace {
                Some(mode) => normalize_whitespace(value, mode),
                None => std::borrow::Cow::Borrowed(value),
            };
            let value = value.as_ref();
            let mut result = if let Some(inline_st) = ct_data.resolved_simple_content_type {
                validate_simple_type_inner(value, inline_st, schema_set, resolver)?
            } else if let Some(base_key) = ct_data.resolved_base_type {
                validate_simple_type_inner(value, base_key, schema_set, resolver)?
            } else {
                // No base type — treat as anySimpleType (accept any value)
                SimpleTypeResult {
                    typed_value: XmlValue::untyped(value),
                    member_type: None,
                    normalized_value: None,
                }
            };
            if own_whitespace.is_some() && result.normalized_value.is_none() {
                result.normalized_value = Some(value.to_owned());
            }
            // §3.4.2.2 clause 1.2: a simpleContent restriction may declare
            // local facets that further constrain the recursively validated
            // value. The base/inline call already applied the base type's
            // facets; here we apply only the restriction's local set, as the
            // facets of a simple type. Extension cannot add facets, so it is
            // excluded.
            if let Some(sc) = restriction {
                if !sc.facets.is_empty() {
                    let lex = result.normalized_value.as_deref().unwrap_or(value);
                    check_simple_content_facets(lex, &result.typed_value, &sc.facets, schema_set)?;
                }
            }
            Ok(result)
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// A facet error as the instance validation error of its constraint.
fn facet_error(error: crate::error::FacetError) -> ValidationError {
    let code = errors::facet_constraint_code(&error);
    errors::from_facet_error(code, error, None)
}

/// The whiteSpace of an atomic simple type: its effective `whiteSpace` facet,
/// otherwise its datatype's (Datatypes §4.3.6: `collapse` for every atomic
/// datatype but `string`), and `preserve` without a datatype (Structures
/// §3.1.4: for `xs:anySimpleType` "the ·normalized value· must be determined
/// as in the preserve case").
fn atomic_whitespace(sk: SimpleTypeKey, schema_set: &SchemaSet) -> WhitespaceMode {
    schema_set
        .effective_facets(sk)
        .whitespace
        .as_ref()
        .map(|w| w.value)
        .or_else(|| {
            resolve_type_code(sk, schema_set)
                .and_then(|code| VALIDATOR_REGISTRY.get_by_code(code))
                .map(|validator| validator.whitespace())
        })
        .unwrap_or(WhitespaceMode::Preserve)
}

/// The ·normalized value· of `literal` as a lexical form of `type_key`
/// (Structures §3.1.4: normalized "according to the value of the whiteSpace
/// facet … associated with" the type) — the string its lexical mapping maps:
/// for an atomic type with the type's whiteSpace, for a list collapsed (each
/// item is then free of white space), for a union with the whiteSpace of the
/// member type that accepts it (Datatypes §4.3.6: "the normalization
/// behavior of ·union· types is controlled by the value of whiteSpace on that
/// one of the ·basic members· against which the ·union· is successfully
/// validated"), and for a complex type with simple content as for its
/// {content type}'s simple type definition, whose whiteSpace is the most
/// derived one along its restrictions. `None` when no member of a union
/// accepts the literal.
pub(crate) fn normalized_literal<'a>(
    literal: &'a str,
    type_key: TypeKey,
    schema_set: &SchemaSet,
) -> Option<std::borrow::Cow<'a, str>> {
    let mut current = type_key;
    for _ in 0..100 {
        match current {
            TypeKey::Simple(sk) => {
                let st = schema_set.arenas.simple_types.get(sk)?;
                return match st.variety {
                    SimpleTypeVariety::Atomic => Some(normalize_whitespace(
                        literal,
                        atomic_whitespace(sk, schema_set),
                    )),
                    SimpleTypeVariety::List => {
                        Some(normalize_whitespace(literal, WhitespaceMode::Collapse))
                    }
                    SimpleTypeVariety::Union => {
                        st.resolved_member_types.iter().find_map(|&member| {
                            let result =
                                validate_simple_type_inner(literal, member, schema_set, None)
                                    .ok()?;
                            Some(match result.normalized_value {
                                Some(normalized) if normalized != literal => {
                                    std::borrow::Cow::Owned(normalized)
                                }
                                _ => std::borrow::Cow::Borrowed(literal),
                            })
                        })
                    }
                };
            }
            TypeKey::Complex(ck) => {
                let ct = schema_set.arenas.complex_types.get(ck)?;
                if let ComplexContentResult::Simple(sc) = &ct.content {
                    if sc.derivation == DerivationMethod::Restriction {
                        if let Some(whitespace) = &sc.facets.whitespace {
                            return Some(normalize_whitespace(literal, whitespace.value));
                        }
                    }
                }
                current = match ct.resolved_simple_content_type.or(ct.resolved_base_type) {
                    Some(next) => next,
                    None => return Some(std::borrow::Cow::Borrowed(literal)),
                };
            }
        }
    }
    Some(std::borrow::Cow::Borrowed(literal))
}

/// The whiteSpace that normalizes every lexical form of `type_key`, as
/// [`normalized_literal`] applies it: an atomic type's, `collapse` for a list,
/// a complex type's simple content's (its restriction's own facet, or its
/// content type's) — `None` for a union, whose normalization is that of the
/// member type that accepts the literal, and for a type that is not found.
#[cfg(feature = "xsd11")]
pub(crate) fn content_whitespace(
    type_key: TypeKey,
    schema_set: &SchemaSet,
) -> Option<WhitespaceMode> {
    let mut current = type_key;
    for _ in 0..100 {
        match current {
            TypeKey::Simple(sk) => {
                return match schema_set.arenas.simple_types.get(sk)?.variety {
                    SimpleTypeVariety::Atomic => Some(atomic_whitespace(sk, schema_set)),
                    SimpleTypeVariety::List => Some(WhitespaceMode::Collapse),
                    SimpleTypeVariety::Union => None,
                };
            }
            TypeKey::Complex(ck) => {
                let ct = schema_set.arenas.complex_types.get(ck)?;
                if let ComplexContentResult::Simple(sc) = &ct.content {
                    if sc.derivation == DerivationMethod::Restriction {
                        if let Some(whitespace) = &sc.facets.whitespace {
                            return Some(whitespace.value);
                        }
                    }
                }
                current = match ct.resolved_simple_content_type.or(ct.resolved_base_type) {
                    Some(next) => next,
                    None => return Some(WhitespaceMode::Preserve),
                };
            }
        }
    }
    Some(WhitespaceMode::Preserve)
}

/// `text` normalized with `mode` when that changes it.
#[cfg(feature = "xsd11")]
pub(crate) fn whitespace_changes(text: &str, mode: WhitespaceMode) -> Option<String> {
    match normalize_whitespace(text, mode) {
        std::borrow::Cow::Borrowed(same) if same.len() == text.len() => None,
        normalized => Some(normalized.into_owned()),
    }
}

/// The [schema normalized value] of a valid value `text` of `type_key` — its
/// ·normalized value· with respect to the type's whiteSpace (as
/// [`normalized_literal`] gives it) — when it differs from `text`; `None` when
/// `text` is already normalized.
#[cfg(feature = "xsd11")]
pub(crate) fn schema_normalized(
    text: &str,
    type_key: TypeKey,
    schema_set: &SchemaSet,
) -> Option<String> {
    match content_whitespace(type_key, schema_set) {
        Some(mode) => whitespace_changes(text, mode),
        None => {
            let normalized = normalized_literal(text, type_key, schema_set)?;
            (normalized != text).then(|| normalized.into_owned())
        }
    }
}

/// The facets of a complex type's simple content restriction, checked on the
/// value its base content type gave (`typed`, from `lex`, the string
/// normalized with the restriction's whiteSpace — see
/// [`validate_simple_type_inner`]). Structures 1.1 §3.4.2.2: the {content type}'s simple type
/// definition has "{facets} a set of facet components corresponding to the
/// appropriate element information items among the <restriction>'s [children]
/// …, as defined in Simple Type Restriction (Facets) (§3.16.6.4)" — so they
/// are checked as a simple type's are: an atomic value by its datatype's
/// validator (bounds and enumeration in its order and equality, digits,
/// patterns on the lexical form), a list value by its items, a union value by
/// the member type that validated it, and the XSD 1.1 assertions on the
/// value. A value without a datatype (`xs:anySimpleType` content) is checked
/// as a string.
fn check_simple_content_facets(
    lex: &str,
    typed: &XmlValue,
    facets: &FacetSet,
    schema_set: &SchemaSet,
) -> Result<(), ValidationError> {
    let base_type = typed
        .schema_type
        .and_then(|sk| schema_set.arenas.simple_types.get(sk));
    match &typed.value {
        XmlValueKind::List { items, .. } => match base_type.and_then(|st| st.resolved_item_type) {
            Some(item_type) => check_list_facets(
                facets,
                &normalize_whitespace(lex, WhitespaceMode::Collapse),
                items,
                item_type,
                schema_set,
            )?,
            None => facets.validate_string(lex).map_err(facet_error)?,
        },
        XmlValueKind::Union(member) => match base_type {
            Some(st) => check_union_facets(
                facets,
                None,
                lex,
                member,
                &st.resolved_member_types,
                schema_set,
            )?,
            None => facets.validate_string(lex).map_err(facet_error)?,
        },
        XmlValueKind::Atomic(XmlAtomicValue::QName(name) | XmlAtomicValue::Notation(name)) => {
            check_qname_facets(
                &normalize_whitespace(lex, WhitespaceMode::Collapse),
                name,
                matches!(
                    typed.value,
                    XmlValueKind::Atomic(XmlAtomicValue::Notation(_))
                ),
                facets,
                None,
                schema_set,
            )?
        }
        XmlValueKind::Atomic(_) => match VALIDATOR_REGISTRY.get_by_code(typed.type_code) {
            Some(validator) => {
                let normalized = normalize_whitespace(
                    lex,
                    facets
                        .whitespace
                        .as_ref()
                        .map(|w| w.value)
                        .unwrap_or_else(|| validator.whitespace()),
                );
                let xsd10 = schema_set
                    .is_xsd10()
                    .then(|| xsd10_nan_bound_facets(facets, typed.type_code, &normalized))
                    .flatten();
                validator
                    .validate_with_facets(lex, xsd10.as_ref().unwrap_or(facets))
                    .map_err(|e| errors::from_value_error_default(e, None))?;
            }
            None => facets.validate_string(lex).map_err(facet_error)?,
        },
        _ => facets.validate_string(lex).map_err(facet_error)?,
    }
    #[cfg(feature = "xsd11")]
    evaluate_assertion_facets(typed, facets, schema_set)?;
    Ok(())
}

/// XSD 1.0 §3.2.4 (float; §3.2.5 double): "NaN equals itself but is
/// ·incomparable· with (neither greater than nor less than) any other value",
/// so with "NaN" as the value of an inclusive bound "the result is a ·value
/// space· … having NaN as its only member". [`FacetSet::validate_float`] /
/// [`FacetSet::validate_double`] implement Datatypes 1.1, where NaN is
/// incomparable with itself too. For the NaN value (`normalized`) of an
/// `xs:float` / `xs:double` type in XSD 1.0 mode, the facets without the
/// inclusive bounds whose value is NaN, which it satisfies; `None` when there
/// is nothing to take out.
fn xsd10_nan_bound_facets(
    facets: &FacetSet,
    type_code: XmlTypeCode,
    normalized: &str,
) -> Option<FacetSet> {
    if !matches!(type_code, XmlTypeCode::Float | XmlTypeCode::Double) || normalized != "NaN" {
        return None;
    }
    let is_nan = |literal: &str| normalize_whitespace(literal, WhitespaceMode::Collapse) == "NaN";
    let min = facets
        .min_inclusive
        .as_ref()
        .is_some_and(|f| is_nan(&f.value));
    let max = facets
        .max_inclusive
        .as_ref()
        .is_some_and(|f| is_nan(&f.value));
    if !min && !max {
        return None;
    }
    let mut admitted = facets.clone();
    if min {
        admitted.min_inclusive = None;
    }
    if max {
        admitted.max_inclusive = None;
    }
    Some(admitted)
}

/// XSD 1.0 §3.2.7: "'-0001' is the lexical representation of the year 1
/// Before Common Era (1 BCE …)" and the year 0000 is disallowed, so a 1.0
/// year before year 1 is one later than the 1.1 reading the validators order
/// in (Datatypes 1.1 §3.3.7: "'0000' … 1 BCE"). For a `xs:dateTime`,
/// `xs:date`, `xs:gYear` or `xs:gYearMonth` value in XSD 1.0 mode where the
/// value or a bound or enumeration value has a negative year, the facets
/// without the bounds and the enumeration, which
/// [`check_xsd10_year_facets`] then checks in the 1.0 numbering; `None`
/// when no negative year is involved.
fn xsd10_year_facets(
    facets: &FacetSet,
    type_code: XmlTypeCode,
    normalized: &str,
) -> Option<FacetSet> {
    if !matches!(
        type_code,
        XmlTypeCode::DateTime | XmlTypeCode::Date | XmlTypeCode::GYear | XmlTypeCode::GYearMonth
    ) {
        return None;
    }
    let negative = |literal: &str| literal.trim_start().starts_with('-');
    let bounds = [
        facets.min_inclusive.as_ref().map(|f| f.value.as_str()),
        facets.max_inclusive.as_ref().map(|f| f.value.as_str()),
        facets.min_exclusive.as_ref().map(|f| f.value.as_str()),
        facets.max_exclusive.as_ref().map(|f| f.value.as_str()),
    ];
    let enumerated = facets
        .enumeration
        .as_ref()
        .map(|e| e.values.iter().any(|v| negative(v)));
    let has_facet = bounds.iter().any(Option::is_some) || enumerated.is_some();
    let involved = negative(normalized)
        || bounds.iter().flatten().any(|b| negative(b))
        || enumerated == Some(true);
    if !has_facet || !involved {
        return None;
    }
    let mut rest = facets.clone();
    rest.min_inclusive = None;
    rest.max_inclusive = None;
    rest.min_exclusive = None;
    rest.max_exclusive = None;
    rest.enumeration = None;
    Some(rest)
}

/// The bounds and enumeration [`xsd10_year_facets`] took out, on the value
/// `val` (written `raw`), in the XSD 1.0 year numbering.
fn check_xsd10_year_facets(
    val: &XmlValue,
    facets: &FacetSet,
    type_code: XmlTypeCode,
    raw: &str,
) -> Result<(), ValidationError> {
    use crate::error::FacetError;
    use crate::types::value_order::literal_cmp_in;
    use std::cmp::Ordering::{Equal, Greater, Less};
    let XmlValueKind::Atomic(atom) = &val.value else {
        return Ok(());
    };
    let order = |literal: &str| {
        VALIDATOR_REGISTRY
            .validate(type_code, literal)
            .ok()
            .and_then(|bound| match bound.value {
                XmlValueKind::Atomic(bound) => literal_cmp_in(true, atom, raw, &bound, literal),
                _ => None,
            })
    };
    let value = raw.to_string();
    if let Some(f) = &facets.min_inclusive {
        if !matches!(order(&f.value), Some(Greater | Equal)) {
            return Err(facet_error(FacetError::MinInclusiveViolation {
                value,
                min: f.value.clone(),
            }));
        }
    }
    if let Some(f) = &facets.max_inclusive {
        if !matches!(order(&f.value), Some(Less | Equal)) {
            return Err(facet_error(FacetError::MaxInclusiveViolation {
                value,
                max: f.value.clone(),
            }));
        }
    }
    if let Some(f) = &facets.min_exclusive {
        if order(&f.value) != Some(Greater) {
            return Err(facet_error(FacetError::MinExclusiveViolation {
                value,
                min: f.value.clone(),
            }));
        }
    }
    if let Some(f) = &facets.max_exclusive {
        if order(&f.value) != Some(Less) {
            return Err(facet_error(FacetError::MaxExclusiveViolation {
                value,
                max: f.value.clone(),
            }));
        }
    }
    if let Some(enumeration) = &facets.enumeration {
        if !enumeration.values.iter().any(|v| order(v) == Some(Equal)) {
            return Err(facet_error(FacetError::enumeration(raw)));
        }
    }
    Ok(())
}

/// Walk the resolved_base_type chain from `sk` until we find a built-in type code.
/// Returns `None` if no built-in ancestor is found (cycle guard at depth 100).
fn resolve_type_code(sk: SimpleTypeKey, schema_set: &SchemaSet) -> Option<XmlTypeCode> {
    let mut current = sk;
    for _ in 0..100 {
        if let Some(code) = schema_set.get_type_code(current) {
            return Some(code);
        }
        let st_data = schema_set.arenas.simple_types.get(current)?;
        match st_data.resolved_base_type {
            Some(TypeKey::Simple(base)) => current = base,
            _ => return None,
        }
    }
    None
}

// Effective (base-chain-merged) facet sets are memoized by
// `SchemaSet::effective_facets`; the hot path clones a cached `Arc<FacetSet>`
// instead of re-walking the derivation chain per value.

// ---------------------------------------------------------------------------
// Atomic
// ---------------------------------------------------------------------------

fn validate_atomic_type(
    value: &str,
    sk: SimpleTypeKey,
    schema_set: &SchemaSet,
    resolver: Option<PrefixResolver<'_>>,
) -> Result<SimpleTypeResult, ValidationError> {
    let type_code = resolve_type_code(sk, schema_set);

    // Short-circuit for abstract base types — accept as untyped
    match type_code {
        Some(XmlTypeCode::AnySimpleType) | Some(XmlTypeCode::AnyAtomicType) | None => {
            return Ok(SimpleTypeResult {
                typed_value: XmlValue::untyped(value),
                member_type: None,
                normalized_value: None,
            });
        }
        _ => {}
    }
    let type_code = type_code.unwrap();

    let validator = match VALIDATOR_REGISTRY.get_by_code(type_code) {
        Some(v) => v,
        None => {
            return Err(errors::error(
                "cvc-datatype-valid",
                format!("No validator for type code {:?}", type_code),
                None,
            ));
        }
    };

    let facets = schema_set.effective_facets(sk);

    // XSD Part 2 §3.2 / W3C bug 14388: `xs:NOTATION` is only usable via
    // restriction with an `enumeration` facet binding the value to a
    // declared notation. The schema may legally reference `xs:NOTATION`
    // directly, but any instance value must fail because there is no
    // enumeration to match against.
    if type_code == XmlTypeCode::Notation && facets.enumeration.is_none() {
        return Err(errors::error(
            "cvc-datatype-valid",
            "xs:NOTATION cannot be used directly for instance validation; \
             only datatypes derived from NOTATION by restriction with an \
             enumeration facet are permitted (§3.2)",
            None,
        ));
    }

    // An `xs:QName` / `xs:NOTATION` value whose prefix the caller's bindings
    // resolve is validated in the value space.
    if let (Some(resolver), XmlTypeCode::QName | XmlTypeCode::Notation) = (resolver, type_code) {
        if let Some(result) = validate_resolved_qname(
            value, sk, type_code, validator, &facets, resolver, schema_set,
        )? {
            return Ok(result);
        }
    }

    // Compute normalized value (PSVI [schema normalized value])
    let effective_ws = facets
        .whitespace
        .as_ref()
        .map(|w| w.value)
        .unwrap_or_else(|| validator.whitespace());
    let normalized = normalize_whitespace(value, effective_ws);

    let year_facets = schema_set
        .is_xsd10()
        .then(|| xsd10_year_facets(&facets, type_code, &normalized))
        .flatten();
    let xsd10 = schema_set
        .is_xsd10()
        .then(|| xsd10_nan_bound_facets(&facets, type_code, &normalized))
        .flatten()
        .or_else(|| year_facets.clone());
    let typed_value = if facets.is_empty() {
        validator.validate(value)
    } else {
        validator.validate_with_facets(value, xsd10.as_ref().unwrap_or(&facets))
    };

    match typed_value {
        Ok(mut val) => {
            // Propagate schema type so XPath2 sequence type matching works
            if val.schema_type.is_none() {
                val.schema_type = Some(sk);
            }
            // XSD 1.0 §3.2.7.2: year zero ("0000") is not allowed in dateTime,
            // date, gYear, or gYearMonth lexical forms. XSD 1.1 explicitly
            // permits 0000 as the representation of 1 BCE.
            // XSD 1.0 §3.2.4 / §3.2.5 also omit `+INF` from the special
            // float / double lexicals enumerated by XSD 1.1 §3.3.16/17.
            if schema_set.is_xsd10() {
                check_xsd10_year_zero(&val, value)?;
                check_xsd10_plus_inf(&val, value)?;
                if year_facets.is_some() {
                    check_xsd10_year_facets(&val, &facets, type_code, value)?;
                }
            }
            // XSD 1.1: evaluate assertion facets
            #[cfg(feature = "xsd11")]
            evaluate_assertion_facets(&val, &facets, schema_set)?;
            Ok(SimpleTypeResult {
                typed_value: val,
                member_type: None,
                normalized_value: Some(normalized.into_owned()),
            })
        }
        Err(type_err) => Err(errors::from_value_error_default(type_err, None)),
    }
}

/// An `xs:QName` / `xs:NOTATION` value of the atomic type `sk` (whose
/// built-in type is `type_code`) in the value space: the lexical form is
/// checked as without a namespace context (the same errors), the prefix is
/// resolved with `resolver`, then the facets see the expanded name
/// ([`check_qname_facets`]) and the assertions the QName (NOTATION) value.
/// `Ok(None)` when `resolver` binds the prefix nowhere: the value is then
/// validated in its lexical form, as without bindings.
fn validate_resolved_qname(
    value: &str,
    sk: SimpleTypeKey,
    type_code: XmlTypeCode,
    validator: &dyn crate::types::validators::TypeValidator,
    facets: &FacetSet,
    resolver: PrefixResolver<'_>,
    schema_set: &SchemaSet,
) -> Result<Option<SimpleTypeResult>, ValidationError> {
    let lexical = validator
        .validate(value)
        .map_err(|e| errors::from_value_error_default(e, None))?
        .to_string_value();
    let (prefix, local) = match lexical.split_once(':') {
        Some((prefix, local)) => (Some(prefix), local),
        None => (None, lexical.as_str()),
    };
    let Some(namespace) = resolver(prefix) else {
        return Ok(None);
    };
    let names = &schema_set.name_table;
    let name = QNameValue::new(
        QualifiedName::new(namespace, names.add(local), prefix.map(|p| names.add(p))),
        names,
    );
    check_qname_facets(
        &lexical,
        &name,
        type_code == XmlTypeCode::Notation,
        facets,
        Some(sk),
        schema_set,
    )?;
    let atom = if type_code == XmlTypeCode::Notation {
        XmlAtomicValue::Notation(name)
    } else {
        XmlAtomicValue::QName(name)
    };
    let typed_value = XmlValue::with_schema_type(type_code, sk, XmlValueKind::Atomic(atom));
    #[cfg(feature = "xsd11")]
    evaluate_assertion_facets(&typed_value, facets, schema_set)?;
    Ok(Some(SimpleTypeResult {
        typed_value,
        member_type: None,
        normalized_value: Some(lexical),
    }))
}

/// XSD 1.0 §3.2.4 / §3.2.5: `+INF` is not in the lexical space (added in
/// Datatypes 1.1). The collapsed value is checked against the original
/// lexical form (after whitespace collapse, since `xs:float` / `xs:double`
/// declare `whiteSpace = collapse`).
fn check_xsd10_plus_inf(val: &XmlValue, raw: &str) -> Result<(), ValidationError> {
    use crate::types::value::XmlAtomicValue;
    let is_float_or_double = matches!(
        val.value,
        XmlValueKind::Atomic(XmlAtomicValue::Float(_))
            | XmlValueKind::Atomic(XmlAtomicValue::Double(_))
    );
    if !is_float_or_double {
        return Ok(());
    }
    let collapsed = crate::types::facets::normalize_whitespace(
        raw,
        crate::types::facets::WhitespaceMode::Collapse,
    );
    if collapsed == "+INF" {
        Err(errors::error(
            "cvc-datatype-valid",
            format!(
                "'+INF' is not a valid XSD 1.0 float/double lexical form (value '{}')",
                raw
            ),
            None,
        ))
    } else {
        Ok(())
    }
}

/// XSD 1.0 §3.2.7.2: reject year 0000 in dateTime / date / gYear / gYearMonth.
fn check_xsd10_year_zero(val: &XmlValue, raw: &str) -> Result<(), ValidationError> {
    use crate::types::value::XmlAtomicValue;
    let year_zero = match &val.value {
        XmlValueKind::Atomic(XmlAtomicValue::DateTime(v)) => v.year == 0,
        XmlValueKind::Atomic(XmlAtomicValue::Date(v)) => v.year == 0,
        XmlValueKind::Atomic(XmlAtomicValue::GYear(v)) => v.year == 0,
        XmlValueKind::Atomic(XmlAtomicValue::GYearMonth(v)) => v.year == 0,
        _ => false,
    };
    if year_zero {
        Err(errors::error(
            "cvc-datatype-valid",
            format!(
                "Year 0000 is not a valid XSD 1.0 lexical form (value '{}'); use '-0001' for 1 BCE",
                raw
            ),
            None,
        ))
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// List
// ---------------------------------------------------------------------------

fn validate_list_type(
    value: &str,
    sk: SimpleTypeKey,
    schema_set: &SchemaSet,
    resolver: Option<PrefixResolver<'_>>,
) -> Result<SimpleTypeResult, ValidationError> {
    let st_data = match schema_set.arenas.simple_types.get(sk) {
        Some(d) => d,
        None => {
            return Err(errors::error(
                "cvc-simple-type",
                "List type definition not found",
                None,
            ));
        }
    };

    let item_type_key = match st_data.resolved_item_type {
        Some(tk) => tk,
        None => {
            if let Some(deferred) = &st_data.deferred_item_type_error {
                return Err(errors::error("src-resolve", deferred.message.clone(), None));
            }
            // No item type resolved — cannot validate list items, accept as untyped
            return Ok(SimpleTypeResult {
                typed_value: XmlValue::untyped(value),
                member_type: None,
                normalized_value: None,
            });
        }
    };

    // Normalize whitespace (collapse) and split
    let normalized = normalize_whitespace(value, WhitespaceMode::Collapse);
    let items_str: Vec<&str> = if normalized.is_empty() {
        Vec::new()
    } else {
        normalized.split(' ').collect()
    };

    // Validate each item. A member keeps the typed value its validation gave
    // it — its own type code and `schema_type` — and, when the item type is a
    // union, the value of the member type that accepted it (XPath 2.0 §2.5.2:
    // "when the type annotation of a node is a union type, the typed value of
    // the node will be an instance of one of the member types of the union").
    let mut items = Vec::with_capacity(items_str.len());
    for item_str in &items_str {
        let result = validate_simple_type_inner(item_str, item_type_key, schema_set, resolver)?;
        items.push(list_member(result.typed_value));
    }
    // The declared item type's code; a union's is that of its base type,
    // `xs:anySimpleType`.
    let item_type_code = match item_type_key {
        TypeKey::Simple(item_sk) => resolve_type_code(item_sk, schema_set),
        TypeKey::Complex(_) => None,
    }
    .unwrap_or(XmlTypeCode::UntypedAtomic);

    // Check list-level facets (length, minLength, maxLength, pattern, enumeration)
    let facets = schema_set.effective_facets(sk);
    if !facets.is_empty() {
        check_list_facets(&facets, &normalized, &items, item_type_key, schema_set)?;
    }

    // Use the list type's own code (e.g. IdRefs, NmTokens, Entities) when it
    // has a built-in one, so that downstream consumers like collect_id_idref
    // can distinguish list types from their item types.
    let list_type_code = resolve_type_code(sk, schema_set)
        .filter(|code| code.is_list())
        .unwrap_or(item_type_code);

    let typed_value = XmlValue::with_schema_type(
        list_type_code,
        sk,
        XmlValueKind::List {
            item_type: item_type_code,
            items,
        },
    );

    // XSD 1.1: evaluate assertion facets ($value is the sequence of list items)
    #[cfg(feature = "xsd11")]
    evaluate_assertion_facets(&typed_value, &facets, schema_set)?;

    Ok(SimpleTypeResult {
        typed_value,
        member_type: None,
        normalized_value: Some(normalized.into_owned()),
    })
}

/// The facets of a list type on a list value: its whitespace-collapsed form
/// `normalized`, its members `items` (as [`list_member`] gives them) and its
/// item type.
fn check_list_facets(
    facets: &FacetSet,
    normalized: &str,
    items: &[XmlValue],
    item_type_key: TypeKey,
    schema_set: &SchemaSet,
) -> Result<(), ValidationError> {
    // Length constraints on list are item count
    facets
        .validate_list_length(items.len() as u64)
        .map_err(facet_error)?;

    // Datatypes §4.3.4: a pattern constrains the lexical space — the
    // normalized string.
    facets
        .validate_patterns_only(normalized)
        .map_err(facet_error)?;

    // Datatypes §4.3.5.4 / §2.4.1.2: an enumeration value is a whole list,
    // compared with the instance's value — same length, members pairwise
    // equal in the value space of the type that validated each. A
    // lexically identical enumeration value is a match without parsing —
    // unless the members are QName values: the same spelling can name
    // other names under other bindings. Each literal is read with the
    // namespace bindings of the schema element it was written on. (A member
    // whose prefix is bound nowhere stays lexical; the list is then compared
    // as without bindings, and the caller reports the prefix.)
    if let Some(enumeration) = &facets.enumeration {
        let qname_members =
            items.iter().any(holds_qname_value) && !items.iter().any(holds_lexical_qname);
        if qname_members || !enumeration.values.contains(normalized) {
            facets
                .validate_enum_value_space(
                    |literal| {
                        literal_readings(enumeration, literal, &schema_set.name_table, &|reading| {
                            list_literal_matches(
                                literal,
                                items,
                                normalized,
                                item_type_key,
                                schema_set,
                                reading,
                            )
                        })
                    },
                    normalized,
                )
                .map_err(facet_error)?;
        }
    }
    Ok(())
}

/// Whether the list value `literal` — an enumeration value of a list type —
/// is the list whose members are `members`: same length, and each item of the
/// literal, validated against the item type, equal or identical to the member
/// at its position (Datatypes §2.4.1.2). An item that does not validate
/// matches nothing.
fn list_literal_matches(
    literal: &str,
    members: &[XmlValue],
    instance: &str,
    item_type_key: TypeKey,
    schema_set: &SchemaSet,
    resolver: Option<PrefixResolver<'_>>,
) -> bool {
    let collapsed = normalize_whitespace(literal, WhitespaceMode::Collapse);
    let tokens: Vec<&str> = if collapsed.is_empty() {
        Vec::new()
    } else {
        collapsed.split(' ').collect()
    };
    tokens.len() == members.len()
        && tokens
            .iter()
            .zip(members.iter().zip(instance.split_ascii_whitespace()))
            .all(|(token, (member, member_lexical))| {
                validate_simple_type_inner(token, item_type_key, schema_set, resolver).is_ok_and(
                    |item| {
                        super::identity::literal_values_equal_or_identical(
                            &list_member(item.typed_value),
                            token,
                            member,
                            member_lexical,
                        )
                    },
                )
            })
}

/// The facets of a union type on a value of it: its lexical form `value`, the
/// value `member` the member type that accepted it gave, and the union's
/// member types. `key` is the union type whose effective facets `facets` are
/// (`None` for a simple-content restriction's own facets).
///
/// `pattern` constrains the lexical form. `enumeration` compares values
/// (Datatypes §4.3.5.4): each literal is the value the union's ordered member
/// types give it — "validated against the ·member types· in the order in
/// which they appear in the definition until a match is found" (§2.4.1.3),
/// read with the namespace bindings it was written with — and the instance's
/// value is the one its own member type gave; "no two values from different
/// ·primitive· ·value spaces· are equal" (§2.2.3). A QName / NOTATION value
/// is looked up by expanded name among the literals whose value is one; a
/// value whose prefix is bound nowhere is compared by lexical form, as
/// before (the caller reports the prefix).
fn check_union_facets(
    facets: &FacetSet,
    key: Option<SimpleTypeKey>,
    value: &str,
    member: &XmlValue,
    member_types: &[TypeKey],
    schema_set: &SchemaSet,
) -> Result<(), ValidationError> {
    let check_value = member.to_string_value();
    // §cvc-pattern-valid: patterns apply to the LEXICAL form (original value),
    // not the canonical/parsed form (e.g. "-.5" must match "\-\.\d+" not "-0.5")
    let lex_value = normalize_whitespace(value, WhitespaceMode::Collapse);
    facets
        .validate_patterns_only(&lex_value)
        .map_err(facet_error)?;
    let Some(enumeration) = &facets.enumeration else {
        return Ok(());
    };
    if !holds_list(member) && holds_lexical_qname(member) {
        let type_code = member.type_code;
        return facets
            .validate_enum_value_space(
                |s| {
                    VALIDATOR_REGISTRY
                        .validate(type_code, s)
                        .is_ok_and(|v| v.to_string_value() == check_value)
                },
                &check_value,
            )
            .map_err(facet_error);
    }
    let index = match key.and_then(|sk| schema_set.enumeration_index(sk)) {
        Some(index) => index,
        None => std::sync::Arc::new(union_enumeration_index(
            enumeration,
            member_types,
            schema_set,
        )),
    };
    let admitted = match &unwrap_unions(member).value {
        XmlValueKind::Atomic(XmlAtomicValue::QName(name)) => {
            let name = name.name();
            index.admits(&check_value, false, name.namespace_uri, name.local_name)
        }
        XmlValueKind::Atomic(XmlAtomicValue::Notation(name)) => {
            let name = name.name();
            index.admits(&check_value, true, name.namespace_uri, name.local_name)
        }
        _ => index.values().iter().any(|(literal, literal_value)| {
            union_literal_equals(literal_value, literal, member, &lex_value)
        }),
    };
    if admitted {
        Ok(())
    } else {
        Err(facet_error(crate::error::FacetError::enumeration(
            &check_value,
        )))
    }
}

/// The enumeration index of a union type: every literal's value as the
/// union's ordered member types `member_types` give it (the first that
/// accepts it decides, Datatypes §2.4.1.3), once per namespace snapshot
/// recorded for the literal (a QName member reads its prefix with it); a
/// literal no member accepts has no value.
pub(crate) fn union_enumeration_index(
    enumeration: &EnumerationFacet,
    member_types: &[TypeKey],
    schema_set: &SchemaSet,
) -> crate::types::facets::EnumerationIndex {
    let names = &schema_set.name_table;
    let mut readings: std::collections::HashMap<&str, Vec<&NamespaceContextSnapshot>> =
        std::collections::HashMap::new();
    for (literal, snapshot) in &enumeration.qname_contexts {
        readings.entry(literal.as_str()).or_default().push(snapshot);
    }
    let value_of = |literal: &str, resolver: Option<PrefixResolver<'_>>| {
        member_types.iter().find_map(|&member| {
            validate_simple_type_inner(literal, member, schema_set, resolver)
                .ok()
                .map(|result| result.typed_value)
        })
    };
    let mut index = crate::types::facets::EnumerationIndex::default();
    for literal in &enumeration.values {
        match readings.get(literal.as_str()) {
            Some(snapshots) => {
                for snapshot in snapshots {
                    let resolver = snapshot_prefixes(snapshot, names);
                    if let Some(value) = value_of(literal, Some(&resolver)) {
                        index.push_value(literal, value);
                    }
                }
            }
            None => {
                if let Some(value) = value_of(literal, None) {
                    if !holds_list(&value) && holds_lexical_qname(&value) {
                        index.push_lexical(literal);
                    }
                    index.push_value(literal, value);
                }
            }
        }
    }
    index
}

/// Whether the union literal value `literal` equals the instance value
/// `value` (Datatypes §4.3.5.4, "equal or identical"): the comparison every
/// value constraint and identity constraint makes
/// ([`values_equal_or_identical`](super::identity::values_equal_or_identical))
/// — lists member for member; atomic values only within one primitive
/// datatype (§2.2.3: "no two values from different ·primitive· ·value
/// spaces· are equal"), each in its value space: decimals by exact value,
/// `0` equal to `-0`, date/time values on the time line (§D.2.1), durations
/// by months and seconds, QName / NOTATION values by expanded name — as an
/// atomic type's own `enumeration` compares them.
fn union_literal_equals(
    literal: &XmlValue,
    literal_lexical: &str,
    value: &XmlValue,
    value_lexical: &str,
) -> bool {
    super::identity::literal_values_equal_or_identical(
        literal,
        literal_lexical,
        value,
        value_lexical,
    )
}

/// `value` with its union wrappers taken off (nested unions included).
fn unwrap_unions(mut value: &XmlValue) -> &XmlValue {
    while let XmlValueKind::Union(inner) = &value.value {
        value = inner;
    }
    value
}

/// Whether `value` is a list value, possibly inside union wrappers.
pub(crate) fn holds_list(value: &XmlValue) -> bool {
    match &value.value {
        XmlValueKind::List { .. } => true,
        XmlValueKind::Union(inner) => holds_list(inner),
        _ => false,
    }
}

/// A list member as the list stores it: the typed value of one item, with the
/// union wrappers taken off (nested unions included), so that the member is
/// the atomic value of the member type that validated it — with that type's
/// code and `schema_type`.
fn list_member(mut value: XmlValue) -> XmlValue {
    loop {
        match value.value {
            XmlValueKind::Union(inner) => value = *inner,
            _ => return value,
        }
    }
}

// ---------------------------------------------------------------------------
// Union
// ---------------------------------------------------------------------------

fn validate_union_type(
    value: &str,
    sk: SimpleTypeKey,
    schema_set: &SchemaSet,
    resolver: Option<PrefixResolver<'_>>,
) -> Result<SimpleTypeResult, ValidationError> {
    let st_data = match schema_set.arenas.simple_types.get(sk) {
        Some(d) => d,
        None => {
            return Err(errors::error(
                "cvc-simple-type",
                "Union type definition not found",
                None,
            ));
        }
    };

    // Try each member type in order (Datatypes §2.4.1.3: "validated against
    // the ·member types· in the order in which they appear in the definition
    // until a match is found"). With namespace bindings, a QName / NOTATION
    // member whose prefix they bind nowhere does not match: the literal has no
    // lexical mapping there (§3.3.18). It is kept, and taken — for the caller
    // to report the prefix (`cvc-datatype-valid.1.2.1`) — only when no later
    // member matches.
    let mut unbound: Option<(SimpleTypeResult, TypeKey)> = None;
    for &member_key in &st_data.resolved_member_types {
        if let Ok(result) = validate_simple_type_inner(value, member_key, schema_set, resolver) {
            if resolver.is_some() && holds_lexical_qname(&result.typed_value) {
                if unbound.is_none() {
                    unbound = Some((result, member_key));
                }
                continue;
            }
            return union_member_result(value, sk, schema_set, result, member_key);
        }
    }
    if let Some((result, member_key)) = unbound {
        return union_member_result(value, sk, schema_set, result, member_key);
    }

    // No member matched
    Err(errors::error(
        "cvc-simple-type",
        format!(
            "Value '{}' does not match any member type of the union",
            value
        ),
        None,
    ))
}

/// The union value of `sk` for the value `result` its member type
/// `member_key` gave: the union-level facets (pattern, enumeration) and
/// assertions on it, the member's typed value wrapped as the union's.
fn union_member_result(
    value: &str,
    sk: SimpleTypeKey,
    schema_set: &SchemaSet,
    result: SimpleTypeResult,
    member_key: TypeKey,
) -> Result<SimpleTypeResult, ValidationError> {
    let st_data = schema_set.arenas.simple_types.get(sk);
    let members = st_data
        .map(|d| d.resolved_member_types.as_slice())
        .unwrap_or_default();
    let facets = schema_set.effective_facets(sk);
    if !facets.is_empty() {
        check_union_facets(
            &facets,
            Some(sk),
            value,
            &result.typed_value,
            members,
            schema_set,
        )?;
    }

    // Propagate schema type on inner value from the matched member type
    let mut inner = result.typed_value;
    if inner.schema_type.is_none() {
        inner.schema_type = member_key.as_simple();
    }

    // XSD 1.1: evaluate assertion facets ($value is the member-validated value)
    #[cfg(feature = "xsd11")]
    evaluate_assertion_facets(&inner, &facets, schema_set)?;

    Ok(SimpleTypeResult {
        normalized_value: result.normalized_value,
        typed_value: XmlValue::with_schema_type(
            inner.type_code,
            sk,
            XmlValueKind::Union(Box::new(inner)),
        ),
        member_type: Some(member_key),
    })
}

// ---------------------------------------------------------------------------
// XSD 1.1: Assertion facet evaluation
// ---------------------------------------------------------------------------

#[cfg(feature = "xsd11")]
pub(crate) fn resolve_assertion_default_ns(
    raw: Option<&str>,
    source: Option<&crate::parser::location::SourceRef>,
    ns_snapshot: &crate::namespace::context::NamespaceContextSnapshot,
    schema_set: &SchemaSet,
) -> Option<crate::ids::NameId> {
    // Look up the schema document that defines this assertion
    let doc = source.and_then(|s| schema_set.documents.get(s.doc_id as usize));

    // Cascade: facet-level > schema-level (of the defining document)
    let effective = if let Some(raw) = raw {
        Some(raw.to_string())
    } else {
        doc.and_then(|d| d.xpath_default_namespace)
            .map(|id| schema_set.name_table.resolve(id))
    };

    match effective.as_deref() {
        Some("##defaultNamespace") => ns_snapshot.default_ns,
        Some("##targetNamespace") => doc.and_then(|d| d.target_namespace),
        Some("##local") | None => None,
        Some(uri) => Some(schema_set.name_table.add(uri)),
    }
}

/// Evaluate assertion facets against a typed value (XSD 1.1).
///
/// Assertion facets on simpleType restrictions receive only the `$value`
/// variable (the typed value being validated). No context node or DOM
/// access is needed, so this evaluates inline during streaming validation.
#[cfg(feature = "xsd11")]
pub(crate) fn evaluate_assertion_facets(
    typed_value: &XmlValue,
    facets: &FacetSet,
    schema_set: &SchemaSet,
) -> Result<(), ValidationError> {
    use crate::xpath::api::XPathExpr;
    use crate::xpath::functions::effective_boolean_value;
    use crate::xpath::{RoXmlNavigator, XPathContext};

    if facets.assertions.is_empty() {
        return Ok(());
    }

    for assertion in &facets.assertions {
        if assertion.test.is_empty() {
            continue;
        }

        // Resolve assertion source location for error reporting
        let location = assertion
            .source
            .as_ref()
            .and_then(|s| schema_set.source_maps.locate(s));

        // Static context: the schema-time check
        // (`schema::validate_assertion_xpath`) refused every user-defined name
        // in the built-ins-only context of XPath Valid; the schema set serves
        // the evaluation, and `without_schema_casts` stays as a second guard.
        let ctx = XPathContext::new(&schema_set.name_table)
            .with_namespaces(assertion.ns_snapshot.clone())
            .with_schema_set(schema_set)
            .without_schema_casts();

        // Set default element namespace from xpathDefaultNamespace cascade
        let ctx = if let Some(default_ns) = resolve_assertion_default_ns(
            assertion.xpath_default_namespace.as_deref(),
            assertion.source.as_ref(),
            &assertion.ns_snapshot,
            schema_set,
        ) {
            ctx.with_default_element_ns(default_ns)
        } else {
            ctx
        };

        // Compile the XPath expression with $value declared
        let expr =
            XPathExpr::compile_with_vars(&assertion.test, &ctx, &["value"]).map_err(|e| {
                errors::error(
                    "cvc-assertion",
                    format!(
                        "Failed to compile assertion test '{}': {}",
                        assertion.test, e
                    ),
                    location.clone(),
                )
            })?;

        // Convert typed value to XPathValue: a list gives its members, each
        // with the type that validated it.
        let xpath_value = typed_value.to_xpath_value::<RoXmlNavigator<'static>>();

        // Evaluate with $value bound; the expression's warnings go to the
        // validation run (`expression_warnings`).
        let warnings = std::cell::RefCell::new(Vec::new());
        let outcome = crate::xpath::operators::with_decimal_capacity_marks(|| {
            expr.evaluator(&ctx)
                .with_warnings(&warnings)
                .run_with::<RoXmlNavigator<'static>, _>(|eval| {
                    eval.set_variable_by_name("value", xpath_value).unwrap();
                })
        });
        super::assertions::note_expression_warnings(
            "assertion",
            &assertion.test,
            warnings.into_inner(),
        );
        let result = match outcome {
            Ok(result) => result,
            // Datatypes 1.1 §5.4: an `xs:decimal` beyond this implementation's
            // capacity "must not be treated as invalid". When the assertion
            // raises its error because it computed with such a value — and
            // only then: the error is marked where that operand is converted
            // — its outcome is unknown, not false; the value is reported with
            // its capacity warning instead. Any other error (an overflow of
            // in-range operands, a division by zero, FOCA0006 for a literal)
            // fails the assertion (Structures §3.13.4.1).
            Err(e) if e.is_decimal_capacity_operand() => {
                continue;
            }
            Err(e) => {
                return Err(errors::error(
                    "cvc-assertion",
                    format!(
                        "Failed to evaluate assertion test '{}': {}",
                        assertion.test, e
                    ),
                    location.clone(),
                ));
            }
        };

        // Effective boolean value must be true
        let ebv = effective_boolean_value(&result).map_err(|e| {
            errors::error(
                "cvc-assertion",
                format!(
                    "Failed to compute boolean value for assertion '{}': {}",
                    assertion.test, e
                ),
                location.clone(),
            )
        })?;

        if !ebv {
            return Err(errors::error(
                "cvc-assertion",
                format!(
                    "Assertion '{}' failed for value '{}'",
                    assertion.test,
                    typed_value.to_string_value()
                ),
                location,
            ));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::load_and_process_schema;

    fn load_schema(xsd: &str) -> SchemaSet {
        let mut schema_set = SchemaSet::new();
        load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None)
            .expect("failed to load schema");
        schema_set
    }

    /// Helper: get the TypeKey for a global element's type
    fn element_type(schema_set: &SchemaSet, local_name: &str) -> TypeKey {
        let name_id = schema_set.name_table.add(local_name);
        let elem_key = schema_set
            .lookup_element(None, name_id)
            .expect("element not found");
        schema_set.arenas.elements[elem_key]
            .resolved_type
            .expect("element has no resolved type")
    }

    /// XSD 1.0's canonical forms (Datatypes 1.0 §2.5.1, §3.2.x.2): built from
    /// the value, the seconds' fraction as written.
    #[test]
    fn xsd10_canonical_forms() {
        let ss = SchemaSet::new();
        let builtin = |code: XmlTypeCode| {
            TypeKey::Simple(ss.builtin_types().get_by_type_code(code).expect("built-in"))
        };
        let form = |literal: &str, code: XmlTypeCode| {
            let value = validate_simple_type(literal, builtin(code), &ss)
                .expect("valid")
                .typed_value;
            xsd10_canonical_form(&value, literal)
        };
        for (literal, code, want) in [
            ("1", XmlTypeCode::Decimal, "1.0"),
            ("+001.500", XmlTypeCode::Decimal, "1.5"),
            ("-0.50", XmlTypeCode::Decimal, "-0.5"),
            ("0", XmlTypeCode::Decimal, "0.0"),
            ("007", XmlTypeCode::Integer, "7"),
            ("100", XmlTypeCode::Float, "1.0E2"),
            ("0.5", XmlTypeCode::Double, "5.0E-1"),
            ("-1234.5", XmlTypeCode::Double, "-1.2345E3"),
            ("1", XmlTypeCode::Double, "1.0E0"),
            ("0", XmlTypeCode::Double, "0.0E0"),
            ("INF", XmlTypeCode::Double, "INF"),
            ("NaN", XmlTypeCode::Float, "NaN"),
            ("1", XmlTypeCode::Boolean, "true"),
            (" a  b ", XmlTypeCode::Token, "a b"),
            ("0fb7", XmlTypeCode::HexBinary, "0FB7"),
            (
                "2000-01-01T00:00:00Z",
                XmlTypeCode::DateTime,
                "2000-01-01T00:00:00Z",
            ),
            (
                "2000-01-01T12:30:00+01:00",
                XmlTypeCode::DateTime,
                "2000-01-01T11:30:00Z",
            ),
            (
                "2000-01-01T00:30:00+01:00",
                XmlTypeCode::DateTime,
                "1999-12-31T23:30:00Z",
            ),
            (
                "2000-01-01T24:00:00",
                XmlTypeCode::DateTime,
                "2000-01-02T00:00:00",
            ),
            (
                "2000-01-01T00:00:01.500",
                XmlTypeCode::DateTime,
                "2000-01-01T00:00:01.5",
            ),
            (
                "2000-01-01T00:00:00.000",
                XmlTypeCode::DateTime,
                "2000-01-01T00:00:00",
            ),
            (
                "2000-01-01T00:00:00.123456789012345678901234567891Z",
                XmlTypeCode::DateTime,
                "2000-01-01T00:00:00.123456789012345678901234567891Z",
            ),
            (
                "-0001-12-31T23:00:00-02:00",
                XmlTypeCode::DateTime,
                "0001-01-01T01:00:00Z",
            ),
            (
                "0001-01-01T00:00:00+01:00",
                XmlTypeCode::DateTime,
                "-0001-12-31T23:00:00Z",
            ),
            ("23:30:00-01:00", XmlTypeCode::Time, "00:30:00Z"),
            ("24:00:00", XmlTypeCode::Time, "00:00:00"),
            ("2002-10-10+13:00", XmlTypeCode::Date, "2002-10-09-11:00"),
            ("2002-10-10-12:00", XmlTypeCode::Date, "2002-10-11+12:00"),
            ("2002-10-10+12:00", XmlTypeCode::Date, "2002-10-10+12:00"),
            ("2002-10-10+00:00", XmlTypeCode::Date, "2002-10-10Z"),
            ("PT60M", XmlTypeCode::Duration, "PT60M"),
            ("P0Y1M", XmlTypeCode::Duration, "P1M"),
            ("PT0.000S", XmlTypeCode::Duration, "PT0S"),
            ("-PT1.50S", XmlTypeCode::Duration, "-PT1.5S"),
            (
                "PT0.123456789012345678901234567891S",
                XmlTypeCode::Duration,
                "PT0.123456789012345678901234567891S",
            ),
            ("2000-01+00:00", XmlTypeCode::GYearMonth, "2000-01Z"),
        ] {
            assert_eq!(form(literal, code), want, "{literal} as {code:?}");
        }
        assert_eq!(form(" p:x ", XmlTypeCode::QName), "p:x");
    }

    /// The expanded-name index of a QName enumeration is built once, shared
    /// while the arenas are unchanged, and rebuilt after they are mutated
    /// (`ArenasGuard::entries_mut` clears it with the effective facets).
    #[test]
    fn test_qname_enumeration_index_is_cached_and_invalidated() {
        let mut schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:t="urn:t">
                 <xs:simpleType name="code"><xs:restriction base="xs:QName">
                   <xs:enumeration value="t:a"/>
                 </xs:restriction></xs:simpleType>
                 <xs:element name="e" type="code"/>
               </xs:schema>"#,
        );
        let TypeKey::Simple(sk) = element_type(&schema_set, "e") else {
            panic!("a simple type");
        };
        let names = &schema_set.name_table;
        let (t, a, b) = (names.add("urn:t"), names.add("a"), names.add("b"));
        let first = schema_set.enumeration_index(sk).expect("an index");
        assert!(first.admits("t:a", false, Some(t), a));
        assert!(!first.admits("t:b", false, Some(t), b));
        let second = schema_set.enumeration_index(sk).expect("an index");
        assert!(std::sync::Arc::ptr_eq(&first, &second));

        // Replace the literal through the mutation gate.
        {
            let arenas = schema_set.arenas.entries_mut();
            let enumeration = arenas.simple_types[sk].facets.enumeration.as_mut().unwrap();
            enumeration.values = ["t:b".to_string()].into_iter().collect();
            for (literal, _) in &mut enumeration.qname_contexts {
                *literal = "t:b".to_string();
            }
        }
        let rebuilt = schema_set.enumeration_index(sk).expect("an index");
        assert!(!std::sync::Arc::ptr_eq(&first, &rebuilt));
        assert!(rebuilt.admits("t:b", false, Some(t), b));
        assert!(!rebuilt.admits("t:a", false, Some(t), a));
    }

    #[test]
    fn test_builtin_string_accepts_anything() {
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="e" type="xs:string"/>
            </xs:schema>"#,
        );
        let tk = element_type(&schema, "e");
        let result = validate_simple_type("hello world", tk, &schema).unwrap();
        assert!(result.member_type.is_none());
        assert_eq!(result.typed_value.type_code, XmlTypeCode::String);
    }

    #[test]
    fn test_builtin_integer_valid() {
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="e" type="xs:integer"/>
            </xs:schema>"#,
        );
        let tk = element_type(&schema, "e");
        let result = validate_simple_type("42", tk, &schema).unwrap();
        assert_eq!(result.typed_value.type_code, XmlTypeCode::Integer);
    }

    #[test]
    fn test_builtin_integer_invalid() {
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="e" type="xs:integer"/>
            </xs:schema>"#,
        );
        let tk = element_type(&schema, "e");
        let err = validate_simple_type("abc", tk, &schema).unwrap_err();
        assert_eq!(err.constraint, "cvc-datatype-valid");
    }

    #[test]
    fn test_builtin_boolean_valid() {
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="e" type="xs:boolean"/>
            </xs:schema>"#,
        );
        let tk = element_type(&schema, "e");
        for v in &["true", "false", "1", "0"] {
            assert!(
                validate_simple_type(v, tk, &schema).is_ok(),
                "failed for '{}'",
                v
            );
        }
    }

    #[test]
    fn test_builtin_boolean_invalid() {
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="e" type="xs:boolean"/>
            </xs:schema>"#,
        );
        let tk = element_type(&schema, "e");
        assert!(validate_simple_type("yes", tk, &schema).is_err());
    }

    #[test]
    fn test_user_defined_restriction_min_max() {
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:simpleType name="score">
                    <xs:restriction base="xs:integer">
                        <xs:minInclusive value="0"/>
                        <xs:maxInclusive value="100"/>
                    </xs:restriction>
                </xs:simpleType>
                <xs:element name="e" type="score"/>
            </xs:schema>"#,
        );
        let tk = element_type(&schema, "e");

        assert!(validate_simple_type("50", tk, &schema).is_ok());
        assert!(validate_simple_type("0", tk, &schema).is_ok());
        assert!(validate_simple_type("100", tk, &schema).is_ok());

        let err = validate_simple_type("101", tk, &schema).unwrap_err();
        assert_eq!(err.constraint, "cvc-maxInclusive-valid");

        let err = validate_simple_type("-1", tk, &schema).unwrap_err();
        assert_eq!(err.constraint, "cvc-minInclusive-valid");
    }

    // -----------------------------------------------------------------------
    // Phase 3: allocation-free numeric fast path (`try_validate_lexical_only`).
    // -----------------------------------------------------------------------

    #[test]
    fn test_numeric_fast_path_builtin_int() {
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="e" type="xs:int"/>
            </xs:schema>"#,
        );
        let tk = element_type(&schema, "e");

        // In-range value handled by the fast path, no value built.
        assert!(matches!(
            try_validate_lexical_only("42", tk, &schema),
            Some(Ok(()))
        ));
        assert!(matches!(
            try_validate_lexical_only("+42", tk, &schema),
            Some(Ok(()))
        ));
        assert!(matches!(
            try_validate_lexical_only("2147483647", tk, &schema),
            Some(Ok(()))
        ));

        // Out of the native xs:int range but within i128: the fast path defers
        // (returns None) so the full path emits its canonical diagnostic.
        assert!(try_validate_lexical_only("9999999999", tk, &schema).is_none());
        // And that canonical diagnostic is the native-parse one, unchanged.
        let err = validate_simple_type("9999999999", tk, &schema).unwrap_err();
        assert_eq!(err.constraint, "cvc-datatype-valid");
    }

    #[test]
    fn test_numeric_fast_path_restriction_bounds_defer_to_full_path() {
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:simpleType name="score">
                    <xs:restriction base="xs:integer">
                        <xs:minInclusive value="0"/>
                        <xs:maxInclusive value="100"/>
                    </xs:restriction>
                </xs:simpleType>
                <xs:element name="e" type="score"/>
            </xs:schema>"#,
        );
        let tk = element_type(&schema, "e");

        for ok in ["0", "50", "100"] {
            assert!(
                matches!(try_validate_lexical_only(ok, tk, &schema), Some(Ok(()))),
                "fast path should accept {ok}"
            );
        }

        // Out-of-bounds values defer; the full path keeps the same cvc codes
        // whether PSVI retention is on or off.
        for (val, code) in [
            ("101", "cvc-maxInclusive-valid"),
            ("-1", "cvc-minInclusive-valid"),
        ] {
            assert!(
                try_validate_lexical_only(val, tk, &schema).is_none(),
                "fast path should defer {val}"
            );
            assert_eq!(
                validate_simple_type(val, tk, &schema)
                    .unwrap_err()
                    .constraint,
                code
            );
        }
    }

    #[test]
    fn test_numeric_fast_path_total_digits() {
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:simpleType name="t">
                    <xs:restriction base="xs:integer">
                        <xs:totalDigits value="3"/>
                    </xs:restriction>
                </xs:simpleType>
                <xs:element name="e" type="t"/>
            </xs:schema>"#,
        );
        let tk = element_type(&schema, "e");
        assert!(matches!(
            try_validate_lexical_only("123", tk, &schema),
            Some(Ok(()))
        ));
        // Violation defers to the full path, which reports it.
        assert!(try_validate_lexical_only("1234", tk, &schema).is_none());
        let err = validate_simple_type("1234", tk, &schema).unwrap_err();
        assert_eq!(err.constraint, "cvc-totalDigits-valid");
    }

    #[test]
    fn test_numeric_fast_path_decimal() {
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:simpleType name="t">
                    <xs:restriction base="xs:decimal">
                        <xs:maxInclusive value="9.99"/>
                    </xs:restriction>
                </xs:simpleType>
                <xs:element name="e" type="t"/>
            </xs:schema>"#,
        );
        let tk = element_type(&schema, "e");
        assert!(matches!(
            try_validate_lexical_only("3.14", tk, &schema),
            Some(Ok(()))
        ));
        // Out-of-bounds decimal defers; full path reports the canonical code.
        assert!(try_validate_lexical_only("10.5", tk, &schema).is_none());
        assert_eq!(
            validate_simple_type("10.5", tk, &schema)
                .unwrap_err()
                .constraint,
            "cvc-maxInclusive-valid"
        );
    }

    #[test]
    fn test_numeric_fast_path_falls_back_when_out_of_i128() {
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="e" type="xs:integer"/>
            </xs:schema>"#,
        );
        let tk = element_type(&schema, "e");

        // 40-digit integer overflows i128 → defer to the BigInt full path.
        let big = "1234567890123456789012345678901234567890";
        assert!(try_validate_lexical_only(big, tk, &schema).is_none());
        // The full path validates it (a valid, very large xs:integer).
        assert!(validate_simple_type(big, tk, &schema).is_ok());

        // Invalid lexical also defers, so the full path reports the exact error.
        assert!(try_validate_lexical_only("abc", tk, &schema).is_none());
    }

    /// Core Phase 3 correctness gate: the numeric fast path must never
    /// synthesize an error and must never accept a value the full path rejects.
    /// It either accepts a genuinely-valid value (`Some(Ok)`) or defers (`None`);
    /// every invalid value defers so the full path owns the diagnostic.
    #[test]
    fn test_numeric_fast_path_verdict_parity() {
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="i" type="xs:int"/>
                <xs:element name="l" type="xs:long"/>
                <xs:element name="b" type="xs:byte"/>
                <xs:element name="ub" type="xs:unsignedByte"/>
                <xs:element name="ul" type="xs:unsignedLong"/>
                <xs:element name="nni" type="xs:nonNegativeInteger"/>
                <xs:element name="pi" type="xs:positiveInteger"/>
                <xs:element name="dec" type="xs:decimal"/>
            </xs:schema>"#,
        );

        let cases: &[(&str, &str)] = &[
            ("i", "0"),
            ("i", "-2147483648"),
            ("i", "2147483647"),
            ("i", "2147483648"),  // over xs:int max
            ("i", "-2147483649"), // under xs:int min
            ("l", "9223372036854775807"),
            ("l", "9223372036854775808"), // over xs:long max
            ("b", "127"),
            ("b", "128"), // over xs:byte max
            ("b", "-128"),
            ("ub", "0"),
            ("ub", "255"),
            ("ub", "256"), // over xs:unsignedByte max
            ("ub", "-1"),  // negative for unsigned
            ("ul", "18446744073709551615"),
            ("ul", "18446744073709551616"), // over u64 max, fits i128
            ("nni", "0"),
            ("nni", "-1"), // negative for nonNegativeInteger
            ("pi", "1"),
            ("pi", "0"), // zero for positiveInteger
            ("dec", "3.14"),
            ("dec", "-0.001"),
        ];

        let mut fast_accepts = 0;
        for (elem, value) in cases {
            let tk = element_type(&schema, elem);
            let full_ok = validate_simple_type(value, tk, &schema).is_ok();
            // A `None` means the fast path deferred (invalid, or integer past
            // i128) — the full path owns the verdict and diagnostic. A `Some`
            // must be `Ok` (numerics never synthesize an error) and a genuine
            // accept (no false accept).
            if let Some(fast) = try_validate_lexical_only(value, tk, &schema) {
                assert!(fast.is_ok(), "numeric fast path must not emit errors");
                assert!(
                    full_ok,
                    "false accept for <{elem}>{value}</{elem}>: full path rejects it"
                );
                fast_accepts += 1;
            }
        }
        // Guard against a vacuous test: the valid in-range cases must be taken
        // by the fast path, not all deferred.
        assert!(
            fast_accepts >= 8,
            "expected the fast path to accept the valid in-range cases, got {fast_accepts}"
        );
    }

    #[test]
    fn test_enumeration_facet() {
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:simpleType name="color">
                    <xs:restriction base="xs:string">
                        <xs:enumeration value="red"/>
                        <xs:enumeration value="green"/>
                        <xs:enumeration value="blue"/>
                    </xs:restriction>
                </xs:simpleType>
                <xs:element name="e" type="color"/>
            </xs:schema>"#,
        );
        let tk = element_type(&schema, "e");

        assert!(validate_simple_type("red", tk, &schema).is_ok());
        assert!(validate_simple_type("green", tk, &schema).is_ok());

        let err = validate_simple_type("yellow", tk, &schema).unwrap_err();
        assert_eq!(err.constraint, "cvc-enumeration-valid");
    }

    #[test]
    fn test_pattern_facet() {
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:simpleType name="zipCode">
                    <xs:restriction base="xs:string">
                        <xs:pattern value="[0-9]{5}"/>
                    </xs:restriction>
                </xs:simpleType>
                <xs:element name="e" type="zipCode"/>
            </xs:schema>"#,
        );
        let tk = element_type(&schema, "e");

        assert!(validate_simple_type("12345", tk, &schema).is_ok());

        let err = validate_simple_type("1234", tk, &schema).unwrap_err();
        assert_eq!(err.constraint, "cvc-pattern-valid");
    }

    #[test]
    fn test_empty_string_against_integer() {
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="e" type="xs:integer"/>
            </xs:schema>"#,
        );
        let tk = element_type(&schema, "e");
        assert!(validate_simple_type("", tk, &schema).is_err());
    }

    #[test]
    fn test_empty_string_against_string() {
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="e" type="xs:string"/>
            </xs:schema>"#,
        );
        let tk = element_type(&schema, "e");
        assert!(validate_simple_type("", tk, &schema).is_ok());
    }

    #[test]
    fn test_list_type() {
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:simpleType name="intList">
                    <xs:list itemType="xs:integer"/>
                </xs:simpleType>
                <xs:element name="e" type="intList"/>
            </xs:schema>"#,
        );
        let tk = element_type(&schema, "e");

        let result = validate_simple_type("1 2 3", tk, &schema).unwrap();
        assert!(result.typed_value.is_list());

        // Non-integer item should fail
        assert!(validate_simple_type("1 abc 3", tk, &schema).is_err());
    }

    #[test]
    fn test_union_type() {
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:simpleType name="intOrString">
                    <xs:union memberTypes="xs:integer xs:string"/>
                </xs:simpleType>
                <xs:element name="e" type="intOrString"/>
            </xs:schema>"#,
        );
        let tk = element_type(&schema, "e");

        // Integer matches first
        let result = validate_simple_type("42", tk, &schema).unwrap();
        assert!(result.member_type.is_some());
        assert_eq!(result.typed_value.type_code, XmlTypeCode::Integer);

        // Non-integer matches xs:string
        let result = validate_simple_type("hello", tk, &schema).unwrap();
        assert!(result.member_type.is_some());
        assert_eq!(result.typed_value.type_code, XmlTypeCode::String);
    }
}

// ---------------------------------------------------------------------------
// XSD 1.1 tests: schema_type propagation and to_xpath_value conversion
// ---------------------------------------------------------------------------

#[cfg(all(test, feature = "xsd11"))]
mod xsd11_tests {
    use super::*;
    use crate::navigator::RoXmlNavigator;
    use crate::pipeline::load_and_process_schema;
    use crate::types::sequence::resolve_list_item_schema_type;
    use crate::xpath::XPathValue;

    fn load_schema(xsd: &str) -> SchemaSet {
        let mut schema_set = SchemaSet::xsd11();
        load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None)
            .expect("failed to load schema");
        schema_set
    }

    /// The constraint the schema is refused with (Structures 1.1 §3.13.6.1:
    /// an assertion whose test is not XPath Valid makes the schema invalid).
    fn refused_with(xsd: &str) -> &'static str {
        let mut schema_set = SchemaSet::xsd11();
        match load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None) {
            Err(crate::error::SchemaError::StructuralError { constraint, .. }) => constraint,
            other => panic!("expected a structural error, got {other:?}"),
        }
    }

    fn element_type(schema_set: &SchemaSet, local_name: &str) -> TypeKey {
        let name_id = schema_set.name_table.add(local_name);
        let elem_key = schema_set
            .lookup_element(None, name_id)
            .expect("element not found");
        schema_set.arenas.elements[elem_key]
            .resolved_type
            .expect("element has no resolved type")
    }

    #[test]
    fn test_atomic_schema_type_set() {
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:simpleType name="score">
                    <xs:restriction base="xs:integer">
                        <xs:minInclusive value="0"/>
                        <xs:maxInclusive value="100"/>
                    </xs:restriction>
                </xs:simpleType>
                <xs:element name="e" type="score"/>
            </xs:schema>"#,
        );
        let tk = element_type(&schema, "e");
        let result = validate_simple_type("50", tk, &schema).unwrap();
        assert!(
            result.typed_value.schema_type.is_some(),
            "atomic value should have schema_type set"
        );
    }

    #[test]
    fn test_list_schema_type_set() {
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:simpleType name="intList">
                    <xs:list itemType="xs:integer"/>
                </xs:simpleType>
                <xs:element name="e" type="intList"/>
            </xs:schema>"#,
        );
        let tk = element_type(&schema, "e");
        let result = validate_simple_type("1 2 3", tk, &schema).unwrap();
        assert!(
            result.typed_value.schema_type.is_some(),
            "list value should have schema_type set"
        );
    }

    #[test]
    fn test_union_schema_type_set() {
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:simpleType name="intOrString">
                    <xs:union memberTypes="xs:integer xs:string"/>
                </xs:simpleType>
                <xs:element name="e" type="intOrString"/>
            </xs:schema>"#,
        );
        let tk = element_type(&schema, "e");
        let result = validate_simple_type("42", tk, &schema).unwrap();

        // Outer union value has schema_type
        assert!(
            result.typed_value.schema_type.is_some(),
            "union value should have schema_type set"
        );

        // Inner value also has schema_type (from matched member)
        if let XmlValueKind::Union(ref inner) = result.typed_value.value {
            assert!(
                inner.schema_type.is_some(),
                "inner union value should have schema_type set from matched member"
            );
        } else {
            panic!("expected Union variant");
        }
    }

    #[test]
    fn test_list_to_xpath_value() {
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:simpleType name="intList">
                    <xs:list itemType="xs:integer"/>
                </xs:simpleType>
                <xs:element name="e" type="intList"/>
            </xs:schema>"#,
        );
        let tk = element_type(&schema, "e");
        let result = validate_simple_type("1 2 3", tk, &schema).unwrap();

        // The list's item type
        let list_sk = tk.as_simple().expect("should be simple type");
        let item_sk = resolve_list_item_schema_type(list_sk, &schema);

        let xpath_val: XPathValue<RoXmlNavigator<'static>> = result.typed_value.to_xpath_value();

        // Should produce a sequence of 3 items
        let items = xpath_val.into_vec();
        assert_eq!(items.len(), 3, "list should produce 3 XPath items");

        // Each item carries the item type it was validated with
        for item in &items {
            let val = item.as_atomic().expect("should be atomic");
            assert_eq!(
                val.schema_type, item_sk,
                "each list item should have the item type as its schema_type"
            );
        }
    }

    /// The members of a list whose item type is a union are the values of the
    /// member types that validated them (XPath 2.0 §2.5.2); the list records
    /// the declared item type, whose code is that of a union's base type.
    #[test]
    fn test_list_of_union_members_have_their_member_types() {
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:simpleType name="small">
                    <xs:restriction base="xs:integer"><xs:maxInclusive value="9"/></xs:restriction>
                </xs:simpleType>
                <xs:simpleType name="smallOrString">
                    <xs:union memberTypes="small xs:string"/>
                </xs:simpleType>
                <xs:simpleType name="nested">
                    <xs:union memberTypes="xs:date smallOrString"/>
                </xs:simpleType>
                <xs:simpleType name="l"><xs:list itemType="nested"/></xs:simpleType>
                <xs:element name="e" type="l"/>
            </xs:schema>"#,
        );
        let tk = element_type(&schema, "e");
        let result = validate_simple_type("5 x 2020-01-01 10", tk, &schema).unwrap();
        let XmlValueKind::List { item_type, items } = &result.typed_value.value else {
            panic!("expected a list value, got {:?}", result.typed_value);
        };
        assert_eq!(*item_type, XmlTypeCode::AnySimpleType);
        let named = |name: &str| {
            let id = schema.name_table.get(name).expect("interned");
            schema.lookup_type(None, id).and_then(|tk| tk.as_simple())
        };
        let members: Vec<(XmlTypeCode, String, bool)> = items
            .iter()
            .map(|m| {
                assert!(matches!(m.value, XmlValueKind::Atomic(_)), "{m:?}");
                (m.type_code, m.to_string_value(), m.schema_type.is_some())
            })
            .collect();
        assert_eq!(
            members,
            [
                (XmlTypeCode::Integer, "5".to_string(), true),
                (XmlTypeCode::String, "x".to_string(), true),
                (XmlTypeCode::Date, "2020-01-01".to_string(), true),
                (XmlTypeCode::String, "10".to_string(), true),
            ]
        );
        assert_eq!(items[0].schema_type, named("small"));
        assert_eq!(result.typed_value.to_string_value(), "5 x 2020-01-01 10");
    }

    #[test]
    fn test_union_to_xpath_value() {
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:simpleType name="intOrString">
                    <xs:union memberTypes="xs:integer xs:string"/>
                </xs:simpleType>
                <xs:element name="e" type="intOrString"/>
            </xs:schema>"#,
        );
        let tk = element_type(&schema, "e");
        let result = validate_simple_type("42", tk, &schema).unwrap();

        let xpath_val: XPathValue<RoXmlNavigator<'static>> = result.typed_value.to_xpath_value();

        // Union unwraps to the inner atomic value
        let items = xpath_val.into_vec();
        assert_eq!(items.len(), 1, "union should unwrap to single item");
        assert!(items[0].is_atomic());
    }

    #[test]
    fn test_atomic_to_xpath_value() {
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="e" type="xs:integer"/>
            </xs:schema>"#,
        );
        let tk = element_type(&schema, "e");
        let result = validate_simple_type("42", tk, &schema).unwrap();

        let xpath_val: XPathValue<RoXmlNavigator<'static>> = result.typed_value.to_xpath_value();

        let items = xpath_val.into_vec();
        assert_eq!(items.len(), 1, "atomic should produce single item");
        assert!(items[0].is_atomic());
        assert_eq!(
            items[0].as_atomic().unwrap().type_code,
            XmlTypeCode::Integer
        );
    }

    // -----------------------------------------------------------------------
    // Assertion facet tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_assertion_even_integer() {
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:simpleType name="evenInt">
                    <xs:restriction base="xs:integer">
                        <xs:assertion test="$value mod 2 = 0"/>
                    </xs:restriction>
                </xs:simpleType>
                <xs:element name="e" type="evenInt"/>
            </xs:schema>"#,
        );
        let tk = element_type(&schema, "e");
        assert!(validate_simple_type("4", tk, &schema).is_ok());
        assert!(validate_simple_type("0", tk, &schema).is_ok());
        assert!(validate_simple_type("-2", tk, &schema).is_ok());

        let err = validate_simple_type("3", tk, &schema).unwrap_err();
        assert_eq!(err.constraint, "cvc-assertion");

        let err = validate_simple_type("7", tk, &schema).unwrap_err();
        assert_eq!(err.constraint, "cvc-assertion");
    }

    #[test]
    fn test_assertion_positive_value() {
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:simpleType name="posInt">
                    <xs:restriction base="xs:integer">
                        <xs:assertion test="$value gt 0"/>
                    </xs:restriction>
                </xs:simpleType>
                <xs:element name="e" type="posInt"/>
            </xs:schema>"#,
        );
        let tk = element_type(&schema, "e");
        assert!(validate_simple_type("1", tk, &schema).is_ok());
        assert!(validate_simple_type("100", tk, &schema).is_ok());

        let err = validate_simple_type("0", tk, &schema).unwrap_err();
        assert_eq!(err.constraint, "cvc-assertion");

        let err = validate_simple_type("-5", tk, &schema).unwrap_err();
        assert_eq!(err.constraint, "cvc-assertion");
    }

    #[test]
    fn test_assertion_string_length() {
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:simpleType name="shortStr">
                    <xs:restriction base="xs:string">
                        <xs:assertion test="string-length($value) le 5"/>
                    </xs:restriction>
                </xs:simpleType>
                <xs:element name="e" type="shortStr"/>
            </xs:schema>"#,
        );
        let tk = element_type(&schema, "e");
        assert!(validate_simple_type("hello", tk, &schema).is_ok());
        assert!(validate_simple_type("", tk, &schema).is_ok());
        assert!(validate_simple_type("abcde", tk, &schema).is_ok());

        let err = validate_simple_type("toolong", tk, &schema).unwrap_err();
        assert_eq!(err.constraint, "cvc-assertion");
    }

    #[test]
    fn test_assertion_inherited() {
        // Assertion inherited from base type through derivation chain
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:simpleType name="positiveInt">
                    <xs:restriction base="xs:integer">
                        <xs:assertion test="$value gt 0"/>
                    </xs:restriction>
                </xs:simpleType>
                <xs:simpleType name="smallPositiveInt">
                    <xs:restriction base="positiveInt">
                        <xs:maxInclusive value="10"/>
                    </xs:restriction>
                </xs:simpleType>
                <xs:element name="e" type="smallPositiveInt"/>
            </xs:schema>"#,
        );
        let tk = element_type(&schema, "e");
        assert!(validate_simple_type("5", tk, &schema).is_ok());
        assert!(validate_simple_type("10", tk, &schema).is_ok());

        // Fails maxInclusive
        assert!(validate_simple_type("11", tk, &schema).is_err());
        // Fails inherited assertion ($value gt 0)
        let err = validate_simple_type("0", tk, &schema).unwrap_err();
        assert_eq!(err.constraint, "cvc-assertion");
    }

    #[test]
    fn test_assertion_compile_error() {
        // Invalid XPath in an assertion test: a static error (XPST0003), so
        // the test is not XPath Valid and the schema is invalid
        // (as-props-correct, Structures 1.1 §3.13.6.1).
        let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:simpleType name="badAssert">
                    <xs:restriction base="xs:integer">
                        <xs:assertion test="$value @@@ invalid"/>
                    </xs:restriction>
                </xs:simpleType>
                <xs:element name="e" type="badAssert"/>
            </xs:schema>"#;
        assert_eq!(refused_with(xsd), "as-props-correct");
    }

    #[test]
    fn test_assertion_unknown_foreign_function_fails_the_value() {
        // A function of another namespace that the engine lacks is not a
        // schema error (the function signatures are implementation-defined,
        // §3.13.6.2 clause 2.2.8): each value fails the assertion.
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:f="urn:f">
                <xs:simpleType name="foreign">
                    <xs:restriction base="xs:integer">
                        <xs:assertion test="f:check($value)"/>
                    </xs:restriction>
                </xs:simpleType>
                <xs:element name="e" type="foreign"/>
            </xs:schema>"#,
        );
        let tk = element_type(&schema, "e");
        let err = validate_simple_type("42", tk, &schema).unwrap_err();
        assert_eq!(err.constraint, "cvc-assertion");
        assert!(err.message.contains("compile"));
    }

    #[test]
    fn test_assertion_with_other_facets() {
        // Assertion combined with pattern and range facets
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:simpleType name="evenScore">
                    <xs:restriction base="xs:integer">
                        <xs:minInclusive value="0"/>
                        <xs:maxInclusive value="100"/>
                        <xs:assertion test="$value mod 2 = 0"/>
                    </xs:restriction>
                </xs:simpleType>
                <xs:element name="e" type="evenScore"/>
            </xs:schema>"#,
        );
        let tk = element_type(&schema, "e");
        assert!(validate_simple_type("50", tk, &schema).is_ok());
        assert!(validate_simple_type("0", tk, &schema).is_ok());
        assert!(validate_simple_type("100", tk, &schema).is_ok());

        // Fails assertion (odd number)
        let err = validate_simple_type("51", tk, &schema).unwrap_err();
        assert_eq!(err.constraint, "cvc-assertion");

        // Fails range (out of bounds)
        assert!(validate_simple_type("102", tk, &schema).is_err());
        assert!(validate_simple_type("-2", tk, &schema).is_err());
    }

    #[test]
    fn test_assertion_multiple() {
        // Multiple assertion facets on the same type
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:simpleType name="constrained">
                    <xs:restriction base="xs:integer">
                        <xs:assertion test="$value gt 0"/>
                        <xs:assertion test="$value mod 2 = 0"/>
                    </xs:restriction>
                </xs:simpleType>
                <xs:element name="e" type="constrained"/>
            </xs:schema>"#,
        );
        let tk = element_type(&schema, "e");
        // Must satisfy both: positive AND even
        assert!(validate_simple_type("2", tk, &schema).is_ok());
        assert!(validate_simple_type("4", tk, &schema).is_ok());

        // Positive but odd -> fails second assertion
        let err = validate_simple_type("3", tk, &schema).unwrap_err();
        assert_eq!(err.constraint, "cvc-assertion");

        // Even but not positive -> fails first assertion
        let err = validate_simple_type("0", tk, &schema).unwrap_err();
        assert_eq!(err.constraint, "cvc-assertion");
    }

    #[test]
    fn test_assertion_boolean_value() {
        // Assertion on xs:boolean type
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:simpleType name="mustBeTrue">
                    <xs:restriction base="xs:boolean">
                        <xs:assertion test="$value"/>
                    </xs:restriction>
                </xs:simpleType>
                <xs:element name="e" type="mustBeTrue"/>
            </xs:schema>"#,
        );
        let tk = element_type(&schema, "e");
        assert!(validate_simple_type("true", tk, &schema).is_ok());
        assert!(validate_simple_type("1", tk, &schema).is_ok());

        let err = validate_simple_type("false", tk, &schema).unwrap_err();
        assert_eq!(err.constraint, "cvc-assertion");
    }

    #[test]
    fn test_assertion_union_with_list_member_item_typing() {
        // Union whose member is a list of xs:integer.
        // The assertion uses `instance of` on each list item, which requires
        // item_schema_type to be propagated through the union validation path.
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:simpleType name="intList">
                    <xs:list itemType="xs:integer"/>
                </xs:simpleType>
                <xs:simpleType name="unionWithList">
                    <xs:union memberTypes="intList">
                        <xs:simpleType>
                            <xs:restriction base="xs:string"/>
                        </xs:simpleType>
                    </xs:union>
                </xs:simpleType>
                <xs:simpleType name="checkedUnion">
                    <xs:restriction base="unionWithList">
                        <xs:assertion test="every $i in $value satisfies $i instance of xs:integer"/>
                    </xs:restriction>
                </xs:simpleType>
                <xs:element name="e" type="checkedUnion"/>
            </xs:schema>"#,
        );
        let tk = element_type(&schema, "e");
        // Space-separated integers should match the intList member and pass the assertion
        assert!(validate_simple_type("1 2 3", tk, &schema).is_ok());
    }

    // -----------------------------------------------------------------------
    // xpathDefaultNamespace cascade for assertion facets
    // -----------------------------------------------------------------------

    #[test]
    fn test_assertion_xpath_default_ns_schema_level_fallback() {
        // Schema-level xpathDefaultNamespace set to the XS namespace.
        // The assertion uses an unprefixed type name `integer` which should
        // resolve to xs:integer via the default element namespace cascade.
        let schema = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                        xpathDefaultNamespace="http://www.w3.org/2001/XMLSchema">
                <xs:simpleType name="checkedInt">
                    <xs:restriction base="xs:integer">
                        <xs:assertion test="$value instance of integer"/>
                    </xs:restriction>
                </xs:simpleType>
                <xs:element name="e" type="checkedInt"/>
            </xs:schema>"#,
        );
        let tk = element_type(&schema, "e");
        // Unprefixed `integer` resolves to xs:integer → assertion passes
        assert!(validate_simple_type("42", tk, &schema).is_ok());
    }

    #[test]
    fn test_assertion_xpath_default_ns_assertion_level_overrides_schema() {
        // Schema-level xpathDefaultNamespace is the XS namespace, but the
        // assertion element overrides it with ##local.  Now unprefixed
        // `integer` resolves to no-namespace, which names no built-in type:
        // a static error (XPST0051), so the schema is invalid.
        let xsd = r###"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                        xpathDefaultNamespace="http://www.w3.org/2001/XMLSchema">
                <xs:simpleType name="checkedInt">
                    <xs:restriction base="xs:integer">
                        <xs:assertion test="$value instance of integer"
                                      xpathDefaultNamespace="##local"/>
                    </xs:restriction>
                </xs:simpleType>
                <xs:element name="e" type="checkedInt"/>
            </xs:schema>"###;
        // ##local overrides → `integer` resolves to no-namespace → refused
        assert_eq!(refused_with(xsd), "as-props-correct");
    }

    #[test]
    fn test_assertion_xpath_default_ns_target_namespace_token() {
        // xpathDefaultNamespace="##targetNamespace" with a non-XS target namespace.
        // Unprefixed `integer` resolves to http://example.com/ns (the target
        // namespace), not to the XS namespace, so `instance of integer` names
        // no built-in type — proving the token is resolved: the schema is
        // invalid (XPST0051 in the built-ins-only static context).
        let xsd = r###"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                        targetNamespace="http://example.com/ns"
                        xmlns:tns="http://example.com/ns"
                        xpathDefaultNamespace="##targetNamespace">
                <xs:simpleType name="checkedInt">
                    <xs:restriction base="xs:integer">
                        <xs:assertion test="$value instance of integer"/>
                    </xs:restriction>
                </xs:simpleType>
                <xs:element name="e" type="tns:checkedInt"/>
            </xs:schema>"###;
        // ##targetNamespace → http://example.com/ns → `integer` is NOT xs:integer
        assert_eq!(refused_with(xsd), "as-props-correct");
    }
}

fn validate_simple_type_inner(
    value: &str,
    type_key: TypeKey,
    schema_set: &SchemaSet,
    resolver: Option<PrefixResolver<'_>>,
) -> Result<SimpleTypeResult, ValidationError> {
    validate_simple_type_impl(value, type_key, schema_set, resolver)
        .map_err(|err| err.with_type_context(type_key, schema_set))
}
