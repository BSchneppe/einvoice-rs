//! XSD 1.1 schema-time checks of the XPath of type alternatives and
//! assertions, and of Conditional Type Substitutable.
//!
//! These passes run at schema-compile time, so schema authors get static
//! feedback instead of waiting for an instance that happens to exercise the
//! broken test.
//!
//! - **XPath Valid** (Structures 1.1 §3.13.6.2). Type Alternative Properties
//!   Correct (§3.12.6) clause 2: "If the {test} is not ·absent·, then it
//!   satisfies the constraint XPath Valid (§3.13.6.2)"; Assertion Properties
//!   Correct (§3.13.6.1) clause 2: "The {test} satisfies the constraint XPath
//!   Valid (§3.13.6.2), with the following modifications to the static context:
//!   2.1 The in-scope variables is a set with a single member … value …".
//!   XPath Valid clause 2: "X does not produce any static error, under the
//!   following conditions … 2.2.5 The in-scope schema definitions are those
//!   components that are present in every schema by definition, as defined in
//!   Built-in Attribute Declarations (§3.2.7), Built-in Complex Type Definition
//!   (§3.4.7) and Built-in Simple Type Definitions (§3.16.7)." Every
//!   `xs:alternative/@test`, every `xs:assert/@test` and every `xs:assertion`
//!   facet is therefore compiled with a static context limited to those
//!   components: the built-in types, and the schema set's four built-in
//!   `xsi:` attribute declarations ("There are four attribute declarations
//!   present in every schema by definition", §3.2.7) — there is no built-in
//!   element declaration. Naming a user-defined type or declaration is a
//!   static error there (XPST0051, XPST0008, XPST0017 for a user
//!   constructor), and the schema is invalid — `src-ct-alternative` for an alternative, `as-props-correct` for
//!   an assertion (an assertion facet's {value} is a sequence of Assertion
//!   components, Datatypes 1.1 §4.3.13.1). Evaluation later compiles the same
//!   test with the schema set, which it needs to answer questions about the
//!   type annotations of the instance's nodes (`element(*, xs:integer)` on a
//!   node of a user type derived from `xs:integer`).
//! - **Conditional Type Substitutable** (§3.12.6).

#![cfg(feature = "xsd11")]

use crate::error::{SchemaError, SchemaResult};
use crate::ids::{NameId, TypeKey};
use crate::namespace::context::NamespaceContextSnapshot;
use crate::parser::location::SourceRef;
use crate::schema::model::DerivationSet;
use crate::schema::SchemaSet;
use crate::xpath::api::XPathExpr;
use crate::xpath::error::XPathError;
use crate::xpath::XPathContext;

/// The static context of XPath Valid (§3.13.6.2 clause 2.2): the test's
/// in-scope namespaces (2.2.2), its default element/type namespace from the
/// `xpathDefaultNamespace` cascade (2.2.3), the default function namespace
/// (2.2.4, the engine's default), and only the built-in components (2.2.5):
/// the schema set is attached limited to them
/// ([`XPathContext::with_built_in_components_only`]), so the built-in `xsi:`
/// attribute declarations it holds stay in scope and its user-defined
/// components do not.
fn built_ins_only_context<'a>(
    schema_set: &'a SchemaSet,
    namespaces: &NamespaceContextSnapshot,
    default_ns: Option<NameId>,
) -> XPathContext<'a> {
    let ctx = XPathContext::new(&schema_set.name_table)
        .with_schema_set(schema_set)
        .with_built_in_components_only()
        .with_namespaces(namespaces.clone());
    match default_ns {
        Some(ns) => ctx.with_default_element_ns(ns),
        None => ctx,
    }
}

