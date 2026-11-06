//! The types the XPath 2.0 data model adds to the XML Schema namespace, for a
//! host that validates nodes against a type it names.
//!
//! XDM 1.0 §2.6.2 (Predefined Types): "In addition to the 19 types defined in
//! Section 3.2 Primitive datatypes of [Schema Part 2], the data model defines
//! five additional types: xs:anyAtomicType, xs:untyped, xs:untypedAtomic,
//! xs:dayTimeDuration, and xs:yearMonthDuration." XML Schema 1.1 Part 2 adds
//! `xs:anyAtomicType` (§3.2.2), `xs:yearMonthDuration` (§3.4.26) and
//! `xs:dayTimeDuration` (§3.4.27) to its built-in datatypes; `xs:untypedAtomic`
//! is in neither version (Structures 1.1 §1.3.1.1: the data model "makes use of
//! type labels in the XSD namespace (untyped, untypedAtomic) which are not
//! defined in this specification").
//!
//! Every schema set has the four as components, which a host reaches with
//! [`SchemaSet::xpath_type`]; a schema document reaches only the ones its XSD
//! version defines, and an `xsi:type` the others only with
//! [`ValidationFlags::XPATH_TYPES`]. Every case runs in XSD 1.0 and XSD 1.1
//! mode; the drivers are streaming (with and without PSVI typed values), the
//! navigator, and — with the `xsd11` feature — the buffer document and the
//! typed build, over which the XPath checks run.

use xsd_schema::namespace::context::NamespaceContextSnapshot;
use xsd_schema::types::value::XmlValueKind;
use xsd_schema::types::XmlTypeCode;
use xsd_schema::validation::{
    drive_navigator, drive_quick_xml, validate_simple_type, CollectingValidationSink, SchemaInfo,
    SchemaValidator, SchemaValidity, ValidationError, ValidationFlags, ValidationRoot,
};
use xsd_schema::{
    load_and_process_schema, RoXmlNavigator, SchemaSet, SchemaSetBuilder, TypeKey, XsdVersion,
};

use SchemaValidity::{Invalid, Valid};

const VERSIONS: [XsdVersion; 2] = [XsdVersion::V1_0, XsdVersion::V1_1];

const XS: &str = "http://www.w3.org/2001/XMLSchema";

/// The four types, by code and local name.
const XPATH_TYPES: [(XmlTypeCode, &str); 4] = [
    (XmlTypeCode::AnyAtomicType, "anyAtomicType"),
    (XmlTypeCode::UntypedAtomic, "untypedAtomic"),
    (XmlTypeCode::DayTimeDuration, "dayTimeDuration"),
    (XmlTypeCode::YearMonthDuration, "yearMonthDuration"),
];

/// The host's flag: the data model's types are in scope for `xsi:type`.
const HOST: ValidationFlags = ValidationFlags::XPATH_TYPES;

/// A schema whose elements are of `xs:anyType`, so an `xsi:type` can name any
/// type: `r` holds the typed children the XPath checks read.
const SCHEMA: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="r">
    <xs:complexType>
      <xs:sequence>
        <xs:element name="d" minOccurs="0" maxOccurs="unbounded"/>
        <xs:element name="y" minOccurs="0" maxOccurs="unbounded"/>
        <xs:element name="u" minOccurs="0"/>
        <xs:element name="a" minOccurs="0"/>
      </xs:sequence>
    </xs:complexType>
  </xs:element>
  <xs:element name="any"/>
  <xs:element name="dur" type="xs:duration"/>
</xs:schema>"#;

fn compile(version: XsdVersion, xsd: &str) -> Result<SchemaSet, String> {
    SchemaSetBuilder::with_version(version)
        .add_source(xsd, "file:///xpath_types_for_hosts.xsd")
        .map_err(|e| e.to_string())?
        .compile()
        .map(|c| c.into_schema_set())
        .map_err(|e| e.to_string())
}

fn schema(version: XsdVersion) -> SchemaSet {
    compile(version, SCHEMA).unwrap_or_else(|e| panic!("{version:?}: {e}"))
}

/// The host lookup.
fn xpath_key(ss: &SchemaSet, code: XmlTypeCode) -> Option<TypeKey> {
    ss.xpath_type(code)
}

fn key(ss: &SchemaSet, code: XmlTypeCode) -> TypeKey {
    xpath_key(ss, code).unwrap_or_else(|| panic!("{:?}: the host finds {code:?}", ss.xsd_version))
}

fn builtin(ss: &SchemaSet, code: XmlTypeCode) -> TypeKey {
    TypeKey::Simple(
        ss.get_built_in_simple_type_by_code(code)
            .unwrap_or_else(|| panic!("{code:?} is a built-in of every set")),
    )
}

fn codes(errors: &[ValidationError]) -> Vec<String> {
    errors.iter().map(|e| e.constraint.to_string()).collect()
}

fn xs_snapshot(ss: &SchemaSet) -> NamespaceContextSnapshot {
    let mut ns = NamespaceContextSnapshot::default();
    ns.bindings
        .push((ss.name_table.add("xs"), ss.name_table.add(XS)));
    ns
}

// ── The host lookup and the hierarchy ────────────────────────────────────

#[test]
fn the_host_finds_the_four_types_in_both_versions() {
    for version in VERSIONS {
        let ss = schema(version);
        for (code, local) in XPATH_TYPES {
            let TypeKey::Simple(simple) = key(&ss, code) else {
                panic!("{version:?}: {local} is a simple type");
            };
            assert_eq!(ss.get_type_code(simple), Some(code), "{version:?} {local}");
            let def = ss.arenas.simple_types.get(simple).expect("a component");
            assert_eq!(
                def.name.map(|n| ss.name_table.resolve(n).to_string()),
                Some(local.to_string()),
                "{version:?}: the component's {{name}}"
            );
            assert_eq!(
                def.target_namespace
                    .map(|n| ss.name_table.resolve(n).to_string()),
                Some(XS.to_string()),
                "{version:?}: {local}'s {{target namespace}}"
            );
        }
        // Every other built-in type is the set's own.
        for code in [
            XmlTypeCode::String,
            XmlTypeCode::Duration,
            XmlTypeCode::AnySimpleType,
        ] {
            assert_eq!(key(&ss, code), builtin(&ss, code), "{version:?} {code:?}");
        }
        assert_eq!(
            xpath_key(&ss, XmlTypeCode::AnyType),
            Some(TypeKey::Complex(ss.any_type_key())),
            "{version:?}: xs:anyType"
        );
        // XSD 1.1's own additions that XPath 2.0 does not have stay 1.1's.
        let v11 = version == XsdVersion::V1_1;
        assert_eq!(xpath_key(&ss, XmlTypeCode::DateTimeStamp).is_some(), v11);
        assert_eq!(xpath_key(&ss, XmlTypeCode::Error).is_some(), v11);
        // Node kinds and item() are no types of the set.
        assert_eq!(xpath_key(&ss, XmlTypeCode::Element), None);
        assert_eq!(xpath_key(&ss, XmlTypeCode::Item), None);
    }
}

