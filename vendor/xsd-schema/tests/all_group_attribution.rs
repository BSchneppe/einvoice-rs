//! XSD 1.1 all-groups: an element particle wins over a competing wildcard.
//!
//! Structures 1.1 §3.8.4.1.3: "if one of the Particles in M ·attributes· an
//! element information item to a ·wildcard particle·, and a ·competing·
//! Particle ·attributes· the same item to an ·element particle·, then the
//! ·element particle· is used for validation." The Note there: for
//! `<xs:all><xs:any/><xs:element name="a"/></xs:all>`, "After the first `a`
//! the ·element particle· accepts no more elements and so no longer
//! ·competes· with the ·wildcard particle·", so `(<a/><a/>)` attributes the
//! first `a` to the element particle and the second to the wildcard.
//!
//! The matcher used to give an element to the first particle in declaration
//! order that matched it, so a wildcard declared before an element particle
//! (or inherited from a base type, whose particles come first in an
//! all-group extension) captured the element and valid instances were
//! rejected. Every case runs through the streaming validator, the
//! fragment-buffering (assertion) validator and the navigator driver.
#![cfg(feature = "xsd11")]

use xsd_schema::validation::{
    drive_navigator, drive_quick_xml, CollectingValidationSink, SchemaValidator, SchemaValidity,
    ValidationFlags,
};
use xsd_schema::{RoXmlNavigator, SchemaSet, SchemaSetBuilder};

use SchemaValidity::{Invalid, Valid};

/// Compile XSD 1.1 schema documents given as `(base URI, text)`.
fn compile(documents: &[(&str, &str)]) -> SchemaSet {
    let mut builder = SchemaSetBuilder::xsd11();
    for (uri, text) in documents {
        builder = builder
            .add_source(text, uri)
            .unwrap_or_else(|e| panic!("{uri}: {e}"));
    }
    builder
        .compile()
        .unwrap_or_else(|e| panic!("schema compiles: {e}"))
        .into_schema_set()
}

fn one(body: &str) -> SchemaSet {
    compile(&[(
        "file:///all.xsd",
        &format!(
            r###"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">{body}</xs:schema>"###
        ),
    )])
}

/// The root validity from each driver.
fn verdicts(set: &SchemaSet, xml: &str) -> [(&'static str, Option<SchemaValidity>); 3] {
    let stream = |validator: SchemaValidator<'_>| {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut runtime = validator.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        drive_quick_xml(xml.as_bytes(), &mut runtime, set)
            .expect("driver completes")
            .root_validity
    };
    let navigator = {
        let doc = roxmltree::Document::parse(xml).expect("well-formed");
        let nav = RoXmlNavigator::new(&doc);
        let validator = SchemaValidator::new(set, ValidationFlags::default());
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut runtime = validator.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        drive_navigator(&nav, &mut runtime, set)
            .expect("driver completes")
            .root_validity
    };
    [
        (
            "stream",
            stream(SchemaValidator::new(set, ValidationFlags::default())),
        ),
        (
            "fragment-buffer",
            stream(SchemaValidator::new_fragment_buffer(
                set,
                ValidationFlags::default(),
            )),
        ),
        ("navigator", navigator),
    ]
}

fn check(set: &SchemaSet, xml: &str, expected: SchemaValidity) {
    for (driver, got) in verdicts(set, xml) {
        assert_eq!(got, Some(expected), "{driver}: {xml}");
    }
}

