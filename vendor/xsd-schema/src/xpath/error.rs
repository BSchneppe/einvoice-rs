//! XPath error types.
//!
//! This module defines XPath 2.0 specification error codes as per the W3C XPath 2.0
//! specification. Error codes follow the pattern:
//! - XPST: Static errors detected during parsing/analysis
//! - XPDY: Dynamic errors detected during evaluation
//! - XPTY: Type errors
//! - XQTY: XQuery type errors
//! - FORG: Function and operators errors (general)
//! - FOAR: Arithmetic errors
//! - FOCA: Casting errors
//! - FONS: Namespace errors
//! - FODT: Date/time errors

use std::fmt;
use std::sync::Arc;

use thiserror::Error;

use crate::types::value::XmlValue;

/// XPath-specific error type with W3C specification error codes.
///
/// Most variants stand for one specification code each. An error that is
/// identified by an **error QName** instead — what `fn:error` raises, what a
/// host's own function raises with a code of its choosing, and the
/// specification codes that have no variant of their own — is
/// [`XPathError::Raised`].
///
/// The enum is `#[non_exhaustive]`: later releases may add variants (a code
/// that travels as [`XPathError::Raised`] today may get one), so a `match`
/// outside this crate needs a wildcard arm. Code that needs the specification
/// code of any error should ask [`XPathError::error_code`] and
/// [`XPathError::raised_error`] rather than match variants.
#[derive(Debug, Clone, Error)]
#[non_exhaustive]
pub enum XPathError {
    // ========================================================================
    // Static Errors (XPST)
    // ========================================================================
    /// XPST0003: Syntax error in expression.
    #[error("[XPST0003] Syntax error: {message}")]
    XPST0003 { message: String },

    /// XPST0008: QName is not defined (undefined variable or type).
    #[error("[XPST0008] QName '{qname}' is not defined")]
    XPST0008 { qname: String },

    /// XPST0051: Unknown atomic type in sequence type.
    #[error("[XPST0051] The type name '{type_name}' is not defined as an atomic type")]
    XPST0051 { type_name: String },

    /// XPST0017: Function not found.
    #[error("[XPST0017] Function '{name}/{arity}' not found in namespace '{namespace}'")]
    XPST0017 {
        name: String,
        arity: usize,
        namespace: String,
    },

