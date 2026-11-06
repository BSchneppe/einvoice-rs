//! XPath 2.0 QName functions.
//!
//! This module implements:
//! - fn:resolve-QName($qname, $element) - resolve a QName against element's namespaces
//! - fn:QName($uri, $local) - construct a QName from URI and local name
//! - fn:prefix-from-QName($arg) - extract prefix from QName
//! - fn:local-name-from-QName($arg) - extract local name from QName
//! - fn:namespace-uri-from-QName($arg) - extract namespace URI from QName
//! - fn:namespace-uri-for-prefix($prefix, $element) - lookup namespace for prefix
//! - fn:in-scope-prefixes($element) - return all prefixes in scope

use crate::xpath::context::DynamicContext;
use crate::xpath::error::XPathError;
use crate::xpath::DomNavigator;
use crate::xpath::{DomNodeType, NamespaceAxisScope};

use super::{atomize_to_single_opt, atomize_to_string_opt, XPathValue};
use crate::namespace::qname::{is_ncname, QualifiedName};
use crate::types::value::{QNameValue, XmlAtomicValue, XmlValue, XmlValueKind};
use crate::types::XmlTypeCode;
use crate::xpath::iterator::XmlItem;

/// fn:resolve-QName($qname as xs:string?, $element as element()) as xs:QName?
///
/// Resolves a lexical QName using the in-scope namespaces of an element node.
///
/// - If $qname is empty, returns empty sequence.
/// - FORG0001 if $qname is not a valid lexical QName.
/// - XPST0081 (FONS0004) if the prefix is not bound.
pub fn resolve_qname<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 2 {
        return Err(XPathError::wrong_number_of_arguments(
            "resolve-QName",
            2,
            args.len(),
        ));
    }

    let element = get_element_arg(&mut args)?;
    let qname_arg = args.remove(0);
    let qname_str = atomize_to_string_opt(qname_arg)?;

    // If $qname is the empty sequence, return empty. A $qname that is not a
    // lexical QName — including the zero-length string — is FOCA0002 here,
    // not the FORG0001 of a failed `cast as xs:QName`.
    let qname_str = match qname_str {
        None => return Ok(XPathValue::Empty),
        Some(s) if s.is_empty() => {
            return Err(XPathError::FOCA0002 {
                value: String::new(),
            });
        }
        Some(s) => s,
    };

    // Parse the lexical QName
    let (prefix, local_name) =
        parse_lexical_qname(&qname_str).map_err(|_| XPathError::FOCA0002 {
            value: qname_str.clone(),
        })?;

    // Lookup namespace for prefix using element's in-scope namespaces
    let namespace_uri = lookup_namespace_for_prefix(&element, prefix.as_deref())?;

    // Create the QName value
    let qn = create_qname_value(
        context,
        namespace_uri.as_deref(),
        &local_name,
        prefix.as_deref(),
    );
    Ok(qn)
}

/// fn:QName($paramURI as xs:string?, $paramQName as xs:string) as xs:QName
///
/// Constructs a QName from a namespace URI and a lexical QName.
///
/// - $paramURI is the namespace URI (empty string = no namespace)
/// - $paramQName is the lexical form "prefix:local" or just "local"
/// - FORG0001 if $paramQName is not valid
/// - FOCA0002 if prefix with no namespace
pub fn qname_constructor<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 2 {
        return Err(XPathError::wrong_number_of_arguments(
            "QName",
            2,
            args.len(),
        ));
    }

    let local_arg = args.pop().unwrap();
    let uri_arg = args.pop().unwrap();

    let param_qname = super::atomize_to_string_required(local_arg)?;
    let param_uri = atomize_to_string_opt(uri_arg)?;

    // Parse the lexical QName
    let (prefix, local_name) = parse_lexical_qname(&param_qname)?;

    // Determine namespace URI
    let namespace_uri = match param_uri {
        None => None,
        Some(s) if s.is_empty() => None,
        Some(s) => Some(s),
    };

    // Check constraint: if prefix is present, namespace must be non-empty
    if prefix.is_some() && namespace_uri.is_none() {
        return Err(XPathError::FOCA0002 { value: param_qname });
    }

    // Create the QName value
    let qn = create_qname_value(
        context,
        namespace_uri.as_deref(),
        &local_name,
        prefix.as_deref(),
    );
    Ok(qn)
}

