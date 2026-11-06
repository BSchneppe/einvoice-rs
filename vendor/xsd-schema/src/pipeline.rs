//! Schema processing pipeline
//!
//! This module provides a high-level orchestration function that coordinates
//! all phases of schema processing:
//!
//! 1. **Parse Phase**: Parse the primary XSD document
//! 2. **Directive Resolution Phase**: Process include/import/redefine/override directives
//! 3. **Redefine/Override Application Phase**: Apply component replacements
//! 4. **Inline Type Assembly Phase**: Materialize inline type definitions
//! 5. **Reference Resolution Phase**: Resolve QName references to component keys
//!
//! # Usage
//!
//! ```
//! use xsd_schema::{SchemaSet, load_and_process_schema};
//!
//! let mut schema_set = SchemaSet::new();
//! let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
//!     <xs:element name="root" type="xs:string"/>
//! </xs:schema>"#;
//!
//! // XSD version is derived from schema_set (V1_0 by default).
//! // Use SchemaSet::xsd11() for XSD 1.1 schemas.
//! let result = load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None)
//!     .expect("failed to process schema");
//! println!("Processed {} inline types", result.inline_stats.unwrap().total_inline_types);
//! println!("Resolved {} type references", result.resolution_stats.unwrap().types_resolved);
//! ```

use crate::error::SchemaResult;
use crate::ids::DocumentId;
use crate::parser::parse::{parse_schema_with_config, ParserConfig};
#[cfg(feature = "async")]
use crate::parser::resolver::resolve_directive_closure_async;
use crate::parser::resolver::{
    fixup_composition_edges, resolve_directive_closure, DirectiveClosure, ResolutionResult,
    ResolverConfig, SchemaResolver,
};
use crate::schema::{
    allocate_content_particle_elements, allocate_model_group_particle_elements,
    assemble_inline_types, build_dependency_graph, compile_all_patterns, resolve_all_references,
    validate_all_derivations, validate_attribute_id_constraints,
    validate_attribute_value_constraints, validate_element_value_constraints, InlineAssemblyStats,
    ResolutionStats,
};
use crate::SchemaSet;

/// Configuration for the schema processing pipeline
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Parser configuration
    pub parser: ParserConfig,
    /// Resolver configuration for include/import handling
    pub resolver: ResolverConfig,
    /// Whether to load external schemas via include/import/redefine/override directives.
    /// When false, no I/O is performed and redefine/override application is deferred
    /// (callers should use `process_loaded_schemas` after all schemas are parsed).
    pub resolve_directives: bool,
    /// Whether to assemble inline types
    pub assemble_inline_types: bool,
    /// Whether to resolve QName references
    pub resolve_references: bool,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            parser: ParserConfig::default(),
            resolver: ResolverConfig::default(),
            resolve_directives: true,
            assemble_inline_types: true,
            resolve_references: true,
        }
    }
}

impl PipelineConfig {
    /// Create a minimal configuration that only parses (no directive/type resolution)
    pub fn parse_only() -> Self {
        Self {
            parser: ParserConfig::default(),
            resolver: ResolverConfig::default(),
            resolve_directives: false,
            assemble_inline_types: false,
            resolve_references: false,
        }
    }

    /// Create a configuration for full processing
    pub fn full() -> Self {
        Self::default()
    }
}

/// Statistics from processing the entire pipeline
#[derive(Debug, Default)]
pub struct PipelineStats {
    /// The primary document ID
    pub doc_id: DocumentId,
    /// Document IDs loaded via include/import directives
    pub loaded_docs: Vec<DocumentId>,
    /// Directive resolution result
    pub directive_result: Option<DirectiveStats>,
    /// Inline type assembly statistics
    pub inline_stats: Option<InlineAssemblyStats>,
    /// Reference resolution statistics
    pub resolution_stats: Option<ResolutionStats>,
}

/// Statistics from directive resolution
#[derive(Debug, Default)]
pub struct DirectiveStats {
    /// Number of schemas loaded successfully
    pub loaded_count: usize,
    /// Number of schemas skipped (already loaded/circular)
    pub skipped_count: usize,
    /// Number of errors during directive resolution
    pub error_count: usize,
}

impl From<&ResolutionResult> for DirectiveStats {
    fn from(result: &ResolutionResult) -> Self {
        Self {
            loaded_count: result.loaded.len(),
            skipped_count: result.skipped.len(),
            error_count: result.errors.len() + result.import_errors.len(),
        }
    }
}

