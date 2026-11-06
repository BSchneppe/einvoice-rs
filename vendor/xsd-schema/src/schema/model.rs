//! Schema model - SchemaSet, SchemaDocument, NamespaceTable
//!
//! This module contains the core schema organization structures:
//! - `SchemaSet` - Complete schema collection with all documents and components
//! - `SchemaDocument` - Individual schema document (root or included/imported)
//! - `NamespaceTable` - Per-namespace component lookup

use bitflags::bitflags;
use std::collections::HashMap;
use std::sync::Arc;

use crate::arenas::ArenasGuard;
use crate::ids::*;
use crate::namespace::table::well_known;
use crate::namespace::NameTable;
use crate::namespace::QualifiedName;
use crate::parser::location::{SourceLocation, SourceMapStorage, SourceRef};
use crate::schema::annotation::Annotation;
use crate::schema::composition::{
    ComponentIdentity, ComponentKind, CompositionEdge, DocumentComponentIndex, EffectiveComponent,
};
use crate::schema::wildcard::ElementWildcard;
use crate::types::{BuiltinTypes, XmlTypeCode};

/// XSD version mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum XsdVersion {
    #[default]
    V1_0,
    V1_1,
}

/// Regex compatibility mode.
///
/// Controls how strictly the pattern facet grammar is enforced. The default
/// `Strict` rejects any construct outside XSD Part 2 §F (1.0) / §G (1.1)
/// regex grammar. `LenientMs` enables a closed list of safely-stripable MS
/// dialect leniencies for schemas authored against .NET's regex engine —
/// see `doc/INTRODUCTION.md` for the exact construct list.
///
/// This is an enum (not a bool) so future modes can be added without
/// breaking the API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RegexCompat {
    /// Strict XSD Part 2 regex grammar. Default.
    #[default]
    Strict,
    /// Tolerate a closed list of MS dialect leniencies (anchors at
    /// pattern start/end, `(?#...)` comments). See `doc/INTRODUCTION.md`.
    LenientMs,
}

bitflags! {
    /// Derivation control flags (for final, block attributes)
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct DerivationSet: u8 {
        const EXTENSION = 0x01;
        const RESTRICTION = 0x02;
        const LIST = 0x04;
        const UNION = 0x08;
        const SUBSTITUTION = 0x10;

        /// All derivation methods blocked
        const ALL = Self::EXTENSION.bits() | Self::RESTRICTION.bits() |
                   Self::LIST.bits() | Self::UNION.bits() | Self::SUBSTITUTION.bits();

        /// Element-relevant block bits (LIST and UNION are not meaningful for elements)
        const ELEMENT_BLOCK = Self::EXTENSION.bits() | Self::RESTRICTION.bits() | Self::SUBSTITUTION.bits();
    }
}

impl DerivationSet {
    /// Create a DerivationSet with only EXTENSION
    pub fn extension() -> Self {
        Self::EXTENSION
    }

    /// Create a DerivationSet with only RESTRICTION
    pub fn restriction() -> Self {
        Self::RESTRICTION
    }

    /// Check if extension is blocked/final
    pub fn contains_extension(&self) -> bool {
        self.contains(Self::EXTENSION)
    }

    /// Check if restriction is blocked/final
    pub fn contains_restriction(&self) -> bool {
        self.contains(Self::RESTRICTION)
    }

    /// Check if list derivation is blocked/final
    pub fn contains_list(&self) -> bool {
        self.contains(Self::LIST)
    }

    /// Check if union derivation is blocked/final
    pub fn contains_union(&self) -> bool {
        self.contains(Self::UNION)
    }

    /// Check if substitution is blocked
    pub fn contains_substitution(&self) -> bool {
        self.contains(Self::SUBSTITUTION)
    }

    /// Mask to only element-relevant block bits (extension, restriction, substitution).
    /// LIST and UNION are simple-type derivation methods and have no meaning in element
    /// block attributes. Per the spec, element `block="#all"` means `{extension, restriction,
    /// substitution}` only.
    pub fn element_block_mask(self) -> Self {
        self & Self::ELEMENT_BLOCK
    }
}

/// Form choice for element/attribute form defaults
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FormChoice {
    #[default]
    Unqualified,
    Qualified,
}

/// Complete schema set (possibly from multiple documents)
///
/// This is the main entry point for working with XSD schemas.
/// It owns all schema components and provides namespace-based lookup.
#[derive(Debug)]
pub struct SchemaSet {
    /// String interning table for names and namespace URIs
    pub name_table: NameTable,

    /// Centralized source map storage for all documents
    pub source_maps: SourceMapStorage,

    /// All parsed schema documents
    pub documents: Vec<SchemaDocument>,

    /// Per-namespace component tables (keyed by NameId; None = no namespace)
    pub namespaces: HashMap<Option<NameId>, NamespaceTable>,

    /// XSD version mode (1.0 or 1.1)
    pub xsd_version: XsdVersion,

    /// Regex compatibility mode for pattern facets. Default `Strict`.
    pub regex_compatibility: RegexCompat,

    /// Arena storage for all components
    pub arenas: ArenasGuard,

    /// Loaded schema locations (for cycle detection)
    pub loaded_locations: HashMap<String, DocumentId>,

    /// Secondary cache for chameleon schema variants loaded under different
    /// target namespaces. Keyed by `(resolved_uri, adopted_namespace)`.
    /// Allows the same no-namespace schema to be loaded separately for
    /// each including namespace per §4.2.3.
    pub chameleon_cache: HashMap<(String, NameId), DocumentId>,

    /// Composition graph edges recorded during directive resolution
    pub composition_edges: Vec<CompositionEdge>,

    /// Effective component map with provenance (populated by composition phase).
    /// Keyed by `ComponentIdentity` so redefine/override *replaces* the entry
    /// instead of appending, producing the final visible component set.
    pub effective_components: HashMap<ComponentIdentity, EffectiveComponent>,

    /// Built-in type registry with well-known type IDs
    builtin_types: Option<BuiltinTypes>,

    /// Parsing errors collected during error-recovery mode.
    /// These are structural errors that make the schema invalid but
    /// were deferred so parsing could continue for better diagnostics.
    pub parsing_errors: Vec<crate::error::SchemaError>,

    /// The namespace bindings in scope at each `<xs:element>` /
    /// `<xs:attribute>` with a `default` or `fixed` value, keyed by its
    /// document and start offset (its `SourceRef`): the {value constraint}
    /// {value} is "the ·actual value· … of the [attribute]" (Structures
    /// §3.3.2.1, §3.2.2.1 – §3.2.2.3), and an `xs:QName` / `xs:NOTATION` value's actual
    /// value depends on "the namespace bindings in scope where the literal
    /// occurs" (Datatypes §3.3.18).
    value_constraint_namespaces:
        HashMap<(DocumentId, usize), crate::namespace::context::NamespaceContextSnapshot>,

    /// The directives whose schema document could not be read, in the
    /// order they were met (see [`SchemaSet::directive_failures`]).
    directive_failures: Vec<crate::parser::resolver::DirectiveFailure>,

    /// The nesting limit of the passes over the set's components (see
    /// [`SchemaSet::max_depth`]).
    max_depth: usize,
}

impl SchemaSet {
    /// Create a new empty schema set
    pub fn new() -> Self {
        Self::with_version(XsdVersion::V1_0)
    }

    /// Create a new schema set configured for XSD 1.0.
    pub fn xsd10() -> Self {
        Self::with_version(XsdVersion::V1_0)
    }

    /// Create a new schema set configured for XSD 1.1.
    pub fn xsd11() -> Self {
        Self::with_version(XsdVersion::V1_1)
    }

    /// Create a new schema set with specified version
    pub fn with_version(version: XsdVersion) -> Self {
        let mut set = Self {
            name_table: NameTable::new(),
            source_maps: SourceMapStorage::new(),
            documents: Vec::new(),
            namespaces: HashMap::new(),
            xsd_version: version,
            regex_compatibility: RegexCompat::Strict,
            arenas: ArenasGuard::new(),
            loaded_locations: HashMap::new(),
            chameleon_cache: HashMap::new(),
            composition_edges: Vec::new(),
            effective_components: HashMap::new(),
            builtin_types: None,
            parsing_errors: Vec::new(),
            value_constraint_namespaces: HashMap::new(),
            directive_failures: Vec::new(),
            max_depth: crate::parser::parse::DEFAULT_MAX_DEPTH,
        };

        // Initialize built-in types
        let builtin_types = BuiltinTypes::new(&mut set);
        set.builtin_types = Some(builtin_types);

        set
    }

