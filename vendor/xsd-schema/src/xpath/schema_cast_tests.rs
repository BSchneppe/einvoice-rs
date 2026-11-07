//! `cast as`, `castable as` and constructor functions for the user-defined
//! atomic types of the schema set attached to the static context.
//!
//! XPath 2.0 §3.10.2: "The target type must be an atomic type that is in the
//! in-scope schema types \[err:XPST0051\]. In addition, the target type cannot be
//! xs:NOTATION or xs:anyAtomicType \[err:XPST0080\]." §3.10.4: "For every atomic
//! type in the in-scope schema types (except xs:NOTATION and xs:anyAtomicType,
//! which are not instantiable), a constructor function is implicitly defined.
//! In each case, the name of the constructor function is the same as the name
//! of its target type (including namespace)", and `T($arg)` is `($arg) cast as
//! T?`. The casting rules between types derived from primitives are those of
//! F&O §17.2 – §17.5: identity, up the hierarchy (a new annotation, always
//! succeeds), within one primitive (the target's facets, the pattern on the
//! canonical form, FORG0001) and across primitives (through the primitives;
//! the pattern checked on a string or `xs:untypedAtomic` source).

use crate::namespace::context::NamespaceContextSnapshot;
use crate::namespace::table::XS_NAMESPACE;
use crate::schema::SchemaSet;
use crate::xpath::functions::signature::types::{any, string};
use crate::xpath::functions::{DynamicFunctionSignature, FunctionSet, XPathValue};
use crate::xpath::iterator::XmlItem;
use crate::xpath::{RoXmlNavigator, XPathContext, XPathExpr};
use crate::{SchemaSetBuilder, XsdVersion};

const MY_NS: &str = "urn:c37";

const SCHEMA: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           targetNamespace="urn:c37" xmlns:my="urn:c37">
  <xs:notation name="png" public="image/png"/>
  <xs:notation name="gif" public="image/gif"/>
  <xs:simpleType name="hatSize">
    <xs:restriction base="xs:integer">
      <xs:minInclusive value="1"/><xs:maxInclusive value="20"/>
    </xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="smallHat">
    <xs:restriction base="my:hatSize"><xs:maxInclusive value="10"/></xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="evenHat">
    <xs:restriction base="my:hatSize"><xs:pattern value="\d*[02468]"/></xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="price">
    <xs:restriction base="xs:decimal">
      <xs:minInclusive value="0"/><xs:fractionDigits value="2"/>
    </xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="twoPlaces">
    <xs:restriction base="xs:decimal"><xs:pattern value="\d+\.\d{2}"/></xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="positive">
    <xs:restriction base="xs:decimal"><xs:minExclusive value="0"/></xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="nonNegative">
    <xs:restriction base="xs:decimal"><xs:minInclusive value="0"/></xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="tinyEnum">
    <xs:restriction base="xs:decimal">
      <xs:enumeration value="0.000000000000000000000000000001"/><xs:enumeration value="1"/>
    </xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="percent">
    <xs:restriction base="xs:double">
      <xs:minInclusive value="0"/><xs:maxInclusive value="100"/>
    </xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="dbl"><xs:restriction base="xs:double"/></xs:simpleType>
  <xs:simpleType name="code">
    <xs:restriction base="xs:string"><xs:pattern value="[A-Z]{3}"/></xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="short3">
    <xs:restriction base="xs:string"><xs:maxLength value="3"/></xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="pair">
    <xs:restriction base="xs:string"><xs:length value="2"/></xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="color">
    <xs:restriction base="xs:string">
      <xs:enumeration value="red"/><xs:enumeration value="green"/><xs:enumeration value="blue"/>
    </xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="collapsed">
    <xs:restriction base="xs:string">
      <xs:whiteSpace value="collapse"/><xs:maxLength value="3"/>
    </xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="anyDate"><xs:restriction base="xs:date"/></xs:simpleType>
  <xs:simpleType name="ub"><xs:restriction base="xs:unsignedByte"/></xs:simpleType>
  <xs:simpleType name="y2k">
    <xs:restriction base="xs:date"><xs:minInclusive value="2000-01-01"/></xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="qn"><xs:restriction base="xs:QName"/></xs:simpleType>
  <xs:simpleType name="myqn">
    <xs:restriction base="xs:QName"><xs:pattern value="my:.+"/></xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="picture">
    <xs:restriction base="xs:NOTATION">
      <xs:enumeration value="my:png"/><xs:enumeration value="my:gif"/>
    </xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="pngOnly">
    <xs:restriction base="my:picture"><xs:enumeration value="my:png"/></xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="ent"><xs:restriction base="xs:ENTITY"/></xs:simpleType>
  <xs:simpleType name="ints"><xs:list itemType="xs:integer"/></xs:simpleType>
  <xs:simpleType name="shortInts">
    <xs:restriction base="my:ints"><xs:maxLength value="2"/></xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="intOrDate"><xs:union memberTypes="xs:integer xs:date"/></xs:simpleType>
  <xs:complexType name="ct"><xs:sequence/></xs:complexType>
  <xs:element name="doc">
    <xs:complexType>
      <xs:sequence>
        <xs:element name="s" type="my:smallHat"/>
        <xs:element name="c" type="my:code"/>
      </xs:sequence>
      <xs:attribute name="h" type="my:hatSize"/>
    </xs:complexType>
  </xs:element>
</xs:schema>"#;

/// A user type in no namespace (F&O §5.4: "if the user-defined type apple is
/// derived from xs:integer but is not in a namespace …").
const NO_NAMESPACE_SCHEMA: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:simpleType name="apple">
    <xs:restriction base="xs:integer"><xs:maxInclusive value="100"/></xs:restriction>
  </xs:simpleType>
</xs:schema>"#;

/// An XSD 1.1 assertion facet on a simple type (Datatypes 1.1 §4.3.13).
const ASSERTION_FACET_SCHEMA: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           targetNamespace="urn:c37" xmlns:my="urn:c37">
  <xs:simpleType name="even">
    <xs:restriction base="xs:integer"><xs:assertion test="$value mod 2 = 0"/></xs:restriction>
  </xs:simpleType>
</xs:schema>"#;

fn compile_set(version: XsdVersion, xsd: &str) -> SchemaSet {
    SchemaSetBuilder::with_version(version)
        .add_source(xsd, "file:///c37.xsd")
        .expect("schema source accepted")
        .compile()
        .expect("schema compiles")
        .into_schema_set()
}

fn namespaces(set: &SchemaSet) -> NamespaceContextSnapshot {
    let mut namespaces = NamespaceContextSnapshot::default();
    for (prefix, uri) in [("xs", XS_NAMESPACE), ("my", MY_NS)] {
        namespaces
            .bindings
            .push((set.name_table.add(prefix), set.name_table.add(uri)));
    }
    namespaces
}

fn context(set: &SchemaSet) -> XPathContext<'_> {
    XPathContext::new(&set.name_table)
        .with_namespaces(namespaces(set))
        .with_schema_set(set)
}

