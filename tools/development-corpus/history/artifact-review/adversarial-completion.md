# Independent adversarial completion review

**S1 CONFIRMED (major, prospective verifier scope); S2 CONFIRMED (minor, standalone reporting scope); S3 DOWNGRADE (publication provenance cross-reference, not unavailable source provenance).** No retained invoice observation was disproved. The malformed-XML portion of S1 needs narrower wording, and the claim that the manifested bundle has no upstream seed locator is false.

Reviewed HEAD: `e89b2fd2f3bd3701197f18f76fd2c50c0422a99b`, branch `benediktschneppe/bsc-10-prove-corpus-coverage-and`. HEAD and merged commit `25fcbb5552366610392fb35ddc67c1e5bdecc539` both resolve to tree `f9f1043f2788401bd414677fc0dd5bd86f405700`. This is an artifact review, not a tracked-diff review. Root `/Users/bene/RustroverProjects/rust-einvoice/AGENTS.md` was read; no project/backlog change was requested or made.

Paths cited below are relative to `.scratch/bsc10/expansion/` unless stated otherwise. The two provisional reports were treated as claims. I wrote independent probes without reading or rerunning either critic's probe implementation/results. No child delegation, tracked edits, fixes, packaging, commits, PRs, oracle reruns, Java/native/compiler execution, held-out access, dataset access, benchmark or timing work occurred. Shell invocations used `/bin/bash`, `login=false`; Python was the supplied absolute interpreter, version 3.12.6, with `-B`. The original frozen 39-file corpus and shared artifact cache were not reopened. Manifested binaries were hashed as evidence, never executed.

## Integrity and probe limits

Manifest `artifact-review/manifest.json`, SHA-256 `34924b22c0d9be1f6fe3d78fe6a2ee496793e466c27bddea7e20cd7946b68a26`, contains exactly 799 unique members: 201 family-design, 394 family-run, 204 discriminant. Every member's byte length and SHA-256 matched before and after this review. Evidence: `artifact-review/adversarial-completion/integrity-before.json` and `integrity-after.json`. No manifested member was edited. All new probe files are under `artifact-review/adversarial-completion/`; this report is the separately authorized adjacent output.

The final independent probe is `artifact-review/adversarial-completion/probe-v2.py`; definitive results are `independent-02/results.json` below that directory. It runs 21 family summarizer probes, eight discriminant classification-slice probes and four report-slice probes. Each has fresh synthetic fixture/output paths. These are reporting-contract tests, never new reference labels, observed KoSIT behavior or a second business-rule evaluator.

The family tests execute the unchanged summarizer's entire `main` AST with bounded replacement storage helpers/constants. They do not import `common` or execute its frozen-corpus checker. The discriminant tests execute only the exact result-classification statements at `run_v2.py:80–86,91`, and observation-binding/table statements at `report_results.py:15–25,45–53`, plus the exact successful-processing string expression at line 54. They do not execute either runner or the full report generator. In particular, these slices do not establish that manipulated fixtures pass the original whole dependency/closure audit. They isolate the missing semantic checks after record binding. Their unchanged source-file hashes are recorded in results.

Initial harness `probe.py` and `independent-01/` are retained. Its family and runner probes succeeded, but its report-slice AST mistakenly included the entire multi-line tail list beginning at line 54 and encountered an unrelated undefined `JDK` dependency. Those two NameErrors are harness failures, not evidence about the product scripts. Version 2 evaluates only the exact prose expression; the corrected positive controls pass, and all final expected failure/success controls are asserted. No initial result was overwritten.

To reproduce into a **new, unused** output child:

```sh
/Users/bene/.t3/worktrees/rust-einvoice/benediktschneppe-bsc-5--45552c76f3d0fedb/tools/oracle/.cache/test-venv/bin/python -B .scratch/bsc10/expansion/artifact-review/adversarial-completion/probe-v2.py fresh-review-03
```

## S1 — CONFIRMED, with two narrowed subclaims

