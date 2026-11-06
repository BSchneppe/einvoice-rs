use einvoice_core::parse_invoice;
use einvoice_core::{InvoiceError, InvoiceStandard};

fn main() {
    let xml = r#"
<Invoice xmlns="urn:oasis:names:specification:ubl:schema:xsd:Invoice-2" xmlns:cac="urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2" xmlns:cbc="urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2">
    <cbc:ID>123</cbc:ID>
</Invoice>
"#;
    let parsed_invoice: Result<InvoiceStandard, InvoiceError> = parse_invoice(xml);
    match parsed_invoice {
        Ok(_) => println!("XML parsed; invoice conformance has not been checked"),
        Err(e) => println!("XML parsing failed: {:?}", e),
    }
}
