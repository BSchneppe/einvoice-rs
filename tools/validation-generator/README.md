# Offline validation source frontend

This unpublished Rust 1.83 workspace implements the first BSC-7 frontend slice for
`xrechnung-3.0.2-cii-d16b-kosit-2026-08-31`. Both packages have `publish = false`.
It is outside the consumer workspace and has its own lockfile. It emits
**non-executable inspection data**. It cannot validate invoices, install a target,
or make the existing facade available. It does not run Java, Xee or source XSLT.

## Reproduce

From the repository root, put these **exact archive files** in a local directory:
`cen.zip`, `xr.zip`, `config.zip`, `validator.jar`, `cen-source.zip`. Versions and
SHA-256 pins come exclusively from `../oracle/artifacts.lock.json`. No URL is
resolved by the Rust compiler. `cen-source.zip` supplies the original CEN license.

An explicit, optional acquisition step downloads only those five pinned URLs:

```sh
python3 tools/validation-generator/scripts/fetch.py --out tools/validation-generator/.cache/archives
cargo +1.83.0 fetch --locked --manifest-path tools/validation-generator/Cargo.toml
```

To compile from explicit offline paths (paths may be anywhere readable):

```sh
cargo +1.83.0 run --locked --offline --manifest-path tools/validation-generator/Cargo.toml -- inspect \
  --archive cen.zip=tools/validation-generator/.cache/archives/cen.zip \
  --archive xr.zip=tools/validation-generator/.cache/archives/xr.zip \
  --archive config.zip=tools/validation-generator/.cache/archives/config.zip \
  --archive validator.jar=tools/validation-generator/.cache/archives/validator.jar \
  --archive cen-source.zip=tools/validation-generator/.cache/archives/cen-source.zip \
  --out tools/validation-generator/.cache/inspection
```

Output directories must be empty or absent. Exit 0 means inspection completed;
exit 2 means failure. `runnable` with the same inputs always exits 2 without
creating its output directory: lowering/runtime evidence is unavailable.

Generate twice, reverse archive argument order, compare every emitted byte, and
run independent Python stdlib XML accounting directly against the pinned ZIPs:

```sh
python3 tools/validation-generator/scripts/generate.py \
  --archives tools/validation-generator/.cache/archives \
  --out tools/validation-generator/.cache/reproduction \
  --check-accounting tools/validation-generator/SOURCE-ACCOUNTING.json
```

`ir.json` is canonical pretty JSON with a trailing LF. `manifest.json` separates
IR format, exact target, source-package digest, generator version/content digest,
and unavailable backend. `resources/` retains original selected bytes, including
schemas, Schematron, policy/report resources, classfile evidence and notices.
The manifest records all verified regular archive members, including unselected
members. It contains no local paths, timestamps or unordered maps.

The full output is about 8.4 MiB (IR about 5.5 MiB), predominantly repetitive AST
and source/provenance data. To keep review focused, the repository carries the
41 KiB authored `tests/fixtures/inspection.json` plus `SOURCE-ACCOUNTING.json`
with full-output hashes/counts. CI uploads the complete inspection directory.
Reproduce the authored artifact with:

```sh
cargo +1.83.0 run --locked --offline --manifest-path tools/validation-generator/Cargo.toml --example fixture
```

## Source coverage and integrity

The frontend parses all 806 CEN and 74 XR assertions, in 156 rules; all four phases,
14 helpers, 17 parameters, 19 helper variables, and the 20 included common globals
are retained. Independent accounting compares every one of the 1,108
expression-bearing attributes, ordered sites/patterns/rules, declared/effective
binding scopes, helper signatures/constructor trees, and exact message text/tails.
This establishes source preservation and static checking, not execution parity.
Independent Python lexical accounting also compares per-expression multiplicities
of numeric lexemes/kinds, decoded strings, node names/kind tests, calls, operators,
axes, filters, and variable declarations/references, including both policy trees.
It ignores nested comments/whitespace, decodes doubled string delimiters, normalizes
attribute/parent/implicit-child axes, and treats `Group` transparently. Type names
and occurrence suffixes are accounted separately. It does not prove association,
precedence, grouping, binding identity, or evaluation order; scope checks, authored
structure fixtures and the later execution-equivalence gate remain necessary.
`generate.py` runs the stdlib lexical and full-inventory corruption tests in CI;
they require neither Java nor a consumer compiler/backend.
Codelist strings keep padding, leading zeroes, substring-membership syntax and
sequence order; they are not rewritten as numeric values or sets.

