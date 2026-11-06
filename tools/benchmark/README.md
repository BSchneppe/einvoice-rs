# BSC-11 preparation bundles, version 1

`preflight.py` is a Python 3.12 standard-library preparation tool. It inventories a
fixed protocol and records this collector process's visible resource constraints.
It does not run a validator, a benchmark, an enforcement canary or an arbitrary
command. It has no adapter, correctness evaluator, container integration, network
operation, refresh command or release override.

```sh
python3.12 -B tools/benchmark/preflight.py prepare --output NEW_DIRECTORY
python3.12 -B tools/benchmark/preflight.py compare LEFT_BUNDLE RIGHT_BUNDLE --output NEW_DIRECTORY
python3.12 -B -m unittest discover -s tools/benchmark/tests -v
```

Destinations must not exist, including files, directories and dangling symlinks.
Their parent must exist. A failed operation can leave an incomplete directory;
retain it for investigation and choose a new destination. Never reuse evidence.
Comparison destinations must also be outside both input bundles. Containment is
checked before creating anything, including relative and `..` path aliases.

Exit 0 means the preparation or comparison operation completed. Exit 1 means a
comparison observed at least one difference. Exit 2 means invalid, tampered,
incomplete, unsupported-version, inaccessible or out-of-bounds input, or an I/O
failure. An unsupported OS can produce a complete preparation bundle with
unavailable observations and exit 0. Unsupported Linux cgroup layouts produce an
explicit gap; malformed or unsafe paths/control values fail with exit 2. Neither
exit 0 nor `complete: true` authorizes timing. Stdout contains only a constant
completion status, operation kind and safety fields; stderr errors contain a
constant `E_*` code and safety fields. No input identities, paths or raw exception
messages are printed. `--help` prints usage.

Every report and CLI status/error fixes `release_gate_eligible` to literal `false`
and these six fields to `"UNPROVEN"`: `effective_budget`, `execution`,
`enforcement`, `exclusivity`, `correctness`, `timing`. There is no ≥5x result or
preflight pass. BSC-12 consumes later performance evidence, never this exit status.
Full BSC-11 and its ≥5x gate remain unresolved.

The dedicated workflow uses the runner's existing `python3.12` for offline tests
and a Linux collector smoke. It has no runtime/artifact download or installation
step and does not alter the existing workspace CI.

## Frozen protocol identity

`PINS` in `preflight.py` is the reviewed byte inventory at
`756c15089dbd8d3fe586be4486641501338fa4ab`. In fixed insertion order it binds:

| Bundle artifact | Source under `tools/oracle/` |
| --- | --- |
| `protocol-0.bin` | `workload.json` |
| `protocol-1.bin` | `corpus/initial.jsonl` |
| `protocol-2.bin` | `corpus/split.json` |
| `protocol-3.bin` | `oracle.py` |
| `protocol-4.bin` | `java/Oracle.java` |
| `protocol-5.bin` | `artifacts.lock.json` |

The tool checks exact sizes and SHA-256 values before deriving the summary or
collecting resources. Imports check the same fixed pins; producer-supplied hashes
cannot replace them. Changing any byte requires a deliberate reviewed source-pin
revision. This is a narrow inventory, not an oracle/cache verifier. The driver,
bridge, split and artifact lock are opaque pinned bytes; none are executed or
imported. No outcome labels/results or XML inputs are opened. The initial
manifest contains public input metadata, including held-out membership, and is
copied inside the bundle; protect the bundle accordingly. Reports and errors do
not dump held-out identities or observations.

The complete-scope selection preserves the original manifest order and all 41
identities even when input bytes are equal. There are 19 equally weighted groups.
The summary includes group cardinalities, original weights, 20 warmup passes once
before all 5 runs, 10 repeats, 410 samples/run and 2,050 samples/mode. These are
protocol expectations, never observed samples. `ordered_identity_sha256` hashes
the UTF-8 canonical JSON array of `[id, input_sha256, group, split]` for each
selected row in order. Canonical JSON here means sorted object keys, compact
separators, literal Unicode, finite numbers only and one final LF. This is a local
protocol-identity digest, not a new BSC-10 request/attestation canonicalization.

