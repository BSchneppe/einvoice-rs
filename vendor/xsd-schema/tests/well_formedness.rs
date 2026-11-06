//! Well-formedness and parse limits on every reader of XML text (Wave B
//! batch (w): C50, C55, C58, C59).
//!
//! Each case runs through every reader path that applies:
//!
//! * the quick-xml validation driver (`drive_quick_xml`, layer 1, and
//!   `drive_quick_xml_with`, layer 2), with and without PSVI typed values,
//!   against an XSD 1.0 and an XSD 1.1 schema set;
//! * `BufferDocument::from_reader` (the document builder) and the typed build
//!   (`build_typed_document_with`), with the `xsd11` feature;
//! * the schema-document reader, where the case can be written as a schema
//!   document.
//!
//! The rules are XML 1.0 (Fifth Edition) and Namespaces in XML 1.0 (Third
//! Edition); each case names its production or constraint.

use xsd_schema::pipeline::load_and_process_schema;
use xsd_schema::validation::{
    drive_quick_xml, drive_quick_xml_with, CollectingValidationSink, DoctypeView, DriveError,
    DriveWithError, NoopHandler, SchemaValidator, ValidationEventHandler, ValidationFlags,
};
use xsd_schema::xml::{ParseLimit, ParseLimits, WellFormednessRule as R};
use xsd_schema::{NameId, SchemaError, SchemaSet};

// ── Harness ───────────────────────────────────────────────────────────────

const XSD: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="r"/>
</xs:schema>"#;

fn schema_set(v11: bool) -> SchemaSet {
    #[cfg(feature = "xsd11")]
    let mut ss = if v11 {
        SchemaSet::xsd11()
    } else {
        SchemaSet::new()
    };
    #[cfg(not(feature = "xsd11"))]
    let mut ss = {
        let _ = v11;
        SchemaSet::new()
    };
    load_and_process_schema(XSD.as_bytes(), "file:///wf.xsd", &mut ss, None).expect("schema");
    ss
}

/// How a reader ended.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Outcome {
    Ok,
    /// `NotWellFormed` with this rule.
    Wf(R),
    /// `LimitExceeded` with this limit.
    Limit(ParseLimit),
    /// An I/O error of the source.
    Io,
    /// Anything else (the message).
    Other(String),
}

fn of_drive_error(e: DriveError) -> Outcome {
    match e {
        DriveError::NotWellFormed(e) => Outcome::Wf(e.rule()),
        DriveError::LimitExceeded(e) => Outcome::Limit(e.limit()),
        DriveError::Io(_) => Outcome::Io,
        other => Outcome::Other(other.to_string()),
    }
}

fn of_drive_with_error<E: std::fmt::Debug>(e: DriveWithError<E>) -> Outcome {
    match e {
        DriveWithError::NotWellFormed(e) => Outcome::Wf(e.rule()),
        DriveWithError::LimitExceeded(e) => Outcome::Limit(e.limit()),
        DriveWithError::Io(_) => Outcome::Io,
        other => Outcome::Other(format!("{other:?}")),
    }
}

/// Layer 1 (`drive_quick_xml`, which also calls `end_validation`).
fn drive1(ss: &SchemaSet, xml: &[u8], limits: &ParseLimits, psvi: bool) -> Outcome {
    let mut flags = ValidationFlags::default();
    if !psvi {
        flags &= !ValidationFlags::BUILD_PSVI_TYPED_VALUES;
    }
    let validator = SchemaValidator::new(ss, flags);
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    let mut runtime = validator.start_run(CollectingValidationSink {
        errors: &mut errors,
        warnings: &mut warnings,
    });
    runtime.set_parse_limits(limits.clone());
    match drive_quick_xml(xml, &mut runtime, ss) {
        Ok(_) => Outcome::Ok,
        // A validation failure of the whole run is not a reader outcome.
        Err(DriveError::Validation(_)) => Outcome::Ok,
        Err(e) => of_drive_error(e),
    }
}

/// Layer 2 (`drive_quick_xml_with`, a handler).
fn drive2(ss: &SchemaSet, xml: &[u8], limits: &ParseLimits, psvi: bool) -> Outcome {
    let mut flags = ValidationFlags::default();
    if !psvi {
        flags &= !ValidationFlags::BUILD_PSVI_TYPED_VALUES;
    }
    let validator = SchemaValidator::new(ss, flags);
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    let mut runtime = validator.start_run(CollectingValidationSink {
        errors: &mut errors,
        warnings: &mut warnings,
    });
    runtime.set_parse_limits(limits.clone());
    match drive_quick_xml_with(xml, &mut runtime, ss, &mut NoopHandler) {
        Ok(_) => Outcome::Ok,
        Err(e) => of_drive_with_error(e),
    }
}

#[cfg(feature = "xsd11")]
mod doc {
    use super::Outcome;
    use bumpalo::Bump;
    use xsd_schema::document::{
        build_typed_document_with, BufferDocument, BufferDocumentBuilder, BufferDocumentError,
        BufferDocumentOptions, TypedBuildError, TypedBuildOptions,
    };
    use xsd_schema::namespace::NameTable;
    use xsd_schema::validation::{
        CollectingValidationSink, NoopHandler, SchemaValidator, ValidationFlags,
    };
    use xsd_schema::xml::ParseLimits;
    use xsd_schema::SchemaSet;

    pub fn of_document_error(e: BufferDocumentError) -> Outcome {
        match e {
            BufferDocumentError::NotWellFormed(e) => Outcome::Wf(e.rule()),
            BufferDocumentError::LimitExceeded(e) => Outcome::Limit(e.limit()),
            BufferDocumentError::Io(_) => Outcome::Io,
            other => Outcome::Other(other.to_string()),
        }
    }

    /// `BufferDocumentBuilder::build` (what `BufferDocument::from_reader` runs).
    pub fn buffer(xml: &[u8], limits: &ParseLimits) -> Outcome {
        let arena = Bump::new();
        let names = NameTable::new();
        let builder =
            BufferDocumentBuilder::new(&arena, &names, None, BufferDocumentOptions::default())
                .expect("builder")
                .with_parse_limits(limits.clone());
        match builder.build(xml) {
            Ok(_) => Outcome::Ok,
            Err(e) => of_document_error(e),
        }
    }

    /// `BufferDocument::from_reader` with default options.
    pub fn from_reader(xml: &[u8]) -> Outcome {
        let arena = Bump::new();
        let names = NameTable::new();
        match BufferDocument::from_reader_default(xml, &arena, &names) {
            Ok(_) => Outcome::Ok,
            Err(e) => of_document_error(e),
        }
    }

    /// The typed build, limits on the runtime.
    pub fn typed(ss: &SchemaSet, xml: &[u8], limits: &ParseLimits) -> Outcome {
        let arena = Bump::new();
        let validator = SchemaValidator::new(ss, ValidationFlags::default());
        let (mut errors, mut warnings) = (Vec::new(), Vec::new());
        let mut runtime = validator.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        runtime.set_parse_limits(limits.clone());
        match build_typed_document_with(
            xml,
            &arena,
            ss,
            &mut runtime,
            &mut NoopHandler,
            TypedBuildOptions::default(),
        ) {
            Ok(_) => Outcome::Ok,
            Err(TypedBuildError::Document(e)) => of_document_error(e),
            Err(other) => Outcome::Other(format!("{other:?}")),
        }
    }
}

/// Every instance reader path that does not give `expected` for `xml`.
fn reader_mismatches(xml: &[u8], expected: &Outcome, limits: &ParseLimits) -> Vec<String> {
    let shown = String::from_utf8_lossy(xml);
    let shown: String = shown.chars().take(120).collect();
    let mut out = Vec::new();
    let mut expect = |reader: &str, got: Outcome| {
        if &got != expected {
            out.push(format!(
                "{shown:?} [{reader}]: got {got:?}, expected {expected:?}"
            ));
        }
    };
    for v11 in [false, true] {
        let ss = schema_set(v11);
        for psvi in [true, false] {
            expect(
                &format!("drive_quick_xml v11={v11} psvi={psvi}"),
                drive1(&ss, xml, limits, psvi),
            );
            expect(
                &format!("drive_quick_xml_with v11={v11} psvi={psvi}"),
                drive2(&ss, xml, limits, psvi),
            );
        }
        #[cfg(feature = "xsd11")]
        expect(
            &format!("build_typed_document_with v11={v11}"),
            doc::typed(&ss, xml, limits),
        );
    }
    #[cfg(feature = "xsd11")]
    {
        expect("BufferDocumentBuilder::build", doc::buffer(xml, limits));
        if limits.is_unlimited() {
            expect("BufferDocument::from_reader", doc::from_reader(xml));
        }
    }
    out
}

/// Every instance reader path gives `expected` for `xml`.
fn assert_all_readers(xml: &[u8], expected: &Outcome, limits: &ParseLimits) {
    let m = reader_mismatches(xml, expected, limits);
    assert!(m.is_empty(), "{} mismatch(es):\n{}", m.len(), m.join("\n"));
}

fn check(cases: &[(&str, Outcome)]) {
    let mut all = Vec::new();
    for (xml, expected) in cases {
        all.extend(reader_mismatches(
            xml.as_bytes(),
            expected,
            &ParseLimits::none(),
        ));
    }
    assert!(
        all.is_empty(),
        "{} mismatch(es):\n{}",
        all.len(),
        all.join("\n")
    );
}

fn wf(rule: R) -> Outcome {
    Outcome::Wf(rule)
}

// ── Document structure: XML 1.0 §2.1 [1] document, C50 truncation ────────

#[test]
fn content_outside_the_root_element_is_refused() {
    check(&[
        ("x<r/>", wf(R::Document)),
        ("<r/>x", wf(R::Document)),
        ("<r/>\n  y\n", wf(R::Document)),
        ("<![CDATA[x]]><r/>", wf(R::Document)),
        ("<r/><![CDATA[x]]>", wf(R::Document)),
        ("&amp;<r/>", wf(R::Document)),
        ("<r/>&#65;", wf(R::Document)),
        ("<r/>&#32;", wf(R::Document)),
        // White space, comments and PIs are Misc ([27]) and allowed.
        (
            " \t\r\n<!-- c --><?pi data?>\n<r/>\n<!-- e --><?pi?> \n",
            Outcome::Ok,
        ),
    ]);
}

#[test]
fn a_second_root_element_is_refused() {
    check(&[
        ("<r/><r/>", wf(R::Document)),
        ("<r></r><s/>", wf(R::Document)),
        ("<r/> <!-- c --> <r/>", wf(R::Document)),
    ]);
}

#[test]
fn a_document_without_a_root_element_is_refused() {
    check(&[
        ("", wf(R::Document)),
        ("  \n", wf(R::Document)),
        ("<!-- c -->", wf(R::Document)),
        ("<?xml version=\"1.0\"?>", wf(R::Document)),
    ]);
}

#[test]
fn truncated_input_is_refused_by_every_reader() {
    // `BufferDocument::from_reader` accepted these before.
    check(&[
        ("<r>", wf(R::Truncated)),
        ("<r><e>1</e>", wf(R::Truncated)),
        ("<r><e>1", wf(R::Truncated)),
        // Inside markup.
        ("<r><!-- unclosed </r>", wf(R::Truncated)),
        ("<r a=\"unclosed/>", wf(R::Truncated)),
        ("<r><![CDATA[x", wf(R::Truncated)),
        ("<r><?pi", wf(R::Truncated)),
        ("<!DOCTYPE r [", wf(R::Truncated)),
        ("<r", wf(R::Truncated)),
    ]);
}

// ── XML declaration: XML 1.0 §2.8 [23]–[26], [32], [80], [81] ─────────────

#[test]
fn the_xml_declaration_follows_its_grammar_and_comes_first() {
    check(&[
        (" <?xml version=\"1.0\"?><r/>", wf(R::XmlDeclaration)),
        ("\n<?xml version=\"1.0\"?><r/>", wf(R::XmlDeclaration)),
        (
            "<!-- c --><?xml version=\"1.0\"?><r/>",
            wf(R::XmlDeclaration),
        ),
        ("<r><?xml version=\"1.0\"?></r>", wf(R::XmlDeclaration)),
        ("<r/><?xml version=\"1.0\"?>", wf(R::XmlDeclaration)),
        (
            "<?xml version=\"1.0\"?><?xml version=\"1.0\"?><r/>",
            wf(R::XmlDeclaration),
        ),
        ("<?xml?><r/>", wf(R::XmlDeclaration)),
        ("<?xml encoding=\"UTF-8\"?><r/>", wf(R::XmlDeclaration)),
        (
            "<?xml encoding=\"UTF-8\" version=\"1.0\"?><r/>",
            wf(R::XmlDeclaration),
        ),
        (
            "<?xml version=\"1.0\" standalone=\"yes\" encoding=\"UTF-8\"?><r/>",
            wf(R::XmlDeclaration),
        ),
        (
            "<?xml version=\"1.0\" foo=\"x\"?><r/>",
            wf(R::XmlDeclaration),
        ),
        (
            "<?xml version=\"1.0\" standalone=\"maybe\"?><r/>",
            wf(R::XmlDeclaration),
        ),
        ("<?xml version=\"2.0\"?><r/>", wf(R::XmlDeclaration)),
        ("<?xml version=\"1.\"?><r/>", wf(R::XmlDeclaration)),
        ("<?xml version=1.0?><r/>", wf(R::XmlDeclaration)),
        ("<?xml version=\"1.0'?><r/>", wf(R::XmlDeclaration)),
        (
            "<?xml version=\"1.0\"encoding=\"UTF-8\"?><r/>",
            wf(R::XmlDeclaration),
        ),
        (
            "<?xml version=\"1.0\" encoding=\"9x\"?><r/>",
            wf(R::XmlDeclaration),
        ),
        (
            "<?xml version=\"1.0\" encoding=\"\"?><r/>",
            wf(R::XmlDeclaration),
        ),
        (
            "<?xml version=\"1.0\" version=\"1.0\"?><r/>",
            wf(R::XmlDeclaration),
        ),
        // Well-formed declarations.
        ("<?xml version=\"1.0\"?><r/>", Outcome::Ok),
        (
            "<?xml version='1.0' encoding='utf-8' standalone='no' ?>\n<r/>",
            Outcome::Ok,
        ),
        (
            "<?xml version = \"1.0\"\tencoding = 'UTF-8'?><r/>",
            Outcome::Ok,
        ),
        (
            "<?xml version=\"1.0\" standalone=\"yes\"?><r/>",
            Outcome::Ok,
        ),
        ("\u{FEFF}<?xml version=\"1.0\"?><r/>", Outcome::Ok),
        // [26] VersionNum ::= '1.' [0-9]+ — an XML 1.0 processor reads a 1.x
        // document as 1.0 (XML 1.0 §2.8).
        ("<?xml version=\"1.1\"?><r/>", Outcome::Ok),
        // A PI whose target only starts with `xml` is a PI.
        ("<?xml-model href=\"a\"?><r/>", Outcome::Ok),
    ]);
}

