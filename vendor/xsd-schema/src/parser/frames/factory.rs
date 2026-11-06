// ============================================================================
// Frame Factory
// ============================================================================

/// Create a frame for the given element
///
/// The namespace snapshot is used to resolve QName references in attributes
/// (type, ref, base, substitutionGroup, etc.) during frame construction.
pub fn create_frame(
    local_name: &str,
    attrs: &AttributeMap,
    name_table: &NameTable,
    source: Option<SourceRef>,
    ns_snapshot: &NamespaceContextSnapshot,
) -> SchemaResult<Box<dyn Frame>> {
    let frame: Box<dyn Frame> = match local_name {
        xsd_names::SCHEMA => Box::new(SchemaFrame::new(attrs, name_table, source, ns_snapshot)?),
        xsd_names::SIMPLE_TYPE => Box::new(SimpleTypeFrame::new(attrs, name_table, source)?),
        xsd_names::COMPLEX_TYPE => Box::new(ComplexTypeFrame::new(attrs, name_table, source)?),
        xsd_names::ELEMENT => Box::new(ElementFrame::new(attrs, name_table, source, ns_snapshot)?),
        xsd_names::ATTRIBUTE => Box::new(AttributeFrame::new(attrs, name_table, source, ns_snapshot)?),
        xsd_names::GROUP => Box::new(GroupFrame::new(attrs, name_table, source, ns_snapshot)?),
        xsd_names::ATTRIBUTE_GROUP => Box::new(AttributeGroupFrame::new(attrs, name_table, source, ns_snapshot)?),
        xsd_names::NOTATION => Box::new(NotationFrame::new(attrs, name_table, source)?),
        xsd_names::SIMPLE_CONTENT => Box::new(SimpleContentFrame::new(attrs, name_table, source)?),
        xsd_names::COMPLEX_CONTENT => Box::new(ComplexContentFrame::new(attrs, name_table, source)?),
        xsd_names::RESTRICTION => Box::new(RestrictionFrame::new(attrs, name_table, source, ns_snapshot)?),
        xsd_names::EXTENSION => Box::new(ExtensionFrame::new(attrs, name_table, source, ns_snapshot)?),
        xsd_names::LIST => Box::new(ListFrame::new(attrs, name_table, source, ns_snapshot)?),
        xsd_names::UNION => Box::new(UnionFrame::new(attrs, name_table, source, ns_snapshot)?),
        xsd_names::SEQUENCE => {
            Box::new(ModelGroupFrame::new(Compositor::Sequence, attrs, name_table, source, ns_snapshot)?)
        }
        xsd_names::CHOICE => {
            Box::new(ModelGroupFrame::new(Compositor::Choice, attrs, name_table, source, ns_snapshot)?)
        }
        xsd_names::ALL => {
            Box::new(ModelGroupFrame::new(Compositor::All, attrs, name_table, source, ns_snapshot)?)
        }
        xsd_names::ANY => Box::new(AnyFrame::new(
            attrs, name_table, source,
            #[cfg(feature = "xsd11")] ns_snapshot,
        )?),
        xsd_names::ANY_ATTRIBUTE => Box::new(AnyAttributeFrame::new(
            attrs, name_table, source,
            #[cfg(feature = "xsd11")] ns_snapshot,
        )?),
        xsd_names::ANNOTATION => Box::new(AnnotationFrame::new(attrs, name_table, source)?),
        xsd_names::APPINFO => Box::new(AppinfoFrame::new(attrs, name_table, source)?),
        xsd_names::DOCUMENTATION => Box::new(DocumentationFrame::new(attrs, name_table, source)?),
        xsd_names::INCLUDE => Box::new(IncludeFrame::new(attrs, name_table, source)?),
        xsd_names::IMPORT => Box::new(ImportFrame::new(attrs, name_table, source)?),
        xsd_names::REDEFINE => Box::new(RedefineFrame::new(attrs, name_table, source)?),
        #[cfg(feature = "xsd11")]
        xsd_names::OVERRIDE => Box::new(OverrideFrame::new(attrs, name_table, source)?),
        xsd_names::KEY => Box::new(IdentityFrame::new(IdentityKind::Key, attrs, name_table, source, ns_snapshot)?),
        xsd_names::KEYREF => Box::new(IdentityFrame::new(IdentityKind::Keyref, attrs, name_table, source, ns_snapshot)?),
        xsd_names::UNIQUE => Box::new(IdentityFrame::new(IdentityKind::Unique, attrs, name_table, source, ns_snapshot)?),
        xsd_names::SELECTOR => Box::new(SelectorFrame::new(attrs, name_table, source, ns_snapshot.clone())?),
        xsd_names::FIELD => Box::new(FieldFrame::new(attrs, name_table, source, ns_snapshot.clone())?),
        #[cfg(feature = "xsd11")]
        xsd_names::ALTERNATIVE => Box::new(AlternativeFrame::new(attrs, name_table, source, ns_snapshot.clone())?),
        #[cfg(feature = "xsd11")]
        xsd_names::ASSERT => Box::new(AssertFrame::new(attrs, name_table, source, ns_snapshot.clone())?),
        xsd_names::ENUMERATION => Box::new(FacetFrame::new(
            FacetKind::Enumeration,
            attrs,
            name_table,
            source,
            Some(ns_snapshot.clone()),
        )?),
        xsd_names::PATTERN => Box::new(FacetFrame::new(FacetKind::Pattern, attrs, name_table, source, None)?),
        xsd_names::MIN_INCLUSIVE => {
            Box::new(FacetFrame::new(FacetKind::MinInclusive, attrs, name_table, source, None)?)
        }
        xsd_names::MAX_INCLUSIVE => {
            Box::new(FacetFrame::new(FacetKind::MaxInclusive, attrs, name_table, source, None)?)
        }
        xsd_names::MIN_EXCLUSIVE => {
            Box::new(FacetFrame::new(FacetKind::MinExclusive, attrs, name_table, source, None)?)
        }
        xsd_names::MAX_EXCLUSIVE => {
            Box::new(FacetFrame::new(FacetKind::MaxExclusive, attrs, name_table, source, None)?)
        }
        xsd_names::MIN_LENGTH => Box::new(FacetFrame::new(FacetKind::MinLength, attrs, name_table, source, None)?),
        xsd_names::MAX_LENGTH => Box::new(FacetFrame::new(FacetKind::MaxLength, attrs, name_table, source, None)?),
        xsd_names::LENGTH => Box::new(FacetFrame::new(FacetKind::Length, attrs, name_table, source, None)?),
        xsd_names::TOTAL_DIGITS => Box::new(FacetFrame::new(FacetKind::TotalDigits, attrs, name_table, source, None)?),
        xsd_names::FRACTION_DIGITS => Box::new(FacetFrame::new(FacetKind::FractionDigits, attrs, name_table, source, None)?),
        xsd_names::WHITE_SPACE => Box::new(FacetFrame::new(FacetKind::WhiteSpace, attrs, name_table, source, None)?),
        #[cfg(feature = "xsd11")]
        xsd_names::OPEN_CONTENT => Box::new(OpenContentFrame::new(attrs, name_table, source)?),
        #[cfg(feature = "xsd11")]
        xsd_names::DEFAULT_OPEN_CONTENT => {
            Box::new(DefaultOpenContentFrame::new(attrs, name_table, source)?)
        }
        // XSD 1.1 facets
        xsd_names::ASSERTION => {
            Box::new(FacetFrame::new(FacetKind::Assertion, attrs, name_table, source, Some(ns_snapshot.clone()))?)
        }
        xsd_names::EXPLICIT_TIMEZONE => {
            Box::new(FacetFrame::new(FacetKind::ExplicitTimezone, attrs, name_table, source, None)?)
        }
        _ => {
            // Unknown element - skip it
            Box::new(SkipFrame::new(source))
        }
    };

    frame.validate_attributes(attrs, name_table)?;
    Ok(frame)
}

pub fn create_frame_recovering(
    local_name: &str,
    attrs: &AttributeMap,
    name_table: &NameTable,
    source: Option<SourceRef>,
    ns_snapshot: &NamespaceContextSnapshot,
    errors: &mut Vec<SchemaError>,
) -> Box<dyn Frame> {
    let recovery_source = source.clone();
    match create_frame(local_name, attrs, name_table, source, ns_snapshot) {
        Ok(frame) => frame,
        Err(err) => {
            errors.push(err);
            Box::new(SkipFrame::new(recovery_source))
        }
    }
}
