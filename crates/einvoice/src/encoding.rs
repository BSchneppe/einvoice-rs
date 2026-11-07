//! Private fresh-byte preparation. These bytes have not passed native validation.

use crate::{
    AccountIdentifier, CalculatedInvoice, CalculationError, CalculationLimits, Contact,
    ContactPoint, Identifier, InvoiceDraft, PartyDraft, PaymentMeans, PostalAddress, Target,
};

#[derive(Debug, Clone, Copy)]
pub(crate) struct PreparationLimits {
    pub(crate) calculation: CalculationLimits,
    pub(crate) max_xml_bytes: usize,
}

impl Default for PreparationLimits {
    fn default() -> Self {
        Self {
            calculation: CalculationLimits::default(),
            max_xml_bytes: 16_777_216,
        }
    }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum PreparationError {
    #[error(transparent)]
    Calculation(#[from] CalculationError),
    #[error("XML output limit must be nonzero and platform-safe")]
    InvalidConfiguration,
    #[error("XML output exceeds {limit} bytes")]
    OutputLimit { limit: usize },
    #[error("XML output size overflow")]
    SizeOverflow,
    #[error("XML output reservation failed")]
    Reservation,
    #[error("internal XML preparation invariant failed")]
    InternalInvariant,
}

type Result<T> = std::result::Result<T, PreparationError>;

/// Calculate exactly once from the same immutable input used for both XML passes.
/// Retained calculation bounds and the XML cap are separate, not an RSS quota.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "BSC-8 native creation wiring unavailable")
)]
pub(crate) fn prepare(
    draft: &InvoiceDraft,
    target: &Target,
    limits: PreparationLimits,
) -> Result<Vec<u8>> {
    if target.bundle().is_none() {
        return Err(CalculationError::UnsupportedTarget.into());
    }
    if limits.max_xml_bytes == 0 || limits.max_xml_bytes > isize::MAX as usize {
        return Err(PreparationError::InvalidConfiguration);
    }
    let calculated = crate::calculation::calculate(draft, target, limits.calculation)?;
    let prepared = Prepared { draft, calculated };
    let mut count = Count {
        length: 0,
        cap: limits.max_xml_bytes,
    };
    prepared.emit(&mut Writer(&mut count))?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(count.length)
        .map_err(|_| PreparationError::Reservation)?;
    let mut output = Output {
        bytes,
        expected: count.length,
        cap: limits.max_xml_bytes,
    };
    prepared.emit(&mut Writer(&mut output))?;
    if output.bytes.len() != count.length {
        return Err(PreparationError::InternalInvariant);
    }
    Ok(output.bytes)
}

trait Sink {
    fn append(&mut self, bytes: &[u8]) -> Result<()>;
}

fn next_length(length: usize, additional: usize, cap: usize) -> Result<usize> {
    let next = length
        .checked_add(additional)
        .ok_or(PreparationError::SizeOverflow)?;
    if next > cap {
        return Err(PreparationError::OutputLimit { limit: cap });
    }
    Ok(next)
}

struct Count {
    length: usize,
    cap: usize,
}
impl Sink for Count {
    fn append(&mut self, bytes: &[u8]) -> Result<()> {
        self.length = next_length(self.length, bytes.len(), self.cap)?;
        Ok(())
    }
}

struct Output {
    bytes: Vec<u8>,
    expected: usize,
    cap: usize,
}
impl Sink for Output {
    fn append(&mut self, bytes: &[u8]) -> Result<()> {
        let next = next_length(self.bytes.len(), bytes.len(), self.cap)?;
        if next > self.expected || next > self.bytes.capacity() {
            return Err(PreparationError::InternalInvariant);
        }
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
}

struct Writer<'a, S>(&'a mut S);
impl<S: Sink> Writer<'_, S> {
    // Only fixed vocabulary fragments reach raw; dynamic values always use value.
    fn raw(&mut self, constant: &'static str) -> Result<()> {
        self.0.append(constant.as_bytes())
    }

    fn value(&mut self, value: &str, attribute: bool) -> Result<()> {
        let mut start = 0;
        for (index, c) in value.char_indices() {
            let escaped = match c {
                '&' => Some("&amp;"),
                '<' => Some("&lt;"),
                '>' => Some("&gt;"),
                '\r' => Some("&#xD;"),
                '"' if attribute => Some("&quot;"),
                '\n' if attribute => Some("&#xA;"),
                '\t' if attribute => Some("&#x9;"),
                '\t'
                | '\n'
                | '\u{20}'..='\u{D7FF}'
                | '\u{E000}'..='\u{FFFD}'
                | '\u{10000}'..='\u{10FFFF}' => None,
                _ => return Err(PreparationError::InternalInvariant),
            };
            if let Some(escaped) = escaped {
                self.0.append(&value.as_bytes()[start..index])?;
                self.raw(escaped)?;
                start = index + c.len_utf8();
            }
        }
        self.0.append(&value.as_bytes()[start..])
    }
}

