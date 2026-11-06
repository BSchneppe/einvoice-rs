/// Find the index of `letter` in `text`, ignoring positions inside string literals.
///
/// Port of C# EscapedIndexOf (Form1.cs:595-623).
fn escaped_index_of(text: &str, letter: char) -> Option<usize> {
    let mut is_literal = false;
    let mut literal_char = '\0';
    for (i, ch) in text.char_indices() {
        if is_literal {
            if ch == literal_char {
                is_literal = false;
            }
        } else {
            match ch {
                '"' | '\'' => {
                    literal_char = ch;
                    is_literal = true;
                }
                _ => {
                    if ch == letter {
                        return Some(i);
                    }
                }
            }
        }
    }
    None
}

/// Preprocess XQuery text to extract a bare XPath expression.
///
/// Port of C# PrepareQueryText (Form1.cs:625-641):
/// 1. Remove "(: Kelvin sign :)" marker
/// 2. Skip past the last ":)" comment close
/// 3. Extract content between { ... } (respecting string escaping)
/// 4. Trim whitespace
pub fn prepare_query_text(text: &str) -> String {
    let mut text = text.to_string();

    // Remove Kelvin sign marker
    if let Some(idx) = text.find("(: Kelvin sign :)") {
        text = format!(
            "{}{}",
            &text[..idx],
            &text[idx + "(: Kelvin sign :)".len()..]
        );
    }

    // Skip past last ":)" comment close
    if let Some(idx) = text.rfind(":)") {
        text = text[idx + 2..].to_string();
    }

    // Extract content between { ... }
    if let Some(open) = escaped_index_of(&text, '{') {
        if let Some(close) = text.rfind('}') {
            if close > open {
                text = text[open + 1..close].to_string();
            }
        }
    }

    expand_xquery_string_literals(text.trim())
}

/// Expand the escapes that XQuery — but not XPath — applies to string literals.
///
/// The suite's queries are XQuery source. XQuery 1.0 §3.1.1 expands XML entity
/// references and character references inside a string literal, and §A.2.2
/// normalizes literal line endings in the query text to `#xA`. XPath 2.0
/// §3.1.1 does neither: the value of a string literal is the characters between
/// the delimiters, with a doubled delimiter standing for one. Doing the XQuery
/// expansion here, while the query is still XQuery text, keeps the XPath lexer
/// faithful to XPath.
///
/// A character produced by an expansion is re-escaped when it is the literal's
/// own delimiter (`&quot;` inside a `"…"` literal becomes `""`), so the result
/// is an XPath expression with the same literal *values*.
fn expand_xquery_string_literals(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;

    while i < chars.len() {
        let quote = chars[i];
        if quote != '"' && quote != '\'' {
            out.push(chars[i]);
            i += 1;
            continue;
        }

        // Copy the literal, collecting its raw (still escaped) body.
        let literal_start = i;
        out.push(quote);
        i += 1;
        let mut raw = String::new();
        let mut closed = false;
        loop {
            match chars.get(i) {
                None => break,
                Some(&c) if c == quote => {
                    if chars.get(i + 1) == Some(&quote) {
                        // A doubled delimiter stands for one character; the
                        // re-emit loop below escapes it again.
                        raw.push(quote);
                        i += 2;
                    } else {
                        i += 1;
                        closed = true;
                        break;
                    }
                }
                Some(&c) => {
                    raw.push(c);
                    i += 1;
                }
            }
        }

        if !closed {
            // An unterminated literal must stay unterminated so the lexer still
            // reports it; copy the rest of the text through verbatim.
            out.truncate(out.len() - quote.len_utf8());
            out.extend(chars[literal_start..].iter());
            break;
        }

        // `normalize_string_value` performs exactly the two XQuery steps:
        // entity/character-reference expansion and CR/CRLF folding.
        let expanded = xsd_schema::xpath::string_ops::normalize_string_value(&raw, false, false)
            .unwrap_or(raw);
        for c in expanded.chars() {
            if c == quote {
                out.push(quote);
            }
            out.push(c);
        }
        out.push(quote);
    }

    out
}

// Self-test for prepare module (run via driver's --self-test or manually)
#[allow(dead_code)]
pub fn self_test() {
    assert_eq!(escaped_index_of("hello{world", '{'), Some(5));
    assert_eq!(escaped_index_of("'hello{world'}", '}'), Some(13));
    assert_eq!(escaped_index_of("hello world", '{'), None);

    assert_eq!(prepare_query_text("(: comment :)\n{ 1 + 2 }"), "1 + 2");
    assert_eq!(
        prepare_query_text("(: Kelvin sign :)(: test :)\n{ 42 }"),
        "42"
    );
    assert_eq!(prepare_query_text("(: comment :)\n1 + 2"), "1 + 2");
}