/// The public fields and the code lookups of a 1.0 set do not have the four,
/// and a schema document's QName lookup finds none of them in 1.0 and all but
/// `xs:untypedAtomic` in 1.1.
#[test]
fn the_fields_and_lookups_of_each_version() {
    let ss = schema(XsdVersion::V1_0);
    let b = ss.builtin_types();
    assert_eq!(b.any_atomic_type, None);
    assert_eq!(b.untyped_atomic, None);
    assert_eq!(b.day_time_duration, None);
    assert_eq!(b.year_month_duration, None);
    for (code, local) in XPATH_TYPES {
        assert_eq!(ss.get_built_in_simple_type_by_code(code), None, "{local}");
        let name = ss.name_table.add(local);
        assert_eq!(
            ss.get_built_in_type_by_qname(Some(ss.name_table.add(XS)), name),
            None,
            "{local}: a schema document's QName lookup"
        );
    }

    // An XSD 1.1 set names three of them; `xs:untypedAtomic` keeps its field
    // and its code lookup, but a schema document's QName lookup does not find
    // it (Structures 1.1 §1.3.1.1).
    let ss = schema(XsdVersion::V1_1);
    let xs = Some(ss.name_table.add(XS));
    for (_, local) in XPATH_TYPES {
        let name = ss.name_table.add(local);
        assert_eq!(
            ss.get_built_in_type_by_qname(xs, name).is_some(),
            local != "untypedAtomic",
            "1.1 {local}: a schema document's QName lookup"
        );
    }
    assert_eq!(
        ss.get_built_in_simple_type_by_code(XmlTypeCode::UntypedAtomic)
            .map(TypeKey::Simple),
        Some(key(&ss, XmlTypeCode::UntypedAtomic))
    );
    let b = ss.builtin_types();
    for (code, local) in XPATH_TYPES {
        let field = match code {
            XmlTypeCode::AnyAtomicType => b.any_atomic_type,
            XmlTypeCode::UntypedAtomic => b.untyped_atomic,
            XmlTypeCode::DayTimeDuration => b.day_time_duration,
            _ => b.year_month_duration,
        };
        assert_eq!(
            field.map(TypeKey::Simple),
            Some(key(&ss, code)),
            "1.1 {local}"
        );
    }
}

#[test]
fn their_places_in_the_hierarchy() {
    use xsd_schema::schema::model::DerivationSet;
    for version in VERSIONS {
        let ss = schema(version);
        let any_atomic = key(&ss, XmlTypeCode::AnyAtomicType);
        let untyped_atomic = key(&ss, XmlTypeCode::UntypedAtomic);
        let dt = key(&ss, XmlTypeCode::DayTimeDuration);
        let ym = key(&ss, XmlTypeCode::YearMonthDuration);
        let any_simple = builtin(&ss, XmlTypeCode::AnySimpleType);
        let any_type = TypeKey::Complex(ss.any_type_key());
        let duration = builtin(&ss, XmlTypeCode::Duration);
        let derived =
            |d: TypeKey, b: TypeKey| ss.is_type_derived_from(d, b, DerivationSet::empty());
        // XDM 1.0 §2.6.2: dayTimeDuration and yearMonthDuration are derived
        // from xs:duration; anyAtomicType's "base type is xs:anySimpleType";
        // "All primitive atomic types … have xs:anyAtomicType as their base
        // type"; untypedAtomic is atomic (Figure 2.6.3: under anyAtomicType).
        for (d, b, expected, what) in [
            (dt, duration, true, "dayTimeDuration < duration"),
            (ym, duration, true, "yearMonthDuration < duration"),
            (dt, any_atomic, true, "dayTimeDuration < anyAtomicType"),
            (ym, any_simple, true, "yearMonthDuration < anySimpleType"),
            (
                untyped_atomic,
                any_atomic,
                true,
                "untypedAtomic < anyAtomicType",
            ),
            (
                untyped_atomic,
                any_simple,
                true,
                "untypedAtomic < anySimpleType",
            ),
            (
                any_atomic,
                any_simple,
                true,
                "anyAtomicType < anySimpleType",
            ),
            (any_atomic, any_type, true, "anyAtomicType < anyType"),
            (
                builtin(&ss, XmlTypeCode::Integer),
                any_atomic,
                true,
                "integer < anyAtomicType",
            ),
            (
                builtin(&ss, XmlTypeCode::String),
                any_atomic,
                true,
                "string < anyAtomicType",
            ),
            (
                any_simple,
                any_atomic,
                false,
                "anySimpleType is not < anyAtomicType",
            ),
            (
                builtin(&ss, XmlTypeCode::NmTokens),
                any_atomic,
                false,
                "a list is not atomic",
            ),
            (dt, ym, false, "the durations are siblings"),
            (duration, dt, false, "duration is not < dayTimeDuration"),
            (
                builtin(&ss, XmlTypeCode::String),
                untyped_atomic,
                false,
                "nothing derives from untypedAtomic",
            ),
            (
                any_atomic,
                untyped_atomic,
                false,
                "anyAtomicType is not < untypedAtomic",
            ),
        ] {
            assert_eq!(derived(d, b), expected, "{version:?}: {what}");
        }
        // Every step is a restriction (Structures §3.16.6.3).
        assert!(
            !ss.is_type_derived_from(
                builtin(&ss, XmlTypeCode::Integer),
                any_atomic,
                DerivationSet::RESTRICTION
            ),
            "{version:?}: a blocked restriction"
        );
    }
}

// ── Schema documents ─────────────────────────────────────────────────────

fn element_of_type(local: &str) -> String {
    format!(
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"><xs:element name="e" type="xs:{local}"/></xs:schema>"#
    )
}

