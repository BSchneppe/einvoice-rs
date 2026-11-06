//! Deterministic inspection emission, with no public capability assertion or runnable bypass.
use crate::{bundle::Inspection, source, Error, Result};
use serde::Serialize;
use std::path::Path;
#[derive(Serialize)]
struct Manifest<'a> {
    format: &'static str,
    artifact_kind: &'static str,
    target: &'a str,
    source_package: &'a str,
    ir_format: &'a str,
    ir_sha256: String,
    generator: Generator,
    backend: &'static str,
    archives: &'a std::collections::BTreeMap<String, serde_json::Value>,
    resources: &'a std::collections::BTreeMap<String, validation_ir::Resource>,
}
#[derive(Serialize)]
struct Generator {
    name: &'static str,
    version: &'static str,
    source_sha256: String,
}
pub fn canonical<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut b = serde_json::to_vec_pretty(value)?;
    b.push(b'\n');
    Ok(b)
}
fn generator_hash() -> String {
    source::hash(
        concat!(
            include_str!("lib.rs"),
            include_str!("main.rs"),
            include_str!("../Cargo.toml"),
            include_str!("../crates/validation-ir/Cargo.toml"),
            include_str!("source.rs"),
            include_str!("xpath.rs"),
            include_str!("schematron.rs"),
            include_str!("schema.rs"),
            include_str!("bundle.rs"),
            include_str!("emit.rs"),
            include_str!("../crates/validation-ir/src/lib.rs"),
            include_str!("../Cargo.lock"),
            include_str!("../jar-closure.lock.json")
        )
        .as_bytes(),
    )
}
pub fn inspect(inspection: &Inspection, directory: &Path) -> Result<String> {
    if directory.exists() && std::fs::read_dir(directory)?.next().is_some() {
        return Err(Error::new("inspection output directory must be empty"));
    }
    std::fs::create_dir_all(directory)?;
    let bytes = canonical(&inspection.package)?;
    let digest = source::hash(&bytes);
    std::fs::write(directory.join("ir.json"), &bytes)?;
    let manifest = Manifest {
        format: "einvoice-inspection-manifest/1",
        artifact_kind: "non-executable-inspection; cannot install or run",
        target: &inspection.package.target,
        source_package: &inspection.package.source_package,
        ir_format: &inspection.package.format,
        ir_sha256: digest.clone(),
        generator: Generator {
            name: env!("CARGO_PKG_NAME"),
            version: env!("CARGO_PKG_VERSION"),
            source_sha256: generator_hash(),
        },
        backend: "unavailable; no lowering/runtime evidence",
        archives: &inspection.archives,
        resources: &inspection.package.resources,
    };
    std::fs::write(directory.join("manifest.json"), canonical(&manifest)?)?;
    for (id, data) in &inspection.bytes {
        source::safe_member(id)?;
        let path = directory.join("resources").join(id);
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(path, data)?;
    }
    Ok(digest)
}
/// There is intentionally no caller supplied capability flag, callback, or evidence constructor.
/// BSC-8 must add a verified lowering/evidence gate bound to exact IR bytes and backend revision.
pub fn runnable(_inspection: &Inspection, _directory: &Path) -> Result<()> {
    Err(Error::new("backend unavailable: missing exact-package lowering/runtime evidence for XPath, numeric semantics, helpers, match dispatch, XSD closure and policy; inspection cannot install or run"))
}
