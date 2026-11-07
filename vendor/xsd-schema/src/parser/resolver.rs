//! Schema resolution for include, import, and redefine directives
//!
//! This module handles resolving and loading external schema documents
//! referenced by `xs:include`, `xs:import`, and `xs:redefine` directives.
//!
//! # Resolution Process
//!
//! 1. **Include** - Same target namespace, required schemaLocation
//!    - Loads the referenced schema
//!    - Merges components into the same namespace
//!    - Supports chameleon includes (no targetNamespace)
//!
//! 2. **Import** - Different namespace, optional schemaLocation
//!    - Loads schema for the specified namespace
//!    - Components remain in their declared namespace
//!    - Without schemaLocation, relies on catalog or pre-loaded schemas
//!
//! 3. **Redefine** - Same namespace, extends/restricts existing types
//!    - Deprecated in XSD 1.1 (use override instead)
//!    - Allows redefining types/groups from included schema
//!
//! # Circular Dependencies
//!
//! The resolver tracks loaded schema locations to:
//! - Detect circular includes (allowed, just skip)
//! - Prevent infinite loops
//! - Enable caching of resolved schemas
//!
//! # URI Resolution
//!
//! The resolver supports:
//! - Absolute file paths
//! - Relative paths (resolved against base URI)
//! - URIs (`http:`, `https:`, `file:`, …): RFC 3986 reference resolution and
//!   normalization, so that one document has one identity
//!   ([`SchemaResolver::resolve_location`])
//! - HTTP/HTTPS URLs (via async trait)
//! - Catalog-based resolution
//! - A [`LocationRewriter`] that maps directive locations before they are
//!   resolved and identified
//!
//! # Customizable Loading
//!
//! The [`SchemaLoader`] trait allows custom loading strategies:
//! - [`FileSystemLoader`] - Loads from local file system
//! - [`EmbeddedLoader`] - Loads from embedded static assets
//! - [`LoaderChain`] - Combines multiple loaders with priority
//!
//! # Failures and limits
//!
//! A directive whose document cannot be read is not a schema error
//! (Structures §4.2.3, §4.2.6.2): it is recorded as a [`DirectiveFailure`]
//! (`SchemaSet::directive_failures`). Directives are resolved in one walk
//! that does not recurse, the same for the synchronous and the asynchronous
//! loaders; [`ResolverConfig::max_depth`] bounds the chain of directives.

use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::path::{Component, Path, PathBuf};
#[cfg(feature = "async")]
use std::pin::Pin;

use crate::error::{SchemaError, SchemaResult};
use crate::ids::{DocumentId, NameId};
use crate::parser::parse::ParserConfig;
use crate::schema::composition::{CompositionEdge, CompositionEdgeKind};
use crate::SchemaSet;

/// Result of a single `load_schema` call, distinguishing three outcomes.
#[derive(Debug)]
pub enum LoadOutcome {
    /// Schema was freshly loaded and parsed.
    Loaded(DocumentId),
    /// Schema was already in `loaded_locations`.
    AlreadyLoaded(DocumentId),
    /// Schema is currently mid-parse (in the `resolving` set). Contains the
    /// resolved URI so the caller can record a cycle edge and fix it up later.
    Cycle(String),
}

// ============================================================================
// Encoding-Aware Decoding
// ============================================================================

const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
const UTF16_LE_BOM: &[u8] = &[0xFF, 0xFE];
const UTF16_BE_BOM: &[u8] = &[0xFE, 0xFF];

#[derive(Copy, Clone)]
enum Endian {
    Le,
    Be,
}

/// Decode raw XML bytes into UTF-8 bytes, sniffing common Unicode encodings
/// per XML 1.0 §F.1.
///
/// Recognizes UTF-8 with/without BOM and UTF-16 LE/BE with/without BOM. The
/// returned `Vec<u8>` is the input buffer unchanged when it is already UTF-8
/// with no BOM (zero-copy fast path).
pub fn decode_xml_to_utf8_bytes(bytes: Vec<u8>) -> SchemaResult<Vec<u8>> {
    if bytes.starts_with(UTF8_BOM) {
        return Ok(bytes[UTF8_BOM.len()..].to_vec());
    }
    if bytes.starts_with(UTF16_LE_BOM) {
        return Ok(decode_utf16(&bytes[UTF16_LE_BOM.len()..], Endian::Le)?.into_bytes());
    }
    if bytes.starts_with(UTF16_BE_BOM) {
        return Ok(decode_utf16(&bytes[UTF16_BE_BOM.len()..], Endian::Be)?.into_bytes());
    }
    if let Some(endian) = sniff_utf16_no_bom(&bytes) {
        return Ok(decode_utf16(&bytes, endian)?.into_bytes());
    }
    Ok(bytes)
}

/// Decode raw XML bytes into a UTF-8 `String`, sniffing common Unicode
/// encodings per XML 1.0 §F.1. See [`decode_xml_to_utf8_bytes`].
pub fn decode_xml_bytes(bytes: Vec<u8>) -> SchemaResult<String> {
    let utf8 = decode_xml_to_utf8_bytes(bytes)?;
    String::from_utf8(utf8)
        .map_err(|e| SchemaError::resolution(format!("Invalid UTF-8 content: {}", e)))
}

fn sniff_utf16_no_bom(bytes: &[u8]) -> Option<Endian> {
    // XML 1.0 §F.1: with no BOM, the first four bytes of '<?' in UTF-16 LE
    // are `3C 00 ?? 00` and in UTF-16 BE are `00 3C 00 ??`. The non-null
    // third/fourth byte distinguishes UTF-16 from UTF-32.
    if bytes.len() < 4 {
        return None;
    }
    match (bytes[0], bytes[1]) {
        (0x3C, 0x00) if bytes[2] != 0x00 && bytes[3] == 0x00 => Some(Endian::Le),
        (0x00, 0x3C) if bytes[2] == 0x00 && bytes[3] != 0x00 => Some(Endian::Be),
        _ => None,
    }
}

fn decode_utf16(bytes: &[u8], endian: Endian) -> SchemaResult<String> {
    // `usize::is_multiple_of` was stabilized in Rust 1.87; the modulo keeps
    // the crate buildable on older toolchains.
    #[allow(clippy::manual_is_multiple_of)]
    if bytes.len() % 2 != 0 {
        return Err(SchemaError::resolution(
            "UTF-16 byte stream has an odd number of bytes".to_string(),
        ));
    }
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| match endian {
            Endian::Le => u16::from_le_bytes([c[0], c[1]]),
            Endian::Be => u16::from_be_bytes([c[0], c[1]]),
        })
        .collect();
    String::from_utf16(&units)
        .map_err(|e| SchemaError::resolution(format!("Invalid UTF-16 sequence: {}", e)))
}

// ============================================================================
// SchemaLoader Trait and Implementations
// ============================================================================

/// Trait for loading schema content from various sources.
///
/// Implementations can support file systems, HTTP, embedded resources, etc.
/// The loader chain uses priority to determine which loader handles a request.
pub trait SchemaLoader: Send + Sync + Debug {
    /// Load schema content from the given location.
    ///
    /// Returns the schema content as a string, or an error if loading fails.
    fn load(&self, location: &str) -> SchemaResult<String>;

    /// Check if this loader can handle the given location.
    ///
    /// Used by [`LoaderChain`] to find an appropriate loader.
    fn can_load(&self, location: &str) -> bool;

    /// Priority for loader chain (higher = checked first).
    ///
    /// Default is 0. Embedded loader uses 100 to be checked before file system.
    fn priority(&self) -> i32 {
        0
    }
}

/// File system schema loader (default).
///
/// Loads schemas from local file system paths.
#[derive(Debug, Clone, Default)]
pub struct FileSystemLoader {
    /// Base directory for resolving relative paths (not currently used directly)
    pub base_dir: Option<PathBuf>,
}

impl FileSystemLoader {
    /// Create a new file system loader.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a file system loader with a base directory.
    pub fn with_base_dir(base_dir: PathBuf) -> Self {
        Self {
            base_dir: Some(base_dir),
        }
    }
}

impl SchemaLoader for FileSystemLoader {
    /// Reads a file path, or the local file a `file:` URI names.
    fn load(&self, location: &str) -> SchemaResult<String> {
        let path = match crate::parser::uri::file_uri_to_path(location) {
            Some(path) => path,
            None => PathBuf::from(location),
        };
        let bytes = std::fs::read(&path).map_err(|e| {
            SchemaError::resolution(format!("Failed to read file '{}': {}", location, e))
        })?;
        decode_xml_bytes(bytes)
    }

    fn can_load(&self, location: &str) -> bool {
        match crate::parser::uri::scheme(location) {
            None => true,
            Some(_) => crate::parser::uri::file_uri_to_path(location).is_some(),
        }
    }

    fn priority(&self) -> i32 {
        0
    }
}

/// Embedded resource loader for built-in schemas.
///
/// Loads schemas from static assets embedded in the binary using the
/// `embedded://` URI scheme.
#[derive(Debug, Clone, Default)]
pub struct EmbeddedLoader;

impl EmbeddedLoader {
    /// Create a new embedded loader.
    pub fn new() -> Self {
        Self
    }
}

impl SchemaLoader for EmbeddedLoader {
    fn load(&self, location: &str) -> SchemaResult<String> {
        if let Some(rest) = location.strip_prefix("embedded://") {
            match rest {
                "xml.xsd" => {
                    let bytes = crate::embedded::XML_XSD;
                    String::from_utf8(bytes.to_vec()).map_err(|e| {
                        SchemaError::resolution(format!("Invalid UTF-8 in embedded schema: {}", e))
                    })
                }
                "xlink.xsd" => {
                    let bytes = crate::embedded::XLINK_XSD;
                    String::from_utf8(bytes.to_vec()).map_err(|e| {
                        SchemaError::resolution(format!("Invalid UTF-8 in embedded schema: {}", e))
                    })
                }
                _ => Err(SchemaError::resolution(format!(
                    "Unknown embedded schema: {}",
                    rest
                ))),
            }
        } else {
            Err(SchemaError::resolution(format!(
                "Not an embedded location: {}",
                location
            )))
        }
    }

    fn can_load(&self, location: &str) -> bool {
        location.starts_with("embedded://")
    }

    fn priority(&self) -> i32 {
        100 // High priority - checked before file system
    }
}

/// Composite loader that chains multiple loaders.
///
/// Loaders are tried in priority order (highest first) until one can handle
/// the requested location.
#[derive(Debug, Default)]
pub struct LoaderChain {
    loaders: Vec<Box<dyn SchemaLoader>>,
}

impl LoaderChain {
    /// Create a new empty loader chain.
    pub fn new() -> Self {
        Self {
            loaders: Vec::new(),
        }
    }

    /// Create a loader chain with default loaders (embedded + filesystem).
    pub fn with_defaults() -> Self {
        let mut chain = Self::new();
        chain.add(Box::new(EmbeddedLoader::new()));
        chain.add(Box::new(FileSystemLoader::new()));
        chain
    }

    /// Add a loader to the chain.
    ///
    /// Loaders are automatically sorted by priority (highest first).
    pub fn add(&mut self, loader: Box<dyn SchemaLoader>) {
        self.loaders.push(loader);
        self.loaders
            .sort_by_key(|b| std::cmp::Reverse(b.priority()));
    }

    /// Get the number of loaders in the chain.
    pub fn len(&self) -> usize {
        self.loaders.len()
    }

    /// Check if the chain is empty.
    pub fn is_empty(&self) -> bool {
        self.loaders.is_empty()
    }
}

impl SchemaLoader for LoaderChain {
    fn load(&self, location: &str) -> SchemaResult<String> {
        for loader in &self.loaders {
            if loader.can_load(location) {
                return loader.load(location);
            }
        }
        Err(SchemaError::resolution(format!(
            "No loader available for: {}",
            location
        )))
    }

    fn can_load(&self, location: &str) -> bool {
        self.loaders.iter().any(|l| l.can_load(location))
    }

    fn priority(&self) -> i32 {
        // Chain priority is max of all loaders
        self.loaders.iter().map(|l| l.priority()).max().unwrap_or(0)
    }
}

// ============================================================================
// Location rewriting
// ============================================================================

/// A location, as a [`LocationRewriter`] sees it: a directive's, or one
/// given to a [`SchemaSetBuilder`](crate::SchemaSetBuilder).
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct LocationRequest<'a> {
    /// The location as written: the directive's `schemaLocation`, or, for an
    /// `xs:import` the catalog maps, the catalog's location; the location
    /// given to the builder.
    pub location: &'a str,
    /// The base URI of the document that contains the directive — what
    /// `location` would be resolved against without a rewriter. A rewriter's
    /// answer is **not** resolved against it (see [`LocationRewriter`]). For
    /// a location given to the builder: the base of `try_add_relative`, else
    /// empty.
    pub base_uri: &'a str,
    /// The `namespace` of an `xs:import` (`None` for the other directives,
    /// and for an import without one).
    pub namespace: Option<&'a str>,
    /// The directive; `None` for a location given to the builder itself
    /// (`add`, `try_add`, `try_add_relative`, `add_async`, and the base URI
    /// of `add_source` / `add_bytes`).
    pub kind: Option<CompositionEdgeKind>,
}

