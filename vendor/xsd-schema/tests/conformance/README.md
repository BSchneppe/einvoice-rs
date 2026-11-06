# W3C XSD Conformance Test Driver

This test driver validates the xsd-schema crate against the
[W3C XSD Test Suite](https://www.w3.org/XML/2004/xml-schema-test-suite/)
for both XSD 1.0 and XSD 1.1.

## Obtaining the Test Suite

Clone the W3C XSD test suite repository:

```bash
git clone https://github.com/w3c/xsdtests.git
```

The test suite is organized as:

```text
xsdtests/
├── suite.xml           # Test suite manifest
├── nist/               # NIST tests
├── sun/                # Sun Microsystems tests
├── ms/                 # Microsoft tests
└── ibm/                # IBM tests
```

## Usage

All conformance commands must be run from the `rust/xsd-schema` directory.

**Always run conformance tests in `--release` mode** — debug builds are ~10x slower.

Always use `--features xsd11` for conformance runs — it enables the `regexml` crate
for full XSD regex support. The runtime XSD version is controlled by
`SchemaSet::new()` (1.0) vs `SchemaSet::xsd11()` (1.1), not the cargo feature.

```bash
# XSD 1.0 conformance
cargo test --test conformance --features xsd11 --release -- --test-suite /path/to/xsdtests --version 1.0

# XSD 1.1 conformance
cargo test --test conformance --features xsd11 --release -- --test-suite /path/to/xsdtests --version 1.1

# Filter by test group
cargo test --test conformance --features xsd11 --release -- --test-suite /path/to/xsdtests --group idA001

# Filter by test name (substring match, repeatable)
cargo test --test conformance --features xsd11 --release -- --test-suite /path/to/xsdtests --name particleS

# Verbose (per-test results)
cargo test --test conformance --features xsd11 --release -- --test-suite /path/to/xsdtests --version 1.0 --verbose

# Limit test count
cargo test --test conformance --features xsd11 --release -- --test-suite /path/to/xsdtests --max 100 --verbose

# Strict mode — exit non-zero if any test fails or errors
cargo test --test conformance --features xsd11 --release -- --test-suite /path/to/xsdtests --expect-pass
```

### CLI Options

| Flag | Short | Description |
|------|-------|-------------|
| `--test-suite PATH` | `-s` | Path to the cloned `xsdtests` directory (required) |
| `--version VER` | `-V` | Filter by XSD version: `1.0` or `1.1` |
| `--group NAME` | `-g` | Filter by test group name |
| `--name PATTERN` | `-n` | Filter by test name substring (repeatable) |
| `--max NUM` | `-m` | Maximum number of tests to run |
| `--verbose` | `-v` | Print per-test pass/fail results |
| `--expect-pass` | | Exit non-zero if any test fails or errors |
| `--help` | `-h` | Show help message |

### Performance Tip

**Always use `--release` mode.** The test driver is ~10x slower in debug builds,
making full suite runs impractical without it.

Save verbose output to a file first, then grep/analyze from that file
instead of re-running the full suite:

```bash
cargo test --test conformance --features xsd11 --release -- \
  --test-suite /path/to/xsdtests --version 1.0 --verbose 2>&1 > /tmp/xsd10_results.txt

grep "FAIL" /tmp/xsd10_results.txt | wc -l
grep "Schema was valid but expected invalid" /tmp/xsd10_results.txt | wc -l
tail -5 /tmp/xsd10_results.txt   # totals
```

## Test Outcomes

Each test in the W3C suite declares an expected outcome:

| Expected | Meaning |
|----------|---------|
| `Valid` | Schema should be accepted |
| `Invalid` | Schema should be rejected |
| `NotKnown` | Instance validity is unknown; meaningless for schema tests |
| `RuntimeSchemaError` | Latent schema error surfaced during instance validation |
| `ImplementationDefined` / `ImplementationDependent` | Outcome depends on an implementation-specific choice |
| `Indeterminate` | W3C suite intentionally does not assert a single correct result |
| `InstanceValid` | Instance document should validate against its schema |
| `InstanceInvalid` | Instance document should fail validation |

The driver compares actual behavior against the expected outcome and reports
each test as **Pass**, **Fail**, **Skip**, or **Error**.

For the W3C metadata-only outcomes `implementation-defined`,
`implementation-dependent`, and `indeterminate`, the driver records a `Skip`
instead of forcing them into `valid` / `invalid`. Schema-side `notKnown` and
`runtime-schema-error` are also treated as non-asserting because the XSTS
schema defines those values as meaningless for schema tests.

## Source Files

| File | Purpose |
|------|---------|
| `driver.rs` | Main test harness: CLI parsing, test suite XML parsing, schema/instance validation, result reporting |
| `report.rs` | Report generation: plain text summary, JSON, CSV, and HTML export |
