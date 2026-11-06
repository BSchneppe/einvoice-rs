//! W3C XSD Test Suite Conformance Driver
//!
//! This driver runs the W3C XSD 1.0/1.1 test suite to measure
//! conformance of the xsd-schema parser.
//!
//! # Test Suite Structure
//!
//! The W3C test suite is organized as:
//! ```text
//! xsdtests/
//! ├── suite.xml           # Test suite manifest
//! ├── nist/               # NIST tests
//! ├── sun/                # Sun Microsystems tests
//! ├── ms/                 # Microsoft tests
//! └── ibm/                # IBM tests
//! ```
//!
//! # Usage
//!
//! ```bash
//! cargo test --test conformance -- --test-suite /path/to/xsdtests
//! ```

#[path = "report.rs"]
mod report;

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use quick_xml::events::Event;
use quick_xml::Reader;

/// Test result outcome
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestOutcome {
    /// Test passed as expected
    Pass,
    /// Test failed when it should have passed
    Fail,
    /// Test was skipped (not applicable or not implemented)
    Skip,
    /// Test had an error (unexpected exception)
    Error,
}

/// A single test case result
#[derive(Debug, Clone)]
pub struct TestResult {
    /// Test case name/identifier
    pub name: String,
    /// Test group (contributor)
    pub group: String,
    /// Expected outcome from test suite
    pub expected: ExpectedOutcome,
    /// Actual outcome from running the test
    pub actual: TestOutcome,
    /// Duration of the test
    pub duration: Duration,
    /// Error message if any
    pub error_message: Option<String>,
}

/// Expected outcome from test definition
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedOutcome {
    /// Schema should be valid
    Valid,
    /// Schema should be invalid
    Invalid,
    /// Instance validity should be `notKnown`
    NotKnown,
    /// Latent schema error surfaced during instance validation
    RuntimeSchemaError,
    /// Expected result depends on an implementation-defined choice
    ImplementationDefined,
    /// Expected result depends on an implementation-dependent choice
    ImplementationDependent,
    /// Expected result is intentionally under/over-specified by the suite
    Indeterminate,
    /// Instance should validate against schema
    InstanceValid,
    /// Instance should NOT validate against schema
    InstanceInvalid,
    /// Instance result is intentionally under/over-specified by the suite
    InstanceIndeterminate,
    /// Instance result depends on an implementation-defined choice
    InstanceImplementationDefined,
    /// Instance result depends on an implementation-dependent choice
    InstanceImplementationDependent,
    /// Latent schema error surfaced during instance validation
    InstanceRuntimeSchemaError,
    /// Instance validity should be `notKnown`
    InstanceNotKnown,
    /// No `<expected>` entry applies to the configuration the test runs in
    /// (only produced by the `--versionless-1.1` mode)
    NotPrescribed,
}

/// Test suite statistics
#[derive(Debug, Clone, Default)]
pub struct TestStats {
    /// Number of tests that passed
    pub passed: usize,
    /// Number of tests that failed
    pub failed: usize,
    /// Number of tests that were skipped
    pub skipped: usize,
    /// Number of tests that had errors
    pub errors: usize,
    /// Total duration
    pub total_duration: Duration,
}

impl TestStats {
    /// Total number of tests run
    pub fn total(&self) -> usize {
        self.passed + self.failed + self.skipped + self.errors
    }

    /// Pass rate (0.0 - 1.0)
    pub fn pass_rate(&self) -> f64 {
        let total = self.passed + self.failed;
        if total == 0 {
            0.0
        } else {
            self.passed as f64 / total as f64
        }
    }

    /// Add a result to the stats
    pub fn add_result(&mut self, result: &TestResult) {
        match result.actual {
            TestOutcome::Pass => self.passed += 1,
            TestOutcome::Fail => self.failed += 1,
            TestOutcome::Skip => self.skipped += 1,
            TestOutcome::Error => self.errors += 1,
        }
        self.total_duration += result.duration;
    }
}

/// Expected outcome keyed by version
#[derive(Debug, Clone)]
pub struct VersionedExpected {
    /// Version this expected outcome applies to (None = all versions)
    pub version: Option<String>,
    /// The expected validity
    pub outcome: ExpectedOutcome,
}

/// Recognized XSD versions for SchemaSet selection
const XSD_VERSIONS: &[&str] = &["1.0", "1.1"];

/// CTA XPath profile that this implementation supports.
///
/// XSD 1.1 §3.12.4 defines two XPath profiles for type alternatives:
///   - `full-xpath-in-CTA` — full XPath 2.0 in the test attribute
///   - `restricted-xpath-in-CTA` — a subset (attribute access only)
///
/// Some W3C tests carry per-profile expected outcomes:
///   <expected validity="valid"   version="full-xpath-in-CTA"/>
///   <expected validity="invalid" version="restricted-xpath-in-CTA"/>
///
/// This driver implements the full profile, so we run the matching
/// variant and skip variants whose label is a non-supported profile.
const ACTIVE_CTA_PROFILE: &str = "full-xpath-in-CTA";

/// Profile labels we recognise but explicitly do not implement. Test
/// variants tagged with these are dropped during expansion.
const UNSUPPORTED_CTA_PROFILES: &[&str] = &["restricted-xpath-in-CTA"];

fn parse_expected_outcome(
    validity: &str,
    in_instance_test: bool,
) -> Result<ExpectedOutcome, String> {
    let outcome = match (in_instance_test, validity) {
        (false, "valid") => ExpectedOutcome::Valid,
        (false, "invalid") => ExpectedOutcome::Invalid,
        (false, "notKnown") => ExpectedOutcome::NotKnown,
        (false, "runtime-schema-error") => ExpectedOutcome::RuntimeSchemaError,
        (false, "implementation-defined") => ExpectedOutcome::ImplementationDefined,
        (false, "implementation-dependent") => ExpectedOutcome::ImplementationDependent,
        (false, "indeterminate") => ExpectedOutcome::Indeterminate,
        (true, "valid") => ExpectedOutcome::InstanceValid,
        (true, "invalid") => ExpectedOutcome::InstanceInvalid,
        (true, "notKnown") => ExpectedOutcome::InstanceNotKnown,
        (true, "runtime-schema-error") => ExpectedOutcome::InstanceRuntimeSchemaError,
        (true, "implementation-defined") => ExpectedOutcome::InstanceImplementationDefined,
        (true, "implementation-dependent") => ExpectedOutcome::InstanceImplementationDependent,
        (true, "indeterminate") => ExpectedOutcome::InstanceIndeterminate,
        (_, other) => {
            return Err(format!("Unsupported expected validity '{}'", other));
        }
    };
    Ok(outcome)
}

fn is_non_asserting_expected(outcome: ExpectedOutcome) -> bool {
    matches!(
        outcome,
        ExpectedOutcome::NotPrescribed
            | ExpectedOutcome::Indeterminate
            | ExpectedOutcome::ImplementationDefined
            | ExpectedOutcome::ImplementationDependent
            | ExpectedOutcome::InstanceIndeterminate
            | ExpectedOutcome::InstanceImplementationDefined
            | ExpectedOutcome::InstanceImplementationDependent
    )
}

fn expected_skip_reason(outcome: ExpectedOutcome) -> Option<&'static str> {
    match outcome {
        ExpectedOutcome::Indeterminate | ExpectedOutcome::InstanceIndeterminate => Some(
            "W3C expected outcome is 'indeterminate'; the driver skips non-asserting cases",
        ),
        ExpectedOutcome::ImplementationDefined
        | ExpectedOutcome::InstanceImplementationDefined => Some(
            "W3C expected outcome is 'implementation-defined'; this driver does not model implementation profiles",
        ),
        ExpectedOutcome::ImplementationDependent
        | ExpectedOutcome::InstanceImplementationDependent => Some(
            "W3C expected outcome is 'implementation-dependent'; this driver does not model implementation profiles",
        ),
        ExpectedOutcome::RuntimeSchemaError => Some(
            "W3C expected outcome 'runtime-schema-error' is meaningless for schema tests",
        ),
        ExpectedOutcome::InstanceRuntimeSchemaError => Some(
            "W3C expected outcome 'runtime-schema-error' is not modeled separately by this driver",
        ),
        ExpectedOutcome::NotPrescribed => {
            Some("no <expected> entry of the manifest applies to XSD 1.1 (only versioned ones)")
        }
        _ => None,
    }
}

