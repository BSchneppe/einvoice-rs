// ============================================================================
// Annotation Frame
// ============================================================================

/// Frame for xs:annotation
pub struct AnnotationFrame {
    id: Option<String>,
    items: Vec<AnnotationItem>,
    source: Option<SourceRef>,
    foreign_attributes: Vec<ForeignAttribute>,
}

impl AnnotationFrame {
    pub fn new(
        attrs: &AttributeMap,
        name_table: &NameTable,
        source: Option<SourceRef>,
    ) -> SchemaResult<Self> {
        let id = attrs
            .get_value_by_name(name_table, "id")
            .map(String::from);

        Ok(Self {
            id,
            items: Vec::new(),
            source,
            foreign_attributes: Vec::new(),
        })
    }
}

impl Frame for AnnotationFrame {
    fn allows(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(local_name, xsd_names::APPINFO | xsd_names::DOCUMENTATION)
    }

    fn allows_attribute(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(local_name, "id")
    }

    fn on_child_start(&mut self, _local_name: &str, _name_table: &NameTable) {}

    fn attach(&mut self, child: FrameResult) -> SchemaResult<()> {
        match child {
            FrameResult::AppInfo(appinfo) => {
                self.items.push(AnnotationItem::AppInfo(appinfo));
            }
            FrameResult::Documentation(doc) => {
                self.items.push(AnnotationItem::Documentation(doc));
            }
            _ => {
                // Ignore other content in annotations
            }
        }
        Ok(())
    }

    fn finish(self: Box<Self>) -> SchemaResult<FrameResult> {
        Ok(FrameResult::Annotation(Annotation {
            id: self.id,
            items: self.items,
            source: self.source,
            attributes: self.foreign_attributes,
        }))
    }

    fn source(&self) -> Option<&SourceRef> {
        self.source.as_ref()
    }

    fn set_foreign_attributes(&mut self, attrs: Vec<ForeignAttribute>) {
        self.foreign_attributes = attrs;
    }
}

// ============================================================================
// AppInfo Frame
// ============================================================================

/// Frame for xs:appinfo element
pub struct AppinfoFrame {
    source_attr: Option<String>,
    start_span: Option<SourceRef>,
    namespaces: NamespaceContextSnapshot,
    foreign_attributes: Vec<ForeignAttribute>,
}

impl AppinfoFrame {
    pub fn new(
        attrs: &AttributeMap,
        name_table: &NameTable,
        source: Option<SourceRef>,
    ) -> SchemaResult<Self> {
        let source_attr = attrs
            .get_value_by_name(name_table, "source")
            .map(String::from);

        Ok(Self {
            source_attr,
            start_span: source,
            namespaces: NamespaceContextSnapshot::default(),
            foreign_attributes: Vec::new(),
        })
    }
}

impl Frame for AppinfoFrame {
    fn allows(&self, _local_name: &str, _name_table: &NameTable) -> bool {
        // Appinfo allows any content (mixed content)
        true
    }

    fn allows_attribute(&self, local_name: &str, _name_table: &NameTable) -> bool {
        // Only 'source' attribute is standard, but allow foreign attrs
        local_name == "source"
    }

    fn on_child_start(&mut self, _local_name: &str, _name_table: &NameTable) {
        // Content is captured as raw XML, not parsed
    }

    fn attach(&mut self, _child: FrameResult) -> SchemaResult<()> {
        // Any children are captured as part of mixed content
        Ok(())
    }

    fn finish(self: Box<Self>) -> SchemaResult<FrameResult> {
        // Create XmlFragment from span
        let fragment = if let Some(ref source) = self.start_span {
            XmlFragment::new(source.doc_id, source.span)
        } else {
            XmlFragment::new(0, crate::parser::location::SourceSpan::new(0, 0))
        };

        let mut appinfo = AppInfoElement::new(fragment, self.namespaces);
        appinfo.source = self.source_attr;
        appinfo.attributes = self.foreign_attributes;
        appinfo.source_ref = self.start_span;

        Ok(FrameResult::AppInfo(appinfo))
    }

