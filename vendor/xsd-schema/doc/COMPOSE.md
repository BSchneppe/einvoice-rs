# Composing XML From Rust

This document is a guide to the `compose` module: XQuery-style composition of
XML in Rust, without an XQuery processor. It is written to be read in order —
each section builds on the previous one — but the section list is also a
reference, and every claim in it points at an API you can look up in rustdoc.

## 1. What This Is

A FLWOR expression does three things at once: it evaluates XPath, it iterates
and binds, and it constructs elements. An XQuery processor is one program that
does all three. This module says that only the third one is missing.

The XPath 2.0 engine in this crate already evaluates every expression these
queries need — `max`, `count`, `avg`, `sum`, `distinct-values`, `contains`,
`empty`, date component extraction, quantified expressions, nested `for`
inside a predicate. Rust already iterates, binds and branches better than any
query language does. What was missing was a way to bind a Rust value to an
XPath `$variable`, a way to describe a result element, and the data-model rules
that turn a description into a tree.

So there are three ingredients:

| XQuery construct | C# with LINQ | This module (Rust) |
| --- | --- | --- |
| `for` / `let` / `where` / `order by` / `return` | LINQ `from` / `let` / `where` / `orderby` / `select` | `pipe::nodes` / `try_map` / `try_filter` / `order_by` / `try_map` (§7) |
| an XPath expression with variables bound to host values | `node.XPath2Select("//bid_tuple[itemno = $i/itemno]", new { i = item })` | `xpath!(c, "//bid_tuple[itemno = $i/itemno]", bids, i = &i)?` (§5) |
| element constructors with `{ expr }` splices | `new XElement("item_tuple", item.Element("itemno"), …)` | `form! { (item_tuple ^{ … } ^{ … }) }` (§6) |
| `doc("items.xml")` | `XDocument.Load("items.xml")` | `c.load_file("items.xml")?` (§3) |
| the serialized result | `XNode` printed by .NET | `doc.to_xml(&opts)?` (§10) |

The C# column shows the same three ingredients in another host language. The
difference worth naming up front is what the Rust column does *not* need.
In C#, an empty result has to be special-cased so an empty element is not
constructed:

```csharp
// C# with LINQ, abridged
new XElement("item_tuple", item.Element("itemno"),
    !bid.Any() ? null : new XElement("high_bid", /* … */));
```

Here, `form! { (high_bid ^{ max_bid }) }` with an empty `max_bid` *is*
`<high_bid/>`, because that is what the data model says an empty sequence
spliced into an element produces. No conditional, no `null`, nothing for the
host to remember.

What this is not: a query parser, a FLWOR runtime, or a new value model. There
is no XQuery syntax anywhere, the values are the XPath engine's own, and the
only thing this module adds to the crate's data model is the ability to
describe an element before you build it.

## 2. Setup

The module is behind one feature flag, which implies `xsd11`:

```toml
[dependencies]
xsd-schema = { version = "0.3", features = ["compose"] }
bumpalo = "3"
```

`bumpalo` is named explicitly because a `Composer` borrows a `Bump` arena and
the crate does not re-export it.

```rust
use bumpalo::Bump;
use xsd_schema::compose::Composer;
use xsd_schema::namespace::NameTable;

let arena = Bump::new();
let names = NameTable::new();
let c = Composer::new(&arena, &names);
```

A `Composer` holds the arena, the name table, the namespace context, the copy
options and a cache of compiled expressions. Its builder methods are:

| Method | What it sets |
| --- | --- |
| `with_schema_set(&SchemaSet)` | the schema for typed values and annotations; its own name table then wins over the one passed to `new` |
| `with_namespace(prefix, uri)` | a prefix for `$p:x` in an expression and `p::local` in a form |
| `with_default_element_namespace(uri)` | the namespace an unprefixed *element* name in a form takes |
| `with_base_uri(uri)` | the static base URI for relative-URI resolution in expressions |
| `with_copy_options(CopyOptions)` | how spliced nodes are copied — namespace retention, inheritance, annotations |

Each setter rebuilds the static context and clears the expression cache, so
call them before evaluating anything.

Three prefixes are bound before you bind anything — `xs`, `xsi` and `fn` — so
`xs:date("1999-01-31")` in an expression and `(xs::schema …)` in a form work
with no declaration of your own, and `with_namespace` rebinds any of them
because the last declaration of a prefix wins.

### The lifetime model

Everything the composer loads or builds is finalized into the arena and handed
back as a `Doc<'a>` — a `Copy` handle. Every navigator, `Value` and `Form`
therefore shares that one lifetime `'a`, which is why a node taken out of one
document splices into another with no lifetime work at all, and why a function
that returns a `Form<'a>` needs nothing but `c: &Composer<'a>`.