/// The lead's reproduction: a wildcard declared before the element particle.
#[test]
fn element_particle_wins_over_an_earlier_wildcard() {
    let set = one(r###"<xs:element name="r"><xs:complexType><xs:all>
          <xs:any namespace="##any" processContents="skip"/>
          <xs:element name="E"/>
        </xs:all></xs:complexType></xs:element>"###);
    check(&set, "<r><E/><x/></r>", Valid);
    check(&set, "<r><x/><E/></r>", Valid);
    // Two non-E elements: the element particle is never satisfied.
    check(&set, "<r><x/><y/></r>", Invalid);
}

/// An all-group extension lists the base's particles first; a base wildcard
/// `##other` of `urn:a` matches `urn:b`, so it competes with the extension's
/// `b:E` (with U-D's correct namespace reading this became reachable).
#[test]
fn inherited_wildcard_does_not_capture_the_extension_element() {
    let a = r###"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:a" elementFormDefault="qualified">
        <xs:complexType name="Base"><xs:all>
          <xs:element name="T" type="xs:string"/>
          <xs:any namespace="##other" processContents="skip"/>
        </xs:all></xs:complexType></xs:schema>"###;
    let b = r###"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:a="urn:a" xmlns:b="urn:b" targetNamespace="urn:b" elementFormDefault="qualified">
        <xs:import namespace="urn:a"/>
        <xs:complexType name="D"><xs:complexContent><xs:extension base="a:Base">
          <xs:all><xs:element name="E" type="xs:string"/></xs:all>
        </xs:extension></xs:complexContent></xs:complexType>
        <xs:element name="r" type="b:D"/></xs:schema>"###;
    let set = compile(&[("file:///a.xsd", a), ("file:///b.xsd", b)]);
    let r = |content: &str| {
        format!(r###"<b:r xmlns:a="urn:a" xmlns:b="urn:b" xmlns:c="urn:c">{content}</b:r>"###)
    };
    // Rejected by the 0.2.3 release candidate (0.2.2 accepted them).
    check(&set, &r("<b:E>e</b:E><c:x/><a:T>t</a:T>"), Valid);
    check(&set, &r("<b:E>e</b:E><a:T>t</a:T><c:x/>"), Valid);
    check(&set, &r("<a:T>t</a:T><b:E>e</b:E><c:x/>"), Valid);
    // Already valid, and must stay so.
    check(&set, &r("<a:T>t</a:T><c:x/><b:E>e</b:E>"), Valid);
    check(&set, &r("<b:x/><a:T>t</a:T><b:E>e</b:E>"), Valid);
    // `##other` of urn:a excludes urn:a and unqualified names.
    check(&set, &r("<a:x/><a:T>t</a:T><b:E>e</b:E>"), Invalid);
    check(&set, &r("<x/><a:T>t</a:T><b:E>e</b:E>"), Invalid);
}

/// Once the element particle's {max occurs} is used up it no longer
/// competes, and the wildcard takes the item (§3.8.4.1.3 Note).
#[test]
fn an_exhausted_element_particle_leaves_the_item_to_the_wildcard() {
    let set = one(r###"<xs:element name="r"><xs:complexType><xs:all>
          <xs:any processContents="skip"/>
          <xs:element name="a"/>
        </xs:all></xs:complexType></xs:element>"###);
    check(&set, "<r><a/><a/></r>", Valid);
    check(&set, "<r><a/><x/></r>", Valid);
    check(&set, "<r><x/><a/></r>", Valid);
    check(&set, "<r><a/></r>", Invalid);
    check(&set, "<r><a/><a/><a/></r>", Invalid);

    // maxOccurs="2": the element particle takes the first two `a`s, so the
    // wildcard (minOccurs 1) is left without one in `<a/><a/>` — no
    // ·validation-path· attributes an `a` to the wildcard while the element
    // particle can still take it.
    let set = one(r###"<xs:element name="r"><xs:complexType><xs:all>
          <xs:any processContents="skip"/>
          <xs:element name="a" maxOccurs="2"/>
        </xs:all></xs:complexType></xs:element>"###);
    check(&set, "<r><a/><x/><a/></r>", Valid);
    check(&set, "<r><a/><a/><a/></r>", Valid);
    check(&set, "<r><a/><a/></r>", Invalid);
    check(&set, "<r><a/><a/><a/><a/></r>", Invalid);
}

/// An optional element particle competes too: the item is validated against
/// the element declaration, not skipped by the wildcard.
#[test]
fn an_optional_element_particle_still_wins() {
    let set = one(r###"<xs:element name="r"><xs:complexType><xs:all>
          <xs:any processContents="skip" minOccurs="0"/>
          <xs:element name="E" type="xs:int" minOccurs="0"/>
        </xs:all></xs:complexType></xs:element>"###);
    check(&set, "<r><E>5</E><x/></r>", Valid);
    check(&set, "<r><E>5</E></r>", Valid);
    check(&set, "<r><x/></r>", Valid);
    check(&set, "<r/>", Valid);
    // `E` is governed by its declaration: "abc" is not an xs:int.
    check(&set, "<r><E>abc</E></r>", Invalid);
}

/// A member of a substitution group is attributed to the head's element
/// particle (Structures §3.9.4.1.2 "matches").
#[test]
fn a_substitution_group_member_goes_to_the_head_particle() {
    let set = one(r###"<xs:element name="H" type="xs:string"/>
        <xs:element name="M" substitutionGroup="H"/>
        <xs:element name="r"><xs:complexType><xs:all>
          <xs:any namespace="##any" processContents="skip"/>
          <xs:element ref="H"/>
        </xs:all></xs:complexType></xs:element>"###);
    check(&set, "<r><M/><x/></r>", Valid);
    check(&set, "<r><x/><M/></r>", Valid);
    check(&set, "<r><M/><H/></r>", Valid);
    check(&set, "<r><M/><M/><M/></r>", Invalid);
}

/// Open content still only takes what no declared particle takes; an
/// all-group reached through a group reference behaves the same.
#[test]
fn open_content_and_group_references() {
    let interleave = one(r###"<xs:element name="r"><xs:complexType>
          <xs:openContent mode="interleave"><xs:any namespace="##any" processContents="skip"/></xs:openContent>
          <xs:all><xs:any processContents="skip"/><xs:element name="E" type="xs:int"/></xs:all>
        </xs:complexType></xs:element>"###);
    check(&interleave, "<r><E>1</E><x/></r>", Valid);
    check(&interleave, "<r><x/><E>1</E><y/></r>", Valid);
    check(&interleave, "<r><E>x</E><y/></r>", Invalid);

    let suffix = one(r###"<xs:element name="r"><xs:complexType>
          <xs:openContent mode="suffix"><xs:any namespace="##any" processContents="skip"/></xs:openContent>
          <xs:all><xs:any processContents="skip"/><xs:element name="E" type="xs:int"/></xs:all>
        </xs:complexType></xs:element>"###);
    check(&suffix, "<r><E>1</E><x/><y/></r>", Valid);
    check(&suffix, "<r><x/><E>1</E><y/></r>", Valid);
    // The suffix may contain anything once the all-group is satisfied.
    check(&suffix, "<r><E>1</E><x/><y/><E>2</E></r>", Valid);
    check(&suffix, "<r><x/><y/><E>1</E></r>", Invalid);

    let group = one(r###"<xs:group name="g"><xs:all>
          <xs:any processContents="skip"/><xs:element name="E"/>
        </xs:all></xs:group>
        <xs:element name="r"><xs:complexType><xs:group ref="g"/></xs:complexType></xs:element>"###);
    check(&group, "<r><E/><x/></r>", Valid);
    check(&group, "<r><x/><E/></r>", Valid);
}
