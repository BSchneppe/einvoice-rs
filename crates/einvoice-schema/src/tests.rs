use super::*;
use std::{
    num::NonZeroUsize,
    sync::{atomic::AtomicBool, Arc, Barrier},
    time::{Duration, Instant},
};

const XSD: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
 <xs:simpleType name="Positive"><xs:restriction base="xs:decimal"><xs:minInclusive value="0"/></xs:restriction></xs:simpleType>
 <xs:complexType name="Body"><xs:sequence><xs:element name="d" type="Positive" minOccurs="2" maxOccurs="2"/><xs:element name="z" type="xs:string"/></xs:sequence><xs:attribute name="req" type="xs:int" use="required"/><xs:attribute name="fixed" type="xs:string" fixed="ok"/><xs:attribute name="no" use="prohibited"/></xs:complexType>
 <xs:element name="r" type="Body"/>
 <xs:element name="n" type="xs:int" nillable="true"/>
 <xs:element name="ref" type="xs:IDREF"/>
 <xs:element name="refs"><xs:complexType><xs:sequence><xs:element name="i" type="xs:IDREF" maxOccurs="unbounded"/></xs:sequence><xs:attribute name="ref" type="xs:IDREF"/></xs:complexType></xs:element>
 <xs:element name="decimals"><xs:complexType><xs:sequence><xs:element name="d" type="xs:decimal" maxOccurs="unbounded"/></xs:sequence></xs:complexType></xs:element>
