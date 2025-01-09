pub mod cii {
    pub mod cii_business_rule_validator;
    pub mod cii_model;
}

pub mod ubl {
    pub mod ubl_business_rule_validator;
    pub mod ubl_model;
}

use crate::cii::cii_business_rule_validator;
use crate::ubl::ubl_business_rule_validator;
use crate::ZugferdProfile::EN16931;
pub use cii::cii_model::CrossIndustryInvoice;
use lopdf::Document;
use lopdf::Error;
use lopdf::Object;
use std::fmt::Debug;
use std::io::{Read, Write};
use std::sync::Arc;
use thiserror::Error;
pub use ubl::ubl_model::UblInvoice;
use yaserde::__xml::attribute::OwnedAttribute;
use yaserde::__xml::namespace::Namespace;
use yaserde::de::Deserializer;
use yaserde::ser::Serializer;
use yaserde::{YaDeserialize, YaSerialize};

uniffi::setup_scaffolding!();
#[derive(Debug, uniffi::Error, Error)]
pub enum InvoiceError {
    #[error("Failed to parse XML")]
    ParseError(String),
    #[error("Failed to validate XML: {0}")]
    ValidationError(String),
    #[error("Failed to unwrap Arc")]
    ArcError,
}

#[derive(Debug, Clone, uniffi::Record)]
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