    /// Returns `true` if this schema set is configured for XSD 1.0.
    pub fn is_xsd10(&self) -> bool {
        self.xsd_version == XsdVersion::V1_0
    }

    /// Returns `true` if this schema set is configured for XSD 1.1.
    pub fn is_xsd11(&self) -> bool {
        self.xsd_version == XsdVersion::V1_1
    }

    /// The effective (base-chain-merged) facet set for a simple type, memoized
    /// on the arenas guard. Thin forwarder to [`ArenasGuard::effective_facets`];
    /// the memo lives next to the arenas it caches so the guard's mutation gate
    /// can invalidate it, keeping the cache consistent with the live arenas.
    pub(crate) fn effective_facets(
        &self,
        sk: SimpleTypeKey,
    ) -> Arc<crate::types::facets::FacetSet> {
        self.arenas.effective_facets(sk)
    }

    /// Set the regex compatibility mode for this schema set.
    ///
    /// Affects how pattern facets in subsequently compiled schemas are
    /// validated. Has no effect on already-compiled patterns. Default is
    /// `RegexCompat::Strict`.
    pub fn set_regex_compatibility(&mut self, compat: RegexCompat) {
        self.regex_compatibility = compat;
    }

    /// Get the regex compatibility mode for this schema set.
    pub fn regex_compatibility(&self) -> RegexCompat {
        self.regex_compatibility
    }

    /// The prepared effective `enumeration` of the simple type `sk`
    /// ([`EnumerationIndex`](crate::types::facets::EnumerationIndex): the
    /// expanded names of its QName / NOTATION values and, for a union, every
    /// literal's value as the union decides it), cached with its effective
    /// facets; `None` without an enumeration.
    pub(crate) fn enumeration_index(
        &self,
        sk: SimpleTypeKey,
    ) -> Option<Arc<crate::types::facets::EnumerationIndex>> {
        let facets = self.effective_facets(sk);
        let enumeration = facets.enumeration.as_ref()?;
        let st = self.arenas.simple_types.get(sk)?;
        Some(self.arenas.enumeration_index(sk, || {
            if st.variety == crate::parser::frames::SimpleTypeVariety::Union {
                crate::validation::simple::union_enumeration_index(
                    enumeration,
                    &st.resolved_member_types,
                    self,
                )
            } else {
                crate::types::facets::EnumerationIndex::for_qnames(
                    enumeration,
                    &self.name_table,
                    self.derives_from(sk, self.builtin_types().notation),
                )
            }
        }))
    }

    /// The namespace bindings in scope at the declaration at `source` when it
    /// carries a `default` or `fixed` value — the bindings its value is read
    /// with.
    pub(crate) fn value_constraint_namespaces(
        &self,
        source: Option<&SourceRef>,
    ) -> Option<&crate::namespace::context::NamespaceContextSnapshot> {
        let source = source?;
        self.value_constraint_namespaces
            .get(&(source.doc_id, source.span.start))
    }

    /// Record the bindings of the declarations of document `doc_id` that
    /// carry a `default` or `fixed` value, by start offset.
    pub(crate) fn record_value_constraint_namespaces(
        &mut self,
        doc_id: DocumentId,
        entries: Vec<(usize, crate::namespace::context::NamespaceContextSnapshot)>,
    ) {
        self.value_constraint_namespaces.extend(
            entries
                .into_iter()
                .map(|(start, snapshot)| ((doc_id, start), snapshot)),
        );
    }

    /// Resolve an optional `SourceRef` to its line/column location.
    /// Returns `None` if the source is absent or cannot be located.
    pub fn locate(&self, source: Option<&SourceRef>) -> Option<SourceLocation> {
        source.and_then(|s| self.source_maps.locate(s))
    }

    /// The namespace that `##other` and `##targetNamespace` denote in a
    /// wildcard (`<xs:any>`, `<xs:anyAttribute>`) declared at `source`.
    ///
    /// Structures §3.10.2.2 fixes it by the wildcard's own schema document:
    /// for `##other` the {namespaces} are "a set consisting of ·absent· and,
    /// if the targetNamespace [attribute] of the `<schema>` ancestor element
    /// information item is present, its ·actual value·", and a
    /// `##targetNamespace` member is "the ·actual value· of the
    /// targetNamespace [attribute] of the `<schema>` ancestor element
    /// information item if present, otherwise ·absent·". This is that
    /// document's target namespace (after chameleon adoption, §4.2.3), not
    /// the target namespace of a type that inherits the wildcard or of a
    /// type whose content refers to a model group or attribute group of
    /// another namespace. `fallback` is used when the declaring document is
    /// unknown (synthesized wildcards).
    pub(crate) fn wildcard_target_namespace(
        &self,
        source: Option<&SourceRef>,
        fallback: Option<NameId>,
    ) -> Option<NameId> {
        source
            .and_then(|s| self.documents.get(s.doc_id as usize))
            .map_or(fallback, |doc| doc.target_namespace)
    }

    /// The `xs:include`, `xs:import`, `xs:redefine` and `xs:override`
    /// directives whose schema document could not be read, in the order they
    /// were met while the set was loaded.
    ///
    /// Such a directive is not a schema error: Structures §4.2.3 says "It is
    /// not an error for the ·actual value· of the schemaLocation \[attribute\]
    /// to fail to resolve at all, in which case the corresponding inclusion
    /// must not be performed", and §4.2.6.2 "It is not an error for the
    /// application schema component reference strategy to fail" — so the set
    /// compiles without the document. But the schema may then lack
    /// components the author meant it to have ("Failure to resolve is likely
    /// to cause less than complete ·assessment· outcomes, of course"): a
    /// host that needs the whole schema checks that this list is empty, or
    /// that each failure is of an `xs:import` location hint whose namespace
    /// another document supplies ([`DirectiveFailure::namespace_supplied`](crate::DirectiveFailure::namespace_supplied)).
    ///
    /// A document that was read but is not a schema document (not
    /// well-formed, no `xs:schema` root, the wrong target namespace) is an
    /// error, as before, and is not listed here.
    ///
    /// ```
    /// use xsd_schema::SchemaSetBuilder;
    ///
    /// let compiled = SchemaSetBuilder::new()
    ///     .add_source(
    ///         r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
    ///              <xs:include schemaLocation="no-such-file.xsd"/>
    ///              <xs:element name="root"/>
    ///            </xs:schema>"#,
    ///         "/schemas/main.xsd",
    ///     )
    ///     .unwrap()
    ///     .compile()
    ///     .expect("the set compiles without the missing document");
    /// let failures = compiled.directive_failures();
    /// assert_eq!(failures.len(), 1);
    /// assert_eq!(failures[0].schema_location(), Some("no-such-file.xsd"));
    /// assert!(failures[0].to_string().contains("could not be read"));
    /// ```
    pub fn directive_failures(&self) -> &[crate::parser::resolver::DirectiveFailure] {
        &self.directive_failures
    }

    /// Mark the failed imports whose namespace a document of the set
    /// supplies (`DirectiveFailure::namespace_supplied`). Run after the
    /// directives are resolved.
    pub(crate) fn refresh_directive_failures(&mut self) {
        let documents = &self.documents;
        let names = &self.name_table;
        for failure in &mut self.directive_failures {
            failure.refresh_supplied(|ns| {
                names.get(ns).is_some_and(|id| {
                    documents.iter().any(|d| {
                        d.target_namespace == Some(id) || d.declared_target_namespace == Some(id)
                    })
                })
            });
        }
    }

    /// Record a directive whose schema document could not be read.
    pub(crate) fn record_directive_failure(
        &mut self,
        failure: crate::parser::resolver::DirectiveFailure,
    ) {
        self.directive_failures.push(failure);
    }

    /// The nesting limit the passes over the set's components enforce: the
    /// longest chain of components nested in each other — model groups in
    /// model groups (inline or by reference), a type in its base type or in
    /// the types it lists or unites, an attribute group in the attribute
    /// groups it references, an element in its substitution group's head —
    /// may be at most this long. A schema that breaks it is refused with a
    /// [`SchemaError::LimitExceeded`](crate::SchemaError::LimitExceeded)
    /// (limit [`SchemaLimit::ComponentDepth`](crate::SchemaLimit::ComponentDepth)).
    /// `0` is no limit, and turns off this protection against stack
    /// overflow: the passes' own recursion bounds then refuse only some
    /// shapes (a content model nesting more group references than the
    /// content-model compiler's bound), and others overflow the stack.
    ///
    /// The schema-document parser bounds the element nesting of each schema
    /// document with [`ParserConfig::max_depth`](crate::parser::parse::ParserConfig::max_depth);
    /// [`SchemaSetBuilder::compile`](crate::SchemaSetBuilder::compile) and
    /// [`load_and_process_schema`](crate::load_and_process_schema) set this
    /// limit to the same value before they process the set. Both bound the
    /// recursion of the schema-time passes, so that a deeply nested schema is
    /// refused with an error instead of overflowing the stack. The default
    /// is [`DEFAULT_MAX_DEPTH`](crate::parser::parse::DEFAULT_MAX_DEPTH).
    pub fn max_depth(&self) -> usize {
        self.max_depth
    }

