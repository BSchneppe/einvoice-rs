//! The XPath of XSD 1.1 assertions, assertion facets and type alternatives
//! sees only the built-in components (option B of plan row C43).
//!
//! Structures 1.1 §3.13.6.2 (XPath Valid): "2 X does not produce any static
//! error, under the following conditions … 2.2.5 The in-scope schema
//! definitions are those components that are present in every schema by
//! definition, as defined in Built-in Attribute Declarations (§3.2.7), Built-in
//! Complex Type Definition (§3.4.7) and Built-in Simple Type Definitions
//! (§3.16.7)." §3.13.6.1 Assertion Properties Correct, clause 2: "The {test}
//! satisfies the constraint XPath Valid (§3.13.6.2), with the following
//! modifications to the static context: 2.1 The in-scope variables is a set
//! with a single member … value … 2.2 The function signatures includes
//! signatures for all of the following: 2.2.1 Functions in the
//! http://www.w3.org/2005/xpath-functions namespace … 2.2.2 Constructor
//! functions for the built-in datatypes." §3.12.6 Type Alternative Properties
//! Correct, clause 2: "If the {test} is not ·absent·, then it satisfies the
//! constraint XPath Valid (§3.13.6.2)." An assertion facet's {value} is a
//! sequence of Assertion components (Datatypes 1.1 §4.3.13.1).
//!
//! So naming a user-defined type or declaration there — `instance of`, `treat
//! as`, `cast as`, `castable as`, a constructor, `element(*, T)`,
//! `attribute(*, T)`, `schema-element`, `schema-attribute` — is a static
//! error, and the schema is invalid: `as-props-correct` for an assertion or an
//! assertion facet, `src-ct-alternative` for a type alternative. So are an
//! undeclared prefix, an undefined variable and an unknown constructor of the
//! XML Schema namespace. A function of another namespace the engine does not
//! know stays lenient: the schema is accepted and the assertion fails on each
//! instance (`cvc-assertion`), the alternative never matches.
//!
//! Evaluation still sees the schema set: a node annotated with a user type
//! derived from `xs:integer` is an `element(*, xs:integer)`.
//!
//! Each instance case runs on the streaming driver with and without PSVI
//! typed values and on the navigator driver; XSD 1.0 has neither assertions nor
//! type alternatives.

use xsd_schema::{SchemaSetBuilder, XsdVersion};

/// `{ATTRS}` (on `<xs:schema>`), `{ASSERT}`, `{FACET}`, `{ALT}` are replaced
/// by each case.
const SCHEMA: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:t="urn:t"
    xmlns:f="urn:f" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
    targetNamespace="urn:t" elementFormDefault="qualified" {ATTRS}>
  <xs:simpleType name="small">
    <xs:restriction base="xs:integer"><xs:maxInclusive value="10"/></xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="smaller">
    <xs:restriction base="t:small"><xs:maxInclusive value="9"/>{FACET}</xs:restriction>
  </xs:simpleType>
  <xs:attribute name="ga" type="xs:int"/>
  <xs:element name="holder" type="t:H" nillable="true"/>
  <xs:complexType name="H">
    <xs:sequence><xs:element name="c" type="t:small"/></xs:sequence>
    <xs:attribute name="a" type="t:small"/>
    {ASSERT}
  </xs:complexType>
  <xs:complexType name="Base"><xs:attribute name="a"/></xs:complexType>
  <xs:complexType name="Derived">
    <xs:complexContent><xs:restriction base="t:Base">
      <xs:attribute name="a" type="xs:string"/>
    </xs:restriction></xs:complexContent>
  </xs:complexType>
  <xs:complexType name="NoA">
    <xs:complexContent><xs:restriction base="t:Base">
      <xs:attribute name="a" use="prohibited"/>
    </xs:restriction></xs:complexContent>
  </xs:complexType>
  <xs:element name="alt" type="t:Base" nillable="true">{ALT}</xs:element>
  <xs:element name="s" type="t:smaller" nillable="true"/>
</xs:schema>"#;

