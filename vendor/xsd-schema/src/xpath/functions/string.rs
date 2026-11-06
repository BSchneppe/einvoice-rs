//! String functions for XPath 2.0.
//!
//! This module implements XPath 2.0 string functions, delegating to
//! the implementations in `xpath::string_ops`.

use crate::types::value::XmlValue;
use crate::types::XmlTypeCode;
use crate::xpath::collation::{self, ActiveCollation};
use crate::xpath::error::XPathError;
use crate::xpath::iterator::XmlItem;
use crate::xpath::string_ops;
use crate::xpath::DomNavigator;

use super::{
    atomize_sequence, atomize_to_double, atomize_to_string, atomize_to_string_opt,
    atomize_to_string_required, atomize_to_string_strict, atomize_to_string_strict_opt, convert,
    XPathValue,
};
use crate::xpath::context::DynamicContext;

/// The collation of a call: its `$collation` argument if it has one, and
/// otherwise the static context's default collation.
///
/// The argument is taken off the back of `args`, so the caller keeps its
/// positional arguments where they were. Unlike the comparison operators, a
/// function that takes a `$collation` argument needs the collation whatever its
/// other arguments turn out to be, so an unsupported URI is FOCH0002 here and
/// not later ([`ActiveCollation::require`]).
fn call_collation<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    args: &mut Vec<XPathValue<N>>,
    has_collation_arg: bool,
) -> Result<ActiveCollation, XPathError> {
    let explicit = if has_collation_arg {
        Some(atomize_to_string_required(args.pop().unwrap())?)
    } else {
        None
    };
    collation::resolve_collation_cached(context, explicit.as_deref()).require()
}

// ============================================================================
// String Functions
// ============================================================================

/// fn:concat($arg1 as xs:anyAtomicType?, $arg2 as xs:anyAtomicType?, ...) as xs:string
///
/// Concatenates two or more strings.
pub fn concat<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() < 2 {
        return Err(XPathError::wrong_number_of_arguments(
            "concat",
            2,
            args.len(),
        ));
    }

    let strings: Result<Vec<String>, _> = args.into_iter().map(atomize_to_string).collect();
    let strings = strings?;
    let refs: Vec<&str> = strings.iter().map(|s| s.as_str()).collect();
    let result = string_ops::concat(&refs);
    Ok(XPathValue::string(result))
}

/// fn:string-join($arg1 as xs:string*, $arg2 as xs:string) as xs:string
///
/// Joins strings with a separator.
pub fn string_join<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 2 {
        return Err(XPathError::wrong_number_of_arguments(
            "string-join",
            2,
            args.len(),
        ));
    }

    let separator = atomize_to_string_required(args.pop().unwrap())?;
    let sequence = args.pop().unwrap();

    // `$arg1 as xs:string*`: the function conversion rules atomize the
    // sequence (XPath 2.0 §3.1.5), so a node contributes its typed value — one
    // string per member of a list-typed node — rather than its string value.
    let strings: Vec<String> = atomize_sequence(sequence)?
        .iter()
        .map(|v| v.to_string_value())
        .collect();
    let refs: Vec<&str> = strings.iter().map(|s| s.as_str()).collect();
    let result = string_ops::string_join(&refs, &separator);
    Ok(XPathValue::string(result))
}

/// fn:substring($sourceString as xs:string?, $start as xs:double) as xs:string
/// fn:substring($sourceString as xs:string?, $start as xs:double, $length as xs:double) as xs:string
///
/// Returns a portion of the source string.
pub fn substring<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(XPathError::wrong_number_of_arguments(
            "substring",
            2,
            args.len(),
        ));
    }

    let source = atomize_to_string(args.remove(0))?;
    let start = atomize_to_double(args.remove(0))?;
    let length = if !args.is_empty() {
        Some(atomize_to_double(args.remove(0))?)
    } else {
        None
    };

    let result = string_ops::substring(&source, start, length);
    Ok(XPathValue::string(result))
}

