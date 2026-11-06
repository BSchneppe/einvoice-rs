# xsd-schema Introduction

This document is a short introduction to the public API in `xsd-schema`.
It is intentionally smaller than rustdoc: use it to find the right entry
points first, then drill into rustdoc for method-level details. Coming from
0.2.x? [Upgrading to 0.3](UPGRADING_0_3.md) lists what changed.

## Feature Sets

Before choosing an API entry point, decide which crate configuration you want.
The public surface changes mainly through the `xsd11` and `async` features.

| Mode | Cargo setup | What you get |
| --- | --- | --- |
| XSD 1.0 only | `default-features = false` | Most lightweight mode. XSD 1.0 schema parsing, compilation, and validation only. No XPath engine, no `BufferDocument` support, no `regexml`-backed XSD/XPath regex support. |
| XSD 1.0 + async | `default-features = false, features = ["async"]` | Same XSD 1.0-only surface, plus async schema-loading/directive-resolution APIs. |
| XSD 1.1 | `features = ["xsd11"]` | Full XSD 1.1 mode: XSD 1.1 processing, XPath 2.0 engine, `regexml`, Unicode normalization, and `BufferDocument` / typed document support. |
| XSD 1.1 + async | `features = ["xsd11", "async"]` | Full XSD 1.1 surface plus async schema-loading/directive-resolution APIs. |
| XSD 1.1 + composition | `features = ["compose"]` | Full XSD 1.1 mode plus the `compose` module: the `xpath!` and `form!` macros, iterator pipelines and the composition layer of section 9. `compose` implies `xsd11`. |

Notes:

- `xsd11` is the feature that unlocks the `xpath` and `document` modules.
- `async` does not make the whole pipeline async. It only affects schema loading
  and directive resolution I/O.
- If you just want the smallest build, use XSD 1.0 without default features.
- If you need XSD 1.0 validation with the XPath engine (or full `regexml`-backed
  XSD regex features like character-class subtraction), enable the `xsd11`
  feature and create your schema set with `SchemaSet::new()` (XSD 1.0 mode).
  The `xsd11` feature controls which code is *compiled*; `SchemaSet::new()`
  vs `SchemaSet::xsd11()` controls which *semantics* are applied at runtime.
  Note: pattern-facet `\p{X}` category escapes are version-gated — XSD 1.0
  mode pins the tables to Unicode 3.0 (matching the W3C XSD 1.0 test corpus,
  see `src/regex_xsd_unicode.rs`) regardless of whether the engine is the
  `regex` crate or `regexml`, while XSD 1.1 mode passes through to regexml's
  current Unicode tables per §G.4.2's "or in some later version" clause.

## Main Entry Points

| Task | Start here | Async variant (feature `async`) |
| --- | --- | --- |
| Load one schema | `load_and_process_schema`, `load_schema` | `load_and_process_schema_async`, `load_schema_async` |
| Load many related schemas | `SchemaSetBuilder`, or `parse_schema_only` + `process_loaded_schemas` | `SchemaSetBuilder::compile_async`, `add_async` |
| Switch between XSD 1.0 and 1.1 | `SchemaSet::new()` / `SchemaSet::xsd11()` | — |
| Check which directives could not be read | `SchemaSet::directive_failures`, `CompiledSchemaSet::directive_failures` | the same |
| Map schema locations to local copies | `LocationRewriter`, `SchemaSetBuilder::set_location_rewriter` | the same |
| Stream XML validation | `validation::SchemaValidator` + `ValidationRuntime` | — |
| Validate against a stipulated declaration or type, or laxly | `ValidationRuntime::set_validation_root` with a `ValidationRoot` | — |
| Bound what an instance may contain | `xml::ParseLimits`, `ValidationRuntime::set_parse_limits` | — |
| Validate and build a typed document in one pass (`xsd11`) | `document::build_typed_document_with`, `TypedBuildOptions` | — |
| XPath evaluation | `xpath::XPathContext`, `xpath::XPathExpr`, `RoXmlNavigator` | — |
| External schema resolution | `SchemaResolver`, `SchemaLoader`, `LoaderChain`, `SchemaCatalog` | `AsyncSchemaLoader`, `SchemaResolver::load_schema_async` |
| Traverse compiled schema model | `SchemaSet`, `schema_set.namespaces`, `schema_set.arenas`, `schema::build_dependency_graph` | — |

## 1. Schema Loading

For most callers, the easiest API is:

- `SchemaSet::new()` for XSD 1.0
- `SchemaSet::xsd11()` for XSD 1.1
- `load_and_process_schema()` for a single schema document
- `SchemaSetBuilder` when you want a fluent multi-document load/compile flow

`load_and_process_schema()` runs the full pipeline:

1. Parse the primary schema.
2. Resolve `xs:include` / `xs:import` / `xs:redefine` / `xs:override`.
3. Apply redefine/override composition.
4. Assemble inline types.
5. Resolve QName references.
6. Allocate particle-local element/type bindings.

```rust
use xsd_schema::{SchemaSet, load_and_process_schema};

let mut xsd10 = SchemaSet::new();
load_and_process_schema(
    br#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
            <xs:element name="root" type="xs:string"/>
        </xs:schema>"#,
    "memory.xsd",
    &mut xsd10,
    None,
)?;

let mut xsd11 = SchemaSet::xsd11();
load_and_process_schema(
    br#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
                    xmlns:vc="http://www.w3.org/2007/XMLSchema-versioning"
                    vc:minVersion="1.1">
            <xs:element name="root" type="xs:string"/>
        </xs:schema>"#,
    "memory-11.xsd",
    &mut xsd11,
    None,
)?;
# Ok::<(), xsd_schema::SchemaError>(())
```

For multi-file loading, `SchemaSetBuilder` is usually the cleanest API:

```rust
use xsd_schema::SchemaSetBuilder;

let compiled = SchemaSetBuilder::xsd11()
    .add("urn:books", "examples/books.xsd")?
    .compile()?;

println!("loaded {} document(s)", compiled.stats.documents_loaded);
let schema_set = compiled.schema_set();
# let _ = schema_set;
# Ok::<(), xsd_schema::SchemaError>(())
```

Use the lower-level two-phase API when you need to control document loading
yourself, for example when schemas come from a database, cache, or custom
loader:

```rust
use xsd_schema::{SchemaSet, parse_schema_only, process_loaded_schemas};

let primary_bytes = br#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"/>"#;
let shared_bytes = br#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"/>"#;

let mut schema_set = SchemaSet::new();

parse_schema_only(primary_bytes, "primary.xsd", &mut schema_set)?;
parse_schema_only(shared_bytes, "shared.xsd", &mut schema_set)?;

let (_inline_stats, _resolution_stats) = process_loaded_schemas(&mut schema_set)?;
# Ok::<(), xsd_schema::SchemaError>(())
```

Use `PipelineConfig::parse_only()` or `parse_schema_only()` if you want to stop
before directive loading and reference resolution.

Every entry point checks a schema alike: `load_and_process_schema`,
`SchemaSetBuilder::compile`, `process_loaded_schemas` and the asynchronous
counterparts apply the same directive rules (`src-include`, `src-import`,
`src-redefine`, the catalog) and run the same schema checks — derivation,
value constraints, Unique Particle Attribution, type alternatives,
identity-constraint references, substitution groups. They differ only in how
the documents are read.

### Directives that could not be read

A directive whose document cannot be read — a missing file, a refused URL — is
not a schema error (Structures §4.2.3, §4.2.6.2: a `schemaLocation` is a hint):
the set compiles without the document. It is recorded instead, with the
location as written, the namespace, the location read and the directive's
position, and a reference that then fails to resolve names it. A host that
needs the whole schema checks the list (a failed `xs:redefine` that redefines
components is an error, `src-redefine`):

```rust
use xsd_schema::SchemaSetBuilder;

let compiled = SchemaSetBuilder::new()
    .add_source(
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
             <xs:include schemaLocation="no-such-file.xsd"/>
             <xs:element name="root"/>
           </xs:schema>"#,
        "/schemas/main.xsd",
    )?
    .compile()?;
let failures = compiled.directive_failures(); // also SchemaSet::directive_failures
assert_eq!(failures.len(), 1);
assert_eq!(failures[0].schema_location(), Some("no-such-file.xsd"));
# Ok::<(), xsd_schema::SchemaError>(())
```

For an `xs:import`, `DirectiveFailure::namespace_supplied` says whether another
document of the set supplies the namespace — the hint failed, but the
namespace's components are not missing for that.

