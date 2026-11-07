//! XPath 2.0 Lexer with stateful lookahead.
//!
//! This lexer implements the tokenization strategy from the original C# XPath2
//! implementation (Tokenizer.cs). It uses a state machine with lookahead to
//! emit composite tokens (axis specifiers, multi-word keywords) that avoid
//! shift/reduce conflicts in the grammar.
//!
//! # Lexer States
//!
//! The lexer transitions between states based on what was just parsed:
//! - `Default`: Initial state, expecting expression start
//! - `Operator`: After an operand, expecting an operator
//! - `SingleType`: After `cast as` / `castable as`, expecting type name
//! - `ItemType`: After `instance of` / `treat as`, expecting item type
//! - `KindTest`: Inside kind test parentheses
//! - `KindTestForPi`: Inside `processing-instruction()`
//! - `CloseKindTest`: After QName in kind test
//! - `TypeNameInKindTest`: After `,` in kind test, expecting type name (may have `?` suffix)
//! - `OccurrenceIndicator`: After item type, expecting `?`/`+`/`*`
//! - `VarName`: After `$`, expecting variable name

use std::collections::VecDeque;
use std::fmt;

use super::XPathMode;

/// Token type for XPath 2.0 expressions.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Literals
    StringLiteral(String),
    IntegerLiteral(String),
    DecimalLiteral(String),
    DoubleLiteral(String),

    // Names
    NCName(String),
    QName(String),
    /// QName followed by "?" in kind test context, meaning nillable type.
    /// Used to resolve shift/reduce conflict in element(name, type?).
    QNameNillable(String),
    VarName {
        prefix: String,
        local: String,
    },

    // Keywords
    For,
    In,
    Return,
    If,
    Then,
    Else,
    Some,
    Every,
    Satisfies,
    And,
    Or,
    To,
    Div,
    IDiv,
    Mod,
    Union,
    Except,
    Intersect,

    // Type keywords (composite tokens from lookahead)
    InstanceOf,
    TreatAs,
    CastAs,
    CastableAs,

    // Kind test keywords
    Element,
    Attribute,
    Text,
    Comment,
    Node,
    DocumentNode,
    ProcessingInstruction,
    SchemaElement,
    SchemaAttribute,
    Item,
    EmptySequence,

    // Axis specifiers (composite tokens: name + "::")
    AxisChild,
    AxisDescendant,
    AxisAttribute,
    AxisSelf,
    AxisDescendantOrSelf,
    AxisFollowingSibling,
    AxisFollowing,
    AxisParent,
    AxisAncestor,
    AxisPrecedingSibling,
    AxisPreceding,
    AxisAncestorOrSelf,
    AxisNamespace,

    // Value comparison operators
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Is,

    // Occurrence indicators (emitted in OccurrenceIndicator state)
    OccurrenceZeroOrOne,  // ?
    OccurrenceOneOrMore,  // +
    OccurrenceZeroOrMore, // *

    // Multi-character operators
    DoublePeriod,  // ..
    DoubleSlash,   // //
    NotEquals,     // !=
    LessEquals,    // <=
    GreaterEquals, // >=
    DoubleLess,    // <<
    DoubleGreater, // >>

    // Single-character tokens
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Colon,
    At,
    Dollar,
    Slash,
    /// "/" when NOT followed by something that starts a path step.
    /// Used to resolve the shift/reduce conflict for standalone "/" (root).
    SlashOnly,
    Pipe,
    Plus,
    Minus,
    // Mode-specific unary operators (for precedence routing in grammar)
    Minus10, // XPath 1.0 unary minus (pre-union precedence)
    Plus10,  // XPath 1.0 unary plus (pre-union precedence)
    Minus20, // XPath 2.0 unary minus (post-union precedence)
    Plus20,  // XPath 2.0 unary plus (post-union precedence)
    Star,
    Equals,
    LessThan,
    GreaterThan,
    Question,
    Dot,

    // End of input
    Eof,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::StringLiteral(s) => write!(f, "\"{}\"", s),
            Token::IntegerLiteral(s) => write!(f, "{}", s),
            Token::DecimalLiteral(s) => write!(f, "{}", s),
            Token::DoubleLiteral(s) => write!(f, "{}", s),
            Token::NCName(s) => write!(f, "{}", s),
            Token::QName(s) => write!(f, "{}", s),
            Token::QNameNillable(s) => write!(f, "{}?", s),
            Token::VarName { prefix, local } => {
                if prefix.is_empty() {
                    write!(f, "${}", local)
                } else {
                    write!(f, "${}:{}", prefix, local)
                }
            }
            Token::For => write!(f, "for"),
            Token::In => write!(f, "in"),
            Token::Return => write!(f, "return"),
            Token::If => write!(f, "if"),
            Token::Then => write!(f, "then"),
            Token::Else => write!(f, "else"),
            Token::Some => write!(f, "some"),
            Token::Every => write!(f, "every"),
            Token::Satisfies => write!(f, "satisfies"),
            Token::And => write!(f, "and"),
            Token::Or => write!(f, "or"),
            Token::To => write!(f, "to"),
            Token::Div => write!(f, "div"),
            Token::IDiv => write!(f, "idiv"),
            Token::Mod => write!(f, "mod"),
            Token::Union => write!(f, "union"),
            Token::Except => write!(f, "except"),
            Token::Intersect => write!(f, "intersect"),
            Token::InstanceOf => write!(f, "instance of"),
            Token::TreatAs => write!(f, "treat as"),
            Token::CastAs => write!(f, "cast as"),
            Token::CastableAs => write!(f, "castable as"),
            Token::Element => write!(f, "element"),
            Token::Attribute => write!(f, "attribute"),
            Token::Text => write!(f, "text"),
            Token::Comment => write!(f, "comment"),
            Token::Node => write!(f, "node"),
            Token::DocumentNode => write!(f, "document-node"),
            Token::ProcessingInstruction => write!(f, "processing-instruction"),
            Token::SchemaElement => write!(f, "schema-element"),
            Token::SchemaAttribute => write!(f, "schema-attribute"),
            Token::Item => write!(f, "item"),
            Token::EmptySequence => write!(f, "empty-sequence"),
            Token::AxisChild => write!(f, "child::"),
            Token::AxisDescendant => write!(f, "descendant::"),
            Token::AxisAttribute => write!(f, "attribute::"),
            Token::AxisSelf => write!(f, "self::"),
            Token::AxisDescendantOrSelf => write!(f, "descendant-or-self::"),
            Token::AxisFollowingSibling => write!(f, "following-sibling::"),
            Token::AxisFollowing => write!(f, "following::"),
            Token::AxisParent => write!(f, "parent::"),
            Token::AxisAncestor => write!(f, "ancestor::"),
            Token::AxisPrecedingSibling => write!(f, "preceding-sibling::"),
            Token::AxisPreceding => write!(f, "preceding::"),
            Token::AxisAncestorOrSelf => write!(f, "ancestor-or-self::"),
            Token::AxisNamespace => write!(f, "namespace::"),
            Token::Eq => write!(f, "eq"),
            Token::Ne => write!(f, "ne"),
            Token::Lt => write!(f, "lt"),
            Token::Le => write!(f, "le"),
            Token::Gt => write!(f, "gt"),
            Token::Ge => write!(f, "ge"),
            Token::Is => write!(f, "is"),
            Token::OccurrenceZeroOrOne => write!(f, "?"),
            Token::OccurrenceOneOrMore => write!(f, "+"),
            Token::OccurrenceZeroOrMore => write!(f, "*"),
            Token::DoublePeriod => write!(f, ".."),
            Token::DoubleSlash => write!(f, "//"),
            Token::NotEquals => write!(f, "!="),
            Token::LessEquals => write!(f, "<="),
            Token::GreaterEquals => write!(f, ">="),
            Token::DoubleLess => write!(f, "<<"),
            Token::DoubleGreater => write!(f, ">>"),
            Token::LParen => write!(f, "("),
            Token::RParen => write!(f, ")"),
            Token::LBracket => write!(f, "["),
            Token::RBracket => write!(f, "]"),
            Token::Comma => write!(f, ","),
            Token::Colon => write!(f, ":"),
            Token::At => write!(f, "@"),
            Token::Dollar => write!(f, "$"),
            Token::Slash => write!(f, "/"),
            Token::SlashOnly => write!(f, "/"),
            Token::Pipe => write!(f, "|"),
            Token::Plus => write!(f, "+"),
            Token::Minus => write!(f, "-"),
            Token::Minus10 | Token::Minus20 => write!(f, "-"),
            Token::Plus10 | Token::Plus20 => write!(f, "+"),
            Token::Star => write!(f, "*"),
            Token::Equals => write!(f, "="),
            Token::LessThan => write!(f, "<"),
            Token::GreaterThan => write!(f, ">"),
            Token::Question => write!(f, "?"),
            Token::Dot => write!(f, "."),
            Token::Eof => write!(f, "EOF"),
        }
    }
}

/// Lexer error.
#[derive(Debug, Clone, PartialEq)]
pub struct LexerError {
    pub message: String,
    pub position: usize,
}

impl fmt::Display for LexerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Lexer error at position {}: {}",
            self.position, self.message
        )
    }
}

impl std::error::Error for LexerError {}

/// Spanned token: (start_position, token, end_position).
pub type Spanned = (usize, Token, usize);

/// Lexer state machine states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LexerState {
    /// Initial state, expecting expression start.
    Default,
    /// After an operand, expecting an operator.
    Operator,
    /// After `cast as` / `castable as`, expecting atomic type name.
    SingleType,
    /// After `instance of` / `treat as`, expecting item type.
    ItemType,
    /// Inside kind test parentheses (e.g., `element(...)`).
    KindTest,
    /// Inside `processing-instruction()`.
    KindTestForPi,
    /// After QName in kind test, expecting `)` or `,`.
    CloseKindTest,
    /// After `,` in kind test, expecting type name (which may have `?` suffix).
    TypeNameInKindTest,
    /// After item type, expecting occurrence indicator.
    OccurrenceIndicator,
    /// After `$`, expecting variable name.
    VarName,
}

