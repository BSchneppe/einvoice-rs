//! The public facet-derivation and wildcard helpers.
//!
//! `FacetSet::merge_with_base` checks a restriction's facets against its
//! base's in the value space of the datatype it is given — the check the
//! schema compiler makes (Datatypes 1.1 §4.3, "according to the datatype's
//! order relation"; a `fixed` facet "cannot specify a value … other than
//! {value}"; §4.3.5.5: an enumeration value must be in the base's value
//! space). `types::complex::other_matches_namespace` is the `##other` rule,
//! the same in both XSD versions (Structures §3.10.4).

use xsd_schema::types::complex::other_matches_namespace;
use xsd_schema::types::facets::{FacetFixed, FacetSet};
use xsd_schema::{SchemaSetBuilder, XmlTypeCode, XsdVersion};

fn bounds(facets: &[(&str, &str)]) -> FacetSet {
    let mut set = FacetSet::new();
    for (facet, value) in facets {
        let value = (*value).to_string();
        match *facet {
            "minInclusive" => set.set_min_inclusive(value, FacetFixed::Default, None),
            "maxInclusive" => set.set_max_inclusive(value, FacetFixed::Default, None),
            "minExclusive" => set.set_min_exclusive(value, FacetFixed::Default, None),
            "maxExclusive" => set.set_max_exclusive(value, FacetFixed::Default, None),
            "fixedMaxInclusive" => set.set_max_inclusive(value, FacetFixed::Fixed, None),
            "fixedMinExclusive" => set.set_min_exclusive(value, FacetFixed::Fixed, None),
            other => panic!("{other}"),
        }
    }
    set
}

fn enumeration(values: &[&str]) -> FacetSet {
    let mut set = FacetSet::new();
    for value in values {
        set.add_enumeration((*value).to_string(), None);
    }
    set
}

fn merges(derived: &FacetSet, base: &FacetSet, datatype: XmlTypeCode) -> bool {
    derived.merge_with_base(base, datatype).is_ok()
}

#[test]
fn merge_with_base_compares_bounds_in_the_value_space_of_the_datatype() {
    let none = FacetSet::new();
    // A fixed bound: the same value in another spelling is not another value.
    let fixed10 = bounds(&[("fixedMaxInclusive", "10")]);
    for (derived, datatype, ok) in [
        (" 10 ", XmlTypeCode::Decimal, true),
        ("10.0", XmlTypeCode::Decimal, true),
        ("+10", XmlTypeCode::Integer, true),
        ("9", XmlTypeCode::Decimal, false),
        ("1.0E1", XmlTypeCode::Double, true),
    ] {
        assert_eq!(
            merges(&bounds(&[("maxInclusive", derived)]), &fixed10, datatype),
            ok,
            "fixed 10, derived {derived:?} as {datatype:?}"
        );
    }
    let fixed_date = bounds(&[("fixedMinExclusive", "2000-01-01Z")]);
    assert!(merges(
        &bounds(&[("minExclusive", "2000-01-01+00:00")]),
        &fixed_date,
        XmlTypeCode::Date
    ));
    assert!(!merges(
        &bounds(&[("minExclusive", "2000-01-01")]),
        &fixed_date,
        XmlTypeCode::Date
    ));
    // minExclusive may equal maxExclusive; the other pairs as before.
    for (facets, datatype, ok) in [
        (
            vec![("minExclusive", "5"), ("maxExclusive", "5")],
            XmlTypeCode::Integer,
            true,
        ),
        (
            vec![("minExclusive", "5"), ("maxExclusive", "4")],
            XmlTypeCode::Integer,
            false,
        ),
        (
            vec![("minInclusive", "5"), ("maxExclusive", "5")],
            XmlTypeCode::Integer,
            false,
        ),
        (
            vec![("minExclusive", "5"), ("maxInclusive", "5")],
            XmlTypeCode::Integer,
            false,
        ),
        (
            vec![("minInclusive", "5"), ("maxInclusive", "5")],
            XmlTypeCode::Integer,
            true,
        ),
        // The same double, spelled two ways; different decimals.
        (
            vec![
                ("minInclusive", "0.10000000000000000001"),
                ("maxInclusive", "0.1"),
            ],
            XmlTypeCode::Double,
            true,
        ),
        (
            vec![
                ("minInclusive", "0.10000000000000000001"),
                ("maxInclusive", "0.1"),
            ],
            XmlTypeCode::Decimal,
            false,
        ),
        (
            vec![
                ("minInclusive", "1.00000000000000000000000000002"),
                ("maxInclusive", "1.00000000000000000000000000001"),
            ],
            XmlTypeCode::Decimal,
            false,
        ),
        (
            vec![("minInclusive", "-INF"), ("maxInclusive", "INF")],
            XmlTypeCode::Float,
            true,
        ),
        (
            vec![("minInclusive", "INF"), ("maxInclusive", "-INF")],
            XmlTypeCode::Float,
            false,
        ),
        // Date/time and duration bounds in their partial orders.
        (
            vec![
                ("minInclusive", "2000-01-01T05:00:00"),
                ("maxInclusive", "2000-01-01T00:00:00Z"),
            ],
            XmlTypeCode::DateTime,
            true,
        ),
        (
            vec![
                ("minInclusive", "2000-01-02T00:00:00"),
                ("maxInclusive", "2000-01-01T00:00:00Z"),
            ],
            XmlTypeCode::DateTime,
            false,
        ),
        (
            vec![("minInclusive", "P1M"), ("maxInclusive", "P30D")],
            XmlTypeCode::Duration,
            true,
        ),
        (
            vec![("minInclusive", "P1M"), ("maxInclusive", "P27D")],
            XmlTypeCode::Duration,
            false,
        ),
        (
            vec![("minInclusive", "---02"), ("maxInclusive", "---01-12:00")],
            XmlTypeCode::GDay,
            true,
        ),
    ] {
        assert_eq!(
            merges(&bounds(&facets), &none, datatype),
            ok,
            "{facets:?} as {datatype:?}"
        );
    }
}

