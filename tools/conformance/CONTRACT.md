# BSC-8 serializer contract: bsc10-native-v1

This is a bounded offline wire boundary, not a validator API or an execution
attestation. BSC-8 owns the adapter. BSC-10 supplies the request and independently
checks its result. No native backend currently exists. This slice **always** emits
`release_gate_eligible: false`, `evidence_complete: false`, and
`native_execution_attested: false`. A self-reported `kind: native` proves nothing
about execution. Harness tests use `kind: harness_fixture`.

## Request and digest binding

Run `evaluate.py request --manifest initial|sinabis --selection development|full`.
The default is `initial`, `development`. This first verifies frozen evidence.
The exact request keys are:

| Key | Meaning |
| --- | --- |
| `format` | Integer `1` |
| `kind` | `bsc10_request` |
| `release_gate_eligible` | Always `false` |
| `manifest` | `tools/oracle/corpus/initial.jsonl` or `tools/oracle/corpus/sinabis.jsonl` |
| `manifest_sha256` | SHA-256 of the entire frozen manifest bytes |
| `selection` | `development` (complete rows whose split is development) or `full` (all complete rows) |
| `requested_target` | `xrechnung-3.0.2-cii-d16b-kosit-2026-08-31` |
| `wire_target` | `xrechnung-3.0.2-cii-2026-08-31` |
| `target_projection` | `bsc10-target-v1`, only the above explicit spelling projection |
| `artifact_lock_sha256` | SHA-256 of frozen `tools/oracle/artifacts.lock.json` |
| `evidence_lock_sha256` | SHA-256 of the exact selected evidence lock bytes |
| `evaluator_sha256` | Evaluator policy/code/schema/documentation/test digest described below |
| `records` | Complete frozen manifest row objects, in original order, without deduplication by bytes |

`records` retain `id`, `path`, `sha256`, `bytes`, `source`, `scope`, `scenario`,
`family`, `split`, `group`; source-specific keys are `revision`, `source_path`,
`parent`, `parent_sha256`. The adapter must preserve identities. `path` is relative
to `tools/oracle` and begins `.cache/`; an explicitly selected input cache may
supply that suffix. It must hash the original input bytes, not a reserialization.
Split is evaluator-supplied, never adapter-inferred. No fragments are in this
request. Empty requests and convenient subsets are unsupported.

The request digest is SHA-256 of canonical JSON of the **whole request object**:
UTF-8, recursively sorted object keys, no insignificant spaces, literal Unicode,
no nonfinite numbers (`json.dumps(sort_keys=True, separators=(',', ':'),
ensure_ascii=False, allow_nan=False).encode('utf-8')`). Array order is significant.
No newline participates. The request contains no own digest. Output/lock/source
file hashes instead hash their exact bytes including whitespace and final LF.

## Record stream

Write one complete record per UTF-8 JSONL line with final LF, no blank lines,
metadata records or trailing partial line. Exact record count and request order
are mandatory. Duplicate keys/IDs, unknown fields, NaN/infinity, invalid Unicode,
missing/extra/reordered documents are errors. See `native-record.schema.json` for
all required keys and types; no keys are optional:

`format`, `id`, `sha256`, `scope`, `split`, `engine`, `target`, `processing`,
`processing_errors`, `schema`, `business`, `stages`, `target_matched`, `scenario`,
`findings`, `structural_findings`, `reference_acceptance`, `acceptance`.

`engine` is `native` (including synthetic wire fixtures, which MUST use sidecar
`kind: harness_fixture`). `scope` is `complete`. `target` is the legacy wire target;
this projection only applies to identical pinned source/policy, never to generic
profile matching. `scenario` retains the actual matched reference-contract scope:
`EN16931 XRechnung (CII)`, broader `EN16931 (CII)`, or null as observed. Exact-target
preflight rejection cannot erase broader reference stage/diagnostic evidence.

`processing` is `completed` or `failed`, with nonempty `processing_errors` iff
failed. `schema` is `valid`, `invalid`, or `not_evaluated`. `business` is `evaluated`
iff both business steps are present. `stages` has only actual observed
`val-xml`, `val-xsd`, `val-sch.1`, `val-sch.2` keys, each `valid` or `invalid`.
Missing keys mean not evaluated, not valid. XSD requires valid XML; business requires
valid XSD; XR requires CEN. Invalid XSD skips business. CEN diagnostic invalidity
does not short-circuit XR for the matched exact target.
Completed processing for either the exact target or the broader `EN16931 (CII)`
scenario requires an observed XSD verdict; valid XSD additionally requires CEN.
The exact target also requires XR. A nonempty skip explanation cannot substitute
for those required observations. A genuine unmatched scenario does not require
target-specific XSD or business execution.