The price is the other half of that sentence: **nothing is freed before the
composer and its arena go away**. There is no way to release one intermediate
document early. The model is therefore one composer per request, per document,
or per batch — create it, compose, serialize, drop the lot.

Every method takes `&self`: the arena allocates through a shared reference, the
name table is interior-mutable, and the cache is a `RefCell`. So `xpath!` and
`form!` work inside iterator closures and nested loops without any `&mut`
gymnastics. The name table is not synchronized, so a `Composer` is neither
`Send` nor `Sync` — one per thread.

## 3. Loading Documents

```rust
use xsd_schema::compose::{ComposeError, Composer};

fn load_three(c: &Composer<'_>, handle: std::fs::File) -> Result<(), ComposeError> {
    let items = c.load_file("TestSources/items.xml")?;            // from a path
    let inline = c.load_str("<stock><part>bolt</part></stock>")?; // from a string
    let piped = c.load_reader(std::io::BufReader::new(handle))?;  // from a reader
    let _ = (items, inline, piped);
    Ok(())
}
```

All three parse into the composer's arena and return `Doc<'a>`. A `Doc` gives
you:

| Method | Result |
| --- | --- |
| `root()` | the document node, as a navigator — what `//x` should be evaluated against |
| `document_element()` | the outermost element, or `None` |
| `children()` | every top-level node, in document order (§9) |
| `to_xml(&opts)` / `write_xml(w, &opts)` | the XML text (§10) |
| `inner()` | the underlying `BufferDocument`, for the rest of the crate's API |

What the parse keeps and drops is the document buffer's contract, not this
module's: whitespace text is kept, CDATA and entity references are merged into
text, comments and processing instructions are kept, the XML declaration is
kept on the document (`inner().xml_declaration()`) but not written back out
unless the serialization options ask for one, and a document type declaration
is checked and dropped. The text is read through the crate's well-formedness
layer, so a document that is not well-formed is refused
(`ComposeError::Document`).

## 4. RQ2 From Start To Finish

This is the second query of the relational use case in the XQuery 1.0 test
suite, and it exercises most of the module at once — a source, a `let`, a
`where`, an `order by`, a projection, and an empty sequence spliced into an
element.

```xquery
<result>{
  for $i in $input-context1//item_tuple
  let $b := $input-context2//bid_tuple[itemno = $i/itemno]
  where contains($i/description, "Bicycle")
  order by $i/itemno
  return <item_tuple>{ $i/itemno }{ $i/description }
         <high_bid>{ max($b/bid) }</high_bid></item_tuple>
}</result>
```

```rust,no_run
// Not run: reads items.xml and bids.xml of the XQuery 1.0 test suite.
use bumpalo::Bump;
use xsd_schema::compose::order::{Direction, EmptyOrder};
use xsd_schema::compose::{pipe, Composer};
use xsd_schema::document::SerializeOptions;
use xsd_schema::namespace::NameTable;
use xsd_schema::{form, xpath};

let arena = Bump::new();
let names = NameTable::new();
let c = Composer::new(&arena, &names);
let items = c.load_file("XQTS_1_0_2/TestSources/items.xml")?;
let bids = c.load_file("XQTS_1_0_2/TestSources/bids.xml")?;

// for $i in ...
let rows = pipe::nodes(xpath!(c, "//item_tuple", items)?)
    // let $b := ...
    .try_map(|i| {
        let b = xpath!(c, "//bid_tuple[itemno = $i/itemno]", bids, i = &i)?;
        Ok((i, b))
    })
    // where ...
    .try_filter(|(i, _)| xpath!(c, "contains(description, 'Bicycle')", i)?.boolean())
    // order by ...
    .order_by(Direction::Ascending, EmptyOrder::Least, |(i, _)| {
        xpath!(c, "itemno", i)?.key()
    })?
    // return ...
    .try_map(|(i, b)| {
        Ok(form! {
            (item_tuple
                ^{ xpath!(c, "itemno", &i)? }
                ^{ xpath!(c, "description", &i)? }
                (high_bid ^{ xpath!(c, "max($b/bid)", b = &b)? }))
        })
    });

let doc = c.build(form! { (result ..?^{ rows }) })?;
println!("{}", doc.to_xml(&SerializeOptions::default())?);
# Ok::<(), xsd_schema::compose::ComposeError>(())
```

```xml
<result><item_tuple><itemno>1001</itemno><description>Red Bicycle</description><high_bid>55</high_bid></item_tuple><item_tuple><itemno>1003</itemno><description>Old Bicycle</description><high_bid>20</high_bid></item_tuple><item_tuple><itemno>1007</itemno><description>Racing Bicycle</description><high_bid>225</high_bid></item_tuple><item_tuple><itemno>1008</itemno><description>Broken Bicycle</description><high_bid/></item_tuple></result>
```

