//! Relative schema locations that climb out of the directory they start in.
//!
//! A location such as `../xsd/main.xsd` — given to the builder, or resolved
//! from an include against a relative base URI — must keep its leading `..`:
//! lexical normalization may only cancel a `..` against a preceding normal
//! segment. Dropping it reads `xsd/main.xsd` below the current directory
//! instead.
//!
//! Every location here is spelled relative to the current directory and
//! starts with `..`, so the tests need neither absolute paths nor
//! `std::env::set_current_dir`.

use std::fs;
use std::path::{Path, PathBuf};

use xsd_schema::{load_schema, SchemaSet, SchemaSetBuilder};

const MAIN_XSD: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:include schemaLocation="../common/types.xsd"/>
  <xs:element name="root" type="Code"/>
</xs:schema>"#;

const TYPES_XSD: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:simpleType name="Code">
    <xs:restriction base="xs:string"/>
  </xs:simpleType>
</xs:schema>"#;

/// Scratch layout, removed on drop:
///
/// ```text
/// <dir>/xsd/main.xsd        includes ../common/types.xsd
/// <dir>/common/types.xsd
/// ```
struct Layout {
    dir: PathBuf,
}

impl Layout {
    fn new(name: &str) -> Self {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("relative_locations")
            .join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("xsd")).unwrap();
        fs::create_dir_all(dir.join("common")).unwrap();
        fs::write(dir.join("xsd").join("main.xsd"), MAIN_XSD).unwrap();
        fs::write(dir.join("common").join("types.xsd"), TYPES_XSD).unwrap();
        Layout { dir }
    }

    /// `<dir>/<path>` relative to the current directory, spelled through its
    /// parent so that it starts with `..`: `../<cwd name>/...` when `<dir>`
    /// lies inside the current directory.
    fn relative(&self, path: &str) -> String {
        let cwd = std::env::current_dir().unwrap();
        let start = cwd.parent().expect("current directory has a parent");
        let target = self.dir.join(path);
        let common = start
            .components()
            .zip(target.components())
            .take_while(|(a, b)| a == b)
            .count();
        assert!(
            common > 0,
            "{} is not reachable relatively",
            target.display()
        );

        let mut relative = PathBuf::from("..");
        for _ in start.components().skip(common) {
            relative.push("..");
        }
        for component in target.components().skip(common) {
            relative.push(component);
        }
        let relative = relative.to_str().unwrap().to_string();
        assert!(relative.starts_with(".."));
        relative
    }
}

impl Drop for Layout {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn add_location_with_leading_parent_dir() {
    let layout = Layout::new("add_location");
    let location = layout.relative("xsd/main.xsd");

    let compiled = SchemaSetBuilder::new()
        .add("", &location)
        .unwrap_or_else(|e| panic!("add({location}): {e}"))
        .compile()
        .expect("schema compiles");

    assert_eq!(compiled.stats.documents_loaded, 2);
}

#[test]
fn try_add_relative_against_relative_base() {
    let layout = Layout::new("try_add_relative");
    let base = layout.relative("probe/instance.xml");

    let mut builder = SchemaSetBuilder::new();
    let added = builder
        .try_add_relative("../xsd/main.xsd", &base)
        .unwrap_or_else(|e| panic!("try_add_relative(../xsd/main.xsd, {base}): {e}"));
    assert!(added);
    assert!(!builder.try_add_relative("../xsd/main.xsd", &base).unwrap());

    let compiled = builder.compile().expect("schema compiles");
    assert_eq!(compiled.stats.documents_loaded, 2);
}

#[test]
fn include_resolved_against_relative_base_uri() {
    let layout = Layout::new("include_relative_base");
    let base = layout.relative("xsd/main.xsd");

    let mut schema_set = SchemaSet::new();
    let stats = load_schema(MAIN_XSD.as_bytes(), &base, &mut schema_set)
        .unwrap_or_else(|e| panic!("load_schema with base {base}: {e}"));

    assert_eq!(stats.loaded_docs.len(), 1, "the include was not loaded");
}
