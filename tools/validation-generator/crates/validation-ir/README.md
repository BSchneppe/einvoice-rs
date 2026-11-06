# validation-ir: inspection contract v1

This unpublished, serde-only crate exposes backend-independent data. It has no
parser, resource loader, engine dependency, execution entry point or capability
claim constructor. It can be a path dependency of the BSC-8 owned runtime.

`Package.format` is `einvoice-validation-ir/1`. Select both `target` and
`source_package` exactly; `select` rejects unknown/duplicate identities without a
latest/library-version fallback. Fixture identities coexist without adding a
production profile. BackendState currently has only `Unavailable`.

- `Schema.patterns`, `Pattern.rules`, `Rule.sites` keep source order. Dispatch is
  document traversal, first matching rule per node **per pattern**; later patterns
  remain independent. A context is a match pattern, not a document-root selection.
  Phases supply activation membership, without reordering patterns. Explicit
  selection is independent of the retained declared default phase; inactive source
  remains parsed and checked.
- A site or source binding is identified by `(source_package, identity)`, where
  identity is `archive/member@raw-XML-element-byte-start`. Expression binder IDs
  extend the containing attribute source identity with decoded-expression position
  and qualified name. Resource hashes tie identities to exact source bytes.
- `Name` stores namespace/local plus original qualified spelling. Preserve original
  **invoice** element/attribute QNames in the runtime DOM too: `name()` is
  prefix-sensitive. Do not replace it with `local-name()`. Node-test default
  namespace, function default namespace and XML element namespaces are distinct.
- `Expression` stores decoded text, raw XML `Location` and a real recursive `Expr`.
  Numeric kinds/lexemes never require machine-number conversion. Signs are unary
  operations. `Step`, `/`, `//`, filters and function steps remain distinct AST
  operations. `Group` preserves explicit parentheses, including nested groups.
  A `Filter` rooted directly in a `Step` (possibly through earlier `Filter`s)
  uses that axis's proximity order. A `Group` ends that axis-predicate chain:
  a filter on its result uses sequence order. Thus `ancestor::*[1]` and
  `(ancestor::*)[1]` are distinct trees; runtime must not discard this boundary
  before determining predicate semantics. `text()` stays a child text-node test: the exact scenario predicate
  `/ID/text() = '...'` is existential node comparison, not element string-value
  comparison. Comments can split text nodes. Duplicate profile elements may match
  selection before XSD rejection; the frontend does not execute or simplify it.
- `Binding.id` is the resolved variable identity, independent of qualified aliases.
  Pattern/schema lets initialize at document root, including inactive patterns'
  global declarations. Rule/helper locals and expression bindings shadow outer
  IDs lexically. A new binding's initializer cannot see its own new ID. Helpers
  retain parameter types (including omission), return types and sequence-constructor
  trees. Untyped content bindings are temporary-document constructors in xslt2;
  typed content and select bindings have different semantics. Recursive helper
  calls are represented, while global initialization cycles are rejected.
- Messages are ordered text/name/value-of parts with exact significant text/tails.
  Preserve source flags, report base severity and override-derived effective
  severity independently. Base stage validity rejects warnings/errors; assessment
  checks effective errors, processing errors, unmatched/fallback scenarios.
  PROCESSING_ERROR cannot be downgraded. Missing SVRL is a processing fault, not
  empty success. Policy clause evidence names exact resources/hashes/locators.
- `resources` names exact retained bytes/roles/pin authority; `source_graph` retains
  includes, schema namespace imports, opaque report imports and engine resource
  loads. Opaque report/HTML/class resources are not executable native instructions.
  XSD is retained as source plus feature requirements, not compiled schema IR.
- `requirements` is an ordered set of actual AST kinds, functions with expanded
  name/arity, types/occurrences, operators, axes, numeric kinds, all helper
  declarations, policy clause kinds, XSD element/attribute constructs, and shared
  dispatch/binding/failure semantics. These are requirements, not supported flags.
  `node-test-kind` entries distinguish `node`, `text`, `comment` and
  `processing-instruction`, independently of literal helper text instructions.

`manifest.ir_sha256` hashes exact canonical `ir.json` bytes (pretty JSON plus final
LF), including target, source identity, trees and requirements. A future executable
package must bind that digest, this format, backend revision and lowering/runtime
test evidence for every requirement and complete source closure. Evidence must
come from the actual backend/lowering implementation and its tested construct
matrix; it cannot be manufactured by passing a supported Boolean or arbitrary
caller callback. Current `runnable` always rejects and exposes no evidence API.
Backend/lowering integration can extend this contract under review; changing AST
or policy semantics requires a new format instead of silently reinterpreting v1.

`resolved` adds a separate `einvoice-resolved-candidate/1` serde envelope. Raw
Package serialization stays unchanged. The envelope contains one materialized
effective Package and preserves the original Site/test/context, effective test,
complete duplicate occurrences, source selectors/ranges/attribute and ancestry
hashes, namespace map, decision reason/authority and comparison-only lineage.
It binds raw IR and fixed decision-manifest digests. Additional source closure and
edges are explicit. Select exact candidate **content identity** in addition to
target/source-package; the raw `select` helper does not admit candidates or choose
between raw and effective generations.

This is non-executable inspection data. Deserializing a candidate does not prove
its source/AST pairing, its format string, compiler validation or execution
eligibility. Only the fixed generator constructs its private compiler-owned
emission object; there is no public candidate admission verifier in this slice.
The only audit state is pending full predicate/context/helper/message/policy
correspondence, and backend/policy execution remain Unavailable. One local
source-origin decision cannot certify unknown configured-target differences.

The pinned backend profile also needs explicit focus, demand and error-boundary
tests. A helper binding's `lexical-dynamic-context` does not inherit the caller's
matched-node focus: stylesheet function entry has absent focus. Globals use
document-root focus when demanded. Unused typed arguments/globals and consumers
of a sequence prefix must not be assumed eager. At the pinned match-pattern
boundary, recoverable XPath dynamic/type failures become nonmatches with warnings;
the same faults in site evaluation propagate. Fatal resource, capability,
cancellation and internal failures must never enter that recovery path. These are
backend obligations, not behavior executed or certified by this source resolver.
Full regex/casing Unicode semantics and their table identities likewise belong in
the backend's exact-package evidence. None changes the raw AST or grants execution
eligibility to this candidate.