// ── Document type declaration: XML 1.0 §2.8 [22], [28]; C55 ──────────────

#[test]
fn a_document_type_declaration_outside_the_prolog_is_refused() {
    check(&[
        ("<r><!DOCTYPE r></r>", wf(R::DocumentTypeDeclaration)),
        ("<r/><!DOCTYPE r>", wf(R::DocumentTypeDeclaration)),
        (
            "<!DOCTYPE r><!DOCTYPE r><r/>",
            wf(R::DocumentTypeDeclaration),
        ),
        ("<!DOCTYPEr><r/>", wf(R::DocumentTypeDeclaration)),
        ("<!doctype r><r/>", wf(R::DocumentTypeDeclaration)),
        ("<!DOCTYPE 1r><r/>", wf(R::Name)),
        // In the prolog it is accepted (and reported to the handler).
        ("<!DOCTYPE r><r/>", Outcome::Ok),
        (
            "<?xml version=\"1.0\"?>\n<!-- c -->\n<!DOCTYPE r SYSTEM \"r.dtd\">\n<?pi?><r/>",
            Outcome::Ok,
        ),
        ("<!DOCTYPE r [<!ELEMENT r ANY>]>\n<r/>", Outcome::Ok),
    ]);
}

#[derive(Default)]
struct Doctypes {
    seen: Vec<(String, String, usize)>,
    refuse: bool,
}

impl ValidationEventHandler for Doctypes {
    type Error = String;
    fn on_doctype(&mut self, d: DoctypeView<'_>) -> Result<(), String> {
        self.seen
            .push((d.name.to_string(), d.text.to_string(), d.byte_pos));
        if self.refuse {
            Err(format!("DOCTYPE {} refused", d.name))
        } else {
            Ok(())
        }
    }
}

fn drive_doctypes(xml: &str, refuse: bool) -> (Doctypes, Result<(), String>) {
    let ss = schema_set(false);
    let validator = SchemaValidator::new(&ss, ValidationFlags::default());
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    let mut runtime = validator.start_run(CollectingValidationSink {
        errors: &mut errors,
        warnings: &mut warnings,
    });
    let mut h = Doctypes {
        refuse,
        ..Default::default()
    };
    let r = drive_quick_xml_with(xml.as_bytes(), &mut runtime, &ss, &mut h)
        .map(|_| ())
        .map_err(|e| match e {
            DriveWithError::Hook(m) => m,
            other => format!("{other}"),
        });
    (h, r)
}

#[test]
fn a_document_type_declaration_in_the_prolog_is_reported_to_the_handler() {
    let xml = "<?xml version=\"1.0\"?>\n<!DOCTYPE r SYSTEM \"r.dtd\" [<!ENTITY e \"x\">]>\n<r/>";
    let (h, r) = drive_doctypes(xml, false);
    assert_eq!(r, Ok(()));
    assert_eq!(
        h.seen,
        vec![(
            "r".to_string(),
            "r SYSTEM \"r.dtd\" [<!ENTITY e \"x\">]".to_string(),
            22
        )]
    );

    // A handler refuses it by returning an error.
    let (h, r) = drive_doctypes(xml, true);
    assert_eq!(h.seen.len(), 1);
    assert_eq!(r, Err("DOCTYPE r refused".to_string()));

    // No declaration, no call.
    let (h, r) = drive_doctypes("<r/>", true);
    assert_eq!(r, Ok(()));
    assert!(h.seen.is_empty());

    // Inside the content it is not well-formed, and the hook is not called.
    let (h, r) = drive_doctypes("<r><!DOCTYPE r></r>", true);
    assert!(h.seen.is_empty());
    assert!(r.unwrap_err().contains("[28] doctypedecl"));
}

#[cfg(feature = "xsd11")]
#[test]
fn the_typed_build_passes_the_document_type_declaration_to_the_inner_handler() {
    use bumpalo::Bump;
    use xsd_schema::document::{build_typed_document_with, TypedBuildError, TypedBuildOptions};

    let ss = schema_set(false);
    let validator = SchemaValidator::new(&ss, ValidationFlags::default());
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    let mut runtime = validator.start_run(CollectingValidationSink {
        errors: &mut errors,
        warnings: &mut warnings,
    });
    let arena = Bump::new();
    let mut h = Doctypes {
        refuse: true,
        ..Default::default()
    };
    let r = build_typed_document_with(
        "<!DOCTYPE r><r/>".as_bytes(),
        &arena,
        &ss,
        &mut runtime,
        &mut h,
        TypedBuildOptions::default(),
    );
    assert!(matches!(r, Err(TypedBuildError::Handler(ref m)) if m == "DOCTYPE r refused"));
    assert_eq!(h.seen.len(), 1);
}

// ── Comments, PIs, CDATA: XML 1.0 §2.5–§2.7 ──────────────────────────────

#[test]
fn comments_contain_no_double_hyphen() {
    check(&[
        ("<r><!-- a -- b --></r>", wf(R::Comment)),
        ("<r><!-- a ---></r>", wf(R::Comment)),
        ("<!-- a -- b --><r/>", wf(R::Comment)),
        ("<r/><!-- a -- b -->", wf(R::Comment)),
        ("<r><!-- a - b --></r>", Outcome::Ok),
        ("<r><!----></r>", Outcome::Ok),
        ("<r><!---a--></r>", Outcome::Ok),
    ]);
}

#[test]
fn processing_instruction_targets_are_names_other_than_xml() {
    check(&[
        ("<r><?XML x?></r>", wf(R::ProcessingInstruction)),
        ("<r><?xMl?></r>", wf(R::ProcessingInstruction)),
        ("<?Xml x?><r/>", wf(R::ProcessingInstruction)),
        ("<r><?1a x?></r>", wf(R::Name)),
        ("<r><?a?b?></r>", wf(R::ProcessingInstruction)),
        // Namespaces in XML §7: a PI target is an NCName.
        ("<r><?a:b x?></r>", wf(R::QName)),
        ("<r><?pi?><?pi data ?><?xml-model a?></r>", Outcome::Ok),
    ]);
}

// ── Characters: XML 1.0 §2.2 [2] Char, §4.1 WFC: Legal Character ─────────

#[test]
fn raw_characters_are_xml_characters() {
    check(&[
        ("<r>\u{1}</r>", wf(R::Char)),
        ("<r>a\u{8}b</r>", wf(R::Char)),
        ("<r>\u{B}</r>", wf(R::Char)),
        ("<r>\u{1F}</r>", wf(R::Char)),
        ("<r>\u{FFFE}</r>", wf(R::Char)),
        ("<r>\u{FFFF}</r>", wf(R::Char)),
        ("<r a=\"\u{1}\"/>", wf(R::Char)),
        ("<r a='x\u{FFFF}'/>", wf(R::Char)),
        ("<r><!--\u{1}--></r>", wf(R::Char)),
        ("<!--\u{2}--><r/>", wf(R::Char)),
        ("<r><?pi \u{1}?></r>", wf(R::Char)),
        ("<r><![CDATA[\u{B}]]></r>", wf(R::Char)),
        ("<!DOCTYPE r [<!-- \u{1} -->]><r/>", wf(R::Char)),
        ("<r>\u{0}</r>", wf(R::Char)),
        // Every other character is a Char, raw.
        (
            "<r a=\"\t\n\r\u{20}\u{D7FF}\u{E000}\u{FFFD}\u{10000}\u{10FFFF}\">\t\n\r\u{20}\u{7F}\u{85}\u{D7FF}\u{E000}\u{FEFF}\u{FFFD}\u{10000}\u{10FFFF}</r>",
            Outcome::Ok,
        ),
    ]);
}

#[test]
fn character_references_name_xml_characters_whatever_their_length() {
    check(&[
        ("<r>&#1;</r>", wf(R::LegalCharacter)),
        ("<r>&#x1;</r>", wf(R::LegalCharacter)),
        ("<r>&#0;</r>", wf(R::LegalCharacter)),
        ("<r>&#xB;</r>", wf(R::LegalCharacter)),
        ("<r>&#xFFFE;</r>", wf(R::LegalCharacter)),
        ("<r>&#xFFFF;</r>", wf(R::LegalCharacter)),
        ("<r>&#xD800;</r>", wf(R::LegalCharacter)),
        ("<r>&#xDFFF;</r>", wf(R::LegalCharacter)),
        ("<r>&#x110000;</r>", wf(R::LegalCharacter)),
        // RR2: twelve digits and more.
        ("<r>&#000000000001;</r>", wf(R::LegalCharacter)),
        ("<r>&#x0000000000001;</r>", wf(R::LegalCharacter)),
        ("<r>&#99999999999999999999;</r>", wf(R::LegalCharacter)),
        ("<r>&#;</r>", wf(R::LegalCharacter)),
        ("<r>&#x;</r>", wf(R::LegalCharacter)),
        ("<r>&#12a;</r>", wf(R::LegalCharacter)),
        ("<r>&#X41;</r>", wf(R::LegalCharacter)),
        // In attribute values too.
        ("<r a=\"&#1;\"/>", wf(R::LegalCharacter)),
        ("<r a=\"&#000000000001;\"/>", wf(R::LegalCharacter)),
        ("<r a=\"&#xFFFF;\"/>", wf(R::LegalCharacter)),
        (
            "<r xmlns:p=\"urn:&#x1;\"/>",
            wf(R::LegalCharacter),
        ),
        // Legal references, of any length.
        (
            "<r a=\"&#9;&#xA;&#13;\">&#9;&#xA;&#13;&#65;&#x42;&#x1D11E;&#000000000065;&#x10FFFF;</r>",
            Outcome::Ok,
        ),
    ]);
}

#[test]
fn every_ampersand_starts_a_reference() {
    check(&[
        ("<r a=\"&\"/>", wf(R::Reference)),
        ("<r a=\"a & b\"/>", wf(R::Reference)),
        ("<r a=\"&amp\"/>", wf(R::Reference)),
        ("<r a=\"&1;\"/>", wf(R::Reference)),
        ("<r>&1;</r>", wf(R::Reference)),
        ("<r>&a b;</r>", wf(R::Reference)),
        (
            "<r a=\"&amp;&lt;&gt;&apos;&quot;\">&amp;&lt;&gt;&apos;&quot;</r>",
            Outcome::Ok,
        ),
        // WFC: Entity Declared — no reader processes a DTD.
        ("<r>&nope;</r>", wf(R::EntityDeclared)),
        ("<r a=\"&nope;\"/>", wf(R::EntityDeclared)),
        (
            "<!DOCTYPE r [<!ENTITY e \"x\">]><r>&e;</r>",
            wf(R::EntityDeclared),
        ),
        // A lone `&` in character data.
        ("<r>a & b</r>", wf(R::Reference)),
    ]);
}

#[test]
fn character_data_contains_no_cdata_end() {
    check(&[
        ("<r>]]></r>", wf(R::CharData)),
        ("<r>a]]>b</r>", wf(R::CharData)),
        ("<r>]]&gt; ]] > ]> ]]</r>", Outcome::Ok),
        ("<r><![CDATA[]]]]><![CDATA[>]]></r>", Outcome::Ok),
    ]);
}

#[test]
fn attribute_values_contain_no_less_than_sign() {
    check(&[
        ("<r a=\"<\"/>", wf(R::AttributeValue)),
        ("<r a='x<y'/>", wf(R::AttributeValue)),
        ("<r a=\"&lt;\" b='>'/>", Outcome::Ok),
    ]);
}

// ── Names and tags: XML 1.0 §2.3 [4]–[5], §3.1 [40]–[44]; RR1, RR7 ───────

