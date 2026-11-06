# DEVELOPMENT family proposal, revision 3

This is a **proposed, unfrozen, unlabelled** batch of 59 inputs in six whole
DEVELOPMENT technique families. The parent must inspect it, freeze exact bytes
and the complete allocation, and post the plan to Linear before any fresh pinned
reference labels. Nothing here evaluates an invoice or authorizes a runtime,
compiler, builder, performance, release, or native-conformance change.

The current machine-readable ledger is [v3/proposed-ledger.json](v3/proposed-ledger.json).
The compact [candidate index](candidate-index.md) lists all exact parameter values
and their source references without the longer byte-anchor records.
Every XML/schema/business/scenario/acceptance/processing expectation is the string
`unknown`; observations are null and `oracle_evaluated=false`. Positive and
negative **intent** describe independently selected source constraints, never
observed outcomes. A single case can have both intents. A future accepted
negative or rejected positive remains in the corpus.

The original accepted public development seed is the only invoice ancestor:
`../dev-seed-smoke-01/development-seed.xml`, 11,214 bytes, SHA-256
`881f7bc3ff0513a6627a981301af2c0ec9bcdce2c9d1628d7858e581f4d8e91c`.
No prior corpus inputs, result labels, discriminant outcomes, builder fixtures,
or runtime expectations selected these mutations. The required discovery/plan
documents were read as task context. Seed acceptance was supplied as an entry
condition, and was not rerun here.

| Whole family | Cases | Fixed technique and parameter domain |
|---|---:|---|
| F01 | 5 | Global prefix alias, default RAM namespace, locally shadowed URI, and empty header delivery with original versus equivalent aliased prefix |
| F02 | 14 | Buyer-reference absence/empty/space/duplicate; CareOf prohibition; singleton duplication; zero lines; terms duplication; second-line tax count 0/2; missing product names at line 2, line 3, both, and both with shifted source location |
| F03 | 11 | Decimal plus/leading zero/trailing zero/exponent/magnitude; negative price; sum immediately below/at/above a half-cent and an opposing header at the tie; precision at line 2 and 3 |
| F04 | 6 | Leap/non-leap February 29, month 13, day zero, hyphenated issue date, format 203 |
| F05 | 5 | Currency case/padding/out-of-list token, unit case, out-of-list units at lines 2 and 3 |
| F06 | 18 | XML1.0 CR/CRLF literal/reference; XML1.0 versus XML1.1 NEL/LS literal/reference; XML1.0 tab and XML1.1 NEL attributes literal/reference; UTF-8 BOM and UTF-16 BOM |

These are finite contrasts selected from the seed and specific source constructs,
not a random case-count target or a Cartesian product inflated to a quota.
All related variants are allocated together. The two encoding representation
controls and the seed identity control **do not add independent diversity**.
There are 57 mutation cases plus two representation controls; 59 is an input
count, six is a technique-family count, and one is the ancestor/source-shape
count. No case-level, source-provenance, statistical or held-out independence is
claimed. Do not promote parameter variants to independent families.

F02's shifted missing-name case overlaps its unshifted recipe and remains F02.
F01's two empty-delivery variants share the same removal and expanded name and
remain one family; the pinned empty rule explicitly uses lexical `name()`.
F03's single and multiple precision variants remain F03; F05's single/multiple
unit variants remain F05. F06 keeps paired source forms, versions and encodings
together. Other families do not combine these mutations. Shared seed bytes and
collateral rules can still correlate all families. No cross-split overlap audit
or claim of private-family inaccessibility has been performed.

## Exact recipes and original locations

Each row binds the original seed hash, a canonical recipe hash, input hash and
byte length, parameters, source references, modification intents, and whole-family
allocation. Each splice specifies original zero-based seed byte offsets, exact
old/new bytes in base64, old-byte hash, a checked **unique surrounding byte
anchor**, a named source occurrence, and original one-based seed line and byte
column. Replay checks the complete seed hash and each anchor/old span, applies
nonoverlapping edits in descending original offsets, then applies the declared
encoding/BOM transformation. It does not parse and reserialize the invoice.

UTF-16 uses explicit little-endian code units, `FF FE`, and declaration
`encoding='UTF-16'`; UTF-8 BOM uses `EF BB BF`. Other cases remain UTF-8 without
a BOM. XML1.1 cases change only the declared version plus their declared source
content/attribute splice. No implicit newline, whitespace or encoding conversion
is allowed during later reference labeling.

Location means **the original submitted source**, not a normalized tree or a
reserialized copy. The F02 location control inserts the exact comment
`<!--BSC10 location control\nneutral\n-->\n` immediately before line item 2;
its three inserted LF bytes and unchanged line IDs are recorded. Later emitted
locations must remain bound to these input bytes. This design predicts neither
diagnostic paths nor target line/column conventions.

Rerendering twice **from the same original seed** produces identical bytes.
Applying a recipe to its already-mutated output is refused by the seed hash:
"idempotence" here is reproducible materialization, not repeated unguarded edits.
Creation refuses an existing version directory; verification only reads it.

