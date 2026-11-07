//! A location rewriter maps directive locations before they are resolved
//! and identified (C15b).
//!
//! The resolver identifies a schema document by its resolved location. A
//! loader that maps a remote URL to a local copy runs after that identity is
//! chosen, so one document reached under two spellings — the SOAP 1.2
//! envelope as `http://www.w3.org/2003/05/soap-envelope` and as `https://…`,
//! as the ONVIF WSDLs import it — loads twice and its declarations clash. A
//! rewriter answers before the identity is chosen, and it sees the import's
//! namespace, which a namespace-keyed rule (a service's `?xsd=N` locations)
//! needs.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use xsd_schema::schema::CompositionEdgeKind;
use xsd_schema::{
    LocationRequest, ResolverConfig, SchemaError, SchemaLoader, SchemaResolver, SchemaResult,
    SchemaSetBuilder, XsdVersion,
};

const XS: &str = r#"xmlns:xs="http://www.w3.org/2001/XMLSchema""#;
const ENV: &str = "http://www.w3.org/2003/05/soap-envelope";

/// A local mirror: serves the envelope for both of its remote spellings (a
/// loader-only catalog), and the local documents by path.
#[derive(Debug, Clone)]
struct Mirror {
    docs: Arc<HashMap<String, String>>,
    reads: Arc<Mutex<Vec<String>>>,
}

impl Mirror {
    fn new() -> Self {
        let envelope = format!(
            r#"<xs:schema {XS} targetNamespace="{ENV}"><xs:element name="Envelope"/><xs:complexType name="Fault"/></xs:schema>"#
        );
        let a = format!(
            r#"<xs:schema {XS} targetNamespace="urn:a"><xs:import namespace="{ENV}" schemaLocation="http://www.w3.org/2003/05/soap-envelope"/></xs:schema>"#
        );
        let b = format!(
            r#"<xs:schema {XS} targetNamespace="urn:b"><xs:import namespace="{ENV}" schemaLocation="https://www.w3.org/2003/05/soap-envelope"/></xs:schema>"#
        );
        let main = format!(
            r#"<xs:schema {XS} targetNamespace="urn:m">
                 <xs:import namespace="urn:a" schemaLocation="a.xsd"/>
                 <xs:import namespace="urn:b" schemaLocation="b.xsd"/>
                 <xs:import namespace="urn:orders" schemaLocation="http://svc.example/Orders.svc?xsd=xsd0"/>
               </xs:schema>"#
        );
        let orders = format!(
            r#"<xs:schema {XS} targetNamespace="urn:orders"><xs:element name="order"/></xs:schema>"#
        );
        let mut docs = HashMap::new();
        docs.insert("/local/w3c/soap-envelope.xsd".to_string(), envelope.clone());
        docs.insert(
            "http://www.w3.org/2003/05/soap-envelope".to_string(),
            envelope.clone(),
        );
        docs.insert(
            "https://www.w3.org/2003/05/soap-envelope".to_string(),
            envelope,
        );
        docs.insert("/local/main.xsd".to_string(), main);
        docs.insert("/local/a.xsd".to_string(), a);
        docs.insert("/local/b.xsd".to_string(), b);
        docs.insert("/local/orders.xsd".to_string(), orders.clone());
        docs.insert("http://svc.example/Orders.svc?xsd=xsd0".to_string(), orders);
        Mirror {
            docs: Arc::new(docs),
            reads: Arc::default(),
        }
    }
}

impl SchemaLoader for Mirror {
    fn load(&self, location: &str) -> SchemaResult<String> {
        self.reads.lock().unwrap().push(location.to_string());
        self.docs
            .get(location)
            .cloned()
            .ok_or_else(|| SchemaError::resolution(format!("not mirrored: {location}")))
    }
    fn can_load(&self, location: &str) -> bool {
        self.docs.contains_key(location)
    }
}

/// What the rewriter was asked: location, base URI, namespace, directive
/// (`None` for a location given to the builder).
type Seen = (String, String, Option<String>, Option<CompositionEdgeKind>);

fn builder(mirror: &Mirror, version: XsdVersion) -> SchemaSetBuilder {
    // Every location reaches the mirror.
    let config = ResolverConfig {
        allow_network: true,
        ..Default::default()
    };
    let mut resolver = SchemaResolver::with_config_and_loader(config, Box::new(mirror.clone()));
    resolver.catalog_mut().add_xml_catalog();
    SchemaSetBuilder::with_resolver(resolver, version)
}

