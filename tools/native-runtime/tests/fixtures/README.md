# Scalar development evidence

These are independently authored development inputs and observations, with exact
lexical bytes, primitive type QNames, singleton cardinality, error QNames and XPath
expression evidence. No invoice, held-out label or frozen evaluator artifact is
an input. Native tests call typed atomic primitives; they do not parse or execute
the retained XPath expressions. They demonstrate scalar operations only.

There are 725 stored rows (the original 495 remain byte-identical):

- `development.tsv`: 162 rows = 70 applicable original numeric111 rows (54 exact
  primitives, omitting three half-even rows outside this API, plus 16 scalar/type
  cases) + seven applicable typed280 rows + 85 additive boundary observations.
  This includes all three historical panic examples: integer/decimal round at
  precision -10 and the 30-digit integer literal division.
- `rendering.tsv`: 228 independent binary rendering inputs: 96 seeded bit patterns
  per binary width plus 18 power-of-ten inputs per width; seed `0xB5C8001`.
- `boundaries.tsv`: one discriminating float `idiv` example plus 104 independently
  authored binary-to-integer and division observations, including 48 large double
  and 24 large float bit patterns from seed `0xB5C8CA57`, signed-64 boundaries,
  non-finite faults, large integer conversion and float quotient overflow.

- `corrections.tsv`: 134 fresh review-correction observations covering signed
  double power-of-two boundaries and adjacent values, float exponent boundaries,
  typed string casts and castability, C0 edge/interior controls and the pinned
  short/long Float/Double grammar distinction. Expected results come solely from
  the pinned reference; source expressions replay without scratch evidence.

- `subnormal.tsv`: 96 fresh reference routes over 48 signed float/double
  identities: minimum values, the observed decimal-decade mismatches and their
  neighbors, a candidate that rounds across a decade, and the largest-subnormal /
  smallest-normal boundary. Both render and String casts are retained. The
  reference independently checked the input bits; no native output generated
  expectations. This is bounded development coverage, not exhaustive formatting.

Columns are UTF-8 TSV: ID, source family, primitive route, a type, a UTF-8 hex,
b type, b UTF-8 hex, route argument (cast target or signed round precision),
reference status, exact primitive type or error QName, exact value UTF-8 hex,
original expression UTF-8 hex. Numeric operands are never stored as JSON numbers.
Every successful reference row is one atomic item; native tests check its type as
well as exact string value. The route is independently specified test glue, not
an XPath parser, evaluator, AST substitute or production dispatch language.

Original immutable input identities:

- numeric cases: `0c9485de37ba60db43a9ca7a594c95ddfdd0dd81b426e7d0c12b71947c67bd67`
- numeric results: `e621b68975b00208ff3837ddbf4fa79ac81f1c681ef06794d47c49882e2a5e9a`
- typed cases: `b3f5f54696ca5c9fbdd94d190124c4cce249eba9bf80d605dbe813af9d189b17`
- typed results: `935699076f18f58fb48bbb05ece3087da4a13c2fc74b1260484b9c299a20f747`

`provenance.json` records additive source/fixture SHA-256 identities. The original
111/280 observations and the merged kernel's 57-row fixture are unchanged. The
new matrix is not a claim to have ported all 111/280 expression semantics. Nodes,
sequences, lazy errors and function-call/cardinality cases remain OPEN.

Reference executable: KoSIT validator 1.6.3 standalone JAR, SHA-256
`799e64befca97d4080e03608c80b85dd5a5ecc5f4ae4f35d1116ec2855b9a7c9`, reporting
Saxon HE 12.9. Runtime: Amazon Corretto 27.0.1.0.11, Java runtime
`27.0.1+11-FR`, vendor `Amazon.com Inc.`. Original numeric and new additive rows
use XPath 3.1; the seven original typed rows retain XPath 2.0. XPath version here
records the independent development reference, not a runtime parser/capability
claim. Initial additive XPath2 probing retained XPST0017 for two-argument round;
the final additive 3.1 run records that function's scalar result separately. No
original pin or prior observation was overwritten to obtain a passing test.

From a clean checkout, obtain the pinned local JAR/runtime as described in the
repository's oracle README, and replay these committed scalar expressions without
any scratch files, Rust executable, network, source invoice or expected-data writes:

```sh
ATOMIC_JAVA=/path/to/corretto-27.0.1.0.11/bin/java
ATOMIC_KOSIT_JAR=/path/to/validationtool-1.6.3-standalone.jar
"$ATOMIC_JAVA" --class-path "$ATOMIC_KOSIT_JAR" \
  tools/native-runtime/tools/ReplayValues.java \
  tools/native-runtime/tests/fixtures/development.tsv \
  tools/native-runtime/tests/fixtures/rendering.tsv \
  tools/native-runtime/tests/fixtures/boundaries.tsv \
  tools/native-runtime/tests/fixtures/corrections.tsv \
  tools/native-runtime/tests/fixtures/subnormal.tsv
```

The read-only verifier checks the loaded JAR SHA-256, Saxon version, Java
runtime/vendor, duplicate IDs, exactly 725 rows, successful singleton primitive
QNames and exact values, or exact error QNames. It never generates/replaces
expectations. A successful replay is development scalar evidence only.

Historical extraction (the scratch inputs need not exist for native tests/replay):

```sh
python3 tools/native-runtime/tools/extract_development.py \
  .scratch/bsc8/numeric .scratch/bsc8/evaluator-plan/probes \
  .scratch/bsc8/typed-runtime-implementation/fresh-results.json /tmp/development.tsv
cmp /tmp/development.tsv tools/native-runtime/tests/fixtures/development.tsv
```

Extraction verifies immutable original pins and the separately recorded additive
result pin; expected outputs are copied only from reference observations, never
calculated by the native implementation. Complete fresh error messages/phases,
commands and additive probe inputs remain in the authorized implementation scratch
report. Clean-checkout replay independently checks the committed outcomes.