fn schema_text(attrs: &str, assert: &str, facet: &str, alt: &str) -> String {
    SCHEMA
        .replace("{ATTRS}", attrs)
        .replace("{ASSERT}", assert)
        .replace("{FACET}", facet)
        .replace("{ALT}", alt)
}

fn compile(version: XsdVersion, text: &str) -> Result<xsd_schema::SchemaSet, String> {
    SchemaSetBuilder::with_version(version)
        .add_source(text, "file:///assertion_type_scope.xsd")
        .map_err(|e| e.to_string())?
        .compile()
        .map(|c| c.into_schema_set())
        .map_err(|e| e.to_string())
}

/// XSD 1.0 has no `assert`, `assertion` or `alternative`: every case's
/// schema is refused.
#[test]
fn xsd_1_0_has_no_assertions_or_alternatives() {
    for text in [
        schema_text(
            "",
            r#"<xs:assert test="data(@a) instance of t:small"/>"#,
            "",
            "",
        ),
        schema_text(
            "",
            "",
            r#"<xs:assertion test="$value instance of t:small"/>"#,
            "",
        ),
        schema_text(
            "",
            "",
            "",
            r#"<xs:alternative test="@a = 'x'" type="t:Derived"/>"#,
        ),
    ] {
        assert!(
            compile(XsdVersion::V1_0, &text).is_err(),
            "XSD 1.0 accepted {text}"
        );
    }
}

#[cfg(feature = "xsd11")]
mod xsd11 {
    use super::*;
    use xsd_schema::validation::{
        drive_navigator, drive_quick_xml, CollectingValidationSink, SchemaValidator,
        SchemaValidity, ValidationFlags,
    };
    use xsd_schema::{RoXmlNavigator, SchemaSet};

    /// The expected verdict of a case.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Expect {
        /// The schema compiles; the instance is valid on every driver.
        Valid,
        /// The schema compiles; the instance is invalid (`cvc-assertion`).
        Invalid,
        /// The schema compiles; the instance is invalid (any constraint: a
        /// type alternative selected a type the instance does not satisfy).
        Rejected,
        /// The schema is refused with this constraint.
        Refused(&'static str),
        /// The schema compiles (type alternatives: no instance checked).
        Accepted,
    }
    use Expect::*;

    const AS: Expect = Refused("as-props-correct");
    const CTA: Expect = Refused("src-ct-alternative");

    fn flags(psvi: bool) -> ValidationFlags {
        let mut f = ValidationFlags::default() | ValidationFlags::PROCESS_ASSERTIONS;
        if !psvi {
            f &= !ValidationFlags::BUILD_PSVI_TYPED_VALUES;
        }
        f
    }

    fn stream(ss: &SchemaSet, xml: &str, psvi: bool) -> (Option<SchemaValidity>, Vec<String>) {
        let v = SchemaValidator::new(ss, flags(psvi));
        let (mut errors, mut warnings) = (Vec::new(), Vec::new());
        let root = {
            let mut rt = v.start_run(CollectingValidationSink {
                errors: &mut errors,
                warnings: &mut warnings,
            });
            drive_quick_xml(xml.as_bytes(), &mut rt, ss)
                .expect("drive")
                .root_validity
        };
        (root, errors.iter().map(|e| e.to_string()).collect())
    }

    fn navigator(ss: &SchemaSet, xml: &str) -> (Option<SchemaValidity>, Vec<String>) {
        let v = SchemaValidator::new(ss, flags(true));
        let doc = roxmltree::Document::parse(xml).expect("well-formed");
        let nav = RoXmlNavigator::new(&doc);
        let (mut errors, mut warnings) = (Vec::new(), Vec::new());
        let root = {
            let mut rt = v.start_run(CollectingValidationSink {
                errors: &mut errors,
                warnings: &mut warnings,
            });
            drive_navigator(&nav, &mut rt, ss)
                .expect("drive")
                .root_validity
        };
        (root, errors.iter().map(|e| e.to_string()).collect())
    }

    /// Compile `text`, run `xml` on every driver, compare with `expect`.
    fn check(label: &str, text: &str, xml: &str, expect: Expect, wrong: &mut Vec<String>) {
        check_each(label, text, &[(xml, expect)], wrong);
    }

