use super::*;
use std::{
    io::{Cursor, Write},
    path::PathBuf,
};

const ENTRY: &str = include_str!("../../tests/fixtures/resolution/entry.sch");
const CONCRETE: &str = include_str!("../../tests/fixtures/resolution/concrete.sch");
const ABSTRACT: &str = include_str!("../../tests/fixtures/resolution/abstract.sch");
const RAW: &str = include_str!("../../tests/fixtures/resolution/raw.sch");
const COMPARISON: &str = include_str!("../../tests/fixtures/resolution/comparison.xml");
fn authored() -> BTreeMap<String, String> {
    [
        ("entry.sch", ENTRY),
        ("concrete.sch", CONCRETE),
        ("abstract.sch", ABSTRACT),
        ("raw.sch", RAW),
        ("comparison.xml", COMPARISON),
    ]
    .into_iter()
    .map(|(k, v)| (k.into(), v.into()))
    .collect()
}
fn sources(texts: &BTreeMap<String, String>) -> source::Sources {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, text) in texts {
        zip.start_file(name, zip::write::FileOptions::default())
            .unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    }
    let bytes = zip.finish().unwrap().into_inner();
    let pin = source::Pin {
        file: "fixture.zip".into(),
        version: "authored".into(),
        sha256: source::hash(&bytes),
        members: Some(
            texts
                .iter()
                .map(|(k, v)| (k.clone(), source::hash(v.as_bytes())))
                .collect(),
        ),
    };
    source::Sources {
        archives: BTreeMap::from([(
            "fixture.zip".into(),
            source::verify_archive(bytes, pin).unwrap(),
        )]),
    }
}
fn guard(node: Node<'_, '_>, resource: &str) -> SourceNode {
    let mut chain: Vec<_> = node.ancestors().filter(Node::is_element).collect();
    chain.reverse();
    SourceNode {
        selector: chain
            .iter()
            .map(|n| StructuralStep {
                namespace: n.tag_name().namespace().unwrap_or("").into(),
                local: n.tag_name().name().into(),
                sibling: n
                    .prev_siblings()
                    .skip(1)
                    .filter(|s| s.is_element() && s.tag_name() == n.tag_name())
                    .count(),
                identity: n
                    .attributes()
                    .filter(|a| ["id", "name", "href", "prefix"].contains(&a.name()))
                    .map(|a| (a.name().into(), a.value().into()))
                    .collect(),
            })
            .collect(),
        node: origin(node, resource),
        ancestors: chain[..chain.len() - 1]
            .iter()
            .map(|n| origin(*n, resource))
            .collect(),
    }
}
fn guards(texts: &BTreeMap<String, String>) -> BTreeMap<String, SourceNode> {
    let mut out = BTreeMap::new();
    for (member, text) in texts {
        let doc = source::xml(text).unwrap();
        let resource = format!("fixture.zip/{member}");
        for n in doc.descendants().filter(Node::is_element) {
            let key = match member.as_str() {
                "entry.sch" => match n.tag_name().name() {
                    "schema" => Some("entry"),
                    "include" => match n.attribute("href") {
                        Some("abstract.sch") => Some("abstract_include"),
                        Some("concrete.sch") => Some("concrete_include"),
                        _ => None,
                    },
                    _ => None,
                },
                "concrete.sch" => match n.tag_name().name() {
                    "pattern" => Some("concrete_pattern"),
                    "param" => match n.attribute("name") {
                        Some("CII-SR-282") => {
                            if n.attribute("value").unwrap().contains("buyer") {
                                Some("buyer")
                            } else {
                                Some("seller")
                            }
                        }
                        Some("Agreement") => Some("context_param"),
                        _ => None,
                    },
                    _ => None,
                },
                "abstract.sch" => match n.tag_name().name() {
                    "pattern" => Some("abstract_pattern"),
                    "rule" => Some("abstract_rule"),
                    "assert" if n.attribute("id") == Some("CII-SR-282") => Some("abstract_site"),
                    _ => None,
                },
                "raw.sch" => match n.tag_name().name() {
                    "rule" => Some("raw_rule"),
                    "assert" if n.attribute("id") == Some("CII-SR-282") => Some("raw_site"),
                    _ => None,
                },
                "comparison.xml" => match n.tag_name().name() {
                    "release" => Some("release_comparison"),
                    "configured" => Some("configured_comparison"),
                    "condition" => Some("configured_condition"),
                    "scenario" => Some("scenario"),
                    _ => None,
                },
                _ => None,
            };
            if let Some(key) = key {
                out.insert(key.into(), guard(n, &resource));
            }
        }
    }
    out
}
struct Fixture {
    texts: BTreeMap<String, String>,
    sources: source::Sources,
    raw: bundle::Inspection,
    manifest: Manifest,
}
impl Fixture {
    fn new() -> Self {
        let texts = authored();
        let schema = schematron::parse(
            &BTreeMap::from([("fixture.zip/raw.sch".into(), RAW.into())]),
            "fixture.zip/raw.sch",
            None,
        )
        .unwrap();
        let site = &schema.patterns[0].rules[0].sites[0];
        let mut manifest = load_manifest(MANIFEST).unwrap();
        manifest.target = "authored".into();
        manifest.source_package = "authored:1".into();
        manifest.namespaces = schema.namespaces.clone();
        let d = &mut manifest.decisions[0];
        d.context_parameter = "Agreement".into();
        d.message_sha256 = source::hash(b"Authored buyer diagnostic.");
        d.original_ast_sha256 = digest(&site.test.tree).unwrap();
        let c = xpath::Context {
            namespaces: schema.namespaces.clone(),
            helpers: BTreeSet::new(),
        };
        let concrete_doc = source::xml(CONCRETE).unwrap();
        let buyer = concrete_doc
            .descendants()
            .find(|n| n.attribute("value").is_some_and(|v| v.contains("buyer")))
            .unwrap();
        d.effective_ast_sha256 = digest(
            &expression(buyer, "fixture.zip/concrete.sch", &c, false)
                .unwrap()
                .tree,
        )
        .unwrap();
        d.context_ast_sha256 = digest(&schema.patterns[0].rules[0].context.tree).unwrap();
        let resource = Resource {
            archive: "fixture.zip".into(),
            member: "raw.sch".into(),
            sha256: source::hash(RAW.as_bytes()),
            size: RAW.len() as u64,
            role: ResourceRole::Schematron,
            pin: "authored".into(),
        };
        let package = Package {
            format: validation_ir::FORMAT.into(),
            target: manifest.target.clone(),
            source_package: manifest.source_package.clone(),
            backend: BackendState::Unavailable,
            schemas: vec![schema],
            resources: BTreeMap::from([("fixture.zip/raw.sch".into(), resource)]),
            source_graph: Vec::new(),
            policy: None,
            requirements: BTreeSet::new(),
        };
        manifest.raw_ir_sha256 = digest(&package).unwrap();
        manifest.policy_sha256 = digest(&package.policy).unwrap();
        manifest.nodes = guards(&texts);
        let raw = bundle::Inspection {
            package,
            bytes: BTreeMap::from([("fixture.zip/raw.sch".into(), RAW.as_bytes().to_vec())]),
            archives: BTreeMap::new(),
        };
        let mut f = Self {
            sources: sources(&texts),
            texts,
            raw,
            manifest,
        };
        f.repin();
        f
    }
    /// Refresh the fixture ZIP and member pins, leaving semantic and node guards alone.
    fn repin(&mut self) {
        self.sources = sources(&self.texts);
        self.manifest.resources = self
            .texts
            .iter()
            .map(|(member, text)| {
                (
                    format!("fixture.zip/{member}"),
                    ResourcePin {
                        archive_sha256: self.sources.archives["fixture.zip"].pin.sha256.clone(),
                        sha256: source::hash(text.as_bytes()),
                        size: text.len() as u64,
                        role: if member == "comparison.xml" {
                            ResourceRole::OracleExecutableEvidence
                        } else {
                            ResourceRole::Schematron
                        },
                    },
                )
            })
            .collect();
    }
    fn prepare(&self) -> Result<Prepared> {
        prepare(&self.sources, &self.raw, &self.manifest)
    }
    fn update_raw_digest(&mut self) {
        self.manifest.raw_ir_sha256 = digest(&self.raw.package).unwrap();
    }
    fn assert_error(&self, expected: &str) {
        let error = self
            .prepare()
            .err()
            .expect("accepted invalid fixture")
            .to_string();
        assert!(
            error.contains(expected),
            "expected {expected:?}, got {error}"
        );
    }
}
#[test]
fn authored_linkage_materializes_only_existing_source_and_roundtrips() {
    let f = Fixture::new();
    let p = f.prepare().unwrap();
    let c = materialize(&f.raw.package, &f.manifest, &p).unwrap();
    validate_candidate(&f.raw.package, &f.manifest, &p, &c).unwrap();
    assert_eq!(
        c.resolutions[0].duplicates[0].expression,
        c.resolutions[0].effective
    );
    assert_eq!(
        c.resolutions[0].duplicates[1].expression.tree,
        c.resolutions[0].original_site.test.tree
    );
    assert_eq!(
        c.effective_package.schemas[0].patterns[0].rules[0].sites[0].test,
        c.resolutions[0].effective
    );
    assert_eq!(
        c.effective_package.schemas[0].patterns[0].rules[0].sites[1],
        f.raw.package.schemas[0].patterns[0].rules[0].sites[1]
    );
    let bytes = emit::canonical(&c).unwrap();
    assert_eq!(
        serde_json::from_slice::<ResolvedCandidate>(&bytes).unwrap(),
        c
    );
    // Raw/candidate formats cannot be substituted by deserialization.
    assert!(
        serde_json::from_slice::<ResolvedCandidate>(&emit::canonical(&f.raw.package).unwrap())
            .is_err()
    );
    assert!(serde_json::from_slice::<Package>(&bytes).is_err());
    let mut unknown = serde_json::to_value(&c).unwrap();
    unknown["supported"] = true.into();
    assert!(serde_json::from_value::<ResolvedCandidate>(unknown).is_err());
    for (id, g) in &c.resolutions[0].lineage {
        let (_, member) = g.node.source.resource.split_once('/').unwrap();
        let doc = source::xml(&f.texts[member]).unwrap();
        assert!(select(&doc, g).is_ok(), "{id}");
    }
    assert_eq!(
        c.correspondence_audit,
        CorrespondenceAudit::PendingFullPredicateContextHelperMessagePolicyCorrespondence
    );
}
#[test]
fn manifest_unknown_duplicate_fields_and_decisions_fail_closed() {
    for text in [
        MANIFEST.replacen(
            "\"format\":",
            "\"replacement_expression\": \"true()\", \"format\":",
            1,
        ),
        MANIFEST.replacen("\"format\":", "\"format\": \"unused\", \"format\":", 1),
        MANIFEST.replacen("\"buyer\": {", "\"buyer\": {}, \"buyer\": {", 1),
        MANIFEST.replace("original_declarative_parameter", "first_wins"),
    ] {
        assert!(load_manifest(&text).is_err());
    }
    let mut field = serde_json::from_str::<serde_json::Value>(MANIFEST).unwrap();
    field["nodes"]["buyer"]["node"]["source"]["ignored"] = true.into();
    assert!(load_manifest(&field.to_string()).is_err());
    for change in 0..5 {
        let mut f = Fixture::new();
        match change {
            0 => f.manifest.decisions.clear(),
            1 => f.manifest.decisions.push(f.manifest.decisions[0].clone()),
            2 => f.manifest.decisions[0].id = "unknown".into(),
            3 => {
                f.manifest
                    .nodes
                    .insert("unused".into(), f.manifest.nodes["buyer"].clone());
            }
            _ => f.manifest.decisions[0].selected = "seller".into(),
        }
        assert!(f.prepare().is_err());
    }
}
#[test]
fn archive_member_site_attribute_range_parent_selector_guards_are_independent() {
    for change in 0..8 {
        let mut f = Fixture::new();
        let expected = match change {
            0 => {
                f.manifest
                    .resources
                    .get_mut("fixture.zip/concrete.sch")
                    .unwrap()
                    .archive_sha256 = "0".repeat(64);
                "archive identity"
            }
            1 => {
                f.manifest
                    .resources
                    .get_mut("fixture.zip/concrete.sch")
                    .unwrap()
                    .sha256 = "0".repeat(64);
                "member hash"
            }
            2 => {
                f.manifest.nodes.get_mut("buyer").unwrap().node.sha256 = "0".repeat(64);
                "node range/hash/attribute"
            }
            3 => {
                f.manifest
                    .nodes
                    .get_mut("buyer")
                    .unwrap()
                    .node
                    .source
                    .xml_byte_end += 1;
                "node range/hash/attribute"
            }
            4 => {
                f.manifest
                    .nodes
                    .get_mut("buyer")
                    .unwrap()
                    .node
                    .attributes
                    .get_mut("value")
                    .unwrap()
                    .sha256 = "0".repeat(64);
                "node range/hash/attribute"
            }
            5 => {
                f.manifest.nodes.get_mut("buyer").unwrap().ancestors[0].sha256 = "0".repeat(64);
                "parent/ancestry"
            }
            6 => {
                f.manifest
                    .nodes
                    .get_mut("buyer")
                    .unwrap()
                    .selector
                    .last_mut()
                    .unwrap()
                    .sibling = 100;
                "structural selector"
            }
            _ => {
                f.texts.insert(
                    "concrete.sch".into(),
                    CONCRETE.replace(
                        "<param name=\"CII-SR-282\" value=\"not(t:buyer)\"/>",
                        "<wrapper><param name=\"CII-SR-282\" value=\"not(t:buyer)\"/></wrapper>",
                    ),
                );
                f.repin();
                "node range/hash/attribute"
            }
        };
        f.assert_error(expected);
    }
}
#[test]
fn drift_diagnostics_identify_distinct_resources_and_guards() {
    for (key, resource) in [
        ("buyer", "fixture.zip/concrete.sch"),
        ("abstract_site", "fixture.zip/abstract.sch"),
    ] {
        let mut f = Fixture::new();
        f.manifest.nodes.get_mut(key).unwrap().node.sha256 = "0".repeat(64);
        let error = f.prepare().err().unwrap().to_string();
        assert!(error.contains(&format!("guard {key}:")), "{error}");
        assert!(error.contains(resource), "{error}");
        assert!(error.contains("node range/hash/attribute drift"), "{error}");

        let mut f = Fixture::new();
        f.manifest.resources.get_mut(resource).unwrap().sha256 = "0".repeat(64);
        let error = f.prepare().err().unwrap().to_string();
        assert!(error.contains(resource), "{error}");
        assert!(error.contains("member hash/size drift"), "{error}");
    }
}
#[test]
fn duplicate_missing_extra_reordered_and_unknown_names_rejected_after_repinning() {
    let buyer = "  <param name=\"CII-SR-282\" value=\"not(t:buyer)\"/>\n";
    let seller = "  <param name=\"CII-SR-282\" value=\"not(t:seller/t:id)\"/>\n";
    for text in [
        CONCRETE.replace(buyer, ""),
        CONCRETE.replace(seller, &format!("{seller}{seller}")),
        CONCRETE.replace(&format!("{buyer}{seller}"), &format!("{seller}{buyer}")),
        CONCRETE.replace(
            "</pattern>",
            "<param name=\"Other\" value=\"false()\"/></pattern>",
        ),
    ] {
        let mut f = Fixture::new();
        f.texts.insert("concrete.sch".into(), text);
        f.repin();
        // Refresh every surviving site guard too: extra/reordered sets must reach
        // the complete ordered-set check, rather than stopping at stale ZIP hashes.
        let fresh = guards(&f.texts);
        for (key, node) in fresh {
            f.manifest.nodes.insert(key, node);
        }
        let expected = if !f.texts["concrete.sch"].contains("not(t:buyer)") {
            "node range/hash/attribute drift"
        } else if f.texts["concrete.sch"].contains("false()") {
            "unreviewed duplicate binding"
        } else {
            "duplicate set"
        };
        f.assert_error(expected);
    }
}
#[test]
fn linkage_namespace_context_message_flag_and_policy_drift_rejected_beyond_member_hash() {
    for (member, from, to, expected) in [
        (
            "concrete.sch",
            "is-a=\"abstract\"",
            "is-a=\"other\"",
            "linkage",
        ),
        ("entry.sch", "urn:authored:nodes", "urn:drift", "namespace"),
        (
            "abstract.sch",
            "$Agreement ",
            "$Other ",
            "parameter linkage",
        ),
        ("abstract.sch", "$CII-SR-282", "$Other", "parameter linkage"),
        (
            "abstract.sch",
            "Authored buyer diagnostic.",
            "Changed diagnostic.",
            "message/flag",
        ),
        (
            "abstract.sch",
            "flag=\"warning\"",
            "flag=\"fatal\"",
            "message/flag",
        ),
        (
            "abstract.sch",
            "Authored buyer diagnostic.",
            "Authored <name/> diagnostic.",
            "dynamic abstract message",
        ),
        (
            "concrete.sch",
            "/t:root/t:agreement",
            "/t:root/t:elsewhere",
            "AST guard",
        ),
    ] {
        let mut f = Fixture::new();
        f.texts
            .insert(member.into(), f.texts[member].replace(from, to));
        f.repin();
        f.manifest.nodes = guards(&f.texts);
        f.assert_error(expected);
    }
    let mut f = Fixture::new();
    f.raw.package.schemas[0]
        .namespaces
        .insert("t".into(), "urn:drift".into());
    f.update_raw_digest();
    f.assert_error("raw static namespace");
    let mut f = Fixture::new();
    f.manifest.policy_sha256 = "0".repeat(64);
    f.assert_error("policy drift");
    let mut f = Fixture::new();
    f.raw.package.schemas[0].patterns[0].rules[0].sites[0].effective_severity = Severity::Error;
    f.update_raw_digest();
    f.assert_error("severity drift");
    let mut f = Fixture::new();
    f.raw.package.schemas[0].patterns[0].rules[0].context.tree = Expr::Context;
    f.update_raw_digest();
    f.assert_error("context expression drift");
}
#[test]
fn seller_selection_and_source_ast_pairing_cannot_pass_compiler_boundary() {
    let mut f = Fixture::new();
    f.manifest
        .nodes
        .insert("buyer".into(), f.manifest.nodes["seller"].clone());
    f.assert_error("duplicate set");
    let f = Fixture::new();
    let p = f.prepare().unwrap();
    let c = materialize(&f.raw.package, &f.manifest, &p).unwrap();
    for change in 0..13 {
        let mut bad = c.clone();
        match change {
            0 => {
                bad.effective_package.schemas[0].patterns[0].rules[0].sites[0]
                    .test
                    .tree = Expr::Context
            }
            1 => {
                bad.resolutions[0].effective.source =
                    bad.resolutions[0].duplicates[1].expression.source.clone()
            }
            2 => {
                bad.resolutions[0].effective.tree =
                    bad.resolutions[0].original_site.test.tree.clone()
            }
            3 => {
                bad.resolutions[0].duplicates.pop();
            }
            4 => bad.resolutions[0].selected = bad.resolutions[0].duplicates[1].origin.clone(),
            5 => bad.raw_ir_sha256 = "0".repeat(64),
            6 => {
                bad.resolutions[0]
                    .lineage
                    .get_mut("abstract_site")
                    .unwrap()
                    .node
                    .sha256 = "0".repeat(64)
            }
            7 => {
                bad.effective_package.schemas[0].patterns[0].rules[0].sites[1]
                    .test
                    .tree = Expr::Context
            }
            8 => bad.effective_package.schemas[0].helpers[0].body.clear(),
            9 => {
                bad.effective_package.policy = Some(Policy {
                    scenario: "unexplained".into(),
                    namespaces: BTreeMap::new(),
                    selection: p.provenance.effective.clone(),
                    acceptance: p.provenance.effective.clone(),
                    clauses: Vec::new(),
                    execution: BackendState::Unavailable,
                })
            }
            10 => {
                bad.effective_package.schemas[0].patterns[0].rules[0]
                    .context
                    .tree = Expr::Context
            }
            11 => bad.effective_package.schemas[0].patterns[0].rules[0].sites[0]
                .message
                .clear(),
            _ => {
                bad.added_resources.clear();
            }
        }
        // Even recomputing an outer JSON hash cannot make the pairing compiler-owned.
        let bytes = emit::canonical(&bad).unwrap();
        assert_ne!(
            source::hash(&bytes),
            source::hash(&emit::canonical(&c).unwrap())
        );
        let decoded = serde_json::from_slice::<ResolvedCandidate>(&bytes).unwrap();
        assert!(
            validate_candidate(&f.raw.package, &f.manifest, &p, &decoded).is_err(),
            "accepted mutation {change}"
        );
    }
}
/// Full positive path is mandatory in generate.py/CI after explicit archive acquisition.
/// Offline unit tests deliberately need no local archive cache.
#[test]
#[ignore = "requires the five explicitly acquired pinned archives; generate.py runs this test"]
fn pinned_source_candidate_roundtrip() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let archives = std::env::var_os("VALIDATION_GENERATOR_ARCHIVES")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join(".cache/archives"));
    let paths = [
        "cen.zip",
        "xr.zip",
        "config.zip",
        "validator.jar",
        "cen-source.zip",
    ]
    .into_iter()
    .map(|n| (n.to_owned(), archives.join(n)))
    .collect();
    let sources = source::Sources::load(&paths).unwrap();
    let compiled = compile_candidate(&sources).unwrap();
    assert_eq!(
        digest(&compiled.raw.package).unwrap(),
        "8882e1e0550e0f45db2aa6e55ec28de0cc120b51983bba79f2bb175c5b02bed6"
    );
    assert_eq!(compiled.raw.package.resources.len(), 31);
    let bytes = emit::canonical(compiled.candidate()).unwrap();
    let decoded: ResolvedCandidate = serde_json::from_slice(&bytes).unwrap();
    let manifest = load_manifest(MANIFEST).unwrap();
    let prepared = prepare(&sources, &compiled.raw, &manifest).unwrap();
    validate_candidate(&compiled.raw.package, &manifest, &prepared, &decoded).unwrap();
    assert!(decoded.resolutions[0]
        .effective
        .text
        .contains("BuyerTaxRepresentativeTradeParty"));
    assert!(decoded.resolutions[0]
        .original_site
        .test
        .text
        .contains("SellerTaxRepresentativeTradeParty/ram:ID"));
    for g in decoded.resolutions[0].lineage.values() {
        let (archive, member) = g.node.source.resource.split_once('/').unwrap();
        let text = std::str::from_utf8(sources.bytes(archive, member).unwrap()).unwrap();
        let doc = source::xml(text).unwrap();
        select(&doc, g).unwrap();
    }
}

