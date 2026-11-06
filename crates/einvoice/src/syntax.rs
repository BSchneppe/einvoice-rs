use std::borrow::Cow;
use std::collections::HashSet;

use crate::{ParseError, SourceLocation};

// Called only AFTER bounded, DTD-free xml-rs parsing. Its StartElement event
// omits xmlns attributes, including duplicates. Check their lexical spelling
// before granting Document's syntax guarantee. This tokenizer does not expand
// entities or build a second DOM. The decoded view is never serialized/stored.
pub(crate) fn check_namespace_declarations(bytes: &[u8], encoding: &str) -> Result<(), ParseError> {
    let text = decoded_view(bytes, encoding)?;
    // xml-rs already checked the declaration (including legal whitespace such as
    // <?xml\n...). Skip it for fragment tokenization while retaining full-text
    // offsets. xmlparser's declaration tokenizer only recognizes <?xml SPACE.
    let mut start = if text.starts_with('\u{feff}') { 3 } else { 0 };
    if text[start..].starts_with("<?xml")
        && text
            .as_bytes()
            .get(start + 5)
            .is_some_and(|b| b" \t\r\n".contains(b))
    {
        if let Some(end) = text[start..].find("?>") {
            start += end + 2;
        }
    }
    let mut prefixes = HashSet::new();
    for token in xmlparser::Tokenizer::from_fragment(&text, start..text.len()) {
        let token = token.map_err(|error| ParseError::XmlSyntax {
            message: error.to_string(),
            location: position(&text, error.pos()),
        })?;
        match token {
            xmlparser::Token::ElementStart { .. } => prefixes.clear(),
            xmlparser::Token::Attribute {
                prefix,
                local,
                span,
                ..
            } => {
                let declared = if prefix.as_str() == "xmlns" {
                    Some(local.as_str())
                } else if prefix.as_str().is_empty() && local.as_str() == "xmlns" {
                    Some("")
                } else {
                    None
                };
                if let Some(declared) = declared {
                    if !prefixes.insert(declared) {
                        return Err(ParseError::XmlSyntax {
                            message: "duplicate namespace declaration".into(),
                            location: position(
                                &text,
                                xmlparser::Stream::from(text.as_ref())
                                    .gen_text_pos_from(span.start()),
                            ),
                        });
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

// Decoding follows the already-validated byte encoding. No lossy replacement.
fn decoded_view<'a>(bytes: &'a [u8], encoding: &str) -> Result<Cow<'a, str>, ParseError> {
    let decode_error = |message: String| ParseError::XmlSyntax {
        message,
        location: SourceLocation { line: 1, column: 1 },
    };
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        if !bytes.chunks_exact(2).remainder().is_empty() {
            return Err(decode_error("incomplete UTF-16 code unit".into()));
        }
        let little_endian = bytes[0] == 0xff;
        let units = bytes.chunks_exact(2).map(|unit| {
            if little_endian {
                u16::from_le_bytes([unit[0], unit[1]])
            } else {
                u16::from_be_bytes([unit[0], unit[1]])
            }
        });
        let decoded: Result<String, _> = char::decode_utf16(units).collect();
        return decoded
            .map(Cow::Owned)
            .map_err(|e| decode_error(e.to_string()));
    }
    match encoding.parse::<xml::Encoding>() {
        Ok(xml::Encoding::Latin1) => Ok(Cow::Owned(bytes.iter().map(|b| char::from(*b)).collect())),
        _ => std::str::from_utf8(bytes)
            .map(Cow::Borrowed)
            .map_err(|e| decode_error(e.to_string())),
    }
}

// Convert the tokenizer's LF-only, BOM-counting coordinates into XML character
// coordinates. Do not normalize the source buffer just to obtain diagnostics.
fn position(text: &str, pos: xmlparser::TextPos) -> SourceLocation {
    let (mut raw_line, mut raw_column) = (1, 1);
    let mut result = SourceLocation { line: 1, column: 1 };
    let mut previous_cr = false;
    for (offset, character) in text.char_indices() {
        if (raw_line, raw_column) == (pos.row, pos.col) {
            break;
        }
        if character == '\n' {
            raw_line += 1;
            raw_column = 1;
        } else {
            raw_column += 1;
        }
        if offset == 0 && character == '\u{feff}' {
            continue;
        }
        match character {
            '\r' => {
                result.line += 1;
                result.column = 1;
            }
            '\n' => {
                if !previous_cr {
                    result.line += 1;
                }
                result.column = 1;
            }
            _ => result.column += 1,
        }
        previous_cr = character == '\r';
    }
    result
}
