//! XSD 1.1 complex-type assertion evaluation.
//!
//! Complex types can carry `xs:assert` elements whose XPath 2.0 expressions
//! are evaluated against the element subtree. This module provides:
//!
//! - `AssertionBufferFrame` — per-element bookkeeping for assertion buffering
//! - `has_inherited_assertions` — cheap hot-path check for any assertions
//! - `collect_inherited_assertions` — full base-first collection with owner keys
//! - `resolve_ct_assertion_default_ns` — xpathDefaultNamespace cascade
//! - `evaluate_complex_type_assertions` — core XPath evaluation

use crate::document::buffer::BufferDocument;
use crate::document::navigator::BufferDocNavigator;
use crate::ids::{ComplexTypeKey, NameId, TypeKey};
use crate::navigator::{DomNavigator, TypedValue};
use crate::parser::frames::{AssertResult, ComplexContentResult};
use crate::parser::location::SourceLocation;
use crate::schema::SchemaSet;
use crate::validation::errors::{self, ValidationError};
use crate::validation::simple::validate_simple_type_in;
use crate::xpath::api::XPathExpr;
use crate::xpath::functions::{effective_boolean_value, XPathValue};
use crate::xpath::XPathContext;

use crate::arenas::SchemaArenas;

// ---------------------------------------------------------------------------
// AssertionBufferFrame
// ---------------------------------------------------------------------------

/// Per-element assertion buffer frame.
///
/// Created when a complex type with assertions is encountered during streaming
/// validation. Tracks the node reference in the fragment document and the
/// owning complex type, so assertions can be evaluated at element close.
pub(crate) struct AssertionBufferFrame {
    /// Node ref of this element in the fragment document.
    pub element_ref: u32,
    /// ComplexType key whose assertions triggered this frame.
    pub complex_type_key: ComplexTypeKey,
    /// This frame's element as a diagnostic subject (error reporting).
    /// Populated when the frame is popped at its own end-element (before deferral).
    pub context: crate::validation::runtime::ErrorContext,
    /// Source location at the time this frame's element closed.
    pub location: Option<SourceLocation>,
    /// Whether a failure of this (deferred, nested) frame makes the outermost
    /// asserted element invalid. Populated at deferral: `true` when every
    /// element from this frame's parent up to and including the outermost
    /// asserted element was strictly assessed, so `[validity]` = invalid
    /// travels up the chain (Structures §3.3.5.1 `[validity]` clause 1.1.2); a
    /// laxly assessed element in between is `notKnown` (clause 2) and stops it.
    pub propagates_to_outer: bool,
}

// ---------------------------------------------------------------------------
// has_inherited_assertions — cheap hot-path check
// ---------------------------------------------------------------------------

/// Returns `true` if the complex type (or any base in its derivation chain)
/// has non-empty `assertions`. No allocation. Used in
/// `validate_element_by_id` to decide whether to start assertion buffering.
pub(crate) fn has_inherited_assertions(ct_key: ComplexTypeKey, arenas: &SchemaArenas) -> bool {
    let ct = &arenas.complex_types[ct_key];
    if !ct.assertions.is_empty() {
        return true;
    }
    // Walk the derivation chain
    let mut current = ct.resolved_base_type;
    while let Some(TypeKey::Complex(base_key)) = current {
        let base = &arenas.complex_types[base_key];
        if !base.assertions.is_empty() {
            return true;
        }
        current = base.resolved_base_type;
    }
    false
}

// ---------------------------------------------------------------------------
// collect_inherited_assertions — full collection
// ---------------------------------------------------------------------------

/// Collects all assertions from the complex type and its base types,
/// ordered base-first. Each assertion is paired with its **defining** type's
/// key — essential for the xpathDefaultNamespace cascade, which must use the
/// type-level default from the type that declared the assertion.
pub(crate) fn collect_inherited_assertions(
    ct_key: ComplexTypeKey,
    arenas: &SchemaArenas,
) -> Vec<(&AssertResult, ComplexTypeKey)> {
    // Collect chain of complex type keys from derived to base
    let mut chain = vec![ct_key];
    let mut current = arenas.complex_types[ct_key].resolved_base_type;
    while let Some(TypeKey::Complex(base_key)) = current {
        chain.push(base_key);
        current = arenas.complex_types[base_key].resolved_base_type;
    }

    // Reverse for base-first order, then collect assertions
    let mut result = Vec::new();
    for &key in chain.iter().rev() {
        let ct = &arenas.complex_types[key];
        for assertion in &ct.assertions {
            result.push((assertion, key));
        }
    }
    result
}

