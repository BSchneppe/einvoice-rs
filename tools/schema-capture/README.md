# Exact-producer schema capture verifier

This Python 3.12 stdlib tool reads a retained development attempt offline. It
checks the strict serialization projection of reviewed producer
`2d82953fda88ad313144d6909e3c5010834d20af`, supplied byte relations, source inventory
and necessary event/terminal/source consistency. It does not execute a validator.
`release_gate_eligible` is always `false`. Producer-reported `Valid` or `Invalid`
is a schema-only assessment of an authored structural probe, not invoice acceptance.

Run with the standard library isolated from the working directory/PYTHONPATH:

```sh
python3.12 -I tools/schema-capture/verify.py \
  --attempt tools/schema-capture/development-evidence/linux/schema-valid
python3.12 -I tools/schema-capture/verify.py \
  --attempt tools/schema-capture/development-evidence/windows/schema-producer-tests/unchanged \
  --observation partial
python3.12 -I -B -m unittest discover -s tools/schema-capture/tests -v
```

The CLI needs no Git history. The test that checks the Rust variant inventory
requires Git and the merged producer commit above with its `evidence.rs` blob.
CI checks all 407 historical blobs and requests full history with checkout's
`fetch-depth: 0`. A source export or shallow clone missing those objects fails
explicitly; supply the historical objects before running that source check.
Neither tests nor the CLI fetch objects themselves. The non-ancestor hosted
receipt commit is not required for routine checks; its tree correspondence is
retained separately in provenance.

`--observation complete|partial` defaults to `complete`. Both paired outputs,
statuses and any failure are checked before selecting an observation. Selecting
snapshot-change's matching partial still rejects its contradictory complete
binding. Absolute historical paths, executable commands and schema URIs are data;
none is opened, executed or fetched. No output-file option exists. Consume both
the process exit code and its bounded JSON report.

| Exit | Meaning |
| --- | --- |
| 0 | Supported, supplied-byte-bound, internally consistent **selected complete capture**; producer assessment remains distinctly `Valid` or `Invalid`. |
| 1 | Malformed/contradictory projection, failed supplied binding or unsafe file path. |
| 2 | Consistent Incomplete, retained no-Observation/child failure, required evidence or producer profile unavailable, or consumer I/O/resource refusal. |

A failed child's stdout can be an arbitrary retained prefix. Its status/raw
bindings are checked, but the tool does not parse that prefix as a completed
Observation or invent a validation verdict. Missing required evidence is
unavailable. Partially present status/stream groups and a successful receipt
coexisting with failure are contradictory. A receipt alone is insufficient.

## Reviewed compatibility

[`producer-2d82953.json`](producer-2d82953.json) is consumer configuration, pinned
by a SHA-256 constant in `verify.py`. It records the actual 407-member source map,
64-bit unsigned Rust integers, exact schemas/example strings/fixed allowances,
and reviewed full-tree correspondence between historical receipt revision
`890d1582ecb831e83c54b4e6cb817d1c3a7703d6` and the merged revision. Both Git trees
are `3e09dcfbde65cb699c5bf3d422618e059d81b8ba`. A matching draft format string alone
never establishes compatibility. Unknown receipt revisions or source maps require
review. Changing a descriptor also requires deliberate code+descriptor review.
The descriptor does not identify an executing binary or authenticate a receipt.

The exact grammar preserves 14 NativeCause, 4 ValueCause, 17 FacetCause and 62
TypeCode variants, nested payloads, explicit nulls and all struct fields. It rejects
unknown/missing/duplicate keys, unknown tags, omitted nullable fields, invalid
UTF-8/scalars, numeric strings/bools/floats/exponents for integer fields and integer
overflow. `TypeCode::None` is the string `"None"`, not JSON null. Optional admission
hash/root reference are permitted in a legitimate Incomplete; a missing hash makes
that Observation's input binding unavailable. Zero/oversized configured count/work
can legitimately record `InvalidLimits`. Successful finalization can still be
Incomplete. No missing-cause reason or diagnostic is inferred.

Checks apply to the actual example and driver, including its remaining request
count, epoch retirement, first-failure/terminal relationships and ordered combined
events. Incomplete captures can have at most one entered but unretained callback.
Failure tags are constrained by their observable producer preconditions, and
terminal capture failures by the capture function's return domain. Event phase
timing is absent: the tool cannot prove an error happened
during finalization. Root Invalid does not require a retained Error. Root Valid
with a completed Invalid requires at least one retained Error, but its phase and
cause remain unavailable. Logical retained byte usage is checked against allowances
and cumulative charged work,
not independently recomputed as Rust layout or heap usage.

## Source checks and explicit limits

