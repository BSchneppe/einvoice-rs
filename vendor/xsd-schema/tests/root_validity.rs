//! An element's `[validity]` reflects its children, attributes, identity
//! constraints and — for the validation root — its ID/IDREF table, end to end
//! through the public drivers (`DriveOutcome::root_validity`) and the push API.
//!
//! Structures §3.3.5.1 Assessment Outcome (Element), `[validity]`: "1 If it
//! was ·strictly assessed·, then … 1.1 If all of the following are true: 1.1.1
//! [the item was locally ·valid·] … 1.1.2 Neither its [children] nor its
//! [attributes] contains an information item (element or attribute
//! respectively) whose [validity] is invalid. 1.1.3 Neither its [children] nor
//! its [attributes] contains an information item (element or attribute
//! respectively) which is ·attributed· to a strict ·wildcard particle· and
//! whose [validity] is notKnown. then valid; 1.2 otherwise invalid. 2
//! otherwise notKnown."
//!
//! Element Locally Valid (Element) (§3.3.4.3): "6 E is ·valid· with respect to
//! each of the {identity-constraint definitions} as per Identity-constraint
//! Satisfied (§3.11.4). 7 If E is the ·validation root·, then it is ·valid· per
//! Validation Root Valid (ID/IDREF) (§3.3.4.5)."
//!
//! The W3C suites judge an instance by "any error reported, else the root's
//! validity", so a root left `valid` beside reported errors is invisible to
//! them; these tests assert `root_validity` directly.

use xsd_schema::namespace::context::NamespaceContextSnapshot;
use xsd_schema::validation::{
    drive_navigator, drive_quick_xml, drive_quick_xml_with, CollectingValidationSink,
    EndElementInfo, SchemaValidator, SchemaValidity, ValidationEventHandler, ValidationFlags,
};
use xsd_schema::{RoXmlNavigator, SchemaSet, SchemaSetBuilder, XsdVersion};

use SchemaValidity::{Invalid, Valid};

// ── Harness ───────────────────────────────────────────────────────────────

fn schema(version: XsdVersion, body: &str) -> SchemaSet {
    let xsd =
        format!(r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">{body}</xs:schema>"#);
    SchemaSetBuilder::with_version(version)
        .add_source(&xsd, "file:///root_validity.xsd")
        .expect("schema source accepted")
        .compile()
        .expect("schema compiles")
        .into_schema_set()
}

fn validator(ss: &SchemaSet, psvi: bool) -> SchemaValidator<'_> {
    let mut flags = ValidationFlags::default();
    if !psvi {
        // The lexical fast paths (no typed values) must reach the same verdicts.
        flags &= !ValidationFlags::BUILD_PSVI_TYPED_VALUES;
    }
    #[cfg(feature = "xsd11")]
    {
        SchemaValidator::new_fragment_buffer(ss, flags)
    }
    #[cfg(not(feature = "xsd11"))]
    {
        SchemaValidator::new(ss, flags)
    }
}

/// Root validity and reported constraint codes from one driver.
type Outcome = (Option<SchemaValidity>, Vec<String>);

fn stream(ss: &SchemaSet, xml: &str, psvi: bool) -> Outcome {
    let v = validator(ss, psvi);
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let validity = {
        let mut rt = v.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        drive_quick_xml(xml.as_bytes(), &mut rt, ss)
            .expect("stream drive completes")
            .root_validity
    };
    (validity, codes(&errors))
}

fn dom(ss: &SchemaSet, xml: &str) -> Outcome {
    let v = validator(ss, true);
    let doc = roxmltree::Document::parse(xml).expect("well-formed instance");
    let nav = RoXmlNavigator::new(&doc);
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let validity = {
        let mut rt = v.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        drive_navigator(&nav, &mut rt, ss)
            .expect("navigator drive completes")
            .root_validity
    };
    (validity, codes(&errors))
}

#[cfg(feature = "xsd11")]
fn buffer(ss: &SchemaSet, xml: &str) -> Outcome {
    use xsd_schema::document::{BufferDocument, BufferDocumentOptions};
    use xsd_schema::validation::drive_buffer_document;
    let v = validator(ss, true);
    let arena = bumpalo::Bump::new();
    let doc = BufferDocument::from_reader(
        xml.as_bytes(),
        &arena,
        &ss.name_table,
        BufferDocumentOptions::default(),
        None,
    )
    .expect("buffer build");
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let validity = {
        let mut rt = v.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        drive_buffer_document(&doc, &mut rt, ss)
            .expect("buffer drive completes")
            .root_validity
    };
    (validity, codes(&errors))
}

