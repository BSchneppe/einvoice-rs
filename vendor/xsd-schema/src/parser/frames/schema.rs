// ============================================================================
// Schema Frame
// ============================================================================

/// Parsing phase for schema element
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SchemaPhase {
    /// Expecting composition elements (include, import, redefine, override)
    Composition,
    /// Expecting annotation or components
    Components,
}

/// Frame for xs:schema element
pub struct SchemaFrame {
    phase: SchemaPhase,
    target_namespace: Option<NameId>,
    element_form_default: Option<String>,
    attribute_form_default: Option<String>,
    block_default: DerivationSet,
    default_attributes: Option<QNameRef>,
    xpath_default_namespace: Option<String>,
    final_default: DerivationSet,
    version: Option<String>,
    default_open_content: Option<DefaultOpenContentResult>,
    xml_lang: Option<String>,
    id: Option<String>,
    source: Option<SourceRef>,
    annotations: Vec<Annotation>,
    directives: Vec<DirectiveResult>,
    components: Vec<FrameResult>,
    foreign_attributes: Vec<ForeignAttribute>,
    xml_namespace_id: Option<NameId>,
    xml_lang_id: Option<NameId>,
}

impl SchemaFrame {
    pub fn new(
        attrs: &AttributeMap,
        name_table: &NameTable,
        source: Option<SourceRef>,
        ns_snapshot: &NamespaceContextSnapshot,
    ) -> SchemaResult<Self> {
        let target_namespace = attrs
            .get_value_by_name(name_table, "targetNamespace")
            .map(|s| name_table.add(s));

        validate_attr_value(attrs, name_table, "elementFormDefault", parse_form)?;
        let element_form_default = attrs
            .get_value_by_name(name_table, "elementFormDefault")
            .map(String::from);

        validate_attr_value(attrs, name_table, "attributeFormDefault", parse_form)?;
        let attribute_form_default = attrs
            .get_value_by_name(name_table, "attributeFormDefault")
            .map(String::from);

        let block_default = parse_derivation_set(
            attrs.get_value_by_name(name_table, "blockDefault"),
        )?;

        let default_attributes = attrs
            .get_value_by_name(name_table, "defaultAttributes")
            .map(|s| parse_qname_ref(s, name_table, ns_snapshot))
            .transpose()?;

        #[cfg(feature = "xsd11")]
        let xpath_default_namespace = attrs
            .get_value_by_name(name_table, "xpathDefaultNamespace")
            .map(String::from);
        #[cfg(not(feature = "xsd11"))]
        let xpath_default_namespace: Option<String> = None;

        let final_default = parse_derivation_set(
            attrs.get_value_by_name(name_table, "finalDefault"),
        )?;

        let version = attrs
            .get_value_by_name(name_table, "version")
            .map(String::from);

        let id = attrs
            .get_value_by_name(name_table, "id")
            .map(String::from);

        Ok(Self {
            phase: SchemaPhase::Composition,
            target_namespace,
            element_form_default,
            attribute_form_default,
            block_default,
            default_attributes,
            xpath_default_namespace,
            final_default,
            version,
            default_open_content: None,
            xml_lang: None,
            id,
            source,
            annotations: Vec::new(),
            directives: Vec::new(),
            components: Vec::new(),
            foreign_attributes: Vec::new(),
            xml_namespace_id: name_table.get(crate::namespace::XML_NAMESPACE),
            xml_lang_id: name_table.get("lang"),
        })
    }
}

