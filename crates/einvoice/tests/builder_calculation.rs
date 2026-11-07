use einvoice::{
    AccountIdentifier, BuildDiagnostic, BuildError, CalculatedInvoice, CalculationError,
    CalculationLimits, CalculationResource, Contact, ContactPoint, Document, ElectronicAddress,
    Identifier, InvoiceDraft, InvoiceLineDraft, LineVat, NetPrice, PartyDraft, PaymentInstructions,
    PaymentMeans, PostalAddress, PriceBasis, Quantity, SellerDraft, Target, TaxRepresentativeDraft,
    TransferAccount, VatGroupExpectation,
};

fn party(name: &str) -> PartyDraft {
    PartyDraft {
        address: Some(PostalAddress {
            country: "DE".into(),
            city: Some("Bonn".into()),
            postcode: Some("53111".into()),
            ..PostalAddress::default()
        }),
        endpoint: Some(ElectronicAddress::new("route@example.test", "EM")),
        ..PartyDraft::new(name)
    }
}
fn line(quantity: &str, price: &str, basis: Option<&str>, rate: &str) -> InvoiceLineDraft {
    InvoiceLineDraft {
        quantity: Quantity {
            value: quantity.into(),
            unit: "C62".into(),
        },
        price: NetPrice {
            amount: price.into(),
            basis: basis.map(|value| PriceBasis {
                value: value.into(),
                unit: None,
            }),
        },
        vat: LineVat {
            category: "S".into(),
            rate: rate.into(),
        },
        ..InvoiceLineDraft::new("original-id", "Original item")
    }
}
fn draft() -> InvoiceDraft {
    let mut draft = InvoiceDraft::new("ORIGINAL-CALC-001", "EUR");
    draft.issue_date = "20261106".into();
    draft.business_process = "caller-process".into();
    draft.references.buyer_reference = "caller-reference".into();
    let mut seller = party("Original Seller");
    seller.vat_identifier = Some("DE123456789".into());
    seller.contact = Some(Contact {
        point: Some(ContactPoint::Department("Billing".into())),
        telephone: Some("1234567".into()),
        email: Some("billing@example.test".into()),
    });
    draft.seller = Some(SellerDraft {
        party: seller,
        tax_registration: None,
    });
    draft.buyer = Some(party("Original Buyer"));
    draft.payment = Some(PaymentInstructions {
        accounts: vec![TransferAccount::new(AccountIdentifier::Proprietary(
            "original-account".into(),
        ))],
        ..PaymentInstructions::default()
    });
    draft.lines = vec![line("2", "12.345", None, "19")];
    draft
}
fn calculate(draft: &InvoiceDraft) -> Result<CalculatedInvoice, CalculationError> {
    draft.calculate(&Target::xrechnung_3_0_2())
}
fn limited(
    draft: &InvoiceDraft,
    limits: CalculationLimits,
) -> Result<CalculatedInvoice, CalculationError> {
    draft.calculate_with_limits(&Target::xrechnung_3_0_2(), limits)
}
fn fields(result: Result<CalculatedInvoice, CalculationError>) -> Vec<BuildDiagnostic> {
    match result {
        Err(CalculationError::Fields { diagnostics }) => diagnostics,
        other => panic!("expected fields: {other:?}"),
    }
}
fn finding<'a>(diagnostics: &'a [BuildDiagnostic], path: &str, code: &str) -> &'a BuildDiagnostic {
    diagnostics
        .iter()
        .find(|d| d.path == path && d.code == code)
        .unwrap_or_else(|| panic!("missing {code} at {path}: {diagnostics:?}"))
}
fn resource(
    result: Result<CalculatedInvoice, CalculationError>,
    expected: CalculationResource,
) -> String {
    match result {
        Err(CalculationError::Resource { path, cause }) => {
            assert_eq!(cause, expected);
            path
        }
        other => panic!("expected {expected:?}: {other:?}"),
    }
}

