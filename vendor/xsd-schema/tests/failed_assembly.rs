//! A schema document that fails to assemble must leave no trace.
//!
//! Parsing a document allocates its top-level components, binds their names
//! in the namespace tables and stores its source map before assembly can
//! fail (for instance on a duplicate declaration, `sch-props-correct`).
//! `SchemaSet::documents` and `SchemaSet::source_maps` are parallel tables
//! indexed by `DocumentId`; a source map left behind by a failed document
//! handed the next document an id one past the end of `documents`, and
//! recording that document's composition edge panicked
//! (`index out of bounds`) instead of reporting the duplicate.
//!
//! These tests check both halves through the public builder API: the error
//! is reported, and the schema set is exactly as it was before the failed
//! document (no component, no name binding, no id shift).

use std::collections::HashMap;

use xsd_schema::{SchemaLoader, SchemaResult, SchemaSet, SchemaSetBuilder};

/// Serves schema documents from memory, keyed by absolute path.
#[derive(Debug)]
struct MemoryLoader(HashMap<String, String>);

impl MemoryLoader {
    fn new(files: &[(&str, &str)]) -> Box<Self> {
        Box::new(MemoryLoader(
            files
                .iter()
                .map(|(path, text)| (path.to_string(), text.to_string()))
                .collect(),
        ))
    }
}

impl SchemaLoader for MemoryLoader {
    fn load(&self, location: &str) -> SchemaResult<String> {
        self.0
            .get(location)
            .cloned()
            .ok_or_else(|| xsd_schema::SchemaError::resolution(format!("no file {location}")))
    }

    fn can_load(&self, location: &str) -> bool {
        self.0.contains_key(location)
    }
}

fn builder(loader: Box<MemoryLoader>, xsd11: bool) -> SchemaSetBuilder {
    if xsd11 {
        SchemaSetBuilder::xsd11_with_loader(loader)
    } else {
        SchemaSetBuilder::with_loader(loader)
    }
}

/// `main.xsd` includes `dup.xsd`, which declares `e` again, and then imports
/// `z.xsd`: the import is loaded after the failed include.
const MAIN: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:x">
  <xs:include schemaLocation="dup.xsd"/>
  <xs:import namespace="urn:z" schemaLocation="z.xsd"/>
  <xs:element name="e"/>
</xs:schema>"#;
const DUP: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:x">
  <xs:element name="e"/>
</xs:schema>"#;
const Z: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:z">
  <xs:element name="z"/>
</xs:schema>"#;

#[test]
fn duplicate_in_included_document_is_reported_not_a_panic() {
    for xsd11 in [false, true] {
        let loader = MemoryLoader::new(&[
            ("/mem/main.xsd", MAIN),
            ("/mem/dup.xsd", DUP),
            ("/mem/z.xsd", Z),
        ]);
        let Err(err) = builder(loader, xsd11)
            .add("", "/mem/main.xsd")
            .expect("main.xsd parses")
            .compile()
        else {
            panic!("xsd11={xsd11}: duplicate declaration of 'e' must be rejected");
        };
        let message = err.to_string();
        assert!(
            message.contains("Duplicate element declaration 'e'")
                && message.contains("sch-props-correct")
                && message.contains("dup.xsd"),
            "xsd11={xsd11}: unexpected error: {message}"
        );
    }
}

/// Declares one component of every kind and an identity constraint, then
/// collides with `a.xsd` on the element `e`, after all of them were
/// registered.
const BAD: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:x="urn:x" targetNamespace="urn:x">
  <xs:simpleType name="S"><xs:restriction base="xs:string"/></xs:simpleType>
  <xs:complexType name="G"><xs:sequence><xs:element name="c"/></xs:sequence></xs:complexType>
  <xs:group name="Gr"><xs:sequence><xs:element name="c"/></xs:sequence></xs:group>
  <xs:attributeGroup name="AG"><xs:attribute name="b"/></xs:attributeGroup>
  <xs:attribute name="at"/>
  <xs:notation name="n" public="p"/>
  <xs:element name="g" type="x:G">
    <xs:key name="k"><xs:selector xpath="x:c"/><xs:field xpath="."/></xs:key>
  </xs:element>
  <xs:element name="e"/>
</xs:schema>"#;
const A: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:x">
  <xs:element name="e" type="xs:int"/>