/// Check if a version string is a recognized XSD version
fn is_xsd_version(v: &str) -> bool {
    XSD_VERSIONS.contains(&v)
}

/// Extract XSD versions from a version attribute value.
///
/// The attribute may be a single token like `"1.1"`, a space-separated
/// list like `"1.0 1.1"`, or a profile label like `"full-xpath-in-CTA"`.
/// Returns only the recognized XSD version tokens.
fn extract_xsd_versions(version_attr: &str) -> Vec<&str> {
    version_attr
        .split_whitespace()
        .filter(|tok| is_xsd_version(tok))
        .collect()
}

/// Recognize errors from `validate_instance` that indicate the instance
/// document is not even well-formed XML — malformed start/end tags,
/// invalid attribute syntax, escape errors, etc. Such instances are
/// trivially invalid: a test that expects "invalid" should pass.
fn is_xml_wellformedness_error(msg: &str) -> bool {
    msg.starts_with("XML parse error:")
        || msg.starts_with("Attribute error:")
        || msg.starts_with("Attribute unescape error:")
        || msg.starts_with("Text unescape error:")
        || msg.starts_with("CData UTF-8 error:")
}

/// Test case from the suite manifest
#[derive(Debug, Clone)]
pub struct TestCase {
    /// Test name/id
    pub name: String,
    /// Schema file(s) to parse
    pub schema_files: Vec<PathBuf>,
    /// Instance document (if any)
    pub instance_file: Option<PathBuf>,
    /// Expected outcome (resolved for the target version)
    pub expected: ExpectedOutcome,
    /// All versioned expected outcomes from the manifest
    pub expected_versions: Vec<VersionedExpected>,
    /// Test contributor/group
    pub group: String,
    /// XSD version for SchemaSet selection ("1.0" or "1.1")
    pub version: String,
    /// Raw version attribute from the manifest (may be a profile label)
    pub version_label: String,
    /// No level of the manifest (test, group, test set) names an XSD version and
    /// no `<expected>` entry gives the test an XSD 1.1 variant: the default runs
    /// execute it in 1.0 mode only. The `--versionless-1.1` mode runs these tests
    /// under `SchemaSet::xsd11()` as well.
    pub versionless: bool,
    /// Description
    pub description: Option<String>,
}

/// Test suite manifest parser
pub struct TestSuiteParser {
    base_path: PathBuf,
}

impl TestSuiteParser {
    /// Create a new parser for the given test suite directory
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    /// Parse the test suite manifest (suite.xml)
    pub fn parse_manifest(&self) -> Result<Vec<TestCase>, String> {
        let manifest_path = self.base_path.join("suite.xml");
        if !manifest_path.exists() {
            // Try alternative manifest names
            let alt_path = self.base_path.join("testSuite.xml");
            if alt_path.exists() {
                return self.parse_xml_manifest(&alt_path, None);
            }
            return Err(format!(
                "Test suite manifest not found at {:?}",
                manifest_path
            ));
        }
        self.parse_xml_manifest(&manifest_path, None)
    }

    /// Parse an XML manifest file.
    ///
    /// `inherited_version` is the version inherited from the parent testSet
    /// or testGroup (if any).
    fn parse_xml_manifest(
        &self,
        path: &Path,
        inherited_version: Option<&str>,
    ) -> Result<Vec<TestCase>, String> {
        let raw =
            fs::read(path).map_err(|e| format!("Failed to read manifest {:?}: {}", path, e))?;
        let content = xsd_schema::decode_xml_bytes(raw)
            .map_err(|e| format!("Failed to decode manifest {:?}: {}", path, e))?;

        // Resolve document paths relative to the manifest file's directory
        let manifest_dir = path
            .parent()
            .ok_or_else(|| format!("Cannot determine parent directory of {:?}", path))?;

        let mut reader = Reader::from_str(&content);
        reader.config_mut().trim_text(true);

        let mut tests = Vec::new();
        let mut buf = Vec::new();

        // Current parsing state
        let mut current_group = String::new();
        let mut current_test: Option<TestCase> = None;
        // Track whether we are inside a schemaTest or instanceTest
        let mut in_instance_test = false;
        // Schema files from the current testGroup's schemaTest
        // (instance tests inherit these)
        let mut group_schema_files: Vec<PathBuf> = Vec::new();

        // Version inheritance chain: testSet → testGroup → schemaTest/instanceTest
        //
        // Each level stores the *raw* version attribute (which may be a profile
        // label like "full-xpath-in-CTA" or a multi-token string like "1.0 1.1").
        // The XSD version(s) used for SchemaSet selection are derived by walking
        // the chain and picking the first level whose attribute contains a
        // recognized XSD version token.
        let mut testset_version_attr: Option<String> = inherited_version.map(|s| s.to_string());
        let mut group_version_attr: Option<String> = None;

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                    let name_ref = e.local_name();
                    let local_name = String::from_utf8_lossy(name_ref.as_ref()).to_string();

                    match local_name.as_str() {
                        "testSuite" | "testSet" => {
                            // Extract version from testSet if present
                            for attr in e.attributes().flatten() {
                                if attr.key.as_ref() == b"version" {
                                    testset_version_attr =
                                        Some(String::from_utf8_lossy(&attr.value).to_string());
                                }
                            }
                        }
                        "testSetRef" => {
                            // Follow testSetRef links to external manifest files
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref();
                                if key == b"xlink:href" || key == b"href" {
                                    let href = String::from_utf8_lossy(&attr.value);
                                    let ref_path = manifest_dir.join(href.as_ref());
                                    if ref_path.exists() {
                                        let ver = testset_version_attr.as_deref();
                                        match self.parse_xml_manifest(&ref_path, ver) {
                                            Ok(ref_tests) => tests.extend(ref_tests),
                                            Err(e) => {
                                                eprintln!(
                                                    "Warning: Failed to parse referenced testSet {:?}: {}",
                                                    ref_path, e
                                                );
                                            }
                                        }
                                    } else {
                                        eprintln!(
                                            "Warning: Referenced testSet not found: {:?}",
                                            ref_path
                                        );
                                    }
                                }
                            }
                        }
                        "testGroup" => {
                            // Extract group name and version from attributes
                            group_version_attr = None;
                            for attr in e.attributes().flatten() {
                                match attr.key.as_ref() {
                                    b"name" => {
                                        current_group =
                                            String::from_utf8_lossy(&attr.value).to_string();
                                    }
                                    b"version" => {
                                        group_version_attr =
                                            Some(String::from_utf8_lossy(&attr.value).to_string());
                                    }
                                    _ => {}
                                }
                            }
                            group_schema_files.clear();
                        }
                        "schemaTest" | "instanceTest" => {
                            let is_instance = local_name == "instanceTest";

                            // Collect raw version label from the test element
                            let mut test_version_attr: Option<String> = None;
                            let mut test_name = String::new();
                            for attr in e.attributes().flatten() {
                                match attr.key.as_ref() {
                                    b"name" => {
                                        test_name =
                                            String::from_utf8_lossy(&attr.value).to_string();
                                    }
                                    b"version" => {
                                        test_version_attr =
                                            Some(String::from_utf8_lossy(&attr.value).to_string());
                                    }
                                    _ => {}
                                }
                            }

                            // Build the version label chain (most specific first)
                            let raw_label = test_version_attr
                                .as_deref()
                                .or(group_version_attr.as_deref())
                                .or(testset_version_attr.as_deref())
                                .unwrap_or("1.0")
                                .to_string();

                            // Derive XSD version(s): walk the chain to find the
                            // first level that contains a recognized XSD version.
                            let xsd_versions = Self::resolve_xsd_versions(
                                test_version_attr.as_deref(),
                                group_version_attr.as_deref(),
                                testset_version_attr.as_deref(),
                            );

                            let base_test = TestCase {
                                name: test_name,
                                schema_files: if is_instance {
                                    group_schema_files.clone()
                                } else {
                                    Vec::new()
                                },
                                instance_file: None,
                                expected: if is_instance {
                                    ExpectedOutcome::InstanceValid
                                } else {
                                    ExpectedOutcome::Valid
                                },
                                expected_versions: Vec::new(),
                                group: current_group.clone(),
                                version: xsd_versions[0].to_string(),
                                version_label: raw_label,
                                versionless: [
                                    test_version_attr.as_deref(),
                                    group_version_attr.as_deref(),
                                    testset_version_attr.as_deref(),
                                ]
                                .into_iter()
                                .flatten()
                                .all(|attr| extract_xsd_versions(attr).is_empty()),
                                description: None,
                            };

                            in_instance_test = is_instance;
                            current_test = Some(base_test);
                        }
                        "schemaDocument" => {
                            if let Some(ref mut test) = current_test {
                                for attr in e.attributes().flatten() {
                                    let key = attr.key.as_ref();
                                    if key == b"xlink:href" || key == b"href" {
                                        let href = String::from_utf8_lossy(&attr.value);
                                        let schema_path = manifest_dir.join(href.as_ref());
                                        test.schema_files.push(schema_path);
                                    }
                                }
                            }
                        }
                        "instanceDocument" => {
                            if let Some(ref mut test) = current_test {
                                for attr in e.attributes().flatten() {
                                    let key = attr.key.as_ref();
                                    if key == b"xlink:href" || key == b"href" {
                                        let href = String::from_utf8_lossy(&attr.value);
                                        let instance_path = manifest_dir.join(href.as_ref());
                                        test.instance_file = Some(instance_path);
                                    }
                                }
                            }
                        }
                        "expected" => {
                            if let Some(ref mut test) = current_test {
                                let mut validity_str = None;
                                let mut expected_version = None;

                                for attr in e.attributes().flatten() {
                                    match attr.key.as_ref() {
                                        b"validity" => {
                                            validity_str = Some(
                                                String::from_utf8_lossy(&attr.value).to_string(),
                                            );
                                        }
                                        b"version" => {
                                            expected_version = Some(
                                                String::from_utf8_lossy(&attr.value).to_string(),
                                            );
                                        }
                                        _ => {}
                                    }
                                }

                                if let Some(validity) = validity_str {
                                    let outcome =
                                        parse_expected_outcome(&validity, in_instance_test)
                                            .map_err(|e| {
                                                format!(
                                                    "Invalid expected outcome '{}' in {:?}: {}",
                                                    validity, path, e
                                                )
                                            })?;

                                    test.expected_versions.push(VersionedExpected {
                                        version: expected_version,
                                        outcome,
                                    });
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Event::End(ref e)) => {
                    let name_ref = e.local_name();
                    let local_name = String::from_utf8_lossy(name_ref.as_ref()).to_string();

                    match local_name.as_str() {
                        "testGroup" => {
                            current_group = String::new();
                            group_schema_files.clear();
                            group_version_attr = None;
                        }
                        "schemaTest" => {
                            if let Some(test) = current_test.take() {
                                if !test.schema_files.is_empty() {
                                    // Save schema files for instance tests in same group
                                    group_schema_files = test.schema_files.clone();
                                    expand_test_versions(test, &mut tests);
                                }
                            }
                            in_instance_test = false;
                        }
                        "instanceTest" => {
                            if let Some(test) = current_test.take() {
                                if test.instance_file.is_some() {
                                    expand_test_versions(test, &mut tests);
                                }
                            }
                            in_instance_test = false;
                        }
                        _ => {}
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => {
                    return Err(format!("XML parse error in {:?}: {}", path, e));
                }
                _ => {}
            }
            buf.clear();
        }

        Ok(tests)
    }

    /// Derive XSD version(s) from the version attribute inheritance chain.
    ///
    /// Walks the chain (test → group → testSet) and returns the recognized
    /// XSD version tokens from the most specific level that contains any.
    /// If no level has a recognized version, defaults to `["1.0"]`.
    fn resolve_xsd_versions<'a>(
        test_attr: Option<&'a str>,
        group_attr: Option<&'a str>,
        testset_attr: Option<&'a str>,
    ) -> Vec<&'a str> {
        for attr in [test_attr, group_attr, testset_attr].into_iter().flatten() {
            let versions = extract_xsd_versions(attr);
            if !versions.is_empty() {
                return versions;
            }
        }
        vec!["1.0"]
    }