### Nesting limits

Schemas are bounded in nesting, so that a hostile or broken schema is refused
with an error instead of overflowing the stack:

- `ParserConfig::max_depth` (default `parser::DEFAULT_MAX_DEPTH`, 128) bounds
  the element nesting of each schema document and, as `SchemaSet::max_depth`,
  the nesting of components through references (model groups, base, item and
  member types, attribute groups, substitution group heads);
- `ResolverConfig::max_depth` (default 100) bounds the chain of `xs:include` /
  `xs:import` / `xs:redefine` / `xs:override` directives;
- XPath expressions — in a schema's assertions and type alternatives, and in
  any expression the engine parses — nest at most
  `xpath::MAX_EXPRESSION_DEPTH` (256) levels.

A refusal is `SchemaError::LimitExceeded`, naming its setting (`SchemaLimit`),
or `XPathError::LimitExceeded`; `0` turns a schema limit off, and the
protection with it. The measured headroom is in
[Implementation Limits](IMPLEMENTATION_LIMITS.md#6-nesting).

### Async loading note

When the `async` feature is enabled, the crate also exposes async variants of
the top-level loading API:

- `load_schema_async(...)`
- `load_and_process_schema_async(...)`

For builder-style loading, the async counterparts are:

- `SchemaSetBuilder::with_async_loader(...)`
- `SchemaSetBuilder::add_async(...)`
- `SchemaSetBuilder::compile_async(...)`

Important detail: these APIs are async only for schema loading and directive
resolution I/O. Parsing, inline type assembly, QName resolution, and particle
allocation still run synchronously because they are CPU-bound phases.

## 2. XML Validation With Event Driving

The validation API is push-based. The usual call order is:

1. `SchemaValidator::new(...)`
2. `start_run(...)`
3. For each element:
   `validate_element` -> `validate_attribute*` -> `validate_end_of_attributes`
4. For content:
   `validate_text` / `validate_whitespace`
5. On close:
   `validate_end_element`
6. After EOF:
   `end_validation`

Each event returns `SchemaInfo`, which can be used to inspect the selected
declaration, resolved type, content model, typed value, and XSD 1.1 type
alternative/assertion results.

### ValidationFlags

`SchemaValidator::new(&schema_set, flags)` takes a `ValidationFlags` bitset
that turns runtime features on and off. The type is a
[`bitflags`](https://docs.rs/bitflags)-style set, so compose with `|`, remove
with `-`, and test with `.contains(...)`.

| Flag | Default | Purpose |
| --- | --- | --- |
| `REPORT_WARNINGS` | **on** | Emit XSD warnings (non-fatal diagnostics) to the sink alongside errors. Clear this if you only want hard errors. |
| `BUILD_PSVI_TYPED_VALUES` | **on** | Retain the typed value and whitespace-normalized value (PSVI `[schema actual value]` / `[schema normalized value]`) on the returned `SchemaInfo`, so callers can read `typed_value` / `normalized_value`. On by default for back-compat. **Clear it for validity-only streaming**: the runtime then skips building and keeping values the caller would discard — including an allocation-free fast path for the string family and for the integer hierarchy / `xs:decimal` (no per-value `BigInt`). **Validity and diagnostics are never affected** — values are still materialized internally wherever something *needs* one: a value-space facet (numeric range, `totalDigits`, value-space `enumeration`), a `fixed`/`default` comparison, ID/IDREF/ENTITY, an identity constraint, QName/NOTATION prefix binding, or an XSD 1.1 assertion. The fast paths only *accept* values they can prove valid; anything else defers to the normal path, so the emitted error codes are identical too. Only the *retained output* is dropped: with the bit clear, `SchemaInfo.typed_value` / `normalized_value` are `None` for nodes nothing else consumed. The W3C suite pass/fail set is identical with the bit on or off. |
| `PROCESS_IDENTITY_CONSTRAINTS` | — | **Deprecated, no effect.** `xs:key`, `xs:unique` and `xs:keyref` are always enforced: they are part of an element's validity (Structures §3.3.4.3 clause 6), and the bookkeeping costs only elements in a constraint's scope. Remove the bit from your flags. |
| `ALLOW_XML_ATTRIBUTES` | off | Accept every attribute in the reserved `http://www.w3.org/XML/1998/namespace` namespace (`xml:lang`, `xml:space`, `xml:base`, `xml:id`) **without** checking the element's complex type for an allowing declaration or wildcard. This is a lenient-parser convenience and is **not** XSD-conformant: the spec requires every attribute (including those in the xml namespace) to be matched by a declared `{attribute use}` or an `{attribute wildcard}` whose namespace constraint admits the xml namespace. Set this bit when you want `xml:lang` to "just work" against schemas that don't explicitly import the xml namespace; leave it clear for strict conformance (e.g. when running the W3C XSD test suite). `xml:base` base-URI tracking for `xsi:schemaLocation` hint resolution happens **regardless** of this flag — the flag only affects whether the attribute itself participates in type-level attribute validation. |
| `STRICT_MODE` | off | Promote warnings to errors. Combine with `REPORT_WARNINGS`. |
| `XPATH_TYPES` | off | Let an `xsi:type` name the types the XPath 2.0 data model adds to the XML Schema namespace where the set's XSD version does not define them — `xs:anyAtomicType`, `xs:untypedAtomic`, `xs:dayTimeDuration` and `xs:yearMonthDuration` in an XSD 1.0 set, `xs:untypedAtomic` in an XSD 1.1 set. For a host that validates nodes of the data model; `SchemaSet::xpath_type` finds the same types by code. Schema documents never see them. |
| `PROCESS_ASSERTIONS` (`xsd11`) | off | Enable XSD 1.1 `xs:assert` processing. `SchemaValidator::new(...)` honours the bit: with it set, assertions are evaluated inline while streaming (the subtree of each asserted element is buffered) — the mode `SchemaValidator::new_fragment_buffer(...)` selects, which sets the bit for you. Without the bit, `xs:assert` is not evaluated and an asserted element reports `AssertionOutcome::NotEvaluated`. (The `assertion` facet of simple types is part of simple-type validation and runs either way.) |

The default — `ValidationFlags::default()` — enables `REPORT_WARNINGS` and
`BUILD_PSVI_TYPED_VALUES`. This matches the strict-conformance posture:
`xml:*` leniency and strict-mode warning promotion are opt-in, identity
constraints are always enforced, and the PSVI typed values are populated for
back-compat. Common recipes:

```rust
use xsd_schema::validation::ValidationFlags;

// Strict XSD conformance (key/unique/keyref included):
let strict = ValidationFlags::default();

// Lenient mode for hand-written schemas: accept xml:lang / xml:space / xml:base
// on any element even if the schema does not declare them.
let lenient = ValidationFlags::default() | ValidationFlags::ALLOW_XML_ATTRIBUTES;

// Lean streaming: only need valid/invalid + errors, not the PSVI typed values.
// Drops per-value materialization (incl. BigInt per integer) where nothing
// else needs it; validity is unchanged.
let lean = ValidationFlags::default()
    & !ValidationFlags::BUILD_PSVI_TYPED_VALUES;
```

For XSD 1.1 assertion-backed types, request assertion processing — either
with the flag on `SchemaValidator::new`, or with the fragment-buffering
constructor, which sets the flag for you:

```rust
# #[cfg(feature = "xsd11")] {
use xsd_schema::validation::{SchemaValidator, ValidationFlags};
use xsd_schema::SchemaSetBuilder;

let compiled = SchemaSetBuilder::xsd11()
    .add_source(
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
             <xs:element name="range">
               <xs:complexType>
                 <xs:attribute name="min" type="xs:int"/>
                 <xs:attribute name="max" type="xs:int"/>
                 <xs:assert test="@min le @max"/>
               </xs:complexType>
             </xs:element>
           </xs:schema>"#,
        "range.xsd",
    )?
    .compile()?;
let schema_set = compiled.schema_set();

let flags = ValidationFlags::default();
// Both evaluate xs:assert inline while streaming:
let validator = SchemaValidator::new(schema_set, flags | ValidationFlags::PROCESS_ASSERTIONS);
let validator = SchemaValidator::new_fragment_buffer(schema_set, flags);
# }
# Ok::<(), xsd_schema::SchemaError>(())
```

Without `PROCESS_ASSERTIONS`, `SchemaValidator::new(&schema_set, flags)` does
not evaluate `<xs:assert>`: a schema with assertions then validates every
instance as if the assertion did not exist, and the asserted element's
`SchemaInfo::assertion_outcome` says `NotEvaluated`. Pick the flags
explicitly when you ship XSD 1.1 validation:

| Goal | Constructor |
| --- | --- |
| XSD 1.0, or XSD 1.1 without `xs:assert` | `SchemaValidator::new(...)` |
| XSD 1.1 with `xs:assert` evaluation, streaming | `SchemaValidator::new(...)` with `PROCESS_ASSERTIONS`, or `SchemaValidator::new_fragment_buffer(...)` |
| — | `SchemaValidator::new_main_document(...)` selects the `MainDocument` assertion mode, in which **no assertion is evaluated** (an instance that violates an `xs:assert` validates; the asserted element reports `AssertionOutcome::NotEvaluated`). Do not use it to check assertions. |

The XPath of an assertion (`xs:assert`, the `xs:assertion` facet) and of a
type alternative (`xs:alternative/@test`) sees only the built-in components:
Structures 1.1 §3.13.6.2 (XPath Valid) gives it a static context whose
"in-scope schema definitions are those components that are present in every
schema by definition": the built-in types (`xs:anyType`, the built-in simple
types) and the four attribute declarations `xsi:type`, `xsi:nil`,
`xsi:schemaLocation` and `xsi:noNamespaceSchemaLocation`, so
`schema-attribute(xsi:nil)` is fine — there is no built-in element declaration.
A test that names a user-defined type or declaration —
`$value instance of my:t`, `@a cast as my:t`, `my:t(@a)`, `element(*, my:T)`,
`schema-element(my:e)` — or that has another static error (an undeclared
prefix, an undefined variable, an unknown `xs:` constructor) makes the
**schema** invalid: `as-props-correct` for an assertion, `src-ct-alternative`
for a type alternative. Write such tests against the built-in type the user
type is derived from (`$value instance of xs:integer`, `@a castable as
xs:integer and xs:integer(@a) le 10`), or state the condition as facets. A
function the engine does not know, in a namespace other than the XML Schema
one, is left to the evaluation: the assertion then fails (`cvc-assertion`)
and the alternative never matches. Evaluation itself still uses the schema
set, so a node of a user type derived from `xs:integer` matches
`element(*, xs:integer)`.

### The validation root

How a run assesses its validation root is the run's choice (Structures 1.1
§5.2), set with `ValidationRuntime::set_validation_root` before the first
event. Every driver that takes the caller's runtime honours it, the typed
build included:

| `ValidationRoot` | The root is assessed |
| --- | --- |
| `Declaration` (the default) | by the top-level declaration of its name; an undeclared root is `cvc-elt.1` |
| `Element(key)` | against a stipulated element declaration, whatever the root's name (a root of another name is `cvc-elt.1`) |
| `Type(key)` | against a stipulated type definition — "validate as this type" — with no element declaration; an `xsi:type` may still override it |
| `Lax` | as `Declaration` when the root is declared (or has a usable `xsi:type`), otherwise laxly: no `cvc-elt.1`, its declared children and attributes validated strictly, its `[validity]` notKnown |

```rust
use xsd_schema::validation::{
    drive_quick_xml, CollectingValidationSink, SchemaValidator, SchemaValidity, ValidationFlags,
    ValidationRoot,
};
use xsd_schema::SchemaSetBuilder;

let schema_set = SchemaSetBuilder::new()
    .add_source(
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
             <xs:complexType name="Order">
               <xs:sequence><xs:element name="qty" type="xs:positiveInteger"/></xs:sequence>
             </xs:complexType>
           </xs:schema>"#,
        "order.xsd",
    )?
    .compile()?
    .into_schema_set();
let order = schema_set
    .lookup_type(None, schema_set.name_table.add("Order"))
    .expect("a named type");
let validator = SchemaValidator::new(&schema_set, ValidationFlags::default());
let instance = "<purchase><qty>3</qty></purchase>";

// Type-driven: <purchase> has no declaration and is validated as an `Order`.
let (mut errors, mut warnings) = (Vec::new(), Vec::new());
let mut runtime =
    validator.start_run(CollectingValidationSink { errors: &mut errors, warnings: &mut warnings });
runtime.set_validation_root(ValidationRoot::Type(order));
let outcome = drive_quick_xml(instance.as_bytes(), &mut runtime, &schema_set)?;
assert_eq!(outcome.root_validity, Some(SchemaValidity::Valid));
drop(runtime);
assert!(errors.is_empty());

// Lax: the undeclared root is no error, and nothing is known about its validity.
let (mut errors, mut warnings) = (Vec::new(), Vec::new());
let mut runtime =
    validator.start_run(CollectingValidationSink { errors: &mut errors, warnings: &mut warnings });
runtime.set_validation_root(ValidationRoot::Lax);
let outcome = drive_quick_xml(instance.as_bytes(), &mut runtime, &schema_set)?;
assert_eq!(outcome.root_validity, Some(SchemaValidity::NotKnown));
drop(runtime);
assert!(errors.is_empty());
# Ok::<(), Box<dyn std::error::Error>>(())
```

A type-driven root reports `TypeSource::Stipulated` as the source of its type
in its `SchemaInfo`.

### Driving validation from a `quick-xml` stream

The push-based runtime in §2 needs an XML event source. Hand-rolling that
plumbing means tracking xmlns scopes, scanning `xsi:type` / `xsi:nil`,
building a `NamespaceContextSnapshot` per element, and dispatching whitespace
vs character data correctly. Almost no caller wants to write that twice. The
`validation::quick_xml_driver` module ships a reusable driver in two layers:

| Layer | Entry points | Use when |
| --- | --- | --- |
| 1 — turn-key | `drive_quick_xml`, `drive_quick_xml_in` | All you want is for the runtime's sink to receive every diagnostic. The helper calls `runtime.end_validation()` for you. |
| 2 — hooks | `drive_quick_xml_with`, `drive_quick_xml_with_in` | You need to interleave work between validator events — building a typed DOM, tracking source spans, mirroring events into a custom store. The caller is responsible for calling `end_validation` after collecting any post-stream runtime state (e.g. `schema_location_hints`). |

Both layers read the text through the crate's well-formedness layer
(`xsd_schema::xml`), which the document builder and the schema parser share.
The checks are always on — XML 1.0 §1.2 makes a well-formedness violation a
fatal error — and follow the XML declaration: XML 1.1 and Namespaces in XML 1.1
for `version="1.1"`, XML 1.0 and Namespaces in XML 1.0 otherwise. A refusal is
`DriveError::NotWellFormed`, naming the rule and the byte position. Parse
limits are opt-in and set on the runtime; each is checked on the raw markup
before anything of it is interned:

```rust
use xsd_schema::validation::{drive_quick_xml, DriveError, ValidationRuntime, ValidationSink};
use xsd_schema::xml::{ParseLimit, ParseLimits};
use xsd_schema::SchemaSet;

fn validate_bounded<S: ValidationSink>(
    instance_xml: &str,
    runtime: &mut ValidationRuntime<'_, S>,
    schema_set: &SchemaSet,
) -> Result<(), DriveError> {
    runtime.set_parse_limits(
        ParseLimits::none()
            .with_max_depth(256)
            .with_max_name_bytes(1024)
            .with_max_nodes(1_000_000),
    );
    match drive_quick_xml(instance_xml.as_bytes(), runtime, schema_set) {
        Err(DriveError::LimitExceeded(e)) if e.limit() == ParseLimit::Depth => { /* too deep */ }
        Err(DriveError::NotWellFormed(e)) => eprintln!("{e}"), // rule, position
        other => {
            other?;
        }
    }
    Ok(())
}
```

The limits bound what is interned and built, not the reader's memory (a
whole tag or text run is read before it is checked): bound the input size
too.

Layer 1 in full:

```rust
use xsd_schema::{SchemaSet, load_and_process_schema};
use xsd_schema::validation::{
    drive_quick_xml, CollectingValidationSink, SchemaValidator, ValidationFlags,
};

fn validate(schema_xml: &str, instance_xml: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut schema_set = SchemaSet::new();
    load_and_process_schema(schema_xml.as_bytes(), "memory.xsd", &mut schema_set, None)?;

    let validator = SchemaValidator::new(&schema_set, ValidationFlags::default());
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let sink = CollectingValidationSink { errors: &mut errors, warnings: &mut warnings };
    let mut runtime = validator.start_run(sink);

    // Layer 1 internally: pushes xmlns scopes, scans xsi:type / xsi:nil,
    // dispatches whitespace vs text, and calls runtime.end_validation()
    // once the stream is drained. Comments and PIs are dropped.
    let _outcome = drive_quick_xml(instance_xml.as_bytes(), &mut runtime, &schema_set)?;

    drop(runtime); // releases the sink's borrow of `errors`
    for error in &errors {
        // The message, and the item it is about: `/Q{}order[1]/Q{}qty[2]`.
        eprintln!("{error} at {:?}", error.instance_path().map(|p| p.to_string()));
    }
    Ok(())
}
```