</xs:schema>"#;
const VALID: &str = r#"<r req="1"><d>1</d><d>2</d><z/></r>"#;
fn limits() -> RequestLimits {
    RequestLimits {
        input_bytes: 1_000_000,
        nodes: 10_000,
        depth: 128,
        attributes_per_element: 100,
        declarations_per_element: 100,
        namespaces_per_element: 100,
        name_bytes: 1000,
        uri_bytes: 4000,
        value_bytes: 100_000,
        temporary_bytes: 100_000,
        work: 10_000_000,
        diagnostic_count: 100,
        diagnostic_bytes: 1_000_000,
        terminal_bytes: 100_000,
    }
}
fn authored<T>(
    s: &str,
    n: usize,
    f: impl FnOnce(&mut Epoch<'_>) -> T,
) -> Result<T, InitializationError> {
    let schema = bundle::authored(s)?;
    prepared(&schema, NonZeroUsize::new(n).unwrap(), f)
}
fn observe(xml: &str) -> Observation {
    let source = SourceDocument::parse(xml.as_bytes()).unwrap();
    authored(XSD, 1, |e| {
        e.validate(&source, &limits(), RequestControl::default())
            .unwrap()
    })
    .unwrap()
}
fn diagnostic(o: &Observation, i: usize) -> &Diagnostic {
    &o.events[i].diagnostic
}
fn slice(xml: &str, span: Span) -> &str {
    let Span::Utf8Bytes { start, end } = span else {
        panic!("UTF8")
    };
    &xml[start..end]
}
#[test]
fn duplicate_occurrences_and_full_nested_value_payload() {
    let xml = r#"<r req="1"><d>bad</d><d>bad</d><z/></r>"#;
    let o = observe(xml);
    assert_eq!(o.assessment, Assessment::Invalid);
    assert!(o.finalization_succeeded);
    assert_eq!(o.events.len(), 2);
    for (i, event) in o.events.iter().enumerate() {
        assert_eq!(event.ordinal, i);
        assert_eq!(event.severity, Severity::Error);
        let d = &event.diagnostic;
        assert_eq!(d.raw_location, None);
        assert_eq!(d.element_path.as_deref(), Some("/r/d"));
        assert!(
            matches!(&d.path.as_ref().unwrap()[1],PathStep::Element{position,..} if *position==i+1)
        );
        assert_eq!(slice(xml, d.source.as_ref().unwrap().subject), "<d>bad</d>");
        let evidence = d.evidence.as_ref().unwrap();
        assert!(
            matches!(&evidence.cause,Some(NativeCause::Value(ValueCause::InvalidLexical{value,type_name,message})) if value=="bad" && *type_name=="decimal" && !message.is_empty())
        );
        assert_eq!(
            evidence
                .governing_type
                .as_ref()
                .unwrap()
                .name
                .as_ref()
                .unwrap()
                .local,
            "Positive"
        );
        assert!(evidence.actual_type.is_some());
        assert!(evidence.facet_origin.is_none());
        assert!(d.triggering_occurrence.is_none());
        assert!(d.causal_parent.is_none());
    }
    assert_ne!(diagnostic(&o, 0).source, diagnostic(&o, 1).source);
}
#[test]
fn facet_payload_and_schema_origin_stay_separate() {
    let o = observe(r#"<r req="1"><d>-1</d><d>2</d><z/></r>"#);
    let e = diagnostic(&o, 0).evidence.as_ref().unwrap();
    assert!(
        matches!(&e.cause,Some(NativeCause::Value(ValueCause::FacetViolation(FacetCause::MinInclusiveViolation{value,min}))) if value=="-1"&&min=="0")
    );
    let c = e.governing_type.as_ref().unwrap();
    assert_eq!(c.document_uri.as_deref(), Some("authored.xsd"));
    assert!(c.schema_span.is_some());
    assert_eq!(e.facet_origin, None);
    let json = serde_json::to_value(&o).unwrap();
    assert_eq!(
        json["events"][0]["evidence"]["cause"]["Value"]["FacetViolation"]["MinInclusiveViolation"]
            ["min"],
        "0"
    );
}
#[test]
fn attributes_owner_and_rejected_child() {
    let xml = r#"<r req="bad" extra="x"><d>1</d><d>2</d><z/><extra/></r>"#;
    let o = observe(xml);
    assert_eq!(o.assessment, Assessment::Invalid);
    assert!(o
        .events
        .iter()
        .any(|e| e
            .diagnostic
            .source
            .as_ref()
            .is_some_and(|s| slice(xml, s.subject) == r#"req="bad""#
                && slice(xml, s.attribute_value.unwrap()) == "bad"
                && slice(xml, s.owner) == xml)));
    let rejected = o
        .events
        .iter()
        .find(|e| {
            matches!(
                e.diagnostic
                    .evidence
                    .as_ref()
                    .and_then(|e| e.cause.as_ref()),
                Some(NativeCause::UnexpectedChild { .. })
            )
        })
        .unwrap();
    assert_eq!(
        slice(xml, rejected.diagnostic.source.as_ref().unwrap().subject),
        xml
    );
    assert!(matches!(&rejected.diagnostic.subject,Some(Subject::Element(n)) if n.local=="r"));
    let missing = observe("<r><d>1</d></r>");
    assert!(missing.events.iter().any(|e|matches!(e.diagnostic.evidence.as_ref().and_then(|e|e.cause.as_ref()),Some(NativeCause::MissingRequiredAttribute{name}) if name.local=="req")&&e.diagnostic.source.as_ref().unwrap().attribute_value.is_none()));
    for xml in [
        r#"<r req="1" no="x"><d>1</d><d>2</d><z/></r>"#,
        r#"<r req="1" fixed="bad"><d>1</d><d>2</d><z/></r>"#,
    ] {
        assert_eq!(observe(xml).assessment, Assessment::Invalid);
    }
}
#[test]
fn deferred_occurrence_and_attribute_joins() {
    let xml = r#"<refs ref="owner-missing"><i>a</i><i>b</i></refs>"#;
    let o = observe(xml);
    assert_eq!(o.assessment, Assessment::Invalid);
    assert_eq!(o.root_validity, Some(RootValidity::Invalid));
    assert!(o.finalization_succeeded);
    assert_eq!(o.events.len(), 3);
    let slices: Vec<_> = o
        .events
        .iter()
        .map(|e| slice(xml, e.diagnostic.source.as_ref().unwrap().subject))
        .collect();
    assert!(slices.contains(&r#"ref="owner-missing""#));
    assert!(slices.contains(&"<i>a</i>"));
    assert!(slices.contains(&"<i>b</i>"));
}
#[test]
fn mixed_callbacks_preserve_real_order_and_duplicate_warnings() {
    let huge = "10000000000000000000000000000000000000000";
    let xml = format!("<decimals><d>bad</d><d>{huge}</d><d>bad</d><d>{huge}</d></decimals>");
    let o = observe(&xml);
    assert_eq!(o.assessment, Assessment::Invalid);
    assert_eq!(
        o.events.iter().map(|e| e.severity).collect::<Vec<_>>(),
        [
            Severity::Error,
            Severity::Warning,
            Severity::Error,
            Severity::Warning
        ]
    );
    for (i, e) in o.events.iter().enumerate() {
        assert_eq!(e.ordinal, i);
    }
    assert!(diagnostic(&o, 1).evidence.is_none());
    assert_eq!(diagnostic(&o, 1).message, diagnostic(&o, 3).message);
}
#[test]
fn namespace_aliases_shadowing_undeclaration_and_xsi() {
    let s = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:t="urn:t" targetNamespace="urn:t" elementFormDefault="qualified"><xs:complexType name="Body"><xs:sequence><xs:element name="v" type="xs:int"/><xs:element name="plain" form="unqualified" type="xs:string"/></xs:sequence></xs:complexType><xs:element name="r" type="t:Body"/></xs:schema>"#;
    for xml in [
        r#"<r xmlns="urn:t"><p:v xmlns:p="urn:t">1</p:v><plain xmlns=""/></r>"#,
        r#"<a:r xmlns:a="urn:t" xmlns:b="urn:other"><b:v xmlns:b="urn:t">1</b:v><plain/></a:r>"#,
        r#"<a:r xmlns:a="urn:t" xmlns:alias="urn:t" xmlns:i="http://www.w3.org/2001/XMLSchema-instance" i:type="alias:Body"><a:v>1</a:v><plain/></a:r>"#,
    ] {
        let doc = SourceDocument::parse(xml.as_bytes()).unwrap();
        let o = authored(s, 1, |e| {
            e.validate(&doc, &limits(), RequestControl::default())
                .unwrap()
        })
        .unwrap();
        assert_eq!(o.assessment, Assessment::Valid, "{o:?}");
    }
    for (index, xml) in [
        r#"<n xmlns:i="http://www.w3.org/2001/XMLSchema-instance" i:nil="true"/>"#,
        r#"<n xmlns:i="http://www.w3.org/2001/XMLSchema-instance" i:type="bad">1</n>"#,
        r#"<n xmlns:i="http://www.w3.org/2001/XMLSchema-instance" i:nil="true">1</n>"#,
        r#"<n xmlns:i="http://www.w3.org/2001/XMLSchema-instance" i:nil="bad"/>"#,
    ]
    .into_iter()
    .enumerate()
    {
        let o = observe(xml);
        assert!(o.finalization_succeeded);
        assert_eq!(
            o.assessment,
            if index == 0 {
                Assessment::Valid
            } else {
                Assessment::Invalid
            }
        );
        if index != 0 {
            assert!(!o.events.is_empty());
        }
    }
}
#[test]
fn xml_whitespace_comments_pi_and_split_cdata() {
    assert_eq!(
        observe("<r req='1'> \t\r\n<d>1<![CDATA[.]]>0</d><!--c--><d>2</d><?p q?><z/></r>")
            .assessment,
        Assessment::Valid
    );
    let invalid = observe("<r req='1'>\u{a0}<d>1</d><d>2</d><z/></r>");
    assert_eq!(invalid.assessment, Assessment::Invalid);
    assert!(invalid.failure.is_none());
    assert!(invalid.finalization_succeeded);
    assert_eq!(diagnostic(&invalid, 0).code, "cvc-complex-type.2.3");
}
#[test]
fn unicode_character_content_completes_with_native_findings_and_reuse() {
    let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
      <xs:element name="eo" nillable="true"><xs:complexType><xs:sequence><xs:element name="c" type="xs:string" minOccurs="0"/></xs:sequence></xs:complexType></xs:element>
      <xs:element name="mixed"><xs:complexType mixed="true"/></xs:element>
      <xs:element name="s" type="xs:string"/>
      <xs:element name="empty"><xs:complexType/></xs:element>
    </xs:schema>"#;
    let cases = [
        (
            "<eo>&#xA0;<!--c--><![CDATA[\u{85}]]></eo>",
            Some("cvc-complex-type.2.3"),
        ),
        ("<mixed>&#xA0;<?p d?><![CDATA[\u{85}]]></mixed>", None),
        ("<s>&#xA0;<!--c-->\u{3000}</s>", None),
        ("<empty> </empty>", Some("cvc-complex-type.2.1")),
        (
            "<eo xmlns:xsi='http://www.w3.org/2001/XMLSchema-instance' xsi:nil='true'> </eo>",
            Some("cvc-elt.3.2.1"),
        ),
        ("<eo> \t&#xD;\n<c/></eo>", None),
    ];
    authored(xsd, cases.len() * 2, |epoch| {
        for (xml, code) in cases {
            for utf16 in [false, true] {
                let bytes: Vec<u8> = if utf16 {
                    [0xff, 0xfe]
                        .into_iter()
                        .chain(xml.encode_utf16().flat_map(u16::to_le_bytes))
                        .collect()
                } else {
                    xml.as_bytes().to_vec()
                };
                let source = SourceDocument::parse(&bytes).unwrap();
                let observation = epoch
                    .validate(&source, &limits(), RequestControl::default())
                    .unwrap();
                assert_eq!(
                    observation.assessment,
                    if code.is_some() {
                        Assessment::Invalid
                    } else {
                        Assessment::Valid
                    }
                );
                assert!(observation.finalization_succeeded);
                assert!(observation.failure.is_none());
                assert_eq!(source.original_bytes(), bytes);
                if let Some(code) = code {
                    let event = observation
                        .events
                        .iter()
                        .find(|e| e.diagnostic.code == code)
                        .unwrap();
                    let joined = event.diagnostic.source.as_ref().unwrap();
                    assert_eq!(joined.subject, observation.validated_root);
                } else {
                    assert!(observation.events.is_empty());
                }
            }
        }
    })
    .unwrap();
}
#[test]
fn original_utf8_bom_and_utf16_scalar_spans() {
    let xml = "\u{feff}<r req='1'><d>é</d><d>2</d><z/></r>";
    let o = observe(xml);
    assert_eq!(
        o.validated_root,
        Span::Utf8Bytes {
            start: 3,
            end: xml.len()
        }
    );
    assert_eq!(o.bom_bytes, 3);
    assert_eq!(
        slice(xml, diagnostic(&o, 0).source.as_ref().unwrap().subject),
        "<d>é</d>"
    );
    let xml = "<r req='1'>\r\n<d>𝄞</d><d>2</d><z/></r>";
    let bytes: Vec<u8> = [0xff, 0xfe]
        .into_iter()
        .chain(xml.encode_utf16().flat_map(u16::to_le_bytes))
        .collect();
    let doc = SourceDocument::parse(&bytes).unwrap();
    let o = authored(XSD, 1, |e| {
        e.validate(&doc, &limits(), RequestControl::default())
            .unwrap()
    })
    .unwrap();
    assert_eq!(o.input_sha256, Some(sha256(&bytes)));
    assert_eq!(
        o.validated_root,
        Span::UnicodeScalars {
            start: 0,
            end: xml.chars().count()
        }
    );
    let start = xml.chars().position(|c| c == '\n').unwrap() + 1;
    assert_eq!(
        diagnostic(&o, 0).source.as_ref().unwrap().subject,
        Span::UnicodeScalars {
            start,
            end: start + 8
        }
    );
}
#[test]
fn no_instance_hint_loading_or_xml_fallback() {
    let o = observe(
        r#"<r xmlns:i="http://www.w3.org/2001/XMLSchema-instance" i:schemaLocation="urn:t https://invalid.example/unavailable.xsd" req="1"><d>1</d><d>2</d><z/></r>"#,
    );
    assert_eq!(o.assessment, Assessment::Valid);
    let o = observe("<r xml:lang='en' req='1'><d>1</d><d>2</d><z/></r>");
    assert_eq!(o.assessment, Assessment::Invalid);
}
#[test]
fn resource_refusal_retains_prefix_and_retires() {
    let source = SourceDocument::parse(b"<refs><i>a</i><i>b</i><i>c</i></refs>").unwrap();
    let mut l = limits();
    l.diagnostic_count = 1;
    authored(XSD, 2, |e| {
        let o = e.validate(&source, &l, RequestControl::default()).unwrap();
        assert_eq!(o.assessment, Assessment::Incomplete);
        assert_eq!(o.events.len(), 1);
        assert_eq!(o.observed_callbacks, 2);
        assert!(o.finalization_attempted);
        assert!(!o.finalization_succeeded);
        assert_eq!(
            o.failure,
            Some(Failure::Limit {
                dimension: "diagnostic_count",
                limit: 1
            })
        );
        assert!(o.operational_failure.is_some());
        assert!(o.finalization_error.is_some());
        assert!(e.is_retired());
        assert!(matches!(
            e.validate(&source, &limits(), RequestControl::default()),
            Err(Failure::EpochRetired)
        ));
    })
    .unwrap();
}
#[test]
fn byte_work_inventory_and_configuration_limits_are_processing_failures() {
    let source = SourceDocument::parse(VALID.as_bytes()).unwrap();
    for dimension in [
        "input",
        "nodes",
        "depth",
        "attribute",
        "name",
        "uri",
        "value",
        "namespace",
        "declaration",
        "temporary",
        "work",
        "diagnostic",
        "invalid",
    ] {
        let mut l = limits();
        let xml = match dimension {
            "input" => {
                l.input_bytes = 1;
                VALID
            }
            "nodes" => {
                l.nodes = 1;
                VALID
            }
            "depth" => {
                l.depth = 1;
                VALID
            }
            "attribute" => {
                l.attributes_per_element = 1;
                "<r req='1' extra='x'/>"
            }
            "name" => {
                l.name_bytes = 1;
                "<long/>"
            }
            "uri" => {
                l.uri_bytes = 1;
                "<r xmlns='urn:long'/>"
            }
            "value" => {
                l.value_bytes = 1;
                "<n>123</n>"
            }
            "namespace" => {
                l.namespaces_per_element = 1;
                "<r xmlns:p='urn:p'/>"
            }
            "declaration" => {
                l.declarations_per_element = 1;
                "<r xmlns:p='p' xmlns:q='q'/>"
            }
            "temporary" => {
                l.temporary_bytes = 1;
                VALID
            }
            "work" => {
                l.work = 1;
                VALID
            }
            "diagnostic" => {
                l.diagnostic_bytes = 1;
                "<other/>"
            }
            "invalid" => {
                l.depth = 0;
                VALID
            }
            _ => unreachable!(),
        };
        let doc = SourceDocument::parse(xml.as_bytes()).unwrap();
        authored(XSD, 1, |e| {
            let o = e.validate(&doc, &l, RequestControl::default()).unwrap();
            assert_eq!(o.assessment, Assessment::Incomplete, "{dimension}: {o:?}");
            assert!(o.failure.is_some());
            assert!(e.is_retired());
        })
        .unwrap();
    }
    // Preflight refuses BEFORE any request names can be interned.
    let schema = bundle::authored(XSD).unwrap();
    let count = schema.name_table.len();
    let mut l = limits();
    l.depth = 1;
    prepared(&schema, NonZeroUsize::new(1).unwrap(), |e| {
        let o = e.validate(&source, &l, RequestControl::default()).unwrap();
        assert!(!o.finalization_attempted);
    })
    .unwrap();
    assert_eq!(schema.name_table.len(), count);
}
#[test]
fn cancellation_deadline_healthy_reuse_and_finite_allowance() {
    let source = SourceDocument::parse(VALID.as_bytes()).unwrap();
    let cancel = AtomicBool::new(true);
    for control in [
        RequestControl {
            cancellation: Some(&cancel),
            deadline: None,
        },
        RequestControl {
            cancellation: None,
            deadline: Some(Instant::now() - Duration::from_secs(1)),
        },
    ] {
        authored(XSD, 2, |e| {
            let o = e.validate(&source, &limits(), control).unwrap();
            assert_eq!(o.assessment, Assessment::Incomplete);
            assert!(o.events.is_empty());
            assert!(!o.finalization_attempted);
            assert!(e.is_retired());
        })
        .unwrap();
    }
    authored(XSD, 3, |e| {
        for xml in [VALID, "<other/>", VALID] {
            let doc = SourceDocument::parse(xml.as_bytes()).unwrap();
            let o = e
                .validate(&doc, &limits(), RequestControl::default())
                .unwrap();
            assert_ne!(o.assessment, Assessment::Incomplete, "{o:?}");
        }
        assert!(e.is_retired());
        assert_eq!(e.remaining_requests(), 0);
        assert!(matches!(
            e.validate(&source, &limits(), RequestControl::default()),
            Err(Failure::EpochRetired)
        ));
    })
    .unwrap();
}
#[test]
fn independent_concurrent_epochs_share_only_immutable_source() {
    let source = SourceDocument::parse(VALID.as_bytes()).unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let handles: Vec<_> = (0..3)
        .map(|_| {
            let source = source.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                authored(XSD, 8, |e| {
                    barrier.wait();
                    for _ in 0..8 {
                        assert_eq!(
                            e.validate(&source, &limits(), RequestControl::default())
                                .unwrap()
                                .assessment,
                            Assessment::Valid
                        );
                    }
                })
                .unwrap()
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
}
#[test]
fn schema_errors_directives_dtd_and_deferred_components_refuse_availability() {
    for schema in [
        "<not-schema/>",
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"><xs:element name="r" type="missing"/></xs:schema>"#,
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"><xs:simpleType name="L"><xs:list itemType="missing"/></xs:simpleType></xs:schema>"#,
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"><xs:import namespace="urn:x" schemaLocation="https://invalid.example/x"/></xs:schema>"#,
        r#"<!DOCTYPE xs:schema><xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"/>"#,
    ] {
        assert!(authored(schema, 1, |_| ()).is_err(), "{schema}");
    }
}
#[test]
fn checked_size_arithmetic() {
    let mut size = budget::Size::new(usize::MAX);
    size.add(usize::MAX).unwrap();
    assert_eq!(size.add(1), Err(Failure::ArithmeticOverflow));
}
#[test]
fn unpreparable_content_model_is_not_an_invoice_rejection() {
    let schema = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"><xs:element name="r"><xs:complexType><xs:sequence><xs:sequence minOccurs="0" maxOccurs="1000000"><xs:sequence minOccurs="0" maxOccurs="1000000"><xs:element name="a" type="xs:string" minOccurs="0"/></xs:sequence></xs:sequence></xs:sequence></xs:complexType></xs:element></xs:schema>"#;
    let error = authored(schema, 1, |_| ()).unwrap_err();
    assert_eq!(error.phase, "content model preparation");
}
#[test]
fn finalization_error_without_operational_failure_remains_distinct() {
    let schema = bundle::authored(XSD).unwrap();
    let validator = xsd_schema::validation::SchemaValidator::new(
        &schema,
        xsd_schema::validation::ValidationFlags::default(),
    );
    let source = SourceDocument::parse(b"<ref/>").unwrap();
    let l = limits();
    let c = budget::Control {
        source: &source,
        limits: &l,
        signals: RequestControl::default(),
        state: Default::default(),
    };
    let mut runtime = validator.start_run(capture::Sink(&c));
    runtime.validate_element_novalue("ref", "", None, None, &Default::default());
    let error = runtime.end_validation().unwrap_err();
    assert!(runtime.operational_failure().is_none());
    let diagnostic = capture::terminal(&error, &c, &mut l.terminal_bytes.clone()).unwrap();
    assert_eq!(diagnostic.code, error.constraint);
    assert_eq!(diagnostic.message, error.message);
}
#[test]
fn midwalk_stop_and_terminal_capture_exhaustion_do_not_finalize_or_reuse() {
    let source =
        SourceDocument::parse(b"<decimals><d>bad</d><d>bad</d><d>bad</d></decimals>").unwrap();
    let mut l = limits();
    l.diagnostic_count = 1;
    l.terminal_bytes = 1;
    authored(XSD, 2, |e| {
        let o = e.validate(&source, &l, RequestControl::default()).unwrap();
        assert_eq!(o.assessment, Assessment::Incomplete);
        assert_eq!(o.events.len(), 1);
        assert!(!o.finalization_attempted);
        assert!(o.terminal_capture_failure.is_some());
        assert_eq!(
            o.failure,
            Some(Failure::Limit {
                dimension: "diagnostic_count",
                limit: 1
            })
        );
        assert!(e.is_retired());
    })
    .unwrap();
}
#[test]
fn latin1_ascii_original_scalar_attribute_witnesses() {
    for (label, value) in [("ISO-8859-1", vec![0xe9]), ("US-ASCII", vec![b'x'])] {
        let mut bytes = format!("<?xml version='1.0' encoding='{label}'?><r req='").into_bytes();
        bytes.extend(value);
        bytes.extend_from_slice(b"'><d>1</d><d>2</d><z/></r>");
        let source = SourceDocument::parse(&bytes).unwrap();
        let o = authored(XSD, 1, |e| {
            e.validate(&source, &limits(), RequestControl::default())
                .unwrap()
        })
        .unwrap();
        assert_eq!(o.assessment, Assessment::Invalid);
        assert!(matches!(
            diagnostic(&o, 0).source.as_ref().unwrap().attribute_value,
            Some(Span::UnicodeScalars { .. })
        ));
    }
}
#[test]
fn backend_execution_resource_failure_is_out_of_band_and_terminal() {
    let schema = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"><xs:element name="r"><xs:complexType><xs:sequence><xs:element name="b" type="xs:string"/><xs:sequence minOccurs="0" maxOccurs="1000000"><xs:sequence minOccurs="0" maxOccurs="1000000"><xs:element name="a" type="xs:string" minOccurs="0"/></xs:sequence></xs:sequence></xs:sequence></xs:complexType></xs:element></xs:schema>"#;
    let source = SourceDocument::parse(b"<r><b/></r>").unwrap();
    authored(schema, 2, |e| {
        let o = e
            .validate(&source, &limits(), RequestControl::default())
            .unwrap();
        assert_eq!(o.assessment, Assessment::Incomplete);
        assert!(o.operational_failure.is_some());
        assert!(!o.finalization_attempted);
        assert!(e.is_retired());
        assert_eq!(
            o.failure,
            Some(Failure::Backend {
                phase: "operational"
            })
        );
    })
    .unwrap();
}
#[test]
fn portable_witnesses_explicitly_map_actual_handles_and_full_root() {
    let source =
        SourceDocument::parse(b"<?p q?><!--before--><r req='bad'><d>1</d><d>2</d><z/></r>")
            .unwrap();
    let o = authored(XSD, 1, |e| {
        e.validate(&source, &limits(), RequestControl::default())
            .unwrap()
    })
    .unwrap();
    let Some(SourceReference::Node { preorder }) = o.validated_root_reference else {
        panic!("actual root binding")
    };
    assert_eq!(
        source.nodes().nth(preorder).unwrap().identity(),
        source.root_element().identity()
    );
    let joined = diagnostic(&o, 0).source.as_ref().unwrap();
    assert_eq!(joined.owner_reference, SourceReference::Node { preorder });
    let SourceReference::Attribute {
        owner_preorder,
        attribute,
    } = joined.subject_reference
    else {
        panic!("attribute witness")
    };
    let owner = source.nodes().nth(owner_preorder).unwrap();
    let actual = owner.attributes().nth(attribute).unwrap();
    assert_eq!(
        actual.identity(),
        source
            .root_element()
            .attributes()
            .next()
            .unwrap()
            .identity()
    );
    assert_eq!(
        actual.token().span(),
        match joined.subject {
            Span::Utf8Bytes { start, end } =>
                einvoice_source::EvidenceSpan::Utf8Byte(einvoice_source::OriginalUtf8Span {
                    start,
                    end
                }),
            _ => panic!("UTF8"),
        }
    );
}
