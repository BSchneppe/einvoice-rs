# Private bounded XPath expression runtime

Unpublished Rust 1.83 atomic infrastructure and a borrowed-AST expression machine
in their own workspace. This crate has no validator or package execution API.
See [MACHINE.md](MACHINE.md) for the low-level machine API, semantics, resources
and explicit capability gaps. The root workspace's
`crates/*` wildcard cannot discover `tools/native-runtime`. All acceptance gates
remain incomplete; parent integration and independent review are required.

`Atomic` privately stores Integer, Decimal, Float, Double, Boolean, String or
UntypedAtomic. Integer wraps the merged `einvoice-numeric::Decimal` with an integral
invariant, while Decimal retains its distinct type even when integral. Literal
construction accepts the merged `validation_ir::NumericKind` directly. No parser,
AST copy, compiler or second exact arithmetic substrate exists. The expression
machine consumes the merged immutable `einvoice-source` API.

```rust
use einvoice_native_runtime::{Arithmetic, AtomicType, Limits, NumericKind, Scope};
# fn main() -> einvoice_native_runtime::Result<()> {
// Example allowances, not release defaults. Numeric and runtime fuel are separate.
let limits = Limits::new(
    4096, 65536, 8192, 1_000_000_000, 1_000_000,
    1_000_000, 1_000_000, 128, 1_000_000,
)?;
let mut scope = Scope::new(limits);
let amount = scope.literal(&NumericKind::Decimal, "9007199254740993.01")?;
let fraction = scope.lexical(AtomicType::Decimal, "0.01")?;
let result = scope.arithmetic(Arithmetic::Subtract, &amount, &fraction)?;
assert_eq!(result.atomic_type(), AtomicType::Decimal);
assert_eq!(scope.render(&result)?.as_str(), "9007199254740993");
# Ok(())
# }
```

The scalar API includes lexical/typed casts, singleton-operand castability,
`number(Some(atom))`/`number(None)`, arithmetic, numeric comparison, negation, abs,
round and explicit string rendering. `resolve_type` rejects every other requested
type, including derived types such as `xs:int`, as a capability failure.
`numeric_compare` requires numeric operands after the machine's comparison
conversion; it does not implement general/value/node comparison policies.
String/untyped lexical constructors preserve all bytes. Integer casts trim edge codepoints through U+0020. Decimal and Boolean casts
retain XML-only whitespace. The pinned Float/Double adapter admits edge C0
controls only on its short numeric fallback (fewer than nine original codepoints);
longer inputs and INF/NaN recognition retain XML-only edges. Strings do not become numeric
arithmetic operands implicitly; untypedAtomic arithmetic operands become Double.

Decimal/integer operations exclusively use the merged kernel. Pinned Saxon HE
12.9 `div` explicitly computes `max(18, a.normalized_scale() -
b.normalized_scale() + 18)` with checked signed arithmetic and HalfTowardZero.
`idiv` directly calls the exact truncating quotient; `mod` calls exact remainder.
XPath round uses HalfTowardPositiveInfinity at the caller's signed precision.
Binary round decomposes exact bits before quantizing, retaining negative zero.
The supplied `i64` precision is a scalar API bound; arbitrary integer argument
conversion/cardinality belong to the expression function layer.

Float/Double preserve IEEE zero signs, NaN, infinities, unordered comparisons,
underflow and overflow. Required promotion to binary is explicit. Decimal-only
money operations never use f64. Finite binary-to-decimal casts decompose
mantissa/exponent and use exact kernel powers/products/terminating division;
shortest-display-then-parse is never used for those casts.
Binary-to-integer instead follows Saxon's distinct profile: an inclusive signed-64
threshold and saturating cast, then nearest shortest round-trip decimal candidates
for larger integral binary values. Candidates are generated from exact bits with
the kernel, not parsed from a binary display spelling. The positive `2^63` boundary
thus becomes `2^63 - 1` in this pinned profile; `xs:decimal` retains exact `2^63`.
Float `idiv` overflow follows Saxon's decimal candidate fallback and exact quotient;
double `idiv` quotient overflow reports FOCA0002. Decimal-to-binary64
renders exact decimal within checked buffers and uses Rust 1.83's bounded parser.
Pinned Saxon lexical/decimal-to-float narrows through binary64, including the tested
double-rounding boundary. This is a pinned profile choice, rather than a claim
about every XPath implementation. Binary rendering uses pinned exponent-based symmetric margins for normal values,
strict interior candidates and nearest ties toward zero, two significant positions
in scientific form, exponent thresholds and signed zero. At powers of two these
margins can emit a spelling that parses to an adjacent IEEE value, matching the
pinned converter rather than claiming universal round-trip correctness. Subnormals follow the pinned JDK selection used by Saxon: the exact decimal
decade starts the closest one-or-two-digit search, then progressively longer
candidates, with even-significand ties and IEEE midpoint inclusion for even
binary significands. The selected subnormal candidate supplies its normalized
exponent. Exact text generation and scanning are charged before their work.
The separate integer conversion profile remains unchanged. Standard binary
formatting supplies only a bounded stack exponent seed for normal values. The tests include
Saxon's `0.9999999999999999E23`, `4.9E-324` and `1.4E-45` spellings. A bounded
candidate search that finds no result fails as a capability error; it never emits
an approximate success. These observations do not prove all IEEE formatting cases.