#[derive(Error, Debug, Clone, uniffi::Enum)]
pub enum ValidationError {
    #[error("Fatal: {0:?}")]
    Fatal(BusinessRuleViolation),
    #[error("Warning: {0:?}")]
    Warning(BusinessRuleViolation),
    #[error("Failed to concurrently execute: {0:?}")]
    ConcurrencyError(String),
}
impl YaDeserialize for InvoiceStandard {
    fn deserialize<R: Read>(reader: &mut Deserializer<R>) -> Result<Self, String> {
        UblInvoice::deserialize(reader)
            .map(InvoiceStandard::UBL)
            .or_else(|_| CrossIndustryInvoice::deserialize(reader).map(InvoiceStandard::CII))
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(uniffi::Enum)]
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
/// Validate an invoice XML file
/// # Arguments
/// * `xml` - The XML content of the invoice
/// # Returns
/// * `InvoiceStandard` - The parsed invoice
/// # Errors
/// * `InvoiceError` - If the invoice is invalid
#[uniffi::export]
pub fn validate_invoice(xml: &str) -> Result<InvoiceStandard, InvoiceError> {
    let invoice_standard: InvoiceStandard =
        yaserde::de::from_str(xml).map_err(InvoiceError::ParseError)?;

    match &invoice_standard {
        InvoiceStandard::UBL(invoice) => {
            let arc = Arc::new(invoice);
            ubl_business_rule_validator::validate_invoice(arc).map_err(|e| {
                InvoiceError::ValidationError(
                    e.iter()
                        .map(|e| e.to_string())
                        .collect::<Vec<String>>()
                        .join("\n"),
                )
            })?;
        }
        InvoiceStandard::CII(invoice) => {
            let arc = Arc::new(invoice);
            cii_business_rule_validator::validate_invoice(EN16931, arc).map_err(|e| {
                InvoiceError::ValidationError(
                    e.iter()
                        .map(|e| e.to_string())
                        .collect::<Vec<String>>()
                        .join("\n"),
                )
            })?;
        }
    }

    Ok(invoice_standard)
}
/// Validate a ZUGFeRD PDF file
/// # Arguments
/// * `file_path` - The path to the PDF file
/// # Returns
/// * `InvoiceStandard` - The parsed invoice
/// # Errors
/// * `InvoiceError` - If the invoice is invalid
///
/// This first extracts the embedded XML file from the PDF and then delegates to `validate_invoice`
#[uniffi::export]
pub fn validate_zugferd_pdf(file_path: &str) -> Result<InvoiceStandard, InvoiceError> {
    let pdf_document =
        Document::load(file_path).map_err(|e| InvoiceError::ParseError(e.to_string()))?;
    match get_embedded_xml_file(&pdf_document) {
        Ok(embedded_file) => {
            if let Some((_file_name, content)) = embedded_file {
                let xml_content = String::from_utf8(content)
                    .map_err(|err| InvoiceError::ParseError(err.to_string()))?;
                validate_invoice(&xml_content)
            } else {
                Err(InvoiceError::ParseError(
                    "No embedded XML file found".to_string(),
                ))
            }
        }
        Err(e) => Err(InvoiceError::ParseError(e.to_string())),
    }
}

fn get_embedded_xml_file(pdf: &Document) -> Result<Option<(String, Vec<u8>)>, Error> {
    let root_ref = pdf.trailer.get(b"Root")?.as_reference()?;

    let catalog = pdf.get_object(root_ref).and_then(|obj| obj.as_dict())?;

    let names = catalog.get(b"Names")?;

    let names_dict = match names {
        Object::Dictionary(dict) => dict.clone(),
        Object::Reference(obj_ref) => {
            let dict_obj = pdf.get_object(*obj_ref).and_then(|obj| obj.as_dict())?;
            dict_obj.clone()
        }
        _ => {
            return Err(Error::DictKey);
        }
    };

    names_dict.get(b"EmbeddedFiles")?;

    let embedded_files_obj = names_dict.get(b"EmbeddedFiles")?;

    let embedded_files_dict = match embedded_files_obj {
        Object::Dictionary(dict) => dict.clone(),
        Object::Reference(obj_ref) => pdf.get_object(*obj_ref)?.as_dict()?.clone(),
        _ => return Err(Error::DictKey),
    };

    let names_array_obj = embedded_files_dict.get(b"Names")?;

    let names_array = names_array_obj.as_array()?;

    let mut idx = 0;
    while idx < names_array.len() {
        let file_name_obj = &names_array[idx];
        let file_spec_obj = &names_array[idx + 1];
        idx += 2;

        let file_name = match file_name_obj {
            Object::String(bytes, _) => String::from_utf8_lossy(bytes).to_string(),
            _ => continue, // Not a valid file name
        };

        if !file_name.to_lowercase().ends_with(".xml") {
            continue;
        }

        let file_spec_dict = match file_spec_obj {
            Object::Reference(r) => pdf.get_object(*r)?.as_dict()?.clone(),
            Object::Dictionary(d) => d.clone(),
            _ => continue, // Not a valid file spec
        };

        let ef_obj = file_spec_dict.get(b"EF")?;

        let ef_dict = match ef_obj {
            Object::Reference(r) => pdf.get_object(*r)?.as_dict()?.clone(),
            Object::Dictionary(d) => d.clone(),
            _ => continue,
        };

        let file_stream_ref = ef_dict.get(b"F")?;

        let file_stream = match file_stream_ref {
            Object::Reference(r) => pdf.get_object(*r)?,
            Object::Stream(s) => &Object::Stream(s.clone()),
            _ => continue,
        };

        let file_bytes = match file_stream {
            Object::Stream(ref stream) => stream.decompressed_content()?,
            _ => continue,
        };

        return Ok(Some((file_name, file_bytes)));
    }
    Ok(None)
}

#[derive(uniffi::Enum)]
// TODO: parse the profile based on the XML content
pub enum ZugferdProfile {
    Basic,
    EN16931,
}

#[cfg(test)]
mod tests {
    use crate::{validate_invoice, InvoiceError, InvoiceStandard};

    #[test]
    fn it_works() {
        let s = include_str!("../tests/inputs/ubl/01.01_comprehensive_test_ubl.xml");
        let invoice_standard = validate_invoice(s).unwrap();
        match invoice_standard {
            InvoiceStandard::UBL(invoice) => {
                assert!(invoice.cbc_id.is_some());
            }
            _ => panic!("Expected UBL invoice"),
        }
    }

    #[test]
    fn err_works() {
        let s = include_str!(
            "../tests/inputs/ubl/invalid/no_customizationid_01.01_comprehensive_test_ubl.xml"
        );
        let res = validate_invoice(s);
        assert!(res.is_err());
        match res {
            Err(e) => {
                assert!(matches!(e, InvoiceError::ValidationError(_)));
            }
            _ => panic!("Expected an error"),
        }
    }

    #[test]
    fn pdf_extraction_works() {
        let invoice = crate::validate_zugferd_pdf(
            "./tests/inputs/zugferd/EXTENDED_Projektabschlussrechnung.pdf",
        )
        .unwrap();
        match invoice {
            InvoiceStandard::CII(invoice) => {
                assert!(invoice.rsm_exchanged_document.is_some());
            }
            _ => panic!("Expected CII invoice"),
        }
    }
}
