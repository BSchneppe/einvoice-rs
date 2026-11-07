//! The tree walks over an instance tree keep their depth on the heap (C125):
//! a deep document costs memory, not stack.
//!
//! `drive_navigator` and `drive_buffer_document` validate a tree a host already
//! holds, and the comparer behind `TreeComparer::deep_equal` and
//! `fn:deep-equal` walks two of them. A tree a host builds itself is not bounded
//! by any parse limit, so each walk must survive any depth on an ordinary
//! thread. Every test here walks a tree far deeper than a recursive walk
//! survives, on a thread with a 2 MiB stack (the default for spawned threads),
//! in whatever build the tests run in.
//!
//! A stack overflow aborts the process, so each walk runs in a child process
//! (the `*_child` tests, ignored in a normal run): a regression fails the test
//! that spawned it rather than taking the whole test run down.

use std::process::Command;

use xsd_schema::pipeline::load_and_process_schema;
use xsd_schema::validation::{
    drive_navigator, drive_quick_xml, CollectingValidationSink, SchemaValidator, SchemaValidity,
    ValidationFlags,
};
use xsd_schema::{DomNavigator, RoXmlNavigator, SchemaSet};

/// The depth of the trees the buffer-document tests build: the drives and the
/// comparers.
#[cfg(feature = "xsd11")]
const DEPTH: usize = 100_000;

/// The depth of the roxmltree documents. roxmltree's own parser recurses once
/// per element level, so a document must be parsed on a large stack; this
/// depth keeps that within 256 MiB in an unoptimized build and is still well
/// beyond what a recursive walk survives on 2 MiB.
const RO_DEPTH: usize = 10_000;

const STACK: usize = 2 << 20;

const CHILD_ENV: &str = "XSD_SCHEMA_DEEP_TREE_CHILD";

/// `e` nests itself; the innermost `e` holds a `v`, an `xs:int`.
const SCHEMA: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="e">
    <xs:complexType>
      <xs:choice>
        <xs:element ref="e"/>
        <xs:element name="v" type="xs:int"/>
      </xs:choice>
    </xs:complexType>
  </xs:element>
</xs:schema>"#;

/// Runs the ignored test `name` in a child process and asserts that it passed.
fn in_child(name: &str) {
    let exe = std::env::current_exe().expect("test executable");
    let output = Command::new(exe)
        .args(["--exact", name, "--ignored", "--test-threads=1"])
        .env(CHILD_ENV, "1")
        .output()
        .expect("child test runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("1 passed"),
        "{name} ({}):\n{stdout}\n{}",
        output.status,
        stderr.lines().rev().take(5).collect::<Vec<_>>().join("\n")
    );
}

/// Whether this process is a child run by [`in_child`].
fn is_child() -> bool {
    std::env::var_os(CHILD_ENV).is_some()
}

/// Runs `f` on a thread with a 2 MiB stack.
fn on_small_stack<F: FnOnce() + Send>(f: F) {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(STACK)
            .spawn_scoped(scope, f)
            .expect("thread")
            .join()
            .expect("no stack overflow, no panic");
    });
}

/// `depth` nested `e` elements around a `v` holding `value`, as text.
fn deep_text(depth: usize, value: &str) -> String {
    let mut text = String::with_capacity(depth * 7 + 16);
    for _ in 0..depth {
        text.push_str("<e>");
    }
    text.push_str("<v>");
    text.push_str(value);
    text.push_str("</v>");
    for _ in 0..depth {
        text.push_str("</e>");
    }
    text
}

/// Parses `text` with roxmltree on a 256 MiB stack.
fn parse_on_large_stack(text: &str) -> roxmltree::Document<'_> {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn_scoped(scope, || {
                roxmltree::Document::parse(text).expect("well-formed")
            })
            .expect("thread")
            .join()
            .expect("roxmltree parses")
    })
}

fn schema() -> SchemaSet {
    let mut schema_set = SchemaSet::new();
    load_and_process_schema(SCHEMA.as_bytes(), "deep.xsd", &mut schema_set, None)
        .expect("schema loads");
    schema_set
}

/// What a run reports: root validity, maximum depth, the errors as text.
type Outcome = (Option<SchemaValidity>, usize, Vec<String>);

fn validator(schema_set: &SchemaSet) -> SchemaValidator<'_> {
    SchemaValidator::new(schema_set, ValidationFlags::default())
}