/// fn:string-length($arg as xs:string?) as xs:integer
/// fn:string-length() as xs:integer (uses context item)
///
/// Returns the length of the string in characters.
pub fn string_length<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() > 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "string-length",
            1,
            args.len(),
        ));
    }

    let source = if args.is_empty() {
        // Use context item
        match &context.context_item {
            Some(item) => match item {
                XmlItem::Atomic(v) => v.to_string_value(),
                XmlItem::Node(n) => n.value(),
            },
            None => {
                return Err(XPathError::XPDY0002 {
                    message: "Context item is absent".to_string(),
                })
            }
        }
    } else {
        atomize_to_string(args.into_iter().next().unwrap())?
    };

    let len = string_ops::string_length(&source);
    Ok(XPathValue::integer(len as i64))
}

/// fn:normalize-space($arg as xs:string?) as xs:string
/// fn:normalize-space() as xs:string (uses context item)
///
/// Normalizes whitespace in a string.
pub fn normalize_space<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() > 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "normalize-space",
            1,
            args.len(),
        ));
    }

    let source = if args.is_empty() {
        // Use context item
        match &context.context_item {
            Some(item) => match item {
                XmlItem::Atomic(v) => v.to_string_value(),
                XmlItem::Node(n) => n.value(),
            },
            None => {
                return Err(XPathError::XPDY0002 {
                    message: "Context item is absent".to_string(),
                })
            }
        }
    } else {
        atomize_to_string(args.into_iter().next().unwrap())?
    };

    let result = string_ops::normalize_space(&source);
    Ok(XPathValue::string(result))
}

/// fn:normalize-unicode($arg as xs:string?) as xs:string
/// fn:normalize-unicode($arg as xs:string?, $normalizationForm as xs:string) as xs:string
///
/// Normalizes a string using Unicode normalization.
pub fn normalize_unicode<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.is_empty() || args.len() > 2 {
        return Err(XPathError::wrong_number_of_arguments(
            "normalize-unicode",
            1,
            args.len(),
        ));
    }

    let source = atomize_to_string_strict(args.remove(0))?;

    let form = if !args.is_empty() {
        let form_str = atomize_to_string_required(args.remove(0))?;
        let trimmed = form_str.trim();
        if trimmed.is_empty() {
            None
        } else {
            match string_ops::UnicodeNormalizationForm::parse(trimmed) {
                Some(f) => Some(f),
                None => {
                    return Err(XPathError::FOCH0003 {
                        normalization_form: form_str,
                    })
                }
            }
        }
    } else {
        // Default is NFC
        Some(string_ops::UnicodeNormalizationForm::NFC)
    };

    #[cfg(feature = "unicode-normalization")]
    let result = string_ops::normalize_unicode(&source, form);

    #[cfg(not(feature = "unicode-normalization"))]
    let result = string_ops::normalize_unicode(&source, form)?;

    Ok(XPathValue::string(result))
}

/// fn:upper-case($arg as xs:string?) as xs:string
///
/// Converts a string to uppercase.
pub fn upper_case<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "upper-case",
            1,
            args.len(),
        ));
    }

    let source = atomize_to_string(args.remove(0))?;
    let result = string_ops::upper_case(&source);
    Ok(XPathValue::string(result))
}

/// fn:lower-case($arg as xs:string?) as xs:string
///
/// Converts a string to lowercase.
pub fn lower_case<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "lower-case",
            1,
            args.len(),
        ));
    }

    let source = atomize_to_string(args.remove(0))?;
    let result = string_ops::lower_case(&source);
    Ok(XPathValue::string(result))
}

/// fn:translate($arg as xs:string?, $mapString as xs:string, $transString as xs:string) as xs:string
///
/// Translates characters in a string.
pub fn translate<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 3 {
        return Err(XPathError::wrong_number_of_arguments(
            "translate",
            3,
            args.len(),
        ));
    }

    let source = atomize_to_string_strict(args.remove(0))?;
    let map_string = atomize_to_string_strict(args.remove(0))?;
    let trans_string = atomize_to_string_strict(args.remove(0))?;

    let result = string_ops::translate(&source, &map_string, &trans_string);
    Ok(XPathValue::string(result))
}

