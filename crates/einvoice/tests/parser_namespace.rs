use einvoice::{Document, ParseError};

#[test]
fn maintained_parser_preserves_legal_xml_prefixed_elements() {
    for source in [
        "<xml:r/>",
        "<xml:r></xml:r>",
        "<r><xml:c xml:lang='de'/></r>",
        "<r xmlns='xml'/>",
        "<r xmlns='xmln&#x73;'><c/></r>",
    ] {
        let document = Document::parse(source.as_bytes()).unwrap();
        assert_eq!(document.original_bytes(), source.as_bytes());
        assert_eq!(document.to_bytes(), source.as_bytes());
    }
}

#[test]
fn maintained_parser_still_rejects_reserved_namespace_misuse() {
    for source in [
        "<xmlns:r/>",
        "<xml:r xmlns:xml='urn:wrong'/>",
        "<xml:r xmlns:xml=''/>",
        "<r xmlns='http://www.w3.org/XML/1998/namespace'/>",
        "<r xmlns='http://www.w3.org/2000/xmlns/'/>",
    ] {
        assert!(matches!(
            Document::parse(source.as_bytes()),
            Err(ParseError::XmlSyntax { .. })
        ));
    }
}