/// Maps a location to the one to load, **before** it is resolved and
/// identified — in the style of an OASIS XML Catalog's `rewriteSystem` /
/// `uri` entries.
///
/// The resolver identifies a schema document by its resolved location (a
/// local file by its canonical path, however it is spelled: a path or a
/// `file:` URI), and reads each identity once. A [`SchemaLoader`] that maps
/// locations (a remote URL to a local copy, say) runs after that identity is
/// chosen, so two spellings of one document (`http:` and `https:`) still
/// load it twice, and a mapping that depends on the import's namespace (an
/// `?xsd=N` URL) cannot be made there. A rewriter sees the location as
/// written, the base URI and the namespace, and its answer is what is
/// identified and loaded. It has the resolver, for its
/// [`config`](SchemaResolver::config), [`catalog`](SchemaResolver::catalog)
/// and [`loader`](SchemaResolver::loader).
///
/// Any `Fn(&LocationRequest<'_>, &SchemaResolver) -> Option<String> + Send +
/// Sync` is a rewriter. `None` keeps the location as written, resolved
/// against the base URI as usual.
///
/// **The answer is final.** It is never resolved against the base URI of
/// the document that named the location, and never rewritten again:
///
/// * an absolute URI (`https://…`, `file:///…`, `embedded://…`) is loaded as
///   it is (normalized: scheme and host in lower case, dot segments removed);
/// * an absolute file path (`/opt/schemas/x.xsd`, `C:\schemas\x.xsd`) is
///   loaded as it is (in its canonical form when the file exists) — a remote
///   location can be mapped to a local mirror under any base, `http:` or
///   `file:`;
/// * a relative path is resolved against the process's working directory,
///   as a relative location given to
///   [`SchemaSetBuilder::add`](crate::SchemaSetBuilder::add) is.
///
/// A directive's answer is read without its `#fragment`, as a directive's
/// location is.
///
/// **It is asked once per location**: once per request (location, base URI,
/// namespace, directive), however many times the resolver needs the answer
/// (to check whether the document is loaded, to load it, to try the
/// catalog, to report a failure, to prefetch it asynchronously); the answer
/// is kept until [`SchemaResolver::set_location_rewriter`] replaces the
/// rewriter.
///
/// It also maps the locations given to the builder (`kind` is then `None`),
/// so that a document added under one spelling and named by a directive
/// under another is one document. The locations a set was loaded from, which
/// [`SchemaSetBuilder::add_from`](crate::SchemaSetBuilder::add_from) adds
/// again, are final already and are not rewritten.
///
/// ```
/// use xsd_schema::{LocationRequest, SchemaResolver, SchemaSetBuilder};
///
/// // rewriteSystem: one prefix to a local mirror, under both schemes. The
/// // answer is an absolute path: it is loaded as it is, whatever the base
/// // of the document that names the location.
/// let mirror = |request: &LocationRequest<'_>, _: &SchemaResolver| -> Option<String> {
///     let rest = request
///         .location
///         .strip_prefix("http://www.w3.org/2003/05/")
///         .or_else(|| request.location.strip_prefix("https://www.w3.org/2003/05/"))?;
///     Some(format!("/opt/schemas/w3c/{rest}.xsd"))
/// };
/// let mut builder = SchemaSetBuilder::new();
/// builder.set_location_rewriter(mirror);
/// ```
pub trait LocationRewriter: Send + Sync {
    /// The location to load for `request`, or `None` to keep it.
    fn rewrite(&self, request: &LocationRequest<'_>, resolver: &SchemaResolver) -> Option<String>;
}

/// A request to the rewriter: location as written, base URI, directive,
/// namespace.
type RewriteKey = (String, String, Option<CompositionEdgeKind>, Option<String>);

/// The kept answers of the rewriter (a poisoned lock still holds them: an
/// answer is inserted whole).
fn lock_rewrites(
    memo: &std::sync::Mutex<HashMap<RewriteKey, Option<String>>>,
) -> std::sync::MutexGuard<'_, HashMap<RewriteKey, Option<String>>> {
    memo.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl<F> LocationRewriter for F
where
    F: Fn(&LocationRequest<'_>, &SchemaResolver) -> Option<String> + Send + Sync,
{
    fn rewrite(&self, request: &LocationRequest<'_>, resolver: &SchemaResolver) -> Option<String> {
        self(request, resolver)
    }
}

// ============================================================================
// Schema Resolver
// ============================================================================

/// Schema resolver for loading external schema documents.
///
/// Uses a [`SchemaLoader`] chain to support multiple loading strategies
/// (file system, embedded assets, HTTP, etc.).
pub struct SchemaResolver {
    /// Configuration for resolution
    pub config: ResolverConfig,
    /// Set of locations currently being resolved (for cycle detection)
    resolving: HashSet<String>,
    /// Catalog for namespace-to-location mapping
    catalog: SchemaCatalog,
    /// Schema loader chain
    loader: Box<dyn SchemaLoader>,
    /// Maps directive locations before they are resolved (C15b).
    rewriter: Option<Box<dyn LocationRewriter>>,
    /// The rewriter's answers, by request: it is asked once per location.
    rewrites: std::sync::Mutex<HashMap<RewriteKey, Option<String>>>,
    /// Optional async loader for non-blocking I/O (HTTP, cloud storage, etc.)
    ///
    /// When set, async methods use this loader instead of wrapping the sync
    /// loader. When `None`, async methods fall back to the sync `loader`.
    #[cfg(feature = "async")]
    async_loader: Option<Box<dyn AsyncSchemaLoader>>,
    /// Content read asynchronously ahead of a synchronous resolution step,
    /// by resolved location (see `prefetch_directives`).
    #[cfg(feature = "async")]
    prefetched: std::collections::HashMap<String, SchemaResult<String>>,
}

/// Configuration for schema resolution
#[derive(Debug, Clone)]
pub struct ResolverConfig {
    /// Base directory for resolving relative paths
    pub base_dir: Option<PathBuf>,
    /// Whether to allow network access for HTTP URLs
    pub allow_network: bool,
    /// The longest chain of nested directives (`xs:include`, `xs:import`,
    /// `xs:redefine`, `xs:override`; `0` = unlimited). The documents given
    /// to the builder (or the pipeline) are at depth 0, a document a depth-n
    /// document's directive reads at depth n + 1, counted along the shortest
    /// chain of directives that reaches it; a document deeper than the limit
    /// is refused with a [`SchemaError::LimitExceeded`] (limit
    /// [`SchemaLimit::DirectiveDepth`]) at the directive that reads it.
    /// Default 100. `0` turns the limit off; the walk over directives does
    /// not recurse, so this limit bounds the work, not the stack.
    ///
    /// [`SchemaError::LimitExceeded`]: crate::SchemaError::LimitExceeded
    /// [`SchemaLimit::DirectiveDepth`]: crate::SchemaLimit::DirectiveDepth
    pub max_depth: usize,
    /// Parser configuration to use for resolved schemas
    pub parser_config: ParserConfig,
}

impl Default for ResolverConfig {
    fn default() -> Self {
        Self {
            base_dir: None,
            allow_network: false,
            max_depth: 100,
            parser_config: ParserConfig::default(),
        }
    }
}

/// Catalog for mapping namespaces to schema locations
#[derive(Debug, Clone, Default)]
pub struct SchemaCatalog {
    /// Namespace URI to schema location mapping
    entries: Vec<CatalogEntry>,
}

/// A single catalog entry
#[derive(Debug, Clone)]
pub struct CatalogEntry {
    /// Namespace URI
    pub namespace: String,
    /// Schema location (file path or URL)
    pub location: String,
}

impl SchemaCatalog {
    /// Create a new empty catalog
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an entry to the catalog
    pub fn add(&mut self, namespace: impl Into<String>, location: impl Into<String>) {
        self.entries.push(CatalogEntry {
            namespace: namespace.into(),
            location: location.into(),
        });
    }

    /// Look up a location by namespace
    pub fn lookup(&self, namespace: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| e.namespace == namespace)
            .map(|e| e.location.as_str())
    }

    /// Add well-known XML namespaces with embedded schema locations.
    ///
    /// Maps standard XML namespaces to `embedded://` URIs that are resolved
    /// by the [`EmbeddedLoader`].
    pub fn add_xml_catalog(&mut self) {
        // XML namespace (xml:lang, xml:space, xml:base, xml:id) - uses embedded schema
        self.add("http://www.w3.org/XML/1998/namespace", "embedded://xml.xsd");

        // XLink namespace (xlink:type, xlink:href, etc.) - uses embedded schema
        self.add("http://www.w3.org/1999/xlink", "embedded://xlink.xsd");

        // XML Schema instance namespace (xsi:type, xsi:nil, etc.)
        // Note: This could be embedded in the future
        self.add(
            "http://www.w3.org/2001/XMLSchema-instance",
            "http://www.w3.org/2001/XMLSchema-instance.xsd",
        );
    }
}

impl SchemaResolver {
    /// Create a new resolver with default configuration and loader chain.
    ///
    /// Uses [`LoaderChain::with_defaults()`] which includes:
    /// - [`EmbeddedLoader`] for `embedded://` URIs
    /// - [`FileSystemLoader`] for file paths
    pub fn new() -> Self {
        Self {
            config: ResolverConfig::default(),
            resolving: HashSet::new(),
            catalog: SchemaCatalog::new(),
            loader: Box::new(LoaderChain::with_defaults()),
            rewriter: None,
            rewrites: Default::default(),
            #[cfg(feature = "async")]
            async_loader: None,
            #[cfg(feature = "async")]
            prefetched: Default::default(),
        }
    }

    /// Create a resolver with the specified configuration.
    ///
    /// Uses the default loader chain.
    pub fn with_config(config: ResolverConfig) -> Self {
        Self {
            config,
            resolving: HashSet::new(),
            catalog: SchemaCatalog::new(),
            loader: Box::new(LoaderChain::with_defaults()),
            rewriter: None,
            rewrites: Default::default(),
            #[cfg(feature = "async")]
            async_loader: None,
            #[cfg(feature = "async")]
            prefetched: Default::default(),
        }
    }

    /// Create a resolver with a custom loader.
    ///
    /// # Example
    /// ```
    /// use xsd_schema::{SchemaResolver, LoaderChain};
    ///
    /// let loader = LoaderChain::with_defaults();
    /// let resolver = SchemaResolver::with_loader(Box::new(loader));
    /// ```
    pub fn with_loader(loader: Box<dyn SchemaLoader>) -> Self {
        Self {
            config: ResolverConfig::default(),
            resolving: HashSet::new(),
            catalog: SchemaCatalog::new(),
            loader,
            rewriter: None,
            rewrites: Default::default(),
            #[cfg(feature = "async")]
            async_loader: None,
            #[cfg(feature = "async")]
            prefetched: Default::default(),
        }
    }

    /// Create a resolver with custom configuration and loader.
    pub fn with_config_and_loader(config: ResolverConfig, loader: Box<dyn SchemaLoader>) -> Self {
        Self {
            config,
            resolving: HashSet::new(),
            catalog: SchemaCatalog::new(),
            loader,
            rewriter: None,
            rewrites: Default::default(),
            #[cfg(feature = "async")]
            async_loader: None,
            #[cfg(feature = "async")]
            prefetched: Default::default(),
        }
    }

    /// Create a resolver with a custom async loader for non-blocking I/O.
    ///
    /// The async loader is used by `load_content_async` and `load_schema_async`.
    /// The default sync loader chain is still used for sync methods.
    #[cfg(feature = "async")]
    pub fn with_async_loader(async_loader: Box<dyn AsyncSchemaLoader>) -> Self {
        Self {
            config: ResolverConfig::default(),
            resolving: HashSet::new(),
            catalog: SchemaCatalog::new(),
            loader: Box::new(LoaderChain::with_defaults()),
            rewriter: None,
            rewrites: Default::default(),
            async_loader: Some(async_loader),
            prefetched: Default::default(),
        }
    }

    /// Create a resolver with custom configuration and an async loader.
    #[cfg(feature = "async")]
    pub fn with_config_and_async_loader(
        config: ResolverConfig,
        async_loader: Box<dyn AsyncSchemaLoader>,
    ) -> Self {
        Self {
            config,
            resolving: HashSet::new(),
            catalog: SchemaCatalog::new(),
            loader: Box::new(LoaderChain::with_defaults()),
            rewriter: None,
            rewrites: Default::default(),
            async_loader: Some(async_loader),
            prefetched: Default::default(),
        }
    }

    /// Get a mutable reference to the catalog
    pub fn catalog_mut(&mut self) -> &mut SchemaCatalog {
        &mut self.catalog
    }

    /// The catalog (namespace → location).
    pub fn catalog(&self) -> &SchemaCatalog {
        &self.catalog
    }

    /// The configuration.
    pub fn config(&self) -> &ResolverConfig {
        &self.config
    }

    /// The (synchronous) loader the resolver reads documents with.
    pub fn loader(&self) -> &dyn SchemaLoader {
        self.loader.as_ref()
    }

    /// Map every location the resolver loads — each directive's, and each
    /// one given to a [`SchemaSetBuilder`](crate::SchemaSetBuilder) — through
    /// `rewriter` before it is resolved and identified (see
    /// [`LocationRewriter`]: its answer is final, and it is asked once per
    /// location); replaces a rewriter set before, and forgets its answers.
    pub fn set_location_rewriter(&mut self, rewriter: impl LocationRewriter + 'static) {
        self.rewriter = Some(Box::new(rewriter));
        self.rewrites
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }

    /// The location a directive loads: the rewriter's answer for it (or the
    /// location as written), resolved against the base URI.
    pub(crate) fn directive_target(
        &self,
        location: &str,
        base_uri: &str,
        kind: Option<CompositionEdgeKind>,
        namespace: Option<&str>,
    ) -> SchemaResult<String> {
        let resolved = self.rewritten_location(location, base_uri, kind, namespace)?;
        // A fragment identifies a part of a document ("a secondary resource",
        // RFC 3986 §3.5), not another one: a directive reads, and identifies,
        // the document without it. (A plain path's `#` is a file-name
        // character.)
        if kind.is_some() && crate::parser::uri::scheme(&resolved).is_some() {
            if let Some(hash) = resolved.find('#') {
                return Ok(resolved[..hash].to_string());
            }
        }
        Ok(resolved)
    }

    /// The location to load for `location`, named in the document at
    /// `base_uri`: the rewriter's answer, which is final (see
    /// [`final_location`](Self::final_location)), or, when there is none,
    /// `location` resolved against the base URI. The rewriter is asked once
    /// per request; its answer is kept.
    pub(crate) fn rewritten_location(
        &self,
        location: &str,
        base_uri: &str,
        kind: Option<CompositionEdgeKind>,
        namespace: Option<&str>,
    ) -> SchemaResult<String> {
        match self.rewrite(location, base_uri, kind, namespace) {
            Some(answer) => self.final_location(&answer),
            None => self.resolve_location(location, base_uri),
        }
    }

    /// The rewriter's answer for a request, asked once: a second request
    /// with the same location, base URI, directive and namespace gets the
    /// kept answer.
    fn rewrite(
        &self,
        location: &str,
        base_uri: &str,
        kind: Option<CompositionEdgeKind>,
        namespace: Option<&str>,
    ) -> Option<String> {
        let rewriter = self.rewriter.as_ref()?;
        let key: RewriteKey = (
            location.to_string(),
            base_uri.to_string(),
            kind,
            namespace.map(str::to_string),
        );
        if let Some(answer) = lock_rewrites(&self.rewrites).get(&key) {
            return answer.clone();
        }
        // Asked outside the lock: the rewriter may call back into the
        // resolver (`config`, `catalog`, `loader`).
        let answer = rewriter.rewrite(
            &LocationRequest {
                location,
                base_uri,
                namespace,
                kind,
            },
            self,
        );
        lock_rewrites(&self.rewrites).insert(key, answer.clone());
        answer
    }

    /// A rewriter's answer as the location to load. It is final: never
    /// resolved against the base URI of the document that named the
    /// original location.
    ///
    /// * An absolute URI is used as it is (normalized, RFC 3986 §6.2.2 /
    ///   §6.2.3, as every URI the resolver loads is).
    /// * An absolute file path is used as it is (in its canonical form when
    ///   the file exists, as every path the resolver loads is).
    /// * A relative path is resolved against the process's working
    ///   directory, as a relative location given to
    ///   [`SchemaSetBuilder::add`](crate::SchemaSetBuilder::add) is.
    fn final_location(&self, answer: &str) -> SchemaResult<String> {
        if is_absolute_uri(answer) {
            return self.resolve_location(answer, "");
        }
        match std::env::current_dir() {
            Ok(cwd) => Ok(canonical_absolute_path(
                normalize_path(&cwd.join(answer))
                    .to_string_lossy()
                    .into_owned(),
            )),
            Err(_) => Ok(answer.to_string()),
        }
    }

    /// Resolve a schema location against the base URI of the document that
    /// names it: the location a document is loaded from and identified by.
    ///
    /// * A location with a scheme, or one under a base with a scheme (`http:`,
    ///   `file:`, …), is an RFC 3986 URI reference: resolved as §5.2.2
    ///   "Transform References" resolves it (§5.2.3 merge, §5.2.4 dot
    ///   segments removed) and normalized as §6.2.2 / §6.2.3 normalize it —
    ///   scheme and host in lower case, percent-encodings of unreserved
    ///   characters decoded, an empty path after an authority `/`, a default
    ///   port dropped, `file:/p` and `file://localhost/p` spelled `file:///p`
    ///   — so that one document has one location however it is spelled.
    /// * Otherwise it is a file path, resolved against the base path's
    ///   directory and normalized lexically; an absolute path is then
    ///   replaced by its canonical form when the file exists (symlinks, case
    ///   variants). A relative path under a relative base stays relative: it
    ///   is not resolved against the process's working directory.
    ///
    /// ```
    /// use xsd_schema::SchemaResolver;
    ///
    /// let resolver = SchemaResolver::new();
    /// let at = |l: &str, b: &str| resolver.resolve_location(l, b).unwrap();
    /// assert_eq!(at("../common/t.xsd", "http://example.com/s/v1/main.xsd"),
    ///            "http://example.com/s/common/t.xsd");
    /// assert_eq!(at("/x.xsd", "HTTP://Example.com:80/s/main.xsd"), "http://example.com/x.xsd");
    /// assert_eq!(at("?xsd=2", "http://example.com/svc?wsdl"), "http://example.com/svc?xsd=2");
    /// assert_eq!(at("t.xsd", "file:///schemas/main.xsd"), "file:///schemas/t.xsd");
    /// ```
    pub fn resolve_location(&self, schema_location: &str, base_uri: &str) -> SchemaResult<String> {
        if crate::parser::uri::scheme(schema_location).is_some() {
            if schema_location.starts_with("embedded://") {
                return Ok(schema_location.to_string());
            }
            return Ok(crate::parser::uri::normalize(schema_location));
        }
        if crate::parser::uri::scheme(base_uri).is_some() {
            return Ok(crate::parser::uri::normalize(&crate::parser::uri::resolve(
                base_uri,
                schema_location,
            )));
        }
        // A file path. An absolute one stands on its own; a relative one is
        // resolved against the base path's directory.
        let resolved = if is_absolute_uri(schema_location) {
            schema_location.to_string()
        } else {
            resolve_relative_path(schema_location, base_uri)?
        };
        Ok(canonical_absolute_path(resolved))
    }

