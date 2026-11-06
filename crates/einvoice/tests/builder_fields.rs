use einvoice::{
    AccountIdentifier, BuildDiagnostic, BuildError, Contact, ContactPoint, ElectronicAddress,
    Identifier, InvoiceDraft, InvoiceLineDraft, NetPrice, PartyDraft, PaymentInstructions,
    PaymentMeans, PostalAddress, PriceBasis, Quantity, SellerDraft, Target, TaxRepresentativeDraft,
    TransferAccount, VatGroupExpectation,
};

// Original business input, not an oracle fixture or a claim of conformance.
fn party(name: &str) -> PartyDraft {
    PartyDraft {
        address: Some(PostalAddress {
            country: "NL".into(),
            city: Some("Utrecht".into()),
            postcode: Some("3511 AA".into()),
            ..PostalAddress::default()
        }),
        endpoint: Some(ElectronicAddress::new("routing-identifier", "0088")),
        ..PartyDraft::new(name)
    }
}

fn line(id: &str) -> InvoiceLineDraft {
    InvoiceLineDraft {
        quantity: Quantity {
            value: "1".into(),
            unit: "HUR".into(),
        },
        price: NetPrice {
            amount: "10.00".into(),
            basis: None,
        },
        vat: einvoice::LineVat {
            category: "S".into(),
            rate: "21.5".into(),
        },
        ..InvoiceLineDraft::new(id, "Consulting")
    }
}

fn populated() -> InvoiceDraft {
    let mut draft = InvoiceDraft::new("INV-2026-1", "USD");
    draft.issue_date = "20261106".into();
    draft.business_process = "caller-selected-process".into();
    draft.references.buyer_reference = "caller-routing-reference".into();
    let mut seller_party = party("Seller");
    seller_party.vat_identifier = Some("NL123456789B01".into());
    seller_party.contact = Some(Contact {
        point: Some(ContactPoint::Department("Billing".into())),
        telephone: Some("+31 (0)30 123 4567".into()),
        email: Some("billing@example.test".into()),
    });
    draft.seller = Some(SellerDraft {
        party: seller_party,
        tax_registration: None,
    });
    draft.buyer = Some(party("Buyer"));
    draft.payment = Some(PaymentInstructions {
        accounts: vec![TransferAccount::new(AccountIdentifier::Proprietary(
            "local-account-001".into(),
        ))],
        ..PaymentInstructions::default()
    });
    draft.lines.push(line("first"));
    draft
}

fn finding<'a>(diagnostics: &'a [BuildDiagnostic], path: &str) -> &'a BuildDiagnostic {
    diagnostics
        .iter()
        .find(|diagnostic| diagnostic.path == path)
        .unwrap_or_else(|| panic!("missing diagnostic at {path}: {diagnostics:?}"))
}

#[test]
fn constructor_getters_and_unavailable_encoding_remain_compatible() {
    let draft = InvoiceDraft::new(" INV / original ", "EUR");
    assert_eq!(draft.invoice_number(), " INV / original ");
    assert_eq!(draft.currency(), "EUR");
    assert_eq!(draft.invoice_type, "380");
    assert!(draft.business_process.is_empty());
    assert!(draft.references.buyer_reference.is_empty());
    assert!(matches!(
        Target::xrechnung_3_0_2().encode(&draft),
        Err(BuildError::Unavailable { .. })
    ));
    assert!(matches!(
        Target::xrechnung_3_0_2().encode(&populated()),
        Err(BuildError::Unavailable { .. })
    ));
}

