#![warn(missing_docs)]
//! A small facade for immutable invoice imports and explicit validation targets.
//!
//! **No native conformance backend or invoice encoder is available yet.** XML
//! parsing and target preflight are implemented; neither proves conformance.
//! See the crate README for the boundary with the experimental `einvoice-core`.
//!
//! ```
//! use einvoice::{Document, InvoiceDraft, Target, ValidationOutcome};
//!
//! let source = b"<?xml version='1.0'?><received unexpected='kept'/>\r\n";
//! let document = Document::parse(source)?;
//! assert_eq!(document.original_bytes(), source);
//! let target = Target::xrechnung_3_0_2();
//! let report = document.validate(&target);
//! assert_eq!(report.outcome(), ValidationOutcome::InvalidDocument);
//! assert!(!report.is_valid());
//!
//! // The draft is a separate creation path; it cannot edit an import.
//! let draft = InvoiceDraft::new("INV-2026-1", "EUR");
//! assert!(target.encode(&draft).is_err()); // encoder is unavailable
//! # Ok::<(), einvoice::ParseError>(())
//! ```

mod calculation;
mod calculation_bounds;
mod document;
mod domain;
mod draft;
mod encoding;
mod fields;
mod report;
mod syntax;
mod target;

pub use calculation::{
    CalculatedInvoice, CalculatedLine, CalculatedTotals, CalculatedVatGroup, CalculationError,
    CalculationFailure, CalculationResource,
};
pub use calculation_bounds::CalculationLimits;
pub use document::{Document, Limits, ParseError, ResourceLimit};
pub use domain::{
    AccountIdentifier, Contact, ContactPoint, ElectronicAddress, Identifier, InvoiceExpectations,
    InvoiceLineDraft, InvoiceReferences, LineVat, NetPrice, PartyDraft, PaymentInstructions,
    PaymentMeans, PaymentTerms, PostalAddress, PriceBasis, Quantity, SellerDraft,
    TaxRepresentativeDraft, TransferAccount, VatGroupExpectation,
};
pub use draft::{BuildError, InvoiceDraft};
pub use fields::BuildDiagnostic;
pub use report::{
    Diagnostic, EngineFailure, Severity, SkipReason, SourceLocation, Stage, StageResult,
    StageStatus, ValidationFailure, ValidationOutcome, ValidationReport,
};
pub use target::{ArtifactIdentity, BundleIdentity, ImplementationIdentity, Target};
