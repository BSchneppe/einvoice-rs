//! `##other` and `##targetNamespace` belong to the wildcard's own schema document.
//!
//! Structures §3.10.2.2 maps `namespace="##other"` to the {namespaces} "a set
//! consisting of ·absent· and, if the targetNamespace [attribute] of the
//! `<schema>` ancestor element information item is present, its ·actual
//! value·", and a `##targetNamespace` member to "the ·actual value· of the
//! targetNamespace [attribute] of the `<schema>` ancestor element
//! information item if present, otherwise ·absent·". A wildcard keeps that
//! meaning wherever it is used: in a type of another namespace that extends
//! the wildcard's type, or that refers to a model group declaring it.
//!
//! Such wildcards used to be evaluated against the target namespace of the
//! type whose content model they ended up in. That rejected valid instances,
//! accepted invalid ones (an inherited `##targetNamespace` wildcard admitted
//! the deriving namespace instead of its own), reported false UPA conflicts
//! and missed real ones, and rejected a valid open-content extension. Every
//! case runs in XSD 1.0 and XSD 1.1 mode where the schema is valid in both.

use xsd_schema::validation::{
    drive_quick_xml, CollectingValidationSink, SchemaValidator, SchemaValidity, ValidationFlags,
};
use xsd_schema::{SchemaResult, SchemaSet, SchemaSetBuilder};

/// Compile schema documents given as `(base URI, text)`; documents import
/// each other's namespace without a schema location.
fn compile(documents: &[(&str, &str)], xsd11: bool) -> SchemaResult<SchemaSet> {
    let mut builder = if xsd11 {
        SchemaSetBuilder::xsd11()
    } else {
        SchemaSetBuilder::new()
    };
    for (uri, text) in documents {
        builder = builder.add_source(text, uri)?;
    }
    builder.compile().map(|compiled| compiled.into_schema_set())
}

fn validity(schema_set: &SchemaSet, xml: &str) -> Option<SchemaValidity> {
    let validator = SchemaValidator::new(schema_set, ValidationFlags::default());
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let sink = CollectingValidationSink {
        errors: &mut errors,
        warnings: &mut warnings,
    };
    let mut runtime = validator.start_run(sink);
    drive_quick_xml(xml.as_bytes(), &mut runtime, schema_set)
        .expect("driver completes")
        .root_validity
}

fn check(set: &SchemaSet, label: &str, xml: &str, expected: SchemaValidity) {
    assert_eq!(validity(set, xml), Some(expected), "{label}: {xml}");
}

/// `urn:a` schema document with `extra` as its top-level content.
fn doc_a(extra: &str) -> String {
    format!(
        r###"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:a="urn:a" targetNamespace="urn:a" elementFormDefault="qualified">
{extra}
</xs:schema>"###
    )
}

/// `urn:b` schema document importing `urn:a`, with `extra` as its content.
fn doc_b(extra: &str) -> String {
    format!(
        r###"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:a="urn:a" xmlns:b="urn:b" targetNamespace="urn:b" elementFormDefault="qualified">
  <xs:import namespace="urn:a"/>
{extra}
</xs:schema>"###
    )
}

/// `Base = (any ##other*, T)` in `urn:a` — the S0 reproducer `a.xsd`.
fn base_with(namespace: &str) -> String {
    doc_a(&format!(
        r###"  <xs:complexType name="Base">
    <xs:sequence>
      <xs:any namespace="{namespace}" processContents="lax" minOccurs="0" maxOccurs="unbounded"/>
      <xs:element name="T" type="xs:string"/>
    </xs:sequence>
  </xs:complexType>"###
    ))
}

const B_EXTENDS_BASE: &str = r###"  <xs:complexType name="D">
    <xs:complexContent><xs:extension base="a:Base"/></xs:complexContent>
  </xs:complexType>
  <xs:element name="r" type="b:D"/>"###;