fn codes(errors: &[xsd_schema::validation::ValidationError]) -> Vec<String> {
    errors.iter().map(|e| e.constraint.to_string()).collect()
}

/// Every driver: the root's `[validity]` is `expected`, and an error whose
/// constraint code starts with `code` was reported (`None`: no error at all).
fn check(ss: &SchemaSet, xml: &str, expected: SchemaValidity, code: Option<&str>) {
    let runs = [
        ("stream", stream(ss, xml, true)),
        ("stream/no-psvi", stream(ss, xml, false)),
        ("navigator", dom(ss, xml)),
    ];
    #[cfg(feature = "xsd11")]
    let runs = runs.into_iter().chain([("buffer", buffer(ss, xml))]);
    for (driver, (validity, errors)) in runs {
        assert_eq!(
            validity,
            Some(expected),
            "{driver}: root [validity] of {xml} (errors {errors:?})"
        );
        match code {
            Some(code) => assert!(
                errors.iter().any(|e| e.starts_with(code)),
                "{driver}: expected a {code} error for {xml}, got {errors:?}"
            ),
            None => assert!(errors.is_empty(), "{driver}: {xml}: errors {errors:?}"),
        }
    }
}

const VERSIONS: [XsdVersion; 2] = [XsdVersion::V1_0, XsdVersion::V1_1];

// ── Children and attributes (§3.3.5.1 clause 1.1.2) ───────────────────────

const CONTENT: &str = r#"
  <xs:element name="n" type="xs:int"/>
  <xs:element name="s3"><xs:complexType><xs:sequence><xs:element ref="n"/></xs:sequence></xs:complexType></xs:element>
  <xs:element name="s4"><xs:complexType><xs:attribute name="at" type="xs:int"/></xs:complexType></xs:element>
  <xs:element name="s5"><xs:complexType><xs:sequence>
    <xs:element name="m"><xs:complexType><xs:sequence>
      <xs:element name="m2"><xs:complexType><xs:sequence><xs:element ref="n"/></xs:sequence></xs:complexType></xs:element>
    </xs:sequence></xs:complexType></xs:element>
  </xs:sequence></xs:complexType></xs:element>
  <xs:element name="s6"><xs:complexType><xs:sequence>
    <xs:element name="c"><xs:complexType><xs:attribute name="at" type="xs:int"/></xs:complexType></xs:element>
  </xs:sequence></xs:complexType></xs:element>"#;

#[test]
fn invalid_child_content_invalidates_the_root() {
    for version in VERSIONS {
        let ss = schema(version, CONTENT);
        check(
            &ss,
            "<s3><n>abc</n></s3>",
            Invalid,
            Some("cvc-datatype-valid"),
        );
        check(&ss, "<s3><n>7</n></s3>", Valid, None);
    }
}

#[test]
fn invalid_descendant_three_levels_down_invalidates_the_root() {
    for version in VERSIONS {
        let ss = schema(version, CONTENT);
        check(
            &ss,
            "<s5><m><m2><n>abc</n></m2></m></s5>",
            Invalid,
            Some("cvc-datatype-valid"),
        );
        check(&ss, "<s5><m><m2><n>1</n></m2></m></s5>", Valid, None);
    }
}

#[test]
fn invalid_attribute_of_a_child_invalidates_the_root() {
    for version in VERSIONS {
        let ss = schema(version, CONTENT);
        check(
            &ss,
            "<s6><c at='abc'/></s6>",
            Invalid,
            Some("cvc-datatype-valid"),
        );
        // The root's own attribute already did (guard).
        check(&ss, "<s4 at='abc'/>", Invalid, Some("cvc-datatype-valid"));
        check(&ss, "<s6><c at='5'/></s6>", Valid, None);
    }
}