    /// Compile `text` once, run each instance on every driver and compare it
    /// with its expectation. A `Refused` / `Accepted` expectation is about the
    /// schema alone, so it must be the only case.
    fn check_each(label: &str, text: &str, cases: &[(&str, Expect)], wrong: &mut Vec<String>) {
        let schema_only = cases
            .iter()
            .find(|(_, e)| matches!(e, Refused(_) | Accepted))
            .map(|(_, e)| *e);
        assert!(
            schema_only.is_none() || cases.len() == 1,
            "{label}: a schema-only expectation must be the only case"
        );
        let ss = match (compile(XsdVersion::V1_1, text), schema_only) {
            (Err(e), Some(Refused(code))) => {
                if !e.contains(code) {
                    wrong.push(format!("{label}: refused, but not with {code}: {e}"));
                }
                return;
            }
            (Err(e), _) => {
                wrong.push(format!("{label}: schema refused: {e}"));
                return;
            }
            (Ok(_), Some(Refused(code))) => {
                wrong.push(format!("{label}: schema accepted, expected {code}"));
                return;
            }
            (Ok(_), Some(_)) => return,
            (Ok(ss), None) => ss,
        };
        for &(xml, expect) in cases {
            let expected = if expect == Valid {
                SchemaValidity::Valid
            } else {
                SchemaValidity::Invalid
            };
            for (driver, (root, errors)) in [
                ("stream", stream(&ss, xml, true)),
                ("stream/no-psvi", stream(&ss, xml, false)),
                ("navigator", navigator(&ss, xml)),
            ] {
                if root != Some(expected) {
                    wrong.push(format!(
                        "{label} {driver} on {xml}: {root:?}, expected {expected:?}: {errors:?}"
                    ));
                }
                if expect == Invalid && !errors.iter().any(|e| e.contains("cvc-assertion")) {
                    wrong.push(format!(
                        "{label} {driver} on {xml}: no cvc-assertion: {errors:?}"
                    ));
                }
            }
        }
    }

    const HOLDER: &str = r#"<t:holder xmlns:t="urn:t" a="3"><t:c>3</t:c></t:holder>"#;
    const S: &str = r#"<t:s xmlns:t="urn:t">3</t:s>"#;
    const XS: &str = "http://www.w3.org/2001/XMLSchema";

    /// The constructors of the types XSD 1.1 and the data model added to the
    /// built-in types of XSD 1.0 (`xs:anyAtomicType` has none).
    const ADDED_CONSTRUCTORS: &str = "exists(xs:dateTimeStamp('2020-01-01T00:00:00Z')) \
         and exists(xs:dayTimeDuration('PT1H')) and exists(xs:yearMonthDuration('P1Y')) \
         and exists(xs:untypedAtomic('x'))";

    /// A call of the constructor of every built-in atomic type (Structures
    /// §3.13.6.1 clause 2.2.2, XPath 2.0 §3.10.4: all but xs:NOTATION and
    /// xs:anyAtomicType), each of which must return one item.
    fn every_constructor() -> String {
        const CALLS: &[(&str, &str)] = &[
            ("string", "a"),
            ("boolean", "true"),
            ("decimal", "1.5"),
            ("float", "1"),
            ("double", "1"),
            ("duration", "P1D"),
            ("dateTime", "2020-01-01T00:00:00"),
            ("time", "00:00:00"),
            ("date", "2020-01-01"),
            ("gYearMonth", "2020-01"),
            ("gYear", "2020"),
            ("gMonthDay", "--01-01"),
            ("gDay", "---01"),
            ("gMonth", "--01"),
            ("hexBinary", "0A"),
            ("base64Binary", "AA=="),
            ("anyURI", "urn:x"),
            ("QName", "xs:string"),
            ("normalizedString", "a"),
            ("token", "a"),
            ("language", "en"),
            ("NMTOKEN", "a"),
            ("Name", "a"),
            ("NCName", "a"),
            ("ID", "a"),
            ("IDREF", "a"),
            ("ENTITY", "a"),
            ("integer", "1"),
            ("nonPositiveInteger", "0"),
            ("negativeInteger", "-1"),
            ("long", "1"),
            ("int", "1"),
            ("short", "1"),
            ("byte", "1"),
            ("nonNegativeInteger", "1"),
            ("unsignedLong", "1"),
            ("unsignedInt", "1"),
            ("unsignedShort", "1"),
            ("unsignedByte", "1"),
            ("positiveInteger", "1"),
            ("yearMonthDuration", "P1Y"),
            ("dayTimeDuration", "PT1H"),
            ("dateTimeStamp", "2020-01-01T00:00:00Z"),
            ("untypedAtomic", "x"),
        ];
        let calls: Vec<String> = CALLS
            .iter()
            .map(|(t, v)| format!("xs:{t}('{v}')"))
            .collect();
        format!("count(({})) = {}", calls.join(", "), CALLS.len())
    }

