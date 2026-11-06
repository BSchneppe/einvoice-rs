# Pinned XRechnung development oracle

BSC-5 supplies reference infrastructure for **XRechnung 3.0.2 CII D16B**, KoSIT
configuration **2026-08-31**, Validator **1.6.3**, CEN **1.3.16**, XRechnung
Schematron **2.6.0** and Saxon HE **12.9** (inside the validator JAR). It does not
implement native validation, approve Xee, or demonstrate the 5× release target.
Nothing here is a runtime dependency of the library. The root Cargo workspace,
lockfile and BSC-6 facade are untouched.

Planning authority: [BSC-5](https://linear.app/bschneppe/issue/BSC-5/pin-xrechnung-artifacts-and-establish-the-reference-oracle),
[project](https://linear.app/bschneppe/project/rust-e-invoice-library-0bad68cfeaeb),
[discovery](https://linear.app/bschneppe/document/discovery-evidence-and-pinned-references-462b18ff0ee2).

## Reproduce

Run from the repository root, with Python 3.12 and Corretto 27.0.1 for the
frozen outcomes (also used in CI), and Rust for the optional native probes.
Other JDK versions can change JAXP limits and diagnostics; record/review those
differences rather than silently replacing the frozen reference:

```sh
python3 -m venv tools/oracle/.cache/venv
. tools/oracle/.cache/venv/bin/activate
pip install -r tools/oracle/requirements-test.txt
python3 tools/oracle/oracle.py fetch
python3 tools/oracle/oracle.py corpus
python3 -m unittest discover -s tools/oracle/tests -v
python3 tools/oracle/oracle.py label --output tools/oracle/results/initial.jsonl
python3 tools/oracle/oracle.py scoped --output tools/oracle/results/scoped.jsonl
python3 tools/oracle/oracle.py label --manifest tools/oracle/corpus/sinabis.jsonl --output tools/oracle/results/sinabis.jsonl
```

`fetch` is the only command that downloads artifacts. It checks SHA-256 before
renaming a partial download; an existing corrupt file causes failure rather than
silent replacement. `prepare` verifies the archives, every imported member
(including all four CII XSDs), and recompiles the small Java bridge. It preserves
the entire upstream configuration, including nine CII severity overrides and
acceptance policy. Cached `.cache` data is disposable and ignored by Git.
The corpus command verifies XML against the upstream per-row manifest, writes
original UTF-8 bytes, and regenerates the checked-in inventory and split. Review
any resulting inventory diff deliberately before accepting it.

CLI exit **0** means processing completed, not that invoices are accepted.
Exit **2** means infrastructure/processing failed. The initial corpus deliberately
includes malformed/deep XML, so its label command exits 2 **after writing the
observed outcomes**. A timeout produces indeterminate records, never stale success.
Input/artifact integrity failures stop before evaluation and emit JSON on stderr.
Consume exit status, completion metadata and document records together. The tests
compare fresh initial/scoped outcomes exactly with the frozen records and verify
full-dataset schema, unique ID coverage, input hashes and bridge/artifact provenance.

For a fresh cache the downloads total about 23 MB. Only corpus import needs
PyArrow; the regression suite also uses pinned jsonschema to validate frozen outcomes. The Sinabis import downloads the **6 MB standalone-XML Parquet**, not the
roughly 400 MB hybrid PDF archive. The dataset is pinned to revision
`8d9ab205ab74f360fac344a50fcd54c35d0cd9ba`; all 10,000 XML file hashes and
source-relative paths are in `corpus/sinabis.jsonl`.

## Corpus and provenance

`artifacts.lock.json` records source URLs, versions, archive/file SHA-256 and member
SHA-256. `notices/` preserves KoSIT validator/configuration, bundled JAR notice,
CEN EUPL-1.2, XRechnung Apache-2.0, and the Sinabis dataset card (Sinabis Group,
CC-BY-4.0). Schema copyright headers remain in the byte-identical extracted XSDs.
The repository's MIT license does not relicense upstream inputs. Generated
packages must carry the applicable notices and attribution with their own manifest.

`corpus/initial.jsonl` includes one representative from every synthetic theme,
the original twelve discovery invoices, fifteen complete CEN examples, both
upstream CII rule test containers, and deterministic local mutations. Each entry
records scope, source, SHA-256, scenario, family, and split; mutations also record
the parent's SHA-256. The standalone files are reconstructed in `.cache` instead
of duplicating 100+ MB of XML in Git.

Complete official examples are **not assumed XRechnung-valid**. Some match the
broader EN16931 scenario; others match none. `scoped` runs the two official
`test/cii` containers' nine fragments through the pinned KoSIT CEN stylesheet,
retains all collateral findings, and compares only each fragment's declared
success/error assertions. Its `expectations_met` is never global validity:
`schema=not_evaluated`, `acceptance=not_applicable`. Original test containers and
scope declarations are retained without rewriting them into invoices.

`corpus/split.json` assigns **whole scenarios** to development/held-out sets:
service, hardware and feed-in are development; municipal EPC, reverse charge and
credit-note correction are held out. Whole mutation families and official rule
families also stay in one split. Mutation seeds are public, so this initial split
separates families, not base-document bytes. No shuffled adjacent-row split or
random seed can move a family. Held-out observations here establish the evaluator;
future optimization must not tune to their labels. Broader adversarial coverage is
BSC-10's responsibility.

## Outcome contract and BSC-6 mapping

`results/*.jsonl` is an independent, versioned development schema, documented in
`outcome.schema.json`. It intentionally imports no library API.

| Oracle field | Meaning / possible BSC-6 mapping |
| --- | --- |
| `sha256`, `id`, `scope` | Exact input identity and evaluation scope; attach as provenance |
| `processing`, `processing_errors` | Engine/resource/incomplete-evaluation status, separate from invalidity |
| `schema` | `valid`, `invalid`, or `not_evaluated`; never infer it from deserialization |
| `stages` | Actual KoSIT XML/XSD/Schematron steps, including schema-driven short-circuit |
| `findings` | Business ID, original and effective severity, XPath location, message and stage |
| `structural_findings` | XML/XSD diagnostics including line/column when available |
| `reference_acceptance` | KoSIT's own recommendation for the scenario it matched |
| `acceptance` | Accepted only after successful, complete evaluation of the exact CII target |
| `target_matched`, `scenario` | Prevent an accepted EN16931-only or fallback result from certifying XRechnung |

A raw rule error may be an effective warning and still be accepted (BR-CL-23).
A raw warning may become a rejecting error (CII-SR-452). Do not recompute acceptance
by counting raw findings. Schema rejection may skip business rules. An engine
failure yields `indeterminate`, never valid. `business` indicates whether both
pinned layers ran; individual stage results retain partial evaluation evidence.

The Java bridge uses KoSIT's public `checkInput` API, original configuration and
report policy. The namespace-aware report walker retains all diagnostics. JSON
has no timestamps or machine-dependent input paths; timings/engine provenance
live in adjacent metadata. Java catches per-input exceptions. Process death,
timeout and startup failure are handled by the CLI without claiming success.

## Structural and XPath counterexamples

```sh
# Fetch native dev dependencies once explicitly; execution below is offline.
cargo fetch --locked --manifest-path tools/oracle/probes/xpath/Cargo.toml
cargo fetch --locked --manifest-path tools/oracle/probes/xsd/Cargo.toml
python3 tools/oracle/probes.py
cargo run --locked --offline --manifest-path tools/oracle/probes/xsd/Cargo.toml -- tools/oracle/.cache
```

Both probes have isolated `[workspace]` manifests and committed Cargo.lock files.
They cannot add dependencies to the shipping workspace. The XSD probe preserves
the discovery adapter's four-file import allowlist, no networking, both driver
and diagnostic checks, malformed/namespace/depth cases, and xsd-schema 0.3.0.
The structural mutation expectations were independently reproduced by KoSIT.

The 16 XPath probes cover decimal rounding, sequence comparisons, quantifiers,
namespace navigation, XML regex, large-integer modulo, error behavior, inline
helpers and IBAN arithmetic. `probes.py` runs pinned Saxon and locked Xee and
requires the two known decimal divergences to remain visible. A changed result
fails the probe pending investigation; this is not a conformance-passing test of
Xee. `results/xpath.json` records both sides. The 30-digit decimal fails in Xee;
29 fractional digits silently round. Full Schematron execution is not implemented.

## Warm benchmark contract

```sh
# On Linux reserve the same isolated CPU for both engines (example CPU 2).
taskset -c 2 python3 tools/oracle/oracle.py benchmark \
  --machine 'dedicated-host/model/governor' --cpu-allocation 'isolated CPU 2' \
  --output tools/oracle/.cache/benchmark.json
```

Use the same machine, CPU affinity/quota and memory allowance for a future Rust
**release** build. The harness runs one worker, JVM `ActiveProcessorCount=1`,
SerialGC and fixed 256 MiB initial / 1 GiB maximum heap. JVM processor count is
**not** an OS CPU quota; OS allocation must be imposed externally (e.g. taskset
or a container cpuset). Mac smoke measurements without affinity are explicitly
not release measurements. Do not run competing builds/tests during real timing.

`workload.json` fixes equal weight per theme, official-complete group and mutation
family **before measurement**, not per adjacent dataset row. Every declared group
must appear. Each of five default runs uses a deterministic shuffled traversal,
20 complete warmup passes and ten measured passes. Timed work includes a fresh
input, XML parsing, original schema and business rules, configured acceptance,
full diagnostic extraction and JSON normalization. Input bytes and compiled
artifacts are preloaded. File I/O and stdout writes are outside per-document
timing. Outcomes are consumed and checked for stability on every repetition.

The no-HTML report imports the original report stylesheet and overrides only its
supported named `html:html` template. The harness first demands **exact equality
of all outcome records** against the untouched HTML-producing configuration.
Both scenario reports and the unmatched/malformed fallback report are wrapped.
The CLI verifies zero XHTML roots in every primary report. No rule, scenario match,
severity override or acceptance expression is removed.
Then it times both modes separately. Full HTML cost is the independently measured
full-mode distribution, not an unreliable subtraction of unrelated samples.
Initialization (configuration compilation), input file I/O, whole-process wall
time, heap-pool peaks, per-group p50/p95/mean, and raw latency samples are recorded
separately. Whole-process wall includes warmup, output, all runs and shutdown;
it is not a startup metric. JVM launch overhead and OS RSS need an external
process profiler for a release experiment; heap-pool peaks are not RSS.

The predeclared aggregate is `1 / sum(weight * mean_latency_seconds)`; retain every
run and group. No JVM-per-invoice baseline, reduced rules, validity-only fast path,
or cherry-picked invoice is permitted. Schema failures follow KoSIT's actual
short-circuit. Expected malformed/resource-negative inputs retain failed processing
labels in `observed_outcomes`; timing them cannot turn them into valid invoices.

This tool always marks `release_gate_eligible=false`: it provides a reference
measurement, not proof of equivalent native coverage/diagnostics or ≥5× speedup.
A native adapter must match the frozen outcomes and stage/short-circuit policy,
use equivalent diagnostic work, and pair measurements under the same allocation
before BSC-11 can claim the gate. Do not optimize by weakening the evaluator.