/// Every element on the path reports `invalid` at its own end event, through
/// the per-element hook of the streaming driver (the novalue end path).
#[test]
fn every_ancestor_of_an_invalid_element_ends_invalid() {
    struct Ends(Vec<(usize, SchemaValidity)>);
    impl ValidationEventHandler for Ends {
        type Error = std::convert::Infallible;
        fn after_end_element(
            &mut self,
            info: &EndElementInfo,
            depth: usize,
        ) -> Result<(), Self::Error> {
            self.0.push((depth, info.validity));
            Ok(())
        }
    }
    let ss = schema(XsdVersion::V1_0, CONTENT);
    let v = validator(&ss, true);
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut ends = Ends(Vec::new());
    {
        let mut rt = v.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        drive_quick_xml_with(
            "<s5><m><m2><n>abc</n></m2></m></s5>".as_bytes(),
            &mut rt,
            &ss,
            &mut ends,
        )
        .expect("drive completes");
        rt.end_validation().expect("end_validation");
    }
    assert_eq!(
        ends.0,
        vec![(4, Invalid), (3, Invalid), (2, Invalid), (1, Invalid)]
    );
}

/// The push API's value-returning end path agrees.
#[test]
fn push_api_end_element_reports_the_parent_invalid() {
    let ss = schema(XsdVersion::V1_0, CONTENT);
    let v = validator(&ss, true);
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut rt = v.start_run(CollectingValidationSink {
        errors: &mut errors,
        warnings: &mut warnings,
    });
    let ns = NamespaceContextSnapshot::default();
    rt.validate_element("s3", "", None, None, &ns);
    rt.validate_end_of_attributes();
    rt.validate_element("n", "", None, None, &ns);
    rt.validate_end_of_attributes();
    rt.validate_text("abc");
    assert_eq!(rt.validate_end_element().validity, Invalid, "n");
    assert_eq!(rt.validate_end_element().validity, Invalid, "s3");
    rt.end_validation().expect("end_validation");
    drop(rt);
    assert!(codes(&errors)
        .iter()
        .any(|c| c.starts_with("cvc-datatype-valid")));
}

// ── A child's own local validity: content, nil, abstract, attributes ──────

const LOCAL: &str = r#"
  <xs:element name="abs" type="xs:string" abstract="true"/>
  <xs:element name="r"><xs:complexType><xs:sequence>
    <xs:element name="eo" minOccurs="0"><xs:complexType><xs:sequence><xs:element name="x" minOccurs="0"/></xs:sequence></xs:complexType></xs:element>
    <xs:element name="empty" minOccurs="0"><xs:complexType/></xs:element>
    <xs:element name="nil" minOccurs="0" nillable="true"><xs:complexType mixed="true"><xs:sequence><xs:element name="x" minOccurs="0"/></xs:sequence></xs:complexType></xs:element>
    <xs:element name="s" type="xs:string" minOccurs="0"/>
    <xs:element ref="abs" minOccurs="0"/>
  </xs:sequence></xs:complexType></xs:element>"#;

/// §3.4.4.2 clauses 1.1 / 1.3 and §3.3.4.3 clause 3.2.3.1: character or
/// element children where the child's type or `xsi:nil` forbids them.
#[test]
fn child_content_violations_invalidate_the_root() {
    for version in VERSIONS {
        let ss = schema(version, LOCAL);
        check(
            &ss,
            "<r><eo>text</eo></r>",
            Invalid,
            Some("cvc-complex-type.2.3"),
        );
        check(
            &ss,
            "<r><empty>text</empty></r>",
            Invalid,
            Some("cvc-complex-type.2.1"),
        );
        check(
            &ss,
            "<r><empty> </empty></r>",
            Invalid,
            Some("cvc-complex-type.2.1"),
        );
        let xsi = "xmlns:xsi='http://www.w3.org/2001/XMLSchema-instance'";
        check(
            &ss,
            &format!("<r {xsi}><nil xsi:nil='true'><x/></nil></r>"),
            Invalid,
            Some("cvc-elt.3.2.1"),
        );
        check(
            &ss,
            &format!("<r {xsi}><nil xsi:nil='true'>text</nil></r>"),
            Invalid,
            Some("cvc-elt.3.2.1"),
        );
        check(
            &ss,
            &format!("<r {xsi}><nil xsi:nil='true'/></r>"),
            Valid,
            None,
        );
        check(&ss, "<r><eo> <x/> </eo><empty/></r>", Valid, None);
    }
}

