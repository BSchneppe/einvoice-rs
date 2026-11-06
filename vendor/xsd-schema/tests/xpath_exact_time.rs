#![cfg(feature = "xsd11")]
//! Exact date/time and duration comparison and arithmetic in XPath.
//!
//! F&O 1.0 §10.4 compares date/time values on the time line and durations by
//! their months and seconds (§10.4.5 `op:duration-equal`: "the function
//! returns true if the months and seconds values of the two durations are
//! equal"); nothing in it rounds. §10.1.1: a processor that limits the digits
//! of date/time values "·must· raise an error [err:FODT0001] in case of
//! overflow" in the arithmetic of §10.8, and for durations "·must· raise an
//! error [err:FODT0002] in case of overflow" (§10.6).
//!
//! The XPath operators compare on the same exact keys as the schema facets
//! (whole seconds in `i128`, the fraction apart), so a fraction of 10⁻²⁰ s
//! keeps two values apart, the largest durations equal themselves, and no
//! comparison or sum panics.

use xsd_schema::namespace::context::NamespaceContextSnapshot;
use xsd_schema::namespace::table::NameTable;
use xsd_schema::types::value::TimezoneOffset;
use xsd_schema::xpath::{RoXmlNavigator, XPathContext, XPathExpr};
use xsd_schema::XmlItem;

const XS: &str = "http://www.w3.org/2001/XMLSchema";

/// The largest `xs:duration` the value struct holds (every field at its
/// limit).
const HUGE: &str =
    "P4294967295Y4294967295M4294967295DT4294967295H4294967295M79228162514264337593543950335S";
/// The largest `xs:dayTimeDuration`.
const HUGE_DTD: &str = "P4294967295DT4294967295H4294967295M79228162514264337593543950335S";

/// Evaluate `expr` (implicit timezone `Z`); the items joined by spaces, the
/// error code, or `PANIC`.
fn eval(expr: &str) -> String {
    std::panic::catch_unwind(|| {
        let names = NameTable::new();
        let mut namespaces = NamespaceContextSnapshot::default();
        namespaces.bindings.push((names.add("xs"), names.add(XS)));
        let ctx = XPathContext::new(&names)
            .with_namespaces(namespaces)
            .with_implicit_timezone(TimezoneOffset::UTC);
        let compiled = match XPathExpr::compile(expr, &ctx) {
            Ok(c) => c,
            Err(e) => return format!("ERR:{}", e.error_code().unwrap_or("?")),
        };
        match compiled.evaluator(&ctx).run::<RoXmlNavigator<'static>>() {
            Ok(v) => v
                .into_vec()
                .into_iter()
                .map(|item| match item {
                    XmlItem::Atomic(a) => a.to_string_value(),
                    XmlItem::Node(_) => "<node>".to_string(),
                })
                .collect::<Vec<_>>()
                .join(" "),
            Err(e) => format!("ERR:{}", e.error_code().unwrap_or("?")),
        }
    })
    .unwrap_or_else(|_| "PANIC".to_string())
}