#[test]
fn original_a_through_j_have_independently_authored_exact_results() {
    let mut count = 0;
    for row in include_str!("fixtures/calculation/original.tsv")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let columns: Vec<_> = row.split('\t').collect();
        assert_eq!(columns.len(), 8);
        let mut input = draft();
        // Currency is deliberately constructor-only.
        let mut invoice = InvoiceDraft::new(columns[0], columns[1]);
        std::mem::swap(&mut invoice.seller, &mut input.seller);
        invoice.buyer = input.buyer;
        invoice.payment = input.payment;
        invoice.issue_date = input.issue_date;
        invoice.business_process = input.business_process;
        invoice.references = input.references;
        let lines: Vec<Vec<_>> = columns[2]
            .split(';')
            .map(|l| l.split(',').collect())
            .collect();
        for l in &lines {
            let mut item = line(l[0], l[1], Some(l[2]), l[3]);
            item.expected_net = Some(l[4].into());
            invoice.lines.push(item);
        }
        let groups: Vec<Vec<_>> = columns[3]
            .split(';')
            .map(|g| g.split(',').collect())
            .collect();
        invoice.expected.vat_groups = groups
            .iter()
            .map(|g| VatGroupExpectation {
                rate: g[0].into(),
                basis: Some(g[1].into()),
                tax: Some(g[2].into()),
            })
            .collect();
        invoice.expected.net = Some(columns[4].into());
        invoice.expected.tax = Some(columns[5].into());
        invoice.expected.gross = Some(columns[6].into());
        invoice.expected.due = Some(columns[7].into());
        let before = invoice.clone();
        let output = calculate(&invoice).unwrap();
        assert_eq!(invoice, before);
        assert_eq!(output.currency(), columns[1]);
        assert_eq!(
            output.lines().iter().map(|l| l.net()).collect::<Vec<_>>(),
            lines.iter().map(|l| l[4]).collect::<Vec<_>>(),
            "{}",
            columns[0]
        );
        assert_eq!(
            output
                .vat_groups()
                .iter()
                .map(|g| (g.rate(), g.basis(), g.tax()))
                .collect::<Vec<_>>(),
            groups
                .iter()
                .map(|g| (g[0], g[1], g[2]))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            (
                output.totals().net(),
                output.totals().tax(),
                output.totals().gross(),
                output.totals().due()
            ),
            (columns[4], columns[5], columns[6], columns[7])
        );
        count += 1;
    }
    assert_eq!(count, 10);
}

#[test]
fn canonical_values_preserve_exact_inputs_and_explicit_vs_absent_basis() {
    let mut input = draft();
    input.lines = vec![
        line(" -000.2500 ", "+001.2300", Some("002.00"), "019.000"),
        line("0", "0", None, "19.0"),
    ];
    let before = input.clone();
    let result = calculate(&input).unwrap();
    assert_eq!(input, before);
    let first = &result.lines()[0];
    assert_eq!(
        (
            first.quantity(),
            first.price(),
            first.price_basis(),
            first.vat_rate(),
            first.net()
        ),
        ("-0.25", "1.23", Some("2"), "19", "-0.15")
    );
    assert_eq!(result.lines()[1].price_basis(), None);
    assert_eq!(result.vat_groups().len(), 1);
}

#[test]
fn numeric_rate_sorting_and_grouping_are_independent_of_line_order() {
    let mut input = draft();
    input.lines = vec![
        line("1", "1", None, "100"),
        line("1", "2", None, "019.0"),
        line("1", "3", None, "2"),
        line("1", "4", None, "19.000"),
    ];
    let a = calculate(&input).unwrap();
    assert_eq!(
        a.vat_groups()
            .iter()
            .map(|g| (g.rate(), g.basis(), g.tax()))
            .collect::<Vec<_>>(),
        [
            ("2", "3.00", "0.06"),
            ("19", "6.00", "1.14"),
            ("100", "1.00", "1.00")
        ]
    );
    input.lines.reverse();
    let b = calculate(&input).unwrap();
    assert_eq!(a.vat_groups(), b.vat_groups());
    assert_eq!(a.totals(), b.totals());
    assert_eq!(
        b.lines().iter().map(|l| l.net()).collect::<Vec<_>>(),
        ["4.00", "3.00", "2.00", "1.00"]
    );
}

#[test]
fn signed_line_and_group_ties_round_away_from_zero() {
    for (quantity, net, tax, gross) in [
        ("1", "0.50", "0.10", "0.60"),
        ("-1", "-0.50", "-0.10", "-0.60"),
    ] {
        let mut input = draft();
        input.lines = vec![line(quantity, ".495", None, "19")];
        let result = calculate(&input).unwrap();
        assert_eq!(
            (
                result.totals().net(),
                result.totals().tax(),
                result.totals().gross()
            ),
            (net, tax, gross)
        );
    }
}