XPath faults retain a typed QName in `Error::XPath`: FORG0001, FOCA0002,
FOAR0001, FOAR0002 and XPTY0004, in the standard XQT error namespace. Resource,
unsupported capability, foreign-scope and internal failures use `Error::Fatal`.
`castable` catches only XPath faults from its own supplied-atom cast, and `number`
converts only its specified lexical/type faults to NaN. Neither catches fatal
failures. Operand evaluation errors cannot enter these APIs without a caller
explicitly discarding them; evaluation/cardinality are handled by the expression machine.

A `Scope` is non-cloneable, with no reset method. It owns one kernel Budget for
its whole lifetime, plus cumulative runtime fuel, conservative string allocation
and external rendering allowances, live value count and live storage slots.
Numeric fuel is reserved independently at scope creation and is never recreated
per operator. Failed checks retain prior charges; exhausted counters remain zero.
One failed allowance does not poison unrelated counters; callers must propagate a
fatal result. A new caller creates a new scope. Values from other scopes are
rejected, and retained atoms/texts keep their accounting ledger alive after the
scope is dropped. The current implementation is caller-local (`Rc`), not Send/Sync.
Worker scheduling/transfer is a parent decision.

Every allocation of a retained atom or rendered text reserves a live slot first.
A numeric slot is `ceil(numeric_bits/8) + 128` logical bytes; exact, binary and
Boolean atoms all conservatively use this full slot. String slots reserve UTF-8
length plus 128. Duplicating a handle charges another full slot even when immutable
storage is shared; dropping atoms/texts releases only live retention, never fuel
or cumulative bytes. There is no universal Atomic Clone/Eq/Ord/Hash/Display.
Debug shows metadata; borrowed rendered text and scalar bit/Boolean observations
inspect already retained data. Any caller-owned copies are outside this layer.

Kernel temporaries obey its own per-operation bit/scale/output preflight and
cumulative fuel. Because it hides coefficient size, decimal text reserves the
conservative `3*numeric_slot + abs(normalized_scale) + 4` bytes before allocation.
Every such buffer is charged, including conversion/rendering scratch; rendering
also precharges its external output bound. Binary rendering precharges a bounded
128-byte final buffer and charges its digit scratch separately. Scientific assembly
excludes padding before copying into a checked fixed-capacity sink; it cannot grow
a temporary string beyond the admitted bound and trim it afterwards. Bounds are not
refunded to actual length. Conservative refusal can therefore occur although the
eventual result is small. These policies need parent measurement; they are not
production defaults. Counters bound logical values/operations, not allocator limb
capacity, private numeric scratch, allocator metadata, RSS, wall time, cancellation
or out-of-memory behavior. A worker bound and containment remain open.

Build/test offline after populating an isolated cache from already cached,
checksum-matching archives (Python 3.12+):

```sh
python3 tools/native-runtime/tools/cache_dependencies.py
/bin/bash tools/native-runtime/tools/check.sh
```

The helper reads only this lock and cached archives and writes this crate's ignored
`.cache/`. It performs no downloads or registry mutation. The check script uses
Rust 1.83, `--locked --offline`, isolated Cargo home/target, own-workspace fmt,
all-target Clippy `-D warnings`, and docs `-D warnings`. Tests need no Java or
network. Fixture provenance and an optional read-only pinned Saxon replay are in
`tests/fixtures/README.md`.

OPEN: helper/Instruction execution and temporary constructors, remaining strings,
regex/casing data, match dispatch/messages, package capability closure, target
availability, source/schema/policy/report integration, independent correspondence,
coverage and release gates. These are explicit preparation failures or separate
unimplemented layers. The scalar fixture/replay artifacts remain byte-identical;
their historical README describes the scalar API's boundary. Machine evidence is
separate under `tests/evaluator-fixtures`. Neither suite establishes full XPath,
invoice acceptance, package conformance, performance or release readiness.