#[test]
fn refreshed_source_pins_do_not_authorize_wrong_values_or_ambiguous_linkage() {
    // Preserve the complete pair and update the changed site hash; selecting the
    // seller value through the reviewed buyer site must still fail the AST guard.
    let mut f = Fixture::new();
    f.texts.insert(
        "concrete.sch".into(),
        CONCRETE.replace("not(t:buyer)", "not(t:seller/t:id)"),
    );
    f.repin();
    let doc = source::xml(&f.texts["concrete.sch"]).unwrap();
    let params: Vec<_> = doc
        .root_element()
        .children()
        .filter(|n| n.is_element() && n.attribute("name") == Some("CII-SR-282"))
        .collect();
    let mut fresh = guards(&f.texts);
    fresh.insert("buyer".into(), guard(params[0], "fixture.zip/concrete.sch"));
    fresh.insert(
        "seller".into(),
        guard(params[1], "fixture.zip/concrete.sch"),
    );
    f.manifest.nodes = fresh;
    f.assert_error("AST guard");
    for (member,from,to,expected) in [
        ("entry.sch","<include href=\"concrete.sch\"/>","<include href=\"concrete.sch\"/><include href=\"concrete.sch\"/>","duplicate include"),
        ("entry.sch","<ns prefix=\"t\" uri=\"urn:authored:nodes\"/>","<ns prefix=\"t\" uri=\"urn:authored:nodes\"/><ns prefix=\"t\" uri=\"urn:authored:nodes\"/>","duplicate namespace"),
        ("concrete.sch","<param name=\"Agreement\" value=\"/t:root/t:agreement\"/>","<param name=\"Agreement\" value=\"/t:root/t:agreement\"/><param name=\"Agreement\" value=\"/t:root/t:agreement\"/>","unreviewed duplicate binding"),
        ("abstract.sch","<assert id=\"Other\"","<assert id=\"CII-SR-282\"","ambiguous abstract assertion"),
    ] {
        let mut f=Fixture::new(); f.texts.insert(member.into(),f.texts[member].replace(from,to)); f.repin();
        let fresh=guards(&f.texts);
        // Keep the originally bound assertion when another same-ID node is added.
        for (key,g) in fresh { if key != "abstract_site" || member != "abstract.sch" { f.manifest.nodes.insert(key,g); } }
        if member=="abstract.sch" {
            let doc=source::xml(&f.texts[member]).unwrap();
            let n=doc.descendants().find(|n| n.attribute("id")==Some("CII-SR-282")).unwrap();
            f.manifest.nodes.insert("abstract_site".into(),guard(n,"fixture.zip/abstract.sch"));
        }
        f.assert_error(expected);
    }
}

