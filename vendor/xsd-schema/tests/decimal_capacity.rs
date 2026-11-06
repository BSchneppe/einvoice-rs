//! `xs:decimal` values beyond `rust_decimal`'s capacity (issue #25), end to
//! end through the public drivers.
//!
//! Datatypes 1.1 §3.3.3: the value space of `xs:decimal` "is the set of numbers
//! that can be obtained by dividing an integer by a non-negative power of
//! ten", and "Precision is not reflected in this value space; the number 2.0
//! is not distinct from the number 2.00." It is unbounded; the typed value
//! `rust_decimal::Decimal` holds a 96-bit mantissa with a scale of at most 28.
//!
//! Datatypes 1.1 §5.4: "When the datatype validity of a value or literal is
//! uncertain because it exceeds the capacity of a partial implementation, the
//! literal or value must not be treated as invalid, and the unsupported value
//! must not be quietly changed to a supported value", and "a conforming
//! processor must indicate to the user and/or downstream application that it
//! cannot process the input data with assured correctness".
//!
//! So: every value-based check is decided on the exact value — larger or more
//! precise values are neither rejected nor rounded into a different verdict —
//! and a valid value the typed value cannot hold exactly is reported with a
//! warning (`xs-decimal-capacity`), never an error.

use xsd_schema::types::validators::{DecimalValidator, IntegerValidator, TypeValidator};
use xsd_schema::types::value::{XmlAtomicValue, XmlValueKind};
use xsd_schema::types::XmlTypeCode;
use xsd_schema::validation::{
    drive_navigator, drive_quick_xml, CollectingValidationSink, SchemaValidator, SchemaValidity,
    ValidationFlags,
};
use xsd_schema::{RoXmlNavigator, SchemaSet, SchemaSetBuilder, XsdVersion};

use SchemaValidity::{Invalid, Valid};

const NOTICE: &str = "xs-decimal-capacity";
const VERSIONS: [XsdVersion; 2] = [XsdVersion::V1_0, XsdVersion::V1_1];

/// 10^40 — the value from the issue.
const HUGE: &str = "10000000000000000000000000000000000000000";
/// `Decimal::MAX` (2^96 − 1) and its successor.
const MAX: &str = "79228162514264337593543950335";
const MAX_PLUS_ONE: &str = "79228162514264337593543950336";
/// 29 fraction digits; `rust_decimal` would round it to 28.
const PRECISE: &str = "0.12345678901234567890123456789";
/// 1 + 10^-29; `rust_decimal` would round it to 1.
const ONE_PLUS: &str = "1.00000000000000000000000000001";

// ── Harness ───────────────────────────────────────────────────────────────

fn schema(version: XsdVersion, body: &str) -> SchemaSet {
    let xsd =
        format!(r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">{body}</xs:schema>"#);
    SchemaSetBuilder::with_version(version)
        .add_source(&xsd, "file:///decimal_capacity.xsd")
        .expect("schema source accepted")
        .compile()
        .expect("schema compiles")
        .into_schema_set()
}

fn validator(ss: &SchemaSet, flags: ValidationFlags) -> SchemaValidator<'_> {
    #[cfg(feature = "xsd11")]
    {
        SchemaValidator::new_fragment_buffer(ss, flags)
    }
    #[cfg(not(feature = "xsd11"))]
    {
        SchemaValidator::new(ss, flags)
    }
}

/// Root validity, error constraint codes and warning codes of one run.
struct Outcome {
    validity: Option<SchemaValidity>,
    errors: Vec<String>,
    warnings: Vec<String>,
}

fn stream(ss: &SchemaSet, xml: &str, flags: ValidationFlags) -> Outcome {
    let v = validator(ss, flags);
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let validity = {
        let mut rt = v.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        drive_quick_xml(xml.as_bytes(), &mut rt, ss)
            .expect("stream drive completes")
            .root_validity
    };
    Outcome {
        validity,
        errors: errors.iter().map(|e| e.constraint.to_string()).collect(),
        warnings: warnings.iter().map(|w| w.code.to_string()).collect(),
    }
}

fn dom(ss: &SchemaSet, xml: &str) -> Outcome {
    let v = validator(ss, ValidationFlags::default());
    let doc = roxmltree::Document::parse(xml).expect("well-formed instance");
    let nav = RoXmlNavigator::new(&doc);
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let validity = {
        let mut rt = v.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        drive_navigator(&nav, &mut rt, ss)
            .expect("navigator drive completes")
            .root_validity
    };
    Outcome {
        validity,
        errors: errors.iter().map(|e| e.constraint.to_string()).collect(),
        warnings: warnings.iter().map(|w| w.code.to_string()).collect(),
    }
}

/// Every driver — streaming with and without PSVI typed values, and the
/// navigator — reaches `expected`; `code` is the prefix of an expected error
/// (`None`: no error), and `notices` is how many capacity warnings to expect.
fn check(ss: &SchemaSet, xml: &str, expected: SchemaValidity, code: Option<&str>, notices: usize) {
    let lean = ValidationFlags::default() & !ValidationFlags::BUILD_PSVI_TYPED_VALUES;
    let runs = [
        ("stream", stream(ss, xml, ValidationFlags::default())),
        ("stream/no-psvi", stream(ss, xml, lean)),
        ("navigator", dom(ss, xml)),
    ];
    for (driver, out) in runs {
        assert_eq!(
            out.validity,
            Some(expected),
            "{driver}: root [validity] of {xml} (errors {:?})",
            out.errors
        );
        match code {
            Some(code) => assert!(
                out.errors.iter().any(|e| e.starts_with(code)),
                "{driver}: expected a {code} error for {xml}, got {:?}",
                out.errors
            ),
            None => assert!(
                out.errors.is_empty(),
                "{driver}: {xml}: errors {:?}",
                out.errors
            ),
        }
        // Errors stop value processing, so the notice count is only pinned
        // for valid instances.
        if code.is_none() {
            let got = out.warnings.iter().filter(|w| *w == NOTICE).count();
            assert_eq!(
                got, notices,
                "{driver}: capacity notices for {xml}: {:?}",
                out.warnings
            );
        }
    }
}

fn valid(ss: &SchemaSet, xml: &str, notices: usize) {
    check(ss, xml, Valid, None, notices);
}

fn invalid(ss: &SchemaSet, xml: &str, code: &str) {
    check(ss, xml, Invalid, Some(code), 0);
}

fn restriction(facets: &str) -> String {
    format!(
        r#"<xs:element name="v"><xs:simpleType><xs:restriction base="xs:decimal">{facets}</xs:restriction></xs:simpleType></xs:element>"#
    )
}

fn v(value: &str) -> String {
    format!("<v>{value}</v>")
}

// ── The issue's reproduction ──────────────────────────────────────────────