/// fn:encode-for-uri($uri-part as xs:string?) as xs:string
///
/// Encodes a string for use in a URI.
pub fn encode_for_uri<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "encode-for-uri",
            1,
            args.len(),
        ));
    }

    let source = atomize_to_string_strict(args.remove(0))?;
    let result = string_ops::encode_for_uri(&source);
    Ok(XPathValue::string(result))
}

/// fn:iri-to-uri($iri as xs:string?) as xs:string
///
/// Converts an IRI to a URI.
pub fn iri_to_uri<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "iri-to-uri",
            1,
            args.len(),
        ));
    }

    let source = atomize_to_string_strict(args.remove(0))?;
    let result = string_ops::iri_to_uri(&source);
    Ok(XPathValue::string(result))
}

/// fn:escape-html-uri($uri as xs:string?) as xs:string
///
/// Escapes a URI for use in HTML.
pub fn escape_html_uri<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "escape-html-uri",
            1,
            args.len(),
        ));
    }

    let source = atomize_to_string_strict(args.remove(0))?;
    let result = string_ops::escape_html_uri(&source);
    Ok(XPathValue::string(result))
}

/// fn:contains($arg1 as xs:string?, $arg2 as xs:string?) as xs:boolean
/// fn:contains($arg1 as xs:string?, $arg2 as xs:string?, $collation as xs:string) as xs:boolean
///
/// Checks if a string contains a substring.
///
/// F&O §7.5.3 defines the answer over *collation units*: true when some
/// substring of `$arg1` has the collation units of `$arg2`. Under the codepoint
/// collation a collation unit is a codepoint and that is plain substring
/// search; under a host collation it is
/// [`Collation::find`](crate::xpath::collation::Collation::find), and a
/// collation that has no collation units raises FOCH0004. The order is F&O
/// §7.5.1's: an unsupported collation is FOCH0002 first (§7.3.1), then a
/// zero-length `$arg2` is `true` whatever the collation ("If the value of $arg2
/// is the zero-length string, then the function returns true"), and only a
/// search that needs collation units can raise FOCH0004.
pub fn contains<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(XPathError::wrong_number_of_arguments(
            "contains",
            2,
            args.len(),
        ));
    }

    let has_collation_arg = args.len() == 3;
    let collation = call_collation(context, &mut args, has_collation_arg)?;
    let source = atomize_to_string(args.remove(0))?;
    let substring = atomize_to_string(args.remove(0))?;

    let result = match collation.as_ref().custom() {
        None => string_ops::contains(&source, &substring),
        Some(custom) => {
            collation::collated_find(custom, collation.uri(), &source, &substring)?.is_some()
        }
    };
    Ok(XPathValue::boolean(result))
}

/// fn:starts-with($arg1 as xs:string?, $arg2 as xs:string?) as xs:boolean
/// fn:starts-with($arg1 as xs:string?, $arg2 as xs:string?, $collation as xs:string) as xs:boolean
///
/// Checks if a string starts with a prefix.
///
/// F&O §7.5.2: true when some *prefix* of `$arg1` has the collation units of
/// `$arg2`; see [`contains`] for what that means under a host collation.
pub fn starts_with<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(XPathError::wrong_number_of_arguments(
            "starts-with",
            2,
            args.len(),
        ));
    }

    let has_collation_arg = args.len() == 3;
    let collation = call_collation(context, &mut args, has_collation_arg)?;
    let source = atomize_to_string(args.remove(0))?;
    let prefix = atomize_to_string(args.remove(0))?;

    let result = match collation.as_ref().custom() {
        None => string_ops::starts_with(&source, &prefix),
        Some(custom) => collation::collated_starts_with(custom, collation.uri(), &source, &prefix)?,
    };
    Ok(XPathValue::boolean(result))
}