// Literal-only vocabulary. Neither helper accepts runtime element/attribute names.
macro_rules! text {
    ($w:expr, $tag:literal, $value:expr) => {{
        $w.raw(concat!("<", $tag, ">"))?;
        $w.value($value, false)?;
        $w.raw(concat!("</", $tag, ">"))?;
    }};
}
macro_rules! optional {
    ($w:expr, $tag:literal, $value:expr) => {
        if let Some(value) = $value.as_deref() {
            text!($w, $tag, value);
        }
    };
}
macro_rules! attributed {
    ($w:expr, $tag:literal, $attr:literal, $value:expr, $text:expr) => {{
        $w.raw(concat!("<", $tag, " ", $attr, "=\""))?;
        $w.value($value, true)?;
        $w.raw("\">")?;
        $w.value($text, false)?;
        $w.raw(concat!("</", $tag, ">"))?;
    }};
}

// Only prepare constructs this pair; sibling modules cannot supply stale results.
struct Prepared<'a> {
    draft: &'a InvoiceDraft,
    calculated: CalculatedInvoice,
}
impl Prepared<'_> {
    fn emit<S: Sink>(&self, w: &mut Writer<'_, S>) -> Result<()> {
        let d = self.draft;
        let c = &self.calculated;
        w.raw("<?xml version=\"1.0\" encoding=\"UTF-8\"?><rsm:CrossIndustryInvoice xmlns:rsm=\"urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100\" xmlns:ram=\"urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100\" xmlns:udt=\"urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100\"><rsm:ExchangedDocumentContext><ram:BusinessProcessSpecifiedDocumentContextParameter>")?;
        text!(w, "ram:ID", &d.business_process);
        w.raw("</ram:BusinessProcessSpecifiedDocumentContextParameter><ram:GuidelineSpecifiedDocumentContextParameter>")?;
        text!(w, "ram:ID", crate::target::PROFILE_ID);
        w.raw("</ram:GuidelineSpecifiedDocumentContextParameter></rsm:ExchangedDocumentContext><rsm:ExchangedDocument>")?;
        text!(w, "ram:ID", d.invoice_number());
        text!(w, "ram:TypeCode", &d.invoice_type);
        w.raw("<ram:IssueDateTime>")?;
        date(w, &d.issue_date)?;
        w.raw("</ram:IssueDateTime></rsm:ExchangedDocument><rsm:SupplyChainTradeTransaction>")?;
        if d.lines.len() != c.lines().len() {
            return Err(PreparationError::InternalInvariant);
        }
        for (line, amounts) in d.lines.iter().zip(c.lines()) {
            w.raw("<ram:IncludedSupplyChainTradeLineItem><ram:AssociatedDocumentLineDocument>")?;
            text!(w, "ram:LineID", &line.id);
            w.raw("</ram:AssociatedDocumentLineDocument><ram:SpecifiedTradeProduct>")?;
            text!(w, "ram:Name", &line.name);
            w.raw("</ram:SpecifiedTradeProduct><ram:SpecifiedLineTradeAgreement><ram:NetPriceProductTradePrice>")?;
            text!(w, "ram:ChargeAmount", amounts.price());
            if let Some(basis) = &line.price.basis {
                let value = amounts
                    .price_basis()
                    .ok_or(PreparationError::InternalInvariant)?;
                if let Some(unit) = &basis.unit {
                    attributed!(w, "ram:BasisQuantity", "unitCode", unit, value);
                } else {
                    text!(w, "ram:BasisQuantity", value);
                }
            } else if amounts.price_basis().is_some() {
                return Err(PreparationError::InternalInvariant);
            }
            w.raw("</ram:NetPriceProductTradePrice></ram:SpecifiedLineTradeAgreement><ram:SpecifiedLineTradeDelivery>")?;
            attributed!(
                w,
                "ram:BilledQuantity",
                "unitCode",
                &line.quantity.unit,
                amounts.quantity()
            );
            w.raw("</ram:SpecifiedLineTradeDelivery><ram:SpecifiedLineTradeSettlement><ram:ApplicableTradeTax>")?;
            text!(w, "ram:TypeCode", "VAT");
            text!(w, "ram:CategoryCode", &line.vat.category);
            text!(w, "ram:RateApplicablePercent", amounts.vat_rate());
            w.raw("</ram:ApplicableTradeTax><ram:SpecifiedTradeSettlementLineMonetarySummation>")?;
            text!(w, "ram:LineTotalAmount", amounts.net());
            w.raw("</ram:SpecifiedTradeSettlementLineMonetarySummation></ram:SpecifiedLineTradeSettlement></ram:IncludedSupplyChainTradeLineItem>")?;
        }
        w.raw("<ram:ApplicableHeaderTradeAgreement>")?;
        text!(w, "ram:BuyerReference", &d.references.buyer_reference);
        let seller = d
            .seller
            .as_ref()
            .ok_or(PreparationError::InternalInvariant)?;
        w.raw("<ram:SellerTradeParty>")?;
        party(w, &seller.party, seller.tax_registration.as_deref())?;
        w.raw("</ram:SellerTradeParty><ram:BuyerTradeParty>")?;
        party(
            w,
            d.buyer
                .as_ref()
                .ok_or(PreparationError::InternalInvariant)?,
            None,
        )?;
        w.raw("</ram:BuyerTradeParty>")?;
        if let Some(rep) = &d.tax_representative {
            w.raw("<ram:SellerTaxRepresentativeTradeParty>")?;
            text!(w, "ram:Name", &rep.name);
            address(
                w,
                rep.address
                    .as_ref()
                    .ok_or(PreparationError::InternalInvariant)?,
            )?;
            tax_id(w, &rep.vat_identifier, "VA")?;
            w.raw("</ram:SellerTaxRepresentativeTradeParty>")?;
        }
        if let Some(value) = &d.references.seller_order {
            w.raw("<ram:SellerOrderReferencedDocument>")?;
            text!(w, "ram:IssuerAssignedID", value);
            w.raw("</ram:SellerOrderReferencedDocument>")?;
        }
        if let Some(value) = &d.references.purchase_order {
            w.raw("<ram:BuyerOrderReferencedDocument>")?;
            text!(w, "ram:IssuerAssignedID", value);
            w.raw("</ram:BuyerOrderReferencedDocument>")?;
        }
        if let Some(value) = &d.references.contract {
            w.raw("<ram:ContractReferencedDocument>")?;
            text!(w, "ram:IssuerAssignedID", value);
            w.raw("</ram:ContractReferencedDocument>")?;
        }
        w.raw("</ram:ApplicableHeaderTradeAgreement><ram:ApplicableHeaderTradeDelivery/><ram:ApplicableHeaderTradeSettlement>")?;
        let payment = d
            .payment
            .as_ref()
            .ok_or(PreparationError::InternalInvariant)?;
        optional!(w, "ram:PaymentReference", payment.reference);
        text!(w, "ram:InvoiceCurrencyCode", c.currency());
        let means = match payment.means {
            PaymentMeans::CreditTransfer => "30",
            PaymentMeans::SepaCreditTransfer => "58",
            PaymentMeans::Unsupported(_) => return Err(PreparationError::InternalInvariant),
        };
        for account in &payment.accounts {
            w.raw("<ram:SpecifiedTradeSettlementPaymentMeans>")?;
            text!(w, "ram:TypeCode", means);
            optional!(w, "ram:Information", payment.description);
            w.raw("<ram:PayeePartyCreditorFinancialAccount>")?;
            if let AccountIdentifier::Iban(value) = &account.identifier {
                text!(w, "ram:IBANID", value);
            }
            optional!(w, "ram:AccountName", account.name);
            if let AccountIdentifier::Proprietary(value) = &account.identifier {
                text!(w, "ram:ProprietaryID", value);
            }
            w.raw("</ram:PayeePartyCreditorFinancialAccount>")?;
            if let Some(bic) = &account.bic {
                w.raw("<ram:PayeeSpecifiedCreditorFinancialInstitution>")?;
                text!(w, "ram:BICID", bic);
                w.raw("</ram:PayeeSpecifiedCreditorFinancialInstitution>")?;
            }
            w.raw("</ram:SpecifiedTradeSettlementPaymentMeans>")?;
        }
        for group in c.vat_groups() {
            w.raw("<ram:ApplicableTradeTax>")?;
            text!(w, "ram:CalculatedAmount", group.tax());
            text!(w, "ram:TypeCode", "VAT");
            text!(w, "ram:BasisAmount", group.basis());
            text!(w, "ram:CategoryCode", "S");
            text!(w, "ram:RateApplicablePercent", group.rate());
            w.raw("</ram:ApplicableTradeTax>")?;
        }
        if d.payment_terms.text.is_some() || d.payment_terms.due_date.is_some() {
            w.raw("<ram:SpecifiedTradePaymentTerms>")?;
            optional!(w, "ram:Description", d.payment_terms.text);
            if let Some(due) = &d.payment_terms.due_date {
                w.raw("<ram:DueDateDateTime>")?;
                date(w, due)?;
                w.raw("</ram:DueDateDateTime>")?;
            }
            w.raw("</ram:SpecifiedTradePaymentTerms>")?;
        }
        w.raw("<ram:SpecifiedTradeSettlementHeaderMonetarySummation>")?;
        text!(w, "ram:LineTotalAmount", c.totals().net());
        text!(w, "ram:TaxBasisTotalAmount", c.totals().net());
        attributed!(
            w,
            "ram:TaxTotalAmount",
            "currencyID",
            c.currency(),
            c.totals().tax()
        );
        text!(w, "ram:GrandTotalAmount", c.totals().gross());
        text!(w, "ram:DuePayableAmount", c.totals().due());
        w.raw("</ram:SpecifiedTradeSettlementHeaderMonetarySummation></ram:ApplicableHeaderTradeSettlement></rsm:SupplyChainTradeTransaction></rsm:CrossIndustryInvoice>")
    }
}

