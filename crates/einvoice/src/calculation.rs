//! Exact pinned builder policy; no serialization or conformance decision.

use std::cmp::Ordering;
use std::mem::size_of;

use einvoice_numeric::{Budget, Decimal, Error, ResourceCause, RoundingMode};

use crate::calculation_bounds::{checked_add, preflight, resource, Diagnostics};
use crate::{BuildDiagnostic, CalculationLimits, InvoiceDraft, Target};

/// A resource refusal during preparation, distinct from invoice field findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalculationResource {
    /// Zero or platform-unsafe configuration.
    InvalidLimits,
    /// Too many invoice lines.
    Lines,
    /// Too many receiving accounts.
    Accounts,
    /// Too many expected VAT groups.
    ExpectedGroups,
    /// Aggregate supplied text exceeds its allowance.
    InputBytes,
    /// One supplied business string exceeds its allowance.
    StringBytes,
    /// Too many retained diagnostics.
    Diagnostics,
    /// Aggregate retained output exceeds its allowance.
    OutputBytes,
    /// One numeric lexical input exceeds its allowance.
    NumericInputBytes,
    /// Numeric coefficient or temporary estimate exceeds its bit allowance.
    CoefficientBits,
    /// Numeric scale exceeds its allowance.
    Scale,
    /// A numeric rendering or scratch estimate exceeds its per-value allowance.
    NumericOutputBytes,
    /// Invoice-wide numeric fuel is exhausted.
    Work,
    /// Checked size arithmetic cannot represent an estimate.
    SizeOverflow,
}

/// Unexpected numeric processing failures, never invoice field faults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalculationFailure {
    /// A divisor unexpectedly became zero after field checking.
    DivideByZero,
    /// An explicitly rounded amount unexpectedly required rendering-rounding.
    InexactRendering,
    /// An internal numeric constant was unexpectedly malformed.
    InvalidLexical,
}

/// Calculation cannot establish native conformance, even when successful.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CalculationError {
    /// The target is not supported. Its unbounded identifier is not copied.
    #[error("unsupported calculation target")]
    UnsupportedTarget,
    /// Bounded public-field findings, including exact expectation disagreements.
    #[error("invoice draft has calculation or field findings")]
    Fields {
        /// Original paths, supplied/computed values and supporting source references.
        diagnostics: Vec<BuildDiagnostic>,
    },
    /// Processing stopped because a resource policy refused the operation.
    #[error("calculation resource limit at {path}: {cause:?}")]
    Resource {
        /// Public field path, or `input`, `limits` or `calculation` for whole-scope checks.
        path: String,
        /// Typed limit cause; not a conformance result.
        cause: CalculationResource,
    },
    /// Unexpected checked numeric failure, separate from supplied lexical faults.
    #[error("calculation processing failure at {path}: {cause:?}")]
    Processing {
        /// Field or calculation phase where processing stopped.
        path: String,
        /// Typed numeric processing failure.
        cause: CalculationFailure,
    },
}

/// Immutable arithmetic preparation in input line order and numeric VAT-rate order.
/// No constructor, mutation, source-document conversion or validation claim is provided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalculatedInvoice {
    currency: String,
    lines: Vec<CalculatedLine>,
    vat_groups: Vec<CalculatedVatGroup>,
    totals: CalculatedTotals,
}
impl CalculatedInvoice {
    /// Original currency, without currency conversion or code-list validation.
    pub fn currency(&self) -> &str {
        &self.currency
    }
    /// Canonical numeric line inputs and two-place amounts in input order.
    pub fn lines(&self) -> &[CalculatedLine] {
        &self.lines
    }
    /// One group per numeric rate, sorted in ascending numeric order.
    pub fn vat_groups(&self) -> &[CalculatedVatGroup] {
        &self.vat_groups
    }
    /// Exact derived two-place totals.
    pub fn totals(&self) -> &CalculatedTotals {
        &self.totals
    }
}

/// Canonical numeric inputs retained for future serialization without recalculation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalculatedLine {
    quantity: String,
    price: String,
    price_basis: Option<String>,
    vat_rate: String,
    net: String,
}
impl CalculatedLine {
    /// Exact canonical quantity, retaining signed and zero values.
    pub fn quantity(&self) -> &str {
        &self.quantity
    }
    /// Exact canonical unit net price, without intermediate rounding.
    pub fn price(&self) -> &str {
        &self.price
    }
    /// Exact canonical explicit basis; None preserves omission (calculated as one).
    pub fn price_basis(&self) -> Option<&str> {
        self.price_basis.as_deref()
    }
    /// Exact canonical standard VAT rate.
    pub fn vat_rate(&self) -> &str {
        &self.vat_rate
    }
    /// Rounded line net, always with two decimal places.
    pub fn net(&self) -> &str {
        &self.net
    }
}

