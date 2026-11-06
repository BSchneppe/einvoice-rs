//! Three XQuery use cases, run without an XQuery processor.
//!
//! ```text
//! cargo run --example xquery_without_xquery --features compose -- ../../XQTS_1_0_2
//! ```
//!
//! The argument is the root of the XQuery 1.0 test suite checkout (the
//! `XQTS_DIR` environment variable works too); the three input documents come
//! from its `TestSources` directory, and the results are the ones its
//! `ExpectedTestResults` directory holds for the relational use case.
//!
//! Each query is printed twice: compact, which is the mode for output another
//! program reads, and formatted with a two-space indent, which is the mode
//! for output a person reads.
//!
//! The three ingredients, in every one of them:
//!
//! * `xpath!` evaluates an XPath 2.0 expression with Rust values bound to its
//!   `$variables` — a node, a document, a sequence, an atomic value;
//! * a `pipe` pipeline supplies the FLWOR clauses: a source, `try_map` for
//!   `let`, `try_filter` for `where`, `try_flat_map` for a nested `for`,
//!   `order_by` for `order by`;
//! * `form!` describes the result elements, and `Composer::build` turns the
//!   description into a document.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bumpalo::Bump;
use xsd_schema::compose::order::{Direction, EmptyOrder};
use xsd_schema::compose::{pipe, ComposeError, Composer, Doc};
use xsd_schema::document::SerializeOptions;
use xsd_schema::namespace::NameTable;
use xsd_schema::{form, xpath};

fn main() -> ExitCode {
    let Some(root) = suite_root() else {
        eprintln!(
            "usage: cargo run --example xquery_without_xquery --features compose \
             -- <XQTS_1_0_2 directory>\n\
             (or set XQTS_DIR; the directory must contain TestSources/items.xml)"
        );
        return ExitCode::FAILURE;
    };

    match report(&root) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("the composition failed: {error}");
            ExitCode::FAILURE
        }
    }
}

/// The suite root from the command line, the environment, or the checkout
/// next to this crate — whichever exists first.
fn suite_root() -> Option<PathBuf> {
    let mut candidates = std::env::args_os()
        .nth(1)
        .into_iter()
        .chain(std::env::var_os("XQTS_DIR"))
        .map(PathBuf::from)
        .chain(std::iter::once(PathBuf::from("../../XQTS_1_0_2")));

    candidates.find(|root| root.join("TestSources/items.xml").is_file())
}

/// Runs the three queries and prints each result twice.
fn report(root: &Path) -> Result<(), ComposeError> {
    // One composer, one arena, one name table for the whole batch: every
    // document loaded or built lives in the arena until it goes away, and
    // every node and value shares that one lifetime.
    let arena = Bump::new();
    let names = NameTable::new();
    // RQ9 constructs an `xs:date`, and `xs` is one of the prefixes a composer
    // starts with bound, so there is nothing to declare here.
    let c = Composer::new(&arena, &names);

    let items = c.load_file(root.join("TestSources/items.xml"))?;
    let bids = c.load_file(root.join("TestSources/bids.xml"))?;
    let users = c.load_file(root.join("TestSources/users.xml"))?;

    show(
        "RQ2 — the highest bid for every bicycle",
        highest_bid_per_bicycle(&c, items, bids)?,
    )?;
    show(
        "RQ3 — badly rated users offering expensive items",
        risky_sellers(&c, items, users)?,
    )?;
    show(
        "RQ9 — how many auctions ended in each month",
        auctions_per_month(&c, items)?,
    )?;

    Ok(())
}

/// Prints one result compactly and then formatted.
fn show(title: &str, doc: Doc<'_>) -> Result<(), ComposeError> {
    let formatted = SerializeOptions {
        indent: Some(2),
        ..SerializeOptions::default()
    };

    println!("── {title} ──\n");
    println!("{}\n", doc.to_xml(&SerializeOptions::default())?);
    println!("{}\n", doc.to_xml(&formatted)?);
    Ok(())
}