`reference_acceptance` is the pinned policy recommendation `acceptable`, `reject`,
or `undefined`; `acceptance` is `accepted` only with completed processing, exact
target match, valid XML/XSD, both business steps and acceptable recommendation.
Otherwise completed processing is `rejected`, failed processing `indeterminate`.
An accepted record cannot have effective errors. Failed processing, unsupported
features/targets/backend, timeout, crash and limits never pass parity even when the
reference also failed. XSD invalidity is a completed document fault, not an engine
failure. A run may preserve partial observations before failure.

Each diagnostic in either array requires exactly `id`, `severity`,
`effective_severity`, `location`, `line`, `column`, `message`, `stage`.
Nullable fields remain null; raw positions are nullable **strings**, not zero or
inferred root positions. `message` is untrimmed. `severity` is the full oracle's
**base report level**, `error`, `warning`, `information`, or null. Raw fatal maps
to base error; flag/role live separately below and are not independently verified.
Effective severity reflects the pinned policy. Stage diagnostic verdicts use base
warning/error, so base error/effective warning may coexist with accepted and
`val-sch.1: invalid`. Every identical business duplicate and its order matters.
Business ID, base/effective level, full canonical path, raw line/column, message,
and stage compare exactly. No sorting, trimming or set conversion is allowed.

Locations support only the pinned indexed EQName grammar: `/` document root;
`/Q{URI}local[n]` element steps; terminal `/@Q{URI}local`, `/text()[n]`,
`/comment()[n]`, `/processing-instruction("target")[n]`. Namespace URIs may contain
slashes. Indices are positive decimal without leading zero, at most 2^64-1,
counted among siblings with the same expanded name (PI index among same target),
not all children. NCNames follow XML 1.0. Attributes/text require an element owner.
Only elements permit another step. Wildcards, prefixes, omitted/zero indices,
predicates and unknown syntax are rejected. Canonical paths never authorize
rewriting original lexical QNames or messages; lexical prefixes affect rules.

## Adjacent run sidecar

Write `<records.jsonl>.metadata.json` after the final data bytes are known; stale
or missing sidecars are invalid. The consumer's default is that exact adjacent
path. `run.schema.json` specifies strict keys and types. All keys below are required:

| Key | Meaning |
| --- | --- |
| `format`, `contract` | `1`, `bsc10-native-v1` |
| `release_gate_eligible` | Required `false`; a producer cannot claim release proof |
| `kind` | `native` or `harness_fixture`; self-description, never attestation |
| `run_id` | Nonempty producer run identifier; uniqueness is producer responsibility, not replay protection |
| `output_sha256` | Exact JSONL byte digest, not canonical JSON |
| `request_sha256` | Whole request canonical digest above |
| `evaluator_sha256`, `evidence_lock_sha256`, `artifact_lock_sha256` | Exact request bindings |
| `requested_target`, `wire_target`, `target_projection` | Exact request bindings above |
| `implementation` | Actual loaded identities below, not just expected bundle pins |
| `completion` | `{status: completed|failed, exit_code: integer, record_count: integer}` |
| `documents` | Per-request document details in the same exact order |
| `coverage`, `resources` | Each `{status: unavailable, reason: nonempty string}` in v1 |

A complete successful run has `completion.status: completed`, `exit_code: 0` and
no failed records. Any failed record requires failed completion and nonzero exit.
Missing/truncated counts, contradictory status or skipped evidence cannot be
rescued by old successful files. No timestamps, nonce echo or digest can establish
fresh execution without an evaluator-controlled runner; replayed identical evidence
remains possible and is one reason this slice cannot certify a native release.

`implementation` requires `facade`, `serializer`, `backend`, `generator`,
`package`, `source`, `build`, `unavailable_reasons`. `facade`, `serializer`,
`generator` identities are `{name, version, sha256}`; backend/package additionally
require `capabilities_sha256` for the actual loaded capability inventory bytes.
The identity SHA denotes the actual implementation/package content (producer must
document what was hashed), never a fabricated expected identity. `source` is
`{revision, tree_sha256, artifact_lock_sha256}` for the actual loaded source closure;
its artifact lock must match the request. `build` is `{revision, dirty, profile,
rustc, target, executable_sha256}` for the running executable/build. Strings are
nonempty, hashes lowercase SHA-256. `backend`, `generator`, `package`, `source` may
be null only with a nonempty corresponding key in `unavailable_reasons`; those
keys otherwise must be null. Completed native validation requires all four loaded
identities. These are checked for format/binding, not independently proven content
or capability execution. Package membership, build integrity and capability
attestation require later controlled execution, not invented source instrumentation.
Backend/package hashes may identify a documented deterministic composite of the
actual runtime, XSD backend and executable-rule closure. The generator identity
is the build-time provenance of the loaded package; it does not assert that a
compiler executes during invoice validation.

