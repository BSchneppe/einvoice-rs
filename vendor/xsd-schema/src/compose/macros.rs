//! The two macros: [`xpath!`](crate::xpath!) and [`form!`](crate::form!).
//!
//! Everything in this module is `macro_rules!`, and every expansion goes
//! through `$crate::compose::…` paths, so the macros need nothing that a host
//! crate cannot reach itself.
//!
//! # Importing them
//!
//! Both macros live at the crate root:
//!
//! ```
//! use xsd_schema::{form, xpath};
//! ```
//!
//! That is the canonical import, and the one every example here uses. The
//! macros are *not* re-exported from `compose`: `compose::form` is already the
//! module holding [`Form`](crate::compose::Form), and `xsd_schema::xpath` is
//! already the XPath engine's module, so re-exporting the macros there would
//! put two different things behind one path for no gain. Names in the macro
//! namespace and names in the type namespace never collide, which is why
//! `use xsd_schema::xpath;` brings in both the module and the macro and both
//! keep working.
//!
//! # What they expand to
//!
//! [`xpath!`](crate::xpath!) is one call to the composer's cached evaluator:
//! the expression text and the variable names are `'static` literals, so a
//! call site inside a loop compiles its expression once and then only
//! evaluates.
//!
//! [`form!`](crate::form!) is a token-tree muncher over the form grammar; it
//! builds an owned [`Form`](crate::compose::Form) eagerly, evaluating every
//! escape exactly once, left to right, as the form is constructed. No closure
//! and no iterator survives into the form, which is why a `?` inside an
//! escape returns from the enclosing function and why the finished form can
//! be stored, returned and spliced later.
//!
//! In a form, a prefixed name is written `p::local`: one colon after the
//! element name is always the attribute marker, so `(p:title "One")` is the
//! element `p` with `title="One"`, and `(p::title "One")` is the element
//! `p:title`.
//!
//! # Expansion depth
//!
//! Both the nesting depth of a form and the number of items in one form cost
//! macro expansion steps, which `recursion_limit` (128 by default) bounds. A
//! form nested more than about thirty levels deep, or one with more than about
//! a hundred items, needs `#![recursion_limit = "256"]` in the consuming
//! crate. Neither is close for the compositions this layer is built for.

