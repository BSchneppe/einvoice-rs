use einvoice::{
    BuildError, Document, EngineFailure, InvoiceDraft, Limits, ParseError, ResourceLimit, Severity,
    SkipReason, Stage, StageStatus, Target, ValidationFailure, ValidationOutcome,
};

const PROFILE: &str = "urn:cen.eu:en16931:2017#compliant#urn:xeinkauf.de:kosit:xrechnung_3.0";
const FIXTURE: &[u8] =
    include_bytes!("../../einvoice-core/tests/inputs/cii/01.01a-INVOICE_uncefact.xml");

// Minimal XML sufficient for target preflight, deliberately NOT a valid invoice.
fn source(profile: &str) -> String {
    format!(
        r#"<rsm:CrossIndustryInvoice
 xmlns:rsm="urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100"
 xmlns:ram="urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100">
 <rsm:ExchangedDocumentContext><ram:GuidelineSpecifiedDocumentContextParameter>
 <ram:ID>{profile}</ram:ID>
 </ram:GuidelineSpecifiedDocumentContextParameter></rsm:ExchangedDocumentContext>
 </rsm:CrossIndustryInvoice>"#
    )
}

#[test]
fn immutable_import_serializes_exactly_without_normalizing_any_xml() {
    let original = b"\xef\xbb\xbf<?xml version='1.0'?>\r\n<!--keep--><?pi keep?><a z='1' a=\"2\">&#65;<![CDATA[<&>]]>\r\n</a>\r\n".to_vec();
    let mut caller_buffer = original.clone();
    let document = Document::parse(&caller_buffer).unwrap();
    caller_buffer.fill(b'x');
    assert_eq!(document.original_bytes(), original);
    let mut serialized = document.to_bytes();
    assert_eq!(serialized, original);
    serialized.fill(b'y');
    assert_eq!(document.clone().original_bytes(), original);
}

#[test]
fn imported_utf16_bytes_keep_their_encoding_and_bom() {
    let text = "<?xml version='1.0' encoding='UTF-16'?><a>Grüße</a>";
    for little_endian in [true, false] {
        let mut bytes = if little_endian {
            vec![0xff, 0xfe]
        } else {
            vec![0xfe, 0xff]
        };
        for unit in text.encode_utf16() {
            bytes.extend(if little_endian {
                unit.to_le_bytes()
            } else {
                unit.to_be_bytes()
            });
        }
        assert_eq!(Document::parse(&bytes).unwrap().to_bytes(), bytes);
    }
}

#[test]
fn parsing_checks_the_whole_xml_document_but_not_invoice_conformance() {
    assert!(Document::parse(b"<not-an-invoice unexpected='yes'/>").is_ok());
    for malformed in [
        "",
        "<a>",
        "<a/><b/>",
        "<a/>trailing",
        "<a x='1' x='2'/>",
        "<p:a/>",
        "<a>&unknown;</a>",
    ] {
        assert!(
            matches!(
                Document::parse(malformed),
                Err(ParseError::XmlSyntax { .. })
            ),
            "{malformed}"
        );
        let report = Target::xrechnung_3_0_2().validate(malformed);
        assert_eq!(report.outcome(), ValidationOutcome::InvalidDocument);
        assert!(!report.is_complete());
        assert_eq!(report.diagnostics()[0].rule_id, "API-XML-SYNTAX");
        assert_eq!(report.diagnostics()[0].severity, Severity::Error);
        assert!(report.diagnostics()[0].location.as_ref().unwrap().line >= 1);
        assert!(report.stages()[1..]
            .iter()
            .all(|s| s.status
                == StageStatus::Skipped(SkipReason::PreviousStageFailed(Stage::XmlSyntax))));
    }
}