    /// Load and parse a schema from a location.
    ///
    /// Returns a [`LoadOutcome`] distinguishing freshly loaded, already loaded,
    /// and cycle-in-progress cases.
    ///
    /// If `chameleon_namespace` is `Some` and the loaded schema has no
    /// `targetNamespace`, the chameleon namespace is adopted per §4.2.3.
    pub fn load_schema(
        &mut self,
        location: &str,
        base_uri: &str,
        schema_set: &mut SchemaSet,
        chameleon_namespace: Option<NameId>,
    ) -> SchemaResult<LoadOutcome> {
        let kind = if chameleon_namespace.is_some() {
            CompositionEdgeKind::Include
        } else {
            CompositionEdgeKind::Import
        };
        self.load_directive(
            location,
            base_uri,
            schema_set,
            chameleon_namespace,
            Some(kind),
            None,
        )
    }

    /// [`load_schema`](Self::load_schema) for a directive of `kind`, with the
    /// import's `namespace` for the location rewriter.
    fn load_directive(
        &mut self,
        location: &str,
        base_uri: &str,
        schema_set: &mut SchemaSet,
        chameleon_namespace: Option<NameId>,
        kind: Option<CompositionEdgeKind>,
        namespace: Option<&str>,
    ) -> SchemaResult<LoadOutcome> {
        // Rewrite and resolve the location: what is read; its identity is
        // the document's key (one key however the location is spelled).
        let resolved = self.directive_target(location, base_uri, kind, namespace)?;
        self.load_target(resolved, schema_set, chameleon_namespace)
    }

    /// Load the document at `resolved` — a location already rewritten and
    /// resolved ([`directive_target`](Self::directive_target)), so neither
    /// again.
    fn load_target(
        &mut self,
        resolved: String,
        schema_set: &mut SchemaSet,
        chameleon_namespace: Option<NameId>,
    ) -> SchemaResult<LoadOutcome> {
        let identity = document_identity(&resolved);

        // Check if already loaded (chameleon-aware).
        if let Some(id) = check_loaded_cache(schema_set, &identity, chameleon_namespace) {
            return Ok(LoadOutcome::AlreadyLoaded(id));
        }

        // Check for circular resolution
        if self.resolving.contains(&identity) {
            // Circular include is allowed, just skip
            return Ok(LoadOutcome::Cycle(identity));
        }

        // Mark as being resolved (cycle detection)
        self.resolving.insert(identity.clone());

        // Load the schema content — clean up resolving set on error
        let content = match self.fetch_content(&resolved) {
            Ok(c) => c,
            Err(e) => {
                self.resolving.remove(&identity);
                return Err(e);
            }
        };

        // Parse the schema — clean up resolving set on error.
        // Apply chameleon namespace adoption if specified.
        let doc_id = match crate::parser::parse::parse_schema_with_chameleon(
            content.as_bytes(),
            &resolved,
            schema_set,
            &self.config.parser_config,
            chameleon_namespace,
        ) {
            Ok(id) => id,
            Err(e) => {
                self.resolving.remove(&identity);
                return Err(e);
            }
        };

        // Mark as loaded (chameleon-aware).
        mark_loaded_chameleon_aware(schema_set, &identity, doc_id, chameleon_namespace);

        // Remove from resolving set
        self.resolving.remove(&identity);

        Ok(LoadOutcome::Loaded(doc_id))
    }

    /// The content of `location`: what an asynchronous prefetch read for it,
    /// else what the loader chain reads.
    fn fetch_content(&self, location: &str) -> SchemaResult<String> {
        #[cfg(feature = "async")]
        if let Some(content) = self.prefetched.get(location) {
            return match content {
                Ok(text) => Ok(text.clone()),
                Err(e) => Err(e.duplicate()),
            };
        }
        self.load_content(location)
    }

    /// Load content from a location using the configured loader chain.
    ///
    /// Supports embedded://, file paths, and potentially HTTP (if configured).
    pub fn load_content(&self, location: &str) -> SchemaResult<String> {
        // Check network access for HTTP URLs
        if (location.starts_with("http://") || location.starts_with("https://"))
            && !self.config.allow_network
        {
            return Err(SchemaError::resolution(format!(
                "Network access not allowed for: {}",
                location
            )));
        }

        // Use the loader chain
        self.loader.load(location)
    }

    /// Process an include directive.
    ///
    /// Passes `target_namespace` as the chameleon namespace: if the included
    /// schema has no `targetNamespace`, it adopts the includer's (§4.2.3).
    pub fn process_include(
        &mut self,
        schema_location: &str,
        base_uri: &str,
        target_namespace: Option<NameId>,
        schema_set: &mut SchemaSet,
    ) -> SchemaResult<LoadOutcome> {
        self.load_directive(
            schema_location,
            base_uri,
            schema_set,
            target_namespace,
            Some(CompositionEdgeKind::Include),
            None,
        )
    }

    /// Process an import directive.
    ///
    /// Returns `Ok(None)` only when there is no `schemaLocation` and no
    /// catalog match (namespace-only import). All other paths return
    /// `Ok(Some(LoadOutcome))`.
    ///
    /// Per src-import (§4.2.6.1) the loaded schema's `targetNamespace` is
    /// validated against the directive's `namespace` attribute:
    /// 1.1 (`namespace` absent) — the imported schema must have an absent
    ///     `targetNamespace`.
    /// 1.2 (`namespace` present) — the imported schema's `targetNamespace`
    ///     must equal the directive's `namespace` value.
    pub fn process_import(
        &mut self,
        namespace: Option<&str>,
        schema_location: Option<&str>,
        base_uri: &str,
        schema_set: &mut SchemaSet,
    ) -> SchemaResult<Option<LoadOutcome>> {
        // Import does not do chameleon namespace adoption.
        //
        // Resolution order:
        //   1. Explicit `schemaLocation` (when loadable). The user's hint wins
        //      over a catalog entry whenever the file can actually be read,
        //      which is what saxon `over030` (xml-namespace overlay) relies
        //      on.
        //   2. Catalog redirect (used when no schemaLocation is given OR the
        //      schemaLocation cannot be loaded — typically because it points
        //      at a remote URL with network access disabled). This keeps
        //      embedded `xml`/`xlink` schemas in play for the W3C suite's
        //      `xsts.xsd` harness, which references `http://...xlink.xsd`.
        //   3. Namespace-only import (relies on the namespace being loaded
        //      elsewhere or provided externally).
        if let Some(location) = schema_location {
            match self.load_directive(
                location,
                base_uri,
                schema_set,
                None,
                Some(CompositionEdgeKind::Import),
                namespace,
            ) {
                Ok(outcome) => {
                    validate_import_target_namespace(schema_set, &outcome, namespace)?;
                    return Ok(Some(outcome));
                }
                Err(load_err) => {
                    if let Some(cat_loc) = namespace.and_then(|ns| self.catalog.lookup(ns)) {
                        let cat_loc = cat_loc.to_string();
                        return self
                            .try_catalog_load(&cat_loc, base_uri, namespace, schema_set)
                            .map_err(|catalog_err| {
                                if catalog_err.is_schema_content_error() {
                                    return catalog_err;
                                }
                                // Both failed and the catalog's location could
                                // not be read: say what happened to each.
                                let hint = if load_err.is_schema_content_error() {
                                    format!(
                                        "the location hint '{location}' is not a schema \
                                         document ({load_err})"
                                    )
                                } else {
                                    format!(
                                        "the location hint '{location}' could not be read \
                                         ({load_err})"
                                    )
                                };
                                SchemaError::resolution(format!(
                                    "{hint}; the catalog's location '{cat_loc}' for the \
                                     namespace could not be read: {catalog_err}"
                                ))
                            });
                    }
                    return Err(load_err);
                }
            }
        }

        if let Some(cat_loc) = namespace.and_then(|ns| self.catalog.lookup(ns)) {
            let cat_loc = cat_loc.to_string();
            return self.try_catalog_load(&cat_loc, base_uri, namespace, schema_set);
        }

        // Import without schemaLocation and no catalog entry is allowed
        // (the namespace might already be loaded or provided externally)
        Ok(None)
    }

    /// Load a catalog-resolved import target, validating namespace match.
    /// Returns `Ok(None)` when the catalog entry is already loaded, or when
    /// the requested namespace is already covered by a previously loaded
    /// schema document — embedded catalog schemas (xml.xsd, xlink.xsd) are
    /// only injected when nothing else has supplied that namespace, so a
    /// user-provided overlay (msData/additional `test264908_*` chain;
    /// saxon `over030`) does not collide with the embedded copy.
    fn try_catalog_load(
        &mut self,
        catalog_location: &str,
        base_uri: &str,
        namespace: Option<&str>,
        schema_set: &mut SchemaSet,
    ) -> SchemaResult<Option<LoadOutcome>> {
        // The catalog's location, rewritten and resolved once: checked, then
        // loaded as it is.
        let target = self.directive_target(
            catalog_location,
            base_uri,
            Some(CompositionEdgeKind::Import),
            namespace,
        );
        if target.as_ref().is_ok_and(|t| schema_set.is_loaded(t)) {
            return Ok(None);
        }
        if namespace_already_covered(schema_set, namespace) {
            return Ok(None);
        }
        let outcome = self.load_target(target?, schema_set, None)?;
        validate_import_target_namespace(schema_set, &outcome, namespace)?;
        Ok(Some(outcome))
    }

    /// Process a redefine directive.
    ///
    /// Passes `target_namespace` as the chameleon namespace: if the redefined
    /// schema has no `targetNamespace`, it adopts the redefiner's (§4.2.4).
    pub fn process_redefine(
        &mut self,
        schema_location: &str,
        base_uri: &str,
        target_namespace: Option<NameId>,
        schema_set: &mut SchemaSet,
    ) -> SchemaResult<LoadOutcome> {
        self.load_directive(
            schema_location,
            base_uri,
            schema_set,
            target_namespace,
            Some(CompositionEdgeKind::Redefine),
            None,
        )
    }

    /// Process an override directive (XSD 1.1).
    ///
    /// Passes `target_namespace` as the chameleon namespace: if the overridden
    /// schema has no `targetNamespace`, it adopts the overrider's (§4.2.5).
    #[cfg(feature = "xsd11")]
    pub fn process_override(
        &mut self,
        schema_location: &str,
        base_uri: &str,
        target_namespace: Option<NameId>,
        schema_set: &mut SchemaSet,
    ) -> SchemaResult<LoadOutcome> {
        self.load_directive(
            schema_location,
            base_uri,
            schema_set,
            target_namespace,
            Some(CompositionEdgeKind::Override),
            None,
        )
    }
}

impl Default for SchemaResolver {
    fn default() -> Self {
        Self::new()
    }
}

/// Validate that an `<xs:import>` directive's `namespace` attribute matches
/// the loaded schema's `targetNamespace`, per src-import (§4.2.6.1):
///
/// - 1.1: `namespace` absent ⇒ imported schema must have absent
///   `targetNamespace`.
/// - 1.2: `namespace` present ⇒ imported schema must have a `targetNamespace`
///   equal to that value.
///
/// Re-checks both freshly `Loaded` and `AlreadyLoaded` outcomes so a stale
/// duplicate import that disagrees with a previously-loaded document fails
/// the same way as the first import would.
/// Whether any already-loaded schema document declares (or chameleon-adopts)
/// the given namespace URI. Used to suppress a redundant catalog fallback
/// that would otherwise inject the embedded copy of `xml.xsd` / `xlink.xsd`
/// on top of a user-supplied schema for the same namespace (msData
/// `test264908_*` chain; saxon `over030`).
fn namespace_already_covered(schema_set: &SchemaSet, namespace: Option<&str>) -> bool {
    let Some(ns_str) = namespace else {
        return false;
    };
    let Some(ns_id) = schema_set.name_table.get(ns_str) else {
        return false;
    };
    schema_set
        .documents
        .iter()
        .any(|d| d.declared_target_namespace == Some(ns_id) || d.target_namespace == Some(ns_id))
}

fn validate_import_target_namespace(
    schema_set: &SchemaSet,
    outcome: &LoadOutcome,
    namespace: Option<&str>,
) -> SchemaResult<()> {
    let doc_id = match outcome {
        LoadOutcome::Loaded(id) | LoadOutcome::AlreadyLoaded(id) => *id,
        LoadOutcome::Cycle(_) => return Ok(()),
    };
    let Some(doc) = schema_set.documents.get(doc_id as usize) else {
        return Ok(());
    };
    let imported_tns = doc
        .target_namespace
        .map(|n| schema_set.name_table.resolve_ref(n));
    if namespace == imported_tns {
        return Ok(());
    }
    let msg = match (namespace, imported_tns) {
        (None, Some(tns)) => format!(
            "Import directive has no namespace attribute, but imported schema has \
             targetNamespace='{}' (src-import clause 1.1 requires absent targetNamespace)",
            tns
        ),
        (Some(ns), None) => format!(
            "Import directive namespace='{}' does not match imported schema's absent \
             targetNamespace (src-import clause 1.2)",
            ns
        ),
        (Some(ns), Some(tns)) => format!(
            "Import directive namespace='{}' does not match imported schema's \
             targetNamespace='{}' (src-import clause 1.2)",
            ns, tns
        ),
        (None, None) => unreachable!("handled by early return above"),
    };
    Err(SchemaError::structural("src-import", msg, None))
}

// ============================================================================
// Async Schema Loading (feature = "async")
// ============================================================================

/// Trait for loading schema content asynchronously.
///
/// Implementations can provide truly non-blocking I/O for HTTP, cloud storage,
/// or other async sources. Pass a `Box<dyn AsyncSchemaLoader>` to
/// [`SchemaResolver::with_async_loader`] to enable async loading.
///
/// When no async loader is configured, async resolver methods fall back to the
/// sync [`SchemaLoader`] (blocking the current task).
///
/// The trait is object-safe (`Pin<Box<dyn Future>>`), so it can be stored as
/// `Box<dyn AsyncSchemaLoader>` without conflicting with sync trait impls.
#[cfg(feature = "async")]
pub trait AsyncSchemaLoader: Send + Sync + Debug {
    /// Load schema content asynchronously from the given location.
    fn load_async(
        &self,
        location: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = SchemaResult<String>> + Send + '_>>;

    /// Check if this loader can handle the given location.
    fn can_load(&self, location: &str) -> bool;
}

#[cfg(feature = "async")]
impl SchemaResolver {
    /// Load content asynchronously from a location.
    ///
    /// Uses the [`AsyncSchemaLoader`] if one was provided via
    /// [`with_async_loader`](SchemaResolver::with_async_loader); otherwise
    /// falls back to the sync [`SchemaLoader`].
    pub async fn load_content_async(&self, location: &str) -> SchemaResult<String> {
        // Check network access for HTTP URLs
        if (location.starts_with("http://") || location.starts_with("https://"))
            && !self.config.allow_network
        {
            return Err(SchemaError::resolution(format!(
                "Network access not allowed for: {}",
                location
            )));
        }

        // Use the async loader only when it can handle this location;
        // otherwise fall back to the sync loader chain (embedded, filesystem, etc.)
        if let Some(ref async_loader) = self.async_loader {
            if async_loader.can_load(location) {
                return async_loader.load_async(location).await;
            }
        }
        self.loader.load(location)
    }

