mod support;
use support::*;
use xsd_schema::error::FacetError;
use xsd_schema::types::validators::ValidationError as ValueError;
use xsd_schema::validation::errors::{self, ComponentKind, NativeCause};
use xsd_schema::validation::*;

#[test]
fn lexical_and_facet_causes_are_owned_and_raw_diagnostics_are_untouched() {
    let errors = case("duplicate-bad");
    assert_eq!(errors.len(), 2);
    assert_eq!(errors[0].message, errors[1].message);
    for (i, e) in errors.iter().enumerate() {
        assert_eq!(e.constraint, "cvc-datatype-valid");
        assert!(e.location.is_none());
        assert_eq!(
            e.instance_path().unwrap().to_string(),
            format!("/Q{{}}r[1]/Q{{}}d[{}]", i + 1)
        );
        let evidence = e.evidence().unwrap();
        assert!(
            matches!(&evidence.cause, Some(NativeCause::Value(ValueError::InvalidLexical { value, type_name: "decimal", .. })) if value == "bad")
        );
        let governing = evidence.governing_type.as_ref().unwrap();
        assert_eq!(governing.name.as_ref().unwrap().local_name(), "Positive");
        assert_eq!(governing.kind, ComponentKind::SimpleType);
        assert_eq!(governing.document_uri.as_deref(), Some("authored.xsd"));
        let (start, end) = governing.schema_span.unwrap();
        assert!(XSD[start..end].starts_with("<xs:simpleType"));
        assert!(evidence.facet_origin.is_none());
    }
    // Schema/runtime are already dropped; original typed payloads still work.
    let errors = case("facet");
    let e = &errors[0];
    assert_eq!(e.constraint, "cvc-minInclusive-valid");
    assert!(matches!(&e.evidence().unwrap().cause,
        Some(NativeCause::Value(ValueError::FacetViolation(FacetError::MinInclusiveViolation { value, min }))) if value == "-1" && min == "0"));
    let raw = errors::error("native-code", "  untrimmed\nmessage \t", None);
    assert!(raw.evidence().is_none());
    let enriched = raw
        .clone()
        .with_native_cause(NativeCause::IncompleteContent);
    assert_eq!(enriched.message.as_bytes(), raw.message.as_bytes());
    assert_eq!(enriched.to_string(), raw.to_string());
    let native = FacetError::PatternViolation {
        value: " x ".to_owned(),
        pattern: "[0-9]+".to_owned(),
    };
    let message = native.to_string();
    let retained = errors::from_facet_error("cvc-pattern-valid", native, None);
    assert_eq!(retained.message, message);
    assert!(
        matches!(&retained.evidence().unwrap().cause, Some(NativeCause::Facet(FacetError::PatternViolation { value, pattern })) if value == " x " && pattern == "[0-9]+")
    );
}

#[test]
fn actual_emission_branches_are_distinct() {
    for name in ["extra", "order"] {
        let es = case(name);
        assert!(es.iter().any(|e| matches!(
            e.evidence().and_then(|v| v.cause.as_ref()),
            Some(NativeCause::UnexpectedChild { .. })
        )));
    }
    let es = case("missing-child");
    let e = &es[0];
    assert!(matches!(
        e.evidence().unwrap().cause,
        Some(NativeCause::IncompleteContent)
    ));
    assert_eq!(
        e.evidence()
            .unwrap()
            .governing_type
            .as_ref()
            .unwrap()
            .name
            .as_ref()
            .unwrap()
            .local_name(),
        "Body"
    );
    assert_eq!(e.instance_path().unwrap().to_string(), "/Q{}r[1]");
    for (name, predicate) in [
        (
            "extra-attribute",
            (|c: &NativeCause| matches!(c, NativeCause::AttributeNotAllowed))
                as fn(&NativeCause) -> bool,
        ),
        ("prohibited-attribute", |c| {
            matches!(c, NativeCause::ProhibitedAttribute)
        }),
        (
            "fixed-attribute",
            |c| matches!(c, NativeCause::AttributeFixedValue { expected, actual } if expected == "ok" && actual == "no"),
        ),
        ("xsi-nil", |c| matches!(c, NativeCause::NilNotAllowed)),
        ("nil-content", |c| matches!(c, NativeCause::NilWithContent)),
        ("nil-fixed", |c| matches!(c, NativeCause::NilWithFixedValue)),
        ("unknown-root", |c| {
            matches!(c, NativeCause::UndeclaredElement)
        }),
    ] {
        let es = case(name);
        assert!(
            es.iter().any(|e| e
                .evidence()
                .and_then(|v| v.cause.as_ref())
                .is_some_and(predicate)),
            "{name}: {es:?}"
        );
    }
    let es = case("unknown-root");
    let evidence = es[0].evidence().unwrap();
    assert!(evidence.component.is_none() && evidence.governing_type.is_none());
}

