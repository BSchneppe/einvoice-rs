//! Schema locations are resolved as RFC 3986 URI references, and one
//! document has one identity (C15a).
//!
//! RFC 3986 §5.2.2 (Transform References) resolves a reference against a
//! base URI component by component — a network-path reference keeps only the
//! base's scheme, an absolute-path reference its scheme and authority, an
//! empty path the base's path (and query, unless the reference has its own),
//! a relative path is merged with "all but the last segment of the base URI's
//! path" (§5.2.3) — and every resulting path goes through
//! `remove_dot_segments` (§5.2.4). §6.2.2 lists the syntax-based
//! normalizations under which two spellings denote the same resource: the
//! scheme and host are case-insensitive, percent-encoded unreserved
//! characters are equivalent to the characters, dot segments are removed;
//! §6.2.3 adds that an empty path with an authority is "/" and a default port
//! may be dropped.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use xsd_schema::{
    ResolverConfig, SchemaError, SchemaLoader, SchemaResolver, SchemaResult, SchemaSetBuilder,
};

/// The base of RFC 3986 §5.4.
const BASE: &str = "http://a/b/c/d;p?q";

fn resolve(reference: &str, base: &str) -> String {
    SchemaResolver::new()
        .resolve_location(reference, base)
        .unwrap_or_else(|e| panic!("{reference}: {e}"))
}

/// RFC 3986 §5.4.1 Normal Examples (the fragment kept, as §5.2.2 does).
#[test]
fn rfc_3986_normal_examples() {
    for (reference, target) in [
        ("g:h", "g:h"),
        ("g", "http://a/b/c/g"),
        ("./g", "http://a/b/c/g"),
        ("g/", "http://a/b/c/g/"),
        ("/g", "http://a/g"),
        ("//g", "http://g/"),
        ("?y", "http://a/b/c/d;p?y"),
        ("g?y", "http://a/b/c/g?y"),
        ("#s", "http://a/b/c/d;p?q#s"),
        ("g#s", "http://a/b/c/g#s"),
        ("g?y#s", "http://a/b/c/g?y#s"),
        (";x", "http://a/b/c/;x"),
        ("g;x", "http://a/b/c/g;x"),
        ("g;x?y#s", "http://a/b/c/g;x?y#s"),
        ("", "http://a/b/c/d;p?q"),
        (".", "http://a/b/c/"),
        ("./", "http://a/b/c/"),
        ("..", "http://a/b/"),
        ("../", "http://a/b/"),
        ("../g", "http://a/b/g"),
        ("../..", "http://a/"),
        ("../../", "http://a/"),
        ("../../g", "http://a/g"),
    ] {
        assert_eq!(resolve(reference, BASE), target, "{reference:?}");
    }
}

/// RFC 3986 §5.4.2 Abnormal Examples (strict parser).
#[test]
fn rfc_3986_abnormal_examples() {
    for (reference, target) in [
        ("../../../g", "http://a/g"),
        ("../../../../g", "http://a/g"),
        ("/./g", "http://a/g"),
        ("/../g", "http://a/g"),
        ("g.", "http://a/b/c/g."),
        (".g", "http://a/b/c/.g"),
        ("g..", "http://a/b/c/g.."),
        ("..g", "http://a/b/c/..g"),
        ("./../g", "http://a/b/g"),
        ("./g/.", "http://a/b/c/g/"),
        ("g/./h", "http://a/b/c/g/h"),
        ("g/../h", "http://a/b/c/h"),
        ("g;x=1/./y", "http://a/b/c/g;x=1/y"),
        ("g;x=1/../y", "http://a/b/c/y"),
        ("g?y/./x", "http://a/b/c/g?y/./x"),
        ("g?y/../x", "http://a/b/c/g?y/../x"),
        ("g#s/./x", "http://a/b/c/g#s/./x"),
        ("g#s/../x", "http://a/b/c/g#s/../x"),
        ("http:g", "http:g"),
    ] {
        assert_eq!(resolve(reference, BASE), target, "{reference:?}");
    }
}