#[test]
fn empty_findings_never_mean_valid_when_the_native_backend_is_unavailable() {
    for bytes in [FIXTURE, source(PROFILE).as_bytes()] {
        let target = Target::xrechnung_3_0_2();
        let document = Document::parse(bytes).unwrap();
        for report in [target.validate(bytes), document.validate(&target)] {
            assert!(report.diagnostics().is_empty());
            assert_eq!(report.outcome(), ValidationOutcome::EngineFailure);
            assert_eq!(
                report.failure(),
                Some(&ValidationFailure::Engine(
                    EngineFailure::BackendUnavailable
                ))
            );
            assert!(!report.is_valid());
            assert!(!report.is_complete());
            assert_eq!(
                report.stages().iter().map(|s| s.stage).collect::<Vec<_>>(),
                vec![
                    Stage::XmlSyntax,
                    Stage::TargetSelection,
                    Stage::Xsd,
                    Stage::CenRules,
                    Stage::XrechnungRules,
                    Stage::Acceptance,
                ]
            );
            assert!(report.stages()[..2]
                .iter()
                .all(|s| s.status == StageStatus::Completed));
            assert!(report.stages()[2..]
                .iter()
                .all(|s| s.status == StageStatus::Skipped(SkipReason::BackendUnavailable)));
        }
        assert_eq!(document.to_bytes(), bytes);
    }
}