/// §3.3.4.3 clause 2: "D.{abstract} = false." (A direct reference in a
/// content model is already rejected by the parent's content model, which
/// the parent's own clause 1.4 covers; these two cases are not.)
#[test]
fn abstract_element_declaration_invalidates_the_element() {
    for version in VERSIONS {
        let ss = schema(version, LOCAL);
        // The validation root itself.
        check(&ss, "<abs>a</abs>", Invalid, Some("cvc-elt.2"));
        // Matched by a lax wildcard, so governed by the abstract declaration.
        let ss = schema(
            version,
            r#"
  <xs:element name="abs" type="xs:string" abstract="true"/>
  <xs:element name="lax"><xs:complexType><xs:sequence><xs:any processContents="lax"/></xs:sequence></xs:complexType></xs:element>"#,
        );
        check(&ss, "<lax><abs>a</abs></lax>", Invalid, Some("cvc-elt.2"));
    }
}

/// Attributes whose own `[validity]` is invalid: an `xsi:` value, an
/// attribute on a simple-typed element (§3.3.4.4 clause 3.1.1).
#[test]
fn invalid_child_attribute_kinds_invalidate_the_root() {
    for version in VERSIONS {
        let ss = schema(version, LOCAL);
        let xsi = "xmlns:xsi='http://www.w3.org/2001/XMLSchema-instance'";
        check(
            &ss,
            &format!("<r {xsi}><nil xsi:nil='maybe'/></r>"),
            Invalid,
            Some("cvc-datatype-valid"),
        );
        check(
            &ss,
            &format!("<r {xsi}><eo xsi:schemaLocation='urn:only-one-token'/></r>"),
            Invalid,
            Some("cvc-schema-location"),
        );
        check(
            &ss,
            "<r><s at='1'>a</s></r>",
            Invalid,
            Some("cvc-complex-type.3.2.1"),
        );
    }
}

/// XSD 1.0 only: at most one attribute of type `xs:ID` per element, counting
/// the ones a wildcard admits.
#[test]
fn second_id_attribute_on_a_child_invalidates_the_root_xsd10() {
    let ss = schema(
        XsdVersion::V1_0,
        r#"
  <xs:attribute name="gid" type="xs:ID"/>
  <xs:element name="r"><xs:complexType><xs:sequence>
    <xs:element name="c"><xs:complexType>
      <xs:attribute name="id" type="xs:ID"/><xs:anyAttribute processContents="lax"/>
    </xs:complexType></xs:element>
  </xs:sequence></xs:complexType></xs:element>"#,
    );
    check(
        &ss,
        "<r><c id='a' gid='b'/></r>",
        Invalid,
        Some("cvc-complex-type"),
    );
    check(&ss, "<r><c id='a'/></r>", Valid, None);
}

/// §3.16.4 String Valid clause 3: "Every ·ENTITY value· in V is a ·declared
/// entity name·" — as an attribute value and as element content.
#[test]
fn undeclared_entity_in_a_child_invalidates_the_root() {
    let run = |ss: &SchemaSet, xml: &str| -> Outcome {
        let v = validator(ss, true);
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let validity = {
            let mut rt = v.start_run(CollectingValidationSink {
                errors: &mut errors,
                warnings: &mut warnings,
            });
            rt.set_unparsed_entities(["pic".to_string()].into_iter().collect());
            drive_quick_xml(xml.as_bytes(), &mut rt, ss)
                .expect("stream drive completes")
                .root_validity
        };
        (validity, codes(&errors))
    };
    for version in VERSIONS {
        let ss = schema(
            version,
            r#"
  <xs:element name="r"><xs:complexType><xs:sequence>
    <xs:element name="a" minOccurs="0"><xs:complexType><xs:attribute name="ent" type="xs:ENTITY"/></xs:complexType></xs:element>
    <xs:element name="e" type="xs:ENTITY" minOccurs="0"/>
  </xs:sequence></xs:complexType></xs:element>"#,
        );
        for xml in ["<r><a ent='nope'/></r>", "<r><e>nope</e></r>"] {
            let (validity, errors) = run(&ss, xml);
            assert_eq!(validity, Some(Invalid), "{xml}: {errors:?}");
            assert!(
                errors.iter().any(|c| c.starts_with("cvc-datatype-valid")),
                "{xml}: {errors:?}"
            );
        }
        assert_eq!(
            run(&ss, "<r><a ent='pic'/><e>pic</e></r>"),
            (Some(Valid), vec![])
        );
    }
}

// ── Wildcards: strict, lax, skip (§3.3.5.1 clauses 1.1.3 and 2) ───────────

