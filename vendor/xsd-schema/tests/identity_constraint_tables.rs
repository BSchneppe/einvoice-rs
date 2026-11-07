//! Identity constraints: which node tables a keyref sees, keyrefs that refer
//! to a key declared later, and key tables at scale.
//!
//! Structures 1.1 §3.11.4 (Identity-constraint Satisfied), clause 4.3: "If the
//! {identity-constraint category} is keyref, then for each member of the
//! ·qualified node set· (call this the keyref member), there is a ·node table·
//! associated with the {referenced key} in the [identity-constraint table] of
//! E … and there is an entry in that table whose ·key-sequence· is equal or
//! identical to the keyref member's ·key-sequence· member for member". Its
//! note: "·node tables· are assembled strictly recursively from the node
//! tables of descendants, only element information items within the sub-tree
//! rooted at the element information item being ·validated· can be referenced
//! successfully." §3.11.5 (Identity-constraint Table): an element's
//! [identity-constraint table] has a binding for each ·eligible
//! identity-constraint· — its own, or one "any of the element information item
//! [children] of that item have an [identity-constraint table] property whose
//! value has an entry for" — whose [node table] holds the entries of the
//! children's node tables and its own ·qualified node set·. A key declared on
//! a sibling or an ancestor of the keyref's element is in neither. XSD 1.0
//! §3.11.4 / §3.11.5 say the same.
//!
//! The {referenced key} of a keyref (§3.11.2: "The {referenced key} … the
//! identity-constraint definition resolved to by the ·actual value· of the
//! `refer` [attribute]") is a QName reference in the identity-constraint
//! symbol space: its declaration may come later in the document, or in
//! another schema document.
//!
//! Every instance case runs in XSD 1.0 and XSD 1.1 mode, on the streaming
//! driver with and without PSVI typed values, on the navigator driver and —
//! with the `xsd11` feature — on the typed build.

use xsd_schema::validation::{
    drive_navigator, drive_quick_xml, CollectingValidationSink, SchemaValidator, SchemaValidity,
    ValidationFlags,
};
use xsd_schema::{RoXmlNavigator, SchemaSet, SchemaSetBuilder, XsdVersion};

const VERSIONS: [XsdVersion; 2] = [XsdVersion::V1_0, XsdVersion::V1_1];

fn compile(version: XsdVersion, docs: &[&str]) -> Result<SchemaSet, String> {
    let mut builder = SchemaSetBuilder::with_version(version);
    for (i, doc) in docs.iter().enumerate() {
        builder = builder
            .add_source(doc, &format!("file:///identity_constraint_tables_{i}.xsd"))
            .map_err(|e| e.to_string())?;
    }
    builder
        .compile()
        .map(|c| c.into_schema_set())
        .map_err(|e| e.to_string())
}

fn schema(version: XsdVersion, docs: &[&str]) -> SchemaSet {
    compile(version, docs).unwrap_or_else(|e| panic!("{version:?}: schema compiles: {e}"))
}

fn flags(psvi: bool) -> ValidationFlags {
    let mut f = ValidationFlags::default();
    if !psvi {
        f &= !ValidationFlags::BUILD_PSVI_TYPED_VALUES;
    }
    f
}

/// The root's `[validity]` and the error codes (with their element paths).
type Run = (Option<SchemaValidity>, Vec<String>);

fn stream_with(ss: &SchemaSet, xml: &str, f: ValidationFlags) -> Run {
    let v = SchemaValidator::new(ss, f);
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    let root = {
        let mut rt = v.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        drive_quick_xml(xml.as_bytes(), &mut rt, ss)
            .expect("stream drive completes")
            .root_validity
    };
    (root, codes(&errors))
}

fn codes(errors: &[xsd_schema::validation::ValidationError]) -> Vec<String> {
    errors
        .iter()
        .map(|e| format!("{} {}", e.constraint, e.element_path().unwrap_or("")))
        .collect()
}

