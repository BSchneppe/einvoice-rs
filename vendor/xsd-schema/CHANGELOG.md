# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0] - 2026-10-29

Conformance against 0.2.3: the W3C XSD 1.0 suite passes 39,461 of 39,510 tests
(was 39,458). Three tests now pass: `reZ001.v` (the regular-expression
engine), `addB139.i` (a schema document that undeclares a prefix,
`xmlns:f=""`, is not namespace-well-formed XML 1.0) and `addB065.i` (a nilled
element whose declaration has a fixed value is invalid); none fails that
passed. The XSD 1.1 suite (2,313 of 2,319) and the XPath 2.0 tests of XQTS
(8,047 of 8,047) are unchanged. All 32 GAEB DA XML 3.3 schemas now load in XSD
1.0 mode (9 were refused by the restriction check before).

### Breaking changes

See [Upgrading to 0.3](doc/UPGRADING_0_3.md) for every item below with the
0.2.x and the 0.3 code, and the verdict changes users will see;
[What's New in 0.3](doc/WHATS_NEW.md) is the short overview.

**Error and view types**

- `XPathError`, `FunctionId` and `ItemType` are `#[non_exhaustive]`: a `match`
  on them needs a wildcard arm, and later variants are additive. `FunctionId`
  has the new variants `Error` and `Idref`.
- `DriveError`, `DriveWithError` and `BufferDocumentError` are
  `#[non_exhaustive]`, have `NotWellFormed` and `LimitExceeded` variants, and
  report an I/O error of the source as `Io(std::io::Error)`. The variants
  `Parse(quick_xml::Error)`, `Utf8`, `UnboundPrefix` and (`DriveError`,
  `DriveWithError`) `UnexpectedEof` are removed, with their `From`
  conversions; no quick-xml type is left in these error types.
- `ValidationError` and `ValidationWarning` are `#[non_exhaustive]`.
  `ValidationError::element_path` is a method (`element_path() ->
  Option<&str>`, the same local-name path as before) instead of a field.
  `Display` is unchanged. Build warnings with `ValidationWarning::new`.
- `SchemaError` and `xpath::parser::ParseError` are `#[non_exhaustive]`; a
  `match` needs a wildcard arm. `SchemaError::location()` gives any variant's
  location.
- `TypeSource` is `#[non_exhaustive]` and has a new variant,
  `TypeSource::Stipulated`: the governing type a `ValidationRoot::Type` run
  stipulates for the validation root. A `match` on `TypeSource` needs a
  wildcard arm.
- `EndOfAttributesView` is `#[non_exhaustive]` and has a new field,
  `default_attributes`: the attributes the element's attribute uses default.
  It can no longer be built with a struct literal outside the crate.
- `EndElementInfo` is `#[non_exhaustive]` and has a new field,
  `validation_attempted`: the element's `[validation attempted]` beside its
  `[validity]`.
- `DefaultAttribute::attribute_key` is an `Option<AttributeKey>` (`None` for a
  local attribute declaration), and `DefaultAttribute` has a new field
  `type_key: Option<TypeKey>`, the declaration's type; `DefaultAttribute` is
  `#[non_exhaustive]`.
- `parser::frames::AttributeFrameResult::inheritable` is an `Option<bool>`:
  `None` when the schema document does not write `inheritable`, so that an
  attribute reference takes its declaration's `{inheritable}` (XSD 1.1
  §3.2.2.3).

**Reading XML text**

- Every reader of XML text — the quick-xml validation drivers, the typed
  build, `BufferDocument::from_reader` / `BufferDocumentBuilder::build` and
  the schema-document parser — goes through one well-formedness layer
  (`xsd_schema::xml`) and refuses a document that is not well-formed or not
  namespace-well-formed with `xml::NotWellFormed`, naming the rule
  (`xml::WellFormednessRule`) and the byte position. Newly refused: content,
  CDATA or references outside the root element; a second or missing root
  element; a misplaced or malformed XML declaration; a document type
  declaration outside the prolog or not following its grammar; `--` in a
  comment; a reserved PI target; names that are not XML names or qualified
  names; attributes not separated by white space; `<` in an attribute value;
  characters that are not XML characters, raw or as character references of
  any length; `]]>` in character data; reserved prefixes and namespace names;
  `xmlns:p=""` in XML 1.0; two attributes with the same expanded name; bytes
  that are not UTF-8 anywhere in a schema document. Namespace names are
  compared after the attribute-value normalization of the document's XML
  version, by the well-formedness checks and the validators alike.
- What was refused before is now refused as `xml::NotWellFormed` too:
  duplicate attributes, unbound prefixes, undeclared entities, bytes that are
  not UTF-8, mismatched end tags, unclosed markup and input that ends inside
  an element (which names the innermost open element and how many are open) —
  `build_typed_document` reported the last as an internal error.
- `BufferDocument::from_reader` / `BufferDocumentBuilder::build` refuse input
  that ends inside an element and input without a root element (both were
  accepted).
- A document declared `version="1.1"` — an instance or a schema document — is
  read with XML 1.1 and Namespaces in XML 1.1: character references to the
  restricted control characters are allowed and the raw characters refused,
  NEL and U+2028 are line ends in text and attribute values, and `xmlns:p=""`
  undeclares `p` (in a schema document, QName-valued attributes and the XPath
  expressions of selectors, fields, type alternatives and assertions then
  treat it as unbound). Every other document is read with XML 1.0. A NEL or
  U+2028 used as white space inside markup is not supported.
  `NamespaceContext::add_namespace` with an empty URI undeclares a prefix.
- Byte offsets — `on_element_start_offset` / `on_element_end_offset`,
  `BufferDocument` source spans, `TrackedReader` spans — count a leading
  byte-order mark (they were offsets after it), and `TrackedReader` spans
  start where their event starts.

**XPath**

- Errors identified by an error QName — what `fn:error` raises, a host's own
  codes, and the specification codes without a variant of their own — are the
  new variant `XPathError::Raised(RaisedErrorInfo)` instead of an `Internal`
  message. `XPathError::raised`, `raised_error` and `error_code` keep working;
  the `Display` of such an error is `[CODE] description`.
- `XPathError::XPDY0050` carries a message (`XPDY0050 { message }`), and
  XPST0080 is a variant of its own (`XPST0080 { type_name }`, no longer in
  `QNAMED_ERROR_CODES`). `XPathError::more_than_one_item()` returns `XPTY0004`
  (XPath 2.0 §3.1.5); `XPDY0050` is the code of `treat as` and of a leading `/`
  outside a document.
- `XPathError::FOCA0002 { qname }` is `XPathError::FOCA0002 { value }`: the
  error is also raised for the special float / double values cast to
  `xs:decimal` / `xs:integer` (see *Fixed*), so the field holds the offending
  lexical form, not only a QName. Its message is "[FOCA0002] Invalid lexical
  value: '…'".
- `XPathError` is no longer `UnwindSafe` / `RefUnwindSafe`: the payload of a
  host error (`XPathError::host`, see *Added*) is an
  `Arc<dyn Error + Send + Sync>`. Wrap an error in
  `std::panic::AssertUnwindSafe` to carry it across `catch_unwind`.
- `ItemType::Element` and `ItemType::Attribute` are struct variants,
  `Element { name, type_test, nillable }` and `Attribute { name, type_test }`,
  built with `ItemType::element` / `ItemType::attribute`. Their type is a
  `TypeTest` (`#[non_exhaustive]`): a schema type key — simple or complex —, a
  predefined `xs` type by its code, or `xs:untyped`; `TypeTest::canonical` gives
  the spelling the binder produces. `ItemType::element` without a type ignores
  `nillable`. The element and attribute tests of a host-built `ItemType` read
  the node's type annotation (complex types included) and the untyped rules
  (`xs:untyped`, `xs:untypedAtomic`).
- `xpath::ast::TypeExprNode` has a crate-private field, so it cannot be built
  with a struct literal outside the crate: use `TypeExprNode::new`. Bind a node
  again after editing it.
- `SequenceType`'s `Display` prints element, attribute, document and schema
  tests in SequenceType syntax.

**Typed values**

- `XmlAtomicValue::QName` and `XmlAtomicValue::Notation` hold a `QNameValue`
  instead of a `QualifiedName`: the expanded name (`QNameValue::name()`)
  together with its string form, so such a value converts to a string without
  a name table. Build one with `QNameValue::new(name, &name_table)`.
- `XmlValueKind::List::items` is a `Vec<XmlValue>` (it was a
  `Vec<XmlAtomicValue>`): each member carries its own type code and
  `schema_type` — the item type's or, for a list whose item type is a union,
  those of the member type that validated that member. `item_type` is the code
  of the declared item type (`xs:anySimpleType` for a union); it was the type
  of the last member. `XmlValue::to_xpath_value` takes no argument.
- The `PartialOrd` and `PartialEq` of the date/time value structs
  (`DateTimeValue`, `DateValue`, `TimeValue` and the five `g*` structs) and of
  `DurationValue`, `YearMonthDurationValue` and `DayTimeDurationValue` are now
  the XSD order and equality, computed exactly: date/time values on the time
  line, a value without a time zone offset incomparable (`None`) with one that
  has an offset unless 14 hours either way give the same order, and never
  equal to it; durations by the four reference dateTimes (`P1M` and `P30D` are
  incomparable, `P1D == PT24H`, `P400Y == P146097D`). Before, `PartialEq`
  compared fields and `PartialOrd` read a missing offset as UTC and durations
  in average months. The derived `PartialEq` of `XmlAtomicValue` and
  `XmlValue` follows. Upgrading: code that used `==` to mean "the same fields"
  compares the fields; `partial_cmp` may return `None`, so sort with your own
  total order if you need one. The structs are no longer usable as constant
  patterns in a `match` (they lost `StructuralPartialEq`).

**Schema API**

- `FacetSet::merge_with_base` takes the built-in datatype the facet values
  belong to (`merge_with_base(&base, XmlTypeCode::Decimal)`) and compares them
  in its value space: a `fixed` bound is matched by another literal of the
  same value, an enumeration value must equal a value of the base's, the
  bounds are ordered as the datatype orders them (exactly for `xs:decimal`, in
  the partial orders of the date/time and duration datatypes), every bound is
  checked against every bound of the base by the "valid restriction"
  constraints, and a `length` must equal the base's. `base` is the base type's
  facets, inherited ones included. With `XmlTypeCode::AnySimpleType` (a list
  or union type), and for `xs:QName` / `xs:NOTATION`, literals are compared as
  written. The schema compiler uses the same method. Upgrading: pass the
  nearest built-in type code of the restricted type and the base's effective
  facets.
- `types::complex::other_matches_namespace`,
  `types::complex::NamespaceConstraint::matches` and
  `schema::wildcard::NamespaceConstraint::allows` no longer take an
  `XsdVersion`: no namespace constraint depends on the version.
- The `document::namespace` module is private: `NamespaceNode`,
  `NamespacePageFactory`, `NamespaceChain`, `NsRef` and the `NS_PAGE_*`
  constants are no longer public (they were also re-exported from `document`).
  No public API handed one out or took one, so only code that named them is
  affected; the namespace bindings of a document are read through
  `BufferDocNavigator` (the namespace axis).

**Behaviour**

- `SchemaValidator::new` (and `with_substitution_groups`) honour
  `ValidationFlags::PROCESS_ASSERTIONS`: with the flag set, XSD 1.1 `xs:assert`
  is evaluated inline while streaming, as with `new_fragment_buffer`. The flag
  used to be removed silently, so assertions never ran. `new_main_document`
  still removes it; that mode evaluates no assertion at all (now documented).
- Schemas are bounded in nesting, so that a deeply nested schema is refused
  with an error instead of overflowing a 2 MiB thread stack (for XPath
  expressions: in an optimized build). `ParserConfig::max_depth` (default
  `DEFAULT_MAX_DEPTH` = 128, `0` = no limit) bounds the element nesting of
  each schema document and, as `SchemaSet::max_depth`, the nesting of
  components through references (model groups, base / item / member types,
  attribute groups, substitution group heads), checked before any pass
  recurses along it. `ResolverConfig::max_depth` (default 100) bounds the
  chain of `xs:include` / `xs:import` / `xs:redefine` / `xs:override`
  directives. XPath expressions nest at most
  `xpath::depth::MAX_EXPRESSION_DEPTH` = 256 levels — in a schema's assertions
  and type alternatives, checked when the schema is read, and in any
  expression the XPath engine parses; at that depth an optimized build
  evaluates within half a MiB of stack (an unoptimized build needs a larger
  stack near the limit). Each refusal is a `SchemaError::LimitExceeded` naming
  its setting (`SchemaLimit`), or `XPathError::LimitExceeded`. `0` turns the
  schema limits off, and with them this protection. The deepest schema of the
  W3C XSD suite nests 16 elements deep; the deepest XPath expression of the
  W3C XSD and XQuery test suites 14 levels.
- Every entry point checks a schema alike: `load_and_process_schema` and
  `load_and_process_schema_async` run the builder's directive rules and phases
  — the asynchronous pipeline used to skip UPA, the XSD 1.0 all-group rules,
  local element value constraints, type alternatives, identity-constraint
  references and the substitution-group checks, the synchronous one the local
  `targetNamespace`, `no-xsi` and substitution-group consistency checks — and
  the asynchronous directive resolution applies the synchronous rules (it
  skipped the `src-import` / `src-include` namespace checks and tried catalog
  entries first). The pipeline adds the XML catalog and records its primary
  document as loaded, like the builder.
- Schema locations are resolved as RFC 3986 URI references (§5.2) and
  normalized (§6.2.2, §6.2.3) when the location or its base has a scheme: dot
  segments are removed, scheme and host compared in lower case, `file:` URIs
  resolved and read, and one document is loaded once however its URL is
  spelled — a local `file:` URI and the path it names are one document. A
  relative location under a relative base is no longer made absolute against
  the working directory.