#[test]
fn merge_with_base_compares_enumeration_values() {
    for (base, derived, datatype, ok) in [
        (vec!["1", "5"], "01", XmlTypeCode::Integer, true),
        (vec!["1", "5"], "2", XmlTypeCode::Integer, false),
        (vec!["1.0"], "1", XmlTypeCode::Decimal, true),
        (vec!["0"], "-0", XmlTypeCode::Double, true),
        (vec!["true"], "1", XmlTypeCode::Boolean, true),
        (vec!["0A"], "0a", XmlTypeCode::HexBinary, true),
        (
            vec!["2000-01-01Z"],
            "2000-01-01+00:00",
            XmlTypeCode::Date,
            true,
        ),
        (vec!["2000-01-01Z"], "2000-01-01", XmlTypeCode::Date, false),
        (vec!["P1D"], "PT24H", XmlTypeCode::Duration, true),
        (vec!["---02-14:00"], "---03+10:00", XmlTypeCode::GDay, true),
        (vec!["a b"], "a  b", XmlTypeCode::NmTokens, true),
        (vec!["red"], "red", XmlTypeCode::String, true),
        (vec!["red"], " red", XmlTypeCode::String, false),
        // Without a datatype the values are compared as written.
        (vec!["1"], "01", XmlTypeCode::AnySimpleType, false),
        (vec!["1"], "1", XmlTypeCode::AnySimpleType, true),
    ] {
        assert_eq!(
            merges(&enumeration(&[derived]), &enumeration(&base), datatype),
            ok,
            "{derived:?} under {base:?} as {datatype:?}"
        );
    }
}

/// `##other` admits every namespace but the target namespace and ·absent·,
/// in both versions.
#[test]
fn other_matches_namespace_is_version_free() {
    let set = SchemaSetBuilder::with_version(XsdVersion::V1_0)
        .add_source(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:t"/>"#,
            "file:///value_space_api.xsd",
        )
        .expect("schema source accepted")
        .compile()
        .expect("schema compiles")
        .into_schema_set();
    let target = Some(set.name_table.add("urn:t"));
    let other = Some(set.name_table.add("urn:o"));
    assert!(other_matches_namespace(other, target));
    assert!(!other_matches_namespace(target, target));
    assert!(!other_matches_namespace(None, target));
    assert!(other_matches_namespace(other, None));
    assert!(!other_matches_namespace(None, None));
}

