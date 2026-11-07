# Expected schedule and synthetic reducer, version 1

This is the bounded BSC-11 protocol predeclaration of 2026-11-07, implemented
before measurement. It checks expected coordinates and exact synthetic
arithmetic. It executes no validator, reads no XML or outcome data, collects no
timers, imports no attestation and authorizes no benchmark. BSC-11 remains open.
BSC-8 owns native runtime/lifecycle; BSC-10 owns correctness, controlled execution
and sealed custody. Their required evidence and approved interfaces remain open.

```sh
python3.12 -B tools/benchmark/protocol.py schedule
python3.12 -B tools/benchmark/protocol.py reduce-fixture INPUT
python3.12 -B -m unittest discover -s tools/benchmark/tests -v
```

Both commands write JSON to stdout only and create no files. Exit 0 means an
expected schedule or synthetic reduction was produced; exit 2 emits a constant
`E_*` error without rates, input paths, record values or exception text. Help is
ordinary usage text. There is no executor, native stub, output-path option,
subset/default/weight override or release decision. Every JSON output has
`release_gate_eligible: false` and `attestation`, `correctness`, `custody`,
`resources`, `execution`, `timing` fixed to `"UNPROVEN"`.

## Fixed schedule and byte bindings

The existing read-only preflight helpers verify all six frozen source pins in
`preflight.py` before parsing public manifest metadata. Those source files,
preflight, oracle/conformance/runtime code and root manifests are unchanged.
Original manifest order selects all 41 complete identities in 19 groups. There
are 40 distinct expected input hashes: equal bytes in different identity slots
remain different slots. XML bytes are not opened or verified by this utility.

`schedule` emits the preflight protocol summary, public slot inventory, groups
in workload insertion order, exact weights and the following expected arrays:

- `baseline`: 41 `{position, slot}` objects in original order.
- `warmup`: 820 `{warmup_pass, position, slot}` objects: passes 0..19, each in
  original order, once before all timed runs.
- `timed`: 2,050 `{run, repeat, position, slot}` objects: runs 0..4, repeats
  0..9, positions 0..40. Ordinal is `410*run + 41*repeat + position`.

A slot object has exactly `slot`, `manifest_row` (original zero-based row),
`group_index`, `input_sha256` and `input_bytes` (expected original bytes).
Groups contain `index`, public `name`, frozen input `count` and exact `weight`.
The five independently reproduced Java vectors start afresh from original order
for each seed `271828+run`; each permutation repeats ten times. Raw vectors and
the small Java source are pinned by SHA-256; see
[provenance](tests/fixtures/schedule/PROVENANCE.md). Baseline/warmup and these
vectors are expectations, never observations or reconstructed legacy identities.
Full-HTML collateral/parity is separate and cannot be pooled as an alternate lane.

`schedule_sha256` hashes the whole schedule object **before adding that field**,
using preflight's sorted-key, compact, literal-Unicode UTF-8 encoding with one
final LF. This commits to six source pins, expected slot/size/hash inventory,
coordinate arrays, raw vector digest, Java source digest, exact current
`protocol.py` and `preflight.py` source digests and safety fields. The existing
preflight ordered identity digest also retains LF. BSC-10's request digest uses
no LF and must not be substituted. Tool/source/fixture hashes identify bytes;
the tool authenticates neither itself nor its producer. No signature or guessed
attestation fields can turn this fixture contract into trusted evidence.

## Exact fixture schema

Only UTF-8 JSON of this shape is accepted. All object fields below are required;
all unknown fields, versions and kinds are rejected. Array order is significant.
No sample row contains a family ID, split label, path, XML or diagnostic text.
Pair and invocation IDs are opaque strings. Lane and pair association are inherited
by their nested rows; these are synthetic coordinates, not observed input identity.