fn restriction_of(local: &str) -> String {
    format!(
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"><xs:simpleType name="s"><xs:restriction base="xs:{local}"/></xs:simpleType></xs:schema>"#
    )
}

/// XML Schema 1.0 has none of the four: a schema document that names one is
/// refused (`src-resolve`), whether or not a host looked the type up first —
/// eagerly for a `base`, and for an element declaration's `type` the way
/// XSD 1.0 sets defer that one (the set compiles; the declaration fails with
/// `src-resolve` when an instance selects it).
#[test]
fn schema_documents_of_1_0_cannot_name_them() {
    for (code, local) in XPATH_TYPES {
        // A fresh set, and a set the host has already asked for the type.
        for host_looked_first in [false, true] {
            let label = format!("1.0 {local} (host lookup first: {host_looked_first})");
            let load = |xsd: &str| {
                let mut set = SchemaSet::new();
                if host_looked_first {
                    assert!(set.xpath_type(code).is_some(), "{label}: the host finds it");
                }
                load_and_process_schema(xsd.as_bytes(), "file:///late.xsd", &mut set, None)
                    .map(|_| set)
                    .map_err(|e| e.to_string())
            };
            let err = load(&restriction_of(local)).expect_err(&label);
            assert!(err.contains("src-resolve"), "{label}: {err}");
            let set = load(&element_of_type(local)).unwrap_or_else(|e| panic!("{label}: {e}"));
            let instance = "<e>PT1H</e>";
            for flags in [
                ValidationFlags::default(),
                ValidationFlags::default() | HOST,
            ] {
                let (validity, errors) = stream(&set, ValidationRoot::Declaration, instance, flags);
                assert_eq!(validity, Some(Invalid), "{label}: {errors:?}");
                assert!(
                    errors.iter().any(|e| e == "src-resolve"),
                    "{label}: {errors:?}"
                );
            }
        }
        // The builder path agrees.
        assert!(compile(XsdVersion::V1_0, &restriction_of(local))
            .is_err_and(|e| e.contains("src-resolve")));
    }
}

/// XML Schema 1.1 defines three of the four (Part 2 §3.2.2, §3.4.26,
/// §3.4.27); a 1.1 schema document names them as before.
#[test]
fn schema_documents_of_1_1_name_the_three_xsd_1_1_types() {
    for local in ["anyAtomicType", "dayTimeDuration", "yearMonthDuration"] {
        compile(XsdVersion::V1_1, &element_of_type(local))
            .unwrap_or_else(|e| panic!("1.1 type=\"xs:{local}\": {e}"));
    }
    // Datatypes 1.1 §3.2.2.3: "anyAtomicType must not be used as the ·base
    // type·" of a facet-based restriction.
    assert!(compile(XsdVersion::V1_1, &restriction_of("anyAtomicType")).is_err());
    for local in ["dayTimeDuration", "yearMonthDuration"] {
        compile(XsdVersion::V1_1, &restriction_of(local))
            .unwrap_or_else(|e| panic!("1.1 restriction of xs:{local}: {e}"));
    }
}

/// The constraint a schema error names (`… (constraint: src-resolve)`).
fn constraint_of(error: &str) -> &str {
    error
        .rsplit_once("(constraint: ")
        .and_then(|(_, rest)| rest.split_once(')'))
        .map_or(error, |(code, _)| code)
}

/// What becomes of `body` (the children of an `xs:schema` element) in
/// `version`: the constraint the compilation reports, or — where XSD 1.0
/// defers an unresolved element `type` or list `itemType` until an instance
/// selects it — the constraint the instance `<u>x</u>` reports.
fn refusal(version: XsdVersion, body: &str) -> String {
    let xsd = format!("<xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema'>{body}</xs:schema>");
    match compile(version, &xsd) {
        Err(error) => constraint_of(&error).to_string(),
        Ok(set) => {
            let (validity, errors) = stream(
                &set,
                ValidationRoot::Declaration,
                "<u>x</u>",
                ValidationFlags::default() | HOST,
            );
            match errors.iter().find(|e| *e == "src-resolve") {
                Some(_) => "src-resolve at instance time".to_string(),
                None => format!("compiled ({validity:?}, {errors:?})"),
            }
        }
    }
}