/// The value structs' `PartialOrd` is the XSD order and their `PartialEq` its
/// equality (decision 3): the seven-property date/time types on the time line,
/// ·incomparable· (`None`) when exactly one value has a time zone offset and
/// the ±14:00 readings disagree (Datatypes 1.1 §D.2.1; XSD 1.0 §3.2.7.4);
/// `xs:duration` by the four reference dateTimes (§3.3.6.1; XSD 1.0
/// §3.2.6.2); `Equal` exactly when `==`.
#[test]
fn the_value_structs_order_and_equality_are_the_xsd_ones() {
    use std::cmp::Ordering::{Equal, Greater, Less};
    use xsd_schema::types::validators::VALIDATOR_REGISTRY;
    use xsd_schema::types::value::{XmlAtomicValue, XmlValueKind};

    fn atom(code: XmlTypeCode, lexical: &str) -> XmlAtomicValue {
        match VALIDATOR_REGISTRY
            .validate(code, lexical)
            .expect(lexical)
            .value
        {
            XmlValueKind::Atomic(atom) => atom,
            other => panic!("{other:?}"),
        }
    }
    fn cmp(code: XmlTypeCode, a: &str, b: &str) -> (Option<std::cmp::Ordering>, bool) {
        use XmlAtomicValue as V;
        match (atom(code, a), atom(code, b)) {
            (V::DateTime(x), V::DateTime(y)) => (x.partial_cmp(&y), x == y),
            (V::Date(x), V::Date(y)) => (x.partial_cmp(&y), x == y),
            (V::Time(x), V::Time(y)) => (x.partial_cmp(&y), x == y),
            (V::GDay(x), V::GDay(y)) => (x.partial_cmp(&y), x == y),
            (V::GYear(x), V::GYear(y)) => (x.partial_cmp(&y), x == y),
            (V::Duration(x), V::Duration(y)) => (x.partial_cmp(&y), x == y),
            (V::YearMonthDuration(x), V::YearMonthDuration(y)) => (x.partial_cmp(&y), x == y),
            (V::DayTimeDuration(x), V::DayTimeDuration(y)) => (x.partial_cmp(&y), x == y),
            other => panic!("{other:?}"),
        }
    }
    for (code, a, b, order) in [
        (
            XmlTypeCode::DateTime,
            "2000-01-01T12:00:00Z",
            "2000-01-01T13:00:00+01:00",
            Some(Equal),
        ),
        (
            XmlTypeCode::DateTime,
            "2000-01-01T12:00:00",
            "2000-01-01T12:00:00Z",
            None,
        ),
        (
            XmlTypeCode::DateTime,
            "2000-01-01T12:00:00",
            "2000-01-02T03:00:00Z",
            Some(Less),
        ),
        (
            XmlTypeCode::DateTime,
            "2000-01-01T12:00:00",
            "2000-01-01T12:00:00",
            Some(Equal),
        ),
        (
            XmlTypeCode::Date,
            "2000-01-02Z",
            "2000-01-01Z",
            Some(Greater),
        ),
        (
            XmlTypeCode::Time,
            "12:00:00Z",
            "13:00:00+01:00",
            Some(Equal),
        ),
        (XmlTypeCode::GDay, "---02-14:00", "---03+10:00", Some(Equal)),
        (XmlTypeCode::GDay, "---02", "---02Z", None),
        (XmlTypeCode::GYear, "2000", "2001", Some(Less)),
        (XmlTypeCode::Duration, "P1M", "P30D", None),
        (XmlTypeCode::Duration, "P1M", "P27D", Some(Greater)),
        (XmlTypeCode::Duration, "P1D", "PT24H", Some(Equal)),
        (XmlTypeCode::Duration, "P400Y", "P146097D", Some(Equal)),
        (XmlTypeCode::Duration, "P1Y", "P12M", Some(Equal)),
        (XmlTypeCode::YearMonthDuration, "P12M", "P1Y", Some(Equal)),
        (XmlTypeCode::DayTimeDuration, "PT24H", "P1D", Some(Equal)),
    ] {
        let (got, equal) = cmp(code, a, b);
        assert_eq!(got, order, "{a} vs {b}");
        assert_eq!(equal, order == Some(Equal), "{a} == {b}");
        let (back, _) = cmp(code, b, a);
        assert_eq!(back, order.map(std::cmp::Ordering::reverse), "{b} vs {a}");
    }
}

/// `NamespaceConstraint::matches` / `wildcard::NamespaceConstraint::allows`
/// take no XSD version (decision 5): no constraint kind depends on it.
#[test]
fn namespace_constraints_take_no_version() {
    use xsd_schema::schema::wildcard::NamespaceConstraint as WildcardConstraint;
    use xsd_schema::types::complex::NamespaceConstraint;
    let set = SchemaSetBuilder::with_version(XsdVersion::V1_0)
        .add_source(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:t"/>"#,
            "file:///value_space_api.xsd",
        )
        .expect("schema source accepted")
        .compile()
        .expect("schema compiles")
        .into_schema_set();
    let target = Some(set.name_table.add("urn:t"));
    let other = Some(set.name_table.add("urn:o"));
    assert!(NamespaceConstraint::Other.matches(other, target));
    assert!(!NamespaceConstraint::Other.matches(None, target));
    assert!(NamespaceConstraint::Local.matches(None, target));
    assert!(WildcardConstraint::Other.allows(other, target));
    assert!(!WildcardConstraint::Other.allows(target, target));
}