/// fn:ends-with($arg1 as xs:string?, $arg2 as xs:string?) as xs:boolean
/// fn:ends-with($arg1 as xs:string?, $arg2 as xs:string?, $collation as xs:string) as xs:boolean
///
/// Checks if a string ends with a suffix.
///
/// F&O §7.5.2: true when some *suffix* of `$arg1` has the collation units of
/// `$arg2`; see [`contains`] for what that means under a host collation.
pub fn ends_with<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(XPathError::wrong_number_of_arguments(
            "ends-with",
            2,
            args.len(),
        ));
    }

    let has_collation_arg = args.len() == 3;
    let collation = call_collation(context, &mut args, has_collation_arg)?;
    let source = atomize_to_string(args.remove(0))?;
    let suffix = atomize_to_string(args.remove(0))?;

    let result = match collation.as_ref().custom() {
        None => string_ops::ends_with(&source, &suffix),
        Some(custom) => collation::collated_ends_with(custom, collation.uri(), &source, &suffix)?,
    };
    Ok(XPathValue::boolean(result))
}

/// fn:substring-before($arg1 as xs:string?, $arg2 as xs:string?) as xs:string
/// fn:substring-before($arg1 as xs:string?, $arg2 as xs:string?, $collation as xs:string) as xs:string
///
/// Returns the substring before the first occurrence of the pattern.
///
/// F&O §7.5.4: the part of `$arg1` that precedes the *first* minimal match of
/// `$arg2`'s collation units, and the zero-length string when there is none;
/// see [`contains`] for what a minimal match is under a host collation.
pub fn substring_before<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(XPathError::wrong_number_of_arguments(
            "substring-before",
            2,
            args.len(),
        ));
    }

    let has_collation_arg = args.len() == 3;
    let collation = call_collation(context, &mut args, has_collation_arg)?;
    let source = atomize_to_string(args.remove(0))?;
    let pattern = atomize_to_string(args.remove(0))?;

    let result = match collation.as_ref().custom() {
        None => string_ops::substring_before(&source, &pattern),
        Some(custom) => {
            match collation::collated_find(custom, collation.uri(), &source, &pattern)? {
                Some((start, _)) => collation::slice_at(&source, 0..start)?.to_string(),
                None => String::new(),
            }
        }
    };
    Ok(XPathValue::string(result))
}

/// fn:substring-after($arg1 as xs:string?, $arg2 as xs:string?) as xs:string
/// fn:substring-after($arg1 as xs:string?, $arg2 as xs:string?, $collation as xs:string) as xs:string
///
/// Returns the substring after the first occurrence of the pattern.
///
/// F&O §7.5.5: the part of `$arg1` that follows the *first* minimal match of
/// `$arg2`'s collation units, and the zero-length string when there is none;
/// see [`contains`] for what a minimal match is under a host collation.
pub fn substring_after<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(XPathError::wrong_number_of_arguments(
            "substring-after",
            2,
            args.len(),
        ));
    }

    let has_collation_arg = args.len() == 3;
    let collation = call_collation(context, &mut args, has_collation_arg)?;
    let source = atomize_to_string(args.remove(0))?;
    let pattern = atomize_to_string(args.remove(0))?;

    let result = match collation.as_ref().custom() {
        None => string_ops::substring_after(&source, &pattern),
        Some(custom) => {
            match collation::collated_find(custom, collation.uri(), &source, &pattern)? {
                Some((_, end)) => collation::slice_at(&source, end..source.len())?.to_string(),
                None => String::new(),
            }
        }
    };
    Ok(XPathValue::string(result))
}

/// fn:string-to-codepoints($arg as xs:string?) as xs:integer*
///
/// Converts a string to a sequence of codepoints.
pub fn string_to_codepoints<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "string-to-codepoints",
            1,
            args.len(),
        ));
    }

    let source = atomize_to_string_strict_opt(args.remove(0))?;

    match source {
        None => Ok(XPathValue::empty()),
        Some(ref s) if s.is_empty() => Ok(XPathValue::empty()),
        Some(s) => {
            let codepoints = string_ops::string_to_codepoints(&s);
            let items: Vec<XmlItem<N>> = codepoints
                .into_iter()
                .map(|cp| XmlItem::Atomic(XmlValue::integer(cp.into())))
                .collect();
            Ok(XPathValue::from_sequence(items))
        }
    }
}

