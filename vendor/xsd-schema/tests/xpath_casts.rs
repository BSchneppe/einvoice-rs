#![cfg(feature = "xsd11")]
//! XPath casts at the edges of the value spaces, and casts to `xs:QName` /
//! `xs:NOTATION` from a computed value.
//!
//! - `xs:integer` has no size limit here (`doc/IMPLEMENTATION_LIMITS.md` §2:
//!   "`xs:integer` values have no size limit (`BigInt`)"), and neither have
//!   `xs:nonNegativeInteger`, `xs:positiveInteger`, `xs:nonPositiveInteger`
//!   and `xs:negativeInteger`, whose value spaces are unbounded on one side.
//! - F&O 1.0 §17.1.3.3: casting `xs:float` / `xs:double` to `xs:decimal`
//!   gives "the xs:decimal value, within the set of xs:decimal values that the
//!   implementation is capable of representing, that is numerically closest
//!   to SV. If two values are equally close, then the one that is closest to
//!   zero is chosen. If SV is too large to be accommodated as an xs:decimal
//!   … an error is raised [err:FOCA0001]. If SV is one of the special
//!   xs:float or xs:double values NaN, INF, or -INF, an error is raised
//!   [err:FOCA0002]."
//! - F&O §17.1.1: "In casting to a duration value, if the value is too large
//!   or too small to be represented by the implementation, error
//!   [err:FODT0002] is raised", and "In casting to xs:date, xs:dateTime,
//!   xs:gYear, or xs:gYearMonth (or types derived from these), if the value is
//!   too large or too small to be represented by the implementation, error
//!   [err:FODT0001] is raised."
//! - XPath 2.0 §3.10.2: "If the target type of a cast expression is
//!   xs:QName, or is a type that is derived from xs:QName or xs:NOTATION, and
//!   if the base type of the input is not the same as the base type of the
//!   target type, then the input expression must be a string literal
//!   [err:XPTY0004]"; §3.10.4 says the same of the constructor functions, and
//!   §3.10.3: "If the target type of a castable expression is xs:QName, or is
//!   a type that is derived from xs:QName or xs:NOTATION, and the input
//!   argument of the expression is of type xs:string but it is not a literal
//!   string, the result of the castable expression is false."

use xsd_schema::namespace::context::NamespaceContextSnapshot;
use xsd_schema::namespace::table::NameTable;
use xsd_schema::types::value::TimezoneOffset;
use xsd_schema::xpath::{RoXmlNavigator, XPathContext, XPathExpr};
use xsd_schema::{SchemaSet, SchemaSetBuilder, XmlItem, XsdVersion};

const XS: &str = "http://www.w3.org/2001/XMLSchema";

fn run(ctx: &XPathContext<'_>, expr: &str) -> String {
    let compiled = match XPathExpr::compile(expr, ctx) {
        Ok(c) => c,
        Err(e) => return format!("ERR:{}", e.error_code().unwrap_or("?")),
    };
    match compiled.evaluator(ctx).run::<RoXmlNavigator<'static>>() {
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
}

fn eval(expr: &str) -> String {
    let names = NameTable::new();
    let mut namespaces = NamespaceContextSnapshot::default();
    namespaces.bindings.push((names.add("xs"), names.add(XS)));
    let ctx = XPathContext::new(&names)
        .with_namespaces(namespaces)
        .with_implicit_timezone(TimezoneOffset::UTC);
    run(&ctx, expr)
}

