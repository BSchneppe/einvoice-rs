// ============================================================================
// Element Frame
// ============================================================================

/// Parsing phase for element
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum ElementPhase {
    Annotation,
    Type,
    Identity,
    Done,
}

/// Frame for xs:element
pub struct ElementFrame {
    phase: ElementPhase,
    name: Option<NameId>,
    ref_name: Option<QNameRef>,
    target_namespace: Option<NameId>,
    type_ref: Option<TypeRefResult>,
    inline_type: Option<Box<TypeFrameResult>>,
    substitution_group: Vec<QNameRef>,
    default_value: Option<String>,
    fixed_value: Option<String>,
    nillable: bool,
    is_abstract: bool,
    min_occurs: u32,
    max_occurs: Option<u32>,
    block: Option<DerivationSet>,
    final_derivation: Option<DerivationSet>,
    form: Option<String>,
    id: Option<String>,
    alternatives: Vec<AlternativeResult>,
    identity_constraints: Vec<IdentityResult>,
    /// XSD 1.1: pending identity constraint references (@ref)
    identity_constraint_refs: Vec<IdentityRefResult>,
    annotation: Option<Annotation>,
    source: Option<SourceRef>,
    foreign_attributes: Vec<ForeignAttribute>,
}

impl ElementFrame {
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

        let target_namespace = attrs
            .get_value_by_name(name_table, "targetNamespace")
            .map(|s| name_table.add(s));

        let type_ref = attrs
            .get_value_by_name(name_table, "type")
            .map(|s| parse_qname_ref(s, name_table, ns_snapshot))
            .transpose()?
            .map(TypeRefResult::QName);

        let substitution_group = attrs
            .get_value_by_name(name_table, "substitutionGroup")
            .map(|s| parse_qname_list(s, name_table, ns_snapshot))
            .transpose()?
            .unwrap_or_default();

        let default_value = attrs
            .get_value_by_name(name_table, "default")
            .map(String::from);

        let fixed_value = attrs
            .get_value_by_name(name_table, "fixed")
            .map(String::from);

        let nillable = parse_bool_attr_default(attrs, name_table, "nillable", false)?;

        let is_abstract = parse_bool_attr_default(attrs, name_table, "abstract", false)?;

        let min_occurs = parse_min_occurs_attr(attrs, name_table, "minOccurs")?;

        let max_occurs = parse_max_occurs_attr(attrs, name_table, "maxOccurs")?;

        let block = parse_derivation_set_opt(
            attrs.get_value_by_name(name_table, "block"),
        )?;

        let final_derivation = parse_derivation_set_opt(
            attrs.get_value_by_name(name_table, "final"),
        )?;

        let form = attrs
            .get_value_by_name(name_table, "form")
            .map(String::from);

        let id = attrs
            .get_value_by_name(name_table, "id")
            .map(String::from);

        Ok(Self {
            phase: ElementPhase::Annotation,
            name,
            ref_name,
            target_namespace,
            type_ref,
            inline_type: None,
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
            alternatives: Vec::new(),
            identity_constraints: Vec::new(),
            identity_constraint_refs: Vec::new(),
            annotation: None,
            source,
            foreign_attributes: Vec::new(),
        })
    }
}

impl Frame for ElementFrame {
    fn allows(&self, local_name: &str, _name_table: &NameTable) -> bool {
        #[cfg(feature = "xsd11")]
        let is_xsd11_element = matches!(local_name, xsd_names::ALTERNATIVE);
        #[cfg(not(feature = "xsd11"))]
        let is_xsd11_element = false;

        // src-element clause 3: when `ref` is present, only annotation is allowed
        // (no inline simpleType/complexType, no identity constraints).
        if self.ref_name.is_some() {
            return local_name == xsd_names::ANNOTATION
                && self.phase == ElementPhase::Annotation;
        }

        match self.phase {
            ElementPhase::Annotation => matches!(
                local_name,
                xsd_names::ANNOTATION
                    | xsd_names::SIMPLE_TYPE
                    | xsd_names::COMPLEX_TYPE
                    | xsd_names::KEY
                    | xsd_names::KEYREF
                    | xsd_names::UNIQUE
            ) || is_xsd11_element,
            ElementPhase::Type => matches!(
                local_name,
                xsd_names::SIMPLE_TYPE
                    | xsd_names::COMPLEX_TYPE
                    | xsd_names::KEY
                    | xsd_names::KEYREF
                    | xsd_names::UNIQUE
            ) || is_xsd11_element,
            ElementPhase::Identity => {
                matches!(
                    local_name,
                    xsd_names::KEY | xsd_names::KEYREF | xsd_names::UNIQUE
                ) || is_xsd11_element
            }
            ElementPhase::Done => false,
        }
    }