#[test]
fn element_and_attribute_names_are_xml_names() {
    check(&[
        ("<></>", wf(R::Name)),
        ("<1r/>", wf(R::Name)),
        ("<-r/>", wf(R::Name)),
        ("<\u{B7}r/>", wf(R::Name)),
        ("<r =a=\"1\"/>", wf(R::StartTag)),
        ("<r \u{B7}a=\"1\"/>", wf(R::Name)),
        ("<r 1a=\"1\"/>", wf(R::Name)),
        // Non-ASCII names, and names with `.`, `-`, `_`, digits, `·`.
        (
            "<é·ü.r-1_x a\u{300}=\"1\" \u{10000}b=\"2\"><\u{4E2D}\u{6587}/></é·ü.r-1_x>",
            Outcome::Ok,
        ),
    ]);
}

#[test]
fn tags_separate_their_attributes_with_white_space() {
    check(&[
        // RR1: `/` or `"` in a name ends it in XML, not in quick-xml.
        ("<r/ x>", wf(R::StartTag)),
        ("<x:h/ xmlns:x=\"urn:x\"></x:h/>", wf(R::StartTag)),
        ("<x:h xmlns:x=\"urn:x\" a/b=\"1\"/>", wf(R::StartTag)),
        ("<x:h\">\" xmlns:x=\"urn:x\"/>", wf(R::StartTag)),
        // RR7: [40] (S Attribute)*.
        ("<r a=\"1\"b=\"2\"/>", wf(R::StartTag)),
        ("<r xmlns:s=\"urn:s\"xmlns:o=\"urn:o\"/>", wf(R::StartTag)),
        ("<r a=\"1\" b/>", wf(R::StartTag)),
        ("<r a b=\"1\"/>", wf(R::StartTag)),
        ("<r a=1/>", wf(R::StartTag)),
        ("<r a=\"1\" / >", wf(R::StartTag)),
        // Well-formed spacing.
        ("<r a = \"1\"\tb='2'\n\rc=\"3\" ></r >", Outcome::Ok),
        ("<r a=\"1\" />", Outcome::Ok),
        ("<r\n/>", Outcome::Ok),
    ]);
}

// ── Namespaces in XML 1.0 (Third Edition) ────────────────────────────────

#[test]
fn names_are_qualified_names() {
    check(&[
        ("<t:x:y xmlns:t=\"urn:t\"/>", wf(R::QName)),
        (
            "<t:x xmlns:t=\"urn:t\" xmlns:a=\"urn:a\" a:b:c=\"1\"/>",
            wf(R::QName),
        ),
        ("<:r/>", wf(R::QName)),
        ("<r:/>", wf(R::QName)),
        ("<r :a=\"1\"/>", wf(R::QName)),
        ("<r xmlns:=\"urn:x\"/>", wf(R::QName)),
        ("<r xmlns:a:b=\"urn:x\"/>", wf(R::QName)),
        // Each part is an NCName: the local part starts with a NameStartChar.
        ("<a:1b xmlns:a=\"urn:a\"/>", wf(R::QName)),
        ("<r xmlns:--=\"urn:x\"/>", wf(R::QName)),
        ("<r xmlns:a=\"urn:a\" a:-b=\"1\"/>", wf(R::QName)),
    ]);
}

#[test]
fn reserved_prefixes_and_namespace_names_are_respected() {
    const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";
    const XMLNS_NS: &str = "http://www.w3.org/2000/xmlns/";
    let cases: Vec<(String, Outcome)> = vec![
        ("<r xmlns:xml=\"urn:x\"/>".into(), wf(R::ReservedPrefix)),
        ("<r xmlns:xmlns=\"urn:x\"/>".into(), wf(R::ReservedPrefix)),
        (
            format!("<r xmlns:xmlns=\"{XMLNS_NS}\"/>"),
            wf(R::ReservedPrefix),
        ),
        (format!("<r xmlns:p=\"{XML_NS}\"/>"), wf(R::ReservedPrefix)),
        (format!("<r xmlns=\"{XML_NS}\"/>"), wf(R::ReservedPrefix)),
        (
            format!("<r xmlns:p=\"{XMLNS_NS}\"/>"),
            wf(R::ReservedPrefix),
        ),
        (format!("<r xmlns=\"{XMLNS_NS}\"/>"), wf(R::ReservedPrefix)),
        // The value is compared after references are resolved.
        (
            "<r xmlns:p=\"&#104;ttp://www.w3.org/XML/1998/namespace\"/>".into(),
            wf(R::ReservedPrefix),
        ),
        ("<xmlns:x/>".into(), wf(R::ReservedPrefix)),
        // `xml` may be declared, to its own namespace.
        (
            format!("<r xmlns:xml=\"{XML_NS}\" xml:lang=\"en\"/>"),
            Outcome::Ok,
        ),
        // Other prefixes starting with `xml` are reserved for later
        // specifications, not errors.
        (
            "<r xmlns:xml2=\"urn:x\" xmlns:xmlfoo=\"urn:y\"/>".into(),
            Outcome::Ok,
        ),
    ];
    for (xml, expected) in &cases {
        assert_all_readers(xml.as_bytes(), expected, &ParseLimits::none());
    }
}

#[test]
fn a_prefix_is_not_undeclared() {
    check(&[
        ("<r xmlns:p=\"\"/>", wf(R::PrefixUndeclaring)),
        (
            "<r xmlns:p=\"urn:p\"><s xmlns:p=\"\"/></r>",
            wf(R::PrefixUndeclaring),
        ),
        // Undeclaring the default namespace is allowed.
        ("<r xmlns=\"urn:d\"><s xmlns=\"\"/></r>", Outcome::Ok),
    ]);
}

#[test]
fn attributes_are_unique_by_expanded_name() {
    check(&[
        (
            "<t:x xmlns:t=\"urn:t\" xmlns:a=\"urn:u\" xmlns:b=\"urn:u\" a:z=\"1\" b:z=\"2\"/>",
            wf(R::AttributesUnique),
        ),
        (
            "<r xmlns:a=\"urn:u\"><s xmlns:b=\"urn:u\" a:z=\"1\" b:z=\"2\"/></r>",
            wf(R::AttributesUnique),
        ),
        (
            "<r xmlns:a=\"urn:u\" xmlns:b=\"urn:v\"><s xmlns:b=\"urn:u\" a:z=\"1\" b:z=\"2\"/></r>",
            wf(R::AttributesUnique),
        ),
        // The default namespace does not apply to attribute names.
        (
            "<r xmlns:a=\"urn:u\" xmlns=\"urn:u\" a:z=\"1\" z=\"2\"/>",
            Outcome::Ok,
        ),
        (
            "<r xmlns:a=\"urn:u\" xmlns:b=\"urn:v\" a:z=\"1\" b:z=\"2\"/>",
            Outcome::Ok,
        ),
        // A redeclared prefix in a child scope.
        (
            "<r xmlns:a=\"urn:u\" xmlns:b=\"urn:u\"><s xmlns:b=\"urn:v\" a:z=\"1\" b:z=\"2\"/></r>",
            Outcome::Ok,
        ),
    ]);
    // Many attributes: the check stays linear (quick-xml's own duplicate
    // check is, too).
    let mut big = String::from("<r xmlns:a=\"urn:u\" xmlns:b=\"urn:u\"");
    for i in 0..5_000 {
        big.push_str(&format!(" a:n{i}=\"1\""));
    }
    big.push_str(" b:n4999=\"2\"/>");
    assert_all_readers(
        big.as_bytes(),
        &wf(R::AttributesUnique),
        &ParseLimits::none(),
    );
}

/// Every well-formedness refusal is `NotWellFormed`, also for what quick-xml
/// itself detects (it used to keep quick-xml's error, or an unbound-prefix
/// error).
#[test]
fn every_refusal_is_not_well_formed() {
    check(&[
        ("<r z=\"1\" z=\"2\"/>", wf(R::UniqueAttSpec)),
        (
            "<r xmlns:a=\"urn:u\" xmlns:a=\"urn:v\"/>",
            wf(R::UniqueAttSpec),
        ),
        ("<r xmlns=\"urn:u\" xmlns=\"urn:v\"/>", wf(R::UniqueAttSpec)),
        ("<r></s>", wf(R::ElementTypeMatch)),
        ("</r>", wf(R::ElementTypeMatch)),
        ("<r/></r>", wf(R::ElementTypeMatch)),
        ("<q:x/>", wf(R::PrefixDeclared)),
        ("<r q:z=\"1\"/>", wf(R::PrefixDeclared)),
        ("<r><q:x xmlns:p=\"urn:p\"/></r>", wf(R::PrefixDeclared)),
        ("<r><!foo></r>", wf(R::Markup)),
        ("<!DOCTYPE>", wf(R::DocumentTypeDeclaration)),
    ]);
}

// ── Positions ────────────────────────────────────────────────────────────

fn wf_error(xml: &str) -> xsd_schema::xml::NotWellFormed {
    let ss = schema_set(false);
    let validator = SchemaValidator::new(&ss, ValidationFlags::default());
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    let mut runtime = validator.start_run(CollectingValidationSink {
        errors: &mut errors,
        warnings: &mut warnings,
    });
    match drive_quick_xml(xml.as_bytes(), &mut runtime, &ss) {
        Err(DriveError::NotWellFormed(e)) => e,
        other => panic!("expected NotWellFormed for {xml:?}, got {other:?}"),
    }
}

#[test]
fn errors_name_the_rule_and_the_byte_position() {
    // (input, rule, byte offset of the offending character, name or markup)
    let cases: &[(&str, R, u64)] = &[
        ("<r>ab\u{1}</r>", R::Char, 5),
        ("<r a='xy\u{1}'/>", R::Char, 8),
        ("<r><!--ab\u{1}--></r>", R::Char, 9),
        ("<r>ab&#1;</r>", R::LegalCharacter, 5),
        ("<r a='x&#1;'/>", R::LegalCharacter, 7),
        ("<r><!-- a -- b --></r>", R::Comment, 10),
        ("<r>a]]>b</r>", R::CharData, 4),
        ("<r a='1'b='2'/>", R::StartTag, 8),
        ("<r a='<'/>", R::AttributeValue, 6),
        ("<r/>x", R::Document, 4),
        ("<r/><s/>", R::Document, 4),
        (" <?xml version='1.0'?><r/>", R::XmlDeclaration, 1),
        ("<r><!DOCTYPE r></r>", R::DocumentTypeDeclaration, 3),
        ("<a:b:c xmlns:a='u'/>", R::QName, 1),
        ("<r xmlns:p=''/>", R::PrefixUndeclaring, 3),
        // A byte-order mark counts: positions are offsets into the input.
        ("\u{FEFF}<r>\u{1}</r>", R::Char, 6),
    ];
    for (xml, rule, pos) in cases {
        let e = wf_error(xml);
        assert_eq!(e.rule(), *rule, "{xml:?}: {e}");
        assert_eq!(e.position(), *pos, "{xml:?}: {e}");
        let shown = e.to_string();
        assert!(shown.contains(rule.citation()), "{shown}");
        assert!(shown.contains(&format!("at byte {pos}")), "{shown}");
    }
}

// ── The schema-document reader ───────────────────────────────────────────

fn load_schema_text(xsd: &str) -> Result<(), SchemaError> {
    let mut ss = SchemaSet::new();
    load_and_process_schema(xsd.as_bytes(), "file:///wf-schema.xsd", &mut ss, None).map(|_| ())
}

const XS: &str = "xmlns:xs=\"http://www.w3.org/2001/XMLSchema\"";

#[test]
fn the_schema_reader_refuses_documents_that_are_not_well_formed() {
    let cases: Vec<(String, R)> = vec![
        (format!("<xs:schema {XS}/>x"), R::Document),
        (format!("<xs:schema {XS}/><xs:schema {XS}/>"), R::Document),
        (format!(" <?xml version=\"1.0\"?><xs:schema {XS}/>"), R::XmlDeclaration),
        (format!("<?xml encoding=\"UTF-8\"?><xs:schema {XS}/>"), R::XmlDeclaration),
        (format!("<xs:schema {XS}><!-- a -- b --></xs:schema>"), R::Comment),
        (format!("<xs:schema {XS}><?XML x?></xs:schema>"), R::ProcessingInstruction),
        (
            format!("<xs:schema {XS}><xs:element name=\"a\" id=\"b\"type=\"xs:string\"/></xs:schema>"),
            R::StartTag,
        ),
        (
            format!("<xs:schema {XS}><xs:element name=\"a\u{1}\"/></xs:schema>"),
            R::Char,
        ),
        (
            format!(
                "<xs:schema {XS}><xs:annotation><xs:documentation>\u{1}</xs:documentation></xs:annotation></xs:schema>"
            ),
            R::Char,
        ),
        (
            format!(
                "<xs:schema {XS}><xs:annotation><xs:documentation>&#1;</xs:documentation></xs:annotation></xs:schema>"
            ),
            R::LegalCharacter,
        ),
        (
            format!(
                "<xs:schema {XS}><xs:annotation><xs:documentation>]]></xs:documentation></xs:annotation></xs:schema>"
            ),
            R::CharData,
        ),
        (format!("<xs:schema {XS} xmlns:p=\"\"/>"), R::PrefixUndeclaring),
        (format!("<xs:schema {XS} xmlns:xml=\"urn:x\"/>"), R::ReservedPrefix),
        (format!("<xs:schema:x {XS}/>"), R::QName),
        (
            format!(
                "<xs:schema {XS} xmlns:a=\"urn:u\" xmlns:b=\"urn:u\" a:z=\"1\" b:z=\"2\"/>"
            ),
            R::AttributesUnique,
        ),
        (format!("<xs:schema {XS}/><!DOCTYPE x>"), R::DocumentTypeDeclaration),
    ];
    for (xsd, rule) in &cases {
        match load_schema_text(xsd) {
            Err(SchemaError::XmlError { message, location }) => {
                assert!(
                    message.contains(rule.citation()),
                    "{xsd:?}: expected {rule:?}, got {message:?}"
                );
                assert!(location.is_some(), "{xsd:?}: no location in {message:?}");
            }
            other => panic!("{xsd:?}: expected an XML error for {rule:?}, got {other:?}"),
        }
    }
}