**Evidence.** `family-run/summarize.py:30–35` checks schema/business/scenario field consistency and requires sufficient stages only for `acceptance == accepted`. The completion predicate at lines 50–64 requires a completed processing string but never demands applicable stage sufficiency for rejected records. The unchanged bridge at `family-run/snapshots/Oracle.java:89–103` separately exposes processing, stage completeness and acceptance; its fallback can return `rejected` without complete target-stage evidence.

Independent results, with the baseline synthetically changed and matching synthetic output receipt/stream bytes:

| Synthetic record shape | Summarizer result |
| --- | --- |
| `completed`, `rejected`, no stages, schema/business `not_evaluated`, recommendation `undefined` | `complete`, 60 observed, zero contract errors |
| Rejected with XSD and both business stages present, XML stage absent | `complete`, zero contract errors |
| Valid XML/XSD, rejected, neither business stage present | `complete`, zero contract errors |
| Valid XML/XSD, rejected, only SCH.1 present | `complete`, zero contract errors |
| Full stage keys but undefined recommendation | `complete`; the omission also survives on an accepted record |

The valid XML/XSD cases missing business stages are the clearest counterexamples: they lack an earlier structural terminal rejection that could explain stopping. No inference about an invoice business rule or its expected verdict is needed. Retaining a raw record is appropriate; calling the batch sufficiently evaluated for publication is not supported by these checks.

**Discriminant scope.** The exact classification slice calls both a failed-processing/indeterminate record with all stage keys and a rejected empty-stage record `observed`; its final status check passes. `observed` is not itself an assertion of processing success. The reporting slice is the problem downstream: with a bound failed-processing/indeterminate record that retains all four stage keys, the table renders and the exact line-54 expression still says “All four processed successfully”. Initial XSD-pass prose at line 47 is also unconditional. Thus future successful-processing prose needs semantic and process checks, separately from raw observation retention.

**Counterevidence and controls.** All 64 original completed records have `processing: completed`, empty processing-error arrays, valid XML and the matched CII target. All four original family XSD-invalid records retain structural diagnostics and correctly leave business stages unevaluated. The remaining 56 family records, plus all four corrected discriminants, retain both business-stage statuses. The untouched summarizer control returns complete and preserves every original record value and finding occurrence. Two accepted family records have an invalid CEN stage; their recorded recommendation is still `acceptable`. Requiring all business stages to be valid would corrupt legitimate retained observations.

The accepted-missing-XML and accepted-invalid-XSD controls fail with `acceptance_stage_contract`. Explicit failed/indeterminate processing, missing bridge output, missing output receipt, synthetic timeout metadata and nonzero process exit all produce `failed_or_incomplete`. Failed processing paired with `rejected` additionally triggers the existing acceptance contract. Wrong discriminant ID/hash, zero/two returned JSON records and an invalid receipt fail their existing binding/cardinality checks.

Two claims are narrower than the provisional report suggests:

1. An XML-invalid result is not automatically an infrastructure/processing failure. The synthetic XML-invalid/no-target shape is counted complete, but synthetic reachability does not establish how pinned KoSIT classifies that case. The retained report schema, `family-run/config/resources/xsd/report.xsd:56–67`, explicitly permits an XML-well-formedness failure under `noScenarioMatched`. An explicitly evaluated invalid XML stage can be a terminal XML-validation observation if supported by the record/process evidence; it cannot stand for business validation. No actual malformed-XML run exists among these 64 records. Reject the blanket inference “XML invalid implies failed processing”.
2. The full discriminant report does not silently render every empty-stage shape: its direct table indexing at `report_results.py:53` raises `KeyError('val-xml')` in the missing-stage control. The dynamic prose reproduction concerns failed-processing records **with the required table keys**, not arbitrary empty maps. A raw-binding mismatch also fails before prose rendering.

**Supported progression, without business-rule interpretation.** This is a logical evidence requirement, not a claim about map serialization order or internal temporal events:

