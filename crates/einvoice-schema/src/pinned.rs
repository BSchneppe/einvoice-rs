//! Original CII D16B structural closure, reconciled with generator SOURCE-ACCOUNTING.
use crate::bundle::ResourceInfo;
pub const CONFIG_ARCHIVE_SHA256: &str =
    "2530cd107c414511c5d0462ec10f886910395abfca820db82e83d70bf01221a8";
pub const CII_RESOURCES: [ResourceInfo; 4] = [
    ResourceInfo { member: "resources/cii/16b/xsd/CrossIndustryInvoice_100pD16B.xsd", sha256: "3baf143b90f289d62c5dfb086aa51fa183490a2d0b38a40c6b31dcae0760f2ba", bytes: 3078, namespace: "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100", imports: &[
        ("urn:un:unece:uncefact:data:standard:QualifiedDataType:100", "CrossIndustryInvoice_QualifiedDataType_100pD16B.xsd"),
        ("urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100", "CrossIndustryInvoice_ReusableAggregateBusinessInformationEntity_100pD16B.xsd"),
        ("urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100", "CrossIndustryInvoice_UnqualifiedDataType_100pD16B.xsd"),
    ] },
    ResourceInfo { member: "resources/cii/16b/xsd/CrossIndustryInvoice_QualifiedDataType_100pD16B.xsd", sha256: "6649215914728652e883662bec1d3eab008c215e86a59f3bb417db9723ee8c5c", bytes: 36806, namespace: "urn:un:unece:uncefact:data:standard:QualifiedDataType:100", imports: &[
        ("urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100", "CrossIndustryInvoice_UnqualifiedDataType_100pD16B.xsd"),
    ] },
    ResourceInfo { member: "resources/cii/16b/xsd/CrossIndustryInvoice_ReusableAggregateBusinessInformationEntity_100pD16B.xsd", sha256: "1c42244298a863bcc0b5196983ff0a5d084fcabe58aea77ef82b242e7cd0ddfd", bytes: 99402, namespace: "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100", imports: &[
        ("urn:un:unece:uncefact:data:standard:QualifiedDataType:100", "CrossIndustryInvoice_QualifiedDataType_100pD16B.xsd"),
        ("urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100", "CrossIndustryInvoice_UnqualifiedDataType_100pD16B.xsd"),
    ] },
    ResourceInfo { member: "resources/cii/16b/xsd/CrossIndustryInvoice_UnqualifiedDataType_100pD16B.xsd", sha256: "04fce6f6f28521886731294be18a4e6bb5cb3003456457d99ad5c4d3e652f497", bytes: 6967, namespace: "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100", imports: &[
    ] },
];