fn date<S: Sink>(w: &mut Writer<'_, S>, value: &str) -> Result<()> {
    attributed!(w, "udt:DateTimeString", "format", "102", value);
    Ok(())
}

fn identifier<S: Sink>(w: &mut Writer<'_, S>, id: &Identifier) -> Result<()> {
    if let Some(scheme) = &id.scheme {
        attributed!(w, "ram:ID", "schemeID", scheme, &id.value);
    } else {
        text!(w, "ram:ID", &id.value);
    }
    Ok(())
}

fn tax_id<S: Sink>(w: &mut Writer<'_, S>, value: &str, scheme: &'static str) -> Result<()> {
    w.raw("<ram:SpecifiedTaxRegistration>")?;
    attributed!(w, "ram:ID", "schemeID", scheme, value);
    w.raw("</ram:SpecifiedTaxRegistration>")
}

fn party<S: Sink>(w: &mut Writer<'_, S>, p: &PartyDraft, fc: Option<&str>) -> Result<()> {
    if let Some(id) = &p.identifier {
        if let Some(scheme) = &id.scheme {
            attributed!(w, "ram:GlobalID", "schemeID", scheme, &id.value);
        } else {
            text!(w, "ram:ID", &id.value);
        }
    }
    text!(w, "ram:Name", &p.name);
    if let Some(id) = &p.legal_registration {
        w.raw("<ram:SpecifiedLegalOrganization>")?;
        identifier(w, id)?;
        w.raw("</ram:SpecifiedLegalOrganization>")?;
    }
    if let Some(value) = &p.contact {
        contact(w, value)?;
    }
    address(
        w,
        p.address
            .as_ref()
            .ok_or(PreparationError::InternalInvariant)?,
    )?;
    let endpoint = p
        .endpoint
        .as_ref()
        .ok_or(PreparationError::InternalInvariant)?;
    w.raw("<ram:URIUniversalCommunication>")?;
    attributed!(
        w,
        "ram:URIID",
        "schemeID",
        &endpoint.scheme,
        &endpoint.value
    );
    w.raw("</ram:URIUniversalCommunication>")?;
    if let Some(value) = &p.vat_identifier {
        tax_id(w, value, "VA")?;
    }
    if let Some(value) = fc {
        tax_id(w, value, "FC")?;
    }
    Ok(())
}