    /// Load and parse a schema asynchronously from a location.
    ///
    /// Returns a [`LoadOutcome`] distinguishing freshly loaded, already loaded,
    /// and cycle-in-progress cases. The content is read with the async
    /// loader; everything else is [`load_schema`](Self::load_schema).
    pub async fn load_schema_async(
        &mut self,
        location: &str,
        base_uri: &str,
        schema_set: &mut SchemaSet,
        chameleon_namespace: Option<NameId>,
    ) -> SchemaResult<LoadOutcome> {
        let kind = if chameleon_namespace.is_some() {
            CompositionEdgeKind::Include
        } else {
            CompositionEdgeKind::Import
        };
        let resolved = self.directive_target(location, base_uri, Some(kind), None)?;
        // Decide what to read before the await: the future must not hold the
        // (`!Sync`) schema set across it.
        let wanted = self.wants(&resolved, schema_set, chameleon_namespace);
        if wanted {
            self.prefetch_locations(vec![resolved]).await;
        }
        let outcome = self.load_schema(location, base_uri, schema_set, chameleon_namespace);
        self.prefetched.clear();
        outcome
    }

    /// Whether a synchronous step that loads `resolved` would read it: not
    /// loaded already, not being loaded, not read already.
    fn wants(
        &self,
        resolved: &str,
        schema_set: &SchemaSet,
        chameleon_namespace: Option<NameId>,
    ) -> bool {
        let identity = document_identity(resolved);
        check_loaded_cache(schema_set, &identity, chameleon_namespace).is_none()
            && !self.resolving.contains(&identity)
            && !self.prefetched.contains_key(resolved)
    }

    /// Read `locations` with the async loader, ahead of a synchronous step
    /// that loads them. Owned data only: the future holds no borrow of the
    /// schema set, so it is `Send`.
    pub(crate) async fn prefetch_locations(&mut self, locations: Vec<String>) {
        for location in locations {
            if self.prefetched.contains_key(&location) {
                continue;
            }
            let content = self.load_content_async(&location).await;
            self.prefetched.insert(location, content);
        }
    }

    /// The documents the directives of `doc_id` may read, so that
    /// [`resolve_all_directives`] — the one implementation of the directive
    /// rules — then finds their content without blocking: every directive's
    /// target, and for an import whose namespace the catalog maps the
    /// catalog's location too (the synchronous rules turn to it when the hint
    /// cannot be read, or is not a schema document). Computed synchronously,
    /// before anything is awaited.
    pub(crate) fn prefetch_plan(&self, doc_id: DocumentId, schema_set: &SchemaSet) -> Vec<String> {
        let Some(doc) = schema_set.documents.get(doc_id as usize) else {
            return Vec::new();
        };
        let base_uri = doc.base_uri.as_str();
        let target_namespace = doc.target_namespace;
        let mut plan: Vec<String> = Vec::new();
        let want = |resolved: String, chameleon: Option<NameId>, plan: &mut Vec<String>| {
            if self.wants(&resolved, schema_set, chameleon) && !plan.contains(&resolved) {
                plan.push(resolved);
            }
        };
        let mut chameleon: Vec<(&str, CompositionEdgeKind)> = doc
            .includes
            .iter()
            .map(|d| (d.schema_location.as_str(), CompositionEdgeKind::Include))
            .chain(
                doc.redefines
                    .iter()
                    .map(|d| (d.schema_location.as_str(), CompositionEdgeKind::Redefine)),
            )
            .collect();
        #[cfg(feature = "xsd11")]
        chameleon.extend(
            doc.overrides
                .iter()
                .map(|d| (d.schema_location.as_str(), CompositionEdgeKind::Override)),
        );
        for (location, kind) in chameleon {
            if let Ok(resolved) = self.directive_target(location, base_uri, Some(kind), None) {
                want(resolved, target_namespace, &mut plan);
            }
        }
        for import in &doc.imports {
            let namespace = import.namespace.as_deref();
            let kind = Some(CompositionEdgeKind::Import);
            if let Some(location) = import.schema_location.as_deref() {
                if let Ok(resolved) = self.directive_target(location, base_uri, kind, namespace) {
                    want(resolved, None, &mut plan);
                }
            }
            if let Some(catalog) = namespace.and_then(|ns| self.catalog.lookup(ns)) {
                if let Ok(resolved) = self.directive_target(catalog, base_uri, kind, namespace) {
                    want(resolved, None, &mut plan);
                }
            }
        }
        plan
    }

    /// Forget what [`prefetch_locations`](Self::prefetch_locations) read.
    pub(crate) fn clear_prefetched(&mut self) {
        self.prefetched.clear();
    }
}

/// Resolve all directives in a schema document asynchronously.
///
/// The same rules and outcome as [`resolve_all_directives`], which it runs
/// once the documents the directives name have been read with the async
/// loader.
#[cfg(feature = "async")]
pub async fn resolve_all_directives_async(
    doc_id: DocumentId,
    resolver: &mut SchemaResolver,
    schema_set: &mut SchemaSet,
) -> ResolutionResult {
    let plan = resolver.prefetch_plan(doc_id, schema_set);
    resolver.prefetch_locations(plan).await;
    let result = resolve_all_directives(doc_id, resolver, schema_set);
    resolver.clear_prefetched();
    result
}

/// Check if a URI is absolute (has a scheme)
fn is_absolute_uri(uri: &str) -> bool {
    crate::parser::uri::scheme(uri).is_some()
        || (cfg!(windows) && uri.len() >= 2 && &uri[1..2] == ":")
        || uri.starts_with('/')
        || (cfg!(windows) && uri.starts_with('\\'))
}

/// Resolve a relative file path against a base file path
fn resolve_relative_path(relative: &str, base: &str) -> SchemaResult<String> {
    let base_path = Path::new(base);
    let base_dir = base_path.parent().unwrap_or(Path::new("."));
    let resolved = base_dir.join(relative);

    // Normalize the path
    let normalized = normalize_path(&resolved);

    Ok(normalized.to_string_lossy().into_owned())
}

/// Map a resolved plain file path to its on-disk canonical form when the
/// file exists. Schema-document identity (dedup in `loaded_locations`)
/// keys on this string, so two spellings of the same file — e.g. case
/// variants on a case-insensitive filesystem (msData schZ012_b/c) or a
/// symlinked path — must collapse to one location. Non-file locations
/// (http, embedded, file:// URIs) and missing files pass through.
pub(crate) fn canonicalize_file_location(resolved: String) -> String {
    document_identity(&resolved)
}

/// The identity of the document at `location` — the key it is loaded under
/// (`SchemaSet::loaded_locations`), the same however the location is spelled:
///
/// * a local `file:` URI is the file it names, canonicalized (`file:///d/a.xsd`
///   and `/d/a.xsd` are one document; a fragment is kept, as a `#` in the
///   path);
/// * a file path, absolute or relative, is canonicalized when the file
///   exists (symlinks, case variants on a case-insensitive file system). A
///   relative path is canonicalized for the key only: the document is still
///   read, and its directives resolved, under the relative location;
/// * any other URI is itself (normalized by `resolve_location`).
pub(crate) fn document_identity(location: &str) -> String {
    let path = match crate::parser::uri::scheme(location) {
        Some(_) => match crate::parser::uri::file_uri_to_path(location) {
            Some(path) => {
                let fragment = location.split_once('#').map(|(_, f)| f);
                let canonical = std::fs::canonicalize(&path).unwrap_or(path);
                let mut key = canonical.to_string_lossy().into_owned();
                if let Some(fragment) = fragment {
                    key.push('#');
                    key.push_str(fragment);
                }
                return key;
            }
            None => return location.to_string(),
        },
        None => Path::new(location),
    };
    match std::fs::canonicalize(path) {
        Ok(canonical) => canonical.to_string_lossy().into_owned(),
        Err(_) => location.to_string(),
    }
}

/// The location a plain path is read from: canonical when it is absolute and
/// the file exists, as written otherwise (a relative location is not
/// resolved against the working directory).
fn canonical_absolute_path(resolved: String) -> String {
    if crate::parser::uri::scheme(&resolved).is_some() || !Path::new(&resolved).is_absolute() {
        return resolved;
    }
    match std::fs::canonicalize(Path::new(&resolved)) {
        Ok(canonical) => canonical.to_string_lossy().into_owned(),
        Err(_) => resolved,
    }
}

/// Lexically normalize a path: drop `.` components and let each `..` cancel
/// the normal segment before it.
///
/// A `..` with no normal segment to cancel is kept when the path is relative
/// (`../a` stays `../a`, `a/../..` becomes `..`), because it refers to a
/// directory outside the part of the path that is spelled out. Above a root it
/// is dropped (`/..` is `/`). The filesystem is not consulted, so symlinks are
/// not taken into account.
pub(crate) fn normalize_path(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match result.components().next_back() {
                Some(Component::Normal(_)) => {
                    result.pop();
                }
                Some(Component::RootDir) => {}
                // Empty, `..`, or a drive-relative prefix such as `C:`.
                _ => result.push(component),
            },
            _ => result.push(component),
        }
    }

    result
}

/// Result of resolving all directives in a schema
#[derive(Debug, Default)]
pub struct ResolutionResult {
    /// Document IDs of successfully loaded schemas
    pub loaded: Vec<DocumentId>,
    /// Errors from include/redefine/override directives
    pub errors: Vec<SchemaError>,
    /// Errors from xs:import directives
    pub import_errors: Vec<SchemaError>,
    /// Schemas that were already loaded (circular references)
    pub skipped: Vec<String>,
}

impl ResolutionResult {
    /// Check if resolution was fully successful
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty() && self.import_errors.is_empty()
    }

    /// Check if any schemas were loaded
    pub fn has_loaded(&self) -> bool {
        !self.loaded.is_empty()
    }
}

/// A directive whose schema document could not be read (see
/// [`SchemaSet::directive_failures`]).
///
/// `Display`: `xs:include schemaLocation="missing.xsd" at main.xsd:2:3: the
/// schema document could not be read: Failed to read file '…': …`.
#[derive(Debug)]
#[non_exhaustive]
pub struct DirectiveFailure {
    kind: CompositionEdgeKind,
    schema_location: Option<String>,
    namespace: Option<String>,
    resolved_location: Option<String>,
    location: Option<crate::parser::location::SourceLocation>,
    error: SchemaError,
    namespace_supplied: bool,
}

impl DirectiveFailure {
    /// The directive: include, import, redefine or override.
    pub fn kind(&self) -> CompositionEdgeKind {
        self.kind
    }

    /// The `schemaLocation` as written, or, for an import without one that
    /// a catalog entry mapped, `None`.
    pub fn schema_location(&self) -> Option<&str> {
        self.schema_location.as_deref()
    }

    /// The `namespace` of an `xs:import`.
    pub fn namespace(&self) -> Option<&str> {
        self.namespace.as_deref()
    }

    /// The location that was read and failed: the location rewriter's
    /// answer for the `schemaLocation`, if one is set (final, see
    /// [`LocationRewriter`]), else the `schemaLocation` resolved against the
    /// including document's base URI; for an `xs:import` whose namespace the
    /// catalog maps, the catalog's location (tried after the hint, so the one
    /// whose failure is reported).
    pub fn resolved_location(&self) -> Option<&str> {
        self.resolved_location.as_deref()
    }

    /// Whether another document of the set supplies the namespace of this
    /// `xs:import` — a location hint failed, but the namespace's components
    /// are not missing for that (they may still be incomplete). Always
    /// `false` for the other directives.
    pub fn namespace_supplied(&self) -> bool {
        self.namespace_supplied
    }

    /// Where the directive is, in the document that contains it.
    pub fn location(&self) -> Option<&crate::parser::location::SourceLocation> {
        self.location.as_ref()
    }

    /// Why the document could not be read (a resolution or I/O error).
    pub fn error(&self) -> &SchemaError {
        &self.error
    }

    /// Set [`namespace_supplied`](Self::namespace_supplied) from whether a
    /// namespace is supplied by a document of the set.
    pub(crate) fn refresh_supplied(&mut self, supplied: impl Fn(&str) -> bool) {
        self.namespace_supplied = self.kind == CompositionEdgeKind::Import
            && self.namespace.as_deref().is_some_and(supplied);
    }
}

impl std::fmt::Display for DirectiveFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let element = match self.kind {
            CompositionEdgeKind::Include => "xs:include",
            CompositionEdgeKind::Import => "xs:import",
            CompositionEdgeKind::Redefine => "xs:redefine",
            #[cfg(feature = "xsd11")]
            CompositionEdgeKind::Override => "xs:override",
        };
        f.write_str(element)?;
        if let Some(namespace) = &self.namespace {
            write!(f, " namespace=\"{namespace}\"")?;
        }
        if let Some(location) = &self.schema_location {
            write!(f, " schemaLocation=\"{location}\"")?;
        }
        if let Some(at) = &self.location {
            write!(f, " at {at}")?;
        }
        write!(f, ": the schema document could not be read: {}", self.error)?;
        if self.namespace_supplied {
            f.write_str(" (the namespace is supplied by another document of the set)")?;
        }
        Ok(())
    }
}

/// Keep a directive's failure on the set, unless it is a schema error
/// (the document was read but is not a usable schema document), which fails
/// the compilation instead.
#[allow(clippy::too_many_arguments)]
fn record_failure(
    schema_set: &mut SchemaSet,
    resolver: &SchemaResolver,
    kind: CompositionEdgeKind,
    schema_location: Option<&str>,
    namespace: Option<&str>,
    base_uri: &str,
    source: Option<&crate::parser::location::SourceRef>,
    error: &SchemaError,
) {
    if error.is_schema_content_error() {
        return;
    }
    // An import whose namespace the catalog maps failed on the catalog's
    // location (tried after the hint): name it.
    let catalog = match kind {
        CompositionEdgeKind::Import => namespace.and_then(|ns| resolver.catalog.lookup(ns)),
        _ => None,
    };
    let resolved_location = match catalog.or(schema_location) {
        Some(location) => resolver
            .directive_target(location, base_uri, Some(kind), namespace)
            .ok(),
        None => None,
    };
    let location = source.and_then(|s| schema_set.source_maps.locate(s));
    schema_set.record_directive_failure(DirectiveFailure {
        kind,
        schema_location: schema_location.map(str::to_string),
        namespace: namespace.map(str::to_string),
        resolved_location,
        location,
        error: error.duplicate(),
        namespace_supplied: false,
    });
}

/// The outcome of resolving the directives of a set of documents and of
/// every document they read, transitively.
#[derive(Debug, Default)]
pub(crate) struct DirectiveClosure {
    /// The documents read, in the order they were read.
    pub loaded: Vec<DocumentId>,
    /// Documents skipped (already loaded or being loaded).
    pub skipped_count: usize,
    /// Errors of `xs:include` / `xs:redefine` / `xs:override`.
    pub errors: Vec<SchemaError>,
    /// Errors of `xs:import`.
    pub import_errors: Vec<SchemaError>,
    /// Documents met deeper than `ResolverConfig::max_depth` and not walked
    /// yet: (the document whose directive read it, the document).
    deferred: Vec<(DocumentId, DocumentId)>,
}

