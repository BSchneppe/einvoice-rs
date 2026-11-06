# BSC10 frozen DEVELOPMENT evaluation observations

Batch status: **complete**. Requested 60; observed 60; unobserved 0. The baseline is request 0; requests 1–59 preserve the frozen ledger order. The earlier four CII-SR-282 discriminants are a separate batch and are excluded.

All prelabel expectations remain unknown in the unchanged copied ledger. These observations are discovery evidence for later independent artifact review/publication; they do not establish full conformance, evaluator attestation, native behavior, source-site coverage, resources, or performance.

Acceptance counts: `{"accepted": 28, "rejected": 32}`. Processing counts: `{"completed": 60}`. Consumed closure verified before/after: `True`. Process status: `exited`; exit: `0`. Contract errors: 0; execution failures: 0.

## Actual observation matrix

Missing stages are `not_evaluated`, not validity results. `null` is the original nullable value. Codes are listed occurrence by occurrence in original per-bucket order; the full exact messages, BASE severity, effective severity, paths, string coordinates, and duplicate occurrences remain in raw bridge JSON and observations.json.

| # | Request | Processing | XML | XSD | SCH.1 | SCH.2 | Scenario / target match | Reference recommendation | Acceptance | Business findings [BASE→effective] | Structural findings [BASE→effective] |
|---:|---|---|---|---|---|---|---|---|---|---|---|
| 0 | baseline | completed | valid | valid | valid | valid | EN16931 XRechnung (CII) / True | acceptable | accepted | none | none |
| 1 | F01-prefix-b | completed | valid | valid | valid | valid | EN16931 XRechnung (CII) / True | acceptable | accepted | none | none |
| 2 | F01-default-ram | completed | valid | valid | valid | valid | EN16931 XRechnung (CII) / True | acceptable | accepted | none | none |
| 3 | F01-empty-delivery-ram | completed | valid | valid | valid | valid | EN16931 XRechnung (CII) / True | acceptable | accepted | BR-DE-TMP-32 [information→information] | none |
| 4 | F01-empty-delivery-b | completed | valid | valid | valid | invalid | EN16931 XRechnung (CII) / True | reject | rejected | PEPPOL-EN16931-R008 [error→error], BR-DE-TMP-32 [information→information] | none |
| 5 | F01-local-shadow | completed | valid | invalid | not_evaluated | not_evaluated | EN16931 XRechnung (CII) / True | reject | rejected | none | cvc-complex-type.2.4.a [error→error] |
| 6 | F02-buyer-missing | completed | valid | valid | valid | invalid | EN16931 XRechnung (CII) / True | reject | rejected | BR-DE-15 [error→error] | none |
| 7 | F02-buyer-empty | completed | valid | valid | valid | invalid | EN16931 XRechnung (CII) / True | reject | rejected | PEPPOL-EN16931-R008 [error→error], BR-DE-15 [error→error] | none |
| 8 | F02-buyer-whitespace | completed | valid | valid | valid | invalid | EN16931 XRechnung (CII) / True | reject | rejected | PEPPOL-EN16931-R008 [error→error], BR-DE-15 [error→error] | none |
| 9 | F02-buyer-duplicate | completed | valid | invalid | not_evaluated | not_evaluated | EN16931 XRechnung (CII) / True | reject | rejected | none | cvc-complex-type.2.4.a [error→error] |
| 10 | F02-careof | completed | valid | valid | invalid | valid | EN16931 XRechnung (CII) / True | reject | rejected | CII-DT-093 [error→error] | none |
| 11 | F02-issue-date-duplicate | completed | valid | invalid | not_evaluated | not_evaluated | EN16931 XRechnung (CII) / True | reject | rejected | none | cvc-complex-type.2.4.a [error→error] |
| 12 | F02-zero-lines | completed | valid | valid | invalid | valid | EN16931 XRechnung (CII) / True | reject | rejected | BR-16 [error→error], BR-S-08 [error→error], BR-CO-10 [error→error] | none |
| 13 | F02-terms-duplicate | completed | valid | valid | invalid | valid | EN16931 XRechnung (CII) / True | reject | rejected | CII-SR-452 [warning→error], CII-SR-453 [warning→error] | none |
| 14 | F02-tax-zero | completed | valid | valid | invalid | valid | EN16931 XRechnung (CII) / True | reject | rejected | BR-CO-04 [error→error], BR-S-08 [error→error], CII-SR-454 [warning→error] | none |
| 15 | F02-tax-two | completed | valid | valid | invalid | valid | EN16931 XRechnung (CII) / True | reject | rejected | CII-SR-454 [warning→error] | none |
| 16 | F02-name-missing-2 | completed | valid | valid | invalid | valid | EN16931 XRechnung (CII) / True | reject | rejected | BR-25 [error→error] | none |
| 17 | F02-name-missing-3 | completed | valid | valid | invalid | valid | EN16931 XRechnung (CII) / True | reject | rejected | BR-25 [error→error] | none |
| 18 | F02-name-missing-2-3 | completed | valid | valid | invalid | valid | EN16931 XRechnung (CII) / True | reject | rejected | BR-25 [error→error], BR-25 [error→error] | none |
| 19 | F02-name-missing-2-3-shifted | completed | valid | valid | invalid | valid | EN16931 XRechnung (CII) / True | reject | rejected | BR-25 [error→error], BR-25 [error→error] | none |
| 20 | F03-plus | completed | valid | valid | valid | valid | EN16931 XRechnung (CII) / True | acceptable | accepted | none | none |
| 21 | F03-leading-zero | completed | valid | valid | valid | valid | EN16931 XRechnung (CII) / True | acceptable | accepted | none | none |
| 22 | F03-trailing-zero | completed | valid | valid | invalid | valid | EN16931 XRechnung (CII) / True | reject | rejected | BR-DEC-23 [error→error] | none |
| 23 | F03-exponent | completed | valid | invalid | not_evaluated | not_evaluated | EN16931 XRechnung (CII) / True | reject | rejected | none | cvc-datatype-valid.1.2.1 [error→error], cvc-complex-type.2.2 [error→error] |
| 24 | F03-magnitude | completed | valid | valid | invalid | invalid | EN16931 XRechnung (CII) / True | reject | rejected | BR-S-08 [error→error], BR-CO-10 [error→error], PEPPOL-EN16931-R120 [warning→warning] | none |
| 25 | F03-negative-price | completed | valid | valid | invalid | invalid | EN16931 XRechnung (CII) / True | reject | rejected | BR-27 [error→error], PEPPOL-EN16931-R120 [warning→warning] | none |
| 26 | F03-round-below | completed | valid | valid | invalid | valid | EN16931 XRechnung (CII) / True | reject | rejected | BR-DEC-23 [error→error] | none |
| 27 | F03-round-tie | completed | valid | valid | invalid | valid | EN16931 XRechnung (CII) / True | reject | rejected | BR-DEC-23 [error→error], BR-CO-10 [error→error] | none |
| 28 | F03-round-above | completed | valid | valid | invalid | valid | EN16931 XRechnung (CII) / True | reject | rejected | BR-DEC-23 [error→error] | none |
| 29 | F03-round-tie-lower-header | completed | valid | valid | invalid | valid | EN16931 XRechnung (CII) / True | reject | rejected | BR-DEC-23 [error→error], BR-S-08 [error→error] | none |
| 30 | F03-precision-lines-2-3 | completed | valid | valid | invalid | valid | EN16931 XRechnung (CII) / True | reject | rejected | BR-DEC-23 [error→error], BR-DEC-23 [error→error] | none |
| 31 | F04-leap-day | completed | valid | valid | valid | valid | EN16931 XRechnung (CII) / True | acceptable | accepted | none | none |
| 32 | F04-nonleap-day | completed | valid | valid | valid | valid | EN16931 XRechnung (CII) / True | acceptable | accepted | none | none |
| 33 | F04-month-13 | completed | valid | valid | invalid | valid | EN16931 XRechnung (CII) / True | reject | rejected | CII-DT-097 [error→error] | none |
| 34 | F04-day-zero | completed | valid | valid | invalid | valid | EN16931 XRechnung (CII) / True | reject | rejected | CII-DT-097 [error→error] | none |
| 35 | F04-hyphenated | completed | valid | valid | invalid | invalid | EN16931 XRechnung (CII) / True | reject | rejected | CII-DT-097 [error→error], BR-TMP-7 [warning→warning] | none |
| 36 | F04-format-203 | completed | valid | valid | invalid | invalid | EN16931 XRechnung (CII) / True | reject | rejected | BR-03 [error→error], BR-TMP-7 [warning→warning] | none |
| 37 | F05-currency-case | completed | valid | valid | invalid | invalid | EN16931 XRechnung (CII) / True | reject | rejected | BR-CO-15 [error→error], BR-CL-04 [error→error], PEPPOL-EN16931-R054 [error→error] | none |
| 38 | F05-currency-space | completed | valid | valid | invalid | invalid | EN16931 XRechnung (CII) / True | reject | rejected | BR-CO-15 [error→error], PEPPOL-EN16931-R054 [error→error] | none |
| 39 | F05-currency-outside | completed | valid | valid | invalid | invalid | EN16931 XRechnung (CII) / True | reject | rejected | BR-CO-15 [error→error], BR-CL-04 [error→error], PEPPOL-EN16931-R054 [error→error] | none |
| 40 | F05-unit-case | completed | valid | valid | invalid | valid | EN16931 XRechnung (CII) / True | acceptable | accepted | BR-CL-23 [error→warning] | none |
| 41 | F05-unit-lines-2-3 | completed | valid | valid | invalid | valid | EN16931 XRechnung (CII) / True | acceptable | accepted | BR-CL-23 [error→warning], BR-CL-23 [error→warning] | none |
| 42 | F06-xml10-crlf-literal | completed | valid | valid | valid | valid | EN16931 XRechnung (CII) / True | acceptable | accepted | none | none |
| 43 | F06-xml10-crlf-reference | completed | valid | valid | valid | valid | EN16931 XRechnung (CII) / True | acceptable | accepted | none | none |
| 44 | F06-xml10-cr-literal | completed | valid | valid | valid | valid | EN16931 XRechnung (CII) / True | acceptable | accepted | none | none |
| 45 | F06-xml10-cr-reference | completed | valid | valid | valid | valid | EN16931 XRechnung (CII) / True | acceptable | accepted | none | none |
| 46 | F06-xml10-nel-literal | completed | valid | valid | valid | valid | EN16931 XRechnung (CII) / True | acceptable | accepted | none | none |
| 47 | F06-xml10-nel-reference | completed | valid | valid | valid | valid | EN16931 XRechnung (CII) / True | acceptable | accepted | none | none |
| 48 | F06-xml11-nel-literal | completed | valid | valid | valid | valid | EN16931 XRechnung (CII) / True | acceptable | accepted | none | none |
| 49 | F06-xml11-nel-reference | completed | valid | valid | valid | valid | EN16931 XRechnung (CII) / True | acceptable | accepted | none | none |
| 50 | F06-xml10-ls-literal | completed | valid | valid | valid | valid | EN16931 XRechnung (CII) / True | acceptable | accepted | none | none |
| 51 | F06-xml10-ls-reference | completed | valid | valid | valid | valid | EN16931 XRechnung (CII) / True | acceptable | accepted | none | none |
| 52 | F06-xml11-ls-literal | completed | valid | valid | valid | valid | EN16931 XRechnung (CII) / True | acceptable | accepted | none | none |
| 53 | F06-xml11-ls-reference | completed | valid | valid | valid | valid | EN16931 XRechnung (CII) / True | acceptable | accepted | none | none |
| 54 | F06-attr-tab-literal | completed | valid | valid | valid | valid | EN16931 XRechnung (CII) / True | acceptable | accepted | none | none |
| 55 | F06-attr-tab-reference | completed | valid | valid | valid | valid | EN16931 XRechnung (CII) / True | acceptable | accepted | none | none |
| 56 | F06-xml11-attr-nel-literal | completed | valid | valid | valid | valid | EN16931 XRechnung (CII) / True | acceptable | accepted | none | none |
| 57 | F06-xml11-attr-nel-reference | completed | valid | valid | valid | valid | EN16931 XRechnung (CII) / True | acceptable | accepted | none | none |
| 58 | F06-utf8-bom | completed | valid | valid | valid | valid | EN16931 XRechnung (CII) / True | acceptable | accepted | none | none |
| 59 | F06-utf16-bom | completed | valid | valid | valid | valid | EN16931 XRechnung (CII) / True | acceptable | accepted | none | none |