#[test]
fn moved_assertion_rebound_parent_and_lexical_context_drift_fail() {
    let mut f = Fixture::new();
    let previous = f.manifest.nodes["abstract_site"].ancestors.clone();
    let assertion = "<assert id=\"CII-SR-282\" flag=\"warning\" test=\"$CII-SR-282\">Authored buyer diagnostic.</assert>";
    f.texts.insert(
        "abstract.sch".into(),
        ABSTRACT.replace(assertion, "").replace(
            "</pattern>",
            &format!("<rule context=\"$Agreement \">{assertion}</rule></pattern>"),
        ),
    );
    f.repin();
    // Refresh member, moved node range/hash and structural site, retaining the
    // expected original ancestry. The deeper parent guard must reject it.
    f.manifest.nodes = guards(&f.texts);
    f.manifest.nodes.get_mut("abstract_site").unwrap().ancestors = previous;
    f.assert_error("parent/ancestry");

    let mut f = Fixture::new();
    f.raw.package.schemas[0].patterns[0].rules[0]
        .context
        .text
        .push(' ');
    f.update_raw_digest();
    f.assert_error("raw context expression drift");

    let mut f = Fixture::new();
    let expression = f.raw.package.schemas[0].patterns[0].rules[0]
        .context
        .clone();
    f.raw.package.policy = Some(Policy {
        scenario: "changed".into(),
        namespaces: BTreeMap::new(),
        selection: expression.clone(),
        acceptance: expression,
        clauses: Vec::new(),
        execution: BackendState::Unavailable,
    });
    f.update_raw_digest();
    f.assert_error("policy drift");
}