    /// Set the nesting limit of the passes over the set's components (see
    /// [`max_depth`](Self::max_depth)); for a set processed with
    /// [`process_loaded_schemas`](crate::process_loaded_schemas).
    pub fn set_max_depth(&mut self, max_depth: usize) {
        self.max_depth = max_depth;
    }

    /// The bound for a recursive pass whose own safety bound is `own`: never
    /// below `own`, and above the nesting limit by a margin, so that the
    /// bound is never reached by a schema within the limit. With no limit
    /// (`0`), `own`.
    pub(crate) fn recursion_bound(&self, own: usize) -> usize {
        if self.max_depth == 0 {
            own
        } else {
            own.max(self.max_depth.saturating_mul(2).saturating_add(16))
        }
    }

    /// Returns `true` if any parsing errors were collected during error-recovery parsing.
    pub fn has_parsing_errors(&self) -> bool {
        !self.parsing_errors.is_empty()
    }

    /// Iterate the normalized locations of all loaded schema documents.
    ///
    /// Useful for seeding a new [`SchemaSetBuilder`](crate::builder::SchemaSetBuilder) with the same schemas
    /// when enriching with `xsi:schemaLocation` hints.
    pub fn loaded_schema_locations(&self) -> impl Iterator<Item = &str> {
        self.loaded_locations.keys().map(|s| s.as_str())
    }

    /// Check if a schema location has already been loaded
    pub fn is_loaded(&self, location: &str) -> bool {
        // Schema-document identity is canonical-path based (symlinks and
        // case variants on case-insensitive filesystems collapse), so
        // normalize the probe the same way `mark_loaded` normalizes keys.
        let canonical = crate::parser::resolver::canonicalize_file_location(location.to_string());
        self.loaded_locations.contains_key(canonical.as_str())
    }

    /// Mark a schema location as loaded
    pub fn mark_loaded(&mut self, location: String, doc_id: DocumentId) {
        let canonical = crate::parser::resolver::canonicalize_file_location(location);
        self.loaded_locations.insert(canonical, doc_id);
    }

    /// Get or create namespace table for a namespace
    pub fn get_or_create_namespace(&mut self, ns: Option<NameId>) -> &mut NamespaceTable {
        self.namespaces.entry(ns).or_default()
    }

    /// Look up a type by namespace and name
    pub fn lookup_type(&self, ns: Option<NameId>, name: NameId) -> Option<TypeKey> {
        self.namespaces.get(&ns)?.types.get(&name).copied()
    }

    /// Look up an element by namespace and name
    pub fn lookup_element(&self, ns: Option<NameId>, name: NameId) -> Option<ElementKey> {
        self.namespaces.get(&ns)?.elements.get(&name).copied()
    }

    /// Look up an attribute by namespace and name
    pub fn lookup_attribute(&self, ns: Option<NameId>, name: NameId) -> Option<AttributeKey> {
        self.namespaces.get(&ns)?.attributes.get(&name).copied()
    }

    /// Look up a model group by namespace and name
    pub fn lookup_model_group(&self, ns: Option<NameId>, name: NameId) -> Option<ModelGroupKey> {
        self.namespaces.get(&ns)?.model_groups.get(&name).copied()
    }

    /// Look up an attribute group by namespace and name
    pub fn lookup_attribute_group(
        &self,
        ns: Option<NameId>,
        name: NameId,
    ) -> Option<AttributeGroupKey> {
        self.namespaces
            .get(&ns)?
            .attribute_groups
            .get(&name)
            .copied()
    }

    /// Look up a notation by namespace and name
    pub fn lookup_notation(&self, ns: Option<NameId>, name: NameId) -> Option<NotationKey> {
        self.namespaces.get(&ns)?.notations.get(&name).copied()
    }

    // ========================================================================
    // Built-in type access
    // ========================================================================

    /// Get the built-in types registry.
    ///
    /// This provides access to well-known type IDs for all 47+ built-in XSD types.
    pub fn builtin_types(&self) -> &BuiltinTypes {
        self.builtin_types
            .as_ref()
            .expect("BuiltinTypes should always be initialized")
    }

    /// Get a built-in simple type by QName (namespace + local name).
    ///
    /// This only looks up built-in types in the XS namespace.
    /// For user-defined types, use `lookup_type` instead.
    ///
    /// # Arguments
    /// * `namespace` - The namespace URI (should be XS namespace for built-in types)
    /// * `local_name` - The local name of the type
    ///
    /// # Returns
    /// The `SimpleTypeKey` for the built-in type, or `None` if not found.
    pub fn get_built_in_simple_type_by_qname(
        &self,
        namespace: Option<NameId>,
        local_name: NameId,
    ) -> Option<SimpleTypeKey> {
        // Built-in types are only in the XS namespace
        if namespace != Some(well_known::XS_NAMESPACE) {
            return None;
        }
        self.builtin_types().get_by_local_name(local_name)
    }

    /// Get a built-in type by QName (namespace + local name).
    ///
    /// This includes the built-in complex type `xs:anyType` and all built-in simple types.
    pub fn get_built_in_type_by_qname(
        &self,
        namespace: Option<NameId>,
        local_name: NameId,
    ) -> Option<TypeKey> {
        if namespace != Some(well_known::XS_NAMESPACE) {
            return None;
        }

        if let Some(any_type_name) = self.name_table.get("anyType") {
            if local_name == any_type_name {
                return Some(TypeKey::Complex(self.builtin_types().any_type));
            }
        }

        self.get_built_in_simple_type_by_qname(namespace, local_name)
            .map(TypeKey::Simple)
    }

    /// Get the built-in `xs:anyType` key.
    pub fn any_type_key(&self) -> ComplexTypeKey {
        self.builtin_types().any_type
    }

    /// Check if the given type key refers to `xs:anyType`.
    pub fn is_any_type(&self, type_key: TypeKey) -> bool {
        matches!(type_key, TypeKey::Complex(key) if key == self.builtin_types().any_type)
    }

    /// Get a built-in simple type by its XmlTypeCode.
    ///
    /// # Returns
    /// The `SimpleTypeKey` for the built-in type, or `None` if not a simple type code.
    pub fn get_built_in_simple_type_by_code(&self, code: XmlTypeCode) -> Option<SimpleTypeKey> {
        self.builtin_types().get_by_type_code(code)
    }

    /// Get the XmlTypeCode for a simple type.
    ///
    /// Returns `None` if the type is not a built-in type.
    pub fn get_type_code(&self, type_id: SimpleTypeKey) -> Option<XmlTypeCode> {
        self.builtin_types().get_type_code(type_id)
    }

    /// The built-in type with this code as a host of the XPath 2.0 data model
    /// sees the set — the lookup for a host that validates nodes against a
    /// type it names (an annotation it stipulates, a type test): `xs:anyType`,
    /// every built-in simple type of the set, and in both XSD versions the four
    /// types the data model adds to the XML Schema namespace (XDM 1.0 §2.6.2):
    /// `xs:anyAtomicType`, `xs:untypedAtomic`, `xs:dayTimeDuration`,
    /// `xs:yearMonthDuration` ([`BuiltinTypes::xpath_type`]). `None` for a code
    /// that names no type of the set (a node kind, `item()`, and in an XSD 1.0
    /// set `xs:dateTimeStamp` and `xs:error`).
    ///
    /// An XSD 1.0 set's schema documents still cannot name those four (XML
    /// Schema 1.0 has none of them: a reference is `src-resolve`), nor an
    /// XSD 1.1 set's `xs:untypedAtomic` (no XSD version defines it), and an
    /// `xsi:type` resolves those only for a validator with
    /// [`ValidationFlags::XPATH_TYPES`](crate::validation::ValidationFlags::XPATH_TYPES).
    ///
    /// ```
    /// use xsd_schema::{SchemaSet, TypeKey, XmlTypeCode};
    /// use xsd_schema::validation::validate_simple_type;
    ///
    /// let set = SchemaSet::new(); // XML Schema 1.0
    /// let day_time = set.xpath_type(XmlTypeCode::DayTimeDuration).unwrap();
    /// let value = validate_simple_type("PT90M", day_time, &set).unwrap();
    /// assert_eq!(value.typed_value.type_code, XmlTypeCode::DayTimeDuration);
    /// assert!(validate_simple_type("P1Y", day_time, &set).is_err());
    /// // Schema documents of the set do not see it.
    /// assert_eq!(set.builtin_types().day_time_duration, None);
    /// ```
    pub fn xpath_type(&self, code: XmlTypeCode) -> Option<TypeKey> {
        if code == XmlTypeCode::AnyType {
            return Some(TypeKey::Complex(self.any_type_key()));
        }
        self.builtin_types().xpath_type(code).map(TypeKey::Simple)
    }