#[test]
fn direct_division_strictly_below_half_beyond_eighteen_digits() {
    let mut input = draft();
    input.lines = vec![line(
        "1",
        ".029999999999999999999999999999",
        Some("6"),
        "19",
    )];
    assert_eq!(calculate(&input).unwrap().lines()[0].net(), "0.00");
    input.lines[0].quantity.value = "-1".into();
    assert_eq!(calculate(&input).unwrap().lines()[0].net(), "0.00");
    input.lines[0].price.amount = ".03".into();
    assert_eq!(calculate(&input).unwrap().lines()[0].net(), "-0.01");
}

#[test]
fn absence_alone_defaults_basis_to_one() {
    let input = draft();
    assert_eq!(calculate(&input).unwrap().lines()[0].net(), "24.69");
    for (value, code) in [
        ("0", "API-BUILD-NUMERIC-SIGN"),
        ("-1", "API-BUILD-NUMERIC-SIGN"),
        ("malformed", "API-BUILD-DECIMAL"),
        ("", "API-BUILD-DECIMAL"),
    ] {
        let mut input = input.clone();
        input.lines[0].price.basis = Some(PriceBasis {
            value: value.into(),
            unit: None,
        });
        let before = input.clone();
        finding(
            &fields(calculate(&input)),
            "lines[0].price.basis.value",
            code,
        );
        assert_eq!(input, before);
    }
}

#[test]
fn sign_policy_rejects_negative_price_and_nonpositive_standard_rates() {
    for (price, rate, path) in [
        ("-1", "19", "lines[0].price.amount"),
        ("1", "0", "lines[0].vat.rate"),
        ("1", "-19", "lines[0].vat.rate"),
    ] {
        let mut input = draft();
        input.lines = vec![line("1", price, None, rate)];
        finding(&fields(calculate(&input)), path, "API-BUILD-NUMERIC-SIGN");
    }
    for quantity in ["0", "-1"] {
        let mut input = draft();
        input.lines = vec![line(quantity, "0", Some(".25"), "123.456")];
        assert_eq!(calculate(&input).unwrap().totals().gross(), "0.00");
    }
}

#[test]
fn explicit_basis_unit_must_match_quantity_unit() {
    let mut input = draft();
    input.lines[0].price.basis = Some(PriceBasis {
        value: "1".into(),
        unit: Some("HUR".into()),
    });
    let diagnostics = fields(calculate(&input));
    assert_eq!(
        finding(
            &diagnostics,
            "lines[0].price.basis.unit",
            "API-BUILD-BASIS-UNIT"
        )
        .rule_ids,
        ["PEPPOL-EN16931-R130"]
    );
    input.lines[0].price.basis.as_mut().unwrap().unit = Some("C62".into());
    calculate(&input).unwrap();
}

#[test]
fn standard_rate_rule_provenance_requires_standard_category() {
    for category in ["AE", "Z", "E", "O", "", " S ", "s"] {
        for rate in ["0", "-19", "19", "bad"] {
            let mut input = draft();
            input.lines[0].vat.category = category.into();
            input.lines[0].vat.rate = rate.into();
            let before = input.clone();
            let diagnostics = fields(calculate(&input));
            finding(
                &diagnostics,
                "lines[0].vat.category",
                if category.is_empty() {
                    "API-BUILD-REQUIRED"
                } else {
                    "API-BUILD-UNSUPPORTED-VAT-CATEGORY"
                },
            );
            assert!(diagnostics.iter().all(|d| !d.rule_ids.contains(&"BR-S-05")));
            assert!(diagnostics
                .iter()
                .all(|d| d.code != "API-BUILD-NUMERIC-SIGN"));
            if rate == "bad" {
                finding(&diagnostics, "lines[0].vat.rate", "API-BUILD-DECIMAL");
            }
            assert_eq!(input, before);
        }
    }
    for rate in ["0", "-19", "-0.00"] {
        let mut input = draft();
        input.lines[0].vat.rate = rate.into();
        assert_eq!(
            finding(
                &fields(calculate(&input)),
                "lines[0].vat.rate",
                "API-BUILD-NUMERIC-SIGN"
            )
            .rule_ids,
            ["BR-S-05"]
        );
    }
    let mut input = draft();
    input.lines[0].vat.category = "AE".into();
    input.lines[0].vat.rate = "1".repeat(1025);
    assert_eq!(
        resource(calculate(&input), CalculationResource::NumericInputBytes),
        "lines[0].vat.rate"
    );
}