#[test]
fn reports_distinguish_expected_bundle_from_executed_implementation() {
    let report = Target::xrechnung_3_0_2().validate(source(PROFILE));
    let bundle = report.bundle().unwrap();
    assert_eq!(
        report.requested_target(),
        "xrechnung-3.0.2-cii-d16b-kosit-2026-08-31"
    );
    assert_eq!(bundle.id, report.requested_target());
    assert_eq!(bundle.profile_version, "3.0.2");
    assert_eq!(bundle.syntax, "CII D16B");
    assert_eq!(bundle.kosit_configuration.version, "2026-08-31");
    assert_eq!(
        bundle.kosit_configuration.sha256,
        "2530cd107c414511c5d0462ec10f886910395abfca820db82e83d70bf01221a8"
    );
    assert_eq!(bundle.cen_rules.version, "1.3.16");
    assert_eq!(bundle.xrechnung_rules.version, "2.6.0");
    assert_eq!(
        report.implementation().facade,
        concat!("einvoice@", env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(
        report.implementation().xml_parser,
        "xml-rs@0.8.28+xmlparser@0.13.6"
    );
    assert_eq!(report.implementation().validator, None);
    assert_eq!(report.implementation().generator, None);
}

#[test]
fn unknown_exact_targets_never_fall_back_in_either_workflow() {
    let target = Target::named("xrechnung-3.0.2-cii-d16b-kosit-OLDER");
    // Target rejection takes precedence even over malformed input.
    let report = target.validate(b"<broken");
    assert_eq!(report.outcome(), ValidationOutcome::UnsupportedTarget);
    assert_eq!(
        report.failure(),
        Some(&ValidationFailure::UnsupportedTarget)
    );
    assert_eq!(report.requested_target(), target.id());
    assert!(report.bundle().is_none());
    assert!(report.diagnostics().is_empty());
    assert!(!report.is_valid());
    assert_eq!(
        report.stages()[0].status,
        StageStatus::Skipped(SkipReason::UnsupportedTarget)
    );
    let document = Document::parse(source(PROFILE)).unwrap();
    let parsed_report = document.validate(&target);
    assert_eq!(
        parsed_report.outcome(),
        ValidationOutcome::UnsupportedTarget
    );
    assert_eq!(parsed_report.stages()[0].status, StageStatus::Completed);
    assert!(!parsed_report.is_valid());
    assert!(
        matches!(target.encode(&InvoiceDraft::new("I-1", "EUR")), Err(BuildError::UnsupportedTarget { target: id }) if id == target.id())
    );
}

#[test]
fn wrong_profile_and_namespace_spoofing_cannot_select_a_weaker_validator() {
    let base = source(PROFILE);
    let cases = [
        source("urn:cen.eu:en16931:2017"),
        source(""),
        base.replace("CrossIndustryInvoice:100", "CrossIndustryInvoice:WRONG"),
        base.replace(
            "ReusableAggregateBusinessInformationEntity:100",
            "ReusableAggregateBusinessInformationEntity:WRONG",
        ),
        base.replace("<ram:ID>", "<ram:WrongPlace><ram:ID>")
            .replace("</ram:ID>", "</ram:ID></ram:WrongPlace>"),
        base.replace("</ram:ID>", &format!("</ram:ID><ram:ID>{PROFILE}</ram:ID>")),
        "<Invoice xmlns='urn:oasis:names:specification:ubl:schema:xsd:Invoice-2'/>".into(),
    ];
    let target = Target::xrechnung_3_0_2();
    for xml in cases {
        let report = Document::parse(&xml).unwrap().validate(&target);
        assert_eq!(
            report.outcome(),
            ValidationOutcome::InvalidDocument,
            "{xml}"
        );
        assert_eq!(report.failure(), Some(&ValidationFailure::Document));
        assert_eq!(report.requested_target(), target.id());
        assert!(report.diagnostics()[0].rule_id.starts_with("API-TARGET-"));
        assert_eq!(report.diagnostics()[0].stage, Stage::TargetSelection);
        assert!(!report.is_valid());
        assert!(report.stages()[2..].iter().all(|s| s.status
            == StageStatus::Skipped(SkipReason::PreviousStageFailed(Stage::TargetSelection))));
    }
    let renamed_prefix = base
        .replace("rsm:", "invoice:")
        .replace("xmlns:rsm", "xmlns:invoice");
    assert_eq!(
        target.validate(renamed_prefix).outcome(),
        ValidationOutcome::EngineFailure
    );
}

#[test]
fn unknown_structure_is_preserved_and_never_misrepresented_as_schema_valid() {
    let base = source(PROFILE);
    for xml in [
        base.replace(
            "<rsm:CrossIndustryInvoice",
            "<rsm:CrossIndustryInvoice unexpected='kept'",
        ),
        base.replace(
            "</rsm:CrossIndustryInvoice>",
            "<ram:Unknown>kept</ram:Unknown></rsm:CrossIndustryInvoice>",
        ),
        base.replace(
            "</rsm:CrossIndustryInvoice>",
            "<ram:Amount>not-a-decimal</ram:Amount></rsm:CrossIndustryInvoice>",
        ),
    ] {
        let document = Document::parse(&xml).unwrap();
        assert_eq!(document.original_bytes(), xml.as_bytes());
        let report = document.validate(&Target::xrechnung_3_0_2());
        assert_eq!(
            report.stages()[2].status,
            StageStatus::Skipped(SkipReason::BackendUnavailable)
        );
        assert!(!report.is_valid());
    }
}

#[test]
fn byte_and_depth_limits_are_not_document_faults() {
    for (xml, limits, expected) in [
        (
            "<a/>",
            Limits {
                max_bytes: 3,
                max_depth: 10,
            },
            ResourceLimit::InputBytes {
                limit: 3,
                actual: 4,
            },
        ),
        (
            "<a><b/></a>",
            Limits {
                max_bytes: 100,
                max_depth: 1,
            },
            ResourceLimit::Depth { limit: 1 },
        ),
    ] {
        assert_eq!(
            Document::parse_with_limits(xml, limits).unwrap_err(),
            ParseError::ResourceLimit(expected.clone())
        );
        let report = Target::xrechnung_3_0_2().validate_with_limits(xml, limits);
        assert_eq!(report.outcome(), ValidationOutcome::ResourceLimit);
        assert_eq!(
            report.failure(),
            Some(&ValidationFailure::ResourceLimit(expected))
        );
        assert!(report.diagnostics().is_empty());
        assert!(!report.is_valid());
    }
    assert!(Document::parse_with_limits(
        "<a/>",
        Limits {
            max_bytes: 4,
            max_depth: 1
        }
    )
    .is_ok());
}

#[test]
fn parser_token_budgets_remain_resource_failures() {
    let too_long_name = format!("<{} />", "a".repeat(8192));
    let too_many_attributes = format!(
        "<a {}/>",
        (0..1025)
            .map(|i| format!("a{i}='x'"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    for xml in [too_long_name, too_many_attributes] {
        let result = Document::parse(&xml);
        assert!(
            matches!(
                &result,
                Err(ParseError::ResourceLimit(ResourceLimit::Parser { .. }))
            ),
            "{result:?}"
        );
        let report = Target::xrechnung_3_0_2().validate(&xml);
        assert_eq!(report.outcome(), ValidationOutcome::ResourceLimit);
        assert!(report.diagnostics().is_empty());
        assert!(!report.is_valid());
    }
}

#[test]
fn new_drafts_never_return_unvalidated_xml_or_edit_an_import() {
    let draft = InvoiceDraft::new("INV-2026-001", "EUR");
    assert_eq!(draft.invoice_number(), "INV-2026-001");
    assert_eq!(draft.currency(), "EUR");
    let target = Target::xrechnung_3_0_2();
    assert!(
        matches!(target.encode(&draft), Err(BuildError::Unavailable { target: id }) if id == target.id())
    );
    let original = Document::parse(FIXTURE).unwrap();
    assert_eq!(original.to_bytes(), FIXTURE);
}

#[test]
fn unsupported_encoding_is_an_engine_capability_failure_not_a_document_fault() {
    let xml = b"<?xml version='1.0' encoding='Shift_JIS'?><a/>";
    assert!(matches!(
        Document::parse(xml),
        Err(ParseError::UnsupportedEncoding { .. })
    ));
    let report = Target::xrechnung_3_0_2().validate(xml);
    assert_eq!(report.outcome(), ValidationOutcome::EngineFailure);
    assert!(matches!(
        report.failure(),
        Some(ValidationFailure::Engine(
            EngineFailure::UnsupportedEncoding { .. }
        ))
    ));
    assert!(report.diagnostics().is_empty());
    assert!(!report.is_valid());
}

#[test]
fn dtds_are_stopped_before_parameter_or_external_entity_processing() {
    // Deliberately modest chain: rejection must occur at DOCTYPE, before expansion.
    let mut chain = String::from("<!DOCTYPE a [<!ENTITY % a '0123456789'>");
    for (previous, name) in [('a', 'b'), ('b', 'c'), ('c', 'd')] {
        chain += &format!(
            "<!ENTITY % {name} '{}'>",
            format!("%{previous};").repeat(10)
        );
    }
    chain += "]><a/>";
    for dtd in [
        "<!DOCTYPE><a/>",
        "<!DOCTYPE a><a/>",
        "<!DOCTYPE a SYSTEM 'file:///must-not-read'><a/>",
        "<!DOCTYPE a [<!ENTITY x SYSTEM 'https://must-not-fetch.invalid/'>]><a>&x;</a>",
        &chain,
    ] {
        for prefix in [
            "",
            "<?xml version='1.0'?>\r\n<!-- <!DOCTYPE ignored --><?pi keep?>\n",
        ] {
            let xml = format!("{prefix}{dtd}");
            let mut variants = vec![xml.as_bytes().to_vec()];
            for little_endian in [true, false] {
                let mut bytes = if little_endian {
                    vec![0xff, 0xfe]
                } else {
                    vec![0xfe, 0xff]
                };
                for unit in xml.encode_utf16() {
                    bytes.extend(if little_endian {
                        unit.to_le_bytes()
                    } else {
                        unit.to_be_bytes()
                    });
                }
                variants.push(bytes);
            }
            for bytes in variants {
                assert_eq!(
                    Document::parse(&bytes).unwrap_err(),
                    ParseError::UnsupportedDtd
                );
                let report = Target::xrechnung_3_0_2().validate(&bytes);
                assert_eq!(report.outcome(), ValidationOutcome::EngineFailure);
                assert_eq!(
                    report.failure(),
                    Some(&ValidationFailure::Engine(EngineFailure::UnsupportedDtd))
                );
                assert!(report.diagnostics().is_empty());
                assert!(!report.is_complete());
                assert!(!report.is_valid());
            }
        }
    }
}

#[test]
fn dtd_markers_inside_comments_instructions_and_cdata_do_not_block_imports() {
    let text = "<?xml\nversion='1.0'?>\n<!-- <!DOCTYPE a> --><?pi <!DOCTYPE a> ?><a><![CDATA[<!DOCTYPE a>]]></a>";
    assert_eq!(Document::parse(text).unwrap().to_bytes(), text.as_bytes());
    for little_endian in [true, false] {
        let mut bytes = if little_endian {
            vec![0xff, 0xfe]
        } else {
            vec![0xfe, 0xff]
        };
        for unit in text.encode_utf16() {
            bytes.extend(if little_endian {
                unit.to_le_bytes()
            } else {
                unit.to_be_bytes()
            });
        }
        assert_eq!(Document::parse(&bytes).unwrap().to_bytes(), bytes);
    }
    let latin1 = b"<?xml version='1.0' encoding='ISO-8859-1'?><a>caf\xe9</a>";
    assert_eq!(Document::parse(latin1).unwrap().to_bytes(), latin1);
}

#[test]
fn attribute_uniqueness_uses_expanded_names_not_namespace_prefixes() {
    let duplicate = "<a xmlns:p='urn:test' xmlns:q='urn:test' p:x='1' q:x='2'/>";
    assert!(matches!(
        Document::parse(duplicate),
        Err(ParseError::XmlSyntax { .. })
    ));
    let report = Target::xrechnung_3_0_2().validate(duplicate);
    assert_eq!(report.outcome(), ValidationOutcome::InvalidDocument);
    assert_eq!(report.diagnostics()[0].rule_id, "API-XML-SYNTAX");
    assert_eq!(
        report.stages()[0].status,
        StageStatus::Failed(ValidationFailure::Document)
    );
    for distinct in [
        "<a xmlns:p='urn:one' xmlns:q='urn:two' p:x='1' q:x='2'/>",
        "<a xmlns='urn:test' xmlns:p='urn:test' x='1' p:x='2'/>",
    ] {
        assert_eq!(
            Document::parse(distinct).unwrap().to_bytes(),
            distinct.as_bytes()
        );
    }
}

#[test]
fn duplicate_namespace_declarations_are_syntax_faults_in_all_supported_encodings() {
    for xml in [
        "<a xmlns:p='u' xmlns:p='v'/>",
        "<a xmlns='u' xmlns='v'/>",
        "<a xmlns:p='u' xmlns:p='u'/>",
        "<?xml\nversion='1.0'?><a xmlns='u' xmlns='u'/>",
    ] {
        let mut variants = vec![xml.as_bytes().to_vec()];
        for little_endian in [true, false] {
            let mut bytes = if little_endian {
                vec![0xff, 0xfe]
            } else {
                vec![0xfe, 0xff]
            };
            for unit in xml.encode_utf16() {
                bytes.extend(if little_endian {
                    unit.to_le_bytes()
                } else {
                    unit.to_be_bytes()
                });
            }
            variants.push(bytes);
        }
        for bytes in variants {
            assert!(matches!(
                Document::parse(&bytes),
                Err(ParseError::XmlSyntax { .. })
            ));
            let report = Target::xrechnung_3_0_2().validate(&bytes);
            assert_eq!(report.outcome(), ValidationOutcome::InvalidDocument);
            assert_eq!(
                report.stages()[0].status,
                StageStatus::Failed(ValidationFailure::Document)
            );
        }
    }
    let valid = "<a xmlns:p='u'><b xmlns:p='v'/><c xmlns:p='u'/></a>";
    assert_eq!(Document::parse(valid).unwrap().to_bytes(), valid.as_bytes());
}

#[test]
fn namespace_fault_locations_handle_xml_line_endings_and_boms() {
    for (separator, line, column) in [(" ", 1, 16), ("\r", 2, 1), ("\n", 2, 1), ("\r\n", 2, 1)] {
        let text = format!("<a xmlns:p='u'{separator}xmlns:p='v'/>");
        let mut variants = vec![
            text.as_bytes().to_vec(),
            format!("\u{feff}{text}").into_bytes(),
        ];
        for little_endian in [true, false] {
            let mut bytes = if little_endian {
                vec![0xff, 0xfe]
            } else {
                vec![0xfe, 0xff]
            };
            for unit in text.encode_utf16() {
                bytes.extend(if little_endian {
                    unit.to_le_bytes()
                } else {
                    unit.to_be_bytes()
                });
            }
            variants.push(bytes);
        }
        for bytes in variants {
            let report = Target::xrechnung_3_0_2().validate(&bytes);
            assert_eq!(
                report.diagnostics()[0].location,
                Some(einvoice::SourceLocation { line, column })
            );
        }
    }
}