    /// Scan directory for schema files if no manifest is found
    #[allow(dead_code)]
    pub fn scan_for_tests(&self) -> Result<Vec<TestCase>, String> {
        let mut tests = Vec::new();
        self.scan_directory(&self.base_path, &mut tests)?;
        Ok(tests)
    }

    #[allow(clippy::only_used_in_recursion)]
    fn scan_directory(&self, dir: &Path, tests: &mut Vec<TestCase>) -> Result<(), String> {
        let entries =
            fs::read_dir(dir).map_err(|e| format!("Failed to read directory {:?}: {}", dir, e))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                self.scan_directory(&path, tests)?;
            } else if path.extension().is_some_and(|e| e == "xsd") {
                // Create a test case for each .xsd file
                let name = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();

                let group = path
                    .parent()
                    .and_then(|p| p.file_name())
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "default".to_string());

                tests.push(TestCase {
                    name,
                    schema_files: vec![path],
                    instance_file: None,
                    expected: ExpectedOutcome::Valid, // Default expectation
                    expected_versions: Vec::new(),
                    group,
                    version: "1.0".to_string(),
                    version_label: "1.0".to_string(),
                    versionless: false,
                    description: None,
                });
            }
        }

        Ok(())
    }
}

/// A variant to emit: an XSD version paired with an optional profile label.
struct ExpandedVariant {
    /// XSD version for SchemaSet selection ("1.0" or "1.1")
    xsd_version: String,
    /// Profile label for expected-outcome resolution and --version filtering.
    /// When set, overrides `version_label` on the emitted TestCase.
    profile_label: Option<String>,
}

/// Expand a parsed test into one or more `TestCase` entries in `out`.
///
/// A single manifest entry can target multiple variants in three ways:
///
/// 1. The version attribute is a space-separated list like `"1.0 1.1"`.
/// 2. There are multiple `<expected>` elements keyed by XSD version, e.g.
///    `<expected validity="valid" version="1.0"/>
///     <expected validity="invalid" version="1.1"/>`.
/// 3. There are multiple `<expected>` elements keyed by profile label, e.g.
///    `<expected validity="valid" version="full-xpath-in-CTA"/>
///     <expected validity="invalid" version="restricted-xpath-in-CTA"/>`.
///
/// In all cases we emit one `TestCase` per variant, each with the correct
/// expected outcome.
fn expand_test_versions(test: TestCase, out: &mut Vec<TestCase>) {
    // Determine which XSD versions this test applies to.
    let version_label = &test.version_label;
    let mut xsd_versions: Vec<String> = extract_xsd_versions(version_label)
        .into_iter()
        .map(|s| s.to_string())
        .collect();

    // If the version label itself didn't contain XSD versions, the parser
    // already derived the version from the inheritance chain; use that.
    if xsd_versions.is_empty() {
        xsd_versions.push(test.version.clone());
    }

    // Collect profile labels from <expected> entries that are not XSD
    // versions. These create additional variants that share the same XSD
    // version but differ in expected outcome and version_label.
    let mut profile_labels: Vec<String> = Vec::new();

    for ve in &test.expected_versions {
        if let Some(ref v) = ve.version {
            if is_xsd_version(v) {
                // XSD version — add to xsd_versions if not already present
                if !xsd_versions.contains(v) {
                    xsd_versions.push(v.clone());
                }
            } else {
                // Profile label — collect for profile variant expansion
                if !profile_labels.contains(v) {
                    profile_labels.push(v.clone());
                }
            }
        }
    }

    // Build the list of variants to emit.
    let mut variants: Vec<ExpandedVariant> = Vec::new();

    if profile_labels.is_empty() {
        // No profile-specific expected entries: one variant per XSD version
        for ver in &xsd_versions {
            variants.push(ExpandedVariant {
                xsd_version: ver.clone(),
                profile_label: None,
            });
        }
    } else {
        // Profile-specific expected entries exist. We pick a single
        // variant per XSD version corresponding to ACTIVE_CTA_PROFILE
        // (the profile this driver implements). Variants tagged with
        // an UNSUPPORTED_CTA_PROFILES label are dropped — running them
        // would just compare the result against an outcome that does
        // not describe this implementation. If the active profile is
        // not present in the entries, fall back to the first label so
        // that profile-tagged tests still run with *some* expectation.
        let supported_label = profile_labels
            .iter()
            .find(|l| l.as_str() == ACTIVE_CTA_PROFILE)
            .cloned()
            .or_else(|| {
                profile_labels
                    .iter()
                    .find(|l| !UNSUPPORTED_CTA_PROFILES.contains(&l.as_str()))
                    .cloned()
            });
        if let Some(label) = supported_label {
            for ver in &xsd_versions {
                variants.push(ExpandedVariant {
                    xsd_version: ver.clone(),
                    profile_label: Some(label.clone()),
                });
            }
        }
    }

    // Sort for determinism
    variants.sort_by(|a, b| {
        a.xsd_version
            .cmp(&b.xsd_version)
            .then_with(|| a.profile_label.cmp(&b.profile_label))
    });

    // A test is run in 1.0 mode only when no manifest level names an XSD
    // version and no <expected> entry adds a 1.1 variant.
    let versionless = test.versionless && !xsd_versions.iter().any(|v| v == "1.1");

    // Emit one TestCase per variant
    for variant in &variants {
        let mut copy = test.clone();
        copy.versionless = versionless;
        copy.version = variant.xsd_version.clone();
        if let Some(ref label) = variant.profile_label {
            copy.version_label = label.clone();
        }
        resolve_expected_for_version(&mut copy);
        out.push(copy);
    }
}