/// `xs:untypedAtomic` is not a type definition of any schema (Structures 1.1
/// §1.3.1.1: the data model's "type labels in the XSD namespace (untyped,
/// untypedAtomic) … are not defined in this specification"), so a schema
/// document that names it is refused in both versions by QName resolution
/// (Schema Document) (§3.17.6.2 clause 1.1: the component must be a member of
/// the schema's {type definitions}), wherever it names it. The schemas the
/// 1.1 set compiled before this batch are refused too; those it refused
/// before (a value against the type) now fail at the same reference.
#[test]
fn every_schema_document_naming_untyped_atomic_is_refused() {
    let at_compile_time = [
        // Positions.
        "<xs:attribute name='a' type='xs:untypedAtomic'/>",
        "<xs:element name='w'><xs:complexType><xs:attribute name='a' type='xs:untypedAtomic'/></xs:complexType></xs:element>",
        "<xs:simpleType name='s'><xs:restriction base='xs:untypedAtomic'/></xs:simpleType>",
        "<xs:simpleType name='s'><xs:union memberTypes='xs:int xs:untypedAtomic'/></xs:simpleType>",
        "<xs:complexType name='c'><xs:simpleContent><xs:extension base='xs:untypedAtomic'/></xs:simpleContent></xs:complexType>",
        // Compiled by a 1.1 set before.
        "<xs:simpleType name='s'><xs:restriction base='xs:untypedAtomic'><xs:pattern value='a+'/></xs:restriction></xs:simpleType>",
        "<xs:simpleType name='s'><xs:restriction base='xs:untypedAtomic'><xs:whiteSpace value='collapse'/></xs:restriction></xs:simpleType>",
        "<xs:simpleType name='s'><xs:union memberTypes='xs:untypedAtomic xs:int'/></xs:simpleType><xs:element name='u' type='s' default='1'/>",
        // Refused by a 1.1 set before, by a value against the type.
        "<xs:attribute name='a' type='xs:untypedAtomic' default='x'/>",
        "<xs:attribute name='a' type='xs:untypedAtomic' fixed='x'/>",
        "<xs:element name='w'><xs:complexType><xs:attribute name='a' type='xs:untypedAtomic' default='x'/></xs:complexType></xs:element>",
        "<xs:simpleType name='s'><xs:restriction base='xs:untypedAtomic'><xs:enumeration value='a'/></xs:restriction></xs:simpleType>",
        "<xs:simpleType name='s'><xs:restriction base='xs:untypedAtomic'/></xs:simpleType><xs:element name='u' type='s' default='x'/>",
        "<xs:simpleType name='s'><xs:restriction><xs:simpleType><xs:union memberTypes='xs:untypedAtomic'/></xs:simpleType><xs:enumeration value='a'/></xs:restriction></xs:simpleType>",
        "<xs:simpleType name='s'><xs:union memberTypes='xs:int xs:untypedAtomic'/></xs:simpleType><xs:element name='u' type='s' default='x'/>",
        "<xs:complexType name='c'><xs:simpleContent><xs:extension base='xs:untypedAtomic'/></xs:simpleContent></xs:complexType><xs:element name='u' type='c' default='x'/>",
        "<xs:complexType name='c'><xs:simpleContent><xs:extension base='xs:untypedAtomic'/></xs:simpleContent></xs:complexType><xs:complexType name='d'><xs:simpleContent><xs:restriction base='c'><xs:enumeration value='a'/></xs:restriction></xs:simpleContent></xs:complexType>",
        "<xs:simpleType name='s'><xs:restriction base='xs:untypedAtomic'><xs:whiteSpace value='preserve'/></xs:restriction></xs:simpleType>",
        "<xs:simpleType name='s'><xs:restriction base='xs:untypedAtomic'><xs:minLength value='1'/></xs:restriction></xs:simpleType>",
    ];
    // An element declaration's `type` and a list's `itemType`, which an XSD
    // 1.0 set resolves when an instance selects the declaration (a value the
    // schema gives against such a list fails first, as before): the
    // constraint in 1.0; 1.1 refuses each with `src-resolve`.
    let deferred_in_1_0 = [
        (
            "<xs:element name='u' type='xs:untypedAtomic'/>",
            "src-resolve at instance time",
        ),
        (
            "<xs:element name='u' type='xs:untypedAtomic' default='x'/>",
            "src-resolve at instance time",
        ),
        (
            "<xs:element name='u' type='xs:untypedAtomic' fixed='x'/>",
            "src-resolve at instance time",
        ),
        (
            "<xs:simpleType name='s'><xs:list itemType='xs:untypedAtomic'/></xs:simpleType><xs:element name='u' type='s'/>",
            "src-resolve at instance time",
        ),
        (
            "<xs:simpleType name='s'><xs:list itemType='xs:untypedAtomic'/></xs:simpleType><xs:element name='u' type='s' default='a b'/>",
            "e-props-correct.2",
        ),
        (
            "<xs:simpleType name='s'><xs:restriction><xs:simpleType><xs:list itemType='xs:untypedAtomic'/></xs:simpleType><xs:enumeration value='a b'/></xs:restriction></xs:simpleType><xs:element name='u' type='s'/>",
            "enumeration-valid-restriction",
        ),
    ];
    for version in VERSIONS {
        for body in at_compile_time {
            assert_eq!(refusal(version, body), "src-resolve", "{version:?}: {body}");
        }
        for (body, in_1_0) in deferred_in_1_0 {
            let expected = match version {
                XsdVersion::V1_0 => in_1_0,
                _ => "src-resolve",
            };
            assert_eq!(refusal(version, body), expected, "{version:?}: {body}");
        }
    }
    // Assertions and type alternatives are XSD 1.1's (and the `xsd11`
    // feature's).
    #[cfg(feature = "xsd11")]
    for body in [
        "<xs:simpleType name='s'><xs:restriction base='xs:untypedAtomic'><xs:assertion test='true()'/></xs:restriction></xs:simpleType>",
        "<xs:element name='u'><xs:alternative test='@x' type='xs:untypedAtomic'/></xs:element>",
    ] {
        assert_eq!(refusal(XsdVersion::V1_1, body), "src-resolve", "1.1: {body}");
    }
}

/// Conditional inclusion (Structures 1.1 §4.2.2): `vc:typeAvailable` names
/// types ·automatically known· to the processor, which here are those a
/// schema document can name. `xs:untypedAtomic` is not one in either version,
/// the three other types of the data model only in 1.1.
#[test]
fn vc_type_available_answers_what_a_schema_document_can_name() {
    const VC: &str = "http://www.w3.org/2007/XMLSchema-versioning";
    for version in VERSIONS {
        let v11 = version == XsdVersion::V1_1;
        for (local, available) in [
            ("untypedAtomic", false),
            ("anyAtomicType", v11),
            ("dayTimeDuration", v11),
            ("yearMonthDuration", v11),
            ("string", true),
        ] {
            let xsd = format!(
                "<xs:schema xmlns:xs='{XS}' xmlns:vc='{VC}'>\
                   <xs:element name='a' vc:typeAvailable='xs:{local}'/>\
                   <xs:element name='b' vc:typeUnavailable='xs:{local}'/>\
                 </xs:schema>"
            );
            let ss = compile(version, &xsd).unwrap_or_else(|e| panic!("{version:?} {local}: {e}"));
            let has = |name: &str| ss.lookup_element(None, ss.name_table.add(name)).is_some();
            assert_eq!(
                has("a"),
                available,
                "{version:?}: vc:typeAvailable='xs:{local}'"
            );
            assert_eq!(
                has("b"),
                !available,
                "{version:?}: vc:typeUnavailable='xs:{local}'"
            );
        }
    }
}

// ── Simple values ────────────────────────────────────────────────────────

/// (type, lexical form, valid, typed value's code, its string value).
type ValueCase = (XmlTypeCode, &'static str, bool, XmlTypeCode, &'static str);

const UA: XmlTypeCode = XmlTypeCode::UntypedAtomic;
const AA: XmlTypeCode = XmlTypeCode::AnyAtomicType;
const DT: XmlTypeCode = XmlTypeCode::DayTimeDuration;
const YM: XmlTypeCode = XmlTypeCode::YearMonthDuration;