- A failed `xs:redefine` that redefines components is an error
  (`src-redefine`; Structures §4.2.4, clause 1: its `schemaLocation` "must
  successfully resolve").
- `xs:openContent` with a mode other than `none` and no `xs:any` child is
  refused (XSD 1.1, Complex Type Definition Representation OK, clause 3).
- A model group that contains itself is reported as `mg-props-correct.2`;
  attributes and elements the schema for schemas does not allow, and invalid
  attribute values, as `s4s-att-not-allowed` / `s4s-elt-invalid-content` /
  `s4s-att-invalid-value` (was `ct-props-correct` / `sch-props-correct`).
- A schema document of an XML Schema 1.1 set can no longer name
  `xs:untypedAtomic` — a `type`, `base`, `itemType`, `memberTypes` or type
  alternative naming it is `src-resolve`, as in an XML Schema 1.0 set. The
  type is the XPath 2.0 data model's (XML Schema 1.1 Structures §1.3.1.1: "not
  defined in this specification"); a host reaches it through
  `SchemaSet::xpath_type`. `vc:typeAvailable` / `vc:typeUnavailable` treat it
  as not available in either version.

### Added

**Validation**

- `ValidationRoot` and `ValidationRuntime::set_validation_root`: how a run
  assesses its validation root (XSD 1.1 §5.2) — by the top-level declaration
  of the root's name (the default, unchanged), against a stipulated element
  declaration (`Element`) or type definition (`Type`: "validate as this type",
  whatever the root's name), or laxly when the root is undeclared (`Lax`: no
  `cvc-elt.1`, `[validity]` notKnown). Every driver that takes the caller's
  runtime honours it, the typed build included.
- `document::build_typed_document_with`: validate and type a document in one
  pass with your own `SchemaValidator` (e.g. with assertions), your own
  `ValidationSink` (errors reported, not dropped) and your own
  `ValidationEventHandler` (also `&mut dyn`) alongside the builder; options in
  `TypedBuildOptions`. `document::TypedBuilderHandler` (public) composes the
  builder with a handler for hosts that drive `drive_quick_xml_with`
  themselves; `TypedBuild`, `TypedBuildError`. `build_typed_document` is built
  on it.
- Errors and warnings name the item they are about: `subject()`
  (`ValidationSubject::Element` / `Attribute` with an `ExpandedName`) and
  `instance_path()` — a namespace-aware, positional path in XPath 3.0 form,
  `/Q{urn:po}order[1]/Q{}item[3]/@Q{}qty` (`InstancePath`, `PathStep`),
  including for errors reported later (unresolved IDREFs, nested assertions,
  deferred keyrefs). Paths share their ancestor steps, so a diagnostic costs
  the same at any depth; the positions cost no measurable validation time.
  `with_subject` / `with_instance_path` for errors a host builds itself.
- `TypedBuildOptions::defaults_as_nodes` and
  `TypedBuilderHandler::with_defaults_as_nodes` (default `true`).
- `ValidationRuntime::take_default_attributes` and
  `EndOfAttributesView::default_attributes`: the defaulted attributes of the
  element, found by the end-of-attributes step.
- `DefaultAttribute::typed_value`: the value constraint's {value}, read with
  the namespace bindings of the schema element it is written on. It is present
  when the run keeps PSVI typed values, and always for a type whose values can
  be or hold QNames or NOTATIONs.
- `ValidationEventHandler::skip_attribute` (defaulted to `false`) lets a
  handler have an attribute that the element's type does not declare recorded
  but not validated, as if a `processContents="skip"` wildcard matched it (no
  PSVI, no error, the element's `[validation attempted]` partial); a declared
  attribute is always validated. Also
  `ValidationRuntime::skip_attribute[_by_id]`. Honoured by the quick-xml
  drivers and the typed build; the tree-walking drivers validate every
  attribute.

**Reading XML text**

- Parse limits (`xml::ParseLimits`): depth, name and namespace-URI length,
  attributes and namespace declarations per element, namespaces in scope
  (bound prefixes; undeclarations are not counted), attribute-value and text
  size, and nodes. Checked on the raw markup before anything of it is
  interned; a violation is `xml::LimitExceeded` naming the limit.
  `ValidationRuntime::set_parse_limits` for the drivers and the typed build,
  `BufferDocumentBuilder::with_parse_limits`. No limits by default; they do
  not bound the reader's memory.
- `ValidationEventHandler::on_doctype` (`DoctypeView`) and
  `ValidationEventHandler::on_xml_declaration` (`xml::XmlDeclaration`), both
  defaulted to accept: return an error to refuse documents with a document
  type declaration, or an encoding the host did not decode.
  `BufferDocument::xml_declaration`.
- `ParseLimit::description()` and `ParseLimit::unit()`: what a parse limit
  counts and its unit, the words `LimitExceeded` uses.
- `TrackedReader::error_position` and `TrackedReader::xml_version`; a schema
  document that is not well-formed gets a source location.

**Schema loading**

- `SchemaSet::directive_failures` / `CompiledSchemaSet::directive_failures`
  and `DirectiveFailure`: the directives whose schema document could not be
  read (not schema errors, Structures §4.2.3 / §4.2.6.2), with the location as
  written, the namespace, the location read and the directive's position. A
  reference that then fails to resolve names those documents.
- `LocationRewriter` / `LocationRequest`,
  `SchemaSetBuilder::set_location_rewriter`,
  `SchemaResolver::set_location_rewriter`: map the locations of directives
  (with the import's namespace) and the locations given to the builder before
  they are identified and loaded, in the style of an XML Catalog's
  `rewriteSystem` / `uri` entries. The answer is final — an absolute URI or
  file path is loaded as it is, whatever the base of the document that named
  the location; a relative path is resolved against the working directory —
  and the rewriter is asked once per location.
- `SchemaSetBuilder::with_resolver`, `SchemaResolver::{catalog, config,
  loader}`, `SchemaSet::{max_depth, set_max_depth}`,
  `parser::DEFAULT_MAX_DEPTH`, `SchemaError::LimitExceeded`, `SchemaLimit`,
  `XPathError::LimitExceeded`, `xpath::depth::{MAX_EXPRESSION_DEPTH,
  expression_depth}`, `NameTable::heap_bytes`.
- `schema::validate_assertion_xpath` (`xsd11`): the schema-time XPath Valid
  check of assertions (see *Changed*), beside the existing
  `validate_cta_xpath` of type alternatives.

**XPath**

- `fn:error` is a built-in function, with the four signatures of F&O §3; a
  host's own registration of it still takes precedence. The error object
  travels with the error (`RaisedErrorInfo::error_object`).
- `XPathError::host(namespace, local, description, payload)`: a host error with
  a code of its own and a payload of the host's own error type, read back with
  `XPathError::downcast_host_payload` / `host_payload`, or as the error's
  `source()`. `RaisedErrorInfo` implements `std::error::Error`; cloning a
  raised error shares its data. Also `XPathError::treat_mismatch` and
  `root_not_a_document`.
- XPath: `cast as`, `castable as` and constructor functions for the atomic
  simple types of the schema set attached to the static context
  (`XPathContext::with_schema_set`), XPath 2.0 §3.10.2 – §3.10.4. The value is
  cast to the type's nearest built-in type and checked against the type's
  facets with the schema validator (`FORG0001`, naming the type, when one is
  not satisfied): a string or `xs:untypedAtomic` source as given, after the
  type's whitespace normalization, any other value through its `xs:string`
  cast; when the conversion of a string changes its value (an `xs:decimal`
  below 10^-28 becomes 0), the type's digits, bounds, enumeration and XSD 1.1
  assertions are checked again on the result. A cast up the hierarchy only
  changes the annotation. The result carries the type as
  `XmlValue::schema_type`, so `instance of` and `treat as` see it. Types
  derived from `xs:QName` and `xs:NOTATION` cast as `xs:QName` does. A
  function the host registers under a user type's constructor name and arity
  takes precedence. A type in no namespace has a constructor when the default
  function namespace is none. Lists and unions have neither (`XPST0051`,
  `XPST0017`). XSD 1.1 assertions do not see these casts and constructors
  (Structures §3.13.6.2 clause 2.2.5): there they behave as before.
- `fn:idref` (F&O 1.0 §15.5.3), in its one- and two-argument forms: the
  elements and attributes with the is-idrefs property whose string value has
  one of the given IDs as a token, in document order. It is empty for a tree
  without type annotations. The is-idrefs property:
  `DomNavigator::is_idrefs` and `DomNavigator::find_nodes_by_idref` (defaults:
  `false`, none). A typed document sets it on an element or attribute whose
  typed value holds an `xs:IDREF` value (`document::Node::IS_IDREFS`);
  `BufferDocumentBuilder::register_idrefs` sets it on a node without an
  annotation.
- `xpath::RegexCache`: compiled regular expressions of `fn:matches`,
  `fn:replace` and `fn:tokenize` that a host keeps across evaluation runs.
  Lend it to a run with `XPathEvaluator::with_regex_cache`, or
  `DynamicContext::with_regex_cache` / `set_regex_cache` (inside a `run_with`
  callback). A host that evaluates the same expressions once per item — for
  example a test evaluated for every item of a loop — then compiles each
  pattern once instead of once per evaluation. Bounded (64 entries by
  default, `RegexCache::with_capacity`, least recently used dropped), `Send`
  but not `Sync`, no global state. Without one, each run keeps its own cache as
  before. `compose::Composer` keeps one such cache for all its evaluations.
- `DynamicContext::implicit_timezone_or_host`,
  `xpath::operators::eval_binary_with_timezone`.
- XPath warnings: `xpath::XPathWarning`, `XPathEvaluator::with_warnings`,
  `DynamicContext::take_warnings`. A date/time or duration result of `+`, `-`,
  `fn:sum` or `fn:avg` whose seconds have more fraction digits than the value
  holds beside the whole seconds keeps them truncated toward zero and raises
  the warning `xs-seconds-capacity` (once per evaluation and operator or
  function). During validation, such a warning from an XSD 1.1 assertion or
  type alternative is reported to the sink, as the same code is for a literal
  (with `ValidationFlags::REPORT_WARNINGS`).
- `types::QNameValue` (`new`, `name`, `prefix`, `local_name`, `lexical`): its
  `Display` is the lexical form, and its equality ignores the prefix.
- `TypeTest`, `ItemType::element`, `ItemType::attribute`, and `Display` for
  `ItemType`, `NameTest`, `TypeTest` and the AST sequence types, kind tests and
  QNames.
- `XPathValue::to_owned_string` and `XmlItem::to_owned_string`: the string of
  a single atomic item of type `xs:string` (or a type derived from it) or
  `xs:untypedAtomic`, copied; `None` for any other value. They replace
  `as_str` (deprecated, below), whose name suggests a borrow.

**Hosts of the XPath data model**

- `SchemaSet::xpath_type` / `BuiltinTypes::xpath_type`: the types the XPath
  2.0 data model adds to the XML Schema namespace (`xs:anyAtomicType`,
  `xs:untypedAtomic`, `xs:dayTimeDuration`, `xs:yearMonthDuration`) for a host
  that validates nodes against a type it names — in an XML Schema 1.0 set too,
  where they validate, annotate and take part in the type hierarchy as in a
  1.1 set. Schema documents of a 1.0 set still cannot name them.
  `ValidationFlags::XPATH_TYPES`: an `xsi:type` may name those types where the
  set's XSD version does not define them. `types::validators::UntypedAtomicValidator`
  validates `xs:untypedAtomic`.
- `SchemaSet::element_value_constraint` and
  `SchemaSet::element_value_constraint_for_type`: an element declaration's
  {value constraint} — its variety (`default` / `fixed`), its {lexical form}
  and its {value}, with `xs:QName` / `xs:NOTATION` values read with the
  namespace bindings of the schema element the value is written on (Structures
  §3.3.2.1, Datatypes §3.3.18). Under a governing type (`xsi:type`, an XSD 1.1
  type alternative) the second gives the {lexical form} read under that type —
  what validation reports as `SchemaInfo::typed_value` when an empty element
  takes the value — and whether the value constraint is a valid default for
  the type (Element Default Valid (Immediate), required for an `xsi:type`). A
  host that builds and validates an element can now declare the prefixes such
  a value needs on the start tag. New types `schema::ElementValueConstraint`,
  `schema::ValueConstraintVariety`.
- `schema::composition::CompositionEdgeKind` implements `Hash`.

### Changed

**Regular expressions**

- The regular-expression engine of the `xsd11` feature is now `regexml-fork`
  0.3, the maintained fork of `regexml` (the code keeps the name `regexml`; no
  public item exposes an engine type). It brings fixes that change results:
  captured groups are restored when the matcher backtracks (`fn:replace`'s
  `$N`); a counted repetition keeps its bounds when its operand can match the
  empty string — an `xs:pattern` such as `(\d|){2}` accepted any number of
  digits; a repetition tries every way of matching before it fails —
  `([a-z]*)+x` rejected valid values; and an expression nested more than 64
  levels deep is an error instead of a stack overflow.
- `xs:pattern` (`xsd11` feature, `RegexCompat::Strict`): `^` and `$` are
  ordinary characters, as XSD 1.0 Appendix F and XSD 1.1 Part 2 §G.3 define
  them; they were compiled as anchors, so `^abc$` matched `abc` instead of
  `^abc$`. W3C `reZ001.v` now passes. Under `RegexCompat::LenientMs` they stay
  anchors.
- XSD 1.1 patterns: a block escape with an unrecognized name denotes every
  character only when the name matches `IsBlock` (`'Is' [a-zA-Z0-9-]+`, Part 2
  §G.4.2.4); `\p{IsFoo_Bar}` is now an invalid pattern.
- `RegexCompat::LenientMs` with XSD 1.1 no longer applies the strict XSD
  grammar check, so back-references, `(?:…)` and reluctant quantifiers are
  accepted as with XSD 1.0, as documented.
- `fn:matches`, `fn:replace`, `fn:tokenize` use the engine's XPath 2.0 dialect:
  without the `s` flag `.` matches a carriage return (#xD) — F&O 1.0 §7.6.1.1
  excludes only the newline; the `x` flag removes exactly #x9, #xA, #xD and
  #x20 (a form feed now stays); a pattern that nests groups and character
  classes more than 64 levels deep raises `FORX0002`. See
  [Implementation Limits](doc/IMPLEMENTATION_LIMITS.md).
- `FacetError::InvalidPattern` messages are the engine's `Display` text
  (`invalid regular expression: …`) instead of its `Debug` form.

**XPath**

- XPath: date/time comparisons, the subtraction of two date/time values,
  `fn:adjust-*-to-timezone` (one argument), `fn:implicit-timezone` and
  `fn:current-dateTime` read the implicit timezone of the dynamic context
  (`DynamicContext::implicit_timezone`); without one, the host's local offset,
  read once per evaluation. `fn:current-dateTime()` is the current instant
  expressed in that timezone. `eq` on `xs:gYear`, `xs:gYearMonth`,
  `xs:gMonthDay`, `xs:gMonth` and `xs:gDay` compares starting instants with
  the implicit timezone (F&O §10.4.15 – §10.4.19).
- XPath: date/time and duration comparisons are exact (no rounding beyond 28
  digits) and durations compare by months and seconds; arithmetic is exact and
  raises FODT0001 / FODT0002 / FOCA0005 / FOAR0001 / FOAR0002 at the limits of
  the values instead of panicking or wrapping. An XSD 1.1 assertion or type
  alternative comparing the largest durations no longer panics.
- XPath casts: `xs:nonNegativeInteger`, `xs:positiveInteger`,
  `xs:nonPositiveInteger` and `xs:negativeInteger` have no size limit;
  `xs:float`/`xs:double` → `xs:decimal` raises FOCA0001 when too large and
  gives ±10⁻²⁸ for magnitudes in (5 × 10⁻²⁹, 10⁻²⁸); a date/time or duration
  literal beyond the value limits raises FODT0001 / FODT0002.
- XPath: a cast or constructor to `xs:QName` (or a type derived from
  `xs:QName` or `xs:NOTATION`) from anything but a string literal or a value
  of the same base type raises XPTY0004, and `castable as` is false.
- XPath: `fn:subsequence` with an empty `$startingLoc` or `$length` raises
  XPTY0004; an unclosed `(:` comment is XPST0003; `fn:base-uri` resolves
  `xml:base` per RFC 3986 (dot segments removed); the substring functions
  answer a zero-length second argument before FOCH0004.
- XPath 1.0 compatibility mode: general comparisons follow XPath 2.0 §3.5.2
  after atomization. `XPathMode::XPath10`: `number()` reads strings as XPath
  1.0 does (no exponent, `+` or `INF`).
- `DynamicContext::eval_function` calls the context's function evaluator
  without `unsafe` code (it turned the evaluator reference into a raw pointer
  and back). The library has three `unsafe` blocks left, in two places
  ([Unsafe Code](doc/UNSAFE.md)).

**Schema documents**

- The XPath of assertions and type alternatives sees only the built-in
  components (XSD 1.1). Structures 1.1 §3.13.6.2 (XPath Valid) gives the tests
  of `xs:assert`, `xs:assertion` and `xs:alternative` a static context whose
  "in-scope schema definitions are those components that are present in every
  schema by definition", and a static error makes the schema invalid
  (§3.13.6.1, §3.12.6). Each test is now checked when the schema is built: one
  that names a user-defined type or declaration — `instance of`, `treat as`,
  `cast as`, `castable as`, a constructor, `element(*, T)`, `attribute(*, T)`,
  `schema-element`, `schema-attribute` — or has another static error (an
  undeclared prefix, an undefined variable, an unknown `xs:` constructor, a
  syntax error) is refused with `as-props-correct` (assertions) or
  `src-ct-alternative` (type alternatives). The components present in every
  schema stay in scope: the built-in types and the four `xsi:` attribute
  declarations, so `schema-attribute(xsi:nil)` is fine; there is no built-in
  element declaration. Such assertions used to fail or pass on each instance;
  `instance of` / `treat as` a user type were evaluated. A function the engine
  does not know, in a namespace other than the XML Schema one, is still left
  to the evaluation (`cvc-assertion`). The schema-time check of a type
  alternative now also applies the schema document's `xpathDefaultNamespace`.
  Upgrading: an assertion such as `$value instance of my:t` or
  `@a castable as my:t` must be written against built-in types —
  `$value instance of xs:integer and $value le 10`,
  `@a castable as xs:integer and xs:integer(@a) le 10` — or the condition
  moved into facets of `my:t`; `element(*, my:T)` / `schema-element(my:e)`
  become name tests (`self::my:e`) or checks of built-in types. Node type
  annotations still count: a node of a user type derived from `xs:integer`
  matches `element(*, xs:integer)`.
- XSD 1.1 assertions: an assertion whose test is a static error now — a
  `schema-element(N)` / `schema-attribute(N)` with an undeclared `N`, or a
  union or list type in `instance of`, `treat as`, `cast` or `castable` —
  fails with `cvc-assertion`, as other static errors in assertions already
  did.
- XSD 1.1: a type alternative whose test calls the constructor of a
  user-defined atomic type is rejected with `src-ct-alternative`, as one with
  `cast as` such a type already was (Type Alternative Properties Correct, XPath
  Valid). Before, the test was accepted and never matched.
- Assertions after the attribute wildcard (XSD 1.1). `<xs:assert>` is accepted
  after `<xs:anyAttribute>` in `<complexType>` and in the `<restriction>` /
  `<extension>` of simple and complex content, as Structures 1.1 §3.4.2
  allows, and the children of these elements are checked in the order the
  content models give: an `assert` before an attribute, an attribute group,
  the attribute wildcard, a particle or an `openContent`, a second
  `openContent`, an `openContent` after the particle, and a facet after an
  `assert` are refused. An `assert` or `openContent` in a simple type's
  `<restriction>`, and an inline `<simpleType>` in a complex-content
  `<restriction>`, are refused; they were accepted and ignored.
- Schema errors are deterministic. When several independent components are
  invalid, the error reported no longer varies from run to run: types are
  checked in the order they are defined, enumeration values in sorted order.
- `cos-nonambig` (Unique Particle Attribution) names the same competing pair
  on every run: the first in content-model order. A content model with several
  competing pairs could name a different pair from run to run; the verdict and
  the constraint were already stable.
- Schema-document errors raised from an element's attributes (an undeclared
  prefix in a QName, say) carry the element's location; with error recovery,
  the first error inside an element is reported rather than a consequence of
  skipping the refused child.
- `<xs:whiteSpace value=" preserve "/>` and
  `<xs:explicitTimezone value=" required "/>` are read after whitespace
  collapse, as their `xs:NMTOKEN` type prescribes (the first was taken as
  `collapse`, the second was a schema error).

**Restriction checks**

- Complex-content restriction (`derivation-ok-restriction`) is checked against
  the base's content with its nested sequences flattened: the sequence an
  extension builds from its base's content and its own (one extension level is
  enough), nested `xs:sequence` elements, and, under XSD 1.1, model-group
  references. XSD 1.0 §3.9.6 ignores these "pointless" sequences and XSD 1.1
  §3.4.6.4 compares languages; a restriction that lists the elements of an
  all-optional base one after another was rejected. For example Exchange Web
  Services' `WellKnownResponseObjectType` now loads.
- The restriction check counts occurrences exactly, on both sides: a group
  that repeats a particle needing two or more occurrences admits counts with
  gaps (`sequence{1,2}(b{2,2})` admits two or four `b`s, never three), and the
  check no longer fills them in. Restrictions whose content admits child
  sequences their base rejects are now rejected in both versions — a choice
  branch that needs two or more occurrences restricted to fewer, a
  single-branch choice or a sequence repeating such a branch (W3C
  `particlesHa147` under XSD 1.1), a repeated group branch, a repeated choice
  whose iterations mix its branches in an order the base rejects
  (`choice{0,2}(a, b)` admits `(b, a)`), and a base sequence repeated more
  than once matched by a single iteration's worth of the restriction
  (`sequence(sequence{2,2}(b, c), d?)` restricted to `sequence(b, c)`). Valid
  restrictions with such counts load in both versions, an identical
  restriction among them.
- The restriction check's occurrence arithmetic is exact far beyond any schema
  (products of nested bounds up to 2¹²⁰) and every comparison is bounded: a
  restriction whose bounds multiply past 64 bits — for example
  `choice{3000000,∞}(choice{3000000,∞}(a{3000000}))` restricted by itself — no
  longer hangs.
- A particle with `minOccurs="0" maxOccurs="0"` maps to no component: under
  XSD 1.1 a choice whose branches are all such particles accepts nothing when
  it must occur, and repeating it does not make it admit anything
  (`choice{1,unbounded}(a{0,0})` no longer admits `a`).
- XSD 1.1: a repeated choice restricting a base sequence is accepted when one
  child of the sequence accepts the whole choice and the other children are
  emptiable (`sequence(choice{0,2}(a, b), c?)` restricted to `choice{0,2}(a,
  b)`); a restriction to an optional element is accepted when a sibling branch
  of the base choice is emptiable (`choice(a, b?)` restricted to `a?`). XSD
  1.0 keeps §3.9.6's rule that a choice cannot restrict a sequence.
- XSD 1.0: a single-child sequence or choice that is not pointless (its
  occurrence range is not 1..1) is compared as written, child ranges included,
  against the base as written (`choice?(a?)` does not restrict `choice?(a,
  b)`; `sequence(a, sequence?(b))` restricts `sequence(a, sequence?(b, c?))`).
  All GAEB DA XML 3.3 schemas now load under XSD 1.0.

**Values, facets and value constraints**

- XSD 1.0 mode compares `fixed` values by value too (see *Fixed*). That
  departs from the letter of two XSD 1.0 clauses that compare strings —
  Attribute Locally Valid (Use), "its normalized value must match the
  canonical lexical representation of the attribute use's {value constraint}
  value", and derivation-ok-restriction 2.1.3.2, "fixed with the same string"
  — so in 1.0 mode values that are equal but spelled differently are accepted
  there (`13:00:00+01:00` against `fixed="12:00:00Z"`). See
  [Overview](doc/OVERVIEW.md).
- An enumeration value is normalized with the base type's whiteSpace: its
  values are values of the base type (Datatypes 1.1 §4.3.5), so
  `<xs:enumeration value=" red "/>` restricting `xs:token` — or `xs:language`,
  `xs:Name`, `xs:NCName`, `xs:NMTOKEN`, `xs:ID`, `xs:anyURI`, `xs:QName`,
  `xs:NOTATION`, a `normalizedString` value with a tab, a list, a union member
  with its own `whiteSpace`, a simple content restriction — now admits `red`
  (it admitted no value before), and so do the restrictions inheriting it,
  their defaults, fixed values, casts and further enumerations. An enumeration
  value of an `xs:string` type keeps its white space, also under a restriction
  that collapses.
- A `default` or `fixed` value of type `xs:QName` or `xs:NOTATION` (also in a
  list, or as a union's member) is read with the namespace bindings of the
  schema element it is written on (Datatypes §3.3.18). An empty element that
  takes such a value is validated and typed with it, also where the instance
  binds the prefix elsewhere or not at all, and a defaulted attribute's value
  joins identity constraints as that value.
- The attribute wildcard of an extension that declares one of its own takes
  that wildcard's {process contents} (Structures §3.4.2.5; it took the weaker
  of the two), so some attributes the combined wildcard admitted laxly or
  skipped before are validated strictly now.
- Facet verdicts follow the specification text where Xerces-J 2.12.2 (XSD 1.0)
  differs: it reports a minimum and a maximum bound that are incomparable
  (date/time values with and without a time zone offset, durations such as
  `P1M` and `P30D`) as out of order, where the text forbids only "greater
  than"; it keeps a base's `fixed` facet after a restriction step restates it
  without `fixed` and does not check a facet against an inclusive bound a
  later step replaced; and it compares `fixed` `g*` values as strings.
- The date/time and duration values hold a year in `i32`, the other duration
  fields in `u32` and the seconds in a 96-bit decimal; literals beyond those
  are refused (unchanged; now documented in `types::value`). A seconds number
  with more fraction digits than the typed value holds (28) is valid: the
  typed value holds it truncated toward zero (it was rounded, and `…59.999…`
  with 29 nines became an invalid second 60); the facets, value constraints
  and identity constraints compare the literal as written, and the validator
  reports the warning `xs-seconds-capacity`.

**Identity constraints and the PSVI**

- Keyrefs see only their own element's keys. A `keyref` is checked against the
  identity-constraint table of the element that declares it — its own keys and
  uniques and those of its descendants (Structures §3.11.4 clause 4.3,
  §3.11.5). A key declared on a sibling or an ancestor of that element no
  longer satisfies it; such instances were accepted and are now invalid
  (`cvc-identity-constraint.4.3`).
- `refer` may name a key declared later. A keyref's `refer` is resolved once
  every identity constraint of the schema is known, so a key or unique
  declared later in the document, on a local element, or in another schema
  document of the namespace is found. Such schemas were refused with
  `src-identity-constraint`.
- The end-of-element `SchemaInfo::is_nil` is the [nil] property (Structures
  §3.3.5.3): `false` for an element with `xsi:nil="true"` that has character
  or element children (`cvc-elt.3.2.1`, still reported). The start-of-element
  value still says whether the element is nilled.

**Typed documents**

- A typed document (`build_typed_document`, `build_typed_document_with`,
  `TypedBuilderHandler`) is the XDM instance of the PSVI in two more ways
  (XDM 1.0 §6.2.4, §6.7.4):
  - each defaulted attribute is an attribute node, and a valid empty element
    that takes its `default` / `fixed` value has a text node with it. A QName
    in such a value is written with a prefix the instance binds to its
    namespace; a prefix is declared on the element when none is (a QName in
    no namespace where a default namespace is in scope keeps its lexical
    form). `with_defaults_as_nodes(false)` restores the old behaviour for
    defaults;
  - a text node that is all white space is not built in element-only
    content.
- In a typed document (`build_typed_document`, `build_typed_document_with`), a
  valid attribute and a valid element of a simple type or with simple content
  have their [schema normalized value] as their string value (XDM 1.0 §6.2.4,
  §6.3.4): an `xs:token` value read as `"  a   b "` is `"a b"`. Such an
  element keeps one text node, none for an empty value, beside its comments
  and processing instructions; invalid nodes and mixed or element-only content
  keep their text as read. XSD 1.1 assertions see the same string values.

**Documentation**

- Documented: `SchemaInfo::typed_value` is kept whatever
  `ValidationFlags::BUILD_PSVI_TYPED_VALUES` says for an element that took its
  declaration's `default` / `fixed` value and for elements and attributes
  whose type is or can hold `xs:ID`, `xs:IDREF`, `xs:QName` or `xs:NOTATION`
  values.
- The guides in `doc/` are compiled and run with the crate's doctests, and
  two are new: [What's New in 0.3](doc/WHATS_NEW.md) and
  [Upgrading to 0.3](doc/UPGRADING_0_3.md).
- docs.rs builds the documentation with every feature (`all-features = true`),
  so the asynchronous loading API (`async`: `AsyncSchemaLoader`,
  `SchemaSetBuilder::compile_async`, `load_and_process_schema_async`, …) is
  documented there too; it was left out. The crate documentation lists the
  Cargo features and what each adds.

### Deprecated

- `ValidationFlags::PROCESS_IDENTITY_CONSTRAINTS`. Identity constraints are
  always enforced — they are part of an element's validity (Structures
  §3.3.4.3 clause 6) — and the bit never had an effect; remove it.
- `XPathValue::as_str` and `XmlItem::as_str`: they return an owned
  `Option<String>`, not the borrow the name suggests. Use `to_owned_string`,
  which returns the same.

### Fixed

**XPath**

- Casting an `xs:QName` or `xs:NOTATION` to `xs:string` or `xs:untypedAtomic`
  (`xs:string($q)`, `$q cast as xs:string`) raised `XPTY0004`; it gives
  `prefix:local`, or the local name (F&O §17.1.2). Every other string
  conversion of such a value — `fn:string`, `concat`, `string-join`,
  `XmlValue::to_string_value`, `Display` — gave an internal debug form
  (`QName(Some(NameId(…)):…)`) and now gives the same lexical form.
- `schema-element(N)` / `schema-attribute(N)` with an `N` that is not declared
  is the static error XPST0008 (XPath 2.0 §2.5.4.4, §2.5.4.6), also when no
  schema set is attached.
- A union or list type as the type of `instance of`, `treat as`, `cast` or
  `castable` is the static error XPST0051 (XPath 2.0 §2.5.4.2).
- `instance of` / `treat as` a user-defined atomic type match by derivation
  (XPath 2.0 §2.5.4.2); they were always false.
- `instance of` / `treat as` `element(N)` expand an unprefixed `N` with the
  default element namespace, as a path step does (XPath 2.0 §2.5.4).
- `element(*, xs:anyAtomicType)` / `attribute(*, xs:anyAtomicType)` match
  annotated atomic nodes with an XSD 1.0 schema set too.
- `document-node(E)` matches a document with exactly one element child and
  otherwise only comments and processing instructions (XPath 2.0 §2.5.4.2), in
  every form of the test.
- `xs:NOTATION(…)` and `xs:anyAtomicType(…)` name no function (XPST0017, XPath
  2.0 §3.10.4); they were XPST0051 and a dynamic `XPTY0004`.
- A failed `treat as` says which sequence type was expected and what the
  operand was.
- XPath SequenceType matching (`instance of`, `treat as`, `element(N, T)`,
  `attribute(N, T)`, `schema-element`, `schema-attribute`,
  `ItemType::matches_item`) no longer treats a member type of a union as
  derived from the union; XPath 2.0's derives-from is restriction and
  extension only. Validation is unchanged.
- XPath: a cast to a built-in type no longer keeps the user-defined
  `schema_type` of a value whose type code already matches (F&O §17.3: the
  result has a new type annotation).
- XPath: an unprefixed constructor function call is expanded with the default
  function namespace, not the default element/type namespace (`integer('1')`
  with the XML Schema namespace as default function namespace was `XPST0051`).
- XPath: casting `NaN`, `INF` or `-INF` to `xs:decimal`, `xs:integer` or a type
  derived from them raises `FOCA0002` (F&O §17.1.3.3, §17.1.3.4), not
  `FORG0001`.
- XPath: casts between `xs:QName` and `xs:NOTATION` (and types derived from
  them) raise `XPTY0004`, as the casting table of F&O §17.1 requires.
- XPath: `fn:ceiling`, `fn:floor`, `fn:round` and `fn:round-half-to-even` of a
  value of a type derived from `xs:integer` return an `xs:integer` (F&O §6.4).
- `fn:resolve-uri` resolves a reference that contains characters `xs:anyURI`
  allows but a URI holds only escaped (a space, for example) instead of
  raising FORG0002, and returns it unescaped.
- `fn:local-name-from-QName` and `fn:prefix-from-QName` return `xs:NCName`,
  and `fn:in-scope-prefixes` returns its prefixes as `xs:NCName`, as F&O
  specifies.
- The typed value of a union-typed element or attribute is, for XPath, the
  value of the member type that validated it (XDM 1.0 §3.3.1.2):
  `data($n) instance of my:member` is true for a user-defined member type.
- A year `-2147483648` is written as itself (was `--2147483648`).
- `fn:adjust-dateTime-to-timezone` and `fn:adjust-date-to-timezone` raise
  FODT0001 when the adjusted year is out of range (a debug build panicked, a
  release build wrapped the year).
- XSD 1.1 type alternatives: a test that calls an XML Schema constructor
  function that does not exist (`xs:anyAtomicType('a')`, `xs:NOTATION('a')`)
  or casts to `xs:NOTATION` / `xs:anyAtomicType` makes the schema invalid
  (`src-ct-alternative`, Structures §3.12.6); the alternative was skipped.

**Facets and the value space**

- A restriction of an enumerated simple type that adds no `enumeration` of its
  own (`<xs:restriction base="t:color"/>`, or one adding only `maxLength`,
  `pattern`, a bound, …) keeps its base type's enumeration (Structures 1.1
  §3.16.6.4): instance validation, `cast as` / `castable as` and the
  constructor functions accepted any value of the primitive type before, in
  both XSD versions. A further restriction enumerating values outside the
  inherited enumeration, and a default or fixed value outside it, are now
  schema errors (QName and NOTATION enumeration values are compared by their
  lexical form).
- `NaN` no longer satisfies `minInclusive`, `maxInclusive`, `minExclusive` or
  `maxExclusive` of a simple type derived from `xs:float` or `xs:double` (it
  is incomparable with every other value). A `NaN` bound admits no value in
  XSD 1.1; in XSD 1.0 mode an inclusive `NaN` bound admits `NaN` alone (XSD
  1.0 §3.2.4). A bound written with surrounding whitespace is no longer
  ignored.
- A date/time value without a time zone offset no longer equals an enumerated
  value with one, or the reverse (`2000-01-01` against `2000-01-01Z`), and it
  satisfies a bound with an offset — or a value with an offset a bound without
  one — only when it does so with both +14:00 and −14:00 imputed (Datatypes
  1.1 §D.2.1). This tightens bounds: `2000-01-01T00:00:00` no longer satisfies
  `minInclusive="2000-01-01T00:00:00Z"`, `2000-01-01Z` no longer satisfies
  `minInclusive="2000-01-01"`, and `12:00:00` no longer satisfies
  `minInclusive="00:30:00Z"`.
- `xs:gMonth` values are ordered as months on the 1972 time line (Datatypes
  1.1 §E.3.4): with offsets of ten hours and more, bounds accepted values
  outside them (`--07+14:00` under `maxInclusive="--06-14:00"`) and rejected
  values inside them (`--07+14:00` under `minInclusive="--06-14:00"`).
- `enumeration` of `xs:hexBinary` and `xs:base64Binary` types compares octet
  sequences (`0A` is `0a`), and of the `xs:gYearMonth`, `xs:gYear`,
  `xs:gMonthDay`, `xs:gDay` and `xs:gMonth` types values (`--02Z` is
  `--02+00:00`, `---16+13:00` is `---15-11:00`).
- A complex type with simple content checks the facets of its `xs:restriction`
  as a simple type's: bounds and digits were ignored (a 0–100 `xs:double`
  content accepted `101` and `NaN`), enumerations compared strings, a list's
  `length` counted characters, and XSD 1.1 `xs:assertion` facets were not
  evaluated. A `whiteSpace` facet of the restriction now normalizes the value
  before every facet of the type — the base type's length, pattern and
  enumeration included — and before its assertions (`$value`, `xs:assert`
  too), and the `[schema normalized value]` and the typed value are the
  normalized string. An element's `default` or `fixed` value must satisfy
  these facets as well.
