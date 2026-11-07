//! XSD 1.1 type alternative (conditional type assignment) evaluation.
//!
//! When an element declaration has `xs:alternative` children, the governing
//! type is selected at runtime based on XPath test expressions evaluated
//! against the element's attributes. This module provides:
//!
//! - `evaluate_type_alternatives` — core evaluation: builds a minimal
//!   document fragment, evaluates each alternative's test, and returns
//!   the first matching alternative's resolved type.

use bumpalo::Bump;

use crate::document::builder::BufferDocumentBuilder;
use crate::document::navigator::BufferDocNavigator;
use crate::document::{BufferDocumentOptions, DocumentKind};
use crate::ids::{ElementKey, NameId, TypeKey};
use crate::namespace::context::NamespaceContextSnapshot;
use crate::parser::frames::AlternativeResult;
use crate::schema::SchemaSet;
use crate::xpath::api::XPathExpr;
use crate::xpath::functions::effective_boolean_value;
use crate::xpath::XPathContext;

// ---------------------------------------------------------------------------
// resolve_alternative_default_ns — xpathDefaultNamespace cascade
// ---------------------------------------------------------------------------

/// Two-level cascade: **alternative-level > schema-document-level**.
pub(crate) fn resolve_alternative_default_ns(
    alt: &AlternativeResult,
    elem_key: ElementKey,
    schema_set: &SchemaSet,
) -> Option<NameId> {
    let elem = &schema_set.arenas.elements[elem_key];

    // Look up the schema document that defines the element
    let doc = elem
        .source
        .as_ref()
        .and_then(|s| schema_set.documents.get(s.doc_id as usize));

    // Cascade: alternative-level > schema-document-level
    let effective = if let Some(raw) = &alt.xpath_default_namespace {
        Some(raw.clone())
    } else {
        doc.and_then(|d| d.xpath_default_namespace)
            .map(|id| schema_set.name_table.resolve(id))
    };

    match effective.as_deref() {
        Some("##defaultNamespace") => alt.ns_snapshot.default_ns,
        Some("##targetNamespace") => doc.and_then(|d| d.target_namespace),
        Some("##local") | None => None,
        Some(uri) => Some(schema_set.name_table.add(uri)),
    }
}

// ---------------------------------------------------------------------------
// evaluate_type_alternatives — core evaluation
// ---------------------------------------------------------------------------