    /// XPST0080: the target type of a `cast as` or `castable as` expression is
    /// `xs:NOTATION` or `xs:anyAtomicType`.
    ///
    /// XPath 2.0 §3.10.2: "The target type must be an atomic type that is in
    /// the in-scope schema types \[err:XPST0051\]. In addition, the target
    /// type cannot be xs:NOTATION or xs:anyAtomicType \[err:XPST0080\]."
    /// §3.10.3 says the same of `castable`. A static error: the binder raises
    /// it when the expression is compiled.
    #[error(
        "[XPST0080] '{type_name}' cannot be the target type of a cast or castable \
         expression: xs:NOTATION and xs:anyAtomicType are not instantiable"
    )]
    XPST0080 {
        /// The target type as the expression spells it.
        type_name: String,
    },

    /// XPST0081: Prefix cannot be expanded to namespace URI.
    #[error("[XPST0081] Prefix '{prefix}' cannot be expanded to a namespace URI")]
    XPST0081 { prefix: String },

    // ========================================================================
    // Dynamic Errors (XPDY)
    // ========================================================================
    /// XPDY0002: Context item is undefined (with message).
    #[error("[XPDY0002] {message}")]
    XPDY0002 { message: String },

    /// XPDY0050: the operand of a `treat as` expression does not match its
    /// sequence type, or a path starts with `/` or `//` in a tree whose root
    /// is not a document node.
    ///
    /// XPath 2.0 Appendix G: "It is a dynamic error if the dynamic type of the
    /// operand of a treat expression does not match the sequence type specified
    /// by the treat expression. This error might also be raised by a path
    /// expression beginning with "/" or "//" if the context node is not in a
    /// tree that is rooted at a document node." The message says which, and
    /// for `treat as` what was expected and what was found.
    ///
    /// A value of more than one item where one is expected is not this code
    /// but `XPTY0004` (§3.1.5); see [`XPathError::more_than_one_item`].
    #[error("[XPDY0050] {message}")]
    XPDY0050 {
        /// What did not match.
        message: String,
    },

    // ========================================================================
    // Type Errors (XPTY)
    // ========================================================================
    /// XPTY0004: Type mismatch.
    #[error("[XPTY0004] Type mismatch: expected '{expected}', found '{found}'")]
    XPTY0004 { expected: String, found: String },

    /// XPTY0004 variant: Only string literals can be cast to certain types.
    #[error("[XPTY0004] Only string literals can be cast to type '{target_type}'")]
    XPTY0004Cast { target_type: String },

    /// XPTY0018: Path expression result contains both nodes and atomic values.
    #[error("[XPTY0018] Path expression result contains both nodes and atomic values")]
    XPTY0018,

    /// XPTY0019: Step result must not be atomic value in path expression.
    #[error("[XPTY0019] Step result in path expression must not be an atomic value")]
    XPTY0019,

    /// XPTY0020: Context item in axis step is not a node.
    #[error("[XPTY0020] Context item for axis step is not a node")]
    XPTY0020,

    // ========================================================================
    // XQuery Type Errors (XQTY)
    // ========================================================================
    /// XQTY0030: Validate expression argument must be single document or element.
    #[error(
        "[XQTY0030] Validate expression argument must be exactly one document or element node"
    )]
    XQTY0030,

    // ========================================================================
    // XQuery Static Errors (XQST)
    // ========================================================================
    /// XQST0076: Invalid collation URI.
    #[error("[XQST0076] Collation '{collation}' is not supported")]
    XQST0076 { collation: String },

    // ========================================================================
    // Function Errors - General (FORG)
    // ========================================================================
    /// FORG0001: Invalid value for cast/constructor.
    #[error("[FORG0001] Invalid value '{value}' for cast to type '{target_type}'")]
    FORG0001 { value: String, target_type: String },

    /// FORG0003: fn:zero-or-one called with sequence > 1 item.
    #[error("[FORG0003] fn:zero-or-one called with sequence containing more than one item")]
    FORG0003,

    /// FORG0004: fn:one-or-more called with empty sequence.
    #[error("[FORG0004] fn:one-or-more called with empty sequence")]
    FORG0004,

    /// FORG0005: fn:exactly-one called with wrong cardinality.
    #[error(
        "[FORG0005] fn:exactly-one called with sequence containing zero or more than one item"
    )]
    FORG0005,

    /// FORG0006: Invalid argument type for function.
    #[error("[FORG0006] Function '{function}' called with invalid argument type '{arg_type}'")]
    FORG0006Named { function: String, arg_type: String },

    /// FORG0006: General effective boolean value error.
    #[error("[FORG0006] {message}")]
    FORG0006 { message: String },

    /// FORG0008: Both arguments to fn:dateTime have a timezone
    #[error("[FORG0008] Both arguments to fn:dateTime have a timezone")]
    FORG0008,

    // ========================================================================
    // Character/Codepoint Errors (FOCH)
    // ========================================================================
    /// FOCH0001: Invalid codepoint.
    #[error("[FOCH0001] Invalid codepoint '{codepoint}' in codepoints-to-string")]
    FOCH0001 { codepoint: String },

    /// FOCH0002: Unsupported collation.
    #[error("[FOCH0002] Unsupported collation: '{collation}'")]
    FOCH0002 { collation: String },

    /// FOCH0003: Unsupported normalization form.
    #[error("[FOCH0003] Unsupported normalization form '{normalization_form}'")]
    FOCH0003 { normalization_form: String },

    // ========================================================================
    // Arithmetic Errors (FOAR)
    // ========================================================================
    /// FOAR0001: Division by zero.
    #[error("[FOAR0001] Division by zero")]
    FOAR0001,

    /// FOAR0002: Numeric overflow or underflow.
    #[error("[FOAR0002] Numeric operation overflow/underflow")]
    FOAR0002,

    // ========================================================================
    // Casting Errors (FOCA)
    // ========================================================================
    /// FOCA0002: invalid lexical value — where an `xs:QName` is required,
    /// either the wrong lexical form or a prefix with no namespace; or a
    /// special `xs:float` / `xs:double` value (`NaN`, `INF`, `-INF`) cast to
    /// `xs:decimal`, `xs:integer` or a type derived from them (F&O §17.1.3.3,
    /// §17.1.3.4: "If SV is one of the special xs:float or xs:double values
    /// NaN, INF, or -INF, an error is raised \[err:FOCA0002\]"). `value` holds
    /// the offending lexical form.
    #[error("[FOCA0002] Invalid lexical value: '{value}'")]
    FOCA0002 { value: String },

    /// FOCA0003: Input value too large for integer.
    #[error("[FOCA0003] {message}")]
    FOCA0003 { message: String },

    /// FOCA0005: NaN supplied as float/double value.
    #[error("[FOCA0005] NaN supplied as float/double value")]
    FOCA0005,

    // ========================================================================
    // Date/Time Errors (FODT)
    // ========================================================================
    /// FODT0001: Overflow/underflow in date/time operation.
    #[error("[FODT0001] Overflow/underflow in date/time operation")]
    FODT0001,

    /// FODT0002: Overflow/underflow in duration operation.
    #[error("[FODT0002] Overflow/underflow in duration operation")]
    FODT0002,

    /// FODT0003: Invalid timezone value.
    #[error("[FODT0003] Invalid timezone value: {value}")]
    FODT0003 { value: String },

    // ========================================================================
    // XDM Typed Value Errors (FOTY)
    // ========================================================================
    /// FOTY0012: Argument node does not have a typed value.
    #[error("[FOTY0012] Argument node does not have a typed value")]
    FOTY0012,

    // ========================================================================
    // Namespace Errors (FONS)
    // ========================================================================
    /// FONS0005: Base-uri not defined in static context.
    #[error("[FONS0005] Base-uri not defined in the static context")]
    FONS0005,

    // ========================================================================
    // URI Errors (FORG)
    // ========================================================================
    /// FORG0009: Error in resolving a relative URI.
    #[error("[FORG0009] Error in resolving a relative URI against a base URI: '{uri}'")]
    FORG0009 { uri: String },

    // ========================================================================
    // Regex Errors (FORX)
    // ========================================================================
    /// FORX0001: Invalid regular expression flags.
    #[error("[FORX0001] Invalid regular expression flags: '{flags}'")]
    FORX0001 { flags: String },

    /// FORX0002: Invalid regular expression.
    #[error("[FORX0002] Invalid regular expression: '{pattern}'")]
    FORX0002 { pattern: String },

    /// FORX0003: Regular expression matches zero-length string.
    #[error("[FORX0003] Regular expression matches zero-length string: '{pattern}'")]
    FORX0003 { pattern: String },

    /// FORX0004: Invalid replacement string.
    #[error("[FORX0004] Invalid replacement string: '{replacement}'")]
    FORX0004 { replacement: String },

    // ========================================================================
    // Operator Errors
    // ========================================================================
    /// Binary operator not defined for argument types.
    ///
    /// This is the XPath 2.0 §3.4 / §3.5.1 type error: "If the types of the
    /// operands, after evaluation, are not a valid combination for the given
    /// operator, according to the rules in B.2 Operator Mapping, a type error is
    /// raised \[err:XPTY0004\]." The dedicated variant carries the operator and
    /// the two type names for the message; its
    /// [`error_code`](XPathError::error_code) is `XPTY0004`.
    #[error("[XPTY0004] Operator '{operator}' is not defined for arguments of type '{left_type}' and '{right_type}'")]
    BinaryOperatorNotDefined {
        operator: String,
        left_type: String,
        right_type: String,
    },

    /// Unary operator not defined for argument type.
    ///
    /// The unary counterpart of
    /// [`BinaryOperatorNotDefined`](XPathError::BinaryOperatorNotDefined); its
    /// [`error_code`](XPathError::error_code) is likewise `XPTY0004`.
    #[error("[XPTY0004] Operator '{operator}' is not defined for argument of type '{arg_type}'")]
    UnaryOperatorNotDefined { operator: String, arg_type: String },

    // ========================================================================
    // Errors identified by an error QName
    // ========================================================================
    /// An error identified by an **error QName** rather than by a variant of
    /// its own.
    ///
    /// Three kinds of error travel here:
    ///
    /// * what `fn:error` raises — any QName the expression chooses, with its
    ///   description and error object (F&O §3);
    /// * an error a **host** raises from a function it registered, with a code
    ///   of its own and, optionally, a payload of its own type that the engine
    ///   carries through unchanged ([`XPathError::host`],
    ///   [`XPathError::downcast_host_payload`]);
    /// * the specification codes that have no variant of their own, such as
    ///   `FONS0004` (see [`QNAMED_ERROR_CODES`]).
    ///
    /// Build one with [`XPathError::raised`] or [`XPathError::host`]; read it
    /// with [`XPathError::raised_error`] or through the [`RaisedErrorInfo`]
    /// accessors. [`XPathError::error_code`] reports the codes of
    /// [`QNAMED_ERROR_CODES`].
    ///
    /// Its [`Error::source`](std::error::Error::source) is the host payload,
    /// when there is one.
    #[error(transparent)]
    Raised(RaisedErrorInfo),

    // ========================================================================
    // Implementation limits
    // ========================================================================
    /// The expression nests deeper than
    /// [`MAX_EXPRESSION_DEPTH`](crate::xpath::depth::MAX_EXPRESSION_DEPTH),
    /// the limit that keeps binding and evaluation, which recurse over the
    /// expression tree, from overflowing the stack. XPath 2.0 §2.3.1 leaves
    /// such limits to the implementation, so it has no error code
    /// ([`error_code`](XPathError::error_code) is `None`).
    #[error(
        "XPath limit xpath::depth::MAX_EXPRESSION_DEPTH exceeded: nesting depth {found}, \
         the limit is {max}"
    )]
    #[non_exhaustive]
    LimitExceeded {
        /// The limit.
        max: usize,
        /// The nesting depth of the expression (greater than `max`).
        found: usize,
    },

    // ========================================================================
    // Internal/General Errors
    // ========================================================================
    /// Internal error for unexpected failures.
    #[error("XPath error: {0}")]
    Internal(String),
}