- `xs:QName` and `xs:NOTATION` values are validated in their value space
  (Datatypes §3.3.18): `enumeration` compares expanded names — each enumerated
  value read with the namespace bindings in scope where it is written, an
  instance value with its element's — so `e:Sender` with `e` bound to the
  right namespace matches `tns:Sender`, and `tns:Sender` with `tns` bound
  elsewhere does not; `pattern` still constrains the lexical form. The same
  holds for lists and unions of them, `fixed` values, the typed values of a
  typed document, simple-content restrictions of complex types (which rejected
  every value), and casts and constructor functions for types derived from
  them. An XSD 1.1 assertion's `$value` is the QName value, and an element's
  typed value (`SchemaInfo::typed_value`) is the QName value too. Schema
  checks compare QName and NOTATION enumeration and fixed values by expanded
  name: a restriction may write its base's values with another prefix.
- A union type's `enumeration` compares values as the union defines them: each
  enumerated literal is the value of the first member type that accepts it,
  and values of different primitive types never match (Datatypes §4.3.5,
  §2.4.1.3). A QName no longer matches a literal that an earlier string member
  reads as a string, nor an integer a literal that an earlier token member
  reads as a token.
- `-0` (and `-00`, …) is a valid `xs:unsignedLong`, `unsignedInt`,
  `unsignedShort` and `unsignedByte` value with every driver, as a default,
  enumeration value or bound, and in casts to derived types; it is also
  accepted as the value of `length`, `minLength`, `maxLength`,
  `fractionDigits`, `minOccurs` and `maxOccurs`.
- A restriction may set `minExclusive` equal to `maxExclusive`; an exclusive
  bound may equal the base's of the same kind also when the base inherited it;
  every bound is checked against the base's bounds of the other kinds
  (`minExclusive="0" maxExclusive="0"` on `xs:unsignedLong` is an error).
- Date/time bounds that are incomparable are no longer reported as out of
  order; in XSD 1.0 the year `-0001` is 1 BCE, directly before `0001`.
- Duration bounds use the partial order of the four reference dateTimes:
  `maxInclusive="P1M"` no longer admits `P28D` – `P31D`, and
  `minInclusive="P1M"` with `maxInclusive="P30D"` compiles; durations whose
  four sums agree are equal (`P400Y` and `P146097D`).
- Durations, `xs:dayTimeDuration` and the date/time time line are compared
  exactly: a fraction of a second added to a large timestamp or duration no
  longer rounds away (`PT0.00000000000000000001S` is above `PT0S`), and the
  largest values no longer overflow (a panic) or compare unequal to
  themselves. The canonical form of a duration with very large fields no
  longer overflows.
- The `g*` types are equal on the time line for `fixed` values, identity
  constraints and (XSD 1.0) attribute-use value constraints; a union's
  `enumeration` compares its member values the same way.
- The `pattern` of the `g*` types constrains the lexical form, not the
  canonical one.
- Facet derivation compares values and checks the facets of every ancestor,
  `length` included; a `whiteSpace` value other than `preserve`, `replace` or
  `collapse` is a schema error; a simple-content restriction's facets are
  checked against its base content type (value space and applicability).
- A QName value whose prefix is bound nowhere is invalid on attributes
  admitted by a `strict` or `lax` attribute wildcard and on elements of a
  union type with a QName member — unless a later member of the union accepts
  the literal (`xs:QName xs:string`), which is then the value; declared
  attributes, list items and identity-constraint fields follow the same rule.
- `xs:untypedAtomic` had no validator ("No validator for type code
  UntypedAtomic"): every string is now a valid, unnormalized value. Without
  `ValidationFlags::XPATH_TYPES` an `xsi:type` naming it is `cvc-elt.4.1`.
- `xs:dayTimeDuration` and `xs:yearMonthDuration` accepted literals that are
  not durations (`PT`, `P1DT`, `P+1D`, `P+1Y`) and, for the constructor
  functions, the same.
- `xs:duration` refuses fields out of order (`P1M1Y`, `PT1M1H`), in schema
  validation and in XPath casts and constructor functions (Datatypes
  §3.3.6.2).

**Lists and unions**

- The typed value of an `xs:NOTATION`-typed node was an `xs:QName` value, and a
  member of a list of `xs:QName` was a string under the QName type code, so
  `eq` and `namespace-uri-from-QName` failed on it.
- The members of a list whose item type is a union are typed with the member
  type that validated each of them (XPath 2.0 §2.5.2) — in `fn:data` and in
  `$value` of an assertion on a simple type and on a complex type with simple
  content: `data(@a)[2] + 1` on `x 1` (a union of `xs:integer` and
  `xs:string`) is `2` (it raised `XPTY0004`), and `$value` of a complex type
  with simple content no longer types every member with the last member's type
  (`abc` was an `xs:decimal`). A member that the union's `xs:decimal` member
  type validated beyond `rust_decimal`'s capacity is an `xs:decimal` with the
  capacity warning, as any other such value.
- `xs:ENTITY` values inside a union value — a union with an `xs:ENTITIES` or
  `xs:ENTITY` member, also as the item type of a list — are checked against the
  document's unparsed entities (Structures §3.16.4); an undeclared entity was
  accepted. The `xs:ENTITY`, `xs:ID` and `xs:IDREF` members of a list of a
  union are recognized by their own member type: an integer member was checked
  as an entity name when the list's last member was an ENTITY, and an ENTITY
  member was not checked otherwise.
- An `xs:QName` or `xs:NOTATION` member of a list whose prefix is not declared
  is invalid (`cvc-datatype-valid.1.2.1`), as an atomic value already was — in
  a list of `xs:QName` with more than one member, in a list of a union with a
  QName member, and in any such list as element content, none of which was
  checked.
- A list type's `enumeration` compares the instance with each enumerated list
  member for member in value space (Datatypes §4.3.5.4, §2.4.1.2): `01 2`
  matches `enumeration="1 2"` on a list of `xs:integer`, and `1 x` matches
  `1.0 x` on a list of a union of `xs:decimal` and `xs:string`. `pattern` still
  constrains the lexical form. A union whose chosen member type is a list
  compares its enumeration the same way (an enumerated list never matched).
- Identity constraints compare list values member for member, each member in
  its own value space (Structures §3.11.4, Datatypes §2.2.1): a list of
  `xs:integer` `1 2` and a list of `xs:decimal` `1.0 2` are equal keys, and an
  integer member of a list of a union no longer equals the string `1` of a
  list of `xs:string`.

**Value constraints**

- `fixed` values compare "equal or identical" (Structures 1.1 cvc-elt
  5.2.2.2.2, Attribute Locally Valid (Use)) instead of structurally: `NaN`
  matches `NaN`, `0` matches `-0`, an `xs:dateTime` or `xs:time` on the same
  instant in another time zone matches, `PT60S` matches `PT1M`, and a union
  value matches whichever member type each spelling chose. A value with a time
  zone never matches one without, and values of different primitive types
  (`xs:float` against `xs:double`) never match. The restriction checks of a
  `fixed` element particle or attribute use compare the same way.
- The schema check of an attribute use against its declaration
  (`au-props-correct`) compares values instead of whitespace-collapsed
  strings: `1.0` against `1` is accepted, and `a  b` against `a b` for
  `xs:string` (which keeps whitespace) is an error. With XSD 1.1, whose
  clause 3 requires identity, `-0` against `0` and one instant in two time
  zones are errors.
- Identity constraints compare durations exactly, by months and seconds:
  `P1M` and `P30DT10H29M6S` are different keys (they compared equal through
  30.436875-day months), and a value of a type derived from `xs:duration`
  matches an `xs:duration` key with the same value.
- `SchemaInfo::is_default` is set when an empty element takes its `fixed`
  value, as for a `default`.
- An empty element that takes its declaration's `default` / `fixed` value
  under an `xsi:type` or a type alternative is validated with the value
  constraint's {lexical form} (Structures §3.3.4.3 clause 5.1.2), as under the
  declared type: a union whose accepting member collapses white space no
  longer hands the overriding type the value as written. Its
  `SchemaInfo::normalized_value` is the {lexical form}, also for
  `xs:anySimpleType`, and the typed document writes it.
- Clause 5.1.1 is checked: an empty element whose `xsi:type` has empty or
  element-only content, or reads the default as another value than the
  declaration's, is invalid (`cvc-elt.5.1.1`).
- An element with `xsi:nil="true"` whose declaration has a `fixed` value is
  invalid (`cvc-elt.3.2.2`, Structures §3.3.4.3 clause 3.2.3.2), and its [nil]
  property is false.
- Under an `xsi:type` or an XSD 1.1 type alternative, an element's content is
  compared by value with the declaration's fixed {value} (Structures 1.1
  §3.3.4.3 clause 5.2.2.2.2: equal or identical), not with the fixed literal
  read under the governing type. In XSD 1.0 mode a {value} of another
  primitive datatype, a string {value}, or a list against an atomic value
  matches through its canonical lexical representation (XSD 1.0 §3.3.4 clause
  5.2.2.2.2; Datatypes 1.0 §2.5.1 and §3.2.x.2, a dateTime in `Z`, the seconds
  as written) read with the element type's white space; no facet of the
  element's type is applied to that form.
- A QName or NOTATION `default` / `fixed` value whose prefix the declaration
  does not bind is a schema error (`e-props-correct.2`, `a-props-correct`), as
  it already was at validation.

**Attribute uses and wildcards**

- A restriction of a complex type that got its attribute wildcard by extension
  is no longer rejected as adding a wildcard (derivation-ok-restriction): the
  base's {attribute wildcard} is the one it has through extension (Structures
  §3.4.2.5), the one its instances are validated with.
- An explicit restriction of `xs:anyType`
  (`<xs:restriction base="xs:anyType">`) may declare an attribute wildcard
  with any `processContents`, as the shorthand `<xs:complexType>` can; it was
  rejected as adding a wildcard. An extension of `xs:anyType` has anyType's
  attribute wildcard (any namespace, `lax`, Structures §3.4.7): attributes it
  does not declare are validated laxly instead of rejected, and a restriction
  of it may keep a wildcard.
- `ValidationRuntime::get_default_attributes` reports every defaulted
  attribute (Structures §3.4.4.2): uses of local declarations, `fixed` as well
  as `default` value constraints, attribute-group uses, and uses inherited
  from base types — a restriction's redeclared or prohibited attributes
  excepted. It used to report only `default` values of global declarations
  used directly or through a group.
- A required attribute inherited from a base type is reported missing when an
  extension or an attribute group marks it `use="prohibited"` — a prohibition
  only stops a restriction's inheritance (Structures §3.4.2.4); such an
  attribute is also accepted on the element instead of being refused as
  prohibited. A `use="prohibited"` attribute beside an attribute group no
  longer hides the group's attribute of the same name.
- Default and fixed attribute values from attribute groups and base types take
  part in identity constraints and in ID/IDREF checks (Structures §3.11.4);
  under XSD 1.1 an inheritable defaulted attribute from a base type is
  inherited by descendant elements. A defaulted value is compared in its
  type's value space, as an attribute the instance carries (an integer default
  `1` and a string default `"1"` are different key values).
- An attribute reference's own `default` or `fixed` value in an attribute
  group wins over its declaration's (`get_default_attributes`, identity
  constraints, ID/IDREF, inherited attributes), as it already did for a
  reference in a complex type.
- Restrictions of complex types read the {attribute uses} as Structures
  §3.4.2.4 builds them, on both sides of the comparison. Newly accepted: a
  restriction whose attribute group marks a required base attribute
  `use="prohibited"` (the group's prohibition is no attribute use, so the
  attribute is inherited, still required); a restriction with its own
  `use="prohibited"` attribute beside an attribute group's required use of the
  same attribute. Newly refused: a restriction that weakens, retypes or
  changes the `fixed` value of an attribute its base still has — the base side
  is the base's complete {attribute uses}, so an extension's or an attribute
  group's "prohibition" further up the base chain no longer hides the base's
  use of it.
- XSD 1.1: an attribute reference without `inheritable` inherits its
  declaration's `{inheritable}`; an explicit `inheritable="false"` on such a
  reference is honoured, also by the restriction check (a restriction that
  turns an inherited `true` into `false` this way is refused; it was
  accepted).
- `get_expected_attributes` lists the attribute uses inherited from base
  types.
- Defaulted attributes whose types are derived from `xs:ID`, `xs:IDREF` or
  `xs:ENTITY` — by restriction, list or union — take part in the ID/IDREF and
  ENTITY checks.

**PSVI, nil and ID/IDREF**

- `fn:id` and the id index find an element whose typed value is a single
  `xs:ID` (a simple type or simple content), in typed documents, annotated
  copies and trees a host binds itself (XDM 1.0 §6.2.4, F&O 1.0 §15.5.2).
- In a typed document, whether a union-typed element or attribute is an ID or
  has the is-idrefs property is decided by the member type its value has with
  the node's in-scope namespaces, as `fn:data` gives it: a value of a QName
  member is no longer filed as an `xs:ID` or `xs:IDREF`.
- An element assessed laxly (matched by a lax wildcard, or the undeclared root
  of a `ValidationRoot::Lax` run, with no usable `xsi:type`) has `[validity]`
  notKnown, also when one of its attributes or descendants is invalid (which
  is still reported).
- An element is nilled only when its declaration is nillable: with no
  governing element declaration, or with a non-nillable one (where `xsi:nil`
  is still reported, `cvc-elt.3.1`), `xsi:nil="true"` neither suppresses the
  check of the content against the element's type nor makes content an error.
  In a typed document only a valid element is nilled.
- XSD 1.1: an `xsi:type` attribute that resolves to no type definition is
  invalid (Structures 1.1 §3.2.4.1 clause 5, `cvc-attribute.5`); the problem
  is still reported once, as `cvc-elt.4.1`, when the element event carries the
  `xsi:type`.
- In a typed document, an element no declaration or type governs is typed
  `xs:anyType` when one of its attributes or descendants was assessed (XDM 1.0
  §3.3.1.1), and stays untyped otherwise.
- XSD 1.1 type alternatives see all of the element's attributes, `xsi:type`,
  `xsi:nil` and the schema location hints included (Structures §3.12.4 clause
  1.1.2), and `xml:` attributes accepted under `ALLOW_XML_ATTRIBUTES`.

**Deep instance trees**

- `drive_navigator` and `drive_buffer_document` validate a tree of any depth:
  the walk is iterative, so an open element costs heap (the runtime's element
  stack, as in the streaming drivers), not stack. They recursed once per
  element level, and a tree about 4,800 levels deep overflowed a 2 MiB thread
  stack in an optimized build (about 2,700 in an unoptimized one) — an abort.
  A tree a host builds itself is bounded by no parse limit. The runtime sees
  the same calls in the same order, so diagnostics are unchanged.
- `fn:deep-equal` and `TreeComparer::deep_equal` / `deep_equal_iter` compare
  trees of any depth the same way (they overflowed a 2 MiB stack at about
  1,500 to 2,800 levels in an optimized build); the result and the order of
  the comparison are unchanged.

**Content models and schema documents**

- XSD 1.1: All Group Limited is enforced — an `xs:all` group (or a reference
  to one) as the content type's particle must have `maxOccurs` 0 or 1, and a
  reference to an all group may not appear in a sequence or choice
  (`cos-all-limited`). In both versions a reference with
  `minOccurs="0" maxOccurs="0"` is no component and places no all group
  anywhere (XSD 1.0 refused one inside a sequence or choice, and as the
  content type); in XSD 1.0 a type whose only particle has
  `minOccurs="0" maxOccurs="0"` has empty content, so white space is refused
  there too.
- An extension of a type without content by an `xs:all` group (inline or
  referenced) is validated as that all group: required members are required
  and each member occurs at most its `maxOccurs` (both versions). XSD 1.1: an
  extension of a base whose content is an empty `<xs:all/>` takes its own
  content (`<xs:group ref>` to an all group, an inline all group, a sequence),
  a mixed one keeps its empty sequence, and an `<xs:group ref>` to an all
  group extends an all-group base by merging (§3.4.2.3.3 clauses 4.2.1,
  4.2.3.2).
- XSD 1.1: `<xs:import>` of the enclosing schema document's own target
  namespace is an error (`src-import`, §4.2.6.2 clause 1.1).
- A simple-content restriction may restrict a built-in member type of the
  base's union content (Type Derivation OK (Simple) clause 2.2.4).
- `SchemaSetBuilder::compile_async` overflowed the stack on a cycle of
  includes.
- The content model of an extension chain was compiled in time exponential in
  the chain's length (25 levels took seconds); the base content model is
  compiled once.
- Recursion bounds of the derivation and compilation passes no longer cut
  legal schemas short (attribute inheritance stopped after 50 derivation
  levels, attribute groups after 20 levels, group references after about 50).
- "Unexpected element" errors in schema documents carry their location.
- The embedded schema for the XML namespace is the current W3C edition: it
  declares `xml:id` and allows an empty `xml:lang`. `assets/NOTICE.md` records
  its source and the W3C licence that applies to it.
- The embedded schema for the XLink namespace (`XLINK_XSD`,
  `embedded://xlink.xsd`) is byte-identical to the W3C original,
  `http://www.w3.org/XML/2008/06/xlink.xsd`: the shipped copy had LF line ends
  and three blank lines without their trailing space. Nothing it declares
  changes. `assets/NOTICE.md` records its source and the W3C licence that
  applies to it.
- An attribute that `xs:schema` does not allow (or an invalid value of one it
  does) is reported with its name and location instead of the internal error
  "Root frame did not produce a schema result".

### Performance

- Unique Particle Attribution: the epsilon closure of each content-model state
  reuses its working storage instead of building a hash set per state. Large
  WSDL schema sets compile up to 2.7 times faster (NetSuite 0.47 s → 0.24 s,
  0.36 s → 0.13 s under XSD 1.1; vSphere 66 → 53 ms; Workday Human Resources
  25 → 20 ms).
- Key, unique and keyref tables are indexed: validating a document with
  100,000 keys and 100,000 keyrefs takes under half a second instead of over
  five minutes.
- Each `xs:pattern` is compiled once: XSD 1.1 patterns were compiled two to
  eighteen times (a retry per unknown block name, then a second, anchored
  program), XSD 1.0 patterns twice. A first compile of a pattern-heavy XSD 1.1
  schema takes about a third less time.
- With the new engine each Unicode category set is built once per process
  instead of at every compile: compiling 400 distinct XSD 1.1 patterns took
  163 ms and takes 4.4 ms.
- `fn:translate` with a long `$mapString` is linear instead of
  `|$arg| × |$mapString|` (a map of a whole Unicode category took minutes).
- Validating list values is about 5–10 % faster: the members of a list of a
  union are no longer converted to strings and validated again.
- A path step with a named kind test (`element(c)`) compares names without
  allocating, about twice as fast; `instance of` / `treat as` resolve their
  type once per expression.
- A `BufferDocument` finds the namespaces in scope at an element without
  visiting its ancestors. Validating a deep document (`drive_buffer_document`,
  `drive_navigator`), serializing it, or reading its `namespace::` axis or
  `fn:in-scope-prefixes` took time quadratic in its depth, about 9 s each for
  100,000 levels. Where every element declares the default namespace again, as
  some generators write, it was cubic: 2,000 levels took 9.5 s. 100,000 levels
  now take 0.01–0.06 s. Every answer is unchanged. The document keeps 4 more
  bytes per namespace declaration and up to 16 per element that declares one.

## [0.2.3] - 2026-10-15

### Behaviour changes

This release removes no public item and changes no signature. It fixes panics
and validation defects — three of which accepted invalid instances — so some
results change. Those changes are listed here; the remaining fixes are under
*Fixed* and *Performance*.

- **Wildcards across namespaces.** `##other` and `##targetNamespace` in a
  wildcard now always refer to the target namespace of the schema document
  that declares the wildcard (Structures §3.10.2.2), also when the wildcard is
  inherited by an extension in another namespace or comes from a model group
  of another namespace. Such wildcards were read in the deriving type's
  namespace, so valid instances were rejected and **invalid instances were
  accepted**. Unique Particle Attribution (`cos-nonambig`) is now decided with
  each wildcard's own namespace too: schemas that were falsely rejected compile,
  and conflicts through a model group of another namespace — a wildcard and an
  element, or two wildcards — are now found (as Xerces-J); likewise the XSD 1.1
  wildcard type-table check (§3.8.6.3).