#[test]
fn the_schema_reader_accepts_well_formed_documents() {
    for xsd in [
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- c -->\n<xs:schema {XS}>\n  <xs:element name=\"a\"/>\n</xs:schema>\n<!-- e -->\n"),
        format!("<!DOCTYPE xs:schema PUBLIC \"-//W3C//DTD XMLSCHEMA 200102//EN\" \"XMLSchema.dtd\" [<!ATTLIST xs:schema id ID #IMPLIED>]>\n<xs:schema {XS}/>"),
        format!("\u{FEFF}<?xml version=\"1.0\"?><xs:schema {XS}><xs:annotation><xs:documentation>a &amp; b &#x1D11E; <![CDATA[]]]]></xs:documentation></xs:annotation></xs:schema>"),
    ] {
        load_schema_text(&xsd).unwrap_or_else(|e| panic!("{xsd:?}: {e}"));
    }
}

// ── C59: parse limits ────────────────────────────────────────────────────

/// `xml` is accepted under `ok` and refused with `limit` under `refused`.
fn limit_case(xml: &str, ok: ParseLimits, refused: ParseLimits, limit: ParseLimit) {
    assert_all_readers(xml.as_bytes(), &Outcome::Ok, &ok);
    assert_all_readers(xml.as_bytes(), &Outcome::Limit(limit), &refused);
}

fn l() -> ParseLimits {
    ParseLimits::none()
}

#[test]
fn depth_is_limited() {
    let xml = "<r><a><b/></a></r>";
    limit_case(
        xml,
        l().with_max_depth(3),
        l().with_max_depth(2),
        ParseLimit::Depth,
    );
}

#[test]
fn name_length_is_limited() {
    for xml in [
        "<abcde/>",
        "<r abcde=\"1\"/>",
        "<p:abc xmlns:p=\"urn:p\"/>",
        "<r xmlns:abcde=\"urn:p\"/>",
        "<r><?abcde?></r>",
    ] {
        limit_case(
            xml,
            l().with_max_name_bytes(5),
            l().with_max_name_bytes(4),
            ParseLimit::NameLength,
        );
    }
}

#[test]
fn namespace_uri_length_is_limited() {
    for xml in ["<r xmlns:p=\"urn:abcde\"/>", "<r xmlns=\"urn:abcde\"/>"] {
        limit_case(
            xml,
            l().with_max_namespace_uri_bytes(9),
            l().with_max_namespace_uri_bytes(8),
            ParseLimit::NamespaceUriLength,
        );
    }
}

#[test]
fn attributes_per_element_are_limited() {
    // Namespace declarations do not count as attributes.
    let xml = "<r xmlns:p=\"urn:p\" a=\"1\" p:b=\"2\" c=\"3\"/>";
    limit_case(
        xml,
        l().with_max_attributes(3),
        l().with_max_attributes(2),
        ParseLimit::Attributes,
    );
}

#[test]
fn namespace_declarations_per_element_are_limited() {
    let xml = "<r xmlns=\"urn:d\" xmlns:p=\"urn:p\" a=\"1\" b=\"2\"><s xmlns:q=\"urn:q\"/></r>";
    limit_case(
        xml,
        l().with_max_namespace_declarations(2),
        l().with_max_namespace_declarations(1),
        ParseLimit::NamespaceDeclarations,
    );
}

#[test]
fn namespaces_in_scope_are_limited() {
    // `a`, the default namespace, `b`; redeclaring `a` adds none.
    let xml =
        "<r xmlns:a=\"urn:a\"><s xmlns=\"urn:d\" xmlns:a=\"urn:a2\"><t xmlns:b=\"urn:b\"/></s></r>";
    limit_case(
        xml,
        l().with_max_namespaces_in_scope(3),
        l().with_max_namespaces_in_scope(2),
        ParseLimit::NamespacesInScope,
    );
    // Out of scope again after the element closes.
    let xml =
        "<r><s xmlns:a=\"urn:a\" xmlns:b=\"urn:b\"/><s xmlns:c=\"urn:c\" xmlns:d=\"urn:d\"/></r>";
    assert_all_readers(
        xml.as_bytes(),
        &Outcome::Ok,
        &l().with_max_namespaces_in_scope(2),
    );
}

#[test]
fn attribute_value_size_is_limited() {
    for xml in [
        "<r a=\"abcd\"/>",
        "<r a=\"a&amp;\"/>",
        "<r xmlns:p=\"abcd\"/>",
    ] {
        limit_case(
            xml,
            l().with_max_attribute_value_bytes(xml.len()),
            l().with_max_attribute_value_bytes(3),
            ParseLimit::AttributeValueSize,
        );
    }
}

#[test]
fn text_size_is_limited() {
    // A run of character data counts as written: `a&amp;b` is 7 bytes.
    for (xml, max) in [
        ("<r>abcd</r>", 4),
        ("<r>a&amp;b</r>", 7),
        ("<r>a&#65;&#66;</r>", 11),
        ("<r><![CDATA[abcd]]></r>", 4),
        ("<r><!--abcd--></r>", 4),
        ("<r><?pi abcd?></r>", 7),
    ] {
        limit_case(
            xml,
            l().with_max_text_bytes(max),
            l().with_max_text_bytes(max - 1),
            ParseLimit::TextSize,
        );
    }
}

#[test]
fn nodes_are_counted_attributes_and_namespace_declarations_included() {
    // r, xmlns:p, a, p:b, comment, PI, text run, CDATA, s = 9 nodes (RR3).
    let xml = "<r xmlns:p=\"urn:p\" a=\"1\" p:b=\"2\"><!--c--><?pi?>t&amp;u<![CDATA[x]]><s/></r>";
    limit_case(
        xml,
        l().with_max_nodes(9),
        l().with_max_nodes(8),
        ParseLimit::Nodes,
    );
    // RR3: an attribute-heavy element.
    let xml = "<r a=\"1\" b=\"2\" c=\"3\"/>";
    limit_case(
        xml,
        l().with_max_nodes(4),
        l().with_max_nodes(3),
        ParseLimit::Nodes,
    );
    // White space outside the root element is not a node.
    let xml = "\n<r/>\n";
    limit_case(
        xml,
        l().with_max_nodes(1),
        l().with_max_nodes(0),
        ParseLimit::Nodes,
    );
}

#[test]
fn a_limit_error_names_the_limit_and_the_position() {
    let ss = schema_set(false);
    let validator = SchemaValidator::new(&ss, ValidationFlags::default());
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    let mut runtime = validator.start_run(CollectingValidationSink {
        errors: &mut errors,
        warnings: &mut warnings,
    });
    runtime.set_parse_limits(l().with_max_depth(2));
    match drive_quick_xml("<r><a><b/></a></r>".as_bytes(), &mut runtime, &ss) {
        Err(DriveError::LimitExceeded(e)) => {
            assert_eq!(e.limit(), ParseLimit::Depth);
            assert_eq!(e.max(), 2);
            assert_eq!(e.found(), 3);
            assert_eq!(e.position(), 6);
            assert_eq!(
                e.to_string(),
                "parse limit max_depth exceeded at byte 6: element depth 3, the limit is 2"
            );
        }
        other => panic!("expected LimitExceeded, got {other:?}"),
    }
}

/// Entries and text bytes of a name table.
fn table_usage(names: &xsd_schema::namespace::NameTable) -> (usize, usize) {
    let bytes = (0..names.len() as u32)
        .filter_map(|i| names.try_resolve_ref(NameId(i)))
        .map(str::len)
        .sum();
    (names.len(), bytes)
}

const WARM_UP: &[u8] = b"<r><r><r/></r></r>";

/// The construct that breaks a rule or a limit is refused before any of its
/// names, prefixes or namespace URIs is interned: the name table is unchanged
/// by a refused document whose earlier content it already knows — for every
/// refusal kind, on every reader path.
#[test]
fn a_refused_document_leaves_the_name_table_unchanged() {
    let long_name = "n".repeat(8 * 1024);
    let long_uri = format!("urn:{}", "u".repeat(8 * 1024));
    let mut many_decls = String::from("<r><x");
    for i in 0..2_000 {
        many_decls.push_str(&format!(" xmlns:fresh{i}=\"urn:fresh-{i}\""));
    }
    many_decls.push_str("/></r>");
    let mut many_attrs = String::from("<r><x");
    for i in 0..2_000 {
        many_attrs.push_str(&format!(" fresh-attr-{i}=\"1\""));
    }
    many_attrs.push_str("/></r>");
    let text_cases: Vec<(String, ParseLimits)> = vec![
        (
            format!("<r><{long_name}/></r>"),
            l().with_max_name_bytes(1024),
        ),
        (
            format!("<r><{long_name}:x xmlns:{long_name}=\"urn:p\"/></r>"),
            l().with_max_name_bytes(1024),
        ),
        (
            format!("<r><p:x xmlns:p=\"{long_uri}\"/></r>"),
            l().with_max_namespace_uri_bytes(1024),
        ),
        (many_decls, l().with_max_namespace_declarations(32)),
        (many_attrs, l().with_max_attributes(500)),
        (
            "<r><r><r><fresh-c/></r></r></r>".to_string(),
            l().with_max_depth(3),
        ),
        (
            "<r><fresh-x a=\"1\" fresh-y=\"2\"/></r>".to_string(),
            l().with_max_nodes(2),
        ),
        // Not-well-formed tags intern nothing of themselves either.
        (
            "<r><fresh-z xmlns:fresh-p=\"urn:fresh-q\"a=\"1\"/></r>".to_string(),
            l(),
        ),
        (
            "<r><fresh-z xmlns:fresh-p=\"urn:fresh-q\" a=\"\u{1}\"/></r>".to_string(),
            l(),
        ),
        // Rules the callers checked after interning the element.
        (format!("<r><fresh-a{long_name} a=\"&bogus;\"/></r>"), l()),
        (
            format!("<r><fresh-b fresh-attr{long_name}=\"1\" b=\"&bogus;\"/></r>"),
            l(),
        ),
        (format!("<r><fresh-c{long_name} p:a=\"1\"/></r>"), l()),
        (format!("<r><q:fresh-d{long_name}/></r>"), l()),
        (
            "<r><fresh-e xmlns:fresh-p=\"urn:fresh-q\" fresh-p:a=\"1\" fresh-p:a=\"2\"/></r>"
                .to_string(),
            l(),
        ),
    ];
    let mut cases: Vec<(Vec<u8>, ParseLimits)> = text_cases
        .into_iter()
        .map(|(x, l)| (x.into_bytes(), l))
        .collect();
    // Bytes that are not UTF-8, in a value and in a name.
    cases.push((
        [
            &b"<r><fresh-f"[..],
            long_name.as_bytes(),
            b" a=\"\xFF\"/></r>",
        ]
        .concat(),
        l(),
    ));
    cases.push((
        [
            &b"<r><fresh-g"[..],
            long_name.as_bytes(),
            b" a\xFF=\"1\"/></r>",
        ]
        .concat(),
        l(),
    ));
    let shown = |xml: &[u8]| -> String { String::from_utf8_lossy(xml).chars().take(80).collect() };
    for v11 in [false, true] {
        let ss = schema_set(v11);
        // Warm up: everything a valid run interns for these `r` elements.
        assert_eq!(drive2(&ss, WARM_UP, &l(), true), Outcome::Ok);
        #[cfg(feature = "xsd11")]
        assert_eq!(doc::typed(&ss, WARM_UP, &l()), Outcome::Ok);
        for (xml, limits) in &cases {
            let before = table_usage(&ss.name_table);
            let outcome = drive2(&ss, xml, limits, true);
            assert!(
                matches!(outcome, Outcome::Limit(_) | Outcome::Wf(_)),
                "{outcome:?} for {}",
                shown(xml)
            );
            assert_eq!(
                table_usage(&ss.name_table),
                before,
                "driver, {outcome:?} for {}",
                shown(xml)
            );
            #[cfg(feature = "xsd11")]
            {
                let outcome = doc::typed(&ss, xml, limits);
                assert!(
                    matches!(outcome, Outcome::Limit(_) | Outcome::Wf(_)),
                    "{outcome:?} for {}",
                    shown(xml)
                );
                assert_eq!(
                    table_usage(&ss.name_table),
                    before,
                    "typed build, {outcome:?} for {}",
                    shown(xml)
                );
            }
        }
    }
    #[cfg(feature = "xsd11")]
    {
        use bumpalo::Bump;
        use xsd_schema::document::{BufferDocumentBuilder, BufferDocumentOptions};
        let names = xsd_schema::namespace::NameTable::new();
        let build = |xml: &[u8], limits: &ParseLimits| {
            let arena = Bump::new();
            BufferDocumentBuilder::new(&arena, &names, None, BufferDocumentOptions::default())
                .expect("builder")
                .with_parse_limits(limits.clone())
                .build(xml)
                .map(|_| ())
                .map_err(doc::of_document_error)
        };
        build(WARM_UP, &l()).expect("warm-up");
        for (xml, limits) in &cases {
            let before = table_usage(&names);
            let outcome = build(xml, limits).expect_err("refused");
            assert!(
                matches!(outcome, Outcome::Limit(_) | Outcome::Wf(_)),
                "{outcome:?} for {}",
                shown(xml)
            );
            assert_eq!(
                table_usage(&names),
                before,
                "builder, {outcome:?} for {}",
                shown(xml)
            );
        }
    }
}

