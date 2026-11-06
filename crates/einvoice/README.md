# einvoice: public API contract

`einvoice` is the small public facade for the new native Rust library. The sibling
`einvoice-core` crate contains experimental typed XML parsing/serialization APIs;
it is not used by this facade.

**This milestone cannot validate invoice conformance or generate invoices.**
It implements immutable XML imports, target/profile preflight, explicit reports,
and the boundary for future creation. Every validation report currently has
`is_valid() == false`. No Java, subprocess, network download, generated stub rules,
or application-provided fake backend is involved.

## Receive an invoice

```rust
use einvoice::{Document, Target, ValidationOutcome};

let original = std::fs::read("received.xml")?;
let document = Document::parse(&original)?; // XML syntax only
assert_eq!(document.original_bytes(), original);
assert_eq!(document.to_bytes(), original); // exact bytes, not regenerated XML

let target = Target::xrechnung_3_0_2();
let report = document.validate(&target);
assert!(!report.is_valid()); // no native conformance backend yet

// Or include malformed XML and resource failures in a report:
let report = target.validate(&original);
match report.outcome() {
    ValidationOutcome::Valid => unreachable!("unavailable in this milestone"),
    ValidationOutcome::InvalidDocument => { /* inspect document diagnostics */ }
    ValidationOutcome::UnsupportedTarget => { /* fix target selection */ }
    ValidationOutcome::EngineFailure => { /* inspect failure(), not invoice data */ }
    ValidationOutcome::ResourceLimit => { /* processing stopped; not conformance */ }
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

`Document` owns immutable bytes (clones share them). It has no setters and no
conversion into `InvoiceDraft`. Original serialization preserves BOM, supported
encoding, attribute ordering, entities, comments and whitespace exactly. Mutating
the caller's input buffer or a returned `Vec<u8>` cannot change the document.
Parsing consumes the entire XML document and requires a single root. The parser
supports UTF-8 and BOM-marked UTF-16, among other encodings; unsupported encodings
are capability failures, not proof of invalid invoices.

Parsing does not check XSDs or business rules. Unknown elements/attributes and
malformed business amounts remain in the original source. The facade only checks
the root expanded name and exactly one direct-path profile ID. It does **not**
pretend to detect every structural fault: until BSC-8 supplies the full source XML
validator, XSD and rule stages explicitly remain skipped. A matching profile is
not evidence that those stages would pass. Diagnostic locations are one-based
parser character positions in the source, not byte offsets; rule XPaths can be
absent. `API-` diagnostics identify facade checks, not invented CEN/XRechnung rules.

`Limits` defaults to 16 MiB of input and 128 levels of element nesting. The pinned
XML parser additionally limits names, attribute counts/values, text and entity
expansion. Resource exhaustion has a separate outcome and never becomes a document
fault or acceptance. DTD processing is explicitly unsupported: a bounded prolog
guard rejects DOCTYPE before the parser can expand any declarations, including in
BOM-marked UTF-16. This is an `EngineFailure::UnsupportedDtd` capability outcome,
not a claim of invalid invoice data. DTD-like text inside comments, processing
instructions and CDATA remains accepted. No external entity resolver or network
access is provided.
These budgets are initial safeguards, not a completed adversarial robustness gate.

## Select the exact target

`Target::xrechnung_3_0_2()` selects
`xrechnung-3.0.2-cii-d16b-kosit-2026-08-31`: XRechnung 3.0.2, CII D16B, KoSIT
configuration 2026-08-31, CEN 1.3.16 and XRechnung Schematron 2.6.0. The KoSIT
archive hash pins the included four CII XSDs and severity/acceptance policy.

`Target::named(id)` supports configuration-driven selection. An unknown exact ID
produces `UnsupportedTarget` before parsing raw input, even if it is malformed.
An already parsed `Document` retains its completed syntax stage in that report. There
is no UBL, EN16931-only, older-version, or automatic profile fallback. Changing XML
prefixes without changing namespace URIs does not change profile identity.

`ValidationReport::bundle()` describes the **expected** bundle and archive hashes.
It does not claim that a generated package was loaded. `implementation()` records
the facade version and bundled parser version; `validator` and `generator` are
explicitly `None` until an implementation actually exists. Each stage records
whether it completed, failed, or was skipped and why. The schema, CEN, XRechnung,
and acceptance stages are all mandatory. An unavailable backend is reported as
`EngineFailure::BackendUnavailable` with those stages skipped.

Never infer validity from an empty diagnostic list, absence of errors, parsing
success, or profile matching. Use `is_valid()` for complete acceptance;
`is_complete()` separately describes stage completion. Invalid-document reports
may also be incomplete: a wrong profile stops later stages.

The three source archive SHA-256 values in `BundleIdentity` were independently
recomputed from the supplied research artifacts and match the
[authoritative discovery record](https://linear.app/bschneppe/document/discovery-evidence-and-pinned-references-462b18ff0ee2).
No archive is imported here. Artifact acquisition, source-file manifests and oracle
reproducibility belong to BSC-5/BSC-7. The event parser is pinned to `xml-rs = 0.8.28` because its resource/capability
errors use strings; behavior tests guard that mapping. A separate
`xmlparser = 0.13.6` lexical pass checks namespace declarations before reporting
syntax success: the event parser otherwise discards duplicate `xmlns` attributes.
This pass uses a temporary, non-lossy decoded view and never changes imported bytes.
Both parser versions appear in implementation provenance. This additional work is
not a conformance backend, and its throughput has not been benchmarked.

## Start a new invoice

```rust
use einvoice::{BuildError, InvoiceDraft, Target};

let target = Target::xrechnung_3_0_2();
let draft = InvoiceDraft::new("INV-2026-001", "EUR");
assert_eq!(draft.invoice_number(), "INV-2026-001");
assert!(matches!(target.encode(&draft), Err(BuildError::Unavailable { .. })));
```

This is an intentionally incomplete draft, not an invoice ready to submit. It
records only invoice number and currency. BSC-9 owns parties, multiple lines,
standard VAT rates, bank transfer, exact decimal calculations and optional expected
amounts. No `f64`, rounding, zero substitution, totals, or XML generation are
implemented here. That ticket must validate the actual newly encoded bytes with the
same target before returning successful output. Imports remain a separate immutable
path; there is no lossless editing or complete conversion promise.

## Verification and remaining dependencies

Run `cargo test -p einvoice --locked` and
`cargo run -p einvoice --example workflows --locked`. Tests use the public API,
including syntax failures, unknown structure, exact byte preservation, namespace
and profile handling, unsupported targets/encodings/DTDs, namespace-expanded attribute uniqueness, limits, and unavailable
validation/creation. The existing fixture is used as source XML, not as a new claim
of oracle acceptance.

BSC-7 must supply generated packages and their full provenance; BSC-8 must implement
complete source-based structural/business validation, actual rule diagnostics and
policy; BSC-9 must implement validated creation. BSC-10/11 own full adversarial and
performance gates. None of those gates is claimed by this facade.