/// fn:codepoints-to-string($arg as xs:integer*) as xs:string
///
/// Converts a sequence of codepoints to a string.
///
/// The declared parameter type is `xs:integer*`, so the function conversion
/// rules of XPath 2.0 §3.1.5 accept an `xs:integer`, any type derived from it,
/// and an `xs:untypedAtomic` item — which step 2 casts to `xs:integer` — and
/// nothing else. In particular no other numeric type is admitted: step 3
/// promotes `xs:decimal` and `xs:float` *to* `xs:double` and never a numeric
/// item down to `xs:integer` (§B.1 Type Promotion), so a whole-valued
/// `xs:decimal` or `xs:double` reaches the closing rule — "If, after the above
/// conversions, the resulting value does not match the expected type according
/// to the rules for SequenceType Matching, a type error is raised
/// \[err:XPTY0004\]" — and is rejected. `codepoints-to-string(65.0)`, whose
/// argument is an `xs:decimal` literal, and `codepoints-to-string(xs:double(65))`
/// are both `XPTY0004`.
pub fn codepoints_to_string<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "codepoints-to-string",
            1,
            args.len(),
        ));
    }

    // Function conversion rules, XPath 2.0 §3.1.5: atomize the argument, cast
    // every xs:untypedAtomic item to the expected type, xs:integer, and raise
    // XPTY0004 for an item that is not an xs:integer afterwards.
    let values = atomize_sequence(args.remove(0))?;

    if values.is_empty() {
        return Ok(XPathValue::string(""));
    }

    let mut codepoints = Vec::with_capacity(values.len());
    for value in values {
        let value = convert::expect_atomic_as(value, XmlTypeCode::Integer, "codepoints-to-string")?;
        codepoints.push(atomize_to_codepoint(&value)?);
    }

    match string_ops::codepoints_to_string(&codepoints) {
        Some(s) => Ok(XPathValue::string(s)),
        None => Err(XPathError::FOCH0001 {
            codepoint: "invalid".to_string(),
        }),
    }
}

/// Check if a codepoint is a valid XML character per XML 1.0 spec.
///
/// Valid XML chars: #x9 | #xA | #xD | [#x20-#xD7FF] | [#xE000-#xFFFD] | [#x10000-#x10FFFF]
fn is_valid_xml_char(cp: u32) -> bool {
    matches!(cp,
        0x9 | 0xA | 0xD |
        0x20..=0xD7FF |
        0xE000..=0xFFFD |
        0x10000..=0x10FFFF
    )
}

/// Convert one already-converted `xs:integer` argument item to a codepoint.
///
/// The caller has applied the function conversion rules, so the value is an
/// `xs:integer` or a type derived from it. A value outside the range of a valid
/// XML character — negative, past `#x10FFFF`, or one of the excluded ranges of
/// the `Char` production — is `FOCH0001`.
fn atomize_to_codepoint(value: &XmlValue) -> Result<u32, XPathError> {
    let i = value.as_integer().ok_or_else(|| XPathError::XPTY0004 {
        expected: "xs:integer".to_string(),
        found: format!("{:?}", value.type_code),
    })?;
    let cp: u32 = i.try_into().map_err(|_| XPathError::FOCH0001 {
        codepoint: i.to_string(),
    })?;
    if !is_valid_xml_char(cp) {
        return Err(XPathError::FOCH0001 {
            codepoint: cp.to_string(),
        });
    }
    Ok(cp)
}

