//! XQuery-style composition from Rust.
//!
//! This module lets a host build XML from query results without an XQuery
//! processor. The XPath 2.0 engine already evaluates every expression a
//! FLWOR body needs — `max`, `count`, `avg`, `distinct-values`, quantified
//! expressions, date component extraction; Rust supplies the iteration and
//! the binding; and this module supplies the composition: a place to keep the
//! documents, a description of the result elements, and the emission rules
//! that make the result an XDM tree rather than a string of tags.
//!
//! There is no query parser here, no FLWOR runtime, and no new value model.
//!
//! # The pieces
//!
//! | Piece | What it is |
//! |---|---|
//! | [`Composer`] | the arena, the name table, the namespace context, and a cache of compiled expressions |
//! | [`Doc`] | a finished document in that arena — loaded or built — as a `Copy` handle |
//! | [`Value`] | one evaluation's result: an XDM sequence, with the conversions a host asks for |
//! | [`IntoXPathValue`] | the way Rust values, navigators and sequences are bound to `$variables` |
//! | [`Form`] / [`Content`] / [`AttrValue`] / [`Name`] | a result element, described but not yet built |
//! | [`IntoContent`] | the way Rust values become element content |
//! | [`Emitter`] | the walk that turns forms into a document |
//! | [`order`] | `order by`, with XDM comparison and stable sorting |
//! | [`pipe`] | fallible iterator pipelines: the FLWOR clauses as adapters |
//! | [`ComposeError`] | every way a composition can fail |
//! | [`xpath!`](crate::xpath!) / [`form!`](crate::form!) | the two macros: one evaluation, one result element |
//!
//! The macros are exported at the crate root, so the canonical import is
//! `use xsd_schema::{form, xpath};` — see [`macros`] for the grammar they
//! accept. In a form, a prefixed name is written with two colons —
//! `form! { (p::title "One") }` — because one colon after the element name is
//! always the start of an attribute: `form! { (book :id "b1") }`.
//!
//! A composer starts with `xs`, `xsi` and `fn` bound, so a cast —
//! `xs:date('1999-01-31')` — and a form name like `(xs::schema …)` need no
//! declaration of their own; [`Composer::new`] carries the table and the
//! reasoning.
//!
//! # Lifetimes
//!
//! A [`Composer<'a>`] borrows an arena and a name table. Every document it
//! loads or builds is finalized into that arena, so every navigator,
//! [`Value`] and [`Form`] shares the one lifetime `'a`: a node taken from one
//! document splices into another with no lifetime work at all. The price is
//! that nothing is freed until the composer and its arena go away, which
//! makes the model one composer per request or per batch.
//!
//! Every method takes `&self`, so a composer can be used inside iterator
//! closures and nested loops. It is neither `Send` nor `Sync`.
//!
//! # Values
//!
//! An evaluation hands back a [`Value`], which is the engine's own sequence
//! with nothing atomized, copied or flattened. `nodes`, `atomics`, `boolean`,
//! `string`, `number` and `key` each convert exactly as far as asked, and each
//! refuses rather than guessing: an atomic item where a node was required is
//! [`ComposeError::NotANode`], and several items where one was required is
//! [`ComposeError::NotSingleton`].
//!
//! # Forms
//!
//! A [`Form`] is an owned, eager tree holding no closures. Every Rust value
//! inside one has already been evaluated, which is why a `?` inside a form's
//! construction returns from the enclosing function and why a form can be
//! returned, stored and spliced later.
//!
//! # Emission
//!
//! [`Composer::build`] walks the form once. Prefixes resolve against the
//! forms' own declarations first and the composer's table second; the
//! declarations an element must carry are computed by the namespace fixup, so
//! every name in the result resolves; attribute sequences are atomized and
//! joined with single spaces; and spliced nodes are copied with the
//! constructor content rules — atomic values separated from each other by one
//! space and from a node by nothing, document nodes flattened, empty
//! sequences contributing nothing at all. An empty sequence spliced into an
//! element therefore leaves an empty element, which is the XDM answer and not
//! something the host has to special-case.
//!
//! # Pipelines, and what is not lazy
//!
//! [`pipe`] turns the FLWOR clauses into fallible iterator adapters: a source
//! supplies rows, `try_map` carries a `let` binding along in a tuple,
//! `try_filter` is `where`, `try_flat_map` is a nested `for`, and `order_by`
//! is `order by`. Errors travel to the consumer instead of turning into
//! missing rows, and a pipe is fused after the first one.
//!
//! The adapters are pulled on demand, but three things are eager and stay
//! eager: an expression that feeds a source has already been evaluated and
//! its sequence is already materialized; `order_by` buffers its input, which
//! is what makes it a stable sort with one key evaluation per row; and a
//! completed form tree is a tree. The pipelines remove the intermediate
//! vectors between stages and make the query's structure visible. They do not
//! promise XML streaming, incremental evaluation, or a form-free result.
//!
//! # Example
//!
//! One document in, one document out, with a filter, a sort and a projection:
//!
//! ```
//! use bumpalo::Bump;
//! use xsd_schema::compose::order::{Direction, EmptyOrder};
//! use xsd_schema::compose::{pipe, ComposeError, Composer, Content, Form, IntoContent, Name};
//! use xsd_schema::document::SerializeOptions;
//! use xsd_schema::namespace::NameTable;
//!
//! let arena = Bump::new();
//! let names = NameTable::new();
//! let c = Composer::new(&arena, &names);
//!
//! let stock = c.load_str(
//!     "<stock>\
//!        <part name='bolt' qty='7'/>\
//!        <part name='nut' qty='0'/>\
//!        <part name='washer' qty='3'/>\
//!      </stock>",
//! )?;
//!
//! // for $p in //part where $p/@qty > 0 order by $p/@name return <part>…</part>
//! let rows = pipe::nodes(c.eval("//part", &[], Some(stock.root()), Vec::new())?)
//!     .try_filter(|p| c.eval("@qty > 0", &[], Some(p.clone()), Vec::new())?.boolean())
//!     .order_by(Direction::Ascending, EmptyOrder::Least, |p| {
//!         c.eval("@name", &[], Some(p.clone()), Vec::new())?.key()
//!     })?
//!     .try_map(|p| {
//!         let mut part = Form::new(Name::local("part"));
//!         part.push(c.eval("string(@name)", &[], Some(p), Vec::new())?.into_content());
//!         Ok(part)
//!     });
//!
//! let mut result = Form::new(Name::local("in_stock"));
//! for row in rows {
//!     result.push(Content::Element(row?));
//! }
//!
//! assert_eq!(
//!     c.build(result)?.to_xml(&SerializeOptions::default())?,
//!     "<in_stock><part>bolt</part><part>washer</part></in_stock>",
//! );
//! # Ok::<(), ComposeError>(())
//! ```
//!
//! # Walkthroughs
//!
//! Three queries from the relational use case of the XQuery 1.0 test suite,
//! over its own `items.xml`, `bids.xml` and `users.xml`. Every one of the
//! eighteen queries in that use case is rewritten this way in
//! `tests/compose_usecase_r.rs` and compared with the suite's expected
//! result; these three are the ones worth reading. They are `no_run` here
//! because they need the suite checked out — the test is what runs them.
//!
//! The shared setup, and the canonical import list:
//!
//! ```no_run
//! use bumpalo::Bump;
//! use xsd_schema::compose::order::{Direction, EmptyOrder};
//! use xsd_schema::compose::{pipe, ComposeError, Composer};
//! use xsd_schema::document::SerializeOptions;
//! use xsd_schema::namespace::NameTable;
//! use xsd_schema::{form, xpath};
//!
//! let arena = Bump::new();
//! let names = NameTable::new();
//! let c = Composer::new(&arena, &names);
//! let items = c.load_file("XQTS_1_0_2/TestSources/items.xml")?;
//! let bids = c.load_file("XQTS_1_0_2/TestSources/bids.xml")?;
//! let users = c.load_file("XQTS_1_0_2/TestSources/users.xml")?;
//! # Ok::<(), ComposeError>(())
//! ```
//!
//! ## RQ2 — the highest bid for every bicycle
//!
//! ```xquery
//! <result>{
//!   for $i in $input-context1//item_tuple
//!   let $b := $input-context2//bid_tuple[itemno = $i/itemno]
//!   where contains($i/description, "Bicycle")
//!   order by $i/itemno
//!   return <item_tuple>{ $i/itemno }{ $i/description }
//!          <high_bid>{ max($b/bid) }</high_bid></item_tuple>
//! }</result>
//! ```
//!
//! In C# with LINQ the same query reads as an expression with a host value
//! bound to `$i` and a constructor taking the result — the shape this module
//! follows:
//!
//! ```csharp
//! // C# with LINQ, abridged
//! var bid = bids.XPath2Select<XElement>(
//!     "//bid_tuple[itemno = $i/itemno]", new { i = item });
//! new XElement("item_tuple", item.Element("itemno"), item.Element("description"),
//!     !bid.Any() ? null
//!                : new XElement("high_bid", bid.AsQueryable().Max(/* … */)));
//! ```
//!
//! `new { i = item }` is `i = &i` below. The `!bid.Any() ? null : …` is the
//! part that does not carry over, and deliberately: the query says
//! `<high_bid>{ max($b/bid) }</high_bid>` with no conditional in it, and the
//! broken bicycle's empty `high_bid` element is what the content rules
//! produce from an empty sequence. There is no Rust test for "no bids"
//! anywhere in the version below.
//!
//! ```no_run
//! # use bumpalo::Bump;
//! # use xsd_schema::compose::order::{Direction, EmptyOrder};
//! # use xsd_schema::compose::{pipe, ComposeError, Composer};
//! # use xsd_schema::document::SerializeOptions;
//! # use xsd_schema::namespace::NameTable;
//! # use xsd_schema::{form, xpath};
//! # let arena = Bump::new();
//! # let names = NameTable::new();
//! # let c = Composer::new(&arena, &names);
//! # let items = c.load_file("XQTS_1_0_2/TestSources/items.xml")?;
//! # let bids = c.load_file("XQTS_1_0_2/TestSources/bids.xml")?;
//! // for $i in ...
//! let rows = pipe::nodes(xpath!(c, "//item_tuple", items)?)
//!     // let $b := ...
//!     .try_map(|i| {
//!         let b = xpath!(c, "//bid_tuple[itemno = $i/itemno]", bids, i = &i)?;
//!         Ok((i, b))
//!     })
//!     // where ...
//!     .try_filter(|(i, _)| xpath!(c, "contains(description, 'Bicycle')", i)?.boolean())
//!     // order by ...
//!     .order_by(Direction::Ascending, EmptyOrder::Least, |(i, _)| {
//!         xpath!(c, "itemno", i)?.key()
//!     })?
//!     // return ...
//!     .try_map(|(i, b)| {
//!         Ok(form! {
//!             (item_tuple
//!                 ^{ xpath!(c, "itemno", &i)? }
//!                 ^{ xpath!(c, "description", &i)? }
//!                 (high_bid ^{ xpath!(c, "max($b/bid)", b = &b)? }))
//!         })
//!     });
//!
//! let doc = c.build(form! { (result ..?^{ rows }) })?;
//! assert_eq!(
//!     doc.to_xml(&SerializeOptions::default())?,
//!     concat!(
//!         "<result>",
//!         "<item_tuple><itemno>1001</itemno><description>Red Bicycle</description>",
//!         "<high_bid>55</high_bid></item_tuple>",
//!         "<item_tuple><itemno>1003</itemno><description>Old Bicycle</description>",
//!         "<high_bid>20</high_bid></item_tuple>",
//!         "<item_tuple><itemno>1007</itemno><description>Racing Bicycle</description>",
//!         "<high_bid>225</high_bid></item_tuple>",
//!         "<item_tuple><itemno>1008</itemno><description>Broken Bicycle</description>",
//!         "<high_bid/></item_tuple>",
//!         "</result>",
//!     ),
//! );
//! # Ok::<(), ComposeError>(())
//! ```
//!
//! Every closure is one FLWOR clause. The `let` is a projection that carries
//! the bid sequence beside its item as `(i, b)`; the filter and the key
//! selector borrow that row; the last projection consumes it. Relative
//! expressions take the node as their context item rather than rebinding it
//! as a variable, and no result element is built before the sort — `order_by`
//! owns the buffer, and computes each key once.
//!
//! ## RQ3 — users rated worse than "C" offering items over 1000
//!
//! ```xquery
//! <result>{
//!   for $u in $input-context2//user_tuple
//!   for $i in $input-context1//item_tuple
//!   where $u/rating > "C" and $i/reserve_price > 1000
//!     and $i/offered_by = $u/userid
//!   return <warning>{ $u/name }{ $u/rating }
//!          { $i/description }{ $i/reserve_price }</warning>
//! }</result>
//! ```
//!
//! ```no_run
//! # use bumpalo::Bump;
//! # use xsd_schema::compose::{pipe, ComposeError, Composer};
//! # use xsd_schema::document::SerializeOptions;
//! # use xsd_schema::namespace::NameTable;
//! # use xsd_schema::{form, xpath};
//! # let arena = Bump::new();
//! # let names = NameTable::new();
//! # let c = Composer::new(&arena, &names);
//! # let items = c.load_file("XQTS_1_0_2/TestSources/items.xml")?;
//! # let users = c.load_file("XQTS_1_0_2/TestSources/users.xml")?;
//! // for $u in ..., for $i in ...
//! let warnings = pipe::nodes(xpath!(c, "//user_tuple", users)?)
//!     .try_flat_map(|u| {
//!         Ok(pipe::nodes(xpath!(c, "//item_tuple", items)?)
//!             .try_map(move |i| Ok((u.clone(), i))))
//!     })
//!     // where ...
//!     .try_filter(|(u, i)| {
//!         xpath!(
//!             c,
//!             "$u/rating > 'C' and $i/reserve_price > 1000 \
//!              and $i/offered_by = $u/userid",
//!             u = u,
//!             i = i
//!         )?
//!         .boolean()
//!     })
//!     // return ...
//!     .try_map(|(u, i)| {
//!         Ok(form! {
//!             (warning
//!                 ^{ xpath!(c, "name", &u)? }
//!                 ^{ xpath!(c, "rating", &u)? }
//!                 ^{ xpath!(c, "description", &i)? }
//!                 ^{ xpath!(c, "reserve_price", &i)? })
//!         })
//!     });
//!
//! let doc = c.build(form! { (result ..?^{ warnings }) })?;
//! assert_eq!(
//!     doc.to_xml(&SerializeOptions::default())?,
//!     concat!(
//!         "<result><warning><name>Dee Linquent</name><rating>D</rating>",
//!         "<description>Helicopter</description>",
//!         "<reserve_price>50000</reserve_price></warning></result>",
//!     ),
//! );
//! # Ok::<(), ComposeError>(())
//! ```
//!
//! The nested `for` is `try_flat_map`: the item source is created for each
//! user and exhausted before the next user, which is the query's own order.
//! The inner `move` closure owns `u` and clones the handle into each pair —
//! a navigator handle, not a copied subtree; the copying happens later, when
//! the form is built. The three-part `where` stays one expression with two
//! nodes bound to it, which is the shape the C# version has too.
//!
//! ## RQ9 — how many auctions ended in each month
//!
//! ```xquery
//! <result>{
//!   let $end_dates := $input-context//item_tuple/end_date
//!   for $m in distinct-values(for $e in $end_dates
//!                             return month-from-date($e))
//!   let $item := $input-context//item_tuple
//!       [year-from-date(end_date) = 1999 and month-from-date(end_date) = $m]
//!   order by $m
//!   return <monthly_result><month>{ $m }</month>
//!          <item_count>{ count($item) }</item_count></monthly_result>
//! }</result>
//! ```
//!
//! ```no_run
//! # use bumpalo::Bump;
//! # use xsd_schema::compose::order::{Direction, EmptyOrder};
//! # use xsd_schema::compose::{pipe, ComposeError, Composer};
//! # use xsd_schema::document::SerializeOptions;
//! # use xsd_schema::namespace::NameTable;
//! # use xsd_schema::{form, xpath};
//! # let arena = Bump::new();
//! # let names = NameTable::new();
//! let c = Composer::new(&arena, &names);
//! # let items = c.load_file("XQTS_1_0_2/TestSources/items.xml")?;
//!
//! // let $end_dates := ...
//! let end_dates = xpath!(c, "//item_tuple/end_date", items)?;
//! // for $m in distinct-values(...)
//! let months = xpath!(
//!     c,
//!     "distinct-values(for $e in $end_dates return month-from-date($e))",
//!     end_dates = &end_dates
//! )?
//! .atomics()?;
//!
//! let rows = pipe::from(months)
//!     // let $item := ...
//!     .try_map(|m| {
//!         let item = xpath!(
//!             c,
//!             "//item_tuple[year-from-date(end_date) = 1999 \
//!              and month-from-date(end_date) = $m]",
//!             items,
//!             m = &m
//!         )?;
//!         Ok((m, item))
//!     })
//!     // order by $m
//!     .order_by(Direction::Ascending, EmptyOrder::Least, |(m, _)| Ok(Some(m.clone())))?
//!     // return ...
//!     .try_map(|(m, item)| {
//!         Ok(form! {
//!             (monthly_result
//!                 (month ^{ m })
//!                 (item_count ^{ xpath!(c, "count($item)", item = &item)? }))
//!         })
//!     });
//!
//! let doc = c.build(form! { (result ..?^{ rows }) })?;
//! assert_eq!(
//!     doc.to_xml(&SerializeOptions::default())?,
//!     concat!(
//!         "<result>",
//!         "<monthly_result><month>1</month><item_count>1</item_count></monthly_result>",
//!         "<monthly_result><month>2</month><item_count>2</item_count></monthly_result>",
//!         "<monthly_result><month>3</month><item_count>3</item_count></monthly_result>",
//!         "<monthly_result><month>4</month><item_count>1</item_count></monthly_result>",
//!         "<monthly_result><month>5</month><item_count>1</item_count></monthly_result>",
//!         "</result>",
//!     ),
//! );
//! # Ok::<(), ComposeError>(())
//! ```
//!
//! Here the rows are atomic values rather than nodes: `pipe::from` adapts the
//! atomized months, the key closure hands the month straight back, and the
//! sort is numeric because a month is an `xs:integer` whose string value is
//! `1` and not `1.0`. `atomics()` materializes its conversion — the pipeline
//! makes the structure declarative, it does not make XPath evaluation lazy.
//!
//! One detail this query settles: the distinct months come from *all* end
//! dates while the year restriction sits in the per-month lookup, so a month
//! with end dates but no matching 1999 items would still return a count of
//! zero. Note also that `month-from-date(end_date)` is written exactly as the
//! query has it, with no constructor: these documents carry no schema, so
//! their dates are `xs:untypedAtomic`, and the function conversion rules of
//! XPath 2.0 §3.1.5 cast such an argument to the declared `xs:date` parameter.

