//! Fixed compiler-owned source selection. This bounded linkage reader is not an
//! abstract Schematron frontend, an XSLT frontend, or a runtime admission verifier.
use crate::{bundle, emit, schematron, source, xpath, Error, Result};
use roxmltree::{Document, Node};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use validation_ir::{resolved::*, *};

pub(crate) const MANIFEST: &str = include_str!("../source-resolution.lock.json");
const DECISION: &str = "cii-sr-282-original-buyer-param";
const NODE_KEYS: [&str; 16] = [
    "abstract_include",
    "abstract_pattern",
    "abstract_rule",
    "abstract_site",
    "buyer",
    "concrete_include",
    "concrete_pattern",
    "configured_comparison",
    "configured_condition",
    "context_param",
    "entry",
    "raw_rule",
    "raw_site",
    "release_comparison",
    "scenario",
    "seller",
];
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResourcePin {
    archive_sha256: String,
    sha256: String,
    size: u64,
    role: ResourceRole,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Decision {
    id: String,
    kind: DecisionKind,
    authority: String,
    reason: String,
    parameter: String,
    context_parameter: String,
    selected: String,
    duplicates: Vec<String>,
    original_ast_sha256: String,
    effective_ast_sha256: String,
    context_ast_sha256: String,
    message_sha256: String,
    flag: String,
    base_severity: Severity,
    effective_severity: Severity,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
enum DecisionKind {
    OriginalDeclarativeParameter,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: String,
    target: String,
    source_package: String,
    raw_ir_sha256: String,
    resources: BTreeMap<String, ResourcePin>,
    nodes: BTreeMap<String, SourceNode>,
    namespaces: BTreeMap<String, String>,
    policy_sha256: String,
    decisions: Vec<Decision>,
}

// Serde structs reject duplicate fields; also reject duplicate keys in nested maps
// before typed decoding, rather than accepting JSON map last-wins behavior.
fn load_manifest(text: &str) -> Result<Manifest> {
    struct Unique;
    impl<'de> serde::Deserialize<'de> for Unique {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
            struct Visitor;
            impl<'de> serde::de::Visitor<'de> for Visitor {
                type Value = Unique;
                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("JSON with unique object keys")
                }
                fn visit_map<A: serde::de::MapAccess<'de>>(
                    self,
                    mut map: A,
                ) -> std::result::Result<Unique, A::Error> {
                    let mut seen = BTreeSet::new();
                    while let Some(key) = map.next_key::<String>()? {
                        if !seen.insert(key) {
                            return Err(serde::de::Error::custom("duplicate manifest field/key"));
                        }
                        map.next_value::<Unique>()?;
                    }
                    Ok(Unique)
                }
                fn visit_seq<A: serde::de::SeqAccess<'de>>(
                    self,
                    mut seq: A,
                ) -> std::result::Result<Unique, A::Error> {
                    while seq.next_element::<Unique>()?.is_some() {}
                    Ok(Unique)
                }
                fn visit_str<E: serde::de::Error>(self, _: &str) -> std::result::Result<Unique, E> {
                    Ok(Unique)
                }
                fn visit_bool<E: serde::de::Error>(
                    self,
                    _: bool,
                ) -> std::result::Result<Unique, E> {
                    Ok(Unique)
                }
                fn visit_u64<E: serde::de::Error>(self, _: u64) -> std::result::Result<Unique, E> {
                    Ok(Unique)
                }
                fn visit_i64<E: serde::de::Error>(self, _: i64) -> std::result::Result<Unique, E> {
                    Ok(Unique)
                }
                fn visit_f64<E: serde::de::Error>(self, _: f64) -> std::result::Result<Unique, E> {
                    Ok(Unique)
                }
                fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Unique, E> {
                    Ok(Unique)
                }
            }
            d.deserialize_any(Visitor)
        }
    }
    let _: Unique = serde_json::from_str(text)?;
    Ok(serde_json::from_str(text)?)
}

