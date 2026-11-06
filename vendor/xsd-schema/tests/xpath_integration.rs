#![cfg(feature = "xsd11")]
//! XPath integration tests using sample XML files.

use std::fs;
use std::path::PathBuf;
use xsd_schema::namespace::table::NameTable;
use xsd_schema::xpath::api::XPathExpr;
use xsd_schema::xpath::{DomNavigator, RoXmlNavigator, XPathContext};

fn get_examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples")
}

/// Helper to extract string value from an XmlItem (node or atomic)
fn item_to_string<N: DomNavigator>(item: &xsd_schema::xpath::XmlItem<N>) -> String {
    match item {
        xsd_schema::xpath::XmlItem::Node(nav) => nav.value(),
        xsd_schema::xpath::XmlItem::Atomic(val) => val.to_string_value(),
    }
}

/// Test: //book[price > 35]/title on books.xml
/// Expected: "The First Book" (price 44.95)
#[test]
fn test_book_price_filter() {
    let xml_path = get_examples_dir().join("books.xml");
    let xml_content = fs::read_to_string(&xml_path).expect("Failed to read books.xml");

    let names = NameTable::new();
    let ctx = XPathContext::new(&names);

    let expr = XPathExpr::compile("//book[price > 35]/title", &ctx)
        .expect("Failed to compile XPath expression");

    let doc = roxmltree::Document::parse(&xml_content).expect("Failed to parse XML");
    let nav = RoXmlNavigator::new(&doc);

    let result = expr
        .evaluator(&ctx)
        .run_with_node(nav)
        .expect("Failed to evaluate XPath");

    // Should find one book with price > 35 (The First Book with price 44.95)
    assert_eq!(result.len(), 1, "Expected exactly one book with price > 35");

    // Get the string value using run_string
    let title = expr
        .evaluator(&ctx)
        .run_with_node(RoXmlNavigator::new(&doc))
        .expect("Failed to evaluate XPath");

    let items = title.into_vec();
    let title_str = item_to_string(&items[0]);

    assert_eq!(
        title_str, "The First Book",
        "Expected 'The First Book' as the title"
    );
}

/// Test: for $x in //item return concat($x/@partNum, '-', $x/productName) on purchaseOrder_utf8.xml
/// Expected: ["872-AA-Lawnmower", "926-AA-Baby Monitor"]
#[test]
fn test_flwor_item_concat() {
    let xml_path = get_examples_dir().join("purchaseOrder.xml");
    let xml_content = fs::read_to_string(&xml_path).expect("Failed to read purchaseOrder.xml");

    let names = NameTable::new();
    let ctx = XPathContext::new(&names);

    let expr = XPathExpr::compile(
        "for $x in //item return concat($x/@partNum, '-', $x/productName)",
        &ctx,
    )
    .expect("Failed to compile XPath expression");

    let doc = roxmltree::Document::parse(&xml_content).expect("Failed to parse XML");
    let nav = RoXmlNavigator::new(&doc);

    let result = expr
        .evaluator(&ctx)
        .run_with_node(nav)
        .expect("Failed to evaluate XPath");

    // Should return a sequence of two concatenated strings
    assert_eq!(result.len(), 2, "Expected two items in the result");

    let items: Vec<String> = result.into_vec().iter().map(item_to_string).collect();

    assert_eq!(
        items[0], "872-AA-Lawnmower",
        "First item should be 872-AA-Lawnmower"
    );
    assert_eq!(
        items[1], "926-AA-Baby Monitor",
        "Second item should be 926-AA-Baby Monitor"
    );
}

/// Evaluate `expr` over `books.xml` and return the string value of each item.
fn eval_books(expr: &str) -> Vec<String> {
    let xml_path = get_examples_dir().join("books.xml");
    let xml_content = fs::read_to_string(&xml_path).expect("Failed to read books.xml");

    let names = NameTable::new();
    let ctx = XPathContext::new(&names);

    let compiled = XPathExpr::compile(expr, &ctx).expect("Failed to compile XPath expression");

    let doc = roxmltree::Document::parse(&xml_content).expect("Failed to parse XML");
    let nav = RoXmlNavigator::new(&doc);

    let result = compiled
        .evaluator(&ctx)
        .run_with_node(nav)
        .expect("Failed to evaluate XPath");

    result.into_vec().iter().map(item_to_string).collect()
}

