//! Emits raw diagnostics for differential comparison with the unmodified backend.
#[path = "../tests/support/mod.rs"]
mod support;
use support::*;

fn main() {
    println!(
        "RAW layouts={:?}",
        (
            std::mem::size_of::<xsd_schema::validation::errors::ValidationError>(),
            std::mem::size_of::<Result<(), xsd_schema::validation::errors::ValidationError>>()
        )
    );
    let schema = schema(XSD);
    for (name, xml) in CASES {
        let (result, errors, warnings) = capture(&schema, xml);
        println!(
            "RAW case={name:?} validity={:?} errors={} warnings={}",
            result.root_validity,
            errors.len(),
            warnings.len()
        );
        for e in errors {
            println!(
                "RAW error={:?}",
                (
                    e.constraint,
                    &e.message,
                    &e.location,
                    e.subject(),
                    e.instance_path(),
                    e.element_path(),
                    e.to_string()
                )
            );
        }
        for w in warnings {
            println!(
                "RAW warning={:?}",
                (
                    w.code,
                    &w.message,
                    &w.location,
                    w.subject(),
                    w.instance_path()
                )
            );
        }
    }
}