// ---------------------------------------------------------------------------
// resolve_ct_assertion_default_ns — xpathDefaultNamespace cascade
// ---------------------------------------------------------------------------

/// Three-level cascade: **assertion-level > owner-type-level > schema-document-level**.
///
/// Takes the **owner** `ComplexTypeKey` (from `collect_inherited_assertions`),
/// not the derived type, so inherited assertions get the correct type-level default.
pub(crate) fn resolve_ct_assertion_default_ns(
    assertion: &AssertResult,
    owner_ct_key: ComplexTypeKey,
    schema_set: &SchemaSet,
) -> Option<NameId> {
    let ct = &schema_set.arenas.complex_types[owner_ct_key];

    // Look up the schema document that defines the owning type
    let doc = ct
        .source
        .as_ref()
        .and_then(|s| schema_set.documents.get(s.doc_id as usize));

    // Cascade: assertion-level > type-level > schema-document-level
    let effective = if let Some(raw) = &assertion.xpath_default_namespace {
        Some(raw.clone())
    } else if let Some(raw) = &ct.xpath_default_namespace {
        Some(raw.clone())
    } else {
        doc.and_then(|d| d.xpath_default_namespace)
            .map(|id| schema_set.name_table.resolve(id))
    };

    match effective.as_deref() {
        Some("##defaultNamespace") => assertion.ns_snapshot.default_ns,
        Some("##targetNamespace") => doc.and_then(|d| d.target_namespace),
        Some("##local") | None => None,
        Some(uri) => Some(schema_set.name_table.add(uri)),
    }
}

// ---------------------------------------------------------------------------
// compute_dollar_value — XSD 1.1 §3.13.4.1 clause 2.3 binding
// ---------------------------------------------------------------------------

/// Compute the value of `$value` for an assertion.
///
/// Per §3.13.4.1 clause 2.3, `$value` is bound from **E's governing
/// type definition** (the most-derived type for the element), not from
/// each inherited assertion's owner. So this is computed once per
/// element and reused across all assertions in the inheritance chain:
/// - Governing type's content variety **simple**, element not nilled,
///   simple-type validation succeeds → the typed value.
/// - Otherwise → empty sequence (clause 2.3.2).
///
/// The partial-PSVI `[validity]` is unavailable here, so any
/// simple-type-validation failure falls into the empty-sequence branch.
fn compute_dollar_value<'doc>(
    doc: &'doc BufferDocument<'doc>,
    element_ref: u32,
    governing_ct_key: ComplexTypeKey,
    schema_set: &SchemaSet,
) -> XPathValue<BufferDocNavigator<'doc>> {
    let ct = &schema_set.arenas.complex_types[governing_ct_key];
    if !matches!(ct.content, ComplexContentResult::Simple(_)) {
        return XPathValue::empty();
    }

    let nav = BufferDocNavigator::new(doc, element_ref);
    if matches!(nav.typed_value(), TypedValue::Nilled) {
        return XPathValue::empty();
    }

    // An `xs:QName` / `xs:NOTATION` value is resolved with the element's
    // in-scope namespaces, as the validation runtime resolves it.
    let resolver = |prefix: Option<&str>| nav.prefix_binding(prefix);
    match validate_simple_type_in(
        &nav.value(),
        TypeKey::Complex(governing_ct_key),
        schema_set,
        Some(&resolver),
    ) {
        // §3.13.4.1 clause 2.3.1: "the XDM representation of
        // E.[schema actual value]". When the governing simple-content type's
        // {variety} = list (also as the member a union chose), `$value` is a
        // sequence of atomic values, one per list item, each of the type that
        // validated it — for a list of a union, the member type that accepted
        // that item.
        Ok(result) => result.typed_value.to_xpath_value(),
        Err(_) => XPathValue::empty(),
    }
}

