use einvoice_deps_yaserde_derive::{YaDeserialize, YaSerialize};
#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
default_namespace = "",
prefix = "rsm",
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct CrossIndustryInvoice {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ExchangedDocumentContext", prefix = "rsm")]
    pub rsm_exchanged_document_context: Option<RsmExchangedDocumentContext>,
    #[yaserde(rename = "ExchangedDocument", prefix = "rsm")]
    pub rsm_exchanged_document: Option<RsmExchangedDocument>,
    #[yaserde(rename = "SupplyChainTradeTransaction", prefix = "rsm")]
    pub rsm_supply_chain_trade_transaction: Option<RsmSupplyChainTradeTransaction>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RsmExchangedDocumentContext {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(
        rename = "BusinessProcessSpecifiedDocumentContextParameter",
        prefix = "ram"
    )]
    pub ram_business_process_specified_document_context_parameter:
        Option<RamBusinessProcessSpecifiedDocumentContextParameter>,
    #[yaserde(rename = "GuidelineSpecifiedDocumentContextParameter", prefix = "ram")]
    pub ram_guideline_specified_document_context_parameter:
        Option<RamGuidelineSpecifiedDocumentContextParameter>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamBusinessProcessSpecifiedDocumentContextParameter {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ID", prefix = "ram")]
    pub ram_id: Option<IdType>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamGuidelineSpecifiedDocumentContextParameter {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ID", prefix = "ram")]
    pub ram_id: Option<IdType>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RsmExchangedDocument {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ID", prefix = "ram")]
    pub ram_id: Option<IdType>,
    #[yaserde(rename = "TypeCode", prefix = "ram")]
    pub ram_type_code: Option<String>,
    #[yaserde(rename = "IssueDateTime", prefix = "ram")]
    pub ram_issue_date_time: Option<RamIssueDateTime>,
    #[yaserde(rename = "IncludedNote", prefix = "ram")]
    pub ram_included_note: Vec<RsmExchangedDocumentRamIncludedNote>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamIssueDateTime {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "DateTimeString", prefix = "udt")]
    pub udt_date_time_string: Option<DateTimeString>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct DateTimeString {
    #[yaserde(attribute = true)]
    pub format: String,
    #[yaserde(text = true)]
    pub text: String,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RsmExchangedDocumentRamIncludedNote {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "Content", prefix = "ram")]
    pub ram_content: Option<String>,
    #[yaserde(rename = "SubjectCode", prefix = "ram")]
    pub ram_subject_code: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RsmSupplyChainTradeTransaction {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "IncludedSupplyChainTradeLineItem", prefix = "ram")]
    pub ram_included_supply_chain_trade_line_item: Vec<RamIncludedSupplyChainTradeLineItem>,
    #[yaserde(rename = "ApplicableHeaderTradeAgreement", prefix = "ram")]
    pub ram_applicable_header_trade_agreement: Option<RamApplicableHeaderTradeAgreement>,
    #[yaserde(rename = "ApplicableHeaderTradeDelivery", prefix = "ram")]
    pub ram_applicable_header_trade_delivery: Option<RamApplicableHeaderTradeDelivery>,
    #[yaserde(rename = "ApplicableHeaderTradeSettlement", prefix = "ram")]
    pub ram_applicable_header_trade_settlement: Option<RamApplicableHeaderTradeSettlement>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamIncludedSupplyChainTradeLineItem {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "AssociatedDocumentLineDocument", prefix = "ram")]
    pub ram_associated_document_line_document: Option<RamAssociatedDocumentLineDocument>,
    #[yaserde(rename = "SpecifiedTradeProduct", prefix = "ram")]
    pub ram_specified_trade_product: Option<RamSpecifiedTradeProduct>,
    #[yaserde(rename = "SpecifiedLineTradeAgreement", prefix = "ram")]
    pub ram_specified_line_trade_agreement: Option<RamSpecifiedLineTradeAgreement>,
    #[yaserde(rename = "SpecifiedLineTradeDelivery", prefix = "ram")]
    pub ram_specified_line_trade_delivery: Option<RamSpecifiedLineTradeDelivery>,
    #[yaserde(rename = "SpecifiedLineTradeSettlement", prefix = "ram")]
    pub ram_specified_line_trade_settlement: Option<RamSpecifiedLineTradeSettlement>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamAssociatedDocumentLineDocument {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "LineID", prefix = "ram")]
    pub ram_line_id: Option<String>,
    #[yaserde(rename = "IncludedNote", prefix = "ram")]
    pub ram_included_note: Option<RamAssociatedDocumentLineDocumentRamIncludedNote>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamAssociatedDocumentLineDocumentRamIncludedNote {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "Content", prefix = "ram")]
    pub ram_content: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamSpecifiedTradeProduct {
    #[yaserde(rename = "GlobalID", prefix = "ram")]
    pub ram_global_id: Option<IdType>,
    #[yaserde(rename = "SellerAssignedID", prefix = "ram")]
    pub ram_seller_assigned_id: Option<IdType>,
    #[yaserde(rename = "BuyerAssignedID", prefix = "ram")]
    pub ram_buyer_assigned_id: Option<IdType>,
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "Name", prefix = "ram")]
    pub ram_name: Option<String>,
    #[yaserde(rename = "Description", prefix = "ram")]
    pub ram_description: Option<String>,
    #[yaserde(rename = "ApplicableProductCharacteristic", prefix = "ram")]
    pub ram_applicable_product_characteristic: Vec<RamApplicableProductCharacteristic>,
    #[yaserde(rename = "DesignatedProductClassification", prefix = "ram")]
    pub ram_designated_product_classification: Vec<RamDesignatedProductClassification>,
    #[yaserde(rename = "OriginTradeCountry", prefix = "ram")]
    pub ram_origin_trade_country: Option<RamOriginTradeCountry>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamOriginTradeCountry {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ID", prefix = "ram")]
    pub ram_id: Option<IdType>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct IdType {
    #[yaserde(rename = "schemeID", attribute = true)]
    pub scheme_id: Option<String>,
    #[yaserde(text = true)]
    pub id: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamApplicableProductCharacteristic {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "Description", prefix = "ram")]
    pub ram_description: Option<String>,
    #[yaserde(rename = "Value", prefix = "ram")]
    pub ram_value: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamDesignatedProductClassification {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ClassCode", prefix = "ram")]
    pub ram_class_code: Option<RamClassCode>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamClassCode {
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
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamSpecifiedLineTradeAgreement {
    #[yaserde(rename = "BuyerOrderReferencedDocument", prefix = "ram")]
    pub ram_buyer_order_referenced_document:
        Option<RamSpecifiedLineTradeAgreementRamBuyerOrderReferencedDocument>,
    #[yaserde(rename = "GrossPriceProductTradePrice", prefix = "ram")]
    pub ram_gross_price_product_trade_price: Option<RamGrossPriceProductTradePrice>,
    #[yaserde(rename = "NetPriceProductTradePrice", prefix = "ram")]
    pub ram_net_price_product_trade_price: Option<RamNetPriceProductTradePrice>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamNetPriceProductTradePrice {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ChargeAmount", prefix = "ram")]
    pub ram_charge_amount: Option<String>,
    #[yaserde(rename = "BasisQuantity", prefix = "ram")]
    pub ram_basis_quantity: Option<RamBasisQuantity>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamGrossPriceProductTradePrice {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ChargeAmount", prefix = "ram")]
    pub ram_charge_amount: Option<String>,
    #[yaserde(rename = "BasisQuantity", prefix = "ram")]
    pub ram_basis_quantity: Option<RamBasisQuantity>,
    #[yaserde(rename = "AppliedTradeAllowanceCharge", prefix = "ram")]
    pub ram_applied_trade_allowance_charge: Option<RamAppliedTradeAllowanceCharge>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamBasisQuantity {
    #[yaserde(rename = "unitCode", attribute = true)]
    pub unit_code: Option<String>,
    #[yaserde(text = true)]
    pub text: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamAppliedTradeAllowanceCharge {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ChargeIndicator", prefix = "ram")]
    pub ram_charge_indicator:
        Option<RamGrossPriceProductTradePriceRamAppliedTradeAllowanceChargeRamChargeIndicator>,
    #[yaserde(rename = "ActualAmount", prefix = "ram")]
    pub ram_actual_amount: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamGrossPriceProductTradePriceRamAppliedTradeAllowanceChargeRamChargeIndicator {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "Indicator", prefix = "udt")]
    pub udt_indicator: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamSpecifiedLineTradeAgreementRamBuyerOrderReferencedDocument {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "LineID", prefix = "ram")]
    pub ram_line_id: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamSpecifiedLineTradeDelivery {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "BilledQuantity", prefix = "ram")]
    pub ram_billed_quantity: Option<RamBilledQuantity>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamBilledQuantity {
    #[yaserde(rename = "unitCode", attribute = true)]
    pub unit_code: Option<String>,
    #[yaserde(text = true)]
    pub text: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamSpecifiedLineTradeSettlement {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ApplicableTradeTax", prefix = "ram")]
    pub ram_applicable_trade_tax: Option<ApplicableTradeTax>,
    #[yaserde(rename = "BillingSpecifiedPeriod", prefix = "ram")]
    pub ram_billing_specified_period:
        Option<RamSpecifiedLineTradeSettlementRamBillingSpecifiedPeriod>,
    #[yaserde(rename = "SpecifiedTradeAllowanceCharge", prefix = "ram")]
    pub ram_specified_trade_allowance_charge:
        Vec<RamSpecifiedLineTradeSettlementRamSpecifiedTradeAllowanceCharge>,
    #[yaserde(
        rename = "SpecifiedTradeSettlementLineMonetarySummation",
        prefix = "ram"
    )]
    pub ram_specified_trade_settlement_line_monetary_summation:
        Option<RamSpecifiedTradeSettlementLineMonetarySummation>,
    #[yaserde(rename = "AdditionalReferencedDocument", prefix = "ram")]
    pub ram_additional_referenced_document:
        Option<RamSpecifiedLineTradeSettlementRamAdditionalReferencedDocument>,
    #[yaserde(rename = "ReceivableSpecifiedTradeAccountingAccount", prefix = "ram")]
    pub ram_receivable_specified_trade_accounting_account:
        Option<RamSpecifiedLineTradeSettlementRamReceivableSpecifiedTradeAccountingAccount>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamSpecifiedTradeSettlementLineMonetarySummation {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "LineTotalAmount", prefix = "ram")]
    pub ram_line_total_amount: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamSpecifiedLineTradeSettlementRamReceivableSpecifiedTradeAccountingAccount {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ID", prefix = "ram")]
    pub ram_id: Option<IdType>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamSpecifiedLineTradeSettlementRamAdditionalReferencedDocument {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "IssuerAssignedID", prefix = "ram")]
    pub ram_issuer_assigned_id: Option<String>,
    #[yaserde(rename = "TypeCode", prefix = "ram")]
    pub ram_type_code: Option<String>,
    #[yaserde(rename = "ReferenceTypeCode", prefix = "ram")]
    pub ram_reference_type_code: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamSpecifiedLineTradeSettlementRamSpecifiedTradeAllowanceCharge {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ChargeIndicator", prefix = "ram")]
    pub ram_charge_indicator:
        Option<RamSpecifiedLineTradeSettlementRamSpecifiedTradeAllowanceChargeRamChargeIndicator>,
    #[yaserde(rename = "CalculationPercent", prefix = "ram")]
    pub ram_calculation_percent: Option<String>,
    #[yaserde(rename = "BasisAmount", prefix = "ram")]
    pub ram_basis_amount: Option<String>,
    #[yaserde(rename = "ActualAmount", prefix = "ram")]
    pub ram_actual_amount: Option<String>,
    #[yaserde(rename = "ReasonCode", prefix = "ram")]
    pub ram_reason_code: Option<String>,
    #[yaserde(rename = "Reason", prefix = "ram")]
    pub ram_reason: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamSpecifiedLineTradeSettlementRamSpecifiedTradeAllowanceChargeRamChargeIndicator {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "Indicator", prefix = "udt")]
    pub udt_indicator: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamSpecifiedLineTradeSettlementRamBillingSpecifiedPeriod {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "StartDateTime", prefix = "ram")]
    pub ram_start_date_time:
        Option<RamSpecifiedLineTradeSettlementRamBillingSpecifiedPeriodRamStartDateTime>,
    #[yaserde(rename = "EndDateTime", prefix = "ram")]
    pub ram_end_date_time:
        Option<RamSpecifiedLineTradeSettlementRamBillingSpecifiedPeriodRamEndDateTime>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamSpecifiedLineTradeSettlementRamBillingSpecifiedPeriodRamStartDateTime {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "DateTimeString", prefix = "udt")]
    pub udt_date_time_string: Option<DateTimeString>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamSpecifiedLineTradeSettlementRamBillingSpecifiedPeriodRamEndDateTime {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "DateTimeString", prefix = "udt")]
    pub udt_date_time_string: Option<DateTimeString>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamApplicableHeaderTradeAgreement {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "BuyerReference", prefix = "ram")]
    pub ram_buyer_reference: Option<String>,
    #[yaserde(rename = "SellerTradeParty", prefix = "ram")]
    pub ram_seller_trade_party: Option<TradeParty>,
    #[yaserde(rename = "BuyerTradeParty", prefix = "ram")]
    pub ram_buyer_trade_party: Option<TradeParty>,
    #[yaserde(rename = "SellerTaxRepresentativeTradeParty", prefix = "ram")]
    pub ram_seller_tax_representative_trade_party: Option<TradeParty>,
    #[yaserde(rename = "SellerOrderReferencedDocument", prefix = "ram")]
    pub ram_seller_order_referenced_document: Option<RamSellerOrderReferencedDocument>,
    #[yaserde(rename = "BuyerOrderReferencedDocument", prefix = "ram")]
    pub ram_buyer_order_referenced_document:
        Option<RamApplicableHeaderTradeAgreementRamBuyerOrderReferencedDocument>,
    #[yaserde(rename = "ContractReferencedDocument", prefix = "ram")]
    pub ram_contract_referenced_document: Option<RamContractReferencedDocument>,
    #[yaserde(rename = "AdditionalReferencedDocument", prefix = "ram")]
    pub ram_additional_referenced_document:
        Vec<RamApplicableHeaderTradeAgreementRamAdditionalReferencedDocument>,
    #[yaserde(rename = "SpecifiedProcuringProject", prefix = "ram")]
    pub ram_specified_procuring_project: Option<RamSpecifiedProcuringProject>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct TradeParty {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ID", prefix = "ram")]
    pub ram_id: Vec<String>,
    #[yaserde(rename = "GlobalID", prefix = "ram")]
    pub ram_global_id: Option<GlobalId>,
    #[yaserde(rename = "Name", prefix = "ram")]
    pub ram_name: Option<String>,
    #[yaserde(rename = "Description", prefix = "ram")]
    pub ram_description: Option<String>,
    #[yaserde(rename = "SpecifiedLegalOrganization", prefix = "ram")]
    pub ram_specified_legal_organization: Option<SpecifiedLegalOrganization>,
    #[yaserde(rename = "DefinedTradeContact", prefix = "ram")]
    pub ram_defined_trade_contact: Option<DefinedTradeContact>,
    #[yaserde(rename = "PostalTradeAddress", prefix = "ram")]
    pub ram_postal_trade_address: Option<PostalTradeAddress>,
    #[yaserde(rename = "URIUniversalCommunication", prefix = "ram")]
    pub ram_uriuniversal_communication: Option<UriUniversalCommunication>,
    #[yaserde(rename = "SpecifiedTaxRegistration", prefix = "ram")]
    pub ram_specified_tax_registration: Vec<SpecifiedTaxRegistration>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct GlobalId {
    #[yaserde(rename = "schemeID", attribute = true)]
    pub scheme_id: Option<String>,
    #[yaserde(text = true)]
    pub text: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct SpecifiedLegalOrganization {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ID", prefix = "ram")]
    pub ram_id: Option<RamSellerTradePartyRamSpecifiedLegalOrganizationRamId>,
    #[yaserde(rename = "TradingBusinessName", prefix = "ram")]
    pub ram_trading_business_name: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamSellerTradePartyRamSpecifiedLegalOrganizationRamId {
    #[yaserde(rename = "schemeID", attribute = true)]
    pub scheme_id: Option<String>,
    #[yaserde(text = true)]
    pub text: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct DefinedTradeContact {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "PersonName", prefix = "ram")]
    pub ram_person_name: Option<String>,
    #[yaserde(rename = "DepartmentName", prefix = "ram")]
    pub ram_department_name: Option<String>,
    #[yaserde(rename = "TelephoneUniversalCommunication", prefix = "ram")]
    pub ram_telephone_universal_communication:
        Option<RamSellerTradePartyRamDefinedTradeContactRamTelephoneUniversalCommunication>,
    #[yaserde(rename = "EmailURIUniversalCommunication", prefix = "ram")]
    pub ram_email_uriuniversal_communication:
        Option<RamSellerTradePartyRamDefinedTradeContactRamEmailUriuniversalCommunication>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamSellerTradePartyRamDefinedTradeContactRamTelephoneUniversalCommunication {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "CompleteNumber", prefix = "ram")]
    pub ram_complete_number: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamSellerTradePartyRamDefinedTradeContactRamEmailUriuniversalCommunication {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "URIID", prefix = "ram")]
    pub ram_uriid: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct PostalTradeAddress {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "PostcodeCode", prefix = "ram")]
    pub ram_postcode_code: Option<String>,
    #[yaserde(rename = "LineOne", prefix = "ram")]
    pub ram_line_one: Option<String>,
    #[yaserde(rename = "LineTwo", prefix = "ram")]
    pub ram_line_two: Option<String>,
    #[yaserde(rename = "LineThree", prefix = "ram")]
    pub ram_line_three: Option<String>,
    #[yaserde(rename = "CityName", prefix = "ram")]
    pub ram_city_name: Option<String>,
    #[yaserde(rename = "CountryID", prefix = "ram")]
    pub ram_country_id: Option<String>,
    #[yaserde(rename = "CountrySubDivisionName", prefix = "ram")]
    pub ram_country_sub_division_name: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct UriUniversalCommunication {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "URIID", prefix = "ram")]
    pub ram_uriid: Option<
        RamApplicableHeaderTradeAgreementRamSellerTradePartyRamUriuniversalCommunicationRamUriid,
    >,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamApplicableHeaderTradeAgreementRamSellerTradePartyRamUriuniversalCommunicationRamUriid
{
    #[yaserde(rename = "schemeID", attribute = true)]
    pub scheme_id: Option<String>,
    #[yaserde(text = true)]
    pub text: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct SpecifiedTaxRegistration {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ID", prefix = "ram")]
    pub ram_id: Option<RamSellerTradePartyRamSpecifiedTaxRegistrationRamId>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamSellerTradePartyRamSpecifiedTaxRegistrationRamId {
    #[yaserde(rename = "schemeID", attribute = true)]
    pub scheme_id: Option<String>,
    #[yaserde(text = true)]
    pub text: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamBuyerTradeParty {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ID", prefix = "ram")]
    pub ram_id: Option<IdType>,
    #[yaserde(rename = "Name", prefix = "ram")]
    pub ram_name: Option<String>,
    #[yaserde(rename = "SpecifiedLegalOrganization", prefix = "ram")]
    pub ram_specified_legal_organization: Option<RamBuyerTradePartyRamSpecifiedLegalOrganization>,
    #[yaserde(rename = "DefinedTradeContact", prefix = "ram")]
    pub ram_defined_trade_contact: Option<RamBuyerTradePartyRamDefinedTradeContact>,
    #[yaserde(rename = "PostalTradeAddress", prefix = "ram")]
    pub ram_postal_trade_address: Option<RamBuyerTradePartyRamPostalTradeAddress>,
    #[yaserde(rename = "URIUniversalCommunication", prefix = "ram")]
    pub ram_uriuniversal_communication: Option<RamBuyerTradePartyRamUriuniversalCommunication>,
    #[yaserde(rename = "SpecifiedTaxRegistration", prefix = "ram")]
    pub ram_specified_tax_registration: Option<RamBuyerTradePartyRamSpecifiedTaxRegistration>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamBuyerTradePartyRamSpecifiedLegalOrganization {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ID", prefix = "ram")]
    pub ram_id: Option<RamBuyerTradePartyRamSpecifiedLegalOrganizationRamId>,
    #[yaserde(rename = "TradingBusinessName", prefix = "ram")]
    pub ram_trading_business_name: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamBuyerTradePartyRamSpecifiedLegalOrganizationRamId {
    #[yaserde(rename = "schemeID", attribute = true)]
    pub scheme_id: Option<String>,
    #[yaserde(text = true)]
    pub text: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamBuyerTradePartyRamDefinedTradeContact {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "PersonName", prefix = "ram")]
    pub ram_person_name: Option<String>,
    #[yaserde(rename = "TelephoneUniversalCommunication", prefix = "ram")]
    pub ram_telephone_universal_communication:
        Option<RamBuyerTradePartyRamDefinedTradeContactRamTelephoneUniversalCommunication>,
    #[yaserde(rename = "EmailURIUniversalCommunication", prefix = "ram")]
    pub ram_email_uriuniversal_communication:
        Option<RamBuyerTradePartyRamDefinedTradeContactRamEmailUriuniversalCommunication>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamBuyerTradePartyRamDefinedTradeContactRamTelephoneUniversalCommunication {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "CompleteNumber", prefix = "ram")]
    pub ram_complete_number: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamBuyerTradePartyRamDefinedTradeContactRamEmailUriuniversalCommunication {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "URIID", prefix = "ram")]
    pub ram_uriid: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamBuyerTradePartyRamPostalTradeAddress {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "PostcodeCode", prefix = "ram")]
    pub ram_postcode_code: Option<String>,
    #[yaserde(rename = "LineOne", prefix = "ram")]
    pub ram_line_one: Option<String>,
    #[yaserde(rename = "LineTwo", prefix = "ram")]
    pub ram_line_two: Option<String>,
    #[yaserde(rename = "LineThree", prefix = "ram")]
    pub ram_line_three: Option<String>,
    #[yaserde(rename = "CityName", prefix = "ram")]
    pub ram_city_name: Option<String>,
    #[yaserde(rename = "CountryID", prefix = "ram")]
    pub ram_country_id: Option<String>,
    #[yaserde(rename = "CountrySubDivisionName", prefix = "ram")]
    pub ram_country_sub_division_name: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamBuyerTradePartyRamUriuniversalCommunication {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "URIID", prefix = "ram")]
    pub ram_uriid: Option<
        RamApplicableHeaderTradeAgreementRamBuyerTradePartyRamUriuniversalCommunicationRamUriid,
    >,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamApplicableHeaderTradeAgreementRamBuyerTradePartyRamUriuniversalCommunicationRamUriid {
    #[yaserde(rename = "schemeID", attribute = true)]
    pub scheme_id: Option<String>,
    #[yaserde(text = true)]
    pub text: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamBuyerTradePartyRamSpecifiedTaxRegistration {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ID", prefix = "ram")]
    pub ram_id: Option<RamBuyerTradePartyRamSpecifiedTaxRegistrationRamId>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamBuyerTradePartyRamSpecifiedTaxRegistrationRamId {
    #[yaserde(rename = "schemeID", attribute = true)]
    pub scheme_id: Option<String>,
    #[yaserde(text = true)]
    pub text: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamSellerTaxRepresentativeTradeParty {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "Name", prefix = "ram")]
    pub ram_name: Option<String>,
    #[yaserde(rename = "PostalTradeAddress", prefix = "ram")]
    pub ram_postal_trade_address: Option<RamSellerTaxRepresentativeTradePartyRamPostalTradeAddress>,
    #[yaserde(rename = "SpecifiedTaxRegistration", prefix = "ram")]
    pub ram_specified_tax_registration:
        Option<RamSellerTaxRepresentativeTradePartyRamSpecifiedTaxRegistration>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamSellerTaxRepresentativeTradePartyRamPostalTradeAddress {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "PostcodeCode", prefix = "ram")]
    pub ram_postcode_code: Option<String>,
    #[yaserde(rename = "LineOne", prefix = "ram")]
    pub ram_line_one: Option<String>,
    #[yaserde(rename = "LineTwo", prefix = "ram")]
    pub ram_line_two: Option<String>,
    #[yaserde(rename = "LineThree", prefix = "ram")]
    pub ram_line_three: Option<String>,
    #[yaserde(rename = "CityName", prefix = "ram")]
    pub ram_city_name: Option<String>,
    #[yaserde(rename = "CountryID", prefix = "ram")]
    pub ram_country_id: Option<String>,
    #[yaserde(rename = "CountrySubDivisionName", prefix = "ram")]
    pub ram_country_sub_division_name: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamSellerTaxRepresentativeTradePartyRamSpecifiedTaxRegistration {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ID", prefix = "ram")]
    pub ram_id: Option<RamSellerTaxRepresentativeTradePartyRamSpecifiedTaxRegistrationRamId>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamSellerTaxRepresentativeTradePartyRamSpecifiedTaxRegistrationRamId {
    #[yaserde(rename = "schemeID", attribute = true)]
    pub scheme_id: Option<String>,
    #[yaserde(text = true)]
    pub text: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamSellerOrderReferencedDocument {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "IssuerAssignedID", prefix = "ram")]
    pub ram_issuer_assigned_id: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamApplicableHeaderTradeAgreementRamBuyerOrderReferencedDocument {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "IssuerAssignedID", prefix = "ram")]
    pub ram_issuer_assigned_id: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamContractReferencedDocument {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "IssuerAssignedID", prefix = "ram")]
    pub ram_issuer_assigned_id: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamApplicableHeaderTradeAgreementRamAdditionalReferencedDocument {
    #[yaserde(rename = "IssuerAssignedID", prefix = "ram")]
    pub ram_issuer_assigned_id: Option<IdType>,
    #[yaserde(rename = "URIID", prefix = "ram")]
    pub ram_uriid: Option<String>,
    #[yaserde(rename = "TypeCode", prefix = "ram")]
    pub ram_type_code: Option<String>,
    #[yaserde(rename = "Name", prefix = "ram")]
    pub ram_name: Option<String>,
    #[yaserde(rename = "AttachmentBinaryObject", prefix = "ram")]
    pub ram_attachment_binary_object: Option<RamAttachmentBinaryObject>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamAttachmentBinaryObject {
    #[yaserde(rename = "mimeCode", attribute = true)]
    pub mime_code: Option<String>,
    #[yaserde(attribute = true)]
    pub filename: Option<String>,
    #[yaserde(text = true)]
    pub text: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamSpecifiedProcuringProject {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ID", prefix = "ram")]
    pub ram_id: Option<IdType>,
    #[yaserde(rename = "Name", prefix = "ram")]
    pub ram_name: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamApplicableHeaderTradeDelivery {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ShipToTradeParty", prefix = "ram")]
    pub ram_ship_to_trade_party: Option<TradeParty>,
    #[yaserde(rename = "ActualDeliverySupplyChainEvent", prefix = "ram")]
    pub ram_actual_delivery_supply_chain_event: Option<RamActualDeliverySupplyChainEvent>,
    #[yaserde(rename = "DespatchAdviceReferencedDocument", prefix = "ram")]
    pub ram_despatch_advice_referenced_document: Option<RamDespatchAdviceReferencedDocument>,
    #[yaserde(rename = "ReceivingAdviceReferencedDocument", prefix = "ram")]
    pub ram_receiving_advice_referenced_document: Option<RamReceivingAdviceReferencedDocument>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamActualDeliverySupplyChainEvent {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "OccurrenceDateTime", prefix = "ram")]
    pub ram_occurrence_date_time: Option<RamOccurrenceDateTime>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamOccurrenceDateTime {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "DateTimeString", prefix = "udt")]
    pub udt_date_time_string: Option<DateTimeString>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamDespatchAdviceReferencedDocument {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "IssuerAssignedID", prefix = "ram")]
    pub ram_issuer_assigned_id: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamReceivingAdviceReferencedDocument {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "IssuerAssignedID", prefix = "ram")]
    pub ram_issuer_assigned_id: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamApplicableHeaderTradeSettlement {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "CreditorReferenceID", prefix = "ram")]
    pub ram_creditor_reference_id: Option<String>,
    #[yaserde(rename = "PaymentReference", prefix = "ram")]
    pub ram_payment_reference: Option<String>,
    #[yaserde(rename = "TaxCurrencyCode", prefix = "ram")]
    pub ram_tax_currency_code: Option<String>,
    #[yaserde(rename = "InvoiceCurrencyCode", prefix = "ram")]
    pub ram_invoice_currency_code: Option<String>,
    #[yaserde(rename = "PayeeTradeParty", prefix = "ram")]
    pub ram_payee_trade_party: Option<TradeParty>,
    #[yaserde(rename = "SpecifiedTradeSettlementPaymentMeans", prefix = "ram")]
    pub ram_specified_trade_settlement_payment_means: Vec<RamSpecifiedTradeSettlementPaymentMeans>,
    #[yaserde(rename = "ApplicableTradeTax", prefix = "ram")]
    pub ram_applicable_trade_tax: Vec<ApplicableTradeTax>,
    #[yaserde(rename = "BillingSpecifiedPeriod", prefix = "ram")]
    pub ram_billing_specified_period:
        Option<RamApplicableHeaderTradeSettlementRamBillingSpecifiedPeriod>,
    #[yaserde(rename = "SpecifiedTradeAllowanceCharge", prefix = "ram")]
    pub ram_specified_trade_allowance_charge:
        Vec<RamApplicableHeaderTradeSettlementRamSpecifiedTradeAllowanceCharge>,
    #[yaserde(rename = "SpecifiedTradePaymentTerms", prefix = "ram")]
    pub ram_specified_trade_payment_terms: Option<RamSpecifiedTradePaymentTerms>,
    #[yaserde(
        rename = "SpecifiedTradeSettlementHeaderMonetarySummation",
        prefix = "ram"
    )]
    pub ram_specified_trade_settlement_header_monetary_summation:
        Option<RamSpecifiedTradeSettlementHeaderMonetarySummation>,
    #[yaserde(rename = "InvoiceReferencedDocument", prefix = "ram")]
    pub ram_invoice_referenced_document: Option<RamInvoiceReferencedDocument>,
    #[yaserde(rename = "ReceivableSpecifiedTradeAccountingAccount", prefix = "ram")]
    pub ram_receivable_specified_trade_accounting_account:
        Option<RamApplicableHeaderTradeSettlementRamReceivableSpecifiedTradeAccountingAccount>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamSpecifiedTradeSettlementPaymentMeans {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "TypeCode", prefix = "ram")]
    pub ram_type_code: Option<String>,
    #[yaserde(rename = "Information", prefix = "ram")]
    pub ram_information: Option<String>,
    #[yaserde(rename = "PayerPartyDebtorFinancialAccount", prefix = "ram")]
    pub ram_payer_party_debitor_financial_account: Option<RamPayeePartyCreditorFinancialAccount>,
    #[yaserde(rename = "PayeePartyCreditorFinancialAccount", prefix = "ram")]
    pub ram_payee_party_creditor_financial_account: Option<RamPayeePartyCreditorFinancialAccount>,
    #[yaserde(rename = "PayeeSpecifiedCreditorFinancialInstitution", prefix = "ram")]
    pub ram_payee_specified_creditor_financial_institution:
        Option<RamPayeeSpecifiedCreditorFinancialInstitution>,
    #[yaserde(rename = "ApplicableTradeSettlementFinancialCard", prefix = "ram")]
    pub ram_applicable_trade_settlement_financial_card:
        Option<RamApplicableTradeSettlementFinancialCard>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamApplicableTradeSettlementFinancialCard {
    #[yaserde(rename = "ID", prefix = "ram")]
    pub ram_id: Option<String>,
    #[yaserde(rename = "CardholderName", prefix = "ram")]
    pub ram_cardholder_name: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamPayeePartyCreditorFinancialAccount {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "IBANID", prefix = "ram")]
    pub ram_ibanid: Option<String>,
    #[yaserde(rename = "AccountName", prefix = "ram")]
    pub ram_account_name: Option<String>,
    #[yaserde(rename = "ProprietaryID", prefix = "ram")]
    pub ram_proprietary_id: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamPayeeSpecifiedCreditorFinancialInstitution {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "BICID", prefix = "ram")]
    pub ram_bicid: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct ApplicableTradeTax {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "CalculatedAmount", prefix = "ram")]
    pub ram_calculated_amount: Option<String>,
    #[yaserde(rename = "TypeCode", prefix = "ram")]
    pub ram_type_code: Option<String>,
    #[yaserde(rename = "ExemptionReason", prefix = "ram")]
    pub ram_exemption_reason: Option<String>,
    #[yaserde(rename = "BasisAmount", prefix = "ram")]
    pub ram_basis_amount: Option<String>,
    #[yaserde(rename = "CategoryCode", prefix = "ram")]
    pub ram_category_code: Option<String>,
    #[yaserde(rename = "DueDateTypeCode", prefix = "ram")]
    pub ram_due_date_type_code: Option<String>,
    #[yaserde(rename = "ExemptionReasonCode", prefix = "ram")]
    pub ram_exemption_reason_code: Option<String>,
    #[yaserde(rename = "TaxPointDate", prefix = "ram")]
    pub ram_tax_point_date: Option<RamTaxPointDate>,
    #[yaserde(rename = "RateApplicablePercent", prefix = "ram")]
    pub ram_rate_applicable_percent: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamTaxPointDate {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "DateString", prefix = "udt")]
    pub udt_date_string: Option<DateString>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct DateString {
    #[yaserde(attribute = true)]
    pub format: Option<String>,
    #[yaserde(text = true)]
    pub text: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamApplicableHeaderTradeSettlementRamBillingSpecifiedPeriod {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "StartDateTime", prefix = "ram")]
    pub ram_start_date_time:
        Option<RamApplicableHeaderTradeSettlementRamBillingSpecifiedPeriodRamStartDateTime>,
    #[yaserde(rename = "EndDateTime", prefix = "ram")]
    pub ram_end_date_time:
        Option<RamApplicableHeaderTradeSettlementRamBillingSpecifiedPeriodRamEndDateTime>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamApplicableHeaderTradeSettlementRamBillingSpecifiedPeriodRamStartDateTime {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "DateTimeString", prefix = "udt")]
    pub udt_date_time_string: Option<DateTimeString>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamApplicableHeaderTradeSettlementRamBillingSpecifiedPeriodRamEndDateTime {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "DateTimeString", prefix = "udt")]
    pub udt_date_time_string: Option<DateTimeString>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamApplicableHeaderTradeSettlementRamSpecifiedTradeAllowanceCharge {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ChargeIndicator", prefix = "ram")]
    pub ram_charge_indicator: Option<
        RamApplicableHeaderTradeSettlementRamSpecifiedTradeAllowanceChargeRamChargeIndicator,
    >,
    #[yaserde(rename = "CalculationPercent", prefix = "ram")]
    pub ram_calculation_percent: Option<String>,
    #[yaserde(rename = "BasisAmount", prefix = "ram")]
    pub ram_basis_amount: Option<String>,
    #[yaserde(rename = "ActualAmount", prefix = "ram")]
    pub ram_actual_amount: Option<String>,
    #[yaserde(rename = "ReasonCode", prefix = "ram")]
    pub ram_reason_code: Option<String>,
    #[yaserde(rename = "Reason", prefix = "ram")]
    pub ram_reason: Option<String>,
    #[yaserde(rename = "CategoryTradeTax", prefix = "ram")]
    pub ram_category_trade_tax: Option<RamCategoryTradeTax>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamApplicableHeaderTradeSettlementRamSpecifiedTradeAllowanceChargeRamChargeIndicator {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "Indicator", prefix = "udt")]
    pub udt_indicator: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamCategoryTradeTax {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "TypeCode", prefix = "ram")]
    pub ram_type_code: Option<String>,
    #[yaserde(rename = "CategoryCode", prefix = "ram")]
    pub ram_category_code: Option<String>,
    #[yaserde(rename = "RateApplicablePercent", prefix = "ram")]
    pub ram_rate_applicable_percent: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamSpecifiedTradePaymentTerms {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "Description", prefix = "ram")]
    pub ram_description: Option<String>,
    #[yaserde(rename = "DueDateDateTime", prefix = "ram")]
    pub ram_due_date_date_time: Option<RamDueDateDateTime>,
    #[yaserde(rename = "DirectDebitMandateID", prefix = "ram")]
    pub ram_direct_debit_mandate_id: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamDueDateDateTime {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "DateTimeString", prefix = "udt")]
    pub udt_date_time_string: Option<DateTimeString>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamSpecifiedTradeSettlementHeaderMonetarySummation {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "LineTotalAmount", prefix = "ram")]
    pub ram_line_total_amount: Option<String>,
    #[yaserde(rename = "ChargeTotalAmount", prefix = "ram")]
    pub ram_charge_total_amount: Option<String>,
    #[yaserde(rename = "AllowanceTotalAmount", prefix = "ram")]
    pub ram_allowance_total_amount: Option<String>,
    #[yaserde(rename = "TaxBasisTotalAmount", prefix = "ram")]
    pub ram_tax_basis_total_amount: Option<String>,
    #[yaserde(rename = "TaxTotalAmount", prefix = "ram")]
    pub ram_tax_total_amount: Vec<RamTaxTotalAmount>,
    #[yaserde(rename = "RoundingAmount", prefix = "ram")]
    pub ram_rounding_amount: Option<String>,
    #[yaserde(rename = "GrandTotalAmount", prefix = "ram")]
    pub ram_grand_total_amount: Option<String>,
    #[yaserde(rename = "TotalPrepaidAmount", prefix = "ram")]
    pub ram_total_prepaid_amount: Option<String>,
    #[yaserde(rename = "DuePayableAmount", prefix = "ram")]
    pub ram_due_payable_amount: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamTaxTotalAmount {
    #[yaserde(rename = "currencyID", attribute = true)]
    pub currency_id: Option<String>,
    #[yaserde(text = true)]
    pub text: Option<String>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamInvoiceReferencedDocument {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "IssuerAssignedID", prefix = "ram")]
    pub ram_issuer_assigned_id: Option<String>,
    #[yaserde(rename = "FormattedIssueDateTime", prefix = "ram")]
    pub ram_formatted_issue_date_time: Option<RamFormattedIssueDateTime>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamFormattedIssueDateTime {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "DateTimeString", prefix = "qdt")]
    pub qdt_date_time_string: Option<DateTimeString>,
}

#[derive(YaSerialize, YaDeserialize, uniffi::Record)]
#[yaserde(
namespaces = {
    "rsm" = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "qdt" = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
    "ram" = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt" = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
  }
)]
pub struct RamApplicableHeaderTradeSettlementRamReceivableSpecifiedTradeAccountingAccount {
    #[yaserde(text = true)]
    pub text: Option<String>,
    #[yaserde(rename = "ID", prefix = "ram")]
    pub ram_id: Option<IdType>,
}