fn navigator(ss: &SchemaSet, xml: &str) -> Run {
    let v = SchemaValidator::new(ss, flags(true));
    let doc = roxmltree::Document::parse(xml).expect("well-formed instance");
    let nav = RoXmlNavigator::new(&doc);
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    let root = {
        let mut rt = v.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        drive_navigator(&nav, &mut rt, ss)
            .expect("navigator drive completes")
            .root_validity
    };
    (root, codes(&errors))
}

#[cfg(feature = "xsd11")]
fn typed_build(ss: &SchemaSet, xml: &str) -> Run {
    use xsd_schema::document::{build_typed_document_with, TypedBuildOptions};
    use xsd_schema::validation::ValidationEventHandler;
    struct Noop;
    impl ValidationEventHandler for Noop {
        type Error = std::convert::Infallible;
    }
    let v = SchemaValidator::new(ss, flags(true));
    let arena = bumpalo::Bump::new();
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    let root = {
        let mut rt = v.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        let built = build_typed_document_with(
            xml.as_bytes(),
            &arena,
            ss,
            &mut rt,
            &mut Noop,
            TypedBuildOptions::default(),
        )
        .expect("typed build completes");
        built.outcome.root_validity
    };
    (root, codes(&errors))
}

/// Every driver's run of `xml`.
fn runs(ss: &SchemaSet, xml: &str) -> Vec<(&'static str, Run)> {
    let runs = vec![
        ("stream", stream_with(ss, xml, flags(true))),
        ("stream/no-psvi", stream_with(ss, xml, flags(false))),
        ("navigator", navigator(ss, xml)),
    ];
    #[cfg(feature = "xsd11")]
    let runs = {
        let mut runs = runs;
        runs.push(("typed build", typed_build(ss, xml)));
        runs
    };
    runs
}

/// Every driver: valid, or invalid with exactly the error lines `expected`
/// (`"<constraint> <element path>"`, in the order reported; a keyref error
/// carries no element path).
fn check(ss: &SchemaSet, label: &str, xml: &str, expected: &[&str], wrong: &mut Vec<String>) {
    for (driver, (root, errors)) in runs(ss, xml) {
        let want = if expected.is_empty() {
            SchemaValidity::Valid
        } else {
            SchemaValidity::Invalid
        };
        if root != Some(want) || errors != expected {
            wrong.push(format!(
                "{label} {driver}: {xml}\n    got {root:?} {errors:?}\n    expected {want:?} {expected:?}"
            ));
        }
    }
}

fn finish(wrong: Vec<String>) {
    assert!(
        wrong.is_empty(),
        "{} wrong:\n  {}",
        wrong.len(),
        wrong.join("\n  ")
    );
}

// ── Which node tables a keyref sees ──────────────────────────────────────

/// `{KEY_ROOT}`, `{KEY_KEYS}`, `{KEYREF_ROOT}`, `{KEYREF_REFS}` place the key
/// and the keyref on the root, on `keys` or on `refs`.
const PLACEMENT: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="root">
    <xs:complexType><xs:sequence>
      <xs:element name="keys" minOccurs="0">
        <xs:complexType><xs:sequence>
          <xs:element name="k" maxOccurs="unbounded">
            <xs:complexType><xs:attribute name="id" type="xs:string"/></xs:complexType>
          </xs:element>
        </xs:sequence></xs:complexType>
        {KEY_KEYS}
      </xs:element>
      <xs:element name="refs" minOccurs="0">
        <xs:complexType><xs:sequence>
          <xs:element name="r" maxOccurs="unbounded">
            <xs:complexType><xs:attribute name="to" type="xs:string"/></xs:complexType>
          </xs:element>
        </xs:sequence></xs:complexType>
        {KEYREF_REFS}
      </xs:element>
    </xs:sequence></xs:complexType>
    {KEY_ROOT}
    {KEYREF_ROOT}
  </xs:element>
</xs:schema>"#;

const KEY_ON_KEYS: &str =
    r#"<xs:key name="kk"><xs:selector xpath="k"/><xs:field xpath="@id"/></xs:key>"#;
const KEY_ON_ROOT: &str =
    r#"<xs:key name="kk"><xs:selector xpath="keys/k"/><xs:field xpath="@id"/></xs:key>"#;
