//! XPath 2.0 regex functions.
//!
//! This module implements:
//! - fn:matches($input, $pattern, $flags?) - test if string matches pattern
//! - fn:replace($input, $pattern, $replacement, $flags?) - replace matches
//! - fn:tokenize($input, $pattern, $flags?) - split string by pattern
//!
//! Uses the `regexml` engine (the `regexml-fork` crate) in its XPath 2.0
//! dialect, `Regex::xpath20`: the regular expressions of F&O 1.0 Second
//! Edition §7.6.1, with full Unicode support.
//!
//! All three compile their `$pattern` and `$flags` arguments through one private
//! helper, which reuses the program of a `(pattern, flags)` pair it has already
//! compiled during the current evaluation run — or, when the host lent the run a
//! [`RegexCache`](crate::xpath::RegexCache), during any run that used that
//! cache. A pattern that does not change — a literal in a predicate, say — is
//! therefore compiled once, however many items the predicate is evaluated for.
//! The reuse is invisible: a compiled `regexml::Regex` is immutable and builds a
//! fresh matcher for every call, and a compile that fails yields the same error
//! every time.

use std::ops::Deref;
use std::sync::Arc;

use regexml::{ErrorKind, Regex};

use crate::xpath::context::DynamicContext;
use crate::xpath::error::XPathError;
use crate::xpath::DomNavigator;

use super::{atomize_to_string, atomize_to_string_opt, atomize_to_string_required, XPathValue};
use crate::types::value::XmlValue;
use crate::xpath::iterator::XmlItem;

/// fn:matches($input as xs:string?, $pattern as xs:string, $flags as xs:string?) as xs:boolean
///
/// Returns true if $input matches the regular expression $pattern.
///
/// - If $input is empty, it is treated as empty string.
/// - FORX0001 if $flags contains invalid characters.
/// - FORX0002 if $pattern is not a valid regular expression.
pub fn matches<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(XPathError::wrong_number_of_arguments(
            "matches",
            2,
            args.len(),
        ));
    }

    // Get flags (optional third argument)
    let flags = if args.len() == 3 {
        atomize_to_string_opt(args.pop().unwrap())?
    } else {
        None
    };

    // Get pattern (second argument)
    let pattern = atomize_to_string_required(args.pop().unwrap())?;

    // Get input (first argument)
    let input = atomize_to_string(args.pop().unwrap())?;

    let flags_str = flags.as_deref().unwrap_or("");

    // Build the regex
    let regex = build_regex(context, &pattern, flags_str)?;

    let result = regex.is_match(&input);

    Ok(XPathValue::boolean(result))
}

/// fn:replace($input as xs:string?, $pattern as xs:string, $replacement as xs:string,
///            $flags as xs:string?) as xs:string
///
/// Replaces all occurrences of $pattern in $input with $replacement.
///
/// - FORX0001 if $flags contains invalid characters.
/// - FORX0002 if $pattern is not a valid regular expression.
/// - FORX0003 if $pattern matches a zero-length string.
/// - FORX0004 if $replacement has invalid syntax.
pub fn replace<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() < 3 || args.len() > 4 {
        return Err(XPathError::wrong_number_of_arguments(
            "replace",
            3,
            args.len(),
        ));
    }

    // Get flags (optional fourth argument)
    let flags = if args.len() == 4 {
        atomize_to_string_opt(args.pop().unwrap())?
    } else {
        None
    };

    // Get replacement (third argument)
    let replacement = atomize_to_string_required(args.pop().unwrap())?;

    // Get pattern (second argument)
    let pattern = atomize_to_string_required(args.pop().unwrap())?;

    // Get input (first argument)
    let input = atomize_to_string(args.pop().unwrap())?;

    // FORX0001 / FORX0002.
    let regex = build_regex(context, &pattern, flags.as_deref().unwrap_or(""))?;

    // FORX0003, before anything is matched: the rule is about `$pattern`
    // alone, so it applies whatever `$input` is — an empty one included.
    if matches_zero_length_string(&regex) {
        return Err(XPathError::regex_matches_zero_length(&pattern));
    }

    // FORX0004. `$replacement` is a pure syntax rule, so it too applies
    // whatever `$input` is. The backend only validates the replacement while
    // it walks the matches, so a pattern that matches nothing — or an empty
    // input, which it never walks at all — would otherwise let a malformed
    // replacement string through.
    check_replacement_string(&replacement)?;

    let result = regex
        .replace_all(&input, &replacement)
        .map_err(|e| match e.kind() {
            ErrorKind::MatchesEmptyString => XPathError::regex_matches_zero_length(&pattern),
            ErrorKind::InvalidReplacement => XPathError::invalid_replacement_string(&replacement),
            _ => XPathError::invalid_regex_pattern(&pattern),
        })?;

    Ok(XPathValue::string(result))
}

/// fn:tokenize($input as xs:string?, $pattern as xs:string, $flags as xs:string?) as xs:string*
///
/// Splits $input into a sequence of strings using $pattern as delimiter.
///
/// - FORX0001 if $flags contains invalid characters.
/// - FORX0002 if $pattern is not a valid regular expression.
/// - FORX0003 if $pattern matches a zero-length string.
pub fn tokenize<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(XPathError::wrong_number_of_arguments(
            "tokenize",
            2,
            args.len(),
        ));
    }

    // Get flags (optional third argument)
    let flags = if args.len() == 3 {
        atomize_to_string_opt(args.pop().unwrap())?
    } else {
        None
    };

    // Get pattern (second argument)
    let pattern = atomize_to_string_required(args.pop().unwrap())?;

    // Get input (first argument)
    let input = atomize_to_string(args.pop().unwrap())?;

    // FORX0001 / FORX0002, then FORX0003, and only then the empty-input
    // shortcut. F&O §7.6.4 states the three errors with no exemption for any
    // input, and states separately that the result is the empty sequence when
    // `$input` is the empty sequence or the zero-length string. Compiling
    // first costs nothing: the program is cached (see `build_regex`).
    let regex = build_regex(context, &pattern, flags.as_deref().unwrap_or(""))?;
    if matches_zero_length_string(&regex) {
        return Err(XPathError::regex_matches_zero_length(&pattern));
    }

    if input.is_empty() {
        return Ok(XPathValue::Empty);
    }

    let token_iter = regex.tokenize(&input).map_err(|e| match e.kind() {
        ErrorKind::MatchesEmptyString => XPathError::regex_matches_zero_length(&pattern),
        _ => XPathError::invalid_regex_pattern(&pattern),
    })?;

    // Every gap between two adjacent separators is a token, including the
    // zero-length ones produced by a leading separator, a trailing separator
    // and two adjacent separators.
    let items: Vec<XmlItem<N>> = token_iter
        .map(|s| XmlItem::Atomic(XmlValue::string(&s)))
        .collect();

    Ok(XPathValue::from_sequence(items))
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Whether a compiled pattern matches the zero-length string — the FORX0003
/// condition of `fn:replace` (F&O §7.6.3) and `fn:tokenize` (F&O §7.6.4).
///
/// The specification states the condition as `matches("", $pattern, $flags)`
/// being true, and that is literally what this is. The backend computes the
/// same predicate once when it compiles the program, so asking it here does
/// not re-run the matcher over anything but the empty string.
///
/// It is asked explicitly rather than left to the backend because the
/// backend's own `tokenize` short-circuits on an empty haystack *before* it
/// checks, so an empty `$input` would otherwise hide the error.
fn matches_zero_length_string(regex: &Regex) -> bool {
    regex.is_match("")
}

