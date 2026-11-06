# Version 1 publication contract

`bsc10-development-publication-v1` is `publication.json`: exact relative path,
byte length and SHA-256 for every shipped module code, test, documentation,
evidence, metadata and notice file. Only the inventory itself and generated
`__pycache__` directories are excluded. The CI workflow outside the module is
reviewed in Git. Reviewed Git history is the trust anchor; a rewritten inventory
and verifier cannot authenticate themselves. No public automatic refresh exists.

Direct entry points load fixed sibling helpers by compiling held source bytes,
without Python's bytecode import path, including redirected caches. Reproduction
loads the verifier, which loads common; later imports use those initialized
instances. The inventory must match these executed helper snapshots. The small
bootstrap is duplicated in the two trusted entry points to avoid depending on
an unchecked helper to establish source loading. Bootstrap failures stop setup
before report creation. The interpreter, standard library and entry point remain
trusted; this is not hostile-host containment. `-B` alone only suppresses cache
writes. Nonblocking leaf opens reach descriptor type checks for FIFOs promptly.

`bsc10-development-selection-v1` maps each of the exact 799 historical identities
to a retained regular path or an explicit external/derived/excluded disposition
and reason. Identical references resolve directly to a canonical retained row;
cycles, chains and dangling aliases are invalid. External payloads are unavailable
unless explicitly supplied; class recipes are not rebuilt by offline verification.
The old manifest and all old digests remain historical commitments.

`bsc10-development-replay-v1` describes fresh local reproduction. Declarations bind
the publication, runner/contract and requested positions. Each batch's
`operational-closure.json` lists actual consumed owned files and pins runtime
identities; its TSV commits those same files for the unchanged driver API.
The closure excludes its own manifest, generated receipts and execution outputs.
Output inventory commits actual outputs including partial streams; final result
and output inventory itself are named exclusions to avoid cycles. No historical
digest is reused as an operational declaration. Runtime identities cannot attest
transitive OS/JVM state or prevent a malicious host from substituting execution.

V01–V04: strict UTF-8 JSON rejects duplicate keys, nonfinite/overflow values and
lone surrogates. Byte snapshots are read once for hashing and parsing. No symlink,
hardlink, traversal or undeclared file is accepted. All three draft ledgers,
predecessors, revisions, supersessions, family allocation and byte recipes are
checked: descending original-offset splices, spans/old hashes, unique anchors,
original byte line/column, exact recipe serialization, encoding and explicit BOM.
No XML parse/reserialization is used to reproduce original inputs.

V05–V08: requests bind exact freeze IDs/order/families/hashes/sizes and unknown
prelabel expectations, TSV/JSON/declaration, input/output receipts, closure and
driver/ledger/freeze. Processes bind actual command argv/cwd/status/stream hashes.
The four original failed guard attempts remain process failures. Raw family
stdout equals the ordered raw records exactly. Corrected discriminant stdout has
exactly one record per process. Derived records and diagnostic occurrence arrays
retain all fields, duplicates, null coordinates, untrimmed messages and per-bucket
order. No global cross-bucket event ordering or full SVRL is inferred.

V09–V12: every outcome has recognized stage/processing/recommendation fields and
consistent projections. Missing XML evidence is not rejection evidence. A valid
XML/XSD exact target requires both CEN and XR stages. Invalid XSD is a legitimate
completed schema rejection with business stages absent. Invalid CEN may coexist
with effective-policy acceptance; accepted records must have an acceptable
recommendation and no effective errors. Unknown/undefined, unmatched, missing
applicable stages and processing failures cannot manufacture completed target
evaluation. Explicit invalid XML is not inherently a processing failure, but
v1 returns `unsupported_xml_terminal` for that unobserved terminal shape; no actual
malformed XML occurs in these 64 records. Synthetic tooling fixtures cannot add
reference labels. Scoped records cannot pass the complete-document boundary.
No acceptance-total expectation is used as an oracle.

Supported completed classifications also enforce the pinned report contract:
base warning/error requires an invalid associated stage, and each invalid stage
requires a base warning/error observation from that stage. Effective overrides
do not alter these base verdicts. Failed/incomplete processing and unsupported
XML terminal classifications retain their separate boundaries.

V13: optional dependencies are explicit, read-only, held verified archive
snapshots. Extraction checks traversal, duplicate/symlink members and member pins;
runner extraction requires the exact 97-file config inventory. Only fixed named
dependencies are read, never Parquet or unspecified cache files. New replay
classes must match their original derived pins. Original executable/release
hashes/realpaths and versions are required; no fallback runtime is selected.

Each attempted process gets `command.json` before launch, raw stdout/stderr, and
metadata with attempted/started, status (`exited`, `launch_error`, `timeout`,
`interrupted`), nullable exit, exception and stream hashes. Nonzero/timeout stays
incomplete even after every record arrived. Fail-fast skips are unattempted;
missing request receipts mean no request-specific evidence, not proof of no reach.
The corrected discriminant driver has only a returned receipt, no separate input
receipt. Abrupt SIGKILL/storage failure may prevent final accounting: the durable
prelaunch declaration/incomplete marker and unsuccessful exit still forbid success.

Verifier exits 0 for verified retained evidence, 2 for invalid evidence. Runner
exits 0 only for verified completed selected requests; otherwise 2. Byte differences
are separately reported and never change labels. Reports never claim native,
release or timing eligibility. Reports/attempt directories must be fresh and
outside all protected input trees; existing evidence is never overwritten.
