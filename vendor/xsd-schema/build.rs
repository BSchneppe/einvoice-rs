//! Build script for xsd-schema crate.
//!
//! This script configures LALRPOP to generate the XPath parser
//! when the `xsd11` feature is enabled.

fn main() {
    #[cfg(feature = "xsd11")]
    lalrpop::process_root().expect("Failed to process LALRPOP grammar");
}
