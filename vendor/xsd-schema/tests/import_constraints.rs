//! An `<xs:import>` of the enclosing schema document's own target namespace.
//!
//! Structures 1.1 §4.2.6.2 (Import Constraints and Semantics): "1 The
//! appropriate case among the following must be true: 1.1 If the namespace
//! [attribute] is present, then its ·actual value· does not match the ·actual
//! value· of the enclosing <schema>'s targetNamespace [attribute]. 1.2 If the
//! namespace [attribute] is not present, then the enclosing <schema> has a
//! targetNamespace [attribute]". XSD 1.0 §4.2.3 (src-import) has the same
//! clause 1. Every entry point reads directives with the same walk: the
//! builder, the one-document pipeline and their async forms.

use std::fs;
use std::path::{Path, PathBuf};

use xsd_schema::{load_and_process_schema, SchemaError, SchemaSet, SchemaSetBuilder, XsdVersion};

const XS: &str = r#"xmlns:xs="http://www.w3.org/2001/XMLSchema""#;

fn dir(name: &str, files: &[(&str, String)]) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("import_constraints")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    for (file, text) in files {
        fs::write(dir.join(file), text).unwrap();
    }
    dir
}

fn versions() -> Vec<XsdVersion> {
    if cfg!(feature = "xsd11") {
        vec![XsdVersion::V1_0, XsdVersion::V1_1]
    } else {
        vec![XsdVersion::V1_0]
    }
}

fn builder(version: XsdVersion, main: &Path) -> Result<(), SchemaError> {
    SchemaSetBuilder::with_version(version)
        .add("", &main.to_string_lossy())?
        .compile()
        .map(|_| ())
}

fn pipeline(version: XsdVersion, main: &Path) -> Result<(), SchemaError> {
    let text = fs::read_to_string(main).unwrap();
    let mut set = match version {
        XsdVersion::V1_0 => SchemaSet::new(),
        _ => SchemaSet::xsd11(),
    };
    load_and_process_schema(text.as_bytes(), &main.to_string_lossy(), &mut set, None).map(|_| ())
}

/// A case: its label, its files (the first is the main document) and
/// whether it is refused.
type Case = (&'static str, Vec<(&'static str, String)>, bool);

fn cases() -> Vec<Case> {
    vec![
        (
            "own namespace, no location (schZ010)",
            vec![(
                "main.xsd",
                format!(
                    r#"<xs:schema {XS} targetNamespace="urn:t"><xs:import namespace="urn:t"/><xs:element name="a"/></xs:schema>"#
                ),
            )],
            true,
        ),
        (
            "own namespace, a location of that namespace",
            vec![
                (
                    "main.xsd",
                    format!(
                        r#"<xs:schema {XS} targetNamespace="urn:t"><xs:import namespace="urn:t" schemaLocation="other.xsd"/><xs:element name="a"/></xs:schema>"#
                    ),
                ),
                (
                    "other.xsd",
                    format!(
                        r#"<xs:schema {XS} targetNamespace="urn:t"><xs:element name="b"/></xs:schema>"#
                    ),
                ),
            ],
            true,
        ),
        (
            "own namespace in an imported document",
            vec![
                (
                    "main.xsd",
                    format!(
                        r#"<xs:schema {XS} targetNamespace="urn:t"><xs:import namespace="urn:o" schemaLocation="o.xsd"/></xs:schema>"#
                    ),
                ),
                (
                    "o.xsd",
                    format!(
                        r#"<xs:schema {XS} targetNamespace="urn:o"><xs:import namespace="urn:o"/><xs:element name="b"/></xs:schema>"#
                    ),
                ),
            ],
            true,
        ),
        (
            "another namespace",
            vec![
                (
                    "main.xsd",
                    format!(
                        r#"<xs:schema {XS} targetNamespace="urn:t"><xs:import namespace="urn:o" schemaLocation="o.xsd"/></xs:schema>"#
                    ),
                ),
                (
                    "o.xsd",
                    format!(
                        r#"<xs:schema {XS} targetNamespace="urn:o"><xs:element name="b"/></xs:schema>"#
                    ),
                ),
            ],
            false,
        ),
        (
            "no namespace attribute in a document with a target namespace",
            vec![(
                "main.xsd",
                format!(
                    r#"<xs:schema {XS} targetNamespace="urn:t"><xs:import/><xs:element name="a"/></xs:schema>"#
                ),
            )],
            false,
        ),
        (
            "no namespace attribute in a no-namespace document (clause 1.2)",
            vec![(
                "main.xsd",
                format!(r#"<xs:schema {XS}><xs:import/><xs:element name="a"/></xs:schema>"#),
            )],
            true,
        ),
    ]
}

#[test]
fn an_import_of_the_enclosing_target_namespace_is_an_error() {
    let mut wrong = Vec::new();
    for (i, (label, files, refused)) in cases().into_iter().enumerate() {
        let d = dir(&format!("case{i}"), &files);
        let main = d.join("main.xsd");
        for version in versions() {
            for (entry, result) in [
                ("builder", builder(version, &main)),
                ("pipeline", pipeline(version, &main)),
            ] {
                match (result, refused) {
                    (Ok(()), false) => {}
                    (Ok(()), true) => wrong.push(format!("{version:?} {entry} {label}: compiles")),
                    (Err(e), false) => {
                        wrong.push(format!("{version:?} {entry} {label}: refused: {e}"))
                    }
                    (Err(e), true) => {
                        if !e.to_string().contains("src-import") {
                            wrong.push(format!("{version:?} {entry} {label}: not src-import: {e}"));
                        }
                    }
                }
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "{} wrong:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

/// The async entry points read directives with the same rules.
#[cfg(all(feature = "async", feature = "xsd11"))]
#[test]
fn the_async_entry_points_refuse_it_too() {
    let files = vec![(
        "main.xsd",
        format!(
            r#"<xs:schema {XS} targetNamespace="urn:t"><xs:import namespace="urn:t"/><xs:element name="a"/></xs:schema>"#
        ),
    )];
    let d = dir("async", &files);
    let location = d.join("main.xsd").to_string_lossy().into_owned();
    let compiled = futures::executor::block_on(async {
        SchemaSetBuilder::with_version(XsdVersion::V1_1)
            .add_async("", &location)
            .await?
            .compile_async()
            .await
    });
    let Err(error) = compiled else {
        panic!("refused");
    };
    assert!(error.to_string().contains("src-import"), "{error}");
    let mut set = SchemaSet::xsd11();
    let result = futures::executor::block_on(xsd_schema::load_and_process_schema_async(
        files[0].1.as_bytes(),
        &location,
        &mut set,
        None,
    ));
    let Err(error) = result else {
        panic!("refused");
    };
    assert!(error.to_string().contains("src-import"), "{error}");
}