/// Test: `//title[1]` on books.xml.
///
/// A predicate belongs to the step it is written on, and the step is evaluated
/// once per node of the sequence reaching it (XPath 2.0 §3.2 / §3.2.2). `//x[1]`
/// therefore expands to `…/descendant-or-self::node()/child::x[1]` and selects
/// the first `title` child **of every node that has one** — it is *not* the same
/// expression as `(//title)[1]`, where the parentheses make the predicate apply
/// to the whole node sequence.
#[test]
fn test_positional_predicate() {
    assert_eq!(
        eval_books("//title[1]"),
        [
            "The First Book",
            "Becoming Somebody",
            "The Poet's First Poem"
        ],
        "`//title[1]` selects the first title child of each book"
    );
    assert_eq!(
        eval_books("(//title)[1]"),
        ["The First Book"],
        "`(//title)[1]` selects the first title in document order"
    );
}

/// Test: `//title[last()]` on books.xml — the counterpart of
/// [`test_positional_predicate`] for `fn:last()`.
#[test]
fn test_last_predicate() {
    assert_eq!(
        eval_books("//title[last()]"),
        [
            "The First Book",
            "Becoming Somebody",
            "The Poet's First Poem"
        ],
        "`//title[last()]` selects the last title child of each book"
    );
    assert_eq!(
        eval_books("(//title)[last()]"),
        ["The Poet's First Poem"],
        "`(//title)[last()]` selects the last title in document order"
    );
}

// ============================================================================
// Function conversion rules for xs:untypedAtomic arguments (XPath 2.0 §3.1.5)
// ============================================================================

/// An unschema'd document: every element's typed value is `xs:untypedAtomic`,
/// so a built-in whose parameter is a specific atomic type has to cast it.
const UNTYPED_DOC: &str = concat!(
    "<record>",
    "<end_date>1999-01-20</end_date>",
    "<stamp>1999-01-20T12:34:56-05:00</stamp>",
    "<clock>12:34:56</clock>",
    "<span>P1Y2M3DT4H5M6S</span>",
    "<offset>-PT5H</offset>",
    "<codepoint>65</codepoint>",
    "<position>2</position>",
    "<precision>1</precision>",
    "<amount>-3.5</amount>",
    "<ratio>2.4</ratio>",
    "<letter>b</letter>",
    "<text>not-a-date</text>",
    "</record>"
);

/// Evaluate `expr_src` over [`UNTYPED_DOC`] and render the result as strings.
fn eval_untyped(expr_src: &str) -> Result<Vec<String>, String> {
    let names = NameTable::new();
    let ctx = XPathContext::new(&names);
    let expr = XPathExpr::compile(expr_src, &ctx).expect("Failed to compile XPath expression");
    let doc = roxmltree::Document::parse(UNTYPED_DOC).expect("Failed to parse XML");
    let nav = RoXmlNavigator::new(&doc);
    match expr.evaluator(&ctx).run_with_node(nav) {
        Ok(value) => Ok(value.into_vec().iter().map(item_to_string).collect()),
        Err(err) => Err(err.to_string()),
    }
}

/// Evaluate `expr_src` over [`UNTYPED_DOC`], expecting exactly one item.
fn eval_untyped_one(expr_src: &str) -> String {
    let items = eval_untyped(expr_src).unwrap_or_else(|err| panic!("{expr_src}: {err}"));
    assert_eq!(
        items.len(),
        1,
        "{expr_src} should yield one item: {items:?}"
    );
    items.into_iter().next().unwrap()
}

/// The date component functions cast an untyped argument to `xs:date`.
#[test]
fn test_untyped_date_components_are_cast() {
    assert_eq!(eval_untyped_one("year-from-date(/record/end_date)"), "1999");
    assert_eq!(eval_untyped_one("month-from-date(/record/end_date)"), "1");
    assert_eq!(eval_untyped_one("day-from-date(/record/end_date)"), "20");
    // 1999-01-20 carries no timezone, so fn:timezone-from-date is empty.
    assert_eq!(
        eval_untyped("timezone-from-date(/record/end_date)").unwrap(),
        Vec::<String>::new()
    );
}