Four things to take from it:

* **One closure per FLWOR clause.** The `let` is a projection that carries the
  bid sequence beside its item as the tuple `(i, b)`; the filter and the key
  selector borrow that row; the final projection consumes it.
* **Relative expressions take a context item**, not a rebound variable:
  `xpath!(c, "itemno", i)` is `$i/itemno` with `$i` as the focus.
* **No result element exists before the sort.** `order_by` buffers the rows
  and computes each key once; the projection that builds forms comes after it.
* **`<high_bid/>`** is the broken bicycle, which has no bids. `max` of an empty
  sequence is the empty sequence, and an empty sequence spliced into an element
  contributes no content. Nothing in the Rust tests for it.

The whole use case — all eighteen queries — is written this way in
`tests/compose_usecase_r.rs` and compared with the suite's own expected
results. `examples/xquery_without_xquery.rs` runs three of them:

```bash
cargo run --example xquery_without_xquery --features compose -- ../../XQTS_1_0_2
```

## 5. `xpath!` — Evaluating With Rust Values Bound

```rust
use xsd_schema::compose::{ComposeError, Composer, Doc, Nav};
use xsd_schema::xpath;

fn four_shapes<'a>(
    c: &Composer<'a>,
    node: Nav<'a>,
    item: Nav<'a>,
    items: Doc<'a>,
    bids: Doc<'a>,
) -> Result<(), ComposeError> {
    xpath!(c, "1 + 1")?;                             // no context item, no variables
    xpath!(c, "name()", node)?;                      // a context item
    xpath!(c, "($i, $b)", i = &item, b = &bids)?;    // the external variables $i and $b
    xpath!(c, "//item_tuple[. = $i]", items, i = &item)?; // a context item, then variables
    Ok(())
}
```

The macro is exported at the crate root, so the import is
`use xsd_schema::{form, xpath};`.

The expression is always a string literal. A context item, when there is one,
is always the **third** argument, directly after it; everything after that is a
`name = value` binding, and a trailing comma is accepted everywhere. The result
is `Result<Value, ComposeError>`, so a call site ends in `?`.

`name = value` is also Rust's assignment expression, so the rule is explicit:
the bindings-only shape is matched first, and a named binding never implies a
context item. `xpath!(c, "name($node)", node = &n)` binds `$node` and leaves
the focus empty; `xpath!(c, "name()", &n)` supplies the focus and binds
nothing. A context item written *after* a binding, or two context items, is a
compile error whose message names the expected order. For preparatory work use
a block that ends in the node: `xpath!(c, "expr", { prepare(); doc.root() })`.

### What may be a context item

`Nav`, `&Nav`, `Doc` or `&Doc` — whatever `IntoContextNode` accepts. A document
supplies its document node (so `//item_tuple` behaves as it does in a query
against that document), and a borrowed navigator is cloned. Atomic values and
sequences are not context items; bind them by name.

### What may be bound

Anything `IntoXPathValue` accepts:

| Rust type | Bound as |
| --- | --- |
| `bool`, `i32`, `i64`, `f64`, `BigInt`, `rust_decimal::Decimal` | that atomic value |
| `&str`, `String` | `xs:string` |
| `XmlValue`, `&XmlValue` | that atomic value — a date, a duration, a QName, whatever a previous evaluation returned |
| `Nav<'a>`, `&Nav<'a>` | one node |
| `Doc<'a>`, `&Doc<'a>` | its document node |
| `Value<'a>`, `&Value<'a>`, `XPathValue<Nav<'a>>` | the sequence as it stands, unchanged |
| `Vec<Nav<'a>>`, `&[Nav<'a>]`, `Vec<XmlItem<Nav<'a>>>`, `Vec<XmlValue>` | a sequence |
| `Option<T>` where `T: IntoXPathValue` | `T`, or the empty sequence for `None` |
| `()` | the empty sequence |

Binding a node or a sequence costs nothing beyond the variable slot: no copy,
no serialization, and the node keeps its identity, so `$i/itemno` and
`is` comparisons behave as they would inside a query.

Variable names are Rust identifiers, which means a *prefixed* external variable
(`$p:x`) cannot be written with the macro. None of the eighteen use-case
queries needs one; a host that does can call `Composer::eval` directly, which
takes any name (§12).

### What comes back

A `Value` is the engine's own sequence, with nothing atomized, copied or
flattened. It converts exactly as far as you ask, and refuses rather than
guessing:

| Method | Result | Refuses |
| --- | --- | --- |
| `nodes()` | `Vec<Nav>` in order | an atomic item — `NotANode` (silently skipping it would hide an expression bug) |
| `atomics()` | `Vec<XmlValue>`, atomizing nodes | — |
| `boolean()` | the effective boolean value | what the EBV rules refuse |
| `string()` | the string value of one item, `""` for empty | two or more items — `NotSingleton { len }` |
| `number()` | one atomized item as `f64` | two or more items, or a non-numeric one |
| `key()` | `Option<XmlValue>` for `order by`: empty is `None` | two or more items — XPTY0004 |
| `len()`, `is_empty()`, `iter()`, `inner()`, `into_inner()` | the sequence itself | — |

### Caching

Each call site's expression is compiled once per composer and then only
evaluated. The composer also keeps one regular-expression cache
(`xpath::RegexCache`) for all its evaluations, so a pattern of `fn:matches`,
`fn:replace` or `fn:tokenize` is compiled once per composer, not once per
evaluation. The cache key is the expression text *and* the external variable
names, both `'static` literals coming straight from the macro, so the cost per
evaluation is a short string hash, a small `Vec` of bindings and a dynamic
context — no parsing. A call site inside a nested loop is therefore fine. The
compiled expression is tied to the composer's name table through interned
names, which is why the cache lives on the composer and not in a `static`.

## 6. `form!` — Describing Result Elements

```text
form      ::= '(' name item* ')'
name      ::= ident | ident '::' ident | string-literal
item      ::= ':' attr-name attr-value            -- an attribute
            | ':' 'xmlns' string-literal          -- a default namespace declaration
            | ':' 'xmlns' '::' ident string-literal
            | ':' 'xmlns' ':' ident string-literal
            | string-literal                      -- literal text
            | '@' '{' rust-expr '}'               -- text from a Display value
            | '^' '{' rust-expr '}'               -- one content value
            | '..' '^' '{' rust-expr '}'          -- a spliced sequence
            | '..' '?' '^' '{' rust-expr '}'      -- a spliced fallible sequence
            | '#' 'comment' string-literal
            | '#' 'pi' string-literal string-literal
            | form                                -- a child element
attr-name ::= ident | ident '::' ident | ident ':' ident | string-literal
attr-value::= string-literal | '@' '{' rust-expr '}' | '^' '{' rust-expr '}'
```

The shape is a Lisp form: a name, then items. Attributes are named operands
(`:id "b1"`), which leaves `@{…}` with exactly one meaning — evaluate a Rust
expression now and capture the text.

**Write the invocation with braces: `form! { ( … ) }`.** All three delimiters
expand identically, but a form's tokens also parse as a Rust expression —
`(a ^{ x } (b))` is a bit-xor and a call — so rustfmt reads a *parenthesized*
invocation as arguments and reflows it into that shape: `form!((a ^{ x } (b)))`
comes back as `form!((a ^ { x }(b)))`, and the element structure is gone. A
brace-delimited invocation is left alone, so the form keeps the layout you gave
it and no `#[rustfmt::skip]` is needed anywhere. `xpath!(…)` stays
parenthesized: its arguments *are* Rust expressions, and rustfmt formats them
correctly.

### The escapes

| Written | Means |
| --- | --- |
| `"text"` | literal text; two adjacent literals concatenate with nothing between them |
| `@{ e }` | `e` formatted with `Display`, captured now. Rust's formatting, *not* the XDM canonical lexical form |
| `^{ e }` | one content value: a `Value`, a node, an atomic value, a nested `Form`, an `Option` of any of those — whatever `IntoContent` accepts |
| `..^{ e }` | an `IntoIterator` of those, spliced in order; an empty iterator adds nothing |
| `..?^{ e }` | an `IntoIterator<Item = Result<T, ComposeError>>`; the first failure returns from the enclosing function |
| `:name "v"` / `:name @{ e }` / `:name ^{ e }` | an attribute — literal, `Display`, or a sequence atomized and joined with single spaces |

`Result` deliberately has no `IntoContent` implementation. A failure must not
be able to become missing content, so `..^{}` will not compile over an iterator
of results and `..?^{}` is the spelling that propagates them.

A form is **eager and owned**. Building it evaluates every escape immediately,
left to right, exactly once — which is why a `?` inside an escape returns from
the enclosing function, and why the finished `Form` holds no closures and can
be returned, stored and spliced later. What it does not do is build any
document node; that is `build`'s job (§9).

### Names