/// Evaluates an XPath expression on a [`Composer`](crate::compose::Composer).
///
/// ```text
/// xpath!(c, "expr")                             // no context item, no variables
/// xpath!(c, "expr", node)                       // a context item
/// xpath!(c, "expr", i = &item, b = &bids)       // the external variables $i and $b
/// xpath!(c, "expr", items, i = &item)           // a context item, then variables
/// ```
///
/// The result is `Result<`[`Value`](crate::compose::Value)`,
/// `[`ComposeError`](crate::compose::ComposeError)`>`, so a call site ends in
/// `?` and then asks the value for what it wants — `nodes`, `atomics`,
/// `boolean`, `string`, `number`, `key`.
///
/// # The argument order, and why it is a rule
///
/// The expression is always a string literal. A context item, when there is
/// one, is always the **third** argument, directly after it; everything after
/// that is a `name = value` binding, and an optional trailing comma is
/// accepted everywhere.
///
/// `name = value` is also Rust's assignment expression, so the bindings-only
/// shape is matched *before* the context shape: `xpath!(c, "$node", node =
/// value)` binds `$node` and leaves the context item empty. A named binding
/// never implies a context item, and `xpath!(c, "expr", node)` never binds
/// anything. A context item after a binding, or two of them, is a compile
/// error naming the expected order; for preparatory work, use a block that
/// ends in the node — `xpath!(c, "expr", { prepare(); doc.root() })`.
///
/// # What may be a context item
///
/// [`Nav`](crate::compose::Nav), `&Nav`, [`Doc`](crate::compose::Doc) or
/// `&Doc` — anything [`IntoContextNode`](crate::compose::IntoContextNode)
/// accepts. A document supplies its document node, a borrowed navigator is
/// cloned. Binding values are anything
/// [`IntoXPathValue`](crate::compose::IntoXPathValue) accepts, which is every
/// atomic Rust scalar, every XDM value, a node, a document, a sequence, and
/// `Option` of any of those. Variable names are Rust identifiers; a prefixed
/// external variable needs
/// [`Composer::eval`](crate::compose::Composer::eval) directly.
///
/// # Evaluation
///
/// Each supplied expression is evaluated exactly once, in the order written:
/// the composer, then the context item, then the binding values from left to
/// right. Every one of them appears exactly once in the expansion, with the
/// composer and the context item held in temporaries of the macro's own, so
/// no conversion can repeat an evaluation or reorder a side effect.
///
/// # Examples
///
/// All four call forms:
///
/// ```
/// use bumpalo::Bump;
/// use xsd_schema::compose::{ComposeError, Composer};
/// use xsd_schema::namespace::NameTable;
/// use xsd_schema::xpath;
///
/// let arena = Bump::new();
/// let names = NameTable::new();
/// let c = Composer::new(&arena, &names);
/// let items = c.load_str("<items><item n='1'/><item n='2'/></items>")?;
///
/// assert_eq!(xpath!(c, "1 + 1")?.string()?, "2");
/// assert_eq!(xpath!(c, "count(//item)", items)?.string()?, "2");
/// assert_eq!(xpath!(c, "$n * 2", n = 21)?.string()?, "42");
/// assert_eq!(
///     xpath!(c, "count(//item[@n = $n])", items, n = "2")?.string()?,
///     "1",
/// );
/// # Ok::<(), ComposeError>(())
/// ```
///
/// A binding is not a context item:
///
/// ```
/// use bumpalo::Bump;
/// use xsd_schema::compose::{ComposeError, Composer};
/// use xsd_schema::namespace::NameTable;
/// use xsd_schema::xpath;
///
/// let arena = Bump::new();
/// let names = NameTable::new();
/// let c = Composer::new(&arena, &names);
/// let doc = c.load_str("<items><item/></items>")?;
/// let node = doc.document_element().expect("a document element");
///
/// // `node = …` binds $node …
/// assert_eq!(xpath!(c, "name($node)", node = &node)?.string()?, "items");
/// // … and the same value as the context item needs no name.
/// assert_eq!(xpath!(c, "name()", &node)?.string()?, "items");
/// # Ok::<(), ComposeError>(())
/// ```
///
/// Inside a pipeline closure, where most real call sites are:
///
/// ```
/// use xsd_schema::compose::{pipe, ComposeError, Composer};
/// use xsd_schema::document::SerializeOptions;
/// use xsd_schema::namespace::NameTable;
/// use xsd_schema::{form, xpath};
/// use bumpalo::Bump;
///
/// let arena = Bump::new();
/// let names = NameTable::new();
/// let c = Composer::new(&arena, &names);
/// let stock = c.load_str("<stock><part qty='7'>bolt</part><part qty='0'>nut</part></stock>")?;
///
/// let rows = pipe::nodes(xpath!(c, "//part", stock)?)
///     .try_filter(|p| xpath!(c, "@qty > 0", p)?.boolean())
///     .try_map(|p| Ok(form! { (part ^{ xpath!(c, "string()", &p)? }) }));
///
/// assert_eq!(
///     c.build(form! { (in_stock ..?^{ rows }) })?.to_xml(&SerializeOptions::default())?,
///     "<in_stock><part>bolt</part></in_stock>",
/// );
/// # Ok::<(), ComposeError>(())
/// ```
///
/// A context item after a binding does not compile:
///
/// ```compile_fail
/// use bumpalo::Bump;
/// use xsd_schema::compose::{ComposeError, Composer};
/// use xsd_schema::namespace::NameTable;
/// use xsd_schema::xpath;
///
/// let arena = Bump::new();
/// let names = NameTable::new();
/// let c = Composer::new(&arena, &names);
/// let doc = c.load_str("<a><b/></a>")?;
/// let node = doc.root();
///
/// // The context item belongs before the bindings.
/// let wrong = xpath!(c, "name($x)", x = &node, node)?;
/// # Ok::<(), ComposeError>(())
/// ```
///
/// Neither does a second context item:
///
/// ```compile_fail
/// use bumpalo::Bump;
/// use xsd_schema::compose::{ComposeError, Composer};
/// use xsd_schema::namespace::NameTable;
/// use xsd_schema::xpath;
///
/// let arena = Bump::new();
/// let names = NameTable::new();
/// let c = Composer::new(&arena, &names);
/// let doc = c.load_str("<a><b/></a>")?;
///
/// // One context item only; bind the other value by name.
/// let wrong = xpath!(c, "name()", doc.root(), doc.root())?;
/// # Ok::<(), ComposeError>(())
/// ```
#[macro_export]
macro_rules! xpath {
    // The plain form and the bindings. This arm comes first so that
    // `name = value` is always a binding and never Rust's assignment
    // expression in the context-item position.
    ($c:expr, $src:literal $(, $name:ident = $val:expr)* $(,)?) => {{
        // Borrowed once, so the composer expression cannot run twice; the
        // method call derefs whatever `&$c` turns out to be, which is how a
        // `Composer`, a `&Composer` and a guard all work here.
        #[allow(clippy::needless_borrow)]
        let __xpath_composer = &$c;
        __xpath_composer.eval(
            $src,
            &[$(::core::stringify!($name)),*],
            ::core::option::Option::None,
            ::std::vec![$((
                ::core::stringify!($name),
                $crate::compose::IntoXPathValue::into_xpath_value($val),
            )),*],
        )
    }};

    // A context item, then bindings.
    ($c:expr, $src:literal, $ctx:expr $(, $name:ident = $val:expr)* $(,)?) => {{
        // Borrowed once, so the composer expression cannot run twice; the
        // method call derefs whatever `&$c` turns out to be, which is how a
        // `Composer`, a `&Composer` and a guard all work here.
        #[allow(clippy::needless_borrow)]
        let __xpath_composer = &$c;
        let __xpath_context = $crate::compose::IntoContextNode::into_context_node($ctx);
        __xpath_composer.eval(
            $src,
            &[$(::core::stringify!($name)),*],
            ::core::option::Option::Some(__xpath_context),
            ::std::vec![$((
                ::core::stringify!($name),
                $crate::compose::IntoXPathValue::into_xpath_value($val),
            )),*],
        )
    }};

    // ── Diagnostics ───────────────────────────────────────────────────
    //
    // These arms match the two orders that read as if they should work. They
    // come after the two real arms, so they only ever see calls the real arms
    // refused.

    // A binding first and then something that is not another binding: the
    // context item, written too late. Matched with one binding and a token
    // muncher rather than a repetition followed by `$ctx:expr`, because two
    // fragment options at one position are a `macro_rules!` local ambiguity
    // and would replace this message with one about the matcher.
    ($c:expr, $src:literal, $name:ident = $val:expr, $($rest:tt)*) => {
        ::core::compile_error!(
            "xpath!: the context item comes before the `name = value` bindings — \
             write xpath!(c, \"expr\", node, name = value)"
        )
    };

    ($c:expr, $src:literal, $ctx:expr, $second:expr $(, $name:ident = $val:expr)* $(,)?) => {
        ::core::compile_error!(
            "xpath!: only one context item is allowed, and it is the third argument — \
             write xpath!(c, \"expr\", node, name = value), and bind any further value by name"
        )
    };

    ($($rest:tt)*) => {
        ::core::compile_error!(
            "xpath!: expected xpath!(composer, \"expr\") with an optional context item as the \
             third argument and `name = value` bindings after it"
        )
    };
}