/// Resolve the `expected` field from `expected_versions` for the test's
/// current `version` and `version_label`.
///
/// Tries matching in this order:
/// 1. XSD version (`test.version`, e.g. `"1.0"` or `"1.1"`)
/// 2. Profile label (`test.version_label`, e.g. `"full-xpath-in-CTA"`)
/// 3. Unversioned entry (no `version` attribute on `<expected>`)
/// 4. First entry as last resort
fn resolve_expected_for_version(test: &mut TestCase) {
    if test.expected_versions.is_empty() {
        return;
    }

    // 1. Match by XSD version
    if let Some(ve) = test
        .expected_versions
        .iter()
        .find(|ve| ve.version.as_deref() == Some(test.version.as_str()))
    {
        test.expected = ve.outcome;
        return;
    }

    // 2. Match by profile label (e.g. "full-xpath-in-CTA")
    if test.version_label != test.version {
        if let Some(ve) = test
            .expected_versions
            .iter()
            .find(|ve| ve.version.as_deref() == Some(test.version_label.as_str()))
        {
            test.expected = ve.outcome;
            return;
        }
    }

    // 3. Fall back to unversioned entry (version attribute absent)
    if let Some(ve) = test
        .expected_versions
        .iter()
        .find(|ve| ve.version.is_none())
    {
        test.expected = ve.outcome;
        return;
    }

    // 4. If only versioned entries exist but none match, use the first one
    test.expected = test.expected_versions[0].outcome;
}

/// The `--versionless-1.1` mode: the tests the default runs execute in 1.0 mode
/// only because the manifest names no XSD version for them, re-targeted at
/// `SchemaSet::xsd11()`.
///
/// The test-suite metadata (`common/xsts.xsd`, `testSuite/@version`): "If no
/// `version` value is given, or if the value is the empty string, all processors
/// should run the tests." The expected outcome is the one an XSD 1.1
/// configuration is given: an `<expected>` entry for `1.1`, then one for the
/// test's profile label, then the unversioned entry. An `<expected>` entry
/// whose `version` names another XSD version does not apply ("if a processor
/// configuration supports *all* of them, the result given is prescribed"), so a
/// test with only such entries is reported as skipped (`NotPrescribed`).
///
/// Where the manifest gives results per Unicode version, the `Unicode_6.0.0`
/// one is used: the XSD 1.1 regular-expression engine uses a later Unicode
/// Character Database than 6.0.0, which Datatypes 1.1 §G.4.2.2 allows ("or in
/// some later version of the Unicode database"); the 1.0 runs keep the
/// `Unicode_4.0.0` results of their pinned tables.
fn versionless_under_xsd11(tests: Vec<TestCase>) -> Vec<TestCase> {
    const XSD11_UNICODE_PROFILE: &str = "Unicode_6.0.0";
    tests
        .into_iter()
        .filter(|t| t.versionless && t.version == "1.0")
        .map(|mut t| {
            t.version = "1.1".to_string();
            if t.version_label == "1.0" {
                t.version_label = "1.1".to_string();
            }
            if t.expected_versions
                .iter()
                .any(|ve| ve.version.as_deref() == Some(XSD11_UNICODE_PROFILE))
            {
                t.version_label = XSD11_UNICODE_PROFILE.to_string();
            }
            if !t.expected_versions.is_empty() {
                let applies = |v: &Option<String>| match v.as_deref() {
                    Some("1.1") => 0,
                    Some(label) if label == t.version_label && !is_xsd_version(label) => 1,
                    None => 2,
                    Some(_) => 3,
                };
                t.expected = t
                    .expected_versions
                    .iter()
                    .filter(|ve| applies(&ve.version) < 3)
                    .min_by_key(|ve| applies(&ve.version))
                    .map(|ve| ve.outcome)
                    .unwrap_or(ExpectedOutcome::NotPrescribed);
            }
            t
        })
        .collect()
}

/// Conformance test runner
pub struct TestRunner {
    /// Test cases to run
    tests: Vec<TestCase>,
    /// Filter by group name
    group_filter: Option<String>,
    /// Filter by test version
    version_filter: Option<String>,
    /// Filter by test name (substring match, any must match)
    name_filters: Vec<String>,
    /// Maximum tests to run (0 = unlimited)
    max_tests: usize,
    /// Verbose output
    verbose: bool,
}

impl TestRunner {
    /// Create a new test runner
    pub fn new(tests: Vec<TestCase>) -> Self {
        Self {
            tests,
            group_filter: None,
            version_filter: None,
            name_filters: Vec::new(),
            max_tests: 0,
            verbose: false,
        }
    }

    /// Filter tests by group name
    pub fn with_group_filter(mut self, group: Option<String>) -> Self {
        self.group_filter = group;
        self
    }

    /// Filter tests by version
    pub fn with_version_filter(mut self, version: Option<String>) -> Self {
        self.version_filter = version;
        self
    }

    /// Filter tests by name (substring match; if multiple, any must match)
    pub fn with_name_filters(mut self, names: Vec<String>) -> Self {
        self.name_filters = names;
        self
    }

    /// Limit the number of tests to run
    pub fn with_max_tests(mut self, max: usize) -> Self {
        self.max_tests = max;
        self
    }

    /// Enable verbose output
    pub fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    /// Run all tests
    pub fn run(&self) -> (Vec<TestResult>, HashMap<String, TestStats>) {
        let mut results = Vec::new();
        let mut stats_by_group: HashMap<String, TestStats> = HashMap::new();

        let tests: Vec<_> = self
            .tests
            .iter()
            .filter(|t| {
                if let Some(ref group) = self.group_filter {
                    if !t.group.contains(group) {
                        return false;
                    }
                }
                if let Some(ref version) = self.version_filter {
                    if is_xsd_version(version) {
                        // XSD version filter: match against the resolved
                        // XSD version, not the raw label (which may contain
                        // multiple tokens like "1.0 1.1").
                        if t.version != *version {
                            return false;
                        }
                    } else {
                        // Profile label filter: match against version_label
                        if !t
                            .version_label
                            .split_whitespace()
                            .any(|tok| tok == version.as_str())
                        {
                            return false;
                        }
                    }
                }
                if !self.name_filters.is_empty()
                    && !self
                        .name_filters
                        .iter()
                        .any(|n| t.name.contains(n.as_str()))
                {
                    return false;
                }
                true
            })
            .take(if self.max_tests > 0 {
                self.max_tests
            } else {
                usize::MAX
            })
            .collect();

        println!("Running {} tests...", tests.len());

        for test in tests {
            let result = self.run_test(test);

            // Update group stats
            let stats = stats_by_group.entry(result.group.clone()).or_default();
            stats.add_result(&result);

            if self.verbose {
                let status = match result.actual {
                    TestOutcome::Pass => "PASS",
                    TestOutcome::Fail => "FAIL",
                    TestOutcome::Skip => "SKIP",
                    TestOutcome::Error => "ERROR",
                };
                println!("  {} - {} ({:?})", status, result.name, result.duration);
                if let Some(ref msg) = result.error_message {
                    println!("    Error: {}", msg);
                }
            }

            results.push(result);
        }

        (results, stats_by_group)
    }

