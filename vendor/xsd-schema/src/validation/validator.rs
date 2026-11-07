//! Core `SchemaValidator` — immutable validation configuration.
//!
//! `SchemaValidator` holds the compiled schema reference, substitution groups,
//! and validation flags. It is reusable across multiple validation runs.
//!
//! Callers create a per-run [`super::runtime::ValidationRuntime`] via
//! [`SchemaValidator::start_run()`] to perform actual validation.

use crate::compiler::inspect::{report_from_prepared, ContentModelReport};
use crate::compiler::{build_substitution_group_map, SubstitutionGroupMap};
use crate::ids::ComplexTypeKey;
use crate::schema::SchemaSet;

use super::errors::{DiagnosticItem, InstancePath, ValidationError, ValidationSubject};
use super::info::ValidationFlags;
use super::runtime::{build_content_models, PreparedContentModels, ValidationRuntime};

// ---------------------------------------------------------------------------
// ValidationSink trait
// ---------------------------------------------------------------------------

/// Sink for validation errors and warnings
///
/// Implement this trait to receive validation messages from `SchemaValidator`.
pub trait ValidationSink {
    /// Report a validation error
    fn on_error(&mut self, error: ValidationError);
    /// Report a validation warning
    fn on_warning(&mut self, warning: ValidationWarning);

    /// Cooperatively stop this run, e.g. after refusing a diagnostic or on
    /// cancellation. Checked before and after each sink callback and at push
    /// entry/finalization. The default preserves existing sink behavior.
    /// A true response is terminal and reported out of band through
    /// `ValidationRuntime::operational_failure` and `end_validation`, never
    /// by recursively sending an error to this sink. This does not interrupt
    /// an in-progress regex, type validator or native content-model operation.
    fn stop_requested(&self) -> bool {
        false
    }
}

/// A validation warning (non-fatal)
///
/// Like [`ValidationError`], a warning the runtime reports about an instance
/// item names its [`subject`](Self::subject) and the
/// [`instance_path`](Self::instance_path) to it; both are `None` for a
/// warning about the schema rather than the instance.
///
/// The struct is `#[non_exhaustive]`: build one with
/// [`ValidationWarning::new`], then set the public fields or use the
/// `with_*` builders.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ValidationWarning {
    /// Warning code
    pub code: &'static str,
    /// Human-readable message
    pub message: String,
    /// Source location in the instance document
    pub location: Option<crate::parser::location::SourceLocation>,
    /// Subject and positional path; see [`subject`](Self::subject).
    item: Option<Box<DiagnosticItem>>,
}

impl ValidationWarning {
    /// A warning with no subject and no path.
    pub fn new(
        code: &'static str,
        message: impl Into<String>,
        location: Option<crate::parser::location::SourceLocation>,
    ) -> Self {
        ValidationWarning {
            code,
            message: message.into(),
            location,
            item: None,
        }
    }

    /// Set the [`subject`](Self::subject).
    pub fn with_subject(mut self, subject: ValidationSubject) -> Self {
        DiagnosticItem::set_subject(&mut self.item, subject);
        self
    }

    /// Set the [`instance_path`](Self::instance_path).
    pub fn with_instance_path(mut self, path: InstancePath) -> Self {
        DiagnosticItem::set_instance_path(&mut self.item, path);
        self
    }

    /// The element or attribute the warning is about, when there is one.
    pub fn subject(&self) -> Option<&ValidationSubject> {
        DiagnosticItem::subject(&self.item)
    }

    /// The namespace-aware positional path to the [`subject`](Self::subject).
    pub fn instance_path(&self) -> Option<&InstancePath> {
        DiagnosticItem::instance_path(&self.item)
    }
}

impl std::fmt::Display for ValidationWarning {
    /// `[code] message`, then ` at location` when it is known (the subject
    /// and the path are not written).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)?;
        if let Some(loc) = &self.location {
            write!(f, " at {}", loc)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Built-in sinks
// ---------------------------------------------------------------------------

/// Collects errors into a `Vec<ValidationError>` and warnings into a `Vec<ValidationWarning>`
pub struct CollectingValidationSink<'a> {
    pub errors: &'a mut Vec<ValidationError>,
    pub warnings: &'a mut Vec<ValidationWarning>,
}

impl<'a> ValidationSink for CollectingValidationSink<'a> {
    fn on_error(&mut self, error: ValidationError) {
        self.errors.push(error);
    }
    fn on_warning(&mut self, warning: ValidationWarning) {
        self.warnings.push(warning);
    }
}

/// Collects errors only; discards warnings
pub struct ErrorOnlySink<'a> {
    pub errors: &'a mut Vec<ValidationError>,
}

