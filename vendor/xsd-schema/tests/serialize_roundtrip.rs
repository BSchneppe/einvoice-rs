//! `serialize(parse(x)) ≡ x` over the whole W3C XSD conformance corpus.
//!
//! The corpus is the largest pile of real-world XML within reach — 26k instance
//! documents written by many hands, with namespaces, mixed content, comments,
//! processing instructions, CDATA, entity references and every flavour of
//! whitespace. Every file is parsed into a
//! [`BufferDocument`](xsd_schema::document::BufferDocument), written back out
//! with [`serialize::to_string`](xsd_schema::document::serialize::to_string) and
//! then compared with the original along two axes:
//!
//! 1. **Canonical form** (this file's `canonical`): both the original bytes and
//!    the serialized output are parsed with `roxmltree` — an independent parser,
//!    which also proves the output is namespace-well-formed XML, something
//!    `BufferDocument`'s streaming reader does not check — and rendered as
//!    expanded element names, attributes sorted by `(uri, local)` with their
//!    values, merged text, comments and processing instructions.
//! 2. **Prefix fidelity** (`compare_prefixes`): the original tree and a re-parse
//!    of the output are walked side by side through `DomNavigator`, comparing
//!    the prefix of every element and attribute. The serializer preserves
//!    prefixes, so a change is a bug — and `roxmltree` cannot report a prefix,
//!    so the canonical form above cannot see one.
//!
//! Namespace declarations themselves are not part of either comparison: a
//! declaration that merely repeats an inherited one is dropped by design, and
//! the declarations that remain are proven by the names still resolving to the
//! same namespace URIs.
//!
//! What a parse cannot preserve is skipped rather than papered over: the XML
//! declaration (the canonical form ignores it), a `<!DOCTYPE>` with its entity
//! declarations (those files are skipped outright), and CDATA section markers
//! (the canonical form compares text content, not how it was written).
//! Everything else — processing-instruction data included, trailing whitespace
//! and all — is compared exactly.
//!
//! The suite lives at `$XSDTESTS_DIR`, or `../../xsdtests` relative to the
//! crate. When it is not there the test prints a message and passes, the same
//! convention the conformance driver uses for its `--test-suite` root.

use std::path::{Path, PathBuf};
use std::time::Instant;

use bumpalo::Bump;
use xsd_schema::document::{serialize, BufferDocument, SerializeOptions};
use xsd_schema::namespace::NameTable;
use xsd_schema::navigator::{DomNavigator, DomNodeType};

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

// ── Comparison 1: canonical form through an independent parser ───────────

/// Renders a `roxmltree` document as the canonical form described at the top of
/// this file.
fn canonical(doc: &roxmltree::Document<'_>) -> String {
    let mut out = String::new();
    render_children(doc.root(), 0, &mut out);
    out
}

