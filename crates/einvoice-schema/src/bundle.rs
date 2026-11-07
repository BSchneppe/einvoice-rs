//! Exact immutable resources and a resolver without fallback authorities.
use crate::{pinned::CII_RESOURCES, SourceDocument};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use xsd_schema::parser::{
    parse::{parse_schema_with_config, ParserConfig},
    resolver::{resolve_all_directives, SchemaLoader, SchemaResolver},
};
use xsd_schema::{SchemaError, SchemaSet};

#[derive(Debug)]
pub struct ResourceInfo {
    pub member: &'static str,
    pub sha256: &'static str,
    pub bytes: usize,
    pub namespace: &'static str,
    pub imports: &'static [(&'static str, &'static str)],
}
/// Byte authority is verified, but this is no runnable package capability.
#[derive(Debug, Clone)]
pub struct CiiBundle {
    bytes: [Arc<[u8]>; 4],
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitializationError {
    pub phase: &'static str,
    pub detail: String,
}
impl InitializationError {
    pub(crate) fn new(phase: &'static str, detail: impl Into<String>) -> Self {
        Self {
            phase,
            detail: detail.into(),
        }
    }
}
impl std::fmt::Display for InitializationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.phase, self.detail)
    }
}
impl std::error::Error for InitializationError {}
pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
impl CiiBundle {
    /// Exactly four original archive members. Order is immaterial; aliases,
    /// duplicates, changed bytes and additional authorities are refused.
    pub fn from_resources(resources: [(&str, Arc<[u8]>); 4]) -> Result<Self, InitializationError> {
        let mut ordered: [Option<Arc<[u8]>>; 4] = std::array::from_fn(|_| None);
        for (member, bytes) in resources {
            let i = CII_RESOURCES
                .iter()
                .position(|r| r.member == member)
                .ok_or_else(|| InitializationError::new("membership", member))?;
            let meta = &CII_RESOURCES[i];
            if ordered[i].is_some() || bytes.len() != meta.bytes || sha256(&bytes) != meta.sha256 {
                return Err(InitializationError::new("resource identity", member));
            }
            preflight(&bytes, meta)?;
            ordered[i] = Some(bytes);
        }
        // Four distinct members in an array of four prove total membership.
        Ok(Self {
            bytes: ordered.map(|b| b.expect("checked complete membership")),
        })
    }
    pub(crate) fn compile(&self) -> Result<SchemaSet, InitializationError> {
        let resources: CiiLoader = CiiLoader {
            bytes: self.bytes.clone(),
        };
        let mut resolver = SchemaResolver::with_loader(Box::new(resources));
        debug_assert!(resolver
            .catalog()
            .lookup("http://www.w3.org/XML/1998/namespace")
            .is_none());
        let mut schema = SchemaSet::xsd10();
        let config = ParserConfig::default();
        let uri = virtual_uri(CII_RESOURCES[0].member);
        let root = parse_schema_with_config(&self.bytes[0], &uri, &mut schema, &config)
            .map_err(|e| InitializationError::new("parse", e.to_string()))?;
        schema.mark_loaded(uri, root);
        // The closure is fixed, preflighted and acyclic. Resolve each newly loaded
        // document once; the loader can only return the four authenticated bytes.
        let mut cursor = 0;
        while cursor < schema.documents.len() {
            if cursor >= 4 {
                return Err(InitializationError::new("closure", "extra loaded document"));
            }
            let result =
                resolve_all_directives(schema.documents[cursor].id, &mut resolver, &mut schema);
            if !result.is_ok() {
                return Err(InitializationError::new(
                    "directives",
                    format!("{:?}; {:?}", result.errors, result.import_errors),
                ));
            }
            cursor += 1;
        }
        if schema.documents.len() != 4
            || CII_RESOURCES.iter().any(|r| {
                schema
                    .documents
                    .iter()
                    .filter(|d| d.base_uri == virtual_uri(r.member))
                    .count()
                    != 1
            })
        {
            return Err(InitializationError::new(
                "closure",
                "loaded document identities differ",
            ));
        }
        let expected_edges: usize = CII_RESOURCES.iter().map(|r| r.imports.len()).sum();
        if schema.composition_edges.len() != expected_edges || schema.loaded_locations.len() != 4 {
            return Err(InitializationError::new(
                "closure",
                "loaded edge/membership count differs",
            ));
        }
        for meta in &CII_RESOURCES {
            let source = schema
                .documents
                .iter()
                .find(|d| d.base_uri == virtual_uri(meta.member))
                .expect("checked document");
            for (_, location) in meta.imports {
                let target_uri = virtual_uri(&format!("resources/cii/16b/xsd/{location}"));
                let target = schema
                    .documents
                    .iter()
                    .find(|d| d.base_uri == target_uri)
                    .ok_or_else(|| InitializationError::new("closure", "missing import target"))?;
                let count = schema
                    .composition_edges
                    .iter()
                    .filter(|e| {
                        e.source_doc == source.id
                            && e.target_doc == Some(target.id)
                            && e.schema_location == *location
                            && e.resolved_location == target_uri
                            && e.kind
                                == xsd_schema::schema::composition::CompositionEdgeKind::Import
                    })
                    .count();
                if count != 1 {
                    return Err(InitializationError::new(
                        "closure",
                        "resolved import edge differs",
                    ));
                }
            }
        }
        finish_schema(schema)
    }
}
fn virtual_uri(member: &str) -> String {
    format!("bsc8://cii/{member}")
}
#[derive(Debug)]
struct CiiLoader {
    bytes: [Arc<[u8]>; 4],
}
impl SchemaLoader for CiiLoader {
    fn can_load(&self, location: &str) -> bool {
        CII_RESOURCES
            .iter()
            .any(|r| virtual_uri(r.member) == location)
    }
    fn load(&self, location: &str) -> Result<String, SchemaError> {
        let i = CII_RESOURCES
            .iter()
            .position(|r| virtual_uri(r.member) == location)
            .ok_or_else(|| SchemaError::resolution("outside exact CII closure"))?;
        String::from_utf8(self.bytes[i].to_vec())
            .map_err(|e| SchemaError::resolution(e.to_string()))
    }
}
fn preflight(bytes: &[u8], meta: &ResourceInfo) -> Result<(), InitializationError> {
    let doc = SourceDocument::parse(bytes)
        .map_err(|e| InitializationError::new("preflight XML", e.to_string()))?;
    let root = doc.root_element();
    let name = root.name().expect("source element");
    if name.namespace_uri() != "http://www.w3.org/2001/XMLSchema" || name.local() != "schema" {
        return Err(InitializationError::new("preflight", "not xs:schema"));
    }
    let target = root
        .attributes()
        .find(|a| a.name().namespace_uri().is_empty() && a.name().local() == "targetNamespace");
    if target.map(|a| a.value()) != Some(meta.namespace) {
        return Err(InitializationError::new(
            "preflight",
            "target namespace differs",
        ));
    }
    let mut imports = 0;
    for n in doc.nodes() {
        if n.attributes().any(|a| {
            a.name().namespace_uri() == "http://www.w3.org/XML/1998/namespace"
                && a.name().local() == "base"
        }) {
            return Err(InitializationError::new("preflight", "xml:base forbidden"));
        }
        if let Some(name) = n.name() {
            if name.namespace_uri() == "http://www.w3.org/2001/XMLSchema"
                && ["import", "include", "redefine", "override"].contains(&name.local())
            {
                if name.local() != "import" || n.parent() != Some(root) {
                    return Err(InitializationError::new(
                        "preflight",
                        "unexpected directive",
                    ));
                }
                let ns = n
                    .attributes()
                    .find(|a| {
                        a.name().namespace_uri().is_empty() && a.name().local() == "namespace"
                    })
                    .map(|a| a.value());
                let location = n
                    .attributes()
                    .find(|a| {
                        a.name().namespace_uri().is_empty() && a.name().local() == "schemaLocation"
                    })
                    .map(|a| a.value());
                if meta.imports.get(imports).copied() != ns.zip(location) {
                    return Err(InitializationError::new(
                        "preflight",
                        "namespace/location/order differs",
                    ));
                }
                imports += 1;
            }
        }
    }
    if imports != meta.imports.len() {
        return Err(InitializationError::new("preflight", "missing import"));
    }
    Ok(())
}
pub(crate) fn finish_schema(mut schema: SchemaSet) -> Result<SchemaSet, InitializationError> {
    if schema.has_parsing_errors() || !schema.directive_failures().is_empty() {
        return Err(InitializationError::new(
            "schema",
            "parsing/directive failures",
        ));
    }
    xsd_schema::pipeline::process_loaded_schemas(&mut schema)
        .map_err(|e| InitializationError::new("processing", e.to_string()))?;
    if schema.has_parsing_errors() || !schema.directive_failures().is_empty() {
        return Err(InitializationError::new(
            "schema",
            "postprocessing failures",
        ));
    }
    for (_, element) in schema.arenas.elements.iter() {
        if let Some(e) = &element.deferred_type_error {
            return Err(InitializationError::new(
                "deferred element type",
                &e.message,
            ));
        }
        if !element.pending_ic_refs.is_empty() {
            return Err(InitializationError::new(
                "deferred identity constraint",
                "unresolved reference",
            ));
        }
    }
    for (_, ty) in schema.arenas.simple_types.iter() {
        if let Some(e) = &ty.deferred_item_type_error {
            return Err(InitializationError::new(
                "deferred list item type",
                &e.message,
            ));
        }
    }
    Ok(schema)
}
#[cfg(test)]
pub(crate) fn authored(source: &str) -> Result<SchemaSet, InitializationError> {
    let mut schema = SchemaSet::xsd10();
    // No directives allowed in authored tests either.
    let meta = ResourceInfo {
        member: "authored.xsd",
        sha256: "",
        bytes: source.len(),
        namespace: "",
        imports: &[],
    };
    // Authored schemas may omit targetNamespace; still reject all directives/DTD.
    let doc = SourceDocument::parse(source.as_bytes())
        .map_err(|e| InitializationError::new("authored preflight", e.to_string()))?;
    if doc.nodes().any(|n| {
        n.name().is_some_and(|n| {
            n.namespace_uri() == "http://www.w3.org/2001/XMLSchema"
                && ["import", "include", "redefine", "override"].contains(&n.local())
        })
    }) {
        return Err(InitializationError::new("authored preflight", meta.member));
    }
    parse_schema_with_config(
        source.as_bytes(),
        meta.member,
        &mut schema,
        &ParserConfig::default(),
    )
    .map_err(|e| InitializationError::new("authored parse", e.to_string()))?;
    finish_schema(schema)
}