#[test]
fn incomplete_draft_reports_stable_fields_codes_messages_and_supported_rules() {
    let diagnostics = InvoiceDraft::new("", "").field_diagnostics();
    let paths: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.path.as_str())
        .collect();
    assert_eq!(
        paths,
        [
            "invoice_number",
            "currency",
            "issue_date",
            "business_process",
            "references.buyer_reference",
            "seller",
            "buyer",
            "payment",
            "lines"
        ]
    );
    for diagnostic in &diagnostics {
        assert_eq!(diagnostic.code, "API-BUILD-REQUIRED");
        assert_eq!(
            diagnostic.message,
            "Required field or group is missing or blank"
        );
        assert!(!diagnostic.rule_ids.is_empty());
    }
    assert_eq!(
        finding(&diagnostics, "business_process").rule_ids,
        ["PEPPOL-EN16931-R001"]
    );
    assert_eq!(
        finding(&diagnostics, "references.buyer_reference").rule_ids,
        ["BR-DE-15"]
    );
    assert_eq!(finding(&diagnostics, "lines").rule_ids, ["BR-16"]);
}

#[test]
fn populated_required_fields_have_no_missing_findings_but_are_not_conformance() {
    let draft = populated();
    assert_eq!(draft.field_diagnostics(), []);
    // Includes no street, buyer contact/ID/VAT ID, BIC, account name, terms or due date.
    // The lack of field findings still cannot produce an invoice.
    assert!(matches!(
        Target::xrechnung_3_0_2().encode(&draft),
        Err(BuildError::Unavailable { .. })
    ));
}

#[test]
fn seller_identity_and_tax_identity_are_separate_alternatives() {
    let mut draft = populated();
    assert!(draft.field_diagnostics().is_empty()); // VA satisfies both obligations.
    let seller = draft.seller.as_mut().unwrap();
    seller.party.vat_identifier = None;
    seller.tax_registration = Some("local-tax-registration".into());
    let diagnostics = draft.field_diagnostics();
    assert_eq!(diagnostics.len(), 1);
    let identity = finding(&diagnostics, "seller.party.identifier");
    assert_eq!(identity.rule_ids, ["BR-CO-26"]);
    assert!(identity
        .message
        .contains("FC tax registration alone is insufficient"));

    draft.seller.as_mut().unwrap().party.identifier = Some(Identifier::new("local-seller"));
    assert!(draft.field_diagnostics().is_empty()); // FC + local ID.
    let seller = draft.seller.as_mut().unwrap();
    assert_eq!(seller.party.identifier.as_ref().unwrap().scheme, None);
    seller.party.identifier = None;
    seller.party.legal_registration = Some(Identifier::new("legal-registration"));
    assert!(draft.field_diagnostics().is_empty()); // FC + legal ID without scheme.

    draft.seller.as_mut().unwrap().tax_registration = None;
    let diagnostics = draft.field_diagnostics();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        finding(&diagnostics, "seller.tax_registration").rule_ids,
        ["BR-S-02", "BR-DE-16"]
    );
}

#[test]
fn representative_is_a_real_tax_alternative_with_conditional_name_country_and_vat() {
    let mut draft = populated();
    let seller = draft.seller.as_mut().unwrap();
    seller.party.vat_identifier = None;
    seller.party.identifier = Some(Identifier::new("local-seller"));
    draft.tax_representative = Some(TaxRepresentativeDraft::default());
    let diagnostics = draft.field_diagnostics();
    assert_eq!(
        finding(&diagnostics, "tax_representative.name").rule_ids,
        ["BR-18"]
    );
    assert_eq!(
        finding(&diagnostics, "tax_representative.address").rule_ids,
        ["BR-19"]
    );
    assert_eq!(
        finding(&diagnostics, "tax_representative.vat_identifier").rule_ids,
        ["BR-56"]
    );

    let representative = draft.tax_representative.as_mut().unwrap();
    representative.name = "Representative".into();
    representative.vat_identifier = "EL123456789".into();
    representative.address = Some(PostalAddress::default());
    let diagnostics = draft.field_diagnostics();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        finding(&diagnostics, "tax_representative.address.country").rule_ids,
        ["BR-20"]
    );
    draft
        .tax_representative
        .as_mut()
        .unwrap()
        .address
        .as_mut()
        .unwrap()
        .country = "GR".into();
    assert!(draft.field_diagnostics().is_empty()); // Representative city/postcode not required.

    draft.seller.as_mut().unwrap().party.identifier = None;
    assert_eq!(draft.field_diagnostics().len(), 1); // Representative does not establish seller identity.
    assert_eq!(draft.field_diagnostics()[0].rule_ids, ["BR-CO-26"]);
}

