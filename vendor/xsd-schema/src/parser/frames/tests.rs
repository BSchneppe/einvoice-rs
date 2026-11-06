#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::attrs::ParsedAttribute;

    fn make_attr(name_table: &mut NameTable, name: &str, value: &str) -> ParsedAttribute {
        let name_id = name_table.add(name);
        ParsedAttribute {
            namespace: None,
            local_name: name_id,
            prefix: None,
            value: value.to_string(),
            source: None,
        }
    }

    fn empty_snapshot() -> NamespaceContextSnapshot {
        NamespaceContextSnapshot::default()
    }

    fn snapshot_with_default_ns(name_table: &NameTable, ns: &str) -> NamespaceContextSnapshot {
        NamespaceContextSnapshot {
            default_ns: Some(name_table.add(ns)),
            bindings: Vec::new(),
        }
    }

    #[test]
    fn test_parse_qname_ref_with_default_namespace() {
        let name_table = NameTable::new();
        let target_ns = "http://example.com/target";
        let snapshot = snapshot_with_default_ns(&name_table, target_ns);
        let qname = parse_qname_ref("MyType", &name_table, &snapshot).unwrap();
        assert_eq!(name_table.resolve(qname.local_name), "MyType");
        assert!(qname.namespace.is_some());
        assert_eq!(name_table.resolve(qname.namespace.unwrap()), target_ns);
    }

    #[test]
    fn test_parse_qname_ref_without_default_namespace() {
        let name_table = NameTable::new();
        let snapshot = empty_snapshot();
        let qname = parse_qname_ref("MyType", &name_table, &snapshot).unwrap();
        assert_eq!(name_table.resolve(qname.local_name), "MyType");
        assert!(qname.namespace.is_none());
    }

    #[test]
    fn test_parse_qname_ref_prefixed_ignores_default_ns() {
        let name_table = NameTable::new();
        let xs_prefix = name_table.add("xs");
        let xs_ns = name_table.add("http://www.w3.org/2001/XMLSchema");
        let snapshot = NamespaceContextSnapshot {
            default_ns: Some(name_table.add("http://example.com/target")),
            bindings: vec![(xs_prefix, xs_ns)],
        };
        let qname = parse_qname_ref("xs:string", &name_table, &snapshot).unwrap();
        assert_eq!(name_table.resolve(qname.local_name), "string");
        assert!(qname.namespace.is_some());
        assert_eq!(
            name_table.resolve(qname.namespace.unwrap()),
            "http://www.w3.org/2001/XMLSchema"
        );
    }

    #[test]
    fn test_parse_derivation_set_empty() {
        let set = parse_derivation_set(None).unwrap();
        assert!(set.is_empty());
    }

    #[test]
    fn test_parse_derivation_set_all() {
        let set = parse_derivation_set(Some("#all")).unwrap();
        assert!(set.contains(DerivationSet::EXTENSION));
        assert!(set.contains(DerivationSet::RESTRICTION));
    }

    #[test]
    fn test_parse_derivation_set_list() {
        let set = parse_derivation_set(Some("extension restriction")).unwrap();
        assert!(set.contains(DerivationSet::EXTENSION));
        assert!(set.contains(DerivationSet::RESTRICTION));
        assert!(!set.contains(DerivationSet::LIST));
    }

    /// Absent `final=` attribute → None (caller inherits finalDefault).
    #[test]
    fn test_parse_final_attr_absent() {
        let result = parse_derivation_set_opt(None).unwrap();
        assert!(result.is_none(), "absent final= must be None so finalDefault is applied");
    }

    /// `final=""` → Some(empty set): explicit override, no derivation blocked.
    #[test]
    fn test_parse_final_attr_explicit_empty() {
        let result = parse_derivation_set_opt(Some("")).unwrap();
        assert!(result.is_some(), "final=\"\" must be Some (explicit override)");
        assert!(result.unwrap().is_empty(), "final=\"\" must produce an empty DerivationSet");
    }

    /// `final="#all"` → Some(ALL).
    #[test]
    fn test_parse_final_attr_all() {
        let result = parse_derivation_set_opt(Some("#all")).unwrap();
        assert!(result.is_some());
        let set = result.unwrap();
        assert!(set.contains(DerivationSet::EXTENSION));
        assert!(set.contains(DerivationSet::RESTRICTION));
    }

    /// `final="restriction extension"` → Some(RESTRICTION | EXTENSION).
    #[test]
    fn test_parse_final_attr_list() {
        let result = parse_derivation_set_opt(Some("restriction extension")).unwrap();
        assert!(result.is_some());
        let set = result.unwrap();
        assert!(set.contains(DerivationSet::RESTRICTION));
        assert!(set.contains(DerivationSet::EXTENSION));
        assert!(!set.contains(DerivationSet::LIST));
    }

    #[test]
    fn test_parse_process_contents_attr() {
        let name_table = NameTable::new();
        let name_id = name_table.add("processContents");

        let attrs = AttributeMap::new(vec![ParsedAttribute {
            namespace: None,
            local_name: name_id,
            prefix: None,
            value: "lax".to_string(),
            source: None,
        }]);

        assert_eq!(
            parse_process_contents_attr(&attrs, &name_table, "processContents").unwrap(),
            ProcessContents::Lax
        );

        let attrs = AttributeMap::new(Vec::new());
        assert_eq!(
            parse_process_contents_attr(&attrs, &name_table, "processContents").unwrap(),
            ProcessContents::Strict
        );

        let attrs = AttributeMap::new(vec![ParsedAttribute {
            namespace: None,
            local_name: name_id,
            prefix: None,
            value: "invalid".to_string(),
            source: None,
        }]);
        assert!(parse_process_contents_attr(&attrs, &name_table, "processContents").is_err());
    }

    #[cfg(feature = "xsd11")]
    #[test]
    fn test_parse_open_content_mode_attr() {
        let mut name_table = NameTable::new();
        let attrs = AttributeMap::new(vec![make_attr(&mut name_table, "mode", "suffix")]);
        assert_eq!(
            parse_open_content_mode_attr(&attrs, &name_table, "mode").unwrap(),
            OpenContentMode::Suffix
        );

        let attrs = AttributeMap::new(Vec::new());
        assert_eq!(
            parse_open_content_mode_attr(&attrs, &name_table, "mode").unwrap(),
            OpenContentMode::Interleave
        );

        let attrs = AttributeMap::new(vec![make_attr(&mut name_table, "mode", "bad")]);
        assert!(parse_open_content_mode_attr(&attrs, &name_table, "mode").is_err());
    }

    #[cfg(feature = "xsd11")]
    #[test]
    fn test_override_requires_schema_location() {
        let name_table = NameTable::new();
        let attrs = AttributeMap::new(Vec::new());
        let frame = OverrideFrame::new(&attrs, &name_table, None).unwrap();
        assert!(Box::new(frame).finish().is_err());
    }

    #[test]
    fn test_schema_result_retains_annotations() {
        let name_table = NameTable::new();
        let attrs = AttributeMap::new(Vec::new());
        let snapshot = empty_snapshot();
        let mut frame = SchemaFrame::new(&attrs, &name_table, None, &snapshot).unwrap();
        frame
            .attach(FrameResult::Annotation(Annotation::new()))
            .unwrap();
        let result = Box::new(frame).finish().unwrap();
        match result {
            FrameResult::Schema(schema) => {
                assert_eq!(schema.annotations.len(), 1);
            }
            _ => panic!("expected schema result"),
        }
    }

    #[test]
    fn test_element_substitution_group_list() {
        let mut name_table = NameTable::new();
        name_table.add("root");
        name_table.add("head1");
        name_table.add("head2");
        let attrs = AttributeMap::new(vec![
            make_attr(&mut name_table, "name", "root"),
            make_attr(&mut name_table, "substitutionGroup", "head1 head2"),
        ]);
        let snapshot = empty_snapshot();
        let frame = ElementFrame::new(&attrs, &name_table, None, &snapshot).unwrap();
        let result = Box::new(frame).finish().unwrap();
        match result {
            FrameResult::Element(element) => {
                assert_eq!(element.substitution_group.len(), 2);
                assert_eq!(
                    name_table.resolve(element.substitution_group[0].local_name),
                    "head1"
                );
                assert_eq!(
                    name_table.resolve(element.substitution_group[1].local_name),
                    "head2"
                );
            }
            _ => panic!("expected element result"),
        }
    }

    #[test]
    fn test_identity_requires_selector() {
        let mut name_table = NameTable::new();
        name_table.add("key1");
        let attrs = AttributeMap::new(vec![make_attr(&mut name_table, "name", "key1")]);
        let snapshot = empty_snapshot();
        let frame = IdentityFrame::new(IdentityKind::Key, &attrs, &name_table, None, &snapshot).unwrap();
        assert!(Box::new(frame).finish().is_err());
    }

    #[test]
    fn test_identity_requires_field() {
        let mut name_table = NameTable::new();
        name_table.add("key1");
        let attrs = AttributeMap::new(vec![make_attr(&mut name_table, "name", "key1")]);
        let snapshot = empty_snapshot();
        let mut frame =
            IdentityFrame::new(IdentityKind::Unique, &attrs, &name_table, None, &snapshot).unwrap();
        frame
            .attach(FrameResult::Selector(SelectorResult {
                xpath: ".//a".to_string(),
                xpath_default_namespace: None,
                ns_snapshot: empty_snapshot(),
                id: None,
                annotation: None,
                source: None,
            }))
            .unwrap();
        assert!(Box::new(frame).finish().is_err());
    }

    #[test]
    fn test_parse_bool_attr_default_invalid() {
        let mut name_table = NameTable::new();
        let attrs = AttributeMap::new(vec![make_attr(&mut name_table, "mixed", "maybe")]);
        assert!(parse_bool_attr_default(&attrs, &name_table, "mixed", false).is_err());
    }

    #[test]
    fn test_parse_min_occurs_attr_invalid() {
        let mut name_table = NameTable::new();
        let attrs = AttributeMap::new(vec![make_attr(&mut name_table, "minOccurs", "unbounded")]);
        assert!(parse_min_occurs_attr(&attrs, &name_table, "minOccurs").is_err());

        let attrs = AttributeMap::new(vec![make_attr(&mut name_table, "minOccurs", "nope")]);
        assert!(parse_min_occurs_attr(&attrs, &name_table, "minOccurs").is_err());
    }

    #[test]
    fn test_parse_max_occurs_attr() {
        let mut name_table = NameTable::new();
        let attrs = AttributeMap::new(vec![make_attr(&mut name_table, "maxOccurs", "unbounded")]);
        assert_eq!(
            parse_max_occurs_attr(&attrs, &name_table, "maxOccurs").unwrap(),
            None
        );

        let attrs = AttributeMap::new(vec![make_attr(&mut name_table, "maxOccurs", "bad")]);
        assert!(parse_max_occurs_attr(&attrs, &name_table, "maxOccurs").is_err());
    }

    #[test]
    fn test_validate_attr_value_invalid_form() {
        let mut name_table = NameTable::new();
        let attrs = AttributeMap::new(vec![make_attr(&mut name_table, "form", "bad")]);
        assert!(validate_attr_value(&attrs, &name_table, "form", parse_form).is_err());
    }

    #[test]
    fn test_create_frame_recovering_on_invalid_value() {
        let mut name_table = NameTable::new();
        let attrs = AttributeMap::new(vec![make_attr(&mut name_table, "nillable", "maybe")]);
        let mut errors = Vec::new();
        let snapshot = empty_snapshot();

        let frame = create_frame_recovering(
            xsd_names::ELEMENT,
            &attrs,
            &name_table,
            None,
            &snapshot,
            &mut errors,
        );

        assert_eq!(errors.len(), 1);
        let result = frame.finish().unwrap();
        assert!(matches!(result, FrameResult::Skip));
    }

    #[test]
    fn test_skip_frame_depth() {
        let mut frame = SkipFrame::new(None);
        frame.enter();
        frame.enter();
        assert!(!frame.leave());
        assert!(!frame.leave());
        assert!(frame.leave());
    }

    #[test]
    fn test_facet_kind() {
        assert_eq!(FacetKind::Enumeration, FacetKind::Enumeration);
        assert_ne!(FacetKind::Enumeration, FacetKind::Pattern);
    }

    #[test]
    fn test_compositor() {
        assert_eq!(Compositor::Sequence, Compositor::Sequence);
        assert_ne!(Compositor::Sequence, Compositor::Choice);
    }

    #[test]
    fn test_identity_kind() {
        assert_eq!(IdentityKind::Key, IdentityKind::Key);
        assert_ne!(IdentityKind::Key, IdentityKind::Keyref);
    }
}
