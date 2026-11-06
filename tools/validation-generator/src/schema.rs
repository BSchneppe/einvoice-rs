//! Retain the exact schema graph. This does not compile or execute XML Schema.
use crate::{source, Error, Result};
use std::collections::{BTreeMap, BTreeSet};
use validation_ir::Edge;
const XS: &str = "http://www.w3.org/2001/XMLSchema";
/// Only explicit locations or exact (source, namespace) mappings may resolve.
pub fn graph(
    resources: &BTreeMap<String, String>,
    locationless: &BTreeMap<(String, String), String>,
) -> Result<Vec<Edge>> {
    let allowed: BTreeSet<_> = resources.keys().cloned().collect();
    let mut edges = Vec::new();
    for (id, text) in resources {
        let doc = source::xml(text)?;
        let root = doc.root_element();
        if root.tag_name().namespace() != Some(XS) || root.tag_name().name() != "schema" {
            return Err(Error::new("invalid schema resource root"));
        }
        for node in root.children().filter(|n| {
            n.is_element() && ["import", "include", "redefine"].contains(&n.tag_name().name())
        }) {
            let location = source::location(node, id, None);
            let edge = (|| {
                if node.tag_name().namespace() != Some(XS) || node.tag_name().name() != "import" {
                    return Err(Error::new("unsupported schema include/redefine/namespace"));
                }
                for attr in node.attributes() {
                    if attr.namespace().is_some()
                        || !["namespace", "schemaLocation", "id"].contains(&attr.name())
                    {
                        return Err(Error::new("unknown schema import attribute"));
                    }
                }
                let namespace = node
                    .attribute("namespace")
                    .ok_or_else(|| Error::new("import lacks namespace"))?;
                let target = if let Some(href) = node.attribute("schemaLocation") {
                    source::relative(id, href, &allowed)?
                } else {
                    locationless
                        .get(&(id.clone(), namespace.into()))
                        .filter(|t| allowed.contains(*t))
                        .cloned()
                        .ok_or_else(|| Error::new("unmapped locationless schema import"))?
                };
                let target_doc = source::xml(&resources[&target])?;
                if target_doc.root_element().attribute("targetNamespace") != Some(namespace) {
                    return Err(Error::new("schema import namespace mismatch"));
                }
                Ok(Edge {
                    from: id.clone(),
                    to: target,
                    relation: "xsd-import".into(),
                    namespace: Some(namespace.into()),
                })
            })()
            .map_err(|e: Error| e.at(&location))?;
            edges.push(edge);
        }
    }
    Ok(edges)
}
