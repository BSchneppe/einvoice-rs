use super::*;
use crate::{
    BuildError, CalculationResource, Document, ElectronicAddress, InvoiceLineDraft, LineVat,
    NetPrice, PaymentInstructions, PriceBasis, Quantity, SellerDraft, TaxRepresentativeDraft,
    TransferAccount, VatGroupExpectation,
};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use xml::reader::{ParserConfig, XmlEvent};

const RSM: &str = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100";
const RAM: &str =
    "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100";
const UDT: &str = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100";
const SPECIAL: &str = "A&<tag>]]>\"'\rB\nC\r\nD\tE Grüße 東京 😀 e\u{301}";
const PROCESS: &str = "urn:fdc:peppol.eu:2017:poacc:billing:01:1.0";

fn party_input(name: &str) -> PartyDraft {
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
fn line(q: &str, price: &str, basis: Option<&str>, rate: &str) -> InvoiceLineDraft {
    InvoiceLineDraft {
        quantity: Quantity {
            value: q.into(),
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
        ..InvoiceLineDraft::new("line-1", "Original item")
    }
}
fn draft(number: &str, currency: &str) -> InvoiceDraft {
    let mut d = InvoiceDraft::new(number, currency);
    d.issue_date = "20261106".into();
    d.business_process = PROCESS.into();
    d.references.buyer_reference = "buyer-reference".into();
    let mut p = party_input("Original Seller");
    p.vat_identifier = Some("DE123456789".into());
    p.contact = Some(Contact {
        point: Some(ContactPoint::Department("Billing".into())),
        telephone: Some("+49 228 1234567".into()),
        email: Some("billing@example.test".into()),
    });
    d.seller = Some(SellerDraft {
        party: p,
        tax_registration: None,
    });
    d.buyer = Some(party_input("Original Buyer"));
    d.payment = Some(PaymentInstructions {
        accounts: vec![TransferAccount::new(AccountIdentifier::Proprietary(
            "original-account".into(),
        ))],
        ..PaymentInstructions::default()
    });
    d.lines = vec![line("2", "12.345", None, "19")];
    d
}
fn bytes(d: &InvoiceDraft) -> Vec<u8> {
    prepare(d, &Target::xrechnung_3_0_2(), PreparationLimits::default()).unwrap()
}
fn limited(d: &InvoiceDraft, limits: PreparationLimits) -> Result<Vec<u8>> {
    prepare(d, &Target::xrechnung_3_0_2(), limits)
}

#[derive(Debug)]
struct Node {
    name: String,
    attributes: BTreeMap<String, String>,
    text: String,
    children: Vec<Node>,
}
fn expanded(name: &str) -> String {
    let (ns, local) = match name.split_once(':') {
        Some(("rsm", local)) => (RSM, local),
        Some(("ram", local)) => (RAM, local),
        Some(("udt", local)) => (UDT, local),
        Some(_) => panic!("unexpected test prefix: {name}"),
        None => (RAM, name),
    };
    format!("{{{ns}}}{local}")
}
fn parsed(bytes: &[u8]) -> Node {
    let mut stack: Vec<Node> = Vec::new();
    let reader = ParserConfig::new()
        .trim_whitespace(false)
        .whitespace_to_characters(true)
        .create_reader(bytes);
    for event in reader {
        match event.unwrap() {
            XmlEvent::StartElement {
                name, attributes, ..
            } => {
                stack.push(Node {
                    name: format!("{{{}}}{}", name.namespace.unwrap(), name.local_name),
                    attributes: attributes
                        .into_iter()
                        .map(|a| {
                            assert!(a.name.namespace.is_none());
                            (a.name.local_name, a.value)
                        })
                        .collect(),
                    text: String::new(),
                    children: Vec::new(),
                });
            }
            XmlEvent::Characters(value) => stack.last_mut().unwrap().text.push_str(&value),
            XmlEvent::EndElement { .. } => {
                let node = stack.pop().unwrap();
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                } else {
                    assert_eq!(node.name, expanded("rsm:CrossIndustryInvoice"));
                    return node;
                }
            }
            XmlEvent::StartDocument { .. } | XmlEvent::EndDocument => {}
            other => panic!("unexpected XML event: {other:?}"),
        }
    }
    panic!("no root")
}
fn at<'a>(root: &'a Node, path: &str) -> &'a Node {
    let prefix = match path.split('/').next().unwrap() {
        "C" => "rsm:ExchangedDocumentContext",
        "D" => "rsm:ExchangedDocument",
        "L" => "rsm:SupplyChainTradeTransaction/IncludedSupplyChainTradeLineItem",
        "A" => "rsm:SupplyChainTradeTransaction/ApplicableHeaderTradeAgreement",
        "H" => "rsm:SupplyChainTradeTransaction/ApplicableHeaderTradeDelivery",
        "S" => "rsm:SupplyChainTradeTransaction/ApplicableHeaderTradeSettlement",
        _ => "",
    };
    let path = if prefix.is_empty() {
        path.to_owned()
    } else {
        format!("{prefix}{}", &path[path.find('/').unwrap_or(path.len())..])
    };
    let mut node = root;
    for segment in path.split('/').filter(|p| !p.is_empty()) {
        let (name, index) = if let Some((name, index)) = segment.split_once('[') {
            (
                name,
                index.trim_end_matches(']').parse::<usize>().unwrap() - 1,
            )
        } else {
            (segment, 0)
        };
        node = node
            .children
            .iter()
            .filter(|n| n.name == expanded(name))
            .nth(index)
            .unwrap_or_else(|| panic!("missing {segment} in {path}"));
    }
    node
}
fn absent(root: &Node, parent: &str, name: &str) {
    assert!(
        at(root, parent)
            .children
            .iter()
            .all(|n| n.name != expanded(name)),
        "{parent}/{name}"
    );
}
fn order(root: &Node, path: &str, names: &[&str]) {
    assert_eq!(
        at(root, path)
            .children
            .iter()
            .map(|n| n.name.clone())
            .collect::<Vec<_>>(),
        names.iter().map(|n| expanded(n)).collect::<Vec<_>>(),
        "{path}"
    );
}
fn leaves<'a>(node: &'a Node, output: &mut Vec<&'a Node>) {
    if node.children.is_empty() {
        output.push(node);
    }
    for child in &node.children {
        leaves(child, output);
    }
}
fn attribute_count(node: &Node) -> usize {
    node.attributes.len() + node.children.iter().map(attribute_count).sum::<usize>()
}

