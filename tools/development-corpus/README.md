# BSC10 DEVELOPMENT evidence

This is an additive, bounded publication of existing DEVELOPMENT observations.
It does not close BSC10. There is one public ancestor, 64 completed request
positions (60 family requests and four corrected discriminants), and 63 distinct
evaluated byte streams. All three prelabel draft versions survive: 67 distinct
historical input streams are retained. Superseded drafts receive no new labels.

`selection.json` accounts for all 799 original identities: 585 retained identities
resolve directly to 402 ordinary files (4,372,919 bytes); 203 dependencies/members
are external, nine class files have compile recipes, and two planning handoffs
are explicitly excluded. `history/artifact-review/manifest.json` retains the exact
original commitment and reviewed head `e89b2fd2`; that historical fact is unrelated
to the branch on which this module is published. Raw commands, streams, all eight
discriminant attempts, ledgers, receipts, scripts and reports remain unchanged.
Historical scripts are never imported or executed by the public tools.

Run the stdlib-only offline check from any directory with Python 3.12:

```sh
python3.12 -B tools/development-corpus/verify.py \
  --manifest tools/development-corpus/publication.json --out /fresh/path/report.json
python3.12 -B -m unittest discover -s tools/development-corpus/tests -v
```

The report path must not exist and must be outside the bundle. Optional
`--dependencies /read-only/directory` checks only the named supplied pinned
validator/config/CEN/XR archives and selected members; it never acquires anything.
No Parquet read, Java execution, XML business validation, lock refresh or repair
occurs in offline verification. Missing external bytes and not-rebuilt classes
remain explicit; exit zero verifies retained evidence, not all 799 payloads.

The separate reproduction runner requires the already recorded exact Python
3.12.6 executable and Corretto 27.0.1.0.11 executable/release bytes and realpaths.
It is deliberately tied to those caller-supplied pins; there is no portable
installer or OS/JVM attestation. Supply explicit read-only inputs:

```sh
/pinned/python3.12 -B tools/development-corpus/reproduce.py \
  --bundle tools/development-corpus --batch both --python /pinned/python3.12 \
  --jdk-home /pinned/jdk/home --validator /read-only/validator.jar \
  --config /read-only/config.zip --out /fresh/path/replay
```

`--batch family60|discriminant4|both` selects only those existing fixed batches.
The runner compiles unchanged sources afresh, checks all 97 config members,
uses STRICT_LOCAL and the original JVM options, and calls unchanged drivers
using `Oracle.evaluate`. There is no `Oracle.main`, warmup, benchmark, download,
automatic retry, new case, label change or shared cache write. New operational
closures describe actual consumed files; the old closure's omitted lock and
planning handoff are not reconstructed or claimed consumed.

Read `result.json` together with the runner's exit status. `incomplete.json` is
retained even on success. Commands are declared before launch; raw partial
streams, launch exceptions, timeouts, nonzero exits and interruptions survive.
`request-accounting.json` distinguishes missing evidence from verified completion;
missing receipts never prove a request was not reached. `comparison.json` compares
full raw bytes after execution and records differences without relabeling.
Run attempts always need fresh output directories; preserve failed attempts.

Review narratives in `history/` refer to separately retained large probe/replay
trees which are **not bundled proof**. Original process evidence is bundled.
The initial discriminant report generator is genuinely unavailable; initial report
and audit bytes survive, but exact rerendering from that missing source is not
claimed. The later generator remains historical evidence only.

Read [CONTRACT.md](CONTRACT.md) for formats and bounds and
[notices/ATTRIBUTION.md](notices/ATTRIBUTION.md) for source licenses. Full native
conformance, protected held-out evaluation, complete configured-source
correspondence, structural equivalence, runtime context/site coverage, memory,
worker/resource acceptance and release/timing gates remain open.