    /// The built-in type an `xsi:type` naming `{namespace}local_name`
    /// resolves to: what a schema document resolves — and, with
    /// `xpath_types` (a host's [`ValidationFlags::XPATH_TYPES`](crate::validation::ValidationFlags::XPATH_TYPES)),
    /// the XPath 2.0 data model's types the set's XSD version does not define
    /// ([`xpath_type`](Self::xpath_type)).
    pub(crate) fn xsi_type_built_in(
        &self,
        namespace: Option<NameId>,
        local_name: NameId,
        xpath_types: bool,
    ) -> Option<TypeKey> {
        let found = self.get_built_in_type_by_qname(namespace, local_name);
        if found.is_some() || !xpath_types || namespace != Some(well_known::XS_NAMESPACE) {
            return found;
        }
        let code = XmlTypeCode::from_local_name(self.name_table.resolve_ref(local_name))?;
        self.builtin_types().xpath_type(code).map(TypeKey::Simple)
    }

    /// Check if `derived` derives from `base` (transitively).
    ///
    /// For built-in types, this uses the standard XSD derivation hierarchy.
    /// For user-defined types, this walks the base type chain using resolved references.
    ///
    /// # Returns
    /// - `true` if `derived == base`
    /// - `true` if `derived` has `base` somewhere in its derivation chain
    /// - `false` otherwise
    pub fn derives_from(&self, derived: SimpleTypeKey, base: SimpleTypeKey) -> bool {
        // Same type derives from itself
        if derived == base {
            return true;
        }

        // First, check if both are built-in types and use the built-in derivation
        let builtin = self.builtin_types();
        if builtin.is_builtin(derived) && builtin.is_builtin(base) {
            return builtin.derives_from(derived, base);
        }
        // An XSD 1.0 set's `xs:anyAtomicType` (a host's): every atomic type
        // derives from it (XDM 1.0 §2.6.2).
        if builtin.is_detached_any_atomic_type(base) {
            return self.is_atomic_simple_type(derived);
        }

        // For user-defined types (or mixed), walk the resolved base type chain
        let mut current = derived;
        let mut visited = std::collections::HashSet::new();

        while visited.insert(current) {
            // Get the simple type data
            if let Some(type_def) = self.arenas.simple_types.get(current) {
                // Check the resolved base type
                if let Some(crate::ids::TypeKey::Simple(simple_base)) = type_def.resolved_base_type
                {
                    if simple_base == base {
                        return true;
                    }
                    current = simple_base;
                    continue;
                }
            }

            // If no resolved base type, try built-in derivation as fallback
            if builtin.is_builtin(current) {
                if let Some(parent) = builtin.get_base_type(current) {
                    if parent == base {
                        return true;
                    }
                    current = parent;
                    continue;
                }
            }

            // No more base types to traverse
            break;
        }

        false
    }

    // ========================================================================
    // Type derivation checking (analog of C# XmlSchemaType.IsDerivedFrom)
    // ========================================================================

    /// Check if `derived` is derived from `base`, optionally filtering by derivation method.
    ///
    /// This mirrors C#'s `XmlSchemaType.IsDerivedFrom(derivedType, baseType, method)`.
    ///
    /// # Arguments
    /// * `derived` - The potentially derived type
    /// * `base` - The potential base type
    /// * `exclude_methods` - Derivation methods to exclude from the check.
    ///   Use `DerivationSet::empty()` to allow any method (like C#'s Empty).
    ///
    /// # Returns
    /// - `true` if `derived == base`
    /// - `true` if `derived` derives from `base` via a non-excluded derivation method
    /// - `false` otherwise
    ///
    /// This is the validation-time relation (Structures §3.4.6.5 / §3.16.6.3
    /// *Type Derivation OK*): a member type of a union `base` counts as derived
    /// from it (§3.16.6.3 clause 2.2.4), which is what `xsi:type` and
    /// substitution groups need. XPath 2.0's derives-from (§2.5.4) admits only
    /// restriction and extension, so a union member does not derive from its
    /// union there.
    pub fn is_type_derived_from(
        &self,
        derived: TypeKey,
        base: TypeKey,
        exclude_methods: DerivationSet,
    ) -> bool {
        self.type_derived_from(derived, base, exclude_methods, true)
    }

    /// XPath 2.0 §2.5.4 `derives-from(AT, ET)` for two types of this set:
    /// "AT … is the same as ET or is derived by restriction or extension from
    /// ET", directly or through an intermediate type — the {base type
    /// definition} chain only (the built-in hierarchy included; a list or
    /// union type derives from `xs:anySimpleType`). Unlike
    /// [`is_type_derived_from`](Self::is_type_derived_from), a member type of a
    /// union is **not** derived from the union. SequenceType matching
    /// (`instance of`, `treat as`, element / attribute / schema-element /
    /// schema-attribute tests and the casts up the hierarchy) uses this one.
    #[cfg(feature = "xsd11")]
    pub(crate) fn xpath_derives_from(&self, derived: TypeKey, base: TypeKey) -> bool {
        self.type_derived_from(derived, base, DerivationSet::empty(), false)
    }

    /// [`is_type_derived_from`](Self::is_type_derived_from) with the union
    /// member clause (§3.16.6.3 clause 2.2.4) on or off.
    fn type_derived_from(
        &self,
        derived: TypeKey,
        base: TypeKey,
        exclude_methods: DerivationSet,
        union_members: bool,
    ) -> bool {
        // Same type derives from itself
        if derived == base {
            return true;
        }

        // Everything derives from anyType. With no method exclusions we can
        // short-circuit, but with a non-empty exclusion mask we must walk the
        // chain to verify no step uses a blocked method (§3.4.6.5 / §3.16.6).
        // For Simple→AnyType the chain *always* terminates with the
        // anySimpleType→anyType restriction step, so an exclusion containing
        // RESTRICTION (or matching the simple's variety method) blocks it
        // (cvc-elt.4.3 with `block="restriction"` / `block="#all"` and
        // declared anyType: elemT026/27/28/29, elemT054/55/56/57).
        if self.is_any_type(base) {
            if exclude_methods.is_empty() {
                return true;
            }
            if let TypeKey::Simple(d) = derived {
                return self.is_simple_chain_to_any_type_ok(d, exclude_methods);
            }
            // Complex→AnyType falls through to is_complex_type_derived_from below.
        }

        match (derived, base) {
            // Case 1: Both are simple types
            (TypeKey::Simple(d), TypeKey::Simple(b)) => {
                self.is_simple_type_derived_from(d, b, exclude_methods, union_members)
            }

            // Case 2: Both are complex types
            (TypeKey::Complex(d), TypeKey::Complex(b)) => {
                self.is_complex_type_derived_from(d, b, exclude_methods)
            }

            // Case 3: Simple derives from Complex
            // All simple types derive from anyType (via anySimpleType).
            (TypeKey::Simple(_), TypeKey::Complex(_)) => false,

            // Case 4: Complex derives from Simple
            // Complex types with simpleContent can derive from simple types
            (TypeKey::Complex(d), TypeKey::Simple(b)) => {
                self.is_complex_derived_from_simple(d, b, exclude_methods, union_members)
            }
        }
    }

