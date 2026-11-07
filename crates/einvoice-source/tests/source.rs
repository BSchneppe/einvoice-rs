use einvoice_source::{
    Encoding, EvidenceSpan, LedgerKind, LedgerOwner, Node, NodeKind, OriginalScalarSpan,
    OriginalUtf8Span, QueryBudget, SourceDocument, SourceError, SourceLimits, SourceLocation,
};
use std::sync::{atomic::AtomicBool, Arc};

fn parse(s: &str) -> Arc<SourceDocument> {
    SourceDocument::parse(s.as_bytes()).unwrap()
}
fn budget() -> QueryBudget {
    QueryBudget::new(1_000_000, 1_000_000).unwrap()
}
fn path(n: Node<'_>) -> String {
    n.canonical_path(&mut budget()).unwrap()
}
fn value(n: Node<'_>) -> String {
    n.string_value(&mut budget()).unwrap()
}
fn utf16(s: &str, little: bool) -> Vec<u8> {
    let mut raw = if little {
        vec![255, 254]
    } else {
        vec![254, 255]
    };
    for u in s.encode_utf16() {
        raw.extend(if little {
            u.to_le_bytes()
        } else {
            u.to_be_bytes()
        });
    }
    raw
}
fn variants(s: &str) -> Vec<Vec<u8>> {
    vec![
        s.as_bytes().to_vec(),
        format!("\u{feff}{s}").into_bytes(),
        utf16(s, true),
        utf16(s, false),
    ]
}
fn syntax(raw: &[u8]) {
    assert!(
        matches!(SourceDocument::parse(raw), Err(SourceError::Syntax { .. })),
        "{raw:?}: {:?}",
        SourceDocument::parse(raw)
    );
}
fn resource(raw: &[u8], limits: SourceLimits, dimension: &str) {
    match SourceDocument::parse_with_limits(raw, &limits) {
        Err(SourceError::Resource(r)) => assert_eq!(r.dimension, dimension),
        r => panic!("expected {dimension}, got {r:?}"),
    }
}