const KEYREF_ON_REFS: &str = r#"<xs:keyref name="kr" refer="kk"><xs:selector xpath="r"/><xs:field xpath="@to"/></xs:keyref>"#;
const KEYREF_ON_ROOT: &str = r#"<xs:keyref name="kr" refer="kk"><xs:selector xpath="refs/r"/><xs:field xpath="@to"/></xs:keyref>"#;

fn placement(key_on_root: bool, keyref_on_root: bool) -> String {
    PLACEMENT
        .replace("{KEY_ROOT}", if key_on_root { KEY_ON_ROOT } else { "" })
        .replace("{KEY_KEYS}", if key_on_root { "" } else { KEY_ON_KEYS })
        .replace(
            "{KEYREF_ROOT}",
            if keyref_on_root { KEYREF_ON_ROOT } else { "" },
        )
        .replace(
            "{KEYREF_REFS}",
            if keyref_on_root { "" } else { KEYREF_ON_REFS },
        )
}

const MATCH: &str =
    r#"<root><keys><k id="a"/><k id="b"/></keys><refs><r to="a"/><r to="b"/></refs></root>"#;
const MISS: &str = r#"<root><keys><k id="a"/></keys><refs><r to="a"/><r to="z"/></refs></root>"#;
const NO_KEYS: &str = r#"<root><refs><r to="a"/></refs></root>"#;

/// The key is on the keyref's own element or below it: the keyref sees it.
#[test]
fn a_keyref_sees_keys_of_its_element_and_descendants() {
    let mut wrong = Vec::new();
    for version in VERSIONS {
        // Key on `keys` (a child of the root), keyref on the root.
        let ss = schema(version, &[&placement(false, true)]);
        let label = format!("{version:?} descendant key");
        check(&ss, &label, MATCH, &[], &mut wrong);
        check(
            &ss,
            &label,
            MISS,
            &["cvc-identity-constraint.4.3 "],
            &mut wrong,
        );
        // Key and keyref on the root.
        let ss = schema(version, &[&placement(true, true)]);
        let label = format!("{version:?} same element");
        check(&ss, &label, MATCH, &[], &mut wrong);
        check(
            &ss,
            &label,
            MISS,
            &["cvc-identity-constraint.4.3 "],
            &mut wrong,
        );
    }
    finish(wrong);
}

/// The key is on a sibling (`keys`) or an ancestor (the root) of the
/// keyref's element (`refs`): its node table is not in the [identity-constraint
/// table] of `refs`, so every keyref member is unmatched (§3.11.4 clause 4.3).
#[test]
fn a_keyref_does_not_see_keys_of_siblings_or_ancestors() {
    let mut wrong = Vec::new();
    for version in VERSIONS {
        for (label, key_on_root) in [("sibling key", false), ("ancestor key", true)] {
            let ss = schema(version, &[&placement(key_on_root, false)]);
            let label = format!("{version:?} {label}");
            check(
                &ss,
                &label,
                MATCH,
                &[
                    "cvc-identity-constraint.4.3 ",
                    "cvc-identity-constraint.4.3 ",
                ],
                &mut wrong,
            );
            check(
                &ss,
                &label,
                NO_KEYS,
                &["cvc-identity-constraint.4.3 "],
                &mut wrong,
            );
        }
    }
    finish(wrong);
}

// ── `refer` to a key declared later ──────────────────────────────────────

/// The keyref comes before its key on one element.
const REFER_BEFORE_KEY: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="root">
    <xs:complexType><xs:sequence>
      <xs:element name="k" maxOccurs="unbounded">
        <xs:complexType><xs:attribute name="id"/><xs:attribute name="to"/></xs:complexType>
      </xs:element>
    </xs:sequence></xs:complexType>
    <xs:keyref name="kr" refer="kk"><xs:selector xpath="k"/><xs:field xpath="@to"/></xs:keyref>
    <xs:key name="kk"><xs:selector xpath="k"/><xs:field xpath="@id"/></xs:key>
  </xs:element>
</xs:schema>"#;

