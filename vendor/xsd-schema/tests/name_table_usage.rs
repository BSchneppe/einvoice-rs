//! The name table reports its size in bytes, in constant time, so a host can
//! bound how much a long-lived schema set grows from the names of the
//! documents it validates (C47; fix round, decision 10).

use xsd_schema::namespace::NameTable;

#[test]
fn heap_bytes_grows_with_the_interned_text() {
    let table = NameTable::new();
    let before = table.heap_bytes();
    assert!(before > 0);
    let name = "n".repeat(1 << 20);
    table.add(&name);
    let after = table.heap_bytes();
    assert!(after >= before + (1 << 20), "{before} -> {after}");
    // Interning the same text again adds nothing.
    table.add(&name);
    assert_eq!(table.heap_bytes(), after);
    // Many small names add their text and their entries.
    for i in 0..10_000 {
        table.add(&format!("name{i}"));
    }
    assert!(table.heap_bytes() > after + 10_000 * 5);
}