#[test]
fn endpoint_and_contact_email_do_not_substitute_for_each_other() {
    let mut draft = populated();
    draft.seller.as_mut().unwrap().party.endpoint = None;
    let diagnostics = draft.field_diagnostics();
    assert_eq!(
        finding(&diagnostics, "seller.party.endpoint").rule_ids,
        ["PEPPOL-EN16931-R020"]
    );
    draft.seller.as_mut().unwrap().party.endpoint =
        Some(ElectronicAddress::new("endpoint@example.test", ""));
    assert_eq!(
        finding(&draft.field_diagnostics(), "seller.party.endpoint.scheme").rule_ids,
        ["BR-62"]
    );

    draft
        .seller
        .as_mut()
        .unwrap()
        .party
        .endpoint
        .as_mut()
        .unwrap()
        .scheme = "EM".into();
    draft
        .seller
        .as_mut()
        .unwrap()
        .party
        .contact
        .as_mut()
        .unwrap()
        .email = None;
    assert_eq!(
        finding(&draft.field_diagnostics(), "seller.party.contact.email").rule_ids,
        ["BR-DE-7"]
    );
    draft.buyer.as_mut().unwrap().contact = Some(Contact {
        email: Some("buyer@example.test".into()),
        ..Contact::default()
    });
    draft.buyer.as_mut().unwrap().endpoint = None;
    assert_eq!(
        finding(&draft.field_diagnostics(), "buyer.endpoint").rule_ids,
        ["PEPPOL-EN16931-R010"]
    );
}

#[test]
fn contact_type_excludes_simultaneous_person_and_department_and_buyer_is_partial() {
    for point in [
        ContactPoint::Person("Alex".into()),
        ContactPoint::Department("Accounts".into()),
    ] {
        let mut draft = populated();
        draft
            .seller
            .as_mut()
            .unwrap()
            .party
            .contact
            .as_mut()
            .unwrap()
            .point = Some(point.clone());
        draft.buyer.as_mut().unwrap().contact = Some(Contact {
            point: Some(point),
            ..Contact::default()
        });
        assert!(draft.field_diagnostics().is_empty());
    }
    let mut draft = populated();
    draft.seller.as_mut().unwrap().party.contact = Some(Contact::default());
    let diagnostics = draft.field_diagnostics();
    assert_eq!(diagnostics.len(), 3);
    for (field, rule) in [
        ("point", "BR-DE-5"),
        ("telephone", "BR-DE-6"),
        ("email", "BR-DE-7"),
    ] {
        assert_eq!(
            finding(&diagnostics, &format!("seller.party.contact.{field}")).rule_ids,
            [rule]
        );
    }
    draft.seller.as_mut().unwrap().party.contact = None;
    assert_eq!(
        finding(&draft.field_diagnostics(), "seller.party.contact").rule_ids,
        ["BR-DE-2"]
    );

    let mut draft = populated();
    draft.buyer.as_mut().unwrap().contact = Some(Contact::default());
    let before = draft.clone();
    let diagnostics = draft.field_diagnostics();
    assert_eq!(
        diagnostics
            .iter()
            .map(|d| (d.code, d.path.as_str(), d.rule_ids.as_slice()))
            .collect::<Vec<_>>(),
        [("API-BUILD-EMPTY", "buyer.contact", &[][..])]
    );
    assert_eq!(draft, before);
}