fn original_cases() -> Vec<(Vec<&'static str>, InvoiceDraft)> {
    include_str!("../fixtures/calculation/original.tsv")
        .lines()
        .filter(|s| !s.starts_with('#'))
        .map(|row| {
            let columns: Vec<_> = row.split('\t').collect();
            assert_eq!(columns.len(), 8);
            let mut d = draft(columns[0], columns[1]);
            d.lines = columns[2]
                .split(';')
                .enumerate()
                .map(|(i, l)| {
                    let v: Vec<_> = l.split(',').collect();
                    let mut l = line(v[0], v[1], Some(v[2]), v[3]);
                    l.id = format!("line-{}", i + 1);
                    l.expected_net = Some(v[4].into());
                    l
                })
                .collect();
            d.expected.vat_groups = columns[3]
                .split(';')
                .map(|g| {
                    let v: Vec<_> = g.split(',').collect();
                    VatGroupExpectation {
                        rate: v[0].into(),
                        basis: Some(v[1].into()),
                        tax: Some(v[2].into()),
                    }
                })
                .collect();
            d.expected.net = Some(columns[4].into());
            d.expected.tax = Some(columns[5].into());
            d.expected.gross = Some(columns[6].into());
            d.expected.due = Some(columns[7].into());
            (columns, d)
        })
        .collect()
}
fn assert_original(columns: &[&str], b: &[u8]) {
    let root = parsed(b);
    for (i, l) in columns[2].split(';').enumerate() {
        let v: Vec<_> = l.split(',').collect();
        assert_eq!(at(&root, &format!("rsm:SupplyChainTradeTransaction/IncludedSupplyChainTradeLineItem[{}]/SpecifiedLineTradeSettlement/SpecifiedTradeSettlementLineMonetarySummation/LineTotalAmount", i+1)).text, v[4]);
    }
    let s = at(&root, "S");
    let groups: Vec<_> = s
        .children
        .iter()
        .filter(|n| n.name == expanded("ApplicableTradeTax"))
        .collect();
    let expected: Vec<_> = columns[3]
        .split(';')
        .map(|g| g.split(',').collect::<Vec<_>>())
        .collect();
    assert_eq!(groups.len(), expected.len());
    for (g, expected) in groups.iter().zip(expected) {
        assert_eq!(at(g, "RateApplicablePercent").text, expected[0]);
        assert_eq!(at(g, "BasisAmount").text, expected[1]);
        assert_eq!(at(g, "CalculatedAmount").text, expected[2]);
    }
    for (name, expected) in [
        ("LineTotalAmount", columns[4]),
        ("TaxBasisTotalAmount", columns[4]),
        ("TaxTotalAmount", columns[5]),
        ("GrandTotalAmount", columns[6]),
        ("DuePayableAmount", columns[7]),
    ] {
        let node = at(
            s,
            &format!("SpecifiedTradeSettlementHeaderMonetarySummation/{name}"),
        );
        assert_eq!(node.text, expected, "{} {name}", columns[0]);
        if name == "TaxTotalAmount" {
            assert_eq!(node.attributes["currencyID"], columns[1]);
        } else {
            assert!(node.attributes.is_empty());
        }
    }
    assert_eq!(at(s, "InvoiceCurrencyCode").text, columns[1]);
}