fn render_children(node: roxmltree::Node<'_, '_>, depth: usize, out: &mut String) {
    let mut pending = String::new();
    for child in node.children() {
        if child.is_text() {
            // Adjacent text runs (CDATA sections, entity references) are one
            // text node in the data model.
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
    // Text outside the document element is not kept by the parse, and is
    // whitespace in every well-formed document anyway.
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

// ── Comparison 2: prefixes, which roxmltree does not expose ──────────────

/// Walks two trees in parallel comparing the prefix, local name and namespace
/// URI of every element and attribute. Returns the first difference found.
fn compare_prefixes<A: DomNavigator, B: DomNavigator>(left: &A, right: &B) -> Result<(), String> {
    let mut a = left.clone();
    let mut b = right.clone();
    let mut depth = 0usize;
    loop {
        if a.node_type() != b.node_type() {
            return Err(format!(
                "node kind {:?} became {:?}",
                a.node_type(),
                b.node_type()
            ));
        }
        if a.node_type() == DomNodeType::Element {
            compare_names(&a, &b, "element")?;
            let mut aa = a.clone();
            let mut ba = b.clone();
            let mut more_a = aa.move_to_first_attribute();
            let mut more_b = ba.move_to_first_attribute();
            while more_a && more_b {
                compare_names(&aa, &ba, "attribute")?;
                more_a = aa.move_to_next_attribute();
                more_b = ba.move_to_next_attribute();
            }
            if more_a != more_b {
                return Err(format!("attribute count differs on <{}>", a.name()));
            }
        }
        // Depth-first, in lockstep.
        let a_down = a.move_to_first_child();
        let b_down = b.move_to_first_child();
        if a_down != b_down {
            return Err(format!("child presence differs at <{}>", a.name()));
        }
        if a_down {
            depth += 1;
            continue;
        }
        loop {
            if depth == 0 {
                return Ok(());
            }
            let a_next = a.move_to_next_sibling();
            let b_next = b.move_to_next_sibling();
            if a_next != b_next {
                return Err("sibling count differs".to_string());
            }
            if a_next {
                break;
            }
            a.move_to_parent();
            b.move_to_parent();
            depth -= 1;
        }
    }
}

fn compare_names<A: DomNavigator, B: DomNavigator>(a: &A, b: &B, what: &str) -> Result<(), String> {
    if a.prefix() != b.prefix() || a.local_name() != b.local_name() {
        return Err(format!(
            "{what} name {}:{} became {}:{}",
            a.prefix(),
            a.local_name(),
            b.prefix(),
            b.local_name()
        ));
    }
    if a.namespace_uri() != b.namespace_uri() {
        return Err(format!(
            "{what} {} moved from namespace {:?} to {:?}",
            a.name(),
            a.namespace_uri(),
            b.namespace_uri()
        ));
    }
    Ok(())
}

// ── The test ─────────────────────────────────────────────────────────────

#[derive(Default)]
struct Counts {
    total: usize,
    doctype_skipped: usize,
    unparseable: usize,
    rejected_by_roxmltree: usize,
    /// XML 1.1 documents holding a character XML 1.0 has no way to write.
    xml11_char_skipped: usize,
    compared: usize,
    serialize_errors: Vec<String>,
    mismatches: Vec<String>,
}

/// Whether the document declares XML 1.1, which allows C0 control characters
/// (as references) that XML 1.0 — and therefore this serializer — cannot write
/// at all.
fn declares_xml_11(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(128)];
    let head = String::from_utf8_lossy(head);
    let Some(decl) = head.strip_prefix("<?xml") else {
        return false;
    };
    let decl = &decl[..decl.find("?>").unwrap_or(decl.len())];
    decl.contains("version=\"1.1\"") || decl.contains("version='1.1'")
}

/// Round-trips every instance document in the corpus.
///
/// Takes about 12 seconds in a debug build and 2 in release, so it runs by
/// default rather than behind `#[ignore]`.
#[test]
fn serialize_round_trips_the_conformance_corpus() {
    let root = suite_root();
    if !root.is_dir() {
        println!(
            "skipping: XSD test suite not found at {} (set XSDTESTS_DIR)",
            root.display()
        );
        return;
    }

    let started = Instant::now();
    let files = collect_xml_files(&root);
    let mut counts = Counts {
        total: files.len(),
        ..Default::default()
    };

    for path in &files {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        // A DOCTYPE brings entity declarations the reader cannot resolve, so
        // there is nothing to compare.
        if bytes.windows(9).any(|w| w == b"<!DOCTYPE") {
            counts.doctype_skipped += 1;
            continue;
        }
        let text = match std::str::from_utf8(&bytes) {
            Ok(text) => text,
            Err(_) => {
                counts.unparseable += 1;
                continue;
            }
        };

        let arena = Bump::new();
        let names = NameTable::new();
        // The suite is full of deliberately malformed instances.
        let Ok(doc) = BufferDocument::from_reader_default(text.as_bytes(), &arena, &names) else {
            counts.unparseable += 1;
            continue;
        };

        // Compact output: an exact text round trip is the contract of this mode,
        // where formatted output would add layout whitespace text (§4.7).
        let compact = SerializeOptions {
            indent: None,
            ..SerializeOptions::default()
        };
        let output = match serialize::to_string(&doc.create_navigator(), &compact) {
            Ok(output) => output,
            Err(serialize::SerializeError::InvalidChar { .. }) if declares_xml_11(&bytes) => {
                counts.xml11_char_skipped += 1;
                continue;
            }
            Err(e) => {
                counts
                    .serialize_errors
                    .push(format!("{}: {e}", path.display()));
                continue;
            }
        };

        // An independent parser judges both sides; if it will not take the
        // original, it has nothing to say about the output either.
        let Ok(before) = roxmltree::Document::parse(text) else {
            counts.rejected_by_roxmltree += 1;
            continue;
        };
        let after = match roxmltree::Document::parse(&output) {
            Ok(after) => after,
            Err(e) => {
                counts
                    .mismatches
                    .push(format!("{}: output does not parse: {e}", path.display()));
                continue;
            }
        };
        counts.compared += 1;

        let (want, got) = (canonical(&before), canonical(&after));
        if want != got {
            counts.mismatches.push(format!(
                "{}: canonical form differs\n--- parsed original\n{want}--- serialized again\n{got}",
                path.display()
            ));
            continue;
        }

        let reparsed_arena = Bump::new();
        let reparsed_names = NameTable::new();
        let reparsed = BufferDocument::from_reader_default(
            output.as_bytes(),
            &reparsed_arena,
            &reparsed_names,
        )
        .expect("output that roxmltree accepts also parses here");
        if let Err(e) = compare_prefixes(&doc.create_navigator(), &reparsed.create_navigator()) {
            counts.mismatches.push(format!("{}: {e}", path.display()));
        }
    }

    let elapsed = started.elapsed();
    println!(
        "round-trip over {}: {} files, {} doctype-skipped, {} unparseable, \
         {} rejected by roxmltree, {} XML 1.1 characters, {} compared, \
         {} serialize errors, {} mismatches, {:.1}s",
        root.display(),
        counts.total,
        counts.doctype_skipped,
        counts.unparseable,
        counts.rejected_by_roxmltree,
        counts.xml11_char_skipped,
        counts.compared,
        counts.serialize_errors.len(),
        counts.mismatches.len(),
        elapsed.as_secs_f64(),
    );

    for line in counts.serialize_errors.iter().take(10) {
        println!("serialize error: {line}");
    }
    for line in counts.mismatches.iter().take(5) {
        println!("mismatch: {line}");
    }
    assert!(
        counts.serialize_errors.is_empty(),
        "{} files failed to serialize",
        counts.serialize_errors.len()
    );
    assert!(
        counts.mismatches.is_empty(),
        "{} files did not round-trip",
        counts.mismatches.len()
    );
    assert!(counts.compared > 20_000, "suspiciously few files compared");
}