#[test]
fn seller_and_buyer_addresses_have_role_specific_required_fields() {
    let mut draft = populated();
    draft.seller.as_mut().unwrap().party.name = " \t".into();
    draft.seller.as_mut().unwrap().party.address = Some(PostalAddress::default());
    draft.buyer.as_mut().unwrap().address = Some(PostalAddress::default());
    let diagnostics = draft.field_diagnostics();
    for (path, rule) in [
        ("seller.party.name", "BR-06"),
        ("seller.party.address.country", "BR-09"),
        ("seller.party.address.city", "BR-DE-3"),
        ("seller.party.address.postcode", "BR-DE-4"),
        ("buyer.address.country", "BR-11"),
        ("buyer.address.city", "BR-DE-8"),
        ("buyer.address.postcode", "BR-DE-9"),
    ] {
        assert_eq!(finding(&diagnostics, path).rule_ids, [rule]);
    }
    draft.buyer.as_mut().unwrap().name.clear();
    draft.buyer.as_mut().unwrap().address = None;
    let diagnostics = draft.field_diagnostics();
    assert_eq!(finding(&diagnostics, "buyer.name").rule_ids, ["BR-07"]);
    assert_eq!(finding(&diagnostics, "buyer.address").rule_ids, ["BR-10"]);
}

#[test]
fn purchase_order_cannot_replace_required_buyer_reference() {
    let mut draft = populated();
    draft.references.buyer_reference.clear();
    draft.references.purchase_order = Some("PO-123".into());
    let diagnostics = draft.field_diagnostics();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].path, "references.buyer_reference");
    assert_eq!(diagnostics[0].rule_ids, ["BR-DE-15"]);
}

#[test]
fn transfer_account_variants_and_multiple_accounts_do_not_require_bic_or_name() {
    let mut draft = populated();
    let payment = draft.payment.as_mut().unwrap();
    payment
        .accounts
        .push(TransferAccount::new(AccountIdentifier::Iban(
            "NL91ABNA0417164300".into(),
        )));
    assert!(draft.field_diagnostics().is_empty());
    draft.payment.as_mut().unwrap().means = PaymentMeans::SepaCreditTransfer;
    // BR-DE-19 is an advisory; this slice does not convert it into an error.
    assert!(draft.field_diagnostics().is_empty());
    draft.payment.as_mut().unwrap().accounts[0].identifier =
        AccountIdentifier::Proprietary(" ".into());
    assert_eq!(
        finding(&draft.field_diagnostics(), "payment.accounts[0].identifier").rule_ids,
        ["BR-50", "BR-61"]
    );
    draft.payment.as_mut().unwrap().accounts.clear();
    let diagnostics = draft.field_diagnostics();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].path, "payment.accounts");
    assert_eq!(diagnostics[0].rule_ids, ["BR-DE-23-a"]);
}

#[test]
fn original_lines_decimal_spellings_and_expectations_survive_all_field_checks() {
    let mut draft = populated();
    draft.lines[0].quantity.value = "-000.2500".into();
    draft.lines[0].price.amount = "0.029999999999999999999999999999".into();
    draft.lines[0].price.basis = Some(PriceBasis {
        value: "6.000".into(),
        unit: None,
    });
    draft.lines[0].vat.rate = "019.000".into();
    draft.lines[0].expected_net = Some("-0.001249999999999999999999999999".into());
    draft.lines.push(line("second"));
    draft.lines[1].quantity.value = "0".into();
    draft.expected.gross = Some("43.424".into());
    draft.expected.vat_groups.push(VatGroupExpectation {
        rate: "19.0".into(),
        basis: Some("43.4200".into()),
        tax: Some("malformed expectation".into()),
    });
    let before = draft.clone();
    assert!(draft.field_diagnostics().is_empty());
    assert_eq!(draft, before);
    assert_eq!(
        draft
            .lines
            .iter()
            .map(|line| line.id.as_str())
            .collect::<Vec<_>>(),
        ["first", "second"]
    );
}

