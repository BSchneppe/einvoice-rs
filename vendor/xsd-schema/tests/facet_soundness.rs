//! Constraining facets that must reject a value, end to end, through the
//! public drivers and in both XSD versions: the `enumeration` a restriction
//! inherits from its base type, `NaN` against the bounding facets of
//! `xs:float` and `xs:double`, and date/time values with and without a time
//! zone offset against `enumeration` and the bounding facets — and the white
//! space normalization that comes before every facet: of an enumeration value
//! as a value of its base type, and of a simple content restriction's value
//! with the restriction's own whiteSpace.
//!
//! Every case runs on streaming validation with and without PSVI typed values
//! and on the navigator driver, in XSD 1.0 and XSD 1.1 mode.

use xsd_schema::validation::{
    drive_navigator, drive_quick_xml, CollectingValidationSink, SchemaValidator, SchemaValidity,
    ValidationFlags,
};
use xsd_schema::{RoXmlNavigator, SchemaError, SchemaSet, SchemaSetBuilder, XsdVersion};

use SchemaValidity::{Invalid, Valid};

const VERSIONS: [XsdVersion; 2] = [XsdVersion::V1_0, XsdVersion::V1_1];

const ENUM: Option<&str> = Some("cvc-enumeration-valid");
const MIN_IN: Option<&str> = Some("cvc-minInclusive-valid");
const MAX_IN: Option<&str> = Some("cvc-maxInclusive-valid");
const MIN_EX: Option<&str> = Some("cvc-minExclusive-valid");
const MAX_EX: Option<&str> = Some("cvc-maxExclusive-valid");

// ── Harness ───────────────────────────────────────────────────────────────