    fn allows_attribute(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(
            local_name,
            "name"
                | "ref"
                | "targetNamespace"
                | "type"
                | "substitutionGroup"
                | "default"
                | "fixed"
                | "nillable"
                | "abstract"
                | "minOccurs"
                | "maxOccurs"
                | "block"
                | "final"
                | "form"
                | "id"
        )
    }

    fn on_child_start(&mut self, local_name: &str, _name_table: &NameTable) {
        match local_name {
            xsd_names::ANNOTATION => {
                self.phase = ElementPhase::Type;
            }
            xsd_names::SIMPLE_TYPE | xsd_names::COMPLEX_TYPE => {
                self.phase = ElementPhase::Identity;
            }
            xsd_names::ALTERNATIVE => {
                self.phase = ElementPhase::Identity;
            }
            xsd_names::KEY | xsd_names::KEYREF | xsd_names::UNIQUE => {
                self.phase = ElementPhase::Identity;
            }
            _ => {}
        }
    }

    fn attach(&mut self, child: FrameResult) -> SchemaResult<()> {
        match child {
            FrameResult::Annotation(ann) => {
                self.annotation = Some(ann);
            }
            FrameResult::Type(t) => {
                self.inline_type = Some(Box::new(t));
            }
            FrameResult::Alternative(alt) => {
                self.alternatives.push(alt);
            }
            FrameResult::Identity(ic) => {
                self.identity_constraints.push(ic);
            }
            FrameResult::IdentityRef(ic_ref) => {
                self.identity_constraint_refs.push(ic_ref);
            }
            FrameResult::Skip => {}
            _ => {}
        }
        Ok(())
    }

