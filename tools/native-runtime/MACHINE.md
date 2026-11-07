# Borrowed-AST expression machine

This isolated, unpublished Rust 1.83 crate now executes the chosen core expression
subset over the existing `validation_ir::Expr` and immutable `einvoice_source`
API. It does not load or promote a Package/ResolvedCandidate, select source
authority, dispatch rules, or report invoice validity. The resolved candidate's
pending correspondence/backend contract remains unchanged.

## API and identity

`machine::Prepared::new(&Expression, &[Binding], MachineLimits)` traverses the
entire borrowed tree iteratively, including dead branches and unused select
initializers. It admits constructs, expanded function names/arity, axes, operators,
atomic types/occurrences and resolved binding identities. The private index arena
retains each original `&Expr`, enclosing `&Expression`/source location, declaration
and binding slot. No parser, AST clone, serialized replacement IR, function body
string, custom matcher callback or caller-supported flag is involved.

`machine::Run::new` borrows that preparation and one `SourceDocument`. Its finite
allowances are caller supplied: atomic `Limits`, `MachineLimits`, source work and
source UTF-8 output. `EntryFocus` is document 1/1, one original source node 1/1, or
absent. There is no public atom/sequence injection into a run. `cursor()` creates
independent entry-sequence replays; `next(cursor)` demands one item. Copying a
cursor token aliases that cursor, while another `cursor()` replays independently.
`ItemHandle` can only be inspected through the issuing run: `item_ref` borrows the
original node or Scope-owned atom; `render` is metered. `error_origin` retains the
actual original construct/source for evaluation failures through terminal-error
replay. A new external observation or admission refusal has no AST origin and
clears earlier evaluation evidence; it never invents an item-producing construct.
Successful observations preserve earlier evaluation evidence. Replaying an already
latched evaluation fatal preserves that failure's original origin.

Ordinary nodes, attributes and namespace nodes use source-provided document-
qualified identities and order keys. An Arc alias preserves identity; a new parse
of identical XML has a different document identity. Foreign-document entry and
foreign-run cursors/items fail fatally. QName evidence comes from the original
source Name, including its lexical prefix. Source strings are not schema values
or reparsed copies. Elements/documents/text/attributes atomize as untypedAtomic;
comments/PIs/namespaces as String.

## Implemented semantics

All 15 Expr forms have execution semantics: Number, String, Variable, Context,
Root, Group, Step, Call, Unary, Binary, Filter, Sequence, If, Bind and TypeOp.

- Operators: numeric unary +/- and binary +, -, *, div, idiv, mod; arbitrary exact
  integer ranges; general =/!=/</<=/>/>=; singleton value eq/ne/lt/le/gt/ge;
  singleton node is/<</>>; lazy and/or; /, // and node union |; for/some/every;
  singleton/optional cast and castable.
- Builtins in the standard function namespace: exists, empty, count, boolean,
  not, data, abs, round and sum at arity 1; true, false, position and last at arity
  0; string and number at arity 0/1. XML Schema constructors at arity 1 cover
  Integer, Decimal, Float, Double, Boolean, String and UntypedAtomic. Other names,
  arities, operators, types and node tests are fatal preparation capabilities.
- Axes: child, attribute, namespace, parent, self, ancestor, ancestor-or-self,
  descendant, descendant-or-self, preceding-sibling and following-sibling.
  Name/Any/Namespace/Local tests and node/text/comment/processing-instruction
  kinds use the original source handles. Reverse-axis Step/Filter chains use
  proximity order; Group ends the chain and exposes ordinary sequence order.
- Sequence/for preserve duplicates. Paths normalize/deduplicate node input in
  document order and establish right-operand focus position/size. Node outputs
  are ordered/deduplicated; atomic outputs stream in tuple order. A demanded
  node/atomic mixture raises XPTY0018, and atomic left path input XPTY0019.
- Recipes cache only demanded prefixes and a terminal value/error. Binding uses
  have independent replay indices. Exists, empty, focus-independent numeric positional prefixes,
  and/or/if and quantifiers avoid undemanded error tails; count forces the tail.
  `last()` demands its captured sequence size, without recursive Rust evaluation.
  Literal impossible numeric predicates can finish without reading their base.
- Select bindings initialize lazily. Globals capture document root 1/1, even
  when first demanded inside a different focus; rule locals capture entry focus.
  Tuple initializers can see preceding tuple slots and results retain the outer
  focus. Lexical shadowing uses distinct resolved IDs. Globals can forward-
  reference globals but cannot capture rule locals; circular demand is XTDE0640.
