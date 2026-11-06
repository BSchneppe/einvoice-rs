//! Content models must not accept child sequences their particles forbid.
//!
//! Two compiler defects let instance validation accept invalid documents:
//!
//! * Kleene star and plus drew their loop edge between the body's own start
//!   and end states. Combinators attach edges to those boundary states — the
//!   bypass of an optional group, the link of a sequence, an enclosing loop —
//!   which then entered or left the loop: `(X, Y*)?, Z` accepted `<Y/><Z/>`
//!   although `Y` may only follow `X`. A real schema hit it: GAEB DA XML 3.3
//!   DA85 `tgItem`, `choice{0,1}(sequence(Qty, QtySplit*))`.
//! * A particle with minOccurs=maxOccurs=0 "maps to no component at all"
//!   (Structures §3.3.2, §3.7.2, §3.8.2, §3.10.2), but inside a choice it
//!   compiled to an empty branch: `choice(e1{0,0}, e2)` accepted no children
//!   although `e2` is required.
//!
//! Every case runs in XSD 1.0 and XSD 1.1 mode through the streaming driver.

use xsd_schema::validation::{
    drive_quick_xml, CollectingValidationSink, SchemaValidator, SchemaValidity, ValidationFlags,
};
use xsd_schema::{SchemaSet, SchemaSetBuilder};

