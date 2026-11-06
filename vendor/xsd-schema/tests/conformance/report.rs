//! Conformance test report generation
//!
//! This module generates various report formats from conformance test results.
//! Supports:
//! - Plain text summary
//! - JSON report
//! - CSV export
//! - HTML report

#![allow(dead_code)] // Report infrastructure for future use

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use std::time::Duration;

/// Test result outcome (from driver)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestOutcome {
    Pass,
    Fail,
    Skip,
    Error,
}

/// Expected outcome from test definition (from driver)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedOutcome {
    Valid,
    Invalid,
    NotKnown,
    RuntimeSchemaError,
    ImplementationDefined,
    ImplementationDependent,
    Indeterminate,
    InstanceValid,
    InstanceInvalid,
    InstanceIndeterminate,
    InstanceImplementationDefined,
    InstanceImplementationDependent,
    InstanceRuntimeSchemaError,
    InstanceNotKnown,
}

/// A single test case result (from driver)
#[derive(Debug, Clone)]
pub struct TestResult {
    pub name: String,
    pub group: String,
    pub expected: ExpectedOutcome,
    pub actual: TestOutcome,
    pub duration: Duration,
    pub error_message: Option<String>,
}

/// Test suite statistics (from driver)
#[derive(Debug, Clone, Default)]
pub struct TestStats {
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub errors: usize,
    pub total_duration: Duration,
}

impl TestStats {
    pub fn pass_rate(&self) -> f64 {
        let total = self.passed + self.failed;
        if total == 0 {
            0.0
        } else {
            self.passed as f64 / total as f64
        }
    }

    pub fn total(&self) -> usize {
        self.passed + self.failed + self.skipped + self.errors
    }
}

/// Report format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportFormat {
    /// Plain text summary
    Text,
    /// JSON report
    Json,
    /// CSV export
    Csv,
    /// HTML report
    Html,
}

/// Report generator
pub struct ReportGenerator {
    /// Test results
    results: Vec<TestResult>,
    /// Statistics by group
    stats_by_group: HashMap<String, TestStats>,
}

impl ReportGenerator {
    /// Create a new report generator
    pub fn new(results: Vec<TestResult>, stats_by_group: HashMap<String, TestStats>) -> Self {
        Self {
            results,
            stats_by_group,
        }
    }

    /// Generate a report to stdout
    pub fn print(&self, format: ReportFormat) -> io::Result<()> {
        let mut stdout = io::stdout();
        self.write(&mut stdout, format)
    }

    /// Generate a report to a file
    pub fn write_to_file(&self, path: &Path, format: ReportFormat) -> io::Result<()> {
        let mut file = File::create(path)?;
        self.write(&mut file, format)
    }

    /// Generate a report to any writer
    pub fn write<W: Write>(&self, writer: &mut W, format: ReportFormat) -> io::Result<()> {
        match format {
            ReportFormat::Text => self.write_text(writer),
            ReportFormat::Json => self.write_json(writer),
            ReportFormat::Csv => self.write_csv(writer),
            ReportFormat::Html => self.write_html(writer),
        }
    }

    /// Write a plain text report
    fn write_text<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writeln!(writer, "XSD Conformance Test Report")?;
        writeln!(writer, "==========================")?;
        writeln!(writer)?;

        // Summary by group
        writeln!(writer, "Results by Group:")?;
        writeln!(writer, "-----------------")?;

        let mut total = TestStats::default();
        for (group, stats) in &self.stats_by_group {
            writeln!(
                writer,
                "{}: {} passed, {} failed, {} skipped, {} errors ({:.1}% pass rate)",
                group,
                stats.passed,
                stats.failed,
                stats.skipped,
                stats.errors,
                stats.pass_rate() * 100.0
            )?;
            total.passed += stats.passed;
            total.failed += stats.failed;
            total.skipped += stats.skipped;
            total.errors += stats.errors;
            total.total_duration += stats.total_duration;
        }

        writeln!(writer)?;
        writeln!(writer, "Total Summary:")?;
        writeln!(writer, "--------------")?;
        writeln!(writer, "Total tests:  {}", total.total())?;
        writeln!(writer, "Passed:       {}", total.passed)?;
        writeln!(writer, "Failed:       {}", total.failed)?;
        writeln!(writer, "Skipped:      {}", total.skipped)?;
        writeln!(writer, "Errors:       {}", total.errors)?;
        writeln!(writer, "Pass rate:    {:.1}%", total.pass_rate() * 100.0)?;
        writeln!(writer, "Duration:     {:?}", total.total_duration)?;

        // Failed tests
        let failed: Vec<_> = self
            .results
            .iter()
            .filter(|r| r.actual == TestOutcome::Fail)
            .collect();