impl<'a> ValidationSink for ErrorOnlySink<'a> {
    fn on_error(&mut self, error: ValidationError) {
        self.errors.push(error);
    }
    fn on_warning(&mut self, _warning: ValidationWarning) {
        // Discarded
    }
}

// ---------------------------------------------------------------------------
// AssertionSource — mutual exclusion for assertion evaluation paths
// ---------------------------------------------------------------------------

/// Selects which assertion evaluation path is active.
///
/// - `FragmentBuffer` — XSD 1.1 assertions evaluated by inline fragment
///   buffering during streaming validation
/// - `MainDocument` — a placeholder for evaluating them against an external
///   `BufferDocument`; not implemented: nothing evaluates assertions in this
///   mode
///
/// The `Disabled` default means no assertion evaluation occurs.
#[cfg(feature = "xsd11")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AssertionSource {
    /// No assertion evaluation. `PROCESS_ASSERTIONS` must NOT be set.
    #[default]
    Disabled,
    /// Inline fragment buffering. `PROCESS_ASSERTIONS` MUST be set.
    FragmentBuffer,
    /// Placeholder for an external `BufferDocument` (no assertion is
    /// evaluated). `PROCESS_ASSERTIONS` must NOT be set.
    MainDocument,
}

// ---------------------------------------------------------------------------
// SchemaValidator — immutable configuration
// ---------------------------------------------------------------------------

/// Immutable validation configuration — reusable across runs.
///
/// Holds the compiled schema reference, substitution groups, and validation
/// flags. Create a per-run [`ValidationRuntime`] via [`Self::start_run()`].
pub struct SchemaValidator<'a> {
    /// The compiled schema set to validate against
    pub(crate) schema_set: &'a SchemaSet,
    /// Pre-built substitution group map (if any)
    pub(crate) subst_groups: Option<SubstitutionGroupMap>,
    /// Per-complex-type compiled content models, built once here and shared
    /// (borrowed) by every [`ValidationRuntime`]. Moves the content-model NFA
    /// compilation out of the per-element hot path. See `build_content_models`.
    pub(crate) content_models: PreparedContentModels,
    /// Validation flags controlling behaviour
    pub(crate) flags: ValidationFlags,
    /// Which assertion evaluation path is active (XSD 1.1 only)
    #[cfg(feature = "xsd11")]
    pub(crate) assertion_source: AssertionSource,
}

impl<'a> SchemaValidator<'a> {
    /// Create a new `SchemaValidator` for `schema_set` with `flags`.
    ///
    /// Under the `xsd11` feature the assertion mode follows `flags`:
    ///
    /// - with `ValidationFlags::PROCESS_ASSERTIONS` set, the XSD 1.1
    ///   assertions of complex types (`xs:assert`) are evaluated inline while
    ///   streaming, by buffering each asserted element's subtree — the mode
    ///   `new_fragment_buffer` selects;
    /// - without it, they are not evaluated: an asserted element's
    ///   `SchemaInfo::assertion_outcome` is `AssertionOutcome::NotEvaluated`.
    ///
    /// The `assertion` facet of simple types is part of simple-type
    /// validation and is evaluated either way. The flag is never dropped;
    /// only `new_main_document`, which selects a different assertion mode,
    /// removes it.
    pub fn new(schema_set: &'a SchemaSet, flags: ValidationFlags) -> Self {
        let subst_groups = build_substitution_group_map(schema_set);
        let content_models = build_content_models(schema_set);
        SchemaValidator {
            schema_set,
            // The builder prunes trivial self-match entries, so the map is empty —
            // and dropped to `None` here — whenever the schema has no substitutable
            // members and no abstract elements (the common case). `term_matches`
            // short-circuits on `None` and falls through to the identical direct
            // self-match, eliminating the per-term hash probes on every child.
            subst_groups: (!subst_groups.is_empty()).then_some(subst_groups),
            content_models,
            flags,
            // The flag and the mode always agree (the runtime asserts it at
            // the first element): the flag selects inline evaluation.
            #[cfg(feature = "xsd11")]
            assertion_source: if flags.contains(ValidationFlags::PROCESS_ASSERTIONS) {
                AssertionSource::FragmentBuffer
            } else {
                AssertionSource::Disabled
            },
        }
    }

    /// Complex types whose content model could **not** be prepared at
    /// construction, with the reason (an NFA compilation error, or an
    /// execution limit hit while computing the model's initial state).
    ///
    /// Such a type is not silently treated as empty content: the first
    /// element governed by it raises an operational failure
    /// (`validation-preparation-failed`) that aborts the run and is returned
    /// from `end_validation`. Callers that want to fail *before* validating
    /// can check this list up front; it is empty for every schema the W3C
    /// suites contain.
    pub fn content_model_failures(&self) -> Vec<(ComplexTypeKey, &str)> {
        self.content_models
            .failures
            .iter()
            .map(|(k, reason)| (*k, reason.as_str()))
            .collect()
    }

