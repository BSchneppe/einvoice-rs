//! XQTS XPath 2.0 Conformance Test Driver
//!
//! Runs W3C XQTS 1.0.2 test suite against the Rust XPath 2.0 implementation.
//!
//! Usage:
//!   cargo test --test xqts_xpath -- -s /path/to/XQTS_1_0_2 --all -v

#[path = "catalog.rs"]
mod catalog;
#[path = "compare.rs"]
mod compare;
#[path = "filter.rs"]
mod filter;
#[path = "prepare.rs"]
mod prepare;

use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use catalog::{CatalogConfig, Scenario, XqtsTestCase};
use xsd_schema::namespace::context::NamespaceContextSnapshot;
use xsd_schema::namespace::table::NameTable;
use xsd_schema::xpath::api::{XPathEvaluator, XPathExpr};
use xsd_schema::xpath::context::XPathContext;
use xsd_schema::xpath::functions::XPathValue;
use xsd_schema::RoXmlNavigator;

#[derive(Debug, Clone, PartialEq)]
enum TestOutcome {
    Pass,
    Fail,
    Skip,
    Error,
}

struct TestResult {
    name: String,
    outcome: TestOutcome,
    duration: Duration,
    message: Option<String>,
}

fn print_usage() {
    println!("XQTS XPath 2.0 Conformance Test Driver");
    println!();
    println!("Usage: xqts_xpath [OPTIONS]");
    println!();
    println!("Options:");
    println!("  -s, --suite PATH    Path to XQTS_1_0_2 directory");
    println!("  -g, --group NAME    Run tests from a specific group");
    println!("  -t, --test NAME     Run a specific test");
    println!("  -a, --all           Run all XPath 2.0 applicable tests");
    println!("  -v, --verbose       Print each test result");
    println!("  -f, --failed        Output only failed tests");
    println!("  -l, --list          List test groups and exit");
    println!("  --trace             Enable fn:trace() output to stderr");
    println!("  -h, --help          Show this help message");
}

/// Build XPath namespace context with standard XQTS bindings.
///
/// The C# XPath2Context automatically registers xs, xsi, fn, and local prefixes
/// (see XPath2Context.cs:35-44). We replicate that here plus the XQTS-specific
/// foo, FOO, and atomic prefixes from PrepareXPath (Form1.cs:674-677).
fn build_xpath_context(names: &NameTable) -> XPathContext<'_> {
    // XQTS-specific prefixes
    let foo_prefix = names.add("foo");
    let foo_upper_prefix = names.add("FOO");
    let atomic_prefix = names.add("atomic");
    let example_ns = names.add("http://example.org");
    let xquery_test_ns = names.add("http://www.w3.org/XQueryTest");

    // Standard XPath 2.0 prefixes (auto-registered by C# XPath2Context)
    let xs_prefix = names.add("xs");
    let xs_ns = names.add("http://www.w3.org/2001/XMLSchema");
    let xsi_prefix = names.add("xsi");
    let xsi_ns = names.add("http://www.w3.org/2001/XMLSchema-instance");
    let fn_prefix = names.add("fn");
    let fn_ns = names.add("http://www.w3.org/2005/xpath-functions");
    let local_prefix = names.add("local");
    let local_ns = names.add("http://www.w3.org/2005/xquery-local-functions");

    let namespaces = NamespaceContextSnapshot {
        default_ns: None,
        bindings: vec![
            (xs_prefix, xs_ns),
            (xsi_prefix, xsi_ns),
            (fn_prefix, fn_ns),
            (local_prefix, local_ns),
            (foo_prefix, example_ns),
            (foo_upper_prefix, example_ns),
            (atomic_prefix, xquery_test_ns),
        ],
    };

    XPathContext::new(names).with_namespaces(namespaces)
}

