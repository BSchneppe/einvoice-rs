# Public development evidence provenance

The selected files are original PR52 hosted development artifacts, copied without
normalizing line endings, whitespace, JSON or empty files. They are not executions
of this consumer, synthetic native positives, conformant invoices or attestations.

- Reviewed merge: `2d82953fda88ad313144d6909e3c5010834d20af`.
- Actual receipt source HEAD: `890d1582ecb831e83c54b4e6cb817d1c3a7703d6`.
- Both full Git trees: `3e09dcfbde65cb699c5bf3d422618e059d81b8ba`.
- During import, both tree objects and an empty full-tree diff were independently
  checked locally, and all 407 inventoried Git blobs were read and compared with
  every supplied attempt inventory. No report boolean substituted for these checks.
- Source backend consumed-source identity:
  `11311b0c8933256f0bf854cdf546526162ebfeda37973b0e00f7d62e6cc79582`, domain
  `bsc8-maintained-xsd-consumed-source-v1`, 328 backend source members. The
  compatibility descriptor retains all 407 path/size/SHA256 records, not source bytes.

[`selected-core.json`](selected-core.json) records precisely **141 files / 1,131,837
bytes**. Selection digest:
`4b8905f470ed5b38138ac2ee2f45d6ec568b90c96c87212c370cffda3529771e`.
The digest domain is SHA256 of compact, sorted-key UTF8 JSON of the `files` array
sorted by path, without a final newline. The JSON manifest itself also has its
separate raw byte digest pinned by the compatibility descriptor.

Each of Linux, Windows and macOS retains all three receipted pairs (structural
probe, unchanged alpha, external caller change), both actual failed attempts
(malformed and snapshot-change), every actual attempt's original source inventory,
results.json and four caller XML files. Windows source inventories retain all
1,630 CRLF sequences per file; malformed stdout remains zero bytes. Original
absolute paths in receipts/manifests/results are opaque historical data and are
never dereferenced by the consumer.

[`artifact-file-manifest.json`](artifact-file-manifest.json) retains **all 417**
extracted path/length/SHA256 references. Only the selected 141 files are present
under `development-evidence/`. Excluded build stdout/stderr/status, cargo metadata,
identity-command logs and original schema-resource bytes remain explicit
references, not present or independently verified execution evidence. No executable,
XSD, vendor archive or config archive is redistributed. The full inventory's
historical `root` path is likewise only data.

The named import source was the read-only BSC8 sibling
`.scratch/bsc8/pr52-hosted-artifacts`. The importer read selected files once into
held byte buffers, checked selection lengths/digests, then copied those same
buffers. The original selection and source bytes are checked again after
implementation. No historical harness/cache was imported or executed. Source and
ZIP acquisition work is not rerun by the production consumer.

## Hosted transport metadata

[`hosted-artifact-metadata.json`](hosted-artifact-metadata.json) preserves the exact
supplied GitHub metadata. Its workflow run is
[42381413047](https://github.com/BSchneppe/einvoice-rs/actions/runs/42381413047),
repository ID `918302349`. Its recorded workflow head is
`017fb00666944b51a43c5f06c3778f6887adb3d0`, distinct from the receipt source HEAD;
these identities are preserved without substituting one for another. Supplied
parent acquisition matched these published lengths/digests to the three ZIPs;
this implementation did not download or execute them. Metadata and byte hashes
identify artifact transport references, not native execution or custody.

| Artifact / ID | ZIP bytes | ZIP SHA256 |
| --- | ---: | --- |
| schema-consumer-x86_64-unknown-linux-gnu / 13577314264 | 461053 | `a2ee4a55dd0c0a02f20b3fbbe0c8e288136f3e7c9796a796983b78490afc047a` |
| schema-consumer-x86_64-pc-windows-msvc / 13578150993 | 455778 | `19b88b7286fc2dd868577a7f7d422f50221c28651ac1cad1e7d0ef62de9503dd` |
| schema-consumer-aarch64-apple-darwin / 13577407546 | 462897 | `860696428f966081fd4584fcba1c13e1f713c1ca7a94229f27849eac3e52e750` |

The separate original config ZIP is not supplied here. Its reference is 499607
bytes / SHA256 `2530cd107c414511c5d0462ec10f886910395abfca820db82e83d70bf01221a8`.
Its four member metadata pins are in the descriptor; no claim of independently
loading those absent bytes is made.

## Authored XML and license

All selected XML is public owner-authored development data; no corpus XML,
held-out input, label or family data was accessed. The structural probe is the
exact 840-byte Git fixture, SHA256
`96b5b33ecc580dd139e9c8a28f6ccd67804d97eecab4262d94262cf381c0f708`:

[crates/einvoice-schema/tests/fixtures/minimal-schema.xml at reviewed merge](https://github.com/BSchneppe/einvoice-rs/blob/2d82953fda88ad313144d6909e3c5010834d20af/crates/einvoice-schema/tests/fixtures/minimal-schema.xml).
Its comment explicitly says structural-only and not a conformant invoice.

The other exact XML bytes are literals in
[the reviewed reproduction harness](https://github.com/BSchneppe/einvoice-rs/blob/2d82953fda88ad313144d6909e3c5010834d20af/crates/einvoice-schema/tools/test_reproduction.py):

| Literal | Bytes | SHA256 |
| --- | ---: | --- |
| `<alpha/>` | 8 | `294f0e86b9c8db0973be57b7b83ae32624a63a22b18825a7dc981f04f50ba346` |
| `<alpha>` | 7 | `8af67ce6092f04caa6c3a65a0c9503e8e5840ad426d148b93ec98c3039ad483d` |
| `<bravo/>` | 8 | `403b2693207a6debe03c006a0247e56e63a04afc05aae12d4beda3e09de772fb` |
| `<edited-by-caller/>` | 19 | `daabd560d0bc146bba699e26112c89be32beba3c0975404f1e34ebbbecf1e3c4` |

Copyright 2024 Benedikt Schneppe. The original repository
[LICENSE at reviewed merge](https://github.com/BSchneppe/einvoice-rs/blob/2d82953fda88ad313144d6909e3c5010834d20af/LICENSE)
is retained verbatim as [`LICENSE`](LICENSE), including permission and warranty
terms. The crate declares that root license. These provenance links document
locally inspected immutable Git bytes; no web request was needed. The XML notice
does not extend redistribution rights to excluded dependency/config/XSD/binary
bytes or resolve the separate backend source-header/full-terms question.
