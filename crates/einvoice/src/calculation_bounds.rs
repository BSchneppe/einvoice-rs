//! Whole-draft preflight and retained-output accounting, separate from arithmetic.

use crate::{
    AccountIdentifier, BuildDiagnostic, CalculationError, CalculationResource, ContactPoint,
    InvoiceDraft, PartyDraft, PaymentMeans, PostalAddress,
};

/// Deterministic conservative limits for one entire calculation.
///
/// Defaults allow 1,000 lines, 32 accounts, 1,000 expected VAT groups, 1 MiB
/// total input text, 16 KiB per business string, 4,096 diagnostics and 16 MiB
/// retained output. Numeric defaults allow 1,024 input/output bytes per value,
/// 8,192 coefficient bits, absolute scale 1,024 and 1 billion cumulative work
/// units. These are fuel/size bounds, not allocator quotas or RSS guarantees.
/// Every field must be nonzero and platform-safe; explicit configurations may
/// raise the defaults. Conservative numeric/rendering estimates can refuse a
/// value whose eventual normalized representation would fit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalculationLimits {
    /// Maximum invoice lines, checked before walking the vector.
    pub max_lines: usize,
    /// Maximum transfer accounts, checked before walking the vector.
    pub max_accounts: usize,
    /// Maximum supplied expected VAT groups, checked before walking the vector.
    pub max_expected_groups: usize,
    /// Sum of byte lengths of every supplied business/numeric string.
    pub max_input_bytes: usize,
    /// Maximum byte length of any supplied string, before lexical scanning.
    pub max_string_bytes: usize,
    /// Maximum retained field diagnostics.
    pub max_diagnostics: usize,
    /// Aggregate retained strings and result/diagnostic records, including rule references.
    pub max_output_bytes: usize,
    /// Maximum lexical bytes per numeric input.
    pub numeric_input_bytes: usize,
    /// Maximum coefficient/intermediate bit estimate.
    pub coefficient_bits: u64,
    /// Maximum absolute lexical, normalized and intermediate decimal scale.
    pub absolute_scale: u64,
    /// Per-render byte/scratch bound; reserved before each numeric rendering.
    pub numeric_output_bytes: usize,
    /// Shared abstract numeric fuel for parsing, arithmetic, comparison and rendering.
    pub cumulative_work: u64,
}

impl Default for CalculationLimits {
    fn default() -> Self {
        Self {
            max_lines: 1_000,
            max_accounts: 32,
            max_expected_groups: 1_000,
            max_input_bytes: 1_048_576,
            max_string_bytes: 16_384,
            max_diagnostics: 4_096,
            max_output_bytes: 16_777_216,
            numeric_input_bytes: 1_024,
            coefficient_bits: 8_192,
            absolute_scale: 1_024,
            numeric_output_bytes: 1_024,
            cumulative_work: 1_000_000_000,
        }
    }
}

pub(crate) fn resource(path: &str, cause: CalculationResource) -> CalculationError {
    CalculationError::Resource {
        path: path.into(),
        cause,
    }
}

pub(crate) fn checked_add(a: usize, b: usize) -> Result<usize, CalculationError> {
    a.checked_add(b)
        .ok_or_else(|| resource("calculation", CalculationResource::SizeOverflow))
}

impl CalculationLimits {
    pub(crate) fn numeric(self) -> Result<einvoice_numeric::Limits, CalculationError> {
        let sizes = [
            self.max_lines,
            self.max_accounts,
            self.max_expected_groups,
            self.max_input_bytes,
            self.max_string_bytes,
            self.max_diagnostics,
            self.max_output_bytes,
        ];
        if sizes
            .iter()
            .any(|&n| n == 0 || n > isize::MAX as usize / 1024)
            || sizes
                .iter()
                .try_fold(0usize, |n, x| n.checked_add(*x))
                .is_none()
        {
            return Err(resource("limits", CalculationResource::InvalidLimits));
        }
        einvoice_numeric::Limits::new(
            self.numeric_input_bytes,
            self.coefficient_bits,
            self.absolute_scale,
            self.numeric_output_bytes,
            self.cumulative_work,
        )
        .map_err(|_| resource("limits", CalculationResource::InvalidLimits))
    }
}

