use crate::UniffiCustomTypeConverter;
use chrono::{DateTime, NaiveDate, NaiveTime, TimeZone, Utc};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use std::io::Read;
use std::str::FromStr;
use std::time::SystemTime;
use yaserde::__xml::attribute::OwnedAttribute;
use yaserde::__xml::name::OwnedName;
use yaserde::__xml::namespace::Namespace;
use yaserde::__xml::reader::XmlEvent as ReaderXmlEvent;
use yaserde::__xml::writer::XmlEvent;
use yaserde::de::Deserializer;
use yaserde::ser::Serializer;
use yaserde::{YaDeserialize, YaSerialize};
use yaserde_derive::{YaDeserialize, YaSerialize};

#[derive(Debug)]
pub struct Iso8601Date(NaiveDate);
uniffi::custom_type!(Iso8601Date, SystemTime);
impl UniffiCustomTypeConverter for Iso8601Date {
    type Builtin = SystemTime;

    fn into_custom(val: Self::Builtin) -> uniffi::Result<Self> {
        let naive_date: DateTime<Utc> = val.into();
        Ok(Iso8601Date(naive_date.naive_utc().date()))
    }

    fn from_custom(obj: Self) -> Self::Builtin {
        let naive_date_time = obj.0.and_time(NaiveTime::default());
        let utc_date_time: DateTime<Utc> = Utc.from_utc_datetime(&naive_date_time);
        utc_date_time.into()
    }
}

impl YaSerialize for Iso8601Date {
    fn serialize<W: std::io::Write>(&self, writer: &mut Serializer<W>) -> Result<(), String> {
        let start_event_name = writer
            .get_start_event_name()
            .ok_or("no start_event_name provided")?;
        let el = XmlEvent::start_element(start_event_name.as_str());
        writer.write(el).map_err(|e| e.to_string())?;
        writer
            .write(XmlEvent::characters(&self.0.format("%Y-%m-%d").to_string()))
            .expect("unable to serialize Date");
        writer
            .write(XmlEvent::end_element())
            .expect("unable to serialize Date EndElement");
        Ok(())
    }