fn full_address(label: &str) -> PostalAddress {
    PostalAddress {
        country: "DE".into(),
        city: Some(format!("{label} city")),
        postcode: Some(format!("{label} postcode")),
        line_one: Some(format!("{label} street")),
        line_two: Some(format!("{label} building")),
        line_three: Some(format!("{label} floor")),
        country_subdivision: Some(format!("{label} region")),
    }
}
fn all_fields() -> InvoiceDraft {
    let mut d = draft("all-fields", "EUR");
    let seller = d.seller.as_mut().unwrap();
    seller.party.name = "Seller name".into();
    seller.party.identifier = Some(Identifier {
        value: "seller-global".into(),
        scheme: Some("0088".into()),
    });
    seller.party.legal_registration = Some(Identifier {
        value: "seller-legal".into(),
        scheme: Some("0002".into()),
    });
    seller.party.address = Some(full_address("Seller"));
    seller.party.endpoint = Some(ElectronicAddress::new("seller@example.test", "EM"));
    seller.party.contact = Some(Contact {
        point: Some(ContactPoint::Person("Seller person".into())),
        telephone: Some("+49 228 1111111".into()),
        email: Some("seller-contact@example.test".into()),
    });
    seller.tax_registration = Some("seller-fiscal".into());
    let buyer = d.buyer.as_mut().unwrap();
    buyer.name = "Buyer name".into();
    buyer.identifier = Some(Identifier::new("buyer-local"));
    buyer.legal_registration = Some(Identifier::new("buyer-legal"));
    buyer.vat_identifier = Some("DE987654321".into());
    buyer.address = Some(full_address("Buyer"));
    buyer.endpoint = Some(ElectronicAddress::new("buyer-route", "0204"));
    buyer.contact = Some(Contact {
        point: Some(ContactPoint::Department("Buyer department".into())),
        telephone: Some("+49 228 2222222".into()),
        email: Some("buyer-contact@example.test".into()),
    });
    d.tax_representative = Some(TaxRepresentativeDraft {
        name: "Representative name".into(),
        address: Some(full_address("Representative")),
        vat_identifier: "DE111111111".into(),
    });
    d.references.purchase_order = Some("purchase-order".into());
    d.references.seller_order = Some("seller-order".into());
    d.references.contract = Some("contract-reference".into());
    d.payment = Some(PaymentInstructions {
        means: PaymentMeans::CreditTransfer,
        description: Some("Transfer description".into()),
        reference: Some("remittance-reference".into()),
        accounts: vec![
            TransferAccount {
                identifier: AccountIdentifier::Iban("DE89370400440532013000".into()),
                name: Some("IBAN holder".into()),
                bic: Some("COBADEFFXXX".into()),
            },
            TransferAccount {
                identifier: AccountIdentifier::Proprietary("local-account".into()),
                name: Some("Local holder".into()),
                bic: Some("MARKDEF1100".into()),
            },
        ],
    });
    d.payment_terms.text = Some("Pay within 30 days & quote reference #42.".into());
    d.payment_terms.due_date = Some("20261206".into());
    d.lines[0].price.basis = Some(PriceBasis {
        value: "1.00".into(),
        unit: Some("C62".into()),
    });
    d
}
fn escaping() -> InvoiceDraft {
    let mut d = all_fields();
    d.seller.as_mut().unwrap().party.name = SPECIAL.into();
    d.buyer.as_mut().unwrap().endpoint.as_mut().unwrap().scheme = SPECIAL.into();
    d.lines[0].quantity.unit = SPECIAL.into();
    d.lines[0].price.basis.as_mut().unwrap().unit = Some(SPECIAL.into());
    d.payment.as_mut().unwrap().description = Some(SPECIAL.into());
    d.payment_terms.text = Some(SPECIAL.into());
    d
}

fn assert_all_fields(b: &[u8]) {
    let root = parsed(b);
    let mut expected_leaves = Vec::new();
    let mut seen = BTreeSet::new();
    let mut attributes = 0;
    for row in include_str!("all-fields.tsv")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let (path, expected) = row.split_once('\t').unwrap();
        assert!(seen.insert(path));
        if let Some((path, attribute)) = path.split_once("/@") {
            assert_eq!(
                at(&root, path).attributes[attribute],
                expected,
                "{path}/@{attribute}"
            );
            attributes += 1;
        } else {
            let node = at(&root, path);
            assert_eq!(node.text, expected, "{path}");
            assert!(node.children.is_empty(), "expected leaf: {path}");
            expected_leaves.push(node);
        }
    }
    let mut actual = Vec::new();
    leaves(&root, &mut actual);
    assert_eq!(
        actual.len(),
        expected_leaves.len(),
        "all leaves must be covered"
    );
    assert!(
        actual
            .iter()
            .zip(expected_leaves)
            .all(|(a, b)| std::ptr::eq(*a, b)),
        "source-defined leaf order"
    );
    assert_eq!(
        attribute_count(&root),
        attributes,
        "all attributes must be covered"
    );
}

#[test]
fn actual_bytes_originals_all_fields_and_opt_in_export() {
    let mut exports = Vec::new();
    let cases = original_cases();
    assert_eq!(cases.len(), 10);
    for (columns, input) in cases {
        let before = input.clone();
        let b = bytes(&input);
        assert_original(&columns, &b);
        assert_eq!(b, bytes(&input));
        assert_eq!(input, before);
        exports.push((
            format!("{}.xml", columns[0]),
            "ordinary-source-case",
            "original.tsv",
            b,
        ));
    }
    let b = bytes(&all_fields());
    assert_all_fields(&b);
    exports.push((
        "all-fields.xml".into(),
        "ordinary-development",
        "encoding/tests.rs all_fields",
        b,
    ));
    let b = bytes(&escaping());
    assert_escaping(&b);
    exports.push((
        "escaping.xml".into(),
        "syntactic-only-development",
        "encoding/tests.rs escaping",
        b,
    ));
    let Some(directory) = std::env::var_os("EINVOICE_BUILDER_EXPORT_DIR") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    assert!(
        directory.is_absolute() && directory.is_dir(),
        "export requires an existing absolute directory"
    );
    assert!(
        std::fs::read_dir(&directory).unwrap().next().is_none(),
        "export directory must be empty"
    );
    let mut metadata = String::from("# Actual private prepare return vectors; unvalidated development artifacts, never reserialized.\n# Arithmetic constants: tests/fixtures/calculation/original.tsv; source order: encoding-discovery source mapping.\n");
    metadata.push_str(&format!(
        "# target={}\n# profile={}\n# limits={:?}\n",
        Target::xrechnung_3_0_2().id(),
        crate::target::PROFILE_ID,
        PreparationLimits::default()
    ));
    metadata.push_str("filename\tclass\tprovenance\tbytes\n");
    for (name, class, provenance, b) in exports {
        assert!(name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'-' | b'.')));
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join(&name))
            .unwrap()
            .write_all(&b)
            .unwrap();
        metadata.push_str(&format!("{name}\t{class}\t{provenance}\t{}\n", b.len()));
    }
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("cases.tsv"))
        .unwrap()
        .write_all(metadata.as_bytes())
        .unwrap();
}

