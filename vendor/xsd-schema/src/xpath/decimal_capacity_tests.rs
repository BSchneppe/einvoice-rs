//! XPath on `xs:decimal` values beyond `rust_decimal`'s capacity.
//!
//! The validator carries a valid `xs:decimal` that `rust_decimal::Decimal`
//! cannot hold exactly as its canonical representation (an `xs:string`
//! payload with the `xs:decimal` type code). XPath must never compute with a
//! rounded stand-in:
//!
//! - comparisons need no arithmetic and are decided on the exact value;
//! - conversions with a defined result — to `xs:string` (the canonical form,
//!   F&O §17.1.2), `xs:boolean`, `xs:integer` (truncation),
//!   `xs:double`/`xs:float` (nearest) — give it;
//! - arithmetic and the rounding functions raise FOAR0002 (F&O §6.2: "For
//!   xs:decimal operations, overflow behavior ·must· raise an error
//!   \[err:FOAR0002\]"); a division by zero is FOAR0001 whatever the dividend;
//! - a string cast to `xs:decimal` (and a decimal literal, XPath 2.0 §3.1.1)
//!   too large raises FOAR0002, one too small to be held (a non-zero
//!   magnitude below 10^-28) underflows to 0 (F&O §6.2: "On underflow, 0.0
//!   must be returned"), and any other value with more digits than are held
//!   raises FOCA0006 (F&O §17.1.1 allows rounding or the error) — never
//!   rounding.
//!
//! None of it may panic: 0.2.2 panicked on `Decimal::MAX + 1`.

use bumpalo::Bump;

use crate::document::typed_builder::build_typed_document;
use crate::document::BufferDocumentOptions;
use crate::namespace::context::NamespaceContextSnapshot;
use crate::pipeline::load_and_process_schema;
use crate::schema::SchemaSet;
use crate::types::validators::{DecimalValidator, TypeValidator};
use crate::types::value::XmlValue;
use crate::xpath::api::XPathExpr;
use crate::xpath::functions::XPathValue;
use crate::xpath::iterator::XmlItem;
use crate::xpath::{DomNavigator, RoXmlNavigator, XPathContext, XPathError};

const HUGE: &str = "10000000000000000000000000000000000000000";
const PRECISE: &str = "0.12345678901234567890123456789";

fn rendered<N: DomNavigator>(result: Result<XPathValue<N>, XPathError>) -> String {
    match result {
        Err(e) => format!(
            "error:{}",
            e.error_code()
                .map(str::to_string)
                .unwrap_or_else(|| format!("<no code> {e}"))
        ),
        Ok(value) if value.is_empty() => "()".to_string(),
        Ok(value) => value
            .into_vec()
            .iter()
            .map(|item| match item {
                XmlItem::Atomic(v) => v.to_string_value(),
                XmlItem::Node(_) => "<node>".to_string(),
            })
            .collect::<Vec<_>>()
            .join("|"),
    }
}

fn context(names: &crate::namespace::table::NameTable) -> NamespaceContextSnapshot {
    let mut namespaces = NamespaceContextSnapshot::default();
    namespaces.bindings.push((
        names.add("xs"),
        names.add("http://www.w3.org/2001/XMLSchema"),
    ));
    namespaces
}