- **`##other` attribute wildcards (XSD 1.1).** `<xs:anyAttribute
  namespace="##other"/>` no longer admits unqualified attributes in XSD 1.1
  mode: `##other` excludes ·absent· as well as the target namespace (Structures
  §3.10.2.2), as it already did in XSD 1.0 and for `<xs:any>`. Such attributes
  were **accepted**. Attribute-wildcard intersections, extension unions and
  restriction checks use the corrected set, so restricting an `##other`
  attribute wildcard to `##local` or to `notNamespace="##targetNamespace"`, or
  adding an unqualified attribute the base wildcard does not allow, is now
  rejected (`derivation-ok-restriction`), and restricting
  `notNamespace="##targetNamespace ##local"` by `##other` is now accepted.
- **Empty extension content.** A `complexContent` extension whose explicit
  content is empty — `<xs:sequence/>`, `<xs:all/>`, `<xs:choice minOccurs="0"/>`
  or a model group with `maxOccurs="0"` — now has its base type's content type
  (Structures §3.4.2.3.3 clause 4.2.2), as with no particle at all. Such
  extensions were rejected; the compiled content model (and the
  `compiler::inspect` view) is now the base's.
- **`xs:decimal` beyond `rust_decimal`.** Every `xs:decimal` value is validated
  exactly, at any size and precision. A valid value that `rust_decimal::Decimal`
  cannot hold exactly (a magnitude above 79228162514264337593543950335, or more
  than 28 fraction digits) is no longer rejected
  ([#25](https://github.com/semyonc/xsd-schema/issues/25)) nor rounded: its typed
  value is its canonical representation (Datatypes §3.3.3.2: no `+`, no leading
  or trailing zeros, no decimal point for an integer), an `XmlAtomicValue::String`
  with the `xs:decimal` type code (`XmlValue::as_decimal` returns `None`), and
  the validator reports a warning with code `xs-decimal-capacity` (Datatypes
  §5.4). `XmlAtomicValue::Decimal` is unchanged. See
  [Implementation Limits](doc/IMPLEMENTATION_LIMITS.md).
- **Decimal facets on the exact value.** `totalDigits`, `fractionDigits` and the
  bound facets of `xs:decimal` and the integer types are decided on the exact
  value: values with more than 28 fraction digits are no longer checked after
  rounding (`fractionDigits="28"` accepted 29 fraction digits), bound literals
  larger or more precise than `rust_decimal` holds are enforced instead of
  rejected when the schema is compiled, integer values above
  79228162514264337593543950335 are checked against their facets (they were
  not), and `totalDigits` counts leading fraction zeros as Datatypes §4.3.11.3
  defines (`0.05` needs two digits). `enumeration` on these types compares
  values: `1.00` matches `1.0`, and `+1` and `01` match `1`.
- **XPath decimals.** Casting a string to `xs:decimal`, and a decimal literal,
  follows F&O §17.1.1: a value too large to be held raises `FOAR0002` (was
  `FORG0001`), a nonzero value whose magnitude is below 10⁻²⁸ underflows to `0`
  (F&O §6.2), and any other value with more digits than are held raises
  `FOCA0006` instead of being rounded. A decimal held as a string compares
  exactly and casts to `xs:string` (its canonical form, F&O §17.1.2),
  `xs:boolean`, `xs:integer` (truncation), `xs:double` and `xs:float` (nearest
  value); arithmetic on it raises `FOAR0002`.
- **XSD 1.1 assertions on such values.** An assertion (`xs:assert`,
  `xs:assertion`) whose evaluation fails only because it computed with an
  `xs:decimal` beyond `rust_decimal`'s capacity is not counted as false: its
  outcome is unknown, and the value carries the `xs-decimal-capacity` warning
  (Datatypes §5.4). Any other error still fails the assertion (Structures
  §3.13.4.1), also when such a value occurs elsewhere in the instance — an
  overflow of values that are held, a division by zero, a literal that cannot
  be cast.
- **All-groups (XSD 1.1).** An element that both an element particle and a
  wildcard of an `xs:all` group can take is now attributed to the element
  particle (Structures §3.8.4.1.3); it went to whichever came first in
  declaration order. A wildcard declared before the element, or inherited from
  the base type of an all-group extension, captured the element, so valid
  instances were rejected, and an element taken by a `processContents="skip"`
  wildcard was not validated against its declaration. Once the element
  particle's `maxOccurs` is used up, further matching elements go to the
  wildcard, as before. Verdicts change in both directions: for example
  `<xs:all><xs:any/><xs:element name="a" maxOccurs="2"/></xs:all>` now rejects
  `<a/><a/>` and accepts `<a/><x/><a/>`.
- **Conversion API.** `TypeConverter::to_decimal` (and `promote_to` with
  `xs:decimal`) of an `xs:untypedAtomic` follows F&O §17.1.1 like the XPath
  cast: a nonzero magnitude below 10⁻²⁸ gives `0` (it was rounded, sometimes
  to ±10⁻²⁸), a value too large gives `Err(ConversionError::Overflow)` (it was
  `InvalidValue`), and one more precise than `rust_decimal` holds gives
  `Overflow` too (it was rounded); a decimal held in lexical form gives
  `Overflow`;
  `TypeConverter::cast` of such a string to `xs:decimal` now succeeds, with the
  canonical form as the value; `XmlValue::as_double` returns the nearest double for a decimal
  held in lexical form; `xpath::error::QNAMED_ERROR_CODES` includes `FOCA0006`.
  `compiler::inspect` shows a wildcard used in another namespace's content
  model with its namespaces resolved (`Not([…])`, `List([…])`) rather than as
  `##other` or `##targetNamespace`.

### Fixed

- A schema document that fails to assemble (for example on a duplicate
  declaration, `sch-props-correct`) made the next document load panic with an
  index out of bounds. The error is now reported, and the failed document leaves
  nothing in the schema set — none of its components, name bindings, source map
  or deferred parsing errors. Before, its declarations could also replace
  earlier ones of the same name.
- Extending `xs:anyType` or a mixed type with empty explicit content was rejected
  with `cos-ct-extends`; see *Behaviour changes*. Under XSD 1.0 a
  `<xs:sequence/>` extension of a simple-content base is accepted as the base's
  content type, and `maxOccurs="0"` explicit content whose declarations would
  conflict with the base's no longer counts (Element Declarations Consistent
  applies only to components that exist, §3.8.2). XSD 1.1 still rejects complex
  content over a simple-content base.
- An XSD 1.1 extension inheriting the base's open content was rejected with
  `cos-ct-extends` when the base's wildcard used `##other` or
  `##targetNamespace` from another namespace.
- XPath `xs:decimal` arithmetic that overflowed `rust_decimal` (`+`, `-`, `*`,
  `div`, `idiv`, `mod`, and `xs:dayTimeDuration` multiplied or divided by a
  decimal) panicked; it now raises `FOAR0002` (`FODT0002` for the duration).
- `fn:round-half-to-even` on an `xs:decimal` with a precision of −19 or less
  overflowed `i64` computing its scale (a panic in debug builds); it is now
  exact, or `FOAR0002` when the result exceeds the range (5 × 10²⁸ with
  precision −29 rounds to the even multiple, 0). A decimal just above a
  midpoint is no longer rounded as the midpoint
  (`round-half-to-even(5.0000000000000000000000000001, -1)` gave 0; it gives
  10), and an `xs:integer` with a very large negative precision (such as
  −2147483647) no longer computes 10^k for minutes.
- An `xs:integer` beyond `rust_decimal`'s range compared with, or promoted to,
  `xs:decimal` raised an internal error; comparisons are now exact and
  arithmetic raises `FOAR0002`. `fn:distinct-values` and `fn:index-of` never
  found two such integers equal.
- XPath `div`, `idiv` and `mod` of an `xs:decimal` or `xs:integer` by zero
  raise `FOAR0001` (F&O §6.2.4–§6.2.6) also when the dividend is beyond
  `rust_decimal`'s range; they raised `FOAR0002`.
- An `xs:decimal` zero is never negative: `-0.0`, unary minus on a decimal
  zero, `fn:ceiling` of a decimal between −1 and 0,
  `fn:seconds-from-duration` of a negative duration without seconds, and a tiny
  negative `xs:float` or `xs:double` cast to `xs:decimal` (in XPath and through
  `TypeConverter`) gave `-0`; they give `0`, and a decimal zero always displays
  as `0`. `xs:float` and `xs:double` keep IEEE negative zero.
- Bound consistency (a lower bound not above an upper bound, Datatypes
  §4.3.7–§4.3.10) is decided in the type's value space: exactly for
  `xs:decimal` and the integer types, on float values for `xs:float`. An
  `xs:float` restriction whose bounds are the same float but different doubles
  (`0.1000000001` and `0.1`) was rejected with `cos-st-restricts`.
  `FacetSet::merge_with_base`, which does not know the type, compares the bounds
  as doubles as before.
- After `SchemaSet::source_maps.drop_all()`, every schema document parsed
  afterwards got document id 0, so whatever is looked up by document — the
  target namespace that a wildcard's `##other` and `##targetNamespace` refer
  to, an assertion's `xpathDefaultNamespace` — was read from the first
  document. A document's id is now its index in `SchemaSet::documents`.
- Identity-constraint fields, fixed and default values and union enumerations
  compare decimals by exact value, and a union with an `xs:decimal` member no
  longer validates a large decimal against a later member such as `xs:string`.
  An item of a list whose item type is a union reports the
  `xs-decimal-capacity` warning when its decimal member holds it.

### Performance

- Schema compilation, Unique Particle Attribution: the `cos-nonambig` check
  rebuilt the substitution-group map for every complex type and compared every
  pair of element particles reachable from each content-model state. It now
  builds the map once per schema set and finds competing particles through an
  index from expanded name (own and substitutable names) to the first particle
  carrying it. Verdicts and reported errors are unchanged — a differential test
  compares the check with the previous one on generated content models, and
  both W3C suites give the same results. Large WSDL schema sets compile one to
  two orders of magnitude faster (vSphere 10.4 s → 0.09 s, NetSuite 5.0 s →
  0.5 s, Workday Financial Management 2.1 s → 0.04 s), with unchanged peak
  memory. `check_upa` and `check_all_group_upa` are unchanged.

## [0.2.2] - 2026-10-10

### Behaviour changes

This release removes no public item and changes no signature. It fixes
validation defects, and validation is stricter as a result: some schemas and
instances that 0.2.1 accepted are now rejected. Each point is detailed under
*Changed* or *Fixed*.

- **Content models.** Children that an optional or repeated group forbids are
  now invalid — `(X, Y*)?, Z` accepted `<Y/><Z/>` although `Y` may only
  follow `X` — and so is empty content for a choice whose other branches have
  `maxOccurs="0"`. `NfaFragment::repeat_star` and `repeat_plus` now wrap the
  loop in a fresh entry and exit state, two states more; code that inspects a
  fragment's structure, rather than the language it accepts, sees that, and
  state numbers in `inspect_content_model` reports shift. In debug builds
  `NfaFragment::concat`, `alternate`, `optional` and `fragment_to_table` now
  assert that nothing inside a fragment re-enters its start state or leaves
  its end state, so a hand-built fragment that breaks this panics there.
- **Date and time lexical forms.** A timezone offset must match `timezoneFrag`
  exactly (two-digit hours and minutes, minutes below 60, at most `14:00`),
  month and day fields are exactly two digits, and nothing may follow a
  negative-year date. Such values are invalid, XPath casts from them raise
  `FORG0001`, and `castable as` returns `false` for them.
- **XSD 1.0 restriction.** An element declared directly in a restriction no
  longer restricts a base choice whose `minOccurs` is 2 or more. The other way
  round, an element restricting an optional choice through an optional branch
  — UBL 2.1's XAdES `XAdESTimeStampType` — is now accepted.

### Changed

- Under XSD 1.0, an element declared directly in a restriction no longer
  restricts a base choice whose `minOccurs` is 2 or more: `a{2,2}` against
  `choice{2,2}(a, b)` is now rejected. RecurseAsIfGroup fixes the implicit
  group's occurrence range at 1..1, which fails Occurrence Range OK against
  the base choice. That is the letter of the 1.0 Recommendation, unchanged by
  any erratum, and what Xerces-J and .NET implement; processors that accept it
  (XSV, Saxon) deliberately depart from 1.0 towards the 1.1 rule. A repeated
  group that particle normalization folds into the same range, such as
  `sequence{2,2}(a)`, is still checked as a group and accepted. Against a
  repeated choice with `minOccurs` 0 or 1 the previous, laxer result is kept,
  as W3C `particlesZ001`'s schema test expects. XSD 1.1 restriction is
  language subsumption (§3.4.6.4) and accepts all of these.

### Fixed

- Under XSD 1.0, `derivation-ok-restriction` rejected an element restricting
  an optional choice through a branch that is itself optional — for example
  `XAdESTimeStampType` in the XAdES 1.3.2 schema used by UBL 2.1, which
  restricts `choice{0,1}(Include{0,unbounded}, ReferenceInfo{1,unbounded})`
  to `Include{0,unbounded}`. A base choice was checked by folding its
  occurrence range into each branch, never by the XSD 1.0 §3.9.6 rule, and a guard
  that keeps this shortcut from accepting an optional element against a
  *required* branch (W3C `particlesHa161`, invalid for 1.0) tested the derived
  element's `minOccurs` rather than the branch it maps to. For a base choice,
  Particle Derivation OK (Elt:All/Choice/Sequence -- RecurseAsIfGroup) is now
  applied first: the
  element, wrapped in an implicit `choice{1,1}`, is checked with RecurseLax,
  and the guard and the folding fallback apply only when that fails. XSD 1.1
  already accepted the schema. Both W3C suites are unchanged (identical
  failure sets), and so is the GAEB DA XML 3.3 corpus.
- The timezone offset of the date/time types was parsed as two integers
  around a `:`, so malformed and out-of-range offsets were accepted —
  `+1:30`, `+01:60`, `+14:01`, `+15:00`, and `+01:-30`, which read as
  `+00:30` — and `09:15:00+15:00`
  was a valid `xs:time` with a 15-hour offset (found with UBL invoice
  fixtures). The offset must now match `timezoneFrag` (Datatypes §D.2.2)
  exactly: two-digit hours and minutes, minutes below 60, and at most `14:00`
  either way. In XPath, a cast from such a string — `cast as`, an `xs:*`
  constructor function, or a comparison that casts an untyped value — now
  raises `FORG0001`, and `castable as` returns `false`.
- The month and day fields of `xs:date`, `xs:dateTime`, `xs:gYearMonth`,
  `xs:gMonthDay`, `xs:gDay` and `xs:gMonth` were parsed as integers and only
  range-checked, so `2002-002-15`, `2002-2-15`, `---001` and `--+2` were
  accepted. They must now be exactly two digits (`monthFrag`, `dayFrag`).
  And with a negative year, `xs:date` and `xs:gYearMonth` ignored anything
  after the last field: `-2024-01-15-junk` was a valid date, and
  `-2024-01-15-15:00` was accepted with a `-15:00` offset. Such values are
  now invalid, and XPath casts from them raise `FORG0001`.
- Instance validation accepted children that an optional or repeated group
  forbids when the group starts or ends with an unbounded particle:
  `(X, Y*)?, Z` accepted `<Y/><Z/>` although `Y` may only follow `X`, and
  so did `(X, Y+)?`, `(Y*, X)?`, `(X, Y*){0,3}`, `(X, Y*)*` and
  `choice{0,1}(sequence(X, Y*))` (found with GAEB DA XML 3.3, where DA85
  `tgItem` accepted `QtySplit` without `Qty`). The content-model compiler drew
  the loop edge of `*` and `+` between the repeated particle's own boundary
  states, so the bypass of the enclosing optional group, or the link to its
  next particle, ran into or out of the loop. Loops now get their own entry
  and exit states. Present in every release since 0.1.0. Both W3C suites are unchanged
  (identical failure sets), and so is the GAEB corpus.
- A particle with `minOccurs="0" maxOccurs="0"` inside an `xs:choice` made the
  choice accept empty content: `choice(e1{0,0}, e2)` accepted no children
  although `e2` is required. Such a particle maps to no component at all
  (Structures §3.3.2, §3.7.2, §3.8.2, §3.10.2), and it is now left out of
  the choice; a choice left with no particles accepts nothing unless it is
  itself optional. It is also no longer a sibling for `notQName="##definedSibling"`.

## [0.2.1] - 2026-10-03

### Behaviour changes

This release removes no public item and changes no signature, but it fixes a
large number of XPath 2.0 conformance defects, and some expressions now give a
different, correct answer or raise an error where they used to return one.
This list names them by theme; each point is detailed under *Fixed*, or under
*Added* where it says so.

- **Validation outcome.** An element's PSVI `[validity]`, and so
  `DriveOutcome::root_validity` and the validity an end-element event reports,
  now reflects its children, attributes and identity constraints and, for the
  validation root, the ID/IDREF table. An instance with an invalid descendant
  — a bad value, a failing assertion, a duplicate key, a dangling IDREF and so
  on — used to report the error but leave the root `Valid`; the root is now
  `Invalid`. Errors inside a laxly assessed subtree still leave it as it was.
  Which errors are reported is unchanged.
- **Paths and axes.** `/` evaluates its right-hand operand once per node, so
  results come in document order without duplicates and `//t[1]` selects the
  first `t` child of each parent rather than the first `t` in the document. An
  abbreviated `attribute(…)` step uses the attribute axis; `following::` no
  longer skips subtrees; `preceding::` is delivered in reverse document order,
  and the root has none; a name test selects only nodes of its axis's
  principal node kind, and works on the `namespace::` axis; in a step, the
  name of `element(N)` / `attribute(N)` and the type of `element(N, T)` /
  `attribute(N, T)` are honoured and `schema-element(N)` /
  `schema-attribute(N)` are declaration-aware; `xmlns=""` is no longer exposed
  as a namespace node. A leading `/` or `//` raises `XPDY0050` when the root of
  the context node's tree is not a document node.
- **New static errors** — the expression no longer compiles: a QName used as
  an `AtomicType` that is not an atomic type (`XPST0051`); `xs:NOTATION` or
  `xs:anyAtomicType` as the target of `cast as` or `castable as` (`XPST0080`);
  a `TypeName` in `element(N, T)` / `attribute(N, T)` that is neither a
  built-in type nor a type of the static context's schema set (`XPST0008`),
  which without a schema set is any name outside the XML Schema namespace; an
  unbound prefix inside a kind test (`XPST0081`); a numeric literal whose
  exponent has no digits, such as `1e` (`XPST0003`).
- **New dynamic and type errors.** An operator applied to operand types XPath
  2.0 does not define raises `XPTY0004`, and a general comparison no longer
  reports `false` for such a pair; unary `+` on a non-numeric operand, which
  returned it unchanged, raises `XPTY0004` too. An operand or built-in
  function argument of more than one item where at most one is allowed, and an
  operand of `is`, `<<` or `>>` holding more than one node, raise `XPTY0004`
  instead of `XPDY0050`; `fn:name`, `fn:root` and the other functions with a
  `node()?` parameter raise `XPTY0004` for several nodes or an atomic value
  instead of using the first node or answering as for the empty sequence.
  `treat as` raises `XPDY0050` on every failure. `inf`, `Infinity` and `nan`
  are no longer numbers (`FORG0001` in a cast, `NaN` from `fn:number`).
  `fn:sum` and `fn:avg` of a number and a duration, or of the two duration
  types, raise `FORG0006`; `idiv` raises `FOAR0002` for a NaN operand or an
  infinite dividend. A `$collation` the host does not supply raises `FOCH0002`
  instead of being ignored; `fn:tokenize` and `fn:replace` report an invalid
  pattern, flags or replacement whatever the input; `fn:matches`,
  `fn:replace` and `fn:tokenize` reject `(?` and the `q` flag;
  `fn:resolve-uri` raises `FORG0002` for more unusable base URIs. In the other
  direction, an `xs:untypedAtomic` operand of `to` is cast to `xs:integer`
  instead of raising `XPTY0004`, and `() treat as empty-sequence()` succeeds.
- **Atomization.** A list-typed node atomizes to one item per member, through
  `compose::Value` as well; a namespace node to `xs:string`; an element of
  mixed complex content (including `xs:anyType`) to `xs:untypedAtomic`, and
  one of empty content to the empty sequence; a nilled element makes a value
  comparison or arithmetic return the empty sequence; built-in functions
  atomize their arguments instead of reading the string value.
- **Functions.** `fn:index-of` and `fn:distinct-values` compare with `eq`
  (`index-of(xs:token('a'), 'a')` is now `1`); `fn:tokenize` keeps
  zero-length tokens; `fn:sum` keeps `xs:integer`, and `sum((), ())` is the
  empty sequence; `fn:min` and `fn:max` accept every ordered type and return
  NaN in the numbers' common type; `fn:round-half-to-even` decides a tie on
  the exact value and no longer returns NaN for a large precision;
  `fn:namespace-uri-for-prefix` returns the empty sequence for an unbound
  prefix; `fn:id` follows F&O §15.5.2 (normalized values, XML-whitespace
  tokens, `xs:ID`-typed attributes, `FODC0001` outside a document);
  `fn:deep-equal` follows F&O §15.3.1; `fn:nilled` reads the PSVI property;
  `fn:base-uri` of namespace and parentless attribute nodes, the error codes
  of `fn:resolve-QName` and `fn:subsequence` with infinite or NaN arguments
  are corrected.
- **SequenceType matching.** `xs:untypedAtomic` is no longer an `xs:string`,
  and `xs:dayTimeDuration` and `xs:yearMonthDuration` are `xs:duration`s; the
  `TypeName` of `element(N, T)` / `attribute(N, T)` is matched in
  `instance of` and `treat as`, so an untyped node no longer matches an
  ordinary schema type.
- **Lexer.** String literals are no longer entity-decoded a second time;
  `for`, `some` and `every` are ordinary names where the grammar allows one;
  `p:*` and `5.` are accepted; `NCName` follows the XML 1.0 name productions.
- **XSD 1.1 validation.** In an assertion the data model instance is cut at
  the asserted element: `parent::`, `ancestor::`, `following::` and the
  sibling axes no longer reach outside it, and `fn:id` finds the elements
  inside it, where it used to find nothing. In the test of a type alternative,
  whose data model instance is rooted at the element itself, a leading `/` or
  `//` and `fn:id` now raise an error, so the test is false. Each can change
  which type is selected or whether an instance is valid.
- **Schema loading.** A relative schema location that starts with `..` keeps
  it, so a different file may be read.
- **XPath 1.0 compatibility.** `XPathMode::XPath10` applies XPath 1.0's
  first-item rule to arithmetic: `1 + /doc/a` over two `a` elements adds the
  first instead of raising `XPDY0050`.
- **Documents and ids.** `xml:id` is indexed in a document built through the
  push API and in a `Fragment` buffer too; `BufferDocument::get_element_by_id`
  normalizes its argument, ignores values that are not NCNames and also finds
  `xs:ID`-typed attributes; text parsing reports `DuplicateId` for values that
  are equal after normalization (`"a"` and `" a"`) and no longer for repeated
  values that are not NCNames; `BufferDocumentBuilder::register_xml_id`
  accepts a repeat for the same element, works in a `Fragment` buffer and
  panics on a node reference outside the document; `copy_attribute` keeps the
  annotation, or lack of one, of the attribute that replaces another.
- **Public helpers.** `xpath::functions::atomize_to_single_opt` and the
  `atomize_to_*` helpers built on it raise `XPTY0004` instead of `XPDY0050`
  for more than one item; `xpath::atomize::to_number` no longer accepts `inf`,
  `Infinity` or `nan`; `BufferDocNavigator::typed_value` returns
  `TypedValue::Untyped` for mixed and an empty value for empty complex content
  instead of `TypedValue::Absent`; `RoXmlNavigator::name` returns the
  qualified name; `XPathValue::as_slice` returns a one-element slice for a
  single item.

### Added

- Collations. The module `xpath::collation` with the trait `Collation`, the
  trait `CollationResolver` and the constant `CODEPOINT_COLLATION_URI`, plus
  `XPathContext::with_collation_resolver`,
  `XPathContext::with_default_collation` and
  `XPathContext::default_collation`. The Unicode codepoint collation is
  implemented here and is still the default; every other collation is supplied
  by the host through the resolver callback, so the crate takes no dependency
  for collation data. A `Collation` need only implement `compare`; an optional
  `equals` (by default `compare(a, b).is_eq()`) decides equality where that is
  cheaper than ordering, an optional `sort_key` lets the engine keep answering
  a general comparison with a hash index instead of comparing every pair, and
  an optional `find` (with the provided `starts_with` and `ends_with`)
  supplies the collation units that `fn:contains`, `fn:starts-with`,
  `fn:ends-with`, `fn:substring-before` and `fn:substring-after` are defined
  over — without it they raise `FOCH0004`.
  The `$collation` argument of `fn:compare`, `fn:contains`, `fn:starts-with`,
  `fn:ends-with`, `fn:substring-before`, `fn:substring-after`, `fn:index-of`,
  `fn:distinct-values`, `fn:deep-equal`, `fn:min` and `fn:max`, and the static
  context's default collation for the value and general comparisons (`eq`,
  `lt`, `=`, `<`, …, including `=` and `!=` between strings in XPath 1.0
  compatibility mode, which XPath 2.0 §3.5.2 evaluates with `eq` and `ne`),
  now go through it; `fn:default-collation()` returns the
  property rather than a constant. A relative collation URI is resolved
  against the static base URI (F&O §7.3.1). Nothing changes under the
  codepoint collation: it is answered without consulting the resolver, on the
  same code path and at the same speed as before. `XPathError::error_code`
  reports `FOCH0004` for the new error, which travels as an error QName;
  `XPathError::collation_no_units` builds it. A comparison operator asks the
  resolver for the default collation only when it actually compares two
  strings — never for an evaluation that compares only numbers, dates or other
  non-string values — and a function that takes a `$collation` argument asks
  when it is called; within one evaluation run the answer is memoised. The
  public `operators::general_eq_iter` and its siblings, which see only the
  static context, ask at most once per call.