```text
{
  "format": 1,
  "kind": "synthetic_fixture",
  "schedule_sha256": "<current schedule's lowercase hex digest>",
  "pairs": [
    {
      "pair_id": "opaque_pair_id",
      "lanes": [ <reference lane>, <native lane> ]
    }
  ]
}

lane = {
  "lane": "reference" | "native",
  "mode": "primary",
  "worker_count": 1,
  "invocation_id": "globally_unique_opaque_invocation_id",
  "clock_id": "lane_local_clock",
  "envelope": {
    "clock_id": "same_lane_local_clock",
    "start_ns": integer,
    "end_ns": integer
  },
  "terminal": {
    "status": "success",
    "initial_boundary_valid": true,
    "baseline_complete": true,
    "warmup_complete": true,
    "timed_complete": true,
    "document_cleanup_complete": true,
    "prepared_state_retired": true,
    "terminal_flush_complete": true,
    "worker_oom": false,
    "timed_out": false,
    "worker_exit_code": 0,
    "supervisor_exit_code": 0
  },
  "samples": [
    {
      "run": integer,
      "repeat": integer,
      "position": integer,
      "slot": integer,
      "clock_id": "same_lane_local_clock",
      "start_ns": integer,
      "end_ns": integer,
      "duration_ns": integer
    }
  ]
}
```

One fixture contains 1..10 pairs, with unique pair IDs and globally unique
invocation IDs. Each pair has exactly two lanes in reference/native order, each
with exactly 2,050 ordered samples. Coordinates/slots must equal the fixed plan;
counts alone are insufficient. Neither cross-lane clock comparisons nor shared
absolute process timestamps are assumed. Every sample uses its lane's clock,
fits its envelope, has `duration_ns=end_ns-start_ns`, and starts at or after the
preceding sample end. Zero-length samples are allowed. Every run weighted cost
and every native group denominator must be positive; otherwise no comparable
report is produced.

The terminal fields are **declared fixture assumptions only**. False, missing,
unknown, incomplete or failing terminal state prevents rates; validation-worker
OOM prevents rates even if supervisor exit is zero. A completed invoice rejection
is different from an operational failure; BSC-10 owns those semantics. These
flags cannot prove execution, original-byte evaluation, warmup, cleanup or the
initial exclusion. No imported report, native kind, eligibility or attestation
field is accepted. There is no generic event/epoch ledger or retirement policy.

Inputs are bounded to 16 MiB before decoding; regular files only, with symlinks
in any path component rejected through the existing preflight read helper.
The exact original bytes remain the parse/hash input, including whitespace and
sample order. Reads are bounded and nonblocking for nonregular paths. This is
ordinary local integrity handling, not protection against a hostile filesystem
writer racing path checks.

Predecode nesting is at most 8 containers. The decoded budget is 1,000,000
nodes (keys count), at most 32 fields/object, 2,050 items/array and 64 Unicode
characters/string. All strings reject NUL and surrogate code points. Opaque IDs
and clock IDs are 1..64 ASCII letters, digits, underscores or hyphens. Integers
are JSON integers in 0..2^63-1 with at most 19 token characters; booleans do not
substitute for integers. Floats, nonfinite values, duplicate keys and malformed
UTF-8/JSON are rejected. Schema validation further constrains coordinates, IDs,
field counts and scalar values. These budgets accommodate all 41,000 timed
rows of a valid ten-pair fixture, including long clocks and large timestamps;
excess whitespace can still exceed the file budget. No evidence is truncated.

## Chosen exact arithmetic and report

For lane e, pair p, run r, group g with frozen count n_g:

```text
mu[e,p,r,g] = sum(sample_ns) / (10*n_g)
D[e,p,r]    = sum_g(w_g * mu[e,p,r,g])
throughput[e,p,r] = 1e9 / D[e,p,r]
D[e,p]      = sum_r(D[e,p,r]) / 5
pooled service ratio = sum_p(D[reference,p]) / sum_p(D[native,p])
```

Weights use `Fraction.from_float` on the unchanged binary64-loaded workload
weights. Each is exactly `948126237341157/18014398509481984`; their sum is
`18014398509481983/18014398509481984`. Do not normalize or replace with 1/19.
This audits the frozen mathematical formula, not bit-identical intermediate
float accumulation. Original reference reports must be retained separately.

All calculations use integer sums and exact fractions. Every rational is an
object with decimal string `numerator`, positive decimal string `denominator`
and six-place `display`, rounded ties-to-even using integer arithmetic only at
the end. Display never feeds another calculation or comparison. No numeric pass,
missed-target or release gate result is emitted, including for exact or displayed
five. There is no best-run selection, trimming, median/geometric speedup reducer,
mean of reciprocals or post-result weighting.

