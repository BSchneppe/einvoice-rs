//! Values and facets in the value space, end to end: the instance verdicts of
//! every driver, in both XSD versions, and the schema-time checks that compare
//! facet values.
//!
//! - `-0` is a lexical form of zero for the unsigned integer types (Datatypes
//!   1.1 §3.4.21.1 – §3.4.24.1).
//! - The bounding facets: `minExclusive` may equal `maxExclusive` (§4.3.8.4),
//!   and date/time bounds that are ·incomparable· are not out of order
//!   (§D.2.1, XSD 1.0 §3.2.7.4).
//! - Durations are partially ordered (§3.3.6.1, XSD 1.0 §3.2.6.2 / §3.2.6.3).
//! - The `g*` types are equal and ordered on the time line (§D.2.1), for
//!   `fixed`, identity constraints and unions as for `enumeration` and the
//!   bounds; their `pattern` constrains the lexical form (§4.3.4).
//! - Facet derivation compares values: a `fixed` bound, the enumeration
//!   subset, a simple-content restriction's facets against its base content
//!   type; a `whiteSpace` value outside its three keywords is an error.
//! - A QName value whose prefix is bound nowhere is not valid, on every path
//!   (§3.3.18).
//!
//! Every instance row runs on streaming validation with and without PSVI
//! typed values, on the navigator driver and (with `xsd11`) through the typed
//! document build.

#[cfg(feature = "xsd11")]
use bumpalo::Bump;
#[cfg(feature = "xsd11")]
use xsd_schema::document::{build_typed_document_with, BufferDocumentOptions, TypedBuildOptions};
#[cfg(feature = "xsd11")]
use xsd_schema::navigator::{DomNavigator, TypedValue};
#[cfg(feature = "xsd11")]
use xsd_schema::validation::NoopHandler;
use xsd_schema::validation::{
    drive_navigator, drive_quick_xml, CollectingValidationSink, SchemaValidator, SchemaValidity,
    ValidationFlags,
};
use xsd_schema::{RoXmlNavigator, SchemaError, SchemaSet, SchemaSetBuilder, XsdVersion};

use SchemaValidity::{Invalid, Valid};

const VERSIONS: [XsdVersion; 2] = [XsdVersion::V1_0, XsdVersion::V1_1];

#[cfg(feature = "xsd11")]
const DRIVERS: [&str; 4] = ["stream", "stream/no-psvi", "navigator", "typed-build"];
#[cfg(not(feature = "xsd11"))]
const DRIVERS: [&str; 3] = ["stream", "stream/no-psvi", "navigator"];

const DATATYPE: Option<&str> = Some("cvc-datatype-valid");
const UNBOUND: Option<&str> = Some("cvc-datatype-valid.1.2.1");
const ENUM: Option<&str> = Some("cvc-enumeration-valid");
const PATTERN: Option<&str> = Some("cvc-pattern-valid");
const FIXED: Option<&str> = Some("cvc-elt.5.2.2");
const MIN_IN: Option<&str> = Some("cvc-minInclusive-valid");
const MAX_IN: Option<&str> = Some("cvc-maxInclusive-valid");
const MIN_EX: Option<&str> = Some("cvc-minExclusive-valid");
const MAX_EX: Option<&str> = Some("cvc-maxExclusive-valid");
const DUPLICATE: Option<&str> = Some("cvc-identity-constraint.4.2.2");

// ── Harness ───────────────────────────────────────────────────────────────

fn xsd(body: &str) -> String {
    format!(
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:t="urn:t" targetNamespace="urn:t">{body}</xs:schema>"#
    )
}

/// The schema set of `body`; an error the parser reports (a facet value
/// outside its lexical space) or the compiler reports.
fn compile(version: XsdVersion, body: &str) -> Result<SchemaSet, SchemaError> {
    SchemaSetBuilder::with_version(version)
        .add_source(&xsd(body), "file:///value_space_facets.xsd")?
        .compile()
        .map(|compiled| compiled.into_schema_set())
}

fn schema(version: XsdVersion, body: &str) -> SchemaSet {
    compile(version, body).unwrap_or_else(|e| panic!("{version:?}: schema compiles: {e}"))
}

/// The error compiling `body` reports — its constraint, or its message when
/// it names none — or `None` when it compiles.
fn schema_error(version: XsdVersion, body: &str) -> Option<String> {
    match compile(version, body) {
        Ok(_) => None,
        Err(SchemaError::StructuralError { constraint, .. }) => Some(constraint.to_string()),
        Err(error) => Some(error.to_string()),
    }
}

/// Root validity and the error constraint codes of one run on `driver`.
fn run(ss: &SchemaSet, xml: &str, driver: &str) -> (Option<SchemaValidity>, Vec<String>) {
    let flags = match driver {
        "stream/no-psvi" => ValidationFlags::default() & !ValidationFlags::BUILD_PSVI_TYPED_VALUES,
        _ => ValidationFlags::default(),
    };
    let v = SchemaValidator::new(ss, flags);
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let validity = {
        let mut rt = v.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        match driver {
            "navigator" => {
                let doc = roxmltree::Document::parse(xml).expect("well-formed instance");
                let nav = RoXmlNavigator::new(&doc);
                drive_navigator(&nav, &mut rt, ss)
                    .expect("navigator drive completes")
                    .root_validity
            }
            #[cfg(feature = "xsd11")]
            "typed-build" => {
                let arena = Bump::new();
                build_typed_document_with(
                    xml.as_bytes(),
                    &arena,
                    ss,
                    &mut rt,
                    &mut NoopHandler,
                    TypedBuildOptions::default()
                        .with_document_options(BufferDocumentOptions::full()),
                )
                .expect("the typed build completes")
                .outcome
                .root_validity
            }
            _ => {
                drive_quick_xml(xml.as_bytes(), &mut rt, ss)
                    .expect("stream drive completes")
                    .root_validity
            }
        }
    };
    (
        validity,
        errors.iter().map(|e| e.constraint.to_string()).collect(),
    )
}

/// Every `(instance, validity, error code)` row on every driver; mismatches
/// go to `failures`. With a code, some error starts with it; without, there
/// is no error at all.
fn check_rows(
    failures: &mut Vec<String>,
    label: &str,
    ss: &SchemaSet,
    rows: &[(&str, SchemaValidity, Option<&str>)],
) {
    for (xml, expected, code) in rows {
        for driver in DRIVERS {
            let (validity, errors) = run(ss, xml, driver);
            let ok = validity == Some(*expected)
                && match code {
                    Some(code) => errors.iter().any(|e| e.starts_with(code)),
                    None => errors.is_empty(),
                };
            if !ok {
                failures.push(format!(
                    "{label} {driver}: {xml} => {validity:?} {errors:?} (expected {expected:?} {code:?})"
                ));
            }
        }
    }
}

/// Every `(what, schema body, compiles)` row in `version`.
fn check_schemas(failures: &mut Vec<String>, version: XsdVersion, rows: &[(&str, String, bool)]) {
    for (what, body, compiles) in rows {
        let error = schema_error(version, body);
        if error.is_none() != *compiles {
            failures.push(format!(
                "{version:?}: {what}: {error:?} (expected to {})",
                if *compiles { "compile" } else { "fail" }
            ));
        }
    }
}

