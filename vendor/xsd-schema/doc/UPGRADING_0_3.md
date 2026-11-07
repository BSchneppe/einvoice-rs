# Upgrading from 0.2.x to 0.3

0.3.0 changes the public API in places that need a code change, and it changes
validation verdicts where 0.2.x was wrong. This guide has both, in that order:

1. [Code changes](#1-code-changes), each with the 0.2.x code and the 0.3 code;
2. [Verdict changes](#2-verdict-changes): documents and schemas that are
   refused now and were accepted, and the reverse;
3. [Deprecations](#3-deprecations).

The [CHANGELOG](../CHANGELOG.md) lists every change with its specification
reference. In the examples below, a block marked as 0.2.x code is not compiled
(it no longer builds); every other block is compiled and run with the crate.

## 1. Code changes

### 1.1 `#[non_exhaustive]` types

These public types of 0.2.x are `#[non_exhaustive]` in 0.3, so that later
variants and fields are additive:

| Kind | Types |
| --- | --- |
| enums | `xpath::XPathError`, `xpath::functions::FunctionId`, `types::sequence::ItemType` (and its `Element` / `Attribute` variants), `xpath::parser::ParseError`, `SchemaError`, `validation::DriveError`, `validation::DriveWithError`, `document::BufferDocumentError`, `validation::TypeSource` |
| structs | `validation::ValidationError`, `validation::ValidationWarning`, `validation::DefaultAttribute`, `validation::EndOfAttributesView`, `validation::EndElementInfo` |

New types are `#[non_exhaustive]` from the start: `ValidationRoot`,
`ValidationSubject`, `PathStep`, `TypeTest`, `SchemaLimit`, `DirectiveFailure`,
`LocationRequest`, `TypedBuild`, `TypedBuildOptions`, `TypedBuildError`,
`ElementValueConstraint`, `DoctypeView` and the `xml` module's
`WellFormednessRule`, `XmlVersion`, `XmlDeclaration`, `ParseLimit`,
`ParseLimits`.

A `match` on such an enum needs a wildcard arm; a struct can no longer be
built with a struct literal outside the crate (its fields stay readable).

```rust,ignore
// 0.2.x
match err {
    DriveError::Parse(e) => eprintln!("XML error: {e}"),
    DriveError::Utf8(e) => eprintln!("encoding: {e}"),
    DriveError::UnboundPrefix(p) => eprintln!("unbound prefix {p}"),
    DriveError::UnexpectedEof { depth } => eprintln!("truncated at depth {depth}"),
    DriveError::Validation(e) => eprintln!("{e}"),
}
let warning = ValidationWarning { code: "w1", message: "…".into(), location: None };
```

```rust
use xsd_schema::validation::{DriveError, ValidationWarning};

fn report(err: &DriveError) {
    match err {
        DriveError::NotWellFormed(e) => eprintln!("{e}"), // rule, byte position
        DriveError::LimitExceeded(e) => eprintln!("{e}"), // which ParseLimit
        DriveError::Io(e) => eprintln!("I/O: {e}"),
        other => eprintln!("{other}"),                    // the wildcard arm
    }
}

let warning = ValidationWarning::new("w1", "…", None);
# let _ = warning;
```

### 1.2 XPath errors

- **Errors identified by an error QName** — what `fn:error` raises, a host's
  own codes, and the specification codes without a variant of their own — are
  `XPathError::Raised(RaisedErrorInfo)`, not an `Internal` message. Read the
  code with `error_code()` (the specifications' codes) or `raised_error()`
  (any error QName), as before; `Display` is `[CODE] description`.
- `XPDY0050` carries a message (`XPDY0050 { message }`); `XPST0080` is a
  variant of its own (`XPST0080 { type_name }`); `FOCA0002 { qname }` is
  `FOCA0002 { value }` (it is also raised for `NaN` / `INF` cast to
  `xs:decimal` or `xs:integer`); `XPathError::more_than_one_item()` is
  `XPTY0004`.
- `XPathError::LimitExceeded { max, found }` is new (the nesting limit, §1.8).
- A host error with a payload of the host's own type:
  `XPathError::host(namespace, local, description, payload)`, read back with
  `downcast_host_payload::<T>()`. Such a payload is
  `Arc<dyn Error + Send + Sync>`, so **`XPathError` is no longer
  `UnwindSafe` / `RefUnwindSafe`**: wrap it in `std::panic::AssertUnwindSafe`
  to carry it across `catch_unwind`.

```rust,ignore
// 0.2.x
match err {
    XPathError::XPDY0050 => "treat as failed".to_string(),
    XPathError::FOCA0002 { qname } => format!("invalid QName {qname}"),
    XPathError::Internal(message) if message.starts_with("[MY0001]") => "mine".to_string(),
    other => other.to_string(),
}
```

```rust
#[cfg(feature = "xsd11")]
fn describe(err: &xsd_schema::xpath::XPathError) -> String {
    use xsd_schema::xpath::XPathError;
    match err {
        XPathError::XPDY0050 { message } => format!("treat as failed: {message}"),
        XPathError::FOCA0002 { value } => format!("invalid lexical value {value}"),
        XPathError::Raised(info) if info.local_name() == "MY0001" => "mine".to_string(),
        other => other.to_string(),
    }
}
```

### 1.3 `ItemType` and `TypeTest`

`ItemType::Element` and `ItemType::Attribute` are struct variants, built with
`ItemType::element(name, type_test, nillable)` and
`ItemType::attribute(name, type_test)`. Their type is a `TypeTest`: a schema
type key (simple or complex), a predefined `xs` type by its code, or
`xs:untyped`. A host-built test now reads the node's type annotation, complex
types included.

```rust,ignore
// 0.2.x
let test = ItemType::Element(Some(name), Some(simple_type_key));
if let ItemType::Element(name, Some(key)) = &test { /* … */ }
```

```rust
# #[cfg(feature = "xsd11")] {
use xsd_schema::ids::SimpleTypeKey;
use xsd_schema::types::sequence::{ItemType, NameTest, TypeTest};
use xsd_schema::TypeKey;

fn element_test(name: NameTest, key: SimpleTypeKey) -> ItemType {
    ItemType::element(Some(name), Some(TypeTest::Schema(TypeKey::Simple(key))), false)
}

fn its_type(test: &ItemType) -> Option<&TypeTest> {
    match test {
        ItemType::Element { type_test, .. } | ItemType::Attribute { type_test, .. } => {
            type_test.as_ref()
        }
        _ => None,
    }
}
# }
```

`xpath::ast::TypeExprNode` has a private field: build it with
`TypeExprNode::new`, and bind a node again after editing it. The `Display` of
`SequenceType` prints element, attribute, document and schema tests in
SequenceType syntax.

### 1.4 Typed values

- `XmlAtomicValue::QName` and `XmlAtomicValue::Notation` hold a `QNameValue`
  — the expanded name (`name()`) together with its lexical form — instead of a
  `QualifiedName`, so such a value converts to a string without a name table.
  Build one with `QNameValue::new(name, &name_table)`; its equality ignores the
  prefix.
- `XmlValueKind::List::items` is a `Vec<XmlValue>`: each member carries its
  own type code and schema type (for a list of a union, the member type that
  validated it). `item_type` is the declared item type's code
  (`xs:anySimpleType` for a union).
- `XmlValue::to_xpath_value()` takes no argument.

```rust,ignore
// 0.2.x
if let XmlAtomicValue::QName(qname) = &atomic {
    let local = names.resolve(qname.local_name);
}
for member in &items { /* member: &XmlAtomicValue */ }
let xpath_value = value.to_xpath_value::<N>(item_schema_type);
```

```rust
use xsd_schema::types::value::{XmlAtomicValue, XmlValue, XmlValueKind};

fn local_names(value: &XmlValue) -> Vec<String> {
    let atomics: Vec<&XmlAtomicValue> = match &value.value {
        XmlValueKind::Atomic(a) => vec![a],
        XmlValueKind::List { items, .. } => items
            .iter()
            .filter_map(|member| match &member.value {
                XmlValueKind::Atomic(a) => Some(a),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    atomics
        .into_iter()
        .filter_map(|a| match a {
            XmlAtomicValue::QName(q) | XmlAtomicValue::Notation(q) => Some(q.local_name().to_string()),
            _ => None,
        })
        .collect()
}
```

**Dates, times and durations compare as XSD orders them.** The `PartialEq` and
`PartialOrd` of `DateTimeValue`, `DateValue`, `TimeValue`, the five `g*`
structs, `DurationValue`, `YearMonthDurationValue` and `DayTimeDurationValue`
are the XSD order and equality, computed exactly: date/time values on the time
line, a value without a time zone offset incomparable (`partial_cmp` is
`None`) with one that has an offset unless 14 hours either way give the same
order, and never equal to it; durations by the four reference dateTimes (`P1M`
and `P30D` are incomparable, `P1D == PT24H`). In 0.2.x `==` compared fields
and `partial_cmp` read a missing offset as UTC. The derived equality of
`XmlAtomicValue` and `XmlValue` follows. Code that meant "the same fields"
compares the fields; code that sorts needs its own total order. (The structs
are no longer usable as constant patterns in a `match`.)

### 1.5 Validation API

- `ValidationError::element_path` is a method, `element_path() ->
  Option<&str>`, not a field. Errors and warnings also name their item:
  `subject()` and `instance_path()` (a positional path in XPath 3.0 form).
- `SchemaValidator::new` honours `ValidationFlags::PROCESS_ASSERTIONS`: with
  the bit set, XSD 1.1 assertions are evaluated while streaming. 0.2.x removed
  the bit silently, so a validator built with it evaluated no assertion.
- `DefaultAttribute::attribute_key` is an `Option<AttributeKey>` (`None` for a
  local attribute declaration); new fields `type_key` and `typed_value`.
- `EndOfAttributesView` has a new field `default_attributes`;
  `EndElementInfo` a new field `validation_attempted`.
- `TypeSource` has a new variant `Stipulated` (a `ValidationRoot::Type` run).
- `parser::frames::AttributeFrameResult::inheritable` is an `Option<bool>`.

```rust,ignore
// 0.2.x
if let Some(path) = &error.element_path { eprintln!("at {path}"); }
let key: AttributeKey = default.attribute_key;
```

```rust
use xsd_schema::validation::{DefaultAttribute, ValidationError};

fn where_(error: &ValidationError) -> Option<String> {
    // The local-name path of 0.2.x, or the namespace-aware positional path.
    error.instance_path().map(|p| p.to_string()).or(error.element_path().map(str::to_string))
}

fn is_global(default: &DefaultAttribute) -> bool {
    default.attribute_key.is_some()
}
```

### 1.6 Reading XML: the `xml` layer and its errors

Every reader of XML text — the quick-xml drivers, the typed build,
`BufferDocument::from_reader` / `BufferDocumentBuilder::build` and the
schema-document parser — reads through one well-formedness layer,
`xsd_schema::xml`. Its errors replace the quick-xml types:

- `DriveError`, `DriveWithError` and `BufferDocumentError` lose the variants
  `Parse(quick_xml::Error)`, `Utf8`, `UnboundPrefix` and (`DriveError`,
  `DriveWithError`) `UnexpectedEof`, with their `From` conversions. They gain
  `NotWellFormed(xml::NotWellFormed)` — the rule (`WellFormednessRule`), the
  byte position, the XML version — `LimitExceeded(xml::LimitExceeded)` and
  `Io(std::io::Error)`.
- `build_typed_document` reports input that ends inside an element as
  `BufferDocumentError::NotWellFormed`.
- Byte offsets — the span hooks, `BufferDocument` source spans,
  `TrackedReader` spans — count a leading byte-order mark.

```rust,ignore
// 0.2.x
Err(DriveError::Parse(quick_xml::Error::Syntax(e))) => …
Err(DriveError::UnexpectedEof { depth }) => …
```

```rust
use xsd_schema::validation::DriveError;
use xsd_schema::xml::WellFormednessRule;

fn truncated(err: &DriveError) -> bool {
    match err {
        DriveError::NotWellFormed(e) => e.rule() == WellFormednessRule::Truncated,
        _ => false,
    }
}
```

### 1.7 Schema API

- `FacetSet::merge_with_base` takes the built-in datatype the facet values
  belong to and compares them in its value space: pass the nearest built-in
  type code of the restricted type and the base's effective facets
  (`XmlTypeCode::AnySimpleType` for a list or union type).
- `types::complex::other_matches_namespace`,
  `types::complex::NamespaceConstraint::matches` and
  `schema::wildcard::NamespaceConstraint::allows` take no `XsdVersion`.
- `SchemaError` has a new variant `LimitExceeded` (§1.8);
  `SchemaError::location()` gives any variant's location.
- XSD 1.1 schema documents can no longer name `xs:untypedAtomic` (it is the
  XPath data model's type, not an XSD 1.1 built-in): a `type`, `base`,
  `itemType`, `memberTypes` or type alternative naming it is `src-resolve`,
  and `get_built_in_type_by_qname` no longer finds it in a 1.1 set. A host
  reaches it with `SchemaSet::xpath_type(XmlTypeCode::UntypedAtomic)`.
- `xsd_schema::document::{NamespaceNode, NamespacePageFactory, NamespaceChain,
  NsRef}` and the `NS_PAGE_*` constants are no longer public: no public API
  produced or accepted them. Read a document's namespace bindings through the
  navigator's namespace axis (`DomNavigator::move_to_first_namespace` /
  `move_to_next_namespace`) instead.

```rust,ignore
// 0.2.x
let merged = derived_facets.merge_with_base(&base_facets)?;
let ok = constraint.matches(ns, target_ns, XsdVersion::V1_1);
```

```rust
use xsd_schema::types::complex::NamespaceConstraint;
use xsd_schema::{FacetResult, FacetSet, NameId, XmlTypeCode};

fn merge(derived: &FacetSet, base: &FacetSet) -> FacetResult<FacetSet> {
    derived.merge_with_base(base, XmlTypeCode::Decimal)
}

fn admits(constraint: &NamespaceConstraint, ns: Option<NameId>, target: Option<NameId>) -> bool {
    constraint.matches(ns, target)
}
```

### 1.8 Nesting limits

Schemas are bounded in nesting, so that a deeply nested schema is refused with
an error instead of overflowing the stack: `ParserConfig::max_depth` (default
`parser::DEFAULT_MAX_DEPTH`, 128) bounds the element nesting of each schema
document and, as `SchemaSet::max_depth`, the nesting of components through
references; `ResolverConfig::max_depth` (default 100) the chain of directives;
`xpath::MAX_EXPRESSION_DEPTH` (256) every XPath expression the engine parses.
A schema that breaks one is `SchemaError::LimitExceeded` (naming a
`SchemaLimit`), an expression `XPathError::LimitExceeded`. The deepest schema
of the W3C XSD test suite nests 16 elements, the deepest expression of the W3C
XSD and XQuery suites 14 levels, so only hostile or generated input meets
them. `0` turns a schema limit off (and the protection with it).

```rust
use xsd_schema::{SchemaError, SchemaLimit, SchemaSetBuilder};

let mut deep = String::from(r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">"#);
deep.push_str(r#"<xs:element name="e"><xs:complexType>"#);
for _ in 0..200 {
    deep.push_str("<xs:sequence>");
}
for _ in 0..200 {
    deep.push_str("</xs:sequence>");
}
deep.push_str("</xs:complexType></xs:element></xs:schema>");

match SchemaSetBuilder::new().add_source(&deep, "deep.xsd").and_then(|b| b.compile()) {
    Err(SchemaError::LimitExceeded { limit, .. }) => assert_eq!(limit, SchemaLimit::ElementDepth),
    other => panic!("expected the nesting limit, got {:?}", other.map(|_| ())),
}
```

### 1.9 XSD 1.1 assertions and type alternatives see only the built-in components

Structures 1.1 §3.13.6.2 (XPath Valid) gives the tests of `xs:assert`,
`xs:assertion` and `xs:alternative` a static context whose "in-scope schema
definitions are those components that are present in every schema by
definition". Each test is now checked when the schema is built; one that names
a user-defined type or declaration — `instance of`, `treat as`, `cast as`,
`castable as`, a constructor, `element(*, T)`, `attribute(*, T)`,
`schema-element`, `schema-attribute` — or has another static error makes the
schema invalid (`as-props-correct`, `src-ct-alternative`). In 0.2.x such
assertions failed or passed on each instance. Write the tests against the
built-in types, or move the condition into facets:

| 0.2.x test | 0.3 test |
| --- | --- |
| `$value instance of my:t` | `$value instance of xs:integer and $value le 10` (or a facet of `my:t`) |
| `@a castable as my:t` | `@a castable as xs:integer and xs:integer(@a) le 10` |
| `element(*, my:T)`, `schema-element(my:e)` | a name test, `self::my:e`, or a check of a built-in type |

Node type annotations still count: a node of a user type derived from
`xs:integer` matches `element(*, xs:integer)`. `schema-attribute(xsi:nil)` and
the other three `xsi:` attribute declarations are present in every schema and
stay usable.

## 2. Verdict changes

0.3 fixes cases where 0.2.x accepted what the specifications refuse, or
refused what they allow. A document or schema that changes verdict is listed
here by area; the CHANGELOG has the clause of each.

### Reading XML

- **Newly refused** as `xml::NotWellFormed`: content, CDATA or references
  outside the root element; a second or missing root element; a misplaced or
  malformed XML declaration; a document type declaration outside the prolog or
  not following its grammar; `--` in a comment; a reserved PI target; names that
  are not XML names or qualified names; attributes not separated by white
  space; `<` in an attribute value; characters that are not XML characters, raw
  or as character references; `]]>` in character data; reserved prefixes and
  namespace names; `xmlns:p=""` in XML 1.0; two attributes with the same
  expanded name; input that ends inside an element (`BufferDocument`
  accepted it).
- A document declared `version="1.1"` is read with XML 1.1 and Namespaces in
  XML 1.1 (`xmlns:p=""` undeclares `p`, NEL and U+2028 are line ends).

### Schema loading

- Schema locations are RFC 3986 references: one document is loaded once
  however its location is spelled, and a relative location under a relative
  base stays relative.
- Every entry point applies the same checks: `load_and_process_schema_async`
  used to skip UPA, the XSD 1.0 all-group rules, local element value
  constraints, type alternatives and others; `load_and_process_schema` some
  namespace and substitution-group checks.
- A failed `xs:redefine` that redefines components is `src-redefine`; a
  directive whose document cannot be read is recorded
  (`directive_failures()`), not an error.
- XSD 1.1: `<xs:import>` of the document's own target namespace is
  `src-import`; `xs:untypedAtomic` is not a type a schema document can name.
- Schema documents are checked against the order the content models give:
  `<xs:assert>` after `<xs:anyAttribute>` is accepted (XSD 1.1), an `assert`
  before an attribute or particle, an `assert` or `openContent` in a simple
  type's restriction, and an inline `<xs:simpleType>` in a complex-content
  restriction are refused.

### Restriction and content models

- `derivation-ok-restriction` compares against the base's content with its
  pointless nested sequences flattened (Exchange Web Services' schemas load),
  and counts occurrences exactly on both sides: restrictions whose content
  admits sequences the base rejects are refused in both versions; valid
  restrictions with such counts load.
- A particle with `minOccurs="0" maxOccurs="0"` maps to no component.
- XSD 1.1: All Group Limited is enforced (`cos-all-limited`); an extension of a
  type without content by an `xs:all` group is validated as that all group in
  both versions.
- The attribute uses of a restriction are read as Structures §3.4.2.4 builds
  them: some restrictions that weaken, retype or change a `fixed` attribute
  their base still has are refused; a group's prohibition no longer hides a
  base's required attribute.

### Simple types and facets

- An enumeration value is normalized with the base type's white space, and a
  restriction without an enumeration of its own keeps the base's.
- Facets compare in the value space: `NaN` satisfies no bound; date/time values
  with and without a time zone compare as Datatypes 1.1 §D.2.1 says; durations
  in their partial order; `xs:gMonth` on the 1972 time line; binary values as
  octets; `xs:QName` / `xs:NOTATION` values by expanded name; unions and lists
  member by member.
- A complex type with simple content checks the facets of its restriction as a
  simple type does (bounds, digits, assertions).
- `-0` is a valid unsigned value; `xs:duration` refuses misordered fields
  (`P1M1Y`); `xs:dayTimeDuration` / `xs:yearMonthDuration` refuse `PT`, `P1DT`,
  `P+1D`.
- A seconds value with more fraction digits than the typed value holds is
  valid, its typed value truncated, with the warning `xs-seconds-capacity`.

### Values and value constraints

- `fixed` values compare by value ("equal or identical"): `NaN` matches `NaN`,
  one instant in two time zones matches, `PT60S` matches `PT1M`. Under an
  `xsi:type` or a type alternative, content is compared with the declaration's
  {value} (XSD 1.0 through its canonical lexical form).
- A QName or NOTATION `default` / `fixed` value is read with the bindings of
  the schema element it is written on; one with an unbound prefix is a schema
  error.
- An empty element under an `xsi:type` takes the value constraint's lexical
  form, and one whose `xsi:type` does not admit it is `cvc-elt.5.1.1`.
- An element with `xsi:nil="true"` whose declaration has a `fixed` value is
  `cvc-elt.3.2.2`.

### PSVI: lax assessment, nil, ID/IDREF and identity constraints

- An element assessed laxly has `[validity]` notKnown, also when one of its
  attributes or descendants is invalid (still reported).
- An element is nilled only when its declaration is nillable; the
  end-of-element `SchemaInfo::is_nil` is the [nil] property (`false` for a
  nilled element with content).
- Default and fixed attribute values from attribute groups and base types take
  part in identity constraints and in the ID/IDREF and ENTITY checks;
  `get_default_attributes` reports all of them.
- **A keyref sees only its own element's keys** (those declared on it and its
  descendants): a key on a sibling or an ancestor no longer satisfies it, and
  such instances are invalid (`cvc-identity-constraint.4.3`). A keyref's
  `refer` may name a key declared later.
- `xs:ENTITY`, `xs:ID` and `xs:IDREF` members of lists and unions are checked
  by their own member type.

### Typed documents

- Defaulted attributes are attribute nodes and element defaults text nodes
  (`TypedBuildOptions::with_defaults_as_nodes(false)` for the 0.2.x tree);
  white space in element-only content is no text node; simple values have
  their normalized value as their string value.
- `fn:id` finds an element whose typed value is a single `xs:ID`; `fn:idref`
  is new.

### XPath

- The implicit timezone comes from the dynamic context (the host's offset,
  read once per evaluation, without one); date/time and duration comparisons
  and arithmetic are exact, and raise `FODT0001` / `FODT0002` / `FOCA0005` /
  `FOAR0001` / `FOAR0002` at the limits instead of panicking or wrapping.
- Casts: the one-sided integer types are unbounded; `xs:float` / `xs:double`
  to `xs:decimal` raises `FOCA0001` when too large; a cast to `xs:QName` from
  anything but a string literal or a QName is `XPTY0004`.
- `fn:subsequence` with an empty position or length is `XPTY0004`; an unclosed
  comment is `XPST0003`; `fn:base-uri` removes dot segments.
- Regular expressions (`xsd11` feature, the `regexml-fork` 0.3 engine):
  `^` and `$` are ordinary characters in `xs:pattern`; a counted repetition of
  an emptiable operand keeps its bounds; captured groups are restored on
  backtracking; without the `s` flag `.` matches a carriage return.

## 3. Deprecations

`ValidationFlags::PROCESS_IDENTITY_CONSTRAINTS` is deprecated: identity
constraints are always enforced (they are part of an element's validity,
Structures §3.3.4.3 clause 6), and the bit never had an effect. Remove it from
your flags.

```rust,ignore
// 0.2.x
let flags = ValidationFlags::default() | ValidationFlags::PROCESS_IDENTITY_CONSTRAINTS;
```

```rust
use xsd_schema::validation::ValidationFlags;

let flags = ValidationFlags::default(); // identity constraints included
# let _ = flags;
```

`XPathValue::as_str` and `XmlItem::as_str` are deprecated: they return an
owned `Option<String>` — a copy of the string of a single `xs:string` or
`xs:untypedAtomic` item — not the borrow the name suggests. `as_str` →
`to_owned_string`, which returns the same.

```rust,ignore
// 0.2.x
let name = args.remove(0).as_str().unwrap_or_default();
```

```rust
# #[cfg(feature = "xsd11")] {
use xsd_schema::xpath::functions::XPathValue;
use xsd_schema::xpath::RoXmlNavigator;

let mut args: Vec<XPathValue<RoXmlNavigator<'static>>> = vec![XPathValue::string("World")];
let name = args.remove(0).to_owned_string().unwrap_or_default();
assert_eq!(name, "World");
# }
```