/// RQ2, the highest bid for every bicycle, in item-number order.
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
/// One stage per clause. The last bicycle has no bids, so `max` returns the
/// empty sequence and its `high_bid` element comes out empty — no Rust test
/// for "no bids" anywhere, because the data model already says it.
fn highest_bid_per_bicycle<'a>(
    c: &Composer<'a>,
    items: Doc<'a>,
    bids: Doc<'a>,
) -> Result<Doc<'a>, ComposeError> {
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

    c.build(form! { (result ..?^{ rows }) })
}

/// RQ3, users rated worse than "C" who offer items over 1000.
///
/// ```xquery
/// <result>{
///   for $u in $input-context2//user_tuple
///   for $i in $input-context1//item_tuple
///   where $u/rating > "C" and $i/reserve_price > 1000
///     and $i/offered_by = $u/userid
///   return <warning>{ $u/name }{ $u/rating }
///          { $i/description }{ $i/reserve_price }</warning>
/// }</result>
/// ```
///
/// The nested `for` is `try_flat_map`: the item source is created for each
/// user and exhausted before the next user, which is the query's own order.
/// The three-part `where` stays one expression with two nodes bound to it.
fn risky_sellers<'a>(
    c: &Composer<'a>,
    items: Doc<'a>,
    users: Doc<'a>,
) -> Result<Doc<'a>, ComposeError> {
    // for $u in ..., for $i in ...
    let warnings = pipe::nodes(xpath!(c, "//user_tuple", users)?)
        .try_flat_map(|u| {
            Ok(pipe::nodes(xpath!(c, "//item_tuple", items)?).try_map(move |i| Ok((u.clone(), i))))
        })
        // where ...
        .try_filter(|(u, i)| {
            xpath!(
                c,
                "$u/rating > 'C' and $i/reserve_price > 1000 \
                 and $i/offered_by = $u/userid",
                u = u,
                i = i
            )?
            .boolean()
        })
        // return ...
        .try_map(|(u, i)| {
            Ok(form! {
                (warning
                    ^{ xpath!(c, "name", &u)? }
                    ^{ xpath!(c, "rating", &u)? }
                    ^{ xpath!(c, "description", &i)? }
                    ^{ xpath!(c, "reserve_price", &i)? })
            })
        });

    c.build(form! { (result ..?^{ warnings }) })
}

/// RQ9, how many auctions ended in each month of 1999.
///
/// ```xquery
/// <result>{
///   let $end_dates := $input-context//item_tuple/end_date
///   for $m in distinct-values(for $e in $end_dates
///                             return month-from-date($e))
///   let $item := $input-context//item_tuple
///       [year-from-date(end_date) = 1999 and month-from-date(end_date) = $m]
///   order by $m
///   return <monthly_result><month>{ $m }</month>
///          <item_count>{ count($item) }</item_count></monthly_result>
/// }</result>
/// ```
///
/// The rows here are atomic values rather than nodes: `pipe::from` adapts the
/// atomized months, and the sort compares `xs:integer`s numerically. These
/// documents have no schema, so their dates are `xs:untypedAtomic`; the
/// function conversion rules of XPath 2.0 §3.1.5 cast them to the declared
/// `xs:date` parameter, so the expressions read exactly as the query does.
fn auctions_per_month<'a>(c: &Composer<'a>, items: Doc<'a>) -> Result<Doc<'a>, ComposeError> {
    // let $end_dates := ...
    let end_dates = xpath!(c, "//item_tuple/end_date", items)?;
    // for $m in distinct-values(...)
    let months = xpath!(
        c,
        "distinct-values(for $e in $end_dates return month-from-date($e))",
        end_dates = &end_dates
    )?
    .atomics()?;

    let rows = pipe::from(months)
        // let $item := ...
        .try_map(|m| {
            let item = xpath!(
                c,
                "//item_tuple[year-from-date(end_date) = 1999 \
                 and month-from-date(end_date) = $m]",
                items,
                m = &m
            )?;
            Ok((m, item))
        })
        // order by $m
        .order_by(Direction::Ascending, EmptyOrder::Least, |(m, _)| {
            Ok(Some(m.clone()))
        })?
        // return ...
        .try_map(|(m, item)| {
            Ok(form! {
                (monthly_result
                    (month ^{ m })
                    (item_count ^{ xpath!(c, "count($item)", item = &item)? }))
            })
        });

    c.build(form! { (result ..?^{ rows }) })
}
