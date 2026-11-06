// ============================================================================
// Model Group Frame (sequence, choice, all)
// ============================================================================

/// Frame for xs:sequence, xs:choice, xs:all
pub struct ModelGroupFrame {
    compositor: Compositor,
    min_occurs: u32,
    max_occurs: Option<u32>,
    id: Option<String>,
    particles: Vec<ParticleResult>,
    /// True once any non-annotation child has been seen (annotation must come first)
    past_annotation: bool,
    annotation: Option<Annotation>,
    source: Option<SourceRef>,
    foreign_attributes: Vec<ForeignAttribute>,
}

impl ModelGroupFrame {
    pub fn new(
        compositor: Compositor,
        attrs: &AttributeMap,
        name_table: &NameTable,
        source: Option<SourceRef>,
        _ns_snapshot: &NamespaceContextSnapshot,
    ) -> SchemaResult<Self> {
        let min_occurs = parse_min_occurs_attr(attrs, name_table, "minOccurs")?;

        let max_occurs = parse_max_occurs_attr(attrs, name_table, "maxOccurs")?;

        let id = attrs
            .get_value_by_name(name_table, "id")
            .map(String::from);

        Ok(Self {
            compositor,
            min_occurs,
            max_occurs,
            id,
            particles: Vec::new(),
            past_annotation: false,
            annotation: None,
            source,
            foreign_attributes: Vec::new(),
        })
    }
}

impl Frame for ModelGroupFrame {
    fn allows(&self, local_name: &str, _name_table: &NameTable) -> bool {
        // Annotation must come first
        if local_name == xsd_names::ANNOTATION && self.past_annotation {
            return false;
        }
        match self.compositor {
            Compositor::All => matches!(
                local_name,
                xsd_names::ANNOTATION | xsd_names::ELEMENT | xsd_names::ANY | xsd_names::GROUP
            ),
            Compositor::Sequence | Compositor::Choice => matches!(
                local_name,
                xsd_names::ANNOTATION
                    | xsd_names::ELEMENT
                    | xsd_names::GROUP
                    | xsd_names::SEQUENCE
                    | xsd_names::CHOICE
                    | xsd_names::ANY
            ),
        }
    }

    fn allows_attribute(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(local_name, "minOccurs" | "maxOccurs" | "id")
    }

    fn on_child_start(&mut self, local_name: &str, _name_table: &NameTable) {
        if local_name != xsd_names::ANNOTATION {
            self.past_annotation = true;
        }
    }

