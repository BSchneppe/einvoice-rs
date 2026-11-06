//! Print the compiled content model of a complex type.
//!
//! ```text
//! cargo run --example inspect_content_model -- <schema.xsd> <type-local-name> [namespace]
//! cargo run --example inspect_content_model -- examples/books.xsd BookForm urn:books
//! ```
//!
//! The report has three views — the type's source facts, the authored particle
//! tree, and the compiled matcher the validator executes. See
//! [`xsd_schema::compiler::inspect`] for what each one contains.
//!
//! `<type-local-name>` names a **global** complex type. An anonymous local type
//! has no name to look up, so pass the name of the global element that owns it
//! instead: the example falls back to that element's type when no named complex
//! type matches.
//!
//! The schema is loaded in XSD 1.0 mode by default. Set `XSD_VERSION=1.1` to
//! load it in XSD 1.1 mode (open content, all-group extensions, `notQName`);
//! that needs the `xsd11` feature to be meaningful for 1.1-only constructs.

use std::process::ExitCode;

use xsd_schema::compiler::{find_complex_type, inspect_content_model};
use xsd_schema::ids::TypeKey;
use xsd_schema::{SchemaSet, SchemaSetBuilder, XsdVersion};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (path, local, namespace) = match args.as_slice() {
        [path, local] => (path.as_str(), local.as_str(), None),
        [path, local, ns] => (path.as_str(), local.as_str(), Some(ns.as_str())),
        _ => {
            eprintln!(
                "usage: inspect_content_model <schema.xsd> <type-local-name> [namespace]\n\
                 \n\
                 example: inspect_content_model examples/books.xsd BookForm urn:books"
            );
            return ExitCode::FAILURE;
        }
    };

    let version = match std::env::var("XSD_VERSION").as_deref() {
        Ok("1.1") => XsdVersion::V1_1,
        _ => XsdVersion::V1_0,
    };

    let source = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("cannot read {path}: {err}");
            return ExitCode::FAILURE;
        }
    };
    let base_uri = format!("file://{path}");
    let compiled = SchemaSetBuilder::with_version(version)
        .add_source(&source, &base_uri)
        .and_then(|b| b.compile());
    let schema_set = match compiled {
        Ok(compiled) => compiled.into_schema_set(),
        Err(err) => {
            eprintln!("cannot compile {path}: {err}");
            return ExitCode::FAILURE;
        }
    };

    let key = find_complex_type(&schema_set, namespace, local)
        .or_else(|| complex_type_of_element(&schema_set, namespace, local));
    let Some(key) = key else {
        eprintln!(
            "no complex type named {} in {path}",
            display_name(namespace, local)
        );
        let mut names = named_complex_types(&schema_set);
        names.sort();
        if names.is_empty() {
            eprintln!("(this schema declares no named complex types)");
        } else {
            eprintln!("named complex types in this schema:");
            for name in names {
                eprintln!("  {name}");
            }
        }
        return ExitCode::FAILURE;
    };

    match inspect_content_model(&schema_set, key) {
        Ok(report) => {
            print!("{report}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!(
                "the content model of {} does not compile: {err}",
                display_name(namespace, local)
            );
            ExitCode::FAILURE
        }
    }
}

/// Fall back to a global element declaration of the same name, so anonymous
/// local complex types are reachable from the command line.
fn complex_type_of_element(
    schema_set: &SchemaSet,
    namespace: Option<&str>,
    local: &str,
) -> Option<xsd_schema::ids::ComplexTypeKey> {
    let ns = match namespace {
        Some(ns) => Some(schema_set.name_table.get(ns)?),
        None => None,
    };
    let local_id = schema_set.name_table.get(local)?;
    let element = schema_set.lookup_element(ns, local_id)?;
    match schema_set.arenas.get_element(element)?.resolved_type? {
        TypeKey::Complex(key) => Some(key),
        TypeKey::Simple(_) => None,
    }
}

fn named_complex_types(schema_set: &SchemaSet) -> Vec<String> {
    schema_set
        .arenas
        .complex_types
        .values()
        .filter_map(|ct| {
            let name = schema_set.name_table.resolve(ct.name?);
            Some(match ct.target_namespace {
                Some(ns) => format!("{{{}}}{name}", schema_set.name_table.resolve(ns)),
                None => name,
            })
        })
        .collect()
}

fn display_name(namespace: Option<&str>, local: &str) -> String {
    match namespace {
        Some(ns) => format!("{{{ns}}}{local}"),
        None => local.to_string(),
    }
}
