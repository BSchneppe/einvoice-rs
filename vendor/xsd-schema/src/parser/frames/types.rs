// ============================================================================
// Simple Type Frame
// ============================================================================

/// Parsing phase for simpleType
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SimpleTypePhase {
    /// Expecting annotation
    Annotation,
    /// Expecting restriction, list, or union
    Derivation,
    /// Done
    Done,
}

/// Frame for xs:simpleType element
pub struct SimpleTypeFrame {
    phase: SimpleTypePhase,
    name: Option<NameId>,
    final_derivation: Option<DerivationSet>,
    id: Option<String>,
    derivation_id: Option<String>,
    variety: Option<SimpleTypeVariety>,
    base_type: Option<TypeRefResult>,
    item_type: Option<TypeRefResult>,
    member_types: Vec<TypeRefResult>,
    facets: FacetSet,
    annotation: Option<Annotation>,
    source: Option<SourceRef>,
    foreign_attributes: Vec<ForeignAttribute>,
}

impl SimpleTypeFrame {
    pub fn new(
        attrs: &AttributeMap,
        name_table: &NameTable,
        source: Option<SourceRef>,
    ) -> SchemaResult<Self> {
        let name = attrs
            .get_value_by_name(name_table, "name")
            .and_then(|s| name_table.get(s));

        let final_derivation = parse_derivation_set_opt(
            attrs.get_value_by_name(name_table, "final"),
        )?;

        let id = attrs
            .get_value_by_name(name_table, "id")
            .map(String::from);

        Ok(Self {
            phase: SimpleTypePhase::Annotation,
            name,
            final_derivation,
            id,
            derivation_id: None,
            variety: None,
            base_type: None,
            item_type: None,
            member_types: Vec::new(),
            facets: FacetSet::new(),
            annotation: None,
            source,
            foreign_attributes: Vec::new(),
        })
    }
}

impl Frame for SimpleTypeFrame {
    fn allows(&self, local_name: &str, _name_table: &NameTable) -> bool {
        match self.phase {
            SimpleTypePhase::Annotation => matches!(
                local_name,
                xsd_names::ANNOTATION | xsd_names::RESTRICTION | xsd_names::LIST | xsd_names::UNION
            ),
            SimpleTypePhase::Derivation => matches!(
                local_name,
                xsd_names::RESTRICTION | xsd_names::LIST | xsd_names::UNION
            ),
            SimpleTypePhase::Done => false,
        }
    }

    fn allows_attribute(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(local_name, "name" | "final" | "id")
    }

    fn on_child_start(&mut self, local_name: &str, _name_table: &NameTable) {
        match local_name {
            xsd_names::ANNOTATION => {
                self.phase = SimpleTypePhase::Derivation;
            }
            xsd_names::RESTRICTION | xsd_names::LIST | xsd_names::UNION => {
                self.phase = SimpleTypePhase::Done;
            }
            _ => {}
        }
    }

    fn attach(&mut self, child: FrameResult) -> SchemaResult<()> {
        match child {
            FrameResult::Annotation(ann) => {
                self.annotation = Some(ann);
            }
            FrameResult::Type(TypeFrameResult::Simple(st)) => {
                // Inline type from restriction/list/union
                self.variety = Some(st.variety);
                self.base_type = st.base_type;
                self.item_type = st.item_type;
                self.member_types = st.member_types;
                self.facets = st.facets;
                self.derivation_id = st.derivation_id;
            }
            FrameResult::Restriction(res) => {
                // src-restriction-base-or-simpleType: in simpleType/restriction,
                // cannot have both 'base' attribute and inline simpleType child
                if res.base_type.is_some() && res.inline_type.is_some() {
                    return Err(SchemaError::structural(
                        "src-restriction-base-or-simpleType",
                        "Simple type restriction cannot have both 'base' attribute and inline type",
                        None,
                    ));
                }
                // The restriction element inside <simpleType> only admits
                // annotation, an inline simpleType, and facets (§3.16.2 XML
                // representation). Attribute uses, attribute groups, wildcards,
                // particles, open content and `assert` (its `assertion` facet
                // is a facet) belong to simpleContent/complexContent
                // restrictions and are syntax errors here (msData stC029).
                if res.attribute_wildcard.is_some()
                    || !res.attributes.is_empty()
                    || !res.attribute_groups.is_empty()
                    || res.particle.is_some()
                    || res.open_content.is_some()
                    || !res.assertions.is_empty()
                {
                    return Err(SchemaError::structural(
                        "src-simple-type",
                        "Simple type restriction only allows annotation, an inline simpleType, \
                         and facet elements",
                        None,
                    ));
                }
                let base = if let Some(inline) = res.inline_type.clone() {
                    Some(TypeRefResult::Inline(Box::new(TypeFrameResult::Simple(Box::new(inline)))))
                } else {
                    res.base_type.clone()
                };
                self.variety = Some(SimpleTypeVariety::Atomic);
                self.base_type = base;
                self.facets = res.facets.clone();
                self.derivation_id = res.id.clone();
            }
            FrameResult::Skip => {}
            _ => {}
        }
        Ok(())
    }

