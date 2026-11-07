# Maintained XSD backend regression workspace

This unpublished, independent Cargo workspace tests the repository's patched
`xsd-schema 0.3.0`. It does not implement the invoice facade, source DOM adapter,
walker, worker pool or validation report contract. It does not certify KoSIT or
Xerces diagnostic equivalence. No Java, invoice corpus or held-out inputs are
used by these tests.

The registry dependency is exactly `=0.3.0`, with a local crates.io patch to the
complete published package in `vendor/xsd-schema`. The package remains named
`xsd-schema`, version `0.3.0`, and is explicitly unpublished. Its original
`Cargo.toml.orig`, VCS metadata, source headers, tests, docs, notices and assets
remain intact. See `vendor/xsd-schema.provenance.json` and the ordered patches.
A future distribution decision is still required before publishing consumers.

## Source and dependency identity

The archive authority is SHA-256
`c6e18f97809c6066c39ea64c9067fee882e92181bdcb366c6a8d99ab4f618d3e`.
The provenance manifest records original and consumed bytes/hash for all 328
regular files and ordered patch digests. The verifier computes the consumed
source identity from actual bytes using canonical compact, key-sorted JSON of
`{domain:"bsc8-maintained-xsd-consumed-source-v1",files:[{path,sha256},...]}`;
paths are sorted. There is no self-digest or assertion of an executing binary's
identity. Default verification checks the recorded local inventory offline;
with the supplied authenticated archive it also verifies every original byte
and exact forward/reverse patch replay.

`Cargo.lock` was seeded from the demonstrated Rust 1.83 backport resolution.
All 91 non-harness package records remain identical. The deliberate pins are
`proc-macro-crate 3.3.0`, `indexmap 2.13.0`, `hashbrown 0.16.1`. They include
lock-only dependencies, not claims that every package is enabled host code.
The harness's direct dev dependency on already-locked `rust_decimal 1.43.0`
supports unchanged upstream public tests without adding or updating packages.

`verify_features.py` checks Cargo's resolved enabled features and actual local
manifest path. The only accepted backend features are `default` and
`unicode-normalization`. `async`, `xsd11` and `compose` remain disabled; an
unexpected unification or missing Unicode normalization fails the guard.
XSD 1.0 is the authored fixture mode. No WASM support is claimed. CI checks
Rust 1.83 on Linux x86_64, Windows x86_64 and macOS arm64; these are configured
checks, not claims that a CI run has already completed.

## Evidence and cooperative control

`ValidationError::evidence()` returns optional owned `DiagnosticEvidence`.
Evidence is boxed inside the existing boxed diagnostic item: the error and
its `Result` layout do not grow. Raw code, message, location, subject, paths,
ordering and duplicates are preserved. `NativeCause` retains actual value and
facet enums, content rejection/completion, required names (including their
namespace), attribute prohibited/not-allowed/fixed failures, undeclared
elements and selected xsi:nil/xsi:type failures at their emission sites.
Deferred xsi:type diagnostics keep structure until the child is pushed.

`governing_type` and `actual_type` distinguish the outer governing type from
the innermost observed dispatch (or resolved xsi:type). For atomic effective
facets these can be the same resolved type: this is not a primitive-type or
facet-declaration inference. `facet_origin` remains unavailable. Component
kind, actual declared name, global/local declaration status and recorded
schema URI/span are copied when available. Anonymous types have no invented
name. Local names do not uniquely identify declarations. Schema spans often
cover only the start tag; they are not invoice locations or whole declaration
ranges. Exact attribute-use and wildcard origins and many other causes remain
unavailable. A rejected child's name is recorded, but no triggering child
occurrence path is invented; the raw diagnostic still identifies its parent.

`ValidationSink::stop_requested(&self) -> bool` defaults to false for existing
implementations. Every runtime sink callback passes a pre/post control check.
Push entry points, multi-diagnostic dispatch loops and IDREF finalization also
check control. First refusal terminates the finalization loop immediately.
`ValidationRuntime::abort_run(message)` permits adapter-initiated aborts.
The first operational failure wins, persists even if the signal clears, and
makes subsequent validation pushes inert and `end_validation()` return `Err`.
`operational_failure()` exposes it out of band: operational errors are never
recursively emitted into the exhausted sink or added as document findings.
Previously observed findings remain available in the sink.

The hook is cooperative, not a hard deadline/RSS/work limit. It cannot interrupt
in-progress regex, type, native content-model or NFA work. The test sink's byte
probe counts message bytes solely to exercise the hook, not full evidence
retention or allocator usage. Count/byte policies and stop causes can be held
in an adapter's shared control later; this backend has no facade/report coupling.
A cancelled runtime must be discarded. A fresh runtime over the prepared
schema is independently healthy in the regression cases. This does not reset
upstream's persistent schema name table or establish worker retirement policy.

## Git byte preservation