#[test]
fn no_limits_by_default() {
    let mut deep = String::new();
    for _ in 0..2_000 {
        deep.push_str("<r>");
    }
    for _ in 0..2_000 {
        deep.push_str("</r>");
    }
    assert_all_readers(deep.as_bytes(), &Outcome::Ok, &ParseLimits::default());
    assert!(ParseLimits::default().is_unlimited());
}

// ══ Fix round ═════════════════════════════════════════════════════════════

// ── XML 1.1 (Second Edition) and Namespaces in XML 1.1 (Second Edition) ───

const V11: &str = "<?xml version=\"1.1\"?>";

fn v11(body: &str) -> String {
    format!("{V11}{body}")
}

/// XML 1.1 \[2\] `Char ::= [#x1-#xD7FF] | [#xE000-#xFFFD] | [#x10000-#x10FFFF]`,
/// \[2a\] `RestrictedChar ::= [#x1-#x8] | [#xB-#xC] | [#xE-#x1F] | [#x7F-#x84] |
/// [#x86-#x9F]`, \[1\] `document ::= ( prolog element Misc* ) - ( Char*
/// RestrictedChar Char* )`: references to the restricted characters are
/// legal, the characters themselves are not.
#[test]
fn xml_1_1_documents_are_read_with_xml_1_1_rules() {
    let cases: Vec<(String, Outcome)> = vec![
        (v11("<r>&#x7;</r>"), Outcome::Ok),
        (v11("<r a=\"&#x1;\"/>"), Outcome::Ok),
        (
            v11("<r>&#x1F;&#xB;&#xC;&#x7F;&#x80;&#x84;&#x86;&#x9F;</r>"),
            Outcome::Ok,
        ),
        (v11("<r>&#x0;</r>"), wf(R::LegalCharacter)),
        (v11("<r>&#xFFFE;</r>"), wf(R::LegalCharacter)),
        (v11("<r>\u{80}</r>"), wf(R::Char)),
        (v11("<r>\u{84}</r>"), wf(R::Char)),
        (v11("<r>\u{86}</r>"), wf(R::Char)),
        (v11("<r a=\"\u{9F}\"/>"), wf(R::Char)),
        (v11("<r>\u{7F}</r>"), wf(R::Char)),
        (v11("<r><!--\u{7F}--></r>"), wf(R::Char)),
        (v11("<r><?pi \u{90}?></r>"), wf(R::Char)),
        (v11("<r><![CDATA[\u{81}]]></r>"), wf(R::Char)),
        (v11("<r>\u{1}</r>"), wf(R::Char)),
        // XML 1.0 is unchanged: the references are refused, C1 raw allowed.
        ("<r>&#x7;</r>".into(), wf(R::LegalCharacter)),
        ("<r>\u{80}\u{9F}\u{7F}</r>".into(), Outcome::Ok),
    ];
    for (xml, expected) in &cases {
        assert_all_readers(xml.as_bytes(), expected, &ParseLimits::none());
    }
}

/// XML 1.1 §2.11: "the XML processor must behave as if it normalized all line
/// breaks … by translating all of the following to a single #xA character:
/// … the two-character sequence #xD #x85, the single character #x85, the
/// single character #x2028 …"; "it is a fatal error to use them within the
/// XML declaration".
#[test]
fn xml_1_1_line_ends() {
    let cases: Vec<(String, Outcome)> = vec![
        (v11("<r>a\u{85}b\u{2028}c\r\u{85}d</r>"), Outcome::Ok),
        (v11("<r a=\"x\u{85}y\"/>"), Outcome::Ok),
        // As white space inside markup: not supported by this reader.
        (v11("<r\u{85}a=\"1\"/>"), wf(R::Xml11LineEndInMarkup)),
        (v11("<r a=\"1\"\u{2028}/>"), wf(R::Xml11LineEndInMarkup)),
        (v11("<r></r\u{85}>"), wf(R::Xml11LineEndInMarkup)),
        // … also inside the document type declaration.
        (v11("<!DOCTYPE\u{85}r><r/>"), wf(R::Xml11LineEndInMarkup)),
        (v11("<!DOCTYPE r\u{85}><r/>"), wf(R::Xml11LineEndInMarkup)),
        (
            v11("<!DOCTYPE r SYSTEM\u{85}\"x\"><r/>"),
            wf(R::Xml11LineEndInMarkup),
        ),
        (
            v11("<!DOCTYPE r PUBLIC \"a\"\u{2028}\"x\"><r/>"),
            wf(R::Xml11LineEndInMarkup),
        ),
        (
            v11("<!DOCTYPE r [<!ELEMENT\u{85}r ANY>]><r/>"),
            wf(R::Xml11LineEndInMarkup),
        ),
        (
            "<?xml version=\"1.1\"\u{85}?><r/>".into(),
            wf(R::XmlDeclaration),
        ),
        // XML 1.0: U+0085 and U+2028 are characters, not white space.
        ("<r\u{85}a=\"1\"/>".into(), wf(R::StartTag)),
        (
            "<!DOCTYPE\u{85}r><r/>".into(),
            wf(R::DocumentTypeDeclaration),
        ),
        ("<r>a\u{85}b</r>".into(), Outcome::Ok),
    ];
    for (xml, expected) in &cases {
        assert_all_readers(xml.as_bytes(), expected, &ParseLimits::none());
    }
}

#[cfg(feature = "xsd11")]
#[test]
fn xml_1_1_line_ends_are_normalized_in_the_document() {
    use xsd_schema::navigator::DomNavigator;
    let arena = bumpalo::Bump::new();
    let names = xsd_schema::namespace::NameTable::new();
    let xml = v11("<r a=\"x\u{85}y\u{2028}z\">a\u{85}b\u{2028}c\r\u{85}d\r\ne</r>");
    let doc =
        xsd_schema::document::BufferDocument::from_reader_default(xml.as_bytes(), &arena, &names)
            .expect("well-formed");
    let mut nav = doc.create_navigator();
    assert!(nav.move_to_first_child());
    assert_eq!(nav.value(), "a\nb\nc\nd\ne");
    assert!(nav.move_to_first_attribute());
    assert_eq!(nav.value(), "x y z");
}

/// Namespaces in XML 1.1 §6.1: "The attribute value in a namespace declaration
/// for a prefix may be empty. This has the effect, within the scope of the
/// declaration, of removing any association of the prefix with a namespace
/// name"; NSC: Prefix Declared: "Furthermore, the attribute value in the
/// innermost such declaration must not be an empty string"; NSC: Reserved
/// Prefixes: `xml` "must not be undeclared", `xmlns` "must not be declared or
/// undeclared".
#[test]
fn namespaces_1_1_prefixes_may_be_undeclared() {
    let cases: Vec<(String, Outcome)> = vec![
        (
            v11("<r xmlns:p=\"urn:p\"><e xmlns:p=\"\"><f/></e></r>"),
            Outcome::Ok,
        ),
        (
            v11("<r xmlns:p=\"urn:p\"><e xmlns:p=\"\"><p:f/></e></r>"),
            wf(R::PrefixDeclared),
        ),
        (
            v11("<r xmlns:p=\"urn:p\"><p:e xmlns:p=\"\"/></r>"),
            wf(R::PrefixDeclared),
        ),
        (
            v11("<r xmlns:p=\"urn:p\"><e xmlns:p=\"\" p:a=\"1\"/></r>"),
            wf(R::PrefixDeclared),
        ),
        (
            v11("<r xmlns:p=\"urn:p\"><e xmlns:p=\"\"><g xmlns:p=\"urn:q\"><p:h/></g></e></r>"),
            Outcome::Ok,
        ),
        (
            v11("<r xmlns:p=\"urn:p\"><e xmlns:p=\"\"/><p:f/></r>"),
            Outcome::Ok,
        ),
        (v11("<r xmlns:xml=\"\"/>"), wf(R::ReservedPrefix)),
        (v11("<r xmlns:xmlns=\"\"/>"), wf(R::ReservedPrefix)),
        // Namespaces in XML 1.0 is unchanged.
        (
            "<r xmlns:p=\"urn:p\"><e xmlns:p=\"\"/></r>".into(),
            wf(R::PrefixUndeclaring),
        ),
    ];
    for (xml, expected) in &cases {
        assert_all_readers(xml.as_bytes(), expected, &ParseLimits::none());
    }
}

/// Only `version="1.1"` selects XML 1.1: no declaration is 1.0 (XML 1.1
/// §4.3.4), another `1.x` is read as 1.0 (XML 1.0 §2.8 Note).
#[test]
fn other_versions_are_read_as_xml_1_0() {
    for prolog in [
        "",
        "<?xml version=\"1.0\"?>",
        "<?xml version=\"1.2\"?>",
        "<?xml version=\"1.10\"?>",
    ] {
        let xml = format!("{prolog}<r>&#x7;</r>");
        assert_all_readers(xml.as_bytes(), &wf(R::LegalCharacter), &ParseLimits::none());
    }
}

#[test]
fn the_schema_reader_honours_xml_1_1() {
    load_schema_text(&format!(
        "{V11}<xs:schema {XS}><xs:annotation><xs:documentation>a &#x7; bell</xs:documentation></xs:annotation></xs:schema>"
    ))
    .expect("XML 1.1 references to restricted characters");
    match load_schema_text(&format!(
        "{V11}<xs:schema {XS}><xs:annotation><xs:documentation>\u{80}</xs:documentation></xs:annotation></xs:schema>"
    )) {
        Err(SchemaError::XmlError { message, .. }) => {
            assert!(message.contains("XML 1.1 §2.2"), "{message}")
        }
        other => panic!("{other:?}"),
    }
}

/// Namespaces in XML 1.1 §6.1: an empty value in a declaration for a prefix
/// removes the prefix's association within the declaration's scope; NSC
/// Prefix Declared: the innermost declaration of a used prefix is not empty.
/// XSD Part 2 §3.3.18 (QName) resolves a QName with the in-scope namespaces
/// of its element, so an undeclared prefix is unbound in every QName-valued
/// attribute and in the XPath of selectors, fields and assertions of a
/// schema document — not bound to the namespace name "".
#[test]
fn xml_1_1_schema_documents_undeclare_prefixes() {
    let load = |body: &str, v11: bool| -> Result<(), String> {
        let xsd = format!(
            "{V11}<xs:schema {XS} xmlns:p=\"urn:p\" targetNamespace=\"urn:p\">\
             <xs:simpleType name=\"t\"><xs:restriction base=\"xs:string\"/></xs:simpleType>\
             <xs:element name=\"g\" type=\"xs:string\"/>{body}</xs:schema>"
        );
        let mut ss = schema_set_empty(v11);
        load_and_process_schema(xsd.as_bytes(), "file:///wf-undeclare.xsd", &mut ss, None)
            .map(|_| ())
            .map_err(|e| e.to_string())
    };
    let unbound = |body: &str, v11: bool| match load(body, v11) {
        Err(m) => {
            assert!(
                m.contains("'p'") || m.contains("prefix `p`"),
                "{body}: the error does not name the unbound prefix: {m}"
            );
            assert!(
                m.contains("wf-undeclare.xsd:1:"),
                "{body}: no location: {m}"
            );
        }
        Ok(()) => panic!("{body}: loaded with an undeclared prefix"),
    };
    let key = "<xs:element name=\"list\"><xs:complexType><xs:sequence>\
               <xs:element name=\"item\" maxOccurs=\"unbounded\" type=\"xs:string\"/>\
               </xs:sequence></xs:complexType>";
    for v11 in [false, true] {
        // QName-valued attributes.
        unbound("<xs:element name=\"e\" xmlns:p=\"\" type=\"p:t\"/>", v11);
        unbound(
            "<xs:simpleType name=\"u\" xmlns:p=\"\"><xs:restriction base=\"p:t\"/></xs:simpleType>",
            v11,
        );
        unbound(
            "<xs:simpleType name=\"u\" xmlns:p=\"\"><xs:list itemType=\"p:t\"/></xs:simpleType>",
            v11,
        );
        unbound(
            "<xs:simpleType name=\"u\" xmlns:p=\"\"><xs:union memberTypes=\"xs:int p:t\"/></xs:simpleType>",
            v11,
        );
        unbound(
            "<xs:complexType name=\"c\" xmlns:p=\"\"><xs:sequence><xs:element ref=\"p:g\"/></xs:sequence></xs:complexType>",
            v11,
        );
        unbound(
            "<xs:element name=\"h\" xmlns:p=\"\" substitutionGroup=\"p:g\" type=\"xs:string\"/>",
            v11,
        );
        // The XPath of a selector and of a field.
        unbound(
            &format!("{key}<xs:key name=\"k\" xmlns:p=\"\"><xs:selector xpath=\"p:item\"/><xs:field xpath=\".\"/></xs:key></xs:element>"),
            v11,
        );
        unbound(
            &format!("{key}<xs:key name=\"k\"><xs:selector xpath=\"p:item\"/><xs:field xmlns:p=\"\" xpath=\"p:x\"/></xs:key></xs:element>"),
            v11,
        );
        // The undeclaration ends with its element: the outer binding is back.
        load(
            "<xs:element name=\"e\" xmlns:p=\"\" type=\"xs:string\"/><xs:element name=\"f\" type=\"p:t\"/>",
            v11,
        )
        .expect("the outer binding of p after the undeclaring element");
        // Redeclared inside the undeclaring element.
        load(
            "<xs:element name=\"e\" xmlns:p=\"\"><xs:simpleType xmlns:p=\"urn:p\"><xs:restriction base=\"p:t\"/></xs:simpleType></xs:element>",
            v11,
        )
        .expect("p redeclared inside the undeclaring element");
    }
    #[cfg(feature = "xsd11")]
    {
        // The XPath of a type alternative (XSD 1.1), checked when the schema
        // is loaded.
        unbound(
            "<xs:element name=\"a\" type=\"xs:anyType\">\
             <xs:alternative xmlns:p=\"\" test=\"@p:b\" type=\"xs:string\"/></xs:element>",
            true,
        );
        // The XPath of an assertion (XSD 1.1), checked when the schema is
        // loaded as well (Structures 1.1 §3.13.6.1: a static error makes the
        // schema invalid): `p:x` is a static error (XPST0081), not a test for
        // an unqualified `x`.
        unbound(
            "<xs:complexType name=\"c\"><xs:sequence><xs:element name=\"x\" type=\"xs:string\"/></xs:sequence>\
             <xs:assert xmlns:p=\"\" test=\"p:x\"/></xs:complexType>",
            true,
        );
    }
}