/// Execute a single XQTS test case.
fn execute_test(
    tc: &XqtsTestCase,
    config: &CatalogConfig,
    sources: &HashMap<String, PathBuf>,
    names: &NameTable,
    verbose: bool,
    trace_enabled: bool,
) -> TestResult {
    let start = Instant::now();

    // Build query file path
    let query_path = config.base_path.join(
        format!(
            "{}{}{}{}",
            config.query_offset_path, tc.file_path, tc.query_name, config.query_file_extension
        )
        .replace('\\', "/"),
    );

    // Read and prepare query
    let query_text = match std::fs::read_to_string(&query_path) {
        Ok(text) => text,
        Err(e) => {
            return TestResult {
                name: tc.name.clone(),
                outcome: TestOutcome::Skip,
                duration: start.elapsed(),
                message: Some(format!("Cannot read query file: {}", e)),
            };
        }
    };

    let xpath_text = prepare::prepare_query_text(&query_text);
    if xpath_text.is_empty() {
        return TestResult {
            name: tc.name.clone(),
            outcome: TestOutcome::Skip,
            duration: start.elapsed(),
            message: Some("Empty query after preprocessing".to_string()),
        };
    }

    let ctx = build_xpath_context(names).with_trace_enabled(trace_enabled);

    // Collect variable names
    let mut var_names: Vec<String> = Vec::new();
    for input in &tc.input_files {
        if !input.variable.is_empty() {
            var_names.push(input.variable.clone());
        }
    }
    for uri in &tc.input_uris {
        if !uri.variable.is_empty() {
            var_names.push(uri.variable.clone());
        }
    }
    let var_refs: Vec<&str> = var_names.iter().map(|s| s.as_str()).collect();

    // Compile
    let compiled = match XPathExpr::compile_with_vars(&xpath_text, &ctx, &var_refs) {
        Ok(expr) => expr,
        Err(e) => {
            if tc.scenario == Scenario::ParseError
                || tc.scenario == Scenario::RuntimeError
                || !tc.expected_errors.is_empty()
            {
                return TestResult {
                    name: tc.name.clone(),
                    outcome: TestOutcome::Pass,
                    duration: start.elapsed(),
                    message: Some(format!("Expected compile error: {}", e)),
                };
            }
            return TestResult {
                name: tc.name.clone(),
                outcome: TestOutcome::Fail,
                duration: start.elapsed(),
                message: Some(format!("Compile error: {}", e)),
            };
        }
    };

    // Load source documents. We need to keep the String contents and
    // parsed Documents alive for the RoXmlNavigators to reference.
    let mut doc_contents: Vec<String> = Vec::new();
    let mut doc_source_ids: Vec<String> = Vec::new();
    let mut doc_paths: Vec<String> = Vec::new();

    // Collect all source IDs we need
    let mut needed_sources: Vec<String> = Vec::new();
    if let Some(ref ctx_item) = tc.context_item {
        needed_sources.push(ctx_item.clone());
    }
    for input in &tc.input_files {
        needed_sources.push(input.source_id.clone());
    }

    for source_id in &needed_sources {
        let source_path = match sources.get(source_id.as_str()) {
            Some(p) => p,
            None => {
                return TestResult {
                    name: tc.name.clone(),
                    outcome: TestOutcome::Skip,
                    duration: start.elapsed(),
                    message: Some(format!("Source ID '{}' not found in catalog", source_id)),
                };
            }
        };
        match std::fs::read_to_string(source_path) {
            Ok(content) => {
                doc_contents.push(content);
                doc_source_ids.push(source_id.clone());
                doc_paths.push(source_path.to_string_lossy().to_string());
            }
            Err(e) => {
                return TestResult {
                    name: tc.name.clone(),
                    outcome: TestOutcome::Skip,
                    duration: start.elapsed(),
                    message: Some(format!("Cannot read source {}: {}", source_id, e)),
                };
            }
        }
    }

    // Parse all documents
    let mut parsed_docs: Vec<roxmltree::Document> = Vec::new();
    for (i, content) in doc_contents.iter().enumerate() {
        match roxmltree::Document::parse(content) {
            Ok(doc) => parsed_docs.push(doc),
            Err(e) => {
                return TestResult {
                    name: tc.name.clone(),
                    outcome: TestOutcome::Skip,
                    duration: start.elapsed(),
                    message: Some(format!("Cannot parse source {}: {}", doc_source_ids[i], e)),
                };
            }
        }
    }

    // Build a map from source_id -> index into parsed_docs
    let mut source_doc_index: HashMap<&str, usize> = HashMap::new();
    for (i, id) in doc_source_ids.iter().enumerate() {
        source_doc_index.insert(id.as_str(), i);
    }

    // Set up context node
    let context_nav: Option<RoXmlNavigator> = tc.context_item.as_ref().and_then(|ctx_id| {
        source_doc_index
            .get(ctx_id.as_str())
            .map(|&idx| RoXmlNavigator::with_base_uri(&parsed_docs[idx], &doc_paths[idx]))
    });

    // Evaluate
    let evaluator: XPathEvaluator = compiled.evaluator(&ctx);

    let eval_result = evaluator.run_with_node_and_setup(context_nav, |typed_eval| {
        // Bind input-file variables as node values
        for input in &tc.input_files {
            if input.variable.is_empty() {
                continue;
            }
            if let Some(&idx) = source_doc_index.get(input.source_id.as_str()) {
                let nav = RoXmlNavigator::with_base_uri(&parsed_docs[idx], &doc_paths[idx]);
                let val = XPathValue::from_node(nav);
                if let Err(e) = typed_eval.set_variable_by_name(&input.variable, val) {
                    if verbose {
                        eprintln!("  Warning: failed to bind var '{}': {}", input.variable, e);
                    }
                }
            }
        }

        // Bind input-URI variables as string values
        for uri_input in &tc.input_uris {
            if uri_input.variable.is_empty() {
                continue;
            }
            // Try to expand source reference to file path
            let value = if let Some(path) = sources.get(uri_input.value.as_str()) {
                path.to_string_lossy().to_string()
            } else {
                uri_input.value.clone()
            };
            let val = XPathValue::string(value);
            if let Err(e) = typed_eval.set_variable_by_name(&uri_input.variable, val) {
                if verbose {
                    eprintln!(
                        "  Warning: failed to bind URI var '{}': {}",
                        uri_input.variable, e
                    );
                }
            }
        }
    });

    let result = match eval_result {
        Ok(val) => val,
        Err(e) => {
            if tc.scenario == Scenario::ParseError
                || tc.scenario == Scenario::RuntimeError
                || !tc.expected_errors.is_empty()
            {
                return TestResult {
                    name: tc.name.clone(),
                    outcome: TestOutcome::Pass,
                    duration: start.elapsed(),
                    message: Some(format!("Expected eval error: {}", e)),
                };
            }
            return TestResult {
                name: tc.name.clone(),
                outcome: TestOutcome::Fail,
                duration: start.elapsed(),
                message: Some(format!("Eval error: {}", e)),
            };
        }
    };

    // Handle scenarios
    match tc.scenario {
        Scenario::Standard => {
            // Compare against each output file; pass if any match.
            // First pass: try all variants without debug output, so we don't
            // print "not accepted" noise for variants when the test passes overall.
            let mut last_error: Option<String> = None;
            let mut any_passed = false;
            for output in &tc.output_files {
                let result_path = config.base_path.join(
                    format!(
                        "{}{}{}",
                        config.result_offset_path, tc.file_path, output.file_name
                    )
                    .replace('\\', "/"),
                );

                let xml_compare = output.compare == "XML";
                let text_or_fragment = output.compare == "Text" || output.compare == "Fragment";

                if output.compare == "Inspect" {
                    return TestResult {
                        name: tc.name.clone(),
                        outcome: TestOutcome::Pass,
                        duration: start.elapsed(),
                        message: Some("Inspection needed".to_string()),
                    };
                }
                if output.compare == "Ignore" {
                    continue;
                }

                if !xml_compare && !text_or_fragment {
                    return TestResult {
                        name: tc.name.clone(),
                        outcome: TestOutcome::Error,
                        duration: start.elapsed(),
                        message: Some(format!("Unknown compare mode: {}", output.compare)),
                    };
                }

                let cmp_result = compare::compare_result(
                    &tc.name,
                    result.clone(),
                    &result_path.to_string_lossy(),
                    xml_compare,
                );
                match cmp_result {
                    Ok(true) => {
                        any_passed = true;
                        break;
                    }
                    Ok(false) => {
                        last_error = Some("Result did not match expected output".to_string());
                    }
                    Err(e) => {
                        last_error = Some(format!("Compare error: {}", e));
                    }
                }
            }

            if any_passed {
                return TestResult {
                    name: tc.name.clone(),
                    outcome: TestOutcome::Pass,
                    duration: start.elapsed(),
                    message: None,
                };
            }

            // If no output files were specified, pass
            if tc.output_files.is_empty() {
                return TestResult {
                    name: tc.name.clone(),
                    outcome: TestOutcome::Pass,
                    duration: start.elapsed(),
                    message: None,
                };
            }

            // Test failed — re-run with debug output if verbose, to show
            // ACTUAL vs EXPECTED for the first non-Inspect/Ignore variant.
            if verbose {
                for output in &tc.output_files {
                    if output.compare == "Inspect" || output.compare == "Ignore" {
                        continue;
                    }
                    let result_path = config.base_path.join(
                        format!(
                            "{}{}{}",
                            config.result_offset_path, tc.file_path, output.file_name
                        )
                        .replace('\\', "/"),
                    );
                    let xml_compare = output.compare == "XML";
                    let _ = compare::compare_result_debug(
                        &tc.name,
                        result.clone(),
                        &result_path.to_string_lossy(),
                        xml_compare,
                    );
                    break;
                }
            }

            TestResult {
                name: tc.name.clone(),
                outcome: TestOutcome::Fail,
                duration: start.elapsed(),
                message: last_error.or(Some("No output file matched".to_string())),
            }
        }
        Scenario::RuntimeError => {
            // For runtime-error scenario: force materialization, expecting error
            // (the C# code iterates through the result to trigger lazy eval errors)
            let _items = result.clone().into_vec();

            // Some runtime-error tests also have output files as acceptable alternatives.
            // Try comparing against them before declaring failure.
            if !tc.output_files.is_empty() {
                for output in &tc.output_files {
                    let result_path = config.base_path.join(
                        format!(
                            "{}{}{}",
                            config.result_offset_path, tc.file_path, output.file_name
                        )
                        .replace('\\', "/"),
                    );
                    let xml_compare = output.compare == "XML";
                    if output.compare == "Inspect" || output.compare == "Ignore" {
                        continue;
                    }
                    let cmp_result = compare::compare_result(
                        &tc.name,
                        result.clone(),
                        &result_path.to_string_lossy(),
                        xml_compare,
                    );
                    if let Ok(true) = cmp_result {
                        return TestResult {
                            name: tc.name.clone(),
                            outcome: TestOutcome::Pass,
                            duration: start.elapsed(),
                            message: None,
                        };
                    }
                }
            }

            TestResult {
                name: tc.name.clone(),
                outcome: TestOutcome::Fail,
                duration: start.elapsed(),
                message: Some("Expected runtime error but evaluation succeeded".to_string()),
            }
        }
        Scenario::ParseError => {
            // If we got here, compilation succeeded but we expected a parse error
            TestResult {
                name: tc.name.clone(),
                outcome: TestOutcome::Fail,
                duration: start.elapsed(),
                message: Some("Expected parse error but compilation succeeded".to_string()),
            }
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut suite_path: Option<PathBuf> = None;
    let mut group_name: Option<String> = None;
    let mut test_name: Option<String> = None;
    let mut run_all = false;
    let mut verbose = false;
    let mut failed_only = false;
    let mut list_mode = false;
    let mut trace_enabled = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--suite" | "-s" => {
                if i + 1 < args.len() {
                    suite_path = Some(PathBuf::from(&args[i + 1]));
                    i += 1;
                }
            }
            "--group" | "-g" => {
                if i + 1 < args.len() {
                    group_name = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            "--test" | "-t" => {
                if i + 1 < args.len() {
                    test_name = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            "--all" | "-a" => {
                run_all = true;
            }
            "--verbose" | "-v" => {
                verbose = true;
            }
            "--failed" | "-f" => {
                failed_only = true;
            }
            "--list" | "-l" => {
                list_mode = true;
            }
            "--trace" => {
                trace_enabled = true;
            }
            "--help" | "-h" => {
                print_usage();
                return;
            }
            _ => {}
        }
        i += 1;
    }

    let suite_path = match suite_path {
        Some(p) => p,
        None => {
            println!("XQTS XPath 2.0 Conformance Test Driver\n");
            println!("No suite path specified. Use --suite /path/to/XQTS_1_0_2\n");
            println!("To run XQTS tests, you need the W3C XQTS 1.0.2 test suite.");
            println!("Download from: http://www.w3.org/XML/Query/test-suite/");
            return;
        }
    };

    let catalog_path = suite_path.join("XQTSCatalog.xml");
    if !catalog_path.exists() {
        eprintln!(
            "Error: XQTSCatalog.xml not found at {}",
            catalog_path.display()
        );
        std::process::exit(1);
    }

    println!("Loading XQTS catalog from {}...", catalog_path.display());
    let (config, sources, root_group) = match catalog::parse_catalog(&catalog_path) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("Error parsing catalog: {}", e);
            std::process::exit(1);
        }
    };
    println!("Loaded {} source files.", sources.len());

    // List mode
    if list_mode {
        println!("\nTest Groups:");
        filter::list_groups(&root_group, 0);
        return;
    }

    // Determine which tests to run
    let tests: Vec<&XqtsTestCase> = if let Some(ref name) = test_name {
        match filter::find_test(&root_group, name) {
            Some(tc) => vec![tc],
            None => {
                eprintln!("Error: test '{}' not found", name);
                std::process::exit(1);
            }
        }
    } else if let Some(ref name) = group_name {
        let tests = filter::collect_group_tests(&root_group, name);
        if tests.is_empty() {
            eprintln!("Error: group '{}' not found or has no tests", name);
            std::process::exit(1);
        }
        tests
    } else if run_all {
        filter::collect_xpath2_tests(&root_group)
    } else {
        eprintln!("Error: specify --all, --group, or --test. Use --help for usage.");
        std::process::exit(1);
    };

    println!("Running {} tests...\n", tests.len());

    let names = NameTable::new();
    let mut results: Vec<TestResult> = Vec::new();
    let overall_start = Instant::now();

    for tc in &tests {
        let tr = execute_test(tc, &config, &sources, &names, verbose, trace_enabled);
        let show = if failed_only {
            tr.outcome == TestOutcome::Fail
        } else {
            verbose
        };
        if show {
            let status = match tr.outcome {
                TestOutcome::Pass => "PASS",
                TestOutcome::Fail => "FAIL",
                TestOutcome::Skip => "SKIP",
                TestOutcome::Error => "ERR ",
            };
            print!(
                "[{}] {} ({:.1}ms)",
                status,
                tr.name,
                tr.duration.as_secs_f64() * 1000.0
            );
            if let Some(ref msg) = tr.message {
                print!(" - {}", msg);
            }
            println!();
        }
        results.push(tr);
    }

    let total_duration = overall_start.elapsed();

    // Summary
    let passed = results
        .iter()
        .filter(|r| r.outcome == TestOutcome::Pass)
        .count();
    let failed = results
        .iter()
        .filter(|r| r.outcome == TestOutcome::Fail)
        .count();
    let skipped = results
        .iter()
        .filter(|r| r.outcome == TestOutcome::Skip)
        .count();
    let errors = results
        .iter()
        .filter(|r| r.outcome == TestOutcome::Error)
        .count();
    let total = results.len();

    println!("\n{}", "=".repeat(60));
    println!("XQTS XPath 2.0 Test Results");
    println!("{}", "=".repeat(60));
    println!("Total:   {}", total);
    println!(
        "Passed:  {} ({:.1}%)",
        passed,
        if total > 0 {
            passed as f64 / total as f64 * 100.0
        } else {
            0.0
        }
    );
    println!("Failed:  {}", failed);
    println!("Skipped: {}", skipped);
    println!("Errors:  {}", errors);
    println!("Time:    {:.2}s", total_duration.as_secs_f64());
    println!("{}", "=".repeat(60));

    // Print failed tests if not already shown (verbose or --failed already showed them)
    if !verbose && !failed_only && failed > 0 {
        println!("\nFailed tests:");
        for r in &results {
            if r.outcome == TestOutcome::Fail {
                print!("  {}", r.name);
                if let Some(ref msg) = r.message {
                    print!(" - {}", msg);
                }
                println!();
            }
        }
    }

    if failed > 0 {
        std::process::exit(1);
    }
}