/// Validate a `fn:replace` `$replacement` string, raising FORX0004 (F&O
/// §7.6.3) when it is malformed.
///
/// Two rules, applied left to right: a `\` must be followed by a `\` or a `$`,
/// and a `$` that is not part of such a pair must be followed by a digit.
///
/// The backend applies the same two rules, but only while it walks the
/// matches, so a `$replacement` is never looked at when the input is empty or
/// when the pattern matches nothing. The rule is a property of `$replacement`
/// alone, so it is checked here for every call.
fn check_replacement_string(replacement: &str) -> Result<(), XPathError> {
    let chars: Vec<char> = replacement.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '\\' => {
                if !matches!(chars.get(i + 1), Some('\\' | '$')) {
                    return Err(XPathError::invalid_replacement_string(replacement));
                }
                i += 2;
            }
            '$' => {
                if !matches!(chars.get(i + 1), Some(c) if c.is_ascii_digit()) {
                    return Err(XPathError::invalid_replacement_string(replacement));
                }
                i += 2;
            }
            _ => i += 1,
        }
    }
    Ok(())
}

/// A compiled program, borrowed from the run's own cache or shared out of the
/// host's [`RegexCache`](crate::xpath::RegexCache).
enum CachedRegex<'run> {
    Run(&'run Regex),
    Host(Arc<Regex>),
}

impl Deref for CachedRegex<'_> {
    type Target = Regex;

    fn deref(&self) -> &Regex {
        match self {
            CachedRegex::Run(regex) => regex,
            CachedRegex::Host(regex) => regex,
        }
    }
}

/// The Regex for an XPath pattern and flags, compiled at most once per
/// `(pattern, flags)` pair per evaluation run — or per host cache.
///
/// The program is compiled by [`compile_regex`] the first time it is asked for
/// and is kept afterwards: in the host's
/// [`RegexCache`](crate::xpath::RegexCache) when the host lent this run one, and
/// otherwise in the run's own [`regex_cache`](crate::xpath::regex_cache). So a
/// pattern that does not change — a literal in a predicate, say — is compiled
/// once however many items the predicate is evaluated for, and, with a host
/// cache, however many runs evaluate it. Reuse is invisible: a compiled
/// `regexml::Regex` is immutable and builds a fresh matcher for each call, and a
/// compile that fails yields the same error every time because that error is
/// built from the pattern and the flags alone.
fn build_regex<'run, N: DomNavigator>(
    context: &'run mut DynamicContext<'_, N>,
    pattern: &str,
    flags: &str,
) -> Result<CachedRegex<'run>, XPathError> {
    match context.host_regex_cache() {
        Some(cache) => cache
            .get_or_compile(pattern, flags, || compile_regex(pattern, flags))
            .map(CachedRegex::Host),
        None => context
            .regex_cache_mut()
            .get_or_compile(pattern, flags, || compile_regex(pattern, flags))
            .map(CachedRegex::Run),
    }
}

