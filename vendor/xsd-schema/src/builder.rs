//! SchemaSet builder with compile() pattern
//!
//! Provides a fluent API for loading multiple schemas before compilation,
//! similar to .NET's XmlSchemaSet pattern.
//!
//! XSD version is set on the builder — the parser derives it automatically.
//! Use `SchemaSetBuilder::xsd11()` for XSD 1.1, `SchemaSetBuilder::new()` for XSD 1.0.
//!
//! # Example
//!
//! ```
//! use xsd_schema::SchemaSetBuilder;
//!
//! let compiled = SchemaSetBuilder::new()
//!     .add_source(r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
//!         <xs:element name="root" type="xs:string"/>
//!     </xs:schema>"#, "schema.xsd")
//!     .expect("parse failed")
//!     .compile()
//!     .expect("compile failed");
//!
//! println!("Loaded {} documents", compiled.stats.documents_loaded);
//! ```

use crate::error::{SchemaError, SchemaResult};
use crate::ids::DocumentId;
use crate::parser::parse::parse_schema_with_config;
use crate::parser::resolver::{
    fixup_composition_edges, normalize_path, resolve_directive_closure, DirectiveClosure,
    ResolverConfig, SchemaLoader, SchemaResolver,
};
#[cfg(feature = "async")]
use crate::parser::resolver::{resolve_directive_closure_async, AsyncSchemaLoader};
use crate::pipeline::process_loaded_schemas;
use crate::schema::model::{RegexCompat, XsdVersion};
use crate::schema::SchemaSet;
use std::path::Path;

/// Builder for creating compiled schema sets.
///
/// Implements the C# XmlSchemaSet pattern where schemas are added first,
/// then compiled together as a group.
///
/// # Example
///
/// ```
/// use xsd_schema::SchemaSetBuilder;
///
/// let compiled = SchemaSetBuilder::new()
///     .add_source(r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
///         <xs:element name="root" type="xs:string"/>
///     </xs:schema>"#, "inline.xsd")
///     .expect("parse failed")
///     .compile()
///     .expect("compile failed");
///
/// assert_eq!(compiled.stats.documents_loaded, 1);
/// ```
pub struct SchemaSetBuilder {
    schema_set: SchemaSet,
    resolver: SchemaResolver,
    pending_docs: Vec<DocumentId>,
    /// Errors from mandatory directives (include/redefine/override)
    errors: Vec<SchemaError>,
    /// Errors from xs:import directives
    import_errors: Vec<SchemaError>,
}

impl SchemaSetBuilder {
    /// Create a new builder with default configuration.
    ///
    /// Uses the default loader chain (embedded + filesystem) and
    /// automatically adds the XML catalog for well-known namespaces.
    pub fn new() -> Self {
        let mut resolver = SchemaResolver::new();
        resolver.catalog_mut().add_xml_catalog();

        Self {
            schema_set: SchemaSet::new(),
            resolver,
            pending_docs: Vec::new(),
            errors: Vec::new(),
            import_errors: Vec::new(),
        }
    }

    /// Create builder with custom resolver configuration.
    pub fn with_config(config: ResolverConfig) -> Self {
        let mut resolver = SchemaResolver::with_config(config);
        resolver.catalog_mut().add_xml_catalog();

        Self {
            schema_set: SchemaSet::new(),
            resolver,
            pending_docs: Vec::new(),
            errors: Vec::new(),
            import_errors: Vec::new(),
        }
    }

    /// Create builder with custom loader.
    pub fn with_loader(loader: Box<dyn SchemaLoader>) -> Self {
        let mut resolver = SchemaResolver::with_loader(loader);
        resolver.catalog_mut().add_xml_catalog();

        Self {
            schema_set: SchemaSet::new(),
            resolver,
            pending_docs: Vec::new(),
            errors: Vec::new(),
            import_errors: Vec::new(),
        }
    }

    /// Create a builder for XSD `version` over a resolver set up by the
    /// caller: its configuration, loaders, catalog and location rewriter are
    /// used as they are (no catalog entry is added).
    ///
    /// ```
    /// use xsd_schema::{LoaderChain, ResolverConfig, SchemaResolver, SchemaSetBuilder, XsdVersion};
    ///
    /// let mut config = ResolverConfig::default();
    /// config.max_depth = 16;
    /// let mut resolver =
    ///     SchemaResolver::with_config_and_loader(config, Box::new(LoaderChain::with_defaults()));
    /// resolver.catalog_mut().add_xml_catalog();
    /// let builder = SchemaSetBuilder::with_resolver(resolver, XsdVersion::V1_1);
    /// assert_eq!(builder.schema_count(), 0);
    /// ```
    pub fn with_resolver(resolver: SchemaResolver, version: XsdVersion) -> Self {
        Self {
            schema_set: SchemaSet::with_version(version),
            resolver,
            pending_docs: Vec::new(),
            errors: Vec::new(),
            import_errors: Vec::new(),
        }
    }

