//! Every schema-loading entry point runs the same checks (C87).
//!
//! A schema set can be built through the builder (`SchemaSetBuilder::compile`,
//! and `compile_async` with the `async` feature), through the one-document
//! pipeline (`load_and_process_schema`, `load_and_process_schema_async`), or
//! by hand (`parse_schema_only` + `process_loaded_schemas`). They must accept
//! and refuse the same schemas with the same first error: a schema that is
//! not valid for one of them is not valid, whatever entry point loads it.
//!
//! Each case below breaks exactly one of the checks that the entry points
//! used to run differently — the post-allocation pass (UPA, the XSD 1.0
//! all-group rules, occurrence ranges, local element value constraints, type
//! alternatives, identity-constraint references), the substitution-group
//! checks, the local `targetNamespace` and `xsi` rules, and the parse errors
//! collected in error-recovery mode.

use xsd_schema::{
    load_and_process_schema, parse_schema_only, process_loaded_schemas, SchemaError, SchemaSet,
    SchemaSetBuilder, XsdVersion,
};

const URI: &str = "file:///pipeline-parity/main.xsd";

/// The verdict of one entry point: `Ok` or the first error, as its kind,
/// constraint (structural errors) and message.
#[derive(Debug, PartialEq, Eq, Clone)]
enum Verdict {
    Valid,
    Invalid { kind: &'static str, text: String },
}

impl Verdict {
    fn of<T>(result: Result<T, SchemaError>) -> Verdict {
        match result {
            Ok(_) => Verdict::Valid,
            Err(e) => Verdict::Invalid {
                kind: match &e {
                    SchemaError::StructuralError { constraint, .. } => constraint,
                    SchemaError::XmlError { .. } => "xml",
                    SchemaError::NamespaceError { .. } => "namespace",
                    SchemaError::FeatureError { .. } => "feature",
                    SchemaError::ResolutionError { .. } => "resolution",
                    SchemaError::IoError(_) => "io",
                    SchemaError::Internal(_) => "internal",
                    SchemaError::LimitExceeded { .. } => "limit",
                    _ => "other",
                },
                text: e.to_string(),
            },
        }
    }

    fn kind(&self) -> Option<&'static str> {
        match self {
            Verdict::Valid => None,
            Verdict::Invalid { kind, .. } => Some(kind),
        }
    }
}

fn builder(version: XsdVersion, xsd: &str) -> Verdict {
    Verdict::of(
        SchemaSetBuilder::with_version(version)
            .add_source(xsd, URI)
            .and_then(|b| b.compile()),
    )
}

fn builder_bytes(version: XsdVersion, xsd: &str) -> Verdict {
    Verdict::of(
        SchemaSetBuilder::with_version(version)
            .add_bytes(xsd.as_bytes(), URI)
            .and_then(|b| b.compile()),
    )
}

fn pipeline(version: XsdVersion, xsd: &str) -> Verdict {
    let mut set = SchemaSet::with_version(version);
    Verdict::of(load_and_process_schema(xsd.as_bytes(), URI, &mut set, None))
}

fn by_hand(version: XsdVersion, xsd: &str) -> Verdict {
    let mut set = SchemaSet::with_version(version);
    Verdict::of(
        parse_schema_only(xsd.as_bytes(), URI, &mut set)
            .and_then(|_| process_loaded_schemas(&mut set)),
    )
}

#[cfg(feature = "async")]
fn pipeline_async(version: XsdVersion, xsd: &str) -> Verdict {
    let mut set = SchemaSet::with_version(version);
    Verdict::of(futures::executor::block_on(
        xsd_schema::load_and_process_schema_async(xsd.as_bytes(), URI, &mut set, None),
    ))
}

#[cfg(feature = "async")]
fn builder_async(version: XsdVersion, xsd: &str) -> Verdict {
    let builder = SchemaSetBuilder::with_version(version).add_source(xsd, URI);
    Verdict::of(futures::executor::block_on(async move {
        builder?.compile_async().await
    }))
}

/// Every entry point's verdict on `xsd`, by name.
fn verdicts(version: XsdVersion, xsd: &str) -> Vec<(&'static str, Verdict)> {
    #[allow(unused_mut)]
    let mut all = vec![
        ("SchemaSetBuilder::compile", builder(version, xsd)),
        (
            "SchemaSetBuilder::add_bytes + compile",
            builder_bytes(version, xsd),
        ),
        ("load_and_process_schema", pipeline(version, xsd)),
        (
            "parse_schema_only + process_loaded_schemas",
            by_hand(version, xsd),
        ),
    ];
    #[cfg(feature = "async")]
    {
        all.push((
            "load_and_process_schema_async",
            pipeline_async(version, xsd),
        ));
        all.push((
            "SchemaSetBuilder::compile_async",
            builder_async(version, xsd),
        ));
    }
    all
}