fn schema(xsd: &str, xsd11: bool) -> SchemaSet {
    let builder = if xsd11 {
        SchemaSetBuilder::xsd11()
    } else {
        SchemaSetBuilder::new()
    };
    builder
        .add_source(xsd, "file:///content_model_soundness.xsd")
        .expect("schema source accepted")
        .compile()
        .expect("schema compiles")
        .into_schema_set()
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

/// `<root>` whose content is `body` inside a top-level `xs:sequence`.
fn root_schema(body: &str) -> String {
    format!(
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="root"><xs:complexType><xs:sequence>{body}</xs:sequence></xs:complexType></xs:element>
</xs:schema>"#
    )
}

fn check(label: &str, xsd: &str, valid: &[&str], invalid: &[&str]) {
    for xsd11 in [false, true] {
        let set = schema(xsd, xsd11);
        let version = if xsd11 { "1.1" } else { "1.0" };
        for xml in valid {
            assert_eq!(
                validity(&set, xml),
                Some(SchemaValidity::Valid),
                "{label} [{version}]: {xml} must be valid"
            );
        }
        for xml in invalid {
            assert_eq!(
                validity(&set, xml),
                Some(SchemaValidity::Invalid),
                "{label} [{version}]: {xml} must be invalid"
            );
        }
    }
}

const X: &str = r#"<xs:element name="X"/>"#;
const Y_STAR: &str = r#"<xs:element name="Y" minOccurs="0" maxOccurs="unbounded"/>"#;
const Y_PLUS: &str = r#"<xs:element name="Y" maxOccurs="unbounded"/>"#;
const Z: &str = r#"<xs:element name="Z"/>"#;

#[test]
fn loop_inside_optional_group_is_not_entered_by_the_bypass() {
    let xsd = root_schema(&format!(
        r#"<xs:sequence minOccurs="0">{X}{Y_STAR}</xs:sequence>{Z}"#
    ));
    check(
        "(X, Y*)?, Z",
        &xsd,
        &[
            "<root><Z/></root>",
            "<root><X/><Z/></root>",
            "<root><X/><Y/><Y/><Z/></root>",
        ],
        &["<root><Y/><Z/></root>", "<root><Y/><X/><Z/></root>"],
    );
}

#[test]
fn plus_inside_optional_group_is_not_entered_by_the_bypass() {
    let xsd = root_schema(&format!(
        r#"<xs:sequence minOccurs="0">{X}{Y_PLUS}</xs:sequence>{Z}"#
    ));
    check(
        "(X, Y+)?, Z",
        &xsd,
        &["<root><Z/></root>", "<root><X/><Y/><Z/></root>"],
        &["<root><Y/><Z/></root>", "<root><X/><Z/></root>"],
    );
}

#[test]
fn loop_at_the_start_of_an_optional_group_cannot_skip_the_rest() {
    let xsd = root_schema(&format!(
        r#"<xs:sequence minOccurs="0">{Y_STAR}{X}</xs:sequence>{Z}"#
    ));
    check(
        "(Y*, X)?, Z",
        &xsd,
        &[
            "<root><Z/></root>",
            "<root><X/><Z/></root>",
            "<root><Y/><Y/><X/><Z/></root>",
        ],
        &["<root><Y/><Z/></root>", "<root><Y/><Y/><Z/></root>"],
    );
}

#[test]
fn loop_inside_repeated_groups() {
    let unrolled = root_schema(&format!(
        r#"<xs:sequence minOccurs="0" maxOccurs="3">{X}{Y_STAR}</xs:sequence>{Z}"#
    ));
    check(
        "(X, Y*){{0,3}}, Z",
        &unrolled,
        &["<root><Z/></root>", "<root><X/><Y/><X/><Z/></root>"],
        &["<root><Y/><Z/></root>", "<root><X/><X/><X/><X/><Z/></root>"],
    );

    // Above the unroll threshold the group becomes a counted loop.
    let counted = root_schema(&format!(
        r#"<xs:sequence minOccurs="0" maxOccurs="20">{X}{Y_STAR}</xs:sequence>{Z}"#
    ));
    check(
        "(X, Y*){{0,20}}, Z",
        &counted,
        &["<root><Z/></root>", "<root><X/><Y/><X/><Z/></root>"],
        &["<root><Y/><Z/></root>"],
    );

    let starred = root_schema(&format!(
        r#"<xs:sequence minOccurs="0" maxOccurs="unbounded">{X}{Y_STAR}</xs:sequence>{Z}"#
    ));
    check(
        "(X, Y*)*, Z",
        &starred,
        &["<root><Z/></root>", "<root><X/><Y/><X/><Y/><Y/><Z/></root>"],
        &["<root><Y/><Z/></root>", "<root><Y/><X/><Z/></root>"],
    );

    let plussed = root_schema(&format!(
        r#"<xs:sequence maxOccurs="unbounded">{X}{Y_STAR}</xs:sequence>{Z}"#
    ));
    check(
        "(X, Y*)+, Z",
        &plussed,
        &["<root><X/><Z/></root>", "<root><X/><Y/><X/><Z/></root>"],
        &["<root><Y/><Z/></root>", "<root><Z/></root>"],
    );
}

#[test]
fn nested_loops_and_counted_prefixes_inside_optional_groups() {
    // A star of a starred group: two loops share no boundary state.
    let nested = root_schema(&format!(
        r#"<xs:sequence minOccurs="0">{X}<xs:sequence minOccurs="0" maxOccurs="unbounded">{Y_STAR}</xs:sequence></xs:sequence>{Z}"#
    ));
    check(
        "(X, (Y*)*)?, Z",
        &nested,
        &["<root><Z/></root>", "<root><X/><Y/><Y/><Z/></root>"],
        &["<root><Y/><Z/></root>"],
    );

    // minOccurs above the unroll threshold with no maximum compiles to a
    // counted prefix followed by a star.
    let counted_prefix = root_schema(&format!(
        r#"<xs:sequence minOccurs="0">{X}<xs:element name="Y" minOccurs="17" maxOccurs="unbounded"/></xs:sequence>{Z}"#
    ));
    let y17 = "<Y/>".repeat(17);
    let y18 = "<Y/>".repeat(18);
    let y16 = "<Y/>".repeat(16);
    check(
        "(X, Y{17,∞})?, Z",
        &counted_prefix,
        &[
            "<root><Z/></root>",
            &format!("<root><X/>{y17}<Z/></root>"),
            &format!("<root><X/>{y18}<Z/></root>"),
        ],
        &[
            &format!("<root>{y17}<Z/></root>"),
            &format!("<root><X/>{y16}<Z/></root>"),
        ],
    );

    // A plus at the start of an optional group cannot skip what follows it.
    let plus_first = root_schema(&format!(
        r#"<xs:sequence minOccurs="0">{Y_PLUS}{X}</xs:sequence>{Z}"#
    ));
    check(
        "(Y+, X)?, Z",
        &plus_first,
        &["<root><Z/></root>", "<root><Y/><Y/><X/><Z/></root>"],
        &["<root><Y/><Z/></root>", "<root><X/><Z/></root>"],
    );
}

/// The shape of GAEB DA XML 3.3 DA85 `tgItem`: `QtySplit` may only follow `Qty`.
#[test]
fn optional_choice_of_a_sequence_ending_in_a_loop() {
    let xsd = root_schema(
        r#"<xs:choice minOccurs="0">
             <xs:sequence>
               <xs:element name="Qty"/>
               <xs:element name="QtySplit" minOccurs="0" maxOccurs="unbounded"/>
             </xs:sequence>
           </xs:choice>
           <xs:element name="QU"/>"#,
    );
    check(
        "choice{0,1}(Qty, QtySplit*), QU",
        &xsd,
        &[
            "<root><QU/></root>",
            "<root><Qty/><QU/></root>",
            "<root><Qty/><QtySplit/><QtySplit/><QU/></root>",
        ],
        &["<root><QtySplit/><QU/></root>"],
    );
}

#[test]
fn controls_without_an_enclosing_optional_group() {
    check(
        "X, Y*, Z",
        &root_schema(&format!("{X}{Y_STAR}{Z}")),
        &["<root><X/><Z/></root>", "<root><X/><Y/><Z/></root>"],
        &["<root><Y/><Z/></root>"],
    );
    check(
        "(X, Y*), Z",
        &root_schema(&format!("<xs:sequence>{X}{Y_STAR}</xs:sequence>{Z}")),
        &["<root><X/><Y/><Z/></root>"],
        &["<root><Y/><Z/></root>"],
    );
}

/// `<doc>` whose content is the given `xs:choice` (or other model group).
fn doc_schema(content: &str) -> String {
    format!(
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:group name="g"><xs:sequence><xs:element name="g1"/></xs:sequence></xs:group>
  <xs:element name="doc"><xs:complexType>{content}</xs:complexType></xs:element>
</xs:schema>"#
    )
}

#[test]
fn zero_occurrence_element_is_not_a_choice_branch() {
    let xsd = doc_schema(
        r#"<xs:choice><xs:element name="e1" minOccurs="0" maxOccurs="0"/><xs:element name="e2"/></xs:choice>"#,
    );
    check(
        "choice(e1{0,0}, e2)",
        &xsd,
        &["<doc><e2/></doc>"],
        &["<doc/>", "<doc><e1/></doc>"],
    );
}

#[test]
fn zero_occurrence_group_ref_and_wildcard_are_not_choice_branches() {
    let group = doc_schema(
        r#"<xs:choice><xs:group ref="g" minOccurs="0" maxOccurs="0"/><xs:element name="e2"/></xs:choice>"#,
    );
    check(
        "choice(g{0,0}, e2)",
        &group,
        &["<doc><e2/></doc>"],
        &["<doc/>", "<doc><g1/></doc>"],
    );

    let wildcard = doc_schema(
        r#"<xs:choice><xs:any processContents="skip" minOccurs="0" maxOccurs="0"/><xs:element name="e2"/></xs:choice>"#,
    );
    check(
        "choice(any{0,0}, e2)",
        &wildcard,
        &["<doc><e2/></doc>"],
        &["<doc/>", "<doc><other/></doc>"],
    );
}

#[test]
fn choice_whose_only_branches_have_zero_occurrences() {
    // No particles remain: a required choice accepts nothing at all, not even
    // empty content; an optional one accepts only empty content.
    let required =
        doc_schema(r#"<xs:choice><xs:element name="e1" minOccurs="0" maxOccurs="0"/></xs:choice>"#);
    check(
        "choice(e1{0,0})",
        &required,
        &[],
        &["<doc/>", "<doc><e1/></doc>"],
    );

    let optional = doc_schema(
        r#"<xs:choice minOccurs="0"><xs:element name="e1" minOccurs="0" maxOccurs="0"/></xs:choice>"#,
    );
    check(
        "choice{0,1}(e1{0,0})",
        &optional,
        &["<doc/>"],
        &["<doc><e1/></doc>"],
    );
}

#[test]
fn only_the_zero_occurrence_particle_itself_disappears() {
    // The inner sequence keeps existing with no particles of its own, so it
    // matches empty content and the choice does accept `<doc/>`.
    let nested = doc_schema(
        r#"<xs:choice><xs:sequence><xs:element name="e1" minOccurs="0" maxOccurs="0"/></xs:sequence><xs:element name="e2"/></xs:choice>"#,
    );
    check(
        "choice(sequence(e1{0,0}), e2)",
        &nested,
        &["<doc/>", "<doc><e2/></doc>"],
        &["<doc><e1/></doc>"],
    );

    // In a sequence a zero-occurrence particle always behaved as absent.
    let sequence = doc_schema(
        r#"<xs:sequence><xs:element name="e1" minOccurs="0" maxOccurs="0"/><xs:element name="e2"/></xs:sequence>"#,
    );
    check(
        "sequence(e1{0,0}, e2)",
        &sequence,
        &["<doc><e2/></doc>"],
        &["<doc/>", "<doc><e1/><e2/></doc>"],
    );
}

/// A zero-occurrence element declares no sibling, so `##definedSibling` does
/// not exclude its name.
#[cfg(feature = "xsd11")]
#[test]
fn zero_occurrence_element_is_not_a_defined_sibling() {
    let xsd = doc_schema(
        r###"<xs:sequence>
             <xs:element name="e1" minOccurs="0" maxOccurs="0"/>
             <xs:element name="e2"/>
             <xs:any processContents="skip" notQName="##definedSibling" minOccurs="0"/>
           </xs:sequence>"###,
    );
    let set = schema(&xsd, true);
    assert_eq!(
        validity(&set, "<doc><e2/><e1/></doc>"),
        Some(SchemaValidity::Valid),
        "e1 maps to no component, so the wildcard admits it"
    );
    assert_eq!(
        validity(&set, "<doc><e2/><e2/></doc>"),
        Some(SchemaValidity::Invalid),
        "e2 is a real sibling and stays excluded"
    );
}
