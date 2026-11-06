//! XPath2 evaluation engine
//!
//! This module provides the full XPath 2.0 parser, binder, and evaluator.
//!
//! An expression is compiled once against a static context ([`XPathContext`]:
//! names, namespaces, the schema set, the function catalog, the language
//! [`XPathMode`]) and evaluated by an [`XPathEvaluator`], which builds the
//! dynamic context ([`DynamicContext`]) of each run. The dynamic context holds
//! the focus, the variables, the current dateTime and the **implicit
//! timezone** — [`XPathContext::with_implicit_timezone`] or
//! [`DynamicContext::with_implicit_timezone`]; without one, the host's local
//! offset, read once per evaluation
//! ([`DynamicContext::implicit_timezone_or_host`]). Date/time and duration
//! comparison and arithmetic are exact and raise the F&O errors at the limits
//! of the value types.
//!
//! A host can lend a run a [`RegexCache`] it keeps across runs
//! ([`XPathEvaluator::with_regex_cache`]) and collect the run's
//! [`XPathWarning`]s ([`XPathEvaluator::with_warnings`]). Every expression the
//! parser reads nests at most [`MAX_EXPRESSION_DEPTH`] levels
//! ([`XPathError::LimitExceeded`] beyond). [`XPathError`] is
//! `#[non_exhaustive]`; errors identified by an error QName are
//! [`XPathError::Raised`].
//!
//! ## Core Types (re-exported from `navigator`)
//!
//! - [`DomNavigator`] - Trait for cursor-based XML navigation
//! - [`DomNodeType`] - Node type enumeration (Element, Text, Attribute, etc.)
//! - [`XmlNodeOrder`] - Document order comparison result
//! - [`NamespaceAxisScope`] - Scope filter for namespace axis traversal
//!
//! ## Parser
//!
//! - [`ast`] - AST node types for XPath expressions
//! - [`arena`] - Arena-based storage for AST nodes
//! - [`lexer`] - Stateful tokenizer with lookahead
//! - [`parser`] - LALRPOP-based parser

// Parser modules
pub mod arena;
pub mod ast;
pub mod axis_iterators;
// String comparison under a collation, and the host callback that supplies
// every collation other than the Unicode codepoint one.
pub mod collation;
pub mod context;
pub mod error;
pub mod item_set;
pub mod iterator;
// Indexed evaluation of the general comparison operators. Internal: it only
// ever changes how fast `operators` answers a comparison, never the answer.
pub(crate) mod general_compare;
// Reuse of a general-comparison index across evaluations of one comparison
// node, for the duration of one evaluation run. Internal, and likewise
// answer-preserving.
pub(crate) mod compare_cache;
pub mod depth;
pub mod lexer;
pub mod node_test;
pub mod operators;
pub mod parser;
// Reuse of the compiled programs of `fn:matches` / `fn:replace` / `fn:tokenize`
// across the calls of one evaluation run. Internal, and likewise
// answer-preserving.
pub(crate) mod regex_cache;
pub mod timsort;
pub mod tree_comparer;

// Core function modules (ported from CoreFuncs.cs)
pub mod atomize;
pub mod boolean;
pub mod cast;
pub mod iter_adapters;
pub mod node_ops;
pub mod quantified;
// Casts and constructor functions for the atomic types of a schema set.
pub(crate) mod schema_cast;
pub mod sequence_ops;
pub mod string_ops;
pub mod type_info;

// XPath 2.0 function registry and dispatch
pub mod functions;

// AST binding and evaluation phases
pub mod bind;
pub mod eval;

// Compile-time dependency analysis (focus use, referenced variables, calls).
// Internal: the public surface is `FunctionCallRef` plus the accessors on
// `XPathExpr` (re-exported below).
pub(crate) mod deps;

// High-level public API
pub mod api;

// `xs:decimal` values beyond `rust_decimal`'s capacity, end to end.
#[cfg(test)]
mod decimal_capacity_tests;

#[cfg(test)]
mod schema_cast_tests;

// Re-export navigator types for backward compatibility
pub use crate::navigator::{
    DomNavigator, DomNodeType, NamespaceAxisScope, NavigatorError, RoXmlNavigator, TypedValue,
    XmlNodeOrder,
};

// High-level API re-exports
pub use self::api::{EvalValue, ExternalVar, TypedEvaluator, XPathEvaluator, XPathExpr};

// Re-export key parser types
pub use self::arena::{AstArena, AstNodeId, SourceSpan};
pub use self::ast::AstNode;
pub use self::axis_iterators::{
    AncestorAxis, AttributeAxis, AxisTraversal, ChildAxis, ChildOverDescendantsNodeIterator,
    DescendantNodeIterator, FollowingNodeIterator, FollowingSiblingAxis, NamespaceAxis, ParentAxis,
    PrecedingNodeIterator, PrecedingSiblingAxis, SelfAxis, SequentialAxisNodeIterator,
    SpecialChildNodeIterator, SpecialDescendantNodeIterator,
};
pub use self::bind::bind_node;
pub use self::collation::{Collation, CollationResolver, CODEPOINT_COLLATION_URI};
pub use self::context::{DynamicContext, NameBinder, VarRef, VarSlotId, VarStore, XPathContext};
pub use self::deps::FunctionCallRef;
pub use self::depth::{expression_depth, MAX_EXPRESSION_DEPTH};
pub use self::error::{XPathError, XPathWarning};
pub use self::eval::eval_node;
pub use self::functions::{
    FunctionArity, FunctionId, FunctionSignature, XPathValue, FUNCTION_REGISTRY,
};
pub use self::item_set::{
    ItemSet, ItemSetIter, ItemSetIterMut, XPathComparer, XPathEqualityComparer,
};
pub use self::iterator::{
    BufferedNodeIterator, DocumentOrderNodeIterator, EmptyIterator, ItemIterator,
    PositionFilterNodeIterator, RangeIterator, VecNodeIterator, XmlItem, XmlItemRef,
    XmlNodeIterator,
};
pub use self::lexer::{Lexer, Token};
pub use self::node_test::{matches_name_test, matches_sequence_type, NodeTest};
pub use self::parser::{
    parse, parse_with_options, parse_xpath10, parse_xpath20, ParseError, ParsedXPath,
};
pub use self::regex_cache::RegexCache;
pub use self::timsort::{
    timsort, timsort_by, timsort_slice, timsort_slice_by, timsort_slice_with_comparer,
    timsort_with_comparer, FnComparer, IComparer, OrdComparer, ReverseComparer,
};
pub use self::tree_comparer::TreeComparer;

/// Selects XPath language version for parsing and evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum XPathMode {
    XPath10,
    #[default]
    XPath20,
}

/// Options for XPath parsing.
#[derive(Debug, Clone)]
pub struct XPathParseOptions {
    pub mode: XPathMode,
}

impl Default for XPathParseOptions {
    fn default() -> Self {
        Self {
            mode: XPathMode::XPath20,
        }
    }
}