fn assert_escaping(b: &[u8]) {
    let text = std::str::from_utf8(b).unwrap();
    assert!(text.contains("A&amp;&lt;tag&gt;]]&gt;\"'&#xD;B\nC&#xD;\nD\tE Grüße 東京 😀 e\u{301}"));
    assert!(text.contains(
        "A&amp;&lt;tag&gt;]]&gt;&quot;'&#xD;B&#xA;C&#xD;&#xA;D&#x9;E Grüße 東京 😀 e\u{301}"
    ));
    assert!(!text.contains('\r'));
    let root = parsed(b);
    assert_eq!(at(&root, "A/SellerTradeParty/Name").text, SPECIAL);
    assert_eq!(
        at(&root, "A/BuyerTradeParty/URIUniversalCommunication/URIID").attributes["schemeID"],
        SPECIAL
    );
    assert_eq!(
        at(&root, "L/SpecifiedLineTradeDelivery/BilledQuantity").attributes["unitCode"],
        SPECIAL
    );
    for i in 1..=2 {
        assert_eq!(
            at(
                &root,
                &format!("S/SpecifiedTradeSettlementPaymentMeans[{i}]/Information")
            )
            .text,
            SPECIAL
        );
    }
    assert_eq!(
        at(&root, "S/SpecifiedTradePaymentTerms/Description").text,
        SPECIAL
    );
}

#[test]
fn fixed_envelope_and_schema_container_order() {
    let b = bytes(&all_fields());
    let s = std::str::from_utf8(&b).unwrap();
    let opening = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><rsm:CrossIndustryInvoice xmlns:rsm=\"{RSM}\" xmlns:ram=\"{RAM}\" xmlns:udt=\"{UDT}\">");
    assert!(s.starts_with(&opening));
    assert!(s.ends_with("</rsm:CrossIndustryInvoice>"));
    assert!(s.contains("<ram:ApplicableHeaderTradeDelivery/>"));
    assert!(!s.starts_with('\u{feff}'));
    assert!(!s.contains("\n"));
    let r = parsed(&b);
    order(
        &r,
        "",
        &[
            "rsm:ExchangedDocumentContext",
            "rsm:ExchangedDocument",
            "rsm:SupplyChainTradeTransaction",
        ],
    );
    order(
        &r,
        "rsm:SupplyChainTradeTransaction",
        &[
            "IncludedSupplyChainTradeLineItem",
            "ApplicableHeaderTradeAgreement",
            "ApplicableHeaderTradeDelivery",
            "ApplicableHeaderTradeSettlement",
        ],
    );
    order(
        &r,
        "L",
        &[
            "AssociatedDocumentLineDocument",
            "SpecifiedTradeProduct",
            "SpecifiedLineTradeAgreement",
            "SpecifiedLineTradeDelivery",
            "SpecifiedLineTradeSettlement",
        ],
    );
    order(
        &r,
        "A",
        &[
            "BuyerReference",
            "SellerTradeParty",
            "BuyerTradeParty",
            "SellerTaxRepresentativeTradeParty",
            "SellerOrderReferencedDocument",
            "BuyerOrderReferencedDocument",
            "ContractReferencedDocument",
        ],
    );
    order(
        &r,
        "S",
        &[
            "PaymentReference",
            "InvoiceCurrencyCode",
            "SpecifiedTradeSettlementPaymentMeans",
            "SpecifiedTradeSettlementPaymentMeans",
            "ApplicableTradeTax",
            "SpecifiedTradePaymentTerms",
            "SpecifiedTradeSettlementHeaderMonetarySummation",
        ],
    );
}