/// Preflight uses lengths only: no content scans, draft clones or diagnostic paths.
/// All collection lengths are checked before any of the collections are walked.
pub(crate) fn preflight(
    draft: &InvoiceDraft,
    limits: CalculationLimits,
) -> Result<(), CalculationError> {
    for (count, max, path, cause) in [
        (
            draft.lines.len(),
            limits.max_lines,
            "lines",
            CalculationResource::Lines,
        ),
        (
            draft.payment.as_ref().map_or(0, |p| p.accounts.len()),
            limits.max_accounts,
            "payment.accounts",
            CalculationResource::Accounts,
        ),
        (
            draft.expected.vat_groups.len(),
            limits.max_expected_groups,
            "expected.vat_groups",
            CalculationResource::ExpectedGroups,
        ),
    ] {
        if count > max {
            return Err(resource(path, cause));
        }
    }
    let mut input = Input { limits, bytes: 0 };
    for value in [
        draft.invoice_number(),
        draft.currency(),
        &draft.invoice_type,
        &draft.issue_date,
        &draft.business_process,
        &draft.references.buyer_reference,
    ] {
        input.text(value)?;
    }
    for value in [
        &draft.references.purchase_order,
        &draft.references.seller_order,
        &draft.references.contract,
        &draft.payment_terms.text,
        &draft.payment_terms.due_date,
    ] {
        input.optional(value)?;
    }
    if let Some(seller) = &draft.seller {
        input.party(&seller.party)?;
        input.optional(&seller.tax_registration)?;
    }
    if let Some(buyer) = &draft.buyer {
        input.party(buyer)?;
    }
    if let Some(representative) = &draft.tax_representative {
        input.text(&representative.name)?;
        input.text(&representative.vat_identifier)?;
        if let Some(address) = &representative.address {
            input.address(address)?;
        }
    }
    if let Some(payment) = &draft.payment {
        if let PaymentMeans::Unsupported(code) = &payment.means {
            input.text(code)?;
        }
        input.optional(&payment.description)?;
        input.optional(&payment.reference)?;
        for account in &payment.accounts {
            let (AccountIdentifier::Iban(value) | AccountIdentifier::Proprietary(value)) =
                &account.identifier;
            input.text(value)?;
            input.optional(&account.name)?;
            input.optional(&account.bic)?;
        }
    }
    for line in &draft.lines {
        for value in [
            &line.id,
            &line.name,
            &line.quantity.value,
            &line.quantity.unit,
            &line.price.amount,
            &line.vat.category,
            &line.vat.rate,
        ] {
            input.text(value)?;
        }
        if let Some(basis) = &line.price.basis {
            input.text(&basis.value)?;
            input.optional(&basis.unit)?;
        }
        input.optional(&line.expected_net)?;
    }
    for group in &draft.expected.vat_groups {
        input.text(&group.rate)?;
        input.optional(&group.basis)?;
        input.optional(&group.tax)?;
    }
    for value in [
        &draft.expected.net,
        &draft.expected.tax,
        &draft.expected.gross,
        &draft.expected.due,
    ] {
        input.optional(value)?;
    }
    Ok(())
}