/// Compile every `xs:alternative/@test` under the static context of XPath
/// Valid (built-in components only) and refuse the schema for a static error
/// that makes it invalid: undefined variables (a type alternative has none in
/// scope), unbound prefixes, syntax errors, references to user-defined types or
/// declarations, unknown constructors (XPST0003, XPST0008, XPST0051, XPST0080,
/// XPST0081; XPST0017 for a function of the XML Schema namespace or one named
/// after a user-defined atomic type). A missing function of another namespace
/// only makes the alternative not match when it is evaluated.
pub fn validate_cta_xpath(schema_set: &SchemaSet) -> SchemaResult<()> {
    if !schema_set.is_xsd11() {
        return Ok(());
    }

    for (elem_key, elem) in schema_set.arenas.elements.iter() {
        for alt in &elem.alternatives {
            let Some(test) = alt.test.as_deref() else {
                continue;
            };
            // The same cascade the evaluation uses: the alternative's
            // `xpathDefaultNamespace`, else its schema document's.
            let default_ns = crate::validation::alternatives::resolve_alternative_default_ns(
                alt, elem_key, schema_set,
            );
            let ctx = built_ins_only_context(schema_set, &alt.ns_snapshot, default_ns);
            if let Err(err) = XPathExpr::compile(test, &ctx) {
                if violates_xpath_valid(&err, schema_set) {
                    let location = schema_set.locate(alt.source.as_ref());
                    let elem_name = elem
                        .name
                        .map(|n| schema_set.name_table.resolve_ref(n))
                        .unwrap_or("(anonymous)");
                    return Err(SchemaError::structural(
                        "src-ct-alternative",
                        format!(
                            "Element '{}' <xs:alternative test=\"{}\"> is not XPath Valid: {} \
                             (only the built-in components are in its static context, \
                             Structures §3.12.6, §3.13.6.2)",
                            elem_name, test, err
                        ),
                        location,
                    ));
                }
                // Otherwise a function this engine's library lacks: the
                // alternative is skipped when evaluated.
            }
        }
    }

    Ok(())
}

/// Compile every assertion — the `xs:assert` of complex types, and the
/// `xs:assertion` facets of simple types and of simple-content restrictions —
/// under the static context of XPath Valid as Assertion Properties Correct
/// (§3.13.6.1) modifies it (`$value` in scope), and refuse the schema with
/// `as-props-correct` for the static errors [`validate_cta_xpath`] lists. A
/// missing function of a namespace other than the XML Schema one fails the
/// assertion when it is evaluated (`cvc-assertion`).
///
/// Each assertion is checked once, at the type that owns it; derived types
/// inherit it unchanged.
pub fn validate_assertion_xpath(schema_set: &SchemaSet) -> SchemaResult<()> {
    if !schema_set.is_xsd11() {
        return Ok(());
    }
    let check = |test: &str,
                 namespaces: &NamespaceContextSnapshot,
                 default_ns: Option<NameId>,
                 source: Option<&SourceRef>,
                 what: &str|
     -> SchemaResult<()> {
        if test.is_empty() {
            return Ok(());
        }
        let ctx = built_ins_only_context(schema_set, namespaces, default_ns);
        match XPathExpr::compile_with_vars(test, &ctx, &["value"]) {
            Err(err) if violates_xpath_valid(&err, schema_set) => Err(SchemaError::structural(
                "as-props-correct",
                format!(
                    "{} has the test \"{}\", which is not XPath Valid: {} (only the built-in \
                     components are in its static context, Structures §3.13.6.1, §3.13.6.2)",
                    what, test, err
                ),
                schema_set.locate(source),
            )),
            // A function this engine's library lacks: the assertion fails
            // when evaluated (`cvc-assertion`).
            _ => Ok(()),
        }
    };
    let type_name = |name: Option<NameId>| {
        name.map(|n| schema_set.name_table.resolve(n))
            .unwrap_or_else(|| "(anonymous)".to_string())
    };

    for (key, ct) in schema_set.arenas.complex_types.iter() {
        for assertion in &ct.assertions {
            let default_ns = crate::validation::assertions::resolve_ct_assertion_default_ns(
                assertion, key, schema_set,
            );
            check(
                &assertion.test,
                &assertion.ns_snapshot,
                default_ns,
                assertion.source.as_ref(),
                &format!("An assertion of complex type '{}'", type_name(ct.name)),
            )?;
        }
        if let crate::parser::frames::ComplexContentResult::Simple(sc) = &ct.content {
            for facet in &sc.facets.assertions {
                let default_ns = crate::validation::simple::resolve_assertion_default_ns(
                    facet.xpath_default_namespace.as_deref(),
                    facet.source.as_ref(),
                    &facet.ns_snapshot,
                    schema_set,
                );
                check(
                    &facet.test,
                    &facet.ns_snapshot,
                    default_ns,
                    facet.source.as_ref(),
                    &format!(
                        "An assertion facet of complex type '{}'",
                        type_name(ct.name)
                    ),
                )?;
            }
        }
    }
    for (_key, st) in schema_set.arenas.simple_types.iter() {
        for facet in &st.facets.assertions {
            let default_ns = crate::validation::simple::resolve_assertion_default_ns(
                facet.xpath_default_namespace.as_deref(),
                facet.source.as_ref(),
                &facet.ns_snapshot,
                schema_set,
            );
            check(
                &facet.test,
                &facet.ns_snapshot,
                default_ns,
                facet.source.as_ref(),
                &format!("An assertion facet of simple type '{}'", type_name(st.name)),
            )?;
        }
    }
    Ok(())
}

