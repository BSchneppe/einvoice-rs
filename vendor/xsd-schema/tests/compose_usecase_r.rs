//! The relational use case of the XQuery 1.0 test suite, rewritten in Rust.
//!
//! Eighteen queries, one function each, written with nothing but the public
//! composition surface: `xpath!` for evaluation, `pipe` for the FLWOR
//! clauses, `form!` for the result elements. Every function is compared with
//! the suite's own expected result, which is what makes this an oracle rather
//! than a set of assertions about our own behaviour.
//!
//! The suite marks these cases `compare="XML"`, so the comparison is a
//! canonical rendering of both trees rather than their text: each side is
//! parsed with `roxmltree` — an independent parser — and rendered with
//! expanded names, sorted attributes and merged text. Whitespace-only text at
//! document level is ignored, because the expected files end with a newline
//! and a serialized document does not.
//!
//! The suite is located through `XQTS_DIR`, or the checkout next to this
//! crate, and every test prints a message and passes when it is not there —
//! the same convention as the conformance drivers.
//!
//! Each query's own documentation carries the XQuery it rewrites and the
//! catalog's input bindings, since which of `items`, `bids` and `users` an
//! `$input-context` names changes from query to query.

use std::fs;
use std::path::{Path, PathBuf};

use bumpalo::Bump;
use xsd_schema::compose::order::{Direction, EmptyOrder};
use xsd_schema::compose::{pipe, ComposeError, Composer, Doc};
use xsd_schema::document::SerializeOptions;
use xsd_schema::namespace::NameTable;
use xsd_schema::{form, xpath};

// ── The suite ─────────────────────────────────────────────────────────

/// The XQuery 1.0 test suite's root, or `None` when it is not around.
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

/// The three documents the use case joins.
///
/// The queries name them as the catalog does, so a query's own documentation
/// can say `input-context2 = bids` and the code can say `inputs.bids`.
struct Inputs<'a> {
    /// `items.xml`: one `item_tuple` per auction lot.
    items: Doc<'a>,
    /// `bids.xml`: one `bid_tuple` per bid.
    bids: Doc<'a>,
    /// `users.xml`: one `user_tuple` per bidder or seller.
    users: Doc<'a>,
}

impl<'a> Inputs<'a> {
    fn load(c: &Composer<'a>, root: &Path) -> Result<Self, ComposeError> {
        Ok(Inputs {
            items: c.load_file(root.join("TestSources/items.xml"))?,
            bids: c.load_file(root.join("TestSources/bids.xml"))?,
            users: c.load_file(root.join("TestSources/users.xml"))?,
        })
    }
}

/// One rewritten query: the composer and the three inputs in, a document out.
type Query = for<'a> fn(&Composer<'a>, &Inputs<'a>) -> Result<Doc<'a>, ComposeError>;

/// Every query of the use case, in order.
const QUERIES: [(u32, Query); 18] = [
    (1, q1),
    (2, q2),
    (3, q3),
    (4, q4),
    (5, q5),
    (6, q6),
    (7, q7),
    (8, q8),
    (9, q9),
    (10, q10),
    (11, q11),
    (12, q12),
    (13, q13),
    (14, q14),
    (15, q15),
    (16, q16),
    (17, q17),
    (18, q18),
];

/// Runs one query over a composer of its own and writes it out compactly.
///
/// The arena lives for the length of the call, which is the composition
/// model: one composer per batch, and the result is text by the time the
/// arena goes away.
fn run(root: &Path, query: Query) -> Result<String, ComposeError> {
    let arena = Bump::new();
    let names = NameTable::new();
    // Four of these queries construct an `xs:date`, and `xs` is one of the
    // prefixes a composer starts with bound, so nothing is declared here.
    let c = Composer::new(&arena, &names);
    let inputs = Inputs::load(&c, root)?;
    query(&c, &inputs)?.to_xml(&SerializeOptions::default())
}

/// Runs one query and compares it with the suite's expected result.
fn check(n: u32, query: Query) {
    let Some(root) = suite_root() else { return };

    let written = match run(&root, query) {
        Ok(xml) => xml,
        Err(error) => panic!("q{n} did not compose: {error}"),
    };
    let path = root.join(format!(
        "ExpectedTestResults/UseCase/UseCaseR/rdb-queries-results-q{n}.txt"
    ));
    let expected = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("q{n}: {} is unreadable: {error}", path.display()));

    assert_eq!(
        canonical_of(&written),
        canonical_of(&expected),
        "q{n} does not match its oracle.\nwritten:  {written}\nexpected: {expected}",
    );
}

// ── Comparison: canonical form through an independent parser ──────────
//
// The renderer below is the one `serialize_roundtrip` uses, kept in step with
// it: expanded names, attributes sorted by `(uri, local)`, text runs merged,
// comments and PIs rendered, namespace declarations left out because a
// redundant declaration is legitimately dropped.

/// Parses XML text and renders its canonical form.
fn canonical_of(xml: &str) -> String {
    let doc = roxmltree::Document::parse(xml)
        .unwrap_or_else(|error| panic!("this is not XML: {error}\n{xml}"));
    let mut out = String::new();
    render_children(doc.root(), 0, &mut out);
    out
}

