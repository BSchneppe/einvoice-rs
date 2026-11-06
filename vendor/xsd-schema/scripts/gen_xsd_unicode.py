#!/usr/bin/env python3
"""Generate src/regex_xsd_unicode.rs from the Unicode 3.0.0 UCD.

Regenerate by running:
    python3 scripts/gen_xsd_unicode.py

The UCD file is NOT checked in. On first run (or if the cached file is
missing / has the wrong hash) this script downloads it from
`UCD_URL` below and verifies the SHA-256. The cache lives under
`assets/unicode/` which is gitignored.

Why Unicode 3.0 (not 3.1) even though XSD 1.0 Part 2 §F.1.1 cites 3.1:
the MS `msData/regex/reJ*` tests (W3C Bugzilla 4113, status="queried")
were authored against a pre-3.1 / BMP-only implementation. U+1D7A8 and
friends are Lu/Ll/... in Unicode 3.1.0 already, so a 3.1-aligned pin
would produce the same matches as modern Unicode and would NOT reject
the reJ instances that the tests expect to reject. The tests
effectively encode a BMP-only pin; Unicode 3.0 (plane 0 only) is the
closest normative Unicode version that yields the test-expected
behavior. See plan doc + team-lead correspondence for full reasoning.

Format handling: the UCD uses <X, First>/<X, Last> pairs for CJK,
Hangul, and private-use blocks. Surrogates (Cs) are never emitted
(§G.4.2.2 spec note). Cn (unassigned) is also not emitted — XSD 1.0
reJ tests do not probe it in a version-distinguishing way.
"""

import hashlib
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CACHE_DIR = ROOT / "assets" / "unicode"
SRC = CACHE_DIR / "UnicodeData-3.0.0.txt"
OUT = ROOT / "src" / "regex_xsd_unicode.rs"

UCD_URL = "https://www.unicode.org/Public/3.0-Update/UnicodeData-3.0.0.txt"
UCD_SHA256 = "f41d967bc458ee106f0c3948bfad71cd0860d96c49304e3fd02eaf2bbae4b6d9"


def ensure_ucd():
    """Fetch UnicodeData-3.0.0.txt on demand and verify SHA-256."""
    if SRC.exists():
        digest = hashlib.sha256(SRC.read_bytes()).hexdigest()
        if digest == UCD_SHA256:
            return
        print(f"[gen_xsd_unicode] stale cache at {SRC} (sha256 {digest}), refetching")
    else:
        CACHE_DIR.mkdir(parents=True, exist_ok=True)
        print(f"[gen_xsd_unicode] fetching {UCD_URL}")
    with urllib.request.urlopen(UCD_URL) as resp:
        data = resp.read()
    digest = hashlib.sha256(data).hexdigest()
    if digest != UCD_SHA256:
        raise RuntimeError(
            f"downloaded UCD hash {digest} != expected {UCD_SHA256}"
        )
    SRC.write_bytes(data)

XSD_CATEGORIES = [
    "Lu", "Ll", "Lt", "Lm", "Lo",
    "Mn", "Mc", "Me",
    "Nd", "Nl", "No",
    "Pc", "Pd", "Ps", "Pe", "Pi", "Pf", "Po",
    "Zs", "Zl", "Zp",
    "Sm", "Sc", "Sk", "So",
    "Cc", "Cf", "Co",
]


def load_assignments():
    """Return list of (codepoint, category) for every assigned non-surrogate cp."""
    out = []
    with SRC.open() as f:
        lines = f.readlines()
    i = 0
    while i < len(lines):
        fields = lines[i].rstrip("\n").split(";")
        cp = int(fields[0], 16)
        name = fields[1]
        cat = fields[2]
        if name.endswith(", First>"):
            fields2 = lines[i + 1].rstrip("\n").split(";")
            cp2 = int(fields2[0], 16)
            for c in range(cp, cp2 + 1):
                if cat != "Cs":
                    out.append((c, cat))
            i += 2
            continue
        if cat != "Cs":
            out.append((cp, cat))
        i += 1
    return out