fn check(rows: &[(String, &str)]) {
    let failures: Vec<String> = rows
        .iter()
        .filter_map(|(expr, expected)| {
            let got = eval(expr);
            (got != *expected).then(|| format!("{expr}\n    expected {expected}, got {got}"))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn rows(rows: &[(&str, &str)]) -> Vec<(String, String)> {
    rows.iter()
        .map(|(e, x)| {
            (
                e.replace("{HUGE}", HUGE).replace("{HUGE_DTD}", HUGE_DTD),
                x.to_string(),
            )
        })
        .collect()
}

fn check_rows(table: &[(&str, &str)]) {
    let owned = rows(table);
    let borrowed: Vec<(String, &str)> =
        owned.iter().map(|(e, x)| (e.clone(), x.as_str())).collect();
    check(&borrowed);
}

const DT0: &str = "xs:dateTime('2000-01-01T00:00:00Z')";
const DT20: &str = "xs:dateTime('2000-01-01T00:00:00.00000000000000000001Z')";

#[test]
fn comparisons_keep_every_fraction_digit() {
    let dt = |e: &str| e.replace("DT0", DT0).replace("DT20", DT20);
    let table: Vec<(String, &str)> = [
        ("DT20 eq DT0", "false"),
        ("DT20 ne DT0", "true"),
        ("DT20 gt DT0", "true"),
        ("DT0 lt DT20", "true"),
        ("DT20 le DT0", "false"),
        ("DT20 = DT0", "false"),
        ("DT20 > DT0", "true"),
        (
            "(for $i in 1 to 100 return DT20) = (for $i in 1 to 100 return DT0)",
            "false",
        ),
        ("count(distinct-values((DT20, DT0)))", "2"),
        ("index-of((DT20, DT0), DT0)", "2"),
        ("deep-equal(DT20, DT0)", "false"),
        ("string(max((DT0, DT20)))", "2000-01-01T00:00:00.00000000000000000001Z"),
        ("string(min((DT20, DT0)))", "2000-01-01T00:00:00Z"),
        ("DT20 - DT0", "PT0.00000000000000000001S"),
        // Twenty-eight fraction digits, in a value whose whole seconds have
        // eleven: no single `Decimal` holds the sum.
        (
            "xs:dateTime('2000-01-01T00:00:00.0000000000000000000000000001Z') gt DT0",
            "true",
        ),
        (
            "xs:dateTime('1000-01-01T00:00:00.0000000000000000000000000001Z') - xs:dateTime('1000-01-01T00:00:00Z')",
            "PT0.0000000000000000000000000001S",
        ),
        (
            "xs:dateTime('2000-01-01T00:00:00.1234567890123456789012345678Z') - xs:dateTime('1000-01-01T00:00:00Z')",
            "P365242DT0.1234567890123456789012345678S",
        ),
        (
            "xs:date('2000-01-01Z') lt xs:date('2000-01-02+14:00')",
            "true",
        ),
        (
            "xs:time('00:00:00.00000000000000000001Z') gt xs:time('00:00:00Z')",
            "true",
        ),
        // Twenty-five fraction digits in a time of day: the seconds since
        // midnight need thirty.
        (
            "xs:time('23:59:59.9999999999999999999999998Z') lt xs:time('23:59:59.9999999999999999999999999Z')",
            "true",
        ),
    ]
    .into_iter()
    .map(|(e, x)| (dt(e), x))
    .collect();
    check(&table);
}

#[test]
fn duration_comparisons_are_exact() {
    check_rows(&[
        (
            "xs:dayTimeDuration('P10000DT0.0000000000000000000001S') gt xs:dayTimeDuration('P10000D')",
            "true",
        ),
        (
            "xs:dayTimeDuration('P10000DT0.0000000000000000000001S') eq xs:dayTimeDuration('P10000D')",
            "false",
        ),
        (
            "string(max((xs:dayTimeDuration('P10000D'), xs:dayTimeDuration('P10000DT0.0000000000000000000001S'))))",
            "P10000DT0.0000000000000000000001S",
        ),
        (
            "xs:duration('P10000DT0.0000000000000000000001S') eq xs:duration('P10000D')",
            "false",
        ),
        // §10.4.5: months and seconds, not the four reference dateTimes.
        ("xs:duration('P1Y') eq xs:duration('P12M')", "true"),
        ("xs:duration('PT24H') eq xs:duration('P1D')", "true"),
        ("xs:duration('P1Y') eq xs:duration('P365D')", "false"),
        ("xs:duration('P400Y') eq xs:duration('P146097D')", "false"),
        ("xs:yearMonthDuration('P0Y') eq xs:dayTimeDuration('P0D')", "true"),
        ("xs:yearMonthDuration('P1Y') eq xs:dayTimeDuration('P365D')", "false"),
        ("xs:duration('P2Y0M0DT0H0M0S') eq xs:yearMonthDuration('P24M')", "true"),
        ("xs:duration('P0Y0M10D') eq xs:dayTimeDuration('PT240H')", "true"),
        ("xs:duration('-PT0.5S') eq xs:dayTimeDuration('-PT0.50S')", "true"),
        ("xs:duration('P1M') lt xs:duration('P2M')", "ERR:XPTY0004"),
        // The largest values equal themselves and are ordered.
        ("xs:duration('{HUGE}') eq xs:duration('{HUGE}')", "true"),
        ("xs:duration('-{HUGE}') eq xs:duration('-{HUGE}')", "true"),
        ("xs:duration('{HUGE}') ne xs:duration('-{HUGE}')", "true"),
        ("xs:duration('{HUGE}') = (xs:duration('PT0S'), xs:duration('{HUGE}'))", "true"),
        ("count(distinct-values((xs:duration('{HUGE}'), xs:duration('{HUGE}'))))", "1"),
        ("index-of((xs:duration('-{HUGE}'), xs:duration('{HUGE}')), xs:duration('{HUGE}'))", "2"),
        ("deep-equal(xs:duration('{HUGE}'), xs:duration('{HUGE}'))", "true"),
        ("xs:dayTimeDuration('{HUGE_DTD}') gt xs:dayTimeDuration('PT0S')", "true"),
        ("xs:dayTimeDuration('-{HUGE_DTD}') lt xs:dayTimeDuration('{HUGE_DTD}')", "true"),
        ("xs:dayTimeDuration('{HUGE_DTD}') eq xs:dayTimeDuration('{HUGE_DTD}')", "true"),
        (
            "string(max((xs:dayTimeDuration('{HUGE_DTD}'), xs:dayTimeDuration('-{HUGE_DTD}'))))",
            "P916992621692878754609936DT18H47M15S",
        ),
        (
            "xs:yearMonthDuration('P4294967295Y4294967295M') gt xs:yearMonthDuration('P4294967295Y')",
            "true",
        ),
        (
            "(for $i in 1 to 100 return xs:dayTimeDuration('PT0.00000000000000000001S')) = (for $i in 1 to 100 return xs:dayTimeDuration('PT0S'))",
            "false",
        ),
    ]);
}

/// §10.1.1, §10.6, §10.8: results stay within the value structs and raise the
/// F&O error beyond them, never panic or wrap.
#[test]
fn arithmetic_is_exact_and_overflows_with_the_f_and_o_errors() {
    check_rows(&[
        // Durations (§10.6; FODT0002).
        (
            "xs:dayTimeDuration('PT79228162514264337593543950335S') + xs:dayTimeDuration('PT1S')",
            "ERR:FODT0002",
        ),
        (
            "xs:dayTimeDuration('PT79228162514264337593543950335S') - xs:dayTimeDuration('-PT1S')",
            "ERR:FODT0002",
        ),
        ("xs:dayTimeDuration('{HUGE_DTD}') + xs:dayTimeDuration('{HUGE_DTD}')", "ERR:FODT0002"),
        ("xs:dayTimeDuration('{HUGE_DTD}') - xs:dayTimeDuration('{HUGE_DTD}')", "PT0S"),
        (
            "xs:dayTimeDuration('P1DT0.0000000000000000000000000001S') + xs:dayTimeDuration('P1D')",
            "P2DT0.0000000000000000000000000001S",
        ),
        (
            "xs:dayTimeDuration('PT0.0000000000000000000000001S') - xs:dayTimeDuration('P10000D')",
            "-P9999DT23H59M59.9999999999999999999999999S",
        ),
        (
            "xs:yearMonthDuration('P4294967295Y11M') + xs:yearMonthDuration('P1M')",
            "ERR:FODT0002",
        ),
        (
            "xs:yearMonthDuration('-P4294967295Y11M') - xs:yearMonthDuration('P1M')",
            "ERR:FODT0002",
        ),
        ("xs:yearMonthDuration('P4294967295Y') * 1000", "ERR:FODT0002"),
        ("xs:yearMonthDuration('P1Y') * xs:double('INF')", "ERR:FODT0002"),
        ("xs:yearMonthDuration('P1Y') * xs:double('NaN')", "ERR:FOCA0005"),
        ("xs:yearMonthDuration('P1Y') div 0", "ERR:FODT0002"),
        ("xs:yearMonthDuration('P1Y') div xs:double('INF')", "P0M"),
        ("xs:yearMonthDuration('P1Y') div xs:double('NaN')", "ERR:FOCA0005"),
        ("xs:yearMonthDuration('P1Y') div xs:yearMonthDuration('P0M')", "ERR:FOAR0001"),
        ("xs:dayTimeDuration('P1D') * xs:double('INF')", "ERR:FODT0002"),
        ("xs:dayTimeDuration('P1D') * xs:double('NaN')", "ERR:FOCA0005"),
        ("xs:dayTimeDuration('P1D') div 0", "ERR:FODT0002"),
        ("xs:dayTimeDuration('P1D') div xs:double('INF')", "PT0S"),
        ("xs:dayTimeDuration('P1D') div xs:double('NaN')", "ERR:FOCA0005"),
        ("xs:dayTimeDuration('P4294967295D') * 1000000000000000000", "ERR:FODT0002"),
        ("xs:dayTimeDuration('P1D') div xs:dayTimeDuration('PT0S')", "ERR:FOAR0001"),
        (
            "xs:dayTimeDuration('{HUGE_DTD}') div xs:dayTimeDuration('PT0.0000000000000000000000000001S')",
            "ERR:FOAR0002",
        ),
        // The seconds of the largest value exceed what an `xs:decimal` holds
        // here (2^96 - 1): FOAR0002, as for any decimal overflow (§6.2).
        ("xs:dayTimeDuration('{HUGE_DTD}') div xs:dayTimeDuration('{HUGE_DTD}')", "ERR:FOAR0002"),
        (
            "xs:dayTimeDuration('P10000DT0.0000000000000000000001S') div xs:dayTimeDuration('P10000D')",
            "1",
        ),
        // Date/time values (§10.8; FODT0001).
        (
            "xs:dateTime('2000-01-01T00:00:00Z') + xs:dayTimeDuration('PT79228162514264337593543950335S')",
            "ERR:FODT0001",
        ),
        ("xs:dateTime('2147483647-12-31T23:59:59Z') + xs:dayTimeDuration('P1D')", "ERR:FODT0001"),
        ("xs:dateTime('2147483647-12-31T23:59:59Z') + xs:dayTimeDuration('PT1S')", "ERR:FODT0001"),
        (
            "xs:dateTime('2147483647-12-31T23:59:58Z') + xs:dayTimeDuration('PT1S')",
            "2147483647-12-31T23:59:59Z",
        ),
        ("xs:date('2147483647-12-31') + xs:dayTimeDuration('P1D')", "ERR:FODT0001"),
        ("xs:date('-2147483648-01-01') - xs:dayTimeDuration('P1D')", "ERR:FODT0001"),
        ("xs:dateTime('2000-01-01T00:00:00Z') + xs:yearMonthDuration('P4294967295Y')", "ERR:FODT0001"),
        ("xs:date('2000-01-01') - xs:yearMonthDuration('P4294967295Y')", "ERR:FODT0001"),
        (
            "xs:dateTime('2000-01-01T00:00:00Z') - xs:dateTime('-2147483648-01-01T00:00:00Z')",
            "ERR:FODT0001",
        ),
        // fn:adjust-*-to-timezone across the ends of the year range (fix round 1).
        (
            "adjust-dateTime-to-timezone(xs:dateTime('2147483647-12-31T23:00:00Z'), xs:dayTimeDuration('PT14H'))",
            "ERR:FODT0001",
        ),
        (
            "adjust-dateTime-to-timezone(xs:dateTime('-2147483648-01-01T00:00:00Z'), xs:dayTimeDuration('-PT14H'))",
            "ERR:FODT0001",
        ),
        (
            "adjust-date-to-timezone(xs:date('-2147483648-01-01Z'), xs:dayTimeDuration('-PT14H'))",
            "ERR:FODT0001",
        ),
        (
            "adjust-dateTime-to-timezone(xs:dateTime('2147483647-12-31T09:00:00Z'), xs:dayTimeDuration('PT14H'))",
            "2147483647-12-31T23:00:00+14:00",
        ),
        ("xs:time('12:00:00') + xs:dayTimeDuration('{HUGE_DTD}')", "06:47:15"),
        ("xs:time('12:00:00') - xs:dayTimeDuration('{HUGE_DTD}')", "17:12:45"),
        (
            "xs:dateTime('2000-01-01T00:00:00.5Z') + xs:dayTimeDuration('PT0.0000000000000000000000000001S')",
            "2000-01-01T00:00:00.5000000000000000000000000001Z",
        ),
    ]);
}

// ---------------------------------------------------------------------------
// Through a schema: an XSD 1.1 assertion, and typed trees in 1.0 and 1.1
// ---------------------------------------------------------------------------

mod schema {
    use xsd_schema::validation::{
        drive_navigator, drive_quick_xml, CollectingValidationSink, SchemaValidator,
        SchemaValidity, ValidationFlags,
    };
    use xsd_schema::{RoXmlNavigator, SchemaSet, SchemaSetBuilder, XsdVersion};

    pub fn compile(version: XsdVersion, body: &str) -> SchemaSet {
        let xsd =
            format!(r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">{body}</xs:schema>"#);
        SchemaSetBuilder::with_version(version)
            .add_source(&xsd, "file:///xpath_exact_time.xsd")
            .expect("schema source accepted")
            .compile()
            .expect("schema compiles")
            .into_schema_set()
    }

    /// The root validity and the error constraints on the streaming driver
    /// (with and without PSVI typed values) and the navigator driver; `PANIC`
    /// when validation panics.
    pub fn outcomes(ss: &SchemaSet, xml: &str) -> Vec<String> {
        let mut out = Vec::new();
        for flags in [
            ValidationFlags::default(),
            ValidationFlags::default() & !ValidationFlags::BUILD_PSVI_TYPED_VALUES,
        ] {
            out.push(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let v = SchemaValidator::new_fragment_buffer(ss, flags);
                    let mut errors = Vec::new();
                    let mut warnings = Vec::new();
                    let validity = {
                        let mut rt = v.start_run(CollectingValidationSink {
                            errors: &mut errors,
                            warnings: &mut warnings,
                        });
                        drive_quick_xml(xml.as_bytes(), &mut rt, ss)
                            .expect("drive completes")
                            .root_validity
                    };
                    verdict(validity, &errors)
                }))
                .unwrap_or_else(|_| "PANIC".to_string()),
            );
        }
        out.push(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let v = SchemaValidator::new_fragment_buffer(ss, ValidationFlags::default());
                let doc = roxmltree::Document::parse(xml).expect("well-formed");
                let nav = RoXmlNavigator::new(&doc);
                let mut errors = Vec::new();
                let mut warnings = Vec::new();
                let validity = {
                    let mut rt = v.start_run(CollectingValidationSink {
                        errors: &mut errors,
                        warnings: &mut warnings,
                    });
                    drive_navigator(&nav, &mut rt, ss)
                        .expect("drive completes")
                        .root_validity
                };
                verdict(validity, &errors)
            }))
            .unwrap_or_else(|_| "PANIC".to_string()),
        );
        out
    }

    fn verdict(
        validity: Option<SchemaValidity>,
        errors: &[xsd_schema::validation::ValidationError],
    ) -> String {
        match (validity, errors.first()) {
            (Some(SchemaValidity::Valid), None) => "valid".to_string(),
            (_, Some(e)) => format!("invalid:{}", e.constraint),
            (other, None) => format!("{other:?}"),
        }
    }
}

