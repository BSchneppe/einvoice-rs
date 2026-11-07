//! Separately invoked, original-resource integration; no bundled XSD or oracle labels.
use einvoice_schema::*;
use std::{num::NonZeroUsize, path::PathBuf, sync::Arc};
#[test]
#[ignore = "requires authenticated original resources and a supplied development input"]
fn exact_original_closure_drives_and_refuses_resource_drift() {
    let directory = PathBuf::from(
        std::env::var_os("EINVOICE_CII_RESOURCE_DIR").expect("original resources directory"),
    );
    let input = std::env::var_os("EINVOICE_SCHEMA_INPUT").expect("supplied development input");
    let resources: [(&str, Arc<[u8]>); 4] = std::array::from_fn(|i| {
        (
            CII_RESOURCES[i].member,
            std::fs::read(directory.join(CII_RESOURCES[i].member))
                .unwrap()
                .into(),
        )
    });
    let bundle = CiiBundle::from_resources(resources.clone()).unwrap();
    let source = SourceDocument::parse(&std::fs::read(input).unwrap()).unwrap();
    let limits = RequestLimits {
        input_bytes: 16 * 1024 * 1024,
        nodes: 500_000,
        depth: 128,
        attributes_per_element: 1024,
        declarations_per_element: 256,
        namespaces_per_element: 1024,
        name_bytes: 4096,
        uri_bytes: 16384,
        value_bytes: 16 * 1024 * 1024,
        temporary_bytes: 1024 * 1024,
        work: 100_000_000,
        diagnostic_count: 1000,
        diagnostic_bytes: 16 * 1024 * 1024,
        terminal_bytes: 1024 * 1024,
    };
    with_epoch(&bundle, NonZeroUsize::new(2).unwrap(), |e| {
        for _ in 0..2 {
            let o = e
                .validate(&source, &limits, RequestControl::default())
                .unwrap();
            assert_eq!(o.assessment, Assessment::Valid, "{o:?}");
            assert!(o.finalization_succeeded);
            assert!(o.events.is_empty());
        }
        assert!(e.is_retired());
    })
    .unwrap();
    let mut changed = resources.clone();
    let mut bytes = changed[0].1.to_vec();
    bytes[0] ^= 1;
    changed[0].1 = bytes.into();
    assert!(CiiBundle::from_resources(changed).is_err());
    let mut duplicate = resources.clone();
    duplicate[1] = duplicate[0].clone();
    assert!(CiiBundle::from_resources(duplicate).is_err());
    let mut alias = resources;
    alias[0].0 = "resources/cii/16b/xsd/../xsd/CrossIndustryInvoice_100pD16B.xsd";
    assert!(CiiBundle::from_resources(alias).is_err());
}