    /// Create a builder configured for a specific XSD version.
    pub fn with_version(version: XsdVersion) -> Self {
        let mut resolver = SchemaResolver::new();
        resolver.catalog_mut().add_xml_catalog();

        Self {
            schema_set: SchemaSet::with_version(version),
            resolver,
            pending_docs: Vec::new(),
            errors: Vec::new(),
            import_errors: Vec::new(),
        }
    }

    /// Create a builder configured for XSD 1.1.
    pub fn xsd11() -> Self {
        Self::with_version(XsdVersion::V1_1)
    }

    /// Set the regex compatibility mode on the underlying schema set.
    ///
    /// Affects how pattern facets in subsequently added/compiled schemas
    /// are validated. See [`RegexCompat`] and `doc/INTRODUCTION.md` for
    /// the closed list of leniencies enabled by `LenientMs`. Default is
    /// `RegexCompat::Strict`.
    pub fn set_regex_compatibility(&mut self, compat: RegexCompat) -> &mut Self {
        self.schema_set.set_regex_compatibility(compat);
        self
    }

    /// Map every location — each directive's, and each one given to this
    /// builder — through `rewriter` before it is resolved and identified:
    /// the place for catalog rules that a loader cannot apply (one document
    /// under two spellings, namespace-keyed mappings, a remote location to a
    /// local mirror). Its answer is final (an absolute URI or path is loaded
    /// as it is, never resolved against the including document's base; a
    /// relative path against the working directory), and it is asked once
    /// per location; see [`LocationRewriter`](crate::LocationRewriter).
    ///
    /// ```
    /// use xsd_schema::{LocationRequest, SchemaResolver, SchemaSetBuilder};
    ///
    /// // A namespace-keyed rule: `?xsd=N` locations of a service, by namespace.
    /// let mut builder = SchemaSetBuilder::new();
    /// builder.set_location_rewriter(|r: &LocationRequest<'_>, _: &SchemaResolver| {
    ///     match (r.namespace, r.location.contains("?xsd=")) {
    ///         (Some("urn:orders"), true) => Some("/srv/schemas/orders.xsd".to_string()),
    ///         _ => None,
    ///     }
    /// });
    /// ```
    pub fn set_location_rewriter(
        &mut self,
        rewriter: impl crate::LocationRewriter + 'static,
    ) -> &mut Self {
        self.resolver.set_location_rewriter(rewriter);
        self
    }

    /// Create a builder configured for XSD 1.1 with a custom schema loader.
    pub fn xsd11_with_loader(loader: Box<dyn SchemaLoader>) -> Self {
        let mut resolver = SchemaResolver::with_loader(loader);
        resolver.catalog_mut().add_xml_catalog();
        Self {
            schema_set: SchemaSet::with_version(XsdVersion::V1_1),
            resolver,
            pending_docs: Vec::new(),
            errors: Vec::new(),
            import_errors: Vec::new(),
        }
    }

    /// Create a builder with a custom async loader for non-blocking I/O.
    ///
    /// The async loader is used by [`add_async`](SchemaSetBuilder::add_async)
    /// and [`compile_async`](SchemaSetBuilder::compile_async).
    #[cfg(feature = "async")]
    pub fn with_async_loader(loader: Box<dyn AsyncSchemaLoader>) -> Self {
        let mut resolver = SchemaResolver::with_async_loader(loader);
        resolver.catalog_mut().add_xml_catalog();

        Self {
            schema_set: SchemaSet::new(),
            resolver,
            pending_docs: Vec::new(),
            errors: Vec::new(),
            import_errors: Vec::new(),
        }
    }