#[test]
fn inherited_other_wildcard_keeps_its_namespace() {
    let a = base_with("##other");
    let b = doc_b(B_EXTENDS_BASE);
    for xsd11 in [false, true] {
        let set = compile(&[("file:///a.xsd", &a), ("file:///b.xsd", &b)], xsd11)
            .unwrap_or_else(|e| panic!("xsd11={xsd11}: {e}"));
        let label = format!("xsd11={xsd11}");
        // urn:b is "other" for the wildcard of urn:a.
        check(
            &set,
            &label,
            r###"<b:r xmlns:b="urn:b" xmlns:a="urn:a"><b:other/><a:T>t</a:T></b:r>"###,
            SchemaValidity::Valid,
        );
        // urn:a is not.
        check(
            &set,
            &label,
            r###"<b:r xmlns:b="urn:b" xmlns:a="urn:a"><a:x/><a:T>t</a:T></b:r>"###,
            SchemaValidity::Invalid,
        );
    }
}

/// The UPA check of the extension: `a:T` can never match `##other` of `urn:a`,
/// so `(any ##other*, a:T, b:E?)` is deterministic (S0 reproducer `b3.xsd`).
#[test]
fn inherited_other_wildcard_is_not_a_false_upa_conflict() {
    let a = base_with("##other");
    let b = doc_b(
        r###"  <xs:complexType name="D">
    <xs:complexContent><xs:extension base="a:Base">
      <xs:sequence><xs:element name="E" type="xs:string" minOccurs="0"/></xs:sequence>
    </xs:extension></xs:complexContent>
  </xs:complexType>
  <xs:element name="r" type="b:D"/>"###,
    );
    for xsd11 in [false, true] {
        let set = compile(&[("file:///a.xsd", &a), ("file:///b.xsd", &b)], xsd11)
            .unwrap_or_else(|e| panic!("xsd11={xsd11}: {e}"));
        check(
            &set,
            &format!("xsd11={xsd11}"),
            r###"<b:r xmlns:b="urn:b" xmlns:a="urn:a" xmlns:c="urn:c"><c:x/><a:T>t</a:T><b:E>e</b:E></b:r>"###,
            SchemaValidity::Valid,
        );
    }
}

/// An inherited `##targetNamespace` wildcard admits its own namespace and not
/// the deriving one. (In XSD 1.0 `Base` itself violates UPA: the wildcard and
/// `a:T` compete — S0 reproducers `at.xsd` / `bt.xsd`.)
#[test]
fn inherited_target_namespace_wildcard_keeps_its_namespace() {
    let a = base_with("##targetNamespace");
    let b = doc_b(B_EXTENDS_BASE);
    let set = compile(&[("file:///a.xsd", &a), ("file:///b.xsd", &b)], true)
        .unwrap_or_else(|e| panic!("xsd11: {e}"));
    check(
        &set,
        "xsd11",
        r###"<b:r xmlns:b="urn:b" xmlns:a="urn:a"><a:x/><a:T>t</a:T></b:r>"###,
        SchemaValidity::Valid,
    );
    // The validator used to accept this one.
    check(
        &set,
        "xsd11",
        r###"<b:r xmlns:b="urn:b" xmlns:a="urn:a"><b:other/><a:T>t</a:T></b:r>"###,
        SchemaValidity::Invalid,
    );

    let err = compile(&[("file:///a.xsd", &a), ("file:///b.xsd", &b)], false)
        .expect_err("XSD 1.0: a:Base is ambiguous");
    assert!(err.to_string().contains("cos-nonambig"), "{err}");
}