impl From<crate::xpath::parser::ParseError> for XPathError {
    fn from(e: crate::xpath::parser::ParseError) -> Self {
        match e {
            crate::xpath::parser::ParseError::LimitExceeded { max, found } => {
                XPathError::LimitExceeded { max, found }
            }
            e => XPathError::XPST0003 {
                message: e.to_string(),
            },
        }
    }
}

impl From<crate::navigator::NavigatorError> for XPathError {
    fn from(e: crate::navigator::NavigatorError) -> Self {
        XPathError::Internal(e.to_string())
    }
}

impl XPathError {
    // ========================================================================
    // Convenience Constructors
    // ========================================================================

    /// Create a new internal XPath error.
    pub fn internal(message: impl Into<String>) -> Self {
        XPathError::Internal(message.into())
    }

    /// Create XPST0003 syntax error.
    pub fn syntax_error(message: impl Into<String>) -> Self {
        XPathError::XPST0003 {
            message: message.into(),
        }
    }

    /// Create XPST0008 undefined QName error.
    pub fn undefined_qname(qname: impl Into<String>) -> Self {
        XPathError::XPST0008 {
            qname: qname.into(),
        }
    }

    /// Create XPST0051 unknown type error.
    pub fn unknown_type(type_name: impl Into<String>) -> Self {
        XPathError::XPST0051 {
            type_name: type_name.into(),
        }
    }

    /// Create XPST0017 function not found error.
    pub fn function_not_found(
        name: impl Into<String>,
        arity: usize,
        namespace: impl Into<String>,
    ) -> Self {
        XPathError::XPST0017 {
            name: name.into(),
            arity,
            namespace: namespace.into(),
        }
    }

    /// Create XPST0081 undefined prefix error.
    pub fn undefined_prefix(prefix: impl Into<String>) -> Self {
        XPathError::XPST0081 {
            prefix: prefix.into(),
        }
    }

    /// Create XPDY0002 context undefined error.
    pub fn context_undefined() -> Self {
        XPathError::XPDY0002 {
            message: "The context item is undefined".to_string(),
        }
    }

    /// The type error for a value of more than one item where at most one is
    /// expected.
    ///
    /// XPath 2.0 §3.1.5: "If, after the above conversions, the resulting value
    /// does not match the expected type according to the rules for
    /// SequenceType Matching, a type error is raised \[err:XPTY0004\]"; the
    /// operators state the same rule for their operands (§3.4, §3.5.1,
    /// §3.10.2). Before 0.3.0 this constructor returned `XPDY0050`, which is
    /// the code of a failed `treat as` (§3.10.5) instead.
    pub fn more_than_one_item() -> Self {
        XPathError::XPTY0004 {
            expected: "a single item".to_string(),
            found: "a sequence of more than one item".to_string(),
        }
    }

    /// XPDY0050 for a failed `treat as` (XPath 2.0 §3.10.5): `expected` is the
    /// sequence type of the expression, `found` describes the operand.
    pub fn treat_mismatch(expected: impl fmt::Display, found: impl fmt::Display) -> Self {
        XPathError::XPDY0050 {
            message: format!("treat as {expected}: the operand is {found}"),
        }
    }

    /// XPDY0050 for a path that starts with `/` or `//` where the root of the
    /// context node's tree is not a document node (XPath 2.0 §3.2).
    pub fn root_not_a_document() -> Self {
        XPathError::XPDY0050 {
            message: "a path starting with '/' or '//' needs a tree rooted at a document node, \
                      and the root of the context node's tree is not one"
                .to_string(),
        }
    }

    /// Create XPTY0004 type mismatch error.
    pub fn type_mismatch(expected: impl Into<String>, found: impl Into<String>) -> Self {
        XPathError::XPTY0004 {
            expected: expected.into(),
            found: found.into(),
        }
    }

    /// Create XPTY0004 cast-only-from-string error.
    pub fn cast_requires_string_literal(target_type: impl Into<String>) -> Self {
        XPathError::XPTY0004Cast {
            target_type: target_type.into(),
        }
    }

    /// Create XPTY0020 context item not a node error.
    pub fn context_not_a_node() -> Self {
        XPathError::XPTY0020
    }

    /// Create FORG0001 invalid cast value error.
    pub fn invalid_cast_value(value: impl Into<String>, target_type: impl Into<String>) -> Self {
        XPathError::FORG0001 {
            value: value.into(),
            target_type: target_type.into(),
        }
    }

    /// Create FORG0006 invalid argument type error.
    pub fn invalid_argument_type(function: impl Into<String>, arg_type: impl Into<String>) -> Self {
        XPathError::FORG0006Named {
            function: function.into(),
            arg_type: arg_type.into(),
        }
    }