fn streamed(schema_set: &SchemaSet, text: &str) -> Outcome {
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    let outcome = {
        let validator = validator(schema_set);
        let mut runtime = validator.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        drive_quick_xml(text.as_bytes(), &mut runtime, schema_set).expect("stream drive")
    };
    let errors = errors.iter().map(ToString::to_string).collect();
    (outcome.root_validity, outcome.max_depth, errors)
}

fn walked<N: DomNavigator>(schema_set: &SchemaSet, nav: &N) -> Outcome {
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    let outcome = {
        let validator = validator(schema_set);
        let mut runtime = validator.start_run(CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        });
        drive_navigator(nav, &mut runtime, schema_set).expect("navigator drive")
    };
    let errors = errors.iter().map(ToString::to_string).collect();
    (outcome.root_validity, outcome.max_depth, errors)
}

/// The tree walk of a deep tree reports what the streaming driver reports for
/// the same text: validity, depth and every error, instance path included.
fn assert_same_as_streamed(schema_set: &SchemaSet, text: &str, walked: Outcome, depth: usize) {
    let streamed = streamed(schema_set, text);
    assert_eq!(walked.1, depth + 1, "the walk saw every level");
    assert_eq!(walked, streamed, "the walk and the stream agree");
}

// ── drive_navigator over roxmltree ──────────────────────────────────────

#[test]
fn drive_navigator_walks_a_deep_roxmltree_document() {
    in_child("drive_navigator_walks_a_deep_roxmltree_document_child");
}

#[test]
#[ignore = "run in a child process by drive_navigator_walks_a_deep_roxmltree_document"]
fn drive_navigator_walks_a_deep_roxmltree_document_child() {
    if !is_child() {
        return;
    }
    for value in ["12", "twelve"] {
        let text = deep_text(RO_DEPTH, value);
        let doc = parse_on_large_stack(&text);
        on_small_stack(|| {
            let schema_set = schema();
            let outcome = walked(&schema_set, &RoXmlNavigator::new(&doc));
            let expected = if value == "12" {
                SchemaValidity::Valid
            } else {
                SchemaValidity::Invalid
            };
            assert_eq!(outcome.0, Some(expected));
            assert_same_as_streamed(&schema_set, &text, outcome, RO_DEPTH);
        });
    }
}

// ── The buffer document: drive_navigator, drive_buffer_document ─────────

#[cfg(feature = "xsd11")]
mod buffer {
    use super::*;
    use bumpalo::Bump;
    use xsd_schema::document::{BufferDocument, BufferDocumentBuilder, BufferDocumentOptions};
    use xsd_schema::namespace::NameTable;
    use xsd_schema::validation::drive_buffer_document;
    use xsd_schema::xpath::{TreeComparer, XPathContext, XPathExpr, XPathValue};