/// A wildcard reached through a model group of another namespace.
#[test]
fn wildcard_in_a_model_group_of_another_namespace_keeps_its_namespace() {
    let a = doc_a(
        r###"  <xs:group name="G">
    <xs:sequence>
      <xs:any namespace="##other" processContents="lax" minOccurs="0" maxOccurs="unbounded"/>
    </xs:sequence>
  </xs:group>"###,
    );
    let b = doc_b(
        r###"  <xs:complexType name="R"><xs:sequence><xs:group ref="a:G"/><xs:element name="E"/></xs:sequence></xs:complexType>
  <xs:element name="r" type="b:R"/>"###,
    );
    let documents = [("file:///a.xsd", a.as_str()), ("file:///b.xsd", b.as_str())];

    // XSD 1.0: `##other` of urn:a matches b:E, which follows the repeated
    // wildcard — a UPA violation (it used to be read as `##other` of urn:b).
    let err = compile(&documents, false).expect_err("XSD 1.0: wildcard competes with b:E");
    assert!(err.to_string().contains("cos-nonambig"), "{err}");

    // XSD 1.1 allows the competition (§3.8.6.4); the namespaces stay right.
    let set = compile(&documents, true).unwrap_or_else(|e| panic!("xsd11: {e}"));
    check(
        &set,
        "xsd11",
        r###"<b:r xmlns:b="urn:b" xmlns:c="urn:c"><c:x/><b:E/></b:r>"###,
        SchemaValidity::Valid,
    );
    check(
        &set,
        "xsd11",
        r###"<b:r xmlns:b="urn:b"><b:x/><b:E/></b:r>"###,
        SchemaValidity::Valid,
    );
    check(
        &set,
        "xsd11",
        r###"<b:r xmlns:b="urn:b" xmlns:a="urn:a"><a:x/><b:E/></b:r>"###,
        SchemaValidity::Invalid,
    );
}

