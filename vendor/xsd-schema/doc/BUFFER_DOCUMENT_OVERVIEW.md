# BufferDocument Overview

`BufferDocument` is a compact, flat-array XML store optimized for XPath 2.0
navigation with XSD type information. It lives in `src/document/` and is
gated by the `xsd11` feature.

---

## Why It Exists

Two consumers need an in-memory XML model:

- The XPath 2.0 engine, both for standalone use and for evaluating XSD 1.1
  `xs:assert` and type alternatives.
- Schema-aware traversal that needs to ask, per node, "what is the resolved
  type and declaration that governs this element/attribute?"

It implements the `DomNavigator` trait (defined in `src/navigator/mod.rs`)
through `BufferDocNavigator`, sharing the same trait with `RoXmlNavigator`.
XPath functions stay DOM-agnostic.

---

## Two Modes

```rust,ignore
// Not compiled: an outline of `document::DocumentKind` (src/document/mod.rs).
pub enum DocumentKind {
    Full,        // Whole document; element index, IDs, source spans active
    Fragment,    // Synthetic root wrapping a single asserted element subtree
}
```

Both variants are unconditional. The whole `src/document/` module — and all
of XPath with it — is `xsd11`-gated at the crate root, so per-variant
`#[cfg(feature = "xsd11")]` would be redundant. Earlier draft designs showed
`Fragment` gated; the shipping code does not.

`Full` is what callers use when they own the document. It does not insist on
a single document element: the builder accepts several top-level elements
under the root, which is what a constructed node sequence or a temporary
tree needs. `Fragment` is what `ValidationRuntime` builds on the fly during streaming validation so that
XSD 1.1 assertions can run XPath against the asserted element's subtree
without preloading the instance into a DOM. Side tables (element index,
source spans, ID map) are skipped in `Fragment` mode.

---

## Node Layout

Each node is exactly 16 bytes: four `u32` words.

```rust,ignore
// Not compiled: an outline of `document::node::Node` (src/document/node.rs).
pub struct Node {
    pub next_sibling: u32,   // NULL = u32::MAX
    pub parent:       u32,
    pub props_type:   u32,   // packed: node type + flags + 20-bit binding index
    pub value:        u32,   // QNameAtom index OR string-store index, depending on type
}
```

`props_type` bit layout:

| Bits | Meaning |
| --- | --- |
| `[3:0]` | `NodeType` (Root, Element, Attribute, ChildValue, Text, Whitespace, SignificantWhitespace, Comment, PI, Nul) |
| `[7:4]` | Flags: `HAS_ATTRIBUTE`, `HAS_CHILDREN`, `IS_COMPLEX_TYPE`, `HAS_NMSP_DECLS` |
| `[8]` | `IS_NIL` — per-instance `xsi:nil="true"` flag |
| `[9]` | `IS_IDREFS` — the XDM is-idrefs property (XDM 1.0 §6.2.4, §6.3.4) |
| `[31:12]` | 20-bit `binding_index` into the document-local `BindingRemapTable` |

### Two-node pattern

Element, Attribute, and Processing Instruction all use a uniform encoding:

- `Element` / `Attribute` / `ProcessingInstruction` nodes carry the *name*
  (a `QNameAtom` index for elements/attributes; a string index for PI targets).
- The associated *value* is a separate `ChildValue` child node.
- Element content children are then ordinary nodes in document order.

This uniformity simplifies forward scans: a single node-type dispatch handles
all three cases.

---

## Addressing

A node reference is a plain `u32` index into a flat array of fixed-size pages:

- Page size: 4096 nodes (`PAGE_SHIFT = 12`), so each page is 64 KB.
- Page/slot decoding is a bitshift / mask — no bit-packed addressing.
- `NULL = u32::MAX`; `Nul` node type marks end-of-document for forward scans.
- First child of `parent` is always `parent + 1` (when `HAS_CHILDREN`).
- Subtree end is found by walking `next_sibling`/`parent` chains until a
  sibling exists or the document ends.

Capacity (~4 billion nodes, ~64 GB) is well past any practical document.

### Identity and order across documents

A node reference is only meaningful inside its own document, so a navigator's
identity is the pair (document, cursor): `is_same_position` compares the
document pointer before the node index, the virtual attribute/namespace
cursor and the namespace chain position. Every `BufferDocument` also gets a
`serial()` — a creation ordinal from a process-wide counter, unique within
the process and strictly increasing — and `compare_position` orders nodes of
different documents by it. XPath 2.0 §2.4.1 leaves the order of distinct
trees implementation-dependent but requires it to be stable and to keep each
tree contiguous; the serial gives a reproducible order where the heap address
would not.