## Read-only resource capture

Linux capture supports only an unambiguous unified cgroup v2 membership and one
matching cgroup2 mount under `/sys/fs/cgroup`. It resolves the membership relative
to the mount root, validates every component and confines resolved reads. It
reads the leaf and each visible ancestor up to that mount, at most 64 nodes.
`/proc/self` file reads use the collector's numeric PID; namespace identities use
only the specific `/proc/self/ns/{cgroup,mnt}` links. Symlink components in other
file paths are rejected. Nonregular files are rejected and reads are bounded.
There is no proc-root or fixture-path CLI option. The internal `LinuxReader`
dependency seam marks supplied test roots `fixture`.

A capture records before and after snapshots, each containing:

- allowlisted host hostname, machine ID, boot ID, kernel release, architecture,
  cgroup namespace and mount namespace identity;
- raw `/proc/self/cgroup`, only cgroup2 lines from mountinfo, and sorted collector
  process affinity from `sched_getaffinity(0)`;
- raw `cgroup.controllers`, `cpu.max` (quota **and period**),
  `cpuset.cpus.effective`, `memory.max`, `memory.swap.max` and `pids.max` at every
  visible ancestor. Unreadable/missing files are `null`.

No topology, processor-count substitute, environment dump or machine label is
used to infer allocation. Darwin and other unsupported systems record null
snapshots with `UNSUPPORTED_PLATFORM`. Collector metadata includes PID, Python
patch version and UTC capture time; it does not attest the whole Python install.

Raw controls are independently parsed on bundle import. A resource-state change
between snapshots adds `RESOURCE_STATE_DRIFT`. Missing identities, affinity,
controllers, layout or controls, empty effective cpusets and affinity outside a
recorded cpuset add explicit gaps. Controller availability/readback is not active
enforcement evidence. These reads can race changes within a snapshot or changes
that revert between snapshots; they cannot establish stability over a future run.
Every capture also carries `CORRECTNESS_ATTESTATION_UNAVAILABLE` and
`TIMING_AUTHORIZATION_UNAVAILABLE` gaps.

Every present membership/mount field is validated before layout availability is
decided. A missing counterpart or unsupported hybrid layout does not waive type,
length or recognized path-safety checks. Well-formed unavailable observations
still produce gaps.

`HIDDEN_ANCESTORS_UNPROVEN` and `NAMESPACE_SCOPE_UNPROVEN` are always present for
Linux snapshots, **including cgroup namespace root `/` and mount root `/`**.
Neither certifies access to all effective ancestor limits. The collector is not
a validation worker or its child/thread tree. Quota is a ceiling, affinity/cpuset
is placement, and none guarantees CPU service, reservation or exclusivity.

## Exact version 1 bundle contract

All JSON uses UTF-8; duplicate keys, nonfinite numbers, invalid Unicode, unknown
schema fields and bool-as-integer substitutions are rejected. Generated JSON
uses the canonical encoding above. Imported JSON may use other whitespace/key
ordering; recomputed reports must have exactly the same canonical representation.
Only preparation bundles are accepted by `compare`; comparison outputs are
terminal reports, not preparation evidence.

A preparation directory contains exactly the six protocol files above plus:

| File | Contents |
| --- | --- |
| `tool.py` | Exact current `preflight.py` bytes; a different tool version is rejected on import |
| `resources.json` | Raw resource capture described below |
| `report.json` | Recomputed preparation summary described below |
| `complete.json` | Completion manifest, installed last |

The completion manifest has exactly `format: 1`, `kind: "preparation"`,
`complete: true`, `release_gate_eligible: false`, and `artifacts`. `artifacts`
maps every other filename to exactly `{bytes: integer, sha256: lowercase_hex64}`.
Missing/extra files, symlinks, unexpected names and path traversal are rejected.
Every artifact's bytes are read and checked. The manifest does not hash itself.
A comparison directory has the same manifest structure with `kind: "comparison"`
and exactly `report.json` and `tool.py` as its artifacts.

