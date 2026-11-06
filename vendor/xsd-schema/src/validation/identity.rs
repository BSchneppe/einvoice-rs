//! Per-constraint state management and key table types for identity-constraint
//! streaming validation.
//!
//! This module bridges parsed constraint definitions (`IdentityConstraintData`)
//! to runtime validation by providing:
//!
//! - `CompiledIdentityConstraint` — pre-compiled selector/field Asttrees
//! - [`KeyFieldValue`] / [`KeySequence`] — extracted field values with XSD equality
//! - [`KeyTable`] — duplicate detection for key/unique, deferred storage for keyref
//! - `ConstraintStruct` — per-activation state driving `ActiveAxis` matchers

use crate::ids::NameId;
use crate::namespace::table::NameTable;
use crate::parser::frames::{IdentityKind, QNameRef};
use crate::parser::location::SourceLocation;
use crate::schema::model::XsdVersion;
use crate::types::value::XmlValue;
use crate::types::{PrimitiveTypeCode, XmlTypeCode};

use super::active_axis::ActiveAxis;
use super::asttree::{Asttree, IdentityXPathError};
use super::errors::{error, error_with_path, ValidationError};

use crate::arenas::IdentityConstraintData;
use crate::ids::IdentityConstraintKey;

/// Extract the single atomic value from any value shape.
///
/// Used for IC equality: XSD §3.11.4 says "single atomic values are not
/// distinguished from lists with single items" — an atomic value and a
/// singleton list containing the same atomic value are treated as equal.
fn extract_single_atomic(value: &XmlValue) -> Option<&crate::types::value::XmlAtomicValue> {
    match &value.value {
        crate::types::value::XmlValueKind::Atomic(a) => Some(a),
        crate::types::value::XmlValueKind::List { items, .. } if items.len() == 1 => {
            extract_single_atomic(&items[0])
        }
        crate::types::value::XmlValueKind::Union(inner) => extract_single_atomic(inner),
        _ => None,
    }
}

/// The value a field holds with its union wrappers taken off.
fn unwrap_union(value: &XmlValue) -> &XmlValue {
    match &value.value {
        crate::types::value::XmlValueKind::Union(inner) => unwrap_union(inner),
        _ => value,
    }
}

/// The members of a list value, or `None` for any other value.
fn list_members(value: &XmlValue) -> Option<&[XmlValue]> {
    match &value.value {
        crate::types::value::XmlValueKind::List { items, .. } => Some(items),
        _ => None,
    }
}

/// IC equality of two atomic values (Datatypes §2.2.1, §2.2.2): values of
/// different primitive datatypes are distinct; within one, value-space
/// equality or identity. `None` when either has no primitive datatype, so the
/// caller decides on the string forms.
fn atomic_ic_eq(a: &XmlValue, b: &XmlValue) -> Option<bool> {
    match (a.primitive_type(), b.primitive_type()) {
        (Some(pa), Some(pb)) if pa == pb => {
            // XSD value-space equality: for the decimal hierarchy
            // (xs:decimal, xs:integer, and all derived integer types)
            // different storage variants (Decimal vs Integer) must be
            // compared numerically, not structurally.
            Some(if pa == PrimitiveTypeCode::Decimal {
                decimal_family_ic_eq(a, b)
            } else {
                xml_value_ic_eq(a, b)
            })
        }
        (Some(_), Some(_)) => Some(false), // different primitive types → never equal
        _ => None,
    }
}

/// Whether two typed values are "equal or identical" in the sense of
/// Datatypes §2.2.1 / §2.2.2 — the comparison identity constraints
/// (Structures §3.11.4), value constraints (`fixed`, §3.3.4.3, §3.5.4) and
/// the `enumeration` facet of a list type (Datatypes §4.3.5.4, §2.4.1.2:
/// "enumerated values are compared to the entire ·list·") rely on. A list compares member for member, each
/// member in the value space of the type that validated it; values of
/// different primitive datatypes are distinct; a value without a primitive
/// datatype (`xs:untypedAtomic`) compares by its string. `schema_type` never
/// matters: restrictions do not create new values.
pub(crate) fn values_equal_or_identical(a: &XmlValue, b: &XmlValue) -> bool {
    let (a, b) = (unwrap_union(a), unwrap_union(b));
    if list_members(a).is_some() || list_members(b).is_some() {
        return list_ic_eq(a, b);
    }
    atomic_ic_eq(a, b).unwrap_or_else(|| a.to_string_value() == b.to_string_value())
}

/// [`values_equal_or_identical`] of two values read from the literals
/// `a_lexical` and `b_lexical`: a date/time or duration literal whose seconds
/// have more fraction digits than its typed value holds (truncated) is
/// compared with the fraction as written
/// ([`literal_cmp_in`](crate::types::value_order::literal_cmp_in)); a list
/// member is paired with its whitespace-separated token.
pub(crate) fn literal_values_equal_or_identical(
    a: &XmlValue,
    a_lexical: &str,
    b: &XmlValue,
    b_lexical: &str,
) -> bool {
    use crate::types::validators::seconds_beyond_capacity;
    use crate::types::value::XmlValueKind as K;
    if !seconds_beyond_capacity(a_lexical) && !seconds_beyond_capacity(b_lexical) {
        return values_equal_or_identical(a, b);
    }
    let (a, b) = (unwrap_union(a), unwrap_union(b));
    match (&a.value, &b.value) {
        (K::List { items: xs, .. }, K::List { items: ys, .. }) => {
            xs.len() == ys.len()
                && xs
                    .iter()
                    .zip(a_lexical.split_ascii_whitespace())
                    .zip(ys.iter().zip(b_lexical.split_ascii_whitespace()))
                    .all(|((x, xl), (y, yl))| literal_values_equal_or_identical(x, xl, y, yl))
        }
        // An atomic value equals the list of that one value (§3.11.4).
        (K::List { items, .. }, K::Atomic(_)) if items.len() == 1 => {
            literal_values_equal_or_identical(&items[0], a_lexical.trim(), b, b_lexical)
        }
        (K::Atomic(_), K::List { items, .. }) if items.len() == 1 => {
            literal_values_equal_or_identical(a, a_lexical, &items[0], b_lexical.trim())
        }
        (K::Atomic(x), K::Atomic(y)) if seconds_type(x) && seconds_type(y) => {
            crate::types::value_order::literal_cmp_in(false, x, a_lexical, y, b_lexical)
                == Some(std::cmp::Ordering::Equal)
        }
        _ => values_equal_or_identical(a, b),
    }
}

/// [`values_identical`] of two values read from the literals `a_lexical` and
/// `b_lexical`, with the seconds as written as in
/// [`literal_values_equal_or_identical`]: two date/time values are
/// identical when their seven properties are, the seconds exactly; two
/// durations when their months and exact seconds are.
pub(crate) fn literal_values_identical(
    a: &XmlValue,
    a_lexical: &str,
    b: &XmlValue,
    b_lexical: &str,
) -> bool {
    use crate::types::validators::seconds_beyond_capacity;
    use crate::types::value::XmlValueKind as K;
    if !seconds_beyond_capacity(a_lexical) && !seconds_beyond_capacity(b_lexical) {
        return values_identical(a, b);
    }
    let (a, b) = (unwrap_union(a), unwrap_union(b));
    match (&a.value, &b.value) {
        (K::List { items: xs, .. }, K::List { items: ys, .. }) => {
            xs.len() == ys.len()
                && xs
                    .iter()
                    .zip(a_lexical.split_ascii_whitespace())
                    .zip(ys.iter().zip(b_lexical.split_ascii_whitespace()))
                    .all(|((x, xl), (y, yl))| literal_values_identical(x, xl, y, yl))
        }
        (K::Atomic(x), K::Atomic(y)) if seconds_type(x) && seconds_type(y) => {
            crate::types::value_order::literal_same_properties(x, a_lexical, y, b_lexical)
        }
        _ => values_identical(a, b),
    }
}

/// Whether `value` is, or (inside unions and lists) holds, a value of a
/// datatype with seconds.
fn holds_seconds(value: &XmlValue) -> bool {
    use crate::types::value::XmlValueKind as K;
    match &value.value {
        K::Atomic(atom) => seconds_type(atom),
        K::Union(inner) => holds_seconds(inner),
        K::List { items, .. } => items.iter().any(holds_seconds),
        K::UntypedAtomic(_) => false,
    }
}

/// Whether `atom` is a value of a datatype with seconds (`xs:dateTime`,
/// `xs:time`, the durations with a day-time part).
fn seconds_type(atom: &crate::types::value::XmlAtomicValue) -> bool {
    use crate::types::value::XmlAtomicValue as V;
    matches!(
        atom,
        V::DateTime(_) | V::Time(_) | V::Duration(_) | V::DayTimeDuration(_)
    )
}

