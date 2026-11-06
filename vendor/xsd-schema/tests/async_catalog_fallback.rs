//! The asynchronous directive walk follows the synchronous rule for an
//! import whose location hint is read but is not a schema document: the
//! catalog's location for the namespace is tried next (fix round F4).
#![cfg(feature = "async")]

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use xsd_schema::{
    AsyncSchemaLoader, ResolverConfig, SchemaError, SchemaLoader, SchemaResolver, SchemaResult,
    SchemaSetBuilder, XsdVersion,
};

const XS: &str = r#"xmlns:xs="http://www.w3.org/2001/XMLSchema""#;

#[derive(Debug, Clone)]
struct Web(Arc<HashMap<String, String>>);

impl Web {
    fn new(with_catalog_target: bool) -> Self {
        let mut docs = HashMap::new();
        docs.insert(
            "http://h/main.xsd".to_string(),
            format!(
                r#"<xs:schema {XS} targetNamespace="urn:t" xmlns:x="urn:x"><xs:import namespace="urn:x" schemaLocation="notschema.xml"/><xs:element name="r"><xs:complexType><xs:sequence><xs:element ref="x:e"/></xs:sequence></xs:complexType></xs:element></xs:schema>"#
            ),
        );
        docs.insert("http://h/notschema.xml".to_string(), "<html/>".to_string());
        if with_catalog_target {
            docs.insert(
                "http://h/x.xsd".to_string(),
                format!(
                    r#"<xs:schema {XS} targetNamespace="urn:x"><xs:element name="e"/></xs:schema>"#
                ),
            );
        }
        Web(Arc::new(docs))
    }
    fn get(&self, location: &str) -> SchemaResult<String> {
        self.0
            .get(location)
            .cloned()
            .ok_or_else(|| SchemaError::resolution(format!("404 {location}")))
    }
}

impl SchemaLoader for Web {
    fn load(&self, location: &str) -> SchemaResult<String> {
        self.get(location)
    }
    fn can_load(&self, _: &str) -> bool {
        true
    }
}

impl AsyncSchemaLoader for Web {
    fn load_async(
        &self,
        location: &str,
    ) -> Pin<Box<dyn Future<Output = SchemaResult<String>> + Send + '_>> {
        let result = self.get(location);
        Box::pin(async move { result })
    }
    fn can_load(&self, _: &str) -> bool {
        true
    }
}

fn config() -> ResolverConfig {
    ResolverConfig {
        allow_network: true,
        ..Default::default()
    }
}

fn sync(web: Web) -> SchemaResult<usize> {
    let mut resolver = SchemaResolver::with_config_and_loader(config(), Box::new(web));
    resolver.catalog_mut().add("urn:x", "http://h/x.xsd");
    SchemaSetBuilder::with_resolver(resolver, XsdVersion::V1_0)
        .add("", "http://h/main.xsd")?
        .compile()
        .map(|c| c.stats.documents_loaded)
}

fn asynchronous(web: Web) -> SchemaResult<usize> {
    // The synchronous loader of this resolver is the default chain, which
    // cannot read http: locations: everything comes through the async one.
    let mut resolver = SchemaResolver::with_config_and_async_loader(config(), Box::new(web));
    resolver.catalog_mut().add("urn:x", "http://h/x.xsd");
    futures::executor::block_on(async {
        SchemaSetBuilder::with_resolver(resolver, XsdVersion::V1_0)
            .add_async("", "http://h/main.xsd")
            .await?
            .compile_async()
            .await
            .map(|c| c.stats.documents_loaded)
    })
}

#[test]
fn an_unusable_hint_falls_back_to_the_catalog_in_both_walks() {
    assert_eq!(sync(Web::new(true)).expect("sync"), 2);
    assert_eq!(asynchronous(Web::new(true)).expect("async"), 2);
}

#[test]
fn both_walks_report_a_failed_fallback_alike() {
    let s = sync(Web::new(false))
        .expect_err("x:e is missing")
        .to_string();
    let a = asynchronous(Web::new(false))
        .expect_err("x:e is missing")
        .to_string();
    assert_eq!(s, a);
    assert!(
        s.contains("http://h/x.xsd"),
        "names the catalog's location: {s}"
    );
}
