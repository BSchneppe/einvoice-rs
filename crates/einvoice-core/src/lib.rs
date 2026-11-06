pub mod cii {
    pub mod cii_model;
}

#[allow(clippy::needless_question_mark)]
pub mod ubl {
    pub mod ubl_model;
}

pub use cii::cii_model::CrossIndustryInvoice;
use einvoice_deps_yaserde::__xml::attribute::OwnedAttribute;
use einvoice_deps_yaserde::__xml::namespace::Namespace;
use einvoice_deps_yaserde::de::Deserializer;
use einvoice_deps_yaserde::ser::Serializer;
use einvoice_deps_yaserde::{YaDeserialize, YaSerialize};
use std::fmt::Debug;
use std::io::{Read, Write};
use thiserror::Error;
pub use ubl::ubl_model::UblInvoice;

#[derive(Debug, Error, PartialEq)]
pub enum InvoiceError {
    #[error("Failed to parse XML")]
    ParseError(String),
    #[error("Failed to validate XML: {0:?}")]
    ValidationError(Vec<ValidationError>),
    #[error("Failed to unwrap Arc")]
    ArcError,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BusinessRuleViolation {
    pub rule_id: String,
    pub rule_text: String,
}
impl BusinessRuleViolation {
    pub fn new(rule_id: &str, rule_text: &str) -> Self {
        Self {
            rule_id: rule_id.to_string(),
            rule_text: rule_text.to_string(),
        }
    }
}

#[derive(Error, Debug, Clone, PartialEq)]
pub enum ValidationError {
    #[error("Fatal: {0:?}")]
    Fatal(BusinessRuleViolation),
    #[error("Warning: {0:?}")]
    Warning(BusinessRuleViolation),
    #[error("Failed to execute validation: {0:?}")]
    Error(String),
}
impl YaDeserialize for InvoiceStandard {
    fn deserialize<R: Read>(reader: &mut Deserializer<R>) -> Result<Self, String> {
        UblInvoice::deserialize(reader)
            .map(InvoiceStandard::UBL)
            .or_else(|_| CrossIndustryInvoice::deserialize(reader).map(InvoiceStandard::CII))
    }
}

#[allow(clippy::large_enum_variant)]
pub enum InvoiceStandard {
    UBL(UblInvoice),
    CII(CrossIndustryInvoice),
}

impl YaSerialize for InvoiceStandard {
    fn serialize<W: Write>(&self, writer: &mut Serializer<W>) -> Result<(), String> {
        match self {
            InvoiceStandard::UBL(invoice) => invoice.serialize(writer),
            InvoiceStandard::CII(invoice) => invoice.serialize(writer),
        }
    }

    fn serialize_attributes(
        &self,
        attributes: Vec<OwnedAttribute>,
        namespace: Namespace,
    ) -> Result<(Vec<OwnedAttribute>, Namespace), String> {
        match self {
            InvoiceStandard::UBL(invoice) => invoice.serialize_attributes(attributes, namespace),
            InvoiceStandard::CII(invoice) => invoice.serialize_attributes(attributes, namespace),
        }
    }
}

/// Parse an invoice XML string into a typed `InvoiceStandard` enum.
///
/// Attempts UBL parsing first, then falls back to CII.
///
/// # Arguments
/// * `xml` - The XML content of the invoice
/// # Returns
/// * `InvoiceStandard` - The parsed invoice (UBL or CII)
/// # Errors
/// * `InvoiceError::ParseError` - If the XML cannot be parsed as either format
pub fn parse_invoice(xml: &str) -> Result<InvoiceStandard, InvoiceError> {
    einvoice_deps_yaserde::de::from_str(xml).map_err(InvoiceError::ParseError)
}

#[cfg(test)]
mod tests {
    use crate::{parse_invoice, InvoiceStandard};

    #[test]
    fn parse_ubl_works() {
        let s = include_str!("../tests/inputs/ubl/01.01_comprehensive_test_ubl.xml");
        let invoice_standard = parse_invoice(s).unwrap();
        match invoice_standard {
            InvoiceStandard::UBL(invoice) => {
                assert!(invoice.cbc_id.is_some());
            }
            _ => panic!("Expected UBL invoice"),
        }
    }
}