    /// Re-load all schemas from an existing compiled schema set.
    ///
    /// Iterates the loaded locations in `schema_set` and adds each one to
    /// this builder. This lets you seed a new builder from a previously
    /// compiled set without manually tracking the original file paths —
    /// useful for enriching with `xsi:schemaLocation` hints.
    ///
    /// Locations that fail to load (e.g. inline sources without a file
    /// path) are silently skipped. Already-loaded locations are
    /// deduplicated.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let mut builder = SchemaSetBuilder::new();
    /// builder.add_from(&original_schema_set);
    /// load_hints_into_builder(&mut builder, &sl_hints, &nnsl_hints);
    /// let enriched = builder.compile()?;
    /// ```
    pub fn add_from(&mut self, schema_set: &SchemaSet) -> &mut Self {
        // The locations the set was loaded from are final: a location
        // rewriter mapped them already, and is not asked again.
        for location in schema_set.loaded_schema_locations() {
            let _ = self.add_normalized(absolute_location(location.to_string()));
        }
        self
    }

    /// Add a schema by namespace and location.
    ///
    /// Matches the C# `XmlSchemaSet.Add(namespace, location)` pattern.
    /// The namespace parameter is for documentation/validation purposes;
    /// the actual namespace comes from the schema's targetNamespace attribute.
    ///
    /// # Arguments
    ///
    /// * `_namespace` - Expected namespace (for documentation; not enforced)
    /// * `location` - File path or URI to load the schema from
    ///
    /// # Example
    ///
    /// ```
    /// use xsd_schema::SchemaSetBuilder;
    ///
    /// let builder = SchemaSetBuilder::new()
    ///     .add("urn:books", "examples/books.xsd")
    ///     .expect("failed to load books.xsd");
    ///
    /// assert_eq!(builder.schema_count(), 1);
    /// ```
    pub fn add(mut self, _namespace: &str, location: &str) -> SchemaResult<Self> {
        self.try_add(location)?;
        Ok(self)
    }

    /// Add a schema by location without consuming the builder.
    ///
    /// Returns `Ok(true)` if the schema was freshly loaded, `Ok(false)` if
    /// it was already present (dedup). Returns `Err` on load/parse failure.
    ///
    /// The location is first normalized via the resolver so that relative
    /// and absolute forms of the same path are correctly deduplicated.
    pub fn try_add(&mut self, location: &str) -> SchemaResult<bool> {
        let normalized = normalize_loaded_location(&self.resolver, location, "");
        self.add_normalized(normalized)
    }

    /// Load the document at `normalized` — a location already rewritten and
    /// resolved ([`normalize_loaded_location`]), so neither again.
    fn add_normalized(&mut self, normalized: String) -> SchemaResult<bool> {
        if self.schema_set.is_loaded(&normalized) {
            return Ok(false);
        }
        let content = self.resolver.load_content(&normalized)?;
        let doc_id = parse_schema_with_config(
            content.as_bytes(),
            &normalized,
            &mut self.schema_set,
            &self.resolver.config.parser_config,
        )?;
        self.pending_docs.push(doc_id);
        self.schema_set.mark_loaded(normalized, doc_id);
        Ok(true)
    }

    /// Add a schema by resolving a relative location against a base URI.
    ///
    /// Uses the builder's resolver for URI resolution (handles Windows
    /// paths, URL normalization, etc.). The resolved absolute URI is used
    /// for loading and dedup tracking.
    ///
    /// Returns `Ok(true)` if freshly loaded, `Ok(false)` if already present.
    pub fn try_add_relative(&mut self, location: &str, base_uri: &str) -> SchemaResult<bool> {
        let normalized = normalize_loaded_location(&self.resolver, location, base_uri);
        self.add_normalized(normalized)
    }

    /// Add a schema from XML source string.
    ///
    /// # Arguments
    ///
    /// * `xml` - The schema XML content as a string
    /// * `base_uri` - Base URI for resolving relative references
    ///
    /// # Example
    ///
    /// ```
    /// use xsd_schema::SchemaSetBuilder;
    ///
    /// let builder = SchemaSetBuilder::new()
    ///     .add_source(r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
    ///         <xs:element name="root" type="xs:string"/>
    ///     </xs:schema>"#, "inline.xsd")
    ///     .expect("parse failed");
    ///
    /// assert_eq!(builder.schema_count(), 1);
    /// ```
    pub fn add_source(mut self, xml: &str, base_uri: &str) -> SchemaResult<Self> {
        let normalized = normalize_loaded_location(&self.resolver, base_uri, "");
        let doc_id = parse_schema_with_config(
            xml.as_bytes(),
            &normalized,
            &mut self.schema_set,
            &self.resolver.config.parser_config,
        )?;
        self.pending_docs.push(doc_id);
        self.schema_set.mark_loaded(normalized, doc_id);
        Ok(self)
    }