impl DirectiveClosure {
    /// The first error that makes the schema invalid (a document that was
    /// read but is not a usable schema document, a nesting limit), if any.
    pub fn into_first_schema_error(self) -> Option<SchemaError> {
        self.errors
            .into_iter()
            .chain(self.import_errors)
            .find(|e| e.is_schema_content_error())
    }
}

/// One document of a directive closure on the way down: its directives'
/// outcome, and the next of the documents they read to descend into.
struct ClosureFrame {
    doc: DocumentId,
    depth: usize,
    result: Option<ResolutionResult>,
    next: usize,
}

impl ClosureFrame {
    fn new(doc: DocumentId, depth: usize) -> Self {
        ClosureFrame {
            doc,
            depth,
            result: None,
            next: 0,
        }
    }
}

/// The error for a document nested deeper than `ResolverConfig::max_depth`,
/// at the directive of `parent` that reads it.
fn directive_depth_error(
    schema_set: &SchemaSet,
    parent: DocumentId,
    child: DocumentId,
    depth: usize,
    limit: usize,
) -> SchemaError {
    let edge = schema_set
        .composition_edges
        .iter()
        .rev()
        .find(|e| e.source_doc == parent && e.target_doc == Some(child));
    let location = edge.and_then(|e| {
        e.source
            .as_ref()
            .and_then(|s| schema_set.source_maps.locate(s))
    });
    let name = schema_set
        .documents
        .get(child as usize)
        .map_or("?", |d| d.base_uri.as_str());
    SchemaError::limit(
        crate::error::SchemaLimit::DirectiveDepth,
        limit,
        depth,
        format!(
            "'{name}' is read through {depth} nested xs:include / xs:import / xs:redefine / \
             xs:override directives"
        ),
        location,
    )
}

/// Resolve the directives of `roots` and, depth first, of every document
/// they read — the one directive walk of the builder and the pipeline.
///
/// Iterative, in the order of a recursive walk: a document's directives are
/// resolved, then those of each document they read, and a document's errors
/// are collected after those of the documents it read. A document deeper
/// than `ResolverConfig::max_depth` is refused.
pub(crate) fn resolve_directive_closure(
    roots: &[DocumentId],
    resolver: &mut SchemaResolver,
    schema_set: &mut SchemaSet,
) -> DirectiveClosure {
    let mut out = DirectiveClosure::default();
    let mut starts: Vec<(DocumentId, usize)> = roots.iter().map(|&r| (r, 0)).collect();
    loop {
        for (root, depth) in std::mem::take(&mut starts) {
            let mut stack = vec![ClosureFrame::new(root, depth)];
            while let Some(top) = stack.last_mut() {
                if top.result.is_none() {
                    let result = resolve_all_directives(top.doc, resolver, schema_set);
                    out.loaded.extend(result.loaded.iter().copied());
                    out.skipped_count += result.skipped.len();
                    top.result = Some(result);
                }
                if let Some(step) = closure_step(top, resolver.config.max_depth, &mut out) {
                    stack.push(step);
                    continue;
                }
                let frame = stack.pop().expect("non-empty");
                let result = frame.result.expect("resolved");
                out.errors.extend(result.errors);
                out.import_errors.extend(result.import_errors);
            }
        }
        match resume_deferred(roots, resolver.config.max_depth, schema_set, &mut out) {
            Some(resumed) => starts = resumed,
            None => break,
        }
    }
    schema_set.refresh_directive_failures();
    out
}

/// The asynchronous [`resolve_directive_closure`]: the documents each step
/// reads are read with the async loader first.
#[cfg(feature = "async")]
pub(crate) async fn resolve_directive_closure_async(
    roots: &[DocumentId],
    resolver: &mut SchemaResolver,
    schema_set: &mut SchemaSet,
) -> DirectiveClosure {
    let mut out = DirectiveClosure::default();
    let mut starts: Vec<(DocumentId, usize)> = roots.iter().map(|&r| (r, 0)).collect();
    loop {
        for (root, depth) in std::mem::take(&mut starts) {
            let mut stack = vec![ClosureFrame::new(root, depth)];
            while let Some(top) = stack.last_mut() {
                if top.result.is_none() {
                    let doc = top.doc;
                    let plan = resolver.prefetch_plan(doc, schema_set);
                    resolver.prefetch_locations(plan).await;
                    let result = resolve_all_directives(doc, resolver, schema_set);
                    resolver.clear_prefetched();
                    out.loaded.extend(result.loaded.iter().copied());
                    out.skipped_count += result.skipped.len();
                    // The await above ended the borrow of the stack.
                    let top = stack.last_mut().expect("non-empty");
                    top.result = Some(result);
                }
                let top = stack.last_mut().expect("non-empty");
                if let Some(step) = closure_step(top, resolver.config.max_depth, &mut out) {
                    stack.push(step);
                    continue;
                }
                let frame = stack.pop().expect("non-empty");
                let result = frame.result.expect("resolved");
                out.errors.extend(result.errors);
                out.import_errors.extend(result.import_errors);
            }
        }
        match resume_deferred(roots, resolver.config.max_depth, schema_set, &mut out) {
            Some(resumed) => starts = resumed,
            None => break,
        }
    }
    schema_set.refresh_directive_failures();
    out
}

/// The next document `top`'s directives read to descend into, or `None`
/// when all have been visited. A document met deeper than the limit is not
/// descended into yet: it is kept in `out.deferred` (it may be nearer the
/// roots through another path, which `resume_deferred` decides).
fn closure_step(
    top: &mut ClosureFrame,
    limit: usize,
    out: &mut DirectiveClosure,
) -> Option<ClosureFrame> {
    let loaded = &top.result.as_ref()?.loaded;
    while let Some(&child) = loaded.get(top.next) {
        top.next += 1;
        let depth = top.depth + 1;
        if limit != 0 && depth > limit {
            out.deferred.push((top.doc, child));
            continue;
        }
        return Some(ClosureFrame::new(child, depth));
    }
    None
}

/// After a walk: the deferred documents whose shortest distance from the
/// roots (through the composition edges recorded so far) is within the
/// limit, to walk next from that depth — or `None` when there are none, in
/// which case every document still deferred is refused. A document's
/// directive depth is thus its shortest distance from the documents added,
/// whatever order the depth-first walk meets it in.
fn resume_deferred(
    roots: &[DocumentId],
    limit: usize,
    schema_set: &SchemaSet,
    out: &mut DirectiveClosure,
) -> Option<Vec<(DocumentId, usize)>> {
    if out.deferred.is_empty() {
        return None;
    }
    // Breadth-first distances over the composition graph.
    let mut depth: HashMap<DocumentId, usize> = HashMap::new();
    let mut queue: std::collections::VecDeque<DocumentId> = std::collections::VecDeque::new();
    for &root in roots {
        if depth.insert(root, 0).is_none() {
            queue.push_back(root);
        }
    }
    let mut targets: HashMap<DocumentId, Vec<DocumentId>> = HashMap::new();
    for edge in &schema_set.composition_edges {
        if let Some(target) = edge.target_doc {
            targets.entry(edge.source_doc).or_default().push(target);
        }
    }
    while let Some(doc) = queue.pop_front() {
        let next = depth[&doc] + 1;
        for &target in targets.get(&doc).map(Vec::as_slice).unwrap_or(&[]) {
            if let std::collections::hash_map::Entry::Vacant(slot) = depth.entry(target) {
                slot.insert(next);
                queue.push_back(target);
            }
        }
    }
    let deferred = std::mem::take(&mut out.deferred);
    let mut resumed: Vec<(DocumentId, usize)> = Vec::new();
    let mut refused: Vec<(DocumentId, DocumentId)> = Vec::new();
    for (parent, child) in deferred {
        match depth.get(&child) {
            Some(&d) if d <= limit => {
                if !resumed.iter().any(|&(c, _)| c == child) {
                    resumed.push((child, d));
                }
            }
            _ => refused.push((parent, child)),
        }
    }
    if resumed.is_empty() {
        for (parent, child) in refused {
            let found = depth.get(&child).copied().unwrap_or(limit + 1);
            out.errors.push(directive_depth_error(
                schema_set, parent, child, found, limit,
            ));
        }
        return None;
    }
    out.deferred = refused;
    Some(resumed)
}

/// A schema error of a directive's target that carries no location (a
/// target-namespace mismatch, say) is located at the directive. Resolution
/// and I/O errors carry none by their nature and are left alone.
fn locate_at(
    error: SchemaError,
    schema_set: &SchemaSet,
    source: Option<&crate::parser::location::SourceRef>,
) -> SchemaError {
    if !error.is_schema_content_error() {
        return error;
    }
    match source.and_then(|s| schema_set.source_maps.locate(s)) {
        Some(location) => error.with_location(location),
        None => error,
    }
}

/// Record a composition edge from a [`LoadOutcome`].
///
/// Edges are always recorded. For `Cycle` outcomes, `target_doc` is `None`
/// and will be filled in by [`fixup_composition_edges`] after resolution.
fn record_edge(
    schema_set: &mut SchemaSet,
    source_doc: DocumentId,
    outcome: &LoadOutcome,
    kind: CompositionEdgeKind,
    source: Option<&crate::parser::location::SourceRef>,
    schema_location: &str,
) {
    let (target_doc, resolved_location) = match outcome {
        LoadOutcome::Loaded(id) | LoadOutcome::AlreadyLoaded(id) => {
            // The resolved URI is stored as the document's base_uri during parsing.
            let loc = schema_set.documents[*id as usize].base_uri.clone();
            (Some(*id), loc)
        }
        LoadOutcome::Cycle(resolved) => (None, resolved.clone()),
    };
    schema_set.composition_edges.push(CompositionEdge {
        source_doc,
        target_doc,
        resolved_location,
        kind,
        source: source.cloned(),
        schema_location: schema_location.to_string(),
    });
}

/// Check the chameleon-aware loaded-location caches for a previously loaded
/// document.  Returns `Some(doc_id)` when the cached document is compatible
/// with the requested `chameleon_namespace`, `None` otherwise.
///
/// Shared by both `load_schema` (sync) and `load_schema_async`.
fn check_loaded_cache(
    schema_set: &SchemaSet,
    resolved: &str,
    chameleon_namespace: Option<NameId>,
) -> Option<DocumentId> {
    // Check chameleon-specific cache first.
    if let Some(ns) = chameleon_namespace {
        if let Some(&id) = schema_set.chameleon_cache.get(&(resolved.to_owned(), ns)) {
            return Some(id);
        }
    }
    // Then check primary cache with reusability check.
    if let Some(&id) = schema_set.loaded_locations.get(resolved) {
        let reusable = schema_set.documents.get(id as usize).is_none_or(|doc| {
            if doc.is_chameleon() {
                false
            } else if doc.target_namespace.is_some() {
                true
            } else {
                // Raw no-namespace document — reusable only when no
                // chameleon adoption is requested (§4.2.3).
                chameleon_namespace.is_none()
            }
        });
        if reusable {
            return Some(id);
        }
    }
    None
}

/// Record a freshly loaded document in the appropriate caches.
///
/// Chameleon variants are stored in `chameleon_cache`; the primary
/// `loaded_locations` only gets the first entry per URI.
///
/// Shared by both `load_schema` (sync) and `load_schema_async`.
fn mark_loaded_chameleon_aware(
    schema_set: &mut SchemaSet,
    resolved: &str,
    doc_id: DocumentId,
    chameleon_namespace: Option<NameId>,
) {
    let doc_is_chameleon = schema_set
        .documents
        .get(doc_id as usize)
        .is_some_and(|doc| doc.is_chameleon());
    if doc_is_chameleon {
        if let Some(ns) = chameleon_namespace {
            schema_set
                .chameleon_cache
                .insert((resolved.to_owned(), ns), doc_id);
        }
    }
    if !schema_set.loaded_locations.contains_key(resolved) {
        schema_set.mark_loaded(resolved.to_owned(), doc_id);
    }
}

/// Fixup pass: fill in `target_doc` on cycle edges whose target has since
/// been loaded. Call after all directive resolution rounds complete.
pub fn fixup_composition_edges(schema_set: &mut SchemaSet) {
    for edge in &mut schema_set.composition_edges {
        if edge.target_doc.is_none() {
            edge.target_doc = schema_set
                .loaded_locations
                .get(&edge.resolved_location)
                .copied();
        }
    }
}