#[test]
fn later_resource_refusal_takes_precedence_over_retained_amount_mismatch() {
    let mut input = draft();
    input.lines = vec![line("1", "1", None, "19")];
    input.lines[0].expected_net = Some("0".into());
    let limits = CalculationLimits {
        coefficient_bits: 24,
        ..CalculationLimits::default()
    };
    let diagnostics = fields(limited(&input, limits));
    assert_eq!(diagnostics.len(), 1);
    finding(
        &diagnostics,
        "lines[0].expected_net",
        "API-BUILD-EXPECTED-AMOUNT",
    );
    input.lines.push(line("9999", "9999", None, "19"));
    let before = input.clone();
    assert_eq!(
        resource(
            limited(&input, limits),
            CalculationResource::CoefficientBits
        ),
        "lines[1]"
    );
    assert_eq!(input, before);
}

#[test]
fn diagnostic_count_cap_is_shared_by_business_and_numeric_checks() {
    let mut input = draft();
    input.references.buyer_reference.clear();
    input.lines[0].quantity.value = "bad".into();
    let before = input.clone();
    let limits = CalculationLimits {
        max_diagnostics: 1,
        ..CalculationLimits::default()
    };
    assert_eq!(
        resource(limited(&input, limits), CalculationResource::Diagnostics),
        "lines[0].quantity.value"
    );
    let diagnostics = fields(limited(
        &input,
        CalculationLimits {
            max_diagnostics: 2,
            ..limits
        },
    ));
    assert_eq!(diagnostics.len(), 2);
    finding(
        &diagnostics,
        "references.buyer_reference",
        "API-BUILD-REQUIRED",
    );
    finding(&diagnostics, "lines[0].quantity.value", "API-BUILD-DECIMAL");
    assert_eq!(input, before);
}

#[test]
fn every_numeric_input_and_expectation_has_lexical_diagnostics() {
    let mut input = draft();
    input.lines = vec![line("NaN", "1e2", Some("--1"), "19%")];
    input.lines[0].expected_net = Some("1,00".into());
    input.expected.vat_groups.push(VatGroupExpectation {
        rate: "unknown".into(),
        basis: Some("bad".into()),
        tax: Some("bad".into()),
    });
    input.expected.net = Some("bad".into());
    input.expected.tax = Some("bad".into());
    input.expected.gross = Some("bad".into());
    input.expected.due = Some("bad".into());
    let diagnostics = fields(calculate(&input));
    assert_eq!(diagnostics.len(), 12);
    for path in [
        "lines[0].quantity.value",
        "lines[0].price.amount",
        "lines[0].price.basis.value",
        "lines[0].vat.rate",
        "lines[0].expected_net",
        "expected.vat_groups[0].rate",
        "expected.vat_groups[0].basis",
        "expected.vat_groups[0].tax",
        "expected.net",
        "expected.tax",
        "expected.gross",
        "expected.due",
    ] {
        finding(&diagnostics, path, "API-BUILD-DECIMAL");
    }
}

#[test]
fn collects_exact_disagreements_at_line_group_and_all_total_fields() {
    let mut input = draft();
    input.lines[0].expected_net = Some("24.694".into());
    input.expected.vat_groups = vec![VatGroupExpectation {
        rate: "019.000".into(),
        basis: Some("24.694".into()),
        tax: Some("4.694".into()),
    }];
    input.expected.net = Some("24.694".into());
    input.expected.tax = Some("4.694".into());
    input.expected.gross = Some("29.384".into());
    input.expected.due = Some("29.384".into());
    let before = input.clone();
    let diagnostics = fields(calculate(&input));
    assert_eq!(diagnostics.len(), 7);
    for (path, supplied, computed) in [
        ("lines[0].expected_net", "24.694", "24.69"),
        ("expected.vat_groups[0].basis", "24.694", "24.69"),
        ("expected.vat_groups[0].tax", "4.694", "4.69"),
        ("expected.net", "24.694", "24.69"),
        ("expected.tax", "4.694", "4.69"),
        ("expected.gross", "29.384", "29.38"),
        ("expected.due", "29.384", "29.38"),
    ] {
        assert_eq!(
            finding(&diagnostics, path, "API-BUILD-EXPECTED-AMOUNT").message,
            format!("Supplied {supplied}; computed {computed} EUR")
        );
    }
    assert_eq!(input, before);
}