    fn serialize_attributes(
        &self,
        _: Vec<OwnedAttribute>,
        _: Namespace,
    ) -> Result<(Vec<OwnedAttribute>, Namespace), String> {
        unreachable!("Date does not have attributes");
    }
}
impl YaDeserialize for Iso8601Date {
    fn deserialize<R: Read>(reader: &mut Deserializer<R>) -> Result<Self, String> {
        loop {
            match reader.next_event()? {
                ReaderXmlEvent::StartElement { .. } => {}
                ReaderXmlEvent::Characters(text) => {
                    let naive_date = NaiveDate::parse_from_str(&text, "%Y-%m-%d")
                        .map_err(|e| format!("Couldnt parse date: {}", e))?;
                    return Ok(Iso8601Date(naive_date));
                }
                other_event => {
                    return Err(format!("Unexpected event {:?}", other_event));
                }
            }
        }
    }
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
default_namespace = "",
prefix = "",
rename = "Invoice",
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct UblInvoice {
    #[yaserde(rename = "UBLVersionID", prefix = "cbc")]
    pub cbc_ubl_version_id: Option<String>,
    #[yaserde(rename = "CustomizationID", prefix = "cbc")]
    pub cbc_customization_id: Option<String>,
    #[yaserde(rename = "ProfileID", prefix = "cbc")]
    pub cbc_profile_id: Option<String>,
    #[yaserde(rename = "ID", prefix = "cbc")]
    pub cbc_id: Option<String>,
    #[yaserde(rename = "IssueDate", prefix = "cbc")]
    pub cbc_issue_date: Option<Iso8601Date>,
    #[yaserde(rename = "DueDate", prefix = "cbc")]
    pub cbc_due_date: Option<Iso8601Date>,
    #[yaserde(rename = "InvoiceTypeCode", prefix = "cbc")]
    pub cbc_invoice_type_code: Option<String>,
    #[yaserde(rename = "Note", prefix = "cbc")]
    pub cbc_note: Vec<String>,
    #[yaserde(rename = "TaxPointDate", prefix = "cbc")]
    pub cbc_tax_point_date: Option<Iso8601Date>,
    #[yaserde(rename = "DocumentCurrencyCode", prefix = "cbc")]
    pub cbc_document_currency_code: Option<String>,
    #[yaserde(rename = "TaxCurrencyCode", prefix = "cbc")]
    pub cbc_tax_currency_code: Option<String>,
    #[yaserde(rename = "AccountingCost", prefix = "cbc")]
    pub cbc_accounting_cost: Option<String>,
    #[yaserde(rename = "BuyerReference", prefix = "cbc")]
    pub cbc_buyer_reference: Option<String>,
    #[yaserde(rename = "InvoicePeriod", prefix = "cac")]
    pub cac_invoice_period: Option<Period>,
    #[yaserde(rename = "OrderReference", prefix = "cac")]
    pub cac_order_reference: Option<CacOrderReference>,
    #[yaserde(rename = "BillingReference", prefix = "cac")]
    pub cac_billing_reference: Vec<CacBillingReference>,
    #[yaserde(rename = "DespatchDocumentReference", prefix = "cac")]
    pub cac_despatch_document_reference: Option<CacDocumentReference>,
    #[yaserde(rename = "ReceiptDocumentReference", prefix = "cac")]
    pub cac_receipt_document_reference: Option<CacDocumentReference>,
    #[yaserde(rename = "OriginatorDocumentReference", prefix = "cac")]
    pub cac_originator_document_reference: Option<CacDocumentReference>,
    #[yaserde(rename = "ContractDocumentReference", prefix = "cac")]
    pub cac_contract_document_reference: Option<CacDocumentReference>,
    #[yaserde(rename = "AdditionalDocumentReference", prefix = "cac")]
    pub cac_additional_document_reference: Vec<CacDocumentReference>,
    #[yaserde(rename = "ProjectReference", prefix = "cac")]
    pub cac_project_reference: Option<CacProjectReference>,
    #[yaserde(rename = "AccountingSupplierParty", prefix = "cac")]
    pub cac_accounting_supplier_party: Option<CacAccountingSupplierParty>,
    #[yaserde(rename = "AccountingCustomerParty", prefix = "cac")]
    pub cac_accounting_customer_party: Option<CacAccountingCustomerParty>,
    #[yaserde(rename = "PayeeParty", prefix = "cac")]
    pub cac_payee_party: Option<Party>,
    #[yaserde(rename = "TaxRepresentativeParty", prefix = "cac")]
    pub cac_tax_representative_party: Option<Party>,
    #[yaserde(rename = "Delivery", prefix = "cac")]
    pub cac_delivery: Option<CacDelivery>,
    #[yaserde(rename = "PaymentMeans", prefix = "cac")]
    pub cac_payment_means: Vec<PaymentMeans>,
    #[yaserde(rename = "PaymentTerms", prefix = "cac")]
    pub cac_payment_terms: Option<CacPaymentTerms>,
    #[yaserde(rename = "AllowanceCharge", prefix = "cac")]
    pub cac_allowance_charge: Vec<CacAllowanceCharge>,
    #[yaserde(rename = "PrepaidPayment", prefix = "cac")]
    pub cac_prepaid_payment: Vec<Payment>,
    #[yaserde(rename = "TaxTotal", prefix = "cac")]
    pub cac_tax_total: Vec<CacTaxTotal>,
    #[yaserde(rename = "LegalMonetaryTotal", prefix = "cac")]
    pub cac_legal_monetary_total: Option<LegalMonetaryTotal>,
    #[yaserde(rename = "InvoiceLine", prefix = "cac")]
    pub cac_invoice_line: Vec<InvoiceLine>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct Payment {
    #[yaserde(rename = "ID", prefix = "cbc")]
    pub cbc_id: Option<Identifier>,
    #[yaserde(rename = "PaidAmount", prefix = "cbc")]
    pub cbc_paid_amount: Option<Amount>,
    #[yaserde(rename = "ReceivedDate", prefix = "cbc")]
    pub cbc_received_date: Option<Iso8601Date>,
    #[yaserde(rename = "PaidDate", prefix = "cbc")]
    pub cbc_paid_date: Option<Iso8601Date>,
    #[yaserde(rename = "PaidTime", prefix = "cbc")]
    pub cbc_paid_time: Option<String>,
    #[yaserde(rename = "InstructionID", prefix = "cbc")]
    pub cbc_instruction_id: Option<Identifier>,
}

#[derive(uniffi::Record, Debug)]
pub struct MoneyDecimal {
    pub as_integer: i64,
    pub scale: u32,
}
impl MoneyDecimal {
    pub fn as_decimal(&self) -> f64 {
        self.as_integer as f64 / 10f64.powi(self.scale as i32)
    }
}

uniffi::custom_type!(Decimal, MoneyDecimal);
impl UniffiCustomTypeConverter for Decimal {
    type Builtin = MoneyDecimal;

    fn into_custom(val: Self::Builtin) -> uniffi::Result<Self> {
        Ok(Decimal::new(val.as_integer, val.scale))
    }

    fn from_custom(obj: Self) -> Self::Builtin {
        MoneyDecimal {
            as_integer: obj.to_i64().unwrap(),
            scale: obj.scale(),
        }
    }
}
#[derive(uniffi::Record, Debug)]
pub struct Amount {
    pub value: Option<Decimal>,
    pub currency_id: Option<String>,
}

impl YaSerialize for Amount {
    fn serialize<W: std::io::Write>(&self, writer: &mut Serializer<W>) -> Result<(), String> {
        let start_event_name = writer
            .get_start_event_name()
            .ok_or("no start_event_name provided")?;
        let el = self
            .currency_id
            .as_ref()
            .map(|currency_id| {
                let el = XmlEvent::start_element(start_event_name.as_str());
                el.attr("currencyID", currency_id.as_str())
            })
            .unwrap_or(XmlEvent::start_element(start_event_name.as_str()));
        writer.write(el).map_err(|e| e.to_string())?;
        if let Some(v) = self.value {
            writer
                .write(XmlEvent::characters(&v.to_string()))
                .expect("unable to serialize Amount")
        }
        writer
            .write(XmlEvent::end_element())
            .expect("unable to serialize Amount EndElement");
        Ok(())
    }

    fn serialize_attributes(
        &self,
        _: Vec<OwnedAttribute>,
        namespace: Namespace,
    ) -> Result<(Vec<OwnedAttribute>, Namespace), String> {
        Ok((
            vec![OwnedAttribute {
                name: OwnedName::local("currencyID"),
                value: self
                    .currency_id
                    .as_ref()
                    .unwrap_or(&String::new())
                    .to_string(),
            }],
            namespace,
        ))
    }
}

impl YaDeserialize for Amount {
    fn deserialize<R: Read>(reader: &mut Deserializer<R>) -> Result<Self, String> {
        let mut currency_id = None;
        loop {
            match reader.next_event()? {
                ReaderXmlEvent::StartElement { attributes, .. } => {
                    for attr in attributes {
                        if attr.name.local_name == "currencyID" {
                            currency_id = Some(attr.value);
                            break;
                        }
                    }
                }
                ReaderXmlEvent::Characters(text) => {
                    let value = Decimal::from_str(&text)
                        .map_err(|e| format!("Couldnt parse amount value: {}", e))?;
                    return Ok(Amount {
                        value: Some(value),
                        currency_id,
                    });
                }
                other_event => {
                    return Err(format!("Unexpected event {:?}", other_event));
                }
            }
        }
    }
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct Period {
    #[yaserde(rename = "StartDate", prefix = "cbc")]
    pub cbc_start_date: Option<Iso8601Date>,
    #[yaserde(rename = "StartTime", prefix = "cbc")]
    pub cbc_start_time: Option<String>,
    #[yaserde(rename = "EndDate", prefix = "cbc")]
    pub cbc_end_date: Option<Iso8601Date>,
    #[yaserde(rename = "EndTime", prefix = "cbc")]
    pub cbc_end_time: Option<String>,
    #[yaserde(rename = "DurationMeasure", prefix = "cbc")]
    pub cbc_duration_measure: Option<String>,
    #[yaserde(rename = "DescriptionCode", prefix = "cbc")]
    pub cbc_description_code: Option<String>,
    #[yaserde(rename = "Description", prefix = "cbc")]
    pub cbc_description: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct Identifier {
    #[yaserde(text = true)]
    pub id: Option<String>,
    #[yaserde(rename = "schemeID", attribute = true)]
    pub scheme_id: Option<String>,
    #[yaserde(rename = "schemeName", attribute = true)]
    pub scheme_name: Option<String>,
    #[yaserde(rename = "schemeAgencyID", attribute = true)]
    pub scheme_agency_id: Option<String>,
    #[yaserde(rename = "schemeAgencyName", attribute = true)]
    pub scheme_agency_name: Option<String>,
    #[yaserde(rename = "schemeVersionID", attribute = true)]
    pub scheme_version_id: Option<String>,
    #[yaserde(rename = "schemeDataURI", attribute = true)]
    pub scheme_data_uri: Option<String>,
    #[yaserde(rename = "schemeURI", attribute = true)]
    pub scheme_uri: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct Party {
    #[yaserde(rename = "MarkCareIndicator", prefix = "cbc")]
    pub cbc_mark_care_indicator: Option<bool>,
    #[yaserde(rename = "MarkAttentionIndicator", prefix = "cbc")]
    pub cbc_mark_attention_indicator: Option<bool>,
    #[yaserde(rename = "WebsiteURI", prefix = "cbc")]
    pub cbc_website_uri: Option<String>,
    #[yaserde(rename = "LogoReferenceID", prefix = "cbc")]
    pub cbc_logo_reference_id: Option<String>,
    #[yaserde(rename = "EndpointID", prefix = "cbc")]
    pub cbc_endpoint_id: Option<Identifier>,
    #[yaserde(rename = "IndustryClassificationCode", prefix = "cbc")]
    pub cbc_industry_classification_code: Option<String>,
    #[yaserde(rename = "PartyIdentification", prefix = "cac")]
    pub cac_party_identification: Vec<PartyIdentification>,
    #[yaserde(rename = "PartyName", prefix = "cac")]
    pub cac_party_name: Vec<PartyName>,
    #[yaserde(rename = "Language", prefix = "cac")]
    pub cac_language: Option<Language>,
    #[yaserde(rename = "PostalAddress", prefix = "cac")]
    pub cac_postal_address: Option<Address>,
    #[yaserde(rename = "PhysicalLocation", prefix = "cac")]
    pub cac_physical_location: Option<Location>,
    #[yaserde(rename = "PartyTaxScheme", prefix = "cac")]
    pub cac_party_tax_scheme: Vec<PartyTaxScheme>,
    #[yaserde(rename = "PartyLegalEntity", prefix = "cac")]
    pub cac_party_legal_entity: Vec<PartyLegalEntity>,
    #[yaserde(rename = "Contact", prefix = "cac")]
    pub cac_contact: Option<Contact>,
    #[yaserde(rename = "Person", prefix = "cac")]
    pub cac_person: Vec<CacPerson>,
    #[yaserde(rename = "PowerOfAttorney", prefix = "cac")]
    pub cac_power_of_attorney: Vec<CacPowerOfAttorney>,
    #[yaserde(rename = "FinancialAccount", prefix = "cac")]
    pub cac_financial_account: Option<FinancialAccount>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacPerson {
    #[yaserde(rename = "ID", prefix = "cbc")]
    pub cbc_id: Option<Identifier>,
    #[yaserde(rename = "FirstName", prefix = "cbc")]
    pub cbc_first_name: Option<String>,
    #[yaserde(rename = "FamilyName", prefix = "cbc")]
    pub cbc_family_name: Option<String>,
    #[yaserde(rename = "Title", prefix = "cbc")]
    pub cbc_title: Option<String>,
    #[yaserde(rename = "MiddleName", prefix = "cbc")]
    pub cbc_middle_name: Option<String>,
    #[yaserde(rename = "OtherName", prefix = "cbc")]
    pub cbc_other_name: Option<String>,
    #[yaserde(rename = "NameSuffix", prefix = "cbc")]
    pub cbc_name_suffix: Option<String>,
    #[yaserde(rename = "JobTitle", prefix = "cbc")]
    pub cbc_job_title: Option<String>,
    #[yaserde(rename = "NationalityID", prefix = "cbc")]
    pub cbc_nationality_id: Option<Identifier>,
    #[yaserde(rename = "GenderCode", prefix = "cbc")]
    pub cbc_gender_code: Option<String>,
    #[yaserde(rename = "BirthDate", prefix = "cbc")]
    pub cbc_birth_date: Option<Iso8601Date>,
    #[yaserde(rename = "BirthplaceName", prefix = "cbc")]
    pub cbc_birthplace_name: Option<String>,
    #[yaserde(rename = "OrganizationDepartment", prefix = "cbc")]
    pub cbc_organization_department: Option<String>,
    #[yaserde(rename = "Contact", prefix = "cac")]
    pub cac_contact: Option<Contact>,
    #[yaserde(rename = "FinancialAccount", prefix = "cac")]
    pub cac_financial_account: Option<FinancialAccount>,
    #[yaserde(rename = "IdentityDocumentReference", prefix = "cac")]
    pub cac_identity_document_reference: Vec<CacDocumentReference>,
    #[yaserde(rename = "ResidenceAddress", prefix = "cac")]
    pub cac_residence_address: Option<Address>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacPowerOfAttorney {
    #[yaserde(rename = "ID", prefix = "cbc")]
    pub cbc_id: Option<String>,
    #[yaserde(rename = "IssueDate", prefix = "cbc")]
    pub cbc_issue_date: Option<Iso8601Date>,
    #[yaserde(rename = "IssueTime", prefix = "cbc")]
    pub cbc_issue_time: Option<String>,
    #[yaserde(rename = "Description", prefix = "cbc")]
    pub cbc_description: Vec<String>,
    #[yaserde(rename = "NotaryParty", prefix = "cac")]
    pub cac_notary_party: Option<Party>,
    #[yaserde(rename = "AgentParty", prefix = "cac")]
    pub cac_agent_party: Option<Party>,
    #[yaserde(rename = "WitnessParty", prefix = "cac")]
    pub cac_witness_party: Vec<Party>,
    #[yaserde(rename = "MandateDocumentReference", prefix = "cac")]
    pub cac_mandate_document_reference: Vec<CacDocumentReference>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct PartyTaxScheme {
    #[yaserde(rename = "RegistrationName", prefix = "cbc")]
    pub cbc_registration_name: Option<String>,
    #[yaserde(rename = "CompanyID", prefix = "cbc")]
    pub cbc_company_id: Option<String>,
    #[yaserde(rename = "TaxLevelCode", prefix = "cbc")]
    pub cbc_tax_level_code: Option<String>,
    #[yaserde(rename = "ExemptionReasonCode", prefix = "cbc")]
    pub cbc_exemption_reason_code: Option<String>,
    #[yaserde(rename = "ExemptionReason", prefix = "cbc")]
    pub cbc_exemption_reason: Vec<String>,
    #[yaserde(rename = "RegistrationAddress", prefix = "cac")]
    pub cac_registration_address: Option<Address>,
    #[yaserde(rename = "TaxScheme", prefix = "cac")]
    pub cac_tax_scheme: Option<TaxScheme>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct Location {
    #[yaserde(rename = "ID", prefix = "cbc")]
    pub cbc_id: Option<Identifier>,
    #[yaserde(rename = "Description", prefix = "cbc")]
    pub cbc_description: Vec<String>,
    #[yaserde(rename = "Conditions", prefix = "cbc")]
    pub cbc_conditions: Vec<String>,
    #[yaserde(rename = "CountrySubentity", prefix = "cbc")]
    pub cbc_country_subentity: Option<String>,
    #[yaserde(rename = "CountrySubentityCode", prefix = "cbc")]
    pub cbc_country_subentity_code: Option<String>,
    #[yaserde(rename = "LocationTypeCode", prefix = "cbc")]
    pub cbc_location_type_code: Option<String>,
    #[yaserde(rename = "InformationURI", prefix = "cbc")]
    pub cbc_information_uri: Option<String>,
    #[yaserde(rename = "Name", prefix = "cbc")]
    pub cbc_name: Option<String>,
    #[yaserde(rename = "ValidityPeriod", prefix = "cac")]
    pub cac_validity_period: Vec<Period>,
    #[yaserde(rename = "Address", prefix = "cac")]
    pub cac_address: Option<Address>,
    #[yaserde(rename = "SubsidiaryLocation", prefix = "cac")]
    pub cac_subsidiary_location: Vec<Location>,
    #[yaserde(rename = "LocationCoordinate", prefix = "cac")]
    pub cac_location_coordinate: Vec<LocationCoordinate>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct Address {
    #[yaserde(rename = "ID", prefix = "cbc")]
    pub cbc_id: Option<String>,
    #[yaserde(rename = "Postbox", prefix = "cbc")]
    pub cbc_postbox: Option<String>,
    #[yaserde(rename = "Floor", prefix = "cbc")]
    pub cbc_floor: Option<String>,
    #[yaserde(rename = "Room", prefix = "cbc")]
    pub cbc_room: Option<String>,
    #[yaserde(rename = "StreetName", prefix = "cbc")]
    pub cbc_street_name: Option<String>,
    #[yaserde(rename = "AdditionalStreetName", prefix = "cbc")]
    pub cbc_additional_street_name: Option<String>,
    #[yaserde(rename = "BlockName", prefix = "cbc")]
    pub cbc_block_name: Option<String>,
    #[yaserde(rename = "BuildingName", prefix = "cbc")]
    pub cbc_building_name: Option<String>,
    #[yaserde(rename = "BuildingNumber", prefix = "cbc")]
    pub cbc_building_number: Option<String>,
    #[yaserde(rename = "InhouseMail", prefix = "cbc")]
    pub cbc_inhouse_mail: Option<String>,
    #[yaserde(rename = "Department", prefix = "cbc")]
    pub cbc_department: Option<String>,
    #[yaserde(rename = "MarkAttention", prefix = "cbc")]
    pub cbc_mark_attention: Option<String>,
    #[yaserde(rename = "MarkCare", prefix = "cbc")]
    pub cbc_mark_care: Option<String>,
    #[yaserde(rename = "PlotIdentification", prefix = "cbc")]
    pub cbc_plot_identification: Option<String>,
    #[yaserde(rename = "CitySubdivisionName", prefix = "cbc")]
    pub cbc_city_subdivision_name: Option<String>,
    #[yaserde(rename = "CityName", prefix = "cbc")]
    pub cbc_city_name: Option<String>,
    #[yaserde(rename = "PostalZone", prefix = "cbc")]
    pub cbc_postal_zone: Option<String>,
    #[yaserde(rename = "CountrySubentity", prefix = "cbc")]
    pub cbc_country_subentity: Option<String>,
    #[yaserde(rename = "CountrySubentityCode", prefix = "cbc")]
    pub cbc_country_subentity_code: Option<String>,
    #[yaserde(rename = "Region", prefix = "cbc")]
    pub cbc_region: Option<String>,
    #[yaserde(rename = "District", prefix = "cbc")]
    pub cbc_district: Option<String>,
    #[yaserde(rename = "TimezoneOffset", prefix = "cbc")]
    pub cbc_timezone_offset: Option<String>,
    #[yaserde(rename = "AddressLine", prefix = "cac")]
    pub cac_address_line: Vec<CacAddressLine>,
    #[yaserde(rename = "Country", prefix = "cac")]
    pub cac_country: Option<CacCountry>,
    #[yaserde(rename = "LocationCoordinate", prefix = "cac")]
    pub cac_location_coordinate: Vec<LocationCoordinate>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct LocationCoordinate {
    #[yaserde(rename = "CoordinateSystemCode", prefix = "cbc")]
    pub cbc_coordinate_system_code: Option<String>,
    #[yaserde(rename = "LatitudeDegreesMeasure", prefix = "cbc")]
    pub cbc_latitude_degrees_measure: Option<String>,
    #[yaserde(rename = "LatitudeMinutesMeasure", prefix = "cbc")]
    pub cbc_latitude_minutes_measure: Option<String>,
    #[yaserde(rename = "LatitudeDirectionCode", prefix = "cbc")]
    pub cbc_latitude_direction_code: Option<String>,
    #[yaserde(rename = "LongitudeDegreesMeasure", prefix = "cbc")]
    pub cbc_longitude_degrees_measure: Option<String>,
    #[yaserde(rename = "LongitudeMinutesMeasure", prefix = "cbc")]
    pub cbc_longitude_minutes_measure: Option<String>,
    #[yaserde(rename = "LongitudeDirectionCode", prefix = "cbc")]
    pub cbc_longitude_direction_code: Option<String>,
    #[yaserde(rename = "AltitudeMeasure", prefix = "cbc")]
    pub cbc_altitude_measure: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacCountry {
    #[yaserde(rename = "IdentificationCode", prefix = "cbc")]
    pub cbc_identification_code: Option<String>,
    #[yaserde(rename = "Name", prefix = "cbc")]
    pub cbc_name: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacAddressLine {
    #[yaserde(rename = "Line", prefix = "cbc")]
    pub cbc_line: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct PartyIdentification {
    #[yaserde(rename = "ID", prefix = "cbc")]
    pub cbc_id: Option<Identifier>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct Language {
    #[yaserde(rename = "ID", prefix = "cbc")]
    pub cbc_id: Option<String>,
    #[yaserde(rename = "Name", prefix = "cbc")]
    pub cbc_name: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct PartyName {
    #[yaserde(rename = "Name", prefix = "cbc")]
    pub cbc_name: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacOrderReference {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ID", prefix = "cbc")]
    pub cbc_id: Option<String>,
    #[yaserde(rename = "SalesOrderID", prefix = "cbc")]
    pub cbc_sales_order_id: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacBillingReference {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "InvoiceDocumentReference", prefix = "cac")]
    pub cac_invoice_document_reference: Option<CacDocumentReference>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacAttachment {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "EmbeddedDocumentBinaryObject", prefix = "cbc")]
    pub cbc_embedded_document_binary_object: Option<CbcEmbeddedDocumentBinaryObject>,
    #[yaserde(rename = "ExternalReference", prefix = "cac")]
    pub cac_external_reference: Option<CacExternalReference>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CbcEmbeddedDocumentBinaryObject {
    #[yaserde(rename = "mimeCode", attribute = true)]
    pub mime_code: Option<String>,
    #[yaserde(rename = "filename", attribute = true)]
    pub filename: Option<String>,
    #[yaserde(text = true)]
    pub text: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacExternalReference {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "URI", prefix = "cbc")]
    pub cbc_uri: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacProjectReference {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ID", prefix = "cbc")]
    pub cbc_id: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacAccountingSupplierParty {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "Party", prefix = "cac")]
    pub cac_party: Option<Party>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacAccountingSupplierPartyCacPartyCacPostalAddressCacCountry {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "IdentificationCode", prefix = "cbc")]
    pub cbc_identification_code: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacAccountingSupplierPartyCacPartyCacPartyTaxScheme {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "CompanyID", prefix = "cbc")]
    pub cbc_company_id: Option<String>,
    #[yaserde(rename = "TaxScheme", prefix = "cac")]
    pub cac_tax_scheme: Option<CacAccountingSupplierPartyCacPartyCacPartyTaxSchemeCacTaxScheme>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacAccountingSupplierPartyCacPartyCacPartyTaxSchemeCacTaxScheme {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ID", prefix = "cbc")]
    pub cbc_id: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct PartyLegalEntity {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "RegistrationName", prefix = "cbc")]
    pub cbc_registration_name: Option<String>,
    #[yaserde(rename = "CompanyID", prefix = "cbc")]
    pub cbc_company_id: Option<CompanyId>,
    #[yaserde(rename = "CompanyLegalForm", prefix = "cbc")]
    pub cbc_company_legal_form: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CompanyId {
    #[yaserde(rename = "schemeID", attribute = true)]
    pub scheme_id: Option<String>,
    #[yaserde(text = true)]
    pub text: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct Contact {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "Name", prefix = "cbc")]
    pub cbc_name: Option<String>,
    #[yaserde(rename = "Telephone", prefix = "cbc")]
    pub cbc_telephone: Option<String>,
    #[yaserde(rename = "ElectronicMail", prefix = "cbc")]
    pub cbc_electronic_mail: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacAccountingCustomerParty {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "Party", prefix = "cac")]
    pub cac_party: Option<Party>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct InvoiceCacTaxRepresentativePartyCacPartyTaxSchemeCacTaxScheme {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ID", prefix = "cbc")]
    pub cbc_id: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacDelivery {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ActualDeliveryDate", prefix = "cbc")]
    pub cbc_actual_delivery_date: Option<Iso8601Date>,
    #[yaserde(rename = "DeliveryLocation", prefix = "cac")]
    pub cac_delivery_location: Option<Location>,
    #[yaserde(rename = "DeliveryParty", prefix = "cac")]
    pub cac_delivery_party: Option<Party>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacDeliveryLocation {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ID", prefix = "cbc")]
    pub cbc_id: Option<String>,
    #[yaserde(rename = "Address", prefix = "cac")]
    pub cac_address: Option<CacAddress>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacAddress {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "StreetName", prefix = "cbc")]
    pub cbc_street_name: Option<String>,
    #[yaserde(rename = "AdditionalStreetName", prefix = "cbc")]
    pub cbc_additional_street_name: Option<String>,
    #[yaserde(rename = "CityName", prefix = "cbc")]
    pub cbc_city_name: Option<String>,
    #[yaserde(rename = "PostalZone", prefix = "cbc")]
    pub cbc_postal_zone: Option<String>,
    #[yaserde(rename = "CountrySubentity", prefix = "cbc")]
    pub cbc_country_subentity: Option<String>,
    #[yaserde(rename = "AddressLine", prefix = "cac")]
    pub cac_address_line: Option<CacAddressLine>,
    #[yaserde(rename = "Country", prefix = "cac")]
    pub cac_country: Option<CacCountry>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacDeliveryParty {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "PartyName", prefix = "cac")]
    pub cac_party_name: Option<CacDeliveryCacDeliveryPartyCacPartyName>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacDeliveryCacDeliveryPartyCacPartyName {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "Name", prefix = "cbc")]
    pub cbc_name: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct PaymentMeans {
    #[yaserde(rename = "ID", prefix = "cbc")]
    pub cbc_id: Option<Identifier>,
    #[yaserde(rename = "PaymentMeansCode", prefix = "cbc")]
    pub cbc_payment_means_code: Option<CbcPaymentMeansCode>,
    #[yaserde(rename = "PaymentDueDate", prefix = "cbc")]
    pub cbc_payment_due_date: Option<Iso8601Date>,
    #[yaserde(rename = "PaymentChannelCode", prefix = "cbc")]
    pub cbc_payment_channel_code: Option<String>,
    #[yaserde(rename = "InstructionID", prefix = "cbc")]
    pub cbc_instruction_id: Option<Identifier>,
    #[yaserde(rename = "PaymentID", prefix = "cbc")]
    pub cbc_payment_id: Option<Identifier>,
    #[yaserde(rename = "CardAccount", prefix = "cac")]
    pub cac_card_account: Option<CardAccount>,
    #[yaserde(rename = "PayerFinancialAccount", prefix = "cac")]
    pub cac_payer_financial_account: Option<FinancialAccount>,
    #[yaserde(rename = "PayeeFinancialAccount", prefix = "cac")]
    pub cac_payee_financial_account: Option<FinancialAccount>,
    #[yaserde(rename = "CreditAccount", prefix = "cac")]
    pub cac_credit_account: Option<FinancialAccount>,
    #[yaserde(rename = "PaymentMandate", prefix = "cac")]
    pub cac_payment_mandate: Option<PaymentMandate>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct PaymentMandate {
    #[yaserde(rename = "ID", prefix = "cbc")]
    pub cbc_id: Option<Identifier>,
    #[yaserde(rename = "MandateTypeCode", prefix = "cbc")]
    pub cbc_mandate_type_code: Option<String>,
    #[yaserde(rename = "MaximumPaymentInstructionsNumeric", prefix = "cbc")]
    pub cbc_maximum_payment_instructions_numeric: Option<i32>,
    #[yaserde(rename = "MaximumPaidAmount", prefix = "cbc")]
    pub cbc_maximum_paid_amount: Option<String>,
    #[yaserde(rename = "SignatureID", prefix = "cbc")]
    pub cbc_signature_id: Option<Identifier>,
    #[yaserde(rename = "PayerParty", prefix = "cac")]
    pub cac_payer_party: Option<Party>,
    #[yaserde(rename = "PayerFinancialAccount", prefix = "cac")]
    pub cac_payer_financial_account: Option<FinancialAccount>,
    #[yaserde(rename = "ValidityPeriod", prefix = "cac")]
    pub cac_validity_period: Option<Period>,
    #[yaserde(rename = "PaymentReversalPeriod", prefix = "cac")]
    pub cac_payment_reversal_period: Option<Period>,
    #[yaserde(rename = "Clause", prefix = "cac")]
    pub cac_clause: Vec<Clause>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct Clause {
    #[yaserde(rename = "ID", prefix = "cbc")]
    pub cbc_id: Option<Identifier>,
    #[yaserde(rename = "Content", prefix = "cbc")]
    pub cbc_content: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CardAccount {
    #[yaserde(rename = "PrimaryAccountNumberID", prefix = "cbc")]
    pub primary_account_number_id: Option<Identifier>,
    #[yaserde(rename = "NetworkID", prefix = "cbc")]
    pub cbc_network_id: Option<Identifier>,
    #[yaserde(rename = "CardTypeCode", prefix = "cbc")]
    pub cbc_card_type_code: Option<String>,
    #[yaserde(rename = "ValidityStartDate", prefix = "cbc")]
    pub cbc_validity_start_date: Option<Iso8601Date>,
    #[yaserde(rename = "ExpiryDate", prefix = "cbc")]
    pub cbc_expiry_date: Option<Iso8601Date>,
    #[yaserde(rename = "IssuerID", prefix = "cbc")]
    pub cbc_issuer_id: Option<Identifier>,
    #[yaserde(rename = "IssueNumberID", prefix = "cbc")]
    pub cbc_issue_number_id: Option<Identifier>,
    #[yaserde(rename = "CV2ID", prefix = "cbc")]
    pub cbc_cv2_id: Option<Identifier>,
    #[yaserde(rename = "CardChipCode", prefix = "cbc")]
    pub cbc_card_chip_code: Option<String>,
    #[yaserde(rename = "ChipApplicationID", prefix = "cbc")]
    pub cbc_chip_application_id: Option<Identifier>,
    #[yaserde(rename = "HolderName", prefix = "cbc")]
    pub cbc_holder_name: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CbcPaymentMeansCode {
    #[yaserde(rename = "name", attribute = true)]
    pub name: Option<String>,
    #[yaserde(text = true)]
    pub text: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct FinancialAccount {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ID", prefix = "cbc")]
    pub cbc_id: Option<String>,
    #[yaserde(rename = "Name", prefix = "cbc")]
    pub cbc_name: Option<String>,
    #[yaserde(rename = "FinancialInstitutionBranch", prefix = "cac")]
    pub cac_financial_institution_branch: Option<CacFinancialInstitutionBranch>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacFinancialInstitutionBranch {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ID", prefix = "cbc")]
    pub cbc_id: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacPaymentTerms {
    #[yaserde(rename = "Note", prefix = "cbc")]
    pub cbc_note: Option<String>,
    #[yaserde(rename = "PaymentDueDate", prefix = "cbc")]
    pub cbc_payment_due_date: Option<Iso8601Date>,
    #[yaserde(rename = "SettlementPeriod", prefix = "cac")]
    pub cac_settlement_period: Option<Period>,
    #[yaserde(rename = "PenaltyPeriod", prefix = "cac")]
    pub cac_penalty_period: Option<Period>,
    #[yaserde(rename = "ExchangeRate", prefix = "cac")]
    pub cac_exchange_rate: Option<ExchangeRate>,
    #[yaserde(rename = "ValidityPeriod", prefix = "cac")]
    pub cac_validity_period: Option<Period>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct ExchangeRate {
    #[yaserde(rename = "SourceCurrencyCode", prefix = "cbc")]
    pub cbc_source_currency_code: Option<String>,
    #[yaserde(rename = "SourceCurrencyBaseRate", prefix = "cbc")]
    pub cbc_source_currency_base_rate: Option<String>,
    #[yaserde(rename = "TargetCurrencyCode", prefix = "cbc")]
    pub cbc_target_currency_code: Option<String>,
    #[yaserde(rename = "TargetCurrencyBaseRate", prefix = "cbc")]
    pub cbc_target_currency_base_rate: Option<String>,
    #[yaserde(rename = "ExchangeMarketID", prefix = "cbc")]
    pub cbc_exchange_market_id: Option<String>,
    #[yaserde(rename = "CalculationRate", prefix = "cbc")]
    pub cbc_calculation_rate: Option<String>,
    #[yaserde(rename = "MathematicOperatorCode", prefix = "cbc")]
    pub cbc_mathematic_operator_code: Option<String>,
    #[yaserde(rename = "Date", prefix = "cbc")]
    pub cbc_date: Option<Iso8601Date>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacAllowanceCharge {
    #[yaserde(rename = "ID", prefix = "cbc")]
    pub cbc_id: Option<Identifier>,
    #[yaserde(rename = "ChargeIndicator", prefix = "cbc")]
    pub charge_indicator: Option<bool>,
    #[yaserde(rename = "AllowanceChargeReasonCode", prefix = "cbc")]
    pub cbc_allowance_charge_reason_code: Option<String>,
    #[yaserde(rename = "AllowanceChargeReason", prefix = "cbc")]
    pub cbc_allowance_charge_reason: Option<String>,
    #[yaserde(rename = "MultiplierFactorNumeric", prefix = "cbc")]
    pub cbc_multiplier_factor_numeric: Option<f32>,
    #[yaserde(rename = "PrepaidIndicator", prefix = "cbc")]
    pub cbc_prepaid_indicator: Option<bool>,
    #[yaserde(rename = "SequenceNumeric", prefix = "cbc")]
    pub cbc_sequence_numeric: Option<i32>,
    #[yaserde(rename = "Amount", prefix = "cbc")]
    pub amount: Option<Amount>,
    #[yaserde(rename = "BaseAmount", prefix = "cbc")]
    pub cbc_base_amount: Option<Amount>,
    #[yaserde(rename = "AccountingCostCode", prefix = "cbc")]
    pub cbc_accounting_cost_code: Option<String>,
    #[yaserde(rename = "AccountingCost", prefix = "cbc")]
    pub cbc_accounting_cost: Option<String>,
    #[yaserde(rename = "PerUnitAmount", prefix = "cbc")]
    pub cbc_per_unit_amount: Option<Amount>,
    #[yaserde(rename = "TaxCategory", prefix = "cac")]
    pub cac_tax_category: Vec<TaxCategory>,
    #[yaserde(rename = "TaxTotal", prefix = "cac")]
    pub cac_tax_total: Option<CacTaxTotal>,
    #[yaserde(rename = "PaymentMeans", prefix = "cac")]
    pub cac_payment_means: Vec<PaymentMeans>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacTaxTotal {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "TaxAmount", prefix = "cbc")]
    pub cbc_tax_amount: Option<Amount>,
    #[yaserde(rename = "TaxSubtotal", prefix = "cac")]
    pub cac_tax_subtotal: Vec<CacTaxSubtotal>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacTaxSubtotal {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "TaxableAmount", prefix = "cbc")]
    pub cbc_taxable_amount: Option<Amount>,
    #[yaserde(rename = "TaxAmount", prefix = "cbc")]
    pub cbc_tax_amount: Option<Amount>,
    #[yaserde(rename = "TaxCategory", prefix = "cac")]
    pub cac_tax_category: Option<TaxCategory>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct TaxCategory {
    #[yaserde(rename = "ID", prefix = "cbc")]
    pub cbc_id: Option<String>,
    #[yaserde(rename = "Name", prefix = "cbc")]
    pub cbc_name: Option<String>,
    #[yaserde(rename = "Percent", prefix = "cbc")]
    pub cbc_percent: Option<String>,
    #[yaserde(rename = "BaseUnitMeasure", prefix = "cbc")]
    pub cbc_base_unit_measure: Option<String>,
    #[yaserde(rename = "PerUnitAmount", prefix = "cbc")]
    pub cbc_per_unit_amount: Option<Amount>,
    #[yaserde(rename = "TaxExemptionReasonCode", prefix = "cbc")]
    pub cbc_tax_exemption_reason_code: Option<String>,
    #[yaserde(rename = "TaxExemptionReason", prefix = "cbc")]
    pub cbc_tax_exemption_reason: Vec<String>,
    #[yaserde(rename = "TierRange", prefix = "cbc")]
    pub cbc_tier_range: Option<String>,
    #[yaserde(rename = "TierRatePercent", prefix = "cbc")]
    pub cbc_tier_rate_percent: Option<String>,
    #[yaserde(rename = "TaxScheme", prefix = "cac")]
    pub cac_tax_scheme: Option<TaxScheme>,
}
#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct TaxScheme {
    #[yaserde(rename = "ID", prefix = "cbc")]
    pub id: Option<Identifier>,
    #[yaserde(rename = "Name", prefix = "cbc")]
    pub cbc_name: Option<String>,
    #[yaserde(rename = "TaxTypeCode", prefix = "cbc")]
    pub cbc_tax_type_code: Option<String>,
    #[yaserde(rename = "CurrencyCode", prefix = "cbc")]
    pub cbc_currency_code: Option<String>,
    #[yaserde(rename = "JurisdictionRegionAddress", prefix = "cac")]
    pub cac_jurisdiction_region_address: Vec<Address>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacTaxSubtotalCacTaxCategory {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ID", prefix = "cbc")]
    pub cbc_id: Option<String>,
    #[yaserde(rename = "Percent", prefix = "cbc")]
    pub cbc_percent: Option<String>,
    #[yaserde(rename = "TaxExemptionReasonCode", prefix = "cbc")]
    pub cbc_tax_exemption_reason_code: Option<String>,
    #[yaserde(rename = "TaxExemptionReason", prefix = "cbc")]
    pub cbc_tax_exemption_reason: Option<String>,
    #[yaserde(rename = "TaxScheme", prefix = "cac")]
    pub cac_tax_scheme: Option<CacTaxTotalCacTaxSubtotalCacTaxCategoryCacTaxScheme>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacTaxTotalCacTaxSubtotalCacTaxCategoryCacTaxScheme {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ID", prefix = "cbc")]
    pub cbc_id: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct LegalMonetaryTotal {
    #[yaserde(text = true)]
    pub value: Option<String>,
    #[yaserde(rename = "LineExtensionAmount", prefix = "cbc")]
    pub cbc_line_extension_amount: Option<Amount>,
    #[yaserde(rename = "TaxExclusiveAmount", prefix = "cbc")]
    pub cbc_tax_exclusive_amount: Option<Amount>,
    #[yaserde(rename = "TaxInclusiveAmount", prefix = "cbc")]
    pub cbc_tax_inclusive_amount: Option<Amount>,
    #[yaserde(rename = "AllowanceTotalAmount", prefix = "cbc")]
    pub cbc_allowance_total_amount: Option<Amount>,
    #[yaserde(rename = "ChargeTotalAmount", prefix = "cbc")]
    pub cbc_charge_total_amount: Option<Amount>,
    #[yaserde(rename = "PrepaidAmount", prefix = "cbc")]
    pub cbc_prepaid_amount: Option<Amount>,
    #[yaserde(rename = "PayableRoundingAmount", prefix = "cbc")]
    pub cbc_payable_rounding_amount: Option<Amount>,
    #[yaserde(rename = "PayableAmount", prefix = "cbc")]
    pub payable_amount: Option<Amount>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct InvoiceLine {
    #[yaserde(rename = "ID", prefix = "cbc")]
    pub cbc_id: Option<Identifier>,
    #[yaserde(rename = "Note", prefix = "cbc")]
    pub cbc_note: Option<String>,
    #[yaserde(rename = "InvoicedQuantity", prefix = "cbc")]
    pub cbc_invoiced_quantity: Option<CbcInvoicedQuantity>,
    #[yaserde(rename = "LineExtensionAmount", prefix = "cbc")]
    pub cbc_line_extension_amount: Option<Amount>,
    #[yaserde(rename = "AccountingCost", prefix = "cbc")]
    pub cbc_accounting_cost: Option<String>,
    #[yaserde(rename = "InvoicePeriod", prefix = "cac")]
    pub cac_invoice_period: Option<Period>,
    #[yaserde(rename = "OrderLineReference", prefix = "cac")]
    pub cac_order_line_reference: Option<CacOrderLineReference>,
    #[yaserde(rename = "DocumentReference", prefix = "cac")]
    pub cac_document_reference: Option<CacDocumentReference>,
    #[yaserde(rename = "AllowanceCharge", prefix = "cac")]
    pub cac_allowance_charge: Vec<CacAllowanceCharge>,
    #[yaserde(rename = "Item", prefix = "cac")]
    pub cac_item: Option<CacItem>,
    #[yaserde(rename = "Price", prefix = "cac")]
    pub cac_price: Option<CacPrice>,
    #[yaserde(rename = "SubInvoiceLine", prefix = "cac")]
    pub cac_sub_invoice_line: Vec<SubInvoiceLine>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct SubInvoiceLine {
    #[yaserde(rename = "ID", prefix = "cbc")]
    pub cbc_id: Option<Identifier>,
    #[yaserde(rename = "Note", prefix = "cbc")]
    pub cbc_note: Option<String>,
    #[yaserde(rename = "InvoicedQuantity", prefix = "cbc")]
    pub cbc_invoiced_quantity: Option<CbcInvoicedQuantity>,
    #[yaserde(rename = "LineExtensionAmount", prefix = "cbc")]
    pub cbc_line_extension_amount: Option<Amount>,
    #[yaserde(rename = "AccountingCost", prefix = "cbc")]
    pub cbc_accounting_cost: Option<String>,
    #[yaserde(rename = "InvoicePeriod", prefix = "cac")]
    pub cac_invoice_period: Option<Period>,
    #[yaserde(rename = "OrderLineReference", prefix = "cac")]
    pub cac_order_line_reference: Option<CacOrderLineReference>,
    #[yaserde(rename = "DocumentReference", prefix = "cac")]
    pub cac_document_reference: Option<CacDocumentReference>,
    #[yaserde(rename = "AllowanceCharge", prefix = "cac")]
    pub cac_allowance_charge: Vec<CacAllowanceCharge>,
    #[yaserde(rename = "Item", prefix = "cac")]
    pub cac_item: Option<CacItem>,
    #[yaserde(rename = "Price", prefix = "cac")]
    pub cac_price: Option<CacPrice>,
    #[yaserde(rename = "SubInvoiceLine", prefix = "cac")]
    pub cac_sub_invoice_line: Vec<SubSubInvoiceLine>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct SubSubInvoiceLine {
    #[yaserde(rename = "ID", prefix = "cbc")]
    pub cbc_id: Option<Identifier>,
    #[yaserde(rename = "Note", prefix = "cbc")]
    pub cbc_note: Option<String>,
    #[yaserde(rename = "InvoicedQuantity", prefix = "cbc")]
    pub cbc_invoiced_quantity: Option<CbcInvoicedQuantity>,
    #[yaserde(rename = "LineExtensionAmount", prefix = "cbc")]
    pub cbc_line_extension_amount: Option<Amount>,
    #[yaserde(rename = "AccountingCost", prefix = "cbc")]
    pub cbc_accounting_cost: Option<String>,
    #[yaserde(rename = "InvoicePeriod", prefix = "cac")]
    pub cac_invoice_period: Option<Period>,
    #[yaserde(rename = "OrderLineReference", prefix = "cac")]
    pub cac_order_line_reference: Option<CacOrderLineReference>,
    #[yaserde(rename = "DocumentReference", prefix = "cac")]
    pub cac_document_reference: Option<CacDocumentReference>,
    #[yaserde(rename = "AllowanceCharge", prefix = "cac")]
    pub cac_allowance_charge: Vec<CacAllowanceCharge>,
    #[yaserde(rename = "Item", prefix = "cac")]
    pub cac_item: Option<CacItem>,
    #[yaserde(rename = "Price", prefix = "cac")]
    pub cac_price: Option<CacPrice>,
    #[yaserde(rename = "SubInvoiceLine", prefix = "cac")]
    pub cac_sub_invoice_line: Vec<SubSubSubInvoiceLine>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct SubSubSubInvoiceLine {
    #[yaserde(rename = "ID", prefix = "cbc")]
    pub cbc_id: Option<Identifier>,
    #[yaserde(rename = "Note", prefix = "cbc")]
    pub cbc_note: Option<String>,
    #[yaserde(rename = "InvoicedQuantity", prefix = "cbc")]
    pub cbc_invoiced_quantity: Option<CbcInvoicedQuantity>,
    #[yaserde(rename = "LineExtensionAmount", prefix = "cbc")]
    pub cbc_line_extension_amount: Option<Amount>,
    #[yaserde(rename = "AccountingCost", prefix = "cbc")]
    pub cbc_accounting_cost: Option<String>,
    #[yaserde(rename = "InvoicePeriod", prefix = "cac")]
    pub cac_invoice_period: Option<Period>,
    #[yaserde(rename = "OrderLineReference", prefix = "cac")]
    pub cac_order_line_reference: Option<CacOrderLineReference>,
    #[yaserde(rename = "DocumentReference", prefix = "cac")]
    pub cac_document_reference: Option<CacDocumentReference>,
    #[yaserde(rename = "AllowanceCharge", prefix = "cac")]
    pub cac_allowance_charge: Vec<CacAllowanceCharge>,
    #[yaserde(rename = "Item", prefix = "cac")]
    pub cac_item: Option<CacItem>,
    #[yaserde(rename = "Price", prefix = "cac")]
    pub cac_price: Option<CacPrice>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CbcInvoicedQuantity {
    #[yaserde(rename = "unitCode", attribute = true)]
    pub unit_code: Option<String>,
    #[yaserde(text = true)]
    pub text: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacItem {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "Description", prefix = "cbc")]
    pub cbc_description: Option<String>,
    #[yaserde(rename = "Name", prefix = "cbc")]
    pub cbc_name: Option<String>,
    #[yaserde(rename = "BuyersItemIdentification", prefix = "cac")]
    pub cac_buyers_item_identification: Option<CacBuyersItemIdentification>,
    #[yaserde(rename = "SellersItemIdentification", prefix = "cac")]
    pub cac_sellers_item_identification: Option<CacSellersItemIdentification>,
    #[yaserde(rename = "StandardItemIdentification", prefix = "cac")]
    pub cac_standard_item_identification: Option<CacStandardItemIdentification>,
    #[yaserde(rename = "OriginCountry", prefix = "cac")]
    pub cac_origin_country: Option<CacCountry>,
    #[yaserde(rename = "CommodityClassification", prefix = "cac")]
    pub cac_commodity_classification: Vec<CacCommodityClassification>,
    #[yaserde(rename = "ClassifiedTaxCategory", prefix = "cac")]
    pub cac_classified_tax_category: Option<TaxCategory>,
    #[yaserde(rename = "AdditionalItemProperty", prefix = "cac")]
    pub cac_additional_item_property: Vec<CacAdditionalItemProperty>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacClassifiedTaxCategory {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ID", prefix = "cbc")]
    pub cbc_id: Option<String>,
    #[yaserde(rename = "Percent", prefix = "cbc")]
    pub cbc_percent: Option<String>,
    #[yaserde(rename = "TaxScheme", prefix = "cac")]
    pub cac_tax_scheme: Option<CacInvoiceLineCacItemCacClassifiedTaxCategoryCacTaxScheme>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacInvoiceLineCacItemCacClassifiedTaxCategoryCacTaxScheme {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ID", prefix = "cbc")]
    pub cbc_id: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacAdditionalItemProperty {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "Name", prefix = "cbc")]
    pub cbc_name: Option<String>,
    #[yaserde(rename = "Value", prefix = "cbc")]
    pub cbc_value: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacSellersItemIdentification {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ID", prefix = "cbc")]
    pub cbc_id: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacOriginCountry {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "IdentificationCode", prefix = "cbc")]
    pub cbc_identification_code: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacStandardItemIdentification {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ID", prefix = "cbc")]
    pub cbc_id: Option<InvoiceCacInvoiceLineCacItemCacStandardItemIdentificationCbcId>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct InvoiceCacInvoiceLineCacItemCacStandardItemIdentificationCbcId {
    #[yaserde(rename = "schemeID", attribute = true)]
    pub scheme_id: Option<String>,
    #[yaserde(text = true)]
    pub text: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacBuyersItemIdentification {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ID", prefix = "cbc")]
    pub cbc_id: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacCommodityClassification {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ItemClassificationCode", prefix = "cbc")]
    pub cbc_item_classification_code: Option<CbcItemClassificationCode>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CbcItemClassificationCode {
    #[yaserde(rename = "listID", attribute = true)]
    pub list_id: Option<String>,
    #[yaserde(rename = "listVersionID", attribute = true)]
    pub list_version_id: Option<String>,
    #[yaserde(text = true)]
    pub text: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacPrice {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "PriceAmount", prefix = "cbc")]
    pub cbc_price_amount: Option<Amount>,
    #[yaserde(rename = "BaseQuantity", prefix = "cbc")]
    pub cbc_base_quantity: Option<CbcBaseQuantity>,
    #[yaserde(rename = "AllowanceCharge", prefix = "cac")]
    pub cac_allowance_charge: Option<CacAllowanceCharge>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CbcBaseQuantity {
    #[yaserde(rename = "unitCode", attribute = true)]
    pub unit_code: Option<String>,
    #[yaserde(text = true)]
    pub text: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacOrderLineReference {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "LineID", prefix = "cbc")]
    pub cbc_line_id: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacDocumentReference {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ID", prefix = "cbc")]
    pub id: Option<Identifier>,
    #[yaserde(rename = "DocumentTypeCode", prefix = "cbc")]
    pub cbc_document_type_code: Option<String>,
    #[yaserde(rename = "DocumentType", prefix = "cbc")]
    pub cbc_document_type: Option<String>,
    #[yaserde(rename = "DocumentDescription", prefix = "cbc")]
    pub cbc_document_description: Vec<String>,
    #[yaserde(rename = "IssueDate", prefix = "cbc")]
    pub cbc_issue_date: Option<Iso8601Date>,
    #[yaserde(rename = "IssueTime", prefix = "cbc")]
    pub cbc_issue_time: Option<String>,
    #[yaserde(rename = "Attachment", prefix = "cac")]
    pub cac_attachment: Option<CacAttachment>,
    #[yaserde(rename = "ValidityPeriod", prefix = "cac")]
    pub cac_validity_period: Option<Period>,
    #[yaserde(rename = "IssuerParty", prefix = "cac")]
    pub cac_issuer_party: Option<Party>,
    #[yaserde(rename = "ResultOfVerification", prefix = "cac")]
    pub cac_result_of_verification: Option<CacResultOfVerification>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "" = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
    "cac" = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
    "cec" = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2",
    "cbc" = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
  }
)]
pub struct CacResultOfVerification {
    #[yaserde(rename = "ValidatorID", prefix = "cbc")]
    pub cbc_validator_id: Option<Identifier>,
    #[yaserde(rename = "ValidationResultCode", prefix = "cbc")]
    pub cbc_validation_result_code: Option<String>,
    #[yaserde(rename = "ValidationDate", prefix = "cbc")]
    pub cbc_validation_date: Option<Iso8601Date>,
    #[yaserde(rename = "ValidationTime", prefix = "cbc")]
    pub cbc_validation_time: Option<String>,
    #[yaserde(rename = "ValidateProcess", prefix = "cbc")]
    pub cbc_validate_process: Option<String>,
    #[yaserde(rename = "ValidateTool", prefix = "cbc")]
    pub cbc_validate_tool: Option<String>,
    #[yaserde(rename = "ValidateToolVersion", prefix = "cbc")]
    pub cbc_validate_tool_version: Option<String>,
    #[yaserde(rename = "SignatoryParty", prefix = "cac")]
    pub cac_signatory_party: Option<Party>,
}
