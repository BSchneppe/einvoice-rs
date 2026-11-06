//! A seeded, deterministic differential test of the well-formedness checks
//! against `roxmltree` (DTDs disallowed): for generated and mutated
//! documents, the quick-xml validation driver (and, with `xsd11`, the
//! document builder) accepts a document exactly when `roxmltree` does —
//! except where `roxmltree` is known to be more lenient than XML 1.0 or
//! Namespaces in XML, each listed in [`roxmltree_leniency`] with the rule it
//! skips.
//!
//! Documents containing a document type declaration are left out: the
//! readers accept one in the prolog (and report it to the handler), while
//! `roxmltree` with `allow_dtd: false` refuses every one.

use xsd_schema::pipeline::load_and_process_schema;
use xsd_schema::validation::{
    drive_quick_xml_with, CollectingValidationSink, DriveWithError, NoopHandler, SchemaValidator,
    ValidationFlags,
};
use xsd_schema::xml::{NotWellFormed, WellFormednessRule as R};
use xsd_schema::SchemaSet;

/// xorshift64*: deterministic, no clock, no dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }
    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len())]
    }
}

const ELEMENT_NAMES: &[&str] = &["r", "a", "b", "p:e", "q:f", "é", "x-y.z", "_u1", "p:é"];
const ATTRIBUTE_NAMES: &[&str] = &["a", "b", "c", "p:a", "q:a", "xml:lang", "p:b", "d-1"];
const ATTRIBUTE_VALUES: &[&str] = &[
    "1",
    "",
    "x y",
    "&amp;",
    "&lt;&gt;",
    "&#65;",
    "&#x1D11E;",
    "&#9;",
    "\t",
    "é",
    ">",
    "]]>",
    "--",
    "&#1;",
    "&#xFFFE;",
    "&",
    "<",
    "\u{1}",
];
const TEXTS: &[&str] = &[
    "t",
    " ",
    "\n  ",
    "&amp;",
    "&#65;",
    "&#x42;",
    "a&lt;b",
    "]]",
    ">",
    "é",
    "\r\n",
    "&#000065;",
    "&#1;",
    "&#xD800;",
    "]]>",
    "\u{1}",
    "\u{FFFF}",
    "&",
    "&nope;",
];
const NAMESPACES: &[&str] = &[
    "urn:p",
    "urn:q",
    "urn:p",
    "",
    "http://www.w3.org/XML/1998/namespace",
    "http://www.w3.org/2000/xmlns/",
];
/// Inserted by mutations.
const SNIPPETS: &[&str] = &[
    "<",
    ">",
    "/",
    "&",
    ";",
    ":",
    "\"",
    "'",
    "=",
    " ",
    "!",
    "?",
    "-",
    "[",
    "]",
    "#",
    "x",
    "1",
    "a",
    "é",
    "\u{1}",
    "\u{FFFE}",
    "\t",
    "\n",
    "<a>",
    "</a>",
    "<b/>",
    "<!---->",
    "<?pi x?>",
    "<![CDATA[c]]>",
    "&amp;",
    "&#1;",
    "&#65;",
    "--",
    "]]>",
    " p:z=\"1\"",
    " xmlns:p=\"\"",
    " xmlns:q=\"urn:p\"",
    " q:a=\"2\"",
    "<?xml version=\"1.0\"?>",
    "xmlns",
    "a:b:c",
];

struct Gen<'r> {
    rng: &'r mut Rng,
    out: String,
}

