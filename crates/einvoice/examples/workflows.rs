use einvoice::{BuildError, Document, EngineFailure, InvoiceDraft, Target, ValidationFailure};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // XML with a matching profile, deliberately incomplete as an invoice.
    let source = br#"<?xml version="1.0"?>
<rsm:CrossIndustryInvoice xmlns:rsm="urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100"
 xmlns:ram="urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100">
 <rsm:ExchangedDocumentContext><ram:GuidelineSpecifiedDocumentContextParameter>
  <ram:ID>urn:cen.eu:en16931:2017#compliant#urn:xeinkauf.de:kosit:xrechnung_3.0</ram:ID>
 </ram:GuidelineSpecifiedDocumentContextParameter></rsm:ExchangedDocumentContext>
</rsm:CrossIndustryInvoice>"#;
    let target = Target::xrechnung_3_0_2();
    let document = Document::parse(source)?;
    assert_eq!(document.original_bytes(), source);
    assert_eq!(document.to_bytes(), source);

    let report = document.validate(&target);
    assert_eq!(
        report.failure(),
        Some(&ValidationFailure::Engine(
            EngineFailure::BackendUnavailable
        ))
    );
    assert!(!report.is_valid());
    assert!(!report.is_complete());
    println!("Target: {}", report.requested_target());
    println!("Validation unavailable: {:?}", report.failure());
    for stage in report.stages() {
        println!("{:?}: {:?}", stage.stage, stage.status);
    }

    // Separate workflow: start a new invoice, without converting the import.
    let draft = InvoiceDraft::new("INV-2026-001", "EUR");
    assert!(matches!(
        target.encode(&draft),
        Err(BuildError::Unavailable { .. })
    ));
    println!("Creation unavailable: no validated XML was generated.");
    Ok(())
}
