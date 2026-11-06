//! Complex-content restriction (`derivation-ok-restriction`): a restricted
//! type's content must admit no child sequence its base's content rejects.
//!
//! * **Nested base sequences.** The content of an extension is
//!   `sequence(inherited, own)` with occurrence range 1..1 (XSD 1.0 §3.4.2
//!   clause 3.2.3, XSD 1.1 §3.4.2.3.3 clause 4.2.3.3). XSD 1.0 §3.9.6 clause
//!   2.2 ignores such "pointless" sequences when it checks a restriction;
//!   XSD 1.1 §3.4.6.4 compares languages, which a unit sequence inside a
//!   sequence does not change. A restriction that lists the base's elements
//!   one after another must therefore be checked against the flattened base,
//!   whether the nesting comes from extension (one level is enough), from
//!   nested `xs:sequence` elements or from a model-group reference.
//! * **Repetition counts.** `choice{m,n}(X{p,q})` repeats a branch `k` times
//!   for `k` in `m..=n`, which gives `[k·p, k·q]` occurrences of `X` — a set
//!   with gaps once `p ≥ 2`. Multiplying the ranges into `X{m·p, n·q}` fills
//!   the gaps in, and checking a repeated derived choice branch by branch
//!   misses the sequences that mix its branches. Each case below whose
//!   restriction is invalid carries a *witness*: a child sequence that the
//!   restriction's content accepts (as a standalone type) and the base
//!   rejects.
//!
//! Every case runs under XSD 1.0 and XSD 1.1.

use xsd_schema::validation::{
    drive_quick_xml, CollectingValidationSink, SchemaValidator, SchemaValidity, ValidationFlags,
};
use xsd_schema::{SchemaResult, SchemaSet, SchemaSetBuilder, XsdVersion};

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

// ── Schema building blocks ─────────────────────────────────────────────────

const U: Option<u32> = None;