fn assert_no_failures(failures: &[String]) {
    assert!(
        failures.is_empty(),
        "{} wrong:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// A wrapper element `r` holding any sequence of the global elements.
const ROOT: &str = r#"<xs:element name="r"><xs:complexType><xs:sequence>
    <xs:any processContents="strict" minOccurs="0" maxOccurs="unbounded"/>
  </xs:sequence></xs:complexType></xs:element>"#;

fn doc(inner: &str) -> String {
    format!(r#"<t:r xmlns:t="urn:t">{inner}</t:r>"#)
}

/// A restriction of `base` named `name` with `facets`.
fn st(name: &str, base: &str, facets: &str) -> String {
    format!(
        r#"<xs:simpleType name="{name}"><xs:restriction base="{base}">{facets}</xs:restriction></xs:simpleType>"#
    )
}

/// One global element per type, named after it.
fn elements(types: &[&str]) -> String {
    types
        .iter()
        .map(|t| format!(r#"<xs:element name="{t}" type="t:{t}"/>"#))
        .collect()
}

// ── C27: `-0` for the unsigned integer types ─────────────────────────────

/// Datatypes 1.1 §3.4.21.1 (unsignedLong; §3.4.22.1 – §3.4.24.1 alike): "If
/// the sign is present, it must be '+' except for lexical forms denoting
/// zero, which may be preceded by a positive ('+') or a negative ('-')
/// sign". XSD 1.0 §2.4.2: "Constraining the ·value space· consequently
/// constrains the ·lexical space·" — the unsigned types restrict
/// `nonNegativeInteger` (§3.3.20.1: zero "may be preceded by a positive
/// ("+") or a negative ("-") sign") by `maxInclusive`.
#[test]
fn minus_zero_is_zero_for_the_unsigned_types() {
    let body = format!(
        r#"{ROOT}
      <xs:element name="uL" type="xs:unsignedLong"/>
      <xs:element name="uI" type="xs:unsignedInt"/>
      <xs:element name="uS" type="xs:unsignedShort"/>
      <xs:element name="uB" type="xs:unsignedByte"/>
      <xs:element name="nn" type="xs:nonNegativeInteger"/>
      <xs:element name="pos" type="xs:positiveInteger"/>
      {small}{zero}
      <xs:element name="fixedZero" type="xs:unsignedInt" fixed="0"/>
      <xs:element name="a"><xs:complexType>
        <xs:attribute name="l" type="xs:unsignedLong"/><xs:attribute name="i" type="xs:unsignedInt"/>
        <xs:attribute name="s" type="xs:unsignedShort"/><xs:attribute name="b" type="xs:unsignedByte"/>
        <xs:attribute name="f" type="xs:unsignedByte" fixed="0"/>
      </xs:complexType></xs:element>
      {els}"#,
        small = st(
            "small",
            "xs:unsignedByte",
            r#"<xs:maxInclusive value="5"/>"#
        ),
        zero = st("zero", "xs:unsignedShort", r#"<xs:enumeration value="0"/>"#),
        els = elements(&["small", "zero"]),
    );
    let mut failures = Vec::new();
    for version in VERSIONS {
        let ss = schema(version, &body);
        let label = format!("{version:?}");
        check_rows(
            &mut failures,
            &label,
            &ss,
            &[
                (
                    &doc("<t:uL>-0</t:uL><t:uI>-0</t:uI><t:uS>-0</t:uS><t:uB>-0</t:uB>"),
                    Valid,
                    None,
                ),
                (
                    &doc("<t:uL>-00</t:uL><t:uI> -000 </t:uI><t:uS>+0</t:uS><t:uB>0</t:uB>"),
                    Valid,
                    None,
                ),
                (
                    &doc("<t:small>-0</t:small><t:zero>-0</t:zero><t:fixedZero>-0</t:fixedZero>"),
                    Valid,
                    None,
                ),
                (
                    &doc(r#"<t:a l="-0" i="-00" s="-0" b="-0" f="-0"/>"#),
                    Valid,
                    None,
                ),
                (&doc("<t:nn>-0</t:nn>"), Valid, None),
                (&doc("<t:uL>-1</t:uL>"), Invalid, DATATYPE),
                (&doc("<t:uI>-01</t:uI>"), Invalid, DATATYPE),
                (&doc("<t:uB>-</t:uB>"), Invalid, DATATYPE),
                (&doc("<t:uS>--0</t:uS>"), Invalid, DATATYPE),
                (&doc("<t:uB>-0.0</t:uB>"), Invalid, DATATYPE),
                (&doc(r#"<t:a b="-1"/>"#), Invalid, DATATYPE),
                (&doc("<t:pos>-0</t:pos>"), Invalid, DATATYPE),
            ],
        );
        // The schema uses `-0` too: a default, an enumeration value, a bound.
        check_schemas(
            &mut failures,
            version,
            &[
                (
                    "default -0",
                    r#"<xs:element name="d" type="xs:unsignedInt" default="-0"/>"#.to_string(),
                    true,
                ),
                (
                    "enumeration -0",
                    st("e", "xs:unsignedByte", r#"<xs:enumeration value="-0"/>"#),
                    true,
                ),
                (
                    "maxInclusive -0",
                    st("m", "xs:unsignedLong", r#"<xs:maxInclusive value="-0"/>"#),
                    true,
                ),
                (
                    "default -1",
                    r#"<xs:element name="d" type="xs:unsignedInt" default="-1"/>"#.to_string(),
                    false,
                ),
            ],
        );
    }
    assert_no_failures(&failures);
}

/// The typed value of `-0` is the integer zero, through the typed build.
#[cfg(feature = "xsd11")]
#[test]
fn the_typed_value_of_minus_zero_is_zero() {
    for version in VERSIONS {
        let ss = schema(version, r#"<xs:element name="u" type="xs:unsignedInt"/>"#);
        let v = SchemaValidator::new(&ss, ValidationFlags::default());
        let mut rt = v.start_run(xsd_schema::document::SilentValidationSink);
        let arena = Bump::new();
        let built = build_typed_document_with(
            r#"<t:u xmlns:t="urn:t">-0</t:u>"#.as_bytes(),
            &arena,
            &ss,
            &mut rt,
            &mut NoopHandler,
            TypedBuildOptions::default().with_document_options(BufferDocumentOptions::full()),
        )
        .expect("the typed build completes");
        assert_eq!(built.outcome.root_validity, Some(Valid), "{version:?}");
        let mut nav = built.document.create_navigator();
        assert!(nav.move_to_first_child());
        match nav.typed_value() {
            TypedValue::Value(value) => {
                assert_eq!(value.to_string_value(), "0", "{version:?}");
                assert!(value.as_integer().is_some(), "{version:?}: {value:?}");
            }
            other => panic!("{version:?}: {other:?}"),
        }
    }
}

// ── C28: an exclusive range whose two bounds are equal ───────────────────

/// Datatypes 1.1 §4.3.8.4 (XSD 1.0 §4.3.8.4 word for word), "minExclusive <=
/// maxExclusive": "It is an ·error· for the value specified for
/// ·minExclusive· to be greater than the value specified for ·maxExclusive·
/// for the same datatype" — equal is allowed (the value space is empty). The
/// other three pairs stay strict (§4.3.9.4 "minExclusive < maxInclusive",
/// §4.3.10.4 "minInclusive < maxExclusive") or `<=` (§4.3.7.4), and a
/// derived `maxExclusive` equal to its base's `minExclusive` is still an
/// error (§4.3.8.4 maxExclusive valid restriction, clause 4: "less than or
/// equal to the {value} of that minExclusive").
#[test]
fn equal_exclusive_bounds_are_allowed() {
    let pair = |base: &str, lo: &str, hi: &str| {
        st(
            "t",
            base,
            &format!(r#"<xs:minExclusive value="{lo}"/><xs:maxExclusive value="{hi}"/>"#),
        )
    };
    let mut failures = Vec::new();
    for version in VERSIONS {
        check_schemas(
            &mut failures,
            version,
            &[
                ("integer 5..5", pair("xs:integer", "5", "5"), true),
                ("decimal 1.5..1.50", pair("xs:decimal", "1.5", "1.50"), true),
                ("double 1..1.0E0", pair("xs:double", "1", "1.0E0"), true),
                ("float 1..1.0", pair("xs:float", "1", "1.0"), true),
                ("unsignedByte 7..7", pair("xs:unsignedByte", "7", "7"), true),
                (
                    "date 2000-01-01Z twice",
                    pair("xs:date", "2000-01-01Z", "2000-01-01Z"),
                    true,
                ),
                (
                    "dateTime one instant, two offsets",
                    pair(
                        "xs:dateTime",
                        "2000-01-01T05:00:00+05:00",
                        "2000-01-01T00:00:00Z",
                    ),
                    true,
                ),
                (
                    "duration P1D..PT24H",
                    pair("xs:duration", "P1D", "PT24H"),
                    true,
                ),
                ("gYear 2000..2000", pair("xs:gYear", "2000", "2000"), true),
                ("integer 5..4", pair("xs:integer", "5", "4"), false),
                (
                    "date 2000-01-02Z..2000-01-01Z",
                    pair("xs:date", "2000-01-02Z", "2000-01-01Z"),
                    false,
                ),
                (
                    "minInclusive 5, maxExclusive 5",
                    st(
                        "t",
                        "xs:integer",
                        r#"<xs:minInclusive value="5"/><xs:maxExclusive value="5"/>"#,
                    ),
                    false,
                ),
                (
                    "minExclusive 5, maxInclusive 5",
                    st(
                        "t",
                        "xs:integer",
                        r#"<xs:minExclusive value="5"/><xs:maxInclusive value="5"/>"#,
                    ),
                    false,
                ),
                (
                    "minInclusive 5, maxInclusive 5",
                    st(
                        "t",
                        "xs:integer",
                        r#"<xs:minInclusive value="5"/><xs:maxInclusive value="5"/>"#,
                    ),
                    true,
                ),
                (
                    "maxExclusive 5 under a base minExclusive 5",
                    format!(
                        "{}{}",
                        st("b", "xs:integer", r#"<xs:minExclusive value="5"/>"#),
                        st("t", "t:b", r#"<xs:maxExclusive value="5"/>"#)
                    ),
                    false,
                ),
                (
                    "maxExclusive 5 with minExclusive 5 under a base minExclusive 4",
                    format!(
                        "{}{}",
                        st("b", "xs:integer", r#"<xs:minExclusive value="4"/>"#),
                        st(
                            "t",
                            "t:b",
                            r#"<xs:minExclusive value="5"/><xs:maxExclusive value="5"/>"#
                        )
                    ),
                    true,
                ),
            ],
        );
        // The empty value space admits nothing.
        let ss = schema(
            version,
            &format!("{ROOT}{}{}", pair("xs:integer", "5", "5"), elements(&["t"])),
        );
        check_rows(
            &mut failures,
            &format!("{version:?}"),
            &ss,
            &[
                (&doc("<t:t>5</t:t>"), Invalid, MIN_EX),
                (&doc("<t:t>6</t:t>"), Invalid, MAX_EX),
            ],
        );
    }
    assert_no_failures(&failures);
}

// ── C49: date/time bounds that are incomparable ──────────────────────────

/// Datatypes 1.1 §D.2.1: values "are ordered the same as their
/// ·timeOnTimeline· values, except that if one value's ·timezoneOffset· is
/// absent and the other's is not, and using maximum and minimum
/// ·timezoneOffset· values for the one whose ·timezoneOffset· is actually
/// absent changes the resulting (strict) inequality, the original two values
/// are incomparable"; XSD 1.0 §3.2.7.4: "P <> Q otherwise". §4.3.7.4 forbids a
/// minimum "greater than" the maximum: an incomparable pair is not.
#[test]
fn incomparable_date_time_bounds_are_not_out_of_order() {
    let bounds = |base: &str, lo: (&str, &str), hi: (&str, &str)| {
        st(
            "t",
            base,
            &format!(
                r#"<xs:{} value="{}"/><xs:{} value="{}"/>"#,
                lo.0, lo.1, hi.0, hi.1
            ),
        )
    };
    let mut failures = Vec::new();
    for version in VERSIONS {
        check_schemas(
            &mut failures,
            version,
            &[
                (
                    "dateTime local 05:00 .. 00:00Z",
                    bounds(
                        "xs:dateTime",
                        ("minInclusive", "2000-01-01T05:00:00"),
                        ("maxInclusive", "2000-01-01T00:00:00Z"),
                    ),
                    true,
                ),
                (
                    "dateTime 10:00Z .. local 2000-01-01T00:00 (exclusive bounds)",
                    bounds(
                        "xs:dateTime",
                        ("minExclusive", "2000-01-01T10:00:00Z"),
                        ("maxExclusive", "2000-01-01T00:00:00"),
                    ),
                    true,
                ),
                (
                    "time local 05:00 .. 00:00Z",
                    bounds(
                        "xs:time",
                        ("minInclusive", "05:00:00"),
                        ("maxInclusive", "00:00:00Z"),
                    ),
                    true,
                ),
                (
                    "date local 2000-01-02 .. 2000-01-02-10:00 (exclusive bound)",
                    bounds(
                        "xs:date",
                        ("minInclusive", "2000-01-02"),
                        ("maxExclusive", "2000-01-02-10:00"),
                    ),
                    true,
                ),
                (
                    "gDay local ---02 .. ---01-12:00",
                    bounds(
                        "xs:gDay",
                        ("minInclusive", "---02"),
                        ("maxInclusive", "---01-12:00"),
                    ),
                    true,
                ),
                (
                    "gYearMonth local 2000-02 .. 2000-01-12:00 is greater",
                    bounds(
                        "xs:gYearMonth",
                        ("minInclusive", "2000-02"),
                        ("maxInclusive", "2000-01-12:00"),
                    ),
                    false,
                ),
                (
                    "gMonthDay local --01-02 .. --01-01-12:00",
                    bounds(
                        "xs:gMonthDay",
                        ("minExclusive", "--01-02"),
                        ("maxInclusive", "--01-01-12:00"),
                    ),
                    true,
                ),
                (
                    "dateTime local 2000-01-02T00:00 .. 2000-01-01T00:00Z is greater",
                    bounds(
                        "xs:dateTime",
                        ("minInclusive", "2000-01-02T00:00:00"),
                        ("maxInclusive", "2000-01-01T00:00:00Z"),
                    ),
                    false,
                ),
                (
                    "time 10:00Z .. 09:00Z is greater",
                    bounds(
                        "xs:time",
                        ("minInclusive", "10:00:00Z"),
                        ("maxInclusive", "09:00:00Z"),
                    ),
                    false,
                ),
            ],
        );
    }
    assert_no_failures(&failures);
}

// ── C35: durations are partially ordered ─────────────────────────────────

/// Datatypes 1.1 §3.3.6.1: "the equality or order of two duration values is
/// determined by adding each duration in the pair to each of the following
/// four dateTime values: 1696-09-01T00:00:00Z 1697-02-01T00:00:00Z
/// 1903-03-01T00:00:00Z 1903-07-01T00:00:00Z. If all four resulting dateTime
/// value pairs are ordered the same way (less than, equal, or greater than),
/// then the original pair of duration values is ordered the same way;
/// otherwise the original pair is ·incomparable·." XSD 1.0 §3.2.6.2 uses the
/// same four dateTimes and tabulates "P1M > P27D <> P28D <> P29D <> P30D <>
/// P31D < P32D", "P1Y > P364D <> P365D <> P366D < P367D"; §3.2.6.3: "In
/// comparing duration values with minInclusive, minExclusive, maxInclusive
/// and maxExclusive facet values indeterminate comparisons should be
/// considered as "false"."
#[test]
fn durations_are_partially_ordered() {
    let body = format!(
        "{ROOT}{}{}{}{}{}{}",
        st("max1M", "xs:duration", r#"<xs:maxInclusive value="P1M"/>"#),
        st("min1M", "xs:duration", r#"<xs:minInclusive value="P1M"/>"#),
        st(
            "maxEx1Y",
            "xs:duration",
            r#"<xs:maxExclusive value="P1Y"/>"#
        ),
        st(
            "minEx5M",
            "xs:duration",
            r#"<xs:minExclusive value="P5M"/>"#
        ),
        st("min1Y", "xs:duration", r#"<xs:minInclusive value="P1Y"/>"#),
        elements(&["max1M", "min1M", "maxEx1Y", "minEx5M", "min1Y"]),
    );
    let mut failures = Vec::new();
    for version in VERSIONS {
        let ss = schema(version, &body);
        let label = format!("{version:?}");
        let mut rows = Vec::new();
        for (value, verdict) in [
            ("P27D", Valid),
            ("P28D", Invalid),
            ("P29D", Invalid),
            ("P30D", Invalid),
            ("P31D", Invalid),
            ("P32D", Invalid),
            ("P1M", Valid),
            ("PT0S", Valid),
            ("-P40D", Valid),
            ("P30DT23H", Invalid),
            ("P27DT23H59M59S", Valid),
        ] {
            rows.push((doc(&format!("<t:max1M>{value}</t:max1M>")), verdict, MAX_IN));
        }
        for (value, verdict) in [
            ("P31D", Invalid),
            ("P32D", Valid),
            ("P2M", Valid),
            ("P1M", Valid),
            ("P28D", Invalid),
            ("PT744H", Invalid),
            ("PT768H", Valid),
        ] {
            rows.push((doc(&format!("<t:min1M>{value}</t:min1M>")), verdict, MIN_IN));
        }
        for (value, verdict) in [
            ("P364D", Valid),
            ("P365D", Invalid),
            ("P366D", Invalid),
            ("P11M", Valid),
            ("P12M", Invalid),
        ] {
            rows.push((
                doc(&format!("<t:maxEx1Y>{value}</t:maxEx1Y>")),
                verdict,
                MAX_EX,
            ));
        }
        for (value, verdict) in [("P154D", Valid), ("P153D", Invalid), ("P149D", Invalid)] {
            rows.push((
                doc(&format!("<t:minEx5M>{value}</t:minEx5M>")),
                verdict,
                MIN_EX,
            ));
        }
        for (value, verdict) in [
            ("P367D", Valid),
            ("P366D", Invalid),
            ("P365D", Invalid),
            ("P364D", Invalid),
            ("P12M", Valid),
        ] {
            rows.push((doc(&format!("<t:min1Y>{value}</t:min1Y>")), verdict, MIN_IN));
        }
        let rows: Vec<(&str, SchemaValidity, Option<&str>)> = rows
            .iter()
            .map(|(xml, verdict, code)| {
                (
                    xml.as_str(),
                    *verdict,
                    if *verdict == Valid { None } else { *code },
                )
            })
            .collect();
        check_rows(&mut failures, &label, &ss, &rows);
        check_schemas(
            &mut failures,
            version,
            &[
                (
                    "minInclusive P1M, maxInclusive P30D: incomparable",
                    st(
                        "t",
                        "xs:duration",
                        r#"<xs:minInclusive value="P1M"/><xs:maxInclusive value="P30D"/>"#,
                    ),
                    true,
                ),
                (
                    "minExclusive P1Y, maxExclusive P365D: incomparable",
                    st(
                        "t",
                        "xs:duration",
                        r#"<xs:minExclusive value="P1Y"/><xs:maxExclusive value="P365D"/>"#,
                    ),
                    true,
                ),
                (
                    "minInclusive P1M, maxInclusive P27D: greater",
                    st(
                        "t",
                        "xs:duration",
                        r#"<xs:minInclusive value="P1M"/><xs:maxInclusive value="P27D"/>"#,
                    ),
                    false,
                ),
                (
                    "maxInclusive P30D under maxInclusive P1M: not in the base",
                    format!(
                        "{}{}",
                        st("b", "xs:duration", r#"<xs:maxInclusive value="P1M"/>"#),
                        st("t", "t:b", r#"<xs:maxInclusive value="P30D"/>"#)
                    ),
                    false,
                ),
                (
                    "maxInclusive P27D under maxInclusive P1M",
                    format!(
                        "{}{}",
                        st("b", "xs:duration", r#"<xs:maxInclusive value="P1M"/>"#),
                        st("t", "t:b", r#"<xs:maxInclusive value="P27D"/>"#)
                    ),
                    true,
                ),
            ],
        );
    }
    // The two totally ordered restrictions keep their order (XSD 1.1 only).
    let ss = schema(
        XsdVersion::V1_1,
        &format!(
            "{ROOT}{}{}{}",
            st(
                "ym",
                "xs:yearMonthDuration",
                r#"<xs:maxInclusive value="P1Y"/>"#
            ),
            st(
                "dt",
                "xs:dayTimeDuration",
                r#"<xs:maxInclusive value="P30D"/>"#
            ),
            elements(&["ym", "dt"])
        ),
    );
    check_rows(
        &mut failures,
        "V1_1",
        &ss,
        &[
            (&doc("<t:ym>P12M</t:ym><t:dt>PT720H</t:dt>"), Valid, None),
            (&doc("<t:ym>P13M</t:ym>"), Invalid, MAX_IN),
            (&doc("<t:dt>P30DT1S</t:dt>"), Invalid, MAX_IN),
        ],
    );
    assert_no_failures(&failures);
}

// ── C34: the g* types on the time line ───────────────────────────────────

/// Datatypes 1.1 §3.3.13.1 (gDay): "Equality and order are as prescribed in
/// The Seven-property Model (§D.2.1)" — "---15−11:00 = ---16+13:00"; the
/// other g* types alike (§3.3.9.1 – §3.3.14.1). XSD 1.0 §3.2.13: "the order
/// relation on gDay values is the order relation on their starting instants".
/// A `fixed` value is matched when "the ·actual value· of E is equal or
/// identical to D.{value constraint}.{value}" (Structures §3.3.4.3 clause
/// 5.2.2.2.2) and identity-constraint fields are compared by equality
/// (§3.11.4), so both use the time line the bounds and `enumeration` use.
#[test]
fn g_values_are_equal_on_the_time_line() {
    let body = format!(
        r#"{ROOT}
      <xs:element name="gd" type="xs:gDay" fixed="---02-14:00"/>
      <xs:element name="gmd" type="xs:gMonthDay" fixed="--02-14+12:00"/>
      <xs:element name="gy" type="xs:gYear" fixed="2000Z"/>
      <xs:element name="gym" type="xs:gYearMonth" fixed="2000-01Z"/>
      <xs:element name="gm" type="xs:gMonth" fixed="--05Z"/>
      <xs:element name="dtf" type="xs:dateTime" fixed="2000-01-01T00:00:00Z"/>
      <xs:element name="ga"><xs:complexType>
        <xs:attribute name="d" type="xs:gDay" fixed="---02-14:00"/>
      </xs:complexType></xs:element>
      <xs:element name="keys"><xs:complexType><xs:sequence>
        <xs:element name="k" type="xs:gDay" maxOccurs="unbounded"/>
        <xs:element name="ref" type="xs:gDay" minOccurs="0" maxOccurs="unbounded"/>
      </xs:sequence></xs:complexType>
        <xs:key name="dayKey"><xs:selector xpath="k"/><xs:field xpath="."/></xs:key>
        <xs:keyref name="dayRef" refer="t:dayKey"><xs:selector xpath="ref"/><xs:field xpath="."/></xs:keyref>
      </xs:element>"#
    );
    let mut failures = Vec::new();
    for version in VERSIONS {
        let ss = schema(version, &body);
        let label = format!("{version:?}");
        check_rows(
            &mut failures,
            &label,
            &ss,
            &[
                (&doc("<t:gd>---03+10:00</t:gd>"), Valid, None),
                (&doc("<t:gd>---02-14:00</t:gd>"), Valid, None),
                (&doc("<t:gd/>"), Valid, None),
                (&doc("<t:gd>---02</t:gd>"), Invalid, FIXED),
                (&doc("<t:gd>---02+10:00</t:gd>"), Invalid, FIXED),
                (&doc("<t:gmd>--02-13-12:00</t:gmd>"), Valid, None),
                (&doc("<t:gmd>--02-14</t:gmd>"), Invalid, FIXED),
                (&doc("<t:gy>2000+00:00</t:gy><t:gym>2000-01-00:00</t:gym><t:gm>--05+00:00</t:gm>"), Valid, None),
                (&doc("<t:gy>2000</t:gy>"), Invalid, FIXED),
                (&doc("<t:dtf>2000-01-01T05:00:00+05:00</t:dtf>"), Valid, None),
                (&doc(r#"<t:ga d="---03+10:00"/>"#), Valid, None),
                (&doc(r#"<t:ga d="---03"/>"#), Invalid, Some("cvc-attribute.4")),
                (
                    &doc("<t:keys><k>---02-14:00</k><k>---03+10:00</k></t:keys>"),
                    Invalid,
                    DUPLICATE,
                ),
                (
                    &doc("<t:keys><k>---02-14:00</k><k>---02</k><ref>---03+10:00</ref></t:keys>"),
                    Valid,
                    None,
                ),
                (
                    &doc("<t:keys><k>---02</k><ref>---02Z</ref></t:keys>"),
                    Invalid,
                    Some("cvc-identity-constraint.4.3"),
                ),
            ],
        );
    }
    // An attribute use's fixed value against its declaration's: XSD 1.0
    // compares by equality, XSD 1.1 by identity (Structures 1.1 §3.5.6.1
    // Attribute Use Correct clause 3: "is identical to"; Datatypes 1.1
    // §2.2.2: "In the prior version of this specification (1.0), equality was
    // always identity") — the same instant with two offsets is equal, not
    // identical.
    let uses = r#"<xs:attribute name="g" type="xs:gDay" fixed="---02-14:00"/>
      <xs:element name="u"><xs:complexType>
        <xs:attribute ref="t:g" fixed="---03+10:00"/>
      </xs:complexType></xs:element>"#;
    let mut failures10 = Vec::new();
    check_schemas(
        &mut failures10,
        XsdVersion::V1_0,
        &[(
            "attribute use fixed ---03+10:00 for ---02-14:00",
            uses.to_string(),
            true,
        )],
    );
    check_schemas(
        &mut failures10,
        XsdVersion::V1_1,
        &[(
            "attribute use fixed ---03+10:00 for ---02-14:00",
            uses.to_string(),
            false,
        )],
    );
    failures.extend(failures10);
    assert_no_failures(&failures);
}

// ── C48 (union part): unions compare their members' values ───────────────

/// Datatypes 1.1 §4.3.5.4: "A value in a ·value space· is facet-valid with
/// respect to ·enumeration· if and only if the value is equal or identical
/// to one of the values specified in {value}" — for a union, the values its
/// member types give the literals (§2.4.1.3), compared in the member's
/// value space as the atomic types compare them.
#[test]
fn union_enumerations_compare_member_values() {
    let union = |name: &str, members: &str, values: &[&str]| {
        let values: String = values
            .iter()
            .map(|v| format!(r#"<xs:enumeration value="{v}"/>"#))
            .collect();
        format!(
            r#"<xs:simpleType name="{name}u"><xs:union memberTypes="{members}"/></xs:simpleType>
               {}"#,
            st(name, &format!("t:{name}u"), &values)
        )
    };
    let body = format!(
        "{ROOT}{}{}{}{}{}{}{}",
        union(
            "dt",
            "xs:integer xs:dateTime",
            &["2000-01-01T00:00:00Z", "7"]
        ),
        union("gd", "xs:gDay xs:string", &["---02-14:00"]),
        union("du", "xs:duration xs:integer", &["P1D"]),
        union("fl", "xs:float xs:string", &["0"]),
        union("dec", "xs:decimal xs:date", &["1.0", "2000-01-01Z"]),
        union("tm", "xs:time xs:boolean", &["12:00:00Z", "true"]),
        elements(&["dt", "gd", "du", "fl", "dec", "tm"]),
    );
    let mut failures = Vec::new();
    for version in VERSIONS {
        let ss = schema(version, &body);
        check_rows(
            &mut failures,
            &format!("{version:?}"),
            &ss,
            &[
                (&doc("<t:dt>2000-01-01T05:00:00+05:00</t:dt>"), Valid, None),
                (&doc("<t:dt>1999-12-31T14:00:00-10:00</t:dt>"), Valid, None),
                (&doc("<t:dt>007</t:dt>"), Valid, None),
                (&doc("<t:dt>2000-01-01T00:00:00</t:dt>"), Invalid, ENUM),
                (&doc("<t:gd>---03+10:00</t:gd>"), Valid, None),
                (&doc("<t:gd>---02</t:gd>"), Invalid, ENUM),
                (&doc("<t:du>PT24H</t:du>"), Valid, None),
                (&doc("<t:du>P1DT1S</t:du>"), Invalid, ENUM),
                (&doc("<t:fl>-0</t:fl>"), Valid, None),
                (&doc("<t:fl>0.0E0</t:fl>"), Valid, None),
                (
                    &doc("<t:dec>1</t:dec><t:dec>2000-01-01+00:00</t:dec>"),
                    Valid,
                    None,
                ),
                (&doc("<t:dec>2000-01-01</t:dec>"), Invalid, ENUM),
                (
                    &doc("<t:tm>13:00:00+01:00</t:tm><t:tm>1</t:tm>"),
                    Valid,
                    None,
                ),
                (&doc("<t:tm>12:00:00</t:tm>"), Invalid, ENUM),
            ],
        );
    }
    assert_no_failures(&failures);
}

// ── C56: g* patterns constrain the lexical form ──────────────────────────

/// Datatypes 1.1 §4.3.4: "pattern is a constraint on the ·value space· of a
/// datatype which is achieved by constraining the ·lexical space· to
/// ·literals· which match each member of {value}"; §4.3.4.4 pattern valid: "A
/// ·literal· in a ·lexical space· is facet-valid with respect to ·pattern·
/// if: the ·literal· is among the set of character sequences denoted by the
/// ·regular expression·" (XSD 1.0 §4.3.4, §4.3.4.4 alike). The ·literal· is
/// the instance's normalized value, as for every other datatype, not the
/// canonical representation of its value.
#[test]
fn g_patterns_constrain_the_lexical_form() {
    let body = format!(
        "{ROOT}{}{}{}{}{}{}{}",
        st("yPlus", "xs:gYear", r#"<xs:pattern value="\d{4}\+00:00"/>"#),
        st("yZ", "xs:gYear", r#"<xs:pattern value="\d{4}Z"/>"#),
        st(
            "dMinus",
            "xs:gDay",
            r#"<xs:pattern value="---\d\d-00:00"/>"#
        ),
        st(
            "mdPlain",
            "xs:gMonthDay",
            r#"<xs:pattern value="--\d\d-\d\d"/>"#
        ),
        st(
            "ymZ",
            "xs:gYearMonth",
            r#"<xs:pattern value="\d{4}-\d\d\+00:00"/>"#
        ),
        st("mZ", "xs:gMonth", r#"<xs:pattern value="--\d\dZ"/>"#),
        elements(&["yPlus", "yZ", "dMinus", "mdPlain", "ymZ", "mZ"]),
    );
    let mut failures = Vec::new();
    for version in VERSIONS {
        let ss = schema(version, &body);
        check_rows(
            &mut failures,
            &format!("{version:?}"),
            &ss,
            &[
                (&doc("<t:yPlus>2000+00:00</t:yPlus>"), Valid, None),
                (&doc("<t:yPlus>2000Z</t:yPlus>"), Invalid, PATTERN),
                (&doc("<t:yZ>2000Z</t:yZ>"), Valid, None),
                (&doc("<t:yZ>2000+00:00</t:yZ>"), Invalid, PATTERN),
                (&doc("<t:dMinus>---05-00:00</t:dMinus>"), Valid, None),
                (&doc("<t:dMinus>---05Z</t:dMinus>"), Invalid, PATTERN),
                (&doc("<t:mdPlain> --05-01 </t:mdPlain>"), Valid, None),
                (&doc("<t:ymZ>2000-01+00:00</t:ymZ>"), Valid, None),
                (&doc("<t:ymZ>2000-01Z</t:ymZ>"), Invalid, PATTERN),
                (&doc("<t:mZ>--05Z</t:mZ>"), Valid, None),
                (&doc("<t:mZ>--05+00:00</t:mZ>"), Invalid, PATTERN),
            ],
        );
    }
    assert_no_failures(&failures);
}

// ── C60 + C29 residue: facet derivation in the value space ───────────────

/// Datatypes 1.1 §4.3.7.1 (and §4.3.8.1 – §4.3.10.1; XSD 1.0 §4.3.7.1 alike):
/// "If {fixed} is true, then types for which the current type is the {base
/// type definition} cannot specify a value for maxInclusive other than
/// {value}" — a value, so another literal of the same value is not another
/// value. §4.3.5.5 (both versions): "It is an ·error· if any member of
/// {value} is not in the ·value space· of {base type definition}" — for an
/// enumerated base, its enumerated values.
#[test]
fn facet_derivation_compares_values() {
    let fixed = |base: &str, facet: &str, value: &str, derived: &str| {
        format!(
            "{}{}",
            st(
                "b",
                base,
                &format!(r#"<xs:{facet} value="{value}" fixed="true"/>"#)
            ),
            st("t", "t:b", &format!(r#"<xs:{facet} value="{derived}"/>"#))
        )
    };
    let subset = |base: &str, values: &[&str], derived: &str| {
        let values: String = values
            .iter()
            .map(|v| format!(r#"<xs:enumeration value="{v}"/>"#))
            .collect();
        format!(
            "{}{}",
            st("b", base, &values),
            st(
                "t",
                "t:b",
                &format!(r#"<xs:enumeration value="{derived}"/>"#)
            )
        )
    };
    let ints = r#"<xs:simpleType name="ints"><xs:list itemType="xs:integer"/></xs:simpleType>"#;
    let int_or_date = r#"<xs:simpleType name="intOrDate"><xs:union memberTypes="xs:integer xs:date"/></xs:simpleType>"#;
    let mut failures = Vec::new();
    for version in VERSIONS {
        check_schemas(
            &mut failures,
            version,
            &[
                (
                    "fixed maxInclusive 10, derived ' 10 '",
                    fixed("xs:decimal", "maxInclusive", "10", " 10 "),
                    true,
                ),
                (
                    "fixed maxInclusive 10, derived 10.0",
                    fixed("xs:decimal", "maxInclusive", "10", "10.0"),
                    true,
                ),
                (
                    "fixed minInclusive 10 (integer), derived +10",
                    fixed("xs:integer", "minInclusive", "10", "+10"),
                    true,
                ),
                (
                    "fixed maxExclusive 1 (double), derived 1.0E0",
                    fixed("xs:double", "maxExclusive", "1", "1.0E0"),
                    true,
                ),
                (
                    "fixed minExclusive 2000-01-01Z, derived 2000-01-01+00:00",
                    fixed("xs:date", "minExclusive", "2000-01-01Z", "2000-01-01+00:00"),
                    true,
                ),
                (
                    "fixed maxInclusive 2000-01-01T00:00:00Z, derived the same instant at +05:00",
                    fixed(
                        "xs:dateTime",
                        "maxInclusive",
                        "2000-01-01T00:00:00Z",
                        "2000-01-01T05:00:00+05:00",
                    ),
                    true,
                ),
                (
                    "fixed maxInclusive P1D, derived PT24H",
                    fixed("xs:duration", "maxInclusive", "P1D", "PT24H"),
                    true,
                ),
                (
                    "fixed maxInclusive 10, derived 9",
                    fixed("xs:decimal", "maxInclusive", "10", "9"),
                    false,
                ),
                (
                    "fixed minInclusive 2000Z, derived 2000",
                    fixed("xs:gYear", "minInclusive", "2000Z", "2000"),
                    false,
                ),
                (
                    "fixed maxInclusive 10 two levels up, derived 9",
                    format!(
                        "{}{}{}",
                        st(
                            "a",
                            "xs:decimal",
                            r#"<xs:maxInclusive value="10" fixed="true"/>"#
                        ),
                        st("b", "t:a", ""),
                        st("t", "t:b", r#"<xs:maxInclusive value="9"/>"#)
                    ),
                    false,
                ),
                (
                    "enumeration 01 under {1, 5}",
                    subset("xs:integer", &["1", "5"], "01"),
                    true,
                ),
                (
                    "enumeration +5 under {1, 5}",
                    subset("xs:integer", &["1", "5"], "+5"),
                    true,
                ),
                (
                    "enumeration 2 under {1, 5}",
                    subset("xs:integer", &["1", "5"], "2"),
                    false,
                ),
                (
                    "enumeration 1 under {1.0} of decimal",
                    subset("xs:decimal", &["1.0"], "1"),
                    true,
                ),
                (
                    "enumeration -0 under {0} of double",
                    subset("xs:double", &["0"], "-0"),
                    true,
                ),
                (
                    "enumeration 1 under {true} of boolean",
                    subset("xs:boolean", &["true"], "1"),
                    true,
                ),
                (
                    "enumeration 0a under {0A} of hexBinary",
                    subset("xs:hexBinary", &["0A"], "0a"),
                    true,
                ),
                (
                    "enumeration 2000-01-01+00:00 under {2000-01-01Z}",
                    subset("xs:date", &["2000-01-01Z"], "2000-01-01+00:00"),
                    true,
                ),
                (
                    "enumeration 2000-01-01 under {2000-01-01Z}",
                    subset("xs:date", &["2000-01-01Z"], "2000-01-01"),
                    false,
                ),
                (
                    "enumeration PT24H under {P1D}",
                    subset("xs:duration", &["P1D"], "PT24H"),
                    true,
                ),
                (
                    "enumeration ---03+10:00 under {---02-14:00}",
                    subset("xs:gDay", &["---02-14:00"], "---03+10:00"),
                    true,
                ),
                (
                    "list enumeration '01 02' under {'1 2'}",
                    format!("{ints}{}", subset("t:ints", &["1 2"], "01 02")),
                    true,
                ),
                (
                    "list enumeration '1 3' under {'1 2'}",
                    format!("{ints}{}", subset("t:ints", &["1 2"], "1 3")),
                    false,
                ),
                (
                    "union enumeration 01 under {1, 2000-01-01Z}",
                    format!(
                        "{int_or_date}{}",
                        subset("t:intOrDate", &["1", "2000-01-01Z"], "01")
                    ),
                    true,
                ),
                (
                    "union enumeration 2000-01-01+00:00 under {1, 2000-01-01Z}",
                    format!(
                        "{int_or_date}{}",
                        subset("t:intOrDate", &["1", "2000-01-01Z"], "2000-01-01+00:00")
                    ),
                    true,
                ),
                (
                    "union enumeration 2 under {1, 2000-01-01Z}",
                    format!(
                        "{int_or_date}{}",
                        subset("t:intOrDate", &["1", "2000-01-01Z"], "2")
                    ),
                    false,
                ),
                // An exclusive bound equal to the base's of the same kind
                // (§4.3.8.1: "or be equal to {value} in {base type
                // definition}"), also under a built-in type with bounds.
                (
                    "minExclusive -128 under minExclusive -128 of xs:byte",
                    format!(
                        "{}{}",
                        st("b", "xs:byte", r#"<xs:minExclusive value="-128"/>"#),
                        st("t", "t:b", r#"<xs:minExclusive value="-128"/>"#)
                    ),
                    true,
                ),
                (
                    "maxExclusive 5 under maxExclusive 5 of xs:unsignedByte",
                    format!(
                        "{}{}",
                        st("b", "xs:unsignedByte", r#"<xs:maxExclusive value="5"/>"#),
                        st("t", "t:b", r#"<xs:maxExclusive value="5"/>"#)
                    ),
                    true,
                ),
                (
                    "minExclusive +0 under minExclusive 0 of xs:nonNegativeInteger",
                    format!(
                        "{}{}",
                        st(
                            "b",
                            "xs:nonNegativeInteger",
                            r#"<xs:minExclusive value="0"/>"#
                        ),
                        st("t", "t:b", r#"<xs:minExclusive value="+0"/>"#)
                    ),
                    true,
                ),
                (
                    "maxExclusive 6 under maxExclusive 5 of xs:unsignedByte",
                    format!(
                        "{}{}",
                        st("b", "xs:unsignedByte", r#"<xs:maxExclusive value="5"/>"#),
                        st("t", "t:b", r#"<xs:maxExclusive value="6"/>"#)
                    ),
                    false,
                ),
                // Datatypes §4.3.6: "The value of whiteSpace must be one of
                // {preserve, replace, collapse}" (XSD 1.0 §4.3.6 alike).
                (
                    "whiteSpace bogus",
                    st("t", "xs:string", r#"<xs:whiteSpace value="bogus"/>"#),
                    false,
                ),
                (
                    "whiteSpace ' replace '",
                    st("t", "xs:string", r#"<xs:whiteSpace value=" replace "/>"#),
                    true,
                ),
            ],
        );
    }
    assert_no_failures(&failures);
}

/// A simple-content restriction's facets restrict its base's content type
/// (Structures 1.1 §3.4.2.2: a simple type definition "which restricts the
/// simple type definition identified in clause 4.1 … with a set of facet
/// components corresponding to the appropriate element information items
/// among the <restriction>'s [children] …, as defined in Simple Type
/// Restriction (Facets) (§3.16.6.4)"), so the Datatypes constraints on those
/// facets hold against it: §4.3.5.5 enumeration valid restriction, and the
/// bound facets' {value} "must be equal to some value in the ·value space·
/// of {base type definition}" (§4.3.7.2).
#[test]
fn simple_content_restriction_facets_restrict_the_base_content() {
    let base = r#"
      <xs:complexType name="dec"><xs:simpleContent>
        <xs:extension base="xs:decimal"><xs:attribute name="u" type="xs:string"/></xs:extension>
      </xs:simpleContent></xs:complexType>
      <xs:complexType name="ab"><xs:simpleContent><xs:restriction base="t:strCT">
        <xs:enumeration value="a"/><xs:enumeration value="b"/>
      </xs:restriction></xs:simpleContent></xs:complexType>
      <xs:complexType name="strCT"><xs:simpleContent>
        <xs:extension base="xs:string"><xs:attribute name="u" type="xs:string"/></xs:extension>
      </xs:simpleContent></xs:complexType>
      <xs:complexType name="pct"><xs:simpleContent><xs:restriction base="t:dec">
        <xs:minInclusive value="0"/><xs:maxInclusive value="100"/>
      </xs:restriction></xs:simpleContent></xs:complexType>"#;
    let restrict = |name: &str, base_type: &str, facets: &str| {
        format!(
            r#"{base}<xs:complexType name="{name}"><xs:simpleContent><xs:restriction base="t:{base_type}">{facets}</xs:restriction></xs:simpleContent></xs:complexType>"#
        )
    };
    let mut failures = Vec::new();
    for version in VERSIONS {
        check_schemas(
            &mut failures,
            version,
            &[
                ("the base types alone", base.to_string(), true),
                (
                    "enumeration abc over decimal content",
                    restrict("t", "dec", r#"<xs:enumeration value="abc"/>"#),
                    false,
                ),
                (
                    "enumeration 1.50 over decimal content",
                    restrict("t", "dec", r#"<xs:enumeration value="1.50"/>"#),
                    true,
                ),
                (
                    "enumeration c under {a, b}",
                    restrict("t", "ab", r#"<xs:enumeration value="c"/>"#),
                    false,
                ),
                (
                    "enumeration a under {a, b}",
                    restrict("t", "ab", r#"<xs:enumeration value="a"/>"#),
                    true,
                ),
                (
                    "maxInclusive 200 under maxInclusive 100",
                    restrict("t", "pct", r#"<xs:maxInclusive value="200"/>"#),
                    false,
                ),
                (
                    "maxInclusive 50 under maxInclusive 100",
                    restrict("t", "pct", r#"<xs:maxInclusive value="50"/>"#),
                    true,
                ),
                (
                    "minInclusive abc over decimal content",
                    restrict("t", "dec", r#"<xs:minInclusive value="abc"/>"#),
                    false,
                ),
                (
                    "enumeration 101 under 0..100",
                    restrict("t", "pct", r#"<xs:enumeration value="101"/>"#),
                    false,
                ),
                (
                    "enumeration 100.0 under 0..100",
                    restrict("t", "pct", r#"<xs:enumeration value="100.0"/>"#),
                    true,
                ),
                (
                    "minExclusive 7 with maxExclusive 7",
                    restrict(
                        "t",
                        "dec",
                        r#"<xs:minExclusive value="7"/><xs:maxExclusive value="7"/>"#,
                    ),
                    true,
                ),
                (
                    "minInclusive 8 with maxInclusive 7",
                    restrict(
                        "t",
                        "dec",
                        r#"<xs:minInclusive value="8"/><xs:maxInclusive value="7"/>"#,
                    ),
                    false,
                ),
            ],
        );
    }
    assert_no_failures(&failures);
}

// ── C65: an unbound prefix in a QName value ──────────────────────────────

/// Datatypes 1.1 §3.3.18: "The mapping from lexical space to value space for
/// a particular QName ·literal· depends on the namespace bindings in scope
/// where the literal occurs. When QNames appear in an XML context, the
/// bindings to be used in the ·lexical mapping· are those in the [in-scope
/// namespaces] property of the relevant element" — a prefix bound nowhere
/// maps to no value, so the literal is not valid (`cvc-datatype-valid.1.2.1`),
/// whichever path validates it: an attribute a wildcard admits, or an
/// element of a union type with a QName member.
#[test]
fn an_unbound_prefix_is_invalid_on_every_path() {
    let body = format!(
        r#"{ROOT}
      <xs:simpleType name="qnames"><xs:list itemType="xs:QName"/></xs:simpleType>
      <xs:simpleType name="qOrInt"><xs:union memberTypes="xs:QName xs:integer"/></xs:simpleType>
      <xs:simpleType name="qsOrInt"><xs:union memberTypes="t:qnames xs:integer"/></xs:simpleType>
      <xs:attribute name="ga" type="xs:QName"/>
      <xs:attribute name="gl" type="t:qnames"/>
      <xs:attribute name="gu" type="t:qOrInt"/>
      <xs:element name="strict"><xs:complexType><xs:anyAttribute processContents="strict"/></xs:complexType></xs:element>
      <xs:element name="lax"><xs:complexType><xs:anyAttribute processContents="lax"/></xs:complexType></xs:element>
      <xs:element name="decl"><xs:complexType>
        <xs:attribute ref="t:ga"/><xs:attribute ref="t:gu"/>
      </xs:complexType></xs:element>
      <xs:element name="u" type="t:qOrInt"/>
      <xs:element name="ul" type="t:qsOrInt"/>
      <xs:element name="q" type="xs:QName"/>"#
    );
    let mut failures = Vec::new();
    for version in VERSIONS {
        let ss = schema(version, &body);
        let mut rows = Vec::new();
        for wildcard in ["strict", "lax"] {
            for (attr, value) in [("ga", "zz:a"), ("gl", "t:a zz:b"), ("gu", "zz:a")] {
                rows.push((
                    doc(&format!(r#"<t:{wildcard} t:{attr}="{value}"/>"#)),
                    Invalid,
                    UNBOUND,
                ));
            }
            rows.push((
                doc(&format!(
                    r#"<t:{wildcard} t:ga="t:a" t:gl="t:a t:b" t:gu="7"/>"#
                )),
                Valid,
                None,
            ));
            rows.push((doc(&format!(r#"<t:{wildcard} t:gu="t:a"/>"#)), Valid, None));
        }
        rows.push((doc(r#"<t:decl t:ga="zz:a"/>"#), Invalid, UNBOUND));
        rows.push((doc(r#"<t:decl t:gu="zz:a"/>"#), Invalid, UNBOUND));
        rows.push((doc("<t:u>zz:a</t:u>"), Invalid, UNBOUND));
        rows.push((doc("<t:u>t:a</t:u><t:u>a</t:u><t:u>7</t:u>"), Valid, None));
        rows.push((doc("<t:ul>t:a zz:b</t:ul>"), Invalid, UNBOUND));
        rows.push((doc("<t:ul>t:a t:b</t:ul><t:ul>7</t:ul>"), Valid, None));
        rows.push((doc("<t:q>zz:a</t:q>"), Invalid, UNBOUND));
        let rows: Vec<(&str, SchemaValidity, Option<&str>)> = rows
            .iter()
            .map(|(xml, verdict, code)| (xml.as_str(), *verdict, *code))
            .collect();
        check_rows(&mut failures, &format!("{version:?}"), &ss, &rows);
    }
    assert_no_failures(&failures);
}

// ── Fix round 1 ───────────────────────────────────────────────────────────

/// Datatypes 1.1 §2.4.1.3: a union value "is validated against the ·member
/// types· in the order in which they appear in the definition until a match
/// is found". A QName literal whose prefix is bound nowhere has no lexical
/// mapping (§3.3.18), so the QName member does not match it and a later
/// member that accepts the literal does (`xs:string`, `xs:anyURI`); only
/// when no member matches is the value invalid (review V1, on every path).
#[test]
fn a_union_tries_the_next_member_after_an_unbound_prefix() {
    let body = format!(
        r#"{ROOT}
      <xs:simpleType name="qs"><xs:union memberTypes="xs:QName xs:string"/></xs:simpleType>
      <xs:simpleType name="qu"><xs:union memberTypes="xs:QName xs:anyURI"/></xs:simpleType>
      <xs:simpleType name="qi"><xs:union memberTypes="xs:QName xs:integer"/></xs:simpleType>
      <xs:simpleType name="lqs"><xs:list itemType="t:qs"/></xs:simpleType>
      <xs:simpleType name="lqi"><xs:list itemType="t:qi"/></xs:simpleType>
      <xs:attribute name="gq" type="t:qu"/>
      <xs:attribute name="gi" type="t:qi"/>
      <xs:element name="u1" type="t:qs"/>
      <xs:element name="ui" type="t:qi"/>
      <xs:element name="l1" type="t:lqs"/>
      <xs:element name="li" type="t:lqi"/>
      <xs:element name="x" type="xs:anySimpleType"/>
      <xs:element name="sc"><xs:complexType><xs:simpleContent>
        <xs:extension base="t:qs"><xs:attribute name="n" type="xs:string"/></xs:extension>
      </xs:simpleContent></xs:complexType></xs:element>
      <xs:element name="da"><xs:complexType><xs:attribute name="a" type="t:qs"/></xs:complexType></xs:element>
      <xs:element name="strict"><xs:complexType><xs:anyAttribute processContents="strict"/></xs:complexType></xs:element>
      <xs:element name="lax"><xs:complexType><xs:anyAttribute processContents="lax"/></xs:complexType></xs:element>
      <xs:element name="anyLax"><xs:complexType><xs:sequence>
        <xs:any processContents="lax" maxOccurs="unbounded"/></xs:sequence></xs:complexType></xs:element>
      <xs:element name="keys"><xs:complexType><xs:sequence>
        <xs:element name="it" maxOccurs="unbounded"><xs:complexType><xs:attribute name="a" type="t:qs"/></xs:complexType></xs:element>
      </xs:sequence></xs:complexType>
        <xs:key name="itKey"><xs:selector xpath="it"/><xs:field xpath="@a"/></xs:key>
      </xs:element>"#
    );
    const XSI: &str = r#"xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance""#;
    let mut failures = Vec::new();
    for version in VERSIONS {
        let ss = schema(version, &body);
        let rows = [
            (doc("<t:u1>zz:a</t:u1><t:u1>t:a</t:u1>"), Valid, None),
            (doc("<t:sc>zz:a</t:sc>"), Valid, None),
            (doc(r#"<t:da a="zz:a"/>"#), Valid, None),
            (
                doc(r#"<t:strict t:gq="zz:a"/><t:lax t:gq="zz:a"/>"#),
                Valid,
                None,
            ),
            (doc("<t:anyLax><t:u1>zz:a</t:u1></t:anyLax>"), Valid, None),
            (
                doc(&format!(r#"<t:x {XSI} xsi:type="t:qs">zz:a</t:x>"#)),
                Valid,
                None,
            ),
            (doc("<t:l1>zz:a t:b</t:l1>"), Valid, None),
            (
                doc(r#"<t:keys><it a="zz:a"/><it a="zz:b"/></t:keys>"#),
                Valid,
                None,
            ),
            (
                doc(r#"<t:keys><it a="zz:a"/><it a="zz:a"/></t:keys>"#),
                Invalid,
                DUPLICATE,
            ),
            // No later member accepts the literal: the prefix is reported.
            (doc("<t:ui>zz:a</t:ui>"), Invalid, UNBOUND),
            (doc("<t:li>5 zz:a</t:li>"), Invalid, UNBOUND),
            (doc(r#"<t:strict t:gi="zz:a"/>"#), Invalid, UNBOUND),
            (
                doc(&format!(r#"<t:x {XSI} xsi:type="t:qi">zz:a</t:x>"#)),
                Invalid,
                UNBOUND,
            ),
        ];
        let rows: Vec<(&str, SchemaValidity, Option<&str>)> = rows
            .iter()
            .map(|(xml, verdict, code)| (xml.as_str(), *verdict, *code))
            .collect();
        check_rows(&mut failures, &format!("{version:?}"), &ss, &rows);
    }
    assert_no_failures(&failures);
}

/// The four "valid restriction" constraints compare a derived bound with
/// every bound of the base's {facets} — the built-in types' own included —
/// whatever the derived step's other bounds are. Datatypes 1.1 §4.3.8.4
/// maxExclusive valid restriction clause 3 (XSD 1.0 alike): an error if
/// "minInclusive is among the members of {facets} of {base type definition}
/// and {value} is less than or equal to the {value} of that minInclusive";
/// 1.1 §4.3.9.4 minExclusive valid restriction clause 3: "maxInclusive is
/// among the members of {facets} of {base type definition} and {value} is
/// greater than or equal to the {value} of that maxInclusive" (review V2).
#[test]
fn exclusive_bounds_against_the_base_bounds() {
    let pair = |base: &str, lo: &str, hi: &str| {
        st(
            "t",
            base,
            &format!(r#"<xs:minExclusive value="{lo}"/><xs:maxExclusive value="{hi}"/>"#),
        )
    };
    let mut failures = Vec::new();
    for version in VERSIONS {
        check_schemas(
            &mut failures,
            version,
            &[
                (
                    "unsignedLong 0..0",
                    pair("xs:unsignedLong", "0", "0"),
                    false,
                ),
                (
                    "unsignedLong maxExclusive 0 alone",
                    st("t", "xs:unsignedLong", r#"<xs:maxExclusive value="0"/>"#),
                    false,
                ),
                ("byte 127..127", pair("xs:byte", "127", "127"), false),
                ("byte -128..-128", pair("xs:byte", "-128", "-128"), false),
                (
                    "positiveInteger 1..1",
                    pair("xs:positiveInteger", "1", "1"),
                    false,
                ),
                ("unsignedByte 7..7", pair("xs:unsignedByte", "7", "7"), true),
                (
                    "minExclusive 0, maxExclusive 0 under a user minInclusive 0",
                    format!(
                        "{}{}",
                        st("b", "xs:integer", r#"<xs:minInclusive value="0"/>"#),
                        pair("t:b", "0", "0")
                    ),
                    false,
                ),
                (
                    "minExclusive 9 under a user maxInclusive 9",
                    format!(
                        "{}{}",
                        st("b", "xs:integer", r#"<xs:maxInclusive value="9"/>"#),
                        st(
                            "t",
                            "t:b",
                            r#"<xs:minExclusive value="9"/><xs:maxExclusive value="9"/>"#
                        )
                    ),
                    false,
                ),
            ],
        );
    }
    assert_no_failures(&failures);
}

/// §4.3.8.1 (maxExclusive; §4.3.9.1 minExclusive alike): its value "must be
/// equal to some value in the ·value space· of the ·base type· or be equal
/// to {value} in {base type definition}" — the base's {facets}, which hold a
/// bound inherited from further up; the alternative is unconditional
/// (review V3).
#[test]
fn an_exclusive_bound_may_equal_an_inherited_one() {
    let chain = |first: &str, middle: &str, last: &str| {
        format!(
            "{}{}{}",
            st("s0", "xs:decimal", first),
            st("s1", "t:s0", middle),
            st("s2", "t:s1", last)
        )
    };
    let max10 = r#"<xs:maxExclusive value="10"/>"#;
    let mut failures = Vec::new();
    for version in VERSIONS {
        check_schemas(
            &mut failures,
            version,
            &[
                (
                    "maxExclusive 10 / fractionDigits 0 / maxExclusive 10",
                    chain(max10, r#"<xs:fractionDigits value="0"/>"#, max10),
                    true,
                ),
                (
                    "maxExclusive 10 / pattern / maxExclusive 10",
                    chain(max10, r#"<xs:pattern value="\d+"/>"#, max10),
                    true,
                ),
                (
                    "maxExclusive 10 / enumeration 5 9 / maxExclusive 10",
                    chain(
                        max10,
                        r#"<xs:enumeration value="5"/><xs:enumeration value="9"/>"#,
                        max10,
                    ),
                    true,
                ),
                (
                    "minExclusive 1.5 fixed / totalDigits 3 / minExclusive 1.5",
                    chain(
                        r#"<xs:minExclusive value="1.5" fixed="true"/>"#,
                        r#"<xs:totalDigits value="3"/>"#,
                        r#"<xs:minExclusive value="1.50"/>"#,
                    ),
                    true,
                ),
                (
                    "maxExclusive 10 / fractionDigits 0 / maxExclusive 11",
                    chain(
                        max10,
                        r#"<xs:fractionDigits value="0"/>"#,
                        r#"<xs:maxExclusive value="11"/>"#,
                    ),
                    false,
                ),
            ],
        );
    }
    assert_no_failures(&failures);
}

/// Datatypes 1.1 §4.3.1.4 length valid restriction (XSD 1.0 §4.3.1.4 word for
/// word): "It is an ·error· if length is among the members of {facets} of
/// {base type definition} and {value} is not equal to the {value} of the
/// parent length", fixed or not; minLength (§4.3.2.4), maxLength (§4.3.3.4),
/// totalDigits (§4.3.11.4) and fractionDigits (§4.3.12.4) valid restriction
/// through every ancestor (review V4).
#[test]
fn length_and_digit_facets_restrict_their_bases() {
    let two = |base: &str, first: &str, second: &str| {
        format!("{}{}", st("s0", base, first), st("s1", "t:s0", second))
    };
    let three = |base: &str, first: &str, middle: &str, last: &str| {
        format!(
            "{}{}{}",
            st("s0", base, first),
            st("s1", "t:s0", middle),
            st("s2", "t:s1", last)
        )
    };
    let pat = r#"<xs:pattern value=".*"/>"#;
    let mut failures = Vec::new();
    for version in VERSIONS {
        check_schemas(
            &mut failures,
            version,
            &[
                (
                    "length 4 / length 2",
                    two(
                        "xs:string",
                        r#"<xs:length value="4"/>"#,
                        r#"<xs:length value="2"/>"#,
                    ),
                    false,
                ),
                (
                    "length 4 / length 4",
                    two(
                        "xs:string",
                        r#"<xs:length value="4"/>"#,
                        r#"<xs:length value="4"/>"#,
                    ),
                    true,
                ),
                (
                    "length 4 / pattern / length 5",
                    three(
                        "xs:string",
                        r#"<xs:length value="4"/>"#,
                        pat,
                        r#"<xs:length value="5"/>"#,
                    ),
                    false,
                ),
                (
                    "minLength 2 / pattern / minLength 1",
                    three(
                        "xs:string",
                        r#"<xs:minLength value="2"/>"#,
                        pat,
                        r#"<xs:minLength value="1"/>"#,
                    ),
                    false,
                ),
                (
                    "maxLength 3 / pattern / maxLength 4",
                    three(
                        "xs:string",
                        r#"<xs:maxLength value="3"/>"#,
                        pat,
                        r#"<xs:maxLength value="4"/>"#,
                    ),
                    false,
                ),
                (
                    "totalDigits 5 / pattern / totalDigits 6",
                    three(
                        "xs:decimal",
                        r#"<xs:totalDigits value="5"/>"#,
                        pat,
                        r#"<xs:totalDigits value="6"/>"#,
                    ),
                    false,
                ),
                (
                    "fractionDigits 2 / pattern / fractionDigits 3",
                    three(
                        "xs:decimal",
                        r#"<xs:fractionDigits value="2"/>"#,
                        pat,
                        r#"<xs:fractionDigits value="3"/>"#,
                    ),
                    false,
                ),
                (
                    "fractionDigits 2 / fractionDigits 1",
                    two(
                        "xs:decimal",
                        r#"<xs:fractionDigits value="2"/>"#,
                        r#"<xs:fractionDigits value="1"/>"#,
                    ),
                    true,
                ),
            ],
        );
    }
    assert_no_failures(&failures);
}

/// Datatypes 1.1 §3.3.6.1: "the equality or order of two duration values is
/// determined by adding each duration in the pair to each of the following
/// four dateTime values"; `P400Y` and `P146097D` give equal sums at all four,
/// so they are equal — for `enumeration`, `fixed`, keys and unions as for
/// the bounds (review V5, decision 11).
#[test]
fn durations_with_equal_sums_are_equal() {
    let body = format!(
        r#"{ROOT}{}{}
      <xs:element name="f" type="xs:duration" fixed="P400Y"/>
      <xs:element name="keys"><xs:complexType><xs:sequence>
        <xs:element name="k" type="xs:duration" maxOccurs="unbounded"/></xs:sequence></xs:complexType>
        <xs:key name="durKey"><xs:selector xpath="k"/><xs:field xpath="."/></xs:key>
      </xs:element>{}"#,
        st("e", "xs:duration", r#"<xs:enumeration value="P400Y"/>"#),
        st(
            "b",
            "xs:duration",
            r#"<xs:minInclusive value="P400Y"/><xs:maxInclusive value="P400Y"/>"#
        ),
        elements(&["e", "b"]),
    );
    let mut failures = Vec::new();
    for version in VERSIONS {
        let ss = schema(version, &body);
        check_rows(
            &mut failures,
            &format!("{version:?}"),
            &ss,
            &[
                (
                    &doc("<t:e>P146097D</t:e><t:b>P146097D</t:b><t:f>P146097D</t:f>"),
                    Valid,
                    None,
                ),
                (&doc("<t:e>P146096D</t:e>"), Invalid, ENUM),
                (
                    &doc("<t:keys><k>P400Y</k><k>P146097D</k></t:keys>"),
                    Invalid,
                    DUPLICATE,
                ),
            ],
        );
    }
    assert_no_failures(&failures);
}

/// XSD 1.0 §3.2.7: "'-0001' is the lexical representation of the year 1
/// Before Common Era (1 BCE …)", and the year 0000 is disallowed; so in XSD
/// 1.0 the year -0001 is followed by the year 0001, and
/// `-0001-12-31T23:00:00Z` is six hours before `0001-01-01T05:00:00Z`.
/// Datatypes 1.1 §3.3.7: "'0000' … 1 BCE", so `-0001` is 2 BCE, a year
/// earlier (review V7).
#[test]
fn xsd_1_0_has_no_year_zero_on_the_time_line() {
    let body = format!(
        "{ROOT}{}{}",
        st(
            "m",
            "xs:dateTime",
            r#"<xs:minInclusive value="-0001-12-31T23:00:00Z"/>"#
        ),
        elements(&["m"])
    );
    let mut failures = Vec::new();
    let v10 = schema(XsdVersion::V1_0, &body);
    check_rows(
        &mut failures,
        "V1_0",
        &v10,
        &[
            (&doc("<t:m>0001-01-01T05:00:00</t:m>"), Invalid, MIN_IN),
            (&doc("<t:m>0001-01-01T20:00:00</t:m>"), Valid, None),
            (&doc("<t:m>-0001-12-31T22:00:00Z</t:m>"), Invalid, MIN_IN),
        ],
    );
    let v11 = schema(XsdVersion::V1_1, &body);
    check_rows(
        &mut failures,
        "V1_1",
        &v11,
        &[
            (&doc("<t:m>0001-01-01T05:00:00</t:m>"), Valid, None),
            (&doc("<t:m>-0001-12-31T22:00:00Z</t:m>"), Invalid, MIN_IN),
        ],
    );
    let pair = st(
        "t",
        "xs:dateTime",
        r#"<xs:minInclusive value="0001-01-01T05:00:00"/><xs:maxInclusive value="-0001-12-31T23:00:00Z"/>"#,
    );
    check_schemas(
        &mut failures,
        XsdVersion::V1_0,
        &[(
            "min 0001-01-01T05:00 local, max -0001-12-31T23:00Z",
            pair.clone(),
            true,
        )],
    );
    check_schemas(
        &mut failures,
        XsdVersion::V1_1,
        &[(
            "min 0001-01-01T05:00 local, max -0001-12-31T23:00Z",
            pair,
            false,
        )],
    );
    assert_no_failures(&failures);
}

/// `length`, `minLength`, `maxLength`, `fractionDigits` and `minOccurs` /
/// `maxOccurs` are `xs:nonNegativeInteger` values, whose zero "may be
/// preceded by a positive ("+") or a negative ("-") sign" (XSD 1.0
/// §3.3.20.1; Datatypes 1.1 §3.4.20.1) — C27's class (review V8).
#[test]
fn minus_zero_counts_are_zero() {
    let mut failures = Vec::new();
    for version in VERSIONS {
        check_schemas(
            &mut failures,
            version,
            &[
                ("length -0", st("t", "xs:string", r#"<xs:length value="-0"/>"#), true),
                (
                    "minLength -0, maxLength -00",
                    st(
                        "t",
                        "xs:string",
                        r#"<xs:minLength value="-0"/><xs:maxLength value="-00"/>"#,
                    ),
                    true,
                ),
                ("fractionDigits -0", st("t", "xs:decimal", r#"<xs:fractionDigits value="-0"/>"#), true),
                (
                    "minOccurs -0",
                    r#"<xs:element name="r"><xs:complexType><xs:sequence>
                         <xs:element name="v" minOccurs="-0" maxOccurs="-0"/></xs:sequence></xs:complexType></xs:element>"#
                        .to_string(),
                    true,
                ),
                ("length -1", st("t", "xs:string", r#"<xs:length value="-1"/>"#), false),
                (
                    "minOccurs -1",
                    r#"<xs:element name="r"><xs:complexType><xs:sequence>
                         <xs:element name="v" minOccurs="-1"/></xs:sequence></xs:complexType></xs:element>"#
                        .to_string(),
                    false,
                ),
            ],
        );
    }
    assert_no_failures(&failures);
}

/// cos-applicable-facets for a simple-content restriction: its facets
/// restrict the content's simple type, so they must be applicable to it —
/// pattern and enumeration (and assertions in 1.1) for a union, the length
/// facets, pattern, enumeration and whiteSpace for a list, the datatype's
/// facets for an atomic type (Datatypes 1.1 §4.1.5; review V9).
#[test]
fn simple_content_facets_must_apply_to_the_content_type() {
    let base = r#"
      <xs:simpleType name="intOrDate"><xs:union memberTypes="xs:integer xs:date"/></xs:simpleType>
      <xs:simpleType name="ints"><xs:list itemType="xs:integer"/></xs:simpleType>
      <xs:complexType name="u"><xs:simpleContent><xs:extension base="t:intOrDate"/></xs:simpleContent></xs:complexType>
      <xs:complexType name="l"><xs:simpleContent><xs:extension base="t:ints"/></xs:simpleContent></xs:complexType>
      <xs:complexType name="d"><xs:simpleContent><xs:extension base="xs:decimal"/></xs:simpleContent></xs:complexType>"#;
    let restrict = |of: &str, facets: &str| {
        format!(
            r#"{base}<xs:complexType name="r"><xs:simpleContent><xs:restriction base="t:{of}">{facets}</xs:restriction></xs:simpleContent></xs:complexType>"#
        )
    };
    let mut failures = Vec::new();
    for version in VERSIONS {
        check_schemas(
            &mut failures,
            version,
            &[
                (
                    "union content: maxInclusive",
                    restrict("u", r#"<xs:maxInclusive value="5"/>"#),
                    false,
                ),
                (
                    "union content: length",
                    restrict("u", r#"<xs:length value="1"/>"#),
                    false,
                ),
                (
                    "union content: enumeration 5",
                    restrict("u", r#"<xs:enumeration value="5"/>"#),
                    true,
                ),
                (
                    "list content: maxInclusive",
                    restrict("l", r#"<xs:maxInclusive value="5"/>"#),
                    false,
                ),
                (
                    "list content: length 2",
                    restrict("l", r#"<xs:length value="2"/>"#),
                    true,
                ),
                (
                    "decimal content: length",
                    restrict("d", r#"<xs:length value="3"/>"#),
                    false,
                ),
                (
                    "decimal content: totalDigits 3",
                    restrict("d", r#"<xs:totalDigits value="3"/>"#),
                    true,
                ),
            ],
        );
    }
    assert_no_failures(&failures);
}

// ── Fix round 2: exact sums ───────────────────────────────────────────────

/// Datatypes 1.1 §3.3.6.1: the order of two durations is that of the four
/// reference dateTimes plus each duration, and "the original pair of duration
/// values is ordered the same way". `PT0.00000000000000000001S` is a positive
/// duration, so it is greater than `PT0S` at all four: not an enumerated
/// `PT0S`, above a `minExclusive` of `PT0S`. A value is equal to itself
/// however large it is (review, fix round 2).
#[test]
fn duration_sums_are_exact() {
    const HUGE: &str =
        "-P4294967295Y4294967295M4294967295DT4294967295H4294967295M79228162514264337593543950335S";
    let body = format!(
        r#"{ROOT}{}{}{}{}{}
      <xs:element name="f" type="xs:duration" fixed="{HUGE}"/>
      <xs:element name="keys"><xs:complexType><xs:sequence>
        <xs:element name="k" type="xs:duration" maxOccurs="unbounded"/></xs:sequence></xs:complexType>
        <xs:key name="durKey"><xs:selector xpath="k"/><xs:field xpath="."/></xs:key>
      </xs:element>"#,
        st("zero", "xs:duration", r#"<xs:enumeration value="PT0S"/>"#),
        st("above", "xs:duration", r#"<xs:minExclusive value="PT0S"/>"#),
        st(
            "huge",
            "xs:duration",
            &format!(r#"<xs:enumeration value="{HUGE}"/>"#)
        ),
        st(
            "hugeBounds",
            "xs:duration",
            &format!(r#"<xs:minInclusive value="{HUGE}"/><xs:maxInclusive value="{HUGE}"/>"#),
        ),
        elements(&["zero", "above", "huge", "hugeBounds"]),
    );
    let tiny = "PT0.00000000000000000001S";
    let mut failures = Vec::new();
    for version in VERSIONS {
        let ss = schema(version, &body);
        check_rows(
            &mut failures,
            &format!("{version:?}"),
            &ss,
            &[
                (&doc(&format!("<t:zero>{tiny}</t:zero>")), Invalid, ENUM),
                (&doc(&format!("<t:above>{tiny}</t:above>")), Valid, None),
                (&doc("<t:above>P10000DT0.0000000000000000000001S</t:above>"), Valid, None),
                (&doc("<t:above>-PT0.00000000000000000001S</t:above>"), Invalid, MIN_EX),
                (&doc("<t:zero>PT0S</t:zero><t:above>PT1S</t:above>"), Valid, None),
                (
                    &doc(&format!(
                        "<t:huge>{HUGE}</t:huge><t:hugeBounds>{HUGE}</t:hugeBounds><t:f>{HUGE}</t:f>"
                    )),
                    Valid,
                    None,
                ),
                (
                    &doc(&format!("<t:keys><k>{HUGE}</k><k>{HUGE}</k></t:keys>")),
                    Invalid,
                    DUPLICATE,
                ),
            ],
        );
        check_schemas(
            &mut failures,
            version,
            &[(
                "minInclusive = maxInclusive = the largest negative duration",
                st(
                    "t",
                    "xs:duration",
                    &format!(
                        r#"<xs:minInclusive value="{HUGE}"/><xs:maxInclusive value="{HUGE}"/>"#
                    ),
                ),
                true,
            )],
        );
    }
    assert_no_failures(&failures);
}

/// The time line of the seven-property types (Datatypes 1.1 §D.2.1, §E.3.4
/// ·timeOnTimeline·) is exact too: a fraction of a second below 10⁻¹⁸ after
/// a timestamp of the year 2000, a year near the end of the range, a
/// negative year (review, fix round 2).
#[test]
fn time_line_sums_are_exact() {
    let body = format!(
        "{ROOT}{}{}{}{}",
        st(
            "afterY2k",
            "xs:dateTime",
            r#"<xs:minExclusive value="2000-01-01T00:00:00Z"/>"#
        ),
        st(
            "y2k",
            "xs:dateTime",
            r#"<xs:enumeration value="2000-01-01T00:00:00Z"/>"#
        ),
        st(
            "afterNoon",
            "xs:time",
            r#"<xs:minExclusive value="12:00:00+05:00"/>"#
        ),
        elements(&["afterY2k", "y2k", "afterNoon"]),
    );
    let mut failures = Vec::new();
    for version in VERSIONS {
        let ss = schema(version, &body);
        check_rows(
            &mut failures,
            &format!("{version:?}"),
            &ss,
            &[
                (&doc("<t:afterY2k>2000-01-01T00:00:00.00000000000000000001Z</t:afterY2k>"), Valid, None),
                (&doc("<t:afterY2k>2000-01-01T05:00:00.0000000000000000000001+05:00</t:afterY2k>"), Valid, None),
                (&doc("<t:afterY2k>2000-01-01T00:00:00Z</t:afterY2k>"), Invalid, MIN_EX),
                (&doc("<t:y2k>2000-01-01T00:00:00.00000000000000000001Z</t:y2k>"), Invalid, ENUM),
                (&doc("<t:y2k>2000-01-01T05:00:00.000+05:00</t:y2k>"), Valid, None),
                (&doc("<t:afterNoon>07:00:00.0000000000000000000000001Z</t:afterNoon>"), Valid, None),
                (&doc("<t:afterY2k>2147483647-12-31T23:59:59.9999999999999999999999999999Z</t:afterY2k>"), Valid, None),
            ],
        );
        check_schemas(
            &mut failures,
            version,
            &[
                (
                    "a large year equal to itself",
                    st(
                        "t",
                        "xs:dateTime",
                        r#"<xs:minInclusive value="2147483647-12-31T23:59:59.9999999999999999999999999999Z"/><xs:maxInclusive value="2147483647-12-31T23:59:59.9999999999999999999999999999Z"/>"#,
                    ),
                    true,
                ),
                (
                    "a large negative year equal to itself",
                    st(
                        "t",
                        "xs:dateTime",
                        r#"<xs:minInclusive value="-2147483647-01-01T00:00:00.5-14:00"/><xs:maxInclusive value="-2147483647-01-01T00:00:00.5-14:00"/>"#,
                    ),
                    true,
                ),
            ],
        );
    }
    assert_no_failures(&failures);
}

/// Seconds with more fraction digits than this implementation holds (a
/// 96-bit mantissa, at most 28 fraction digits): the literal is valid, its
/// fraction is truncated toward zero to the digits that fit — never rounded
/// up to the next second — and the validator says so with the warning
/// `xs-seconds-capacity` (Datatypes 1.1 §5.4: such a literal "must not be
/// treated as invalid", and the processor "must indicate to the user and/or
/// downstream application that it cannot process the input data with
/// assured correctness"). Fix round 2.
#[test]
fn seconds_beyond_capacity_are_valid_with_a_warning() {
    let body = format!(
        r#"{ROOT}{}{}{}{}
      <xs:element name="a"><xs:complexType>
        <xs:attribute name="d" type="xs:duration"/></xs:complexType></xs:element>"#,
        st(
            "beforeY2k",
            "xs:dateTime",
            r#"<xs:maxExclusive value="2000-01-01T00:00:00Z"/>"#
        ),
        st("t", "xs:time", ""),
        st("dur", "xs:duration", ""),
        elements(&["beforeY2k", "t", "dur"]),
    );
    const NOTICE: &str = "xs-seconds-capacity";
    let rows: &[(&str, bool)] = &[
        // 59 and 29 nines: truncated to 59 and 27 nines, still before 2000.
        (
            "<t:beforeY2k>1999-12-31T23:59:59.99999999999999999999999999999Z</t:beforeY2k>",
            true,
        ),
        ("<t:t>10:00:00.000000000000000000000000000001</t:t>", true),
        ("<t:dur>-PT0.000000000000000000000000000001S</t:dur>", true),
        ("<t:dur>PT79228162514264337593543950335.5S</t:dur>", true),
        (r#"<t:a d="PT0.000000000000000000000000000001S"/>"#, true),
        // Exact: at most 28 fraction digits, or only trailing zeros beyond.
        ("<t:t>10:00:00.0000000000000000000000000001</t:t>", false),
        (
            "<t:dur>PT1.50000000000000000000000000000000S</t:dur>",
            false,
        ),
        ("<t:beforeY2k>1999-12-31T23:59:59.5Z</t:beforeY2k>", false),
    ];
    let mut failures = Vec::new();
    for version in VERSIONS {
        let ss = schema(version, &body);
        for (inner, warned) in rows {
            let xml = doc(inner);
            let v = SchemaValidator::new(&ss, ValidationFlags::default());
            let mut errors = Vec::new();
            let mut warnings = Vec::new();
            let validity = {
                let mut rt = v.start_run(CollectingValidationSink {
                    errors: &mut errors,
                    warnings: &mut warnings,
                });
                drive_quick_xml(xml.as_bytes(), &mut rt, &ss)
                    .expect("stream drive completes")
                    .root_validity
            };
            let notices = warnings.iter().filter(|w| w.code == NOTICE).count();
            if validity != Some(Valid) || !errors.is_empty() || (notices == 1) != *warned {
                failures.push(format!(
                    "{version:?}: {xml} => {validity:?} {:?}, {notices} notice(s) (expected Valid, {})",
                    errors.iter().map(|e| e.constraint.to_string()).collect::<Vec<_>>(),
                    if *warned { "one notice" } else { "none" }
                ));
            }
        }
    }
    assert_no_failures(&failures);
}

/// The comparisons that see the literals — the bounds, `enumeration` (also
/// of a union and of a list), `fixed`, identity constraints and the
/// schema-time facet checks —
/// compare a seconds number with more fraction digits than the typed value
/// holds as written: `PT0.000000000000000000000000000001S` (10⁻³⁰) is above
/// `PT0S`, and `PT60.0000000000000000000000000005S`, whose 28th digit the
/// typed value drops, is above `PT1M0.0000000000000000000000000001S`, which
/// keeps all 28 (truncation alone would order the two the other way).
/// Fix round 2.
#[test]
fn seconds_beyond_capacity_compare_as_written() {
    const E30: &str = "PT0.000000000000000000000000000001S";
    const E30_DT: &str = "2000-01-01T00:00:00.000000000000000000000000000001Z";
    let body = format!(
        r#"{ROOT}{}{}{}{}{}{}{}{}{}{}
      <xs:element name="fx" type="xs:duration" fixed="{E30}"/>
      <xs:element name="keys"><xs:complexType><xs:sequence>
        <xs:element name="k" type="xs:duration" maxOccurs="unbounded"/></xs:sequence></xs:complexType>
        <xs:key name="durKey"><xs:selector xpath="k"/><xs:field xpath="."/></xs:key>
      </xs:element>"#,
        st("above", "xs:duration", r#"<xs:minExclusive value="PT0S"/>"#),
        st("below", "xs:duration", r#"<xs:maxExclusive value="PT0S"/>"#),
        st("zero", "xs:duration", r#"<xs:enumeration value="PT0S"/>"#),
        st(
            "aboveMinute",
            "xs:duration",
            r#"<xs:minExclusive value="PT1M0.0000000000000000000000000001S"/>"#
        ),
        st(
            "tiny",
            "xs:dateTime",
            &format!(r#"<xs:enumeration value="{E30_DT}"/>"#)
        ),
        st(
            "fromNoon",
            "xs:time",
            r#"<xs:minInclusive value="12:00:00.000000000000000000000000000001+05:00"/>"#
        ),
        r#"<xs:simpleType name="u"><xs:restriction><xs:simpleType><xs:union memberTypes="xs:duration xs:string"/></xs:simpleType><xs:enumeration value="PT0.000000000000000000000000000001S"/></xs:restriction></xs:simpleType>"#,
        r#"<xs:simpleType name="l"><xs:restriction><xs:simpleType><xs:list itemType="xs:duration"/></xs:simpleType><xs:enumeration value="PT0S PT0.000000000000000000000000000001S"/></xs:restriction></xs:simpleType>"#,
        st("dummy", "xs:string", ""),
        elements(&[
            "above",
            "below",
            "zero",
            "aboveMinute",
            "tiny",
            "fromNoon",
            "u",
            "l"
        ]),
    );
    let mut failures = Vec::new();
    for version in VERSIONS {
        let ss = schema(version, &body);
        check_rows(
            &mut failures,
            &format!("{version:?}"),
            &ss,
            &[
                (&doc(&format!("<t:above>{E30}</t:above>")), Valid, None),
                (&doc(&format!("<t:below>-{E30}</t:below>")), Valid, None),
                (&doc(&format!("<t:zero>{E30}</t:zero>")), Invalid, ENUM),
                (&doc("<t:aboveMinute>PT60.0000000000000000000000000005S</t:aboveMinute>"), Valid, None),
                (&doc("<t:aboveMinute>PT60.00000000000000000000000000009S</t:aboveMinute>"), Invalid, MIN_EX),
                (&doc("<t:tiny>2000-01-01T00:00:00Z</t:tiny>"), Invalid, ENUM),
                (&doc(&format!("<t:tiny>{E30_DT}</t:tiny>")), Valid, None),
                (&doc("<t:tiny>2000-01-01T05:00:00.0000000000000000000000000000010+05:00</t:tiny>"), Valid, None),
                (&doc("<t:fromNoon>07:00:00Z</t:fromNoon>"), Invalid, MIN_IN),
                (&doc("<t:fromNoon>07:00:00.000000000000000000000000000001Z</t:fromNoon>"), Valid, None),
                (&doc("<t:fx>PT0S</t:fx>"), Invalid, FIXED),
                (&doc(&format!("<t:fx>{E30}</t:fx>")), Valid, None),
                (&doc("<t:fx>PT0.0000000000000000000000000000010S</t:fx>"), Valid, None),
                (&doc("<t:u>PT0S</t:u>"), Invalid, ENUM),
                (&doc(&format!("<t:u>{E30}</t:u>")), Valid, None),
                (&doc("<t:l>PT0S PT0S</t:l>"), Invalid, ENUM),
                (&doc(&format!("<t:l>PT0S {E30}</t:l>")), Valid, None),
                (&doc(&format!("<t:keys><k>PT0S</k><k>{E30}</k></t:keys>")), Valid, None),
                (
                    &doc(&format!(
                        "<t:keys><k>{E30}</k><k>PT0.0000000000000000000000000000010S</k></t:keys>"
                    )),
                    Invalid,
                    DUPLICATE,
                ),
            ],
        );
        check_schemas(
            &mut failures,
            version,
            &[
                (
                    "minInclusive above maxInclusive beyond the 28th digit",
                    st(
                        "t",
                        "xs:duration",
                        r#"<xs:minInclusive value="PT0.000000000000000000000000000002S"/><xs:maxInclusive value="PT0.000000000000000000000000000001S"/>"#,
                    ),
                    false,
                ),
                (
                    "minInclusive below maxInclusive beyond the 28th digit",
                    st(
                        "t",
                        "xs:duration",
                        r#"<xs:minInclusive value="PT0.000000000000000000000000000001S"/><xs:maxInclusive value="PT0.000000000000000000000000000002S"/>"#,
                    ),
                    true,
                ),
                (
                    "an enumeration value outside the base enumeration beyond the 28th digit",
                    format!(
                        "{}{}",
                        st("b", "xs:duration", r#"<xs:enumeration value="PT0S"/>"#),
                        st("d", "t:b", &format!(r#"<xs:enumeration value="{E30}"/>"#)),
                    ),
                    false,
                ),
                (
                    "a fixed maxInclusive changed beyond the 28th digit",
                    format!(
                        "{}{}",
                        st(
                            "b",
                            "xs:duration",
                            &format!(r#"<xs:maxInclusive value="{E30}" fixed="true"/>"#)
                        ),
                        st("d", "t:b", r#"<xs:maxInclusive value="PT0S"/>"#),
                    ),
                    false,
                ),
                (
                    "an attribute use's fixed value differing beyond the 28th digit",
                    format!(
                        r#"<xs:attribute name="at" type="xs:duration" fixed="{E30}"/>
                        <xs:element name="e"><xs:complexType>
                          <xs:attribute ref="t:at" fixed="PT0S"/></xs:complexType></xs:element>"#
                    ),
                    false,
                ),
                (
                    "a fixed bound equal to itself beyond the 28th digit",
                    format!(
                        "{}{}",
                        st(
                            "b",
                            "xs:dateTime",
                            &format!(r#"<xs:maxInclusive value="{E30_DT}" fixed="true"/>"#)
                        ),
                        st(
                            "d",
                            "t:b",
                            &format!(r#"<xs:maxInclusive value="{E30_DT}"/>"#)
                        ),
                    ),
                    true,
                ),
            ],
        );
    }
    assert_no_failures(&failures);
}

/// `xs:dayTimeDuration` (XSD 1.1) is ordered on the exact sums too: a fraction
/// below 10⁻¹⁸ after ten thousand days is above zero, and the largest value
/// the fields hold is equal to itself (the sum used to overflow a `Decimal`).
/// Fix round 2.
#[test]
fn day_time_duration_sums_are_exact() {
    const HUGE: &str = "P4294967295DT4294967295H4294967295M79228162514264337593543950335S";
    let body = format!(
        "{ROOT}{}{}{}",
        st(
            "above",
            "xs:dayTimeDuration",
            r#"<xs:minExclusive value="PT0S"/>"#
        ),
        st(
            "huge",
            "xs:dayTimeDuration",
            &format!(
                r#"<xs:enumeration value="-{HUGE}"/><xs:minInclusive value="-{HUGE}"/><xs:maxInclusive value="-{HUGE}"/>"#
            ),
        ),
        elements(&["above", "huge"]),
    );
    let ss = schema(XsdVersion::V1_1, &body);
    let mut failures = Vec::new();
    check_rows(
        &mut failures,
        "V1_1",
        &ss,
        &[
            (
                &doc("<t:above>P10000DT0.0000000000000000000001S</t:above>"),
                Valid,
                None,
            ),
            (
                &doc("<t:above>PT0.00000000000000000001S</t:above>"),
                Valid,
                None,
            ),
            (
                &doc("<t:above>-PT0.00000000000000000001S</t:above>"),
                Invalid,
                MIN_EX,
            ),
            (
                &doc("<t:above>PT0.000000000000000000000000000001S</t:above>"),
                Valid,
                None,
            ),
            (&doc(&format!("<t:huge>-{HUGE}</t:huge>")), Valid, None),
            (&doc(&format!("<t:huge>{HUGE}</t:huge>")), Invalid, MAX_IN),
        ],
    );
    assert_no_failures(&failures);
}

/// Every value is equal to itself — as an enumeration, both inclusive bounds,
/// a `fixed` value and a key twice — also at the edges of the representation:
/// 30 fraction digits (rounded up to an invalid second before), the largest
/// years, the largest duration fields and their negation (their sums
/// overflowed before). Fix round 2 (the precision sweep's reflexivity rows).
#[test]
fn every_value_is_equal_to_itself() {
    const HUGE: &str =
        "P4294967295Y4294967295M4294967295DT4294967295H4294967295M79228162514264337593543950335S";
    let mut literals: Vec<(&str, String)> = vec![
        (
            "xs:dateTime",
            "1999-12-31T23:59:59.999999999999999999999999999999Z".into(),
        ),
        (
            "xs:dateTime",
            "2147483647-12-31T23:59:59.9999999999999999999999999999Z".into(),
        ),
        (
            "xs:dateTime",
            "-2147483648-01-01T00:00:00.000000000000000000000000000001".into(),
        ),
        ("xs:time", "23:59:59.999999999999999999999999999999Z".into()),
        ("xs:time", "07:59:07.92281625142643375935439503359".into()),
        ("xs:date", "-2147483648-01-01+14:00".into()),
        ("xs:gYear", "2147483647-14:00".into()),
        ("xs:duration", HUGE.into()),
        ("xs:duration", format!("-{HUGE}")),
        ("xs:duration", "-PT0.000000000000000000000000000001S".into()),
        ("xs:duration", "PT79228162514264337593543950335.5S".into()),
    ];
    let mut failures = Vec::new();
    for version in VERSIONS {
        if version == XsdVersion::V1_1 {
            literals.push((
                "xs:dayTimeDuration",
                "-P4294967295DT4294967295H4294967295M79228162514264337593543950335S".into(),
            ));
            literals.push(("xs:yearMonthDuration", "-P4294967295Y4294967295M".into()));
        }
        for (base, v) in &literals {
            let body = format!(
                r#"{ROOT}{}
      <xs:element name="f" type="{base}" fixed="{v}"/>
      <xs:element name="keys"><xs:complexType><xs:sequence>
        <xs:element name="k" type="{base}" maxOccurs="unbounded"/></xs:sequence></xs:complexType>
        <xs:key name="selfKey"><xs:selector xpath="k"/><xs:field xpath="."/></xs:key>
      </xs:element>{}"#,
                st(
                    "same",
                    base,
                    &format!(
                        r#"<xs:enumeration value="{v}"/><xs:minInclusive value="{v}"/><xs:maxInclusive value="{v}"/>"#
                    ),
                ),
                elements(&["same"]),
            );
            let ss = schema(version, &body);
            check_rows(
                &mut failures,
                &format!("{version:?} {base}"),
                &ss,
                &[
                    (
                        &doc(&format!("<t:same>{v}</t:same><t:f>{v}</t:f>")),
                        Valid,
                        None,
                    ),
                    (
                        &doc(&format!("<t:keys><k>{v}</k><k>{v}</k></t:keys>")),
                        Invalid,
                        DUPLICATE,
                    ),
                ],
            );
        }
    }
    assert_no_failures(&failures);
}
