//! Reproduce the small authored inspection artifact (no upstream resources).
use std::collections::{BTreeMap, BTreeSet};
use validation_generator::{bundle, emit, ir::*, schematron};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let schema = schematron::parse(
        &BTreeMap::from([
            (
                "main.sch".into(),
                include_str!("../tests/fixtures/scope.sch").into(),
            ),
            (
                "common.sch".into(),
                include_str!("../tests/fixtures/common.sch").into(),
            ),
        ]),
        "main.sch",
        Some("chosen"),
    )?;
    let mut package = Package {
        format: FORMAT.into(),
        target: "fixture:one".into(),
        source_package: "revision:1".into(),
        backend: BackendState::Unavailable,
        schemas: vec![schema],
        resources: BTreeMap::new(),
        source_graph: Vec::new(),
        policy: None,
        requirements: BTreeSet::new(),
    };
    package.requirements = bundle::requirements(&package)?;
    let bytes = emit::canonical(&package)?;
    use std::io::Write;
    std::io::stdout().write_all(&bytes)?;
    Ok(())
}
