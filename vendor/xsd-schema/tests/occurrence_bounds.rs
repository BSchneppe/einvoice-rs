//! P1 of `XSD_COMPILER_REWORK.md` (§8): finite occurrence bounds are exact and
//! execution limits are operational failures, end to end through the public
//! drivers.
//!
//! Structures §3.9.4.3 clause 2.2 (Element Sequence Accepted): "If P.{max
//! occurs} is a number, then the length of the sequence is less than or equal
//! to the {max occurs}." The former `MAX_COUNTED_OCCURS = 10_000` approximation
//! widened larger finite maxima to unbounded, so `a{0,10001}` accepted 10 002
//! children. These tests pin the corrected behaviour independently of the
//! content-model backend.

use xsd_schema::validation::{
    drive_navigator, drive_quick_xml, CollectingValidationSink, DriveError, SchemaValidator,
    SchemaValidity, ValidationError, ValidationFlags,
};
use xsd_schema::{RoXmlNavigator, SchemaSet, SchemaSetBuilder};

fn schema(xsd: &str) -> SchemaSet {
    SchemaSetBuilder::new()
        .add_source(xsd, "file:///occurrence_bounds.xsd")
        .expect("schema source accepted")
        .compile()
        .expect("schema compiles")
        .into_schema_set()
}

fn element_a_schema(min: &str, max: &str) -> String {
    format!(
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="r">
    <xs:complexType>
      <xs:sequence>
        <xs:element name="a" type="xs:string" minOccurs="{min}" maxOccurs="{max}"/>
      </xs:sequence>
    </xs:complexType>
  </xs:element>
</xs:schema>"#
    )
}

fn instance_with_a(n: usize) -> String {
    let mut s = String::with_capacity(8 + 4 * n);
    s.push_str("<r>");
    for _ in 0..n {
        s.push_str("<a/>");
    }
    s.push_str("</r>");
    s
}

struct Run {
    /// `Ok(root validity)` when the driver completed, `Err(constraint)` when it
    /// returned an error (operational failure or unclosed elements).
    outcome: Result<Option<SchemaValidity>, String>,
    errors: Vec<ValidationError>,
}

fn stream(schema_set: &SchemaSet, xml: &str) -> Run {
    let validator = SchemaValidator::new(schema_set, ValidationFlags::default());
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let outcome = {
        let sink = CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        };
        let mut runtime = validator.start_run(sink);
        let outcome = drive_quick_xml(xml.as_bytes(), &mut runtime, schema_set);
        match outcome {
            Ok(o) => {
                assert!(runtime.operational_failure().is_none());
                Ok(o.root_validity)
            }
            Err(DriveError::Validation(e)) => {
                assert_eq!(
                    runtime.operational_failure().map(|f| f.constraint),
                    Some(e.constraint),
                    "the driver error is the recorded operational failure"
                );
                Err(e.constraint.to_string())
            }
            Err(other) => Err(other.to_string()),
        }
    };
    Run { outcome, errors }
}

fn dom(schema_set: &SchemaSet, xml: &str) -> Run {
    let validator = SchemaValidator::new(schema_set, ValidationFlags::default());
    let doc = roxmltree::Document::parse(xml).expect("well-formed instance");
    let nav = RoXmlNavigator::new(&doc);
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let outcome = {
        let sink = CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        };
        let mut runtime = validator.start_run(sink);
        drive_navigator(&nav, &mut runtime, schema_set)
            .map(|o| o.root_validity)
            .map_err(|e| e.constraint.to_string())
    };
    Run { outcome, errors }
}

fn assert_valid(run: &Run, what: &str) {
    assert_eq!(
        run.outcome,
        Ok(Some(SchemaValidity::Valid)),
        "{what}: outcome"
    );
    assert!(run.errors.is_empty(), "{what}: errors {:?}", run.errors);
}

fn assert_invalid_content_model(run: &Run, what: &str) {
    assert_eq!(
        run.outcome,
        Ok(Some(SchemaValidity::Invalid)),
        "{what}: outcome"
    );
    assert!(
        run.errors
            .iter()
            .any(|e| e.constraint.starts_with("cvc-complex-type.2.4")),
        "{what}: expected a content-model violation, got {:?}",
        run.errors
    );
}

/// The reproduced 10 001 / 10 002 case, on both drivers.
#[test]
fn max_occurs_10001_is_exact() {
    let ss = schema(&element_a_schema("0", "10001"));
    for (name, run) in [
        ("stream", stream as fn(&SchemaSet, &str) -> Run),
        ("dom", dom as fn(&SchemaSet, &str) -> Run),
    ] {
        assert_valid(
            &run(&ss, &instance_with_a(10_001)),
            &format!("{name}: 10001 children"),
        );
        assert_invalid_content_model(
            &run(&ss, &instance_with_a(10_002)),
            &format!("{name}: 10002 children"),
        );
    }
}