- Atomic select declarations support singleton/?/*/+ cardinality and function
  conversion, including untyped atomization/casts, numeric subtyping/promotion,
  lazy typed-star prefixes and XTTE0570 mismatch. Casts, function conversion,
  EBV, predicate numeric positions and general/value/node comparison are distinct.
  Invalid lexical conversion owned by a declaration reports XTTE0570; an explicit
  failing cast inside its initializer retains FORG0001. Fatal failures remain fatal.
  Castable catches only its own cast faults/cardinality mismatch, never operand
  XPath errors, source-query refusals or Fatal failures.

The existing exact/IEEE/conversion algorithms are unchanged. `machine/atomic.rs`
adds metered EBV/comparison adapters using existing Scope casts, numeric comparison
and duplication; it implements no numeric algorithms. Every run owns exactly one
non-resettable Scope/kernel Budget. New fault/resource variants and `pub mod
machine` are the only changes to the original scalar source modules.

## Resource contract

Preparation and execution are explicit loops over flat arenas and bounded heap
continuations. AST ownership/deserialization and safe destruction of the caller's
owned AST are outside this borrowed API; the deep test dismantles its input
iteratively too. No recursive evaluator, eval-to-Vec, recursive sequence ownership
or Rc cycle exists. Numeric atoms retain the existing Scope leases. Recipes,
cache cells, cursor slots, environments, result vectors, axis/traversal storage,
frames, source text and vector capacity/copy growth are admitted before allocation.

Machine limits independently bound cumulative objects, conservative live storage
reservations, cumulative storage admission, frame depth and work. Geometric arena
capacity is precharged with its copying work. Retired capacity reservations remain
charged until run drop; some conservative storage charges outlive the actual
scratch buffer. These are logical upper reservations, not allocator/RSS metrics.
Released continuation/stack slots do not refund cumulative bytes or work.

One persistent QueryBudget is used for source string-value streaming. Adapter
preflight reserves whole-document node/byte work for each ordinary string query
and direct byte/work for attributes/namespaces. Output accounting spans all three
kinds. Source writer admission precedes each text-buffer growth. Axis visits and
name comparisons use machine work; namespace-frame lookup conservatively reserves
quadratic original-input work before entering the source namespace iterator.
Node-order sorting uses fallible, metered insertion comparisons. Numeric and
string comparisons remain metered even for replayed values.

Node axes and node path ordering may materialize bounded handle vectors. Atomic
paths stream once their atomic output kind is known; later mixed-output errors
are terminal and are visible when demanded. Full node ordering, union, `last`,
large range prefixes or general-comparison products may refuse conservative
allowances. There is no truncation or successful false/empty substitute. Fatal
failures poison all further mutating operations in the run, including a first
foreign-handle failure from read-only inspection. The first fatal and its evidence
remain latched; valid retained items can still be inspected as partial evidence.
A fresh run has fresh Scope,
source query budget, caches, environments and focus.

No limits here are production defaults or measured hard RSS/deadline/cancellation
containment. Source parser allocation is governed separately by einvoice-source.

## Evidence and clean replay

`MACHINE-SOURCES.json` pins the exact merged source/numeric/IR/local xml-rs and
unchanged scalar fixture/replay inputs. The isolated manifest adds the source
path dependency with `xml-rs = { path = "../../vendor/xml-rs" }`; its lock contains
no registry xml-rs fallback. Root Cargo/lock are not used by these commands.

From a clean checkout with Rust 1.83 and checksum-matching crate archives already
in the local Cargo registry cache:

```sh
/bin/bash tools/native-runtime/tools/check_machine.sh /tmp/einvoice-machine-check
```

The additive `cache_dependencies.py --cache-dir` argument directs all cache
outputs there; its original default invocation is unchanged. The check runs locked
offline tests, own-workspace fmt, all-target Clippy -Dwarnings, and docs -Dwarnings.
It does not build the root workspace or need Java/network/scratch fixtures.

Optional fresh development reference replay uses Python **3.12.6**, explicit
Corretto **27.0.1.0.11** at
`/Library/Java/JavaVirtualMachines/corretto-27.0.1.0.11/Contents/Home/bin/java`,
and `/tmp/einvoice-validator-1.6.3.jar`, SHA256
`799e64befca97d4080e03608c80b85dd5a5ecc5f4ae4f35d1116ec2855b9a7c9`,
asserting Saxon HE **12.9**:

```sh
python3.12 tools/native-runtime/tools/replay_machine.py --output /tmp/einvoice-machine-reference
```

Only committed queries/XML/Java are read; output and command/identity logs go to
the supplied directory. Expected files are never rewritten by replay. The 63
XPath inputs and seven standalone XSLT select-binding contexts were independently
authored before fresh observation: 70 inputs, 98 item/text/error/empty rows. Rust
tests construct exact ASTs/declarations directly, with no XPath parser. Source
query text is linked to the original Expression for evidence, never executed as
text. XSLT binding tests compare only the represented select semantics and the
value-of text projection; they do not implement an XSLT dispatcher/constructor.
Reference static error phase is retained in raw replay logs; the low-level
machine reports those cardinality faults at demand time. This is not a static
optimizer or a universal match to every Saxon error-order optimization.

The original 725 scalar observations and their replay remain unchanged. Additional
native tests exercise focus capture, independent variable replays, path atomic
prefix/mixed terminals, namespaces/attributes/foreign documents, 20,000-level
preparation/execution/drop, cumulative numeric/source work, pair/string comparison
work, range/cache/storage/frame admission, terminal refusal and good/error/good
run isolation.

## Remaining gates

Helper entry/parameters/returns, all Instruction/temporary-document constructors,
recursive helpers, other string functions, regex/casing/Unicode tables, pattern
matching/ordered dispatch, messages/evidence sinks, full package closure and
resolved provenance admission remain unimplemented. They are not implemented by
body strings, caller-focus inheritance or rule-specific Rust conditions. Complete
source/XSD/policy/report/facade integration, independent correspondence/coverage,
development differential/release tests and worker containment remain parent work.
Passing these selected probes is not full XPath conformance, BSC8 completion,
invoice acceptance, executable-package availability or release evidence.