    /// Whether the simple→anyType derivation chain rooted at `derived` avoids
    /// every method in `exclude_methods`. Used by `is_type_derived_from` when
    /// `base = anyType` and `exclude_methods` is non-empty (§3.16.6 + the
    /// implicit anySimpleType→anyType restriction step).
    fn is_simple_chain_to_any_type_ok(
        &self,
        derived: SimpleTypeKey,
        exclude_methods: DerivationSet,
    ) -> bool {
        use crate::parser::frames::SimpleTypeVariety;

        let mut current = derived;
        let mut visited = std::collections::HashSet::new();

        while visited.insert(current) {
            if let Some(type_def) = self.arenas.simple_types.get(current) {
                let method_flag = match type_def.variety {
                    SimpleTypeVariety::Atomic => DerivationSet::RESTRICTION,
                    SimpleTypeVariety::List => DerivationSet::LIST,
                    SimpleTypeVariety::Union => DerivationSet::UNION,
                };
                if exclude_methods.contains(method_flag) {
                    return false;
                }
                if let Some(TypeKey::Simple(simple_base)) = type_def.resolved_base_type {
                    current = simple_base;
                    continue;
                }
            }
            // Fell off the user-defined simple chain — the next step is the
            // anySimpleType→anyType restriction in the built-in hierarchy.
            return !exclude_methods.contains(DerivationSet::RESTRICTION);
        }
        // Cycle detected (shouldn't happen for resolved schemas).
        false
    }

    /// Check if simple type `derived` is derived from simple type `base` with method filtering.
    ///
    /// Implements XSD spec §3.16.6.3 "Type Derivation OK (Simple)":
    /// - Clause 2.2.1/2.2.2: walks the `resolved_base_type` chain
    /// - Clause 2.2.4: if `base` is a union, checks transitive member types
    ///   (only with `union_members`; XPath's derives-from leaves it out)
    fn is_simple_type_derived_from(
        &self,
        derived: SimpleTypeKey,
        base: SimpleTypeKey,
        exclude_methods: DerivationSet,
        union_members: bool,
    ) -> bool {
        use crate::parser::frames::SimpleTypeVariety;

        // Clause 1: Same type
        if derived == base {
            return true;
        }

        // Clause 2.2.1/2.2.2: Walk the base type chain
        let builtin = self.builtin_types();

        // An XSD 1.0 set's `xs:anyAtomicType`, which only a host reaches: XDM
        // 1.0 §2.6.2 makes it the base of every primitive atomic type ("All
        // primitive atomic types … have xs:anyAtomicType as their base type"),
        // while the {base type definition} of a 1.0 primitive stays
        // `xs:anySimpleType`. So every atomic type derives from it, by
        // restriction steps only (a list or union type does not, nor does
        // `xs:anySimpleType`).
        if builtin.is_detached_any_atomic_type(base) {
            return !exclude_methods.contains(DerivationSet::RESTRICTION)
                && self.is_atomic_simple_type(derived);
        }

        let mut current = derived;
        let mut visited = std::collections::HashSet::new();

        while visited.insert(current) {
            // Get type definition
            if let Some(type_def) = self.arenas.simple_types.get(current) {
                // Determine derivation method based on variety
                let method_flag = match type_def.variety {
                    SimpleTypeVariety::Atomic => DerivationSet::RESTRICTION,
                    SimpleTypeVariety::List => DerivationSet::LIST,
                    SimpleTypeVariety::Union => DerivationSet::UNION,
                };

                // If this derivation method is excluded, stop traversal
                if exclude_methods.contains(method_flag) {
                    break;
                }

                // Check resolved base type
                if let Some(TypeKey::Simple(simple_base)) = type_def.resolved_base_type {
                    if simple_base == base {
                        return true;
                    }
                    current = simple_base;
                    continue;
                }
            }

            // Fallback to built-in derivation
            if builtin.is_builtin(current) {
                // For built-in types, derivation is always by restriction
                if exclude_methods.contains(DerivationSet::RESTRICTION) {
                    break;
                }
                if let Some(parent) = builtin.get_base_type(current) {
                    if parent == base {
                        return true;
                    }
                    current = parent;
                    continue;
                }
            }

            break;
        }

        // Clause 2.2.4: If base is a union with no facets, check whether
        // derived is derived from a transitive member type.
        if !union_members {
            return false;
        }
        if let Some(base_def) = self.arenas.simple_types.get(base) {
            if base_def.variety == SimpleTypeVariety::Union && base_def.facets.is_empty() {
                for &member_type_key in &base_def.resolved_member_types {
                    if let TypeKey::Simple(member_key) = member_type_key {
                        if self.is_simple_type_derived_from(
                            derived,
                            member_key,
                            exclude_methods,
                            union_members,
                        ) {
                            return true;
                        }
                    }
                }
            }
        }

        false
    }

    /// Whether `key` is an atomic simple type: atomic variety and not
    /// `xs:anySimpleType` (whose built-in component carries the atomic
    /// variety). A restriction of a list or union type inherits its base's
    /// variety when references are resolved.
    fn is_atomic_simple_type(&self, key: SimpleTypeKey) -> bool {
        key != self.builtin_types().any_simple_type
            && self
                .arenas
                .simple_types
                .get(key)
                .is_some_and(|def| def.variety == crate::parser::frames::SimpleTypeVariety::Atomic)
    }

    /// Check if complex type `derived` is derived from complex type `base` with method filtering.
    fn is_complex_type_derived_from(
        &self,
        derived: ComplexTypeKey,
        base: ComplexTypeKey,
        exclude_methods: DerivationSet,
    ) -> bool {
        use crate::parser::frames::DerivationMethod;

        if derived == base {
            return true;
        }

        let mut current = derived;
        let mut visited = std::collections::HashSet::new();

        while visited.insert(current) {
            if let Some(type_def) = self.arenas.complex_types.get(current) {
                // Determine derivation method flag
                let method_flag = match type_def.derivation_method {
                    Some(DerivationMethod::Extension) => DerivationSet::EXTENSION,
                    Some(DerivationMethod::Restriction) | None => DerivationSet::RESTRICTION,
                };

                // If this derivation method is excluded, stop traversal
                if exclude_methods.contains(method_flag) {
                    return false;
                }

                // Check resolved base type
                if let Some(TypeKey::Complex(complex_base)) = type_def.resolved_base_type {
                    if complex_base == base {
                        return true;
                    }
                    current = complex_base;
                    continue;
                }

                // resolved_base_type is None (shorthand complex type) or
                // Some(TypeKey::Simple(_)) (simpleContent). Both ultimately
                // derive from xs:anyType — check if that is the target.
                return base == self.any_type_key();
            }

            break;
        }

        false
    }

    /// Check if complex type `derived` (with simpleContent) derives from simple type `base`.
    fn is_complex_derived_from_simple(
        &self,
        derived: ComplexTypeKey,
        base: SimpleTypeKey,
        exclude_methods: DerivationSet,
        union_members: bool,
    ) -> bool {
        use crate::parser::frames::DerivationMethod;

        // Walk the complex type chain, stepping through each derivation level.
        // A complex type with simpleContent can have a chain:
        //   ct_n (restriction of ct_{n-1}) → … → ct_1 (extension of simple_type)
        let mut current = derived;
        let mut visited = std::collections::HashSet::new();

        while visited.insert(current) {
            let Some(type_def) = self.arenas.complex_types.get(current) else {
                break;
            };

            let method_flag = match type_def.derivation_method {
                Some(DerivationMethod::Extension) => DerivationSet::EXTENSION,
                Some(DerivationMethod::Restriction) | None => DerivationSet::RESTRICTION,
            };

            if exclude_methods.contains(method_flag) {
                return false;
            }

            match type_def.resolved_base_type {
                Some(TypeKey::Simple(simple_base)) => {
                    if simple_base == base {
                        return true;
                    }
                    // Walk further up the simple type chain.
                    return self.is_simple_type_derived_from(
                        simple_base,
                        base,
                        exclude_methods,
                        union_members,
                    );
                }
                Some(TypeKey::Complex(complex_base)) => {
                    // Base is another complex type; keep walking.
                    current = complex_base;
                }
                None => break,
            }
        }

        false
    }

    /// Format a provenance note for a component (returns empty string if none/declared).
    ///
    /// Used to enrich error messages with information about where a component
    /// originated (e.g., redefined from another schema document).
    pub fn format_provenance_note(
        &self,
        kind: ComponentKind,
        namespace: Option<NameId>,
        name: NameId,
    ) -> String {
        use crate::schema::composition::CompositionAction;

        let identity = ComponentIdentity {
            kind,
            name,
            namespace,
        };
        match self.effective_components.get(&identity) {
            Some(eff) => match &eff.action {
                CompositionAction::Redefined { from_doc, replaced } => {
                    let target_uri = replaced
                        .owner_doc
                        .and_then(|id| self.documents.get(id as usize))
                        .map(|d| d.base_uri.as_str())
                        .unwrap_or("unknown");
                    let from_uri = from_doc
                        .and_then(|id| self.documents.get(id as usize))
                        .map(|d| d.base_uri.as_str())
                        .unwrap_or("unknown");
                    format!(" (originally in {}, redefined by {})", target_uri, from_uri)
                }
                #[cfg(feature = "xsd11")]
                CompositionAction::Overridden { from_doc, replaced } => {
                    let target_uri = replaced
                        .owner_doc
                        .and_then(|id| self.documents.get(id as usize))
                        .map(|d| d.base_uri.as_str())
                        .unwrap_or("unknown");
                    let from_uri = from_doc
                        .and_then(|id| self.documents.get(id as usize))
                        .map(|d| d.base_uri.as_str())
                        .unwrap_or("unknown");
                    format!(
                        " (originally in {}, overridden by {})",
                        target_uri, from_uri
                    )
                }
                CompositionAction::Included { from_doc } => {
                    let uri = self
                        .documents
                        .get(*from_doc as usize)
                        .map(|d| d.base_uri.as_str())
                        .unwrap_or("unknown");
                    format!(" (included by {})", uri)
                }
                CompositionAction::Declared => String::new(),
            },
            None => String::new(),
        }
    }

