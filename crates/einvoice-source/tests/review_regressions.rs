use einvoice_source::{NodeKind, QueryBudget, SourceDocument, SourceError, SourceLimits};

fn variants(s: &str) -> Vec<Vec<u8>> {
    let mut out = vec![
        s.as_bytes().to_vec(),
        [b"\xef\xbb\xbf".as_slice(), s.as_bytes()].concat(),
    ];
    for (bom, little) in [([255, 254], true), ([254, 255], false)] {
        let mut raw = bom.to_vec();
        for c in s.encode_utf16() {
            raw.extend(if little {
                c.to_le_bytes()
            } else {
                c.to_be_bytes()
            });
        }
        out.push(raw);
    }
    if s.is_ascii() {
        for encoding in ["ISO-8859-1", "US-ASCII"] {
            out.push(format!("<?xml version='1.0' encoding='{encoding}'?>{s}").into_bytes());
        }
    }
    out
}

#[test]
fn xml_elements_preserve_implicit_binding_and_original_prefix() {
    for source in [
        "<xml:r/>",
        "<xml:r></xml:r>",
        "<r><xml:c xml:lang='de'/></r>",
        "<xml:r xmlns:xml='http://www.w3.org/XML/1998/namespace'></xml:r>",
    ] {
        for raw in variants(source) {
            let d = SourceDocument::parse(&raw).unwrap();
            assert_eq!(d.original_bytes(), raw);
            let n = d
                .nodes()
                .find(|n| n.name().is_some_and(|n| n.prefix() == Some("xml")))
                .unwrap();
            assert_eq!(
                n.name().unwrap().namespace_uri(),
                "http://www.w3.org/XML/1998/namespace"
            );
        }
    }
    for source in [
        "<xmlns:r/>",
        "<xml:r xmlns:xml='urn:wrong'/>",
        "<xml:r xmlns:xml=''/>",
        "<r xmlns:p='http://www.w3.org/XML/1998/namespace'/>",
        "<xml:r></r>",
    ] {
        for raw in variants(source) {
            assert!(matches!(
                SourceDocument::parse(&raw),
                Err(SourceError::Syntax { .. })
            ));
        }
    }
}

#[test]
fn relative_default_namespace_values_are_not_reserved_prefixes() {
    for (value, expected) in [
        ("xml", "xml"),
        ("xmlns", "xmlns"),
        ("xm&#108;", "xml"),
        ("xmln&#x73;", "xmlns"),
    ] {
        for raw in variants(&format!("<r xmlns='{value}'><c/></r>")) {
            let d = SourceDocument::parse(&raw).unwrap();
            assert_eq!(d.original_bytes(), raw);
            for n in d.nodes().filter(|n| n.kind() == NodeKind::Element) {
                assert_eq!(n.name().unwrap().namespace_uri(), expected);
            }
        }
    }
    for source in [
        "<r xmlns='http://www.w3.org/XML/1998/namespace'/>",
        "<r xmlns='http://www.w3.org/2000/xmln&#115;/'/>",
        "<r xmlns:xml='xml'/>",
        "<r xmlns:xmlns='xmlns'/>",
    ] {
        for raw in variants(source) {
            assert!(matches!(
                SourceDocument::parse(&raw),
                Err(SourceError::Syntax { .. })
            ));
        }
    }
}

#[test]
fn pi_separator_excludes_only_leading_xml_whitespace() {
    for (source, expected) in [
        ("<r><?p   a  ?></r>", "a  "),
        ("<r><?p \t\r\n a  ?></r>", "a  "),
        ("<r><?p  ?></r>", ""),
        ("<r><?p?></r>", ""),
        ("<r><?π  \u{a0}a \t?></r>", "\u{a0}a \t"),
    ] {
        for raw in variants(source) {
            let d = SourceDocument::parse(&raw).unwrap();
            assert_eq!(d.original_bytes(), raw);
            let n = d
                .nodes()
                .find(|n| n.kind() == NodeKind::ProcessingInstruction)
                .unwrap();
            assert_eq!(n.content(), Some(expected));
            assert_eq!(
                n.string_value(&mut QueryBudget::new(10000, 10000).unwrap())
                    .unwrap(),
                expected
            );
        }
    }
}

#[test]
fn pi_targets_are_ncnames_without_restricting_valid_unicode() {
    for source in [
        "<r><?p:q data?></r>",
        "<?: data?><r/>",
        "<r/><?p:q?>",
        "<r xmlns:p='urn:p'><?p:q?></r>",
    ] {
        for raw in variants(source) {
            assert!(matches!(
                SourceDocument::parse(&raw),
                Err(SourceError::Syntax { .. })
            ));
        }
    }
    // XML 1.0 Fifth Edition permits supplementary NameStartChar even though
    // the pinned Java parser rejects this control. It is not an oracle pass.
    for source in ["<r><?π data?></r>", "<r><?\u{10000} data?></r>"] {
        for raw in variants(source) {
            SourceDocument::parse(&raw).unwrap();
        }
    }
}

#[test]
fn namespace_admission_retains_inheritance_aliases_shadows_and_references() {
    let source = "<p:r xmlns:p='urn:&#x75;' xmlns:q='urn:u' xmlns='urn:def'><a p:x='' q:y=''/><q:b xmlns:p='urn:other' p:x='' xml:lang='en'/><c xmlns=''/></p:r>";
    let d = SourceDocument::parse(source.as_bytes()).unwrap();
    let names: Vec<_> = d
        .nodes()
        .filter_map(|n| n.name().map(|v| (v.local(), v.namespace_uri())))
        .collect();
    assert_eq!(
        names,
        [("r", "urn:u"), ("a", "urn:def"), ("b", "urn:u"), ("c", "")]
    );
    let tiny = SourceLimits {
        string_bytes: 80,
        ..SourceLimits::default()
    };
    assert!(matches!(
        SourceDocument::parse_with_limits(source.as_bytes(), &tiny),
        Err(SourceError::Resource(_))
    ));
}