</xs:schema>"#;
/// Declares the very names `bad.xsd` failed with, and includes `d.xsd`.
const C: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:x="urn:x" targetNamespace="urn:x">
  <xs:include schemaLocation="d.xsd"/>
  <xs:simpleType name="S"><xs:restriction base="xs:token"/></xs:simpleType>
  <xs:complexType name="G"><xs:sequence><xs:element name="c" type="x:D"/></xs:sequence></xs:complexType>
  <xs:group name="Gr"><xs:sequence><xs:element name="c"/></xs:sequence></xs:group>
  <xs:attributeGroup name="AG"><xs:attribute name="b"/></xs:attributeGroup>
  <xs:attribute name="at"/>
  <xs:notation name="n" public="p"/>
  <xs:element name="g" type="x:G">
    <xs:key name="k"><xs:selector xpath="x:c"/><xs:field xpath="."/></xs:key>
  </xs:element>
</xs:schema>"#;
const D: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:x">
  <xs:simpleType name="D"><xs:restriction base="xs:string"/></xs:simpleType>
</xs:schema>"#;

#[test]
fn failed_document_leaves_no_component_behind() {
    for xsd11 in [false, true] {
        let loader = MemoryLoader::new(&[
            ("/mem/a.xsd", A),
            ("/mem/bad.xsd", BAD),
            ("/mem/c.xsd", C),
            ("/mem/d.xsd", D),
        ]);
        let mut builder = builder(loader, xsd11);
        assert!(builder.try_add("/mem/a.xsd").expect("a.xsd parses"));
        let err = builder
            .try_add("/mem/bad.xsd")
            .expect_err("bad.xsd redeclares 'e'");
        assert!(
            err.to_string()
                .contains("Duplicate element declaration 'e'"),
            "xsd11={xsd11}: unexpected error: {err}"
        );
        assert!(!builder.is_loaded("/mem/bad.xsd"));

        // c.xsd declares every name bad.xsd declared: none of them may still
        // be bound, and c.xsd's include must be resolved under c.xsd's own
        // document id.
        assert!(builder.try_add("/mem/c.xsd").expect("c.xsd parses"));
        let compiled = builder
            .compile()
            .unwrap_or_else(|e| panic!("xsd11={xsd11}: a.xsd + c.xsd must compile: {e}"));
        let set = compiled.schema_set();
        assert_eq!(set.documents.len(), 3, "a.xsd, c.xsd, d.xsd");
        assert_eq!(set.source_maps.len(), set.documents.len());
        for (id, doc) in set.documents.iter().enumerate() {
            assert_eq!(doc.id as usize, id, "document id is its index");
        }

        // `e` is a.xsd's declaration again, not bad.xsd's.
        let ns = set.name_table.get("urn:x");
        let e = set
            .lookup_element(ns, set.name_table.get("e").unwrap())
            .expect("e is declared");
        let source = set.arenas.elements[e].source.as_ref().unwrap();
        assert!(
            set.documents[source.doc_id as usize]
                .base_uri
                .ends_with("a.xsd"),
            "xsd11={xsd11}: 'e' must come from a.xsd"
        );
        // Every component of the set belongs to a.xsd, c.xsd or d.xsd.
        let in_set = |doc_id: u32| (doc_id as usize) < set.documents.len();
        assert!(set
            .arenas
            .elements
            .values()
            .all(|d| d.source.as_ref().is_none_or(|s| in_set(s.doc_id))));
        assert_eq!(
            set.arenas
                .elements
                .values()
                .filter(|d| d.name == set.name_table.get("g"))
                .count(),
            1,
            "xsd11={xsd11}: only c.xsd's 'g' remains"
        );
        assert_eq!(set.arenas.notations.len(), 1);
        assert_eq!(set.arenas.identity_constraints.len(), 1);
    }
}

#[test]
fn failed_document_in_a_fresh_namespace_leaves_no_namespace_table() {
    let mut set = SchemaSet::new();
    xsd_schema::parser::parse::parse_schema(A.as_bytes(), "/mem/a.xsd", &mut set)
        .expect("a.xsd parses");
    let tables = set.namespaces.len();
    let bad = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:fresh">
  <xs:element name="f"/>
  <xs:element name="f"/>
</xs:schema>"#;
    xsd_schema::parser::parse::parse_schema(bad.as_bytes(), "/mem/bad.xsd", &mut set)
        .expect_err("duplicate 'f'");
    assert_eq!(set.namespaces.len(), tables);
    assert_eq!(set.documents.len(), 1);
    assert_eq!(set.source_maps.len(), 1);
    assert_eq!(set.arenas.elements.len(), 1);
}