const WILDCARDS: &str = r#"
  <xs:element name="n" type="xs:int"/>
  <xs:attribute name="gat" type="xs:int"/>
  <xs:element name="lax"><xs:complexType><xs:sequence><xs:any processContents="lax"/></xs:sequence></xs:complexType></xs:element>
  <xs:element name="skip"><xs:complexType><xs:sequence><xs:any processContents="skip"/></xs:sequence></xs:complexType></xs:element>
  <xs:element name="strict"><xs:complexType><xs:sequence><xs:any processContents="strict"/></xs:sequence></xs:complexType></xs:element>"#;

#[test]
fn lax_matched_declared_child_counts_strict_wildcard_undeclared_child_counts() {
    for version in VERSIONS {
        let ss = schema(version, WILDCARDS);
        // A declared element matched by a lax wildcard is strictly assessed
        // (governing element declaration, clause 3) — clause 1.1.2 applies.
        check(
            &ss,
            "<lax><n>abc</n></lax>",
            Invalid,
            Some("cvc-datatype-valid"),
        );
        // Undeclared under a strict wildcard: clause 1.1.3.
        check(
            &ss,
            "<strict><x:foo xmlns:x='urn:x'/></strict>",
            Invalid,
            Some("cvc-elt.1"),
        );
    }
}

/// A laxly assessed element is `notKnown` (clause 2), whatever lies inside or
/// on it; its strictly assessed parent stays `valid` (clauses 1.1.2 / 1.1.3
/// only look at invalid children, and at notKnown ones attributed to *strict*
/// wildcards). The errors inside are still reported.
#[test]
fn laxly_assessed_subtree_does_not_invalidate_the_root() {
    for version in VERSIONS {
        let ss = schema(version, WILDCARDS);
        check(
            &ss,
            "<lax><x:foo xmlns:x='urn:x'><n>abc</n></x:foo></lax>",
            Valid,
            Some("cvc-datatype-valid"),
        );
        check(
            &ss,
            "<lax><x:foo xmlns:x='urn:x'><x:bar><n>abc</n></x:bar></x:foo></lax>",
            Valid,
            Some("cvc-datatype-valid"),
        );
        check(
            &ss,
            "<lax><x:foo xmlns:x='urn:x' gat='abc'/></lax>",
            Valid,
            Some("cvc-datatype-valid"),
        );
        // Skipped content is not assessed at all (guard).
        check(&ss, "<skip><n>abc</n></skip>", Valid, None);
    }
}

// ── Identity constraints (§3.3.4.3 clause 6) ──────────────────────────────

const KEYED: &str = r#"
  <xs:complexType name="K"><xs:attribute name="id" type="xs:string"/><xs:attribute name="id2" type="xs:string"/></xs:complexType>
  <xs:complexType name="R"><xs:attribute name="ref" type="xs:string"/></xs:complexType>
  <xs:element name="onroot"><xs:complexType><xs:sequence><xs:element name="k" type="K" maxOccurs="unbounded"/></xs:sequence></xs:complexType>
    <xs:key name="rootKey"><xs:selector xpath="k"/><xs:field xpath="@id"/></xs:key></xs:element>
  <xs:element name="multi"><xs:complexType><xs:sequence><xs:element name="k" type="K" maxOccurs="unbounded"/></xs:sequence></xs:complexType>
    <xs:unique name="multiUnique"><xs:selector xpath="k"/><xs:field xpath="@id|@id2"/></xs:unique></xs:element>
  <xs:element name="onchild"><xs:complexType><xs:sequence>
    <xs:element name="mid"><xs:complexType><xs:sequence>
      <xs:element name="k" type="K" minOccurs="0" maxOccurs="unbounded"/>
      <xs:element name="r" type="R" minOccurs="0" maxOccurs="unbounded"/>
    </xs:sequence></xs:complexType>
    <xs:key name="midKey"><xs:selector xpath="k"/><xs:field xpath="@id"/></xs:key>
    <xs:keyref name="midRef" refer="midKey"><xs:selector xpath="r"/><xs:field xpath="@ref"/></xs:keyref></xs:element>
  </xs:sequence></xs:complexType></xs:element>
  <xs:element name="siblings"><xs:complexType><xs:choice maxOccurs="unbounded">
    <xs:element name="keys"><xs:complexType><xs:sequence><xs:element name="k" type="K" maxOccurs="unbounded"/></xs:sequence></xs:complexType>
      <xs:key name="sibKey"><xs:selector xpath="k"/><xs:field xpath="@id"/></xs:key></xs:element>
    <xs:element name="refs"><xs:complexType><xs:sequence><xs:element name="r" type="R" maxOccurs="unbounded"/></xs:sequence></xs:complexType>
      <xs:keyref name="sibRef" refer="sibKey"><xs:selector xpath="r"/><xs:field xpath="@ref"/></xs:keyref></xs:element>
  </xs:choice></xs:complexType></xs:element>
  <xs:element name="ancestor"><xs:complexType><xs:sequence>
    <xs:element name="refs"><xs:complexType><xs:sequence><xs:element name="r" type="R" maxOccurs="unbounded"/></xs:sequence></xs:complexType>
      <xs:keyref name="ancRef" refer="ancKey"><xs:selector xpath="r"/><xs:field xpath="@ref"/></xs:keyref></xs:element>
    <xs:element name="k" type="K" maxOccurs="unbounded"/>
  </xs:sequence></xs:complexType>
    <xs:key name="ancKey"><xs:selector xpath="k"/><xs:field xpath="@id"/></xs:key></xs:element>
  <xs:element name="lax"><xs:complexType><xs:sequence><xs:any processContents="lax"/></xs:sequence></xs:complexType></xs:element>"#;

