use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Configuration extracted from the XQTS catalog root element.
#[allow(dead_code)]
pub struct CatalogConfig {
    pub base_path: PathBuf,
    pub source_offset_path: String,
    pub query_offset_path: String,
    pub result_offset_path: String,
    pub query_file_extension: String,
}

/// Scenario type for a test case.
#[derive(Debug, Clone, PartialEq)]
pub enum Scenario {
    Standard,
    ParseError,
    RuntimeError,
}

/// An input-file binding for a test case.
#[derive(Debug, Clone)]
pub struct InputFile {
    pub variable: String,
    pub source_id: String,
}

/// An input-URI binding for a test case.
#[derive(Debug, Clone)]
pub struct InputUri {
    pub variable: String,
    pub value: String,
}

/// An output-file specification for a test case.
#[derive(Debug, Clone)]
pub struct OutputFile {
    pub compare: String,
    pub file_name: String,
}

/// A single XQTS test case parsed from the catalog.
#[allow(dead_code)]
pub struct XqtsTestCase {
    pub name: String,
    pub file_path: String,
    pub scenario: Scenario,
    pub is_xpath2: Option<bool>,
    pub creator: String,
    pub description: String,
    pub query_name: String,
    pub input_files: Vec<InputFile>,
    pub context_item: Option<String>,
    pub input_uris: Vec<InputUri>,
    pub output_files: Vec<OutputFile>,
    pub expected_errors: Vec<String>,
}

/// A group in the test hierarchy.
pub struct TestGroup {
    pub name: String,
    pub children: Vec<TestGroup>,
    pub test_cases: Vec<XqtsTestCase>,
}

const XQTS_NS: &str = "http://www.w3.org/2005/02/query-test-XQTSCatalog";

/// Parse the XQTS catalog and return config, source map, and root test group.
pub fn parse_catalog(
    catalog_path: &Path,
) -> Result<(CatalogConfig, HashMap<String, PathBuf>, TestGroup), String> {
    let base_path = catalog_path
        .parent()
        .ok_or("Cannot determine catalog directory")?
        .to_path_buf();

    let xml = std::fs::read_to_string(catalog_path)
        .map_err(|e| format!("Failed to read catalog: {}", e))?;

    let doc = roxmltree::Document::parse(&xml)
        .map_err(|e| format!("Failed to parse catalog XML: {}", e))?;

    let root = doc.root_element();

    // Verify namespace and element name
    if root.tag_name().namespace() != Some(XQTS_NS) || root.tag_name().name() != "test-suite" {
        return Err("Input file is not XQTS catalog.".to_string());
    }
    if root.attribute("version") != Some("1.0.2") {
        return Err("Only version 1.0.2 of XQTS is supported.".to_string());
    }

    let config = CatalogConfig {
        base_path: base_path.clone(),
        source_offset_path: root
            .attribute("SourceOffsetPath")
            .unwrap_or("./")
            .to_string(),
        query_offset_path: root
            .attribute("XQueryQueryOffsetPath")
            .unwrap_or("Queries/XQuery/")
            .to_string(),
        result_offset_path: root
            .attribute("ResultOffsetPath")
            .unwrap_or("ExpectedTestResults/")
            .to_string(),
        query_file_extension: root
            .attribute("XQueryFileExtension")
            .unwrap_or(".xq")
            .to_string(),
    };

    // Build source map: ID -> full path
    let mut sources = HashMap::new();
    for child in root.children() {
        if child.tag_name().name() == "sources" && child.tag_name().namespace() == Some(XQTS_NS) {
            for source in child.children() {
                if source.tag_name().name() == "source"
                    && source.tag_name().namespace() == Some(XQTS_NS)
                {
                    if let (Some(id), Some(filename)) =
                        (source.attribute("ID"), source.attribute("FileName"))
                    {
                        let full_path = base_path.join(filename.replace('\\', "/"));
                        sources.insert(id.to_string(), full_path);
                    }
                }
            }
            break;
        }
    }

    // Build test group tree
    let root_group = TestGroup {
        name: "test-suite".to_string(),
        children: parse_test_groups(&root, XQTS_NS),
        test_cases: Vec::new(),
    };

    Ok((config, sources, root_group))
}

fn parse_test_groups(parent: &roxmltree::Node, ns: &str) -> Vec<TestGroup> {
    let mut groups = Vec::new();
    for child in parent.children() {
        if child.tag_name().name() == "test-group" && child.tag_name().namespace() == Some(ns) {
            let name = child.attribute("name").unwrap_or("").to_string();
            let test_cases = parse_test_cases(&child, ns);
            let children = parse_test_groups(&child, ns);
            groups.push(TestGroup {
                name,
                children,
                test_cases,
            });
        }
    }
    groups
}

fn parse_test_cases(group: &roxmltree::Node, ns: &str) -> Vec<XqtsTestCase> {
    let mut cases = Vec::new();
    for child in group.children() {
        if child.tag_name().name() == "test-case" && child.tag_name().namespace() == Some(ns) {
            if let Some(tc) = parse_test_case(&child, ns) {
                cases.push(tc);
            }
        }
    }
    cases
}

fn parse_test_case(node: &roxmltree::Node, ns: &str) -> Option<XqtsTestCase> {
    let name = node.attribute("name")?.to_string();
    let file_path = node.attribute("FilePath").unwrap_or("").to_string();
    let scenario = match node.attribute("scenario").unwrap_or("standard") {
        "parse-error" => Scenario::ParseError,
        "runtime-error" => Scenario::RuntimeError,
        _ => Scenario::Standard,
    };
    let is_xpath2 = match node.attribute("is-XPath2") {
        Some("true") => Some(true),
        Some("false") => Some(false),
        _ => None,
    };
    let creator = node.attribute("Creator").unwrap_or("").to_string();

    let mut description = String::new();
    let mut query_name = String::new();
    let mut input_files = Vec::new();
    let mut context_item = None;
    let mut input_uris = Vec::new();
    let mut output_files = Vec::new();
    let mut expected_errors = Vec::new();

    for child in node.children() {
        if child.tag_name().namespace() != Some(ns) {
            continue;
        }
        match child.tag_name().name() {
            "description" => {
                description = child.text().unwrap_or("").to_string();
            }
            "query" => {
                query_name = child.attribute("name").unwrap_or("").to_string();
            }
            "input-file" => {
                let variable = child.attribute("variable").unwrap_or("").to_string();
                let source_id = child.text().unwrap_or("").trim().to_string();
                input_files.push(InputFile {
                    variable,
                    source_id,
                });
            }
            "contextItem" => {
                context_item = Some(child.text().unwrap_or("").trim().to_string());
            }
            "input-URI" => {
                let variable = child.attribute("variable").unwrap_or("").to_string();
                let value = child.text().unwrap_or("").trim().to_string();
                input_uris.push(InputUri { variable, value });
            }
            "output-file" => {
                let compare = child.attribute("compare").unwrap_or("").to_string();
                let file_name = child.text().unwrap_or("").trim().to_string();
                output_files.push(OutputFile { compare, file_name });
            }
            "expected-error" => {
                let code = child.text().unwrap_or("").trim().to_string();
                if !code.is_empty() {
                    expected_errors.push(code);
                }
            }
            _ => {}
        }
    }

    Some(XqtsTestCase {
        name,
        file_path,
        scenario,
        is_xpath2,
        creator,
        description,
        query_name,
        input_files,
        context_item,
        input_uris,
        output_files,
        expected_errors,
    })
}
