# Immutable source prerequisite

`einvoice-source` is an unpublished Rust 1.83 XML 1.0 construction/read-access
crate. It does not validate schemas, rules, target profiles or invoice acceptance.
It depends on exact `xml-rs =0.8.28` and `xmlparser =0.13.6` pins. The root
registry patch selects the [maintained parser](../../vendor/xml-rs-maintenance.md),
which permits legal `xml`-prefixed elements without changing original XML.
The same correction applies to existing facade/core parsing; it does not enable
invoice validation. BSC-7's compiler/AST and the numeric kernel remain separate.
Both the maintained parser and source crate are unpublished.

```rust
use einvoice_source::{QueryBudget, SourceDocument};

let source = SourceDocument::parse(b"<r a='1'>A<![CDATA[B]]>&amp;</r>")?;
let root = source.root_element();
assert_eq!(source.original_bytes(), b"<r a='1'>A<![CDATA[B]]>&amp;</r>");
assert_eq!(root.attributes().next().unwrap().value(), "1");
assert_eq!(root.string_value(&mut QueryBudget::new(100, 100)?)?, "AB&");
assert_eq!(root.canonical_path(&mut QueryBudget::new(100, 100)?)?, "/Q{}r[1]");
# Ok::<(), einvoice_source::SourceError>(())
```

## Construction and source evidence

Input size is refused before copying. The facade's encoding-aware ASCII prolog
DTD guard runs before any xml-rs invocation. A bounded declaration-token check
closes xml-rs 0.8.28's encoding-label prefix bug; a byte-limited xml-rs probe then
requests only `StartDocument`. The probe does not parse the document body. Exact
case-insensitive supported labels are UTF-8/UTF8, UTF-16/UTF16, ISO-8859-1/Latin1,
and US-ASCII/ASCII. UTF-16 requires the matching LE/BE BOM. BOM/declaration
conflicts are syntax failures. UTF-16LE/UTF-16BE *declaration labels* are an
unsupported encoding capability of the pinned parser; BOM-marked UTF-16 LE/BE
with `UTF-16` declarations, or no declaration, are supported. No lossy decoding
or fallback encoding is used. A literal U+FFFD is preserved as actual content.

The non-lossy original decoded view is saved separately (BOM excluded). A full
xmlparser token pass retains declarations, namespace declarations, empty CDATA,
document whitespace, exact token/value ranges and owner/piece associations.
Original decoded gaps before/between/after tokens are accessible, so concatenating
gaps and token spellings reconstructs `source_view()`. BOM bytes have their own
ledger marker and remain in `original_bytes()`.

Lexical admission bounds names, values, CDATA, all attributes **including xmlns**,
nodes, pieces, depth and namespace frames before the full parser's buffers or
squashed namespace snapshots grow. Duplicate lexical attributes/declarations are
refused before namespace-map collapse. Effective-prefix admission uses ancestor
frames; its payload/work charges include shadowed bindings because xml-rs
`squash()` clones them before overwriting. The reserved xmlns framework and
implicit xml/default mappings are charged separately from exposed namespaces.

Before SAX, lexical QName resolution admits conservative UTF-8 payload bounds for
every namespace/name copy: four copies for opening/stack/queued-end names, three
for each ordinary attribute, and three for explicit closing names (including
mismatches). Namespace declarations separately reserve their QName buffer,
parsed fields and namespace-stack key; the snapshot key is charged with its frame.
Default namespaces apply only to elements; implicit xml, aliases, inherited and
shadowed declarations, and raw reference spellings are included. These counts
bound logical string-copy payload, not allocator capacities. Name adoption moves
already-admitted strings without charging the reservation again. Parser values
and possible arena text copies are admitted from raw spans at twice their size;
PI separator whitespace and document whitespace count even when absent from the
semantic value. Later checks still enforce actual value/URI limits. Conservative
reservations may refuse an input earlier than exact final-arena accounting.