/// Minima and maxima around the unroll threshold (16) and the former cutoff
/// (10 000): below the minimum, at the minimum, at the maximum, one past it.
#[test]
fn occurrence_boundary_matrix() {
    let mins = [0usize, 1, 2, 16, 17];
    let maxs = [1usize, 2, 15, 16, 17, 9_999, 10_000, 10_001];
    for &min in &mins {
        for &max in &maxs {
            if min > max {
                continue;
            }
            let ss = schema(&element_a_schema(&min.to_string(), &max.to_string()));
            let tag = format!("a{{{min},{max}}}");
            if min > 0 {
                assert_invalid_content_model(
                    &stream(&ss, &instance_with_a(min - 1)),
                    &format!("{tag}: {} children", min - 1),
                );
            }
            assert_valid(
                &stream(&ss, &instance_with_a(min)),
                &format!("{tag}: {min} children"),
            );
            assert_valid(
                &stream(&ss, &instance_with_a(max)),
                &format!("{tag}: {max} children"),
            );
            assert_invalid_content_model(
                &stream(&ss, &instance_with_a(max + 1)),
                &format!("{tag}: {} children", max + 1),
            );
        }
    }
}

/// `unbounded` is still unbounded — the exactness fix only concerns numbers.
#[test]
fn unbounded_stays_unbounded() {
    let ss = schema(&element_a_schema("0", "unbounded"));
    assert_valid(
        &stream(&ss, &instance_with_a(25_000)),
        "unbounded: 25000 children",
    );
}

/// A literal beyond `u32` is schema-valid (nonNegativeInteger has no bound);
/// it saturates to `u32::MAX` — documented in `parse_occurs` — and stays a
/// finite, exact bound rather than becoming `unbounded`.
#[test]
fn huge_occurrence_literal_loads_and_validates() {
    let ss = schema(&element_a_schema("0", "79228162514264337593543950335"));
    assert_valid(
        &stream(&ss, &instance_with_a(5)),
        "huge literal: 5 children",
    );
    let ss = schema(&element_a_schema("2", "4294967296"));
    assert_invalid_content_model(&stream(&ss, &instance_with_a(1)), "huge literal: below min");
    assert_valid(&stream(&ss, &instance_with_a(2)), "huge literal: at min");
}

/// `((a?){0,N}){0,N}` with a huge N: the initial configuration set of the
/// model exceeds the execution limit at validator construction.
fn nested_nullable_schema(prefix: &str) -> String {
    format!(
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="r">
    <xs:complexType>
      <xs:sequence>
        {prefix}
        <xs:sequence minOccurs="0" maxOccurs="1000000">
          <xs:sequence minOccurs="0" maxOccurs="1000000">
            <xs:element name="a" type="xs:string" minOccurs="0"/>
          </xs:sequence>
        </xs:sequence>
      </xs:sequence>
    </xs:complexType>
  </xs:element>
</xs:schema>"#
    )
}

/// A content model that cannot be prepared is not silently treated as empty
/// content: it is reported up front and, if used, aborts the run with
/// `validation-preparation-failed`.
#[test]
fn unpreparable_content_model_is_an_operational_failure() {
    let ss = schema(&nested_nullable_schema(""));
    let validator = SchemaValidator::new(&ss, ValidationFlags::default());
    let failures = validator.content_model_failures();
    assert_eq!(
        failures.len(),
        1,
        "exactly the root type fails: {failures:?}"
    );
    assert!(
        failures[0].1.contains("execution limit exceeded"),
        "{}",
        failures[0].1
    );

    for (name, run) in [
        ("stream", stream as fn(&SchemaSet, &str) -> Run),
        ("dom", dom as fn(&SchemaSet, &str) -> Run),
    ] {
        let run = run(&ss, "<r><a/></r>");
        assert_eq!(
            run.outcome,
            Err("validation-preparation-failed".to_string()),
            "{name}: completion must fail"
        );
        let codes: Vec<_> = run.errors.iter().map(|e| e.constraint).collect();
        assert_eq!(
            codes,
            ["validation-preparation-failed"],
            "{name}: exactly one diagnostic"
        );
    }
}

/// The same pathological model behind a leading `b`: the initial state is
/// small, the explosion happens while advancing over `b` in the document.
/// That is `validation-resource-limit`: the driver returns an error, the sink
/// gets exactly one diagnostic, and no validity verdict is produced.
#[test]
fn execution_limit_mid_document_is_an_operational_failure() {
    let ss = schema(&nested_nullable_schema(
        r#"<xs:element name="b" type="xs:string"/>"#,
    ));
    let validator = SchemaValidator::new(&ss, ValidationFlags::default());
    assert!(
        validator.content_model_failures().is_empty(),
        "initial state is small"
    );

    for (name, run) in [
        ("stream", stream as fn(&SchemaSet, &str) -> Run),
        ("dom", dom as fn(&SchemaSet, &str) -> Run),
    ] {
        let run = run(&ss, "<r><b/><a/><a/></r>");
        assert_eq!(
            run.outcome,
            Err("validation-resource-limit".to_string()),
            "{name}: completion must fail"
        );
        let codes: Vec<_> = run.errors.iter().map(|e| e.constraint).collect();
        assert_eq!(
            codes,
            ["validation-resource-limit"],
            "{name}: exactly one diagnostic"
        );
        let msg = &run.errors[0].message;
        assert!(msg.contains("execution limit"), "{msg}");
        assert!(
            msg.contains("'r'"),
            "names the element whose model failed: {msg}"
        );
    }
}