    /// Add a schema from bytes.
    ///
    /// # Arguments
    ///
    /// * `xml` - The schema XML content as bytes
    /// * `base_uri` - Base URI for resolving relative references
    pub fn add_bytes(mut self, xml: &[u8], base_uri: &str) -> SchemaResult<Self> {
        let normalized = normalize_loaded_location(&self.resolver, base_uri, "");
        let doc_id = parse_schema_with_config(
            xml,
            &normalized,
            &mut self.schema_set,
            &self.resolver.config.parser_config,
        )?;
        self.pending_docs.push(doc_id);
        self.schema_set.mark_loaded(normalized, doc_id);
        Ok(self)
    }

    /// Get the number of schemas added so far.
    pub fn schema_count(&self) -> usize {
        self.pending_docs.len()
    }

    /// Check if a schema location has already been loaded.
    pub fn is_loaded(&self, location: &str) -> bool {
        self.schema_set.is_loaded(location)
    }

    /// Compile all added schemas.
    ///
    /// This performs the following phases:
    /// 1. **Directive Resolution** - Process include/import/redefine/override directives
    /// 2. **Redefine/Override Application** - Apply component replacements
    /// 3. **Inline Type Assembly** - Materialize inline type definitions
    /// 4. **Reference Resolution** - Resolve QName references to component keys
    /// 5. **Particle Allocation** - Allocate element declarations for content particles
    ///
    /// # Returns
    ///
    /// A [`CompiledSchemaSet`] containing the fully processed schema set and
    /// compilation statistics.
    ///
    /// # Errors
    ///
    /// Returns an error if any phase fails (invalid schema, missing references, etc.)
    pub fn compile(mut self) -> SchemaResult<CompiledSchemaSet> {
        // Phase 1: Resolve directives for all pending documents, and for
        // every document they read
        let pending = std::mem::take(&mut self.pending_docs);
        let closure = resolve_directive_closure(&pending, &mut self.resolver, &mut self.schema_set);
        self.finish(closure)
    }

    /// Phases 2–5 after directive resolution, shared by
    /// [`compile`](Self::compile) and `compile_async`.
    fn finish(mut self, closure: DirectiveClosure) -> SchemaResult<CompiledSchemaSet> {
        // Fixup cycle edges now that all documents have been loaded
        fixup_composition_edges(&mut self.schema_set);

        // Propagate schema-content errors from directive resolution.
        // Resolution/IO errors (file not found, network denied) are non-fatal
        // for all directive types — the target may be unavailable; they are
        // kept on the set (`SchemaSet::directive_failures`).
        // Schema-content errors (structural, XML parse, namespace) mean the
        // target was found but is invalid — those are always fatal.
        self.errors.extend(closure.errors);
        self.import_errors.extend(closure.import_errors);
        if let Some(err) = self
            .errors
            .into_iter()
            .chain(self.import_errors)
            .find(|e| e.is_schema_content_error())
        {
            return Err(err);
        }

        // Phases 2-5: Delegate to the pipeline's shared processing function
        // (redefine/override, inline assembly, reference resolution, particle allocation)
        self.schema_set
            .set_max_depth(self.resolver.config.parser_config.max_depth);
        let (inline_stats, resolution_stats) = process_loaded_schemas(&mut self.schema_set)?;

        let documents_loaded = self.schema_set.documents.len();
        Ok(CompiledSchemaSet {
            schema_set: self.schema_set,
            stats: CompilationStats {
                documents_loaded,
                inline_types_assembled: inline_stats.total_inline_types,
                types_resolved: resolution_stats.types_resolved,
                elements_resolved: resolution_stats.elements_resolved,
                attributes_resolved: resolution_stats.attributes_resolved,
                groups_resolved: resolution_stats.groups_resolved,
                attribute_groups_resolved: resolution_stats.attribute_groups_resolved,
            },
        })
    }

    /// Add a schema by namespace and location, loading content asynchronously.
    ///
    /// Async variant of [`add`](SchemaSetBuilder::add): the location is
    /// normalized and deduplicated the same way.
    #[cfg(feature = "async")]
    pub async fn add_async(mut self, _namespace: &str, location: &str) -> SchemaResult<Self> {
        let normalized = normalize_loaded_location(&self.resolver, location, "");
        if self.schema_set.is_loaded(&normalized) {
            return Ok(self);
        }
        let content = self.resolver.load_content_async(&normalized).await?;
        let doc_id = parse_schema_with_config(
            content.as_bytes(),
            &normalized,
            &mut self.schema_set,
            &self.resolver.config.parser_config,
        )?;
        self.pending_docs.push(doc_id);
        self.schema_set.mark_loaded(normalized, doc_id);
        Ok(self)
    }

