//! Business input for the bounded type-380, standard-VAT creation path.
//! All values remain caller input, including malformed decimal text.

/// A business identifier whose scheme may legally be absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identifier {
    /// Original identifier text.
    pub value: String,
    /// Optional scheme; no scheme is invented when absent.
    pub scheme: Option<String>,
}

impl Identifier {
    /// Construct an identifier without a scheme.
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            scheme: None,
        }
    }
}

/// Electronic delivery address, distinct from a contact email or buyer reference.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ElectronicAddress {
    /// Original address or identifier text; need not be an email.
    pub value: String,
    /// Required electronic-address scheme. Code-list checking is deferred.
    pub scheme: String,
}

impl ElectronicAddress {
    /// Construct an endpoint with its caller-selected scheme.
    pub fn new(value: impl Into<String>, scheme: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            scheme: scheme.into(),
        }
    }
}

/// Postal address. Street lines are optional; seller/buyer city and postcode are required.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PostalAddress {
    /// Country code, without an artificial DE-only restriction.
    pub country: String,
    /// City; optional for a tax representative, required for seller/buyer.
    pub city: Option<String>,
    /// Postcode; optional for a tax representative, required for seller/buyer.
    pub postcode: Option<String>,
    /// First optional address line.
    pub line_one: Option<String>,
    /// Second optional address line.
    pub line_two: Option<String>,
    /// Third optional address line.
    pub line_three: Option<String>,
    /// Optional region or subdivision name.
    pub country_subdivision: Option<String>,
}

/// Exactly one representation of a contact point; person and department cannot coexist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContactPoint {
    /// Named individual.
    Person(String),
    /// Named department.
    Department(String),
}

/// Contact information. All three fields are required for the seller only.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Contact {
    /// Individual or department.
    pub point: Option<ContactPoint>,
    /// Telephone text; no E.164-only restriction is imposed.
    pub telephone: Option<String>,
    /// Contact email, separate from the electronic delivery endpoint.
    pub email: Option<String>,
}

/// Common seller/buyer details, separate from seller-specific tax alternatives.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PartyDraft {
    /// Required party name.
    pub name: String,
    /// Required postal address.
    pub address: Option<PostalAddress>,
    /// Required electronic delivery endpoint.
    pub endpoint: Option<ElectronicAddress>,
    /// Mandatory for seller, optional (including individual fields) for buyer.
    pub contact: Option<Contact>,
    /// Local identifier when scheme is absent; global identifier when supplied.
    pub identifier: Option<Identifier>,
    /// Legal registration identifier; its scheme may be absent.
    pub legal_registration: Option<Identifier>,
    /// VAT identifier (VA); optional for an ordinary buyer.
    pub vat_identifier: Option<String>,
}

impl PartyDraft {
    /// Start party details with a name and no invented address or identity.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }
}

/// Seller details and its optional local tax registration (FC).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SellerDraft {
    /// Shared party fields. VAT identity is `party.vat_identifier` (VA).
    pub party: PartyDraft,
    /// Local tax registration (FC). Alone this does not establish seller identity.
    pub tax_registration: Option<String>,
}

impl SellerDraft {
    /// Start a seller with a name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            party: PartyDraft::new(name),
            tax_registration: None,
        }
    }
}

/// Optional seller tax representative; supplying it requires name, address and VAT ID.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaxRepresentativeDraft {
    /// Required representative name.
    pub name: String,
    /// Required address; only country is mandatory within this address.
    pub address: Option<PostalAddress>,
    /// Required representative VAT identifier (VA).
    pub vat_identifier: String,
}

/// Small set of ordinary invoice references; purchase order never replaces buyer reference.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InvoiceReferences {
    /// Required caller-supplied routing/business reference.
    pub buyer_reference: String,
    /// Optional purchase order number.
    pub purchase_order: Option<String>,
    /// Optional seller order number.
    pub seller_order: Option<String>,
    /// Optional contract reference.
    pub contract: Option<String>,
}

/// Transfer account identifier. No BIC or account name is implied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountIdentifier {
    /// IBAN text; checksum/code-list checks are not performed in this slice.
    Iban(String),
    /// Local or other proprietary account identifier.
    Proprietary(String),
}