/// Evaluate every `(expression, expected)` pair with `$h` bound to 10^40,
/// `$h2` to `+10^40.00`, `$p` to a 29-fraction-digit value, `$s` to
/// `-0000.000…00100` (10^-30 after the point, spelled with redundant zeros) —
/// all four as the validator types them, as canonical strings — and `$one`
/// to `1`.
fn check_vars(cases: &[(&str, &str)]) {
    let names = crate::namespace::table::NameTable::new();
    let ctx = XPathContext::new(&names).with_namespaces(context(&names));
    let value = |s: &str| DecimalValidator.validate(s).expect("valid xs:decimal");
    let vars: [(&str, XmlValue); 5] = [
        ("h", value(HUGE)),
        ("h2", value(&format!("+{HUGE}.00"))),
        ("p", value(PRECISE)),
        ("s", value(&format!("-0000.{}100", "0".repeat(29)))),
        ("one", value("1")),
    ];
    assert_eq!(
        vars[0].1.as_decimal(),
        None,
        "$h is carried in lexical form"
    );
    let doc = roxmltree::Document::parse("<r/>").expect("well-formed");
    let mut wrong = Vec::new();
    for (expr, expected) in cases {
        let names_only: Vec<&str> = vars.iter().map(|(n, _)| *n).collect();
        let got = match XPathExpr::compile_with_vars(expr, &ctx, &names_only) {
            Err(e) => rendered::<RoXmlNavigator<'_>>(Err(e)),
            Ok(compiled) => {
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    rendered(compiled.evaluator(&ctx).run_with_node_and_setup(
                        Some(RoXmlNavigator::new(&doc)),
                        |eval| {
                            for (name, value) in &vars {
                                eval.set_variable_by_name(
                                    name,
                                    XPathValue::from_atomic(value.clone()),
                                )
                                .expect("declared");
                            }
                        },
                    ))
                }));
                outcome.unwrap_or_else(|_| "PANIC".to_string())
            }
        };
        if got != *expected {
            wrong.push(format!("{expr}  =>  {got}   (expected {expected})"));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} of {} cases wrong:\n{}",
        wrong.len(),
        cases.len(),
        wrong.join("\n")
    );
}

#[test]
fn comparisons_are_exact() {
    check_vars(&[
        ("$h eq $h2", "true"),
        ("$h = $h2", "true"),
        ("$h ne $h2", "false"),
        ("$h gt $one", "true"),
        ("$h lt $one", "false"),
        ("$h ge 79228162514264337593543950335", "true"),
        // xs:integer literals are exact BigInts; compared by value.
        ("$h eq 10000000000000000000000000000000000000000", "true"),
        ("$h lt 10000000000000000000000000000000000000001", "true"),
        ("$p gt 0.1234567890123456789012345678", "true"),
        ("$p lt 0.1234567890123456789012345679", "true"),
        ("$p eq $p", "true"),
        ("$p gt 0", "true"),
        ("-1 lt $h", "true"),
        // Against xs:double the decimal is promoted (F&O §6.2): nearest double.
        ("$h eq 1.0e40", "true"),
        ("$h gt xs:float(1)", "true"),
        ("max(($one, $h, $p))", HUGE),
        ("min(($one, $h, $p))", PRECISE),
        ("count(distinct-values(($h, $h2, $one)))", "2"),
        ("index-of(($one, $h2), $h)", "2"),
        ("deep-equal(($h, $one), ($h2, 1))", "true"),
        ("some $x in ($one, $h2) satisfies $x eq $h", "true"),
    ]);
}

#[test]
fn conversions_with_a_defined_result() {
    let truncated = "0";
    check_vars(&[
        ("string($h)", HUGE),
        // F&O §17.1.2: xs:decimal → xs:string is the canonical representation
        // (the integer form when the value is integral), not the spelling.
        ("xs:string($h2)", HUGE),
        ("string($h2)", HUGE),
        ("xs:untypedAtomic($h2)", HUGE),
        ("concat($h2, '')", HUGE),
        ("string-join(($h2, $p), ' ')", &format!("{HUGE} {PRECISE}")),
        ("string-length(string($h2))", "41"),
        ("string($s)", &format!("-0.{}1", "0".repeat(29))),
        (
            "xs:string($s) eq '-0.000000000000000000000000000001'",
            "true",
        ),
        ("$h2 cast as xs:decimal", HUGE),
        // In-capacity decimals have always been canonical.
        ("string(xs:decimal('+0010.500'))", "10.5"),
        ("string(xs:decimal('5.0'))", "5"),
        ("$h instance of xs:decimal", "true"),
        ("$h cast as xs:decimal", HUGE),
        ("$h cast as xs:integer", HUGE),
        (
            "xs:integer($h2) + 1",
            "10000000000000000000000000000000000000001",
        ),
        ("xs:integer($p)", truncated),
        ("xs:boolean($h)", "true"),
        ("boolean($p)", "true"),
        ("if ($h) then 1 else 0", "1"),
        ("xs:double($h)", "1.0E40"),
        ("number($h)", "1.0E40"),
        ("$h + 1.0e0", "1.0E40"),
        ("xs:float($p)", "0.12345679"),
        ("$h castable as xs:double", "true"),
        ("xs:long($h)", "error:FORG0001"),
        ("sum(($h))", HUGE),
    ]);
}

