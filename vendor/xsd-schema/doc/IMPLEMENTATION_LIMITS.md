# Implementation Limits

Where the specifications leave a value space unbounded and this crate stores
values in a bounded type, and what happens at the bound. Datatypes 1.1 §5.4
allows such limits on two conditions: a value beyond them "must not be treated
as invalid", and it "must not be quietly changed to a supported value".

## 1. `xs:decimal`

### Validation is exact

The value space of `xs:decimal` is unbounded, and every value is validated
exactly: `totalDigits`, `fractionDigits`, the bound facets, `enumeration`,
fixed/default values and identity-constraint fields are decided on the exact
value, however large or precise. The same holds for the integer types derived
from `xs:decimal`.

### The typed value

The typed value (`XmlAtomicValue::Decimal`, a `rust_decimal::Decimal`) holds a
value exactly when it is `m / 10^s` for integers with

- `|m| ≤ 2^96 − 1 = 79228162514264337593543950335`, and
- `0 ≤ s ≤ 28`

(trailing fraction zeros do not count). Every value that a `totalDigits` facet
of 28 accepts qualifies: Datatypes §4.3.11.3 makes it `i / 10^n` with
`|i| < 10^28 < 2^96` and `n ≤ 28`. A count of significant digits does not
decide it — `10^−29` and `10^29` have one each and do not qualify.

A valid value beyond that is never rounded:

- its typed value is its canonical representation (Datatypes §3.3.3.2), an
  `XmlAtomicValue::String` with the `xs:decimal` type code, so
  `XmlValue::as_decimal` returns `None`;
- the validator reports a warning with code `xs-decimal-capacity` (Datatypes 1.1
  §5.4) while `ValidationFlags::REPORT_WARNINGS` is set, which it is by default.

### In XPath

A decimal held in canonical form:

- compares exactly;
- casts to `xs:string` (the canonical form), `xs:boolean`, `xs:integer`
  (truncation), `xs:double` and `xs:float` (nearest value);
- raises `FOAR0002` in arithmetic, rather than computing with an approximation.

An XSD 1.1 assertion (`xs:assert`, `xs:assertion`) whose evaluation fails only
because it computed with such a value is not counted as false: its outcome is
unknown, and the value carries the `xs-decimal-capacity` warning. Any other
error still fails the assertion (Structures §3.13.4.1), also when such a value
occurs elsewhere in the instance.

Casting a string to `xs:decimal`, or writing a decimal literal, follows F&O
§17.1.1 and §6.2:

| Value | Result |
| --- | --- |
| magnitude larger than held | `FOAR0002` |
| nonzero magnitude below `10^−28`, the smallest positive value held | `0` (underflow) |
| any other value with more precision than is held | `FOCA0006` |

Casting an `xs:float` or `xs:double` to `xs:decimal` follows F&O §17.1.3.3 ("the
xs:decimal value … that is numerically closest to SV"):

| Value | Result |
| --- | --- |
| magnitude larger than held | `FOCA0001` |
| `NaN`, `INF`, `-INF` | `FOCA0002` |
| nonzero magnitude below `10^−28` | `±10^−28` above `5 × 10^−29`, else `0` |
| any other value | the shortest decimal that converts back to the same binary value |

## 2. `xs:integer`

`xs:integer` values have no size limit (`BigInt`), and neither have the four
types derived from it whose value space is unbounded on one side:
`xs:nonNegativeInteger`, `xs:positiveInteger`, `xs:nonPositiveInteger` and
`xs:negativeInteger`.

## 3. Regular expressions

Groups and character classes may be nested 64 levels deep (`(`…`)` and
`[`…`]`, a subtracted class `[a-z-[aeiou]]` included, counted together). The
regular-expression engine compiles and matches by recursion, one level of the
stack per level of nesting, so a deeper expression is refused rather than
allowed to overflow the stack: an `xs:pattern` facet is an invalid pattern
(the message names the limit), and `fn:matches`, `fn:replace` and
`fn:tokenize` raise `FORX0002`. Real expressions nest a few levels; the
deepest in the W3C XML Schema test suite nests 15.

## 4. XML input

Every reader of XML text in the crate — the streaming validation driver, the
typed build, `BufferDocument::from_reader` and the schema-document parser —
reads through one well-formedness layer (`xsd_schema::xml`). A document that
is not well-formed is refused with `xml::NotWellFormed`, naming the rule and
the byte position (XML 1.0 §1.2: a fatal error); the checks are always on.

### Versions

- A document declared `version="1.1"` is read with XML 1.1 (Second Edition)
  and Namespaces in XML 1.1 (Second Edition): character references to the
  restricted control characters are allowed and the characters themselves
  refused raw, NEL and LINE SEPARATOR are line ends in character data and
  attribute values, and a prefix may be undeclared (`xmlns:p=""`).
- Every other document is read with XML 1.0 (Fifth Edition) and Namespaces in
  XML 1.0 (Third Edition): no declaration (XML 1.1 §4.3.4: "treated as if
  labeled as version 1.0"), `version="1.0"`, and any other `1.x` (XML 1.0
  §2.8: "it will process it as a 1.0 document").
- Not supported: a NEL or LINE SEPARATOR used as white space *inside markup*
  of an XML 1.1 document (between attributes, in an end tag, in a declaration)
  is refused (`WellFormednessRule::Xml11LineEndInMarkup`), although XML 1.1
  reads it as a line feed; the optional XML 1.1 normalization checks (§2.13)
  are not made.

### Encodings

- The readers read UTF-8. A UTF-8 byte-order mark is skipped. Bytes that are
  not well-formed UTF-8 are refused (XML 1.0 §4.3.3).
- The XML declaration's `encoding` does not change how the bytes are read: a
  host that decodes another encoding first (UTF-16 with `decode_xml_bytes`,
  say) passes UTF-8 text that keeps its original declaration. The
  declaration is reported to `ValidationEventHandler::on_xml_declaration` and
  kept by `BufferDocument::xml_declaration`; a host that receives bytes it did
  not decode refuses an `encoding` other than UTF-8 there.
- UTF-16 or another encoding passed undecoded is not read correctly: most such
  input is refused as ill-formed UTF-8, but bytes of a single-byte encoding
  can happen to form valid UTF-8.

### Document type declarations

- A document type declaration is accepted only in the prolog, once; its name,
  external identifier and internal subset are checked for well-formedness to
  the extent that the subset's markup is balanced (literals, comments,
  processing instructions, markup declarations, parameter-entity references).