/// Whether two typed values are **identical** (Datatypes 1.1 §2.2.1: "Two
/// things are identical if and only if they are actually the same thing") —
/// stricter than [`values_equal_or_identical`] in exactly the places 1.1
/// equality is not identity: `0` and `−0` (`xs:float`, `xs:double`) are equal
/// but not identical, NaN is identical to itself, and two date/time values on
/// one instant with different time zone offsets are "equal but not identical"
/// (§3.3.7). Decimals compare by value (precision is not part of it), durations
/// by months and seconds (the two properties of the value, §3.3.6.1 — so
/// `P400Y` and `P146097D`, equal by the four reference dateTimes, are not
/// identical), lists pairwise; values of different primitive datatypes are
/// distinct, and restrictions do not create new values.
pub(crate) fn values_identical(a: &XmlValue, b: &XmlValue) -> bool {
    use crate::types::value::{XmlAtomicValue as V, XmlValueKind as K};
    let (a, b) = (unwrap_union(a), unwrap_union(b));
    if list_members(a).is_some() || list_members(b).is_some() {
        let a_members = list_members(a).unwrap_or(std::slice::from_ref(a));
        let b_members = list_members(b).unwrap_or(std::slice::from_ref(b));
        return a_members.len() == b_members.len()
            && a_members
                .iter()
                .zip(b_members)
                .all(|(x, y)| values_identical(x, y));
    }
    match (a.primitive_type(), b.primitive_type()) {
        (Some(pa), Some(pb)) if pa == pb => {}
        (Some(_), Some(_)) => return false,
        _ => return a.to_string_value() == b.to_string_value(),
    }
    if a.primitive_type() == Some(PrimitiveTypeCode::Decimal) {
        return decimal_family_ic_eq(a, b);
    }
    match (&a.value, &b.value) {
        (K::Atomic(V::Double(x)), K::Atomic(V::Double(y))) => {
            (x.is_nan() && y.is_nan()) || (x == y && x.is_sign_negative() == y.is_sign_negative())
        }
        (K::Atomic(V::Float(x)), K::Atomic(V::Float(y))) => {
            (x.is_nan() && y.is_nan()) || (x == y && x.is_sign_negative() == y.is_sign_negative())
        }
        (K::Atomic(x), K::Atomic(y)) => match (
            crate::types::value_order::duration_parts(x),
            crate::types::value_order::duration_parts(y),
        ) {
            (Some(dx), Some(dy)) => dx == dy,
            // A date/time value is its seven properties (it keeps its time
            // zone offset); every other value is what its `PartialEq` compares
            // (a string its characters).
            _ => crate::types::value_order::same_properties(x, y).unwrap_or(x == y),
        },
        _ => a.value == b.value,
    }
}

/// IC equality when at least one of the two values is a list: §3.11.4 clause
/// 4 compares members "pairwise", and Datatypes §2.2.1 makes two lists the
/// same list when "they have the same length and … the items of A are
/// pairwise identical to the items of B" — so there is only one empty list,
/// whatever its item type. An atomic value counts as a list of that one value
/// (§3.11.4: "single atomic values are not distinguished from lists with
/// single items"). Each member carries the type that validated it, so a list
/// of a union compares every member in its own member type's value space.
fn list_ic_eq(a: &XmlValue, b: &XmlValue) -> bool {
    let a_members = list_members(a).unwrap_or(std::slice::from_ref(a));
    let b_members = list_members(b).unwrap_or(std::slice::from_ref(b));
    a_members.len() == b_members.len()
        && a_members.iter().zip(b_members).all(|(x, y)| {
            atomic_ic_eq(x, y).unwrap_or_else(|| x.to_string_value() == y.to_string_value())
        })
}

/// IC value-space equality: compare type_code + value, ignoring schema_type.
///
/// XSD identity constraints compare values in value-space, not by schema type
/// identity. Two identical primitive values from different anonymous type
/// restrictions must be considered equal.
///
/// For date/time/duration types the derived `PartialEq` compares fields
/// (hour=12+tz=Z differs from hour=13+tz=+01 even though both denote the
/// same instant), so we fall back to each type's `PartialOrd::partial_cmp`
/// which uses value-space (instant / total seconds) comparison.
///
/// Also handles singleton-list ↔ atomic equivalence per XSD §3.11.4.
fn xml_value_ic_eq(a: &XmlValue, b: &XmlValue) -> bool {
    if a.type_code == b.type_code && a.value == b.value {
        return true;
    }
    if a.type_code == b.type_code && (xml_value_datetime_eq(a, b) || xml_value_nan_eq(a, b)) {
        return true;
    }
    // Singleton-list ↔ atomic equality (XSD §3.11.4)
    match (extract_single_atomic(a), extract_single_atomic(b)) {
        (Some(va), Some(vb)) => {
            va == vb
                || xml_atomic_datetime_eq(va, vb)
                || xml_atomic_nan_eq(va, vb)
                || xml_atomic_decimal_eq(a.type_code, va, b.type_code, vb)
        }
        _ => false,
    }
}

/// IC equality for two values of the `xs:decimal` primitive: by value,
/// exactly. Same-variant pairs take the allocation-free comparison; anything
/// else — a `Decimal` against an `Integer`, a decimal carried in lexical form
/// because it is beyond `rust_decimal`'s capacity, an `xs:integer` beyond it,
/// a singleton list — goes through [`xml_value_ic_eq`], whose atomic path
/// compares decimal-family atoms by exact value.
fn decimal_family_ic_eq(a: &XmlValue, b: &XmlValue) -> bool {
    use crate::types::value::{XmlAtomicValue as V, XmlValueKind as K};
    match (&a.value, &b.value) {
        (K::Atomic(V::Decimal(x)), K::Atomic(V::Decimal(y))) => x == y,
        (K::Atomic(V::Integer(x)), K::Atomic(V::Integer(y))) => x == y,
        _ => xml_value_ic_eq(a, b),
    }
}

/// For identity-constraint purposes NaN is *identical* to NaN (XSD 1.1
/// Datatypes §2.2.1 "identity" / W3C bug 9196 — saxonData Complex
/// unique003): two NaN fields must collide, unlike IEEE `==`.
fn xml_atomic_nan_eq(
    a: &crate::types::value::XmlAtomicValue,
    b: &crate::types::value::XmlAtomicValue,
) -> bool {
    use crate::types::value::XmlAtomicValue as V;
    match (a, b) {
        (V::Double(x), V::Double(y)) => x.is_nan() && y.is_nan(),
        (V::Float(x), V::Float(y)) => x.is_nan() && y.is_nan(),
        _ => false,
    }
}