Layer 1 returns a `DriveOutcome { root_validity, max_depth }`. A document
that is not well-formed is `DriveError::NotWellFormed`, a broken limit
`DriveError::LimitExceeded`, an I/O error of the source `DriveError::Io`;
validation diagnostics flow through the sink, not the return type. Each
`ValidationError` and `ValidationWarning` names the item it is about:
`subject()` (the element or attribute, by expanded name) and `instance_path()`
(a positional path in XPath 3.0 form, `/Q{urn:po}order[1]/Q{}item[3]/@Q{}qty`),
also for the errors reported at the end of the document. `DriveError` is
`#[non_exhaustive]`: match it with a wildcard arm.

#### Layer 2 — handler-driven (advanced)

Layer 2 is `drive_quick_xml_with(reader, &mut runtime, &schema_set, &mut handler)`
plus a `ValidationEventHandler` you implement. Every method has a default
no-op, so a handler that only cares about (say) end-of-element fires exactly
that one method. Hook ordering is documented on the trait; the most-used
hooks are:

| Hook | Fires when |
| --- | --- |
| `before_element(view)` | After the new element's xmlns scope is pushed and the namespace-context snapshot is built, but **before** `runtime.validate_element` is called. |
| `after_element(view, info)` | After `validate_element` returns. `info.schema_type`, `info.is_nil`, `info.element_decl`, and `info.content_type` are stable here. |
| `before_attribute(view)` / `after_attribute(view, info)` | Per-attribute, in document order. |
| `skip_attribute(view) -> bool` | Between the two, for every attribute outside the `xsi` namespace: return `true` to have the attribute recorded but not validated **if the element's type does not declare it** (as if a `processContents="skip"` wildcard matched it — see `ValidationRuntime::skip_attribute`); a declared attribute is validated regardless. Default `false`. |
| `after_end_of_attributes(view)` | After `validate_end_of_attributes`. `view.info` is the EOA `SchemaInfo` (CTA-selected type lives here); under `xsd11`, `view.deferred_attribute_results` carries any CTA-revalidated per-attribute results, in original encounter order. |
| `after_end_element(end_info, depth)` | After `validate_end_element`, with the depth of the element that just closed (1 = root close). |
| `on_text(kind, text)` / `on_comment(text)` / `on_processing_instruction(target, data)` | Body events. `kind` distinguishes whitespace text, character text, and CDATA. |
| `on_xml_declaration(declaration)` | The document's XML declaration (version, encoding, standalone), before anything else. The readers read UTF-8 whatever `encoding` says; return an error to refuse an encoding you did not decode. |
| `on_doctype(view)` | A document type declaration in the prolog, once its grammar has been checked (one anywhere else is refused as not well-formed). Return an error to refuse documents that carry one. |

Layer 2 does **not** call `runtime.end_validation()`; you do, after
draining whatever runtime state your caller cares about (e.g.
`schema_location_hints`).

#### Source-span tracking (advanced)

Two further hooks carry byte offsets into the input (a leading byte-order
mark included, as in every position the readers report):

- `on_element_start_offset(byte_pos)` — fires immediately before
  `before_element`; `byte_pos` is the `<` of the element being reported.
- `on_element_end_offset(byte_pos)` — fires immediately after
  `after_end_element`; `byte_pos` is one past the `>` of the closing tag,
  whether the close came from `Event::End` or `Event::Empty`.

Both default to no-ops, so non-span callers pay only the cost of two empty
calls per element. Override them when building a span-aware DOM
(`document::typed_builder` is the in-tree consumer; see
`source_span_tracking` in its tests for a regression net).

### Unparsed entity declarations (ENTITY / ENTITIES types)

XSD §3.16.4 requires that every `xs:ENTITY` or `xs:ENTITIES` value names a
declared unparsed entity from the document's DTD. Modern streaming XML parsers
like `quick-xml` deliberately omit DTD processing — DTDs are widely considered
a legacy attack surface (billion-laughs, external entity injection, etc.) and
most contemporary XML workflows avoid them entirely. `xsd-schema` processes no
DTD either: a document type declaration in the prolog is checked (its name,
external identifier and the balance of its internal subset) and reported
through `ValidationEventHandler::on_doctype`, one anywhere else is refused as
not well-formed, and the declarations of an internal subset are not yet
supported (a reference to an entity it declares is refused).

However, a conformant XSD validator must still enforce the entity-name constraint
when DTD information is available. To bridge this gap without coupling the
validator to a DTD parser, `ValidationRuntime` accepts an optional set of
declared unparsed entity names via `set_unparsed_entities()`:

```rust
use std::collections::HashSet;
use xsd_schema::validation::{ValidationRuntime, ValidationSink};

fn declare_entities<S: ValidationSink>(runtime: &mut ValidationRuntime<'_, S>) {
    let mut entities = HashSet::new();
    entities.insert("logo".to_string());
    entities.insert("photo".to_string());

    runtime.set_unparsed_entities(entities);
}
```

When the set is provided, every `xs:ENTITY` / `xs:ENTITIES` value (including
defaulted attribute values) is checked against it. Undeclared names produce a
`cvc-datatype-valid.1.2.1` error. When the set is **not** provided (the
default), entity-name checking is skipped — the validator still validates
NCName syntax but does not require DTD context. This opt-in design keeps the
common case (no DTD) zero-cost while allowing conformance-critical deployments
to supply the information from whatever DTD source they have (pre-scan,
catalog, external parser, etc.).

Useful runtime helpers for editor or tooling scenarios:

- `get_expected_elements()` for content-model-aware completion/hints
- `get_expected_attributes()` for attribute completion
- `get_default_attributes()` for schema-supplied defaults — every defaulted
  attribute use of the element's type, inherited ones and attribute groups
  included; `take_default_attributes()` takes the list the end-of-attributes
  step found, each with its type and its typed value
- `set_location()` if you want validation errors tied to stream positions

To validate a document and build a typed in-memory copy of it in the same
pass, use `document::build_typed_document_with` (your validator, your sink,
your handler alongside the builder) or compose `document::TypedBuilderHandler`
with your own handler and drive it yourself; `src/document/typed_builder.rs`
is also a good reference for a namespace-aware `quick-xml` integration.

### XSI attributes

The four built-in XSI attributes are validated with proper per-attribute
`SchemaInfo`:

| Attribute | Type | Notes |
| --- | --- | --- |
| `xsi:type` | `xs:QName` | Lexical QName validation; semantic resolution is in element validation |
| `xsi:nil` | `xs:boolean` | |
| `xsi:schemaLocation` | `list(xs:anyURI)` | Even token count enforced (namespace/location pairs) |
| `xsi:noNamespaceSchemaLocation` | `xs:anyURI` | |

Schema-location hints are accumulated during a validation run. Each hint
carries the instance document's base URI for correct relative URI resolution.
Set the base URI before starting validation:

```rust
use xsd_schema::validation::{ValidationRuntime, ValidationSink};

fn before_the_run<S: ValidationSink>(runtime: &mut ValidationRuntime<'_, S>) {
    runtime.set_instance_base_uri("/absolute/path/to/instance.xml");
}

// Retrieve the hints afterwards:
fn after_the_run<S: ValidationSink>(runtime: &ValidationRuntime<'_, S>) {
    let sl_hints = runtime.schema_location_hints(); // &[SchemaLocationHint]
    let nnsl_hints = runtime.no_namespace_schema_location_hints(); // &[NoNamespaceSchemaLocationHint]
    println!("{} + {} hints", sl_hints.len(), nnsl_hints.len());
}
```

Complete pairs are accumulated from every `xsi:schemaLocation` attribute,
even from values that failed even-token-count enforcement (the complete
pairs are still valid hints).

### Hint-driven schema enrichment (two-pass validation)

Because `ValidationRuntime` borrows `&SchemaSet` immutably, schemas named by
the `xsi:schemaLocation` hints collected above cannot be loaded mid-run. They
feed a *second* validation pass against an enriched schema set built with
`enrich_schema_set`:

```rust
use xsd_schema::validation::{ValidationRuntime, ValidationSink};
use xsd_schema::{enrich_schema_set, SchemaSet};

fn enrich<S: ValidationSink>(
    schema_set: &SchemaSet,
    runtime: &ValidationRuntime<'_, S>,
) -> Option<SchemaSet> {
    let sl = runtime.schema_location_hints().to_vec();
    let nnsl = runtime.no_namespace_schema_location_hints().to_vec();

    let outcome = enrich_schema_set(schema_set, &sl, &nnsl);
    outcome.schema_set // Some(..) once hints loaded and recompiled
}
```

The wrinkle is *how to drive that second pass without parsing the instance
twice*. The push API is XML-source-agnostic, so alongside `drive_quick_xml`
there is a tree-walk driver that re-validates an already-parsed document:

| Driver | Module | Use when |
| --- | --- | --- |
| `drive_quick_xml` | `validation::quick_xml_driver` | Streaming from bytes/reader. For two-pass enrichment, call it again over the same bytes — simple, but parses the instance twice. |
| `drive_navigator` | `validation::navigator_driver` | Re-validate any `DomNavigator` tree (works with `RoXmlNavigator` without `xsd11`). Emits the *same* push-API sequence as the streaming driver. Walks iteratively: a tree of any depth costs heap, not stack. |
| `drive_buffer_document` | `validation::navigator_driver` (`xsd11`) | Convenience wrapper of `drive_navigator` for a `BufferDocument`. |

The tree-walk drivers read no text: well-formedness is the business of the
parser that built the tree — `BufferDocument::from_reader` goes through the
same well-formedness layer as the streaming driver, `RoXmlNavigator` relies on
`roxmltree`'s own checks.

**Buffer once, validate many times.** Parse the instance a single time into a
`BufferDocument`, then validate it against the original *and* enriched schema
sets by re-walking the in-memory tree — no second parse:

```rust
#[cfg(feature = "xsd11")]
fn validate_twice(
    instance: &[u8],
    schema_set: &xsd_schema::SchemaSet,
) -> Result<(), Box<dyn std::error::Error>> {
    use bumpalo::Bump;
    use xsd_schema::document::{BufferDocument, BufferDocumentOptions, SilentValidationSink};
    use xsd_schema::enrich_schema_set;
    use xsd_schema::validation::{drive_buffer_document, SchemaValidator, ValidationFlags};

    // Parse once.
    let arena = Bump::new();
    let doc = BufferDocument::from_reader(
        instance,
        &arena,
        &schema_set.name_table,
        BufferDocumentOptions::default(),
        Some(schema_set),
    )?;

    // Pass 1: collect hints.
    let validator = SchemaValidator::new(schema_set, ValidationFlags::default());
    let mut rt = validator.start_run(SilentValidationSink);
    drive_buffer_document(&doc, &mut rt, schema_set)?;
    let sl = rt.schema_location_hints().to_vec();
    let nnsl = rt.no_namespace_schema_location_hints().to_vec();

    // Pass 2: the SAME tree against the enriched set — no re-parse.
    if let Some(enriched) = enrich_schema_set(schema_set, &sl, &nnsl).schema_set {
        let validator = SchemaValidator::new(&enriched, ValidationFlags::default());
        let mut rt2 = validator.start_run(SilentValidationSink);
        drive_buffer_document(&doc, &mut rt2, &enriched)?;
    }
    Ok(())
}
```

`drive_navigator` walks the tree in document order and reproduces the exact
runtime call sequence the streaming driver emits, so both passes yield
identical diagnostics from one parse. Buffering needs the `xsd11` feature (the
`document` module is gated on it); without it, the two-pass re-parse with
`drive_quick_xml` is the straightforward route.

