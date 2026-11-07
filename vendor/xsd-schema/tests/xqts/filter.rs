use std::collections::HashSet;

use super::catalog::{TestGroup, XqtsTestCase};

/// Groups to exclude from MinimalConformance tests.
const EXCLUDED_GROUPS: &[&str] = &[
    "QuantExprWith",
    "XQueryComment",
    "Surrogates",
    "SeqIDFunc",
    "SeqCollectionFunc",
    "SeqDocFunc",
    "StaticBaseURIFunc",
];

/// Individual test names to ignore (ported from C# Form1.cs lines 59-79).
const IGNORED_TESTS: &[&str] = &[
    "nametest-1",
    "nametest-2",
    "nametest-5",
    "nametest-6",
    "nametest-7",
    "nametest-8",
    "nametest-9",
    "nametest-10",
    "nametest-11",
    "nametest-12",
    "nametest-13",
    "nametest-14",
    "nametest-15",
    "nametest-16",
    "nametest-17",
    "nametest-18",
    "CastAs660",
    "CastAs661",
    "CastAs662",
    "CastAs663",
    "CastAs664",
    "CastAs665",
    "CastAs666",
    "CastAs667",
    "CastAs668",
    "CastAs669",
    "CastAs671",
    "CastableAs648",
    "fn-trace-2",
    "fn-trace-9",
    "NodeTesthc-1",
    "NodeTesthc-2",
    "NodeTesthc-3",
    "NodeTesthc-4",
    "NodeTesthc-5",
    "NodeTesthc-6",
    "NodeTesthc-7",
    "NodeTesthc-8",
    "fn-max-3",
    "fn-min-3",
    "defaultnamespacedeclerr-1",
    "defaultnamespacedeclerr-2",
    "fn-document-uri-12",
    "fn-document-uri-15",
    "fn-document-uri-16",
    "fn-document-uri-17",
    "fn-document-uri-18",
    "fn-document-uri-19",
    "fn-prefix-from-qname-8",
    "boundaryspacedeclerr-1",
    "fn-resolve-uri-2",
    "ancestor-21",
    "ancestorself-21",
    "following-21",
    "followingsibling-21",
    "preceding-21",
    "preceding-sibling-21",
];

/// Find a test group by name in the tree (recursive DFS).
pub fn find_group<'a>(root: &'a TestGroup, name: &str) -> Option<&'a TestGroup> {
    if root.name == name {
        return Some(root);
    }
    for child in &root.children {
        if let Some(found) = find_group(child, name) {
            return Some(found);
        }
    }
    None
}

/// Find a single test case by name in the tree.
pub fn find_test<'a>(root: &'a TestGroup, name: &str) -> Option<&'a XqtsTestCase> {
    for tc in &root.test_cases {
        if tc.name == name {
            return Some(tc);
        }
    }
    for child in &root.children {
        if let Some(found) = find_test(child, name) {
            return Some(found);
        }
    }
    None
}

/// Collect all test cases from a group (recursively).
fn collect_all_tests<'a>(group: &'a TestGroup, out: &mut Vec<&'a XqtsTestCase>) {
    for tc in &group.test_cases {
        out.push(tc);
    }
    for child in &group.children {
        collect_all_tests(child, out);
    }
}

/// Collect all test cases from a named group.
pub fn collect_group_tests<'a>(root: &'a TestGroup, name: &str) -> Vec<&'a XqtsTestCase> {
    let mut tests = Vec::new();
    if let Some(group) = find_group(root, name) {
        collect_all_tests(group, &mut tests);
    }
    tests
}

/// Collect all XPath 2.0 applicable tests.
///
/// This replicates the C# filtering logic:
/// 1. Start with all tests from MinimalConformance where is_xpath2 != false
/// 2. Remove tests from excluded groups
/// 3. Add all tests from FullAxis
/// 4. Remove individually ignored tests
pub fn collect_xpath2_tests(root: &TestGroup) -> Vec<&XqtsTestCase> {
    let ignored: HashSet<&str> = IGNORED_TESTS.iter().copied().collect();

    // Collect names from excluded groups to remove
    let mut excluded_names: HashSet<String> = HashSet::new();
    for group_name in EXCLUDED_GROUPS {
        let tests = collect_group_tests(root, group_name);
        for tc in tests {
            excluded_names.insert(tc.name.clone());
        }
    }

    // Start with MinimalConformance tests
    let mut result: Vec<&XqtsTestCase> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    let mc_tests = collect_group_tests(root, "MinimalConformance");
    for tc in mc_tests {
        if tc.is_xpath2 != Some(false)
            && !excluded_names.contains(&tc.name)
            && !ignored.contains(tc.name.as_str())
            && seen.insert(tc.name.clone())
        {
            result.push(tc);
        }
    }

    // Add FullAxis tests
    let fa_tests = collect_group_tests(root, "FullAxis");
    for tc in fa_tests {
        if !ignored.contains(tc.name.as_str()) && seen.insert(tc.name.clone()) {
            result.push(tc);
        }
    }

    result
}

/// List all group names in the tree (for --list mode).
pub fn list_groups(group: &TestGroup, depth: usize) {
    let indent = "  ".repeat(depth);
    let test_count = count_tests(group);
    println!("{}{} ({} tests)", indent, group.name, test_count);
    for child in &group.children {
        list_groups(child, depth + 1);
    }
}

fn count_tests(group: &TestGroup) -> usize {
    let mut count = group.test_cases.len();
    for child in &group.children {
        count += count_tests(child);
    }
    count
}