/// Describes a result element: a [`Form`](crate::compose::Form), built eagerly
/// and emitted later by [`Composer::build`](crate::compose::Composer::build).
///
/// ```text
/// form      ::= '(' name item* ')'
/// name      ::= ident | ident '::' ident | string-literal
///                                                   -- one colon after the name is an attribute
/// item      ::= ':' attr-name attr-value            -- an attribute
///             | ':' 'xmlns' string-literal          -- a default namespace declaration
///             | ':' 'xmlns' '::' ident string-literal
///             | ':' 'xmlns' ':' ident string-literal
///             | string-literal                      -- literal text
///             | '@' '{' rust-expr '}'               -- text from a Display value
///             | '^' '{' rust-expr '}'               -- one content value
///             | '..' '^' '{' rust-expr '}'          -- a spliced sequence
///             | '..' '?' '^' '{' rust-expr '}'      -- a spliced fallible sequence
///             | '#' 'comment' string-literal
///             | '#' 'pi' string-literal string-literal
///             | form                                -- a child element
/// attr-name ::= ident | ident '::' ident | ident ':' ident | string-literal
/// attr-value::= string-literal | '@' '{' rust-expr '}' | '^' '{' rust-expr '}'
/// ```
///
/// # The escapes
///
/// | Written | Means |
/// |---|---|
/// | `"text"` | literal text; two adjacent texts concatenate with nothing between them |
/// | `@{ e }` | `e` formatted with `Display`, captured now — Rust's formatting, not XDM's canonical lexical form |
/// | `^{ e }` | one content value: anything [`IntoContent`](crate::compose::IntoContent) accepts — a query result, a node, an atomic value, a nested [`Form`](crate::compose::Form), an `Option` of any of those |
/// | `..^{ e }` | an `IntoIterator` of those, spliced in order; an empty iterator adds nothing |
/// | `..?^{ e }` | an `IntoIterator<Item = Result<T, ComposeError>>`; the first failure returns from the enclosing function |
/// | `:name "v"`, `:name @{ e }`, `:name ^{ e }` | an attribute — literal, `Display`, or a sequence atomized and joined with single spaces |
///
/// `Result` deliberately has no [`IntoContent`](crate::compose::IntoContent)
/// implementation, so `..^{}` cannot quietly turn a failure into missing
/// content; `..?^{}` is the spelling that propagates it.
///
/// # Write the invocation with braces
///
/// **`form! { ( … ) }` is the spelling to use**, and it is what every example
/// here and in the guide uses. All three delimiters expand identically —
/// `macro_rules!` does not care which one an invocation carries — but a form's
/// tokens also happen to parse as a Rust expression: `(a ^{ x } (b))` is a
/// bit-xor and a call. rustfmt reads a parenthesized invocation as arguments
/// and reflows them into that shape, so `form!((a ^{ x } (b)))` comes back as
/// `form!((a ^ { x }(b)))` and the element structure stops being visible. A
/// brace-delimited invocation is left alone, which is all it takes: the form
/// keeps the layout you gave it, with no `#[rustfmt::skip]` anywhere.
///
/// # Names
///
/// Prefixes are resolved when the document is built — against the form's own
/// `:xmlns` declarations, innermost first, then the composer's
/// [`with_namespace`](crate::compose::Composer::with_namespace) table.
/// An unprefixed element name takes the default element namespace; an
/// unprefixed attribute is in no namespace. `xml` is always bound.
///
/// **A prefixed element name is written with two colons** — `(p::title
/// "One")` — or as a string literal — `("p:title" "One")`, which is also how
/// a name that is not a Rust identifier is written: `("bid-count" "7")`.
///
/// One colon is the attribute marker, always. Rust's tokens carry no
/// whitespace, so `(book :id "b1")` and `(p:title "One")` are the same shape,
/// and rather than guess, the macro reads a single colon after the element
/// name as the start of an attribute in every case: `(p:title "One")` and
/// `(p :title "One")` are both the element `p` with `title="One"`, and
/// `(p:root (child))` — where no attribute value follows — does not compile,
/// with a message naming the `p::root` spelling. Nothing is ever reinterpreted
/// silently.
///
/// After that marker the ambiguity is gone, so an attribute name and a
/// declaration take either spelling: `:xml::lang "en"` and `:xml:lang "en"`
/// are the same attribute, `:xmlns::p "urn:x"` and `:xmlns:p "urn:x"` the same
/// declaration.
///
/// A literal name that is not an `NCName` or `prefix:NCName` is
/// [`ComposeError::InvalidName`](crate::compose::ComposeError::InvalidName)
/// at build time, and a prefix nothing binds is
/// [`ComposeError::UnboundPrefix`](crate::compose::ComposeError::UnboundPrefix).
///
/// # Order
///
/// Attributes and declarations belong before content. The macro does not
/// enforce that — a [`Form`](crate::compose::Form) keeps its attributes and
/// its content apart — but the emitter does enforce it for a *spliced*
/// attribute node, which is
/// [`ComposeError::Copy`](crate::compose::ComposeError::Copy) when content has
/// already been written.
///
/// Items and escapes are processed left to right, each Rust expression
/// evaluated exactly once, as the form is built.
///
/// # Expansion depth
///
/// The macro is a token-tree muncher, so both the nesting depth of a form and
/// the number of items in one form cost macro expansion steps, which
/// `recursion_limit` (128 by default) bounds. A form nested more than about
/// thirty levels deep, or one carrying more than about a hundred items, needs
/// `#![recursion_limit = "256"]` in the crate that writes it. The compositions
/// this layer is built for are nowhere near either bound.
///
/// # Examples
///
/// Every escape at once:
///
/// ```
/// use bumpalo::Bump;
/// use xsd_schema::compose::{ComposeError, Composer};
/// use xsd_schema::document::SerializeOptions;
/// use xsd_schema::namespace::NameTable;
/// use xsd_schema::{form, xpath};
///
/// let arena = Bump::new();
/// let names = NameTable::new();
/// let c = Composer::new(&arena, &names);
/// let stock = c.load_str("<stock><part>bolt</part><part>nut</part></stock>")?;
///
/// let count = 4;
/// let parts = xpath!(c, "//part", stock)?;
/// let extra = ["washer", "screw"];
///
/// let doc = c.build(form! {
///     (inventory
///         :kind "hardware"                                       // a literal attribute
///         :count @{ count }                                      // an attribute from a Rust value
///         :first ^{ xpath!(c, "//part[1]", stock)? }             // an attribute from a query
///         "in stock: "                                           // literal text
///         @{ count }                                             // text from a Rust value
///         ^{ parts }                                             // two copied elements
///         ..^{ extra.iter().map(|n| form! { (part ^{ *n }) }) }   // a spliced sequence
///         (note :kind "generated" "written by form!")            // a child element
///         #comment " counted "                                   // a comment
///         #pi "sort" "by name"                                   // a processing instruction
///     )
/// })?;
///
/// assert_eq!(
///     doc.to_xml(&SerializeOptions::default())?,
///     concat!(
///         r#"<inventory kind="hardware" count="4" first="bolt">"#,
///         "in stock: 4<part>bolt</part><part>nut</part>",
///         "<part>washer</part><part>screw</part>",
///         r#"<note kind="generated">written by form!</note>"#,
///         "<!-- counted --><?sort by name?></inventory>",
///     ),
/// );
/// # Ok::<(), ComposeError>(())
/// ```
///
/// Namespaces, declared on the form or by the composer:
///
/// ```
/// use bumpalo::Bump;
/// use xsd_schema::compose::{ComposeError, Composer};
/// use xsd_schema::document::SerializeOptions;
/// use xsd_schema::form;
/// use xsd_schema::namespace::NameTable;
///
/// let arena = Bump::new();
/// let names = NameTable::new();
/// let c = Composer::new(&arena, &names).with_namespace("q", "urn:q");
///
/// let doc = c.build(form! {
///     (e::root :xmlns::e "urn:e" :xml::lang "en"
///         (q::child :xmlns "urn:d" (leaf "text"))
///     )
/// })?;
///
/// assert_eq!(
///     doc.to_xml(&SerializeOptions::default())?,
///     concat!(
///         r#"<e:root xmlns:e="urn:e" xml:lang="en">"#,
///         r#"<q:child xmlns="urn:d" xmlns:q="urn:q"><leaf>text</leaf></q:child>"#,
///         "</e:root>",
///     ),
/// );
/// # Ok::<(), ComposeError>(())
/// ```
///
/// A fallible splice, with the `?` returning from the enclosing function:
///
/// ```
/// use bumpalo::Bump;
/// use xsd_schema::compose::{pipe, ComposeError, Composer, Doc};
/// use xsd_schema::document::SerializeOptions;
/// use xsd_schema::namespace::NameTable;
/// use xsd_schema::{form, xpath};
///
/// fn loud<'a>(c: &Composer<'a>, stock: Doc<'a>) -> Result<Doc<'a>, ComposeError> {
///     let rows = pipe::nodes(xpath!(c, "//part", stock)?)
///         .try_map(|p| Ok(form! { (part ^{ xpath!(c, "upper-case(string())", &p)? }) }));
///     // A failure anywhere in `rows` leaves this function instead of
///     // becoming missing content, and `build` is never reached.
///     c.build(form! { (parts ..?^{ rows }) })
/// }
///
/// let arena = Bump::new();
/// let names = NameTable::new();
/// let c = Composer::new(&arena, &names);
/// let stock = c.load_str("<stock><part>bolt</part></stock>")?;
///
/// assert_eq!(
///     loud(&c, stock)?.to_xml(&SerializeOptions::default())?,
///     "<parts><part>BOLT</part></parts>",
/// );
/// # Ok::<(), ComposeError>(())
/// ```
///
/// A single colon after the element name is an attribute, so a prefixed
/// element name written with one does not compile:
///
/// ```compile_fail
/// use bumpalo::Bump;
/// use xsd_schema::compose::{ComposeError, Composer};
/// use xsd_schema::form;
/// use xsd_schema::namespace::NameTable;
///
/// let arena = Bump::new();
/// let names = NameTable::new();
/// let c = Composer::new(&arena, &names).with_namespace("p", "urn:p");
///
/// // `:root` starts an attribute, and `(child)` is not an attribute value.
/// // Write form! { (p::root (child)) } for the element `p:root`.
/// let refused = form! { (p:root (child)) };
/// # Ok::<(), ComposeError>(())
/// ```
///
/// `..^{}` refuses an iterator of results:
///
/// ```compile_fail
/// use bumpalo::Bump;
/// use xsd_schema::compose::{ComposeError, Composer};
/// use xsd_schema::form;
/// use xsd_schema::namespace::NameTable;
///
/// let arena = Bump::new();
/// let names = NameTable::new();
/// let c = Composer::new(&arena, &names);
///
/// let rows: Vec<Result<&str, ComposeError>> = vec![Ok("a")];
/// // `Result` is not content: write `..?^{ rows }` to propagate the failure.
/// let refused = form! { (result ..^{ rows }) };
/// # Ok::<(), ComposeError>(())
/// ```
///
/// … and `..?^{}` needs a function that can carry the failure:
///
/// ```compile_fail
/// use xsd_schema::compose::ComposeError;
/// use xsd_schema::form;
///
/// fn how_many() -> usize {
///     let rows: Vec<Result<&'static str, ComposeError>> = vec![Ok("a")];
///     // There is nothing here for the `?` to return to.
///     let refused = form! { (result ..?^{ rows }) };
///     refused.content().len()
/// }
/// ```
#[macro_export]
macro_rules! form {
    // ── The element name ──────────────────────────────────────────────
    //
    // `p::local` is the prefixed spelling, and it is decided first. A single
    // colon is always the attribute marker: `(book :id "b1")` and
    // `(p:title "One")` are the same token shape, so the arms below look ahead
    // for a complete attribute — a name and a value — and hand everything
    // else to the plain-name arm, where an attribute without a value is
    // refused by name.

    (($prefix:ident :: $local:ident $($items:tt)*)) => {
        $crate::__form_build!(
            $crate::compose::Name::prefixed(
                ::core::stringify!($prefix),
                ::core::stringify!($local),
            ),
            $($items)*
        )
    };

    // An unprefixed name whose first item is an attribute with a plain name.
    (($name:ident : $a:ident $v:literal $($items:tt)*)) => {
        $crate::__form_build!(
            $crate::compose::Name::local(::core::stringify!($name)),
            : $a $v $($items)*
        )
    };
    (($name:ident : $a:ident @ { $e:expr } $($items:tt)*)) => {
        $crate::__form_build!(
            $crate::compose::Name::local(::core::stringify!($name)),
            : $a @ { $e } $($items)*
        )
    };
    (($name:ident : $a:ident ^ { $e:expr } $($items:tt)*)) => {
        $crate::__form_build!(
            $crate::compose::Name::local(::core::stringify!($name)),
            : $a ^ { $e } $($items)*
        )
    };

    // … or with a prefixed one, in either spelling (`:xml::lang`, `:xml:lang`).
    (($name:ident : $ap:ident :: $al:ident $v:literal $($items:tt)*)) => {
        $crate::__form_build!(
            $crate::compose::Name::local(::core::stringify!($name)),
            : $ap :: $al $v $($items)*
        )
    };
    (($name:ident : $ap:ident :: $al:ident @ { $e:expr } $($items:tt)*)) => {
        $crate::__form_build!(
            $crate::compose::Name::local(::core::stringify!($name)),
            : $ap :: $al @ { $e } $($items)*
        )
    };
    (($name:ident : $ap:ident :: $al:ident ^ { $e:expr } $($items:tt)*)) => {
        $crate::__form_build!(
            $crate::compose::Name::local(::core::stringify!($name)),
            : $ap :: $al ^ { $e } $($items)*
        )
    };
    (($name:ident : $ap:ident : $al:ident $v:literal $($items:tt)*)) => {
        $crate::__form_build!(
            $crate::compose::Name::local(::core::stringify!($name)),
            : $ap : $al $v $($items)*
        )
    };
    (($name:ident : $ap:ident : $al:ident @ { $e:expr } $($items:tt)*)) => {
        $crate::__form_build!(
            $crate::compose::Name::local(::core::stringify!($name)),
            : $ap : $al @ { $e } $($items)*
        )
    };
    (($name:ident : $ap:ident : $al:ident ^ { $e:expr } $($items:tt)*)) => {
        $crate::__form_build!(
            $crate::compose::Name::local(::core::stringify!($name)),
            : $ap : $al ^ { $e } $($items)*
        )
    };

    // There is deliberately no `ident : ident` element-name arm: a single
    // colon after the name always starts an attribute, so `(p:root (child))`
    // reaches the attribute arms and is refused there by name, rather than
    // quietly meaning something else than `(p::root (child))`.

    // A name that is not a Rust identifier.
    (($name:literal $($items:tt)*)) => {
        $crate::__form_build!($crate::compose::__name_from_literal($name), $($items)*)
    };

    // The plain case.
    (($name:ident $($items:tt)*)) => {
        $crate::__form_build!(
            $crate::compose::Name::local(::core::stringify!($name)),
            $($items)*
        )
    };

    ($($rest:tt)*) => {
        ::core::compile_error!(
            "form!: expected one parenthesized form — form! { (name item…) }, whose name \
             is `local`, `prefix::local` or a string literal"
        )
    };
}