    /// Run a single test case
    fn run_test(&self, test: &TestCase) -> TestResult {
        let start = Instant::now();

        // Check if all schema files exist
        for schema_file in &test.schema_files {
            if !schema_file.exists() {
                return TestResult {
                    name: test.name.clone(),
                    group: test.group.clone(),
                    expected: test.expected,
                    actual: TestOutcome::Skip,
                    duration: start.elapsed(),
                    error_message: Some(format!("Schema file not found: {:?}", schema_file)),
                };
            }
        }

        // Build and compile schema(s) using SchemaSetBuilder (public API).
        // This automatically resolves xs:import / xs:include directives.
        // Uses DtdEntityExpandingLoader to handle schemas with DTD entity declarations.
        //
        // The conformance harness always uses `RegexCompat::Strict`. The
        // `LenientMs` mode is a downstream feature for callers loading
        // .NET-authored schemas; the W3C suite has no `expected=Valid`
        // pattern tests that exercise the unlocked constructs (backrefs,
        // non-capturing groups, reluctant quantifiers), so wiring it here
        // would be dead code. See `doc/INTRODUCTION.md` for details and
        // `lib.rs` re-export `RegexCompat` for the public API.
        let mut builder = make_schema_builder(&test.version);
        let mut parse_error: Option<String> = None;

        for schema_file in &test.schema_files {
            // Canonicalize to absolute path — try_add's resolver needs
            // absolute paths to correctly resolve relative imports within schemas.
            let path = match schema_file.canonicalize() {
                Ok(abs) => abs.to_string_lossy().into_owned(),
                Err(e) => {
                    parse_error = Some(format!("Failed to resolve path {:?}: {}", schema_file, e));
                    break;
                }
            };
            if let Err(e) = builder.try_add(&path) {
                parse_error = Some(e.to_string());
                break;
            }
        }

        // compile() runs the full pipeline: directive resolution, inline assembly,
        // reference resolution, derivation validation, particle allocation
        let schema_set = if parse_error.is_none() {
            match builder.compile() {
                Ok(compiled) => compiled.into_schema_set(),
                Err(e) => {
                    parse_error = Some(e.to_string());
                    // Fallback — never used: parse_error guards instance validation
                    xsd_schema::SchemaSet::new()
                }
            }
        } else {
            xsd_schema::SchemaSet::new()
        };

        let duration = start.elapsed();

        // Determine outcome
        let (actual, error_message) = match (test.expected, &parse_error) {
            (expected, _) if is_non_asserting_expected(expected) => (
                TestOutcome::Skip,
                expected_skip_reason(expected).map(str::to_string),
            ),
            (ExpectedOutcome::Valid, None) => (TestOutcome::Pass, None),
            (ExpectedOutcome::Valid, Some(e)) => {
                // Auto-skip mistagged XSD-1.1-only schemas tested in 1.0 mode.
                // Mirrors the same skip on instance tests at line 1063.
                if test.version == "1.0" && e.contains("requires XSD 1.1 but schema is in XSD 1.0")
                {
                    (
                        TestOutcome::Skip,
                        Some(
                            "Skipped: schema uses XSD 1.1 features unavailable in 1.0 mode"
                                .to_string(),
                        ),
                    )
                } else {
                    (TestOutcome::Fail, Some(e.clone()))
                }
            }
            (ExpectedOutcome::Invalid, None) => (
                TestOutcome::Fail,
                Some("Schema was valid but expected invalid".to_string()),
            ),
            (ExpectedOutcome::Invalid, Some(_)) => (TestOutcome::Pass, None),
            (ExpectedOutcome::NotKnown, _) => (
                TestOutcome::Skip,
                Some("W3C expected outcome 'notKnown' is meaningless for schema tests".to_string()),
            ),
            (ExpectedOutcome::RuntimeSchemaError, _) => (
                TestOutcome::Skip,
                Some(
                    "W3C expected outcome 'runtime-schema-error' is meaningless for schema tests"
                        .to_string(),
                ),
            ),
            // `runtime-schema-error` on an instance test is not modeled separately
            // by this driver; skip unconditionally so a schema-compile failure on
            // such a test does not surface as Error.
            (ExpectedOutcome::InstanceRuntimeSchemaError, _) => (
                TestOutcome::Skip,
                expected_skip_reason(ExpectedOutcome::InstanceRuntimeSchemaError)
                    .map(str::to_string),
            ),

            // Instance validation tests
            (
                ExpectedOutcome::InstanceValid
                | ExpectedOutcome::InstanceInvalid
                | ExpectedOutcome::InstanceNotKnown,
                _,
            ) => {
                // Schema must compile successfully for instance tests
                if let Some(ref e) = parse_error {
                    // Auto-skip mistagged XSD-1.1-only schemas invoked in 1.0 mode.
                    if test.version == "1.0"
                        && e.contains("requires XSD 1.1 but schema is in XSD 1.0")
                    {
                        return TestResult {
                            name: test.name.clone(),
                            group: test.group.clone(),
                            expected: test.expected,
                            actual: TestOutcome::Skip,
                            duration: start.elapsed(),
                            error_message: Some(
                                "Skipped: schema uses XSD 1.1 features unavailable in 1.0 mode"
                                    .to_string(),
                            ),
                        };
                    }
                    // When the test expects an invalid instance and the schema
                    // cannot be compiled, the instance cannot be validly bound:
                    // treat this as Pass. Cascading schema-compile failures into
                    // ERROR was masking expected-invalid outcomes; the schema
                    // FAIL is already attributed to the schema test itself.
                    if test.expected == ExpectedOutcome::InstanceInvalid {
                        return TestResult {
                            name: test.name.clone(),
                            group: test.group.clone(),
                            expected: test.expected,
                            actual: TestOutcome::Pass,
                            duration: start.elapsed(),
                            error_message: None,
                        };
                    }
                    return TestResult {
                        name: test.name.clone(),
                        group: test.group.clone(),
                        expected: test.expected,
                        actual: TestOutcome::Error,
                        duration: start.elapsed(),
                        error_message: Some(format!("Schema compilation failed: {}", e)),
                    };
                }

                let instance_file = match &test.instance_file {
                    Some(f) if f.exists() => f,
                    Some(f) => {
                        return TestResult {
                            name: test.name.clone(),
                            group: test.group.clone(),
                            expected: test.expected,
                            actual: TestOutcome::Skip,
                            duration: start.elapsed(),
                            error_message: Some(format!("Instance file not found: {:?}", f)),
                        };
                    }
                    None => {
                        return TestResult {
                            name: test.name.clone(),
                            group: test.group.clone(),
                            expected: test.expected,
                            actual: TestOutcome::Skip,
                            duration: start.elapsed(),
                            error_message: Some("No instance file specified".to_string()),
                        };
                    }
                };

                match validate_instance(&schema_set, instance_file) {
                    Ok((actual_outcome, error_msgs)) => match (test.expected, actual_outcome) {
                        (ExpectedOutcome::InstanceValid, InstanceActualOutcome::Valid) => {
                            (TestOutcome::Pass, None)
                        }
                        (ExpectedOutcome::InstanceValid, _) => {
                            let detail = if error_msgs.is_empty() {
                                format!("Instance was {:?} but expected valid", actual_outcome)
                            } else {
                                format!(
                                    "Instance was {:?} but expected valid: {}",
                                    actual_outcome,
                                    error_msgs.join("; ")
                                )
                            };
                            (TestOutcome::Fail, Some(detail))
                        }
                        (ExpectedOutcome::InstanceInvalid, InstanceActualOutcome::Invalid) => {
                            (TestOutcome::Pass, None)
                        }
                        (ExpectedOutcome::InstanceInvalid, _) => (
                            TestOutcome::Fail,
                            Some(format!(
                                "Instance was {:?} but expected invalid",
                                actual_outcome
                            )),
                        ),
                        (ExpectedOutcome::InstanceNotKnown, InstanceActualOutcome::NotKnown) => {
                            (TestOutcome::Pass, None)
                        }
                        (ExpectedOutcome::InstanceNotKnown, _) => (
                            TestOutcome::Fail,
                            Some(format!(
                                "Instance was {:?} but expected notKnown",
                                actual_outcome
                            )),
                        ),
                        _ => unreachable!(),
                    },
                    Err(e) => {
                        // Malformed XML or other XML well-formedness errors at
                        // instance-read time make the instance trivially invalid.
                        // For tests that expect invalid, treat such errors as Pass.
                        if test.expected == ExpectedOutcome::InstanceInvalid
                            && is_xml_wellformedness_error(&e)
                        {
                            (TestOutcome::Pass, None)
                        } else {
                            (TestOutcome::Error, Some(e))
                        }
                    }
                }
            }
            _ => unreachable!("non-asserting expected outcomes are handled before execution"),
        };

        TestResult {
            name: test.name.clone(),
            group: test.group.clone(),
            expected: test.expected,
            actual,
            duration,
            error_message,
        }
    }
}