#[test]
fn canonical_numbers_absent_and_explicit_basis_and_signed_zero() {
    let mut d = draft("canonical", "EUR");
    d.lines = vec![
        line(" -000.2500 ", "+001.2300", Some("002.00"), "019.000"),
        line("-0.000", "+0.00", None, "19.0"),
    ];
    let r = parsed(&bytes(&d));
    let l = at(&r, "L");
    assert_eq!(
        at(l, "SpecifiedLineTradeDelivery/BilledQuantity").text,
        "-0.25"
    );
    assert_eq!(
        at(
            l,
            "SpecifiedLineTradeAgreement/NetPriceProductTradePrice/ChargeAmount"
        )
        .text,
        "1.23"
    );
    assert_eq!(
        at(
            l,
            "SpecifiedLineTradeAgreement/NetPriceProductTradePrice/BasisQuantity"
        )
        .text,
        "2"
    );
    assert_eq!(
        at(
            l,
            "SpecifiedLineTradeSettlement/ApplicableTradeTax/RateApplicablePercent"
        )
        .text,
        "19"
    );
    assert_eq!(at(l, "SpecifiedLineTradeSettlement/SpecifiedTradeSettlementLineMonetarySummation/LineTotalAmount").text, "-0.15");
    let second = at(
        &r,
        "rsm:SupplyChainTradeTransaction/IncludedSupplyChainTradeLineItem[2]",
    );
    assert_eq!(
        at(second, "SpecifiedLineTradeDelivery/BilledQuantity").text,
        "0"
    );
    assert_eq!(at(second, "SpecifiedLineTradeSettlement/SpecifiedTradeSettlementLineMonetarySummation/LineTotalAmount").text, "0.00");
    absent(
        second,
        "SpecifiedLineTradeAgreement/NetPriceProductTradePrice",
        "BasisQuantity",
    );
    let mut d = draft("basis", "EUR");
    let no_basis = parsed(&bytes(&d));
    absent(
        &no_basis,
        "L/SpecifiedLineTradeAgreement/NetPriceProductTradePrice",
        "BasisQuantity",
    );
    d.lines[0].price.basis = Some(PriceBasis {
        value: "1".into(),
        unit: None,
    });
    let explicit = parsed(&bytes(&d));
    assert_eq!(
        at(
            &explicit,
            "L/SpecifiedLineTradeAgreement/NetPriceProductTradePrice/BasisQuantity"
        )
        .text,
        "1"
    );
    assert!(at(
        &explicit,
        "L/SpecifiedLineTradeAgreement/NetPriceProductTradePrice/BasisQuantity"
    )
    .attributes
    .is_empty());
    for (quantity, price, basis, net) in [
        ("1", ".495", None, "0.50"),
        ("-1", ".495", None, "-0.50"),
        ("-1", ".029999999999999999999999999999", Some("6"), "0.00"),
        ("-1", ".03", Some("6"), "-0.01"),
    ] {
        d.lines = vec![line(quantity, price, basis, "19")];
        let r = parsed(&bytes(&d));
        assert_eq!(at(&r, "L/SpecifiedLineTradeSettlement/SpecifiedTradeSettlementLineMonetarySummation/LineTotalAmount").text, net);
    }
}

#[test]
fn numeric_group_order_is_not_lexical_or_input_order() {
    let mut d = draft("rates", "EUR");
    d.lines = vec![
        line("1", "1", None, "100"),
        line("1", "2", None, "019.0"),
        line("1", "3", None, "2"),
        line("1", "4", None, "19.000"),
    ];
    let r = parsed(&bytes(&d));
    for (i, (rate, basis, tax)) in [
        ("2", "3.00", "0.06"),
        ("19", "6.00", "1.14"),
        ("100", "1.00", "1.00"),
    ]
    .into_iter()
    .enumerate()
    {
        let g = at(&r, &format!("S/ApplicableTradeTax[{}]", i + 1));
        assert_eq!(at(g, "RateApplicablePercent").text, rate);
        assert_eq!(at(g, "BasisAmount").text, basis);
        assert_eq!(at(g, "CalculatedAmount").text, tax);
    }
}

#[test]
fn seller_identity_tax_alternatives_and_partial_buyer_contacts() {
    for identity in ["local", "global", "legal"] {
        for tax in ["FC", "representative"] {
            let mut d = draft("alternative", "EUR");
            let s = d.seller.as_mut().unwrap();
            s.party.vat_identifier = None;
            let id = Identifier {
                value: "identity".into(),
                scheme: (identity == "global").then(|| "0088".into()),
            };
            if identity == "legal" {
                s.party.legal_registration = Some(id);
            } else {
                s.party.identifier = Some(id);
            }
            if tax == "FC" {
                s.tax_registration = Some("tax-fc".into());
            } else {
                d.tax_representative = Some(TaxRepresentativeDraft {
                    name: "Representative".into(),
                    address: Some(PostalAddress {
                        country: "FR".into(),
                        ..PostalAddress::default()
                    }),
                    vat_identifier: "FR12345678901".into(),
                });
            }
            let r = parsed(&bytes(&d));
            let path = match identity {
                "global" => "GlobalID",
                "legal" => "SpecifiedLegalOrganization/ID",
                _ => "ID",
            };
            assert_eq!(
                at(&r, &format!("A/SellerTradeParty/{path}")).text,
                "identity"
            );
            if tax == "FC" {
                assert_eq!(
                    at(&r, "A/SellerTradeParty/SpecifiedTaxRegistration/ID").attributes["schemeID"],
                    "FC"
                );
            } else {
                order(
                    &r,
                    "A/SellerTaxRepresentativeTradeParty/PostalTradeAddress",
                    &["CountryID"],
                );
            }
        }
    }
    for (contact, expected) in [
        (
            Contact {
                point: Some(ContactPoint::Person("Buyer person".into())),
                ..Contact::default()
            },
            "PersonName",
        ),
        (
            Contact {
                point: Some(ContactPoint::Department("Buyer department".into())),
                ..Contact::default()
            },
            "DepartmentName",
        ),
        (
            Contact {
                telephone: Some("buyer phone".into()),
                ..Contact::default()
            },
            "TelephoneUniversalCommunication",
        ),
        (
            Contact {
                email: Some("buyer@example.test".into()),
                ..Contact::default()
            },
            "EmailURIUniversalCommunication",
        ),
    ] {
        let mut d = draft("partial-contact", "EUR");
        d.buyer.as_mut().unwrap().contact = Some(contact);
        let r = parsed(&bytes(&d));
        order(&r, "A/BuyerTradeParty/DefinedTradeContact", &[expected]);
    }
}