Both parser constructions preadmit the implicit namespace strings and framework
records, separately from each event snapshot. The bounded header probe also
reserves four times its admitted source-byte span for decoded token/name payloads
before requesting StartDocument, which may first consume a PI target. A fixed
encoding-label allowance is reserved for each parser. These conservative charges
can reject small-budget inputs before their exact consumed header is known.

Tokenizer attribute-value ranges guide XML 1.0 normalization: literal CRLF/CR
become LF everywhere, and literal attribute TAB/LF/CRLF become space. References
remain spelled as references until xml-rs expands them. The normalized view is
re-encoded to the original encoding and BOM, preserving declaration consistency.
There is exactly one full semantic SAX parse. Comments remain enabled; trimming
and parser coalescing are disabled; CDATA remains a separate event; document
whitespace is lexical only. Full structure, names, references and EOF are checked
by xml-rs. Expanded ordinary attribute duplicates are additionally refused across
prefix aliases. Reserved namespace URI bindings also receive an explicit check.

Preorder kind/parent/QName/attribute-count/attribute-name/subtree/EOF alignment
must match before publishing the arena. Failure discards all partial records.
XML 1.1 returns `UnsupportedVersion` before XML 1.0 normalization. Proving XML 1.1
semantics is an **OPEN full native parity gate**. No DTD is expanded, no external
resolver exists, and no source construction result certifies conformance.

## Read access and identity

The result is `Arc<SourceDocument>`, containing owned immutable records and private
indexes, without unsafe lifetimes or a self-referential parser tree. It is
`Send + Sync`. Borrowed `Node`, `Attribute`, `Namespace`, `Declaration` and
`LedgerEntry` handles retain their owner; an external local ordinal cannot be
silently interpreted in another document. Opaque node identity combines a unique
checked document sequence ID with a typed node/attribute/namespace identity.
Arc clones preserve identity; a new parse of identical bytes is a new document.

Elements and attributes expose original lexical prefixes and expanded URI/local
names. Attribute order is source order. Attributes/namespaces are separate axes,
not children. Local namespace declarations retain aliases, redundant bindings
and default undeclarations. Effective namespace iteration uses persistent frames:
nearest frame first, source order within each frame, then ancestors, then implicit
`xml` if not explicit. Shadowed/empty bindings and reserved `xmlns` are excluded.
Namespace identity belongs to the element exposing it, even for inherited bindings.
Document order is element, effective namespaces in this implementation order,
source-order attributes, then ordinary children. Cross-document order is undefined.

Adjacent nonempty text/CDATA/reference chunks share one text node, including an
empty CDATA inside a text run. Comments, PIs and elements separate runs. Empty
CDATA alone has no semantic node and is associated with its owner element.
PI targets must be NCNames, including valid Unicode names. PI string-values
exclude leading XML separator whitespace and preserve trailing content.
Document-level comments/PIs are retained; declarations/BOM are not PI/text nodes.
Sibling occurrence is cached by expanded element name, text/comment kind or PI
target. `canonical_path` reproduces the schxslt location function for these axes.
Paths are rendering evidence, not identity or a diagnostic deduplication key.

`OriginalUtf8Span` points only into actual original UTF-8 bytes, including BOM
offset. UTF-16/Latin1/ASCII export `OriginalScalarSpan` before normalization and
excluding BOM. Internal decoded UTF-8 indexes have a separate private Rust type.
CRLF is two source scalars but one diagnostic newline, reference spellings count
lexically, and supplementary characters count once. UTF-16LE BOM + `<r>é</r>` is
18 original bytes, 8 scalars, and element scalar range `[0,8)`. Attributes expose
exact token/value spans and an explicitly named owner fallback. Text exposes
ordered contributing pieces and a **covering** range, never a fabricated single
token. Original coordinate checkpoints every 64 scalars avoid repeated full scans;
normalized parser positions map back through separate checkpoints. Missing semantic
coordinate mapping is an engine alignment error. Header/decode errors can have no
position when an original mapping cannot be established; no position is guessed.