/// Restriction of a wildcard (rcase-NSSubset / cos-ns-subset): each side is
/// read against its own document.
#[test]
fn wildcard_restriction_across_namespaces() {
    let a = doc_a(
        r###"  <xs:complexType name="Base2">
    <xs:sequence>
      <xs:any namespace="##other" processContents="lax" minOccurs="0" maxOccurs="unbounded"/>
    </xs:sequence>
  </xs:complexType>
  <xs:group name="G">
    <xs:sequence>
      <xs:any namespace="##other" processContents="lax" minOccurs="0" maxOccurs="unbounded"/>
    </xs:sequence>
  </xs:group>
  <xs:group name="GT">
    <xs:sequence>
      <xs:any namespace="##targetNamespace" processContents="lax" minOccurs="0" maxOccurs="unbounded"/>
    </xs:sequence>
  </xs:group>"###,
    );
    let restriction = |content: &str| {
        doc_b(&format!(
            r###"  <xs:complexType name="D"><xs:complexContent><xs:restriction base="a:Base2">{content}</xs:restriction></xs:complexContent></xs:complexType>"###
        ))
    };
    let any = |namespace: &str| {
        format!(
            r###"<xs:sequence><xs:any namespace="{namespace}" processContents="lax" minOccurs="0" maxOccurs="unbounded"/></xs:sequence>"###
        )
    };
    let cases = [
        // not(urn:b, absent) admits urn:a, which not(urn:a, absent) does not.
        (any("##other"), false),
        // {urn:b} ⊆ not(urn:a, absent).
        (any("##targetNamespace"), true),
        // {absent} ⊄ not(urn:a, absent).
        (any("##local"), false),
        // The base's own group: identical wildcard.
        (r###"<xs:group ref="a:G"/>"###.to_string(), true),
        // {urn:a} ⊄ not(urn:a, absent).
        (r###"<xs:group ref="a:GT"/>"###.to_string(), false),
    ];
    for xsd11 in [false, true] {
        for (content, valid) in &cases {
            let b = restriction(content);
            let result = compile(&[("file:///a.xsd", &a), ("file:///b.xsd", &b)], xsd11);
            assert_eq!(
                result.is_ok(),
                *valid,
                "xsd11={xsd11} `{content}`: {:?}",
                result.err()
            );
        }
    }
}

/// Attribute wildcards resolve each contributor against its own component
/// already; these pin that behaviour for an inherited `anyAttribute` and one
/// from an attribute group of another namespace.
#[test]
fn attribute_wildcards_keep_their_namespace() {
    let a = doc_a(
        r###"  <xs:complexType name="BaseA"><xs:anyAttribute namespace="##other" processContents="lax"/></xs:complexType>
  <xs:attributeGroup name="AG"><xs:anyAttribute namespace="##other" processContents="lax"/></xs:attributeGroup>"###,
    );
    let b = doc_b(
        r###"  <xs:complexType name="DA"><xs:complexContent><xs:extension base="a:BaseA"/></xs:complexContent></xs:complexType>
  <xs:complexType name="DG"><xs:attributeGroup ref="a:AG"/></xs:complexType>
  <xs:element name="ra" type="b:DA"/>
  <xs:element name="rg" type="b:DG"/>"###,
    );
    for xsd11 in [false, true] {
        let set = compile(&[("file:///a.xsd", &a), ("file:///b.xsd", &b)], xsd11)
            .unwrap_or_else(|e| panic!("xsd11={xsd11}: {e}"));
        let label = format!("xsd11={xsd11}");
        for element in ["ra", "rg"] {
            check(
                &set,
                &label,
                &format!(r###"<b:{element} xmlns:b="urn:b" b:x="1"/>"###),
                SchemaValidity::Valid,
            );
            check(
                &set,
                &label,
                &format!(r###"<b:{element} xmlns:b="urn:b" xmlns:a="urn:a" a:x="1"/>"###),
                SchemaValidity::Invalid,
            );
        }
    }

    // Restricting `##other` of urn:a by `##other` of urn:b widens it.
    let b = doc_b(
        r###"  <xs:complexType name="DR"><xs:complexContent><xs:restriction base="a:BaseA">
    <xs:anyAttribute namespace="##other" processContents="lax"/>
  </xs:restriction></xs:complexContent></xs:complexType>"###,
    );
    for xsd11 in [false, true] {
        assert!(
            compile(&[("file:///a.xsd", &a), ("file:///b.xsd", &b)], xsd11).is_err(),
            "xsd11={xsd11}"
        );
    }
}

/// XSD 1.1 open content and all groups inherited by an extension in another
/// namespace (§3.4.2.3.3 clause 6.1: the base's {open content} is inherited).
#[cfg(feature = "xsd11")]
#[test]
fn inherited_open_content_and_all_group_wildcards_keep_their_namespace() {
    let a = doc_a(
        r###"  <xs:complexType name="BaseOC">
    <xs:openContent mode="suffix"><xs:any namespace="##other" processContents="skip"/></xs:openContent>
    <xs:sequence><xs:element name="T"/></xs:sequence>
  </xs:complexType>
  <xs:complexType name="BaseAll">
    <xs:all><xs:element name="T"/><xs:any namespace="##other" processContents="skip"/></xs:all>
  </xs:complexType>"###,
    );
    let b = doc_b(
        r###"  <xs:complexType name="D"><xs:complexContent><xs:extension base="a:BaseOC"/></xs:complexContent></xs:complexType>
  <xs:complexType name="DA"><xs:complexContent><xs:extension base="a:BaseAll"><xs:all><xs:element name="E"/></xs:all></xs:extension></xs:complexContent></xs:complexType>
  <xs:element name="r" type="b:D"/>
  <xs:element name="ra" type="b:DA"/>"###,
    );
    // The open-content extension check (cos-ct-extends 1.4.3.2.2.4) used to
    // compare the inherited wildcard with itself read in urn:b and fail.
    let set = compile(&[("file:///a.xsd", &a), ("file:///b.xsd", &b)], true)
        .unwrap_or_else(|e| panic!("xsd11: {e}"));
    check(
        &set,
        "open content",
        r###"<b:r xmlns:b="urn:b" xmlns:a="urn:a"><a:T/><b:x/></b:r>"###,
        SchemaValidity::Valid,
    );
    check(
        &set,
        "open content",
        r###"<b:r xmlns:b="urn:b" xmlns:a="urn:a"><a:T/><a:y/></b:r>"###,
        SchemaValidity::Invalid,
    );
    check(
        &set,
        "all group",
        r###"<b:ra xmlns:b="urn:b" xmlns:a="urn:a"><b:x/><a:T/><b:E/></b:ra>"###,
        SchemaValidity::Valid,
    );
    check(
        &set,
        "all group",
        r###"<b:ra xmlns:b="urn:b" xmlns:a="urn:a"><a:y/><a:T/><b:E/></b:ra>"###,
        SchemaValidity::Invalid,
    );
}

/// Element Declarations Consistent with a wildcard (XSD 1.1 §3.8.6.3): a
/// local element whose expanded name a lax or strict wildcard of the same
/// content model admits must have a type table equivalent to the top-level
/// declaration's. The wildcard here comes from a model group of `urn:a`, so
/// its `##other` admits `b:E`.
#[cfg(feature = "xsd11")]
#[test]
fn wildcard_type_table_consistency_reads_the_wildcard_in_its_namespace() {
    let a = doc_a(
        r###"  <xs:group name="G"><xs:sequence><xs:any namespace="##other" processContents="lax" minOccurs="0"/></xs:sequence></xs:group>"###,
    );
    let b = doc_b(
        r###"  <xs:element name="E" type="xs:string"/>
  <xs:complexType name="R"><xs:sequence>
    <xs:element name="E" type="xs:string"><xs:alternative test="@x" type="xs:string"/></xs:element>
    <xs:group ref="a:G"/>
  </xs:sequence></xs:complexType>"###,
    );
    let err = compile(&[("file:///a.xsd", &a), ("file:///b.xsd", &b)], true)
        .expect_err("the wildcard admits b:E, whose type tables differ");
    assert!(err.to_string().contains("type table"), "{err}");
}

/// A wildcard's `##other` / `##targetNamespace` is read in the schema
/// document that declares it, found by the document id its components carry.
/// After `SourceMapStorage::drop_all` the ids of further documents were taken
/// from the (now empty) source-map storage, so every one of them claimed to be
/// document 0 and its wildcards were read in the first document's namespace:
/// `##other` of `urn:b` admitted `urn:b` and rejected `urn:a`.
#[test]
fn wildcards_keep_their_namespace_after_source_maps_are_dropped() {
    use xsd_schema::pipeline::{parse_schema_only, process_loaded_schemas};
    let a = doc_a(r###"  <xs:element name="x"/>"###);
    let b = r###"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:b" elementFormDefault="qualified">
  <xs:element name="x"/>
  <xs:element name="r"><xs:complexType><xs:sequence>
    <xs:any namespace="##other" processContents="lax"/>
  </xs:sequence></xs:complexType></xs:element>
</xs:schema>"###;
    let c = r###"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:c" elementFormDefault="qualified">
  <xs:element name="s"><xs:complexType><xs:sequence>
    <xs:any namespace="##targetNamespace" processContents="skip"/>
  </xs:sequence></xs:complexType></xs:element>
</xs:schema>"###;
    for xsd11 in [false, true] {
        for drop_maps in [false, true] {
            let label = format!("xsd11={xsd11} drop_all={drop_maps}");
            let mut set = if xsd11 {
                SchemaSet::xsd11()
            } else {
                SchemaSet::new()
            };
            if drop_maps {
                set.source_maps.drop_all();
            }
            let ids: Vec<_> = [
                ("file:///a.xsd", &a[..]),
                ("file:///b.xsd", b),
                ("file:///c.xsd", c),
            ]
            .iter()
            .map(|(uri, text)| {
                parse_schema_only(text.as_bytes(), uri, &mut set)
                    .unwrap_or_else(|e| panic!("{label}: {uri}: {e}"))
            })
            .collect();
            assert_eq!(ids, [0, 1, 2], "{label}: document ids");
            process_loaded_schemas(&mut set).unwrap_or_else(|e| panic!("{label}: {e}"));
            check(
                &set,
                &label,
                r###"<b:r xmlns:b="urn:b" xmlns:a="urn:a"><a:x/></b:r>"###,
                SchemaValidity::Valid,
            );
            check(
                &set,
                &label,
                r###"<b:r xmlns:b="urn:b"><b:x/></b:r>"###,
                SchemaValidity::Invalid,
            );
            check(
                &set,
                &label,
                r###"<c:s xmlns:c="urn:c"><c:y/></c:s>"###,
                SchemaValidity::Valid,
            );
            check(
                &set,
                &label,
                r###"<c:s xmlns:c="urn:c" xmlns:a="urn:a"><a:x/></c:s>"###,
                SchemaValidity::Invalid,
            );
        }
    }
}