    fn finish(self: Box<Self>) -> SchemaResult<FrameResult> {
        // src-simple-type: a simpleType definition must have exactly one of
        // <restriction>, <list>, or <union> as a derivation child. A bare
        // <simpleType> with no derivation (e.g. only an <annotation>) is
        // structurally invalid (stB001).
        if self.variety.is_none() {
            return Err(SchemaError::structural(
                "src-simple-type",
                "simpleType must contain one of <restriction>, <list>, or <union>",
                None,
            ));
        }
        let annotation = merge_foreign_attributes(
            self.annotation,
            self.foreign_attributes,
            self.source.clone(),
        );
        Ok(FrameResult::Type(TypeFrameResult::Simple(Box::new(SimpleTypeResult {
            name: self.name,
            variety: self.variety.unwrap_or(SimpleTypeVariety::Atomic),
            base_type: self.base_type,
            item_type: self.item_type,
            member_types: self.member_types,
            facets: self.facets,
            final_derivation: self.final_derivation,
            id: self.id,
            derivation_id: self.derivation_id,
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
// Content order of complexType / restriction / extension
// ============================================================================

/// A child of `<complexType>`, `<restriction>` or `<extension>`, by the place
/// it takes in the content models of Structures 1.1 §3.4.2 (and §3.16.2 for a
/// simple type's `<restriction>`, which shares the restriction frame):
///
/// - `<complexType>`: (annotation?, (simpleContent | complexContent |
///   (openContent?, (group | all | choice | sequence)?, ((attribute |
///   attributeGroup)*, anyAttribute?), assert*)))
/// - `<simpleContent>` / `<restriction>`: (annotation?, (simpleType?,
///   (*facets* | assertion)*)?, ((attribute | attributeGroup)*,
///   anyAttribute?), assert*)
/// - `<simpleContent>` / `<extension>`: (annotation?, ((attribute |
///   attributeGroup)*, anyAttribute?), assert*)
/// - `<complexContent>` / `<restriction>`: (annotation?, openContent?, (group |
///   all | choice | sequence)?, ((attribute | attributeGroup)*,
///   anyAttribute?), assert*)
/// - `<complexContent>` / `<extension>`: (annotation?, openContent?, ((group |
///   all | choice | sequence)?, ((attribute | attributeGroup)*,
///   anyAttribute?), assert*))
///
/// One `<restriction>` frame serves the three restrictions and one
/// `<extension>` frame both extensions; what a context forbids outright (a
/// particle in simple content, a facet in complex content, an `assert` in a
/// simple type) is refused by the parent when the result is attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TypeContentChild {
    Annotation,
    /// `<simpleContent>` or `<complexContent>` (complexType only).
    Derivation,
    OpenContent,
    /// `<group>`, `<all>`, `<choice>` or `<sequence>`.
    Particle,
    /// An inline `<simpleType>` (restriction only).
    SimpleType,
    /// A constraining facet, including XSD 1.1 `assertion` and
    /// `explicitTimezone` (restriction only).
    Facet,
    /// `<attribute>` or `<attributeGroup>`.
    Attribute,
    /// `<anyAttribute>`.
    AttributeWildcard,
    /// `<assert>`.
    Assert,
}

impl TypeContentChild {
    fn of(local_name: &str) -> Option<Self> {
        Some(match local_name {
            xsd_names::ANNOTATION => Self::Annotation,
            xsd_names::SIMPLE_CONTENT | xsd_names::COMPLEX_CONTENT => Self::Derivation,
            xsd_names::SEQUENCE | xsd_names::CHOICE | xsd_names::ALL | xsd_names::GROUP => {
                Self::Particle
            }
            xsd_names::SIMPLE_TYPE => Self::SimpleType,
            xsd_names::ENUMERATION
            | xsd_names::PATTERN
            | xsd_names::MIN_INCLUSIVE
            | xsd_names::MAX_INCLUSIVE
            | xsd_names::MIN_EXCLUSIVE
            | xsd_names::MAX_EXCLUSIVE
            | xsd_names::MIN_LENGTH
            | xsd_names::MAX_LENGTH
            | xsd_names::LENGTH
            | xsd_names::TOTAL_DIGITS
            | xsd_names::FRACTION_DIGITS
            | xsd_names::WHITE_SPACE => Self::Facet,
            #[cfg(feature = "xsd11")]
            xsd_names::ASSERTION | xsd_names::EXPLICIT_TIMEZONE => Self::Facet,
            xsd_names::ATTRIBUTE | xsd_names::ATTRIBUTE_GROUP => Self::Attribute,
            xsd_names::ANY_ATTRIBUTE => Self::AttributeWildcard,
            #[cfg(feature = "xsd11")]
            xsd_names::OPEN_CONTENT => Self::OpenContent,
            #[cfg(feature = "xsd11")]
            xsd_names::ASSERT => Self::Assert,
            _ => return None,
        })
    }
}

/// How far the content of `<complexType>`, `<restriction>` or `<extension>`
/// has got: the last kind of child seen (see [`TypeContentChild`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TypeContentPhase {
    Start,
    Annotation,
    /// After `<simpleContent>` / `<complexContent>`: nothing may follow.
    Done,
    OpenContent,
    Particle,
    SimpleType,
    Facets,
    Attributes,
    AttributeWildcard,
    Assertions,
}

impl TypeContentPhase {
    /// Whether `child` may come next.
    fn admits(self, child: TypeContentChild) -> bool {
        use TypeContentChild as C;
        use TypeContentPhase as P;
        match child {
            C::Annotation => self == P::Start,
            C::Derivation | C::SimpleType | C::OpenContent => {
                matches!(self, P::Start | P::Annotation)
            }
            C::Particle => matches!(self, P::Start | P::Annotation | P::OpenContent),
            C::Facet => matches!(self, P::Start | P::Annotation | P::SimpleType | P::Facets),
            C::Attribute | C::AttributeWildcard => matches!(
                self,
                P::Start
                    | P::Annotation
                    | P::OpenContent
                    | P::Particle
                    | P::SimpleType
                    | P::Facets
                    | P::Attributes
            ),
            C::Assert => self != P::Done,
        }
    }

    /// The phase after `child`.
    fn after(child: TypeContentChild) -> Self {
        match child {
            TypeContentChild::Annotation => Self::Annotation,
            TypeContentChild::Derivation => Self::Done,
            TypeContentChild::OpenContent => Self::OpenContent,
            TypeContentChild::Particle => Self::Particle,
            TypeContentChild::SimpleType => Self::SimpleType,
            TypeContentChild::Facet => Self::Facets,
            TypeContentChild::Attribute => Self::Attributes,
            TypeContentChild::AttributeWildcard => Self::AttributeWildcard,
            TypeContentChild::Assert => Self::Assertions,
        }
    }

    /// `allows` of a frame whose content model has the children `kinds`.
    fn allows(self, local_name: &str, kinds: &[TypeContentChild]) -> bool {
        TypeContentChild::of(local_name)
            .is_some_and(|child| kinds.contains(&child) && self.admits(child))
    }

    /// `on_child_start`: the phase after `local_name`, or `self` when it is no
    /// child of the content model (the parser has refused it already).
    fn advance(self, local_name: &str) -> Self {
        TypeContentChild::of(local_name).map_or(self, Self::after)
    }
}

/// The children of `<restriction>` (any of the three).
const RESTRICTION_CHILDREN: &[TypeContentChild] = &[
    TypeContentChild::Annotation,
    TypeContentChild::OpenContent,
    TypeContentChild::Particle,
    TypeContentChild::SimpleType,
    TypeContentChild::Facet,
    TypeContentChild::Attribute,
    TypeContentChild::AttributeWildcard,
    TypeContentChild::Assert,
];

/// The children of `<extension>` (either).
const EXTENSION_CHILDREN: &[TypeContentChild] = &[
    TypeContentChild::Annotation,
    TypeContentChild::OpenContent,
    TypeContentChild::Particle,
    TypeContentChild::Attribute,
    TypeContentChild::AttributeWildcard,
    TypeContentChild::Assert,
];

/// The children of `<complexType>`.
const COMPLEX_TYPE_CHILDREN: &[TypeContentChild] = &[
    TypeContentChild::Annotation,
    TypeContentChild::Derivation,
    TypeContentChild::OpenContent,
    TypeContentChild::Particle,
    TypeContentChild::Attribute,
    TypeContentChild::AttributeWildcard,
    TypeContentChild::Assert,
];

// ============================================================================
// Restriction Frame
// ============================================================================

/// Frame for xs:restriction
pub struct RestrictionFrame {
    phase: TypeContentPhase,
    base_type: Option<TypeRefResult>,
    facets: FacetSet,
    particle: Option<ParticleResult>,
    open_content: Option<OpenContentResult>,
    attributes: Vec<AttributeUseResult>,
    attribute_groups: Vec<QNameRef>,
    attribute_wildcard: Option<WildcardResult>,
    assertions: Vec<AssertResult>,
    id: Option<String>,
    annotation: Option<Annotation>,
    inline_type: Option<SimpleTypeResult>,
    source: Option<SourceRef>,
    foreign_attributes: Vec<ForeignAttribute>,
}

impl RestrictionFrame {
    pub fn new(
        attrs: &AttributeMap,
        name_table: &NameTable,
        source: Option<SourceRef>,
        ns_snapshot: &NamespaceContextSnapshot,
    ) -> SchemaResult<Self> {
        let base_type = attrs
            .get_value_by_name(name_table, "base")
            .map(|s| parse_qname_ref(s, name_table, ns_snapshot))
            .transpose()?
            .map(TypeRefResult::QName);

        let id = attrs
            .get_value_by_name(name_table, "id")
            .map(String::from);

        Ok(Self {
            phase: TypeContentPhase::Start,
            base_type,
            facets: FacetSet::new(),
            particle: None,
            open_content: None,
            attributes: Vec::new(),
            attribute_groups: Vec::new(),
            attribute_wildcard: None,
            assertions: Vec::new(),
            id,
            annotation: None,
            inline_type: None,
            source,
            foreign_attributes: Vec::new(),
        })
    }
}

impl Frame for RestrictionFrame {
    fn allows(&self, local_name: &str, _name_table: &NameTable) -> bool {
        self.phase.allows(local_name, RESTRICTION_CHILDREN)
    }

    fn allows_attribute(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(local_name, "base" | "id")
    }

    fn on_child_start(&mut self, local_name: &str, _name_table: &NameTable) {
        self.phase = self.phase.advance(local_name);
    }

    fn attach(&mut self, child: FrameResult) -> SchemaResult<()> {
        match child {
            FrameResult::Annotation(ann) => {
                self.annotation = Some(ann);
            }
            FrameResult::Type(TypeFrameResult::Simple(st)) => {
                self.inline_type = Some(*st);
            }
            FrameResult::Facet(facet) => {
                apply_facet(&mut self.facets, facet)?;
            }
            FrameResult::Particle(particle) => {
                self.particle = Some(particle);
            }
            FrameResult::OpenContent(open_content) => {
                self.open_content = Some(open_content);
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
            FrameResult::Group(GroupFrameResult::Model(mg)) => {
                let min_occurs = mg.min_occurs;
                let max_occurs = mg.max_occurs;
                self.particle = Some(ParticleResult {
                    term: ParticleTerm::Group(*mg),
                    min_occurs,
                    max_occurs,
                    source: None,
                });
            }
            FrameResult::Wildcard(wc) => {
                self.attribute_wildcard = Some(wc);
            }
            FrameResult::Assert(assertion) => {
                self.assertions.push(assertion);
            }
            FrameResult::Skip => {}
            _ => {}
        }
        Ok(())
    }

    fn finish(self: Box<Self>) -> SchemaResult<FrameResult> {
        // Note: The "base XOR inline simpleType" constraint (src-restriction-base-or-simpleType)
        // only applies to simpleType/restriction, not to simpleContent/restriction where both
        // are valid (base names the complex type, inline simpleType restricts its content).
        // The parent frame (SimpleTypeFrame vs SimpleContentFrame) enforces this contextually.

        let annotation = merge_foreign_attributes(
            self.annotation,
            self.foreign_attributes,
            self.source.clone(),
        );
        Ok(FrameResult::Restriction(Box::new(RestrictionResult {
            base_type: self.base_type,
            inline_type: self.inline_type,
            facets: self.facets,
            particle: self.particle,
            open_content: self.open_content,
            attributes: self.attributes,
            attribute_groups: self.attribute_groups,
            attribute_wildcard: self.attribute_wildcard,
            assertions: self.assertions,
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

// ============================================================================
// Extension Frame
// ============================================================================

/// Frame for xs:extension
pub struct ExtensionFrame {
    phase: TypeContentPhase,
    base_type: Option<TypeRefResult>,
    particle: Option<ParticleResult>,
    open_content: Option<OpenContentResult>,
    attributes: Vec<AttributeUseResult>,
    attribute_groups: Vec<QNameRef>,
    attribute_wildcard: Option<WildcardResult>,
    assertions: Vec<AssertResult>,
    id: Option<String>,
    annotation: Option<Annotation>,
    source: Option<SourceRef>,
    foreign_attributes: Vec<ForeignAttribute>,
}

impl ExtensionFrame {
    pub fn new(
        attrs: &AttributeMap,
        name_table: &NameTable,
        source: Option<SourceRef>,
        ns_snapshot: &NamespaceContextSnapshot,
    ) -> SchemaResult<Self> {
        let base_type = attrs
            .get_value_by_name(name_table, "base")
            .map(|s| parse_qname_ref(s, name_table, ns_snapshot))
            .transpose()?
            .map(TypeRefResult::QName);

        let id = attrs
            .get_value_by_name(name_table, "id")
            .map(String::from);

        Ok(Self {
            phase: TypeContentPhase::Start,
            base_type,
            particle: None,
            open_content: None,
            attributes: Vec::new(),
            attribute_groups: Vec::new(),
            attribute_wildcard: None,
            assertions: Vec::new(),
            id,
            annotation: None,
            source,
            foreign_attributes: Vec::new(),
        })
    }
}

impl Frame for ExtensionFrame {
    fn allows(&self, local_name: &str, _name_table: &NameTable) -> bool {
        self.phase.allows(local_name, EXTENSION_CHILDREN)
    }

    fn allows_attribute(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(local_name, "base" | "id")
    }

    fn on_child_start(&mut self, local_name: &str, _name_table: &NameTable) {
        self.phase = self.phase.advance(local_name);
    }

    fn attach(&mut self, child: FrameResult) -> SchemaResult<()> {
        match child {
            FrameResult::Annotation(ann) => {
                self.annotation = Some(ann);
            }
            FrameResult::Particle(particle) => {
                self.particle = Some(particle);
            }
            FrameResult::OpenContent(open_content) => {
                self.open_content = Some(open_content);
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
            FrameResult::Group(GroupFrameResult::Model(mg)) => {
                let min_occurs = mg.min_occurs;
                let max_occurs = mg.max_occurs;
                self.particle = Some(ParticleResult {
                    term: ParticleTerm::Group(*mg),
                    min_occurs,
                    max_occurs,
                    source: None,
                });
            }
            FrameResult::Wildcard(wc) => {
                self.attribute_wildcard = Some(wc);
            }
            FrameResult::Assert(assertion) => {
                self.assertions.push(assertion);
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
        Ok(FrameResult::Extension(ExtensionResult {
            base_type: self.base_type,
            particle: self.particle,
            open_content: self.open_content,
            attributes: self.attributes,
            attribute_groups: self.attribute_groups,
            attribute_wildcard: self.attribute_wildcard,
            assertions: self.assertions,
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
// List Frame
// ============================================================================

/// Frame for xs:list within simpleType
pub struct ListFrame {
    item_type: Option<TypeRefResult>,
    id: Option<String>,
    annotation: Option<Annotation>,
    inline_type: Option<SimpleTypeResult>,
    source: Option<SourceRef>,
    foreign_attributes: Vec<ForeignAttribute>,
    /// Set on `<simpleType>` start (before the inline frame is pushed and
    /// before `attach` populates `inline_type`). Used by `allows()` to
    /// reject `<annotation>` siblings that follow a `<simpleType>`, since
    /// `inline_type.is_some()` is also the natural "saw it" signal once
    /// `attach` has run. The flag captures the earlier moment so that any
    /// hypothetical pre-attach query also rejects (stD017).
    saw_inline_type: bool,
}

impl ListFrame {
    pub fn new(
        attrs: &AttributeMap,
        name_table: &NameTable,
        source: Option<SourceRef>,
        ns_snapshot: &NamespaceContextSnapshot,
    ) -> SchemaResult<Self> {
        let item_type = attrs
            .get_value_by_name(name_table, "itemType")
            .map(|s| parse_qname_ref(s, name_table, ns_snapshot))
            .transpose()?
            .map(TypeRefResult::QName);

        let id = attrs
            .get_value_by_name(name_table, "id")
            .map(String::from);

        Ok(Self {
            item_type,
            id,
            annotation: None,
            inline_type: None,
            source,
            foreign_attributes: Vec::new(),
            saw_inline_type: false,
        })
    }
}

impl Frame for ListFrame {
    fn allows(&self, local_name: &str, _name_table: &NameTable) -> bool {
        match local_name {
            xsd_names::ANNOTATION => !self.saw_inline_type,
            xsd_names::SIMPLE_TYPE => true,
            _ => false,
        }
    }

    fn allows_attribute(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(local_name, "itemType" | "id")
    }

    fn on_child_start(&mut self, local_name: &str, _name_table: &NameTable) {
        if local_name == xsd_names::SIMPLE_TYPE {
            self.saw_inline_type = true;
        }
    }

    fn attach(&mut self, child: FrameResult) -> SchemaResult<()> {
        match child {
            FrameResult::Annotation(ann) => {
                self.annotation = Some(ann);
            }
            FrameResult::Type(TypeFrameResult::Simple(st)) => {
                if self.inline_type.is_some() {
                    return Err(SchemaError::structural(
                        "src-list-itemType-or-simpleType",
                        "xs:list may contain at most one inline <simpleType>",
                        None,
                    ));
                }
                self.inline_type = Some(*st);
            }
            FrameResult::Skip => {}
            _ => {}
        }
        Ok(())
    }

    fn finish(self: Box<Self>) -> SchemaResult<FrameResult> {
        // Validate list structure: itemType XOR inline simpleType
        let has_item_type_attr = self.item_type.is_some();
        let has_inline_type = self.inline_type.is_some();

        if has_item_type_attr && has_inline_type {
            return Err(SchemaError::structural(
                "src-list-itemType-or-simpleType",
                "List cannot have both 'itemType' attribute and inline simpleType",
                None,
            ));
        }

        if !has_item_type_attr && !has_inline_type {
            return Err(SchemaError::structural(
                "src-list-itemType-or-simpleType",
                "List must have either 'itemType' attribute or inline simpleType",
                None,
            ));
        }

        let item = if let Some(inline) = self.inline_type {
            Some(TypeRefResult::Inline(Box::new(TypeFrameResult::Simple(Box::new(inline)))))
        } else {
            self.item_type
        };

        let annotation = merge_foreign_attributes(
            self.annotation,
            self.foreign_attributes,
            self.source.clone(),
        );
        Ok(FrameResult::Type(TypeFrameResult::Simple(Box::new(SimpleTypeResult {
            name: None,
            variety: SimpleTypeVariety::List,
            base_type: None,
            item_type: item,
            member_types: Vec::new(),
            facets: FacetSet::new(),
            final_derivation: None,
            id: None,
            derivation_id: self.id,
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
// Union Frame
// ============================================================================

/// Frame for xs:union within simpleType
pub struct UnionFrame {
    member_types: Vec<TypeRefResult>,
    id: Option<String>,
    annotation: Option<Annotation>,
    source: Option<SourceRef>,
    foreign_attributes: Vec<ForeignAttribute>,
    /// Whether a `<simpleType>` member has been seen — once true, no more
    /// `<annotation>` siblings are allowed (annotation must come first per
    /// the xs:union content model). stE016.
    saw_inline_type: bool,
}

impl UnionFrame {
    pub fn new(
        attrs: &AttributeMap,
        name_table: &NameTable,
        source: Option<SourceRef>,
        ns_snapshot: &NamespaceContextSnapshot,
    ) -> SchemaResult<Self> {
        let member_types = if let Some(s) = attrs.get_value_by_name(name_table, "memberTypes") {
            parse_qname_list(s, name_table, ns_snapshot)?
                .into_iter()
                .map(TypeRefResult::QName)
                .collect()
        } else {
            Vec::new()
        };

        let id = attrs
            .get_value_by_name(name_table, "id")
            .map(String::from);

        Ok(Self {
            member_types,
            id,
            annotation: None,
            source,
            foreign_attributes: Vec::new(),
            saw_inline_type: false,
        })
    }
}

impl Frame for UnionFrame {
    fn allows(&self, local_name: &str, _name_table: &NameTable) -> bool {
        match local_name {
            xsd_names::ANNOTATION => !self.saw_inline_type,
            xsd_names::SIMPLE_TYPE => true,
            _ => false,
        }
    }

    fn allows_attribute(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(local_name, "memberTypes" | "id")
    }

    fn on_child_start(&mut self, local_name: &str, _name_table: &NameTable) {
        if local_name == xsd_names::SIMPLE_TYPE {
            self.saw_inline_type = true;
        }
    }

    fn attach(&mut self, child: FrameResult) -> SchemaResult<()> {
        match child {
            FrameResult::Annotation(ann) => {
                self.annotation = Some(ann);
            }
            FrameResult::Type(TypeFrameResult::Simple(st)) => {
                self.member_types.push(TypeRefResult::Inline(Box::new(
                    TypeFrameResult::Simple(st),
                )));
            }
            FrameResult::Skip => {}
            _ => {}
        }
        Ok(())
    }

    fn finish(self: Box<Self>) -> SchemaResult<FrameResult> {
        // Validate union structure: must have memberTypes and/or inline simpleTypes
        if self.member_types.is_empty() {
            return Err(SchemaError::structural(
                "src-union-memberTypes-or-simpleTypes",
                "Union must have 'memberTypes' attribute or inline simpleType children",
                None,
            ));
        }

        let annotation = merge_foreign_attributes(
            self.annotation,
            self.foreign_attributes,
            self.source.clone(),
        );
        Ok(FrameResult::Type(TypeFrameResult::Simple(Box::new(SimpleTypeResult {
            name: None,
            variety: SimpleTypeVariety::Union,
            base_type: None,
            item_type: None,
            member_types: self.member_types,
            facets: FacetSet::new(),
            final_derivation: None,
            id: None,
            derivation_id: self.id,
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
// Simple/Complex Content Frames
// ============================================================================

/// src-ct: the XSD schema-for-schemas content model of
/// `<xs:simpleContent>/<xs:restriction>` and `<xs:simpleContent>/<xs:extension>`
/// permits only facets (restriction only), attributes, attribute groups, an
/// attribute wildcard, and (XSD 1.1) assertions. Model groups and
/// `<xs:openContent>` are not allowed.
fn reject_simple_content_particle_or_open_content(
    context: &str,
    particle: Option<&ParticleResult>,
    open_content: Option<&OpenContentResult>,
) -> SchemaResult<()> {
    if particle.is_some() {
        return Err(SchemaError::structural(
            "src-ct",
            format!(
                "{context} must not contain a model group \
                 (xs:sequence, xs:choice, xs:all, or xs:group)"
            ),
            None,
        ));
    }
    if open_content.is_some() {
        return Err(SchemaError::structural(
            "src-ct",
            format!("{context} must not contain xs:openContent"),
            None,
        ));
    }
    Ok(())
}

/// Frame for xs:simpleContent
pub struct SimpleContentFrame {
    id: Option<String>,
    base_type: Option<TypeRefResult>,
    content_type: Option<Box<SimpleTypeResult>>,
    derivation: Option<DerivationMethod>,
    facets: FacetSet,
    attributes: Vec<AttributeUseResult>,
    attribute_groups: Vec<QNameRef>,
    attribute_wildcard: Option<WildcardResult>,
    assertions: Vec<AssertResult>,
    derivation_id: Option<String>,
    annotation: Option<Annotation>,
    source: Option<SourceRef>,
    foreign_attributes: Vec<ForeignAttribute>,
    /// `true` after a `<restriction>` or `<extension>` child has been seen.
    /// The XSD schema-for-schemas content model `(annotation?, (restriction |
    /// extension))` forbids any further children — including a trailing
    /// `<annotation>` — so we reject them via `allows`.
    derivation_seen: bool,
}

impl SimpleContentFrame {
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
            base_type: None,
            content_type: None,
            derivation: None,
            facets: FacetSet::new(),
            attributes: Vec::new(),
            attribute_groups: Vec::new(),
            attribute_wildcard: None,
            assertions: Vec::new(),
            derivation_id: None,
            annotation: None,
            source,
            foreign_attributes: Vec::new(),
            derivation_seen: false,
        })
    }
}

impl Frame for SimpleContentFrame {
    fn allows(&self, local_name: &str, _name_table: &NameTable) -> bool {
        if self.derivation_seen {
            return false;
        }
        matches!(
            local_name,
            xsd_names::ANNOTATION | xsd_names::RESTRICTION | xsd_names::EXTENSION
        )
    }

    fn allows_attribute(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(local_name, "id")
    }

    fn on_child_start(&mut self, local_name: &str, _name_table: &NameTable) {
        if matches!(local_name, xsd_names::RESTRICTION | xsd_names::EXTENSION) {
            self.derivation_seen = true;
        }
    }

    fn attach(&mut self, child: FrameResult) -> SchemaResult<()> {
        match child {
            FrameResult::Annotation(ann) => {
                self.annotation = Some(ann);
            }
            FrameResult::Restriction(res) => {
                // src-ct: the XSD schema-for-schemas content model of
                // <xs:simpleContent>/{restriction|extension} does not permit
                // model groups or <xs:openContent>.
                reject_simple_content_particle_or_open_content(
                    "xs:simpleContent/xs:restriction",
                    res.particle.as_ref(),
                    res.open_content.as_ref(),
                )?;
                if res.base_type.is_some() && res.inline_type.is_some() {
                    // simpleContent/restriction with both base and inline simpleType:
                    // base names the complex type being restricted,
                    // inline simpleType = B (content type restriction per spec 3.4.2.2 clause 1.1)
                    self.base_type = res.base_type.clone();
                    self.content_type = res.inline_type.map(Box::new);
                    self.facets = res.facets.clone();
                } else {
                    let base = if let Some(inline) = res.inline_type.clone() {
                        Some(TypeRefResult::Inline(Box::new(TypeFrameResult::Simple(Box::new(inline)))))
                    } else {
                        res.base_type.clone()
                    };
                    self.base_type = base;
                    self.facets = res.facets.clone();
                }
                self.derivation = Some(DerivationMethod::Restriction);
                self.attributes = res.attributes.clone();
                self.attribute_groups = res.attribute_groups.clone();
                self.attribute_wildcard = res.attribute_wildcard.clone();
                self.assertions = res.assertions.clone();
                self.derivation_id = res.id.clone();
            }
            FrameResult::Extension(res) => {
                reject_simple_content_particle_or_open_content(
                    "xs:simpleContent/xs:extension",
                    res.particle.as_ref(),
                    res.open_content.as_ref(),
                )?;
                self.base_type = res.base_type;
                self.derivation = Some(DerivationMethod::Extension);
                self.attributes = res.attributes;
                self.attribute_groups = res.attribute_groups;
                self.attribute_wildcard = res.attribute_wildcard;
                self.assertions = res.assertions;
                self.derivation_id = res.id;
            }
            FrameResult::Skip => {}
            _ => {}
        }
        Ok(())
    }

    fn finish(self: Box<Self>) -> SchemaResult<FrameResult> {
        let base_type = self.base_type.ok_or_else(|| SchemaError::structural(
            "s4s-att-must-appear",
            "xs:simpleContent requires a base type",
            None,
        ))?;

        Ok(FrameResult::SimpleContent(SimpleContentDefResult {
            base_type: Some(base_type),
            content_type: self.content_type,
            derivation: self.derivation.unwrap_or(DerivationMethod::Restriction),
            facets: self.facets,
            attributes: self.attributes,
            attribute_groups: self.attribute_groups,
            attribute_wildcard: self.attribute_wildcard,
            assertions: self.assertions,
            id: self.id,
            derivation_id: self.derivation_id,
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

/// Frame for xs:complexContent
pub struct ComplexContentFrame {
    id: Option<String>,
    mixed: bool,
    /// `true` when the `<xs:complexContent mixed="…">` attribute was
    /// explicitly present.  See `ComplexContentDefResult::mixed_explicit`.
    mixed_explicit: bool,
    base_type: Option<TypeRefResult>,
    derivation: Option<DerivationMethod>,
    particle: Option<ParticleResult>,
    open_content: Option<OpenContentResult>,
    attributes: Vec<AttributeUseResult>,
    attribute_groups: Vec<QNameRef>,
    attribute_wildcard: Option<WildcardResult>,
    assertions: Vec<AssertResult>,
    derivation_id: Option<String>,
    annotation: Option<Annotation>,
    source: Option<SourceRef>,
    foreign_attributes: Vec<ForeignAttribute>,
    /// See `SimpleContentFrame::derivation_seen`.
    derivation_seen: bool,
}

impl ComplexContentFrame {
    pub fn new(
        attrs: &AttributeMap,
        name_table: &NameTable,
        source: Option<SourceRef>,
    ) -> SchemaResult<Self> {
        let id = attrs
            .get_value_by_name(name_table, "id")
            .map(String::from);

        let mixed_opt = parse_optional_bool_attr(attrs, name_table, "mixed")?;
        let mixed_explicit = mixed_opt.is_some();
        let mixed = mixed_opt.unwrap_or(false);

        Ok(Self {
            id,
            mixed,
            mixed_explicit,
            base_type: None,
            derivation: None,
            particle: None,
            open_content: None,
            attributes: Vec::new(),
            attribute_groups: Vec::new(),
            attribute_wildcard: None,
            assertions: Vec::new(),
            derivation_id: None,
            annotation: None,
            source,
            foreign_attributes: Vec::new(),
            derivation_seen: false,
        })
    }
}

impl Frame for ComplexContentFrame {
    fn allows(&self, local_name: &str, _name_table: &NameTable) -> bool {
        if self.derivation_seen {
            return false;
        }
        matches!(
            local_name,
            xsd_names::ANNOTATION | xsd_names::RESTRICTION | xsd_names::EXTENSION
        )
    }

    fn allows_attribute(&self, local_name: &str, _name_table: &NameTable) -> bool {
        matches!(local_name, "id" | "mixed")
    }

    fn on_child_start(&mut self, local_name: &str, _name_table: &NameTable) {
        if matches!(local_name, xsd_names::RESTRICTION | xsd_names::EXTENSION) {
            self.derivation_seen = true;
        }
    }

    fn attach(&mut self, child: FrameResult) -> SchemaResult<()> {
        match child {
            FrameResult::Annotation(ann) => {
                self.annotation = Some(ann);
            }
            FrameResult::Restriction(res) => {
                // src-ct: the schema-for-schemas content model of
                // <xs:complexContent>/<xs:restriction> has no facet elements —
                // facets belong to simple type / simpleContent restrictions
                // (msData addB112: <xs:length> under complexContent).
                if !res.facets.is_empty() {
                    return Err(SchemaError::structural(
                        "src-ct",
                        "Facet elements are not allowed in a complexContent restriction",
                        None,
                    ));
                }
                // Nor an inline <simpleType> (§3.4.2.3: "(annotation?,
                // openContent?, (group | all | choice | sequence)?, …)").
                if res.inline_type.is_some() {
                    return Err(SchemaError::structural(
                        "src-ct",
                        "xs:simpleType is not allowed in a complexContent restriction",
                        None,
                    ));
                }
                self.base_type = res.base_type.clone();
                self.derivation = Some(DerivationMethod::Restriction);
                self.particle = res.particle.clone();
                self.open_content = res.open_content.clone();
                self.attributes = res.attributes.clone();
                self.attribute_groups = res.attribute_groups.clone();
                self.attribute_wildcard = res.attribute_wildcard.clone();
                self.assertions = res.assertions.clone();
                self.derivation_id = res.id.clone();
            }
            FrameResult::Extension(res) => {
                self.base_type = res.base_type;
                self.derivation = Some(DerivationMethod::Extension);
                self.particle = res.particle;
                self.open_content = res.open_content;
                self.attributes = res.attributes;
                self.attribute_groups = res.attribute_groups;
                self.attribute_wildcard = res.attribute_wildcard;
                self.assertions = res.assertions;
                self.derivation_id = res.id;
            }
            FrameResult::Skip => {}
            _ => {}
        }
        Ok(())
    }

    fn finish(self: Box<Self>) -> SchemaResult<FrameResult> {
        // src-ct: the XML content model of <xs:complexContent> requires exactly
        // one of <xs:restriction> or <xs:extension> as a child (plus an optional
        // leading annotation). An empty <xs:complexContent> or one containing
        // only an annotation is a content-model violation.
        let derivation = self.derivation.ok_or_else(|| SchemaError::structural(
            "src-ct",
            "xs:complexContent requires an xs:restriction or xs:extension child",
            None,
        ))?;
        Ok(FrameResult::ComplexContent(ComplexContentDefResult {
            particle: self.particle,
            derivation,
            mixed: self.mixed,
            mixed_explicit: self.mixed_explicit,
            base_type: self.base_type,
            open_content: self.open_content,
            attributes: self.attributes,
            attribute_groups: self.attribute_groups,
            attribute_wildcard: self.attribute_wildcard,
            assertions: self.assertions,
            id: self.id,
            derivation_id: self.derivation_id,
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
// Complex Type Frame
// ============================================================================

/// Frame for xs:complexType element
pub struct ComplexTypeFrame {
    phase: TypeContentPhase,
    name: Option<NameId>,
    base_type: Option<TypeRefResult>,
    derivation_method: Option<DerivationMethod>,
    mixed: bool,
    /// `true` when the `<xs:complexType mixed="…">` attribute was explicitly
    /// present. Used by §3.4.2.3 step 1 to require that an explicit
    /// `<complexContent mixed="…">` agree with this value.
    mixed_explicit: bool,
    is_abstract: bool,
    final_derivation: Option<DerivationSet>,
    block: Option<DerivationSet>,
    default_attributes_apply: bool,
    id: Option<String>,
    content: ComplexContentResult,
    open_content: Option<OpenContentResult>,
    attributes: Vec<AttributeUseResult>,
    attribute_groups: Vec<QNameRef>,
    attribute_wildcard: Option<WildcardResult>,
    assertions: Vec<AssertResult>,
    #[cfg(feature = "xsd11")]
    xpath_default_namespace: Option<String>,
    annotation: Option<Annotation>,
    source: Option<SourceRef>,
    foreign_attributes: Vec<ForeignAttribute>,
}

impl ComplexTypeFrame {
    pub fn new(
        attrs: &AttributeMap,
        name_table: &NameTable,
        source: Option<SourceRef>,
    ) -> SchemaResult<Self> {
        let name = attrs
            .get_value_by_name(name_table, "name")
            .and_then(|s| name_table.get(s));

        let mixed_opt = parse_optional_bool_attr(attrs, name_table, "mixed")?;
        let mixed_explicit = mixed_opt.is_some();
        let mixed = mixed_opt.unwrap_or(false);

        let is_abstract = parse_bool_attr_default(attrs, name_table, "abstract", false)?;

        let final_derivation = parse_derivation_set_opt(
            attrs.get_value_by_name(name_table, "final"),
        )?;

        let block = parse_derivation_set_opt(
            attrs.get_value_by_name(name_table, "block"),
        )?;

        let default_attributes_apply =
            parse_bool_attr_default(attrs, name_table, "defaultAttributesApply", true)?;

        let id = attrs
            .get_value_by_name(name_table, "id")
            .map(String::from);

        #[cfg(feature = "xsd11")]
        let xpath_default_namespace = attrs
            .get_value_by_name(name_table, "xpathDefaultNamespace")
            .map(String::from);

        Ok(Self {
            phase: TypeContentPhase::Start,
            name,
            base_type: None,
            derivation_method: None,
            mixed,
            mixed_explicit,
            is_abstract,
            final_derivation,
            block,
            default_attributes_apply,
            id,
            content: ComplexContentResult::Empty,
            open_content: None,
            attributes: Vec::new(),
            attribute_groups: Vec::new(),
            attribute_wildcard: None,
            assertions: Vec::new(),
            #[cfg(feature = "xsd11")]
            xpath_default_namespace,
            annotation: None,
            source,
            foreign_attributes: Vec::new(),
        })
    }
}

impl Frame for ComplexTypeFrame {
    fn allows(&self, local_name: &str, _name_table: &NameTable) -> bool {
        self.phase.allows(local_name, COMPLEX_TYPE_CHILDREN)
    }

    fn allows_attribute(&self, local_name: &str, _name_table: &NameTable) -> bool {
        #[cfg(feature = "xsd11")]
        if local_name == "xpathDefaultNamespace" {
            return true;
        }
        matches!(
            local_name,
            "name"
                | "mixed"
                | "abstract"
                | "final"
                | "block"
                | "defaultAttributesApply"
                | "id"
        )
    }

    fn on_child_start(&mut self, local_name: &str, _name_table: &NameTable) {
        self.phase = self.phase.advance(local_name);
    }

    fn attach(&mut self, child: FrameResult) -> SchemaResult<()> {
        match child {
            FrameResult::Annotation(ann) => {
                self.annotation = Some(ann);
            }
            FrameResult::OpenContent(open_content) => {
                self.open_content = Some(open_content);
            }
            FrameResult::SimpleContent(mut sc) => {
                self.base_type = sc
                    .base_type
                    .as_ref()
                    .and_then(|bt| match bt {
                        TypeRefResult::QName(qname) => {
                            Some(TypeRefResult::QName(qname.clone()))
                        }
                        _ => None,
                    });
                self.derivation_method = Some(sc.derivation);
                self.attributes = std::mem::take(&mut sc.attributes);
                self.attribute_groups = std::mem::take(&mut sc.attribute_groups);
                self.attribute_wildcard = sc.attribute_wildcard.take();
                self.content = ComplexContentResult::Simple(sc);
            }
            FrameResult::ComplexContent(mut cc) => {
                self.base_type = cc
                    .base_type
                    .as_ref()
                    .and_then(|bt| match bt {
                        TypeRefResult::QName(qname) => {
                            Some(TypeRefResult::QName(qname.clone()))
                        }
                        _ => None,
                    });
                self.derivation_method = Some(cc.derivation);
                self.attributes = std::mem::take(&mut cc.attributes);
                self.attribute_groups = std::mem::take(&mut cc.attribute_groups);
                self.attribute_wildcard = cc.attribute_wildcard.take();
                // §3.4.2.3 step 1: `<complexContent mixed=…>` (clause 1.1) and
                // `<complexType mixed=…>` (clause 1.2) must agree when both are
                // explicit. The note at the end of step 1 says these "will never
                // contradict each other in a conforming schema document".
                if cc.mixed_explicit && self.mixed_explicit && cc.mixed != self.mixed {
                    return Err(SchemaError::structural(
                        "src-ct",
                        "<complexType mixed=…> and <complexContent mixed=…> are both \
                         present but disagree (§3.4.2.3 step 1)",
                        None,
                    ));
                }
                if cc.mixed_explicit {
                    self.mixed = cc.mixed;
                } else {
                    cc.mixed = self.mixed;
                }
                self.content = ComplexContentResult::Complex(cc);
            }
            FrameResult::Particle(particle) => {
                self.content = ComplexContentResult::Complex(ComplexContentDefResult {
                    particle: Some(particle),
                    derivation: DerivationMethod::Restriction,
                    mixed: self.mixed,
                    mixed_explicit: true,
                    base_type: None,
                    open_content: None,
                    attributes: Vec::new(),
                    attribute_groups: Vec::new(),
                    attribute_wildcard: None,
                    assertions: Vec::new(),
                    id: None,
                    derivation_id: None,
                    source: None,
                });
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
            FrameResult::Group(GroupFrameResult::Model(mg)) => {
                let min_occurs = mg.min_occurs;
                let max_occurs = mg.max_occurs;
                let particle = ParticleResult {
                    term: ParticleTerm::Group(*mg),
                    min_occurs,
                    max_occurs,
                    source: None,
                };
                self.content = ComplexContentResult::Complex(ComplexContentDefResult {
                    particle: Some(particle),
                    derivation: DerivationMethod::Restriction,
                    mixed: self.mixed,
                    mixed_explicit: true,
                    base_type: None,
                    open_content: None,
                    attributes: Vec::new(),
                    attribute_groups: Vec::new(),
                    attribute_wildcard: None,
                    assertions: Vec::new(),
                    id: None,
                    derivation_id: None,
                    source: None,
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
            FrameResult::Assert(assertion) => {
                self.assertions.push(assertion);
            }
            FrameResult::Skip => {}
            _ => {}
        }
        Ok(())
    }

    fn finish(self: Box<Self>) -> SchemaResult<FrameResult> {
        let mut content = self.content;
        match &mut content {
            ComplexContentResult::Empty => {
                if self.open_content.is_some() || !self.assertions.is_empty() {
                    content = ComplexContentResult::Complex(ComplexContentDefResult {
                        particle: None,
                        derivation: DerivationMethod::Restriction,
                        mixed: self.mixed,
                        mixed_explicit: true,
                        base_type: None,
                        open_content: self.open_content,
                        attributes: Vec::new(),
                        attribute_groups: Vec::new(),
                        attribute_wildcard: None,
                        assertions: self.assertions,
                        id: None,
                        derivation_id: None,
                        source: None,
                    });
                }
            }
            ComplexContentResult::Complex(cc) => {
                if cc.open_content.is_none() {
                    cc.open_content = self.open_content;
                }
                if !self.assertions.is_empty() {
                    cc.assertions.extend(self.assertions);
                }
            }
            ComplexContentResult::Simple(_) => {}
        }

        let annotation = merge_foreign_attributes(
            self.annotation,
            self.foreign_attributes,
            self.source.clone(),
        );
        Ok(FrameResult::Type(TypeFrameResult::Complex(Box::new(ComplexTypeResult {
            name: self.name,
            base_type: self.base_type,
            derivation_method: self.derivation_method,
            content,
            attributes: self.attributes,
            attribute_groups: self.attribute_groups,
            attribute_wildcard: self.attribute_wildcard,
            mixed: self.mixed,
            is_abstract: self.is_abstract,
            final_derivation: self.final_derivation,
            block: self.block,
            default_attributes_apply: self.default_attributes_apply,
            id: self.id,
            #[cfg(feature = "xsd11")]
            xpath_default_namespace: self.xpath_default_namespace,
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

    fn children_inside_complex_type(&self) -> bool {
        true
    }
}