#[test]
fn payment_variants_and_terms_presence_are_preserved() {
    for means in [
        PaymentMeans::CreditTransfer,
        PaymentMeans::SepaCreditTransfer,
    ] {
        let mut d = all_fields();
        d.payment.as_mut().unwrap().means = means.clone();
        let r = parsed(&bytes(&d));
        for i in 1..=2 {
            assert_eq!(
                at(
                    &r,
                    &format!("S/SpecifiedTradeSettlementPaymentMeans[{i}]/TypeCode")
                )
                .text,
                if means == PaymentMeans::CreditTransfer {
                    "30"
                } else {
                    "58"
                }
            );
        }
        assert_eq!(at(&r, "S/SpecifiedTradeSettlementPaymentMeans[2]/PayeePartyCreditorFinancialAccount/ProprietaryID").text, "local-account");
    }
    for (text, due, names) in [
        (None, None, vec![]),
        (Some("Plain prose #42"), None, vec!["Description"]),
        (None, Some("20261206"), vec!["DueDateDateTime"]),
        (
            Some("Plain prose without final newline"),
            Some("20261206"),
            vec!["Description", "DueDateDateTime"],
        ),
    ] {
        let mut d = draft("terms", "EUR");
        d.payment_terms.text = text.map(Into::into);
        d.payment_terms.due_date = due.map(Into::into);
        let r = parsed(&bytes(&d));
        if names.is_empty() {
            absent(&r, "S", "SpecifiedTradePaymentTerms");
        } else {
            order(&r, "S/SpecifiedTradePaymentTerms", &names);
        }
    }
    let minimal = parsed(&bytes(&draft("minimal", "EUR")));
    absent(&minimal, "A/BuyerTradeParty", "DefinedTradeContact");
    absent(&minimal, "A", "SellerTaxRepresentativeTradeParty");
    absent(&minimal, "A", "ContractReferencedDocument");
    absent(&minimal, "S", "PaymentReference");
    absent(
        &minimal,
        "S/SpecifiedTradeSettlementPaymentMeans",
        "Information",
    );
    absent(
        &minimal,
        "S/SpecifiedTradeSettlementPaymentMeans",
        "PayeeSpecifiedCreditorFinancialInstitution",
    );
    order(
        &minimal,
        "S/SpecifiedTradeSettlementPaymentMeans/PayeePartyCreditorFinancialAccount",
        &["ProprietaryID"],
    );
}

