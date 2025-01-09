use einvoice::validate_invoice;
use einvoice::{InvoiceError, InvoiceStandard};

fn main() {
    let xml = r#"
<Invoice xmlns="urn:oasis:names:specification:ubl:schema:xsd:Invoice-2" xmlns:cac="urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2" xmlns:cbc="urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2">
    <cbc:ID>123</cbc:ID>
</Invoice>
"#;
    let invalid_invoice: Result<InvoiceStandard, InvoiceError> = validate_invoice(xml);
    match invalid_invoice {
        Ok(_) => println!("Invoice is valid"),
        Err(e) => println!("Invoice is invalid: {:?}", e),
    }
}
