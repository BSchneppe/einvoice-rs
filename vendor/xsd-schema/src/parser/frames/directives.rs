// ============================================================================
// Include/Import/Redefine Frames
// ============================================================================

/// Frame for xs:include
pub struct IncludeFrame {
    schema_location: Option<String>,
    id: Option<String>,
    annotation: Option<Annotation>,
    source: Option<SourceRef>,
    foreign_attributes: Vec<ForeignAttribute>,
}

impl IncludeFrame {
    pub fn new(
        attrs: &AttributeMap,
        name_table: &NameTable,
        source: Option<SourceRef>,
    ) -> SchemaResult<Self> {
        let schema_location = attrs
            .get_value_by_name(name_table, "schemaLocation")
            .map(String::from);

        let id = attrs
            .get_value_by_name(name_table, "id")
            .map(String::from);

        Ok(Self {
            schema_location,
            id,
            annotation: None,
            source,
            foreign_attributes: Vec::new(),
        })
    }
}

impl Frame for IncludeFrame {
    fn allows(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(local_name, xsd_names::ANNOTATION)
    }

    fn allows_attribute(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(local_name, "schemaLocation" | "id")
    }

    fn on_child_start(&mut self, _local_name: &str, _name_table: &NameTable) {}

    fn attach(&mut self, child: FrameResult) -> SchemaResult<()> {
        if let FrameResult::Annotation(ann) = child {
            self.annotation = Some(ann);
        }
        Ok(())
    }

    fn finish(self: Box<Self>) -> SchemaResult<FrameResult> {
        let schema_location = self.schema_location.ok_or_else(|| SchemaError::structural(
            "src-include",
            "xs:include requires 'schemaLocation' attribute",
            None,
        ))?;

        let annotation = merge_foreign_attributes(
            self.annotation,
            self.foreign_attributes,
            self.source.clone(),
        );
        Ok(FrameResult::Directive(DirectiveResult::Include(IncludeResult {
            schema_location,
            id: self.id,
            annotation,
            source: self.source,
        })))
    }

    fn has_annotation(&self) -> bool {
        self.annotation.is_some()
    }

    fn source(&self) -> Option<&SourceRef> {
        self.source.as_ref()
    }

    fn set_foreign_attributes(&mut self, attrs: Vec<ForeignAttribute>) {
        self.foreign_attributes = attrs;
    }
}

/// Frame for xs:import
pub struct ImportFrame {
    namespace: Option<String>,
    schema_location: Option<String>,
    id: Option<String>,
    annotation: Option<Annotation>,
    source: Option<SourceRef>,
    foreign_attributes: Vec<ForeignAttribute>,
}

impl ImportFrame {
    pub fn new(
        attrs: &AttributeMap,
        name_table: &NameTable,
        source: Option<SourceRef>,
    ) -> SchemaResult<Self> {
        let namespace = attrs
            .get_value_by_name(name_table, "namespace")
            .map(String::from);

        let schema_location = attrs
            .get_value_by_name(name_table, "schemaLocation")
            .map(String::from);

        let id = attrs
            .get_value_by_name(name_table, "id")
            .map(String::from);

        Ok(Self {
            namespace,
            schema_location,
            id,
            annotation: None,
            source,
            foreign_attributes: Vec::new(),
        })
    }
}

impl Frame for ImportFrame {
    fn allows(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(local_name, xsd_names::ANNOTATION)
    }

    fn allows_attribute(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(local_name, "namespace" | "schemaLocation" | "id")
    }

    fn on_child_start(&mut self, _local_name: &str, _name_table: &NameTable) {}

    fn attach(&mut self, child: FrameResult) -> SchemaResult<()> {
        if let FrameResult::Annotation(ann) = child {
            self.annotation = Some(ann);
        }
        Ok(())
    }