Archive hashes are checked before ZIP parsing. Every regular member in a
member-locked archive must occur exactly once and have the expected bytes.
Duplicate, missing, extra, unsafe, symlink, size-mismatched and over-budget entries
fail. Inputs are never extracted. Limits: 128 MiB per archive, 64 MiB per member,
512 MiB uncompressed total per archive, 20,000 entries, 250,000 XML nodes;
DTDs are disabled. Only explicit in-memory resource allowlists resolve references.
Network/absolute references, archive escapes, missing imports, namespace mismatch,
unmapped locationless imports, schema includes/redefines and unknown Schematron
constructs fail. Includes accept the bounded pattern-document form used here.

The four original CII XSDs and six import edges are retained, without claiming
XSD compilation. The selected CII scenario, both complete report stylesheets,
configuration/report schemas, five JAR-internal closure resources and six engine
classfiles are retained. Compiled CEN/XR XSLTs are marked reference evidence and
never used as the expression frontend. The report-input XSD cycle is retained as
a finite graph. The output report's locationless scenarios import uses one exact
namespace/resource mapping, with no schema discovery.

`jar-closure.lock.json` adds individual resource/class/notice hashes under the
existing authoritative JAR hash. The oracle lock itself has **no JAR member map**;
we do not describe these new member hashes as existing oracle pins. Policy
clauses cite exact report lines, configuration nodes and pinned classfile methods.
Typed clauses cover ordered stages/schema short-circuit, flag/role conversion,
nine overrides, PROCESSING_ERROR immunity, missing SVRL, base stage validity,
effective-severity assessment, fallback/unmatched rejection and acceptance/error
handling. Whole report/HTML stylesheets remain **opaque retained resources**;
their retention does not implement arbitrary XSLT, HTML or policy execution.
Any other source version fails pins instead of inheriting this mapping.

Upstream licenses remain upstream: original CRLF CEN EUPL license from its locked
source ZIP; XRechnung Apache license from `xr.zip`; the separately locked KoSIT
configuration license; and validator `LICENSE.validator`/`META-INF/NOTICE` from
the JAR. XSD copyright headers and all selected resource bytes stay unchanged.
The repository MIT license does not relicense them.

## Bounded XPath and static checking

The owned tokenizer/parser produces recursive syntax trees, with complete input
consumption. It supports nested `for`, `some`, `every`, `if`, sequences, unary and
binary arithmetic/boolean/comparison/set operators, ranges, casts/castable,
absolute/relative/descendant paths, explicit axes, filters, predicates, attribute
and namespace/local wildcards, node tests and function steps. Match patterns have
a separate validation entry: top-level unions of matchable child/attribute paths
with expression predicates. Arbitrary function expressions are not match patterns.
The static function/arity and atomic type allowlists are explicit in `src/xpath.rs`.

Numeric literals carry `integer`, `decimal` or `double` lexical kind and an
arbitrary-length string; no `f64`, fixed decimal or numeric evaluation is used.
String literal values and original decoded expressions are both preserved.
Namespaces expand names while retaining their original qualified spelling.
Unprefixed XPath node/variable names use the empty namespace; unprefixed functions
use the XPath functions namespace. The xslt2 defaults include `xs`, even though
XR does not declare it. Source `sch:ns` declarations retain their order separately.

Schema and pattern lets are effective xslt2 document-root globals, retaining
source ownership. Rule lets, helper parameters/locals and expression binders have
lexical binding IDs; initializers see preceding bindings, not their own new ID.
Helper signatures are registered first, so forward calls and recursion work.
Global initialization cycles, including cycles through helpers, fail. Every
helper is parsed and checked, even when unused or in an inactive pattern.

Explicit parentheses remain `Group` nodes, preserving reverse-axis predicate
proximity order versus filtering a grouped result in sequence order. Malformed
path steps and cast ordering fail during source compilation; valid parenthesized
counterparts retain their boundary. The function table is XPath 2 (`round/1`,
not the XPath 3 `round/2` overload).

XSLT helper constructors support parameters, select/content variables, sequences,
and choose/when/otherwise. Significant literal text is exact; whitespace-only
stylesheet text nodes are stripped by the bounded xslt2 dialect. Untyped content
bindings retain their content-constructor form (temporary-document semantics),
separate from select bindings and typed sequence content. No coercion to a plain
string or handwritten helper implementation is performed.
Helper XML namespace bindings that conflict with the shared Schematron/xslt2
static context are explicitly unsupported and rejected, including inherited
and nested instruction rebindings. The bounded dialect does not silently
resolve those QNames against the wrong namespace. Missing `xs` declarations
still use the documented xslt2 built-in context; XML default namespaces do not
change XPath node/function defaults.

Unknown functions, types, namespace prefixes, variables, XML elements/attributes,
misplaced instructions and grammar fail. General XQuery, XPath 3 constructs,
XPath extension functions, arbitrary Schematron/XSLT and full sequence-type
grammar are not supported. NCNames use a bounded letters/digits/underscore/dot/
hyphen grammar. Expressions are bounded to 4,096 tokens, 64 recursive parser
frames, 128 operators per precedence chain and 64 AST levels. These are frontend safeguards,
not the BSC-10 adversarial runtime/resource gate.