/// XPath 2.0 lexer with stateful lookahead.
pub struct Lexer<'input> {
    #[allow(dead_code)]
    input: &'input str,
    chars: Vec<char>,
    pos: usize,
    state: LexerState,
    state_stack: Vec<LexerState>,
    token_queue: VecDeque<Spanned>,
    /// Where an unclosed comment began, once one has been found: XPath 2.0
    /// §A.2.4 `Comment ::= "(:" (CommentContents | Comment)* ":)"` has no
    /// other end, so the expression is not well formed (XPST0003).
    unclosed_comment: Option<usize>,
    finished: bool,
    mode: XPathMode,
}

impl<'input> Lexer<'input> {
    /// Create a new lexer for the given input (XPath 2.0 mode).
    pub fn new(input: &'input str) -> Self {
        Self {
            input,
            chars: input.chars().collect(),
            pos: 0,
            state: LexerState::Default,
            state_stack: Vec::new(),
            token_queue: VecDeque::new(),
            unclosed_comment: None,
            finished: false,
            mode: XPathMode::XPath20,
        }
    }

    /// Create a new lexer with an explicit XPath mode.
    pub fn new_with_mode(input: &'input str, mode: XPathMode) -> Self {
        Self {
            input,
            chars: input.chars().collect(),
            pos: 0,
            state: LexerState::Default,
            state_stack: Vec::new(),
            token_queue: VecDeque::new(),
            unclosed_comment: None,
            finished: false,
            mode,
        }
    }

    /// Check if the lexer is in XPath 1.0 mode.
    fn is_xpath10(&self) -> bool {
        self.mode == XPathMode::XPath10
    }

