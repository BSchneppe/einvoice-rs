//! Authored XSD 1.0 character-content regressions, independent of invoice labels.
//! XML 1.0 S; XSD 1.0 cvc-complex-type.2.1/.2.3 and cvc-elt.3.2.1.
mod support;
use support::schema;
use xsd_schema::namespace::context::NamespaceContextSnapshot;
use xsd_schema::validation::errors::ValidationError;
use xsd_schema::validation::*;
use xsd_schema::{RoXmlNavigator, SchemaSet};

const XSD: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
<xs:element name="empty" nillable="true"><xs:complexType/></xs:element>
<xs:element name="eo" nillable="true"><xs:complexType><xs:sequence><xs:element name="child" type="xs:string" minOccurs="0"/></xs:sequence></xs:complexType></xs:element>
<xs:element name="required"><xs:complexType><xs:sequence><xs:element name="child" type="xs:string"/></xs:sequence></xs:complexType></xs:element>
<xs:element name="mixed" nillable="true"><xs:complexType mixed="true"><xs:sequence><xs:element name="child" minOccurs="0"/></xs:sequence></xs:complexType></xs:element>
<xs:element name="string" type="xs:string" nillable="true"/>
<xs:element name="token" type="xs:token" nillable="true"/>
<xs:element name="simple" nillable="true"><xs:complexType><xs:simpleContent><xs:extension base="xs:string"/></xs:simpleContent></xs:complexType></xs:element>
</xs:schema>"#;
const XML_S: &[&str] = &[" ", "\t", "\r", "\n", " \t\r\n"];
const OTHER: &[&str] = &[
    "\u{a0}", "\u{85}", "\u{1680}", "\u{2003}", "\u{2028}", "\u{3000}",
];

fn direct(
    set: &SchemaSet,
    name: &str,
    chunks: &[&str],
    nil: bool,
    whitespace: bool,
    typed: bool,
    value_free: bool,
) -> (SchemaValidity, Vec<ValidationError>) {
    let flags = if typed {
        ValidationFlags::default()
    } else {
        ValidationFlags::default() & !ValidationFlags::BUILD_PSVI_TYPED_VALUES
    };
    let validator = SchemaValidator::new(set, flags);
    assert!(validator.content_model_failures().is_empty());
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut rt = validator.start_run(CollectingValidationSink {
        errors: &mut errors,
        warnings: &mut warnings,
    });
    rt.validate_element_novalue(
        name,
        "",
        None,
        nil.then_some("true"),
        &NamespaceContextSnapshot::default(),
    );
    rt.validate_end_of_attributes_novalue();
    for text in chunks {
        if whitespace {
            rt.validate_whitespace(text)
        } else {
            rt.validate_text(text)
        }
    }
    let validity = if value_free {
        rt.validate_end_element_novalue()
    } else {
        let info = rt.validate_end_element();
        assert_eq!(info.validation_attempted, ValidationAttempted::Full);
        if nil {
            assert_eq!(info.is_nil, chunks.iter().all(|s| s.is_empty()));
        }
        info.validity
    };
    assert!(rt.end_validation().is_ok());
    assert!(rt.operational_failure().is_none());
    drop(rt);
    assert!(warnings.is_empty());
    (validity, errors)
}

fn expect(result: (SchemaValidity, Vec<ValidationError>), code: Option<&str>) {
    assert_eq!(
        result.0,
        if code.is_some() {
            SchemaValidity::Invalid
        } else {
            SchemaValidity::Valid
        }
    );
    if let Some(code) = code {
        assert!(
            result.1.iter().any(|e| e.constraint == code),
            "{:#?}",
            result.1
        );
        for event in &result.1 {
            assert!(event.instance_path().is_some());
        }
    } else {
        assert!(result.1.is_empty(), "{:#?}", result.1);
    }
}

#[test]
fn direct_text_uses_xml_s_only_and_preserves_legal_unicode() {
    let set = schema(XSD);
    for typed in [false, true] {
        for no_value in [false, true] {
            for text in XML_S {
                expect(
                    direct(&set, "eo", &[text], false, false, typed, no_value),
                    None,
                );
            }
            for text in OTHER {
                expect(
                    direct(&set, "eo", &[text], false, false, typed, no_value),
                    Some("cvc-complex-type.2.3"),
                );
                for name in ["mixed", "string", "token", "simple"] {
                    expect(
                        direct(&set, name, &[text], false, false, typed, no_value),
                        None,
                    );
                }
            }
        }
    }
}