#[test]
fn numeric_lexical_sign_comparison_and_equality_checks_are_explicitly_deferred() {
    for (quantity, price, basis, rate) in [
        ("-2", "0", "1", "19"),
        ("0", "-1", "0", "0"),
        ("not decimal", "1e3", "malformed", "NaN"),
    ] {
        let mut draft = populated();
        draft.lines[0].quantity.value = quantity.into();
        draft.lines[0].price.amount = price.into();
        draft.lines[0].price.basis = Some(PriceBasis {
            value: basis.into(),
            unit: Some("different unit".into()),
        });
        draft.lines[0].vat.rate = rate.into();
        draft.expected.gross = Some("not an amount".into());
        let original = draft.clone();
        assert!(draft.field_diagnostics().is_empty()); // Presence only, never validity.
        assert_eq!(draft, original);
    }
    let mut draft = populated();
    draft.lines[0].quantity.value.clear();
    draft.lines[0].price.amount.clear();
    draft.lines[0].vat.rate.clear();
    let diagnostics = draft.field_diagnostics();
    assert_eq!(
        finding(&diagnostics, "lines[0].quantity.value").rule_ids,
        ["BR-22"]
    );
    assert_eq!(
        finding(&diagnostics, "lines[0].price.amount").rule_ids,
        ["BR-26"]
    );
    assert_eq!(
        finding(&diagnostics, "lines[0].vat.rate").rule_ids,
        ["BR-S-05"]
    );
}

#[test]
fn supplied_optional_text_is_checked_without_stripping_or_treating_it_as_absent() {
    let mut draft = populated();
    draft.buyer.as_mut().unwrap().identifier = Some(Identifier {
        value: " buyer original ".into(),
        scheme: None,
    });
    draft
        .buyer
        .as_mut()
        .unwrap()
        .address
        .as_mut()
        .unwrap()
        .line_one = Some("  keep spaces  ".into());
    let before = draft.clone();
    assert!(draft.field_diagnostics().is_empty());
    assert_eq!(draft, before);
    draft.payment_terms.text = Some(" \r\n\t".into());
    draft.payment.as_mut().unwrap().accounts[0].bic = Some("".into());
    draft
        .buyer
        .as_mut()
        .unwrap()
        .identifier
        .as_mut()
        .unwrap()
        .scheme = Some("".into());
    draft
        .buyer
        .as_mut()
        .unwrap()
        .address
        .as_mut()
        .unwrap()
        .line_two = Some("bad\u{0}text".into());
    let before = draft.clone();
    let diagnostics = draft.field_diagnostics();
    assert_eq!(
        finding(&diagnostics, "payment_terms.text").code,
        "API-BUILD-EMPTY"
    );
    assert_eq!(
        finding(&diagnostics, "payment.accounts[0].bic").code,
        "API-BUILD-EMPTY"
    );
    assert_eq!(
        finding(&diagnostics, "buyer.identifier.scheme").code,
        "API-BUILD-EMPTY"
    );
    assert_eq!(
        finding(&diagnostics, "buyer.address.line_two").code,
        "API-BUILD-XML-CHAR"
    );
    assert!(diagnostics
        .iter()
        .all(|diagnostic| diagnostic.rule_ids.is_empty()));
    assert_eq!(draft, before);
}

#[test]
fn calendar_checks_are_api_invariants_and_terms_are_optional() {
    for date in ["20240229", "20000229", "00010101", "99991231"] {
        let mut draft = populated();
        draft.issue_date = date.into();
        assert!(draft.field_diagnostics().is_empty());
    }
    for date in [
        "20260229",
        "19000229",
        "20260431",
        "00000101",
        "20260010",
        "20260100",
        "2026-11-06",
        " 20261106",
        "２０２６１１０６",
    ] {
        let mut draft = populated();
        draft.payment_terms.due_date = Some(date.into());
        let diagnostics = draft.field_diagnostics();
        assert_eq!(diagnostics.len(), 1, "{date}: {diagnostics:?}");
        assert_eq!(diagnostics[0].code, "API-BUILD-DATE");
        assert_eq!(diagnostics[0].path, "payment_terms.due_date");
        assert!(diagnostics[0].rule_ids.is_empty());
    }
    let mut draft = populated();
    draft.payment_terms.text = Some("Pay at your convenience\n".into());
    draft.payment_terms.due_date = Some("20200101".into()); // No invented date ordering invariant.
    assert!(draft.field_diagnostics().is_empty());
}