**A prefixed element name is written with two colons**: `(p::title "One")`.
Rust's tokens carry no whitespace, so `(book :id "b1")` and `(p:title "One")`
are the same token shape — rather than guess, the macro reads one colon after
the element name as the start of an attribute in *every* case. `(p:root
(child))`, where no attribute value follows, does not compile, and the error
names the `p::root` spelling. After the `:` marker the ambiguity is gone, so an
attribute or declaration takes either spelling: `:xml::lang "en"` and
`:xml:lang "en"` are the same attribute.

A name that is not a Rust identifier is a string literal: `("bid-count" "7")`.

Prefixes are resolved **when the document is built**, not when the macro
expands: a form's own `:xmlns` declarations first, innermost out, then the
composer's table — the three predeclared prefixes of §2 included, so
`(xs::schema …)` and `:xsi:type "…"` resolve and are declared on the output
like any other. `xml` is always bound and never declared. An
unprefixed element name takes the default element namespace; an unprefixed
attribute is always in no namespace. A prefix nothing binds is
`UnboundPrefix { prefix, at }`, and a literal name that is not an `NCName` or
`prefix:NCName` is `InvalidName` — both at build time, which is what keeps
`form!` a `Form` rather than a `Result`.

### Namespaces, and the fixup

```rust
use bumpalo::Bump;
use xsd_schema::compose::Composer;
use xsd_schema::document::SerializeOptions;
use xsd_schema::form;
use xsd_schema::namespace::NameTable;

let arena = Bump::new();
let names = NameTable::new();
let c = Composer::new(&arena, &names).with_namespace("q", "urn:q");

let doc = c.build(form! {
    (e::root :xmlns::e "urn:e" :xml::lang "en"
        (q::child :xmlns "urn:d" (leaf "text"))
    )
})?;
let pretty = SerializeOptions { indent: Some(2), ..SerializeOptions::default() };
assert_eq!(
    doc.to_xml(&pretty)?,
    r#"<e:root xmlns:e="urn:e" xml:lang="en">
  <q:child xmlns="urn:d" xmlns:q="urn:q">
    <leaf>text</leaf>
  </q:child>
</e:root>"#
);
# Ok::<(), xsd_schema::compose::ComposeError>(())
```

The declarations are not written where you declared them — they are written
where they are *needed*. `xmlns:q` appears on `q:child` because that is the
element whose name needs it, even though `q` was bound on the composer; a
declaration that only repeats what is already in scope is dropped; an element
that leaves an inherited default namespace gets `xmlns=""`. Spliced nodes go
through the same fixup, so a copied element carries the declarations its own
names need even when the source inherited them from ancestors the copy does not
have. The result is the invariant worth having: **every name in the output
resolves to the namespace it had when you wrote it.**

### Attributes, and a conditional

An attribute value from a sequence is atomized and joined with single spaces,
which is the constructor rule for attribute content:

```rust
use bumpalo::Bump;
use xsd_schema::compose::Composer;
use xsd_schema::document::SerializeOptions;
use xsd_schema::namespace::NameTable;
use xsd_schema::{form, xpath};

let arena = Bump::new();
let names = NameTable::new();
let c = Composer::new(&arena, &names);
let stock = c.load_str("<stock><part name='bolt'/><part name='nut'/><part name='washer'/></stock>")?;

let tags = xpath!(c, "//part/@name", stock)?;
let summary = form! { (parts :count @{ tags.len() } :names ^{ &tags }) };
assert_eq!(
    c.build(summary)?.to_xml(&SerializeOptions::default())?,
    r#"<parts count="3" names="bolt nut washer"/>"#
);
# Ok::<(), xsd_schema::compose::ComposeError>(())
```

A conditional constructor is a Rust `if` that chooses between two forms — this
is the sixteenth use-case query, whose XQuery reads
`if (empty($b)) then <status>inactive</status> else <status>active</status>`:

```rust
use xsd_schema::compose::{ComposeError, Composer, Form, Nav, Value};
use xsd_schema::{form, xpath};

fn user_status<'a>(c: &Composer<'a>, u: Nav<'a>, b: &Value<'a>) -> Result<Form<'a>, ComposeError> {
    let status = if b.is_empty() {
        form! { (status "inactive") }
    } else {
        form! { (status "active") }
    };
    Ok(form! { (user ^{ xpath!(c, "userid", &u)? } ^{ status }) })
}
```

An `Option<Form>` works the same way, with `None` contributing nothing.

### Order, comments and PIs

Attributes and declarations belong before content. The macro does not enforce
it — a `Form` keeps its attributes and its content in separate lists, so a
`:attr` written after content is still an attribute — but a *spliced attribute
node* arriving after content is refused, which is the same error XQuery raises.
Comments and processing instructions are `#comment "text"` and
`#pi "target" "data"`.