/// Only the fixed compiler can construct this value; serde candidate data cannot.
pub struct CandidateInspection {
    pub(crate) raw: bundle::Inspection,
    pub(crate) candidate: ResolvedCandidate,
    pub(crate) extra_bytes: BTreeMap<String, Vec<u8>>,
}
impl CandidateInspection {
    pub fn candidate(&self) -> &ResolvedCandidate {
        &self.candidate
    }
}
/// Compile the exact pinned raw package and fixed source-origin decision.
/// No caller-provided raw package, manifest, selector or expression is accepted.
pub fn compile_candidate(sources: &source::Sources) -> Result<CandidateInspection> {
    let raw = bundle::compile(sources)?;
    let manifest = load_manifest(MANIFEST)?;
    let prepared = prepare(sources, &raw, &manifest)?;
    let candidate = materialize(&raw.package, &manifest, &prepared)?;
    validate_candidate(&raw.package, &manifest, &prepared, &candidate)?;
    Ok(CandidateInspection {
        raw,
        candidate,
        extra_bytes: prepared.extra_bytes,
    })
}
fn require(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(Error::new(format!("source resolution: {message}")))
    }
}
fn digest<T: serde::Serialize>(value: &T) -> Result<String> {
    Ok(source::hash(&emit::canonical(value)?))
}
fn attr<'a>(node: Node<'a, '_>, name: &str) -> Result<&'a str> {
    node.attribute(name)
        .ok_or_else(|| Error::new(format!("source resolution: missing {name}")))
}
fn origin(node: Node<'_, '_>, resource: &str) -> NodeOrigin {
    let text = node.document().input_text();
    let attributes = node
        .attributes()
        .map(|a| {
            let range = a.range();
            (
                a.name().to_owned(),
                AttributeOrigin {
                    source: source::location(node, resource, Some(a.name())),
                    sha256: source::hash(&text.as_bytes()[range]),
                    decoded_sha256: source::hash(a.value().as_bytes()),
                },
            )
        })
        .collect();
    NodeOrigin {
        source: source::location(node, resource, None),
        sha256: source::hash(&text.as_bytes()[node.range()]),
        attributes,
    }
}
/// Select structurally first; then check byte identity, every attribute, and the full ancestry.
fn select<'a>(doc: &'a Document<'a>, guard: &SourceNode) -> Result<Node<'a, 'a>> {
    let mut parents = vec![doc.root()];
    for step in &guard.selector {
        let mut matches = Vec::new();
        for parent in parents {
            for (index, node) in parent
                .children()
                .filter(|n| {
                    n.is_element()
                        && n.tag_name().namespace() == Some(step.namespace.as_str())
                        && n.tag_name().name() == step.local
                })
                .enumerate()
            {
                if index == step.sibling
                    && step
                        .identity
                        .iter()
                        .all(|(k, v)| node.attribute(k.as_str()) == Some(v.as_str()))
                {
                    matches.push(node);
                }
            }
        }
        require(matches.len() == 1, "missing/ambiguous structural selector")?;
        parents = matches;
    }
    require(!guard.selector.is_empty(), "empty selector")?;
    let node = parents[0];
    let resource = &guard.node.source.resource;
    require(
        origin(node, resource) == guard.node,
        "node range/hash/attribute drift",
    )?;
    let mut ancestors: Vec<_> = node
        .ancestors()
        .skip(1)
        .filter(Node::is_element)
        .map(|n| origin(n, resource))
        .collect();
    ancestors.reverse();
    require(ancestors == guard.ancestors, "parent/ancestry drift")?;
    Ok(node)
}
fn expression(
    node: Node<'_, '_>,
    resource: &str,
    context: &xpath::Context,
    pattern: bool,
) -> Result<Expression> {
    schematron::check(node, schematron::SCH, "param", &["name", "value"])?;
    require(
        !node
            .children()
            .any(|n| n.is_element() || n.is_text() && !n.text().unwrap_or("").trim().is_empty()),
        "nonempty parameter",
    )?;
    let text = attr(node, "value")?.to_owned();
    let source = source::location(node, resource, Some("value"));
    let mut tree = if pattern {
        xpath::parse_pattern(&text, context)
    } else {
        xpath::parse(&text, context)
    }
    .map_err(|e| e.at(&source))?;
    xpath::resolve(&mut tree, &BTreeMap::new(), resource)?;
    Ok(Expression { text, source, tree })
}
fn reference(
    node: Node<'_, '_>,
    attribute: &str,
    name: &str,
    context: &xpath::Context,
) -> Result<()> {
    let tree = xpath::parse(attr(node, attribute)?, context)?;
    require(
        matches!(tree, Expr::Variable { name: n, .. } if n.qualified == name && n.namespace.is_empty()),
        "abstract parameter linkage drift",
    )
}
struct Prepared {
    provenance: ResolutionProvenance,
    resources: BTreeMap<String, Resource>,
    edges: Vec<Edge>,
    extra_bytes: BTreeMap<String, Vec<u8>>,
}
fn prepare(
    sources: &source::Sources,
    raw: &bundle::Inspection,
    manifest: &Manifest,
) -> Result<Prepared> {
    require(
        manifest.format == "einvoice-source-resolution/1",
        "unknown manifest format",
    )?;
    require(
        manifest.decisions.len() == 1 && manifest.decisions[0].id == DECISION,
        "unknown/unused/duplicate decision",
    )?;
    let decision = &manifest.decisions[0];
    match decision.kind {
        DecisionKind::OriginalDeclarativeParameter => (),
    }
    require(
        decision.selected == "buyer" && decision.duplicates == ["buyer", "seller"],
        "unsupported selected occurrence/duplicate decision set",
    )?;
    require(
        !decision.authority.is_empty() && !decision.reason.is_empty(),
        "missing decision authority/reason",
    )?;
    require(
        manifest.nodes.keys().cloned().collect::<BTreeSet<_>>()
            == NODE_KEYS.iter().map(|k| (*k).to_owned()).collect(),
        "unknown/unused/missing node guard",
    )?;
    require(
        raw.package.target == manifest.target
            && raw.package.source_package == manifest.source_package
            && raw.package.format == validation_ir::FORMAT
            && digest(&raw.package)? == manifest.raw_ir_sha256,
        "raw identity/digest drift",
    )?;
    require(
        digest(&raw.package.policy)? == manifest.policy_sha256,
        "policy drift",
    )?;
    let mut texts = BTreeMap::new();
    let mut resources = BTreeMap::new();
    let mut extra_bytes = BTreeMap::new();
    for (id, pin) in &manifest.resources {
        let (archive, member) = id
            .split_once('/')
            .ok_or_else(|| Error::new("invalid resolution resource"))?;
        let a = sources
            .archives
            .get(archive)
            .ok_or_else(|| Error::new(format!("resource {id}: missing resolution archive")))?;
        require(
            a.pin.sha256 == pin.archive_sha256,
            &format!("resource {id}: archive identity drift"),
        )?;
        let bytes = sources
            .bytes(archive, member)
            .map_err(|e| Error::new(format!("resource {id}: {e}")))?;
        require(
            bytes.len() as u64 == pin.size && source::hash(bytes) == pin.sha256,
            &format!("resource {id}: member hash/size drift"),
        )?;
        texts.insert(
            id.clone(),
            std::str::from_utf8(bytes)
                .map_err(|_| Error::new(format!("resource {id}: resolution source UTF-8")))?,
        );
        if !raw.package.resources.contains_key(id) {
            resources.insert(
                id.clone(),
                Resource {
                    archive: archive.into(),
                    member: member.into(),
                    sha256: pin.sha256.clone(),
                    size: pin.size,
                    role: pin.role.clone(),
                    pin: "oracle-member-lock; fixed source-resolution.lock.json".into(),
                },
            );
            extra_bytes.insert(id.clone(), bytes.to_vec());
        } else {
            let resource = &raw.package.resources[id];
            require(
                resource.sha256 == pin.sha256 && resource.role == pin.role,
                &format!("resource {id}: retained resource identity/role drift"),
            )?;
        }
    }
    require(
        manifest.resources.keys().all(|r| {
            manifest
                .nodes
                .values()
                .any(|n| &n.node.source.resource == r)
        }),
        "unused resource pin",
    )?;
    let docs: BTreeMap<_, _> = texts
        .iter()
        .map(|(id, text)| {
            source::xml(text)
                .map(|doc| (id.clone(), doc))
                .map_err(|e| Error::new(format!("resource {id}: {e}")))
        })
        .collect::<Result<_>>()?;
    let mut nodes = BTreeMap::new();
    for (key, guard) in &manifest.nodes {
        let doc = docs.get(&guard.node.source.resource).ok_or_else(|| {
            Error::new(format!("guard {key}: unbound node resource")).at(&guard.node.source)
        })?;
        nodes.insert(
            key.as_str(),
            select(doc, guard)
                .map_err(|e| Error::new(format!("guard {key}: {e}")).at(&guard.node.source))?,
        );
    }
    let entry = nodes["entry"];
    schematron::check(entry, schematron::SCH, "schema", &["queryBinding"])?;
    require(
        entry.attribute("queryBinding") == Some("xslt2"),
        "entry query binding drift",
    )?;
    let mut context = xpath::Context::xslt2();
    let mut declared = BTreeSet::new();
    for node in entry
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "ns")
    {
        schematron::check(node, schematron::SCH, "ns", &["prefix", "uri"])?;
        let prefix = attr(node, "prefix")?;
        require(declared.insert(prefix), "duplicate namespace declaration")?;
        context
            .namespaces
            .insert(prefix.into(), attr(node, "uri")?.into());
    }
    require(
        context.namespaces == manifest.namespaces,
        "entry namespace drift",
    )?;
    let concrete = nodes["concrete_pattern"];
    let abstract_pattern = nodes["abstract_pattern"];
    schematron::check(concrete, schematron::SCH, "pattern", &["id", "is-a"])?;
    schematron::check(
        abstract_pattern,
        schematron::SCH,
        "pattern",
        &["id", "abstract"],
    )?;
    require(
        attr(concrete, "is-a")? == attr(abstract_pattern, "id")?
            && abstract_pattern.attribute("abstract") == Some("true"),
        "abstract/concrete linkage drift",
    )?;
    let mut edges = Vec::new();
    for (key, target) in [
        ("abstract_include", "abstract_pattern"),
        ("concrete_include", "concrete_pattern"),
    ] {
        let include = nodes[key];
        schematron::check(include, schematron::SCH, "include", &["href"])?;
        require(include.parent() == Some(entry), "include parent drift")?;
        let href = attr(include, "href")?;
        require(
            entry
                .children()
                .filter(|n| {
                    n.is_element()
                        && n.tag_name().name() == "include"
                        && n.attribute("href") == Some(href)
                })
                .count()
                == 1,
            "duplicate include",
        )?;
        let from = manifest.nodes[key].node.source.resource.clone();
        let to = source::relative(&from, href, &texts.keys().cloned().collect())?;
        require(
            to == manifest.nodes[target].node.source.resource,
            "include linkage drift",
        )?;
        edges.push(Edge {
            from,
            to,
            relation: "bounded-declarative-include".into(),
            namespace: None,
        });
    }
    let abstract_site = nodes["abstract_site"];
    let abstract_rule = nodes["abstract_rule"];
    schematron::check(
        abstract_site,
        schematron::SCH,
        "assert",
        &["id", "flag", "test"],
    )?;
    schematron::check(abstract_rule, schematron::SCH, "rule", &["context"])?;
    require(
        abstract_site.parent() == Some(abstract_rule)
            && abstract_rule.parent() == Some(abstract_pattern),
        "abstract assertion parent drift",
    )?;
    require(
        abstract_pattern
            .descendants()
            .filter(|n| n.is_element() && n.attribute("id") == Some(decision.parameter.as_str()))
            .count()
            == 1,
        "missing/ambiguous abstract assertion",
    )?;
    reference(abstract_site, "test", &decision.parameter, &context)?;
    reference(
        abstract_rule,
        "context",
        &decision.context_parameter,
        &context,
    )?;
    require(
        !abstract_site.children().any(|n| n.is_element()),
        "dynamic abstract message",
    )?;
    let message: String = abstract_site
        .children()
        .filter(Node::is_text)
        .filter_map(|n| n.text())
        .collect();
    require(
        source::hash(message.as_bytes()) == decision.message_sha256
            && abstract_site.attribute("flag") == Some(decision.flag.as_str()),
        "abstract message/flag drift",
    )?;
    // Inspect the entire direct parameter set: no default policy for other duplicates.
    let mut groups: BTreeMap<&str, Vec<Node<'_, '_>>> = BTreeMap::new();
    for node in concrete.children().filter(Node::is_element) {
        schematron::check(node, schematron::SCH, "param", &["name", "value"])?;
        groups.entry(attr(node, "name")?).or_default().push(node);
    }
    require(
        groups
            .iter()
            .all(|(name, set)| set.len() == 1 || *name == decision.parameter),
        "unreviewed duplicate binding",
    )?;
    let actual = groups
        .get(decision.parameter.as_str())
        .ok_or_else(|| Error::new("missing duplicate set"))?;
    let expected: Vec<_> = decision
        .duplicates
        .iter()
        .map(|key| nodes[key.as_str()])
        .collect();
    require(*actual == expected, "missing/extra/reordered duplicate set")?;
    let cp = groups
        .get(decision.context_parameter.as_str())
        .ok_or_else(|| Error::new("missing context parameter"))?;
    require(
        cp.as_slice() == [nodes["context_param"]],
        "missing/ambiguous context parameter",
    )?;
    let context_expression = expression(
        cp[0],
        &manifest.nodes["context_param"].node.source.resource,
        &context,
        true,
    )?;
    let duplicates: Vec<_> = decision
        .duplicates
        .iter()
        .map(|key| {
            Ok(ParameterOccurrence {
                origin: manifest.nodes[key].clone(),
                expression: expression(
                    nodes[key.as_str()],
                    &manifest.nodes[key].node.source.resource,
                    &context,
                    false,
                )?,
            })
        })
        .collect::<Result<_>>()?;
    let selected = duplicates
        .iter()
        .find(|p| p.origin == manifest.nodes[&decision.selected])
        .ok_or_else(|| Error::new("unused decision"))?;
    require(
        digest(&selected.expression.tree)? == decision.effective_ast_sha256
            && digest(&duplicates[1].expression.tree)? == decision.original_ast_sha256
            && selected.expression.tree != duplicates[1].expression.tree
            && digest(&context_expression.tree)? == decision.context_ast_sha256,
        "selected/original/context AST guard drift",
    )?;
    let raw_node = nodes["raw_site"];
    let raw_rule_node = nodes["raw_rule"];
    require(raw_node.parent() == Some(raw_rule_node), "raw parent drift")?;
    let matches: Vec<_> = raw
        .package
        .schemas
        .iter()
        .flat_map(|schema| {
            schema.patterns.iter().flat_map(move |p| {
                p.rules.iter().flat_map(move |r| {
                    r.sites
                        .iter()
                        .filter(move |s| s.id == decision.parameter)
                        .map(move |s| (schema, p, r, s))
                })
            })
        })
        .collect();
    require(matches.len() == 1, "missing/ambiguous raw assertion")?;
    let (schema, pattern, rule, site) = matches[0];
    require(
        schema.namespaces == context.namespaces
            && schema.xpath_element_default_namespace.is_empty()
            && schema.xpath_function_default_namespace == xpath::FN,
        "raw static namespace drift",
    )?;
    require(
        pattern.id == attr(concrete, "id")?
            && pattern.active
            && rule.source
                == source::location(
                    raw_rule_node,
                    &manifest.nodes["raw_rule"].node.source.resource,
                    None,
                )
            && rule.identity == format!("{}@{}", rule.source.resource, rule.source.xml_byte_start)
            && rule.flag.is_none()
            && rule.bindings.is_empty(),
        "raw context/dispatch drift",
    )?;
    require(
        site.source
            == source::location(
                raw_node,
                &manifest.nodes["raw_site"].node.source.resource,
                None,
            )
            && site.identity == format!("{}@{}", site.source.resource, site.source.xml_byte_start)
            && site.test.source == source::location(raw_node, &site.source.resource, Some("test"))
            && site.test.text == attr(raw_node, "test")?
            && site.test.tree == duplicates[1].expression.tree
            && site.message == vec![MessagePart::Text { text: message }]
            && site.raw_flag == decision.flag
            && site.base_severity == decision.base_severity
            && site.effective_severity == decision.effective_severity,
        "raw predicate/message/flag/severity drift",
    )?;
    require(
        rule.context.source
            == source::location(raw_rule_node, &rule.source.resource, Some("context"))
            && rule.context.text == attr(raw_rule_node, "context")?
            && rule.context.text == context_expression.text
            && rule.context.tree == context_expression.tree,
        "raw context expression drift",
    )?;
    edges.push(Edge {
        from: manifest.nodes["concrete_pattern"]
            .node
            .source
            .resource
            .clone(),
        to: manifest.nodes["abstract_pattern"]
            .node
            .source
            .resource
            .clone(),
        relation: "bounded-is-a-parameter-linkage".into(),
        namespace: Some(schematron::SCH.into()),
    });
    Ok(Prepared {
        provenance: ResolutionProvenance {
            decision: decision.id.clone(),
            authority: decision.authority.clone(),
            reason: decision.reason.clone(),
            original_site: site.clone(),
            original_context: rule.context.clone(),
            effective: selected.expression.clone(),
            context_parameter: ParameterOccurrence {
                origin: manifest.nodes["context_param"].clone(),
                expression: context_expression,
            },
            duplicates,
            selected: manifest.nodes[&decision.selected].clone(),
            lineage: manifest.nodes.clone(),
            namespaces: context.namespaces,
            policy_sha256: manifest.policy_sha256.clone(),
        },
        resources,
        edges,
        extra_bytes,
    })
}
fn effective_package(raw: &Package, prepared: &Prepared) -> Result<Package> {
    let mut effective = raw.clone();
    let mut consumed = 0;
    for site in effective
        .schemas
        .iter_mut()
        .flat_map(|s| &mut s.patterns)
        .flat_map(|p| &mut p.rules)
        .flat_map(|r| &mut r.sites)
    {
        if site.identity == prepared.provenance.original_site.identity {
            require(
                *site == prepared.provenance.original_site,
                "original site/provenance mismatch",
            )?;
            site.test = prepared.provenance.effective.clone();
            consumed += 1;
        }
    }
    require(consumed == 1, "unused/duplicate applied decision")?;
    for (id, resource) in &prepared.resources {
        require(
            effective
                .resources
                .insert(id.clone(), resource.clone())
                .is_none(),
            "added resource collision",
        )?;
    }
    effective.source_graph.extend(prepared.edges.clone());
    effective
        .requirements
        .extend(bundle::requirements(&effective)?);
    Ok(effective)
}
fn materialize(
    raw: &Package,
    manifest: &Manifest,
    prepared: &Prepared,
) -> Result<ResolvedCandidate> {
    Ok(ResolvedCandidate {
        format: validation_ir::resolved::FORMAT.into(),
        target: raw.target.clone(),
        source_package: raw.source_package.clone(),
        raw_ir_format: raw.format.clone(),
        raw_ir_sha256: manifest.raw_ir_sha256.clone(),
        decision_manifest_sha256: source::hash(MANIFEST.as_bytes()),
        effective_package: effective_package(raw, prepared)?,
        resolutions: vec![prepared.provenance.clone()],
        added_resources: prepared.resources.clone(),
        added_source_graph: prepared.edges.clone(),
        correspondence_audit:
            CorrespondenceAudit::PendingFullPredicateContextHelperMessagePolicyCorrespondence,
        backend: BackendState::Unavailable,
    })
}
/// Private compiler boundary, using source-derived expectations, never self-reported
/// candidate lineage. Full equality covers helpers, policy, messages, contexts and ASTs.
fn validate_candidate(
    raw: &Package,
    manifest: &Manifest,
    prepared: &Prepared,
    candidate: &ResolvedCandidate,
) -> Result<()> {
    require(
        candidate.format == validation_ir::resolved::FORMAT
            && candidate.target == raw.target
            && candidate.source_package == raw.source_package
            && candidate.raw_ir_format == raw.format
            && candidate.raw_ir_sha256 == manifest.raw_ir_sha256
            && candidate.decision_manifest_sha256 == source::hash(MANIFEST.as_bytes())
            && candidate.resolutions == [prepared.provenance.clone()]
            && candidate.added_resources == prepared.resources
            && candidate.added_source_graph == prepared.edges
            && candidate.correspondence_audit
                == CorrespondenceAudit::PendingFullPredicateContextHelperMessagePolicyCorrespondence
            && candidate.backend == BackendState::Unavailable,
        "unexplained candidate source/provenance/envelope delta",
    )?;
    // Independently reverse only the permitted changes, instead of trusting a
    // second invocation of the materializer to validate its own implementation.
    let mut normalized = candidate.effective_package.clone();
    let mut consumed = 0;
    for site in normalized
        .schemas
        .iter_mut()
        .flat_map(|s| &mut s.patterns)
        .flat_map(|p| &mut p.rules)
        .flat_map(|r| &mut r.sites)
    {
        if site.identity == prepared.provenance.original_site.identity {
            require(
                site.test == prepared.provenance.effective,
                "effective AST/source pairing drift",
            )?;
            site.test = prepared.provenance.original_site.test.clone();
            consumed += 1;
        }
    }
    require(consumed == 1, "missing/duplicate effective decision site")?;
    for (id, resource) in &prepared.resources {
        require(
            normalized.resources.remove(id).as_ref() == Some(resource),
            "added source closure drift",
        )?;
    }
    let tail = normalized
        .source_graph
        .strip_prefix(raw.source_graph.as_slice());
    require(
        tail == Some(prepared.edges.as_slice()),
        "added source graph drift",
    )?;
    normalized.source_graph = raw.source_graph.clone();
    normalized.requirements = raw.requirements.clone();
    require(
        normalized == *raw,
        "unexplained effective predicate/context/helper/message/policy/package delta",
    )?;
    let mut requirements = raw.requirements.clone();
    requirements.extend(bundle::requirements(&candidate.effective_package)?);
    require(
        candidate.effective_package.requirements == requirements,
        "unexplained effective requirements delta",
    )
}

#[cfg(test)]
mod tests;