/// An empty schema set of the requested XSD version.
fn schema_set_empty(v11: bool) -> SchemaSet {
    #[cfg(feature = "xsd11")]
    {
        if v11 {
            return SchemaSet::xsd11();
        }
    }
    let _ = v11;
    SchemaSet::new()
}

// ── The XML declaration is reported ───────────────────────────────────────

#[derive(Default)]
struct Declarations {
    seen: Vec<(String, Option<String>, Option<bool>)>,
    refuse_encoding: Option<&'static str>,
}

impl ValidationEventHandler for Declarations {
    type Error = String;
    fn on_xml_declaration(&mut self, d: &xsd_schema::xml::XmlDeclaration) -> Result<(), String> {
        self.seen
            .push((d.version.clone(), d.encoding.clone(), d.standalone));
        match (self.refuse_encoding, d.encoding.as_deref()) {
            (Some(bad), Some(enc)) if enc.eq_ignore_ascii_case(bad) => {
                Err(format!("encoding {enc} refused"))
            }
            _ => Ok(()),
        }
    }
}

#[test]
fn the_xml_declaration_is_reported_to_the_handler() {
    let ss = schema_set(false);
    let run = |xml: &str, refuse: Option<&'static str>| {
        let validator = SchemaValidator::new(&ss, ValidationFlags::default());
        let (mut errors, mut warnings) = (Vec::new(), Vec::new());
        let mut runtime = validator.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        let mut h = Declarations {
            refuse_encoding: refuse,
            ..Default::default()
        };
        let r = drive_quick_xml_with(xml.as_bytes(), &mut runtime, &ss, &mut h)
            .map(|_| ())
            .map_err(|e| match e {
                DriveWithError::Hook(m) => m,
                other => format!("{other}"),
            });
        (h.seen, r)
    };
    let (seen, r) = run(
        "<?xml version='1.0' encoding='ISO-8859-1' standalone='yes'?><r/>",
        None,
    );
    assert_eq!(r, Ok(()));
    assert_eq!(
        seen,
        vec![("1.0".into(), Some("ISO-8859-1".into()), Some(true))]
    );
    let (seen, r) = run("<?xml version=\"1.1\"?><r/>", None);
    assert_eq!(r, Ok(()));
    assert_eq!(seen, vec![("1.1".into(), None, None)]);
    let (_, r) = run(
        "<?xml version=\"1.0\" encoding=\"iso-8859-1\"?><r/>",
        Some("ISO-8859-1"),
    );
    assert_eq!(r, Err("encoding iso-8859-1 refused".into()));
    let (seen, r) = run("<r/>", Some("UTF-8"));
    assert_eq!(r, Ok(()));
    assert!(seen.is_empty());
}

#[cfg(feature = "xsd11")]
#[test]
fn the_document_keeps_its_xml_declaration() {
    use bumpalo::Bump;
    use xsd_schema::document::{build_typed_document_with, BufferDocument, TypedBuildOptions};
    let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?><r/>";
    let arena = Bump::new();
    let names = xsd_schema::namespace::NameTable::new();
    let doc = BufferDocument::from_reader_default(xml.as_bytes(), &arena, &names).unwrap();
    let d = doc.xml_declaration().expect("kept");
    assert_eq!(
        (d.version.as_str(), d.encoding.as_deref(), d.standalone),
        ("1.0", Some("UTF-8"), Some(false))
    );
    assert_eq!(d.xml_version(), xsd_schema::xml::XmlVersion::V1_0);
    let doc = BufferDocument::from_reader_default("<r/>".as_bytes(), &arena, &names).unwrap();
    assert!(doc.xml_declaration().is_none());

    // The typed build keeps it too, and the inner handler sees it.
    let ss = schema_set(false);
    let validator = SchemaValidator::new(&ss, ValidationFlags::default());
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    let mut runtime = validator.start_run(CollectingValidationSink {
        errors: &mut errors,
        warnings: &mut warnings,
    });
    let mut h = Declarations::default();
    let built = build_typed_document_with(
        "<?xml version=\"1.1\"?><r/>".as_bytes(),
        &arena,
        &ss,
        &mut runtime,
        &mut h,
        TypedBuildOptions::default(),
    )
    .unwrap();
    assert_eq!(
        built.document.xml_declaration().map(|d| d.xml_version()),
        Some(xsd_schema::xml::XmlVersion::V1_1)
    );
    assert_eq!(h.seen.len(), 1);
}

// ── The DOCTYPE hook sees only a declaration that is well-formed ──────────

#[test]
fn on_doctype_sees_only_a_checked_declaration() {
    for xml in [
        "<!doctype r><r/>",
        "<!DocType r><r/>",
        "<!DOCTYPEr><r/>",
        "<!DOCTYPE r garbage><r/>",
    ] {
        let (h, r) = drive_doctypes(xml, false);
        assert!(h.seen.is_empty(), "{xml}: {:?}", h.seen);
        assert!(r.unwrap_err().contains("[28] doctypedecl"), "{xml}");
    }
}

// ── The document type declaration's grammar ──────────────────────────────

/// \[28\] `doctypedecl ::= '<!DOCTYPE' S Name (S ExternalID)? S? ('['
/// intSubset ']' S?)? '>'`, \[75\] `ExternalID ::= 'SYSTEM' S SystemLiteral |
/// 'PUBLIC' S PubidLiteral S SystemLiteral`, \[11\]–\[13\]; the internal
/// subset's markup is read for balance, its declarations are not processed.
#[test]
fn the_document_type_declaration_follows_its_grammar() {
    check(&[
        ("<!DOCTYPE r garbage><r/>", wf(R::DocumentTypeDeclaration)),
        ("<!DOCTYPE r SYSTEM><r/>", wf(R::DocumentTypeDeclaration)),
        ("<!DOCTYPE r SYSTEM 'a'b><r/>", wf(R::DocumentTypeDeclaration)),
        ("<!DOCTYPE r PUBLIC \"a\"><r/>", wf(R::DocumentTypeDeclaration)),
        ("<!DOCTYPE r PUBLIC \"a{b\" \"x\"><r/>", wf(R::DocumentTypeDeclaration)),
        ("<!DOCTYPE r SYSTEM \"x\" [ junk ]><r/>", wf(R::DocumentTypeDeclaration)),
        ("<!DOCTYPE r [<!-- a -- b -->]><r/>", wf(R::Comment)),
        ("<!DOCTYPE r [<?xml x?>]><r/>", wf(R::ProcessingInstruction)),
        ("<!DOCTYPE r [<!ELEMENT r ANY>] x><r/>", wf(R::DocumentTypeDeclaration)),
        ("<!DOCTYPE r [<!FOO r>]><r/>", wf(R::DocumentTypeDeclaration)),
        ("<!DOCTYPE r [ %pe ]><r/>", wf(R::DocumentTypeDeclaration)),
        ("<!DOCTYPE r SYSTEM \"x\"><r/>", Outcome::Ok),
        ("<!DOCTYPE r SYSTEM 'x'\n><r/>", Outcome::Ok),
        ("<!DOCTYPE r PUBLIC \"-//A//B 1.0//EN\" 'r.dtd'><r/>", Outcome::Ok),
        ("<!DOCTYPE r[<!ELEMENT r ANY>]><r/>", Outcome::Ok),
        (
            "<!DOCTYPE r PUBLIC '-//X//Y' \"r.dtd\" [\n  <!ELEMENT r (#PCDATA)>\n  <!ATTLIST r a CDATA \"x>y\">\n  <!ENTITY e \"]>\">\n  <!NOTATION n SYSTEM \"n\">\n  <!-- a > b -->\n  <?pi x?>\n  %pe;\n] ><r/>",
            Outcome::Ok,
        ),
    ]);
}

// ── A BOM split across `fill_buf` calls ───────────────────────────────────

/// A `BufRead` that returns one byte per `fill_buf`.
struct OneByte<'a> {
    data: &'a [u8],
    pos: usize,
}

impl std::io::Read for OneByte<'_> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.pos >= self.data.len() || out.is_empty() {
            return Ok(0);
        }
        out[0] = self.data[self.pos];
        self.pos += 1;
        Ok(1)
    }
}

impl std::io::BufRead for OneByte<'_> {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        let end = (self.pos + 1).min(self.data.len());
        Ok(&self.data[self.pos..end])
    }
    fn consume(&mut self, n: usize) {
        self.pos += n;
    }
}

fn one_byte(data: &[u8]) -> OneByte<'_> {
    OneByte { data, pos: 0 }
}

#[test]
fn a_bom_split_across_fills_is_a_bom() {
    let ss = schema_set(false);
    let drive = |xml: &[u8]| {
        let validator = SchemaValidator::new(&ss, ValidationFlags::default());
        let (mut errors, mut warnings) = (Vec::new(), Vec::new());
        let mut runtime = validator.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        drive_quick_xml_with(one_byte(xml), &mut runtime, &ss, &mut NoopHandler)
            .map(|_| ())
            .map_err(of_drive_with_error)
    };
    let ok: [&[u8]; 3] = [
        b"\xEF\xBB\xBF<r/>",
        b"\xEF\xBB\xBF<?xml version=\"1.0\"?><r/>",
        b"\xEF\xBB\xBF\n<r/>",
    ];
    for xml in ok {
        assert_eq!(drive(xml), Ok(()), "{xml:?}");
        #[cfg(feature = "xsd11")]
        {
            let arena = bumpalo::Bump::new();
            let names = xsd_schema::namespace::NameTable::new();
            assert!(
                xsd_schema::document::BufferDocument::from_reader_default(
                    one_byte(xml),
                    &arena,
                    &names
                )
                .is_ok(),
                "{xml:?}"
            );
        }
        let mut r = xsd_schema::parser::TrackedReader::from_reader(one_byte(xml));
        let mut buf = Vec::new();
        loop {
            let e = r.read_event(&mut buf).expect("well-formed");
            if e.is_eof() {
                break;
            }
        }
    }
    // A declaration after the BOM and white space is still misplaced, and
    // positions stay input offsets.
    assert_eq!(
        drive(b"\xEF\xBB\xBF <?xml version=\"1.0\"?><r/>"),
        Err(wf(R::XmlDeclaration))
    );
    let ss2 = schema_set(false);
    let validator = SchemaValidator::new(&ss2, ValidationFlags::default());
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    let mut runtime = validator.start_run(CollectingValidationSink {
        errors: &mut errors,
        warnings: &mut warnings,
    });
    match drive_quick_xml_with(
        one_byte(b"\xEF\xBB\xBF<r>\x01</r>"),
        &mut runtime,
        &ss2,
        &mut NoopHandler,
    ) {
        Err(DriveWithError::NotWellFormed(e)) => assert_eq!((e.rule(), e.position()), (R::Char, 6)),
        other => panic!("{other:?}"),
    }
}

// ── One offset system ────────────────────────────────────────────────────

#[derive(Default)]
struct Offsets {
    doctype: Vec<usize>,
    starts: Vec<usize>,
    ends: Vec<usize>,
}

impl ValidationEventHandler for Offsets {
    type Error = std::convert::Infallible;
    fn on_doctype(&mut self, d: DoctypeView<'_>) -> Result<(), Self::Error> {
        self.doctype.push(d.byte_pos);
        Ok(())
    }
    fn on_element_start_offset(&mut self, p: usize) -> Result<(), Self::Error> {
        self.starts.push(p);
        Ok(())
    }
    fn on_element_end_offset(&mut self, p: usize) -> Result<(), Self::Error> {
        self.ends.push(p);
        Ok(())
    }
}