#[test]
fn arithmetic_raises_foar0002() {
    check_vars(&[
        ("$h + 1", "error:FOAR0002"),
        ("$h - $one", "error:FOAR0002"),
        ("$h * 2", "error:FOAR0002"),
        ("$h div 2", "error:FOAR0002"),
        ("$h idiv 2", "error:FOAR0002"),
        ("$h mod 2", "error:FOAR0002"),
        ("-$h", "error:FOAR0002"),
        ("+$p", "error:FOAR0002"),
        ("$p + 0", "error:FOAR0002"),
        ("abs($h)", "error:FOAR0002"),
        ("floor($p)", "error:FOAR0002"),
        ("ceiling($p)", "error:FOAR0002"),
        ("round($p)", "error:FOAR0002"),
        ("round-half-to-even($p, 2)", "error:FOAR0002"),
        ("sum(($h, 1))", "error:FOAR0002"),
        ("avg(($h, $one))", "error:FOAR0002"),
        ("avg(($h))", "error:FOAR0002"),
        ("xs:dayTimeDuration('PT1S') * $h", "error:FODT0002"),
        // F&O §6.2.4–§6.2.6: a decimal or integer division by zero is
        // FOAR0001 — certain, whatever the size of the dividend.
        ("$h div 0", "error:FOAR0001"),
        ("$h idiv 0", "error:FOAR0001"),
        ("$h mod 0", "error:FOAR0001"),
        ("$p div 0.0", "error:FOAR0001"),
        ("$h div -0.0", "error:FOAR0001"),
        (
            "100000000000000000000000000000000000000 div 0.0",
            "error:FOAR0001",
        ),
        // Floating-point division is IEEE: no error.
        ("$h div 0e0", "INF"),
        // A non-zero divisor beyond capacity is still the capacity error.
        ("1 div $h", "error:FOAR0002"),
    ]);
}

