// ============================================================================
// Alternative Frame (XSD 1.1)
// ============================================================================

/// Frame for xs:alternative
pub struct AlternativeFrame {
    test: Option<String>,
    type_ref: Option<TypeRefResult>,
    inline_type: Option<Box<TypeFrameResult>>,
    xpath_default_namespace: Option<String>,
    ns_snapshot: NamespaceContextSnapshot,
    id: Option<String>,
    annotation: Option<Annotation>,
    source: Option<SourceRef>,
    foreign_attributes: Vec<ForeignAttribute>,
}

impl AlternativeFrame {
    pub fn new(
        attrs: &AttributeMap,
        name_table: &NameTable,
        source: Option<SourceRef>,
        ns_snapshot: NamespaceContextSnapshot,
    ) -> SchemaResult<Self> {
        let test = attrs
            .get_value_by_name(name_table, "test")
            .map(String::from);
        #[cfg(feature = "xsd11")]
        if let Some(test) = &test {
            crate::xpath::depth::check_schema_test(test, xsd_names::ALTERNATIVE)?;
        }

        let type_ref = attrs
            .get_value_by_name(name_table, "type")
            .map(|s| parse_qname_ref(s, name_table, &ns_snapshot))
            .transpose()?
            .map(TypeRefResult::QName);

        let xpath_default_namespace = attrs
            .get_value_by_name(name_table, "xpathDefaultNamespace")
            .map(String::from);

        let id = attrs
            .get_value_by_name(name_table, "id")
            .map(String::from);

        Ok(Self {
            test,
            type_ref,
            inline_type: None,
            xpath_default_namespace,
            ns_snapshot,
            id,
            annotation: None,
            source,
            foreign_attributes: Vec::new(),
        })
    }
}

impl Frame for AlternativeFrame {
    fn allows(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(
            local_name,
            xsd_names::ANNOTATION | xsd_names::SIMPLE_TYPE | xsd_names::COMPLEX_TYPE
        )
    }

    fn allows_attribute(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(
            local_name,
            "test" | "type" | "xpathDefaultNamespace" | "id"
        )
    }

    fn on_child_start(&mut self, _local_name: &str, _name_table: &NameTable) {}

    fn attach(&mut self, child: FrameResult) -> SchemaResult<()> {
        match child {
            FrameResult::Annotation(ann) => {
                self.annotation = Some(ann);
            }
            FrameResult::Type(t) => {
                self.inline_type = Some(Box::new(t));
            }
            FrameResult::Skip => {}
            _ => {}
        }
        Ok(())
    }

    fn finish(self: Box<Self>) -> SchemaResult<FrameResult> {
        let annotation = merge_foreign_attributes(
            self.annotation,
            self.foreign_attributes,
            self.source.clone(),
        );
        Ok(FrameResult::Alternative(AlternativeResult {
            test: self.test,
            type_ref: self.type_ref,
            inline_type: self.inline_type,
            xpath_default_namespace: self.xpath_default_namespace,
            ns_snapshot: self.ns_snapshot,
            resolved_type: None,
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

// ============================================================================
// Assert Frame (XSD 1.1)
// ============================================================================

/// Frame for xs:assert
pub struct AssertFrame {
    test: String,
    xpath_default_namespace: Option<String>,
    ns_snapshot: NamespaceContextSnapshot,
    id: Option<String>,
    annotation: Option<Annotation>,
    source: Option<SourceRef>,
    foreign_attributes: Vec<ForeignAttribute>,
}

impl AssertFrame {
    pub fn new(
        attrs: &AttributeMap,
        name_table: &NameTable,
        source: Option<SourceRef>,
        ns_snapshot: NamespaceContextSnapshot,
    ) -> SchemaResult<Self> {
        let test = attrs
            .get_value_by_name(name_table, "test")
            .map(String::from)
            .unwrap_or_default();
        #[cfg(feature = "xsd11")]
        crate::xpath::depth::check_schema_test(&test, xsd_names::ASSERT)?;

        let xpath_default_namespace = attrs
            .get_value_by_name(name_table, "xpathDefaultNamespace")
            .map(String::from);

        let id = attrs
            .get_value_by_name(name_table, "id")
            .map(String::from);

        Ok(Self {
            test,
            xpath_default_namespace,
            ns_snapshot,
            id,
            annotation: None,
            source,
            foreign_attributes: Vec::new(),
        })
    }
}

impl Frame for AssertFrame {
    fn allows(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(local_name, xsd_names::ANNOTATION)
    }

    fn allows_attribute(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(local_name, "test" | "xpathDefaultNamespace" | "id")
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
        Ok(FrameResult::Assert(AssertResult {
            test: self.test,
            xpath_default_namespace: self.xpath_default_namespace,
            ns_snapshot: self.ns_snapshot,
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

