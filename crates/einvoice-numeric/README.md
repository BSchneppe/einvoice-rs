# einvoice-numeric

Internal, unpublished exact signed integer and decimal primitives for the invoice
builder and future evaluator. Rust 1.83 is supported. The repository MIT license
applies. No Java or network access is needed to build or test this library.

`Decimal` stores a normalized arbitrary precision integer coefficient times
10 to the negative signed scale. `19`, `19.0`, and `019.00` compare equal; `300`
has normalized scale -2. All zeros are equal with scale zero. Source spelling
belongs in the consuming source model. There is no binary floating point, fixed
precision decimal context, parser fallback, or implicit rounding.

Parsing accepts ASCII digits, an optional sign, and (for decimals) one optional
decimal point with at least one digit overall. Only XML space, tab, CR and LF
are allowed at the edges. Integer parsing rejects decimal points. Exponents,
NaN, infinity, underscores, Unicode digits and NBSP are rejected.

Exact addition, subtraction, multiplication, comparison, truncating integer
quotient and signed remainder use the same representation. Integer quotients
always satisfy `is_integral()`. `quantize` and `rounded_divide` require a signed
output scale and an explicit `RoundingMode`. HalfTowardPositiveInfinity means
nearest with ties toward positive infinity; it is not ceiling. Division has no
default scale or mode. Runtime types, IEEE behavior, cast/error mapping and Saxon
division policy belong to the evaluator. For example, the pinned Saxon policy's
`max(18, a.normalized_scale() - b.normalized_scale() + 18)` must be computed with
checked arithmetic by that caller, then passed with HalfTowardZero. Builder
line, tax, currency, base-quantity and expected-amount policies also stay outside.
A builder can multiply quantity and price exactly, then divide once at its chosen
scale, compare the expected amount exactly, group normalized equal rates, and
render two places after that explicit rounding.

```rust
use einvoice_numeric::{Budget, Decimal, Limits, RoundingMode};
# fn main() -> einvoice_numeric::Result<()> {
let limits = Limits::new(4096, 65536, 8192, 16384, 100_000_000)?;
let mut budget = Budget::new(limits); // reuse throughout one calculation scope
let quantity = Decimal::parse_decimal("3", &mut budget)?;
let price = Decimal::parse_decimal("0.335", &mut budget)?;
let base = Decimal::parse_decimal("2", &mut budget)?;
let product = quantity.mul(&price, &mut budget)?;
let amount = product.rounded_divide(
    &base, 2, RoundingMode::HalfAwayFromZero, &mut budget,
)?;
assert_eq!(amount.to_fixed(2, &mut budget)?, "0.50");
assert!(budget.remaining_work() < 100_000_000);
# Ok(())
# }
```

`to_canonical` renders the exact plain decimal with no rounding or exponent
notation. `to_fixed` pads exact values but returns `InexactRendering` if rounding
would be needed. At a negative fixed scale it requires a multiple of the
corresponding power of ten and renders a plain integer. Values remain normalized
after quantizing; the requested output padding is supplied at rendering time.
There is no allocating `Display`, `Ord`, arithmetic operator, or public BigInt
conversion. `Debug` shows only metadata. Clone, negation and absolute value share
an immutable `Arc` coefficient without allocating or copying its digits. Equality
compares normalized coefficients without allocation; like any large-value
equality it can scan operand storage. Use budgeted `compare` for metered ordering.

## Resource and failure contract

Every allocating numeric entry point takes a mutable caller-scoped `Budget`.
`Limits::new` requires nonzero, platform-safe limits for input bytes, coefficient
bits, absolute signed scale, output bytes and cumulative work. No sentinel disables
checking. Values reused under a tighter budget are checked again. Input bytes
are checked before lexical scanning; invalid text and resource refusal are
separate errors. Divide-by-zero and inexact rendering are separate typed errors
too. A consumer must propagate `Error::Resource` as processing failure, never
convert it to lexical invalidity, zero, a failed cast, or an invoice rule result.

Preflight bounds use checked integer arithmetic: four bits per significant
decimal digit or power-of-ten exponent (plus a carry), summed multiplication
bit bounds, maximum operand bits plus an addition carry, and a conservative
integer decimal-digit bound for formatting. These bounds apply to mathematical
coefficients, powers and aligned temporaries before allocation, including a
rounded quotient's possible carry. Parsing builds the coefficient monotonically;
formatting divides a checked copy into decimal digits. Both avoid the substrate's
radix conversion routines and their floating-point allocation estimates.
Lexical and normalized scales are checked;
normalization can fail if removing a trailing zero exceeds the signed scale
limit. Zero and unchanged quantization avoid unnecessary power allocation.
Conservative estimates can refuse an operation whose eventual normalized result
would fit, including cancellation, exact divisions or tight output capacities.
This is deliberate; limits are not a promise to accept every result below a cap.

Work is cumulative abstract fuel weighted by operand size: lexical bytes,
quadratic bit bounds for parsing, multiplication, powers, division and formatting,
linear bit charges for scans, clones and each normalization iteration, and output
bytes. Multi-step operations charge their steps before each allocation, so an
error can consume work. Exhausted fuel stays exhausted. These estimates are a
deterministic conservative accounting model, not a formal instruction count,
allocator quota, wall-clock deadline or cancellation mechanism. num-bigint's
private limb capacity and algorithm scratch use additional storage proportional
to the bounded operands; allocator overhead and allocation failure are not caught.
No hard RSS bound is claimed. A caller must separately bound live retained values,
sequence/document size, concurrent budgets, worker lifetime and cancellation.
Cloning retains shared storage and does not account whole-worker retention.

The primitive reference fixture records its inputs, explicit division scales and
rounding modes, authored XPath expressions and pinned reference provenance in
`tests/fixtures/README.md`. It covers selected primitives, not typed XPath
execution or invoice acceptance. No frozen oracle data is changed.
