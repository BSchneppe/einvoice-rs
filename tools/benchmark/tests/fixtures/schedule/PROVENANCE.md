# Expected Java schedule vectors

`Schedule.java` is the small pure scheduling probe saved during the fresh
BSC-11 lifecycle discovery on 2026-11-07. It constructs slots 0..40 afresh for
each run and reproduces `tools/oracle/java/Oracle.java`'s
`Collections.shuffle(order, new Random(271828L + run))`. It reads no invoices,
executes no validator and takes no measurements.

The discovery ran the installed Amazon Corretto 27.0.1.0.11. The parent then
independently reran this saved source with that installed JDK and compared all
five output vectors byte for byte before authorizing this implementation. The
copied golden is the parent's raw output, including Java list spacing and LF.
These facts are supplied discovery/parent provenance, not an attestation of a
JDK package closure or of any validation execution.

Reproduction in a temporary directory using an already installed Java:

```sh
java /absolute/path/to/Schedule.java > vectors.jsonl
shasum -a 256 vectors.jsonl
```

`vectors.jsonl`: 980 bytes, SHA-256
`1eb93aa8894db989cb2e8bacce32c86018efe7c66c2802215e28fe6ecd052f1e`.

`Schedule.java`: SHA-256
`597561aaf4a3b1a1587cf635d4c650c838fe04788884a5878cbf8627de270817`.

These five fixed vectors define expected coordinates only. No general Java
PRNG port, Python shuffle substitution, observed input identity, historical
sample reconstruction or benchmark result is implied. Unit tests consume these
small goldens offline; Java is not needed for the Python tests.