/// An XSD 1.1 assertion compares a duration the struct holds at its limit,
/// and date/time values that differ beyond the 28th digit: no panic, and the
/// exact verdict, on every driver.
#[test]
fn assertions_compare_exactly_and_never_panic() {
    use xsd_schema::XsdVersion;
    let ss = schema::compile(
        XsdVersion::V1_1,
        r#"
        <xs:simpleType name="selfEqual">
          <xs:restriction base="xs:duration">
            <xs:assertion test="$value eq $value and $value = $value"/>
          </xs:restriction>
        </xs:simpleType>
        <xs:simpleType name="afterMidnight">
          <xs:restriction base="xs:dateTime">
            <xs:assertion test="$value gt xs:dateTime('2000-01-01T00:00:00Z')"/>
          </xs:restriction>
        </xs:simpleType>
        <xs:element name="r">
          <xs:complexType>
            <xs:sequence>
              <xs:element name="d" type="selfEqual" minOccurs="0"/>
              <xs:element name="t" type="afterMidnight" minOccurs="0"/>
              <xs:element name="a" type="xs:dayTimeDuration" minOccurs="0"/>
              <xs:element name="b" type="xs:dayTimeDuration" minOccurs="0"/>
            </xs:sequence>
            <xs:assert test="not(a) or a gt b"/>
          </xs:complexType>
        </xs:element>"#,
    );
    let cases = [
        (format!("<r><d>{HUGE}</d></r>"), "valid"),
        (format!("<r><d>-{HUGE}</d></r>"), "valid"),
        (
            "<r><t>2000-01-01T00:00:00.00000000000000000001Z</t></r>".to_string(),
            "valid",
        ),
        (
            "<r><t>2000-01-01T00:00:00Z</t></r>".to_string(),
            "invalid:cvc-assertion",
        ),
        (
            "<r><a>P10000DT0.0000000000000000000001S</a><b>P10000D</b></r>".to_string(),
            "valid",
        ),
        (
            format!("<r><a>{HUGE_DTD}</a><b>-{HUGE_DTD}</b></r>"),
            "valid",
        ),
        (
            format!("<r><a>{HUGE_DTD}</a><b>{HUGE_DTD}</b></r>"),
            "invalid:cvc-assertion",
        ),
    ];
    let mut failures = Vec::new();
    for (xml, expected) in &cases {
        let got = schema::outcomes(&ss, xml);
        if got.iter().any(|g| g != expected) {
            failures.push(format!("{xml}: expected {expected}, got {got:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// An XSD 1.1 type alternative whose test compares the largest duration: no
/// panic, and the alternative is selected.
#[test]
fn type_alternatives_compare_exactly_and_never_panic() {
    use xsd_schema::XsdVersion;
    let ss = schema::compile(
        XsdVersion::V1_1,
        &format!(
            r#"
        <xs:complexType name="base">
          <xs:simpleContent>
            <xs:extension base="xs:string">
              <xs:attribute name="d" type="xs:string"/>
            </xs:extension>
          </xs:simpleContent>
        </xs:complexType>
        <xs:complexType name="huge">
          <xs:simpleContent>
            <xs:restriction base="base">
              <xs:enumeration value="huge"/>
            </xs:restriction>
          </xs:simpleContent>
        </xs:complexType>
        <xs:element name="r" type="base">
          <xs:alternative test="xs:duration(@d) eq xs:duration('{HUGE}')" type="huge"/>
          <xs:alternative test="xs:dateTime(@d) gt xs:dateTime('2000-01-01T00:00:00Z')" type="huge"/>
        </xs:element>"#
        ),
    );
    for (xml, expected) in [
        (format!(r#"<r d="{HUGE}">huge</r>"#), "valid"),
        (
            format!(r#"<r d="{HUGE}">other</r>"#),
            "invalid:cvc-enumeration-valid",
        ),
        (
            r#"<r d="2000-01-01T00:00:00.00000000000000000001Z">other</r>"#.to_string(),
            "invalid:cvc-enumeration-valid",
        ),
        (
            r#"<r d="2000-01-01T00:00:00Z">other</r>"#.to_string(),
            "valid",
        ),
    ] {
        let got = schema::outcomes(&ss, &xml);
        assert!(
            got.iter().all(|g| g.starts_with(expected)),
            "{xml}: expected {expected}, got {got:?}"
        );
    }
}

/// The year `-2147483648`, the smallest an `i32` holds, is written as itself
/// (it was `--2147483648`).
#[test]
fn the_smallest_year_is_written_as_itself() {
    check_rows(&[
        (
            "string(xs:dateTime('-2147483648-01-01T00:00:00Z'))",
            "-2147483648-01-01T00:00:00Z",
        ),
        ("string(xs:gYear('-2147483648'))", "-2147483648"),
        (
            "xs:date('-2147483647-01-01') - xs:dayTimeDuration('P1D')",
            "-2147483648-12-31",
        ),
    ]);
}

/// The typed values of a document validated in XSD 1.0 and in 1.1 compare
/// exactly in XPath.
#[test]
fn typed_values_compare_exactly_in_both_versions() {
    use bumpalo::Bump;
    use xsd_schema::document::{
        build_typed_document_with, BufferDocumentOptions, SilentValidationSink, TypedBuildOptions,
    };
    use xsd_schema::validation::{NoopHandler, SchemaValidator, ValidationFlags};
    use xsd_schema::XsdVersion;

    for version in [XsdVersion::V1_0, XsdVersion::V1_1] {
        let ss = schema::compile(
            version,
            r#"
            <xs:element name="r">
              <xs:complexType>
                <xs:sequence>
                  <xs:element name="a" type="xs:dateTime"/>
                  <xs:element name="b" type="xs:dateTime"/>
                  <xs:element name="d" type="xs:duration"/>
                </xs:sequence>
              </xs:complexType>
            </xs:element>"#,
        );
        let xml = format!(
            "<r><a>2000-01-01T00:00:00.00000000000000000001Z</a><b>2000-01-01T00:00:00Z</b><d>{HUGE}</d></r>"
        );
        let validator = SchemaValidator::new(&ss, ValidationFlags::default());
        let mut runtime = validator.start_run(SilentValidationSink);
        let arena = Bump::new();
        let built = build_typed_document_with(
            xml.as_bytes(),
            &arena,
            &ss,
            &mut runtime,
            &mut NoopHandler,
            TypedBuildOptions::default().with_document_options(BufferDocumentOptions::full()),
        )
        .expect("the build completes");
        let mut ns = NamespaceContextSnapshot::default();
        ns.bindings
            .push((ss.name_table.add("xs"), ss.name_table.add(XS)));
        let ctx = XPathContext::new(&ss.name_table)
            .with_schema_set(&ss)
            .with_namespaces(ns);
        for (expr, expected) in [
            ("data(/r/a) gt data(/r/b)", "true"),
            ("data(/r/a) eq data(/r/b)", "false"),
            ("/r/a = /r/b", "false"),
            ("data(/r/d) eq data(/r/d)", "true"),
            ("/r/d = /r/d", "true"),
        ] {
            let got = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let compiled = XPathExpr::compile(expr, &ctx).expect("compiles");
                compiled
                    .evaluator(&ctx)
                    .run_with_node(built.document.create_navigator())
                    .map(|v| {
                        v.into_vec()
                            .into_iter()
                            .map(|item| match item {
                                XmlItem::Atomic(a) => a.to_string_value(),
                                XmlItem::Node(_) => "<node>".to_string(),
                            })
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .unwrap_or_else(|e| format!("ERR:{}", e.error_code().unwrap_or("?")))
            }))
            .unwrap_or_else(|_| "PANIC".to_string());
            assert_eq!(got, expected, "{version:?}: {expr}");
        }
    }
}