/// fn:prefix-from-QName($arg as xs:QName?) as xs:NCName?
///
/// Returns the prefix component of a QName, or empty if no prefix.
pub fn prefix_from_qname<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "prefix-from-QName",
            1,
            args.len(),
        ));
    }

    let arg = args.remove(0);
    let qname = atomize_to_qname(context, arg)?;

    match qname {
        None => Ok(XPathValue::Empty),
        Some(qn) => match qn.prefix {
            None => Ok(XPathValue::Empty),
            Some(prefix_id) => {
                let prefix = context
                    .static_context
                    .names
                    .try_resolve(prefix_id)
                    .unwrap_or_default();
                if prefix.is_empty() {
                    Ok(XPathValue::Empty)
                } else {
                    Ok(make_ncname(prefix))
                }
            }
        },
    }
}

/// fn:local-name-from-QName($arg as xs:QName?) as xs:NCName?
///
/// Returns the local name component of a QName.
pub fn local_name_from_qname<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "local-name-from-QName",
            1,
            args.len(),
        ));
    }

    let arg = args.remove(0);
    let qname = atomize_to_qname(context, arg)?;

    match qname {
        None => Ok(XPathValue::Empty),
        Some(qn) => {
            let local = context
                .static_context
                .names
                .try_resolve(qn.local_name)
                .unwrap_or_default();
            Ok(make_ncname(local))
        }
    }
}

/// fn:namespace-uri-from-QName($arg as xs:QName?) as xs:anyURI?
///
/// Returns the namespace URI component of a QName.
pub fn namespace_uri_from_qname<N: DomNavigator>(
    context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "namespace-uri-from-QName",
            1,
            args.len(),
        ));
    }

    let arg = args.remove(0);
    let qname = atomize_to_qname(context, arg)?;

    match qname {
        None => Ok(XPathValue::Empty),
        Some(qn) => {
            match qn.namespace_uri {
                None => {
                    // Return empty anyURI (not empty sequence)
                    Ok(make_any_uri(""))
                }
                Some(ns_id) => {
                    let ns = context
                        .static_context
                        .names
                        .try_resolve(ns_id)
                        .unwrap_or_default();
                    Ok(make_any_uri(&ns))
                }
            }
        }
    }
}

/// fn:namespace-uri-for-prefix($prefix as xs:string?, $element as element()) as xs:anyURI?
///
/// Returns the namespace URI bound to `$prefix` in the in-scope namespaces of
/// `$element`.
///
/// - An absent (`()`) or zero-length `$prefix` asks for the element's *default*
///   namespace, i.e. the binding whose prefix is the empty one.
/// - `xml` is bound in every element's in-scope namespaces, so it always
///   answers the XML namespace.
/// - When the prefix — the empty one included — has no binding, the result is
///   the **empty sequence**, not `xs:anyURI("")`. A namespace undeclaration
///   (`xmlns=""`) counts as "no binding".
///
/// From memory of F&O §14 (the specification is not checked out locally),
/// corroborated by XQTS `fn-namespace-uri-for-prefix-2` / `-6` / `-16` / `-17`,
/// which wrap the call in `fn:count(...)` and expect `0`, and by `-3` / `-4`,
/// which expect the default namespace for `""` and `()`.
pub fn namespace_uri_for_prefix<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 2 {
        return Err(XPathError::wrong_number_of_arguments(
            "namespace-uri-for-prefix",
            2,
            args.len(),
        ));
    }

    let element = get_element_arg(&mut args)?;
    let prefix_arg = args.remove(0);
    let prefix = atomize_to_string_opt(prefix_arg)?;

    // An absent prefix is the same request as a zero-length one: the binding
    // for the empty prefix, i.e. the element's default namespace.
    match lookup_namespace_for_prefix_opt(&element, prefix.as_deref()) {
        Some(ns) => Ok(make_any_uri(&ns)),
        // Not bound — including the empty prefix when the element has no
        // default namespace, and a prefix undeclared by `xmlns=""`.
        None => Ok(XPathValue::Empty),
    }
}