/// Every offset a reader reports — hooks, source spans, error positions — is
/// a byte offset into the input, a leading BOM included.
#[test]
fn offsets_are_input_offsets_with_a_bom_too() {
    for bom in [&b""[..], b"\xEF\xBB\xBF"] {
        let xml = [bom, b"<!DOCTYPE r><r><e/></r>"].concat();
        let at = |needle: &[u8]| xml.windows(needle.len()).position(|w| w == needle).unwrap();
        let ss = schema_set(false);
        let validator = SchemaValidator::new(&ss, ValidationFlags::default());
        let (mut errors, mut warnings) = (Vec::new(), Vec::new());
        let mut runtime = validator.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        let mut h = Offsets::default();
        drive_quick_xml_with(&xml[..], &mut runtime, &ss, &mut h).unwrap();
        assert_eq!(h.doctype, vec![at(b"<!DOCTYPE")]);
        assert_eq!(h.starts, vec![at(b"<r>"), at(b"<e/>")]);
        assert_eq!(h.ends, vec![at(b"<e/>") + 4, xml.len()]);

        #[cfg(feature = "xsd11")]
        {
            let arena = bumpalo::Bump::new();
            let names = xsd_schema::namespace::NameTable::new();
            let doc = xsd_schema::document::BufferDocument::from_reader(
                &xml[..],
                &arena,
                &names,
                xsd_schema::document::BufferDocumentOptions::full(),
                None,
            )
            .unwrap();
            // Root(0), r(1), e(2).
            let span = doc.source_span(1).expect("tracked");
            assert_eq!((span.start, span.end), (at(b"<r>"), xml.len()));
        }

        let mut r = xsd_schema::parser::TrackedReader::from_bytes(&xml);
        let mut buf = Vec::new();
        let mut starts = Vec::new();
        loop {
            let e = r.read_event(&mut buf).unwrap();
            if e.is_eof() {
                break;
            }
            if e.is_start() || e.is_empty() {
                starts.push(e.span.start);
            }
        }
        assert_eq!(starts, vec![at(b"<r>"), at(b"<e/>")]);
    }
}

/// Every `TrackedReader` span starts where its event starts in the input — the
/// first one too, and with a byte-order mark: the `<` of markup, the first
/// character of (trimmed) text. (A document type declaration or an end tag
/// after skipped white space starts at that white space; see `read_event`.)
#[test]
fn tracked_reader_spans_start_at_their_event() {
    for bom in [&b""[..], b"\xEF\xBB\xBF"] {
        let xml = [
            bom,
            b"<?xml version=\"1.0\"?><!DOCTYPE r>\n<!--c-->\n<r> <?p x?>text<![CDATA[d]]><e/></r>",
        ]
        .concat();
        let at = |needle: &[u8]| xml.windows(needle.len()).position(|w| w == needle).unwrap();
        let mut r = xsd_schema::parser::TrackedReader::from_bytes(&xml);
        let mut buf = Vec::new();
        let mut starts = Vec::new();
        loop {
            let e = r.read_event(&mut buf).unwrap();
            if e.is_eof() {
                break;
            }
            let start = e.span.start;
            starts.push(start);
            assert_eq!(r.last_position(), start, "{bom:?}");
        }
        assert_eq!(
            starts,
            vec![
                at(b"<?xml"),
                at(b"<!DOCTYPE"),
                at(b"<!--"),
                at(b"<r>"),
                at(b"<?p"),
                at(b"text"),
                at(b"<![CDATA["),
                at(b"<e/>"),
                at(b"</r>"),
            ],
            "{bom:?}"
        );
        // After skipped white space: from the end of the previous event.
        let xml = [bom, b"<?xml version=\"1.0\"?>\n<!DOCTYPE r><r><e/>\n</r>"].concat();
        let mut r = xsd_schema::parser::TrackedReader::from_bytes(&xml);
        let mut starts = Vec::new();
        loop {
            let e = r.read_event(&mut buf).unwrap();
            if e.is_eof() {
                break;
            }
            starts.push(e.span.start);
        }
        let b = bom.len();
        assert_eq!(starts, vec![b, b + 21, b + 34, b + 37, b + 41], "{bom:?}");
    }
}

// ── The schema reader ─────────────────────────────────────────────────────

#[test]
fn the_schema_reader_refuses_what_the_instance_readers_refuse() {
    let cases: Vec<(Vec<u8>, R)> = vec![
        ([format!("<xs:schema {XS}><!-- ").as_bytes(), b"\xFF", b" --></xs:schema>"].concat(), R::Encoding),
        ([format!("<xs:schema {XS}><?pi ").as_bytes(), b"\xFF", b"?></xs:schema>"].concat(), R::Encoding),
        (
            [
                format!("<xs:schema {XS}><xs:annotation><xs:documentation><![CDATA[").as_bytes(),
                b"\xFF",
                b"]]></xs:documentation></xs:annotation></xs:schema>",
            ]
            .concat(),
            R::Encoding,
        ),
        (
            format!("<xs:schema {XS}><xs:annotation><xs:appinfo><q:x/></xs:appinfo></xs:annotation></xs:schema>").into_bytes(),
            R::PrefixDeclared,
        ),
        (
            format!("<xs:schema {XS}><xs:annotation><xs:appinfo><x q:a=\"1\"/></xs:appinfo></xs:annotation></xs:schema>").into_bytes(),
            R::PrefixDeclared,
        ),
        (format!("<xs:schema {XS} q:a=\"1\"/>").into_bytes(), R::PrefixDeclared),
        (format!("<xs:schema {XS}></xs:element>").into_bytes(), R::ElementTypeMatch),
    ];
    for (xsd, rule) in &cases {
        let mut ss = SchemaSet::new();
        match load_and_process_schema(xsd, "file:///wf-schema.xsd", &mut ss, None) {
            Err(SchemaError::XmlError { message, location }) => {
                assert!(
                    message.contains(rule.citation()),
                    "{:?}: {message}",
                    String::from_utf8_lossy(xsd)
                );
                assert!(location.is_some(), "{message}");
            }
            other => panic!("{:?}: {other:?}", String::from_utf8_lossy(xsd)),
        }
    }
}

// ── Limits on what lies outside the root element ─────────────────────────

#[test]
fn white_space_outside_the_root_and_the_doctype_are_limited() {
    limit_case(
        "<r/>\n          \n",
        l().with_max_text_bytes(12),
        l().with_max_text_bytes(11),
        ParseLimit::TextSize,
    );
    limit_case(
        "<!DOCTYPE r [<!ELEMENT r ANY>]><r/>",
        l().with_max_text_bytes(20),
        l().with_max_text_bytes(19),
        ParseLimit::TextSize,
    );
}

// ── Truncation names the innermost open element ──────────────────────────

/// The `NotWellFormed` errors every instance reader gives for `xml`.
fn not_well_formed_everywhere(xml: &[u8]) -> Vec<(&'static str, xsd_schema::xml::NotWellFormed)> {
    let ss = schema_set(false);
    let mut out = Vec::new();
    let validator = SchemaValidator::new(&ss, ValidationFlags::default());
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    {
        let mut runtime = validator.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        match drive_quick_xml(xml, &mut runtime, &ss) {
            Err(DriveError::NotWellFormed(e)) => out.push(("drive_quick_xml", e)),
            other => panic!("drive_quick_xml {xml:?}: {other:?}"),
        }
    }
    {
        let mut runtime = validator.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        match drive_quick_xml_with(xml, &mut runtime, &ss, &mut NoopHandler) {
            Err(DriveWithError::NotWellFormed(e)) => out.push(("drive_quick_xml_with", e)),
            other => panic!("drive_quick_xml_with {xml:?}: {other:?}"),
        }
    }
    #[cfg(feature = "xsd11")]
    {
        use xsd_schema::document::{
            build_typed_document_with, BufferDocument, BufferDocumentError, TypedBuildError,
            TypedBuildOptions,
        };
        let arena = bumpalo::Bump::new();
        let names = xsd_schema::namespace::NameTable::new();
        match BufferDocument::from_reader_default(xml, &arena, &names) {
            Err(BufferDocumentError::NotWellFormed(e)) => out.push(("from_reader", e)),
            other => panic!("from_reader {xml:?}: {:?}", other.map(|_| ())),
        }
        let mut runtime = validator.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        match build_typed_document_with(
            xml,
            &arena,
            &ss,
            &mut runtime,
            &mut NoopHandler,
            TypedBuildOptions::default(),
        ) {
            Err(TypedBuildError::Document(BufferDocumentError::NotWellFormed(e))) => {
                out.push(("typed build", e))
            }
            other => panic!("typed build {xml:?}: {:?}", other.map(|_| ())),
        }
    }
    out
}

/// XML 1.0 \[39\] `element ::= EmptyElemTag | STag content ETag`: input that
/// ends inside an element is refused naming the innermost open element as
/// written (its qualified name) and how many elements are open.
#[test]
fn truncation_names_the_innermost_open_element() {
    let cases: &[(&str, &[&str])] = &[
        ("<r>", &["'r'", "(1 element not closed)"]),
        (
            "<a><p:b xmlns:p=\"urn:p\"><c/>",
            &["'p:b'", "(2 elements not closed)"],
        ),
        ("<r><e></e>text", &["'r'", "(1 element not closed)"]),
        // Inside markup: which markup, and the element it is in.
        (
            "<r><e>text<!-- x",
            &["a comment", "'e'", "(2 elements not closed)"],
        ),
        (
            "<r><e a=\"1",
            &["an attribute value", "'r'", "(1 element not closed)"],
        ),
    ];
    for (xml, parts) in cases {
        for (path, e) in not_well_formed_everywhere(xml.as_bytes()) {
            assert_eq!(e.rule(), R::Truncated, "{path} {xml:?}: {e}");
            for part in *parts {
                assert!(
                    e.message().contains(part),
                    "{path} {xml:?}: {e} lacks {part}"
                );
            }
        }
    }
    // Before the root element nothing is open: no element is named.
    for (path, e) in not_well_formed_everywhere(b"<!-- x") {
        assert_eq!(e.rule(), R::Truncated, "{path}: {e}");
        assert!(!e.message().contains("not closed"), "{path}: {e}");
    }
}

// ── I/O errors are I/O errors ─────────────────────────────────────────────

/// A source that serves `ok` and then fails.
struct Failing<'a> {
    ok: &'a [u8],
    pos: usize,
}

impl std::io::Read for Failing<'_> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let n = std::io::BufRead::fill_buf(self)?.len().min(out.len());
        out[..n].copy_from_slice(&self.ok[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

impl std::io::BufRead for Failing<'_> {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        if self.pos < self.ok.len() {
            Ok(&self.ok[self.pos..])
        } else {
            Err(std::io::Error::other("the disk is on fire"))
        }
    }
    fn consume(&mut self, n: usize) {
        self.pos += n;
    }
}

/// The source's own `std::io::Error`, with its kind and message, behind
/// every reader's `Io` variant and through `source()`.
#[test]
fn io_errors_are_io_errors() {
    use std::error::Error as _;
    let is_ours = |e: &std::io::Error| {
        e.kind() == std::io::ErrorKind::Other && e.to_string().contains("the disk is on fire")
    };
    let ss = schema_set(false);
    let validator = SchemaValidator::new(&ss, ValidationFlags::default());
    for ok in [&b""[..], b"<r><a>"] {
        let (mut errors, mut warnings) = (Vec::new(), Vec::new());
        {
            let mut runtime = validator.start_run(CollectingValidationSink {
                errors: &mut errors,
                warnings: &mut warnings,
            });
            match drive_quick_xml(Failing { ok, pos: 0 }, &mut runtime, &ss) {
                Err(e @ DriveError::Io(_)) => {
                    let DriveError::Io(io) = &e else {
                        unreachable!()
                    };
                    assert!(is_ours(io), "{e}");
                    let source = e.source().and_then(|s| s.downcast_ref::<std::io::Error>());
                    assert!(source.is_some_and(is_ours), "{e}: source {source:?}");
                }
                other => panic!("drive_quick_xml {ok:?}: {other:?}"),
            }
        }
        {
            let mut runtime = validator.start_run(CollectingValidationSink {
                errors: &mut errors,
                warnings: &mut warnings,
            });
            match drive_quick_xml_with(Failing { ok, pos: 0 }, &mut runtime, &ss, &mut NoopHandler)
            {
                Err(DriveWithError::Io(io)) => assert!(is_ours(&io), "{io}"),
                other => panic!("drive_quick_xml_with {ok:?}: {other:?}"),
            }
        }
        #[cfg(feature = "xsd11")]
        {
            use xsd_schema::document::{
                build_typed_document_with, BufferDocument, BufferDocumentError, TypedBuildError,
                TypedBuildOptions,
            };
            let arena = bumpalo::Bump::new();
            let names = xsd_schema::namespace::NameTable::new();
            match BufferDocument::from_reader_default(Failing { ok, pos: 0 }, &arena, &names) {
                Err(e @ BufferDocumentError::Io(_)) => {
                    let source = e.source().and_then(|s| s.downcast_ref::<std::io::Error>());
                    assert!(source.is_some_and(is_ours), "{e}");
                }
                other => panic!("from_reader {ok:?}: {:?}", other.map(|_| ())),
            }
            let mut runtime = validator.start_run(CollectingValidationSink {
                errors: &mut errors,
                warnings: &mut warnings,
            });
            match build_typed_document_with(
                Failing { ok, pos: 0 },
                &arena,
                &ss,
                &mut runtime,
                &mut NoopHandler,
                TypedBuildOptions::default(),
            ) {
                Err(e @ TypedBuildError::Document(BufferDocumentError::Io(_))) => {
                    // `TypedBuildError::source` is the document error's own.
                    let source = e.source().and_then(|s| s.downcast_ref::<std::io::Error>());
                    assert!(source.is_some_and(is_ours), "{e}");
                }
                other => panic!("typed build {ok:?}: {:?}", other.map(|_| ())),
            }
        }
        // The schema-document reader.
        let mut r = xsd_schema::parser::TrackedReader::from_reader(Failing { ok, pos: 0 });
        let mut buf = Vec::new();
        let e = loop {
            match r.read_event(&mut buf) {
                Ok(ev) if ev.is_eof() => panic!("TrackedReader {ok:?}: no error"),
                Ok(_) => {}
                Err(e) => break e,
            }
        };
        assert!(
            e.to_string().contains("I/O error: the disk is on fire"),
            "{e}"
        );
    }
}

