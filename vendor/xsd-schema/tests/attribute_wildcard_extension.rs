//! The `{attribute wildcard}` a complex type gets by extension is the one a
//! restriction of that type is checked against.
//!
//! Structures 1.1 §3.4.2.5: for {derivation method} = extension, "the ·base
//! wildcard·" is the {attribute wildcard} of the {base type definition}, and
//! clause 2.2.2.2 reads "If the ·complete wildcard· is ·absent·, then the
//! ·base wildcard·"; clause 2.2.2.3 takes the {process contents} of the
//! ·complete wildcard· and the union of the two namespace constraints. XSD 1.0
//! §3.4.2 (complex content) has the same rule (clauses 3.2.1 – 3.2.2). A type
//! that extends a type with an `<xs:anyAttribute>` therefore has that
//! wildcard, and a restriction of it may keep (or narrow) it:
//! derivation-ok-restriction compares the derived wildcard with the base's
//! {attribute wildcard}, not with the wildcard the base declares itself.
//!
//! Every case runs in XSD 1.0 and XSD 1.1 mode; instances run on streaming
//! validation with and without PSVI typed values and on the navigator driver.

use xsd_schema::validation::{
    drive_navigator, drive_quick_xml, CollectingValidationSink, SchemaValidator, SchemaValidity,
    ValidationFlags,
};
use xsd_schema::{RoXmlNavigator, SchemaResult, SchemaSet, SchemaSetBuilder, XsdVersion};

use SchemaValidity::{Invalid, Valid};

const VERSIONS: [XsdVersion; 2] = [XsdVersion::V1_0, XsdVersion::V1_1];

// ── Harness ───────────────────────────────────────────────────────────────

/// A `urn:b` schema document (prefix `b`) with `body`.
fn compile(version: XsdVersion, body: &str) -> SchemaResult<SchemaSet> {
    let xsd = format!(
        r###"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:b="urn:b" targetNamespace="urn:b">{body}</xs:schema>"###
    );
    SchemaSetBuilder::with_version(version)
        .add_source(&xsd, "file:///attribute_wildcard_extension.xsd")?
        .compile()
        .map(|compiled| compiled.into_schema_set())
}

fn schema(version: XsdVersion, body: &str) -> SchemaSet {
    compile(version, body).unwrap_or_else(|e| panic!("{version:?}: schema compiles: {e}"))
}

/// The schema is rejected with a `derivation-ok-restriction` error.
fn rejected(version: XsdVersion, body: &str) -> String {
    let err = match compile(version, body) {
        Ok(_) => panic!("{version:?}: expected derivation-ok-restriction, schema compiled"),
        Err(e) => e.to_string(),
    };
    assert!(
        err.contains("derivation-ok-restriction"),
        "{version:?}: expected derivation-ok-restriction, got: {err}"
    );
    err
}

fn validity(ss: &SchemaSet, xml: &str, driver: &str) -> Option<SchemaValidity> {
    let flags = match driver {
        "stream/no-psvi" => ValidationFlags::default() & !ValidationFlags::BUILD_PSVI_TYPED_VALUES,
        _ => ValidationFlags::default(),
    };
    let v = SchemaValidator::new(ss, flags);
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut rt = v.start_run(CollectingValidationSink {
        errors: &mut errors,
        warnings: &mut warnings,
    });
    if driver == "navigator" {
        let doc = roxmltree::Document::parse(xml).expect("well-formed instance");
        let nav = RoXmlNavigator::new(&doc);
        drive_navigator(&nav, &mut rt, ss)
            .expect("navigator drive completes")
            .root_validity
    } else {
        drive_quick_xml(xml.as_bytes(), &mut rt, ss)
            .expect("stream drive completes")
            .root_validity
    }
}

fn check(ss: &SchemaSet, label: &str, xml: &str, expected: SchemaValidity) {
    for driver in ["stream", "stream/no-psvi", "navigator"] {
        assert_eq!(
            validity(ss, xml, driver),
            Some(expected),
            "{label} {driver}: {xml}"
        );
    }
}

// ── The reported case ─────────────────────────────────────────────────────