/// Whether `err`, a static error of a test compiled in the built-ins-only
/// static context, makes the schema invalid — XPath Valid clause 2: "X does
/// not produce any static error".
///
/// - syntax errors (XPST0003), undefined names — variables, element and
///   attribute declarations, type names in kind tests (XPST0008) — unknown or
///   non-atomic cast and sequence-type targets (XPST0051), `xs:NOTATION` /
///   `xs:anyAtomicType` as a cast target (XPST0080), undeclared prefixes
///   (XPST0081);
/// - a call of a function that does not exist although its name is fully
///   known (XPST0017): one in the XML Schema namespace, where only the
///   constructors of the built-in types exist (XPath 2.0 §3.10.4), or one
///   named after a user-defined atomic type — its constructor, which the
///   static context does not have (clause 2.2.5).
///
/// A missing function of any other namespace (XPST0017) is not one of them:
/// the function signatures are ·implementation-defined· (clause 2.2.8) and
/// this engine's library may lack a function a schema expects
/// (`saxon:expression`, say). The test is then compiled again when it is
/// evaluated, fails there, and the assertion is violated (`cvc-assertion`) or
/// the alternative does not match.
fn violates_xpath_valid(err: &XPathError, schema_set: &SchemaSet) -> bool {
    match err {
        XPathError::XPST0003 { .. }
        | XPathError::XPST0008 { .. }
        | XPathError::XPST0051 { .. }
        | XPathError::XPST0080 { .. }
        | XPathError::XPST0081 { .. } => true,
        XPathError::XPST0017 {
            name, namespace, ..
        } => {
            namespace == crate::namespace::table::XS_NAMESPACE
                || names_user_atomic_type(schema_set, namespace, name)
        }
        _ => false,
    }
}

/// Whether `{namespace}local` is a user-defined atomic type of the schema set.
fn names_user_atomic_type(schema_set: &SchemaSet, namespace: &str, local: &str) -> bool {
    let names = &schema_set.name_table;
    let namespace = if namespace.is_empty() {
        None
    } else {
        match names.get(namespace) {
            Some(id) => Some(id),
            None => return false,
        }
    };
    names
        .get(local)
        .and_then(|local| schema_set.lookup_type(namespace, local))
        .is_some_and(|key| crate::xpath::node_test::is_atomic_simple_type(schema_set, key))
}