/// Standard VAT group whose basis sums rounded line nets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalculatedVatGroup {
    rate: String,
    basis: String,
    tax: String,
}
impl CalculatedVatGroup {
    /// Exact canonical numeric rate.
    pub fn rate(&self) -> &str {
        &self.rate
    }
    /// Two-place sum of rounded net amounts in this group.
    pub fn basis(&self) -> &str {
        &self.basis
    }
    /// Two-place tax, rounded once for the whole group.
    pub fn tax(&self) -> &str {
        &self.tax
    }
}

/// Two-place amounts in the invoice currency; no balancing adjustment is added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalculatedTotals {
    net: String,
    tax: String,
    gross: String,
    due: String,
}
impl CalculatedTotals {
    /// Sum of rounded line nets.
    pub fn net(&self) -> &str {
        &self.net
    }
    /// Sum of rounded VAT group taxes.
    pub fn tax(&self) -> &str {
        &self.tax
    }
    /// Net plus tax.
    pub fn gross(&self) -> &str {
        &self.gross
    }
    /// Gross, with no prepayments or payable adjustments in this slice.
    pub fn due(&self) -> &str {
        &self.due
    }
}

type Result<T> = std::result::Result<T, CalculationError>;

fn numeric_error(path: &str, error: Error) -> CalculationError {
    let cause = match error {
        Error::Resource(cause) => match cause {
            ResourceCause::InvalidLimits => CalculationResource::InvalidLimits,
            ResourceCause::InputBytes => CalculationResource::NumericInputBytes,
            ResourceCause::CoefficientBits => CalculationResource::CoefficientBits,
            ResourceCause::Scale => CalculationResource::Scale,
            ResourceCause::OutputBytes => CalculationResource::NumericOutputBytes,
            ResourceCause::Work => CalculationResource::Work,
            ResourceCause::SizeOverflow => CalculationResource::SizeOverflow,
        },
        error => {
            return CalculationError::Processing {
                path: path.into(),
                cause: match error {
                    Error::InvalidLexical => CalculationFailure::InvalidLexical,
                    Error::DivideByZero => CalculationFailure::DivideByZero,
                    Error::InexactRendering => CalculationFailure::InexactRendering,
                    Error::Resource(_) => unreachable!(),
                },
            }
        }
    };
    resource(path, cause)
}