impl Gen<'_> {
    fn misc(&mut self) {
        for _ in 0..self.rng.below(3) {
            match self.rng.below(5) {
                0 => self.out.push_str("<!-- c -->"),
                1 => self.out.push_str("<?pi data?>"),
                2 => self.out.push(' '),
                3 => self.out.push('\n'),
                _ => {
                    let text = self
                        .rng
                        .pick(&["<!--a-b-->", "<?t?>", "\r\n", "<!-- - -->"]);
                    self.out.push_str(text)
                }
            }
        }
    }

    fn element(&mut self, depth: usize) {
        let name = self.rng.pick(ELEMENT_NAMES);
        self.out.push('<');
        self.out.push_str(name);
        // Namespace declarations: usually bind what the names use.
        if depth == 0 || self.rng.chance(20) {
            if self.rng.chance(90) {
                self.out.push_str(" xmlns:p=\"urn:p\"");
            }
            if self.rng.chance(70) {
                let ns = self.rng.pick(NAMESPACES);
                self.out.push_str(&format!(" xmlns:q=\"{ns}\""));
            }
            if self.rng.chance(20) {
                let ns = self.rng.pick(NAMESPACES);
                self.out.push_str(&format!(" xmlns=\"{ns}\""));
            }
        }
        for _ in 0..self.rng.below(3) {
            let attr = self.rng.pick(ATTRIBUTE_NAMES);
            let value = self.rng.pick(ATTRIBUTE_VALUES);
            let quote = if self.rng.chance(50) { '"' } else { '\'' };
            let space = self.rng.pick(&[" ", "  ", "\t", "\n"]);
            self.out
                .push_str(&format!("{space}{attr}={quote}{value}{quote}"));
        }
        if depth >= 3 || self.rng.chance(25) {
            self.out
                .push_str(if self.rng.chance(80) { "/>" } else { " />" });
            return;
        }
        self.out.push('>');
        for _ in 0..self.rng.below(4) {
            match self.rng.below(7) {
                0 | 1 => self.element(depth + 1),
                2 | 3 => {
                    let text = self.rng.pick(TEXTS);
                    self.out.push_str(text);
                }
                4 => self
                    .out
                    .push_str(self.rng.pick(&["<!-- c -->", "<!-- a -- b -->"])),
                5 => self
                    .out
                    .push_str(self.rng.pick(&["<?pi d?>", "<?XML d?>", "<?q:t?>"])),
                _ => self
                    .out
                    .push_str(self.rng.pick(&["<![CDATA[x<&]]>", "<![CDATA[\u{B}]]>"])),
            }
        }
        self.out.push_str("</");
        self.out.push_str(name);
        self.out.push('>');
    }

    fn document(&mut self) {
        if self.rng.chance(50) {
            let decl = self.rng.pick(&[
                "<?xml version=\"1.0\"?>",
                "<?xml version='1.0' encoding='UTF-8'?>",
                "<?xml version=\"1.0\" standalone=\"yes\"?>",
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\" ?>",
                "<?xml encoding=\"UTF-8\" version=\"1.0\"?>",
                "<?xml version=\"1.0\"encoding=\"UTF-8\"?>",
            ]);
            self.out.push_str(decl);
        }
        self.misc();
        self.element(0);
        self.misc();
    }
}

/// Up to three edits at character boundaries.
fn mutate(rng: &mut Rng, doc: &str) -> String {
    let mut chars: Vec<char> = doc.chars().collect();
    for _ in 0..rng.below(4) {
        let at = rng.below(chars.len() + 1);
        match rng.below(3) {
            0 => {
                let snippet: Vec<char> = rng.pick(SNIPPETS).chars().collect();
                chars.splice(at..at, snippet);
            }
            1 if at < chars.len() => {
                let n = (1 + rng.below(3)).min(chars.len() - at);
                chars.drain(at..at + n);
            }
            _ if at < chars.len() => {
                let snippet: Vec<char> = rng.pick(SNIPPETS).chars().collect();
                chars.splice(at..at + 1, snippet);
            }
            _ => {}
        }
    }
    chars.into_iter().collect()
}

/// Whether the refusal `e` of `doc` is one `roxmltree` does not make, by
/// design or omission (verified against `roxmltree` 0.19 by hand):
///
/// * WFC: Legal Character for a reference to a surrogate or above
///   U+10FFFF (`&#xD800;`, `&#x110000;`: roxmltree accepts them);
/// * NSC: No Prefix Undeclaring (`xmlns:p=""`) and the `xmlns` prefix
///   declared (`xmlns:xmlns="…"`);
/// * the values of the XML declaration's pseudo-attributes ([26]
///   `VersionNum`, [81] `EncName`, [32] `standalone`) — roxmltree checks
///   their order but not their values;
/// * [17] `PITarget` (`xml` in another case) and an `NCName` target
///   (Namespaces §7), and a target not followed by white space;
/// * a qualified name with an empty prefix (`<:r/>`);
/// * a repeated `xmlns` attribute on one tag (WFC: Unique Att Spec).
fn roxmltree_leniency(doc: &str, e: &NotWellFormed) -> Option<&'static str> {
    let at = &doc.as_bytes()[e.position() as usize..];
    let starts = |s: &str| at.starts_with(s.as_bytes());
    match e.rule() {
        R::LegalCharacter => {
            let text = std::str::from_utf8(at).ok()?;
            let reference = text.strip_prefix("&#")?.split(';').next()?;
            let value = match reference.strip_prefix('x') {
                Some(hex) => u64::from_str_radix(hex.trim_start_matches('0'), 16).ok(),
                None => reference.trim_start_matches('0').parse::<u64>().ok(),
            };
            match value {
                Some(v) if (0xD800..=0xDFFF).contains(&v) || v > 0x10FFFF => {
                    Some("a reference to a surrogate or above U+10FFFF")
                }
                _ => None,
            }
        }
        R::PrefixUndeclaring => Some("xmlns:p=\"\""),
        // roxmltree accepts a repeated default namespace declaration
        // (`xmlns="a" xmlns="b"` on one tag).
        R::UniqueAttSpec if starts("xmlns=") => Some("a repeated xmlns attribute"),
        R::ReservedPrefix if starts("xmlns:xmlns") => Some("xmlns:xmlns declared"),
        R::XmlDeclaration if doc.starts_with("<?xml") && e.position() > 0 => {
            Some("an XML declaration's values")
        }
        R::ProcessingInstruction => Some("a PI target"),
        R::QName if doc[..e.position() as usize].ends_with("<?") || starts(":") => {
            Some("a PI target with a colon, or an empty prefix")
        }
        _ => None,
    }
}