    /// A readable, source-attributed description of the content model this
    /// validator holds for `ct_key` — see
    /// [`ContentModelReport`].
    ///
    /// Unlike
    /// [`inspect_content_model`](crate::compiler::inspect::inspect_content_model),
    /// which recompiles, this reads the model **already prepared** at
    /// construction, so the compiled view is the exact object validation
    /// executes. For a type listed in
    /// [`content_model_failures`](Self::content_model_failures) the compiled
    /// view states the preparation failure and its reason instead. For a type
    /// whose content type is neither element-only nor mixed — no model is
    /// prepared for those — the model is compiled on demand and the view says
    /// so.
    ///
    /// Returns `None` when `ct_key` does not belong to this validator's schema
    /// set.
    pub fn describe_content_model(&self, ct_key: ComplexTypeKey) -> Option<ContentModelReport> {
        report_from_prepared(
            self.schema_set,
            ct_key,
            self.content_models.models.get(&ct_key),
            self.content_models
                .failures
                .get(&ct_key)
                .map(|reason| reason.as_str()),
            self.subst_groups.as_ref(),
        )
    }

    /// Create a new `SchemaValidator` with pre-built substitution groups.
    ///
    /// `flags` are handled as by [`new`](Self::new), `PROCESS_ASSERTIONS`
    /// included.
    pub fn with_substitution_groups(
        schema_set: &'a SchemaSet,
        flags: ValidationFlags,
        subst_groups: SubstitutionGroupMap,
    ) -> Self {
        SchemaValidator {
            subst_groups: Some(subst_groups),
            ..Self::new(schema_set, flags)
        }
    }

    /// XSD 1.1: a validator that evaluates assertions inline while streaming
    /// (the `FragmentBuffer` mode), whether or not `flags` contains
    /// `PROCESS_ASSERTIONS` — the flag is added.
    ///
    /// Equivalent to [`new`](Self::new) with
    /// `flags | ValidationFlags::PROCESS_ASSERTIONS`.
    #[cfg(feature = "xsd11")]
    pub fn new_fragment_buffer(schema_set: &'a SchemaSet, flags: ValidationFlags) -> Self {
        Self::new(schema_set, flags | ValidationFlags::PROCESS_ASSERTIONS)
    }

    /// XSD 1.1: a validator in the `MainDocument` assertion mode, in which
    /// **no assertion is evaluated**: neither the runtime nor any other entry
    /// point of this crate evaluates `xs:assert` for it, so an instance that
    /// violates an assertion validates, and an asserted element reports
    /// [`AssertionOutcome::NotEvaluated`](super::AssertionOutcome::NotEvaluated).
    /// (The mode is a placeholder for evaluating assertions against an
    /// external [`BufferDocument`](crate::document::BufferDocument), which is
    /// not implemented.)
    ///
    /// `PROCESS_ASSERTIONS` is removed from `flags`. To evaluate assertions,
    /// use [`new`](Self::new) with the flag, or
    /// [`new_fragment_buffer`](Self::new_fragment_buffer).
    #[cfg(feature = "xsd11")]
    pub fn new_main_document(schema_set: &'a SchemaSet, flags: ValidationFlags) -> Self {
        let flags = flags & !ValidationFlags::PROCESS_ASSERTIONS;
        let mut v = Self::new(schema_set, flags);
        v.assertion_source = AssertionSource::MainDocument;
        v
    }

    /// Set the assertion evaluation source.
    ///
    /// Enforces the mutual exclusion contract between `PROCESS_ASSERTIONS`
    /// flag and the chosen `AssertionSource` mode:
    /// - `FragmentBuffer` requires `PROCESS_ASSERTIONS` to be set
    /// - `Disabled` and `MainDocument` require it to NOT be set
    ///
    /// # Panics
    /// Panics if the flag/mode combination is invalid.
    #[cfg(feature = "xsd11")]
    #[allow(dead_code)] // Future non-test callers will use this
    pub(crate) fn set_assertion_source(&mut self, source: AssertionSource) -> &mut Self {
        let has_flag = self.flags.contains(ValidationFlags::PROCESS_ASSERTIONS);
        match source {
            AssertionSource::FragmentBuffer => {
                assert!(
                    has_flag,
                    "AssertionSource::FragmentBuffer requires ValidationFlags::PROCESS_ASSERTIONS"
                );
            }
            AssertionSource::Disabled | AssertionSource::MainDocument => {
                assert!(
                    !has_flag,
                    "AssertionSource::{:?} requires PROCESS_ASSERTIONS to NOT be set",
                    source
                );
            }
        }
        self.assertion_source = source;
        self
    }