    /// Compute the effective namespace for a local element declaration per XSD spec.
    ///
    /// Rules: explicit targetNamespace > form attribute > elementFormDefault > Unqualified.
    /// Qualified → document target namespace; Unqualified → None.
    pub fn effective_local_element_namespace(
        &self,
        elem_target_namespace: Option<NameId>,
        elem_form: Option<&str>,
        source: Option<&SourceRef>,
        fallback_namespace: Option<NameId>,
    ) -> Option<NameId> {
        self.effective_local_namespace(
            elem_target_namespace,
            elem_form,
            source,
            fallback_namespace,
            |d| d.element_form_default,
        )
    }

    /// Compute the effective namespace for a local attribute declaration per XSD spec.
    ///
    /// Rules: explicit targetNamespace > form attribute > attributeFormDefault > Unqualified.
    /// Qualified → document target namespace; Unqualified → None.
    pub fn effective_local_attribute_namespace(
        &self,
        attr_target_namespace: Option<NameId>,
        attr_form: Option<&str>,
        source: Option<&SourceRef>,
        fallback_namespace: Option<NameId>,
    ) -> Option<NameId> {
        self.effective_local_namespace(
            attr_target_namespace,
            attr_form,
            source,
            fallback_namespace,
            |d| d.attribute_form_default,
        )
    }

    fn effective_local_namespace(
        &self,
        explicit_target_namespace: Option<NameId>,
        form: Option<&str>,
        source: Option<&SourceRef>,
        fallback_namespace: Option<NameId>,
        form_default: impl Fn(&SchemaDocument) -> FormChoice,
    ) -> Option<NameId> {
        if explicit_target_namespace.is_some() {
            return explicit_target_namespace;
        }
        // Use defaults_doc() so override children read the overridden
        // document's form defaults per §4.2.5 / F.2 semantics.
        let doc = source.and_then(|s| self.documents.get(s.defaults_doc() as usize));
        let default_form = doc.map(&form_default).unwrap_or(FormChoice::Unqualified);
        let target_namespace = doc
            .map(|d| d.target_namespace)
            .unwrap_or(fallback_namespace);
        let resolved_form = match form {
            Some("qualified") => FormChoice::Qualified,
            Some("unqualified") => FormChoice::Unqualified,
            _ => default_form,
        };
        match resolved_form {
            FormChoice::Qualified => target_namespace,
            FormChoice::Unqualified => None,
        }
    }
}

impl Default for SchemaSet {
    fn default() -> Self {
        Self::new()
    }
}

/// A single schema document (root or included/imported)
///
/// Represents one XSD file with its components and directives.
#[derive(Debug)]
pub struct SchemaDocument {
    /// Document ID for source map reference
    pub id: DocumentId,

    /// Base URI (location) of this document
    pub base_uri: String,

    /// The `targetNamespace` as declared in the `<xs:schema>` element.
    /// `None` when the schema document omits `targetNamespace`.
    /// Preserved even after chameleon adoption so the original fact
    /// "this document had no declared namespace" is never lost.
    pub declared_target_namespace: Option<NameId>,

    /// Effective target namespace after chameleon pre-processing (§4.2.3).
    /// Equals `declared_target_namespace` for non-chameleon documents;
    /// set to the includer's namespace for chameleon-adopted documents.
    pub target_namespace: Option<NameId>,

    /// Schema-level attributes
    pub version: Option<String>,
    pub element_form_default: FormChoice,
    pub attribute_form_default: FormChoice,
    pub block_default: DerivationSet,
    pub final_default: DerivationSet,
    pub schema_id: Option<String>,
    pub xml_lang: Option<String>,

    /// XSD 1.1: Default attributes group reference
    pub default_attributes: Option<QualifiedName>,

    /// XSD 1.1: Default namespace for XPath
    pub xpath_default_namespace: Option<NameId>,

    /// Composition directives (in document order)
    pub includes: Vec<IncludeDirective>,
    pub imports: Vec<ImportDirective>,
    pub redefines: Vec<RedefineDirective>,
    pub overrides: Vec<OverrideDirective>, // XSD 1.1

    /// XSD 1.1: Default open content
    pub default_open_content: Option<DefaultOpenContent>,

    /// Schema-level annotations
    pub annotations: Vec<Annotation>,

    /// Per-document index of top-level components declared in this document.
    /// Populated during assembly; used for document-scoped lookup in
    /// `apply_redefine()` and `apply_override()`.
    pub component_index: DocumentComponentIndex,

    /// Source reference for error reporting
    pub source: Option<SourceRef>,
}

impl SchemaDocument {
    /// Whether this document had no declared `targetNamespace` and adopted
    /// one via chameleon include pre-processing (§4.2.3 clause 2.3).
    pub fn is_chameleon(&self) -> bool {
        self.declared_target_namespace.is_none() && self.target_namespace.is_some()
    }

    /// Per-document QName visibility per XSD §3.17.6.2 `src-resolve` clause 4.
    ///
    /// Returns `true` when a QName whose resolved namespace is `qname_ns`
    /// may be referenced from this schema document. Resolution is strictly
    /// per-document and lexical: imports are not transitive.
    ///
    /// Reads `declared_target_namespace` for clause 4.1.1 / 4.2.1 so chameleon
    /// includes (no declared `targetNamespace`) take 4.1.1 for absent-NS QNames
    /// instead of failing 4.2.1 against the includer's NS. For chameleon docs
    /// whose QNames were rewritten by §4.2.3 adoption to the includer's NS,
    /// also accept the post-adoption `target_namespace`.
    pub fn can_see_namespace(&self, qname_ns: Option<NameId>, name_table: &NameTable) -> bool {
        if qname_ns.is_none() {
            // 4.1
            return self.declared_target_namespace.is_none()
                || self.imports.iter().any(|i| i.namespace.is_none());
        }
        // 4.2.1 (incl. chameleon-adopted target namespace)
        if qname_ns == self.declared_target_namespace
            || (self.is_chameleon() && qname_ns == self.target_namespace)
        {
            return true;
        }
        // 4.2.3 / 4.2.4
        if qname_ns == Some(well_known::XS_NAMESPACE) || qname_ns == Some(well_known::XSI_NAMESPACE)
        {
            return true;
        }
        // 4.2.2. `import.namespace` is `Option<String>` (not pre-interned), but every
        // *used* namespace is interned at parse time, so `get` (read-only) suffices —
        // a string never interned cannot match the already-interned `qname_ns`.
        self.imports.iter().any(|i| {
            i.namespace
                .as_deref()
                .and_then(|s| name_table.get(s))
                .map(|id| Some(id) == qname_ns)
                .unwrap_or(false)
        })
    }

    /// Create a new schema document
    pub fn new(id: DocumentId, base_uri: String) -> Self {
        Self {
            id,
            base_uri,
            declared_target_namespace: None,
            target_namespace: None,
            version: None,
            element_form_default: FormChoice::default(),
            attribute_form_default: FormChoice::default(),
            block_default: DerivationSet::empty(),
            final_default: DerivationSet::empty(),
            schema_id: None,
            xml_lang: None,
            default_attributes: None,
            xpath_default_namespace: None,
            includes: Vec::new(),
            imports: Vec::new(),
            redefines: Vec::new(),
            overrides: Vec::new(),
            default_open_content: None,
            annotations: Vec::new(),
            component_index: DocumentComponentIndex::new(),
            source: None,
        }
    }
}

