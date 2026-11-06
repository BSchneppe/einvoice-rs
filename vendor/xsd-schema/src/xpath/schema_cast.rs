//! `cast as`, `castable as` and constructor functions for the atomic types of
//! the schema set attached to the static context.
//!
//! XPath 2.0 §3.10.2: "The target type must be an atomic type that is in the
//! in-scope schema types \[err:XPST0051\]." The in-scope schema types are the
//! built-in types plus, when a schema set is attached to the
//! [`XPathContext`], its named atomic simple types; §3.10.4 gives each of
//! those (but `xs:NOTATION` and `xs:anyAtomicType`) a constructor function,
//! `T($arg)` being `($arg) cast as T?`.
//!
//! The normative rules for a target type derived from a primitive are F&O
//! §17.2 – §17.5, implemented by [`cast_value`]:
//!
//! * **Identity and up the hierarchy** (§17.2 case 4, §17.3): a value whose
//!   type derives from the target type "will have the same value as the
//!   original, but will have a new type annotation" — no facet is checked.
//! * **Within one primitive and across primitives** (§17.4, §17.5): the value
//!   is cast to the target's nearest built-in type with the built-in casts —
//!   the casting table (`XPTY0004`, including `xs:QName` ↔ `xs:NOTATION`),
//!   the lexical mappings (`FORG0001`, `FOCA0002` for `NaN` / `INF` to an
//!   integer or decimal type, …) and, for `xs:QName` and `xs:NOTATION`, the
//!   static context's namespaces — and then the target type's facets are
//!   checked with the schema validator
//!   ([`validate_simple_type`](crate::validation::validate_simple_type)); a failure
//!   is `FORG0001` naming the target. The facets see the source's string for
//!   an `xs:string` (or derived) or `xs:untypedAtomic` source — §17.5 checks
//!   "its value against the pattern facet of TT", and XPath §3.10.2 first
//!   applies "the whitespace normalization rules for the target type" — and
//!   otherwise the value's `xs:string` cast: XPath §3.10.2 checks the pattern
//!   "by generating a string representation of the input value, using the
//!   rules for casting to xs:string". (F&O §17.4 names "the canonical lexical
//!   representation of the value" instead; the two differ for `xs:float` and
//!   `xs:double` between 10^-6 and 10^6 — `100` against `1.0E2`.) When the
//!   conversion of a string changed the value — an `xs:decimal` below 10^-28
//!   underflows to 0 — the value facets and the assertions are checked again
//!   on the result (`converted_decimal_conforms`). The facets are those the
//!   validator applies to an instance, gaps included.
//!
//! The result keeps the nearest built-in type as its
//! [`type_code`](XmlValue::type_code) and the target as its
//! [`schema_type`](XmlValue::schema_type), so arithmetic, comparisons and the
//! functions treat it as a value of the built-in type, while `instance of`
//! and `treat as` see the user type (`derives-from`, §2.5.4). A cast to a
//! built-in type gives a value without `schema_type`: the new annotation is
//! the built-in type.

use std::cell::RefCell;

use crate::ids::{SimpleTypeKey, TypeKey};
use crate::namespace::qname::QualifiedName;
use crate::namespace::table::well_known;
use crate::schema::SchemaSet;
use crate::types::value::{QNameValue, XmlAtomicValue, XmlValue, XmlValueKind};
use crate::types::XmlTypeCode;
use crate::validation::simple::{validate_simple_type_in, PrefixResolver};
use crate::xpath::cast::{cast_to, resolved_type_to_type_code};
use crate::xpath::context::XPathContext;
use crate::xpath::error::XPathError;
use crate::xpath::operators::cast_to_qname_with_context;

/// The target type of a `cast as`, `castable as` or constructor function,
/// resolved against the static context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CastTarget {
    /// A built-in atomic type of the XML Schema namespace.
    BuiltIn(XmlTypeCode),
    /// An atomic simple type of the schema set in the static context, with
    /// the code of its nearest built-in ancestor (`builtin`), the type code
    /// its values carry.
    Schema {
        key: SimpleTypeKey,
        builtin: XmlTypeCode,
    },
}