    fn finish(self: Box<Self>) -> SchemaResult<FrameResult> {
        // src-element.1: type and inline type are mutually exclusive
        if self.type_ref.is_some() && self.inline_type.is_some() {
            return Err(SchemaError::structural(
                "src-element",
                "Element cannot have both 'type' attribute and inline type definition",
                None,
            ));
        }

        let annotation = merge_foreign_attributes(
            self.annotation,
            self.foreign_attributes,
            self.source.clone(),
        );
        Ok(FrameResult::Element(ElementFrameResult {
            name: self.name,
            ref_name: self.ref_name,
            target_namespace: self.target_namespace,
            type_ref: self.type_ref,
            inline_type: self.inline_type,
            substitution_group: self.substitution_group,
            default_value: self.default_value,
            fixed_value: self.fixed_value,
            nillable: self.nillable,
            is_abstract: self.is_abstract,
            min_occurs: self.min_occurs,
            max_occurs: self.max_occurs,
            block: self.block,
            final_derivation: self.final_derivation,
            form: self.form,
            id: self.id,
            alternatives: self.alternatives,
            identity_constraints: self.identity_constraints,
            identity_constraint_refs: self.identity_constraint_refs,
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
// Attribute Frame
// ============================================================================

/// Frame for xs:attribute
pub struct AttributeFrame {
    name: Option<NameId>,
    ref_name: Option<QNameRef>,
    target_namespace: Option<NameId>,
    type_ref: Option<TypeRefResult>,
    inline_type: Option<Box<SimpleTypeResult>>,
    default_value: Option<String>,
    fixed_value: Option<String>,
    use_kind: Option<String>,
    form: Option<String>,
    inheritable: Option<bool>,
    id: Option<String>,
    annotation: Option<Annotation>,
    /// True once a non-annotation child has been seen — annotation must come first.
    past_annotation: bool,
    source: Option<SourceRef>,
    foreign_attributes: Vec<ForeignAttribute>,
}

impl AttributeFrame {
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

        let target_namespace = attrs
            .get_value_by_name(name_table, "targetNamespace")
            .map(|s| name_table.add(s));

        let type_ref = attrs
            .get_value_by_name(name_table, "type")
            .map(|s| parse_qname_ref(s, name_table, ns_snapshot))
            .transpose()?
            .map(TypeRefResult::QName);

        let default_value = attrs
            .get_value_by_name(name_table, "default")
            .map(String::from);

        let fixed_value = attrs
            .get_value_by_name(name_table, "fixed")
            .map(String::from);

        validate_attr_value(attrs, name_table, "use", parse_use)?;
        let use_kind = attrs
            .get_value_by_name(name_table, "use")
            .map(String::from);

        validate_attr_value(attrs, name_table, "form", parse_form)?;
        let form = attrs
            .get_value_by_name(name_table, "form")
            .map(String::from);

        let inheritable = parse_optional_bool_attr(attrs, name_table, "inheritable")?;

        let id = attrs
            .get_value_by_name(name_table, "id")
            .map(String::from);

        Ok(Self {
            name,
            ref_name,
            target_namespace,
            type_ref,
            inline_type: None,
            default_value,
            fixed_value,
            use_kind,
            form,
            inheritable,
            id,
            annotation: None,
            past_annotation: false,
            source,
            foreign_attributes: Vec::new(),
        })
    }
}

impl Frame for AttributeFrame {
    fn allows(&self, local_name: &str, _name_table: &NameTable) -> bool {
        // Annotation must precede the inline simpleType; only one inline
        // simpleType is permitted (xs:attribute content model:
        // (annotation?, simpleType?)).
        if local_name == xsd_names::ANNOTATION && self.past_annotation {
            return false;
        }
        if local_name == xsd_names::SIMPLE_TYPE && self.inline_type.is_some() {
            return false;
        }
        matches!(local_name, xsd_names::ANNOTATION | xsd_names::SIMPLE_TYPE)
    }

    fn allows_attribute(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(
            local_name,
            "name"
                | "ref"
                | "targetNamespace"
                | "type"
                | "default"
                | "fixed"
                | "use"
                | "form"
                | "inheritable"
                | "id"
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
            FrameResult::Type(TypeFrameResult::Simple(st)) => {
                self.inline_type = Some(st);
            }
            FrameResult::Skip => {}
            _ => {}
        }
        Ok(())
    }

    fn finish(self: Box<Self>) -> SchemaResult<FrameResult> {
        // src-attribute.3.2: ref + <simpleType> are mutually exclusive
        if self.ref_name.is_some() && self.inline_type.is_some() {
            return Err(SchemaError::structural(
                "src-attribute",
                "Attribute reference cannot have inline simpleType",
                None,
            ));
        }
        // src-attribute.4: type and inline simpleType are mutually exclusive
        if self.type_ref.is_some() && self.inline_type.is_some() {
            return Err(SchemaError::structural(
                "src-attribute",
                "Attribute cannot have both 'type' attribute and inline simpleType",
                None,
            ));
        }

        let annotation = merge_foreign_attributes(
            self.annotation,
            self.foreign_attributes,
            self.source.clone(),
        );
        Ok(FrameResult::Attribute(AttributeFrameResult {
            name: self.name,
            ref_name: self.ref_name,
            target_namespace: self.target_namespace,
            type_ref: self.type_ref,
            inline_type: self.inline_type,
            default_value: self.default_value,
            fixed_value: self.fixed_value,
            use_kind: self.use_kind,
            form: self.form,
            inheritable: self.inheritable,
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

