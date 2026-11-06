//! A schema with several errors reports the same one every time.
//!
//! Which error is reported first among independent invalid components is not
//! prescribed (a schema is invalid when any component violates a constraint),
//! but it must not change from one run to the next. Two orders used to come
//! from hash order: the order independent types were checked in (the
//! dependency graph's topological sort), and the order an enumeration's
//! values were checked in (its {value} is a set, Datatypes 1.1 §4.3.5.1).
//! Types are now checked in the order they were created — top-level
//! definitions in document order — and enumeration values in sorted order.
//!
//! Each case compiles 40 times in one process: every `HashMap` gets its own
//! random keys, so an order taken from one varies here too.

use xsd_schema::{SchemaSetBuilder, XsdVersion};

fn error(version: XsdVersion, text: &str) -> String {
    let result = SchemaSetBuilder::with_version(version)
        .add_source(text, "file:///schema_error_determinism.xsd")
        .and_then(|b| b.compile());
    match result {
        Ok(_) => panic!("{version:?}: schema accepted"),
        Err(e) => e.to_string(),
    }
}

fn same_error_every_time(version: XsdVersion, text: &str, expected: &str) {
    let first = error(version, text);
    assert!(
        first.contains(expected),
        "{version:?}: expected an error containing {expected:?}, got {first}"
    );
    for _ in 0..40 {
        assert_eq!(
            error(version, text),
            first,
            "{version:?}: the error changed"
        );
    }
}

/// Two independent invalid simple types (ibmData `d3_4_27si03`'s shape, on
/// `xs:decimal` so that it holds in both versions): the first one in the
/// document is reported.
#[test]
fn the_first_invalid_type_in_document_order_is_reported() {
    let text = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
      <xs:simpleType name="first">
        <xs:restriction base="xs:decimal">
          <xs:minInclusive value="3"/><xs:maxInclusive value="2"/>
        </xs:restriction>
      </xs:simpleType>
      <xs:simpleType name="second">
        <xs:restriction base="xs:decimal">
          <xs:minExclusive value="x"/><xs:maxExclusive value="2"/>
        </xs:restriction>
      </xs:simpleType>
      <xs:simpleType name="third">
        <xs:restriction base="xs:decimal"><xs:maxInclusive value="y"/></xs:restriction>
      </xs:simpleType>
    </xs:schema>"#;
    for version in [XsdVersion::V1_0, XsdVersion::V1_1] {
        same_error_every_time(version, text, "'first'");
    }
}

/// An enumeration with several values outside the base type: the smallest
/// (sorted) is named.
#[test]
fn the_same_invalid_enumeration_value_is_named() {
    let text = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
      <xs:simpleType name="e">
        <xs:restriction base="xs:integer">
          <xs:enumeration value="1"/>
          <xs:enumeration value="zeta"/>
          <xs:enumeration value="beta"/>
          <xs:enumeration value="2"/>
          <xs:enumeration value="alpha"/>
          <xs:enumeration value="gamma"/>
        </xs:restriction>
      </xs:simpleType>
    </xs:schema>"#;
    for version in [XsdVersion::V1_0, XsdVersion::V1_1] {
        same_error_every_time(version, text, "'alpha'");
    }
}

/// A cycle among simple types is described along the same path every time.
#[test]
fn a_type_cycle_is_described_the_same_way() {
    let text = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
      <xs:simpleType name="a"><xs:restriction base="b"/></xs:simpleType>
      <xs:simpleType name="b"><xs:restriction base="c"/></xs:simpleType>
      <xs:simpleType name="c"><xs:restriction base="a"/></xs:simpleType>
      <xs:simpleType name="d"><xs:restriction base="e"/></xs:simpleType>
      <xs:simpleType name="e"><xs:restriction base="d"/></xs:simpleType>
    </xs:schema>"#;
    for version in [XsdVersion::V1_0, XsdVersion::V1_1] {
        let first = error(version, text);
        for _ in 0..40 {
            assert_eq!(
                error(version, text),
                first,
                "{version:?}: the error changed"
            );
        }
    }
}