/// Resolve the expanded name of a cast target (the binder's
/// `resolved_atomic_type`) to a [`CastTarget`].
///
/// A name in the XML Schema namespace is a built-in type; any other name must
/// be an atomic simple type of the schema set in `ctx`, which must not keep
/// them out ([`XPathContext::without_schema_casts`], an assertion's context).
/// Anything else is `XPST0051`, as before for every name outside the `xs`
/// namespace.
pub(crate) fn resolve_cast_target(
    qname: &QualifiedName,
    ctx: &XPathContext<'_>,
) -> Result<CastTarget, XPathError> {
    if qname.namespace_uri == Some(well_known::XS_NAMESPACE) {
        return resolved_type_to_type_code(qname, ctx.names).map(CastTarget::BuiltIn);
    }
    let unknown = || XPathError::XPST0051 {
        type_name: ctx.names.resolve(qname.local_name).to_string(),
    };
    let schema_set = ctx
        .schema_set
        .filter(|_| ctx.schema_casts())
        .ok_or_else(unknown)?;
    schema_atomic_type(schema_set, qname.namespace_uri, qname.local_name).ok_or_else(unknown)
}

/// The atomic simple type of `schema_set` with this expanded name, as a
/// [`CastTarget::Schema`] — `None` for a name the set does not define, and
/// for a list, union or complex type.
pub(crate) fn schema_atomic_type(
    schema_set: &SchemaSet,
    namespace: Option<crate::ids::NameId>,
    local_name: crate::ids::NameId,
) -> Option<CastTarget> {
    match schema_set.lookup_type(namespace, local_name) {
        Some(key @ TypeKey::Simple(simple))
            if crate::xpath::node_test::is_atomic_simple_type(schema_set, key) =>
        {
            Some(CastTarget::Schema {
                key: simple,
                builtin: nearest_built_in_code(schema_set, simple)?,
            })
        }
        _ => None,
    }
}

/// The code of the nearest built-in ancestor of an atomic simple type (the
/// type itself when it is built in); `None` when the chain reaches no
/// concrete built-in atomic type.
fn nearest_built_in_code(schema_set: &SchemaSet, key: SimpleTypeKey) -> Option<XmlTypeCode> {
    let mut current = key;
    // A compiled schema set has finite, acyclic derivation chains; the bound
    // only protects against a malformed one.
    for _ in 0..256 {
        if let Some(code) = schema_set.get_type_code(current) {
            return match code {
                XmlTypeCode::AnySimpleType | XmlTypeCode::AnyAtomicType => None,
                code => Some(code),
            };
        }
        match schema_set
            .arenas
            .simple_types
            .get(current)?
            .resolved_base_type
        {
            Some(TypeKey::Simple(base)) => current = base,
            _ => return None,
        }
    }
    None
}

/// Cast one atomic value to `target` — the semantics of `cast as` once
/// atomization and the empty sequence are dealt with (see the module
/// documentation). `type_name` names the target in a `FORG0001` message.
pub(crate) fn cast_value(
    ctx: &XPathContext<'_>,
    value: &XmlValue,
    target: CastTarget,
    type_name: &dyn Fn() -> String,
) -> Result<XmlValue, XPathError> {
    match target {
        CastTarget::BuiltIn(code) => {
            let mut result = cast_to_built_in(ctx, value, code)?;
            // §17.3: the result is annotated with the built-in type, not with
            // the user type a value of that type code may have carried.
            result.schema_type = None;
            Ok(result)
        }
        CastTarget::Schema { key, builtin } => {
            cast_to_schema_type(ctx, value, key, builtin, type_name)
        }
    }
}

/// The built-in casts, `xs:QName` and `xs:NOTATION` through the static
/// context's namespaces (the only difference the two make between a built-in
/// target and a user type derived from them).
///
/// F&O §17.1, the casting table: `xs:QName` → `xs:NOTATION` and
/// `xs:NOTATION` → `xs:QName` are not supported — "a type error is raised
/// \[err:XPTY0004\]" — and neither is a cast between types derived from them
/// (§17.5 only assumes "that casting to xs:NOTATION succeeds"). A value's
/// type code is its nearest built-in type, so the test covers both.
fn cast_to_built_in(
    ctx: &XPathContext<'_>,
    value: &XmlValue,
    code: XmlTypeCode,
) -> Result<XmlValue, XPathError> {
    match (value.type_code, code) {
        (XmlTypeCode::QName, XmlTypeCode::Notation)
        | (XmlTypeCode::Notation, XmlTypeCode::QName) => Err(XPathError::type_mismatch(
            format!("{:?}", value.type_code),
            format!("{code:?}"),
        )),
        (_, XmlTypeCode::QName | XmlTypeCode::Notation) => {
            cast_to_qname_with_context(ctx, value, code)
        }
        _ => cast_to(value, code),
    }
}

