use crate::{
    BuildDiagnostic, InvoiceExpectations, InvoiceLineDraft, InvoiceReferences, PartyDraft,
    PaymentInstructions, PaymentTerms, SellerDraft, TaxRepresentativeDraft,
};

/// Starting point for a new type-380 standard-VAT invoice, separate from imports.
///
/// Public business fields allow incomplete input without a forwarding builder API.
/// Numeric values remain original text for later shared-kernel preparation.
/// Construction and even an empty field-diagnostic list never prove conformance.
/// This slice has no fields for allowances, charges, prepayments, payable
/// adjustments or structured cash discounts; it does not implement those features.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvoiceDraft {
    invoice_number: String,
    currency: String,
    /// Caller-requested type code; defaults to the bounded creation type `380`.
    /// Other types are explicit unsupported requests, not incoming validation errors.
    pub invoice_type: String,
    /// Required calendar date in `YYYYMMDD` form; no clock default.
    pub issue_date: String,
    /// Required business process, supplied by the caller without an invented default.
    pub business_process: String,
    /// Required seller details, with independent identity and tax alternatives.
    pub seller: Option<SellerDraft>,
    /// Required buyer details; its contact remains optional.
    pub buyer: Option<PartyDraft>,
    /// Optional seller tax representative, with conditionally required contents.
    pub tax_representative: Option<TaxRepresentativeDraft>,
    /// Buyer reference and optional ordinary order/contract references.
    pub references: InvoiceReferences,
    /// Required transfer instructions and at least one receiving account.
    pub payment: Option<PaymentInstructions>,
    /// Optional due date and prose, including when the eventual payable is positive.
    pub payment_terms: PaymentTerms,
    /// Required nonempty lines, retained in caller order.
    pub lines: Vec<InvoiceLineDraft>,
    /// Optional original expectations; no comparison or replacement occurs here.
    pub expected: InvoiceExpectations,
}

impl InvoiceDraft {
    /// Begin a new invoice; no XML is generated and no amounts are inferred.
    pub fn new(invoice_number: impl Into<String>, currency: impl Into<String>) -> Self {
        Self {
            invoice_number: invoice_number.into(),
            currency: currency.into(),
            invoice_type: "380".into(),
            issue_date: String::new(),
            business_process: String::new(),
            seller: None,
            buyer: None,
            tax_representative: None,
            references: InvoiceReferences::default(),
            payment: None,
            payment_terms: PaymentTerms::default(),
            lines: Vec::new(),
            expected: InvoiceExpectations::default(),
        }
    }
    /// Caller-supplied invoice number.
    pub fn invoice_number(&self) -> &str {
        &self.invoice_number
    }
    /// Caller-supplied invoice currency, not yet validated.
    pub fn currency(&self) -> &str {
        &self.currency
    }

    /// Collect required/conditional field and API text/date diagnostics.
    ///
    /// Paths use public field names and zero-based vector indices. Checking is
    /// explicitly incomplete: no numeric lexical/sign/comparison checks, calculations,
    /// expected-amount equality, code lists, IBAN checks, serialization or native
    /// conformance validation run. An empty list never means a valid invoice;
    /// [`crate::Target::encode`] remains unavailable even for a populated draft.
    /// This helper scans supplied input and collects findings without resource limits.
    /// Caller strings are never stripped, normalized or replaced.
    pub fn field_diagnostics(&self) -> Vec<BuildDiagnostic> {
        crate::fields::diagnose(self)
    }
}

/// Creation cannot succeed until both encoding and native validation exist.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BuildError {
    /// The exact requested target is not installed; no weaker encoder was used.
    #[error("unsupported target: {target}")]
    UnsupportedTarget {
        /// Requested target identifier.
        target: String,
    },
    /// No encoder exists yet for this target. No partial XML is returned.
    #[error("validated invoice creation is unavailable for {target}")]
    Unavailable {
        /// Requested target identifier.
        target: String,
    },
}