/// Exact value equality of two decimal-family atoms, each typed by its
/// value's type code (a list member by its own, the type that validated it),
/// so that a member carried in lexical form compares by value too.
fn xml_atomic_decimal_eq(
    a_type: XmlTypeCode,
    a: &crate::types::value::XmlAtomicValue,
    b_type: XmlTypeCode,
    b: &crate::types::value::XmlAtomicValue,
) -> bool {
    use crate::types::exact_decimal::ExactDecimal;
    match (
        ExactDecimal::from_atomic(a_type, a),
        ExactDecimal::from_atomic(b_type, b),
    ) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

fn xml_value_nan_eq(a: &XmlValue, b: &XmlValue) -> bool {
    use crate::types::value::XmlValueKind;
    match (&a.value, &b.value) {
        (XmlValueKind::Atomic(va), XmlValueKind::Atomic(vb)) => xml_atomic_nan_eq(va, vb),
        _ => false,
    }
}

/// Value-space equality for date/time/duration atomic kinds — the order the
/// facets use (`types::value_order`), so `fixed`, identity constraints,
/// `enumeration` and the bounds agree.
///
/// Date/time values are equal when they are one instant on the time line
/// (Datatypes §D.2.1: values "are ordered the same as their ·timeOnTimeline·
/// values"; every seven-property datatype, the `g*` ones included: §3.3.13.1
/// "---15−11:00 = ---16+13:00"), so one instant written in two time zones is
/// equal; a value with a time zone and one without are incomparable, never
/// equal.
///
/// Durations compare exactly, by the four reference dateTimes (Datatypes
/// §3.3.6.1: "the equality or order of two duration values is determined by
/// adding each duration in the pair to each of the following four dateTime
/// values") — `PT60S` equals `PT1M`, `P1M` equals no number of days, and
/// `P400Y` equals `P146097D`. `xs:yearMonthDuration` and `xs:dayTimeDuration`
/// are restrictions of `xs:duration`, so a value of one equals the same value
/// of another. Identity ([`values_identical`]) is the same months and seconds.
fn xml_atomic_datetime_eq(
    a: &crate::types::value::XmlAtomicValue,
    b: &crate::types::value::XmlAtomicValue,
) -> bool {
    crate::types::value_order::date_time_or_duration_eq(a, b).unwrap_or(false)
}

fn xml_value_datetime_eq(a: &XmlValue, b: &XmlValue) -> bool {
    use crate::types::value::XmlValueKind;
    match (&a.value, &b.value) {
        (XmlValueKind::Atomic(va), XmlValueKind::Atomic(vb)) => xml_atomic_datetime_eq(va, vb),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// CompiledIdentityConstraint
// ---------------------------------------------------------------------------

/// Pre-compiled identity constraint with cloneable `Asttree` instances.
///
/// Shared across multiple activations — each activation clones the Asttrees
/// into fresh `ActiveAxis` instances.
pub(crate) struct CompiledIdentityConstraint {
    /// Arena key for this constraint.
    pub key: IdentityConstraintKey,
    /// Constraint QName (interned).
    pub name: NameId,
    /// Constraint kind: Key, Unique, or Keyref.
    pub kind: IdentityKind,
    /// Pre-compiled selector expression.
    pub selector: Asttree,
    /// Pre-compiled field expressions.
    pub fields: Vec<Asttree>,
    /// Keyref target reference (only for `IdentityKind::Keyref`).
    pub refer: Option<QNameRef>,
    /// Resolved arena key of the referenced key/unique constraint (only for keyrefs).
    /// Set by the validator after compilation via `resolve_refer_key()`.
    pub refer_key: Option<IdentityConstraintKey>,
    /// Cached `fields.len()`.
    pub field_count: usize,
    /// Target namespace of the schema that defines this constraint.
    pub target_namespace: Option<NameId>,
}

impl CompiledIdentityConstraint {
    /// Compile an identity constraint from its parsed data.
    ///
    /// The `schema_xpath_default_ns` is the schema-level fallback for the
    /// three-level `xpathDefaultNamespace` cascade (field > selector > schema).
    pub fn compile(
        data: &IdentityConstraintData,
        key: IdentityConstraintKey,
        name_table: &NameTable,
        schema_xpath_default_ns: Option<NameId>,
        target_namespace: Option<NameId>,
        xsd_version: XsdVersion,
    ) -> Result<Self, IdentityXPathError> {
        // Compile selector
        let selector = Asttree::compile_selector(
            &data.selector.xpath,
            &data.selector.ns_snapshot,
            name_table,
            data.selector.xpath_default_namespace.as_deref(),
            schema_xpath_default_ns,
            target_namespace,
            xsd_version,
        )?;

        // Determine the selector-level xpath_default_namespace for cascading to fields.
        // If the selector has its own xpathDefaultNamespace, resolve it to a NameId;
        // otherwise fall back to the schema-level value.
        let selector_level_ns = match &data.selector.xpath_default_namespace {
            Some(val) => match val.as_str() {
                "##targetNamespace" => target_namespace,
                "##local" => None,
                uri => Some(name_table.add(uri)),
            },
            None => schema_xpath_default_ns,
        };

        // Compile fields
        let fields: Vec<Asttree> = data
            .fields
            .iter()
            .map(|f| {
                Asttree::compile_field(
                    &f.xpath,
                    &f.ns_snapshot,
                    name_table,
                    f.xpath_default_namespace.as_deref(),
                    selector_level_ns,
                    target_namespace,
                    xsd_version,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        let field_count = fields.len();

        Ok(CompiledIdentityConstraint {
            key,
            name: data.name,
            kind: data.kind,
            selector,
            fields,
            refer: data.refer.clone(),
            refer_key: None,
            field_count,
            target_namespace,
        })
    }
}

// ---------------------------------------------------------------------------
// KeyFieldValue
// ---------------------------------------------------------------------------

/// Per-field extracted value for key sequence equality.
///
/// Equality semantics follow the XSD spec:
/// - If both have `typed_value` with the same `PrimitiveTypeCode`, compare in
///   value space.  For the `Decimal` primitive type (which covers `xs:decimal`,
///   `xs:integer`, and all integer-derived types), comparison is performed
///   numerically so that, e.g., `xs:decimal "1"` equals `xs:unsignedByte "1"`.
/// - For all other primitive types with the same `PrimitiveTypeCode`, compare
///   via `XmlValue::PartialEq` (structural comparison of `XmlValueKind`).
/// - Different primitive types are *never* equal (no type promotion for IC equality).
/// - If either value is untyped, compare `string_value` (string equality).
/// - If either value is a list, compare member for member (Structures
///   §3.11.4: "pairwise equal or identical"), each member by the rules above
///   in the type that validated it — for a list of a union, the member type
///   that accepted it; an atomic value counts as a list of one member, and
///   all empty lists are equal (Datatypes §2.2.1).
#[derive(Debug, Clone)]
pub struct KeyFieldValue {
    pub string_value: String,
    pub typed_value: Option<XmlValue>,
}

impl PartialEq for KeyFieldValue {
    fn eq(&self, other: &Self) -> bool {
        use crate::types::validators::seconds_beyond_capacity;
        match (&self.typed_value, &other.typed_value) {
            (Some(a), Some(b))
                if (holds_seconds(a) || holds_seconds(b))
                    && (seconds_beyond_capacity(&self.string_value)
                        || seconds_beyond_capacity(&other.string_value)) =>
            {
                // A seconds number with more fraction digits than the typed
                // value holds: compare the literals' fractions as written.
                literal_values_equal_or_identical(a, &self.string_value, b, &other.string_value)
            }
            (Some(a), Some(b)) => {
                let (a, b) = (unwrap_union(a), unwrap_union(b));
                if list_members(a).is_some() || list_members(b).is_some() {
                    return list_ic_eq(a, b);
                }
                atomic_ic_eq(a, b).unwrap_or_else(|| self.string_value == other.string_value)
            }
            _ => self.string_value == other.string_value,
        }
    }
}

impl Eq for KeyFieldValue {}

// ---------------------------------------------------------------------------
// KeySequence
// ---------------------------------------------------------------------------

/// Complete key sequence from one selector match.
///
/// Each slot corresponds to a `<field>` expression. `None` means the field
/// did not select a node (missing value).
#[derive(Debug, Clone)]
pub struct KeySequence {
    pub fields: Vec<Option<KeyFieldValue>>,
}

impl KeySequence {
    /// Returns `true` if all fields have a value.
    pub fn is_complete(&self) -> bool {
        self.fields.iter().all(|f| f.is_some())
    }
}

impl PartialEq for KeySequence {
    fn eq(&self, other: &Self) -> bool {
        if self.fields.len() != other.fields.len() {
            return false;
        }
        for (a, b) in self.fields.iter().zip(other.fields.iter()) {
            match (a, b) {
                (Some(va), Some(vb)) => {
                    if va != vb {
                        return false;
                    }
                }
                (None, None) => {
                    // Both absent — equal for that slot
                }
                _ => return false,
            }
        }
        true
    }
}

impl Eq for KeySequence {}

// ---------------------------------------------------------------------------
// KeyTable
// ---------------------------------------------------------------------------

/// Collection of key sequences for one constraint activation with duplicate detection.
pub struct KeyTable {
    /// Arena key identifying which identity constraint produced this table.
    pub ic_key: IdentityConstraintKey,
    pub constraint_name: NameId,
    pub kind: IdentityKind,
    pub sequences: Vec<KeySequence>,
}

impl KeyTable {
    /// Create a new empty key table.
    pub fn new(ic_key: IdentityConstraintKey, constraint_name: NameId, kind: IdentityKind) -> Self {
        KeyTable {
            ic_key,
            constraint_name,
            kind,
            sequences: Vec::new(),
        }
    }

    /// Add a key sequence, performing duplicate/completeness checks as appropriate.
    ///
    /// - **Key**: error if incomplete (`cvc-identity-constraint.4.2.1`), error if
    ///   duplicate (`cvc-identity-constraint.4.2.2`).
    /// - **Unique**: check duplicate only if complete (incomplete sequences are skipped
    ///   per XSD spec).
    /// - **Keyref**: just store (deferred to `check_keyref_against`).
    pub fn add_sequence(
        &mut self,
        seq: KeySequence,
        name_table: &NameTable,
        element_path: &str,
        location: Option<SourceLocation>,
    ) -> Vec<ValidationError> {
        let duplicate =
            self.duplicate_check(&seq, |table, seq| table.find_duplicate(seq).is_some());
        self.push_checked(seq, duplicate, name_table, element_path, location)
    }

    /// Whether `seq` must be checked for a duplicate, and if so, whether it is
    /// one (`has_duplicate`): keys always, uniques when complete, keyrefs
    /// never.
    fn duplicate_check(
        &self,
        seq: &KeySequence,
        has_duplicate: impl FnOnce(&Self, &KeySequence) -> bool,
    ) -> bool {
        match self.kind {
            IdentityKind::Key => has_duplicate(self, seq),
            IdentityKind::Unique => seq.is_complete() && has_duplicate(self, seq),
            IdentityKind::Keyref => false,
        }
    }

    /// Store `seq` and report what [`add_sequence`](Self::add_sequence)
    /// reports for it; `duplicate` is the result of the duplicate check.
    fn push_checked(
        &mut self,
        seq: KeySequence,
        duplicate: bool,
        name_table: &NameTable,
        element_path: &str,
        location: Option<SourceLocation>,
    ) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let name = name_table.resolve(self.constraint_name);

        match self.kind {
            IdentityKind::Key => {
                if !seq.is_complete() {
                    errors.push(error_with_path(
                        "cvc-identity-constraint.4.2.1",
                        format!("Key constraint '{}': not all fields have values", name),
                        location.clone(),
                        element_path,
                    ));
                }
                // Check for duplicate even if incomplete (per spec, key requires
                // completeness AND uniqueness)
                if duplicate {
                    errors.push(error_with_path(
                        "cvc-identity-constraint.4.2.2",
                        format!("Key constraint '{}': duplicate key value detected", name),
                        location,
                        element_path,
                    ));
                }
            }
            IdentityKind::Unique => {
                // For unique, only check duplicates when the sequence is complete
                if duplicate {
                    errors.push(error_with_path(
                        "cvc-identity-constraint.4.2.2",
                        format!("Unique constraint '{}': duplicate key value detected", name),
                        location,
                        element_path,
                    ));
                }
            }
            IdentityKind::Keyref => {
                // Just store — checked later via check_keyref_against
            }
        }

        self.sequences.push(seq);
        errors
    }

    /// Check all keyref sequences against a target key/unique table.
    ///
    /// Returns `cvc-identity-constraint.4.3` errors for unmatched keyrefs.
    pub fn check_keyref_against(
        &self,
        target: &KeyTable,
        name_table: &NameTable,
    ) -> Vec<ValidationError> {
        self.keyref_errors(target.constraint_name, name_table, |seq| {
            target.sequences.iter().any(|ts| ts == seq)
        })
    }

    /// `cvc-identity-constraint.4.3` for every complete keyref sequence that
    /// `found` does not find in the table of the constraint `target_name`.
    fn keyref_errors(
        &self,
        target_name: NameId,
        name_table: &NameTable,
        found: impl Fn(&KeySequence) -> bool,
    ) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let name = name_table.resolve(self.constraint_name);

        for seq in &self.sequences {
            if !seq.is_complete() {
                // Incomplete keyref sequences are not checked
                continue;
            }
            if !found(seq) {
                errors.push(error(
                    "cvc-identity-constraint.4.3",
                    format!(
                        "Keyref constraint '{}': no matching key value found in referenced constraint '{}'",
                        name,
                        name_table.resolve(target_name)
                    ),
                    None,
                ));
            }
        }

        errors
    }

    /// Linear scan for a duplicate of `seq` in the existing sequences.
    fn find_duplicate(&self, seq: &KeySequence) -> Option<usize> {
        self.sequences.iter().position(|existing| existing == seq)
    }
}

// ---------------------------------------------------------------------------
// KeyIndex / IndexedKeyTable
// ---------------------------------------------------------------------------

/// How the values one field of a key table holds compare, as far as the
/// index needs to know (see [`KeyIndex`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldClass {
    /// No value in this field yet (absent everywhere).
    Unknown,
    /// Values compared by their strings: no typed value, or a typed value
    /// without a primitive type ([`KeyFieldValue`]'s string fallback).
    Strings,
    /// Atomic values of a primitive type (compared in its value space).
    Atomic,
    /// List values (compared member by member).
    Lists,
    /// Values of more than one class: a pair of them may be equal through
    /// the string fallback or the list-of-one rule, so this field does not
    /// discriminate in the index.
    Mixed,
}

/// One field value's class and the key its class hashes, for
/// [`KeyIndex`]. Equal values (by [`KeyFieldValue`]'s `==`) of one class
/// always get the same key; unequal ones may share one (a bucket is then
/// searched with `==`).
fn field_class_and_key(value: &KeyFieldValue, hasher: &mut ahash::AHasher) -> FieldClass {
    use crate::types::value::{XmlAtomicValue as V, XmlValueKind as K};
    use std::hash::Hash;
    let Some(typed) = value.typed_value.as_ref() else {
        1u8.hash(hasher);
        value.string_value.hash(hasher);
        return FieldClass::Strings;
    };
    let typed = unwrap_union(typed);
    let K::Atomic(atom) = &typed.value else {
        if list_members(typed).is_some() {
            3u8.hash(hasher);
            return FieldClass::Lists;
        }
        // `UntypedAtomic`: no primitive type.
        1u8.hash(hasher);
        value.string_value.hash(hasher);
        return FieldClass::Strings;
    };
    let Some(primitive) = typed.primitive_type() else {
        1u8.hash(hasher);
        value.string_value.hash(hasher);
        return FieldClass::Strings;
    };
    2u8.hash(hasher);
    match (primitive, atom) {
        (PrimitiveTypeCode::String, V::String(s)) | (PrimitiveTypeCode::AnyUri, V::AnyUri(s)) => {
            primitive.hash(hasher);
            s.hash(hasher);
        }
        (PrimitiveTypeCode::Boolean, V::Boolean(b)) => {
            primitive.hash(hasher);
            b.hash(hasher);
        }
        (PrimitiveTypeCode::Decimal, _) => {
            primitive.hash(hasher);
            // The exact value, normalized (`1.0`, `1`, `+01.00` are one
            // value), read from the canonical form every decimal-family value
            // displays as — a `Decimal`, an `Integer`, or the lexical form of
            // a value beyond `Decimal`'s capacity.
            let text = atom.to_string();
            match crate::types::exact_decimal::ExactDecimal::parse(&text) {
                Some(exact) => exact.to_string().hash(hasher),
                None => text.hash(hasher),
            }
        }
        (PrimitiveTypeCode::Float, V::Float(x)) => {
            primitive.hash(hasher);
            // `0 = -0`; NaN is identical to NaN (see `xml_atomic_nan_eq`).
            let bits = if x.is_nan() {
                u32::MAX
            } else if *x == 0.0 {
                0
            } else {
                x.to_bits()
            };
            bits.hash(hasher);
        }
        (PrimitiveTypeCode::Double, V::Double(x)) => {
            primitive.hash(hasher);
            let bits = if x.is_nan() {
                u64::MAX
            } else if *x == 0.0 {
                0
            } else {
                x.to_bits()
            };
            bits.hash(hasher);
        }
        (PrimitiveTypeCode::QName, V::QName(q)) | (PrimitiveTypeCode::Notation, V::Notation(q)) => {
            primitive.hash(hasher);
            q.hash(hasher);
        }
        (PrimitiveTypeCode::HexBinary, V::HexBinary(b))
        | (PrimitiveTypeCode::Base64Binary, V::Base64Binary(b)) => {
            primitive.hash(hasher);
            b.hash(hasher);
        }
        // Dates, times, durations (equal across time zones and by the four
        // reference dateTimes, seconds compared as written) and anything
        // else: one key for all of them.
        _ => 0xFFu8.hash(hasher),
    }
    FieldClass::Atomic
}

/// A hash index over a key table's sequences, for its duplicate check and
/// keyref lookups in expected constant time per sequence.
///
/// A sequence's hash combines one key per field. Equality of field values
/// ([`KeyFieldValue`]'s `==`) is not transitive across classes — a typed
/// value equals an untyped one by its string, an atomic value equals the
/// list of that value — so a field whose values come from more than one
/// [`FieldClass`] stops contributing (and the index is rebuilt, once per
/// field). Within a class, equal values hash equally. A bucket is searched
/// with `==`, so a shared hash is never a false match.
#[derive(Debug, Default)]
struct KeyIndex {
    classes: Vec<FieldClass>,
    /// Hash of a sequence → its last indexed position; `next` chains the
    /// earlier positions with the same hash (`u32::MAX` ends a chain).
    heads: std::collections::HashMap<u64, u32, std::hash::BuildHasherDefault<ahash::AHasher>>,
    next: Vec<u32>,
}

impl KeyIndex {
    const END: u32 = u32::MAX;

    /// The classes of `seq`'s fields (`Unknown` for an absent one).
    fn classes_of(seq: &KeySequence) -> Vec<FieldClass> {
        let mut scratch = ahash::AHasher::default();
        seq.fields
            .iter()
            .map(|f| match f {
                Some(v) => field_class_and_key(v, &mut scratch),
                None => FieldClass::Unknown,
            })
            .collect()
    }

    /// The hash of `seq` under this index's classes.
    fn hash(&self, seq: &KeySequence) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = ahash::AHasher::default();
        for (i, field) in seq.fields.iter().enumerate() {
            if self.classes.get(i) == Some(&FieldClass::Mixed) {
                4u8.hash(&mut hasher);
                continue;
            }
            match field {
                None => 0u8.hash(&mut hasher),
                Some(v) => {
                    field_class_and_key(v, &mut hasher);
                }
            }
        }
        seq.fields.len().hash(&mut hasher);
        hasher.finish()
    }

    /// Merge `seq`'s classes into the index's; `true` when a field became
    /// `Mixed` (the existing entries must be rehashed).
    fn merge_classes(&mut self, seq: &KeySequence) -> bool {
        let classes = Self::classes_of(seq);
        if self.classes.len() < classes.len() {
            self.classes.resize(classes.len(), FieldClass::Unknown);
        }
        let mut became_mixed = false;
        for (have, new) in self.classes.iter_mut().zip(classes) {
            match (*have, new) {
                (_, FieldClass::Unknown) | (FieldClass::Mixed, _) => {}
                (FieldClass::Unknown, c) => *have = c,
                (h, c) if h == c => {}
                _ => {
                    *have = FieldClass::Mixed;
                    became_mixed = true;
                }
            }
        }
        became_mixed
    }

    /// Index `sequences[position]`, after merging its classes.
    fn insert(&mut self, sequences: &[KeySequence], position: usize) {
        if self.merge_classes(&sequences[position]) {
            self.rebuild(&sequences[..position]);
        }
        self.link(sequences, position);
    }

    fn link(&mut self, sequences: &[KeySequence], position: usize) {
        let h = self.hash(&sequences[position]);
        let previous = self.heads.insert(h, position as u32).unwrap_or(Self::END);
        if self.next.len() <= position {
            self.next.resize(position + 1, Self::END);
        }
        self.next[position] = previous;
    }

    /// Rehash `sequences` (all indexed before) under the current classes.
    fn rebuild(&mut self, sequences: &[KeySequence]) {
        self.heads.clear();
        self.next.clear();
        for position in 0..sequences.len() {
            self.link(sequences, position);
        }
    }

    /// Whether `sequences` holds a sequence equal to `probe`. Falls back to
    /// a scan when `probe` has a field of another class than the table's
    /// (equality then may hold through the string fallback only).
    fn contains(&self, sequences: &[KeySequence], probe: &KeySequence) -> bool {
        let comparable = Self::classes_of(probe)
            .iter()
            .enumerate()
            .all(|(i, class)| {
                match (
                    self.classes.get(i).copied().unwrap_or(FieldClass::Unknown),
                    *class,
                ) {
                    (FieldClass::Mixed, _)
                    | (_, FieldClass::Unknown)
                    | (FieldClass::Unknown, _) => true,
                    (have, probe) => have == probe,
                }
            });
        if !comparable {
            return sequences.iter().any(|s| s == probe);
        }
        let mut at = self
            .heads
            .get(&self.hash(probe))
            .copied()
            .unwrap_or(Self::END);
        while at != Self::END {
            if &sequences[at as usize] == probe {
                return true;
            }
            at = self.next[at as usize];
        }
        false
    }
}

/// A [`KeyTable`] with a [`KeyIndex`]: what the validation runtime keeps for
/// `key` and `unique` node tables, so that the duplicate check and the keyref
/// lookup do not scan the table (a table of n sequences costs O(n) to build
/// instead of O(n²)). Verdicts and error order are those of [`KeyTable`].
pub(crate) struct IndexedKeyTable {
    table: KeyTable,
    index: KeyIndex,
}

impl IndexedKeyTable {
    pub(crate) fn new(
        ic_key: IdentityConstraintKey,
        constraint_name: NameId,
        kind: IdentityKind,
    ) -> Self {
        IndexedKeyTable {
            table: KeyTable::new(ic_key, constraint_name, kind),
            index: KeyIndex::default(),
        }
    }

    /// [`KeyTable::add_sequence`], with the indexed duplicate check.
    pub(crate) fn add_sequence(
        &mut self,
        seq: KeySequence,
        name_table: &NameTable,
        element_path: &str,
        location: Option<SourceLocation>,
    ) -> Vec<ValidationError> {
        if self.table.kind == IdentityKind::Keyref {
            // Keyref tables are only ever probed against, never searched.
            return self
                .table
                .add_sequence(seq, name_table, element_path, location);
        }
        let index = &self.index;
        let duplicate = self
            .table
            .duplicate_check(&seq, |table, seq| index.contains(&table.sequences, seq));
        let errors = self
            .table
            .push_checked(seq, duplicate, name_table, element_path, location);
        let position = self.table.sequences.len() - 1;
        self.index.insert(&self.table.sequences, position);
        errors
    }

    /// Append the sequences of `other` (a node table of the same constraint
    /// propagated from a child, §3.11.5), in order, without checks.
    pub(crate) fn append(&mut self, other: IndexedKeyTable) {
        let start = self.table.sequences.len();
        self.table.sequences.extend(other.table.sequences);
        if self.table.kind != IdentityKind::Keyref {
            for position in start..self.table.sequences.len() {
                self.index.insert(&self.table.sequences, position);
            }
        }
    }

    /// [`KeyTable::check_keyref_against`] with this table as the target.
    pub(crate) fn check_keyref(
        &self,
        keyref: &KeyTable,
        name_table: &NameTable,
    ) -> Vec<ValidationError> {
        keyref.keyref_errors(self.table.constraint_name, name_table, |seq| {
            self.index.contains(&self.table.sequences, seq)
        })
    }

    pub(crate) fn table(&self) -> &KeyTable {
        &self.table
    }

    pub(crate) fn into_table(self) -> KeyTable {
        self.table
    }
}

// ---------------------------------------------------------------------------
// FieldCollectionFrame
// ---------------------------------------------------------------------------

/// One level of field collection for a single selector match.
///
/// A stack of these is maintained in `ConstraintStruct` to handle
/// overlapping (nested) selector matches correctly — e.g. `.//item`
/// matching both an outer and inner `<item>`.
struct FieldCollectionFrame {
    /// Field axis matchers (one per `<field>`), cloned from templates.
    fields: Vec<ActiveAxis>,
    /// Current key sequence being collected (one slot per field).
    current_key_sequence: Vec<Option<KeyFieldValue>>,
    /// How many times each field slot has been matched so far.
    /// Used to detect multi-node field violations (§3.11.4 cvc-identity-constraint.4.2.1).
    field_match_count: Vec<u8>,
}

// ---------------------------------------------------------------------------
// ConstraintStruct
// ---------------------------------------------------------------------------

/// Per-constraint activation state during streaming validation.
///
/// Each time an identity constraint's scope element is entered, a fresh
/// `ConstraintStruct` is created. It drives selector and field `ActiveAxis`
/// matchers, collects key sequences, and accumulates them in a `KeyTable`.
///
/// Nested selector matches (e.g. `.//item` with nested `<item>` elements)
/// are handled via a stack of [`FieldCollectionFrame`]s. Each selector hit
/// pushes a new frame; each selector exit pops the top frame and finalizes
/// its key sequence.
pub(crate) struct ConstraintStruct {
    /// Arena key for the compiled constraint (used for deactivation lookup).
    pub ic_key: IdentityConstraintKey,
    /// Selector axis matcher.
    pub selector: ActiveAxis,
    /// Field Asttree templates for cloning into new collection frames.
    field_asttrees: Vec<Asttree>,
    /// Number of fields (cached from compiled constraint).
    field_count: usize,
    /// Stack of active field collection frames (one per nested selector match).
    /// Empty means no selector match is currently active.
    collection_stack: Vec<FieldCollectionFrame>,
    /// Key table accumulating complete key sequences.
    pub key_table: IndexedKeyTable,
    /// Validation-stack index of the element whose declaration carries this
    /// constraint (its scope element). Every violation of the constraint makes
    /// *that* element invalid — Structures §3.3.4.3 Element Locally Valid
    /// (Element) clause 6: "E is ·valid· with respect to each of the
    /// {identity-constraint definitions} as per Identity-constraint Satisfied
    /// (§3.11.4)" — whichever descendant the violation was detected at.
    /// Set by the runtime when it activates the constraint.
    pub owner_depth: usize,
}

impl ConstraintStruct {
    /// Create a new constraint activation state.
    ///
    /// Stores Asttree templates for cloning into fresh `ActiveAxis` instances
    /// on each selector match.
    pub fn new(compiled: &CompiledIdentityConstraint) -> Self {
        let selector = ActiveAxis::new(compiled.selector.clone());

        ConstraintStruct {
            ic_key: compiled.key,
            selector,
            field_asttrees: compiled.fields.clone(),
            field_count: compiled.field_count,
            collection_stack: Vec::new(),
            key_table: IndexedKeyTable::new(compiled.key, compiled.name, compiled.kind),
            owner_depth: 0,
        }
    }

    /// Whether field collection is active (at least one selector match on the stack).
    #[cfg(test)]
    pub fn collecting_fields(&self) -> bool {
        !self.collection_stack.is_empty()
    }

    /// Activate the constraint for the current scope element.
    ///
    /// Returns `true` if the selector is a bare `.` match (immediate scope match).
    pub fn activate(&mut self) -> bool {
        let is_scope_match = self.selector.activate();
        if is_scope_match {
            // Bare "." — immediately start collecting fields
            self.push_field_collection();
        }
        is_scope_match
    }

    /// Handle an element start event.
    ///
    /// Advances the selector; if it matches, pushes a new field collection
    /// frame. Existing frames continue receiving element events for correct
    /// depth tracking.
    pub fn start_element(&mut self, local_name: NameId, ns: NameId) {
        self.selector.move_to_start_element(local_name, ns);

        // Advance ALL existing frames' field axes first (depth tracking for
        // outer frames that are "paused" by an inner selector match).
        for frame in &mut self.collection_stack {
            for field in &mut frame.fields {
                field.move_to_start_element(local_name, ns);
            }
        }

        if self.selector.entered_match() {
            // New selector match — push a fresh frame (does NOT advance the
            // new frame's fields because the matched element is the field scope root).
            self.push_field_collection();
        }
    }

    /// Handle an element start event for an element inside a wildcard
    /// `processContents="skip"` subtree. Bumps depth tracking on selectors and
    /// fields so the matching state pops correctly when the subtree ends, but
    /// suppresses any new matches: skipped elements are outside the schema's
    /// validation scope and must not be visible to identity-constraint
    /// selectors/fields per XSD 1.1 §3.11.4 (see wild101..103).
    pub fn start_element_skipped(&mut self, _local_name: NameId, _ns: NameId) {
        self.selector.move_to_skipped_element();
        for frame in &mut self.collection_stack {
            for field in &mut frame.fields {
                field.move_to_skipped_element();
            }
        }
    }

    /// Return all field indices whose axis matches the given attribute.
    ///
    /// Multiple fields can match the same attribute (e.g. repeated or
    /// overlapping field expressions), so all matching indices are returned.
    pub fn matching_fields(&self, local_name: NameId, ns: NameId) -> Vec<usize> {
        let frame = match self.collection_stack.last() {
            Some(f) => f,
            None => return Vec::new(),
        };
        let mut indices = Vec::new();
        for (i, field) in frame.fields.iter().enumerate() {
            if field.matches_attribute(local_name, ns) {
                indices.push(i);
            }
        }
        indices
    }

    /// Store a field value at the given index in the topmost collection frame.
    ///
    /// Returns `true` if this is a second (or later) match for the same field slot
    /// in the current selector-bound element — the caller should report
    /// `cvc-identity-constraint.4.2.1`.
    pub fn set_field_value(
        &mut self,
        field_idx: usize,
        string_value: String,
        typed_value: Option<XmlValue>,
    ) -> bool {
        if let Some(frame) = self.collection_stack.last_mut() {
            if field_idx < frame.current_key_sequence.len() {
                let already_matched = frame.field_match_count[field_idx] > 0;
                frame.field_match_count[field_idx] =
                    frame.field_match_count[field_idx].saturating_add(1);
                frame.current_key_sequence[field_idx] = Some(KeyFieldValue {
                    string_value,
                    typed_value,
                });
                return already_matched;
            }
        }
        false
    }

    /// Increment the field match count without storing a value.
    ///
    /// Used for skip-processed wildcard attributes: the attribute is selected by
    /// the field XPath (counts toward multi-node detection) but cannot contribute
    /// a typed value, so the field slot stays absent.
    ///
    /// Returns `true` if this is a second (or later) match for the same field slot.
    pub fn increment_field_match_count(&mut self, field_idx: usize) -> bool {
        if let Some(frame) = self.collection_stack.last_mut() {
            if field_idx < frame.field_match_count.len() {
                let already_matched = frame.field_match_count[field_idx] > 0;
                frame.field_match_count[field_idx] =
                    frame.field_match_count[field_idx].saturating_add(1);
                return already_matched;
            }
        }
        false
    }

    /// Handle an element end event with text content for element-field matching.
    ///
    /// Advances field axes in ALL frames FIRST (to detect `exited_match` and
    /// set values from text content), THEN advances the selector.
    /// On selector exit, pops the topmost frame and finalizes its key sequence.
    ///
    /// `is_nil` indicates that the element carries `xsi:nil="true"`.  Nilled
    /// elements have no typed value but ARE selected by field XPaths; they
    /// contribute an empty-string field value (with no type information) so
    /// that two nil sibling elements are detected as duplicates under
    /// `xs:unique` / `xs:key`.
    ///
    /// `is_complex_content` must be `true` when the closing element has element-only
    /// or mixed content type.  Per XSD §3.11.4 (cvc-identity-constraint.4), a field
    /// that selects such an element cannot yield a valid typed value; the constraint
    /// is violated for every identity-constraint kind (key *and* unique).
    #[allow(clippy::too_many_arguments)]
    pub fn end_element_with_text(
        &mut self,
        text_content: &str,
        typed_value: Option<&XmlValue>,
        is_nil: bool,
        is_complex_content: bool,
        name_table: &NameTable,
        element_path: &str,
        location: Option<SourceLocation>,
    ) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        // 1. Advance field axes in ALL frames FIRST (detect exited element-field matches)
        for frame in &mut self.collection_stack {
            for (field_idx, field) in frame.fields.iter_mut().enumerate() {
                field.end_element();
                if field.exited_match() {
                    if frame.field_match_count[field_idx] > 0 {
                        // §3.11.4 cvc-identity-constraint.4.2.1: field evaluates to
                        // a node-set with more than one member.
                        let name = name_table.resolve(self.key_table.table().constraint_name);
                        errors.push(error_with_path(
                            "cvc-identity-constraint.4.2.1",
                            format!(
                                "Identity constraint '{}': field {} matches more than one node",
                                name,
                                field_idx + 1
                            ),
                            location.clone(),
                            element_path,
                        ));
                    } else if frame.current_key_sequence[field_idx].is_none() {
                        // First match — set the field value (or report complex content error).
                        frame.field_match_count[field_idx] =
                            frame.field_match_count[field_idx].saturating_add(1);
                        if is_complex_content {
                            // XSD §3.11.4 cvc-identity-constraint.4: a field that selects an
                            // element with element-only or mixed content type cannot contribute
                            // a typed value.  This is a constraint violation for both xs:key
                            // and xs:unique (not merely "absent"), because the node IS present
                            // but its type precludes a meaningful field value.
                            let constraint_name =
                                name_table.resolve(self.key_table.table().constraint_name);
                            errors.push(error_with_path(
                                "cvc-identity-constraint.4",
                                format!(
                                    "Identity constraint '{}': field matched an element with \
                                     complex content type (element-only or mixed); \
                                     typed field values must be simple-typed",
                                    constraint_name
                                ),
                                location.clone(),
                                element_path,
                            ));
                        } else if typed_value.is_some() {
                            // Normal case: element has a simple typed value.
                            frame.current_key_sequence[field_idx] = Some(KeyFieldValue {
                                string_value: text_content.to_string(),
                                typed_value: typed_value.cloned(),
                            });
                        } else if is_nil {
                            // Nilled element (xsi:nil="true"): the typed value is absent
                            // but the element node IS selected.  Contribute an empty-string
                            // binding (no type) so that two nil siblings compare equal and
                            // trigger a duplicate violation under xs:unique / xs:key.
                            frame.current_key_sequence[field_idx] = Some(KeyFieldValue {
                                string_value: String::new(),
                                typed_value: None,
                            });
                        }
                        // For empty-content elements that are neither typed nor nilled,
                        // the slot stays None (absent).
                    }
                }
            }
        }

        // 2. Then advance selector
        self.selector.end_element();

        // 3. If selector exits match, pop frame and finalize key sequence
        if self.selector.exited_match() {
            if let Some(frame) = self.collection_stack.pop() {
                let seq = KeySequence {
                    fields: frame.current_key_sequence,
                };
                errors.extend(
                    self.key_table
                        .add_sequence(seq, name_table, element_path, location),
                );
            }
        }

        errors
    }

    /// Whether the selector axis is still active.
    pub fn is_active(&self) -> bool {
        self.selector.is_active()
    }

    /// Push a new field collection frame: clone Asttrees into fresh ActiveAxis
    /// instances and initialize an empty key sequence.
    fn push_field_collection(&mut self) {
        let fields: Vec<ActiveAxis> = self
            .field_asttrees
            .iter()
            .map(|ast| {
                let mut axis = ActiveAxis::new(ast.clone());
                axis.activate();
                axis
            })
            .collect();
        self.collection_stack.push(FieldCollectionFrame {
            fields,
            current_key_sequence: vec![None; self.field_count],
            field_match_count: vec![0; self.field_count],
        });
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespace::context::NamespaceContextSnapshot;
    use crate::namespace::table::NameTable;
    use crate::schema::model::XsdVersion;
    use crate::types::value::{XmlAtomicValue, XmlValueKind};
    use crate::types::XmlTypeCode;

    // -- Helpers --

    fn make_string_field(s: &str) -> KeyFieldValue {
        KeyFieldValue {
            string_value: s.to_string(),
            typed_value: None,
        }
    }

    fn make_typed_field(s: &str, type_code: XmlTypeCode, value: XmlValueKind) -> KeyFieldValue {
        KeyFieldValue {
            string_value: s.to_string(),
            typed_value: Some(XmlValue::new(type_code, value)),
        }
    }

    fn make_name_table() -> NameTable {
        NameTable::new()
    }

    // -----------------------------------------------------------------------
    // KeyFieldValue equality
    // -----------------------------------------------------------------------

    #[test]
    fn key_field_value_untyped_same_string_equal() {
        let a = make_string_field("hello");
        let b = make_string_field("hello");
        assert_eq!(a, b);
    }

    #[test]
    fn key_field_value_untyped_different_string_not_equal() {
        let a = make_string_field("hello");
        let b = make_string_field("world");
        assert_ne!(a, b);
    }

    #[test]
    fn key_field_value_typed_same_primitive_same_value_equal() {
        let a = make_typed_field(
            "42",
            XmlTypeCode::Integer,
            XmlValueKind::Atomic(XmlAtomicValue::Integer(42.into())),
        );
        let b = make_typed_field(
            "42",
            XmlTypeCode::Integer,
            XmlValueKind::Atomic(XmlAtomicValue::Integer(42.into())),
        );
        assert_eq!(a, b);
    }

    #[test]
    fn key_field_value_typed_same_primitive_different_value_not_equal() {
        let a = make_typed_field(
            "42",
            XmlTypeCode::Integer,
            XmlValueKind::Atomic(XmlAtomicValue::Integer(42.into())),
        );
        let b = make_typed_field(
            "99",
            XmlTypeCode::Integer,
            XmlValueKind::Atomic(XmlAtomicValue::Integer(99.into())),
        );
        assert_ne!(a, b);
    }

    #[test]
    fn key_field_value_typed_different_primitive_not_equal() {
        // xs:integer(5) vs xs:string("5") — different primitive types → never equal
        let a = make_typed_field(
            "5",
            XmlTypeCode::Integer,
            XmlValueKind::Atomic(XmlAtomicValue::Integer(5.into())),
        );
        let b = make_typed_field(
            "5",
            XmlTypeCode::String,
            XmlValueKind::Atomic(XmlAtomicValue::String("5".to_string())),
        );
        assert_ne!(a, b);
    }

    #[test]
    fn key_field_value_one_typed_one_untyped_fallback_string() {
        // One typed + one untyped → fall back to string comparison
        let a = make_typed_field(
            "hello",
            XmlTypeCode::String,
            XmlValueKind::Atomic(XmlAtomicValue::String("hello".to_string())),
        );
        let b = make_string_field("hello");
        assert_eq!(a, b);

        let c = make_typed_field(
            "42",
            XmlTypeCode::Integer,
            XmlValueKind::Atomic(XmlAtomicValue::Integer(42.into())),
        );
        let d = make_string_field("99");
        assert_ne!(c, d);
    }

    // -----------------------------------------------------------------------
    // KeySequence
    // -----------------------------------------------------------------------

    #[test]
    fn key_sequence_is_complete_all_present() {
        let seq = KeySequence {
            fields: vec![Some(make_string_field("a")), Some(make_string_field("b"))],
        };
        assert!(seq.is_complete());
    }

    #[test]
    fn key_sequence_is_complete_missing_field() {
        let seq = KeySequence {
            fields: vec![Some(make_string_field("a")), None],
        };
        assert!(!seq.is_complete());
    }

    #[test]
    fn key_sequence_equal() {
        let a = KeySequence {
            fields: vec![Some(make_string_field("x")), Some(make_string_field("y"))],
        };
        let b = KeySequence {
            fields: vec![Some(make_string_field("x")), Some(make_string_field("y"))],
        };
        assert_eq!(a, b);
    }

    #[test]
    fn key_sequence_not_equal() {
        let a = KeySequence {
            fields: vec![Some(make_string_field("x")), Some(make_string_field("y"))],
        };
        let b = KeySequence {
            fields: vec![Some(make_string_field("x")), Some(make_string_field("z"))],
        };
        assert_ne!(a, b);
    }

    #[test]
    fn key_sequence_both_none_equal() {
        let a = KeySequence {
            fields: vec![Some(make_string_field("x")), None],
        };
        let b = KeySequence {
            fields: vec![Some(make_string_field("x")), None],
        };
        assert_eq!(a, b);
    }

    // -----------------------------------------------------------------------
    // KeyTable duplicate detection
    // -----------------------------------------------------------------------

    #[test]
    fn key_table_key_duplicate_error() {
        let nt = make_name_table();
        let name = nt.add("pk");
        let mut table = KeyTable::new(IdentityConstraintKey::default(), name, IdentityKind::Key);

        let seq1 = KeySequence {
            fields: vec![Some(make_string_field("1"))],
        };
        let errs = table.add_sequence(seq1, &nt, "/root/item[1]", None);
        assert!(errs.is_empty());

        let seq2 = KeySequence {
            fields: vec![Some(make_string_field("1"))],
        };
        let errs = table.add_sequence(seq2, &nt, "/root/item[2]", None);
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].constraint, "cvc-identity-constraint.4.2.2");
    }

    #[test]
    fn key_table_key_incomplete_error() {
        let nt = make_name_table();
        let name = nt.add("pk");
        let mut table = KeyTable::new(IdentityConstraintKey::default(), name, IdentityKind::Key);

        let seq = KeySequence {
            fields: vec![Some(make_string_field("a")), None],
        };
        let errs = table.add_sequence(seq, &nt, "/root/item[1]", None);
        assert!(errs
            .iter()
            .any(|e| e.constraint == "cvc-identity-constraint.4.2.1"));
    }

    #[test]
    fn key_table_unique_duplicate_error() {
        let nt = make_name_table();
        let name = nt.add("uq");
        let mut table = KeyTable::new(IdentityConstraintKey::default(), name, IdentityKind::Unique);

        let seq1 = KeySequence {
            fields: vec![Some(make_string_field("val"))],
        };
        let errs = table.add_sequence(seq1, &nt, "/root/item[1]", None);
        assert!(errs.is_empty());

        let seq2 = KeySequence {
            fields: vec![Some(make_string_field("val"))],
        };
        let errs = table.add_sequence(seq2, &nt, "/root/item[2]", None);
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].constraint, "cvc-identity-constraint.4.2.2");
    }

    #[test]
    fn key_table_unique_incomplete_no_error() {
        let nt = make_name_table();
        let name = nt.add("uq");
        let mut table = KeyTable::new(IdentityConstraintKey::default(), name, IdentityKind::Unique);

        let seq = KeySequence { fields: vec![None] };
        let errs = table.add_sequence(seq, &nt, "/root/item[1]", None);
        assert!(errs.is_empty());
    }

    #[test]
    fn key_table_keyref_no_error() {
        let nt = make_name_table();
        let name = nt.add("fk");
        let mut table = KeyTable::new(IdentityConstraintKey::default(), name, IdentityKind::Keyref);

        let seq = KeySequence {
            fields: vec![Some(make_string_field("anything"))],
        };
        let errs = table.add_sequence(seq, &nt, "/root/item[1]", None);
        assert!(errs.is_empty());
    }

    #[test]
    fn check_keyref_against_matching() {
        let nt = make_name_table();
        let pk_name = nt.add("pk");
        let fk_name = nt.add("fk");

        let mut key_table =
            KeyTable::new(IdentityConstraintKey::default(), pk_name, IdentityKind::Key);
        key_table.sequences.push(KeySequence {
            fields: vec![Some(make_string_field("1"))],
        });

        let mut keyref_table = KeyTable::new(
            IdentityConstraintKey::default(),
            fk_name,
            IdentityKind::Keyref,
        );
        keyref_table.sequences.push(KeySequence {
            fields: vec![Some(make_string_field("1"))],
        });

        let errs = keyref_table.check_keyref_against(&key_table, &nt);
        assert!(errs.is_empty());
    }

    #[test]
    fn check_keyref_against_missing() {
        let nt = make_name_table();
        let pk_name = nt.add("pk");
        let fk_name = nt.add("fk");

        let key_table = KeyTable::new(IdentityConstraintKey::default(), pk_name, IdentityKind::Key);

        let mut keyref_table = KeyTable::new(
            IdentityConstraintKey::default(),
            fk_name,
            IdentityKind::Keyref,
        );
        keyref_table.sequences.push(KeySequence {
            fields: vec![Some(make_string_field("missing"))],
        });

        let errs = keyref_table.check_keyref_against(&key_table, &nt);
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].constraint, "cvc-identity-constraint.4.3");
    }

    // -----------------------------------------------------------------------
    // CompiledIdentityConstraint::compile
    // -----------------------------------------------------------------------

    fn make_selector_result(xpath: &str) -> crate::parser::frames::SelectorResult {
        crate::parser::frames::SelectorResult {
            xpath: xpath.to_string(),
            xpath_default_namespace: None,
            ns_snapshot: NamespaceContextSnapshot::default(),
            id: None,
            annotation: None,
            source: None,
        }
    }

    fn make_field_result(xpath: &str) -> crate::parser::frames::FieldResult {
        crate::parser::frames::FieldResult {
            xpath: xpath.to_string(),
            xpath_default_namespace: None,
            ns_snapshot: NamespaceContextSnapshot::default(),
            id: None,
            annotation: None,
            source: None,
        }
    }

    fn make_identity_data(
        kind: IdentityKind,
        name: NameId,
        selector_xpath: &str,
        field_xpaths: &[&str],
    ) -> IdentityConstraintData {
        IdentityConstraintData {
            kind,
            name,
            ref_name: None,
            refer: None,
            selector: make_selector_result(selector_xpath),
            fields: field_xpaths.iter().map(|x| make_field_result(x)).collect(),
            id: None,
            annotation: None,
            source: None,
        }
    }

    #[test]
    fn compile_simple_constraint() {
        let nt = make_name_table();
        let name = nt.add("testKey");
        let key = IdentityConstraintKey::default();

        let data = make_identity_data(IdentityKind::Key, name, "./item", &["@id"]);

        let compiled =
            CompiledIdentityConstraint::compile(&data, key, &nt, None, None, XsdVersion::V1_0);
        assert!(compiled.is_ok());
        let compiled = compiled.unwrap();
        assert_eq!(compiled.field_count, 1);
        assert_eq!(compiled.kind, IdentityKind::Key);
        assert_eq!(compiled.name, name);
    }

    #[test]
    fn compile_invalid_xpath_propagates_error() {
        let nt = make_name_table();
        let name = nt.add("badKey");
        let key = IdentityConstraintKey::default();

        let data = make_identity_data(IdentityKind::Key, name, "///invalid", &["@id"]);

        let result =
            CompiledIdentityConstraint::compile(&data, key, &nt, None, None, XsdVersion::V1_0);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // ConstraintStruct lifecycle
    // -----------------------------------------------------------------------

    #[test]
    fn constraint_struct_lifecycle() {
        let nt = make_name_table();
        let name = nt.add("testKey");
        let key = IdentityConstraintKey::default();

        let data = make_identity_data(IdentityKind::Key, name, "./item", &["@id"]);
        let compiled =
            CompiledIdentityConstraint::compile(&data, key, &nt, None, None, XsdVersion::V1_0)
                .unwrap();

        let mut cs = ConstraintStruct::new(&compiled);

        // Activate at scope element
        let scope_match = cs.activate();
        assert!(!scope_match); // ./item is not a bare "."
        assert!(cs.is_active());

        // Start element: "item" (the selector target)
        let item_name = nt.add("item");
        let empty_ns = NameId(0);
        cs.start_element(item_name, empty_ns);

        // The selector should have matched "item"
        assert!(cs.collecting_fields());

        // Check attribute "@id"
        let id_name = nt.add("id");
        let matches = cs.matching_fields(id_name, empty_ns);
        assert_eq!(matches, vec![0]);

        // Set field value
        cs.set_field_value(0, "val1".to_string(), None);

        // End element — should finalize key sequence
        let errors = cs.end_element_with_text("", None, false, false, &nt, "/root/item[1]", None);
        assert!(errors.is_empty());

        // Key table should have one sequence
        assert_eq!(cs.key_table.table().sequences.len(), 1);
        assert!(cs.key_table.table().sequences[0].is_complete());
        assert_eq!(
            cs.key_table.table().sequences[0].fields[0]
                .as_ref()
                .unwrap()
                .string_value,
            "val1"
        );
    }

    /// Nested selector matches: `.//item` with `<item><item/></item>`.
    /// Both outer and inner matches should produce independent key sequences.
    #[test]
    fn constraint_struct_nested_selector() {
        let nt = make_name_table();
        let name = nt.add("uq");
        let key = IdentityConstraintKey::default();

        let data = make_identity_data(IdentityKind::Unique, name, ".//item", &["@id"]);
        let compiled =
            CompiledIdentityConstraint::compile(&data, key, &nt, None, None, XsdVersion::V1_0)
                .unwrap();

        let mut cs = ConstraintStruct::new(&compiled);
        cs.activate();

        let item = nt.add("item");
        let id = nt.add("id");
        let ns = NameId(0);

        // Outer <item>
        cs.start_element(item, ns);
        assert!(cs.collecting_fields());
        let m = cs.matching_fields(id, ns);
        assert_eq!(m, vec![0]);
        cs.set_field_value(0, "outer".to_string(), None);

        // Inner <item> (nested match — pushes second frame)
        cs.start_element(item, ns);
        // Now two frames on the stack
        let m = cs.matching_fields(id, ns);
        assert_eq!(m, vec![0]); // top frame's field
        cs.set_field_value(0, "inner".to_string(), None);

        // End inner </item> — finalizes inner sequence
        let errors = cs.end_element_with_text("", None, false, false, &nt, "/root/item/item", None);
        assert!(errors.is_empty());
        assert_eq!(cs.key_table.table().sequences.len(), 1);
        assert_eq!(
            cs.key_table.table().sequences[0].fields[0]
                .as_ref()
                .unwrap()
                .string_value,
            "inner"
        );

        // Outer frame is still active
        assert!(cs.collecting_fields());

        // End outer </item> — finalizes outer sequence
        let errors = cs.end_element_with_text("", None, false, false, &nt, "/root/item", None);
        assert!(errors.is_empty());
        assert_eq!(cs.key_table.table().sequences.len(), 2);
        assert_eq!(
            cs.key_table.table().sequences[1].fields[0]
                .as_ref()
                .unwrap()
                .string_value,
            "outer"
        );

        // No more active frames
        assert!(!cs.collecting_fields());
    }

    /// Multiple fields matching the same attribute should all be populated.
    #[test]
    fn constraint_struct_multi_field_same_attr() {
        let nt = make_name_table();
        let name = nt.add("uq2");
        let key = IdentityConstraintKey::default();

        // Two fields both matching @id
        let data = make_identity_data(IdentityKind::Unique, name, "./item", &["@id", "@id"]);
        let compiled =
            CompiledIdentityConstraint::compile(&data, key, &nt, None, None, XsdVersion::V1_0)
                .unwrap();

        let mut cs = ConstraintStruct::new(&compiled);
        cs.activate();

        let item = nt.add("item");
        let id = nt.add("id");
        let ns = NameId(0);

        cs.start_element(item, ns);
        let matches = cs.matching_fields(id, ns);
        assert_eq!(matches, vec![0, 1]); // both fields match

        cs.set_field_value(0, "v".to_string(), None);
        cs.set_field_value(1, "v".to_string(), None);

        let errors = cs.end_element_with_text("", None, false, false, &nt, "/root/item", None);
        assert!(errors.is_empty());
        assert!(cs.key_table.table().sequences[0].is_complete());
    }

    /// The indexed table reports exactly what the linear [`KeyTable`] does,
    /// on sequences mixing every class of field value: strings, decimals in
    /// several forms, doubles (`0` / `-0` / NaN), dates, untyped values,
    /// lists (a singleton list equals its atomic value), absent fields — in
    /// several orders, so that fields turn `Mixed` at different points.
    #[test]
    fn indexed_key_table_agrees_with_the_linear_one() {
        use crate::types::value::{XmlAtomicValue, XmlValue, XmlValueKind};
        use num_bigint::BigInt;
        use rust_decimal::Decimal;
        use std::str::FromStr;

        let nt = make_name_table();
        let name = nt.add("k");
        let typed = |s: &str, v: XmlValue| KeyFieldValue {
            string_value: s.to_string(),
            typed_value: Some(v),
        };
        let int = |s: &str| {
            typed(
                s,
                XmlValue::integer(BigInt::from_str(s.trim_start_matches('+')).unwrap()),
            )
        };
        let dec = |s: &str| typed(s, XmlValue::decimal(Decimal::from_str(s).unwrap()));
        let dbl = |s: &str, d: f64| typed(s, XmlValue::double(d));
        let st = |s: &str| typed(s, XmlValue::string(s));
        let untyped = |s: &str| typed(s, XmlValue::untyped(s));
        let list = |s: &str, items: &[i64]| {
            typed(
                s,
                XmlValue::new(
                    XmlTypeCode::Int,
                    XmlValueKind::List {
                        item_type: XmlTypeCode::Int,
                        items: items
                            .iter()
                            .map(|i| {
                                XmlValue::new(
                                    XmlTypeCode::Int,
                                    XmlValueKind::Atomic(XmlAtomicValue::Integer(BigInt::from(*i))),
                                )
                            })
                            .collect(),
                    },
                ),
            )
        };
        let values: Vec<Option<KeyFieldValue>> = vec![
            Some(int("1")),
            Some(dec("1.0")),
            Some(dec("1.50")),
            Some(dec("1.5")),
            Some(int("+1")),
            Some(dbl("0", 0.0)),
            Some(dbl("-0", -0.0)),
            Some(dbl("NaN", f64::NAN)),
            Some(dbl("NaN", f64::NAN)),
            Some(st("a")),
            Some(st("1")),
            Some(make_string_field("1")),
            Some(make_string_field("a")),
            Some(untyped("a")),
            Some(untyped("1.0")),
            Some(list("7", &[7])),
            Some(int("7")),
            Some(list("7 8", &[7, 8])),
            Some(list(" 7  8 ", &[7, 8])),
            None,
            None,
        ];
        // Deterministic pseudo-random orders and pairings.
        let mut state: u64 = 0x2545F4914F6CDD1D;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for kind in [IdentityKind::Key, IdentityKind::Unique] {
            for round in 0..60 {
                let mut linear = KeyTable::new(IdentityConstraintKey::default(), name, kind);
                let mut indexed =
                    IndexedKeyTable::new(IdentityConstraintKey::default(), name, kind);
                let fields = 1 + (round % 3);
                for step in 0..40 {
                    let seq = KeySequence {
                        fields: (0..fields)
                            .map(|_| values[(next() % values.len() as u64) as usize].clone())
                            .collect(),
                    };
                    let a: Vec<String> = linear
                        .add_sequence(seq.clone(), &nt, "/p", None)
                        .iter()
                        .map(|e| e.constraint.to_string())
                        .collect();
                    let b: Vec<String> = indexed
                        .add_sequence(seq.clone(), &nt, "/p", None)
                        .iter()
                        .map(|e| e.constraint.to_string())
                        .collect();
                    assert_eq!(a, b, "{kind:?} round {round} step {step}: {seq:?}");
                    // A keyref probe of the same shape finds the same.
                    let probe = KeySequence {
                        fields: (0..fields)
                            .map(|_| values[(next() % values.len() as u64) as usize].clone())
                            .collect(),
                    };
                    let mut keyref =
                        KeyTable::new(IdentityConstraintKey::default(), name, IdentityKind::Keyref);
                    keyref.sequences.push(probe.clone());
                    assert_eq!(
                        keyref.check_keyref_against(&linear, &nt).len(),
                        indexed.check_keyref(&keyref, &nt).len(),
                        "{kind:?} round {round} step {step}: probe {probe:?}"
                    );
                }
                // Appending one table to another keeps them in agreement.
                let mut merged = IndexedKeyTable::new(IdentityConstraintKey::default(), name, kind);
                merged.append(indexed);
                let probe = KeySequence {
                    fields: vec![values[0].clone(); fields],
                };
                let mut keyref =
                    KeyTable::new(IdentityConstraintKey::default(), name, IdentityKind::Keyref);
                keyref.sequences.push(probe);
                assert_eq!(
                    keyref.check_keyref_against(&linear, &nt).len(),
                    merged.check_keyref(&keyref, &nt).len()
                );
                assert_eq!(merged.table().sequences.len(), linear.sequences.len());
            }
        }
    }
}