/// Load and fully process an XSD schema document
///
/// This is the main entry point for schema processing. It orchestrates all
/// phases of schema handling:
///
/// 1. **Parse**: Parse the primary XSD document
/// 2. **Directives**: Load and parse included/imported/redefined/overridden schemas
/// 3. **Redefine/Override**: Apply component replacements from redefine/override directives
/// 4. **Inline Assembly**: Allocate inline type definitions in arenas
/// 5. **Reference Resolution**: Resolve QName references to component keys
///
/// With the full configuration (the default) the schema is checked exactly as
/// [`SchemaSetBuilder::compile`](crate::SchemaSetBuilder::compile) checks it:
/// the same directive rules (the XML catalog included), the same phases
/// ([`process_loaded_schemas`]), the same nesting limits
/// (`config.parser.max_depth`, `config.resolver.max_depth`). A directive
/// whose document cannot be read is kept on the set
/// ([`SchemaSet::directive_failures`]).
///
/// # Arguments
///
/// * `xml` - Raw XML bytes of the schema document
/// * `base_uri` - Base URI for this document (for error messages and directive resolution)
/// * `schema_set` - Schema set to add the parsed document to
/// * `config` - Optional pipeline configuration (uses defaults if None)
///
/// # Returns
///
/// Pipeline statistics including document IDs and processing counts, or an error.
///
/// # Example
///
/// ```
/// use xsd_schema::{SchemaSet, load_and_process_schema};
///
/// let mut schema_set = SchemaSet::new();
/// let xsd = r#"<?xml version="1.0"?>
/// <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
///     <xs:element name="root">
///         <xs:complexType>
///             <xs:sequence>
///                 <xs:element name="child" type="xs:string"/>
///             </xs:sequence>
///         </xs:complexType>
///     </xs:element>
/// </xs:schema>"#;
///
/// let stats = load_and_process_schema(xsd.as_bytes(), "schema.xsd", &mut schema_set, None)
///     .expect("failed to process schema");
/// assert!(stats.inline_stats.unwrap().total_inline_types > 0);
/// ```
pub fn load_and_process_schema(
    xml: &[u8],
    base_uri: &str,
    schema_set: &mut SchemaSet,
    config: Option<PipelineConfig>,
) -> SchemaResult<PipelineStats> {
    let config = config.unwrap_or_default();
    let mut stats = PipelineStats::default();

    // Phase 1: Parse the primary schema document
    let doc_id = parse_schema_with_config(xml, base_uri, schema_set, &config.parser)?;
    stats.doc_id = doc_id;

    // Phase 2: Resolve directives (include/import/redefine/override), with
    // the builder's rules
    if config.resolve_directives {
        let mut resolver = pipeline_resolver(&config);
        mark_primary_loaded(&resolver, base_uri, doc_id, schema_set);
        let closure = resolve_directive_closure(&[doc_id], &mut resolver, schema_set);
        finish_directives(schema_set, closure, &mut stats)?;
    }

    run_phases(schema_set, &config, &mut stats)?;
    Ok(stats)
}

/// The resolver of the pipeline: the configured one, with the XML catalog
/// the builder adds (the `xml` and `xlink` namespaces map to the embedded
/// schemas).
fn pipeline_resolver(config: &PipelineConfig) -> SchemaResolver {
    let mut resolver = SchemaResolver::with_config(config.resolver.clone());
    resolver.catalog_mut().add_xml_catalog();
    resolver
}

/// Record the primary document under its location — spelled as a directive
/// of the document resolves it — so that a directive that reads it again (a
/// cycle of includes) finds it, as in the builder.
fn mark_primary_loaded(
    resolver: &SchemaResolver,
    base_uri: &str,
    doc_id: DocumentId,
    schema_set: &mut SchemaSet,
) {
    let Ok(location) = resolver.resolve_location(base_uri, "") else {
        return;
    };
    if !schema_set.is_loaded(&location) {
        schema_set.mark_loaded(location, doc_id);
    }
}