    fn finish(self: Box<Self>) -> SchemaResult<FrameResult> {
        let annotation = merge_foreign_attributes(
            self.annotation,
            self.foreign_attributes,
            self.source.clone(),
        );
        Ok(FrameResult::Directive(DirectiveResult::Import(ImportResult {
            namespace: self.namespace,
            schema_location: self.schema_location,
            id: self.id,
            annotation,
            source: self.source,
        })))
    }

    fn has_annotation(&self) -> bool {
        self.annotation.is_some()
    }

    fn source(&self) -> Option<&SourceRef> {
        self.source.as_ref()
    }

    fn set_foreign_attributes(&mut self, attrs: Vec<ForeignAttribute>) {
        self.foreign_attributes = attrs;
    }
}

#[cfg(feature = "xsd11")]
/// Frame for xs:override (XSD 1.1)
pub struct OverrideFrame {
    schema_location: Option<String>,
    id: Option<String>,
    annotation: Option<Annotation>,
    source: Option<SourceRef>,
    foreign_attributes: Vec<ForeignAttribute>,
    // Overridden components (schemaTop)
    simple_types: Vec<SimpleTypeResult>,
    complex_types: Vec<ComplexTypeResult>,
    elements: Vec<ElementFrameResult>,
    attributes: Vec<AttributeFrameResult>,
    groups: Vec<GroupFrameResult>,
    attribute_groups: Vec<GroupFrameResult>,
    notations: Vec<NotationResult>,
}

#[cfg(feature = "xsd11")]
impl OverrideFrame {
    pub fn new(
        attrs: &AttributeMap,
        name_table: &NameTable,
        source: Option<SourceRef>,
    ) -> SchemaResult<Self> {
        let schema_location = attrs
            .get_value_by_name(name_table, "schemaLocation")
            .map(String::from);

        let id = attrs
            .get_value_by_name(name_table, "id")
            .map(String::from);

        Ok(Self {
            schema_location,
            id,
            annotation: None,
            source,
            foreign_attributes: Vec::new(),
            simple_types: Vec::new(),
            complex_types: Vec::new(),
            elements: Vec::new(),
            attributes: Vec::new(),
            groups: Vec::new(),
            attribute_groups: Vec::new(),
            notations: Vec::new(),
        })
    }
}

#[cfg(feature = "xsd11")]
impl Frame for OverrideFrame {
    fn allows(&self, local_name: &str, _name_table: &NameTable) -> bool {
        // Override can contain annotation and schemaTop elements
        matches!(
            local_name,
            xsd_names::ANNOTATION
                | xsd_names::SIMPLE_TYPE
                | xsd_names::COMPLEX_TYPE
                | xsd_names::ELEMENT
                | xsd_names::ATTRIBUTE
                | xsd_names::GROUP
                | xsd_names::ATTRIBUTE_GROUP
                | xsd_names::NOTATION
        )
    }

    fn allows_attribute(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(local_name, "schemaLocation" | "id")
    }

    fn on_child_start(&mut self, _local_name: &str, _name_table: &NameTable) {}

    fn attach(&mut self, child: FrameResult) -> SchemaResult<()> {
        match child {
            FrameResult::Annotation(ann) => {
                self.annotation = Some(ann);
            }
            FrameResult::Type(TypeFrameResult::Simple(st)) => {
                self.simple_types.push(*st);
            }
            FrameResult::Type(TypeFrameResult::Complex(ct)) => {
                self.complex_types.push(*ct);
            }
            FrameResult::Element(el) => {
                self.elements.push(el);
            }
            FrameResult::Attribute(attr) => {
                self.attributes.push(attr);
            }
            FrameResult::Group(group) => {
                match group {
                    GroupFrameResult::Model(_) => self.groups.push(group),
                    GroupFrameResult::Attribute(_) => self.attribute_groups.push(group),
                }
            }
            FrameResult::Notation(notation) => {
                self.notations.push(notation);
            }
            _ => {}
        }
        Ok(())
    }