`Location` identifies a raw UTF-8 XML attribute/element range with one-based
character line/column. A parse error's separate **decoded XPath UTF-8 offset**
refers to the decoded attribute value. Entity expansion and XML normalization
prevent treating that offset as an original XML byte offset.

## Runtime contract

Use the serde-only unpublished crate at `crates/validation-ir` (or deserialize
its documented JSON representation). Runtime needs no generator, ZIP, hash or
XML compiler dependencies. `einvoice-validation-ir/1` is independent of target,
source-package, generator and backend revisions. See that crate's README for
field semantics and the required exact-package execution gate.

This frontend supplies required construct/function/type/operator/numeric/helper/
policy/schema-feature entries, never a caller-supplied supported flag or callback.
There is no executable program, package installer, evidence constructor or fake
backend. BSC-8 owns DOM/navigation (including original invoice QNames), exact
numbers, XPath/helper evaluation, schema execution and report wiring. Later
BSC-7 runnable emission must bind actual backend revision, IR format **and
content digest**, lowering evidence and a tested construct matrix for the exact
package. Source retention or passing parser tests cannot satisfy that gate.

Exact arithmetic/rounding/error semantics, all XSD structures, differential
KoSIT/Saxon diagnostics and acceptance, persistent precompilation/AOT, memory
bounds and the 5x release gate remain unproven. Compile-once-at-worker-init would
be initialization, not AOT. The public facade remains unchanged:
`BackendUnavailable`, `generator=None`, `validator=None`, `is_valid=false`.
The oracle evaluator schema, labels, workload and dataset splits are untouched.

## Checks

```sh
cargo +1.83.0 test --manifest-path tools/validation-generator/Cargo.toml --workspace --all-features --locked --offline
cargo +1.83.0 fmt --manifest-path tools/validation-generator/Cargo.toml --all -- --check
cargo +1.83.0 clippy --manifest-path tools/validation-generator/Cargo.toml --workspace --all-targets --all-features --locked --offline -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo +1.83.0 doc --manifest-path tools/validation-generator/Cargo.toml --workspace --no-deps --document-private-items --all-features --locked --offline --examples
```

## Bounded original-source candidate

`resolve-candidate` accepts the same five explicit archive arguments and empty
`--out` directory as `inspect`. It emits a **non-executable** materialized
`einvoice-resolved-candidate/1` envelope. The compiler owns the fixed
`source-resolution.lock.json`; there is no caller manifest, selector, replacement
expression, capability flag or arbitrary raw-package constructor.

The single CII-SR-282 decision reads the existing original concrete
`sch:param/@value` at the guarded buyer site. A separate bounded reader verifies
the original entry includes, concrete `is-a`/abstract pattern linkage, unique
assertion/context linkage, exact complete ordered duplicate pair, source and
attribute ranges/hashes, full ancestry, namespace map, message, flags, severities
and unchanged policy. It parses both occurrences and the source context with the
existing XPath parser. No general abstract instantiation or first-wins duplicate
policy is added. Unknown decisions/fields, duplicate JSON keys, unused guards,
source drift and unexplained materialization differences fail compilation.

Output layout:

- `raw/`: unchanged raw IR and 31 original resources, with current generator metadata.
- `resolved-candidate.json`: effective Package, original Site/Expression/context,
  effective Expression, both parameter occurrences and complete source lineage.
- `source-resolution.lock.json`: exact fixed decision manifest bytes.
- `candidate-resources/`: original entry/abstract/concrete syntax documents and
  original release XSLT comparison bytes. XSLT is reference evidence only.
- `resolved-manifest.json`: exact candidate/effective-package/raw/decision digests,
  all emitted file hashes, generator source fingerprint and pending audit/backend.

The candidate adds only its selected predicate and explicit source resources/edges
(and any mechanically discovered requirements). A private compiler check compares
all candidate fields with source-derived expectations; any second predicate,
context, helper, message, policy or AST/source/provenance change fails. Serde
roundtrips are inspection data, **not** verified compiler values or runtime
admission. Runtime consumes the materialized Package and never applies an overlay
or chooses source authority.

Raw IR retains SHA-256
`8882e1e0550e0f45db2aa6e55ec28de0cc120b51983bba79f2bb175c5b02bed6`.
The raw manifest changes when compiler code/contract/fixed-manifest fingerprint
changes. `SOURCE-ACCOUNTING.json` protects raw bytes; separate
`CANDIDATE-ACCOUNTING.json` protects candidate outputs. Neither file certifies
semantics. Reproduce and check both expectations with:

