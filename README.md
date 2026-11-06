# einvoice-rs

An experimental native Rust library for European e-invoices. The workspace currently contains `crates/einvoice-core`, with typed CII and UBL XML parsing and serialization.

> [!CAUTION]
> Not ready for production use. Parsing does not establish invoice conformance. The incomplete business-rule validators (mostly stubs, with some real checks) and UniFFI/Kotlin bindings have been removed. The workspace crate is not publishable yet.

## Direction

The first release targets XRechnung CII XML: complete in-process validation against a pinned ruleset, immutable received documents, and a limited builder for ordinary VAT invoices. PDF/ZUGFeRD containers, language bindings and additional formats come later.

The current plan and backlog live in [Linear](https://linear.app/bschneppe/project/rust-e-invoice-library-0bad68cfeaeb). The existing schema-shaped models are experimental. The public business API, validator, builder and conversion support are separate follow-up work.

## Workspace

- `crates/einvoice-core`: existing XML models, parser, serialization.

## Parsing example

```rust
use einvoice_core::parse_invoice;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let xml = std::fs::read_to_string("invoice.xml")?;
    let _parsed = parse_invoice(&xml)?;
    // Parsing succeeded; schema and business-rule conformance remain unchecked.
    Ok(())
}
```

Runnable examples live in [crates/einvoice-core/examples](crates/einvoice-core/examples):

```sh
cargo run -p einvoice-core --example parse_cii
cargo run -p einvoice-core --example parse_ubl
```

## Checks

```sh
cargo test --workspace --all-features --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
```

The existing fixture tests exercise parsing and normalized serialization. They are not schema/business-rule conformance tests and do not prove byte-identical round trips. The reference-rule fixture preserves a small set of historical checks for later implementation.

## License

[MIT](LICENSE).