/// Per-namespace component lookup tables
///
/// Each namespace has its own table mapping local names to component keys.
/// Uses NameId as keys for fast equality checks.
#[derive(Debug, Default)]
pub struct NamespaceTable {
    /// Type definitions (simple and complex)
    pub types: HashMap<NameId, TypeKey>,
    /// Element declarations
    pub elements: HashMap<NameId, ElementKey>,
    /// Attribute declarations
    pub attributes: HashMap<NameId, AttributeKey>,
    /// Attribute groups
    pub attribute_groups: HashMap<NameId, AttributeGroupKey>,
    /// Named model groups
    pub model_groups: HashMap<NameId, ModelGroupKey>,
    /// Notations
    pub notations: HashMap<NameId, NotationKey>,
    /// Identity constraints (global, for XSD 1.1 refs)
    pub identity_constraints: HashMap<NameId, IdentityConstraintKey>,
}

impl NamespaceTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a type in this namespace
    pub fn register_type(&mut self, name: NameId, key: TypeKey) -> Option<TypeKey> {
        self.types.insert(name, key)
    }

    /// Register an element in this namespace
    pub fn register_element(&mut self, name: NameId, key: ElementKey) -> Option<ElementKey> {
        self.elements.insert(name, key)
    }

    /// Register an attribute in this namespace
    pub fn register_attribute(&mut self, name: NameId, key: AttributeKey) -> Option<AttributeKey> {
        self.attributes.insert(name, key)
    }

    /// Register a model group in this namespace
    pub fn register_model_group(
        &mut self,
        name: NameId,
        key: ModelGroupKey,
    ) -> Option<ModelGroupKey> {
        self.model_groups.insert(name, key)
    }

    /// Register an attribute group in this namespace
    pub fn register_attribute_group(
        &mut self,
        name: NameId,
        key: AttributeGroupKey,
    ) -> Option<AttributeGroupKey> {
        self.attribute_groups.insert(name, key)
    }

    /// Register a notation in this namespace
    pub fn register_notation(&mut self, name: NameId, key: NotationKey) -> Option<NotationKey> {
        self.notations.insert(name, key)
    }
}

// Schema composition directives

/// xs:include directive
#[derive(Debug, Clone)]
pub struct IncludeDirective {
    pub source: Option<SourceRef>,
    pub schema_location: String,
    pub resolved_doc_id: Option<DocumentId>,
}

/// xs:import directive
#[derive(Debug, Clone)]
pub struct ImportDirective {
    pub source: Option<SourceRef>,
    pub namespace: Option<String>,
    pub schema_location: Option<String>,
    pub resolved_doc_id: Option<DocumentId>,
}

/// xs:redefine directive (deprecated in XSD 1.1)
#[derive(Debug, Clone)]
pub struct RedefineDirective {
    pub source: Option<SourceRef>,
    pub schema_location: String,
    pub resolved_doc_id: Option<DocumentId>,
    pub simple_types: Vec<SimpleTypeKey>,
    pub complex_types: Vec<ComplexTypeKey>,
    pub groups: Vec<ModelGroupKey>,
    pub attribute_groups: Vec<AttributeGroupKey>,
}

/// xs:override directive (XSD 1.1)
#[derive(Debug, Clone)]
pub struct OverrideDirective {
    pub source: Option<SourceRef>,
    pub schema_location: String,
    pub resolved_doc_id: Option<DocumentId>,
    pub components: Vec<OverrideComponent>,
}

/// Component that can appear in xs:override
#[derive(Debug, Clone)]
pub enum OverrideComponent {
    SimpleType(SimpleTypeKey),
    ComplexType(ComplexTypeKey),
    Group(ModelGroupKey),
    AttributeGroup(AttributeGroupKey),
    Element(ElementKey),
    Attribute(AttributeKey),
    Notation(NotationKey),
}

/// Default open content at schema level (XSD 1.1)
#[derive(Debug, Clone)]
pub struct DefaultOpenContent {
    pub source: Option<SourceRef>,
    pub applies_to_empty: bool,
    pub mode: OpenContentMode,
    pub wildcard: Option<ElementWildcard>,
}

/// Open content mode (XSD 1.1)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OpenContentMode {
    None,
    #[default]
    Interleave,
    Suffix,
}