fn render_children(node: roxmltree::Node<'_, '_>, depth: usize, out: &mut String) {
    let mut pending = String::new();
    for child in node.children() {
        if child.is_text() {
            // Adjacent text runs are one text node in the data model.
            pending.push_str(child.text().unwrap_or(""));
            continue;
        }
        flush_text(&mut pending, depth, node.is_root(), out);
        render_node(child, depth, out);
    }
    flush_text(&mut pending, depth, node.is_root(), out);
}

fn flush_text(pending: &mut String, depth: usize, at_document_level: bool, out: &mut String) {
    if pending.is_empty() {
        return;
    }
    // The expected files end with a newline; a serialized document does not.
    if !(at_document_level && pending.trim().is_empty()) {
        out.push_str(&format!(
            "{:indent$}T {pending:?}\n",
            "",
            indent = depth * 2
        ));
    }
    pending.clear();
}

fn render_node(node: roxmltree::Node<'_, '_>, depth: usize, out: &mut String) {
    let pad = depth * 2;
    match node.node_type() {
        roxmltree::NodeType::Element => {
            let name = node.tag_name();
            out.push_str(&format!(
                "{:pad$}E {{{}}}{}\n",
                "",
                name.namespace().unwrap_or(""),
                name.name(),
            ));
            let mut attrs: Vec<(String, String, String)> = node
                .attributes()
                .map(|a| {
                    (
                        a.namespace().unwrap_or("").to_string(),
                        a.name().to_string(),
                        a.value().to_string(),
                    )
                })
                .collect();
            attrs.sort();
            for (uri, local, value) in attrs {
                out.push_str(&format!("{:pad$}  A {{{uri}}}{local}={value:?}\n", ""));
            }
            render_children(node, depth + 1, out);
        }
        roxmltree::NodeType::Comment => {
            out.push_str(&format!("{:pad$}C {:?}\n", "", node.text().unwrap_or("")));
        }
        roxmltree::NodeType::PI => {
            let pi = node.pi().expect("a PI node has PI data");
            out.push_str(&format!(
                "{:pad$}P {} {:?}\n",
                "",
                pi.target,
                pi.value.unwrap_or(""),
            ));
        }
        // Text is merged by the caller; the root cannot appear as a child.
        roxmltree::NodeType::Text | roxmltree::NodeType::Root => {}
    }
}

// ── The queries ───────────────────────────────────────────────────────

/// Q1 — bicycles on auction on 31 January 1999, in item-number order.
///
/// `input-context = items`.
///
/// ```xquery
/// <result>{
///   for $i in $input-context//item_tuple
///   where $i/start_date <= xs:date("1999-01-31")
///     and $i/end_date >= xs:date("1999-01-31")
///     and contains($i/description, "Bicycle")
///   order by $i/itemno
///   return <item_tuple>{ $i/itemno }{ $i/description }</item_tuple>
/// }</result>
/// ```
///
/// The whole `where` clause is one expression evaluated with the item as its
/// context item. The dates in these documents are untyped, and a general
/// comparison casts an untyped operand to the type of the other one, so the
/// comparison is a date comparison and nothing about dates reaches Rust.
fn q1<'a>(c: &Composer<'a>, inputs: &Inputs<'a>) -> Result<Doc<'a>, ComposeError> {
    let rows = pipe::nodes(xpath!(c, "//item_tuple", inputs.items)?)
        .try_filter(|i| {
            xpath!(
                c,
                "start_date <= xs:date('1999-01-31') \
                 and end_date >= xs:date('1999-01-31') \
                 and contains(description, 'Bicycle')",
                i
            )?
            .boolean()
        })
        .order_by(Direction::Ascending, EmptyOrder::Least, |i| {
            xpath!(c, "itemno", i)?.key()
        })?
        .try_map(|i| {
            Ok(form! {
                (item_tuple
                    ^{ xpath!(c, "itemno", &i)? }
                    ^{ xpath!(c, "description", &i)? })
            })
        });

    c.build(form! { (result ..?^{ rows }) })
}