    /// Create a not implemented error.
    pub fn not_implemented(message: impl Into<String>) -> Self {
        XPathError::Internal(format!("Not implemented: {}", message.into()))
    }

    /// Create XPST0017 wrong number of arguments error.
    pub fn wrong_number_of_arguments(function: &str, expected: usize, actual: usize) -> Self {
        XPathError::XPST0017 {
            name: function.to_string(),
            arity: actual,
            namespace: format!(
                "(expected {} argument{})",
                expected,
                if expected == 1 { "" } else { "s" }
            ),
        }
    }

    /// Create binary operator not defined error.
    pub fn binary_operator_not_defined(
        operator: impl Into<String>,
        left_type: impl Into<String>,
        right_type: impl Into<String>,
    ) -> Self {
        XPathError::BinaryOperatorNotDefined {
            operator: operator.into(),
            left_type: left_type.into(),
            right_type: right_type.into(),
        }
    }

    /// Create unary operator not defined error.
    pub fn unary_operator_not_defined(
        operator: impl Into<String>,
        arg_type: impl Into<String>,
    ) -> Self {
        XPathError::UnaryOperatorNotDefined {
            operator: operator.into(),
            arg_type: arg_type.into(),
        }
    }

    /// Create FOTY0012 no typed value error.
    pub fn no_typed_value() -> Self {
        XPathError::FOTY0012
    }

    /// Create FONS0005 base-uri not defined error.
    pub fn base_uri_not_defined() -> Self {
        XPathError::FONS0005
    }

    /// Create FORG0009 URI resolution error.
    pub fn uri_resolution_error(uri: impl Into<String>) -> Self {
        XPathError::FORG0009 { uri: uri.into() }
    }

    /// Create FORX0001 invalid regex flags error.
    pub fn invalid_regex_flags(flags: impl Into<String>) -> Self {
        XPathError::FORX0001 {
            flags: flags.into(),
        }
    }

    /// Create FORX0002 invalid regex pattern error.
    pub fn invalid_regex_pattern(pattern: impl Into<String>) -> Self {
        XPathError::FORX0002 {
            pattern: pattern.into(),
        }
    }

    /// Create FORX0003 regex matches zero-length string error.
    pub fn regex_matches_zero_length(pattern: impl Into<String>) -> Self {
        XPathError::FORX0003 {
            pattern: pattern.into(),
        }
    }

    /// Create FORX0004 invalid replacement string error.
    pub fn invalid_replacement_string(replacement: impl Into<String>) -> Self {
        XPathError::FORX0004 {
            replacement: replacement.into(),
        }
    }

    /// Create XQST0076 unsupported collation error.
    pub fn unsupported_collation(collation: impl Into<String>) -> Self {
        XPathError::XQST0076 {
            collation: collation.into(),
        }
    }

    /// Create FOCH0002 unsupported collation error.
    pub fn unknown_collation(collation: impl Into<String>) -> Self {
        XPathError::FOCH0002 {
            collation: collation.into(),
        }
    }

    /// FOCH0004: the collation does not support collation units.
    ///
    /// Raised by the five substring functions — `fn:contains`,
    /// `fn:starts-with`, `fn:ends-with`, `fn:substring-before`,
    /// `fn:substring-after` — which F&O §7.5 defines over collation units, when
    /// the collation in force has no notion of them (a
    /// [`Collation`](crate::xpath::collation::Collation) that leaves
    /// [`find`](crate::xpath::collation::Collation::find) at its default). The
    /// code has no dedicated variant, so it travels as an error QName; see
    /// [`XPathError::raised`]. [`XPathError::error_code`] answers
    /// `Some("FOCH0004")`.
    pub fn collation_no_units(collation: &str) -> Self {
        XPathError::raised(
            XQT_ERRORS_NAMESPACE,
            "FOCH0004",
            Some(&format!(
                "Collation '{collation}' does not support collation units"
            )),
        )
    }

    /// Build an error identified by an error QName — an
    /// [`XPathError::Raised`].
    ///
    /// `fn:error` may raise *any* error QName, so such an error cannot be one
    /// of this enum's fixed, code-per-variant arms. Read it back with
    /// [`XPathError::raised_error`]. The same variant serves the spec-defined
    /// codes that have no dedicated variant, such as `FONS0004`;
    /// [`XPathError::error_code`] resolves those back to their `'static` code
    /// string.
    ///
    /// `namespace_uri` is empty for an error QName in no namespace.
    pub fn raised(namespace_uri: &str, local_name: &str, description: Option<&str>) -> Self {
        XPathError::Raised(RaisedErrorInfo::new(
            namespace_uri,
            local_name,
            description,
            Vec::new(),
            None,
        ))
    }