Artifacts are exclusively written, flushed and fsynced. `.completion.tmp` is
written/fsynced and atomically renamed to `complete.json` last in the newly
created directory. An interruption before this final rename leaves invalid
incomplete evidence. This is a completion/integrity convention, not a signature,
a filesystem power-loss guarantee or protection from a hostile writer racing
filesystem operations. Digests and tool bytes alone cannot attest truthful or
fresh execution; a party able to rewrite raw data and all digests can fabricate
observations. Imported capture origin is a recorded claim, never independently
attested provenance.

`resources.json` has exactly:

- `format: 1`, `origin: "live_self_process" | "fixture"`,
  `scope: "collector_process_only"`, `platform: string`;
- `collector: {pid: positive_integer, python: "3.12.PATCH", captured_at:
  "YYYY-MM-DDTHH:MM:SS.ffffff+00:00"}`;
- `before`, `after`: snapshots for Linux, both `null` otherwise.

A snapshot has exactly `identity`, `cgroup`, `mounts`, `affinity`, `ancestors`.
`identity` maps exactly `host`, `machine`, `boot`, `kernel`, `arch`, `cgroup_ns`,
`mount_ns` to nonempty strings or `null`. `cgroup` is raw membership text or
`null`; `mounts` is a list of raw cgroup2 mount lines or `null`; `affinity` is a
nonempty sorted unique integer CPU list or `null`. `ancestors` lists exactly the
resolved visible paths, leaf first, each as `{path, controls}`; `controls` maps
exactly the six allowlisted control filenames to raw strings or `null`. An
unsupported/unavailable layout has no ancestor entries. No producer-provided
`enforced`, ancestor-completeness, promotion or effective-budget key is accepted.

`report.json` has exactly `format: 1`, `kind: "preparation"`, `protocol`,
`resources`, and the seven invariant safety fields. `protocol` contains `base`,
`identity_scope: "protocol_bytes_only"`, `complete_inputs`,
`ordered_identity_sha256`, `group_counts`, `weights`, `warmup_passes`,
`warmup_placement: "once_before_all_runs"`, `runs`, `repeats`, `samples_per_run`,
`samples_per_mode`, `aggregation`, `sources` (original names to pinned
`{bytes, sha256}`). `resources` contains exactly normalized `before`, `after` and
sorted unique `gaps`. Normalized snapshots preserve identity, membership,
mount lines, affinity and ancestor paths. Control values become integer or
`"max"` for limits, `[quota_or_max, period_integer]` for `cpu.max`, sorted integer
CPU lists for cpusets, sorted controller-name lists, or `null` for unavailable
facts. Import recomputes the entire report from pinned source and raw resource
artifacts, rather than trusting a producer-derived summary or false release flag.

Comparison reports have exactly `format: 1`,
`kind: "visible_constraint_comparison"`, `left_manifest_sha256`,
`right_manifest_sha256`, `visible_constraints`, `differences`, `gaps` and the
seven safety fields. Both input bundles undergo independent complete validation.
The comparison checks host/boot/kernel/architecture/namespace scope, membership,
mounts, affinity, each ancestor's full normalized controls and platform. PID and
capture timestamp are not compared as resource constraints.

`visible_constraints` is `"different"` if any known facts differ, `"equal"` if
Linux snapshots exist and their jointly available facts have no difference, or
`"unavailable"` if no paired Linux snapshots exist. Equal partial captures can
have gaps, even no usable controls. The field describes only available visible
observations. `differences` lists sorted field paths, without observed values;
`gaps` is the independent union of both inputs' gaps. Unknown facts do not erase
known differences, and equality cannot resolve unknown allocation. Fixtures keep
their origin and `FIXTURE_NOT_LIVE_EVIDENCE` gap through comparison. All imports
remain observations without execution attestation even when claiming live origin.
There is no `same_effective_budget` field.

