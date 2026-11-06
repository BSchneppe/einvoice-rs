// ============================================================================
// Wildcard Frames (any, anyAttribute)
// ============================================================================

/// Frame for xs:any
pub struct AnyFrame {
    namespace: WildcardNamespace,
    process_contents: ProcessContents,
    not_namespace: Vec<NamespaceToken>,
    not_qname: Vec<NotQNameItem>,
    min_occurs: u32,
    max_occurs: Option<u32>,
    id: Option<String>,
    annotation: Option<Annotation>,
    source: Option<SourceRef>,
    foreign_attributes: Vec<ForeignAttribute>,
}

impl AnyFrame {
    pub fn new(
        attrs: &AttributeMap,
        name_table: &NameTable,
        source: Option<SourceRef>,
        #[cfg(feature = "xsd11")] ns_snapshot: &NamespaceContextSnapshot,
    ) -> SchemaResult<Self> {
        let has_namespace = attrs.get_value_by_name(name_table, "namespace").is_some();
        let has_not_namespace = attrs.get_value_by_name(name_table, "notNamespace").is_some();

        #[cfg(feature = "xsd11")]
        if has_namespace && has_not_namespace {
            return Err(SchemaError::structural(
                "src-wildcard",
                "Attributes 'namespace' and 'notNamespace' are mutually exclusive on xs:any".to_string(),
                None,
            ));
        }

        let namespace = parse_namespace_constraint(
            attrs.get_value_by_name(name_table, "namespace"),
            name_table,
        )?;

        let process_contents =
            parse_process_contents_attr(attrs, name_table, "processContents")?;

        #[cfg(feature = "xsd11")]
        let not_namespace = parse_not_namespace(
            attrs.get_value_by_name(name_table, "notNamespace"),
            name_table,
        );
        #[cfg(not(feature = "xsd11"))]
        let not_namespace = {
            let _ = has_namespace;
            let _ = has_not_namespace;
            Vec::new()
        };

        #[cfg(feature = "xsd11")]
        let not_qname = parse_not_qname(
            attrs.get_value_by_name(name_table, "notQName"),
            name_table,
            ns_snapshot,
            true, // is_element_wildcard
        )?;
        #[cfg(not(feature = "xsd11"))]
        let not_qname = Vec::new();

        let min_occurs = parse_min_occurs_attr(attrs, name_table, "minOccurs")?;

        let max_occurs = parse_max_occurs_attr(attrs, name_table, "maxOccurs")?;

        let id = attrs
            .get_value_by_name(name_table, "id")
            .map(String::from);

        Ok(Self {
            namespace,
            process_contents,
            not_namespace,
            not_qname,
            min_occurs,
            max_occurs,
            id,
            annotation: None,
            source,
            foreign_attributes: Vec::new(),
        })
    }
}

impl Frame for AnyFrame {
    fn allows(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(local_name, xsd_names::ANNOTATION)
    }

    fn allows_attribute(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(
            local_name,
            "namespace"
                | "processContents"
                | "notNamespace"
                | "notQName"
                | "minOccurs"
                | "maxOccurs"
                | "id"
        )
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
        Ok(FrameResult::Particle(ParticleResult {
            term: ParticleTerm::Any(WildcardResult {
                namespace: self.namespace,
                process_contents: self.process_contents,
                not_namespace: self.not_namespace,
                not_qname: self.not_qname,
                id: self.id,
                annotation,
                source: self.source.clone(),
            }),
            min_occurs: self.min_occurs,
            max_occurs: self.max_occurs,
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

/// Frame for xs:anyAttribute
pub struct AnyAttributeFrame {
    namespace: WildcardNamespace,
    process_contents: ProcessContents,
    not_namespace: Vec<NamespaceToken>,
    not_qname: Vec<NotQNameItem>,
    id: Option<String>,
    annotation: Option<Annotation>,
    source: Option<SourceRef>,
    foreign_attributes: Vec<ForeignAttribute>,
}

impl AnyAttributeFrame {
    pub fn new(
        attrs: &AttributeMap,
        name_table: &NameTable,
        source: Option<SourceRef>,
        #[cfg(feature = "xsd11")] ns_snapshot: &NamespaceContextSnapshot,
    ) -> SchemaResult<Self> {
        let has_namespace = attrs.get_value_by_name(name_table, "namespace").is_some();
        let has_not_namespace = attrs.get_value_by_name(name_table, "notNamespace").is_some();

        #[cfg(feature = "xsd11")]
        if has_namespace && has_not_namespace {
            return Err(SchemaError::structural(
                "src-wildcard",
                "Attributes 'namespace' and 'notNamespace' are mutually exclusive on xs:anyAttribute".to_string(),
                None,
            ));
        }

        let namespace = parse_namespace_constraint(
            attrs.get_value_by_name(name_table, "namespace"),
            name_table,
        )?;

        let process_contents =
            parse_process_contents_attr(attrs, name_table, "processContents")?;

        #[cfg(feature = "xsd11")]
        let not_namespace = parse_not_namespace(
            attrs.get_value_by_name(name_table, "notNamespace"),
            name_table,
        );
        #[cfg(not(feature = "xsd11"))]
        let not_namespace = {
            let _ = has_namespace;
            let _ = has_not_namespace;
            Vec::new()
        };

        #[cfg(feature = "xsd11")]
        let not_qname = parse_not_qname(
            attrs.get_value_by_name(name_table, "notQName"),
            name_table,
            ns_snapshot,
            false, // is_element_wildcard = false (attribute wildcard)
        )?;
        #[cfg(not(feature = "xsd11"))]
        let not_qname = Vec::new();

        let id = attrs
            .get_value_by_name(name_table, "id")
            .map(String::from);

        Ok(Self {
            namespace,
            process_contents,
            not_namespace,
            not_qname,
            id,
            annotation: None,
            source,
            foreign_attributes: Vec::new(),
        })
    }
}

impl Frame for AnyAttributeFrame {
    fn allows(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(local_name, xsd_names::ANNOTATION)
    }

    fn allows_attribute(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(
            local_name,
            "namespace" | "processContents" | "notNamespace" | "notQName" | "id"
        )
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
        Ok(FrameResult::Wildcard(WildcardResult {
            namespace: self.namespace,
            process_contents: self.process_contents,
            not_namespace: self.not_namespace,
            not_qname: self.not_qname,
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