    /// Compile all added schemas using async directive resolution.
    ///
    /// Async variant of [`compile`](SchemaSetBuilder::compile), with the same
    /// rules and outcome. Only directive resolution (I/O) is async; all
    /// computation phases remain synchronous.
    #[cfg(feature = "async")]
    pub async fn compile_async(mut self) -> SchemaResult<CompiledSchemaSet> {
        let pending = std::mem::take(&mut self.pending_docs);
        let closure =
            resolve_directive_closure_async(&pending, &mut self.resolver, &mut self.schema_set)
                .await;
        self.finish(closure)
    }
}

/// The location a document given to the builder is read and identified
/// under: the location rewriter's answer for it (decision 9: a document added
/// under one spelling and named by a directive under another is one
/// document), resolved, and made absolute against the working directory.
fn normalize_loaded_location(resolver: &SchemaResolver, location: &str, base_uri: &str) -> String {
    let resolved = resolver
        .rewritten_location(location, base_uri, None, None)
        .unwrap_or_else(|_| location.to_string());
    absolute_location(resolved)
}

/// `resolved`, made absolute against the working directory when it is a
/// relative path.
fn absolute_location(resolved: String) -> String {
    if is_absolute_location(&resolved) {
        return resolved;
    }

    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(_) => return resolved,
    };
    normalize_path(&cwd.join(&resolved))
        .to_string_lossy()
        .into_owned()
}

fn is_absolute_location(location: &str) -> bool {
    crate::parser::uri::scheme(location).is_some()
        || Path::new(location).is_absolute()
        || (location.len() >= 2 && location.as_bytes().get(1) == Some(&b':'))
}

impl Default for SchemaSetBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Compiled schema set ready for validation.
///
/// Contains the fully processed [`SchemaSet`] with all references resolved
/// and inline types assembled.
pub struct CompiledSchemaSet {
    /// The compiled schema set
    pub schema_set: SchemaSet,
    /// Compilation statistics
    pub stats: CompilationStats,
}

impl CompiledSchemaSet {
    /// Get a reference to the underlying schema set.
    pub fn schema_set(&self) -> &SchemaSet {
        &self.schema_set
    }

    /// Consume self and return the underlying schema set.
    pub fn into_schema_set(self) -> SchemaSet {
        self.schema_set
    }

    /// The directives whose schema document could not be read; see
    /// [`SchemaSet::directive_failures`].
    pub fn directive_failures(&self) -> &[crate::parser::resolver::DirectiveFailure] {
        self.schema_set.directive_failures()
    }
}

/// Statistics from schema compilation.
#[derive(Debug, Default, Clone)]
pub struct CompilationStats {
    /// Number of schema documents loaded
    pub documents_loaded: usize,
    /// Number of inline types assembled
    pub inline_types_assembled: usize,
    /// Number of type references resolved
    pub types_resolved: usize,
    /// Number of element references resolved
    pub elements_resolved: usize,
    /// Number of attribute references resolved
    pub attributes_resolved: usize,
    /// Number of group references resolved
    pub groups_resolved: usize,
    /// Number of attribute group references resolved
    pub attribute_groups_resolved: usize,
}

