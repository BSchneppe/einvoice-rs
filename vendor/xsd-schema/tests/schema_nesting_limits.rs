//! Deeply nested schemas are refused with an error that names the limit,
//! never with a stack overflow (C17).
//!
//! Three limits bound the recursion of the schema-time passes:
//!
//! * `ParserConfig::max_depth` — the element nesting of each schema document,
//!   checked when it is read (an XML error at the element that breaks it);
//! * the same value as `SchemaSet::max_depth` — the nesting of components in
//!   each other through references (model groups, base / item / member
//!   types, attribute groups), checked once before the passes that recurse
//!   along it;
//! * `ResolverConfig::max_depth` — the nesting of `xs:include` / `xs:import`
//!   / `xs:redefine` / `xs:override` directives.
//!
//! Every compile here runs on a thread with a 2 MiB stack, the default for
//! spawned threads, in this (debug) build: within the limits the schema
//! compiles, beyond them it is refused, and the process never aborts.

use std::fmt::Write as _;

use xsd_schema::parser::parse::{ParserConfig, DEFAULT_MAX_DEPTH};
use xsd_schema::{
    load_and_process_schema, ResolverConfig, SchemaError, SchemaLimit, SchemaSet, SchemaSetBuilder,
    XsdVersion,
};

const XS: &str = r#"xmlns:xs="http://www.w3.org/2001/XMLSchema""#;
const STACK: usize = 2 << 20;

fn on_small_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(STACK)
        .spawn(f)
        .unwrap()
        .join()
        .expect("no stack overflow, no panic")
}

fn versions() -> Vec<XsdVersion> {
    if cfg!(feature = "xsd11") {
        vec![XsdVersion::V1_0, XsdVersion::V1_1]
    } else {
        vec![XsdVersion::V1_0]
    }
}

// ── Schema shapes ─────────────────────────────────────────────────────────

