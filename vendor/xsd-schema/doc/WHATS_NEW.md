# What's New in 0.3

0.3.0 is a correctness release with new host APIs. It changes verdicts where
0.2.x was wrong, bounds what hostile input can cost, and breaks the API in
places to make room for both. This page is the overview; the detail is in the
[upgrade guide](UPGRADING_0_3.md) and the [CHANGELOG](../CHANGELOG.md).

## New capabilities

- **Validation roots.** A run validates its root by declaration (the default),
  against a stipulated element declaration or type ("validate as this type"),
  or laxly — `ValidationRoot`, Structures 1.1 §5.2.
  [Introduction §2](INTRODUCTION.md#the-validation-root)
- **Typed documents in one pass.** `build_typed_document_with` validates and
  builds with your validator, sink and handler; the tree is the XPath data
  model of the result — defaults as nodes, no white space in element-only
  content, normalized values. [Introduction §8](INTRODUCTION.md#typed-documents)
- **Diagnostics that name their item.** Every error and warning has a subject
  and a positional path (`/Q{urn:po}order[1]/Q{}item[3]/@Q{}qty`).
  [Introduction §2](INTRODUCTION.md#driving-validation-from-a-quick-xml-stream)
- **Schema loading for real-world sets.** Locations are RFC 3986 references,
  loaded once however they are spelled; a `LocationRewriter` maps them to local
  mirrors; directives whose document cannot be read are reported
  (`directive_failures`) instead of lost.
  [Introduction §4](INTRODUCTION.md#locations-and-mapping-them-to-local-copies),
  [Extensibility §2](EXTENSIBILITY.md#28-location-rewriting)
- **XPath for hosts.** The implicit timezone from the context, exact date/time
  and duration arithmetic, `fn:idref`, `fn:error` as a built-in, a regular
  expression cache a host keeps across runs (`RegexCache`), a warning channel,
  host error payloads (`XPathError::host`), casts and constructors for
  user-defined atomic types, and `SchemaSet::xpath_type` /
  `element_value_constraint` for hosts that validate data-model nodes.
  [Introduction §3](INTRODUCTION.md#3-xpath), [Extensibility §4](EXTENSIBILITY.md#4-custom-xpath-functions)

## Correctness and conformance

The W3C XSD 1.0 suite passes 39,461 of 39,510 tests (39,458 in 0.2.3; the 15
failures are documented disputes), XSD 1.1 2,313 of
2,319, and XQTS's XPath 2.0 tests 8,047 of 8,047 (the README has the
commands). Most of the work is in cases
the suites barely reach, found by differential sweeps against Xerces-J, xmllint
and independent oracles:

- restriction checks with exact occurrence counts, and the base's nested
  sequences flattened;
- facets in the value space — enumerations, bounds, `NaN`, dates with and
  without time zones, durations, QNames by expanded name, lists and unions
  member by member;
- `fixed` and `default` values compared by value, QName defaults read with the
  schema's bindings;
- the PSVI of lax assessment and nil; ID/IDREF and identity constraints with
  defaulted attributes; keyrefs scoped to their element;
- XSD 1.1 assertions and type alternatives checked at schema time against the
  built-in components, as Structures 1.1 §3.13.6.2 requires;
- one well-formedness layer for every reader of XML text, XML 1.0 and 1.1 with
  their Namespaces recommendations.

Some documents and schemas that 0.2.x accepted are refused now, and the
reverse: [upgrade guide §2](UPGRADING_0_3.md#2-verdict-changes) lists them by
area.

## Safety and limits

- **No stack overflow on deep input.** Schema documents, component chains,
  directive chains and XPath expressions are bounded in nesting and refused
  with `LimitExceeded` instead of crashing; the tree-walk drivers and
  `fn:deep-equal` walk an instance tree of any depth.
  [Implementation Limits §6](IMPLEMENTATION_LIMITS.md#6-nesting)
- **Parse limits for instances.** `ParseLimits` bounds depth, names, attributes,
  namespaces, text and nodes before anything is interned.
  [Implementation Limits §4](IMPLEMENTATION_LIMITS.md#4-xml-input)
- **No panics at the edges of the value types.** Date/time and duration
  comparison and arithmetic are exact and raise the specification's errors at
  the limits; a seconds value with more digits than the typed value holds is
  valid, truncated, and warned about (`xs-seconds-capacity`).
  [Implementation Limits §5](IMPLEMENTATION_LIMITS.md#5-dates-times-and-durations)
- **The guides are tested.** Every example in `doc/` is compiled and run with
  the crate's doctests.

## Performance

Each `xs:pattern` is compiled once; key, unique and keyref tables are indexed
(no more quadratic time on large documents); the Unique Particle Attribution
check reuses its storage; extension chains no longer compile in exponential
time; list values
of unions are not validated twice; a deep `BufferDocument` is validated,
serialized and read for its namespaces in time linear in its depth. The
measurements are in the
[CHANGELOG](../CHANGELOG.md).

## Breaking changes

Many public enums and structs are `#[non_exhaustive]`; XPath errors carry
error QNames (`XPathError::Raised`); `ItemType` element and attribute tests
take a `TypeTest`; QName values are `QNameValue`; list members are `XmlValue`s;
the date/time and duration structs compare as XSD orders them; the driver and
document error enums report `xml::NotWellFormed` instead of quick-xml errors;
`FacetSet::merge_with_base` takes the datatype; XSD 1.1 schema documents can no
longer name `xs:untypedAtomic`. Each change, with the 0.2.x and the 0.3 code,
is in the [upgrade guide](UPGRADING_0_3.md).