fn versions() -> Vec<XsdVersion> {
    if cfg!(feature = "xsd11") {
        vec![XsdVersion::V1_0, XsdVersion::V1_1]
    } else {
        vec![XsdVersion::V1_0]
    }
}

/// The control: a loader-only mapping reads the envelope under both
/// spellings, as two documents, and their declarations clash.
#[test]
fn a_loader_only_mapping_loads_one_document_twice() {
    for version in versions() {
        let mirror = Mirror::new();
        let error = builder(&mirror, version)
            .add("", "/local/main.xsd")
            .and_then(|b| b.compile())
            .err()
            .expect("two copies of the envelope clash");
        assert!(error.to_string().contains("Envelope"), "{error}");
    }
}

/// With a rewriter, both spellings are one local document, read once.
#[test]
fn a_rewriter_maps_both_spellings_to_one_document() {
    for version in versions() {
        let mirror = Mirror::new();
        let seen: Arc<Mutex<Vec<Seen>>> = Arc::default();
        let log = seen.clone();
        let mut builder = builder(&mirror, version);
        builder.set_location_rewriter(move |r: &LocationRequest<'_>, resolver: &SchemaResolver| {
            assert!(
                resolver.config().allow_network,
                "the rewriter sees the configuration"
            );
            log.lock().unwrap().push((
                r.location.to_string(),
                r.base_uri.to_string(),
                r.namespace.map(str::to_string),
                r.kind,
            ));
            if r.location
                .trim_start_matches("https")
                .trim_start_matches("http")
                == "://www.w3.org/2003/05/soap-envelope"
            {
                return Some("/local/w3c/soap-envelope.xsd".to_string());
            }
            // A namespace-keyed rule: the service's ?xsd=N location.
            if r.namespace == Some("urn:orders") && r.location.contains("?xsd=") {
                return Some("/local/orders.xsd".to_string());
            }
            None
        });
        let compiled = builder
            .add("", "/local/main.xsd")
            .and_then(|b| b.compile())
            .unwrap_or_else(|e| panic!("{version:?}: {e}"));
        assert_eq!(
            compiled.stats.documents_loaded, 5,
            "main, a, b, envelope, orders"
        );
        let reads = mirror.reads.lock().unwrap().clone();
        assert_eq!(
            reads.iter().filter(|r| r.contains("soap-envelope")).count(),
            1,
            "{reads:?}"
        );
        assert!(!reads.iter().any(|r| r.starts_with("http")), "{reads:?}");
        let seen = seen.lock().unwrap().clone();
        assert!(
            seen.contains(&(
                "https://www.w3.org/2003/05/soap-envelope".to_string(),
                "/local/b.xsd".to_string(),
                Some(ENV.to_string()),
                Some(CompositionEdgeKind::Import),
            )),
            "{seen:?}"
        );
    }
}

/// A rewritten location that cannot be read is reported under the location
/// it was rewritten to.
#[test]
fn a_failed_rewritten_location_is_reported() {
    let mirror = Mirror::new();
    let mut builder = builder(&mirror, XsdVersion::V1_0);
    builder.set_location_rewriter(|r: &LocationRequest<'_>, _: &SchemaResolver| {
        (r.namespace == Some("urn:orders")).then(|| "/local/missing-orders.xsd".to_string())
    });
    let main = format!(
        r#"<xs:schema {XS} targetNamespace="urn:m"><xs:import namespace="urn:orders" schemaLocation="http://svc.example/Orders.svc?xsd=xsd0"/></xs:schema>"#
    );
    let compiled = builder
        .add_source(&main, "/local/other.xsd")
        .and_then(|b| b.compile())
        .expect("a failed import is not an error");
    let failures = compiled.directive_failures();
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert_eq!(
        failures[0].schema_location(),
        Some("http://svc.example/Orders.svc?xsd=xsd0")
    );
    assert_eq!(
        failures[0].resolved_location(),
        Some("/local/missing-orders.xsd")
    );
}

