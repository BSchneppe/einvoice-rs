//! Domain field checks, deliberately separate from native validation.

use crate::{
    AccountIdentifier, Contact, ContactPoint, Identifier, InvoiceDraft, PartyDraft, PaymentMeans,
    PostalAddress,
};

/// A draft field finding, never a native validation report or conformance decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildDiagnostic {
    /// Stable API code. Unsupported-feature codes describe creation capabilities.
    pub code: &'static str,
    /// Public field path with zero-based vector indices. Alternative requirements
    /// use the first candidate field's path and name all alternatives in the message.
    pub path: String,
    /// Explanation of the missing, supplied invalid, or unsupported input.
    pub message: String,
    /// Supporting source rules, when established by the pinned field evidence.
    /// These are references, not claims that a native rule engine executed.
    /// API-only lexical checks and capability limits have no source rule IDs.
    pub rule_ids: Vec<&'static str>,
}

pub(crate) fn diagnose(draft: &InvoiceDraft) -> Vec<BuildDiagnostic> {
    let mut diagnostics = crate::calculation_bounds::Diagnostics::unbounded();
    collect(draft, &mut diagnostics);
    diagnostics.into_values()
}

pub(crate) fn collect(
    draft: &InvoiceDraft,
    diagnostics: &mut crate::calculation_bounds::Diagnostics,
) {
    let mut checks = Checks { diagnostics };
    checks.required(draft.invoice_number(), "invoice_number", &["BR-02"]);
    checks.required(draft.currency(), "currency", &["BR-05"]);
    checks.required(&draft.invoice_type, "invoice_type", &["BR-04"]);
    if !blank(&draft.invoice_type) && draft.invoice_type != "380" {
        checks.push(
            "API-BUILD-UNSUPPORTED-TYPE",
            "invoice_type",
            "Creation supports invoice type 380 only",
            &[],
        );
    }
    checks.required(&draft.issue_date, "issue_date", &["BR-03"]);
    checks.date(&draft.issue_date, "issue_date");
    checks.required(
        &draft.business_process,
        "business_process",
        &["PEPPOL-EN16931-R001"],
    );
    checks.required(
        &draft.references.buyer_reference,
        "references.buyer_reference",
        &["BR-DE-15"],
    );
    for (name, value) in [
        ("purchase_order", &draft.references.purchase_order),
        ("seller_order", &draft.references.seller_order),
        ("contract", &draft.references.contract),
    ] {
        checks.optional(value, &format!("references.{name}"));
    }

    if let Some(seller) = &draft.seller {
        checks.party(&seller.party, "seller.party", true);
        checks.optional(&seller.tax_registration, "seller.tax_registration");
        if !identifier_present(&seller.party.identifier)
            && !identifier_present(&seller.party.legal_registration)
            && !present(&seller.party.vat_identifier)
        {
            checks.push("API-BUILD-REQUIRED", "seller.party.identifier",
                "Seller needs an identifier, legal registration or VAT identifier; FC tax registration alone is insufficient",
                &["BR-CO-26"]);
        }
    } else {
        checks.missing("seller", &["BR-06", "BR-08"]);
    }
    if let Some(buyer) = &draft.buyer {
        checks.party(buyer, "buyer", false);
    } else {
        checks.missing("buyer", &["BR-07", "BR-10"]);
    }
    if let Some(representative) = &draft.tax_representative {
        checks.required(&representative.name, "tax_representative.name", &["BR-18"]);
        checks.address(
            representative.address.as_ref(),
            "tax_representative.address",
            &["BR-19", "BR-20"],
            None,
        );
        checks.required(
            &representative.vat_identifier,
            "tax_representative.vat_identifier",
            &["BR-56"],
        );
    }
    if draft.lines.iter().any(|line| line.vat.category == "S") {
        let seller_tax = draft.seller.as_ref().is_some_and(|seller| {
            present(&seller.party.vat_identifier) || present(&seller.tax_registration)
        });
        let representative_tax = draft
            .tax_representative
            .as_ref()
            .is_some_and(|representative| !blank(&representative.vat_identifier));
        if !seller_tax && !representative_tax {
            checks.push("API-BUILD-REQUIRED", "seller.tax_registration",
                "Standard VAT lines need seller VAT ID, seller tax registration or representative VAT ID",
                &["BR-S-02", "BR-DE-16"]);
        }
    }

    if let Some(payment) = &draft.payment {
        let account_rules: &[&str] = match &payment.means {
            PaymentMeans::CreditTransfer | PaymentMeans::SepaCreditTransfer => &["BR-50", "BR-61"],
            PaymentMeans::Unsupported(_) => &[],
        };
        match &payment.means {
            PaymentMeans::CreditTransfer | PaymentMeans::SepaCreditTransfer => {
                if payment.accounts.is_empty() {
                    checks.missing("payment.accounts", &["BR-DE-23-a"]);
                }
            }
            PaymentMeans::Unsupported(code) => {
                checks.required(code, "payment.means", &[]);
                checks.push("API-BUILD-UNSUPPORTED-PAYMENT", "payment.means",
                    "Creation supports ordinary credit transfer (30) and SEPA credit transfer (58) only", &[]);
            }
        }
        checks.optional(&payment.description, "payment.description");
        checks.optional(&payment.reference, "payment.reference");
        for (index, account) in payment.accounts.iter().enumerate() {
            let path = format!("payment.accounts[{index}]");
            let value = match &account.identifier {
                AccountIdentifier::Iban(value) | AccountIdentifier::Proprietary(value) => value,
            };
            checks.required(value, &format!("{path}.identifier"), account_rules);
            checks.optional(&account.name, &format!("{path}.name"));
            checks.optional(&account.bic, &format!("{path}.bic"));
        }
    } else {
        checks.missing("payment", &["BR-DE-1"]);
    }
    checks.optional(&draft.payment_terms.text, "payment_terms.text");
    if draft.payment_terms.text.as_ref().is_some_and(|text| {
        text.split('\n').any(|line| {
            line.trim_start_matches([' ', '\t', '\r', '\n'])
                .starts_with('#')
        })
    }) {
        checks.push(
            "API-BUILD-UNSUPPORTED-TERMS",
            "payment_terms.text",
            "Creation supports plain prose only; hash-start lines request unsupported structured payment terms",
            &[],
        );
    }
    checks.optional(&draft.payment_terms.due_date, "payment_terms.due_date");
    if let Some(date) = &draft.payment_terms.due_date {
        checks.date(date, "payment_terms.due_date");
    }

    if draft.lines.is_empty() {
        checks.missing("lines", &["BR-16"]);
    }
    for (index, line) in draft.lines.iter().enumerate() {
        let path = format!("lines[{index}]");
        checks.required(&line.id, &format!("{path}.id"), &["BR-21"]);
        checks.required(&line.name, &format!("{path}.name"), &["BR-25"]);
        checks.required(
            &line.quantity.value,
            &format!("{path}.quantity.value"),
            &["BR-22"],
        );
        checks.required(
            &line.quantity.unit,
            &format!("{path}.quantity.unit"),
            &["BR-23"],
        );
        checks.required(
            &line.price.amount,
            &format!("{path}.price.amount"),
            &["BR-26"],
        );
        if let Some(basis) = &line.price.basis {
            checks.required(&basis.value, &format!("{path}.price.basis.value"), &[]);
            checks.optional(&basis.unit, &format!("{path}.price.basis.unit"));
        }
        checks.required(&line.vat.category, &format!("{path}.vat.category"), &[]);
        if !blank(&line.vat.category) && line.vat.category != "S" {
            checks.push(
                "API-BUILD-UNSUPPORTED-VAT-CATEGORY",
                &format!("{path}.vat.category"),
                "Creation supports standard VAT category S only",
                &[],
            );
        }
        // Presence only: positivity/lexical form awaits the shared numeric kernel.
        let rate_rules: &[&str] = if line.vat.category == "S" {
            &["BR-S-05"]
        } else {
            &[]
        };
        checks.required(&line.vat.rate, &format!("{path}.vat.rate"), rate_rules);
        checks.optional(&line.expected_net, &format!("{path}.expected_net"));
    }
    for (index, group) in draft.expected.vat_groups.iter().enumerate() {
        let path = format!("expected.vat_groups[{index}]");
        checks.required(&group.rate, &format!("{path}.rate"), &[]);
        checks.optional(&group.basis, &format!("{path}.basis"));
        checks.optional(&group.tax, &format!("{path}.tax"));
    }
    for (name, value) in [
        ("net", &draft.expected.net),
        ("tax", &draft.expected.tax),
        ("gross", &draft.expected.gross),
        ("due", &draft.expected.due),
    ] {
        checks.optional(value, &format!("expected.{name}"));
    }
}