/// `Generic` declares `##other`; `Ext` extends it and declares no wildcard, so
/// its {attribute wildcard} is `Generic`'s (clause 2.2.2.2); `Res` restricts
/// `Ext` with the same wildcard. The base side used to be computed from
/// `Ext`'s own `<anyAttribute>` only: "derived type has an attribute wildcard
/// but the base type does not".
#[test]
fn a_restriction_keeps_the_wildcard_its_base_got_by_extension() {
    let body = r###"
      <xs:complexType name="Generic"><xs:anyAttribute namespace="##other" processContents="lax"/></xs:complexType>
      <xs:complexType name="Ext"><xs:complexContent><xs:extension base="b:Generic">
        <xs:attribute name="v" type="xs:decimal"/>
      </xs:extension></xs:complexContent></xs:complexType>
      <xs:complexType name="Res"><xs:complexContent><xs:restriction base="b:Ext">
        <xs:attribute name="v" type="xs:decimal"/>
        <xs:anyAttribute namespace="##other" processContents="lax"/>
      </xs:restriction></xs:complexContent></xs:complexType>
      <xs:element name="r" type="b:Res"/>
      <xs:element name="e" type="b:Ext"/>"###;
    for version in VERSIONS {
        let ss = schema(version, body);
        let label = format!("{version:?}");
        check(
            &ss,
            &label,
            r###"<b:r xmlns:b="urn:b" xmlns:o="urn:o" v="1" o:x="1"/>"###,
            Valid,
        );
        check(&ss, &label, r###"<b:r xmlns:b="urn:b" x="1"/>"###, Invalid);
        check(
            &ss,
            &label,
            r###"<b:r xmlns:b="urn:b" b:x="1"/>"###,
            Invalid,
        );
        // The extension itself admits the same foreign attribute.
        check(
            &ss,
            &label,
            r###"<b:e xmlns:b="urn:b" xmlns:o="urn:o" o:x="1"/>"###,
            Valid,
        );
    }
}

/// The same shape with the wildcard in attribute groups, two levels of
/// restriction and a further extension (the layout of a published W3C schema
/// that hit this).
#[test]
fn wildcards_from_attribute_groups_through_extension_and_restriction() {
    let body = r###"
      <xs:attributeGroup name="common">
        <xs:attribute name="note" type="xs:string"/>
        <xs:anyAttribute namespace="##other" processContents="lax"/>
      </xs:attributeGroup>
      <xs:attributeGroup name="common-and-version">
        <xs:attributeGroup ref="b:common"/>
        <xs:attribute name="version" type="xs:decimal" use="optional"/>
      </xs:attributeGroup>
      <xs:complexType name="generic" mixed="true">
        <xs:attributeGroup ref="b:common"/>
      </xs:complexType>
      <xs:complexType name="versioned" mixed="true"><xs:complexContent>
        <xs:extension base="b:generic">
          <xs:attribute name="version" type="xs:decimal" use="optional"/>
        </xs:extension>
      </xs:complexContent></xs:complexType>
      <xs:complexType name="element-only" mixed="false"><xs:complexContent>
        <xs:restriction base="b:versioned">
          <xs:attributeGroup ref="b:common-and-version"/>
        </xs:restriction>
      </xs:complexContent></xs:complexType>
      <xs:complexType name="top-base"><xs:complexContent>
        <xs:restriction base="b:element-only">
          <xs:attribute name="version" type="xs:decimal" use="required"/>
          <xs:anyAttribute namespace="##other" processContents="lax"/>
        </xs:restriction>
      </xs:complexContent></xs:complexType>
      <xs:complexType name="text-base"><xs:simpleContent>
        <xs:restriction base="b:versioned">
          <xs:simpleType><xs:restriction base="xs:string"/></xs:simpleType>
          <xs:attributeGroup ref="b:common-and-version"/>
        </xs:restriction>
      </xs:simpleContent></xs:complexType>
      <xs:element name="top"><xs:complexType><xs:complexContent>
        <xs:extension base="b:top-base">
          <xs:sequence><xs:element name="item" minOccurs="0" maxOccurs="unbounded" type="b:element-only"/></xs:sequence>
        </xs:extension>
      </xs:complexContent></xs:complexType></xs:element>
      <xs:element name="text"><xs:complexType><xs:simpleContent>
        <xs:extension base="b:text-base"><xs:attribute name="flag" type="xs:boolean"/></xs:extension>
      </xs:simpleContent></xs:complexType></xs:element>"###;
    for version in VERSIONS {
        let ss = schema(version, body);
        let label = format!("{version:?}");
        check(
            &ss,
            &label,
            r###"<b:top xmlns:b="urn:b" xmlns:o="urn:o" version="2.0" o:x="1"><item note="n" o:y="2"/></b:top>"###,
            Valid,
        );
        // `version` is required on `top`.
        check(&ss, &label, r###"<b:top xmlns:b="urn:b"/>"###, Invalid);
        // An unqualified attribute is not `##other`.
        check(
            &ss,
            &label,
            r###"<b:top xmlns:b="urn:b" version="2.0"><item x="1"/></b:top>"###,
            Invalid,
        );
        check(
            &ss,
            &label,
            r###"<b:text xmlns:b="urn:b" xmlns:o="urn:o" flag="true" o:x="1">t</b:text>"###,
            Valid,
        );
    }
}

/// Two extension steps before the restriction: the wildcard is still the one
/// `Generic` declared.
#[test]
fn the_wildcard_passes_through_several_extensions() {
    let body = r###"
      <xs:complexType name="Generic"><xs:anyAttribute namespace="urn:x urn:y" processContents="strict"/></xs:complexType>
      <xs:complexType name="Ext1"><xs:complexContent><xs:extension base="b:Generic"/></xs:complexContent></xs:complexType>
      <xs:complexType name="Ext2"><xs:complexContent><xs:extension base="b:Ext1">
        <xs:attribute name="a" type="xs:string"/>
      </xs:extension></xs:complexContent></xs:complexType>
      <xs:complexType name="Res"><xs:complexContent><xs:restriction base="b:Ext2">
        <xs:anyAttribute namespace="urn:x" processContents="strict"/>
      </xs:restriction></xs:complexContent></xs:complexType>
      <xs:element name="r" type="b:Res"/>"###;
    for version in VERSIONS {
        schema(version, body);
    }
}

// ── The subset check, both directions ─────────────────────────────────────

/// A restriction may not widen the inherited wildcard, nor add one where the
/// whole chain has none (derivation-ok-restriction: 1.0 clause 4.1 / 4.2,
/// 1.1 clause 3).
#[test]
fn a_restriction_cannot_widen_or_add_the_inherited_wildcard() {
    let chain = r###"
      <xs:complexType name="Generic"><xs:anyAttribute namespace="##other" processContents="lax"/></xs:complexType>
      <xs:complexType name="Ext"><xs:complexContent><xs:extension base="b:Generic"/></xs:complexContent></xs:complexType>
      <xs:complexType name="Plain"/>
      <xs:complexType name="PlainExt"><xs:complexContent><xs:extension base="b:Plain"/></xs:complexContent></xs:complexType>"###;
    for version in VERSIONS {
        // Narrower: fine.
        schema(
            version,
            &format!(
                r###"{chain}<xs:complexType name="Res"><xs:complexContent><xs:restriction base="b:Ext">
                     <xs:anyAttribute namespace="urn:o" processContents="lax"/>
                   </xs:restriction></xs:complexContent></xs:complexType>"###
            ),
        );
        // Wider (`##any` admits unqualified and `urn:b` attributes).
        let err = rejected(
            version,
            &format!(
                r###"{chain}<xs:complexType name="Res"><xs:complexContent><xs:restriction base="b:Ext">
                     <xs:anyAttribute namespace="##any" processContents="lax"/>
                   </xs:restriction></xs:complexContent></xs:complexType>"###
            ),
        );
        assert!(err.contains("not a subset"), "{version:?}: {err}");
        // Added where the chain has no wildcard at all.
        let err = rejected(
            version,
            &format!(
                r###"{chain}<xs:complexType name="Res"><xs:complexContent><xs:restriction base="b:PlainExt">
                     <xs:anyAttribute namespace="##other" processContents="lax"/>
                   </xs:restriction></xs:complexContent></xs:complexType>"###
            ),
        );
        assert!(err.contains("base type does not"), "{version:?}: {err}");
        // Weaker {process contents} than the inherited wildcard (1.0 clause 4.3).
        rejected(
            version,
            &format!(
                r###"{chain}<xs:complexType name="Res"><xs:complexContent><xs:restriction base="b:Ext">
                     <xs:anyAttribute namespace="##other" processContents="skip"/>
                   </xs:restriction></xs:complexContent></xs:complexType>"###
            ),
        );
    }
}

/// When the extension declares its own wildcard too, its {attribute wildcard}
/// is the union of the two namespace constraints (clause 2.2.2.3): a
/// restriction may keep the union, not more.
#[test]
fn the_base_side_is_the_extension_union() {
    let chain = r###"
      <xs:complexType name="Generic"><xs:anyAttribute namespace="urn:x" processContents="lax"/></xs:complexType>
      <xs:complexType name="Ext"><xs:complexContent><xs:extension base="b:Generic">
        <xs:anyAttribute namespace="urn:y" processContents="lax"/>
      </xs:extension></xs:complexContent></xs:complexType>"###;
    for version in VERSIONS {
        let ss = schema(
            version,
            &format!(
                r###"{chain}<xs:complexType name="Res"><xs:complexContent><xs:restriction base="b:Ext">
                     <xs:anyAttribute namespace="urn:x urn:y" processContents="lax"/>
                   </xs:restriction></xs:complexContent></xs:complexType>
                   <xs:element name="r" type="b:Res"/>"###
            ),
        );
        let label = format!("{version:?}");
        check(
            &ss,
            &label,
            r###"<b:r xmlns:b="urn:b" xmlns:x="urn:x" xmlns:y="urn:y" x:a="1" y:a="2"/>"###,
            Valid,
        );
        check(
            &ss,
            &label,
            r###"<b:r xmlns:b="urn:b" xmlns:z="urn:z" z:a="1"/>"###,
            Invalid,
        );
        let err = rejected(
            version,
            &format!(
                r###"{chain}<xs:complexType name="Res"><xs:complexContent><xs:restriction base="b:Ext">
                     <xs:anyAttribute namespace="urn:x urn:z" processContents="lax"/>
                   </xs:restriction></xs:complexContent></xs:complexType>"###
            ),
        );
        assert!(err.contains("not a subset"), "{version:?}: {err}");
    }
}

/// Clause 2.2.2.3 (1.1) / 3.2.2.1.2 (1.0): the union takes the {process
/// contents} of the ·complete wildcard·, the extension's own — not the weaker
/// of the two. An extension declaring `strict` over a `skip` base wildcard
/// is strict: an undeclared attribute it admits is invalid, and a restriction
/// with `lax` is weaker than the base (1.0 clause 4.3).
#[test]
fn the_extension_union_keeps_the_process_contents_of_the_complete_wildcard() {
    let chain = r###"
      <xs:complexType name="Generic"><xs:anyAttribute namespace="##any" processContents="skip"/></xs:complexType>
      <xs:complexType name="Ext"><xs:complexContent><xs:extension base="b:Generic">
        <xs:anyAttribute namespace="##any" processContents="strict"/>
      </xs:extension></xs:complexContent></xs:complexType>
      <xs:complexType name="Loose"><xs:complexContent><xs:extension base="b:Generic">
        <xs:anyAttribute namespace="##any" processContents="skip"/>
      </xs:extension></xs:complexContent></xs:complexType>
      <xs:element name="e" type="b:Ext"/>
      <xs:element name="l" type="b:Loose"/>"###;
    for version in VERSIONS {
        let ss = schema(version, chain);
        let label = format!("{version:?}");
        // `o:x` has no declaration: `strict` makes it invalid.
        check(
            &ss,
            &label,
            r###"<b:e xmlns:b="urn:b" xmlns:o="urn:o" o:x="1"/>"###,
            Invalid,
        );
        check(
            &ss,
            &label,
            r###"<b:l xmlns:b="urn:b" xmlns:o="urn:o" o:x="1"/>"###,
            Valid,
        );
        rejected(
            version,
            &format!(
                r###"{chain}<xs:complexType name="Res"><xs:complexContent><xs:restriction base="b:Ext">
                     <xs:anyAttribute namespace="##any" processContents="lax"/>
                   </xs:restriction></xs:complexContent></xs:complexType>"###
            ),
        );
    }
}

// ── xs:anyType as the base (fix round R1, R2) ─────────────────────────────

/// XSD 1.0 Derivation Valid (Restriction, Complex) clause 4.3: "Unless the
/// {base type definition} is the ·ur-type definition·, the complex type
/// definition's {attribute wildcard}'s {process contents} must be identical to
/// or stronger than the {base type definition}'s". A restriction of
/// `xs:anyType` written out (`<xs:restriction base="xs:anyType">`) is the same
/// component as the shorthand `<xs:complexType>` (Structures 1.1 §3.4.2.6: "at
/// the component level, the following declaration is equivalent to the
/// preceding one"), so every {process contents} compiles in both spellings and
/// both versions, and instances see the declared {process contents}.
#[test]
fn a_restriction_of_any_type_may_have_any_process_contents() {
    for process_contents in ["skip", "lax", "strict"] {
        let wildcard = format!(
            r###"<xs:anyAttribute namespace="##other" processContents="{process_contents}"/>"###
        );
        let explicit = format!(
            r###"<xs:complexType name="R"><xs:complexContent><xs:restriction base="xs:anyType">
                   {wildcard}
                 </xs:restriction></xs:complexContent></xs:complexType>
                 <xs:element name="r" type="b:R"/>"###
        );
        let shorthand = format!(
            r###"<xs:complexType name="R">{wildcard}</xs:complexType>
                 <xs:element name="r" type="b:R"/>"###
        );
        for version in VERSIONS {
            for (spelling, body) in [("explicit", &explicit), ("shorthand", &shorthand)] {
                let ss = schema(version, body);
                let label = format!("{version:?} {spelling} {process_contents}");
                // An undeclared foreign attribute: valid unless strict.
                let expected = if process_contents == "strict" {
                    Invalid
                } else {
                    Valid
                };
                check(
                    &ss,
                    &label,
                    r###"<b:r xmlns:b="urn:b" xmlns:o="urn:o" o:x="1"/>"###,
                    expected,
                );
                // Unqualified attributes are not `##other`.
                check(&ss, &label, r###"<b:r xmlns:b="urn:b" x="1"/>"###, Invalid);
            }
        }
    }
}

/// Structures 1.1 §3.4.2.5 clause 2.2.1.1 (XSD 1.0 §3.4.2 clause 3.2.1.1): the
/// ·base wildcard· of an extension is the {attribute wildcard} of its base, and
/// `xs:anyType` has one — §3.4.7: {attribute wildcard} "a wildcard … {variety}
/// any … {process contents} lax". So `<xs:extension base="xs:anyType">` admits
/// any attribute laxly (or, with a wildcard of its own, the union with that
/// wildcard's {process contents}), and a restriction of it may keep a wildcard.
#[test]
fn an_extension_of_any_type_has_its_wildcard() {
    let body = r###"
      <xs:complexType name="E"><xs:complexContent><xs:extension base="xs:anyType">
        <xs:attribute name="a" type="xs:integer"/>
      </xs:extension></xs:complexContent></xs:complexType>
      <xs:complexType name="S"><xs:complexContent><xs:extension base="xs:anyType">
        <xs:anyAttribute namespace="##other" processContents="strict"/>
      </xs:extension></xs:complexContent></xs:complexType>
      <xs:complexType name="R"><xs:complexContent><xs:restriction base="b:E">
        <xs:attribute name="a" type="xs:integer"/>
        <xs:anyAttribute namespace="##other" processContents="lax"/>
      </xs:restriction></xs:complexContent></xs:complexType>
      <xs:attribute name="g" type="xs:integer"/>
      <xs:element name="e" type="b:E"/>
      <xs:element name="s" type="b:S"/>
      <xs:element name="r" type="b:R"/>"###;
    for version in VERSIONS {
        let ss = schema(version, body);
        let label = format!("{version:?}");
        // `##any`, lax: undeclared attributes in any namespace or none.
        check(
            &ss,
            &label,
            r###"<b:e xmlns:b="urn:b" xmlns:o="urn:o" a="1" o:y="2" z="3"/>"###,
            Valid,
        );
        // A declared one is validated (lax finds the declaration).
        check(
            &ss,
            &label,
            r###"<b:e xmlns:b="urn:b" b:g="x"/>"###,
            Invalid,
        );
        check(&ss, &label, r###"<b:e xmlns:b="urn:b" a="x"/>"###, Invalid);
        // The union with the extension's own wildcard is `##any`, strict.
        check(
            &ss,
            &label,
            r###"<b:s xmlns:b="urn:b" xmlns:o="urn:o" o:y="2"/>"###,
            Invalid,
        );
        check(&ss, &label, r###"<b:s xmlns:b="urn:b" b:g="2"/>"###, Valid);
        // The restriction keeps a narrower lax wildcard.
        check(
            &ss,
            &label,
            r###"<b:r xmlns:b="urn:b" xmlns:o="urn:o" a="1" o:y="2"/>"###,
            Valid,
        );
        check(&ss, &label, r###"<b:r xmlns:b="urn:b" z="3"/>"###, Invalid);
        // `skip` is weaker than the inherited lax wildcard: the base is not
        // `xs:anyType` itself, so 1.0 clause 4.3 applies.
        let err = rejected(
            version,
            &format!(
                r###"{body}<xs:complexType name="W"><xs:complexContent><xs:restriction base="b:E">
                     <xs:anyAttribute namespace="##other" processContents="skip"/>
                   </xs:restriction></xs:complexContent></xs:complexType>"###
            ),
        );
        assert!(err.contains("processContents"), "{version:?}: {err}");
    }
}
