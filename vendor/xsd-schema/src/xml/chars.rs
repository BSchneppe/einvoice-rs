//! Character classes of XML 1.0 (Fifth Edition) used by the reader.
//!
//! ```text
//! [2]  Char          ::= #x9 | #xA | #xD | [#x20-#xD7FF] | [#xE000-#xFFFD] | [#x10000-#x10FFFF]
//! [3]  S             ::= (#x20 | #x9 | #xD | #xA)+
//! [4]  NameStartChar ::= ":" | [A-Z] | "_" | [a-z] | [#xC0-#xD6] | [#xD8-#xF6] | [#xF8-#x2FF]
//!                      | [#x370-#x37D] | [#x37F-#x1FFF] | [#x200C-#x200D] | [#x2070-#x218F]
//!                      | [#x2C00-#x2FEF] | [#x3001-#xD7FF] | [#xF900-#xFDCF] | [#xFDF0-#xFFFD]
//!                      | [#x10000-#xEFFFF]
//! [4a] NameChar      ::= NameStartChar | "-" | "." | [0-9] | #xB7 | [#x0300-#x036F] | [#x203F-#x2040]
//! [5]  Name          ::= NameStartChar (NameChar)*
//! ```

/// \[2\] `Char`, for a code point.
#[inline]
pub(crate) fn is_char(c: u32) -> bool {
    matches!(c, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
}

/// XML 1.1 \[2\] `Char ::= [#x1-#xD7FF] | [#xE000-#xFFFD] | [#x10000-#x10FFFF]`
/// — what a character reference may name in an XML 1.1 document.
#[inline]
pub(crate) fn is_char11(c: u32) -> bool {
    matches!(c, 0x1..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
}

/// The length of an XML 1.1 line end other than CR / LF at `b[j]`: NEL
/// (U+0085, `C2 85`) or LINE SEPARATOR (U+2028, `E2 80 A8`) — XML 1.1 §2.11.
#[inline]
pub(crate) fn xml11_line_end_at(b: &[u8], j: usize) -> Option<usize> {
    match b.get(j..) {
        Some([0xC2, 0x85, ..]) => Some(2),
        Some([0xE2, 0x80, 0xA8, ..]) => Some(3),
        _ => None,
    }
}

/// \[3\] `S`, for a byte.
#[inline]
pub(crate) fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

/// \[4\] `NameStartChar`, for a code point.
#[inline]
pub(crate) fn is_name_start(c: u32) -> bool {
    matches!(c,
        0x3A | 0x41..=0x5A | 0x5F | 0x61..=0x7A | 0xC0..=0xD6 | 0xD8..=0xF6
        | 0xF8..=0x2FF | 0x370..=0x37D | 0x37F..=0x1FFF | 0x200C..=0x200D
        | 0x2070..=0x218F | 0x2C00..=0x2FEF | 0x3001..=0xD7FF | 0xF900..=0xFDCF
        | 0xFDF0..=0xFFFD | 0x10000..=0xEFFFF)
}

/// \[4a\] `NameChar`, for a code point.
#[inline]
pub(crate) fn is_name_char(c: u32) -> bool {
    is_name_start(c)
        || matches!(c, 0x2D | 0x2E | 0x30..=0x39 | 0xB7 | 0x300..=0x36F | 0x203F..=0x2040)
}

/// Per ASCII byte: bit 0 = `NameStartChar`, bit 1 = `NameChar`.
const ASCII_NAME: [u8; 128] = {
    let mut t = [0u8; 128];
    let mut i = 0;
    while i < 128 {
        let c = i as u32;
        let start = c == 0x3A || (c >= 0x41 && c <= 0x5A) || c == 0x5F || (c >= 0x61 && c <= 0x7A);
        let name = start || c == 0x2D || c == 0x2E || (c >= 0x30 && c <= 0x39);
        t[i] = (start as u8) | ((name as u8) << 1);
        i += 1;
    }
    t
};

/// The code point of the UTF-8 sequence at `b[i]` (`b[i] >= 0x80`) and its
/// length, or `None` when the bytes there are not well-formed UTF-8 — the
/// readers report those as encoding errors when they decode the bytes.
#[inline]
pub(crate) fn decode_at(b: &[u8], i: usize) -> Option<(u32, usize)> {
    let c0 = b[i];
    let (len, init) = match c0 {
        0xC2..=0xDF => (2, u32::from(c0 & 0x1F)),
        0xE0..=0xEF => (3, u32::from(c0 & 0x0F)),
        0xF0..=0xF4 => (4, u32::from(c0 & 0x07)),
        _ => return None,
    };
    if i + len > b.len() {
        return None;
    }
    let mut v = init;
    for &cb in &b[i + 1..i + len] {
        if cb & 0xC0 != 0x80 {
            return None;
        }
        v = (v << 6) | u32::from(cb & 0x3F);
    }
    let min = match len {
        2 => 0x80,
        3 => 0x800,
        _ => 0x10000,
    };
    if v < min || (0xD800..=0xDFFF).contains(&v) || v > 0x10FFFF {
        return None;
    }
    Some((v, len))
}

/// What [`scan_name`] found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NameScan {
    /// One past the last byte of the name.
    pub end: usize,
    /// The number of colons in it.
    pub colons: usize,
    /// The offset of the first colon, if any.
    pub first_colon: Option<usize>,
}

/// Scans the \[5\] `Name` starting at `b[i]`: the name runs to the first
/// character that is not a `NameChar` (bytes that are not well-formed UTF-8
/// end it too — the caller reports them). Returns `None` when no
/// `NameStartChar` is at `i`.
#[inline]
pub(crate) fn scan_name(b: &[u8], i: usize) -> Option<NameScan> {
    let len = b.len();
    let first = *b.get(i)?;
    let mut k = i;
    if first < 0x80 {
        if ASCII_NAME[first as usize] & 1 == 0 {
            return None;
        }
        k += 1;
    } else {
        match decode_at(b, k) {
            Some((cp, n)) if is_name_start(cp) => k += n,
            _ => return None,
        }
    }
    loop {
        // ASCII name characters, the common case, in a tight loop.
        while k < len {
            let c = b[k];
            if c >= 0x80 || ASCII_NAME[c as usize] & 2 == 0 {
                break;
            }
            k += 1;
        }
        if k < len && b[k] >= 0x80 {
            if let Some((cp, n)) = decode_at(b, k) {
                if is_name_char(cp) {
                    k += n;
                    continue;
                }
            }
        }
        break;
    }
    let name = &b[i..k];
    let (colons, first_colon) = match name.iter().position(|&c| c == b':') {
        None => (0, None),
        Some(p) => (
            1 + name[p + 1..].iter().filter(|&&c| c == b':').count(),
            Some(i + p),
        ),
    };
    Some(NameScan {
        end: k,
        colons,
        first_colon,
    })
}

/// Whether `b` is exactly one \[5\] `Name`.
pub(crate) fn is_name(b: &[u8]) -> bool {
    matches!(scan_name(b, 0), Some(s) if s.end == b.len())
}

/// The offset of the first character in `b` (well-formed UTF-8, or bytes the
/// reader will refuse as such) that is not a \[2\] `Char`: a C0 control other
/// than tab, line feed and carriage return, or U+FFFE / U+FFFF (surrogates
/// cannot occur in well-formed UTF-8).
///
/// One pass: blocks of 32 bytes are tested with a branch-free predicate the
/// compiler vectorizes; only a block that may hold one is looked at byte by
/// byte (`0xEF` also starts other characters of U+F000–U+FFFF).
#[inline]
pub(crate) fn find_non_char(b: &[u8]) -> Option<usize> {
    let mut i = 0;
    while i < b.len() {
        let end = (i + 32).min(b.len());
        let mut flag = false;
        for &c in &b[i..end] {
            flag |= ((c < 0x20) & (c != b'\t') & (c != b'\n') & (c != b'\r')) | (c == 0xEF);
        }
        if flag {
            for j in i..end {
                let c = b[j];
                if c < 0x20 {
                    if c != b'\t' && c != b'\n' && c != b'\r' {
                        return Some(j);
                    }
                } else if c == 0xEF
                    && j + 2 < b.len()
                    && b[j + 1] == 0xBF
                    && (b[j + 2] == 0xBE || b[j + 2] == 0xBF)
                {
                    return Some(j);
                }
            }
        }
        i = end;
    }
    None
}

/// The first character of `b` that XML 1.1 does not allow raw: a C0 control
/// other than tab, line feed and carriage return, U+007F–U+0084,
/// U+0086–U+009F (\[2a\] `RestrictedChar`), U+FFFE, U+FFFF.
pub(crate) fn find_non_char11(b: &[u8]) -> Option<usize> {
    let mut j = 0;
    while j < b.len() {
        match b[j] {
            b'\t' | b'\n' | b'\r' => {}
            0..=0x1F | 0x7F => return Some(j),
            0xC2 if matches!(b.get(j + 1), Some(0x80..=0x84 | 0x86..=0x9F)) => return Some(j),
            0xEF if b.get(j + 1) == Some(&0xBF) && matches!(b.get(j + 2), Some(0xBE | 0xBF)) => {
                return Some(j)
            }
            _ => {}
        }
        j += 1;
    }
    None
}

/// The offset of the first byte of `b` that does not belong to well-formed
/// UTF-8 (XML 1.0 §4.3.3), if any.
#[inline]
pub(crate) fn find_bad_utf8(b: &[u8]) -> Option<usize> {
    if b.is_ascii() {
        return None;
    }
    std::str::from_utf8(b).err().map(|e| e.valid_up_to())
}

/// The code point of a character reference's digits (`65`, `x41`, without
/// `&#` and `;`), any number of them, saturating above U+10FFFF; `None` when
/// they are not \[66\] `CharRef` digits (none, or a non-digit).
pub(crate) fn char_ref_value(digits: &[u8]) -> Option<u32> {
    let (hex, digits) = match digits.first() {
        Some(b'x') => (true, &digits[1..]),
        _ => (false, digits),
    };
    if digits.is_empty() {
        return None;
    }
    let mut v: u32 = 0;
    for &d in digits {
        let n = match d {
            b'0'..=b'9' => u32::from(d - b'0'),
            b'a'..=b'f' if hex => u32::from(d - b'a' + 10),
            b'A'..=b'F' if hex => u32::from(d - b'A' + 10),
            _ => return None,
        };
        v = (v * if hex { 16 } else { 10 } + n).min(0x11_0000);
    }
    Some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn char_class() {
        for c in [
            0x9, 0xA, 0xD, 0x20, 0xD7FF, 0xE000, 0xFFFD, 0x10000, 0x10FFFF,
        ] {
            assert!(is_char(c), "{c:#X}");
        }
        for c in [
            0, 1, 8, 0xB, 0xC, 0xE, 0x1F, 0xD800, 0xDFFF, 0xFFFE, 0xFFFF, 0x110000,
        ] {
            assert!(!is_char(c), "{c:#X}");
        }
    }

    #[test]
    fn names() {
        assert!(is_name(b"a"));
        assert!(is_name(b":a"));
        assert!(is_name("é·ü.r-1_x".as_bytes()));
        assert!(is_name("a\u{300}\u{203F}".as_bytes()));
        assert!(is_name("\u{10000}".as_bytes()));
        assert!(!is_name(b""));
        assert!(!is_name(b"1a"));
        assert!(!is_name(b"-a"));
        assert!(!is_name("\u{B7}a".as_bytes()));
        assert!(!is_name(b"a/b"));
        assert!(!is_name(b"a\"b"));
        assert!(!is_name("a\u{F0000}".as_bytes()));
        let s = scan_name(b"a:b:c d", 0).unwrap();
        assert_eq!(
            s,
            NameScan {
                end: 5,
                colons: 2,
                first_colon: Some(1)
            }
        );
    }

    #[test]
    fn non_chars() {
        assert_eq!(find_non_char(b"abc\t\n\r"), None);
        assert_eq!(
            find_non_char("\u{FFFD}\u{FEFF}\u{F000}\u{10FFFF}".as_bytes()),
            None
        );
        assert_eq!(find_non_char(b"ab\x01"), Some(2));
        assert_eq!(find_non_char("ab\u{FFFE}".as_bytes()), Some(2));
        assert_eq!(find_non_char("ab\u{FFFF}".as_bytes()), Some(2));
        let mut long = "x".repeat(100);
        long.push('\u{B}');
        assert_eq!(find_non_char(long.as_bytes()), Some(100));
        let long = format!("{}\n{}\u{0}", " ".repeat(40), "y".repeat(40));
        assert_eq!(find_non_char(long.as_bytes()), Some(81));
    }

    #[test]
    fn char_refs() {
        assert_eq!(char_ref_value(b"65"), Some(65));
        assert_eq!(char_ref_value(b"x41"), Some(65));
        assert_eq!(char_ref_value(b"000000000001"), Some(1));
        assert_eq!(char_ref_value(b"x0000000000001"), Some(1));
        assert_eq!(char_ref_value(b"99999999999999999999"), Some(0x11_0000));
        assert_eq!(char_ref_value(b""), None);
        assert_eq!(char_ref_value(b"x"), None);
        assert_eq!(char_ref_value(b"X41"), None);
        assert_eq!(char_ref_value(b"12a"), None);
        assert_eq!(char_ref_value(b"xFFFE"), Some(0xFFFE));
    }
}
