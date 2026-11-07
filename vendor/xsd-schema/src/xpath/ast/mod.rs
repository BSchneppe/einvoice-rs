//! XPath 2.0 Abstract Syntax Tree (AST) definitions.
//
//! This module defines the AST node types for the XPath 2.0 parser.
//! All types are stubbed (fields defined but minimal behavior) to enable
//! parser development. Full evaluation semantics will be added later.

pub use crate::xpath::arena::{AstNodeId, SourceSpan};

mod control_flow;
mod core;
mod expressions;
mod functions;
mod operators;
mod paths;
mod types;

#[cfg(test)]
mod tests;

pub use control_flow::*;
pub use core::AstNode;
pub use expressions::*;
pub use functions::*;
pub use operators::*;
pub use paths::*;
pub use types::*;
