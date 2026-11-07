//! The composition layer, exercised only through its public surface.
//!
//! This test lives outside the crate on purpose. The macros expand to
//! `$crate::compose::…` paths, so anything they reach must be public, and a
//! private item would fail this compilation at once. The import list below is
//! therefore part of what is being tested.
//!
//! The first case is the relational use case of the XQuery 1.0 test suite,
//! written in the authoring style the design specifies, and compared with the
//! suite's own expected result. The second needs no corpus: it nests a
//! pipeline inside another pipeline's callback and checks that a failure in
//! the inner one reaches the caller instead of turning into missing content.

use std::path::{Path, PathBuf};

use bumpalo::Bump;
use xsd_schema::compose::order::{Direction, EmptyOrder};
use xsd_schema::compose::{pipe, ComposeError, Composer, Doc};
use xsd_schema::document::serialize::SerializeOptions;
use xsd_schema::namespace::table::NameTable;
use xsd_schema::{form, xpath};

/// The XQuery 1.0 test suite's root, or `None` when it is not around.
///
/// The same convention as the conformance drivers: an environment variable
/// first, then the checkout next to this crate, and a printed message rather
/// than a failure when neither is there.
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
/// The query this rewrites:
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
/// One pipeline stage per FLWOR clause: a source, a projection that carries
/// the bid sequence beside its item, a filter, the sort, and a projection that
/// builds the result element.
fn highest_bid_per_bicycle(root: &Path) -> Result<String, ComposeError> {
    let arena = Bump::new();
    let names = NameTable::new();
    let c = Composer::new(&arena, &names);
    let items = c.load_file(root.join("TestSources/items.xml"))?;
    let bids = c.load_file(root.join("TestSources/bids.xml"))?;

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
    doc.to_xml(&SerializeOptions::default())
}

#[test]
fn the_relational_use_case_matches_its_oracle() {
    let Some(root) = suite_root() else { return };

    let written = highest_bid_per_bicycle(&root).expect("the composition succeeds");

    assert_eq!(written,
      "<result><item_tuple><itemno>1001</itemno><description>Red Bicycle</description><high_bid>55</high_bid></item_tuple>\
       <item_tuple><itemno>1003</itemno><description>Old Bicycle</description><high_bid>20</high_bid></item_tuple>\
       <item_tuple><itemno>1007</itemno><description>Racing Bicycle</description><high_bid>225</high_bid></item_tuple>\
       <item_tuple><itemno>1008</itemno><description>Broken Bicycle</description><high_bid/></item_tuple></result>");
}

/// Every group with its members, each group's size counted in Rust.
///
/// The inner pipeline is created inside the outer projection's callback and
/// collected there, so its failures are the outer row's failures.
fn grouped<'a>(c: &Composer<'a>, doc: Doc<'a>) -> Result<Doc<'a>, ComposeError> {
    let groups = pipe::nodes(xpath!(c, "//group", doc)?).try_map(|g| {
        let members = pipe::nodes(xpath!(c, "member", &g)?)
            .try_map(|m| Ok(form! { (member :name ^{ xpath!(c, "@name", &m)? }) }))
            .collect::<Result<Vec<_>, ComposeError>>()?;
        Ok(form! {
            (group
                :id ^{ xpath!(c, "@id", &g)? }
                :size @{ members.len() }
                ..^{ members })
        })
    });

    c.build(form! { (groups ..?^{ groups }) })
}

/// The same shape, but insisting every group has exactly one member.
///
/// `string()` on a two-item sequence is
/// [`ComposeError::NotSingleton`](xsd_schema::compose::ComposeError::NotSingleton),
/// raised inside a callback of a callback: it has to travel out through the
/// inner projection, the fallible splice, and this function's `?`.
fn only_children<'a>(c: &Composer<'a>, doc: Doc<'a>) -> Result<Doc<'a>, ComposeError> {
    let groups = pipe::nodes(xpath!(c, "//group", doc)?).try_map(|g| {
        let only = xpath!(c, "member/@name", &g)?.string()?;
        Ok(form! { (group ^{ only }) })
    });

    c.build(form! { (groups ..?^{ groups }) })
}

#[test]
fn a_nested_composition_builds_and_propagates() {
    let arena = Bump::new();
    let names = NameTable::new();
    let c = Composer::new(&arena, &names);
    let doc = c
        .load_str(
            "<teams>\
               <group id='a'><member name='ann'/><member name='bo'/></group>\
               <group id='b'><member name='cy'/></group>\
             </teams>",
        )
        .expect("the source parses");

    assert_eq!(
        grouped(&c, doc)
            .expect("the composition succeeds")
            .to_xml(&SerializeOptions::default())
            .expect("the document serializes"),
        concat!(
            r#"<groups><group id="a" size="2">"#,
            r#"<member name="ann"/><member name="bo"/></group>"#,
            r#"<group id="b" size="1"><member name="cy"/></group></groups>"#,
        )
    );

    // The two-member group reaches `string()`, which refuses it.
    assert!(matches!(
        only_children(&c, doc),
        Err(ComposeError::NotSingleton { len: 2 })
    ));
}