The published `Cargo.toml.orig` also has CRLF line endings, and upstream's
`.gitattributes` only protects the XSD assets. Stage with conversion disabled.
Local or global ignore rules can omit upstream `CLAUDE.md`, so force-add that
specific package member. It is retained package data, not project instructions.
After staging this slice, verify the actual index through a temporary archive
before committing (run from the repository root):

```bash
set -euo pipefail
git -c core.autocrlf=false add -- vendor/xsd-schema vendor/xsd-schema-patches vendor/xsd-schema.provenance.json tools/schema-backend .github/workflows/schema-backend.yml
git -c core.autocrlf=false add -f -- vendor/xsd-schema/CLAUDE.md
schema_snapshot=$(mktemp -d "${TMPDIR:-/tmp}/bsc8-staged-schema.XXXXXX")
git archive --format=tar "$(git write-tree)" | tar -xf - -C "$schema_snapshot"
python3 "$schema_snapshot/tools/schema-backend/verify_provenance.py"
```

The verifier checks all 328 package members, patch bytes and executable modes
(on platforms that expose them). A working-copy check alone cannot detect an
omitted index entry. Keep the temporary archive for inspection if verification
fails. Verify again after checkout with `core.autocrlf=false` or `input`; CI
disables conversion before checkout. Do not normalize upstream whitespace to
make a whole new vendor package pass `git diff --check`; it includes original
trailing spaces and CRLF. The three patch files and new maintainer code have
clean whitespace.

## Reproduction

Use Rust 1.83.0 and Python 3.12 with `/bin/bash`, `login=false`. Keep compiler
output separate from the root target directory, for example:

```bash
export CARGO_TARGET_DIR="$PWD/tools/schema-backend/target"
python3 tools/schema-backend/verify_provenance.py
python3 -m unittest discover -s tools/schema-backend -p 'test_*.py' -v
python3 tools/schema-backend/verify_features.py --target aarch64-apple-darwin
cargo +1.83.0 test --locked --manifest-path tools/schema-backend/Cargo.toml
cargo +1.83.0 fmt --manifest-path tools/schema-backend/Cargo.toml --all -- --check
rustfmt +1.83.0 --edition 2021 --check vendor/xsd-schema/src/validation/errors.rs vendor/xsd-schema/src/validation/simple.rs vendor/xsd-schema/src/validation/runtime.rs vendor/xsd-schema/src/validation/validator.rs
cargo +1.83.0 clippy --locked --manifest-path tools/schema-backend/Cargo.toml --all-targets -- -D warnings
cargo +1.83.0 clippy --locked --manifest-path tools/schema-backend/Cargo.toml -p xsd-schema --lib -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo +1.83.0 doc --locked --manifest-path tools/schema-backend/Cargo.toml --no-deps --document-private-items
python3 tools/schema-backend/run_upstream.py
```

After dependencies have been fetched, add `--offline` to Cargo commands (the
upstream/comparison helpers always run Cargo offline). Use the actual host
triple for the feature check. CI supplies the three intended triples.

With a separately supplied copy of the pinned archive:

```bash
export XSD_UPSTREAM_ARCHIVE=/path/to/xsd-schema-0.3.0.crate
python3 tools/schema-backend/verify_provenance.py --archive "$XSD_UPSTREAM_ARCHIVE"
python3 tools/schema-backend/verify_provenance.py --archive "$XSD_UPSTREAM_ARCHIVE" --reproduce /tmp/new-xsd-package
python3 tools/schema-backend/compare_upstream.py "$XSD_UPSTREAM_ARCHIVE" --output tools/schema-backend/target/raw-comparison
python3 -m unittest discover -s tools/schema-backend -p 'test_*.py' -v
```

`--reproduce` requires a nonexistent destination. Verification performs no
network requests; the CI download is a separate step, authenticated by the
pinned archive digest. `record_provenance.py ARCHIVE` is an explicit maintenance
operation to regenerate the inventory after reviewed patch changes; it also
requires successful full replay verification.

`run_upstream.py` uses a disposable workspace to select seven unchanged public
integration suites: attributes, decimal capacity, facets, nil, root validity,
validation API and validation roots (104 tests observed locally). Cargo cannot
directly test dev targets of a nonmember path dependency. The disposable
workspace uses the same lock and only already-locked test dependencies; it
neither edits the vendored package nor enables optional backend features.
Full upstream unit/bench/conformance/optional-feature suites are not certified.
Defensive schema-warning tests deliberately use public programmatic arenas:
the normal text-schema loader eagerly rejects those invalid constraints.

Strict private documentation of the *entire upstream backend* currently fails
on existing unescaped HTML and unresolved links. The original manifest-only
backport fails with the identical 45 rustdoc lint diagnostics plus the terminal error. This slice
preserves those upstream comments; CI gates the harness docs with `-D warnings`
and checks the backend library with strict Clippy, without claiming upstream
private rustdoc is warning-free. Logs of both strict rustdoc attempts are
retained in the implementation report directory.