struct Input {
    limits: CalculationLimits,
    bytes: usize,
}
impl Input {
    fn text(&mut self, text: &str) -> Result<(), CalculationError> {
        if text.len() > self.limits.max_string_bytes {
            return Err(resource("input", CalculationResource::StringBytes));
        }
        self.bytes = checked_add(self.bytes, text.len())?;
        if self.bytes > self.limits.max_input_bytes {
            return Err(resource("input", CalculationResource::InputBytes));
        }
        Ok(())
    }
    fn optional(&mut self, text: &Option<String>) -> Result<(), CalculationError> {
        if let Some(text) = text {
            self.text(text)?;
        }
        Ok(())
    }
    fn address(&mut self, address: &PostalAddress) -> Result<(), CalculationError> {
        self.text(&address.country)?;
        for text in [
            &address.city,
            &address.postcode,
            &address.line_one,
            &address.line_two,
            &address.line_three,
            &address.country_subdivision,
        ] {
            self.optional(text)?;
        }
        Ok(())
    }
    fn party(&mut self, party: &PartyDraft) -> Result<(), CalculationError> {
        self.text(&party.name)?;
        if let Some(address) = &party.address {
            self.address(address)?;
        }
        if let Some(endpoint) = &party.endpoint {
            self.text(&endpoint.value)?;
            self.text(&endpoint.scheme)?;
        }
        if let Some(contact) = &party.contact {
            if let Some(ContactPoint::Person(value) | ContactPoint::Department(value)) =
                &contact.point
            {
                self.text(value)?;
            }
            self.optional(&contact.telephone)?;
            self.optional(&contact.email)?;
        }
        for identifier in [&party.identifier, &party.legal_registration]
            .into_iter()
            .flatten()
        {
            self.text(&identifier.value)?;
            self.optional(&identifier.scheme)?;
        }
        self.optional(&party.vat_identifier)
    }
}

pub(crate) struct Output {
    remaining: usize,
}
impl Output {
    pub(crate) fn new(bytes: usize) -> Self {
        Self { remaining: bytes }
    }
    pub(crate) fn take(&mut self, bytes: usize, path: &str) -> Result<(), CalculationError> {
        self.remaining = self
            .remaining
            .checked_sub(bytes)
            .ok_or_else(|| resource(path, CalculationResource::OutputBytes))?;
        Ok(())
    }
    pub(crate) fn refund(&mut self, bytes: usize) -> Result<(), CalculationError> {
        self.remaining = checked_add(self.remaining, bytes)?;
        Ok(())
    }
}

/// Both bounded calculation and the compatible unbounded public helper use this sink.
/// Check byte/count bounds before allocating messages, paths, rule vectors or records.
pub(crate) struct Diagnostics {
    values: Vec<BuildDiagnostic>,
    max_count: usize,
    output: Output,
    error: Option<CalculationError>,
}
impl Diagnostics {
    pub(crate) fn new(limits: CalculationLimits) -> Self {
        Self {
            values: Vec::new(),
            max_count: limits.max_diagnostics,
            output: Output::new(limits.max_output_bytes),
            error: None,
        }
    }
    pub(crate) fn unbounded() -> Self {
        Self {
            values: Vec::new(),
            max_count: usize::MAX,
            output: Output::new(usize::MAX),
            error: None,
        }
    }
    pub(crate) fn push(
        &mut self,
        code: &'static str,
        path: &str,
        message: &str,
        rules: &[&'static str],
    ) {
        self.formatted(code, path, message.len(), rules, || message.into());
    }
    pub(crate) fn formatted(
        &mut self,
        code: &'static str,
        path: &str,
        bytes: usize,
        rules: &[&'static str],
        message: impl FnOnce() -> String,
    ) {
        if self.error.is_some() {
            return;
        }
        let reserve = || {
            if self.values.len() >= self.max_count {
                return Err(resource(path, CalculationResource::Diagnostics));
            }
            let rules_bytes = rules
                .len()
                .checked_mul(std::mem::size_of::<&str>())
                .ok_or_else(|| resource(path, CalculationResource::SizeOverflow))?;
            let bytes = checked_add(
                checked_add(bytes, path.len())?,
                checked_add(std::mem::size_of::<BuildDiagnostic>(), rules_bytes)?,
            )?;
            Ok(bytes)
        };
        match reserve().and_then(|bytes| self.output.take(bytes, path)) {
            Ok(()) => self.values.push(BuildDiagnostic {
                code,
                path: path.into(),
                message: message(),
                rule_ids: rules.to_vec(),
            }),
            Err(error) => self.error = Some(error),
        }
    }
    pub(crate) fn check(&mut self) -> Result<(), CalculationError> {
        match self.error.take() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
    pub(crate) fn into_values(self) -> Vec<BuildDiagnostic> {
        self.values
    }
    pub(crate) fn output(&mut self) -> &mut Output {
        &mut self.output
    }
}
