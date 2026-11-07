# xsd-schema

`xsd-schema` is a Rust XML Schema validator for XSD 1.0 and 1.1 with a full post-schema-validation infoset (PSVI). Its push-based API integrates with event-based parsers such as `quick-xml` and DOM-style sources such as `roxmltree`, and a built-in XPath 2.0 engine adapts to any DOM through the `DomNavigator` trait.

## Documentation

| Document | Description |
| --- | --- |
| [What's New in 0.3](doc/WHATS_NEW.md) | A short overview of what 0.3.0 brings: new capabilities, correctness and conformance, safety and limits, performance, and the breaking changes. |
| [Introduction](doc/INTRODUCTION.md) | Public API overview, feature sets, schema loading, validation flow, XPath entry points, and async loading notes. |
| [Architecture Overview](doc/OVERVIEW.md) | Crate structure, pipeline diagram, module map, key abstractions, milestone history, and build reference. |
| [Extensibility Guide](doc/EXTENSIBILITY.md) | Extension points for annotations/appinfo, schema loaders, DOM navigation, and custom XPath functions. |
| [Unsafe Code](doc/UNSAFE.md) | Inventory of unsafe blocks, safety invariants, and Miri verification commands. |
| [Composing XML from Rust](doc/COMPOSE.md) | XPath 2.0 with Rust values bound as `$variables`, the Lisp-style `form!` constructor, iterator pipelines for the FLWOR clauses, and serialization. |
| [Implementation Limits](doc/IMPLEMENTATION_LIMITS.md) | Where values are bounded and what happens at the bound: `xs:decimal` beyond `rust_decimal` — exact validation, the typed value, the `xs-decimal-capacity` warning, XPath behaviour; regular expressions nested more than 64 levels deep. |
| [Upgrading to 0.3](doc/UPGRADING_0_3.md) | Every breaking change with the 0.2.x and the 0.3 code, then the verdict changes users will see. |
| [Changelog](CHANGELOG.md) | Release notes for every version, with upgrade notes for breaking releases. |

To see exactly what the validator compiled for a complex type — the authored
particle tree, the automaton it became, counters, and where each state comes
from in the schema — use the content-model inspector:
[Inspecting a compiled content model](doc/INTRODUCTION.md#7-inspecting-a-compiled-content-model).

To build XML from query results in Rust — XPath 2.0 with Rust values bound as
`$variables`, iterator pipelines for the FLWOR clauses, and a Lisp-style
constructor — use the composition layer:
[Composing XML from Rust](doc/COMPOSE.md).

## Test Results

| Suite | Command | Total | Passed | Failed | Skipped | Errors | Pass rate |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| W3C XSD 1.0  | `cargo test --test conformance --features xsd11 --release -- --test-suite ../../xsdtests --version 1.0` | 39,510 | 39,461 | 15 | 34 | 0 | 99.96% |
| W3C XSD 1.1  | `cargo test --test conformance --features xsd11 --release -- --test-suite ../../xsdtests --version 1.1` | 2,319 | 2,313 | 6 | 0 | 0 | 99.7% |
| XQTS XPath 2.0 | `cargo test --test xqts_xpath --features xsd11 --release -- -s ../../XQTS_1_0_2 --all -v -f` | 8,047 | 8,047 | 0 | 0 | 0 | 100.0% |

All 15 remaining XSD 1.0 failures and all 6 XSD 1.1 failures are documented
disputes: W3C-queried tests (Bugzilla 4146/4680/4957/6901/29085), tests that
contradict other accepted tests in the same suite (the `elemM002` /
`xsd015.e` family vs. Saxon's `Missing` group), or documented waivers.

Beyond the W3C suites, two corpora of published schemas are run locally (their
files are not part of this repository):

| Corpus | Files | Load (XSD 1.0) | Load (XSD 1.1) | Not loaded, each for a defect of its own |
| --- | ---: | ---: | ---: | --- |
| GAEB DA XML 3.3 (2021-05), the German construction data-exchange standard | 32 schemas | 32 | 32 | none |
| WSDL 1.1 services from 18 public sources (W3C Databinding patterns, Apache CXF, python-zeep, PHP SOAP interop, .NET WCF, ONVIF, IHE ITI, VMware vSphere, EU VIES, and vendor and government services) | 517 WSDL files | 459 | 482 | 35: 24 invalid schemas (each confirmed with Xerces-J), 7 broken references to other files, 3 not well-formed XML, 1 needs a schema the corpus does not hold |

The 23 WSDL files that load only in XSD 1.1 mode (ONVIF) violate XSD 1.0's Unique
Particle Attribution constraint (`cos-nonambig`), which XSD 1.1 relaxes.

## Regular-expression engine

With the `xsd11` feature, `xs:pattern` facets and the XPath functions `fn:matches`,
`fn:replace` and `fn:tokenize` use [`regexml-fork`](https://crates.io/crates/regexml-fork),
a maintained fork of [`regexml`](https://crates.io/crates/regexml) published as its own
crate under the same licenses (`Apache-2.0 OR MPL-2.0`); its
[README](https://github.com/semyonc/regexml#readme) explains why. The fork fixes engine
defects found while testing against the W3C suites — captured groups after backtracking,
counted repetitions of expressions that can match the empty string, searches that hung or
took exponential time — and adds whole-string matching, the XPath 2.0 dialect and
structured error kinds, so each pattern is compiled once and `^` and `$` are ordinary
characters as XML Schema defines them. The dependency is renamed in `Cargo.toml`
(`regexml = { package = "regexml-fork", … }`); nothing changes for code that uses
`xsd-schema`. Without the `xsd11` feature, patterns are translated for the `regex` crate.

## Benchmark

Instance-validation throughput and memory for `xsd-schema`'s three ingestion
strategies, over a synthetic dataset ≈ 15.56 MB — validated against its XSD. Each
strategy runs in its own subprocess (so RSS deltas are clean), timing is averaged
over 10 iterations, and the schema is compiled **once, off the clock**.

- **streaming** — push-based, no DOM (`drive_quick_xml` + `SchemaValidator`)
- **DOM (roxmltree)** — third-party tree via the `DomNavigator` trait
- **DOM (BufferDoc)** — built-in compact 16-byte-node document

| Strategy | Parser | Time | Throughput | RSS delta |
| --- | --- | ---: | ---: | ---: | 
| streaming | quick-xml | 322 ms | 48.3 MB/s | **616 KB** |
| DOM (roxmltree) | roxmltree | 279 ms | 55.9 MB/s | 88.4 MB |
| DOM (BufferDoc) | quick-xml | 357 ms | 43.6 MB/s | 61.6 MB |

Re-measured 2026-10-29 for 0.3.0 with Rust 1.99.1 (eight runs per strategy,
medians, spread under 3.5 %); the previous table (2026-09-11) read
308 / 265 / 339 ms. On the same machine and toolchain 0.2.3 measures
296 / 252 / 325 ms: the exact value-space facet checks of 0.3.0 cost about
9–11 % on this facet-heavy corpus, which a later release is to win back.



# AI Disclosure

This project was generated with AI as an experiment.
The generated code and content were reviewed and refined by the author.
Use of this repository is governed by its license, including any production use.
This notice is provided for transparency.

## Source Provenance

No third-party source repositories were used as rewrite sources for this codebase, with the sole exception of the author's own prior work:

- [semyonc/xpath2](https://github.com/semyonc/xpath2) — C# XPath 2.0 implementation
- WmHelp XmlPad — earlier Delphi tool by the same author together with Edward Aponasko and Alex Pospelov

A small number of Microsoft .NET API shapes are mirrored where they map naturally onto the data model, but no Microsoft source code was ported or rewritten. 