/// The keyref is on the root; its key is on a local element declared below.
const REFER_TO_LOCAL_KEY: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="root">
    <xs:complexType><xs:sequence>
      <xs:element name="keys">
        <xs:complexType><xs:sequence>
          <xs:element name="k" maxOccurs="unbounded">
            <xs:complexType><xs:attribute name="id"/></xs:complexType>
          </xs:element>
        </xs:sequence></xs:complexType>
        <xs:key name="kk"><xs:selector xpath="k"/><xs:field xpath="@id"/></xs:key>
      </xs:element>
      <xs:element name="r" maxOccurs="unbounded">
        <xs:complexType><xs:attribute name="to"/></xs:complexType>
      </xs:element>
    </xs:sequence></xs:complexType>
    <xs:keyref name="kr" refer="kk"><xs:selector xpath="r"/><xs:field xpath="@to"/></xs:keyref>
  </xs:element>
</xs:schema>"#;

/// Two schema documents of one namespace: the keyref is in the first, the
/// key (on a type's local element) in the second.
const REFER_DOC_1: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
    xmlns:t="urn:t" targetNamespace="urn:t" elementFormDefault="qualified">
  <xs:element name="root" type="t:RootType">
    <xs:keyref name="kr" refer="t:kk"><xs:selector xpath="t:r"/><xs:field xpath="@to"/></xs:keyref>
  </xs:element>
</xs:schema>"#;
const REFER_DOC_2: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
    xmlns:t="urn:t" targetNamespace="urn:t" elementFormDefault="qualified">
  <xs:complexType name="RootType"><xs:sequence>
    <xs:element name="keys">
      <xs:complexType><xs:sequence>
        <xs:element name="k" maxOccurs="unbounded">
          <xs:complexType><xs:attribute name="id"/></xs:complexType>
        </xs:element>
      </xs:sequence></xs:complexType>
      <xs:key name="kk"><xs:selector xpath="t:k"/><xs:field xpath="@id"/></xs:key>
    </xs:element>
    <xs:element name="r" maxOccurs="unbounded">
      <xs:complexType><xs:attribute name="to"/></xs:complexType>
    </xs:element>
  </xs:sequence></xs:complexType>
</xs:schema>"#;

#[test]
fn a_keyref_may_refer_to_a_key_declared_later() {
    let mut wrong = Vec::new();
    for version in VERSIONS {
        let cases: [(&str, Vec<&str>, &str, &str, &str); 3] = [
            (
                "keyref before key",
                vec![REFER_BEFORE_KEY],
                r#"<root><k id="a" to="b"/><k id="b" to="a"/></root>"#,
                r#"<root><k id="a" to="z"/></root>"#,
                "/root",
            ),
            (
                "key on a later local element",
                vec![REFER_TO_LOCAL_KEY],
                r#"<root><keys><k id="a"/></keys><r to="a"/></root>"#,
                r#"<root><keys><k id="a"/></keys><r to="z"/></root>"#,
                "/root",
            ),
            (
                "key in another schema document",
                vec![REFER_DOC_1, REFER_DOC_2],
                r#"<t:root xmlns:t="urn:t"><t:keys><t:k id="a"/></t:keys><t:r to="a"/></t:root>"#,
                r#"<t:root xmlns:t="urn:t"><t:keys><t:k id="a"/></t:keys><t:r to="z"/></t:root>"#,
                "/Q{urn:t}root",
            ),
        ];
        for (label, docs, valid, invalid, path) in cases {
            let label = format!("{version:?} {label}");
            match compile(version, &docs) {
                Err(e) => wrong.push(format!("{label}: schema refused: {e}")),
                Ok(ss) => {
                    check(&ss, &label, valid, &[], &mut wrong);
                    let _ = path;
                    check(
                        &ss,
                        &label,
                        invalid,
                        &["cvc-identity-constraint.4.3 "],
                        &mut wrong,
                    );
                }
            }
        }
    }
    finish(wrong);
}

