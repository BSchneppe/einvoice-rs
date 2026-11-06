// ============================================================================
// Open Content Frames (XSD 1.1)
// ============================================================================

/// Reject `minOccurs` / `maxOccurs` on the wildcard child of `xs:openContent`
/// or `xs:defaultOpenContent`.
///
/// The XSD 1.1 schema for schemas declares these two parents as containing
/// a restricted wildcard type with `minOccurs` and `maxOccurs` **prohibited**
/// (W3C Bugzilla 15618; saxon open048 test). When the parser sees default
/// values `(1, Some(1))` we assume the attributes were absent; any other
/// value means the schema author explicitly wrote a disallowed attribute.
fn validate_open_content_wildcard_occurs(
    min_occurs: u32,
    max_occurs: Option<u32>,
    _source: Option<&SourceRef>,
) -> SchemaResult<()> {
    if min_occurs != 1 || max_occurs != Some(1) {
        return Err(SchemaError::structural(
            "src-openContent",
            "xs:any inside xs:openContent/xs:defaultOpenContent must not \
             carry 'minOccurs' or 'maxOccurs' (XSD 1.1 structures schema, \
             see W3C Bugzilla 15618)"
                .to_string(),
            None,
        ));
    }
    Ok(())
}

/// Frame for xs:openContent
pub struct OpenContentFrame {
    mode: OpenContentMode,
    wildcard: Option<WildcardResult>,
    id: Option<String>,
    annotation: Option<Annotation>,
    source: Option<SourceRef>,
    foreign_attributes: Vec<ForeignAttribute>,
}

impl OpenContentFrame {
    pub fn new(
        attrs: &AttributeMap,
        name_table: &NameTable,
        source: Option<SourceRef>,
    ) -> SchemaResult<Self> {
        let mode = parse_open_content_mode_attr(attrs, name_table, "mode")?;

        let id = attrs
            .get_value_by_name(name_table, "id")
            .map(String::from);

        Ok(Self {
            mode,
            wildcard: None,
            id,
            annotation: None,
            source,
            foreign_attributes: Vec::new(),
        })
    }
}

impl Frame for OpenContentFrame {
    fn allows(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(local_name, xsd_names::ANNOTATION | xsd_names::ANY)
    }

    fn allows_attribute(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(local_name, "id" | "mode")
    }

    fn on_child_start(&mut self, _local_name: &str, _name_table: &NameTable) {}

    fn attach(&mut self, child: FrameResult) -> SchemaResult<()> {
        match child {
            FrameResult::Annotation(ann) => {
                self.annotation = Some(ann);
            }
            FrameResult::Particle(particle) => {
                if let ParticleTerm::Any(wc) = particle.term {
                    // XSD 1.1 §3.4.2 (schema for schemas): the xs:any child of
                    // xs:openContent uses a restricted wildcard type that
                    // PROHIBITS minOccurs and maxOccurs. See W3C Bugzilla
                    // 15618 and the saxon open048 test. Reject any wildcard
                    // whose occurrence range differs from the default [1,1].
                    validate_open_content_wildcard_occurs(
                        particle.min_occurs,
                        particle.max_occurs,
                        particle.source.as_ref(),
                    )?;
                    self.wildcard = Some(wc);
                }
            }
            FrameResult::Skip => {}
            _ => {}
        }
        Ok(())
    }

    fn finish(self: Box<Self>) -> SchemaResult<FrameResult> {
        // §3.4.2 / W3C Bugzilla 7069: when xs:openContent declares mode="none"
        // the wildcard child is meaningless and the schema for schemas
        // disallows it. Report the schema as structurally invalid.
        if self.mode == OpenContentMode::None && self.wildcard.is_some() {
            return Err(SchemaError::structural(
                "src-openContent",
                "xs:openContent with mode='none' must not contain an xs:any \
                 child wildcard (W3C Bugzilla 7069)"
                    .to_string(),
                None,
            ));
        }
        // Structures 1.1 §3.4.3 Complex Type Definition Representation OK,
        // clause 3: "If <openContent> is present and has mode ≠ 'none', then
        // there must be an <any> among the [children] of <openContent>."
        if self.mode != OpenContentMode::None && self.wildcard.is_none() {
            let mode = match self.mode {
                OpenContentMode::Suffix => "suffix",
                _ => "interleave",
            };
            return Err(SchemaError::structural(
                "src-ct",
                format!(
                    "xs:openContent with mode='{mode}' must contain an xs:any child \
                     (Complex Type Definition Representation OK, clause 3)"
                ),
                None,
            ));
        }
        let annotation = merge_foreign_attributes(
            self.annotation,
            self.foreign_attributes,
            self.source.clone(),
        );
        Ok(FrameResult::OpenContent(OpenContentResult {
            mode: self.mode,
            wildcard: self.wildcard,
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

/// Frame for xs:defaultOpenContent
pub struct DefaultOpenContentFrame {
    mode: OpenContentMode,
    applies_to_empty: bool,
    wildcard: Option<WildcardResult>,
    id: Option<String>,
    annotation: Option<Annotation>,
    source: Option<SourceRef>,
    foreign_attributes: Vec<ForeignAttribute>,
}

impl DefaultOpenContentFrame {
    pub fn new(
        attrs: &AttributeMap,
        name_table: &NameTable,
        source: Option<SourceRef>,
    ) -> SchemaResult<Self> {
        let mode = parse_open_content_mode_attr(attrs, name_table, "mode")?;

        let applies_to_empty =
            parse_bool_attr_default(attrs, name_table, "appliesToEmpty", false)?;

        let id = attrs
            .get_value_by_name(name_table, "id")
            .map(String::from);

        Ok(Self {
            mode,
            applies_to_empty,
            wildcard: None,
            id,
            annotation: None,
            source,
            foreign_attributes: Vec::new(),
        })
    }
}

impl Frame for DefaultOpenContentFrame {
    fn allows(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(local_name, xsd_names::ANNOTATION | xsd_names::ANY)
    }

    fn allows_attribute(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(local_name, "id" | "mode" | "appliesToEmpty")
    }

    fn on_child_start(&mut self, _local_name: &str, _name_table: &NameTable) {}

    fn attach(&mut self, child: FrameResult) -> SchemaResult<()> {
        match child {
            FrameResult::Annotation(ann) => {
                self.annotation = Some(ann);
            }
            FrameResult::Particle(particle) => {
                if let ParticleTerm::Any(wc) = particle.term {
                    // Same restriction applies to xs:defaultOpenContent — the
                    // wildcard child prohibits minOccurs / maxOccurs.
                    validate_open_content_wildcard_occurs(
                        particle.min_occurs,
                        particle.max_occurs,
                        particle.source.as_ref(),
                    )?;
                    self.wildcard = Some(wc);
                }
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
        Ok(FrameResult::DefaultOpenContent(DefaultOpenContentResult {
            mode: self.mode,
            applies_to_empty: self.applies_to_empty,
            wildcard: self.wildcard,
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