fn schema(attrs: &str, body: &str) -> String {
    format!(r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" {attrs}>{body}</xs:schema>"#)
}

/// Asserts that every entry point gives `expected` (a constraint, or `None`
/// for a valid schema) and the same message.
fn assert_parity(name: &str, version: XsdVersion, xsd: &str, expected: Option<&str>) {
    let all = verdicts(version, xsd);
    let reference = &all[0].1;
    let mut problems = Vec::new();
    for (entry, verdict) in &all {
        if verdict.kind() != expected {
            problems.push(format!("  {entry}: expected {expected:?}, got {verdict:?}"));
        } else if verdict != reference {
            problems.push(format!(
                "  {entry}: same constraint, different error: {verdict:?} vs {reference:?}"
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "{name} ({version:?}):\n{}",
        problems.join("\n")
    );
}

#[cfg(feature = "xsd11")]
const BOTH: &[XsdVersion] = &[XsdVersion::V1_0, XsdVersion::V1_1];
#[cfg(not(feature = "xsd11"))]
const BOTH: &[XsdVersion] = &[XsdVersion::V1_0];

#[test]
fn a_valid_schema_is_valid_everywhere() {
    let xsd = schema(
        r#"targetNamespace="urn:t" xmlns:t="urn:t""#,
        r#"<xs:element name="r" type="t:T"/>
           <xs:complexType name="T"><xs:sequence>
             <xs:element name="a" type="xs:int" default="1" minOccurs="0"/>
             <xs:element ref="t:h" minOccurs="0"/>
           </xs:sequence></xs:complexType>
           <xs:element name="h" type="xs:anyType"/>
           <xs:element name="m" type="xs:int" substitutionGroup="t:h"/>"#,
    );
    for &version in BOTH {
        assert_parity("valid", version, &xsd, None);
    }
}

/// §3.8.6.4 Unique Particle Attribution, through the post-allocation pass.
#[test]
fn upa_is_checked_everywhere() {
    let xsd = schema(
        "",
        r#"<xs:complexType name="T"><xs:sequence>
             <xs:element name="a" minOccurs="0"/><xs:element name="a"/>
           </xs:sequence></xs:complexType>"#,
    );
    for &version in BOTH {
        assert_parity("UPA", version, &xsd, Some("cos-nonambig"));
    }
}

/// XSD 1.0 All Group Limited: a named all group referenced from a sequence.
#[test]
fn all_group_placement_is_checked_everywhere() {
    let xsd = schema(
        "",
        r#"<xs:group name="G"><xs:all><xs:element name="a"/></xs:all></xs:group>
           <xs:complexType name="T"><xs:sequence>
             <xs:group ref="G"/><xs:element name="b"/>
           </xs:sequence></xs:complexType>"#,
    );
    assert_parity(
        "all-group placement",
        XsdVersion::V1_0,
        &xsd,
        Some("cos-all-limited"),
    );
}

/// XSD 1.0 All Group Limited: an all group's particle with maxOccurs > 1.
#[test]
fn all_group_outer_occurs_is_checked_everywhere() {
    let xsd = schema(
        "",
        r#"<xs:group name="G"><xs:all><xs:element name="a"/></xs:all></xs:group>
           <xs:complexType name="T"><xs:group ref="G" maxOccurs="2"/></xs:complexType>"#,
    );
    assert_parity(
        "all-group outer occurs",
        XsdVersion::V1_0,
        &xsd,
        Some("cos-all-limited"),
    );
}

/// e-props-correct.2: a local element's default is not valid for its type.
#[test]
fn local_element_value_constraints_are_checked_everywhere() {
    let xsd = schema(
        "",
        r#"<xs:complexType name="T"><xs:sequence>
             <xs:element name="x" type="xs:int" default="abc"/>
           </xs:sequence></xs:complexType>"#,
    );
    for &version in BOTH {
        assert_parity("local default", version, &xsd, Some("e-props-correct.2"));
    }
}

/// e-props-correct.4: a substitution-group member's type must derive from
/// the head's.
#[test]
fn substitution_groups_are_checked_everywhere() {
    let xsd = schema(
        "",
        r#"<xs:element name="h" type="xs:int"/>
           <xs:element name="m" type="xs:string" substitutionGroup="h"/>"#,
    );
    for &version in BOTH {
        assert_parity(
            "substitution group",
            version,
            &xsd,
            Some("e-props-correct.4"),
        );
    }
}

/// §3.2.6.4 (no-xsi): no attribute declaration in the xsi namespace.
#[test]
fn xsi_attribute_declarations_are_refused_everywhere() {
    let xsd = schema(
        r#"targetNamespace="http://www.w3.org/2001/XMLSchema-instance""#,
        r#"<xs:attribute name="foo"/>"#,
    );
    for &version in BOTH {
        assert_parity("no-xsi", version, &xsd, Some("no-xsi"));
    }
}

/// §3.8.6.3 Element Declarations Consistent, across a substitution group.
#[test]
fn element_consistency_through_substitution_is_checked_everywhere() {
    let xsd = schema(
        "",
        r#"<xs:element name="h" type="xs:anyType"/>
           <xs:element name="m" type="xs:int" substitutionGroup="h"/>
           <xs:complexType name="T"><xs:sequence>
             <xs:element ref="h"/><xs:element name="m" type="xs:string"/>
           </xs:sequence></xs:complexType>"#,
    );
    for &version in BOTH {
        assert_parity(
            "element consistency",
            version,
            &xsd,
            Some("cos-element-consistent"),
        );
    }
}

/// XSD 1.1 §3.3.3 clause 4.3: a local element's `targetNamespace` may
/// differ from the schema's only in a restriction of a non-anyType base.
#[cfg(feature = "xsd11")]
#[test]
fn local_target_namespace_is_checked_everywhere() {
    let xsd = schema(
        r#"targetNamespace="urn:t""#,
        r#"<xs:complexType name="T"><xs:sequence>
             <xs:element name="x" targetNamespace="urn:other"/>
           </xs:sequence></xs:complexType>"#,
    );
    assert_parity(
        "local targetNamespace",
        XsdVersion::V1_1,
        &xsd,
        Some("src-element"),
    );
}

/// XSD 1.1 §3.12.6: a local element's type alternative must be validly
/// substitutable for the element's declared type.
#[cfg(feature = "xsd11")]
#[test]
fn local_type_alternatives_are_checked_everywhere() {
    let xsd = schema(
        "",
        r#"<xs:complexType name="T"><xs:sequence>
             <xs:element name="x" type="xs:int">
               <xs:alternative test="@a = 1" type="xs:string"/>
             </xs:element>
           </xs:sequence></xs:complexType>"#,
    );
    let verdict = &verdicts(XsdVersion::V1_1, &xsd)[0].1;
    let expected = verdict.kind();
    assert!(expected.is_some(), "the builder refuses it: {verdict:?}");
    assert_parity("local alternative", XsdVersion::V1_1, &xsd, expected);
}

/// An element the schema for schemas does not allow is collected in
/// error-recovery mode and reported by every entry point (as a schema for
/// schemas violation, `s4s-elt-invalid-content`, since fix round F14).
#[test]
fn parse_errors_collected_in_recovery_are_reported_everywhere() {
    let xsd = schema(
        "",
        r#"<xs:element name="a"><xs:annotation/><xs:annotation/></xs:element>"#,
    );
    for &version in BOTH {
        assert_parity(
            "recovered parse error",
            version,
            &xsd,
            Some("s4s-elt-invalid-content"),
        );
    }
}

/// Structures 1.1 §3.13.6.1 / §3.13.6.2 (XPath Valid, clause 2.2.5: only the
/// built-in components are in scope) and §3.12.6: an assertion, an assertion
/// facet or a type alternative — on a top-level or a local declaration — that
/// names a user-defined type is refused by every entry point.
#[cfg(feature = "xsd11")]
#[test]
fn assertion_and_alternative_xpath_is_checked_everywhere() {
    let ns = r#"targetNamespace="urn:t" xmlns:t="urn:t""#;
    let small = r#"<xs:simpleType name="small"><xs:restriction base="xs:int"/></xs:simpleType>"#;
    let cases = [
        (
            "assertion",
            format!(
                r#"{small}<xs:complexType name="T"><xs:attribute name="a" type="t:small"/>
                   <xs:assert test="data(@a) instance of t:small"/></xs:complexType>"#
            ),
            "as-props-correct",
        ),
        (
            "local assertion",
            format!(
                r#"{small}<xs:element name="r"><xs:complexType><xs:sequence>
                     <xs:element name="x"><xs:complexType>
                       <xs:attribute name="a" type="t:small"/>
                       <xs:assert test="(@a cast as t:small) = 1"/>
                     </xs:complexType></xs:element>
                   </xs:sequence></xs:complexType></xs:element>"#
            ),
            "as-props-correct",
        ),
        (
            "assertion facet",
            format!(
                r#"{small}<xs:simpleType name="s"><xs:restriction base="t:small">
                   <xs:assertion test="t:small($value) = $value"/></xs:restriction></xs:simpleType>"#
            ),
            "as-props-correct",
        ),
        (
            "simple-content assertion facet",
            format!(
                r#"{small}<xs:complexType name="B"><xs:simpleContent><xs:extension base="t:small"/></xs:simpleContent></xs:complexType>
                   <xs:complexType name="R"><xs:simpleContent><xs:restriction base="t:B">
                   <xs:assertion test="$value instance of t:small"/></xs:restriction></xs:simpleContent></xs:complexType>"#
            ),
            "as-props-correct",
        ),
        (
            "local type alternative",
            format!(
                r#"{small}<xs:element name="r"><xs:complexType><xs:sequence>
                     <xs:element name="x"><xs:alternative test="exists(@a[. instance of attribute(*, t:small)])" type="xs:anyType"/></xs:element>
                   </xs:sequence></xs:complexType></xs:element>"#
            ),
            "src-ct-alternative",
        ),
    ];
    for (name, body, expected) in cases {
        assert_parity(name, XsdVersion::V1_1, &schema(ns, &body), Some(expected));
    }
}
