#![cfg(feature = "xsd11")]
//! A date/time or duration arithmetic result whose seconds field cannot hold
//! the whole fraction keeps it truncated toward zero — and says so, with the
//! warning the validator reports for a literal of that kind,
//! `xs-seconds-capacity` (Datatypes 1.1 §5.4: a processor "must indicate to
//! the user and/or downstream application that it cannot process the input
//! data with assured correctness").
//!
//! The seconds field of a result is a `Decimal` holding the seconds below 60
//! and the fraction: `59.1234567890123456789012345678` needs 30 digits and
//! keeps 27 fraction digits. Every path reports it: an XPath evaluation
//! (`XPathEvaluator::with_warnings`, `DynamicContext::take_warnings`), and
//! during validation an XSD 1.1 assertion or type alternative (the run's
//! warnings). Once per evaluation and site (operator or function): a loop
//! that truncates a thousand times is one warning. An exact result is none.

use std::cell::RefCell;

use xsd_schema::namespace::context::NamespaceContextSnapshot;
use xsd_schema::namespace::table::NameTable;
use xsd_schema::types::value::TimezoneOffset;
use xsd_schema::xpath::{RoXmlNavigator, XPathContext, XPathExpr, XPathWarning};
use xsd_schema::XmlItem;

const XS: &str = "http://www.w3.org/2001/XMLSchema";
const CODE: &str = "xs-seconds-capacity";

/// A fraction of 28 digits: held exactly beside 0 whole seconds, not beside 59.
const F28: &str = "0.1234567890123456789012345678";

fn eval(expr: &str) -> (String, Vec<XPathWarning>) {
    let names = NameTable::new();
    let mut namespaces = NamespaceContextSnapshot::default();
    namespaces.bindings.push((names.add("xs"), names.add(XS)));
    let ctx = XPathContext::new(&names)
        .with_namespaces(namespaces)
        .with_implicit_timezone(TimezoneOffset::UTC);
    let warnings = RefCell::new(Vec::new());
    let compiled = XPathExpr::compile(expr, &ctx).unwrap_or_else(|e| panic!("{expr}: {e}"));
    let value = compiled
        .evaluator(&ctx)
        .with_warnings(&warnings)
        .run::<RoXmlNavigator<'static>>()
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
        .unwrap_or_else(|e| format!("ERR:{}", e.error_code().unwrap_or("?")));
    (value, warnings.into_inner())
}

