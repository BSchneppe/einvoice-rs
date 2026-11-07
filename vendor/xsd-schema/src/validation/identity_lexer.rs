//! Lexer for XSD identity-constraint XPath subset (selector/field expressions).
//!
//! This subset is much simpler than full XPath 2.0: no predicates, no function calls,
//! no parent axis, no operators. All names (including `child`, `attribute`, `and`, `or`)
//! are emitted as plain `NCName` tokens, avoiding reserved-word conflicts with the
//! main XPath 2.0 lexer.

use std::fmt;

/// Token produced by the identity-constraint XPath lexer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdXPathToken<'a> {
    /// An XML NCName (unqualified name).
    NCName(&'a str),
    /// `*`
    Star,
    /// `:`
    Colon,
    /// `/`
    Slash,
    /// `//`
    DoubleSlash,
    /// `::`
    DoubleColon,
    /// `|`
    Pipe,
    /// `.`
    Dot,
    /// `@`
    At,
}

/// A spanned token: `(start, token, end)` where positions are byte offsets.
pub type IdXPathSpanned<'a> = (usize, IdXPathToken<'a>, usize);

/// Error produced during identity-constraint XPath lexing.
#[derive(Debug, Clone)]
pub struct IdXPathLexError {
    /// Human-readable error message.
    pub message: String,
    /// Byte offset where the error occurred.
    pub position: usize,
}

impl fmt::Display for IdXPathLexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "identity XPath lex error at position {}: {}",
            self.position, self.message
        )
    }
}

impl std::error::Error for IdXPathLexError {}

/// Lexer for the restricted XPath subset used in XSD identity constraints.
pub struct IdXPathLexer<'a> {
    input: &'a str,
    pos: usize,
}

/// Check if a character is an XML NCName start character.
fn is_ncname_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

/// Check if a character is an XML NCName character.
fn is_ncname_char(c: char) -> bool {
    c.is_alphanumeric()
        || c == '_'
        || c == '-'
        || c == '.'
        || c == '\u{B7}'
        || ('\u{0300}'..='\u{036F}').contains(&c)
        || ('\u{203F}'..='\u{2040}').contains(&c)
}

impl<'a> IdXPathLexer<'a> {
    /// Create a new lexer for the given input string.
    pub fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    /// Peek at the current character without advancing.
    fn current(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    /// Peek at the character after the current one.
    fn peek_next(&self) -> Option<char> {
        let mut chars = self.input[self.pos..].chars();
        chars.next();
        chars.next()
    }

    /// Advance past the current character and return it.
    fn advance(&mut self) -> Option<char> {
        let c = self.current()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    /// Skip whitespace characters.
    fn skip_whitespace(&mut self) {
        while let Some(c) = self.current() {
            if matches!(c, ' ' | '\t' | '\r' | '\n') {
                self.pos += c.len_utf8();
            } else {
                break;
            }
        }
    }

    /// Lex an NCName starting at the current position.
    fn lex_ncname(&mut self) -> IdXPathToken<'a> {
        let start = self.pos;
        self.advance(); // consume the start character
        while let Some(c) = self.current() {
            if is_ncname_char(c) {
                self.advance();
            } else {
                break;
            }
        }
        IdXPathToken::NCName(&self.input[start..self.pos])
    }

    /// Produce the next token or an error.
    fn next_token(&mut self) -> Option<Result<IdXPathSpanned<'a>, IdXPathLexError>> {
        self.skip_whitespace();
        let start = self.pos;
        let c = self.current()?;

        // NCName
        if is_ncname_start(c) {
            let tok = self.lex_ncname();
            return Some(Ok((start, tok, self.pos)));
        }

        match c {
            '/' => {
                self.advance();
                if self.current() == Some('/') {
                    self.advance();
                    Some(Ok((start, IdXPathToken::DoubleSlash, self.pos)))
                } else {
                    Some(Ok((start, IdXPathToken::Slash, self.pos)))
                }
            }
            ':' => {
                self.advance();
                if self.current() == Some(':') {
                    self.advance();
                    Some(Ok((start, IdXPathToken::DoubleColon, self.pos)))
                } else {
                    Some(Ok((start, IdXPathToken::Colon, self.pos)))
                }
            }
            '.' => {
                if self.peek_next() == Some('.') {
                    Some(Err(IdXPathLexError {
                        message: "parent axis `..` is not allowed in identity-constraint XPath"
                            .into(),
                        position: start,
                    }))
                } else {
                    self.advance();
                    Some(Ok((start, IdXPathToken::Dot, self.pos)))
                }
            }
            '*' => {
                self.advance();
                Some(Ok((start, IdXPathToken::Star, self.pos)))
            }
            '|' => {
                self.advance();
                Some(Ok((start, IdXPathToken::Pipe, self.pos)))
            }
            '@' => {
                self.advance();
                Some(Ok((start, IdXPathToken::At, self.pos)))
            }
            '[' => Some(Err(IdXPathLexError {
                message: "predicates `[...]` are not allowed in identity-constraint XPath".into(),
                position: start,
            })),
            '(' => Some(Err(IdXPathLexError {
                message: "function calls are not allowed in identity-constraint XPath".into(),
                position: start,
            })),
            _ => Some(Err(IdXPathLexError {
                message: format!("unexpected character `{c}`"),
                position: start,
            })),
        }
    }
}

