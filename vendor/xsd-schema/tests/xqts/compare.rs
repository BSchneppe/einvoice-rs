use xsd_schema::document::{serialize, SerializeError, SerializeOptions};
use xsd_schema::xpath::functions::XPathValue;
use xsd_schema::xpath::iterator::XmlItem;
use xsd_schema::xpath::tree_comparer::TreeComparer;
use xsd_schema::xpath::{DomNavigator, DomNodeType};
use xsd_schema::RoXmlNavigator;

/// Special test names that force XML comparison.
const FORCE_XML_COMPARE: &[&str] = &["ReturnExpr010"];

/// Special test names that force is_single to false.
const FORCE_NOT_SINGLE: &[&str] = &["CondExpr012", "NodeTest006"];

/// Special test names treated as exceptions (no wrapping).
const IS_EXCEPTION: &[&str] = &[
    "fn-union-node-args-005",
    "fn-union-node-args-006",
    "fn-union-node-args-007",
    "fn-union-node-args-009",
    "fn-union-node-args-010",
    "fn-union-node-args-011",
];

/// Serialize XPath result items to an XML string.
///
/// `wrap_element` is the element name to wrap in (from expected output root element).
/// Per C# CompareResult: wraps in `<name>...</name>` unless `(xml_compare && is_single) || is_excpt`.
fn serialize_xpath_result<N: DomNavigator>(
    items: &[XmlItem<N>],
    xml_compare: bool,
    is_single: bool,
    is_excpt: bool,
    wrap_element: &str,
    wrap_namespaces: &[(Option<String>, String)],
) -> Result<String, SerializeError> {
    let mut out = String::new();
    let wrap = !((xml_compare && is_single) || is_excpt);

    if wrap {
        out.push_str("<?xml version='1.0'?>");
        out.push('<');
        out.push_str(wrap_element);

        // Emit namespace declarations from the expected root element
        for (prefix, uri) in wrap_namespaces {
            if let Some(pfx) = prefix {
                out.push_str(&format!(" xmlns:{}=\"{}\"", pfx, escape_xml_attr(uri)));
            } else {
                out.push_str(&format!(" xmlns=\"{}\"", escape_xml_attr(uri)));
            }
        }

        // Per C# CompareResult: attribute result items become attributes on the wrapper element
        for item in items {
            if let XmlItem::Node(nav) = item {
                if nav.node_type() == DomNodeType::Attribute {
                    let prefix = nav.prefix();
                    let local = nav.local_name();
                    let value = nav.value();
                    if prefix.is_empty() {
                        out.push_str(&format!(" {}=\"{}\"", local, escape_xml_attr(&value)));
                    } else {
                        out.push_str(&format!(
                            " {}:{}=\"{}\"",
                            prefix,
                            local,
                            escape_xml_attr(&value)
                        ));
                    }
                }
            }
        }
    }

    // Check if there are any non-attribute items
    let has_non_attr = items.iter().any(|item| match item {
        XmlItem::Node(nav) => nav.node_type() != DomNodeType::Attribute,
        XmlItem::Atomic(_) => true,
    });

    if wrap && has_non_attr {
        out.push('>');
    } else if wrap && !has_non_attr {
        out.push_str("/>");
        return Ok(out);
    }

    let mut string_flag = false;
    for item in items {
        match item {
            XmlItem::Node(nav) => {
                if nav.node_type() == DomNodeType::Attribute {
                    if !wrap {
                        // Non-wrapped: serialize attribute as text
                        let prefix = nav.prefix();
                        let local = nav.local_name();
                        let value = nav.value();
                        if prefix.is_empty() {
                            out.push_str(&format!("{}=\"{}\"", local, escape_xml_attr(&value)));
                        } else {
                            out.push_str(&format!(
                                "{}:{}=\"{}\"",
                                prefix,
                                local,
                                escape_xml_attr(&value)
                            ));
                        }
                    }
                    // When wrapping, attributes were already added to the wrapper element above
                } else {
                    // The crate's serializer, explicitly in compact mode:
                    // added layout whitespace would show up as text nodes in
                    // the comparison. Declarations land only where they are
                    // introduced, which `deep_equal` does not look at, so what
                    // changes here is nothing a comparison can see.
                    let compact = SerializeOptions {
                        indent: None,
                        ..SerializeOptions::default()
                    };
                    out.push_str(&serialize::to_string(nav, &compact)?);
                }
                string_flag = false;
            }
            XmlItem::Atomic(val) => {
                if string_flag {
                    out.push(' ');
                }
                if wrap {
                    out.push_str(&escape_xml_text(&val.to_string_value()));
                } else {
                    out.push_str(&val.to_string_value());
                }
                string_flag = true;
            }
        }
    }

    if wrap {
        out.push_str("</");
        out.push_str(wrap_element);
        out.push('>');
    }

    Ok(out)
}

