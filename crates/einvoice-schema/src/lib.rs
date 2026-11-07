#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]

mod budget;
mod bundle;
mod capture;
mod driver;
mod evidence;
mod model;
mod pinned;

pub use budget::{Failure, RequestControl, RequestLimits};
pub use bundle::{sha256, CiiBundle, InitializationError, ResourceInfo};
pub use einvoice_source::SourceDocument;
pub use evidence::{FacetCause, NativeCause, TypeCode, ValueCause};
pub use model::*;
pub use pinned::{CII_RESOURCES, CONFIG_ARCHIVE_SHA256};
use std::num::NonZeroUsize;
use xsd_schema::validation::{SchemaValidator, ValidationFlags};

/// An initialized worker-private lexical epoch. Its backend and borrowed schema
/// cannot escape `with_epoch`; no shared lock or self-reference is required.
pub struct Epoch<'a> {
    schema: &'a xsd_schema::SchemaSet,
    validator: &'a SchemaValidator<'a>,
    remaining: usize,
    retired: bool,
}
impl Epoch<'_> {
    /// A finite allowance supplied by the owner. Incomplete processing retires
    /// immediately; healthy valid and invalid requests may reuse the epoch.
    pub fn validate(
        &mut self,
        source: &SourceDocument,
        limits: &RequestLimits,
        control: RequestControl<'_>,
    ) -> Result<Observation, Failure> {
        if self.retired || self.remaining == 0 {
            return Err(Failure::EpochRetired);
        }
        self.remaining -= 1;
        // Mark terminal before calling backend code: even a caught unwind must
        // not accidentally permit reuse of partially mutated backend state.
        self.retired = true;
        let observation = driver::run(source, limits, control, self.schema, self.validator);
        self.retired = observation.assessment == Assessment::Incomplete || self.remaining == 0;
        Ok(observation)
    }
    pub fn is_retired(&self) -> bool {
        self.retired
    }
    pub fn remaining_requests(&self) -> usize {
        self.remaining
    }
}
/// Compiles only the exact pinned structural closure. This proves neither BSC7
/// package admission nor business-rule/acceptance capability. Construct and call
/// within the owning worker thread; returned values must be owned.
pub fn with_epoch<T>(
    bundle: &CiiBundle,
    requests: NonZeroUsize,
    f: impl FnOnce(&mut Epoch<'_>) -> T,
) -> Result<T, InitializationError> {
    let schema = bundle.compile()?;
    prepared(&schema, requests, f)
}
fn prepared<T>(
    schema: &xsd_schema::SchemaSet,
    requests: NonZeroUsize,
    f: impl FnOnce(&mut Epoch<'_>) -> T,
) -> Result<T, InitializationError> {
    let validator = SchemaValidator::new(schema, ValidationFlags::default());
    let failures = validator.content_model_failures();
    if !failures.is_empty() {
        return Err(InitializationError::new(
            "content model preparation",
            format!("{failures:?}"),
        ));
    }
    let mut epoch = Epoch {
        schema,
        validator: &validator,
        remaining: requests.get(),
        retired: false,
    };
    Ok(f(&mut epoch))
}
#[cfg(test)]
mod tests;