/// A `refer` that names no key or unique stays a schema error, wherever it
/// would be declared.
#[test]
fn a_refer_to_nothing_or_to_a_keyref_is_still_a_schema_error() {
    for version in VERSIONS {
        let missing = REFER_BEFORE_KEY.replace(r#"refer="kk""#, r#"refer="nosuch""#);
        let err = compile(version, &[&missing]).expect_err("refer to nothing accepted");
        assert!(
            err.contains("src-identity-constraint"),
            "{version:?}: {err}"
        );
        let to_keyref = REFER_BEFORE_KEY.replace(
            r#"<xs:key name="kk">"#,
            r#"<xs:keyref name="kk" refer="kr">"#,
        );
        let to_keyref = to_keyref.replace("</xs:key>", "</xs:keyref>");
        let err = compile(version, &[&to_keyref]).expect_err("refer to a keyref accepted");
        assert!(
            err.contains("src-identity-constraint"),
            "{version:?}: {err}"
        );
        let fields = REFER_BEFORE_KEY.replace(
            r#"<xs:field xpath="@to"/>"#,
            r#"<xs:field xpath="@to"/><xs:field xpath="@id"/>"#,
        );
        let err = compile(version, &[&fields]).expect_err("field count mismatch accepted");
        assert!(
            err.contains("src-identity-constraint"),
            "{version:?}: {err}"
        );
    }
}

// ── Key tables: equality, order of the errors ─────────────────────────────

/// One unique over the children of the root: `a` … `e` of the type `{T}` the
/// case sets, `w` and `x` lists of `xs:int`, `s` of `xs:anySimpleType` (its
/// values have no primitive type and compare by their strings).
const VALUES: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:simpleType name="ints"><xs:list itemType="xs:int"/></xs:simpleType>
  <xs:element name="root">
    <xs:complexType><xs:choice maxOccurs="unbounded">
      <xs:element name="a" type="{T}"/>
      <xs:element name="b" type="{T}"/>
      <xs:element name="c" type="{T}"/>
      <xs:element name="d" type="{T}"/>
      <xs:element name="e" type="{T}"/>
      <xs:element name="w" type="ints"/>
      <xs:element name="x" type="ints"/>
      <xs:element name="s" type="xs:anySimpleType"/>
      <xs:element name="t" type="xs:anySimpleType"/>
    </xs:choice></xs:complexType>
    <xs:unique name="uq"><xs:selector xpath="*"/><xs:field xpath="."/></xs:unique>
  </xs:element>
</xs:schema>"#;

/// Equal values collide whatever their form; the duplicate is reported on
/// the element that repeats a value, in document order.
#[test]
fn equal_values_collide_and_errors_come_in_document_order() {
    let dup = "cvc-identity-constraint.4.2.2";
    let mut wrong = Vec::new();
    for version in VERSIONS {
        let cases: &[(&str, &str, &[&str])] = &[
            ("xs:decimal", "<root><a>1.0</a><b>1</b><c>01.00</c></root>", &["/root/b", "/root/c"]),
            ("xs:decimal", "<root><a>1</a><b>2</b><c>1.5</c></root>", &[]),
            ("xs:integer", "<root><a>1</a><b>+1</b><c>2</c><d>02</d></root>", &["/root/b", "/root/d"]),
            ("xs:double", "<root><a>0</a><b>-0</b><c>NaN</c><d>NaN</d></root>", &["/root/b", "/root/d"]),
            ("xs:double", "<root><a>1e0</a><b>1.0</b><c>INF</c><d>-INF</d></root>", &["/root/b"]),
            (
                "xs:dateTime",
                "<root><a>2020-01-01T00:00:00Z</a><b>2020-01-01T01:00:00+01:00</b><c>2020-01-01T00:00:00</c></root>",
                &["/root/b"],
            ),
            ("xs:string", "<root><a>p</a><b>q</b><c>p</c><d>q</d><e>r</e></root>", &["/root/c", "/root/d"]),
            ("xs:boolean", "<root><a>true</a><b>1</b><c>0</c></root>", &["/root/b"]),
            ("xs:QName", "<root xmlns:p='urn:p' xmlns:q='urn:p'><a>p:n</a><b>q:n</b><c>n</c></root>", &["/root/b"]),
            // An atomic value equals the list of that one value (§3.11.4).
            ("xs:int", "<root><a>7</a><w>7</w><x>7 8</x><w> 7  8 </w></root>", &["/root/w", "/root/w"]),
            // Values of different primitive types are distinct.
            ("xs:int", "<root><a>1</a><b>2</b><w>3</w></root>", &[]),
        ];
        for (ty, xml, dups) in cases {
            let ss = schema(version, &[&VALUES.replace("{T}", ty)]);
            let expected: Vec<String> = dups.iter().map(|p| format!("{dup} {p}")).collect();
            let expected: Vec<&str> = expected.iter().map(String::as_str).collect();
            check(
                &ss,
                &format!("{version:?} {ty}"),
                xml,
                &expected,
                &mut wrong,
            );
        }
    }
    finish(wrong);
}

/// A field whose values come from types of different primitive types and from
/// `xs:anySimpleType` (`s`, `t`): the table compares each pair as before,
/// whatever the order the values come in.
#[test]
fn typed_and_untyped_values_in_one_table() {
    let dup = "cvc-identity-constraint.4.2.2";
    let mut wrong = Vec::new();
    for version in VERSIONS {
        let ss = schema(version, &[&VALUES.replace("{T}", "xs:int")]);
        let label = format!("{version:?} mixed");
        let cases: &[(&str, &[&str])] = &[
            ("<root><a>1</a><s>x</s><t>x</t></root>", &["/root/t"]),
            ("<root><s>x</s><a>1</a><t>x</t></root>", &["/root/t"]),
            // `xs:anySimpleType` "5" and `xs:int` 5 are distinct values.
            ("<root><s>5</s><a>5</a></root>", &[]),
            ("<root><a>5</a><s>5</s></root>", &[]),
            (
                "<root><a>5</a><s>05</s><b>05</b><t>05</t></root>",
                &["/root/b", "/root/t"],
            ),
            (
                "<root><a>5</a><b>6</b><s>7</s><c>8</c><t>6</t><d>7</d></root>",
                &[],
            ),
            (
                "<root><s>p</s><a>1</a><t>p</t><b>1</b></root>",
                &["/root/t", "/root/b"],
            ),
        ];
        for (xml, dups) in cases {
            let expected: Vec<String> = dups.iter().map(|p| format!("{dup} {p}")).collect();
            let expected: Vec<&str> = expected.iter().map(String::as_str).collect();
            check(&ss, &label, xml, &expected, &mut wrong);
        }
    }
    finish(wrong);
}

// ── PROCESS_IDENTITY_CONSTRAINTS ─────────────────────────────────────────

/// Identity constraints are part of validity (§3.3.4.3 clause 6: "E is
/// ·valid· with respect to each of the {identity-constraint definitions} as
/// per Identity-constraint Satisfied (§3.11.4)"): they are enforced whatever
/// the flags — the deprecated `PROCESS_IDENTITY_CONSTRAINTS` bit changes
/// nothing.
#[test]
#[allow(deprecated)]
fn identity_constraints_are_enforced_whatever_the_flags() {
    let xml = "<root><a>1</a><b>1</b></root>";
    for version in VERSIONS {
        let ss = schema(version, &[&VALUES.replace("{T}", "xs:int")]);
        for (label, f) in [
            ("default", ValidationFlags::default()),
            (
                "default | PROCESS_IDENTITY_CONSTRAINTS",
                ValidationFlags::default() | ValidationFlags::PROCESS_IDENTITY_CONSTRAINTS,
            ),
            (
                "default - PROCESS_IDENTITY_CONSTRAINTS",
                ValidationFlags::default() - ValidationFlags::PROCESS_IDENTITY_CONSTRAINTS,
            ),
            ("empty", ValidationFlags::empty()),
        ] {
            let (root, errors) = stream_with(&ss, xml, f);
            assert_eq!(
                root,
                Some(SchemaValidity::Invalid),
                "{version:?} {label}: {errors:?}"
            );
            assert_eq!(
                errors,
                ["cvc-identity-constraint.4.2.2 /root/b"],
                "{version:?} {label}"
            );
        }
    }
}