    fn finish(self: Box<Self>) -> SchemaResult<FrameResult> {
        let schema_location = self.schema_location.ok_or_else(|| SchemaError::structural(
            "src-override",
            "xs:override requires 'schemaLocation' attribute",
            None,
        ))?;

        let annotation = merge_foreign_attributes(
            self.annotation,
            self.foreign_attributes,
            self.source.clone(),
        );
        Ok(FrameResult::Directive(DirectiveResult::Override(OverrideResult {
            schema_location,
            id: self.id,
            annotation,
            source: self.source,
            simple_types: self.simple_types,
            complex_types: self.complex_types,
            elements: self.elements,
            attributes: self.attributes,
            groups: self.groups,
            attribute_groups: self.attribute_groups,
            notations: self.notations,
        })))
    }

    fn children_are_top_level(&self) -> bool {
        true
    }

    fn source(&self) -> Option<&SourceRef> {
        self.source.as_ref()
    }

    fn set_foreign_attributes(&mut self, attrs: Vec<ForeignAttribute>) {
        self.foreign_attributes = attrs;
    }
}

/// Frame for xs:redefine
pub struct RedefineFrame {
    schema_location: Option<String>,
    id: Option<String>,
    annotation: Option<Annotation>,
    components: Vec<RedefineComponent>,
    source: Option<SourceRef>,
    foreign_attributes: Vec<ForeignAttribute>,
}

impl RedefineFrame {
    pub fn new(
        attrs: &AttributeMap,
        name_table: &NameTable,
        source: Option<SourceRef>,
    ) -> SchemaResult<Self> {
        let schema_location = attrs
            .get_value_by_name(name_table, "schemaLocation")
            .map(String::from);

        let id = attrs
            .get_value_by_name(name_table, "id")
            .map(String::from);

        Ok(Self {
            schema_location,
            id,
            annotation: None,
            components: Vec::new(),
            source,
            foreign_attributes: Vec::new(),
        })
    }
}

impl Frame for RedefineFrame {
    fn allows(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(
            local_name,
            xsd_names::ANNOTATION
                | xsd_names::SIMPLE_TYPE
                | xsd_names::COMPLEX_TYPE
                | xsd_names::GROUP
                | xsd_names::ATTRIBUTE_GROUP
        )
    }

    fn allows_attribute(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(local_name, "schemaLocation" | "id")
    }

    fn on_child_start(&mut self, _local_name: &str, _name_table: &NameTable) {}

    fn attach(&mut self, child: FrameResult) -> SchemaResult<()> {
        match child {
            FrameResult::Annotation(ann) => {
                self.annotation = Some(ann);
            }
            FrameResult::Type(TypeFrameResult::Simple(st)) => {
                self.components.push(RedefineComponent::SimpleType(st));
            }
            FrameResult::Type(TypeFrameResult::Complex(ct)) => {
                self.components.push(RedefineComponent::ComplexType(ct));
            }
            FrameResult::Group(GroupFrameResult::Model(mg)) => {
                self.components.push(RedefineComponent::Group(mg));
            }
            FrameResult::Group(GroupFrameResult::Attribute(ag)) => {
                self.components.push(RedefineComponent::AttributeGroup(ag));
            }
            FrameResult::Skip => {}
            _ => {}
        }
        Ok(())
    }

    fn finish(self: Box<Self>) -> SchemaResult<FrameResult> {
        let schema_location = self.schema_location.ok_or_else(|| SchemaError::structural(
            "src-redefine",
            "xs:redefine requires 'schemaLocation' attribute",
            None,
        ))?;

        let annotation = merge_foreign_attributes(
            self.annotation,
            self.foreign_attributes,
            self.source.clone(),
        );
        Ok(FrameResult::Directive(DirectiveResult::Redefine(RedefineResult {
            schema_location,
            id: self.id,
            annotation,
            components: self.components,
            source: self.source,
        })))
    }

    fn children_are_top_level(&self) -> bool {
        true
    }

    fn source(&self) -> Option<&SourceRef> {
        self.source.as_ref()
    }

    fn set_foreign_attributes(&mut self, attrs: Vec<ForeignAttribute>) {
        self.foreign_attributes = attrs;
    }
}

