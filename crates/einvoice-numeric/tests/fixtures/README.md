# Selected primitive reference evidence

`primitives.tsv` contains 57 independently authored primitive inputs and their
recorded Saxon HE 12.9 values/error local codes. The source was the isolated
2026-11-06 BSC-8 numeric discovery experiment, not invoice labels or a frozen
oracle. The exact authored XPath expressions are retained in UTF-8 hex alongside
the primitive operands. Decimal division rows explicitly carry output scale and
HalfTowardZero mode; round rows explicitly carry scale and mode. Those are fixture
policy inputs, not a default in the library. Runtime types, sequences and typed
XPath execution are outside these tests. Passing these rows makes no claim about
the other 54 discovery expressions or full validation.

Reference executable: KoSIT validator 1.6.3 JAR, SHA-256
`799e64befca97d4080e03608c80b85dd5a5ecc5f4ae4f35d1116ec2855b9a7c9`, reporting
Saxon HE 12.9. Java and javac: Amazon Corretto `27.0.1.0.11`, build
`27.0.1+11-FR`. The original probe evaluated each expression separately against
its synthetic document; selected expressions use only literals and numeric
primitives. No Java is needed for Rust tests.

To replay all committed expressions from a clean checkout, obtain the pinned
KoSIT validator JAR and Corretto version above (see `tools/oracle/README.md` at
the repository root). From the repository root, supply their local paths:

```sh
NUMERIC_JAVA=/path/to/corretto-27.0.1.0.11/bin/java
NUMERIC_KOSIT_JAR=/path/to/validationtool-1.6.3-standalone.jar
"$NUMERIC_JAVA" --class-path "$NUMERIC_KOSIT_JAR" \
  crates/einvoice-numeric/tools/ReplayPrimitives.java \
  crates/einvoice-numeric/tests/fixtures/primitives.tsv
```

The read-only verifier checks the SHA-256 of the **loaded** Saxon JAR, its product
version and the Java runtime/vendor before evaluating all 57 retained expressions.
It requires fresh singleton values or matching XPath error codes; any mismatch,
missing row or duplicate ID fails with a nonzero exit. It needs no original
discovery JSON, Rust executable, network access or invoice document, and never
rewrites expected results. This is a development primitive replay, not full
XPath or invoice conformance evidence.

The checked-in `tools/extract_primitives.py` extracts these rows from the original
recorded `cases.json` and `saxon-results.json` without invoking the Rust algorithm
or computing expected outputs. Source SHA-256 identities:

- cases.json: `0c9485de37ba60db43a9ca7a594c95ddfdd0dd81b426e7d0c12b71947c67bd67`
- saxon-results.json: `e621b68975b00208ff3837ddbf4fa79ac81f1c681ef06794d47c49882e2a5e9a`

Example extraction from retained development evidence (paths are arguments,
not build dependencies; these historical files are not shipped in the repository):

```sh
python3 crates/einvoice-numeric/tools/extract_primitives.py \
  .scratch/bsc8/numeric/cases.json .scratch/bsc8/numeric/saxon-results.json \
  /tmp/primitives.tsv
cmp /tmp/primitives.tsv crates/einvoice-numeric/tests/fixtures/primitives.tsv
```

Other integration tests use hand-calculated results and integer identities, not
outputs produced by this kernel. No existing oracle, generator or frozen data
was edited. New fixtures and extraction tooling remain confined to this crate.