#[test]
fn owned_send_sync_clone_and_distinct_document_identity() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<SourceDocument>();
    let mut caller = b"<r a='v'><c/>text</r>".to_vec();
    let original = caller.clone();
    let a = SourceDocument::parse(&caller).unwrap();
    caller.fill(b'x');
    assert_eq!(a.original_bytes(), original);
    let clone = Arc::clone(&a);
    let b = SourceDocument::parse(&original).unwrap();
    assert!(Arc::ptr_eq(&a, &clone));
    assert_eq!(a.root_element(), clone.root_element());
    assert_ne!(a.root_element().identity(), b.root_element().identity());
    assert_eq!(a.root_element().compare_order(b.root_element()), None);
    assert!(a
        .root_element()
        .order()
        .partial_cmp(&b.root_element().order())
        .is_none());
    assert_eq!(
        a.root_element().attributes().next().unwrap().owner(),
        a.root_element()
    );
    assert_ne!(
        a.root_element().identity(),
        a.root_element().attributes().next().unwrap().identity()
    );
    assert_ne!(
        a.root_element().identity(),
        a.root_element().namespaces().next().unwrap().identity()
    );
}
#[test]
fn mixed_content_and_document_level_nodes() {
    let d=parse("<?pre x?><r>A<![CDATA[B]]>&amp;C<!--cut-->D<?pi v?>E<![CDATA[]]><![CDATA[F]]><n/>G</r><!--post-->");
    let root = d.root_element();
    let children: Vec<_> = root.children().collect();
    assert_eq!(
        children.iter().map(|n| n.kind()).collect::<Vec<_>>(),
        vec![
            NodeKind::Text,
            NodeKind::Comment,
            NodeKind::Text,
            NodeKind::ProcessingInstruction,
            NodeKind::Text,
            NodeKind::Element,
            NodeKind::Text
        ]
    );
    assert_eq!(
        children
            .iter()
            .filter(|n| n.kind() == NodeKind::Text)
            .map(|n| n.content().unwrap())
            .collect::<Vec<_>>(),
        vec!["AB&C", "D", "EF", "G"]
    );
    assert_eq!(value(root), "AB&CDEFG");
    assert_eq!(value(d.document()), "AB&CDEFG");
    assert_eq!(
        d.document()
            .children()
            .map(|n| n.kind())
            .collect::<Vec<_>>(),
        vec![
            NodeKind::ProcessingInstruction,
            NodeKind::Element,
            NodeKind::Comment
        ]
    );
    assert_eq!(
        children[0].pieces().map(|p| p.source()).collect::<Vec<_>>(),
        vec!["A", "<![CDATA[B]]>", "&amp;C"]
    );
    assert_eq!(
        children[4]
            .pieces()
            .filter(|p| p.kind() == LedgerKind::Cdata)
            .count(),
        2
    );
}
#[test]
fn literal_normalization_precedes_reference_expansion() {
    let input="<r a='X\r\nY\tZ&#xD;&#xA;&#x9;'>A\r\nB\rC&#xD;&#xA;<![CDATA[D\r\nE]]><!--C\r\nD--><?pi P\r\nQ?></r>";
    for raw in variants(input) {
        let d = SourceDocument::parse(&raw).unwrap();
        let r = d.root_element();
        assert_eq!(r.attributes().next().unwrap().value(), "X Y Z\r\n\t");
        assert_eq!(
            r.children().next().unwrap().content(),
            Some("A\nB\nC\r\nD\nE")
        );
        assert_eq!(r.children().nth(1).unwrap().content(), Some("C\nD"));
        assert_eq!(r.children().nth(2).unwrap().content(), Some("P\nQ"));
        assert_eq!(d.original_bytes(), raw);
        assert_eq!(d.source_view(), input);
    }
}
#[test]
fn all_encodings_supplementary_values_and_exact_source() {
    for (raw, encoding) in [
        (
            b"<?xml version='1.0' encoding='UTF-8'?><r>\xf0\x9f\x98\x80\r\n<a x='&#x1F600;'/></r>"
                .to_vec(),
            Encoding::Utf8,
        ),
        (
            utf16(
                "<?xml version='1.0' encoding='UTF-16'?><r>😀\r\n<a x='&#x1F600;'/></r>",
                true,
            ),
            Encoding::Utf16Le,
        ),
        (
            utf16(
                "<?xml version='1.0' encoding='UTF-16'?><r>😀\r\n<a x='&#x1F600;'/></r>",
                false,
            ),
            Encoding::Utf16Be,
        ),
        (
            b"<?xml version='1.0' encoding='ISO-8859-1'?><r>caf\xe9\r\n<a x='&#x1F600;'/></r>"
                .to_vec(),
            Encoding::Latin1,
        ),
        (
            b"<?xml version='1.0' encoding='US-ASCII'?><r>&#x1F600;</r>".to_vec(),
            Encoding::Ascii,
        ),
    ] {
        let d = SourceDocument::parse(&raw).unwrap();
        assert_eq!(d.encoding(), encoding);
        assert_eq!(d.original_bytes(), raw);
        if let Some(a) = d.nodes().flat_map(|n| n.attributes()).next() {
            assert_eq!(a.value(), "😀");
            assert!(a.token().source().ends_with("'&#x1F600;'"));
            assert_eq!(a.token().location(), SourceLocation { line: 2, column: 4 });
        } else {
            assert_eq!(value(d.root_element()), "😀");
        };
    }
}
#[test]
fn header_grammar_versions_encoding_labels_and_conflicts() {
    for valid in [
        "<?xml\nversion='1.0'?><r/>",
        "<?xml\r\nversion = '1.0'\t encoding='uTf-8' standalone='yes'?><r/>",
        "<?xml version='1.0' encoding='latin1'?><r/>",
    ] {
        assert!(SourceDocument::parse(valid.as_bytes()).is_ok(), "{valid}");
    }
    for invalid in [
        "",
        "<r/",
        "<r>",
        "<r></x>",
        "<r/><s/>",
        "text<r/>",
        "<r/>tail",
        "<?xml?><r/>",
        "<?xml encoding='UTF-8' version='1.0'?><r/>",
        " <?xml version='1.0'?><r/>",
        "<?xml version='1.0'version='1.0'?><r/>",
        "<?xml version='1.0' standalone='maybe'?><r/>",
        "<?XML version='1.0'?><r/>",
        "<?xml version='1.&#48;'?><r/>",
    ] {
        syntax(invalid.as_bytes());
    }
    for label in [
        "UTF",
        "UTF-8-extra",
        "ISO",
        "UTF-16LE",
        "windows-1252",
        "EBCDIC",
        "X-UNKNOWN",
    ] {
        let input = format!("<?xml version='1.0' encoding='{label}'?><r/>");
        assert!(
            matches!(
                SourceDocument::parse(input.as_bytes()),
                Err(SourceError::UnsupportedEncoding(_))
            ),
            "{label}: {:?}",
            SourceDocument::parse(input.as_bytes())
        );
    }
    for raw in variants("<?xml version='1.1'?><r>\u{85}</r>") {
        assert!(matches!(
            SourceDocument::parse(&raw),
            Err(SourceError::UnsupportedVersion(_))
        ));
    }
    for raw in [
        utf16("<?xml version='1.0' encoding='UTF-8'?><r/>", true),
        utf16("<?xml version='1.0' encoding='ISO-8859-1'?><r/>", false),
        b"\xef\xbb\xbf<?xml version='1.0' encoding='Latin1'?><r/>".to_vec(),
        b"<?xml version='1.0' encoding='UTF-16'?><r/>".to_vec(),
    ] {
        syntax(&raw);
    }
}
#[test]
fn decoding_is_never_lossy() {
    for raw in [
        b"<r>\xff</r>".to_vec(),
        b"\xff\xfe<\x00r\x00>\x00\x00\xd8<\x00/\x00r\x00>\x00".to_vec(),
        vec![255, 254, 60],
        b"<?xml version='1.0' encoding='ASCII'?><r>\x80</r>".to_vec(),
    ] {
        syntax(&raw);
    }
    let d = parse("<r>�</r>");
    assert_eq!(value(d.root_element()), "�");
}
#[test]
fn namespace_names_frames_rebinding_and_source_attribute_order() {
    let d=parse("<p:r xmlns='urn:def' xmlns:p='urn:a' xmlns:q='urn:a' x='1' p:x='2'><q:k/><p:k/><c xmlns='' xmlns:p='urn:b' p:x='3'><p:k/></c></p:r>");
    let r = d.root_element();
    let c = r.children().nth(2).unwrap();
    assert_eq!(r.name().unwrap().prefix(), Some("p"));
    assert_eq!(r.name().unwrap().namespace_uri(), "urn:a");
    assert_eq!(
        r.attributes()
            .map(|a| (a.name().namespace_uri(), a.value()))
            .collect::<Vec<_>>(),
        vec![("", "1"), ("urn:a", "2")]
    );
    assert_eq!(
        r.declarations()
            .map(|n| (n.prefix(), n.uri()))
            .collect::<Vec<_>>(),
        vec![("", "urn:def"), ("p", "urn:a"), ("q", "urn:a")]
    );
    assert_eq!(
        path(r.children().next().unwrap()),
        "/Q{urn:a}r[1]/Q{urn:a}k[1]"
    );
    assert_eq!(
        path(r.children().nth(1).unwrap()),
        "/Q{urn:a}r[1]/Q{urn:a}k[2]"
    );
    assert_eq!(c.name().unwrap().namespace_uri(), "");
    assert_eq!(
        c.children().next().unwrap().name().unwrap().namespace_uri(),
        "urn:b"
    );
    assert_eq!(
        c.namespaces()
            .map(|n| (n.prefix(), n.uri()))
            .collect::<Vec<_>>(),
        vec![
            ("p", "urn:b"),
            ("q", "urn:a"),
            ("xml", "http://www.w3.org/XML/1998/namespace")
        ]
    );
    let attrs: Vec<_> = r.attributes().collect();
    let namespaces: Vec<_> = r.namespaces().collect();
    assert!(r.order() < namespaces[0].order());
    assert!(namespaces.last().unwrap().order() < attrs[0].order());
    assert!(attrs[0].order() < attrs[1].order());
    assert!(attrs[1].order() < r.children().next().unwrap().order());
    assert_ne!(
        r.namespaces()
            .find(|n| n.prefix() == "q")
            .unwrap()
            .identity(),
        c.namespaces()
            .find(|n| n.prefix() == "q")
            .unwrap()
            .identity()
    );
}
#[test]
fn declarations_preserve_redundancy_aliases_and_normalized_references() {
    let d=parse("<r xmlns:p='urn:a' xmlns:q='urn:a' xmlns:xml='http://www.w3.org/XML/1998/namespace'><c xmlns:p='urn:a' xmlns='urn:&#x41;' xml:lang='en'/></r>");
    let c = d.root_element().children().next().unwrap();
    assert_eq!(
        c.declarations()
            .map(|n| (n.prefix(), n.uri()))
            .collect::<Vec<_>>(),
        vec![("p", "urn:a"), ("", "urn:A")]
    );
    assert_eq!(c.name().unwrap().namespace_uri(), "urn:A");
    assert_eq!(
        c.attributes().next().unwrap().name().namespace_uri(),
        "http://www.w3.org/XML/1998/namespace"
    );
    assert_eq!(c.namespaces().filter(|n| n.prefix() == "p").count(), 1);
    assert_eq!(
        c.namespaces()
            .find(|n| n.prefix() == "xml")
            .unwrap()
            .declaration()
            .unwrap()
            .owner(),
        d.root_element()
    );
}
#[test]
fn duplicate_namespace_and_expanded_ordinary_attributes_are_refused() {
    for input in ["<r xmlns:p='u' xmlns:p='v'/>","<r xmlns='u' xmlns='v'/>","<r xmlns:xml='http://www.w3.org/XML/1998/namespace' xmlns:xml='http://www.w3.org/XML/1998/namespace'/>","<r xmlns:p='u' xmlns:q='u' p:x='1' q:x='2'/>","<r a='1' a='2'/>"]{
        for raw in variants(input){syntax(&raw);}
    }
    assert!(SourceDocument::parse(b"<r xmlns:p='u' xmlns:q='v' p:x='1' q:x='2' x='3'/>").is_ok());
}
#[test]
fn illegal_namespace_bindings_and_unbound_names_are_syntax() {
    for input in [
        "<p:r/>",
        "<r p:x='1'/>",
        "<r xmlns:p=''/>",
        "<r xmlns:xmlns='u'/>",
        "<r xmlns:xml='u'/>",
        "<r xmlns:p='http://www.w3.org/XML/1998/namespace'/>",
        "<r xmlns:p='http://www.w3.org/2000/xmlns/'/>",
        "<r xmlns='http://www.w3.org/XML/1998/namespace'/>",
        "<xmlns:r/>",
    ] {
        syntax(input.as_bytes());
    }
}
#[test]
fn complete_ledger_reconstructs_original_view_and_owner_associations() {
    let input="<?xml version='1.0'?>\r\n<?pre x?><r  xmlns='u'\n a = 'v' > A<![CDATA[]]>B<!--c--><?pi x?><n/> \r\n</r>\t<!--post-->\n";
    for raw in variants(input) {
        let d = SourceDocument::parse(&raw).unwrap();
        let mut replay = String::new();
        let mut declaration = 0;
        let mut empty = 0;
        for token in d.ledger() {
            replay.push_str(token.gap_before());
            replay.push_str(token.source());
            match token.owner() {
                LedgerOwner::Node(n) => {
                    assert!(d.nodes().any(|own| own == n));
                    assert!(n
                        .pieces()
                        .any(|p| p.span() == token.span() && p.kind() == token.kind()));
                }
                LedgerOwner::Attribute(a) => {
                    assert_eq!(a.token().span(), token.span());
                    assert_eq!(token.kind(), LedgerKind::Attribute);
                }
                LedgerOwner::Declaration(n) => {
                    assert_eq!(n.token().span(), token.span());
                    assert_eq!(token.kind(), LedgerKind::NamespaceDeclaration);
                }
            }
            if token.kind() == LedgerKind::Declaration {
                declaration += 1;
            }
            if token.kind() == LedgerKind::Cdata {
                assert_eq!(token.source(), "<![CDATA[]]>");
                empty += 1;
            }
        }
        replay.push_str(d.trailing_gap());
        assert_eq!(replay, input);
        assert_eq!(declaration, 1);
        assert_eq!(empty, 1);
        assert!(d
            .ledger()
            .any(|l| l.kind() == LedgerKind::DocumentWhitespace));
        assert!(!d
            .nodes()
            .any(|n| n.kind() == NodeKind::ProcessingInstruction
                && n.name().unwrap().local() == "xml"));
    }
}
#[test]
fn empty_cdata_is_lexical_only_and_never_splits_text() {
    for (input, count, text) in [
        ("<r><![CDATA[]]></r>", 2, ""),
        ("<r><![CDATA[]]><!--x--><![CDATA[]]></r>", 3, ""),
        ("<r>A<![CDATA[]]>B</r>", 3, "AB"),
    ] {
        let d = parse(input);
        assert_eq!(d.nodes().len(), count);
        assert_eq!(value(d.root_element()), text);
        assert!(d.ledger().any(|p| p.kind() == LedgerKind::Cdata));
    }
}
#[test]
fn spans_are_original_units_and_attributes_are_exact() {
    let input = "<r>é</r>";
    let d = SourceDocument::parse(&utf16(input, true)).unwrap();
    assert_eq!(d.original_bytes().len(), 18);
    assert_eq!(d.source_view().chars().count(), 8);
    assert_eq!(
        d.root_element().covering_span(),
        EvidenceSpan::UnicodeScalar(OriginalScalarSpan { start: 0, end: 8 })
    );
    let d = parse(input);
    assert_eq!(
        d.root_element().covering_span(),
        EvidenceSpan::Utf8Byte(OriginalUtf8Span { start: 0, end: 9 })
    );
    let d = parse("\u{feff}<r>😀<a x='&#13;\r\n😀'/></r>");
    let a = d.nodes().flat_map(|n| n.attributes()).next().unwrap();
    assert_eq!(a.token().source(), "x='&#13;\r\n😀'");
    assert_eq!(a.token().location(), SourceLocation { line: 1, column: 8 });
    assert_ne!(a.token().span(), a.owner_fallback_span());
    let EvidenceSpan::Utf8Byte(span) = a.value_span() else {
        panic!()
    };
    assert_eq!(
        &d.original_bytes()[span.start..span.end],
        "&#13;\r\n😀".as_bytes()
    );
    assert_eq!(
        d.root_element().covering_span(),
        EvidenceSpan::Utf8Byte(OriginalUtf8Span {
            start: 3,
            end: d.original_bytes().len()
        })
    );
    let d = SourceDocument::parse(&utf16("<r>😀<a x='&#13;\r\n😀'/></r>", false)).unwrap();
    let a = d.nodes().flat_map(|n| n.attributes()).next().unwrap();
    assert_eq!(
        a.value_span(),
        EvidenceSpan::UnicodeScalar(OriginalScalarSpan { start: 10, end: 18 })
    );
}
#[test]
fn occurrences_paths_and_text_pieces_are_not_a_covering_token_claim() {
    let d =
        parse("<?pre?><r>a<![CDATA[b]]>&amp;<!--x-->c<?pi?>d<?other?><?pi?><k/><k/><!--y--></r>");
    let children: Vec<_> = d.root_element().children().collect();
    assert_eq!(path(children[0]), "/Q{}r[1]/text()[1]");
    assert_eq!(path(children[2]), "/Q{}r[1]/text()[2]");
    assert_eq!(path(children[4]), "/Q{}r[1]/text()[3]");
    assert_eq!(
        path(children[6]),
        "/Q{}r[1]/processing-instruction(\"pi\")[2]"
    );
    assert_eq!(path(children[8]), "/Q{}r[1]/Q{}k[2]");
    assert_eq!(path(children[9]), "/Q{}r[1]/comment()[2]");
    assert_eq!(children[0].pieces().count(), 3);
    assert!(children[0]
        .pieces()
        .all(|p| p.span() != children[0].covering_span()));
    let d = parse("<r a='1'/>");
    assert_eq!(
        d.root_element()
            .attributes()
            .next()
            .unwrap()
            .canonical_path(&mut budget())
            .unwrap(),
        "/Q{}r[1]/@Q{}a"
    );
}
#[test]
fn dtds_never_reach_expansion_or_a_resolver() {
    for input in [
        "<!DOCTYPE><r/>",
        "<!DOCTYPE r><r/>",
        "<?xml version='1.0'?>\r\n<!--p--><?pi x?><!DOCTYPE r [<!ENTITY % a 'x'>%a;]><r/>",
        "<!DOCTYPE r SYSTEM 'file:///must-not-read'><r/>",
        "<!DOCTYPE r [<!ENTITY e SYSTEM 'https://must-not-fetch.invalid/'>]><r>&e;</r>",
        "<!DOCTYPE r><?xml version='1.0'?><r/>",
    ] {
        for raw in variants(input) {
            assert!(
                matches!(
                    SourceDocument::parse(&raw),
                    Err(SourceError::UnsupportedDtd)
                ),
                "{input}"
            );
        }
    }
    for input in [
        "<r><!DOCTYPE x [<!ENTITY e 'x'>]></r>",
        "<r/><!DOCTYPE x>",
        "<r/><!DOCTYPE r [<!ENTITY % a 'x'>%a;]>",
        "<!ENTITY e 'x'><r/>",
        "<!ATTLIST r x CDATA 'x'><r/>",
        "<r x='<!DOCTYPE r>'/>",
        "<r><!DOCTYPE r SYSTEM 'file:///must-not-read'></r>",
    ] {
        for raw in variants(input) {
            assert!(SourceDocument::parse(&raw).is_err());
        }
    }
    let input="<?xml\nversion='1.0'?><!-- <!DOCTYPE r> --><?pi <!DOCTYPE r> ?><r><![CDATA[<!DOCTYPE r>]]></r>";
    for raw in variants(input) {
        assert!(SourceDocument::parse(&raw).is_ok());
    }
    let latin = b"<?xml version='1.0' encoding='Latin1'?><!--\xe9 <!DOCTYPE r> --><r/>";
    assert!(SourceDocument::parse(latin).is_ok());
}
#[test]
fn malformed_references_and_eof_are_not_partial_success() {
    for input in [
        "<r>&unknown;</r>",
        "<r>&#0;</r>",
        "<r>&#xD800;</r>",
        "<r>&#x110000;</r>",
        "<r>&amp</r>",
        "<r a='&unknown;'/>",
        "<r><![CDATA[unterminated</r>",
        "<r><!--unfinished</r>",
        "<r><?pi unfinished</r>",
        "<r><c/></r><",
        "<r>]]></r>",
    ] {
        syntax(input.as_bytes());
    }
}
#[test]
fn original_coordinates_survive_crlf_bom_multibyte_and_attribute_normalization() {
    // This failure is from semantic name binding after an attribute's three
    // physical newlines have disappeared from the normalized parser view.
    for newline in ["\n", "\r", "\r\n"] {
        let input = format!("<r a='é{newline}😀{newline}x'>{newline}<p:c/></r>");
        for raw in variants(&input) {
            let e = SourceDocument::parse(&raw).unwrap_err();
            let SourceError::Syntax {
                location: Some(location),
                ..
            } = e
            else {
                panic!("{e:?}")
            };
            assert_eq!(location, SourceLocation { line: 4, column: 5 }, "{input:?}");
        }
        let input = format!("<r>{newline}<c a='1'{newline}a='2'/></r>");
        for raw in variants(&input) {
            let SourceError::Syntax {
                location: Some(location),
                ..
            } = SourceDocument::parse(&raw).unwrap_err()
            else {
                panic!()
            };
            assert_eq!(location, SourceLocation { line: 3, column: 1 });
        }
    }
}
#[test]
fn indexed_coordinate_checkpoints_cover_long_lines_and_crlf_boundaries() {
    let prefix = "😀\r\n".repeat(150);
    let input = format!("<r>{prefix}<a x='1'/></r>");
    for raw in variants(&input) {
        let d = SourceDocument::parse(&raw).unwrap();
        let a = d.nodes().flat_map(|n| n.attributes()).next().unwrap();
        assert_eq!(
            a.token().location(),
            SourceLocation {
                line: 151,
                column: 4
            }
        );
    }
    let input = format!(
        "<r a='{}\r\n{}'><p:c/></r>",
        "x".repeat(200),
        "😀".repeat(200)
    );
    let SourceError::Syntax {
        location: Some(location),
        ..
    } = SourceDocument::parse(input.as_bytes()).unwrap_err()
    else {
        panic!()
    };
    assert_eq!(
        location,
        SourceLocation {
            line: 2,
            column: 207
        }
    );
}
#[test]
fn explicit_profile_direct_text_existential_semantics() {
    for (body, expected) in [
        ("<ID>ab<![CDATA[cd]]>&#101;</ID>", true),
        ("<ID>ab<!--x-->cde</ID>", false),
        ("<ID>ab<?x?>cde</ID>", false),
        ("<ID>wrong</ID><ID>abcde</ID>", true),
        ("<ID>abcde<!--x-->other</ID>", true),
        ("<ID> abcde </ID>", false),
        ("<ID><c>abcde</c></ID>", false),
    ] {
        let d = parse(&format!("<r>{body}</r>"));
        let found = d
            .root_element()
            .children()
            .filter(|n| n.kind() == NodeKind::Element && n.name().unwrap().local() == "ID")
            .any(|id| {
                id.children()
                    .any(|t| t.kind() == NodeKind::Text && t.content() == Some("abcde"))
            });
        assert_eq!(found, expected, "{body}");
    }
}
#[test]
fn original_input_depth_token_node_attribute_and_namespace_budgets() {
    let defaults = SourceLimits::default();
    assert_eq!(defaults.original_bytes, 16 << 20);
    assert_eq!(defaults.depth, 128);
    resource(
        b"<r/>",
        SourceLimits {
            original_bytes: 3,
            ..defaults
        },
        "original_bytes",
    );
    assert!(SourceDocument::parse_with_limits(
        b"<r/>",
        &SourceLimits {
            original_bytes: 4,
            depth: 1,
            ..defaults
        }
    )
    .is_ok());
    resource(
        b"<r><c/></r>",
        SourceLimits {
            depth: 1,
            ..defaults
        },
        "depth",
    );
    resource(
        b"<r><c/></r>",
        SourceLimits {
            nodes: 2,
            ..defaults
        },
        "nodes",
    );
    resource(
        b"<r/>",
        SourceLimits {
            lexical_tokens: 1,
            ..defaults
        },
        "lexical_tokens",
    );
    resource(
        b"<r a='12345'/>",
        SourceLimits {
            token_bytes: 6,
            ..defaults
        },
        "token_bytes",
    );
    resource(
        b"<long/>",
        SourceLimits {
            name_bytes: 3,
            ..defaults
        },
        "name_bytes",
    );
    resource(
        b"<r xmlns:p='u' a='1'/>",
        SourceLimits {
            attributes: 1,
            ..defaults
        },
        "attributes",
    );
    resource(
        b"<r xmlns:p='u' xmlns:q='v'/>",
        SourceLimits {
            local_namespaces: 1,
            ..defaults
        },
        "local_namespaces",
    );
    resource(
        b"<r xmlns:p='u'><c xmlns:q='v'/></r>",
        SourceLimits {
            effective_namespaces: 2,
            ..defaults
        },
        "effective_namespaces",
    );
    resource(
        b"<r xmlns:p='long'/>",
        SourceLimits {
            uri_bytes: 3,
            ..defaults
        },
        "uri_bytes",
    );
    resource(
        b"<r/>",
        SourceLimits {
            namespace_bytes: 1,
            ..defaults
        },
        "namespace_bytes",
    );
    resource(
        b"<?xml version='1.0'?><r/>",
        SourceLimits {
            header_bytes: 6,
            ..defaults
        },
        "header_bytes",
    );
}
#[test]
fn decode_normalized_values_total_storage_pieces_and_work_budgets() {
    let d = SourceLimits::default();
    resource(
        b"<r/>",
        SourceLimits {
            decoded_bytes: 3,
            ..d
        },
        "decoded_bytes",
    );
    resource(
        &utf16("<r/>", true),
        SourceLimits {
            normalized_bytes: 8,
            ..d
        },
        "normalized_bytes",
    );
    resource(
        b"<r a='1234'/>",
        SourceLimits {
            value_bytes: 3,
            ..d
        },
        "value_bytes",
    );
    resource(
        b"<r>AB<![CDATA[CD]]></r>",
        SourceLimits {
            value_bytes: 3,
            ..d
        },
        "value_bytes",
    );
    resource(
        b"<r><![CDATA[1234]]></r>",
        SourceLimits {
            value_bytes: 3,
            ..d
        },
        "value_bytes",
    );
    resource(
        b"<r a='12' b='34'/>",
        SourceLimits {
            expanded_bytes: 3,
            ..d
        },
        "expanded_bytes",
    );
    resource(
        b"<r/>",
        SourceLimits {
            string_bytes: 3,
            ..d
        },
        "string_bytes",
    );
    resource(
        b"<r/>",
        SourceLimits {
            storage_bytes: 3,
            ..d
        },
        "storage_bytes",
    );
    resource(b"<r/>", SourceLimits { pieces: 1, ..d }, "pieces");
    resource(b"<r/>", SourceLimits { work: 3, ..d }, "work");
}
#[test]
fn overflow_unlimited_and_cancellation_are_typed_before_growth() {
    let d = SourceLimits::default();
    for limits in [
        SourceLimits {
            work: usize::MAX,
            ..d
        },
        SourceLimits {
            nodes: usize::MAX,
            ..d
        },
        SourceLimits { depth: 0, ..d },
        SourceLimits {
            storage_bytes: usize::MAX,
            ..d
        },
        SourceLimits {
            lexical_tokens: u32::MAX as usize + 1,
            ..d
        },
    ] {
        assert!(matches!(
            SourceDocument::parse_with_limits(b"<r/>", &limits),
            Err(SourceError::InvalidLimits(_))
        ));
    }
    let cancel = AtomicBool::new(true);
    assert_eq!(
        SourceDocument::parse_cancellable(b"<r/>", &d, &cancel).unwrap_err(),
        SourceError::Cancelled
    );
    assert!(QueryBudget::new(usize::MAX, 1).is_err());
    assert!(QueryBudget::new(0, 1).is_err());
}
#[test]
fn rendering_is_bounded_and_streaming_writer_errors_are_explicit() {
    let d = parse("<r>abc<c>def</c><!--ignore--><?pi ignore?></r>");
    assert_eq!(value(d.root_element()), "abcdef");
    assert!(matches!(
        d.root_element()
            .string_value(&mut QueryBudget::new(100, 5).unwrap()),
        Err(SourceError::Resource(_))
    ));
    assert!(matches!(
        d.root_element()
            .canonical_path(&mut QueryBudget::new(100, 3).unwrap()),
        Err(SourceError::Resource(_))
    ));
    assert!(matches!(
        d.document()
            .string_value(&mut QueryBudget::new(1, 100).unwrap()),
        Err(SourceError::Resource(_))
    ));
    struct Refuse;
    impl std::fmt::Write for Refuse {
        fn write_str(&mut self, _: &str) -> std::fmt::Result {
            Err(std::fmt::Error)
        }
    }
    assert_eq!(
        d.root_element()
            .write_string_value(&mut Refuse, &mut budget())
            .unwrap_err(),
        SourceError::Output
    );
}