#[test]
fn plain_terms_guard_is_shared_bounded_and_has_no_source_rule_claim() {
    for text in [
        "#SKONTO#TAGE=14#PROZENT=2.00#\n",
        "#INVALID#",
        "prose\n \t\r#INVALID#",
        "\r#INVALID#",
    ] {
        let mut d = draft("terms-refusal", "EUR");
        d.payment_terms.text = Some(text.into());
        let before = d.clone();
        let diagnostics = d.field_diagnostics();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "API-BUILD-UNSUPPORTED-TERMS");
        assert_eq!(diagnostics[0].path, "payment_terms.text");
        assert!(diagnostics[0].rule_ids.is_empty());
        let error = CalculationError::Fields { diagnostics };
        assert_eq!(d.calculate(&Target::xrechnung_3_0_2()).unwrap_err(), error);
        assert_eq!(
            limited(&d, PreparationLimits::default()),
            Err(PreparationError::Calculation(error))
        );
        assert_eq!(d, before);
    }
    for text in [
        "Pay #42",
        "Plain prose",
        "2% Skonto",
        "\u{a0}#prose",
        "\u{2003}#prose",
        "x\r#embedded",
        "No final newline",
    ] {
        let mut d = draft("plain", "EUR");
        d.payment_terms.text = Some(text.into());
        assert!(d.field_diagnostics().is_empty());
        assert_eq!(
            at(
                &parsed(&bytes(&d)),
                "S/SpecifiedTradePaymentTerms/Description"
            )
            .text,
            text
        );
    }
    let mut d = draft("diagnostics", "EUR");
    d.payment_terms.text = Some("#unsupported".into());
    d.lines[0].quantity.value = "bad".into();
    let limit = PreparationLimits {
        calculation: CalculationLimits {
            max_diagnostics: 1,
            ..CalculationLimits::default()
        },
        ..PreparationLimits::default()
    };
    assert!(matches!(
        limited(&d, limit),
        Err(PreparationError::Calculation(CalculationError::Resource {
            cause: CalculationResource::Diagnostics,
            ..
        }))
    ));
    let limit = PreparationLimits {
        calculation: CalculationLimits {
            max_diagnostics: 2,
            ..CalculationLimits::default()
        },
        ..PreparationLimits::default()
    };
    match limited(&d, limit).unwrap_err() {
        PreparationError::Calculation(CalculationError::Fields { diagnostics }) => {
            assert_eq!(diagnostics.len(), 2)
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn field_numeric_expectation_failures_are_wrapped_without_changes() {
    let mut cases = Vec::new();
    let mut d = draft("missing", "EUR");
    d.seller = None;
    cases.push(d);
    let mut d = draft("blank", "EUR");
    d.references.contract = Some("".into());
    cases.push(d);
    let mut d = draft("expectations", "EUR");
    d.lines[0].expected_net = Some("999".into());
    d.expected.vat_groups = vec![VatGroupExpectation {
        rate: "19".into(),
        basis: Some("999".into()),
        tax: Some("999".into()),
    }];
    d.expected.net = Some("999".into());
    d.expected.tax = Some("999".into());
    d.expected.gross = Some("999".into());
    d.expected.due = Some("999".into());
    cases.push(d);
    let mut d = draft("numeric", "EUR");
    d.lines[0].price.amount = "not-a-number".into();
    cases.push(d);
    let mut d = draft("basis", "EUR");
    d.lines[0].price.basis = Some(PriceBasis {
        value: "0".into(),
        unit: None,
    });
    cases.push(d);
    let mut d = draft("unsupported", "EUR");
    d.payment.as_mut().unwrap().means = PaymentMeans::Unsupported("49".into());
    cases.push(d);
    for d in cases {
        let before = d.clone();
        let error = d.calculate(&Target::xrechnung_3_0_2()).unwrap_err();
        assert!(matches!(error, CalculationError::Fields { .. }));
        assert_eq!(
            limited(&d, PreparationLimits::default()),
            Err(PreparationError::Calculation(error))
        );
        assert_eq!(d, before);
    }
}

#[test]
fn target_and_resource_precedence_and_separate_budgets() {
    let empty = InvoiceDraft::new("", "");
    assert_eq!(
        prepare(
            &empty,
            &Target::named("x".repeat(1_000_000)),
            PreparationLimits {
                max_xml_bytes: 0,
                calculation: CalculationLimits {
                    max_lines: 0,
                    ..CalculationLimits::default()
                }
            }
        ),
        Err(PreparationError::Calculation(
            CalculationError::UnsupportedTarget
        ))
    );
    for cap in [0, usize::MAX] {
        assert_eq!(
            limited(
                &empty,
                PreparationLimits {
                    max_xml_bytes: cap,
                    ..PreparationLimits::default()
                }
            ),
            Err(PreparationError::InvalidConfiguration)
        );
    }
    let mut d = draft("resources", "EUR");
    d.payment_terms.text = Some("#unsupported".into());
    d.lines.push(d.lines[0].clone());
    d.payment
        .as_mut()
        .unwrap()
        .accounts
        .push(TransferAccount::new(AccountIdentifier::Iban(
            "DE89370400440532013000".into(),
        )));
    d.expected.vat_groups = vec![
        VatGroupExpectation {
            rate: "19".into(),
            ..VatGroupExpectation::default()
        },
        VatGroupExpectation {
            rate: "7".into(),
            ..VatGroupExpectation::default()
        },
    ];
    for calculation in [
        CalculationLimits {
            max_lines: 1,
            ..CalculationLimits::default()
        },
        CalculationLimits {
            max_accounts: 0,
            ..CalculationLimits::default()
        },
        CalculationLimits {
            max_accounts: 1,
            ..CalculationLimits::default()
        },
        CalculationLimits {
            max_expected_groups: 1,
            ..CalculationLimits::default()
        },
        CalculationLimits {
            max_input_bytes: 1,
            ..CalculationLimits::default()
        },
        CalculationLimits {
            max_string_bytes: 1,
            ..CalculationLimits::default()
        },
        CalculationLimits {
            max_output_bytes: 1,
            ..CalculationLimits::default()
        },
        CalculationLimits {
            cumulative_work: 1,
            ..CalculationLimits::default()
        },
    ] {
        let expected = d
            .calculate_with_limits(&Target::xrechnung_3_0_2(), calculation)
            .unwrap_err();
        assert!(matches!(expected, CalculationError::Resource { .. }));
        assert_eq!(
            limited(
                &d,
                PreparationLimits {
                    calculation,
                    max_xml_bytes: 1
                }
            ),
            Err(PreparationError::Calculation(expected))
        );
    }
    let d = draft("resources", "EUR");
    let defaults = PreparationLimits::default();
    assert_eq!(defaults.max_xml_bytes, 16_777_216);
    let exact = bytes(&d).len();
    assert_eq!(
        limited(
            &d,
            PreparationLimits {
                max_xml_bytes: exact,
                ..defaults
            }
        )
        .unwrap(),
        bytes(&d)
    );
    assert_eq!(
        limited(
            &d,
            PreparationLimits {
                max_xml_bytes: exact - 1,
                ..defaults
            }
        ),
        Err(PreparationError::OutputLimit { limit: exact - 1 })
    );
    assert_eq!(
        limited(
            &d,
            PreparationLimits {
                max_xml_bytes: 1,
                ..defaults
            }
        ),
        Err(PreparationError::OutputLimit { limit: 1 })
    );
    // Numeric fuel is passed through once; even a large XML cap cannot replenish it.
    assert!(matches!(
        limited(
            &d,
            PreparationLimits {
                calculation: CalculationLimits {
                    cumulative_work: 1,
                    ..defaults.calculation
                },
                ..defaults
            }
        ),
        Err(PreparationError::Calculation(CalculationError::Resource {
            cause: CalculationResource::Work,
            ..
        }))
    ));
}

#[test]
fn exact_count_includes_escape_expansion_and_repeated_payment_information() {
    let mut d = all_fields();
    d.payment.as_mut().unwrap().description = Some("a".into());
    let ordinary = bytes(&d);
    d.payment.as_mut().unwrap().description = Some("&".into());
    let escaped = bytes(&d);
    assert_eq!(escaped.len(), ordinary.len() + 2 * 4);
    let d = escaping();
    let b = bytes(&d);
    assert_eq!(
        limited(
            &d,
            PreparationLimits {
                max_xml_bytes: b.len(),
                ..PreparationLimits::default()
            }
        )
        .unwrap(),
        b
    );
    assert_eq!(
        limited(
            &d,
            PreparationLimits {
                max_xml_bytes: b.len() - 1,
                ..PreparationLimits::default()
            }
        ),
        Err(PreparationError::OutputLimit { limit: b.len() - 1 })
    );
}

#[test]
fn invalid_xml_is_a_field_fault_and_defensive_writer_fault_is_an_invariant() {
    for c in ['\0', '\u{8}', '\u{b}', '\u{1f}', '\u{fffe}', '\u{ffff}'] {
        for attribute in [false, true] {
            let mut d = draft("invalid-character", "EUR");
            if attribute {
                d.lines[0].quantity.unit = format!("unit{c}");
            } else {
                d.lines[0].name = format!("name{c}");
            }
            let error = d.calculate(&Target::xrechnung_3_0_2()).unwrap_err();
            assert!(
                matches!(&error, CalculationError::Fields { diagnostics } if diagnostics.iter().any(|d| d.code == "API-BUILD-XML-CHAR"))
            );
            assert_eq!(
                limited(&d, PreparationLimits::default()),
                Err(PreparationError::Calculation(error))
            );
        }
        let mut sink = Count {
            length: 0,
            cap: 1000,
        };
        assert_eq!(
            Writer(&mut sink).value(&format!("before{c}after"), false),
            Err(PreparationError::InternalInvariant)
        );
    }
    let mut d = draft("unicode", "EUR");
    d.lines[0].name = "\t\n\r\u{20}\u{d7ff}\u{e000}\u{fffd}\u{10000}\u{10ffff}".into();
    assert_eq!(
        at(&parsed(&bytes(&d)), "L/SpecifiedTradeProduct/Name").text,
        d.lines[0].name
    );
}

#[test]
fn checked_sinks_refuse_before_writing_and_do_not_grow() {
    assert_eq!(
        next_length(usize::MAX, 1, usize::MAX),
        Err(PreparationError::SizeOverflow)
    );
    let mut count = Count {
        length: usize::MAX,
        cap: usize::MAX,
    };
    assert_eq!(count.append(b"x"), Err(PreparationError::SizeOverflow));
    let mut output = Output {
        bytes: Vec::with_capacity(4),
        expected: 4,
        cap: 4,
    };
    output.append(b"1234").unwrap();
    let capacity = output.bytes.capacity();
    assert_eq!(
        output.append(b"5"),
        Err(PreparationError::OutputLimit { limit: 4 })
    );
    assert_eq!(output.bytes, b"1234");
    assert_eq!(output.bytes.capacity(), capacity);
    output.expected = 3;
    assert_eq!(output.append(b""), Err(PreparationError::InternalInvariant));
    assert_eq!(output.bytes, b"1234");
}

#[test]
fn imported_bytes_and_drafts_remain_unchanged_and_public_encoding_unavailable() {
    let source = include_bytes!("../fixtures/profile-only.xml");
    let document = Document::parse(source).unwrap();
    let copied = document.to_bytes();
    let d = all_fields();
    let before = d.clone();
    assert!(!bytes(&d).is_empty());
    assert!(limited(
        &d,
        PreparationLimits {
            max_xml_bytes: 1,
            ..PreparationLimits::default()
        }
    )
    .is_err());
    assert_eq!(d, before);
    assert_eq!(document.original_bytes(), source);
    assert_eq!(document.to_bytes(), copied);
    assert_eq!(copied, source);
    assert!(matches!(
        Target::xrechnung_3_0_2().encode(&d),
        Err(BuildError::Unavailable { .. })
    ));
    let mut invalid = d;
    invalid.payment_terms.text = Some("#unsupported".into());
    let before = invalid.clone();
    assert!(limited(&invalid, PreparationLimits::default()).is_err());
    assert_eq!(invalid, before);
    assert_eq!(document.original_bytes(), source);
}

#[test]
fn calculation_fuel_is_forwarded_at_boundary() {
    let mut input = draft("review-fuel", "EUR");
    input.lines = (1..=5)
        .map(|id| {
            let mut item = line("2", "12.345", None, "19");
            item.id = id.to_string();
            item
        })
        .collect();
    let target = Target::xrechnung_3_0_2();
    let policy = |work| CalculationLimits {
        cumulative_work: work,
        ..CalculationLimits::default()
    };
    let (mut lo, mut hi) = (1, CalculationLimits::default().cumulative_work);
    input.calculate_with_limits(&target, policy(hi)).unwrap();
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        match input.calculate_with_limits(&target, policy(mid)) {
            Ok(_) => hi = mid,
            Err(CalculationError::Resource {
                cause: CalculationResource::Work,
                ..
            }) => lo = mid + 1,
            other => panic!("unexpected calculator calibration: {other:?}"),
        }
    }
    assert!(lo > 1);
    let direct = input
        .calculate_with_limits(&target, policy(lo - 1))
        .unwrap_err();
    assert!(matches!(
        direct,
        CalculationError::Resource {
            cause: CalculationResource::Work,
            ..
        }
    ));
    let limits = |work| PreparationLimits {
        calculation: policy(work),
        ..PreparationLimits::default()
    };
    assert_eq!(limited(&input, limits(lo)).unwrap(), bytes(&input));
    let actual = limited(&input, limits(lo - 1));
    assert_eq!(actual, Err(PreparationError::Calculation(direct.clone())));
    assert_eq!(
        limited(
            &input,
            PreparationLimits {
                max_xml_bytes: 1,
                ..limits(lo - 1)
            }
        ),
        Err(PreparationError::Calculation(direct))
    );
}