/// An authority without a path (§5.2.3: "/" before the reference's path),
/// absolute locations, and the §6.2.2 / §6.2.3 normalizations.
#[test]
fn authority_without_a_path_and_normalized_spellings() {
    assert_eq!(
        resolve("a.xsd", "http://example.com"),
        "http://example.com/a.xsd"
    );
    assert_eq!(
        resolve("a.xsd", "https://example.com?x"),
        "https://example.com/a.xsd"
    );
    for spelling in [
        "http://example.com/s/x/../a.xsd",
        "HTTP://Example.COM/s/./a.xsd",
        "http://example.com:80/s/a.xsd",
        "http://example.com/%73/a.xsd",
    ] {
        assert_eq!(
            resolve(spelling, ""),
            "http://example.com/s/a.xsd",
            "{spelling}"
        );
    }
    assert_eq!(resolve("https://h:443/a.xsd", ""), "https://h/a.xsd");
    assert_eq!(resolve("http://h:8080/a.xsd", ""), "http://h:8080/a.xsd");
    // A percent-encoded reserved character stays encoded (hex in upper case).
    assert_eq!(resolve("http://h/a%2fb.xsd", ""), "http://h/a%2Fb.xsd");
    assert_eq!(
        resolve("embedded://xml.xsd", "http://h/s/a.xsd"),
        "embedded://xml.xsd"
    );
}

/// `file:` URIs are resolved as URIs, and their spellings of a local file
/// (`file:/p`, `file:///p`, `file://localhost/p`) are one.
#[test]
fn file_uris() {
    assert_eq!(
        resolve("sub/x.xsd", "file:///dir/main.xsd"),
        "file:///dir/sub/x.xsd"
    );
    assert_eq!(
        resolve("../x.xsd", "file:///dir/s/main.xsd"),
        "file:///dir/x.xsd"
    );
    assert_eq!(resolve("/x.xsd", "file:///dir/main.xsd"), "file:///x.xsd");
    for spelling in [
        "file:/dir/x.xsd",
        "file:///dir/./x.xsd",
        "file://localhost/dir/x.xsd",
    ] {
        assert_eq!(resolve(spelling, ""), "file:///dir/x.xsd", "{spelling}");
    }
}

/// Serves documents by exact location and records what was asked for.
#[derive(Debug, Clone)]
struct Web {
    docs: Arc<HashMap<String, String>>,
    asked: Arc<Mutex<Vec<String>>>,
}

impl Web {
    fn new(docs: &[(&str, String)]) -> Self {
        Web {
            docs: Arc::new(
                docs.iter()
                    .map(|(l, t)| (l.to_string(), t.clone()))
                    .collect(),
            ),
            asked: Arc::default(),
        }
    }
}

impl SchemaLoader for Web {
    fn load(&self, location: &str) -> SchemaResult<String> {
        self.asked.lock().unwrap().push(location.to_string());
        self.docs
            .get(location)
            .cloned()
            .ok_or_else(|| SchemaError::resolution(format!("404 {location}")))
    }
    fn can_load(&self, _location: &str) -> bool {
        true
    }
}

const XS: &str = r#"xmlns:xs="http://www.w3.org/2001/XMLSchema""#;

fn builder(web: &Web) -> SchemaSetBuilder {
    let config = ResolverConfig {
        allow_network: true,
        ..Default::default()
    };
    let mut resolver = SchemaResolver::with_config_and_loader(config, Box::new(web.clone()));
    resolver.catalog_mut().add_xml_catalog();
    SchemaSetBuilder::with_resolver(resolver, xsd_schema::XsdVersion::V1_0)
}

/// One URL under two spellings is one document: loaded once, no duplicate
/// declarations.
#[test]
fn one_document_under_two_spellings_is_loaded_once() {
    let main = format!(
        r#"<xs:schema {XS} targetNamespace="urn:t">
             <xs:include schemaLocation="types.xsd"/>
             <xs:include schemaLocation="sub/../types.xsd"/>
             <xs:include schemaLocation="http://EXAMPLE.com:80/s/./types.xsd"/>
           </xs:schema>"#
    );
    let types = format!(
        r#"<xs:schema {XS} targetNamespace="urn:t"><xs:simpleType name="T"><xs:restriction base="xs:string"/></xs:simpleType></xs:schema>"#
    );
    let web = Web::new(&[
        ("http://example.com/s/main.xsd", main),
        ("http://example.com/s/types.xsd", types),
    ]);
    let compiled = builder(&web)
        .add("", "http://example.com/s/main.xsd")
        .and_then(|b| b.compile())
        .expect("no duplicate type T");
    assert_eq!(compiled.stats.documents_loaded, 2);
    let asked = web.asked.lock().unwrap().clone();
    assert_eq!(
        asked,
        [
            "http://example.com/s/main.xsd",
            "http://example.com/s/types.xsd"
        ],
        "each document read once, under its normalized URL"
    );
}