#[test]
fn issue_25_huge_decimal_is_valid_with_a_capacity_notice() {
    for version in VERSIONS {
        let ss = schema(version, r#"<xs:element name="v" type="xs:decimal"/>"#);
        valid(&ss, &v(HUGE), 1);
        valid(&ss, &v(MAX_PLUS_ONE), 1);
        valid(&ss, &v(&format!("-{HUGE}.5")), 1);
        // Exactly representable values carry no notice.
        valid(&ss, &v(MAX), 0);
        valid(&ss, &v("-1.5"), 0);
        valid(&ss, &v("1.0000000000000000000000000000000000000"), 0);
        // Precise values are valid too, never rounded.
        valid(&ss, &v(PRECISE), 1);
        valid(&ss, &v(ONE_PLUS), 1);
        // Still a lexical check first.
        invalid(&ss, &v("1e40"), "cvc-datatype-valid");
    }
}

#[test]
fn the_notice_follows_report_warnings() {
    let ss = schema(
        XsdVersion::V1_0,
        r#"<xs:element name="v" type="xs:decimal"/>"#,
    );
    let quiet = ValidationFlags::default() & !ValidationFlags::REPORT_WARNINGS;
    let out = stream(&ss, &v(HUGE), quiet);
    assert_eq!(out.validity, Some(Valid));
    assert!(out.errors.is_empty() && out.warnings.is_empty());
}

#[test]
fn typed_value_is_exact_or_the_lexical_form() {
    let small = DecimalValidator.validate(" 1.50 ").expect("valid");
    assert_eq!(small.type_code, XmlTypeCode::Decimal);
    assert_eq!(small.as_decimal(), Some(rust_decimal::Decimal::new(150, 2)));

    for lexical in [HUGE, MAX_PLUS_ONE, PRECISE, ONE_PLUS] {
        let big = DecimalValidator
            .validate(&format!("\n {lexical}\t"))
            .expect("valid");
        assert_eq!(big.type_code, XmlTypeCode::Decimal, "{lexical}");
        assert_eq!(big.as_decimal(), None, "{lexical} is never rounded");
        assert_eq!(
            big.value,
            XmlValueKind::Atomic(XmlAtomicValue::String(lexical.to_string()))
        );
        assert_eq!(big.to_string_value(), lexical);
        assert_eq!(
            big.as_double(),
            Some(lexical.parse::<f64>().unwrap()),
            "{lexical}"
        );
    }
    // A long form whose value fits is a `Decimal` with the exact value.
    let fits = DecimalValidator
        .validate("1.000000000000000000000000000000000000000000")
        .expect("valid");
    assert_eq!(fits.as_decimal(), Some(rust_decimal::Decimal::ONE));
}

// ── Digit facets (Datatypes 1.1 §4.3.11.3, §4.3.12.3) ─────────────────────

#[test]
fn fraction_digits_counts_the_exact_value() {
    for version in VERSIONS {
        let ss = schema(version, &restriction(r#"<xs:fractionDigits value="28"/>"#));
        // 29 fraction digits: invalid (0.2.2 rounded to 28 and accepted it).
        invalid(&ss, &v(PRECISE), "cvc-fractionDigits-valid");
        invalid(&ss, &v(ONE_PLUS), "cvc-fractionDigits-valid");
        valid(&ss, &v("0.1234567890123456789012345678"), 0);
        // Trailing zeros are not fraction digits of the value.
        valid(&ss, &v("0.12345678901234567890123456780000"), 0);
        valid(&ss, &v(HUGE), 1);

        let ss = schema(version, &restriction(r#"<xs:fractionDigits value="30"/>"#));
        valid(&ss, &v(PRECISE), 1);
        invalid(
            &ss,
            &v("0.0000000000000000000000000000001"),
            "cvc-fractionDigits-valid",
        );
    }
}

#[test]
fn total_digits_counts_the_exact_value() {
    for version in VERSIONS {
        let ss = schema(version, &restriction(r#"<xs:totalDigits value="40"/>"#));
        let forty = "1000000000000000000000000000000000000000"; // 10^39: 40 digits
        valid(&ss, &v(forty), 1);
        valid(&ss, &v(&format!("+000{forty}.000")), 1);
        invalid(&ss, &v(HUGE), "cvc-totalDigits-valid"); // 10^40: 41 digits

        let ss = schema(version, &restriction(r#"<xs:totalDigits value="29"/>"#));
        // 0.1234…789 = i / 10^29 with i of 29 digits.
        valid(&ss, &v(PRECISE), 1);
        // 1 + 10^-29 = (10^29 + 1) / 10^29: i has 30 digits.
        invalid(&ss, &v(ONE_PLUS), "cvc-totalDigits-valid");

        // §4.3.11.3 "|i| < 10^t and 0 ≤ n ≤ t": 0.05 = 5 / 10^2 needs t ≥ 2.
        let ss = schema(version, &restriction(r#"<xs:totalDigits value="1"/>"#));
        invalid(&ss, &v("0.05"), "cvc-totalDigits-valid");
        invalid(&ss, &v("10"), "cvc-totalDigits-valid");
        valid(&ss, &v("0.5"), 0);
        valid(&ss, &v("-9.000"), 0);
        valid(&ss, &v("0"), 0);
    }
}

// ── Bound facets — huge and very precise bounds and values ────────────────

#[test]
fn bounds_compare_exactly_with_huge_bounds_and_values() {
    let nines = "9999999999999999999999999999999999999999"; // 10^40 − 1
    for version in VERSIONS {
        let ss = schema(
            version,
            &restriction(&format!(r#"<xs:minInclusive value="{HUGE}"/>"#)),
        );
        valid(&ss, &v(HUGE), 1);
        valid(
            &ss,
            &v(&format!("{HUGE}.0000000000000000000000000000001")),
            1,
        );
        invalid(&ss, &v(nines), "cvc-minInclusive-valid");
        invalid(&ss, &v(MAX), "cvc-minInclusive-valid");

        let ss = schema(
            version,
            &restriction(&format!(r#"<xs:maxInclusive value="{HUGE}"/>"#)),
        );
        valid(&ss, &v(HUGE), 1);
        valid(&ss, &v(nines), 1);
        valid(&ss, &v("-1"), 0);
        invalid(
            &ss,
            &v(&format!("{HUGE}.0000000000000000000000000000001")),
            "cvc-maxInclusive-valid",
        );

        let ss = schema(
            version,
            &restriction(&format!(r#"<xs:minExclusive value="-{HUGE}"/>"#)),
        );
        valid(&ss, &v(&format!("-{nines}")), 1);
        valid(&ss, &v("0"), 0);
        invalid(&ss, &v(&format!("-{HUGE}.000")), "cvc-minExclusive-valid");

        let ss = schema(
            version,
            &restriction(&format!(r#"<xs:maxExclusive value="{HUGE}"/>"#)),
        );
        valid(&ss, &v(nines), 1);
        invalid(&ss, &v(HUGE), "cvc-maxExclusive-valid");
    }
}

#[test]
fn bounds_compare_exactly_with_very_precise_values() {
    for version in VERSIONS {
        // 0.2.2 judged 1 + 10^-29 as 1.0: minExclusive rejected it,
        // maxInclusive accepted it.
        let ss = schema(version, &restriction(r#"<xs:minExclusive value="1"/>"#));
        valid(&ss, &v(ONE_PLUS), 1);
        invalid(
            &ss,
            &v("1.00000000000000000000000000000000000"),
            "cvc-minExclusive-valid",
        );

        let ss = schema(version, &restriction(r#"<xs:maxInclusive value="1"/>"#));
        invalid(&ss, &v(ONE_PLUS), "cvc-maxInclusive-valid");
        valid(&ss, &v("0.99999999999999999999999999999999"), 1);

        let ss = schema(version, &restriction(r#"<xs:minInclusive value="1"/>"#));
        invalid(
            &ss,
            &v("0.99999999999999999999999999999999"),
            "cvc-minInclusive-valid",
        );

        let ss = schema(version, &restriction(r#"<xs:maxExclusive value="1"/>"#));
        invalid(&ss, &v(ONE_PLUS), "cvc-maxExclusive-valid");

        // A bound that is itself beyond 28 fraction digits.
        let ss = schema(
            version,
            &restriction(&format!(r#"<xs:maxExclusive value="{PRECISE}"/>"#)),
        );
        valid(&ss, &v("0.1234567890123456789012345678"), 0);
        valid(&ss, &v("0.123456789012345678901234567889"), 1);
        invalid(&ss, &v(PRECISE), "cvc-maxExclusive-valid");
        invalid(
            &ss,
            &v("0.123456789012345678901234567890001"),
            "cvc-maxExclusive-valid",
        );
    }
}

#[test]
fn inconsistent_huge_bounds_are_a_schema_error() {
    // minInclusive > maxInclusive by 1 at 10^40 — equal as f64.
    let body = restriction(&format!(
        r#"<xs:minInclusive value="{HUGE}1"/><xs:maxInclusive value="{HUGE}0"/>"#
    ));
    for version in VERSIONS {
        let xsd =
            format!(r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">{body}</xs:schema>"#);
        let compiled = SchemaSetBuilder::with_version(version)
            .add_source(&xsd, "file:///bounds.xsd")
            .and_then(|b| b.compile());
        assert!(compiled.is_err(), "{version:?}: min > max must not compile");
    }
    // Distinct huge exclusive bounds one apart are consistent.
    let ok = restriction(&format!(
        r#"<xs:minExclusive value="{HUGE}0"/><xs:maxExclusive value="{HUGE}1"/>"#
    ));
    for version in VERSIONS {
        let ss = schema(version, &ok);
        invalid(&ss, &v(&format!("{HUGE}0")), "cvc-minExclusive-valid");
        valid(&ss, &v(&format!("{HUGE}0.5")), 1);
    }
}

// ── Enumeration and fixed/default: value equality ─────────────────────────

#[test]
fn decimal_enumeration_compares_values() {
    for version in VERSIONS {
        // §4.3.5.4: "the value is equal or identical to one of the values".
        let ss = schema(version, &restriction(r#"<xs:enumeration value="1.0"/>"#));
        valid(&ss, &v("1.00"), 0);
        valid(&ss, &v("1"), 0);
        valid(&ss, &v("+01.000"), 0);
        invalid(&ss, &v("1.1"), "cvc-enumeration-valid");
        invalid(&ss, &v(ONE_PLUS), "cvc-enumeration-valid");

        let ss = schema(
            version,
            &restriction(&format!(
                r#"<xs:enumeration value="{HUGE}"/><xs:enumeration value="{PRECISE}"/>"#
            )),
        );
        valid(&ss, &v(&format!("+{HUGE}.00")), 1);
        valid(&ss, &v(&format!("{PRECISE}000")), 1);
        invalid(&ss, &v(&format!("{HUGE}1")), "cvc-enumeration-valid");
        invalid(
            &ss,
            &v("0.1234567890123456789012345679"),
            "cvc-enumeration-valid",
        );
    }
}

#[test]
fn integer_enumeration_compares_values() {
    for version in VERSIONS {
        let ss = schema(
            version,
            r#"<xs:element name="v"><xs:simpleType><xs:restriction base="xs:integer">
                 <xs:enumeration value="1"/></xs:restriction></xs:simpleType></xs:element>"#,
        );
        valid(&ss, &v("+1"), 0);
        valid(&ss, &v("01"), 0);
        invalid(&ss, &v("2"), "cvc-enumeration-valid");
    }
}

#[test]
fn fixed_and_default_compare_values() {
    for version in VERSIONS {
        let ss = schema(
            version,
            &format!(
                r#"<xs:element name="r"><xs:complexType><xs:sequence>
                     <xs:element name="a" type="xs:decimal" fixed="1.0" minOccurs="0"/>
                     <xs:element name="b" type="xs:decimal" fixed="{HUGE}" minOccurs="0"/>
                     <xs:element name="c" type="xs:decimal" default="{PRECISE}" minOccurs="0"/>
                   </xs:sequence>
                   <xs:attribute name="at" type="xs:decimal" fixed="{ONE_PLUS}"/>
                 </xs:complexType></xs:element>"#
            ),
        );
        valid(&ss, "<r><a>1.00</a></r>", 0);
        invalid(&ss, "<r><a>1.5</a></r>", "cvc-elt.5.2.2");
        valid(&ss, &format!("<r><b>+{HUGE}.0</b></r>"), 1);
        valid(&ss, "<r><b/></r>", 1);
        invalid(&ss, &format!("<r><b>{HUGE}1</b></r>"), "cvc-elt.5.2.2");
        valid(&ss, "<r><c/></r>", 1);
        valid(&ss, &format!(r#"<r at="{ONE_PLUS}0"/>"#), 1);
        invalid(&ss, r#"<r at="1"/>"#, "cvc-attribute.4");
    }
}

// ── Identity constraints: field equality by value ─────────────────────────

#[test]
fn identity_constraints_compare_huge_keys_by_value() {
    let body = r#"
      <xs:element name="r"><xs:complexType><xs:sequence>
        <xs:element name="k" type="xs:decimal" maxOccurs="unbounded"/>
        <xs:element name="i" type="xs:integer" minOccurs="0" maxOccurs="unbounded"/>
        <xs:element name="ref" type="xs:decimal" minOccurs="0" maxOccurs="unbounded"/>
      </xs:sequence></xs:complexType>
        <xs:key name="key"><xs:selector xpath="k|i"/><xs:field xpath="."/></xs:key>
        <xs:keyref name="kr" refer="key"><xs:selector xpath="ref"/><xs:field xpath="."/></xs:keyref>
      </xs:element>"#;
    for version in VERSIONS {
        let ss = schema(version, body);
        // Different spellings of 10^40: duplicates.
        invalid(
            &ss,
            &format!("<r><k>{HUGE}</k><k>+{HUGE}.000</k></r>"),
            "cvc-identity-constraint",
        );
        // An xs:integer and an xs:decimal of the same value: duplicates.
        invalid(
            &ss,
            &format!("<r><k>{HUGE}.0</k><i>{HUGE}</i></r>"),
            "cvc-identity-constraint",
        );
        // 1 + 10^-29 and 1 differ.
        valid(&ss, &format!("<r><k>{ONE_PLUS}</k><k>1</k></r>"), 1);
        // Values one apart at 10^40 differ.
        valid(&ss, &format!("<r><k>{HUGE}</k><k>{HUGE}1</k></r>"), 2);
        // A keyref spelled differently resolves by value.
        valid(&ss, &format!("<r><k>{HUGE}</k><ref>{HUGE}.0</ref></r>"), 2);
        invalid(
            &ss,
            &format!("<r><k>{HUGE}</k><ref>{HUGE}1</ref></r>"),
            "cvc-identity-constraint",
        );
    }
}

// ── Derived types, xs:integer, unions and lists ───────────────────────────

#[test]
fn derived_decimal_type_and_integer_family() {
    for version in VERSIONS {
        let ss = schema(
            version,
            r#"<xs:simpleType name="price"><xs:restriction base="xs:decimal">
                 <xs:minInclusive value="0"/><xs:fractionDigits value="30"/>
               </xs:restriction></xs:simpleType>
               <xs:simpleType name="cents"><xs:restriction base="price">
                 <xs:fractionDigits value="2"/></xs:restriction></xs:simpleType>
               <xs:element name="r"><xs:complexType><xs:sequence>
                 <xs:element name="p" type="price" minOccurs="0"/>
                 <xs:element name="c" type="cents" minOccurs="0"/>
                 <xs:element name="n" type="xs:integer" minOccurs="0"/>
                 <xs:element name="m" minOccurs="0"><xs:simpleType>
                   <xs:restriction base="xs:integer"><xs:maxInclusive value="100"/></xs:restriction>
                 </xs:simpleType></xs:element>
                 <xs:element name="u" type="xs:unsignedLong" minOccurs="0"/>
               </xs:sequence></xs:complexType></xs:element>"#,
        );
        valid(&ss, &format!("<r><p>{HUGE}.{}</p></r>", "5".repeat(30)), 1);
        invalid(
            &ss,
            &format!("<r><p>-{HUGE}</p></r>"),
            "cvc-minInclusive-valid",
        );
        valid(&ss, &format!("<r><c>{HUGE}.25</c></r>"), 1);
        invalid(
            &ss,
            &format!("<r><c>{HUGE}.255</c></r>"),
            "cvc-fractionDigits-valid",
        );
        // xs:integer keeps its BigInt typed value: no notice at any size.
        valid(&ss, &format!("<r><n>{HUGE}{HUGE}</n></r>"), 0);
        // 0.2.2 skipped every value facet of an integer beyond `Decimal`.
        invalid(
            &ss,
            &format!("<r><m>{HUGE}</m></r>"),
            "cvc-maxInclusive-valid",
        );
        valid(&ss, "<r><m>100</m></r>", 0);
        invalid(&ss, &format!("<r><u>{HUGE}</u></r>"), "cvc-datatype-valid");
    }
    let n = IntegerValidator.validate(HUGE).expect("valid");
    assert_eq!(n.type_code, XmlTypeCode::Integer);
    assert!(matches!(
        n.value,
        XmlValueKind::Atomic(XmlAtomicValue::Integer(_))
    ));
}

#[test]
fn union_and_list_members() {
    for version in VERSIONS {
        let ss = schema(
            version,
            r#"<xs:simpleType name="numOrText"><xs:union memberTypes="xs:decimal xs:string"/></xs:simpleType>
               <xs:simpleType name="small"><xs:restriction base="xs:decimal">
                 <xs:maxInclusive value="10"/></xs:restriction></xs:simpleType>
               <xs:simpleType name="smallOrBool"><xs:union memberTypes="small xs:boolean"/></xs:simpleType>
               <xs:simpleType name="decs"><xs:list itemType="xs:decimal"/></xs:simpleType>
               <xs:simpleType name="smalls"><xs:list itemType="small"/></xs:simpleType>
               <xs:simpleType name="oneOf"><xs:restriction base="numOrText">
                 <xs:enumeration value="1.0"/></xs:restriction></xs:simpleType>
               <xs:element name="r"><xs:complexType><xs:sequence>
                 <xs:element name="u" type="numOrText" minOccurs="0"/>
                 <xs:element name="s" type="smallOrBool" minOccurs="0"/>
                 <xs:element name="l" type="decs" minOccurs="0"/>
                 <xs:element name="m" type="smalls" minOccurs="0"/>
                 <xs:element name="e" type="oneOf" minOccurs="0"/>
               </xs:sequence></xs:complexType></xs:element>"#,
        );
        // The decimal member validates a huge value (0.2.2 fell through to
        // xs:string); the capacity notice follows the member's value.
        valid(&ss, &format!("<r><u>{HUGE}</u></r>"), 1);
        valid(&ss, "<r><u>abc</u></r>", 0);
        // A huge value is above `small`'s bound: no member accepts it.
        invalid(&ss, &format!("<r><s>{HUGE}</s></r>"), "cvc-simple-type");
        invalid(
            &ss,
            &format!("<r><s>10.{}1</s></r>", "0".repeat(30)),
            "cvc-simple-type",
        );
        valid(&ss, "<r><s>true</s></r>", 0);
        // Lists: every member exact; one notice per list value.
        valid(&ss, &format!("<r><l>1 {HUGE} {PRECISE}</l></r>"), 1);
        invalid(
            &ss,
            &format!("<r><m>1 10.{}1</m></r>", "0".repeat(30)),
            "cvc-maxInclusive-valid",
        );
        // Union enumeration by value.
        valid(&ss, "<r><e>1.000</e></r>", 0);
        invalid(
            &ss,
            &format!("<r><e>{ONE_PLUS}</e></r>"),
            "cvc-enumeration-valid",
        );
    }
}

// ── XSD 1.1 assertions on values beyond capacity ──────────────────────────

#[cfg(feature = "xsd11")]
#[test]
fn assertions_on_values_beyond_capacity() {
    let ss = schema(
        XsdVersion::V1_1,
        r#"<xs:simpleType name="positive"><xs:restriction base="xs:decimal">
             <xs:assertion test="$value gt 0"/></xs:restriction></xs:simpleType>
           <xs:simpleType name="nextPositive"><xs:restriction base="xs:decimal">
             <xs:assertion test="$value + 1 gt 0"/></xs:restriction></xs:simpleType>
           <xs:element name="r"><xs:complexType><xs:sequence>
             <xs:element name="p" type="positive" minOccurs="0"/>
             <xs:element name="q" type="nextPositive" minOccurs="0"/>
             <xs:element name="d" type="xs:decimal" minOccurs="0"/>
           </xs:sequence>
           <xs:assert test="empty(d) or d * 2 gt 0"/>
           <xs:assert test="empty(d) or d ne 0"/>
           </xs:complexType></xs:element>"#,
    );
    // Comparisons are exact: decided, no capacity question.
    valid(&ss, &format!("<r><p>{HUGE}</p></r>"), 1);
    invalid(&ss, &format!("<r><p>-{HUGE}</p></r>"), "cvc-assertion");
    valid(&ss, &format!("<r><p>{ONE_PLUS}</p></r>"), 1);
    // Arithmetic on a value beyond capacity raises FOAR0002: the assertion's
    // outcome is unknown, which must not make the value invalid (§5.4).
    valid(&ss, &format!("<r><q>{HUGE}</q></r>"), 1);
    valid(&ss, &format!("<r><d>{HUGE}</d></r>"), 1);
    // Within capacity the same assertions still decide.
    invalid(&ss, "<r><p>0</p></r>", "cvc-assertion");
    invalid(&ss, "<r><d>0</d></r>", "cvc-assertion");
}

// ── 0.2.3 review fixes ────────────────────────────────────────────────────

/// F1. The typed value of a decimal beyond capacity is its canonical
/// representation (Datatypes §3.3.3.2), whatever the instance's spelling —
/// as it always was for a value `rust_decimal` holds (`+0010.500` → `10.5`).
#[test]
fn typed_value_beyond_capacity_is_canonical() {
    for (spelling, canonical) in [
        (format!("+00{HUGE}.500"), format!("{HUGE}.5")),
        (format!("{HUGE}.000"), HUGE.to_string()),
        (
            "-.000000000000000000000000000000500".to_string(),
            "-0.0000000000000000000000000000005".to_string(),
        ),
        (format!("{PRECISE}000"), PRECISE.to_string()),
    ] {
        let value = DecimalValidator
            .validate(&format!(" {spelling}\n"))
            .expect("valid");
        assert_eq!(value.type_code, XmlTypeCode::Decimal, "{spelling}");
        assert_eq!(value.as_decimal(), None, "{spelling}");
        assert_eq!(
            value.value,
            XmlValueKind::Atomic(XmlAtomicValue::String(canonical.clone())),
            "{spelling}"
        );
        assert_eq!(value.to_string_value(), canonical, "{spelling}");
        // Two spellings of one value are one typed value.
        assert_eq!(
            value,
            DecimalValidator.validate(&canonical).expect("valid"),
            "{spelling}"
        );
    }
    let small = DecimalValidator.validate("+0010.500").expect("valid");
    assert_eq!(small.to_string_value(), "10.5");
    assert_eq!(
        DecimalValidator
            .validate("5.0")
            .expect("valid")
            .to_string_value(),
        "5"
    );
}

/// F1. F&O §17.1.2: casting an `xs:decimal` to `xs:string` gives its canonical
/// representation; a node's string value is still its text.
#[cfg(feature = "xsd11")]
#[test]
fn string_of_a_decimal_beyond_capacity_is_canonical() {
    let spelled = format!("+0{HUGE}00.500");
    let canonical = format!("{HUGE}00.5");
    let ss = schema(
        XsdVersion::V1_1,
        &format!(
            r#"<xs:simpleType name="st"><xs:restriction base="xs:decimal">
                 <xs:assertion test="string($value) = '{canonical}'"/>
                 <xs:assertion test="xs:string($value) = '{canonical}'"/>
                 <xs:assertion test="xs:untypedAtomic($value) = xs:untypedAtomic('{canonical}')"/>
                 <xs:assertion test="string-length(concat($value, '')) = {len}"/>
               </xs:restriction></xs:simpleType>
               <xs:simpleType name="sl"><xs:restriction>
                 <xs:simpleType><xs:list itemType="xs:decimal"/></xs:simpleType>
                 <xs:assertion test="string-join(for $m in $value return string($m), ' ') = '{canonical} 1.5'"/>
               </xs:restriction></xs:simpleType>
               <xs:element name="r"><xs:complexType><xs:sequence>
                 <xs:element name="s" type="st" minOccurs="0"/>
                 <xs:element name="l" type="sl" minOccurs="0"/>
                 <xs:element name="d" type="xs:decimal" minOccurs="0"/>
               </xs:sequence>
               <xs:attribute name="a" type="xs:decimal"/>
               <xs:assert test="empty(d) or (string(d) = '{spelled}' and string(data(d)) = '{canonical}')"/>
               <xs:assert test="empty(@a) or (string(@a) = '{spelled}' and string(data(@a)) = '{canonical}')"/>
               </xs:complexType></xs:element>"#,
            len = canonical.len()
        ),
    );
    valid(&ss, &format!("<r><s>{spelled}</s></r>"), 1);
    valid(&ss, &format!("<r><s>{canonical}</s></r>"), 1);
    valid(&ss, &format!("<r><l>{spelled} 01.50</l></r>"), 1);
    valid(&ss, &format!("<r><d>{spelled}</d></r>"), 1);
    valid(&ss, &format!(r#"<r a="{spelled}"/>"#), 1);
    invalid(&ss, &format!("<r><s>{HUGE}</s></r>"), "cvc-assertion");
}

/// F3. An assertion is skipped only when its error comes from computing with
/// a decimal beyond capacity; any other error fails it (Structures
/// §3.13.4.1), even when such a decimal is elsewhere in the instance.
#[cfg(feature = "xsd11")]
#[test]
fn assertion_errors_not_caused_by_capacity_fail_the_assertion() {
    let ss = schema(
        XsdVersion::V1_1,
        r#"<xs:simpleType name="div0"><xs:restriction base="xs:decimal">
             <xs:assertion test="($value div 0) gt 0"/></xs:restriction></xs:simpleType>
           <xs:simpleType name="idiv0"><xs:restriction base="xs:decimal">
             <xs:assertion test="($value idiv 0) gt 0"/></xs:restriction></xs:simpleType>
           <xs:simpleType name="mod0"><xs:restriction base="xs:decimal">
             <xs:assertion test="($value mod 0) gt 0"/></xs:restriction></xs:simpleType>
           <xs:simpleType name="precise"><xs:restriction base="xs:decimal">
             <xs:assertion test="$value gt 0.12345678901234567890123456789"/></xs:restriction></xs:simpleType>
           <xs:element name="r"><xs:complexType><xs:sequence>
             <xs:element name="x" type="xs:decimal" minOccurs="0"/>
             <xs:element name="y" type="xs:decimal" minOccurs="0"/>
             <xs:element name="d" type="div0" minOccurs="0"/>
             <xs:element name="i" type="idiv0" minOccurs="0"/>
             <xs:element name="m" type="mod0" minOccurs="0"/>
             <xs:element name="p" type="precise" minOccurs="0"/>
           </xs:sequence>
           <xs:assert test="empty(y) or y * 100000000000000000000 lt 0"/>
           <xs:assert test="empty(x) or (x div 0) ne 1"/>
           </xs:complexType></xs:element>"#,
    );
    // (a) `y` in capacity overflows: a genuine FOAR0002, whether or not an
    // unrelated huge `x` is present.
    invalid(
        &ss,
        &format!("<r><x>1</x><y>{MAX}</y></r>"),
        "cvc-assertion",
    );
    invalid(
        &ss,
        &format!("<r><x>{HUGE}</x><y>{MAX}</y></r>"),
        "cvc-assertion",
    );
    // (b) Division by zero is FOAR0001 (F&O §6.2.4), whatever the dividend.
    for element in ["d", "i", "m"] {
        invalid(
            &ss,
            &format!("<r><{element}>5</{element}></r>"),
            "cvc-assertion",
        );
        invalid(
            &ss,
            &format!("<r><{element}>{HUGE}</{element}></r>"),
            "cvc-assertion",
        );
    }
    invalid(&ss, &format!("<r><x>{HUGE}</x></r>"), "cvc-assertion");
    // A literal too precise to cast is FOCA0006 for every value.
    invalid(&ss, &format!("<r><p>{HUGE}</p></r>"), "cvc-assertion");
    invalid(&ss, "<r><p>1</p></r>", "cvc-assertion");
    // Arithmetic with the huge value itself is still an unknown outcome.
    let ss = schema(
        XsdVersion::V1_1,
        r#"<xs:element name="r"><xs:complexType><xs:sequence>
             <xs:element name="x" type="xs:decimal"/><xs:element name="y" type="xs:decimal"/>
           </xs:sequence><xs:assert test="x + y lt 0"/></xs:complexType></xs:element>"#,
    );
    valid(&ss, &format!("<r><x>{HUGE}</x><y>1</y></r>"), 1);
    invalid(&ss, "<r><x>1</x><y>1</y></r>", "cvc-assertion");
}

/// F10. F&O §17.1.1 / §6.2: a decimal literal too small to be held (a
/// non-zero magnitude below 10^-28) underflows to 0 — also in an assertion,
/// where it is no longer an error.
#[cfg(feature = "xsd11")]
#[test]
fn an_underflowing_literal_in_an_assertion_is_zero() {
    let tiny = format!("0.{}1", "0".repeat(28));
    let ss = schema(
        XsdVersion::V1_1,
        &format!(
            r#"<xs:simpleType name="above"><xs:restriction base="xs:decimal">
                 <xs:assertion test="$value gt {tiny}"/></xs:restriction></xs:simpleType>
               <xs:simpleType name="cast"><xs:restriction base="xs:decimal">
                 <xs:assertion test="$value ge xs:decimal('-{tiny}')"/></xs:restriction></xs:simpleType>
               <xs:element name="r"><xs:complexType><xs:sequence>
                 <xs:element name="a" type="above" minOccurs="0"/>
                 <xs:element name="c" type="cast" minOccurs="0"/>
               </xs:sequence></xs:complexType></xs:element>"#
        ),
    );
    // `$value gt 0`.
    valid(&ss, "<r><a>0.0000000000000000000000000001</a></r>", 0);
    invalid(&ss, "<r><a>0</a></r>", "cvc-assertion");
    // A value below 10^-28 is valid — exact, carried as a string — but its
    // comparison with the literal (now 0) is exact too.
    valid(&ss, &format!("<r><a>{tiny}</a></r>"), 1);
    // `$value ge 0`.
    valid(&ss, "<r><c>0</c></r>", 0);
    invalid(
        &ss,
        "<r><c>-0.0000000000000000000000000001</c></r>",
        "cvc-assertion",
    );
}

/// F5. Bound consistency compares in the datatype's value space: exactly for
/// `xs:decimal`, as `f64` for `xs:double` and as `f32` for `xs:float`.
#[test]
fn float_and_double_bounds_compare_in_their_own_value_space() {
    let restrict = |base: &str, min: &str, max: &str| {
        format!(
            r#"<xs:simpleType name="T"><xs:restriction base="{base}">
                 <xs:minInclusive value="{min}"/><xs:maxInclusive value="{max}"/>
               </xs:restriction></xs:simpleType><xs:element name="v" type="T"/>"#
        )
    };
    let compiles = |version: XsdVersion, body: &str| {
        let xsd =
            format!(r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">{body}</xs:schema>"#);
        SchemaSetBuilder::with_version(version)
            .add_source(&xsd, "file:///bounds.xsd")
            .and_then(|b| b.compile())
            .is_ok()
    };
    for version in VERSIONS {
        // The same double (0.2.2 compiled it; the RC rejected it).
        let dbl = restrict("xs:double", "0.10000000000000000001", "0.1");
        assert!(compiles(version, &dbl), "{version:?}");
        valid(&schema(version, &dbl), "<v>0.1</v>", 0);
        // The same float, two different doubles.
        assert!(
            compiles(version, &restrict("xs:float", "0.1000000001", "0.1")),
            "{version:?}"
        );
        // Genuine conflicts.
        assert!(!compiles(version, &restrict("xs:double", "0.2", "0.1")));
        assert!(!compiles(version, &restrict("xs:float", "0.11", "0.1")));
        // xs:decimal compares exactly, beyond double precision.
        assert!(!compiles(
            version,
            &restrict(
                "xs:decimal",
                "1.00000000000000000000000000002",
                "1.00000000000000000000000000001"
            )
        ));
        assert!(compiles(
            version,
            &restrict(
                "xs:decimal",
                "1.00000000000000000000000000001",
                "1.00000000000000000000000000002"
            )
        ));
    }
}

/// F9. A list whose item type is a union records one item type for all its
/// members; a member that the union's `xs:decimal` member validated beyond
/// capacity is still reported (Datatypes §5.4), and one that another member
/// type validated is not.
#[test]
fn list_of_union_members_beyond_capacity_are_reported() {
    for version in VERSIONS {
        let ss = schema(
            version,
            r#"<xs:simpleType name="decOrBool"><xs:union memberTypes="xs:decimal xs:boolean"/></xs:simpleType>
               <xs:simpleType name="nonNeg"><xs:restriction base="xs:decimal">
                 <xs:minInclusive value="0"/></xs:restriction></xs:simpleType>
               <xs:simpleType name="nested"><xs:union memberTypes="xs:date decOrBool"/></xs:simpleType>
               <xs:simpleType name="small"><xs:restriction base="xs:decimal">
                 <xs:maxInclusive value="10"/></xs:restriction></xs:simpleType>
               <xs:simpleType name="decs"><xs:list itemType="xs:decimal"/></xs:simpleType>
               <xs:element name="r"><xs:complexType><xs:sequence>
                 <xs:element name="lu" minOccurs="0"><xs:simpleType><xs:list itemType="decOrBool"/></xs:simpleType></xs:element>
                 <xs:element name="ld" minOccurs="0"><xs:simpleType><xs:list><xs:simpleType>
                   <xs:union memberTypes="nonNeg xs:boolean"/></xs:simpleType></xs:list></xs:simpleType></xs:element>
                 <xs:element name="ln" minOccurs="0"><xs:simpleType><xs:list itemType="nested"/></xs:simpleType></xs:element>
                 <xs:element name="ls" minOccurs="0"><xs:simpleType><xs:list><xs:simpleType>
                   <xs:union memberTypes="small xs:string"/></xs:simpleType></xs:list></xs:simpleType></xs:element>
                 <xs:element name="lr" minOccurs="0"><xs:simpleType><xs:restriction>
                   <xs:simpleType><xs:list itemType="decOrBool"/></xs:simpleType>
                   <xs:maxLength value="3"/></xs:restriction></xs:simpleType></xs:element>
                 <xs:element name="ul" minOccurs="0"><xs:simpleType>
                   <xs:union memberTypes="xs:boolean decs"/></xs:simpleType></xs:element>
               </xs:sequence>
               <xs:attribute name="a"><xs:simpleType><xs:list itemType="decOrBool"/></xs:simpleType></xs:attribute>
               </xs:complexType></xs:element>"#,
        );
        valid(&ss, &format!("<r><lu>1 {HUGE} true</lu></r>"), 1);
        valid(&ss, &format!("<r><lu>true {PRECISE}</lu></r>"), 1);
        valid(&ss, "<r><lu>1 2.5 true</lu></r>", 0);
        // More than 28 digits, but held exactly: no notice.
        valid(&ss, &format!("<r><lu>{}1 true</lu></r>", "0".repeat(40)), 0);
        valid(&ss, &format!("<r><ld>true {HUGE}</ld></r>"), 1);
        valid(&ss, &format!("<r><ln>2020-01-01 {HUGE} false</ln></r>"), 1);
        // `small` rejects the huge value, `xs:string` takes it: not a decimal
        // (0.2.3 RC reported it when the list's last item was a decimal).
        valid(&ss, &format!("<r><ls>{HUGE} 5</ls></r>"), 0);
        valid(&ss, &format!("<r><lr>true {HUGE}</lr></r>"), 1);
        valid(&ss, &format!("<r><ul>1 {HUGE}</ul></r>"), 1);
        valid(&ss, &format!(r#"<r a="{HUGE} false"/>"#), 1);
    }
}

/// The members of a list whose item type is a union are typed with the member
/// type that validated each of them (XPath 2.0 §2.5.2: "when the type
/// annotation of a node is a union type, the typed value of the node will be
/// an instance of one of the member types of the union"; Structures
/// §3.13.4.1 clause 2.3.1: `$value` is "the XDM representation of
/// E.[schema actual value]"), in `$value` of a simple type and in `data()` of
/// a node, on every driver. A number the union's `xs:string` member validated
/// is an `xs:string` and can never pass for an `xs:decimal` beyond capacity.
///
/// 0.2.2 (and 0.2.3, which kept its typing here) typed every member
/// `xs:string`; the rows marked "member type" now give the specification's
/// answer instead: `5` is a value of the union's decimal member.
#[cfg(feature = "xsd11")]
#[test]
fn list_of_union_members_have_their_member_types() {
    const UNION_LIST: &str = r#"<xs:simpleType name="u"><xs:union>
          <xs:simpleType><xs:restriction base="xs:decimal"><xs:maxInclusive value="10"/></xs:restriction></xs:simpleType>
          <xs:simpleType><xs:restriction base="xs:string"/></xs:simpleType>
        </xs:union></xs:simpleType>
        <xs:simpleType name="l"><xs:list itemType="u"/></xs:simpleType>"#;
    let simple = |test: &str| {
        schema(
            XsdVersion::V1_1,
            &format!(
                r#"{UNION_LIST}<xs:element name="d"><xs:simpleType><xs:restriction base="l">
                     <xs:assertion test="{test}"/></xs:restriction></xs:simpleType></xs:element>"#
            ),
        )
    };
    let node = |test: &str| {
        schema(
            XsdVersion::V1_1,
            &format!(
                r#"{UNION_LIST}<xs:element name="r"><xs:complexType><xs:sequence>
                     <xs:element name="d" type="l"/></xs:sequence>
                     <xs:assert test="{test}"/></xs:complexType></xs:element>"#
            ),
        )
    };
    let huge_string = format!("<d>{HUGE} 5</d>");
    // (assertion on $value, instance, valid?)
    let rows: [(&str, &str, bool); 14] = [
        ("$value[1] + 1 eq 0", "<d>20 5</d>", false),
        ("$value[1] instance of xs:decimal", "<d>20 5</d>", false),
        // Member type (0.2.2: false).
        ("$value[2] instance of xs:decimal", "<d>20 5</d>", true),
        ("$value[1] instance of xs:string", "<d>20 5</d>", true),
        // Member type (0.2.2: true).
        ("$value[2] instance of xs:string", "<d>20 5</d>", false),
        // Member type (0.2.2: false, a type error on a string).
        ("$value[2] + 1 eq 6", "<d>20 5</d>", true),
        ("$value[2] + 1 eq 0", "<d>abc 5</d>", false),
        // Member type (0.2.2: false).
        ("$value[2] instance of xs:decimal", "<d>abc 5</d>", true),
        ("$value[1] instance of xs:string", "<d>abc 5</d>", true),
        ("$value[1] + 1 eq 0", &huge_string, false),
        ("$value[1] instance of xs:decimal", &huge_string, false),
        ("$value[2] + 1 eq 0", &huge_string, false),
        // Member type (0.2.2: true).
        ("$value[2] instance of xs:string", &huge_string, false),
        ("count($value) eq 2", &huge_string, true),
    ];
    for (test, xml, ok) in rows {
        let ss = simple(test);
        if ok {
            valid(&ss, xml, 0);
        } else {
            invalid(&ss, xml, "cvc-assertion");
        }
        // The same members through `data()` of a node.
        let ss = node(&test.replace("$value", "data(d)"));
        let xml = format!("<r>{xml}</r>");
        if ok {
            valid(&ss, &xml, 0);
        } else {
            invalid(&ss, &xml, "cvc-assertion");
        }
    }
}

/// A member of a list of a union that the union's `xs:decimal` member
/// validated beyond capacity is an `xs:decimal` (0.2.2 and 0.2.3 typed it
/// `xs:string`): it draws the capacity warning, and an assertion that would
/// compute with it is skipped (Datatypes §5.4) — on `$value` of a simple type,
/// `data()` of a node and `$value` of a complex type with simple content.
#[cfg(feature = "xsd11")]
#[test]
fn list_of_union_decimal_members_beyond_capacity_are_decimals() {
    const UNION_LIST: &str = r#"<xs:simpleType name="u"><xs:union memberTypes="xs:decimal xs:boolean"/></xs:simpleType>
        <xs:simpleType name="l"><xs:list itemType="u"/></xs:simpleType>"#;
    let schemas = |test: &str| {
        [
            schema(
                XsdVersion::V1_1,
                &format!(
                    r#"{UNION_LIST}<xs:element name="d"><xs:simpleType><xs:restriction base="l">
                         <xs:assertion test="{test}"/></xs:restriction></xs:simpleType></xs:element>"#
                ),
            ),
            schema(
                XsdVersion::V1_1,
                &format!(
                    r#"{UNION_LIST}<xs:element name="d"><xs:complexType><xs:simpleContent>
                         <xs:extension base="l"><xs:assert test="{test}"/></xs:extension>
                         </xs:simpleContent></xs:complexType></xs:element>"#
                ),
            ),
        ]
    };
    let node = |test: &str| {
        schema(
            XsdVersion::V1_1,
            &format!(
                r#"{UNION_LIST}<xs:element name="r"><xs:complexType><xs:sequence>
                     <xs:element name="d" type="l"/></xs:sequence>
                     <xs:assert test="{}"/></xs:complexType></xs:element>"#,
                test.replace("$value", "data(d)")
            ),
        )
    };
    let huge = format!("<d>{HUGE} true</d>");
    // (assertion, instance, valid?, capacity notices)
    let rows: [(&str, &str, bool, usize); 5] = [
        ("$value[1] instance of xs:decimal", &huge, true, 1),
        ("$value[2] instance of xs:boolean", &huge, true, 1),
        // Skipped: the outcome is unknown, which must not make it invalid.
        ("$value[1] + 1 gt 0", &huge, true, 1),
        // Within capacity the member computes.
        ("$value[1] + 1 gt 0", "<d>5 true</d>", true, 0),
        ("$value[1] + 1 lt 0", "<d>5 true</d>", false, 0),
    ];
    for (test, xml, ok, notices) in rows {
        for ss in schemas(test) {
            if ok {
                valid(&ss, xml, notices);
            } else {
                invalid(&ss, xml, "cvc-assertion");
            }
        }
        let ss = node(test);
        let xml = format!("<r>{xml}</r>");
        if ok {
            valid(&ss, &xml, notices);
        } else {
            invalid(&ss, &xml, "cvc-assertion");
        }
    }
}

/// Second follow-up (P1). In `$value` of a complex type with simple content
/// whose type is a list of a union, a string spelling a decimal beyond
/// capacity that the union's `xs:string` member validated is an `xs:string`,
/// never a decimal whose arithmetic would skip the assertion.
#[cfg(feature = "xsd11")]
#[test]
fn complex_simple_content_list_of_union_never_skips() {
    let complex = |test: &str| {
        schema(
            XsdVersion::V1_1,
            &format!(
                r#"<xs:simpleType name="u"><xs:union>
                     <xs:simpleType><xs:restriction base="xs:decimal"><xs:maxInclusive value="10"/></xs:restriction></xs:simpleType>
                     <xs:simpleType><xs:restriction base="xs:string"/></xs:simpleType>
                   </xs:union></xs:simpleType>
                   <xs:simpleType name="l"><xs:list itemType="u"/></xs:simpleType>
                   <xs:element name="d"><xs:complexType><xs:simpleContent><xs:extension base="l">
                     <xs:assert test="{test}"/></xs:extension></xs:simpleContent></xs:complexType></xs:element>"#
            ),
        )
    };
    let huge_string = format!("<d>{HUGE} 5</d>");
    invalid(
        &complex("$value[1] + 1 eq 0"),
        "<d>20 5</d>",
        "cvc-assertion",
    );
    invalid(
        &complex("$value[2] + 1 eq 0"),
        "<d>abc 5</d>",
        "cvc-assertion",
    );
    // The string member beyond capacity: computed with as a string (a type
    // error), not skipped; 0 capacity notices — nothing was a decimal beyond
    // capacity.
    invalid(
        &complex("$value[1] + 1 eq 0"),
        &huge_string,
        "cvc-assertion",
    );
    valid(&complex("$value[1] instance of xs:string"), &huge_string, 0);
    // A small member is a value of the decimal member and computes.
    valid(&complex("$value[2] + 1 eq 6"), "<d>20 5</d>", 0);
    // B5: each member has its own member type. 0.2.2 and 0.2.3 typed every
    // member with the recorded (last member's) item type, so `abc` was an
    // `xs:decimal` holding a string.
    valid(
        &complex("$value[1] instance of xs:string"),
        "<d>abc 5</d>",
        0,
    );
    invalid(
        &complex("$value[1] instance of xs:decimal"),
        "<d>abc 5</d>",
        "cvc-assertion",
    );
    valid(
        &complex("$value[2] instance of xs:decimal"),
        "<d>abc 5</d>",
        0,
    );
}

/// Second follow-up (P1). A list whose item type is `xs:decimal` itself still
/// types its members beyond capacity `xs:decimal`, warns, and skips arithmetic
/// on them (Datatypes §5.4), while small members compute.
#[cfg(feature = "xsd11")]
#[test]
fn list_of_decimal_members_beyond_capacity_stay_decimals() {
    let list = |test: &str| {
        schema(
            XsdVersion::V1_1,
            &format!(
                r#"<xs:simpleType name="dl"><xs:list itemType="xs:decimal"/></xs:simpleType>
                   <xs:element name="d"><xs:simpleType><xs:restriction base="dl">
                     <xs:assertion test="{test}"/></xs:restriction></xs:simpleType></xs:element>
                   <xs:element name="r"><xs:complexType><xs:sequence><xs:element name="e" type="dl"/></xs:sequence>
                     <xs:assert test="{node}"/></xs:complexType></xs:element>"#,
                node = test.replace("$value", "data(e)")
            ),
        )
    };
    for xml in [format!("1 {HUGE}"), format!("{HUGE} {HUGE}")] {
        let ss = list("$value[2] instance of xs:decimal");
        valid(&ss, &format!("<d>{xml}</d>"), 1);
        valid(&ss, &format!("<r><e>{xml}</e></r>"), 1);
        let ss = list("$value[2] + 1 lt 0");
        valid(&ss, &format!("<d>{xml}</d>"), 1);
        valid(&ss, &format!("<r><e>{xml}</e></r>"), 1);
    }
    let ss = list("$value[1] + 1 eq 2");
    valid(&ss, &format!("<d>1 {HUGE}</d>"), 1);
    invalid(&ss, &format!("<d>2 {HUGE}</d>"), "cvc-assertion");
}