/// The occurrence attributes for `{min, max}` (defaults omitted).
fn occurs(min: u32, max: Option<u32>) -> String {
    let mut s = String::new();
    if min != 1 {
        s.push_str(&format!(r#" minOccurs="{min}""#));
    }
    match max {
        Some(1) => {}
        Some(n) => s.push_str(&format!(r#" maxOccurs="{n}""#)),
        None => s.push_str(r#" maxOccurs="unbounded""#),
    }
    s
}

/// A local element `name{min,max}` of type `xs:string`.
fn e(name: &str, min: u32, max: Option<u32>) -> String {
    format!(
        r#"<xs:element name="{name}" type="xs:string"{}/>"#,
        occurs(min, max)
    )
}

/// A model group `compositor{min,max}(children)`.
fn g(compositor: &str, min: u32, max: Option<u32>, children: &[String]) -> String {
    format!(
        "<xs:{compositor}{}>{}</xs:{compositor}>",
        occurs(min, max),
        children.concat()
    )
}

fn seq(children: &[String]) -> String {
    g("sequence", 1, Some(1), children)
}

fn choice(children: &[String]) -> String {
    g("choice", 1, Some(1), children)
}

fn complex_type(name: &str, content: &str) -> String {
    format!(r#"<xs:complexType name="{name}">{content}</xs:complexType>"#)
}

fn extension(name: &str, base: &str, own: &str) -> String {
    format!(
        r#"<xs:complexType name="{name}"><xs:complexContent><xs:extension base="t:{base}">{own}</xs:extension></xs:complexContent></xs:complexType>"#
    )
}

fn restriction(name: &str, base: &str, content: &str) -> String {
    format!(
        r#"<xs:complexType name="{name}"><xs:complexContent><xs:restriction base="t:{base}">{content}</xs:restriction></xs:complexContent></xs:complexType>"#
    )
}

fn document(body: &str) -> String {
    format!(
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:t="urn:t" targetNamespace="urn:t">{body}</xs:schema>"#
    )
}

fn compile(version: XsdVersion, xsd: &str) -> SchemaResult<SchemaSet> {
    SchemaSetBuilder::with_version(version)
        .add_source(xsd, "file:///content_restriction.xsd")?
        .compile()
        .map(|compiled| compiled.into_schema_set())
}

fn validity(schema_set: &SchemaSet, xml: &str) -> Option<SchemaValidity> {
    let validator = SchemaValidator::new(schema_set, ValidationFlags::default());
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut runtime = validator.start_run(CollectingValidationSink {
        errors: &mut errors,
        warnings: &mut warnings,
    });
    drive_quick_xml(xml.as_bytes(), &mut runtime, schema_set)
        .expect("driver completes")
        .root_validity
}

/// `<t:{root}>` with one child `<x>x</x>` per name of `children`.
fn instance(root: &str, children: &[&str]) -> String {
    let body: String = children.iter().map(|c| format!("<{c}>x</{c}>")).collect();
    format!(r#"<t:{root} xmlns:t="urn:t">{body}</t:{root}>"#)
}

// ── Cases ──────────────────────────────────────────────────────────────────

const VERSIONS: [XsdVersion; 2] = [XsdVersion::V1_0, XsdVersion::V1_1];

/// One restriction: the base `B` (with the type definitions it derives
/// from), the restriction's content, and whether the restriction is valid
/// under XSD 1.0 and XSD 1.1 (`None`: not asserted).
struct Case {
    label: String,
    /// Type definitions; the base type is named `B`.
    base: String,
    /// The content of the restriction `R` of `B`.
    restricted: String,
    valid: [Option<bool>; 2],
    /// A child sequence `R`'s content accepts and `B` rejects.
    witness: Option<&'static [&'static str]>,
    /// Content that repeats `B`'s own, for [`every_base_restricts_itself`].
    identity: Option<String>,
}

impl Case {
    fn new(label: &str, base: String, restricted: String, v10: bool, v11: bool) -> Self {
        Case {
            label: label.to_string(),
            base,
            restricted,
            valid: [Some(v10), Some(v11)],
            witness: None,
            identity: None,
        }
    }

    /// The base is the single complex type `B` with content `content`.
    fn plain(label: &str, content: String, restricted: String, v10: bool, v11: bool) -> Self {
        let mut case = Case::new(label, complex_type("B", &content), restricted, v10, v11);
        case.identity = Some(content);
        case
    }

    /// `content` restates `B`'s content (written out for a derived base).
    fn identity(mut self, content: String) -> Self {
        self.identity = Some(content);
        self
    }

    fn witness(mut self, children: &'static [&'static str]) -> Self {
        self.witness = Some(children);
        self
    }

    /// Every way this case's expectations fail, as messages.
    fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        let xsd = document(&format!(
            "{}{}",
            self.base,
            restriction("R", "B", &self.restricted)
        ));
        for (version, expected) in VERSIONS.iter().zip(self.valid) {
            let Some(expected) = expected else { continue };
            match (compile(*version, &xsd), expected) {
                (Ok(_), true) => {}
                (Err(err), false) => {
                    let err = err.to_string();
                    if !err.contains("derivation-ok-restriction") {
                        problems.push(format!(
                            "{} [{version:?}]: expected derivation-ok-restriction, got: {err}",
                            self.label
                        ));
                    }
                }
                (Ok(_), false) => problems.push(format!(
                    "{} [{version:?}]: the restriction must be rejected (derivation-ok-restriction)",
                    self.label
                )),
                (Err(err), true) => problems.push(format!(
                    "{} [{version:?}]: the restriction must load: {err}",
                    self.label
                )),
            }
        }
        if let Some(children) = self.witness {
            self.witness_problems(children, &mut problems);
        }
        problems
    }

    /// The language fact behind a rejection: `children` is valid against
    /// `R`'s content as a standalone type and invalid against `B`.
    fn witness_problems(&self, children: &[&str], problems: &mut Vec<String>) {
        let xsd = document(&format!(
            r#"{}{}<xs:element name="root" type="t:B"/><xs:element name="rootR" type="t:Rs"/>"#,
            self.base,
            complex_type("Rs", &self.restricted)
        ));
        for version in VERSIONS {
            let set = match compile(version, &xsd) {
                Ok(set) => set,
                Err(e) => {
                    problems.push(format!("{} [{version:?}]: witness schema: {e}", self.label));
                    continue;
                }
            };
            if validity(&set, &instance("rootR", children)) != Some(SchemaValidity::Valid) {
                problems.push(format!(
                    "{} [{version:?}]: witness {children:?} must be valid against R's content",
                    self.label
                ));
            }
            if validity(&set, &instance("root", children)) != Some(SchemaValidity::Invalid) {
                problems.push(format!(
                    "{} [{version:?}]: witness {children:?} must be invalid against B",
                    self.label
                ));
            }
        }
    }
}

/// Checks every case and reports all failures together.
fn check_all(cases: &[Case]) {
    let problems: Vec<String> = cases.iter().flat_map(Case::problems).collect();
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

// ── Nested base sequences (C21) ────────────────────────────────────────────

/// `A(a?, b?) ⊕ (d?, e?) ⊕ (f?)`: the content of `B` is
/// `sequence(sequence(sequence(a?, b?), sequence(d?, e?)), sequence(f?))`.
fn restriction_of_a_three_level_extension_chain_cases() -> Vec<Case> {
    let base = [
        complex_type("A", &seq(&[e("a", 0, Some(1)), e("b", 0, Some(1))])),
        extension("M", "A", &seq(&[e("d", 0, Some(1)), e("e", 0, Some(1))])),
        extension("B", "M", &seq(&[e("f", 0, Some(1))])),
    ]
    .concat();
    let restricted = seq(&[e("b", 0, Some(1)), e("d", 0, Some(1)), e("f", 0, Some(1))]);
    let identity = seq(&[
        seq(&[
            seq(&[e("a", 0, Some(1)), e("b", 0, Some(1))]),
            seq(&[e("d", 0, Some(1)), e("e", 0, Some(1))]),
        ]),
        seq(&[e("f", 0, Some(1))]),
    ]);
    vec![Case::new("three-level", base, restricted, true, true).identity(identity)]
}

#[test]
fn restriction_of_a_three_level_extension_chain() {
    check_all(&restriction_of_a_three_level_extension_chain_cases());
}

/// The same nesting written out literally.
fn restriction_of_literally_nested_sequences_cases() -> Vec<Case> {
    let content = seq(&[
        seq(&[
            seq(&[e("a", 0, Some(1)), e("b", 0, Some(1))]),
            seq(&[e("d", 0, Some(1))]),
        ]),
        seq(&[e("f", 0, Some(1))]),
    ]);
    let restricted = seq(&[e("b", 0, Some(1)), e("d", 0, Some(1)), e("f", 0, Some(1))]);
    vec![Case::plain(
        "nested-literal",
        content,
        restricted,
        true,
        true,
    )]
}

#[test]
fn restriction_of_literally_nested_sequences() {
    check_all(&restriction_of_literally_nested_sequences_cases());
}

/// One extension level is enough: `A(a?, b?) ⊕ (d?)` restricted to
/// `(a?, b?, d?)` — the shape of a restriction that repeats an all-optional
/// base element by element.
fn restriction_of_a_one_level_extension_cases() -> Vec<Case> {
    let base = [
        complex_type("A", &seq(&[e("a", 0, Some(1)), e("b", 0, Some(1))])),
        extension("B", "A", &seq(&[e("d", 0, Some(1))])),
    ]
    .concat();
    let restricted = seq(&[e("a", 0, Some(1)), e("b", 0, Some(1)), e("d", 0, Some(1))]);
    let identity = seq(&[
        seq(&[e("a", 0, Some(1)), e("b", 0, Some(1))]),
        seq(&[e("d", 0, Some(1))]),
    ]);
    vec![Case::new("one level, all optional", base, restricted, true, true).identity(identity)]
}

#[test]
fn restriction_of_a_one_level_extension() {
    check_all(&restriction_of_a_one_level_extension_cases());
}

/// A model-group reference nested in the base's sequence.
fn restriction_of_a_sequence_with_a_group_reference_cases() -> Vec<Case> {
    let base = format!(
        r#"<xs:group name="g">{}</xs:group>{}"#,
        seq(&[e("a", 0, Some(1)), e("b", 0, Some(1)), e("c", 0, Some(1))]),
        complex_type(
            "B",
            &seq(&[r#"<xs:group ref="t:g"/>"#.to_string(), e("d", 0, Some(1))])
        )
    );
    let restricted = seq(&[e("a", 0, Some(1)), e("b", 0, Some(1)), e("d", 0, Some(1))]);
    let identity = seq(&[r#"<xs:group ref="t:g"/>"#.to_string(), e("d", 0, Some(1))]);
    vec![Case::new("group reference", base, restricted, true, true).identity(identity)]
}

#[test]
fn restriction_of_a_sequence_with_a_group_reference() {
    check_all(&restriction_of_a_sequence_with_a_group_reference_cases());
}

/// Controls that load already without flattening: two extension levels
/// restricted to one element of each part, and required elements, which
/// the restriction repeats as whole units.
fn restriction_of_extension_chains_controls_cases() -> Vec<Case> {
    let two_level = [
        complex_type("A", &seq(&[e("a", 0, Some(1)), e("b", 0, Some(1))])),
        extension("B", "A", &seq(&[e("d", 0, Some(1)), e("e", 0, Some(1))])),
    ]
    .concat();
    let required = [
        complex_type("A", &seq(&[e("a", 1, Some(1)), e("b", 1, Some(1))])),
        extension("B", "A", &seq(&[e("d", 1, Some(1))])),
    ]
    .concat();
    vec![
        Case::new(
            "two-level",
            two_level,
            seq(&[e("b", 0, Some(1)), e("d", 0, Some(1))]),
            true,
            true,
        )
        .identity(seq(&[
            seq(&[e("a", 0, Some(1)), e("b", 0, Some(1))]),
            seq(&[e("d", 0, Some(1)), e("e", 0, Some(1))]),
        ])),
        Case::new(
            "one level, required",
            required,
            seq(&[e("a", 1, Some(1)), e("b", 1, Some(1)), e("d", 1, Some(1))]),
            true,
            true,
        )
        .identity(seq(&[
            seq(&[e("a", 1, Some(1)), e("b", 1, Some(1))]),
            seq(&[e("d", 1, Some(1))]),
        ])),
    ]
}

#[test]
fn restriction_of_extension_chains_controls() {
    check_all(&restriction_of_extension_chains_controls_cases());
}

// ── Repetition counts ──────────────────────────────────────────────────────

/// A sequence restricting a repeated base choice: the number of `a`s it
/// admits must be a number the choice's iterations can produce.
fn sequence_restricting_a_repeated_choice_cases() -> Vec<Case> {
    let a = |min, max| e("a", min, max);
    let b = |min, max| e("b", min, max);
    vec![
        // Each `a*` and `b+` is one branch; `a*` alone restricts it.
        Case::plain(
            "choice?(a*, b+) > (a*)",
            g("choice", 0, Some(1), &[a(0, U), b(1, U)]),
            seq(&[a(0, U)]),
            true,
            true,
        ),
        // XSD 1.0: the W3C suite's reading (particlesZ001); XSD 1.1: (a|b)*
        // contains a*.
        Case::plain(
            "choice{0,∞}(a, b) > (a*)",
            g("choice", 0, U, &[a(1, Some(1)), b(1, Some(1))]),
            seq(&[a(0, U)]),
            true,
            true,
        ),
        Case::plain(
            "choice?(a{2,3}, b) > (a)",
            g("choice", 0, Some(1), &[a(2, Some(3)), b(1, Some(1))]),
            seq(&[a(1, Some(1))]),
            false,
            false,
        )
        .witness(&["a"]),
        Case::plain(
            "choice{0,∞}(a{2,2}, b) > (a)",
            g("choice", 0, U, &[a(2, Some(2)), b(1, Some(1))]),
            seq(&[a(1, Some(1))]),
            false,
            false,
        )
        .witness(&["a"]),
        Case::plain(
            "choice(a{2,3}, b) > (a)",
            choice(&[a(2, Some(3)), b(1, Some(1))]),
            seq(&[a(1, Some(1))]),
            false,
            false,
        )
        .witness(&["a"]),
        Case::plain(
            "choice{1,∞}(a{2,2}, b) > (a{3,3})",
            g("choice", 1, U, &[a(2, Some(2)), b(1, Some(1))]),
            seq(&[a(3, Some(3))]),
            false,
            false,
        )
        .witness(&["a", "a", "a"]),
        // XSD 1.0: RecurseAsIfGroup needs a?'s range inside the choice's
        // 1..1 (particlesHa161); XSD 1.1: {ε, a} ⊆ {ε, a, b}.
        Case::plain(
            "choice?(a, b) > (a?)",
            g("choice", 0, Some(1), &[a(1, Some(1)), b(1, Some(1))]),
            seq(&[a(0, Some(1))]),
            false,
            true,
        ),
        // XSD 1.1: the empty sequence is the empty iteration of b?.
        Case::plain(
            "choice(a, b?) > (a?)",
            choice(&[a(1, Some(1)), b(0, Some(1))]),
            seq(&[a(0, Some(1))]),
            false,
            true,
        ),
        // The same with a large bound: decided without walking the bound.
        Case::plain(
            "choice{1,9999999}(a{1,3}, b?) > (a{0,9999999})",
            g(
                "choice",
                1,
                Some(9_999_999),
                &[a(1, Some(3)), b(0, Some(1))],
            ),
            seq(&[a(0, Some(9_999_999))]),
            false,
            true,
        ),
    ]
}

#[test]
fn sequence_restricting_a_repeated_choice() {
    check_all(&sequence_restricting_a_repeated_choice_cases());
}

/// A choice restricting a repeated base choice.
fn choice_restricting_a_choice_cases() -> Vec<Case> {
    let a = |min, max| e("a", min, max);
    let b = |min, max| e("b", min, max);
    vec![
        Case::plain(
            "choice?(a{2,3}, b) > choice(a, b)",
            g("choice", 0, Some(1), &[a(2, Some(3)), b(1, Some(1))]),
            choice(&[a(1, Some(1)), b(1, Some(1))]),
            false,
            false,
        )
        .witness(&["a"]),
        // The derived choice repeats: (a, b) mixes its branches.
        Case::plain(
            "choice(a{0,2}, b{0,2}) > choice{0,2}(a, b)",
            choice(&[a(0, Some(2)), b(0, Some(2))]),
            g("choice", 0, Some(2), &[a(1, Some(1)), b(1, Some(1))]),
            false,
            false,
        )
        .witness(&["a", "b"]),
        // Guards: a wildcard branch absorbs the mixtures …
        Case::plain(
            "choice?(any{0,2}, z) > choice{0,2}(a, b)",
            g(
                "choice",
                0,
                Some(1),
                &[
                    r###"<xs:any namespace="##any" processContents="lax" maxOccurs="2"/>"###
                        .to_string(),
                    e("z", 1, Some(1)),
                ],
            ),
            g("choice", 0, Some(2), &[a(1, Some(1)), b(1, Some(1))]),
            false,
            true,
        ),
        // … and so does a base choice repeated as often, branch for branch
        // (XSD 1.0: RecurseLax keeps the order).
        Case::plain(
            "choice{0,∞}(a, b, c) > choice{0,∞}(b, a)",
            g(
                "choice",
                0,
                U,
                &[a(1, Some(1)), b(1, Some(1)), e("c", 1, Some(1))],
            ),
            g("choice", 0, U, &[b(1, Some(1)), a(1, Some(1))]),
            false,
            true,
        ),
    ]
}

#[test]
fn choice_restricting_a_choice() {
    check_all(&choice_restricting_a_choice_cases());
}

/// A choice kept with one of its base's branches, as the GAEB DA XML 3.3
/// redefinitions do (`tgBoQCtgy` of DA 84Z, `tgItem` of DA 85, 86 and 87,
/// reduced). XSD 1.0 §3.9.6 clause 2.2 makes a single-branch choice
/// pointless only when its particle is 1..1, so `choice{0,1}(b)` stays a
/// choice and RecurseLax maps its branch onto the base's; Xerces-J 2.12.2
/// accepts the four valid rows and rejects the three others (the XSD 1.1
/// verdicts are the enumeration oracle's, length ≤ 6).
fn single_branch_choice_restricting_a_choice_cases() -> Vec<Case> {
    let row = |base: &str, restricted: &str, valid: bool| {
        Case::plain(
            &format!("{base} > {restricted}"),
            particle(base),
            particle(restricted),
            valid,
            valid,
        )
    };
    vec![
        row(
            "seq(a{0,1},b{0,1},choice{0,1}(c,d))",
            "seq(b{0,1},choice{0,1}(d))",
            true,
        ),
        row(
            "seq(choice{0,1}(seq(a,b{0,1}),seq(b,c{0,∞})),d)",
            "seq(choice{0,1}(seq(b,c{0,∞})),d)",
            true,
        ),
        row(
            "seq(choice{0,1}(a,b),c{0,1})",
            "seq(choice{0,1}(a),c{0,1})",
            true,
        ),
        row("choice{0,1}(a,b)", "choice{0,1}(b)", true),
        // Controls: Xerces-J rcase-RecurseLax.1, rcase-RecurseLax.1,
        // rcase-Recurse.2.
        row("choice(a,b)", "choice{0,1}(b)", false).witness(&[]),
        row("choice{0,1}(a,b)", "choice{0,2}(b)", false).witness(&["b", "b"]),
        row(
            "seq(choice{0,1}(a,b),c)",
            "seq(choice{0,1}(b),c{0,1})",
            false,
        )
        .witness(&[]),
    ]
}

#[test]
fn single_branch_choice_restricting_a_choice() {
    check_all(&single_branch_choice_restricting_a_choice_cases());
}

/// A repeated derived choice inside or against a base sequence.
fn repeated_choice_restricting_a_sequence_cases() -> Vec<Case> {
    let a = |min, max| e("a", min, max);
    let b = |min, max| e("b", min, max);
    let mut not_repeated = Case::plain(
        "seq(a?, b?) > choice(a, b)",
        seq(&[a(0, Some(1)), b(0, Some(1))]),
        choice(&[a(1, Some(1)), b(1, Some(1))]),
        true,
        true,
    );
    // XSD 1.0's table marks Choice:Sequence Forbidden; not asserted here.
    not_repeated.valid[0] = None;
    vec![
        Case::plain(
            "seq(a{0,2}, b{0,2}, c) > seq(choice{0,2}(a, b), c)",
            seq(&[a(0, Some(2)), b(0, Some(2)), e("c", 1, Some(1))]),
            seq(&[
                g("choice", 0, Some(2), &[a(1, Some(1)), b(1, Some(1))]),
                e("c", 1, Some(1)),
            ]),
            false,
            false,
        )
        .witness(&["b", "a", "c"]),
        Case::plain(
            "seq(a{0,2}, b{0,2}) > choice{0,2}(a, b)",
            seq(&[a(0, Some(2)), b(0, Some(2))]),
            g("choice", 0, Some(2), &[a(1, Some(1)), b(1, Some(1))]),
            false,
            false,
        )
        .witness(&["b", "a"]),
        // Guard: a choice that does not repeat cannot mix its branches.
        not_repeated,
    ]
}

#[test]
fn repeated_choice_restricting_a_sequence() {
    check_all(&repeated_choice_restricting_a_sequence_cases());
}

/// Groups whose single child needs two or more occurrences: the counts of
/// a repeated group have gaps.
fn single_child_groups_with_gapped_counts_cases() -> Vec<Case> {
    let b = |min, max| e("b", min, max);
    vec![
        // XSD 1.1 folds the base sequence{1,2}(b{2,2}) (particlesHa147).
        Case::plain(
            "choice(seq{1,2}(b{2,2})) > (b{3,3})",
            choice(&[g("sequence", 1, Some(2), &[b(2, Some(2))])]),
            seq(&[b(3, Some(3))]),
            false,
            false,
        )
        .witness(&["b", "b", "b"]),
        // Normalization collapses the single-branch choice{1,2}(b{2,2}).
        Case::plain(
            "choice{1,2}(b{2,2}) > (b{3,3})",
            g("choice", 1, Some(2), &[b(2, Some(2))]),
            seq(&[b(3, Some(3))]),
            false,
            false,
        )
        .witness(&["b", "b", "b"]),
        // A repeated group branch of a choice.
        Case::plain(
            "choice?(seq{2,2}(a, b), c) > seq(a, b)",
            g(
                "choice",
                0,
                Some(1),
                &[
                    g("sequence", 2, Some(2), &[e("a", 1, Some(1)), b(1, Some(1))]),
                    e("c", 1, Some(1)),
                ],
            ),
            seq(&[e("a", 1, Some(1)), b(1, Some(1))]),
            false,
            false,
        )
        .witness(&["a", "b"]),
        // An element against it (XSD 1.0: RecurseAsIfGroup against a choice
        // that does not repeat is the whole rule).
        Case::plain(
            "choice?(seq{2,2}(a, b?), c) > (a)",
            g(
                "choice",
                0,
                Some(1),
                &[
                    g("sequence", 2, Some(2), &[e("a", 1, Some(1)), b(0, Some(1))]),
                    e("c", 1, Some(1)),
                ],
            ),
            seq(&[e("a", 1, Some(1))]),
            false,
            false,
        )
        .witness(&["a"]),
    ]
}

#[test]
fn single_child_groups_with_gapped_counts() {
    check_all(&single_child_groups_with_gapped_counts_cases());
}

// ── Fix round 1: valid restrictions the first version refused ──────────────

/// The reviewer's rows: a derived group keeps the gaps of its counts just as
/// the base does, and a repeated choice is compared with a base sequence as a
/// whole particle. XSD 1.0's verdicts follow its §3.9.6 table.
fn reviewer_rows_cases() -> Vec<Case> {
    let a = |min, max| e("a", min, max);
    let b = |min, max| e("b", min, max);
    let mix = || g("choice", 0, Some(2), &[a(1, Some(1)), b(1, Some(1))]);
    vec![
        // {2, 4} ⊆ {2, 4}. XSD 1.0: Sequence:Sequence Recurse.
        Case::plain(
            "seq{1,2}(b{2,2}) > seq{1,2}(b{2,2})",
            g("sequence", 1, Some(2), &[b(2, Some(2))]),
            g("sequence", 1, Some(2), &[b(2, Some(2))]),
            true,
            true,
        ),
        // {2, 4} ⊆ {2, 4, 6}. XSD 1.0: Recurse, {1,2} within {1,3}.
        Case::plain(
            "seq{1,3}(b{2,2}) > seq{1,2}(b{2,2})",
            g("sequence", 1, Some(3), &[b(2, Some(2))]),
            g("sequence", 1, Some(2), &[b(2, Some(2))]),
            true,
            true,
        ),
        // {3} ⊄ {2, 4}. XSD 1.0: Elt:Sequence RecurseAsIfGroup, b{3,3}
        // against b{2,2} fails NameAndTypeOK's occurrence range.
        Case::plain(
            "seq{1,2}(b{2,2}) > (b{3,3})",
            g("sequence", 1, Some(2), &[b(2, Some(2))]),
            seq(&[b(3, Some(3))]),
            false,
            false,
        )
        .witness(&["b", "b", "b"]),
        // XSD 1.1: the whole choice restricts the first child, `c?` is
        // emptiable. XSD 1.0: Choice:Sequence is Forbidden.
        Case::plain(
            "seq(choice{0,2}(a, b), c?) > choice{0,2}(a, b)",
            seq(&[mix(), e("c", 0, Some(1))]),
            mix(),
            false,
            true,
        ),
        // The restriction admits the empty sequence; the base needs `c`.
        Case::plain(
            "seq(choice{0,2}(a, b), c) > choice{0,2}(a, b)",
            seq(&[mix(), e("c", 1, Some(1))]),
            mix(),
            false,
            false,
        )
        .witness(&[]),
        // (E6) `(b, a)`: no single child of the base takes the mixtures.
        Case::plain(
            "seq(a{0,2}, b{0,2}) > choice{0,2}(a, b) [reviewer]",
            seq(&[a(0, Some(2)), b(0, Some(2))]),
            mix(),
            false,
            false,
        )
        .witness(&["b", "a"]),
    ]
}

#[test]
fn reviewer_rows() {
    check_all(&reviewer_rows_cases());
}

// ── Fix round 2: count arithmetic at its edges ─────────────────────────────

/// How long one case may take. Every case below answers in a few
/// milliseconds; the limit only turns a hang into a failure.
const TIME_LIMIT: Duration = Duration::from_secs(2);

/// [`check_all`], each case on its own thread with [`TIME_LIMIT`]: a case
/// without an answer by then fails (its thread is left behind).
fn check_all_within_time_limit(cases: Vec<Case>) {
    let mut problems = Vec::new();
    for case in cases {
        let label = case.label.clone();
        let (sender, receiver) = mpsc::channel();
        let started = Instant::now();
        thread::spawn(move || {
            let _ = sender.send(case.problems());
        });
        match receiver.recv_timeout(TIME_LIMIT) {
            Ok(found) => problems.extend(found),
            Err(_) => problems.push(format!("{label}: no answer within {TIME_LIMIT:?}")),
        }
        eprintln!("{:>9.3} ms  {label}", started.elapsed().as_secs_f64() * 1e3);
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

/// Bounds whose products leave `u64` (`3,000,000³ ≈ 2.7·10¹⁹`), bounds at
/// `u32::MAX`, unbounded groups around a term with equal per-iteration
/// bounds (`{k·p}`, a progression with no end), two and three levels deep.
/// The verdicts of the non-identical rows are arithmetic:
/// `choice{G,∞}(choice{G,∞}(a{G,G}))` admits exactly the multiples of `G`
/// from `G³` on.
fn large_bounds_cases() -> Vec<Case> {
    const G: u32 = 3_000_000;
    const M: u32 = u32::MAX;
    let row = |base: String, restricted: String, valid: bool| {
        Case::plain(
            &format!("{base} > {restricted}"),
            particle(&base),
            particle(&restricted),
            valid,
            valid,
        )
    };
    let same = |base: String| row(base.clone(), base, true);
    let reviewer = format!("choice{{{G},∞}}(choice{{{G},∞}}(a{{{G},{G}}}))");
    vec![
        // The reviewer's shape, restricted by itself.
        same(reviewer.clone()),
        // G³ = G²·G is a multiple of G, at least G³ (Xerces-J accepts).
        row(
            reviewer.clone(),
            format!("choice{{{G},{G}}}(choice{{{G},{G}}}(a{{{G},{G}}}))"),
            true,
        ),
        // u32::MAX and 2³¹ − 1 occurrences are below G³ (Xerces-J rejects
        // the second, rcase-RecurseLax.1; it does not read u32::MAX).
        row(reviewer.clone(), format!("seq(a{{{M},{M}}})"), false),
        row(
            reviewer.clone(),
            "seq(a{2147483647,2147483647})".to_string(),
            false,
        ),
        same(format!(
            "seq{{{G},∞}}(choice{{{G},∞}}(choice{{{G},∞}}(a{{{G},{G}}})))"
        )),
        same(format!("choice{{{M},∞}}(choice{{{M},∞}}(a{{{M},{M}}}))")),
        same(format!("choice{{{M},{M}}}(seq{{{M},{M}}}(a{{{M},{M}}}))")),
        same(format!(
            "seq{{{M},{M}}}(choice{{{M},∞}}(choice{{{M},∞}}(a{{{M},{M}}})))"
        )),
        same(format!("choice{{{M},∞}}(choice{{{M},∞}}(a{{0,0}}))")),
        // Unbounded, equal per-iteration bounds: {k·G : k ≥ 2}.
        same(format!("choice{{2,∞}}(a{{{G},{G}}})")),
        same(format!("choice{{0,∞}}(seq{{3,3}}(a{{{M},{M}}}))")),
        row(
            format!("choice{{2,∞}}(a{{{G},{G}}})"),
            format!("choice{{3,∞}}(a{{{G},{G}}})"),
            true,
        ),
        // G is not among {2G, 3G, …} (Xerces-J rcase-RecurseLax.1).
        row(
            format!("choice{{2,∞}}(a{{{G},{G}}})"),
            format!("seq(a{{{G},{G}}})"),
            false,
        ),
    ]
}

#[test]
fn large_bounds() {
    check_all_within_time_limit(large_bounds_cases());
}

/// A particle with `minOccurs = maxOccurs = 0` maps to no component
/// (§3.3.2): repeating its group does not make it admit anything. The
/// reviewer's `choice+(a{0,0})` against a restriction that needs one `a`,
/// the same zero under the other folds (a single-child sequence, a base
/// choice branch, a derived group), and two valid controls.
fn zero_occurrences_cases() -> Vec<Case> {
    let row = |base: &str, restricted: &str, valid: [Option<bool>; 2]| {
        let mut case = Case::plain(
            &format!("{base} > {restricted}"),
            particle(base),
            particle(restricted),
            valid[0].unwrap_or(true),
            valid[1].unwrap_or(true),
        );
        case.valid = valid;
        case
    };
    let invalid = [Some(false), Some(false)];
    vec![
        // The reviewer's case (Xerces-J cos-particle-restrict.b).
        row("choice{1,∞}(a{0,0})", "seq(a)", invalid).witness(&["a"]),
        row("seq{1,∞}(a{0,0})", "seq(a)", invalid).witness(&["a"]),
        row("seq(b,choice{2,∞}(a{0,0}))", "seq(b,a{0,1})", invalid).witness(&["b", "a"]),
        row("choice{1,∞}(a{0,0},b)", "seq(a,b)", invalid).witness(&["a", "b"]),
        // Valid: `(b, b)` repeated, the zero branch absent on both sides.
        row(
            "choice{0,∞}(a{0,0},b{2,2})",
            "choice{0,∞}(b{2,2})",
            [Some(true), Some(true)],
        ),
        // The derived group admits nothing beyond the empty sequence, which
        // the base admits (XSD 1.0 not asserted: Xerces-J calls choice
        // against sequence Forbidden, and 0.2.x accepted it).
        row("seq(b{0,1})", "choice{1,∞}(a{0,0})", [None, Some(true)]),
    ]
}

#[test]
fn zero_occurrences() {
    check_all_within_time_limit(zero_occurrences_cases());
}

/// XSD 1.0 compares a single-branch choice that is not pointless as
/// written (§3.9.6 clause 2.2), its branch with the branch's own range:
/// RecurseLax maps `a?` onto the base's `a` only if `a?`'s range lies within
/// `a`'s. XSD 1.1 compares languages. Xerces-J agrees with every XSD 1.0
/// verdict asserted here.
fn re_expanded_choice_bounds_cases() -> Vec<Case> {
    let row = |base: &str, restricted: &str, valid: [Option<bool>; 2]| {
        let mut case = Case::plain(
            &format!("{base} > {restricted}"),
            particle(base),
            particle(restricted),
            valid[0].unwrap_or(true),
            valid[1].unwrap_or(true),
        );
        case.valid = valid;
        case
    };
    vec![
        // The reviewer's case (Xerces-J rcase-RecurseLax.2).
        row(
            "choice{0,1}(a,b)",
            "choice{0,1}(a{0,1})",
            [Some(false), Some(true)],
        ),
        row(
            "choice{0,1}(seq(a,b{0,1}),c)",
            "choice{0,1}(seq{0,1}(a,b{0,1}))",
            [Some(false), Some(true)],
        ),
        // The branch's range within the base branch's (Xerces-J accepts).
        row(
            "choice{0,1}(a{0,1},b)",
            "choice{0,1}(a{0,1})",
            [Some(true), Some(true)],
        ),
        row(
            "choice{0,3}(a{2,2},b)",
            "choice{0,1}(a{2,2})",
            [Some(true), Some(true)],
        ),
    ]
}

#[test]
fn re_expanded_choice_bounds() {
    check_all_within_time_limit(re_expanded_choice_bounds_cases());
}

/// From the edge sweep (occurrence bounds 0–3, 3,000,000, 2³¹ − 1, `u32::MAX`
/// and unbounded, `maxOccurs="0"` particles): one row per defect it found
/// in the fix-round-1 tree. XSD 1.0 verdicts are Xerces-J's (a row with
/// bounds Xerces-J cannot read is checked arithmetically); XSD 1.1 verdicts
/// the enumeration oracle's, or arithmetic where the witness is too long to
/// spell (noted). `None`: an XSD 1.0 difference from Xerces-J that 0.2.x
/// already had.
fn edge_sweep_cases() -> Vec<Case> {
    const G: u32 = 3_000_000;
    const I: u32 = 2_147_483_647;
    const M: u32 = u32::MAX;
    let row = |base: String, restricted: String, valid: [Option<bool>; 2]| {
        let mut case = Case::plain(
            &format!("{base} > {restricted}"),
            particle(&base),
            particle(&restricted),
            valid[0].unwrap_or(true),
            valid[1].unwrap_or(true),
        );
        case.valid = valid;
        case
    };
    let both = |valid| [Some(valid), Some(valid)];
    vec![
        // The atomic unit match of a sequence takes one iteration of the
        // base unit: `seq{2,2}(…)` admits none alone (a⁶: three iterations
        // of two `a`s; the base needs four per iteration).
        row(
            "seq{3,5}(seq{2,2}(a{2,6}))".into(),
            "seq{3,5}(seq{2,2}(a{1,2}))".into(),
            both(false),
        )
        .witness(&["a", "a", "a", "a", "a", "a"]),
        row(
            "seq(seq{2,2}(b,c),d{0,1})".into(),
            "seq(b,c)".into(),
            both(false),
        )
        .witness(&["b", "c"]),
        // A sequence's demand on a base choice from its exact counts: the
        // restriction admits M·G + 1 `b`s, the base at most M·G (no
        // instance: arithmetic).
        row(
            format!("choice{{2,{M}}}(seq{{0,2}}(choice{{{G},∞}}(a{{1,{I}}})),seq(b{{3,{G}}}))"),
            format!("choice{{2,{M}}}(seq{{0,{G}}}(b{{3,{G}}}))"),
            both(false),
        ),
        // A derived particle restricting a base child repeated over the
        // base's iterations, also where the base could iterate once.
        row(
            "choice{0,3}(seq{0,1}(b{0,∞},c))".into(),
            "choice{0,3}(seq{0,1}(c))".into(),
            both(true),
        ),
        row(
            "choice{1,3}(seq(a{0,∞},choice{3,3}(b{1,∞},c{0,3},d{1,3})))".into(),
            "choice{1,3}(seq(choice{3,3}(b{1,∞},c{0,3},d{1,3})))".into(),
            [Some(false), Some(true)],
        ),
        // A set repeated a gapped number of times: two or more iterations
        // of {2j : j ≥ G} are the even counts from 4G on.
        row(
            format!("choice{{1,{M}}}(choice{{2,2}}(a{{2,{G}}},choice{{{G},∞}}(b{{2,2}})))"),
            format!("choice{{1,{M}}}(choice{{2,2}}(choice{{{G},∞}}(b{{2,2}})))"),
            both(true),
        ),
        // Absent particles: a required choice of `maxOccurs="0"` branches
        // accepts nothing, an optional one only the empty sequence.
        row(
            "choice{0,1}(a{2,2},seq{3,5}(c,choice{2,2}(d{0,0})))".into(),
            "choice{0,1}(a{2,2},seq{3,5}(c,choice{0,∞}(d{0,0})))".into(),
            [None, Some(false)],
        )
        .witness(&["c", "c", "c"]),
        row(
            "choice{0,3}(a{0,2},choice{2,3}(b,seq{2,2}(d{0,0})))".into(),
            "choice{0,3}(a{0,0},choice{2,3}(b,seq{2,2}(d{0,0})))".into(),
            both(true),
        ),
        // The restriction's iterations each need a choice with no branch:
        // it accepts only the empty sequence, which the base accepts.
        row(
            "seq{0,2}(choice{3,3}(a{2,∞}),choice{1,5}(b{0,1}),c)".into(),
            "seq{0,2}(choice{2,∞}(a{2,∞}),choice{1,5}(b{0,0}),c)".into(),
            [Some(false), Some(true)],
        ),
        // XSD 1.0 compares a re-offered group with the base as written
        // too (Xerces-J rcase-RecurseLax.2, rcase-MapAndSum.2,
        // rcase-RecurseLax.2).
        row(
            "choice{1,2}(a{3,3},seq{1,∞}(seq{0,1}(b,c{2,3})))".into(),
            "choice{1,2}(seq(seq{0,1}(b,c{2,3})))".into(),
            [Some(false), Some(true)],
        ),
        row(
            "choice(choice{0,1}(a{1,2},b{1,∞}),seq{2,3}(choice(c{0,1}),seq{0,1}(d{2,∞})))".into(),
            "choice(seq{2,3}(choice(c{0,1})))".into(),
            [Some(false), Some(true)],
        ),
        row(
            "choice{2,2}(a,choice{1,2}(b{0,1},choice{0,1}(c{2,3},d)))".into(),
            "choice{2,2}(a,choice{1,2}(choice(c{2,3},d)))".into(),
            [Some(false), Some(true)],
        ),
        // XSD 1.0, a single-child sequence that is not pointless, compared as
        // written (the GAEB DA XML 3.3 `tgSubDescr` and `tgAward` shapes).
        row(
            "seq(a,seq{0,1}(b,c{0,1}))".into(),
            "seq(a,seq{0,1}(b))".into(),
            both(true),
        ),
        row(
            "seq(a,seq{0,∞}(b,c{0,1}),d{0,1})".into(),
            "seq(a,seq{0,∞}(b),d)".into(),
            both(true),
        ),
        row(
            format!("choice{{0,{M}}}(a{{0,∞}},b{{0,1}},seq{{0,3}}(choice{{2,2}}(c{{0,0}}),d{{{M},{M}}}))"),
            format!("choice{{0,{M}}}(seq{{0,3}}(choice{{2,2}}(c{{0,0}}),d{{{M},{M}}}))"),
            [Some(true), Some(true)],
        ),
    ]
}

#[test]
fn edge_sweep_rows() {
    check_all_within_time_limit(edge_sweep_cases());
}

/// Every base of this file restricted by its own content is a valid
/// restriction, in each version where the base itself is a valid schema
/// (`choice?(any{0,2}, z)` violates XSD 1.0's Unique Particle Attribution).
#[test]
fn every_base_restricts_itself() {
    let cases: Vec<Case> = [
        restriction_of_a_three_level_extension_chain_cases(),
        restriction_of_literally_nested_sequences_cases(),
        restriction_of_a_one_level_extension_cases(),
        restriction_of_a_sequence_with_a_group_reference_cases(),
        restriction_of_extension_chains_controls_cases(),
        sequence_restricting_a_repeated_choice_cases(),
        choice_restricting_a_choice_cases(),
        single_branch_choice_restricting_a_choice_cases(),
        repeated_choice_restricting_a_sequence_cases(),
        single_child_groups_with_gapped_counts_cases(),
        reviewer_rows_cases(),
        zero_occurrences_cases(),
        re_expanded_choice_bounds_cases(),
        edge_sweep_cases(),
        sweep_regression_rows_cases(),
        risky_rows_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    let mut problems = Vec::new();
    let mut checked = 0;
    for case in &cases {
        let identity = case
            .identity
            .as_ref()
            .unwrap_or_else(|| panic!("{}: no identity content", case.label));
        for version in VERSIONS {
            if compile(version, &document(&case.base)).is_err() {
                continue;
            }
            checked += 1;
            let xsd = document(&format!("{}{}", case.base, restriction("R", "B", identity)));
            if let Err(err) = compile(version, &xsd) {
                problems.push(format!(
                    "identity of [{}] [{version:?}]: the restriction must load: {err}",
                    case.label
                ));
            }
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
    assert!(
        checked >= 2 * cases.len() - 2,
        "bases that do not compile alone: {checked}"
    );
}

// ── Fix round 1: the differential sweep ───────────────────────────────────

/// A particle written `name{min,max}`, `seq{min,max}(…)` or
/// `choice{min,max}(…)` (`{1,1}` omitted, `∞` unbounded), as XSD.
fn particle(text: &str) -> String {
    fn parse(text: &[char], at: &mut usize) -> String {
        let start = *at;
        while text[*at].is_ascii_alphabetic() {
            *at += 1;
        }
        let name: String = text[start..*at].iter().collect();
        let mut range = (1u32, Some(1u32));
        if text.get(*at) == Some(&'{') {
            let close = *at + text[*at..].iter().position(|&c| c == '}').unwrap();
            let inner: String = text[*at + 1..close].iter().collect();
            let (min, max) = inner.split_once(',').unwrap();
            range = (
                min.parse().unwrap(),
                (max != "∞").then(|| max.parse().unwrap()),
            );
            *at = close + 1;
        }
        let (min, max) = range;
        match name.as_str() {
            "seq" | "choice" => {
                let compositor = if name == "seq" { "sequence" } else { "choice" };
                assert_eq!(text[*at], '(');
                *at += 1;
                let mut children = Vec::new();
                loop {
                    children.push(parse(text, at));
                    *at += 1;
                    if text[*at - 1] == ')' {
                        break;
                    }
                }
                g(compositor, min, max, &children)
            }
            _ => e(&name, min, max),
        }
    }
    let chars: Vec<char> = text.chars().collect();
    let mut at = 0;
    let xsd = parse(&chars, &mut at);
    assert_eq!(at, chars.len(), "{text}");
    xsd
}

/// Restrictions that `d5f2a8e` accepted and an intermediate version of this
/// change refused, from 20,000 random base and restriction pairs (removing an
/// optional particle, narrowing a range, keeping one branch of a choice,
/// unwrapping a group; occurrences 0–3 or unbounded, nesting up to 3). Every
/// one is valid under XSD 1.1 (no child sequence of length 5 or 6 that the
/// restriction accepts and the base rejects); the XSD 1.0 verdict is
/// Xerces-J 2.12.2's (schema-full-checking), not asserted where XSD 1.0
/// support here already differed from it before this change.
#[rustfmt::skip]
const SWEEP_ROWS: &[(&str, &str, Option<bool>)] = &[
    ("choice{0,1}(a{2,3},choice(b{0,∞},choice{1,3}(c{2,3},d{2,3})))", "choice{0,1}(a{2,3},b{0,∞},choice{1,3}(c{2,3},d{2,3}))", Some(true)), // 00105: Xerces OK
    ("choice{0,2}(choice{2,2}(choice{0,∞}(a{3,3},b{0,1})),c)", "choice{0,2}(choice{2,2}(choice{0,∞}(a{3,3})),c)", Some(true)), // 00145: Xerces OK
    ("choice{1,2}(a{1,∞},choice{0,3}(b{0,1},c{2,2}),choice{0,1}(seq{2,3}(d{2,2})))", "choice{1,2}(a{1,∞},choice{0,3}(b{0,1},c{2,2}),choice{0,1}(seq{2,3}(d{2,2})))", Some(true)), // 00178: Xerces OK
    ("seq{0,1}(choice{1,2}(a{1,2},b{1,3}))", "seq(choice{1,2}(a{1,2},b{1,3}))", Some(false)), // 00216: Xerces cos-particle-restrict.2
    ("choice{0,1}(choice{1,∞}(seq{1,3}(a{0,1},b{0,2}),c{3,3}))", "choice{0,1}(choice{3,∞}(seq{1,3}(a{0,1},b{0,2}),c{3,3}))", Some(true)), // 00312: Xerces OK
    ("choice{2,2}(seq{0,1}(seq{0,∞}(a,b{0,1})),choice{2,3}(c{2,2}))", "choice{2,2}(choice{2,3}(c{2,2}))", Some(true)), // 00481: Xerces OK
    ("choice(a{0,1},choice{0,1}(seq{2,3}(b{1,2},c{0,3})),d{2,∞})", "choice(a{0,1},choice(seq{2,3}(b{1,2},c{0,3})),d{2,∞})", None), // 00637: Xerces rcase-RecurseLax.2 (1.0 not asserted: accepted before and now)
    ("seq{0,1}(choice{2,3}(a{3,3}))", "seq{0,1}(choice{2,3}(a{3,3}))", Some(true)), // 00738: Xerces OK
    ("choice{3,3}(choice{0,1}(choice{3,∞}(a{3,3})))", "choice{3,3}(choice(choice{3,∞}(a{3,3})))", None), // 00765: Xerces rcase-RecurseLax.2 (1.0 not asserted: accepted before and now)
    ("choice{0,1}(choice{3,3}(seq{3,3}(a{0,1},b{0,2},c{2,3}),d{0,1}))", "choice{0,1}(choice{3,3}(seq{3,3}(a{0,1},b{0,2},c{2,3})))", None), // 00867: Xerces OK (1.0 not asserted: rejected before and now)
    ("choice{1,2}(seq{0,1}(seq{3,3}(a{1,∞})))", "choice{1,2}(seq{0,1}(seq{3,3}(a)))", Some(true)), // 00878: Xerces OK
    ("seq{0,1}(choice{1,2}(seq{2,∞}(a{2,2})),choice{0,1}(choice{3,3}(b{0,1},c{1,2},d{1,2})))", "seq{0,1}(choice{1,2}(seq{3,4}(a{2,2})),choice{0,1}(choice{3,3}(b{0,1},c{1,2},d{1,2})))", Some(true)), // 00903: Xerces OK
    ("choice(choice{1,2}(choice{1,∞}(a{3,∞},b{2,3},c{0,1})),choice{1,3}(d{2,3}))", "choice(choice{1,3}(d{2,3}))", None), // 00915: Xerces rcase-RecurseLax.1 (1.0 not asserted: accepted before and now)
    ("choice(choice{1,2}(seq{1,2}(a{1,2}),seq{1,2}(b{0,1},c{0,3}),d{1,3}))", "choice(choice{1,2}(seq{1,2}(a{1,2})))", None), // 00927: Xerces OK (1.0 not asserted: rejected before and now)
    ("choice(seq{2,2}(seq{2,∞}(a{1,∞},b{2,3})),c{0,1})", "choice(seq{2,2}(seq{2,∞}(a{1,∞},b{2,3})))", Some(false)), // 00932: Xerces rcase-MapAndSum.2
    ("seq{0,1}(a{2,2},choice{0,1}(b{1,2},choice(c{1,3},d{2,3})))", "seq{0,1}(a{2,2},choice{0,1}(b{1,2},c{1,3},d{2,3}))", Some(true)), // 01043: Xerces OK
    ("choice{0,1}(seq{3,3}(a{0,1},b{0,∞}),seq{1,∞}(choice{0,1}(c,d{2,3})))", "choice{0,1}(seq{3,3}(a{0,1},b{0,∞}),seq{1,∞}(choice{0,1}(d{2,3})))", None), // 01134: Xerces OK (1.0 not asserted: rejected before and now)
    ("choice{0,1}(choice{0,3}(a,b{1,2}),choice{0,1}(c{2,3},d{1,3}))", "choice{0,1}(choice{0,1}(c{2,3},d{1,3}))", None), // 01170: Xerces OK (1.0 not asserted: rejected before and now)
    ("seq{1,3}(a{0,∞},choice{3,∞}(choice{2,2}(b{2,2},c{1,∞},d{2,2})))", "seq{1,3}(choice{3,3}(choice{2,2}(b{2,2},c{1,∞},d{2,2})))", Some(true)), // 01253: Xerces OK
    ("choice{1,2}(choice{1,2}(a{1,3},choice(b{0,1},c{2,3}),d))", "choice{1,2}(choice{2,2}(a{1,3},choice(b{0,1},c{2,3}),d))", Some(true)), // 01261: Xerces OK
    ("seq{0,1}(seq{2,2}(a{1,∞}))", "seq{0,1}(seq{2,2}(a{1,∞}))", Some(true)), // 01301: Xerces OK
    ("choice{2,3}(seq{3,3}(a{1,3}))", "choice{2,3}(seq{3,3}(a{1,3}))", Some(true)), // 01478: Xerces OK
    ("choice{0,1}(choice{3,3}(a{0,1},b{0,1}))", "choice(choice{3,3}(a{0,1},b{0,1}))", None), // 01553: Xerces rcase-RecurseLax.1 (1.0 not asserted: accepted before and now)
    ("choice{2,2}(seq{0,1}(a{1,2}),b{0,∞})", "choice{2,2}(seq{0,1}(a{1,2}))", None), // 01598: Xerces OK (1.0 not asserted: rejected before and now)
    ("seq{1,2}(choice{0,1}(a{3,3},choice(b{0,1},c{0,1},d{0,1})))", "seq{1,2}(choice{0,1}(a{3,3}))", None), // 01691: Xerces OK (1.0 not asserted: rejected before and now)
    ("seq{1,2}(a{1,3},b{1,2},seq{1,∞}(choice{2,3}(c{2,2},d{0,1})))", "seq{1,2}(a{1,3},b{1,2},seq{1,∞}(choice{2,3}(c{2,2})))", Some(true)), // 01838: Xerces OK
    ("choice{0,1}(a{1,3},b,choice{3,3}(choice(c,d{2,∞})))", "choice{0,1}(choice{3,3}(choice(d{2,∞})))", None), // 01854: Xerces OK (1.0 not asserted: rejected before and now)
    ("seq{3,3}(a,seq{1,∞}(choice{1,3}(b{2,3},c{2,2})),choice{1,3}(seq{1,∞}(d{1,2})))", "seq{3,3}(a,seq{1,∞}(choice{1,3}(c{2,2})),choice{1,3}(seq{1,∞}(d{2,2})))", Some(true)), // 01883: Xerces OK
    ("seq{3,3}(a{2,3},seq{2,3}(choice{0,1}(b{2,3})),c{0,1})", "seq{3,3}(a{2,3},seq{2,3}(choice(b{2,3})),c{0,1})", Some(true)), // 01932: Xerces OK
    ("choice{0,1}(seq{2,3}(a{1,∞}))", "choice{0,1}(seq{2,3}(a{1,∞}))", Some(true)), // 01999: Xerces OK
    ("choice{2,2}(choice{2,∞}(choice{0,1}(a)),seq{0,3}(seq{1,2}(b{1,2},c,d{1,3})))", "choice{2,2}(seq{0,3}(seq{1,2}(b{1,2},c,d{1,3})))", None), // 02092: Xerces OK (1.0 not asserted: rejected before and now)
    ("choice(choice{2,∞}(a{0,∞},b{1,3},choice{0,2}(c{0,2})),seq{1,2}(seq{3,3}(d{2,3})))", "choice(choice{2,∞}(b{1,3},choice{0,2}(c{0,2})),seq{1,2}(seq{3,3}(d{2,3})))", Some(true)), // 02103: Xerces OK
    ("seq{0,∞}(seq{2,2}(seq{1,∞}(a{0,2}),b{0,1}))", "seq{0,∞}(seq{2,2}(seq{1,∞}(a{0,2})))", Some(true)), // 02342: Xerces OK
    ("seq{2,2}(choice{0,1}(a{1,∞}))", "seq{2,2}(choice{0,1}(a{2,2}))", Some(true)), // 02452: Xerces OK
    ("choice{0,1}(seq{2,∞}(a{1,∞}))", "choice(seq{2,∞}(a{1,∞}))", None), // 02477: Xerces rcase-MapAndSum.2 (1.0 not asserted: accepted before and now)
    ("choice{1,2}(choice{2,2}(a{0,1},seq{0,1}(b,c{0,∞},d{3,3})))", "choice{1,2}(choice{2,2}(a{0,1}))", Some(true)), // 02494: Xerces OK
    ("choice{0,1}(choice{0,1}(a{2,∞},b{0,1},c{2,2}),seq{1,∞}(seq{0,1}(d{2,2})))", "choice{0,1}(choice{0,1}(a{2,∞},b{0,1},c{2,2}))", None), // 02498: Xerces OK (1.0 not asserted: rejected before and now)
    ("choice{0,1}(a,seq{2,2}(b{2,3}),choice(c{0,1},d{0,∞}))", "choice{0,1}(a,seq{2,2}(b{2,3}),c{0,1},d{0,∞})", Some(true)), // 02806: Xerces OK
    ("seq{1,2}(choice{0,3}(a{0,3},choice{1,3}(b{2,3},c{0,2},d)))", "seq{1,2}(choice{0,3}(choice{1,3}(c{0,2})))", Some(true)), // 02815: Xerces OK
    ("seq{0,∞}(choice{0,1}(seq{2,2}(a{1,∞})))", "seq{0,∞}(choice{0,1}(seq{2,2}(a)))", None), // 03249: Xerces OK (1.0 not asserted: rejected before and now)
    ("seq{1,3}(seq{0,1}(choice{0,1}(a{1,2},b{1,3}),seq{1,∞}(c{0,∞})),seq{0,∞}(seq{0,1}(d{3,3})))", "seq{1,3}(seq{0,∞}(seq{0,1}(d{3,3})))", Some(true)), // 03264: Xerces OK
    ("choice{1,∞}(choice{3,3}(seq{0,1}(a{1,3}),seq{0,1}(b{2,2},c{3,3},d{0,2})))", "choice{1,∞}(choice{3,3}(seq{0,1}(b{2,2},c{3,3},d{0,2})))", None), // 03341: Xerces OK (1.0 not asserted: rejected before and now)
    ("seq{1,3}(choice{0,2}(choice{0,1}(a{3,3},b{3,3},c)),seq{3,3}(seq{3,∞}(d{0,1})))", "seq{1,3}(choice{0,2}(choice{0,1}(b{3,3})),seq{3,3}(seq{3,∞}(d{0,1})))", Some(true)), // 03499: Xerces OK
    ("choice{1,2}(seq{2,2}(a{2,2}),seq{1,3}(seq{2,3}(b,c{2,3},d{0,1})))", "choice{1,2}(seq{2,2}(a{2,2}))", None), // 03577: Xerces OK (1.0 not asserted: rejected before and now)
    ("choice{2,3}(a{1,∞},seq{0,2}(b{1,2}),c{1,2})", "choice{2,3}(seq{0,2}(b{1,2}))", None), // 03736: Xerces OK (1.0 not asserted: rejected before and now)
    ("seq{0,1}(choice{0,1}(choice(a{0,∞},b{1,3},c{1,2}),choice{3,3}(d{1,2})))", "seq{0,1}(choice{0,1}(a{0,1},b{1,3},c{1,2},choice{3,3}(d{1,2})))", Some(true)), // 03856: Xerces OK
    ("choice{0,1}(choice{2,∞}(choice{1,2}(a{1,2},b{1,2},c{3,∞})),d)", "choice{0,1}(choice{2,∞}(choice{2,2}(a{1,2},b{1,2},c{3,∞})),d)", Some(true)), // 04597: Xerces OK
    ("choice{1,2}(seq{3,3}(a{1,2}))", "choice{1,2}(seq{3,3}(a{1,2}))", Some(true)), // 04634: Xerces OK
    ("seq{2,3}(seq{0,1}(seq{3,3}(a{2,3})))", "seq{3,3}(seq{0,1}(seq{3,3}(a{2,3})))", Some(true)), // 04711: Xerces OK
    ("seq{1,2}(choice{0,1}(choice{0,1}(a,b{2,2},c{2,3})))", "seq{1,2}(choice{0,1}(choice{0,1}(c{2,3})))", None), // 04776: Xerces OK (1.0 not asserted: rejected before and now)
    ("choice{1,2}(choice{1,3}(seq{1,2}(a{1,3}),choice{0,3}(b{1,2},c{0,2},d{1,2})))", "choice{1,2}(choice{1,3}(seq{1,2}(a{1,3})))", None), // 04792: Xerces OK (1.0 not asserted: rejected before and now)
    ("seq{2,2}(a{2,2},choice{1,∞}(choice{3,3}(b{0,1},c,d{1,3})))", "seq{2,2}(a{2,2},choice{1,∞}(choice{3,3}(c,d{1,3})))", Some(true)), // 04806: Xerces OK
    ("seq{0,1}(choice{0,2}(a{2,2},choice{0,∞}(b{1,2},c{0,1}),d{0,1}))", "seq(choice{0,2}(a{2,2},choice{0,∞}(b{1,2},c{0,1}),d{0,1}))", Some(false)), // 04844: Xerces cos-particle-restrict.2
    ("choice{0,3}(seq{2,2}(seq{2,2}(a{0,1},b{3,3})))", "choice{0,3}(seq{2,2}(seq{2,2}(a{0,1},b{3,3})))", Some(true)), // 05100: Xerces OK
    ("seq(choice{1,∞}(a{2,3}),choice{2,2}(choice{0,1}(b{2,3},c{0,1},d{2,2})))", "seq(choice{1,∞}(a{2,3}),choice{2,2}(choice{0,1}(b{2,3})))", Some(true)), // 05150: Xerces OK
    ("seq{0,1}(a{0,1},choice{2,3}(seq{0,1}(b{2,2},c{0,1}),seq{3,∞}(d{2,2})))", "seq{0,1}(a{0,1},choice{2,3}(seq{3,∞}(d{2,2})))", None), // 05271: Xerces OK (1.0 not asserted: rejected before and now)
    ("choice{0,1}(choice{3,3}(choice{2,3}(a{0,1},b{0,1},c{2,3}),seq(d{0,1})))", "choice(choice{3,3}(choice{2,3}(a{0,1},b{0,1},c{2,3}),seq(d{0,1})))", None), // 05306: Xerces rcase-RecurseLax.1 (1.0 not asserted: accepted before and now)
    ("seq{0,1}(choice{0,∞}(a{2,3}))", "seq(choice{0,∞}(a{2,3}))", None), // 05316: Xerces cos-particle-restrict.2 (1.0 not asserted: accepted before and now)
    ("seq{3,3}(seq{2,3}(seq{3,3}(a{1,3})),b{2,2},choice{0,1}(c{0,1}))", "seq{3,3}(seq{2,3}(seq{3,3}(a{1,3})),b{2,2},choice{0,1}(c{0,1}))", Some(true)), // 05342: Xerces OK
    ("seq(a{2,2},choice{0,∞}(choice{1,2}(b{0,1},c{0,1},d{1,2})))", "seq(a{2,2},choice{0,∞}(choice{2,2}(b{0,1},c{0,1},d{1,2})))", Some(true)), // 05373: Xerces OK
    ("choice{1,3}(choice{2,3}(seq(a,b{2,3},c{1,∞}),seq{0,2}(d{3,3})))", "choice{1,3}(choice{2,3}(seq{0,2}(d{3,3})))", None), // 05436: Xerces OK (1.0 not asserted: rejected before and now)
    ("seq(a{1,3},choice{2,3}(choice{2,2}(b{0,1},c{0,2})))", "seq(a{1,3},choice{2,3}(choice{2,2}(b{0,1})))", Some(true)), // 05661: Xerces OK
    ("choice{1,∞}(seq(seq(a{2,3},b{1,∞})),choice{0,1}(seq{3,3}(c{0,1},d{0,1})))", "choice{1,∞}(seq(seq(a{2,3},b{1,∞})),choice(seq{3,3}(c{0,1},d{0,1})))", None), // 05729: Xerces rcase-RecurseLax.2 (1.0 not asserted: accepted before and now)
    ("seq{0,3}(choice{0,∞}(seq{2,∞}(a{3,3})),b{2,2},seq{2,2}(c{3,3},d))", "seq{0,3}(choice{0,∞}(seq{2,∞}(a{3,3})),b{2,2},seq{2,2}(c{3,3},d))", Some(true)), // 05807: Xerces OK
    ("seq{0,1}(a{1,3},seq{2,3}(seq{3,3}(b{1,2})))", "seq{0,1}(a,seq{2,3}(seq{3,3}(b{1,2})))", Some(true)), // 05886: Xerces OK
    ("seq{0,∞}(choice{1,3}(a{3,3},choice{0,∞}(b{3,3})))", "seq{0,∞}(choice{1,3}(choice{0,∞}(b{3,3})))", Some(true)), // 05909: Xerces OK
    ("choice{0,1}(seq{1,2}(a{1,3}))", "choice{0,1}(seq{2,2}(a{1,3}))", Some(true)), // 06021: Xerces OK
    ("choice{0,2}(choice{0,3}(choice{0,1}(a{1,3},b{1,2},c),choice{0,1}(d{1,2})))", "choice{0,2}(choice{0,3}(choice{0,1}(a{1,3})))", Some(true)), // 06233: Xerces OK
    ("seq{0,1}(a{0,∞},seq(b{3,3},c{1,∞}),seq{0,1}(seq{2,2}(d{1,2})))", "seq{0,1}(seq(b{3,3},c{1,∞}),seq{0,1}(seq{2,2}(d{1,2})))", Some(true)), // 06256: Xerces OK
    ("choice{2,3}(choice{3,3}(seq{3,3}(a{1,2})))", "choice{2,3}(choice{3,3}(seq{3,3}(a{1,2})))", Some(true)), // 06359: Xerces OK
    ("choice{0,∞}(a{1,3},choice{3,3}(choice{0,∞}(b{0,1},c{2,3},d{2,2})))", "choice{0,∞}(a{1,3},choice{3,3}(choice{0,∞}(c{2,3})))", Some(true)), // 06521: Xerces OK
    ("seq{3,∞}(a{0,1},choice{1,2}(seq{2,3}(b{1,2},c{1,2},d{1,2})))", "seq{3,∞}(choice{1,2}(seq{2,3}(b{1,2},c{1,2},d{1,2})))", Some(true)), // 06640: Xerces OK
    ("choice{0,1}(choice{0,∞}(a{3,3},seq{0,3}(b{1,2},c{0,1})))", "choice{0,1}(choice{2,2}(a{3,3},seq{0,3}(b{1,2},c{0,1})))", Some(true)), // 06682: Xerces OK
    ("seq(a{2,∞},choice{0,1}(choice{1,∞}(b{1,∞},c),seq{1,2}(d{1,3})))", "seq(a{2,∞},choice{0,1}(seq{1,2}(d{1,3})))", None), // 06714: Xerces OK (1.0 not asserted: rejected before and now)
    ("choice{0,1}(seq{2,3}(choice{1,2}(a),choice{1,∞}(b{0,1})),choice{0,1}(seq{3,∞}(c{1,2})),d{0,1})", "choice{0,1}(seq{2,3}(choice{1,2}(a),choice{1,∞}(b{0,1})),choice{0,1}(seq{3,∞}(c{1,2})),d{0,1})", Some(true)), // 06765: Xerces OK
    ("choice{0,∞}(seq{1,2}(a{0,1},choice{0,1}(b{3,∞})),c{2,3})", "choice{0,∞}(seq{1,2}(a{0,1},choice{0,1}(b{3,∞})))", Some(true)), // 06803: Xerces OK
    ("seq{0,1}(seq{2,3}(seq{2,3}(a,b{1,3})))", "seq{0,1}(seq{2,3}(seq{2,3}(a,b{1,3})))", Some(true)), // 07053: Xerces OK
    ("choice{0,1}(choice{2,3}(seq{3,3}(a{1,∞})))", "choice{0,1}(choice{2,3}(seq{3,3}(a{1,∞})))", Some(true)), // 07066: Xerces OK
    ("choice{1,∞}(choice{1,∞}(a{1,3},b{2,2}))", "choice{1,∞}(choice{2,3}(a{1,3},b{2,2}))", Some(true)), // 07278: Xerces OK
    ("choice{0,1}(choice{0,3}(a{3,3}),choice{1,∞}(seq{1,2}(b{2,∞},c{0,3}),d{1,2}))", "choice{0,1}(choice{0,3}(a{3,3}))", Some(true)), // 07290: Xerces OK
    ("choice{1,∞}(choice{1,∞}(choice{2,∞}(a{3,3})),b{0,1},c{0,1})", "choice{1,∞}(choice{1,∞}(choice{2,2}(a{3,3})),b{0,1},c{0,1})", Some(true)), // 07385: Xerces OK
    ("choice(a{1,2},choice{0,1}(b{1,∞},seq{2,3}(c{0,1},d)))", "choice(choice{0,1}(b{1,∞},seq{2,3}(c{0,1},d)))", Some(false)), // 07403: Xerces rcase-RecurseLax.1
    ("choice{0,1}(seq{1,2}(seq{1,2}(a{3,3})),b{0,1})", "choice{0,1}(seq{1,2}(seq{1,2}(a{3,3})))", None), // 07667: Xerces OK (1.0 not asserted: rejected before and now)
    ("choice{1,∞}(choice{0,1}(seq{2,3}(a,b{1,2},c{0,3}),seq{2,2}(d{1,2})))", "choice{1,∞}(choice{0,1}(seq{2,3}(a,b{1,2},c{0,3})))", None), // 07927: Xerces OK (1.0 not asserted: rejected before and now)
    ("choice(a{0,∞},choice{0,1}(b{3,3}))", "choice(choice{0,1}(b{3,3}))", None), // 08048: Xerces rcase-RecurseLax.1 (1.0 not asserted: accepted before and now)
    ("choice{0,1}(choice{2,3}(choice(a,b{2,3}),c{0,1},choice(d{0,1})))", "choice{0,1}(choice{2,3}(b{2,3}))", Some(true)), // 08286: Xerces OK
    ("seq(choice{0,1}(a{1,3}),seq{1,2}(choice(b{1,2},c{0,1})))", "seq(seq{1,2}(choice(b{1,2},c{0,1})))", Some(false)), // 08321: Xerces rcase-Recurse.1
    ("choice{1,∞}(choice{0,∞}(a,b{2,2}))", "choice{1,∞}(choice{2,∞}(a,b{2,2}))", Some(true)), // 08425: Xerces OK
    ("choice{1,3}(choice{0,∞}(seq{2,2}(a{0,1},b{1,∞},c{0,2}),d{2,2}))", "choice{1,3}(choice{0,∞}(d{2,2}))", Some(true)), // 08442: Xerces OK
    ("choice{1,2}(choice{0,1}(a{1,3},b{2,2},seq{3,3}(c{1,2},d)))", "choice{1,2}(choice{0,1}(seq{3,3}(c{1,2},d)))", None), // 08457: Xerces OK (1.0 not asserted: rejected before and now)
    ("choice{1,3}(a{3,3},seq{1,2}(choice{0,2}(b{3,3},c{1,2},d{2,2})))", "choice(a{3,3},seq{1,2}(choice{0,2}(b{3,3})))", Some(true)), // 08468: Xerces OK
    ("choice{1,∞}(seq{2,2}(choice(a{0,1},b{2,∞})),seq{0,∞}(c{0,1},seq{2,2}(d{1,∞})))", "choice{1,∞}(seq{2,2}(a{0,1},b{2,∞}))", Some(false)), // 08542: Xerces rcase-RecurseLax.2
    ("choice{1,3}(a{0,1},choice{2,3}(seq{1,3}(b{3,3})),choice(c{0,∞},choice{1,2}(d)))", "choice{1,3}(choice{2,3}(seq{1,3}(b{3,3})))", None), // 08582: Xerces OK (1.0 not asserted: rejected before and now)
    ("choice{1,∞}(seq{0,2}(choice{2,2}(a{2,3}),b{0,1}),choice{0,1}(choice{1,2}(c{0,1},d{2,3})))", "choice{1,∞}(seq{0,2}(choice{2,2}(a{2,3}),b{0,1}),choice{0,1}(choice{1,2}(d{2,3})))", Some(true)), // 08652: Xerces OK
    ("choice{0,∞}(seq{2,2}(a{2,3}))", "choice{0,∞}(seq{2,2}(a{2,3}))", Some(true)), // 08669: Xerces OK
    ("choice{0,∞}(seq{3,3}(seq{1,2}(a{1,3},b{2,3},c)))", "choice{0,∞}(seq{3,3}(seq{1,2}(a{1,3},b{2,3},c)))", Some(true)), // 08800: Xerces OK
    ("choice{2,2}(choice{0,∞}(seq{1,2}(a{1,2},b{0,3},c{1,∞}),choice{1,∞}(d{3,∞})))", "choice{2,2}(choice{0,∞}(choice{1,∞}(d{3,∞})))", Some(true)), // 08808: Xerces OK
    ("choice{0,1}(seq{2,2}(seq{1,∞}(a{3,∞})))", "choice{0,1}(seq{2,2}(seq{1,∞}(a{3,∞})))", Some(true)), // 08822: Xerces OK
    ("seq{1,2}(choice{2,3}(a,seq{0,1}(b),seq{0,1}(c{1,2},d{2,3})))", "seq{1,2}(choice{2,3}(seq{0,1}(b)))", Some(true)), // 08900: Xerces OK
    ("choice{1,∞}(choice{2,3}(a{0,1},seq{0,3}(b{2,3},c{0,1},d)))", "choice{1,∞}(choice{2,3}(seq{0,3}(b{2,3},c{0,1},d)))", None), // 08931: Xerces OK (1.0 not asserted: rejected before and now)
    ("seq(choice{0,∞}(a{1,2}),choice{0,1}(b{2,2},c{0,1},d{3,3}))", "seq(choice{0,1}(b{2,2},d{3,3}))", Some(false)), // 08956: Xerces cos-particle-restrict.2
    ("seq(seq{1,∞}(seq(a{1,∞},b)),choice{1,2}(choice{0,∞}(c{1,∞},d{3,3})))", "seq(seq{1,∞}(seq(a{1,∞},b)),choice{1,2}(choice{0,∞}(d{3,3})))", Some(true)), // 09009: Xerces OK
    ("seq{0,2}(choice{2,2}(a{1,3},b))", "seq{2,2}(choice{2,2}(a{1,3},b))", Some(true)), // 09018: Xerces OK
    ("choice{0,1}(a{0,1},seq{3,3}(seq{3,3}(b,c{1,3},d{2,2})))", "choice(seq{3,3}(seq{3,3}(b,c{1,3},d{2,2})))", Some(false)), // 09137: Xerces rcase-MapAndSum.2
    ("seq{1,∞}(choice{0,1}(seq{0,2}(a),b{2,3},seq{2,3}(c,d{0,1})))", "seq{1,∞}(choice{0,1}(b{2,3}))", None), // 09268: Xerces OK (1.0 not asserted: rejected before and now)
    ("choice{3,3}(choice{1,3}(a{0,3}),seq{0,∞}(choice{0,1}(b{3,3},c{0,∞},d{0,1})))", "choice{3,3}(choice{1,3}(a{0,3}),seq{0,∞}(choice{0,1}(b{3,3})))", None), // 09358: Xerces OK (1.0 not asserted: rejected before and now)
    ("seq{1,2}(choice{2,2}(seq{2,2}(a{1,∞},b{1,2}),choice{0,1}(c{0,1},d{1,3})))", "seq{1,2}(choice{2,2}(seq{2,2}(a{1,∞},b{1,2})))", None), // 09360: Xerces OK (1.0 not asserted: rejected before and now)
    ("choice(choice{0,2}(a,seq{2,∞}(b{1,∞},c{0,1}),d{0,1}))", "choice(choice{0,2}(a,seq{2,∞}(b{1,∞}),d{0,1}))", Some(true)), // 09363: Xerces OK
    ("choice{1,2}(seq{1,3}(choice{1,2}(a)))", "choice{1,2}(seq{3,3}(choice{2,2}(a)))", Some(true)), // 09388: Xerces OK
    ("choice{1,3}(choice{2,3}(a{2,2},choice{0,1}(b,c{3,3},d{3,3})))", "choice{1,3}(choice{2,3}(choice{0,1}(b,c{3,3},d{3,3})))", None), // 09471: Xerces OK (1.0 not asserted: rejected before and now)
    ("choice{1,3}(seq{2,2}(seq{3,3}(a{2,∞},b,c{2,2})))", "choice{1,3}(seq{2,2}(seq{3,3}(a{2,∞},b,c{2,2})))", Some(true)), // 09719: Xerces OK
    ("choice{1,∞}(choice(seq{0,1}(a{1,3},b{1,2},c{3,3}),choice{2,3}(d{2,2})))", "choice{1,∞}(choice(choice{2,3}(d{2,2})))", Some(true)), // 09823: Xerces OK
    ("seq{0,3}(choice{1,∞}(a{0,1}),choice{2,2}(choice{2,3}(b{0,∞}),seq{0,3}(c{1,2})))", "seq{0,3}(choice{1,∞}(a{0,1}),choice{2,2}(seq{0,3}(c{1,2})))", None), // 09862: Xerces OK (1.0 not asserted: rejected before and now)
    ("choice{3,3}(choice{2,3}(seq{0,1}(a{2,3},b{0,3},c),seq{0,3}(d{1,2})))", "choice{3,3}(choice{2,3}(seq{0,3}(d{1,2})))", None), // 09938: Xerces OK (1.0 not asserted: rejected before and now)
    ("choice{3,3}(seq{0,∞}(a{0,1},seq{2,3}(b{0,1},c{0,1})),d{1,2})", "choice{3,3}(seq{0,∞}(seq{2,3}(b{0,1},c{0,1})),d{1,2})", Some(true)), // 10234: Xerces OK
    ("seq{0,∞}(choice{2,2}(a{0,1},b{1,∞},choice{1,2}(c{0,1})))", "seq{0,∞}(choice{2,2}(a{0,1}))", Some(true)), // 10368: Xerces OK
    ("choice{0,1}(choice(choice(a{3,3},b{3,3},c),seq{1,2}(d{3,∞})))", "choice{0,1}(choice(a{3,3},b{3,3},c,seq{1,2}(d{3,∞})))", Some(true)), // 10652: Xerces OK
    ("choice{3,∞}(a{0,1},seq{0,1}(seq{2,3}(b{1,2},c{1,3}),seq{0,1}(d)))", "choice{3,∞}(a{0,1},seq{0,1}(seq{2,3}(b{1,2},c{1,3})))", Some(true)), // 10693: Xerces OK
    ("choice{0,2}(seq{0,1}(seq{2,2}(a{2,2})),choice{1,3}(b),seq{3,3}(c{0,2}))", "choice(seq{0,1}(seq{2,2}(a{2,2})),choice{1,3}(b),seq{3,3}(c{0,2}))", Some(true)), // 10919: Xerces OK
    ("choice(choice{3,∞}(seq{2,2}(a{3,3})),choice{0,3}(choice(b{1,2},c{0,1}),d))", "choice(choice{3,∞}(seq{2,2}(a{3,3})))", Some(false)), // 10942: Xerces rcase-RecurseLax.1
    ("seq{1,∞}(seq{3,3}(a{1,∞}))", "seq{1,∞}(seq{3,3}(a{1,∞}))", Some(true)), // 11003: Xerces OK
    ("choice(a{0,1},choice{1,2}(b{3,∞}),c{3,3})", "choice(choice{1,2}(b{3,∞}))", None), // 11013: Xerces rcase-RecurseLax.1 (1.0 not asserted: accepted before and now)
    ("seq{1,2}(choice{3,3}(a{2,3},b))", "seq{2,2}(choice{3,3}(a{2,3},b))", Some(true)), // 11163: Xerces OK
    ("seq{0,1}(choice{1,3}(seq{3,3}(a{1,3})),b{0,1})", "seq(choice{1,3}(seq{3,3}(a{1,3})),b{0,1})", Some(true)), // 11215: Xerces OK
    ("choice{2,3}(a{3,3},choice{0,∞}(choice{2,3}(b{2,2},c{0,3})),seq{0,1}(seq(d)))", "choice{2,3}(a{3,3},choice{0,∞}(choice{2,3}(b{2,2})),seq{0,1}(seq(d)))", Some(true)), // 11239: Xerces OK
    ("seq(seq{2,3}(choice{1,2}(a{2,2})),choice{0,1}(b{1,∞},choice{1,∞}(c{1,2},d)))", "seq(seq{2,3}(choice{1,2}(a{2,2})))", Some(false)), // 11368: Xerces rcase-Recurse.1
    ("choice{0,1}(seq{0,1}(seq{1,∞}(a{1,2}),choice{0,∞}(b{0,∞})),seq{3,3}(seq{2,3}(c{1,∞},d{2,3})))", "choice{0,1}(seq{3,3}(seq{2,3}(c{1,∞},d{2,3})))", None), // 11371: Xerces OK (1.0 not asserted: rejected before and now)
    ("choice{2,3}(choice{1,2}(choice{1,∞}(a{1,3},b{2,3},c{2,3})),seq{2,2}(d{0,∞}))", "choice{2,3}(choice{1,2}(choice{2,∞}(a{1,3},b{2,3},c{2,3})),seq{2,2}(d{0,∞}))", Some(true)), // 11431: Xerces OK
    ("choice(choice{0,1}(seq{3,3}(a{3,3})))", "choice(choice{0,1}(seq{3,3}(a{3,3})))", Some(true)), // 11509: Xerces OK
    ("choice{2,2}(a{2,∞},seq{0,1}(seq{2,2}(b{2,3})),choice{3,∞}(c{1,∞},choice(d{0,1})))", "choice{2,2}(a{2,∞},seq{0,1}(seq{2,2}(b{2,3})),choice{3,∞}(c{1,∞},choice(d{0,1})))", Some(true)), // 11613: Xerces OK
    ("seq{1,3}(choice{0,1}(seq{2,2}(a{1,2},b{1,3},c{2,2}),d{1,2}))", "seq{1,3}(choice{0,1}(seq{2,2}(a{1,2},b{1,3},c{2,2})))", None), // 11857: Xerces OK (1.0 not asserted: rejected before and now)
    ("seq{3,3}(a{3,∞},b{2,3},seq{1,3}(choice{2,2}(c,d{1,2})))", "seq{3,3}(a{3,∞},b{2,3},seq{1,3}(choice{2,2}(d{1,2})))", Some(true)), // 11903: Xerces OK
    ("seq{0,1}(a{2,∞},choice{0,2}(choice{0,∞}(b{3,3},c{0,1},d)))", "seq{0,1}(a{2,∞},choice{0,2}(choice{2,∞}(b{3,3},d)))", Some(true)), // 12047: Xerces OK
    ("seq{2,2}(choice{2,3}(choice{0,1}(a{2,∞},b{1,∞})),seq{0,1}(choice{2,2}(c{0,1}),choice{3,3}(d{0,1})))", "seq{2,2}(choice{2,3}(choice{0,1}(a{2,∞})),seq{0,1}(choice{2,2}(c{0,1}),choice{3,3}(d{0,1})))", Some(true)), // 12071: Xerces OK
    ("seq{3,3}(choice{3,3}(a{1,2},choice{0,3}(b{0,1},c{0,1})),choice{1,2}(seq{3,3}(d{2,2})))", "seq{3,3}(choice{3,3}(a{1,2},choice{0,3}(b{0,1},c{0,1})),choice{1,2}(seq{3,3}(d{2,2})))", Some(true)), // 12095: Xerces OK
    ("choice{2,3}(choice{2,3}(seq{0,1}(a{1,∞},b{0,∞},c),seq{0,1}(d{1,∞})))", "choice{2,3}(choice{2,3}(seq{0,1}(a{1,∞},b{0,∞},c)))", None), // 12135: Xerces OK (1.0 not asserted: rejected before and now)
    ("choice{0,3}(choice{3,3}(choice{0,1}(a{0,∞},b{1,2},c),seq(d{0,1})))", "choice{0,3}(choice{3,3}(seq(d{0,1})))", Some(true)), // 12213: Xerces OK
    ("seq(a,seq{3,3}(choice{3,3}(b,c{0,3},d{0,1})))", "seq(a,seq{3,3}(choice{3,3}(c{0,3})))", Some(true)), // 12454: Xerces OK
    ("seq{1,∞}(choice{3,3}(seq(a{3,3},b{3,3},c{0,∞}),d{0,1}))", "seq{1,∞}(choice{3,3}(d{0,1}))", Some(true)), // 12548: Xerces OK
    ("choice{1,3}(seq{0,1}(a{0,∞},seq{1,3}(b{1,2},c{1,2})),choice{0,1}(seq{1,2}(d{1,3})))", "choice{1,3}(choice{0,1}(seq{1,2}(d{1,3})))", None), // 12668: Xerces OK (1.0 not asserted: rejected before and now)
    ("seq{0,2}(choice{2,2}(a{1,∞},b{1,3}))", "seq{2,2}(choice{2,2}(a{1,∞}))", Some(true)), // 12712: Xerces OK
    ("choice{2,3}(seq{3,3}(a{1,2}))", "choice{2,3}(seq{3,3}(a{1,2}))", Some(true)), // 12814: Xerces OK
    ("choice{0,1}(choice(choice{1,∞}(a{3,3},b{2,3})))", "choice{0,1}(choice(choice{1,∞}(b{2,3})))", Some(true)), // 12879: Xerces OK
    ("choice{3,3}(choice{1,2}(seq{1,2}(a{1,2},b{1,∞},c{0,1}),seq{3,3}(d{3,3})))", "choice{3,3}(choice{1,2}(seq{3,3}(d{3,3})))", None), // 12985: Xerces OK (1.0 not asserted: rejected before and now)
    ("seq{1,2}(choice{1,3}(choice(a{2,3},b{2,∞})),choice{3,3}(choice{1,2}(c{3,3},d{0,1})))", "seq{1,2}(choice{1,3}(choice(a{2,3},b{2,∞})),choice{3,3}(choice{1,2}(c{3,3})))", Some(true)), // 13015: Xerces OK
    ("choice{1,∞}(seq{2,2}(a{1,∞}),choice{1,2}(seq{0,3}(b,c{1,3},d{1,2})))", "choice{1,∞}(seq{2,2}(a{1,∞}))", None), // 13168: Xerces OK (1.0 not asserted: rejected before and now)
    ("choice(seq{1,∞}(seq{3,3}(a{0,∞},b{0,1}),c{2,∞}),choice{3,3}(d{3,3}))", "choice(choice{3,3}(d{3,3}))", None), // 13193: Xerces rcase-RecurseLax.1 (1.0 not asserted: accepted before and now)
    ("choice{1,∞}(a{3,3},seq{2,2}(b{2,∞}),choice{1,2}(choice(c{1,3}),d{3,3}))", "choice{1,∞}(seq{2,2}(b{2,∞}))", None), // 13360: Xerces OK (1.0 not asserted: rejected before and now)
    ("choice{3,3}(a{0,1},choice{1,2}(seq{0,∞}(b{1,∞},c{0,1})),d)", "choice{3,3}(choice{1,2}(seq{0,∞}(b{1,∞})),d)", Some(true)), // 13386: Xerces OK
    ("choice{3,∞}(choice{0,1}(a{3,3},choice{0,1}(b{3,3},c{2,3},d{1,2})))", "choice{3,∞}(choice{0,1}(a{3,3}))", Some(true)), // 13546: Xerces OK
    ("choice{0,1}(seq{3,3}(choice{1,3}(a{1,3},b),c{3,3}),seq{1,2}(seq{3,3}(d{1,2})))", "choice{0,1}(seq{3,3}(choice{1,3}(b),c{3,3}),seq{1,2}(seq{3,3}(d{1,2})))", Some(true)), // 13665: Xerces OK
    ("seq{1,2}(seq{0,2}(seq{2,2}(a{2,∞})))", "seq{2,2}(seq{0,2}(seq{2,2}(a{2,∞})))", Some(true)), // 13718: Xerces OK
    ("choice{0,1}(choice{3,3}(seq{1,2}(a{1,∞})))", "choice{0,1}(choice{3,3}(seq{1,2}(a{2,3})))", Some(true)), // 13768: Xerces OK
    ("choice{2,3}(choice{0,2}(choice(a{3,3}),choice{1,∞}(b{2,2},c{3,∞},d{2,3})))", "choice{2,3}(choice{2,2}(choice(a{3,3})))", Some(true)), // 13858: Xerces OK
    ("choice{0,1}(choice{2,3}(seq{1,∞}(a{0,1},b{0,∞},c{0,2}),seq{1,∞}(d)))", "choice(choice{2,3}(seq{1,∞}(a{0,1},b{0,∞},c{0,2}),seq{1,∞}(d)))", None), // 13972: Xerces rcase-RecurseLax.1 (1.0 not asserted: accepted before and now)
    ("seq{0,1}(choice{1,3}(choice{2,2}(a{1,2},b{3,3}),c{2,∞}),d{1,∞})", "seq{0,1}(choice{1,3}(choice{2,2}(b{3,3})),d{1,∞})", Some(true)), // 14143: Xerces OK
    ("choice{2,2}(seq{1,2}(seq{1,2}(a{0,2},b{2,3},c{2,2})),seq{2,∞}(seq{2,2}(d{1,3})))", "choice{2,2}(seq{1,2}(seq{1,2}(a{2,2},b{2,3},c{2,2})),seq{2,∞}(seq{2,2}(d{1,3})))", Some(true)), // 14284: Xerces OK
    ("seq{0,1}(seq{3,∞}(choice{2,3}(a{2,3})))", "seq{0,1}(seq{3,∞}(choice{2,3}(a{3,3})))", Some(true)), // 14435: Xerces OK
    ("choice{1,∞}(seq{3,3}(choice(a{0,∞},b{1,2},c)),choice{1,2}(d{1,∞}))", "choice{1,∞}(seq{3,3}(a{0,∞},b{1,2},c),choice{1,2}(d{1,∞}))", Some(false)), // 14465: Xerces rcase-RecurseLax.2
    ("seq{1,∞}(a{2,2},choice{1,2}(seq{3,3}(b{1,∞}),seq{2,3}(c{0,1},d)))", "seq{1,∞}(a{2,2},choice{1,2}(seq{3,3}(b{1,∞})))", None), // 14474: Xerces OK (1.0 not asserted: rejected before and now)
    ("choice{0,1}(seq{0,1}(a{1,2},b{2,∞},c{1,∞}),seq{1,∞}(seq{2,2}(d{1,2})))", "choice{0,1}(seq{1,∞}(seq{2,2}(d{1,2})))", None), // 14521: Xerces OK (1.0 not asserted: rejected before and now)
    ("choice{0,2}(seq{0,1}(choice{2,3}(a,b{3,3})))", "choice{0,2}(seq{0,1}(choice{2,3}(b{3,3})))", Some(true)), // 14540: Xerces OK
    ("choice(seq{0,1}(a{0,1},choice{0,1}(b{0,3},c{2,3},d{3,3})))", "choice(seq{0,1}(choice{0,1}(b{0,3},c{2,3},d{3,3})))", None), // 14587: Xerces OK (1.0 not asserted: rejected before and now)
    ("choice{3,3}(seq{2,3}(a{1,2},seq{1,2}(b{1,2})),choice{2,2}(choice{0,1}(c{2,2}),d))", "choice{3,3}(seq{2,3}(a{1,2},seq{1,2}(b{1,2})),choice{2,2}(choice{0,1}(c{2,2})))", Some(true)), // 14858: Xerces OK
    ("choice{3,3}(choice(a{0,2},choice{1,∞}(b{2,2})),c{1,2},choice{1,3}(seq{3,3}(d{2,∞})))", "choice{3,3}(choice(choice{1,2}(b{2,2})),c{1,2},choice{1,3}(seq{3,3}(d{2,∞})))", Some(true)), // 14859: Xerces OK
    ("seq{2,2}(a{2,2},choice{1,3}(choice{1,2}(b{3,3},c,d{2,∞})))", "seq{2,2}(a{2,2},choice{1,3}(choice{1,2}(b{3,3})))", Some(true)), // 14877: Xerces OK
    ("choice{1,2}(seq{2,∞}(a{2,2}),seq{0,1}(seq{0,1}(b{1,3}),c{0,2},d{2,3}))", "choice{1,2}(seq{2,∞}(a{2,2}))", None), // 14990: Xerces OK (1.0 not asserted: rejected before and now)
    ("seq{0,2}(seq{0,1}(choice{1,2}(a{0,3},b{1,2},c{1,∞})),seq{0,1}(choice{2,∞}(d{2,2})))", "seq{0,2}(seq{0,1}(choice{1,2}(a{0,3})),seq{0,1}(choice{2,∞}(d{2,2})))", Some(true)), // 15064: Xerces OK
    ("seq{1,∞}(choice{0,1}(choice{0,∞}(a{3,3},b{3,∞},c{2,3}),seq{2,3}(d{1,2})))", "seq{1,∞}(choice{0,1}(seq{2,3}(d{1,2})))", None), // 15083: Xerces OK (1.0 not asserted: rejected before and now)
    ("choice{2,3}(choice{1,3}(choice{3,3}(a{1,∞},b{3,3},c{1,3}),choice{2,2}(d{2,2})))", "choice{2,3}(choice{1,3}(choice{3,3}(a{1,2},b{3,3},c{1,3})))", None), // 15129: Xerces OK (1.0 not asserted: rejected before and now)
    ("seq{3,3}(choice{2,∞}(a{0,1},b{2,2},c))", "seq{3,3}(choice{2,∞}(a{0,1}))", Some(true)), // 15592: Xerces OK
    ("choice{1,∞}(seq(choice{2,2}(a{0,1},b{0,1})),c{0,1})", "choice{1,∞}(seq(choice{2,2}(b{0,1})))", Some(true)), // 15652: Xerces OK
    ("choice{0,1}(choice{2,∞}(a{1,3},choice{2,3}(b{3,3},c{1,∞},d{0,3})))", "choice{0,1}(choice{2,∞}(a{1,3},choice{2,3}(b{3,3},c{1,∞})))", Some(true)), // 15684: Xerces OK
    ("choice{1,∞}(choice{0,1}(seq{1,3}(a{3,3},b{2,∞},c{0,1}),d{3,∞}))", "choice{1,2}(choice{0,1}(d{3,∞}))", Some(true)), // 15703: Xerces OK
    ("seq{0,1}(choice{1,2}(a{1,2},choice{3,∞}(b{2,3},c{1,2})))", "seq{0,1}(choice{1,2}(choice{3,∞}(c{1,2})))", Some(true)), // 15709: Xerces OK
    ("choice{1,2}(seq{1,2}(seq{3,3}(a{2,3},b)),seq{0,∞}(seq{0,3}(c{0,1})),choice{2,2}(choice(d{3,∞})))", "choice{1,2}(seq{2,2}(seq{3,3}(a{2,3},b)),seq{0,∞}(seq{0,3}(c{0,1})),choice{2,2}(choice(d{3,∞})))", Some(true)), // 15718: Xerces OK
    ("seq{1,2}(seq{0,1}(choice{1,∞}(a{1,2})))", "seq{1,2}(seq{0,1}(choice{1,∞}(a{2,2})))", Some(true)), // 15730: Xerces OK
    ("choice{0,1}(choice{2,3}(a{1,∞},choice{1,2}(b{2,3},c{0,1},d{1,2})))", "choice{0,1}(choice{2,3}(a{1,∞},choice{1,2}(d{1,2})))", Some(true)), // 15795: Xerces OK
    ("choice{3,∞}(seq{1,2}(seq(a{3,3})))", "choice{3,∞}(seq{2,2}(seq(a{3,3})))", Some(true)), // 15877: Xerces OK
    ("seq{0,2}(seq{0,1}(choice{0,3}(a{1,3},b{1,2})),seq{1,2}(c{0,∞}),choice{1,∞}(seq{3,3}(d{2,2})))", "seq{0,1}(seq{0,1}(choice{0,3}(a{1,3},b{1,2})),seq{1,2}(c{0,∞}),choice{1,∞}(seq{3,3}(d{2,2})))", Some(true)), // 15941: Xerces OK
    ("choice{2,2}(a{1,3},choice{1,3}(b{2,3},c{0,∞}),seq(seq{3,3}(d{2,2})))", "choice{2,2}(seq(seq{3,3}(d{2,2})))", None), // 15948: Xerces OK (1.0 not asserted: rejected before and now)
    ("seq{0,3}(choice{1,2}(a{1,∞},b,seq{3,3}(c{0,∞},d{2,3})))", "seq{0,3}(choice{1,2}(seq{3,3}(c{0,∞},d{2,3})))", None), // 15949: Xerces OK (1.0 not asserted: rejected before and now)
    ("choice{0,2}(seq{3,3}(a{1,2}))", "choice{0,2}(seq{3,3}(a{1,2}))", Some(true)), // 15956: Xerces OK
    ("seq(seq{1,3}(a{1,2},b{1,2}),choice{0,1}(choice{2,3}(c{0,∞},d)))", "seq(seq{1,3}(a{1,2},b{1,2}),choice{0,1}(choice{2,3}(d)))", Some(true)), // 15977: Xerces OK
    ("choice{1,3}(seq{1,3}(seq{0,∞}(a{0,1}),seq{0,3}(b{0,1},c{0,1})),choice{0,3}(choice{0,1}(d{2,3})))", "choice{1,3}(choice{0,3}(choice{0,1}(d{2,3})))", Some(true)), // 16020: Xerces OK
    ("choice{1,2}(choice{2,2}(a,b{2,2},seq{3,∞}(c{1,3},d{0,1})))", "choice{1,2}(choice{2,2}(seq{3,∞}(c{1,3},d{0,1})))", None), // 16072: Xerces OK (1.0 not asserted: rejected before and now)
    ("seq{0,2}(choice{0,∞}(seq{0,1}(a{1,2},b{1,∞},c),d{3,3}))", "seq{0,2}(choice{0,∞}(d{3,3}))", Some(true)), // 16185: Xerces OK
    ("choice{0,3}(choice{0,1}(choice{2,3}(a{1,∞}),seq{0,∞}(b{1,3},c{0,3},d{3,3})))", "choice{0,3}(choice{0,1}(choice{2,3}(a{1,∞})))", Some(true)), // 16212: Xerces OK
    ("choice{2,2}(seq(seq{1,2}(a{0,1})),seq{0,3}(seq{2,2}(b{3,3})))", "choice{2,2}(seq(seq{1,2}(a{0,1})),seq{0,3}(seq{2,2}(b{3,3})))", Some(true)), // 16225: Xerces OK
    ("seq{1,3}(choice{2,2}(seq(a,b{2,3},c{1,2}),d{0,1}))", "seq{1,3}(choice{2,2}(d{0,1}))", Some(true)), // 16274: Xerces OK
    ("seq{0,1}(a{0,1},seq{2,2}(choice{0,3}(b{3,3})))", "seq{0,1}(a{0,1},seq{2,2}(choice{2,2}(b{3,3})))", Some(true)), // 16292: Xerces OK
    ("choice{0,∞}(choice{2,3}(a{0,1},seq{1,2}(b{0,2})))", "choice{0,∞}(choice{2,3}(seq{1,2}(b{0,2})))", Some(true)), // 16296: Xerces OK
    ("choice{1,2}(choice{0,3}(a{1,∞},seq{0,∞}(b{3,3}),seq{3,3}(c{2,3})),choice{1,2}(d{2,∞}))", "choice{1,2}(choice{1,2}(d{2,∞}))", Some(true)), // 16412: Xerces OK
    ("choice{0,3}(choice{3,3}(choice{2,2}(a{0,∞},b{0,3}),c,choice(d{2,3})))", "choice{0,3}(choice{3,3}(choice{2,2}(b{2,3}),c,choice(d{2,3})))", Some(true)), // 16451: Xerces OK
    ("choice{0,1}(choice{2,3}(seq{0,1}(a,b),choice{1,3}(c)))", "choice{0,1}(choice{2,3}(choice{1,3}(c)))", Some(true)), // 16850: Xerces OK
    ("choice(choice(seq{1,3}(a{3,3},b,c{0,1})),choice{1,3}(d{2,3}))", "choice(choice{1,3}(d{2,3}))", None), // 16862: Xerces rcase-RecurseLax.1 (1.0 not asserted: accepted before and now)
    ("choice{0,∞}(choice{3,3}(seq{0,∞}(a{2,3},b{0,∞},c{1,2}),seq(d)))", "choice{0,∞}(choice{3,3}(seq(d)))", Some(true)), // 17118: Xerces OK
    ("choice{1,2}(choice(seq{2,2}(a{1,2},b{0,1})),choice{2,2}(c{3,3},choice{1,2}(d{2,2})))", "choice{1,2}(choice(seq{2,2}(a{1,2},b{0,1})),choice{2,2}(choice{1,2}(d{2,2})))", Some(true)), // 17193: Xerces OK
    ("seq{0,1}(choice{0,∞}(choice{1,∞}(a{3,3},b{0,∞})),seq{0,1}(seq{1,∞}(c{2,2}),choice{0,∞}(d{1,∞})))", "seq{0,1}(choice{0,∞}(choice{1,∞}(a{3,3})),seq{0,1}(seq{1,∞}(c{2,2}),choice{0,∞}(d{1,∞})))", Some(true)), // 17272: Xerces OK
    ("choice{1,2}(choice{0,1}(choice(a{2,3},b{3,∞}),c{2,∞}),choice{1,3}(choice{0,1}(d{1,3})))", "choice{1,2}(choice{0,1}(a{2,3},b{3,∞},c{2,∞}),choice{1,3}(choice{0,1}(d{1,3})))", Some(true)), // 17324: Xerces OK
    ("seq{0,1}(choice{1,∞}(seq{2,3}(a{2,∞})))", "seq{0,1}(choice{1,∞}(seq{2,3}(a{2,∞})))", Some(true)), // 17373: Xerces OK
    ("seq(choice{0,2}(choice{1,3}(a{2,2},b{2,3})))", "seq(choice{1,2}(choice{1,3}(a{2,2})))", Some(true)), // 17425: Xerces OK
    ("choice(choice{1,3}(a{3,3}),choice{0,1}(b{0,∞}))", "choice(choice{1,3}(a{3,3}))", None), // 17466: Xerces rcase-RecurseLax.1 (1.0 not asserted: accepted before and now)
    ("choice{1,2}(choice{0,2}(seq(a{1,3}),choice{1,2}(b{2,3},c{0,∞},d)))", "choice{1,2}(choice{2,2}(seq(a{1,3}),choice{1,2}(b{2,3},c{0,∞},d)))", Some(true)), // 17596: Xerces OK
    ("choice{1,2}(choice{3,3}(choice{1,∞}(a{1,2}),seq{0,∞}(b,c{0,1},d{0,1})))", "choice(choice{3,3}(choice{1,∞}(a{1,2}),seq{0,∞}(b,c{0,1},d{0,1})))", None), // 17654: Xerces rcase-RecurseLax.1 (1.0 not asserted: accepted before and now)
    ("choice{1,2}(choice{2,∞}(seq{2,3}(a{1,2},b{2,2})),choice{0,1}(choice{3,3}(c{1,2},d{0,1})))", "choice{1,2}(choice{2,∞}(seq{2,3}(a{1,2},b{2,2})),choice{0,1}(choice{3,3}(c{1,2})))", Some(true)), // 17707: Xerces OK
    ("choice{2,2}(a{1,3},choice{0,1}(seq{3,3}(b{2,3})),choice{1,2}(choice(c{0,1}),choice{1,2}(d{3,∞})))", "choice{2,2}(a{1,3},choice{0,1}(seq{3,3}(b{2,3})),choice{1,2}(c{0,1},choice{1,2}(d{3,∞})))", Some(true)), // 17718: Xerces OK
    ("seq{1,3}(seq{3,3}(a{1,3}))", "seq{1,3}(seq{3,3}(a{1,3}))", Some(true)), // 17800: Xerces OK
    ("seq{0,1}(a{1,3},choice{1,2}(choice{2,2}(b{0,1},c{0,1})))", "seq{0,1}(a{1,3},choice{1,2}(choice{2,2}(c{0,1})))", Some(true)), // 18021: Xerces OK
    ("seq{1,3}(seq{2,2}(a{0,∞}))", "seq{1,3}(seq{2,2}(a{1,∞}))", Some(true)), // 18423: Xerces OK
    ("choice{1,2}(choice{0,1}(a{0,∞},b{2,3}))", "choice{2,2}(choice{0,1}(b{2,3}))", Some(true)), // 18459: Xerces OK
    ("choice{0,1}(a{0,1},choice{3,∞}(b{3,3}))", "choice(choice{3,∞}(b{3,3}))", None), // 18550: Xerces rcase-RecurseLax.1 (1.0 not asserted: accepted before and now)
    ("choice(seq{0,1}(choice(a{2,3},b)),c{0,1})", "choice(seq{0,1}(choice(a{2,3},b)))", Some(false)), // 18763: Xerces rcase-MapAndSum.2
    ("choice{2,3}(seq{3,3}(a{1,3}))", "choice{2,3}(seq{3,3}(a{1,3}))", Some(true)), // 18791: Xerces OK
    ("choice{2,2}(choice{2,2}(a,seq{0,1}(b{0,1})))", "choice{2,2}(choice{2,2}(seq(b{0,1})))", Some(true)), // 18959: Xerces OK
    ("seq{1,2}(a{1,∞},choice{2,3}(choice{1,2}(b{1,3},c{0,1},d{2,2})))", "seq{1,2}(a{1,∞},choice{2,3}(choice{1,2}(d{2,2})))", Some(true)), // 19007: Xerces OK
    ("choice{0,1}(seq{0,1}(seq{1,2}(a{0,2},b{0,1},c{1,∞})),choice{1,2}(d{2,3}))", "choice{0,1}(choice{1,2}(d{2,3}))", None), // 19069: Xerces OK (1.0 not asserted: rejected before and now)
    ("choice(choice{0,1}(a{0,3},choice(b{0,3},c{1,2},d{2,3})))", "choice(choice{0,1}(a{0,3},b{0,3},c{1,2},d{2,3}))", Some(true)), // 19090: Xerces OK
    ("seq{2,3}(seq{0,∞}(choice{1,2}(a{2,2},b{0,1})),seq{0,1}(seq{3,3}(c{1,∞},d)))", "seq{2,3}(seq{0,∞}(choice{1,2}(a{2,2})),seq{0,1}(seq{3,3}(c{1,∞},d)))", Some(true)), // 19101: Xerces OK
    ("choice{3,3}(a{0,1},b{0,1},seq{0,1}(seq{2,2}(c{1,3})))", "choice{3,3}(a{0,1},b{0,1},seq{0,1}(seq{2,2}(c{1,3})))", Some(true)), // 19125: Xerces OK
    ("seq{1,2}(choice{0,1}(choice{0,∞}(a{0,∞}),b{0,2},seq{2,3}(c{3,3},d)))", "seq{1,2}(choice{0,1}(seq{2,2}(c{3,3},d)))", None), // 19156: Xerces OK (1.0 not asserted: rejected before and now)
    ("seq{1,2}(seq{1,∞}(seq{3,3}(a{0,1},b{0,2},c{0,∞})),seq{2,3}(seq{3,3}(d{1,2})))", "seq{1,2}(seq{1,∞}(seq{3,3}(a{0,1},b{0,2},c{0,∞})),seq{2,3}(seq{3,3}(d{1,2})))", Some(true)), // 19318: Xerces OK
    ("seq{2,∞}(choice{0,1}(choice{2,2}(a{1,2},b{0,1})),c{1,2})", "seq{2,∞}(choice{0,1}(choice{2,2}(a{1,2})),c{1,2})", Some(true)), // 19360: Xerces OK
    ("choice{1,2}(choice{1,2}(choice{1,2}(a{0,1},b{0,1},c{0,1}),choice{1,2}(d{2,∞})))", "choice{1,2}(choice{2,2}(choice{1,2}(a{0,1},b{0,1},c{0,1}),choice{1,2}(d{2,∞})))", Some(true)), // 19481: Xerces OK
    ("choice{2,3}(seq{0,∞}(seq{1,∞}(a),b{3,3},c{2,2}),seq{2,2}(d{1,3}))", "choice{2,3}(seq{2,2}(d{2,2}))", None), // 19625: Xerces OK (1.0 not asserted: rejected before and now)
    ("seq{1,∞}(choice{0,1}(seq{2,∞}(a{1,∞},b,c{0,2}),d{0,1}))", "seq{1,∞}(choice{0,1}(seq{2,∞}(a{1,∞},b,c{0,2})))", None), // 19779: Xerces OK (1.0 not asserted: rejected before and now)
    ("choice{3,3}(seq(seq{0,3}(a{2,3}),choice{1,2}(b)),choice{0,1}(choice{0,1}(c{2,2})))", "choice{3,3}(choice{0,1}(choice{0,1}(c{2,2})))", Some(true)), // 19784: Xerces OK
    ("choice(choice{2,∞}(seq{1,3}(a{1,3})),choice{0,1}(b{0,∞},choice{2,2}(c{3,3})),d{1,3})", "choice(choice{0,1}(b{0,∞},choice{2,2}(c{3,3})))", Some(false)), // 19811: Xerces rcase-RecurseLax.1
    ("seq{0,2}(choice{2,∞}(a{3,3},b{0,∞},choice{0,1}(c{3,3})))", "seq{2,2}(choice{2,∞}(a{3,3}))", Some(true)), // 19846: Xerces OK
    ("choice{1,2}(seq{1,2}(a{0,1}),b{3,3},choice{0,1}(choice{2,3}(c{0,1},d{2,3})))", "choice{1,2}(seq{1,2}(a{0,1}),b{3,3},choice{0,1}(choice{2,3}(c,d{2,3})))", Some(true)), // 19869: Xerces OK
    ("seq{1,∞}(choice{2,3}(seq{0,1}(a{0,3},b),c{1,3}))", "seq{1,∞}(choice{2,3}(seq{0,1}(a{0,3},b)))", None), // 19972: Xerces OK (1.0 not asserted: rejected before and now)
    ("choice{0,∞}(a{1,2},seq{0,2}(choice{2,2}(b{0,1},c{2,2},d{0,1})))", "choice{0,1}(seq{0,2}(choice{2,2}(b{0,1},c{2,2},d{0,1})))", Some(true)), // 19976: Xerces OK
];

fn sweep_regression_rows_cases() -> Vec<Case> {
    SWEEP_ROWS
        .iter()
        .map(|&(base, restricted, v10)| {
            let mut case = Case::plain(
                &format!("{base} > {restricted}"),
                particle(base),
                particle(restricted),
                v10.unwrap_or(true),
                true,
            );
            case.valid[0] = v10;
            case
        })
        .collect()
}

#[test]
fn sweep_regression_rows() {
    check_all(&sweep_regression_rows_cases());
}

/// From a second sweep whose edits may break validity (widening a range,
/// swapping two particles, doubling a maximum): restrictions `d5f2a8e`
/// accepted that are invalid — each with a witness the restriction's content
/// accepts and the base rejects (the last two longer than the enumeration
/// reaches, checked as instances) — and two valid restrictions an
/// intermediate version refused. XSD 1.0: rejected, as Xerces-J rejects
/// them all.
#[rustfmt::skip]
const RISKY_ROWS: &[(&str, &str, bool, &[&str])] = &[
    ("seq{1,2}(choice{2,∞}(a{0,∞},choice{2,3}(b{2,3},c{1,2},d{2,2})))", "seq{1,2}(choice{2,∞}(a{0,∞},choice{2,3}(b{2,3},c{1,4},d{2,2})))", true, &[]), // 01006: oracle included (≤ 5), Xerces rcase-Recurse.2
    ("seq{2,2}(choice{1,∞}(choice{0,1}(a,b,c{2,3}),choice{1,3}(d{0,1})))", "seq{1,4}(choice{1,∞}(choice{0,1}(a,b,c{2,3}),choice{1,3}(d{0,1})))", true, &[]), // 05147: oracle included (≤ 5), Xerces rcase-Recurse.1
    ("choice{1,∞}(a{2,2},choice{1,3}(choice{0,3}(b{2,3}),c{0,1}))", "choice{1,∞}(a{2,2},choice{1,3}(choice{0,3}(b{1,4}),c{0,1}))", false, &["b"]), // 00395: oracle witness, Xerces rcase-RecurseLax.2
    ("seq{1,∞}(choice{1,3}(seq{0,1}(a),choice(b{1,∞},c{2,∞},d{2,2})))", "seq{1,∞}(choice{1,3}(seq{0,1}(a),choice(b{1,∞},c{2,∞},d{2,4})))", false, &["d", "d", "d"]), // 01583: oracle witness, Xerces rcase-Recurse.2
    ("choice{0,1}(a,choice{3,3}(seq{1,∞}(b{1,3}),choice{1,∞}(c{3,3},d{0,1})))", "choice{0,1}(a,choice{3,3}(seq{1,∞}(b{1,3}),choice{1,∞}(c{3,6},d{0,1})))", false, &["c", "c", "c", "c"]), // 02367: oracle witness, Xerces rcase-RecurseLax.2
    ("choice{0,∞}(choice{3,3}(a{0,3},b{1,2},c{2,3}))", "choice{0,∞}(choice{3,3}(a{0,3},b{1,2},c{1,5}))", false, &["c"]), // 02758: oracle witness, Xerces rcase-RecurseLax.2
    ("choice{2,∞}(a{0,1},seq{2,2}(b{1,3},choice{1,2}(c{0,∞},d{2,∞})))", "choice{2,2}(a{0,1},seq{2,4}(b{1,3},choice{1,2}(c{0,∞},d{2,∞})))", false, &["b", "c", "b", "c", "b"]), // 02856: oracle witness, Xerces rcase-RecurseLax.2
    ("choice{0,1}(seq{1,2}(choice{0,∞}(a{0,3}),choice{1,∞}(b{3,3})),choice{1,3}(c,seq{1,2}(d{3,3})))", "choice{0,1}(seq{1,2}(choice{0,∞}(a{0,3}),choice{1,∞}(b{3,6})),choice{1,3}(c,seq{1,2}(d{3,3})))", false, &["b", "b", "b", "b"]), // 03572: oracle witness, Xerces rcase-RecurseLax.2
    ("choice{1,3}(choice{0,1}(a{1,∞},choice{1,∞}(b{0,1},c{1,3},d{0,∞})))", "choice{1,∞}(choice{0,1}(a{1,∞},choice{1,∞}(b{0,1},c{1,3},d{0,∞})))", false, &["a", "b", "a", "b"]), // 04508: oracle witness, Xerces rcase-RecurseLax.1
    ("seq{0,1}(choice{0,1}(seq{2,∞}(a{1,2},b,c{2,3}),choice{3,∞}(d)))", "seq{0,2}(choice{0,1}(seq{2,∞}(a{1,2},b,c{2,3}),choice{3,∞}(d)))", false, &["a", "b", "c", "c", "a", "b", "c", "c", "d", "d", "d"]), // 04730: witness checked on the runtime, Xerces rcase-Recurse.1
    ("seq{0,∞}(choice{1,2}(seq{0,1}(a{0,1}),choice(b{1,2})),choice{0,1}(c{0,1},choice{3,∞}(d{1,2})))", "seq{0,∞}(choice{1,2}(seq{0,1}(a{0,1}),choice(b{1,2})),choice{0,1}(c{0,1},choice{2,∞}(d{1,2})))", false, &["d", "d"]), // 05210: oracle witness, Xerces rcase-Recurse.2
    ("choice{2,3}(seq{1,∞}(choice(a{0,1},b),choice{0,∞}(c{3,3})))", "choice{2,3}(seq{1,∞}(choice(a{0,1},b),choice{0,∞}(c{3,6})))", false, &["c", "c", "c", "c"]), // 05274: oracle witness, Xerces rcase-RecurseLax.2
    ("seq{2,3}(a{1,∞},choice{1,3}(b{1,2}),choice{0,∞}(c{3,3}))", "seq{2,3}(a{1,∞},choice{1,3}(b{1,2}),choice{0,∞}(c{3,∞}))", false, &["a", "b", "a", "b", "c", "c", "c", "c"]), // 05903: witness checked on the runtime, Xerces rcase-Recurse.2
];

fn risky_rows_cases() -> Vec<Case> {
    RISKY_ROWS
        .iter()
        .map(|&(base, restricted, v11, witness)| {
            let case = Case::plain(
                &format!("{base} > {restricted}"),
                particle(base),
                particle(restricted),
                false,
                v11,
            );
            if v11 {
                case
            } else {
                case.witness(witness)
            }
        })
        .collect()
}

#[test]
fn risky_rows() {
    check_all(&risky_rows_cases());
}