    fn attach(&mut self, child: FrameResult) -> SchemaResult<()> {
        match child {
            FrameResult::Annotation(ann) => {
                self.annotation = Some(ann);
            }
            FrameResult::Element(elem) => {
                let min_occurs = elem.min_occurs;
                let max_occurs = elem.max_occurs;
                let source = elem.source.clone();
                self.particles.push(ParticleResult {
                    term: ParticleTerm::Element(elem),
                    min_occurs,
                    max_occurs,
                    source,
                });
            }
            FrameResult::Particle(particle) => {
                self.particles.push(particle);
            }
            FrameResult::Wildcard(wc) => {
                let source = wc.source.clone();
                self.particles.push(ParticleResult {
                    term: ParticleTerm::Any(wc),
                    min_occurs: 1,
                    max_occurs: Some(1),
                    source,
                });
            }
            FrameResult::Group(GroupFrameResult::Model(mg)) => {
                let min_occurs = mg.min_occurs;
                let max_occurs = mg.max_occurs;
                let source = mg.source.clone();
                self.particles.push(ParticleResult {
                    term: ParticleTerm::Group(*mg),
                    min_occurs,
                    max_occurs,
                    source,
                });
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
        Ok(FrameResult::Particle(ParticleResult {
            term: ParticleTerm::Group(ModelGroupDefResult {
                name: None,
                ref_name: None,
                compositor: Some(self.compositor),
                particles: self.particles,
                min_occurs: self.min_occurs,
                max_occurs: self.max_occurs,
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

// ============================================================================
// Group Frame (named model group)
// ============================================================================

/// Frame for xs:group (named or reference)
pub struct GroupFrame {
    name: Option<NameId>,
    ref_name: Option<QNameRef>,
    min_occurs: u32,
    max_occurs: Option<u32>,
    id: Option<String>,
    compositor: Option<Compositor>,
    particles: Vec<ParticleResult>,
    /// True once any non-annotation child has been seen (annotation must come first)
    past_annotation: bool,
    annotation: Option<Annotation>,
    source: Option<SourceRef>,
    foreign_attributes: Vec<ForeignAttribute>,
}

impl GroupFrame {
    pub fn new(
        attrs: &AttributeMap,
        name_table: &NameTable,
        source: Option<SourceRef>,
        ns_snapshot: &NamespaceContextSnapshot,
    ) -> SchemaResult<Self> {
        let name = attrs
            .get_value_by_name(name_table, "name")
            .and_then(|s| name_table.get(s));

        let ref_name = attrs
            .get_value_by_name(name_table, "ref")
            .map(|s| parse_qname_ref(s, name_table, ns_snapshot))
            .transpose()?;

        let min_occurs = parse_min_occurs_attr(attrs, name_table, "minOccurs")?;

        let max_occurs = parse_max_occurs_attr(attrs, name_table, "maxOccurs")?;

        let id = attrs
            .get_value_by_name(name_table, "id")
            .map(String::from);

        Ok(Self {
            name,
            ref_name,
            min_occurs,
            max_occurs,
            id,
            compositor: None,
            particles: Vec::new(),
            past_annotation: false,
            annotation: None,
            source,
            foreign_attributes: Vec::new(),
        })
    }
}

impl Frame for GroupFrame {
    fn allows(&self, local_name: &str, _name_table: &NameTable) -> bool {
        // Annotation must come first (before compositor)
        if local_name == xsd_names::ANNOTATION && self.past_annotation {
            return false;
        }
        // xs:group ref-form content model: (annotation?). The compositor is
        // only allowed in the named (top-level) form.
        if self.ref_name.is_some() {
            return local_name == xsd_names::ANNOTATION;
        }
        // xs:group content model: (annotation?, (all | choice | sequence)?) — at most one compositor
        if self.compositor.is_some()
            && matches!(
                local_name,
                xsd_names::SEQUENCE | xsd_names::CHOICE | xsd_names::ALL
            )
        {
            return false;
        }
        matches!(
            local_name,
            xsd_names::ANNOTATION | xsd_names::SEQUENCE | xsd_names::CHOICE | xsd_names::ALL
        )
    }

    fn allows_attribute(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(
            local_name,
            "name" | "ref" | "minOccurs" | "maxOccurs" | "id"
        )
    }

    fn on_child_start(&mut self, local_name: &str, _name_table: &NameTable) {
        if local_name != xsd_names::ANNOTATION {
            self.past_annotation = true;
        }
    }

    fn attach(&mut self, child: FrameResult) -> SchemaResult<()> {
        match child {
            FrameResult::Annotation(ann) => {
                self.annotation = Some(ann);
            }
            FrameResult::Particle(ParticleResult {
                term: ParticleTerm::Group(mg),
                min_occurs,
                max_occurs,
                ..
            }) => {
                // For named (top-level) group definitions, the compositor child
                // must not have minOccurs/maxOccurs (cos-all-limited / src-group)
                // Safety net: reject if a compositor was already attached
                if self.compositor.is_some() {
                    return Err(SchemaError::structural(
                        "src-group",
                        "Group definition must have exactly one compositor child (sequence, choice, or all)",
                        None,
                    ));
                }
                // For named (top-level) group definitions, the compositor child
                // must not have minOccurs/maxOccurs (cos-all-limited / src-group)
                if self.name.is_some() && (min_occurs != 1 || max_occurs != Some(1)) {
                    return Err(SchemaError::structural(
                        "src-group",
                        "Compositor in top-level group definition cannot have minOccurs/maxOccurs",
                        None,
                    ));
                }
                self.compositor = mg.compositor;
                self.particles = mg.particles;
            }
            FrameResult::Skip => {}
            _ => {}
        }
        Ok(())
    }

    fn finish(self: Box<Self>) -> SchemaResult<FrameResult> {
        // Named group definitions must have exactly one compositor child
        if self.name.is_some() && self.ref_name.is_none() && self.compositor.is_none() {
            return Err(SchemaError::structural(
                "src-group",
                "Named group definition must contain a compositor (sequence, choice, or all)",
                None,
            ));
        }
        let annotation = merge_foreign_attributes(
            self.annotation,
            self.foreign_attributes,
            self.source.clone(),
        );
        Ok(FrameResult::Group(GroupFrameResult::Model(Box::new(ModelGroupDefResult {
            name: self.name,
            ref_name: self.ref_name,
            compositor: self.compositor,
            particles: self.particles,
            min_occurs: self.min_occurs,
            max_occurs: self.max_occurs,
            id: self.id,
            annotation,
            source: self.source,
        }))))
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
// Attribute Group Frame
// ============================================================================

/// Frame for xs:attributeGroup
pub struct AttributeGroupFrame {
    name: Option<NameId>,
    ref_name: Option<QNameRef>,
    id: Option<String>,
    attributes: Vec<AttributeUseResult>,
    attribute_groups: Vec<QNameRef>,
    attribute_wildcard: Option<WildcardResult>,
    /// True once any non-annotation child has been seen (annotation must come first)
    past_annotation: bool,
    annotation: Option<Annotation>,
    source: Option<SourceRef>,
    foreign_attributes: Vec<ForeignAttribute>,
}

impl AttributeGroupFrame {
    pub fn new(
        attrs: &AttributeMap,
        name_table: &NameTable,
        source: Option<SourceRef>,
        ns_snapshot: &NamespaceContextSnapshot,
    ) -> SchemaResult<Self> {
        let name = attrs
            .get_value_by_name(name_table, "name")
            .and_then(|s| name_table.get(s));

        let ref_name = attrs
            .get_value_by_name(name_table, "ref")
            .map(|s| parse_qname_ref(s, name_table, ns_snapshot))
            .transpose()?;

        let id = attrs
            .get_value_by_name(name_table, "id")
            .map(String::from);

        Ok(Self {
            name,
            ref_name,
            id,
            attributes: Vec::new(),
            attribute_groups: Vec::new(),
            attribute_wildcard: None,
            past_annotation: false,
            annotation: None,
            source,
            foreign_attributes: Vec::new(),
        })
    }
}

impl Frame for AttributeGroupFrame {
    fn allows(&self, local_name: &str, _name_table: &NameTable) -> bool {
        // Annotation must come first
        if local_name == xsd_names::ANNOTATION && self.past_annotation {
            return false;
        }
        // When `ref` is present, only annotation is allowed (ref-form has no
        // attribute or wildcard children — XML Representation of <attributeGroup>).
        if self.ref_name.is_some() {
            return local_name == xsd_names::ANNOTATION;
        }
        matches!(
            local_name,
            xsd_names::ANNOTATION
                | xsd_names::ATTRIBUTE
                | xsd_names::ATTRIBUTE_GROUP
                | xsd_names::ANY_ATTRIBUTE
        )
    }

    fn allows_attribute(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(local_name, "name" | "ref" | "id")
    }

    fn on_child_start(&mut self, local_name: &str, _name_table: &NameTable) {
        if local_name != xsd_names::ANNOTATION {
            self.past_annotation = true;
        }
    }

    fn attach(&mut self, child: FrameResult) -> SchemaResult<()> {
        match child {
            FrameResult::Annotation(ann) => {
                self.annotation = Some(ann);
            }
            FrameResult::Attribute(attr) => {
                let use_kind = match attr.use_kind.as_deref() {
                    Some("required") => AttributeUseKind::Required,
                    Some("prohibited") => AttributeUseKind::Prohibited,
                    _ => AttributeUseKind::Optional,
                };
                self.attributes.push(AttributeUseResult {
                    attribute: attr,
                    use_kind,
                });
            }
            FrameResult::Group(GroupFrameResult::Attribute(ag)) => {
                if let Some(ref_name) = ag.ref_name {
                    self.attribute_groups.push(ref_name);
                }
            }
            FrameResult::Wildcard(wc) => {
                self.attribute_wildcard = Some(wc);
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
        Ok(FrameResult::Group(GroupFrameResult::Attribute(Box::new(AttributeGroupDefResult {
            name: self.name,
            ref_name: self.ref_name,
            attributes: self.attributes,
            attribute_groups: self.attribute_groups,
            attribute_wildcard: self.attribute_wildcard,
            id: self.id,
            annotation,
            source: self.source,
        }))))
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

