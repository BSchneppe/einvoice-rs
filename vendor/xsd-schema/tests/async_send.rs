//! Every public future of the asynchronous loaders is `Send`, so that it can
//! be spawned on a multi-threaded executor (`tokio::spawn` requires `Send`).
//!
//! A compile-time check: the file compiles only if each future is `Send`.
//! (A future that holds a `&SchemaSet` across an `.await` is not: the name
//! table's interior mutability makes `SchemaSet` `!Sync`.)
#![cfg(feature = "async")]

use xsd_schema::{SchemaResolver, SchemaSet, SchemaSetBuilder};

fn assert_send<T: Send>(_: &T) {}

#[test]
fn the_async_futures_are_send() {
    let builder = SchemaSetBuilder::new();
    let future = builder.compile_async();
    assert_send(&future);
    drop(future);

    let future = SchemaSetBuilder::new().add_async("", "/no/such.xsd");
    assert_send(&future);
    drop(future);

    let mut set = SchemaSet::new();
    let future = xsd_schema::load_and_process_schema_async(b"", "/x.xsd", &mut set, None);
    assert_send(&future);
    drop(future);

    let mut set = SchemaSet::new();
    let future = xsd_schema::load_schema_async(b"", "/x.xsd", &mut set);
    assert_send(&future);
    drop(future);

    let mut resolver = SchemaResolver::new();
    let mut set = SchemaSet::new();
    let future = resolver.load_schema_async("a.xsd", "", &mut set, None);
    assert_send(&future);
    drop(future);

    let resolver = SchemaResolver::new();
    let future = resolver.load_content_async("a.xsd");
    assert_send(&future);
    drop(future);

    let mut resolver = SchemaResolver::new();
    let mut set = SchemaSet::new();
    let future =
        xsd_schema::parser::resolver::resolve_all_directives_async(0, &mut resolver, &mut set);
    assert_send(&future);
    drop(future);
}
