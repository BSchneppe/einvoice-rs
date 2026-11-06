//! A directive whose schema document cannot be read is reported, not
//! dropped (C15c).
//!
//! Structures §4.2.3 (Inclusion Constraints and Semantics): "It is not an
//! error for the ·actual value· of the schemaLocation [attribute] to fail to
//! resolve at all, in which case the corresponding inclusion must not be
//! performed. It is an error for it to resolve but the rest of clause 1 above
//! to fail to be satisfied. Failure to resolve is likely to cause less than
//! complete ·assessment· outcomes, of course." §4.2.6.2 (Import Constraints
//! and Semantics): "It is not an error for the application schema component
//! reference strategy to fail."
//!
//! So a schema set compiles without the document, as before — and the
//! failure is kept on the set (`SchemaSet::directive_failures`), so a host can
//! tell a complete schema from an incomplete one. A reference that then fails
//! to resolve names the documents that could not be read.

use std::fs;
use std::path::{Path, PathBuf};

use xsd_schema::schema::CompositionEdgeKind;
use xsd_schema::{load_and_process_schema, SchemaError, SchemaSet, SchemaSetBuilder, XsdVersion};

const XS: &str = r#"xmlns:xs="http://www.w3.org/2001/XMLSchema""#;

fn dir(name: &str, files: &[(&str, String)]) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("directive_failures")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    for (file, text) in files {
        fs::write(dir.join(file), text).unwrap();
    }
    dir
}

fn compile(version: XsdVersion, main: &Path) -> Result<SchemaSet, SchemaError> {
    SchemaSetBuilder::with_version(version)
        .add("", &main.to_string_lossy())?
        .compile()
        .map(|c| c.into_schema_set())
}

fn versions() -> Vec<XsdVersion> {
    if cfg!(feature = "xsd11") {
        vec![XsdVersion::V1_0, XsdVersion::V1_1]
    } else {
        vec![XsdVersion::V1_0]
    }
}

#[test]
fn a_missing_include_is_reported_and_the_set_compiles() {
    let main = format!(
        "<xs:schema {XS} targetNamespace=\"urn:t\">\n  <xs:include schemaLocation=\"missing.xsd\"/>\n  <xs:element name=\"a\"/>\n</xs:schema>"
    );
    let d = dir("missing-include", &[("main.xsd", main)]);
    for version in versions() {
        let set = compile(version, &d.join("main.xsd")).expect("compiles without it");
        let failures = set.directive_failures();
        assert_eq!(failures.len(), 1, "{failures:?}");
        let f = &failures[0];
        assert_eq!(f.kind(), CompositionEdgeKind::Include);
        assert_eq!(f.schema_location(), Some("missing.xsd"));
        assert_eq!(f.namespace(), None);
        assert!(
            f.resolved_location()
                .is_some_and(|r| r.ends_with("missing.xsd") && Path::new(r).is_absolute()),
            "{f:?}"
        );
        let at = f.location().expect("the directive is located");
        assert_eq!((at.line, at.column), (2, 3), "{at}");
        assert!(at.base_uri.ends_with("main.xsd"), "{at}");
        assert!(!f.error().is_schema_content_error(), "{f:?}");
        let text = f.to_string();
        assert!(
            text.contains("xs:include") && text.contains("missing.xsd") && text.contains(":2:3"),
            "{text}"
        );
    }
}

#[test]
fn a_missing_import_is_reported_with_its_namespace() {
    let main = format!(
        "<xs:schema {XS} targetNamespace=\"urn:t\">\n  <xs:import namespace=\"urn:z\" schemaLocation=\"nowhere/z.xsd\"/>\n</xs:schema>"
    );
    let d = dir("missing-import", &[("main.xsd", main)]);
    for version in versions() {
        let set = compile(version, &d.join("main.xsd")).expect("compiles without it");
        let failures = set.directive_failures();
        assert_eq!(failures.len(), 1, "{failures:?}");
        assert_eq!(failures[0].kind(), CompositionEdgeKind::Import);
        assert_eq!(failures[0].namespace(), Some("urn:z"));
        assert_eq!(failures[0].schema_location(), Some("nowhere/z.xsd"));
    }
}