pub mod composer;
pub mod emit;
pub mod error;
pub mod form;
pub mod macros;
pub mod order;
pub mod pipe;
pub mod value;

pub use composer::{Composer, Doc};
pub use emit::Emitter;
pub use error::ComposeError;
pub use form::{AttrValue, Content, Form, IntoContent, Name};
pub use value::{IntoContextNode, IntoXPathValue, Value};

// What `form!` calls, and nothing a host writes by hand. The macros are
// exported at the crate root (`xsd_schema::form!`, `xsd_schema::xpath!`) and
// expand to `$crate::compose::…` paths only, so these have to be reachable
// from outside the crate even though they are not part of the surface.
#[doc(hidden)]
pub use form::{__display_text, __name_from_literal};

/// The navigator every composed value and form uses.
///
/// v1 fixes the navigator type to the document buffer's own. A composer
/// generic over [`DomNavigator`](crate::navigator::DomNavigator) is a
/// mechanical extension, and is not needed until a host wants to splice nodes
/// from another tree implementation.
pub type Nav<'a> = crate::document::BufferDocNavigator<'a>;

#[cfg(test)]
mod use_case_tests {
    use std::path::{Path, PathBuf};

    use bumpalo::Bump;

    use super::order::{Direction, EmptyOrder};
    use super::{pipe, ComposeError, Composer, Content, Form, IntoContent, IntoXPathValue, Name};
    use crate::document::SerializeOptions;
    use crate::namespace::NameTable;

