//! One exact target, with separate retained-resource and mapped-policy coverage.
use crate::{
    schematron,
    source::{self, Sources},
    xpath, Error, Result,
};
use std::collections::{BTreeMap, BTreeSet};
use validation_ir::*;
pub const TARGET: &str = "xrechnung-3.0.2-cii-d16b-kosit-2026-08-31";
pub const CEN: &str = "schematron/preprocessed/EN16931-CII-validation-preprocessed.sch";
pub const XR: &str = "schematron/cii/XRechnung-CII-validation.sch";
pub const COMMON: &str = "schematron/common.sch";
const CONFIG_NS: &str = "http://www.xoev.de/de/validator/framework/1/scenarios";
#[derive(serde::Deserialize)]
struct Closure {
    format: u32,
    parent_archive: String,
    parent_sha256: String,
    members: BTreeMap<String, String>,
}
pub struct Inspection {
    pub(crate) package: Package,
    pub(crate) bytes: BTreeMap<String, Vec<u8>>,
    pub(crate) archives: BTreeMap<String, serde_json::Value>,
}
fn resource_id(a: &str, m: &str) -> String {
    format!("{a}/{m}")
}
impl Inspection {
    pub fn package(&self) -> &Package {
        &self.package
    }
    fn add(
        &mut self,
        s: &Sources,
        a: &str,
        m: &str,
        role: ResourceRole,
        pin: &str,
    ) -> Result<String> {
        source::safe_member(m)?;
        let data = s.bytes(a, m)?.to_vec();
        let id = resource_id(a, m);
        self.package.resources.insert(
            id.clone(),
            Resource {
                archive: a.into(),
                member: m.into(),
                sha256: source::hash(&data),
                size: data.len() as u64,
                role,
                pin: pin.into(),
            },
        );
        self.bytes.insert(id.clone(), data);
        Ok(id)
    }
    fn evidence(&self, id: &str, locator: &str) -> Evidence {
        Evidence {
            resource: id.into(),
            sha256: self.package.resources[id].sha256.clone(),
            locator: locator.into(),
        }
    }
}
pub fn compile(s: &Sources) -> Result<Inspection> {
    let mut out = Inspection {
        package: Package {
            format: FORMAT.into(),
            target: TARGET.into(),
            source_package: String::new(),
            backend: BackendState::Unavailable,
            schemas: Vec::new(),
            resources: BTreeMap::new(),
            source_graph: Vec::new(),
            policy: None,
            requirements: BTreeSet::new(),
        },
        bytes: BTreeMap::new(),
        archives: BTreeMap::new(),
    };
    // Whole archive and member inventories are recorded independently of the selected closure.
    for (name, a) in &s.archives {
        out.archives.insert(name.clone(),serde_json::json!({"version":a.pin.version,"sha256":a.pin.sha256,"inventory_pin":if a.pin.members.is_some(){"oracle-member-lock"}else{"archive-only; selected members additionally closure-pinned"},"verified_regular_members":a.members.iter().map(|(n,b)|(n.clone(),serde_json::json!({"sha256":source::hash(b),"size":b.len()}))).collect::<BTreeMap<_,_>>() }));
    }
    let mut identities = BTreeMap::new();
    for (n, a) in &s.archives {
        identities.insert(n, &a.pin.sha256);
    }
    out.package.source_package =
        format!("sha256:{}", source::hash(&serde_json::to_vec(&identities)?));
    let cen = out.add(
        s,
        "cen.zip",
        CEN,
        ResourceRole::Schematron,
        "oracle-member-lock",
    )?;
    let xr = out.add(
        s,
        "xr.zip",
        XR,
        ResourceRole::Schematron,
        "oracle-member-lock",
    )?;
    let common = out.add(
        s,
        "xr.zip",
        COMMON,
        ResourceRole::Schematron,
        "oracle-member-lock",
    )?;
    let mut resources = BTreeMap::new();
    for id in [&cen, &xr, &common] {
        resources.insert(
            id.clone(),
            String::from_utf8(out.bytes[id].clone())
                .map_err(|_| Error::new("non-UTF8 Schematron"))?,
        );
    }
    out.package
        .schemas
        .push(schematron::parse(&resources, &cen, None)?);
    out.package
        .schemas
        .push(schematron::parse(&resources, &xr, None)?);
    out.add(
        s,
        "cen.zip",
        "schematron/codelist/EN16931-CII-codes.sch",
        ResourceRole::CodelistTrace,
        "oracle-member-lock",
    )?;
    for (a, m) in [
        ("xr.zip", "LICENSE"),
        (
            "cen-source.zip",
            "eInvoicing-EN16931-validation-1.3.16/LICENSE.txt",
        ),
    ] {
        out.add(
            s,
            a,
            m,
            ResourceRole::Notice,
            "oracle-member-lock; original bytes",
        )?;
    }
    // Config ZIP lacks a full license member. Standalone notice is independently locked by oracle.
    let lock: serde_json::Value = serde_json::from_str(source::ORACLE_LOCK)?;
    let notice = include_bytes!("../../oracle/notices/KoSIT-configuration-LICENSE").to_vec();
    let pin = lock["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["file"] == "config-LICENSE")
        .ok_or_else(|| Error::new("missing config license pin"))?;
    if pin["sha256"] != source::hash(&notice) {
        return Err(Error::new("configuration license mismatch"));
    }
    let notice_id = "standalone/config-LICENSE".to_owned();
    out.package.resources.insert(
        notice_id.clone(),
        Resource {
            archive: "standalone".into(),
            member: "config-LICENSE".into(),
            sha256: source::hash(&notice),
            size: notice.len() as u64,
            role: ResourceRole::Notice,
            pin: "oracle standalone-file lock".into(),
        },
    );
    out.bytes.insert(notice_id, notice);
    for m in ["resources/cii/16b/xsd/CrossIndustryInvoice_100pD16B.xsd","resources/cii/16b/xsd/CrossIndustryInvoice_QualifiedDataType_100pD16B.xsd","resources/cii/16b/xsd/CrossIndustryInvoice_ReusableAggregateBusinessInformationEntity_100pD16B.xsd","resources/cii/16b/xsd/CrossIndustryInvoice_UnqualifiedDataType_100pD16B.xsd"] { out.add(s,"config.zip",m,ResourceRole::CiiSchema,"oracle-member-lock")?; }
    for (m, role) in [
        ("scenarios.xml", ResourceRole::PolicyMappedSource),
        ("resources/xrechnung-report.xsl", ResourceRole::OpaqueReport),
        ("resources/default-report.xsl", ResourceRole::OpaqueReport),
        ("resources/xsd/scenarios.xsd", ResourceRole::ReportSchema),
        ("resources/xsd/report.xsd", ResourceRole::ReportSchema),
    ] {
        out.add(s, "config.zip", m, role, "oracle-member-lock")?;
    }
    for m in [
        "resources/cii/16b/xsl/EN16931-CII-validation.xsl",
        "resources/xrechnung/3.0.2/xsl/XRechnung-CII-validation.xsl",
    ] {
        out.add(
            s,
            "config.zip",
            m,
            ResourceRole::OracleExecutableEvidence,
            "oracle-member-lock; retained reference only, never expression frontend",
        )?;
    }
    let closure: Closure = serde_json::from_str(include_str!("../jar-closure.lock.json"))?;
    if closure.format != 1
        || closure.parent_archive != "validator.jar"
        || s.archives["validator.jar"].pin.sha256 != closure.parent_sha256
    {
        return Err(Error::new("JAR closure parent mismatch"));
    }
    for (m, expected) in &closure.members {
        if source::hash(s.bytes("validator.jar", m)?) != *expected {
            return Err(Error::new("JAR closure member mismatch"));
        }
        let role = if m == "LICENSE.validator" || m == "META-INF/NOTICE" {
            ResourceRole::Notice
        } else if m.starts_with("xsd/") {
            ResourceRole::ReportSchema
        } else {
            ResourceRole::EngineEvidence
        };
        out.add(
            s,
            "validator.jar",
            m,
            role,
            "additive jar-closure.lock.json under oracle archive pin",
        )?;
    }
    source_graph(&mut out)?;
    let policy = policy(&out)?;
    let overrides = policy
        .clauses
        .iter()
        .find_map(|c| {
            if let PolicyClause::Overrides { values, .. } = c {
                Some(values)
            } else {
                None
            }
        })
        .unwrap();
    for schema in &mut out.package.schemas {
        for p in &mut schema.patterns {
            for r in &mut p.rules {
                for site in &mut r.sites {
                    if let Some(level) = overrides.get(&site.id) {
                        site.effective_severity = *level;
                    }
                }
            }
        }
    }
    out.package.policy = Some(policy);
    out.package.requirements = requirements(&out.package)?;
    for (id, resource) in &out.package.resources {
        if resource.role == ResourceRole::CiiSchema {
            let doc = source::xml(
                std::str::from_utf8(&out.bytes[id]).map_err(|_| Error::new("schema UTF-8"))?,
            )?;
            for node in doc.descendants().filter(|n| n.is_element()) {
                out.package.requirements.insert(Requirement {
                    category: "xsd-element".into(),
                    name: format!(
                        "{{{}}}{}",
                        node.tag_name().namespace().unwrap_or(""),
                        node.tag_name().name()
                    ),
                });
                for attr in node.attributes() {
                    out.package.requirements.insert(Requirement {
                        category: "xsd-attribute".into(),
                        name: format!("{{{}}}{}", attr.namespace().unwrap_or(""), attr.name()),
                    });
                }
            }
        }
    }
    Ok(out)
}
fn source_graph(out: &mut Inspection) -> Result<()> {
    let mut schemas = BTreeMap::new();
    for (id, res) in &out.package.resources {
        if matches!(
            res.role,
            ResourceRole::CiiSchema | ResourceRole::ReportSchema
        ) {
            schemas.insert(
                id.clone(),
                std::str::from_utf8(&out.bytes[id])
                    .map_err(|_| Error::new("schema encoding"))?
                    .to_owned(),
            );
        }
    }
    let mappings = BTreeMap::from([(
        (
            "config.zip/resources/xsd/report.xsd".into(),
            CONFIG_NS.into(),
        ),
        "config.zip/resources/xsd/scenarios.xsd".into(),
    )]);
    out.package
        .source_graph
        .extend(crate::schema::graph(&schemas, &mappings)?);
    let allowed: BTreeSet<_> = out.bytes.keys().cloned().collect();
    for schema in &out.package.schemas {
        for include in &schema.includes {
            out.package.source_graph.push(Edge {
                from: include.from.resource.clone(),
                to: include.to.clone(),
                relation: "schematron-include".into(),
                namespace: None,
            });
        }
    }
    for (id, res) in &out.package.resources {
        if !matches!(
            res.role,
            ResourceRole::CiiSchema | ResourceRole::ReportSchema | ResourceRole::OpaqueReport
        ) {
            continue;
        }
        let text =
            std::str::from_utf8(&out.bytes[id]).map_err(|_| Error::new("non-UTF8 XML resource"))?;
        let doc = source::xml(text)?;
        if matches!(
            res.role,
            ResourceRole::CiiSchema | ResourceRole::ReportSchema
        ) {
            continue;
        } else {
            for n in doc
                .descendants()
                .filter(|n| n.is_element() && n.tag_name().namespace() == Some(schematron::XSL))
            {
                if ["import", "include"].contains(&n.tag_name().name()) {
                    let to = source::relative(
                        id,
                        n.attribute("href")
                            .ok_or_else(|| Error::new("stylesheet import lacks href"))?,
                        &allowed,
                    )?;
                    out.package.source_graph.push(Edge {
                        from: id.clone(),
                        to,
                        relation: "opaque-xslt-import".into(),
                        namespace: None,
                    });
                }
            }
        }
    }
    // Explicit selected scenario references; compiled transformations are oracle-only and are not IR inputs.
    for to in [
        "resources/cii/16b/xsd/CrossIndustryInvoice_100pD16B.xsd",
        "resources/xrechnung-report.xsl",
        "resources/default-report.xsl",
        "resources/cii/16b/xsl/EN16931-CII-validation.xsl",
        "resources/xrechnung/3.0.2/xsl/XRechnung-CII-validation.xsl",
    ] {
        out.package.source_graph.push(Edge {
            from: "config.zip/scenarios.xml".into(),
            to: format!("config.zip/{to}"),
            relation: "selected-policy-resource".into(),
            namespace: None,
        });
    }
    for (from, to) in [
        ("SchemaProvider", "validator.jar/xsd/createReportInput.xsd"),
        ("ContentRepository", "validator.jar/transform/identity.xsl"),
    ] {
        out.package.source_graph.push(Edge {
            from: format!("validator.jar/de/kosit/validationtool/impl/{from}.class"),
            to: to.into(),
            relation: "engine-classpath-resource".into(),
            namespace: None,
        });
    }
    Ok(())
}
fn policy(out: &Inspection) -> Result<Policy> {
    let id = "config.zip/scenarios.xml";
    let text = std::str::from_utf8(&out.bytes[id]).map_err(|_| Error::new("policy encoding"))?;
    let doc = source::xml(text)?;
    let selected: Vec<_> = doc
        .descendants()
        .filter(|n| {
            n.is_element()
                && n.tag_name().name() == "scenario"
                && n.children().any(|c| {
                    c.is_element()
                        && c.tag_name().name() == "name"
                        && c.text() == Some("EN16931 XRechnung (CII)")
                })
        })
        .collect();
    if selected.len() != 1 {
        return Err(Error::new("exact scenario missing/ambiguous"));
    }
    let node = selected[0];
    if node.tag_name().namespace() != Some(CONFIG_NS) {
        return Err(Error::new("unknown configuration namespace"));
    }
    let mut context = xpath::Context::xslt2();
    let mut overrides = BTreeMap::new();
    let mut stages = Vec::new();
    for n in node.children().filter(|n| n.is_element()) {
        if n.tag_name().namespace() != Some(CONFIG_NS) {
            return Err(Error::new("unknown policy clause namespace"));
        }
        match n.tag_name().name() {
            "namespace" => {
                context.namespaces.insert(
                    n.attribute("prefix")
                        .ok_or_else(|| Error::new("namespace lacks prefix"))?
                        .into(),
                    n.text().unwrap_or("").into(),
                );
            }
            "validateWithXmlSchema" => stages.push("cii-xsd".to_owned()),
            "validateWithSchematron" => stages.push(
                if stages.len() == 1 {
                    "cen"
                } else {
                    "xrechnung"
                }
                .into(),
            ),
            "createReport" => {
                stages.push("report".into());
                for custom in n
                    .children()
                    .filter(|c| c.is_element() && c.tag_name().name() == "customLevel")
                {
                    let severity = match custom.attribute("level") {
                        Some("warning") => Severity::Warning,
                        Some("error") => Severity::Error,
                        Some("information") => Severity::Information,
                        _ => return Err(Error::new("unknown policy severity")),
                    };
                    for code in custom.text().unwrap_or("").split_whitespace() {
                        if overrides.insert(code.into(), severity).is_some() {
                            return Err(Error::new("duplicate severity override"));
                        }
                    }
                }
            }
            "name" | "description" | "match" | "acceptMatch" => (),
            _ => return Err(Error::new("unknown policy mapping")),
        }
    }
    if stages != ["cii-xsd", "cen", "xrechnung", "report"] || overrides.len() != 9 {
        return Err(Error::new("unsupported policy stage/override mapping"));
    }
    let expression = |local: &str| -> Result<Expression> {
        let n = node
            .children()
            .find(|n| n.is_element() && n.tag_name().name() == local)
            .ok_or_else(|| Error::new("missing policy expression"))?;
        let text = n.text().unwrap_or("").to_owned();
        let location = source::location(n, id, None);
        let mut tree = xpath::parse(&text, &context).map_err(|e| e.at(&location))?;
        xpath::resolve(&mut tree, &BTreeMap::new(), id)?;
        Ok(Expression {
            text,
            source: location,
            tree,
        })
    };
    let report = |lines: &str| vec![out.evidence("config.zip/resources/default-report.xsl", lines)];
    let jar = |member: &str, method: &str| {
        vec![out.evidence(
            &format!("validator.jar/de/kosit/validationtool/impl/{member}.class"),
            method,
        )]
    };
    Ok(Policy { scenario:"EN16931 XRechnung (CII)".into(),namespaces:context.namespaces.clone(),selection:expression("match")?,acceptance:expression("acceptMatch")?,execution:BackendState::Unavailable,clauses:vec![
        PolicyClause::OrderedStages { stages:vec!["document-parse","document-identification","scenario-selection","cii-xsd","cen","xrechnung","report-input-validation","report","acceptance"].into_iter().map(str::to_owned).collect(),schema_failure_skips_rules:true,evidence:vec![out.evidence(id,"scenario lines 334-403"),out.evidence("validator.jar/de/kosit/validationtool/impl/DefaultCheck.class","task sequence"),out.evidence("validator.jar/de/kosit/validationtool/impl/tasks/SchematronValidationAction.class","isSkipped/isSchemaInvalid")] },
        PolicyClause::FlagConversion { inputs:vec!["raw-flag".into(),"raw-role".into()],precedence:vec![Severity::Error,Severity::Warning,Severity::Information],error:vec!["fatal".into(),"error".into()],warning:vec!["warning".into(),"warn".into()],information:vec!["information".into(),"info".into()],unknown:Severity::Error,evidence:report("lines 243-266; raw flag or role to base report level") },
        PolicyClause::Overrides { values:overrides,processing_error_immune:true,evidence:vec![out.evidence(id,"lines 384-400"),out.evidence("config.zip/resources/default-report.xsl","lines 361-379; PROCESSING_ERROR immunity")] },
        PolicyClause::MissingSvrl { creates_processing_error_unless_already_present:true,evidence:report("lines 209-239") },
        PolicyClause::Assessment { rejects_processing_errors:true,rejects_unmatched:true,rejects_normalized_fallback_name:true,rejecting_severity:Severity::Error,stage_invalid_severities:vec![Severity::Error,Severity::Warning],stage_validity_uses:"base-report-level (before custom overrides)".into(),evidence:report("lines 233-234 and 312-335") },
        PolicyClause::Acceptance { absent_selector_fallback:"schema-and-schematron-valid (engine only; selected scenario has explicit selector)".into(),selector_ebv:true,requires_schema_scenario_report:true,xpath_failure_is_processing_failure:true,evidence:jar("tasks/ComputeAcceptanceAction","check; selector evaluation and schema/Schematron fallback") },
    ] })
}
pub fn requirements(package: &Package) -> Result<BTreeSet<Requirement>> {
    fn walk(value: &serde_json::Value, out: &mut BTreeSet<Requirement>) {
        match value {
            serde_json::Value::Object(map) => {
                for key in ["kind", "operator", "axis"] {
                    if let Some(v) = map.get(key).and_then(serde_json::Value::as_str) {
                        out.insert(Requirement {
                            category: key.into(),
                            name: v.into(),
                        });
                    }
                }
                if map.get("kind").and_then(serde_json::Value::as_str) == Some("step") {
                    if let Some(test) = map.get("test") {
                        if test.get("kind").and_then(serde_json::Value::as_str) == Some("kind") {
                            if let Some(name) = test.get("name").and_then(serde_json::Value::as_str)
                            {
                                out.insert(Requirement {
                                    category: "node-test-kind".into(),
                                    name: name.into(),
                                });
                            }
                        }
                    }
                }
                if map.get("kind").and_then(serde_json::Value::as_str) == Some("call") {
                    if let (Some(n), Some(args)) = (
                        map.get("name"),
                        map.get("arguments").and_then(serde_json::Value::as_array),
                    ) {
                        out.insert(Requirement {
                            category: "function".into(),
                            name: format!(
                                "{{{}}}{}/{}",
                                n["namespace"].as_str().unwrap_or(""),
                                n["local"].as_str().unwrap_or(""),
                                args.len()
                            ),
                        });
                    }
                }
                for key in ["sequence_type", "result_type"] {
                    if let Some(t) = map.get(key).filter(|v| v.is_object()) {
                        out.insert(Requirement {
                            category: "type".into(),
                            name: format!(
                                "{{{}}}{}{}",
                                t["name"]["namespace"].as_str().unwrap_or(""),
                                t["name"]["local"].as_str().unwrap_or(""),
                                t["occurrence"].as_str().unwrap_or("")
                            ),
                        });
                    }
                }
                if let Some(v) = map.get("lexical_kind").and_then(serde_json::Value::as_str) {
                    out.insert(Requirement {
                        category: "numeric".into(),
                        name: v.into(),
                    });
                }
                for v in map.values() {
                    walk(v, out);
                }
            }
            serde_json::Value::Array(items) => {
                for v in items {
                    walk(v, out);
                }
            }
            _ => (),
        }
    }
    let mut out = BTreeSet::new();
    for schema in &package.schemas {
        for helper in &schema.helpers {
            out.insert(Requirement {
                category: "helper".into(),
                name: format!("{}/{}", helper.name.expanded(), helper.parameters.len()),
            });
        }
    }
    walk(&serde_json::to_value(package)?, &mut out);
    for name in [
        "document-root-global-initialization",
        "lexical-shadowing",
        "recursive-helpers",
        "first-match-per-pattern",
        "match-pattern-dispatch",
        "dynamic-diagnostics",
        "xml-name-prefix-preservation",
        "schema-full-closure",
        "processing-failure-propagation",
    ] {
        out.insert(Requirement {
            category: "semantics".into(),
            name: name.into(),
        });
    }
    Ok(out)
}