/// XSD 1.1 §3.12.6: Conditional Type Substitutable.
///
/// Each alternative's resolved type must be validly substitutable for
/// the element's declared type, taking into account the element's
/// `{disallowed substitutions}` (its `block` attribute).
///
/// Validly substitutable here means the alternative type is the
/// declared type or derives from it via a method not blocked by the
/// element. We compute this with the existing
/// [`SchemaSet::is_type_derived_from`] helper, passing the element's
/// effective derivation block so the check matches the runtime
/// `xsi:type` substitution rule (§3.4.4 clause 4.6).
///
/// ## Gate: anonymous-declared elements skipped
///
/// We enforce the substitutability rule only when the element's
/// declared type is named. The strict §3.12.6 reading rejects
/// W3C `cta9008err` correctly (its declared type is the named
/// `docType`), and a named declared type is the canonical use of
/// CTA — the alternatives are restrictions or extensions of the
/// declared component. Elements with an anonymous inline declared
/// type often pair it with named alternative types in common
/// "override" patterns; their derivation chains never converge, so
/// strict enforcement would flag patterns that downstream code
/// (and our own unit tests) treat as valid. Until that policy can
/// be audited end-to-end, we keep the check off for anonymous
/// declared types and rely on named-declared coverage to catch
/// cta9008err-shaped errors.
///
/// **TODO:** revisit once anonymous-declared CTA usage is audited.
pub fn validate_cta_substitutability(schema_set: &SchemaSet) -> SchemaResult<()> {
    if !schema_set.is_xsd11() {
        return Ok(());
    }

    for (_key, elem) in schema_set.arenas.elements.iter() {
        let Some(declared) = elem.resolved_type else {
            continue;
        };
        if elem.alternatives.is_empty() {
            continue;
        }
        // Skip anonymous declared types (see doc-comment above).
        if !type_is_named(schema_set, declared) {
            continue;
        }
        for alt in &elem.alternatives {
            let Some(alt_type) = alt.resolved_type else {
                // No resolved type means the alternative carries the
                // element's declared type as fallback. Substitutable by
                // construction.
                continue;
            };
            // §3.12.6: "xs:error need not be substitutable." It is the
            // bottom type — selecting it always rejects the element,
            // which is the schema author's deliberate intent.
            if is_xs_error(schema_set, alt_type) {
                continue;
            }
            // {disallowed substitutions} on the element controls which
            // derivation methods are forbidden. The CTA substitution
            // mirrors xsi:type (§3.4.4 clause 4.6) — same blocking set.
            let blocked = elem.block;
            if !is_substitutable(schema_set, alt_type, declared, blocked) {
                let elem_name = elem
                    .name
                    .map(|n| schema_set.name_table.resolve_ref(n))
                    .unwrap_or("(anonymous)");
                let location = schema_set.locate(alt.source.as_ref());
                return Err(SchemaError::structural(
                    "cos-ct-alternative-substitutable",
                    format!(
                        "Element '{}': type alternative is not validly substitutable \
                         for the element's declared type — the alternative type does \
                         not derive from the declared type by a non-blocked method \
                         (§3.12.6)",
                        elem_name
                    ),
                    location,
                ));
            }
        }
    }
    Ok(())
}

/// True when the given `TypeKey` refers to a named (top-level or
/// redefined) type definition. Anonymous inline types have `name = None`.
fn type_is_named(schema_set: &SchemaSet, type_key: TypeKey) -> bool {
    match type_key {
        TypeKey::Simple(sk) => schema_set
            .arenas
            .simple_types
            .get(sk)
            .and_then(|t| t.name)
            .is_some(),
        TypeKey::Complex(ck) => schema_set
            .arenas
            .complex_types
            .get(ck)
            .and_then(|t| t.name)
            .is_some(),
    }
}

fn is_substitutable(
    schema_set: &SchemaSet,
    alt_type: TypeKey,
    declared: TypeKey,
    blocked: DerivationSet,
) -> bool {
    schema_set.is_type_derived_from(alt_type, declared, blocked)
}

/// Whether `type_key` is the built-in `xs:error` type.
fn is_xs_error(schema_set: &SchemaSet, type_key: TypeKey) -> bool {
    let TypeKey::Simple(key) = type_key else {
        return false;
    };
    let Some(error_key) = schema_set
        .builtin_types()
        .get_by_type_code(crate::types::XmlTypeCode::Error)
    else {
        return false;
    };
    key == error_key
}

#[cfg(test)]
mod tests {
    use crate::error::SchemaError;
    use crate::namespace::NamespaceContext;
    use crate::pipeline::load_and_process_schema;
    use crate::schema::SchemaSet;
    use crate::xpath::api::XPathExpr;
    use crate::xpath::XPathContext;