    /// An error a **host** raises from a function it registered, identified by
    /// an error QName of its choosing and carrying a payload of the host's own
    /// type.
    ///
    /// The engine treats it like any other [`XPathError::Raised`] — it
    /// propagates unchanged out of the evaluation, and its QName and
    /// description read back through [`XPathError::raised_error`] — and never
    /// looks inside the payload. The host gets its own error back with
    /// [`XPathError::downcast_host_payload`], so it needs no side channel to
    /// carry what an `XPathError` cannot express (a location, an origin, a
    /// type of its own).
    ///
    /// ```
    /// use xsd_schema::xpath::error::{XPathError, XQT_ERRORS_NAMESPACE};
    ///
    /// #[derive(Debug, Clone, PartialEq)]
    /// struct HostError {
    ///     code: String,
    ///     line: u32,
    /// }
    /// impl std::fmt::Display for HostError {
    ///     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    ///         write!(f, "{} at line {}", self.code, self.line)
    ///     }
    /// }
    /// impl std::error::Error for HostError {}
    ///
    /// let mine = HostError { code: "HOST0001".into(), line: 7 };
    /// let err = XPathError::host(XQT_ERRORS_NAMESPACE, "HOST0001", Some("no such thing"), mine.clone());
    ///
    /// assert_eq!(err.raised_error().unwrap().local_name, "HOST0001");
    /// assert_eq!(err.downcast_host_payload::<HostError>(), Some(&mine));
    /// // A clone shares the payload.
    /// assert_eq!(err.clone().downcast_host_payload::<HostError>(), Some(&mine));
    /// ```
    pub fn host<E>(
        namespace_uri: &str,
        local_name: &str,
        description: Option<&str>,
        payload: E,
    ) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        XPathError::Raised(RaisedErrorInfo::new(
            namespace_uri,
            local_name,
            description,
            Vec::new(),
            Some(Arc::new(payload)),
        ))
    }

    /// What `fn:error` raises: an error QName, a description and the error
    /// object, already reduced to atomic values (see
    /// [`RaisedErrorInfo::error_object`]).
    pub(crate) fn raised_with_object(
        namespace_uri: &str,
        local_name: &str,
        description: Option<&str>,
        error_object: Vec<XmlValue>,
    ) -> Self {
        XPathError::Raised(RaisedErrorInfo::new(
            namespace_uri,
            local_name,
            description,
            error_object,
            None,
        ))
    }

    /// The payload a host attached with [`XPathError::host`], if this error
    /// carries one.
    pub fn host_payload(&self) -> Option<&(dyn std::error::Error + Send + Sync + 'static)> {
        match self {
            XPathError::Raised(info) => info.host_payload(),
            _ => None,
        }
    }

    /// The host payload as a `T`, if this error carries one of that type.
    pub fn downcast_host_payload<T: std::error::Error + 'static>(&self) -> Option<&T> {
        self.host_payload()?.downcast_ref::<T>()
    }

    /// FORG0002: an argument that is not a valid URI reference.
    ///
    /// The code has no dedicated variant, so it travels as an error QName; see
    /// [`XPathError::raised`].
    pub fn invalid_uri_argument(uri: &str) -> Self {
        XPathError::raised(
            XQT_ERRORS_NAMESPACE,
            "FORG0002",
            Some(&format!("'{uri}' is not a valid URI reference")),
        )
    }

    /// XPST0080: the target type of a `cast as` or `castable as` expression is
    /// `xs:NOTATION` or `xs:anyAtomicType` (XPath 2.0 §3.10.2, §3.10.3).
    pub(crate) fn cast_target_not_instantiable(type_name: &str) -> Self {
        XPathError::XPST0080 {
            type_name: type_name.to_string(),
        }
    }

    /// FOCA0001: an `xs:float` or `xs:double` value too large for `xs:decimal`.
    ///
    /// F&O 1.0 §17.1.3.3: "If SV is too large to be accommodated as an
    /// xs:decimal, (see \[XML Schema Part 2: Datatypes Second Edition\] for
    /// ·implementation-defined· limits on numeric values) an error is raised
    /// \[err:FOCA0001\]." This crate's `xs:decimal` holds a 96-bit mantissa.
    /// The code has no dedicated variant, so it travels as an error QName; see
    /// [`XPathError::raised`]. [`XPathError::error_code`] answers
    /// `Some("FOCA0001")`.
    pub(crate) fn decimal_too_large(lexical: &str) -> Self {
        XPathError::raised(
            XQT_ERRORS_NAMESPACE,
            "FOCA0001",
            Some(&format!(
                "'{lexical}' is too large for xs:decimal here (a 96-bit mantissa)"
            )),
        )
    }

    /// FOCA0006: a string cast to `xs:decimal` has more digits of precision
    /// than the implementation holds.
    ///
    /// F&O §17.1.1: "In casting to xs:decimal or to a type derived from
    /// xs:decimal, if the value is not too large or too small but nevertheless
    /// cannot be represented accurately with the number of decimal digits
    /// available to the implementation, the implementation may round to the
    /// nearest representable value or may raise a dynamic error
    /// \[err:FOCA0006\]." This crate raises it rather than round: its
    /// `xs:decimal` holds a 96-bit mantissa with at most 28 fraction digits.
    /// A value too small to be held at all — a magnitude below 10^−28 — is an
    /// underflow instead, and casts to 0 (F&O §6.2).
    /// The code has no dedicated variant, so it travels as an error QName; see
    /// [`XPathError::raised`]. [`XPathError::error_code`] answers
    /// `Some("FOCA0006")`.
    pub(crate) fn decimal_too_precise(lexical: &str) -> Self {
        XPathError::raised(
            XQT_ERRORS_NAMESPACE,
            "FOCA0006",
            Some(&format!(
                "'{lexical}' has more digits of precision than xs:decimal holds here \
                 (a 96-bit mantissa with at most 28 fraction digits)"
            )),
        )
    }

    /// Whether this error is how the engine reports an `xs:decimal` beyond its
    /// capacity: `FOAR0002` (overflow — arithmetic on a decimal carried in
    /// lexical form, or a result too large) or `FOCA0006` (too precise to
    /// cast).
    pub(crate) fn is_decimal_capacity_error(&self) -> bool {
        matches!(self, XPathError::FOAR0002)
            || self.is_decimal_capacity_operand()
            || self.raised_error().is_some_and(|raised| {
                raised.namespace_uri == XQT_ERRORS_NAMESPACE && raised.local_name == "FOCA0006"
            })
    }

    /// `FOAR0002` raised because an operand is an `xs:decimal` that this
    /// implementation carries as a string — a valid value beyond what
    /// `rust_decimal::Decimal` holds — while
    /// [`with_decimal_capacity_marks`](crate::xpath::operators::with_decimal_capacity_marks)
    /// is in force.
    ///
    /// The validator evaluates XSD 1.1 assertions that way, so it can tell an
    /// assertion whose outcome is unknown because of the implementation's
    /// capacity (Datatypes 1.1 §5.4) from one that genuinely raised an error.
    /// Outside that scope the same condition is the plain
    /// [`XPathError::FOAR0002`]; this form never reaches a caller of the public
    /// API, and [`XPathError::error_code`] reports it as `FOAR0002` all the
    /// same.
    pub(crate) fn decimal_capacity_operand() -> Self {
        XPathError::Internal(DECIMAL_CAPACITY_OPERAND.to_string())
    }

    /// Whether this is the error [`XPathError::decimal_capacity_operand`]
    /// builds.
    pub(crate) fn is_decimal_capacity_operand(&self) -> bool {
        matches!(self, XPathError::Internal(message) if message == DECIMAL_CAPACITY_OPERAND)
    }

    /// FONS0004: no namespace binding for a prefix.
    ///
    /// The code has no dedicated variant, so it travels as an error QName; see
    /// [`XPathError::raised`].
    pub fn no_namespace_for_prefix(prefix: &str) -> Self {
        XPathError::raised(
            XQT_ERRORS_NAMESPACE,
            "FONS0004",
            Some(&format!("No namespace binding for prefix '{prefix}'")),
        )
    }

    /// The error QName and description of an [`XPathError::Raised`] error.
    ///
    /// Returns `None` for every error that has a variant of its own; see
    /// [`XPathError::raised`].
    pub fn raised_error(&self) -> Option<RaisedError<'_>> {
        match self {
            XPathError::Raised(info) => Some(info.as_raised_error()),
            _ => None,
        }
    }

    /// Get the error code (e.g., "XPTY0004") if this is a spec-defined error.
    ///
    /// An error identified by an error QName (an [`XPathError::Raised`])
    /// reports its code only when the QName is in the
    /// [`XQT_ERRORS_NAMESPACE`] **and** its local name is one of
    /// [`QNAMED_ERROR_CODES`] — the codes this crate itself raises that way.
    /// Every other QName returns `None`, even one that spells a code of the
    /// specifications, such as `fn:error(xs:QName('err:FORG0001'))`, or a
    /// host's own code (its `Display` still prints `[FORG0001]`): the answer is
    /// a `'static` string, and those codes are not known statically. Read any
    /// raised error's QName from [`XPathError::raised_error`] instead.
    pub fn error_code(&self) -> Option<&'static str> {
        match self {
            XPathError::XPST0003 { .. } => Some("XPST0003"),
            XPathError::XPST0008 { .. } => Some("XPST0008"),
            XPathError::XPST0051 { .. } => Some("XPST0051"),
            XPathError::XPST0017 { .. } => Some("XPST0017"),
            XPathError::XPST0080 { .. } => Some("XPST0080"),
            XPathError::XPST0081 { .. } => Some("XPST0081"),
            XPathError::XPDY0002 { .. } => Some("XPDY0002"),
            XPathError::XPDY0050 { .. } => Some("XPDY0050"),
            XPathError::XPTY0004 { .. } => Some("XPTY0004"),
            XPathError::XPTY0004Cast { .. } => Some("XPTY0004"),
            XPathError::XPTY0018 => Some("XPTY0018"),
            XPathError::XPTY0019 => Some("XPTY0019"),
            XPathError::XPTY0020 => Some("XPTY0020"),
            XPathError::XQTY0030 => Some("XQTY0030"),
            XPathError::XQST0076 { .. } => Some("XQST0076"),
            XPathError::FORG0001 { .. } => Some("FORG0001"),
            XPathError::FORG0003 => Some("FORG0003"),
            XPathError::FORG0004 => Some("FORG0004"),
            XPathError::FORG0005 => Some("FORG0005"),
            XPathError::FORG0006Named { .. } => Some("FORG0006"),
            XPathError::FORG0006 { .. } => Some("FORG0006"),
            XPathError::FORG0008 => Some("FORG0008"),
            XPathError::FOCH0001 { .. } => Some("FOCH0001"),
            XPathError::FOCH0002 { .. } => Some("FOCH0002"),
            XPathError::FOCH0003 { .. } => Some("FOCH0003"),
            XPathError::FOAR0001 => Some("FOAR0001"),
            XPathError::FOAR0002 => Some("FOAR0002"),
            XPathError::FOCA0002 { .. } => Some("FOCA0002"),
            XPathError::FOCA0003 { .. } => Some("FOCA0003"),
            XPathError::FOCA0005 => Some("FOCA0005"),
            XPathError::FODT0001 => Some("FODT0001"),
            XPathError::FODT0002 => Some("FODT0002"),
            XPathError::FODT0003 { .. } => Some("FODT0003"),
            XPathError::FOTY0012 => Some("FOTY0012"),
            XPathError::FONS0005 => Some("FONS0005"),
            XPathError::FORG0009 { .. } => Some("FORG0009"),
            XPathError::FORX0001 { .. } => Some("FORX0001"),
            XPathError::FORX0002 { .. } => Some("FORX0002"),
            XPathError::FORX0003 { .. } => Some("FORX0003"),
            XPathError::FORX0004 { .. } => Some("FORX0004"),
            // XPath 2.0 §3.4 and §3.5.1: an operand combination that no
            // operator mapping covers is a type error.
            XPathError::BinaryOperatorNotDefined { .. } => Some("XPTY0004"),
            XPathError::UnaryOperatorNotDefined { .. } => Some("XPTY0004"),
            XPathError::Internal(_) if self.is_decimal_capacity_operand() => Some("FOAR0002"),
            XPathError::Internal(_) => None,
            XPathError::LimitExceeded { .. } => None,
            XPathError::Raised(info) => {
                if info.namespace_uri() != XQT_ERRORS_NAMESPACE {
                    return None;
                }
                QNAMED_ERROR_CODES
                    .iter()
                    .copied()
                    .find(|code| *code == info.local_name())
            }
        }
    }
}