---

## Schema Binding

XSD validation needs more than a type per
node: default and fixed values, nillability, identity-constraint membership,
and substitution-group membership are all *declaration-level*, not
type-level. Two element declarations can share one complex type but
different defaults.

Each typed node therefore carries a binding, not just a type:

```rust,ignore
// Not compiled: an outline of `document::type_remap::NodeSchemaBinding`.
pub struct NodeSchemaBinding {
    pub type_key:        TypeKey,
    pub element_decl:    Option<ElementKey>,    // for elements
    pub attribute_decl:  Option<AttributeKey>,  // for attributes
    pub content_type:    Option<ContentType>,   // for elements
}
```

Bindings live in a per-document `BindingRemapTable` and are referenced by a
20-bit `binding_index` packed into `props_type`. The table deduplicates:
identical `NodeSchemaBinding`s collapse to one entry. Index `0` means
*unbound*. The 20-bit field comfortably exceeds the typical document's
~100–1000 unique bindings.

Two flags optimize hot paths off this design:

- `IS_COMPLEX_TYPE` (bit 6) records simple-vs-complex without a table lookup.
- `IS_NIL` and `IS_IDREFS` are kept *out* of the binding so dedup works —
  nil state is per-instance, not per-declaration, and whether a union-typed
  node holds an IDREF depends on the member type its value has.

The binding's `type_key` is what `DomNavigator::type_annotation()` returns
for element and attribute nodes — complex or simple — while `schema_type()`
is its simple-type projection (`None` for a complex type). Both are `None`
on unbound nodes.

**Defaults as nodes.** The typed build (`build_typed_document_with`,
`TypedBuilderHandler`) makes the tree the XDM instance of the PSVI (XDM 1.0
§6.2.4, §6.7.4): each defaulted attribute is an attribute node, and a valid
empty element that takes its declaration's `default` / `fixed` value gets a
text node with it — a QName in such a value written with a prefix the
instance binds, declared on the element when none is.
`TypedBuildOptions::defaults_as_nodes = false` (or
`TypedBuilderHandler::with_defaults_as_nodes(false)`) builds neither.
`typed_value()` still reads `element_decl` for an empty element without such
a text node: when its declaration carries a `default` or `fixed` value, the
declared value is the typed value, so assertions and a tree built without
defaults as nodes see default-aware values too.

**White space and normalized values.** A text node that is all white space is
not built in element-only content. A valid attribute, and a valid element of a
simple type or with simple content, has its schema normalized value as its
string value — one text node beside its comments and processing instructions,
none for an empty value. Mixed content and invalid nodes keep their text as
read; source spans still point at the text as read.

---

## Side Tables

Kept off the main node array because their access patterns differ:

| Table | When | Purpose |
| --- | --- | --- |
| `NamespacePageFactory` | always | In-scope namespace chains (namespace axis has different semantics) |
| `NamespaceScopes` | always | Which chain is in scope at each node, and how to walk it (below) |
| `ElementIndex` | `Full` only | Fast `//name` lookup |
| `NodeSourceSpans` | `Full` + tracking enabled | Line/column for error reporting |
| id index (`id_elements`) | `Full` only | `xs:ID` → element node, for `fn:id` and `find_element_by_id`: an element whose typed value is a single `xs:ID` (simple type or simple content), the owner of an ID-typed attribute, or a node a host registered (`register_xml_id`) |
| idrefs index | `Full` only | each IDREF token → the element and attribute nodes with the is-idrefs property, for `fn:idref` and `find_nodes_by_idref` |

The shared `NameTable` (from `src/namespace/`) and a per-document
`QNameTable` / `StringStore` carry the actual string data.

**Namespace scope.** An element that declares namespaces owns a chain: its own
declarations, last first, followed by the chain in scope at its parent, so one
chain holds every binding in scope there, each prefix's binding in force first.
`NamespaceScopes` answers the two questions a reader of the chain asks — the
namespace axis, `fn:in-scope-prefixes`, the validation drivers' namespace
context, the serializer's declarations, the typed value of an `xs:QName` — in
time that does not grow with the element's depth:

- *Which chain is in scope at a node.* The builder adds nodes in document
  order, and the chain in scope changes only where a declaring element starts
  and right after it ends. Those changes are recorded in node order (8 bytes
  each, two at most per declaring element), and a binary search finds the one
  in force at any node, attributes and text included.
