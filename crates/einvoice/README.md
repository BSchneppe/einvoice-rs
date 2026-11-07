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

The constructor creates an incomplete draft. Its public business fields hold
seller/buyer details, addresses, electronic endpoints, contacts, references,
bank-transfer accounts, ordered lines and optional amount expectations. Required
business information has no invented default. Contact points distinguish a person
from a department; account identifiers distinguish IBAN from proprietary accounts.
Due date and payment terms remain optional for this exact pinned target.

`draft.field_diagnostics()` collects missing or inconsistent required fields,
unsupported invoice types/VAT categories/payment means, and API text/date findings.
Each finding has a stable API code, field path, message and supporting rule IDs
where applicable. Those IDs are source references, not evidence of native rule
execution. Seller identity and tax identification are separate obligations: local
tax registration alone needs an additional seller identifier or legal registration.
Supplied text is preserved, including original decimal spellings and expectations.

**An empty field-diagnostic list does not establish invoice validity.** This helper
does not parse numeric inputs, check code lists, calculate or compare amounts,
generate XML, or run native validation. It scans caller-owned input and collects
findings without a resource budget. `Target::encode` still returns
`BuildError::Unavailable` even for a populated, calculated draft. No `f64` or
malformed-to-zero path exists.

`draft.calculate(&target)` prepares exact amounts in a separate immutable
`CalculatedInvoice`; `calculate_with_limits(&target, CalculationLimits { .. })`
selects explicit limits. It first recognizes the existing exact target, checks
all collection counts and every supplied string's byte length, then collects
bounded field findings and parses numbers through the shared `einvoice-numeric`
kernel. The draft and imported bytes remain unchanged.

Line net is exact quantity × price, divided directly by the explicit positive
price basis (one only when omitted), rounded once to two places with ties away
from zero. Quantities may be signed or zero; prices are nonnegative and standard
VAT rates are positive. An explicit basis unit must equal the quantity unit.
Rounded line nets are summed by numerically equal VAT rate; groups are sorted by
budgeted numeric comparison. Group tax is exact basis × rate / 100, rounded once
with the same policy. Net sums line nets, tax sums group taxes, gross = net + tax,
and due = gross. All currencies use two-place monetary output for this target;
there is no currency conversion or minor-unit substitution.

Every supplied amount expectation compares exactly, without rounding or
validation tolerance. Trailing-zero spellings compare equal. Mismatches give
`BuildDiagnostic` findings with exact public field paths, supplied/computed values
and currency. Malformed expectations give lexical findings. Repeated numeric
expected-group rates are ambiguous findings; rates absent from calculated lines
are unknown-group findings, including when no expected amounts were supplied.

The result exposes read-only `lines()`, `vat_groups()` and `totals()` plus
`currency()`. Line inputs (`quantity`, `price`, optional `price_basis`, `vat_rate`)
are canonical exact decimal strings for eventual serialization without repeating
arithmetic. Line `net`, group `basis`/`tax` and total `net`/`tax`/`gross`/`due` are
always two-place strings. Input line order and explicit-versus-omitted basis are
preserved. It contains no party/description copies and cannot construct a public
`Document`. **Calculation success is arithmetic preparation, never conformance.**

`CalculationError` separates `UnsupportedTarget`, bounded `Fields` findings,
`Resource { path, cause }` and unexpected `Processing { path, cause }` failures.
Unknown target identifiers are not copied. Public `CalculationResource` and
`CalculationFailure` types require no knowledge of numeric storage or XPath.
One numeric budget covers all parses, operations, grouping/order/expectation
comparisons and rendering; exhaustion never becomes an invoice fault or zero.

`CalculationLimits::default()` allows 1,000 lines, 32 accounts, 1,000 expected
groups, 1 MiB total supplied text, 16 KiB per business string, 4,096 diagnostics
and 16 MiB retained result/diagnostic output (strings and records). Numeric
per-value limits are 1,024 lexical bytes, 8,192 coefficient/intermediate bits,
absolute scale 1,024 and 1,024 rendering/scratch bytes; cumulative numeric fuel
is one billion abstract work units. These accommodate ordinary invoices and the
original high-precision rounding cases while bounding adversarial work and
retention. Larger callers may raise limits explicitly. Zero/platform-unsafe
configurations are rejected with checked size arithmetic. Each rendering reserves
its full per-value allowance before allocation and refunds unused bytes, so a
tight aggregate cap may conservatively refuse even a small final string. Limits
are deterministic conservative fuel/size bounds, not allocator/RSS guarantees;
vector capacity, numeric scratch and allocator overhead remain proportional to
the bounded data. No elapsed-time deadline or allocator-failure recovery is
claimed.

Plain payment terms preserve caller prose, including embedded `#`, Unicode
whitespace and text without a final newline. An LF-separated line beginning with
`#` after leading XML whitespace (space, tab, CR, LF) yields
`API-BUILD-UNSUPPORTED-TERMS` at `payment_terms.text`. This builder capability
restriction includes source-valid structured discounts and invalid hash protocols;
it is not an incoming-validity rule or a claim to interpret natural-language
discounts. Field diagnostics and bounded calculation use the same check.

A private preparation path calculates once from the same immutable draft and
emits fresh CII UTF-8 bytes with the fixed source-mapped vocabulary. It consumes
canonical calculated numbers without repeating arithmetic, preserves every
represented business field, counts exact escaped output before allocation, and
checks every append against the reserved size. Its separate XML output cap is
16 MiB by default; calculation limits still apply independently. These bounds do
not promise total RSS or allocator quotas. This path returns no public bytes or
`Document`; `Target::encode` remains `Unavailable` until genuine native validation
and independent KoSIT acceptance of the exact emitted bytes are integrated.

Private unit tests can export actual returned vectors for independent checks;
see [the encoding test instructions](tests/encoding/README.md). These outputs
have development provenance and are not validated invoices.

The creation model has no fields for allowances, charges, prepayments, payable
adjustments or structured cash discounts. It supports no credit-note generation,
PDF workflow or import editing. Incoming validation keeps its full target scope.
BSC-9 must validate actual fresh bytes with the same native target and independent
KoSIT before successful creation. Imports remain a separate immutable path.

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