/// fn:compare($comparand1 as xs:string?, $comparand2 as xs:string?) as xs:integer?
/// fn:compare($comparand1 as xs:string?, $comparand2 as xs:string?, $collation as xs:string) as xs:integer?
///
/// Compares two strings under a collation: -1, 0 or 1 as `$comparand1` sorts
/// before, equal to or after `$comparand2`. With no `$collation` argument the
/// static context's default collation is used (F&O §7.3.2).
pub fn compare<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(XPathError::wrong_number_of_arguments(
            "compare",
            2,
            args.len(),
        ));
    }

    let has_collation_arg = args.len() == 3;
    let collation = call_collation(context, &mut args, has_collation_arg)?;
    let s1 = atomize_to_string_opt(args.remove(0))?;
    let s2 = atomize_to_string_opt(args.remove(0))?;

    match (s1, s2) {
        (Some(a), Some(b)) => {
            let result = match collation.as_ref().custom() {
                None => string_ops::compare(&a, &b),
                Some(custom) => match custom.compare(&a, &b) {
                    std::cmp::Ordering::Less => -1,
                    std::cmp::Ordering::Equal => 0,
                    std::cmp::Ordering::Greater => 1,
                },
            };
            Ok(XPathValue::integer(result as i64))
        }
        _ => Ok(XPathValue::empty()),
    }
}