/// Q2 — the highest bid for every bicycle, in item-number order.
///
/// `input-context1 = items`, `input-context2 = bids`.
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
/// One pipeline stage per clause, and the `let` is a projection that carries
/// the bid sequence beside its item. The broken bicycle has no bids at all,
/// so `max` returns the empty sequence and `high_bid` comes out empty — the
/// data model's answer, with nothing in Rust testing for it.
fn q2<'a>(c: &Composer<'a>, inputs: &Inputs<'a>) -> Result<Doc<'a>, ComposeError> {
    let rows = pipe::nodes(xpath!(c, "//item_tuple", inputs.items)?)
        .try_map(|i| {
            let b = xpath!(c, "//bid_tuple[itemno = $i/itemno]", inputs.bids, i = &i)?;
            Ok((i, b))
        })
        .try_filter(|(i, _)| xpath!(c, "contains(description, 'Bicycle')", i)?.boolean())
        .order_by(Direction::Ascending, EmptyOrder::Least, |(i, _)| {
            xpath!(c, "itemno", i)?.key()
        })?
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

/// Q3 — users rated worse than "C" offering items over 1000.
///
/// `input-context1 = items`, `input-context2 = users`.
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
/// The nested `for` is a `try_flat_map`: the item source is created for each
/// user and exhausted before the next one, which is the query's own order.
/// The `where` stays one expression with two bound nodes.
fn q3<'a>(c: &Composer<'a>, inputs: &Inputs<'a>) -> Result<Doc<'a>, ComposeError> {
    let rows = pipe::nodes(xpath!(c, "//user_tuple", inputs.users)?)
        .try_flat_map(|u| {
            Ok(pipe::nodes(xpath!(c, "//item_tuple", inputs.items)?)
                .try_map(move |i| Ok((u.clone(), i))))
        })
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
        .try_map(|(u, i)| {
            Ok(form! {
                (warning
                    ^{ xpath!(c, "name", &u)? }
                    ^{ xpath!(c, "rating", &u)? }
                    ^{ xpath!(c, "description", &i)? }
                    ^{ xpath!(c, "reserve_price", &i)? })
            })
        });

    c.build(form! { (result ..?^{ rows }) })
}

/// Q4 — items nobody bid on.
///
/// `input-context1 = items`, `input-context2 = bids`.
///
/// ```xquery
/// <result>{
///   for $i in $input-context1//item_tuple
///   where empty($input-context2//bid_tuple[itemno = $i/itemno])
///   return <no_bid_item>{ $i/itemno }{ $i/description }</no_bid_item>
/// }</result>
/// ```
///
/// No `order by`, so the rows keep the source's document order.
fn q4<'a>(c: &Composer<'a>, inputs: &Inputs<'a>) -> Result<Doc<'a>, ComposeError> {
    let rows = pipe::nodes(xpath!(c, "//item_tuple", inputs.items)?)
        .try_filter(|i| {
            xpath!(
                c,
                "empty(//bid_tuple[itemno = $i/itemno])",
                inputs.bids,
                i = i
            )?
            .boolean()
        })
        .try_map(|i| {
            Ok(form! {
                (no_bid_item
                    ^{ xpath!(c, "itemno", &i)? }
                    ^{ xpath!(c, "description", &i)? })
            })
        });

    c.build(form! { (result ..?^{ rows }) })
}

/// Q5 — the bicycle Tom Jones sold, its winning bid, and who made it.
///
/// `input-context1 = items`, `input-context2 = users`, `input-context3 =
/// bids`.
///
/// ```xquery
/// <result>{ unordered(
///   for $seller in $input-context2//user_tuple,
///       $buyer in $input-context2//user_tuple,
///       $item in $input-context1//item_tuple,
///       $highbid in $input-context3//bid_tuple
///   where $seller/name = "Tom Jones"
///     and $seller/userid = $item/offered_by
///     and contains($item/description, "Bicycle")
///     and $item/itemno = $highbid/itemno
///     and $highbid/userid = $buyer/userid
///     and $highbid/bid = max($input-context3//bid_tuple
///                              [itemno = $item/itemno]/bid)
///   return <jones_bike>{ $item/itemno }{ $item/description }
///          <high_bid>{ $highbid/bid }</high_bid>
///          <high_bidder>{ $buyer/name }</high_bidder></jones_bike>) }
/// </result>
/// ```
///
/// A four-way join: three nested sources over three documents, and one
/// `where` holding the rest of the conjunction with all four nodes bound.
/// The one conjunct that mentions a single source — the seller's name — is
/// applied to that source instead, which is an author's choice and not
/// something the pipeline does on its own. `unordered` asks for no particular
/// order, and one row satisfies the join.
fn q5<'a>(c: &Composer<'a>, inputs: &Inputs<'a>) -> Result<Doc<'a>, ComposeError> {
    let rows = pipe::nodes(xpath!(c, "//user_tuple", inputs.users)?)
        .try_filter(|seller| xpath!(c, "name = 'Tom Jones'", seller)?.boolean())
        .try_flat_map(|seller| {
            Ok(pipe::nodes(xpath!(c, "//user_tuple", inputs.users)?)
                .try_map(move |buyer| Ok((seller.clone(), buyer))))
        })
        .try_flat_map(|(seller, buyer)| {
            Ok(pipe::nodes(xpath!(c, "//item_tuple", inputs.items)?)
                .try_map(move |item| Ok((seller.clone(), buyer.clone(), item))))
        })
        .try_flat_map(|(seller, buyer, item)| {
            Ok(pipe::nodes(xpath!(c, "//bid_tuple", inputs.bids)?)
                .try_map(move |highbid| Ok((seller.clone(), buyer.clone(), item.clone(), highbid))))
        })
        .try_filter(|(seller, buyer, item, highbid)| {
            xpath!(
                c,
                "$seller/userid = $item/offered_by \
                 and contains($item/description, 'Bicycle') \
                 and $item/itemno = $highbid/itemno \
                 and $highbid/userid = $buyer/userid \
                 and $highbid/bid = max(//bid_tuple[itemno = $item/itemno]/bid)",
                inputs.bids,
                seller = seller,
                buyer = buyer,
                item = item,
                highbid = highbid
            )?
            .boolean()
        })
        .try_map(|(_seller, buyer, item, highbid)| {
            Ok(form! {
                (jones_bike
                    ^{ xpath!(c, "itemno", &item)? }
                    ^{ xpath!(c, "description", &item)? }
                    (high_bid ^{ xpath!(c, "bid", &highbid)? })
                    (high_bidder ^{ xpath!(c, "name", &buyer)? }))
            })
        });

    c.build(form! { (result ..?^{ rows }) })
}

/// Q6 — items whose highest bid beats twice the reserve price.
///
/// `input-context1 = items`, `input-context2 = bids`.
///
/// ```xquery
/// <result>{
///   for $item in $input-context1//item_tuple
///   let $b := $input-context2//bid_tuple[itemno = $item/itemno]
///   let $z := max($b/bid)
///   where $item/reserve_price * 2 < $z
///   return <successful_item>{ $item/itemno }{ $item/description }
///          { $item/reserve_price }<high_bid>{ $z }</high_bid></successful_item>
/// }</result>
/// ```
///
/// Two `let`s in one projection, and the second one's value is bound back
/// into the `where` as `$z` and spliced into the result as content. An item
/// with no bids gives an empty `$z`, the comparison is then the empty
/// sequence, and its effective boolean value rejects the row.
fn q6<'a>(c: &Composer<'a>, inputs: &Inputs<'a>) -> Result<Doc<'a>, ComposeError> {
    let rows = pipe::nodes(xpath!(c, "//item_tuple", inputs.items)?)
        .try_map(|item| {
            let b = xpath!(
                c,
                "//bid_tuple[itemno = $item/itemno]",
                inputs.bids,
                item = &item
            )?;
            let z = xpath!(c, "max($b/bid)", b = &b)?;
            Ok((item, z))
        })
        .try_filter(|(item, z)| {
            xpath!(c, "$item/reserve_price * 2 < $z", item = item, z = z)?.boolean()
        })
        .try_map(|(item, z)| {
            Ok(form! {
                (successful_item
                    ^{ xpath!(c, "itemno", &item)? }
                    ^{ xpath!(c, "description", &item)? }
                    ^{ xpath!(c, "reserve_price", &item)? }
                    (high_bid ^{ z }))
            })
        });

    c.build(form! { (result ..?^{ rows }) })
}

/// Q7 — the highest bid on anything with two or three wheels.
///
/// `input-context1 = items`, `input-context2 = bids`.
///
/// ```xquery
/// let $allbikes := $input-context1//item_tuple
///                    [contains(description, "Bicycle")
///                     or contains(description, "Tricycle")]
/// let $bikebids := $input-context2//bid_tuple[itemno = $allbikes/itemno]
/// return <high_bid>{ max($bikebids/bid) }</high_bid>
/// ```
///
/// No `for`, so no pipeline: two Rust locals holding node sequences, each
/// bound into the next expression as a variable, and one result element.
fn q7<'a>(c: &Composer<'a>, inputs: &Inputs<'a>) -> Result<Doc<'a>, ComposeError> {
    let allbikes = xpath!(
        c,
        "//item_tuple[contains(description, 'Bicycle') \
         or contains(description, 'Tricycle')]",
        inputs.items
    )?;
    let bikebids = xpath!(
        c,
        "//bid_tuple[itemno = $allbikes/itemno]",
        inputs.bids,
        allbikes = &allbikes
    )?;

    c.build(form! { (high_bid ^{ xpath!(c, "max($bikebids/bid)", bikebids = &bikebids)? }) })
}

/// Q8 — how many auctions ended in March 1999.
///
/// `input-context = items`.
///
/// ```xquery
/// let $item := $input-context//item_tuple
///   [end_date >= xs:date("1999-03-01") and end_date <= xs:date("1999-03-31")]
/// return <item_count>{ count($item) }</item_count>
/// ```
fn q8<'a>(c: &Composer<'a>, inputs: &Inputs<'a>) -> Result<Doc<'a>, ComposeError> {
    let item = xpath!(
        c,
        "//item_tuple[end_date >= xs:date('1999-03-01') \
         and end_date <= xs:date('1999-03-31')]",
        inputs.items
    )?;

    c.build(form! { (item_count ^{ xpath!(c, "count($item)", item = &item)? }) })
}

/// Q9 — how many auctions ended in each month.
///
/// `input-context = items`.
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
/// The rows are atomic values, not nodes: `pipe::from` adapts the atomized
/// months, the key closure hands back the month itself, and the sort is
/// numeric because the month is an `xs:integer` and not its string.
fn q9<'a>(c: &Composer<'a>, inputs: &Inputs<'a>) -> Result<Doc<'a>, ComposeError> {
    let end_dates = xpath!(c, "//item_tuple/end_date", inputs.items)?;
    let months = xpath!(
        c,
        "distinct-values(for $e in $end_dates return month-from-date($e))",
        end_dates = &end_dates
    )?
    .atomics()?;

    let rows = pipe::from(months)
        .try_map(|m| {
            let item = xpath!(
                c,
                "//item_tuple[year-from-date(end_date) = 1999 \
                 and month-from-date(end_date) = $m]",
                inputs.items,
                m = &m
            )?;
            Ok((m, item))
        })
        .order_by(Direction::Ascending, EmptyOrder::Least, |(m, _)| {
            Ok(Some(m.clone()))
        })?
        .try_map(|(m, item)| {
            Ok(form! {
                (monthly_result
                    (month ^{ m })
                    (item_count ^{ xpath!(c, "count($item)", item = &item)? }))
            })
        });

    c.build(form! { (result ..?^{ rows }) })
}

/// Q10 — the winning bid on every item, with the bidder's name.
///
/// `input-context1 = bids`, `input-context2 = users`.
///
/// ```xquery
/// <result>{
///   for $highbid in $input-context1//bid_tuple,
///       $user in $input-context2//user_tuple
///   where $user/userid = $highbid/userid
///     and $highbid/bid = max($input-context1//bid_tuple
///                              [itemno = $highbid/itemno]/bid)
///   order by $highbid/itemno
///   return <high_bid>{ $highbid/itemno }{ $highbid/bid }
///          <bidder>{ $user/name/text() }</bidder></high_bid>
/// }</result>
/// ```
///
/// `$user/name/text()` splices a *text* node, so the bidder's name arrives as
/// characters rather than as a copied `name` element.
fn q10<'a>(c: &Composer<'a>, inputs: &Inputs<'a>) -> Result<Doc<'a>, ComposeError> {
    let rows = pipe::nodes(xpath!(c, "//bid_tuple", inputs.bids)?)
        .try_flat_map(|highbid| {
            Ok(pipe::nodes(xpath!(c, "//user_tuple", inputs.users)?)
                .try_map(move |user| Ok((highbid.clone(), user))))
        })
        .try_filter(|(highbid, user)| {
            xpath!(
                c,
                "$user/userid = $highbid/userid \
                 and $highbid/bid = max(//bid_tuple[itemno = $highbid/itemno]/bid)",
                inputs.bids,
                highbid = highbid,
                user = user
            )?
            .boolean()
        })
        .order_by(Direction::Ascending, EmptyOrder::Least, |(highbid, _)| {
            xpath!(c, "itemno", highbid)?.key()
        })?
        .try_map(|(highbid, user)| {
            Ok(form! {
                (high_bid
                    ^{ xpath!(c, "itemno", &highbid)? }
                    ^{ xpath!(c, "bid", &highbid)? }
                    (bidder ^{ xpath!(c, "name/text()", &user)? }))
            })
        });

    c.build(form! { (result ..?^{ rows }) })
}

/// Q11 — the item that attracted the highest bid of all.
///
/// `input-context1 = items`, `input-context2 = bids`.
///
/// ```xquery
/// let $highbid := max($input-context2//bid_tuple/bid)
/// return <result>{
///   for $item in $input-context1//item_tuple,
///       $b in $input-context2//bid_tuple[itemno = $item/itemno]
///   where $b/bid = $highbid
///   return <expensive_item>{ $item/itemno }{ $item/description }
///          <high_bid>{ $highbid }</high_bid></expensive_item>
/// }</result>
/// ```
///
/// The outer `let` is an ordinary Rust local, evaluated once before the
/// pipeline: it is bound into the `where` as `$highbid` and spliced into the
/// content of every result element.
fn q11<'a>(c: &Composer<'a>, inputs: &Inputs<'a>) -> Result<Doc<'a>, ComposeError> {
    let highbid = xpath!(c, "max(//bid_tuple/bid)", inputs.bids)?;

    let rows = pipe::nodes(xpath!(c, "//item_tuple", inputs.items)?)
        .try_flat_map(|item| {
            let bids = xpath!(
                c,
                "//bid_tuple[itemno = $item/itemno]",
                inputs.bids,
                item = &item
            )?;
            Ok(pipe::nodes(bids).try_map(move |b| Ok((item.clone(), b))))
        })
        .try_filter(|(_, b)| xpath!(c, "$b/bid = $highbid", b = b, highbid = &highbid)?.boolean())
        .try_map(|(item, _)| {
            Ok(form! {
                (expensive_item
                    ^{ xpath!(c, "itemno", &item)? }
                    ^{ xpath!(c, "description", &item)? }
                    (high_bid ^{ &highbid }))
            })
        });

    c.build(form! { (result ..?^{ rows }) })
}

/// The bid count of every item that was bid on, as a sequence of elements.
///
/// ```xquery
/// declare function local:bid_summary() as element()* {
///   for $i in distinct-values($input-context2//itemno)
///   let $b := $input-context2//bid_tuple[itemno = $i]
///   return <bid_count><itemno>{ $i }</itemno>
///          <nbids>{ count($b) }</nbids></bid_count>
/// };
/// ```
///
/// A user-defined function returning `element()*` becomes a Rust function
/// returning a document of its own: `build_sequence` takes any number of
/// top-level elements, and [`Doc::children`] hands them back as nodes that
/// [`q12`] queries with XPath — which is exactly what the query does with
/// `$bid_counts/nbids`.
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

/// Q12 — the items that attracted the most bids.
///
/// `input-context1 = items`, `input-context2 = bids`.
///
/// ```xquery
/// <result>{
///   let $bid_counts := local:bid_summary(),
///       $maxbids := max($bid_counts/nbids),
///       $maxitemnos := $bid_counts[nbids = $maxbids]
///   for $item in $input-context1//item_tuple, $bc in $bid_counts
///   where $bc/nbids = $maxbids and $item/itemno = $bc/itemno
///   return <popular_item>{ $item/itemno }{ $item/description }
///          <bid_count>{ $bc/nbids/text() }</bid_count></popular_item>
/// }</result>
/// ```
///
/// The constructed summary is re-queried: its children are bound as
/// `$bid_counts`, `max` runs over them, and their text nodes are spliced into
/// the result. The query's unused `$maxitemnos` binding is left out.
fn q12<'a>(c: &Composer<'a>, inputs: &Inputs<'a>) -> Result<Doc<'a>, ComposeError> {
    let summary = bid_summary(c, inputs.bids)?;
    let bid_counts = summary.children();
    let maxbids = xpath!(
        c,
        "max($bid_counts/nbids)",
        bid_counts = bid_counts.as_slice()
    )?;

    let rows = pipe::nodes(xpath!(c, "//item_tuple", inputs.items)?)
        .try_flat_map(|item| {
            Ok(pipe::from(bid_counts.iter().cloned()).try_map(move |bc| Ok((item.clone(), bc))))
        })
        .try_filter(|(item, bc)| {
            xpath!(
                c,
                "$bc/nbids = $maxbids and $item/itemno = $bc/itemno",
                item = item,
                bc = bc,
                maxbids = &maxbids
            )?
            .boolean()
        })
        .try_map(|(item, bc)| {
            Ok(form! {
                (popular_item
                    ^{ xpath!(c, "itemno", &item)? }
                    ^{ xpath!(c, "description", &item)? }
                    (bid_count ^{ xpath!(c, "nbids/text()", &bc)? }))
            })
        });

    c.build(form! { (result ..?^{ rows }) })
}

/// Q13 — every bidder, with how much they bid and how often.
///
/// `input-context1 = bids`, `input-context2 = users`.
///
/// ```xquery
/// <result>{
///   for $uid in distinct-values($input-context1//userid),
///       $u in $input-context2//user_tuple[userid = $uid]
///   let $b := $input-context1//bid_tuple[userid = $uid]
///   order by $u/userid
///   return <bidder>{ $u/userid }{ $u/name }
///          <bidcount>{ count($b) }</bidcount>
///          <avgbid>{ avg($b/bid) }</avgbid></bidder>
/// }</result>
/// ```
///
/// The outer source is atomic and the inner one is a node lookup that uses
/// it, so the row starts as a user id, grows a user, and then grows that
/// user's bids.
fn q13<'a>(c: &Composer<'a>, inputs: &Inputs<'a>) -> Result<Doc<'a>, ComposeError> {
    let rows = pipe::from(xpath!(c, "distinct-values(//userid)", inputs.bids)?.atomics()?)
        .try_flat_map(|uid| {
            let users = xpath!(c, "//user_tuple[userid = $uid]", inputs.users, uid = &uid)?;
            Ok(pipe::nodes(users).try_map(move |u| Ok((uid.clone(), u))))
        })
        .try_map(|(uid, u)| {
            let b = xpath!(c, "//bid_tuple[userid = $uid]", inputs.bids, uid = &uid)?;
            Ok((u, b))
        })
        .order_by(Direction::Ascending, EmptyOrder::Least, |(u, _)| {
            xpath!(c, "userid", u)?.key()
        })?
        .try_map(|(u, b)| {
            Ok(form! {
                (bidder
                    ^{ xpath!(c, "userid", &u)? }
                    ^{ xpath!(c, "name", &u)? }
                    (bidcount ^{ xpath!(c, "count($b)", b = &b)? })
                    (avgbid ^{ xpath!(c, "avg($b/bid)", b = &b)? }))
            })
        });

    c.build(form! { (result ..?^{ rows }) })
}

/// Q14 — the items with at least three bids, dearest first.
///
/// `input-context = bids`.
///
/// ```xquery
/// <result>{
///   for $i in distinct-values($input-context//itemno)
///   let $b := $input-context//bid_tuple[itemno = $i]
///   let $avgbid := avg($b/bid)
///   where count($b) >= 3
///   order by $avgbid descending
///   return <popular_item><itemno>{ $i }</itemno>
///          <avgbid>{ $avgbid }</avgbid></popular_item>
/// }</result>
/// ```
///
/// The only descending sort in the use case. The key is the average itself —
/// an `xs:double` the engine computed — so the comparison is numeric.
fn q14<'a>(c: &Composer<'a>, inputs: &Inputs<'a>) -> Result<Doc<'a>, ComposeError> {
    let rows = pipe::from(xpath!(c, "distinct-values(//itemno)", inputs.bids)?.atomics()?)
        .try_map(|i| {
            let b = xpath!(c, "//bid_tuple[itemno = $i]", inputs.bids, i = &i)?;
            let avgbid = xpath!(c, "avg($b/bid)", b = &b)?;
            Ok((i, b, avgbid))
        })
        .try_filter(|(_, b, _)| xpath!(c, "count($b) >= 3", b = b)?.boolean())
        .order_by(
            Direction::Descending,
            EmptyOrder::Least,
            |(_, _, avgbid)| avgbid.key(),
        )?
        .try_map(|(i, _, avgbid)| {
            Ok(form! {
                (popular_item
                    (itemno ^{ i })
                    (avgbid ^{ avgbid }))
            })
        });

    c.build(form! { (result ..?^{ rows }) })
}

/// Q15 — users who bid 100 or more on more than one item.
///
/// `input-context1 = users`, `input-context2 = bids`.
///
/// ```xquery
/// <result>{
///   for $u in $input-context1//user_tuple
///   let $b := $input-context2//bid_tuple[userid=$u/userid and bid>=100]
///   where count($b) > 1
///   return <big_spender>{ $u/name/text() }</big_spender>
/// }</result>
/// ```
fn q15<'a>(c: &Composer<'a>, inputs: &Inputs<'a>) -> Result<Doc<'a>, ComposeError> {
    let rows = pipe::nodes(xpath!(c, "//user_tuple", inputs.users)?)
        .try_map(|u| {
            let b = xpath!(
                c,
                "//bid_tuple[userid = $u/userid and bid >= 100]",
                inputs.bids,
                u = &u
            )?;
            Ok((u, b))
        })
        .try_filter(|(_, b)| xpath!(c, "count($b) > 1", b = b)?.boolean())
        .try_map(|(u, _)| Ok(form! { (big_spender ^{ xpath!(c, "name/text()", &u)? }) }));

    c.build(form! { (result ..?^{ rows }) })
}

/// Q16 — every user, marked active or inactive.
///
/// `input-context1 = users`, `input-context2 = bids`.
///
/// ```xquery
/// <result>{
///   for $u in $input-context1//user_tuple
///   let $b := $input-context2//bid_tuple[userid = $u/userid]
///   order by $u/userid
///   return <user>{ $u/userid }{ $u/name }
///          { if (empty($b)) then <status>inactive</status>
///                           else <status>active</status> }</user>
/// }</result>
/// ```
///
/// The conditional constructor is a Rust `if` choosing between two forms. It
/// needs no XPath at all: the bid sequence is a value in hand, so `empty($b)`
/// is [`Value::is_empty`](xsd_schema::compose::Value::is_empty).
fn q16<'a>(c: &Composer<'a>, inputs: &Inputs<'a>) -> Result<Doc<'a>, ComposeError> {
    let rows = pipe::nodes(xpath!(c, "//user_tuple", inputs.users)?)
        .try_map(|u| {
            let b = xpath!(c, "//bid_tuple[userid = $u/userid]", inputs.bids, u = &u)?;
            Ok((u, b))
        })
        .order_by(Direction::Ascending, EmptyOrder::Least, |(u, _)| {
            xpath!(c, "userid", u)?.key()
        })?
        .try_map(|(u, b)| {
            let status = if b.is_empty() {
                form! { (status "inactive") }
            } else {
                form! { (status "active") }
            };
            Ok(form! {
                (user
                    ^{ xpath!(c, "userid", &u)? }
                    ^{ xpath!(c, "name", &u)? }
                    ^{ status })
            })
        });

    c.build(form! { (result ..?^{ rows }) })
}

/// Q17 — users who bid on every single item.
///
/// `input-context1 = items`, `input-context2 = users`, `input-context3 =
/// bids`.
///
/// ```xquery
/// <frequent_bidder>{
///   for $u in $input-context2//user_tuple
///   where every $item in $input-context1//item_tuple satisfies
///           some $b in $input-context3//bid_tuple satisfies
///             ($item/itemno = $b/itemno and $u/userid = $b/userid)
///   return $u/name
/// }</frequent_bidder>
/// ```
///
/// Both quantified expressions stay inside one XPath expression with all
/// three documents bound as variables; Rust only iterates the users. Nobody
/// qualifies, so the splice contributes nothing and the result is an empty
/// element.
fn q17<'a>(c: &Composer<'a>, inputs: &Inputs<'a>) -> Result<Doc<'a>, ComposeError> {
    let rows = pipe::nodes(xpath!(c, "//user_tuple", inputs.users)?)
        .try_filter(|u| {
            xpath!(
                c,
                "every $item in $items//item_tuple satisfies \
                 some $b in $bids//bid_tuple satisfies \
                 ($item/itemno = $b/itemno and $u/userid = $b/userid)",
                items = inputs.items,
                bids = inputs.bids,
                u = u
            )?
            .boolean()
        })
        .try_map(|u| xpath!(c, "name", &u)?.nodes());

    c.build(form! { (frequent_bidder ..?^{ rows }) })
}

/// Q18 — every user and what they bid on, both lists sorted.
///
/// `input-context1 = items`, `input-context2 = users`, `input-context3 =
/// bids`.
///
/// ```xquery
/// <result>{
///   for $u in $input-context2//user_tuple
///   order by $u/name
///   return <user>{ $u/name }{
///     for $b in distinct-values($input-context3//bid_tuple
///                                 [userid = $u/userid]/itemno)
///     for $i in $input-context1//item_tuple[itemno = $b]
///     let $descr := $i/description/text()
///     order by $descr
///     return <bid_on_item>{ $descr }</bid_on_item> }</user>
/// }</result>
/// ```
///
/// A sorted pipeline inside another sorted pipeline's element content: the
/// inner one is built, sorted and spliced inside the outer projection, so its
/// `order_by` runs once per user and its failures are that user's failures.
/// A user who bid on nothing gets an empty `user` element rather than a
/// special case.
fn q18<'a>(c: &Composer<'a>, inputs: &Inputs<'a>) -> Result<Doc<'a>, ComposeError> {
    let rows = pipe::nodes(xpath!(c, "//user_tuple", inputs.users)?)
        .order_by(Direction::Ascending, EmptyOrder::Least, |u| {
            xpath!(c, "name", u)?.key()
        })?
        .try_map(|u| {
            let bid_on = pipe::from(
                xpath!(
                    c,
                    "distinct-values(//bid_tuple[userid = $u/userid]/itemno)",
                    inputs.bids,
                    u = &u
                )?
                .atomics()?,
            )
            .try_flat_map(|b| {
                Ok(pipe::nodes(xpath!(
                    c,
                    "//item_tuple[itemno = $b]",
                    inputs.items,
                    b = &b
                )?))
            })
            .try_map(|i| xpath!(c, "description/text()", &i))
            .order_by(Direction::Ascending, EmptyOrder::Least, |descr| descr.key())?
            .try_map(|descr| Ok(form! { (bid_on_item ^{ descr }) }));

            Ok(form! {
                (user
                    ^{ xpath!(c, "name", &u)? }
                    ..?^{ bid_on })
            })
        });

    c.build(form! { (result ..?^{ rows }) })
}

// ── The tests ─────────────────────────────────────────────────────────

#[test]
fn q1_bicycles_on_auction_on_a_date() {
    check(1, q1);
}

#[test]
fn q2_the_highest_bid_per_bicycle() {
    check(2, q2);
}

#[test]
fn q3_badly_rated_users_offering_expensive_items() {
    check(3, q3);
}

#[test]
fn q4_items_nobody_bid_on() {
    check(4, q4);
}

#[test]
fn q5_the_bicycle_tom_jones_sold() {
    check(5, q5);
}

#[test]
fn q6_items_that_beat_twice_their_reserve() {
    check(6, q6);
}

#[test]
fn q7_the_highest_bid_on_a_cycle() {
    check(7, q7);
}

#[test]
fn q8_auctions_ending_in_march() {
    check(8, q8);
}

#[test]
fn q9_auctions_per_month() {
    check(9, q9);
}

#[test]
fn q10_the_winning_bid_per_item() {
    check(10, q10);
}

#[test]
fn q11_the_item_with_the_highest_bid() {
    check(11, q11);
}

#[test]
fn q12_the_items_with_the_most_bids() {
    check(12, q12);
}

#[test]
fn q13_every_bidder_summarized() {
    check(13, q13);
}

#[test]
fn q14_items_with_three_bids_dearest_first() {
    check(14, q14);
}

#[test]
fn q15_users_who_bid_a_hundred_twice() {
    check(15, q15);
}

#[test]
fn q16_every_user_active_or_not() {
    check(16, q16);
}

#[test]
fn q17_users_who_bid_on_everything() {
    check(17, q17);
}

#[test]
fn q18_every_user_and_what_they_bid_on() {
    check(18, q18);
}

/// All eighteen at once, with a line per query.
///
/// The individual tests above say which query failed; this one says how the
/// use case as a whole stands, and is the test to read the output of.
#[test]
fn the_whole_use_case() {
    let Some(root) = suite_root() else { return };

    let mut failed = Vec::new();
    for (n, query) in QUERIES {
        let outcome = match run(&root, query) {
            Ok(written) => {
                let path = root.join(format!(
                    "ExpectedTestResults/UseCase/UseCaseR/rdb-queries-results-q{n}.txt"
                ));
                let expected = fs::read_to_string(&path).unwrap_or_else(|error| {
                    panic!("q{n}: {} is unreadable: {error}", path.display())
                });
                if canonical_of(&written) == canonical_of(&expected) {
                    Ok(())
                } else {
                    Err(format!("wrong result: {written}"))
                }
            }
            Err(error) => Err(format!("did not compose: {error}")),
        };

        match outcome {
            Ok(()) => println!("q{n:<2} pass"),
            Err(why) => {
                println!("q{n:<2} FAIL  {why}");
                failed.push(n);
            }
        }
    }

    println!(
        "{} of {} queries pass",
        QUERIES.len() - failed.len(),
        QUERIES.len()
    );
    assert!(failed.is_empty(), "queries that did not match: {failed:?}");
}