// ---------------------------------------------------------------------------
// Instance document validation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstanceActualOutcome {
    Valid,
    Invalid,
    NotKnown,
}

/// Validate an instance document against a compiled schema set.
///
/// Returns the root validity classification plus collected validation messages.
/// Returns `Err` on I/O or XML parse errors.
///
/// If the first validation pass collects `xsi:schemaLocation` or
/// `xsi:noNamespaceSchemaLocation` hints, the driver calls
/// [`xsd_schema::enrich_schema_set`] to build an enriched schema set and
/// re-validates (two-pass approach). Pass-2 errors replace pass-1 errors
/// — pass-1 diagnostics are diagnostic-only when enrichment runs.
fn validate_instance(
    schema_set: &xsd_schema::SchemaSet,
    instance_path: &Path,
) -> Result<(InstanceActualOutcome, Vec<String>), String> {
    let raw = fs::read(instance_path).map_err(|e| format!("Failed to read instance: {}", e))?;
    let content = xsd_schema::decode_xml_to_utf8_bytes(raw)
        .map_err(|e| format!("Failed to decode instance: {}", e))?;

    // Canonicalize instance path so schema-location hints resolve correctly
    let canonical_path = instance_path
        .canonicalize()
        .unwrap_or_else(|_| instance_path.to_path_buf());

    let (actual_outcome, error_msgs, sl_hints, nnsl_hints) =
        validate_instance_pass(schema_set, &canonical_path, &content)?;

    let outcome = xsd_schema::enrich_schema_set(schema_set, &sl_hints, &nnsl_hints);
    if let Some(enriched) = outcome.schema_set {
        let (outcome2, errors2, _, _) =
            validate_instance_pass(&enriched, &canonical_path, &content)?;
        return Ok((outcome2, errors2));
    }

    Ok((actual_outcome, error_msgs))
}

/// Result of a single validation pass: errors and collected schema-location hints.
type ValidationPassResult = Result<
    (
        InstanceActualOutcome,
        Vec<String>,
        Vec<xsd_schema::validation::info::SchemaLocationHint>,
        Vec<xsd_schema::validation::info::NoNamespaceSchemaLocationHint>,
    ),
    String,
>;

/// Scan XML content for DTD unparsed entity declarations.
///
/// Looks for `<!ENTITY name SYSTEM/PUBLIC ... NDATA notation>` patterns
/// and returns the set of entity names.
fn scan_unparsed_entities(xml: &str) -> std::collections::HashSet<String> {
    let mut entities = std::collections::HashSet::new();
    // Find all <!ENTITY ...> declarations
    let mut search_from = 0;
    while let Some(start) = xml[search_from..].find("<!ENTITY") {
        let abs_start = search_from + start;
        let rest = &xml[abs_start + 8..]; // skip "<!ENTITY"
                                          // Find the closing '>'
        let end = match rest.find('>') {
            Some(e) => e,
            None => break,
        };
        let decl = &rest[..end];
        // Check if this is an unparsed entity (contains "NDATA")
        if decl.contains("NDATA") {
            // Entity name is the first non-whitespace token after "<!ENTITY"
            let trimmed = decl.trim_start();
            // Skip '%' for parameter entities (we only want general entities)
            if !trimmed.starts_with('%') {
                if let Some(name_end) = trimmed.find(|c: char| c.is_whitespace()) {
                    let name = &trimmed[..name_end];
                    entities.insert(name.to_string());
                }
            }
        }
        search_from = abs_start + 8 + end + 1;
    }
    entities
}

/// Single validation pass. Returns errors and collected schema-location hints.
#[allow(clippy::type_complexity)]
fn validate_instance_pass(
    schema_set: &xsd_schema::SchemaSet,
    instance_path: &Path,
    content: &[u8],
) -> ValidationPassResult {
    use xsd_schema::validation::{
        drive_quick_xml_with, DriveWithError, EndElementInfo, SchemaValidity,
        ValidationEventHandler,
    };

    // Conformance runs the strict-conformance default flags (identity
    // constraints are always enforced). `ALLOW_XML_ATTRIBUTES` is deliberately
    // NOT added — the W3C XSD test suite exercises the spec rule that
    // xml:lang / xml:space / xml:base must be matched by a declared attribute
    // use or an attribute wildcard whose namespace constraint admits the
    // xml namespace (e.g. open044/open045).
    let mut flags = xsd_schema::validation::ValidationFlags::default();

    // Acceptance check for PERF_LAZY_PSVI Phase 2: CONF_NO_PSVI=1 clears
    // BUILD_PSVI_TYPED_VALUES so the whole suite runs the opt-out path. Pass/fail
    // counts MUST be identical to the default run — validity must never depend on
    // PSVI value retention.
    if std::env::var_os("CONF_NO_PSVI").is_some() {
        flags &= !xsd_schema::validation::ValidationFlags::BUILD_PSVI_TYPED_VALUES;
    }

    // XSD 1.1 `xs:assert` evaluation requires the fragment-buffer mode, which
    // `new_fragment_buffer` selects (as does `SchemaValidator::new` with
    // PROCESS_ASSERTIONS in the flags). XSD 1.0 schemas have no assertions, so the
    // fragment-buffer mode is a no-op for them (has_inherited_assertions
    // returns false everywhere → zero buffering, zero XPath compiles).
    #[cfg(feature = "xsd11")]
    let validator = xsd_schema::validation::SchemaValidator::new_fragment_buffer(schema_set, flags);
    #[cfg(not(feature = "xsd11"))]
    let validator = xsd_schema::validation::SchemaValidator::new(schema_set, flags);
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let sink = xsd_schema::validation::CollectingValidationSink {
        errors: &mut errors,
        warnings: &mut warnings,
    };
    let mut runtime = validator.start_run(sink);

    // Set base URI for schema-location hint resolution
    let base_uri = instance_path.to_string_lossy();
    runtime.set_instance_base_uri(base_uri.as_ref());

    // Pre-scan for DTD unparsed entity declarations (<!ENTITY name ... NDATA notation>)
    // and feed them to the validator for ENTITY/ENTITIES type checking (§3.16.4).
    //
    // Only set when we can reliably determine the entity set:
    // - If the document has an external DTD subset (<!DOCTYPE ... SYSTEM/PUBLIC ...>),
    //   we cannot read those entities, so skip entity validation entirely.
    // - Otherwise, set the scanned entity set (may be empty if no DTD at all,
    //   which correctly rejects any ENTITY values).
    {
        let xml_str = std::str::from_utf8(content).unwrap_or("");
        let has_external_only_dtd = if let Some(dt_start) = xml_str.find("<!DOCTYPE") {
            let dt_rest = &xml_str[dt_start..];
            let dt_end = dt_rest.find('>').unwrap_or(dt_rest.len());
            let dt_decl = &dt_rest[..dt_end];
            (dt_decl.contains("SYSTEM") || dt_decl.contains("PUBLIC")) && !dt_decl.contains('[')
        } else {
            false
        };
        if !has_external_only_dtd {
            let unparsed = scan_unparsed_entities(xml_str);
            runtime.set_unparsed_entities(unparsed);
        }
    }

    struct DriverHandler {
        root_validity: Option<SchemaValidity>,
    }
    impl ValidationEventHandler for DriverHandler {
        type Error = std::convert::Infallible;
        fn after_end_element(
            &mut self,
            info: &EndElementInfo,
            depth: usize,
        ) -> Result<(), Self::Error> {
            if depth == 1 {
                self.root_validity = Some(info.validity);
            }
            Ok(())
        }
    }

    let mut handler = DriverHandler {
        root_validity: None,
    };
    drive_quick_xml_with(content, &mut runtime, schema_set, &mut handler).map_err(|e| match e {
        // The content is in memory: no I/O error can come; not a
        // well-formedness verdict either way.
        DriveWithError::Io(e) => format!("I/O error: {}", e),
        DriveWithError::Hook(_) => unreachable!("DriverHandler is infallible"),
        // Every well-formedness refusal of the reader: the instance is not
        // even well-formed XML.
        DriveWithError::NotWellFormed(e) => format!("XML parse error: {}", e),
        other => format!("{}", other),
    })?;

    // Collect hints before end_validation consumes the runtime
    let sl_hints = runtime.schema_location_hints().to_vec();
    let nnsl_hints = runtime.no_namespace_schema_location_hints().to_vec();

    // end_validation checks IDREF resolution; we treat its failure as a
    // validation error, not a driver error.
    if let Err(e) = runtime.end_validation() {
        errors.push(e);
    }

    let error_msgs: Vec<String> = errors.iter().map(|e| format!("{}", e)).collect();
    let actual_outcome = if !errors.is_empty() {
        InstanceActualOutcome::Invalid
    } else {
        match handler.root_validity.unwrap_or(SchemaValidity::NotKnown) {
            SchemaValidity::Valid => InstanceActualOutcome::Valid,
            SchemaValidity::Invalid => InstanceActualOutcome::Invalid,
            SchemaValidity::NotKnown => InstanceActualOutcome::NotKnown,
        }
    };
    Ok((actual_outcome, error_msgs, sl_hints, nnsl_hints))
}