/// After directive resolution: the statistics, the composition graph's
/// cycle edges, and the first error that makes the schema invalid.
fn finish_directives(
    schema_set: &mut SchemaSet,
    closure: DirectiveClosure,
    stats: &mut PipelineStats,
) -> SchemaResult<()> {
    stats.loaded_docs.extend(closure.loaded.iter().copied());
    stats.directive_result = Some(DirectiveStats {
        loaded_count: closure.loaded.len(),
        skipped_count: closure.skipped_count,
        error_count: closure.errors.len() + closure.import_errors.len(),
    });

    // Fixup cycle edges now that all documents have been loaded
    fixup_composition_edges(schema_set);

    // Propagate schema-content errors from directive resolution.
    // Resolution/IO errors are non-fatal for all directive types; they are
    // kept on the set (`SchemaSet::directive_failures`).
    match closure.into_first_schema_error() {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

/// The phases after directive resolution, as the configuration selects
/// them. The full configuration runs exactly what
/// [`process_loaded_schemas`] — the builder's path — runs.
fn run_phases(
    schema_set: &mut SchemaSet,
    config: &PipelineConfig,
    stats: &mut PipelineStats,
) -> SchemaResult<()> {
    // The components of the set are processed with this configuration's
    // limit; a parse-only call leaves the set's own.
    if config.assemble_inline_types || config.resolve_references {
        schema_set.set_max_depth(config.parser.max_depth);
    }
    if config.assemble_inline_types && config.resolve_references {
        let (inline_stats, resolution_stats) = process_loaded_schemas(schema_set)?;
        stats.inline_stats = Some(inline_stats);
        stats.resolution_stats = Some(resolution_stats);
        return Ok(());
    }

    // Fail early if parsing collected structural errors (error-recovery mode)
    if !schema_set.parsing_errors.is_empty() {
        let errors = std::mem::take(&mut schema_set.parsing_errors);
        return Err(errors.into_iter().next().unwrap());
    }

    // A partial configuration (parse only, or one of assembly and
    // resolution). Redefine/override application is skipped in parse-only
    // mode because not all schemas may be loaded yet; callers use
    // process_loaded_schemas() to apply later.
    if config.assemble_inline_types || config.resolve_references {
        crate::schema::apply_redefine_override(schema_set)?;
    }
    if config.assemble_inline_types {
        let inline_stats = assemble_inline_types(schema_set)?;
        stats.inline_stats = Some(inline_stats);
    }
    if config.resolve_references {
        crate::schema::nesting::check_nesting_depth(schema_set)?;
        let resolution_stats = resolve_all_references(schema_set)?;
        stats.resolution_stats = Some(resolution_stats);
        compile_all_patterns(schema_set)?;
        #[cfg(feature = "xsd11")]
        crate::compiler::validate_all_default_open_content(schema_set)?;
        let (dep_graph, _dep_stats) = build_dependency_graph(schema_set)?;
        validate_all_derivations(schema_set, &dep_graph)?;
        validate_attribute_id_constraints(schema_set)?;
        validate_attribute_value_constraints(schema_set)?;
        validate_element_value_constraints(schema_set)?;
        #[cfg(feature = "xsd11")]
        xsd11_pre_resolution_validations(schema_set)?;
        crate::schema::validate_xsd10_annotation_source_anyuri(schema_set)?;
        crate::schema::validate_complex_type_attribute_uniqueness(schema_set)?;
        crate::schema::validate_local_decl_target_namespace(schema_set)?;
        crate::schema::validate_no_xsi_attribute_declarations(schema_set)?;
        crate::compiler::substitution::validate_all_substitution_groups(schema_set)?;
    }
    Ok(())
}

/// Load and process a schema with full processing (convenience function)
///
/// This is a simplified version of `load_and_process_schema` that uses
/// default configuration for full processing.
pub fn load_schema(
    xml: &[u8],
    base_uri: &str,
    schema_set: &mut SchemaSet,
) -> SchemaResult<PipelineStats> {
    load_and_process_schema(xml, base_uri, schema_set, Some(PipelineConfig::full()))
}

/// Parse a schema without processing directives or resolving references
///
/// This is useful when you want to manually control the processing phases
/// or when parsing multiple schemas before batch processing.
pub fn parse_schema_only(
    xml: &[u8],
    base_uri: &str,
    schema_set: &mut SchemaSet,
) -> SchemaResult<DocumentId> {
    let config = PipelineConfig::parse_only();
    let stats = load_and_process_schema(xml, base_uri, schema_set, Some(config))?;
    Ok(stats.doc_id)
}

/// Process inline types and references for schemas already loaded
///
/// Call this after manually loading multiple schemas to perform
/// the redefine/override application, inline assembly, and reference resolution phases.
///
/// **Precondition**: All participating schemas — including redefine/override targets —
/// must have been parsed and loaded into the schema set before calling this function.
pub fn process_loaded_schemas(
    schema_set: &mut SchemaSet,
) -> SchemaResult<(InlineAssemblyStats, ResolutionStats)> {
    // Fail early if parsing collected structural errors (error-recovery mode)
    if !schema_set.parsing_errors.is_empty() {
        let errors = std::mem::take(&mut schema_set.parsing_errors);
        return Err(errors.into_iter().next().unwrap());
    }

    schema_set.refresh_directive_failures();
    process_loaded_schemas_inner(schema_set).map_err(|e| note_unread_documents(e, schema_set))
}

/// A reference that fails to resolve may be due to a schema document that
/// could not be read: name those documents in the error.
fn note_unread_documents(
    error: crate::error::SchemaError,
    schema_set: &SchemaSet,
) -> crate::error::SchemaError {
    match error {
        crate::error::SchemaError::StructuralError {
            constraint,
            message,
            location,
        } if constraint.starts_with("src-resolve") => match unread_documents_note(schema_set) {
            Some(note) => crate::error::SchemaError::StructuralError {
                constraint,
                message: format!("{message}{note}"),
                location,
            },
            None => crate::error::SchemaError::StructuralError {
                constraint,
                message,
                location,
            },
        },
        other => other,
    }
}

/// `"; schema documents that could not be read: 'a.xsd' (main.xsd:2:3), …"`
/// for a reference that fails to resolve, when directives failed.
pub(crate) fn unread_documents_note(schema_set: &SchemaSet) -> Option<String> {
    let failures = schema_set.directive_failures();
    if failures.is_empty() {
        return None;
    }
    let mut unread: Vec<String> = failures
        .iter()
        .take(3)
        .map(|f| {
            let name = f.resolved_location().or(f.schema_location()).unwrap_or("?");
            match f.location() {
                Some(at) => format!("'{name}' ({at})"),
                None => format!("'{name}'"),
            }
        })
        .collect();
    if failures.len() > 3 {
        unread.push(format!("and {} more", failures.len() - 3));
    }
    Some(format!(
        "; schema documents that could not be read: {}",
        unread.join(", ")
    ))
}

fn process_loaded_schemas_inner(
    schema_set: &mut SchemaSet,
) -> SchemaResult<(InlineAssemblyStats, ResolutionStats)> {
    // Apply redefine/override directives before assembly
    crate::schema::apply_redefine_override(schema_set)?;

    let inline_stats = assemble_inline_types(schema_set)?;
    // Bound the nesting of components before any pass recurses along it
    // (reference resolution and everything after it).
    crate::schema::nesting::check_nesting_depth(schema_set)?;
    let resolution_stats = resolve_all_references(schema_set)?;

    // Compile all deferred pattern facets
    compile_all_patterns(schema_set)?;

    // XSD 1.1: Validate default open content declarations
    #[cfg(feature = "xsd11")]
    crate::compiler::validate_all_default_open_content(schema_set)?;

    // Validate type derivation constraints. The dependency graph's toposort is
    // also the only circular-derivation detector.
    let (dep_graph, _dep_stats) = build_dependency_graph(schema_set)?;
    validate_all_derivations(schema_set, &dep_graph)?;

    // Validate cos-attribute-decl (XSD 1.0: ID attrs must not have default/fixed)
    validate_attribute_id_constraints(schema_set)?;

    // a-props-correct.3 — validate attribute default/fixed values are type-valid
    validate_attribute_value_constraints(schema_set)?;

    // e-props-correct.2 / e-props-correct.4 — validate element default/fixed values
    validate_element_value_constraints(schema_set)?;

    #[cfg(feature = "xsd11")]
    xsd11_pre_resolution_validations(schema_set)?;

    // (XSD 1.0): strict xs:anyURI lexical check on annotation source
    // attributes. XSD 1.1 explicitly relaxed the rule, so no-op there.
    crate::schema::validate_xsd10_annotation_source_anyuri(schema_set)?;

    // ct-props-correct.4 / ag-props-correct.2 — every complex type's
    // effective attribute uses must be unique by (namespace, name).
    crate::schema::validate_complex_type_attribute_uniqueness(schema_set)?;

    // src-element §3.3.3 clause 4.3 / src-attribute §3.2.3 clause 6.3:
    // a local element/attribute's explicit `targetNamespace` may differ from
    // the schema's only inside a <complexContent>/<restriction> of a non-
    // anyType base.
    crate::schema::validate_local_decl_target_namespace(schema_set)?;

    // §3.2.6.4 (`no-xsi`): user-declared attributes must not live in the
    // XML Schema instance namespace.
    crate::schema::validate_no_xsi_attribute_declarations(schema_set)?;

    // Validate substitution group membership constraints (e-props-correct.4)
    crate::compiler::substitution::validate_all_substitution_groups(schema_set)?;

    allocate_content_particle_elements(schema_set)?;
    allocate_model_group_particle_elements(schema_set)?;

    // §3.8.6.3 (cos-element-consistent): when a content model contains a
    // local element with QName Q and an element ref whose substitution-
    // group expansion includes another declaration of Q, both must agree
    // on `{type definition}`. Runs after particle-element allocation so
    // local elements are tracked through their ElementKey.
    crate::schema::validate_substitution_group_element_consistency(schema_set)?;
    finalize_local_element_pass(schema_set)?;
    Ok((inline_stats, resolution_stats))
}

/// Validation passes that depend on local element declarations having been
/// allocated by `allocate_content_particle_elements`. Both pipeline entry
/// points run these in identical order to finalize IC `@ref` resolution,
/// re-validate default/fixed values, and (under xsd11) re-run CTA analysis
/// against newly-visible local-element alternatives.
fn finalize_local_element_pass(schema_set: &mut SchemaSet) -> SchemaResult<()> {
    // XSD 1.1: assemble inline alternative types attached to local elements
    // (which only acquired their ElementKey in the allocation pass).
    #[cfg(feature = "xsd11")]
    {
        let new_alt_types = crate::schema::inline::resolve_local_element_alternatives(schema_set)?;
        if !new_alt_types.is_empty() {
            resolve_all_references(schema_set)?;
            allocate_content_particle_elements(schema_set)?;
        }
    }
    // IC `@ref`s on top-level elements whose target IC lives on a local
    // element couldn't resolve until now (the local IC was registered by the
    // allocation pass).
    crate::schema::finalize_pending_ic_refs(schema_set)?;
    // Every identity constraint is in the arena now (top-level and local
    // elements, every document): a keyref's `refer` may name any of them.
    crate::schema::inline::validate_all_keyref_refers(schema_set)?;
    // e-props-correct.2 / .4 against local element default/fixed values
    // (saxon `s3_3_4si07/08`).
    validate_element_value_constraints(schema_set)?;
    #[cfg(feature = "xsd11")]
    {
        crate::schema::validate_cta_xpath(schema_set)?;
        // Every complex and simple type exists now, the inline types of local
        // elements' alternatives included.
        crate::schema::validate_assertion_xpath(schema_set)?;
        crate::schema::validate_cta_substitutability(schema_set)?;
    }
    #[cfg(feature = "xsd11")]
    xsd11_element_consistency_checks(schema_set)?;
    validate_all_group_outer_occurs(schema_set)?;
    validate_all_group_content(schema_set)?;
    validate_all_group_placement(schema_set)?;
    validate_all_particle_occurs(schema_set)?;
    validate_all_upa_constraints(schema_set)?;
    Ok(())
}

/// XSD 1.1 schema-validity checks that run after type derivation but before
/// inline alternative-type resolution and content-particle allocation.
///
/// Covers the three §3.12.x / §3.10.6.1 passes that touch alternative
/// declarations and wildcard `notQName` lists. No-op when xsd11 is disabled
/// (every callee gates internally on `schema_set.is_xsd11()`).
#[cfg(feature = "xsd11")]
fn xsd11_pre_resolution_validations(schema_set: &SchemaSet) -> SchemaResult<()> {
    // src-type-alternative: only the last <xs:alternative> may omit @test.
    crate::schema::validate_element_type_alternatives(schema_set)?;
    // §3.12.6 / §3.13.6.2: XPath Valid with only the built-in components in
    // the static context (undefined variables, unbound prefixes, user-defined
    // types and declarations, unknown constructors).
    crate::schema::validate_cta_xpath(schema_set)?;
    // §3.12.6 cos-ct-alternative-substitutable.
    crate::schema::validate_cta_substitutability(schema_set)?;
    // §3.10.6.1 rule 4: notQName entries must lie within the wildcard's
    // namespace constraint.
    crate::schema::validate_wildcard_disallowed_names(schema_set)?;
    Ok(())
}

/// XSD 1.1 cos-element-consistent passes that run after content-particle
/// allocation has populated each complex type's
/// `resolved_content_particle_elements` (which carries the post-resolution
/// alternative type-table the parser-frame copy lacks).
#[cfg(feature = "xsd11")]
fn xsd11_element_consistency_checks(schema_set: &SchemaSet) -> SchemaResult<()> {
    // §3.8.6.3: same-named local element declarations within one content
    // model must have equivalent type tables.
    crate::schema::validate_local_element_type_table_consistency(schema_set)?;
    // §3.8.6.3 (extended): local element + lax/strict wildcard + global
    // element with same QName ⇒ type tables must agree.
    crate::schema::validate_wildcard_element_type_table_consistency(schema_set)?;
    // §3.4.6.3: when a restriction-derived complex type re-issues a base
    // local element, the type tables must remain equivalent.
    crate::schema::validate_restriction_local_element_type_table_consistency(schema_set)?;
    Ok(())
}

/// Validate outer occurrence constraints on top-level all-groups.
///
/// XSD 1.0 (cos-all-limited.2): a particle whose term is an all-group must
/// have minOccurs in {0, 1} and maxOccurs = 1. This check runs on every
/// complex type independently of UPA validation.
fn validate_all_group_outer_occurs(schema_set: &SchemaSet) -> SchemaResult<()> {
    use crate::compiler::{
        is_top_level_all_group, resolve_top_level_all_group_ref, validate_outer_all_group_occurs,
    };

    for (_, type_def) in schema_set.arenas.complex_types.iter() {
        let Some(particle) = (match &type_def.content {
            crate::parser::frames::ComplexContentResult::Complex(content) => {
                content.particle.as_ref()
            }
            crate::parser::frames::ComplexContentResult::Empty
            | crate::parser::frames::ComplexContentResult::Simple(_) => None,
        }) else {
            continue;
        };

        let is_all = is_top_level_all_group(particle).is_some()
            || resolve_top_level_all_group_ref(particle, schema_set).is_some();
        if !is_all {
            continue;
        }

        validate_outer_all_group_occurs(particle, schema_set.xsd_version).map_err(|error| {
            let location = error
                .location()
                .and_then(|source| schema_set.source_maps.locate(source));
            crate::error::SchemaError::structural("cos-all-limited", format!("{}", error), location)
        })?;
    }

    Ok(())
}

/// Validate all-group content constraints.
///
/// XSD 1.0 §3.8 cos-all-limited: every particle inside an `xs:all` must be
/// an element declaration whose `minOccurs` is 0 or 1 and whose `maxOccurs`
/// is 1 — wildcards (`xs:any`) and nested groups are forbidden, and a member
/// element with `maxOccurs > 1` is illegal even when its sibling has the
/// default `maxOccurs="1"`. XSD 1.1 relaxes these constraints (any
/// `maxOccurs`, wildcards, group refs are allowed subject to cos-all-limited
/// 1.3 / rule 2 enforced at compile time).
fn validate_all_group_content(schema_set: &SchemaSet) -> SchemaResult<()> {
    use crate::parser::frames::{ComplexContentResult, Compositor};

    if !schema_set.is_xsd10() {
        return Ok(());
    }

    // Check named model groups
    for (_, mg) in schema_set.arenas.model_groups.iter() {
        if mg.compositor == Some(Compositor::All) {
            check_all_group_xsd10_constraints(&mg.particles, schema_set)?;
        }
    }

    // Check content particles in complex types
    for (_, type_def) in schema_set.arenas.complex_types.iter() {
        if let ComplexContentResult::Complex(content) = &type_def.content {
            if let Some(particle) = content.particle.as_ref() {
                check_particle_all_group_constraints(particle, schema_set)?;
            }
        }
    }

    Ok(())
}

fn check_all_group_xsd10_constraints(
    particles: &[crate::parser::frames::ParticleResult],
    schema_set: &SchemaSet,
) -> SchemaResult<()> {
    use crate::parser::frames::ParticleTerm;

    for particle in particles {
        let particle_loc = schema_set.locate(particle.source.as_ref());
        match &particle.term {
            ParticleTerm::Any(wc) => {
                let location = schema_set
                    .locate(wc.source.as_ref())
                    .or_else(|| particle_loc.clone());
                return Err(crate::error::SchemaError::structural(
                    "cos-all-limited",
                    "In XSD 1.0, xs:any (wildcard) is not allowed inside an xs:all group"
                        .to_string(),
                    location,
                ));
            }
            ParticleTerm::Group(mg) => {
                let location = schema_set
                    .locate(mg.source.as_ref())
                    .or_else(|| particle_loc.clone());
                return Err(crate::error::SchemaError::structural(
                    "cos-all-limited",
                    "In XSD 1.0, a nested model group is not allowed inside an xs:all group"
                        .to_string(),
                    location,
                ));
            }
            ParticleTerm::Element(_) => {}
        }

        if particle.min_occurs > 1 {
            return Err(crate::error::SchemaError::structural(
                "cos-all-limited",
                format!(
                    "In XSD 1.0, an xs:all member's minOccurs must be 0 or 1, found {}",
                    particle.min_occurs
                ),
                particle_loc,
            ));
        }

        match particle.max_occurs {
            Some(0) | Some(1) => {}
            Some(n) => {
                return Err(crate::error::SchemaError::structural(
                    "cos-all-limited",
                    format!(
                        "In XSD 1.0, an xs:all member's maxOccurs must be 0 or 1, found {}",
                        n
                    ),
                    particle_loc,
                ));
            }
            None => {
                return Err(crate::error::SchemaError::structural(
                    "cos-all-limited",
                    "In XSD 1.0, an xs:all member's maxOccurs must be 0 or 1 (unbounded not allowed)"
                        .to_string(),
                    particle_loc,
                ));
            }
        }
    }
    Ok(())
}

fn check_particle_all_group_constraints(
    particle: &crate::parser::frames::ParticleResult,
    schema_set: &SchemaSet,
) -> SchemaResult<()> {
    use crate::parser::frames::{Compositor, ParticleTerm};

    if let ParticleTerm::Group(mg) = &particle.term {
        if mg.compositor == Some(Compositor::All) {
            check_all_group_xsd10_constraints(&mg.particles, schema_set)?;
        }
        // Recurse into child particles regardless of compositor
        for child in &mg.particles {
            check_particle_all_group_constraints(child, schema_set)?;
        }
    }
    Ok(())
}

/// Validate cos-all-limited placement: an `xs:all` model group (whether
/// inline or reached through a named-group reference) may only appear as the
/// top-level particle of a complex type's content. It must not appear nested
/// inside `xs:sequence` / `xs:choice`, nor reached via a chain of group
/// references that pass through a non-all compositor.
///
/// XSD 1.0 §3.8.6 clauses 1–2; XSD 1.1 §3.8.6.2 All Group Limited clause 1,
/// which also admits an all group inside another all group (clause 1.3, a
/// group reference with minOccurs = maxOccurs = 1) — checked when the all
/// group is compiled, as the top-level particle below is not walked.
fn validate_all_group_placement(schema_set: &SchemaSet) -> SchemaResult<()> {
    use crate::compiler::{is_top_level_all_group, resolve_top_level_all_group_ref};
    use crate::parser::frames::ComplexContentResult;

    for (_, type_def) in schema_set.arenas.complex_types.iter() {
        let ComplexContentResult::Complex(content) = &type_def.content else {
            continue;
        };
        let Some(particle) = content.particle.as_ref() else {
            continue;
        };

        // Per-particle constraints on a top-level all-group are enforced
        // by validate_all_group_content; only deeper placements need the
        // recursion guard below.
        if is_top_level_all_group(particle).is_some()
            || resolve_top_level_all_group_ref(particle, schema_set).is_some()
        {
            continue;
        }

        let mut visited = std::collections::HashSet::new();
        check_no_nested_all_group(particle, schema_set, &mut visited)?;
    }
    Ok(())
}

fn check_no_nested_all_group(
    particle: &crate::parser::frames::ParticleResult,
    schema_set: &SchemaSet,
    visited: &mut std::collections::HashSet<crate::ids::ModelGroupKey>,
) -> SchemaResult<()> {
    use crate::parser::frames::{Compositor, ModelGroupDefResult, ParticleResult, ParticleTerm};

    fn placement_location(
        particle: &ParticleResult,
        group: &ModelGroupDefResult,
        schema_set: &SchemaSet,
    ) -> Option<crate::parser::location::SourceLocation> {
        schema_set
            .locate(particle.source.as_ref())
            .or_else(|| schema_set.locate(group.source.as_ref()))
    }

    // A particle with minOccurs = maxOccurs = 0 is no component (1.1 §3.7.2:
    // "Otherwise, the <group> has minOccurs=maxOccurs=0, in which case it maps
    // to no component at all"; §3.8.2 likewise for <all>, <choice> and
    // <sequence>; XSD 1.0 §3.7.2 / §3.8.2 the same): it places no all group.
    if particle.min_occurs == 0 && particle.max_occurs == Some(0) {
        return Ok(());
    }
    let ParticleTerm::Group(group) = &particle.term else {
        return Ok(());
    };

    // XSD 1.1 §3.8.6.2 clause 1: "It appears only as the value of one or more
    // of the following properties: 1.1 the {model group} property of a model
    // group definition. 1.2 the {term} property of a Particle with {max
    // occurs} = 1 which is the {particle} of the {content type} of a complex
    // type definition. 1.3 the {term} property of a Particle P with {min
    // occurs} = {max occurs} = 1, where P is among the {particles} of a Model
    // Group whose {compositor} is all."
    let misplaced = |reference: bool| {
        let message = match (schema_set.is_xsd10(), reference) {
            (true, false) => {
                "In XSD 1.0, an xs:all model group may only appear as the top-level \
                 particle of a complex type definition"
            }
            (true, true) => {
                "In XSD 1.0, a reference to a named xs:all group may only \
                 appear as the top-level particle of a complex type definition"
            }
            (false, false) => {
                "An xs:all model group may only appear as the top-level particle of a \
                 complex type definition or inside another xs:all group (All Group \
                 Limited, clause 1)"
            }
            (false, true) => {
                "A reference to a named xs:all group may only appear as the top-level \
                 particle of a complex type definition or inside another xs:all group \
                 (All Group Limited, clause 1)"
            }
        };
        crate::error::SchemaError::structural(
            "cos-all-limited",
            message.to_string(),
            placement_location(particle, group, schema_set),
        )
    };

    if group.compositor == Some(Compositor::All) && group.ref_name.is_none() {
        return Err(misplaced(false));
    }

    if let Some(ref_name) = group.ref_name.as_ref() {
        if let Some(group_key) =
            schema_set.lookup_model_group(ref_name.namespace, ref_name.local_name)
        {
            if let Some(group_data) = schema_set.arenas.get_model_group(group_key) {
                if group_data.compositor == Some(Compositor::All) {
                    return Err(misplaced(true));
                }
                if visited.insert(group_key) {
                    for child in &group_data.particles {
                        check_no_nested_all_group(child, schema_set, visited)?;
                    }
                    visited.remove(&group_key);
                }
            }
        }
    } else {
        for child in &group.particles {
            check_no_nested_all_group(child, schema_set, visited)?;
        }
    }
    Ok(())
}

/// Validate occurrence constraints (p-props-correct clause 2.1):
/// minOccurs must not exceed maxOccurs for all particles.
fn validate_all_particle_occurs(schema_set: &SchemaSet) -> SchemaResult<()> {
    for (_, type_def) in schema_set.arenas.complex_types.iter() {
        if let crate::parser::frames::ComplexContentResult::Complex(content) = &type_def.content {
            if let Some(particle) = content.particle.as_ref() {
                validate_particle_occurs_recursive(particle, schema_set)?;
            }
        }
    }
    for (_, mg) in schema_set.arenas.model_groups.iter() {
        for particle in &mg.particles {
            validate_particle_occurs_recursive(particle, schema_set)?;
        }
    }
    Ok(())
}

fn validate_particle_occurs_recursive(
    particle: &crate::parser::frames::ParticleResult,
    schema_set: &SchemaSet,
) -> SchemaResult<()> {
    if let Some(max) = particle.max_occurs {
        if particle.min_occurs > max {
            let location = particle
                .source
                .as_ref()
                .and_then(|s| schema_set.source_maps.locate(s));
            return Err(crate::error::SchemaError::structural(
                "p-props-correct",
                format!(
                    "minOccurs ({}) exceeds maxOccurs ({})",
                    particle.min_occurs, max
                ),
                location,
            ));
        }
    }
    if let crate::parser::frames::ParticleTerm::Group(ref mg) = particle.term {
        for child in &mg.particles {
            validate_particle_occurs_recursive(child, schema_set)?;
        }
    }
    Ok(())
}

fn validate_all_upa_constraints(schema_set: &SchemaSet) -> SchemaResult<()> {
    // The substitution-group map depends on the schema set only, which this
    // pass does not change: build it once, on first use, rather than once per
    // complex type (a walk of every element declaration each time).
    let mut substitution_sets: Option<crate::compiler::SubstitutionGroupMap> = None;

    for (_, type_def) in schema_set.arenas.complex_types.iter() {
        // Skip built-in types (xs:anyType etc.) — they have no source location
        // and are valid by construction.
        if type_def.source.is_none() {
            continue;
        }

        let Some(_particle) = (match &type_def.content {
            crate::parser::frames::ComplexContentResult::Complex(content) => {
                content.particle.as_ref()
            }
            crate::parser::frames::ComplexContentResult::Empty
            | crate::parser::frames::ComplexContentResult::Simple(_) => None,
        }) else {
            continue;
        };

        // Compile with capped occurrence bounds for UPA checking.
        // All maxOccurs values are reduced to <=2, producing a counter-free NFA.
        let matcher = crate::compiler::compile_content_model_for_upa(schema_set, type_def)
            .map_err(|error| {
                let location = error
                    .location()
                    .and_then(|source| schema_set.source_maps.locate(source));
                if let crate::compiler::NfaCompileError::RecursionLimitExceeded { .. } = error {
                    // The compiler's own recursion bound: reached only with
                    // `SchemaSet::max_depth` = 0 (no nesting limit) or past it.
                    let bound = schema_set.recursion_bound(100);
                    return crate::error::SchemaError::limit(
                        crate::error::SchemaLimit::ComponentDepth,
                        bound,
                        bound + 1,
                        format!(
                            "the content model nests more than {bound} group references \
                             (the compiler's own recursion bound; SchemaSet::max_depth is {})",
                            schema_set.max_depth()
                        ),
                        location,
                    );
                }
                crate::error::SchemaError::structural(
                    "cos-nonambig",
                    format!(
                        "Failed to compile content model for UPA checking: {}",
                        error
                    ),
                    location,
                )
            })?;

        let substitution_sets = substitution_sets
            .get_or_insert_with(|| crate::compiler::upa_substitution_map(schema_set));
        match matcher {
            crate::compiler::ContentModelMatcher::Nfa(nfa)
            | crate::compiler::ContentModelMatcher::WithOpenContent { nfa, .. } => {
                crate::compiler::check_upa_with_substitutions(
                    &nfa,
                    schema_set,
                    type_def.target_namespace,
                    substitution_sets,
                )?;
            }
            crate::compiler::ContentModelMatcher::AllGroup(model) => {
                crate::compiler::check_all_group_upa_with_substitutions(
                    &model,
                    schema_set,
                    type_def.target_namespace,
                    substitution_sets,
                )?;
            }
        }
    }

    Ok(())
}

// ============================================================================
// Async Pipeline Functions (feature = "async")
// ============================================================================

/// Load and fully process an XSD schema document asynchronously.
///
/// Async variant of [`load_and_process_schema`], with the same rules and
/// outcome. Only the directive resolution phase (I/O) is async; all
/// computation phases (parse, assembly, resolution) remain synchronous.
///
/// # Arguments
///
/// * `xml` - Raw XML bytes of the schema document
/// * `base_uri` - Base URI for this document
/// * `schema_set` - Schema set to add the parsed document to
/// * `config` - Optional pipeline configuration (uses defaults if None)
#[cfg(feature = "async")]
pub async fn load_and_process_schema_async(
    xml: &[u8],
    base_uri: &str,
    schema_set: &mut SchemaSet,
    config: Option<PipelineConfig>,
) -> SchemaResult<PipelineStats> {
    let config = config.unwrap_or_default();
    let mut stats = PipelineStats::default();

    // Phase 1: Parse the primary schema document (sync — CPU-bound)
    let doc_id = parse_schema_with_config(xml, base_uri, schema_set, &config.parser)?;
    stats.doc_id = doc_id;

    // Phase 2: Resolve directives asynchronously, with the same rules
    if config.resolve_directives {
        let mut resolver = pipeline_resolver(&config);
        mark_primary_loaded(&resolver, base_uri, doc_id, schema_set);
        let closure = resolve_directive_closure_async(&[doc_id], &mut resolver, schema_set).await;
        finish_directives(schema_set, closure, &mut stats)?;
    }

    // The same phases as the synchronous pipeline (sync — CPU-bound)
    run_phases(schema_set, &config, &mut stats)?;
    Ok(stats)
}

/// Load and process a schema asynchronously with full processing (convenience function).
///
/// Async variant of [`load_schema`].
#[cfg(feature = "async")]
pub async fn load_schema_async(
    xml: &[u8],
    base_uri: &str,
    schema_set: &mut SchemaSet,
) -> SchemaResult<PipelineStats> {
    load_and_process_schema_async(xml, base_uri, schema_set, Some(PipelineConfig::full())).await
}

#[cfg(test)]
#[path = "pipeline_tests.rs"]
mod tests;