- It is reported to `ValidationEventHandler::on_doctype`; a host refuses DTDs
  there.
- Its declarations are not processed — not yet supported: the external subset
  is never read, a reference to an entity the internal subset declares is
  refused (only `amp`, `lt`, `gt`, `apos` and `quot` are known), and
  attribute defaults and types declared in it are not applied.

### Parse limits

- `ParseLimits` (no limits by default): element depth, name length,
  namespace-URI length, attributes and namespace declarations per element,
  namespaces in scope, attribute-value size, text size (also white space
  outside the root element and the document type declaration) and the node
  count (attributes and namespace declarations included). Set on the
  validation runtime (`set_parse_limits`) or the document builder
  (`with_parse_limits`).
- Each is checked on the raw markup before anything of it is interned or
  built: nothing of the construct that breaks a limit — or a well-formedness
  rule — reaches the name table. What came before it in the document was read
  normally.
- They do not bound peak memory: the underlying reader holds a whole tag, text
  run, comment, processing instruction or document type declaration in its
  buffer before it is checked, and a text run is gathered before validation.
  Bound the size of the input as well.

### Known gaps

- Line ends inside CDATA sections, comments and processing instructions are
  passed as written: a carriage return (CR LF, or a lone CR) is not
  normalized to a line feed, and in an XML 1.1 document NEL and LINE
  SEPARATOR stay as they are there. Character data and attribute values are
  normalized.

## 5. Dates, times and durations

### The typed values

A year is held in `i32` (`xs:dateTime`, `xs:date`, `xs:gYearMonth`,
`xs:gYear`), the year, month, day, hour and minute numbers of a duration in
`u32`, and seconds in a `rust_decimal::Decimal` (whole seconds below `2^96`, at
most 28 fraction digits). A literal beyond the integer limits is not a value
here; one with more fraction digits than a `Decimal` holds is valid and its
typed value keeps the fraction truncated toward zero — not rounded, which could
make `…59.999…` an invalid second 60 — while the facets, value constraints and
identity constraints compare the literal as written. The validator reports the
warning `xs-seconds-capacity` for such a value (Datatypes 1.1 §5.4), while
`ValidationFlags::REPORT_WARNINGS` is set; the `types::value` module
documentation has the details.

### In XPath

- **Comparison** (`eq`, `lt`, …, the general comparisons, `min`, `max`,
  `distinct-values`, `index-of`, `deep-equal`) is exact for every value held:
  whole seconds in `i128` and the fraction apart, so no comparison rounds or
  overflows. Durations compare by their months and seconds (F&O §10.4.5).
- **Arithmetic** is exact up to the limits of the result's value struct; beyond
  them it raises the F&O error rather than wrapping or panicking (F&O §10.1.1):
  `FODT0001` for a date/time result whose year leaves `i32` and for the
  difference of two date/time values beyond `2^32 − 1` days, `FODT0002` for a
  duration result beyond `2^32 − 1` days or years. A result's seconds field
  holds at most the digits a `Decimal` holds next to its whole seconds; more
  fraction digits are truncated toward zero, and the evaluation raises the
  warning `xs-seconds-capacity` (an `xpath::XPathWarning`, once per evaluation
  and operator or function: `XPathEvaluator::with_warnings` collects it). During
  validation such a warning from an XSD 1.1 assertion or type alternative is
  reported to the sink, as the same code is for a literal.
- **Casts** of a literal beyond the integer limits raise `FODT0001` (date/time
  types) or `FODT0002` (durations), F&O §17.1.1.