const VALUES: [ValueCase; 17] = [
    // XDM 1.0 §2.6.4: "When xs:untypedAtomic is specified as the type, no
    // validation takes place" — every string, unnormalized.
    (UA, "  a \n b ", true, UA, "  a \n b "),
    (UA, "", true, UA, ""),
    // Datatypes 1.1 §3.2.2.2: anyAtomicType's lexical space is every Char
    // sequence; XDM 1.0 §3.3.1.2: "If T is xs:anySimpleType or
    // xs:anyAtomicType, the typed value is the [schema normalized value] as
    // an instance of xs:untypedAtomic."
    (AA, " x  y ", true, UA, " x  y "),
    // Datatypes 1.1 §3.4.27: '[^YM]*[DT].*' on duration's lexical space.
    (DT, "PT1H", true, DT, "PT1H"),
    (DT, " P1DT2H ", true, DT, "P1DT2H"),
    (DT, "P1Y", false, DT, ""),
    (DT, "P1M1D", false, DT, ""),
    // A lexical form of duration first (§3.3.6.2): "T" needs a time field,
    // digits have no sign, the fields come in order. Before this batch the
    // 1.1 validators took `PT`, `P1DT`, `P+1D` and `P+1Y`.
    (DT, "PT", false, DT, ""),
    (DT, "P1DT", false, DT, ""),
    (DT, "P+1D", false, DT, ""),
    (DT, "PT1M1H", false, DT, ""),
    // §3.4.26: '[^DT]*'.
    (YM, "P1Y2M", true, YM, "P1Y2M"),
    (YM, "-P3M", true, YM, "-P3M"),
    (YM, "P1D", false, YM, ""),
    (YM, "1Y", false, YM, ""),
    (YM, "P+1Y", false, YM, ""),
    (YM, "P1M1Y", false, YM, ""),
];

#[test]
fn simple_values_against_the_four() {
    for version in VERSIONS {
        let ss = schema(version);
        for (code, lexical, valid, value_code, string) in VALUES {
            let result = validate_simple_type(lexical, key(&ss, code), &ss);
            match result {
                Ok(result) => {
                    assert!(valid, "{version:?} {code:?}: {lexical:?} must be invalid");
                    assert_eq!(
                        result.typed_value.type_code, value_code,
                        "{version:?} {lexical:?}"
                    );
                    assert_eq!(
                        result.typed_value.to_string_value(),
                        string,
                        "{version:?} {lexical:?}"
                    );
                    if code == XmlTypeCode::UntypedAtomic {
                        assert!(
                            matches!(result.typed_value.value, XmlValueKind::UntypedAtomic(_)),
                            "{version:?}: untypedAtomic's typed value"
                        );
                    }
                }
                Err(e) => assert!(
                    !valid,
                    "{version:?} {code:?}: {lexical:?} must be valid: {e}"
                ),
            }
        }
    }
}

// ── Element runs: every driver ───────────────────────────────────────────

type Outcome = (Option<SchemaValidity>, Vec<String>);

fn stream(ss: &SchemaSet, root: ValidationRoot, xml: &str, flags: ValidationFlags) -> Outcome {
    let v = SchemaValidator::new(ss, flags);
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    let validity = {
        let mut rt = v.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        rt.set_validation_root(root);
        drive_quick_xml(xml.as_bytes(), &mut rt, ss)
            .expect("the stream drive completes")
            .root_validity
    };
    (validity, codes(&errors))
}

fn dom(ss: &SchemaSet, root: ValidationRoot, xml: &str, flags: ValidationFlags) -> Outcome {
    let v = SchemaValidator::new(ss, flags);
    let doc = roxmltree::Document::parse(xml).expect("well-formed");
    let nav = RoXmlNavigator::new(&doc);
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    let validity = {
        let mut rt = v.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        rt.set_validation_root(root);
        drive_navigator(&nav, &mut rt, ss)
            .expect("the navigator drive completes")
            .root_validity
    };
    (validity, codes(&errors))
}

#[cfg(feature = "xsd11")]
fn buffer(ss: &SchemaSet, root: ValidationRoot, xml: &str, flags: ValidationFlags) -> Outcome {
    use xsd_schema::document::{BufferDocument, BufferDocumentOptions};
    use xsd_schema::validation::drive_buffer_document;
    let v = SchemaValidator::new(ss, flags);
    let arena = bumpalo::Bump::new();
    let doc = BufferDocument::from_reader(
        xml.as_bytes(),
        &arena,
        &ss.name_table,
        BufferDocumentOptions::default(),
        None,
    )
    .expect("buffer build");
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    let validity = {
        let mut rt = v.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        rt.set_validation_root(root);
        drive_buffer_document(&doc, &mut rt, ss)
            .expect("the buffer drive completes")
            .root_validity
    };
    (validity, codes(&errors))
}

/// The typed build: validity, errors, and the document element's annotation
/// and typed value as the navigator reads them back.
#[cfg(feature = "xsd11")]
fn typed(
    ss: &SchemaSet,
    root: ValidationRoot,
    xml: &str,
    flags: ValidationFlags,
) -> (
    Outcome,
    Option<TypeKey>,
    Option<xsd_schema::types::value::XmlValue>,
) {
    use xsd_schema::document::{build_typed_document_with, TypedBuildOptions};
    use xsd_schema::navigator::{DomNavigator, TypedValue};
    use xsd_schema::validation::NoopHandler;
    let v = SchemaValidator::new(ss, flags);
    let arena = bumpalo::Bump::new();
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    let (validity, annotation, value) = {
        let mut rt = v.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        rt.set_validation_root(root);
        let built = build_typed_document_with(
            xml.as_bytes(),
            &arena,
            ss,
            &mut rt,
            &mut NoopHandler,
            TypedBuildOptions::default(),
        )
        .expect("the typed build completes");
        rt.end_validation().expect("end_validation");
        let mut nav = built.document.create_navigator();
        assert!(nav.move_to_first_child(), "a document element");
        let value = match nav.typed_value() {
            TypedValue::Value(v) => Some(v),
            _ => None,
        };
        (built.outcome.root_validity, nav.type_annotation(), value)
    };
    ((validity, codes(&errors)), annotation, value)
}

