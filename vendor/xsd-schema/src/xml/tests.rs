//! Unit tests of the well-formedness layer: the reader on its own, below the
//! drivers (whose behaviour `tests/well_formedness.rs` covers end to end).

use quick_xml::events::Event;

use super::reader::{WfError, WfReader};
use super::{ParseLimit, ParseLimits, WellFormednessRule as R};

/// Every event kind of `xml` (as `Debug` names), or the error.
fn read_all(xml: &[u8], limits: &ParseLimits, trim: bool) -> Result<Vec<String>, WfError> {
    let mut r = WfReader::new(xml, limits, trim);
    let mut buf = Vec::new();
    let mut out = Vec::new();
    loop {
        let event = r.read_event_into(&mut buf)?;
        let name = match &event {
            Event::Start(_) => "Start",
            Event::Empty(_) => "Empty",
            Event::End(_) => "End",
            Event::Text(_) => "Text",
            Event::GeneralRef(_) => "Ref",
            Event::CData(_) => "CData",
            Event::Comment(_) => "Comment",
            Event::PI(_) => "PI",
            Event::Decl(_) => "Decl",
            Event::DocType(_) => "DocType",
            Event::Eof => break,
        };
        out.push(name.to_string());
    }
    Ok(out)
}

fn rule_at(xml: &[u8], trim: bool) -> (R, u64) {
    match read_all(xml, &ParseLimits::none(), trim) {
        Err(WfError::NotWellFormed(e)) => (e.rule(), e.position()),
        other => panic!(
            "expected NotWellFormed for {:?}, got {other:?}",
            String::from_utf8_lossy(xml)
        ),
    }
}

#[test]
fn a_well_formed_document_reads_through() {
    let xml = b"<?xml version='1.0'?>\n<!DOCTYPE r>\n<!-- c --><r a='1' xmlns:p='urn:p' p:b='2'>t&amp;<![CDATA[x]]><?pi d?><e/></r>\n";
    let events = read_all(xml, &ParseLimits::none(), false).unwrap();
    assert_eq!(
        events,
        [
            "Decl", "Text", "DocType", "Text", "Comment", "Start", "Text", "Ref", "CData", "PI",
            "Empty", "End", "Text"
        ]
    );
}

#[test]
fn positions_are_input_offsets_with_trimming_too() {
    // With quick-xml skipping white space before markup (the schema reader),
    // a position still points at the offending byte.
    assert_eq!(rule_at(b"<r>\n   ab\x01</r>", true), (R::Char, 9));
    assert_eq!(rule_at(b"<r>\n   ab\x01</r>", false), (R::Char, 9));
    assert_eq!(
        rule_at(b"  <?xml version='1.0'?><r/>", true),
        (R::XmlDeclaration, 2)
    );
    assert_eq!(rule_at(b"<r/>\n  x", true), (R::Document, 7));
    assert_eq!(
        rule_at(b"<r/>  <!DOCTYPE r>", true).0,
        R::DocumentTypeDeclaration
    );
    assert_eq!(rule_at(b"\xEF\xBB\xBF<r>\x01</r>", false), (R::Char, 6));
}

#[test]
fn the_doctype_keyword_is_checked_on_the_next_read() {
    // The event does not show the keyword; the markup still in the buffer
    // does, and the next read looks at it.
    let mut r = WfReader::new(&b"<!doctype r><r/>"[..], &ParseLimits::none(), false);
    let mut buf = Vec::new();
    assert!(matches!(r.read_event_into(&mut buf), Ok(Event::DocType(_))));
    match r.read_event_into(&mut buf) {
        Err(WfError::NotWellFormed(e)) => {
            assert_eq!(e.rule(), R::DocumentTypeDeclaration);
            assert_eq!(e.position(), 0);
        }
        other => panic!("{other:?}"),
    }
    // A caller that clears the buffer in between only loses that check.
    let mut r = WfReader::new(&b"<!doctype r><r/>"[..], &ParseLimits::none(), false);
    let mut buf = Vec::new();
    assert!(r.read_event_into(&mut buf).is_ok());
    buf.clear();
    assert!(matches!(r.read_event_into(&mut buf), Ok(Event::Empty(_))));
}

