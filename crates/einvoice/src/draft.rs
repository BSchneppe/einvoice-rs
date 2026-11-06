/// Starting point for a new invoice, separate from immutable imports.
///
/// This deliberately incomplete draft records identity and currency only.
/// Parties, lines, exact decimal calculations, expected totals and encoding
/// belong to BSC-9. Constructing a draft does not check business validity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvoiceDraft {
    invoice_number: String,
    currency: String,
}

impl InvoiceDraft {
    /// Begin a new invoice; no XML is generated and no amounts are inferred.
    pub fn new(invoice_number: impl Into<String>, currency: impl Into<String>) -> Self {
        Self {
            invoice_number: invoice_number.into(),
            currency: currency.into(),
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