/// The dateTime component functions cast an untyped argument to `xs:dateTime`.
#[test]
fn test_untyped_datetime_components_are_cast() {
    assert_eq!(
        eval_untyped_one("year-from-dateTime(/record/stamp)"),
        "1999"
    );
    assert_eq!(eval_untyped_one("month-from-dateTime(/record/stamp)"), "1");
    assert_eq!(eval_untyped_one("day-from-dateTime(/record/stamp)"), "20");
    assert_eq!(eval_untyped_one("hours-from-dateTime(/record/stamp)"), "12");
    assert_eq!(
        eval_untyped_one("minutes-from-dateTime(/record/stamp)"),
        "34"
    );
    assert_eq!(
        eval_untyped_one("seconds-from-dateTime(/record/stamp)"),
        "56"
    );
    assert_eq!(
        eval_untyped_one("timezone-from-dateTime(/record/stamp)"),
        "-PT5H"
    );
}

/// The time component functions cast an untyped argument to `xs:time`.
#[test]
fn test_untyped_time_components_are_cast() {
    assert_eq!(eval_untyped_one("hours-from-time(/record/clock)"), "12");
    assert_eq!(eval_untyped_one("minutes-from-time(/record/clock)"), "34");
    assert_eq!(eval_untyped_one("seconds-from-time(/record/clock)"), "56");
}

/// The duration component functions cast an untyped argument to `xs:duration`.
#[test]
fn test_untyped_duration_components_are_cast() {
    assert_eq!(eval_untyped_one("years-from-duration(/record/span)"), "1");
    assert_eq!(eval_untyped_one("months-from-duration(/record/span)"), "2");
    assert_eq!(eval_untyped_one("days-from-duration(/record/span)"), "3");
    assert_eq!(eval_untyped_one("hours-from-duration(/record/span)"), "4");
    assert_eq!(eval_untyped_one("minutes-from-duration(/record/span)"), "5");
    assert_eq!(eval_untyped_one("seconds-from-duration(/record/span)"), "6");
}

/// `fn:adjust-*-to-timezone` casts both its value and its timezone argument.
#[test]
fn test_untyped_timezone_adjustment_is_cast() {
    assert_eq!(
        eval_untyped_one("adjust-date-to-timezone(/record/end_date, /record/offset)"),
        "1999-01-20-05:00"
    );
    assert_eq!(
        eval_untyped_one("adjust-dateTime-to-timezone(/record/stamp, /record/offset)"),
        "1999-01-20T12:34:56-05:00"
    );
    assert_eq!(
        eval_untyped_one("adjust-time-to-timezone(/record/clock, /record/offset)"),
        "12:34:56-05:00"
    );
}

/// `fn:dateTime($date, $time)` casts both of its arguments.
#[test]
fn test_untyped_datetime_constructor_is_cast() {
    assert_eq!(
        eval_untyped_one("dateTime(/record/end_date, /record/clock)"),
        "1999-01-20T12:34:56"
    );
}

/// `fn:codepoints-to-string` atomizes and casts to `xs:integer`.
///
/// Its declared return type is `xs:string`, not `xs:string?`, so an empty
/// argument yields the empty string rather than the empty sequence.
#[test]
fn test_untyped_codepoints_to_string_is_cast() {
    assert_eq!(
        eval_untyped_one("codepoints-to-string(/record/codepoint)"),
        "A"
    );
    assert_eq!(eval_untyped_one("codepoints-to-string(/record/absent)"), "");
}