impl<'a> Iterator for IdXPathLexer<'a> {
    type Item = Result<IdXPathSpanned<'a>, IdXPathLexError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_token()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Collect all tokens from an input, asserting no errors.
    fn lex_ok(input: &str) -> Vec<IdXPathSpanned<'_>> {
        IdXPathLexer::new(input)
            .collect::<Result<Vec<_>, _>>()
            .unwrap_or_else(|e| panic!("unexpected lex error: {e}"))
    }

    /// Expect a lex error from the input.
    fn lex_err(input: &str) -> IdXPathLexError {
        IdXPathLexer::new(input)
            .collect::<Result<Vec<_>, _>>()
            .expect_err("expected a lex error")
    }

    // --- Individual token types ---

    #[test]
    fn ncname_simple() {
        let tokens = lex_ok("foo");
        assert_eq!(tokens, vec![(0, IdXPathToken::NCName("foo"), 3)]);
    }

    #[test]
    fn star() {
        let tokens = lex_ok("*");
        assert_eq!(tokens, vec![(0, IdXPathToken::Star, 1)]);
    }

    #[test]
    fn colon() {
        // A bare colon (namespace separator context)
        let tokens = lex_ok("ns:local");
        assert_eq!(
            tokens,
            vec![
                (0, IdXPathToken::NCName("ns"), 2),
                (2, IdXPathToken::Colon, 3),
                (3, IdXPathToken::NCName("local"), 8),
            ]
        );
    }

    #[test]
    fn slash() {
        let tokens = lex_ok("/");
        assert_eq!(tokens, vec![(0, IdXPathToken::Slash, 1)]);
    }

    #[test]
    fn double_slash() {
        let tokens = lex_ok("//");
        assert_eq!(tokens, vec![(0, IdXPathToken::DoubleSlash, 2)]);
    }

    #[test]
    fn double_colon() {
        let tokens = lex_ok("::");
        assert_eq!(tokens, vec![(0, IdXPathToken::DoubleColon, 2)]);
    }

    #[test]
    fn pipe() {
        let tokens = lex_ok("|");
        assert_eq!(tokens, vec![(0, IdXPathToken::Pipe, 1)]);
    }

    #[test]
    fn dot() {
        let tokens = lex_ok(".");
        assert_eq!(tokens, vec![(0, IdXPathToken::Dot, 1)]);
    }

    #[test]
    fn at() {
        let tokens = lex_ok("@");
        assert_eq!(tokens, vec![(0, IdXPathToken::At, 1)]);
    }

    // --- Full expressions ---

    #[test]
    fn descendant_path() {
        // .//foo/bar
        let tokens = lex_ok(".//foo/bar");
        assert_eq!(
            tokens,
            vec![
                (0, IdXPathToken::Dot, 1),
                (1, IdXPathToken::DoubleSlash, 3),
                (3, IdXPathToken::NCName("foo"), 6),
                (6, IdXPathToken::Slash, 7),
                (7, IdXPathToken::NCName("bar"), 10),
            ]
        );
    }

    #[test]
    fn child_axis() {
        // child::foo
        let tokens = lex_ok("child::foo");
        assert_eq!(
            tokens,
            vec![
                (0, IdXPathToken::NCName("child"), 5),
                (5, IdXPathToken::DoubleColon, 7),
                (7, IdXPathToken::NCName("foo"), 10),
            ]
        );
    }

    #[test]
    fn namespace_wildcard() {
        // ns:*
        let tokens = lex_ok("ns:*");
        assert_eq!(
            tokens,
            vec![
                (0, IdXPathToken::NCName("ns"), 2),
                (2, IdXPathToken::Colon, 3),
                (3, IdXPathToken::Star, 4),
            ]
        );
    }

    #[test]
    fn attribute_path() {
        // .//foo/@bar
        let tokens = lex_ok(".//foo/@bar");
        assert_eq!(
            tokens,
            vec![
                (0, IdXPathToken::Dot, 1),
                (1, IdXPathToken::DoubleSlash, 3),
                (3, IdXPathToken::NCName("foo"), 6),
                (6, IdXPathToken::Slash, 7),
                (7, IdXPathToken::At, 8),
                (8, IdXPathToken::NCName("bar"), 11),
            ]
        );
    }

    // --- XPath keywords emitted as NCName ---

    #[test]
    fn keywords_as_ncname() {
        for kw in &["and", "or", "div", "mod", "child", "attribute"] {
            let tokens = lex_ok(kw);
            assert_eq!(tokens, vec![(0, IdXPathToken::NCName(kw), kw.len())]);
        }
    }

    // --- Error cases ---

    #[test]
    fn error_parent_axis() {
        let err = lex_err("..");
        assert!(
            err.message.contains("parent axis"),
            "message: {}",
            err.message
        );
        assert_eq!(err.position, 0);
    }

    #[test]
    fn error_predicate() {
        let err = lex_err("foo[1]");
        assert!(
            err.message.contains("predicates"),
            "message: {}",
            err.message
        );
        assert_eq!(err.position, 3);
    }

    #[test]
    fn error_function_call() {
        let err = lex_err("fn(");
        assert!(
            err.message.contains("function calls"),
            "message: {}",
            err.message
        );
        assert_eq!(err.position, 2);
    }

    // --- Edge cases ---

    #[test]
    fn empty_input() {
        let tokens = lex_ok("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn whitespace_only() {
        let tokens = lex_ok("   \t\n  ");
        assert!(tokens.is_empty());
    }

    #[test]
    fn span_correctness_with_whitespace() {
        // "  foo / bar  "
        let tokens = lex_ok("  foo / bar  ");
        assert_eq!(
            tokens,
            vec![
                (2, IdXPathToken::NCName("foo"), 5),
                (6, IdXPathToken::Slash, 7),
                (8, IdXPathToken::NCName("bar"), 11),
            ]
        );
    }
}