/// Compare XPath result against an expected output file.
///
/// Port of C# CompareResult (Form1.cs:827-920).
/// Takes ownership of result since we need to convert to Vec.
pub fn compare_result<'a>(
    test_name: &str,
    result: XPathValue<RoXmlNavigator<'a>>,
    expected_path: &str,
    xml_compare: bool,
) -> Result<bool, String> {
    compare_result_inner(test_name, result, expected_path, xml_compare, false)
}

/// Compare with optional debug output.
pub fn compare_result_debug<'a>(
    test_name: &str,
    result: XPathValue<RoXmlNavigator<'a>>,
    expected_path: &str,
    xml_compare: bool,
) -> Result<bool, String> {
    compare_result_inner(test_name, result, expected_path, xml_compare, true)
}

fn compare_result_inner<'a>(
    test_name: &str,
    result: XPathValue<RoXmlNavigator<'a>>,
    expected_path: &str,
    xml_compare: bool,
    debug: bool,
) -> Result<bool, String> {
    let xml_compare = xml_compare || FORCE_XML_COMPARE.contains(&test_name);
    let is_excpt = IS_EXCEPTION.contains(&test_name);

    let is_single = if FORCE_NOT_SINGLE.contains(&test_name) {
        false
    } else {
        result.is_single()
    };

    // Load expected file first to extract root element name for wrapping
    let expected_content = std::fs::read_to_string(expected_path)
        .map_err(|e| format!("Failed to read expected file {}: {}", expected_path, e))?;

    let expected_xml = if xml_compare {
        expected_content
    } else {
        format!("<?xml version='1.0'?><root>{}</root>", expected_content)
    };

    // Parse expected XML to get root element name (per C# CompareResult: doc1.DocumentElement.Name)
    let doc1 = roxmltree::Document::parse(&expected_xml).map_err(|e| {
        format!(
            "Failed to parse expected XML: {} (content: {})",
            e,
            truncate(&expected_xml, 200)
        )
    })?;

    let root_elem = doc1.root().children().find(|n| n.is_element());
    let wrap_element = root_elem
        .map(|e| {
            let prefix = e
                .tag_name()
                .namespace()
                .and_then(|uri| e.lookup_prefix(uri));
            if let Some(pfx) = prefix {
                format!("{}:{}", pfx, e.tag_name().name())
            } else {
                e.tag_name().name().to_string()
            }
        })
        .unwrap_or_else(|| "root".to_string());

    // Collect namespace declarations from the root element for the wrapper
    let wrap_namespaces: Vec<(Option<String>, String)> = root_elem
        .map(|e| {
            e.namespaces()
                .map(|ns| (ns.name().map(String::from), ns.uri().to_string()))
                .collect()
        })
        .unwrap_or_default();

    // Serialize result to XML using the expected document's root element name
    let items = result.into_vec();
    let result_xml = serialize_xpath_result(
        &items,
        xml_compare,
        is_single,
        is_excpt,
        &wrap_element,
        &wrap_namespaces,
    )
    .map_err(|e| format!("Failed to serialize result: {e}"))?;

    // Compare with TreeComparer.
    // When xml_compare is true and the result is a bare atomic value (not valid XML),
    // fall back to text comparison: extract text content from expected XML and compare strings.
    match roxmltree::Document::parse(&result_xml) {
        Ok(doc2) => {
            let nav1 = RoXmlNavigator::new(&doc1);
            let nav2 = RoXmlNavigator::new(&doc2);
            let comparer = TreeComparer::with_ignore_whitespace(true);
            let eq = comparer.deep_equal(&nav1, &nav2);
            if !eq && debug {
                eprintln!("  ACTUAL:   {}", truncate(&result_xml, 300));
                eprintln!("  EXPECTED: {}", truncate(&expected_xml, 300));
            }
            Ok(eq)
        }
        Err(_) if xml_compare => {
            // Result is a bare atomic value (e.g. "false", "0"). Extract text content
            // from expected XML and compare as strings.
            let expected_text = extract_deep_text(&doc1.root());
            let eq = result_xml.trim() == expected_text.trim();
            if !eq && debug {
                eprintln!("  ACTUAL:   {}", truncate(&result_xml, 300));
                eprintln!("  EXPECTED: {}", truncate(&expected_text, 300));
            }
            Ok(eq)
        }
        Err(e) => Err(format!(
            "Failed to parse result XML: {} (content: {})",
            e,
            truncate(&result_xml, 200)
        )),
    }
}

/// Recursively extract all text content from an XML node (roxmltree).
fn extract_deep_text(node: &roxmltree::Node) -> String {
    let mut out = String::new();
    for child in node.children() {
        if child.is_text() {
            if let Some(text) = child.text() {
                out.push_str(text);
            }
        } else if child.is_element() {
            out.push_str(&extract_deep_text(&child));
        }
    }
    out
}

fn escape_xml_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_xml_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn truncate(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        &s[..max_len]
    }
}
