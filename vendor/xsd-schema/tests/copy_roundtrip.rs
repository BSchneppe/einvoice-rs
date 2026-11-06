//! `copy(x) ≡ x` over a sample of the W3C XSD conformance corpus.
//!
//! [`copy_subtree`] promises two things that unit tests can only check on
//! hand-written fixtures: that a copied subtree says exactly what the original
//! said, and that it stands on its own — every name resolving through the
//! declarations the copy carries, which is the invariant that lets
//! [`serialize`] refuse an unbound name outright.
//!
//! This test holds both promises up against real documents. For a spread of
//! files across the corpus it copies every element child of the document
//! element (or the document element itself when it has none) into a fresh
//! builder, writes source subtree and copy out with
//! [`serialize::to_string`](xsd_schema::document::serialize::to_string), parses
//! both with `roxmltree` — an independent parser, so a copy that is not
//! namespace-well-formed cannot slip through — and compares canonical forms:
//! expanded element names, attributes sorted by `(uri, local)` with their
//! values, merged text, comments and processing instructions. Namespace
//! declarations are not compared: the copy declares what its names need, the
//! source subtree carries whatever its ancestors declared, and the names
//! resolving to the same URIs is the point.
//!
//! The suite lives at `$XSDTESTS_DIR`, or `../../xsdtests` relative to the
//! crate. When it is not there the test prints a message and passes, the same
//! convention the conformance driver uses for its `--test-suite` root.
//!
//! [`copy_subtree`]: xsd_schema::document::BufferDocumentBuilder::copy_subtree
//! [`serialize`]: xsd_schema::document::serialize

use std::path::{Path, PathBuf};

use bumpalo::Bump;
use xsd_schema::document::{serialize, BufferDocument, BufferDocumentBuilder};
use xsd_schema::document::{BufferDocumentOptions, CopyOptions};
use xsd_schema::namespace::NameTable;
use xsd_schema::navigator::{DomNavigator, DomNodeType};

/// How many files to sample, spread evenly over the sorted corpus.
const SAMPLE: usize = 512;

/// Where the corpus is.
fn suite_root() -> PathBuf {
    match std::env::var_os("XSDTESTS_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../xsdtests"),
    }
}

/// Every `*.xml` file under `root`, in a stable (sorted) order.
fn collect_xml_files(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(t) if t.is_dir() => dirs.push(path),
                Ok(t) if t.is_file() && path.extension().is_some_and(|e| e == "xml") => {
                    found.push(path);
                }
                _ => {}
            }
        }
    }
    found.sort();
    found
}

// ── Canonical form, the same shape `serialize_roundtrip` compares ────────

fn canonical(doc: &roxmltree::Document<'_>) -> String {
    let mut out = String::new();
    render_children(doc.root(), &mut out);
    out
}

fn render_children(node: roxmltree::Node<'_, '_>, out: &mut String) {
    let mut pending = String::new();
    for child in node.children() {
        if child.is_text() {
            pending.push_str(child.text().unwrap_or_default());
            continue;
        }
        flush_text(&mut pending, out);
        render_node(child, out);
    }
    flush_text(&mut pending, out);
}

fn flush_text(pending: &mut String, out: &mut String) {
    if !pending.is_empty() {
        out.push_str("text(");
        out.push_str(pending);
        out.push_str(")\n");
        pending.clear();
    }
}

fn render_node(node: roxmltree::Node<'_, '_>, out: &mut String) {
    match node.node_type() {
        roxmltree::NodeType::Element => {
            let name = node.tag_name();
            out.push_str("element {");
            out.push_str(name.namespace().unwrap_or(""));
            out.push('}');
            out.push_str(name.name());
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
                out.push_str(&format!(" {{{uri}}}{local}={value:?}"));
            }
            out.push('\n');
            render_children(node, out);
            out.push_str("/element\n");
        }
        roxmltree::NodeType::Comment => {
            out.push_str("comment(");
            out.push_str(node.text().unwrap_or_default());
            out.push_str(")\n");
        }
        roxmltree::NodeType::PI => {
            let pi = node.pi().expect("a PI node has PI data");
            out.push_str("pi(");
            out.push_str(pi.target);
            out.push(' ');
            out.push_str(pi.value.unwrap_or_default());
            out.push_str(")\n");
        }
        roxmltree::NodeType::Root | roxmltree::NodeType::Text => {}
    }
}