impl From<crate::parser::frames::OpenContentMode> for OpenContentMode {
    fn from(m: crate::parser::frames::OpenContentMode) -> Self {
        use crate::parser::frames::OpenContentMode as Src;
        match m {
            Src::None => Self::None,
            Src::Interleave => Self::Interleave,
            Src::Suffix => Self::Suffix,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arenas::ComplexTypeDefData;
    use crate::parser::frames::ComplexContentResult;

    #[test]
    fn test_schema_set_creation() {
        let set = SchemaSet::new();
        assert_eq!(set.xsd_version, XsdVersion::V1_0);
        assert!(set.documents.is_empty());
        // XSI namespace is pre-populated with built-in attribute declarations
        assert!(set
            .namespaces
            .contains_key(&Some(crate::namespace::table::well_known::XSI_NAMESPACE)));
    }

    #[test]
    fn test_schema_set_with_version() {
        let set = SchemaSet::with_version(XsdVersion::V1_1);
        assert_eq!(set.xsd_version, XsdVersion::V1_1);
    }

    #[test]
    fn test_schema_set_xsd10() {
        let set = SchemaSet::xsd10();
        assert_eq!(set.xsd_version, XsdVersion::V1_0);
    }

    #[test]
    fn test_schema_set_xsd11() {
        let set = SchemaSet::xsd11();
        assert_eq!(set.xsd_version, XsdVersion::V1_1);
    }

    #[test]
    fn test_namespace_table_registration() {
        use slotmap::SlotMap;
        let mut table = NamespaceTable::new();

        // Create a dummy key for testing
        let mut dummy_map: SlotMap<SimpleTypeKey, ()> = SlotMap::with_key();
        let key1 = dummy_map.insert(());
        let key2 = dummy_map.insert(());

        // Register a type
        let old = table.register_type(NameId(1), TypeKey::Simple(key1));
        assert!(old.is_none());

        // Re-registering returns old value
        let old = table.register_type(NameId(1), TypeKey::Simple(key2));
        assert!(old.is_some());
    }

    #[test]
    fn test_schema_set_load_tracking() {
        let mut set = SchemaSet::new();

        assert!(!set.is_loaded("test.xsd"));
        set.mark_loaded("test.xsd".to_string(), 0);
        assert!(set.is_loaded("test.xsd"));
    }

    #[test]
    fn test_derivation_set_flags() {
        let mut flags = DerivationSet::empty();
        assert!(flags.is_empty());

        flags |= DerivationSet::EXTENSION;
        assert!(flags.contains(DerivationSet::EXTENSION));
        assert!(!flags.contains(DerivationSet::RESTRICTION));

        let all = DerivationSet::ALL;
        assert!(all.contains(DerivationSet::EXTENSION));
        assert!(all.contains(DerivationSet::RESTRICTION));
    }

    #[test]
    fn test_form_choice_default() {
        assert_eq!(FormChoice::default(), FormChoice::Unqualified);
    }

    // ========================================================================
    // Tests for is_type_derived_from (analog of C# XmlSchemaType.IsDerivedFrom)
    // ========================================================================

    #[test]
    fn test_is_type_derived_from_same_type() {
        let set = SchemaSet::new();
        let string_key = set.builtin_types().string;

        // Same type derives from itself
        assert!(set.is_type_derived_from(
            TypeKey::Simple(string_key),
            TypeKey::Simple(string_key),
            DerivationSet::empty()
        ));
    }

    #[test]
    fn test_is_type_derived_from_direct_derivation() {
        let set = SchemaSet::new();
        let builtin = set.builtin_types();

        // xs:normalizedString derives from xs:string
        assert!(set.is_type_derived_from(
            TypeKey::Simple(builtin.normalized_string),
            TypeKey::Simple(builtin.string),
            DerivationSet::empty()
        ));

        // xs:integer derives from xs:decimal
        assert!(set.is_type_derived_from(
            TypeKey::Simple(builtin.integer),
            TypeKey::Simple(builtin.decimal),
            DerivationSet::empty()
        ));
    }

    #[test]
    fn test_is_type_derived_from_transitive() {
        let set = SchemaSet::new();
        let builtin = set.builtin_types();

        // xs:NCName derives from xs:string (NCName < Name < token < normalizedString < string)
        assert!(set.is_type_derived_from(
            TypeKey::Simple(builtin.ncname),
            TypeKey::Simple(builtin.string),
            DerivationSet::empty()
        ));

        // xs:byte derives from xs:decimal (byte < short < int < long < integer < decimal)
        assert!(set.is_type_derived_from(
            TypeKey::Simple(builtin.byte),
            TypeKey::Simple(builtin.decimal),
            DerivationSet::empty()
        ));

        // xs:ID derives from xs:string (ID < NCName < Name < token < normalizedString < string)
        assert!(set.is_type_derived_from(
            TypeKey::Simple(builtin.id),
            TypeKey::Simple(builtin.string),
            DerivationSet::empty()
        ));
    }

    #[test]
    fn test_is_type_derived_from_not_derived() {
        let set = SchemaSet::new();
        let builtin = set.builtin_types();

        // xs:string does NOT derive from xs:integer
        assert!(!set.is_type_derived_from(
            TypeKey::Simple(builtin.string),
            TypeKey::Simple(builtin.integer),
            DerivationSet::empty()
        ));

        // xs:decimal does NOT derive from xs:integer (reverse direction)
        assert!(!set.is_type_derived_from(
            TypeKey::Simple(builtin.decimal),
            TypeKey::Simple(builtin.integer),
            DerivationSet::empty()
        ));

        // xs:date does NOT derive from xs:duration
        assert!(!set.is_type_derived_from(
            TypeKey::Simple(builtin.date),
            TypeKey::Simple(builtin.duration),
            DerivationSet::empty()
        ));
    }

    #[test]
    fn test_is_type_derived_from_any_simple_type() {
        let set = SchemaSet::new();
        let builtin = set.builtin_types();

        // All simple types derive from xs:anySimpleType
        assert!(set.is_type_derived_from(
            TypeKey::Simple(builtin.string),
            TypeKey::Simple(builtin.any_simple_type),
            DerivationSet::empty()
        ));

        assert!(set.is_type_derived_from(
            TypeKey::Simple(builtin.integer),
            TypeKey::Simple(builtin.any_simple_type),
            DerivationSet::empty()
        ));

        assert!(set.is_type_derived_from(
            TypeKey::Simple(builtin.byte),
            TypeKey::Simple(builtin.any_simple_type),
            DerivationSet::empty()
        ));
    }

    #[test]
    fn test_is_type_derived_from_any_type() {
        let mut set = SchemaSet::new();
        let any_type = set.builtin_types().any_type;
        let string_type = set.builtin_types().string;

        assert!(set.is_type_derived_from(
            TypeKey::Simple(string_type),
            TypeKey::Complex(any_type),
            DerivationSet::empty()
        ));

        let ct_key = set.arenas.alloc_complex_type(ComplexTypeDefData {
            name: None,
            target_namespace: None,
            base_type: None,
            derivation_method: None,
            content: ComplexContentResult::Empty,
            open_content: None,
            attributes: Vec::new(),
            attribute_groups: Vec::new(),
            attribute_wildcard: None,
            mixed: false,
            is_abstract: false,
            final_derivation: DerivationSet::empty(),
            block: DerivationSet::empty(),
            default_attributes_apply: true,
            id: None,
            #[cfg(feature = "xsd11")]
            assertions: Vec::new(),
            #[cfg(feature = "xsd11")]
            xpath_default_namespace: None,
            annotation: None,
            source: None,
            resolved_base_type: None,
            resolved_attribute_groups: Vec::new(),
            resolved_attributes: Vec::new(),
            resolved_content_particle_types: Vec::new(),
            resolved_content_particle_elements: Vec::new(),
            resolved_simple_content_type: None,
            redefine_original: None,
        });

        assert!(set.is_type_derived_from(
            TypeKey::Complex(ct_key),
            TypeKey::Complex(any_type),
            DerivationSet::empty()
        ));
    }

    #[test]
    fn test_is_type_derived_from_exclude_restriction() {
        let set = SchemaSet::new();
        let builtin = set.builtin_types();

        // With RESTRICTION excluded, xs:normalizedString does NOT derive from xs:string
        assert!(!set.is_type_derived_from(
            TypeKey::Simple(builtin.normalized_string),
            TypeKey::Simple(builtin.string),
            DerivationSet::RESTRICTION
        ));

        // Same type still derives from itself even with exclusions
        assert!(set.is_type_derived_from(
            TypeKey::Simple(builtin.string),
            TypeKey::Simple(builtin.string),
            DerivationSet::RESTRICTION
        ));
    }

    #[test]
    fn test_is_type_derived_from_list_types() {
        let set = SchemaSet::new();
        let builtin = set.builtin_types();

        // xs:NMTOKENS is a list type that derives from xs:anySimpleType
        assert!(set.is_type_derived_from(
            TypeKey::Simple(builtin.nmtokens),
            TypeKey::Simple(builtin.any_simple_type),
            DerivationSet::empty()
        ));

        // With LIST excluded, xs:NMTOKENS should not derive from xs:anySimpleType
        assert!(!set.is_type_derived_from(
            TypeKey::Simple(builtin.nmtokens),
            TypeKey::Simple(builtin.any_simple_type),
            DerivationSet::LIST
        ));
    }

    /// Clause 2.2.4: D is derived from union B if D is derived from a
    /// transitive member of B and B has no facets.
    #[test]
    fn test_union_member_derivation_clause_2_2_4() {
        use crate::pipeline::load_and_process_schema;

        let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:simpleType name="myUnion">
                <xs:union memberTypes="xs:float xs:integer"/>
            </xs:simpleType>
        </xs:schema>"#;

        let mut set = SchemaSet::new();
        load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut set, None)
            .expect("schema should parse");

        let integer_key = TypeKey::Simple(set.builtin_types().integer);
        let float_key = TypeKey::Simple(set.builtin_types().float);

        // Find myUnion by name
        let union_name = set.name_table.add("myUnion");
        let union_key = set
            .namespaces
            .get(&None)
            .unwrap()
            .types
            .get(&union_name)
            .copied()
            .expect("myUnion should exist");

        // xs:integer is a member of myUnion → should be "derived" via 2.2.4
        assert!(
            set.is_type_derived_from(integer_key, union_key, DerivationSet::empty()),
            "xs:integer should be derived from union(float, integer) via clause 2.2.4"
        );

        // xs:float is also a member
        assert!(
            set.is_type_derived_from(float_key, union_key, DerivationSet::empty()),
            "xs:float should be derived from union(float, integer) via clause 2.2.4"
        );

        // xs:string is NOT a member
        let string_key = TypeKey::Simple(set.builtin_types().string);
        assert!(
            !set.is_type_derived_from(string_key, union_key, DerivationSet::empty()),
            "xs:string should NOT be derived from union(float, integer)"
        );

        // XPath 2.0 §2.5.4 derives-from: restriction / extension only, so a
        // member type does not derive from its union (C69); the union itself
        // derives from xs:anySimpleType and from itself.
        #[cfg(feature = "xsd11")]
        {
            assert!(!set.xpath_derives_from(integer_key, union_key));
            assert!(!set.xpath_derives_from(float_key, union_key));
            assert!(set.xpath_derives_from(union_key, union_key));
            let any_simple = TypeKey::Simple(set.builtin_types().any_simple_type);
            assert!(set.xpath_derives_from(union_key, any_simple));
            assert!(
                set.xpath_derives_from(integer_key, TypeKey::Simple(set.builtin_types().decimal))
            );
        }
    }

    /// Clause 2.2.4 with nested unions: D derived from a member of
    /// a union that is itself a member of B.
    #[test]
    fn test_union_member_derivation_transitive() {
        use crate::pipeline::load_and_process_schema;

        let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:simpleType name="innerUnion">
                <xs:union memberTypes="xs:boolean xs:date"/>
            </xs:simpleType>
            <xs:simpleType name="outerUnion">
                <xs:union memberTypes="xs:integer innerUnion"/>
            </xs:simpleType>
        </xs:schema>"#;

        let mut set = SchemaSet::new();
        load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut set, None)
            .expect("schema should parse");

        let outer_name = set.name_table.add("outerUnion");
        let outer_key = set
            .namespaces
            .get(&None)
            .unwrap()
            .types
            .get(&outer_name)
            .copied()
            .expect("outerUnion should exist");

        // xs:boolean is in innerUnion which is in outerUnion → transitive
        let bool_key = TypeKey::Simple(set.builtin_types().boolean);
        assert!(
            set.is_type_derived_from(bool_key, outer_key, DerivationSet::empty()),
            "xs:boolean should be transitively derived from outerUnion via innerUnion"
        );

        // xs:integer is a direct member
        let int_key = TypeKey::Simple(set.builtin_types().integer);
        assert!(
            set.is_type_derived_from(int_key, outer_key, DerivationSet::empty()),
            "xs:integer should be derived from outerUnion as direct member"
        );
    }
}