fn xsd(body: &str) -> String {
    format!(r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">{body}</xs:schema>"#)
}

fn schema(version: XsdVersion, body: &str) -> SchemaSet {
    SchemaSetBuilder::with_version(version)
        .add_source(&xsd(body), "file:///facet_soundness.xsd")
        .expect("schema source accepted")
        .compile()
        .expect("schema compiles")
        .into_schema_set()
}

/// The error compiling `body` reports — its constraint, or its message when
/// it names none — or `None` when it compiles.
fn schema_error(version: XsdVersion, body: &str) -> Option<String> {
    match SchemaSetBuilder::with_version(version)
        .add_source(&xsd(body), "file:///facet_soundness.xsd")
        .expect("schema source accepted")
        .compile()
    {
        Ok(_) => None,
        Err(SchemaError::StructuralError { constraint, .. }) => Some(constraint.to_string()),
        Err(error) => Some(error.to_string()),
    }
}

/// Root validity and error constraint codes of one run on `driver`, with
/// `extra` validation flags set as well as the driver's.
fn run_with(
    ss: &SchemaSet,
    xml: &str,
    driver: &str,
    extra: ValidationFlags,
) -> (Option<SchemaValidity>, Vec<String>) {
    let flags = extra
        | match driver {
            "stream/no-psvi" => {
                ValidationFlags::default() & !ValidationFlags::BUILD_PSVI_TYPED_VALUES
            }
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
        if driver == "navigator" {
            let doc = roxmltree::Document::parse(xml).expect("well-formed instance");
            let nav = RoXmlNavigator::new(&doc);
            drive_navigator(&nav, &mut rt, ss)
                .expect("navigator drive completes")
                .root_validity
        } else {
            drive_quick_xml(xml.as_bytes(), &mut rt, ss)
                .expect("stream drive completes")
                .root_validity
        }
    };
    (
        validity,
        errors.iter().map(|e| e.constraint.to_string()).collect(),
    )
}

/// Run every `(xml, expected, code)` row on every driver; with `code`, some
/// error starts with it, and without, there is no error at all. Mismatches
/// are collected in `failures`, so that a test reports all of them, in both
/// versions, at once.
fn check_rows(
    failures: &mut Vec<String>,
    version: XsdVersion,
    ss: &SchemaSet,
    rows: &[(&str, SchemaValidity, Option<&str>)],
) {
    check_rows_with(failures, version, ss, rows, ValidationFlags::empty());
}

/// [`check_rows`] with `extra` validation flags set on every driver.
fn check_rows_with(
    failures: &mut Vec<String>,
    version: XsdVersion,
    ss: &SchemaSet,
    rows: &[(&str, SchemaValidity, Option<&str>)],
    extra: ValidationFlags,
) {
    for (xml, expected, code) in rows {
        for driver in ["stream", "stream/no-psvi", "navigator"] {
            let (validity, errors) = run_with(ss, xml, driver, extra);
            let code_ok = match code {
                Some(code) => errors.iter().any(|e| e.starts_with(code)),
                None => errors.is_empty(),
            };
            if validity != Some(*expected) || !code_ok {
                failures.push(format!(
                    "{version:?} {driver}: {xml} => {validity:?} {errors:?} (expected {expected:?} {code:?})"
                ));
            }
        }
    }
}

fn assert_no_failures(failures: &[String]) {
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// One global element per type, named after it.
fn elements(types: &[&str]) -> String {
    types
        .iter()
        .map(|t| format!(r#"<xs:element name="{t}" type="{t}"/>"#))
        .collect()
}

// ── C44: a restriction inherits its base type's enumeration ──────────────

const INHERITED_ENUMERATION: &str = r#"
  <xs:notation name="png" public="image/png"/>
  <xs:notation name="gif" public="image/gif"/>
  <xs:simpleType name="color"><xs:restriction base="xs:string">
    <xs:enumeration value="red"/><xs:enumeration value="green"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="plainColor"><xs:restriction base="color"/></xs:simpleType>
  <xs:simpleType name="plainPlainColor"><xs:restriction base="plainColor"/></xs:simpleType>
  <xs:simpleType name="shortColor"><xs:restriction base="color"><xs:maxLength value="4"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="rColor"><xs:restriction base="color"><xs:pattern value="r.*"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="redOnly"><xs:restriction base="plainColor"><xs:enumeration value="red"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="sizes"><xs:restriction base="xs:integer">
    <xs:enumeration value="1"/><xs:enumeration value="5"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="smallSizes"><xs:restriction base="sizes"><xs:maxInclusive value="1"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="plainSizes"><xs:restriction base="sizes"/></xs:simpleType>
  <xs:simpleType name="when"><xs:restriction base="xs:date"><xs:enumeration value="2000-01-01"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="plainWhen"><xs:restriction base="when"/></xs:simpleType>
  <xs:simpleType name="ratio"><xs:restriction base="xs:double"><xs:enumeration value="0.5"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="plainRatio"><xs:restriction base="ratio"/></xs:simpleType>
  <xs:simpleType name="ints"><xs:list itemType="xs:integer"/></xs:simpleType>
  <xs:simpleType name="intPairs"><xs:restriction base="ints">
    <xs:enumeration value="1 2"/><xs:enumeration value="3 4"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="shortIntPairs"><xs:restriction base="intPairs"><xs:maxLength value="2"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="intOrDate"><xs:union memberTypes="xs:integer xs:date"/></xs:simpleType>
  <xs:simpleType name="someIntOrDate"><xs:restriction base="intOrDate">
    <xs:enumeration value="1"/><xs:enumeration value="2000-01-01"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="digitIntOrDate"><xs:restriction base="someIntOrDate"><xs:pattern value="[0-9-]+"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="qe"><xs:restriction base="xs:QName"><xs:enumeration value="a"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="plainQe"><xs:restriction base="qe"/></xs:simpleType>
  <xs:simpleType name="picture"><xs:restriction base="xs:NOTATION"><xs:enumeration value="png"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="plainPicture"><xs:restriction base="picture"/></xs:simpleType>
  <xs:complexType name="labelled"><xs:simpleContent>
    <xs:extension base="xs:string"><xs:attribute name="lang" type="xs:language"/></xs:extension>
  </xs:simpleContent></xs:complexType>
  <xs:complexType name="colorCT"><xs:simpleContent><xs:restriction base="labelled">
    <xs:enumeration value="red"/><xs:enumeration value="green"/>
  </xs:restriction></xs:simpleContent></xs:complexType>
  <xs:complexType name="shortColorCT"><xs:simpleContent><xs:restriction base="colorCT">
    <xs:maxLength value="4"/>
  </xs:restriction></xs:simpleContent></xs:complexType>
  <xs:element name="att"><xs:complexType>
    <xs:attribute name="c" type="plainColor"/>
    <xs:attribute name="p" type="plainPicture"/>
  </xs:complexType></xs:element>"#;

/// Structures 1.1 §3.16.6.4 Simple Type Restriction (Facets), clause 3: "The
/// {facets} of R are the {facets} of B ·overlaid· with S", where overlaying
/// keeps "Every facet in B … unless it is of the same kind as some facet in
/// S". A restriction that adds no `enumeration` keeps its base type's; one
/// that adds its own replaces it (and its own must be a subset, checked when
/// the schema compiles).
#[test]
fn a_restriction_keeps_the_base_types_enumeration() {
    let mut failures = Vec::new();
    for version in VERSIONS {
        let body = format!(
            "{INHERITED_ENUMERATION}{}",
            elements(&[
                "color",
                "plainColor",
                "plainPlainColor",
                "shortColor",
                "rColor",
                "redOnly",
                "smallSizes",
                "plainSizes",
                "plainWhen",
                "plainRatio",
                "shortIntPairs",
                "digitIntOrDate",
                "plainQe",
                "shortColorCT",
            ])
        );
        let ss = schema(version, &body);
        check_rows(
            &mut failures,
            version,
            &ss,
            &[
                ("<color>blue</color>", Invalid, ENUM),
                ("<color>red</color>", Valid, None),
                ("<plainColor>blue</plainColor>", Invalid, ENUM),
                ("<plainColor>red</plainColor>", Valid, None),
                ("<plainColor> red </plainColor>", Invalid, ENUM),
                ("<plainPlainColor>blue</plainPlainColor>", Invalid, ENUM),
                ("<plainPlainColor>green</plainPlainColor>", Valid, None),
                ("<shortColor>blue</shortColor>", Invalid, ENUM),
                ("<shortColor>red</shortColor>", Valid, None),
                ("<rColor>rose</rColor>", Invalid, ENUM),
                ("<rColor>red</rColor>", Valid, None),
                ("<redOnly>green</redOnly>", Invalid, ENUM),
                ("<redOnly>red</redOnly>", Valid, None),
                ("<smallSizes>0</smallSizes>", Invalid, ENUM),
                ("<smallSizes>5</smallSizes>", Invalid, MAX_IN),
                ("<smallSizes>1</smallSizes>", Valid, None),
                ("<plainSizes>2</plainSizes>", Invalid, ENUM),
                ("<plainSizes>05</plainSizes>", Valid, None),
                ("<plainWhen>2000-01-02</plainWhen>", Invalid, ENUM),
                ("<plainWhen>2000-01-01</plainWhen>", Valid, None),
                ("<plainRatio>0.25</plainRatio>", Invalid, ENUM),
                ("<plainRatio>5E-1</plainRatio>", Valid, None),
                ("<shortIntPairs>1 3</shortIntPairs>", Invalid, ENUM),
                ("<shortIntPairs>3 04</shortIntPairs>", Valid, None),
                ("<digitIntOrDate>2</digitIntOrDate>", Invalid, ENUM),
                ("<digitIntOrDate>2000-01-02</digitIntOrDate>", Invalid, ENUM),
                ("<digitIntOrDate>1</digitIntOrDate>", Valid, None),
                ("<digitIntOrDate>2000-01-01</digitIntOrDate>", Valid, None),
                ("<plainQe>b</plainQe>", Invalid, ENUM),
                ("<plainQe>a</plainQe>", Valid, None),
                // A complex type with simple content checks its base's
                // facets through the base (Structures §3.4.2.2 clause 1.2).
                ("<shortColorCT>blue</shortColorCT>", Invalid, ENUM),
                (
                    "<shortColorCT>green</shortColorCT>",
                    Invalid,
                    Some("cvc-maxLength-valid"),
                ),
                ("<shortColorCT>red</shortColorCT>", Valid, None),
                (r#"<att c="blue"/>"#, Invalid, ENUM),
                (r#"<att c="green"/>"#, Valid, None),
                (r#"<att p="gif"/>"#, Invalid, ENUM),
                (r#"<att p="png"/>"#, Valid, None),
            ],
        );
    }
    assert_no_failures(&failures);
}

/// Datatypes §4.3.5.5 enumeration valid restriction: "It is an ·error· if any
/// member of {value} is not in the ·value space· of {base type definition}" —
/// the value space of a restriction of an enumerated type is the enumerated
/// values, so a further restriction cannot enumerate another one. A value
/// constraint must be valid for its type as well (Structures §3.3.6.2).
#[test]
fn an_enumeration_or_default_outside_the_inherited_enumeration_is_a_schema_error() {
    let base = r#"
      <xs:simpleType name="color"><xs:restriction base="xs:string">
        <xs:enumeration value="red"/><xs:enumeration value="green"/></xs:restriction></xs:simpleType>
      <xs:simpleType name="plainColor"><xs:restriction base="color"/></xs:simpleType>"#;
    for version in VERSIONS {
        let sub = format!(
            r#"{base}<xs:simpleType name="blueOnly"><xs:restriction base="plainColor">
                 <xs:enumeration value="blue"/></xs:restriction></xs:simpleType>"#
        );
        assert_eq!(
            schema_error(version, &sub).as_deref(),
            Some("enumeration-valid-restriction"),
            "{version:?}: enumeration 'blue' under plainColor"
        );
        let ok = format!(
            r#"{base}<xs:simpleType name="greenOnly"><xs:restriction base="plainColor">
                 <xs:enumeration value="green"/></xs:restriction></xs:simpleType>"#
        );
        assert_eq!(schema_error(version, &ok), None, "{version:?}: green");

        let default = format!(r#"{base}<xs:element name="d" type="plainColor" default="blue"/>"#);
        assert!(
            schema_error(version, &default).is_some(),
            "{version:?}: default 'blue' for plainColor compiles"
        );
        let default = format!(r#"{base}<xs:element name="d" type="plainColor" default="red"/>"#);
        assert_eq!(
            schema_error(version, &default),
            None,
            "{version:?}: default red"
        );
    }
}

// ── C45: NaN is incomparable with every bound ────────────────────────────

const FLOATING_BOUNDS: &str = r#"
  <xs:simpleType name="dMinIn"><xs:restriction base="xs:double"><xs:minInclusive value="0"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="dMaxIn"><xs:restriction base="xs:double"><xs:maxInclusive value="100"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="dMinEx"><xs:restriction base="xs:double"><xs:minExclusive value="0"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="dMaxEx"><xs:restriction base="xs:double"><xs:maxExclusive value="100"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="pct"><xs:restriction base="xs:double">
    <xs:minInclusive value="0"/><xs:maxInclusive value="100"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="dPadded"><xs:restriction base="xs:double"><xs:maxInclusive value=" 100 "/></xs:restriction></xs:simpleType>
  <xs:simpleType name="fPadded"><xs:restriction base="xs:float"><xs:minExclusive value="&#9;0&#10;"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="dUpToInf"><xs:restriction base="xs:double"><xs:maxInclusive value="INF"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="fMinIn"><xs:restriction base="xs:float"><xs:minInclusive value="0"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="fMaxIn"><xs:restriction base="xs:float"><xs:maxInclusive value="100"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="fMinEx"><xs:restriction base="xs:float"><xs:minExclusive value="0"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="fMaxEx"><xs:restriction base="xs:float"><xs:maxExclusive value="100"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="fFromMinusInf"><xs:restriction base="xs:float"><xs:minInclusive value="-INF"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="dPlain"><xs:restriction base="xs:double"/></xs:simpleType>
  <xs:simpleType name="dNaNOrHalf"><xs:restriction base="xs:double">
    <xs:enumeration value="NaN"/><xs:enumeration value="0.5"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="nanOnly"><xs:restriction base="xs:double"><xs:pattern value="NaN"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="pctOrNaN"><xs:union memberTypes="pct nanOnly"/></xs:simpleType>
  <xs:simpleType name="pcts"><xs:list itemType="pct"/></xs:simpleType>"#;

/// Datatypes 1.1 §3.3.4.1 / §3.3.5.1 (float, double): "NaN is ·incomparable·
/// with any value in the ·value space· including itself", and "Any value
/// ·incomparable· with the value used for the four bounding facets … will be
/// excluded from the resulting restricted ·value space·. … If any other value
/// is used for a bounding facet, NaN will be excluded from the resulting
/// restricted ·value space·; to add NaN back in requires union with the
/// NaN-only space (which may be derived using the pattern 'NaN')". §4.3.10.3
/// minInclusive Valid: "if and only if the value is greater than or equal to
/// {value}, according to the datatype's order relation" (§4.3.7.3 – §4.3.9.3
/// alike).
#[test]
fn nan_is_outside_every_bound() {
    let mut failures = Vec::new();
    for version in VERSIONS {
        let body = format!(
            "{FLOATING_BOUNDS}{}",
            elements(&[
                "dMinIn",
                "dMaxIn",
                "dMinEx",
                "dMaxEx",
                "pct",
                "dUpToInf",
                "dPadded",
                "fPadded",
                "fMinIn",
                "fMaxIn",
                "fMinEx",
                "fMaxEx",
                "fFromMinusInf",
                "dPlain",
                "dNaNOrHalf",
                "pctOrNaN",
                "pcts",
            ])
        );
        let ss = schema(version, &body);
        check_rows(
            &mut failures,
            version,
            &ss,
            &[
                ("<dMinIn>NaN</dMinIn>", Invalid, MIN_IN),
                ("<dMinIn>0</dMinIn>", Valid, None),
                ("<dMinIn>-0</dMinIn>", Valid, None),
                ("<dMinIn>INF</dMinIn>", Valid, None),
                ("<dMinIn>-INF</dMinIn>", Invalid, MIN_IN),
                ("<dMaxIn>NaN</dMaxIn>", Invalid, MAX_IN),
                ("<dMaxIn>-INF</dMaxIn>", Valid, None),
                ("<dMinEx>NaN</dMinEx>", Invalid, MIN_EX),
                ("<dMinEx>0</dMinEx>", Invalid, MIN_EX),
                ("<dMinEx>1E-300</dMinEx>", Valid, None),
                ("<dMaxEx>NaN</dMaxEx>", Invalid, MAX_EX),
                ("<dMaxEx>99.5</dMaxEx>", Valid, None),
                ("<pct>NaN</pct>", Invalid, MIN_IN),
                ("<pct> NaN </pct>", Invalid, MIN_IN),
                ("<pct>50</pct>", Valid, None),
                ("<pct>101</pct>", Invalid, MAX_IN),
                // A bound is read after the whitespace collapse of its
                // datatype, as every other datatype's bounds are.
                ("<dPadded>101</dPadded>", Invalid, MAX_IN),
                ("<dPadded>100</dPadded>", Valid, None),
                ("<fPadded>0</fPadded>", Invalid, MIN_EX),
                ("<fPadded>1</fPadded>", Valid, None),
                ("<dUpToInf>NaN</dUpToInf>", Invalid, MAX_IN),
                ("<dUpToInf>INF</dUpToInf>", Valid, None),
                ("<fMinIn>NaN</fMinIn>", Invalid, MIN_IN),
                ("<fMinIn>0</fMinIn>", Valid, None),
                ("<fMaxIn>NaN</fMaxIn>", Invalid, MAX_IN),
                ("<fMaxIn>100</fMaxIn>", Valid, None),
                ("<fMinEx>NaN</fMinEx>", Invalid, MIN_EX),
                ("<fMinEx>INF</fMinEx>", Valid, None),
                ("<fMaxEx>NaN</fMaxEx>", Invalid, MAX_EX),
                ("<fMaxEx>-INF</fMaxEx>", Valid, None),
                ("<fFromMinusInf>NaN</fFromMinusInf>", Invalid, MIN_IN),
                ("<fFromMinusInf>-INF</fFromMinusInf>", Valid, None),
                // No bound: NaN is a value of the type.
                ("<dPlain>NaN</dPlain>", Valid, None),
                // enumeration: "equal or identical", and NaN is identical to
                // itself.
                ("<dNaNOrHalf>NaN</dNaNOrHalf>", Valid, None),
                ("<dNaNOrHalf>0.25</dNaNOrHalf>", Invalid, ENUM),
                // "to add NaN back in requires union with the NaN-only space".
                ("<pctOrNaN>NaN</pctOrNaN>", Valid, None),
                ("<pctOrNaN>50</pctOrNaN>", Valid, None),
                ("<pctOrNaN>101</pctOrNaN>", Invalid, Some("cvc-simple-type")),
                ("<pcts>1 NaN</pcts>", Invalid, MIN_IN),
                ("<pcts>1 2</pcts>", Valid, None),
            ],
        );
    }
    assert_no_failures(&failures);
}

// ── C46: a value with a time zone offset and one without ─────────────────

/// One type with an `enumeration` value that has a time zone offset, one with
/// the same value without it, for each date/time datatype.
const ZONED_ENUMERATIONS: &[(&str, &str, &str)] = &[
    ("dateTime", "2000-01-01T00:00:00Z", "2000-01-01T00:00:00"),
    ("date", "2000-01-01Z", "2000-01-01"),
    ("time", "12:00:00Z", "12:00:00"),
    ("gYearMonth", "2000-01Z", "2000-01"),
    ("gYear", "2000Z", "2000"),
    ("gMonthDay", "--01-01Z", "--01-01"),
    ("gDay", "---01Z", "---01"),
    ("gMonth", "--01Z", "--01"),
];

/// Datatypes 1.1 §D.2.1: values of a date/time datatype "are ordered the same
/// as their ·timeOnTimeline· values, except that if one value's
/// ·timezoneOffset· is absent and the other's is not, and using maximum and
/// minimum ·timezoneOffset· values for the one whose ·timezoneOffset· is
/// actually absent changes the resulting (strict) inequality, the original two
/// values are incomparable" — so a value without a time zone offset is never
/// equal to one with an offset, and §4.3.5.4 enumeration valid ("equal or
/// identical to one of the values specified in {value}") does not hold for
/// it, in either direction.
#[test]
fn a_value_without_a_time_zone_is_no_enumerated_value_with_one() {
    let mut body = String::new();
    for (ty, zoned, local) in ZONED_ENUMERATIONS {
        body.push_str(&format!(
            r#"<xs:simpleType name="{ty}Z"><xs:restriction base="xs:{ty}"><xs:enumeration value="{zoned}"/></xs:restriction></xs:simpleType>
               <xs:simpleType name="{ty}Local"><xs:restriction base="xs:{ty}"><xs:enumeration value="{local}"/></xs:restriction></xs:simpleType>
               <xs:element name="{ty}Z" type="{ty}Z"/><xs:element name="{ty}Local" type="{ty}Local"/>"#
        ));
    }
    body.push_str(
        r#"<xs:simpleType name="dates"><xs:list itemType="xs:date"/></xs:simpleType>
           <xs:simpleType name="zonedDates"><xs:restriction base="dates"><xs:enumeration value="2000-01-01Z"/></xs:restriction></xs:simpleType>
           <xs:simpleType name="intOrDate"><xs:union memberTypes="xs:integer xs:date"/></xs:simpleType>
           <xs:simpleType name="zonedIntOrDate"><xs:restriction base="intOrDate"><xs:enumeration value="2000-01-01Z"/></xs:restriction></xs:simpleType>
           <xs:element name="zonedDates" type="zonedDates"/><xs:element name="zonedIntOrDate" type="zonedIntOrDate"/>"#,
    );
    let mut failures = Vec::new();
    for version in VERSIONS {
        let ss = schema(version, &body);
        let mut rows: Vec<(String, SchemaValidity, Option<&str>)> = Vec::new();
        for (ty, zoned, local) in ZONED_ENUMERATIONS {
            let plus_zero = zoned.replace('Z', "+00:00");
            rows.push((format!("<{ty}Z>{local}</{ty}Z>"), Invalid, ENUM));
            rows.push((format!("<{ty}Z>{zoned}</{ty}Z>"), Valid, None));
            rows.push((format!("<{ty}Z>{plus_zero}</{ty}Z>"), Valid, None));
            rows.push((format!("<{ty}Local>{zoned}</{ty}Local>"), Invalid, ENUM));
            rows.push((format!("<{ty}Local>{plus_zero}</{ty}Local>"), Invalid, ENUM));
            rows.push((format!("<{ty}Local>{local}</{ty}Local>"), Valid, None));
        }
        // The same instant in another time zone is an equal value.
        rows.push((
            "<dateTimeZ>1999-12-31T19:00:00-05:00</dateTimeZ>".to_string(),
            Valid,
            None,
        ));
        rows.push(("<timeZ>13:00:00+01:00</timeZ>".to_string(), Valid, None));
        rows.push((
            "<zonedDates>2000-01-01</zonedDates>".to_string(),
            Invalid,
            ENUM,
        ));
        rows.push((
            "<zonedDates>2000-01-01Z</zonedDates>".to_string(),
            Valid,
            None,
        ));
        rows.push((
            "<zonedIntOrDate>2000-01-01</zonedIntOrDate>".to_string(),
            Invalid,
            ENUM,
        ));
        rows.push((
            "<zonedIntOrDate>2000-01-01Z</zonedIntOrDate>".to_string(),
            Valid,
            None,
        ));
        let rows: Vec<(&str, SchemaValidity, Option<&str>)> =
            rows.iter().map(|(x, v, c)| (x.as_str(), *v, *c)).collect();
        check_rows(&mut failures, version, &ss, &rows);
    }
    assert_no_failures(&failures);
}

const ZONED_BOUNDS: &str = r#"
  <xs:simpleType name="dtMinZ"><xs:restriction base="xs:dateTime"><xs:minInclusive value="2000-01-01T00:00:00Z"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="dtMaxZ"><xs:restriction base="xs:dateTime"><xs:maxInclusive value="2000-01-01T00:00:00Z"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="dtAfterZ"><xs:restriction base="xs:dateTime"><xs:minExclusive value="2000-01-01T00:00:00Z"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="dtBeforeZ"><xs:restriction base="xs:dateTime"><xs:maxExclusive value="2000-01-01T00:00:00Z"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="dtNoonMinZ"><xs:restriction base="xs:dateTime"><xs:minInclusive value="2000-01-01T12:00:00Z"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="dtMinLocal"><xs:restriction base="xs:dateTime"><xs:minInclusive value="2000-01-01T00:00:00"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="dMinZ"><xs:restriction base="xs:date"><xs:minInclusive value="2000-01-01Z"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="tMinZ"><xs:restriction base="xs:time"><xs:minInclusive value="12:00:00Z"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="gyMinZ"><xs:restriction base="xs:gYear"><xs:minInclusive value="2000Z"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="gmMaxLocal"><xs:restriction base="xs:gMonth"><xs:maxInclusive value="--06"/></xs:restriction></xs:simpleType>"#;

/// The bounding facets compare in the same order (§D.2.1, above): with a time
/// zone offset of +14:00 and of −14:00 imputed to the value that has none,
/// both comparisons must come out the same way, or the two values are
/// ·incomparable· and the bound does not hold (§4.3.10.3 minInclusive Valid:
/// "greater than or equal to {value}, according to the datatype's order
/// relation"; §4.3.7.3 – §4.3.9.3 alike). Comparable values keep their
/// verdict.
#[test]
fn bounds_compare_values_with_and_without_a_time_zone_in_the_partial_order() {
    let mut failures = Vec::new();
    for version in VERSIONS {
        let body = format!(
            "{ZONED_BOUNDS}{}",
            elements(&[
                "dtMinZ",
                "dtMaxZ",
                "dtAfterZ",
                "dtBeforeZ",
                "dtNoonMinZ",
                "dtMinLocal",
                "dMinZ",
                "tMinZ",
                "gyMinZ",
                "gmMaxLocal",
            ])
        );
        let ss = schema(version, &body);
        check_rows(
            &mut failures,
            version,
            &ss,
            &[
                // Incomparable: 2000-01-01T00:00:00 lies between
                // 1999-12-31T10:00:00Z (+14:00) and 2000-01-01T14:00:00Z
                // (−14:00).
                ("<dtMinZ>2000-01-01T00:00:00</dtMinZ>", Invalid, MIN_IN),
                // At the edge one imputation gives equality, the other not.
                ("<dtMinZ>2000-01-01T14:00:00</dtMinZ>", Invalid, MIN_IN),
                ("<dtMinZ>2000-01-01T14:00:01</dtMinZ>", Valid, None),
                ("<dtMinZ>1999-12-31T09:59:59</dtMinZ>", Invalid, MIN_IN),
                ("<dtMinZ>2000-01-01T00:00:00Z</dtMinZ>", Valid, None),
                ("<dtMinZ>1999-12-31T23:00:00-01:00</dtMinZ>", Valid, None),
                ("<dtMaxZ>2000-01-01T00:00:00</dtMaxZ>", Invalid, MAX_IN),
                ("<dtMaxZ>1999-12-31T09:59:59</dtMaxZ>", Valid, None),
                ("<dtAfterZ>2000-01-01T00:00:00</dtAfterZ>", Invalid, MIN_EX),
                ("<dtAfterZ>2000-01-02T00:00:00</dtAfterZ>", Valid, None),
                (
                    "<dtBeforeZ>2000-01-01T00:00:00</dtBeforeZ>",
                    Invalid,
                    MAX_EX,
                ),
                ("<dtBeforeZ>1999-12-31T00:00:00</dtBeforeZ>", Valid, None),
                // Already rejected before: below the bound either way.
                (
                    "<dtNoonMinZ>2000-01-01T00:00:00</dtNoonMinZ>",
                    Invalid,
                    MIN_IN,
                ),
                ("<dtNoonMinZ>2000-01-02T03:00:00</dtNoonMinZ>", Valid, None),
                // The bound without a time zone, the value with one.
                (
                    "<dtMinLocal>2000-01-01T00:00:00Z</dtMinLocal>",
                    Invalid,
                    MIN_IN,
                ),
                ("<dtMinLocal>2000-01-01T00:00:00</dtMinLocal>", Valid, None),
                ("<dtMinLocal>2000-01-02T00:00:00Z</dtMinLocal>", Valid, None),
                ("<dMinZ>2000-01-01</dMinZ>", Invalid, MIN_IN),
                ("<dMinZ>2000-01-02</dMinZ>", Valid, None),
                ("<tMinZ>12:00:00</tMinZ>", Invalid, MIN_IN),
                ("<tMinZ>13:00:00Z</tMinZ>", Valid, None),
                ("<gyMinZ>2000</gyMinZ>", Invalid, MIN_IN),
                ("<gyMinZ>2001</gyMinZ>", Valid, None),
                ("<gmMaxLocal>--06Z</gmMaxLocal>", Invalid, MAX_IN),
                ("<gmMaxLocal>--05Z</gmMaxLocal>", Valid, None),
                ("<gmMaxLocal>--06</gmMaxLocal>", Valid, None),
            ],
        );
    }
    assert_no_failures(&failures);
}

// ── Fix round: the g* types on the 1972 time line (review F1) ────────────

const G_BOUNDS: &str = r#"
  <xs:simpleType name="fromJuneLocal"><xs:restriction base="xs:gMonth"><xs:minInclusive value="--06"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="gmMinNeg14"><xs:restriction base="xs:gMonth"><xs:minInclusive value="--06-14:00"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="gmMaxPos14"><xs:restriction base="xs:gMonth"><xs:maxInclusive value="--06+14:00"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="gmMaxNeg14"><xs:restriction base="xs:gMonth"><xs:maxInclusive value="--06-14:00"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="gdMinLocal"><xs:restriction base="xs:gDay"><xs:minInclusive value="---15"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="gdMinNeg14"><xs:restriction base="xs:gDay"><xs:minInclusive value="---15-14:00"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="gmdMinNeg14"><xs:restriction base="xs:gMonthDay"><xs:minInclusive value="--06-15-14:00"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="gyMinNeg14"><xs:restriction base="xs:gYear"><xs:minInclusive value="2000-14:00"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="gymMaxPos14"><xs:restriction base="xs:gYearMonth"><xs:maxInclusive value="2000-06+14:00"/></xs:restriction></xs:simpleType>"#;

/// Datatypes 1.1 §E.3.4 ·timeOnTimeline·: "Let yr be 1971 when dt's ·year·
/// is absent, and dt's ·year· − 1 otherwise, mo be 12 or dt's ·month·,
/// similarly, da be ·daysInMonth·(yr+1, mo) − 1 or (dt's ·day·) − 1,
/// similarly" — a `gMonth` is a day of 1972, so two months are about 30 days
/// apart, never within the 28 hours that two time zone offsets span (XSD 1.0
/// §3.2.14: "the order relation on gMonth is the order relation on their
/// starting instants"). Every g* type, offsets of 10 hours and more, the bound
/// or the value without an offset, and both with one.
#[test]
fn g_types_order_on_the_1972_time_line() {
    let mut failures = Vec::new();
    for version in VERSIONS {
        let body = format!(
            "{G_BOUNDS}{}",
            elements(&[
                "fromJuneLocal",
                "gmMinNeg14",
                "gmMaxPos14",
                "gmMaxNeg14",
                "gdMinLocal",
                "gdMinNeg14",
                "gmdMinNeg14",
                "gyMinNeg14",
                "gymMaxPos14",
            ])
        );
        let ss = schema(version, &body);
        check_rows(
            &mut failures,
            version,
            &ss,
            &[
                ("<fromJuneLocal>--07+10:00</fromJuneLocal>", Valid, None),
                ("<fromJuneLocal>--07-14:00</fromJuneLocal>", Valid, None),
                ("<fromJuneLocal>--06</fromJuneLocal>", Valid, None),
                // Equal with +14:00 imputed to the bound, less with −14:00.
                ("<fromJuneLocal>--06+14:00</fromJuneLocal>", Invalid, MIN_IN),
                ("<fromJuneLocal>--05</fromJuneLocal>", Invalid, MIN_IN),
                ("<gmMinNeg14>--07</gmMinNeg14>", Valid, None),
                ("<gmMinNeg14>--07+14:00</gmMinNeg14>", Valid, None),
                ("<gmMinNeg14>--06-14:00</gmMinNeg14>", Valid, None),
                ("<gmMinNeg14>--06+14:00</gmMinNeg14>", Invalid, MIN_IN),
                ("<gmMinNeg14>--06</gmMinNeg14>", Invalid, MIN_IN),
                ("<gmMaxPos14>--05</gmMaxPos14>", Valid, None),
                ("<gmMaxPos14>--05-14:00</gmMaxPos14>", Valid, None),
                ("<gmMaxPos14>--06</gmMaxPos14>", Invalid, MAX_IN),
                ("<gmMaxNeg14>--07+14:00</gmMaxNeg14>", Invalid, MAX_IN),
                ("<gmMaxNeg14>--06+14:00</gmMaxNeg14>", Valid, None),
                ("<gdMinLocal>---17+10:00</gdMinLocal>", Valid, None),
                ("<gdMinLocal>---16-10:00</gdMinLocal>", Valid, None),
                ("<gdMinLocal>---16+10:00</gdMinLocal>", Invalid, MIN_IN),
                ("<gdMinNeg14>---16+10:00</gdMinNeg14>", Valid, None),
                ("<gdMinNeg14>---16+11:00</gdMinNeg14>", Invalid, MIN_IN),
                ("<gdMinNeg14>---15</gdMinNeg14>", Invalid, MIN_IN),
                ("<gmdMinNeg14>--06-16+10:00</gmdMinNeg14>", Valid, None),
                ("<gmdMinNeg14>--06-17</gmdMinNeg14>", Valid, None),
                ("<gmdMinNeg14>--06-16</gmdMinNeg14>", Invalid, MIN_IN),
                ("<gyMinNeg14>2001+14:00</gyMinNeg14>", Valid, None),
                ("<gyMinNeg14>2000+10:00</gyMinNeg14>", Invalid, MIN_IN),
                ("<gyMinNeg14>2000</gyMinNeg14>", Invalid, MIN_IN),
                ("<gymMaxPos14>2000-05</gymMaxPos14>", Valid, None),
                ("<gymMaxPos14>2000-05+14:00</gymMaxPos14>", Valid, None),
                ("<gymMaxPos14>2000-06-14:00</gymMaxPos14>", Invalid, MAX_IN),
            ],
        );
        // The same order when the schema compiles: a value of the base type
        // as an enumeration or a default, and two bounds in order.
        for (what, body) in [
            (
                "enumeration --07+10:00 under fromJuneLocal",
                format!(
                    r#"{G_BOUNDS}<xs:simpleType name="t"><xs:restriction base="fromJuneLocal">
                         <xs:enumeration value="--07+10:00"/></xs:restriction></xs:simpleType>"#
                ),
            ),
            (
                "default --07+10:00 for fromJuneLocal",
                format!(
                    r#"{G_BOUNDS}<xs:element name="d" type="fromJuneLocal" default="--07+10:00"/>"#
                ),
            ),
            (
                "minInclusive --06-14:00 with maxInclusive --07+14:00",
                r#"<xs:simpleType name="t"><xs:restriction base="xs:gMonth">
                     <xs:minInclusive value="--06-14:00"/><xs:maxInclusive value="--07+14:00"/>
                   </xs:restriction></xs:simpleType>"#
                    .to_string(),
            ),
        ] {
            if let Some(error) = schema_error(version, &body) {
                failures.push(format!("{version:?}: {what}: {error}"));
            }
        }
    }
    assert_no_failures(&failures);
}

// ── Fix round: XSD 1.0, a NaN bound (review F2) ───────────────────────────

const NAN_BOUNDS: &str = r#"
  <xs:simpleType name="nanMinIn"><xs:restriction base="xs:double"><xs:minInclusive value="NaN"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="nanMaxIn"><xs:restriction base="xs:float"><xs:maxInclusive value=" NaN "/></xs:restriction></xs:simpleType>
  <xs:simpleType name="nanBoth"><xs:restriction base="xs:double">
    <xs:minInclusive value="NaN"/><xs:maxInclusive value="NaN"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="nanMinEx"><xs:restriction base="xs:double"><xs:minExclusive value="NaN"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="nanAndFive"><xs:restriction base="xs:double">
    <xs:minInclusive value="NaN"/><xs:maxInclusive value="5"/></xs:restriction></xs:simpleType>"#;

/// XSD 1.0 §3.2.4 (float; §3.2.5 double): "NaN equals itself but is
/// ·incomparable· with (neither greater than nor less than) any other value",
/// so "when "NaN" is used as a facet value for a bounding facet, … the result
/// is a ·value space· either having NaN as its only member (the inclusive
/// cases) or that is empty (the exclusive cases)". Datatypes 1.1 §3.3.4.1:
/// "NaN is ·incomparable· with any value in the ·value space· including
/// itself", and "the result is a ·value space· that is empty".
#[test]
fn an_inclusive_nan_bound_admits_nan_in_xsd_1_0_only() {
    let mut failures = Vec::new();
    for version in VERSIONS {
        let v10 = version == XsdVersion::V1_0;
        let only_in_10 = |xml| {
            if v10 {
                (xml, Valid, None)
            } else {
                (xml, Invalid, None)
            }
        };
        let body = format!(
            "{NAN_BOUNDS}{}",
            elements(&["nanMinIn", "nanMaxIn", "nanBoth", "nanMinEx", "nanAndFive"])
        );
        let ss = schema(version, &body);
        let mut rows = vec![
            only_in_10("<nanMinIn>NaN</nanMinIn>"),
            only_in_10("<nanMaxIn>NaN</nanMaxIn>"),
            only_in_10("<nanBoth> NaN </nanBoth>"),
            ("<nanMinIn>0</nanMinIn>", Invalid, MIN_IN),
            ("<nanMinIn>INF</nanMinIn>", Invalid, MIN_IN),
            ("<nanMaxIn>-INF</nanMaxIn>", Invalid, MAX_IN),
            ("<nanMinEx>NaN</nanMinEx>", Invalid, MIN_EX),
            ("<nanAndFive>NaN</nanAndFive>", Invalid, None),
            ("<nanAndFive>1</nanAndFive>", Invalid, MIN_IN),
        ];
        // An Invalid row with no code stands for "some error".
        for row in &mut rows {
            if row.1 == Invalid && row.2.is_none() {
                row.2 = Some("cvc-");
            }
        }
        check_rows(&mut failures, version, &ss, &rows);
        for (what, extra) in [
            (
                "enumeration NaN under nanMinIn",
                r#"<xs:simpleType name="t"><xs:restriction base="nanMinIn">
                     <xs:enumeration value="NaN"/></xs:restriction></xs:simpleType>"#,
            ),
            (
                "default NaN for nanMinIn",
                r#"<xs:element name="d" type="nanMinIn" default="NaN"/>"#,
            ),
            (
                "maxInclusive NaN under nanMinIn",
                r#"<xs:simpleType name="t"><xs:restriction base="nanMinIn">
                     <xs:maxInclusive value="NaN"/></xs:restriction></xs:simpleType>"#,
            ),
        ] {
            let error = schema_error(version, &format!("{NAN_BOUNDS}{extra}"));
            if error.is_some() == v10 {
                failures.push(format!("{version:?}: {what}: {error:?}"));
            }
        }
    }
    assert_no_failures(&failures);
}

// ── Fix round: simple content restrictions (review F5) ───────────────────

const SIMPLE_CONTENT: &str = r#"
  <xs:complexType name="num"><xs:simpleContent>
    <xs:extension base="xs:double"><xs:attribute name="u" type="xs:string"/></xs:extension>
  </xs:simpleContent></xs:complexType>
  <xs:complexType name="pctCT"><xs:simpleContent><xs:restriction base="num">
    <xs:minInclusive value="0"/><xs:maxInclusive value="100"/>
  </xs:restriction></xs:simpleContent></xs:complexType>
  <xs:complexType name="plainPctCT"><xs:simpleContent><xs:restriction base="pctCT"/></xs:simpleContent></xs:complexType>
  <xs:complexType name="when"><xs:simpleContent>
    <xs:extension base="xs:date"><xs:attribute name="u" type="xs:string"/></xs:extension>
  </xs:simpleContent></xs:complexType>
  <xs:complexType name="whenMin"><xs:simpleContent><xs:restriction base="when">
    <xs:minInclusive value="2000-01-01Z"/>
  </xs:restriction></xs:simpleContent></xs:complexType>
  <xs:complexType name="whenZ"><xs:simpleContent><xs:restriction base="when">
    <xs:enumeration value="2000-01-01Z"/>
  </xs:restriction></xs:simpleContent></xs:complexType>
  <xs:complexType name="amount"><xs:simpleContent>
    <xs:extension base="xs:decimal"><xs:attribute name="cur" type="xs:string"/></xs:extension>
  </xs:simpleContent></xs:complexType>
  <xs:complexType name="price"><xs:simpleContent><xs:restriction base="amount">
    <xs:totalDigits value="4"/><xs:fractionDigits value="2"/><xs:maxExclusive value="100"/>
  </xs:restriction></xs:simpleContent></xs:complexType>
  <xs:complexType name="half"><xs:simpleContent><xs:restriction base="amount">
    <xs:enumeration value="0.5"/>
  </xs:restriction></xs:simpleContent></xs:complexType>
  <xs:complexType name="codes"><xs:simpleContent>
    <xs:extension base="xs:string"><xs:attribute name="u" type="xs:string"/></xs:extension>
  </xs:simpleContent></xs:complexType>
  <xs:complexType name="code3"><xs:simpleContent><xs:restriction base="codes">
    <xs:pattern value="[A-Z]+"/><xs:maxLength value="3"/>
  </xs:restriction></xs:simpleContent></xs:complexType>
  <xs:simpleType name="ints"><xs:list itemType="xs:integer"/></xs:simpleType>
  <xs:complexType name="intList"><xs:simpleContent>
    <xs:extension base="ints"><xs:attribute name="u" type="xs:string"/></xs:extension>
  </xs:simpleContent></xs:complexType>
  <xs:complexType name="intPair"><xs:simpleContent><xs:restriction base="intList">
    <xs:length value="2"/>
  </xs:restriction></xs:simpleContent></xs:complexType>
  <xs:simpleType name="intOrDate"><xs:union memberTypes="xs:integer xs:date"/></xs:simpleType>
  <xs:complexType name="mixedValue"><xs:simpleContent>
    <xs:extension base="intOrDate"><xs:attribute name="u" type="xs:string"/></xs:extension>
  </xs:simpleContent></xs:complexType>
  <xs:complexType name="oneOrY2k"><xs:simpleContent><xs:restriction base="mixedValue">
    <xs:enumeration value="1"/><xs:enumeration value="2000-01-01"/>
  </xs:restriction></xs:simpleContent></xs:complexType>"#;

/// Structures 1.1 §3.4.2.2 (complex type with simple content, restriction):
/// the {content type}'s simple type definition has "{facets} a set of facet
/// components corresponding to the appropriate element information items
/// among the <restriction>'s [children] (i.e. those which specify facets, if
/// any), as defined in Simple Type Restriction (Facets) (§3.16.6.4)" — the
/// facets of a simple type, so the bounds, the digits and the enumeration
/// compare values in the datatype's order and equality, the patterns the
/// lexical form, and a list's length counts items.
#[test]
fn simple_content_restrictions_check_value_facets() {
    let mut failures = Vec::new();
    for version in VERSIONS {
        let body = format!(
            "{SIMPLE_CONTENT}{}",
            [
                ("pct", "pctCT"),
                ("plainPct", "plainPctCT"),
                ("whenMin", "whenMin"),
                ("whenZ", "whenZ"),
                ("price", "price"),
                ("half", "half"),
                ("code", "code3"),
                ("pair", "intPair"),
                ("oneOrY2k", "oneOrY2k"),
            ]
            .iter()
            .map(|(e, t)| format!(r#"<xs:element name="{e}" type="{t}"/>"#))
            .collect::<String>()
        );
        let ss = schema(version, &body);
        check_rows(
            &mut failures,
            version,
            &ss,
            &[
                ("<pct>50</pct>", Valid, None),
                ("<pct u='x'>-0</pct>", Valid, None),
                ("<pct>101</pct>", Invalid, MAX_IN),
                ("<pct>NaN</pct>", Invalid, MIN_IN),
                ("<pct>-INF</pct>", Invalid, MIN_IN),
                ("<plainPct>101</plainPct>", Invalid, MAX_IN),
                ("<plainPct>100</plainPct>", Valid, None),
                ("<whenMin>1999-01-01Z</whenMin>", Invalid, MIN_IN),
                ("<whenMin>2000-01-01</whenMin>", Invalid, MIN_IN),
                ("<whenMin>2000-01-02Z</whenMin>", Valid, None),
                ("<whenZ>2000-01-01+00:00</whenZ>", Valid, None),
                ("<whenZ>2000-01-01</whenZ>", Invalid, ENUM),
                ("<price>99.99</price>", Valid, None),
                (
                    "<price>1.005</price>",
                    Invalid,
                    Some("cvc-fractionDigits-valid"),
                ),
                ("<price>100</price>", Invalid, MAX_EX),
                ("<half>0.50</half>", Valid, None),
                ("<half>0.25</half>", Invalid, ENUM),
                ("<code>ABC</code>", Valid, None),
                ("<code>ABCD</code>", Invalid, Some("cvc-maxLength-valid")),
                ("<code>abc</code>", Invalid, Some("cvc-pattern-valid")),
                ("<pair>1 2</pair>", Valid, None),
                ("<pair>12</pair>", Invalid, Some("cvc-length-valid")),
                ("<pair>1 2 3</pair>", Invalid, Some("cvc-length-valid")),
                ("<oneOrY2k>01</oneOrY2k>", Valid, None),
                ("<oneOrY2k>2000-01-01</oneOrY2k>", Valid, None),
                ("<oneOrY2k>2</oneOrY2k>", Invalid, ENUM),
            ],
        );
        // A default must be valid for the content type.
        let default =
            format!(r#"{SIMPLE_CONTENT}<xs:element name="d" type="pctCT" default="101"/>"#);
        if schema_error(version, &default).is_none() {
            failures.push(format!("{version:?}: default 101 for pctCT compiles"));
        }
    }
    assert_no_failures(&failures);
}

/// XSD 1.1: an `assertion` facet among the facets of a simple content
/// restriction constrains the value too (Datatypes 1.1 §4.3.13.3).
#[cfg(feature = "xsd11")]
#[test]
fn simple_content_restrictions_check_assertion_facets() {
    let body = r#"
      <xs:complexType name="amount"><xs:simpleContent>
        <xs:extension base="xs:decimal"><xs:attribute name="cur" type="xs:string"/></xs:extension>
      </xs:simpleContent></xs:complexType>
      <xs:complexType name="nonZero"><xs:simpleContent><xs:restriction base="amount">
        <xs:assertion test="$value ne 0"/>
      </xs:restriction></xs:simpleContent></xs:complexType>
      <xs:element name="n" type="nonZero"/>"#;
    let ss = schema(XsdVersion::V1_1, body);
    let mut failures = Vec::new();
    check_rows(
        &mut failures,
        XsdVersion::V1_1,
        &ss,
        &[
            ("<n>1.5</n>", Valid, None),
            ("<n>0.0</n>", Invalid, Some("cvc-assertion")),
        ],
    );
    assert_no_failures(&failures);
}

// ── Fix round: binary and g* enumerations by value (review F3) ────────────

const VALUE_ENUMERATIONS: &str = r#"
  <xs:simpleType name="hx"><xs:restriction base="xs:hexBinary"><xs:enumeration value="0a"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="plainHx"><xs:restriction base="hx"/></xs:simpleType>
  <xs:simpleType name="b64"><xs:restriction base="xs:base64Binary"><xs:enumeration value="YQ=="/></xs:restriction></xs:simpleType>
  <xs:simpleType name="plainB64"><xs:restriction base="b64"/></xs:simpleType>
  <xs:simpleType name="gmz"><xs:restriction base="xs:gMonth"><xs:enumeration value="--02+00:00"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="plainGmz"><xs:restriction base="gmz"/></xs:simpleType>
  <xs:simpleType name="gdFar"><xs:restriction base="xs:gDay"><xs:enumeration value="---15-11:00"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="gmdFar"><xs:restriction base="xs:gMonthDay"><xs:enumeration value="--12-31+14:00"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="gyz"><xs:restriction base="xs:gYear"><xs:enumeration value="2000+00:00"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="gymz"><xs:restriction base="xs:gYearMonth"><xs:enumeration value="2000-01-00:00"/></xs:restriction></xs:simpleType>"#;

/// Datatypes 1.1 §4.3.5.4 enumeration valid: "equal or identical to one of
/// the values specified in {value}". A hexBinary or base64Binary value is a
/// sequence of octets (§3.3.15.1, §3.3.16.1), so `0A` is the value of `0a`;
/// a g* value is equal to another by the §D.2.1 order the bounds use, so
/// `--02Z` is `--02+00:00` and `---16+13:00` is `---15-11:00` (§3.3.13.1:
/// "---15−11:00 = ---16+13:00"). A restriction inherits the comparison with
/// the enumeration.
#[test]
fn binary_and_g_enumerations_compare_values() {
    let mut failures = Vec::new();
    for version in VERSIONS {
        let body = format!(
            "{VALUE_ENUMERATIONS}{}",
            elements(&[
                "hx", "plainHx", "b64", "plainB64", "gmz", "plainGmz", "gdFar", "gmdFar", "gyz",
                "gymz",
            ])
        );
        let ss = schema(version, &body);
        check_rows(
            &mut failures,
            version,
            &ss,
            &[
                ("<hx>0A</hx>", Valid, None),
                ("<hx>0a</hx>", Valid, None),
                ("<hx>0b</hx>", Invalid, ENUM),
                ("<plainHx>0A</plainHx>", Valid, None),
                ("<plainHx>0B</plainHx>", Invalid, ENUM),
                ("<b64>YQ==</b64>", Valid, None),
                ("<b64>Yg==</b64>", Invalid, ENUM),
                ("<plainB64>YQ==</plainB64>", Valid, None),
                ("<plainB64>Yg==</plainB64>", Invalid, ENUM),
                ("<gmz>--02Z</gmz>", Valid, None),
                ("<gmz>--02+00:00</gmz>", Valid, None),
                ("<gmz>--02</gmz>", Invalid, ENUM),
                ("<gmz>--02+01:00</gmz>", Invalid, ENUM),
                ("<plainGmz>--02Z</plainGmz>", Valid, None),
                ("<plainGmz>--02</plainGmz>", Invalid, ENUM),
                ("<gdFar>---16+13:00</gdFar>", Valid, None),
                ("<gdFar>---15-11:00</gdFar>", Valid, None),
                ("<gdFar>---15</gdFar>", Invalid, ENUM),
                ("<gmdFar>--12-30-10:00</gmdFar>", Valid, None),
                ("<gmdFar>--12-31</gmdFar>", Invalid, ENUM),
                ("<gyz>2000Z</gyz>", Valid, None),
                ("<gyz>2000</gyz>", Invalid, ENUM),
                ("<gymz>2000-01Z</gymz>", Valid, None),
                ("<gymz>2000-01</gymz>", Invalid, ENUM),
            ],
        );
        for (what, extra) in [
            (
                "enumeration 0A under plainHx",
                r#"<xs:simpleType name="t"><xs:restriction base="plainHx">
                     <xs:enumeration value="0A"/></xs:restriction></xs:simpleType>"#,
            ),
            (
                "default 0A for plainHx",
                r#"<xs:element name="d" type="plainHx" default="0A"/>"#,
            ),
            (
                "enumeration --02Z under plainGmz",
                r#"<xs:simpleType name="t"><xs:restriction base="plainGmz">
                     <xs:enumeration value="--02Z"/></xs:restriction></xs:simpleType>"#,
            ),
        ] {
            if let Some(error) = schema_error(version, &format!("{VALUE_ENUMERATIONS}{extra}")) {
                failures.push(format!("{version:?}: {what}: {error}"));
            }
        }
    }
    assert_no_failures(&failures);
}

// ── Fix round: QName and NOTATION chains (review F4) ─────────────────────

/// Datatypes §4.3.5.5 enumeration valid restriction: "It is an ·error· if any
/// member of {value} is not in the ·value space· of {base type definition}",
/// through any number of restrictions without an enumeration of their own.
/// QName and NOTATION enumerations are compared by their lexical form here,
/// as the facet check compares them (a known gap).
#[test]
fn qname_and_notation_enumerations_stay_within_the_inherited_one() {
    let base = r#"
      <xs:notation name="png" public="image/png"/>
      <xs:notation name="gif" public="image/gif"/>
      <xs:simpleType name="qe"><xs:restriction base="xs:QName"><xs:enumeration value="a"/></xs:restriction></xs:simpleType>
      <xs:simpleType name="plainQe"><xs:restriction base="qe"/></xs:simpleType>
      <xs:simpleType name="picture"><xs:restriction base="xs:NOTATION"><xs:enumeration value="png"/></xs:restriction></xs:simpleType>
      <xs:simpleType name="plainPicture"><xs:restriction base="picture"/></xs:simpleType>"#;
    let mut failures = Vec::new();
    for version in VERSIONS {
        for (what, extra, error) in [
            (
                "QName b under plainQe",
                r#"<xs:simpleType name="t"><xs:restriction base="plainQe"><xs:enumeration value="b"/></xs:restriction></xs:simpleType>"#,
                Some("enumeration-valid-restriction"),
            ),
            (
                "QName a under plainQe",
                r#"<xs:simpleType name="t"><xs:restriction base="plainQe"><xs:enumeration value="a"/></xs:restriction></xs:simpleType>"#,
                None,
            ),
            (
                "NOTATION gif under plainPicture",
                r#"<xs:simpleType name="t"><xs:restriction base="plainPicture"><xs:enumeration value="gif"/></xs:restriction></xs:simpleType>"#,
                Some("enumeration-valid-restriction"),
            ),
            (
                "NOTATION png under plainPicture",
                r#"<xs:simpleType name="t"><xs:restriction base="plainPicture"><xs:enumeration value="png"/></xs:restriction></xs:simpleType>"#,
                None,
            ),
        ] {
            let got = schema_error(version, &format!("{base}{extra}"));
            if got.as_deref() != error {
                failures.push(format!("{version:?}: {what}: {got:?} (expected {error:?})"));
            }
        }
    }
    assert_no_failures(&failures);
}

// ── Fix round 2: whiteSpace before the facets (review U1) ────────────────

const SIMPLE_CONTENT_WHITESPACE: &str = r#"
  <xs:complexType name="text"><xs:simpleContent>
    <xs:extension base="xs:string"><xs:attribute name="u" type="xs:string"/></xs:extension>
  </xs:simpleContent></xs:complexType>
  <xs:complexType name="collapsedText"><xs:simpleContent><xs:restriction base="text">
    <xs:whiteSpace value="collapse"/></xs:restriction></xs:simpleContent></xs:complexType>
  <xs:complexType name="short"><xs:simpleContent><xs:restriction base="text">
    <xs:maxLength value="3"/></xs:restriction></xs:simpleContent></xs:complexType>
  <xs:complexType name="shortCollapsed"><xs:simpleContent><xs:restriction base="short">
    <xs:whiteSpace value="collapse"/></xs:restriction></xs:simpleContent></xs:complexType>
  <xs:complexType name="ab"><xs:simpleContent><xs:restriction base="text">
    <xs:enumeration value="a b"/></xs:restriction></xs:simpleContent></xs:complexType>
  <xs:complexType name="abCollapsed"><xs:simpleContent><xs:restriction base="ab">
    <xs:whiteSpace value="collapse"/></xs:restriction></xs:simpleContent></xs:complexType>
  <xs:complexType name="abReplaced"><xs:simpleContent><xs:restriction base="ab">
    <xs:whiteSpace value="replace"/></xs:restriction></xs:simpleContent></xs:complexType>
  <xs:simpleType name="letters"><xs:restriction base="xs:string">
    <xs:pattern value="[a-z]( [a-z])*"/></xs:restriction></xs:simpleType>
  <xs:complexType name="lettersText"><xs:simpleContent>
    <xs:extension base="letters"><xs:attribute name="u" type="xs:string"/></xs:extension>
  </xs:simpleContent></xs:complexType>
  <xs:complexType name="lettersCollapsed"><xs:simpleContent><xs:restriction base="lettersText">
    <xs:whiteSpace value="collapse"/></xs:restriction></xs:simpleContent></xs:complexType>
  <xs:complexType name="padded"><xs:simpleContent><xs:restriction base="text">
    <xs:whiteSpace value="collapse"/><xs:enumeration value=" a "/></xs:restriction></xs:simpleContent></xs:complexType>
  <xs:complexType name="tokText"><xs:simpleContent>
    <xs:extension base="xs:token"><xs:attribute name="u" type="xs:string"/></xs:extension>
  </xs:simpleContent></xs:complexType>
  <xs:complexType name="tokColor"><xs:simpleContent><xs:restriction base="tokText">
    <xs:enumeration value=" red "/><xs:enumeration value="blue"/></xs:restriction></xs:simpleContent></xs:complexType>"#;

/// Structures 1.1 §3.4.4.2 Element Locally Valid (Complex Type) clause 1.2:
/// "the ·initial value· of E is ·valid· with respect to T.{content
/// type}.{simple type definition} as defined by String Valid (§3.16.4)",
/// whose clause 1 computes the ·normalized value· "using the whiteSpace
/// facet, and any other pre-lexical facets associated with T"; the content
/// type of a simple content restriction is a restriction of the base's with
/// the `<restriction>`'s facets overlaid (§3.4.2.2, §3.16.6.4), so its
/// whiteSpace is the most derived one, and every facet of the chain — the
/// base's length, pattern and enumeration too — sees the value normalized
/// with it. An enumeration value of the restriction is a value of the base
/// content type (Datatypes 1.1 §4.3.5.2), normalized with the base's
/// whiteSpace (`padded`: `" a "` of `xs:string` keeps its spaces).
#[test]
fn simple_content_restrictions_normalize_with_their_own_white_space() {
    let mut failures = Vec::new();
    for version in VERSIONS {
        let body = format!(
            "{SIMPLE_CONTENT_WHITESPACE}{}",
            elements(&[
                "collapsedText",
                "short",
                "shortCollapsed",
                "abCollapsed",
                "abReplaced",
                "lettersCollapsed",
                "padded",
                "tokColor",
            ])
        );
        let ss = schema(version, &body);
        check_rows(
            &mut failures,
            version,
            &ss,
            &[
                ("<collapsedText>  a   b  </collapsedText>", Valid, None),
                ("<short> ab </short>", Invalid, Some("cvc-maxLength-valid")),
                ("<shortCollapsed>  ab  </shortCollapsed>", Valid, None),
                (
                    "<shortCollapsed> a  b c </shortCollapsed>",
                    Invalid,
                    Some("cvc-maxLength-valid"),
                ),
                ("<abCollapsed>  a   b </abCollapsed>", Valid, None),
                ("<abCollapsed>a  c</abCollapsed>", Invalid, ENUM),
                ("<abReplaced>a&#9;b</abReplaced>", Valid, None),
                ("<abReplaced>a  b</abReplaced>", Invalid, ENUM),
                ("<lettersCollapsed>  a   b </lettersCollapsed>", Valid, None),
                (
                    "<lettersCollapsed> a 1 </lettersCollapsed>",
                    Invalid,
                    Some("cvc-pattern-valid"),
                ),
                ("<padded> a </padded>", Invalid, ENUM),
                ("<padded>a</padded>", Invalid, ENUM),
                ("<tokColor>red</tokColor>", Valid, None),
                ("<tokColor>  red </tokColor>", Valid, None),
                ("<tokColor>green</tokColor>", Invalid, ENUM),
            ],
        );
        for (what, extra) in [
            (
                "default '  ab  ' for shortCollapsed",
                r#"<xs:element name="d" type="shortCollapsed" default="  ab  "/>"#,
            ),
            (
                "default 'red' for tokColor",
                r#"<xs:element name="d" type="tokColor" default="red"/>"#,
            ),
            (
                "fixed ' a  b ' for abCollapsed",
                r#"<xs:element name="d" type="abCollapsed" fixed=" a  b "/>"#,
            ),
        ] {
            if let Some(error) =
                schema_error(version, &format!("{SIMPLE_CONTENT_WHITESPACE}{extra}"))
            {
                failures.push(format!("{version:?}: {what}: {error}"));
            }
        }
    }
    assert_no_failures(&failures);
}

/// The PSVI `[schema normalized value]` and the typed value of an element
/// whose type is a simple content restriction with its own whiteSpace are
/// those of the normalized string (Structures §3.1.4: the ·normalized value·
/// is normalized "according to the value of the whiteSpace facet … associated
/// with the simple type definition used in its ·validation·").
#[test]
fn simple_content_restrictions_report_the_normalized_value() {
    use xsd_schema::namespace::context::NamespaceContextSnapshot;
    let mut failures = Vec::new();
    for version in VERSIONS {
        let body = format!(
            "{SIMPLE_CONTENT_WHITESPACE}{}",
            elements(&["collapsedText", "tokColor"])
        );
        let ss = schema(version, &body);
        for (element, text, expected) in [
            ("collapsedText", "  a \t b  ", "a b"),
            ("tokColor", "  red ", "red"),
        ] {
            let v = SchemaValidator::new(&ss, ValidationFlags::default());
            let mut errors = Vec::new();
            let mut warnings = Vec::new();
            let mut rt = v.start_run(CollectingValidationSink {
                errors: &mut errors,
                warnings: &mut warnings,
            });
            let ns = NamespaceContextSnapshot::default();
            rt.validate_element(element, "", None, None, &ns);
            rt.validate_end_of_attributes();
            rt.validate_text(text);
            let info = rt.validate_end_element();
            rt.end_validation().expect("end_validation");
            drop(rt);
            let typed = info.typed_value.as_ref().map(|v| v.to_string_value());
            if info.validity != Valid
                || info.normalized_value.as_deref() != Some(expected)
                || typed.as_deref() != Some(expected)
            {
                failures.push(format!(
                    "{version:?}: {element} {text:?} => {:?} normalized {:?} typed {typed:?} {:?} (expected {expected:?})",
                    info.validity,
                    info.normalized_value,
                    errors.iter().map(|e| e.constraint.to_string()).collect::<Vec<_>>()
                ));
            }
        }
    }
    assert_no_failures(&failures);
}

/// XSD 1.1: the assertions of a simple content restriction — its assertion
/// facets and its `xs:assert`s — see `$value` as the value of the
/// normalized string (Datatypes 1.1 §4.3.13.3 clause 1.4: `$value` is "the
/// ·XDM representation· of V under T").
#[cfg(feature = "xsd11")]
#[test]
fn simple_content_assertions_see_the_normalized_value() {
    let body = format!(
        r#"{SIMPLE_CONTENT_WHITESPACE}
      <xs:complexType name="isAB"><xs:simpleContent><xs:restriction base="text">
        <xs:whiteSpace value="collapse"/><xs:assertion test="$value eq 'a b'"/>
      </xs:restriction></xs:simpleContent></xs:complexType>
      <xs:complexType name="notAB"><xs:simpleContent><xs:restriction base="text">
        <xs:whiteSpace value="collapse"/><xs:assertion test="$value ne 'a b'"/>
      </xs:restriction></xs:simpleContent></xs:complexType>
      <xs:complexType name="assertsAB"><xs:simpleContent><xs:restriction base="text">
        <xs:whiteSpace value="collapse"/><xs:assert test="$value eq 'a b'"/>
      </xs:restriction></xs:simpleContent></xs:complexType>
      <xs:complexType name="isRed"><xs:simpleContent><xs:restriction base="tokColor">
        <xs:assertion test="$value eq 'red'"/>
      </xs:restriction></xs:simpleContent></xs:complexType>
      {}"#,
        elements(&["isAB", "notAB", "assertsAB", "isRed"])
    );
    let ss = schema(XsdVersion::V1_1, &body);
    let mut failures = Vec::new();
    check_rows(
        &mut failures,
        XsdVersion::V1_1,
        &ss,
        &[
            ("<isAB>a b</isAB>", Valid, None),
            ("<isAB> a  b </isAB>", Valid, None),
            ("<isAB>a c</isAB>", Invalid, Some("cvc-assertion")),
            ("<notAB> a  b </notAB>", Invalid, Some("cvc-assertion")),
            ("<notAB>x</notAB>", Valid, None),
            ("<isRed> red </isRed>", Valid, None),
            ("<isRed>blue</isRed>", Invalid, Some("cvc-assertion")),
        ],
    );
    // A complex type's `xs:assert` is evaluated with PROCESS_ASSERTIONS.
    check_rows_with(
        &mut failures,
        XsdVersion::V1_1,
        &ss,
        &[
            ("<assertsAB> a  b </assertsAB>", Valid, None),
            ("<assertsAB>a c</assertsAB>", Invalid, Some("cvc-assertion")),
        ],
        ValidationFlags::PROCESS_ASSERTIONS,
    );
    let default = format!(r#"{body}<xs:element name="d" type="isAB" default=" a  b "/>"#);
    if let Some(error) = schema_error(XsdVersion::V1_1, &default) {
        failures.push(format!("default ' a  b ' for isAB: {error}"));
    }
    assert_no_failures(&failures);
}

// ── Fix round 2: enumeration values of the base type (review U2) ─────────

const BASE_TYPE_ENUMERATIONS: &str = r#"
  <xs:notation name="png" public="image/png"/>
  <xs:simpleType name="color"><xs:restriction base="xs:token">
    <xs:enumeration value=" red "/><xs:enumeration value="blue"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="color2"><xs:restriction base="color"/></xs:simpleType>
  <xs:simpleType name="color3"><xs:restriction base="color"><xs:maxLength value="10"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="colors"><xs:list itemType="color"/></xs:simpleType>
  <xs:simpleType name="padded"><xs:restriction base="xs:string">
    <xs:enumeration value=" red "/></xs:restriction></xs:simpleType>
  <xs:simpleType name="paddedCollapsed"><xs:restriction base="padded">
    <xs:whiteSpace value="collapse"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="ownWs"><xs:restriction base="xs:string">
    <xs:whiteSpace value="collapse"/><xs:enumeration value=" red "/></xs:restriction></xs:simpleType>
  <xs:simpleType name="tabbed"><xs:restriction base="xs:normalizedString">
    <xs:enumeration value="a&#9;b"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="toks"><xs:list itemType="xs:token"/></xs:simpleType>
  <xs:simpleType name="pair"><xs:restriction base="toks">
    <xs:enumeration value="  a   b  "/></xs:restriction></xs:simpleType>
  <xs:simpleType name="collapsedString"><xs:restriction base="xs:string">
    <xs:whiteSpace value="collapse"/></xs:restriction></xs:simpleType>
  <xs:simpleType name="wordOrInt"><xs:union memberTypes="collapsedString xs:int"/></xs:simpleType>
  <xs:simpleType name="onlyA"><xs:restriction base="wordOrInt">
    <xs:enumeration value=" a "/></xs:restriction></xs:simpleType>
  <xs:simpleType name="pNot"><xs:restriction base="xs:NOTATION">
    <xs:enumeration value=" png "/></xs:restriction></xs:simpleType>
  <xs:element name="att"><xs:complexType>
    <xs:attribute name="c" type="color2"/>
    <xs:attribute name="p" type="pNot"/>
  </xs:complexType></xs:element>
  <xs:element name="local"><xs:simpleType><xs:restriction base="xs:token">
    <xs:enumeration value=" red "/></xs:restriction></xs:simpleType></xs:element>"#;

/// Datatypes 1.1 §4.3.5.1: an enumeration's "{value} A set of values from the
/// ·value space· of the {base type definition}", and §4.3.5.2: the members
/// are "the actual values of all the <enumeration> [children]'s value
/// [attributes], interpreted as instances of the {base type definition}"
/// ("the actual value is identified by using the ·lexical mapping· of the
/// {base type definition}"), the whiteSpace facet first (Structures §3.1.4).
/// So `" red "` of an `xs:token` type is `red`, and a restriction of it
/// inherits `red`; `" red "` of an `xs:string` type keeps its spaces, also
/// under a later restriction that collapses (which then admits no value).
#[test]
fn enumeration_values_are_values_of_the_base_type() {
    let mut failures = Vec::new();
    for version in VERSIONS {
        let body = format!(
            "{BASE_TYPE_ENUMERATIONS}{}",
            elements(&[
                "color",
                "color2",
                "color3",
                "colors",
                "padded",
                "paddedCollapsed",
                "ownWs",
                "tabbed",
                "pair",
                "onlyA",
            ])
        );
        let ss = schema(version, &body);
        check_rows(
            &mut failures,
            version,
            &ss,
            &[
                ("<color>red</color>", Valid, None),
                ("<color> red </color>", Valid, None),
                ("<color>blue</color>", Valid, None),
                ("<color>green</color>", Invalid, ENUM),
                ("<color2>red</color2>", Valid, None),
                ("<color2>  red</color2>", Valid, None),
                ("<color2>green</color2>", Invalid, ENUM),
                ("<color3>red</color3>", Valid, None),
                ("<color3>green</color3>", Invalid, ENUM),
                ("<colors>red  blue red</colors>", Valid, None),
                ("<colors>red green</colors>", Invalid, ENUM),
                ("<padded> red </padded>", Valid, None),
                ("<padded>red</padded>", Invalid, ENUM),
                ("<paddedCollapsed> red </paddedCollapsed>", Invalid, ENUM),
                ("<paddedCollapsed>red</paddedCollapsed>", Invalid, ENUM),
                ("<ownWs> red </ownWs>", Invalid, ENUM),
                ("<tabbed>a b</tabbed>", Valid, None),
                ("<tabbed>a&#9;b</tabbed>", Valid, None),
                ("<tabbed>ab</tabbed>", Invalid, ENUM),
                ("<pair>a b</pair>", Valid, None),
                ("<pair>a c</pair>", Invalid, ENUM),
                ("<onlyA> a </onlyA>", Valid, None),
                ("<onlyA>a</onlyA>", Valid, None),
                ("<onlyA>b</onlyA>", Invalid, ENUM),
                ("<onlyA>1</onlyA>", Invalid, ENUM),
                (r#"<att c=" red " p=" png "/>"#, Valid, None),
                (r#"<att c="green"/>"#, Invalid, ENUM),
                ("<local>red</local>", Valid, None),
            ],
        );
    }
    assert_no_failures(&failures);
}

/// Every primitive datatype (and the string types whose own whiteSpace is
/// not `collapse` is covered above): an enumeration value written with
/// surrounding spaces is the value its base type's lexical mapping gives
/// after whiteSpace collapse.
#[test]
fn padded_enumeration_values_of_every_primitive() {
    let mut types = vec![
        ("pBool", "xs:boolean", " true ", "1"),
        ("pDec", "xs:decimal", " 1.5 ", "1.50"),
        ("pInt", "xs:integer", " 7 ", "7"),
        ("pFloat", "xs:float", " 0.5 ", "0.5"),
        ("pDouble", "xs:double", " 0.5 ", "5E-1"),
        ("pDur", "xs:duration", " P1D ", "PT24H"),
        (
            "pDt",
            "xs:dateTime",
            " 2000-01-01T00:00:00Z ",
            "2000-01-01T00:00:00Z",
        ),
        ("pTime", "xs:time", " 12:00:00 ", "12:00:00"),
        ("pDate", "xs:date", " 2000-01-01 ", "2000-01-01"),
        ("pGym", "xs:gYearMonth", " 2000-01 ", "2000-01"),
        ("pGy", "xs:gYear", " 2000 ", "2000"),
        ("pGmd", "xs:gMonthDay", " --01-01 ", "--01-01"),
        ("pGd", "xs:gDay", " ---01 ", "---01"),
        ("pGm", "xs:gMonth", " --01 ", "--01"),
        ("pHex", "xs:hexBinary", " 0a ", "0A"),
        ("pB64", "xs:base64Binary", " YQ== ", "YQ=="),
        (
            "pUri",
            "xs:anyURI",
            " http://example.com/ ",
            "http://example.com/",
        ),
        ("pQn", "xs:QName", " a ", "a"),
        ("pTok", "xs:token", " red ", "red"),
        ("pLang", "xs:language", " en ", "en"),
        ("pName", "xs:Name", " ab ", "ab"),
        ("pNcName", "xs:NCName", " ab ", "ab"),
        ("pNmtoken", "xs:NMTOKEN", " ab ", "ab"),
        ("pId", "xs:ID", " ab ", "ab"),
    ];
    let xsd11_types = [
        ("pDtd", "xs:dayTimeDuration", " PT1H ", "PT60M"),
        ("pYmd", "xs:yearMonthDuration", " P1Y ", "P12M"),
        (
            "pDts",
            "xs:dateTimeStamp",
            " 2000-01-01T00:00:00Z ",
            "2000-01-01T00:00:00Z",
        ),
    ];
    let mut failures = Vec::new();
    for version in VERSIONS {
        if version == XsdVersion::V1_1 {
            types.extend(xsd11_types);
        }
        let mut body = String::new();
        for (name, base, literal, _) in &types {
            body.push_str(&format!(
                r#"<xs:simpleType name="{name}"><xs:restriction base="{base}">
                     <xs:enumeration value="{literal}"/></xs:restriction></xs:simpleType>
                   <xs:simpleType name="{name}2"><xs:restriction base="{name}"/></xs:simpleType>
                   <xs:element name="{name}" type="{name}"/><xs:element name="{name}2" type="{name}2"/>"#
            ));
        }
        let ss = schema(version, &body);
        let mut rows = Vec::new();
        for (name, _, literal, instance) in &types {
            rows.push((format!("<{name}>{instance}</{name}>"), Valid, None));
            rows.push((format!("<{name}>{literal}</{name}>"), Valid, None));
            rows.push((format!("<{name}2>{instance}</{name}2>"), Valid, None));
        }
        let rows: Vec<(&str, SchemaValidity, Option<&str>)> = rows
            .iter()
            .map(|(xml, validity, code)| (xml.as_str(), *validity, *code))
            .collect();
        check_rows(&mut failures, version, &ss, &rows);
    }
    assert_no_failures(&failures);
}

/// The schema-time checks that compare enumeration values compare the values
/// of the base type: a further restriction's subset of the enumeration
/// (Datatypes §4.3.5.5 enumeration valid restriction, through the chain for
/// QName too), and a default or fixed value of an element (Structures
/// §3.3.6.2 e-props-correct.2).
#[test]
fn schema_time_enumeration_checks_compare_base_type_values() {
    let base = r#"
      <xs:simpleType name="color"><xs:restriction base="xs:token">
        <xs:enumeration value=" red "/><xs:enumeration value="blue"/></xs:restriction></xs:simpleType>
      <xs:simpleType name="color2"><xs:restriction base="color"/></xs:simpleType>
      <xs:simpleType name="padded"><xs:restriction base="xs:string">
        <xs:enumeration value=" red "/></xs:restriction></xs:simpleType>
      <xs:simpleType name="qe"><xs:restriction base="xs:QName">
        <xs:enumeration value=" a "/></xs:restriction></xs:simpleType>
      <xs:simpleType name="plainQe"><xs:restriction base="qe"/></xs:simpleType>"#;
    let mut failures = Vec::new();
    for version in VERSIONS {
        for (what, extra, compiles) in [
            (
                "enumeration 'red' under color",
                r#"<xs:simpleType name="t"><xs:restriction base="color"><xs:enumeration value="red"/></xs:restriction></xs:simpleType>"#,
                true,
            ),
            (
                "enumeration ' red' under color",
                r#"<xs:simpleType name="t"><xs:restriction base="color"><xs:enumeration value=" red"/></xs:restriction></xs:simpleType>"#,
                true,
            ),
            (
                "enumeration 'red' under color2",
                r#"<xs:simpleType name="t"><xs:restriction base="color2"><xs:enumeration value="red"/></xs:restriction></xs:simpleType>"#,
                true,
            ),
            (
                "enumeration 'green' under color2",
                r#"<xs:simpleType name="t"><xs:restriction base="color2"><xs:enumeration value="green"/></xs:restriction></xs:simpleType>"#,
                false,
            ),
            (
                "enumeration 'red' under padded",
                r#"<xs:simpleType name="t"><xs:restriction base="padded"><xs:enumeration value="red"/></xs:restriction></xs:simpleType>"#,
                false,
            ),
            (
                "QName 'a' under plainQe",
                r#"<xs:simpleType name="t"><xs:restriction base="plainQe"><xs:enumeration value="a"/></xs:restriction></xs:simpleType>"#,
                true,
            ),
            (
                "default 'red' for color2",
                r#"<xs:element name="d" type="color2" default="red"/>"#,
                true,
            ),
            (
                "default ' red ' for color",
                r#"<xs:element name="d" type="color" default=" red "/>"#,
                true,
            ),
            (
                "fixed 'red' for color2",
                r#"<xs:element name="d" type="color2" fixed="red"/>"#,
                true,
            ),
            (
                "attribute default 'red' for color2",
                r#"<xs:element name="e"><xs:complexType><xs:attribute name="a" type="color2" default="red"/></xs:complexType></xs:element>"#,
                true,
            ),
            (
                "default 'green' for color2",
                r#"<xs:element name="d" type="color2" default="green"/>"#,
                false,
            ),
            (
                "default 'red' for padded",
                r#"<xs:element name="d" type="padded" default="red"/>"#,
                false,
            ),
        ] {
            let error = schema_error(version, &format!("{base}{extra}"));
            if error.is_none() != compiles {
                failures.push(format!(
                    "{version:?}: {what}: {error:?} (expected to {})",
                    if compiles { "compile" } else { "fail" }
                ));
            }
        }
        // A fixed value supplied for an empty element, and compared.
        let fixed = format!(r#"{base}<xs:element name="d" type="color2" fixed="red"/>"#);
        if let Some(error) = schema_error(version, &fixed) {
            failures.push(format!("{version:?}: fixed red: {error}"));
        } else {
            let ss = schema(version, &fixed);
            check_rows(
                &mut failures,
                version,
                &ss,
                &[
                    ("<d/>", Valid, None),
                    ("<d> red </d>", Valid, None),
                    ("<d>blue</d>", Invalid, Some("cvc-elt.5.2.2")),
                ],
            );
        }
    }
    assert_no_failures(&failures);
}

// ── Fix round 2: facet values read with their own whiteSpace ─────────────

/// The `value` of `xs:whiteSpace` (and, in XSD 1.1, of
/// `xs:explicitTimezone`) is an `xs:NMTOKEN` in the schema for schemas
/// (Datatypes 1.1 Appendix A), so `" preserve "` means `preserve`. The other
/// facet values are read the same way: lengths and digits as the
/// `xs:nonNegativeInteger` / `xs:positiveInteger` they are, bounds as values
/// of the base type (guard rows).
#[test]
fn white_space_facet_values_are_collapsed() {
    let body = r#"
      <xs:simpleType name="kept"><xs:restriction base="xs:string">
        <xs:whiteSpace value=" preserve "/><xs:length value="3"/></xs:restriction></xs:simpleType>
      <xs:simpleType name="replaced"><xs:restriction base="xs:string">
        <xs:whiteSpace value=" replace "/><xs:length value="3"/></xs:restriction></xs:simpleType>
      <xs:simpleType name="short"><xs:restriction base="xs:string">
        <xs:maxLength value=" 3 "/></xs:restriction></xs:simpleType>
      <xs:simpleType name="upToTen"><xs:restriction base="xs:integer">
        <xs:maxInclusive value=" 10 "/><xs:totalDigits value=" 2 "/></xs:restriction></xs:simpleType>
      <xs:simpleType name="sinceY2k"><xs:restriction base="xs:date">
        <xs:minInclusive value=" 2000-01-01 "/></xs:restriction></xs:simpleType>
      <xs:simpleType name="belowOneHalf"><xs:restriction base="xs:decimal">
        <xs:maxExclusive value=" 1.5 "/><xs:fractionDigits value=" 1 "/></xs:restriction></xs:simpleType>
      <xs:element name="kept" type="kept"/><xs:element name="replaced" type="replaced"/>
      <xs:element name="short" type="short"/><xs:element name="upToTen" type="upToTen"/>
      <xs:element name="sinceY2k" type="sinceY2k"/><xs:element name="belowOneHalf" type="belowOneHalf"/>"#;
    let mut failures = Vec::new();
    for version in VERSIONS {
        let ss = schema(version, body);
        check_rows(
            &mut failures,
            version,
            &ss,
            &[
                ("<kept> a </kept>", Valid, None),
                ("<kept>a</kept>", Invalid, Some("cvc-length-valid")),
                ("<replaced>a&#9; </replaced>", Valid, None),
                ("<replaced>a</replaced>", Invalid, Some("cvc-length-valid")),
                ("<short>abc</short>", Valid, None),
                ("<short>abcd</short>", Invalid, Some("cvc-maxLength-valid")),
                ("<upToTen>10</upToTen>", Valid, None),
                ("<upToTen>11</upToTen>", Invalid, MAX_IN),
                ("<sinceY2k>2000-01-01</sinceY2k>", Valid, None),
                ("<sinceY2k>1999-12-31</sinceY2k>", Invalid, MIN_IN),
                ("<belowOneHalf>1.4</belowOneHalf>", Valid, None),
                ("<belowOneHalf>1.5</belowOneHalf>", Invalid, MAX_EX),
                (
                    "<belowOneHalf>1.25</belowOneHalf>",
                    Invalid,
                    Some("cvc-fractionDigits-valid"),
                ),
            ],
        );
    }
    assert_no_failures(&failures);
}

#[cfg(feature = "xsd11")]
#[test]
fn explicit_timezone_facet_values_are_collapsed() {
    let body = r#"
      <xs:simpleType name="zoned"><xs:restriction base="xs:date">
        <xs:explicitTimezone value=" required "/></xs:restriction></xs:simpleType>
      <xs:element name="zoned" type="zoned"/>"#;
    let mut failures = Vec::new();
    match schema_error(XsdVersion::V1_1, body) {
        Some(error) => failures.push(format!("explicitTimezone ' required ': {error}")),
        None => {
            let ss = schema(XsdVersion::V1_1, body);
            check_rows(
                &mut failures,
                XsdVersion::V1_1,
                &ss,
                &[
                    ("<zoned>2000-01-01Z</zoned>", Valid, None),
                    (
                        "<zoned>2000-01-01</zoned>",
                        Invalid,
                        Some("cvc-explicitTimezone-valid"),
                    ),
                ],
            );
        }
    }
    assert_no_failures(&failures);
}