#[test]
fn trailing_zero_expectations_are_equal_without_rounding_or_tolerance() {
    let mut input = draft();
    input.lines[0].expected_net = Some("+024.69000".into());
    input.expected.vat_groups = vec![VatGroupExpectation {
        rate: "19.00".into(),
        basis: Some("24.6900".into()),
        tax: Some("4.69000".into()),
    }];
    input.expected.net = Some("24.6900".into());
    input.expected.tax = Some("4.6900".into());
    input.expected.gross = Some("29.380000".into());
    input.expected.due = Some("29.380".into());
    calculate(&input).unwrap();
    for value in ["29.384", "29.39", "29.37999999999999999999999999999999"] {
        input.expected.gross = Some(value.into());
        finding(
            &fields(calculate(&input)),
            "expected.gross",
            "API-BUILD-EXPECTED-AMOUNT",
        );
    }
}

#[test]
fn duplicate_and_absent_expected_groups_are_never_ignored_even_without_amounts() {
    let mut input = draft();
    input.expected.vat_groups = ["19", "019.00", "7", "7.0"]
        .into_iter()
        .map(|rate| VatGroupExpectation {
            rate: rate.into(),
            ..VatGroupExpectation::default()
        })
        .collect();
    let diagnostics = fields(calculate(&input));
    assert_eq!(diagnostics.len(), 4);
    finding(
        &diagnostics,
        "expected.vat_groups[1].rate",
        "API-BUILD-DUPLICATE-GROUP",
    );
    finding(
        &diagnostics,
        "expected.vat_groups[2].rate",
        "API-BUILD-UNKNOWN-GROUP",
    );
    finding(
        &diagnostics,
        "expected.vat_groups[3].rate",
        "API-BUILD-DUPLICATE-GROUP",
    );
    finding(
        &diagnostics,
        "expected.vat_groups[3].rate",
        "API-BUILD-UNKNOWN-GROUP",
    );
}

#[test]
fn zero_group_basis_is_present_and_distinct_from_an_absent_group() {
    let mut input = draft();
    input.lines = vec![line("0", "1", None, "19")];
    input.expected.vat_groups = vec![VatGroupExpectation {
        rate: "19".into(),
        basis: Some("0".into()),
        tax: None,
    }];
    assert_eq!(calculate(&input).unwrap().vat_groups()[0].basis(), "0.00");
    input.expected.vat_groups[0].rate = "7".into();
    finding(
        &fields(calculate(&input)),
        "expected.vat_groups[0].rate",
        "API-BUILD-UNKNOWN-GROUP",
    );
}

#[test]
fn field_failures_remain_bounded_build_findings_and_encode_stays_unavailable() {
    let mut input = draft();
    input.references.buyer_reference.clear();
    input.invoice_type = "381".into();
    let diagnostics = fields(calculate(&input));
    assert_eq!(diagnostics.len(), 2);
    finding(
        &diagnostics,
        "references.buyer_reference",
        "API-BUILD-REQUIRED",
    );
    finding(&diagnostics, "invoice_type", "API-BUILD-UNSUPPORTED-TYPE");
    assert!(matches!(
        Target::xrechnung_3_0_2().encode(&draft()),
        Err(BuildError::Unavailable { .. })
    ));
    assert!(matches!(
        Target::xrechnung_3_0_2().encode(&input),
        Err(BuildError::Unavailable { .. })
    ));
}

#[test]
fn unsupported_target_precedes_draft_and_invalid_limits_without_copying_its_id() {
    let input = InvoiceDraft::new("\0".repeat(100_000), "");
    let target = Target::named("unknown".repeat(100_000));
    let limits = CalculationLimits {
        max_lines: 0,
        ..CalculationLimits::default()
    };
    assert_eq!(
        input.calculate_with_limits(&target, limits),
        Err(CalculationError::UnsupportedTarget)
    );
}

#[test]
fn imports_remain_byte_identical_through_success_and_failure() {
    let bytes = b"<?xml version='1.0'?><original x='kept'><!-- keep -->001.00</original>\r\n";
    let document = Document::parse(bytes).unwrap();
    let input = draft();
    let before = input.clone();
    calculate(&input).unwrap();
    limited(
        &input,
        CalculationLimits {
            cumulative_work: 1,
            ..CalculationLimits::default()
        },
    )
    .unwrap_err();
    assert_eq!(input, before);
    assert_eq!(document.original_bytes(), bytes);
    assert_eq!(document.to_bytes(), bytes);
}

