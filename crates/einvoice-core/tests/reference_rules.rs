// Reference implementations of the 11 UBL business rules that had real validation logic
// in the original hand-written validator (ubl_business_rule_validator.rs).
//
// These serve as ground truth for verifying the Schematron compiler's output.
// When the compiler generates validation code for these rules, its behavior should
// match these implementations.
//
// Rules extracted from: src/ubl/ubl_business_rule_validator.rs (deleted in remove-stub-validators)

#[cfg(test)]
mod reference_rules {
    use einvoice_core::{BusinessRuleViolation, UblInvoice, ValidationError};

    fn validate_br_01(invoice: &UblInvoice) -> Result<(), ValidationError> {
        if !invoice
            .cbc_customization_id
            .iter()
            .any(|customization_id| !customization_id.is_empty())
        {
            return Err(ValidationError::Fatal(BusinessRuleViolation::new(
                "BR-01",
                "[BR-01]-An Invoice shall have a Specification identifier (BT-24).",
            )));
        }
        Ok(())
    }

    fn validate_br_02(invoice: &UblInvoice) -> Result<(), ValidationError> {
        if !invoice.cbc_id.iter().any(|id| !id.is_empty()) {
            return Err(ValidationError::Fatal(BusinessRuleViolation::new(
                "BR-02",
                "[BR-02]-An Invoice shall have an Invoice number (BT-1).",
            )));
        }
        Ok(())
    }