const XSD: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="r"/>
</xs:schema>"#;

fn drive(ss: &SchemaSet, doc: &str) -> Result<(), DriveWithError<std::convert::Infallible>> {
    let validator = SchemaValidator::new(ss, ValidationFlags::default());
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    let mut runtime = validator.start_run(CollectingValidationSink {
        errors: &mut errors,
        warnings: &mut warnings,
    });
    drive_quick_xml_with(doc.as_bytes(), &mut runtime, ss, &mut NoopHandler).map(|_| ())
}

#[cfg(feature = "xsd11")]
fn build(doc: &str) -> Result<(), xsd_schema::document::BufferDocumentError> {
    let arena = bumpalo::Bump::new();
    let names = xsd_schema::namespace::NameTable::new();
    xsd_schema::document::BufferDocument::from_reader_default(doc.as_bytes(), &arena, &names)
        .map(|_| ())
}

#[test]
fn the_readers_accept_what_roxmltree_accepts() {
    let mut ss = SchemaSet::new();
    load_and_process_schema(XSD.as_bytes(), "file:///diff.xsd", &mut ss, None).expect("schema");
    let options = roxmltree::ParsingOptions {
        allow_dtd: false,
        ..Default::default()
    };
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let (mut both_accept, mut both_refuse, mut lenient) = (0usize, 0usize, 0usize);
    let mut disagreements = Vec::new();
    for i in 0..20_000 {
        let mut g = Gen {
            rng: &mut rng,
            out: String::new(),
        };
        g.document();
        let generated = g.out;
        let doc = if i % 4 == 0 {
            generated
        } else {
            mutate(&mut rng, &generated)
        };
        if doc.to_ascii_lowercase().contains("<!doctype") {
            continue;
        }
        let rox = roxmltree::Document::parse_with_options(&doc, options);
        let ours = drive(&ss, &doc);
        match (&ours, &rox) {
            (Ok(()), Ok(_)) => both_accept += 1,
            (Err(_), Err(_)) => both_refuse += 1,
            (Err(DriveWithError::NotWellFormed(e)), Ok(_))
                if roxmltree_leniency(&doc, e).is_some() =>
            {
                lenient += 1
            }

            _ => disagreements.push(format!(
                "{doc:?}\n    ours: {:?}\n    roxmltree: {:?}",
                ours.as_ref().err().map(|e| e.to_string()),
                rox.as_ref().err().map(|e| e.to_string())
            )),
        }
        // Every refusal is `NotWellFormed` (I/O aside, which a slice cannot
        // raise).
        if let Err(e) = &ours {
            assert!(
                matches!(e, DriveWithError::NotWellFormed(_)),
                "{doc:?}: {e:?}"
            );
        }
        #[cfg(feature = "xsd11")]
        assert_eq!(
            build(&doc).is_ok(),
            ours.is_ok(),
            "the document builder and the driver disagree on {doc:?}: {:?} / {:?}",
            build(&doc).err().map(|e| e.to_string()),
            ours.as_ref().err().map(|e| e.to_string())
        );
    }
    assert!(
        disagreements.is_empty(),
        "{} disagreement(s) with roxmltree (accept {both_accept}, refuse {both_refuse}, roxmltree lenient {lenient}):\n{}",
        disagreements.len(),
        disagreements.iter().take(25).cloned().collect::<Vec<_>>().join("\n")
    );
    println!("both accept {both_accept}, both refuse {both_refuse}, roxmltree lenient {lenient}");
    // The generator reaches both outcomes and the known leniencies.
    assert!(both_accept > 1_500, "{both_accept} accepted");
    assert!(both_refuse > 5_000, "{both_refuse} refused");
    assert!(lenient > 0, "no roxmltree leniency reached");
}