#[test]
fn a_truncated_seconds_field_is_reported() {
    let rows: Vec<(String, String)> = [
        // dayTimeDuration + dayTimeDuration
        (
            "xs:dayTimeDuration('PT{F}S') + xs:dayTimeDuration('PT59S')",
            "PT59.123456789012345678901234567S",
        ),
        (
            "xs:dayTimeDuration('PT59S') - xs:dayTimeDuration('-PT{F}S')",
            "PT59.123456789012345678901234567S",
        ),
        // time − time, dateTime − dateTime
        (
            "xs:time('00:00:59Z') - xs:time('00:00:00.1234567890123456789012345678Z')",
            "PT58.876543210987654321098765432S",
        ),
        (
            "xs:dateTime('2000-01-01T00:00:59Z') - xs:dateTime('2000-01-01T00:00:00.1234567890123456789012345678Z')",
            "PT58.876543210987654321098765432S",
        ),
        // dateTime ± dayTimeDuration, time ± dayTimeDuration
        (
            "xs:dateTime('2000-01-01T00:00:00.1234567890123456789012345678Z') + xs:dayTimeDuration('PT59S')",
            "2000-01-01T00:00:59.123456789012345678901234567Z",
        ),
        (
            "xs:time('00:00:00.1234567890123456789012345678') + xs:dayTimeDuration('PT59S')",
            "00:00:59.123456789012345678901234567",
        ),
        (
            "xs:dayTimeDuration('PT59S') + xs:dateTime('2000-01-01T00:00:00.1234567890123456789012345678Z')",
            "2000-01-01T00:00:59.123456789012345678901234567Z",
        ),
        // fn:sum and fn:avg of dayTimeDurations
        (
            "sum((xs:dayTimeDuration('PT{F}S'), xs:dayTimeDuration('PT59S')))",
            "PT59.123456789012345678901234567S",
        ),
        (
            "avg((xs:dayTimeDuration('PT{F}S'), xs:dayTimeDuration('PT59S'))) gt xs:dayTimeDuration('PT29S')",
            "true",
        ),
    ]
    .iter()
    .map(|(e, x)| (e.replace("{F}", F28), x.to_string()))
    .collect();
    let mut failures = Vec::new();
    for (expr, expected) in &rows {
        let (value, warnings) = eval(expr);
        if value != *expected
            || warnings.len() != 1
            || warnings
                .iter()
                .any(|w| w.code() != CODE || w.message().is_empty())
        {
            failures.push(format!(
                "{expr}\n    expected {expected} with one {CODE}, got {value} {warnings:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn an_exact_result_raises_no_warning() {
    for expr in [
        "xs:dayTimeDuration('PT0.5S') + xs:dayTimeDuration('PT59S')",
        "xs:dayTimeDuration('PT0.1234567890123456789012345678S') + xs:dayTimeDuration('P1D')",
        "xs:time('00:00:59Z') - xs:time('00:00:00.5Z')",
        "xs:dateTime('2000-01-01T00:00:00.5Z') + xs:dayTimeDuration('PT1S')",
        "xs:dateTime('2000-01-01T00:00:00.1234567890123456789012345678Z') + xs:dayTimeDuration('P1D')",
        "xs:date('2000-01-01') + xs:dayTimeDuration('PT59.123456789012345678901234567S')",
        "xs:dayTimeDuration('PT59.123456789012345678901234567S') * 2",
        "xs:dayTimeDuration('PT59.123456789012345678901234567S') div 3",
        "adjust-dateTime-to-timezone(xs:dateTime('2000-01-01T00:00:59.123456789012345678901234567Z'), xs:dayTimeDuration('PT5H'))",
        "sum((xs:dayTimeDuration('PT0.5S'), xs:dayTimeDuration('PT59S')))",
        "xs:dayTimeDuration('PT0.1234567890123456789012345678S') eq xs:dayTimeDuration('PT59S')",
    ] {
        let (value, warnings) = eval(expr);
        assert!(!value.starts_with("ERR"), "{expr}: {value}");
        assert!(warnings.is_empty(), "{expr}: {warnings:?}");
    }
}

/// Once per evaluation and site: the same operator truncating on every
/// iteration of a loop warns once; two operators that truncate warn twice.
#[test]
fn a_site_warns_once_per_evaluation() {
    let x = format!("(xs:dayTimeDuration('PT{F28}S') + xs:dayTimeDuration('PT59S'))");
    let (value, warnings) = eval(&format!("for $i in 1 to 1000 return {x}"));
    assert_eq!(value.split(' ').count(), 1000);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    let (_, warnings) = eval(&format!("({x}, {x})"));
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    let (_, warnings) = eval(&format!(
        "for $i in 1 to 10 return sum((xs:dayTimeDuration('PT{F28}S'), xs:dayTimeDuration('PT59S')))"
    ));
    assert_eq!(warnings.len(), 1, "{warnings:?}");
}

/// `DynamicContext::take_warnings` for a host that evaluates with its own
/// dynamic context (`eval_node`).
#[test]
fn a_dynamic_context_hands_its_warnings_over() {
    use xsd_schema::xpath::{bind_node, eval_node, parse, DynamicContext, NameBinder};
    let names = NameTable::new();
    let mut namespaces = NamespaceContextSnapshot::default();
    namespaces.bindings.push((names.add("xs"), names.add(XS)));
    let ctx = XPathContext::new(&names).with_namespaces(namespaces);
    let mut parsed = parse(&format!(
        "xs:dayTimeDuration('PT{F28}S') + xs:dayTimeDuration('PT59S')"
    ))
    .unwrap();
    let mut binder = NameBinder::new();
    bind_node(&mut parsed.arena, parsed.root, &ctx, &mut binder).unwrap();
    let mut dynamic: DynamicContext<'_, RoXmlNavigator<'static>> =
        DynamicContext::new(&ctx, binder.len());
    assert!(eval_node(&parsed.arena, parsed.root, &mut dynamic).is_ok());
    let warnings = dynamic.take_warnings();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(warnings[0].code(), CODE);
    assert!(dynamic.take_warnings().is_empty(), "taken");

    // Taking them ends the evaluation: the same site warns again.
    assert!(eval_node(&parsed.arena, parsed.root, &mut dynamic).is_ok());
    assert_eq!(dynamic.take_warnings().len(), 1, "a second evaluation");

    // Two expressions in one context: their sites are told apart, even
    // where their node numbers coincide.
    let mut other = parse(&format!(
        "xs:dayTimeDuration('PT{F28}S') + xs:dayTimeDuration('PT58S')"
    ))
    .unwrap();
    bind_node(&mut other.arena, other.root, &ctx, &mut binder).unwrap();
    assert_eq!(other.root, parsed.root, "the same node number");
    assert!(eval_node(&parsed.arena, parsed.root, &mut dynamic).is_ok());
    assert!(eval_node(&other.arena, other.root, &mut dynamic).is_ok());
    assert_eq!(dynamic.take_warnings().len(), 2, "two expressions");
}

// ---------------------------------------------------------------------------
// During validation: XSD 1.1 assertions and type alternatives
// ---------------------------------------------------------------------------

mod schema {
    use xsd_schema::validation::{
        drive_navigator, drive_quick_xml, CollectingValidationSink, SchemaValidator,
        SchemaValidity, ValidationFlags,
    };
    use xsd_schema::{RoXmlNavigator, SchemaSet, SchemaSetBuilder, XsdVersion};

    pub fn compile(body: &str) -> SchemaSet {
        let xsd =
            format!(r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">{body}</xs:schema>"#);
        SchemaSetBuilder::with_version(XsdVersion::V1_1)
            .add_source(&xsd, "file:///xpath_seconds_warnings.xsd")
            .expect("schema source accepted")
            .compile()
            .expect("schema compiles")
            .into_schema_set()
    }

    /// (validity, warning codes) on the streaming driver with and without
    /// PSVI typed values, and on the navigator driver.
    pub fn outcomes(
        ss: &SchemaSet,
        xml: &str,
        flags: ValidationFlags,
    ) -> Vec<(String, Vec<String>)> {
        let run = |stream: bool, flags: ValidationFlags| {
            let v = SchemaValidator::new_fragment_buffer(ss, flags);
            let mut errors = Vec::new();
            let mut warnings = Vec::new();
            let validity = {
                let mut rt = v.start_run(CollectingValidationSink {
                    errors: &mut errors,
                    warnings: &mut warnings,
                });
                if stream {
                    drive_quick_xml(xml.as_bytes(), &mut rt, ss)
                        .expect("drive completes")
                        .root_validity
                } else {
                    let doc = roxmltree::Document::parse(xml).expect("well-formed");
                    drive_navigator(&RoXmlNavigator::new(&doc), &mut rt, ss)
                        .expect("drive completes")
                        .root_validity
                }
            };
            let verdict = match (validity, errors.first()) {
                (Some(SchemaValidity::Valid), None) => "valid".to_string(),
                (_, Some(e)) => format!("invalid:{}", e.constraint),
                (other, None) => format!("{other:?}"),
            };
            (
                verdict,
                warnings.iter().map(|w| w.code.to_string()).collect(),
            )
        };
        vec![
            run(true, flags),
            run(true, flags & !ValidationFlags::BUILD_PSVI_TYPED_VALUES),
            run(false, flags),
        ]
    }
}

const F28_TIME: &str = "00:00:00.1234567890123456789012345678Z";

/// The schema of the assertion tests.
const ASSERTIONS: &str = r#"
        <xs:simpleType name="nearlyAMinute">
          <xs:restriction base="xs:dayTimeDuration">
            <xs:assertion test="$value + xs:dayTimeDuration('PT59S') gt xs:dayTimeDuration('PT59S')"/>
          </xs:restriction>
        </xs:simpleType>
        <xs:element name="r">
          <xs:complexType>
            <xs:sequence>
              <xs:element name="d" type="nearlyAMinute" minOccurs="0"/>
              <xs:element name="a" type="xs:time" minOccurs="0"/>
              <xs:element name="b" type="xs:time" minOccurs="0"/>
            </xs:sequence>
            <xs:attribute name="s" type="nearlyAMinute"/>
            <xs:attribute name="n" type="negativeOnly"/>
            <xs:assert test="not(a) or (a - b) gt xs:dayTimeDuration('PT1S')"/>
          </xs:complexType>
        </xs:element>
        <xs:simpleType name="negativeOnly">
          <xs:restriction base="xs:dayTimeDuration">
            <xs:assertion test="$value + xs:dayTimeDuration('PT59S') lt xs:dayTimeDuration('PT59S')"/>
          </xs:restriction>
        </xs:simpleType>
        <xs:element name="o">
          <xs:complexType>
            <xs:sequence>
              <xs:element ref="r"/>
            </xs:sequence>
            <xs:assert test="true()"/>
          </xs:complexType>
        </xs:element>"#;

#[test]
fn assertions_report_a_truncated_seconds_field() {
    use xsd_schema::validation::ValidationFlags;
    let ss = schema::compile(ASSERTIONS);
    let w = |n: usize| vec!["xs-seconds-capacity".to_string(); n];
    let cases = [
        // A simple type's assertion facet: element and attribute values.
        (format!("<r><d>PT{F28}S</d></r>"), "valid", w(1)),
        (format!(r#"<r s="PT{F28}S"/>"#), "valid", w(1)),
        ("<r><d>PT0.5S</d></r>".to_string(), "valid", w(0)),
        // A complex type's assertion.
        (
            format!("<r><a>00:00:59Z</a><b>{F28_TIME}</b></r>"),
            "valid",
            w(1),
        ),
        (
            "<r><a>00:00:59Z</a><b>00:00:00.5Z</b></r>".to_string(),
            "valid",
            w(0),
        ),
        // The same assertion on an element inside another asserted element:
        // evaluated when the outer one closes (a deferred frame).
        (
            format!("<o><r><a>00:00:59Z</a><b>{F28_TIME}</b></r></o>"),
            "valid",
            w(1),
        ),
        (
            "<o><r><a>00:00:59Z</a><b>00:00:00.5Z</b></r></o>".to_string(),
            "valid",
            w(0),
        ),
        // A value the assertion rejects keeps its warning.
        (
            format!(r#"<r n="PT{F28}S"/>"#),
            "invalid:cvc-assertion",
            w(1),
        ),
        (
            r#"<r n="PT0.5S"/>"#.to_string(),
            "invalid:cvc-assertion",
            w(0),
        ),
    ];
    let mut failures = Vec::new();
    for (xml, verdict, warnings) in &cases {
        for (got, codes) in schema::outcomes(&ss, xml, ValidationFlags::default()) {
            if got != *verdict || codes != *warnings {
                failures.push(format!(
                    "{xml}: expected {verdict} {warnings:?}, got {got} {codes:?}"
                ));
            }
        }
    }
    // Without REPORT_WARNINGS, none.
    for (got, codes) in schema::outcomes(
        &ss,
        &format!("<r><d>PT{F28}S</d></r>"),
        ValidationFlags::default() & !ValidationFlags::REPORT_WARNINGS,
    ) {
        if got != "valid" || !codes.is_empty() {
            failures.push(format!("REPORT_WARNINGS off: got {got} {codes:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn type_alternatives_report_a_truncated_seconds_field() {
    use xsd_schema::validation::ValidationFlags;
    let ss = schema::compile(
        r#"
        <xs:complexType name="base">
          <xs:simpleContent>
            <xs:extension base="xs:string">
              <xs:attribute name="a" type="xs:time"/>
              <xs:attribute name="b" type="xs:time"/>
            </xs:extension>
          </xs:simpleContent>
        </xs:complexType>
        <xs:complexType name="long">
          <xs:simpleContent>
            <xs:restriction base="base">
              <xs:enumeration value="long"/>
            </xs:restriction>
          </xs:simpleContent>
        </xs:complexType>
        <xs:element name="r" type="base">
          <xs:alternative test="xs:time(@a) - xs:time(@b) gt xs:dayTimeDuration('PT1S')" type="long"/>
        </xs:element>"#,
    );
    let w = |n: usize| vec!["xs-seconds-capacity".to_string(); n];
    let cases = [
        (
            format!(r#"<r a="00:00:59Z" b="{F28_TIME}">long</r>"#),
            "valid",
            w(1),
        ),
        (
            r#"<r a="00:00:59Z" b="00:00:00.5Z">long</r>"#.to_string(),
            "valid",
            w(0),
        ),
    ];
    let mut failures = Vec::new();
    for (xml, verdict, warnings) in &cases {
        for (got, codes) in schema::outcomes(&ss, xml, ValidationFlags::default()) {
            if got != *verdict || codes != *warnings {
                failures.push(format!(
                    "{xml}: expected {verdict} {warnings:?}, got {got} {codes:?}"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_validation_warning_names_its_expression_and_its_item() {
    use xsd_schema::validation::{
        drive_quick_xml, CollectingValidationSink, SchemaValidator, ValidationFlags,
    };
    let ss = schema::compile(ASSERTIONS);
    let xml =
        format!(r#"<o><r s="PT{F28}S"><d>PT{F28}S</d><a>00:00:59Z</a><b>{F28_TIME}</b></r></o>"#);
    let v = SchemaValidator::new_fragment_buffer(&ss, ValidationFlags::default());
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    {
        let mut rt = v.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        drive_quick_xml(xml.as_bytes(), &mut rt, &ss).expect("drive completes");
    }
    assert!(errors.is_empty(), "{errors:?}");
    let got: Vec<String> = warnings
        .iter()
        .map(|w| {
            let source = w.message.split(": ").next().unwrap_or_default();
            format!(
                "{} {} {} | {source}",
                w.code,
                w.subject().map(|s| s.to_string()).unwrap_or_default(),
                w.instance_path().map(|p| p.to_string()).unwrap_or_default(),
            )
        })
        .collect();
    let simple = "assertion '$value + xs:dayTimeDuration('PT59S') gt xs:dayTimeDuration('PT59S')'";
    let complex = "assertion 'not(a) or (a - b) gt xs:dayTimeDuration('PT1S')'";
    assert_eq!(
        got,
        [
            format!("{CODE} @Q{{}}s /Q{{}}o[1]/Q{{}}r[1]/@Q{{}}s | {simple} (/o/r)"),
            format!("{CODE} Q{{}}d /Q{{}}o[1]/Q{{}}r[1]/Q{{}}d[1] | {simple} (/o/r/d)"),
            // The nested frame, evaluated when `o` closes, keeps `r`.
            format!("{CODE} Q{{}}r /Q{{}}o[1]/Q{{}}r[1] | {complex} (/o/r)"),
        ]
    );
}
