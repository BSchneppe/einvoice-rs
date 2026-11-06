//! Non-executable materialized candidate data. Deserialization is not compiler validation
//! or runtime admission. There is deliberately no execution/capability API here.
use crate::{BackendState, Edge, Expression, Location, Package, Resource, Site};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const FORMAT: &str = "einvoice-resolved-candidate/1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructuralStep {
    pub namespace: String,
    pub local: String,
    /// Zero-based sibling index among elements with this expanded name.
    pub sibling: usize,
    pub identity: BTreeMap<String, String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttributeOrigin {
    pub source: Location,
    pub sha256: String,
    pub decoded_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeOrigin {
    pub source: Location,
    pub sha256: String,
    pub attributes: BTreeMap<String, AttributeOrigin>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceNode {
    pub selector: Vec<StructuralStep>,
    pub node: NodeOrigin,
    /// Root to immediate parent, including exact ranges, attributes and hashes.
    pub ancestors: Vec<NodeOrigin>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParameterOccurrence {
    pub origin: SourceNode,
    pub expression: Expression,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolutionProvenance {
    pub decision: String,
    pub authority: String,
    pub reason: String,
    pub original_site: Site,
    pub original_context: Expression,
    pub effective: Expression,
    pub context_parameter: ParameterOccurrence,
    /// Complete ordered duplicate set; selection uses exact identity, not first-wins.
    pub duplicates: Vec<ParameterOccurrence>,
    pub selected: SourceNode,
    /// Entry, includes, abstract/concrete/raw linkage and comparison-only references.
    pub lineage: BTreeMap<String, SourceNode>,
    pub namespaces: BTreeMap<String, String>,
    pub policy_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CorrespondenceAudit {
    PendingFullPredicateContextHelperMessagePolicyCorrespondence,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedCandidate {
    pub format: String,
    pub target: String,
    pub source_package: String,
    pub raw_ir_format: String,
    pub raw_ir_sha256: String,
    /// Hash of the exact emitted fixed decision manifest, including its final LF.
    pub decision_manifest_sha256: String,
    pub effective_package: Package,
    pub resolutions: Vec<ResolutionProvenance>,
    pub added_resources: BTreeMap<String, Resource>,
    pub added_source_graph: Vec<Edge>,
    pub correspondence_audit: CorrespondenceAudit,
    pub backend: BackendState,
}