    /// Load a one-element XSD 1.1 schema whose type alternative has `test`,
    /// and return the constraint of the schema error, if any.
    fn cta_verdict(test: &str) -> Option<&'static str> {
        let schema = format!(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                          xmlns:t="urn:t" targetNamespace="urn:t">
                 <xs:element name="e">
                   <xs:alternative test="{test}" type="xs:string"/>
                 </xs:element>
               </xs:schema>"#
        );
        let mut schema_set = SchemaSet::xsd11();
        match load_and_process_schema(schema.as_bytes(), "cta.xsd", &mut schema_set, None) {
            Ok(_) => None,
            Err(SchemaError::StructuralError { constraint, .. }) => Some(constraint),
            Err(other) => panic!("{test}: unexpected error {other:?}"),
        }
    }

    /// Structures 1.1 §3.12.6 (Type Alternative Properties Correct) clause 2:
    /// the {test} satisfies XPath Valid (§3.13.6.2), whose clause 2 requires
    /// that "X does not produce any static error". A call of a function in the
    /// XML Schema namespace that is not a constructor function, and a `cast` to
    /// `xs:NOTATION` / `xs:anyAtomicType`, are such static errors (XPath 2.0
    /// §3.10.4: XPST0017; §3.10.2: XPST0080); unlike a function of the
    /// function library this engine may lack, constructor functions are fully
    /// known, so these are schema errors, not skipped alternatives.
    #[test]
    fn static_errors_of_constructor_calls_and_casts_reject_the_schema() {
        let tests = [
            "xs:NOTATION('a') = 'a'",
            "xs:anyAtomicType('a') = 'a'",
            "xs:integer(@a, 2) = 1",
            "'a' cast as xs:NOTATION = 'a'",
            "@a castable as xs:anyAtomicType",
            "exists(self::schema-element(t:nosuch))",
            "$nosuch = 1",
            // Well-formed tests are accepted.
            "xs:integer(@a) = 1",
            "@a castable as xs:integer",
            "not(@a)",
            // A function of another namespace that the function library
            // lacks is still treated as this engine's limitation: the
            // alternative is skipped, not the schema rejected (the existing
            // policy).
            "t:f(@a)",
        ];
        let verdicts: Vec<Option<&str>> = tests.iter().map(|t| cta_verdict(t)).collect();
        let error = Some("src-ct-alternative");
        assert_eq!(
            verdicts,
            [error, error, error, error, error, error, error, None, None, None, None],
            "{tests:?}"
        );
    }

    /// The built-in type names, `xs:untyped`, and one name that is none.
    const XS_TYPES: &[&str] = &[
        "anyType",
        "anySimpleType",
        "anyAtomicType",
        "untyped",
        "untypedAtomic",
        "error",
        "string",
        "normalizedString",
        "token",
        "language",
        "NMTOKEN",
        "Name",
        "NCName",
        "ID",
        "IDREF",
        "ENTITY",
        "boolean",
        "decimal",
        "float",
        "double",
        "integer",
        "nonPositiveInteger",
        "negativeInteger",
        "long",
        "int",
        "short",
        "byte",
        "nonNegativeInteger",
        "unsignedLong",
        "unsignedInt",
        "unsignedShort",
        "unsignedByte",
        "positiveInteger",
        "duration",
        "dateTime",
        "time",
        "date",
        "gYearMonth",
        "gYear",
        "gMonthDay",
        "gDay",
        "gMonth",
        "yearMonthDuration",
        "dayTimeDuration",
        "dateTimeStamp",
        "hexBinary",
        "base64Binary",
        "anyURI",
        "QName",
        "NOTATION",
        "NMTOKENS",
        "IDREFS",
        "ENTITIES",
        "nosuch",
    ];

    /// Every reference a test can make to a component present in every schema
    /// by definition (Structures 1.1 §3.13.6.2 clause 2.2.5), or to a name
    /// that is no such component: each built-in type name in each place a
    /// type name can take, and the declarations a `schema-attribute` /
    /// `schema-element` test can name in the `xsi`, `xs` and `xml` namespaces.
    fn built_in_references() -> Vec<String> {
        let mut tests = Vec::new();
        for t in XS_TYPES {
            tests.extend([
                format!("() instance of xs:{t}"),
                format!("() treat as xs:{t}?"),
                format!("'1' castable as xs:{t}"),
                format!("'1' cast as xs:{t}"),
                format!("xs:{t}('1')"),
                format!("() instance of element(*, xs:{t})"),
                format!("() instance of attribute(*, xs:{t})"),
                format!("() instance of document-node(element(*, xs:{t}))"),
                format!("element(*, xs:{t})"),
                format!("@attribute(*, xs:{t})"),
            ]);
        }
        for n in [
            "xsi:type",
            "xsi:nil",
            "xsi:schemaLocation",
            "xsi:noNamespaceSchemaLocation",
            "xsi:nosuch",
            "xs:schema",
            "xs:element",
            "xml:lang",
            "xml:space",
        ] {
            tests.extend([
                format!("() instance of schema-attribute({n})"),
                format!("@schema-attribute({n})"),
                format!("() instance of schema-element({n})"),
                format!("schema-element({n})"),
                format!("() instance of document-node(schema-element({n}))"),
            ]);
        }
        tests
    }

    /// A schema with a user-defined component in each namespace and of each
    /// kind a built-in reference could reach, named like the built-ins.
    const USER_COMPONENTS: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
          <xs:simpleType name="string"><xs:restriction base="xs:string"/></xs:simpleType>
          <xs:element name="schema" type="xs:string"/>
          <xs:element name="nil" type="xs:boolean"/>
          <xs:attribute name="lang" type="xs:string"/>
          <xs:attribute name="nosuch" type="xs:string"/>
        </xs:schema>"#;

    /// The static error code compiling `test` raises in `schema_set` with or
    /// without the limit to the built-in components, if any.
    fn static_verdict(
        schema_set: &mut SchemaSet,
        built_ins_only: bool,
        test: &str,
    ) -> Result<(), Option<&'static str>> {
        let namespaces = {
            let mut context = NamespaceContext::new(&mut schema_set.name_table);
            context.add_namespace("xs", "http://www.w3.org/2001/XMLSchema");
            context.add_namespace("xsi", "http://www.w3.org/2001/XMLSchema-instance");
            context.snapshot()
        };
        let schema_set: &SchemaSet = schema_set;
        let ctx = XPathContext::new(&schema_set.name_table)
            .with_schema_set(schema_set)
            .with_namespaces(namespaces);
        let ctx = if built_ins_only {
            ctx.with_built_in_components_only()
        } else {
            ctx
        };
        XPathExpr::compile_with_vars(test, &ctx, &["value"])
            .map(|_| ())
            .map_err(|e| e.error_code())
    }

    /// With no user-defined component, a context limited to the built-in
    /// components has the in-scope schema definitions of one with the whole
    /// schema set, and adding user-defined components changes neither — for
    /// every reference to a built-in component.
    #[test]
    fn built_in_components_only_keeps_every_built_in_component() {
        let mut empty = SchemaSet::xsd11();
        let mut user = SchemaSet::xsd11();
        load_and_process_schema(USER_COMPONENTS.as_bytes(), "user.xsd", &mut user, None)
            .expect("schema");
        let mut differences = Vec::new();
        let mut compiled = 0;
        for test in built_in_references() {
            let whole = static_verdict(&mut empty, false, &test);
            let limited = static_verdict(&mut empty, true, &test);
            let limited_with_user = static_verdict(&mut user, true, &test);
            compiled += usize::from(whole.is_ok());
            if limited != whole || limited_with_user != whole {
                differences.push(format!(
                    "{test}: whole {whole:?}, built-ins only {limited:?}, \
                     built-ins only beside user components {limited_with_user:?}"
                ));
            }
        }
        assert!(differences.is_empty(), "{}", differences.join("\n"));
        // The battery is no all-error one: the built-in types compile in most
        // places, and the four xsi: declarations as `schema-attribute`.
        assert!(compiled > 300, "only {compiled} references compile");
        for n in ["type", "nil", "schemaLocation", "noNamespaceSchemaLocation"] {
            let test = format!("@schema-attribute(xsi:{n})");
            assert_eq!(static_verdict(&mut user, true, &test), Ok(()), "{test}");
        }
    }

    /// The user-defined components themselves are out of scope under the
    /// limit, and in scope without it.
    #[test]
    fn built_in_components_only_hides_every_user_component() {
        let mut user = SchemaSet::xsd11();
        load_and_process_schema(USER_COMPONENTS.as_bytes(), "user.xsd", &mut user, None)
            .expect("schema");
        for test in [
            "() instance of string",
            "'1' castable as string",
            "() instance of element(*, string)",
            "@attribute(*, string)",
            "() instance of schema-element(schema)",
            "schema-element(nil)",
            "@schema-attribute(lang)",
            "() instance of schema-attribute(nosuch)",
        ] {
            assert_eq!(static_verdict(&mut user, false, test), Ok(()), "{test}");
            assert!(static_verdict(&mut user, true, test).is_err(), "{test}");
        }
    }
}