- `XPathContext::with_xpath10_compatibility(bool)` and
  `XPathContext::xpath10_compatibility()`. This is the static-context property
  XPath 2.0 calls *XPath 1.0 compatibility mode*, and it is distinct from
  `XPathMode::XPath10`, which is a *language* mode whose lexer and parser
  reject XPath 2.0 syntax. The flag keeps the full 2.0 syntax and switches
  only the semantics 2.0 itself defines differently when the property is
  true: the 1.0 conversions in arithmetic (§3.4); the 1.0 rules in general
  comparisons — an operand compared with a single boolean is converted, as a
  whole, to its effective boolean value (§3.5.2), `<`, `<=`, `>` and `>=`
  compare numbers, and `=` and `!=` compare numbers when either side is a
  number and strings otherwise; and the first-item, `fn:string` and
  `fn:number` conversions of a function argument whose declared type is a
  single item (§3.1.5). The effective boolean value itself is not affected: a
  sequence of two or more atomic values is `FORG0006` in `and`, `or`, a
  predicate, `fn:boolean` or `fn:not` with the flag as without it (§2.4.3).
  Hosts embedding the XPath engine need this when they run expressions
  written for a 1.0-era host language while still accepting 2.0 syntax in the
  same document. `XPathMode::XPath10` implies the property.
- `BufferDocNavigator::new_orphan(doc, node)` — a navigator under which `node`
  has **no parent**: `move_to_parent` returns `false` there, `move_to_root`
  and `move_to_visible_root` land on it, and it has no siblings; its
  descendants are unaffected. A `BufferDocument` always has a document node at
  the root of its tree, but the XDM allows a parentless element, attribute,
  comment, processing instruction or text node, and a host that constructs
  such nodes has to build them somewhere. Under it `parent::`, `ancestor::`,
  the sibling axes, `following::` and `preceding::` stop at `node`,
  `fn:root()` lands on it, a leading `/` or `//` raises `XPDY0050` because the
  root is not a document node, and `find_element_by_id` searches the subtree
  of `node` only (`fn:id` itself raises `FODC0001` there, for the same
  reason).
  `new_assertion` cuts the same links at the asserted element, but keeps
  `fn:root()` and the absolute paths on the hidden document node. The cut
  applies to the node itself only: its attribute and namespace nodes keep it
  as their parent, and the parentless view travels with `move_to`. On the
  document node `new_orphan` is the ordinary navigator; it panics when `node`
  is not a node of `doc`.
- `BufferDocument::set_document_base_uri(uri)` and
  `BufferDocument::document_base_uri()` — the **document-level base URI**, the
  base URI a node reports once the walk up its `xml:base` ancestors reaches
  the document node without finding one. A parser is handed bytes and cannot
  know the URI a document was retrieved from, so a document built by
  `from_reader` or by a `BufferDocumentBuilder` still starts with none and
  `fn:base-uri` still falls back to the static base URI of the expression; a
  host that does know records it here, and `fn:base-uri` and
  `fn:document-uri` then report it, with any `xml:base` attribute on the node
  or an ancestor still taking precedence and being resolved against it.
- `BufferDocument::source_span(node_ref)` and
  `BufferDocument::has_source_spans()`. The per-node byte ranges a document
  records under `BufferDocumentOptions::track_source_locations` were only
  reachable from inside the crate; a host that parses a document and wants to
  report an error at a line and column of its own copy of the source text can
  now read them.
- `xpath::functions::special::error` — an implementation of `fn:error` for
  arities 0 to 3, which raises a dynamic error identified by the supplied
  QName, defaulting to `err:FOER0000`, and accepts and ignores
  `$error-object`. The four declared signatures are enforced by the
  implementation itself, because the engine does not check a registered
  function's declared parameter types at evaluation time and a host may
  register the function loosely: `$error` is one `xs:QName` in the
  one-argument form and `xs:QName?` in the other two, and `$description` is
  one `xs:string` — so `error(())`, `error((), 42)` and `error((), ())` are
  `XPTY0004`, while `error((), 'why')` raises `FOER0000`. It is **opt-in**: the
  built-in catalog does not contain it, so `error()` in an expression is still
  `XPST0017` until a host registers the function with `FunctionSet::register`,
  for which its documentation carries a worked example that declares one
  signature per arity.
- `XPathError::raised`, `XPathError::raised_error` and the `RaisedError` type,
  which carry a dynamic error identified by an arbitrary error QName — the one
  `fn:error` raises, and the spec-defined codes that have no variant of their
  own. `XPathError::error_code` resolves the codes listed in
  `QNAMED_ERROR_CODES` — `FOER0000`, `FOCH0004`, `FODC0001`, `FONS0004`,
  `FORG0002` and `XPST0080` — back to their `'static` string, and
  `XPathError::no_namespace_for_prefix` builds `FONS0004`. The namespace and
  the default local name are exposed as `XQT_ERRORS_NAMESPACE` and
  `DEFAULT_RAISED_ERROR`. `RaisedError` is `#[non_exhaustive]`: a host reads
  its fields but does not build one.
- `BufferDocument::has_type_annotations` — a constant-time, exact answer to
  "does any element or attribute of this document carry a schema type
  annotation?", maintained where bindings are attached instead of computed by
  walking the tree. It is `true` exactly when some node would report
  `DomNavigator::type_annotation() == Some(_)`, and follows the annotation
  mode of a copy.
- `BufferDocument::get_element_by_id_in_tree(node, id)`, the id lookup scoped
  to the tree containing `node`, answering with the first such element in
  document order. A `DocumentKind::Full` document is one tree — every
  top-level node, a comment or processing instruction before the document
  element included, sits under its document node — so there it answers as
  `get_element_by_id` does; in a `DocumentKind::Fragment` buffer each child of
  the document node is a tree of its own. Passing the document node searches
  the whole buffer, and a `node` that is not a node of the document answers
  `None`.
- An attribute whose typed value is a single `xs:ID` is now an is-id node,
  found by `fn:id` and `BufferDocument::get_element_by_id`, not only `xml:id`
  (XDM §6.3.4): its type is `xs:ID` or
  derived from it, or a union whose value is of such a member, or a list of
  `xs:ID` of length one — and the value is valid. That holds wherever the
  attribute is annotated: in a document built by `build_typed_document`
  (including a type chosen by XSD 1.1 conditional type assignment), in a
  copy made with `Annotations::Preserve`, and wherever
  `BufferDocumentBuilder::set_node_binding` binds an attribute. Binding it
  again replaces what the earlier binding filed.
- Five small public helpers that come with the path, node-test and URI fixes
  below:
  - `xpath::ast::default_forward_axis` — the axis an abbreviated forward step
    runs on: `attribute` for an `attribute(…)` or `schema-attribute(…)` test,
    `child` otherwise (XPath 2.0 §3.2.4);
  - `xpath::ast::PathStepNode::abbrev_forward` — builds an abbreviated
    forward step on that axis;
  - `xpath::node_test::principal_node_kind` — the principal node kind of an
    axis: attribute for `attribute::`, namespace for `namespace::`, element
    for every other axis (§3.2.1.1);
  - `xpath::node_test::name_test_for_principal_kind` — the runtime node test
    for a name test that selects only nodes of that principal kind
    (§3.2.1.2);
  - `XPathError::invalid_uri_argument` — builds `FORG0002` for an argument
    that is not a valid URI reference, carried as an error QName.

### Fixed

- An element's `[validity]` now reflects its children, its attributes, its
  identity constraints and — for the validation root — the ID/IDREF table, as
  Structures §3.3.5.1 requires ("Neither its [children] nor its [attributes]
  contains an information item … whose [validity] is invalid"). Before, an
  error anywhere below the root was reported but `DriveOutcome::root_validity`
  and every ancestor's end-element validity stayed `Valid`: an invalid child
  value or attribute, a failing `xs:assert` or assertion facet on a nested
  element, a type selected by a type alternative, a duplicate key or an
  unmatched keyref (even one declared on the root), a duplicate ID or an
  unresolved IDREF, text in element-only or empty content, content in a nilled
  element, an abstract element declaration, an invalid `xsi:` attribute value,
  an undeclared ENTITY value. Each now makes the element it judges invalid, and
  an invalid element makes its parent invalid. An element that is only laxly
  assessed (no declaration and no type, e.g. matched by a `lax` wildcard)
  stays `notKnown` and does not pass invalidity on, so errors inside such a
  subtree are still reported while the root can remain `Valid`. Which errors
  are reported is unchanged.
- Relative schema locations keep a leading `..`. Path normalization let every
  `..` remove the component before it, and on a relative path with nothing
  left to remove the `..` was silently dropped, so
  `SchemaSetBuilder::add("", "../xsd/a.xsd")` read `xsd/a.xsd` below the
  current directory, and an `include`, `import`, `redefine` or `override` with
  `schemaLocation="../../common.xsd"` resolved against the relative base URI
  `schemas/a.xsd` read `common.xsd` instead of `../common.xsd`. A `..` now
  cancels only a preceding directory name; it is kept at the start of a
  relative path and, as before, dropped at the root of an absolute one.
- `fn:contains`, `fn:starts-with`, `fn:ends-with`, `fn:substring-before` and
  `fn:substring-after` no longer ignore their `$collation` argument. They
  atomized it and dropped it, so a call naming any collation at all was
  answered under the codepoint collation and a collation the implementation
  does not support went unreported. They now use the collation, and a URI the
  implementation does not support raises `FOCH0002` as F&O §7.3.1 requires.
  `fn:index-of`, `fn:distinct-values`, `fn:min` and `fn:max` ignored theirs in
  the same way and likewise use it now — `fn:min` and `fn:max` only for
  `xs:string` items, since F&O §15.4.3 ignores the collation for every other
  type.
- A name test on the `namespace::` axis now selects namespace nodes. The
  principal node kind of that axis is namespace, and a namespace node's name is
  its prefix, in no namespace — but the name-test matcher rejected every node
  that was not an element or an attribute, so `namespace::*` and
  `namespace::p` always returned the empty sequence.
- The lexer now accepts `NCName ":" "*"`, the second alternative of the
  Wildcard production. `*:NCName` already lexed and the grammar already had the
  production, but `p:*` failed with a lexer error in every position.
- `TimSort::merge_hi` no longer panics in a debug build. The algorithm walks
  its cursors backwards and deliberately lets two of them step one position
  past the front of the array, which is the reference implementation's `-1`
  sentinel; with `usize` indices that is wrapping arithmetic, and a plain
  subtraction panicked under `cargo test` without `--release` while the
  release build was already correct. Sorting a node sequence in a debug build
  could hit it.
- `for`, `some` and `every` are no longer treated as reserved words. They are
  keywords only when a variable follows, so `@for`, `some:a` and an element
  named `every` now lex as ordinary names instead of failing to parse.
- The operands of the `to` operator now follow the function conversion rules
  (XPath 2.0 §3.3.1): an `xs:untypedAtomic` operand is cast to `xs:integer`
  instead of raising `XPTY0004`, which matters for a range whose bound comes
  from an untyped node.
- An abbreviated forward step whose node test is an `attribute(...)` or
  `schema-attribute(...)` test now runs on the **attribute** axis. XPath 2.0
  §3.2.4: "If the axis name is omitted from an axis step, the default axis is
  `child` unless the axis step contains an AttributeTest or
  SchemaAttributeTest; in that case, the default axis is `attribute`." Such a
  step was given the child axis, so `/doc/attribute()` and `//attribute()`
  always selected nothing.
- The name of an `element(N)` or `attribute(N)` test used as a step's node
  test is now matched instead of ignored, so `child::element(p)` selects only
  `p` children. Previously the name was parsed and dropped and the test
  behaved as bare `element()` / `attribute()`. (Reachable for attribute names
  only since the previous entry.)
- A name test now selects only nodes of the **principal node kind** of its
  axis (XPath 2.0 §3.2.1.1, §3.2.1.2: "A name test is true if and only if the
  kind of the node is the principal node kind for the step axis and the
  expanded QName of the node is equal ... to the expanded QName specified by
  the name test"). `self::*` and `ancestor-or-self::*` used to select
  attribute nodes.
- An unprefixed QName in a step's name test picks up the default
  element/type namespace only on an axis whose principal node kind is element
  (§3.2.1.2); on the `attribute::` and `namespace::` axes it is in no
  namespace. Binding already applied this rule, but the unbound fallback path
  did not.
- The `following::` axis no longer skips the subtrees of the nodes it
  reaches. Only the *context node's* descendants are off the axis
  (§3.2.1.1), so the subtree escape now happens once, at the start, and the
  walk continues in plain document order. From an attribute or namespace
  node the axis now starts at the owner element's first child, which is the
  first node after the context node in document order.
- The `preceding::` axis is now delivered in **reverse document order**, the
  order a positional predicate's focus counts in on a reverse axis, so
  `preceding::x[1]` selects the nearest preceding `x` rather than the
  furthest. The sequence returned by a path expression is still in document
  order.
- The `preceding::` axis of the root of a tree is now the empty sequence
  instead of the whole tree: every node of the tree is a descendant of the
  root and therefore follows it in document order.
- A namespace **undeclaration** (`xmlns=""`, or `xmlns:p=""` under Namespaces
  1.1) is no longer exposed as a namespace node with a zero-length URI. It is
  the absence of a binding, so it disappears from the `namespace::` axis and
  from `fn:in-scope-prefixes` while still shadowing the outer binding for
  that prefix. The declaration remains visible to serialization, to shallow
  copy, and to the namespace context that resolves QNames during validation,
  which all model declarations rather than namespace nodes.
- The `/` operator now evaluates its right-hand operand once per node of its
  left-hand operand, with the inner focus XPath 2.0 §3.2 and §2.1.2 prescribe.
  The whole operation was applied to the concatenated left-hand sequence, with
  three consequences, all fixed: a step's predicates saw that concatenation
  instead of the sequence the step produced for one context node, so
  `a/b[last()]` returned only the last `b` of the last `a` and `a/b[1]` only
  the first `b` of the first `a`; `fn:position()` and `fn:last()` in a
  non-predicate right-hand operand reported 1 instead of the item's position in
  the left-hand sequence and that sequence's size; and the per-operation
  "returned in document order" / "duplicate nodes are eliminated" rules were
  applied at most once, at the end of the whole path, so
  `(item[5],item[3])/@val` came back in the order written and
  `(a,a)/b` returned every `b` twice. A right-hand operand whose evaluations
  return both a node and an atomic value now raises `XPTY0018`, and a
  non-node left-hand operand raises `XPTY0019` for every kind of right-hand
  operand rather than only for axis steps. Note that `//x[1]` is, as the
  specification defines it, the first `x` child of *each* node — not
  `(//x)[1]`.
- A leading `/` or `//` raises `XPDY0050` when the root of the tree that
  contains the context node is not a document node, as XPath 2.0 §3.2
  requires: "At evaluation time, if the root node above the context node is
  not a document node, a dynamic error is raised [err:XPDY0050]". That is the
  case under `BufferDocNavigator::new_orphan` for any node but the document
  node, and in the test of an XSD 1.1 type alternative, whose data model
  instance is rooted at the element itself (Structures §3.12.4). There `/`
  used to select the element, so `/@kind = 'a'` could select a type; a test
  that uses a leading `/` or `//` is now false, which is how §3.12.4 treats a
  dynamic error. Assertion navigators are unaffected: their root is the
  document node.
- An operator applied to an operand combination that XPath 2.0 Appendix B.2
  does not define now reports the specification's type error code. §3.4 and
  §3.5.1 both say "a type error is raised [err:XPTY0004]", but
  `XPathError::error_code()` returned `None` for
  `BinaryOperatorNotDefined`/`UnaryOperatorNotDefined`, and the arithmetic path
  raised an internal error instead of either, so `3 + '2'`,
  `xs:gYear("2000") gt xs:gYear("2001")` and `xs:date("2000-01-01") * 2`
  surfaced without a code. The two error variants and their `Display` text now
  carry `XPTY0004`.
- Unary `+` is `op:numeric-unary-plus`: an `xs:untypedAtomic` operand is cast
  to `xs:double` (`FORG0001` when it cannot be), a non-numeric operand is
  `XPTY0004`, and the result is an `xs:integer`, `xs:decimal`, `xs:float` or
  `xs:double` (XPath 2.0 §3.4, F&O §6.2.7). It returned its operand unchanged,
  so `+'3'` was the string `'3'`.
- A general comparison no longer reports `false` (or, for `!=`, `true`) when a
  pair of operands cannot be compared. `'001' = 1` and `'001' != 1` both raise
  `XPTY0004`: §3.5.2 defers each pair to the corresponding value comparison and
  §3.5.1 makes an operand combination outside B.2 a type error. The
  specification's freedom to "return true as soon as it finds an item in the
  first operand and an item in the second operand that have the required
  magnitude relationship" is kept — the type error is only reported when no pair
  compares true, so `(1,'a') = 1` is still `true`.
- An operand of an arithmetic operator, a value comparison, `to` or `cast as`
  that atomizes to more than one item now raises `XPTY0004` instead of the
  generic "more than one item" dynamic error. All four sections state the same
  rule: "If the atomized operand is a sequence of length greater than one, a
  type error is raised [err:XPTY0004]" (§3.4, §3.5.1, §3.3.1 via the function
  conversion rules, §3.10.2). `(2,3,4) eq (2,3)` reported `XPDY0050`.
- A built-in function argument of more than one item where the parameter
  allows at most one is `XPTY0004` (XPath 2.0 §3.1.5), and so is an operand of
  `is`, `<<` or `>>` holding more than one node (§3.5.3): "Each operand must
  be either a single node or an empty sequence; otherwise a type error is
  raised [err:XPTY0004]". Both reported `XPDY0050`, which the evaluator now
  raises only for `treat as` and a leading `/` or `//`: `string(('a','b'))`
  and `upper-case(('a','b'))` are `XPTY0004`. `fn:name`, `fn:local-name`,
  `fn:namespace-uri`, `fn:node-name`, `fn:nilled`, `fn:base-uri`,
  `fn:document-uri` and `fn:root` raise `XPTY0004` for more than one node, for
  an atomic argument and, called without an argument, for an atomic context
  item, where they used the first node or answered as for the empty sequence:
  `name(/doc/a)` over two `a` elements returned `a`, and `local-name(1)` the
  empty string. The public helper `xpath::functions::atomize_to_single_opt`,
  and the `atomize_to_*` helpers built on it, report `XPTY0004` for more than
  one item as well.
- `treat as` now raises `XPDY0050` on every failure path, not `XPTY0004`.
  §3.10.5: "If `expr1` matches `type1`, using the rules for SequenceType
  matching, the `treat` expression returns the value of `expr1`; otherwise, it
  raises a dynamic error [err:XPDY0050]" — which covers the wrong cardinality
  as much as the wrong item type. `() treat as empty-sequence()` also stopped
  failing: `empty-sequence()` carries no occurrence indicator of its own, so it
  is no longer run through the cardinality check first.
- A QName used as an `AtomicType` that does not name an atomic type in the
  in-scope schema types is now the static error `XPST0051` (§2.5.4.2, §3.10.2,
  §3.10.3) instead of silently yielding `false` from `instance of` and
  `castable as`, or a run-time type error from `cast as`. This covers an unknown
  name (`instance of nosuchtype`), an unprefixed name with no default
  element/type namespace in scope (`castable as double`), the non-atomic
  built-ins the specification's own note calls out (`xs:IDREFS`, `xs:NMTOKENS`,
  `xs:ENTITIES`, `xs:anyType`, `xs:anySimpleType`, `xs:error`), and a name that
  a schema set in the static context does not define as a simple type.
  `xs:anyAtomicType` and `xs:NOTATION` are atomic types and remain accepted in
  `instance of` and `treat as`; as the target of `cast as` or `castable as`
  they are the static error `XPST0080` (§3.10.2, §3.10.3), which
  `XPathError::error_code` reports for an error carried as an error QName —
  `'a' castable as xs:NOTATION` was `true`, and `1 cast as xs:NOTATION` failed
  at run time with `FORG0001`. A user-defined simple type of the schema set
  passes the `XPST0051` check but is not yet supported as an `AtomicType`: as
  in 0.2.0, `instance of` and `castable as` answer `false` for it, `treat as`
  fails and `cast as` raises `XPST0051` when it is evaluated, and a list or
  union type is not rejected at compile time.
- The `TypeName` of an `element(N, T)` or `attribute(N, T)` test is now matched
  instead of ignored — in `instance of` and `treat as`, and as the node test of
  a path step, inside `document-node(…)` too — so
  `/doc instance of element(*, xs:integer)` is no longer `true` for a document
  that was never validated, and `/doc/element(*, xs:integer)` no longer selects
  every child. §2.5.4.3 and §2.5.4.5 require
  `derives-from(AT, TypeName)`, `AT` being the node's type annotation; an
  untyped element's annotation is `xs:untyped` and an untyped attribute's is
  `xs:untypedAtomic`, which derive from none of the ordinary schema types (but
  do derive from `xs:anyType`, and the latter also from `xs:anySimpleType` and
  `xs:anyAtomicType`). For an annotated node the comparison uses the schema
  set's derivation relation, so a user-defined type name works as well.
  `element(N, T)` now also requires the node's nilled property to be false,
  which only `element(N, T?)` relaxes. A step applies the whole test before its
  predicates, so `element(*, T)[1]` is the first child that matches it. A
  `TypeName` that is not in the in-scope schema types — the built-in types of
  the XML Schema namespace, `xs:untyped` included, and the named types of the
  static context's schema set — is the static error `XPST0008` (§2.5.4.3,
  §2.5.4.5); it used to be accepted and ignored, so without a schema set a type
  name outside the XML Schema namespace now fails to compile.