#[test]
fn empty_and_nil_forbid_any_characters_in_both_entrypoints() {
    let set = schema(XSD);
    for typed in [false, true] {
        for no_value in [false, true] {
            for whitespace in [false, true] {
                for text in XML_S {
                    expect(
                        direct(&set, "empty", &[text], false, whitespace, typed, no_value),
                        Some("cvc-complex-type.2.1"),
                    );
                    for name in ["empty", "eo", "mixed", "string", "simple"] {
                        expect(
                            direct(&set, name, &[text], true, whitespace, typed, no_value),
                            Some("cvc-elt.3.2.1"),
                        );
                    }
                }
                for name in ["empty", "eo", "mixed", "string", "simple"] {
                    expect(
                        direct(&set, name, &[""], true, whitespace, typed, no_value),
                        None,
                    );
                }
            }
            for text in OTHER {
                expect(
                    direct(&set, "empty", &[text], false, false, typed, no_value),
                    Some("cvc-complex-type.2.1"),
                );
                expect(
                    direct(&set, "eo", &[text, "", text], true, false, typed, no_value),
                    Some("cvc-elt.3.2.1"),
                );
            }
        }
    }
}

fn drivers(set: &SchemaSet, xml: &str, code: Option<&str>) {
    for navigator in [false, true] {
        for typed in [false, true] {
            let flags = if typed {
                ValidationFlags::default()
            } else {
                ValidationFlags::default() & !ValidationFlags::BUILD_PSVI_TYPED_VALUES
            };
            let validator = SchemaValidator::new(set, flags);
            let mut errors = Vec::new();
            let mut warnings = Vec::new();
            let mut rt = validator.start_run(CollectingValidationSink {
                errors: &mut errors,
                warnings: &mut warnings,
            });
            let result = if navigator {
                let doc = roxmltree::Document::parse(xml).unwrap();
                drive_navigator(&RoXmlNavigator::new(&doc), &mut rt, set).unwrap()
            } else {
                drive_quick_xml(xml.as_bytes(), &mut rt, set).unwrap()
            };
            assert!(rt.operational_failure().is_none());
            drop(rt);
            expect((result.root_validity.unwrap(), errors), code);
            assert!(warnings.is_empty());
        }
    }
}

#[test]
fn both_bundled_drivers_route_xml_character_content() {
    let set = schema(XSD);
    for text in OTHER {
        let reference = format!("&#x{:X};", text.chars().next().unwrap() as u32);
        for body in [
            text.to_string(),
            reference,
            format!("<![CDATA[{text}]]>"),
            format!("{text}<!--c--><?p d?>{text}"),
        ] {
            drivers(
                &set,
                &format!("<eo>{body}</eo>"),
                Some("cvc-complex-type.2.3"),
            );
            drivers(
                &set,
                &format!("<required>{body}<child/></required>"),
                Some("cvc-complex-type.2.3"),
            );
            for name in ["mixed", "string", "token", "simple"] {
                drivers(&set, &format!("<{name}>{body}</{name}>"), None);
            }
        }
    }
    for name in ["empty", "eo", "mixed", "string", "simple"] {
        for body in [" ", "&#x9;", "&#xD;", "&#xA;", "<![CDATA[ ]]>\n<!--c-->"] {
            drivers(&set, &format!("<{name} xmlns:xsi='http://www.w3.org/2001/XMLSchema-instance' xsi:nil='true'>{body}</{name}>"), Some("cvc-elt.3.2.1"));
        }
        drivers(
            &set,
            &format!(
                "<{name} xmlns:xsi='http://www.w3.org/2001/XMLSchema-instance' xsi:nil='true'/>"
            ),
            None,
        );
    }
    drivers(&set, "<required> \t\r\n<child/></required>", None);
    drivers(&set, "<empty> </empty>", Some("cvc-complex-type.2.1"));
}