#[test]
fn collection_limits_precede_field_scans() {
    let mut input = draft();
    input.lines.push(input.lines[0].clone());
    input.lines[0].name = "\0".repeat(100_000);
    assert_eq!(
        resource(
            limited(
                &input,
                CalculationLimits {
                    max_lines: 1,
                    ..CalculationLimits::default()
                }
            ),
            CalculationResource::Lines
        ),
        "lines"
    );
    input.lines.truncate(1);
    input
        .payment
        .as_mut()
        .unwrap()
        .accounts
        .push(TransferAccount::new(AccountIdentifier::Iban("".into())));
    assert_eq!(
        resource(
            limited(
                &input,
                CalculationLimits {
                    max_accounts: 1,
                    ..CalculationLimits::default()
                }
            ),
            CalculationResource::Accounts
        ),
        "payment.accounts"
    );
    input.payment.as_mut().unwrap().accounts.truncate(1);
    input.expected.vat_groups = vec![VatGroupExpectation::default(); 2];
    assert_eq!(
        resource(
            limited(
                &input,
                CalculationLimits {
                    max_expected_groups: 1,
                    ..CalculationLimits::default()
                }
            ),
            CalculationResource::ExpectedGroups
        ),
        "expected.vat_groups"
    );
}

#[test]
fn input_string_and_aggregate_limits_precede_diagnostic_scans() {
    let input = InvoiceDraft::new("\0".repeat(100), "");
    assert_eq!(
        resource(
            limited(
                &input,
                CalculationLimits {
                    max_string_bytes: 32,
                    ..CalculationLimits::default()
                }
            ),
            CalculationResource::StringBytes
        ),
        "input"
    );
    let mut input = draft();
    input.lines[0].name = "\0".repeat(100);
    resource(
        limited(
            &input,
            CalculationLimits {
                max_input_bytes: 100,
                ..CalculationLimits::default()
            },
        ),
        CalculationResource::InputBytes,
    );
}

#[test]
fn preflight_includes_optional_and_nested_business_text() {
    type Change = fn(&mut InvoiceDraft, String);
    let changes: [Change; 18] = [
        |d, s| d.references.contract = Some(s),
        |d, s| d.payment_terms.due_date = Some(s),
        |d, s| d.payment_terms.text = Some(s),
        |d, s| d.seller.as_mut().unwrap().tax_registration = Some(s),
        |d, s| {
            d.seller
                .as_mut()
                .unwrap()
                .party
                .contact
                .as_mut()
                .unwrap()
                .point = Some(ContactPoint::Person(s))
        },
        |d, s| {
            d.buyer
                .as_mut()
                .unwrap()
                .address
                .as_mut()
                .unwrap()
                .country_subdivision = Some(s)
        },
        |d, s| d.buyer.as_mut().unwrap().endpoint.as_mut().unwrap().scheme = s,
        |d, s| {
            d.buyer.as_mut().unwrap().identifier = Some(Identifier {
                value: "id".into(),
                scheme: Some(s),
            })
        },
        |d, s| d.buyer.as_mut().unwrap().legal_registration = Some(Identifier::new(s)),
        |d, s| d.buyer.as_mut().unwrap().vat_identifier = Some(s),
        |d, s| {
            d.tax_representative = Some(TaxRepresentativeDraft {
                name: s,
                ..TaxRepresentativeDraft::default()
            })
        },
        |d, s| d.payment.as_mut().unwrap().means = PaymentMeans::Unsupported(s),
        |d, s| d.payment.as_mut().unwrap().accounts[0].identifier = AccountIdentifier::Iban(s),
        |d, s| d.payment.as_mut().unwrap().accounts[0].bic = Some(s),
        |d, s| d.payment.as_mut().unwrap().description = Some(s),
        |d, s| {
            d.lines[0].price.basis = Some(PriceBasis {
                value: "1".into(),
                unit: Some(s),
            })
        },
        |d, s| {
            d.expected.vat_groups = vec![VatGroupExpectation {
                rate: "19".into(),
                tax: Some(s),
                basis: None,
            }]
        },
        |d, s| d.expected.due = Some(s),
    ];
    for change in changes {
        let mut input = draft();
        change(&mut input, "\0".repeat(129));
        resource(
            limited(
                &input,
                CalculationLimits {
                    max_string_bytes: 128,
                    ..CalculationLimits::default()
                },
            ),
            CalculationResource::StringBytes,
        );
    }
}