#[test]
fn truncation_and_a_missing_root_are_reported_at_the_end() {
    match read_all(b"<r><e>", &ParseLimits::none(), false) {
        Err(WfError::NotWellFormed(e)) => {
            assert_eq!(e.rule(), R::Truncated);
            assert_eq!(e.position(), 6);
            assert!(e.message().contains("2 element"), "{e}");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(rule_at(b"<!-- c -->", false), (R::Document, 10));
    assert_eq!(rule_at(b"", false), (R::Document, 0));
}

#[test]
fn quick_xml_refusals_are_not_well_formed_too() {
    assert_eq!(rule_at(b"<r></s>", false), (R::ElementTypeMatch, 3));
    assert_eq!(rule_at(b"<r><!-- x", false).0, R::Truncated);
    assert_eq!(rule_at(b"<r>a & b</r>", false).0, R::Reference);
    assert_eq!(rule_at(b"<r a='1' a='2'/>", false), (R::UniqueAttSpec, 9));
}

#[test]
fn a_limit_is_checked_before_the_tag_is_returned() {
    let limits = ParseLimits::none().with_max_attributes(1);
    let mut r = WfReader::new(&b"<r><e a='1' b='2'/></r>"[..], &limits, false);
    let mut buf = Vec::new();
    assert!(matches!(r.read_event_into(&mut buf), Ok(Event::Start(_))));
    match r.read_event_into(&mut buf) {
        Err(WfError::Limit(e)) => {
            assert_eq!(e.limit(), ParseLimit::Attributes);
            assert_eq!((e.max(), e.found(), e.position()), (1, 2, 12));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn text_runs_are_measured_across_references() {
    let limits = ParseLimits::none().with_max_text_bytes(6);
    assert!(read_all(b"<r>a&amp;</r>", &limits, false).is_ok());
    match read_all(b"<r>a&amp;b</r>", &limits, false) {
        Err(WfError::Limit(e)) => {
            assert_eq!(e.limit(), ParseLimit::TextSize);
            assert_eq!((e.found(), e.position()), (7, 3));
        }
        other => panic!("{other:?}"),
    }
    // A CDATA section, comment or PI is measured on its own.
    assert!(read_all(b"<r>abcdef<![CDATA[abcdef]]>abcdef</r>", &limits, false).is_ok());
}

#[test]
fn namespace_scope_follows_the_elements() {
    // `b:z` and `a:z` collide only while `b` is bound to `urn:u`.
    let ok = b"<r xmlns:a='urn:u'><s xmlns:b='urn:u'/><t xmlns:b='urn:v' a:z='1' b:z='2'/></r>";
    assert!(read_all(ok, &ParseLimits::none(), false).is_ok());
    let bad = b"<r xmlns:a='urn:u' xmlns:b='urn:u'><s><t a:z='1' b:z='2'/></s></r>";
    assert_eq!(rule_at(bad, false).0, R::AttributesUnique);
    // Many prefixed attributes take the hashed path.
    let mut many = String::from("<r xmlns:a='urn:u' xmlns:b='urn:v' xmlns:c='urn:u'");
    for i in 0..20 {
        many.push_str(&format!(" a:x{i}='1' b:x{i}='1'"));
    }
    let ok = format!("{many}/>");
    assert!(read_all(ok.as_bytes(), &ParseLimits::none(), false).is_ok());
    many.push_str(" c:x7='1'/>");
    assert_eq!(rule_at(many.as_bytes(), false).0, R::AttributesUnique);
}

#[test]
fn an_unrelated_buffer_after_a_doctype_is_not_mistaken_for_it() {
    // A caller that passes another buffer after the DOCTYPE read: the
    // keyword check looks only at the declaration's own markup.
    let mut r = WfReader::new(&b"<!DOCTYPE r><r/>"[..], &ParseLimits::none(), false);
    let mut buf = Vec::new();
    assert!(matches!(r.read_event_into(&mut buf), Ok(Event::DocType(_))));
    let mut other = b"<!doctype zz>".to_vec();
    assert!(matches!(r.read_event_into(&mut other), Ok(Event::Empty(_))));
}

#[test]
fn xml_1_1_positions_and_citations() {
    let xml = b"<?xml version='1.1'?><r>ab\xC2\x80</r>";
    match read_all(xml, &ParseLimits::none(), false) {
        Err(WfError::NotWellFormed(e)) => {
            assert_eq!((e.rule(), e.position()), (R::Char, 26));
            assert_eq!(e.xml_version(), super::XmlVersion::V1_1);
            assert!(
                e.to_string()
                    .contains("XML 1.1 §2.2 [2] Char, [2a] RestrictedChar"),
                "{e}"
            );
        }
        other => panic!("{other:?}"),
    }
}
