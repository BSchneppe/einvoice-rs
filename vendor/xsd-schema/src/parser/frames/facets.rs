// ============================================================================
// Facet Frame
// ============================================================================

/// Frame for facet elements (enumeration, pattern, etc.)
pub struct FacetFrame {
    kind: FacetKind,
    value: String,
    fixed: bool,
    #[allow(dead_code)]
    id: Option<String>,
    /// XSD 1.1 assertion: xpathDefaultNamespace attribute (raw string)
    xpath_default_namespace: Option<String>,
    /// XSD 1.1 assertion: namespace bindings snapshot for XPath prefix resolution
    ns_snapshot: Option<NamespaceContextSnapshot>,
    annotation: Option<Annotation>,
    source: Option<SourceRef>,
    foreign_attributes: Vec<ForeignAttribute>,
}

impl FacetFrame {
    pub fn new(
        kind: FacetKind,
        attrs: &AttributeMap,
        name_table: &NameTable,
        source: Option<SourceRef>,
        ns_snapshot: Option<NamespaceContextSnapshot>,
    ) -> SchemaResult<Self> {
        // For assertion facets, read 'test' attribute instead of 'value'
        let value = if kind == FacetKind::Assertion {
            attrs
                .get_value_by_name(name_table, "test")
                .map(String::from)
                .unwrap_or_default()
        } else {
            attrs
                .get_value_by_name(name_table, "value")
                .map(String::from)
                .unwrap_or_default()
        };

        #[cfg(feature = "xsd11")]
        if kind == FacetKind::Assertion {
            crate::xpath::depth::check_schema_test(&value, xsd_names::ASSERTION)?;
        }

        let fixed = parse_bool_attr_default(attrs, name_table, "fixed", false)?;

        let id = attrs
            .get_value_by_name(name_table, "id")
            .map(String::from);

        // For assertion facets, read xpathDefaultNamespace
        let xpath_default_namespace = if kind == FacetKind::Assertion {
            attrs
                .get_value_by_name(name_table, "xpathDefaultNamespace")
                .map(String::from)
        } else {
            None
        };

        Ok(Self {
            kind,
            value,
            fixed,
            id,
            xpath_default_namespace,
            ns_snapshot,
            annotation: None,
            source,
            foreign_attributes: Vec::new(),
        })
    }
}

impl Frame for FacetFrame {
    fn allows(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(local_name, xsd_names::ANNOTATION)
    }

    fn allows_attribute(&self, local_name: &str, _name_table: &NameTable) -> bool {
        if self.kind == FacetKind::Assertion {
            matches!(local_name, "test" | "xpathDefaultNamespace" | "id")
        } else {
            matches!(local_name, "value" | "fixed" | "id")
        }
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
        Ok(FrameResult::Facet(FacetResult {
            kind: self.kind,
            value: self.value,
            fixed: self.fixed,
            annotation,
            source: self.source,
            xpath_default_namespace: self.xpath_default_namespace,
            ns_snapshot: self.ns_snapshot,
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