## 7. Pipelines — The FLWOR Clauses

`pipe::Pipe<I>` is a thin wrapper over an ordinary iterator of
`Result<T, ComposeError>`. It exists because Rust's `filter` takes a `bool`
while XPath evaluation returns a `Result`, and because the obvious bridges —
`.ok()`, `filter_map(Result::ok)`, `flatten()` — all make errors disappear.

| Entry point | Rows it yields |
| --- | --- |
| `pipe::items(value)` | every XDM item of a `Value`, in order, nothing atomized or copied |
| `pipe::nodes(value)` | every node, in order; an atomic item is `NotANode` *at that position*, after the rows before it |
| `pipe::from(iter)` | an ordinary `IntoIterator`, every item a successful row |
| `pipe::from_results(iter)` | an iterator that already yields results, failures kept |

| Adapter | Clause it is | Contract |
| --- | --- | --- |
| `try_map(f)` | `let`, and `return` | `FnMut(T) -> Result<U, _>`; one row out per row in — a `let` returns a tuple carrying the extra value |
| `try_filter(p)` | `where` | `FnMut(&T) -> Result<bool, _>`; keep, skip, or fail |
| `try_flat_map(f)` | a nested `for` | `FnMut(T) -> Result<J, _>` where `J` yields results; each inner sequence is exhausted before the outer one advances |
| `order_by(dir, empty, key)` | `order by` | consumes the input, sorts it by XQuery's rules (§8), and hands back a pipe over the sorted rows — a `Result`, because a key can fail |

A `Pipe` is an `Iterator`, so `collect::<Result<Vec<_>, ComposeError>>()?` is
always available when you want the vector.

Errors travel to the consumer instead of turning into missing rows, and a pipe
is **fused after the first one**: no later predicate, projection or inner
iterator is called. All callbacks run in written order; nothing is reordered
and nothing runs in parallel.

### What is not lazy

The adapters are pulled on demand. Three things stay eager:

1. an expression feeding a source has already been evaluated when the source is
   created, and its sequence is already materialized — `xpath!` is not lazy and
   the pipe does not make it so;
2. `order_by` buffers its input, which is exactly what makes it a stable sort
   with one key evaluation per row;
3. a completed form tree is a tree, and `..?^{}` consumes its iterator
   immediately while building it.

So what pipelines remove is the intermediate `Vec` between stages, and what
they add is that the query's structure is visible in the code. They do not
promise bounded-memory streaming, incremental evaluation, or a form-free
result. No closure or iterator is ever stored inside `Content`.

A row is a host value. An XDM sequence held in a row stays one value until a
splice or an explicit flattening consumes it; filtering does not change XPath
focus, `position()` or `last()`; order and duplicates are preserved unless you
ask for something else.

Across a splice boundary the content rules still apply as if the items had come
from one expression: two adjacent atomic values are separated by a single
space even when they arrive from different splices, an atomic value next to a
node is separated by nothing, an empty splice does not break that adjacency,
and two plain Rust string literals still concatenate with nothing between them.

## 8. Ordering

```rust
use xsd_schema::compose::order::{Direction, EmptyOrder};
use xsd_schema::compose::{pipe, ComposeError, Composer, Doc, Nav};
use xsd_schema::xpath;

fn by_itemno<'a>(c: &Composer<'a>, items: Doc<'a>) -> Result<Vec<Nav<'a>>, ComposeError> {
    pipe::nodes(xpath!(c, "//item_tuple", items)?)
        .order_by(Direction::Ascending, EmptyOrder::Least, |row| {
            xpath!(c, "itemno", row)?.key() // the key
        })?
        .collect()
}
```

The key closure returns `Result<Option<XmlValue>, ComposeError>` — `None` is
XQuery's empty key — and `Value::key()` is the usual way to produce one from an
expression.

The comparison is XDM's, not Rust's:

| Rule | Behaviour |
| --- | --- |
| `xs:untypedAtomic` keys | cast to `xs:string` first, so unschema'd element content sorts as text |
| comparison | the engine's own value comparison under the codepoint collation |
| incomparable types | `XPTY0004`, surfaced as `ComposeError::XPath` — never an arbitrary order |
| an empty key | least under `EmptyOrder::Least`, greatest under `EmptyOrder::Greatest` |
| `NaN` | immediately next to the empty key — just after it for `Least`, just before it for `Greatest` — so it precedes every real number either way |
| `Direction::Descending` | reverses the whole ordering, empty key included |
| stability | guaranteed: keys are computed once, a permutation is sorted, the rows are moved into place. XQuery's `stable` is the only behaviour offered, and the only one needed |