/// Evaluate type alternatives for an element declaration.
///
/// Builds a minimal `BufferDocument` fragment containing just the element
/// and its attributes, then evaluates each alternative's XPath test.
/// Returns the `TypeKey` of the first matching alternative, or `None`
/// if no alternative matches (caller should use the element's declared type).
///
/// `instance_ns` carries the runtime in-scope namespace bindings of the
/// element so functions like `resolve-QName(@kind, .)` and prefixed
/// QName literals see the same namespace nodes as the live instance
/// (§3.12.4 — the CTA XDM instance includes E's [namespaces]).
///
/// `instance_base_uri`, when set, is exposed to `fn:base-uri(.)` via the
/// fragment's document-level base URI; the schema document URI is
/// separately exposed to `fn:static-base-uri()` via the XPath static
/// context.
pub(crate) fn evaluate_type_alternatives(
    elem_key: ElementKey,
    elem_local_name: NameId,
    elem_namespace: Option<NameId>,
    collected_attributes: &[(Option<NameId>, NameId, String)],
    instance_ns: Option<&NamespaceContextSnapshot>,
    instance_base_uri: Option<&str>,
    schema_set: &SchemaSet,
) -> Option<TypeKey> {
    let alternatives = &schema_set.arenas.elements[elem_key].alternatives;
    if alternatives.is_empty() {
        return None;
    }

    // Build a minimal fragment document with just the element + attributes
    let arena = Bump::new();
    let opts = BufferDocumentOptions {
        kind: DocumentKind::Fragment,
        track_source_locations: false,
    };
    let mut builder = match BufferDocumentBuilder::new(&arena, &schema_set.name_table, None, opts) {
        Ok(b) => b,
        Err(_) => return None,
    };

    // Materialize ns_declarations from the runtime in-scope bindings so
    // the fragment element exposes the same namespace nodes as the live
    // instance. This is what `resolve-QName(@kind, .)` and prefixed
    // QName tests rely on.
    //
    // We also synthesize a default-namespace declaration when the live
    // element has one in scope (`xmlns="..."`), and we mirror the
    // element's own prefix→ns binding when the element is prefixed but
    // the snapshot has not surfaced that pair yet.
    let mut ns_strings: Vec<(String, String)> = Vec::new();
    if let Some(snapshot) = instance_ns {
        for &(prefix_id, uri_id) in &snapshot.bindings {
            let prefix = schema_set.name_table.resolve(prefix_id);
            let uri = schema_set.name_table.resolve(uri_id);
            ns_strings.push((prefix, uri));
        }
        if let Some(default_ns) = snapshot.default_ns {
            let uri = schema_set.name_table.resolve(default_ns);
            ns_strings.push((String::new(), uri));
        }
    }
    let ns_decls: Vec<(&str, &str)> = ns_strings
        .iter()
        .map(|(p, u)| (p.as_str(), u.as_str()))
        .collect();

    // Start element
    let local_str = schema_set.name_table.resolve(elem_local_name);
    let ns_str = elem_namespace
        .map(|id| schema_set.name_table.resolve(id))
        .unwrap_or_default();
    let elem_ref = match builder.start_element(&local_str, &ns_str, "", &ns_decls) {
        Ok(r) => r,
        Err(_) => return None,
    };

    // Add attributes
    for (ns, name, value) in collected_attributes {
        let attr_local = schema_set.name_table.resolve(*name);
        let attr_ns = ns
            .map(|id| schema_set.name_table.resolve(id))
            .unwrap_or_default();
        if builder.attribute(&attr_local, &attr_ns, "", value).is_err() {
            return None;
        }
    }

    builder.end_of_attributes();
    if builder.end_element().is_err() {
        return None;
    }

    let mut doc = match builder.finalize() {
        Ok(d) => d,
        Err(_) => return None,
    };

    // §3.12.4: E is the root of the CTA XDM instance, and the dynamic
    // base URI is the instance document's. (The static base URI on the
    // XPath context carries the schema document URI separately.)
    let fragment_base_uri = instance_base_uri
        .filter(|s| !s.is_empty())
        .map(|s| &*arena.alloc_str(s));
    doc.set_cta_fragment(elem_ref, fragment_base_uri);

    // Evaluate each alternative
    for alt in alternatives {
        if let Some(ref test_expr) = alt.test {
            // Build XPath context with the alternative's namespace snapshot.
            // The schema-time check (`schema::validate_cta_xpath`) refused any
            // test naming a user-defined component (only the built-in ones
            // are in its static context, Structures §3.12.6, §3.13.6.2); the
            // schema set serves the evaluation (type annotations of nodes).
            let ctx = XPathContext::new(&schema_set.name_table)
                .with_namespaces(alt.ns_snapshot.clone())
                .with_schema_set(schema_set);

            // §3.12.4: the static base URI of the CTA XPath context is
            // the URI of the schema document containing the
            // `xs:alternative` element. This lets `fn:static-base-uri()`
            // identify the schema document rather than the instance.
            let ctx = if let Some(schema_uri) = alt
                .source
                .as_ref()
                .and_then(|s| schema_set.documents.get(s.doc_id as usize))
                .map(|d| d.base_uri.as_str())
                .filter(|u| !u.is_empty())
            {
                ctx.with_base_uri(schema_uri.to_string())
            } else {
                ctx
            };

            let ctx = if let Some(default_ns) =
                resolve_alternative_default_ns(alt, elem_key, schema_set)
            {
                ctx.with_default_element_ns(default_ns)
            } else {
                ctx
            };

            let expr = match XPathExpr::compile(test_expr, &ctx) {
                Ok(e) => e,
                Err(_) => continue,
            };

            let nav = BufferDocNavigator::new(&doc, elem_ref);
            let warnings = std::cell::RefCell::new(Vec::new());
            let outcome = expr
                .evaluator(&ctx)
                .with_warnings(&warnings)
                .run_with_node(nav);
            super::assertions::note_expression_warnings(
                "type alternative test",
                test_expr,
                warnings.into_inner(),
            );
            let result = match outcome {
                Ok(r) => r,
                Err(_) => continue,
            };

            match effective_boolean_value(&result) {
                Ok(true) => return alt.resolved_type,
                _ => continue,
            }
        } else {
            // No test expression — this is the default fallback (must be last)
            return alt.resolved_type;
        }
    }

    None
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

    /// Find an element key by name.
    fn find_elem_key(schema_set: &SchemaSet, name: &str) -> ElementKey {
        let name_id = schema_set.name_table.add(name);
        for (key, elem) in &schema_set.arenas.elements {
            if elem.name == Some(name_id) {
                return key;
            }
        }
        panic!("Element '{}' not found", name);
    }

    /// Find a type key by name.
    fn find_type_key(schema_set: &SchemaSet, name: &str) -> TypeKey {
        let name_id = schema_set.name_table.add(name);
        for (key, ct) in &schema_set.arenas.complex_types {
            if ct.name == Some(name_id) {
                return TypeKey::Complex(key);
            }
        }
        for (key, st) in &schema_set.arenas.simple_types {
            if st.name == Some(name_id) {
                return TypeKey::Simple(key);
            }
        }
        panic!("Type '{}' not found", name);
    }

    #[test]
    fn test_no_alternatives_returns_none() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root" type="xs:string"/>
            </xs:schema>"#,
        );
        let elem_key = find_elem_key(&schema_set, "root");
        let local_name = schema_set.name_table.add("root");
        let result =
            evaluate_type_alternatives(elem_key, local_name, None, &[], None, None, &schema_set);
        assert!(result.is_none());
    }

    #[test]
    fn test_alternatives_parsed_and_resolved() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:complexType name="intType">
                    <xs:sequence>
                        <xs:element name="val" type="xs:integer"/>
                    </xs:sequence>
                </xs:complexType>
                <xs:complexType name="strType">
                    <xs:sequence>
                        <xs:element name="val" type="xs:string"/>
                    </xs:sequence>
                </xs:complexType>
                <xs:element name="root">
                    <xs:complexType>
                        <xs:sequence>
                            <xs:element name="val" type="xs:string"/>
                        </xs:sequence>
                        <xs:attribute name="kind" type="xs:string"/>
                    </xs:complexType>
                    <xs:alternative test="@kind='int'" type="intType"/>
                    <xs:alternative test="@kind='str'" type="strType"/>
                    <xs:alternative/>
                </xs:element>
            </xs:schema>"#,
        );
        let elem_key = find_elem_key(&schema_set, "root");
        let elem = &schema_set.arenas.elements[elem_key];
        assert_eq!(elem.alternatives.len(), 3);

        // First two should have tests and resolved types
        assert!(elem.alternatives[0].test.is_some());
        assert!(elem.alternatives[0].resolved_type.is_some());
        assert!(elem.alternatives[1].test.is_some());
        assert!(elem.alternatives[1].resolved_type.is_some());

        // Third is the default (no test)
        assert!(elem.alternatives[2].test.is_none());

        // Resolved types should match declared types
        let int_type = find_type_key(&schema_set, "intType");
        let str_type = find_type_key(&schema_set, "strType");
        assert_eq!(elem.alternatives[0].resolved_type, Some(int_type));
        assert_eq!(elem.alternatives[1].resolved_type, Some(str_type));
    }

    #[test]
    fn test_first_matching_alternative_wins() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:complexType name="typeA">
                    <xs:sequence>
                        <xs:element name="a" type="xs:string"/>
                    </xs:sequence>
                </xs:complexType>
                <xs:complexType name="typeB">
                    <xs:sequence>
                        <xs:element name="b" type="xs:string"/>
                    </xs:sequence>
                </xs:complexType>
                <xs:element name="root">
                    <xs:complexType>
                        <xs:sequence>
                            <xs:element name="x" type="xs:string"/>
                        </xs:sequence>
                        <xs:attribute name="kind" type="xs:string"/>
                    </xs:complexType>
                    <xs:alternative test="@kind='A'" type="typeA"/>
                    <xs:alternative test="@kind='B'" type="typeB"/>
                </xs:element>
            </xs:schema>"#,
        );
        let elem_key = find_elem_key(&schema_set, "root");
        let local_name = schema_set.name_table.add("root");
        let kind_name = schema_set.name_table.add("kind");

        // kind='A' -> typeA
        let attrs_a = vec![(None, kind_name, "A".to_string())];
        let result_a = evaluate_type_alternatives(
            elem_key,
            local_name,
            None,
            &attrs_a,
            None,
            None,
            &schema_set,
        );
        let type_a = find_type_key(&schema_set, "typeA");
        assert_eq!(result_a, Some(type_a));

        // Same scenario but with a non-empty instance_ns snapshot — this is
        // what `validation/runtime.rs::validate_end_of_attributes` passes,
        // and is suspected of breaking attribute-value comparison in CTA.
        let mut snapshot = NamespaceContextSnapshot::default();
        let xml_prefix = schema_set.name_table.add("xml");
        let xml_uri = schema_set
            .name_table
            .add("http://www.w3.org/XML/1998/namespace");
        snapshot.bindings.push((xml_prefix, xml_uri));
        let result_a_with_ns = evaluate_type_alternatives(
            elem_key,
            local_name,
            None,
            &attrs_a,
            Some(&snapshot),
            Some("file:///some/instance.xml"),
            &schema_set,
        );
        assert_eq!(
            result_a_with_ns,
            Some(type_a),
            "regression: CTA must select typeA even when an instance_ns snapshot is provided"
        );

        // Multiple sequential evaluations from the same name_table — does the
        // second one see the first one's attribute value?
        let attrs_b = vec![(None, kind_name, "B".to_string())];
        let result_b_after_a = evaluate_type_alternatives(
            elem_key,
            local_name,
            None,
            &attrs_b,
            Some(&snapshot),
            Some("file:///some/instance.xml"),
            &schema_set,
        );
        let type_b = find_type_key(&schema_set, "typeB");
        assert_eq!(
            result_b_after_a,
            Some(type_b),
            "regression: CTA must select typeB when called after a typeA evaluation"
        );

        // Three sequential calls (mirroring three polygons in typeAlternatives_001_2)
        let attrs_a2 = vec![(None, kind_name, "A".to_string())];
        let r1 = evaluate_type_alternatives(
            elem_key,
            local_name,
            None,
            &attrs_a2,
            Some(&snapshot),
            Some("file:///x.xml"),
            &schema_set,
        );
        let attrs_b2 = vec![(None, kind_name, "B".to_string())];
        let r2 = evaluate_type_alternatives(
            elem_key,
            local_name,
            None,
            &attrs_b2,
            Some(&snapshot),
            Some("file:///x.xml"),
            &schema_set,
        );
        let attrs_x = vec![(None, kind_name, "X".to_string())];
        let r3 = evaluate_type_alternatives(
            elem_key,
            local_name,
            None,
            &attrs_x,
            Some(&snapshot),
            Some("file:///x.xml"),
            &schema_set,
        );
        assert_eq!(r1, Some(type_a), "first sequential CTA: expected typeA");
        assert_eq!(r2, Some(type_b), "second sequential CTA: expected typeB");
        assert_eq!(r3, None, "third sequential CTA: expected no match");

        // kind='B' -> typeB
        let attrs_b = vec![(None, kind_name, "B".to_string())];
        let result_b = evaluate_type_alternatives(
            elem_key,
            local_name,
            None,
            &attrs_b,
            None,
            None,
            &schema_set,
        );
        let type_b = find_type_key(&schema_set, "typeB");
        assert_eq!(result_b, Some(type_b));
    }

    #[test]
    fn test_no_match_returns_none() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:complexType name="typeA">
                    <xs:sequence>
                        <xs:element name="a" type="xs:string"/>
                    </xs:sequence>
                </xs:complexType>
                <xs:element name="root">
                    <xs:complexType>
                        <xs:sequence>
                            <xs:element name="x" type="xs:string"/>
                        </xs:sequence>
                        <xs:attribute name="kind" type="xs:string"/>
                    </xs:complexType>
                    <xs:alternative test="@kind='A'" type="typeA"/>
                </xs:element>
            </xs:schema>"#,
        );
        let elem_key = find_elem_key(&schema_set, "root");
        let local_name = schema_set.name_table.add("root");
        let kind_name = schema_set.name_table.add("kind");

        // kind='X' -> no match, no default -> None
        let attrs = vec![(None, kind_name, "X".to_string())];
        let result =
            evaluate_type_alternatives(elem_key, local_name, None, &attrs, None, None, &schema_set);
        assert!(result.is_none());
    }

    #[test]
    fn test_default_fallback_no_test() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:complexType name="typeA">
                    <xs:sequence>
                        <xs:element name="a" type="xs:string"/>
                    </xs:sequence>
                </xs:complexType>
                <xs:complexType name="defaultType">
                    <xs:sequence>
                        <xs:element name="d" type="xs:string"/>
                    </xs:sequence>
                </xs:complexType>
                <xs:element name="root">
                    <xs:complexType>
                        <xs:sequence>
                            <xs:element name="x" type="xs:string"/>
                        </xs:sequence>
                        <xs:attribute name="kind" type="xs:string"/>
                    </xs:complexType>
                    <xs:alternative test="@kind='A'" type="typeA"/>
                    <xs:alternative type="defaultType"/>
                </xs:element>
            </xs:schema>"#,
        );
        let elem_key = find_elem_key(&schema_set, "root");
        let local_name = schema_set.name_table.add("root");
        let kind_name = schema_set.name_table.add("kind");

        // kind='X' -> no test match -> default fallback
        let attrs = vec![(None, kind_name, "X".to_string())];
        let result =
            evaluate_type_alternatives(elem_key, local_name, None, &attrs, None, None, &schema_set);
        let default_type = find_type_key(&schema_set, "defaultType");
        assert_eq!(result, Some(default_type));
    }

    #[test]
    fn test_inline_type_alternative() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root">
                    <xs:complexType>
                        <xs:sequence>
                            <xs:element name="x" type="xs:string"/>
                        </xs:sequence>
                        <xs:attribute name="mode" type="xs:string"/>
                    </xs:complexType>
                    <xs:alternative test="@mode='simple'">
                        <xs:complexType>
                            <xs:sequence>
                                <xs:element name="s" type="xs:string"/>
                            </xs:sequence>
                        </xs:complexType>
                    </xs:alternative>
                </xs:element>
            </xs:schema>"#,
        );
        let elem_key = find_elem_key(&schema_set, "root");
        let elem = &schema_set.arenas.elements[elem_key];
        assert_eq!(elem.alternatives.len(), 1);
        // The inline type should have been resolved
        assert!(elem.alternatives[0].resolved_type.is_some());

        // Evaluate with matching attribute
        let local_name = schema_set.name_table.add("root");
        let mode_name = schema_set.name_table.add("mode");
        let attrs = vec![(None, mode_name, "simple".to_string())];
        let result =
            evaluate_type_alternatives(elem_key, local_name, None, &attrs, None, None, &schema_set);
        assert!(result.is_some());
        assert_eq!(result, elem.alternatives[0].resolved_type);
    }

    /// Regression for cta0008.v01: when CTA selects an inline complex type
    /// that *extends* the declared type with new particles, the selected
    /// type's resolved_content_particle_elements must include the
    /// extension's own elements so the runtime content matcher accepts
    /// them.
    #[test]
    fn test_inline_alternative_extension_propagates_particles() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="Example">
                    <xs:complexType>
                        <xs:sequence>
                            <xs:element name="Publication" type="PublicationType" maxOccurs="unbounded">
                                <xs:alternative test="@kind = 'book'">
                                    <xs:complexType>
                                        <xs:complexContent>
                                            <xs:extension base="PublicationType">
                                                <xs:sequence>
                                                    <xs:element name="ISBN" type="xs:string"/>
                                                    <xs:element name="Publisher" type="xs:string"/>
                                                </xs:sequence>
                                            </xs:extension>
                                        </xs:complexContent>
                                    </xs:complexType>
                                </xs:alternative>
                            </xs:element>
                        </xs:sequence>
                    </xs:complexType>
                </xs:element>

                <xs:complexType name="PublicationType">
                    <xs:sequence>
                        <xs:element name="Title" type="xs:string"/>
                    </xs:sequence>
                    <xs:attribute name="kind" type="xs:string"/>
                </xs:complexType>
            </xs:schema>"#,
        );

        // Find the Publication element and its alternative.
        let pub_key = find_elem_key(&schema_set, "Publication");
        let pub_elem = &schema_set.arenas.elements[pub_key];
        assert_eq!(pub_elem.alternatives.len(), 1);
        let resolved_alt_type = pub_elem.alternatives[0]
            .resolved_type
            .expect("alternative should resolve to a TypeKey");

        // The selected alternative type must be a complex type and must
        // expose ISBN/Publisher as resolved particle elements after the
        // allocation pass.
        let TypeKey::Complex(alt_ct_key) = resolved_alt_type else {
            panic!("expected alternative to resolve to a complex type");
        };
        let alt_ct = &schema_set.arenas.complex_types[alt_ct_key];
        let isbn = schema_set.name_table.get("ISBN").expect("ISBN interned");
        let publisher = schema_set
            .name_table
            .get("Publisher")
            .expect("Publisher interned");

        let mut found_isbn = false;
        let mut found_publisher = false;
        for entry in &alt_ct.resolved_content_particle_elements {
            let Some(elem_key) = entry else { continue };
            let elem = &schema_set.arenas.elements[*elem_key];
            if elem.name == Some(isbn) {
                found_isbn = true;
            }
            if elem.name == Some(publisher) {
                found_publisher = true;
            }
        }
        assert!(
            found_isbn && found_publisher,
            "inline alternative extension type should have ISBN and Publisher \
             in resolved_content_particle_elements (found_isbn={}, \
             found_publisher={})",
            found_isbn,
            found_publisher
        );
    }

    /// Regression for cta0044.n01: when CTA selects a restricted type
    /// that prohibits an attribute the base allows, the deferred
    /// attribute validation must report the prohibition. We assert
    /// the schema-time picture: the restricted type's own attribute
    /// uses must list `r` with `Prohibited`.
    #[test]
    fn test_cta0044_restriction_prohibits_attr() {
        use crate::parser::frames::AttributeUseKind;
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:complexType name="aType">
                    <xs:sequence>
                        <xs:element name="t" type="xs:string" minOccurs="0"/>
                        <xs:element name="f" type="xs:string" minOccurs="0"/>
                    </xs:sequence>
                    <xs:attribute name="switch" type="xs:string"/>
                    <xs:attribute name="r" type="xs:string"/>
                </xs:complexType>

                <xs:complexType name="aType_f">
                    <xs:complexContent>
                        <xs:restriction base="aType">
                            <xs:sequence>
                                <xs:element name="f" type="xs:string"/>
                            </xs:sequence>
                            <xs:attribute name="switch" type="xs:string"/>
                            <xs:attribute name="r" use="prohibited"/>
                        </xs:restriction>
                    </xs:complexContent>
                </xs:complexType>

                <xs:element name="a" type="aType">
                    <xs:alternative type="aType_f" test="@switch = 'f'"/>
                </xs:element>
            </xs:schema>"#,
        );

        let r_name = schema_set.name_table.get("r").expect("r interned");
        let TypeKey::Complex(at_f_key) = find_type_key(&schema_set, "aType_f") else {
            panic!("aType_f should be a complex type");
        };
        let at_f = &schema_set.arenas.complex_types[at_f_key];
        let r_use = at_f
            .attributes
            .iter()
            .find(|au| au.attribute.name == Some(r_name))
            .expect("aType_f should declare its own 'r' attribute use");
        assert_eq!(
            r_use.use_kind,
            AttributeUseKind::Prohibited,
            "aType_f's own 'r' use must be Prohibited"
        );
    }

    #[test]
    fn test_numeric_attribute_test() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:complexType name="smallType">
                    <xs:sequence>
                        <xs:element name="v" type="xs:string"/>
                    </xs:sequence>
                </xs:complexType>
                <xs:complexType name="largeType">
                    <xs:sequence>
                        <xs:element name="v" type="xs:string"/>
                    </xs:sequence>
                </xs:complexType>
                <xs:element name="item">
                    <xs:complexType>
                        <xs:sequence>
                            <xs:element name="v" type="xs:string"/>
                        </xs:sequence>
                        <xs:attribute name="size" type="xs:integer"/>
                    </xs:complexType>
                    <xs:alternative test="@size &lt; 10" type="smallType"/>
                    <xs:alternative test="@size >= 10" type="largeType"/>
                </xs:element>
            </xs:schema>"#,
        );
        let elem_key = find_elem_key(&schema_set, "item");
        let local_name = schema_set.name_table.add("item");
        let size_name = schema_set.name_table.add("size");

        // size=5 -> smallType
        let attrs_small = vec![(None, size_name, "5".to_string())];
        let result_small = evaluate_type_alternatives(
            elem_key,
            local_name,
            None,
            &attrs_small,
            None,
            None,
            &schema_set,
        );
        let small_type = find_type_key(&schema_set, "smallType");
        assert_eq!(result_small, Some(small_type));

        // size=100 -> largeType
        let attrs_large = vec![(None, size_name, "100".to_string())];
        let result_large = evaluate_type_alternatives(
            elem_key,
            local_name,
            None,
            &attrs_large,
            None,
            None,
            &schema_set,
        );
        let large_type = find_type_key(&schema_set, "largeType");
        assert_eq!(result_large, Some(large_type));
    }
}