struct Checks<'a> {
    diagnostics: &'a mut crate::calculation_bounds::Diagnostics,
}

impl Checks<'_> {
    fn push(&mut self, code: &'static str, path: &str, message: &str, rules: &[&'static str]) {
        self.diagnostics.push(code, path, message, rules);
    }

    fn missing(&mut self, path: &str, rules: &[&'static str]) {
        self.push(
            "API-BUILD-REQUIRED",
            path,
            "Required field or group is missing or blank",
            rules,
        );
    }

    fn required(&mut self, value: &str, path: &str, rules: &[&'static str]) {
        if blank(value) {
            self.missing(path, rules);
        }
        self.xml(value, path);
    }

    fn optional(&mut self, value: &Option<String>, path: &str) {
        self.optional_text(value.as_deref(), path);
    }

    fn optional_text(&mut self, value: Option<&str>, path: &str) {
        if let Some(value) = value {
            if blank(value) {
                self.push(
                    "API-BUILD-EMPTY",
                    path,
                    "Supplied optional text is blank; use None to omit it",
                    &[],
                );
            }
            self.xml(value, path);
        }
    }

    fn xml(&mut self, value: &str, path: &str) {
        if value.chars().any(|c| !matches!(c, '\u{9}' | '\u{A}' | '\u{D}' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}')) {
            self.push("API-BUILD-XML-CHAR", path, "Text contains a character forbidden in XML 1.0", &[]);
        }
    }

    fn date(&mut self, value: &str, path: &str) {
        if !blank(value) && !calendar_date(value) {
            self.push(
                "API-BUILD-DATE",
                path,
                "Date must be a calendar-valid YYYYMMDD value (years 0001–9999)",
                &[],
            );
        }
    }

    fn identifier(&mut self, value: &Option<Identifier>, path: &str) {
        if let Some(identifier) = value {
            self.required(&identifier.value, &format!("{path}.value"), &[]);
            self.optional(&identifier.scheme, &format!("{path}.scheme"));
        }
    }

    fn party(&mut self, party: &PartyDraft, path: &str, seller: bool) {
        self.required(
            &party.name,
            &format!("{path}.name"),
            if seller { &["BR-06"] } else { &["BR-07"] },
        );
        let (address_rules, locality_rules) = if seller {
            (["BR-08", "BR-09"], ["BR-DE-3", "BR-DE-4"])
        } else {
            (["BR-10", "BR-11"], ["BR-DE-8", "BR-DE-9"])
        };
        self.address(
            party.address.as_ref(),
            &format!("{path}.address"),
            &address_rules,
            Some(locality_rules),
        );
        let endpoint_path = format!("{path}.endpoint");
        let endpoint_rule = if seller {
            "PEPPOL-EN16931-R020"
        } else {
            "PEPPOL-EN16931-R010"
        };
        if let Some(endpoint) = &party.endpoint {
            self.required(
                &endpoint.value,
                &format!("{endpoint_path}.value"),
                &[endpoint_rule],
            );
            self.required(
                &endpoint.scheme,
                &format!("{endpoint_path}.scheme"),
                if seller { &["BR-62"] } else { &["BR-63"] },
            );
        } else {
            self.missing(&endpoint_path, &[endpoint_rule]);
        }
        if let Some(contact) = &party.contact {
            self.contact(contact, &format!("{path}.contact"), seller);
        } else if seller {
            self.missing(&format!("{path}.contact"), &["BR-DE-2"]);
        }
        self.identifier(&party.identifier, &format!("{path}.identifier"));
        self.identifier(
            &party.legal_registration,
            &format!("{path}.legal_registration"),
        );
        self.optional(&party.vat_identifier, &format!("{path}.vat_identifier"));
    }

    fn address(
        &mut self,
        address: Option<&PostalAddress>,
        path: &str,
        rules: &[&'static str; 2],
        locality_rules: Option<[&'static str; 2]>,
    ) {
        let Some(address) = address else {
            self.missing(path, &rules[..1]);
            return;
        };
        self.required(&address.country, &format!("{path}.country"), &rules[1..]);
        for (index, (name, value)) in [("city", &address.city), ("postcode", &address.postcode)]
            .into_iter()
            .enumerate()
        {
            let field = format!("{path}.{name}");
            if let Some(locality_rules) = locality_rules {
                self.required(
                    value.as_deref().unwrap_or(""),
                    &field,
                    &[locality_rules[index]],
                );
            } else {
                self.optional(value, &field);
            }
        }
        for (name, value) in [
            ("line_one", &address.line_one),
            ("line_two", &address.line_two),
            ("line_three", &address.line_three),
            ("country_subdivision", &address.country_subdivision),
        ] {
            self.optional(value, &format!("{path}.{name}"));
        }
    }

    fn contact(&mut self, contact: &Contact, path: &str, seller: bool) {
        let point = contact.point.as_ref().map(|point| match point {
            ContactPoint::Person(value) | ContactPoint::Department(value) => value.as_str(),
        });
        for (name, value, rule) in [
            ("point", point, "BR-DE-5"),
            ("telephone", contact.telephone.as_deref(), "BR-DE-6"),
            ("email", contact.email.as_deref(), "BR-DE-7"),
        ] {
            let field = format!("{path}.{name}");
            if seller {
                self.required(value.unwrap_or(""), &field, &[rule]);
            } else {
                self.optional_text(value, &field);
            }
        }
        if !seller && point.is_none() && contact.telephone.is_none() && contact.email.is_none() {
            self.push(
                "API-BUILD-EMPTY",
                path,
                "Supplied buyer contact has no fields; use None to omit it",
                &[],
            );
        }
    }
}

// XML normalize-space whitespace only. Never trim or mutate the caller's text.
fn blank(value: &str) -> bool {
    value.chars().all(|c| matches!(c, ' ' | '\t' | '\r' | '\n'))
}

fn present(value: &Option<String>) -> bool {
    value.as_ref().is_some_and(|value| !blank(value))
}

fn identifier_present(value: &Option<Identifier>) -> bool {
    value
        .as_ref()
        .is_some_and(|identifier| !blank(&identifier.value))
}

// Calendar lexical check only; this is not a numeric input parser or native rule.
fn calendar_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 8 || !bytes.iter().all(u8::is_ascii_digit) {
        return false;
    }
    let digits = |slice: &[u8]| {
        slice
            .iter()
            .fold(0u16, |n, digit| n * 10 + u16::from(digit - b'0'))
    };
    let year = digits(&bytes[..4]);
    let month = digits(&bytes[4..6]);
    let day = digits(&bytes[6..]);
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => return false,
    };
    year != 0 && day != 0 && day <= days
}