- **Year numbering**: as XSD 1.0, `-0001` is the year before `0001`; a year
  `0000`, which the XSD 1.1 lexical space admits, is read as `0001` (F&O
  §10.1.1: "The results of operations on dates that cross the year 0000 are
  implementation-defined").
- **The implicit timezone** comes from the dynamic context
  (`DynamicContext::implicit_timezone`, F&O §10.4). A context without one
  uses the host's local offset (the process's `TZ`), read once per
  evaluation together with the instant `fn:current-dateTime` reports. The
  functions that take no dynamic context (`operators::value_eq`, `eval_binary`,
  …) read the host's offset when they compare a date/time value;
  `operators::eval_binary_with_timezone` takes the timezone explicitly.

## 6. Nesting

The schema-time passes, the XPath binder and evaluator, and the parsers of
the crate recurse along the structures they read. So that a deeply nested
input is refused with an error rather than overflowing the thread's stack —
an abort, not a recoverable error — every such structure is bounded:

| Limit | Default | Bounds | Refusal |
| --- | --- | --- | --- |
| `ParserConfig::max_depth` | 128 (`parser::DEFAULT_MAX_DEPTH`) | the element nesting of each schema document (`xs:schema` is 1), checked on the raw markup | `SchemaError::LimitExceeded`, `SchemaLimit::ElementDepth`, at the element |
| `SchemaSet::max_depth` | the same value, set by `SchemaSetBuilder::compile` and `load_and_process_schema` | the longest chain of components nested through references — model groups in model groups (inline or by `<xs:group ref>`), a type in its base, item or member types, an attribute group in the groups it references, an element in its substitution group's head — measured once, without recursion, before any pass recurses along it | `SchemaError::LimitExceeded`, `SchemaLimit::ComponentDepth`, at the component |
| `ResolverConfig::max_depth` | 100 | the chain of `xs:include` / `xs:import` / `xs:redefine` / `xs:override` directives, counted along the shortest chain that reaches a document | `SchemaError::LimitExceeded`, `SchemaLimit::DirectiveDepth`, at the directive |
| `xpath::MAX_EXPRESSION_DEPTH` | 256 (fixed) | the nesting of every XPath expression the engine parses — a host's, and the tests of a schema's `xs:assert`, `xs:assertion` and `xs:alternative` (checked when the schema document is read) | `XPathError::LimitExceeded` (`ParseError::LimitExceeded` from the parser); in a schema `SchemaLimit::XPathDepth` |

`0` turns a schema limit off, and with it the protection: the passes' own
recursion bounds then refuse only some shapes, and a deep enough schema
overflows the stack. Identity-constraint selectors and fields need no limit:
their grammar is a union of paths, parsed by a loop.

**Headroom.** The deepest schema of the W3C XSD test suite nests 16 elements
(its deepest component chain 9 levels); the deepest real schema of a corpus of
1,691 WSDL and XSD files 25. 128 is five times that, and every shape at the
limit compiles on a 2 MiB thread in an unoptimized build too (the worst, a
chain of group references, uses about 830 KiB). XPath expressions of the XQuery
1.0 test suite and of the W3C XSD 1.1 suite nest at most 14 levels. 256 counts
tree levels — a parenthesized expression is two, a predicate three, so 126
nested parentheses or 84 nested predicates fit, and a chain of `or`, `+` or
`if … else` nests one level per operator. At the limit an optimized build
evaluates within about half a MiB of stack (344–479 KiB measured, for every
shape and entry point); an unoptimized build needs about 40 KiB a level, so a
larger thread near the limit. A tree a host builds into an `AstArena` itself is
not checked; `xpath::expression_depth` measures one.

Instance documents are not bounded by default, and need no bound for the
stack: every walk over an instance keeps its depth on the heap. The streaming
drivers keep their element stack there; the tree-walk drivers
(`drive_navigator`, `drive_buffer_document`) and the tree comparison
(`fn:deep-equal`, `TreeComparer`) walk iteratively, with an explicit stack, as
do `BufferDocumentBuilder::copy_subtree` and the serializer
(`document::serialize`). A tree of any depth —
one a host builds with `BufferDocumentBuilder` included, which no parse limit
covers — is validated and compared on an ordinary 2 MiB thread. The time of
these walks grows linearly with the depth as well: an element of a
`BufferDocument` finds the namespaces in scope without visiting its ancestors
(*Namespace scope* in `BUFFER_DOCUMENT_OVERVIEW.md`), so 100,000 nested levels
are validated in about 0.05 s and serialized in about 0.015 s in an optimized
build, whether one element declares the namespaces or every element declares
the default namespace again. Two shapes still cost more. When each level adds
a prefix, every element has that many namespace nodes, and the walks that read
them (validation, serialization, the `namespace::` axis) take time quadratic in
the depth. When the levels rebind two prefixes in turn, the walks are quadratic
too (20,000 levels, about 1.4 s). A host that
reads documents of untrusted depth sets `ParseLimits::max_depth` on the runtime
or the document builder (§4), which bounds the depth of the text it reads and
so of the trees it builds. The parser of a third-party tree is its own: roxmltree
0.19, behind `RoXmlNavigator`, recurses once per element level as it parses
(about 3,000 levels fit in 2 MiB in an optimized build).