/// Builds the form once its name is known.
#[doc(hidden)]
#[macro_export]
macro_rules! __form_build {
    ($name:expr, $($items:tt)*) => {{
        let mut __form = $crate::compose::Form::new($name);
        $crate::__form_items!(__form, $($items)*);
        __form
    }};
}

/// Munches a form's items, one statement at a time, left to right.
#[doc(hidden)]
#[macro_export]
macro_rules! __form_items {
    ($f:ident) => {};
    ($f:ident,) => {};

    // ── Namespace declarations ────────────────────────────────────────
    //
    // Before the attribute arms, because `xmlns` is an identifier like any
    // other.

    ($f:ident, : xmlns :: $prefix:ident $uri:literal $($rest:tt)*) => {
        $f.declare(::core::stringify!($prefix), $uri);
        $crate::__form_items!($f, $($rest)*);
    };
    ($f:ident, : xmlns : $prefix:ident $uri:literal $($rest:tt)*) => {
        $f.declare(::core::stringify!($prefix), $uri);
        $crate::__form_items!($f, $($rest)*);
    };
    ($f:ident, : xmlns $uri:literal $($rest:tt)*) => {
        $f.declare("", $uri);
        $crate::__form_items!($f, $($rest)*);
    };
    ($f:ident, : xmlns $($rest:tt)*) => {
        ::core::compile_error!(
            "form!: a namespace declaration takes a string-literal URI — \
             :xmlns \"urn:x\" or :xmlns::p \"urn:x\""
        )
    };

    // ── Attributes ───────────────────────────────────────────────────

    ($f:ident, : $prefix:ident :: $local:ident $($rest:tt)*) => {
        $crate::__form_attr!(
            $f,
            $crate::compose::Name::prefixed(
                ::core::stringify!($prefix),
                ::core::stringify!($local),
            ),
            $($rest)*
        );
    };
    ($f:ident, : $prefix:ident : $local:ident $($rest:tt)*) => {
        $crate::__form_attr!(
            $f,
            $crate::compose::Name::prefixed(
                ::core::stringify!($prefix),
                ::core::stringify!($local),
            ),
            $($rest)*
        );
    };
    ($f:ident, : $name:ident $($rest:tt)*) => {
        $crate::__form_attr!(
            $f,
            $crate::compose::Name::local(::core::stringify!($name)),
            $($rest)*
        );
    };
    ($f:ident, : $name:literal $($rest:tt)*) => {
        $crate::__form_attr!($f, $crate::compose::__name_from_literal($name), $($rest)*);
    };
    ($f:ident, : $($rest:tt)*) => {
        ::core::compile_error!(
            "form!: an attribute name is `local`, `prefix::local` or a string literal — \
             :id \"b1\", :xml::lang \"en\", :\"bid-count\" @{ n }"
        )
    };

    // ── Content ──────────────────────────────────────────────────────

    ($f:ident, $text:literal $($rest:tt)*) => {
        $f.push($crate::compose::Content::Text(::std::string::String::from($text)));
        $crate::__form_items!($f, $($rest)*);
    };
    ($f:ident, @ { $e:expr } $($rest:tt)*) => {
        $f.push($crate::compose::__display_text($e));
        $crate::__form_items!($f, $($rest)*);
    };
    ($f:ident, ^ { $e:expr } $($rest:tt)*) => {
        $f.push($crate::compose::IntoContent::into_content($e));
        $crate::__form_items!($f, $($rest)*);
    };
    ($f:ident, .. ? ^ { $e:expr } $($rest:tt)*) => {
        // The escape expression runs once; the `?` returns from the enclosing
        // function or closure, so a failure skips every later escape and the
        // half-built form is abandoned.
        for __entry in $e {
            let __item = __entry?;
            $f.push($crate::compose::IntoContent::into_content(__item));
        }
        $crate::__form_items!($f, $($rest)*);
    };
    ($f:ident, .. ^ { $e:expr } $($rest:tt)*) => {
        for __item in $e {
            $f.push($crate::compose::IntoContent::into_content(__item));
        }
        $crate::__form_items!($f, $($rest)*);
    };
    ($f:ident, # comment $text:literal $($rest:tt)*) => {
        $f.push($crate::compose::Content::Comment(::std::string::String::from($text)));
        $crate::__form_items!($f, $($rest)*);
    };
    ($f:ident, # pi $target:literal $data:literal $($rest:tt)*) => {
        $f.push($crate::compose::Content::Pi(
            ::std::string::String::from($target),
            ::std::string::String::from($data),
        ));
        $crate::__form_items!($f, $($rest)*);
    };
    ($f:ident, ( $($nested:tt)* ) $($rest:tt)*) => {
        $f.push($crate::compose::Content::Element($crate::form!(($($nested)*))));
        $crate::__form_items!($f, $($rest)*);
    };

    ($f:ident, $($rest:tt)*) => {
        ::core::compile_error!(
            "form!: expected a form item — an attribute (:name value), a namespace \
             declaration (:xmlns \"urn:x\"), text (\"literal\" or @{ e }), content (^{ e }, \
             ..^{ e }, ..?^{ e }), #comment \"…\", #pi \"…\" \"…\", or a nested (form …)"
        )
    };
}

