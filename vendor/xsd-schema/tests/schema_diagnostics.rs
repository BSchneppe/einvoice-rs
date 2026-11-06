//! Schema-document errors carry a location and the right constraint (C15d).
//!
//! * An element the schema for schemas does not allow where it stands (an
//!   `xs:import` after a declaration) is located at that element, with error
//!   recovery on (the default) and off.
//! * An attribute the schema for schemas does not allow is located and named
//!   as a schema-for-schemas violation (`s4s-att-not-allowed`), not as
//!   `ct-props-correct`, which is a constraint on complex type definitions.
//! * An empty or rootless document is an XML error that names the document.

use std::fs;
use std::path::{Path, PathBuf};

use xsd_schema::{ResolverConfig, SchemaError, SchemaSetBuilder};

const XS: &str = r#"xmlns:xs="http://www.w3.org/2001/XMLSchema""#;

fn builder(recovery: bool) -> SchemaSetBuilder {
    let mut config = ResolverConfig::default();
    config.parser_config.error_recovery = recovery;
    SchemaSetBuilder::with_config(config)
}

fn first_error(recovery: bool, xsd: &str) -> SchemaError {
    builder(recovery)
        .add_source(xsd, "file:///diag/main.xsd")
        .and_then(|b| b.compile())
        .err()
        .expect("the schema is refused")
}

fn located(error: &SchemaError) -> Option<(usize, usize)> {
    match error {
        SchemaError::StructuralError { location, .. } | SchemaError::XmlError { location, .. } => {
            location.as_ref().map(|l| (l.line, l.column))
        }
        _ => None,
    }
}

#[test]
fn an_unexpected_element_is_located() {
    let xsd = format!(
        "<xs:schema {XS} targetNamespace=\"urn:t\">\n  <xs:element name=\"a\"/>\n  <xs:import namespace=\"urn:z\"/>\n</xs:schema>"
    );
    for recovery in [true, false] {
        let error = first_error(recovery, &xsd);
        let text = error.to_string();
        assert!(text.contains("Unexpected element 'import'"), "{text}");
        assert_eq!(located(&error), Some((3, 3)), "recovery={recovery}: {text}");
    }
}

#[test]
fn an_attribute_the_schema_for_schemas_does_not_allow_is_located_and_named() {
    let xsd = format!(
        "<xs:schema {XS} targetNamespace=\"urn:t\">\n  <xs:import namespace=\"urn:z\" location=\"z.xsd\"/>\n</xs:schema>"
    );
    for recovery in [true, false] {
        let error = first_error(recovery, &xsd);
        let text = error.to_string();
        assert!(
            text.contains("Attribute 'location' is not allowed here"),
            "{text}"
        );
        match &error {
            SchemaError::StructuralError { constraint, .. } => {
                assert_eq!(*constraint, "s4s-att-not-allowed", "{text}")
            }
            other => panic!("not a structural error: {other:?}"),
        }
        assert_eq!(located(&error), Some((2, 3)), "recovery={recovery}: {text}");
    }
}

fn dir(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("schema_diagnostics")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    for (file, text) in files {
        fs::write(dir.join(file), text).unwrap();
    }
    dir
}

/// XML 1.0 §2.1 [1] `document ::= prolog element Misc*`: a document without
/// a root element is not well-formed, whether it is added or reached
/// through a directive.
#[test]
fn an_empty_or_rootless_document_is_an_xml_error_naming_it() {
    let include = format!(
        r#"<xs:schema {XS} targetNamespace="urn:t"><xs:include schemaLocation="empty.xsd"/></xs:schema>"#
    );
    let import = format!(
        r#"<xs:schema {XS} targetNamespace="urn:t"><xs:import namespace="urn:e" schemaLocation="rootless.xsd"/></xs:schema>"#
    );
    let d = dir(
        "empty",
        &[
            ("empty.xsd", ""),
            (
                "rootless.xsd",
                "<?xml version=\"1.0\"?>\n<!-- no root -->\n",
            ),
            ("include.xsd", &include),
            ("import.xsd", &import),
        ],
    );
    for (main, named) in [
        ("empty.xsd", "empty.xsd"),
        ("rootless.xsd", "rootless.xsd"),
        ("include.xsd", "empty.xsd"),
        ("import.xsd", "rootless.xsd"),
    ] {
        let error = SchemaSetBuilder::new()
            .add("", &d.join(main).to_string_lossy())
            .and_then(|b| b.compile())
            .err()
            .unwrap_or_else(|| panic!("{main} is refused"));
        let text = error.to_string();
        assert!(
            matches!(error, SchemaError::XmlError { .. }),
            "{main}: {text}"
        );
        assert!(
            text.contains(named) && text.contains("no root element"),
            "{main}: {text}"
        );
    }
}