- In XPath 1.0 compatibility mode — `XPathContext::with_xpath10_compatibility`
  (see *Added*), or `XPathMode::XPath10`, which implies it — the conversions
  XPath 2.0 adds in that mode are applied. §3.1.5: if an
  argument **is not of the expected type**, and that type is a single or optional
  single item, the argument is replaced by its first item, an argument whose
  expected type is `xs:string`/`xs:string?` by `fn:string` of it, and one whose
  expected type is `xs:double`/`xs:double?` (the declared type of every `numeric`
  parameter) by `fn:number` of it — so `round(concat('20','.7'))` is `21` instead
  of a type error, while `compare((), '')` and `round(())`, whose argument
  already is of the expected type, keep returning the empty sequence. The
  condition is about the argument's *static* type, which §2.2.3.1 leaves
  implementation-dependent without the Static Typing Feature: an argument that
  evaluates to the empty sequence counts as being of an optional expected type
  only when the argument expression is the literal `()`, the one expression
  §2.3.4 allows the static type `empty-sequence()`, so a path expression that
  selects no nodes is still converted and `round(doc/none)` is `NaN`. §3.4: an
  arithmetic
  operand is likewise reduced to its first item and converted with `fn:number`,
  and an empty operand makes the whole expression `NaN` rather than the empty
  sequence — so `1 + (6 to 10)` is `7` and `1 + ()` is `NaN`. The same first-item
  rule applies to the operands of `to` (§3.3.1), whose expected type is
  `xs:integer?`. The date, time and duration types are outside §3.4's conversion
  list, so arithmetic over them keeps the XPath 2.0 operator mapping in
  compatibility mode, and `idiv`, which XPath 1.0 does not have, keeps its
  integer result. None of this changes anything when the flag is off and the
  language mode is XPath 2.0. In `XPathMode::XPath10`, `1 + /doc/a` over two
  `a` elements therefore adds the first one, where it raised `XPDY0050`.
- `fn:matches`, `fn:replace` and `fn:tokenize` now reject regular-expression
  syntax that XPath 2.0 does not define. The backing engine implements a later
  dialect, which added the `(?…)` group forms and the `q` flag; an XPath 2.0
  `(` always opens a capturing group and can never be followed by `?`, and the
  only defined flags are `s`, `m`, `i` and `x`. `(?:a)` now raises `FORX0002`
  and `q` raises `FORX0001`. An escaped `\(?` and a `?` inside a character
  class, including a subtracted one such as `[a-z-[(?]]`, are unaffected, and
  so are `xs:pattern` facets, which compile through a separate path. With the
  `x` flag the check reads the pattern after whitespace has been removed, so
  `\ (?` is the escaped `\(?` and is accepted, as F&O §7.6.1.1 requires.
- `fn:tokenize` no longer drops zero-length tokens. Every gap between two
  matches of the separator is a token, so a leading separator produces an empty
  first token, a trailing separator an empty last token, and two adjacent
  separators an empty token between them: `tokenize("a,,b", ",")` now returns
  three tokens and `tokenize(",a,", ",")` returns `("", "a", "")`.
- String literals are no longer re-decoded by the lexer. XPath 2.0 §3.1.1 makes
  a literal's value the characters between the delimiters, a doubled delimiter
  standing for one; XML un-escaping belongs to the host language and has already
  happened by the time the expression text arrives. The lexer used to expand XML
  entity and character references a second time and fold CR/CRLF to LF, so
  `concat('a&b','!')` failed to compile, `string-length('&#13;')` was 1 instead
  of 5, and a literal carriage return became a line feed.
- A `DecimalLiteral` may now end with its period. XPath 2.0 §A.2.1 gives
  `DecimalLiteral ::= ("." Digits) | (Digits "." [0-9]*)`, so `5.` is a valid
  literal; it previously failed to lex.
- A numeric literal whose exponent has no digits — `1e`, `5.0e`, `1e+` — is a
  syntax error (`XPST0003`), as the `DoubleLiteral` production of XPath 2.0
  §A.2.1 requires; it lexed as a double literal and evaluated to NaN.
- The lexer's `NCName` predicates now implement the XML 1.0 §2.3
  `NameStartChar` / `NameChar` productions (minus `":"`) instead of Rust's
  Unicode `Alphabetic` / `Alphanumeric` properties, which admitted characters
  such as U+00B5 MICRO SIGN that the productions exclude.
- Only the XML Schema lexical forms convert to `xs:double` and `xs:float`. A
  string or `xs:untypedAtomic` value was converted with Rust's own float
  parser — in `cast as`, `castable as` and the constructor functions, in
  arithmetic and comparisons, in `fn:number`, `fn:sum`, `fn:avg`, `fn:min` and
  `fn:max` — which also accepts `inf`, `infinity` and `nan` in any case and
  trims Unicode whitespace. Those spellings are now not numbers (XPath 2.0
  Appendix I.1): `xs:double('inf')` raises `FORG0001`, as does an
  `xs:untypedAtomic` `inf` in arithmetic, a comparison or an aggregate,
  `'inf' castable as xs:double` is `false`, and `number('Infinity')` is `NaN`;
  and only XML whitespace around a number is ignored. `INF`, `+INF`, `-INF`
  and `NaN` are unchanged. `xpath::atomize::to_number`, the public helper
  behind `fn:number`, follows the same rule.
- SequenceType matching now follows the whole built-in atomic type hierarchy
  instead of a hand-written table covering only the string and numeric
  branches. Two results change: `xs:untypedAtomic` is no longer an `xs:string`
  (it derives from `xs:anyAtomicType`), and `xs:dayTimeDuration` and
  `xs:yearMonthDuration` now are `xs:duration`s. The other branches —
  `xs:dateTimeStamp` under `xs:dateTime`, the `xs:ID`/`xs:IDREF`/`xs:ENTITY`
  leaves of the string branch — are now modelled as well.
- `fn:min` and `fn:max` accept every type that has an ordering, not only the
  numeric and string ones: `xs:date`, `xs:time`, `xs:dateTime`, `xs:boolean`
  and the two ordered duration types used to raise `FORG0006`. A sequence that
  mixes primitive types still raises `FORG0006`, which it did not always do
  before. NaN is returned as a value of the numbers' least common type, so
  `max((xs:float('NaN'), 1))` is an `xs:float`; it was always an `xs:double`.
- `fn:index-of` and `fn:distinct-values` compare with the `eq` operator, as
  F&O §15.1.5 and §15.1.6 require. They compared the type as well as the
  value, so an `xs:token`, an `xs:ID` or an `xs:NMTOKEN` never equalled the
  `xs:string` it spells: `index-of(xs:token('a'), 'a')` was the empty
  sequence, and so was F&O's own example, `index-of(@a, "blue")` over an
  `xs:NMTOKENS` attribute `red green blue`. Dates and times now also compare
  on the timeline, durations by value across their subtypes (`P12M` and `P1Y`
  are one distinct value) and QNames by expanded name; a pair `eq` cannot
  compare is still distinct, and `fn:distinct-values` still treats NaN as
  equal to NaN.
- `fn:sum` no longer widens integers: it accumulates with `op:numeric-add`, so
  `sum((1, 2, 3))` is an `xs:integer` rather than an `xs:decimal`. `fn:avg`
  keeps returning an `xs:decimal` for an integer input, because dividing two
  integers yields one.
- `fn:sum` and `fn:avg` raise `FORG0006` for a mixture of individually
  summable types — a number and a duration, or an `xs:yearMonthDuration` and
  an `xs:dayTimeDuration` — as F&O §15.4.5 and §15.4.2 require. Such a sum
  failed with an error that carried no code.
- `fn:sum`'s `$zero` argument is honoured when it is itself the empty sequence:
  `sum((), ())` is now the empty sequence instead of the integer 0. Omitting
  the argument still gives 0.
- `fn:round-half-to-even` decides a tie on the exact value of its argument
  rather than on a scaled binary product, and a zero result keeps the sign of
  the argument. `round-half-to-even(250.0250e0, 2)` is now `250.03`,
  `round-half-to-even(xs:float(150.0150e0), 2)` is `150.01`, and
  `round-half-to-even(-3.0e0, -2)` is `-0`. A large precision no longer gives
  NaN: `round-half-to-even(1234.5678e0, 400)` and
  `round-half-to-even(5e-324, 400)` return their argument.
- `fn:subsequence` now evaluates its two position comparisons in `xs:double`
  arithmetic as the specification defines them, so infinite and NaN arguments
  behave: `subsequence(1 to 20, -INF, INF)` is the empty sequence, because the
  two bounds sum to NaN.
- `idiv` on `xs:float` and `xs:double` operands raises `FOAR0002`, not
  `FOAR0001`, when an operand is NaN or the dividend is infinite (F&O
  §6.2.5); `FOAR0001` remains the code for a zero divisor. A quotient beyond
  the 64-bit range is now exact — `1e20 idiv 1` is `100000000000000000000` —
  where it saturated at `9223372036854775807`.
- `fn:nilled` reports the post-schema-validation property instead of looking
  for an `xsi:nil` attribute, so an element that was never validated is not
  nilled.
- `fn:resolve-QName` reports the specified error codes: `FOCA0002` for a value
  that is not a lexical QName (it raised `FORG0001`, the code for a failed
  `cast as xs:QName`) and `FONS0004` for a prefix the element does not bind (it
  raised the static error `XPST0081`).
- `fn:base-uri` follows the XDM accessor rules for the remaining node kinds: a
  namespace node has no base URI, and an attribute node has its parent
  element's or none at all when it has no parent. A namespace node used to
  report its owner element's base URI, and a parentless attribute the
  document's.
- `fn:resolve-uri` validates both arguments and reports `FORG0002` for one that
  is not a valid URI reference — including a base URI that is not absolute —
  keeping `FORG0009` for a resolution that fails. The validation also rejects
  more than one `#` and the characters no URI production allows, which it let
  through before. A base URI that has a fragment identifier or is not
  hierarchic (`mailto:…`, `urn:…`) is `FORG0002` as well, and so is a
  zero-length base when `$relative` is relative, which raised `FORG0009`
  (F&O §8.1).
- `RoXmlNavigator::name` returns the qualified name as the document writes it,
  for elements and for attributes. It returned the local name, dropping the
  prefix, which also affected `fn:name` and serialization through that
  navigator. The name is read back out of the document's source text, and the
  scan ends at XML's whitespace — the four characters of XML 1.0 §2.3 production
  `S` — and not at Unicode's, so a name containing U+1680 OGHAM SPACE MARK or
  another character that is a legal XML `NameChar` in `[#x37F-#x1FFF]` is
  returned whole.
- `fn:deep-equal` compares namespace nodes. A namespace node has no children,
  so the generic child-sequence comparison reported every pair of them as
  equal; two namespace nodes are now deep-equal only when their names — the
  prefix, absent for a default-namespace binding — and their string values, the
  bound namespace URI, both match.
- `schema-element(N)` and `schema-attribute(N)` used as **step** node tests are
  the declaration-aware tests of XPath 2.0 §2.5.4.4 and §2.5.4.6. As a step
  they matched every node on the axis. Both tests, in every spelling — a step,
  `instance of`, `treat as`, a function signature — are now decided by one
  implementation, which also gained the substitution-group clause and the
  nilled clause and accepts a complex-typed declaration. A name that is not
  declared matches nothing — §2.5.4.4 and §2.5.4.6 make it the static error
  `XPST0008`, which this release does not raise — and without a schema set in
  the static context the tests remain name-only, as before.
- Atomizing a namespace node yields `xs:string`, not `xs:untypedAtomic` — the
  rule that already applied to comments and processing instructions.
- `fn:namespace-uri-for-prefix` returns the empty sequence, not
  `xs:anyURI("")`, when the prefix has no binding — including the empty prefix
  on an element with no default namespace.
- A namespace prefix with no binding inside a kind test — `element(p:x)`,
  `attribute(p:a)`, `element(*, p:T)`, `schema-element(p:x)`, the same inside
  `document-node(...)` or a SequenceType — is the static error `XPST0081`. It
  silently resolved to no namespace.
- Under XSD 1.1 assertion scope the data model instance is cut at the asserted
  element, as §3.13.4.1 clause 1.3 constructs it: `parent::`, `ancestor::`,
  `following::`, `following-sibling::` and `preceding-sibling::` no longer
  reach outside it (`preceding::` and the downward axes never did). As the
  specification's note says, such a reference is not an error, it selects
  nothing. `/` and `fn:root()` are unchanged: they still answer a document
  node whose children are hidden, so `//x` inside an assertion is empty, which
  is what the W3C test suite expects.
- Inside an XSD 1.1 assertion `fn:id` finds the elements inside the asserted
  element, and only those, as §3.13.4.1 clause 1.3 constructs the data model
  instance. It found nothing at all in an assertion, because the buffer the
  validator builds to evaluate assertions indexed no ids: now
  `exists(id('y'))` holds on an element with a descendant `xml:id="y"`, while
  an element outside the asserted one — a sibling, say — is never found, even
  when it claims the value first.
- `fn:deep-equal` no longer looks at comment and processing-instruction
  **children**. F&O §15.3.1 compares a document or element node's
  `$i/(*|text())`, a sequence that holds neither kind, so
  `deep-equal(<a>x<!--c--></a>, <a>x</a>)` is now true where it was false.
  Such a child still *splits* the text around it and text nodes are never
  merged, so `<a>x<!--c-->y</a>` has two text children and remains different
  from `<a>xy</a>`; and a comment or PI that is itself an item of the two
  compared sequences is still compared, by string value and, for a PI, by
  target. `TreeComparer` is unchanged: its `deep_equal` and `deep_equal_iter`
  keep comparing every child, which is what a serialization round-trip check
  needs.
- `fn:deep-equal` compares two elements according to their **content**, as
  F&O §15.3.1 clauses (2) and (4) prescribe, instead of always comparing their
  children. Two elements annotated as having simple content — a simple type,
  or a complex type whose content is text-only — are compared by their typed
  values, so a validated `<a>1</a>` and `<a>01</a>` of type `xs:integer` are
  now deep-equal. With element-only content only the child elements are
  compared, so whitespace between them no longer counts; with mixed content
  the `(*|text())` sequences are compared, as before. An element annotated as
  having simple content is never deep-equal to one with complex content.
  Without a schema set in the static context the content kind of a complex
  type is not read: the element's typed value decides between simple and
  complex content, complex content is compared as mixed, and a nilled element
  of a complex type is compared as having the other element's content kind.
- `fn:deep-equal` compares two attributes' typed values with `eq` semantics,
  the same rule it applies to a free-standing atomic item, instead of plain
  value equality. The two answers could differ for schema-validated
  attributes: an `xs:integer` `1` and an `xs:decimal` `1.0` were reported
  different as attributes and equal as items. Untyped attributes are
  `xs:untypedAtomic` and compare as strings either way, so nothing changes for
  an unvalidated document.
- `fn:tokenize` reports an invalid `$pattern` (FORX0002), invalid `$flags`
  (FORX0001) and a `$pattern` that matches the zero-length string (FORX0003)
  when `$input` is the empty sequence or a zero-length string. It returned the
  empty sequence for such an input before looking at the pattern at all, so
  `tokenize("", "[")` and `tokenize((), "a*")` silently succeeded. F&O §7.6.4
  states the three errors with no exemption for any input, and states the
  empty-sequence result separately; that result is unchanged for a pattern
  that raises nothing, so `tokenize("", "\s+")` is still the empty sequence.
- `fn:replace` reports a malformed `$replacement` (FORX0004) even when nothing
  is replaced — when `$input` is empty, or when `$pattern` matches nowhere in
  it. The replacement string was only validated while the matches were walked,
  so `replace("", "a", "$")` and `replace("abc", "z", "$")` returned a string
  instead of raising. The two rules are unchanged: a `\` must be followed by a
  `\` or a `$`, and a `$` that is not part of such a pair must be followed by
  a digit.
- `fn:id` found an element by its `xml:id` only in a `DocumentKind::Full`
  document read from text (`from_reader` or `build_typed_document`), and only
  when the attribute value had no surrounding whitespace. The id index is now
  filled from `BufferDocumentBuilder::attribute` for an attribute of an
  element, the one place an attribute node is created, so a document built
  through the push API, a `Fragment` buffer and a copied subtree carry their
  ids too; and the index key is the value with XML whitespace stripped and
  collapsed, so `xml:id="d "` answers to `id(' d')`. Only the key is
  normalized — the attribute keeps its own value.
  `BufferDocument::get_element_by_id` reads the same index: it normalizes its
  argument the same way, never answers for a value that is not an NCName
  (`xml:id="a b"` and `xml:id="1x"` used to be found by their literal value),
  and finds `xs:ID`-typed attributes too (see *Added*).
- `fn:id` tokenized its argument on Unicode whitespace and looked up
  every token. It now follows F&O §15.5.2: the candidate IDREFs are
  `tokenize(normalize-space($s), ' ')` — XML whitespace only — and a
  token that is not a lexical NCName is ignored instead of looked up.
  An `xml:id` whose value is not an NCName is never selected.
- `fn:id` returned an empty sequence where the specification requires an
  error: it now raises `FODC0001` when the node it is given is in a tree
  whose root is not a document node. In the test of an XSD 1.1 type
  alternative, whose data model instance is rooted at the element itself
  (Structures §3.12.4), `fn:id` therefore makes the test false, as any
  dynamic error there does; it used to see the empty sequence.
- A `Fragment` buffer — whose document node holds parentless trees — now
  indexes the ids of each of its trees, and
  `BufferDocumentBuilder::register_xml_id`, which did nothing at all for a
  `Fragment` buffer, files them there too. When two elements of one tree share
  an id, the first in document order is the one selected, as F&O §15.5.2
  requires, and a tree built through the push API is not refused for it.
  Reading a `Full` document from text — with `from_reader` or
  `build_typed_document` — still reports `BufferDocumentError::DuplicateId`,
  now comparing the normalized values, so `xml:id="a"` and `xml:id=" a"`
  collide where they used to be accepted, and ignoring values that are not
  NCNames, which are never ids, so two `xml:id="1x"` are no longer reported.
  `register_xml_id` normalizes its value the same way, refuses one that
  another element of the same tree already has (in a `Full` document, any
  other element), accepts a repeat for the same element, which used to be
  `DuplicateId`, ignores a node that is not an element, and panics on a node
  reference that is not a node of the document being built, where it used to
  accept any reference unchecked.
- `XPathValue::as_slice` returns a one-element slice for a single item. It
  returned an empty slice for the `Item` variant, so a caller that borrowed a
  one-item value as a slice saw no items while `len()` said 1. The slice now
  always has `len()` elements, in sequence order, and is borrowed without
  cloning; the signature is unchanged, and a caller that matched the `Item`
  variant to work around it keeps working.
- `BufferDocumentBuilder::copy_attribute`, replacing an attribute of the
  same expanded name with one that carries no type annotation — a copy with
  `Annotations::Strip`, or of an untyped attribute — left the earlier
  attribute's annotation on the node, so the new value was reported with the
  old type. The later attribute now wins with its annotation or the lack of
  one, and `BufferDocument::has_type_annotations` stays exact.
- Atomizing a node whose type is a list type now yields one atomic value per
  list member, each typed with the list's item type (XDM §3.3.1.2, XPath 2.0
  §2.5.2). It yielded the whole list as a single value, so `data(@refs)` of an
  `xs:IDREFS` attribute holding `a b` had one item, and every operator and
  function saw that one value. This changes answers that a schema-aware host
  can observe: `@refs = 'b'` is now true where it was false (or raised
  `XPTY0004` for a built-in list type such as `xs:NMTOKENS` or `xs:IDREFS`),
  and `@refs = 'a b'` is now false where it was true; `@refs eq 'b'`,
  arithmetic on a list of several members and `xs:string(@refs)` now raise
  `XPTY0004` instead of answering (or failing with an error that had no code);
  `sum()`, `avg()`, `min()` and `max()` of a list of numbers add and compare
  the members instead of returning the list's text; `count(data(@refs))`,
  `distinct-values()`, `index-of()`, `string-join()` and `deep-equal()` see
  the members; a list with no members atomizes to the empty sequence. XPath
  1.0 compatibility mode applies its conversions to each member. The string
  value of such a node, and `fn:string`, are unchanged: still the whole text.
  `atomize::atomize_node` still returns the typed value as it is stored, one
  value of kind `XmlValueKind::List`. For a list whose item type is a union,
  the member type the validator actually chose is not recorded per member:
  each member is typed with the list's recorded item type, or `xs:string`
  when it holds a lexical form that type cannot hold.
  `compose::Value::atomics` likewise yields one value per member, and
  `Value::number` and `Value::key` raise `XPTY0004` for a single node whose
  list-typed value has several members and treat one with no members as the
  empty sequence (`NaN`, `None`); they used to see the whole list as one
  value.
- A nilled element, whose typed value is the empty sequence, now makes a value
  comparison, an arithmetic expression or a unary `+`/`-` return the empty
  sequence, as XPath 2.0 §3.4 and §3.5.1 require, instead of raising
  `XPTY0004`; `castable as T?` of it is true and `castable as T` false.
- The arguments of the built-in functions are now atomized as the function
  conversion rules require (XPath 2.0 §3.1.5): a node contributes its typed
  value, not its string value. A schema-aware host sees the difference:
  `fn:number(@flag)` of an `xs:boolean` attribute `true` is now `1` instead of
  `NaN`, `concat(@n, '')` of an `xs:integer` attribute ` 5 ` is `5`, a string
  argument whose typed value has several members raises `XPTY0004`, an empty
  typed value is the empty sequence (so `compare(@empty, 'a')` is the empty
  sequence), and an element with element-only content passed where a string or
  number is expected raises `FOTY0012`, because it has no typed value.
  `fn:string`, the zero-argument string functions (`string-length()`,
  `normalize-space()`), and the `fn:string(V)` step of XPath 1.0
  compatibility mode keep reading the string value. Untyped nodes are
  unaffected: their typed value is their string value.
- An element whose complex type has mixed content — `xs:anyType` included — now
  has its string value as an `xs:untypedAtomic` typed value, and one whose
  complex type has empty content has the empty sequence, as XDM §6.2.4
  requires. `BufferDocNavigator::typed_value` reported both as having no typed
  value (`TypedValue::Absent`), so atomizing such an element — in a
  comparison, in `fn:data`, in an XSD 1.1 assertion — raised `FOTY0012`. It now
  returns `TypedValue::Untyped` for mixed content and an empty list value for
  empty content; element-only content is still `Absent`.
- `fn:deep-equal` compares two typed values item by item, as F&O §15.3.1
  requires of the typed values of two simple-content elements or two
  attributes: same length, and each pair equal under `eq`, with `NaN` equal to
  `NaN`. A list-typed value was compared as one value, so two lists whose
  members include `NaN`, or an `xs:IDREFS` value and a list-of-`xs:IDREF`
  value with the same members, were reported different. `TreeComparer` is
  unchanged.

### Performance

- The general comparison operators `=` and `!=` no longer walk the whole
  Cartesian product of their operands. XPath 2.0 §3.5.2 makes `A = B` an
  existential test over every pair of atomized items, which the evaluator
  answered with a nested loop — `O(|A| · |B|)`, and unusable once both
  operands hold more than a few thousand items. The comparison now walks a
  bounded prefix of the product, so that a small comparison or an early match
  still costs exactly what it did, and then decides the rest with a hash index
  over the comparison classes of §3.5.2 in `O(|A| + |B|)`: numeric (keyed at
  the type the pair promotes to), string-like (including `xs:untypedAtomic`
  compared as a string, and `xs:untypedAtomic` cast to `xs:double` against a
  numeric), boolean, `xs:dateTime`/`xs:date`/`xs:time` keyed at the instant the
  comparison normalizes them to, the durations keyed at their (months, seconds)
  pair, `xs:QName`, and the types whose equality is structural. Every candidate
  the index finds is confirmed with the ordinary value comparison, and any
  input the index does not model — a conversion that fails, a union-typed
  value, `xs:untypedAtomic` against a type that is neither numeric nor
  string-like — falls back to the original loop, so results and errors,
  including which error a comparison raises and when, are unchanged. Two
  disjoint sequences of 100,000 items now compare in ~25 ms; before, 10,000
  items already took ~16 s.
- A `=` or `!=` comparison that is evaluated over and over — the predicate of
  `$big[. = $c]`, the body of a `for` or a quantified expression — now reuses
  the index of the operand that does not change during the evaluation, instead
  of rebuilding it every time. An operand counts as unchanging only if a
  conservative static analysis says so: it must read no part of the focus, call
  no function, and reference no variable bound by a `for`, `some` or `every`;
  on top of that, every reuse re-checks that the variables it was built from
  have not been rebound. The index lives in the dynamic context of the one
  evaluation run and is dropped with it, nothing is built before the second
  evaluation of the same comparison, and the index only answers a comparison
  when it can prove both the answer and the absence of an error for that pair
  of operands, so results and errors are unchanged. An index is keyed by a
  serial number of the expression's syntax tree, not by its address, so a host
  function that compiles and evaluates expressions of its own on the caller's
  `DynamicContext` is never answered from another expression's index, and
  replacing `DynamicContext::variables` with another store is detected as a
  rebinding. A host function registered
  through `FunctionSet` receives `&mut DynamicContext` and may rebind a
  variable while the other operand of the same comparison is being evaluated;
  such a write is detected, and the comparison then answers exactly what it
  would have answered without an index, with both operands evaluated in source
  order. Filtering 1,000,000 items
  against a 100,000-item sequence goes from days to ~0.4 s; the same filter
  against a one-item sequence is about twice as fast as before.
- `fn:matches`, `fn:replace` and `fn:tokenize` compile each regular expression
  once per evaluation run. They recompiled their `$pattern` argument on every
  call, so a constant pattern inside a predicate —
  `$items[matches(., '\p{Ll}')]` — paid for a full compile, including the
  expansion of any Unicode category it names, once per item. The compiled
  programs of one run are kept in its dynamic context, keyed by
  `(pattern, flags)`, bounded at 32 entries with least-recently-used eviction,
  and dropped when the run ends. A pattern that fails to compile is remembered
  too, and raises the same error every time. Answers, error codes and error
  messages are unchanged. On a 100,000-item filter `matches(., '\p{Ll}')` goes
  from 6.6 s to 67 ms; `replace()` in a loop is 23× faster and `tokenize()`
  39×; a single call, and a pattern computed afresh for every item, cost what
  they did.

## [0.2.0] - 2026-09-13

A breaking release, in three parts:

- **Exact occurrence bounds.** Every finite `maxOccurs` is enforced exactly.
  Execution limits keep pathological counted models finite, and hitting one
  is an operational failure, never a validity verdict. The release also adds
  a content-model inspector and precomputed successor closures on the
  validation hot path.
- **Hosts working with more than one document.** Additive extensions for
  hosts that embed the XPath engine: node identity that includes the
  document, `DomNavigator::type_annotation`, `XPathExpr` dependency
  metadata, the `DynamicContext` extension slot and `set_function_evaluator`,
  an owned default function namespace, and `BufferDocument::serial()`.
- **"XQuery without XQuery".** An XML serializer for any `DomNavigator`
  (`document::serialize`, compact or indented), a subtree copy with
  constructor semantics (`document::copy`), and a `compose` feature with
  `xpath!` and `form!` for building XML from query results in Rust — proven
  against the eighteen queries of the XQuery test suite's relational use
  case.

XPath built-in functions now apply the function conversion rules to
`xs:untypedAtomic` arguments, and `fn:round`, `fn:subsequence` and
`fn:codepoints-to-string` follow F&O to the letter.

Both W3C XSD suites fail exactly the tests 0.1.5 failed (XSD 1.0
39458/39510, XSD 1.1 2313/2319), the XQTS XPath 2.0 selection passes
8047/8047, and the GAEB DA XML 3.3 corpus loads as it did in 0.1.5 (all 32
schemas under XSD 1.1, 23 under XSD 1.0). The crate still builds and tests on
rustc 1.85.0.

### Upgrading from 0.1.x

Source changes, only where you use the item:

- Exhaustive `match`es on `compiler::ContentModelMatcher`,
  `validation::CompiledContentModel` or `validation::ContentValidatorState`
  lose their `AllGroupExtension` arm, and `validation::content::AllGroupExtPhase`
  is gone. No compile path ever constructed them (see *Removed*).
- `compiler::NfaTable` has a private field: build one with `NfaTable::new` or
  `NfaTable::with_counters` instead of a struct literal, and after a table has
  been executed mutate its states through `get_state_mut`.
- `XPathEvaluator::run_with` and `run_with_node_and_setup` require `N: 'ctx`;
  existing call sites satisfy it without change.
