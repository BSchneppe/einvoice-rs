# xsd-schema Architecture Overview

**Crate**: `rust/xsd-schema`  

This document is the architectural map for contributors. It explains how data
moves through the crate, which module owns which phase, and which invariants
must be preserved when changing behavior.

Use 
* [`INTRODUCTION.md`](INTRODUCTION.md) for public API examples, 
* [`EXTENSIBILITY.md`](EXTENSIBILITY.md) for
extension points, 
* [`BUFFER_DOCUMENT_OVERVIEW.md`](BUFFER_DOCUMENT_OVERVIEW.md) for the schema-aware
  XPath/assertion document store, 
* [`IMPLEMENTATION_LIMITS.md`](IMPLEMENTATION_LIMITS.md) for the bounds of the value
  types and of nesting, and 
* [`UNSAFE.md`](UNSAFE.md) for unsafe-code invariants.

---

## Purpose

`xsd-schema` is a Rust implementation of XSD 1.0 and XSD 1.1 schema processing
and instance validation. It parses schema documents, composes include/import/
redefine/override graphs, resolves QName references into typed component keys,
compiles content models, and validates XML instances through a push-event API.

With the `xsd11` feature, the crate also provides:

- XPath 2.0 evaluation.
- XSD 1.1 assertions and type alternatives.
- `BufferDocument`, a compact schema-aware XML document store used by XPath and
  assertion evaluation.
- `regexml`-backed XSD/XPath regex support.

---

## Feature And Version Boundaries

Cargo features control which code is compiled. `SchemaSet` controls which XSD
semantics are used at runtime.

| Boundary | Meaning |
| --- | --- |
| `SchemaSet::new()` | Runtime XSD 1.0 semantics. Use this even with `--features xsd11` when running the XSD 1.0 conformance suite. |
| `SchemaSet::xsd11()` | Runtime XSD 1.1 semantics. Enables XSD 1.1 rules in the schema model and validator. |
| default features | XSD 1.0 processing plus default Unicode normalization support; no XPath/document modules. |
| `xsd11` feature | Compiles XPath 2.0, `document/`, XSD 1.1 code paths, and `regexml`. |
| `async` feature | Adds async schema-loading/directive-resolution I/O. It does not make parsing, resolution, compilation, or validation async. |

Conformance tests should normally be run with `--features xsd11` for both XSD
versions because the feature enables the conformance-grade regex backend. The
runtime version is still selected by the test harness through `SchemaSet::new()`
or `SchemaSet::xsd11()`.

**`fixed` values in XSD 1.0 mode.** In both modes a `fixed` value constraint
compares values as Structures 1.1 says — "equal or identical" (element and
attribute `fixed`, and the `fixed` checks of a restriction), and "identical"
for an attribute use against its declaration in XSD 1.1 (equal in 1.0 mode):
`NaN` matches `NaN`, `0` matches `-0`, two date/time spellings of one instant
in different time zones match, durations compare by months and seconds,
`xs:QName` / `xs:NOTATION` values compare by expanded name (each read with the
namespace bindings where it is written), and values of different primitive
types never match. In XSD 1.0 mode this departs from the letter of two XSD 1.0
clauses that compare strings — Attribute Locally Valid (Use): "its ·normalized
value· must match the canonical lexical representation of the attribute use's
{value constraint} value"; and derivation-ok-restriction 2.1.3.2: "R's
·effective value constraint· is fixed with the same string as B's" — so values
equal in value space but spelled differently (`13:00:00+01:00` against
`fixed="12:00:00Z"`, `PT60S` against `fixed="PT1M"`) are accepted there too.
No W3C XSD 1.0 test tells the two readings apart.

One case follows each version's own text: an element's content under an
`xsi:type` or a type alternative against its declaration's `fixed` {value}
(Element Locally Valid (Element) clause 5.2.2.2.2). XSD 1.1 requires the
actual value to be equal or identical to the {value}; XSD 1.0 requires it to
match the {value}'s canonical lexical representation, so in 1.0 mode a {value}
of another primitive datatype, a string {value}, or a list against an atomic
value matches through that form (Datatypes 1.0 §2.5.1, §3.2.x.2: a dateTime in
`Z`, the seconds as written), read with the element type's white space and no
facet of the element's type applied to it.

---

## Processing Pipeline