// ── Fix round 1 (F14) ─────────────────────────────────────────────────────

fn constraint(error: &SchemaError) -> &str {
    match error {
        SchemaError::StructuralError { constraint, .. } => constraint,
        _ => "",
    }
}

/// A missing `base` on an extension or restriction of simple content is a
/// schema-for-schemas violation, located: an extension at the extension,
/// which must have one; a restriction at the `xs:simpleContent`, which is
/// where the parser finds that neither a base nor an inline type was given.
#[test]
fn a_missing_base_is_located() {
    for (derivation, at) in [("extension", (3, 3)), ("restriction", (2, 26))] {
        let xsd = format!(
            "<xs:schema {XS}>\n<xs:complexType name=\"T\"><xs:simpleContent>\n  <xs:{derivation}/>\n</xs:simpleContent></xs:complexType>\n</xs:schema>"
        );
        for recovery in [true, false] {
            let error = first_error(recovery, &xsd);
            assert_eq!(
                constraint(&error),
                "s4s-att-must-appear",
                "{derivation}: {error}"
            );
            assert_eq!(
                located(&error),
                Some(at),
                "{derivation} recovery={recovery}: {error}"
            );
        }
    }
}

/// An element the schema for schemas does not allow is named as such.
#[test]
fn unexpected_content_is_a_schema_for_schemas_violation() {
    let xsd = format!("<xs:schema {XS}>\n<xs:element name=\"a\"/>\n<xs:import namespace=\"urn:z\"/>\n</xs:schema>");
    let error = first_error(false, &xsd);
    assert_eq!(constraint(&error), "s4s-elt-invalid-content", "{error}");
}

/// A document reached through a directive whose root is not `xs:schema`, or
/// whose target namespace does not fit the directive, is reported with a
/// location: the document's root, or the directive.
#[test]
fn directive_target_errors_are_located() {
    let include_bad_root =
        format!(r#"<xs:schema {XS}><xs:include schemaLocation="notschema.xml"/></xs:schema>"#);
    let include_tns = format!(
        "<xs:schema {XS} targetNamespace=\"urn:t\">\n  <xs:include schemaLocation=\"other.xsd\"/>\n</xs:schema>"
    );
    let import_tns = format!(
        "<xs:schema {XS} targetNamespace=\"urn:t\">\n  <xs:import namespace=\"urn:x\" schemaLocation=\"other.xsd\"/>\n</xs:schema>"
    );
    let other =
        format!(r#"<xs:schema {XS} targetNamespace="urn:o"><xs:element name="o"/></xs:schema>"#);
    let d = dir(
        "directive-targets",
        &[
            ("notschema.xml", "<notschema/>"),
            ("other.xsd", &other),
            ("bad-root.xsd", &include_bad_root),
            ("include-tns.xsd", &include_tns),
            ("import-tns.xsd", &import_tns),
        ],
    );
    for (main, file, at) in [
        ("bad-root.xsd", "notschema.xml", (1, 1)),
        ("include-tns.xsd", "include-tns.xsd", (2, 3)),
        ("import-tns.xsd", "import-tns.xsd", (2, 3)),
    ] {
        let error = SchemaSetBuilder::new()
            .add("", &d.join(main).to_string_lossy())
            .and_then(|b| b.compile())
            .err()
            .unwrap_or_else(|| panic!("{main} is refused"));
        let text = error.to_string();
        assert!(text.contains(file), "{main}: {text}");
        assert_eq!(located(&error), Some(at), "{main}: {text}");
    }
}