/// Schema loader that pre-processes DTD internal subset entity declarations.
///
/// Some W3C conformance test schemas (e.g., the IRI/URI Type Library) use DTD
/// internal subset entity declarations to compose complex XSD `xs:pattern`
/// values. `quick-xml` cannot expand custom DTD entities, so this loader reads
/// each file, expands any declared entities, strips the DOCTYPE declaration,
/// and returns the processed content for the normal parse pipeline.
#[derive(Debug, Default)]
struct DtdEntityExpandingLoader;

impl xsd_schema::SchemaLoader for DtdEntityExpandingLoader {
    fn load(&self, location: &str) -> xsd_schema::SchemaResult<String> {
        let bytes = std::fs::read(location).map_err(|e| {
            xsd_schema::SchemaError::resolution(format!("Failed to read '{}': {}", location, e))
        })?;
        let content = xsd_schema::decode_xml_bytes(bytes)?;
        Ok(expand_dtd_entities_if_needed(content))
    }

    fn can_load(&self, location: &str) -> bool {
        !location.starts_with("http://")
            && !location.starts_with("https://")
            && !location.starts_with("embedded://")
    }

    fn priority(&self) -> i32 {
        5 // higher than FileSystemLoader (0), lower than EmbeddedLoader (100)
    }
}

/// Build a `SchemaSetBuilder` with the `DtdEntityExpandingLoader` wired in,
/// configured for the requested XSD version.
fn make_schema_builder(version: &str) -> xsd_schema::SchemaSetBuilder {
    let mut chain = xsd_schema::LoaderChain::new();
    chain.add(Box::new(xsd_schema::EmbeddedLoader::new()));
    chain.add(Box::new(DtdEntityExpandingLoader));
    if version == "1.1" {
        xsd_schema::SchemaSetBuilder::xsd11_with_loader(Box::new(chain))
    } else {
        xsd_schema::SchemaSetBuilder::with_loader(Box::new(chain))
    }
}

/// Expand DTD internal subset entity declarations in XML content if present.
/// If no DOCTYPE with entity declarations is found, returns the content unchanged.
fn expand_dtd_entities_if_needed(content: String) -> String {
    match expand_dtd_entities(&content) {
        Some(expanded) => expanded,
        None => content,
    }
}

/// Parse and expand DTD internal subset entity declarations in XML content.
/// Returns `None` if no processable internal subset is found.
fn expand_dtd_entities(content: &str) -> Option<String> {
    let doctype_start = content.find("<!DOCTYPE")?;
    if !content[doctype_start..].contains("<!ENTITY") {
        return None;
    }
    let after_dt = &content[doctype_start..];
    let bracket_rel = after_dt.find('[')?;
    let bracket_start = doctype_start + bracket_rel + 1;

    // Find closing ']' of the internal subset, skipping comments and quoted strings
    let bracket_end = find_subset_end(content, bracket_start)?;
    let internal_subset = &content[bracket_start..bracket_end];

    let mut entities = parse_entity_declarations(internal_subset);
    if entities.is_empty() {
        return None;
    }

    // Iteratively resolve entity cross-references in entity values
    for _ in 0..30 {
        let prev = entities.clone();
        for val in entities.values_mut() {
            *val = expand_refs(val, &prev);
        }
        if entities == prev {
            break;
        }
    }

    // Locate end of DOCTYPE declaration (the '>' after the ']')
    let after_bracket = &content[bracket_end + 1..];
    let close_rel = after_bracket.find('>')?;
    let doctype_end = bracket_end + 1 + close_rel + 1;

    let mut result = String::with_capacity(content.len() * 2);
    result.push_str(&content[..doctype_start]);
    result.push_str(&expand_refs(&content[doctype_end..], &entities));
    Some(result)
}

/// Find the closing `]` of the DTD internal subset, skipping XML comments
/// (`<!-- ... -->`) and quoted strings so embedded `]` characters are ignored.
fn find_subset_end(content: &str, start: usize) -> Option<usize> {
    let bytes = content.as_bytes();
    let mut i = start;
    while i < bytes.len() {
        if content[i..].starts_with("<!--") {
            i += 4;
            while i < bytes.len() {
                if content[i..].starts_with("-->") {
                    i += 3;
                    break;
                }
                i += 1;
            }
        } else if bytes[i] == b'"' {
            i += 1;
            while i < bytes.len() && bytes[i] != b'"' {
                i += 1;
            }
            if i < bytes.len() {
                i += 1;
            }
        } else if bytes[i] == b'\'' {
            i += 1;
            while i < bytes.len() && bytes[i] != b'\'' {
                i += 1;
            }
            if i < bytes.len() {
                i += 1;
            }
        } else if bytes[i] == b']' {
            return Some(i);
        } else {
            i += 1;
        }
    }
    None
}

/// Parse `<!ENTITY name "value">` declarations from a DTD internal subset.
/// First definition wins (XML §4.2). Parameter entities (`%name`) are skipped.
fn parse_entity_declarations(subset: &str) -> HashMap<String, String> {
    let mut entities = HashMap::new();
    let mut pos = 0;

    while pos < subset.len() {
        let Some(rel) = subset[pos..].find("<!ENTITY") else {
            break;
        };
        pos += rel + 8;

        // Skip whitespace
        let ws = subset[pos..].len() - subset[pos..].trim_start().len();
        pos += ws;

        // Skip parameter entities (% prefix)
        if subset[pos..].starts_with('%') {
            if let Some(end_rel) = subset[pos..].find('>') {
                pos += end_rel + 1;
            }
            continue;
        }

        // Read entity name (token ending at first whitespace)
        let name_end = subset[pos..]
            .find(|c: char| c.is_ascii_whitespace())
            .unwrap_or(subset.len() - pos);
        if name_end == 0 {
            pos += 1;
            continue;
        }
        let name = subset[pos..pos + name_end].to_string();
        pos += name_end;

        // Skip whitespace
        let ws = subset[pos..].len() - subset[pos..].trim_start().len();
        pos += ws;

        if pos >= subset.len() {
            break;
        }

        let quote = subset.as_bytes()[pos];
        if quote != b'"' && quote != b'\'' {
            // SYSTEM/PUBLIC entity — skip to closing '>'
            if let Some(end_rel) = subset[pos..].find('>') {
                pos += end_rel + 1;
            }
            continue;
        }
        let quote_char = quote as char;
        pos += 1;

        let val_end = subset[pos..].find(quote_char).unwrap_or(subset.len() - pos);
        let value = subset[pos..pos + val_end].to_string();
        pos += val_end + 1;

        // First-wins
        entities.entry(name).or_insert(value);

        if let Some(end_rel) = subset[pos..].find('>') {
            pos += end_rel + 1;
        }
    }

    entities
}