#[test]
fn identity_constraint_violation_invalidates_its_element_and_the_root() {
    for version in VERSIONS {
        let ss = schema(version, KEYED);
        // On the root itself.
        check(
            &ss,
            "<onroot><k id='1'/><k id='1'/></onroot>",
            Invalid,
            Some("cvc-identity-constraint.4.2.2"),
        );
        // A field selecting two nodes (§3.11.4 clause 3), reported at an attribute.
        check(
            &ss,
            "<multi><k id='1' id2='2'/></multi>",
            Invalid,
            Some("cvc-identity-constraint.4.2.1"),
        );
        // On a child: duplicate key, unmatched keyref.
        check(
            &ss,
            "<onchild><mid><k id='1'/><k id='1'/></mid></onchild>",
            Invalid,
            Some("cvc-identity-constraint.4.2.2"),
        );
        check(
            &ss,
            "<onchild><mid><k id='1'/><r ref='2'/></mid></onchild>",
            Invalid,
            Some("cvc-identity-constraint.4.3"),
        );
        check(&ss, "<onroot><k id='1'/><k id='2'/></onroot>", Valid, None);
        check(
            &ss,
            "<onchild><mid><k id='1'/><r ref='1'/></mid></onchild>",
            Valid,
            None,
        );
    }
}

/// Keyrefs whose referenced table is not in scope when their element closes
/// are retried as the ancestors close; a failure found that late still makes
/// the root invalid.
#[test]
fn deferred_keyref_failure_invalidates_the_root() {
    for version in VERSIONS {
        let ss = schema(version, KEYED);
        // Key table arrives with a later sibling.
        check(
            &ss,
            "<siblings><refs><r ref='2'/></refs><keys><k id='1'/></keys></siblings>",
            Invalid,
            Some("cvc-identity-constraint.4.3"),
        );
        // Key table from an earlier sibling: retried while the keyref's own
        // element is still closing.
        check(
            &ss,
            "<siblings><keys><k id='1'/></keys><refs><r ref='2'/></refs></siblings>",
            Invalid,
            Some("cvc-identity-constraint.4.3"),
        );
        // Referring to an ancestor's key: unresolved when the root closes.
        check(
            &ss,
            "<ancestor><refs><r ref='1'/></refs><k id='1'/></ancestor>",
            Invalid,
            Some("cvc-identity-constraint.4.3"),
        );
        // (No `valid` guard here: a keyref whose referenced key sits on a
        // sibling is accepted when the values match, although §3.11.4 clause
        // 4.3 looks only in the keyref element's own [identity-constraint
        // table] — a leniency of the retry, outside this file's subject.)
    }
}

/// The keyref's element is invalid, but a laxly assessed ancestor between it
/// and the root is `notKnown` and the root stays `valid`.
#[test]
fn deferred_keyref_failure_below_a_lax_element_leaves_the_root_valid() {
    for version in VERSIONS {
        let ss = schema(version, KEYED);
        check(
            &ss,
            "<lax><x:foo xmlns:x='urn:x'><siblings><refs><r ref='2'/></refs>\
             <keys><k id='1'/></keys></siblings></x:foo></lax>",
            Valid,
            Some("cvc-identity-constraint.4.3"),
        );
    }
}