The central object is `SchemaSet`. Before processing completes it is mutable
and accumulates parsed documents, components, namespace tables, composition
edges, and source maps. After processing, validation treats it as immutable and
stores per-instance mutable state in `ValidationRuntime`. The XPath/document
layer (compiled with `xsd11`) hangs off the same `SchemaSet` for assertions,
type alternatives, identity helpers, and standalone XPath use.

```text
schema bytes / locations
        |
        v
well-formedness layer over quick-xml (see below)
src/xml/
        |
        v
parse + assemble
src/parser/parse.rs
src/parser/frames/
src/parser/assemble.rs
        |
        v
SchemaSet with documents, arenas, namespace tables,
composition directives, SourceRef-backed diagnostics
        |
        v
directive loading and composition
src/parser/resolver.rs
src/schema/composition.rs
src/schema/redefine.rs
src/schema/override_dir.rs
        |
        v
inline type and local particle allocation
src/schema/inline.rs
        |
        v
nesting check (SchemaSet::max_depth)
src/schema/nesting.rs
        |
        v
QName reference resolution
src/schema/resolver.rs
        |
        v
schema-time checks and content model compilation
src/pipeline.rs
src/schema/derivation/
src/schema/cta.rs (XPath of assertions and type alternatives)
src/compiler/
        |
        v
push-event instance validation
src/validation/validator.rs
src/validation/runtime.rs
        ^
        |
streaming driver (namespaces, xsi: attributes, text runs, hooks)
src/validation/quick_xml_driver.rs
        ^
        |
well-formedness layer over quick-xml (XML 1.0 / 1.1, Namespaces,
declarations, parse limits) — shared with the document builder
src/xml/
        ^
        |
instance bytes
```

**One walk, one sequence.** Every entry point — `load_and_process_schema`,
`SchemaSetBuilder::compile`, `process_loaded_schemas` and the asynchronous
variants — runs the same phases in the same order (`process_loaded_schemas` in
`src/pipeline.rs`), and directive resolution is one walk over the directives
(`src/parser/resolver.rs`) that applies the same rules whether the documents are
read synchronously or prefetched by an `AsyncSchemaLoader`. The walk does not
recurse: `ResolverConfig::max_depth` bounds the work, not the stack. Locations
are identified as normalized RFC 3986 references, after the location rewriter
(if any) has answered, so one document is loaded once however it is named.

### Phase Ownership

| Phase | Main code | Input | Output / mutation |
| --- | --- | --- | --- |
| Parse | `src/parser/parse.rs`, `src/parser/frames/` | XML schema bytes | Frame results with names, namespace snapshots, source locations. |
| Assemble | `src/parser/assemble.rs` | Parser frame results | `SchemaDocument`, top-level arena components, namespace tables, unresolved `QNameRef`s. |
| Directive resolution | `src/parser/resolver.rs`, `src/parser/uri.rs` | `xs:include`, `xs:import`, `xs:redefine`, `xs:override` | Additional documents, loaded-location caches, composition edges, `DirectiveFailure`s for documents that could not be read. |
| Composition | `src/schema/redefine.rs`, `src/schema/override_dir.rs`, `src/schema/composition.rs` | Parsed documents and edges | Effective component graph after redefine/override rules. |
| Inline allocation | `src/schema/inline.rs` | Inline type definitions and local particles | Arena keys for inline types and local element declarations. |
| Nesting check | `src/schema/nesting.rs` | Components and their references | `SchemaLimit::ComponentDepth` refusals before any pass recurses along a reference chain. |
| Reference resolution | `src/schema/resolver.rs` | `QNameRef`s | Typed keys in `resolved_*` fields, plus `src-resolve` diagnostics. |
| Schema checks | `src/pipeline.rs`, `src/schema/derivation/`, `src/schema/edc.rs`, `src/schema/cta.rs` | Resolved schema model | Derivation (restriction with exact occurrence counts, `derivation/counts.rs`), value constraint, EDC, UPA, the XPath of assertions and type alternatives, and structural errors. |
| Compilation | `src/compiler/` | Resolved particles and groups | `NfaTable`, `AllGroupModel`, substitution-group maps, open-content matchers. |
| Validation | `src/validation/` | Compiled `SchemaSet` + XML events | `SchemaInfo`, typed values, validation diagnostics, ID/IDREF and identity-constraint state. |

---

## Core Data Structures

### SchemaSet

`SchemaSet` in `src/schema/model.rs` owns all schema state:

