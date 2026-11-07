# Source-to-schema execution slice

`einvoice-schema` is an unpublished Rust 1.83 internal library. It validates an
existing immutable `einvoice-source::SourceDocument` directly against the exact
four original pinned CII D16B XSDs. It does not parse the invoice a second time.
It does not implement full native XRechnung, rule execution, package admission,
acceptance, frozen conformance output or a release/performance gate.

Supply the four immutable original resources to `CiiBundle::from_resources`.
Member names, byte lengths and SHA-256 values are fixed in `CII_RESOURCES` and
reconciled with the merged generator's `SOURCE-ACCOUNTING.json` and authenticated
config archive. Original bytes are never modified. Runtime has no ZIP/compiler
or generator dependency. An empty catalog and exact in-memory loader admit only
this closure; filesystem, network, embedded XML/XLink fallback and instance hint
enrichment are absent. Namespace/import preflight, parse/directive errors,
deferred component resolution and content-model preparation all gate availability.
BSC7 retains full package/source/candidate authority; this constructor cannot
admit an executable rule package or override its source decisions.

`with_epoch(bundle, NonZeroUsize, |epoch| ...)` keeps a prepared schema and its
borrowing validator as local variables inside the owning thread. Each request
gets a fresh runtime. Call `epoch.validate(source, limits, control)` with explicit
finite limits; there are no calibrated defaults. Healthy valid/invalid requests
can reuse the finite epoch. Every incomplete request retires it, including a
panic caught by its caller. Exhausted epochs refuse subsequent requests. Drop
and rebuild to retire interned state; no global lock, leaks or unsafe lifetimes.
A full bounded worker pool, delivery queue, hard RSS enforcement and process
supervision remain open integration work. Caller-held source/results are outside
this epoch's retention policy.

Admission inspects actual source node/name/value/attribute/declaration/depth and
effective namespace inventory before instance interning. Source does not export
a parse-limit certificate. Checked cumulative work includes source byte scanning,
namespace ancestor comparisons, names/values, runtime calls, diagnostic payload
and source joins. A conservative namespace comparison reservation covers both
inventory and execution. Temporary logical record bounds cover namespace/path
snapshots and the O(depth) iterative walker. Diagnostic count includes warnings;
bytes enumerate actual raw/path/evidence fields before copying. These are logical
payload/record allowances, not allocator capacities, exact heap use, backend
instruction counting or hard deadline guarantees. Backend regex/NFA/type work
cannot be interrupted mid-call. Allocation abort remains possible in backend and
standard collection/string internals. Cancellation/deadline checks are cooperative.

`Observation` preserves a single ordered owned callback stream with duplicates,
raw code/message/nullable location, structured path, error local-name element
path, native causes (including every nested value/facet payload/type code) and
separate governing/actual/component/facet origins. Warning API exports no rich
evidence or local-name path. Every `None` means only unavailable from the observed
API unless its field documentation states a known API limitation; no branch
reason, primitive type, origin, triggering occurrence or causal relationship is
inferred. Native codes are not mapped to another engine's codes/semantics.

The driver always pushes the full document element, checks that root before
joining each backend occurrence path, and retains owner versus exact attribute
token/value witnesses separately. UTF-8 spans address original bytes including
BOM; all other encodings use original Unicode scalars excluding BOM. Raw backend
locations stay untouched. Deferred references resolve their saved original
subject; rejected-child raw diagnostics still join the parent. Schema component
URIs/spans are schema evidence, often start tags, never invoice locations.
Opaque source identities are not serialized. The adapter explicitly maps actual
source handles to zero-based ordinary-node preorder ordinals by comparing their
opaque identities while scanning the same arena under the work allowance.
Attributes use (owner preorder, zero-based source attribute ordinal). These
`SourceReference` witnesses belong only to this observation's sole original-byte
binding; no cross-parse identity is claimed. The actual validated root, joined
owner and subject retain these references alongside their distinct ranges and
structured paths. No extra arena or global identity table is allocated.

Traversal, finalization, operational failure and adapter first failure are
independent fields. Finalization runs once only after a healthy complete walk.
An unsuccessful or truncated run stays Incomplete even with prior errors.
Deferred finalization errors can make the stage Invalid while the earlier root
node was Valid. Operational and finalization errors have a separate explicit
terminal capture byte allowance; capture refusal is explicit. No error-only or
empty-array inference produces Valid. The development JSON projection is not a
frozen wire contract, execution attestation or native-v1 report.

The maintained backend's fourth patch classifies character content using XML S
(space, tab, CR, LF), rather than Unicode whitespace. Element-only content rejects
other characters through actual native diagnostics; mixed/simple content retains
legal Unicode text. Empty and nilled content reject all character children,
including XML S. Both backend drivers and the source adapter have authored
regressions. This correction is not a full XML/XSD parity claim.

`examples/cii-observe.rs` uses the same public driver with supplied original XSDs
and input bytes. `tools/observe.py` is an optional development harness: it verifies
the supplied config ZIP, reconciles member hashes/lengths with the generator,
extracts only the four original members into the selected output directory,
builds on Rust 1.83 and writes complete/partial JSON, raw build output and an
actual executable/source/dirty-tree receipt. It requires cached dependencies
(`--offline --locked`) and an isolated `CARGO_TARGET_DIR`. The metadata graph is
filtered to the actual `rustc -Vv` host; target-only lock entries are not asserted
compatible. No held-out labels or frozen conformance artifacts are read.

Input paths must name regular files; symlinks and streams are refused. Reads are
bounded before materialization (original schemas by their exact sizes, XML by
the source constructor's 16 MiB limit, the ZIP by its pinned size). Unix opens
also use nonblocking/no-follow flags. Filesystem I/O has no hard deadline here.
The admitted XML snapshot is retained as `input.xml`; both real child runs use
it, and both actual Observation lengths/digests must match its receipt binding.
The ZIP is parsed from the authenticated buffer rather than reopened.

Every child stage saves raw stdout/stderr prefixes and a status record before
propagating failure. Each stream has a 32 MiB retained-prefix cap and each direct
child a 300-second timeout; truncation, timeout and incomplete pipe drainage are
failures, not successful observations. These limits are development harness
controls, not a worker process-tree/RSS guarantee. Pre-observation failures get
`failure.json` and no fabricated Observation. Use an empty output directory for
each attempt so earlier failure evidence cannot be overwritten. Source/binary
and input bindings support reproduction, not trusted execution custody.

```text
python3 crates/einvoice-schema/tools/observe.py \
  --archive /supplied/config.zip --input /supplied/development.xml \
  --output /scratch/schema-observation
```

Run the bounded producer helper tests with
`python3 crates/einvoice-schema/tools/test_observe.py` (Unix-only FIFO/symlink
checks are explicitly skipped on other platforms).

The real producer regression runs unchanged, caller-edited, snapshot-edited and
malformed authored inputs, preserving actual outputs and failures:

```text
python3 crates/einvoice-schema/tools/test_reproduction.py \
  --archive /supplied/config.zip --output /scratch/schema-producer-tests
```

The original-resource integration test is excluded from ordinary offline tests
because it requires the supplied authenticated resources. CI invokes it explicitly
after `observe.py` extracts the exact closure. The committed
`tests/fixtures/minimal-schema.xml` is independently authored for this structural
check and deliberately lacks business requirements; it is not an accepted invoice.
