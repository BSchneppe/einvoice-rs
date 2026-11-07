//! Owned observations and explicit completion boundaries.
use crate::evidence::NativeCause;
use serde::Serialize;
use xsd_schema::validation::errors as backend;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Name {
    pub namespace: Option<String>,
    pub local: String,
}
impl Name {
    pub(crate) fn project(n: &backend::ExpandedName) -> Self {
        Self {
            namespace: n.namespace().map(str::to_owned),
            local: n.local_name().to_owned(),
        }
    }
    pub(crate) fn measure(
        n: &backend::ExpandedName,
        s: &mut crate::budget::Size,
    ) -> Result<(), crate::Failure> {
        s.add(std::mem::size_of::<Self>())?;
        s.add(n.namespace_uri().len())?;
        s.add(n.local_name().len())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Subject {
    Element(Name),
    Attribute(Name),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum PathStep {
    Element { name: Name, position: usize },
    Attribute { name: Name },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ComponentKind {
    SimpleType,
    ComplexType,
    Attribute,
    Element,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Component {
    pub kind: ComponentKind,
    pub name: Option<Name>,
    pub global: Option<bool>,
    pub document_uri: Option<String>,
    /// Recorded schema span, often a start tag; never an instance span.
    pub schema_span: Option<(usize, usize)>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Evidence {
    pub cause: Option<NativeCause>,
    pub governing_type: Option<Component>,
    pub actual_type: Option<Component>,
    pub component: Option<Component>,
    pub facet_origin: Option<Component>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RawLocation {
    pub base_uri: String,
    pub line: usize,
    pub column: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Span {
    /// Offsets in original UTF-8 bytes, including any BOM.
    Utf8Bytes { start: usize, end: usize },
    /// Original decoded Unicode scalars, excluding the BOM.
    UnicodeScalars { start: usize, end: usize },
}
impl From<einvoice_source::EvidenceSpan> for Span {
    fn from(s: einvoice_source::EvidenceSpan) -> Self {
        match s {
            einvoice_source::EvidenceSpan::Utf8Byte(s) => Self::Utf8Bytes {
                start: s.start,
                end: s.end,
            },
            einvoice_source::EvidenceSpan::UnicodeScalar(s) => Self::UnicodeScalars {
                start: s.start,
                end: s.end,
            },
        }
    }
}
/// Explicit document-local wire witness. Preorder counts every ordinary node
/// (including document, comments, PI and text); attributes use source axis order.
/// Its document is this Observation's sole original-byte binding. It is not a
/// serialized NodeIdentity or an identity shared with a separately parsed arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SourceReference {
    Node {
        preorder: usize,
    },
    Attribute {
        owner_preorder: usize,
        attribute: usize,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceJoin {
    pub owner: Span,
    pub owner_reference: SourceReference,
    pub subject_reference: SourceReference,
    /// Element covering span or exact attribute token span, according to subject.
    pub subject: Span,
    pub attribute_value: Option<Span>,
    pub line: u64,
    pub column: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Severity {
    Error,
    Warning,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum EvidenceAvailability {
    Captured,
    Unavailable,
    NotExportedByWarningApi,
}
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
    pub raw_location: Option<RawLocation>,
    pub subject: Option<Subject>,
    pub path: Option<Vec<PathStep>>,
    pub element_path: Option<String>,
    pub evidence: Option<Evidence>,
    pub evidence_availability: EvidenceAvailability,
    pub source: Option<SourceJoin>,
    /// Backend does not currently supply a triggering occurrence or causal parent.
    pub triggering_occurrence: Option<Vec<PathStep>>,
    pub causal_parent: Option<usize>,
}
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct Event {
    /// Zero-based ordinal in the actual combined callback sequence.
    pub ordinal: usize,
    pub severity: Severity,
    #[serde(flatten)]
    pub diagnostic: Diagnostic,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum RootValidity {
    Valid,
    Invalid,
    NotKnown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Assessment {
    Valid,
    Invalid,
    Incomplete,
}
#[derive(Debug, Serialize)]
pub struct Observation {
    pub input_sha256: Option<String>,
    pub input_bytes: usize,
    pub encoding: &'static str,
    pub bom_bytes: usize,
    /// The actual full-document element pushed as validation root.
    pub validated_root: Span,
    pub validated_root_reference: Option<SourceReference>,
    pub assessment: Assessment,
    pub root_validity: Option<RootValidity>,
    pub events: Vec<Event>,
    pub failure: Option<crate::Failure>,
    pub finalization_attempted: bool,
    pub finalization_succeeded: bool,
    pub finalization_error: Option<Diagnostic>,
    pub operational_failure: Option<Diagnostic>,
    pub terminal_capture_failure: Option<crate::Failure>,
    pub work: usize,
    pub retained_diagnostic_bytes: usize,
    /// Actual callbacks entered, including the first refused callback if any.
    pub observed_callbacks: usize,
}