    /// The XQuery 1.0 test-suite root, or `None` when it is not around.
    ///
    /// Same convention as the conformance drivers: an environment variable
    /// first, then the checkout next to this crate, and a printed message
    /// rather than a failure when neither is there.
    fn suite_root() -> Option<PathBuf> {
        let candidate = match std::env::var_os("XQTS_DIR") {
            Some(dir) => PathBuf::from(dir),
            None => PathBuf::from("../../XQTS_1_0_2"),
        };
        if candidate.join("TestSources/items.xml").is_file() {
            Some(candidate)
        } else {
            println!(
                "skipping: no XQuery test suite at {} (set XQTS_DIR)",
                candidate.display()
            );
            None
        }
    }

    /// The highest bid for every bicycle, in item-number order.
    ///
    /// The query this rewrites, from the test suite's relational use case:
    ///
    /// ```xquery
    /// <result>{
    ///   for $i in $input-context1//item_tuple
    ///   let $b := $input-context2//bid_tuple[itemno = $i/itemno]
    ///   where contains($i/description, "Bicycle")
    ///   order by $i/itemno
    ///   return <item_tuple>{ $i/itemno }{ $i/description }
    ///          <high_bid>{ max($b/bid) }</high_bid></item_tuple>
    /// }</result>
    /// ```
    ///
    /// Written with [`Composer::eval`] and the [`Form`] API directly, since
    /// the macros are a later step.
    fn highest_bid_per_bicycle(root: &Path) -> Result<String, ComposeError> {
        let arena = Bump::new();
        let names = NameTable::new();
        let c = Composer::new(&arena, &names);

        let items = c.load_file(root.join("TestSources/items.xml"))?;
        let bids = c.load_file(root.join("TestSources/bids.xml"))?;

        // for $i in //item_tuple
        let rows = pipe::nodes(c.eval("//item_tuple", &[], Some(items.root()), Vec::new())?)
            // let $b := //bid_tuple[itemno = $i/itemno]
            .try_map(|i| {
                let b = c.eval(
                    "//bid_tuple[itemno = $i/itemno]",
                    &["i"],
                    Some(bids.root()),
                    vec![("i", (&i).into_xpath_value())],
                )?;
                Ok((i, b))
            })
            // where contains($i/description, "Bicycle")
            .try_filter(|(i, _)| {
                c.eval(
                    "contains(description, 'Bicycle')",
                    &[],
                    Some(i.clone()),
                    Vec::new(),
                )?
                .boolean()
            })
            // order by $i/itemno
            .order_by(Direction::Ascending, EmptyOrder::Least, |(i, _)| {
                c.eval("itemno", &[], Some(i.clone()), Vec::new())?.key()
            })?
            // return <item_tuple>…</item_tuple>
            .try_map(|(i, b)| {
                let mut high_bid = Form::new(Name::local("high_bid"));
                high_bid.push(
                    c.eval(
                        "max($b/bid)",
                        &["b"],
                        None,
                        vec![("b", (&b).into_xpath_value())],
                    )?
                    .into_content(),
                );

                let mut item = Form::new(Name::local("item_tuple"));
                item.push(
                    c.eval("itemno", &[], Some(i.clone()), Vec::new())?
                        .into_content(),
                )
                .push(
                    c.eval("description", &[], Some(i.clone()), Vec::new())?
                        .into_content(),
                )
                .push(Content::Element(high_bid));
                Ok(item)
            });

        let mut result = Form::new(Name::local("result"));
        for row in rows {
            result.push(Content::Element(row?));
        }

        c.build(result)?.to_xml(&SerializeOptions::default())
    }

    #[test]
    fn the_relational_use_case_matches_its_oracle() {
        let Some(root) = suite_root() else { return };

        let written = highest_bid_per_bicycle(&root).expect("the composition succeeds");

        // The expected result of the use case, byte for byte.
        assert_eq!(
            written,
            "<result><item_tuple><itemno>1001</itemno><description>Red Bicycle</description><high_bid>55</high_bid></item_tuple>\
             <item_tuple><itemno>1003</itemno><description>Old Bicycle</description><high_bid>20</high_bid></item_tuple>\
             <item_tuple><itemno>1007</itemno><description>Racing Bicycle</description><high_bid>225</high_bid></item_tuple>\
             <item_tuple><itemno>1008</itemno><description>Broken Bicycle</description><high_bid/></item_tuple></result>",
        );
    }
}
