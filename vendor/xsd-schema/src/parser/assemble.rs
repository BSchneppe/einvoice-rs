//! Assemble schema components from parser frame results.
//!
//! Converts `SchemaFrameResult` into `SchemaDocument` and registers top-level
//! components in the schema set arenas and namespace tables.

use std::collections::{HashMap, HashSet};

use crate::arenas::{
    AttributeDeclData, AttributeGroupData, ComplexTypeDefData, ElementDeclData,
    IdentityConstraintData, ModelGroupData, NotationData, SimpleTypeDefData,
};
use crate::error::{SchemaError, SchemaResult};
use crate::ids::{
    AttributeGroupKey, AttributeKey, DocumentId, ElementKey, IdentityConstraintKey, ModelGroupKey,
    NameId, NotationKey, TypeKey,
};
use crate::namespace::QualifiedName;
use crate::parser::frames::{
    AttributeFrameResult, AttributeGroupDefResult, ComplexContentResult, ComplexTypeResult,
    DirectiveResult, FrameResult, GroupFrameResult, ModelGroupDefResult, NotationResult,
    OverrideResult, RedefineComponent, SchemaFrameResult, SimpleTypeResult, TypeFrameResult,
};
use crate::parser::location::SourceRef;
use crate::schema::composition::{
    ComponentIdentity, ComponentKey, ComponentKind, DocumentComponentIndex,
};
use crate::schema::model::{
    FormChoice, ImportDirective, IncludeDirective, NamespaceTable, OverrideComponent,
    OverrideDirective, RedefineDirective, SchemaDocument,
};
use crate::schema::wildcard::{
    ElementWildcard, NamespaceConstraint, ProcessContents as SchemaProcessContents,
};
use crate::validation::asttree::Asttree;
use crate::SchemaSet;

/// Result type for convert_directives function
type DirectivesResult = (
    Vec<IncludeDirective>,
    Vec<ImportDirective>,
    Vec<RedefineDirective>,
    Vec<OverrideDirective>,
);

pub struct SchemaAssembler<'a> {
    schema_set: &'a mut SchemaSet,
    target_namespace: Option<NameId>,
    block_default: crate::schema::model::DerivationSet,
    final_default: crate::schema::model::DerivationSet,
    /// Identity constraint names seen in this document (for uniqueness checking)
    /// XSD constraint: Identity Constraint Name Uniqueness - names must be unique per schema document
    identity_constraint_names: HashSet<NameId>,
    /// Per-document component index being built during assembly
    doc_components: DocumentComponentIndex,
    /// Side effects on `schema_set` made by this assembly, in order; undone in
    /// reverse by [`SchemaAssembler::rollback`] when the document fails to
    /// assemble, so a rejected document leaves no component behind.
    undo_log: Vec<AssemblyEffect>,
}

/// One reversible side effect of [`SchemaAssembler::assemble`] on the schema set.
#[derive(Debug, Clone, Copy)]
enum AssemblyEffect {
    /// A component was allocated in the arenas.
    Allocated(ComponentKey),
    /// `name` was bound in the namespace table of `namespace`; `previous` is
    /// the binding it replaced (of the same kind), if any.
    Bound {
        namespace: Option<NameId>,
        name: NameId,
        previous: PreviousBinding,
    },
    /// The namespace table of this namespace was created.
    NamespaceCreated(Option<NameId>),
}

/// The binding a namespace-table registration replaced, by symbol space.
#[derive(Debug, Clone, Copy)]
enum PreviousBinding {
    Type(Option<TypeKey>),
    Element(Option<ElementKey>),
    Attribute(Option<AttributeKey>),
    ModelGroup(Option<ModelGroupKey>),
    AttributeGroup(Option<AttributeGroupKey>),
    Notation(Option<NotationKey>),
    IdentityConstraint(Option<IdentityConstraintKey>),
}