#[test]
fn casts_and_literals_never_round() {
    let z28 = "0".repeat(28);
    let z27 = "0".repeat(27);
    // 10^-28, the smallest positive xs:decimal held.
    let min = format!("0.{z27}1");
    check_vars(&[
        // F&O §17.1.1 and §6.2: a magnitude below 10^-28 is too small to be
        // represented — an underflow, which returns 0 (never a negative 0).
        (&format!("0.{z28}1"), "0"),
        (&format!("-0.{z28}1"), "0"),
        (&format!("0.{z28}9999"), "0"),
        (&format!("0.{z28}100000"), "0"),
        (&format!("xs:decimal('0.{z28}1')"), "0"),
        (&format!("xs:decimal('-0.{z28}1')"), "0"),
        (&format!("xs:decimal(' -.{z28}5 ') eq 0"), "true"),
        (&format!("xs:decimal(xs:untypedAtomic('0.{z28}5'))"), "0"),
        (&format!("'0.{z28}1' castable as xs:decimal"), "true"),
        (&format!("0.{z28}1 instance of xs:decimal"), "true"),
        // 10^-28 itself is held, however it is spelled.
        (&min, &min),
        (&format!("-{min}"), &format!("-{min}")),
        (&format!("0.{z27}100000000"), &min),
        (&format!("xs:decimal('0.{z27}1')"), &min),
        // At or above 10^-28 but more precise than held: FOCA0006.
        (&format!("0.{z27}15"), "error:FOCA0006"),
        (&format!("xs:decimal('-0.{z27}15')"), "error:FOCA0006"),
        // A double or float casts to the nearest decimal (F&O §17.1.3.3).
        ("xs:decimal(1.0e-40)", "0"),
        ("xs:decimal(-1.0e-40)", "0"),
        ("xs:decimal(xs:float('-1.0e-40'))", "0"),
        ("-0.0", "0"),
    ]);
    check_vars(&[
        (
            "xs:decimal('0.12345678901234567890123456789')",
            "error:FOCA0006",
        ),
        ("0.12345678901234567890123456789", "error:FOCA0006"),
        (
            "xs:decimal('1.00000000000000000000000000001')",
            "error:FOCA0006",
        ),
        (
            "xs:decimal('79228162514264337593543950336')",
            "error:FOAR0002",
        ),
        ("79228162514264337593543950336.0", "error:FOAR0002"),
        (
            "xs:decimal(100000000000000000000000000000000000000)",
            "error:FOAR0002",
        ),
        (
            "'0.12345678901234567890123456789' castable as xs:decimal",
            "false",
        ),
        // Values that fit are exact, however they are spelled.
        ("xs:decimal('1.00000000000000000000000000000000')", "1"),
        (
            "xs:decimal('0.1234567890123456789012345678')",
            "0.1234567890123456789012345678",
        ),
        (
            "xs:decimal(' 79228162514264337593543950335 ')",
            "79228162514264337593543950335",
        ),
        // Not in the lexical space at all: FORG0001, as before.
        ("xs:decimal('1e5')", "error:FORG0001"),
        ("xs:decimal('1_0')", "error:FORG0001"),
    ]);
}

/// xs:decimal has no negative zero (Datatypes §3.3.3); `rust_decimal` keeps
/// the sign of a zero result. Every decimal-producing operation gives `0`;
/// xs:float and xs:double keep IEEE's negative zero.
#[test]
fn decimal_zero_is_never_negative() {
    check_vars(&[
        ("ceiling(-0.5)", "0"),
        ("ceiling(-0.4)", "0"),
        ("string(ceiling(-0.5))", "0"),
        ("xs:string(ceiling(xs:decimal('-0.9')))", "0"),
        ("xs:double(ceiling(-0.5))", "0"),
        ("xs:untypedAtomic(ceiling(-0.5))", "0"),
        ("floor(-0.0)", "0"),
        ("round(-0.4)", "0"),
        ("round(-0.5)", "0"),
        ("round-half-to-even(-0.4)", "0"),
        ("round-half-to-even(-0.004, 2)", "0"),
        ("abs(-0.0)", "0"),
        ("-0.0", "0"),
        ("-(0.0)", "0"),
        ("-0.5 * 0", "0"),
        ("0.0 div -3", "0"),
        ("-1.5 mod 1.5", "0"),
        ("-0.3 + 0.3", "0"),
        ("avg((-0.5, 0.5))", "0"),
        ("sum((-0.5, 0.5))", "0"),
        ("xs:decimal(-0.0e0)", "0"),
        ("xs:decimal(-1.0e-40)", "0"),
        ("seconds-from-duration(xs:dayTimeDuration('-PT1M'))", "0"),
        (
            "seconds-from-duration(xs:dayTimeDuration('-PT1M1.5S'))",
            "-1.5",
        ),
        (
            "xs:dayTimeDuration('-PT0S') div xs:dayTimeDuration('PT1S')",
            "0",
        ),
        ("ceiling(-0.5) eq 0", "true"),
        // Floating point keeps the IEEE negative zero (F&O §6.4.2).
        ("-0.0e0", "-0"),
        ("ceiling(-0.5e0)", "-0"),
        ("ceiling(xs:float('-0.5'))", "-0"),
    ]);
    // A negative-zero `Decimal` made through the public constructor prints as
    // `0` too.
    let mut negative_zero = rust_decimal::Decimal::ZERO;
    negative_zero.set_sign_negative(true);
    assert!(negative_zero.is_sign_negative());
    assert_eq!(XmlValue::decimal(negative_zero).to_string_value(), "0");
}