// ── Fix round 4: namespace values, the scope limit ───────────────────────

/// Records which element and attribute callbacks a tag reached.
#[derive(Default)]
struct Callbacks {
    seen: Vec<String>,
}

impl ValidationEventHandler for Callbacks {
    type Error = String;
    fn before_element(
        &mut self,
        e: xsd_schema::validation::ElementStartView<'_>,
    ) -> Result<(), String> {
        self.seen.push(format!("element {}", e.local_name));
        Ok(())
    }
    fn before_attribute(
        &mut self,
        a: xsd_schema::validation::AttributeView<'_>,
    ) -> Result<(), String> {
        self.seen
            .push(format!("attribute {{{}}}{}", a.namespace_uri, a.local_name));
        Ok(())
    }
}

/// `rule` on every reader path, and on the driver before any callback or
/// validation error.
fn refused_before_callbacks(xml: &str, rule: R) {
    assert_all_readers(xml.as_bytes(), &wf(rule), &ParseLimits::none());
    for v11 in [false, true] {
        let ss = schema_set(v11);
        let validator = SchemaValidator::new(&ss, ValidationFlags::default());
        let (mut errors, mut warnings) = (Vec::new(), Vec::new());
        let mut h = Callbacks::default();
        {
            let mut runtime = validator.start_run(CollectingValidationSink {
                errors: &mut errors,
                warnings: &mut warnings,
            });
            match drive_quick_xml_with(xml.as_bytes(), &mut runtime, &ss, &mut h) {
                Err(DriveWithError::NotWellFormed(e)) => assert_eq!(e.rule(), rule, "{xml:?}"),
                other => panic!("{xml:?}: {other:?}"),
            }
        }
        assert!(h.seen.is_empty(), "{xml:?}: callbacks {:?}", h.seen);
        assert!(errors.is_empty(), "{xml:?}: validation errors {errors:?}");
    }
}

/// XML 1.0 §3.3.3 / §2.11, XML 1.1 §2.11, Namespaces in XML §6.3: namespace
/// names are compared after attribute-value normalization of the
/// declaration's value with the document's version — the same
/// normalization the readers hand to their callers.
#[test]
fn namespace_names_are_compared_as_the_readers_normalize_them() {
    // XML 1.0: a raw LINE SEPARATOR or NEL is a character, kept as it is; a
    // reference to it gives the same character.
    for xml in [
        "<r xmlns:p=\"urn:\u{2028}&amp;\" xmlns:q=\"urn:&#x2028;&amp;\" p:a=\"1\" q:a=\"2\"/>",
        "<r xmlns:p=\"urn:\u{2028}\" xmlns:q=\"urn:&#x2028;\" p:a=\"1\" q:a=\"2\"/>",
        "<r xmlns:p=\"urn:\u{85}x&#x41;\" xmlns:q=\"urn:&#x85;xA\" p:a=\"1\" q:a=\"2\"/>",
        // CR LF, a lone CR, a tab: one space each, in both versions.
        "<r xmlns:p=\"urn:a\r\nb\" xmlns:q=\"urn:a b\" p:a=\"1\" q:a=\"2\"/>",
        "<r xmlns:p=\"urn:a\rb\" xmlns:q=\"urn:a b\" p:a=\"1\" q:a=\"2\"/>",
        "<r xmlns:p=\"urn:a\tb\" xmlns:q=\"urn:a b\" p:a=\"1\" q:a=\"2\"/>",
        // CR NEL is two characters in XML 1.0: a space (the CR) and NEL.
        "<r xmlns:p=\"urn:a\r\u{85}b\" xmlns:q=\"urn:a \u{85}b\" p:a=\"1\" q:a=\"2\"/>",
    ] {
        refused_before_callbacks(xml, R::AttributesUnique);
    }
    // XML 1.0: NEL and LINE SEPARATOR are not white space — distinct names.
    for xml in [
        "<r xmlns:p=\"urn:a\u{2028}b&amp;\" xmlns:q=\"urn:a b&amp;\" p:a=\"1\" q:a=\"2\"/>",
        "<r xmlns:p=\"urn:a\u{85}b&amp;\" xmlns:q=\"urn:a b&amp;\" p:a=\"1\" q:a=\"2\"/>",
        "<r xmlns:p=\"urn:a\r\u{85}b\" xmlns:q=\"urn:a b\" p:a=\"1\" q:a=\"2\"/>",
    ] {
        assert_all_readers(xml.as_bytes(), &Outcome::Ok, &ParseLimits::none());
    }
    // XML 1.1: CR NEL is one line end, NEL and LINE SEPARATOR are line ends.
    for body in [
        "<r xmlns:p=\"urn:a\r\u{85}b\" xmlns:q=\"urn:a b\" p:a=\"1\" q:a=\"2\"/>",
        "<r xmlns:p=\"urn:a\r\nb\" xmlns:q=\"urn:a b\" p:a=\"1\" q:a=\"2\"/>",
        "<r xmlns:p=\"urn:a\rb\" xmlns:q=\"urn:a b\" p:a=\"1\" q:a=\"2\"/>",
        "<r xmlns:p=\"urn:a\u{85}b\" xmlns:q=\"urn:a b\" p:a=\"1\" q:a=\"2\"/>",
        "<r xmlns:p=\"urn:a\u{2028}b&amp;\" xmlns:q=\"urn:a b&amp;\" p:a=\"1\" q:a=\"2\"/>",
    ] {
        refused_before_callbacks(&v11(body), R::AttributesUnique);
    }
    for body in [
        // CR NEL is one space, not two.
        "<r xmlns:p=\"urn:a\r\u{85}b\" xmlns:q=\"urn:a  b\" p:a=\"1\" q:a=\"2\"/>",
        // A reference to NEL is the character, not a space.
        "<r xmlns:p=\"urn:a&#x85;b\" xmlns:q=\"urn:a b\" p:a=\"1\" q:a=\"2\"/>",
    ] {
        assert_all_readers(v11(body).as_bytes(), &Outcome::Ok, &ParseLimits::none());
    }
}

/// The namespaces-in-scope limit counts the prefixes (and the default
/// namespace) bound at an element once all its declarations apply: an
/// undeclaration removes one, the `xml` prefix is not one.
#[test]
fn the_scope_limit_counts_active_bindings() {
    let ok = |xml: &str, max: usize| {
        assert_all_readers(
            xml.as_bytes(),
            &Outcome::Ok,
            &l().with_max_namespaces_in_scope(max),
        )
    };
    let refused = |xml: &str, max: usize| {
        assert_all_readers(
            xml.as_bytes(),
            &Outcome::Limit(ParseLimit::NamespacesInScope),
            &l().with_max_namespaces_in_scope(max),
        )
    };
    // The default namespace undeclared and a prefix declared, both orders.
    ok(
        "<r xmlns=\"urn:one\"><a xmlns=\"\" xmlns:p=\"urn:two\"/></r>",
        1,
    );
    ok(
        "<r xmlns=\"urn:one\"><a xmlns:p=\"urn:two\" xmlns=\"\"/></r>",
        1,
    );
    ok("<r xmlns=\"\"/>", 0);
    ok("<r xmlns:xml=\"http://www.w3.org/XML/1998/namespace\"/>", 0);
    // XML 1.1: a prefix undeclared and another declared, both orders.
    ok(
        &v11("<r xmlns:p=\"urn:p\"><a xmlns:p=\"\" xmlns:q=\"urn:q\"/></r>"),
        1,
    );
    ok(
        &v11("<r xmlns:p=\"urn:p\"><a xmlns:q=\"urn:q\" xmlns:p=\"\"/></r>"),
        1,
    );
    ok(&v11("<r xmlns:p=\"\"/>"), 0);
    // The undeclared binding comes back when the element closes.
    let restored =
        "<r xmlns=\"urn:one\"><a xmlns=\"\" xmlns:p=\"urn:two\"/><b xmlns:q=\"urn:q\"/></r>";
    refused(restored, 1);
    ok(restored, 2);
    let restored11 =
        v11("<r xmlns:p=\"urn:p\"><a xmlns:p=\"\" xmlns:q=\"urn:q\"/><b xmlns:s=\"urn:s\"/></r>");
    refused(&restored11, 1);
    ok(&restored11, 2);
    // A real excess, at exactly the limit + 1.
    let three = "<r xmlns=\"urn:one\" xmlns:p=\"urn:p\"><a xmlns:q=\"urn:q\"/></r>";
    refused(three, 2);
    ok(three, 3);
    let ss = schema_set(false);
    let validator = SchemaValidator::new(&ss, ValidationFlags::default());
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    let mut runtime = validator.start_run(CollectingValidationSink {
        errors: &mut errors,
        warnings: &mut warnings,
    });
    runtime.set_parse_limits(l().with_max_namespaces_in_scope(2));
    match drive_quick_xml(three.as_bytes(), &mut runtime, &ss) {
        Err(DriveError::LimitExceeded(e)) => {
            assert_eq!(
                (e.limit(), e.max(), e.found()),
                (ParseLimit::NamespacesInScope, 2, 3)
            );
            assert_eq!(e.position(), three.find("<a").unwrap() as u64);
        }
        other => panic!("{other:?}"),
    }
}

/// The namespaces-in-scope limit costs time linear in the tag: a tag with
/// many declarations under the limit is read about as fast as without a
/// limit, and one over it is refused at once.
#[test]
fn the_scope_limit_is_linear_in_the_tag() {
    let n = 20_000;
    let mut xml = String::from("<r");
    for i in 0..n {
        xml.push_str(&format!(" xmlns:p{i}=\"urn:{i}\""));
    }
    xml.push_str("/>");
    let time = |limits: ParseLimits| {
        let ss = schema_set(false);
        let validator = SchemaValidator::new(&ss, ValidationFlags::default());
        let (mut errors, mut warnings) = (Vec::new(), Vec::new());
        let mut runtime = validator.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        runtime.set_parse_limits(limits);
        let start = std::time::Instant::now();
        let r = drive_quick_xml_with(xml.as_bytes(), &mut runtime, &ss, &mut NoopHandler)
            .map(|_| ())
            .map_err(of_drive_with_error);
        (start.elapsed(), r)
    };
    let (free, r) = time(l());
    assert_eq!(r, Ok(()));
    let (at_limit, r) = time(l().with_max_namespaces_in_scope(n));
    assert_eq!(r, Ok(()));
    let (over, r) = time(l().with_max_namespaces_in_scope(1));
    assert_eq!(r, Err(Outcome::Limit(ParseLimit::NamespacesInScope)));
    // Quadratic work over 20,000 declarations (2 * 10^8 comparisons) takes
    // seconds; linear work takes about as long as the unlimited read.
    let bound = free * 5 + std::time::Duration::from_millis(250);
    assert!(
        at_limit < bound,
        "at the limit {at_limit:?}, unlimited {free:?}"
    );
    assert!(over < bound, "over the limit {over:?}, unlimited {free:?}");
}

/// The schema-document reader normalizes namespace declarations and
/// attribute values with the document's version, as the layer does: in XML
/// 1.1 a NEL in `xmlns:p` or `targetNamespace` is a space.
#[test]
fn the_schema_reader_normalizes_values_with_the_document_version() {
    let schema = |ns_decl: &str, tns: &str| {
        format!(
            "{V11}<xs:schema {XS} xmlns:p=\"{ns_decl}\" targetNamespace=\"{tns}\">\
             <xs:simpleType name=\"t\"><xs:restriction base=\"xs:string\"/></xs:simpleType>\
             <xs:element name=\"e\" type=\"p:t\"/></xs:schema>"
        )
    };
    for (ns_decl, tns) in [
        ("urn:a\u{85}b", "urn:a b"),
        ("urn:a b", "urn:a\u{85}b"),
        ("urn:a\r\u{85}b", "urn:a\u{2028}b"),
    ] {
        load_schema_text(&schema(ns_decl, tns))
            .unwrap_or_else(|e| panic!("{ns_decl:?} / {tns:?}: {e}"));
    }
    // XML 1.0: NEL is a character, so the names differ.
    let xsd = schema("urn:a\u{85}b", "urn:a b").replacen(V11, "", 1);
    assert!(
        load_schema_text(&xsd).is_err(),
        "XML 1.0 read NEL as a space"
    );
}