Each document is `{id, failure, stages, diagnostic_details, backend_notices}`.
`failure` is null or `{kind, code, message}`, kind one of `document`, `engine`,
`timeout`, `crash`, `unsupported`, `limit`. Operational failures require failed
processing. `document` can accompany completed invalidity. Raw processing text
is retained in inputs; the comparator does not claim cross-engine text equivalence.
An accepted record must have null `failure`. A document fault may also accompany
failed processing (for example malformed XML); it is not necessarily a completed
rejection and must retain the actual processing state.

`stages` has exactly six entries in this order: `xml_syntax`, `target_selection`,
`xsd`, `cen_rules`, `xrechnung_rules`, `acceptance`. Each is
`{stage, status, verdict, reason}`. `status` is `completed`, `failed` or `skipped`.
Completed has null reason; others have a nonempty explanation. XML/XSD/CEN/XR
completed observations have verdict `valid|invalid`; skipped has null verdict.
Failed may preserve an observed verdict. Selection/acceptance always have null
verdict (their result is in the record fields). Presence and verdict project to
`stages` in the record. Completed processing requires XML, target decision and
acceptance completion, and every projected validation step completed. Completed
business requires all six stages completed. Legitimate XSD short-circuit has
completed-invalid XSD and explicitly skipped CEN/XR, not failed processing.
Any `failed` stage, including one with a null verdict, requires failed record
processing, explicit failure details and failed run completion. Document-fault
failure kinds remain document faults; this rule does not recategorize them as
engine errors.

`diagnostic_details` has one entry per record diagnostic, in `findings` order then
`structural_findings` order, with zero-based `ordinal` within the named `array`.
No omitted/duplicated detail and no extra diagnostic is allowed. Exact keys:

`array`, `ordinal`, `raw_code`, `raw_message`, `raw_flag`, `raw_role`, `constraint`,
`type`, `component`, `cause`, `typed_unavailable_reasons`, `relationship`, `parent`, `source`.

`raw_code` and `raw_message` equal the corresponding diagnostic's ID/message.
Flag/role are nullable strings. Constraint/type/component/cause are each null or
`{kind, namespace, name}`; typed names are native backend evidence, not assertions
of equivalence to Xerces codes. `relationship` is `primary`, `collateral` or
`unclassified`. `parent` is null except collateral requires `{array, ordinal}`
pointing to an earlier detail of this document, preventing cycles. Do not invent
causes or clone diagnostics to resemble another engine's counts.

`typed_unavailable_reasons` requires keys `constraint`, `type`, `component`, `cause`.
Each is a nonempty reason when its typed field is null, otherwise null. Unknown
native typed evidence must remain explicitly unavailable for later mapping.
These identifiers do not encode all constraint parameters, facet bounds or
content-model/cardinality expectations. They cannot establish complete structural
equivalence; v1 implements no such mapping.

`source` is `{path, span, unavailable_reason}`. Path is the exact diagnostic
location or null, using the grammar above. Span is null or
`{unit, start, end, subject, line, column}`. Offsets are zero-based, half-open;
`unit` is `utf8_byte` only for immutable original UTF-8 input, with offsets directly
into those original bytes. For original UTF-16 or other encodings, use
`unicode_scalar` into the original decoded text or a null span with an explicit
unavailable reason. Offsets into UTF-8 transcoding, reserialization or normalized
XML are never original-source byte spans. Byte end cannot exceed manifest byte
length. Line/column
are nullable one-based integers. `subject` is `exact_node` or `owner_element`;
owner_element applies only to an attribute path and MUST NOT claim an exact
attribute span. For unknown path or span give nonempty `unavailable_reason`,
otherwise null. Null typed fields mean no supplied typed observation; unavailable
source evidence never becomes a zero position or guessed location. A Unicode span
cannot be byte-bounds checked without the decoded original; v1 checks its order
but does not independently verify original encoding, resolve nodes or attest span
truth. For example, authored `<r>é</r>` encoded with a UTF-16LE BOM is 18 original
bytes and decodes to 8 scalars. Its root may use scalar `[0,8)` into that decoded
text, or an unavailable span. The 9-byte UTF-8 reserialization is not a valid
`utf8_byte [0,9)` span for that UTF-16 input. This producer obligation is not an
encoding check performed by the offline consumer.

