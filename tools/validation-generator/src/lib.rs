//! Offline maintainer frontend. Inspection is deliberately non-executable.
pub mod bundle;
pub mod emit;
pub mod schema;
pub mod schematron;
pub mod source;
pub mod xpath;
use std::fmt;
pub use validation_ir as ir;
#[derive(Debug)]
pub struct Error(pub String);
impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
    pub fn xpath(offset: usize, message: &str) -> Self {
        Self(format!("decoded XPath UTF-8 offset {offset}: {message}"))
    }
    pub fn at(self, loc: &ir::Location) -> Self {
        Self(format!(
            "{}:{}:{} attribute {:?}: {}",
            loc.resource, loc.line, loc.column, loc.attribute, self.0
        ))
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self(e.to_string())
    }
}
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self(e.to_string())
    }
}
impl From<roxmltree::Error> for Error {
    fn from(e: roxmltree::Error) -> Self {
        Self(e.to_string())
    }
}
impl From<zip::result::ZipError> for Error {
    fn from(e: zip::result::ZipError) -> Self {
        Self(e.to_string())
    }
}
pub type Result<T> = std::result::Result<T, Error>;