/// F&O §6.4.5 `fn:round-half-to-even` with a negative precision `-k`: the
/// nearest multiple of 10^k, an exact midpoint going to the even multiple —
/// decided exactly, and FOAR0002 only when that multiple is beyond the range.
#[test]
fn round_half_to_even_negative_precision_is_exact() {
    let max = "79228162514264337593543950335";
    let half29 = "50000000000000000000000000000"; // 5 × 10^28
    check_vars(&[
        // Midpoint between 0 and 10^29: the even multiple is 0 (was FOAR0002).
        (
            &format!("round-half-to-even(xs:decimal('{half29}'), -29)"),
            "0",
        ),
        (
            &format!("round-half-to-even(xs:decimal('-{half29}'), -29)"),
            "0",
        ),
        (
            &format!("string(round-half-to-even(xs:decimal('-{half29}'), -29))"),
            "0",
        ),
        // Above the midpoint the multiple is ±10^29, beyond the range.
        (
            "round-half-to-even(xs:decimal('50000000000000000000000000001'), -29)",
            "error:FOAR0002",
        ),
        (
            "round-half-to-even(xs:decimal('-50000000000000000000000000001'), -29)",
            "error:FOAR0002",
        ),
        (
            "round-half-to-even(xs:decimal('49999999999999999999999999999'), -29)",
            "0",
        ),
        (
            &format!("round-half-to-even(xs:decimal('{max}'), -29)"),
            "error:FOAR0002",
        ),
        // -28: 8 × 10^28 is beyond the range; 2 × 10^28 is not.
        (
            &format!("round-half-to-even(xs:decimal('{max}'), -28)"),
            "error:FOAR0002",
        ),
        (
            "round-half-to-even(xs:decimal('5000000000000000000000000000'), -28)",
            "0",
        ),
        (
            "round-half-to-even(xs:decimal('25000000000000000000000000000'), -28)",
            "20000000000000000000000000000",
        ),
        (
            "round-half-to-even(xs:decimal('-25000000000000000000000000000'), -28)",
            "-20000000000000000000000000000",
        ),
        // -30 and beyond: always 0.
        (
            &format!("round-half-to-even(xs:decimal('{max}'), -30)"),
            "0",
        ),
        (
            &format!("round-half-to-even(xs:decimal('-{max}'), -100)"),
            "0",
        ),
        // Just above a midpoint is above it (dividing in `Decimal` first
        // rounded 0.50000000000000000000000000001 down to the midpoint).
        (
            "round-half-to-even(xs:decimal('5.0000000000000000000000000001'), -1)",
            "10",
        ),
        (
            "round-half-to-even(xs:decimal('-5.0000000000000000000000000001'), -1)",
            "-10",
        ),
        ("round-half-to-even(xs:decimal('25'), -1)", "20"),
        ("round-half-to-even(xs:decimal('35'), -1)", "40"),
        ("round-half-to-even(xs:decimal('-4'), -1)", "0"),
        ("round-half-to-even(xs:decimal('12345.6789'), -2)", "12300"),
        // xs:integer: exact, no range at all; a huge precision is just 0.
        (
            "round-half-to-even(500000000000000000000000000000000000, -35)",
            "500000000000000000000000000000000000",
        ),
        (
            "round-half-to-even(1500000000000000000000000000000000000, -36)",
            "2000000000000000000000000000000000000",
        ),
        (
            "round-half-to-even(-2500000000000000000000000000000000000, -36)",
            "-2000000000000000000000000000000000000",
        ),
        ("round-half-to-even(123, -100)", "0"),
        ("round-half-to-even(123, -2147483647)", "0"),
        ("round-half-to-even(-15, -1)", "-20"),
        // fn:round on the extremes, no negative zero.
        (
            &format!("round(xs:decimal('-{half29}'))"),
            &format!("-{half29}"),
        ),
        (&format!("round(xs:decimal('{max}'))"), max),
        ("round(xs:decimal('-0.5'))", "0"),
    ]);
}