    /// The tree of [`deep_text`], built with the document builder: no parse,
    /// so no parse limit applies.
    fn built<'a>(
        arena: &'a Bump,
        names: &'a NameTable,
        depth: usize,
        value: &str,
    ) -> BufferDocument<'a> {
        let mut builder =
            BufferDocumentBuilder::new(arena, names, None, BufferDocumentOptions::default())
                .expect("builder");
        for _ in 0..depth {
            builder.start_element("e", "", "", &[]).expect("start e");
            builder.end_of_attributes();
        }
        builder.start_element("v", "", "", &[]).expect("start v");
        builder.end_of_attributes();
        builder.text(value);
        builder.end_element().expect("end v");
        for _ in 0..depth {
            builder.end_element().expect("end e");
        }
        builder.finalize().expect("finalize")
    }

    #[test]
    fn drive_navigator_walks_a_deep_buffer_document() {
        in_child("buffer::drive_navigator_walks_a_deep_buffer_document_child");
    }

    #[test]
    #[ignore = "run in a child process by drive_navigator_walks_a_deep_buffer_document"]
    fn drive_navigator_walks_a_deep_buffer_document_child() {
        if !is_child() {
            return;
        }
        on_small_stack(|| {
            let schema_set = schema();
            for value in ["12", "twelve"] {
                let arena = Bump::new();
                let doc = built(&arena, &schema_set.name_table, DEPTH, value);
                let outcome = walked(&schema_set, &doc.create_navigator());
                let text = deep_text(DEPTH, value);
                assert_same_as_streamed(&schema_set, &text, outcome, DEPTH);
            }
        });
    }

    #[test]
    fn drive_buffer_document_walks_a_deep_buffer_document() {
        in_child("buffer::drive_buffer_document_walks_a_deep_buffer_document_child");
    }

    #[test]
    #[ignore = "run in a child process by drive_buffer_document_walks_a_deep_buffer_document"]
    fn drive_buffer_document_walks_a_deep_buffer_document_child() {
        if !is_child() {
            return;
        }
        on_small_stack(|| {
            let schema_set = schema();
            for value in ["12", "twelve"] {
                let arena = Bump::new();
                let doc = built(&arena, &schema_set.name_table, DEPTH, value);
                let (mut errors, mut warnings) = (Vec::new(), Vec::new());
                let outcome = {
                    let validator = validator(&schema_set);
                    let mut runtime = validator.start_run(CollectingValidationSink {
                        errors: &mut errors,
                        warnings: &mut warnings,
                    });
                    drive_buffer_document(&doc, &mut runtime, &schema_set).expect("drive")
                };
                let errors = errors.iter().map(ToString::to_string).collect();
                let outcome = (outcome.root_validity, outcome.max_depth, errors);
                let text = deep_text(DEPTH, value);
                assert_same_as_streamed(&schema_set, &text, outcome, DEPTH);
            }
        });
    }

    // ── TreeComparer::deep_equal and fn:deep-equal ──────────────────────

    #[test]
    fn tree_comparer_compares_deep_buffer_documents() {
        in_child("buffer::tree_comparer_compares_deep_buffer_documents_child");
    }

    #[test]
    #[ignore = "run in a child process by tree_comparer_compares_deep_buffer_documents"]
    fn tree_comparer_compares_deep_buffer_documents_child() {
        if !is_child() {
            return;
        }
        on_small_stack(|| {
            let arena = Bump::new();
            let names = NameTable::new();
            let a = built(&arena, &names, DEPTH, "12");
            let b = built(&arena, &names, DEPTH, "12");
            let c = built(&arena, &names, DEPTH, "13");
            let comparer = TreeComparer::new();
            assert!(comparer.deep_equal(&a.create_navigator(), &b.create_navigator()));
            assert!(!comparer.deep_equal(&a.create_navigator(), &c.create_navigator()));
        });
    }

    #[test]
    fn tree_comparer_compares_deep_roxmltree_documents() {
        in_child("buffer::tree_comparer_compares_deep_roxmltree_documents_child");
    }

    #[test]
    #[ignore = "run in a child process by tree_comparer_compares_deep_roxmltree_documents"]
    fn tree_comparer_compares_deep_roxmltree_documents_child() {
        if !is_child() {
            return;
        }
        let (text_a, text_c) = (deep_text(RO_DEPTH, "12"), deep_text(RO_DEPTH, "13"));
        let (a, b, c) = (
            parse_on_large_stack(&text_a),
            parse_on_large_stack(&text_a),
            parse_on_large_stack(&text_c),
        );
        on_small_stack(|| {
            let comparer = TreeComparer::new();
            let nav = RoXmlNavigator::new;
            assert!(comparer.deep_equal(&nav(&a), &nav(&b)));
            assert!(!comparer.deep_equal(&nav(&a), &nav(&c)));
        });
    }

    #[test]
    fn fn_deep_equal_compares_deep_buffer_documents() {
        in_child("buffer::fn_deep_equal_compares_deep_buffer_documents_child");
    }

    #[test]
    #[ignore = "run in a child process by fn_deep_equal_compares_deep_buffer_documents"]
    fn fn_deep_equal_compares_deep_buffer_documents_child() {
        if !is_child() {
            return;
        }
        on_small_stack(|| {
            let arena = Bump::new();
            let names = NameTable::new();
            let a = built(&arena, &names, DEPTH, "12");
            let b = built(&arena, &names, DEPTH, "12");
            let c = built(&arena, &names, DEPTH, "13");
            let ctx = XPathContext::new(&names);
            let expr = XPathExpr::compile_with_vars("deep-equal($x, $y)", &ctx, &["x", "y"])
                .expect("compiles");
            for (y, expected) in [(&b, true), (&c, false)] {
                let result = expr
                    .evaluator(&ctx)
                    .run_with(|eval| {
                        let x = XPathValue::from_node(a.create_navigator());
                        let y = XPathValue::from_node(y.create_navigator());
                        eval.set_variable_by_name("x", x).expect("$x");
                        eval.set_variable_by_name("y", y).expect("$y");
                    })
                    .expect("evaluates");
                assert_eq!(result.as_bool(), Some(expected));
            }
        });
    }
}