    /// Cases common to the three kinds of XPath: (label, test, assertion,
    /// assertion facet, type alternative). `$value` is the facet's value; the
    /// complex-type assertion and the alternative read `@a`.
    fn common() -> Vec<(&'static str, String, String, Expect, Expect, Expect)> {
        let row = |label: &'static str, a: &str, f: &str, x: Expect, y: Expect, z: Expect| {
            (label, a.to_string(), f.to_string(), x, y, z)
        };
        vec![
            row(
                "instance of a user type",
                "data(@a) instance of t:small",
                "$value instance of t:small",
                AS,
                AS,
                CTA,
            ),
            row(
                "treat as a user type",
                "(data(@a) treat as t:small) = 3",
                "($value treat as t:small) = $value",
                AS,
                AS,
                CTA,
            ),
            row(
                "cast as a user type",
                "(@a cast as t:small) = 3",
                "($value cast as t:small) = $value",
                AS,
                AS,
                CTA,
            ),
            row(
                "castable as a user type",
                "@a castable as t:small",
                "$value castable as t:small",
                AS,
                AS,
                CTA,
            ),
            row(
                "a user constructor",
                "t:small(@a) = 3",
                "t:small($value) = $value",
                AS,
                AS,
                CTA,
            ),
            row(
                "an undeclared prefix in a type name",
                "@a castable as nope:t",
                "$value castable as nope:t",
                AS,
                AS,
                CTA,
            ),
            row(
                "an undeclared prefix in a name test",
                "exists(nope:c)",
                "exists(nope:c) or true()",
                AS,
                AS,
                CTA,
            ),
            row(
                "an undefined variable",
                "$x = 1",
                "$x = $value",
                AS,
                AS,
                CTA,
            ),
            row(
                "an unknown xs: constructor",
                "xs:nosuch(@a) = 1",
                "xs:nosuch($value) = 1",
                AS,
                AS,
                CTA,
            ),
            row(
                "a cast to a built-in type",
                "(@a cast as xs:integer) = 3",
                "($value cast as xs:integer) = 3",
                Valid,
                Valid,
                Accepted,
            ),
            // What is present in every schema by definition (Structures
            // §3.13.6.2 clause 2.2.5): the built-in simple types (§3.16.7) and
            // xs:anyType (§3.4.7) in every position a type name can take.
            row(
                "xs:anyAtomicType as an AtomicType",
                "data(@a) instance of xs:anyAtomicType",
                "$value instance of xs:anyAtomicType",
                Valid,
                Valid,
                Accepted,
            ),
            row(
                "xs:anySimpleType is not an AtomicType (XPST0051)",
                "data(@a) instance of xs:anySimpleType",
                "$value instance of xs:anySimpleType",
                AS,
                AS,
                CTA,
            ),
            row(
                "xs:error is not an AtomicType (XPST0051)",
                "@a castable as xs:error",
                "$value castable as xs:error",
                AS,
                AS,
                CTA,
            ),
            row(
                "no cast to xs:anyAtomicType (XPST0080)",
                "@a castable as xs:anyAtomicType",
                "$value castable as xs:anyAtomicType",
                AS,
                AS,
                CTA,
            ),
            row(
                "no xs:anyAtomicType constructor (XPST0017)",
                "exists(xs:anyAtomicType(@a))",
                "exists(xs:anyAtomicType($value))",
                AS,
                AS,
                CTA,
            ),
            row(
                "no xs:NOTATION constructor (XPST0017)",
                "exists(xs:NOTATION('x'))",
                "exists(xs:NOTATION('x'))",
                AS,
                AS,
                CTA,
            ),
            row(
                "no constructor of a list type (XPST0017)",
                "exists(xs:NMTOKENS('a b'))",
                "exists(xs:NMTOKENS('a b'))",
                AS,
                AS,
                CTA,
            ),
            row(
                "constructors of the XSD 1.1 and XDM types",
                ADDED_CONSTRUCTORS,
                ADDED_CONSTRUCTORS,
                Valid,
                Valid,
                Accepted,
            ),
            row(
                "constructors of every built-in atomic type",
                &every_constructor(),
                &every_constructor(),
                Valid,
                Valid,
                Accepted,
            ),
            row(
                "attribute(*, T) with built-in types",
                "exists(@a[. instance of attribute(*, xs:anySimpleType)]) \
                 and exists(@a[. instance of attribute(*, xs:anyType)]) \
                 and empty(@a[. instance of attribute(*, xs:error)])",
                "not($value instance of attribute(*, xs:anySimpleType)) \
                 and not($value instance of attribute(*, xs:error))",
                Valid,
                Valid,
                Accepted,
            ),
            row(
                "element(*, T) with built-in types",
                "exists(self::element(*, xs:anyType)) and empty(self::element(*, xs:error))",
                "not($value instance of element(*, xs:anyType)) \
                 and not($value instance of element(*, xs:error))",
                Valid,
                Valid,
                Accepted,
            ),
            // No element declaration is present by definition, and only the
            // four xsi: attribute declarations (§3.2.7) are.
            row(
                "schema-element of an xs: name",
                "exists(self::schema-element(xs:schema)) or true()",
                "exists(($value)[. instance of schema-element(xs:schema)]) or true()",
                AS,
                AS,
                CTA,
            ),
            row(
                "schema-element of an xsi: name",
                "exists(self::schema-element(xsi:nil)) or true()",
                "exists(($value)[. instance of schema-element(xsi:nil)]) or true()",
                AS,
                AS,
                CTA,
            ),
            row(
                "schema-attribute of xml:lang",
                "exists(@*[. instance of schema-attribute(xml:lang)]) or true()",
                "exists(($value)[. instance of schema-attribute(xml:lang)]) or true()",
                AS,
                AS,
                CTA,
            ),
            row(
                "schema-attribute of an undeclared xsi: name",
                "exists(@*[. instance of schema-attribute(xsi:nosuch)]) or true()",
                "exists(($value)[. instance of schema-attribute(xsi:nosuch)]) or true()",
                AS,
                AS,
                CTA,
            ),
            row(
                "a foreign function (lenient)",
                "f:g(@a)",
                "f:g($value)",
                Invalid,
                Invalid,
                Accepted,
            ),
        ]
    }

    /// Complex-type assertions (`xs:assert`).
    #[test]
    fn complex_type_assertions_see_only_built_in_components() {
        let mut cases: Vec<(&str, String, Expect)> = common()
            .into_iter()
            .map(|(l, a, _, x, _, _)| (l, a, x))
            .collect();
        cases.extend([
            (
                "attribute(*, T) with a user type",
                "exists(@a[. instance of attribute(*, t:small)])".into(),
                AS,
            ),
            (
                "element(*, T) with a user type",
                "exists(t:c[. instance of element(*, t:small)])".into(),
                AS,
            ),
            (
                "schema-element of a user declaration",
                "exists(self::schema-element(t:holder)) or true()".into(),
                AS,
            ),
            (
                "schema-attribute of a user declaration",
                "exists(@*[. instance of schema-attribute(t:ga)]) or true()".into(),
                AS,
            ),
            (
                "schema-attribute of an undeclared name",
                "exists(@*[. instance of schema-attribute(t:nosuch)]) or true()".into(),
                AS,
            ),
            // The same names as path steps.
            (
                "an element(*, T) step with a user type",
                "exists(element(*, t:small)) or true()".into(),
                AS,
            ),
            (
                "an attribute(*, T) step with a user type",
                "exists(attribute(*, t:small)) or true()".into(),
                AS,
            ),
            (
                "a schema-attribute step of a user declaration",
                "exists(@schema-attribute(t:ga)) or true()".into(),
                AS,
            ),
            (
                "a schema-attribute step of a built-in xsi: declaration",
                "empty(@schema-attribute(xsi:nil))".into(),
                Valid,
            ),
            // Evaluation sees the schema set: `c` is a `t:small`, derived from xs:integer.
            (
                "element(*, xs:integer) on a user-typed node",
                "exists(t:c[. instance of element(*, xs:integer)])".into(),
                Valid,
            ),
            (
                "element(*, xs:anyType)",
                "exists(t:c[. instance of element(*, xs:anyType)])".into(),
                Valid,
            ),
        ]);
        let mut wrong = Vec::new();
        for (label, test, expect) in &cases {
            let text = schema_text("", &format!(r#"<xs:assert test="{test}"/>"#), "", "");
            check(label, &text, HOLDER, *expect, &mut wrong);
        }
        // An unprefixed type name in the default element/type namespace
        // (`xpathDefaultNamespace`, on the assertion or the schema document).
        let assert =
            format!(r#"<xs:assert test="(@a cast as integer) = 3" xpathDefaultNamespace="{XS}"/>"#);
        check(
            "xpathDefaultNamespace on xs:assert",
            &schema_text("", &assert, "", ""),
            HOLDER,
            Valid,
            &mut wrong,
        );
        let attrs = format!(r#"xpathDefaultNamespace="{XS}""#);
        let assert = r#"<xs:assert test="(@a cast as integer) = 3"/>"#;
        check(
            "xpathDefaultNamespace on xs:schema",
            &schema_text(&attrs, assert, "", ""),
            HOLDER,
            Valid,
            &mut wrong,
        );
        assert!(
            wrong.is_empty(),
            "{} wrong:\n{}",
            wrong.len(),
            wrong.join("\n")
        );
    }

    /// Assertion facets (`xs:assertion` on a simple type).
    #[test]
    fn assertion_facets_see_only_built_in_components() {
        let mut wrong = Vec::new();
        for (label, _, test, _, expect, _) in common() {
            let text = schema_text("", "", &format!(r#"<xs:assertion test="{test}"/>"#), "");
            check(label, &text, S, expect, &mut wrong);
        }
        assert!(
            wrong.is_empty(),
            "{} wrong:\n{}",
            wrong.len(),
            wrong.join("\n")
        );
    }

    /// Type alternatives (`xs:alternative`): checked when the schema is built.
    #[test]
    fn type_alternatives_see_only_built_in_components() {
        let mut cases: Vec<(&str, String, Expect)> = common()
            .into_iter()
            .map(|(l, a, _, _, _, z)| (l, a, z))
            .collect();
        cases.extend([
            (
                "attribute(*, T) with a user type",
                "exists(@a[. instance of attribute(*, t:small)])".into(),
                CTA,
            ),
            (
                "element(*, T) with a user type",
                "exists(self::node()[. instance of element(*, t:small)])".into(),
                CTA,
            ),
            (
                "schema-element of a user declaration",
                "exists(self::schema-element(t:alt))".into(),
                CTA,
            ),
            (
                "schema-attribute of an undeclared name",
                "exists(@*[. instance of schema-attribute(t:nosuch)]) or true()".into(),
                CTA,
            ),
            (
                "an attribute(*, T) step with a user type",
                "exists(attribute(*, t:small))".into(),
                CTA,
            ),
            (
                "a schema-attribute step of a user declaration",
                "exists(@schema-attribute(t:ga))".into(),
                CTA,
            ),
            (
                "a schema-attribute step of a built-in xsi: declaration",
                "exists(@schema-attribute(xsi:nil))".into(),
                Accepted,
            ),
        ]);
        let mut wrong = Vec::new();
        for (label, test, expect) in &cases {
            let text = schema_text(
                "",
                "",
                "",
                &format!(r#"<xs:alternative test="{test}" type="t:Derived"/>"#),
            );
            check(label, &text, "", *expect, &mut wrong);
        }
        // The default element/type namespace (`xpathDefaultNamespace`, on the
        // alternative or the schema document) applies at schema time as it
        // does when the alternative is evaluated.
        let alt = format!(
            r#"<xs:alternative test="@a castable as integer" xpathDefaultNamespace="{XS}" type="t:Derived"/>"#
        );
        check(
            "xpathDefaultNamespace on xs:alternative",
            &schema_text("", "", "", &alt),
            "",
            Accepted,
            &mut wrong,
        );
        let attrs = format!(r#"xpathDefaultNamespace="{XS}""#);
        let alt = r#"<xs:alternative test="@a castable as integer" type="t:Derived"/>"#;
        check(
            "xpathDefaultNamespace on xs:schema",
            &schema_text(&attrs, "", "", alt),
            "",
            Accepted,
            &mut wrong,
        );
        assert!(
            wrong.is_empty(),
            "{} wrong:\n{}",
            wrong.len(),
            wrong.join("\n")
        );
    }

    const XSI: &str = "http://www.w3.org/2001/XMLSchema-instance";

    /// The four attribute declarations present in every schema by definition
    /// (Structures §3.2.7), each with a value for `holder`, `s` and `alt`.
    const XSI_ATTRIBUTES: [(&str, [&str; 3]); 4] = [
        ("type", ["t:H", "t:smaller", "t:Base"]),
        ("nil", ["false", "false", "false"]),
        (
            "schemaLocation",
            ["urn:x x.xsd", "urn:x x.xsd", "urn:x x.xsd"],
        ),
        ("noNamespaceSchemaLocation", ["x.xsd", "x.xsd", "x.xsd"]),
    ];

    /// `schema-attribute(xsi:…)` names a declaration present in every schema by
    /// definition (§3.2.7, §3.13.6.2 clause 2.2.5), in an assertion, an
    /// assertion facet and a type alternative alike, and evaluates as XPath 2.0
    /// §2.5.4.6 says ("derives-from(AT, ET) is true, where AT is the type
    /// annotation of the candidate node"), on an instance with the attribute
    /// and on one without.
    ///
    /// An assertion sees the attribute typed by its declaration (§3.13.4.1
    /// clause 1.1: "the [attributes] and [children] of E will be validated in
    /// the usual way"), so the test holds exactly when the attribute is there.
    /// An assertion facet's `$value` is an atomic value, never an attribute
    /// node. A type alternative sees untyped nodes (§3.12.4: "The element E
    /// and its [attributes] will be represented in the data model instance by
    /// nodes labeled as untyped"), so the `schema-attribute` test never holds;
    /// its negation selects `t:NoA`, which prohibits `a` (and is no base of the
    /// `xsi:type` `t:Base`), so both instances are invalid. (An evaluation
    /// error would count as false, §3.12.4 clause 2, and leave them valid.)
    #[test]
    fn the_built_in_xsi_attribute_declarations_are_in_scope() {
        let mut wrong = Vec::new();
        for (name, [on_holder, on_s, on_alt]) in XSI_ATTRIBUTES {
            let holder = format!(
                r#"<t:holder xmlns:t="urn:t" xmlns:xsi="{XSI}" a="3" xsi:{name}="{on_holder}"><t:c>3</t:c></t:holder>"#
            );
            let s =
                format!(r#"<t:s xmlns:t="urn:t" xmlns:xsi="{XSI}" xsi:{name}="{on_s}">3</t:s>"#);
            let alt = format!(
                r#"<t:alt xmlns:t="urn:t" xmlns:xsi="{XSI}" a="x" xsi:{name}="{on_alt}"/>"#
            );
            let plain_alt = r#"<t:alt xmlns:t="urn:t" a="x"/>"#;

            let test = format!("@xsi:{name} instance of schema-attribute(xsi:{name})");
            check_each(
                &format!("assertion, xsi:{name}"),
                &schema_text("", &format!(r#"<xs:assert test="{test}"/>"#), "", ""),
                &[(&holder, Valid), (HOLDER, Invalid)],
                &mut wrong,
            );
            let test = format!("not($value instance of schema-attribute(xsi:{name}))");
            check_each(
                &format!("assertion facet, xsi:{name}"),
                &schema_text("", "", &format!(r#"<xs:assertion test="{test}"/>"#), ""),
                &[(&s, Valid), (S, Valid)],
                &mut wrong,
            );
            let test = format!("not(@xsi:{name} instance of schema-attribute(xsi:{name}))");
            check_each(
                &format!("type alternative, xsi:{name}"),
                &schema_text(
                    "",
                    "",
                    "",
                    &format!(r#"<xs:alternative test="{test}" type="t:NoA"/>"#),
                ),
                &[(&alt, Rejected), (plain_alt, Rejected)],
                &mut wrong,
            );
        }
        assert!(
            wrong.is_empty(),
            "{} wrong:\n{}",
            wrong.len(),
            wrong.join("\n")
        );
    }

    /// Only the four built-in declarations of the XSI namespace are in scope:
    /// components a schema document for that namespace declares are
    /// user-defined like any other, and it cannot declare an attribute there
    /// at all (§3.2.6.3 `no-xsi`).
    #[test]
    fn user_components_in_the_xsi_namespace_stay_out_of_scope() {
        let compile_with = |assert: &str, xsi_components: &str| {
            let main = schema_text("", assert, "", "").replacen(
                "<xs:simpleType name=\"small\">",
                &format!("<xs:import namespace=\"{XSI}\"/>\n  <xs:simpleType name=\"small\">"),
                1,
            );
            let xsi_doc = format!(
                r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="{XSI}">{xsi_components}</xs:schema>"#
            );
            SchemaSetBuilder::with_version(XsdVersion::V1_1)
                .add_source(&main, "file:///main.xsd")
                .and_then(|b| b.add_source(&xsi_doc, "file:///xsi.xsd"))
                .map_err(|e| e.to_string())?
                .compile()
                .map(|c| c.into_schema_set())
                .map_err(|e| e.to_string())
        };
        let components = r#"<xs:element name="e" type="xs:int"/>
            <xs:simpleType name="T"><xs:restriction base="xs:int"/></xs:simpleType>"#;
        let mut wrong = Vec::new();
        for (label, test, expect) in [
            (
                "the built-in xsi:nil beside them",
                "exists(@xsi:nil) or true()",
                None,
            ),
            (
                "schema-element of a user xsi: element",
                "exists(self::schema-element(xsi:e)) or true()",
                Some("as-props-correct"),
            ),
            (
                "instance of a user xsi: type",
                "data(@a) instance of xsi:T",
                Some("as-props-correct"),
            ),
            (
                "attribute(*, T) with a user xsi: type",
                "exists(@a[. instance of attribute(*, xsi:T)]) or true()",
                Some("as-props-correct"),
            ),
            (
                "a user xsi: constructor",
                "exists(xsi:T(1))",
                Some("as-props-correct"),
            ),
        ] {
            let assert = format!(r#"<xs:assert test="{test}"/>"#);
            match (compile_with(&assert, components), expect) {
                (Ok(_), None) => {}
                (Err(e), Some(code)) if e.contains(code) => {}
                (result, _) => wrong.push(format!(
                    "{label}: {:?}, expected {expect:?}",
                    result.map(|_| "accepted")
                )),
            }
        }
        match compile_with("", r#"<xs:attribute name="foo"/>"#) {
            Err(e) if e.contains("no-xsi") => {}
            result => wrong.push(format!(
                "a user xsi: attribute: {:?}, expected no-xsi",
                result.map(|_| "accepted")
            )),
        }
        assert!(
            wrong.is_empty(),
            "{} wrong:\n{}",
            wrong.len(),
            wrong.join("\n")
        );
    }
}