Borrowed navigation/content/ledger access allocates nothing. Namespace iteration
can walk bounded ancestor frames; a future evaluator must charge navigation work
(or traverse declarations/parents itself under its own budget). No evaluator or
numeric/backend budget type is coupled here. Descendant string values are not
copied into ancestors. String-value collection/streaming and path collection
require an explicit `QueryBudget` for work and UTF-8 output bytes. A caller's
streaming writer may contain a prefix when a writer/budget failure occurs.

## Logical construction limits

All `SourceLimits` fields must be nonzero and <= `isize::MAX`; node/token caps
also fit `u32`. All cumulative charges use checked arithmetic; own vectors,
strings and admission hash maps reserve fallibly after logical admission.

| Dimension | Default |
| --- | ---: |
| Original input / maximum element depth | 16 MiB / 128 |
| Decoded UTF-8 / normalized UTF-8 or re-encoded buffer (each) | 32 / 64 MiB |
| Header probe | 64 KiB |
| Lexical tokens / ordinary nodes | 1,000,000 / 500,000 |
| Per lexical token / QName / URI / raw or expanded value | 16 MiB / 4 KiB / 16 KiB / 16 MiB |
| All attributes / local declarations / effective prefixes per element | 1,024 / 256 / 1,024 |
| Cumulative namespace frame/map payload estimate | 64 MiB |
| Cumulative copied/retained string payload | 192 MiB |
| Piece associations / cumulative logical storage payload | 1,000,000 / 384 MiB |
| Cumulative raw upper bounds on expanded values | 64 MiB |
| Cumulative construction work units | 512 Mi units |

The effective-prefix cap counts implicit xml and distinct explicit prefixes,
including default undeclarations, without double-counting redundant xml bindings.
The parser's reserved xmlns/default framework is charged to namespace storage.
`value_bytes` additionally limits a whole merged text run. Upper bounds before
expansion are conservative: a long reference spelling can consume more admission
budget than its expanded value. Storage/string/namespace counts are cumulative,
including temporary metadata and views, rather than just the final arena.
Namespace payload estimates also consume string/storage admission; these
conservative counters can overlap. They are not exact live-byte accounting.

These are initial finite construction safeguards, **not final RSS/performance
policy**. Logical payload charges are not allocator capacity, overhead or exact
live heap accounting. Vec/HashMap spare capacity, Arc construction/conversion,
parser/BTreeMap internals and error formatting are not all fallible allocations.
The allocator can still abort; no hard RSS or killable deadline guarantee is
claimed. Work includes bytes, tokens/events, namespace walks, name/value hashing
and sibling indexing; it is an explicit logical counter, not instruction counting.
`parse_cancellable` checks an `AtomicBool` at work checkpoints and stage boundaries;
it is cooperative, not a hard interrupt inside a parser operation.

Errors distinguish syntax, unsupported encoding/version/DTD, resource exhaustion,
cancellation, invalid configuration, metadata alignment and writer failure.
Resource/capability/engine errors do not mean invalid invoices. Any parser-internal
limit refusal is a resource error, not syntax; its opaque maximum is reported
as `Resource.limit = None`, rather than inventing a configured maximum.

## Evidence and checks

`tests/source.rs` contains authored synthetic grammar, normalization, namespace,
ledger, span, identity/order/path, direct-text profile and resource regressions.
`tests/saxon_paths.rs` compares 18 independently captured Saxon path/value rows.
Normal Rust tests are offline and require no Java. See [evidence/README.md](tests/evidence/README.md)
for a clean-checkout optional Java replay with supplied SHA-checked dependencies.
Cross-platform CI, physical resource policy, full XML capability parity, facade
integration, source/runtime/schema adapters and native invoice acceptance remain
separate integration/release gates.
