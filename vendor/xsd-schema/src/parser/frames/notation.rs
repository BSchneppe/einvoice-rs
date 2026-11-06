// ============================================================================
// Notation Frame
// ============================================================================

/// Frame for xs:notation
pub struct NotationFrame {
    name: Option<NameId>,
    public: Option<String>,
    system: Option<String>,
    id: Option<String>,
    annotation: Option<Annotation>,
    source: Option<SourceRef>,
    foreign_attributes: Vec<ForeignAttribute>,
}

impl NotationFrame {
    pub fn new(
        attrs: &AttributeMap,
        name_table: &NameTable,
        source: Option<SourceRef>,
    ) -> SchemaResult<Self> {
        let name = attrs
            .get_value_by_name(name_table, "name")
            .and_then(|s| name_table.get(s));

        let public = attrs
            .get_value_by_name(name_table, "public")
            .map(String::from);

        let system = attrs
            .get_value_by_name(name_table, "system")
            .map(String::from);

        let id = attrs
            .get_value_by_name(name_table, "id")
            .map(String::from);

        Ok(Self {
            name,
            public,
            system,
            id,
            annotation: None,
            source,
            foreign_attributes: Vec::new(),
        })
    }
}

impl Frame for NotationFrame {
    fn allows(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(local_name, xsd_names::ANNOTATION)
    }

    fn allows_attribute(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(local_name, "name" | "public" | "system" | "id")
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
        Ok(FrameResult::Notation(NotationResult {
            name: self.name,
            public: self.public,
            system: self.system,
            id: self.id,
            annotation,
            source: self.source,
        }))
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