/// Every driver gives the root `[validity]` `expected`, with an error whose
/// code starts with `code` (`None`: no error).
fn check(
    ss: &SchemaSet,
    root: ValidationRoot,
    xml: &str,
    flags: ValidationFlags,
    expected: SchemaValidity,
    code: Option<&str>,
) {
    let label = format!("{:?} {root:?} {xml}", ss.xsd_version);
    let lean = flags & !ValidationFlags::BUILD_PSVI_TYPED_VALUES;
    let runs = vec![
        ("stream", stream(ss, root, xml, flags)),
        ("stream/no-psvi", stream(ss, root, xml, lean)),
        ("navigator", dom(ss, root, xml, flags)),
    ];
    #[cfg(feature = "xsd11")]
    let runs = {
        let mut runs = runs;
        runs.push(("buffer", buffer(ss, root, xml, flags)));
        runs.push(("typed build", typed(ss, root, xml, flags).0));
        runs
    };
    for (driver, (validity, errors)) in runs {
        assert_eq!(
            validity,
            Some(expected),
            "{label} {driver}: errors {errors:?}"
        );
        match code {
            Some(code) => assert!(
                errors.iter().any(|e| e.starts_with(code)),
                "{label} {driver}: expected {code}, got {errors:?}"
            ),
            None => assert!(errors.is_empty(), "{label} {driver}: errors {errors:?}"),
        }
    }
}

fn z(attrs: &str, content: &str) -> String {
    format!(
        r#"<z xmlns:xs="{XS}" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"{attrs}>{content}</z>"#
    )
}

/// Type-driven validation (Structures §5.2) of an element against each of
/// the four, as a host stipulates the type: simple content (§3.3.4.4 clause
/// 3.1: no attributes but `xsi:*`, no element children, the text String
/// Valid).
#[test]
fn an_element_against_each_type_on_every_driver() {
    for version in VERSIONS {
        let ss = schema(version);
        for flags in [
            ValidationFlags::default(),
            ValidationFlags::default() | HOST,
        ] {
            for (code, lexical, valid, _, _) in VALUES {
                let root = ValidationRoot::Type(key(&ss, code));
                let expected = if valid { Valid } else { Invalid };
                let error = (!valid).then_some("cvc-");
                check(&ss, root, &z("", lexical), flags, expected, error);
            }
            for (code, _) in XPATH_TYPES {
                let root = ValidationRoot::Type(key(&ss, code));
                // §3.3.4.4 clause 3.1.2: no element children; clause 3.1.1: no
                // attributes but xsi:* (reported with the codes the runtime
                // gives for every simple type).
                check(&ss, root, &z("", "PT1H<c/>"), flags, Invalid, Some("cvc-"));
                check(
                    &ss,
                    root,
                    &z(r#" a="1""#, "PT1H"),
                    flags,
                    Invalid,
                    Some("cvc-"),
                );
            }
        }
    }
}

/// The push API (what a host drives): the root's `SchemaInfo` names the type
/// and carries its typed value.
fn push(
    ss: &SchemaSet,
    flags: ValidationFlags,
    root: ValidationRoot,
    xsi_type: Option<&str>,
    text: &str,
) -> (SchemaInfo, Vec<String>) {
    let v = SchemaValidator::new(ss, flags);
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    let info = {
        let mut rt = v.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        rt.set_validation_root(root);
        let ns = xs_snapshot(ss);
        rt.validate_element("z", "", xsi_type, None, &ns);
        rt.validate_end_of_attributes();
        rt.validate_text(text);
        let info = rt.validate_end_element();
        rt.end_validation().expect("end_validation");
        info
    };
    (info, codes(&errors))
}

#[test]
fn the_push_api_annotates_with_the_type() {
    for version in VERSIONS {
        let ss = schema(version);
        for (code, lexical, valid, value_code, string) in VALUES {
            if !valid {
                continue;
            }
            let k = key(&ss, code);
            let (info, errors) = push(&ss, HOST, ValidationRoot::Type(k), None, lexical);
            assert!(
                errors.is_empty(),
                "{version:?} {code:?} {lexical:?}: {errors:?}"
            );
            assert_eq!(info.validity, Valid, "{version:?} {code:?}");
            assert_eq!(
                info.schema_type,
                Some(k),
                "{version:?} {code:?}: the annotation"
            );
            let value = info.typed_value.expect("a typed value");
            assert_eq!(value.type_code, value_code, "{version:?} {code:?}");
            assert_eq!(value.to_string_value(), string, "{version:?} {code:?}");
        }
    }
}

// ── xsi:type ─────────────────────────────────────────────────────────────

/// (stipulated type, `xsi:type`, text, the annotation's code when valid, the
/// error's code prefix when not).
type XsiCase<'a> = (
    ValidationRoot,
    &'a str,
    &'a str,
    Option<XmlTypeCode>,
    Option<&'a str>,
);