```sh
python3 tools/validation-generator/scripts/generate.py \
  --archives tools/validation-generator/.cache/archives \
  --out tools/validation-generator/.cache/resolution-reproduction \
  --check-accounting tools/validation-generator/SOURCE-ACCOUNTING.json \
  --check-candidate-accounting tools/validation-generator/CANDIDATE-ACCOUNTING.json \
  --check-correspondence-accounting tools/validation-generator/CORRESPONDENCE-ACCOUNTING.json
```

This generates raw and candidate outputs twice with reversed archive arguments,
compares complete output trees, runs the real-source compiler/serde/provenance
roundtrip, independently checks source ranges/hashes/materialized deltas and runs
the existing source accounting and 24 corruption tests. Four additional candidate
test groups cover a positive control and eight coordinated mutations (including
selected AST axes/functions/namespaces and added resource identities/roles) after
recomputing outer hashes. The checker compares the selected AST with an independently
authored tree and the fixed AST digest, and resource metadata with source pins.
These tests strengthen compiler evidence; they are not runtime admission. The real-source Rust test
is explicitly ignored in cache-free unit runs and obligatorily enabled by this
script after acquisition; `VALIDATION_GENERATOR_ARCHIVES` supplies its explicit
archive directory. Authored source-only fixtures exercise deeper guards after
fixture archive/member pins are refreshed. No invoice or BSC-10 control is used.

Full predicate/context/helper/message/policy correspondence remains pending.
Backend and policy execution remain `Unavailable`; `runnable` still errors before
creating output. The local decision does not certify undiscovered differences,
BSC-10 evidence, native parity, complete source resolution, protected attestation,
AOT or performance. Independent review of this compiler pass is still required.

## Independent correspondence accounting

`tests/correspondence.py` reads the five pinned archives and `candidate-one`
directly, using Python stdlib XML parsing independently of the Rust generator.
It authenticates archive/member bytes against the oracle lock and additive JAR
closure lock, and every candidate file against `CANDIDATE-ACCOUNTING.json`.
`generate.py` runs this check and its corruption tests, compares summaries from
both generation orders byte for byte, and checks the reviewed
`CORRESPONDENCE-ACCOUNTING.json` with `--check-correspondence-accounting`.
There is no dependency on discovery snapshots or source-inventory output.

The complete denominator is 12,571 source occurrence/facet obligations:
11,225 mapped-exact, 24 different, 3 ambiguous, 27 unsupported and 1,292
unexamined. Ordered ownership, source ranges/hashes, exact expression strings,
phase activation, binding declarations, helper constructors, message parts and
nine severity overrides are compared. Original, preprocessed and configured
views remain separate obligations. Matching IDs only associate comparison
candidates; they do not establish equivalence. Exact strings retain padding and
numeric spelling. Named constructor projections record raw trees, including the
`sch:name` to `name()`/`name(.)` projection and removal of whitespace-only XSLT
stylesheet text. Significant text is preserved.
The complete four-stage scenario sequence differs from the candidate's nine-stage
sequence, which also includes engine steps; resource presence is checked separately.
Configured helpers and content initializers are compared to candidate bodies.
Unknown primary elements/attributes produce unsupported rows. Excluded configured
scaffolding bindings are listed by name and source location in the summary, outside
the obligation denominator, so additions change the reviewed accounting.

The summary binds the exact candidate, both source locks, all source members and
the complete ordered obligation list. Mapped-exact rows are bound by a canonical
list digest and category counts; every other row retains its full source and
candidate evidence, JSON pointers and occurrence identity. Each non-exact row is
one unshortened JSON line inside the summary, preserving whitespace in strings.
Missing, extra or duplicate obligations, changed exact evidence and mixed
candidate/summary pairs cannot pass the reviewed baseline comparison.

The overall result is **incomplete**. The CII-SR-282 alternatives remain
ambiguous/different; this check makes no new authority decision. Lexical or
constructor preservation does not prove full XPath AST structure, configured
rule dispatch/reachability, helper or external-parameter effects, or report/JAR
policy effects. XSD semantics, HTML presentation, unrelated scenarios and
arbitrary compiler scaffolding are outside this denominator. It never executes
XSLT or invoices and proves no runtime parity, conformance, AOT or performance.
`correspondence_audit` stays
`PendingFullPredicateContextHelperMessagePolicyCorrespondence`, the backend and
policy execution stay `Unavailable`, and `runnable` stays unavailable.

Root regression checks (the generator remains isolated from that workspace):

```sh
cargo +1.83.0 test --workspace --all-features --locked --offline
cargo +1.83.0 fmt --all -- --check
cargo +1.83.0 clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo +1.83.0 doc --workspace --no-deps --document-private-items --all-features --locked --offline --examples
```