`source_witness_verification` is **UNAVAILABLE**. The reader checks strict encoding
decoding, BOM count/compatibility, an optional declaration's encoding label,
UTF-8 byte versus original decoded scalar units, boundaries/bounds, containment,
Node/Attribute roles, path-to-subject consistency and repeated-reference agreement.
Paired healthy observations must agree on root span, encoding and BOM, and on
root references when both are present. This detects disagreement without proving
either observation's lexical truth.
UTF-16 requires the appropriate BOM; Latin1/ASCII require their declared labels.
Scalar bounds exclude BOM and preserve CRLF and reference spellings. The small
header-label inspection is not an XML parser or syntax authority.

These checks do not reconstruct an XML lexical tree, namespaces, sibling occurrence,
preorder, attribute axis or exact token/location truth. Consistently replacing every
reference to a node with the same-shaped wrong preorder can evade these checks;
a regression explicitly demonstrates this limit. Schema tuples remain schema data;
tuple ordering, URI membership and bounds against declared schema size pins are checked.
The excluded XSD bytes and component semantic
truth are unavailable. Containment does not prove a source witness.

Reports also keep execution authenticity, custody, loaded executable identity,
build execution, compiler correspondence, structural equivalence, diagnostic cause
truth, event phases, context/site coverage, hard resources, native acceptance and
benchmark/release eligibility explicitly unavailable. Self-reported source stability
and child status are consistency evidence, not attestations.

## Consumer admission and import policy

The CLI uses one trusted source module and stdlib only; there are no local helper
imports, historical script imports or helper bytecode caches. Use `python -I` as
shown above. Tests compile the current verifier from held source bytes. A modified
installation/interpreter is outside this verifier's trust boundary.

The supported safe filesystem profile is POSIX directory-FD `O_NOFOLLOW`/
`O_DIRECTORY` plus nonblocking file opens. Other profiles return unavailable.
Every attempt ancestor is opened without following links, including the attempt
directory. Files must be single-link regular files: symlinks, FIFOs, directories,
devices and hardlinks are refused. Parent traversal is refused. `/tmp` or `/var`
alias symlinks must be supplied by their actual directory names (`/private/tmp`
on macOS). Filesystem I/O has no hard deadline guarantee.

Only ten fixed names are read: `input.xml`, `source-inventory.json`, `receipt.json`,
`failure.json`, and complete/partial `.json`, `.status.json`, `.stderr`. Unrelated
build/resource files in the directory are not opened. Optional absence is retained;
each opened file is read once and its held bytes supply both hash and parse.
Descriptor reads use the same policy. In-read stat changes/short reads are rejected;
no temporal custody or atomic multi-file snapshot is claimed.

| Consumer bound | Limit |
| --- | ---: |
| Original input | 16 MiB |
| Each JSON/stdout/stderr file | 32 MiB |
| Held bytes per reader | 160 MiB |
| Allowlisted file slots per reader | 12 |
| JSON nesting before decoding | 64 |
| Token/materialized node budget | 200,000 each |
| Each array/object | 50,000 entries |
| Raw string token/decoded string | 1 MiB each |
| Aggregate decoded string bytes | 8 MiB |
| Integer token | 20 bytes; typed u64/u32/i32 bounds |
| JSON report | 16 KiB |

Resource refusal is a consumer availability result, never invoice invalidity.
The byte/token limits bound materialization; they are not a measured RSS guarantee.

## Evidence and tests

[`PROVENANCE.md`](PROVENANCE.md) identifies the exact 141 original files, full 417
reference inventory, public authored XML license and hosted artifact metadata.
Windows CRLF and empty malformed stdout are preserved; `.gitattributes` disables
text conversion for originals. No binaries, XSDs or vendor/config archives are
redistributed. Excluded build/resource bytes are digest references, not verified
present bytes. The verifier does not import acquisition scripts or consult caches.

Genuine positives cover only empty streams or one Error/UndeclaredElement and
actual work-stop partials. Synthetic tests are explicitly grammar-only or negative
mutations. They do not establish native positive coverage for warnings/interleaving,
duplicates, nested value/facet/type payloads, attributes, BOM/alternate encodings,
deferred finalization, callback refusal or terminal capture overflow. Separate
producer acquisition is needed for those genuine cases. No held-out inputs,
labels, family reads, native execution, compiler change or benchmark is part of
this slice. Existing conformance/oracle consumers remain separate and unchanged.

CI runs offline stdlib unit/CLI tests and independently validates these existing
retained fixture bytes and reviewed source407 from immutable merged-revision Git
blobs, independently of the current checkout's producer code. It installs no dependencies and
runs no runtime/oracle. Parent independent reviews, tribunal/adversarial validation,
MSRV/current-main checks and shipping remain separate work.