        if !failed.is_empty() {
            writeln!(writer)?;
            writeln!(writer, "Failed Tests ({}):", failed.len())?;
            writeln!(writer, "-------------------")?;
            for result in failed {
                writeln!(writer, "- {} ({})", result.name, result.group)?;
                if let Some(ref msg) = result.error_message {
                    writeln!(writer, "  Error: {}", msg)?;
                }
            }
        }

        Ok(())
    }

    /// Write a JSON report
    fn write_json<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writeln!(writer, "{{")?;
        writeln!(writer, "  \"summary\": {{")?;

        let mut total = TestStats::default();
        for stats in self.stats_by_group.values() {
            total.passed += stats.passed;
            total.failed += stats.failed;
            total.skipped += stats.skipped;
            total.errors += stats.errors;
            total.total_duration += stats.total_duration;
        }

        writeln!(writer, "    \"total\": {},", total.total())?;
        writeln!(writer, "    \"passed\": {},", total.passed)?;
        writeln!(writer, "    \"failed\": {},", total.failed)?;
        writeln!(writer, "    \"skipped\": {},", total.skipped)?;
        writeln!(writer, "    \"errors\": {},", total.errors)?;
        writeln!(writer, "    \"passRate\": {:.4},", total.pass_rate())?;
        writeln!(
            writer,
            "    \"durationMs\": {}",
            total.total_duration.as_millis()
        )?;
        writeln!(writer, "  }},")?;

        // Groups
        writeln!(writer, "  \"groups\": {{")?;
        let groups: Vec<_> = self.stats_by_group.iter().collect();
        for (i, (group, stats)) in groups.iter().enumerate() {
            let comma = if i < groups.len() - 1 { "," } else { "" };
            writeln!(writer, "    \"{}\": {{", group)?;
            writeln!(writer, "      \"passed\": {},", stats.passed)?;
            writeln!(writer, "      \"failed\": {},", stats.failed)?;
            writeln!(writer, "      \"skipped\": {},", stats.skipped)?;
            writeln!(writer, "      \"errors\": {},", stats.errors)?;
            writeln!(writer, "      \"passRate\": {:.4}", stats.pass_rate())?;
            writeln!(writer, "    }}{}", comma)?;
        }
        writeln!(writer, "  }},")?;

        // Results
        writeln!(writer, "  \"results\": [")?;
        for (i, result) in self.results.iter().enumerate() {
            let comma = if i < self.results.len() - 1 { "," } else { "" };
            writeln!(writer, "    {{")?;
            writeln!(writer, "      \"name\": \"{}\",", escape_json(&result.name))?;
            writeln!(
                writer,
                "      \"group\": \"{}\",",
                escape_json(&result.group)
            )?;
            writeln!(
                writer,
                "      \"expected\": \"{}\",",
                expected_to_str(result.expected)
            )?;
            writeln!(
                writer,
                "      \"actual\": \"{}\",",
                outcome_to_str(result.actual)
            )?;
            writeln!(
                writer,
                "      \"durationMs\": {}",
                result.duration.as_micros() as f64 / 1000.0
            )?;
            if let Some(ref msg) = result.error_message {
                writeln!(writer, "      ,\"error\": \"{}\"", escape_json(msg))?;
            }
            writeln!(writer, "    }}{}", comma)?;
        }
        writeln!(writer, "  ]")?;
        writeln!(writer, "}}")?;

        Ok(())
    }

    /// Write a CSV report
    fn write_csv<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        // Header
        writeln!(writer, "name,group,expected,actual,duration_ms,error")?;

        // Results
        for result in &self.results {
            writeln!(
                writer,
                "{},{},{},{},{:.3},{}",
                escape_csv(&result.name),
                escape_csv(&result.group),
                expected_to_str(result.expected),
                outcome_to_str(result.actual),
                result.duration.as_micros() as f64 / 1000.0,
                escape_csv(result.error_message.as_deref().unwrap_or(""))
            )?;
        }

        Ok(())
    }

    /// Write an HTML report
    fn write_html<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writeln!(
            writer,
            r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>XSD Conformance Test Report</title>
    <style>
        body {{ font-family: Arial, sans-serif; margin: 20px; }}
        h1 {{ color: #333; }}
        .summary {{ background: #f5f5f5; padding: 15px; border-radius: 5px; margin-bottom: 20px; }}
        .summary table {{ width: auto; }}
        .summary td {{ padding: 5px 15px; }}
        .pass {{ color: #28a745; }}
        .fail {{ color: #dc3545; }}
        .skip {{ color: #6c757d; }}
        .error {{ color: #fd7e14; }}
        table {{ border-collapse: collapse; width: 100%; }}
        th, td {{ border: 1px solid #ddd; padding: 8px; text-align: left; }}
        th {{ background: #007bff; color: white; }}
        tr:nth-child(even) {{ background: #f2f2f2; }}
        tr:hover {{ background: #ddd; }}
        .filter {{ margin-bottom: 15px; }}
        .filter input {{ padding: 8px; width: 300px; }}
    </style>
    <script>
        function filterResults() {{
            var input = document.getElementById('filter');
            var filter = input.value.toLowerCase();
            var table = document.getElementById('results');
            var tr = table.getElementsByTagName('tr');

            for (var i = 1; i < tr.length; i++) {{
                var td = tr[i].getElementsByTagName('td');
                var found = false;
                for (var j = 0; j < td.length; j++) {{
                    if (td[j].textContent.toLowerCase().indexOf(filter) > -1) {{
                        found = true;
                        break;
                    }}
                }}
                tr[i].style.display = found ? '' : 'none';
            }}
        }}
    </script>
</head>
<body>
    <h1>XSD Conformance Test Report</h1>"#
        )?;

        // Summary
        let mut total = TestStats::default();
        for stats in self.stats_by_group.values() {
            total.passed += stats.passed;
            total.failed += stats.failed;
            total.skipped += stats.skipped;
            total.errors += stats.errors;
            total.total_duration += stats.total_duration;
        }

        writeln!(
            writer,
            r#"
    <div class="summary">
        <h2>Summary</h2>
        <table>
            <tr><td>Total Tests:</td><td><strong>{}</strong></td></tr>
            <tr><td>Passed:</td><td class="pass"><strong>{}</strong></td></tr>
            <tr><td>Failed:</td><td class="fail"><strong>{}</strong></td></tr>
            <tr><td>Skipped:</td><td class="skip"><strong>{}</strong></td></tr>
            <tr><td>Errors:</td><td class="error"><strong>{}</strong></td></tr>
            <tr><td>Pass Rate:</td><td><strong>{:.1}%</strong></td></tr>
            <tr><td>Duration:</td><td><strong>{:?}</strong></td></tr>
        </table>
    </div>"#,
            total.total(),
            total.passed,
            total.failed,
            total.skipped,
            total.errors,
            total.pass_rate() * 100.0,
            total.total_duration
        )?;

        // Group summary
        writeln!(
            writer,
            r#"
    <h2>Results by Group</h2>
    <table>
        <tr>
            <th>Group</th>
            <th>Passed</th>
            <th>Failed</th>
            <th>Skipped</th>
            <th>Errors</th>
            <th>Pass Rate</th>
        </tr>"#
        )?;

        for (group, stats) in &self.stats_by_group {
            writeln!(
                writer,
                r#"        <tr>
            <td>{}</td>
            <td class="pass">{}</td>
            <td class="fail">{}</td>
            <td class="skip">{}</td>
            <td class="error">{}</td>
            <td>{:.1}%</td>
        </tr>"#,
                escape_html(group),
                stats.passed,
                stats.failed,
                stats.skipped,
                stats.errors,
                stats.pass_rate() * 100.0
            )?;
        }

        writeln!(writer, "    </table>")?;

        // Results table
        writeln!(
            writer,
            r#"
    <h2>Test Results</h2>
    <div class="filter">
        <input type="text" id="filter" onkeyup="filterResults()" placeholder="Filter results...">
    </div>
    <table id="results">
        <tr>
            <th>Name</th>
            <th>Group</th>
            <th>Expected</th>
            <th>Actual</th>
            <th>Duration</th>
            <th>Error</th>
        </tr>"#
        )?;

        for result in &self.results {
            let class = match result.actual {
                TestOutcome::Pass => "pass",
                TestOutcome::Fail => "fail",
                TestOutcome::Skip => "skip",
                TestOutcome::Error => "error",
            };

            writeln!(
                writer,
                r#"        <tr>
            <td>{}</td>
            <td>{}</td>
            <td>{}</td>
            <td class="{}">{}</td>
            <td>{:.3}ms</td>
            <td>{}</td>
        </tr>"#,
                escape_html(&result.name),
                escape_html(&result.group),
                expected_to_str(result.expected),
                class,
                outcome_to_str(result.actual),
                result.duration.as_micros() as f64 / 1000.0,
                escape_html(result.error_message.as_deref().unwrap_or(""))
            )?;
        }

        writeln!(
            writer,
            r#"    </table>
</body>
</html>"#
        )?;

        Ok(())
    }
}

/// Convert expected outcome to string
fn expected_to_str(outcome: ExpectedOutcome) -> &'static str {
    match outcome {
        ExpectedOutcome::Valid => "valid",
        ExpectedOutcome::Invalid => "invalid",
        ExpectedOutcome::NotKnown => "notKnown",
        ExpectedOutcome::RuntimeSchemaError => "runtime-schema-error",
        ExpectedOutcome::ImplementationDefined => "implementation-defined",
        ExpectedOutcome::ImplementationDependent => "implementation-dependent",
        ExpectedOutcome::Indeterminate => "indeterminate",
        ExpectedOutcome::InstanceValid => "instanceValid",
        ExpectedOutcome::InstanceInvalid => "instanceInvalid",
        ExpectedOutcome::InstanceIndeterminate => "instanceIndeterminate",
        ExpectedOutcome::InstanceImplementationDefined => "instanceImplementationDefined",
        ExpectedOutcome::InstanceImplementationDependent => "instanceImplementationDependent",
        ExpectedOutcome::InstanceRuntimeSchemaError => "instanceRuntimeSchemaError",
        ExpectedOutcome::InstanceNotKnown => "instanceNotKnown",
    }
}

/// Convert test outcome to string
fn outcome_to_str(outcome: TestOutcome) -> &'static str {
    match outcome {
        TestOutcome::Pass => "pass",
        TestOutcome::Fail => "fail",
        TestOutcome::Skip => "skip",
        TestOutcome::Error => "error",
    }
}

/// Escape a string for JSON
fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Escape a string for CSV
fn escape_csv(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Escape a string for HTML
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn make_test_result(name: &str, outcome: TestOutcome) -> TestResult {
        TestResult {
            name: name.to_string(),
            group: "test-group".to_string(),
            expected: ExpectedOutcome::Valid,
            actual: outcome,
            duration: Duration::from_millis(10),
            error_message: None,
        }
    }

    #[test]
    fn test_escape_json() {
        assert_eq!(escape_json("hello"), "hello");
        assert_eq!(escape_json("hello\"world"), "hello\\\"world");
        assert_eq!(escape_json("line1\nline2"), "line1\\nline2");
    }

    #[test]
    fn test_escape_csv() {
        assert_eq!(escape_csv("hello"), "hello");
        assert_eq!(escape_csv("hello,world"), "\"hello,world\"");
        assert_eq!(escape_csv("hello\"world"), "\"hello\"\"world\"");
    }

    #[test]
    fn test_escape_html() {
        assert_eq!(escape_html("<script>"), "&lt;script&gt;");
        assert_eq!(escape_html("a & b"), "a &amp; b");
    }

    #[test]
    fn test_text_report() {
        let results = vec![
            make_test_result("test1", TestOutcome::Pass),
            make_test_result("test2", TestOutcome::Fail),
        ];

        let mut stats = HashMap::new();
        stats.insert(
            "test-group".to_string(),
            TestStats {
                passed: 1,
                failed: 1,
                skipped: 0,
                errors: 0,
                total_duration: Duration::from_millis(20),
            },
        );

        let generator = ReportGenerator::new(results, stats);
        let mut output = Vec::new();
        generator.write_text(&mut output).unwrap();

        let report = String::from_utf8(output).unwrap();
        assert!(report.contains("XSD Conformance Test Report"));
        assert!(report.contains("test-group"));
    }

    #[test]
    fn test_json_report() {
        let results = vec![make_test_result("test1", TestOutcome::Pass)];

        let mut stats = HashMap::new();
        stats.insert(
            "test-group".to_string(),
            TestStats {
                passed: 1,
                failed: 0,
                skipped: 0,
                errors: 0,
                total_duration: Duration::from_millis(10),
            },
        );

        let generator = ReportGenerator::new(results, stats);
        let mut output = Vec::new();
        generator.write_json(&mut output).unwrap();

        let report = String::from_utf8(output).unwrap();
        assert!(report.contains("\"summary\""));
        assert!(report.contains("\"passed\": 1"));
    }

    #[test]
    fn test_csv_report() {
        let results = vec![make_test_result("test1", TestOutcome::Pass)];

        let generator = ReportGenerator::new(results, HashMap::new());
        let mut output = Vec::new();
        generator.write_csv(&mut output).unwrap();

        let report = String::from_utf8(output).unwrap();
        assert!(report.contains("name,group,expected,actual"));
        assert!(report.contains("test1,test-group,valid,pass"));
    }

    #[test]
    fn test_html_report() {
        let results = vec![make_test_result("test1", TestOutcome::Pass)];

        let mut stats = HashMap::new();
        stats.insert(
            "test-group".to_string(),
            TestStats {
                passed: 1,
                failed: 0,
                skipped: 0,
                errors: 0,
                total_duration: Duration::from_millis(10),
            },
        );

        let generator = ReportGenerator::new(results, stats);
        let mut output = Vec::new();
        generator.write_html(&mut output).unwrap();

        let report = String::from_utf8(output).unwrap();
        assert!(report.contains("<!DOCTYPE html>"));
        assert!(report.contains("XSD Conformance Test Report"));
    }
}