- *Where a walk of the bindings goes next.* One link per namespace node (4
  bytes): its `next`, except at an element's last own declaration, where the
  walk skips the leading run of the parent's bindings whose prefixes the
  element declares itself — hidden there and below. So an element that
  declares the default namespace again, as some generators do on every
  element, does not make the walk revisit every ancestor's declaration. A
  skipped node always has a prefix the walk has already met, so the bindings
  read, and their order, are those of the full chain.

A document without declarations has no scope entries; the bench catalogue
(16 MB, no declarations) gains nothing per node. Each declaration costs 16 bytes
in all (12 in the chain, 4 for the walk) and each declaring element up to 16
more.

---

## Construction

`BufferDocumentBuilder` exposes both a low-level push API and a
`quick-xml`-driven adapter (`build`, which `BufferDocument::from_reader`
runs). The adapter reads through the crate's well-formedness layer
(`xsd_schema::xml`): a document that is not well-formed — input that ends
inside an element included — is `BufferDocumentError::NotWellFormed`, the
rules follow the XML declaration (XML 1.0 or 1.1), and the declaration is kept
(`BufferDocument::xml_declaration`). `BufferDocumentBuilder::with_parse_limits`
sets parse limits (`ParseLimits`), checked before anything of the markup that
breaks one is interned; the push API is not limited. The arena (`bumpalo::Bump`) is
**caller-owned**: the builder borrows it. This is what lets
`ValidationRuntime` reuse a single builder across nested asserted elements
without allocating per-element arenas, and what makes the second
`unsafe` block in the crate necessary (see [`UNSAFE.md`](UNSAFE.md)).

```rust
use bumpalo::Bump;
use xsd_schema::document::{BufferDocument, BufferDocumentBuilder, BufferDocumentOptions};
use xsd_schema::namespace::NameTable;
use xsd_schema::xml::ParseLimits;

let arena = Bump::new();
let names = NameTable::new();
let doc = BufferDocument::from_reader_default("<a><b/></a>".as_bytes(), &arena, &names)?;
let nav = doc.create_navigator();

// With limits:
let limited = BufferDocumentBuilder::new(&arena, &names, None, BufferDocumentOptions::default())?
    .with_parse_limits(ParseLimits::none().with_max_depth(256).with_max_nodes(1_000_000))
    .build("<a><b/></a>".as_bytes())?;
# let _ = (nav, limited);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Schema binding assignment is performed by an external driver
(`SchemaValidator` / `ValidationRuntime`), not by the builder itself —
the document module stays free of validation logic.

---

## Serialization

`src/document/serialize.rs` writes a tree back out as XML text. It is generic
over `DomNavigator`, not over `BufferDocument`, so the same code serializes a
`BufferDocNavigator`, a `RoXmlNavigator` and any navigator a host embedding the
engine brings of its own; it touches no `pub(crate)` node pages.

```rust
use bumpalo::Bump;
use xsd_schema::document::{serialize, BufferDocument, SerializeOptions};
use xsd_schema::namespace::NameTable;

let arena = Bump::new();
let names = NameTable::new();
let doc = BufferDocument::from_reader_default("<a><b/></a>".as_bytes(), &arena, &names)?;