/// The `xs:integer` position arguments of `fn:remove` / `fn:insert-before` and
/// the `numeric` arguments of `fn:abs` and friends are converted too.
#[test]
fn test_untyped_integer_and_numeric_arguments_are_cast() {
    assert_eq!(
        eval_untyped("remove((1, 2, 3), /record/position)").unwrap(),
        vec!["1", "3"]
    );
    assert_eq!(
        eval_untyped("insert-before((1, 2), /record/position, 9)").unwrap(),
        vec!["1", "9", "2"]
    );
    assert_eq!(eval_untyped_one("abs(/record/amount)"), "3.5");
    assert_eq!(eval_untyped_one("ceiling(/record/amount)"), "-3");
    assert_eq!(eval_untyped_one("floor(/record/amount)"), "-4");
    assert_eq!(eval_untyped_one("round(/record/ratio)"), "2");
    assert_eq!(
        eval_untyped_one("round-half-to-even(/record/amount, /record/precision)"),
        "-3.5"
    );
}

/// An optional parameter still yields the empty sequence for an empty argument.
#[test]
fn test_empty_argument_still_yields_empty_sequence() {
    for expr_src in [
        "month-from-date(/record/absent)",
        "hours-from-dateTime(/record/absent)",
        "minutes-from-time(/record/absent)",
        "days-from-duration(/record/absent)",
        "adjust-date-to-timezone(/record/absent)",
        "dateTime(/record/absent, /record/clock)",
        "abs(/record/absent)",
    ] {
        assert_eq!(
            eval_untyped(expr_src).unwrap(),
            Vec::<String>::new(),
            "{expr_src} should be the empty sequence"
        );
    }
}

/// An untyped value whose lexical form is not valid for the expected type is a
/// dynamic error, FORG0001 — not a type error.
#[test]
fn test_uncastable_untyped_argument_is_forg0001() {
    for (expr_src, expected_type) in [
        ("month-from-date(/record/text)", "xs:date"),
        ("hours-from-dateTime(/record/text)", "xs:dateTime"),
        ("minutes-from-time(/record/text)", "xs:time"),
        ("days-from-duration(/record/text)", "xs:duration"),
        ("codepoints-to-string(/record/text)", "xs:integer"),
        ("abs(/record/text)", "xs:double"),
    ] {
        let err = eval_untyped(expr_src).expect_err("invalid lexical form");
        assert!(err.starts_with("[FORG0001]"), "{expr_src}: {err}");
        assert!(err.contains("not-a-date"), "{expr_src}: {err}");
        assert!(err.contains(expected_type), "{expr_src}: {err}");
    }
}

/// A type that is not castable from `xs:untypedAtomic` at all still raises the
/// closing type error of §3.1.5: casting to `xs:QName` requires a string
/// literal, so `fn:prefix-from-QName` keeps rejecting an untyped node.
#[test]
fn test_untyped_argument_to_qname_function_is_xpty0004() {
    let err =
        eval_untyped("prefix-from-QName(/record/text)").expect_err("xs:QName is not castable");
    assert!(err.starts_with("[XPTY0004]"), "{err}");
}

/// `fn:index-of` does *not* cast its search value to the type of the sequence.
///
/// Its items are compared with `$srchParam` under the rules of the `eq`
/// operator, and F&O §15.1.5 adds: "Values that cannot be compared, i.e. the eq
/// operator is not defined for their types, are considered to be distinct."
/// Under `eq` an `xs:untypedAtomic` operand is cast to `xs:string` (XPath 2.0
/// §3.5.1: "If the atomized operand is of type xs:untypedAtomic, it is cast to
/// xs:string"), and `xs:integer eq xs:string` is not a valid combination in
/// §B.2 Operator Mapping. So an untyped `2` is distinct from every `xs:integer`
/// in the sequence: the result is the empty sequence, and no error.
#[test]
fn test_index_of_untyped_search_value_is_compared_as_a_string() {
    assert_eq!(
        eval_untyped("index-of((1, 2, 3), /record/position)").unwrap(),
        Vec::<String>::new(),
        "an untyped search value is a string, so it matches no xs:integer"
    );
    // The same cast makes the comparison succeed against a sequence of strings.
    assert_eq!(
        eval_untyped("index-of(('a', 'b'), /record/letter)").unwrap(),
        vec!["2"]
    );
    // And a typed xs:integer search value still matches.
    assert_eq!(eval_untyped("index-of((1, 2, 3), 2)").unwrap(), vec!["2"]);
}