For the enrichment mechanics themselves — how `enrich_schema_set` re-loads the
original schemas, the `load_hints_into_builder` / `SchemaSetBuilder` path, the
`EnrichmentOutcome` failure modes, and the base-URI resolution rule — see
[*Hint-Driven Schema Enrichment (Two-Pass Validation)*](OVERVIEW.md#hint-driven-schema-enrichment-two-pass-validation)
in the Architecture Overview.

## 3. XPath

The XPath engine is available under the `xsd11` feature, so enable that
feature on your `xsd-schema` dependency before using the `xpath` module.

Main types:

- `xpath::XPathContext` for static context
- `xpath::XPathExpr` for compiled expressions
- `xpath::XPathEvaluator` for builder-style evaluation
- `RoXmlNavigator` (or another `DomNavigator`) for navigating XML trees

Basic usage:

```rust
#[cfg(feature = "xsd11")]
fn run_xpath() -> Result<(), Box<dyn std::error::Error>> {
    use roxmltree::Document;
    use xsd_schema::namespace::table::NameTable;
    use xsd_schema::xpath::{RoXmlNavigator, XPathContext, XPathExpr};

    let xml = r#"
        <books>
            <book><title>The First Book</title></book>
            <book><title>The Second Book</title></book>
        </books>
    "#;

    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let expr = XPathExpr::compile("//book/title", &ctx)?;

    let doc = Document::parse(xml)?;
    let result = expr
        .evaluator(&ctx)
        .run_with_node(RoXmlNavigator::new(&doc))?;

    assert_eq!(result.len(), 2);
    Ok(())
}
```

### DomNavigator

The XPath engine is generic over the `DomNavigator` trait — a read-only
cursor-style interface (modelled after C#'s `XPathNavigator`) that decouples
XPath evaluation from any specific DOM representation. Two implementations
are provided:

| Navigator | Backing store | Use when |
| --- | --- | --- |
| `RoXmlNavigator` | `roxmltree::Document` | You already have a parsed `roxmltree` tree and need XPath queries on it. Zero-copy, no extra allocation. |
| `BufferDocNavigator` | `document::BufferDocument` | You need schema-aware typed values in the DOM (XSD type annotations, typed atomization). Built via `BufferDocumentBuilder` during validation. Requires `xsd11`. |

You can also implement `DomNavigator` for your own DOM to plug into the
XPath engine directly.

### XPath 1.0 mode

The engine defaults to XPath 2.0. Use `XPathContext::with_mode(XPathMode::XPath10)`
to switch to XPath 1.0 syntax and semantics. In 1.0 mode the parser rejects
2.0-only constructs (`for`, `some`, `every`, `if`/`then`/`else`, `instance of`,
`cast as`, etc.), comparisons use the XPath 1.0 rules (§3.4), arithmetic always
coerces to `number` (double), and `number()` reads a string as XPath 1.0 §4.4
does (no exponent, no `+`, no `INF`). The same compiled AST, evaluator, and
function library are reused — mode selection only restricts syntax and adjusts
operator semantics.

`XPathContext::with_xpath10_compatibility(true)` is something else: XPath 2.0
syntax with the semantics XPath 2.0 itself defines for *XPath 1.0
compatibility mode* — arithmetic operands converted with the 1.0 number rules,
general comparisons by the compatibility rules of XPath 2.0 §3.5.2 (after
atomization), the first item of a sequence where a function expects one.

### Dates, times and the implicit timezone

A date/time value without a time zone is compared, subtracted and adjusted with
the *implicit timezone* of the dynamic context (F&O §10.4), which also gives
`fn:implicit-timezone()` and the zone of `fn:current-dateTime()`. Set it on the
static context; without one the host's local offset applies, read once per
evaluation:

```rust
# #[cfg(feature = "xsd11")] {
use xsd_schema::namespace::table::NameTable;
use xsd_schema::types::value::TimezoneOffset;
use xsd_schema::xpath::{RoXmlNavigator, XPathContext, XPathExpr};

let names = NameTable::new();
let ctx = XPathContext::new(&names).with_implicit_timezone(TimezoneOffset::from_hm(5, 30));
let expr = XPathExpr::compile("string(implicit-timezone())", &ctx)?;
assert_eq!(expr.evaluator(&ctx).run_string::<RoXmlNavigator<'static>>()?, "PT5H30M");
# }
# Ok::<(), Box<dyn std::error::Error>>(())
```

(`DynamicContext::with_implicit_timezone` sets it for one evaluation of a host
that builds the dynamic context itself.) Comparisons and arithmetic of dates,
times and durations are exact — no rounding of fractional seconds, durations
compared by months and seconds — and a result beyond the limits of the value
types raises the F&O error (`FODT0001`, `FODT0002`, …) rather than wrapping;
[Implementation Limits](IMPLEMENTATION_LIMITS.md#5-dates-times-and-durations)
has the limits.

### Common extensions

- `XPathContext::with_mode(...)` to switch between XPath 1.0 and 2.0 parsing
- `XPathContext::with_namespaces(...)` for prefix bindings
- `XPathContext::with_schema_set(...)` for schema-aware evaluation
- `XPathExpr::compile_with_vars(...)` plus `with_variable(...)` for external variables
- `run_bool`, `run_string`, `run_number`, `run_nodes` for convenient result coercion
- `run_with(...)` / `run_with_node_and_setup(...)` when variables need full `XPathValue` binding,
  or to install a custom `FunctionEvaluator` and host state
  (`te.context().set_function_evaluator(..)`, `set_extension(..)`) — see
  [`EXTENSIBILITY.md`](EXTENSIBILITY.md) §4.5
- `XPathExpr::referenced_external_vars()`, `uses_focus()`, `uses_position()`, `uses_last()`,
  `function_calls()` — compile-time dependency metadata for hosts that bind many variables
  or need to know whether an expression reads the focus
- `XPathEvaluator::with_regex_cache(&cache)` (an `xpath::RegexCache` the host keeps) for a
  host that evaluates the same expression many times, once per item: each pattern of
  `fn:matches`, `fn:replace` and `fn:tokenize` is then compiled once for the life of the
  cache instead of once per run
- `XPathEvaluator::with_warnings(&collector)` to receive the run's `XPathWarning`s — today
  `xs-seconds-capacity`, when a date/time or duration result has more fractional digits than
  its seconds field holds and keeps them truncated
- `fn:id` and `fn:idref` find the elements and attributes a typed document marks as IDs and
  IDREFs (`DomNavigator::find_element_by_id`, `find_nodes_by_idref`); on an untyped tree
  `fn:idref` is empty
- `SchemaSet::xpath_type(code)` for the types the XPath 2.0 data model adds to the XML Schema
  namespace (`xs:anyAtomicType`, `xs:untypedAtomic`, `xs:dayTimeDuration`,
  `xs:yearMonthDuration`), in an XSD 1.0 set too — for a host that validates nodes against a
  type it names (see `ValidationFlags::XPATH_TYPES`)
- `SchemaSet::element_value_constraint(key)` / `element_value_constraint_for_type(key, type)`
  for an element declaration's `default` / `fixed` value — its lexical form, and its value
  with QNames read with the bindings of the schema element they are written on

A regular-expression run without a host cache keeps its own cache for the run,
so a pattern in a predicate is compiled once per evaluation, not once per item:

```rust
# #[cfg(feature = "xsd11")] {
use xsd_schema::namespace::table::NameTable;
use xsd_schema::xpath::{RegexCache, RoXmlNavigator, XPathContext, XPathExpr};

let names = NameTable::new();
let ctx = XPathContext::new(&names);
let expr = XPathExpr::compile_with_vars(r"matches($s, '^\p{Lu}\p{Ll}+$')", &ctx, &["s"])?;

// Kept by the host across runs: the pattern is compiled once, not once per run.
let regexes = RegexCache::new();
for word in ["Hello", "hello", "World"] {
    let capitalised = expr
        .evaluator(&ctx)
        .with_regex_cache(&regexes)
        .with_variable("s", word)?
        .run_bool::<RoXmlNavigator<'static>>()?;
    assert_eq!(capitalised, word != "hello");
}
assert_eq!(regexes.len(), 1);
# }
# Ok::<(), Box<dyn std::error::Error>>(())
```

`XPathError` is `#[non_exhaustive]`; an error identified by an error QName —
`fn:error`, a host's own code, a specification code without a variant of its
own — is `XPathError::Raised`, and `error_code()` / `raised_error()` read the
code of any error. See [Extensibility](EXTENSIBILITY.md) §4 for host errors.

## 4. Schema Resolving

There are two related layers:

1. Document resolution:
   loading referenced schemas from files, embedded assets, catalogs, or custom loaders.
2. Component resolution:
   resolving QName references inside the compiled schema model.

For document resolution, start with:

- `SchemaSetBuilder::with_loader(...)`
- `SchemaSetBuilder::with_config(...)`
- `SchemaResolver`
- `SchemaLoader`, `FileSystemLoader`, `EmbeddedLoader`, `LoaderChain`
- `SchemaCatalog`

Example with a custom loader chain:

```rust
use xsd_schema::{EmbeddedLoader, FileSystemLoader, LoaderChain, SchemaSetBuilder};

let mut loaders = LoaderChain::new();
loaders.add(Box::new(EmbeddedLoader::new())); // embedded:// (the XML namespace schema, …)
loaders.add(Box::new(FileSystemLoader::new()));

let compiled = SchemaSetBuilder::with_loader(Box::new(loaders))
    .add("urn:books", "examples/books.xsd")?
    .compile()?;
# let _ = compiled;
# Ok::<(), xsd_schema::SchemaError>(())
```

If you need low-level control, use `SchemaResolver` directly. That is useful
when you want to inspect `LoadOutcome`, resolve relative locations yourself, or
drive `load_schema()` manually. When using `SchemaResolver` directly, remember
to populate the standard XML catalog yourself:

```rust
use xsd_schema::{SchemaResolver, SchemaSet};

let mut resolver = SchemaResolver::new();
resolver.catalog_mut().add_xml_catalog();

let mut schema_set = SchemaSet::new();
let _outcome = resolver.load_schema("examples/books.xsd", ".", &mut schema_set, None)?;
# Ok::<(), xsd_schema::SchemaError>(())
```

### Locations, and mapping them to local copies

Schema locations are resolved as RFC 3986 URI references (§5.2) and
normalized (§6.2.2, §6.2.3) when the location or its base has a scheme: dot
segments are removed, scheme and host compared in lower case, and `file:` URIs
read as files. One document is loaded once however its location is spelled —
`file:///s/a.xsd` and `/s/a.xsd` are one document. A relative location under a
relative base stays relative; it is not made absolute against the working
directory.

A `LocationRewriter` maps a location before it is identified and loaded — the
locations of directives (with the import's namespace) and those given to the
builder — in the style of an XML Catalog's `rewriteSystem` / `uri` entries. A
closure is one. Its answer is final: an absolute URI or path is loaded as it
is, whatever the base of the document that named the location, and a relative
path is resolved against the working directory. It is asked once per location.
A remote schema served from a local mirror:

```rust
use xsd_schema::{LocationRequest, SchemaResolver, SchemaSetBuilder};

// `rewriteSystem`: the remote prefix to a local directory.
let mirror = |request: &LocationRequest<'_>, _: &SchemaResolver| -> Option<String> {
    let rest = request.location.strip_prefix("https://example.com/schemas/")?;
    Some(format!("examples/{rest}"))
};

let mut builder = SchemaSetBuilder::new();
builder.set_location_rewriter(mirror);
let compiled = builder
    .add_source(
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
             <xs:import namespace="urn:books"
                        schemaLocation="https://example.com/schemas/books.xsd"/>
           </xs:schema>"#,
        "main.xsd",
    )?
    .compile()?;
// Read from examples/books.xsd; nothing was fetched.
assert!(compiled.directive_failures().is_empty());
# Ok::<(), xsd_schema::SchemaError>(())
```

`SchemaResolver::set_location_rewriter` does the same on a resolver you drive
yourself. `None` leaves a location as it is.

QName/component resolution is normally handled by the high-level pipeline, but
the public API is also available directly:

- `resolve_all_references(&mut schema_set)` for the whole schema set
- `schema::ReferenceResolver` for point lookups against a compiled `SchemaSet`

With the `async` feature, the corresponding async APIs are available via
`AsyncSchemaLoader`, `load_schema_async`, `load_and_process_schema_async`, and
the async builder methods. They follow the same rules as the synchronous ones:
the same directive walk and namespace checks, the same identity of locations,
the location rewriter, the nesting limits.

## 5. Traversal And Schema Analysis

After full processing, the compiled schema model is intentionally open for
application-specific analysis. The main surfaces are:

- `schema_set.documents` for document-level metadata and directives
- `schema_set.namespaces` for global component indexes
- `schema_set.arenas` for the actual component records
- `schema_set.composition_edges` for include/import/redefine/override graph data
- `schema_set.effective_components` for the final post-composition view

Simple inspection example:

```rust
use xsd_schema::schema::build_dependency_graph;

fn inspect(schema_set: &xsd_schema::SchemaSet) -> Result<(), xsd_schema::SchemaError> {
    for doc in &schema_set.documents {
        let ns = doc
            .target_namespace
            .and_then(|id| schema_set.name_table.try_resolve(id))
            .unwrap_or_else(|| "(no namespace)".to_string());

        println!("document {} -> {}", doc.base_uri, ns);
    }

    for (ns_id, table) in &schema_set.namespaces {
        let ns = ns_id
            .and_then(|id| schema_set.name_table.try_resolve(id))
            .unwrap_or_else(|| "(no namespace)".to_string());

        for (name_id, element_key) in &table.elements {
            let local_name = schema_set.name_table.resolve(*name_id);
            let element = &schema_set.arenas.elements[*element_key];

            println!(
                "global element {{{}}}{} -> {:?}",
                ns,
                local_name,
                element.resolved_type
            );
        }
    }

    let (mut graph, _stats) = build_dependency_graph(schema_set)?;
    graph.sort()?;
    println!("types in dependency order: {}", graph.compilation_order().len());

    Ok(())
}
```

Useful patterns for applications:

- Use `SchemaSet::lookup_type`, `lookup_element`, `lookup_attribute`, and friends
  when you already have namespace/name IDs.
- Use `name_table.add(...)` and `name_table.resolve(...)` to move between
  strings and interned IDs.
- Inspect `schema_set.effective_components` when you need the final visible
  components after `xs:include`, `xs:redefine`, and `xs:override`.
- Use `build_dependency_graph(...)` when you need stable analysis or code
  generation order for derived types.
- Use `SchemaDocument::is_chameleon()` to detect chameleon-adopted documents.
  The distinction is captured by two fields: `declared_target_namespace` (the
  literal `targetNamespace` from the `<xs:schema>` element, `None` if absent)
  and `target_namespace` (effective namespace after chameleon adoption).
  `is_chameleon()` returns `true` when `declared` is `None` but `effective`
  is `Some`.

## 6. Regex Compatibility Mode

By default, pattern facets are validated against the strict XSD Part 2
regex grammar (§F for XSD 1.0, §G for XSD 1.1). Schemas authored against
.NET's `XmlValidatingReader` often contain constructs that are valid in
.NET's regex engine but outside XSD's grammar. The `RegexCompat` enum on
`SchemaSet` lets a caller opt into a permissive mode that mirrors what
.NET actually does: skip the strict grammar validation and let the
runtime regex engine decide what is well-formed.

```rust
use xsd_schema::{RegexCompat, SchemaSetBuilder};

let mut builder = SchemaSetBuilder::new();
builder.set_regex_compatibility(RegexCompat::LenientMs);
// `(?:…)` is .NET syntax, outside the XSD regular-expression grammar.
let compiled = builder
    .add_source(
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
             <xs:simpleType name="code">
               <xs:restriction base="xs:string"><xs:pattern value="(?:ab)+"/></xs:restriction>
             </xs:simpleType>
           </xs:schema>"#,
        "ms-authored.xsd",
    )?
    .compile()?;
# let _ = compiled;
# Ok::<(), xsd_schema::SchemaError>(())
```

### What `LenientMs` does

`LenientMs` matches .NET's behaviour: no XSD
grammar enforcement; the pattern is handed to the regex engine after
the standard `\d`/`\D`/`\w`/`\W`/`\i`/`\I`/`\c`/`\C` substitutions.

Concretely, `LenientMs` does three things:

1. **Skips** `validate_xml_pattern_syntax` — the XSD 1.0 §F hyphen rule
   gate (rejects forms like `[a-c-1]`).
2. **Skips** the XSD dialect of the `xsd11` engine — in `Strict` mode a
   pattern is compiled with regexml's `Regex::xsd(...)`, which rejects
   XPath-only constructs and reads `^` and `$` as ordinary characters,
   as XSD Part 2 §F/§G define them; under `LenientMs` it is compiled with
   regexml's XPath dialect, `Regex::xpath(...)`, where `^` and `$` anchor.
3. **Strips** `(?#...)` comments textually before compilation, because
   neither Rust `regex` nor regexml's `xpath()` recognise that .NET
   syntax. Comments inside character classes are left alone.

Either way the pattern is compiled once and matched against the whole
value (default features: Rust `regex`, in both modes). The Unicode-3.0
`\p{X}` pinning for XSD 1.0 schemas applies in both modes, and so does
the XSD 1.1 rule that a block escape with an unrecognized block name
denotes every character (XSD 1.1 Part 2 §G.4.2.4) — those are versioning
rules, not grammar choices.

### Constructs unlocked under `LenientMs`

| Construct | Default features (Rust `regex`) | `xsd11` feature (regexml `xpath()`) |
| --- | --- | --- |
| `^`/`$` anchors outside char class | works | works |
| `(?:non-capturing)` groups | works | works |
| Reluctant quantifiers `*?`, `+?`, `??`, `{n,m}?` | works | works |
| Backreferences `\1`, `\2`… | engine gap — still rejects | **works** |
| Named groups `(?<name>…)` | works | engine gap — still rejects |
| `(?#…)` inline comments | stripped before compilation | stripped before compilation |

### When to use it

Use `LenientMs` when you have a .NET-source schema you cannot edit and
that uses constructs which are valid in .NET but outside XSD's grammar.
A "passing" schema under `LenientMs` is *not* guaranteed to be portable
to a strictly-conformant XSD validator — that is the trade-off.

The W3C XSD Test Suite is run exclusively in `Strict` mode. The
`msData/regex` group includes tests like `RegexTest_119`, `_929`,
`_930`, `_1413`, `_1414` that verify a strict validator rejects
`(?#...)` comments, and the `expected=Valid` tests in that group do
not exercise the constructs unlocked by `LenientMs` — so wiring the
flag into the harness would be dead code.

## 7. Inspecting A Compiled Content Model

When a document is rejected (or accepted) for a reason that is hard to see in
the schema — a nested repetition, a substitution group, open content, a
counted `maxOccurs` — the inspector shows what the validator actually compiled
for a complex type. It is a diagnostic aid: nothing in it runs on the
validation hot path.

`compiler::inspect_content_model(&SchemaSet, ComplexTypeKey)` returns a
`ContentModelReport` (also `Display`) with three labelled views:

- **Source** — expanded type name, schema document and line/column, content
  type (empty / simple / element-only / mixed), derivation, effective open
  content, and the XSD version in force.
- **Authored particles** — the resolved particle tree as the schema wrote it:
  `sequence` / `choice` / `all` / group references (expanded inline but marked
  as references) / elements with their declaration and type bindings /
  wildcards with namespace constraint and `processContents`, each with its
  `minOccurs..maxOccurs` and whether that range is *unrolled* (max ≤ 16) or
  compiled to a *counter*, plus its source location.
- **Compiled** — the matcher kind (NFA, all-group, or NFA with open content),
  state and transition counts, counters
  (`c0: min..max`, nullable body or not), the initial frontier variant the
  runtime will use, an *exposure* line for models that can reach the execution
  limits, a per-state table (term, transitions including counter operations,
  accept flag, origin) and the substitution-group expansions in play.

The report compiles through the same entry point the validator uses, so the
compiled view **is** the automaton validation executes. Two companions:

- `SchemaValidator::describe_content_model(ct_key)` reads the model the
  validator already prepared instead of recompiling, and — for a type listed by
  `SchemaValidator::content_model_failures()` — names the reason the model
  could not be prepared.
- `compiler::find_complex_type(&SchemaSet, namespace, local)` looks a type up
  by expanded name so callers can start from a QName.

From the command line:

```bash
cargo run --example inspect_content_model -- examples/books.xsd BookForm urn:books
```

The example prints the report for the named type and, if the name is unknown,
lists the named complex types of the schema. Anonymous local types are reported
as `(anonymous)` and can be reached through the key of the declaring element.

## 8. Writing Documents Out And Copying Subtrees

Everything so far reads XML. Two modules in `document` write it (both need the
`xsd11` feature, like the rest of `document`):

- `document::serialize` turns any `DomNavigator` position back into XML text —
  `to_string`, `serialize_node` (one node or one subtree), `serialize_document`
  (the whole document, optionally with an XML declaration). `SerializeOptions`
  chooses compact output (`indent: None`, the default — nothing is added and
  nothing removed, so a parse/serialize round trip is an identity on the tree)
  or formatted output (`indent: Some(n)`).
- `document::copy` adds four methods to `BufferDocumentBuilder` for building a
  new document out of nodes that already exist: `copy_subtree`,
  `copy_attribute`, `append_content` (a sequence of nodes and atomic values,
  with the constructor content rules of XQuery 1.0 §3.7.1.3) and
  `append_atomic`. The copy is a new subtree with new node identities, and it
  carries the namespace declarations its names need — including ones the source
  inherited from ancestors the copy does not have.

Load a document, copy one subtree out of it, write the result both ways:

```rust
# #[cfg(feature = "xsd11")] {
use bumpalo::Bump;
use xsd_schema::document::{serialize, BufferDocument, BufferDocumentBuilder};
use xsd_schema::document::{BufferDocumentOptions, CopyOptions, SerializeOptions};
use xsd_schema::namespace::NameTable;
use xsd_schema::navigator::DomNavigator;

let arena = Bump::new();
let names = NameTable::new();

// 1. Load.
let source = BufferDocument::from_reader_default(
    r#"<catalog xmlns:m="urn:meta"><book m:id="b1"><title>One</title></book><book m:id="b2"><title>Two</title></book></catalog>"#.as_bytes(),
    &arena,
    &names,
)?;

// 2. Copy the first <book> into a document of its own.
let mut book = source.create_navigator();
book.move_to_first_child(); // <catalog>
book.move_to_first_child(); // the first <book>

let mut builder =
    BufferDocumentBuilder::new(&arena, &names, None, BufferDocumentOptions::default())?;
builder.start_element("selection", "", "", &[])?;
builder.end_of_attributes();
builder.copy_subtree(&book, CopyOptions::default())?;
builder.end_element()?;
let selection = builder.finalize()?;

// 3. Write it out. `m` was declared on <catalog> in the source; the copy
//    carries the declaration itself, so the name still resolves.
let nav = selection.create_navigator();
assert_eq!(
    serialize::to_string(&nav, &SerializeOptions::default())?,
    r#"<selection><book xmlns:m="urn:meta" m:id="b1"><title>One</title></book></selection>"#,
);

let formatted = SerializeOptions { indent: Some(2), ..SerializeOptions::default() };
assert_eq!(
    serialize::to_string(&nav, &formatted)?,
    "<selection>\n  <book xmlns:m=\"urn:meta\" m:id=\"b1\">\n    <title>One</title>\n  </book>\n</selection>",
);
# }
# Ok::<(), Box<dyn std::error::Error>>(())
```

`CopyOptions` has three knobs: `namespaces` (whether an element keeps
declarations it does not need), `inherit` (whether the copy leans on the
namespaces in scope where it lands, or re-declares what it needs on its own top
element) and `annotations` (whether schema type annotations are carried over —
only possible within one `SchemaSet`). See
[`BUFFER_DOCUMENT_OVERVIEW.md`](BUFFER_DOCUMENT_OVERVIEW.md) for the
serialization and copying rules in full.

### Typed documents

`document::build_typed_document_with` validates a document and builds it,
typed, in one pass, with your validator, your sink and your handler
(`build_typed_document` is the same with defaults). The tree is the XPath data
model instance of the validation's result (XDM 1.0 §6.2.4, §6.3.4), so it
differs from the text in three ways:

- **defaults as nodes**: each defaulted attribute is an attribute node, and a
  valid empty element that takes its `default` / `fixed` value has a text node
  with it (`TypedBuildOptions::with_defaults_as_nodes(false)` turns this off);
- **white space**: a text node that is all white space is not built in
  element-only content;
- **normalized values**: a valid attribute, and a valid element of a simple
  type or with simple content, has its schema normalized value as its string
  value (`"  a   b "` read as `xs:token` is `"a b"`).

```rust
# #[cfg(feature = "xsd11")] {
use bumpalo::Bump;
use xsd_schema::document::{
    build_typed_document_with, serialize, SerializeOptions, SilentValidationSink,
    TypedBuildOptions,
};
use xsd_schema::validation::{NoopHandler, SchemaValidator, ValidationFlags};
use xsd_schema::SchemaSetBuilder;

let schema_set = SchemaSetBuilder::new()
    .add_source(
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
             <xs:element name="order">
               <xs:complexType>
                 <xs:sequence><xs:element name="qty" type="xs:token"/></xs:sequence>
                 <xs:attribute name="currency" type="xs:string" default="EUR"/>
               </xs:complexType>
             </xs:element>
           </xs:schema>"#,
        "order.xsd",
    )?
    .compile()?
    .into_schema_set();
let validator = SchemaValidator::new(&schema_set, ValidationFlags::default());
let mut runtime = validator.start_run(SilentValidationSink);

let arena = Bump::new();
let built = build_typed_document_with(
    "<order>\n  <qty>  2 </qty>\n</order>".as_bytes(),
    &arena,
    &schema_set,
    &mut runtime,
    &mut NoopHandler,
    TypedBuildOptions::default(),
)?;
runtime.end_validation()?;
assert_eq!(
    serialize::to_string(&built.document.create_navigator(), &SerializeOptions::default())?,
    r#"<order currency="EUR"><qty>2</qty></order>"#,
);
# }
# Ok::<(), Box<dyn std::error::Error>>(())
```

A typed document also sets the XDM is-idrefs property on the nodes whose typed
value holds an `xs:IDREF`, which `fn:idref` reads; a host that builds a tree
itself marks such nodes with `BufferDocumentBuilder::register_idrefs`.

## 9. Composing XML With `xpath!` And `form!`

Sections 1 to 8 read XML, validate it, query it and write it back out. The
`compose` module (feature `compose`, which implies `xsd11`) adds the last
piece: building *new* XML out of query results, in the shape an XQuery FLWOR
expression would — without an XQuery processor anywhere in the crate.

The idea is that a FLWOR expression is three things at once, and two of them
already exist. The XPath 2.0 engine evaluates every expression such a query
needs; Rust iterates, binds and branches; so what was missing was a way to
bind a Rust value to an XPath `$variable`, a way to describe a result element,
and the data-model rules that turn a description into a tree. Those are what
the module supplies: a `Composer` holding the arena, the name table and a cache
of compiled expressions; `xpath!` for evaluation; `pipe` adapters for the
FLWOR clauses (`try_map` is `let`, `try_filter` is `where`, `try_flat_map` is
a nested `for`, `order_by` is `order by`); and a Lisp-style `form!`
constructor whose splices follow the constructor content rules, so an empty
sequence spliced into an element leaves an empty element rather than needing a
host-side conditional.

```rust
# #[cfg(feature = "compose")] {
use bumpalo::Bump;
use xsd_schema::compose::{pipe, Composer};
use xsd_schema::document::SerializeOptions;
use xsd_schema::namespace::NameTable;
use xsd_schema::{form, xpath};

let arena = Bump::new();
let names = NameTable::new();
let c = Composer::new(&arena, &names);
let stock = c.load_str("<stock><part qty='7'>bolt</part><part qty='0'>nut</part></stock>")?;

// for $p in //part where $p/@qty > 0 return <part>{ string($p) }</part>
let rows = pipe::nodes(xpath!(c, "//part", stock)?)
    .try_filter(|p| xpath!(c, "@qty > 0", p)?.boolean())
    .try_map(|p| Ok(form! { (part ^{ xpath!(c, "string()", &p)? }) }));

assert_eq!(
    c.build(form! { (in_stock ..?^{ rows }) })?.to_xml(&SerializeOptions::default())?,
    "<in_stock><part>bolt</part></in_stock>",
);
# }
# Ok::<(), Box<dyn std::error::Error>>(())
```

Everything the layer offers — the four `xpath!` call forms and every value that
can be bound, the whole `form!` grammar, the pipeline contract and what is not
lazy, the `order by` rules, `build` versus `build_sequence`, serialization,
the error variants and the v1 limits — is in its own guide:
[Composing XML from Rust](COMPOSE.md). The relational use case of the XQuery
1.0 test suite is rewritten query by query in `tests/compose_usecase_r.rs`,
and `examples/xquery_without_xquery.rs` runs three of them end to end.

## Recommended Reading Order

If you are new to the crate, this sequence usually works well:

1. Start with `load_and_process_schema` or `SchemaSetBuilder`.
2. Add `SchemaValidator` if you need instance validation.
3. Add `XPathExpr` and `XPathContext` if you need XPath/XSD 1.1 features.
4. Move down to `SchemaResolver`, `ReferenceResolver`, and the raw schema model
   only when you need custom loading or custom analysis.
5. Add `Composer`, `xpath!` and `form!` if you need to build XML out of query
   results (section 9).
