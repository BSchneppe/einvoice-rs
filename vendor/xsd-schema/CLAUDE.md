# CLAUDE.md - xsd-schema build instructions

The standard: https://www.w3.org/TR/xmlschema11-1/

## Build Workflow

After making code changes, always run clippy and tests for **both** feature configurations:

### Default (XSD 1.0 only):
```bash
cargo clippy --all-targets                    # Lint check (0 warnings expected)
cargo test --all-targets                      # Run XSD 1.0 tests + conformance
```

### Full (XSD 1.1 + XPath):
```bash
cargo clippy --all-targets --features xsd11   # Lint check (0 warnings expected)
cargo test --all-targets --features xsd11     # Run ALL tests
```

### Miri (unsafe code verification):
```bash
cargo +nightly miri test --lib namespace::table   # NameTable unsafe verification
cargo +nightly miri test --lib                     # Full lib under Miri
MIRIFLAGS=-Zmiri-disable-isolation cargo +nightly miri test --lib --features xsd11   # + XSD 1.1
```
With `xsd11` the XPath engine reads the clock, which isolated Miri refuses; the
targeted runs per `unsafe` block are in `doc/UNSAFE.md`.

### Doctests (the guides in `doc/` are doctested):
```bash
cargo test --doc                              # default features
cargo test --doc --features xsd11
cargo test --doc --all-features               # also COMPOSE.md and the async examples
```
`cargo test --all-targets` does not run doctests. A guide example that needs a
feature is guarded with `#[cfg(feature = "...")]` (see `doc_guides` in `src/lib.rs`).

### W3C conformance (always `--release`, always `--features xsd11`):
```bash
cargo test --test conformance --features xsd11 --release -- --test-suite ../../xsdtests --version 1.0
cargo test --test conformance --features xsd11 --release -- --test-suite ../../xsdtests --version 1.1
# The tests whose manifest names no XSD version (run as 1.0 above), under SchemaSet::xsd11():
cargo test --test conformance --features xsd11 --release -- --test-suite ../../xsdtests --versionless-1.1
```
Compare runs by the FAIL and ERROR lines and the passed count.