    fn source(&self) -> Option<&SourceRef> {
        self.start_span.as_ref()
    }

    fn set_foreign_attributes(&mut self, attrs: Vec<ForeignAttribute>) {
        self.foreign_attributes = attrs;
    }

    fn accepts_text(&self) -> bool {
        true
    }

    fn accepts_foreign_children(&self) -> bool {
        true
    }

    fn on_text(&mut self, _text: &str) {
        // Content is captured via XmlFragment span, not accumulated text
    }

    fn on_cdata(&mut self, _cdata: &str) {
        // Content is captured via XmlFragment span, not accumulated text
    }

    fn set_namespaces(&mut self, namespaces: NamespaceContextSnapshot) {
        self.namespaces = namespaces;
    }
}

// ============================================================================
// Documentation Frame
// ============================================================================

/// Frame for xs:documentation element
pub struct DocumentationFrame {
    source_attr: Option<String>,
    lang: Option<String>,
    start_span: Option<SourceRef>,
    namespaces: NamespaceContextSnapshot,
    foreign_attributes: Vec<ForeignAttribute>,
}

impl DocumentationFrame {
    pub fn new(
        attrs: &AttributeMap,
        name_table: &NameTable,
        source: Option<SourceRef>,
    ) -> SchemaResult<Self> {
        let source_attr = attrs
            .get_value_by_name(name_table, "source")
            .map(String::from);
        let lang = attrs
            .get_value_by_name(name_table, "lang")
            .map(String::from);

        Ok(Self {
            source_attr,
            lang,
            start_span: source,
            namespaces: NamespaceContextSnapshot::default(),
            foreign_attributes: Vec::new(),
        })
    }
}

impl Frame for DocumentationFrame {
    fn allows(&self, _local_name: &str, _name_table: &NameTable) -> bool {
        // Documentation allows any content (mixed content)
        true
    }

    fn allows_attribute(&self, local_name: &str, _name_table: &NameTable) -> bool {
        // 'source' and xml:lang are standard attributes
        matches!(local_name, "source" | "lang")
    }

    fn on_child_start(&mut self, _local_name: &str, _name_table: &NameTable) {
        // Content is captured as raw XML, not parsed
    }

    fn attach(&mut self, _child: FrameResult) -> SchemaResult<()> {
        // Any children are captured as part of mixed content
        Ok(())
    }

    fn finish(self: Box<Self>) -> SchemaResult<FrameResult> {
        // Create XmlFragment from span
        let fragment = if let Some(ref source) = self.start_span {
            XmlFragment::new(source.doc_id, source.span)
        } else {
            XmlFragment::new(0, crate::parser::location::SourceSpan::new(0, 0))
        };

        let mut doc = DocumentationElement::new(fragment, self.namespaces);
        doc.source = self.source_attr;
        doc.lang = self.lang;
        doc.attributes = self.foreign_attributes;
        doc.source_ref = self.start_span;

        Ok(FrameResult::Documentation(doc))
    }

    fn source(&self) -> Option<&SourceRef> {
        self.start_span.as_ref()
    }

    fn set_foreign_attributes(&mut self, attrs: Vec<ForeignAttribute>) {
        self.foreign_attributes = attrs;
    }

    fn accepts_text(&self) -> bool {
        true
    }

    fn accepts_foreign_children(&self) -> bool {
        true
    }

    fn on_text(&mut self, _text: &str) {
        // Content is captured via XmlFragment span, not accumulated text
    }

    fn on_cdata(&mut self, _cdata: &str) {
        // Content is captured via XmlFragment span, not accumulated text
    }

    fn set_namespaces(&mut self, namespaces: NamespaceContextSnapshot) {
        self.namespaces = namespaces;
    }
}