/// With [`ValidationFlags::XPATH_TYPES`] an `xsi:type` resolves the four in
/// both versions and overrides the stipulated type when ·validly
/// substitutable· (Structures §3.3.4.2; §3.16.6.3 with the four's places in
/// the hierarchy).
#[test]
fn xsi_type_with_the_host_flag() {
    for version in VERSIONS {
        let ss = schema(version);
        let any_atomic = ValidationRoot::Type(key(&ss, XmlTypeCode::AnyAtomicType));
        let untyped_atomic = ValidationRoot::Type(key(&ss, XmlTypeCode::UntypedAtomic));
        let duration = ValidationRoot::Type(builtin(&ss, XmlTypeCode::Duration));
        let cases: [XsiCase; 12] = [
            // Under anyAtomicType, any atomic type (XDM 1.0 §2.6.2: it
            // "includes all atomic values (and no values that are not atomic)").
            (
                any_atomic,
                "xs:integer",
                "42",
                Some(XmlTypeCode::Integer),
                None,
            ),
            (any_atomic, "xs:integer", "x", None, Some("cvc-")),
            (
                any_atomic,
                "xs:untypedAtomic",
                "x",
                Some(XmlTypeCode::UntypedAtomic),
                None,
            ),
            (
                any_atomic,
                "xs:dayTimeDuration",
                "PT1H",
                Some(XmlTypeCode::DayTimeDuration),
                None,
            ),
            (
                any_atomic,
                "xs:yearMonthDuration",
                "PT1H",
                None,
                Some("cvc-"),
            ),
            (
                any_atomic,
                "xs:anyAtomicType",
                "x",
                Some(XmlTypeCode::AnyAtomicType),
                None,
            ),
            // "Its base type is xs:anySimpleType": not derived from it.
            (
                any_atomic,
                "xs:anySimpleType",
                "x",
                None,
                Some("cvc-elt.4.2"),
            ),
            (any_atomic, "xs:NMTOKENS", "a b", None, Some("cvc-elt.4.2")),
            // "No predefined types are derived from xs:untypedAtomic."
            (untyped_atomic, "xs:string", "x", None, Some("cvc-elt.4.2")),
            (
                untyped_atomic,
                "xs:untypedAtomic",
                "x",
                Some(XmlTypeCode::UntypedAtomic),
                None,
            ),
            (
                duration,
                "xs:dayTimeDuration",
                "P1D",
                Some(XmlTypeCode::DayTimeDuration),
                None,
            ),
            (duration, "xs:yearMonthDuration", "P1D", None, Some("cvc-")),
        ];
        for (root, xsi, text, annotated, error) in cases {
            let (info, errors) = push(&ss, HOST, root, Some(xsi), text);
            let label = format!("{version:?} {root:?} xsi:type={xsi} {text:?}");
            match error {
                None => {
                    assert!(errors.is_empty(), "{label}: {errors:?}");
                    assert_eq!(info.validity, Valid, "{label}");
                    let code = annotated.expect("the annotation");
                    assert_eq!(info.schema_type, xpath_key(&ss, code), "{label}");
                }
                Some(error) => {
                    assert_eq!(info.validity, Invalid, "{label}");
                    assert!(
                        errors.iter().any(|e| e.starts_with(error)),
                        "{label}: {errors:?}"
                    );
                }
            }
            // The drivers agree.
            let xml = z(&format!(r#" xsi:type="{xsi}""#), text);
            let expected = if error.is_none() { Valid } else { Invalid };
            check(
                &ss,
                root,
                &xml,
                ValidationFlags::default() | HOST,
                expected,
                error,
            );
        }
    }
}

/// Without the flag an `xsi:type` resolves what the set's XSD version
/// defines, as before: in 1.0 none of the four (`cvc-elt.4.1`), in 1.1 all
/// but `xs:untypedAtomic` (the W3C 1.1 suite's `simple050.n03`: an
/// `xsi:type="xs:untypedAtomic"` under `xs:anyAtomicType` is invalid).
#[test]
fn xsi_type_without_the_host_flag_is_what_the_version_defines() {
    let plain = ValidationFlags::default();
    for version in VERSIONS {
        let ss = schema(version);
        let decl = ValidationRoot::Declaration;
        for (_, local) in XPATH_TYPES {
            let text = if local.ends_with("Duration") {
                "P1Y"
            } else {
                "x"
            };
            let text = if local == "dayTimeDuration" {
                "PT1H"
            } else {
                text
            };
            let xml = format!(
                r#"<any xmlns:xs="{XS}" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:type="xs:{local}">{text}</any>"#
            );
            let defined = version == XsdVersion::V1_1 && local != "untypedAtomic";
            if defined {
                check(&ss, decl, &xml, plain, Valid, None);
            } else {
                check(&ss, decl, &xml, plain, Invalid, Some("cvc-elt.4.1"));
                check(&ss, decl, &xml, plain | HOST, Valid, None);
            }
        }
        // A 1.0 schema's xs:duration element and an xsi:type naming a
        // duration subtype: unresolved in 1.0 unless the host asks.
        let xml = format!(
            r#"<dur xmlns:xs="{XS}" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:type="xs:dayTimeDuration">PT1H</dur>"#
        );
        if version == XsdVersion::V1_0 {
            check(&ss, decl, &xml, plain, Invalid, Some("cvc-elt.4.1"));
        } else {
            check(&ss, decl, &xml, plain, Valid, None);
        }
        check(&ss, decl, &xml, plain | HOST, Valid, None);
    }
}

// ── The typed tree and XPath (the M4g review's R3 list) ──────────────────

#[cfg(feature = "xsd11")]
mod xpath {
    use super::*;
    use xsd_schema::document::{
        build_typed_document_with, BufferDocument, BufferDocumentOptions, TypedBuildOptions,
    };
    use xsd_schema::validation::NoopHandler;
    use xsd_schema::xpath::api::XPathExpr;
    use xsd_schema::xpath::{XPathContext, XmlItem};

    const DOC: &str = r#"<r xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
<d xsi:type="xs:dayTimeDuration">PT1H</d><d xsi:type="xs:dayTimeDuration">PT3H</d><d xsi:type="xs:dayTimeDuration">PT2H</d>
<y xsi:type="xs:yearMonthDuration">P1Y</y><y xsi:type="xs:yearMonthDuration">P2M</y>
<u xsi:type="xs:untypedAtomic">  a  b </u><a xsi:type="xs:anyAtomicType"> c </a></r>"#;

    fn with_doc<F: FnOnce(&BufferDocument<'_>)>(ss: &SchemaSet, xml: &str, check: F) {
        let v = SchemaValidator::new(ss, ValidationFlags::default() | HOST);
        let (mut errors, mut warnings) = (Vec::new(), Vec::new());
        let arena = bumpalo::Bump::new();
        let mut rt = v.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        let built = build_typed_document_with(
            xml.as_bytes(),
            &arena,
            ss,
            &mut rt,
            &mut NoopHandler,
            TypedBuildOptions::default().with_document_options(BufferDocumentOptions::full()),
        )
        .expect("the typed build completes");
        rt.end_validation().expect("end_validation");
        drop(rt);
        assert_eq!(built.outcome.root_validity, Some(Valid), "{errors:?}");
        check(&built.document);
    }

    fn eval<'d>(
        ss: &SchemaSet,
        doc: &'d BufferDocument<'_>,
        expr: &str,
    ) -> Result<Vec<XmlItem<xsd_schema::document::BufferDocNavigator<'d>>>, String> {
        let mut ns = NamespaceContextSnapshot::default();
        ns.bindings
            .push((ss.name_table.add("xs"), ss.name_table.add(XS)));
        let ctx = XPathContext::new(&ss.name_table)
            .with_schema_set(ss)
            .with_namespaces(ns);
        let compiled = XPathExpr::compile(expr, &ctx).map_err(|e| format!("static: {e}"))?;
        let value = compiled
            .evaluator(&ctx)
            .run_with_node(doc.create_navigator())
            .map_err(|e| format!("dynamic: {e}"))?;
        Ok(value.into_vec())
    }

    fn eval1(ss: &SchemaSet, doc: &BufferDocument<'_>, expr: &str) -> String {
        match eval(ss, doc, expr) {
            Ok(items) => items
                .iter()
                .map(|item| match item {
                    XmlItem::Node(node) => {
                        use xsd_schema::navigator::DomNavigator;
                        node.value()
                    }
                    XmlItem::Atomic(value) => value.to_string_value(),
                })
                .collect::<Vec<_>>()
                .join(" "),
            Err(e) => format!("error {e}"),
        }
    }

    /// Each expression and its right answer — the 1.1 answers of the review's
    /// table, now in both versions.
    const R3: [(&str, &str); 30] = [
        ("data(/r/d[1]) instance of xs:dayTimeDuration", "true"),
        ("data(/r/d[1]) eq xs:dayTimeDuration('PT1H')", "true"),
        ("data(/r/d[1]) lt xs:dayTimeDuration('PT2H')", "true"),
        ("data(/r/d[2]) gt data(/r/d[3])", "true"),
        ("data(/r/d[1]) + xs:dayTimeDuration('PT1H')", "PT2H"),
        ("data(/r/d[1]) * 2", "PT2H"),
        ("data(/r/y[1]) + data(/r/y[2])", "P1Y2M"),
        ("xs:date('2020-01-01') + data(/r/y[1])", "2021-01-01"),
        ("sum(/r/d)", "PT6H"),
        ("avg(/r/d)", "PT2H"),
        ("max(/r/d)", "PT3H"),
        ("min(/r/d)", "PT1H"),
        ("sum(/r/y)", "P1Y2M"),
        ("/r/d[1] instance of element(*, xs:dayTimeDuration)", "true"),
        ("/r/d[1] instance of element(*, xs:duration)", "true"),
        ("/r/d[1] instance of element(*, xs:anyAtomicType)", "true"),
        (
            "/r/d[1] instance of element(*, xs:yearMonthDuration)",
            "false",
        ),
        (
            "/r/y[1] instance of element(*, xs:yearMonthDuration)",
            "true",
        ),
        ("count(/r/element(*, xs:dayTimeDuration))", "3"),
        ("count(/r/element(*, xs:anyAtomicType))", "7"),
        ("/r/u instance of element(*, xs:untypedAtomic)", "true"),
        ("/r/u instance of element(*, xs:anyAtomicType)", "true"),
        ("/r/u instance of element(*, xs:string)", "false"),
        ("data(/r/u) instance of xs:untypedAtomic", "true"),
        ("string(/r/u) eq '  a  b '", "true"),
        ("/r/a instance of element(*, xs:anyAtomicType)", "true"),
        ("/r/a instance of element(*, xs:untypedAtomic)", "false"),
        ("data(/r/a) instance of xs:untypedAtomic", "true"),
        ("hours-from-duration(data(/r/d[2]))", "3"),
        ("/r/element(*, xs:untyped)", ""),
    ];

    #[test]
    fn the_review_r3_list_on_the_typed_tree() {
        for version in VERSIONS {
            let ss = schema(version);
            with_doc(&ss, DOC, |doc| {
                for (expr, expected) in R3 {
                    assert_eq!(eval1(&ss, doc, expr), expected, "{version:?}: {expr}");
                }
                // Sorting as a host sorts: by `value_lt` on the atomized values.
                let mut values: Vec<_> = eval(&ss, doc, "data(/r/d)")
                    .expect("atomized")
                    .into_iter()
                    .map(|item| match item {
                        XmlItem::Atomic(value) => value,
                        XmlItem::Node(_) => panic!("atomized"),
                    })
                    .collect();
                values.sort_by(|a, b| {
                    if xsd_schema::xpath::operators::value_lt(a, b).expect("ordered") {
                        std::cmp::Ordering::Less
                    } else if xsd_schema::xpath::operators::value_lt(b, a).expect("ordered") {
                        std::cmp::Ordering::Greater
                    } else {
                        std::cmp::Ordering::Equal
                    }
                });
                let sorted: Vec<_> = values.iter().map(|v| v.to_string_value()).collect();
                assert_eq!(sorted, ["PT1H", "PT2H", "PT3H"], "{version:?}");
            });
        }
    }

    /// The navigator reads the annotation back as the set's key.
    #[test]
    fn the_navigator_reads_the_annotation_back() {
        use xsd_schema::navigator::DomNavigator;
        for version in VERSIONS {
            let ss = schema(version);
            for (code, lexical, valid, value_code, _) in VALUES {
                if !valid {
                    continue;
                }
                let k = key(&ss, code);
                let ((validity, errors), annotation, value) = typed(
                    &ss,
                    ValidationRoot::Type(k),
                    &z("", lexical),
                    ValidationFlags::default() | HOST,
                );
                assert_eq!(validity, Some(Valid), "{version:?} {code:?}: {errors:?}");
                assert_eq!(annotation, Some(k), "{version:?} {code:?}");
                assert_eq!(
                    value.map(|v| v.type_code),
                    Some(value_code),
                    "{version:?} {code:?}"
                );
            }
            // An element child of `r` typed by xsi:type.
            with_doc(&ss, DOC, |doc| {
                let mut nav = doc.create_navigator();
                assert!(nav.move_to_first_child() && nav.move_to_first_child());
                while nav.node_type() != xsd_schema::navigator::DomNodeType::Element {
                    assert!(nav.move_to_next_sibling());
                }
                assert_eq!(
                    nav.type_annotation(),
                    Some(key(&ss, XmlTypeCode::DayTimeDuration))
                );
            });
        }
    }

    /// Casts and constructor functions need no schema component (XPath 2.0
    /// §3.10.4, §3.12.3); they answer the same in both versions.
    #[test]
    fn casts_and_constructors() {
        for version in VERSIONS {
            let ss = schema(version);
            with_doc(&ss, "<any/>", |doc| {
                for (expr, expected) in [
                    ("xs:untypedAtomic(' a ') instance of xs:untypedAtomic", "true"),
                    ("string(xs:untypedAtomic(' a '))", " a "),
                    ("xs:dayTimeDuration('PT90M')", "PT1H30M"),
                    ("xs:untypedAtomic('P1Y') cast as xs:yearMonthDuration", "P1Y"),
                    ("xs:dayTimeDuration('PT1H') cast as xs:untypedAtomic instance of xs:untypedAtomic", "true"),
                    ("xs:duration('P1DT1H') cast as xs:dayTimeDuration", "P1DT1H"),
                    ("'P1D' castable as xs:yearMonthDuration", "false"),
                ] {
                    assert_eq!(eval1(&ss, doc, expr), expected, "{version:?}: {expr}");
                }
            });
        }
    }
}