- `MaxOccurs::is_effectively_unbounded` is deprecated in favour of
  `is_unbounded`.

Behaviour you may observe without changing code:

- A finite `maxOccurs` above 10 000 is no longer treated as `unbounded`, so an
  instance with more occurrences than the bound is now invalid.
- The drivers (`drive_quick_xml`, `drive_navigator`, `drive_buffer_document`)
  return `Err` when validation cannot complete — an execution limit, or a
  complex type whose content model failed to prepare, which used to be
  validated silently as empty content.
- PSVI `[validity]` is `Invalid` on a parent whose child the content model
  rejected, and on the start event of an element with `xsi:nil` that is not
  nillable. Sink diagnostics are unchanged.
- Schema error locations point at the element's `<`, so line and column
  numbers in messages move.
- XPath: `round(-2.5)` is `-2`; `codepoints-to-string` raises `XPTY0004` for
  an `xs:decimal`, `xs:float` or `xs:double` argument, even a whole-valued
  one; an untyped argument now casts to the expected type, and one whose value
  is not in that type's lexical space raises `FORG0001` where it used to raise
  `XPTY0004`.
- Processing-instruction data is kept verbatim instead of trimmed, and
  `RoXmlNavigator` returns a PI's data as its string value.

### Added

- **Execution limits for counted content models** —
  `compiler::MAX_ACTIVE_CONFIGS` (live counter configurations per frontier)
  and `compiler::MAX_CLOSURE_WORK` (work per epsilon closure). Removing the
  10 000 cutoff means a pathological nested-nullable model such as
  `((a?){0,1000000}){0,1000000}` would otherwise grow its configuration set
  with the bound; the limits keep memory and per-child work finite. Exceeding
  one is an **operational failure**, never an acceptance decision:
  `compiler::ContentModelLimitExceeded` (with `ContentModelLimit`) at the
  automaton level, and a terminal `validation-resource-limit` diagnostic in
  the runtime. Every shape the old cutoff admitted still fits.
- **Dominance pruning for scalar counter configurations.** With exact bounds,
  nested non-nullable counted loops such as the W3C `particlesZ036` models —
  `choice{1,100000}(sequence{1,100000000}(a+), b)` — keep one configuration
  per way of partitioning the input into iterations, O(k²) after k children;
  the 16 660-child instance hit the new execution limit after several seconds
  where the old star approximation finished in a millisecond. A configuration
  whose counters are all equal to, or below-but-past-the-minimum of, another
  configuration at the same state accepts a superset of inputs through the
  same states, so the dominated one is dropped after every closure. Exact by
  construction (a differential test checks every string up to length 8
  against the unrolled automaton), and the frontier for those models is now
  constant; the three affected W3C tests pass again.
- `ValidationRuntime::operational_failure()` — the terminal failure recorded
  for a run. Once set, every push call is inert and `end_validation` returns
  the failure, so `drive_quick_xml` / `drive_navigator` /
  `drive_buffer_document` return `Err` instead of a `DriveOutcome`. A validity
  field without a successful completion is not a result.
- Fallible automaton entry points: `ActiveStates::{try_from_nfa,
  try_epsilon_closure, try_advance, try_advance_with_priority}`,
  `CompiledContentModel::try_from_matcher`,
  `ContentValidatorState::{try_advance_element, try_is_complete}`. The
  existing infallible methods keep their signatures and now panic with a
  clear message if a limit is exceeded (they are no longer used by the
  runtime).
- Regression tests: `tests/occurrence_bounds.rs` (10 001/10 002, a boundary
  matrix around 16 and 10 000, huge literals, both failure contracts on both
  drivers) and `compiler::nfa::exact_bounds_tests` (counter arithmetic at
  `u32::MAX`, limit behaviour).
- **Content-model inspector** (`compiler::inspect`): a readable,
  source-attributed description of what the validator compiled for a complex
  type, in three views — *Source* (type, document, location, content type,
  open content), *Authored particles* (the resolved particle tree with
  occurrence ranges, whether each range is unrolled or counted, declaration
  and type bindings, source locations) and *Compiled* (matcher kind, states,
  transitions including counter operations, counters, the initial frontier
  variant, an exposure line for models that can hit the execution limits, and
  substitution-group expansions). `inspect_content_model(&SchemaSet,
  ComplexTypeKey)` compiles through the validator's own path so the report
  is exactly what validation executes; `SchemaValidator::describe_content_model`
  reuses the prepared model and names a preparation failure;
  `find_complex_type` looks a type up by expanded name. Example:
  `cargo run --example inspect_content_model -- schema.xsd TypeName [ns]`.
- **`DomNavigator::type_annotation() -> Option<TypeKey>`**, with a default body
  returning `None`. The node's XDM type-name property: complex or simple for
  element and attribute nodes, `None` for untyped nodes and for every other
  node kind. `schema_type()` is unchanged and remains its simple-type
  projection. This is the half of `element(*, T)` / `schema-element(x)`
  matching with a complex `T` that can be added without breaking the public
  `ItemType`; `BufferDocNavigator` overrides it, `RoXmlNavigator` keeps the
  default.
- **`BufferDocument::serial()`** — the document's creation ordinal from a
  process-wide counter: unique within the process, strictly increasing in
  creation order. Intended for reproducible cross-document ordering and
  `generate-id()`-style identifiers.
- **`XPathExpr` dependency metadata**, computed once in a post-bind walk
  (nothing on the evaluation path changes):
  `referenced_external_vars()` — the declared externals the expression
  actually references, a subset of the unchanged `external_vars()`, which
  still lists every name supplied to `compile_with_vars`;
  `references_external_var(slot)`, O(1) and bitset-backed;
  `uses_focus()`, `uses_position()`, `uses_last()` — whether evaluation reads
  the *initial* focus (context item, `fn:position()`, `fn:last()`). Foci
  created inside the expression do not count: "Certain language constructs,
  notably the path expression `E1/E2` and the predicate `E1[E2]`, create a new
  focus for the evaluation of a sub-expression" (XPath 2.0 §2.1.2), so
  `$x[position() = 1]` uses neither the focus nor the position, while
  `a[position() = 1]` uses the focus (its first step is relative) but not the
  position; `for`, quantified and `if` expressions keep the enclosing focus.
  Functions that default to the context item (`string()`, `number()`,
  `name()`, `local-name()`, `namespace-uri()`, `base-uri()`, `root()`,
  `string-length()`, `normalize-space()`, `lang($l)`, `id($x)`) count as
  focus uses at the arity that reads it; calls this crate cannot inspect are
  conservatively counted.
  `function_calls() -> &[FunctionCallRef]` — the distinct (namespace, local
  name, arity) triples of every function called anywhere in the expression,
  nested foci included, so a host language can detect its impure calls
  (`doc`, `document`, `key`, `unparsed-text`, `collection`) itself. The
  namespace is the one the call *bound* to, so `fn:` names are reported
  identically in XPath 1.0 and 2.0 mode and the `FN_2010_NAMESPACE` alias is
  normalized. `FunctionCallRef` is re-exported at the crate root and from
  `xpath` under the `xsd11` feature.
- **`DynamicContext::with_extension` / `set_extension` / `extension::<T>()`** — a
  `&dyn Any` slot for host or engine state that extension functions can read
  during evaluation without changing the `FunctionEvaluator::eval(&self, …)`
  contract. One `DynamicContext` serves a whole expression (sub-expressions
  only save and restore the focus in place), so the slot is visible from every
  function call: predicates, path steps, `for` and quantified bodies, nested
  calls.
- **`DynamicContext::set_function_evaluator`** — the mutable counterpart of
  `with_function_evaluator`, so a custom `FunctionEvaluator` can be installed
  from an `XPathEvaluator::run_with` setup callback. Custom functions were
  reachable only through `parse` / `bind_node` / `eval_node` before; they now
  work through the high-level API.
- **`XPathContext::with_default_function_ns_owned(impl Into<String>)`** — the
  default function namespace from a runtime-built `String`, with no
  `&'static str` to leak per host context. `default_function_namespace()`
  prefers it over the public `default_function_ns` field; XPath 1.0 mode still
  yields `""`.
- **`document::serialize` — a `BufferDocument` can be written back out.** Three
  entry points, `serialize_document`, `serialize_node` and `to_string`, plus
  `SerializeOptions` (an optional XML declaration with `standalone`, and
  `indent`) and `SerializeError`. Generic over `DomNavigator` rather than over
  `BufferDocument`, so the same code writes a `BufferDocNavigator`, a
  `RoXmlNavigator` and whatever navigator a host embedding the engine brings,
  using only navigation, names and `value_ref()`. Escaping follows Canonical
  XML 1.0 §2.3 — the rules under which a re-parse cannot change the value back,
  since XML 1.0's end-of-line handling (§2.11) and attribute-value
  normalization (§3.3.3) would otherwise rewrite literal carriage returns and
  attribute whitespace. Namespace declarations are written where they are
  introduced, diffed against an in-scope stack: redundant re-declarations are
  dropped, an element leaving an inherited default namespace gets `xmlns=""`,
  the `xml` prefix and prefixed undeclarations are never written, and a
  subtree written on its own gains the declarations it inherits. Content XML
  cannot express is refused, never dropped: a character outside the XML 1.0
  `Char` production, a comment containing `--`, a PI target `xml` or data
  containing `?>`, and a name whose prefix is not bound to its namespace URI
  in the output scope. Output is compact by default — no layout whitespace
  added and none removed, which is what makes it an exact text round trip —
  while `indent: Some(n)` opts into formatted output (a line break plus `n`
  spaces per level, `Some(0)` the breaks alone) from the same traversal with
  no second tree and no pretty-print pass. Layout is added conservatively
  because it becomes text nodes when parsed again: every existing text
  character survives, any text child of a container suppresses added layout
  throughout that container's subtree (so mixed content such as
  `<p>Hello <b>world</b>!</p>` stays on one line and existing indentation is
  never reindented), a break lands only at a child boundary next to an
  element, `xml:space` is honoured per XML 1.0 §2.10 including for a subtree
  that reads its ancestors for the inherited value, and no leading blank line
  or final newline is invented. All 26,307 comparable instance documents of
  the W3C XSD test suite round-trip (`tests/serialize_roundtrip.rs`); the
  XQTS driver's private comparison serializer is gone in favour of this one,
  with its pass count unchanged. Other encodings, CDATA sections and a
  doctype are out of scope.
- **`document::copy` — copying existing nodes into a document under
  construction, with constructor semantics.** Four additive methods on
  `BufferDocumentBuilder`: `copy_subtree` (element, text, comment, PI,
  document node, attribute — new node identities, nothing shared with the
  source), `copy_attribute`, `append_content` and `append_atomic`; plus
  `CopyOptions` (`CopyNamespaces::{Preserve, NoPreserve}`, `inherit`,
  `Annotations::{Strip, Preserve}`), `CopyError`, the `CopySource` trait
  (every `DomNavigator` is one with nothing to preserve; `BufferDocNavigator`
  reports its schema bindings) and `NamespaceFixup`. Content sequences follow
  the constructor content rules (XQuery 1.0 §3.7.1.3): consecutive atomic
  values joined with a single space, document nodes flattened, zero-length
  text dropped, an attribute after a child refused (XQTY0024) — a duplicate
  attribute name keeps the later value. A copied element carries the
  namespace declarations its names need (XQuery 1.0 §3.7.4), with generated
  `ns0`, `ns1`, … prefixes on a conflict and `xmlns=""` where an inherited
  default namespace would otherwise apply, which is the invariant that lets
  `document::serialize` refuse an unbound name. Type annotations are stripped
  by default and can be preserved within one `SchemaSet`.
- **XQuery-style composition from Rust** (`compose` feature, implies `xsd11`):
  a `compose` module for hosts that build XML from query results without a
  query processor. `Composer` owns one arena, one name table, one namespace
  context and a cache of compiled expressions keyed by expression text plus
  external variable names, so an expression inside a loop is parsed once;
  every method takes `&self`, so it works inside iterator closures. `Doc` is a
  `Copy` handle on a document in that arena — loaded with
  `load_str`/`load_reader`/`load_file` or built with `build`/`build_sequence`
  — and every navigator, `Value` and `Form` shares its lifetime, so a node of
  one document splices into another with no lifetime work. `Value` exposes a
  result as the XDM sequence it is, with `nodes`/`atomics`/`boolean`/`string`/
  `number`/`key` on top, each refusing rather than guessing.
  `Form`/`Content`/`AttrValue`/`Name` describe a result element as an owned,
  closure-free tree; the emitter resolves prefixes, computes the namespace
  declarations each element must carry, atomizes and space-joins attribute
  sequences, and splices nodes with the constructor content rules, so an
  empty sequence leaves an empty element. `compose::order` implements
  `order by` with XDM comparison, stable sorting and one key evaluation per
  row; `compose::pipe` turns the FLWOR clauses into fallible iterator
  adapters (`try_filter`, `try_map`, `try_flat_map`, `order_by`) that keep the
  `Result` and fuse after the first error. `IntoXPathValue` and `IntoContent`
  convert Rust values, navigators, documents and sequences into bindings and
  content. The module uses only the crate's public API and adds no
  dependencies.
- **`xpath!` and `form!`** (`compose` feature): the composition written as
  macros. `xpath!(c, "expr", node, name = value)` evaluates a cached
  expression with an optional context node and named bindings;
  `form! { (result :id "b1" ^{ value } ..?^{ rows }) }` describes a result
  element — attributes, namespace declarations, literal and `Display` text,
  copied content, spliced sequences, and fallible splices whose first failure
  propagates with `?`. Both are exported at the crate root
  (`use xsd_schema::{form, xpath};`). A prefixed element name is written
  `p::local`: one colon after the element name is always the attribute
  marker, so nothing is ever reinterpreted silently. Element and attribute
  names are checked when the document is built (`ComposeError::InvalidName`).
  Braces are the documented spelling of a form invocation because a form's
  tokens also parse as a Rust expression, which rustfmt would reflow; all
  three delimiters expand identically. `Composer::new` predeclares the `xs`,
  `xsi` and `fn` prefixes, as every XQuery processor does, so `xs:date(…)` in
  an expression and `:xsi:type "…"` in a form need no declaration of their
  own; `with_namespace` rebinds any of them, the last declaration winning, and
  a predeclared prefix a form does not use is not written to the output.
  The eighteen queries of the XQTS "relational" use case, rewritten in this
  style, reproduce their expected results (`tests/compose_usecase_r.rs`);
  `examples/xquery_without_xquery.rs` runs three of them end to end, and
  `doc/COMPOSE.md` is the guide.

### Changed

- Counter increments use checked arithmetic on every counted path.
- Structural refactors, all behaviour-neutral (both W3C suites byte-identical,
  no public signature changed):
  - `src/schema/derivation.rs` (10,817 lines) is now the `schema::derivation`
    module directory: `simple`, `complex`, `normalize`, `particle`,
    `wildcard`, `attributes`, `constraints`, `type_table`, `redefine`,
    `tests`, with the entry point and shared helpers in `mod.rs`. Every
    previously reachable path is re-exported unchanged.
  - The four oversized functions in `validation/runtime.rs` —
    `start_element_by_id` (700 → 136 lines), `end_of_attributes_inner`
    (305 → 110), `validate_attribute_by_id` (291 → 130) and
    `validate_attribute_against_type` (293 → 45) — are entry-point sequences
    over 25 single-responsibility helpers, with no new allocation on the
    per-element path.
  - `compile_content_model_matcher_impl` (267 → 46 lines) dispatches to
    `compile_all_group_matcher`, `compile_all_group_extension_matcher`
    (xsd11), `compile_nfa_matcher` and `attach_own_open_content`.
- **Cross-document document order is reproducible.**
  `BufferDocNavigator::compare_position` orders nodes of distinct trees by
  `BufferDocument::serial()` instead of the document's heap address, which
  XPath 2.0 §2.4.1 permits and constrains: "The relative order of nodes in
  distinct trees is stable but implementation-dependent, subject to the
  following constraint: If any node in a given tree T1 is before any node in
  a different tree T2, then all nodes in tree T1 are before all nodes in tree
  T2." `RoXmlNavigator` keeps address ordering (there is no serial to attach
  to a foreign `roxmltree::Document`), which stays stable for the duration of
  an expression.
- **`XPathEvaluator::run_with` / `run_with_node_and_setup` accept callbacks over
  caller-frame state.** The callback bound is now
  `for<'d> FnOnce(&mut TypedEvaluator<'expr, 'ctx, 'd, N>)`: only the borrow
  of the evaluator stays higher-ranked, and the expression and static-context
  lifetimes are the evaluator's own. A setup callback can therefore install a
  reference to a local (`set_extension`, `set_function_evaluator`) instead of
  `'static` data only. The bound is strictly wider than before; the one new
  requirement is `N: 'ctx` (the navigator outlives the static-context borrow),
  which every existing call site satisfies through variance without change.

### Deprecated

- `MaxOccurs::is_effectively_unbounded` is deprecated; it now equals
  `is_unbounded` because no finite bound is approximated any more.

### Removed

- **The never-constructed all-group-extension composite matcher.**
  `ContentModelMatcher::AllGroupExtension`, `CompiledContentModel::AllGroupExtension`,
  `ContentValidatorState::AllGroupExtension` and
  `validation::content::AllGroupExtPhase` modelled an all-group base followed
  by an NFA extension. No compile path ever built one:
  Structures §3.4.2.3.3 clause 4.2.3 gives an extension of an all-group base a
  `{particle}` that is the base particle itself (4.2.3.1), one merged all group
  — "a model group whose {compositor} is all and whose {particles} are the
  {particles} of the {term} of the ·base particle· followed by the {particles}
  of the {term} of the ·effective content·" (4.2.3.2) — or, in the "otherwise"
  case 4.2.3.3, a sequence containing the base's all group, which All Group
  Limited (§3.8.6.2) clause 1 forbids. The compiler already produced the merged
  all group and rejected the third case, so the composite was unreachable in
  every configuration. These are public enum variants, so this is a breaking
  change for exhaustive `match`es on them. No validation verdict, diagnostic
  or W3C conformance outcome changes. With the arm gone,
  `ContentValidatorState::try_is_complete` always returns `Ok` and
  `is_complete` never panics: the one execution limit completion could hit
  lived there.