The nontrivial rounding recipes are fully fixed now. Original other line totals
sum to `3232.18`. First-line totals `1927.344`, `1927.345`, `1927.346` give exact
unrounded sums `5159.524`, `5159.525`, `5159.526`. Header line total choices are
respectively `5159.52`, `5159.53`, `5159.53`; the opposing tie recipe fixes
`5159.52`. Wherever the lower header is chosen, header tax basis and VAT basis
are also `5159.52`, grand/due totals `6139.83`; tax stays `980.31`.
Original prices are retained. Fractional precision and arithmetic collateral
are deliberate: these are sum/round-expression probes, not assumed globally
valid invoices or an alternate arithmetic validator.

## Source intent and later evidence

Source references resolve through [v3/source-provenance.json](v3/source-provenance.json):
archive URL/hash, selected member/hash and exact rule/type line ranges with
excerpt hashes. Only seven explicitly allowlisted original rule/XSD members are
used; no archive test/example member names or metadata were inspected/printed.
Static rule citations establish intent, not equivalence with configured execution.
The unrelated CII-SR-282 authority question is outside this design.

For XML source processing, permanent normative W3C edition/section URLs and short
read notes are recorded there. The relevant sources are
[XML1.0 §§2.11, 3.3.3, 4.1, 4.3.3](https://www.w3.org/TR/2008/REC-xml-20081126/),
[XML1.1 §§2.11, 3.3.3, 4.3.4](https://www.w3.org/TR/2006/REC-xml11-20060816/), and
[Namespaces §§6.1–6.2](https://www.w3.org/TR/2009/REC-xml-names-20091208/).
These reads select source forms; they do not establish what the current parser,
schema stage, configured target or future native backend does. In particular,
`languageID` is declared `xsd:token`: later schema whitespace processing is a
separate issue from XML attribute normalization. XML1.1 capability is unobserved.

Future labeling must preserve every attempted case, original input/hash,
processing status, complete stages, scenario match, raw/effective severity,
ordered findings including identical duplicates, original locations, all
collateral findings and raw process failures. Empty findings, a parser success,
an exception, a timeout, or absent stages must never be substituted for a verdict.
Labels append in separate run storage after review/freeze; they do not rewrite
intent, input bytes, candidate order or family allocation. A changed case requires
a new version with retained predecessor and disclosed evaluation exposure.

## Checks actually performed

`python3 -B .scratch/bsc10/expansion/family-design/design_batch.py verify`
replays every current recipe and compares stored bytes/hash, recipe digest,
unique anchors, unknown expectations, source provenance and file manifest.
It checks actual line-item source order and cardinalities. Missing-name counts
are `[1,0,1]`, `[1,1,0]`, `[1,0,0]` for the three sequential probes; the shifted
variant retains `[1,0,0]`. Second-line tax counts are `[1,0,1]` and `[1,2,1]`.
IDs remain `1,2,3`: multiplicity comes from real sequential contexts, not merely
repeated assertion IDs or duplicated line identifiers.

53 XML1.0 inputs were parsed with the existing stdlib ElementTree only for
**synthetic syntax/count QA**. Six XML1.1 inputs were deliberately not parsed
with that checker; no XML1.1 capability was assumed. These counts say nothing
about XSD/business validity. No new XML parser, XPath/Schematron interpreter,
schema validator or competing production backend was implemented. No oracle,
compiler, runtime, native adapter, timing or performance measurement ran.

## Revision history and handoff

`v1/` is retained byte-for-byte, with its original `design_batch.v1.py`. Revision
2 replaces the XML1.0 CRLF attribute pair with an XML1.1 NEL attribute pair to
close a source-version design gap. The earlier pair remains in `v1/` with explicit
supersession records in the current ledger. Five decimal intent descriptions were
clarified without changing their inputs; XML1.0 tab rows gained explicit version
metadata and refined intent. Revision 3 replaces the local seller-name alias and
second changed-URI form with the paired empty header-delivery prefix probe. The
remaining local-shadow case still supplies changed-URI intent. This revision
exercises the actual source lexical exception while keeping its overlapping
removal/prefix mutations within F01. `v2/` and `design_batch.v2.py` are retained.
There was no outcome-based revision or discarded evaluation.

`v1/revisions.jsonl`, `v2/revisions.jsonl` and `v3/revisions.jsonl` retain candidate identities,
predecessors, recipe/input hashes and reasons. The retained drafts total 63
distinct input byte streams across history; the **current proposed labeling batch
is exactly 59**, not the union of draft alternatives. Each version's canonical
ledger binds its predecessor digest; each retained script reproduces/verifies its
own proposal version. Parent review can reject
or revise it before freezing, with a new retained version.

The recorded [verification result](verification.json) binds the checks performed
with Python 3.12.6, including predecessor/script digest checks and refusal of all
59 mutated inputs as ancestors. This check's interpreter version is separate from
the future pinned reference-execution closure, which remains unrun here.

[review-bundle.json](review-bundle.json) enumerates exact new scratch file paths
and hashes for handoff. Its hashes commit reviewable bytes; they provide neither
confidentiality nor evaluator attestation. No held-out family was generated or
read. Shared-UID inaccessibility is unavailable. No tracked source file, original
pin, evaluator contract or source-inventory sibling file was edited. No commit,
PR, Linear write or resource/release/coverage claim is part of this handoff.