/// fn:in-scope-prefixes($element as element()) as xs:string*
///
/// Returns a sequence of all prefixes bound to namespaces in scope for an element.
/// Always includes "xml" prefix. F&O §11.2.6: "For namespaces that have a
/// prefix, it returns the prefix as an xs:NCName. For the default namespace,
/// which has no prefix, it returns the zero-length string."
pub fn in_scope_prefixes<N: DomNavigator>(
    _context: &mut DynamicContext<'_, N>,
    mut args: Vec<XPathValue<N>>,
) -> Result<XPathValue<N>, XPathError> {
    if args.len() != 1 {
        return Err(XPathError::wrong_number_of_arguments(
            "in-scope-prefixes",
            1,
            args.len(),
        ));
    }

    let element = get_element_arg(&mut args)?;

    let mut prefixes = Vec::new();

    // Always include "xml" prefix
    prefixes.push("xml".to_string());

    // Iterate over namespace axis
    let mut nav = element.clone();
    if nav.move_to_first_namespace(NamespaceAxisScope::All) {
        loop {
            let prefix = nav.local_name();
            // Skip if already in list (shouldn't happen but be safe)
            if !prefixes.iter().any(|p| p == prefix) {
                prefixes.push(prefix.to_string());
            }
            if !nav.move_to_next_namespace(NamespaceAxisScope::All) {
                break;
            }
        }
    }

    // An `xs:NCName` per prefix, an `xs:string` for the default namespace.
    let items: Vec<XmlItem<N>> = prefixes
        .into_iter()
        .map(|p| {
            XmlItem::Atomic(if p.is_empty() {
                XmlValue::string(p)
            } else {
                ncname_value(p)
            })
        })
        .collect();

    Ok(XPathValue::from_sequence(items))
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Parse a lexical QName string into (prefix, local_name).
///
/// This function validates that both the prefix (if present) and local name
/// are valid NCNames per XML namespaces specification.
pub fn parse_lexical_qname(qname: &str) -> Result<(Option<String>, String), XPathError> {
    let qname = qname.trim();

    if qname.is_empty() {
        return Err(XPathError::invalid_cast_value(qname, "xs:QName"));
    }

    match qname.find(':') {
        Some(pos) if pos > 0 && pos < qname.len() - 1 => {
            let prefix = &qname[..pos];
            let local = &qname[pos + 1..];

            // Validate prefix is NCName
            if !is_ncname(prefix) {
                return Err(XPathError::invalid_cast_value(qname, "xs:QName"));
            }

            // Validate local is NCName (no additional colons)
            if !is_ncname(local) || local.contains(':') {
                return Err(XPathError::invalid_cast_value(qname, "xs:QName"));
            }

            Ok((Some(prefix.to_string()), local.to_string()))
        }
        Some(_) => {
            // Colon at start or end
            Err(XPathError::invalid_cast_value(qname, "xs:QName"))
        }
        None => {
            // No prefix
            if !is_ncname(qname) {
                return Err(XPathError::invalid_cast_value(qname, "xs:QName"));
            }
            Ok((None, qname.to_string()))
        }
    }
}

/// Get an element node from the arguments list.
fn get_element_arg<N: DomNavigator>(args: &mut Vec<XPathValue<N>>) -> Result<N, XPathError> {
    let arg = args
        .pop()
        .ok_or_else(|| XPathError::type_mismatch("element()", "empty-sequence()"))?;

    match arg {
        XPathValue::Item(XmlItem::Node(nav)) => {
            if nav.node_type() == DomNodeType::Element {
                Ok(nav)
            } else {
                Err(XPathError::type_mismatch("element()", "node()"))
            }
        }
        XPathValue::Sequence(items) if items.len() == 1 => {
            if let Some(XmlItem::Node(nav)) = items.into_iter().next() {
                if nav.node_type() == DomNodeType::Element {
                    Ok(nav)
                } else {
                    Err(XPathError::type_mismatch("element()", "node()"))
                }
            } else {
                Err(XPathError::type_mismatch("element()", "atomic value"))
            }
        }
        _ => Err(XPathError::type_mismatch("element()", "sequence")),
    }
}

/// Atomize a value to a QualifiedName.
fn atomize_to_qname<N: DomNavigator>(
    _context: &DynamicContext<'_, N>,
    value: XPathValue<N>,
) -> Result<Option<QualifiedName>, XPathError> {
    let atomic = atomize_to_single_opt(value)?;

    match atomic {
        None => Ok(None),
        Some(value) => {
            if let Some(qn) = value.as_qname() {
                Ok(Some(qn.clone()))
            } else {
                Err(XPathError::type_mismatch(
                    "xs:QName",
                    format!("{:?}", value.type_code),
                ))
            }
        }
    }
}

/// Lookup namespace for a prefix using element's namespace axis.
fn lookup_namespace_for_prefix<N: DomNavigator>(
    element: &N,
    prefix: Option<&str>,
) -> Result<Option<String>, XPathError> {
    let result = lookup_namespace_for_prefix_opt(element, prefix);

    // For resolve-QName an undefined prefix is FONS0004: the element has no
    // binding for it. (XPST0081 is the *static* error for a prefix that the
    // expression's own static context cannot expand.)
    if prefix.is_some() && prefix != Some("") && result.is_none() {
        return Err(XPathError::no_namespace_for_prefix(prefix.unwrap_or("")));
    }

    Ok(result)
}

/// Lookup namespace for a prefix, returning None if not found.
fn lookup_namespace_for_prefix_opt<N: DomNavigator>(
    element: &N,
    prefix: Option<&str>,
) -> Option<String> {
    let target_prefix = prefix.unwrap_or("");

    let mut nav = element.clone();
    if nav.move_to_first_namespace(NamespaceAxisScope::All) {
        loop {
            let ns_prefix = nav.local_name();
            if ns_prefix == target_prefix {
                let uri = nav.value();
                if uri.is_empty() {
                    return None; // Empty namespace (undeclaration)
                }
                return Some(uri);
            }
            if !nav.move_to_next_namespace(NamespaceAxisScope::All) {
                break;
            }
        }
    }

    // Special case: "xml" prefix is always bound
    if target_prefix == "xml" {
        return Some("http://www.w3.org/XML/1998/namespace".to_string());
    }

    None
}

/// Create an xs:anyURI value.
/// An `xs:NCName` value (the caller passes a name that is one).
fn ncname_value(name: impl Into<String>) -> XmlValue {
    XmlValue::new(
        XmlTypeCode::NCName,
        XmlValueKind::Atomic(XmlAtomicValue::String(name.into())),
    )
}

/// Create an `xs:NCName` XPathValue.
fn make_ncname<N: DomNavigator>(name: impl Into<String>) -> XPathValue<N> {
    XPathValue::Item(XmlItem::Atomic(ncname_value(name)))
}

fn make_any_uri<N: DomNavigator>(uri: &str) -> XPathValue<N> {
    let value = XmlValue::new(
        XmlTypeCode::AnyUri,
        XmlValueKind::Atomic(XmlAtomicValue::AnyUri(uri.to_string())),
    );
    XPathValue::Item(XmlItem::Atomic(value))
}

/// Create a QName XPathValue.
///
/// Uses interior mutability of NameTable to intern the strings at runtime.
fn create_qname_value<N: DomNavigator>(
    context: &DynamicContext<'_, N>,
    namespace_uri: Option<&str>,
    local_name: &str,
    prefix: Option<&str>,
) -> XPathValue<N> {
    // Intern strings in the NameTable using interior mutability
    let names = context.static_context.names;

    let local_id = names.add(local_name);
    let ns_id = namespace_uri.map(|ns| names.add(ns));
    let prefix_id = prefix.map(|p| names.add(p));

    let qn = QNameValue::new(QualifiedName::new(ns_id, local_id, prefix_id), names);

    // Create a QName value
    let value = XmlValue::new(
        XmlTypeCode::QName,
        XmlValueKind::Atomic(XmlAtomicValue::QName(qn)),
    );

    XPathValue::Item(XmlItem::Atomic(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespace::table::NameTable;
    use crate::xpath::context::XPathContext;
    use crate::xpath::RoXmlNavigator;

    fn create_context<'a>(names: &'a NameTable) -> DynamicContext<'a, RoXmlNavigator<'a>> {
        let static_ctx = XPathContext::new(names);
        let static_ctx = Box::leak(Box::new(static_ctx));
        DynamicContext::new(static_ctx, 0)
    }

    #[test]
    fn test_qname_constructor_and_extraction() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        // Create a QName: fn:QName("http://example.com", "p:local")
        let qname_result = qname_constructor(
            &mut ctx,
            vec![
                XPathValue::string("http://example.com"),
                XPathValue::string("p:local"),
            ],
        )
        .unwrap();

        // Now extract the components
        // prefix-from-QName should return "p"
        let prefix_result = prefix_from_qname(&mut ctx, vec![qname_result.clone()]).unwrap();
        if let XPathValue::Item(XmlItem::Atomic(value)) = prefix_result {
            assert_eq!(value.as_string(), Some("p"));
        } else {
            panic!("Expected string for prefix");
        }

        // local-name-from-QName should return "local"
        let local_result = local_name_from_qname(&mut ctx, vec![qname_result.clone()]).unwrap();
        if let XPathValue::Item(XmlItem::Atomic(value)) = local_result {
            assert_eq!(value.as_string(), Some("local"));
        } else {
            panic!("Expected string for local name");
        }

        // namespace-uri-from-QName should return "http://example.com"
        let ns_result = namespace_uri_from_qname(&mut ctx, vec![qname_result]).unwrap();
        if let XPathValue::Item(XmlItem::Atomic(value)) = ns_result {
            assert_eq!(value.to_string_value(), "http://example.com");
        } else {
            panic!("Expected anyURI for namespace");
        }
    }

    #[test]
    fn test_qname_unprefixed() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        // Create unprefixed QName
        let qname_result = qname_constructor(
            &mut ctx,
            vec![
                XPathValue::Empty, // No namespace
                XPathValue::string("localonly"),
            ],
        )
        .unwrap();

        // prefix-from-QName should return empty sequence
        let prefix_result = prefix_from_qname(&mut ctx, vec![qname_result.clone()]).unwrap();
        assert!(matches!(prefix_result, XPathValue::Empty));

        // local-name-from-QName should return "localonly"
        let local_result = local_name_from_qname(&mut ctx, vec![qname_result]).unwrap();
        if let XPathValue::Item(XmlItem::Atomic(value)) = local_result {
            assert_eq!(value.as_string(), Some("localonly"));
        } else {
            panic!("Expected string for local name");
        }
    }

    #[test]
    fn test_qname_prefix_with_no_namespace_error() {
        let names = NameTable::new();
        let mut ctx = create_context(&names);

        // Prefix without namespace should error (FOCA0002)
        let result = qname_constructor(
            &mut ctx,
            vec![
                XPathValue::Empty, // No namespace but prefix present
                XPathValue::string("p:local"),
            ],
        );

        assert!(result.is_err());
        if let Err(XPathError::FOCA0002 { .. }) = result {
            // Expected
        } else {
            panic!("Expected FOCA0002 error");
        }
    }

    #[test]
    fn test_parse_lexical_qname_prefixed() {
        let (prefix, local) = parse_lexical_qname("xs:string").unwrap();
        assert_eq!(prefix, Some("xs".to_string()));
        assert_eq!(local, "string");
    }

    #[test]
    fn test_parse_lexical_qname_unprefixed() {
        let (prefix, local) = parse_lexical_qname("localName").unwrap();
        assert_eq!(prefix, None);
        assert_eq!(local, "localName");
    }

    #[test]
    fn test_parse_lexical_qname_invalid() {
        assert!(parse_lexical_qname("").is_err());
        assert!(parse_lexical_qname(":local").is_err());
        assert!(parse_lexical_qname("prefix:").is_err());
        assert!(parse_lexical_qname("a:b:c").is_err());
        assert!(parse_lexical_qname("123invalid").is_err());
    }

    #[test]
    fn test_parse_lexical_qname_with_whitespace() {
        let (prefix, local) = parse_lexical_qname("  xs:string  ").unwrap();
        assert_eq!(prefix, Some("xs".to_string()));
        assert_eq!(local, "string");
    }
}
