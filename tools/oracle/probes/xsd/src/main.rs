use std::{collections::BTreeMap, fs, path::Path, time::Instant};
use xsd_schema::parser::resolver::{ResolverConfig, SchemaLoader, SchemaResolver};
use xsd_schema::validation::{
    drive_quick_xml, CollectingValidationSink, SchemaValidator, SchemaValidity, ValidationFlags,
};
use xsd_schema::xml::ParseLimits;
use xsd_schema::{SchemaError, SchemaSetBuilder, XsdVersion};

#[derive(Debug)]
struct BundleLoader(BTreeMap<String, String>);
impl SchemaLoader for BundleLoader {
    fn load(&self, location: &str) -> Result<String, SchemaError> {
        eprintln!("LOAD {location}");
        self.0
            .get(location)
            .cloned()
            .ok_or_else(|| SchemaError::resolution(format!("not in pinned bundle: {location}")))
    }
    fn can_load(&self, location: &str) -> bool {
        self.0.contains_key(location)
    }
}
fn limits() -> ParseLimits {
    ParseLimits::none()
        .with_max_depth(128)
        .with_max_name_bytes(1024)
        .with_max_namespace_uri_bytes(2048)
        .with_max_attributes(128)
        .with_max_namespace_declarations(64)
        .with_max_namespaces_in_scope(128)
        .with_max_attribute_value_bytes(65536)
        .with_max_text_bytes(1048576)
        .with_max_nodes(1000000)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root_arg = std::env::args()
        .nth(1)
        .expect("usage: einvoice-xsd-rust-probe ORACLE_CACHE");
    let root = Path::new(&root_arg);
    let schema_dir = root.join("config/resources/cii/16b/xsd");
    let names = [
        "CrossIndustryInvoice_100pD16B.xsd",
        "CrossIndustryInvoice_QualifiedDataType_100pD16B.xsd",
        "CrossIndustryInvoice_ReusableAggregateBusinessInformationEntity_100pD16B.xsd",
        "CrossIndustryInvoice_UnqualifiedDataType_100pD16B.xsd",
    ];
    let mut sources = BTreeMap::new();
    for name in names {
        sources.insert(
            format!("/bundle/{name}"),
            fs::read_to_string(schema_dir.join(name))?,
        );
    }
    let mut cfg = ResolverConfig::default();
    cfg.allow_network = false;
    cfg.max_depth = 16;
    cfg.parser_config.max_depth = 128;
    let resolver = SchemaResolver::with_config_and_loader(cfg, Box::new(BundleLoader(sources)));
    let start = Instant::now();
    let compiled = SchemaSetBuilder::with_resolver(resolver, XsdVersion::V1_0)
        .add(
            "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
            "/bundle/CrossIndustryInvoice_100pD16B.xsd",
        )?
        .compile()?;
    println!(
        "COMPILE {:?}; stats={:?}; directive_failures={:?}",
        start.elapsed(),
        compiled.stats,
        compiled.directive_failures()
    );
    assert!(
        compiled.directive_failures().is_empty(),
        "incomplete schema bundle"
    );
    let schema_set = compiled.schema_set();
    let validator = SchemaValidator::new(schema_set, ValidationFlags::default());
    let mut cases: Vec<(String, Vec<u8>, bool, ParseLimits)> = Vec::new();
    for i in 0..12 {
        let p = root.join(format!("corpus/sinabis/{i:05}.xml"));
        cases.push((format!("valid-{i}"), fs::read(p)?, true, limits()));
    }
    cases.push((
        "business-invalid-xsd-valid".into(),
        fs::read(root.join("corpus/mutations/missing-line-total.xml"))?,
        true,
        limits(),
    ));
    for name in [
        "extra-attribute",
        "unknown-element",
        "duplicate-context",
        "invalid-decimal",
    ] {
        cases.push((
            name.into(),
            fs::read(root.join(format!("corpus/mutations/{name}.xml")))?,
            false,
            limits(),
        ));
    }
    let valid = fs::read_to_string(root.join("corpus/sinabis/00000.xml"))?;
    cases.push((
        "malformed-truncated".into(),
        valid[..valid.len() - 100].as_bytes().to_vec(),
        false,
        limits(),
    ));
    cases.push((
        "namespace-prefix-renamed".into(),
        valid
            .replace("rsm:", "invoice:")
            .replace("xmlns:rsm=", "xmlns:invoice=")
            .into_bytes(),
        true,
        limits(),
    ));
    cases.push((
        "namespace-wrong-uri".into(),
        valid
            .replace(
                "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
                "urn:wrong",
            )
            .into_bytes(),
        false,
        limits(),
    ));
    cases.push((
        "bounded-depth".into(),
        valid.into_bytes(),
        false,
        limits().with_max_depth(2),
    ));
    let mut disagreements = 0;
    for (name, xml, expected, bound) in cases {
        if xml.len() > 10 * 1024 * 1024 {
            return Err("input size exceeds 10 MiB".into());
        }
        let (mut errors, mut warnings) = (Vec::new(), Vec::new());
        let mut runtime = validator.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        runtime.set_parse_limits(bound);
        let start = Instant::now();
        let outcome = drive_quick_xml(xml.as_slice(), &mut runtime, schema_set);
        drop(runtime);
        let accepted = matches!(&outcome, Ok(o) if o.root_validity == Some(SchemaValidity::Valid))
            && errors.is_empty();
        println!("CASE {name}: accepted={accepted} expected={expected} elapsed={:?} outcome={outcome:?} errors={} warnings={}", start.elapsed(), errors.len(), warnings.len());
        for error in errors {
            println!("  ERROR {error}");
        }
        for warning in warnings {
            println!("  WARNING {warning:?}");
        }
        disagreements += usize::from(accepted != expected);
    }
    assert_eq!(disagreements, 0, "sample disagreements");
    Ok(())
}