// ── The test ────────────────────────────────────────────────────────────

/// One subtree comparison. `Ok(true)` compared, `Ok(false)` skipped for a
/// reason the source itself has (content XML 1.0 cannot express).
fn compare_subtree<N: DomNavigator + xsd_schema::document::CopySource>(
    source: &N,
    names: &NameTable,
) -> Result<bool, String> {
    let opts = Default::default();
    let source_xml = match serialize::to_string(source, &opts) {
        Ok(xml) => xml,
        // The source holds something XML 1.0 cannot write (an XML 1.1
        // character, say): not a copy failure.
        Err(_) => return Ok(false),
    };
    let source_tree = match roxmltree::Document::parse(&source_xml) {
        Ok(tree) => tree,
        Err(_) => return Ok(false),
    };

    let arena = Bump::new();
    let mut builder =
        BufferDocumentBuilder::new(&arena, names, None, BufferDocumentOptions::default())
            .map_err(|e| format!("builder: {e}"))?;
    builder
        .copy_subtree(source, CopyOptions::default())
        .map_err(|e| format!("copy: {e}"))?;
    let copied = builder.finalize().map_err(|e| format!("finalize: {e}"))?;

    let copy_xml = serialize::to_string(&copied.create_navigator(), &opts)
        .map_err(|e| format!("the copy could not be written: {e}"))?;
    let copy_tree = roxmltree::Document::parse(&copy_xml)
        .map_err(|e| format!("the copy does not re-parse ({e}): {copy_xml}"))?;

    let (expected, actual) = (canonical(&source_tree), canonical(&copy_tree));
    if expected != actual {
        return Err(format!(
            "canonical forms differ\n--- source ---\n{expected}\n--- copy ---\n{actual}"
        ));
    }
    Ok(true)
}

#[test]
fn copies_of_corpus_subtrees_say_what_the_originals_said() {
    let root = suite_root();
    if !root.is_dir() {
        println!(
            "skipping: no XSD test suite at {} (set XSDTESTS_DIR)",
            root.display()
        );
        return;
    }

    let all = collect_xml_files(&root);
    if all.is_empty() {
        println!("skipping: no *.xml files under {}", root.display());
        return;
    }
    let stride = (all.len() / SAMPLE).max(1);
    let sample: Vec<&PathBuf> = all.iter().step_by(stride).take(SAMPLE).collect();

    let mut files = 0usize;
    let mut subtrees = 0usize;
    let mut skipped = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for path in sample {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        // A DOCTYPE brings entity declarations the builder cannot resolve.
        if bytes.windows(9).any(|w| w == b"<!DOCTYPE") {
            skipped += 1;
            continue;
        }
        let arena = Bump::new();
        let names = NameTable::new();
        let doc = match BufferDocument::from_reader_default(bytes.as_slice(), &arena, &names) {
            Ok(doc) => doc,
            // The corpus contains deliberately malformed instances.
            Err(_) => {
                skipped += 1;
                continue;
            }
        };

        let mut document_element = doc.create_navigator();
        if !document_element.move_to_first_child() {
            skipped += 1;
            continue;
        }
        while document_element.node_type() != DomNodeType::Element {
            if !document_element.move_to_next_sibling() {
                break;
            }
        }
        if document_element.node_type() != DomNodeType::Element {
            skipped += 1;
            continue;
        }
        files += 1;

        // Every element child of the document element, or the document
        // element itself when it has no element children.
        let mut roots = Vec::new();
        let mut child = document_element.clone();
        if child.move_to_first_child() {
            loop {
                if child.node_type() == DomNodeType::Element {
                    roots.push(child.clone());
                }
                if !child.move_to_next_sibling() {
                    break;
                }
            }
        }
        if roots.is_empty() {
            roots.push(document_element.clone());
        }

        for source in &roots {
            match compare_subtree(source, &names) {
                Ok(true) => subtrees += 1,
                Ok(false) => skipped += 1,
                Err(reason) => failures.push(format!("{}: {reason}", path.display())),
            }
        }
    }

    println!("copy round trip: {files} files, {subtrees} subtrees compared, {skipped} skipped");
    assert!(subtrees > 0, "the sample compared nothing");
    assert!(
        failures.is_empty(),
        "{} mismatch(es):\n{}",
        failures.len(),
        failures.join("\n"),
    );
}