struct Context {
    budget: Budget,
    diagnostics: Diagnostics,
    render_bound: usize,
}
impl Context {
    fn parse(&mut self, text: &str, path: &str) -> Result<Option<Decimal>> {
        match Decimal::parse_decimal(text, &mut self.budget) {
            Ok(number) => Ok(Some(number)),
            Err(Error::InvalidLexical) => {
                self.diagnostic(
                    "API-BUILD-DECIMAL",
                    path,
                    "Supplied value is not an exact decimal",
                    &[],
                )?;
                Ok(None)
            }
            Err(error) => Err(numeric_error(path, error)),
        }
    }
    fn optional(&mut self, text: &Option<String>, path: &str) -> Result<Option<Decimal>> {
        match text {
            Some(text) => self.parse(text, path),
            None => Ok(None),
        }
    }
    fn diagnostic(
        &mut self,
        code: &'static str,
        path: &str,
        message: &str,
        rules: &[&'static str],
    ) -> Result<()> {
        self.diagnostics.push(code, path, message, rules);
        self.diagnostics.check()
    }
    fn sign(
        &mut self,
        number: &Option<Decimal>,
        positive: bool,
        path: &str,
        rules: &[&'static str],
    ) -> Result<()> {
        if number.as_ref().is_some_and(|n| {
            if positive {
                n.sign() <= 0
            } else {
                n.sign() < 0
            }
        }) {
            self.diagnostic(
                "API-BUILD-NUMERIC-SIGN",
                path,
                if positive {
                    "Supplied decimal must be greater than zero"
                } else {
                    "Supplied decimal must be nonnegative"
                },
                rules,
            )?;
        }
        Ok(())
    }
    fn render(&mut self, number: &Decimal, money: bool, path: &str) -> Result<String> {
        // Reserve the kernel's full per-render allowance BEFORE it allocates.
        // Refund unused bytes; tight aggregate limits may conservatively refuse.
        self.diagnostics.output().take(self.render_bound, path)?;
        let rendered = if money {
            number.to_fixed(2, &mut self.budget)
        } else {
            number.to_canonical(&mut self.budget)
        }
        .map_err(|error| numeric_error(path, error))?;
        let unused = self
            .render_bound
            .checked_sub(rendered.len())
            .ok_or_else(|| resource(path, CalculationResource::SizeOverflow))?;
        self.diagnostics.output().refund(unused)?;
        Ok(rendered)
    }
    fn expectation(
        &mut self,
        supplied: Option<&str>,
        parsed: Option<&Decimal>,
        computed: &Decimal,
        path: &str,
        currency: &str,
    ) -> Result<()> {
        if let (Some(supplied), Some(parsed)) = (supplied, parsed) {
            if parsed
                .compare(computed, &mut self.budget)
                .map_err(|e| numeric_error(path, e))?
                != Ordering::Equal
            {
                let computed = self.render(computed, true, path)?;
                let bytes = [
                    "Supplied ",
                    supplied,
                    "; computed ",
                    &computed,
                    " ",
                    currency,
                ]
                .iter()
                .try_fold(0, |n, s| checked_add(n, s.len()))?;
                self.diagnostics
                    .formatted("API-BUILD-EXPECTED-AMOUNT", path, bytes, &[], || {
                        format!("Supplied {supplied}; computed {computed} {currency}")
                    });
                self.diagnostics.output().refund(computed.len())?;
                self.diagnostics.check()?;
            }
        }
        Ok(())
    }
    fn fields(&mut self) -> Result<()> {
        self.diagnostics.check()?;
        if self.diagnostics.is_empty() {
            Ok(())
        } else {
            // Only used at phase boundaries; move the bounded findings out on error.
            let diagnostics = std::mem::replace(&mut self.diagnostics, Diagnostics::unbounded());
            Err(CalculationError::Fields {
                diagnostics: diagnostics.into_values(),
            })
        }
    }
}

struct Line {
    quantity: Decimal,
    price: Decimal,
    basis: Decimal,
    rate: Decimal,
    expected: Option<Decimal>,
    explicit_basis: bool,
}
struct ExpectedGroup {
    rate: Decimal,
    basis: Option<Decimal>,
    tax: Option<Decimal>,
}
struct Group {
    rate: Decimal,
    basis: Decimal,
    tax: Decimal,
}

/// Budgeted binary search, shared by grouping, order and expected-group lookup.
fn find_rate<T>(
    values: &[T],
    rate: &Decimal,
    key: impl Fn(&T) -> &Decimal,
    budget: &mut Budget,
    path: &str,
) -> Result<std::result::Result<usize, usize>> {
    let (mut low, mut high) = (0, values.len());
    while low < high {
        let middle = low + (high - low) / 2;
        match key(&values[middle])
            .compare(rate, budget)
            .map_err(|e| numeric_error(path, e))?
        {
            Ordering::Equal => return Ok(Ok(middle)),
            Ordering::Less => low = middle + 1,
            Ordering::Greater => high = middle,
        }
    }
    Ok(Err(low))
}

pub(crate) fn calculate(
    draft: &InvoiceDraft,
    target: &Target,
    limits: CalculationLimits,
) -> Result<CalculatedInvoice> {
    // Recognition precedes limits, field walks and any copy of the target identifier.
    if target.bundle().is_none() {
        return Err(CalculationError::UnsupportedTarget);
    }
    let numeric_limits = limits.numeric()?;
    preflight(draft, limits)?;
    let mut cx = Context {
        budget: Budget::new(numeric_limits),
        diagnostics: Diagnostics::new(limits),
        render_bound: limits.numeric_output_bytes,
    };
    crate::fields::collect(draft, &mut cx.diagnostics);
    cx.diagnostics.check()?;

    let one =
        Decimal::parse_decimal("1", &mut cx.budget).map_err(|e| numeric_error("calculation", e))?;
    let hundred = Decimal::parse_decimal("100", &mut cx.budget)
        .map_err(|e| numeric_error("calculation", e))?;
    let mut lines = Vec::new();
    for (index, line) in draft.lines.iter().enumerate() {
        let path = format!("lines[{index}]");
        let quantity = cx.parse(&line.quantity.value, &format!("{path}.quantity.value"))?;
        let price = cx.parse(&line.price.amount, &format!("{path}.price.amount"))?;
        cx.sign(&price, false, &format!("{path}.price.amount"), &["BR-27"])?;
        let basis = if let Some(basis) = &line.price.basis {
            let parsed = cx.parse(&basis.value, &format!("{path}.price.basis.value"))?;
            cx.sign(
                &parsed,
                true,
                &format!("{path}.price.basis.value"),
                &["PEPPOL-EN16931-R121"],
            )?;
            if basis
                .unit
                .as_ref()
                .is_some_and(|unit| unit != &line.quantity.unit)
            {
                cx.diagnostic(
                    "API-BUILD-BASIS-UNIT",
                    &format!("{path}.price.basis.unit"),
                    "Price basis unit must equal the invoiced quantity unit",
                    &["PEPPOL-EN16931-R130"],
                )?;
            }
            parsed
        } else {
            Some(one.clone())
        };
        let rate = cx.parse(&line.vat.rate, &format!("{path}.vat.rate"))?;
        if line.vat.category == "S" {
            cx.sign(&rate, true, &format!("{path}.vat.rate"), &["BR-S-05"])?;
        }
        let expected = cx.optional(&line.expected_net, &format!("{path}.expected_net"))?;
        if let (Some(quantity), Some(price), Some(basis), Some(rate)) =
            (quantity, price, basis, rate)
        {
            lines.push(Line {
                quantity,
                price,
                basis,
                rate,
                expected,
                explicit_basis: line.price.basis.is_some(),
            });
        }
    }
    let mut expectations = Vec::new();
    for (index, group) in draft.expected.vat_groups.iter().enumerate() {
        let path = format!("expected.vat_groups[{index}]");
        let rate = cx.parse(&group.rate, &format!("{path}.rate"))?;
        let basis = cx.optional(&group.basis, &format!("{path}.basis"))?;
        let tax = cx.optional(&group.tax, &format!("{path}.tax"))?;
        if let Some(rate) = rate {
            expectations.push(ExpectedGroup { rate, basis, tax });
        }
    }
    let mut expected_totals = Vec::new();
    for (name, value) in [
        ("net", &draft.expected.net),
        ("tax", &draft.expected.tax),
        ("gross", &draft.expected.gross),
        ("due", &draft.expected.due),
    ] {
        expected_totals.push(cx.optional(value, &format!("expected.{name}"))?);
    }
    cx.fields()?;

    let mut net = Decimal::zero(&mut cx.budget).map_err(|e| numeric_error("totals.net", e))?;
    let mut groups: Vec<Group> = Vec::new();
    let mut nets = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let path = format!("lines[{index}]");
        let product = line
            .quantity
            .mul(&line.price, &mut cx.budget)
            .map_err(|e| numeric_error(&path, e))?;
        let amount = product
            .rounded_divide(
                &line.basis,
                2,
                RoundingMode::HalfAwayFromZero,
                &mut cx.budget,
            )
            .map_err(|e| numeric_error(&path, e))?;
        cx.expectation(
            draft.lines[index].expected_net.as_deref(),
            line.expected.as_ref(),
            &amount,
            &format!("{path}.expected_net"),
            draft.currency(),
        )?;
        net = net
            .add(&amount, &mut cx.budget)
            .map_err(|e| numeric_error("totals.net", e))?;
        match find_rate(
            &groups,
            &line.rate,
            |g| &g.rate,
            &mut cx.budget,
            &format!("{path}.vat.rate"),
        )? {
            Ok(index) => {
                groups[index].basis = groups[index]
                    .basis
                    .add(&amount, &mut cx.budget)
                    .map_err(|e| numeric_error(&path, e))?
            }
            Err(index) => groups.insert(
                index,
                Group {
                    rate: line.rate.clone(),
                    basis: amount.clone(),
                    tax: Decimal::zero(&mut cx.budget).map_err(|e| numeric_error(&path, e))?,
                },
            ),
        }
        nets.push(amount);
    }
    let mut tax = Decimal::zero(&mut cx.budget).map_err(|e| numeric_error("totals.tax", e))?;
    for group in &mut groups {
        let product = group
            .basis
            .mul(&group.rate, &mut cx.budget)
            .map_err(|e| numeric_error("vat_groups.tax", e))?;
        group.tax = product
            .rounded_divide(&hundred, 2, RoundingMode::HalfAwayFromZero, &mut cx.budget)
            .map_err(|e| numeric_error("vat_groups.tax", e))?;
        tax = tax
            .add(&group.tax, &mut cx.budget)
            .map_err(|e| numeric_error("totals.tax", e))?;
    }
    let gross = net
        .add(&tax, &mut cx.budget)
        .map_err(|e| numeric_error("totals.gross", e))?;
    let mut seen: Vec<Decimal> = Vec::new();
    for (index, expected) in expectations.iter().enumerate() {
        let path = format!("expected.vat_groups[{index}]");
        match find_rate(
            &seen,
            &expected.rate,
            |rate| rate,
            &mut cx.budget,
            &format!("{path}.rate"),
        )? {
            Ok(_) => cx.diagnostic(
                "API-BUILD-DUPLICATE-GROUP",
                &format!("{path}.rate"),
                "Repeated numeric VAT rate makes this expectation ambiguous",
                &[],
            )?,
            Err(index) => seen.insert(index, expected.rate.clone()),
        }
        match find_rate(
            &groups,
            &expected.rate,
            |g| &g.rate,
            &mut cx.budget,
            &format!("{path}.rate"),
        )? {
            Ok(group) => {
                let supplied = &draft.expected.vat_groups[index];
                cx.expectation(
                    supplied.basis.as_deref(),
                    expected.basis.as_ref(),
                    &groups[group].basis,
                    &format!("{path}.basis"),
                    draft.currency(),
                )?;
                cx.expectation(
                    supplied.tax.as_deref(),
                    expected.tax.as_ref(),
                    &groups[group].tax,
                    &format!("{path}.tax"),
                    draft.currency(),
                )?;
            }
            Err(_) => cx.diagnostic(
                "API-BUILD-UNKNOWN-GROUP",
                &format!("{path}.rate"),
                "Expected VAT rate has no calculated line group",
                &[],
            )?,
        }
    }
    for (index, (name, supplied, computed)) in [
        ("net", &draft.expected.net, &net),
        ("tax", &draft.expected.tax, &tax),
        ("gross", &draft.expected.gross, &gross),
        ("due", &draft.expected.due, &gross),
    ]
    .into_iter()
    .enumerate()
    {
        cx.expectation(
            supplied.as_deref(),
            expected_totals[index].as_ref(),
            computed,
            &format!("expected.{name}"),
            draft.currency(),
        )?;
    }
    cx.fields()?;

    cx.diagnostics
        .output()
        .take(size_of::<CalculatedInvoice>(), "calculation")?;
    cx.diagnostics
        .output()
        .take(draft.currency().len(), "currency")?;
    let currency = draft.currency().into();
    let mut calculated_lines = Vec::new();
    for (index, (line, amount)) in lines.iter().zip(&nets).enumerate() {
        let path = format!("lines[{index}]");
        cx.diagnostics
            .output()
            .take(size_of::<CalculatedLine>(), &path)?;
        calculated_lines.push(CalculatedLine {
            quantity: cx.render(&line.quantity, false, &format!("{path}.quantity.value"))?,
            price: cx.render(&line.price, false, &format!("{path}.price.amount"))?,
            price_basis: if line.explicit_basis {
                Some(cx.render(&line.basis, false, &format!("{path}.price.basis.value"))?)
            } else {
                None
            },
            vat_rate: cx.render(&line.rate, false, &format!("{path}.vat.rate"))?,
            net: cx.render(amount, true, &format!("{path}.net"))?,
        });
    }
    let mut vat_groups = Vec::new();
    for group in &groups {
        cx.diagnostics
            .output()
            .take(size_of::<CalculatedVatGroup>(), "vat_groups")?;
        vat_groups.push(CalculatedVatGroup {
            rate: cx.render(&group.rate, false, "vat_groups.rate")?,
            basis: cx.render(&group.basis, true, "vat_groups.basis")?,
            tax: cx.render(&group.tax, true, "vat_groups.tax")?,
        });
    }
    let totals = CalculatedTotals {
        net: cx.render(&net, true, "totals.net")?,
        tax: cx.render(&tax, true, "totals.tax")?,
        gross: cx.render(&gross, true, "totals.gross")?,
        due: cx.render(&gross, true, "totals.due")?,
    };
    Ok(CalculatedInvoice {
        currency,
        lines: calculated_lines,
        vat_groups,
        totals,
    })
}