`backend_notices` is an ordered array of `{code: string|null, message: string}`.
Capacity/instrumentation warnings belong here, never in source XSD diagnostics.
They are retained in the bound sidecar and do not create invoice faults.

## Structural and scoped limits

All structural observations remain in original arrays, with all code/message,
severity, raw nullable positions, order, duplicates and source occurrences.
Development reports reproduce both complete structural arrays. Full reports
expose only aggregate counts of observations, raw differences and multiplicity
differences. Raw code/prose/count inequality is **not** a demonstrated semantic
information difference. Any structural findings yield unresolved semantic parity,
including equal counts or identical strings. There is no cause-chain equivalence
mapping in v1. Observed raw shared facts remain available in the bound records and
sidecar for later independent mapping; no suppression or allowlist is implemented.

Frozen scoped records use `scoped.schema.json` and a separate nine-fragment
inventory. Full-document counts exclude both containers/fragments. Without cache,
container membership hashes, shape and contiguous ordinals are checked; with cache,
actual fragment counts and rule scopes are checked against the original containers.
The scoped bridge reserializes fragments, runs CEN without XSD, uses raw flag for
severity and trims messages. `expectations_met` tests only declared assertions,
never global validity or full collateral parity. Native scoped execution is
unavailable; v1 supplies no scoped native adapter or global acceptance projection.

## Evaluator trust and outputs

`evidence.lock.json` is a deliberate reviewed snapshot of every tracked
`tools/oracle` file and its original CI workflow at reviewed revision
`87e0a0a0c7095412faf23efad7aa8f0710328d03`. A separately enumerated code inventory
rejects removed/extra/duplicate paths. All frozen byte sizes/hashes are checked
before consuming evidence. Frozen records, manifests, schema and request digests
are derived from those same verified byte snapshots, including the selected lock.
Native JSONL is also read once for both parsing and output digest validation;
archives are opened from the bytes whose artifact digest was checked. Concurrent
file replacement cannot substitute a later read's bytes into these bindings.
Snapshots establish which bytes were consumed, not proof of fresh execution.
The run path has no lock refresh, network, oracle
imports, relabelling or executable backend. A deliberate lock/code change needs
review against that revision; local hashes cannot resist an attacker rewriting
both verifier and lock. Reviewed Git history and review/CI policy are the trust
anchor, not a self-updated hash or cryptographic attestation.

Evaluator digest: canonical JSON of `{domain: "bsc10-evaluator-v1", files: [...]}`,
where `files` contains `{path, sha256}` sorted by relative path. The fixed file
list is `evaluate.py`, `evidence.lock.json`, `native-record.schema.json`,
`run.schema.json`, `scoped.schema.json`, `CONTRACT.md`, `README.md`,
`requirements.txt`, `tests/test_evaluate.py`. It includes policy, code, schemas and
tests. Reports/requests/sidecars are excluded, avoiding self-hash cycles. Installed
Python/jsonschema binaries and runtime state are not attested by this digest.

`verify` exit 0 means the frozen contract passed, not native parity. Missing cache
is explicit `cache.status: unavailable`, `integrity_complete: false` (not an input
error); an absent cache directory is also an explicit gap; supplied partial/corrupt cache evidence is exit 2. `request` exit 0 only
exports that verified selection. `compare` exits 0 for verified protocol and
observable comparable agreement, 1 for observed comparable mismatch, 2 for invalid,
incomplete, processing-failed or structurally unresolved evidence. A mismatch
coexisting with an unresolved structural gap returns 2 with mismatch counts retained.
Exit 0 cannot mean complete corpus/native/release proof. `observable_parity` is
false for comparable disagreement, null if unresolved processing/structural evidence
prevents a decision, true only for comparable observations without those gaps.

File outputs first receive an atomic `protocol: incomplete` marker before evidence
is read, then an atomic final result. Errors replace it with `protocol: invalid`.
An interrupted run cannot reuse prior success after that marker; always consume
the process exit status as well as the report. Unsafe output destinations are
rejected before any write, including every frozen/evaluator file, both oracle and
conformance workflows, explicit inputs/cache directories and their aliases.

Reports always retain the gaps in controlled execution, coverage, resources,
scoped execution, structural mapping and raw flag/role reference evidence. Future
coverage/resource formats require a reviewed contract version; v1 deliberately
accepts only honest unavailable markers. Full selections produce aggregate reports;
development selects only development records. Errors expose constant codes, never
raw exception values, malformed lines, held-out IDs or diagnostics. Input evidence
files themselves are sensitive and are not copied into full reports.