fn contact<S: Sink>(w: &mut Writer<'_, S>, value: &Contact) -> Result<()> {
    w.raw("<ram:DefinedTradeContact>")?;
    match &value.point {
        Some(ContactPoint::Person(value)) => text!(w, "ram:PersonName", value),
        Some(ContactPoint::Department(value)) => text!(w, "ram:DepartmentName", value),
        None => {}
    }
    if let Some(value) = &value.telephone {
        w.raw("<ram:TelephoneUniversalCommunication>")?;
        text!(w, "ram:CompleteNumber", value);
        w.raw("</ram:TelephoneUniversalCommunication>")?;
    }
    if let Some(value) = &value.email {
        w.raw("<ram:EmailURIUniversalCommunication>")?;
        text!(w, "ram:URIID", value);
        w.raw("</ram:EmailURIUniversalCommunication>")?;
    }
    w.raw("</ram:DefinedTradeContact>")
}

fn address<S: Sink>(w: &mut Writer<'_, S>, a: &PostalAddress) -> Result<()> {
    w.raw("<ram:PostalTradeAddress>")?;
    optional!(w, "ram:PostcodeCode", a.postcode);
    optional!(w, "ram:LineOne", a.line_one);
    optional!(w, "ram:LineTwo", a.line_two);
    optional!(w, "ram:LineThree", a.line_three);
    optional!(w, "ram:CityName", a.city);
    text!(w, "ram:CountryID", &a.country);
    optional!(w, "ram:CountrySubDivisionName", a.country_subdivision);
    w.raw("</ram:PostalTradeAddress>")
}

#[cfg(test)]
#[path = "../tests/encoding/tests.rs"]
mod tests;