impl Frame for SchemaFrame {
    fn allows(&self, local_name: &str, _name_table: &NameTable) -> bool {
        #[cfg(feature = "xsd11")]
        let is_xsd11_element = matches!(
            local_name,
            xsd_names::OVERRIDE | xsd_names::DEFAULT_OPEN_CONTENT
        );
        #[cfg(not(feature = "xsd11"))]
        let is_xsd11_element = false;

        match self.phase {
            SchemaPhase::Composition => matches!(
                local_name,
                xsd_names::INCLUDE
                    | xsd_names::IMPORT
                    | xsd_names::REDEFINE
                    | xsd_names::ANNOTATION
                    | xsd_names::SIMPLE_TYPE
                    | xsd_names::COMPLEX_TYPE
                    | xsd_names::ELEMENT
                    | xsd_names::ATTRIBUTE
                    | xsd_names::GROUP
                    | xsd_names::ATTRIBUTE_GROUP
                    | xsd_names::NOTATION
            ) || is_xsd11_element,
            SchemaPhase::Components => matches!(
                local_name,
                xsd_names::ANNOTATION
                    | xsd_names::SIMPLE_TYPE
                    | xsd_names::COMPLEX_TYPE
                    | xsd_names::ELEMENT
                    | xsd_names::ATTRIBUTE
                    | xsd_names::GROUP
                    | xsd_names::ATTRIBUTE_GROUP
                    | xsd_names::NOTATION
            ),
        }
    }

    fn allows_attribute(&self, local_name: &str, _name_table: &NameTable) -> bool {
        #[cfg(feature = "xsd11")]
        if local_name == "xpathDefaultNamespace" {
            return true;
        }
        matches!(
            local_name,
            "targetNamespace"
                | "elementFormDefault"
                | "attributeFormDefault"
                | "blockDefault"
                | "defaultAttributes"
                | "finalDefault"
                | "version"
                | "id"
        )
    }

    fn on_child_start(&mut self, local_name: &str, _name_table: &NameTable) {
        // Transition from Composition to Components when we see a component
        if self.phase == SchemaPhase::Composition {
            match local_name {
                xsd_names::SIMPLE_TYPE
                | xsd_names::COMPLEX_TYPE
                | xsd_names::ELEMENT
                | xsd_names::ATTRIBUTE
                | xsd_names::GROUP
                | xsd_names::ATTRIBUTE_GROUP
                | xsd_names::NOTATION => {
                    self.phase = SchemaPhase::Components;
                }
                _ => {}
            }
        }
    }

    fn attach(&mut self, child: FrameResult) -> SchemaResult<()> {
        match child {
            FrameResult::Annotation(ann) => {
                self.annotations.push(ann);
            }
            FrameResult::Directive(dir) => {
                self.directives.push(dir);
            }
            FrameResult::Type(_)
            | FrameResult::Element(_)
            | FrameResult::Attribute(_)
            | FrameResult::Group(_)
            | FrameResult::Notation(_) => {
                self.components.push(child);
            }
            FrameResult::DefaultOpenContent(doc) => {
                self.default_open_content = Some(doc);
            }
            FrameResult::Skip => {}
            _ => {}
        }
        Ok(())
    }

    fn finish(self: Box<Self>) -> SchemaResult<FrameResult> {
        Ok(FrameResult::Schema(SchemaFrameResult {
            target_namespace: self.target_namespace,
            element_form_default: self.element_form_default,
            attribute_form_default: self.attribute_form_default,
            block_default: self.block_default,
            default_attributes: self.default_attributes,
            xpath_default_namespace: self.xpath_default_namespace,
            final_default: self.final_default,
            version: self.version,
            default_open_content: self.default_open_content,
            xml_lang: self.xml_lang,
            id: self.id,
            source: self.source,
            annotations: self.annotations,
            directives: self.directives,
            components: self.components,
        }))
    }

    fn children_are_top_level(&self) -> bool {
        true
    }

    fn source(&self) -> Option<&SourceRef> {
        self.source.as_ref()
    }

    fn set_foreign_attributes(&mut self, attrs: Vec<ForeignAttribute>) {
        if let (Some(xml_ns), Some(lang_id)) = (self.xml_namespace_id, self.xml_lang_id) {
            if let Some(attr) = attrs
                .iter()
                .find(|attr| attr.namespace == Some(xml_ns) && attr.local_name == lang_id)
            {
                self.xml_lang = Some(attr.value.clone());
            }
        }
        self.foreign_attributes = attrs;
    }
}