// ── ID / IDREF on the validation root (§3.3.4.3 clause 7, §3.3.4.5) ───────

const IDS: &str = r#"
  <xs:complexType name="E"><xs:attribute name="id" type="xs:ID"/><xs:attribute name="ref" type="xs:IDREF"/></xs:complexType>
  <xs:element name="d"><xs:complexType><xs:sequence><xs:element name="e" type="E" maxOccurs="unbounded"/></xs:sequence></xs:complexType></xs:element>
  <xs:element name="dd"><xs:complexType><xs:sequence>
    <xs:element name="m" maxOccurs="unbounded"><xs:complexType><xs:sequence><xs:element name="e" type="E" maxOccurs="unbounded"/></xs:sequence></xs:complexType></xs:element>
  </xs:sequence></xs:complexType></xs:element>"#;

#[test]
fn duplicate_id_invalidates_the_validation_root() {
    for version in VERSIONS {
        let ss = schema(version, IDS);
        check(
            &ss,
            "<d><e id='a'/><e id='a'/></d>",
            Invalid,
            Some("cvc-id.2"),
        );
        check(
            &ss,
            "<dd><m><e id='a'/></m><m><e id='a'/></m></dd>",
            Invalid,
            Some("cvc-id.2"),
        );
    }
}

#[test]
fn dangling_idref_invalidates_the_validation_root() {
    for version in VERSIONS {
        let ss = schema(version, IDS);
        check(&ss, "<d><e ref='zz'/></d>", Invalid, Some("cvc-id.1"));
        check(&ss, "<d><e ref='a'/><e id='a'/></d>", Valid, None);
    }
}

// ── XSD 1.1: assertions and conditional type assignment ──────────────────

#[cfg(feature = "xsd11")]
const ASSERTED: &str = r#"
  <xs:complexType name="Fails"><xs:sequence/><xs:assert test="false()"/></xs:complexType>
  <xs:simpleType name="OkStr"><xs:restriction base="xs:string"><xs:assertion test="$value = 'ok'"/></xs:restriction></xs:simpleType>
  <xs:element name="inner" type="Fails"/>
  <xs:element name="plain"><xs:complexType><xs:sequence><xs:element ref="inner"/></xs:sequence></xs:complexType></xs:element>
  <xs:element name="asserting"><xs:complexType><xs:sequence><xs:element ref="inner"/></xs:sequence><xs:assert test="true()"/></xs:complexType></xs:element>
  <xs:element name="deep"><xs:complexType><xs:sequence><xs:element name="m"><xs:complexType><xs:sequence>
    <xs:element name="m2"><xs:complexType><xs:sequence><xs:element ref="inner"/></xs:sequence></xs:complexType></xs:element>
  </xs:sequence></xs:complexType></xs:element></xs:sequence></xs:complexType></xs:element>
  <xs:element name="deepAsserting"><xs:complexType><xs:sequence><xs:element name="m"><xs:complexType><xs:sequence>
    <xs:element name="m2"><xs:complexType><xs:sequence><xs:element ref="inner"/></xs:sequence></xs:complexType></xs:element>
  </xs:sequence></xs:complexType></xs:element></xs:sequence><xs:assert test="true()"/></xs:complexType></xs:element>
  <xs:element name="midAsserting"><xs:complexType><xs:sequence><xs:element name="m"><xs:complexType><xs:sequence>
    <xs:element ref="inner"/></xs:sequence><xs:assert test="true()"/></xs:complexType></xs:element></xs:sequence></xs:complexType></xs:element>
  <xs:element name="laxAsserting"><xs:complexType><xs:sequence><xs:any processContents="lax" namespace="urn:x"/></xs:sequence><xs:assert test="true()"/></xs:complexType></xs:element>
  <xs:element name="lax"><xs:complexType><xs:sequence><xs:any processContents="lax"/></xs:sequence></xs:complexType></xs:element>
  <xs:element name="attrFacet"><xs:complexType><xs:sequence>
    <xs:element name="c"><xs:complexType><xs:attribute name="at" type="OkStr"/></xs:complexType></xs:element>
  </xs:sequence></xs:complexType></xs:element>
  <xs:element name="contentFacet"><xs:complexType><xs:sequence><xs:element name="v" type="OkStr"/></xs:sequence></xs:complexType></xs:element>
  <xs:complexType name="IntAttr"><xs:attribute name="kind" type="xs:string"/><xs:attribute name="v" type="xs:int"/></xs:complexType>
  <xs:complexType name="FailsK"><xs:attribute name="kind" type="xs:string"/><xs:assert test="false()"/></xs:complexType>
  <xs:element name="e">
    <xs:alternative test="@kind = 'int'" type="IntAttr"/>
    <xs:alternative test="@kind = 'fail'" type="FailsK"/>
  </xs:element>
  <xs:element name="cta"><xs:complexType><xs:sequence><xs:element ref="e"/></xs:sequence></xs:complexType></xs:element>"#;

