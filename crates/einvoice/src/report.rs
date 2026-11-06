use crate::{BundleIdentity, ImplementationIdentity, ResourceLimit};

/// One-based character position in the original XML, not a byte offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation {
    /// One-based line number.
    pub line: u64,
    /// One-based character column.
    pub column: u64,
}

/// Effective severity after applying the selected target's policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Informational finding.
    Information,
    /// Non-fatal finding.
    Warning,
    /// Rejecting finding.
    Error,
}

/// A source-based finding, distinct from engine or resource failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Rule ID from its source; facade preflight codes use the `API-` prefix.
    pub rule_id: String,
    /// Effective severity, not just the original rule flag.
    pub severity: Severity,
    /// Human-readable explanation.
    pub message: String,
    /// Stage responsible for this finding.
    pub stage: Stage,
    /// Source position when available.
    pub location: Option<SourceLocation>,
    /// Source XPath when supplied by a future rule backend.
    pub source_path: Option<String>,
}

/// Required stages of the pinned validation contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Whole-document XML syntax parsing.
    XmlSyntax,
    /// Explicit target and source profile preflight, not schema validation.
    TargetSelection,
    /// Structural validation against the complete pinned XSD set.
    Xsd,
    /// Complete CEN EN16931 rules.
    CenRules,
    /// Complete XRechnung rules.
    XrechnungRules,
    /// Pinned scenario severity overrides and acceptance decision.
    Acceptance,
}

/// Why a stage did not run. A skip never counts as completion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// A preceding stage could not complete or rejected the input.
    PreviousStageFailed(Stage),
    /// The requested target is not installed.
    UnsupportedTarget,
    /// The native conformance backend has not been implemented.
    BackendUnavailable,
}

/// Engine failures are not evidence of a bad invoice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineFailure {
    /// No native conformance backend is installed for the target.
    BackendUnavailable,
    /// DTD processing is not supported; stopped before any entity expansion.
    UnsupportedDtd,
    /// Source encoding is not supported by the bundled XML parser.
    UnsupportedEncoding {
        /// Parser explanation.
        message: String,
    },
    /// A future installed engine could not finish evaluation.
    Execution {
        /// Explanation from the engine.
        message: String,
    },
}

/// Cause of unsuccessful or incomplete validation, separate from diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationFailure {
    /// A document fault is explained by the report's diagnostics.
    Document,
    /// No fallback to another target was attempted.
    UnsupportedTarget,
    /// Validation could not be performed by the engine.
    Engine(EngineFailure),
    /// Evaluation stopped at a resource budget.
    ResourceLimit(ResourceLimit),
}

/// What happened at one mandatory stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StageStatus {
    /// The stage ran to completion. Findings still determine acceptance.
    Completed,
    /// The stage failed; subsequent stages may be skipped explicitly.
    Failed(ValidationFailure),
    /// The stage did not run.
    Skipped(SkipReason),
}

/// A stage and its completion state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageResult {
    /// Stage in the fixed validation sequence.
    pub stage: Stage,
    /// Completion state or explicit reason for not completing.
    pub status: StageStatus,
}

/// Overall decision; diagnostics alone cannot establish validity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationOutcome {
    /// Reserved for complete native validation; never returned by this release.
    Valid,
    /// A document fault was found; this does not imply all stages ran.
    InvalidDocument,
    /// The requested exact target is not installed.
    UnsupportedTarget,
    /// No engine is available, or an engine failed to finish.
    EngineFailure,
    /// A budget stopped processing; conformance is unknown.
    ResourceLimit,
}

/// Immutable validation result. Only the facade can construct a report.
/// An empty diagnostic list is not success: inspect outcome and stage completion.
#[derive(Debug, Clone)]
pub struct ValidationReport {
    pub(crate) requested_target: String,
    pub(crate) bundle: Option<BundleIdentity>,
    pub(crate) implementation: ImplementationIdentity,
    pub(crate) outcome: ValidationOutcome,
    pub(crate) failure: Option<ValidationFailure>,
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) stages: Vec<StageResult>,
}

impl ValidationReport {
    /// Exact requested target identifier, including unsupported requests.
    pub fn requested_target(&self) -> &str {
        &self.requested_target
    }
    /// Expected pinned bundle. Presence does not mean its rules ran.
    pub fn bundle(&self) -> Option<&BundleIdentity> {
        self.bundle.as_ref()
    }
    /// Actual facade/parser and optional executed backend/generator identity.
    pub fn implementation(&self) -> &ImplementationIdentity {
        &self.implementation
    }
    /// Overall decision, independent of diagnostic count.
    pub fn outcome(&self) -> ValidationOutcome {
        self.outcome
    }
    /// Failure cause, including non-document faults.
    pub fn failure(&self) -> Option<&ValidationFailure> {
        self.failure.as_ref()
    }
    /// Findings observed before validation stopped; potentially incomplete.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
    /// All mandatory stages, including skipped stages and their reasons.
    pub fn stages(&self) -> &[StageResult] {
        &self.stages
    }
    /// Whether every mandatory stage completed (not whether it accepted).
    pub fn is_complete(&self) -> bool {
        self.stages
            .iter()
            .all(|stage| stage.status == StageStatus::Completed)
    }
    /// True only for an explicit complete acceptance with an executed backend.
    /// This release always returns false.
    pub fn is_valid(&self) -> bool {
        self.outcome == ValidationOutcome::Valid
            && self.is_complete()
            && self.failure.is_none()
            && self.implementation.validator.is_some()
            && !self
                .diagnostics
                .iter()
                .any(|d| d.severity == Severity::Error)
    }
}