/// Build a Regex from an XPath pattern and flags, in the XPath 2.0 dialect.
///
/// `Regex::xpath20` is the regular-expression language of F&O 1.0 Second
/// Edition §7.6.1: XML Schema's syntax (character class subtraction, `\i`,
/// `\c`, `\p{…}`) with `^`, `$`, back-references and reluctant quantifiers,
/// but without what F&O 3.0 added — the flags are exactly `s`, `m`, `i` and
/// `x` (§7.6.1.1: "If there are characters present that are not defined here
/// as flags, then an error is raised \[err:FORX0001\]"), there is no `(?…)`
/// group, and without `s` the meta-character `.` "matches any character
/// except a newline (#x0A) character" (the later rule also excludes #xD).
///
/// FORX0001 for invalid flags (checked first); FORX0002 for everything else
/// the engine refuses — a syntax error, an unknown block name (F&O 1.0 takes
/// XML Schema 1.0's list of block names), and a pattern that nests groups and
/// character classes more than 64 levels deep (`ErrorKind::NestingTooDeep`:
/// the engine's limit, which keeps compiling and matching inside the stack).
fn compile_regex(pattern: &str, flags: &str) -> Result<Regex, XPathError> {
    Regex::xpath20(pattern, flags).map_err(|e| match e.kind() {
        ErrorKind::InvalidFlags => XPathError::invalid_regex_flags(flags),
        _ => XPathError::invalid_regex_pattern(pattern),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespace::table::NameTable;
    use crate::xpath::context::XPathContext;
    use crate::xpath::RoXmlNavigator;

    fn create_context<'a>(names: &'a NameTable) -> DynamicContext<'a, RoXmlNavigator<'a>> {
        let static_ctx = XPathContext::new(names);
        let static_ctx = Box::leak(Box::new(static_ctx));
        DynamicContext::new(static_ctx, 0)
    }

    #[test]
    fn test_matches_basic() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        let result = matches(
            &mut ctx,
            vec![XPathValue::string("abracadabra"), XPathValue::string("bra")],
        )
        .unwrap();

        assert!(
            matches!(result, XPathValue::Item(XmlItem::Atomic(v)) if v.as_boolean() == Some(true))
        );
    }

    #[test]
    fn test_matches_no_match() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        let result = matches(
            &mut ctx,
            vec![XPathValue::string("abracadabra"), XPathValue::string("xyz")],
        )
        .unwrap();

        assert!(
            matches!(result, XPathValue::Item(XmlItem::Atomic(v)) if v.as_boolean() == Some(false))
        );
    }

    #[test]
    fn test_matches_case_insensitive() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        let result = matches(
            &mut ctx,
            vec![
                XPathValue::string("HELLO"),
                XPathValue::string("hello"),
                XPathValue::string("i"),
            ],
        )
        .unwrap();

        assert!(
            matches!(result, XPathValue::Item(XmlItem::Atomic(v)) if v.as_boolean() == Some(true))
        );
    }

    #[test]
    fn test_matches_multiline() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        let result = matches(
            &mut ctx,
            vec![
                XPathValue::string("line1\nline2"),
                XPathValue::string("^line2"),
                XPathValue::string("m"),
            ],
        )
        .unwrap();

        assert!(
            matches!(result, XPathValue::Item(XmlItem::Atomic(v)) if v.as_boolean() == Some(true))
        );
    }

    #[test]
    fn test_matches_multiline_empty_line_trailing_newline() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        let result = matches(
            &mut ctx,
            vec![
                XPathValue::string("abcd\ndefg\n"),
                XPathValue::string("^$"),
                XPathValue::string("m"),
            ],
        )
        .unwrap();

        assert!(
            matches!(result, XPathValue::Item(XmlItem::Atomic(v)) if v.as_boolean() == Some(false))
        );
    }

    #[test]
    fn test_matches_multiline_empty_line_in_middle() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        let result = matches(
            &mut ctx,
            vec![
                XPathValue::string("abcd\n\ndefg\n"),
                XPathValue::string("^$"),
                XPathValue::string("m"),
            ],
        )
        .unwrap();

        assert!(
            matches!(result, XPathValue::Item(XmlItem::Atomic(v)) if v.as_boolean() == Some(true))
        );
    }

    #[test]
    fn test_matches_class_subtraction_with_i_flag() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        let match_x = matches(
            &mut ctx,
            vec![
                XPathValue::string("X"),
                XPathValue::string("[A-Z-[OI]]"),
                XPathValue::string("i"),
            ],
        )
        .unwrap();
        assert!(
            matches!(match_x, XPathValue::Item(XmlItem::Atomic(v)) if v.as_boolean() == Some(true))
        );

        let match_o = matches(
            &mut ctx,
            vec![
                XPathValue::string("O"),
                XPathValue::string("[A-Z-[OI]]"),
                XPathValue::string("i"),
            ],
        )
        .unwrap();
        assert!(
            matches!(match_o, XPathValue::Item(XmlItem::Atomic(v)) if v.as_boolean() == Some(false))
        );

        let match_i = matches(
            &mut ctx,
            vec![
                XPathValue::string("i"),
                XPathValue::string("[A-Z-[OI]]"),
                XPathValue::string("i"),
            ],
        )
        .unwrap();
        assert!(
            matches!(match_i, XPathValue::Item(XmlItem::Atomic(v)) if v.as_boolean() == Some(false))
        );
    }

    #[test]
    fn test_matches_unicode_categories_with_i_flag() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        let upper = matches(
            &mut ctx,
            vec![
                XPathValue::string("m"),
                XPathValue::string(r"\p{Lu}"),
                XPathValue::string("i"),
            ],
        )
        .unwrap();
        assert!(
            matches!(upper, XPathValue::Item(XmlItem::Atomic(v)) if v.as_boolean() == Some(false))
        );

        let not_upper = matches(
            &mut ctx,
            vec![
                XPathValue::string("m"),
                XPathValue::string(r"\P{Lu}"),
                XPathValue::string("i"),
            ],
        )
        .unwrap();
        assert!(
            matches!(not_upper, XPathValue::Item(XmlItem::Atomic(v)) if v.as_boolean() == Some(true))
        );
    }

    #[test]
    fn test_matches_invalid_flags() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        let result = matches(
            &mut ctx,
            vec![
                XPathValue::string("test"),
                XPathValue::string("test"),
                XPathValue::string("z"),
            ],
        );

        assert!(matches!(result, Err(XPathError::FORX0001 { .. })));
    }

    #[test]
    fn test_matches_invalid_pattern() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        let result = matches(
            &mut ctx,
            vec![XPathValue::string("test"), XPathValue::string("[invalid")],
        );

        assert!(matches!(result, Err(XPathError::FORX0002 { .. })));
    }

    #[test]
    fn test_replace_basic() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        let result = replace(
            &mut ctx,
            vec![
                XPathValue::string("abracadabra"),
                XPathValue::string("a"),
                XPathValue::string("X"),
            ],
        )
        .unwrap();

        if let XPathValue::Item(XmlItem::Atomic(v)) = result {
            assert_eq!(v.as_string(), Some("XbrXcXdXbrX"));
        } else {
            panic!("Expected string");
        }
    }

    #[test]
    fn test_replace_with_groups() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        let result = replace(
            &mut ctx,
            vec![
                XPathValue::string("hello world"),
                XPathValue::string("([a-z]+) ([a-z]+)"),
                XPathValue::string("$2 $1"),
            ],
        )
        .unwrap();

        if let XPathValue::Item(XmlItem::Atomic(v)) = result {
            assert_eq!(v.as_string(), Some("world hello"));
        } else {
            panic!("Expected string");
        }
    }

    #[test]
    fn test_replace_zero_length_match() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        let result = replace(
            &mut ctx,
            vec![
                XPathValue::string("test"),
                XPathValue::string("a?"),
                XPathValue::string("X"),
            ],
        );

        assert!(matches!(result, Err(XPathError::FORX0003 { .. })));
    }

    #[test]
    fn test_replace_invalid_replacement() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        // $ not followed by digit or $
        let result = replace(
            &mut ctx,
            vec![
                XPathValue::string("test"),
                XPathValue::string("t"),
                XPathValue::string("$x"),
            ],
        );

        assert!(matches!(result, Err(XPathError::FORX0004 { .. })));
    }

    #[test]
    fn test_tokenize_basic() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        let result = tokenize(
            &mut ctx,
            vec![XPathValue::string("a,b,c"), XPathValue::string(",")],
        )
        .unwrap();

        match result {
            XPathValue::Sequence(items) => {
                assert_eq!(items.len(), 3);
                let strs: Vec<String> = items
                    .iter()
                    .map(|item| {
                        if let XmlItem::Atomic(v) = item {
                            v.to_string_value()
                        } else {
                            panic!("Expected atomic")
                        }
                    })
                    .collect();
                assert_eq!(strs, vec!["a", "b", "c"]);
            }
            _ => panic!("Expected sequence"),
        }
    }

    #[test]
    fn test_tokenize_whitespace() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        let result = tokenize(
            &mut ctx,
            vec![
                XPathValue::string("red   green   blue"),
                XPathValue::string("\\s+"),
            ],
        )
        .unwrap();

        match result {
            XPathValue::Sequence(items) => {
                assert_eq!(items.len(), 3);
            }
            _ => panic!("Expected sequence"),
        }
    }

    #[test]
    fn test_tokenize_empty_input() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        let result = tokenize(
            &mut ctx,
            vec![XPathValue::string(""), XPathValue::string(",")],
        )
        .unwrap();

        assert!(matches!(result, XPathValue::Empty));
    }

    /// Collect a tokenize() result as plain strings.
    fn token_strings(value: XPathValue<RoXmlNavigator<'_>>) -> Vec<String> {
        match value {
            XPathValue::Empty => Vec::new(),
            XPathValue::Item(XmlItem::Atomic(v)) => vec![v.to_string_value()],
            XPathValue::Sequence(items) => items
                .iter()
                .map(|item| {
                    if let XmlItem::Atomic(v) = item {
                        v.to_string_value()
                    } else {
                        panic!("Expected atomic")
                    }
                })
                .collect(),
            _ => panic!("Expected a sequence of atomic values"),
        }
    }

    fn tokenize_strings(input: &str, pattern: &str) -> Vec<String> {
        let names = NameTable::new();
        let mut ctx = create_context(&names);
        let result = tokenize(
            &mut ctx,
            vec![XPathValue::string(input), XPathValue::string(pattern)],
        )
        .unwrap();
        token_strings(result)
    }

    #[test]
    fn test_tokenize_leading_separator_keeps_empty_token() {
        // A separator at the start of the input yields a zero-length first token.
        assert_eq!(tokenize_strings(",a,b", ","), vec!["", "a", "b"]);
        assert_eq!(tokenize_strings("/a/b", "/"), vec!["", "a", "b"]);
    }

    #[test]
    fn test_tokenize_trailing_separator_keeps_empty_token() {
        // A separator at the end of the input yields a zero-length last token.
        assert_eq!(tokenize_strings("a,b,", ","), vec!["a", "b", ""]);
        assert_eq!(tokenize_strings("a/b/", "/"), vec!["a", "b", ""]);
    }

    #[test]
    fn test_tokenize_adjacent_separators_keep_empty_token() {
        // Two adjacent separators have a zero-length token between them.
        assert_eq!(tokenize_strings("a,,b", ","), vec!["a", "", "b"]);
        assert_eq!(tokenize_strings(",a,", ","), vec!["", "a", ""]);
        assert_eq!(tokenize_strings(",", ","), vec!["", ""]);
    }

    #[test]
    fn test_tokenize_all_gaps_are_tokens() {
        // Every gap between matches is a token: "abracadabra" split on
        // "(ab)|(a)" starts and ends with a zero-length token.
        assert_eq!(
            tokenize_strings("abracadabra", "(ab)|(a)"),
            vec!["", "r", "c", "d", "r", ""]
        );
    }

    // =========================================================================
    // XPath 2.0 regular-expression dialect
    // =========================================================================

    fn matches_result(
        input: &str,
        pattern: &str,
        flags: Option<&str>,
    ) -> Result<XPathValue<RoXmlNavigator<'static>>, XPathError> {
        let names = Box::leak(Box::new(NameTable::new()));
        let mut ctx = create_context(names);
        let mut args = vec![XPathValue::string(input), XPathValue::string(pattern)];
        if let Some(f) = flags {
            args.push(XPathValue::string(f));
        }
        matches(&mut ctx, args)
    }

    #[test]
    fn test_group_with_question_mark_is_rejected() {
        // `(?:` and the other `(?…)` forms belong to a later dialect; XPath 2.0
        // has no atom that starts with `(?`.
        for pattern in ["(?:a)", "a(?:b)c", "(?i)a", "(?=a)", "(?!a)", "((?:a))"] {
            assert!(
                matches!(
                    matches_result("a", pattern, None),
                    Err(XPathError::FORX0002 { .. })
                ),
                "expected FORX0002 for {pattern}"
            );
        }
    }

    #[test]
    fn test_escaped_paren_followed_by_quantifier_is_accepted() {
        // `\(?` is an escaped `(` carrying a `?` quantifier, which is legal.
        let result = matches_result("x", r"\(?x", None).unwrap();
        assert!(
            matches!(result, XPathValue::Item(XmlItem::Atomic(v)) if v.as_boolean() == Some(true))
        );

        let result = matches_result("(x", r"\(?x", None).unwrap();
        assert!(
            matches!(result, XPathValue::Item(XmlItem::Atomic(v)) if v.as_boolean() == Some(true))
        );
    }

    #[test]
    fn test_question_mark_in_character_class_is_accepted() {
        // A `(` and a `?` are ordinary members of a character class.
        let result = matches_result("?", "[(?]", None).unwrap();
        assert!(
            matches!(result, XPathValue::Item(XmlItem::Atomic(v)) if v.as_boolean() == Some(true))
        );

        // …including inside a subtracted nested class.
        let result = matches_result("b", "[a-z-[(?]]", None).unwrap();
        assert!(
            matches!(result, XPathValue::Item(XmlItem::Atomic(v)) if v.as_boolean() == Some(true))
        );
        let result = matches_result("?", "[a-z-[(?]]", None).unwrap();
        assert!(
            matches!(result, XPathValue::Item(XmlItem::Atomic(v)) if v.as_boolean() == Some(false))
        );

        // A class closed before the `(?` does not shield it.
        assert!(matches!(
            matches_result("a", "[ab](?:c)", None),
            Err(XPathError::FORX0002 { .. })
        ));
    }

    #[test]
    fn test_group_with_question_mark_rejected_under_x_flag() {
        // With `x`, whitespace is removed before the pattern is parsed, so
        // `( ?:a)` is the same pattern as `(?:a)`.
        assert!(matches!(
            matches_result("a", "( ?:a)", Some("x")),
            Err(XPathError::FORX0002 { .. })
        ));
        assert!(matches!(
            matches_result("a", "(\t\n?:a)", Some("x")),
            Err(XPathError::FORX0002 { .. })
        ));
        // Without `x` the space is a literal, so the pattern is a plain group.
        assert!(matches_result("a", "( ?:a)", None).is_ok());
    }

    /// Review finding X-13.1. With `x` the whitespace is removed *before* the
    /// pattern is interpreted (F&O §7.6.1.1), so in `^\ (?$` the backslash
    /// escapes the `(`: the pattern is `^\(?$`, an optional literal `(`.
    /// Whitespace inside a character class is not removed.
    #[test]
    fn test_x_flag_whitespace_is_removed_before_escapes_are_read() {
        let is = |input: &str, pattern: &str, flags: Option<&str>| -> Option<bool> {
            match matches_result(input, pattern, flags) {
                Ok(XPathValue::Item(XmlItem::Atomic(v))) => v.as_boolean(),
                Ok(_) => panic!("{pattern}: not a boolean"),
                Err(XPathError::FORX0002 { .. }) => None,
                Err(e) => panic!("{pattern}: {e}"),
            }
        };
        // With `x`.
        assert_eq!(is("(", r"^\ (?$", Some("x")), Some(true));
        assert_eq!(is("", r"^\ (?$", Some("x")), Some(true));
        assert_eq!(is("((", r"^\ (?$", Some("x")), Some(false));
        assert_eq!(is("(", "^\\\t\n(?$", Some("x")), Some(true));
        // F&O's own example.
        assert_eq!(is("hello world", r"hello\ sworld", Some("x")), Some(true));
        // A class keeps its whitespace and its `(?`.
        assert_eq!(is("?", "^[ (?]$", Some("x")), Some(true));
        assert_eq!(is(" ", "^[ (?]$", Some("x")), Some(true));
        // An unescaped `(` followed by `?` once the whitespace is gone is still
        // the later dialect.
        assert_eq!(is("a", r"\\ ( ?:a)", Some("x")), None);
        assert_eq!(is("a", r"[a] ( ?:a)", Some("x")), None);
        // `\ a` with `x` is `\a`, which is not an escape at all.
        assert_eq!(is("a", r"^\ a$", Some("x")), None);

        // Without `x` the space is what the backslash escapes — not a valid
        // escape — and the `(?` after it is the later dialect.
        assert_eq!(is("(", r"^\ (?$", None), None);
        assert_eq!(is("(", r"^\(?$", None), Some(true));
    }

    #[test]
    fn test_q_flag_is_rejected() {
        // The `q` (literal) flag was introduced after XPath 2.0.
        assert!(matches!(
            matches_result("a.b", "a.b", Some("q")),
            Err(XPathError::FORX0001 { .. })
        ));
        for flag in ["q", "z", "smixq", " "] {
            assert!(
                matches!(
                    matches_result("a", "a", Some(flag)),
                    Err(XPathError::FORX0001 { .. })
                ),
                "expected FORX0001 for flags {flag:?}"
            );
        }
    }

    #[test]
    fn test_defined_flags_are_accepted() {
        for flag in ["", "s", "m", "i", "x", "smix", "ii"] {
            assert!(
                matches_result("a", "a", Some(flag)).is_ok(),
                "expected flags {flag:?} to be accepted"
            );
        }
    }

    #[test]
    fn test_dialect_check_applies_to_replace_and_tokenize() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);
        let result = replace(
            &mut ctx,
            vec![
                XPathValue::string("abc"),
                XPathValue::string("(?:b)"),
                XPathValue::string("X"),
            ],
        );
        assert!(matches!(result, Err(XPathError::FORX0002 { .. })));

        let result = tokenize(
            &mut ctx,
            vec![XPathValue::string("abc"), XPathValue::string("(?:b)")],
        );
        assert!(matches!(result, Err(XPathError::FORX0002 { .. })));
    }

    /// `matches()` as a plain `bool`, or `None` for FORX0002.
    fn is(input: &str, pattern: &str, flags: Option<&str>) -> Option<bool> {
        match matches_result(input, pattern, flags) {
            Ok(value) => Some(boolean_of(value)),
            Err(XPathError::FORX0002 { .. }) => None,
            Err(e) => panic!("{pattern:?} {flags:?}: {e}"),
        }
    }

    /// F&O 1.0 §7.6.1.1: "If the s flag is not specified, the meta-character
    /// . matches any character except a newline (#x0A) character." The
    /// carriage return #xD is matched; F&O 3.1 later excluded it too.
    #[test]
    fn test_dot_matches_a_carriage_return_but_not_a_newline() {
        assert_eq!(is("a\rb", "^a.b$", None), Some(true));
        assert_eq!(is("a\nb", "^a.b$", None), Some(false));
        assert_eq!(is("a\nb", "^a.b$", Some("s")), Some(true));
        assert_eq!(is("\r", "^.$", None), Some(true));
        assert_eq!(is("\r\n", "^.+$", None), Some(false));
        assert_eq!(is("\r\n", "^.+$", Some("s")), Some(true));
        // A class expression is unaffected.
        assert_eq!(is("\r", "^[^\n]$", None), Some(true));
    }

    /// F&O 1.0 §7.6.1.1, `x`: "whitespace characters (#x9, #xA, #xD and
    /// #x20) in the regular expression are removed" — those four only, so a
    /// form feed (#xC) stays a character of the pattern.
    #[test]
    fn test_x_flag_keeps_a_form_feed() {
        assert_eq!(is("\u{C}", "^\u{C}$", Some("x")), Some(true));
        assert_eq!(is("", "^\u{C}$", Some("x")), Some(false));
        // A form feed between `(` and `?` is a character with a quantifier,
        // not the start of a `(?…)` group.
        assert_eq!(is(":a", "^(\u{C}?:a)$", Some("x")), Some(true));
        assert_eq!(is("ab", "^a \t\r\n b$", Some("x")), Some(true));
    }

    /// The flags after a `;` belong to the engine, not to XPath 2.0: FORX0001.
    /// An unknown block name is FORX0002 (F&O 1.0 takes the block names XML
    /// Schema 1.0 lists).
    #[test]
    fn test_engine_flags_and_unknown_blocks_are_errors() {
        for flags in [";k", "i;k", ";"] {
            assert!(
                matches!(
                    matches_result("a", "a", Some(flags)),
                    Err(XPathError::FORX0001 { .. })
                ),
                "{flags:?}"
            );
        }
        assert_eq!(is("a", r"\p{IsNoSuchBlock}", None), None);
        assert_eq!(is("a", r"\p{IsBasicLatin}", None), Some(true));
    }

    /// Groups and character classes nested more than 64 levels deep are
    /// FORX0002 — the engine's limit, which keeps compiling and matching
    /// inside the stack — in all three functions; a pattern of 100,000 open
    /// groups is an error, not an abort. 64 levels work.
    #[test]
    fn test_nesting_beyond_the_engine_limit_is_forx0002() {
        let nested = |depth: usize| format!("{}a{}", "(".repeat(depth), ")".repeat(depth));
        let classes =
            |depth: usize| format!("{}[a]{}", "[a-z-".repeat(depth - 1), "]".repeat(depth - 1));
        assert_eq!(is("xay", &nested(64), None), Some(true));
        // `[a-z-[a-z-[…[a]…]]]` alternates: an even depth is `b`–`z`.
        assert_eq!(is("b", &classes(64), None), Some(true));
        assert_eq!(is("a", &classes(64), None), Some(false));
        assert_eq!(is("xay", &nested(65), None), None);
        assert_eq!(is("a", &classes(65), None), None);
        assert_eq!(is("a", &"(".repeat(100_000), None), None);

        let names = NameTable::new();
        let mut ctx = create_context(&names);
        let result = replace(
            &mut ctx,
            vec![
                XPathValue::string("a"),
                XPathValue::string(nested(65)),
                XPathValue::string("x"),
            ],
        );
        assert!(matches!(result, Err(XPathError::FORX0002 { .. })));
        let result = tokenize(
            &mut ctx,
            vec![XPathValue::string("a"), XPathValue::string(nested(65))],
        );
        assert!(matches!(result, Err(XPathError::FORX0002 { .. })));
        // Invalid flags are still reported first.
        assert!(matches!(
            matches_result("a", &nested(65), Some("q")),
            Err(XPathError::FORX0001 { .. })
        ));
    }

    // =========================================================================
    // One compilation per (pattern, flags) pair per run
    //
    // Every test here asserts the observable result first and the cache
    // counters second, so that it fails both when the cache changes an answer
    // and when it silently stops engaging.
    // =========================================================================

    /// The `true`/`false` a `matches()` result carries.
    fn boolean_of(value: XPathValue<RoXmlNavigator<'_>>) -> bool {
        match value {
            XPathValue::Item(XmlItem::Atomic(v)) => v.as_boolean().expect("xs:boolean"),
            _ => panic!("expected a single xs:boolean"),
        }
    }

    #[test]
    fn test_a_constant_pattern_is_compiled_once_per_run() {
        // The shape this cache exists for: one pattern, one call per item.
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        for i in 0..200 {
            let result = matches(
                &mut ctx,
                vec![
                    XPathValue::string(format!("item-{i}")),
                    XPathValue::string(r"\p{Ll}"),
                ],
            )
            .unwrap();
            assert!(boolean_of(result), "item-{i} has lowercase letters in it");
        }

        assert_eq!(ctx.regex_cache().compiles(), 1);
        assert_eq!(ctx.regex_cache().hits(), 199);
    }

    #[test]
    fn test_the_three_functions_share_one_compiled_pattern() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        let hit = matches(
            &mut ctx,
            vec![XPathValue::string("a,b"), XPathValue::string(",")],
        )
        .unwrap();
        assert!(boolean_of(hit));

        let replaced = replace(
            &mut ctx,
            vec![
                XPathValue::string("a,b"),
                XPathValue::string(","),
                XPathValue::string(";"),
            ],
        )
        .unwrap();
        assert!(
            matches!(replaced, XPathValue::Item(XmlItem::Atomic(ref v)) if v.as_string() == Some("a;b"))
        );

        let tokens = tokenize(
            &mut ctx,
            vec![XPathValue::string("a,b"), XPathValue::string(",")],
        )
        .unwrap();
        assert_eq!(token_strings(tokens), vec!["a", "b"]);

        // One program, built by the `matches()` call and reused by the other two.
        assert_eq!(ctx.regex_cache().compiles(), 1);
        assert_eq!(ctx.regex_cache().hits(), 2);
    }

    #[test]
    fn test_flags_are_part_of_the_cache_key() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        for _ in 0..2 {
            let plain = matches(
                &mut ctx,
                vec![XPathValue::string("A"), XPathValue::string("a")],
            )
            .unwrap();
            assert!(!boolean_of(plain));

            let folded = matches(
                &mut ctx,
                vec![
                    XPathValue::string("A"),
                    XPathValue::string("a"),
                    XPathValue::string("i"),
                ],
            )
            .unwrap();
            assert!(boolean_of(folded));
        }

        // Same pattern, two flag strings, two programs.
        assert_eq!(ctx.regex_cache().compiles(), 2);
        assert_eq!(ctx.regex_cache().hits(), 2);
    }

    #[test]
    fn test_an_invalid_pattern_reports_the_same_error_every_time() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        let mut seen = Vec::new();
        for _ in 0..5 {
            let result = matches(
                &mut ctx,
                vec![XPathValue::string("test"), XPathValue::string("[invalid")],
            );
            let err = result.err().expect("an invalid pattern must raise");
            assert!(matches!(err, XPathError::FORX0002 { .. }));
            seen.push(err.to_string());
        }
        assert!(seen.windows(2).all(|pair| pair[0] == pair[1]), "{seen:?}");
        assert_eq!(ctx.regex_cache().compiles(), 1);
        assert_eq!(ctx.regex_cache().hits(), 4);
    }

    #[test]
    fn test_an_invalid_flag_reports_the_same_error_every_time() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        let mut seen = Vec::new();
        for _ in 0..5 {
            let result = matches(
                &mut ctx,
                vec![
                    XPathValue::string("test"),
                    XPathValue::string("test"),
                    XPathValue::string("z"),
                ],
            );
            let err = result.err().expect("an undefined flag must raise");
            assert!(matches!(err, XPathError::FORX0001 { .. }));
            seen.push(err.to_string());
        }
        assert!(seen.windows(2).all(|pair| pair[0] == pair[1]), "{seen:?}");
        assert_eq!(ctx.regex_cache().compiles(), 1);
    }

    #[test]
    fn test_a_reused_program_still_raises_forx0003() {
        // `a?` matches the zero-length string. `matches()` is happy with it and
        // is what puts it in the cache; `replace()` and `tokenize()` must still
        // raise FORX0003 from the *reused* program, on every call.
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        let hit = matches(
            &mut ctx,
            vec![XPathValue::string("test"), XPathValue::string("a?")],
        )
        .unwrap();
        assert!(boolean_of(hit));

        for _ in 0..3 {
            let result = replace(
                &mut ctx,
                vec![
                    XPathValue::string("test"),
                    XPathValue::string("a?"),
                    XPathValue::string("X"),
                ],
            );
            assert!(matches!(result, Err(XPathError::FORX0003 { .. })));

            let result = tokenize(
                &mut ctx,
                vec![XPathValue::string("test"), XPathValue::string("a?")],
            );
            assert!(matches!(result, Err(XPathError::FORX0003 { .. })));
        }
        assert_eq!(ctx.regex_cache().compiles(), 1);
    }

    #[test]
    fn test_a_reused_program_still_raises_forx0004() {
        // The replacement string is not part of the key, so a cache hit must
        // not carry the previous call's verdict on it.
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        let good = replace(
            &mut ctx,
            vec![
                XPathValue::string("test"),
                XPathValue::string("t"),
                XPathValue::string("X"),
            ],
        )
        .unwrap();
        assert!(
            matches!(good, XPathValue::Item(XmlItem::Atomic(ref v)) if v.as_string() == Some("XesX"))
        );

        for _ in 0..3 {
            let result = replace(
                &mut ctx,
                vec![
                    XPathValue::string("test"),
                    XPathValue::string("t"),
                    XPathValue::string("$x"),
                ],
            );
            assert!(matches!(result, Err(XPathError::FORX0004 { .. })));
        }
        assert_eq!(ctx.regex_cache().compiles(), 1);
    }

    #[test]
    fn test_the_cache_does_not_outlive_its_run() {
        let names = NameTable::new();
        for _ in 0..3 {
            let mut ctx = create_context(&names);
            for _ in 0..2 {
                let hit = matches(
                    &mut ctx,
                    vec![XPathValue::string("abc"), XPathValue::string("b")],
                )
                .unwrap();
                assert!(boolean_of(hit));
            }
            // Reused inside the run, and a fresh context starts empty again:
            // the cache belongs to the run, not to the process.
            assert_eq!(ctx.regex_cache().compiles(), 1);
            assert_eq!(ctx.regex_cache().hits(), 1);
        }
    }

    #[test]
    fn test_a_pattern_computed_per_item_does_not_grow_the_cache() {
        // `matches($s, $row/@pattern)` — a new pattern for every item. The cache
        // must stay bounded and must keep answering correctly.
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        let rounds = crate::xpath::regex_cache::MAX_ENTRIES * 8;
        for i in 0..rounds {
            let pattern = format!("^item-{i}$");
            let hit = matches(
                &mut ctx,
                vec![
                    XPathValue::string(format!("item-{i}")),
                    XPathValue::string(pattern),
                ],
            )
            .unwrap();
            assert!(boolean_of(hit));
            assert!(
                ctx.regex_cache().len() <= crate::xpath::regex_cache::MAX_ENTRIES,
                "{} entries resident after {i} patterns",
                ctx.regex_cache().len()
            );
        }
        assert_eq!(ctx.regex_cache().compiles() as usize, rounds);
        assert_eq!(ctx.regex_cache().hits(), 0);
        assert_eq!(
            ctx.regex_cache().len(),
            crate::xpath::regex_cache::MAX_ENTRIES
        );
    }

    // =========================================================================
    // XSD/XPath character class escape tests (\i, \c)
    // =========================================================================

    #[test]
    fn test_matches_initial_name_char() {
        // Test \i matches initial XML name characters
        let names = NameTable::new();
        let mut ctx = create_context(&names);
        let result = matches(
            &mut ctx,
            vec![XPathValue::string("_foo"), XPathValue::string(r"\i")],
        )
        .unwrap();
        assert!(
            matches!(result, XPathValue::Item(XmlItem::Atomic(v)) if v.as_boolean() == Some(true))
        );
    }

    #[test]
    fn test_matches_xml_name_pattern() {
        // Test \i\c* matches XML names
        let names = NameTable::new();
        let mut ctx = create_context(&names);
        let result = matches(
            &mut ctx,
            vec![XPathValue::string("foo:bar"), XPathValue::string(r"\i\c*")],
        )
        .unwrap();
        assert!(
            matches!(result, XPathValue::Item(XmlItem::Atomic(v)) if v.as_boolean() == Some(true))
        );
    }

    #[test]
    fn test_matches_digit_not_initial() {
        // Test \i does NOT match digits
        let names = NameTable::new();
        let mut ctx = create_context(&names);
        let result = matches(
            &mut ctx,
            vec![XPathValue::string("123"), XPathValue::string(r"^\i")],
        )
        .unwrap();
        assert!(
            matches!(result, XPathValue::Item(XmlItem::Atomic(v)) if v.as_boolean() == Some(false))
        );
    }

    #[test]
    fn test_matches_name_char_with_digits() {
        // Test \c matches digits and other name characters
        let names = NameTable::new();
        let mut ctx = create_context(&names);
        let result = matches(
            &mut ctx,
            vec![XPathValue::string("abc123"), XPathValue::string(r"\c+")],
        )
        .unwrap();
        assert!(
            matches!(result, XPathValue::Item(XmlItem::Atomic(v)) if v.as_boolean() == Some(true))
        );
    }

    #[test]
    fn test_replace_with_name_char_pattern() {
        // Test replace with \c pattern
        let names = NameTable::new();
        let mut ctx = create_context(&names);
        let result = replace(
            &mut ctx,
            vec![
                XPathValue::string("hello world"),
                XPathValue::string(r"\c+"),
                XPathValue::string("X"),
            ],
        )
        .unwrap();

        if let XPathValue::Item(XmlItem::Atomic(v)) = result {
            assert_eq!(v.as_string(), Some("X X"));
        } else {
            panic!("Expected string");
        }
    }

    #[test]
    fn test_tokenize_with_non_name_char() {
        // Test tokenize using \C (non-name character) as delimiter
        let names = NameTable::new();
        let mut ctx = create_context(&names);
        let result = tokenize(
            &mut ctx,
            vec![
                XPathValue::string("foo bar baz"),
                XPathValue::string(r"\C+"),
            ],
        )
        .unwrap();

        match result {
            XPathValue::Sequence(items) => {
                assert_eq!(items.len(), 3);
                let strs: Vec<String> = items
                    .iter()
                    .map(|item| {
                        if let XmlItem::Atomic(v) = item {
                            v.to_string_value()
                        } else {
                            panic!("Expected atomic")
                        }
                    })
                    .collect();
                assert_eq!(strs, vec!["foo", "bar", "baz"]);
            }
            _ => panic!("Expected sequence"),
        }
    }
    // ── Error order against an empty or non-matching input ────────────
    //
    // F&O states FORX0002 (invalid `$pattern`), FORX0001 (invalid `$flags`)
    // and, for `replace` and `tokenize`, FORX0003 (the pattern matches the
    // zero-length string) and FORX0004 (malformed `$replacement`) with no
    // exemption for any input. `tokenize` returning the empty sequence for an
    // empty `$input` is a separate statement about the *result*, so it comes
    // after the errors, not before them.

    #[test]
    fn tokenize_reports_an_invalid_pattern_for_an_empty_string_input() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        let result = tokenize(
            &mut ctx,
            vec![XPathValue::string(""), XPathValue::string("[")],
        );

        assert!(matches!(result, Err(XPathError::FORX0002 { .. })));
    }

    #[test]
    fn tokenize_reports_an_invalid_pattern_for_an_empty_sequence_input() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        let result = tokenize(&mut ctx, vec![XPathValue::Empty, XPathValue::string("[")]);

        assert!(matches!(result, Err(XPathError::FORX0002 { .. })));
    }

    #[test]
    fn tokenize_reports_invalid_flags_for_an_empty_input() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        let result = tokenize(
            &mut ctx,
            vec![
                XPathValue::string(""),
                XPathValue::string("a"),
                XPathValue::string("z"),
            ],
        );

        assert!(matches!(result, Err(XPathError::FORX0001 { .. })));
    }

    #[test]
    fn tokenize_reports_a_zero_length_match_for_an_empty_input() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        for input in [XPathValue::string(""), XPathValue::Empty] {
            let result = tokenize(&mut ctx, vec![input, XPathValue::string("a*")]);
            assert!(matches!(result, Err(XPathError::FORX0003 { .. })));
        }
    }

    /// The empty-sequence result survives for a pattern that raises nothing.
    /// XQTS `fn-tokenize-8` is exactly this: `fn:count(fn:tokenize("", "\s+"))`
    /// with the expected result `0`.
    #[test]
    fn tokenize_still_returns_the_empty_sequence_for_a_valid_pattern() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        for input in [XPathValue::string(""), XPathValue::Empty] {
            let result = tokenize(&mut ctx, vec![input, XPathValue::string("\\s+")]).unwrap();
            assert!(matches!(result, XPathValue::Empty));
        }
    }

    #[test]
    fn replace_reports_a_malformed_replacement_for_an_empty_input() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        for replacement in ["$", "$ ", "\\", "\\ "] {
            let result = replace(
                &mut ctx,
                vec![
                    XPathValue::string(""),
                    XPathValue::string("a"),
                    XPathValue::string(replacement),
                ],
            );
            assert!(
                matches!(result, Err(XPathError::FORX0004 { .. })),
                "expected FORX0004 for replacement {replacement:?}",
            );
        }
    }

    /// The same hole one step further out: the backend validates
    /// `$replacement` only while it walks the matches, so a pattern that
    /// matches nothing used to let a malformed replacement through as well.
    #[test]
    fn replace_reports_a_malformed_replacement_when_nothing_matches() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        let result = replace(
            &mut ctx,
            vec![
                XPathValue::string("abc"),
                XPathValue::string("z"),
                XPathValue::string("$"),
            ],
        );

        assert!(matches!(result, Err(XPathError::FORX0004 { .. })));
    }

    /// A well-formed replacement is still accepted for those same inputs —
    /// `\\`, `\$` and `$0` are the three legal shapes. XQTS `fn-replace-11`
    /// is `fn:count(fn:replace((), "bra", "*"))` with the expected result `1`.
    #[test]
    fn replace_accepts_a_well_formed_replacement_for_an_empty_input() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        for replacement in ["*", "\\\\", "\\$", "$0", "a$1b"] {
            let result = replace(
                &mut ctx,
                vec![
                    XPathValue::string(""),
                    XPathValue::string("(a)"),
                    XPathValue::string(replacement),
                ],
            )
            .unwrap_or_else(|e| panic!("replacement {replacement:?} must be accepted: {e}"));
            assert!(
                matches!(result, XPathValue::Item(XmlItem::Atomic(ref v)) if v.as_string() == Some("")),
            );
        }
    }

    #[test]
    fn replace_reports_a_zero_length_match_for_an_empty_input() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        for input in [XPathValue::string(""), XPathValue::Empty] {
            let result = replace(
                &mut ctx,
                vec![input, XPathValue::string("a*"), XPathValue::string("x")],
            );
            assert!(matches!(result, Err(XPathError::FORX0003 { .. })));
        }
    }

    #[test]
    fn replace_reports_invalid_flags_and_patterns_for_an_empty_input() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        let result = replace(
            &mut ctx,
            vec![
                XPathValue::string(""),
                XPathValue::string("a"),
                XPathValue::string("x"),
                XPathValue::string("z"),
            ],
        );
        assert!(matches!(result, Err(XPathError::FORX0001 { .. })));

        let result = replace(
            &mut ctx,
            vec![
                XPathValue::string(""),
                XPathValue::string("["),
                XPathValue::string("x"),
            ],
        );
        assert!(matches!(result, Err(XPathError::FORX0002 { .. })));
    }

    /// `fn:matches` has no shortcut and no FORX0003 rule; it compiles before
    /// it matches, so an empty input has always reported the pattern and flag
    /// errors. A regression guard for the audit.
    #[test]
    fn matches_reports_pattern_and_flag_errors_for_an_empty_input() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        for input in [XPathValue::string(""), XPathValue::Empty] {
            let result = matches(&mut ctx, vec![input, XPathValue::string("[")]);
            assert!(matches!(result, Err(XPathError::FORX0002 { .. })));
        }

        let result = matches(
            &mut ctx,
            vec![
                XPathValue::Empty,
                XPathValue::string("a"),
                XPathValue::string("z"),
            ],
        );
        assert!(matches!(result, Err(XPathError::FORX0001 { .. })));

        // A pattern that matches the zero-length string is fine for matches().
        let result = matches(&mut ctx, vec![XPathValue::Empty, XPathValue::string("a*")]).unwrap();
        assert!(boolean_of(result));
    }

    #[test]
    fn test_a_host_cache_compiles_once_across_runs() {
        let regexes = crate::xpath::RegexCache::new();
        let names = NameTable::new();
        for _ in 0..5 {
            let mut ctx = create_context(&names).with_regex_cache(&regexes);
            for i in 0..10 {
                let result = matches(
                    &mut ctx,
                    vec![
                        XPathValue::string(format!("item-{i}")),
                        XPathValue::string(r"\p{Ll}"),
                    ],
                )
                .unwrap();
                assert!(boolean_of(result));
            }
            // The run's own cache is not used while a host cache is lent.
            assert_eq!(ctx.regex_cache().len(), 0);
        }
        assert_eq!(regexes.compiles(), 1);
        assert_eq!(regexes.hits(), 49);
    }

    #[test]
    fn test_a_host_cache_serves_the_three_functions_and_replays_errors() {
        let regexes = crate::xpath::RegexCache::new();
        let names = NameTable::new();
        for _ in 0..3 {
            let mut ctx = create_context(&names).with_regex_cache(&regexes);
            let hit = matches(
                &mut ctx,
                vec![XPathValue::string("a,b"), XPathValue::string(",")],
            )
            .unwrap();
            assert!(boolean_of(hit));
            let replaced = replace(
                &mut ctx,
                vec![
                    XPathValue::string("a,b"),
                    XPathValue::string(","),
                    XPathValue::string(";"),
                ],
            )
            .unwrap();
            assert!(
                matches!(replaced, XPathValue::Item(XmlItem::Atomic(ref v)) if v.as_string() == Some("a;b"))
            );
            let tokens = tokenize(
                &mut ctx,
                vec![XPathValue::string("a,b"), XPathValue::string(",")],
            )
            .unwrap();
            assert_eq!(token_strings(tokens), vec!["a", "b"]);

            // FORX0003 from a reused program, FORX0002 and FORX0001 replayed.
            let result = tokenize(
                &mut ctx,
                vec![XPathValue::string("test"), XPathValue::string("a?")],
            );
            assert!(matches!(result, Err(XPathError::FORX0003 { .. })));
            let result = matches(
                &mut ctx,
                vec![XPathValue::string("test"), XPathValue::string("[")],
            );
            assert!(matches!(result, Err(XPathError::FORX0002 { .. })));
            let result = matches(
                &mut ctx,
                vec![
                    XPathValue::string("test"),
                    XPathValue::string("t"),
                    XPathValue::string("z"),
                ],
            );
            assert!(matches!(result, Err(XPathError::FORX0001 { .. })));
        }
        // `,`, `a?`, `[` and (`t`, `z`): four keys, each compiled once.
        assert_eq!(regexes.compiles(), 4);
        assert_eq!(regexes.len(), 4);
    }
}