#[test]
fn missing_qualified_attribute_and_popped_local_type_are_not_guessed() {
    let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:a" elementFormDefault="qualified" attributeFormDefault="qualified"><xs:element name="r"><xs:complexType><xs:sequence><xs:element name="c"><xs:complexType><xs:sequence><xs:element name="d"/></xs:sequence></xs:complexType></xs:element></xs:sequence><xs:attribute name="req" use="required"/></xs:complexType></xs:element></xs:schema>"#;
    let schema = schema(xsd);
    let (_, es, _) = capture(&schema, r#"<r xmlns="urn:a"><c/></r>"#);
    let missing = es
        .iter()
        .find(|e| e.constraint == "cvc-complex-type.4")
        .unwrap();
    assert!(
        matches!(&missing.evidence().unwrap().cause, Some(NativeCause::MissingRequiredAttribute { name }) if name.namespace() == Some("urn:a") && name.local_name() == "req")
    );
    assert!(missing.evidence().unwrap().component.is_none()); // exact attribute use not captured
    let incomplete = es
        .iter()
        .find(|e| e.constraint == "cvc-complex-type.2.4")
        .unwrap();
    assert_eq!(
        incomplete.instance_path().unwrap().to_string(),
        "/Q{urn:a}r[1]/Q{urn:a}c[1]"
    );
    let t = incomplete
        .evidence()
        .unwrap()
        .governing_type
        .as_ref()
        .unwrap();
    assert!(t.name.is_none());
    let (start, end) = t.schema_span.unwrap();
    let child_type_start = xsd.find("<xs:element name=\"c\"><xs:complexType>").unwrap()
        + "<xs:element name=\"c\">".len();
    assert_eq!(start, child_type_start);
    assert!(xsd[start..end].starts_with("<xs:complexType"));
}

#[test]
fn deferred_xsi_type_causes_retain_the_actual_and_declared_types() {
    for name in ["xsi-type-prefix", "xsi-type-unresolved"] {
        let es = case(name);
        let e = es.iter().find(|e| e.constraint == "cvc-elt.4.1").unwrap();
        assert!(matches!(
            e.evidence().unwrap().cause,
            Some(NativeCause::XsiTypeUnresolved { .. })
        ));
        assert_eq!(e.instance_path().unwrap().to_string(), "/Q{}n[1]");
    }
    let es = case("xsi-type-derivation");
    let e = es.iter().find(|e| e.constraint == "cvc-elt.4.2").unwrap();
    let evidence = e.evidence().unwrap();
    assert!(matches!(
        evidence.cause,
        Some(NativeCause::XsiTypeDerivation { blocked: false, .. })
    ));
    assert_eq!(
        evidence
            .governing_type
            .as_ref()
            .unwrap()
            .name
            .as_ref()
            .unwrap()
            .local_name(),
        "int"
    );
    assert_eq!(
        evidence
            .actual_type
            .as_ref()
            .unwrap()
            .name
            .as_ref()
            .unwrap()
            .local_name(),
        "string"
    );
}

#[test]
fn valid_and_invalid_runs_remain_distinct() {
    let schema = schema(XSD);
    let (result, errors, _) = capture(&schema, VALID);
    assert_eq!(result.root_validity, Some(SchemaValidity::Valid));
    assert!(errors.is_empty());
    for (name, xml) in CASES
        .iter()
        .filter(|(name, _)| !matches!(*name, "valid" | "warning"))
    {
        let (_, errors, _) = capture(&schema, xml);
        assert!(!errors.is_empty(), "{name}");
    }
}

#[test]
fn inherited_facets_do_not_claim_the_resolved_type_as_the_facet_origin() {
    let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"><xs:simpleType name="Base"><xs:restriction base="xs:decimal"><xs:minInclusive value="0"/></xs:restriction></xs:simpleType><xs:simpleType name="Derived"><xs:restriction base="Base"><xs:maxInclusive value="10"/></xs:restriction></xs:simpleType><xs:element name="r" type="Derived"/></xs:schema>"#;
    let schema = schema(xsd);
    let (_, es, _) = capture(&schema, "<r>-1</r>");
    let evidence = es[0].evidence().unwrap();
    let resolved_type = evidence.governing_type.as_ref().unwrap();
    assert_eq!(resolved_type.name.as_ref().unwrap().local_name(), "Derived");
    let (start, end) = resolved_type.schema_span.unwrap();
    assert!(xsd[start..end].starts_with("<xs:simpleType name=\"Derived\""));
    assert!(evidence.facet_origin.is_none());
    assert!(matches!(
        evidence.cause,
        Some(NativeCause::Value(ValueError::FacetViolation(
            FacetError::MinInclusiveViolation { .. }
        )))
    ));
}

#[test]
fn simple_content_keeps_governing_and_actual_dispatches_distinct() {
    let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"><xs:simpleType name="Base"><xs:restriction base="xs:decimal"><xs:minInclusive value="0"/></xs:restriction></xs:simpleType><xs:complexType name="Outer"><xs:simpleContent><xs:extension base="Base"/></xs:simpleContent></xs:complexType><xs:element name="r" type="Outer"/></xs:schema>"#;
    let schema = schema(xsd);
    let (_, es, _) = capture(&schema, "<r>bad</r>");
    let evidence = es[0].evidence().unwrap();
    assert_eq!(
        evidence
            .governing_type
            .as_ref()
            .unwrap()
            .name
            .as_ref()
            .unwrap()
            .local_name(),
        "Outer"
    );
    assert_eq!(
        evidence
            .actual_type
            .as_ref()
            .unwrap()
            .name
            .as_ref()
            .unwrap()
            .local_name(),
        "Base"
    );
}

#[test]
fn explicit_raw_location_and_untrimmed_message_survive_enrichment() {
    let location = xsd_schema::parser::location::SourceLocation {
        base_uri: "instance.xml".to_owned(),
        line: 7,
        column: 3,
    };
    let raw = errors::error("native-code", " \t message\n ", Some(location.clone()));
    let enriched = raw
        .clone()
        .with_native_cause(NativeCause::IncompleteContent);
    assert_eq!(enriched.location, Some(location));
    assert_eq!(enriched.constraint, raw.constraint);
    assert_eq!(enriched.message.as_bytes(), raw.message.as_bytes());
    assert_eq!(enriched.to_string(), raw.to_string());
}

#[test]
fn local_complex_content_facets_retain_the_native_cause() {
    let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"><xs:complexType name="Base"><xs:simpleContent><xs:extension base="xs:decimal"/></xs:simpleContent></xs:complexType><xs:complexType name="Tight"><xs:simpleContent><xs:restriction base="Base"><xs:minInclusive value="0"/></xs:restriction></xs:simpleContent></xs:complexType><xs:element name="r" type="Tight"/></xs:schema>"#;
    let schema = schema(xsd);
    let (_, es, _) = capture(&schema, "<r>-1</r>");
    assert_eq!(es[0].constraint, "cvc-minInclusive-valid");
    assert!(matches!(
        es[0].evidence().unwrap().cause,
        Some(NativeCause::Value(ValueError::FacetViolation(
            FacetError::MinInclusiveViolation { .. }
        )))
    ));
}