impl PreviousBinding {
    /// Put the replaced binding back (or remove `name` when there was none).
    fn restore(self, table: &mut NamespaceTable, name: NameId) {
        fn put<K>(map: &mut HashMap<NameId, K>, name: NameId, previous: Option<K>) {
            match previous {
                Some(key) => {
                    map.insert(name, key);
                }
                None => {
                    map.remove(&name);
                }
            }
        }
        match self {
            PreviousBinding::Type(p) => put(&mut table.types, name, p),
            PreviousBinding::Element(p) => put(&mut table.elements, name, p),
            PreviousBinding::Attribute(p) => put(&mut table.attributes, name, p),
            PreviousBinding::ModelGroup(p) => put(&mut table.model_groups, name, p),
            PreviousBinding::AttributeGroup(p) => put(&mut table.attribute_groups, name, p),
            PreviousBinding::Notation(p) => put(&mut table.notations, name, p),
            PreviousBinding::IdentityConstraint(p) => put(&mut table.identity_constraints, name, p),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GroupKeyResult {
    Model(ModelGroupKey),
    Attribute(AttributeGroupKey),
}

impl<'a> SchemaAssembler<'a> {
    pub fn new(
        schema_set: &'a mut SchemaSet,
        target_namespace: Option<NameId>,
        block_default: crate::schema::model::DerivationSet,
        final_default: crate::schema::model::DerivationSet,
    ) -> Self {
        Self {
            schema_set,
            target_namespace,
            block_default,
            final_default,
            identity_constraint_names: HashSet::new(),
            doc_components: DocumentComponentIndex::new(),
            undo_log: Vec::new(),
        }
    }

    /// Assemble one schema document: register its top-level components in
    /// the schema set and return the [`SchemaDocument`].
    ///
    /// On error the schema set is left as it was before the call: every
    /// component this document allocated is removed again and every
    /// namespace-table binding it made (including one that replaced an
    /// existing binding of the same name) is restored.
    pub fn assemble(
        &mut self,
        schema: SchemaFrameResult,
        doc_id: DocumentId,
        base_uri: &str,
    ) -> SchemaResult<SchemaDocument> {
        let result = self.assemble_document(schema, doc_id, base_uri);
        if result.is_err() {
            self.rollback();
        }
        self.undo_log.clear();
        result
    }

    /// Undo, in reverse order, every side effect recorded in `undo_log`.
    fn rollback(&mut self) {
        let schema_set = &mut *self.schema_set;
        for effect in self.undo_log.drain(..).rev() {
            match effect {
                AssemblyEffect::Allocated(key) => {
                    // `entries_mut` is the only `&mut` access to the arenas
                    // (it also clears the effective-facet cache, which is
                    // harmless on this error path).
                    let arenas = schema_set.arenas.entries_mut();
                    match key {
                        ComponentKey::Type(TypeKey::Simple(k)) => {
                            arenas.simple_types.remove(k);
                        }
                        ComponentKey::Type(TypeKey::Complex(k)) => {
                            arenas.complex_types.remove(k);
                        }
                        ComponentKey::Element(k) => {
                            arenas.elements.remove(k);
                        }
                        ComponentKey::Attribute(k) => {
                            arenas.attributes.remove(k);
                        }
                        ComponentKey::ModelGroup(k) => {
                            arenas.model_groups.remove(k);
                        }
                        ComponentKey::AttributeGroup(k) => {
                            arenas.attribute_groups.remove(k);
                        }
                        ComponentKey::Notation(k) => {
                            arenas.notations.remove(k);
                        }
                        ComponentKey::IdentityConstraint(k) => {
                            arenas.identity_constraints.remove(k);
                        }
                    }
                }
                AssemblyEffect::Bound {
                    namespace,
                    name,
                    previous,
                } => {
                    if let Some(table) = schema_set.namespaces.get_mut(&namespace) {
                        previous.restore(table, name);
                    }
                }
                AssemblyEffect::NamespaceCreated(namespace) => {
                    schema_set.namespaces.remove(&namespace);
                }
            }
        }
    }

    /// Record an arena allocation made by this assembly.
    fn allocated(&mut self, key: ComponentKey) {
        self.undo_log.push(AssemblyEffect::Allocated(key));
    }

    /// The namespace table of `namespace`, created (and the creation
    /// recorded) when it does not exist yet.
    fn namespace_table(&mut self, namespace: Option<NameId>) -> &mut NamespaceTable {
        if !self.schema_set.namespaces.contains_key(&namespace) {
            self.undo_log
                .push(AssemblyEffect::NamespaceCreated(namespace));
        }
        self.schema_set.get_or_create_namespace(namespace)
    }

    /// Record a namespace-table binding made by this assembly.
    fn bound(&mut self, namespace: Option<NameId>, name: NameId, previous: PreviousBinding) {
        self.undo_log.push(AssemblyEffect::Bound {
            namespace,
            name,
            previous,
        });
    }

    fn assemble_document(
        &mut self,
        schema: SchemaFrameResult,
        doc_id: DocumentId,
        base_uri: &str,
    ) -> SchemaResult<SchemaDocument> {
        let mut doc =
            build_schema_document(&schema, doc_id, base_uri, &mut self.schema_set.name_table)?;

        let directives = schema.directives;
        let components = schema.components;
        let (includes, imports, redefines, overrides) = convert_directives(directives, self)?;
        doc.includes = includes;
        doc.imports = imports;
        doc.redefines = redefines;
        doc.overrides = overrides;

        for component in components {
            self.assemble_component(component)?;
        }

        doc.component_index = std::mem::take(&mut self.doc_components);
        Ok(doc)
    }

    fn assemble_component(&mut self, component: FrameResult) -> SchemaResult<()> {
        match component {
            FrameResult::Type(result) => {
                self.assemble_type(result, true)?;
            }
            FrameResult::Element(result) => {
                self.assemble_element(result, true)?;
            }
            FrameResult::Attribute(result) => {
                self.assemble_attribute(result, true)?;
            }
            FrameResult::Group(result) => {
                self.assemble_group(result, true)?;
            }
            FrameResult::Notation(result) => {
                self.assemble_notation(result, true)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn assemble_type(&mut self, result: TypeFrameResult, register: bool) -> SchemaResult<TypeKey> {
        match result {
            TypeFrameResult::Simple(simple) => {
                let SimpleTypeResult {
                    name,
                    variety,
                    base_type,
                    item_type,
                    member_types,
                    facets,
                    final_derivation,
                    id,
                    derivation_id,
                    annotation,
                    source,
                } = *simple;
                // final="" explicitly overrides finalDefault; None (absent) inherits it.
                let final_derivation = final_derivation.unwrap_or(self.final_default);
                let source_ref = source.clone();
                let name = name.ok_or_else(|| {
                    missing_name("simpleType", source_ref.as_ref(), self.schema_set)
                })?;
                let data = SimpleTypeDefData {
                    name: Some(name),
                    target_namespace: self.target_namespace,
                    variety,
                    base_type,
                    item_type,
                    member_types,
                    facets,
                    final_derivation,
                    id,
                    derivation_id,
                    annotation,
                    source,
                    // Resolved references (populated after reference resolution phase)
                    resolved_base_type: None,
                    resolved_item_type: None,
                    resolved_member_types: Vec::new(),
                    redefine_original: None,
                    deferred_item_type_error: None,
                };
                let key = self.schema_set.arenas.alloc_simple_type(data);
                let type_key = TypeKey::Simple(key);
                self.allocated(ComponentKey::Type(type_key));
                if register {
                    self.register_type(name, type_key, source_ref.as_ref())?;
                }
                Ok(type_key)
            }
            TypeFrameResult::Complex(complex) => {
                let ComplexTypeResult {
                    name,
                    base_type,
                    derivation_method,
                    content,
                    attributes,
                    attribute_groups,
                    attribute_wildcard,
                    mixed,
                    is_abstract,
                    final_derivation,
                    block,
                    default_attributes_apply,
                    id,
                    #[cfg(feature = "xsd11")]
                    xpath_default_namespace,
                    annotation,
                    source,
                } = *complex;
                // final="" explicitly overrides finalDefault; None (absent) inherits it.
                let final_derivation = final_derivation.unwrap_or(self.final_default);
                // block="" explicitly overrides blockDefault; None (absent) inherits it.
                let block = block.unwrap_or(self.block_default);
                let open_content = match &content {
                    ComplexContentResult::Complex(def) => def.open_content.clone(),
                    _ => None,
                };
                #[cfg(feature = "xsd11")]
                let assertions = match &content {
                    ComplexContentResult::Simple(sc) => sc.assertions.clone(),
                    ComplexContentResult::Complex(cc) => cc.assertions.clone(),
                    ComplexContentResult::Empty => Vec::new(),
                };
                let source_ref = source.clone();
                let name = name.ok_or_else(|| {
                    missing_name("complexType", source_ref.as_ref(), self.schema_set)
                })?;
                let data = ComplexTypeDefData {
                    name: Some(name),
                    target_namespace: self.target_namespace,
                    base_type,
                    derivation_method,
                    content,
                    open_content,
                    attributes,
                    attribute_groups,
                    attribute_wildcard,
                    mixed,
                    is_abstract,
                    final_derivation,
                    block,
                    default_attributes_apply,
                    id,
                    #[cfg(feature = "xsd11")]
                    assertions,
                    #[cfg(feature = "xsd11")]
                    xpath_default_namespace,
                    annotation,
                    source,
                    // Resolved references (populated after reference resolution phase)
                    resolved_base_type: None,
                    resolved_attribute_groups: Vec::new(),
                    resolved_attributes: Vec::new(),
                    resolved_content_particle_types: Vec::new(),
                    resolved_content_particle_elements: Vec::new(),
                    resolved_simple_content_type: None,
                    redefine_original: None,
                };
                let key = self.schema_set.arenas.alloc_complex_type(data);
                let type_key = TypeKey::Complex(key);
                self.allocated(ComponentKey::Type(type_key));
                if register {
                    self.register_type(name, type_key, source_ref.as_ref())?;
                }
                Ok(type_key)
            }
        }
    }

    fn assemble_element(
        &mut self,
        result: crate::parser::frames::ElementFrameResult,
        register: bool,
    ) -> SchemaResult<ElementKey> {
        let crate::parser::frames::ElementFrameResult {
            name,
            ref_name,
            target_namespace: local_namespace,
            type_ref,
            inline_type,
            substitution_group,
            default_value,
            fixed_value,
            nillable,
            is_abstract,
            min_occurs,
            max_occurs,
            block,
            final_derivation,
            form,
            id,
            alternatives,
            identity_constraints,
            identity_constraint_refs,
            annotation,
            source,
        } = result;
        let source_ref = source.clone();
        let name =
            name.ok_or_else(|| missing_name("element", source_ref.as_ref(), self.schema_set))?;
        let target_namespace = local_namespace.or(self.target_namespace);
        // final="" explicitly overrides finalDefault; None (absent) inherits it.
        // block="" explicitly overrides blockDefault; None (absent) inherits it.
        let (final_derivation, block) = if ref_name.is_none() {
            (
                final_derivation.unwrap_or(self.final_default),
                block.unwrap_or(self.block_default),
            )
        } else {
            (
                final_derivation.unwrap_or_default(),
                block.unwrap_or_default(),
            )
        };

        // Check identity constraint name uniqueness (per schema document)
        // XSD Constraint: Identity Constraint Name Uniqueness (§3.11.1)
        for ic in &identity_constraints {
            if !self.identity_constraint_names.insert(ic.name) {
                // Name already exists in this document - duplicate error
                let location = self.schema_set.locate(ic.source.as_ref());
                let name_str = self.schema_set.name_table.resolve(ic.name);
                return Err(SchemaError::structural(
                    "ic-unique",
                    format!(
                        "Duplicate identity constraint name '{}' in schema document",
                        name_str
                    ),
                    location,
                ));
            }
        }

        // Validate identity constraint XPath expressions at schema time (§3.11.4)
        for ic in &identity_constraints {
            let xsd_version = self.schema_set.xsd_version;
            // Validate selector XPath
            if let Err(e) = Asttree::compile_selector(
                &ic.selector.xpath,
                &ic.selector.ns_snapshot,
                &self.schema_set.name_table,
                ic.selector.xpath_default_namespace.as_deref(),
                None, // schema-level xpathDefaultNamespace resolved later
                self.target_namespace,
                xsd_version,
            ) {
                let ic_name = self.schema_set.name_table.resolve_ref(ic.name);
                let location = self.schema_set.locate(ic.source.as_ref());
                return Err(SchemaError::structural(
                    "src-identity-constraint",
                    format!(
                        "Identity constraint '{}': invalid selector XPath '{}': {}",
                        ic_name, ic.selector.xpath, e
                    ),
                    location,
                ));
            }
            // Validate field XPath expressions
            for field in &ic.fields {
                if let Err(e) = Asttree::compile_field(
                    &field.xpath,
                    &field.ns_snapshot,
                    &self.schema_set.name_table,
                    field.xpath_default_namespace.as_deref(),
                    None,
                    self.target_namespace,
                    xsd_version,
                ) {
                    let ic_name = self.schema_set.name_table.resolve_ref(ic.name);
                    let location = self.schema_set.locate(ic.source.as_ref());
                    return Err(SchemaError::structural(
                        "src-identity-constraint",
                        format!(
                            "Identity constraint '{}': invalid field XPath '{}': {}",
                            ic_name, field.xpath, e
                        ),
                        location,
                    ));
                }
            }
        }

        // A keyref's `refer` is checked once every identity constraint of the
        // schema is known (`validate_all_keyref_refers`, pipeline).

        // Allocate identity constraints into the arena and register in namespace table
        let mut identity_constraint_keys: Vec<IdentityConstraintKey> =
            Vec::with_capacity(identity_constraints.len());
        for ic in identity_constraints {
            let ic_name = ic.name;
            let ic_source = ic.source.clone();
            let key = self
                .schema_set
                .arenas
                .alloc_identity_constraint(IdentityConstraintData {
                    kind: ic.kind,
                    name: ic.name,
                    ref_name: ic.ref_name,
                    refer: ic.refer,
                    selector: ic.selector,
                    fields: ic.fields,
                    id: ic.id,
                    annotation: ic.annotation,
                    source: ic.source,
                });
            self.allocated(ComponentKey::IdentityConstraint(key));
            // Register in namespace table for @ref resolution. Identity
            // constraints share one symbol space per namespace (§3.11.1),
            // so a name already registered by another schema document of
            // the same namespace is a collision, not an overwrite.
            let previous = self
                .namespace_table(target_namespace)
                .identity_constraints
                .insert(ic_name, key);
            self.bound(
                target_namespace,
                ic_name,
                PreviousBinding::IdentityConstraint(previous),
            );
            if previous.is_some() {
                let location = self.schema_set.locate(ic_source.as_ref());
                let name_str = self.schema_set.name_table.resolve(ic_name);
                return Err(SchemaError::structural(
                    "ic-unique",
                    format!(
                        "Duplicate identity constraint name '{}' in schema",
                        name_str
                    ),
                    location,
                ));
            }
            identity_constraint_keys.push(key);
        }

        // Defer XSD 1.1 @ref identity constraint references for resolution
        // in resolve_all_references() — the target IC may not exist yet
        // (forward references / included schemas).
        let pending_ic_refs: Vec<_> = identity_constraint_refs
            .into_iter()
            .map(|r| (r.kind, r.ref_name, r.source))
            .collect();

        let data = ElementDeclData {
            name: Some(name),
            target_namespace,
            ref_name,
            type_ref,
            inline_type,
            substitution_group,
            default_value,
            fixed_value,
            nillable,
            is_abstract,
            min_occurs,
            max_occurs,
            block,
            final_derivation,
            form,
            id,
            alternatives,
            identity_constraints: identity_constraint_keys,
            pending_ic_refs,
            annotation,
            source,
            // Resolved references (populated after reference resolution phase)
            resolved_type: None,
            resolved_ref: None,
            resolved_substitution_groups: Vec::new(),
            deferred_type_error: None,
        };
        let key = self.schema_set.arenas.alloc_element(data);
        self.allocated(ComponentKey::Element(key));
        if register {
            self.register_element(name, key, source_ref.as_ref())?;
        }
        Ok(key)
    }

    fn assemble_attribute(
        &mut self,
        result: AttributeFrameResult,
        register: bool,
    ) -> SchemaResult<AttributeKey> {
        let AttributeFrameResult {
            name,
            ref_name,
            target_namespace: local_namespace,
            type_ref,
            inline_type,
            default_value,
            fixed_value,
            use_kind,
            form,
            inheritable,
            id,
            annotation,
            source,
        } = result;
        let source_ref = source.clone();
        let name =
            name.ok_or_else(|| missing_name("attribute", source_ref.as_ref(), self.schema_set))?;
        let target_namespace = local_namespace.or(self.target_namespace);
        let data = AttributeDeclData {
            name: Some(name),
            target_namespace,
            ref_name,
            type_ref,
            inline_type,
            default_value,
            fixed_value,
            use_kind,
            form,
            // §3.2.2.1 / §3.2.2.2: "The ·actual value· of the inheritable
            // [attribute], if present, otherwise false."
            inheritable: inheritable.unwrap_or(false),
            id,
            annotation,
            source,
            // Resolved references (populated after reference resolution phase)
            resolved_type: None,
            resolved_ref: None,
        };
        let key = self.schema_set.arenas.alloc_attribute(data);
        self.allocated(ComponentKey::Attribute(key));
        if register {
            self.register_attribute(name, key, source_ref.as_ref())?;
        }
        Ok(key)
    }

    fn assemble_group(
        &mut self,
        result: GroupFrameResult,
        register: bool,
    ) -> SchemaResult<GroupKeyResult> {
        match result {
            GroupFrameResult::Model(group) => {
                let ModelGroupDefResult {
                    name,
                    ref_name,
                    compositor,
                    particles,
                    min_occurs,
                    max_occurs,
                    id,
                    annotation,
                    source,
                } = *group;
                let source_ref = source.clone();
                let name = name
                    .ok_or_else(|| missing_name("group", source_ref.as_ref(), self.schema_set))?;
                let data = ModelGroupData {
                    name: Some(name),
                    target_namespace: self.target_namespace,
                    ref_name,
                    compositor,
                    particles,
                    min_occurs,
                    max_occurs,
                    id,
                    annotation,
                    source,
                    // Resolved references (populated after reference resolution phase)
                    resolved_ref: None,
                    resolved_particles: Vec::new(),
                    resolved_particle_types: Vec::new(),
                    resolved_particle_elements: Vec::new(),
                    redefine_original: None,
                    redefine_requires_restriction_check: false,
                };
                let key = self.schema_set.arenas.alloc_model_group(data);
                self.allocated(ComponentKey::ModelGroup(key));
                if register {
                    self.register_model_group(name, key, source_ref.as_ref())?;
                }
                Ok(GroupKeyResult::Model(key))
            }
            GroupFrameResult::Attribute(group) => {
                let AttributeGroupDefResult {
                    name,
                    ref_name,
                    attributes,
                    attribute_groups,
                    attribute_wildcard,
                    id,
                    annotation,
                    source,
                } = *group;
                let source_ref = source.clone();
                let name = name.ok_or_else(|| {
                    missing_name("attributeGroup", source_ref.as_ref(), self.schema_set)
                })?;
                let data = AttributeGroupData {
                    name: Some(name),
                    target_namespace: self.target_namespace,
                    ref_name,
                    attributes,
                    attribute_groups,
                    attribute_wildcard,
                    id,
                    annotation,
                    source,
                    // Resolved references (populated after reference resolution phase)
                    resolved_ref: None,
                    resolved_attribute_groups: Vec::new(),
                    resolved_attributes: Vec::new(),
                    redefine_original: None,
                    redefine_requires_restriction_check: false,
                };
                let key = self.schema_set.arenas.alloc_attribute_group(data);
                self.allocated(ComponentKey::AttributeGroup(key));
                if register {
                    self.register_attribute_group(name, key, source_ref.as_ref())?;
                }
                Ok(GroupKeyResult::Attribute(key))
            }
        }
    }

    fn assemble_notation(
        &mut self,
        result: NotationResult,
        register: bool,
    ) -> SchemaResult<NotationKey> {
        let NotationResult {
            name,
            public,
            system,
            id,
            annotation,
            source,
        } = result;
        let source_ref = source.clone();
        let name =
            name.ok_or_else(|| missing_name("notation", source_ref.as_ref(), self.schema_set))?;
        let data = NotationData {
            name,
            target_namespace: self.target_namespace,
            public,
            system,
            id,
            annotation,
            source,
        };
        let key = self.schema_set.arenas.alloc_notation(data);
        self.allocated(ComponentKey::Notation(key));
        if register {
            self.register_notation(name, key, source_ref.as_ref())?;
        }
        Ok(key)
    }

    fn register_type(
        &mut self,
        name: NameId,
        key: TypeKey,
        source: Option<&SourceRef>,
    ) -> SchemaResult<()> {
        let location = source.and_then(|s| self.schema_set.source_maps.locate(s));
        let name_str = self.schema_set.name_table.resolve(name).to_string();
        let namespace = self.target_namespace;
        let previous = self.namespace_table(namespace).register_type(name, key);
        self.bound(namespace, name, PreviousBinding::Type(previous));
        if previous.is_some() {
            return Err(SchemaError::structural(
                "sch-props-correct",
                format!("Duplicate type declaration '{}'", name_str),
                location,
            ));
        }
        let kind = match key {
            TypeKey::Simple(_) => ComponentKind::SimpleType,
            TypeKey::Complex(_) => ComponentKind::ComplexType,
        };
        self.record_component(kind, name, ComponentKey::Type(key));
        Ok(())
    }

    /// Record a component in the per-document component index.
    fn record_component(&mut self, kind: ComponentKind, name: NameId, key: ComponentKey) {
        self.doc_components.insert(
            ComponentIdentity {
                kind,
                name,
                namespace: self.target_namespace,
            },
            key,
        );
    }

    fn register_element(
        &mut self,
        name: NameId,
        key: ElementKey,
        source: Option<&SourceRef>,
    ) -> SchemaResult<()> {
        let location = source.and_then(|s| self.schema_set.source_maps.locate(s));
        let name_str = self.schema_set.name_table.resolve(name).to_string();
        let namespace = self.target_namespace;
        let previous = self.namespace_table(namespace).register_element(name, key);
        self.bound(namespace, name, PreviousBinding::Element(previous));
        if previous.is_some() {
            return Err(SchemaError::structural(
                "sch-props-correct",
                format!("Duplicate element declaration '{}'", name_str),
                location,
            ));
        }
        self.record_component(ComponentKind::Element, name, ComponentKey::Element(key));
        Ok(())
    }

    fn register_attribute(
        &mut self,
        name: NameId,
        key: AttributeKey,
        source: Option<&SourceRef>,
    ) -> SchemaResult<()> {
        let location = source.and_then(|s| self.schema_set.source_maps.locate(s));
        let name_str = self.schema_set.name_table.resolve(name).to_string();
        let namespace = self.target_namespace;
        let previous = self
            .namespace_table(namespace)
            .register_attribute(name, key);
        self.bound(namespace, name, PreviousBinding::Attribute(previous));
        if previous.is_some() {
            return Err(SchemaError::structural(
                "sch-props-correct",
                format!("Duplicate attribute declaration '{}'", name_str),
                location,
            ));
        }
        self.record_component(ComponentKind::Attribute, name, ComponentKey::Attribute(key));
        Ok(())
    }

    fn register_model_group(
        &mut self,
        name: NameId,
        key: ModelGroupKey,
        source: Option<&SourceRef>,
    ) -> SchemaResult<()> {
        let location = source.and_then(|s| self.schema_set.source_maps.locate(s));
        let name_str = self.schema_set.name_table.resolve(name).to_string();
        let namespace = self.target_namespace;
        let previous = self
            .namespace_table(namespace)
            .register_model_group(name, key);
        self.bound(namespace, name, PreviousBinding::ModelGroup(previous));
        if previous.is_some() {
            return Err(SchemaError::structural(
                "sch-props-correct",
                format!("Duplicate group declaration '{}'", name_str),
                location,
            ));
        }
        self.record_component(
            ComponentKind::ModelGroup,
            name,
            ComponentKey::ModelGroup(key),
        );
        Ok(())
    }

    fn register_attribute_group(
        &mut self,
        name: NameId,
        key: AttributeGroupKey,
        source: Option<&SourceRef>,
    ) -> SchemaResult<()> {
        let location = source.and_then(|s| self.schema_set.source_maps.locate(s));
        let name_str = self.schema_set.name_table.resolve(name).to_string();
        let namespace = self.target_namespace;
        let previous = self
            .namespace_table(namespace)
            .register_attribute_group(name, key);
        self.bound(namespace, name, PreviousBinding::AttributeGroup(previous));
        if previous.is_some() {
            return Err(SchemaError::structural(
                "sch-props-correct",
                format!("Duplicate attribute group declaration '{}'", name_str),
                location,
            ));
        }
        self.record_component(
            ComponentKind::AttributeGroup,
            name,
            ComponentKey::AttributeGroup(key),
        );
        Ok(())
    }

    fn register_notation(
        &mut self,
        name: NameId,
        key: NotationKey,
        source: Option<&SourceRef>,
    ) -> SchemaResult<()> {
        let location = source.and_then(|s| self.schema_set.source_maps.locate(s));
        let name_str = self.schema_set.name_table.resolve(name).to_string();
        let namespace = self.target_namespace;
        let previous = self.namespace_table(namespace).register_notation(name, key);
        self.bound(namespace, name, PreviousBinding::Notation(previous));
        if previous.is_some() {
            return Err(SchemaError::structural(
                "sch-props-correct",
                format!("Duplicate notation declaration '{}'", name_str),
                location,
            ));
        }
        self.record_component(ComponentKind::Notation, name, ComponentKey::Notation(key));
        Ok(())
    }
}

pub fn assemble_schema(
    schema_set: &mut SchemaSet,
    doc_id: DocumentId,
    base_uri: &str,
    result: SchemaFrameResult,
) -> SchemaResult<SchemaDocument> {
    let target_namespace = result.target_namespace;
    let mut assembler = SchemaAssembler::new(
        schema_set,
        target_namespace,
        result.block_default,
        result.final_default,
    );
    assembler.assemble(result, doc_id, base_uri)
}

pub fn build_schema_document(
    result: &SchemaFrameResult,
    doc_id: DocumentId,
    base_uri: &str,
    name_table: &mut crate::namespace::NameTable,
) -> SchemaResult<SchemaDocument> {
    let mut doc = SchemaDocument::new(doc_id, base_uri.to_string());

    doc.target_namespace = result.target_namespace;
    doc.element_form_default = parse_form_choice(result.element_form_default.as_deref());
    doc.attribute_form_default = parse_form_choice(result.attribute_form_default.as_deref());
    doc.block_default = result.block_default;
    doc.final_default = result.final_default;
    doc.version = result.version.clone();
    doc.source = result.source.clone();
    doc.schema_id = result.id.clone();
    doc.xml_lang = result.xml_lang.clone();
    doc.annotations = result.annotations.clone();

    if let Some(default_attrs) = &result.default_attributes {
        doc.default_attributes = Some(QualifiedName::new(
            default_attrs.namespace,
            default_attrs.local_name,
            default_attrs.prefix,
        ));
    }

    if let Some(xpath_ns) = result.xpath_default_namespace.as_deref() {
        doc.xpath_default_namespace = name_table
            .get(xpath_ns)
            .or_else(|| Some(name_table.add(xpath_ns)));
    }

    if let Some(doc_result) = &result.default_open_content {
        doc.default_open_content = Some(crate::schema::DefaultOpenContent {
            source: doc_result.source.clone(),
            applies_to_empty: doc_result.applies_to_empty,
            mode: doc_result.mode.into(),
            wildcard: doc_result
                .wildcard
                .as_ref()
                .map(|wc| convert_element_wildcard(wc, result.target_namespace)),
        });
    }

    Ok(doc)
}

pub fn convert_directives(
    directives: Vec<DirectiveResult>,
    assembler: &mut SchemaAssembler<'_>,
) -> SchemaResult<DirectivesResult> {
    let mut includes = Vec::new();
    let mut imports = Vec::new();
    let mut redefines = Vec::new();
    let mut overrides = Vec::new();

    for directive in directives {
        match directive {
            DirectiveResult::Include(inc) => {
                includes.push(IncludeDirective {
                    source: inc.source.clone(),
                    schema_location: inc.schema_location.clone(),
                    resolved_doc_id: None,
                });
            }
            DirectiveResult::Import(imp) => {
                imports.push(ImportDirective {
                    source: imp.source.clone(),
                    namespace: imp.namespace.clone(),
                    schema_location: imp.schema_location.clone(),
                    resolved_doc_id: None,
                });
            }
            DirectiveResult::Redefine(red) => {
                let mut simple_types = Vec::new();
                let mut complex_types = Vec::new();
                let mut groups = Vec::new();
                let mut attribute_groups = Vec::new();

                // §src-redefine clause 5 / 7.2.1: each named component can be
                // redefined at most once within a single `<xs:redefine>`
                // block. The W3C `schT2` fixture exercises this. The check
                // must run before `assemble_*` so a duplicate's allocation
                // does not pollute the arenas before we error out.
                let mut seen_simple_types: HashSet<NameId> = HashSet::new();
                let mut seen_complex_types: HashSet<NameId> = HashSet::new();
                let mut seen_groups: HashSet<NameId> = HashSet::new();
                let mut seen_attribute_groups: HashSet<NameId> = HashSet::new();

                fn dup_redefine_err(
                    schema_set: &SchemaSet,
                    kind_label: &str,
                    name: NameId,
                    source: Option<&SourceRef>,
                ) -> SchemaError {
                    let location = source.and_then(|s| schema_set.source_maps.locate(s));
                    SchemaError::structural(
                        "src-redefine",
                        format!(
                            "Duplicate <xs:{} name=\"{}\"> within a single <xs:redefine> block",
                            kind_label,
                            schema_set.name_table.resolve(name),
                        ),
                        location,
                    )
                }

                for component in red.components {
                    match component {
                        RedefineComponent::SimpleType(st) => {
                            if let Some(name) = st.name {
                                if !seen_simple_types.insert(name) {
                                    return Err(dup_redefine_err(
                                        assembler.schema_set,
                                        "simpleType",
                                        name,
                                        st.source.as_ref(),
                                    ));
                                }
                            }
                            let key =
                                assembler.assemble_type(TypeFrameResult::Simple(st), false)?;
                            if let TypeKey::Simple(simple) = key {
                                simple_types.push(simple);
                            }
                        }
                        RedefineComponent::ComplexType(ct) => {
                            if let Some(name) = ct.name {
                                if !seen_complex_types.insert(name) {
                                    return Err(dup_redefine_err(
                                        assembler.schema_set,
                                        "complexType",
                                        name,
                                        ct.source.as_ref(),
                                    ));
                                }
                            }
                            let key =
                                assembler.assemble_type(TypeFrameResult::Complex(ct), false)?;
                            if let TypeKey::Complex(complex) = key {
                                complex_types.push(complex);
                            }
                        }
                        RedefineComponent::Group(group) => {
                            if let Some(name) = group.name {
                                if !seen_groups.insert(name) {
                                    return Err(dup_redefine_err(
                                        assembler.schema_set,
                                        "group",
                                        name,
                                        group.source.as_ref(),
                                    ));
                                }
                            }
                            if let GroupKeyResult::Model(key) =
                                assembler.assemble_group(GroupFrameResult::Model(group), false)?
                            {
                                groups.push(key);
                            }
                        }
                        RedefineComponent::AttributeGroup(group) => {
                            if let Some(name) = group.name {
                                if !seen_attribute_groups.insert(name) {
                                    return Err(dup_redefine_err(
                                        assembler.schema_set,
                                        "attributeGroup",
                                        name,
                                        group.source.as_ref(),
                                    ));
                                }
                            }
                            if let GroupKeyResult::Attribute(key) = assembler
                                .assemble_group(GroupFrameResult::Attribute(group), false)?
                            {
                                attribute_groups.push(key);
                            }
                        }
                    }
                }

                redefines.push(RedefineDirective {
                    source: red.source.clone(),
                    schema_location: red.schema_location.clone(),
                    resolved_doc_id: None,
                    simple_types,
                    complex_types,
                    groups,
                    attribute_groups,
                });
            }
            DirectiveResult::Override(override_result) => {
                overrides.push(convert_override(override_result, assembler)?);
            }
        }
    }

    Ok((includes, imports, redefines, overrides))
}

fn convert_override(
    override_result: OverrideResult,
    assembler: &mut SchemaAssembler<'_>,
) -> SchemaResult<OverrideDirective> {
    // Override children should NOT inherit the overriding document's defaults.
    // Per F.2, they are conceptually placed into the overridden document D2',
    // so D2's blockDefault/finalDefault should apply instead. We assemble
    // with empty defaults here; the correct defaults are applied later in
    // `apply_override()` once D2 is known.
    let saved_block = std::mem::replace(
        &mut assembler.block_default,
        crate::schema::model::DerivationSet::empty(),
    );
    let saved_final = std::mem::replace(
        &mut assembler.final_default,
        crate::schema::model::DerivationSet::empty(),
    );

    let result = convert_override_body(override_result, assembler);

    // Restore the overriding document's defaults for any subsequent
    // non-override components in the same document.
    assembler.block_default = saved_block;
    assembler.final_default = saved_final;

    result
}

fn convert_override_body(
    override_result: OverrideResult,
    assembler: &mut SchemaAssembler<'_>,
) -> SchemaResult<OverrideDirective> {
    let mut components = Vec::new();

    for st in override_result.simple_types {
        let key = assembler.assemble_type(TypeFrameResult::Simple(Box::new(st)), false)?;
        if let TypeKey::Simple(simple) = key {
            components.push(OverrideComponent::SimpleType(simple));
        }
    }

    for ct in override_result.complex_types {
        let key = assembler.assemble_type(TypeFrameResult::Complex(Box::new(ct)), false)?;
        if let TypeKey::Complex(complex) = key {
            components.push(OverrideComponent::ComplexType(complex));
        }
    }

    for el in override_result.elements {
        let key = assembler.assemble_element(el, false)?;
        components.push(OverrideComponent::Element(key));
    }

    for attr in override_result.attributes {
        let key = assembler.assemble_attribute(attr, false)?;
        components.push(OverrideComponent::Attribute(key));
    }

    for group in override_result.groups {
        match group {
            GroupFrameResult::Model(model) => {
                if let GroupKeyResult::Model(key) =
                    assembler.assemble_group(GroupFrameResult::Model(model), false)?
                {
                    components.push(OverrideComponent::Group(key));
                }
            }
            GroupFrameResult::Attribute(group) => {
                if let GroupKeyResult::Attribute(key) =
                    assembler.assemble_group(GroupFrameResult::Attribute(group), false)?
                {
                    components.push(OverrideComponent::AttributeGroup(key));
                }
            }
        }
    }

    for group in override_result.attribute_groups {
        if let GroupFrameResult::Attribute(group) = group {
            if let GroupKeyResult::Attribute(key) =
                assembler.assemble_group(GroupFrameResult::Attribute(group), false)?
            {
                components.push(OverrideComponent::AttributeGroup(key));
            }
        }
    }

    for notation in override_result.notations {
        let key = assembler.assemble_notation(notation, false)?;
        components.push(OverrideComponent::Notation(key));
    }

    Ok(OverrideDirective {
        source: override_result.source.clone(),
        schema_location: override_result.schema_location.clone(),
        resolved_doc_id: None,
        components,
    })
}

pub fn parse_form_choice(value: Option<&str>) -> FormChoice {
    match value {
        Some("qualified") => FormChoice::Qualified,
        Some("unqualified") | None => FormChoice::Unqualified,
        _ => FormChoice::Unqualified,
    }
}

fn convert_element_wildcard(
    wildcard: &crate::parser::frames::WildcardResult,
    target_namespace: Option<NameId>,
) -> ElementWildcard {
    use crate::schema::wildcard::QNameDisallowed;

    let mut result = ElementWildcard::new();
    result.namespace_constraint = match &wildcard.namespace {
        crate::parser::frames::WildcardNamespace::Any => NamespaceConstraint::Any,
        crate::parser::frames::WildcardNamespace::Other => NamespaceConstraint::Other,
        crate::parser::frames::WildcardNamespace::TargetNamespace => {
            NamespaceConstraint::Enumeration(vec![target_namespace])
        }
        crate::parser::frames::WildcardNamespace::Local => {
            NamespaceConstraint::Enumeration(vec![None])
        }
        crate::parser::frames::WildcardNamespace::List(list) => NamespaceConstraint::Enumeration(
            list.iter().map(|t| t.resolve(target_namespace)).collect(),
        ),
    };

    // notNamespace → NamespaceConstraint::Not(...)
    if !wildcard.not_namespace.is_empty() {
        let excluded: Vec<Option<NameId>> = wildcard
            .not_namespace
            .iter()
            .map(|t| t.resolve(target_namespace))
            .collect();
        result.namespace_constraint = NamespaceConstraint::Not(excluded);
    }

    // notQName → not_qnames
    result.not_qnames = wildcard
        .not_qname
        .iter()
        .map(|item| match item {
            crate::parser::frames::NotQNameItem::QName {
                namespace,
                local_name,
            } => QNameDisallowed::QName {
                namespace: *namespace,
                local_name: *local_name,
            },
            crate::parser::frames::NotQNameItem::Defined => QNameDisallowed::Defined,
            crate::parser::frames::NotQNameItem::DefinedSibling => QNameDisallowed::DefinedSibling,
        })
        .collect();

    result.process_contents = match wildcard.process_contents {
        crate::parser::frames::ProcessContents::Strict => SchemaProcessContents::Strict,
        crate::parser::frames::ProcessContents::Lax => SchemaProcessContents::Lax,
        crate::parser::frames::ProcessContents::Skip => SchemaProcessContents::Skip,
    };
    result.id = wildcard.id.clone();
    result.source = wildcard.source.clone();
    result
}

fn missing_name(kind: &str, source: Option<&SourceRef>, schema_set: &SchemaSet) -> SchemaError {
    let location = source.and_then(|s| schema_set.source_maps.locate(s));
    SchemaError::structural(
        "src-resolve",
        format!("Missing required name for {}", kind),
        location,
    )
}