#[cfg(feature = "async")]
#[test]
fn the_async_builder_applies_the_rewriter() {
    let mirror = Mirror::new();
    let mut builder = builder(&mirror, XsdVersion::V1_0);
    builder.set_location_rewriter(|r: &LocationRequest<'_>, _: &SchemaResolver| {
        if r.location.ends_with("/2003/05/soap-envelope") {
            Some("/local/w3c/soap-envelope.xsd".to_string())
        } else if r.namespace == Some("urn:orders") {
            Some("/local/orders.xsd".to_string())
        } else {
            None
        }
    });
    let compiled = futures::executor::block_on(async {
        builder
            .add_async("", "/local/main.xsd")
            .await?
            .compile_async()
            .await
    })
    .expect("compiles");
    assert_eq!(compiled.stats.documents_loaded, 5);
}

// ── Fix round 1 (decision 9, F3) ──────────────────────────────────────────

/// Decision 9: the rewriter also maps the locations given to the builder
/// (with `kind` `None`), so a document added under one spelling and named by
/// a directive under another is one document.
#[test]
fn the_rewriter_maps_added_locations_too() {
    let mirror = Mirror::new();
    let mut builder = builder(&mirror, XsdVersion::V1_0);
    let added: Arc<Mutex<Vec<Option<CompositionEdgeKind>>>> = Arc::default();
    let log = added.clone();
    builder.set_location_rewriter(move |r: &LocationRequest<'_>, _: &SchemaResolver| {
        log.lock().unwrap().push(r.kind);
        if r.location.ends_with("://www.w3.org/2003/05/soap-envelope") {
            return Some("/local/w3c/soap-envelope.xsd".to_string());
        }
        if r.namespace == Some("urn:orders") {
            return Some("/local/orders.xsd".to_string());
        }
        None
    });
    let compiled = builder
        .add("", "https://www.w3.org/2003/05/soap-envelope")
        .and_then(|b| b.add("", "/local/main.xsd"))
        .and_then(|b| b.compile())
        .expect("one envelope");
    assert_eq!(compiled.stats.documents_loaded, 5);
    assert!(
        added.lock().unwrap().contains(&None),
        "an added location is rewritten"
    );
}