    fn validate_br_11(invoice: &UblInvoice) -> Result<(), ValidationError> {
        if let Some(party) = &invoice.cac_accounting_customer_party {
            if let Some(cac_party) = &party.cac_party {
                if let Some(cac_postal_address) = &cac_party.cac_postal_address {
                    if cac_postal_address.cac_country.is_none() {
                        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
                            "BR-11",
                            "[BR-11]-The Buyer postal address shall contain a Buyer country code (BT-55).",
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    fn validate_br_31(invoice: &UblInvoice) -> Result<(), ValidationError> {
        if invoice.cac_allowance_charge.iter().any(|allowance_charge| {
            allowance_charge
                .charge_indicator
                .map(|charge| !charge)
                .unwrap_or(false)
                && allowance_charge.amount.is_none()
        }) {
            return Err(ValidationError::Fatal(BusinessRuleViolation::new(
                "BR-31",
                "[BR-31]-Each Document level allowance (BG-20) shall have a Document level allowance amount (BT-92).",
            )));
        }
        Ok(())
    }

    fn validate_br_32(invoice: &UblInvoice) -> Result<(), ValidationError> {
        if invoice.cac_allowance_charge.iter().any(|allowance_charge| {
            allowance_charge
                .charge_indicator
                .map(|charge| !charge)
                .unwrap_or(false)
                && allowance_charge
                    .cac_tax_category
                    .iter()
                    .all(|tax_category| {
                        tax_category.cac_tax_scheme.iter().any(|tax_scheme| {
                            tax_scheme
                                .id
                                .iter()
                                .any(|identifier| identifier.id.iter().any(|id| id != "VAT"))
                        })
                    })
        }) {
            return Err(ValidationError::Fatal(BusinessRuleViolation::new(
                "BR-32",
                "[BR-32]-Each Document level allowance (BG-20) shall have a Document level allowance VAT category code (BT-95).",
            )));
        }
        Ok(())
    }

    fn validate_br_33(invoice: &UblInvoice) -> Result<(), ValidationError> {
        if invoice.cac_allowance_charge.iter().any(|allowance_charge| {
            allowance_charge
                .charge_indicator
                .map(|charge_indicator| !charge_indicator)
                .unwrap_or(false)
                && allowance_charge.cbc_allowance_charge_reason.is_none()
                && allowance_charge.cbc_allowance_charge_reason_code.is_none()
        }) {
            return Err(ValidationError::Fatal(BusinessRuleViolation::new(
                "BR-33",
                "[BR-33]-Each Document level allowance (BG-20) shall have a Document level allowance reason (BT-97) or a Document level allowance reason code (BT-98).",
            )));
        }
        Ok(())
    }

    fn validate_br_51(invoice: &UblInvoice) -> Result<(), ValidationError> {
        if invoice.cac_payment_means.iter().any(|payment_means| {
            payment_means.cac_card_account.iter().any(|card_account| {
                card_account
                    .primary_account_number_id
                    .iter()
                    .any(|identifier| identifier.id.iter().any(|id| id.len() > 10))
            })
        }) {
            return Err(ValidationError::Warning(BusinessRuleViolation::new(
                "BR-51",
                "[BR-51]-In accordance with card payments security standards an invoice should never include a full card primary account number (BT-87). At the moment PCI Security Standards Council has defined that the first 6 digits and last 4 digits are the maximum number of digits to be shown.",
            )));
        }
        Ok(())
    }

    fn validate_br_52(invoice: &UblInvoice) -> Result<(), ValidationError> {
        if invoice
            .cac_additional_document_reference
            .iter()
            .any(|doc| doc.id.is_none())
        {
            return Err(ValidationError::Fatal(BusinessRuleViolation::new(
                "BR-52",
                "[BR-52]-Each Additional supporting document (BG-24) shall contain a Supporting document reference (BT-122).",
            )));
        }
        Ok(())
    }

    fn validate_br_57(invoice: &UblInvoice) -> Result<(), ValidationError> {
        if invoice.cac_delivery.iter().any(|delivery| {
            delivery
                .cac_delivery_location
                .iter()
                .any(|delivery_location| {
                    delivery_location
                        .cac_address
                        .iter()
                        .any(|address| address.cac_country.is_none())
                })
        }) {
            return Err(ValidationError::Fatal(BusinessRuleViolation::new(
                "BR-57",
                "[BR-57]-Each Deliver to address (BG-15) shall contain a Deliver to country code (BT-80).",
            )));
        }
        Ok(())
    }

    fn validate_br_63(invoice: &UblInvoice) -> Result<(), ValidationError> {
        if let Some(party) = &invoice.cac_accounting_customer_party {
            if let Some(cac_party) = &party.cac_party {
                if let Some(cbc_endpoint_id) = &cac_party.cbc_endpoint_id {
                    if cbc_endpoint_id.scheme_id.is_none() {
                        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
                            "BR-63",
                            "[BR-63]-The Buyer electronic address (BT-49) shall have a Scheme identifier.",
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    fn validate_br_co_21(invoice: &UblInvoice) -> Result<(), ValidationError> {
        if invoice.cac_allowance_charge.iter().any(|allowance_charge| {
            allowance_charge
                .charge_indicator
                .map(|charge_indicator| !charge_indicator)
                .unwrap_or(false)
                && allowance_charge.cbc_allowance_charge_reason.is_none()
                && allowance_charge.cbc_allowance_charge_reason_code.is_none()
        }) {
            return Err(ValidationError::Fatal(BusinessRuleViolation::new(
                "BR-CO-21",
                "[BR-CO-21]-Each Document level allowance (BG-20) shall contain a Document level allowance reason (BT-97) or a Document level allowance reason code (BT-98), or both.",
            )));
        }
        Ok(())
    }

    #[test]
    fn reference_rules_compile() {
        // Verify these reference implementations still compile against the current UblInvoice type.
        // When the Schematron compiler generates validation code, its output for these rules
        // should produce equivalent results.
        type ReferenceRule = fn(&UblInvoice) -> Result<(), ValidationError>;
        let _fns: Vec<ReferenceRule> = vec![
            validate_br_01,
            validate_br_02,
            validate_br_11,
            validate_br_31,
            validate_br_32,
            validate_br_33,
            validate_br_51,
            validate_br_52,
            validate_br_57,
            validate_br_63,
            validate_br_co_21,
        ];
    }
}