/// fn:codepoint-equal($comparand1 as xs:string?, $comparand2 as xs:string?) as xs:boolean?
///
/// Compares two strings by codepoint.
///
/// This function has no `$collation` argument and takes no collation from the
/// static context: F&O §7.3.3 defines it as `fn:compare` under the Unicode
/// codepoint collation, whatever the default collation is.
pub fn codepoint_equal<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 2 {
        return Err(XPathError::wrong_number_of_arguments(
            "codepoint-equal",
            2,
            args.len(),
        ));
    }

    let s1 = atomize_to_string_strict_opt(args.remove(0))?;
    let s2 = atomize_to_string_strict_opt(args.remove(0))?;

    match (s1, s2) {
        (Some(a), Some(b)) => {
            let result = string_ops::codepoint_equal(&a, &b);
            Ok(XPathValue::boolean(result))
        }
        _ => Ok(XPathValue::empty()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespace::table::NameTable;
    use crate::xpath::context::XPathContext;
    use crate::xpath::RoXmlNavigator;

    fn make_context() -> (NameTable, DynamicContext<'static, RoXmlNavigator<'static>>) {
        let table = Box::leak(Box::new(NameTable::new()));
        let static_ctx = Box::leak(Box::new(XPathContext::new(table)));
        let dyn_ctx = DynamicContext::new(static_ctx, 0);
        (NameTable::new(), dyn_ctx)
    }

    #[test]
    fn test_concat() {
        let (_, mut ctx) = make_context();
        let args = vec![
            XPathValue::string("Hello"),
            XPathValue::string(", "),
            XPathValue::string("World!"),
        ];
        let result = concat(&mut ctx, args).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(v.as_string().unwrap(), "Hello, World!");
            }
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_string_join() {
        let (_, mut ctx) = make_context();
        let seq = XPathValue::from_sequence(vec![
            XmlItem::Atomic(XmlValue::string("a")),
            XmlItem::Atomic(XmlValue::string("b")),
            XmlItem::Atomic(XmlValue::string("c")),
        ]);
        let args = vec![seq, XPathValue::string("-")];
        let result = string_join(&mut ctx, args).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(v.as_string().unwrap(), "a-b-c");
            }
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_substring() {
        let (_, mut ctx) = make_context();
        let args = vec![
            XPathValue::string("hello"),
            XPathValue::double(2.0),
            XPathValue::double(3.0),
        ];
        let result = substring(&mut ctx, args).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(v.as_string().unwrap(), "ell");
            }
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_string_length() {
        let (_, mut ctx) = make_context();
        let args = vec![XPathValue::string("hello")];
        let result = string_length(&mut ctx, args).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(*v.as_integer().unwrap(), 5.into());
            }
            _ => panic!("Expected integer"),
        }
    }

    #[test]
    fn test_upper_lower_case() {
        let (_, mut ctx) = make_context();

        let args = vec![XPathValue::string("Hello")];
        let result = upper_case(&mut ctx, args).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(v.as_string().unwrap(), "HELLO");
            }
            _ => panic!("Expected string"),
        }

        let args = vec![XPathValue::string("Hello")];
        let result = lower_case(&mut ctx, args).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(v.as_string().unwrap(), "hello");
            }
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_contains() {
        let (_, mut ctx) = make_context();
        let args = vec![
            XPathValue::string("hello world"),
            XPathValue::string("world"),
        ];
        let result = contains(&mut ctx, args).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert!(v.as_boolean().unwrap());
            }
            _ => panic!("Expected boolean"),
        }
    }

    #[test]
    fn test_starts_ends_with() {
        let (_, mut ctx) = make_context();

        let args = vec![XPathValue::string("hello"), XPathValue::string("he")];
        let result = starts_with(&mut ctx, args).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert!(v.as_boolean().unwrap());
            }
            _ => panic!("Expected boolean"),
        }

        let args = vec![XPathValue::string("hello"), XPathValue::string("lo")];
        let result = ends_with(&mut ctx, args).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert!(v.as_boolean().unwrap());
            }
            _ => panic!("Expected boolean"),
        }
    }

    #[test]
    fn test_encode_for_uri() {
        let (_, mut ctx) = make_context();
        let args = vec![XPathValue::string("hello world")];
        let result = encode_for_uri(&mut ctx, args).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(v.as_string().unwrap(), "hello%20world");
            }
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_string_to_codepoints() {
        let (_, mut ctx) = make_context();
        let args = vec![XPathValue::string("ABC")];
        let result = string_to_codepoints(&mut ctx, args).unwrap();
        let items = result.into_vec();
        assert_eq!(items.len(), 3);
        match &items[0] {
            XmlItem::Atomic(v) => assert_eq!(*v.as_integer().unwrap(), 65.into()),
            _ => panic!("Expected integer"),
        }
    }

    #[test]
    fn test_codepoints_to_string() {
        let (_, mut ctx) = make_context();
        let seq = XPathValue::from_sequence(vec![
            XmlItem::Atomic(XmlValue::integer(65.into())),
            XmlItem::Atomic(XmlValue::integer(66.into())),
            XmlItem::Atomic(XmlValue::integer(67.into())),
        ]);
        let args = vec![seq];
        let result = codepoints_to_string(&mut ctx, args).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(v.as_string().unwrap(), "ABC");
            }
            _ => panic!("Expected string"),
        }
    }

    /// A whole-valued `xs:double` is *not* an `xs:integer`.
    ///
    /// The declared parameter type is `xs:integer*`. XPath 2.0 §3.1.5 casts only
    /// `xs:untypedAtomic` items to it, and its numeric promotion goes the other
    /// way — `xs:decimal` and `xs:float` are promoted *to* `xs:double`, never a
    /// numeric item down to `xs:integer` — so the closing rule applies: "If,
    /// after the above conversions, the resulting value does not match the
    /// expected type according to the rules for SequenceType Matching, a type
    /// error is raised [err:XPTY0004]."
    #[test]
    fn test_codepoints_to_string_from_doubles_is_a_type_error() {
        let (_, mut ctx) = make_context();
        let seq = XPathValue::from_sequence(vec![
            XmlItem::Atomic(XmlValue::double(65.0)),
            XmlItem::Atomic(XmlValue::double(66.0)),
            XmlItem::Atomic(XmlValue::double(67.0)),
        ]);
        let err = match codepoints_to_string(&mut ctx, vec![seq]) {
            Err(err) => err,
            Ok(_) => panic!("an xs:double is not an xs:integer"),
        };
        assert_eq!(err.error_code(), Some("XPTY0004"));
        assert_eq!(
            err.to_string(),
            "[XPTY0004] Type mismatch: expected 'xs:integer', found 'Double'"
        );
    }

    /// The same rule rejects an `xs:decimal`, which is what the literal `65.0`
    /// is, and an `xs:float`.
    #[test]
    fn test_codepoints_to_string_from_decimal_or_float_is_a_type_error() {
        let (_, mut ctx) = make_context();
        for arg in [
            XmlValue::decimal(rust_decimal::Decimal::from(65)),
            XmlValue::float(65.0),
        ] {
            let found = format!("{:?}", arg.type_code);
            let seq = XPathValue::from_sequence(vec![XmlItem::Atomic(arg)]);
            let err = match codepoints_to_string(&mut ctx, vec![seq]) {
                Err(err) => err,
                Ok(_) => panic!("only xs:integer and its subtypes are accepted: {found}"),
            };
            assert_eq!(err.error_code(), Some("XPTY0004"), "{found}");
        }
    }

    /// An `xs:integer` subtype is accepted: SequenceType matching "permit[s] a
    /// value of a derived type to be substituted for a value of its base type".
    #[test]
    fn test_codepoints_to_string_from_an_integer_subtype() {
        let (_, mut ctx) = make_context();
        let mut short = XmlValue::integer(65.into());
        short.type_code = XmlTypeCode::Short;
        let seq = XPathValue::from_sequence(vec![XmlItem::Atomic(short)]);
        let result = codepoints_to_string(&mut ctx, vec![seq]).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => assert_eq!(v.as_string().unwrap(), "A"),
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_codepoints_to_string_fractional_double_fails() {
        let (_, mut ctx) = make_context();
        // Use a double with a fractional part - should fail
        let seq = XPathValue::from_sequence(vec![XmlItem::Atomic(XmlValue::double(65.5))]);
        let args = vec![seq];
        let result = codepoints_to_string(&mut ctx, args);
        assert!(result.is_err());
    }

    #[test]
    fn test_codepoints_to_string_empty() {
        let (_, mut ctx) = make_context();
        let seq = XPathValue::<RoXmlNavigator>::Empty;
        let args = vec![seq];
        let result = codepoints_to_string(&mut ctx, args).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(v.as_string().unwrap(), "");
            }
            _ => panic!("Expected empty string"),
        }
    }

    #[test]
    fn test_compare() {
        let (_, mut ctx) = make_context();
        let args = vec![XPathValue::string("abc"), XPathValue::string("abd")];
        let result = compare(&mut ctx, args).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(*v.as_integer().unwrap(), (-1).into());
            }
            _ => panic!("Expected integer"),
        }
    }

    #[test]
    fn test_codepoint_equal() {
        let (_, mut ctx) = make_context();
        let args = vec![XPathValue::string("abc"), XPathValue::string("abc")];
        let result = codepoint_equal(&mut ctx, args).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert!(v.as_boolean().unwrap());
            }
            _ => panic!("Expected boolean"),
        }

        let args = vec![XPathValue::string("abc"), XPathValue::string("ABC")];
        let result = codepoint_equal(&mut ctx, args).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert!(!v.as_boolean().unwrap());
            }
            _ => panic!("Expected boolean"),
        }
    }

    #[test]
    fn test_compare_with_default_collation() {
        let (_, mut ctx) = make_context();
        // Should work with default collation
        let args = vec![
            XPathValue::string("abc"),
            XPathValue::string("abd"),
            XPathValue::string(crate::xpath::collation::CODEPOINT_COLLATION_URI),
        ];
        let result = compare(&mut ctx, args).unwrap();
        match result {
            XPathValue::Item(XmlItem::Atomic(v)) => {
                assert_eq!(*v.as_integer().unwrap(), (-1).into());
            }
            _ => panic!("Expected integer"),
        }
    }

    #[test]
    fn test_compare_with_invalid_collation() {
        let (_, mut ctx) = make_context();
        // Should fail with unsupported collation
        let args = vec![
            XPathValue::string("abc"),
            XPathValue::string("abd"),
            XPathValue::string("http://example.com/custom-collation"),
        ];
        let result = compare(&mut ctx, args);
        assert!(matches!(result, Err(XPathError::FOCH0002 { .. })));
    }
}
