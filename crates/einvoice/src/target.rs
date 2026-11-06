use crate::document::CII_NS;
use crate::{
    BuildError, Diagnostic, Document, EngineFailure, InvoiceDraft, Limits, ParseError, Severity,
    SkipReason, SourceLocation, Stage, StageResult, StageStatus, ValidationFailure,
    ValidationOutcome, ValidationReport,
};

const TARGET_ID: &str = "xrechnung-3.0.2-cii-d16b-kosit-2026-08-31";
const PROFILE_ID: &str = "urn:cen.eu:en16931:2017#compliant#urn:xeinkauf.de:kosit:xrechnung_3.0";
const STAGES: [Stage; 6] = [
    Stage::XmlSyntax,
    Stage::TargetSelection,
    Stage::Xsd,
    Stage::CenRules,
    Stage::XrechnungRules,
    Stage::Acceptance,
];

/// Exact upstream artifact pin, not proof that it was executed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactIdentity {
    /// Upstream release identifier.
    pub version: &'static str,
    /// SHA-256 of the upstream archive bytes.
    pub sha256: &'static str,
}

/// Exact expected rule/schema/policy bundle for a supported target.
/// Hashes originate from the project's pinned discovery record; this facade
/// neither ships nor downloads these artifacts. BSC-7 supplies the package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleIdentity {
    /// Complete target ID; profile version alone is insufficient.
    pub id: &'static str,
    /// XRechnung version.
    pub profile_version: &'static str,
    /// Syntax binding.
    pub syntax: &'static str,
    /// KoSIT archive pin: includes the four CII schemas and acceptance policy.
    pub kosit_configuration: ArtifactIdentity,
    /// Declarative CEN source archive pin.
    pub cen_rules: ArtifactIdentity,
    /// Declarative XRechnung source archive pin.
    pub xrechnung_rules: ArtifactIdentity,
}

/// Implementation provenance, distinct from the expected rule bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplementationIdentity {
    /// Facade crate name and exact Cargo package version.
    pub facade: &'static str,
    /// Bundled syntax parser version; see the XML stage to determine execution.
    pub xml_parser: &'static str,
    /// Executed native validation implementation; absent in this release.
    pub validator: Option<&'static str>,
    /// Generator used for an executed rule package; absent in this release.
    pub generator: Option<&'static str>,
}

/// One explicit identity controls both incoming validation and fresh encoding.
/// No format sniffing or fallback to a weaker profile is performed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    id: String,
}

impl Target {
    /// Select XRechnung 3.0.2 CII D16B with KoSIT configuration 2026-08-31.
    /// Recognition of this pin does not mean a conformance backend is available.
    pub fn xrechnung_3_0_2() -> Self {
        Self::named(TARGET_ID)
    }

    /// Select an exact ID, for example from application configuration.
    /// Unknown IDs yield unsupported-target reports/errors, never fallback.
    pub fn named(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }

    /// Complete selected target ID.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Expected pinned bundle, or None for an unsupported target.
    pub fn bundle(&self) -> Option<BundleIdentity> {
        (self.id == TARGET_ID).then_some(BundleIdentity {
            id: TARGET_ID,
            profile_version: "3.0.2",
            syntax: "CII D16B",
            kosit_configuration: ArtifactIdentity {
                version: "2026-08-31",
                sha256: "2530cd107c414511c5d0462ec10f886910395abfca820db82e83d70bf01221a8",
            },
            cen_rules: ArtifactIdentity {
                version: "1.3.16",
                sha256: "1cd53cb8a84d38aedc82c0caede217da983a7934dd663f793a092fd66443c561",
            },
            xrechnung_rules: ArtifactIdentity {
                version: "2.6.0",
                sha256: "ca5e07afd04e72cd283d581590ffff3a15f4aac9d2f40c993345d78e67ea22b4",
            },
        })
    }

    /// Validate original source bytes; malformed XML is represented in the report.
    pub fn validate(&self, bytes: impl AsRef<[u8]>) -> ValidationReport {
        self.validate_with_limits(bytes, Limits::default())
    }