#[test]
fn invalid_and_overflowing_limit_configurations_are_typed() {
    for limits in [
        CalculationLimits {
            max_lines: 0,
            ..CalculationLimits::default()
        },
        CalculationLimits {
            max_expected_groups: usize::MAX,
            ..CalculationLimits::default()
        },
        CalculationLimits {
            max_output_bytes: usize::MAX,
            ..CalculationLimits::default()
        },
        CalculationLimits {
            coefficient_bits: u64::MAX,
            ..CalculationLimits::default()
        },
        CalculationLimits {
            absolute_scale: u64::MAX,
            ..CalculationLimits::default()
        },
        CalculationLimits {
            numeric_input_bytes: 0,
            ..CalculationLimits::default()
        },
        CalculationLimits {
            numeric_output_bytes: 0,
            ..CalculationLimits::default()
        },
        CalculationLimits {
            cumulative_work: 0,
            ..CalculationLimits::default()
        },
    ] {
        assert_eq!(
            resource(
                limited(&draft(), limits),
                CalculationResource::InvalidLimits
            ),
            "limits"
        );
    }
}

#[test]
fn numeric_lexical_bit_scale_work_and_render_refusals_are_processing_errors() {
    let mut input = draft();
    input.lines[0].price.amount = "123456789".into();
    assert_eq!(
        resource(
            limited(
                &input,
                CalculationLimits {
                    numeric_input_bytes: 8,
                    ..CalculationLimits::default()
                }
            ),
            CalculationResource::NumericInputBytes
        ),
        "lines[0].price.amount"
    );
    input.lines[0].price.amount = "12345".into();
    resource(
        limited(
            &input,
            CalculationLimits {
                coefficient_bits: 16,
                ..CalculationLimits::default()
            },
        ),
        CalculationResource::CoefficientBits,
    );
    input.lines[0].price.amount = ".00001".into();
    assert_eq!(
        resource(
            limited(
                &input,
                CalculationLimits {
                    absolute_scale: 4,
                    ..CalculationLimits::default()
                }
            ),
            CalculationResource::Scale
        ),
        "lines[0].price.amount"
    );
    resource(
        limited(
            &draft(),
            CalculationLimits {
                cumulative_work: 1,
                ..CalculationLimits::default()
            },
        ),
        CalculationResource::Work,
    );
    resource(
        limited(
            &draft(),
            CalculationLimits {
                numeric_output_bytes: 3,
                ..CalculationLimits::default()
            },
        ),
        CalculationResource::NumericOutputBytes,
    );
}

#[test]
fn coefficient_and_scale_bounds_also_cover_intermediates() {
    let mut input = draft();
    input.lines = vec![line("9999", "9999", None, "19")];
    assert_eq!(
        resource(
            limited(
                &input,
                CalculationLimits {
                    coefficient_bits: 24,
                    ..CalculationLimits::default()
                }
            ),
            CalculationResource::CoefficientBits
        ),
        "lines[0]"
    );
    input.lines = vec![line(".001", ".001", None, "19")];
    assert_eq!(
        resource(
            limited(
                &input,
                CalculationLimits {
                    absolute_scale: 4,
                    ..CalculationLimits::default()
                }
            ),
            CalculationResource::Scale
        ),
        "lines[0]"
    );
}

#[test]
fn diagnostic_count_and_bytes_are_bounded_before_retention() {
    let input = InvoiceDraft::new("", "");
    resource(
        limited(
            &input,
            CalculationLimits {
                max_diagnostics: 1,
                ..CalculationLimits::default()
            },
        ),
        CalculationResource::Diagnostics,
    );
    resource(
        limited(
            &input,
            CalculationLimits {
                max_output_bytes: 1,
                ..CalculationLimits::default()
            },
        ),
        CalculationResource::OutputBytes,
    );
    let mut input = draft();
    input.expected.net = Some("9".repeat(1000));
    resource(
        limited(
            &input,
            CalculationLimits {
                max_output_bytes: 1100,
                ..CalculationLimits::default()
            },
        ),
        CalculationResource::OutputBytes,
    );
}

#[test]
fn retained_result_output_is_bounded_and_can_be_raised_explicitly() {
    let limits = CalculationLimits {
        max_output_bytes: 1024,
        ..CalculationLimits::default()
    };
    resource(limited(&draft(), limits), CalculationResource::OutputBytes);
    let result = limited(
        &draft(),
        CalculationLimits {
            max_output_bytes: 4096,
            ..limits
        },
    )
    .unwrap();
    assert_eq!(result.totals().gross(), "29.38");
}