/// The value of `expr` as its items' string values joined with a space, `()`
/// for the empty sequence, or `error:CODE`.
fn render(result: Result<XPathValue<RoXmlNavigator<'static>>, crate::xpath::XPathError>) -> String {
    match result {
        Err(e) => format!("error:{}", e.error_code().unwrap_or("?")),
        Ok(v) if v.is_empty() => "()".to_string(),
        Ok(v) => v
            .into_vec()
            .iter()
            .map(|item| match item {
                XmlItem::Atomic(a) => a.to_string_value(),
                XmlItem::Node(n) => crate::xpath::DomNavigator::value(n),
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn eval(ctx: &XPathContext<'_>, expr: &str) -> String {
    match XPathExpr::compile(expr, ctx) {
        Err(e) => format!("error:{}", e.error_code().unwrap_or("?")),
        Ok(compiled) => render(compiled.evaluator(ctx).run::<RoXmlNavigator<'static>>()),
    }
}

/// Evaluate every `(expr, expected)` pair against the main schema in both XSD
/// versions and report all mismatches at once.
fn check_both_versions(cases: &[(&str, &str)]) {
    let mut failures = Vec::new();
    for version in [XsdVersion::V1_0, XsdVersion::V1_1] {
        let set = compile_set(version, SCHEMA);
        let ctx = context(&set);
        for (expr, expected) in cases {
            let got = eval(&ctx, expr);
            if got != *expected {
                failures.push(format!(
                    "{version:?}: {expr}  =>  {got}  (expected {expected})"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// Constructor and `cast as` / `castable as` to a type derived from
/// `xs:integer`: §17.4 facets, FORG0001, the casting table's XPTY0004, the
/// empty sequence and `?`, and §3.10.2's "more than one atomic value".
#[test]
fn integer_derived_constructor_cast_and_castable() {
    check_both_versions(&[
        ("my:hatSize('7')", "7"),
        ("my:hatSize(' 7 ')", "7"),
        ("my:hatSize('7') instance of my:hatSize", "true"),
        ("my:hatSize('7') instance of xs:integer", "true"),
        ("my:hatSize('7') instance of my:smallHat", "false"),
        ("'7' cast as my:hatSize", "7"),
        ("'7' cast as my:hatSize instance of my:hatSize", "true"),
        (
            "xs:untypedAtomic('7') cast as my:hatSize instance of my:hatSize",
            "true",
        ),
        ("my:hatSize('21')", "error:FORG0001"),
        ("my:hatSize('0')", "error:FORG0001"),
        ("my:hatSize('x')", "error:FORG0001"),
        // Within the xs:decimal primitive: decimal → integer truncates
        // (F&O §17.1.3.4), then the target's facets apply.
        ("my:hatSize(7.9)", "7"),
        ("my:hatSize(xs:double('12.5'))", "12"),
        ("my:hatSize(true())", "1"),
        ("my:hatSize(xs:date('2000-01-01'))", "error:XPTY0004"),
        ("xs:date('2000-01-01') cast as my:hatSize", "error:XPTY0004"),
        ("my:hatSize(())", "()"),
        ("() cast as my:hatSize?", "()"),
        ("() cast as my:hatSize", "error:XPTY0004"),
        ("my:hatSize((1, 2))", "error:XPTY0004"),
        ("'7' castable as my:hatSize", "true"),
        ("'27' castable as my:hatSize", "false"),
        ("'x' castable as my:hatSize", "false"),
        ("xs:date('2000-01-01') castable as my:hatSize", "false"),
        ("() castable as my:hatSize", "false"),
        ("() castable as my:hatSize?", "true"),
        ("(1, 2) castable as my:hatSize", "false"),
    ]);
}

/// F&O §17.2's four cases along a two-level chain (`smallHat` ⊂ `hatSize` ⊂
/// `xs:integer`, `evenHat` ⊂ `hatSize`): identity, up (§17.3: "The result will
/// have the same value as the original, but will have a new type
/// annotation"), down and within (§17.4), and to a built-in type, whose result
/// carries no user annotation any more.
#[test]
fn casts_up_down_and_within_a_derivation_chain() {
    check_both_versions(&[
        ("my:smallHat('10')", "10"),
        ("my:smallHat('11')", "error:FORG0001"),
        // The inherited facet of the base, minInclusive 1.
        ("my:smallHat('0')", "error:FORG0001"),
        (
            "my:smallHat('5') cast as my:smallHat instance of my:smallHat",
            "true",
        ),
        (
            "my:smallHat('5') cast as my:hatSize instance of my:hatSize",
            "true",
        ),
        (
            "my:smallHat('5') cast as my:hatSize instance of my:smallHat",
            "false",
        ),
        (
            "my:hatSize('5') cast as my:smallHat instance of my:smallHat",
            "true",
        ),
        ("my:hatSize('12') cast as my:smallHat", "error:FORG0001"),
        ("my:hatSize('12') castable as my:smallHat", "false"),
        ("my:smallHat('4') cast as my:evenHat", "4"),
        (
            "my:smallHat('4') cast as my:evenHat instance of my:evenHat",
            "true",
        ),
        (
            "my:smallHat('4') cast as my:evenHat instance of my:smallHat",
            "false",
        ),
        ("my:smallHat('5') cast as my:evenHat", "error:FORG0001"),
        ("12 cast as my:smallHat", "error:FORG0001"),
        ("8 cast as my:smallHat instance of my:smallHat", "true"),
        (
            "xs:short('8') cast as my:smallHat instance of my:hatSize",
            "true",
        ),
        (
            "my:hatSize('7') cast as xs:integer instance of my:hatSize",
            "false",
        ),
        (
            "my:hatSize('7') cast as xs:integer instance of xs:integer",
            "true",
        ),
        (
            "xs:integer(my:hatSize('7')) instance of my:hatSize",
            "false",
        ),
        ("xs:decimal(my:hatSize('7')) instance of xs:decimal", "true"),
        (
            "xs:decimal(my:hatSize('7')) instance of xs:integer",
            "false",
        ),
        ("xs:short(my:hatSize('7')) instance of xs:short", "true"),
        ("my:hatSize('7') cast as xs:string", "7"),
    ]);
}

/// The results take part in arithmetic, comparisons, `treat as` and the
/// aggregate functions as values of the nearest built-in type, and `instance
/// of` sees the derived type (batch (a) R8: derives-from).
#[test]
fn results_compare_compute_and_match_by_derivation() {
    check_both_versions(&[
        ("my:hatSize('7') + 1", "8"),
        ("(my:hatSize('7') + 1) instance of my:hatSize", "false"),
        ("(my:hatSize('7') + 1) instance of xs:integer", "true"),
        ("my:hatSize('7') eq 7", "true"),
        ("my:hatSize('7') lt my:smallHat('8')", "true"),
        ("my:hatSize('7') = (1, 7)", "true"),
        ("max((my:hatSize('7'), my:smallHat('3')))", "7"),
        ("my:hatSize('7') treat as my:hatSize", "7"),
        ("my:smallHat('7') treat as my:hatSize", "7"),
        ("my:hatSize('7') treat as my:smallHat", "error:XPDY0050"),
        (
            "my:y2k('2001-05-06') + xs:dayTimeDuration('P1D')",
            "2001-05-07",
        ),
        (
            "(my:y2k('2001-05-06') + xs:dayTimeDuration('P1D')) instance of my:y2k",
            "false",
        ),
        ("year-from-date(my:y2k('2001-05-06'))", "2001"),
        (
            "count(distinct-values((my:hatSize('7'), 7, my:smallHat('7'))))",
            "1",
        ),
        ("deep-equal(my:hatSize('7'), 7)", "true"),
        ("index-of((1, 7), my:hatSize('7'))", "2"),
        ("string(my:code('ABC'))", "ABC"),
        ("concat(my:code('ABC'), '-', my:hatSize('7'))", "ABC-7"),
        ("my:code('ABC') = 'ABC'", "true"),
        ("my:qn('my:abc') = xs:QName('my:abc')", "true"),
    ]);
}

/// Types derived from `xs:decimal` and `xs:double`, and §17.4's "the pattern
/// is tested against the canonical lexical representation of the value, as
/// defined for the source type" next to §17.5's "If SV is an instance of
/// xs:string or xs:untypedAtomic, check its value against the pattern facet of
/// TT": `'1.50'` matches `\d+\.\d{2}`, the decimal `1.50` (canonically `1.5`)
/// does not.
#[test]
fn decimal_and_double_derived_types() {
    check_both_versions(&[
        ("my:price('12.5')", "12.5"),
        ("my:price('12.345')", "error:FORG0001"),
        ("my:price('-1')", "error:FORG0001"),
        ("my:price(12)", "12"),
        ("my:price(12) instance of xs:decimal", "true"),
        ("my:price(12) instance of xs:integer", "false"),
        ("my:price(xs:double('2.5'))", "2.5"),
        ("my:price(xs:double('NaN'))", "error:FOCA0002"),
        ("'1.50' cast as my:twoPlaces", "1.5"),
        ("1.50 cast as my:twoPlaces", "error:FORG0001"),
        ("'1.50' castable as my:twoPlaces", "true"),
        ("1.50 castable as my:twoPlaces", "false"),
        ("my:percent('1e2')", "100"),
        ("my:percent(101)", "error:FORG0001"),
        ("my:percent(50) instance of xs:double", "true"),
        ("my:percent(50) instance of my:percent", "true"),
    ]);
}

/// Types derived from `xs:string`: pattern, length, maxLength, enumeration,
/// the target's whiteSpace facet (§3.10.2: "applying the whitespace
/// normalization rules for the target type"), and casts across primitives in
/// both directions.
#[test]
fn string_derived_types() {
    check_both_versions(&[
        ("my:code('ABC')", "ABC"),
        ("my:code('abc')", "error:FORG0001"),
        ("xs:untypedAtomic('XYZ') cast as my:code", "XYZ"),
        ("my:code('ABC') instance of xs:string", "true"),
        ("my:short3('abc')", "abc"),
        ("my:short3('abcd')", "error:FORG0001"),
        ("my:pair('ab')", "ab"),
        ("my:pair('a')", "error:FORG0001"),
        ("my:color('red')", "red"),
        ("my:color('pink')", "error:FORG0001"),
        ("my:color(' red ')", "error:FORG0001"),
        ("my:collapsed('  a   b ')", "a b"),
        ("string-length(my:collapsed('  a   b '))", "3"),
        ("my:collapsed(' abcd ')", "error:FORG0001"),
        ("my:hatSize('7') cast as my:short3", "7"),
        ("my:hatSize('7') cast as my:code", "error:FORG0001"),
        ("my:short3('12') cast as my:hatSize", "12"),
        (
            "my:short3('12') cast as my:hatSize instance of my:hatSize",
            "true",
        ),
        ("xs:anyURI('ABC') cast as my:code", "ABC"),
        (
            "xs:token('ABC') cast as my:code instance of my:code",
            "true",
        ),
        (
            "my:code('ABC') cast as xs:string instance of my:code",
            "false",
        ),
        ("xs:string(my:code('ABC')) instance of xs:string", "true"),
    ]);
}

/// A type derived from `xs:date`: the date / dateTime casts of §17.1.
#[test]
fn date_derived_type() {
    check_both_versions(&[
        ("my:y2k('2001-05-06')", "2001-05-06"),
        ("my:y2k('1999-12-31')", "error:FORG0001"),
        ("my:y2k(xs:dateTime('2001-05-06T10:00:00'))", "2001-05-06"),
        ("my:y2k(5)", "error:XPTY0004"),
        ("my:y2k('2001-05-06') instance of xs:date", "true"),
        ("my:y2k('2001-05-06') instance of my:y2k", "true"),
        (
            "my:y2k('2001-05-06') cast as xs:dateTime",
            "2001-05-06T00:00:00",
        ),
    ]);
}

/// A decimal below the smallest magnitude this engine holds (10^-28) casts to
/// `xs:decimal` as 0 (F&O §6.2: "On underflow, 0.0 must be returned"; see
/// `doc/IMPLEMENTATION_LIMITS.md`). The value a cast returns must satisfy the
/// target's facets (F&O §17.4: "If the value does not conform to the facets
/// defined for the target type, then an error is raised \[err:FORG0001\]"), so
/// 0 is no `my:positive` (minExclusive 0) although the input string is, while
/// it is a `my:nonNegative` (minInclusive 0). The patterns stay on the input
/// string (`'1.50' cast as my:twoPlaces`).
#[test]
fn a_decimal_that_underflows_must_still_satisfy_the_target_facets() {
    let tiny = "'0.000000000000000000000000000001'";
    let cases = [
        (format!("xs:decimal({tiny})"), "0"),
        (format!("my:positive({tiny})"), "error:FORG0001"),
        (
            format!("my:positive({tiny}) instance of my:positive"),
            "error:FORG0001",
        ),
        (format!("my:positive({tiny}) gt 0"), "error:FORG0001"),
        (format!("{tiny} castable as my:positive"), "false"),
        (
            format!("xs:untypedAtomic({tiny}) cast as my:positive"),
            "error:FORG0001",
        ),
        (
            "my:positive(' 0.000000000000000000000000000001 ')".to_string(),
            "error:FORG0001",
        ),
        ("my:positive('0.5')".to_string(), "0.5"),
        (format!("my:nonNegative({tiny})"), "0"),
        (
            format!("my:nonNegative({tiny}) instance of my:nonNegative"),
            "true",
        ),
        (format!("{tiny} castable as my:nonNegative"), "true"),
        // The input string is an enumerated value; the 0 it casts to is not.
        (format!("my:tinyEnum({tiny})"), "error:FORG0001"),
        (format!("{tiny} castable as my:tinyEnum"), "false"),
        ("my:tinyEnum('1')".to_string(), "1"),
        (
            "my:tinyEnum('1.0') instance of my:tinyEnum".to_string(),
            "true",
        ),
        ("'1.50' cast as my:twoPlaces".to_string(), "1.5"),
    ];
    let cases: Vec<(&str, &str)> = cases.iter().map(|(e, r)| (e.as_str(), *r)).collect();
    check_both_versions(&cases);
}

/// Types derived from `xs:QName` and `xs:NOTATION`. F&O §5.3: "There is no
/// constructor function for xs:NOTATION. Constructors are defined, however,
/// for xs:QName, for types derived from xs:QName, and for types derived from
/// xs:NOTATION"; §17.1: "casting from one subtype of xs:NOTATION to another
/// subtype of xs:NOTATION is permitted". The string-literal rule of §3.10.2 is
/// not applied here; see the parity test below.
#[test]
fn qname_and_notation_derived_types() {
    check_both_versions(&[
        ("my:qn('my:abc')", "my:abc"),
        ("my:qn('my:abc') instance of my:qn", "true"),
        ("my:qn('my:abc') instance of xs:QName", "true"),
        ("namespace-uri-from-QName(my:qn('my:abc'))", MY_NS),
        ("my:qn('my:abc') eq xs:QName('my:abc')", "true"),
        ("my:myqn('my:abc')", "my:abc"),
        ("my:myqn('xs:abc')", "error:FORG0001"),
        ("xs:QName('my:abc') cast as my:qn instance of my:qn", "true"),
        ("xs:QName('xs:abc') cast as my:myqn", "error:FORG0001"),
        (
            "my:qn('my:abc') cast as xs:QName instance of my:qn",
            "false",
        ),
        ("my:picture('my:png')", "my:png"),
        ("my:picture('my:png') instance of my:picture", "true"),
        ("my:picture('my:png') instance of xs:NOTATION", "true"),
        ("my:picture('my:bmp')", "error:FORG0001"),
        ("'my:gif' cast as my:picture", "my:gif"),
        ("'my:gif' castable as my:picture", "true"),
        ("'my:bmp' castable as my:picture", "false"),
        (
            "my:picture('my:png') cast as my:pngOnly instance of my:pngOnly",
            "true",
        ),
        ("my:picture('my:gif') cast as my:pngOnly", "error:FORG0001"),
        (
            "my:pngOnly('my:png') cast as my:picture instance of my:picture",
            "true",
        ),
        (
            "my:pngOnly('my:png') cast as my:picture instance of my:pngOnly",
            "false",
        ),
        ("'my:png' cast as xs:NOTATION", "error:XPST0080"),
        ("'my:png' castable as xs:NOTATION", "error:XPST0080"),
        ("'a' cast as xs:anyAtomicType", "error:XPST0080"),
        ("xs:NOTATION('my:png')", "error:XPST0017"),
    ]);
}

/// A QName-derived user type behaves exactly as `xs:QName` does today, for
/// every kind of input: the same value or the same error, the same
/// `castable` answer (the string-literal rule of §3.10.2 is a separate item).
#[test]
fn qname_derived_type_follows_xs_qname() {
    let set = compile_set(XsdVersion::V1_1, SCHEMA);
    let ctx = context(&set);
    let mut failures = Vec::new();
    for input in [
        "'my:abc'",
        "'abc'",
        "' my:abc '",
        "'nope:abc'",
        "'1abc'",
        "''",
        "42",
        "xs:untypedAtomic('my:abc')",
        "xs:QName('my:abc')",
        "my:picture('my:png')",
        "xs:date('2000-01-01')",
    ] {
        for (builtin, user) in [
            (format!("xs:QName({input})"), format!("my:qn({input})")),
            (
                format!("{input} castable as xs:QName"),
                format!("{input} castable as my:qn"),
            ),
        ] {
            let (b, u) = (eval(&ctx, &builtin), eval(&ctx, &user));
            if b != u {
                failures.push(format!("{builtin} => {b}   but   {user} => {u}"));
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// F&O §17.4.1: casting to `xs:ENTITY` or a type derived from it makes no
/// check against the unparsed entities of any document.
#[test]
fn entity_derived_type_makes_no_unparsed_entity_check() {
    check_both_versions(&[
        ("my:ent('foo')", "foo"),
        ("my:ent('foo') instance of xs:ENTITY", "true"),
        ("my:ent('foo') instance of my:ent", "true"),
        ("my:ent('1foo')", "error:FORG0001"),
        ("my:ent('a b')", "error:FORG0001"),
    ]);
}

/// Lists, unions, restrictions of a list and complex types are not atomic
/// types: no constructor (XPST0017) and no cast target (XPST0051); neither is
/// a name the schema set does not define. A constructor takes one argument.
#[test]
fn non_atomic_and_unknown_types_are_neither_constructors_nor_cast_targets() {
    check_both_versions(&[
        ("'1 2' cast as my:ints", "error:XPST0051"),
        ("'1 2' castable as my:ints", "error:XPST0051"),
        ("my:ints('1 2')", "error:XPST0017"),
        ("'1' cast as my:shortInts", "error:XPST0051"),
        ("my:shortInts('1')", "error:XPST0017"),
        ("'1' cast as my:intOrDate", "error:XPST0051"),
        ("my:intOrDate('1')", "error:XPST0017"),
        ("'1' cast as my:ct", "error:XPST0051"),
        ("my:ct('1')", "error:XPST0017"),
        ("'1' cast as my:nope", "error:XPST0051"),
        ("my:nope('1')", "error:XPST0017"),
        ("my:hatSize('1', '2')", "error:XPST0017"),
        ("my:hatSize()", "error:XPST0017"),
    ]);
}

/// Without a schema set the user types are not in the static context at all.
#[test]
fn without_a_schema_set_user_types_are_not_in_scope() {
    let set = compile_set(XsdVersion::V1_1, SCHEMA);
    let ctx = XPathContext::new(&set.name_table).with_namespaces(namespaces(&set));
    assert_eq!(eval(&ctx, "my:hatSize('7')"), "error:XPST0017");
    assert_eq!(eval(&ctx, "'7' cast as my:hatSize"), "error:XPST0051");
    assert_eq!(eval(&ctx, "'7' castable as my:hatSize"), "error:XPST0051");
}

/// F&O §5.4: a type in no namespace is reached by `cast as` when the default
/// element/type namespace is none, and by its constructor only when the
/// default function namespace is none ("declare default function namespace
/// ""; apple(17)").
#[test]
fn a_type_in_no_namespace() {
    for version in [XsdVersion::V1_0, XsdVersion::V1_1] {
        let set = compile_set(version, NO_NAMESPACE_SCHEMA);
        let ctx = context(&set);
        assert_eq!(eval(&ctx, "17 cast as apple"), "17", "{version:?}");
        assert_eq!(eval(&ctx, "17 cast as apple instance of apple"), "true");
        assert_eq!(eval(&ctx, "170 cast as apple"), "error:FORG0001");
        assert_eq!(eval(&ctx, "170 castable as apple"), "false");
        // The default function namespace is fn: — there is no fn:apple.
        assert_eq!(eval(&ctx, "apple(17)"), "error:XPST0017");

        let no_fn_ns = context(&set).with_default_function_ns("");
        assert_eq!(eval(&no_fn_ns, "apple(17)"), "17", "{version:?}");
        assert_eq!(eval(&no_fn_ns, "apple(17) instance of apple"), "true");
        assert_eq!(eval(&no_fn_ns, "apple(170)"), "error:FORG0001");

        // With a default element/type namespace, `apple` is not that type.
        let xs_default = context(&set).with_default_element_ns(set.name_table.add(XS_NAMESPACE));
        assert_eq!(eval(&xs_default, "17 cast as apple"), "error:XPST0051");
    }
}

/// An unprefixed constructor name is expanded with the default *function*
/// namespace (§3.10.4: the constructor's name "is the same as the name of its
/// target type (including namespace)"), not the default element/type
/// namespace, also for the built-in types.
#[test]
fn unprefixed_constructor_names_use_the_default_function_namespace() {
    let set = compile_set(XsdVersion::V1_1, SCHEMA);
    let my_fns = context(&set).with_default_function_ns(MY_NS);
    assert_eq!(eval(&my_fns, "hatSize('7') instance of my:hatSize"), "true");
    assert_eq!(eval(&my_fns, "hatSize('70')"), "error:FORG0001");

    let xs_fns = context(&set).with_default_function_ns(XS_NAMESPACE);
    assert_eq!(eval(&xs_fns, "integer('7') instance of xs:integer"), "true");
}

/// A function the host registers under the name and arity of a constructor
/// takes precedence over it, as a registered `fn:error` does over the
/// built-in; another arity leaves the constructor in place.
#[test]
fn a_host_registered_function_takes_precedence_over_a_constructor() {
    let set = compile_set(XsdVersion::V1_1, SCHEMA);
    let mut functions: FunctionSet<RoXmlNavigator<'static>> = FunctionSet::with_builtins();
    functions.register(
        DynamicFunctionSignature::new(MY_NS, "hatSize", vec![any()], string()),
        |_ctx, _args| Ok(XPathValue::string("host")),
    );
    functions.register(
        DynamicFunctionSignature::new(MY_NS, "smallHat", vec![any(), any()], string()),
        |_ctx, _args| Ok(XPathValue::string("host2")),
    );
    // A built-in type's constructor is not overridden by a registration.
    functions.register(
        DynamicFunctionSignature::new(XS_NAMESPACE, "integer", vec![any()], string()),
        |_ctx, _args| Ok(XPathValue::string("host-integer")),
    );
    let ctx = context(&set).with_function_catalog(&functions);
    let run = |expr: &str| {
        let compiled = match XPathExpr::compile(expr, &ctx) {
            Ok(c) => c,
            Err(e) => return format!("error:{}", e.error_code().unwrap_or("?")),
        };
        render(
            compiled
                .evaluator(&ctx)
                .run_with::<RoXmlNavigator<'static>, _>(|eval| {
                    eval.context().set_function_evaluator(Some(&functions));
                }),
        )
    };
    assert_eq!(run("my:hatSize('70')"), "host");
    assert_eq!(run("my:smallHat('7', '8')"), "host2");
    assert_eq!(run("my:smallHat('7') instance of my:smallHat"), "true");
    assert_eq!(run("xs:integer('7')"), "7");
    assert_eq!(run("xs:integer('7') instance of xs:integer"), "true");
    // `cast as` is not a function call: the registration does not touch it.
    assert_eq!(run("'7' cast as my:hatSize instance of my:hatSize"), "true");
}

/// The target type's lexical space and facets are those of its schema set's
/// XSD version (`validate_simple_type`): `+INF` is a double only in XSD 1.1
/// (Datatypes 1.1 §3.3.5; XSD 1.0 §3.2.5 lists `INF` and `-INF`), and an
/// assertion facet is an XSD 1.1 facet.
#[test]
fn the_target_types_xsd_version_decides_its_lexical_space() {
    let v10 = compile_set(XsdVersion::V1_0, SCHEMA);
    let v11 = compile_set(XsdVersion::V1_1, SCHEMA);
    assert_eq!(
        eval(&context(&v10), "'+INF' cast as my:dbl"),
        "error:FORG0001"
    );
    assert_eq!(eval(&context(&v11), "'+INF' cast as my:dbl"), "INF");
    // A double value is not a string: its `xs:string` cast `INF` is checked.
    assert_eq!(
        eval(&context(&v10), "xs:double('+INF') cast as my:dbl"),
        "INF"
    );
    // Year 0000 is a date only in XSD 1.1 (XSD 1.0 §3.2.7.2 has no year 0000),
    // whether the source is a string or a date value, whose `xs:string` cast
    // is the same form; the built-in `xs:date` cast follows XSD 1.1.
    assert_eq!(eval(&context(&v10), "xs:date('0000-01-01')"), "0000-01-01");
    for expr in [
        "'0000-01-01' cast as my:anyDate",
        "xs:date('0000-01-01') cast as my:anyDate",
    ] {
        assert_eq!(eval(&context(&v10), expr), "error:FORG0001", "{expr}");
        assert_eq!(eval(&context(&v11), expr), "0000-01-01", "{expr}");
    }

    let facets = compile_set(XsdVersion::V1_1, ASSERTION_FACET_SCHEMA);
    let ctx = context(&facets);
    assert_eq!(eval(&ctx, "my:even('4')"), "4");
    assert_eq!(eval(&ctx, "my:even(4) instance of my:even"), "true");
    assert_eq!(eval(&ctx, "my:even('3')"), "error:FORG0001");
    assert_eq!(eval(&ctx, "3 castable as my:even"), "false");
}

/// F&O §17.1, the casting table: `xs:QName` → `xs:NOTATION` and `xs:NOTATION`
/// → `xs:QName` are "N" — "a type error is raised \[err:XPTY0004\]" — and so
/// are the casts between types derived from them (§17.5 assumes only that
/// "casting to xs:NOTATION succeeds", lifting XPST0080, not the table).
#[test]
fn qname_and_notation_types_do_not_cast_into_each_other() {
    check_both_versions(&[
        ("xs:QName('my:png') cast as my:picture", "error:XPTY0004"),
        ("xs:QName('my:png') castable as my:picture", "false"),
        ("my:qn('my:png') cast as my:picture", "error:XPTY0004"),
        ("my:picture('my:png') cast as my:qn", "error:XPTY0004"),
        ("my:picture('my:png') castable as my:qn", "false"),
        ("my:picture('my:png') cast as xs:QName", "error:XPTY0004"),
        ("xs:QName(my:picture('my:png'))", "error:XPTY0004"),
        // Within each family, and to strings, the casts stand.
        ("my:picture('my:png') cast as my:pngOnly", "my:png"),
        ("my:qn('my:png') cast as my:myqn", "my:png"),
        ("my:picture('my:png') cast as xs:string", "my:png"),
        ("xs:untypedAtomic(my:picture('my:png'))", "my:png"),
    ]);
}

/// F&O §17.1.3.3 (to `xs:decimal`) and §17.1.3.4 (to `xs:integer`): "If SV is
/// one of the special xs:float or xs:double values NaN, INF, or -INF, an error
/// is raised \[err:FOCA0002\]" — for the built-in types, the types derived
/// from them, and the user types derived from those. A string source is a
/// lexical mismatch (FORG0001).
#[test]
fn special_floating_point_values_do_not_cast_to_decimal_or_integer() {
    check_both_versions(&[
        ("xs:double('INF') cast as xs:integer", "error:FOCA0002"),
        ("xs:double('-INF') cast as xs:decimal", "error:FOCA0002"),
        ("xs:double('NaN') cast as xs:decimal", "error:FOCA0002"),
        ("xs:float('NaN') cast as xs:integer", "error:FOCA0002"),
        ("xs:float('-INF') cast as xs:short", "error:FOCA0002"),
        ("xs:integer(xs:double('INF'))", "error:FOCA0002"),
        ("xs:double('INF') castable as xs:integer", "false"),
        ("my:hatSize(xs:double('INF'))", "error:FOCA0002"),
        ("xs:double('NaN') cast as my:hatSize", "error:FOCA0002"),
        ("xs:float('INF') cast as my:price", "error:FOCA0002"),
        ("xs:double('INF') castable as my:hatSize", "false"),
        ("'INF' cast as xs:integer", "error:FORG0001"),
        ("'NaN' cast as my:hatSize", "error:FORG0001"),
    ]);
}

/// F&O §6.4 (`ceiling`, `floor`, `round`, `round-half-to-even`): "If the type
/// of $arg is a type derived from one of the numeric types, the result is an
/// instance of the base numeric type" — not of the derived type, built-in or
/// user-defined.
#[test]
fn rounding_functions_return_the_base_numeric_type() {
    let mut cases = Vec::new();
    for f in ["round", "floor", "ceiling", "round-half-to-even"] {
        cases.push((format!("{f}(my:hatSize('7'))"), "7"));
        cases.push((
            format!("{f}(my:hatSize('7')) instance of my:hatSize"),
            "false",
        ));
        cases.push((
            format!("{f}(my:hatSize('7')) instance of xs:integer"),
            "true",
        ));
        cases.push((format!("{f}(xs:short('7')) instance of xs:short"), "false"));
        cases.push((format!("{f}(xs:short('7')) instance of xs:integer"), "true"));
        cases.push((
            format!("{f}(my:price('2.5')) instance of my:price"),
            "false",
        ));
        cases.push((
            format!("{f}(my:percent(50)) instance of my:percent"),
            "false",
        ));
    }
    cases.push(("round-half-to-even(my:hatSize('15'), -1)".to_string(), "20"));
    let cases: Vec<(&str, &str)> = cases.iter().map(|(e, r)| (e.as_str(), *r)).collect();
    check_both_versions(&cases);
}

/// A FORG0001 names the target type, also when the value already fails the
/// target's nearest built-in type.
#[test]
fn a_failed_cast_names_the_target_type() {
    let set = compile_set(XsdVersion::V1_1, SCHEMA);
    let ctx = context(&set);
    for expr in [
        "'300' cast as my:ub",
        "my:ub('-1')",
        "my:hatSize('x')",
        "my:hatSize('70')",
        "'abc' cast as my:code",
    ] {
        let Err(error) = XPathExpr::compile(expr, &ctx)
            .expect("compiles")
            .evaluator(&ctx)
            .run::<RoXmlNavigator<'static>>()
        else {
            panic!("{expr}: no error");
        };
        let message = error.to_string();
        assert!(
            message.starts_with("[FORG0001]") && message.contains("'my:"),
            "{expr}: {message}"
        );
    }
}

/// Values atomized from nodes typed with a user type carry that type, and
/// cast like any other value of it.
#[test]
fn typed_node_values_cast_by_their_annotation() {
    use crate::document::typed_builder::build_typed_document;
    use crate::document::BufferDocumentOptions;
    use bumpalo::Bump;

    for version in [XsdVersion::V1_0, XsdVersion::V1_1] {
        let set = compile_set(version, SCHEMA);
        let arena = Bump::new();
        let doc = build_typed_document(
            r#"<my:doc xmlns:my="urn:c37" h="12"><s>4</s><c>ABC</c></my:doc>"#.as_bytes(),
            &arena,
            &set,
            BufferDocumentOptions::default(),
        )
        .expect("typed document");
        let ctx = context(&set);
        let run = |expr: &str| match XPathExpr::compile(expr, &ctx) {
            Err(e) => format!("error:{}", e.error_code().unwrap_or("?")),
            Ok(c) => match c.evaluator(&ctx).run_with_node(doc.create_navigator()) {
                Err(e) => format!("error:{}", e.error_code().unwrap_or("?")),
                Ok(v) => v
                    .first()
                    .map(|item| match item {
                        XmlItem::Atomic(a) => a.to_string_value(),
                        XmlItem::Node(n) => crate::xpath::DomNavigator::value(n),
                    })
                    .unwrap_or_else(|| "()".to_string()),
            },
        };
        for (expr, expected) in [
            ("data(/my:doc/s) instance of my:smallHat", "true"),
            (
                "/my:doc/s cast as my:hatSize instance of my:hatSize",
                "true",
            ),
            (
                "/my:doc/s cast as my:hatSize instance of my:smallHat",
                "false",
            ),
            (
                "/my:doc/s cast as my:evenHat instance of my:evenHat",
                "true",
            ),
            ("/my:doc/@h cast as my:smallHat", "error:FORG0001"),
            ("/my:doc/@h castable as my:evenHat", "true"),
            ("/my:doc/c cast as my:short3", "ABC"),
            ("my:hatSize(/my:doc/s) + /my:doc/@h", "16"),
        ] {
            assert_eq!(run(expr), expected, "{version:?}: {expr}");
        }
    }
}

// ---------------------------------------------------------------------------
// XSD 1.1 assertions and type alternatives
// ---------------------------------------------------------------------------
//
// Both compile their XPath with the schema set attached. Structures §3.13.6.2
// clause 2.2.5 gives them only the built-in definitions ("The in-scope schema
// definitions are those components that are present in every schema by
// definition"). Assertions therefore keep the casts and constructors of user
// types out (as before these were added): `castable as my:t` is false, `cast
// as my:t` raises XPST0051 when evaluated, `my:t(..)` is XPST0017 — each a
// `cvc-assertion` error rather than the spec's schema error. Type alternatives
// keep them in, so the schema-time check of `schema::cta` rejects a call of a
// user type's constructor as it rejects `cast as my:t` — the spec's verdict.
// (`instance of my:t` stays evaluated in both, batch (a) R8.)

const ASSERT_SCHEMA: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           targetNamespace="urn:c37" xmlns:my="urn:c37" elementFormDefault="qualified">
  <xs:simpleType name="hatSize">
    <xs:restriction base="xs:integer">
      <xs:minInclusive value="1"/><xs:maxInclusive value="20"/>
    </xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="hatText">
    <xs:restriction base="xs:string">
      <xs:assertion test="$value castable as my:hatSize"/>
    </xs:restriction>
  </xs:simpleType>
  <xs:element name="castable">
    <xs:complexType>
      <xs:attribute name="a" type="xs:string"/>
      <xs:assert test="@a castable as my:hatSize"/>
    </xs:complexType>
  </xs:element>
  <xs:element name="notCastable">
    <xs:complexType>
      <xs:attribute name="a" type="xs:string"/>
      <xs:assert test="not(@a castable as my:hatSize)"/>
    </xs:complexType>
  </xs:element>
  <xs:element name="cast">
    <xs:complexType>
      <xs:attribute name="a" type="xs:string"/>
      <xs:assert test="(@a cast as my:hatSize) le 10"/>
    </xs:complexType>
  </xs:element>
  <xs:element name="ctor">
    <xs:complexType>
      <xs:attribute name="a" type="xs:string"/>
      <xs:assert test="my:hatSize(@a) le 10"/>
    </xs:complexType>
  </xs:element>
  <xs:element name="builtin">
    <xs:complexType>
      <xs:attribute name="a" type="xs:string"/>
      <xs:assert test="xs:integer(@a) le 10 and @a castable as xs:byte"/>
    </xs:complexType>
  </xs:element>
  <xs:element name="typed">
    <xs:complexType>
      <xs:attribute name="h" type="my:hatSize"/>
      <xs:assert test="data(@h) instance of my:hatSize"/>
    </xs:complexType>
  </xs:element>
  <xs:element name="facet" type="my:hatText"/>
</xs:schema>"#;

fn assertion_errors(set: &SchemaSet, xml: &str) -> Vec<String> {
    use crate::validation::{
        drive_quick_xml, CollectingValidationSink, SchemaValidator, ValidationFlags,
    };
    let validator = SchemaValidator::new_fragment_buffer(set, ValidationFlags::default());
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    {
        let mut run = validator.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        drive_quick_xml(xml.as_bytes(), &mut run, set).expect("drive completes");
    }
    let mut codes: Vec<String> = errors.iter().map(|e| e.constraint.to_string()).collect();
    codes.dedup();
    codes
}

/// An assertion does not see the casts, constructors or other names of user
/// types: its static context has only the built-in components (Structures 1.1
/// §3.13.6.2 clause 2.2.5), so naming one is a static error and the schema is
/// invalid (`as-props-correct`). Built-in casts and constructors are
/// evaluated.
#[test]
fn assertions_keep_user_type_casts_and_constructors_out() {
    // Every one of these assertions names `my:hatSize`.
    for assertion in ["castable", "notCastable", "cast", "ctor", "typed", "facet"] {
        let mut xsd = ASSERT_SCHEMA.to_string();
        for other in ["castable", "notCastable", "cast", "ctor", "typed"] {
            if other != assertion {
                let start = xsd
                    .find(&format!(r#"<xs:element name="{other}">"#))
                    .unwrap();
                let end =
                    start + xsd[start..].find("</xs:element>").unwrap() + "</xs:element>".len();
                xsd.replace_range(start..end, "");
            }
        }
        if assertion != "facet" {
            xsd = xsd.replace(
                r#"<xs:assertion test="$value castable as my:hatSize"/>"#,
                "",
            );
        }
        let err = SchemaSetBuilder::with_version(XsdVersion::V1_1)
            .add_source(&xsd, "file:///c37.xsd")
            .and_then(|b| b.compile())
            .err()
            .unwrap_or_else(|| panic!("{assertion}: schema accepted"));
        assert!(
            err.to_string().contains("as-props-correct"),
            "{assertion}: {err}"
        );
    }
    // Built-in casts and constructors are evaluated.
    let builtin_only = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           targetNamespace="urn:c37" xmlns:my="urn:c37" elementFormDefault="qualified">
  <xs:element name="builtin">
    <xs:complexType>
      <xs:attribute name="a" type="xs:string"/>
      <xs:assert test="xs:integer(@a) le 10 and @a castable as xs:byte"/>
    </xs:complexType>
  </xs:element>
</xs:schema>"#;
    let set = compile_set(XsdVersion::V1_1, builtin_only);
    let ns = r#"xmlns:my="urn:c37""#;
    assert!(assertion_errors(&set, &format!(r#"<my:builtin {ns} a="7"/>"#)).is_empty());
    assert_eq!(
        assertion_errors(&set, &format!(r#"<my:builtin {ns} a="12"/>"#)),
        ["cvc-assertion"]
    );
}

/// A type alternative's test that calls a user type's constructor is a type
/// expression naming a user-defined type, which the schema-time check of
/// `schema::cta` rejects as it rejects `cast as my:t` (before the user
/// constructors existed the call was XPST0017 outside the `xs` namespace,
/// which that check lets pass, and the schema was accepted).
#[test]
fn a_type_alternative_calling_a_user_constructor_is_a_schema_error() {
    let alternatives = |test: &str| {
        format!(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                   targetNamespace="urn:c37" xmlns:my="urn:c37">
  <xs:simpleType name="hatSize">
    <xs:restriction base="xs:integer"><xs:maxInclusive value="20"/></xs:restriction>
  </xs:simpleType>
  <xs:complexType name="base"><xs:attribute name="a" type="xs:string"/></xs:complexType>
  <xs:complexType name="small">
    <xs:complexContent><xs:restriction base="my:base">
      <xs:attribute name="a" type="xs:string"/>
    </xs:restriction></xs:complexContent>
  </xs:complexType>
  <xs:element name="e" type="my:base">
    <xs:alternative test="{test}" type="my:small"/>
  </xs:element>
</xs:schema>"#
        )
    };
    let compiles = |test: &str| {
        SchemaSetBuilder::with_version(XsdVersion::V1_1)
            .add_source(&alternatives(test), "file:///c37-cta.xsd")
            .and_then(|b| b.compile())
            .is_ok()
    };
    assert!(compiles("@a = '1'"));
    assert!(compiles("xs:integer(@a) le 10"));
    assert!(!compiles("(@a cast as my:hatSize) le 10"));
    assert!(!compiles("@a castable as my:hatSize"));
    assert!(!compiles("my:hatSize(@a) le 10"));
}

/// An assertion facet that casts to a user type — even the type it constrains
/// — cannot reach that type: the cast is not in its static context (only the
/// built-in components are), so the schema is invalid. (The engine also
/// guards the facet check against re-entry, see `schema_cast::check_facets`;
/// with assertions kept out of user types nothing reaches the guard.)
#[test]
fn an_assertion_facet_cannot_cast_to_its_own_type() {
    let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           targetNamespace="urn:c37" xmlns:my="urn:c37" elementFormDefault="qualified">
  <xs:simpleType name="selfish">
    <xs:restriction base="xs:integer">
      <xs:assertion test="string($value) castable as my:selfish"/>
    </xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="identity">
    <xs:restriction base="xs:integer">
      <xs:assertion test="$value castable as my:identity"/>
    </xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="ping">
    <xs:restriction base="xs:integer"><xs:assertion test="$value castable as my:pong"/></xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="pong">
    <xs:restriction base="xs:integer"><xs:assertion test="$value castable as my:ping"/></xs:restriction>
  </xs:simpleType>
  <xs:element name="e" type="my:selfish"/>
</xs:schema>"#;
    let err = SchemaSetBuilder::with_version(XsdVersion::V1_1)
        .add_source(xsd, "file:///c37.xsd")
        .and_then(|b| b.compile())
        .err()
        .expect("schema refused");
    assert!(err.to_string().contains("as-props-correct"), "{err}");
}

/// A tree bound twice binds the same: a node made from an unprefixed
/// constructor call keeps the type its name expanded to through the default
/// function namespace, although `cast as` would expand the same unprefixed
/// name through the default element/type namespace.
#[test]
fn binding_a_constructor_node_again_keeps_its_type() {
    use crate::xpath::bind::bind_node;
    use crate::xpath::context::{DynamicContext, NameBinder};
    use crate::xpath::eval::eval_node;
    use crate::xpath::parser::parse;

    let set = compile_set(XsdVersion::V1_1, SCHEMA);
    let ctx = context(&set)
        .with_default_function_ns(MY_NS)
        .with_default_element_ns(set.name_table.add(XS_NAMESPACE));
    let mut parsed = parse("hatSize('7') instance of my:hatSize").unwrap();
    for _ in 0..2 {
        let mut binder = NameBinder::new();
        bind_node(&mut parsed.arena, parsed.root, &ctx, &mut binder).unwrap();
        let mut dynamic: DynamicContext<'_, RoXmlNavigator<'static>> =
            DynamicContext::new(&ctx, binder.len());
        let value = eval_node(&parsed.arena, parsed.root, &mut dynamic).unwrap();
        assert_eq!(render(Ok(value)), "true");
    }
}

/// Facets a cast's result must satisfy (F&O §17.4: "If the value does not
/// conform to the facets defined for the target type, then an error is raised
/// \[err:FORG0001\]") and that the validator used to let through: the
/// `enumeration` a restriction keeps from its base type (Structures 1.1
/// §3.16.6.4 clause 3, "The {facets} of R are the {facets} of B ·overlaid·
/// with S"), `NaN` against a bound (Datatypes 1.1 §3.3.5.1: "If any other
/// value is used for a bounding facet, NaN will be excluded from the resulting
/// restricted ·value space·"), and a date without a time zone offset against
/// an enumerated date with one, or a bound with one, with which it is
/// incomparable (Datatypes 1.1 §D.2.1).
#[test]
fn inherited_enumerations_nan_and_time_zones_constrain_casts() {
    const XSD: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           targetNamespace="urn:c37" xmlns:my="urn:c37">
  <xs:simpleType name="color">
    <xs:restriction base="xs:string">
      <xs:enumeration value="red"/><xs:enumeration value="green"/>
    </xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="plainColor"><xs:restriction base="my:color"/></xs:simpleType>
  <xs:simpleType name="shortColor">
    <xs:restriction base="my:color"><xs:maxLength value="4"/></xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="sizes">
    <xs:restriction base="xs:integer">
      <xs:enumeration value="1"/><xs:enumeration value="5"/>
    </xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="smallSizes">
    <xs:restriction base="my:sizes"><xs:maxInclusive value="1"/></xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="percent">
    <xs:restriction base="xs:double">
      <xs:minInclusive value="0"/><xs:maxInclusive value="100"/>
    </xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="posFloat">
    <xs:restriction base="xs:float"><xs:minExclusive value="0"/></xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="dZ">
    <xs:restriction base="xs:date"><xs:enumeration value="2000-01-01Z"/></xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="dtMin">
    <xs:restriction base="xs:dateTime"><xs:minInclusive value="2000-01-01T00:00:00Z"/></xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="fromJuneLocal">
    <xs:restriction base="xs:gMonth"><xs:minInclusive value="--06"/></xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="untilJuneNeg14">
    <xs:restriction base="xs:gMonth"><xs:maxInclusive value="--06-14:00"/></xs:restriction>
  </xs:simpleType>
</xs:schema>"#;
    let cases = [
        ("my:plainColor('blue')", "error:FORG0001"),
        ("'blue' castable as my:plainColor", "false"),
        ("my:plainColor('red')", "red"),
        ("my:plainColor('red') instance of my:color", "true"),
        ("my:shortColor('blue')", "error:FORG0001"),
        ("my:smallSizes('0')", "error:FORG0001"),
        ("my:smallSizes(1)", "1"),
        ("xs:double('NaN') cast as my:percent", "error:FORG0001"),
        ("'NaN' castable as my:percent", "false"),
        ("xs:float('NaN') cast as my:posFloat", "error:FORG0001"),
        ("my:percent('50')", "50"),
        ("'2000-01-01' cast as my:dZ", "error:FORG0001"),
        ("xs:date('2000-01-01') cast as my:dZ", "error:FORG0001"),
        ("'2000-01-01Z' cast as my:dZ", "2000-01-01Z"),
        ("xs:date('2000-01-01+00:00') cast as my:dZ", "2000-01-01Z"),
        ("'2000-01-01T00:00:00' castable as my:dtMin", "false"),
        ("'2000-01-01T15:00:00' castable as my:dtMin", "true"),
        (
            "'1999-12-31T23:00:00-05:00' cast as my:dtMin",
            "1999-12-31T23:00:00-05:00",
        ),
        // A gMonth is a day of 1972 (Datatypes 1.1 §E.3.4): July is after
        // June whatever the offsets.
        (
            "xs:gMonth('--07+10:00') cast as my:fromJuneLocal",
            "--07+10:00",
        ),
        ("'--07+10:00' castable as my:fromJuneLocal", "true"),
        ("'--05' castable as my:fromJuneLocal", "false"),
        ("'--07+14:00' castable as my:untilJuneNeg14", "false"),
        ("'--06+14:00' cast as my:untilJuneNeg14", "--06+14:00"),
    ];
    let mut failures = Vec::new();
    for version in [XsdVersion::V1_0, XsdVersion::V1_1] {
        let set = compile_set(version, XSD);
        let ctx = context(&set);
        for (expr, expected) in cases {
            let got = eval(&ctx, expr);
            if got != expected {
                failures.push(format!(
                    "{version:?}: {expr}  =>  {got}  (expected {expected})"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// The assertion facets of the target constrain the value the cast returns as
/// well: a string whose `xs:decimal` cast underflows to 0 (F&O §6.2) is no
/// value of a type asserting `$value ne 0` (Datatypes 1.1 §4.3.13.3
/// Assertions Valid: "A value V is facet-valid with respect to an assertions
/// facet belonging to a simple type T if and only if the {test} property of
/// each Assertion in its {value} evaluates to true"), nor of a restriction
/// inheriting the assertion. A value the cast does not change is checked
/// once, as before.
#[test]
fn an_assertion_facet_holds_for_the_value_a_cast_returns() {
    const XSD: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           targetNamespace="urn:c37" xmlns:my="urn:c37">
  <xs:simpleType name="nonZero">
    <xs:restriction base="xs:decimal"><xs:assertion test="$value ne 0"/></xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="smallNonZero">
    <xs:restriction base="my:nonZero"><xs:maxInclusive value="1"/></xs:restriction>
  </xs:simpleType>
</xs:schema>"#;
    let tiny = "'0.000000000000000000000000000001'";
    let set = compile_set(XsdVersion::V1_1, XSD);
    let ctx = context(&set);
    let cases = [
        (format!("xs:decimal({tiny})"), "0"),
        (format!("my:nonZero({tiny})"), "error:FORG0001"),
        (format!("{tiny} castable as my:nonZero"), "false"),
        (
            format!("xs:untypedAtomic({tiny}) cast as my:nonZero"),
            "error:FORG0001",
        ),
        (format!("my:smallNonZero({tiny})"), "error:FORG0001"),
        ("my:nonZero('0')".to_string(), "error:FORG0001"),
        ("my:nonZero('0.5')".to_string(), "0.5"),
        ("my:smallNonZero('0.5')".to_string(), "0.5"),
    ];
    let mut failures = Vec::new();
    for (expr, expected) in &cases {
        let got = eval(&ctx, expr);
        if got != *expected {
            failures.push(format!("{expr}  =>  {got}  (expected {expected})"));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// A NaN bound through the cast path: XSD 1.0 §3.2.5 gives an inclusive NaN
/// bound the value space {NaN} ("NaN equals itself"), Datatypes 1.1
/// §3.3.5.1 the empty one ("NaN is ·incomparable· with any value in the
/// ·value space· including itself").
#[test]
fn a_nan_bound_follows_the_target_types_xsd_version() {
    const XSD: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           targetNamespace="urn:c37" xmlns:my="urn:c37">
  <xs:simpleType name="nanMin">
    <xs:restriction base="xs:double"><xs:minInclusive value="NaN"/></xs:restriction>
  </xs:simpleType>
</xs:schema>"#;
    let v10 = compile_set(XsdVersion::V1_0, XSD);
    let v11 = compile_set(XsdVersion::V1_1, XSD);
    for (expr, in_10, in_11) in [
        (
            "xs:double('NaN') cast as my:nanMin",
            "NaN",
            "error:FORG0001",
        ),
        ("'NaN' castable as my:nanMin", "true", "false"),
        ("'0' castable as my:nanMin", "false", "false"),
    ] {
        assert_eq!(eval(&context(&v10), expr), in_10, "1.0: {expr}");
        assert_eq!(eval(&context(&v11), expr), in_11, "1.1: {expr}");
    }
}

/// An enumeration value is a value of the base type (Datatypes 1.1
/// §4.3.5.2: "interpreted as instances of the {base type definition}", by
/// "the ·lexical mapping· of the {base type definition}"), so `" red "`
/// enumerated for an `xs:token` type is `red`, the value a cast to it, or to
/// a restriction inheriting the enumeration, accepts; `" red "` of an
/// `xs:string` type keeps its spaces.
#[test]
fn casts_compare_enumeration_values_of_the_base_type() {
    const XSD: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           targetNamespace="urn:c37" xmlns:my="urn:c37">
  <xs:simpleType name="color">
    <xs:restriction base="xs:token">
      <xs:enumeration value=" red "/><xs:enumeration value="blue"/>
    </xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="color2"><xs:restriction base="my:color"/></xs:simpleType>
  <xs:simpleType name="color3">
    <xs:restriction base="my:color"><xs:maxLength value="10"/></xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="padded">
    <xs:restriction base="xs:string"><xs:enumeration value=" red "/></xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="lang">
    <xs:restriction base="xs:language"><xs:enumeration value=" en "/></xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="site">
    <xs:restriction base="xs:anyURI"><xs:enumeration value=" http://example.com/ "/></xs:restriction>
  </xs:simpleType>
</xs:schema>"#;
    let cases = [
        ("'red' cast as my:color", "red"),
        ("' red ' cast as my:color", "red"),
        ("xs:token('red') cast as my:color", "red"),
        ("'red' cast as my:color2", "red"),
        ("my:color2('red')", "red"),
        ("'red' castable as my:color3", "true"),
        ("'green' castable as my:color2", "false"),
        ("my:color2('green')", "error:FORG0001"),
        ("' red ' castable as my:padded", "true"),
        ("'red' castable as my:padded", "false"),
        ("'en' cast as my:lang", "en"),
        (
            "'http://example.com/' cast as my:site",
            "http://example.com/",
        ),
    ];
    let mut failures = Vec::new();
    for version in [XsdVersion::V1_0, XsdVersion::V1_1] {
        let set = compile_set(version, XSD);
        let ctx = context(&set);
        for (expr, expected) in cases {
            let got = eval(&ctx, expr);
            if got != expected {
                failures.push(format!(
                    "{version:?}: {expr}  =>  {got}  (expected {expected})"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// The facets of a type derived from `xs:QName` or `xs:NOTATION` see the value
/// a cast gives — its expanded name (Datatypes §3.3.18: "The ·value space· of
/// QName is the set of tuples {namespace name, local part}"; §3.3.19) — and
/// each enumeration literal is resolved with the bindings of the schema
/// element it is written on (§3.3.18: the lexical mapping "depends on the
/// namespace bindings in scope where the literal occurs"). So `'z:en'` casts
/// to a NOTATION type that enumerates `t:en` when `z` and `t` are bound to the
/// same namespace, and `'t:en'` does not when the static context binds `t`
/// elsewhere; `pattern` still constrains the lexical form, for a value made
/// with the static context's prefixes and for an `xs:QName` value cast with
/// its own.
#[test]
fn casts_to_qname_and_notation_types_compare_expanded_names() {
    const XSD: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           targetNamespace="urn:c33" xmlns:t="urn:c33">
  <xs:notation name="en" public="pub"/>
  <xs:notation name="de" public="pub"/>
  <xs:simpleType name="notat">
    <xs:restriction base="xs:NOTATION"><xs:enumeration value="t:en"/></xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="code">
    <xs:restriction base="xs:QName">
      <xs:enumeration value="t:a"/><xs:enumeration value="t:b"/>
    </xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="tcode">
    <xs:restriction base="t:code"><xs:pattern value="t:.*"/></xs:restriction>
  </xs:simpleType>
</xs:schema>"#;
    let cases = [
        ("z:notat('z:en')", "z:en"),
        ("'z:en' cast as z:notat", "z:en"),
        ("'z:en' castable as z:notat", "true"),
        ("'z:de' castable as z:notat", "false"),
        ("'t:en' castable as z:notat", "false"),
        ("z:code('z:a')", "z:a"),
        ("namespace-uri-from-QName(z:code('z:b'))", "urn:c33"),
        ("'z:c' castable as z:code", "false"),
        ("'t:a' castable as z:code", "false"),
        ("xs:QName('z:a') cast as z:code", "z:a"),
        ("xs:QName('t:a') castable as z:code", "false"),
        ("'z:a' castable as z:tcode", "false"),
        ("xs:QName('z:a') castable as z:tcode", "false"),
    ];
    let mut failures = Vec::new();
    for version in [XsdVersion::V1_0, XsdVersion::V1_1] {
        let set = compile_set(version, XSD);
        let mut namespaces = NamespaceContextSnapshot::default();
        for (prefix, uri) in [("xs", XS_NAMESPACE), ("z", "urn:c33"), ("t", "urn:other")] {
            namespaces
                .bindings
                .push((set.name_table.add(prefix), set.name_table.add(uri)));
        }
        let ctx = XPathContext::new(&set.name_table)
            .with_namespaces(namespaces)
            .with_schema_set(&set);
        for (expr, expected) in cases {
            let got = eval(&ctx, expr);
            if got != expected {
                failures.push(format!(
                    "{version:?}: {expr}  =>  {got}  (expected {expected})"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// A cast checks the target type's facets in the value space instance
/// validation uses (F&O §17.4 / §17.5: the value must be "in the value space"
/// of the target): `-0` is zero for the unsigned types (Datatypes 1.1
/// §3.4.24.1), `P30D` is ·incomparable· with `P1M` (§3.3.6.1), so outside
/// `maxInclusive="P1M"`, and a `g*` pattern constrains the lexical form of a
/// string source (§4.3.4) while a `g*` value is cast through its canonical
/// representation (§17.4, as for every other primitive).
#[test]
fn casts_check_facets_in_the_value_space_of_the_target() {
    const XSD: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:c37">
      <xs:simpleType name="tiny"><xs:restriction base="xs:unsignedByte"><xs:maxInclusive value="5"/></xs:restriction></xs:simpleType>
      <xs:simpleType name="yPlus"><xs:restriction base="xs:gYear"><xs:pattern value="\d{4}\+00:00"/></xs:restriction></xs:simpleType>
      <xs:simpleType name="upTo1M"><xs:restriction base="xs:duration"><xs:maxInclusive value="P1M"/></xs:restriction></xs:simpleType>
      <xs:simpleType name="day2"><xs:restriction base="xs:gDay"><xs:enumeration value="---02-14:00"/></xs:restriction></xs:simpleType>
    </xs:schema>"#;
    let cases = [
        ("my:tiny('-0')", "0"),
        ("'-00' cast as my:tiny", "0"),
        ("'-1' castable as my:tiny", "false"),
        ("xs:unsignedInt('-0')", "0"),
        ("'2000+00:00' castable as my:yPlus", "true"),
        ("'2000Z' castable as my:yPlus", "false"),
        ("xs:gYear('2000+00:00') castable as my:yPlus", "false"),
        ("'P30D' castable as my:upTo1M", "false"),
        ("'P27D' castable as my:upTo1M", "true"),
        ("'---03+10:00' castable as my:day2", "true"),
    ];
    let mut failures = Vec::new();
    for version in [XsdVersion::V1_0, XsdVersion::V1_1] {
        let set = compile_set(version, XSD);
        let ctx = context(&set);
        for (expr, expected) in cases {
            let got = eval(&ctx, expr);
            if got != expected {
                failures.push(format!(
                    "{version:?}: {expr}  =>  {got}  (expected {expected})"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