    /// Peek at a character at the given offset from current position.
    #[inline]
    fn peek(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    /// Peek at the current character.
    #[inline]
    fn current(&self) -> Option<char> {
        self.peek(0)
    }

    /// Advance position by n characters.
    #[inline]
    fn advance(&mut self, n: usize) {
        self.pos += n;
    }

    /// Read and advance one character.
    #[inline]
    fn read(&mut self) -> Option<char> {
        let c = self.current();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    /// Check if a character is an XML NCName start character.
    ///
    /// XML 1.0 §2.3 `NameStartChar`, minus `":"` (Namespaces in XML §3,
    /// `NCName`). This is a fixed list of code-point ranges, not a Unicode
    /// property: `Alphabetic` would also admit characters such as U+00B5
    /// MICRO SIGN, which the production excludes because its ranges start at
    /// #xC0.
    fn is_ncname_start(c: char) -> bool {
        matches!(c,
            'A'..='Z'
            | '_'
            | 'a'..='z'
            | '\u{C0}'..='\u{D6}'
            | '\u{D8}'..='\u{F6}'
            | '\u{F8}'..='\u{2FF}'
            | '\u{370}'..='\u{37D}'
            | '\u{37F}'..='\u{1FFF}'
            | '\u{200C}'..='\u{200D}'
            | '\u{2070}'..='\u{218F}'
            | '\u{2C00}'..='\u{2FEF}'
            | '\u{3001}'..='\u{D7FF}'
            | '\u{F900}'..='\u{FDCF}'
            | '\u{FDF0}'..='\u{FFFD}'
            | '\u{10000}'..='\u{EFFFF}')
    }

    /// Check if a character is an XML NCName character.
    ///
    /// XML 1.0 §2.3 `NameChar`, minus `":"`. Note that only ASCII digits are
    /// added here — the other decimal digits that `char::is_alphanumeric`
    /// would accept are covered (or not) by the `NameStartChar` ranges.
    fn is_ncname_char(c: char) -> bool {
        Self::is_ncname_start(c)
            || matches!(c,
                '-' | '.'
                | '0'..='9'
                | '\u{B7}'
                | '\u{300}'..='\u{36F}'
                | '\u{203F}'..='\u{2040}')
    }

    /// Check if a character is a digit.
    fn is_digit(c: char) -> bool {
        c.is_ascii_digit()
    }

    /// Check if a character is whitespace.
    fn is_whitespace(c: char) -> bool {
        matches!(c, ' ' | '\t' | '\r' | '\n')
    }

    /// Check if the current position (after skipping whitespace/comments) can start a path step.
    /// This is used to disambiguate "/" alone vs "/" followed by a relative path.
    fn can_start_relative_path(&self) -> bool {
        let mut i = 0;

        // Skip whitespace and comments
        loop {
            while self.pos + i < self.chars.len() && Self::is_whitespace(self.chars[self.pos + i]) {
                i += 1;
            }

            // Check for comment
            if self.pos + i + 1 < self.chars.len()
                && self.chars[self.pos + i] == '('
                && self.chars[self.pos + i + 1] == ':'
            {
                i += 2;
                let mut depth = 1;
                while depth > 0 && self.pos + i < self.chars.len() {
                    if self.pos + i + 1 < self.chars.len()
                        && self.chars[self.pos + i] == '('
                        && self.chars[self.pos + i + 1] == ':'
                    {
                        i += 2;
                        depth += 1;
                    } else if self.pos + i + 1 < self.chars.len()
                        && self.chars[self.pos + i] == ':'
                        && self.chars[self.pos + i + 1] == ')'
                    {
                        i += 2;
                        depth -= 1;
                    } else {
                        i += 1;
                    }
                }
                continue;
            }
            break;
        }

        // Now check what character is at position self.pos + i
        if self.pos + i >= self.chars.len() {
            return false; // EOF - "/" is alone
        }

        let c = self.chars[self.pos + i];

        // Characters/tokens that can start a RelativePathExpr (StepExpr):
        // - NCName start char (for QName, axis::, kind-test)
        // - '@' (attribute shorthand)
        // - '*' (wildcard)
        // - '.' (context item or '..' parent)
        // - '$' (variable reference in filter)
        // - '(' (parenthesized expression)
        // - '"' or '\'' (string literal in filter)
        // - digit (numeric literal in filter)
        matches!(c,
            'a'..='z' | 'A'..='Z' | '_' |  // NCName start
            '@' | '*' | '.' | '$' | '(' | '"' | '\''
        ) || c.is_ascii_digit()
            || c.is_alphabetic()
    }

    /// Skip whitespace and comments.
    fn skip_whitespace_and_comments(&mut self) {
        loop {
            // Skip whitespace
            while let Some(c) = self.current() {
                if Self::is_whitespace(c) {
                    self.advance(1);
                } else {
                    break;
                }
            }

            // Check for comment (: ... :)
            if self.peek(0) == Some('(') && self.peek(1) == Some(':') {
                let start = self.pos;
                self.advance(2);
                let mut depth = 1;
                while depth > 0 {
                    match (self.peek(0), self.peek(1)) {
                        (Some('('), Some(':')) => {
                            self.advance(2);
                            depth += 1;
                        }
                        (Some(':'), Some(')')) => {
                            self.advance(2);
                            depth -= 1;
                        }
                        (Some(_), _) => {
                            self.advance(1);
                        }
                        (None, _) => {
                            // Unclosed: reported by `next` as a lexer error.
                            self.unclosed_comment.get_or_insert(start);
                            break;
                        }
                    }
                }
                continue;
            }

            break;
        }
    }

    /// Match an identifier sequence with lookahead.
    /// Returns true if all parts match, and consumes the input.
    /// Parts can be identifiers or specific strings like "(" or "::".
    fn match_identifier(&mut self, parts: &[&str]) -> bool {
        let start_pos = self.pos;
        let mut i = 0;

        for part in parts {
            // Skip whitespace and comments before each part
            while i < self.chars.len() - self.pos {
                let idx = self.pos + i;
                if idx >= self.chars.len() {
                    break;
                }
                let c = self.chars[idx];
                if Self::is_whitespace(c) {
                    i += 1;
                    continue;
                }
                // Check for comment
                if idx + 1 < self.chars.len()
                    && self.chars[idx] == '('
                    && self.chars[idx + 1] == ':'
                {
                    i += 2;
                    let mut depth = 1;
                    while depth > 0 && self.pos + i < self.chars.len() {
                        if self.pos + i + 1 < self.chars.len()
                            && self.chars[self.pos + i] == '('
                            && self.chars[self.pos + i + 1] == ':'
                        {
                            i += 2;
                            depth += 1;
                        } else if self.pos + i + 1 < self.chars.len()
                            && self.chars[self.pos + i] == ':'
                            && self.chars[self.pos + i + 1] == ')'
                        {
                            i += 2;
                            depth -= 1;
                        } else {
                            i += 1;
                        }
                    }
                    continue;
                }
                break;
            }

            // Now match the part
            let part_chars: Vec<char> = part.chars().collect();
            for (j, &pc) in part_chars.iter().enumerate() {
                let idx = self.pos + i + j;
                if idx >= self.chars.len() || self.chars[idx] != pc {
                    self.pos = start_pos;
                    return false;
                }
            }

            // If part is an identifier, check that it's not followed by more NCName chars
            if !part.is_empty() && Self::is_ncname_start(part_chars[0]) {
                let after_idx = self.pos + i + part_chars.len();
                if after_idx < self.chars.len() && Self::is_ncname_char(self.chars[after_idx]) {
                    self.pos = start_pos;
                    return false;
                }
            }

            i += part_chars.len();
        }

        // Success - consume all matched characters
        self.pos += i;
        true
    }

    /// Try to match an identifier sequence without consuming input.
    fn try_match_identifier(&self, parts: &[&str]) -> bool {
        let mut i = 0;

        for part in parts {
            // Skip whitespace and comments before each part
            while self.pos + i < self.chars.len() {
                let idx = self.pos + i;
                let c = self.chars[idx];
                if Self::is_whitespace(c) {
                    i += 1;
                    continue;
                }
                // Check for comment
                if idx + 1 < self.chars.len()
                    && self.chars[idx] == '('
                    && self.chars[idx + 1] == ':'
                {
                    i += 2;
                    let mut depth = 1;
                    while depth > 0 && self.pos + i < self.chars.len() {
                        if self.pos + i + 1 < self.chars.len()
                            && self.chars[self.pos + i] == '('
                            && self.chars[self.pos + i + 1] == ':'
                        {
                            i += 2;
                            depth += 1;
                        } else if self.pos + i + 1 < self.chars.len()
                            && self.chars[self.pos + i] == ':'
                            && self.chars[self.pos + i + 1] == ')'
                        {
                            i += 2;
                            depth -= 1;
                        } else {
                            i += 1;
                        }
                    }
                    continue;
                }
                break;
            }

            // Now match the part
            let part_chars: Vec<char> = part.chars().collect();
            for (j, &pc) in part_chars.iter().enumerate() {
                let idx = self.pos + i + j;
                if idx >= self.chars.len() || self.chars[idx] != pc {
                    return false;
                }
            }

            // If part is an identifier, check boundary
            if !part.is_empty() && Self::is_ncname_start(part_chars[0]) {
                let after_idx = self.pos + i + part_chars.len();
                if after_idx < self.chars.len() && Self::is_ncname_char(self.chars[after_idx]) {
                    return false;
                }
            }

            i += part_chars.len();
        }

        true
    }

    /// Consume and return an NCName.
    fn consume_ncname(&mut self) -> String {
        let start = self.pos;
        while let Some(c) = self.current() {
            if Self::is_ncname_char(c) {
                self.advance(1);
            } else {
                break;
            }
        }
        self.chars[start..self.pos].iter().collect()
    }

    /// Consume and return a QName (NCName or NCName:NCName).
    fn consume_qname(&mut self) -> String {
        let start = self.pos;
        while let Some(c) = self.current() {
            // Allow : only if followed by NCName char
            if (c == ':' && self.peek(1).map(Self::is_ncname_char).unwrap_or(false))
                || Self::is_ncname_char(c)
            {
                self.advance(1);
            } else {
                break;
            }
        }
        self.chars[start..self.pos].iter().collect()
    }

    /// Consume a numeric literal.
    fn consume_number(&mut self) -> Result<(Token, usize, usize), LexerError> {
        let start = self.pos;
        let mut is_decimal = false;
        let mut is_double = false;

        // Integer part
        while let Some(c) = self.current() {
            if Self::is_digit(c) {
                self.advance(1);
            } else {
                break;
            }
        }

        // Decimal part. XPath 2.0 §A.2.1:
        //   DecimalLiteral ::= ("." Digits) | (Digits "." [0-9]*)
        // The fractional digits are optional once at least one digit precedes
        // the ".", so `5.` is a DecimalLiteral; with no leading digit the "."
        // must be followed by at least one digit.
        if self.current() == Some('.')
            && (self.pos > start || self.peek(1).map(Self::is_digit).unwrap_or(false))
        {
            is_decimal = true;
            self.advance(1);
            while let Some(c) = self.current() {
                if Self::is_digit(c) {
                    self.advance(1);
                } else {
                    break;
                }
            }
        }

        // Exponent part. XPath 2.0 §A.2.1:
        //   DoubleLiteral ::= (("." Digits) | (Digits ("." [0-9]*)?)) [eE] [+-]? Digits
        // so the exponent needs at least one digit: `1e` and `5.0e+` are not
        // numeric literals (and a letter straight after a numeric literal is not
        // a separate token either, §A.2.2), which is a syntax error.
        if let Some(c) = self.current() {
            if c == 'e' || c == 'E' {
                is_double = true;
                self.advance(1);
                if let Some(sign) = self.current() {
                    if sign == '+' || sign == '-' {
                        self.advance(1);
                    }
                }
                let digits_start = self.pos;
                while let Some(c) = self.current() {
                    if Self::is_digit(c) {
                        self.advance(1);
                    } else {
                        break;
                    }
                }
                if self.pos == digits_start {
                    let literal: String = self.chars[start..self.pos].iter().collect();
                    return Err(LexerError {
                        message: format!(
                            "Invalid numeric literal '{literal}': the exponent has no digits"
                        ),
                        position: start,
                    });
                }
            }
        }

        let value: String = self.chars[start..self.pos].iter().collect();
        let token = if is_double {
            Token::DoubleLiteral(value)
        } else if is_decimal {
            Token::DecimalLiteral(value)
        } else {
            Token::IntegerLiteral(value)
        };

        Ok((token, start, self.pos))
    }

    /// Consume a string literal.
    fn consume_string(&mut self) -> Result<(Token, usize, usize), LexerError> {
        let start = self.pos;
        let quote = self.read().unwrap();
        let mut value = String::new();

        loop {
            match self.current() {
                None => {
                    return Err(LexerError {
                        message: "Unterminated string literal".to_string(),
                        position: start,
                    });
                }
                Some(c) if c == quote => {
                    self.advance(1);
                    // Check for escaped quote
                    if self.current() == Some(quote) {
                        value.push(quote);
                        self.advance(1);
                    } else {
                        break;
                    }
                }
                Some(c) => {
                    value.push(c);
                    self.advance(1);
                }
            }
        }

        Ok((Token::StringLiteral(value), start, self.pos))
    }

    /// Enqueue a token.
    fn enqueue(&mut self, token: Token, start: usize, end: usize) {
        self.token_queue.push_back((start, token, end));
    }

    /// Process tokens in Default state.
    fn default_state(&mut self) -> Result<(), LexerError> {
        self.skip_whitespace_and_comments();
        let start = self.pos;

        match self.current() {
            None => {
                self.enqueue(Token::Eof, start, start);
            }

            Some('.') => {
                if self.peek(1) == Some('.') {
                    self.advance(2);
                    self.enqueue(Token::DoublePeriod, start, self.pos);
                } else if self.peek(1).map(Self::is_digit).unwrap_or(false) {
                    let (tok, s, e) = self.consume_number()?;
                    self.enqueue(tok, s, e);
                } else {
                    self.advance(1);
                    self.enqueue(Token::Dot, start, self.pos);
                }
                self.state = LexerState::Operator;
            }

            Some(')') => {
                self.advance(1);
                self.enqueue(Token::RParen, start, self.pos);
                self.state = LexerState::Operator;
            }

            Some('*') => {
                self.advance(1);
                let star_end = self.pos;
                // Check for *:NCName wildcard
                if self.current() == Some(':') {
                    self.advance(1);
                    if self.current().map(Self::is_ncname_start).unwrap_or(false) {
                        self.enqueue(Token::Star, start, star_end);
                        let colon_start = star_end;
                        self.enqueue(Token::Colon, colon_start, colon_start + 1);
                        let ncname_start = self.pos;
                        let ncname = self.consume_ncname();
                        self.enqueue(Token::NCName(ncname), ncname_start, self.pos);
                    } else {
                        // Just star followed by colon (unusual but valid)
                        self.pos = star_end; // Back up
                        self.enqueue(Token::Star, start, star_end);
                    }
                } else {
                    self.enqueue(Token::Star, start, self.pos);
                }
                self.state = LexerState::Operator;
            }

            Some(c @ (';' | ',' | '(' | '-' | '+' | '@' | '~')) => {
                self.advance(1);
                let token = match c {
                    ';' => Token::Comma, // Semicolon treated as comma? Actually not standard
                    ',' => Token::Comma,
                    '(' => Token::LParen,
                    '-' => {
                        if self.is_xpath10() {
                            Token::Minus10
                        } else {
                            Token::Minus20
                        }
                    }
                    '+' => {
                        if self.is_xpath10() {
                            Token::Plus10
                        } else {
                            Token::Plus20
                        }
                    }
                    '@' => Token::At,
                    '~' => {
                        if self.is_xpath10() {
                            Token::Minus10
                        } else {
                            Token::Minus20
                        }
                    }
                    _ => unreachable!(),
                };
                self.enqueue(token, start, self.pos);
            }

            Some('/') => {
                if self.peek(1) == Some('/') {
                    self.advance(2);
                    self.enqueue(Token::DoubleSlash, start, self.pos);
                } else {
                    self.advance(1);
                    // Check if something can follow that starts a path step
                    if self.can_start_relative_path() {
                        self.enqueue(Token::Slash, start, self.pos);
                    } else {
                        // "/" alone (root document)
                        self.enqueue(Token::SlashOnly, start, self.pos);
                        self.state = LexerState::Operator;
                    }
                }
            }

            Some('$') => {
                self.advance(1);
                self.enqueue(Token::Dollar, start, self.pos);
                self.state = LexerState::VarName;
            }

            Some('[') => {
                self.advance(1);
                self.enqueue(Token::LBracket, start, self.pos);
                self.state_stack.push(self.state);
            }

            Some(']') => {
                self.advance(1);
                self.enqueue(Token::RBracket, start, self.pos);
                if let Some(s) = self.state_stack.pop() {
                    self.state = s;
                }
            }

            Some('"') | Some('\'') => {
                let (tok, s, e) = self.consume_string()?;
                self.enqueue(tok, s, e);
                self.state = LexerState::Operator;
            }

            Some(c) if Self::is_digit(c) => {
                let (tok, s, e) = self.consume_number()?;
                self.enqueue(tok, s, e);
                self.state = LexerState::Operator;
            }

            Some(c) if Self::is_ncname_start(c) => {
                self.process_name_in_default_state(start)?;
            }

            Some(c) => {
                return Err(LexerError {
                    message: format!("Unexpected character: '{}'", c),
                    position: start,
                });
            }
        }

        Ok(())
    }

    /// Process a name/keyword in Default state with lookahead.
    fn process_name_in_default_state(&mut self, start: usize) -> Result<(), LexerError> {
        // Check for keywords and special constructs with lookahead

        // if ( — XPath 2.0 only; in 1.0 mode, "if(" is a function call
        if !self.is_xpath10() && self.match_identifier(&["if", "("]) {
            self.enqueue(Token::If, start, self.pos - 1);
            self.enqueue(Token::LParen, self.pos - 1, self.pos);
            return Ok(());
        }

        // `for $` — XPath 2.0 only, and only when a variable really follows.
        // `for`, `some` and `every` are not reserved words in XPath 2.0, so
        // `@for`, `some:a` and an element named `every` are ordinary names;
        // in 1.0 mode they are plain NCNames in every position.
        if !self.is_xpath10() && self.try_match_identifier(&["for", "$"]) {
            self.match_identifier(&["for"]);
            self.enqueue(Token::For, start, self.pos);
            self.skip_whitespace_and_comments();
            if self.current() == Some('$') {
                let dollar_start = self.pos;
                self.advance(1);
                self.enqueue(Token::Dollar, dollar_start, self.pos);
                self.state = LexerState::VarName;
            }
            return Ok(());
        }

        // some $ — XPath 2.0 only
        if !self.is_xpath10() && self.try_match_identifier(&["some", "$"]) {
            self.match_identifier(&["some"]);
            self.enqueue(Token::Some, start, self.pos);
            self.skip_whitespace_and_comments();
            if self.current() == Some('$') {
                let dollar_start = self.pos;
                self.advance(1);
                self.enqueue(Token::Dollar, dollar_start, self.pos);
                self.state = LexerState::VarName;
            }
            return Ok(());
        }

        // every $ — XPath 2.0 only
        if !self.is_xpath10() && self.try_match_identifier(&["every", "$"]) {
            self.match_identifier(&["every"]);
            self.enqueue(Token::Every, start, self.pos);
            self.skip_whitespace_and_comments();
            if self.current() == Some('$') {
                let dollar_start = self.pos;
                self.advance(1);
                self.enqueue(Token::Dollar, dollar_start, self.pos);
                self.state = LexerState::VarName;
            }
            return Ok(());
        }

        // Kind tests with (
        // XPath 2.0-only kind tests: element(, attribute(, schema-element(, schema-attribute(, document-node(
        // In XPath 1.0 mode these fall through to QName + "(" (function call)
        if !self.is_xpath10() && self.match_identifier(&["element", "("]) {
            self.enqueue(Token::Element, start, self.pos - 1);
            self.enqueue(Token::LParen, self.pos - 1, self.pos);
            self.state_stack.push(LexerState::Operator);
            self.state = LexerState::KindTest;
            return Ok(());
        }
        if !self.is_xpath10() && self.match_identifier(&["attribute", "("]) {
            self.enqueue(Token::Attribute, start, self.pos - 1);
            self.enqueue(Token::LParen, self.pos - 1, self.pos);
            self.state_stack.push(LexerState::Operator);
            self.state = LexerState::KindTest;
            return Ok(());
        }
        if !self.is_xpath10() && self.match_identifier(&["schema-element", "("]) {
            self.enqueue(Token::SchemaElement, start, self.pos - 1);
            self.enqueue(Token::LParen, self.pos - 1, self.pos);
            self.state_stack.push(LexerState::Operator);
            self.state = LexerState::KindTest;
            return Ok(());
        }
        if !self.is_xpath10() && self.match_identifier(&["schema-attribute", "("]) {
            self.enqueue(Token::SchemaAttribute, start, self.pos - 1);
            self.enqueue(Token::LParen, self.pos - 1, self.pos);
            self.state_stack.push(LexerState::Operator);
            self.state = LexerState::KindTest;
            return Ok(());
        }
        // XPath 1.0 kind tests kept in both modes: comment(, text(, node(, processing-instruction(
        if self.match_identifier(&["comment", "("]) {
            self.enqueue(Token::Comment, start, self.pos - 1);
            self.enqueue(Token::LParen, self.pos - 1, self.pos);
            self.state_stack.push(LexerState::Operator);
            self.state = LexerState::KindTest;
            return Ok(());
        }
        if self.match_identifier(&["text", "("]) {
            self.enqueue(Token::Text, start, self.pos - 1);
            self.enqueue(Token::LParen, self.pos - 1, self.pos);
            self.state_stack.push(LexerState::Operator);
            self.state = LexerState::KindTest;
            return Ok(());
        }
        if self.match_identifier(&["node", "("]) {
            self.enqueue(Token::Node, start, self.pos - 1);
            self.enqueue(Token::LParen, self.pos - 1, self.pos);
            self.state_stack.push(LexerState::Operator);
            self.state = LexerState::KindTest;
            return Ok(());
        }
        if !self.is_xpath10() && self.match_identifier(&["document-node", "("]) {
            self.enqueue(Token::DocumentNode, start, self.pos - 1);
            self.enqueue(Token::LParen, self.pos - 1, self.pos);
            self.state_stack.push(LexerState::Operator);
            self.state = LexerState::KindTest;
            return Ok(());
        }
        if self.match_identifier(&["processing-instruction", "("]) {
            self.enqueue(Token::ProcessingInstruction, start, self.pos - 1);
            self.enqueue(Token::LParen, self.pos - 1, self.pos);
            self.state_stack.push(LexerState::Operator);
            self.state = LexerState::KindTestForPi;
            return Ok(());
        }

        // Axis specifiers with ::
        if self.match_identifier(&["ancestor-or-self", "::"]) {
            self.enqueue(Token::AxisAncestorOrSelf, start, self.pos);
            return Ok(());
        }
        if self.match_identifier(&["ancestor", "::"]) {
            self.enqueue(Token::AxisAncestor, start, self.pos);
            return Ok(());
        }
        if self.match_identifier(&["attribute", "::"]) {
            self.enqueue(Token::AxisAttribute, start, self.pos);
            return Ok(());
        }
        if self.match_identifier(&["child", "::"]) {
            self.enqueue(Token::AxisChild, start, self.pos);
            return Ok(());
        }
        if self.match_identifier(&["descendant-or-self", "::"]) {
            self.enqueue(Token::AxisDescendantOrSelf, start, self.pos);
            return Ok(());
        }
        if self.match_identifier(&["descendant", "::"]) {
            self.enqueue(Token::AxisDescendant, start, self.pos);
            return Ok(());
        }
        if self.match_identifier(&["following-sibling", "::"]) {
            self.enqueue(Token::AxisFollowingSibling, start, self.pos);
            return Ok(());
        }
        if self.match_identifier(&["following", "::"]) {
            self.enqueue(Token::AxisFollowing, start, self.pos);
            return Ok(());
        }
        if self.match_identifier(&["parent", "::"]) {
            self.enqueue(Token::AxisParent, start, self.pos);
            return Ok(());
        }
        if self.match_identifier(&["preceding-sibling", "::"]) {
            self.enqueue(Token::AxisPrecedingSibling, start, self.pos);
            return Ok(());
        }
        if self.match_identifier(&["preceding", "::"]) {
            self.enqueue(Token::AxisPreceding, start, self.pos);
            return Ok(());
        }
        if self.match_identifier(&["self", "::"]) {
            self.enqueue(Token::AxisSelf, start, self.pos);
            return Ok(());
        }
        if self.match_identifier(&["namespace", "::"]) {
            self.enqueue(Token::AxisNamespace, start, self.pos);
            return Ok(());
        }

        // Plain NCName or QName
        let name = self.consume_qname();
        let end = self.pos;

        // `NCName ":" "*"` — the second alternative of the Wildcard production
        // (XPath 2.0 §3.2.1). `consume_qname` stops before the colon, because
        // what follows is not an NCName character, so the three tokens the
        // grammar expects are emitted here.
        if !name.contains(':') && self.current() == Some(':') && self.peek(1) == Some('*') {
            let colon_start = self.pos;
            self.advance(2);
            self.enqueue(Token::NCName(name), start, end);
            self.enqueue(Token::Colon, colon_start, colon_start + 1);
            self.enqueue(Token::Star, colon_start + 1, self.pos);
            self.state = LexerState::Operator;
            return Ok(());
        }

        self.skip_whitespace_and_comments();

        // If followed by '(', it's a function call - stay in Default
        if self.current() != Some('(') {
            self.state = LexerState::Operator;
        }

        self.enqueue(Token::QName(name), start, end);

        Ok(())
    }

    /// Process tokens in VarName state.
    fn varname_state(&mut self) -> Result<(), LexerError> {
        self.skip_whitespace_and_comments();
        let start = self.pos;

        if self.current().map(Self::is_ncname_start).unwrap_or(false) {
            let first = self.consume_ncname();
            let (prefix, local) = if self.current() == Some(':')
                && self.peek(1).map(Self::is_ncname_start).unwrap_or(false)
            {
                self.advance(1);
                (first, self.consume_ncname())
            } else {
                (String::new(), first)
            };

            self.enqueue(Token::VarName { prefix, local }, start, self.pos);
            self.state = LexerState::Operator;
        }

        Ok(())
    }

    /// Process tokens in Operator state.
    fn operator_state(&mut self) -> Result<(), LexerError> {
        self.skip_whitespace_and_comments();
        let start = self.pos;

        match self.current() {
            None => {
                self.enqueue(Token::Eof, start, start);
            }

            Some(c @ (',' | '=' | '+' | '-' | '[' | '|')) => {
                self.advance(1);
                let token = match c {
                    ',' => Token::Comma,
                    '=' => Token::Equals,
                    '+' => Token::Plus,
                    '-' => Token::Minus,
                    '[' => {
                        self.state_stack.push(self.state);
                        Token::LBracket
                    }
                    '|' => Token::Pipe,
                    _ => unreachable!(),
                };
                self.enqueue(token, start, self.pos);
                self.state = LexerState::Default;
            }

            Some('*') => {
                self.advance(1);
                self.enqueue(Token::Star, start, self.pos);
                self.state = LexerState::Default;
            }

            Some('!') if self.peek(1) == Some('=') => {
                self.advance(2);
                self.enqueue(Token::NotEquals, start, self.pos);
                self.state = LexerState::Default;
            }

            Some('>') => {
                if self.peek(1) == Some('=') {
                    self.advance(2);
                    self.enqueue(Token::GreaterEquals, start, self.pos);
                } else if !self.is_xpath10() && self.peek(1) == Some('>') {
                    self.advance(2);
                    self.enqueue(Token::DoubleGreater, start, self.pos);
                } else {
                    self.advance(1);
                    self.enqueue(Token::GreaterThan, start, self.pos);
                }
                self.state = LexerState::Default;
            }

            Some('<') => {
                if self.peek(1) == Some('=') {
                    self.advance(2);
                    self.enqueue(Token::LessEquals, start, self.pos);
                } else if !self.is_xpath10() && self.peek(1) == Some('<') {
                    self.advance(2);
                    self.enqueue(Token::DoubleLess, start, self.pos);
                } else {
                    self.advance(1);
                    self.enqueue(Token::LessThan, start, self.pos);
                }
                self.state = LexerState::Default;
            }

            Some('/') => {
                if self.peek(1) == Some('/') {
                    self.advance(2);
                    self.enqueue(Token::DoubleSlash, start, self.pos);
                    self.state = LexerState::Default;
                } else {
                    self.advance(1);
                    // Check if something can follow that starts a path step
                    if self.can_start_relative_path() {
                        self.enqueue(Token::Slash, start, self.pos);
                        self.state = LexerState::Default;
                    } else {
                        // "/" alone - but in operator context this shouldn't happen often
                        // Still handle it for completeness
                        self.enqueue(Token::SlashOnly, start, self.pos);
                        // Stay in Operator state since SlashOnly is a complete expression
                    }
                }
            }

            Some(')') => {
                self.advance(1);
                self.enqueue(Token::RParen, start, self.pos);
            }

            Some('?') => {
                self.advance(1);
                self.enqueue(Token::Question, start, self.pos);
            }

            Some(']') => {
                self.advance(1);
                self.enqueue(Token::RBracket, start, self.pos);
                if let Some(s) = self.state_stack.pop() {
                    self.state = s;
                }
            }

            Some('$') => {
                self.advance(1);
                self.enqueue(Token::Dollar, start, self.pos);
                self.state = LexerState::VarName;
            }

            Some('"') | Some('\'') => {
                let (tok, s, e) = self.consume_string()?;
                self.enqueue(tok, s, e);
            }

            Some(c) if Self::is_ncname_start(c) => {
                self.process_keyword_in_operator_state(start)?;
            }

            Some(c) => {
                return Err(LexerError {
                    message: format!("Unexpected character in operator context: '{}'", c),
                    position: start,
                });
            }
        }

        Ok(())
    }

    /// Process keywords in Operator state.
    fn process_keyword_in_operator_state(&mut self, start: usize) -> Result<(), LexerError> {
        // Two-word type keywords — XPath 2.0 only
        if !self.is_xpath10() {
            if self.match_identifier(&["castable", "as"]) {
                self.enqueue(Token::CastableAs, start, self.pos);
                self.state = LexerState::SingleType;
                return Ok(());
            }
            if self.match_identifier(&["cast", "as"]) {
                self.enqueue(Token::CastAs, start, self.pos);
                self.state = LexerState::SingleType;
                return Ok(());
            }
            if self.match_identifier(&["instance", "of"]) {
                self.enqueue(Token::InstanceOf, start, self.pos);
                self.state = LexerState::ItemType;
                return Ok(());
            }
            if self.match_identifier(&["treat", "as"]) {
                self.enqueue(Token::TreatAs, start, self.pos);
                self.state = LexerState::ItemType;
                return Ok(());
            }
        }

        // Single-word keywords shared by both XPath 1.0 and 2.0
        let shared_keywords: &[(&str, Token, LexerState)] = &[
            ("and", Token::And, LexerState::Default),
            ("or", Token::Or, LexerState::Default),
            ("div", Token::Div, LexerState::Default),
            ("mod", Token::Mod, LexerState::Default),
        ];

        for (kw, tok, next_state) in shared_keywords {
            if self.match_identifier(&[kw]) {
                self.enqueue(tok.clone(), start, self.pos);
                self.state = *next_state;
                return Ok(());
            }
        }

        // Single-word keywords — XPath 2.0 only
        if !self.is_xpath10() {
            let xpath20_keywords: &[(&str, Token, LexerState)] = &[
                ("then", Token::Then, LexerState::Default),
                ("else", Token::Else, LexerState::Default),
                ("idiv", Token::IDiv, LexerState::Default),
                ("except", Token::Except, LexerState::Default),
                ("intersect", Token::Intersect, LexerState::Default),
                ("union", Token::Union, LexerState::Default),
                ("return", Token::Return, LexerState::Default),
                ("satisfies", Token::Satisfies, LexerState::Default),
                ("to", Token::To, LexerState::Default),
                ("in", Token::In, LexerState::Default),
                ("is", Token::Is, LexerState::Default),
                ("eq", Token::Eq, LexerState::Default),
                ("ne", Token::Ne, LexerState::Default),
                ("lt", Token::Lt, LexerState::Default),
                ("le", Token::Le, LexerState::Default),
                ("gt", Token::Gt, LexerState::Default),
                ("ge", Token::Ge, LexerState::Default),
            ];

            for (kw, tok, next_state) in xpath20_keywords {
                if self.match_identifier(&[kw]) {
                    self.enqueue(tok.clone(), start, self.pos);
                    self.state = *next_state;
                    return Ok(());
                }
            }
        }

        // for $ in operator context — XPath 2.0 only
        if !self.is_xpath10() && self.try_match_identifier(&["for"]) {
            self.match_identifier(&["for"]);
            self.enqueue(Token::For, start, self.pos);
            self.skip_whitespace_and_comments();
            if self.current() == Some('$') {
                let dollar_start = self.pos;
                self.advance(1);
                self.enqueue(Token::Dollar, dollar_start, self.pos);
                self.state = LexerState::VarName;
            } else {
                self.state = LexerState::Default;
            }
            return Ok(());
        }

        // In XPath 1.0 mode, unrecognized names in operator context are path steps (QNames)
        if self.is_xpath10() {
            let name = self.consume_qname();
            let end = self.pos;
            self.skip_whitespace_and_comments();
            // If followed by '(', it's a function call - stay in Default
            if self.current() != Some('(') {
                self.state = LexerState::Operator;
            } else {
                self.state = LexerState::Default;
            }
            self.enqueue(Token::QName(name), start, end);
            return Ok(());
        }

        // Not a keyword - error in operator context (XPath 2.0)
        let name = self.consume_qname();
        Err(LexerError {
            message: format!("Unexpected identifier in operator context: '{}'", name),
            position: start,
        })
    }

    /// Process tokens in SingleType state (after cast as / castable as).
    fn single_type_state(&mut self) -> Result<(), LexerError> {
        self.skip_whitespace_and_comments();
        let start = self.pos;

        if self.current().map(Self::is_ncname_start).unwrap_or(false) {
            let qname = self.consume_qname();
            self.enqueue(Token::QName(qname), start, self.pos);
            // After type name, check for optional "?" occurrence indicator
            self.state = LexerState::OccurrenceIndicator;
        }

        Ok(())
    }

    /// Process tokens in ItemType state (after instance of / treat as).
    fn item_type_state(&mut self) -> Result<(), LexerError> {
        self.skip_whitespace_and_comments();
        let start = self.pos;

        match self.current() {
            None => {
                self.enqueue(Token::Eof, start, start);
            }

            Some('$') => {
                self.advance(1);
                self.enqueue(Token::Dollar, start, self.pos);
                self.state = LexerState::VarName;
            }

            Some(')') => {
                self.advance(1);
                self.enqueue(Token::RParen, start, self.pos);
            }

            Some(c) if Self::is_ncname_start(c) => {
                // Check for kind tests and special tokens
                if self.match_identifier(&["empty-sequence", "(", ")"]) {
                    self.enqueue(Token::EmptySequence, start, self.pos);
                    self.state = LexerState::Operator;
                    return Ok(());
                }
                if self.match_identifier(&["item", "(", ")"]) {
                    self.enqueue(Token::Item, start, self.pos);
                    self.state = LexerState::OccurrenceIndicator;
                    return Ok(());
                }

                // Kind tests
                if self.match_identifier(&["element", "("]) {
                    self.enqueue(Token::Element, start, self.pos - 1);
                    self.enqueue(Token::LParen, self.pos - 1, self.pos);
                    self.state_stack.push(LexerState::OccurrenceIndicator);
                    self.state = LexerState::KindTest;
                    return Ok(());
                }
                if self.match_identifier(&["attribute", "("]) {
                    self.enqueue(Token::Attribute, start, self.pos - 1);
                    self.enqueue(Token::LParen, self.pos - 1, self.pos);
                    self.state_stack.push(LexerState::OccurrenceIndicator);
                    self.state = LexerState::KindTest;
                    return Ok(());
                }
                if self.match_identifier(&["schema-element", "("]) {
                    self.enqueue(Token::SchemaElement, start, self.pos - 1);
                    self.enqueue(Token::LParen, self.pos - 1, self.pos);
                    self.state_stack.push(LexerState::OccurrenceIndicator);
                    self.state = LexerState::KindTest;
                    return Ok(());
                }
                if self.match_identifier(&["schema-attribute", "("]) {
                    self.enqueue(Token::SchemaAttribute, start, self.pos - 1);
                    self.enqueue(Token::LParen, self.pos - 1, self.pos);
                    self.state_stack.push(LexerState::OccurrenceIndicator);
                    self.state = LexerState::KindTest;
                    return Ok(());
                }
                if self.match_identifier(&["comment", "("]) {
                    self.enqueue(Token::Comment, start, self.pos - 1);
                    self.enqueue(Token::LParen, self.pos - 1, self.pos);
                    self.state_stack.push(LexerState::OccurrenceIndicator);
                    self.state = LexerState::KindTest;
                    return Ok(());
                }
                if self.match_identifier(&["text", "("]) {
                    self.enqueue(Token::Text, start, self.pos - 1);
                    self.enqueue(Token::LParen, self.pos - 1, self.pos);
                    self.state_stack.push(LexerState::OccurrenceIndicator);
                    self.state = LexerState::KindTest;
                    return Ok(());
                }
                if self.match_identifier(&["node", "("]) {
                    self.enqueue(Token::Node, start, self.pos - 1);
                    self.enqueue(Token::LParen, self.pos - 1, self.pos);
                    self.state_stack.push(LexerState::OccurrenceIndicator);
                    self.state = LexerState::KindTest;
                    return Ok(());
                }
                if self.match_identifier(&["document-node", "("]) {
                    self.enqueue(Token::DocumentNode, start, self.pos - 1);
                    self.enqueue(Token::LParen, self.pos - 1, self.pos);
                    self.state_stack.push(LexerState::OccurrenceIndicator);
                    self.state = LexerState::KindTest;
                    return Ok(());
                }
                if self.match_identifier(&["processing-instruction", "("]) {
                    self.enqueue(Token::ProcessingInstruction, start, self.pos - 1);
                    self.enqueue(Token::LParen, self.pos - 1, self.pos);
                    self.state_stack.push(LexerState::OccurrenceIndicator);
                    self.state = LexerState::KindTestForPi;
                    return Ok(());
                }

                // Atomic type name
                let qname = self.consume_qname();
                self.enqueue(Token::QName(qname), start, self.pos);
                self.state = LexerState::OccurrenceIndicator;
            }

            _ => {}
        }

        Ok(())
    }

    /// Process tokens in KindTest state.
    fn kind_test_state(&mut self) -> Result<(), LexerError> {
        self.skip_whitespace_and_comments();
        let start = self.pos;

        match self.current() {
            None => {}

            Some(')') => {
                self.advance(1);
                self.enqueue(Token::RParen, start, self.pos);
                if let Some(s) = self.state_stack.pop() {
                    self.state = s;
                }
            }

            Some('*') => {
                self.advance(1);
                self.enqueue(Token::Star, start, self.pos);
                self.state = LexerState::CloseKindTest;
            }

            Some(c) if Self::is_ncname_start(c) => {
                // Check for nested kind tests
                if self.match_identifier(&["element", "("]) {
                    self.enqueue(Token::Element, start, self.pos - 1);
                    self.enqueue(Token::LParen, self.pos - 1, self.pos);
                    self.state_stack.push(LexerState::KindTest);
                    return Ok(());
                }
                if self.match_identifier(&["schema-element", "("]) {
                    self.enqueue(Token::SchemaElement, start, self.pos - 1);
                    self.enqueue(Token::LParen, self.pos - 1, self.pos);
                    self.state_stack.push(LexerState::KindTest);
                    return Ok(());
                }

                let qname = self.consume_qname();
                self.enqueue(Token::QName(qname), start, self.pos);
                self.state = LexerState::CloseKindTest;
            }

            _ => {}
        }

        Ok(())
    }

    /// Process tokens in KindTestForPi state.
    fn kind_test_for_pi_state(&mut self) -> Result<(), LexerError> {
        self.skip_whitespace_and_comments();
        let start = self.pos;

        match self.current() {
            Some(')') => {
                self.advance(1);
                self.enqueue(Token::RParen, start, self.pos);
                if let Some(s) = self.state_stack.pop() {
                    self.state = s;
                }
            }

            Some(c) if Self::is_ncname_start(c) => {
                let ncname = self.consume_ncname();
                self.enqueue(Token::NCName(ncname), start, self.pos);
            }

            Some('"') | Some('\'') => {
                let (tok, s, e) = self.consume_string()?;
                self.enqueue(tok, s, e);
            }

            _ => {}
        }

        Ok(())
    }

    /// Process tokens in CloseKindTest state.
    fn close_kind_test_state(&mut self) -> Result<(), LexerError> {
        self.skip_whitespace_and_comments();
        let start = self.pos;

        match self.current() {
            Some(')') => {
                self.advance(1);
                self.enqueue(Token::RParen, start, self.pos);
                if let Some(s) = self.state_stack.pop() {
                    self.state = s;
                }
            }

            Some(',') => {
                self.advance(1);
                self.enqueue(Token::Comma, start, self.pos);
                // After comma, we expect type name which may have "?" suffix
                self.state = LexerState::TypeNameInKindTest;
            }

            _ => {}
        }

        Ok(())
    }

    /// Process tokens in TypeNameInKindTest state (after comma, expecting type name).
    fn type_name_in_kind_test_state(&mut self) -> Result<(), LexerError> {
        self.skip_whitespace_and_comments();
        let start = self.pos;

        match self.current() {
            Some(')') => {
                self.advance(1);
                self.enqueue(Token::RParen, start, self.pos);
                if let Some(s) = self.state_stack.pop() {
                    self.state = s;
                }
            }

            Some(c) if Self::is_ncname_start(c) => {
                let qname = self.consume_qname();
                let qname_end = self.pos;

                // Check for "?" suffix indicating nillable
                self.skip_whitespace_and_comments();
                if self.current() == Some('?') {
                    self.advance(1);
                    self.enqueue(Token::QNameNillable(qname), start, self.pos);
                } else {
                    self.enqueue(Token::QName(qname), start, qname_end);
                }
                self.state = LexerState::CloseKindTest;
            }

            _ => {}
        }

        Ok(())
    }

    /// Process tokens in OccurrenceIndicator state.
    fn occurrence_indicator_state(&mut self) -> Result<(), LexerError> {
        self.skip_whitespace_and_comments();
        let start = self.pos;

        match self.current() {
            Some('?') => {
                self.advance(1);
                self.enqueue(Token::OccurrenceZeroOrOne, start, self.pos);
            }
            Some('+') => {
                self.advance(1);
                self.enqueue(Token::OccurrenceOneOrMore, start, self.pos);
            }
            Some('*') => {
                self.advance(1);
                self.enqueue(Token::OccurrenceZeroOrMore, start, self.pos);
            }
            _ => {}
        }

        self.state = LexerState::Operator;
        // Process next token in operator state
        self.operator_state()?;

        Ok(())
    }

    /// Enter the appropriate state handler.
    fn enter_state(&mut self) -> Result<(), LexerError> {
        match self.state {
            LexerState::Default => self.default_state(),
            LexerState::Operator => self.operator_state(),
            LexerState::VarName => self.varname_state(),
            LexerState::SingleType => self.single_type_state(),
            LexerState::ItemType => self.item_type_state(),
            LexerState::KindTest => self.kind_test_state(),
            LexerState::KindTestForPi => self.kind_test_for_pi_state(),
            LexerState::CloseKindTest => self.close_kind_test_state(),
            LexerState::TypeNameInKindTest => self.type_name_in_kind_test_state(),
            LexerState::OccurrenceIndicator => self.occurrence_indicator_state(),
        }
    }
}

impl<'input> Iterator for Lexer<'input> {
    type Item = Result<Spanned, LexerError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        // Return queued tokens first
        if let Some(spanned) = self.token_queue.pop_front() {
            if spanned.1 == Token::Eof {
                self.finished = true;
            }
            return Some(Ok(spanned));
        }

        // Process more tokens
        if let Err(e) = self.enter_state() {
            self.finished = true;
            return Some(Err(e));
        }
        if let Some(position) = self.unclosed_comment {
            self.finished = true;
            return Some(Err(LexerError {
                message: "Unterminated comment: '(:' without its ':)'".to_string(),
                position,
            }));
        }

        // Return newly queued token
        if let Some(spanned) = self.token_queue.pop_front() {
            if spanned.1 == Token::Eof {
                self.finished = true;
            }
            Some(Ok(spanned))
        } else {
            self.finished = true;
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokenize(input: &str) -> Vec<Token> {
        Lexer::new(input)
            .map(|r| r.unwrap().1)
            .filter(|t| *t != Token::Eof)
            .collect()
    }

    #[test]
    fn test_simple_number() {
        assert_eq!(
            tokenize("42"),
            vec![Token::IntegerLiteral("42".to_string())]
        );
        assert_eq!(
            tokenize("2.5"),
            vec![Token::DecimalLiteral("2.5".to_string())]
        );
        assert_eq!(
            tokenize("1e10"),
            vec![Token::DoubleLiteral("1e10".to_string())]
        );
    }

    #[test]
    fn test_string_literal() {
        assert_eq!(
            tokenize("'hello'"),
            vec![Token::StringLiteral("hello".to_string())]
        );
        assert_eq!(
            tokenize("\"world\""),
            vec![Token::StringLiteral("world".to_string())]
        );
        assert_eq!(
            tokenize("'it''s'"),
            vec![Token::StringLiteral("it's".to_string())]
        );
    }

    #[test]
    fn test_variable() {
        assert_eq!(
            tokenize("$x"),
            vec![
                Token::Dollar,
                Token::VarName {
                    prefix: String::new(),
                    local: "x".to_string()
                }
            ]
        );
        assert_eq!(
            tokenize("$ns:var"),
            vec![
                Token::Dollar,
                Token::VarName {
                    prefix: "ns".to_string(),
                    local: "var".to_string()
                }
            ]
        );
    }

    #[test]
    fn test_axis_specifiers() {
        assert_eq!(tokenize("child::"), vec![Token::AxisChild]);
        assert_eq!(
            tokenize("ancestor-or-self::"),
            vec![Token::AxisAncestorOrSelf]
        );
        assert_eq!(
            tokenize("descendant-or-self::"),
            vec![Token::AxisDescendantOrSelf]
        );
    }

    #[test]
    fn test_path_expression() {
        let tokens = tokenize("/a/b");
        assert_eq!(
            tokens,
            vec![
                Token::Slash,
                Token::QName("a".to_string()),
                Token::Slash,
                Token::QName("b".to_string())
            ]
        );
    }

    #[test]
    fn test_double_slash() {
        let tokens = tokenize("//a");
        assert_eq!(
            tokens,
            vec![Token::DoubleSlash, Token::QName("a".to_string())]
        );
    }

    #[test]
    fn test_if_expression() {
        let tokens = tokenize("if (");
        assert_eq!(tokens, vec![Token::If, Token::LParen]);
    }

    #[test]
    fn test_for_expression() {
        let tokens = tokenize("for $x");
        assert_eq!(
            tokens,
            vec![
                Token::For,
                Token::Dollar,
                Token::VarName {
                    prefix: String::new(),
                    local: "x".to_string()
                }
            ]
        );
    }

    #[test]
    fn test_comparison_operators() {
        let tokens = tokenize("1 eq 2");
        assert_eq!(
            tokens,
            vec![
                Token::IntegerLiteral("1".to_string()),
                Token::Eq,
                Token::IntegerLiteral("2".to_string())
            ]
        );
    }

    #[test]
    fn test_instance_of() {
        let tokens = tokenize("$x instance of xs:integer");
        assert_eq!(
            tokens,
            vec![
                Token::Dollar,
                Token::VarName {
                    prefix: String::new(),
                    local: "x".to_string()
                },
                Token::InstanceOf,
                Token::QName("xs:integer".to_string())
            ]
        );
    }

    #[test]
    fn test_cast_as() {
        let tokens = tokenize("$x cast as xs:string");
        assert_eq!(
            tokens,
            vec![
                Token::Dollar,
                Token::VarName {
                    prefix: String::new(),
                    local: "x".to_string()
                },
                Token::CastAs,
                Token::QName("xs:string".to_string())
            ]
        );
    }

    #[test]
    fn test_kind_test() {
        let tokens = tokenize("node()");
        assert_eq!(tokens, vec![Token::Node, Token::LParen, Token::RParen]);
    }

    #[test]
    fn test_element_test() {
        let tokens = tokenize("element(foo)");
        assert_eq!(
            tokens,
            vec![
                Token::Element,
                Token::LParen,
                Token::QName("foo".to_string()),
                Token::RParen
            ]
        );
    }

    #[test]
    fn test_element_test_with_type() {
        // element(name, type)
        let tokens = tokenize("element(foo, xs:string)");
        assert_eq!(
            tokens,
            vec![
                Token::Element,
                Token::LParen,
                Token::QName("foo".to_string()),
                Token::Comma,
                Token::QName("xs:string".to_string()),
                Token::RParen
            ]
        );
    }

    #[test]
    fn test_element_test_with_nillable_type() {
        // element(name, type?) - nillable type syntax
        let tokens = tokenize("element(foo, xs:string?)");
        assert_eq!(
            tokens,
            vec![
                Token::Element,
                Token::LParen,
                Token::QName("foo".to_string()),
                Token::Comma,
                Token::QNameNillable("xs:string".to_string()),
                Token::RParen
            ]
        );
    }

    #[test]
    fn test_element_test_nillable_with_whitespace() {
        // Whitespace between type and "?" should still produce QNameNillable
        let tokens = tokenize("element(foo, xs:string ?)");
        assert_eq!(
            tokens,
            vec![
                Token::Element,
                Token::LParen,
                Token::QName("foo".to_string()),
                Token::Comma,
                Token::QNameNillable("xs:string".to_string()),
                Token::RParen
            ]
        );
    }

    #[test]
    fn test_comments() {
        let tokens = tokenize("1 (: comment :) + 2");
        assert_eq!(
            tokens,
            vec![
                Token::IntegerLiteral("1".to_string()),
                Token::Plus,
                Token::IntegerLiteral("2".to_string())
            ]
        );
    }

    #[test]
    fn test_nested_comments() {
        let tokens = tokenize("1 (: outer (: inner :) outer :) + 2");
        assert_eq!(
            tokens,
            vec![
                Token::IntegerLiteral("1".to_string()),
                Token::Plus,
                Token::IntegerLiteral("2".to_string())
            ]
        );
    }

    #[test]
    fn test_arithmetic() {
        let tokens = tokenize("1 + 2 * 3");
        assert_eq!(
            tokens,
            vec![
                Token::IntegerLiteral("1".to_string()),
                Token::Plus,
                Token::IntegerLiteral("2".to_string()),
                Token::Star,
                Token::IntegerLiteral("3".to_string())
            ]
        );
    }

    #[test]
    fn test_predicates() {
        let tokens = tokenize("a[1]");
        assert_eq!(
            tokens,
            vec![
                Token::QName("a".to_string()),
                Token::LBracket,
                Token::IntegerLiteral("1".to_string()),
                Token::RBracket
            ]
        );
    }

    #[test]
    fn test_double_period() {
        let tokens = tokenize("..");
        assert_eq!(tokens, vec![Token::DoublePeriod]);
    }

    #[test]
    fn test_context_item() {
        let tokens = tokenize(".");
        assert_eq!(tokens, vec![Token::Dot]);
    }

    #[test]
    fn test_slash_only() {
        // "/" alone should emit SlashOnly
        let tokens = tokenize("/");
        assert_eq!(tokens, vec![Token::SlashOnly]);
    }

    #[test]
    fn test_slash_only_with_trailing_whitespace() {
        // "/" followed by whitespace and nothing else should emit SlashOnly
        let tokens = tokenize("/  ");
        assert_eq!(tokens, vec![Token::SlashOnly]);
    }

    #[test]
    fn test_slash_only_with_comment() {
        // "/" followed by a comment and nothing else should emit SlashOnly
        let tokens = tokenize("/ (: comment :)");
        assert_eq!(tokens, vec![Token::SlashOnly]);
    }

    #[test]
    fn test_slash_with_path() {
        // "/" followed by a name should emit Slash (not SlashOnly)
        let tokens = tokenize("/a");
        assert_eq!(tokens, vec![Token::Slash, Token::QName("a".to_string())]);
    }

    #[test]
    fn test_slash_with_whitespace_then_path() {
        // "/" followed by whitespace then a name should emit Slash
        let tokens = tokenize("/ a");
        assert_eq!(tokens, vec![Token::Slash, Token::QName("a".to_string())]);
    }

    #[test]
    fn test_slash_with_comment_then_path() {
        // "/" followed by comment then a name should emit Slash
        let tokens = tokenize("/ (: comment :) a");
        assert_eq!(tokens, vec![Token::Slash, Token::QName("a".to_string())]);
    }

    #[test]
    fn test_attribute_shorthand() {
        let tokens = tokenize("@id");
        assert_eq!(tokens, vec![Token::At, Token::QName("id".to_string())]);
    }

    #[test]
    fn test_wildcard() {
        let tokens = tokenize("*");
        assert_eq!(tokens, vec![Token::Star]);
    }

    #[test]
    fn test_namespace_wildcard() {
        let tokens = tokenize("*:local");
        assert_eq!(
            tokens,
            vec![
                Token::Star,
                Token::Colon,
                Token::NCName("local".to_string())
            ]
        );
    }

    #[test]
    fn test_occurrence_indicators() {
        let tokens = tokenize("$x instance of xs:integer?");
        assert!(tokens.contains(&Token::OccurrenceZeroOrOne));
    }

    // ---- XPath 1.0 mode tests ----

    fn tokenize_10(input: &str) -> Vec<Token> {
        Lexer::new_with_mode(input, XPathMode::XPath10)
            .map(|r| r.unwrap().1)
            .filter(|t| *t != Token::Eof)
            .collect()
    }

    #[test]
    fn test_xpath10_if_as_element_name() {
        // //if → DoubleSlash, QName("if")
        let tokens = tokenize_10("//if");
        assert_eq!(tokens, vec![Token::DoubleSlash, Token::QName("if".into())]);
    }

    #[test]
    fn test_xpath10_for_as_element_name() {
        let tokens = tokenize_10("//for");
        assert_eq!(tokens, vec![Token::DoubleSlash, Token::QName("for".into())]);
    }

    #[test]
    fn test_xpath10_every_as_element_name() {
        let tokens = tokenize_10("//every");
        assert_eq!(
            tokens,
            vec![Token::DoubleSlash, Token::QName("every".into())]
        );
    }

    #[test]
    fn test_xpath10_some_as_element_name() {
        let tokens = tokenize_10("//some");
        assert_eq!(
            tokens,
            vec![Token::DoubleSlash, Token::QName("some".into())]
        );
    }

    #[test]
    fn test_xpath10_xpath20_keywords_as_qnames() {
        // Various XPath 2.0-only keywords used as element names in 1.0 mode
        for name in &[
            "then",
            "else",
            "return",
            "to",
            "union",
            "except",
            "intersect",
            "eq",
            "ne",
            "lt",
            "le",
            "gt",
            "ge",
            "is",
            "idiv",
            "satisfies",
            "in",
        ] {
            let input = format!("a/{}", name);
            let tokens = tokenize_10(&input);
            assert_eq!(
                tokens,
                vec![
                    Token::QName("a".into()),
                    Token::Slash,
                    Token::QName((*name).into()),
                ],
                "Failed for keyword: {}",
                name
            );
        }
    }

    #[test]
    fn test_xpath10_unary_minus() {
        let tokens = tokenize_10("-a");
        assert_eq!(tokens, vec![Token::Minus10, Token::QName("a".into())]);
    }

    #[test]
    fn test_xpath10_unary_plus() {
        let tokens = tokenize_10("+a");
        assert_eq!(tokens, vec![Token::Plus10, Token::QName("a".into())]);
    }

    #[test]
    fn test_xpath20_unary_minus() {
        let tokens = tokenize("-a");
        assert_eq!(tokens, vec![Token::Minus20, Token::QName("a".into())]);
    }

    #[test]
    fn test_xpath10_binary_minus() {
        let tokens = tokenize_10("a - b");
        assert_eq!(
            tokens,
            vec![
                Token::QName("a".into()),
                Token::Minus,
                Token::QName("b".into())
            ]
        );
    }

    #[test]
    fn test_xpath10_binary_plus() {
        let tokens = tokenize_10("a + b");
        assert_eq!(
            tokens,
            vec![
                Token::QName("a".into()),
                Token::Plus,
                Token::QName("b".into())
            ]
        );
    }

    #[test]
    fn test_xpath10_and_or_div_mod() {
        let tokens = tokenize_10("a and b");
        assert_eq!(
            tokens,
            vec![
                Token::QName("a".into()),
                Token::And,
                Token::QName("b".into())
            ]
        );
        let tokens = tokenize_10("a or b");
        assert_eq!(
            tokens,
            vec![
                Token::QName("a".into()),
                Token::Or,
                Token::QName("b".into())
            ]
        );
        let tokens = tokenize_10("a div b");
        assert_eq!(
            tokens,
            vec![
                Token::QName("a".into()),
                Token::Div,
                Token::QName("b".into())
            ]
        );
        let tokens = tokenize_10("a mod b");
        assert_eq!(
            tokens,
            vec![
                Token::QName("a".into()),
                Token::Mod,
                Token::QName("b".into())
            ]
        );
    }

    #[test]
    fn test_xpath10_node_kind_test() {
        let tokens = tokenize_10("node()");
        assert_eq!(tokens, vec![Token::Node, Token::LParen, Token::RParen]);
    }

    #[test]
    fn test_xpath10_comment_kind_test() {
        let tokens = tokenize_10("comment()");
        assert_eq!(tokens, vec![Token::Comment, Token::LParen, Token::RParen]);
    }

    #[test]
    fn test_xpath10_text_kind_test() {
        let tokens = tokenize_10("text()");
        assert_eq!(tokens, vec![Token::Text, Token::LParen, Token::RParen]);
    }

    #[test]
    fn test_xpath10_processing_instruction_kind_test() {
        let tokens = tokenize_10("processing-instruction()");
        assert_eq!(
            tokens,
            vec![Token::ProcessingInstruction, Token::LParen, Token::RParen]
        );
    }

    #[test]
    fn test_xpath10_element_as_function() {
        // element(foo) in 1.0 → QName("element"), LParen, QName("foo"), RParen
        let tokens = tokenize_10("element(foo)");
        assert_eq!(
            tokens,
            vec![
                Token::QName("element".into()),
                Token::LParen,
                Token::QName("foo".into()),
                Token::RParen
            ]
        );
    }

    #[test]
    fn test_xpath10_axis_specifiers() {
        let tokens = tokenize_10("child::*");
        assert_eq!(tokens, vec![Token::AxisChild, Token::Star]);
    }

    #[test]
    fn test_xpath10_ancestor_or_self_axis() {
        let tokens = tokenize_10("ancestor-or-self::node()");
        assert_eq!(
            tokens,
            vec![
                Token::AxisAncestorOrSelf,
                Token::Node,
                Token::LParen,
                Token::RParen
            ]
        );
    }

    #[test]
    fn test_xpath10_no_double_less() {
        // In XPath 1.0, << should not produce DoubleLess
        let tokens: Vec<Token> = Lexer::new_with_mode("a<<b", XPathMode::XPath10)
            .filter_map(|r| r.ok().map(|t| t.1))
            .filter(|t| *t != Token::Eof)
            .collect();
        assert!(!tokens.contains(&Token::DoubleLess));
    }

    #[test]
    fn test_xpath10_no_double_greater() {
        // In XPath 1.0, >> should not produce DoubleGreater
        let tokens: Vec<Token> = Lexer::new_with_mode("a>>b", XPathMode::XPath10)
            .filter_map(|r| r.ok().map(|t| t.1))
            .filter(|t| *t != Token::Eof)
            .collect();
        assert!(!tokens.contains(&Token::DoubleGreater));
    }

    #[test]
    fn test_xpath20_double_less_still_works() {
        let tokens = tokenize("a<<b");
        assert!(tokens.contains(&Token::DoubleLess));
    }

    #[test]
    fn test_xpath20_double_greater_still_works() {
        let tokens = tokenize("a>>b");
        assert!(tokens.contains(&Token::DoubleGreater));
    }
    /// `for`, `some` and `every` are not reserved words: they are only
    /// keywords when a variable follows.
    #[test]
    fn the_quantifier_keywords_are_also_ordinary_names() {
        assert_eq!(
            tokenize("@for"),
            vec![Token::At, Token::QName("for".to_string())]
        );
        assert_eq!(tokenize("some:a"), vec![Token::QName("some:a".to_string())]);
        assert_eq!(tokenize("every"), vec![Token::QName("every".to_string())]);
        // With a variable they are still keywords.
        assert_eq!(tokenize("for $i in 1 return $i")[0], Token::For);
        assert_eq!(tokenize("some $i in 1 satisfies $i")[0], Token::Some);
        assert_eq!(tokenize("every $i in 1 satisfies $i")[0], Token::Every);
    }

    /// `NCName ":" "*"` is the second alternative of the Wildcard production.
    #[test]
    fn prefix_wildcard_lexes_as_three_tokens() {
        let tokens = tokenize("bar:*");
        assert_eq!(
            tokens,
            vec![Token::NCName("bar".to_string()), Token::Colon, Token::Star]
        );
        // It is still a wildcard after an axis and inside a path.
        assert_eq!(
            tokenize("child::bar:*"),
            vec![
                Token::AxisChild,
                Token::NCName("bar".to_string()),
                Token::Colon,
                Token::Star
            ]
        );
        // A plain QName is unaffected.
        assert_eq!(
            tokenize("bar:baz"),
            vec![Token::QName("bar:baz".to_string())]
        );
    }

    /// XPath 2.0 §3.1.1: the value of a string literal is the characters
    /// between the delimiters, a doubled delimiter standing for one. XML
    /// un-escaping is the host language's job and has already happened by the
    /// time the expression text reaches the processor.
    mod string_literals_are_not_re_decoded {
        use super::*;

        fn literal(input: &str) -> String {
            match &tokenize(input)[0] {
                Token::StringLiteral(s) => s.clone(),
                other => panic!("expected a string literal, got {other:?}"),
            }
        }

        #[test]
        fn entity_references_are_left_alone() {
            assert_eq!(literal("'a&amp;b'"), "a&amp;b");
            assert_eq!(literal("'&#13;'"), "&#13;");
            assert_eq!(literal("'&lt;&gt;'"), "&lt;&gt;");
        }

        #[test]
        fn a_bare_ampersand_is_an_ordinary_character() {
            // Neither an unterminated nor an unknown reference is an error:
            // there are no references in an XPath string literal at all.
            assert_eq!(literal("'a&b'"), "a&b");
            assert_eq!(literal("'a&foo;b'"), "a&foo;b");
            assert_eq!(literal("'&'"), "&");
        }

        #[test]
        fn carriage_returns_survive() {
            assert_eq!(literal("'a\rb'"), "a\rb");
            assert_eq!(literal("'a\r\nb'"), "a\r\nb");
        }

        #[test]
        fn a_doubled_delimiter_still_stands_for_one() {
            assert_eq!(literal("'it''s'"), "it's");
            assert_eq!(literal("\"say \"\"hi\"\"\""), "say \"hi\"");
        }
    }

    /// XPath 2.0 §A.2.1:
    /// `DecimalLiteral ::= ("." Digits) | (Digits "." [0-9]*)`.
    #[test]
    fn decimal_literal_may_end_with_the_period() {
        assert_eq!(
            tokenize("5."),
            vec![Token::DecimalLiteral("5.".to_string())]
        );
        assert_eq!(
            tokenize("123."),
            vec![Token::DecimalLiteral("123.".to_string())]
        );
        assert_eq!(
            tokenize("5. + 1"),
            vec![
                Token::DecimalLiteral("5.".to_string()),
                Token::Plus,
                Token::IntegerLiteral("1".to_string())
            ]
        );
        // The other two forms are unchanged.
        assert_eq!(
            tokenize(".5"),
            vec![Token::DecimalLiteral(".5".to_string())]
        );
        assert_eq!(
            tokenize("5.25"),
            vec![Token::DecimalLiteral("5.25".to_string())]
        );
        // A lone "." is still the context item, and ".." the parent step.
        assert_eq!(tokenize("."), vec![Token::Dot]);
        assert_eq!(tokenize(".."), vec![Token::DoublePeriod]);
    }

    /// XML 1.0 §2.3 `NameStartChar` / `NameChar`, minus `":"`.
    #[test]
    fn ncname_uses_the_xml_name_productions() {
        // U+00B5 MICRO SIGN is Unicode `Alphabetic` but is below the first
        // NameStartChar range (#xC0), so it cannot start a name.
        assert!(!Lexer::is_ncname_start('\u{B5}'));
        assert!(!Lexer::is_ncname_char('\u{B5}'));
        // …nor can U+00D7 MULTIPLICATION SIGN or U+00F7 DIVISION SIGN, the two
        // holes in the #xC0-#xF6 / #xD8-#xF6 ranges.
        assert!(!Lexer::is_ncname_start('\u{D7}'));
        assert!(!Lexer::is_ncname_start('\u{F7}'));

        for c in ['a', 'Z', '_', '\u{C0}', '\u{D8}', '\u{F8}', '\u{3001}'] {
            assert!(Lexer::is_ncname_start(c), "{c:?} should start a name");
        }
        // ":" is excluded: an NCName is colon-free.
        assert!(!Lexer::is_ncname_start(':'));
        assert!(!Lexer::is_ncname_char(':'));

        // NameChar adds the ASCII digits, "-", ".", and three combining ranges.
        for c in ['0', '9', '-', '.', '\u{B7}', '\u{300}', '\u{203F}'] {
            assert!(Lexer::is_ncname_char(c), "{c:?} should continue a name");
            assert!(!Lexer::is_ncname_start(c), "{c:?} should not start a name");
        }

        // The lexer follows suit.
        assert_eq!(tokenize("a-b.c0"), vec![Token::QName("a-b.c0".to_string())]);
    }
}