#[test]
fn strict_unsupported_version_and_header_token_admission() {
    for version in ["1.1", "1.2", "1.10"] {
        for raw in variants(&format!("<?xml version='{version}'?><r/>")) {
            assert!(matches!(
                SourceDocument::parse(&raw),
                Err(SourceError::UnsupportedVersion(_))
            ));
        }
    }
    for label in ["", "8utf", "-utf", "utf 8", "utf&#45;8"] {
        syntax(format!("<?xml version='1.0' encoding='{label}'?><r/>").as_bytes());
    }
    resource(
        b"<?xml version='1.0'?><r/>",
        SourceLimits {
            token_bytes: 10,
            ..SourceLimits::default()
        },
        "token_bytes",
    );
    assert!(SourceDocument::parse_with_limits(
        b"<r/>",
        &SourceLimits {
            effective_namespaces: 1,
            ..SourceLimits::default()
        }
    )
    .is_ok());
}
#[test]
fn shadowed_namespace_payloads_and_lookup_work_are_admitted_before_sax() {
    let xml="<r xmlns:p='urn:00000000000000000000'><c xmlns:p='urn:11111111111111111111'><d xmlns:p='urn:22222222222222222222'/></c></r>";
    resource(
        xml.as_bytes(),
        SourceLimits {
            namespace_bytes: 1000,
            ..SourceLimits::default()
        },
        "namespace_bytes",
    );
    // The expanded-name namespace axis is small, but construction also charges
    // shadowed frame payloads. This closes a different dimension than cardinality.
    let d = parse(xml);
    assert_eq!(
        d.root_element()
            .children()
            .next()
            .unwrap()
            .children()
            .next()
            .unwrap()
            .namespaces()
            .count(),
        2
    );
}
#[test]
fn repeated_construction_errors_do_not_publish_partial_state() {
    for _ in 0..20 {
        let a = parse("<r><c/>good</r>");
        let id = a.document().identity();
        syntax(b"<r><c/></wrong>");
        resource(
            b"<r><c/></r>",
            SourceLimits {
                nodes: 2,
                ..SourceLimits::default()
            },
            "nodes",
        );
        let b = parse("<r><c/>good</r>");
        assert_ne!(id, b.document().identity());
        assert_eq!(value(b.root_element()), "good");
        assert_eq!(
            path(b.root_element().children().next().unwrap()),
            "/Q{}r[1]/Q{}c[1]"
        );
    }
}

