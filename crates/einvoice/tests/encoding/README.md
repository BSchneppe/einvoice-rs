# Private fresh-byte preparation checks

The unit-test module is included by the private `src/encoding.rs` module. No
public raw-byte API, test feature, imported-document conversion or unvalidated
`Document` constructor is involved. `Target::encode` remains unavailable.

`actual_bytes_originals_all_fields_and_opt_in_export` is a normal, nonignored
correctness test. With no environment variable it performs all assertions and no
filesystem IO. To export, explicitly create a new empty directory and supply its
absolute path. From the repository root, for example:

```sh
mkdir -p .scratch/bsc9/encoding-implementation
encoding_export_dir="$(mktemp -d "$PWD/.scratch/bsc9/encoding-implementation/export-XXXXXXXX")"
CARGO_TARGET_DIR="$PWD/.scratch/bsc9/encoding-implementation/target" \
  EINVOICE_BUILDER_EXPORT_DIR="$encoding_export_dir" \
  cargo +1.83.0 test -p einvoice --locked --offline --lib \
  encoding::tests::actual_bytes_originals_all_fields_and_opt_in_export -- --exact
PYTHONDONTWRITEBYTECODE=1 /usr/local/bin/python3.12 \
  crates/einvoice/tests/encoding/check_export.py "$encoding_export_dir"
```

The directory must already exist and be empty. Fixed safe filenames and
`create_new` prevent overwriting stale files. Repeating the export in that same
directory must fail. The files contain the exact vectors returned by production
`prepare`, without parsing/reserializing or XML templating. `cases.tsv` records
lengths, case classes, target/profile, both limit policies and semantic provenance.
The coordinator records source/build/fixture hashes and SHA-256 of the exact
vectors outside Rust before executing the pinned oracle. There is no crypto
dependency or automatic output into tracked fixtures.

A–J use the unchanged independent inputs and expected amounts in
`../fixtures/calculation/original.tsv`; its README describes the independent
Fraction and Decimal provenance. Party/contact/payment inputs are original test
business data. These ordinary source cases are candidates for full validation,
not an acceptance claim. `all-fields.xml` adds every represented field and option
in one ordinary development case. `all-fields.tsv` independently specifies every
expanded leaf/attribute and their source-defined order, using the settled pinned
CII mapping. It does not derive expected values from the serializer. The
syntactic-only `escaping.xml` deliberately puts CR/LF/tab/Unicode and XML
metacharacters into code/scheme fields to test infoset preservation; code-list
or other source-rule acceptance is not expected for that case. Metadata separates
this case from ordinary cases; do not count it as a conformance acceptance case.

The Python 3.12 helper uses ElementTree independently of Rust's xml-rs reader.
It checks all ordinary A–J line/group/total constants, complete expanded-name
mapping and order for all fields, and original CR/LF/tab/Unicode preservation
(including basis-unit attributes) from the escaping export. Rust additionally
checks the wire character references, exact envelope, deterministic bytes,
option alternatives, bounds and typed refusals. Neither parser substitutes for
XSD, CEN, XRechnung or native acceptance. The coordinator must run pinned complete
KoSIT and genuine same-target native validation on the exact returned bytes.