`order::sort_by_key` and `order::sort_by_keys` are the same machinery for a
vector you already own; `sort_by_keys` takes several `KeySpec`s and compares
them lexicographically, each with its own direction and empty placement.

An untypedAtomic key means `itemno` values sort as strings — which is right for
the use case (`"1001"` before `"1007"`) and worth remembering when the values
are numbers of unequal length. Ask for a number when you want numeric order:
`xpath!(c, "number(itemno)", i)?.key()`.

## 9. Building And Re-Querying

```rust
use xsd_schema::compose::{ComposeError, Composer, Doc, Form};

fn build_both<'a>(
    c: &Composer<'a>,
    form: Form<'a>,
    rows: Vec<Form<'a>>,
) -> Result<(Doc<'a>, Doc<'a>), ComposeError> {
    let doc = c.build(form)?;            // exactly one top-level element
    let seq = c.build_sequence(rows)?;   // any number of top-level nodes
    Ok((doc, seq))
}
```

`build` is the common case and refuses anything else with
`BuildShape { top_level }`. `build_sequence` is how a Rust function that stands
in for a user-defined function returning `element()*` becomes **queryable**:
build the sequence, then bind its children.

That is the twelfth use-case query, whose XQuery declares
`local:bid_summary() as element()*` and then queries `$bid_counts/nbids`:

```rust
use xsd_schema::compose::{pipe, ComposeError, Composer, Doc, Value};
use xsd_schema::{form, xpath};

fn bid_summary<'a>(c: &Composer<'a>, bids: Doc<'a>) -> Result<Doc<'a>, ComposeError> {
    let rows = pipe::from(xpath!(c, "distinct-values(//itemno)", bids)?.atomics()?)
        .try_map(|i| {
            let b = xpath!(c, "//bid_tuple[itemno = $i]", bids, i = &i)?;
            Ok(form! {
                (bid_count
                    (itemno ^{ i })
                    (nbids ^{ xpath!(c, "count($b)", b = &b)? }))
            })
        })
        .collect::<Result<Vec<_>, ComposeError>>()?;
    c.build_sequence(rows)
}

fn max_bids<'a>(c: &Composer<'a>, bids: Doc<'a>) -> Result<Value<'a>, ComposeError> {
    let summary = bid_summary(c, bids)?;
    let bid_counts = summary.children();                // Vec<Nav>
    xpath!(c, "max($bid_counts/nbids)", bid_counts = bid_counts.as_slice())
}
```

The constructed nodes are ordinary nodes: they can be bound as variables,
navigated, atomized and spliced into the next result. They are untyped unless
the composer has a schema set, so `max` over them atomizes to
`xs:untypedAtomic` and compares numerically the way it does over any unschema'd
document.

Emission itself is one walk of the form tree: resolve each name, compute the
declarations the element must carry, write the attributes (a sequence atomized
and joined with spaces), then the content in order — text as text, a nested
form by recursion, a spliced value through the content rules, which copy nodes
with new identities, flatten document nodes, separate adjacent atomic values
with one space and contribute nothing at all for an empty sequence.

## 10. Serialization

```rust
use xsd_schema::compose::{ComposeError, Doc};
use xsd_schema::document::SerializeOptions;

fn write_out(doc: Doc<'_>, file: &mut std::fs::File) -> Result<(), ComposeError> {
    let compact = doc.to_xml(&SerializeOptions::default())?;
    let pretty = doc.to_xml(&SerializeOptions { indent: Some(2), ..Default::default() })?;
    doc.write_xml(file, &SerializeOptions::default())?;    // no intermediate String
    let _ = (compact, pretty);
    Ok(())
}
```

| Option | Default | Meaning |
| --- | --- | --- |
| `indent` | `None` | `None` is compact; `Some(n)` adds a line break plus `n` spaces per level; `Some(0)` gives the breaks alone |
| `xml_declaration` | `false` | write `<?xml version="1.0" encoding="UTF-8"?>` |
| `standalone` | `None` | add ` standalone="yes"`/`"no"` to that declaration |

The whitespace contract in two sentences. Compact mode adds no whitespace and
removes none, which is what makes `parse → serialize` an identity on the tree.
Formatted mode adds a break only at a child boundary next to an element, never
inside a container that has any text child of its own, and never where
`xml:space="preserve"` is in force — so every existing character survives, but
the added whitespace becomes real text nodes if the output is parsed again,
which is why compact is the mode for a round trip and formatted is a
presentation choice.

Output is UTF-8. Escaping follows Canonical XML's rules — the ones designed so
that a re-parse cannot change a value back — and content that no well-formed
document could hold is refused rather than mangled: a character outside XML
1.0's `Char` production, a comment containing `--`, a PI whose target is `xml`
or whose data contains `?>`, a name whose prefix is not bound in the scope the
output creates.