    /// Create a mutable runtime for one validation pass.
    pub fn start_run<S: ValidationSink>(&self, sink: S) -> ValidationRuntime<'_, S> {
        ValidationRuntime::new(
            self.schema_set,
            &self.subst_groups,
            &self.content_models,
            self.flags,
            sink,
            #[cfg(feature = "xsd11")]
            self.assertion_source,
        )
    }
}

// ---------------------------------------------------------------------------
// Tests (config-only — no validation method calls)
// ---------------------------------------------------------------------------

#[cfg(test)]
#[cfg(feature = "xsd11")]
mod assertion_source_tests {
    use super::*;
    use crate::pipeline::load_and_process_schema;

    fn load_schema(xsd: &str) -> SchemaSet {
        let mut schema_set = SchemaSet::new();
        load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None)
            .expect("failed to load schema");
        schema_set
    }

    #[test]
    fn test_assertion_source_default_is_disabled() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root" type="xs:string"/>
            </xs:schema>"#,
        );
        let v = SchemaValidator::new(&schema_set, ValidationFlags::default());
        assert_eq!(v.assertion_source, AssertionSource::Disabled);
    }

    #[test]
    fn test_fragment_buffer_constructor() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root" type="xs:string"/>
            </xs:schema>"#,
        );
        let v = SchemaValidator::new_fragment_buffer(&schema_set, ValidationFlags::default());
        assert_eq!(v.assertion_source, AssertionSource::FragmentBuffer);
        assert!(v.flags.contains(ValidationFlags::PROCESS_ASSERTIONS));
    }

    #[test]
    fn test_new_honours_process_assertions_flag() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root" type="xs:string"/>
            </xs:schema>"#,
        );
        // PROCESS_ASSERTIONS passed to new() selects the inline
        // (fragment-buffer) assertion mode, as new_fragment_buffer() does.
        let flags = ValidationFlags::default() | ValidationFlags::PROCESS_ASSERTIONS;
        let v = SchemaValidator::new(&schema_set, flags);
        assert!(v.flags.contains(ValidationFlags::PROCESS_ASSERTIONS));
        assert_eq!(v.assertion_source, AssertionSource::FragmentBuffer);

        // with_substitution_groups is built on new() and agrees with it.
        let groups = build_substitution_group_map(&schema_set);
        let v = SchemaValidator::with_substitution_groups(&schema_set, flags, groups);
        assert!(v.flags.contains(ValidationFlags::PROCESS_ASSERTIONS));
        assert_eq!(v.assertion_source, AssertionSource::FragmentBuffer);

        // new_main_document() still clears the flag: that mode evaluates
        // assertions against an external document, never inline.
        let v = SchemaValidator::new_main_document(&schema_set, flags);
        assert!(!v.flags.contains(ValidationFlags::PROCESS_ASSERTIONS));
        assert_eq!(v.assertion_source, AssertionSource::MainDocument);
    }

    #[test]
    #[should_panic(expected = "PROCESS_ASSERTIONS")]
    fn test_fragment_buffer_without_flag_panics() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root" type="xs:string"/>
            </xs:schema>"#,
        );
        let mut v = SchemaValidator::new(&schema_set, ValidationFlags::default());
        v.set_assertion_source(AssertionSource::FragmentBuffer);
    }

    #[test]
    #[should_panic(expected = "PROCESS_ASSERTIONS")]
    fn test_disabled_with_flag_panics() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root" type="xs:string"/>
            </xs:schema>"#,
        );
        // Use new_fragment_buffer to get PROCESS_ASSERTIONS set, then
        // attempt to switch to Disabled — should panic.
        let mut v = SchemaValidator::new_fragment_buffer(&schema_set, ValidationFlags::default());
        v.set_assertion_source(AssertionSource::Disabled);
    }

    #[test]
    #[should_panic(expected = "PROCESS_ASSERTIONS")]
    fn test_main_document_with_flag_panics() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root" type="xs:string"/>
            </xs:schema>"#,
        );
        // Use new_fragment_buffer to get PROCESS_ASSERTIONS set, then
        // attempt to switch to MainDocument — should panic.
        let mut v = SchemaValidator::new_fragment_buffer(&schema_set, ValidationFlags::default());
        v.set_assertion_source(AssertionSource::MainDocument);
    }

    #[test]
    fn test_main_document_without_flag_ok() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root" type="xs:string"/>
            </xs:schema>"#,
        );
        let mut v = SchemaValidator::new(&schema_set, ValidationFlags::default());
        v.set_assertion_source(AssertionSource::MainDocument);
        assert_eq!(v.assertion_source, AssertionSource::MainDocument);
    }
}