fn check(rows: &[(&str, &str)], eval: impl Fn(&str) -> String) {
    let failures: Vec<String> = rows
        .iter()
        .filter_map(|(expr, expected)| {
            let got = eval(expr);
            (got != *expected).then(|| format!("{expr}\n    expected {expected}, got {got}"))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

const E56: &str = "100000000000000000000000000000000000000000000000000000000";

#[test]
fn integer_subtypes_unbounded_on_one_side_have_no_size_limit() {
    let rows: Vec<(String, String)> = [
        ("xs:nonNegativeInteger('{E}')", "{E}"),
        ("xs:positiveInteger('{E}')", "{E}"),
        ("xs:nonPositiveInteger('-{E}')", "-{E}"),
        ("xs:negativeInteger('-{E}')", "-{E}"),
        ("xs:integer('{E}') cast as xs:nonNegativeInteger", "{E}"),
        ("{E} cast as xs:positiveInteger", "{E}"),
        ("-{E} cast as xs:negativeInteger", "-{E}"),
        (
            "xs:positiveInteger({E}) instance of xs:positiveInteger",
            "true",
        ),
        ("xs:nonNegativeInteger('{E}') + 1", "{E}1"),
        (
            "xs:double('1e40') cast as xs:nonNegativeInteger",
            "10000000000000000303786028427003666890752",
        ),
        (
            "xs:double('-1e40') cast as xs:negativeInteger",
            "-10000000000000000303786028427003666890752",
        ),
        ("'{E}' castable as xs:positiveInteger", "true"),
        // The bounded side still bounds.
        ("xs:nonNegativeInteger('-1')", "ERR:FORG0001"),
        ("xs:positiveInteger('0')", "ERR:FORG0001"),
        ("xs:nonPositiveInteger('1')", "ERR:FORG0001"),
        ("xs:negativeInteger('-0')", "ERR:FORG0001"),
        ("xs:nonNegativeInteger('-{E}')", "ERR:FORG0001"),
        ("xs:negativeInteger('{E}')", "ERR:FORG0001"),
        ("'-{E}' castable as xs:positiveInteger", "false"),
        // The types with both bounds.
        (
            "xs:unsignedLong('18446744073709551615')",
            "18446744073709551615",
        ),
        ("xs:unsignedLong('18446744073709551616')", "ERR:FORG0001"),
        ("xs:long('-9223372036854775809')", "ERR:FORG0001"),
        ("xs:unsignedLong('{E}')", "ERR:FORG0001"),
    ]
    .iter()
    .map(|(e, x)| (e.replace("{E}", E56), x.replace("{E}", E56)))
    .collect();
    // `{E}1` stands for E + 1.
    let e_plus_one = format!("{}1", &E56[..E56.len() - 1]);
    let rows: Vec<(String, String)> = rows
        .into_iter()
        .map(|(e, x)| {
            let x = if x == format!("{E56}1") {
                e_plus_one.clone()
            } else {
                x
            };
            (e, x)
        })
        .collect();
    let borrowed: Vec<(&str, &str)> = rows.iter().map(|(e, x)| (e.as_str(), x.as_str())).collect();
    check(&borrowed, eval);
}

#[test]
fn float_and_double_to_decimal_follow_f_and_o_17_1_3_3() {
    check(
        &[
            // Too large: FOCA0001.
            ("xs:decimal(xs:double('1e40'))", "ERR:FOCA0001"),
            ("xs:decimal(xs:double('-1e40'))", "ERR:FOCA0001"),
            ("xs:decimal(7.93e28)", "ERR:FOCA0001"),
            ("xs:decimal(xs:float('1e30'))", "ERR:FOCA0001"),
            ("xs:double('1e40') cast as xs:decimal", "ERR:FOCA0001"),
            ("xs:double('1e40') castable as xs:decimal", "false"),
            (
                "xs:decimal(xs:double('1.7976931348623157E308'))",
                "ERR:FOCA0001",
            ),
            // Special values: FOCA0002.
            ("xs:decimal(xs:double('INF'))", "ERR:FOCA0002"),
            ("xs:decimal(xs:double('-INF'))", "ERR:FOCA0002"),
            ("xs:decimal(xs:double('NaN'))", "ERR:FOCA0002"),
            ("xs:decimal(xs:float('NaN'))", "ERR:FOCA0002"),
            // In range.
            (
                "xs:decimal(7.92e28) lt 79228162514264337593543950335",
                "true",
            ),
            ("xs:decimal(xs:double('1.5'))", "1.5"),
            ("xs:decimal(xs:double('-0.0'))", "0"),
            // Below 10^-28, the smallest positive value held: the closer of
            // 0 and ±10^-28, zero on a tie.
            (
                "xs:decimal(xs:double('6e-29'))",
                "0.0000000000000000000000000001",
            ),
            (
                "xs:decimal(xs:double('9e-29'))",
                "0.0000000000000000000000000001",
            ),
            (
                "xs:decimal(xs:double('-9e-29'))",
                "-0.0000000000000000000000000001",
            ),
            (
                "xs:decimal(xs:double('5.0000000001e-29'))",
                "0.0000000000000000000000000001",
            ),
            ("xs:decimal(xs:double('4.9999999999e-29'))", "0"),
            ("xs:decimal(xs:double('4e-29'))", "0"),
            ("xs:decimal(xs:double('-4e-29'))", "0"),
            ("xs:decimal(xs:double('1e-40'))", "0"),
            ("xs:decimal(xs:double('4.9E-324'))", "0"),
            (
                "xs:decimal(xs:float('9e-29'))",
                "0.0000000000000000000000000001",
            ),
            (
                "xs:decimal(xs:float('-6e-29'))",
                "-0.0000000000000000000000000001",
            ),
            ("xs:decimal(xs:float('1e-30'))", "0"),
            (
                "xs:decimal(xs:double('1e-28'))",
                "0.0000000000000000000000000001",
            ),
        ],
        eval,
    );
}

#[test]
fn casts_beyond_the_value_structs_raise_fodt_errors() {
    check(
        &[
            // §17.1.1, durations: FODT0002.
            (
                "xs:yearMonthDuration('P99999999999999999999Y')",
                "ERR:FODT0002",
            ),
            ("xs:yearMonthDuration('P4294967296Y')", "ERR:FODT0002"),
            ("xs:yearMonthDuration('-P4294967296Y')", "ERR:FODT0002"),
            ("xs:yearMonthDuration('P4294967295Y')", "P4294967295Y"),
            ("xs:duration('P4294967296D')", "ERR:FODT0002"),
            (
                "xs:dayTimeDuration('PT79228162514264337593543950336S')",
                "ERR:FODT0002",
            ),
            ("xs:dayTimeDuration('PT4294967296H')", "ERR:FODT0002"),
            ("'P4294967296Y' castable as xs:yearMonthDuration", "false"),
            (
                "xs:untypedAtomic('P4294967296M') cast as xs:yearMonthDuration",
                "ERR:FODT0002",
            ),
            // §17.1.1, dates: FODT0001.
            ("xs:date('2147483648-01-01')", "ERR:FODT0001"),
            ("xs:dateTime('-2147483649-01-01T00:00:00')", "ERR:FODT0001"),
            ("xs:gYear('99999999999999999999')", "ERR:FODT0001"),
            ("xs:gYearMonth('2147483648-01')", "ERR:FODT0001"),
            ("xs:date('2147483647-12-31')", "2147483647-12-31"),
            // Not a value of the type at all: FORG0001 as before.
            ("xs:yearMonthDuration('P1D')", "ERR:FORG0001"),
            ("xs:yearMonthDuration('P')", "ERR:FORG0001"),
            ("xs:duration('P1Y2')", "ERR:FORG0001"),
            ("xs:date('2000-13-01')", "ERR:FORG0001"),
            ("xs:date('2000-02-30')", "ERR:FORG0001"),
            ("xs:gYear('02000')", "ERR:FORG0001"),
            ("xs:dateTime('2000-01-01T25:00:00')", "ERR:FORG0001"),
        ],
        eval,
    );
}

// ---------------------------------------------------------------------------
// C32: xs:QName / xs:NOTATION from a computed value
// ---------------------------------------------------------------------------

#[test]
fn a_qname_cast_needs_a_string_literal_or_a_qname() {
    check(
        &[
            // A string literal.
            ("xs:QName('xs:integer')", "xs:integer"),
            ("'xs:integer' cast as xs:QName", "xs:integer"),
            ("'xs:integer' castable as xs:QName", "true"),
            ("xs:QName('local')", "local"),
            // A computed xs:string: XPTY0004; castable is false.
            ("for $q in 'xs:integer' return xs:QName($q)", "ERR:XPTY0004"),
            (
                "for $q in 'xs:integer' return $q cast as xs:QName",
                "ERR:XPTY0004",
            ),
            (
                "for $q in 'xs:integer' return $q castable as xs:QName",
                "false",
            ),
            ("concat('xs:', 'integer') cast as xs:QName", "ERR:XPTY0004"),
            ("xs:QName(concat('xs:', 'integer'))", "ERR:XPTY0004"),
            ("xs:string('xs:integer') cast as xs:QName", "ERR:XPTY0004"),
            ("xs:QName(xs:string('xs:integer'))", "ERR:XPTY0004"),
            ("xs:QName(string(xs:QName('xs:integer')))", "ERR:XPTY0004"),
            ("concat('xs:', 'integer') castable as xs:QName", "false"),
            // An xs:untypedAtomic is never castable (F&O §17.1, the table).
            (
                "xs:untypedAtomic('xs:integer') cast as xs:QName",
                "ERR:XPTY0004",
            ),
            (
                "for $q in xs:untypedAtomic('xs:integer') return $q cast as xs:QName",
                "ERR:XPTY0004",
            ),
            (
                "xs:untypedAtomic('xs:integer') castable as xs:QName",
                "false",
            ),
            // Another type: XPTY0004, castable false (as before).
            ("xs:QName(1)", "ERR:XPTY0004"),
            ("1 castable as xs:QName", "false"),
            // An xs:QName value: the same base type.
            ("xs:QName(xs:QName('xs:integer'))", "xs:integer"),
            (
                "for $q in xs:QName('xs:integer') return $q cast as xs:QName",
                "xs:integer",
            ),
            (
                "for $q in xs:QName('xs:integer') return $q castable as xs:QName",
                "true",
            ),
            (
                "for $q in xs:QName('xs:integer') return xs:QName($q) eq $q",
                "true",
            ),
            // The empty sequence.
            ("for $q in () return $q cast as xs:QName?", ""),
            ("() castable as xs:QName?", "true"),
            // xs:NOTATION has no constructor (XPST0017) and is no cast target
            // (XPST0080), whatever the operand.
            ("xs:NOTATION('xs:integer')", "ERR:XPST0017"),
            ("'xs:integer' cast as xs:NOTATION", "ERR:XPST0080"),
            (
                "for $q in 'xs:integer' return $q cast as xs:NOTATION",
                "ERR:XPST0080",
            ),
            ("'xs:integer' castable as xs:NOTATION", "ERR:XPST0080"),
        ],
        eval,
    );
}

const USER_TYPES: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
    xmlns:t="urn:t" targetNamespace="urn:t">
  <xs:notation name="png" public="image/png"/>
  <xs:notation name="gif" public="image/gif"/>
  <xs:simpleType name="format">
    <xs:restriction base="xs:NOTATION">
      <xs:enumeration value="t:png"/>
      <xs:enumeration value="t:gif"/>
    </xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="name">
    <xs:restriction base="xs:QName"/>
  </xs:simpleType>
  <xs:simpleType name="otherFormat">
    <xs:restriction base="xs:NOTATION">
      <xs:enumeration value="t:png"/>
    </xs:restriction>
  </xs:simpleType>
</xs:schema>"#;

fn compile(version: XsdVersion, xsd: &str) -> SchemaSet {
    SchemaSetBuilder::with_version(version)
        .add_source(xsd, "file:///xpath_casts.xsd")
        .expect("schema source accepted")
        .compile()
        .expect("schema compiles")
        .into_schema_set()
}

/// Types derived from `xs:QName` and `xs:NOTATION`, in both XSD versions.
#[test]
fn user_types_derived_from_qname_and_notation_need_a_literal_too() {
    for version in [XsdVersion::V1_0, XsdVersion::V1_1] {
        let ss = compile(version, USER_TYPES);
        let mut namespaces = NamespaceContextSnapshot::default();
        namespaces
            .bindings
            .push((ss.name_table.add("xs"), ss.name_table.add(XS)));
        namespaces
            .bindings
            .push((ss.name_table.add("t"), ss.name_table.add("urn:t")));
        let ctx = XPathContext::new(&ss.name_table)
            .with_schema_set(&ss)
            .with_namespaces(namespaces);
        let rows = [
            ("t:format('t:png')", "t:png"),
            ("'t:gif' cast as t:format", "t:gif"),
            ("'t:jpeg' castable as t:format", "false"),
            ("t:name('t:x')", "t:x"),
            ("for $v in 't:png' return t:format($v)", "ERR:XPTY0004"),
            (
                "for $v in 't:png' return $v cast as t:format",
                "ERR:XPTY0004",
            ),
            ("for $v in 't:png' return $v castable as t:format", "false"),
            ("for $v in 't:x' return t:name($v)", "ERR:XPTY0004"),
            (
                "for $v in xs:untypedAtomic('t:x') return $v cast as t:name",
                "ERR:XPTY0004",
            ),
            // The same base type: allowed.
            (
                "for $v in t:format('t:png') return $v cast as t:otherFormat",
                "t:png",
            ),
            ("for $v in xs:QName('t:x') return t:name($v)", "t:x"),
            ("for $v in t:name('t:x') return xs:QName($v)", "t:x"),
            // QName and NOTATION do not cast to each other.
            (
                "for $v in xs:QName('t:png') return $v cast as t:format",
                "ERR:XPTY0004",
            ),
        ];
        let failures: Vec<String> = rows
            .iter()
            .filter_map(|(expr, expected)| {
                let got = run(&ctx, expr);
                (got != *expected)
                    .then(|| format!("{version:?}: {expr}\n    expected {expected}, got {got}"))
            })
            .collect();
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}

/// XSD 1.1 assertions that build an `xs:QName` from a literal keep working;
/// one that builds it from a computed string is a type error, so the
/// assertion fails (Structures §3.13.4.1: an error makes it false).
#[test]
fn assertions_with_qname_constructors() {
    use xsd_schema::validation::{
        drive_quick_xml, CollectingValidationSink, SchemaValidator, SchemaValidity, ValidationFlags,
    };
    let ss = compile(
        XsdVersion::V1_1,
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
            xmlns:t="urn:t" targetNamespace="urn:t" elementFormDefault="qualified">
          <xs:simpleType name="xsName">
            <xs:restriction base="xs:QName">
              <xs:assertion test="$value eq xs:QName('xs:integer') or $value eq xs:QName('t:local')"/>
            </xs:restriction>
          </xs:simpleType>
          <xs:element name="r">
            <xs:complexType>
              <xs:sequence>
                <xs:element name="n" type="t:xsName" minOccurs="0"/>
              </xs:sequence>
              <xs:attribute name="s" type="xs:string"/>
              <xs:assert test="not(@s) or xs:QName(@s) eq xs:QName('xs:integer')"/>
              <xs:assert test="xs:QName('t:local') eq QName('urn:t', 'local')"/>
            </xs:complexType>
          </xs:element>
        </xs:schema>"#,
    );
    let verdict = |xml: &str| {
        let v = SchemaValidator::new_fragment_buffer(&ss, ValidationFlags::default());
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let validity = {
            let mut rt = v.start_run(CollectingValidationSink {
                errors: &mut errors,
                warnings: &mut warnings,
            });
            drive_quick_xml(xml.as_bytes(), &mut rt, &ss)
                .expect("drive completes")
                .root_validity
        };
        match (validity, errors.first()) {
            (Some(SchemaValidity::Valid), None) => "valid".to_string(),
            (_, Some(e)) => format!("invalid:{}", e.constraint),
            (other, None) => format!("{other:?}"),
        }
    };
    let ns = r#"xmlns="urn:t" xmlns:t="urn:t" xmlns:xs="http://www.w3.org/2001/XMLSchema""#;
    assert_eq!(verdict(&format!("<r {ns}><n>xs:integer</n></r>")), "valid");
    assert_eq!(verdict(&format!("<r {ns}><n>t:local</n></r>")), "valid");
    assert_eq!(
        verdict(&format!("<r {ns}><n>t:other</n></r>")),
        "invalid:cvc-assertion"
    );
    // `xs:QName(@s)`: the attribute's typed value is an xs:string, not a
    // literal — XPTY0004, and the assertion fails.
    assert_eq!(
        verdict(&format!(r#"<r {ns} s="xs:integer"/>"#)),
        "invalid:cvc-assertion"
    );
}