## 11. Errors And Diagnostics

`ComposeError` is one `thiserror` enum with `From` impls for everything it
wraps, so `?` works throughout a FLWOR body.

| Variant | When | What it carries |
| --- | --- | --- |
| `XPath { source, expr }` | an expression failed to compile or evaluate | **the expression text**, so the message names the call site: `the expression "max($b/bid)" failed: …` |
| `Document(_)` | a load or a build was refused by the document builder | the builder's error |
| `Copy { source, at }` | a spliced value could not be copied | **the form's path**, spelled like a step list: `result/item_tuple[3]` — the position appears only for a repeated same-named child |
| `Serialize(_)` | the result could not be written as XML | the serializer's error |
| `Io(_)` | `load_file`, or a writer | the I/O error |
| `UnboundPrefix { prefix, at }` | nothing binds a prefix a form used | the prefix and the form's path |
| `InvalidName(_)` | a literal name is not an `NCName`/`prefix:NCName` | the name |
| `NotANode` | an atomic item turned up where a node was required | — |
| `NotSingleton { len }` | several items where one was required | how many |
| `BuildShape { top_level }` | `build` was handed something other than one top-level element | how many there were |

Two of those are worth calling out, because they are the ones that make a
mistake findable rather than mysterious: an evaluation failure quotes the
expression, and a copy or naming failure quotes the path of the form it
happened in. The composer knows both for free — it has the text and it owns the
tree.

## 12. Limits And Non-Goals

v1 is deliberately small. These are the boundaries, with what to do instead:

| Not in v1 | Why, and what to do |
| --- | --- |
| **Computed names** — XQuery's `element { $name } { … }` | no use case needed one. A `Form` is built through `Form::new(Name::parse(text)?)`, so a computed name is available through the API, just not through the macro |
| **Prefixed external variables** (`$p:x`) in `xpath!` | the macro takes Rust identifiers. `Composer::eval(src, vars, ctx, bind)` takes any name; it is `#[doc(hidden)]` because it is what the macros expand to, but it is public and stable |
| **Collations other than codepoint** | `order_by` and every string comparison use the codepoint collation. There is no collation parameter to pass |
| **Threads** | a `Composer` is `!Send` and `!Sync`. Compose per thread, and move the resulting `String` |
| **A generic navigator** | `Nav<'a>` is the document buffer's navigator. A composer generic over `DomNavigator` is mechanical but not written, so nodes from another tree implementation cannot be spliced directly — serialize and reload, or wait for the generalization |
| **Deep form nesting** | `form!` is a token-tree muncher, so nesting depth *and* item count cost macro expansion steps, which `recursion_limit` (128 by default) bounds. About thirty levels deep or a hundred items in one form needs `#![recursion_limit = "256"]` in the crate that writes it |
| **XQuery semantics in full** | duplicate attribute names in spliced content keep the last one rather than raising XQDY0025; `xml:base` is stored but not interpreted; dynamic-error *timing* is not promised, since materialization separates row production from projection |

## 13. Where The Code Lives

| File | What is in it |
| --- | --- |
| `src/compose/composer.rs` | `Composer`, `Doc`, the expression cache, `load_*`, `build*`, `eval` |
| `src/compose/value.rs` | `Value`, `IntoXPathValue`, `IntoContextNode` |
| `src/compose/form.rs` | `Form`, `Content`, `AttrValue`, `Name`, `IntoContent` |
| `src/compose/emit.rs` | `Emitter`: a form tree into a document builder |
| `src/compose/order.rs` | `Direction`, `EmptyOrder`, `KeySpec`, `sort_by_key(s)` |
| `src/compose/pipe.rs` | `Pipe`, the sources and the `try_*` adapters |
| `src/compose/error.rs` | `ComposeError` |
| `src/compose/macros.rs` | `xpath!` and `form!`, `macro_rules!` only |
| `tests/compose_public_api.rs` | the module used from outside the crate |
| `tests/compose_usecase_r.rs` | all eighteen use-case queries against their oracle |
| `examples/xquery_without_xquery.rs` | RQ2, RQ3 and RQ9, runnable |

The module obeys one structural rule: **it uses only the public API of the rest
of the crate.** The macros expand to `$crate::compose::…` paths exclusively,
and `tests/compose_public_api.rs` compiles the module's own examples from
outside the crate, so a reach into a private item fails that test immediately.
The serializer (`document::serialize`) and the copy helper
(`document::copy`) that it builds on are ordinary `xsd11` modules, documented
in [Introduction §8](INTRODUCTION.md#8-writing-documents-out-and-copying-subtrees).