fn cast_to_schema_type(
    ctx: &XPathContext<'_>,
    value: &XmlValue,
    key: SimpleTypeKey,
    builtin: XmlTypeCode,
    type_name: &dyn Fn() -> String,
) -> Result<XmlValue, XPathError> {
    let Some(schema_set) = ctx.schema_set else {
        return Err(XPathError::XPST0051 {
            type_name: type_name(),
        });
    };
    let target = TypeKey::Simple(key);

    // §17.2 case 4 and §17.3: identity, or up the hierarchy — always
    // possible, the same value with a new annotation.
    if let Some(source) = value.schema_type {
        if schema_set.xpath_derives_from(TypeKey::Simple(source), target) {
            let mut result = cast_to_built_in(ctx, value, builtin)?;
            result.schema_type = Some(key);
            return Ok(result);
        }
    }

    // §17.4 / §17.5: through the built-in types, then the target's facets.
    // A value the nearest built-in type already rejects is not a value of the
    // target either: the error names the target.
    let mut result = cast_to_built_in(ctx, value, builtin).map_err(|error| match error {
        XPathError::FORG0001 { value, .. } => XPathError::invalid_cast_value(value, type_name()),
        error => error,
    })?;
    let lexical = if is_string_source(value.type_code) {
        value.to_string_value()
    } else {
        result.to_string_value()
    };
    // An `xs:QName` / `xs:NOTATION` result is checked in its value space:
    // its prefix stands for the namespace the cast resolved it to (the
    // static context's, or the source value's own).
    let name = match &result.value {
        XmlValueKind::Atomic(XmlAtomicValue::QName(name) | XmlAtomicValue::Notation(name)) => {
            Some(name.clone())
        }
        _ => None,
    };
    let normalized = check_facets(&lexical, key, schema_set, name.as_ref()).map_err(|reason| {
        XPathError::invalid_cast_value(
            &lexical,
            match reason {
                FacetFailure::Invalid => type_name(),
                FacetFailure::Circular => format!(
                    "{} (its facets cast to the type itself while being checked)",
                    type_name()
                ),
            },
        )
    })?;

    // F&O §17.4: the value the cast returns must conform to the target's
    // facets ("If the value does not conform to the facets defined for the
    // target type, then an error is raised [err:FORG0001]"). For a string
    // source they were checked on the input string, and the built-in
    // conversion can return another value: see `converted_decimal_conforms`.
    if is_string_source(value.type_code) && builtin == XmlTypeCode::Decimal {
        if let Some(normalized) = normalized.as_deref() {
            if !converted_decimal_conforms(&result, normalized, key, schema_set) {
                return Err(XPathError::invalid_cast_value(&lexical, type_name()));
            }
        }
    }

    // §3.10.2: the target type's whiteSpace facet, which only a type of the
    // string family can set to something its built-in type does not.
    if builtin.is_string_derived() {
        if let Some(normalized) = normalized {
            if normalized != result.to_string_value() {
                result = cast_to(&XmlValue::string(normalized), builtin)?;
            }
        }
    }
    result.schema_type = Some(key);
    Ok(result)
}

/// Whether the `xs:decimal` a string cast to (`result`) satisfies the value
/// facets of `key` when it is not the exact value of the string's
/// whitespace-normalized form `normalized`, which has passed every facet.
///
/// The cast to `xs:decimal` changes a value only on underflow: a non-zero
/// magnitude below 10^-28 becomes 0 (F&O §6.2, "On underflow, 0.0 must be
/// returned"; `doc/IMPLEMENTATION_LIMITS.md`) — a larger or more precise one
/// is an error instead (FOAR0002, FOCA0006). So `'0.000000000000000000000000000001'`
/// passes `minExclusive 0` and casts to 0, which does not. Then the digits,
/// the bounds and the enumeration are checked again, by value, on the result,
/// and the XSD 1.1 assertions are evaluated on it (Datatypes 1.1 §4.3.13.3).
/// The patterns are not: they constrain the input string, which matched them
/// (XPath 2.0 §3.10.2 checks a string source in the lexical space of the
/// target type). The other built-in conversions a string goes through return
/// the value the validator checked: integers are exact, `xs:float` /
/// `xs:double` use the same parse (overflow to `INF` in both), the other types
/// are parsed by the validators themselves.
fn converted_decimal_conforms(
    result: &XmlValue,
    normalized: &str,
    key: SimpleTypeKey,
    schema_set: &SchemaSet,
) -> bool {
    use crate::types::exact_decimal::ExactDecimal;
    let (Some(converted), Some(exact)) = (
        ExactDecimal::from_value(result),
        ExactDecimal::parse(normalized),
    ) else {
        return true;
    };
    if converted == exact {
        return true;
    }
    let mut value_facets = (*schema_set.effective_facets(key)).clone();
    value_facets.patterns.clear();
    if value_facets
        .validate_decimal_family(result, &result.to_string_value())
        .is_err()
    {
        return false;
    }
    // The assertions see the result as the validator gives an instance's
    // value to them: annotated with the type.
    let mut typed = result.clone();
    typed.schema_type = Some(key);
    crate::validation::simple::evaluate_assertion_facets(&typed, &value_facets, schema_set).is_ok()
}