- `name_table`: interned strings for names and namespace URIs.
- `source_maps`: document bytes and byte/line mapping for diagnostics.
- `documents`: every parsed schema document.
- `namespaces`: per-target-namespace component lookup tables.
- `arenas`: flat storage for schema components.
- `loaded_locations` and `chameleon_cache`: directive-resolution caches.
- `composition_edges`: include/import/redefine/override relationships.
- `xsd_version`: runtime XSD 1.0 vs 1.1 semantics.
- `regex_compatibility`: strict XSD regex or narrow compatibility mode.

### SchemaArenas

`SchemaArenas` in `src/arenas.rs` stores each component family in a separate
`SlotMap` with typed keys:

- simple and complex types
- elements and attributes
- model groups and attribute groups
- notations
- identity constraints

The design avoids Rust reference cycles. Components store unresolved lexical
references as `QNameRef` during parse/assemble phases, then store resolved
typed keys in parallel `resolved_*` fields after `resolve_all_references`.

### SourceRef

`SourceRef` ties diagnostics back to the lexical schema document and is load-
bearing for per-document QName visibility: a QName resolves in the context of
the document that lexically contained it, not any document sharing the same
`SchemaSet`.

### NameTable

`NameTable` interns strings and returns stable `NameId`s. It holds two of the
crate's `unsafe` blocks (`resolve_ref` / `try_resolve_ref` return `&str` into
interned data without allocating, so `DomNavigator` methods don't need to
allocate). The third lives in `ValidationRuntime::begin_assertion_buffering`,
which extends the lifetime of a `Bump` arena borrow used by the
self-referential fragment builder. Every site and its contract is listed in
`doc/UNSAFE.md`.

---

## Validation Architecture

Validation is event-driven. `SchemaValidator` owns immutable configuration and
starts a `ValidationRuntime` for each instance validation run.

```text
SchemaValidator
    |
    v
ValidationRuntime
    |
    +-- element stack and SchemaInfo
    +-- content-model state (NFA, all-group, open content)
    +-- attribute and wildcard validation
    +-- simple type and facet validation
    +-- identity constraint tables
    +-- ID/IDREF registries
    +-- optional XSD 1.1 assertion buffering
```

**The validation root.** A run assesses its root as `ValidationRoot` says
(Structures 1.1 §5.2): by the declaration of its name (the default), against a
stipulated element declaration or type definition, or laxly. The mode only
decides the root's governing declaration and type in
`ValidationRuntime::validate_element`; everything below the root is validated
as usual, and every driver that takes the caller's runtime honours it.

**Attribute uses.** The end-of-attributes step walks the governing type's
{attribute uses} as Structures §3.4.2.4 builds them — inherited uses, attribute
groups, a restriction's prohibitions — once, for every consumer: missing
required attributes (`cvc-complex-type.4`), defaulted attributes (reported to
the caller and added to the PSVI), and the defaulted values that take part in
identity constraints and in the ID/IDREF and ENTITY checks. The schema-time
restriction check reads the same {attribute uses} on both sides.

Important runtime modules:

| Path | Responsibility |
| --- | --- |
| `src/validation/validator.rs` | Public push-event API and validator configuration. |
| `src/validation/runtime.rs` | Main per-run validation engine. |
| `src/xml/` | The well-formedness layer over `quick-xml` that every text reader sits on: XML 1.0 / 1.1 and Namespaces rules, the XML and document type declarations, `ParseLimits`, the `NotWellFormed` / `LimitExceeded` errors. |
| `src/validation/quick_xml_driver.rs` | The streaming driver on that layer: turns its checked events into the runtime push API (namespace scope, `xsi:` attributes, namespace snapshots, text runs) and calls the `ValidationEventHandler` hooks. |
| `src/validation/content.rs` | Content-model state transitions. |
| `src/validation/simple.rs` | Simple type, list/union, atomic value, and facet validation. |
| `src/validation/identity.rs` | Streaming `key`, `unique`, and `keyref` evaluation; each keyref is checked against the identity-constraint table of the element that declares it (its own and its descendants' keys, Structures §3.11.4 clause 4.3), through hashed tables. |
| `src/validation/active_axis.rs` | Efficient selector/field axis matching for identity constraints. |
| `src/validation/assertions.rs` | XSD 1.1 assertion evaluation. |
| `src/validation/alternatives.rs` | XSD 1.1 type alternative selection. |
| `src/validation/hint_loader.rs` | `xsi:schemaLocation` / `xsi:noNamespaceSchemaLocation` enrichment. |

`ValidationFlags` controls optional runtime behavior. Identity constraints are
always enforced (the `PROCESS_IDENTITY_CONSTRAINTS` bit is deprecated and has no
effect). XSD 1.1 assertions are evaluated when `PROCESS_ASSERTIONS` is set —
`SchemaValidator::new` honours it, and `SchemaValidator::new_fragment_buffer`
sets it.

### Reading XML: The Well-Formedness Layer And The Streaming Driver

The push-event API on `ValidationRuntime` (`validate_element`,
`validate_attribute`, `validate_end_of_attributes`, `validate_text`,
`validate_whitespace`, `validate_end_element`, `end_validation`) is
deliberately XML-source-agnostic. Reading XML text into it takes two layers,
each owning its part once:

```text
bytes --> quick_xml::Reader --> xml layer -----------> quick_xml_driver ----------> ValidationRuntime
          (tokenizes)            (src/xml/)              (src/validation/)            push API
                                 well-formedness,        xmlns scope -> URIs,         (validate_element,
                                 XML 1.0 / 1.1,          xsi:type / xsi:nil,          validate_attribute,
                                 Namespaces,             namespace snapshots,         ...)
                                 declarations,           text runs joined,
                                 parse limits,           ValidationEventHandler hooks
                                 attribute spans
                                     |
                                     +--> BufferDocumentBuilder::build  (BufferDocument::from_reader)
                                     +--> TrackedReader                 (schema documents)
```

`src/xml/` is the one place where the crate reads XML text: the driver, the
document builder and the schema-document parser all sit on it. It checks each
event before any of them sees it, so nothing of a refused construct reaches
code that interns names or builds nodes:

| Where | What |
| --- | --- |
| `quick-xml` | Finds the markup boundaries; matches end tags to start tags. |
| `src/xml/` | Document structure (one root, nothing but white space, comments and PIs outside it, truncated input); names and qualified names; tag syntax; attribute uniqueness, also by expanded name; references (characters of any length, the five predefined entities); characters (`Char`, UTF-8); the XML declaration (its grammar; it selects XML 1.0 or 1.1 rules) and the document type declaration (placement and grammar, not processed); the namespace constraints (every prefix bound, reserved prefixes and names, undeclaring); `ParseLimits`. Every refusal is `xml::NotWellFormed` (rule, byte position, version), every limit `xml::LimitExceeded`. |
| `quick_xml_driver` | Namespace resolution to URIs, `NamespaceContextSnapshot`s, `xsi:type` / `xsi:nil`, joining and classifying text runs, the hook order. |
| `ValidationRuntime` | Validation. |

The rules follow the XML declaration: XML 1.1 and Namespaces in XML 1.1 for
`version="1.1"`, XML 1.0 and Namespaces in XML 1.0 otherwise. The checks are
always on (XML 1.0 §1.2 makes a well-formedness violation a fatal error); the
limits are opt-in — `ValidationRuntime::set_parse_limits` for the driver and
the typed build, `BufferDocumentBuilder::with_parse_limits` for the builder.

The driver exposes two layers:

- **Layer 1 (turn-key):** `drive_quick_xml` / `drive_quick_xml_in`.
  Streams a reader into the runtime, then calls
  `runtime.end_validation()`. Diagnostics arrive through the runtime's
  `ValidationSink`. The XML declaration and a document type declaration in
  the prolog are accepted, comments and processing instructions dropped.
  Use this when all you want is validation diagnostics.
- **Layer 2 (handler-driven):** `drive_quick_xml_with` /
  `drive_quick_xml_with_in` plus a caller-supplied
  `ValidationEventHandler`. Every method on the trait has a no-op default,
  so a handler that only cares about (say) end-of-element fires exactly
  that one method. The XML declaration (`on_xml_declaration`), a document
  type declaration in the prolog (`on_doctype`, once its grammar has been
  checked), comments, PIs and source-span offsets flow through dedicated
  hooks; a hook that returns an error refuses the document. Layer 2 does
  **not** call `end_validation`; the caller does so after collecting any
  post-stream runtime state (e.g. `schema_location_hints`,
  `no_namespace_schema_location_hints`).

Errors are `DriveError` / `DriveWithError`: `NotWellFormed` for every
refusal of the text, `LimitExceeded` for a limit, `Io` for an I/O error of
the source (its `std::io::Error`), `Hook` for a handler's error; the document builder and
the typed build report the same through `BufferDocumentError`. Offsets — the
span hooks, `DoctypeView::byte_pos`, source spans and error positions — are
byte offsets into the input, a leading byte-order mark included.

In-tree consumers:

- `tests/conformance/driver.rs::validate_instance_pass` — layer 2 with a
  small handler that only records root validity. Hint collection and
  `end_validation` ordering stay in the caller.
- `src/document/typed_builder.rs` — layer 2 with a handler
  (`TypedBuilderHandler`, public) that drives a `BufferDocumentBuilder`,
  preserving comments, PIs, source spans, and CTA-deferred attribute
  bindings, and forwards every event to a caller's handler;
  `build_typed_document_with` runs it with the caller's validator and sink,
  `build_typed_document` with defaults.
- `doc/INTRODUCTION.md` — layer 1 example for the public-facing common
  case.

External integrators that need to mirror events into a custom DOM,
track spans, or otherwise interleave bookkeeping with validation should
implement `ValidationEventHandler` rather than open-coding the
`quick-xml` → push API translation.

### Hint-Driven Schema Enrichment (Two-Pass Validation)

The XSD spec says processors SHOULD attempt to locate schemas from
`xsi:schemaLocation` / `xsi:noNamespaceSchemaLocation` hints. Because
`ValidationRuntime` borrows `&SchemaSet` immutably, schema loading cannot
happen mid-validation — no schemas are loaded during an active run. Instead,
hints are accumulated during a first pass (each carrying the instance base
URI for relative resolution), then used to build an enriched schema set for a
second pass.

`enrich_schema_set` is the turn-key path. It re-loads the original schemas via
`SchemaSetBuilder::add_from()` — reading back the original set's recorded
locations, so the caller need not track file paths — adds the hinted schemas,
and compiles once:

```rust
use xsd_schema::validation::{ValidationRuntime, ValidationSink};
use xsd_schema::{enrich_schema_set, SchemaSet};

fn second_pass<S: ValidationSink>(schema_set: &SchemaSet, runtime: &ValidationRuntime<'_, S>) {
    // First pass: validate and collect hints
    let sl = runtime.schema_location_hints().to_vec();
    let nnsl = runtime.no_namespace_schema_location_hints().to_vec();

    let outcome = enrich_schema_set(schema_set, &sl, &nnsl);
    if let Some(enriched) = outcome.schema_set.as_ref() {
        // Second pass: re-validate with the enriched schema set
        let _ = enriched;
    } else if let Some(err) = outcome.compile_error.as_ref() {
        eprintln!("hint enrichment failed at recompile: {err}");
    } else if !outcome.hint_errors.is_empty() {
        // Hints were given but every one failed to load; the original
        // SchemaSet is still usable without enrichment.
        for e in &outcome.hint_errors {
            eprintln!("hint load failure: {e}");
        }
    }
}
```

The returned `EnrichmentOutcome` separates every failure mode the older
`Option<SchemaSet>` shape conflated: `schema_set` (present only when the
recompile succeeded), `hint_errors` (per-hint load failures — non-fatal, so
partial success is possible), and `compile_error` (the recompile error, if
any). `is_no_op()` distinguishes "no hints to apply" from "tried but failed".
For finer control, drive a `SchemaSetBuilder` directly and inspect
`HintLoadResult::errors`:

```rust
use xsd_schema::validation::{NoNamespaceSchemaLocationHint, SchemaLocationHint};
use xsd_schema::{load_hints_into_builder, CompiledSchemaSet, SchemaSet, SchemaSetBuilder};

fn enrich_by_hand(
    schema_set: &SchemaSet,
    sl_hints: &[SchemaLocationHint],
    nnsl_hints: &[NoNamespaceSchemaLocationHint],
) -> Result<CompiledSchemaSet, xsd_schema::SchemaError> {
    let mut builder = SchemaSetBuilder::new();
    builder.add_from(schema_set); // re-load original schemas
    let loaded = load_hints_into_builder(&mut builder, sl_hints, nnsl_hints);
    for e in &loaded.errors {
        eprintln!("hint load failure: {e}");
    }
    builder.compile() // re-validate with compiled.schema_set()
}
```

**Base URI.** Set an absolute (canonicalized) path via
`ValidationRuntime::set_instance_base_uri` before the first pass. Relative
paths with `..` components can break hint resolution, because the resolver
joins each hint location against the base URI's directory.

**Driving the second pass.** Enrichment only produces a schema set; the
instance still has to be validated against it. The push API is
source-agnostic, so either driver works:

- *Re-parse with `quick-xml`* (no `xsd11` requirement): keep the instance
  bytes and call `drive_quick_xml` again with the enriched set. Simplest, but
  the document is parsed twice.
- *Re-walk a single parse* (`xsd11`): parse the instance once into a
  `BufferDocument`, then validate it against the original and enriched sets by
  re-walking the in-memory tree with `drive_navigator` /
  `drive_buffer_document` (`src/validation/navigator_driver.rs`), iteratively,
  so a tree of any depth costs heap, not stack. The
  tree-walk driver emits the same push-API sequence as the streaming driver
  (same `NamespaceContextSnapshot`, `xsi:*` discovery, whitespace/character
  split), so both passes produce identical diagnostics from one parse. See
  *BufferDocument* below; `drive_navigator` is generic over `DomNavigator`, so
  it also re-validates a `roxmltree` document through `RoXmlNavigator`.

### XSD 1.1 Assertion Buffering

`xs:assert` does not require preloading the instance into a DOM. When an
element whose governing complex type has assertions is opened,
`ValidationRuntime` starts a scoped `BufferDocumentBuilder` in
`DocumentKind::Fragment` mode and feeds subsequent events for that subtree
into it. When the element closes, the fragment is finalized, XPath
assertions are evaluated with the asserted element as context item, and the
temporary arena is released.

The test of each assertion and type alternative is checked when the schema
is built, in the static context Structures 1.1 §3.13.6.2 (XPath Valid) gives
it — the built-in components only (the built-in types and the four `xsi:`
attribute declarations, §3.2.7), so a test naming a user-defined type or
declaration makes the schema invalid (`src/schema/cta.rs`). Evaluation compiles
the test again with the schema set, which it needs for the type annotations of
the instance's nodes.

The public API stays push-based; XPath only sees a navigable fragment for
the scope that needs one. A fully built typed `BufferDocument` is an
optional alternative for consumers that already own a document model.

---

## Content Models

Complex-type content models are compiled in `src/compiler/`.

| Concept | Implementation |
| --- | --- |
| sequence/choice/general particles | NFA fragments and `NfaTable` in `nfa.rs`, `particle.rs`, and `compile.rs`. |
| `xs:all` | Dedicated `AllGroupModel` bitmask/counter matcher in `all_group.rs`; not lowered to an ordinary NFA at runtime. |
| UPA / `cos-nonambig` | `upa.rs`, with conformance-specific handling for counters and substitution groups. |
| substitution groups | `substitution.rs`; runtime and UPA maps are built from resolved head/member links. |
| XSD 1.1 open content | `open_content.rs`; supports interleave and suffix modes. |

Content-model changes often require checking both schema-time compilation and
runtime matching. In particular, local element declarations and model-group
particles have their own allocation/resolution paths in `src/schema/inline.rs`
and `src/schema/resolver.rs`.

---

## XPath And Document Layer

The XPath/document layer is compiled only with `xsd11`.

### DomNavigator

`DomNavigator` is the abstraction that lets XPath run over different XML
backends. It is defined in `src/navigator/mod.rs` (compiled unconditionally,
since validation also uses it). The crate ships:

- `RoXmlNavigator` (`src/navigator/roxmltree.rs`): adapter over `roxmltree`,
  used heavily in tests and conformance harnesses.
- `BufferDocNavigator` (`src/document/navigator.rs`, `xsd11`): adapter over
  `BufferDocument`, used for schema-aware document and assertion evaluation.

### BufferDocument

`BufferDocument` in `src/document/` is a compact flat-array XML store with
schema binding metadata. It is read-only after construction and exists to make
XPath 2.0 navigation and XSD 1.1 assertion evaluation efficient without tying
the XPath engine to `roxmltree`.

It supports two modes: a **fragment** built on the fly during streaming
validation (see *XSD 1.1 Assertion Buffering* above), and a **full document**
when a caller already owns one and wants post-parse traversal.

The `BufferDocument` is not just a
structural DOM. Each node carries schema type information through the
20-bit `binding_index` into a per-document `BindingRemapTable`, where each
entry pins down the resolved type, the governing element/attribute
declaration, and the content type. Because of this, `BufferDocNavigator`
exposes *typed* values — `XmlAtomicValue` / `XmlValue` from `src/types/`,
not just lexical strings. A typed build makes the document the XDM instance
of the PSVI: defaulted attributes are attribute nodes, an empty element that
takes its declaration's default or fixed value has a text node with it,
white space in element-only content is no text node, and simple values have
their schema normalized value as their string value (`TypedBuildOptions`).
`typed_value()` still reads the declaration's default for an empty element
that has no such text node (a tree built with defaults as nodes off).
Consequences for callers:

- XPath 2.0 expressions and XSD 1.1 assertions consume schema-typed values
  directly; no re-parsing per query, and no separate cast layer.
- The shared datatype layer in `src/types/` (see the *One datatype layer
  for both engines* invariant) is what makes this work — atomic values
  produced by the validator and atomic values pulled by the navigator are
  the same objects.
- `RoXmlNavigator` over `roxmltree` is structurally equivalent but
  *untyped*: it cannot answer typed-value queries. Code that needs typed
  navigation must use `BufferDocNavigator`.

### XPath 2.0 Engine

`src/xpath/` contains:

- LALRPOP grammar and lexer.
- AST and name binder.
- static/dynamic context.
- evaluator and axis iterators.
- function registry and built-in functions.
- typed value and sequence/operator logic.

XPath functions use `DomNavigator` and share the datatype layer in
`src/types/` with the XSD validator (atomic values, type codes, casting,
equality, facets, `VALIDATOR_REGISTRY`). They should not depend on a concrete
DOM implementation, and they should not reimplement type machinery — see the
*One datatype layer for both engines* invariant below.

---

## Module Map

| Path | Responsibility |
| --- | --- |
| `src/lib.rs` | Public API surface and re-exports. |
| `src/builder.rs` | `SchemaSetBuilder` fluent load/compile API. |
| `src/pipeline.rs` | High-level orchestration for parse, load, compose, resolve, and checks. |
| `src/error.rs` | `SchemaError`, `SchemaLimit`, `FacetError`, and diagnostic codes (instance errors are `validation::ValidationError`, `src/validation/errors.rs`). |
| `src/ids.rs` | Typed arena keys. |
| `src/arenas.rs` | Flat component storage. |
| `src/embedded.rs` | Embedded catalog schemas such as XML and XLink. |
| `src/regex_convert.rs` | XSD regex translation for the Rust `regex` backend. |
| `src/regex_xsd_unicode.rs` | Unicode 3.0 tables for XSD 1.0 regex compatibility. |
| `src/xml/` | The well-formedness layer over `quick-xml` shared by every text reader; `ParseLimits`. |
| `src/parser/` | Schema XML parsing, frame state machine, assembly, loaders, the location rewriter and `DirectiveFailure`, source maps. |
| `src/parser/uri.rs` | RFC 3986 reference resolution and normalization of schema locations (crate-private). |
| `src/schema/` | Schema model, composition, inline allocation, reference resolution, derivation, EDC. |
| `src/schema/nesting.rs` | The component nesting check (`SchemaSet::max_depth`). |
| `src/schema/derivation/` | Type derivation and restriction checks; `counts.rs` keeps the exact occurrence counts of repeated particles. |
| `src/schema/cta.rs` | Type alternatives, and the schema-time XPath Valid check of assertions and type alternatives (built-in components only). |
| `src/schema/value_constraint.rs` | `SchemaSet::element_value_constraint(_for_type)`: an element declaration's {value constraint} for hosts. |
| `src/types/` | Built-in type registry, XSD atomic values, facets, conversions, equality. |
| `src/types/value_order.rs` | The order of the date/time and duration value spaces, exact, shared by facets, value constraints and identity constraints. |
| `src/compiler/` | Content-model compilation, UPA, all-groups, substitution groups, open content. |
| `src/validation/` | Push-event instance validation and PSVI output. |
| `src/namespace/` | `NameTable`, QName helpers, namespace context snapshots. |
| `src/navigator/` | DOM adapters exposed outside the XPath module. |
| `src/xpath/` | XPath 2.0 parser, binder, evaluator, functions, operators (`xsd11`). |
| `src/xpath/depth.rs` | `MAX_EXPRESSION_DEPTH` and the iterative depth measure every parse checks. |
| `src/document/` | `BufferDocument` and typed document support (`xsd11`). |
| `src/compose/` | `Composer`, `xpath!`, `form!` and the pipeline adapters (`compose`). |
| `tests/conformance/` | W3C XSD conformance driver and reporting. |
| `tests/xqts/` | XPath/XQuery test-suite driver (`xsd11`). |

---

## Architectural Invariants

- **Runtime version is not the cargo feature**: `xsd11` compiles code; `SchemaSet`
  selects XSD 1.0 or 1.1 semantics.
- **QName visibility is lexical-document scoped**: references must be checked
  using the `SourceRef` document that contained the QName.
- **Arena keys are the component identity**: do not store long-lived references
  to arena values; store typed keys.
- **Parser output may contain unresolved names**: parse/assemble should preserve
  `QNameRef`, namespace snapshots, annotations, and source locations. Resolution
  belongs in later phases.
- **Bypassing `ReferenceResolver` is risky**: any fast path that calls
  `lookup_type`, `lookup_element`, or namespace tables directly must still
  enforce namespace visibility and version semantics.
- **Validation state is per run**: never store instance-validation state in
  `SchemaSet`.
- **Leniency must be explicit**: compatibility behavior belongs behind a named
  flag or narrow internal rule with tests; strict conformance is the default.
- **XSD 1.1 code must be feature-gated**: public and internal dependencies on
  XPath, `BufferDocument`, `regexml`, and assertion machinery require
  `#[cfg(feature = "xsd11")]`.
- **Unsafe code is exceptional**: new unsafe blocks require a documented invariant
  in `doc/UNSAFE.md` and Miri coverage.
- **Nesting is bounded everywhere**: every pass that recurses along a schema
  document, a component chain or an XPath expression is protected by a limit
  checked before it recurses (`ParserConfig::max_depth`, `SchemaSet::max_depth`,
  `xpath::MAX_EXPRESSION_DEPTH`); a walk that does not recurse (the directive
  walk) is bounded in work (`ResolverConfig::max_depth`). New recursive passes
  must sit behind one of them. Walks over an instance tree do not recurse at
  all — the streaming and tree-walk drivers, `fn:deep-equal` / `TreeComparer`,
  the copy and the serializer keep their depth on the heap — because a tree a
  host builds is bounded by no limit; new ones must not recurse either.
- **Diagnostics are deterministic**: the error a schema or an instance is
  refused with does not vary from run to run — no hash order decides which
  component is checked first, which competing pair `cos-nonambig` names (the
  first in content-model order), or which enumeration value an error names
  (sorted). Iterate hash maps only where the order cannot reach a message.
- **One datatype layer for both engines**: `src/types/` is the single source of
  atomic values, type codes, casting, equality, facets, and the validator
  registry — shared by the XSD validator and the XPath 2.0 engine. XPath does
  not own its own atomic-value or cast hierarchy: when XPath needs `xs:date`
  arithmetic, a `Decimal` cast, a facet-aware comparison, or a validator,
  it goes through `XmlAtomicValue`, `XmlTypeCode`, `types::convert`,
  `types::equality`, and `VALIDATOR_REGISTRY`. Before adding type, cast, or
  comparison logic under `src/xpath/`, check whether the equivalent already
  exists in `src/types/` and extend that instead. Reason: XSD 1.1 assertions
  evaluate XPath against schema-typed values, so divergence between the two
  layers shows up as conformance bugs that are hard to localize.

---

## Where To Start

| Task | Start with | Usually also inspect |
| --- | --- | --- |
| Public API use or examples | `doc/INTRODUCTION.md` | `src/lib.rs`, `src/builder.rs` |
| Schema loading/import bugs | `src/parser/resolver.rs` | `src/parser/uri.rs`, `src/pipeline.rs`, `src/schema/composition.rs`, `src/embedded.rs` |
| Parser support for an XSD element | `src/parser/frames/` | `src/parser/assemble.rs`, `src/schema/model.rs` |
| QName resolution / `src-resolve` | `src/schema/resolver.rs` | `src/parser/location.rs`, `src/schema/model.rs` |
| Inline type or local particle issues | `src/schema/inline.rs` | `src/schema/resolver.rs`, `src/compiler/compile.rs` |
| Type derivation or value constraints | `src/schema/derivation/` | `src/types/`, `src/validation/simple.rs` |
| Content model or UPA failures | `src/compiler/` | `src/validation/content.rs`, `src/pipeline.rs` |
| Instance validation behavior | `src/validation/runtime.rs` | `src/validation/simple.rs`, `src/validation/info.rs` |
| Well-formedness or parse limits | `src/xml/reader.rs` | `src/xml/chars.rs`, `src/validation/quick_xml_driver.rs`, `tests/well_formedness.rs` |
| Identity constraints | `src/validation/identity.rs` | `src/validation/active_axis.rs`, `src/validation/identity_parser.rs` |
| XSD 1.1 assertions | `src/validation/assertions.rs` | `src/document/`, `src/xpath/` |
| XPath engine work | `src/xpath/` | `src/navigator/`, `src/document/` |
| Unsafe-code review | `doc/UNSAFE.md` | `src/namespace/table.rs`, `src/validation/runtime.rs` |