## Provenance and execution closure

- Freeze SHA-256: `e2d8275974c59a96a6dff1b000a684d75abd2dc61f3e1331e1d593c1a228c2ae`; proposal SHA-256: `60ae574a2908a3f8b40e79bf1dde1b37d116cd7809814623a4a72408f2baf7eb`.
- The freeze binds the verified review-bundle hash and all199 members. Full59 byte replay, original anchors/coordinates, ordered sequential cardinalities, and whole-family DEVELOPMENT allocation passed before execution; see preflight-verification.json.
- Exact Python: `/Users/bene/.t3/worktrees/rust-einvoice/benediktschneppe-bsc-5--45552c76f3d0fedb/tools/oracle/.cache/test-venv/bin/python`; version `3.12.6`; resolved executable `/Library/Frameworks/Python.framework/Versions/3.12/bin/python3.12`.
- Exact Corretto requested home: `/Library/Java/JavaVirtualMachines/corretto-27.0.1.0.11/Contents/Home`. Java/javac realpaths, binary digests, release digest and raw version process outputs are retained. Loaded class resource hashes/code sources and the running Java executable realpath are in raw/closure-before.json.
- Original pinned validator JAR and original full97-member configuration were copied from verified read-only cache archive snapshots. Every config member was checked against the copied archive actually consumed, before and after. STRICT_LOCAL resolution was used; no alternative XSLT was substituted.
- Oracle.java was copied unchanged and hash checked. The custom driver calls only Oracle.evaluate, once per request in one Java process after one configuration load. Oracle.main (timing/benchmark entry point) was never invoked. No warmup, benchmark, timing measurement, fake adapter, package install, download, or shared-cache write occurred.
- Runtime closure binds compiled classes, JAR/config, original byte input copies, requested manifest, freeze/ledger snapshots, driver and scripts. Java checks its actual loaded paths, real JDK home/executable, requested byte hashes, and closure before and after. Read-only input copies were passed as original byte arrays without parsing/reserialization by the harness.
- All39 frozen evidence files were hashed as opaque bytes before and after; labels were not deserialized. See frozen-before.json and frozen-after.json. Source pins, original labels/splits/workload and source-inventory review files were untouched.
- execution-declaration.json and declared-commands.txt were written before the first Java/javac subprocess; the compiled binding and exact evaluation command were written before evaluation. All process argv, raw stdout/stderr, exit codes and timeout/error statuses are retained under processes/. There was one attempt and no rerun.
- Baseline SHA-256: `881f7bc3ff0513a6627a981301af2c0ec9bcdce2c9d1628d7858e581f4d8e91c`. Sinabis GmbH (2026), *Sinabis Green Energy – 10k GoBD-compliant German E-Invoices*, CC-BY-4.0 (<https://creativecommons.org/licenses/by/4.0/>). Parent revision `8d9ab205ab74f360fac344a50fcd54c35d0cd9ba`, source artifact SHA-256 `f45674ce80b477a859c684c68e423f8bb52ad2c1647c97a9e29050f6519dee13`, member `zugferd/b2b_service_maintenance/0000/0000000_SRV-2025-47731.xml`. Baseline unchanged; exact mutation lineage, recipes, original locations and predecessor revisions are retained separately in snapshots/proposed-ledger.json and snapshots/revisions.jsonl.

## Outcome contrasts and semantic gaps

No verdict was predicted or asserted. Accepted cases with negative design intent and rejected cases with positive design intent remain evidence; no recipe, allocation, shape, value, revision, input, or request order was changed after observation. Absence of a finding provides no source-hit or coverage inference.

- `F01-empty-delivery-ram`: accepted; positive intents=["contrast same expanded name at the source lexical empty-delivery exception"]; negative intents=["remove original delivery event; retain all collateral findings, without assuming an exception determines full validity"]. This is an intent/outcome contrast, not a failed expected-result check.
- `F01-empty-delivery-b`: rejected; positive intents=["contrast same expanded name at the source lexical empty-delivery exception"]; negative intents=["remove original delivery event; retain all collateral findings, without assuming an exception determines full validity"]. This is an intent/outcome contrast, not a failed expected-result check.
- `F02-name-missing-2-3-shifted`: rejected; positive intents=["retain same two affected contexts while shifting original source locations"]; negative intents=["same missing-name mutations as sibling case; one family, no new independent technique"]. This is an intent/outcome contrast, not a failed expected-result check.
- `F03-trailing-zero`: rejected; positive intents=["preserve numeric amount using alternative decimal spelling"]; negative intents=["cross source fractional-digit boundary despite numeric equality"]. This is an intent/outcome contrast, not a failed expected-result check.
- `F03-round-below`: rejected; positive intents=["probe exact below/tie/above sum neighborhood with predeclared matching-intent header"]; negative intents=["three fractional digits deliberately retained; collateral checks remain evidence"]. This is an intent/outcome contrast, not a failed expected-result check.
- `F03-round-tie`: rejected; positive intents=["probe exact below/tie/above sum neighborhood with predeclared matching-intent header"]; negative intents=["three fractional digits deliberately retained; collateral checks remain evidence"]. This is an intent/outcome contrast, not a failed expected-result check.
- `F03-round-above`: rejected; positive intents=["probe exact below/tie/above sum neighborhood with predeclared matching-intent header"]; negative intents=["three fractional digits deliberately retained; collateral checks remain evidence"]. This is an intent/outcome contrast, not a failed expected-result check.
- `F03-precision-lines-2-3`: rejected; positive intents=["preserve numeric totals in two sequential line contexts"]; negative intents=["cross source lexical precision boundary in both contexts"]. This is an intent/outcome contrast, not a failed expected-result check.
- `F04-nonleap-day`: accepted; positive intents=[]; negative intents=["contrast calendar, string-shape and exact format-selector constraints"]. This is an intent/outcome contrast, not a failed expected-result check.
- `F05-currency-space`: rejected; positive intents=["explore source normalize-space behavior"]; negative intents=["contrast pinned code-list lexical boundary; tax currency attribute remains EUR"]. This is an intent/outcome contrast, not a failed expected-result check.
- `F05-unit-case`: accepted; positive intents=[]; negative intents=["case boundary in second line unit code"]. This is an intent/outcome contrast, not a failed expected-result check.
- `F05-unit-lines-2-3`: accepted; positive intents=[]; negative intents=["out-of-list intent in two actual sequential unit-code contexts; retain separate diagnostics"]. This is an intent/outcome contrast, not a failed expected-result check.

- Parser/XML, XSD, Schematron stages, scenario matching, processing status and reference recommendation are distinct fields. Processing exceptions or missing output are never invoice-rejection labels. Observed bridge records retain all collateral diagnostics, even when unrelated to the mutation intent.
- All six technique families share one public ancestor/source shape. The57 mutations, two encoding controls and separate identity baseline provide no independent-source or held-out claim. No held-out inputs, labels or family names were inspected.
- Accepted XML1.1/source-form/encoding variants can establish only this configured evaluator’s submitted-byte result. Diagnostic equivalence or absent findings does not reveal the intermediate normalized tree, parser internals, XPath node selection, or source-site execution.
- The copied bridge retains ordered messages within its business and structural arrays and stage validity in insertion order. It does not emit the full KoSIT report DOM; this batch makes no stronger cross-bucket event-order or full-report claim. All bridge records are preserved exactly, including nullable coordinates and strings.
- No independent artifact review/publication was performed here. Frozen-byte integrity and local receipts bind this run; they are not a cryptographic evaluator attestation.

## Files

- `observations.json`: all60 original requested rows, actual records or explicit unobserved/error statuses, per-input receipts and summary.
- `raw/*.bridge.json`: unchanged individual raw bridge records; `processes/evaluate60.stdout` is the original combined process stream. `finding-occurrences.json` preserves every per-bucket occurrence without deduplication.
- `execution-declaration.json`, `compiled-execution-binding.json`, `runtime-closure.tsv`, `preflight-verification.json`, `preparation.json`: execution and provenance commitments.
- `processes/*`: every attempted subprocess command, stdout, stderr and metadata. `run-metadata.json` retains infrastructure failures. No silent overwrite/rerun.
- `hash-manifest.json`: hashes of every retained file except itself; verify against files before review.
