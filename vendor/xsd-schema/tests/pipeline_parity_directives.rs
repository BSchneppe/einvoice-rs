//! Every schema-loading entry point resolves `xs:include` / `xs:import` with
//! the same rules (C87, directives).
//!
//! The rules of Structures §4.2.3 (Inclusion Constraints and Semantics) and
//! §4.2.6.2 (Import Constraints and Semantics) are checked while directives
//! are resolved. The synchronous and the asynchronous resolvers must apply
//! them alike, and both must terminate on a cycle of includes (§4.2.3:
//! "cycles of <include> elements are legal, and processors should guard
//! against infinite loops").
//!
//! The documents are files in a scratch directory, so that the one-document
//! pipeline (which reads directives through the default file loader) sees
//! the same inputs as the builder.

use std::fs;
use std::path::{Path, PathBuf};

use xsd_schema::{load_and_process_schema, SchemaError, SchemaSet, SchemaSetBuilder, XsdVersion};

fn dir(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("pipeline_parity_directives")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    for (file, text) in files {
        fs::write(dir.join(file), text).unwrap();
    }
    dir
}

fn verdict<T>(result: Result<T, SchemaError>) -> Option<String> {
    match result {
        Ok(_) => None,
        Err(SchemaError::StructuralError { constraint, .. }) => Some(constraint.to_string()),
        Err(e) => Some(format!("{e}")),
    }
}

fn verdicts(version: XsdVersion, main: &Path) -> Vec<(&'static str, Option<String>)> {
    let location = main.to_string_lossy().into_owned();
    let text = fs::read_to_string(main).unwrap();
    #[allow(unused_mut)]
    let mut all = vec![
        (
            "SchemaSetBuilder::compile",
            verdict(
                SchemaSetBuilder::with_version(version)
                    .add("", &location)
                    .and_then(|b| b.compile()),
            ),
        ),
        ("load_and_process_schema", {
            let mut set = SchemaSet::with_version(version);
            verdict(load_and_process_schema(
                text.as_bytes(),
                &location,
                &mut set,
                None,
            ))
        }),
    ];
    #[cfg(feature = "async")]
    {
        all.push(("load_and_process_schema_async", {
            let mut set = SchemaSet::with_version(version);
            verdict(futures::executor::block_on(
                xsd_schema::load_and_process_schema_async(
                    text.as_bytes(),
                    &location,
                    &mut set,
                    None,
                ),
            ))
        }));
        all.push((
            "SchemaSetBuilder::compile_async",
            verdict(futures::executor::block_on(async {
                SchemaSetBuilder::with_version(version)
                    .add_async("", &location)
                    .await?
                    .compile_async()
                    .await
            })),
        ));
    }
    all
}

fn assert_all(name: &str, version: XsdVersion, main: &Path, expected: Option<&str>) {
    let problems: Vec<String> = verdicts(version, main)
        .into_iter()
        .filter(|(_, v)| v.as_deref() != expected)
        .map(|(entry, v)| format!("  {entry}: expected {expected:?}, got {v:?}"))
        .collect();
    assert!(
        problems.is_empty(),
        "{name} ({version:?}):\n{}",
        problems.join("\n")
    );
}

const XS: &str = r#"xmlns:xs="http://www.w3.org/2001/XMLSchema""#;

#[test]
fn a_cycle_of_includes_terminates_everywhere() {
    let a = format!(
        r#"<xs:schema {XS} targetNamespace="urn:t"><xs:include schemaLocation="b.xsd"/><xs:element name="a"/></xs:schema>"#
    );
    let b = format!(
        r#"<xs:schema {XS} targetNamespace="urn:t"><xs:include schemaLocation="a.xsd"/><xs:element name="b"/></xs:schema>"#
    );
    let d = dir("include-cycle", &[("a.xsd", &a), ("b.xsd", &b)]);
    assert_all("include cycle", XsdVersion::V1_0, &d.join("a.xsd"), None);
}

/// §4.2.6.2 clause 1.1 (XSD 1.0): an import of the enclosing schema's own
/// target namespace.
#[test]
fn an_import_of_the_own_namespace_is_refused_everywhere_in_1_0() {
    let a = format!(
        r#"<xs:schema {XS} targetNamespace="urn:t"><xs:import namespace="urn:t" schemaLocation="b.xsd"/></xs:schema>"#
    );
    let b =
        format!(r#"<xs:schema {XS} targetNamespace="urn:t"><xs:element name="b"/></xs:schema>"#);
    let d = dir("own-namespace", &[("a.xsd", &a), ("b.xsd", &b)]);
    assert_all(
        "own-namespace import",
        XsdVersion::V1_0,
        &d.join("a.xsd"),
        Some("src-import"),
    );
}

/// §4.2.6.2 clause 3.1: the imported document's target namespace must be
/// the `namespace` attribute.
#[test]
fn an_import_of_another_namespace_is_refused_everywhere() {
    let a = format!(
        r#"<xs:schema {XS} targetNamespace="urn:t"><xs:import namespace="urn:x" schemaLocation="b.xsd"/></xs:schema>"#
    );
    let b =
        format!(r#"<xs:schema {XS} targetNamespace="urn:y"><xs:element name="b"/></xs:schema>"#);
    let d = dir("namespace-mismatch", &[("a.xsd", &a), ("b.xsd", &b)]);
    assert_all(
        "import namespace mismatch",
        XsdVersion::V1_0,
        &d.join("a.xsd"),
        Some("src-import"),
    );
}

/// §4.2.3 clause 2: a schema without a target namespace includes one with a
/// target namespace.
#[test]
fn an_include_into_no_namespace_is_refused_everywhere() {
    let a = format!(r#"<xs:schema {XS}><xs:include schemaLocation="b.xsd"/></xs:schema>"#);
    let b =
        format!(r#"<xs:schema {XS} targetNamespace="urn:y"><xs:element name="b"/></xs:schema>"#);
    let d = dir("include-tns", &[("a.xsd", &a), ("b.xsd", &b)]);
    assert_all(
        "include into no namespace",
        XsdVersion::V1_0,
        &d.join("a.xsd"),
        Some("src-include"),
    );
}