    /// Validate source bytes with explicit parse budgets. Target support is
    /// checked before parsing; unsupported targets do not inspect the input.
    pub fn validate_with_limits(
        &self,
        bytes: impl AsRef<[u8]>,
        limits: Limits,
    ) -> ValidationReport {
        if self.bundle().is_none() {
            return self.unsupported_report();
        }
        match Document::parse_with_limits(bytes, limits) {
            Ok(document) => self.validate_document(&document),
            Err(error) => {
                let mut report = self.report();
                let failure = match error {
                    ParseError::XmlSyntax { message, location } => {
                        report.outcome = ValidationOutcome::InvalidDocument;
                        report.diagnostics.push(diagnostic(
                            "API-XML-SYNTAX",
                            message,
                            Stage::XmlSyntax,
                            location,
                        ));
                        ValidationFailure::Document
                    }
                    ParseError::UnsupportedDtd => {
                        report.outcome = ValidationOutcome::EngineFailure;
                        ValidationFailure::Engine(EngineFailure::UnsupportedDtd)
                    }
                    ParseError::UnsupportedEncoding { message } => {
                        report.outcome = ValidationOutcome::EngineFailure;
                        ValidationFailure::Engine(EngineFailure::UnsupportedEncoding { message })
                    }
                    ParseError::ResourceLimit(limit) => {
                        report.outcome = ValidationOutcome::ResourceLimit;
                        ValidationFailure::ResourceLimit(limit)
                    }
                };
                report.failure = Some(failure.clone());
                report.stages[0].status = StageStatus::Failed(failure);
                for stage in &mut report.stages[1..] {
                    stage.status =
                        StageStatus::Skipped(SkipReason::PreviousStageFailed(Stage::XmlSyntax));
                }
                report
            }
        }
    }

    pub(crate) fn validate_document(&self, document: &Document) -> ValidationReport {
        if self.bundle().is_none() {
            let mut report = self.unsupported_report();
            report.stages[0].status = StageStatus::Completed;
            return report;
        }
        let mut report = self.report();
        report.stages[0].status = StageStatus::Completed;
        let fault = if document.root.namespace.as_deref() != Some(CII_NS)
            || document.root.local_name != "CrossIndustryInvoice"
        {
            Some((
                "API-TARGET-ROOT",
                "Expected the CII D16B CrossIndustryInvoice root",
                document.root_location.clone(),
            ))
        } else if document.profiles.len() != 1 {
            Some((
                "API-TARGET-PROFILE",
                "Expected exactly one direct-path XRechnung profile ID",
                document.root_location.clone(),
            ))
        } else if document.profiles[0].0 != PROFILE_ID {
            Some((
                "API-TARGET-PROFILE",
                "Source profile does not match the explicitly selected XRechnung target",
                document.profiles[0].1.clone(),
            ))
        } else {
            None
        };
        if let Some((id, message, location)) = fault {
            report.outcome = ValidationOutcome::InvalidDocument;
            report.failure = Some(ValidationFailure::Document);
            report.diagnostics.push(diagnostic(
                id,
                message.into(),
                Stage::TargetSelection,
                location,
            ));
            report.stages[1].status = StageStatus::Failed(ValidationFailure::Document);
            for stage in &mut report.stages[2..] {
                stage.status =
                    StageStatus::Skipped(SkipReason::PreviousStageFailed(Stage::TargetSelection));
            }
        } else {
            report.stages[1].status = StageStatus::Completed;
            // No schema/rule/acceptance stage ran. Never substitute stub success.
        }
        report
    }

    /// Encode a fresh draft using this same target. Currently always unavailable.
    /// Later implementations must validate the actual generated XML before Ok.
    pub fn encode(&self, _draft: &InvoiceDraft) -> Result<Document, BuildError> {
        if self.bundle().is_none() {
            Err(BuildError::UnsupportedTarget {
                target: self.id.clone(),
            })
        } else {
            Err(BuildError::Unavailable {
                target: self.id.clone(),
            })
        }
    }

    fn report(&self) -> ValidationReport {
        ValidationReport {
            requested_target: self.id.clone(),
            bundle: self.bundle(),
            implementation: ImplementationIdentity {
                facade: concat!(env!("CARGO_PKG_NAME"), "@", env!("CARGO_PKG_VERSION")),
                xml_parser: "xml-rs@0.8.28+xmlparser@0.13.6",
                validator: None,
                generator: None,
            },
            outcome: ValidationOutcome::EngineFailure,
            failure: Some(ValidationFailure::Engine(EngineFailure::BackendUnavailable)),
            diagnostics: Vec::new(),
            stages: STAGES
                .into_iter()
                .map(|stage| StageResult {
                    stage,
                    status: StageStatus::Skipped(SkipReason::BackendUnavailable),
                })
                .collect(),
        }
    }

    fn unsupported_report(&self) -> ValidationReport {
        let mut report = self.report();
        report.outcome = ValidationOutcome::UnsupportedTarget;
        report.failure = Some(ValidationFailure::UnsupportedTarget);
        for stage in &mut report.stages {
            stage.status = StageStatus::Skipped(SkipReason::UnsupportedTarget);
        }
        report.stages[1].status = StageStatus::Failed(ValidationFailure::UnsupportedTarget);
        report
    }
}

fn diagnostic(id: &str, message: String, stage: Stage, location: SourceLocation) -> Diagnostic {
    Diagnostic {
        rule_id: id.into(),
        severity: Severity::Error,
        message,
        stage,
        location: Some(location),
        source_path: None,
    }
}