#[test]
fn unsupported_requests_are_explicit_feature_findings_and_preserve_input() {
    let mut draft = populated();
    draft.invoice_type = "381".into();
    draft.lines[0].vat.category = "AE".into();
    draft.payment.as_mut().unwrap().means = PaymentMeans::Unsupported("59".into());
    let original = draft.clone();
    let diagnostics = draft.field_diagnostics();
    assert_eq!(diagnostics.len(), 3);
    for (path, code) in [
        ("invoice_type", "API-BUILD-UNSUPPORTED-TYPE"),
        (
            "lines[0].vat.category",
            "API-BUILD-UNSUPPORTED-VAT-CATEGORY",
        ),
        ("payment.means", "API-BUILD-UNSUPPORTED-PAYMENT"),
    ] {
        let diagnostic = finding(&diagnostics, path);
        assert_eq!(diagnostic.code, code);
        assert!(diagnostic.rule_ids.is_empty());
    }
    assert_eq!(draft, original);
    assert!(matches!(
        Target::xrechnung_3_0_2().encode(&draft),
        Err(BuildError::Unavailable { .. })
    ));
    draft.payment.as_mut().unwrap().accounts[0].identifier = AccountIdentifier::Iban("".into());
    // Transfer-specific rules must not be attributed to an unsupported non-transfer code.
    assert!(
        finding(&draft.field_diagnostics(), "payment.accounts[0].identifier")
            .rule_ids
            .is_empty()
    );

    let mut draft = populated();
    let payment = draft.payment.as_mut().unwrap();
    payment.means = PaymentMeans::Unsupported("59".into());
    payment.accounts.clear();
    let diagnostics = draft.field_diagnostics();
    assert_eq!(
        diagnostics
            .iter()
            .map(|d| (d.code, d.path.as_str(), d.rule_ids.as_slice()))
            .collect::<Vec<_>>(),
        [("API-BUILD-UNSUPPORTED-PAYMENT", "payment.means", &[][..])]
    );
}

#[test]
fn xml_text_checks_cover_header_optional_numeric_and_unicode_boundaries() {
    for character in ['\u{0}', '\u{B}', '\u{1F}', '\u{FFFE}', '\u{FFFF}'] {
        let draft = InvoiceDraft::new(format!("INV{character}original"), "EUR");
        let diagnostics = draft.field_diagnostics();
        assert_eq!(
            finding(&diagnostics, "invoice_number").code,
            "API-BUILD-XML-CHAR"
        );
        assert!(finding(&diagnostics, "invoice_number").rule_ids.is_empty());
    }
    let mut draft = populated();
    draft.lines[0].price.amount = "original\u{B}amount".into();
    draft.expected.due = Some("expected\u{FFFE}amount".into());
    assert_eq!(
        finding(&draft.field_diagnostics(), "lines[0].price.amount").code,
        "API-BUILD-XML-CHAR"
    );
    assert_eq!(
        finding(&draft.field_diagnostics(), "expected.due").code,
        "API-BUILD-XML-CHAR"
    );
    let mut draft = populated();
    draft.lines[0].name = "\t\n\r Grüßé\u{85}\u{A0}\u{E000}\u{FFFD}🌱".into();
    assert!(draft.field_diagnostics().is_empty());
}

#[test]
fn field_diagnostics_do_not_promote_advisories_or_invent_code_lists() {
    let mut draft = populated();
    draft.lines[0].quantity.unit = "caller-unit".into();
    draft.lines[0].vat.rate = "123.456".into();
    let seller = draft.seller.as_mut().unwrap();
    seller.party.vat_identifier = Some("XIoriginal-vat".into());
    seller.party.contact.as_mut().unwrap().telephone = Some("1".into());
    seller.party.contact.as_mut().unwrap().email = Some("unverified-mail".into());
    seller.party.endpoint.as_mut().unwrap().scheme = "caller-scheme".into();
    seller.party.address.as_mut().unwrap().country = "caller-country".into();
    assert!(draft.field_diagnostics().is_empty());
    // Missing code-list/checksum/advisory checks are part of this API's documented limit.
}