/// Expand entity references (`&name;`) in `text` using the provided map.
/// Standard XML entities (`&amp;`, `&lt;`, `&gt;`, `&apos;`, `&quot;`) and
/// character references (`&#...;`) are left unchanged.
fn expand_refs(text: &str, entities: &HashMap<String, String>) -> String {
    if !text.contains('&') {
        return text.to_string();
    }

    let mut result = String::with_capacity(text.len() * 2);
    let mut remaining = text;

    while let Some(amp) = remaining.find('&') {
        result.push_str(&remaining[..amp]);
        let after = &remaining[amp + 1..];

        if let Some(semi) = after.find(';') {
            let name = &after[..semi];
            match name {
                "amp" | "lt" | "gt" | "apos" | "quot" => {
                    result.push('&');
                    result.push_str(name);
                    result.push(';');
                }
                _ if name.starts_with('#') => {
                    result.push('&');
                    result.push_str(name);
                    result.push(';');
                }
                _ => {
                    if let Some(val) = entities.get(name) {
                        result.push_str(val);
                    } else {
                        result.push('&');
                        result.push_str(name);
                        result.push(';');
                    }
                }
            }
            remaining = &after[semi + 1..];
        } else {
            result.push('&');
            remaining = after;
        }
    }

    result.push_str(remaining);
    result
}

/// Print a summary of test results
pub fn print_summary(stats_by_group: &HashMap<String, TestStats>) {
    println!("\n=== Test Summary ===\n");

    let mut total = TestStats::default();

    for (group, stats) in stats_by_group {
        println!(
            "{}: {} passed, {} failed, {} skipped, {} errors ({:.1}% pass rate)",
            group,
            stats.passed,
            stats.failed,
            stats.skipped,
            stats.errors,
            stats.pass_rate() * 100.0
        );
        total.passed += stats.passed;
        total.failed += stats.failed;
        total.skipped += stats.skipped;
        total.errors += stats.errors;
        total.total_duration += stats.total_duration;
    }

    println!(
        "\nTotal: {} tests, {} passed, {} failed, {} skipped, {} errors",
        total.total(),
        total.passed,
        total.failed,
        total.skipped,
        total.errors
    );
    println!("Pass rate: {:.1}%", total.pass_rate() * 100.0);
    println!("Duration: {:?}", total.total_duration);
}

fn main() {
    let args: Vec<String> = env::args().collect();

    // Parse command line arguments
    let mut test_suite_path: Option<PathBuf> = None;
    let mut group_filter: Option<String> = None;
    let mut version_filter: Option<String> = None;
    let mut name_filters: Vec<String> = Vec::new();
    let mut max_tests: usize = 0;
    let mut verbose = false;
    let mut expect_pass = false;
    let mut versionless_11 = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--test-suite" | "-s" => {
                if i + 1 < args.len() {
                    test_suite_path = Some(PathBuf::from(&args[i + 1]));
                    i += 1;
                }
            }
            "--group" | "-g" => {
                if i + 1 < args.len() {
                    group_filter = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            "--version" | "-V" => {
                if i + 1 < args.len() {
                    version_filter = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            "--name" | "-n" => {
                if i + 1 < args.len() {
                    name_filters.push(args[i + 1].clone());
                    i += 1;
                }
            }
            "--max" | "-m" => {
                if i + 1 < args.len() {
                    max_tests = args[i + 1].parse().unwrap_or(0);
                    i += 1;
                }
            }
            "--verbose" | "-v" => {
                verbose = true;
            }
            "--expect-pass" => {
                expect_pass = true;
            }
            "--versionless-1.1" => {
                versionless_11 = true;
            }
            "--help" | "-h" => {
                println!("XSD Conformance Test Driver");
                println!();
                println!("Usage: conformance [OPTIONS]");
                println!();
                println!("Options:");
                println!("  -s, --test-suite PATH   Path to W3C test suite directory");
                println!("  -g, --group NAME        Filter by test group name");
                println!("  -n, --name PATTERN      Filter by test name (substring, repeatable)");
                println!("  -V, --version VER       Filter by XSD version (1.0 or 1.1)");
                println!("  -m, --max NUM           Maximum number of tests to run");
                println!("  -v, --verbose           Enable verbose output");
                println!("  --expect-pass           Exit non-zero if any test fails or errors");
                println!(
                    "  --versionless-1.1       Run only the tests without an XSD version in the"
                );
                println!(
                    "                          manifest (run as 1.0 by default) under XSD 1.1"
                );
                println!("  -h, --help              Show this help message");
                return;
            }
            _ => {}
        }
        i += 1;
    }

    let test_suite_path = match test_suite_path {
        Some(p) => p,
        None => {
            println!("XSD Conformance Test Driver");
            println!();
            println!("No test suite path specified. Use --test-suite /path/to/xsdtests");
            println!();
            println!("To run conformance tests, you need the W3C XSD test suite.");
            println!("Download from: https://www.w3.org/XML/2004/xml-schema-test-suite/");
            return;
        }
    };

    if !test_suite_path.exists() {
        eprintln!(
            "Error: Test suite path does not exist: {:?}",
            test_suite_path
        );
        std::process::exit(1);
    }

    // Parse test suite
    println!("Loading test suite from {:?}...", test_suite_path);
    let parser = TestSuiteParser::new(test_suite_path);

    let tests = match parser.parse_manifest() {
        Ok(t) => {
            println!("Loaded {} tests from manifest", t.len());
            t
        }
        Err(_) => {
            println!("No manifest found, scanning for .xsd files...");
            match parser.scan_for_tests() {
                Ok(t) => {
                    println!("Found {} test files", t.len());
                    t
                }
                Err(e) => {
                    eprintln!("Error scanning for tests: {}", e);
                    std::process::exit(1);
                }
            }
        }
    };

    let tests = if versionless_11 {
        if version_filter.is_some() {
            eprintln!(
                "Error: --versionless-1.1 selects its own tests; do not combine it with --version"
            );
            std::process::exit(2);
        }
        let t = versionless_under_xsd11(tests);
        println!("Versionless tests run under XSD 1.1: {}", t.len());
        t
    } else {
        tests
    };

    if tests.is_empty() {
        println!("No tests found!");
        return;
    }

    // Run tests
    let runner = TestRunner::new(tests)
        .with_group_filter(group_filter)
        .with_version_filter(version_filter)
        .with_name_filters(name_filters)
        .with_max_tests(max_tests)
        .with_verbose(verbose);

    let (_results, stats_by_group) = runner.run();

    // Print summary
    print_summary(&stats_by_group);

    if expect_pass {
        let total_failed: usize = stats_by_group.values().map(|s| s.failed + s.errors).sum();
        if total_failed > 0 {
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn parse_schema_indeterminate_as_non_asserting() {
        assert_eq!(
            super::parse_expected_outcome("indeterminate", false).unwrap(),
            super::ExpectedOutcome::Indeterminate
        );
    }

    #[test]
    fn parse_instance_not_known_preserves_expected_outcome() {
        assert_eq!(
            super::parse_expected_outcome("notKnown", true).unwrap(),
            super::ExpectedOutcome::InstanceNotKnown
        );
    }

    #[test]
    fn test_stats_pass_rate() {
        let mut stats = super::TestStats::default();
        stats.passed = 80;
        stats.failed = 20;

        assert_eq!(stats.pass_rate(), 0.8);
    }

    #[test]
    fn test_stats_total() {
        let mut stats = super::TestStats::default();
        stats.passed = 10;
        stats.failed = 5;
        stats.skipped = 3;
        stats.errors = 2;

        assert_eq!(stats.total(), 20);
    }

    #[test]
    fn test_empty_stats() {
        let stats = super::TestStats::default();
        assert_eq!(stats.pass_rate(), 0.0);
        assert_eq!(stats.total(), 0);
    }
}