def coalesce(points):
    ranges = []
    it = iter(sorted(points))
    try:
        start = prev = next(it)
    except StopIteration:
        return ranges
    for cp in it:
        if cp == prev + 1:
            prev = cp
        else:
            ranges.append((start, prev))
            start = prev = cp
    ranges.append((start, prev))
    return ranges


def build_tables():
    by_cat = {c: [] for c in XSD_CATEGORIES}
    for cp, cat in load_assignments():
        if cat in by_cat:
            by_cat[cat].append(cp)
    return {c: coalesce(pts) for c, pts in by_cat.items()}


def emit(tables):
    lines = []
    lines.append("//! XSD 1.0 `\\p{X}` category escape table, pinned to Unicode 3.0 (BMP-only).")
    lines.append("//!")
    lines.append("//! XSD 1.0 Part 2 §F.1.1 normatively cites Unicode 3.1, but the W3C XSD")
    lines.append("//! test suite (MS reJ* tests, Bugzilla 4113 `status=\"queried\"`) was")
    lines.append("//! authored against a pre-3.1 implementation and encodes Unicode 3.0 /")
    lines.append("//! BMP-only category membership. We pin to 3.0 here to match the canonical")
    lines.append("//! test suite and industry peers (Saxon, Xerces). Consequence: SMP")
    lines.append("//! codepoints (U+1D7A8, U+1D1AD, etc.) added in Unicode 3.1+ are NOT")
    lines.append("//! recognized as members of their Unicode categories under XSD 1.0")
    lines.append("//! pattern facets. XSD 1.1 is unaffected — it uses current regexml /")
    lines.append("//! ICU4X tables per §G.4.2.2's \"or in some later version\" relaxation.")
    lines.append("//!")
    lines.append("//! If W3C Bugzilla 4113 ever resolves in favor of spec text (3.1),")
    lines.append("//! swap the embedded tables to 3.1 and delete this note. See T6 in")
    lines.append("//! TEST_ROADMAP_AND_ANALYZE.md.")
    lines.append("//!")
    lines.append(f"//! Source: {UCD_URL}")
    lines.append(f"//! SHA-256: {UCD_SHA256}")
    lines.append("//! Generated by `scripts/gen_xsd_unicode.py`. DO NOT EDIT BY HAND.")
    lines.append("")
    lines.append("use std::collections::HashMap;")
    lines.append("use std::sync::OnceLock;")
    lines.append("")
    lines.append("type Ranges = &'static [(u32, u32)];")
    lines.append("")
    for cat in XSD_CATEGORIES:
        rs = tables[cat]
        lines.append(f"/// Unicode 3.0.0 {cat} ({len(rs)} ranges).")
        lines.append(f"pub(crate) const {cat.upper()}_XSD10: Ranges = &[")
        for (s, e) in rs:
            lines.append(f"    (0x{s:04X}, 0x{e:04X}),")
        lines.append("];")
        lines.append("")
    groups = {
        "L": ["Lu", "Ll", "Lt", "Lm", "Lo"],
        "M": ["Mn", "Mc", "Me"],
        "N": ["Nd", "Nl", "No"],
        "P": ["Pc", "Pd", "Ps", "Pe", "Pi", "Pf", "Po"],
        "Z": ["Zs", "Zl", "Zp"],
        "S": ["Sm", "Sc", "Sk", "So"],
        "C": ["Cc", "Cf", "Co"],
    }
    for g, parts in groups.items():
        pts = set()
        for p in parts:
            for (s, e) in tables[p]:
                for cp in range(s, e + 1):
                    pts.add(cp)
        rs = coalesce(pts)
        lines.append(f"/// Unicode 3.0.0 {g} = {'∪'.join(parts)} ({len(rs)} ranges).")
        lines.append(f"pub(crate) const {g}_XSD10: Ranges = &[")
        for (s, e) in rs:
            lines.append(f"    (0x{s:04X}, 0x{e:04X}),")
        lines.append("];")
        lines.append("")

    lines.append("/// Look up the XSD-1.0 Unicode-3.0 range table for a general-category code.")
    lines.append("/// Returns `None` for block escapes (`Is...`), unknown names, or Cn/Cs.")
    lines.append("pub(crate) fn category_ranges(name: &str) -> Option<Ranges> {")
    lines.append("    Some(match name {")
    for cat in XSD_CATEGORIES:
        lines.append(f"        \"{cat}\" => {cat.upper()}_XSD10,")
    for g in groups.keys():
        lines.append(f"        \"{g}\" => {g}_XSD10,")
    lines.append("        _ => return None,")
    lines.append("    })")
    lines.append("}")
    lines.append("")
    lines.append("/// Emit the XSD-1.0 category `name` as a regex char-class *body* (what goes")
    lines.append("/// INSIDE `[...]` or `[^...]`). Each codepoint is emitted as its literal")
    lines.append("/// Unicode character; the five XSD char-class metacharacters")
    lines.append("/// (`[`, `\\\\`, `]`, `-`, `^`) are escaped with a leading backslash so both")
    lines.append("/// the Rust `regex` crate and `regexml` parse the output unambiguously.")
    lines.append("/// Returns `None` for block names, typos, or intentionally unsupported")
    lines.append("/// categories.")
    lines.append("pub(crate) fn expand_xsd_category_body(name: &str) -> Option<&'static str> {")
    lines.append("    static CACHE: OnceLock<HashMap<&'static str, String>> = OnceLock::new();")
    lines.append("    let cache = CACHE.get_or_init(build_cache);")
    lines.append("    cache.get(name).map(String::as_str)")
    lines.append("}")
    lines.append("")
    lines.append("fn build_cache() -> HashMap<&'static str, String> {")
    lines.append("    let names: &[&str] = &[")
    for cat in XSD_CATEGORIES:
        lines.append(f"        \"{cat}\",")
    for g in groups.keys():
        lines.append(f"        \"{g}\",")
    lines.append("    ];")
    lines.append("    let mut map = HashMap::with_capacity(names.len());")
    lines.append("    for name in names {")
    lines.append("        if let Some(ranges) = category_ranges(name) {")
    lines.append("            let mut out = String::with_capacity(ranges.len() * 8);")
    lines.append("            for &(s, e) in ranges {")
    lines.append("                emit_range(&mut out, s, e);")
    lines.append("            }")
    lines.append("            map.insert(*name, out);")
    lines.append("        }")
    lines.append("    }")
    lines.append("    map")
    lines.append("}")
    lines.append("")
    lines.append("fn push_char_class_char(out: &mut String, cp: u32) {")
    lines.append("    // XSD char-class metacharacters (§G.4.1.3): `[` `\\\\` `]` `-` `^`.")
    lines.append("    // `[` is escaped too so the regex parser does not treat a literal")
    lines.append("    // `[` (e.g. from `\\p{Ps}`) as opening a nested class.")
    lines.append("    if cp == 0x5B || cp == 0x5C || cp == 0x5D || cp == 0x2D || cp == 0x5E {")
    lines.append("        out.push('\\\\');")
    lines.append("    }")
    lines.append("    if let Some(c) = char::from_u32(cp) {")
    lines.append("        out.push(c);")
    lines.append("    }")
    lines.append("}")
    lines.append("")
    lines.append("fn emit_range(out: &mut String, s: u32, e: u32) {")
    lines.append("    if s == e {")
    lines.append("        push_char_class_char(out, s);")
    lines.append("        return;")
    lines.append("    }")
    lines.append("    // If the range endpoint is `-` (U+002D), split so the dash is a")
    lines.append("    // standalone escaped literal rather than a range operator.")
    lines.append("    if s == 0x2D {")
    lines.append("        push_char_class_char(out, 0x2D);")
    lines.append("        if e > s + 1 {")
    lines.append("            push_char_class_char(out, s + 1);")
    lines.append("            out.push('-');")
    lines.append("            push_char_class_char(out, e);")
    lines.append("        } else {")
    lines.append("            push_char_class_char(out, e);")
    lines.append("        }")
    lines.append("        return;")
    lines.append("    }")
    lines.append("    if e == 0x2D {")
    lines.append("        if e > s + 1 {")
    lines.append("            push_char_class_char(out, s);")
    lines.append("            out.push('-');")
    lines.append("            push_char_class_char(out, e - 1);")
    lines.append("        } else {")
    lines.append("            push_char_class_char(out, s);")
    lines.append("        }")
    lines.append("        push_char_class_char(out, 0x2D);")
    lines.append("        return;")
    lines.append("    }")
    lines.append("    push_char_class_char(out, s);")
    lines.append("    out.push('-');")
    lines.append("    push_char_class_char(out, e);")
    lines.append("}")
    lines.append("")
    lines.append("#[cfg(test)]")
    lines.append("mod tests {")
    lines.append("    use super::*;")
    lines.append("")
    lines.append("    fn in_ranges(rs: Ranges, cp: u32) -> bool {")
    lines.append("        rs.iter().any(|&(s, e)| s <= cp && cp <= e)")
    lines.append("    }")
    lines.append("")
    lines.append("    #[test]")
    lines.append("    fn lu_contains_ascii_uppercase() {")
    lines.append("        assert!(in_ranges(LU_XSD10, 0x41));")
    lines.append("        assert!(in_ranges(LU_XSD10, 0x5A));")
    lines.append("        assert!(!in_ranges(LU_XSD10, 0x61));")
    lines.append("    }")
    lines.append("")
    lines.append("    #[test]")
    lines.append("    fn lu_excludes_smp_math_alphanumerics() {")
    lines.append("        // Added in Unicode 3.1; unassigned in 3.0. The reJ11.i test")
    lines.append("        // relies on this exclusion.")
    lines.append("        assert!(!in_ranges(LU_XSD10, 0x1D7A8));")
    lines.append("    }")
    lines.append("")
    lines.append("    #[test]")
    lines.append("    fn co_stops_at_bmp_pua_end() {")
    lines.append("        // Plane-15/16 PUA appeared in 3.1; 3.0 only has U+E000..U+F8FF.")
    lines.append("        assert!(in_ranges(CO_XSD10, 0xE000));")
    lines.append("        assert!(in_ranges(CO_XSD10, 0xF8FF));")
    lines.append("        assert!(!in_ranges(CO_XSD10, 0x100000));")
    lines.append("        assert!(!in_ranges(CO_XSD10, 0x10FFFD));")
    lines.append("    }")
    lines.append("")
    lines.append("    #[test]")
    lines.append("    fn n_is_nd_nl_no_union() {")
    lines.append("        for &(s, e) in ND_XSD10.iter().chain(NL_XSD10).chain(NO_XSD10) {")
    lines.append("            for cp in s..=e {")
    lines.append("                assert!(in_ranges(N_XSD10, cp), \"cp U+{:04X} missing from N\", cp);")
    lines.append("            }")
    lines.append("        }")
    lines.append("    }")
    lines.append("")
    lines.append("    #[test]")
    lines.append("    fn expand_body_is_nonempty_for_known_cats() {")
    lines.append("        for name in [\"Lu\", \"Ll\", \"Lo\", \"Nd\", \"N\", \"L\", \"Cf\", \"Co\", \"M\", \"P\", \"S\"] {")
    lines.append("            let body = expand_xsd_category_body(name).expect(name);")
    lines.append("            assert!(!body.is_empty(), \"empty body for {}\", name);")
    lines.append("        }")
    lines.append("    }")
    lines.append("")
    lines.append("    #[test]")
    lines.append("    fn expand_body_returns_none_for_unknown() {")
    lines.append("        assert!(expand_xsd_category_body(\"IsBasicLatin\").is_none());")
    lines.append("        assert!(expand_xsd_category_body(\"Cn\").is_none());")
    lines.append("        assert!(expand_xsd_category_body(\"Cs\").is_none());")
    lines.append("        assert!(expand_xsd_category_body(\"Xx\").is_none());")
    lines.append("    }")
    lines.append("}")
    lines.append("")
    return "\n".join(lines)


def main():
    ensure_ucd()
    tables = build_tables()
    txt = emit(tables)
    OUT.write_text(txt)
    total_ranges = sum(len(v) for v in tables.values())
    print(f"wrote {OUT} ({total_ranges} ranges across {len(tables)} two-letter categories)")


if __name__ == "__main__":
    main()