/// `k` nested local elements: element / complexType / sequence / element …;
/// element depth 3k + 2.
fn nested_elements(k: usize) -> String {
    let mut s = format!("<xs:schema {XS}>");
    for _ in 0..k {
        s.push_str(r#"<xs:element name="e"><xs:complexType><xs:sequence>"#);
    }
    s.push_str(r#"<xs:element name="leaf" type="xs:string"/>"#);
    for _ in 0..k {
        s.push_str("</xs:sequence></xs:complexType></xs:element>");
    }
    s.push_str("</xs:schema>");
    s
}

/// `k` nested sequences in one complex type; element depth k + 3.
fn nested_sequences(k: usize) -> String {
    let mut s = format!(r#"<xs:schema {XS}><xs:complexType name="T">"#);
    s.push_str(&"<xs:sequence>".repeat(k));
    s.push_str(r#"<xs:element name="a" type="xs:string"/>"#);
    s.push_str(&"</xs:sequence>".repeat(k));
    s.push_str("</xs:complexType></xs:schema>");
    s
}

/// `k` nested anonymous simple types (restriction of restriction …).
fn nested_simple_types(k: usize) -> String {
    let mut s = format!(r#"<xs:schema {XS}><xs:element name="r">"#);
    s.push_str(&"<xs:simpleType><xs:restriction>".repeat(k));
    s.push_str(r#"<xs:simpleType><xs:restriction base="xs:string"/></xs:simpleType>"#);
    s.push_str(&"</xs:restriction></xs:simpleType>".repeat(k));
    s.push_str("</xs:element></xs:schema>");
    s
}

/// A chain of `n` named model groups, `G_i` = sequence(group ref `G_{i-1}`),
/// referenced by a complex type: the type nests n + 1 deep.
fn group_chain(n: usize) -> String {
    let mut s = format!(
        r#"<xs:schema {XS}><xs:complexType name="T"><xs:group ref="G{}"/></xs:complexType>"#,
        n - 1
    );
    s.push_str(
        r#"<xs:group name="G0"><xs:sequence><xs:element name="a"/></xs:sequence></xs:group>"#,
    );
    for i in 1..n {
        write!(
            s,
            r#"<xs:group name="G{i}"><xs:sequence><xs:group ref="G{}"/></xs:sequence></xs:group>"#,
            i - 1
        )
        .unwrap();
    }
    s.push_str("</xs:schema>");
    s
}

/// A chain of `n` complex types, each extending the previous one with an
/// optional element: the last one nests n + 1 deep.
fn extension_chain(n: usize) -> String {
    let mut s = format!(
        r#"<xs:schema {XS}><xs:element name="r" type="T{}"/>"#,
        n - 1
    );
    s.push_str(r#"<xs:complexType name="T0"><xs:sequence><xs:element name="a0" minOccurs="0"/></xs:sequence></xs:complexType>"#);
    for i in 1..n {
        write!(
            s,
            r#"<xs:complexType name="T{i}"><xs:complexContent><xs:extension base="T{}"><xs:sequence><xs:element name="a{i}" minOccurs="0"/></xs:sequence></xs:extension></xs:complexContent></xs:complexType>"#,
            i - 1
        )
        .unwrap();
    }
    s.push_str("</xs:schema>");
    s
}

/// A chain of `n` simple types, each restricting the previous one.
fn restriction_chain(n: usize) -> String {
    let mut s = format!(
        r#"<xs:schema {XS}><xs:element name="r" type="S{}"/>"#,
        n - 1
    );
    s.push_str(r#"<xs:simpleType name="S0"><xs:restriction base="xs:string"/></xs:simpleType>"#);
    for i in 1..n {
        write!(
            s,
            r#"<xs:simpleType name="S{i}"><xs:restriction base="S{}"/></xs:simpleType>"#,
            i - 1
        )
        .unwrap();
    }
    s.push_str("</xs:schema>");
    s
}

/// A chain of `n` attribute groups, each referencing the previous one.
fn attribute_group_chain(n: usize) -> String {
    let mut s = format!(
        r#"<xs:schema {XS}><xs:complexType name="T"><xs:attributeGroup ref="A{}"/></xs:complexType>"#,
        n - 1
    );
    s.push_str(r#"<xs:attributeGroup name="A0"><xs:attribute name="a0"/></xs:attributeGroup>"#);
    for i in 1..n {
        write!(
            s,
            r#"<xs:attributeGroup name="A{i}"><xs:attribute name="a{i}"/><xs:attributeGroup ref="A{}"/></xs:attributeGroup>"#,
            i - 1
        )
        .unwrap();
    }
    s.push_str("</xs:schema>");
    s
}

/// A chain of `n` elements, each in the substitution group of the previous
/// one.
fn substitution_chain(n: usize) -> String {
    let mut s = format!(r#"<xs:schema {XS}><xs:element name="e0" type="xs:string"/>"#);
    for i in 1..n {
        write!(
            s,
            r#"<xs:element name="e{i}" substitutionGroup="e{}"/>"#,
            i - 1
        )
        .unwrap();
    }
    s.push_str("</xs:schema>");
    s
}

// ── Entry points ──────────────────────────────────────────────────────────

fn compile(version: XsdVersion, xsd: String) -> Result<(), SchemaError> {
    on_small_stack(move || {
        SchemaSetBuilder::with_version(version)
            .add_source(&xsd, "file:///nesting/main.xsd")?
            .compile()
            .map(|_| ())
    })
}

fn pipeline(version: XsdVersion, xsd: String) -> Result<(), SchemaError> {
    on_small_stack(move || {
        let mut set = SchemaSet::with_version(version);
        load_and_process_schema(xsd.as_bytes(), "file:///nesting/main.xsd", &mut set, None)
            .map(|_| ())
    })
}

#[cfg(feature = "async")]
fn pipeline_async(version: XsdVersion, xsd: String) -> Result<(), SchemaError> {
    on_small_stack(move || {
        let mut set = SchemaSet::with_version(version);
        futures::executor::block_on(xsd_schema::load_and_process_schema_async(
            xsd.as_bytes(),
            "file:///nesting/main.xsd",
            &mut set,
            None,
        ))
        .map(|_| ())
    })
}

/// Every entry point's outcome.
fn all_entry_points(
    version: XsdVersion,
    xsd: &str,
) -> Vec<(&'static str, Result<(), SchemaError>)> {
    #[allow(unused_mut)]
    let mut all = vec![
        ("builder", compile(version, xsd.to_string())),
        ("pipeline", pipeline(version, xsd.to_string())),
    ];
    #[cfg(feature = "async")]
    all.push(("async pipeline", pipeline_async(version, xsd.to_string())));
    all
}

fn assert_compiles(name: &str, xsd: &str) {
    for version in versions() {
        for (entry, result) in all_entry_points(version, xsd) {
            if let Err(e) = result {
                panic!("{name} ({version:?}, {entry}) must compile: {e}");
            }
        }
    }
}

/// Refused with an error naming `limit`, from every entry point.
fn assert_refused(name: &str, xsd: &str, xml_error: bool, limit: &str) {
    for version in versions() {
        for (entry, result) in all_entry_points(version, xsd) {
            let e = result
                .err()
                .unwrap_or_else(|| panic!("{name} ({version:?}, {entry}) is refused"));
            let text = e.to_string();
            // Decision 2 (fix round): a dedicated error variant, the limit
            // named by `SchemaLimit`.
            let kind_ok = match &e {
                SchemaError::LimitExceeded {
                    limit,
                    location,
                    max,
                    found,
                    ..
                } => {
                    let expected = if xml_error {
                        SchemaLimit::ElementDepth
                    } else {
                        SchemaLimit::ComponentDepth
                    };
                    *limit == expected && location.is_some() && found > max
                }
                _ => false,
            };
            assert!(
                kind_ok && text.contains("max_depth") && text.contains(limit),
                "{name} ({version:?}, {entry}): {text}"
            );
        }
    }
}

fn limit() -> usize {
    const { assert!(DEFAULT_MAX_DEPTH > 0) };
    assert_eq!(ParserConfig::default().max_depth, DEFAULT_MAX_DEPTH);
    assert_eq!(SchemaSet::new().max_depth(), DEFAULT_MAX_DEPTH);
    DEFAULT_MAX_DEPTH
}

// ── The element nesting of schema documents ───────────────────────────────

#[test]
fn documents_nested_up_to_the_limit_compile_on_a_2_mib_stack() {
    let l = limit();
    assert_compiles("nested elements", &nested_elements((l - 2) / 3));
    assert_compiles("nested sequences", &nested_sequences(l - 3));
    assert_compiles("nested simple types", &nested_simple_types((l - 4) / 2));
}

#[test]
fn documents_nested_beyond_the_limit_are_refused_when_read() {
    let l = limit();
    let depth = format!("element depth {}, the limit is {l}", l + 1);
    assert_refused(
        "nested elements",
        &nested_elements((l - 2) / 3 + 1),
        true,
        "ParserConfig::max_depth",
    );
    assert_refused("nested sequences", &nested_sequences(l - 2), true, &depth);
    for k in [5_000, 50_000] {
        assert_refused(
            "nested elements",
            &nested_elements(k),
            true,
            "ParserConfig::max_depth",
        );
        assert_refused(
            "nested sequences",
            &nested_sequences(k),
            true,
            "ParserConfig::max_depth",
        );
        assert_refused(
            "nested simple types",
            &nested_simple_types(k),
            true,
            "ParserConfig::max_depth",
        );
    }
}

// ── The nesting of components ─────────────────────────────────────────────

#[test]
fn component_chains_up_to_the_limit_compile_on_a_2_mib_stack() {
    let l = limit();
    assert_compiles("group chain", &group_chain(l - 1));
    assert_compiles("extension chain", &extension_chain(l - 1));
    assert_compiles("restriction chain", &restriction_chain(l / 2));
    assert_compiles("attribute group chain", &attribute_group_chain(l / 2));
    assert_compiles("substitution chain", &substitution_chain(l / 2));
}

#[test]
fn component_chains_beyond_the_limit_are_refused_before_they_are_walked() {
    let l = limit();
    let found = format!("{} levels deep", l + 1);
    assert_refused("group chain", &group_chain(l), false, &found);
    assert_refused("extension chain", &extension_chain(l), false, &found);
    for n in [l + 10, 2_000] {
        assert_refused(
            "group chain",
            &group_chain(n),
            false,
            "SchemaSet::max_depth",
        );
        assert_refused(
            "extension chain",
            &extension_chain(n),
            false,
            "SchemaSet::max_depth",
        );
        assert_refused(
            "restriction chain",
            &restriction_chain(n),
            false,
            "SchemaSet::max_depth",
        );
        assert_refused(
            "attribute group chain",
            &attribute_group_chain(n),
            false,
            "SchemaSet::max_depth",
        );
        assert_refused(
            "substitution chain",
            &substitution_chain(n),
            false,
            "SchemaSet::max_depth",
        );
    }
}

/// An extension chain compiles in time linear in its length per type: the
/// base type's content model is compiled once, not once per look at it (it
/// used to take seconds at 25 levels and grow ×2 per level).
#[test]
fn an_extension_chain_compiles_in_reasonable_time() {
    let start = std::time::Instant::now();
    assert_compiles("extension chain", &extension_chain(60));
    assert!(start.elapsed().as_secs() < 60, "{:?}", start.elapsed());
}

/// Structures §3.8.6.1 Model Group Correct, clause 2: "Circular groups are
/// disallowed." The check that measures the nesting refuses it as such
/// (it used to surface as a failed UPA check).
#[test]
fn a_circular_group_is_refused_as_such() {
    let xsd = format!(
        r#"<xs:schema {XS}><xs:group name="G"><xs:sequence><xs:element name="a"/><xs:group ref="G" minOccurs="0"/></xs:sequence></xs:group>
           <xs:complexType name="T"><xs:group ref="G"/></xs:complexType></xs:schema>"#
    );
    for version in versions() {
        for (entry, result) in all_entry_points(version, &xsd) {
            let text = result.expect_err(entry).to_string();
            assert!(
                text.contains("mg-props-correct.2") && text.contains("'G'"),
                "{version:?} {entry}: {text}"
            );
        }
    }
}

// ── Configuration ─────────────────────────────────────────────────────────

fn compile_with(parser_depth: usize, xsd: String) -> Result<(), SchemaError> {
    on_small_stack(move || {
        let mut config = ResolverConfig::default();
        config.parser_config.max_depth = parser_depth;
        SchemaSetBuilder::with_config(config)
            .add_source(&xsd, "file:///nesting/main.xsd")?
            .compile()
            .map(|_| ())
    })
}

#[test]
fn the_limit_is_configurable() {
    // Lower: both checks follow it.
    let e = compile_with(10, nested_sequences(8)).expect_err("element depth 11");
    assert!(e.to_string().contains("the limit is 10"), "{e}");
    let e = compile_with(10, group_chain(10)).expect_err("nests 11");
    assert!(e.to_string().contains("the limit is 10"), "{e}");
    compile_with(10, group_chain(9)).expect("nests 10");

    // 0: no limit of its own (the passes keep their own bounds).
    let deep = nested_sequences(DEFAULT_MAX_DEPTH + 50);
    let result = std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
            let mut config = ResolverConfig::default();
            config.parser_config.max_depth = 0;
            SchemaSetBuilder::with_config(config)
                .add_source(&deep, "file:///nesting/main.xsd")
                .and_then(|b| b.compile())
                .map(|_| ())
        })
        .unwrap()
        .join()
        .unwrap();
    result.expect("no limit");

    // The pipeline takes the parser configuration's limit.
    let mut config = xsd_schema::PipelineConfig::default();
    config.parser.max_depth = 10;
    let mut set = SchemaSet::new();
    let e = load_and_process_schema(
        group_chain(10).as_bytes(),
        "file:///p.xsd",
        &mut set,
        Some(config),
    )
    .expect_err("nests 11");
    assert!(e.to_string().contains("the limit is 10"), "{e}");

    // By hand: the set's own limit.
    let mut set = SchemaSet::new();
    set.set_max_depth(10);
    xsd_schema::parse_schema_only(group_chain(10).as_bytes(), "file:///h.xsd", &mut set).unwrap();
    let e = xsd_schema::process_loaded_schemas(&mut set).expect_err("nests 11");
    assert!(e.to_string().contains("the limit is 10"), "{e}");
}

// ── The nesting of directives ─────────────────────────────────────────────

/// `d<i>.xsd` includes `d<i+1>.xsd`, up to `d<n-1>.xsd`, in a scratch
/// directory; returns the path of `d0.xsd`.
fn include_chain_files(n: usize) -> String {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("schema_nesting_limits")
        .join(format!("chain-{n}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for i in 0..n {
        let include = if i + 1 < n {
            format!(r#"<xs:include schemaLocation="d{}.xsd"/>"#, i + 1)
        } else {
            String::new()
        };
        std::fs::write(
            dir.join(format!("d{i}.xsd")),
            format!(r#"<xs:schema {XS} targetNamespace="urn:t">{include}<xs:element name="e{i}"/></xs:schema>"#),
        )
        .unwrap();
    }
    dir.join("d0.xsd").to_string_lossy().into_owned()
}

fn include_chain(n: usize, max_depth: Option<usize>) -> Result<usize, SchemaError> {
    let main = include_chain_files(n);
    on_small_stack(move || {
        let mut config = ResolverConfig::default();
        if let Some(max) = max_depth {
            config.max_depth = max;
        }
        SchemaSetBuilder::with_config(config)
            .add("", &main)?
            .compile()
            .map(|c| c.stats.documents_loaded)
    })
}

#[test]
fn directive_chains_are_bounded_by_the_resolver_limit() {
    // Within the default limit (100).
    assert_eq!(include_chain(60, None).expect("60 documents"), 60);
    // Beyond it, and beyond a lower one.
    for (n, max, shown) in [
        (150, None, "the limit is 100"),
        (8, Some(5), "the limit is 5"),
    ] {
        let e = include_chain(n, max).expect_err("refused");
        let text = e.to_string();
        assert!(
            text.contains("ResolverConfig::max_depth") && text.contains(shown),
            "{text}"
        );
        assert!(
            matches!(
                e,
                SchemaError::LimitExceeded {
                    limit: SchemaLimit::DirectiveDepth,
                    location: Some(_),
                    ..
                }
            ),
            "{e:?}"
        );
    }
    // At the lower limit exactly: d0 … d5 (d5 at depth 5).
    assert_eq!(include_chain(6, Some(5)).expect("6 documents"), 6);
}

// ── Fix round 1 (F8, F11, F18) ────────────────────────────────────────────

/// F8: a document's directive depth is its shortest distance from the
/// documents added, whatever order the walk meets it in. `main` includes
/// `a1` and `b`; `a1` … `a6` is a chain ending in `x`; `b` includes `x`, `x`
/// includes `y`. `x` is 2 deep through `b` (7 through the chain), `y` 3.
#[test]
fn directive_depth_is_the_shortest_path() {
    for order in [["a1", "b"], ["b", "a1"]] {
        let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("schema_nesting_limits")
            .join(format!("shortest-{}", order[0]));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let doc = |name: &str, includes: &[&str]| {
            let inc: String = includes
                .iter()
                .map(|i| format!(r#"<xs:include schemaLocation="{i}.xsd"/>"#))
                .collect();
            std::fs::write(
                dir.join(format!("{name}.xsd")),
                format!(r#"<xs:schema {XS} targetNamespace="urn:t">{inc}<xs:element name="{name}"/></xs:schema>"#),
            )
            .unwrap();
        };
        doc("main", &order);
        for i in 1..=6 {
            let next = if i < 6 {
                format!("a{}", i + 1)
            } else {
                "x".to_string()
            };
            doc(&format!("a{i}"), &[&next]);
        }
        doc("b", &["x"]);
        doc("x", &["y"]);
        doc("y", &[]);
        let main = dir.join("main.xsd").to_string_lossy().into_owned();
        let loaded = on_small_stack(move || {
            let config = ResolverConfig {
                max_depth: 6,
                ..Default::default()
            };
            SchemaSetBuilder::with_config(config)
                .add("", &main)?
                .compile()
                .map(|c| c.stats.documents_loaded)
        });
        assert_eq!(
            loaded.unwrap_or_else(|e| panic!("{order:?}: {e}")),
            10,
            "{order:?}"
        );
    }
}

/// F11: a reference cycle longer than the limit is reported as a circular
/// definition, not only as a deep nesting.
#[test]
fn a_long_cycle_is_named_as_a_cycle() {
    let n = DEFAULT_MAX_DEPTH + 172;
    let mut s = format!("<xs:schema {XS}>");
    for i in 0..n {
        write!(
            s,
            r#"<xs:simpleType name="S{i}"><xs:restriction base="S{}"/></xs:simpleType>"#,
            (i + 1) % n
        )
        .unwrap();
    }
    s.push_str("</xs:schema>");
    let e = compile(XsdVersion::V1_0, s).expect_err("circular");
    let text = e.to_string();
    assert!(text.contains("circular"), "{text}");
}

/// F18: with no limit (`0`) the passes' own bounds still refuse what they
/// cannot handle — as a limit, not as a UPA violation.
#[test]
fn without_a_limit_the_own_bounds_are_named() {
    let xsd = group_chain(500);
    let result = std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            let mut config = ResolverConfig::default();
            config.parser_config.max_depth = 0;
            SchemaSetBuilder::with_config(config)
                .add_source(&xsd, "file:///nesting/main.xsd")
                .and_then(|b| b.compile())
                .map(|_| ())
        })
        .unwrap()
        .join()
        .unwrap();
    match result {
        Ok(()) => {}
        Err(SchemaError::LimitExceeded { limit, .. }) => {
            assert_eq!(limit, SchemaLimit::ComponentDepth)
        }
        Err(other) => panic!("{other}"),
    }
}
