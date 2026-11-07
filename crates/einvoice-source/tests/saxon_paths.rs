//! Independently captured Saxon 12.9/schxslt rows, authored XML only.
use einvoice_source::{QueryBudget, SourceDocument};
use std::fmt::Write;

fn budget() -> QueryBudget {
    QueryBudget::new(100_000, 100_000).unwrap()
}
fn row(out: &mut String, path: &str, value: &str) {
    write!(out, "{path}|").unwrap();
    for (i, c) in value.chars().enumerate() {
        if i != 0 {
            out.push(',');
        }
        write!(out, "{}", c as u32).unwrap();
    }
    out.push('\n');
}
#[test]
fn eighteen_independent_saxon_path_and_value_rows() {
    let d = SourceDocument::parse(include_bytes!("evidence/paths.xml")).unwrap();
    let mut got = String::new();
    for n in d.nodes() {
        row(
            &mut got,
            &n.canonical_path(&mut budget()).unwrap(),
            &n.string_value(&mut budget()).unwrap(),
        );
        for a in n.attributes() {
            row(
                &mut got,
                &a.canonical_path(&mut budget()).unwrap(),
                a.value(),
            );
        }
    }
    let expected = include_str!("evidence/saxon-18.txt");
    assert_eq!(expected.lines().count(), 18);
    assert_eq!(got, expected);
}