/// A string payload with the `xs:decimal` type code that does spell a value
/// `Decimal` holds is that value: it is computed with, never reported as a
/// decimal beyond capacity (which would let an assertion be skipped). Only a
/// value no `Decimal` holds raises the capacity error.
#[test]
fn a_decimal_coded_string_that_fits_is_computed() {
    use crate::types::value::{XmlAtomicValue, XmlValueKind};
    use crate::types::XmlTypeCode;
    use crate::xpath::operators::{decimal_operand, with_decimal_capacity_marks};
    let coded = |s: &str| {
        XmlValue::new(
            XmlTypeCode::Decimal,
            XmlValueKind::Atomic(XmlAtomicValue::String(s.to_string())),
        )
    };
    with_decimal_capacity_marks(|| {
        assert_eq!(
            decimal_operand(&coded("20")).unwrap(),
            rust_decimal::Decimal::from(20)
        );
        assert_eq!(
            decimal_operand(&coded("+0020.50")).unwrap(),
            rust_decimal::Decimal::new(205, 1)
        );
        let not_a_number = decimal_operand(&coded("abc")).unwrap_err();
        assert!(!not_a_number.is_decimal_capacity_operand());
        assert!(!not_a_number.is_decimal_capacity_error());
        assert!(decimal_operand(&coded(HUGE))
            .unwrap_err()
            .is_decimal_capacity_operand());
    });
    // Outside the assertion scope: the plain FOAR0002 for a value beyond
    // capacity, and the value for one that fits.
    assert!(matches!(
        decimal_operand(&coded(HUGE)),
        Err(XPathError::FOAR0002)
    ));
    assert_eq!(
        crate::types::convert::TypeConverter::new()
            .to_decimal(&coded("20"))
            .unwrap(),
        rust_decimal::Decimal::from(20)
    );
    // Through XPath: arithmetic and comparison.
    let names = crate::namespace::table::NameTable::new();
    let ctx = XPathContext::new(&names).with_namespaces(context(&names));
    let doc = roxmltree::Document::parse("<r/>").expect("well-formed");
    let expr = XPathExpr::compile_with_vars("$m + 1", &ctx, &["m"]).expect("compiles");
    let got = rendered(with_decimal_capacity_marks(|| {
        expr.evaluator(&ctx)
            .run_with_node_and_setup(Some(RoXmlNavigator::new(&doc)), |eval| {
                eval.set_variable_by_name("m", XPathValue::from_atomic(coded("20")))
                    .expect("declared");
            })
    }));
    assert_eq!(got, "21");
}

#[test]
fn decimal_overflow_is_foar0002_not_a_panic() {
    let max = "xs:decimal('79228162514264337593543950335')";
    check_vars(&[
        (&format!("{max} + 1"), "error:FOAR0002"),
        (&format!("{max} - -1"), "error:FOAR0002"),
        (&format!("{max} * 10"), "error:FOAR0002"),
        (&format!("{max} div 0.1"), "error:FOAR0002"),
        (&format!("{max} idiv 0.1"), "error:FOAR0002"),
        (&format!("{max} - 1"), "79228162514264337593543950334"),
        // 2^96 − 1 ≡ 0 (mod 7), since 2^3 ≡ 1.
        (&format!("{max} mod 7"), "0"),
        (
            "100000000000000000000000000000000000000 div 3",
            "error:FOAR0002",
        ),
        (
            "100000000000000000000000000000000000000 + 0.5",
            "error:FOAR0002",
        ),
        (
            "100000000000000000000000000000000000000 = 100000000000000000000000000000000000000",
            "true",
        ),
        (&format!("round-half-to-even({max}, -29)"), "error:FOAR0002"),
        ("round-half-to-even(xs:decimal('12345'), -2)", "12300"),
        ("round-half-to-even(xs:decimal('1.5'), -30)", "0"),
        ("round-half-to-even(xs:decimal('-49999.5'), -29)", "0"),
        (&format!("avg(({max}, {max}))"), "error:FOAR0002"),
    ]);
}