/// The namespace of the error QNames the specifications define,
/// `http://www.w3.org/2005/xqt-errors`. It is also the namespace of the
/// default error QName of `fn:error`.
pub const XQT_ERRORS_NAMESPACE: &str = "http://www.w3.org/2005/xqt-errors";

/// The local name of the default error QName of `fn:error`.
pub const DEFAULT_RAISED_ERROR: &str = "FOER0000";

/// The message of [`XPathError::decimal_capacity_operand`], an
/// [`XPathError::Internal`]: nothing `fn:error` or a host raises (an
/// [`XPathError::Raised`]) can be taken for it.
const DECIMAL_CAPACITY_OPERAND: &str =
    "[FOAR0002] xs:decimal operand beyond the capacity of this implementation";

/// A warning an evaluation raised: the result is computed, but not with
/// assured correctness (Datatypes 1.1 §5.4: a processor "must indicate to the
/// user and/or downstream application that it cannot process the input data
/// with assured correctness").
///
/// The one warning today is `xs-seconds-capacity` — the code the validator
/// reports for a date/time or duration literal whose seconds have more fraction
/// digits than this implementation holds — for a date/time or duration
/// arithmetic result whose seconds field could not hold the whole fraction, and
/// kept it truncated toward zero. Collect them with
/// [`XPathEvaluator::with_warnings`](crate::xpath::XPathEvaluator::with_warnings)
/// or [`DynamicContext::take_warnings`](crate::xpath::DynamicContext::take_warnings).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XPathWarning {
    code: &'static str,
    message: String,
}

impl XPathWarning {
    pub(crate) fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// The warning code, e.g. `"xs-seconds-capacity"`.
    pub fn code(&self) -> &'static str {
        self.code
    }

    /// What happened, for a person.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for XPathWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