#[test]
fn exact_namespaced_profile_path_and_prefix_renaming() {
    const CII: &str = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100";
    const RAM: &str =
        "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100";
    for (body, expected) in [
        ("<ram:ID>ab<![CDATA[cd]]>&#101;</ram:ID>", true),
        ("<ram:ID>ab<!--x-->cde</ram:ID>", false),
        ("<ram:ID>ab<?x?>cde</ram:ID>", false),
        ("<ram:ID>wrong</ram:ID><ram:ID>abcde</ram:ID>", true),
        ("<ram:ID>abcde<!--x-->other</ram:ID>", true),
        ("<ram:ID> abcde </ram:ID>", false),
        ("<ram:ID><ram:c>abcde</ram:c></ram:ID>", false),
        ("<ID>abcde</ID>", false),
    ] {
        let xml=format!("<rsm:CrossIndustryInvoice xmlns:rsm='{CII}' xmlns:ram='{RAM}'><rsm:ExchangedDocumentContext><ram:GuidelineSpecifiedDocumentContextParameter>{body}</ram:GuidelineSpecifiedDocumentContextParameter></rsm:ExchangedDocumentContext></rsm:CrossIndustryInvoice>");
        for xml in [
            xml.clone(),
            xml.replace("rsm:", "alias:")
                .replace("xmlns:rsm=", "xmlns:alias="),
        ] {
            let d = parse(&xml);
            let mut current = vec![d.document()];
            for (uri, local) in [
                (CII, "CrossIndustryInvoice"),
                (CII, "ExchangedDocumentContext"),
                (RAM, "GuidelineSpecifiedDocumentContextParameter"),
                (RAM, "ID"),
            ] {
                current = current
                    .into_iter()
                    .flat_map(|n| n.children())
                    .filter(|n| {
                        n.name().is_some_and(|name| {
                            name.namespace_uri() == uri && name.local() == local
                        })
                    })
                    .collect();
            }
            assert_eq!(
                current.into_iter().any(|id| id
                    .children()
                    .any(|n| n.kind() == NodeKind::Text && n.content() == Some("abcde"))),
                expected,
                "{body}"
            );
        }
    }
}