| Recorded evidence | Supported conclusion |
| --- | --- |
| Missing output, timeout/launch/nonzero exit, failed processing or an exception | Preserve the attempt/error and any partial output; no completed invoice-validation outcome is established. |
| Absent XML-stage evidence with no explicitly evidenced terminal path | Incomplete/unclassified execution; a rejected bridge fallback cannot supply missing evaluation evidence. |
| Explicit invalid XML, with supported terminal/process evidence | XML rejection only; schema/business remain unevaluated. This review does not prove the engine's generic processing/recommendation state for that path. |
| Valid XML, matched CII target, explicitly invalid XSD, completed processing | Schema rejection; absent business stages are legitimate here. All four retained family examples demonstrate this path. |
| Valid XML and XSD, matched CII target, completed processing | Both configured business-stage results must be present with recognized statuses before claiming completed target business evaluation. Either may be invalid. |
| Both business stages evaluated | Preserve the actual recommendation/acceptance and all diagnostics, including effective overrides; do not derive acceptance by AND-ing stage validity or predicting particular rules. |
| Undefined recommendation, unknown stage/status, unmatched target or inconsistent fields | Explicitly retain and classify that state; do not manufacture target acceptance or rejection evidence. |

The original full CII configuration declares one XSD resource and two Schematron resources at `config/scenarios.xml:334–375`. This source structure identifies the applicable stages, not assertion hits or coverage.

**Proportionate correction.** Keep S1 major only for future reuse as a publication verifier, not as a defect in the retained labels. Add a separately versioned verifier that checks process outcomes, recognized state values, explicit terminal paths and applicable stage sufficiency on every outcome. Preserve raw bridge values and attach verification errors instead of relabeling history. Generate future discriminant claims from those checks. Never require every rejected record to have business stages, every business stage to be valid, or an expected acceptance total.

## S2 — CONFIRMED, minor and limited to standalone verification

**Evidence.** `family-run/summarize.py:5,13` trusts the current request list; lines 57 and 60 hardcode 60 for closure count and requested count. It neither compares observed/request cardinality to that receipt nor independently binds the current list to frozen membership/order/allocation.

My independent `requests_59_stream_59_stale_closure_60` fixture removes the final requested row and its stream line while leaving the recorded closure-count assertion at 60. Result: `status: complete`, `requested: 60`, `observed: 59`, `unobserved: 0`, zero contract errors. The unused final per-record file can remain on disk; the summarizer never enumerates it. This demonstrates trust in the request-list boundary, not an actual lost historical record. Reordering requests with matching stream order also returns complete. Changing a request's family and its matching input-receipt row returns complete without consulting the freeze.

**Counterevidence and negative controls.** Removing only the request row while retaining the original 60-line stdout fails the existing stream-binding check. Changing the closure count honestly to 59 also fails completeness. Launch `run.py:9–10` checks 60, while `prepare_run.py:28–32,38–58` binds 59 frozen candidates plus the baseline and their order/families. The actual retained request JSON, TSV, execution declaration, freeze order, receipt bindings and closure count all agree at 60. The original manifest rejects the changed synthetic request files; this is explicitly checked, as is its rejection of S1's altered raw record. None of these probes defeats immutable hashing or establishes historical incompleteness.

**Proportionate correction.** Keep minor. A new verifier should bind exact IDs, ordinals/order, whole-family assignments, input digests, request JSON/TSV commitments, returned raw records/stream and process/closure counts to one frozen declaration. Compute counts from verified membership and compare every receipt assertion with those counts. Changing the literal 60 to `len(requests)` alone would merely make the forged 59-row fixture internally consistent; it would not restore the original contract. Do not rerun the oracle or change historical counts/labels to address this.

## S3 — DOWNGRADE to a publication cross-reference gap

**Counterevidence disproving the broad absence claim.** The direct attribution paragraphs do omit the upstream URL and extraction details, but the manifested bundle already contains the exact locator in `family-run/snapshots/artifacts.lock.json:695–705`. Its `sinabis.parquet` record has:

- URL: `https://huggingface.co/datasets/sinabis-group/sinabis-10k-synthetic-zugferd-xrechnung/resolve/8d9ab205ab74f360fac344a50fcd54c35d0cd9ba/data/train-00000-of-00001.parquet`.
- Revision: `8d9ab205ab74f360fac344a50fcd54c35d0cd9ba`.
- SHA-256: `f45674ce80b477a859c684c68e423f8bb52ad2c1647c97a9e29050f6519dee13`.