let compact = serialize::to_string(&doc.create_navigator(), &SerializeOptions::default())?;
assert_eq!(compact, "<a><b/></a>");
let formatted = SerializeOptions { indent: Some(2), ..SerializeOptions::default() };
let mut out = Vec::new();
serialize::serialize_document(&doc.create_navigator(), &mut out, &formatted)?;
assert_eq!(String::from_utf8(out)?, "<a>\n  <b/>\n</a>");
# Ok::<(), Box<dyn std::error::Error>>(())
```

Three entry points — `serialize_document` (a whole document, optionally with an
XML declaration), `serialize_node` (one node: a `Root` behaves as a document, an
element writes its subtree, an attribute or namespace node is refused) and
`to_string` — plus `SerializeOptions` and `SerializeError`.

**What is written.** UTF-8, empty elements collapsed to `<a/>`, attributes in
stored document order. Escaping follows
*Canonical XML 1.0* §2.3: in text `&`, `<`, `>` and U+000D; in attribute values
(always `"`-quoted) `&`, `<`, `"` and the three whitespace characters as
references, so XML 1.0's end-of-line handling (§2.11) and attribute-value
normalization (§3.3.3) cannot change the value on a re-parse. Content that XML
cannot express — a character outside the `Char` production, a comment with `--`,
a PI target `xml`, a name whose prefix is not in scope — is an error, never
dropped or repaired.

**Two modes, one traversal.** `SerializeOptions::indent` is `None` by default —
*compact*: no layout whitespace is added, and none is removed, which is what
makes it the mode for an exact text round trip. `Some(n)` is *formatted*: a line
break (LF everywhere) plus `n` spaces per level, `Some(0)` being the breaks
alone. The outermost element sits at depth 0, a subtree written on its own
included; attributes stay on the start-tag line; no trailing newline is written;
the declaration is controlled separately. Formatting happens inside the same
iterative walk — per-depth layout state and a direct-child scan with cloned
navigators — so there is no second tree, no intermediate string and no textual
pretty-print pass, and compact mode skips the scans entirely.

The whitespace contract is deliberately conservative, because added layout
becomes text nodes when the output is parsed again:

1. Every existing text character survives in both modes. Before formatting a
   container, *all* of its direct children are inspected: any text child
   suppresses added layout throughout that container's subtree — so
   `<p>Hello <b>world</b>!</p>` and `<p><b>world</b>!</p>` alike stay on one
   line, and existing indentation is kept rather than reindented.
2. A break goes only at a child boundary next to an element: before an element
   child at the child's depth, before the closing tag at the container's depth
   when the last child is an element, and between siblings when either is an
   element. A run of comments and PIs is never split internally; nothing is ever
   inserted inside text, a comment, PI data, an attribute value or an empty
   element.
3. `xml:space` (XML 1.0 §2.10, by its expanded XML-namespace name) is honoured:
   `preserve` disables added layout for that element and is inherited, a
   descendant `default` restores it for its own content unless rule 1 still
   suppresses, and a subtree serialized on its own consults its ancestors for the
   inherited value without inventing an attribute.
4. At document level the same rules apply at depth 0. A declaration immediately
   followed by the document element gets a break; a declaration followed by a
   comment or PI does not. No leading blank line, no final newline, and no layout
   around a standalone text/comment/PI node.

**Namespace declarations** are written where they are *introduced*. The
serializer keeps its own in-scope stack while descending and diffs each
element's `namespace::ExcludeXml` axis against it: a binding that is not already
in scope is declared, one that merely repeats an inherited declaration is
dropped, and an element that leaves an inherited default namespace gets
`xmlns=""`. The `xml` prefix is never declared, prefixed undeclarations (XML 1.1
only) are never written. A name that does not resolve through that stack is
`SerializeError::UnboundName` — the serializer refuses to write a tree whose
names would not read back the same, and leaves repairing them to whoever built
it. Declarations on one element come out in prefix order, since the namespace
axis has no order of its own to preserve.

**What `from_reader` cannot give it back.** The parse keeps the XML declaration
(`BufferDocument::xml_declaration`), but the serializer writes its own
(`SerializeOptions::xml_declaration`); it keeps no `<!DOCTYPE` or internal
entity declarations, no CDATA section markers (the content is kept, as text),
and no text outside the document element. A round trip is therefore an
identity on the *tree*, not on the bytes; `tests/serialize_roundtrip.rs` holds
the whole XSD conformance corpus to that standard, in compact mode.

---

## Copying

`src/document/copy.rs` is the step between parsing and writing: taking nodes
that already exist — in another `BufferDocument`, in a `roxmltree` document, in
any navigator a host supplies — and appending them to the document being built,
as new nodes with new identities. Four additive methods on the builder, plus
`CopyOptions`, `CopyError`, the `CopySource` trait and `NamespaceFixup`:

```rust
use xsd_schema::document::copy::{CopyError, CopyOptions, CopySource};
use xsd_schema::document::BufferDocumentBuilder;
use xsd_schema::types::value::XmlValue;
use xsd_schema::xpath::XmlItem;