/// Resolve all directives in a schema document
pub fn resolve_all_directives(
    doc_id: DocumentId,
    resolver: &mut SchemaResolver,
    schema_set: &mut SchemaSet,
) -> ResolutionResult {
    let mut result = ResolutionResult::default();

    // Get the document
    let doc = match schema_set.documents.get(doc_id as usize) {
        Some(d) => d,
        None => {
            result.errors.push(SchemaError::internal(format!(
                "Document {} not found",
                doc_id
            )));
            return result;
        }
    };

    let base_uri = doc.base_uri.clone();
    let target_namespace = doc.target_namespace;

    // Clone directives to avoid borrow issues
    let includes: Vec<_> = doc.includes.to_vec();
    let imports: Vec<_> = doc.imports.to_vec();
    let redefines: Vec<_> = doc.redefines.to_vec();
    #[cfg(feature = "xsd11")]
    let overrides: Vec<_> = doc.overrides.to_vec();

    // Process includes
    for (i, include) in includes.iter().enumerate() {
        match resolver.process_include(
            &include.schema_location,
            &base_uri,
            target_namespace,
            schema_set,
        ) {
            Ok(ref outcome) => {
                match outcome {
                    LoadOutcome::Loaded(id) | LoadOutcome::AlreadyLoaded(id) => {
                        schema_set.documents[doc_id as usize].includes[i].resolved_doc_id =
                            Some(*id);
                        if matches!(outcome, LoadOutcome::Loaded(_)) {
                            result.loaded.push(*id);
                        }
                        // src-include §4.2.3 clause 2.2: when the including schema
                        // has no `targetNamespace`, the included schema must also
                        // have absent `targetNamespace`.
                        if target_namespace.is_none() {
                            let included_declared = schema_set
                                .documents
                                .get(*id as usize)
                                .and_then(|d| d.declared_target_namespace);
                            if let Some(declared) = included_declared {
                                let location = include
                                    .source
                                    .as_ref()
                                    .and_then(|s| schema_set.source_maps.locate(s));
                                let declared_str =
                                    schema_set.name_table.resolve(declared).to_string();
                                result.errors.push(SchemaError::structural(
                                    "src-include",
                                    format!(
                                        "Included schema has targetNamespace '{}' \
                                         but the including schema has no \
                                         targetNamespace",
                                        declared_str
                                    ),
                                    location,
                                ));
                            }
                        }
                    }
                    _ => {
                        result.skipped.push(include.schema_location.clone());
                    }
                }
                record_edge(
                    schema_set,
                    doc_id,
                    outcome,
                    CompositionEdgeKind::Include,
                    include.source.as_ref(),
                    &include.schema_location,
                );
            }
            Err(e) => {
                let e = locate_at(e, schema_set, include.source.as_ref());
                record_failure(
                    schema_set,
                    resolver,
                    CompositionEdgeKind::Include,
                    Some(&include.schema_location),
                    None,
                    &base_uri,
                    include.source.as_ref(),
                    &e,
                );
                result.errors.push(e)
            }
        }
    }

    // Process imports
    for (i, import) in imports.iter().enumerate() {
        // src-import §4.2.3 clause 1.1 (XSD 1.0) / §4.2.6.2 clause 1.1 (XSD
        // 1.1): "If the namespace [attribute] is present, then its ·actual
        // value· does not match the ·actual value· of the enclosing
        // <schema>'s targetNamespace [attribute]." Both versions.
        if let Some(import_ns_str) = import.namespace.as_deref() {
            let tns_str = target_namespace.map(|n| schema_set.name_table.resolve(n));
            if Some(import_ns_str) == tns_str.as_deref() {
                let message = if schema_set.is_xsd10() {
                    format!(
                        "xs:import namespace '{}' must not equal the enclosing \
                         schema's targetNamespace in XSD 1.0",
                        import_ns_str
                    )
                } else {
                    format!(
                        "xs:import namespace '{}' must not equal the enclosing \
                         schema's targetNamespace (Import Constraints and Semantics, \
                         clause 1.1)",
                        import_ns_str
                    )
                };
                result.errors.push(SchemaError::structural(
                    "src-import",
                    message,
                    import
                        .source
                        .as_ref()
                        .and_then(|s| schema_set.source_maps.locate(s)),
                ));
                continue;
            }
        }
        // src-import §4.2.3 clause 1.2 (XSD 1.0) / §4.2.6.1 clause 1.2 (XSD
        // 1.1): if the `namespace` attribute is absent, the enclosing
        // <schema> must have a targetNamespace attribute. addB008 / addB035.
        if import.namespace.is_none() && target_namespace.is_none() {
            result.errors.push(SchemaError::structural(
                "src-import",
                "xs:import without 'namespace' requires the enclosing schema to have \
                 a 'targetNamespace' attribute",
                import
                    .source
                    .as_ref()
                    .and_then(|s| schema_set.source_maps.locate(s)),
            ));
            continue;
        }
        match resolver.process_import(
            import.namespace.as_deref(),
            import.schema_location.as_deref(),
            &base_uri,
            schema_set,
        ) {
            Ok(Some(ref outcome)) => {
                match outcome {
                    LoadOutcome::Loaded(id) => {
                        result.loaded.push(*id);
                        schema_set.documents[doc_id as usize].imports[i].resolved_doc_id =
                            Some(*id);
                    }
                    LoadOutcome::AlreadyLoaded(id) => {
                        // Already processed — record doc_id but don't add to loaded.
                        schema_set.documents[doc_id as usize].imports[i].resolved_doc_id =
                            Some(*id);
                    }
                    _ => {
                        if let Some(loc) = &import.schema_location {
                            result.skipped.push(loc.clone());
                        }
                    }
                }
                record_edge(
                    schema_set,
                    doc_id,
                    outcome,
                    CompositionEdgeKind::Import,
                    import.source.as_ref(),
                    import.schema_location.as_deref().unwrap_or_default(),
                );
            }
            Ok(None) => {
                // No schemaLocation and no catalog match — no edge to record
            }
            Err(e) => {
                let e = locate_at(e, schema_set, import.source.as_ref());
                record_failure(
                    schema_set,
                    resolver,
                    CompositionEdgeKind::Import,
                    import.schema_location.as_deref(),
                    import.namespace.as_deref(),
                    &base_uri,
                    import.source.as_ref(),
                    &e,
                );
                result.import_errors.push(e)
            }
        }
    }

    // Process redefines
    for (i, redefine) in redefines.iter().enumerate() {
        match resolver.process_redefine(
            &redefine.schema_location,
            &base_uri,
            target_namespace,
            schema_set,
        ) {
            Ok(ref outcome) => {
                match outcome {
                    LoadOutcome::Loaded(id) => {
                        result.loaded.push(*id);
                        schema_set.documents[doc_id as usize].redefines[i].resolved_doc_id =
                            Some(*id);
                    }
                    LoadOutcome::AlreadyLoaded(id) => {
                        schema_set.documents[doc_id as usize].redefines[i].resolved_doc_id =
                            Some(*id);
                    }
                    _ => {
                        result.skipped.push(redefine.schema_location.clone());
                    }
                }
                record_edge(
                    schema_set,
                    doc_id,
                    outcome,
                    CompositionEdgeKind::Redefine,
                    redefine.source.as_ref(),
                    &redefine.schema_location,
                );
            }
            Err(e) => {
                // Structures §4.2.4 Redefinition Constraints and Semantics,
                // clause 1: "If there are any element information items
                // among the [children] other than <annotation> then the
                // ·actual value· of the schemaLocation [attribute] must
                // successfully resolve."
                let has_children = !redefine.simple_types.is_empty()
                    || !redefine.complex_types.is_empty()
                    || !redefine.groups.is_empty()
                    || !redefine.attribute_groups.is_empty();
                if has_children && !e.is_schema_content_error() {
                    let location = redefine
                        .source
                        .as_ref()
                        .and_then(|s| schema_set.source_maps.locate(s));
                    result.errors.push(SchemaError::structural(
                        "src-redefine",
                        format!(
                            "the schema document '{}' of an xs:redefine with redefinitions \
                             could not be read: {e} (Redefinition Constraints and Semantics, \
                             clause 1: its schemaLocation \"must successfully resolve\")",
                            redefine.schema_location
                        ),
                        location,
                    ));
                    continue;
                }
                record_failure(
                    schema_set,
                    resolver,
                    CompositionEdgeKind::Redefine,
                    Some(&redefine.schema_location),
                    None,
                    &base_uri,
                    redefine.source.as_ref(),
                    &e,
                );
                result.errors.push(e)
            }
        }
    }

    // Process overrides (XSD 1.1)
    #[cfg(feature = "xsd11")]
    for (i, override_dir) in overrides.iter().enumerate() {
        match resolver.process_override(
            &override_dir.schema_location,
            &base_uri,
            target_namespace,
            schema_set,
        ) {
            Ok(ref outcome) => {
                match outcome {
                    LoadOutcome::Loaded(id) => {
                        result.loaded.push(*id);
                        schema_set.documents[doc_id as usize].overrides[i].resolved_doc_id =
                            Some(*id);
                    }
                    LoadOutcome::AlreadyLoaded(id) => {
                        schema_set.documents[doc_id as usize].overrides[i].resolved_doc_id =
                            Some(*id);
                    }
                    _ => {
                        result.skipped.push(override_dir.schema_location.clone());
                    }
                }
                record_edge(
                    schema_set,
                    doc_id,
                    outcome,
                    CompositionEdgeKind::Override,
                    override_dir.source.as_ref(),
                    &override_dir.schema_location,
                );
            }
            Err(e) => {
                record_failure(
                    schema_set,
                    resolver,
                    CompositionEdgeKind::Override,
                    Some(&override_dir.schema_location),
                    None,
                    &base_uri,
                    override_dir.source.as_ref(),
                    &e,
                );
                result.errors.push(e)
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_absolute_uri() {
        assert!(is_absolute_uri("http://example.com/schema.xsd"));
        assert!(is_absolute_uri("https://example.com/schema.xsd"));
        assert!(is_absolute_uri("/absolute/path/schema.xsd"));
        assert!(!is_absolute_uri("relative/path/schema.xsd"));
        assert!(!is_absolute_uri("../parent/schema.xsd"));
    }

    #[test]
    fn test_resolve_relative_path() {
        let resolved = resolve_relative_path("types.xsd", "/home/user/schema.xsd").unwrap();
        assert!(resolved.contains("types.xsd"));
    }

    #[test]
    fn test_resolve_relative_path_parent() {
        let resolved =
            resolve_relative_path("../common/types.xsd", "/home/user/schemas/main.xsd").unwrap();
        assert_eq!(
            Path::new(&resolved),
            Path::new("/home/user/common/types.xsd")
        );
    }

    #[test]
    fn test_resolve_relative_path_parent_without_base() {
        let resolved = resolve_relative_path("../xsd/X.xsd", "").unwrap();
        assert_eq!(Path::new(&resolved), Path::new("../xsd/X.xsd"));
        let resolved = resolve_relative_path("../../xsd/X.xsd", "").unwrap();
        assert_eq!(Path::new(&resolved), Path::new("../../xsd/X.xsd"));
    }

    #[test]
    fn test_resolve_relative_path_parent_of_relative_base() {
        let resolved = resolve_relative_path("../../common.xsd", "schemas/a.xsd").unwrap();
        assert_eq!(Path::new(&resolved), Path::new("../common.xsd"));
        let resolved = resolve_relative_path("../common.xsd", "../schemas/a.xsd").unwrap();
        assert_eq!(Path::new(&resolved), Path::new("../common.xsd"));
        let resolved = resolve_relative_path("../common.xsd", "schemas/a.xsd").unwrap();
        assert_eq!(Path::new(&resolved), Path::new("common.xsd"));
    }

    #[test]
    fn test_resolve_relative_path_parent_above_root() {
        let resolved = resolve_relative_path("../../../x.xsd", "/a/b.xsd").unwrap();
        assert_eq!(Path::new(&resolved), Path::new("/x.xsd"));
    }

    #[test]
    fn test_resolve_relative_url() {
        let resolved = SchemaResolver::new()
            .resolve_location("types.xsd", "http://example.com/schemas/main.xsd")
            .unwrap();
        assert_eq!(resolved, "http://example.com/schemas/types.xsd");
    }

    #[test]
    fn test_catalog_lookup() {
        let mut catalog = SchemaCatalog::new();
        catalog.add("http://example.com/ns", "/path/to/schema.xsd");

        assert_eq!(
            catalog.lookup("http://example.com/ns"),
            Some("/path/to/schema.xsd")
        );
        assert_eq!(catalog.lookup("http://other.com/ns"), None);
    }

    #[test]
    fn test_resolver_config_default() {
        let config = ResolverConfig::default();
        assert!(!config.allow_network);
        assert_eq!(config.max_depth, 100);
    }

    #[test]
    fn test_resolver_new() {
        let resolver = SchemaResolver::new();
        assert!(resolver.resolving.is_empty());
    }

    fn normalized(path: &str) -> PathBuf {
        normalize_path(Path::new(path))
    }

    #[test]
    fn test_normalize_path() {
        assert_eq!(
            normalized("/home/user/../other/./schema.xsd"),
            Path::new("/home/other/schema.xsd")
        );
        assert_eq!(normalized("a/./b/../c.xsd"), Path::new("a/c.xsd"));
        assert_eq!(normalized("./a.xsd"), Path::new("a.xsd"));
        assert_eq!(normalized("a/.."), Path::new(""));
    }

    #[test]
    fn test_normalize_path_keeps_leading_parent_dir() {
        assert_eq!(normalized("../xsd/X.xsd"), Path::new("../xsd/X.xsd"));
        assert_eq!(normalized("./../xsd/X.xsd"), Path::new("../xsd/X.xsd"));
        assert_eq!(
            normalized("../../common.xsd"),
            Path::new("../../common.xsd")
        );
        assert_eq!(normalized(".."), Path::new(".."));
    }

    #[test]
    fn test_normalize_path_parent_dir_beyond_relative_start() {
        assert_eq!(normalized("a/../.."), Path::new(".."));
        assert_eq!(normalized("a/../../b.xsd"), Path::new("../b.xsd"));
        assert_eq!(
            normalized("schemas/../../common.xsd"),
            Path::new("../common.xsd")
        );
        assert_eq!(normalized("../a/../b.xsd"), Path::new("../b.xsd"));
        assert_eq!(normalized("a/b/../../../c/d.xsd"), Path::new("../c/d.xsd"));
    }

    #[test]
    fn test_normalize_path_parent_dir_at_root() {
        assert_eq!(normalized("/.."), Path::new("/"));
        assert_eq!(normalized("/../a.xsd"), Path::new("/a.xsd"));
        assert_eq!(normalized("/a/../../b/c.xsd"), Path::new("/b/c.xsd"));
    }

    #[cfg(windows)]
    #[test]
    fn test_normalize_path_windows_prefix() {
        assert_eq!(
            normalized(r"C:\schemas\..\..\common.xsd"),
            Path::new(r"C:\common.xsd")
        );
        assert_eq!(
            normalized(r"\\server\share\..\a.xsd"),
            Path::new(r"\\server\share\a.xsd")
        );
        // Drive-relative: `..` is relative to the drive's current directory.
        assert_eq!(normalized(r"C:..\a.xsd"), Path::new(r"C:..\a.xsd"));
        assert_eq!(normalized(r"C:a\..\..\b.xsd"), Path::new(r"C:..\b.xsd"));
    }

    #[test]
    fn test_resolution_result_default() {
        let result = ResolutionResult::default();
        assert!(result.is_ok());
        assert!(!result.has_loaded());
    }

    #[test]
    fn test_catalog_xml_namespaces() {
        let mut catalog = SchemaCatalog::new();
        catalog.add_xml_catalog();

        assert_eq!(
            catalog.lookup("http://www.w3.org/XML/1998/namespace"),
            Some("embedded://xml.xsd")
        );
        assert!(catalog
            .lookup("http://www.w3.org/2001/XMLSchema-instance")
            .is_some());
    }

    #[test]
    fn test_embedded_loader() {
        let loader = EmbeddedLoader::new();

        // Can load embedded URIs
        assert!(loader.can_load("embedded://xml.xsd"));
        assert!(!loader.can_load("/path/to/file.xsd"));
        assert!(!loader.can_load("http://example.com/schema.xsd"));

        // Load xml.xsd
        let content = loader.load("embedded://xml.xsd").unwrap();
        assert!(content.contains("targetNamespace=\"http://www.w3.org/XML/1998/namespace\""));

        // Unknown embedded schema
        assert!(loader.load("embedded://unknown.xsd").is_err());
    }

    #[test]
    fn test_file_system_loader() {
        let loader = FileSystemLoader::new();

        // Can load file paths, not embedded or HTTP
        assert!(loader.can_load("/path/to/file.xsd"));
        assert!(loader.can_load("relative/path.xsd"));
        assert!(!loader.can_load("embedded://xml.xsd"));
        assert!(!loader.can_load("http://example.com/schema.xsd"));
        assert!(!loader.can_load("https://example.com/schema.xsd"));
    }

    #[test]
    fn test_loader_chain() {
        let chain = LoaderChain::with_defaults();

        // Can load both embedded and file paths
        assert!(chain.can_load("embedded://xml.xsd"));
        assert!(chain.can_load("/path/to/file.xsd"));

        // Load embedded schema through chain
        let content = chain.load("embedded://xml.xsd").unwrap();
        assert!(content.contains("http://www.w3.org/XML/1998/namespace"));

        // Chain has expected number of loaders
        assert_eq!(chain.len(), 2);
    }

    #[test]
    fn test_loader_chain_priority() {
        let mut chain = LoaderChain::new();
        chain.add(Box::new(FileSystemLoader::new())); // priority 0
        chain.add(Box::new(EmbeddedLoader::new())); // priority 100

        // EmbeddedLoader should be first due to higher priority
        assert_eq!(chain.priority(), 100);
    }

    #[test]
    fn test_resolver_with_embedded_loader() {
        let resolver = SchemaResolver::new();

        // Load embedded xml.xsd
        let content = resolver.load_content("embedded://xml.xsd").unwrap();
        assert!(content.contains("http://www.w3.org/XML/1998/namespace"));
    }

    #[test]
    fn test_composition_edges_recorded() {
        use crate::parser::parse::parse_schema;
        use crate::schema::composition::CompositionEdgeKind;
        use crate::schema::SchemaSet;

        let tmp = std::env::temp_dir().join("xsd_test_composition_edges");
        std::fs::create_dir_all(&tmp).unwrap();

        // Base schema with a simple type
        let base_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
    <xs:simpleType name="MyString">
        <xs:restriction base="xs:string"/>
    </xs:simpleType>
</xs:schema>"#;
        let base_path = tmp.join("comp_base.xsd");
        std::fs::write(&base_path, base_xsd).unwrap();

        // Main schema with include + redefine
        let main_xsd = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
    <xs:include schemaLocation="{loc}"/>
    <xs:redefine schemaLocation="{loc}">
        <xs:simpleType name="MyString">
            <xs:restriction base="MyString">
                <xs:maxLength value="50"/>
            </xs:restriction>
        </xs:simpleType>
    </xs:redefine>
</xs:schema>"#,
            loc = base_path.to_string_lossy()
        );

        let mut schema_set = SchemaSet::new();
        let main_path = tmp.join("comp_main.xsd").to_string_lossy().to_string();
        let doc_id = parse_schema(main_xsd.as_bytes(), &main_path, &mut schema_set).unwrap();

        let mut resolver = SchemaResolver::new();
        let result = resolve_all_directives(doc_id, &mut resolver, &mut schema_set);
        assert!(result.is_ok(), "Resolution should succeed");

        // Should have edges for include and redefine
        let edges = &schema_set.composition_edges;
        assert!(
            edges.len() >= 2,
            "Expected at least 2 edges, got {}",
            edges.len()
        );

        let include_edges: Vec<_> = edges
            .iter()
            .filter(|e| e.kind == CompositionEdgeKind::Include)
            .collect();
        assert!(!include_edges.is_empty(), "Should have an include edge");
        assert_eq!(include_edges[0].source_doc, doc_id);

        let redefine_edges: Vec<_> = edges
            .iter()
            .filter(|e| e.kind == CompositionEdgeKind::Redefine)
            .collect();
        assert!(!redefine_edges.is_empty(), "Should have a redefine edge");
        assert_eq!(redefine_edges[0].source_doc, doc_id);

        // Both edges should point to the same target document
        assert!(include_edges[0].target_doc.is_some());
        assert_eq!(include_edges[0].target_doc, redefine_edges[0].target_doc);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_composition_edges_cycle() {
        use crate::parser::parse::parse_schema;
        use crate::schema::composition::CompositionEdgeKind;
        use crate::schema::SchemaSet;

        let tmp = std::env::temp_dir().join("xsd_test_composition_cycle");
        std::fs::create_dir_all(&tmp).unwrap();

        let a_path = tmp.join("cycle_a.xsd");
        let b_path = tmp.join("cycle_b.xsd");

        // a.xsd includes b.xsd
        let a_xsd = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
    <xs:include schemaLocation="{}"/>
    <xs:element name="A" type="xs:string"/>
</xs:schema>"#,
            b_path.to_string_lossy()
        );

        // b.xsd includes a.xsd (creates cycle)
        let b_xsd = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
    <xs:include schemaLocation="{}"/>
    <xs:element name="B" type="xs:string"/>
</xs:schema>"#,
            a_path.to_string_lossy()
        );

        std::fs::write(&a_path, &a_xsd).unwrap();
        std::fs::write(&b_path, &b_xsd).unwrap();

        let mut schema_set = SchemaSet::new();
        let a_uri = a_path.to_string_lossy().to_string();
        let a_doc_id = parse_schema(
            std::fs::read_to_string(&a_path).unwrap().as_bytes(),
            &a_uri,
            &mut schema_set,
        )
        .unwrap();

        // Mark a.xsd as loaded so cycle detection works
        schema_set.mark_loaded(a_uri, a_doc_id);

        let mut resolver = SchemaResolver::new();

        // First resolution: a.xsd's directives (loads b.xsd)
        let result_a = resolve_all_directives(a_doc_id, &mut resolver, &mut schema_set);
        assert!(result_a.is_ok(), "Resolution of a.xsd should succeed");
        assert_eq!(result_a.loaded.len(), 1, "Should have loaded b.xsd");

        let b_doc_id = result_a.loaded[0];

        // Second resolution: b.xsd's directives (a.xsd already loaded)
        let result_b = resolve_all_directives(b_doc_id, &mut resolver, &mut schema_set);
        assert!(result_b.is_ok(), "Resolution of b.xsd should succeed");

        // Should have edges for both directions
        let edges = &schema_set.composition_edges;

        // a→b edge (from first resolution, Loaded branch)
        let a_to_b: Vec<_> = edges
            .iter()
            .filter(|e| e.source_doc == a_doc_id && e.target_doc == Some(b_doc_id))
            .collect();
        assert_eq!(a_to_b.len(), 1, "Should have exactly one a→b edge");
        assert_eq!(a_to_b[0].kind, CompositionEdgeKind::Include);

        // b→a edge (from second resolution, AlreadyLoaded branch)
        let b_to_a: Vec<_> = edges
            .iter()
            .filter(|e| e.source_doc == b_doc_id && e.target_doc == Some(a_doc_id))
            .collect();
        assert_eq!(b_to_a.len(), 1, "Should have exactly one b→a edge");
        assert_eq!(b_to_a[0].kind, CompositionEdgeKind::Include);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_resolved_doc_id_populated() {
        use crate::parser::parse::parse_schema;
        use crate::schema::SchemaSet;

        let tmp = std::env::temp_dir().join("xsd_test_resolved_doc_id");
        std::fs::create_dir_all(&tmp).unwrap();

        // Base schema with a simple type
        let base_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
    <xs:simpleType name="MyString">
        <xs:restriction base="xs:string"/>
    </xs:simpleType>
</xs:schema>"#;
        let base_path = tmp.join("base.xsd");
        std::fs::write(&base_path, base_xsd).unwrap();

        // Main schema that includes and redefines the base
        let main_xsd = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
    <xs:include schemaLocation="{loc}"/>
    <xs:redefine schemaLocation="{loc}">
        <xs:simpleType name="MyString">
            <xs:restriction base="MyString">
                <xs:maxLength value="50"/>
            </xs:restriction>
        </xs:simpleType>
    </xs:redefine>
</xs:schema>"#,
            loc = base_path.to_string_lossy()
        );

        let mut schema_set = SchemaSet::new();
        let main_path = tmp.join("main.xsd").to_string_lossy().to_string();
        let doc_id = parse_schema(main_xsd.as_bytes(), &main_path, &mut schema_set).unwrap();

        let mut resolver = SchemaResolver::new();
        let result = resolve_all_directives(doc_id, &mut resolver, &mut schema_set);
        assert!(result.is_ok(), "Resolution should succeed");

        let doc = &schema_set.documents[doc_id as usize];
        assert!(
            doc.includes[0].resolved_doc_id.is_some(),
            "Include should have resolved_doc_id"
        );
        assert!(
            doc.redefines[0].resolved_doc_id.is_some(),
            "Redefine should have resolved_doc_id"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_document_component_index_populated() {
        use crate::parser::parse::parse_schema;
        use crate::schema::SchemaSet;

        let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
    <xs:simpleType name="MyString">
        <xs:restriction base="xs:string"/>
    </xs:simpleType>
    <xs:element name="root" type="MyString"/>
</xs:schema>"#;

        let mut schema_set = SchemaSet::new();
        let doc_id = parse_schema(xsd.as_bytes(), "test.xsd", &mut schema_set).unwrap();

        let doc = &schema_set.documents[doc_id as usize];
        assert!(
            !doc.component_index.is_empty(),
            "Component index should be populated"
        );

        // Should find the simple type
        assert!(
            doc.component_index
                .lookup_type(None, schema_set.name_table.get("MyString").unwrap())
                .is_some(),
            "Should find MyString type in document component index"
        );

        // Should find the element
        assert!(
            doc.component_index
                .lookup_element(None, schema_set.name_table.get("root").unwrap())
                .is_some(),
            "Should find root element in document component index"
        );

        // Should NOT find a non-existent component
        assert!(
            doc.component_index
                .lookup_type(None, schema_set.name_table.get("root").unwrap())
                .is_none(),
            "Should not find 'root' as a type"
        );
    }

    #[test]
    fn test_redefine_uses_document_scoped_lookup() {
        use crate::parser::parse::parse_schema;
        use crate::schema::SchemaSet;

        let tmp = std::env::temp_dir().join("xsd_test_redefine_doc_scoped");
        std::fs::create_dir_all(&tmp).unwrap();

        // Base schema with a simple type
        let base_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
    <xs:simpleType name="MyString">
        <xs:restriction base="xs:string"/>
    </xs:simpleType>
</xs:schema>"#;
        let base_path = tmp.join("redef_base.xsd");
        std::fs::write(&base_path, base_xsd).unwrap();

        // Main schema that redefines the base type
        let main_xsd = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
    <xs:redefine schemaLocation="{loc}">
        <xs:simpleType name="MyString">
            <xs:restriction base="MyString">
                <xs:maxLength value="50"/>
            </xs:restriction>
        </xs:simpleType>
    </xs:redefine>
</xs:schema>"#,
            loc = base_path.to_string_lossy()
        );

        let mut schema_set = SchemaSet::new();
        let main_path = tmp.join("redef_main.xsd").to_string_lossy().to_string();
        let doc_id = parse_schema(main_xsd.as_bytes(), &main_path, &mut schema_set).unwrap();

        // Resolve directives (loads base.xsd, populates resolved_doc_id)
        let mut resolver = SchemaResolver::new();
        let result = resolve_all_directives(doc_id, &mut resolver, &mut schema_set);
        assert!(result.is_ok(), "Resolution should succeed");

        let main_doc = &schema_set.documents[doc_id as usize];
        let target_doc_id = main_doc.redefines[0].resolved_doc_id;
        assert!(
            target_doc_id.is_some(),
            "Redefine should have resolved_doc_id"
        );

        // Verify the target document's component index has MyString
        let target_doc = &schema_set.documents[target_doc_id.unwrap() as usize];
        let my_string_name = schema_set.name_table.get("MyString").unwrap();
        assert!(
            target_doc
                .component_index
                .lookup_type(None, my_string_name)
                .is_some(),
            "Target document should have MyString in component index"
        );

        // Apply redefine — should succeed using document-scoped lookup
        crate::schema::apply_redefine_override(&mut schema_set).unwrap();

        // Verify the namespace table now has the redefined type
        let type_key = schema_set.lookup_type(None, my_string_name);
        assert!(
            type_key.is_some(),
            "MyString should still be in namespace table after redefine"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_effective_components_provenance_populated() {
        use crate::parser::parse::parse_schema;
        use crate::schema::composition::CompositionAction;
        use crate::schema::SchemaSet;

        let tmp = std::env::temp_dir().join("xsd_test_provenance");
        std::fs::create_dir_all(&tmp).unwrap();

        let base_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
    <xs:simpleType name="MyStr">
        <xs:restriction base="xs:string"/>
    </xs:simpleType>
    <xs:element name="root" type="MyStr"/>
</xs:schema>"#;
        let base_path = tmp.join("prov_base.xsd");
        std::fs::write(&base_path, base_xsd).unwrap();

        let main_xsd = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
    <xs:redefine schemaLocation="{loc}">
        <xs:simpleType name="MyStr">
            <xs:restriction base="MyStr">
                <xs:maxLength value="50"/>
            </xs:restriction>
        </xs:simpleType>
    </xs:redefine>
</xs:schema>"#,
            loc = base_path.to_string_lossy()
        );

        let mut schema_set = SchemaSet::new();
        let main_path = tmp.join("prov_main.xsd").to_string_lossy().to_string();
        let doc_id = parse_schema(main_xsd.as_bytes(), &main_path, &mut schema_set).unwrap();

        let mut resolver = SchemaResolver::new();
        let result = resolve_all_directives(doc_id, &mut resolver, &mut schema_set);
        assert!(result.is_ok());

        // Apply composition — builds effective components
        crate::schema::apply_redefine_override(&mut schema_set).unwrap();

        assert!(
            !schema_set.effective_components.is_empty(),
            "Effective components should be populated after composition"
        );

        // The redefined component (MyStr) should have Redefined action,
        // NOT a separate Declared entry — redefine replaces the declared entry.
        let my_str_name = schema_set.name_table.get("MyStr").unwrap();
        let my_str_identity = crate::schema::composition::ComponentIdentity {
            kind: crate::schema::composition::ComponentKind::SimpleType,
            name: my_str_name,
            namespace: None,
        };
        let my_str_eff = schema_set.effective_components.get(&my_str_identity);
        assert!(
            my_str_eff.is_some(),
            "MyStr should be in effective components"
        );
        let my_str_eff = my_str_eff.unwrap();
        assert!(
            matches!(my_str_eff.action, CompositionAction::Redefined { .. }),
            "MyStr should have Redefined action, not Declared"
        );
        // origin should point at the redefining document (main), not the target
        assert_eq!(
            my_str_eff.origin.owner_doc,
            Some(doc_id),
            "Redefined component origin should be the redefining document"
        );

        // The other component (root element) from base.xsd should still be Declared
        let declared_count = schema_set
            .effective_components
            .values()
            .filter(|c| matches!(c.action, CompositionAction::Declared))
            .count();
        assert!(
            declared_count > 0,
            "Should have declared components for non-redefined items"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// When resolved_doc_id is Some but the target document does NOT declare
    /// the component, redefine must fail — it must not fall back to a
    /// same-name component from another document in the global namespace table.
    #[test]
    fn test_redefine_no_fallback_to_global_when_scoped() {
        use crate::parser::parse::parse_schema;
        use crate::schema::model::RedefineDirective;
        use crate::schema::redefine::apply_redefine;
        use crate::schema::SchemaSet;

        let tmp = std::env::temp_dir().join("xsd_test_redefine_no_fallback");
        std::fs::create_dir_all(&tmp).unwrap();

        // doc_a.xsd declares MyType (simple type)
        let doc_a_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
    <xs:simpleType name="MyType">
        <xs:restriction base="xs:string"/>
    </xs:simpleType>
</xs:schema>"#;
        let doc_a_path = tmp.join("no_fallback_a.xsd");
        std::fs::write(&doc_a_path, doc_a_xsd).unwrap();

        // doc_b.xsd declares a DIFFERENT type (not MyType)
        let doc_b_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
    <xs:simpleType name="OtherType">
        <xs:restriction base="xs:string"/>
    </xs:simpleType>
</xs:schema>"#;
        let doc_b_path = tmp.join("no_fallback_b.xsd");
        std::fs::write(&doc_b_path, doc_b_xsd).unwrap();

        // Parse both documents
        let mut schema_set = SchemaSet::new();
        let _doc_a_id = parse_schema(
            std::fs::read_to_string(&doc_a_path).unwrap().as_bytes(),
            &doc_a_path.to_string_lossy(),
            &mut schema_set,
        )
        .unwrap();
        let doc_b_id = parse_schema(
            std::fs::read_to_string(&doc_b_path).unwrap().as_bytes(),
            &doc_b_path.to_string_lossy(),
            &mut schema_set,
        )
        .unwrap();

        // MyType IS in global namespace table (from doc_a)
        let my_type_name = schema_set.name_table.get("MyType").unwrap();
        assert!(
            schema_set.lookup_type(None, my_type_name).is_some(),
            "MyType should be in global namespace table from doc_a"
        );

        // Create a fake redefine that points resolved_doc_id at doc_b
        // (which does NOT declare MyType). The redefine's replacement type
        // needs to exist in the arena with the right name.
        let redef_key = schema_set
            .arenas
            .alloc_simple_type(crate::arenas::SimpleTypeDefData {
                name: Some(my_type_name),
                target_namespace: None,
                variety: crate::parser::frames::SimpleTypeVariety::Atomic,
                base_type: Some(crate::parser::frames::TypeRefResult::QName(
                    crate::parser::frames::QNameRef {
                        namespace: None,
                        local_name: my_type_name,
                        prefix: None,
                    },
                )),
                item_type: None,
                member_types: Vec::new(),
                facets: Default::default(),
                final_derivation: crate::schema::model::DerivationSet::empty(),
                id: None,
                derivation_id: None,
                annotation: None,
                source: None,
                resolved_base_type: None,
                resolved_item_type: None,
                resolved_member_types: Vec::new(),
                redefine_original: None,
                deferred_item_type_error: None,
            });

        let redefine = RedefineDirective {
            source: None,
            schema_location: doc_b_path.to_string_lossy().to_string(),
            resolved_doc_id: Some(doc_b_id), // points at doc_b, which has no MyType
            simple_types: vec![redef_key],
            complex_types: Vec::new(),
            groups: Vec::new(),
            attribute_groups: Vec::new(),
        };

        // This MUST fail: doc_b does not declare MyType, and the lookup
        // must not fall back to the global table where doc_a's MyType lives.
        let result = apply_redefine(&mut schema_set, &redefine);
        assert!(
            result.is_err(),
            "Redefine should fail when target document lacks the component (no global fallback)"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// When the target document has a complex type named "Foo" but the
    /// redefine is for a simple type named "Foo", it must not match —
    /// kind-sensitive lookup must reject the cross-kind match.
    #[test]
    fn test_redefine_simple_vs_complex_kind_mismatch() {
        use crate::parser::parse::parse_schema;
        use crate::schema::model::RedefineDirective;
        use crate::schema::redefine::apply_redefine;
        use crate::schema::SchemaSet;

        let tmp = std::env::temp_dir().join("xsd_test_redefine_kind_mismatch");
        std::fs::create_dir_all(&tmp).unwrap();

        // target.xsd declares Foo as a COMPLEX type
        let target_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
    <xs:complexType name="Foo">
        <xs:sequence>
            <xs:element name="bar" type="xs:string"/>
        </xs:sequence>
    </xs:complexType>
</xs:schema>"#;
        let target_path = tmp.join("kind_target.xsd");
        std::fs::write(&target_path, target_xsd).unwrap();

        let mut schema_set = SchemaSet::new();
        let target_id = parse_schema(
            std::fs::read_to_string(&target_path).unwrap().as_bytes(),
            &target_path.to_string_lossy(),
            &mut schema_set,
        )
        .unwrap();

        let foo_name = schema_set.name_table.get("Foo").unwrap();

        // Verify target doc has Foo as complex type, NOT simple type
        let target_doc = &schema_set.documents[target_id as usize];
        assert!(
            target_doc
                .component_index
                .lookup_complex_type(None, foo_name)
                .is_some(),
            "Target should have Foo as complex type"
        );
        assert!(
            target_doc
                .component_index
                .lookup_simple_type(None, foo_name)
                .is_none(),
            "Target should NOT have Foo as simple type"
        );

        // Create a simple type redefine for "Foo" pointing at target doc
        let redef_key = schema_set
            .arenas
            .alloc_simple_type(crate::arenas::SimpleTypeDefData {
                name: Some(foo_name),
                target_namespace: None,
                variety: crate::parser::frames::SimpleTypeVariety::Atomic,
                base_type: Some(crate::parser::frames::TypeRefResult::QName(
                    crate::parser::frames::QNameRef {
                        namespace: None,
                        local_name: foo_name,
                        prefix: None,
                    },
                )),
                item_type: None,
                member_types: Vec::new(),
                facets: Default::default(),
                final_derivation: crate::schema::model::DerivationSet::empty(),
                id: None,
                derivation_id: None,
                annotation: None,
                source: None,
                resolved_base_type: None,
                resolved_item_type: None,
                resolved_member_types: Vec::new(),
                redefine_original: None,
                deferred_item_type_error: None,
            });

        let redefine = RedefineDirective {
            source: None,
            schema_location: target_path.to_string_lossy().to_string(),
            resolved_doc_id: Some(target_id),
            simple_types: vec![redef_key],
            complex_types: Vec::new(),
            groups: Vec::new(),
            attribute_groups: Vec::new(),
        };

        // Must fail: target has complex type "Foo", not simple type "Foo"
        let result = apply_redefine(&mut schema_set, &redefine);
        assert!(
            result.is_err(),
            "Simple type redefine must not match a same-name complex type in target document"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Chameleon include: a no-namespace schema included by a namespace-bearing
    /// schema should adopt the includer's targetNamespace (§4.2.3 clause 2.3).
    #[test]
    fn test_chameleon_include_adopts_namespace() {
        use crate::parser::parse::parse_schema;
        use crate::schema::SchemaSet;

        let tmp = std::env::temp_dir().join("xsd_test_chameleon_include");
        std::fs::create_dir_all(&tmp).unwrap();

        // chameleon.xsd: no targetNamespace — declares MyType
        let chameleon_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
    <xs:simpleType name="MyType">
        <xs:restriction base="xs:string"/>
    </xs:simpleType>
</xs:schema>"#;
        let chameleon_path = tmp.join("chameleon.xsd");
        std::fs::write(&chameleon_path, chameleon_xsd).unwrap();

        // main.xsd: has targetNamespace, includes chameleon.xsd
        let main_xsd = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           targetNamespace="http://example.com/main">
    <xs:include schemaLocation="{}"/>
    <xs:element name="root" type="tns:MyType" xmlns:tns="http://example.com/main"/>
</xs:schema>"#,
            chameleon_path.to_string_lossy()
        );

        let mut schema_set = SchemaSet::new();
        let main_path = tmp.join("main.xsd").to_string_lossy().to_string();
        let doc_id = parse_schema(main_xsd.as_bytes(), &main_path, &mut schema_set).unwrap();

        // Resolve directives — this triggers chameleon namespace adoption
        let mut resolver = SchemaResolver::new();
        let result = resolve_all_directives(doc_id, &mut resolver, &mut schema_set);
        assert!(result.is_ok(), "Resolution should succeed");
        assert!(
            !result.loaded.is_empty(),
            "Should have loaded chameleon.xsd"
        );

        // The chameleon document should have adopted the includer's namespace
        let chameleon_doc_id = result.loaded[0];
        let chameleon_doc = &schema_set.documents[chameleon_doc_id as usize];
        let main_ns = schema_set
            .name_table
            .get("http://example.com/main")
            .unwrap();
        assert_eq!(
            chameleon_doc.target_namespace,
            Some(main_ns),
            "Chameleon document should adopt includer's targetNamespace"
        );

        // MyType should be registered in the main namespace, not no-namespace
        let my_type_name = schema_set.name_table.get("MyType").unwrap();
        assert!(
            schema_set
                .lookup_type(Some(main_ns), my_type_name)
                .is_some(),
            "MyType should be in the includer's namespace after chameleon adoption"
        );
        assert!(
            schema_set.lookup_type(None, my_type_name).is_none(),
            "MyType should NOT be in no-namespace after chameleon adoption"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Chameleon redefine: a no-namespace schema redefined by a namespace-bearing
    /// schema should adopt the redefiner's targetNamespace (§4.2.4).
    #[test]
    fn test_chameleon_redefine_adopts_namespace() {
        use crate::parser::parse::parse_schema;
        use crate::schema::SchemaSet;

        let tmp = std::env::temp_dir().join("xsd_test_chameleon_redefine");
        std::fs::create_dir_all(&tmp).unwrap();

        // chameleon.xsd: no targetNamespace
        let chameleon_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
    <xs:simpleType name="MyStr">
        <xs:restriction base="xs:string"/>
    </xs:simpleType>
</xs:schema>"#;
        let chameleon_path = tmp.join("cham_redef.xsd");
        std::fs::write(&chameleon_path, chameleon_xsd).unwrap();

        // main.xsd: has targetNamespace, redefines from chameleon.xsd
        let main_xsd = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           targetNamespace="http://example.com/ns">
    <xs:redefine schemaLocation="{}">
        <xs:simpleType name="MyStr">
            <xs:restriction base="MyStr">
                <xs:maxLength value="50"/>
            </xs:restriction>
        </xs:simpleType>
    </xs:redefine>
</xs:schema>"#,
            chameleon_path.to_string_lossy()
        );

        let mut schema_set = SchemaSet::new();
        let main_path = tmp.join("cham_main.xsd").to_string_lossy().to_string();
        let doc_id = parse_schema(main_xsd.as_bytes(), &main_path, &mut schema_set).unwrap();

        let mut resolver = SchemaResolver::new();
        let result = resolve_all_directives(doc_id, &mut resolver, &mut schema_set);
        assert!(result.is_ok(), "Resolution should succeed");

        // The chameleon document should have adopted the namespace
        let chameleon_doc_id = result.loaded[0];
        let chameleon_doc = &schema_set.documents[chameleon_doc_id as usize];
        let ns = schema_set.name_table.get("http://example.com/ns").unwrap();
        assert_eq!(
            chameleon_doc.target_namespace,
            Some(ns),
            "Chameleon redefine target should adopt redefiner's namespace"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// A no-namespace (chameleon) schema included from two schemas with
    /// different target namespaces must produce two separate document views,
    /// each adopting the includer's namespace.  Previously the second
    /// include returned the first document's ID unchanged (§4.2.3 violation).
    #[test]
    fn test_chameleon_multi_namespace_creates_separate_views() {
        use crate::parser::parse::parse_schema;
        use crate::schema::SchemaSet;

        let tmp = std::env::temp_dir().join("xsd_test_chameleon_multi_ns");
        std::fs::create_dir_all(&tmp).unwrap();

        // chameleon.xsd: no targetNamespace
        let chameleon_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
    <xs:simpleType name="SharedType">
        <xs:restriction base="xs:string"/>
    </xs:simpleType>
</xs:schema>"#;
        let chameleon_path = tmp.join("multi_ns_chameleon.xsd");
        std::fs::write(&chameleon_path, chameleon_xsd).unwrap();

        // ns_a.xsd: targetNamespace="urn:a", includes chameleon
        let ns_a_xsd = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           targetNamespace="urn:a">
    <xs:include schemaLocation="{}"/>
</xs:schema>"#,
            chameleon_path.to_string_lossy()
        );
        let ns_a_path = tmp.join("multi_ns_a.xsd");
        std::fs::write(&ns_a_path, &ns_a_xsd).unwrap();

        // ns_b.xsd: targetNamespace="urn:b", includes same chameleon
        let ns_b_xsd = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           targetNamespace="urn:b">
    <xs:include schemaLocation="{}"/>
</xs:schema>"#,
            chameleon_path.to_string_lossy()
        );
        let ns_b_path = tmp.join("multi_ns_b.xsd");
        std::fs::write(&ns_b_path, &ns_b_xsd).unwrap();

        let mut schema_set = SchemaSet::new();

        // Parse ns_a and resolve its include (loads chameleon as urn:a)
        let ns_a_uri = ns_a_path.to_string_lossy().to_string();
        let doc_a = parse_schema(
            std::fs::read_to_string(&ns_a_path).unwrap().as_bytes(),
            &ns_a_uri,
            &mut schema_set,
        )
        .unwrap();
        let mut resolver = SchemaResolver::new();
        let res_a = resolve_all_directives(doc_a, &mut resolver, &mut schema_set);
        assert!(res_a.is_ok(), "ns_a resolution should succeed");
        let chameleon_a_id = res_a.loaded[0];

        // Parse ns_b and resolve its include (must re-parse chameleon as urn:b)
        let ns_b_uri = ns_b_path.to_string_lossy().to_string();
        let doc_b = parse_schema(
            std::fs::read_to_string(&ns_b_path).unwrap().as_bytes(),
            &ns_b_uri,
            &mut schema_set,
        )
        .unwrap();
        let res_b = resolve_all_directives(doc_b, &mut resolver, &mut schema_set);
        assert!(res_b.is_ok(), "ns_b resolution should succeed");
        let chameleon_b_id = res_b.loaded[0];

        // The two chameleon loads must produce DIFFERENT document IDs
        assert_ne!(
            chameleon_a_id, chameleon_b_id,
            "Chameleon schema included from different namespaces must produce separate documents"
        );

        // Each must adopt its includer's namespace
        let ns_a_name = schema_set.name_table.get("urn:a").unwrap();
        let ns_b_name = schema_set.name_table.get("urn:b").unwrap();
        assert_eq!(
            schema_set.documents[chameleon_a_id as usize].target_namespace,
            Some(ns_a_name),
            "First chameleon copy should have urn:a namespace"
        );
        assert_eq!(
            schema_set.documents[chameleon_b_id as usize].target_namespace,
            Some(ns_b_name),
            "Second chameleon copy should have urn:b namespace"
        );

        // Both should be flagged as chameleon
        assert!(schema_set.documents[chameleon_a_id as usize].is_chameleon());
        assert!(schema_set.documents[chameleon_b_id as usize].is_chameleon());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// A no-namespace schema first loaded without chameleon adoption (from a
    /// no-namespace context) must NOT be reused when a later include requests
    /// chameleon adoption into a namespace.  schema(chameleon(tns,D2)) ≠
    /// schema(D2) per §4.2.3.
    #[test]
    fn test_raw_no_namespace_not_reused_for_chameleon() {
        use crate::parser::parse::parse_schema;
        use crate::schema::SchemaSet;

        let tmp = std::env::temp_dir().join("xsd_test_raw_no_ns_chameleon");
        std::fs::create_dir_all(&tmp).unwrap();

        // shared.xsd: no targetNamespace
        let shared_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
    <xs:element name="Shared" type="xs:string"/>
</xs:schema>"#;
        let shared_path = tmp.join("raw_shared.xsd");
        std::fs::write(&shared_path, shared_xsd).unwrap();

        // no_ns.xsd: no targetNamespace, includes shared (raw, no adoption)
        let no_ns_xsd = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
    <xs:include schemaLocation="{}"/>
</xs:schema>"#,
            shared_path.to_string_lossy()
        );
        let no_ns_path = tmp.join("raw_no_ns.xsd");
        std::fs::write(&no_ns_path, &no_ns_xsd).unwrap();

        // with_ns.xsd: targetNamespace="urn:test", includes same shared
        let with_ns_xsd = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           targetNamespace="urn:test">
    <xs:include schemaLocation="{}"/>
</xs:schema>"#,
            shared_path.to_string_lossy()
        );
        let with_ns_path = tmp.join("raw_with_ns.xsd");
        std::fs::write(&with_ns_path, &with_ns_xsd).unwrap();

        let mut schema_set = SchemaSet::new();
        let mut resolver = SchemaResolver::new();

        // First: load from no-namespace context (raw, no chameleon adoption)
        let no_ns_uri = no_ns_path.to_string_lossy().to_string();
        let doc_no_ns = parse_schema(
            std::fs::read_to_string(&no_ns_path).unwrap().as_bytes(),
            &no_ns_uri,
            &mut schema_set,
        )
        .unwrap();
        let res1 = resolve_all_directives(doc_no_ns, &mut resolver, &mut schema_set);
        assert!(res1.is_ok());
        let raw_id = res1.loaded[0];

        // The raw load should NOT be chameleon
        assert!(!schema_set.documents[raw_id as usize].is_chameleon());
        assert!(schema_set.documents[raw_id as usize]
            .target_namespace
            .is_none());

        // Second: load from namespace-bearing context (chameleon adoption)
        let with_ns_uri = with_ns_path.to_string_lossy().to_string();
        let doc_with_ns = parse_schema(
            std::fs::read_to_string(&with_ns_path).unwrap().as_bytes(),
            &with_ns_uri,
            &mut schema_set,
        )
        .unwrap();
        let res2 = resolve_all_directives(doc_with_ns, &mut resolver, &mut schema_set);
        assert!(res2.is_ok());
        let chameleon_id = res2.loaded[0];

        // Must be a DIFFERENT document — the raw no-namespace copy must not
        // leak into the namespace-bearing context.
        assert_ne!(
            raw_id, chameleon_id,
            "Raw no-namespace document must not be reused for chameleon adoption"
        );
        let ns_name = schema_set.name_table.get("urn:test").unwrap();
        assert_eq!(
            schema_set.documents[chameleon_id as usize].target_namespace,
            Some(ns_name),
            "Chameleon copy should adopt urn:test namespace"
        );
        assert!(schema_set.documents[chameleon_id as usize].is_chameleon());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