// Determine affordable fuel without coupling tests to kernel accounting constants.
fn minimum_work(input: &InvoiceDraft) -> u64 {
    let (mut low, mut high) = (1, CalculationLimits::default().cumulative_work);
    while low < high {
        let middle = low + (high - low) / 2;
        match limited(
            input,
            CalculationLimits {
                cumulative_work: middle,
                ..CalculationLimits::default()
            },
        ) {
            Ok(_) => high = middle,
            Err(CalculationError::Resource {
                cause: CalculationResource::Work,
                ..
            }) => low = middle + 1,
            other => panic!("fuel probe: {other:?}"),
        }
    }
    low
}

#[test]
fn one_invoice_budget_is_cumulative_across_individually_affordable_lines() {
    let mut one = draft();
    one.lines = vec![line("1", ".335", None, "19")];
    let work = minimum_work(&one);
    limited(
        &one,
        CalculationLimits {
            cumulative_work: work,
            ..CalculationLimits::default()
        },
    )
    .unwrap();
    let mut many = one.clone();
    many.lines = vec![one.lines[0].clone(); 20];
    resource(
        limited(
            &many,
            CalculationLimits {
                cumulative_work: work,
                ..CalculationLimits::default()
            },
        ),
        CalculationResource::Work,
    );
    assert_eq!(calculate(&many).unwrap().totals().net(), "6.80");
}

#[test]
fn expectation_parses_and_exact_comparisons_share_cumulative_fuel() {
    let base = draft();
    let cases = [
        ("24.69", "net"),
        ("4.69", "tax"),
        ("29.38", "gross"),
        ("29.38", "due"),
    ];
    let mut affordable = 0;
    let mut individual = Vec::new();
    for (value, name) in cases {
        let mut input = base.clone();
        match name {
            "net" => input.expected.net = Some(value.into()),
            "tax" => input.expected.tax = Some(value.into()),
            "gross" => input.expected.gross = Some(value.into()),
            _ => input.expected.due = Some(value.into()),
        }
        affordable = affordable.max(minimum_work(&input));
        individual.push(input);
    }
    for input in &individual {
        limited(
            input,
            CalculationLimits {
                cumulative_work: affordable,
                ..CalculationLimits::default()
            },
        )
        .unwrap();
    }
    let mut all = base;
    all.expected.net = Some("24.69".into());
    all.expected.tax = Some("4.69".into());
    all.expected.gross = Some("29.38".into());
    all.expected.due = Some("29.38".into());
    resource(
        limited(
            &all,
            CalculationLimits {
                cumulative_work: affordable,
                ..CalculationLimits::default()
            },
        ),
        CalculationResource::Work,
    );
    calculate(&all).unwrap();
}

#[test]
fn defaults_handle_the_documented_collection_caps_for_small_values() {
    let mut input = draft();
    input.lines = (1..=1000)
        .map(|rate| line("1", "1", None, &rate.to_string()))
        .collect();
    input.expected.vat_groups = (1..=1000)
        .map(|rate| VatGroupExpectation {
            rate: rate.to_string(),
            ..VatGroupExpectation::default()
        })
        .collect();
    let account = input.payment.as_ref().unwrap().accounts[0].clone();
    input.payment.as_mut().unwrap().accounts = vec![account; 32];
    let result = calculate(&input).unwrap();
    assert_eq!(result.lines().len(), 1000);
    assert_eq!(result.vat_groups().len(), 1000);
    // Sum 1..1000 = 500500; exact tax is 500500 / 100.
    assert_eq!(
        (
            result.totals().net(),
            result.totals().tax(),
            result.totals().gross()
        ),
        ("1000.00", "5005.00", "6005.00")
    );
}

#[test]
fn validator_tax_tolerance_is_never_used_for_an_expected_amount() {
    let mut input = draft();
    input.expected.vat_groups.push(VatGroupExpectation {
        rate: "19".into(),
        basis: None,
        tax: Some("5.68".into()),
    });
    assert_eq!(
        finding(
            &fields(calculate(&input)),
            "expected.vat_groups[0].tax",
            "API-BUILD-EXPECTED-AMOUNT"
        )
        .message,
        "Supplied 5.68; computed 4.69 EUR"
    );
}