Bounds: each artifact is at most 2 MiB; JSON depth 12, total visited nodes 30,000,
list/object cardinality 8,192, string length 262,144, integer magnitude at most
2^63−1 and integer token length 20. Resource paths are at most 4,096 characters
and 64 components. There are at most 16 cgroup2 mount lines of 8,192 characters,
64 ancestors, 64 controller names of 32 letters/underscores, 32,768 characters
per raw control, and 4,096 affinity/cpuset CPUs with IDs at most 1,048,575. CPU-list
ranges are bounded before expansion and must be ordered/nonoverlapping. Resource
numbers have at most 19 decimal digits; quotas/periods are positive and periods
cannot be `max`. Identity strings are at most 1,024 characters. Bounds violations
fail closed; the tool never truncates evidence into an apparently complete fact.

## Future owner handoff — requirements, not current APIs

BSC-8 owns the actual integrated native adapter and diagnostic serialization.
BSC-10 owns the independent correctness evaluator and separately reviewed
controlled-execution attestation. Neither is supplied by this tool. Later
integration requires reviewed owner contracts; no current attestation key names
are invented here. `bsc10-native-v1`, `kind=native`, self-report flags, signatures,
digests, parity alone and exit 0 are insufficient to authorize timing. Required
correctness includes independent full comparison, structural cause equivalence,
scoped/adversarial/family coverage, policy and resource evidence, with every gap
resolved. It must bind evaluator/code/policy/tests, source/input/split/label and
evidence versions, actual result/sidecar bytes and custody to fresh execution.

The exact release executable, build profile, compiler/toolchain/target, source
revision/dirty state and loaded backend/generator/package/serializer closure must
match the artifact actually timed. Bind ordered input, run, repeat and sample
identities, phase boundaries, reference bridge and request/result bytes. The
current reference has group/run arrays, **no per-sample document IDs**. Java's
`Collections.shuffle(new Random(271828L + run))` cannot be replaced with Python
shuffle using the same seed. Any schedule reconstruction or new reference capture
boundary requires independent verification and review.

Keep the unchanged within-run aggregate `1e9 / sum(weight * mean_ns)` and every
frozen group weight. Preserve all raw samples, groups and runs. The cross-run
Rust/reference reducer, uncertainty/repetition policy and lifecycle-cost
allocation remain undecided: justify and predeclare them **before timing**, then
do not choose a favorable reducer from results. Timed work includes fresh
per-document parsing/DOM/globals/diagnostics, all XSD/CEN/XR/policy evaluation,
full diagnostic extraction and JSON serialization. Worker epoch retirement and
prepared-schema recompilation count toward warmed lifecycle work, including
interruptions between per-document timers; reconcile those costs with lifecycle
elapsed time and samples. No invisible reset or diagnostic pruning is allowed.
Initialization, RSS, startup and multiworker results require separate evidence;
heap-pool peaks are not RSS and whole-process wall time is not isolated startup.

Future execution must establish the same **actual total worker budget** across
lanes, not one full budget per worker. Bind/recheck every actual worker/child/thread
and resource drift across execution; readback alone cannot establish enforcement,
all ancestors, exclusivity, topology, host/VM scheduling or a quiet window. The
historical expected bounded canary child OOM plus supervisor exit 0 establishes
enforcement only. Any **actual validation-worker OOM** must make its run
failed/incomplete regardless of wrapper/container exit 0. This slice has no OOM
importer or executor.

The environment contract requires equivalent pinned runtime versions and fair
paired execution on the same hardware/CPU budget. Native Linux x64 is a
recommendation; separately verified and pinned native Linux arm64 remains an
alternative if the unchanged oracle and fair pairing are reproduced. The existing
CI Linux x64 Corretto archive does not impose an architecture whitelist. Do not
silently substitute a frozen artifact, infer full runtime provenance from version
strings, or treat Darwin binaries as Linux evidence. Provisioning, downloads and
real timing require later concrete evidence and a parent-coordinated quiet window.