/// The documents that were read are not failures; nor is an import without a
/// location (there is nothing to read), nor a location hint the catalog
/// replaced.
#[test]
fn what_was_read_is_not_a_failure() {
    let main = format!(
        r#"<xs:schema {XS} targetNamespace="urn:t">
  <xs:include schemaLocation="part.xsd"/>
  <xs:import namespace="urn:elsewhere"/>
  <xs:import namespace="http://www.w3.org/XML/1998/namespace" schemaLocation="no-such-xml.xsd"/>
</xs:schema>"#
    );
    let part =
        format!(r#"<xs:schema {XS} targetNamespace="urn:t"><xs:element name="p"/></xs:schema>"#);
    let d = dir("nothing-failed", &[("main.xsd", main), ("part.xsd", part)]);
    for version in versions() {
        let set = compile(version, &d.join("main.xsd")).expect("compiles");
        assert!(
            set.directive_failures().is_empty(),
            "{:?}",
            set.directive_failures()
        );
    }
}

/// A failure in an included document is reported against that document.
#[test]
fn a_failure_in_a_nested_document_names_that_document() {
    let main = format!(
        r#"<xs:schema {XS} targetNamespace="urn:t"><xs:include schemaLocation="sub/part.xsd"/></xs:schema>"#
    );
    let part = format!(
        r#"<xs:schema {XS} targetNamespace="urn:t"><xs:include schemaLocation="gone.xsd"/></xs:schema>"#
    );
    let d = dir("nested", &[("main.xsd", main)]);
    fs::create_dir_all(d.join("sub")).unwrap();
    fs::write(d.join("sub").join("part.xsd"), part).unwrap();
    let set = compile(XsdVersion::V1_0, &d.join("main.xsd")).expect("compiles");
    let failures = set.directive_failures();
    assert_eq!(failures.len(), 1, "{failures:?}");
    let at = failures[0].location().expect("located");
    assert!(at.base_uri.ends_with("part.xsd"), "{at}");
    assert!(
        failures[0]
            .resolved_location()
            .is_some_and(|r| Path::new(r).ends_with("sub/gone.xsd")),
        "{:?}",
        failures[0]
    );
}

/// A reference that fails because a document could not be read names it.
#[test]
fn an_unresolved_reference_names_the_documents_that_could_not_be_read() {
    let main = format!(
        r#"<xs:schema {XS} targetNamespace="urn:t" xmlns:t="urn:t">
  <xs:include schemaLocation="types.xsd"/>
  <xs:complexType name="T"><xs:complexContent><xs:extension base="t:Base"/></xs:complexContent></xs:complexType>
</xs:schema>"#
    );
    let d = dir("unresolved", &[("main.xsd", main)]);
    for version in versions() {
        let err = compile(version, &d.join("main.xsd")).expect_err("t:Base is missing");
        let text = err.to_string();
        assert!(text.contains("src-resolve"), "{text}");
        assert!(
            text.contains("types.xsd"),
            "names the unread document: {text}"
        );
    }
}

/// The one-document pipeline keeps the failures on the set too.
#[test]
fn the_pipeline_reports_failures_on_the_set() {
    let main = format!(
        r#"<xs:schema {XS} targetNamespace="urn:t"><xs:include schemaLocation="missing.xsd"/></xs:schema>"#
    );
    let d = dir("pipeline", &[("main.xsd", main.clone())]);
    let location = d.join("main.xsd").to_string_lossy().into_owned();
    let mut set = SchemaSet::new();
    load_and_process_schema(main.as_bytes(), &location, &mut set, None).expect("compiles");
    assert_eq!(
        set.directive_failures().len(),
        1,
        "{:?}",
        set.directive_failures()
    );
}

#[cfg(feature = "async")]
#[test]
fn the_async_entry_points_report_failures_too() {
    let main = format!(
        r#"<xs:schema {XS} targetNamespace="urn:t"><xs:include schemaLocation="missing.xsd"/></xs:schema>"#
    );
    let d = dir("async", &[("main.xsd", main.clone())]);
    let location = d.join("main.xsd").to_string_lossy().into_owned();
    let compiled = futures::executor::block_on(async {
        SchemaSetBuilder::new()
            .add_async("", &location)
            .await?
            .compile_async()
            .await
    })
    .expect("compiles");
    assert_eq!(compiled.directive_failures().len(), 1);
    let mut set = SchemaSet::new();
    futures::executor::block_on(xsd_schema::load_and_process_schema_async(
        main.as_bytes(),
        &location,
        &mut set,
        None,
    ))
    .expect("compiles");
    assert_eq!(set.directive_failures().len(), 1);
}

// ── Fix round 1 (F5, F7, F12, F15) ────────────────────────────────────────

/// F5: Structures §4.2.4 Redefinition Constraints and Semantics, clause 1:
/// "If there are any element information items among the [children] other
/// than <annotation> then the ·actual value· of the schemaLocation
/// [attribute] must successfully resolve." A failed `xs:redefine` with
/// children is an error, even when another document of the set supplies the
/// redefined component.
#[test]
fn a_failed_redefine_with_children_is_an_error() {
    let a = format!(r#"<xs:schema {XS}><xs:complexType name="T"/></xs:schema>"#);
    let main = format!(
        r#"<xs:schema {XS}>
  <xs:include schemaLocation="a.xsd"/>
  <xs:redefine schemaLocation="nope.xsd">
    <xs:complexType name="T"><xs:complexContent><xs:extension base="T">
      <xs:sequence><xs:element name="y"/></xs:sequence></xs:extension></xs:complexContent></xs:complexType>
  </xs:redefine>
  <xs:element name="r" type="T"/>
</xs:schema>"#
    );
    let d = dir(
        "redefine-failed",
        &[("main.xsd", main.clone()), ("a.xsd", a)],
    );
    for version in versions() {
        let err = compile(version, &d.join("main.xsd")).expect_err("clause 1");
        let text = err.to_string();
        assert!(
            text.contains("src-redefine") && text.contains("nope.xsd") && text.contains(":3:3"),
            "{version:?}: {text}"
        );
        let mut set = SchemaSet::with_version(version);
        let location = d.join("main.xsd").to_string_lossy().into_owned();
        let err = load_and_process_schema(main.as_bytes(), &location, &mut set, None)
            .expect_err("the pipeline too");
        assert!(err.to_string().contains("src-redefine"), "{err}");
    }
    // Without children (only an annotation) a failed redefine is not an error.
    let main = format!(
        r#"<xs:schema {XS}><xs:redefine schemaLocation="nope.xsd"><xs:annotation/></xs:redefine></xs:schema>"#
    );
    let d = dir("redefine-failed-empty", &[("main.xsd", main)]);
    let set = compile(XsdVersion::V1_0, &d.join("main.xsd")).expect("no children");
    assert_eq!(set.directive_failures().len(), 1);
}

/// F5 (control): an `xs:override` that fails to resolve is not an error —
/// its clause 1 only begins "If the ·actual value· of the schemaLocation
/// [attribute] successfully resolves …".
#[cfg(feature = "xsd11")]
#[test]
fn a_failed_override_is_not_an_error() {
    let main = format!(
        r#"<xs:schema {XS}><xs:override schemaLocation="nope.xsd"><xs:complexType name="T"/></xs:override><xs:element name="r"/></xs:schema>"#
    );
    let d = dir("override-failed", &[("main.xsd", main)]);
    let set = compile(XsdVersion::V1_1, &d.join("main.xsd")).expect("not an error");
    assert_eq!(set.directive_failures().len(), 1);
    assert_eq!(
        set.directive_failures()[0].kind(),
        CompositionEdgeKind::Override
    );
}

/// F7: a failed import location hint whose namespace another document of the
/// set supplies is recorded, and marked as supplied.
#[test]
fn a_hint_whose_namespace_is_supplied_is_marked() {
    let x =
        format!(r#"<xs:schema {XS} targetNamespace="urn:x"><xs:element name="e"/></xs:schema>"#);
    let main = format!(
        r#"<xs:schema {XS} targetNamespace="urn:t">
  <xs:import namespace="urn:x" schemaLocation="gone-1.xsd"/>
  <xs:import namespace="urn:x" schemaLocation="x.xsd"/>
  <xs:import namespace="urn:y" schemaLocation="gone-2.xsd"/>
  <xs:import namespace="urn:x" schemaLocation="gone-3.xsd"/>
</xs:schema>"#
    );
    let d = dir("supplied", &[("main.xsd", main), ("x.xsd", x)]);
    let set = compile(XsdVersion::V1_0, &d.join("main.xsd")).expect("compiles");
    let rows: Vec<(Option<&str>, bool)> = set
        .directive_failures()
        .iter()
        .map(|f| (f.schema_location(), f.namespace_supplied()))
        .collect();
    assert_eq!(
        rows,
        [
            (Some("gone-1.xsd"), true),
            (Some("gone-2.xsd"), false),
            (Some("gone-3.xsd"), true)
        ]
    );
}

/// F12: when a location hint is read but is not a schema document and the
/// catalog's location for its namespace cannot be read, the failure names
/// the catalog's location as what was read, and says the hint is not a
/// schema document.
#[test]
fn a_failed_catalog_fallback_is_reported_as_such() {
    let main = format!(
        r#"<xs:schema {XS} targetNamespace="urn:t"><xs:import namespace="urn:x" schemaLocation="notschema.xml"/></xs:schema>"#
    );
    let d = dir(
        "catalog-fallback",
        &[("main.xsd", main), ("notschema.xml", "<html/>".to_string())],
    );
    let mut resolver = xsd_schema::SchemaResolver::new();
    resolver.catalog_mut().add_xml_catalog();
    resolver.catalog_mut().add(
        "urn:x",
        d.join("missing-x.xsd").to_string_lossy().into_owned(),
    );
    let compiled = SchemaSetBuilder::with_resolver(resolver, XsdVersion::V1_0)
        .add("", &d.join("main.xsd").to_string_lossy())
        .and_then(|b| b.compile())
        .expect("a failed import is not an error");
    let failures = compiled.directive_failures();
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert!(
        failures[0]
            .resolved_location()
            .is_some_and(|r| r.ends_with("missing-x.xsd")),
        "{:?}",
        failures[0]
    );
    let text = failures[0].to_string();
    assert!(
        text.contains("notschema.xml")
            && text.contains("not a schema document")
            && text.contains("missing-x.xsd"),
        "{text}"
    );
}

/// F15: in XSD 1.0 a missing type of an element declaration is reported when
/// the element is validated; that error names the documents that could not
/// be read too.
#[test]
fn a_deferred_missing_type_names_the_unread_documents() {
    use xsd_schema::validation::{
        drive_quick_xml, CollectingValidationSink, SchemaValidator, ValidationFlags,
    };
    let main = format!(
        r#"<xs:schema {XS}><xs:include schemaLocation="types.xsd"/><xs:element name="r" type="T"/></xs:schema>"#
    );
    let d = dir("deferred", &[("main.xsd", main)]);
    for version in versions() {
        let set = match compile(version, &d.join("main.xsd")) {
            Ok(set) => set,
            Err(e) => {
                // XSD 1.1 reports it at compile time, with the note.
                assert!(e.to_string().contains("types.xsd"), "{version:?}: {e}");
                continue;
            }
        };
        let validator = SchemaValidator::new(&set, ValidationFlags::default());
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        {
            let mut run = validator.start_run(CollectingValidationSink {
                errors: &mut errors,
                warnings: &mut warnings,
            });
            let _ = drive_quick_xml(b"<r/>".as_slice(), &mut run, &set);
        }
        let text: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
        assert!(
            text.iter()
                .any(|t| t.contains("src-resolve") && t.contains("types.xsd")),
            "{version:?}: {text:?}"
        );
    }
}