/// Hand the warnings an assertion's or a type alternative's expression `test`
/// raised to the validation run (`expression_warnings`), naming the
/// expression.
pub(crate) fn note_expression_warnings(
    what: &str,
    test: &str,
    warnings: Vec<crate::xpath::XPathWarning>,
) {
    for warning in warnings {
        super::expression_warnings::note(
            warning.code(),
            || format!("{what} '{test}'"),
            warning.message().to_string(),
        );
    }
}

// ---------------------------------------------------------------------------
// evaluate_complex_type_assertions — core evaluation
// ---------------------------------------------------------------------------

/// Evaluate all assertions (own + inherited) for a complex type against
/// the element subtree in a `BufferDocument`.
///
/// Returns a `Vec` of all `cvc-assertion` errors (does not stop at first failure).
pub(crate) fn evaluate_complex_type_assertions(
    doc: &BufferDocument<'_>,
    element_ref: u32,
    ct_key: ComplexTypeKey,
    schema_set: &SchemaSet,
) -> Vec<ValidationError> {
    let assertions = collect_inherited_assertions(ct_key, &schema_set.arenas);
    let mut errors = Vec::new();

    // §3.13.4.1 clause 2.3 ties `$value` to E's governing type
    // (the parameter `ct_key`), so it is identical across all
    // inherited assertions. Compute once and clone per evaluation.
    let dollar_value = compute_dollar_value(doc, element_ref, ct_key, schema_set);

    for (assertion, owner_key) in assertions {
        if assertion.test.is_empty() {
            continue;
        }

        // The schema-time check (`schema::validate_assertion_xpath`) compiled
        // this test with only the built-in components in scope (Structures
        // §3.13.6.2 clause 2.2.5) and refused the schema for any user-defined
        // type or declaration it named. Evaluation needs the schema set to
        // answer questions about the instance's type annotations (a node of
        // a user type derived from `xs:integer` is an
        // `element(*, xs:integer)`); `without_schema_casts` is kept as a
        // second guard. A test that still fails to compile here calls a
        // function this engine lacks: `cvc-assertion` below.
        let ctx = XPathContext::new(&schema_set.name_table)
            .with_namespaces(assertion.ns_snapshot.clone())
            .with_schema_set(schema_set)
            .without_schema_casts();

        // Apply xpathDefaultNamespace cascade
        let ctx = if let Some(default_ns) =
            resolve_ct_assertion_default_ns(assertion, owner_key, schema_set)
        {
            ctx.with_default_element_ns(default_ns)
        } else {
            ctx
        };

        // §3.13.4.1 clause 2.2: `$value` is in scope for every assertion.
        // Declared unconditionally so XPath that references it compiles.
        let expr = match XPathExpr::compile_with_vars(&assertion.test, &ctx, &["value"]) {
            Ok(e) => e,
            Err(e) => {
                errors.push(errors::error(
                    "cvc-assertion",
                    format!(
                        "Failed to compile assertion test '{}': {}",
                        assertion.test, e
                    ),
                    None,
                ));
                continue;
            }
        };

        let nav = BufferDocNavigator::new_assertion(doc, element_ref);
        let value_for_eval = dollar_value.clone();

        let warnings = std::cell::RefCell::new(Vec::new());
        let outcome = crate::xpath::operators::with_decimal_capacity_marks(|| {
            expr.evaluator(&ctx)
                .with_warnings(&warnings)
                .run_with_node_and_setup(Some(nav), |eval| {
                    eval.set_variable_by_name("value", value_for_eval)
                        .expect("$value declared via compile_with_vars");
                })
        });
        note_expression_warnings("assertion", &assertion.test, warnings.into_inner());
        let result = match outcome {
            Ok(r) => r,
            // Datatypes 1.1 §5.4: an `xs:decimal` beyond this implementation's
            // capacity "must not be treated as invalid". An assertion whose
            // error comes from computing with such a value — the error is
            // marked where that operand is converted, so a huge value
            // elsewhere in the subtree cannot excuse an unrelated error — has
            // an unknown outcome, not a false one; the value itself was
            // reported with the capacity warning when it was validated. Any
            // other error fails the assertion (Structures §3.13.4.1).
            Err(e) if e.is_decimal_capacity_operand() => {
                continue;
            }
            Err(e) => {
                errors.push(errors::error(
                    "cvc-assertion",
                    format!(
                        "Failed to evaluate assertion test '{}': {}",
                        assertion.test, e
                    ),
                    None,
                ));
                continue;
            }
        };

        // Check effective boolean value
        match effective_boolean_value(&result) {
            Ok(true) => { /* assertion passed */ }
            Ok(false) => {
                errors.push(errors::error(
                    "cvc-assertion",
                    format!("Assertion '{}' failed", assertion.test),
                    None,
                ));
            }
            Err(e) => {
                errors.push(errors::error(
                    "cvc-assertion",
                    format!(
                        "Failed to compute boolean value for assertion '{}': {}",
                        assertion.test, e
                    ),
                    None,
                ));
            }
        }
    }

    errors
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::load_and_process_schema;

    fn load_schema(xsd: &str) -> SchemaSet {
        let mut schema_set = SchemaSet::xsd11();
        load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None)
            .expect("failed to load schema");
        schema_set
    }

    /// Find the first complex type key in the schema set by name.
    fn find_ct_key(schema_set: &SchemaSet, name: &str) -> ComplexTypeKey {
        let name_id = schema_set.name_table.add(name);
        for (key, ct) in &schema_set.arenas.complex_types {
            if ct.name == Some(name_id) {
                return key;
            }
        }
        panic!("Complex type '{}' not found", name);
    }

    #[test]
    fn test_has_inherited_assertions_none() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:complexType name="plain">
                    <xs:sequence>
                        <xs:element name="x" type="xs:string"/>
                    </xs:sequence>
                </xs:complexType>
            </xs:schema>"#,
        );
        let key = find_ct_key(&schema_set, "plain");
        assert!(!has_inherited_assertions(key, &schema_set.arenas));
    }

    #[test]
    fn test_has_inherited_assertions_own() {
        // xs:assert as direct child of complexType with attribute-only content
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:complexType name="withAssert">
                    <xs:attribute name="val" type="xs:integer"/>
                    <xs:assert test="@val >= 0"/>
                </xs:complexType>
            </xs:schema>"#,
        );
        let key = find_ct_key(&schema_set, "withAssert");
        assert!(has_inherited_assertions(key, &schema_set.arenas));
    }

    #[test]
    fn test_has_inherited_assertions_from_base() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:complexType name="base">
                    <xs:attribute name="val" type="xs:integer"/>
                    <xs:assert test="@val >= 0"/>
                </xs:complexType>
                <xs:complexType name="derived">
                    <xs:complexContent>
                        <xs:restriction base="base">
                            <xs:attribute name="val" type="xs:integer"/>
                        </xs:restriction>
                    </xs:complexContent>
                </xs:complexType>
            </xs:schema>"#,
        );
        let key = find_ct_key(&schema_set, "derived");
        assert!(has_inherited_assertions(key, &schema_set.arenas));
    }

    #[test]
    fn test_collect_inherited_assertions_ordering() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:complexType name="base">
                    <xs:attribute name="val" type="xs:integer"/>
                    <xs:assert test="@val >= 0"/>
                </xs:complexType>
                <xs:complexType name="derived">
                    <xs:complexContent>
                        <xs:restriction base="base">
                            <xs:attribute name="val" type="xs:integer"/>
                            <xs:assert test="@val &lt; 100"/>
                        </xs:restriction>
                    </xs:complexContent>
                </xs:complexType>
            </xs:schema>"#,
        );
        let derived_key = find_ct_key(&schema_set, "derived");
        let base_key = find_ct_key(&schema_set, "base");
        let assertions = collect_inherited_assertions(derived_key, &schema_set.arenas);

        // Base-first ordering: base assertion comes first
        assert_eq!(assertions.len(), 2);
        assert_eq!(
            assertions[0].1, base_key,
            "first assertion should be from base"
        );
        assert_eq!(
            assertions[1].1, derived_key,
            "second assertion should be from derived"
        );
        assert!(assertions[0].0.test.contains(">= 0"));
        assert!(assertions[1].0.test.contains("< 100"));
    }

    #[test]
    fn test_collect_inherited_assertions_no_assertions() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:complexType name="plain">
                    <xs:sequence>
                        <xs:element name="x" type="xs:string"/>
                    </xs:sequence>
                </xs:complexType>
            </xs:schema>"#,
        );
        let key = find_ct_key(&schema_set, "plain");
        let assertions = collect_inherited_assertions(key, &schema_set.arenas);
        assert!(assertions.is_empty());
    }
}