The adjacent pinned README URL and digest also exist. I selected only these named source records from the copied lock; no dataset, original baseline manifest, upstream page or license content was opened. The Parquet locator's revision/digest exactly match `discriminant/provenance.json:45–54` and the direct attribution. Thus “no upstream locator in the review bundle” is a wrong-door conclusion. No acquisition research is needed to recover it.

**Bounded external evidence closes the extraction question locally.** Only the authorized `corpus-discovery.md:108–142` and `seed_smoke.py:63–108` were read. Discovery names `data/train-00000-of-00001.parquet`, the original XML source path, source index 0, 11,214-byte UTF-8 seed and seed hash. The extraction code at lines 99–108 requests provenance/XML columns and filters `xml_file` equal to `zugferd/b2b_service_maintenance/0000/0000000_SRV-2025-47731.xml`; it requires exactly one row, takes the **`xml` string field encoded as UTF-8**, and checks `sha256_xml`, byte hash/length, theme and cached byte equality. The selector is the logical XML path in the `xml_file` column, not an XML file member extracted from the Parquet file. No new dataset read is needed to document that rule.

**Remaining boundary and controls.** Neither external file is a member of the 799-file manifest. A publisher shipping only a reduced set of source paragraphs would still omit the extraction rule, and removing the copied lock would also remove the currently available locator. The current direct source record is therefore not independently sufficient by itself. My checks verify that the lock URL contains the pinned revision/artifact path and its digest/revision match the seed provenance; they also verify that the extraction code requires a unique row and byte checks. This is documentary verification, not a new extraction or independent verification of upstream bytes. An XML-path-as-archive-member reading is contradicted by the code's explicit `xml_file` predicate and `xml` field conversion.

**Proportionate correction.** Downgrade the Hold to an assembly/cross-reference note: retain a small new source record linking the already-present pinned locator to the already-existing selector and byte conversion/checks. Include that record in any future publication manifest if the copied lock/external discovery script is omitted. Preserve creator/title, revision, seed hash/length, attribution/license link and modification lineage. Do not require new acquisition, corpus scanning, seed reselection, relabeling or legal conclusions. The remaining self-contained-publication requirement is real; unavailable or missing local provenance is not established.

## Retained-data validity and unchanged limits

Independent `retained-checks.py` results are in `retained-controls.json`. The 60 original family observations equal their exact raw bridge records and request rows; all 60 finding occurrences equal the independently enumerated original per-bucket values/order, including duplicates, nulls and BASE/effective severity. Seven occurrences have differing severity levels. The original 28 accepted/32 rejected total is a reported observation, never a gate asserted by these probes.

The exact pinned validator JAR and full original configuration archive were checked by digest; all 97 extracted configuration members equal that archive. All 184 family and 103 corrected-discriminant runtime-closure members are within the manifested DEVELOPMENT scope and match their recorded digests. Original request/declaration/TSV/closure counts and hashes agree. Family subprocess streams match their recorded hashes and exit codes; there is one 60-request evaluation process. These are consistency checks of retained local evidence, not an independent execution or evaluator attestation.

All four initial discriminant guard failures remain explicit process failures with empty stdout, `JDK home mismatch` stderr and exit 1; all four corrected raw observations remain separate with matching stdout and exit 0. No failure was converted into an invoice outcome. The immutable prelabel candidate ledger still has DEVELOPMENT assignments, unknown expectations, null observations and `oracle_evaluated: false`; frozen order/family allocations match the actual request rows. Family allocations remain F01=5, F02=14, F03=11, F04=6, F05=5, F06=18. These are technique allocations under one ancestor, not independent-source or execution-coverage evidence.

No source-count, absence-of-finding, stage-presence or synthetic probe result demonstrates actual assertion/site hits. No held-out families were created or accessed. Native correctness, structural equivalence, runtime/source coverage, resource/security outcomes, transitive attestation, release eligibility and timing flags remain unproven. Any correction belongs in new verifier/provenance artifact versions; every original script, raw attempt, report, label and commitment must remain intact.
