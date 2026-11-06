//! The lexical space of `xs:duration`: the fields in their order.
//!
//! Datatypes 1.1 §3.3.6.2: "[15] durationLexicalRep ::= '-'? 'P'
//! ((duYearMonthFrag duDayTimeFrag?) | duDayTimeFrag)" with "[12]
//! duYearMonthFrag ::= (duYearFrag duMonthFrag?) | duMonthFrag", "[13]
//! duTimeFrag ::= 'T' ((duHourFrag duMinuteFrag? duSecondFrag?) |
//! (duMinuteFrag duSecondFrag?) | duSecondFrag)", "[14] duDayTimeFrag ::=
//! (duDayFrag duTimeFrag?) | duTimeFrag" — "Thus, a durationLexicalRep
//! consists of one or more of a duYearFrag, duMonthFrag, duDayFrag,
//! duHourFrag, duMinuteFrag, and/or duSecondFrag, in order". Datatypes 1.0
//! §3.2.6.1 has the same form ("PnYnMnDTnHnMnS").
//!
//! One parser serves schema validation (both versions, both duration
//! subtypes) and XPath casts and constructor functions (F&O §17.1.1: casting
//! a string to a primitive type "is processed as if the value were an
//! xs:string literal of the target type"; an invalid one is FORG0001).

use xsd_schema::validation::{
    drive_quick_xml, validate_simple_type, CollectingValidationSink, SchemaValidator,
    SchemaValidity, ValidationFlags,
};
use xsd_schema::{SchemaSet, SchemaSetBuilder, TypeKey, XsdVersion};

const VERSIONS: [XsdVersion; 2] = [XsdVersion::V1_0, XsdVersion::V1_1];

/// (literal, a lexical form of `xs:duration`)
const LITERALS: &[(&str, bool)] = &[
    // Fields out of order.
    ("P1M1Y", false),
    ("PT1M1H", false),
    ("P1D1Y", false),
    ("P1D1M", false),
    ("PT1S1M", false),
    ("PT1S1H", false),
    ("-P1M1Y", false),
    ("P1Y1D1M", false),
    // In order.
    ("P1Y1M", true),
    ("PT1H1M", true),
    ("P1Y1D", true),
    ("P1M1D", true),
    ("PT1M1S", true),
    ("P1DT1H", true),
    ("-P1Y2M3DT4H5M6.7S", true),
    ("P0D", true),
    // Other lexical errors stay errors.
    ("P1Y1Y", false),
    ("P", false),
    ("PT", false),
];

const SCHEMA: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="d" type="xs:duration"/>
</xs:schema>"#;

fn schema(version: XsdVersion) -> SchemaSet {
    SchemaSetBuilder::with_version(version)
        .add_source(SCHEMA, "file:///duration_lexical.xsd")
        .expect("schema source accepted")
        .compile()
        .unwrap_or_else(|e| panic!("{version:?}: {e}"))
        .into_schema_set()
}

#[test]
fn the_fields_of_a_duration_come_in_order() {
    let mut wrong = Vec::new();
    for version in VERSIONS {
        let ss = schema(version);
        let duration = TypeKey::Simple(ss.builtin_types().duration);
        for &(literal, valid) in LITERALS {
            let got = validate_simple_type(literal, duration, &ss).is_ok();
            if got != valid {
                wrong.push(format!(
                    "{version:?} validate_simple_type({literal:?}): {got}"
                ));
            }
            for psvi in [true, false] {
                let mut flags = ValidationFlags::default();
                if !psvi {
                    flags &= !ValidationFlags::BUILD_PSVI_TYPED_VALUES;
                }
                let v = SchemaValidator::new(&ss, flags);
                let (mut errors, mut warnings) = (Vec::new(), Vec::new());
                let root = {
                    let mut rt = v.start_run(CollectingValidationSink {
                        errors: &mut errors,
                        warnings: &mut warnings,
                    });
                    let xml = format!("<d>{literal}</d>");
                    drive_quick_xml(xml.as_bytes(), &mut rt, &ss)
                        .expect("drive")
                        .root_validity
                };
                let got = root == Some(SchemaValidity::Valid);
                if got != valid {
                    wrong.push(format!(
                        "{version:?} psvi={psvi} <d>{literal}</d>: {got} {errors:?}"
                    ));
                }
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The XPath constructor function, `cast as` and `castable as` read the same
/// lexical space.
#[cfg(feature = "xsd11")]
#[test]
fn xpath_casts_to_a_duration_read_the_fields_in_order() {
    use xsd_schema::namespace::context::NamespaceContextSnapshot;
    use xsd_schema::namespace::table::NameTable;
    use xsd_schema::xpath::{RoXmlNavigator, XPathContext, XPathExpr};
    use xsd_schema::XmlItem;

    let names = NameTable::new();
    let mut namespaces = NamespaceContextSnapshot::default();
    namespaces.bindings.push((
        names.add("xs"),
        names.add("http://www.w3.org/2001/XMLSchema"),
    ));
    let ctx = XPathContext::new(&names).with_namespaces(namespaces);
    let run = |expr: &str| -> String {
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
    };
    let mut wrong = Vec::new();
    for &(literal, valid) in LITERALS {
        for (expr, want) in [
            (
                format!("string(xs:duration('{literal}'))"),
                if valid { None } else { Some("ERR:FORG0001") },
            ),
            (
                format!("string('{literal}' cast as xs:duration)"),
                if valid { None } else { Some("ERR:FORG0001") },
            ),
            (
                format!("'{literal}' castable as xs:duration"),
                Some(if valid { "true" } else { "false" }),
            ),
        ] {
            let got = run(&expr);
            let ok = match want {
                Some(want) => got == want,
                None => !got.starts_with("ERR:"),
            };
            if !ok {
                wrong.push(format!("{expr}: {got}, want {want:?}"));
            }
        }
    }
    // The subtypes keep refusing a misordered literal.
    for expr in [
        "'P1M1Y' castable as xs:yearMonthDuration",
        "'PT1M1H' castable as xs:dayTimeDuration",
    ] {
        if run(expr) != "false" {
            wrong.push(format!("{expr}: {}", run(expr)));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