/// The spec-defined codes this crate reports through an error QName — an
/// [`XPathError::Raised`] in the [`XQT_ERRORS_NAMESPACE`] — rather than
/// through a variant of their own, and which [`XPathError::error_code`]
/// therefore resolves back to a `'static` string.
///
/// (`XPST0080` was on this list before 0.3.0; it is now
/// [`XPathError::XPST0080`].)
pub const QNAMED_ERROR_CODES: &[&str] = &[
    DEFAULT_RAISED_ERROR,
    "FOCA0001",
    "FOCA0006",
    "FOCH0004",
    "FODC0001",
    "FONS0004",
    "FORG0002",
];

/// What an [`XPathError::Raised`] error carries: the error QName, the
/// description, the error object of `fn:error`, and a host's payload.
///
/// The fields are private so that later releases can add to them; read them
/// with the accessors. Cloning is cheap: the data lives behind one shared,
/// reference-counted allocation, so a clone (and a clone of the
/// [`XPathError`] holding it) copies a pointer. Its
/// [`Error::source`](std::error::Error::source) is the host payload.
#[derive(Clone)]
pub struct RaisedErrorInfo {
    inner: Arc<RaisedErrorData>,
}

struct RaisedErrorData {
    namespace_uri: String,
    local_name: String,
    description: Option<String>,
    error_object: Vec<XmlValue>,
    payload: Option<Arc<dyn std::error::Error + Send + Sync + 'static>>,
}

impl RaisedErrorInfo {
    fn new(
        namespace_uri: &str,
        local_name: &str,
        description: Option<&str>,
        error_object: Vec<XmlValue>,
        payload: Option<Arc<dyn std::error::Error + Send + Sync + 'static>>,
    ) -> Self {
        Self {
            inner: Arc::new(RaisedErrorData {
                namespace_uri: namespace_uri.to_string(),
                local_name: local_name.to_string(),
                description: description.map(str::to_string),
                error_object,
                payload,
            }),
        }
    }

    /// Namespace URI of the error QName (`""` for no namespace).
    pub fn namespace_uri(&self) -> &str {
        &self.inner.namespace_uri
    }

    /// Local name of the error QName.
    pub fn local_name(&self) -> &str {
        &self.inner.local_name
    }

    /// The description (`fn:error`'s `$description`), if one was supplied.
    pub fn description(&self) -> Option<&str> {
        self.inner.description.as_deref()
    }

    /// The error object of `fn:error($error, $description, $error-object)`,
    /// as atomic values; empty for every other error.
    ///
    /// F&O §3 leaves how the error object reaches the environment
    /// implementation-dependent. An `XPathError` outlives the documents of the
    /// evaluation that raised it, so it cannot hold their nodes: an atomic item
    /// of the error object is kept as it is, and a node is represented by its
    /// string value as an `xs:untypedAtomic`.
    pub fn error_object(&self) -> &[XmlValue] {
        &self.inner.error_object
    }

    /// The payload a host attached with [`XPathError::host`], if any.
    pub fn host_payload(&self) -> Option<&(dyn std::error::Error + Send + Sync + 'static)> {
        self.inner.payload.as_deref()
    }

    /// Whether `self` and `other` are clones of one error, sharing its data.
    #[cfg(test)]
    pub(crate) fn shares_data_with(&self, other: &RaisedErrorInfo) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    /// The borrowed view [`XPathError::raised_error`] returns.
    pub fn as_raised_error(&self) -> RaisedError<'_> {
        RaisedError {
            namespace_uri: self.namespace_uri(),
            local_name: self.local_name(),
            description: self.description(),
        }
    }
}

impl fmt::Debug for RaisedErrorInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RaisedErrorInfo")
            .field("namespace_uri", &self.inner.namespace_uri)
            .field("local_name", &self.inner.local_name)
            .field("description", &self.inner.description)
            .field("error_object", &self.inner.error_object)
            .field("host_payload", &self.inner.payload)
            .finish()
    }
}

/// The host payload, when there is one, is the source.
impl std::error::Error for RaisedErrorInfo {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.inner
            .payload
            .as_deref()
            .map(|payload| payload as &(dyn std::error::Error + 'static))
    }
}

/// `[code] description`: the local name alone as the code for a QName in the
/// [`XQT_ERRORS_NAMESPACE`], the EQName `Q{namespace}local` for any other.
impl fmt::Display for RaisedErrorInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.namespace_uri() == XQT_ERRORS_NAMESPACE {
            write!(f, "[{}]", self.local_name())?;
        } else {
            write!(f, "[Q{{{}}}{}]", self.namespace_uri(), self.local_name())?;
        }
        match self.description() {
            Some(description) => write!(f, " {description}"),
            None => Ok(()),
        }
    }
}