/// The reproduction from the 0.2.1 report: `<outer><inner/></outer>` with a
/// failing assertion on `inner` (Structures §3.4.4.2 clause 6).
#[cfg(feature = "xsd11")]
#[test]
fn failing_assertion_on_a_child_invalidates_the_root() {
    let ss = schema(XsdVersion::V1_1, ASSERTED);
    check(
        &ss,
        "<plain><inner/></plain>",
        Invalid,
        Some("cvc-assertion"),
    );
    check(
        &ss,
        "<deep><m><m2><inner/></m2></m></deep>",
        Invalid,
        Some("cvc-assertion"),
    );
    // On the root itself (guard).
    check(&ss, "<inner/>", Invalid, Some("cvc-assertion"));
}

/// A nested asserted element's assertions are evaluated only when the
/// outermost asserted element closes; the failure still reaches the root.
#[cfg(feature = "xsd11")]
#[test]
fn deferred_nested_assertion_failure_invalidates_the_root() {
    let ss = schema(XsdVersion::V1_1, ASSERTED);
    check(
        &ss,
        "<asserting><inner/></asserting>",
        Invalid,
        Some("cvc-assertion"),
    );
    check(
        &ss,
        "<deepAsserting><m><m2><inner/></m2></m></deepAsserting>",
        Invalid,
        Some("cvc-assertion"),
    );
    check(
        &ss,
        "<midAsserting><m><inner/></m></midAsserting>",
        Invalid,
        Some("cvc-assertion"),
    );
}

/// The failing element is invalid, but a laxly assessed element between it
/// and the root is `notKnown` (§3.3.5.1 clause 2) — immediate or deferred.
#[cfg(feature = "xsd11")]
#[test]
fn failing_assertion_below_a_lax_element_leaves_the_root_valid() {
    let ss = schema(XsdVersion::V1_1, ASSERTED);
    check(
        &ss,
        "<lax><x:foo xmlns:x='urn:x'><inner/></x:foo></lax>",
        Valid,
        Some("cvc-assertion"),
    );
    check(
        &ss,
        "<laxAsserting><x:foo xmlns:x='urn:x'><inner/></x:foo></laxAsserting>",
        Valid,
        Some("cvc-assertion"),
    );
}

#[cfg(feature = "xsd11")]
#[test]
fn assertion_facet_failure_on_a_child_invalidates_the_root() {
    let ss = schema(XsdVersion::V1_1, ASSERTED);
    check(
        &ss,
        "<attrFacet><c at='bad'/></attrFacet>",
        Invalid,
        Some("cvc-assertion"),
    );
    check(
        &ss,
        "<contentFacet><v>bad</v></contentFacet>",
        Invalid,
        Some("cvc-assertion"),
    );
    check(&ss, "<attrFacet><c at='ok'/></attrFacet>", Valid, None);
    check(&ss, "<contentFacet><v>ok</v></contentFacet>", Valid, None);
}

/// Conditional type assignment: attributes validated only after the type
/// alternative is selected, and a selected type with a failing assertion.
#[cfg(feature = "xsd11")]
#[test]
fn conditional_type_assignment_failure_on_a_child_invalidates_the_root() {
    let ss = schema(XsdVersion::V1_1, ASSERTED);
    check(
        &ss,
        "<cta><e kind='int' v='abc'/></cta>",
        Invalid,
        Some("cvc-datatype-valid"),
    );
    check(
        &ss,
        "<cta><e kind='fail'/></cta>",
        Invalid,
        Some("cvc-assertion"),
    );
    // On the root itself, and the valid case (guards).
    check(
        &ss,
        "<e kind='int' v='abc'/>",
        Invalid,
        Some("cvc-datatype-valid"),
    );
    check(&ss, "<cta><e kind='int' v='5'/></cta>", Valid, None);
}
