# Selected expression-machine development evidence

`queries.tsv` contains 63 independently authored XPath2 inputs: ID and query UTF-8
hex. `select-bindings.tsv` contains seven independently authored standalone XSLT2
select/global/local contexts: ID and stylesheet UTF-8 hex. `document.xml` is the
original tiny source used by both. None is an invoice, held-out label, compiler
payload, frozen evaluator artifact or production parser input.

`observations.tsv` holds 98 fresh pinned item/text/error/empty rows. Successful
items retain primitive QName and exact value as UTF-8 hex, including empty string
versus empty sequence. XSLT text projection retains the explicit value-of separator.
Error rows retain XPath QName local part; the namespace is the standard XQT errors
namespace. Static/evaluate phase and full messages remain in the raw replay logs.

`provenance.json` retains the independent prior authored-input identities and
identifies supplemental inputs authored for this machine. `reference-identities.json`
pins final inputs and replay sources/runtime. Native tests build the corresponding
original AST forms explicitly in `machine_cases.rs` and the selected declarations
in `machine_runtime.rs`. They do not parse these queries or construct helper
bodies from strings. The comparison intentionally does not claim to reproduce
Saxon's static optimizer: some cardinality errors occur statically in reference
and dynamically at machine demand.

Run the clean-checkout commands in [MACHINE.md](../../MACHINE.md). The optional
reference replay checks the existing expected observations and writes only to
its output directory. It needs the pinned JAR, Java and Python; native tests do
not. Original scalar fixtures/replay live separately and remain byte-identical.