/// The error QName and description of an [`XPathError::Raised`] error — what
/// `fn:error` or a host raised.
///
/// Produced by [`XPathError::raised_error`]. `namespace_uri` is empty when the
/// error QName is in no namespace. The owned form, with the error object and
/// the host payload, is [`RaisedErrorInfo`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct RaisedError<'a> {
    /// Namespace URI of the error QName ("" for no namespace).
    pub namespace_uri: &'a str,
    /// Local name of the error QName.
    pub local_name: &'a str,
    /// The `$description` argument, if one was supplied.
    pub description: Option<&'a str>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_codes() {
        assert_eq!(
            XPathError::context_undefined().error_code(),
            Some("XPDY0002")
        );
        assert_eq!(
            XPathError::more_than_one_item().error_code(),
            Some("XPTY0004")
        );
        assert_eq!(
            XPathError::treat_mismatch("xs:integer", "a sequence of 2 items").error_code(),
            Some("XPDY0050")
        );
        assert_eq!(
            XPathError::root_not_a_document().error_code(),
            Some("XPDY0050")
        );
        assert_eq!(
            XPathError::type_mismatch("xs:integer", "xs:string").error_code(),
            Some("XPTY0004")
        );
    }

    #[test]
    fn test_error_display() {
        let err = XPathError::type_mismatch("xs:integer", "xs:string");
        assert!(err.to_string().contains("XPTY0004"));
        assert!(err.to_string().contains("xs:integer"));
        assert!(err.to_string().contains("xs:string"));
    }

    #[test]
    fn test_internal_error() {
        let err = XPathError::internal("something went wrong");
        assert!(err.to_string().contains("something went wrong"));
        assert_eq!(err.error_code(), None);
        assert!(err.raised_error().is_none());
    }

    /// `XPathError` stays usable across threads and cheap to move.
    #[test]
    fn xpath_error_is_send_sync_and_small() {
        fn assert_send_sync<T: Send + Sync + 'static>() {}
        assert_send_sync::<XPathError>();
        // The QName-identified variant is one pointer; the largest variants
        // hold three `String`s.
        assert!(std::mem::size_of::<XPathError>() <= 80);
    }

    /// An error identified by an error QName is a variant of its own, and the
    /// Wave A entry points read it back.
    #[test]
    fn a_raised_error_is_a_variant_with_its_qname() {
        let err = XPathError::raised("urn:x", "boom", Some("why"));
        let XPathError::Raised(info) = &err else {
            panic!("expected XPathError::Raised, got {err:?}");
        };
        assert_eq!(info.namespace_uri(), "urn:x");
        assert_eq!(info.local_name(), "boom");
        assert_eq!(info.description(), Some("why"));
        assert!(info.error_object().is_empty());
        assert!(info.host_payload().is_none());
        let view = err.raised_error().expect("a raised error");
        assert_eq!(
            (view.namespace_uri, view.local_name, view.description),
            ("urn:x", "boom", Some("why"))
        );
        // A QName outside the error namespace has no `'static` code.
        assert_eq!(err.error_code(), None);
        assert_eq!(err.to_string(), "[Q{urn:x}boom] why");

        // In the error namespace, a listed code resolves; the code prints bare.
        let err = XPathError::raised(XQT_ERRORS_NAMESPACE, "FONS0004", None);
        assert_eq!(err.error_code(), Some("FONS0004"));
        assert_eq!(err.to_string(), "[FONS0004]");
        let err = XPathError::raised(XQT_ERRORS_NAMESPACE, "XTDE9999", Some("d"));
        assert_eq!(err.error_code(), None);
        assert_eq!(err.to_string(), "[XTDE9999] d");

        // No namespace.
        let err = XPathError::raised("", "bare", None);
        assert_eq!(err.raised_error().unwrap().namespace_uri, "");
        assert_eq!(err.to_string(), "[Q{}bare]");

        // The codes that travel as QNames are raised as `Raised`.
        assert!(matches!(
            XPathError::no_namespace_for_prefix("p"),
            XPathError::Raised(_)
        ));
        assert_eq!(
            XPathError::invalid_uri_argument("::").error_code(),
            Some("FORG0002")
        );
        // An internal error with the old text encoding is not a raised error.
        assert!(XPathError::internal("Q{urn:x}boom: why")
            .raised_error()
            .is_none());
    }

    /// A host error round-trips through the variant with its own payload.
    #[test]
    fn a_host_error_carries_its_payload() {
        #[derive(Debug, Clone, PartialEq)]
        struct HostError(&'static str, u32);
        impl std::fmt::Display for HostError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{} at {}", self.0, self.1)
            }
        }
        impl std::error::Error for HostError {}

        let err = XPathError::host(
            XQT_ERRORS_NAMESPACE,
            "HOST0001",
            Some("the host failed"),
            HostError("HOST0001", 42),
        );
        assert_eq!(
            err.downcast_host_payload::<HostError>(),
            Some(&HostError("HOST0001", 42))
        );
        assert_eq!(
            err.host_payload().map(|p| p.to_string()),
            Some("HOST0001 at 42".to_string())
        );
        // A different type does not downcast.
        assert!(err.downcast_host_payload::<std::fmt::Error>().is_none());
        // The engine sees an ordinary raised error.
        let view = err.raised_error().unwrap();
        assert_eq!(view.local_name, "HOST0001");
        assert_eq!(view.description, Some("the host failed"));
        assert_eq!(err.to_string(), "[HOST0001] the host failed");
        // Clones share the payload.
        let copy = err.clone();
        assert_eq!(
            copy.downcast_host_payload::<HostError>(),
            Some(&HostError("HOST0001", 42))
        );
        // No other error has a payload.
        assert!(XPathError::raised("urn:x", "e", None)
            .host_payload()
            .is_none());
        assert!(XPathError::FOAR0001.host_payload().is_none());
    }

    /// R12: a clone of a raised error shares its data, and `source()` is the
    /// host payload. R11: `error_code()` reports only the listed codes for a
    /// raised error; `raised_error()` has every QName.
    #[test]
    fn raised_errors_clone_cheaply_and_expose_the_payload_as_source() {
        #[derive(Debug)]
        struct HostError;
        impl std::fmt::Display for HostError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("the host's own error")
            }
        }
        impl std::error::Error for HostError {}

        let err = XPathError::host(XQT_ERRORS_NAMESPACE, "HOST0001", Some("d"), HostError);
        let copy = err.clone();
        let (XPathError::Raised(a), XPathError::Raised(b)) = (&err, &copy) else {
            panic!("raised errors");
        };
        assert!(a.shares_data_with(b), "a clone must share the data");
        let source = std::error::Error::source(&err).expect("the payload is the source");
        assert_eq!(source.to_string(), "the host's own error");
        assert!(source.downcast_ref::<HostError>().is_some());
        assert!(std::error::Error::source(&XPathError::raised("urn:x", "e", None)).is_none());

        // R11.
        let forg = XPathError::raised(XQT_ERRORS_NAMESPACE, "FORG0001", None);
        assert_eq!(forg.error_code(), None);
        assert_eq!(forg.raised_error().unwrap().local_name, "FORG0001");
    }

    /// XPST0080 and XPDY0050 are variants with their own text.
    #[test]
    fn xpst0080_and_xpdy0050_have_variants_and_messages() {
        let err = XPathError::cast_target_not_instantiable("xs:NOTATION");
        assert!(matches!(&err, XPathError::XPST0080 { type_name } if type_name == "xs:NOTATION"));
        assert_eq!(err.error_code(), Some("XPST0080"));
        assert!(err.raised_error().is_none());
        assert!(err.to_string().starts_with("[XPST0080] 'xs:NOTATION'"));

        let err = XPathError::treat_mismatch("xs:integer", "a sequence of 2 items");
        assert!(matches!(&err, XPathError::XPDY0050 { .. }));
        assert_eq!(
            err.to_string(),
            "[XPDY0050] treat as xs:integer: the operand is a sequence of 2 items"
        );
        assert!(XPathError::root_not_a_document()
            .to_string()
            .contains("document node"));
    }
}