/// Typed nodes: the values come from schema validation of a document.
#[test]
fn typed_nodes_beyond_capacity() {
    const SCHEMA: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
      <xs:simpleType name="decs"><xs:list itemType="xs:decimal"/></xs:simpleType>
      <xs:element name="r"><xs:complexType><xs:sequence>
        <xs:element name="d" type="xs:decimal" maxOccurs="unbounded"/>
        <xs:element name="l" type="decs"/>
      </xs:sequence><xs:attribute name="a" type="xs:decimal"/></xs:complexType></xs:element>
    </xs:schema>"#;
    let instance = format!(
        r#"<r a="{PRECISE}"><d>{HUGE}</d><d>+{HUGE}.0</d><d>1.5</d><l>1 {HUGE} {PRECISE}</l></r>"#
    );
    let mut schema_set = SchemaSet::xsd11();
    load_and_process_schema(SCHEMA.as_bytes(), "test.xsd", &mut schema_set, None)
        .expect("schema loads");
    let arena = Bump::new();
    let doc = build_typed_document(
        instance.as_bytes(),
        &arena,
        &schema_set,
        BufferDocumentOptions::default(),
    )
    .expect("document builds");
    let ctx = XPathContext::new(&schema_set.name_table)
        .with_namespaces(context(&schema_set.name_table))
        .with_schema_set(&schema_set);
    let cases: &[(&str, &str)] = &[
        ("data(/r/d[1]) instance of xs:decimal", "true"),
        ("/r/d[1] = /r/d[2]", "true"),
        ("/r/d[1] eq /r/d[2]", "true"),
        ("/r/d[1] gt /r/d[3]", "true"),
        ("count(distinct-values(/r/d))", "2"),
        // A node's string value is its text; its typed value is the
        // canonical form (F&O §17.1.2).
        ("string(/r/d[2])", &format!("+{HUGE}.0")),
        ("string(data(/r/d[2]))", HUGE),
        ("xs:string(/r/d[2])", HUGE),
        ("string(data(/r/l)[2])", HUGE),
        ("/r/d[1] + 1", "error:FOAR0002"),
        ("sum(/r/d)", "error:FOAR0002"),
        ("/r/d[3] * 2", "3"),
        ("data(/r/@a) instance of xs:decimal", "true"),
        ("/r/@a gt 0.1234567890123456789012345678", "true"),
        ("count(data(/r/l))", "3"),
        ("data(/r/l)[2] instance of xs:decimal", "true"),
        ("data(/r/l)[3] instance of xs:decimal", "true"),
        ("data(/r/l)[2] eq /r/d[1]", "true"),
        ("data(/r/l)[1] + 1", "2"),
        ("data(/r/l)[2] + 1", "error:FOAR0002"),
        ("/r/d[1] cast as xs:integer", HUGE),
    ];
    let mut wrong = Vec::new();
    for (expr, expected) in cases {
        let got = match XPathExpr::compile(expr, &ctx) {
            Err(e) => rendered::<RoXmlNavigator<'_>>(Err(e)),
            Ok(compiled) => rendered(
                compiled
                    .evaluator(&ctx)
                    .run_with_node(doc.create_navigator()),
            ),
        };
        if got != *expected {
            wrong.push(format!("{expr}  =>  {got}   (expected {expected})"));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} of {} cases wrong:\n{}",
        wrong.len(),
        cases.len(),
        wrong.join("\n")
    );
}