/// F3: two spellings that the rewriter maps to one local file — a `file:`
/// URI and its plain path — are one document.
#[test]
fn a_file_uri_and_a_path_from_the_rewriter_are_one_document() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("location_rewriter_file");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let dir = std::fs::canonicalize(&dir).unwrap();
    let envelope = format!(
        r#"<xs:schema {XS} targetNamespace="{ENV}"><xs:element name="Envelope"/></xs:schema>"#
    );
    std::fs::write(dir.join("env.xsd"), envelope).unwrap();
    std::fs::write(
        dir.join("a.xsd"),
        format!(r#"<xs:schema {XS} targetNamespace="urn:a"><xs:import namespace="{ENV}" schemaLocation="http://www.w3.org/2003/05/soap-envelope"/></xs:schema>"#),
    )
    .unwrap();
    std::fs::write(
        dir.join("b.xsd"),
        format!(r#"<xs:schema {XS} targetNamespace="urn:b"><xs:import namespace="{ENV}" schemaLocation="https://www.w3.org/2003/05/soap-envelope"/></xs:schema>"#),
    )
    .unwrap();
    std::fs::write(
        dir.join("main.xsd"),
        format!(r#"<xs:schema {XS} targetNamespace="urn:m"><xs:import namespace="urn:a" schemaLocation="a.xsd"/><xs:import namespace="urn:b" schemaLocation="b.xsd"/></xs:schema>"#),
    )
    .unwrap();
    let env = dir.join("env.xsd");
    let mut builder = SchemaSetBuilder::new();
    builder.set_location_rewriter(move |r: &LocationRequest<'_>, _: &SchemaResolver| {
        if r.location == "http://www.w3.org/2003/05/soap-envelope" {
            Some(format!("file://{}", env.display()))
        } else if r.location == "https://www.w3.org/2003/05/soap-envelope" {
            Some(env.display().to_string())
        } else {
            None
        }
    });
    let compiled = builder
        .add("", &dir.join("main.xsd").to_string_lossy())
        .and_then(|b| b.compile())
        .expect("one envelope, no duplicate");
    assert_eq!(compiled.stats.documents_loaded, 4);
}

// ── Fix round 2: a rewriter's answer is final, and asked for once ────────

/// An in-memory loader that records what it reads, exactly as asked.
#[derive(Debug, Clone, Default)]
struct Docs {
    docs: Arc<Mutex<HashMap<String, String>>>,
    reads: Arc<Mutex<Vec<String>>>,
}

impl Docs {
    fn with(entries: &[(&str, String)]) -> Self {
        let docs = Docs::default();
        for (location, text) in entries {
            docs.docs
                .lock()
                .unwrap()
                .insert(location.to_string(), text.clone());
        }
        docs
    }
    fn reads(&self) -> Vec<String> {
        self.reads.lock().unwrap().clone()
    }
}

impl SchemaLoader for Docs {
    fn load(&self, location: &str) -> SchemaResult<String> {
        self.reads.lock().unwrap().push(location.to_string());
        self.docs
            .lock()
            .unwrap()
            .get(location)
            .cloned()
            .ok_or_else(|| SchemaError::resolution(format!("no document at {location}")))
    }
    fn can_load(&self, location: &str) -> bool {
        self.docs.lock().unwrap().contains_key(location)
    }
}

fn docs_builder(docs: &Docs, version: XsdVersion) -> SchemaSetBuilder {
    docs_builder_with_catalog(docs, version, &[])
}

fn docs_builder_with_catalog(
    docs: &Docs,
    version: XsdVersion,
    catalog: &[(&str, &str)],
) -> SchemaSetBuilder {
    let config = ResolverConfig {
        allow_network: true,
        ..Default::default()
    };
    let mut resolver = SchemaResolver::with_config_and_loader(config, Box::new(docs.clone()));
    for (namespace, location) in catalog {
        resolver.catalog_mut().add(*namespace, *location);
    }
    SchemaSetBuilder::with_resolver(resolver, version)
}

fn directive_kinds() -> Vec<(&'static str, XsdVersion)> {
    let mut kinds = vec![
        ("include", XsdVersion::V1_0),
        ("import", XsdVersion::V1_0),
        ("redefine", XsdVersion::V1_0),
    ];
    if cfg!(feature = "xsd11") {
        kinds.push(("override", XsdVersion::V1_1));
    }
    kinds
}

/// A main document at `main` whose directive of `kind` names
/// `http://remote.example/s/types.xsd`; the types document it should get
/// declares `U` (and `T`, which a redefine / override redefines).
fn remote_to_local(kind: &str, main: &str) -> (String, String) {
    let remote = "http://remote.example/s/types.xsd";
    let (tns, directive) = match kind {
        "import" => (
            r#" targetNamespace="urn:m" xmlns:t="urn:t""#,
            format!(r#"<xs:import namespace="urn:t" schemaLocation="{remote}"/>"#),
        ),
        "include" => ("", format!(r#"<xs:include schemaLocation="{remote}"/>"#)),
        "redefine" => (
            "",
            format!(
                r#"<xs:redefine schemaLocation="{remote}"><xs:simpleType name="T"><xs:restriction base="T"/></xs:simpleType></xs:redefine>"#
            ),
        ),
        "override" => (
            "",
            format!(
                r#"<xs:override schemaLocation="{remote}"><xs:simpleType name="T"><xs:restriction base="xs:string"/></xs:simpleType></xs:override>"#
            ),
        ),
        other => panic!("{other}"),
    };
    let u = if kind == "import" { "t:U" } else { "U" };
    let main_doc =
        format!(r#"<xs:schema {XS}{tns}>{directive}<xs:element name="r" type="{u}"/></xs:schema>"#);
    let types_tns = if kind == "import" {
        r#" targetNamespace="urn:t""#
    } else {
        ""
    };
    let types = format!(
        r#"<xs:schema {XS}{types_tns}><xs:simpleType name="T"><xs:restriction base="xs:string"/></xs:simpleType><xs:simpleType name="U"><xs:restriction base="xs:string"/></xs:simpleType></xs:schema>"#
    );
    let _ = main;
    (main_doc, types)
}

/// The reviewer's first reproducer, for every directive, under an `http:`
/// and a `file:` base, synchronously and asynchronously: a rewriter maps a
/// remote location to a local absolute path; that path is what is read —
/// not the path resolved against the including document's base
/// (`http://host.example/mirror/types.xsd`).
#[test]
fn a_local_path_from_the_rewriter_is_read_as_given() {
    let mut problems = Vec::new();
    for main in ["http://host.example/s/main.xsd", "file:///srv/s/main.xsd"] {
        for (kind, version) in directive_kinds() {
            for asynchronous in [false, true] {
                if asynchronous && !cfg!(feature = "async") {
                    continue;
                }
                let (main_doc, types) = remote_to_local(kind, main);
                let docs = Docs::with(&[(main, main_doc), ("/mirror/types.xsd", types)]);
                let mut builder = docs_builder(&docs, version);
                builder.set_location_rewriter(|r: &LocationRequest<'_>, _: &SchemaResolver| {
                    (r.location == "http://remote.example/s/types.xsd")
                        .then(|| "/mirror/types.xsd".to_string())
                });
                let what = format!("{kind} under {main}, async {asynchronous}");
                let result = if asynchronous {
                    compile_async(builder, main)
                } else {
                    builder.add("", main).and_then(|b| b.compile())
                };
                let reads = docs.reads();
                match result {
                    Err(e) => problems.push(format!("{what}: {e} (read {reads:?})")),
                    Ok(compiled) if compiled.stats.documents_loaded != 2 => problems.push(format!(
                        "{what}: {} documents (read {reads:?})",
                        compiled.stats.documents_loaded
                    )),
                    Ok(_) if !reads.iter().any(|r| r == "/mirror/types.xsd") => {
                        problems.push(format!("{what}: the mirror not read: {reads:?}"))
                    }
                    Ok(_) => {}
                }
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[cfg(feature = "async")]
fn compile_async(
    builder: SchemaSetBuilder,
    main: &str,
) -> SchemaResult<xsd_schema::CompiledSchemaSet> {
    futures::executor::block_on(async { builder.add_async("", main).await?.compile_async().await })
}

#[cfg(not(feature = "async"))]
fn compile_async(_: SchemaSetBuilder, _: &str) -> SchemaResult<xsd_schema::CompiledSchemaSet> {
    unreachable!()
}

/// A relative answer is resolved against the working directory (as a
/// relative location given to `add` is), not against the directive's base.
#[test]
fn a_relative_answer_is_resolved_against_the_working_directory() {
    let cwd = std::env::current_dir().unwrap();
    let local = cwd
        .join("mirror-rel")
        .join("types.xsd")
        .to_string_lossy()
        .into_owned();
    let main = "http://host.example/s/main.xsd";
    let (main_doc, types) = remote_to_local("include", main);
    let docs = Docs::with(&[(main, main_doc), (local.as_str(), types)]);
    let mut builder = docs_builder(&docs, XsdVersion::V1_0);
    builder.set_location_rewriter(|r: &LocationRequest<'_>, _: &SchemaResolver| {
        (r.location == "http://remote.example/s/types.xsd")
            .then(|| "mirror-rel/types.xsd".to_string())
    });
    builder
        .add("", main)
        .and_then(|b| b.compile())
        .unwrap_or_else(|e| panic!("{e}: {:?}", docs.reads()));
    assert!(docs.reads().contains(&local), "{:?}", docs.reads());
}

/// The chained mappings of the reviewer's second reproducer: `…a.xsd` → B,
/// B → C. Every entry point that names A loads B — the answer is never fed
/// to the rewriter again.
#[test]
fn chained_mappings_are_applied_once_at_every_entry_point() {
    let b = format!(r#"<xs:schema {XS}><xs:element name="b"/></xs:schema>"#);
    let c = format!(r#"<xs:schema {XS}><xs:element name="c"/></xs:schema>"#);
    let bn =
        format!(r#"<xs:schema {XS} targetNamespace="urn:cat"><xs:element name="b"/></xs:schema>"#);
    let cn =
        format!(r#"<xs:schema {XS} targetNamespace="urn:cat"><xs:element name="c"/></xs:schema>"#);
    let chain = |r: &LocationRequest<'_>, _: &SchemaResolver| -> Option<String> {
        if r.location.ends_with("cat-a.xsd") {
            Some("/m/bn.xsd".to_string())
        } else if r.location == "/m/bn.xsd" {
            Some("/m/cn.xsd".to_string())
        } else if r.location.ends_with("a.xsd") {
            Some("/m/b.xsd".to_string())
        } else if r.location == "/m/b.xsd" {
            Some("/m/c.xsd".to_string())
        } else {
            None
        }
    };
    let directive_main =
        |directive: &str, tns: &str| format!(r#"<xs:schema {XS}{tns}>{directive}</xs:schema>"#);
    let cases: Vec<(&str, XsdVersion, String)> = {
        let mut v = vec![
            (
                "include",
                XsdVersion::V1_0,
                directive_main(r#"<xs:include schemaLocation="a.xsd"/>"#, ""),
            ),
            (
                "import",
                XsdVersion::V1_0,
                directive_main(
                    r#"<xs:import schemaLocation="a.xsd"/>"#,
                    r#" targetNamespace="urn:m""#,
                ),
            ),
            (
                "redefine",
                XsdVersion::V1_0,
                directive_main(r#"<xs:redefine schemaLocation="a.xsd"/>"#, ""),
            ),
            (
                "catalog",
                XsdVersion::V1_0,
                directive_main(
                    r#"<xs:import namespace="urn:cat"/>"#,
                    r#" targetNamespace="urn:m""#,
                ),
            ),
        ];
        if cfg!(feature = "xsd11") {
            v.push((
                "override",
                XsdVersion::V1_1,
                directive_main(r#"<xs:override schemaLocation="a.xsd"/>"#, ""),
            ));
        }
        v
    };
    let all = |extra: &[(&str, String)]| {
        let mut entries = vec![
            ("/m/b.xsd", b.clone()),
            ("/m/c.xsd", c.clone()),
            ("/m/bn.xsd", bn.clone()),
            ("/m/cn.xsd", cn.clone()),
        ];
        entries.extend(extra.iter().cloned());
        Docs::with(&entries)
    };
    let problems: std::cell::RefCell<Vec<String>> = Default::default();
    let loads_b = |what: &str, docs: &Docs| {
        let reads = docs.reads();
        if !reads.iter().any(|r| r == "/m/b.xsd" || r == "/m/bn.xsd") {
            problems
                .borrow_mut()
                .push(format!("{what}: B not read: {reads:?}"));
        }
        if reads.iter().any(|r| r == "/m/c.xsd" || r == "/m/cn.xsd") {
            problems.borrow_mut().push(format!(
                "{what}: C read (the answer was rewritten again): {reads:?}"
            ));
        }
    };
    let failed = |what: &str, e: SchemaError, docs: &Docs| {
        problems
            .borrow_mut()
            .push(format!("{what}: {e} (read {:?})", docs.reads()));
    };

    // Locations given to the builder.
    let docs = all(&[]);
    let mut builder = docs_builder(&docs, XsdVersion::V1_0);
    builder.set_location_rewriter(chain);
    match builder.try_add("http://remote.example/a.xsd") {
        Ok(_) => loads_b("try_add", &docs),
        Err(e) => failed("try_add", e, &docs),
    }

    let docs = all(&[]);
    let mut builder = docs_builder(&docs, XsdVersion::V1_0);
    builder.set_location_rewriter(chain);
    match builder.try_add_relative("a.xsd", "http://remote.example/main.xsd") {
        Ok(_) => loads_b("try_add_relative under an http: base", &docs),
        Err(e) => failed("try_add_relative under an http: base", e, &docs),
    }

    // The reviewer's case: a base under which the answer stays what it is.
    let docs = all(&[]);
    let mut builder = docs_builder(&docs, XsdVersion::V1_0);
    builder.set_location_rewriter(chain);
    match builder.try_add_relative("a.xsd", "/x/main.xsd") {
        Ok(_) => loads_b("try_add_relative under a path base", &docs),
        Err(e) => failed("try_add_relative under a path base", e, &docs),
    }

    let docs = all(&[]);
    let mut builder = docs_builder(&docs, XsdVersion::V1_0);
    builder.set_location_rewriter(chain);
    match builder.add("", "/x/a.xsd").and_then(|b| b.compile()) {
        Ok(compiled) => {
            loads_b("add", &docs);
            // add_from re-adds the locations a set was loaded from: final
            // already.
            let docs2 = all(&[]);
            let mut again = docs_builder(&docs2, XsdVersion::V1_0);
            again.set_location_rewriter(chain);
            again.add_from(compiled.schema_set());
            loads_b("add_from", &docs2);
        }
        Err(e) => failed("add", e, &docs),
    }

    #[cfg(feature = "async")]
    {
        let docs = all(&[]);
        let mut builder = docs_builder(&docs, XsdVersion::V1_0);
        builder.set_location_rewriter(chain);
        match futures::executor::block_on(async {
            builder
                .add_async("", "/x/a.xsd")
                .await?
                .compile_async()
                .await
        }) {
            Ok(_) => loads_b("add_async", &docs),
            Err(e) => failed("add_async", e, &docs),
        }
    }

    // Directives, synchronously and asynchronously.
    for (kind, version, main) in cases {
        for asynchronous in [false, true] {
            if asynchronous && !cfg!(feature = "async") {
                continue;
            }
            let docs = all(&[("/m/main.xsd", main.clone())]);
            let mut builder = docs_builder_with_catalog(
                &docs,
                version,
                &[("urn:cat", "http://remote.example/cat-a.xsd")],
            );
            builder.set_location_rewriter(chain);
            let what = format!("{kind}, async {asynchronous}");
            let result = if asynchronous {
                compile_async(builder, "/m/main.xsd")
            } else {
                builder.add("", "/m/main.xsd").and_then(|b| b.compile())
            };
            match result {
                Ok(_) => loads_b(&what, &docs),
                Err(e) => failed(&what, e, &docs),
            }
        }
    }
    let problems = problems.into_inner();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The rewriter is asked once per location — per (location as written, base
/// URI, namespace, directive) — on the synchronous and the asynchronous
/// path: not again to check whether the document is loaded, to load it, to
/// try the catalog, to report a failure or after a prefetch.
#[test]
fn the_rewriter_is_asked_once_per_location() {
    let main = format!(
        r#"<xs:schema {XS} targetNamespace="urn:m">
             <xs:include schemaLocation="inc.xsd"/>
             <xs:include schemaLocation="missing.xsd"/>
             <xs:import namespace="urn:i" schemaLocation="imp.xsd"/>
             <xs:import namespace="urn:c"/>
             <xs:import namespace="urn:h" schemaLocation="hint-missing.xsd"/>
           </xs:schema>"#
    );
    let inc =
        format!(r#"<xs:schema {XS} targetNamespace="urn:m"><xs:element name="i"/></xs:schema>"#);
    let imp =
        format!(r#"<xs:schema {XS} targetNamespace="urn:i"><xs:element name="i"/></xs:schema>"#);
    let c =
        format!(r#"<xs:schema {XS} targetNamespace="urn:c"><xs:element name="c"/></xs:schema>"#);
    let h =
        format!(r#"<xs:schema {XS} targetNamespace="urn:h"><xs:element name="h"/></xs:schema>"#);
    let mut problems = Vec::new();
    for asynchronous in [false, true] {
        if asynchronous && !cfg!(feature = "async") {
            continue;
        }
        let docs = Docs::with(&[
            ("/r/main.xsd", main.clone()),
            ("/r/inc.xsd", inc.clone()),
            ("/r/imp.xsd", imp.clone()),
            ("/r/c.xsd", c.clone()),
            ("/r/h.xsd", h.clone()),
        ]);
        let mut builder = docs_builder_with_catalog(
            &docs,
            XsdVersion::V1_0,
            &[
                ("urn:c", "http://cat.example/c.xsd"),
                ("urn:h", "http://cat.example/h.xsd"),
            ],
        );
        let asked: Arc<Mutex<HashMap<String, usize>>> = Arc::default();
        let log = asked.clone();
        builder.set_location_rewriter(move |r: &LocationRequest<'_>, _: &SchemaResolver| {
            let key = format!(
                "{} | {} | {:?} | {:?}",
                r.location, r.base_uri, r.namespace, r.kind
            );
            *log.lock().unwrap().entry(key).or_default() += 1;
            match r.location {
                "http://cat.example/c.xsd" => Some("/r/c.xsd".to_string()),
                "http://cat.example/h.xsd" => Some("/r/h.xsd".to_string()),
                _ => None,
            }
        });
        let compiled = if asynchronous {
            compile_async(builder, "/r/main.xsd")
        } else {
            builder.add("", "/r/main.xsd").and_then(|b| b.compile())
        }
        .unwrap_or_else(|e| panic!("async {asynchronous}: {e}"));
        assert_eq!(compiled.directive_failures().len(), 1, "missing.xsd");
        let asked = asked.lock().unwrap().clone();
        let mut repeated: Vec<_> = asked.iter().filter(|(_, n)| **n > 1).collect();
        repeated.sort();
        if !repeated.is_empty() {
            problems.push(format!(
                "async {asynchronous}: asked more than once: {repeated:?}"
            ));
        }
        // main, inc, missing, imp, the catalog's c, hint-missing, the catalog's h.
        if asked.len() != 7 {
            problems.push(format!(
                "async {asynchronous}: {} requests: {asked:?}",
                asked.len()
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