impl CompilationStats {
    /// Get total number of references resolved
    pub fn total_references_resolved(&self) -> usize {
        self.types_resolved
            + self.elements_resolved
            + self.attributes_resolved
            + self.groups_resolved
            + self.attribute_groups_resolved
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_new() {
        let builder = SchemaSetBuilder::new();
        assert_eq!(builder.schema_count(), 0);
    }

    #[test]
    fn test_builder_add_source() {
        let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
            <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root" type="xs:string"/>
            </xs:schema>"#;

        let builder = SchemaSetBuilder::new()
            .add_source(xsd, "test.xsd")
            .expect("Should parse schema");

        assert_eq!(builder.schema_count(), 1);
    }

    #[test]
    fn test_builder_compile() {
        let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
            <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="person">
                    <xs:complexType>
                        <xs:sequence>
                            <xs:element name="name" type="xs:string"/>
                        </xs:sequence>
                    </xs:complexType>
                </xs:element>
            </xs:schema>"#;

        let compiled = SchemaSetBuilder::new()
            .add_source(xsd, "test.xsd")
            .expect("Should parse schema")
            .compile()
            .expect("Should compile");

        assert_eq!(compiled.stats.documents_loaded, 1);
        assert!(compiled.stats.inline_types_assembled > 0);
    }

    /// The builder validates type derivations.
    ///
    /// The fixture is deliberately schema-INVALID: `InvalidRestriction`
    /// declares an element `b` absent from `Base`'s content model, so no rcase
    /// in XSD 1.0 §3.9.6 can hold and `derivation-ok-restriction` must fail.
    #[test]
    fn builder_rejects_invalid_derivation() {
        let schema = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
<xs:complexType name="Base"><xs:sequence><xs:element name="a" type="xs:string"/></xs:sequence></xs:complexType>
<xs:complexType name="InvalidRestriction"><xs:complexContent><xs:restriction base="Base">
<xs:sequence><xs:element name="b" type="xs:string"/></xs:sequence>
</xs:restriction></xs:complexContent></xs:complexType>
</xs:schema>"#;

        let strict = SchemaSetBuilder::new()
            .add_source(schema, "strict-builder.xsd")
            .unwrap()
            .compile();
        assert!(
            strict.is_err(),
            "compile() must reject an invalid derivation"
        );
    }

    /// A circular derivation has no valid compilation order, so the builder
    /// must reject it rather than loop.
    #[test]
    fn builder_detects_circular_derivation() {
        let schema = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
<xs:simpleType name="A"><xs:restriction base="B"><xs:maxLength value="4"/></xs:restriction></xs:simpleType>
<xs:simpleType name="B"><xs:restriction base="A"><xs:maxLength value="8"/></xs:restriction></xs:simpleType>
</xs:schema>"#;

        let result = SchemaSetBuilder::new()
            .add_source(schema, "cycle-builder.xsd")
            .unwrap()
            .compile();
        let err = result.err().expect("circular derivation must still fail");
        assert!(
            err.to_string().contains("Circular type dependency"),
            "expected a circular-dependency failure, got: {err:?}"
        );
    }

    #[cfg(feature = "async")]
    #[test]
    fn async_builder_exposes_compile_async() {
        let schema = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"/>"#;
        let future = SchemaSetBuilder::new()
            .add_source(schema, "async-builder.xsd")
            .unwrap()
            .compile_async();
        drop(future);
    }

    #[test]
    fn test_builder_multiple_schemas() {
        let xsd1 = r#"<?xml version="1.0" encoding="UTF-8"?>
            <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                       targetNamespace="http://example.com/schema1">
                <xs:element name="item1" type="xs:string"/>
            </xs:schema>"#;

        let xsd2 = r#"<?xml version="1.0" encoding="UTF-8"?>
            <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                       targetNamespace="http://example.com/schema2">
                <xs:element name="item2" type="xs:int"/>
            </xs:schema>"#;

        let compiled = SchemaSetBuilder::new()
            .add_source(xsd1, "schema1.xsd")
            .expect("Should parse schema1")
            .add_source(xsd2, "schema2.xsd")
            .expect("Should parse schema2")
            .compile()
            .expect("Should compile");

        assert_eq!(compiled.stats.documents_loaded, 2);
    }

    #[test]
    fn test_normalize_loaded_location_keeps_leading_parent_dir() {
        // Missing files are not canonicalized, so the result is purely lexical.
        let resolver = SchemaResolver::new();
        let cwd = std::env::current_dir().unwrap();
        let parent = cwd.parent().unwrap();

        let location = normalize_loaded_location(&resolver, "../no-such-dir/x.xsd", "");
        assert_eq!(
            Path::new(&location),
            parent.join("no-such-dir").join("x.xsd")
        );

        let location = normalize_loaded_location(&resolver, "./../no-such-dir/x.xsd", "");
        assert_eq!(
            Path::new(&location),
            parent.join("no-such-dir").join("x.xsd")
        );

        let location =
            normalize_loaded_location(&resolver, "../../no-such.xsd", "no-such-dir/a.xsd");
        assert_eq!(Path::new(&location), parent.join("no-such.xsd"));
    }

    #[test]
    fn test_compilation_stats() {
        let stats = CompilationStats {
            documents_loaded: 2,
            inline_types_assembled: 5,
            types_resolved: 10,
            elements_resolved: 8,
            attributes_resolved: 3,
            groups_resolved: 2,
            attribute_groups_resolved: 1,
        };

        assert_eq!(stats.total_references_resolved(), 24);
    }
}