### Fixed

- **`fn:round` takes a half towards positive infinity, not away from zero.**
  F&O §6.4.4: "Returns the number with no fractional part that is closest to the
  argument. If there are two such numbers, then the one that is closest to
  positive infinity is returned", with the example "round(-2.5) returns -2 (not
  the possible alternative, -3)". Halves of a negative argument were rounded
  away from zero, so `round(-2.5)` answered -3 and `round(-3.5)` answered -4.
  All four numeric types now round the same way, and the floating-point special
  cases of §6.4.4 are explicit: NaN, both infinities and both zeroes come back
  unchanged, and an `xs:double` or `xs:float` argument "less than zero, but
  greater than or equal to -0.5" returns negative zero (`round(-0.3)` is `-0`,
  while the `xs:decimal` `-0.3` rounds to plain `0`). The rounding is computed
  from `floor(x)` and an exact fractional part instead of `(x + 0.5).floor()`,
  which for the largest `xs:double` below a half would have answered 1.
  `fn:round-half-to-even` is a different function and is unchanged; so are all
  W3C XSD and XQTS results.
  `fn:subsequence` rounds `$startingLoc` and `$length` through the same helper:
  F&O §15.1.10 defines its result as the items whose position `p` satisfies
  `p >= fn:round($startingLoc)` and `p < fn:round($startingLoc) +
  fn:round($length)`, and it kept a second, half-away-from-zero copy of the
  rounding (whose comment claimed `fn:round-half-to-even`), so
  `subsequence((1,2,3,4,5), -1.5, 4.5)` started at -2 and yielded `(1, 2)`
  instead of the specified `(1, 2, 3)`. Integral positions, NaN and the
  infinities are unaffected.
- **`fn:codepoints-to-string` rejects a non-integer numeric argument.** Its
  declared parameter type is `xs:integer*`, and the function conversion rules of
  XPath 2.0 §3.1.5 cast only an `xs:untypedAtomic` item to it: numeric promotion
  goes the other way, promoting `xs:decimal` and `xs:float` *to* `xs:double`
  (§B.1), never a numeric item down to `xs:integer`. A whole-valued
  `xs:decimal`, `xs:float` or `xs:double` therefore reaches the closing rule —
  "If, after the above conversions, the resulting value does not match the
  expected type according to the rules for SequenceType Matching, a type error
  is raised [err:XPTY0004]" — where it used to be accepted as a codepoint. So
  `codepoints-to-string(65.0)` and `codepoints-to-string(xs:double(65))` are now
  `XPTY0004`, while `xs:integer`, every type derived from it and an untyped node
  still work.
- **Built-in functions apply the function conversion rules to `xs:untypedAtomic`
  arguments.** XPath 2.0 §3.1.5: "Each item in the atomic sequence that is of
  type xs:untypedAtomic is cast to the expected atomic type. For built-in
  functions where the expected type is specified as numeric, arguments of type
  xs:untypedAtomic are cast to xs:double." Functions whose parameter is a
  specific atomic type checked the exact variant instead, so
  `month-from-date(end_date)` over a document with no schema raised `XPTY0004`
  where a general comparison on the same node cast and compared. The whole
  `*-from-date` / `*-from-dateTime` / `*-from-time` / `*-from-duration` /
  `timezone-from-*` / `adjust-*-to-timezone` family, `fn:dateTime`, the
  `xs:integer` parameters of `fn:remove`, `fn:insert-before` and
  `fn:round-half-to-even`, `fn:codepoints-to-string` (which also now atomizes
  its argument) and the `numeric` parameters of `fn:abs`, `fn:ceiling`,
  `fn:floor` and `fn:round` now cast through one shared helper
  (`functions::convert`). An untyped value whose lexical form is invalid for
  the expected type is the dynamic error `FORG0001`; a type that
  `xs:untypedAtomic` cannot be cast to at all still raises `XPTY0004`, so
  `fn:prefix-from-QName` keeps rejecting an untyped node (casting to
  `xs:QName` requires a string literal, §2.3.4). No central signature-driven
  conversion was introduced. W3C XSD and XQTS results are unchanged.
- **A schema element's reported location is now its `<`, not the end of the
  markup before it.** `SourceRef.span.start` was taken from quick-xml's
  `buffer_position()` before the read, which is where the *previous* event
  ended — the same read also consumes the whitespace in front of the tag. A
  declaration was therefore reported on the line above itself and at the
  column just past the preceding `>` (`examples/books.xsd`'s `BookForm` came
  out as 14:21 instead of 16:3). The markup start is now recovered from the
  tag's own length in `parser::reader::TrackedReader::read_event`, so every
  span, every error location and every inspector row points at the `<`. Spans
  stay stable per element, so `compiler::upa`'s `same_particle_origin`, which
  compares `(doc_id, span)`, is unaffected.
- **Epsilon states now carry a source location.** About half the rows of the
  inspector's state table read `(no origin)`: branch and merge states are
  invented by composition and `FragmentBuilder` left them without one. The new
  `compiler::NfaFragment::fill_missing_origin` gives every origin-less epsilon
  state the location of the construct that created it — applied after each
  model group's composition and after each particle's occurrence wrapper, so
  the innermost known construct wins. Term-bearing states are never touched,
  keeping `same_particle_origin` exact. Diagnostics-only; no verdict moves.
- **UPA compilation no longer compiles the base type uncapped.**
  `compile_base_all_group` called the public, non-UPA
  `compile_content_model_matcher`, so when an XSD 1.1 extension type was
  compiled for schema-time UPA checking (`compile_content_model_for_upa`) the
  base type's content model was built with exact occurrence bounds while the
  rest of the same compilation was capped by `cap_for_upa`. The mode is now
  threaded through, so both halves are capped (Sperberg-McQueen 2005: for
  determinism testing `F{n,m}` can be replaced by `F{min(n,1), min(m,2)}`) and
  the counted construction is not run for a model the UPA check discards. No
  UPA verdict changes: an all-group model carries its member bounds in either
  mode and `check_all_group_upa` does not read them.
- **`xsi:nil` on a non-nillable element is now invalid at the start event.**
  The pushed element state was `Invalid` and `cvc-elt.3.1` was reported, but
  the `SchemaInfo` returned by `validate_element` / `validate_element_by_id`
  for the *start* event still said `Valid`, so a streaming consumer reading
  per-element `[validity]` saw the violation only at end-of-element. Element
  Locally Valid (Element) (§3.3.4.2) clause 3.1 — "D . {nillable} = false, and
  E has no xsi:nil attribute" — is a verdict on the element itself, so both now
  report `Invalid`. Diagnostics, their order and every driver outcome are
  unchanged.
- **An unresolved `<xs:group ref="…"/>` now names the group.** The error read
  `unresolved group reference: NameId(42):17` — the raw interned ids — instead
  of the QName. Both group-reference resolution sites (`compile_group_ref` and
  the XSD 1.1 `flatten_all_group_ref_into`) now format the name with
  `schema::resolver::format_resolved_qname`, the helper every sibling error
  site already uses, giving `unresolved group reference:
  {http://example.com/tns}missing`.
- **Finite `maxOccurs` above 10 000 is now enforced exactly.** The compiler
  treated any finite maximum larger than `MAX_COUNTED_OCCURS = 10_000` as
  `unbounded`, so `a{0,10001}` accepted 10 002 children. Structures §3.9.4.3
  clause 2.2 requires the sequence length to be "less than or equal to the
  {max occurs}" whenever it is a number. Every finite bound now compiles to an
  exact counted loop up to the representable maximum `u32::MAX`. Occurrence
  literals beyond `u32` (schema-valid — `nonNegativeInteger` has no bound)
  saturate to `u32::MAX` and stay finite; this is documented on
  `parse_occurs` and only observable past 4 294 967 295 sibling occurrences.
- **A child rejected by the content model now makes its parent invalid.**
  `cvc-complex-type.2.4` was reported to the sink, but the parent's PSVI
  `[validity]` — and therefore `DriveOutcome::root_validity` — stayed
  `Valid`. Clause 2.4 of Element Locally Valid (Complex Type) (§3.4.4) is a
  constraint on the parent, so it is now `Invalid`. Sink diagnostics are
  unchanged; only the validity field moves.
- **A content model that cannot be prepared is no longer treated as empty
  content.** `SchemaValidator::new` silently dropped any complex type whose
  content model failed to compile, and the first element it governed was then
  validated as if the type were empty. The failure is now recorded
  (`SchemaValidator::content_model_failures`) and raised as an operational
  failure (`validation-preparation-failed`) when such a type is first used.
- **Node identity now includes the document.**
  `BufferDocNavigator::is_same_position` compared `current`, `virtual_parent`,
  `current_ns` and `attr_index` — each an index into *one* `BufferDocument` —
  but not the document itself, and `RoXmlNavigator::is_same_position` compared
  roxmltree `NodeId`s, which are per `Document`. Two navigators on the same
  node index of two different documents therefore compared equal. XPath 2.0
  §3.5.3: "A comparison with the `is` operator is true if the two operand
  nodes have the same identity, and are thus the same node; otherwise it is
  `false`." `is`, `<<` and `>>` were unaffected (they go through
  `compare_position`, which already compared the document); the reachable
  consequence was in `union`, `intersect` and `except`, which "eliminate
  duplicate nodes from their result sequences based on node identity"
  (§3.3.3) — `$a/root/x | $b/root/x` over two same-shaped documents yielded 2
  nodes instead of 4 — and in the adjacent-duplicate check of
  `DocumentOrderNodeIterator` at a tree boundary. Both navigators' `move_to`
  also copied the cursor but not the document, so moving onto a node of
  another tree landed on this tree's node of the same index; `RoXmlNavigator`
  now carries the document's base URI over as well. New integration test
  `tests/multi_document.rs` pins identity, `is`, the three set operators and
  block order across two documents for both navigators.
- **Processing-instruction data is kept verbatim.** `PI ::= '<?' PITarget (S
  (Char* - (Char* '?>')))? '?>'` (XML 1.0 §2.6) makes only the `S` between
  target and data a separator, but both quick-xml adapters — the
  `BufferDocument` builder's and the streaming validator's — trimmed the raw
  content first, so a PI ending in whitespace (Microsoft InfoPath writes
  `<?mso-application progid="…" ?>`) reached the tree, and any host hook, a
  character short.
- **A processing instruction's string value through `RoXmlNavigator` is its
  data.** `value()` / `value_ref()` asked roxmltree's `Node::text()`, which
  answers only for text and comments — a PI's data lives in `Node::pi()` — so
  they returned an empty string where `BufferDocNavigator` returned the data,
  and XDM asks for the data.

### Performance

- **Precomputed successor closures for counter-free content models**
  (`NfaTable::successor_closure`, `StateSet::union_with`). Each NFA state's
  epsilon closure of its consuming successors is computed once per table
  (lazily, shared through the `Arc`), so one child step is the OR of a few
  4-word bitsets over the matching frontier states instead of a depth-first
  epsilon search per child, exact because the epsilon closure distributes
  over unions. Paired A/B on a 47 MiB synthetic catalog against the preceding
  revision: validate-only 67.4 → 72.4 MiB/s (roxmltree),
  64.1 → 68.6 (BufferDoc), streaming 47.2 → 50.0, streaming without PSVI
  52.4 → 55.5; libxml2 control flat. A differential test compares the step
  with the previous algorithm on every string up to length 5–7 over unrolled,
  nullable epsilon-cycle, wildcard-priority and substitution-group models in
  both XSD versions. Counted models are unchanged. `NfaTable` gained a private
  cache field: it can no longer be built with a struct literal (use `new` /
  `with_counters`), and code that mutates `states` directly after a table has
  been executed must go through `get_state_mut`, which invalidates the cache.
- Two per-element allocation gates in the validation runtime: the element
  path and location are no longer cloned at every element end, and the
  attribute typed value is no longer cloned at every attribute, unless an
  identity constraint is actually active. Both W3C suites are unchanged.

## [0.1.5] - 2026-08-31

Composition and complex-type restriction fixes, prompted by the official GAEB
DA XML 3.3 schema corpus (<https://www.gaeb.de>) — 32 schemas built on chained,
chameleon `xs:redefine`. All 32 now load with every check enabled when the
schema set is XSD 1.1 — no opt-out, and none was added. Under XSD 1.0 nine
still fail, all on the "intensional restriction" class that the W3C suite
itself accepts only for 1.1. Both W3C suites are unchanged by this work (XSD 1.0 39458/39510, XSD 1.1
2313/2319, byte-identical failure sets).

### Fixed

- `xs:redefine` now resolves the component it redefines through the *effective
  view* of the redefined document — its transitive `include` and `redefine`
  edges — instead of only that document's own component index. A schema whose
  original is declared one hop below the redefine target failed to load with
  `src-redefine: Original ... not found`, which rejected valid chained and
  chameleon redefine graphs (six files of the GAEB corpus).
- Eight defects in `derivation-ok-restriction` (§3.4.6.3 / §3.9.6), each of
  which rejected valid restrictions or accepted invalid ones:
  - `Choice:Choice` (RecurseLax) folded the parent choice's occurrence range
    into every branch. An optional choice therefore made each derived branch
    optional, so it could no longer map onto a base branch whose first child is
    required — and, in the other direction, an optional choice was accepted as a
    restriction of a required one whenever every branch happened to be optional.
    The two choices' own ranges are now compared directly and the mapping runs
    over their raw `{particles}`.
  - Particles whose term is an empty model group were not removed as pointless,
    so `<xs:choice minOccurs="0"/>` still had to map onto something in the base.
    A required empty `choice` is still kept: it accepts no sequence at all.
  - Attribute uses with `use="prohibited"` were run through the attribute-type,
    `fixed`-value and (XSD 1.1) `{inheritable}` checks. Per §3.4.2.4 such an
    `<attribute>` corresponds to no component — it only suppresses the base's
    use — so its declared type no longer takes part in the derivation.
    Prohibiting a *required* base attribute remains an error.
  - Restricting a type derived by extension was checked against the extension's
    own particle alone. §3.4.2.3 makes the content type
    `sequence(inherited-particle, own-particle)`; without the inherited half,
    every element the base contributed looked like one the restriction invented.
  - The base side of a restriction reported only the attribute uses the type
    declares itself, never the ones it inherits, with the same consequence for
    inherited attributes.
  - Under XSD 1.1, a single-child group folded away by particle normalization
    (`<C minOccurs="m" maxOccurs="n">X</C>` → `X{m,n}`) is now also offered to
    the base in its original shape. XSD 1.1 restriction is language subsumption
    (§3.4.6.4), so the folded and unfolded spellings must be treated alike.
    XSD 1.0 is deliberately unchanged here: there the fold is what makes a
    single-branch `<choice>` restricting a multi-branch base choice invalid
    (W3C `msData` `groupH021v`, `particlesZ024`, both marked invalid for 1.0 and
    valid for 1.1).

- Three `cargo doc` intra-doc-link warnings: the Datatypes 1.1 production
  numbers in `is_valid_xsd_decimal_lexical` were read as item links, and
  `SubstitutionGroupMap` linked to a `pub(crate)` item absent from the
  rendered docs. `cargo doc --no-deps` is now warning-free with and without
  `--features xsd11`.

### Changed

- Replaced the two uses of `usize::is_multiple_of`, stabilized in Rust 1.87,
  with the equivalent modulo, so the crate builds on older toolchains. Verified
  against rustc 1.85.0 with `--all-features`. No `rust-version` is declared: the
  crate does not commit to a minimum supported version.

## [0.1.4] - 2026-08-22

Security release. Upgrades `quick-xml` past two denial-of-service advisories
and adopts the XML attribute-value and line-end normalization that the newer
parser performs. No public API change. W3C XSD 1.0 suite failures 19 → 18;
XSD 1.1 suite and the XQTS XPath suite are unchanged, as is instance-validation
throughput (within run-to-run measurement noise).

### Security

- Upgrade `quick-xml` from 0.31 to 0.41, which fixes two denial-of-service
  advisories that affected every parse of untrusted XML through this crate
  (the schema parser, the streaming validation driver and `BufferDocument`
  all read through `quick-xml`):
  - [RUSTSEC-2026-0194] — quadratic run time when checking a start tag for
    duplicate attribute names.
  - [RUSTSEC-2026-0195] — unbounded namespace-declaration allocation in
    `NsReader`.

### Fixed

- Attribute values are now normalized as XML 1.0 §3.3.3 requires before they
  reach the validator: a literal tab, carriage return or line feed inside an
  attribute value becomes a space. Previously the raw character was validated,
  so patterns and facets saw content no conforming XML processor would produce
  (W3C XSD 1.0 suite: `RegexTest_63.i`; suite failures 19 → 18).

### Changed

- Character data containing general references (`&amp;`, `&#65;`) is reported
  by `quick-xml` 0.38+ as separate `Text` and `GeneralRef` events. The schema
  parser, the streaming driver and the `BufferDocument` builder rejoin the run
  before validating it, so a reference no longer splits one text run into
  several validator events. Only the five predefined entities and character
  references are resolved; any other entity is a parse error, as before.
- Line endings in character data are normalized (`\r\n` and `\r` → `\n`)
  per XML 1.0 §2.11, which `quick-xml` 0.31 did not do for text events.

## [0.1.3] - 2026-07-21

### Fixed

- Defer the `cvc-type.2` abstract-type check for XSD 1.1 element
  declarations carrying `xs:alternative` (conditional type assignment).
  The governing type is not known until after attribute processing, so
  the check now runs against the final CTA-selected type in
  `end_of_attributes_inner` rather than the declared type at element
  start. This removes false-positive errors on elements whose *declared*
  type is abstract but whose CTA always resolves to a concrete
  alternative (e.g. OpenDRIVE 1.8 `<junction>`). A genuinely abstract
  governing type still errors.

## [0.1.2] - 2026-07-04

Conformance sweep: W3C XSD 1.0 suite failures reduced 47 → 19 (99.95%);
every remaining failure in both suites is a documented W3C dispute or an
intra-suite contradiction. Per-element allocation cleanup on the validation
hot path (+6–8% pure-validation throughput on the synthetic corpus; both
W3C suites byte-identical). API-additive only.

### Performance

- Build the element path without a per-element `String` (zero-alloc
  interned-name lookup in `push_element`).
- Skip XSD 1.1 inherited-attribute propagation and default recording
  entirely when the schema declares no `inheritable` attribute (every
  XSD 1.0 schema, most XSD 1.1 schemas).
- Prune trivial self-match entries from the substitution-group map and
  drop the map altogether for substitution-free, abstract-free schemas,
  so content-model term matching does no hash probes per child in the
  common case. Abstract-head entries are kept — their self-name omission
  is what blocks abstract elements from matching in instances.
- Precompute each content model's initial NFA state set once per compiled
  complex type instead of re-running the epsilon closure per element.
- Pool `ElementValidationState` shells across elements, retaining
  collection capacity (`text_content`, `seen_attributes`, …); a
  drift-guard test keeps `reset()` equivalent to a fresh state.
- Key the per-type content-model map and the outer substitution-group map
  with `ahash` instead of SipHash (interned keys, hot-path probes).

### Added

- Value-free push-API twins for throughput drivers that discard
  per-element PSVI: `validate_element_novalue`,
  `validate_element_by_id_novalue`, `validate_end_of_attributes_novalue`,
  and `validate_end_element_novalue` (returns `SchemaValidity`). The DOM
  driver uses all of them; the existing value-returning methods are
  unchanged.

### Fixed

- Identity constraints: duplicate names across schema documents of one
  namespace are now a compile error (§3.11 symbol space); NaN compares
  identical to NaN in key/unique fields (W3C bug 9196); a field matching an
  element with an attribute-only (empty) complex type violates
  cvc-identity-constraint clause 3.
- NOTATION: enumeration values must resolve to declared notations
  (Datatypes §3.3.20); `public` is optional when `system` is present under
  XSD 1.0 (errata).
- Facets: `length` may coexist with `minLength`/`maxLength` when inherited
  per Datatypes §4.3.1.4 (W3C bug 6446); facet elements are rejected inside
  complexContent restrictions; `anyAttribute`/attributes/particles are
  rejected inside simpleType restrictions.
- Derivation: user restrictions of `xs:anySimpleType` are rejected
  (cos-st-restricts.1.1); simpleContent restriction of a mixed base
  requires an inline `<simpleType>` (src-ct.2.2); restriction-declared
  attributes must be admitted by the base's attribute wildcard
  (derivation-ok-restriction.2); constraining facets on anySimpleType
  content are rejected; Element Declarations Consistent is enforced across
  extension merges.
- Substitution groups: the head type's `{prohibited substitutions}`
  (`complexType/@block`) now participates in Substitution Group OK
  (Transitive) clause 2.3.
- Wildcards: XSD 1.0 attribute-wildcard unions that are not expressible
  (§3.10.6) are rejected at compile time; XSD 1.1 unaffected.
- Content models: an empty `<xs:choice/>` with `minOccurs ≥ 1` is
  unsatisfiable instead of matching empty content.
- QNames: prefixed QName attribute values with undeclared prefixes are
  rejected (src-qname); dangling element `ref`s are rejected in
  non-chameleon documents (src-resolve).
- anyURI (XSD 1.0): enumeration facet values are checked against RFC 2396
  lexical rules (malformed scheme, incomplete `%`-escape, `\`, `^`).
- Schema loading: file locations are canonicalized, so case variants on
  case-insensitive filesystems and symlinked paths identify one schema
  document.

## [0.1.1] - 2026-06-27

Performance-focused release. No breaking changes to the public API.

### Performance

- Compile content models once at schema load time and share them across
  validations via `Arc`, instead of recompiling per element.
- Avoid cloning `ActiveStates` on the content-model hot path.
- Represent NFA states as a bitset with fused epsilon-closure computation, and
  use keyed `ahash` for name interning.
- Materialize PSVI typed values lazily / opt-out, avoiding allocation when the
  typed value is not consumed.
- Add an allocation-free `i128` fast path for numeric value parsing.

### Fixed

- Gate arena mutations so that mutating an existing entry invalidates the
  effective-facets cache (prevents stale derived facets).
- Resolve all `rustdoc` warnings.

### Changed

- Decompose `validate_end_element` into smaller units for maintainability
  (internal refactor; no behavioral change).

## [0.1.0] - 2026-06-09

Initial release: XML Schema (XSD 1.0/1.1) validator with PSVI and a built-in
XPath 2.0 engine.

[Unreleased]: https://github.com/semyonc/xsd-schema/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/semyonc/xsd-schema/compare/v0.2.3...v0.3.0
[0.2.3]: https://github.com/semyonc/xsd-schema/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/semyonc/xsd-schema/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/semyonc/xsd-schema/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/semyonc/xsd-schema/compare/v0.1.5...v0.2.0
[0.1.5]: https://github.com/semyonc/xsd-schema/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/semyonc/xsd-schema/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/semyonc/xsd-schema/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/semyonc/xsd-schema/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/semyonc/xsd-schema/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/semyonc/xsd-schema/releases/tag/v0.1.0
[RUSTSEC-2026-0194]: https://rustsec.org/advisories/RUSTSEC-2026-0194
[RUSTSEC-2026-0195]: https://rustsec.org/advisories/RUSTSEC-2026-0195
