# XQTS XPath 2.0 Conformance Test Driver

This test driver validates the Rust XPath 2.0 implementation against the
[W3C XQuery Test Suite (XQTS) 1.0.2](http://www.w3.org/XML/Query/test-suite/).
It reuses the official W3C test cases — queries, source documents, and expected
outputs — adapting them from full XQuery syntax to standalone XPath expressions.

The driver is ported from the original C# test harness in `xpath2/XPath20Api/XPath20Api`
(see `Form1.cs`).

## Usage

```bash
# Run all XPath 2.0 applicable tests
cargo test --test xqts_xpath -- -s /path/to/XQTS_1_0_2 --all -v

# Run a specific test group
cargo test --test xqts_xpath -- -s /path/to/XQTS_1_0_2 -g MinimalConformance -v

# Run a single test
cargo test --test xqts_xpath -- -s /path/to/XQTS_1_0_2 -t fn-concat-1

# List available test groups
cargo test --test xqts_xpath -- -s /path/to/XQTS_1_0_2 -l

# Show only failed tests
cargo test --test xqts_xpath -- -s /path/to/XQTS_1_0_2 --all -f
```

The `--trace` flag enables `fn:trace()` output to stderr.

## Source Files

| File | Purpose |
|------|---------|
| `driver.rs` | Main test harness: CLI, execution loop, namespace context, variable binding |
| `catalog.rs` | Parses `XQTSCatalog.xml` into data structures (`CatalogConfig`, `XqtsTestCase`, `TestGroup`) |
| `filter.rs` | Selects which tests to run: group/test lookup, XPath 2.0 subset filtering, ignore lists |
| `prepare.rs` | Preprocesses XQuery files to extract bare XPath expressions |
| `compare.rs` | Serializes XPath results to XML and compares against expected output files |

## How Tests Are Adapted from XQuery

XQTS test cases are written as full XQuery programs. Since this project implements
XPath 2.0 (a subset of XQuery), each query file must be preprocessed into a plain
XPath expression before compilation.

### Input Preprocessing (`prepare.rs`)

The function `prepare_query_text` (port of C# `Form1.cs:625-641`) performs the
following transformations:

1. **Strip the Kelvin sign marker** — removes the `(: Kelvin sign :)` comment
   (XQTS metadata artifact).
2. **Skip past comments** — discards everything up to and including the last
   `:)` comment close.
3. **Extract braced content** — extracts the text between `{` and `}`, which is
   the actual XPath expression inside an XQuery enclosed expression. The brace
   search respects string literals (single- and double-quoted) so that braces
   inside XPath strings are not mistaken for delimiters.
4. **Trim whitespace**.

Example:

```
Input:   (: test for fn:concat :)
         { concat('a', 'b') }

Output:  concat('a', 'b')
```

### Variable Binding and Context Setup (`driver.rs`)

Each test case in the XQTS catalog can declare:

- **`<input-file variable="..." >`** — binds an XML document to an XPath
  variable. The source document is parsed with `roxmltree` and wrapped in
  a `RoXmlNavigator` node.
- **`<input-URI variable="...">`** — binds a URI string to an XPath variable.
- **`<contextItem>`** — sets the context node (`.`) for evaluation.

The driver also registers standard namespace prefixes (`xs`, `xsi`, `fn`,
`local`) and XQTS-specific prefixes (`foo`, `FOO`, `atomic`) that the test
queries expect.

### Test Filtering (`filter.rs`)

Not every XQTS test applies to a pure XPath 2.0 implementation. The function
`collect_xpath2_tests` selects the relevant subset:

1. Start with all tests from the **MinimalConformance** group where
   `is-XPath2` is not `false`.
2. Remove tests from excluded groups (`QuantExprWith`, `XQueryComment`,
   `Surrogates`, `SeqIDFunc`, `SeqCollectionFunc`, `SeqDocFunc`,
   `StaticBaseURIFunc`).
3. Add all tests from the **FullAxis** group.
4. Remove individually ignored tests (tests that require XQuery-only features
   or known unsupported edge cases).
5. Deduplicate by test name.

## Result Comparison

Each XQTS test case specifies one or more **output files** with a `compare`
attribute that controls how the actual result is matched against the expected
output.

### Test Scenarios

Every test case has a `scenario` attribute:

| Scenario | Expected behaviour |
|----------|-------------------|
| `Standard` | Expression compiles and evaluates; result is compared against output file(s) |
| `ParseError` | Compilation must fail. Pass if a compile error occurs, fail if compilation succeeds |
| `RuntimeError` | Evaluation must fail. Pass if an error occurs; some tests also accept matching output as an alternative pass condition |

If the test declares `<expected-error>` codes and compilation or evaluation
fails, the test passes regardless of scenario.

### Compare Modes

Each `<output-file>` element carries a `compare` attribute:

| Mode | Behaviour |
|------|-----------|
| `XML` | Semantic XML comparison via `TreeComparer` with `ignore_whitespace=true` |
| `Text` | Expected content is wrapped in `<root>...</root>` and then compared as XML |
| `Fragment` | Treated identically to `Text` |
| `Inspect` | Automatic pass — the test requires manual inspection |
| `Ignore` | Output file is skipped (not used for comparison) |

### Multiple Output Variants

A single test case can have **multiple output files** (alternative acceptable
results). The comparison is **disjunctive**: the test passes if **any** output
file matches. It fails only when **all** output files mismatch. If no output
files are specified at all, the test passes automatically.

### Serialization and Comparison Pipeline (`compare.rs`)

The comparison algorithm (port of C# `CompareResult`, `Form1.cs:827-920`):

1. **Load expected output** and parse it as XML. For `Text`/`Fragment` mode the
   expected content is first wrapped: `<?xml version='1.0'?><root>CONTENT</root>`.

2. **Extract wrapper element name** from the expected output's root element
   (including its namespace declarations). This name is used to wrap the actual
   result so that both documents share the same root structure.

3. **Serialize the XPath result** to an XML string. The wrapping decision is:

   ```
   wrap = !((xml_compare && is_single) || is_exception)
   ```

   - **Wrapped**: result items are placed inside
     `<?xml version='1.0'?><element>...</element>`, where `element` is the
     expected root's qualified name. Attribute items become attributes on the
     wrapper element; all other items become children.
   - **Non-wrapped**: used for single-node results or exception tests. Nodes
     are serialized directly without an enclosing element.

4. **Tree comparison**: both the expected and actual XML are parsed into
   `RoXmlNavigator` trees and compared with `TreeComparer` (whitespace-ignoring
   deep equality).

5. **Atomic fallback**: if the result is a bare atomic value (e.g. `"false"`,
   `42`) that does not parse as XML, the driver extracts the deep text content
   from the expected XML tree and compares it as a plain string.

### Hardcoded Overrides

A small number of tests require special handling:

- **`FORCE_XML_COMPARE`** — forces XML comparison mode even when catalog says
  otherwise (e.g. `ReturnExpr010`).
- **`FORCE_NOT_SINGLE`** — overrides `is_single` to `false` so the result gets
  wrapped (e.g. `CondExpr012`, `NodeTest006`).
- **`IS_EXCEPTION`** — disables wrapping for specific tests (e.g. several
  `fn-union-node-args-*` tests).

### Debug Output

In verbose mode (`-v`), when a test fails the driver re-runs the comparison with
debug output, printing `ACTUAL` vs `EXPECTED` XML to stderr for the first
non-Inspect/Ignore output variant.