Each lane reports all five runs and their group distributions, exact weighted
cost and throughput. Each distribution has `count`, integer `sum_ns`, rational
`mean_ns`, rational `p50_ns` (median, mean of middle two for even counts), and
integer `p95_ns` at sorted zero-based index `min(n-1,floor(95*n/100))`.
Lane-wide and pooled distributions keep the same conventions. Sorting is a derived
view; original fixture bytes are preserved by `fixture_sha256` and `fixture_bytes`.

Every pair reports its service ratio, conditional sufficient-only bound and 19
group service ratios. The report includes pooled ratios/distributions, every
pair, pair service/bound min/max and leave-one-pair-out pooled sensitivity
(empty for one pair). Group ratios divide pooled reference/native group means;
fixed equal counts make this equal to dividing their integer sums. These are
synthetic descriptive outputs, not confidence intervals or independence claims.
`fixture_pair_count` distinguishes the supplied 1..10 pairs from
`future_collection_pair_count: 10`. There is no per-input performance claim.

## Conditional lifecycle bound and limits

For a sequential native worker, E begins immediately after **one independently
approved** initial-ready preparation and original-byte preload, before baseline.
It ends after mandatory final document cleanup, prepared-schema/interner
retirement and terminal flush/teardown. E includes baseline, all warmups, later
retire/recompile, every gap/run boundary and output overhead. The excluded initial
preparation cannot contain parsed DOM/results, extra ready epochs or advance
payment of later maintenance. This utility verifies none of those facts.
Startup, RSS, actual worker-tree resources and multiworker results need separate
evidence. No retirement threshold is invented.

```text
S[p] = sum(all 2,050 native timed durations)
H[p] = E[p] - S[p]
a_max = max_g(w_g / (5*10*n_g)) = w/50
U[p] = D[native,p] + a_max*H[p]
sufficient-only bound = sum_p(D[reference,p]) / sum_p(U[p])
```

Native output includes `elapsed_ns`, `timed_sum_ns`, `residual_ns`, exact `a_max`
and `upper_sensitivity_cost_ns`. Reference E/S/H is visible but its gaps never
inflate the numerator. Unobserved categories remain fully charged residual;
conservation E=S+H proves arithmetic only. Bound<5 is **inconclusive**, never an
automatic missed target. Bound>=5 would be sufficient only after independently
establishing all premises and correctness/custody/resource/execution gates.
Fixture flags and hashes establish none of these, so eligibility stays false.

For fixed timed-sample semantics, any nonnegative residual allocation with total
H_required<=H contributes at most a_max*H. Moving c ns from a native group-g
timer into the admitted envelope gap changes U by `(a_max-w_g/(50*n_g))*c>=0`.
Moving that residual between boundaries or terminal positions cannot improve the
bound. This does not cover arbitrary reassignment of already-timed maintenance.
Hidden cleanup, wrong clocks or work migrated into an unverified initial exclusion
invalidate the premises. Baseline/warmup inclusion is pessimistic, not an unbiased
warmed-throughput estimate. A stricter a_max*E guard is an alternative design,
not a selectable reducer here. If this guard is inconclusive, a faithful lifecycle
measurement protocol must be prospectively reviewed/predeclared; favorable
post-hoc substitutions and silent replacement of prior evidence are prohibited.

## Future collection design, not execution authorization

The predeclared future primary batch is ten fresh process pairs, each retaining
20 warmup passes once, five runs and ten repeats. Adjacent lane order is
`JN,NJ,NJ,JN,JN,NJ,NJ,JN,JN,NJ`, sequential under the same actual total worker
budget. These five dependent within-process runs are not independent replicates.
Counterbalancing does not prove process independence or remove host/thermal/VM
effects. All attempts need stable pre-execution identities; preserve failures,
incomplete batches and all observations. No selective replacement, optional
stopping or best-batch selection. Changed replication policy requires a version
predeclared before its measurements and preservation of prior evidence.

Ten pairs is a future design, not an additional all-pairs>=5 acceptance criterion.
The ticket target remains >=5 on the agreed representative aggregate. No
confidence interval or sign-test release claim is made. Actual capture needs
original buffer hashes, observed sample correspondence, independently trusted
execution and reviewed BSC-8/BSC-10 interfaces. Legacy group-only timing arrays
cannot be promoted to identified observations by the expected schedule.