/// Munches one attribute value, then hands the rest back to
/// [`__form_items!`](crate::__form_items).
#[doc(hidden)]
#[macro_export]
macro_rules! __form_attr {
    ($f:ident, $name:expr, $v:literal $($rest:tt)*) => {
        $f.attr(
            $name,
            $crate::compose::AttrValue::Text(::std::string::String::from($v)),
        );
        $crate::__form_items!($f, $($rest)*);
    };
    ($f:ident, $name:expr, @ { $e:expr } $($rest:tt)*) => {
        $f.attr($name, $crate::compose::AttrValue::text($e));
        $crate::__form_items!($f, $($rest)*);
    };
    ($f:ident, $name:expr, ^ { $e:expr } $($rest:tt)*) => {
        $f.attr($name, $crate::compose::AttrValue::value($e));
        $crate::__form_items!($f, $($rest)*);
    };
    ($f:ident, $name:expr, $($rest:tt)*) => {
        ::core::compile_error!(
            "form!: an attribute needs a value — a string literal, @{ e } for a Display \
             value, or ^{ e } for a sequence. A single colon always starts an attribute, so \
             a prefixed element name is written with two — form! { (p::local …) } — or as \
             a string literal — form! { (\"p:local\" …) }"
        )
    };
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::fmt;

    use bumpalo::Bump;

    use crate::compose::{ComposeError, Composer, Form};
    use crate::document::SerializeOptions;
    use crate::namespace::NameTable;
    use crate::types::value::XmlValue;

    /// The compact XML of a form, built by `c`.
    fn xml<'a>(c: &Composer<'a>, form: Form<'a>) -> String {
        c.build(form)
            .expect("the form builds")
            .to_xml(&SerializeOptions::default())
            .expect("the document serializes")
    }

    /// Records that an expression ran, and hands its value straight back.
    fn note<T>(log: &RefCell<Vec<&'static str>>, tag: &'static str, value: T) -> T {
        log.borrow_mut().push(tag);
        value
    }

    /// A value that is neither a string nor a number: `@{}` only needs
    /// `Display`.
    struct Bicycle;

    impl fmt::Display for Bicycle {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("Red Bicycle")
        }
    }

    // ── xpath! ────────────────────────────────────────────────────────

    #[test]
    fn all_four_call_forms_work_with_and_without_a_trailing_comma() -> Result<(), ComposeError> {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);
        let items = c.load_str("<items><item n='1'/><item n='2'/></items>")?;

        assert_eq!(xpath!(c, "1 + 1")?.string()?, "2");
        assert_eq!(xpath!(c, "count(//item)", items)?.string()?, "2");
        assert_eq!(xpath!(c, "$n * 2", n = 21)?.string()?, "42");
        assert_eq!(
            xpath!(c, "count(//item[@n = $n])", items, n = "2")?.string()?,
            "1"
        );

        assert_eq!(xpath!(c, "1 + 1",)?.string()?, "2");
        assert_eq!(xpath!(c, "count(//item)", items,)?.string()?, "2");
        assert_eq!(xpath!(c, "$n * 2", n = 21,)?.string()?, "42");
        assert_eq!(
            xpath!(c, "count(//item[@n = $n])", items, n = "2",)?.string()?,
            "1"
        );
        Ok(())
    }

    #[test]
    fn a_context_item_can_be_written_in_any_shape() -> Result<(), ComposeError> {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);
        let doc = c.load_str("<a><b>deep</b></a>")?;
        let node = doc.document_element().expect("a document element");
        let prepared = Cell::new(0);

        // Borrowed …
        assert_eq!(xpath!(c, "name()", &node)?.string()?, "a");
        // … a method call …
        assert_eq!(xpath!(c, "name(*)", doc.root())?.string()?, "a");
        // … a block that does some work first …
        assert_eq!(
            xpath!(c, "string(a/b)", {
                prepared.set(prepared.get() + 1);
                doc.root()
            })?
            .string()?,
            "deep"
        );
        assert_eq!(prepared.get(), 1);
        // … an expression that can itself fail …
        assert_eq!(
            xpath!(c, "name(*)", c.load_str("<z/>")?.root())?.string()?,
            "z"
        );
        // … and a bare identifier, which the context item takes by value.
        assert_eq!(xpath!(c, "name()", node)?.string()?, "a");
        Ok(())
    }

    #[test]
    fn a_named_binding_is_not_a_context_item() -> Result<(), ComposeError> {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);
        let doc = c.load_str("<a><b/></a>")?;
        let node = doc.document_element().expect("a document element");

        // The bindings arm is matched first, so this binds `$node` …
        assert_eq!(xpath!(c, "name($node)", node = &node)?.string()?, "a");
        // … and leaves the context item absent, which `name()` needs.
        assert!(matches!(
            xpath!(c, "name()", node = &node),
            Err(ComposeError::XPath { .. })
        ));
        Ok(())
    }

    #[test]
    fn every_expression_is_evaluated_once_and_in_order() -> Result<(), ComposeError> {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);
        let doc = c.load_str("<a><b/></a>")?;
        let log = RefCell::new(Vec::new());

        let value = xpath!(
            note(&log, "composer", &c),
            "concat(name(), $one, $two)",
            note(&log, "context", doc.document_element().unwrap()),
            one = note(&log, "one", "1"),
            two = note(&log, "two", "2"),
        )?;

        assert_eq!(value.string()?, "a12");
        assert_eq!(*log.borrow(), ["composer", "context", "one", "two"]);
        Ok(())
    }

    #[test]
    fn one_call_site_compiles_one_expression() -> Result<(), ComposeError> {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);

        for _ in 0..2 {
            assert_eq!(xpath!(c, "$n + 1", n = 1)?.string()?, "2");
        }
        assert_eq!(c.cached_expressions(), 1);
        Ok(())
    }

    #[test]
    fn an_expression_error_names_the_expression() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);

        match xpath!(c, "$undeclared + 1") {
            Err(ComposeError::XPath { expr, .. }) => assert_eq!(expr, "$undeclared + 1"),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    // ── form!: the invocation itself ──────────────────────────────────

    #[test]
    fn every_invocation_delimiter_expands_the_same() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);

        // `form! { ( … ) }` is the documented spelling because rustfmt leaves
        // it alone; the other two must keep working all the same.
        let braces = xml(&c, form! { (part :qty "7" "bolt" (tag "new")) });
        let parens = xml(&c, form!((part :qty "7" "bolt" (tag "new"))));
        let brackets = xml(&c, form![(part :qty "7" "bolt" (tag "new"))]);

        assert_eq!(braces, r#"<part qty="7">bolt<tag>new</tag></part>"#);
        assert_eq!(parens, braces);
        assert_eq!(brackets, braces);
    }

    // ── form!: names ──────────────────────────────────────────────────

    #[test]
    fn a_name_is_an_identifier_a_prefixed_pair_or_a_literal() -> Result<(), ComposeError> {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names).with_namespace("p", "urn:p");

        assert_eq!(xml(&c, form! { (item_tuple) }), "<item_tuple/>");
        assert_eq!(
            xml(&c, form! { (p::bid "55") }),
            r#"<p:bid xmlns:p="urn:p">55</p:bid>"#
        );
        // A name that is not a Rust identifier, and a prefixed one.
        assert_eq!(
            xml(&c, form! { ("bid-count" :"low-bid" "3" "7") }),
            r#"<bid-count low-bid="3">7</bid-count>"#
        );
        assert_eq!(
            xml(&c, form! { ("p:bid" "55") }),
            r#"<p:bid xmlns:p="urn:p">55</p:bid>"#
        );
        Ok(())
    }

    #[test]
    fn a_single_colon_after_the_name_starts_an_attribute() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names).with_namespace("p", "urn:p");

        // `(p:title "One")` and `(book :id "b1")` are the same token shape —
        // whitespace is not in the token stream — so one colon after the
        // element name is the attribute marker in both, and in either spacing.
        assert_eq!(xml(&c, form! { (p:title "One") }), r#"<p title="One"/>"#);
        assert_eq!(xml(&c, form! { (p :title "One") }), r#"<p title="One"/>"#);
        // A prefixed element name written with one colon does not compile at
        // all; the doc tests pin that. Nothing here is reinterpreted.
    }

    #[test]
    fn a_prefixed_element_name_is_written_with_two_colons_or_a_literal() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names).with_namespace("p", "urn:p");

        assert_eq!(
            xml(&c, form! { (p::title "One") }),
            r#"<p:title xmlns:p="urn:p">One</p:title>"#
        );
        assert_eq!(
            xml(&c, form! { ("p:title" "One") }),
            r#"<p:title xmlns:p="urn:p">One</p:title>"#
        );
        // An attribute after either spelling is unambiguous.
        assert_eq!(
            xml(&c, form! { (p::title :id "b1" "One") }),
            r#"<p:title xmlns:p="urn:p" id="b1">One</p:title>"#
        );
        assert_eq!(
            xml(&c, form! { (p::title (leaf)) }),
            r#"<p:title xmlns:p="urn:p"><leaf/></p:title>"#
        );
    }

    #[test]
    fn an_unbound_prefix_is_refused_when_the_document_is_built() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);

        match c.build(form! { (p::x) }) {
            Err(ComposeError::UnboundPrefix { prefix, at }) => {
                assert_eq!(prefix, "p");
                assert_eq!(at, "p:x");
            }
            other => panic!("expected an unbound prefix, got {other:?}"),
        }
    }

    #[test]
    fn a_literal_name_that_is_no_name_is_refused_when_the_document_is_built() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);

        match c.build(form! { ("not a name") }) {
            Err(ComposeError::InvalidName(name)) => assert_eq!(name, "not a name"),
            other => panic!("expected an invalid name, got {other:?}"),
        }
        match c.build(form! { (e :"not a name" "v") }) {
            Err(ComposeError::InvalidName(name)) => assert_eq!(name, "not a name"),
            other => panic!("expected an invalid name, got {other:?}"),
        }
    }

    // ── form!: attributes and declarations ────────────────────────────

    #[test]
    fn attributes_come_from_literals_display_values_and_sequences() -> Result<(), ComposeError> {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);
        let ids = xpath!(c, "(1, 2, 3)")?;

        assert_eq!(
            xml(
                &c,
                form! { (item :kind "bike" :count @{ 7 } :what @{ Bicycle } :ids ^{ ids }) }
            ),
            r#"<item kind="bike" count="7" what="Red Bicycle" ids="1 2 3"/>"#
        );
        Ok(())
    }

    #[test]
    fn the_xml_prefix_needs_no_declaration_in_either_spelling() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);

        assert_eq!(
            xml(&c, form! { (p :xml::lang "en" "text") }),
            r#"<p xml:lang="en">text</p>"#
        );
        assert_eq!(
            xml(&c, form! { (p :xml:space "preserve" "text") }),
            r#"<p xml:space="preserve">text</p>"#
        );
    }

    #[test]
    fn namespace_declarations_apply_at_two_levels_and_shadow() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);

        assert_eq!(
            xml(
                &c,
                form! {
                    (outer :xmlns "urn:one"
                        (inner :xmlns "urn:two" (leaf))
                        (sibling))
                }
            ),
            concat!(
                r#"<outer xmlns="urn:one">"#,
                r#"<inner xmlns="urn:two"><leaf/></inner>"#,
                "<sibling/></outer>",
            )
        );
        assert_eq!(
            xml(
                &c,
                form! { (e::root :xmlns::e "urn:e" (e::child :xmlns:e "urn:f" (e::leaf))) }
            ),
            concat!(
                r#"<e:root xmlns:e="urn:e">"#,
                r#"<e:child xmlns:e="urn:f"><e:leaf/></e:child>"#,
                "</e:root>",
            )
        );
    }

    #[test]
    fn a_conflicting_attribute_prefix_gets_a_generated_one() -> Result<(), ComposeError> {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names)
            .with_namespace("p", "urn:a")
            .with_namespace("q", "urn:b");

        // The source binds `p` to a different namespace, so the copied
        // attribute cannot keep that prefix in a `p:out` element.
        let source = c.load_str(r#"<s xmlns:p="urn:b" p:k="v"/>"#)?;
        let attr = xpath!(c, "//@q:k", source)?;

        let written = xml(&c, form! { (p::out ^{ attr }) });
        assert!(written.contains(r#"ns0:k="v""#), "{written}");
        assert!(written.contains(r#"xmlns:ns0="urn:b""#), "{written}");
        assert!(written.contains(r#"xmlns:p="urn:a""#), "{written}");
        Ok(())
    }

    #[test]
    fn a_spliced_attribute_after_content_is_refused_with_the_form_path() -> Result<(), ComposeError>
    {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);
        let source = c.load_str(r#"<s k="v"/>"#)?;
        let attr = xpath!(c, "//@k", source)?;

        match c.build(form! { (result (item "text first" ^{ attr })) }) {
            Err(ComposeError::Copy { source, at }) => {
                assert_eq!(at, "result/item");
                assert!(source.to_string().contains("content"), "{source}");
            }
            other => panic!("expected a copy refusal, got {other:?}"),
        }
        Ok(())
    }

    // ── form!: content ────────────────────────────────────────────────

    #[test]
    fn text_comes_from_literals_and_from_display_values() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);

        // Two adjacent texts concatenate with nothing between them.
        assert_eq!(
            xml(&c, form! { (p "in stock: " @{ 4 } " of " @{ Bicycle }) }),
            "<p>in stock: 4 of Red Bicycle</p>"
        );
    }

    #[test]
    fn one_content_value_copies_a_node_or_takes_a_form() -> Result<(), ComposeError> {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);
        let source = c.load_str("<a><b k='v'>text</b></a>")?;
        let b = xpath!(c, "//b", source)?;

        assert_eq!(
            xml(&c, form! { (out ^{ b } ^{ form! { (child "x") } }) }),
            r#"<out><b k="v">text</b><child>x</child></out>"#
        );
        Ok(())
    }

    #[test]
    fn an_optional_content_value_is_present_or_absent() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);
        let present: Option<Form<'_>> = Some(form! { (here) });
        let absent: Option<Form<'_>> = None;

        assert_eq!(
            xml(&c, form! { (root ^{ present } ^{ absent }) }),
            "<root><here/></root>"
        );
    }

    #[test]
    fn a_splice_over_an_empty_iterator_adds_nothing() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);
        let empty: Vec<&str> = Vec::new();

        assert_eq!(xml(&c, form! { (e "a" ..^{ empty } "b") }), "<e>ab</e>");
    }

    #[test]
    fn separate_splices_keep_the_atomic_adjacency() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);
        let empty: Vec<XmlValue> = Vec::new();

        // One space between successive atomic values, across the splice
        // boundary, and an empty splice does not break it.
        assert_eq!(
            xml(
                &c,
                form! {
                    (e
                        ..^{ [XmlValue::string("a")] }
                        ..^{ empty }
                        ..^{ [XmlValue::string("b")] })
                }
            ),
            "<e>a b</e>"
        );
    }

    #[test]
    fn a_splice_pushes_every_item_in_order() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);
        let extra = ["washer", "screw"];

        assert_eq!(
            xml(
                &c,
                form! { (parts ..^{ extra.iter().map(|n| form! { (part ^{ *n }) }) }) }
            ),
            "<parts><part>washer</part><part>screw</part></parts>"
        );
    }

    #[test]
    fn comments_and_processing_instructions_are_items() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);

        assert_eq!(
            xml(&c, form! { (root #comment " note " #pi "work" "now") }),
            "<root><!-- note --><?work now?></root>"
        );
    }

    #[test]
    fn a_nested_form_is_a_child_element() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);

        assert_eq!(
            xml(&c, form! { (a (b (c "deep")) (b)) }),
            "<a><b><c>deep</c></b><b/></a>"
        );
    }

    #[test]
    fn items_are_evaluated_once_and_in_order() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);
        let log = RefCell::new(Vec::new());

        let form = form! {
            (e
                :first @{ note(&log, "attribute", 1) }
                @{ note(&log, "text", 2) }
                ^{ note(&log, "value", "three") }
                ..^{ note(&log, "splice", vec!["four"]) }
            )
        };

        assert_eq!(*log.borrow(), ["attribute", "text", "value", "splice"]);
        assert_eq!(xml(&c, form), r#"<e first="1">2threefour</e>"#);
    }

    // ── form!: the fallible splice ────────────────────────────────────

    #[test]
    fn a_fallible_splice_carries_the_rows_through() -> Result<(), ComposeError> {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);
        let rows: Vec<Result<&str, ComposeError>> = vec![Ok("a"), Ok("b")];

        assert_eq!(xml(&c, form! { (e ..?^{ rows }) }), "<e>ab</e>");
        Ok(())
    }

    #[test]
    fn a_fallible_splice_stops_at_the_first_failure() {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);

        let pulled = Cell::new(0);
        let later = Cell::new(0);
        let built = Cell::new(0);

        let rows: Vec<Result<&str, ComposeError>> = vec![
            Ok("a"),
            Ok("b"),
            Err(ComposeError::NotANode),
            Ok("never reached"),
        ];

        let outcome = (|| -> Result<String, ComposeError> {
            let form = form! {
                (result
                    ..?^{ rows.into_iter().inspect(|_| pulled.set(pulled.get() + 1)) }
                    ^{ { later.set(later.get() + 1); "after" } }
                )
            };
            built.set(built.get() + 1);
            c.build(form)?.to_xml(&SerializeOptions::default())
        })();

        assert!(
            matches!(outcome, Err(ComposeError::NotANode)),
            "the failure leaves the closure, got {outcome:?}"
        );
        assert_eq!(pulled.get(), 3, "the row after the failure is never pulled");
        assert_eq!(later.get(), 0, "the escape after the splice is skipped");
        assert_eq!(built.get(), 0, "the build is never reached");
    }

    // ── The documented example ────────────────────────────────────────

    #[test]
    fn the_catalog_example_is_the_same_document_in_both_modes() -> Result<(), ComposeError> {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);

        let doc = c.build(form! {
            (catalog
                (book :id "b1" (title "One"))
                (book :id "b2" (title "Two"))
            )
        })?;

        assert_eq!(
            doc.to_xml(&SerializeOptions::default())?,
            concat!(
                r#"<catalog><book id="b1"><title>One</title></book>"#,
                r#"<book id="b2"><title>Two</title></book></catalog>"#,
            )
        );

        let formatted = SerializeOptions {
            indent: Some(2),
            ..SerializeOptions::default()
        };
        assert_eq!(
            doc.to_xml(&formatted)?,
            concat!(
                "<catalog>\n",
                "  <book id=\"b1\">\n    <title>One</title>\n  </book>\n",
                "  <book id=\"b2\">\n    <title>Two</title>\n  </book>\n",
                "</catalog>",
            )
        );
        Ok(())
    }
}