fn copy_into<N: CopySource>(
    builder: &mut BufferDocumentBuilder<'_>,
    node: &N,
    attr: &N,
    items: &[XmlItem<N>],
    value: &XmlValue,
) -> Result<(), CopyError> {
    builder.copy_subtree(node, CopyOptions::default())?; // element/text/comment/PI/document
    builder.copy_attribute(attr, CopyOptions::default())?; // onto the open element
    builder.append_content(items, CopyOptions::default())?; // a whole XPathValue sequence
    builder.append_atomic(value)?; // one atomic value as text
    Ok(())
}
```

**Constructor semantics.** `append_content` applies the rules an XQuery element
constructor applies to its content sequence — the constructor content rules
(XQuery 1.0 §3.7.1.3): an atomic value becomes text, separated from a
*preceding atomic value* by a single space and from a node by nothing; a
document node contributes its children; zero-length text disappears and
adjacent text merges (which the builder's text coalescing already does). An
attribute after a child is `CopyError::AttributeAfterContent` (XQuery XQTY0024)
and one with no element open is `CopyError::AttributeOutsideElement`. Two
attributes with the same expanded name cannot both survive: XQuery raises
XQDY0025, this builder keeps the later value, which needs no lookahead.

The "has a child" and "last item was atomic" facts belong to the container, not
to the call, so the builder carries one `ContentState` per open element (plus
one for document level), pushed at `start_element`, popped at `end_element`.
Splitting a sequence across two `append_content` calls therefore changes
nothing about the output, and an explicit `text()` call breaks an atomic run.

**Namespace fixup.** A copied element's names must keep resolving where the
ancestors are different ones. `NamespaceFixup` mirrors the open elements as a
scope stack and answers, per element, which declarations it has to carry
(XQuery 1.0 §3.7.4): the element's own binding first, then the declarations
carried over from the source (`CopyNamespaces::Preserve`, the source element's
*local* namespace axis — `NoPreserve` carries none), then the attributes. An
attribute whose prefix is empty (a namespaced attribute cannot use the default
namespace, Namespaces in XML 1.0 §6.2) or already bound to another URI gets a
generated prefix, `ns0`, `ns1`, … An unprefixed element in no namespace under
an inherited default namespace gets `xmlns=""`. The declarations are computed
*before* `start_element`, which is where the builder takes them; the one
exception is `copy_attribute`, which may declare on an element that is already
open — sound because an attribute can only be added before any child exists.
`CopyOptions::inherit = false` starts the copy from an empty scope, so its top
element re-declares everything its names need; an inherited *default* namespace
stays visible there, because XML 1.0 cannot undeclare a prefix but an
unprefixed name still needs `xmlns=""` under one.

This is the counterpart of the serializer's strictness: fixup is what
guarantees the invariant the serializer only *checks* (`UnboundName`), so every
tree built through this module can be written out.

**Annotations.** A copy is untyped by default: no binding, no nil flag, so it
atomizes as `xs:untypedAtomic` whatever the source was (a host that wants the
copy of an ID or IDREF node to keep that role registers it with
`register_xml_id` / `register_idrefs`).
`Annotations::Preserve` replays `set_node_binding` and `set_nil` from the
source — but only when the source document is bound to the *same* `SchemaSet`
as the target, compared by address, since a `TypeKey` means nothing in another
set. A typed source bound to a different set is `CopyError::SchemaMismatch`; a
source with no schema set at all has nothing to preserve and copies as `Strip`
would.

`tests/copy_roundtrip.rs` copies subtrees out of a spread of corpus documents,
writes source and copy out, re-parses both with `roxmltree` and compares
canonical forms — so a copy that lost content, or that is not
namespace-well-formed, fails the test.

---

## Fragment Mode And Assertions

When `ValidationRuntime` opens an element whose governing complex type has
assertions and `AssertionSource::FragmentBuffer` is active:

1. A `Bump` arena and a `BufferDocumentBuilder` in `DocumentKind::Fragment`
   mode are started, scoped to that element.
2. Subsequent push events for the subtree are fed into the same builder.
   Nested asserted elements share the builder and arena.
3. On close of the outermost asserted element, `builder.finalize()`
   produces a `BufferDocument` whose synthetic Root wraps the asserted
   element; navigation stops at the fragment boundary.
4. XPath assertions evaluate with the asserted element (not the synthetic
   Root) as context item. The arena is then dropped.

This is the streaming counterpart to the `Full` mode and is the assertion
path described in [`OVERVIEW.md`](OVERVIEW.md) under *XSD 1.1 Assertion Buffering*.

---

## Where Things Live

| Concern | Path |
| --- | --- |
| `Node`, node type, flags | `src/document/node.rs` |
| Pages, allocation | `src/document/page.rs` |
| QName / string interning | `src/document/qname.rs`, `strings.rs` |
| `NodeSchemaBinding`, remap | `src/document/type_remap.rs` |
| Namespaces | `src/document/namespace.rs` |
| Element index / source spans | `src/document/element_index.rs`, `source_spans.rs` |
| Document, navigator, builder | `src/document/buffer.rs`, `navigator.rs`, `builder.rs` |
| Typed build (validation + build in one pass) | `src/document/typed_builder.rs` |
| XML output | `src/document/serialize.rs` |
| Subtree copy, content rules, namespace fixup | `src/document/copy.rs` |