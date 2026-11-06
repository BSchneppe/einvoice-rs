# Bounded offline conformance infrastructure

This Python 3.12 tool independently consumes frozen BSC-5 evidence and future
BSC-8 record exports. It does not run a backend, download, relabel, benchmark or
change the runtime. No native backend is available. It cannot establish full BSC-10
acceptance; `release_gate_eligible` is always false.

Install once, then all checks run offline:

```sh
python3.12 -m venv /tmp/bsc10-venv
/tmp/bsc10-venv/bin/pip install -r tools/conformance/requirements.txt
/tmp/bsc10-venv/bin/python -B -m unittest discover -s tools/conformance/tests -v
/tmp/bsc10-venv/bin/python -B tools/conformance/evaluate.py verify
```

Use an existing read-only cache explicitly for archive/member/extracted-file and
input-byte audits (no Java/PyArrow needed). Missing cache is an explicit integrity
gap; an absent directory is also an explicit gap. Supplying partial/corrupt cache is an error. No cache is inferred from
another checkout or populated automatically.

```sh
python3.12 -B tools/conformance/evaluate.py verify --cache /path/to/oracle/.cache --output /tmp/integrity.json
python3.12 -B tools/conformance/evaluate.py request --manifest initial --selection development --output /tmp/request.json
# BSC-8 supplies real records and the adjacent sidecar; this tool has no fake adapter.
python3.12 -B tools/conformance/evaluate.py compare --manifest initial --selection development --native /tmp/native.jsonl --output /tmp/comparison.json
```

`--manifest` accepts only `initial` and `sinabis`. `--selection full` uses every
complete row of that declared manifest, in order. Development is the default.
Never intersect candidate/reference records or deduplicate equal input bytes.
The union has 10,027 unique complete identities, not 10,041 independent inputs;
14 dataset identities belong to both manifests. Two containers/nine fragments are
separate scoped inventory. Theme/family splits are source-aware; mutation parents
are checked without pretending their public shared seed establishes independence.

[CONTRACT.md](CONTRACT.md) gives every request/record/sidecar key, exact severity,
stage, path, provenance and digest semantics for BSC-8. The machine contracts are
[native-record.schema.json](native-record.schema.json),
[run.schema.json](run.schema.json), and [scoped.schema.json](scoped.schema.json).
Python enforces cross-field invariants in addition to these schemas and the
unchanged frozen outcome schema.

Business findings compare as an ordered sequence, including identical duplicates,
untrimmed messages and source occurrences. Structural findings retain every raw
observation; equality or inequality of counts/prose cannot prove semantic parity.
Any structural observations yield an unresolved semantic gap in this slice.
Completed XSD rejection is distinct from processing failure; failures never pass
parity. Scope/target mismatch and broader reference execution remain visible.

Exit 0 verifies only the selected command's bounded contract. Exit 1 is comparable
mismatch; exit 2 is invalid/incomplete/failed/unresolved evidence. Every report
states absent controlled execution, runtime coverage/resource evidence, native
scoped execution and diagnostic mapping. Offline self-reporting is not attestation.
Harness fixtures are explicitly synthetic and cannot certify native conformance.

Reports default to stdout. File outputs are atomic and cannot overwrite evaluator,
frozen oracle, selected evidence/cache inputs or their resolved symlink/hardlink
aliases. File runs first atomically write an incomplete marker, then replace it
with their result; invalid evidence writes an error report so old success is not
reused. Always consume process exit status. Full reports/errors expose only aggregate counts/constant
codes, never held-out IDs, labels, input contents or diagnostics. Do not publish
supplied raw evidence files. Development diffs contain development identities and
complete raw structural arrays; full raw observations remain in the digest-bound
input files for restricted later analysis.

The frozen lock was constructed deliberately from reviewed Git objects at
`87e0a0a0c7095412faf23efad7aa8f0710328d03`, never from unreviewed current labels.
Run paths never refresh it. The evaluator's independent required path inventory
and byte checks detect accidental drift; code+lock review is the trust anchor.
The evaluator digest includes the fixed policy/code/schema/test/documentation
list in CONTRACT.md, excludes generated results, and has no self-hash cycle.
Future authentic execution, source closure attestation, structural cause mapping,
expanded independently labelled adversarial families, coverage, worker/resource
experiments and release performance remain separate acceptance work.