/// One receiving account; multiple accounts retain caller order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferAccount {
    /// Required account identifier.
    pub identifier: AccountIdentifier,
    /// Optional account holder name.
    pub name: Option<String>,
    /// Optional bank identifier.
    pub bic: Option<String>,
}

impl TransferAccount {
    /// Construct an account without optional bank/name data.
    pub fn new(identifier: AccountIdentifier) -> Self {
        Self {
            identifier,
            name: None,
            bic: None,
        }
    }
}

/// Supported transfer means, or an explicit unsupported creation request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum PaymentMeans {
    /// Ordinary credit transfer (30), supporting IBAN or proprietary accounts.
    #[default]
    CreditTransfer,
    /// SEPA credit transfer (58). Source IBAN advisories remain deferred.
    SepaCreditTransfer,
    /// Other payment code, preserved and diagnosed as a creation capability limit.
    Unsupported(String),
}

/// Common payment instructions for all supplied receiving accounts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PaymentInstructions {
    /// One means shared by all accounts.
    pub means: PaymentMeans,
    /// At least one account for transfer means.
    pub accounts: Vec<TransferAccount>,
    /// Optional payment means description.
    pub description: Option<String>,
    /// Optional remittance reference; never defaulted to invoice number.
    pub reference: Option<String>,
}

/// Optional plain payment terms for the pinned target.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PaymentTerms {
    /// Optional date in calendar-valid `YYYYMMDD` form.
    pub due_date: Option<String>,
    /// Optional original prose. No discount interpretation is performed.
    pub text: Option<String>,
}

/// Unparsed invoiced quantity and unit. Signed and zero quantity text is preserved.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Quantity {
    /// Original decimal input, not checked for lexical form or sign here.
    pub value: String,
    /// Required caller-supplied unit code; code-list membership is deferred.
    pub unit: String,
}

/// Optional net price basis; absence alone will default to one during later preparation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PriceBasis {
    /// Original decimal input. Zero/malformed values are never replaced with one.
    pub value: String,
    /// Optional basis unit. Equality with quantity unit is checked later.
    pub unit: Option<String>,
}

/// Unparsed net price input, without intermediate numeric conversion.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetPrice {
    /// Required original amount text; numeric sign/lexical checks are deferred.
    pub amount: String,
    /// Optional original basis; only absence can later mean one.
    pub basis: Option<PriceBasis>,
}

/// VAT category and unparsed rate for a line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineVat {
    /// Only `S` is supported for creation; other supplied categories are diagnosed.
    pub category: String,
    /// Original rate text; positivity and numeric lexical checks are deferred.
    pub rate: String,
}

impl Default for LineVat {
    fn default() -> Self {
        Self {
            category: "S".into(),
            rate: String::new(),
        }
    }
}

/// Ordered line input. No derived amount is calculated or caller expectation replaced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InvoiceLineDraft {
    /// Required original line identifier; no automatic renumbering.
    pub id: String,
    /// Required item name.
    pub name: String,
    /// Required quantity text and unit.
    pub quantity: Quantity,
    /// Required net price text and optional basis.
    pub price: NetPrice,
    /// VAT category and rate input.
    pub vat: LineVat,
    /// Optional exact line net expectation; comparison belongs to later preparation.
    pub expected_net: Option<String>,
}

impl InvoiceLineDraft {
    /// Start a line with its caller-supplied ID and item name.
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            ..Self::default()
        }
    }
}

/// Caller expectations for one standard VAT rate group; none are calculated here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VatGroupExpectation {
    /// Original decimal rate identifying the group during later numeric preparation.
    pub rate: String,
    /// Optional exact taxable basis expectation.
    pub basis: Option<String>,
    /// Optional exact tax expectation.
    pub tax: Option<String>,
}

/// Optional exact expectations, preserved for later shared-kernel comparisons.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InvoiceExpectations {
    /// Group expectations in caller order, without lexical rate grouping.
    pub vat_groups: Vec<VatGroupExpectation>,
    /// Expected net amount.
    pub net: Option<String>,
    /// Expected VAT total.
    pub tax: Option<String>,
    /// Expected gross amount.
    pub gross: Option<String>,
    /// Expected amount due.
    pub due: Option<String>,
}