/// An absolute-path and a network-path reference keep the base's authority
/// and scheme; a query is part of the location.
#[test]
fn absolute_and_network_path_references_against_a_url() {
    let main = format!(
        r#"<xs:schema {XS} targetNamespace="urn:t">
             <xs:import namespace="urn:a" schemaLocation="/root/a.xsd"/>
             <xs:import namespace="urn:b" schemaLocation="//other.example/b.xsd"/>
             <xs:import namespace="urn:c" schemaLocation="?xsd=c"/>
           </xs:schema>"#
    );
    let doc = |ns: &str| {
        format!(r#"<xs:schema {XS} targetNamespace="{ns}"><xs:element name="e"/></xs:schema>"#)
    };
    let web = Web::new(&[
        ("http://example.com/svc?wsdl", main),
        ("http://example.com/root/a.xsd", doc("urn:a")),
        ("http://other.example/b.xsd", doc("urn:b")),
        ("http://example.com/svc?xsd=c", doc("urn:c")),
    ]);
    let compiled = builder(&web)
        .add("", "http://example.com/svc?wsdl")
        .and_then(|b| b.compile())
        .expect("all three imports load");
    assert_eq!(compiled.stats.documents_loaded, 4);
    assert!(
        compiled.directive_failures().is_empty(),
        "{:?}",
        compiled.directive_failures()
    );
}

/// A relative location under a relative base stays relative: it is not
/// resolved against the process's working directory, even when a file of
/// that name exists there (`Cargo.toml` exists in the directory tests run
/// in), so a host that loads documents under relative locations (through
/// the directive resolver) receives what the directive resolved to.
#[test]
fn a_relative_location_is_not_resolved_against_the_working_directory() {
    let main = format!(r#"<xs:schema {XS}><xs:include schemaLocation="Cargo.toml"/></xs:schema>"#);
    let included = format!(r#"<xs:schema {XS}><xs:element name="e"/></xs:schema>"#);
    let web = Web::new(&[("Cargo.toml", included)]);
    let mut resolver = SchemaResolver::with_loader(Box::new(web.clone()));
    let mut set = xsd_schema::SchemaSet::new();
    let doc = xsd_schema::parse_schema_only(main.as_bytes(), "main.xsd", &mut set).unwrap();
    set.mark_loaded("main.xsd".to_string(), doc);
    let result = xsd_schema::parser::resolver::resolve_all_directives(doc, &mut resolver, &mut set);
    assert!(result.is_ok(), "{:?}", result.errors);
    assert_eq!(result.loaded.len(), 1);
    assert_eq!(web.asked.lock().unwrap().clone(), ["Cargo.toml"]);
    assert!(set.is_loaded("Cargo.toml"));
}

/// The default file loader reads `file:` URIs.
#[test]
fn the_file_loader_reads_file_uris() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("uri_resolution");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("sub dir")).unwrap();
    std::fs::write(
        dir.join("main.xsd"),
        format!(r#"<xs:schema {XS} targetNamespace="urn:t"><xs:include schemaLocation="sub%20dir/x.xsd"/></xs:schema>"#),
    )
    .unwrap();
    std::fs::write(
        dir.join("sub dir").join("x.xsd"),
        format!(r#"<xs:schema {XS} targetNamespace="urn:t"><xs:element name="x"/></xs:schema>"#),
    )
    .unwrap();
    let uri = format!("file://{}", dir.join("main.xsd").to_string_lossy());
    let compiled = SchemaSetBuilder::new()
        .add("", &uri)
        .and_then(|b| b.compile())
        .expect("reads file: URIs");
    assert!(
        compiled.directive_failures().is_empty(),
        "{:?}",
        compiled.directive_failures()
    );
    assert_eq!(compiled.stats.documents_loaded, 2);
}

// ── Fix round 1: one identity per document (F3, F9, F10, F19) ─────────────

fn scratch(name: &str, files: &[(&str, String)]) -> std::path::PathBuf {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("uri_resolution_identity")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (file, text) in files {
        let path = dir.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    std::fs::canonicalize(&dir).unwrap()
}

/// F3: a local `file:` URI and the plain path it names are one document
/// (this compiled at `d6c581d`, where the `file:` spelling silently failed
/// to load, and failed with a duplicate type after C15a).
#[test]
fn a_file_uri_and_its_path_are_one_document() {
    let a = format!(r#"<xs:schema {XS}><xs:complexType name="T"/></xs:schema>"#);
    let d = scratch("file-and-path", &[("a.xsd", a)]);
    let uri = format!("file://{}", d.join("a.xsd").display());
    let main = format!(
        r#"<xs:schema {XS}><xs:include schemaLocation="a.xsd"/><xs:include schemaLocation="{uri}"/><xs:element name="r" type="T"/></xs:schema>"#
    );
    std::fs::write(d.join("main.xsd"), &main).unwrap();
    for start in [
        d.join("main.xsd").display().to_string(),
        format!("file://{}", d.join("main.xsd").display()),
    ] {
        let compiled = SchemaSetBuilder::new()
            .add("", &start)
            .and_then(|b| b.compile())
            .unwrap_or_else(|e| panic!("{start}: {e}"));
        assert_eq!(compiled.stats.documents_loaded, 2, "{start}");
    }
}

/// F9: `file:x.xsd` (no authority, a relative path — not a valid RFC 8089
/// `file:` URI) is left as it is, not turned into a host name.
#[test]
fn a_relative_file_uri_keeps_its_meaning() {
    assert_eq!(resolve("file:x.xsd", ""), "file:x.xsd");
    assert_eq!(resolve("file:sub/x.xsd", ""), "file:sub/x.xsd");
    assert_eq!(resolve("file:/x.xsd", ""), "file:///x.xsd");
}

/// F10: the fragment identifies a part of a document ("a secondary
/// resource", RFC 3986 §3.5), not another document: `a.xsd#f` is `a.xsd`.
#[test]
fn the_fragment_is_not_part_of_the_identity() {
    let main = format!(
        r#"<xs:schema {XS} targetNamespace="urn:t">
             <xs:include schemaLocation="types.xsd#part"/>
             <xs:include schemaLocation="types.xsd"/>
           </xs:schema>"#
    );
    let types = format!(
        r#"<xs:schema {XS} targetNamespace="urn:t"><xs:simpleType name="T"><xs:restriction base="xs:string"/></xs:simpleType></xs:schema>"#
    );
    let web = Web::new(&[
        ("http://example.com/s/main.xsd", main),
        ("http://example.com/s/types.xsd", types),
    ]);
    let compiled = builder(&web)
        .add("", "http://example.com/s/main.xsd")
        .and_then(|b| b.compile())
        .expect("no duplicate type T");
    assert_eq!(compiled.stats.documents_loaded, 2);
    assert_eq!(
        web.asked.lock().unwrap().clone(),
        [
            "http://example.com/s/main.xsd",
            "http://example.com/s/types.xsd"
        ]
    );
}

/// F19: the pipeline, given a relative base in another letter case (on a
/// case-insensitive file system), still finds its primary document again on
/// an include cycle.
#[test]
fn a_relative_case_variant_base_is_one_document() {
    let main = format!(
        r#"<xs:schema {XS} targetNamespace="urn:t"><xs:include schemaLocation="a.xsd"/><xs:element name="r"/></xs:schema>"#
    );
    let a = format!(
        r#"<xs:schema {XS} targetNamespace="urn:t"><xs:include schemaLocation="main.xsd"/><xs:element name="s"/></xs:schema>"#
    );
    let d = scratch(
        "case-variant/real",
        &[("main.xsd", main.clone()), ("a.xsd", a)],
    );
    // A relative spelling of <d>/main.xsd from the working directory, with
    // the directory and file names in upper case.
    let cwd = std::env::current_dir().unwrap();
    let Ok(rel) = d.strip_prefix(&cwd) else {
        return; // the scratch directory is not below the working directory
    };
    let upper = rel.join("MAIN.XSD").to_string_lossy().to_uppercase();
    if !std::path::Path::new(&upper).exists() {
        return; // a case-sensitive file system: there is no case variant
    }
    let mut set = xsd_schema::SchemaSet::new();
    xsd_schema::load_and_process_schema(main.as_bytes(), &upper, &mut set, None)
        .unwrap_or_else(|e| panic!("{upper}: {e}"));
    assert_eq!(set.documents.len(), 2);
}