/// F&O §17.5: "If SV is an instance of xs:string or xs:untypedAtomic".
fn is_string_source(code: XmlTypeCode) -> bool {
    code == XmlTypeCode::UntypedAtomic || code.is_string_derived()
}

enum FacetFailure {
    /// The lexical form is not valid for the target type.
    Invalid,
    /// The target type's own facets (an XSD 1.1 assertion) cast to it again.
    Circular,
}

thread_local! {
    /// The target types whose facets are being checked on this thread, with
    /// their schema set's address. An XSD 1.1 assertion facet is an XPath
    /// expression of its own, and one that casts a string to the type it
    /// constrains (`string($value) castable as my:t` on `my:t`) would
    /// otherwise recurse without end. (`$value` itself already carries the
    /// type, and the identity cast checks no facet.) Assertions are compiled
    /// without the schema set's casts ([`XPathContext::without_schema_casts`]),
    /// so no facet check reaches a user-type cast today; the guard stays for
    /// any context that does.
    static CHECKING: RefCell<Vec<(usize, SimpleTypeKey)>> = const { RefCell::new(Vec::new()) };
}

/// Check `lexical` against the facets of `key` with the schema validator,
/// returning the whitespace-normalized form. With `name`, the QName value the
/// cast made of `lexical`, the facets see that value: `enumeration` by
/// expanded name, `pattern` on the lexical form.
fn check_facets(
    lexical: &str,
    key: SimpleTypeKey,
    schema_set: &SchemaSet,
    name: Option<&QNameValue>,
) -> Result<Option<String>, FacetFailure> {
    let entry = (std::ptr::from_ref(schema_set) as usize, key);
    if CHECKING.with(|stack| stack.borrow().contains(&entry)) {
        return Err(FacetFailure::Circular);
    }
    CHECKING.with(|stack| stack.borrow_mut().push(entry));
    struct Pop;
    impl Drop for Pop {
        fn drop(&mut self) {
            CHECKING.with(|stack| {
                stack.borrow_mut().pop();
            });
        }
    }
    let _pop = Pop;
    let resolver = |prefix: Option<&str>| {
        let name = name?;
        (prefix.filter(|p| !p.is_empty()) == name.prefix()).then_some(name.name().namespace_uri)
    };
    let resolver: Option<PrefixResolver<'_>> = name.map(|_| &resolver as PrefixResolver<'_>);
    validate_simple_type_in(lexical, TypeKey::Simple(key), schema_set, resolver)
        .map(|result| result.normalized_value)
        .map_err(|_| FacetFailure::Invalid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SchemaSetBuilder, XsdVersion};

    /// A facet check of a type that is already being checked on this thread
    /// fails as circular instead of starting again; once the outer check is
    /// done, the type checks normally.
    #[test]
    fn a_re_entered_facet_check_is_circular() {
        let set = SchemaSetBuilder::with_version(XsdVersion::V1_1)
            .add_source(
                r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:g">
                     <xs:simpleType name="t">
                       <xs:restriction base="xs:integer"><xs:maxInclusive value="9"/></xs:restriction>
                     </xs:simpleType>
                   </xs:schema>"#,
                "file:///guard.xsd",
            )
            .unwrap()
            .compile()
            .unwrap()
            .into_schema_set();
        let ns = set.name_table.add("urn:g");
        let local = set.name_table.add("t");
        let Some(CastTarget::Schema { key, .. }) = schema_atomic_type(&set, Some(ns), local) else {
            panic!("an atomic type");
        };
        let entry = (std::ptr::from_ref(&set) as usize, key);

        CHECKING.with(|stack| stack.borrow_mut().push(entry));
        assert!(matches!(
            check_facets("4", key, &set, None),
            Err(FacetFailure::Circular)
        ));
        CHECKING.with(|stack| stack.borrow_mut().pop());

        assert!(matches!(check_facets("4", key, &set, None), Ok(Some(_))));
        assert!(matches!(
            check_facets("40", key, &set, None),
            Err(FacetFailure::Invalid)
        ));
        assert!(CHECKING.with(|stack| stack.borrow().is_empty()));
    }
}
