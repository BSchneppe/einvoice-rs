// ============================================================================
// Helper Functions
// ============================================================================

fn parse_optional_attr<T, F>(
    attrs: &AttributeMap,
    name_table: &NameTable,
    name: &str,
    parse: F,
) -> SchemaResult<Option<T>>
where
    F: FnOnce(&str) -> Result<T, String>,
{
    match attrs.get_value_by_name(name_table, name) {
        Some(value) => {
            let parsed = parse(value).map_err(|err| {
                SchemaError::structural(
                    "s4s-att-invalid-value",
                    format!("Invalid value for attribute '{}': {}", name, err),
                    None,
                )
            })?;
            Ok(Some(parsed))
        }
        None => Ok(None),
    }
}

fn validate_attr_value<T, F>(
    attrs: &AttributeMap,
    name_table: &NameTable,
    name: &str,
    parse: F,
) -> SchemaResult<()>
where
    F: FnOnce(&str) -> Result<T, String>,
{
    parse_optional_attr(attrs, name_table, name, parse).map(|_| ())
}

fn parse_optional_bool_attr(
    attrs: &AttributeMap,
    name_table: &NameTable,
    name: &str,
) -> SchemaResult<Option<bool>> {
    parse_optional_attr(attrs, name_table, name, parse_boolean)
}

fn parse_bool_attr_default(
    attrs: &AttributeMap,
    name_table: &NameTable,
    name: &str,
    default: bool,
) -> SchemaResult<bool> {
    Ok(parse_optional_bool_attr(attrs, name_table, name)?.unwrap_or(default))
}

fn parse_occurs_attr_raw(
    attrs: &AttributeMap,
    name_table: &NameTable,
    name: &str,
) -> SchemaResult<Option<Option<u32>>> {
    match attrs.get_value_by_name(name_table, name) {
        Some(value) => {
            let parsed = parse_occurs(value).map_err(|err| {
                SchemaError::structural(
                    "s4s-att-invalid-value",
                    format!("Invalid value for attribute '{}': {}", name, err),
                    None,
                )
            })?;
            Ok(Some(parsed))
        }
        None => Ok(None),
    }
}

fn parse_min_occurs_attr(
    attrs: &AttributeMap,
    name_table: &NameTable,
    name: &str,
) -> SchemaResult<u32> {
    match parse_occurs_attr_raw(attrs, name_table, name)? {
        None => Ok(1),
        Some(Some(value)) => Ok(value),
        Some(None) => Err(SchemaError::structural(
            "s4s-att-invalid-value",
            format!("Invalid value for attribute '{}': 'unbounded'", name),
            None,
        )),
    }
}

fn parse_max_occurs_attr(
    attrs: &AttributeMap,
    name_table: &NameTable,
    name: &str,
) -> SchemaResult<Option<u32>> {
    match parse_occurs_attr_raw(attrs, name_table, name)? {
        None => Ok(Some(1)),
        Some(Some(value)) => Ok(Some(value)),
        Some(None) => Ok(None),
    }
}

fn parse_process_contents_value(value: &str) -> Result<ProcessContents, String> {
    match value {
        "strict" => Ok(ProcessContents::Strict),
        "lax" => Ok(ProcessContents::Lax),
        "skip" => Ok(ProcessContents::Skip),
        _ => Err(format!("Invalid processContents value: '{}'", value)),
    }
}

#[cfg(feature = "xsd11")]
fn parse_open_content_mode(value: &str) -> Result<OpenContentMode, String> {
    match value {
        "none" => Ok(OpenContentMode::None),
        "interleave" => Ok(OpenContentMode::Interleave),
        "suffix" => Ok(OpenContentMode::Suffix),
        _ => Err(format!("Invalid open content mode: '{}'", value)),
    }
}

fn parse_process_contents_attr(
    attrs: &AttributeMap,
    name_table: &NameTable,
    name: &str,
) -> SchemaResult<ProcessContents> {
    match parse_optional_attr(attrs, name_table, name, parse_process_contents_value)? {
        Some(value) => Ok(value),
        None => Ok(ProcessContents::Strict),
    }
}

#[cfg(feature = "xsd11")]
fn parse_open_content_mode_attr(
    attrs: &AttributeMap,
    name_table: &NameTable,
    name: &str,
) -> SchemaResult<OpenContentMode> {
    match parse_optional_attr(attrs, name_table, name, parse_open_content_mode)? {
        Some(value) => Ok(value),
        None => Ok(OpenContentMode::Interleave),
    }
}

/// Parse a `block`/`final`-style attribute. `None` = attribute absent; caller
/// decides whether to inherit a document-level default. `Some(set)` = present
/// (including the empty value `""`, which explicitly overrides any default).
fn parse_derivation_set_opt(value: Option<&str>) -> SchemaResult<Option<DerivationSet>> {
    let Some(value) = value else {
        return Ok(None);
    };

    if value == "#all" {
        return Ok(Some(DerivationSet::ALL));
    }

    let mut set = DerivationSet::empty();
    for token in value.split_whitespace() {
        match token {
            "extension" => set |= DerivationSet::EXTENSION,
            "restriction" => set |= DerivationSet::RESTRICTION,
            "list" => set |= DerivationSet::LIST,
            "union" => set |= DerivationSet::UNION,
            "substitution" => set |= DerivationSet::SUBSTITUTION,
            _ => {
                return Err(SchemaError::structural(
                    "sch-props-correct",
                    format!("Invalid derivation method: '{}'", token),
                    None,
                ));
            }
        }
    }

    Ok(Some(set))
}

/// Parse a derivation set, collapsing absent into empty (caller has no
/// None-vs-empty distinction to make).
fn parse_derivation_set(value: Option<&str>) -> SchemaResult<DerivationSet> {
    Ok(parse_derivation_set_opt(value)?.unwrap_or_default())
}

/// Parse a QName reference with namespace resolution
///
/// Resolves the prefix to a namespace URI using the provided namespace context snapshot.
fn parse_qname_ref(
    value: &str,
    name_table: &NameTable,
    ns_snapshot: &NamespaceContextSnapshot,
) -> SchemaResult<QNameRef> {
    // xs:QName has whiteSpace=collapse.
    let value = value.trim();
    let (local, prefix) = if let Some(pos) = value.find(':') {
        let prefix = &value[..pos];
        let local = &value[pos + 1..];
        (local, Some(prefix))
    } else {
        (value, None)
    };

    // Validate QName lexical form: both prefix and local part must be non-empty
    // NCNames (no colons allowed in either part).
    if local.is_empty() || local.contains(':') {
        return Err(SchemaError::structural(
            "src-resolve",
            format!("Invalid QName: '{}'", value),
            None,
        ));
    }
    if let Some(p) = prefix {
        if p.is_empty() {
            return Err(SchemaError::structural(
                "src-resolve",
                format!("Invalid QName: '{}'", value),
                None,
            ));
        }
    }

    // Use add() (intern-or-get) rather than get() so that forward-referenced
    // names are interned immediately. Resolution of whether the name actually
    // exists happens later in the reference-resolution phase.
    let local_name = name_table.add(local);

    let prefix_id = prefix.and_then(|p| name_table.get(p));

    // Resolve namespace immediately using the snapshot
    let namespace = if let Some(p) = prefix {
        // src-qname: a prefixed QName whose prefix has no in-scope namespace
        // binding is a schema representation error (msData stE008:
        // memberTypes="foo:listOfMyInt" with xmlns:foo undeclared).
        match prefix_id.and_then(|pid| ns_snapshot.resolve_prefix(pid)) {
            Some(ns) => Some(ns),
            None => {
                return Err(SchemaError::structural(
                    "src-qname",
                    format!("QName '{}' uses undeclared namespace prefix '{}'", value, p),
                    None,
                ));
            }
        }
    } else {
        // For unprefixed QNames in XSD attribute values (type, ref, base, etc.),
        // use the default namespace if one is declared. Per XML namespace rules,
        // an unprefixed QName resolves using the default namespace in scope.
        ns_snapshot.default_namespace()
    };

    Ok(QNameRef {
        prefix: prefix_id,
        local_name,
        namespace,
    })
}

/// Parse a list of QName references with namespace resolution
fn parse_qname_list(
    value: &str,
    name_table: &NameTable,
    ns_snapshot: &NamespaceContextSnapshot,
) -> SchemaResult<Vec<QNameRef>> {
    value
        .split_whitespace()
        .map(|s| parse_qname_ref(s, name_table, ns_snapshot))
        .collect()
}

/// Parse namespace constraint for wildcards
fn parse_namespace_constraint(
    value: Option<&str>,
    name_table: &NameTable,
) -> SchemaResult<WildcardNamespace> {
    let Some(value) = value else {
        return Ok(WildcardNamespace::Any);
    };

    match value {
        "##any" => Ok(WildcardNamespace::Any),
        "##other" => Ok(WildcardNamespace::Other),
        "##targetNamespace" => Ok(WildcardNamespace::TargetNamespace),
        "##local" => Ok(WildcardNamespace::Local),
        _ => {
            let mut namespaces = Vec::new();
            for s in value.split_whitespace() {
                match s {
                    "##targetNamespace" => namespaces.push(NamespaceToken::TargetNamespace),
                    "##local" => namespaces.push(NamespaceToken::Local),
                    "##any" | "##other" => {
                        return Err(SchemaError::structural(
                            "src-wildcard",
                            format!("'{}' must appear alone, not in a list namespace constraint", s),
                            None,
                        ))
                    }
                    s if s.starts_with("##") => {
                        return Err(SchemaError::structural(
                            "src-wildcard",
                            format!("Unrecognized namespace token '{}'; expected ##any, ##other, ##targetNamespace, ##local, or a URI", s),
                            None,
                        ))
                    }
                    _ => namespaces.push(NamespaceToken::Uri(name_table.add(s))),
                }
            }
            Ok(WildcardNamespace::List(namespaces))
        }
    }
}

/// Parse notNamespace attribute (XSD 1.1)
#[cfg(feature = "xsd11")]
pub(crate) fn parse_not_namespace(
    value: Option<&str>,
    name_table: &NameTable,
) -> Vec<NamespaceToken> {
    let Some(value) = value else {
        return Vec::new();
    };
    value
        .split_whitespace()
        .map(|s| match s {
            "##targetNamespace" => NamespaceToken::TargetNamespace,
            "##local" => NamespaceToken::Local,
            _ => NamespaceToken::Uri(name_table.add(s)),
        })
        .collect()
}

/// Parse notQName attribute (XSD 1.1).
///
/// Validates the QName lexical form per Datatypes §3.3.18 and rejects
/// undeclared prefixes per §3.10.6.1 rule 1 (a wildcard's properties must
/// match the property tableau, including QName lexical validity). The
/// per-name namespace-allowed check from §3.10.6.1 rule 4 is applied after
/// assembly when the wildcard's resolved namespace constraint is available
/// (see `validate_wildcard_disallowed_names`).
#[cfg(feature = "xsd11")]
pub(crate) fn parse_not_qname(
    value: Option<&str>,
    name_table: &NameTable,
    ns_snapshot: &NamespaceContextSnapshot,
    is_element_wildcard: bool,
) -> SchemaResult<Vec<NotQNameItem>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let mut items = Vec::new();
    for s in value.split_whitespace() {
        match s {
            "##defined" => items.push(NotQNameItem::Defined),
            "##definedSibling" => {
                if !is_element_wildcard {
                    // §3.10.6.1 rule 5: attribute wildcards must not use sibling.
                    return Err(SchemaError::structural(
                        "w-props-correct",
                        "##definedSibling is not allowed on xs:anyAttribute".to_string(),
                        None,
                    ));
                }
                items.push(NotQNameItem::DefinedSibling);
            }
            _ => {
                // Validate QName lexical form: split on the FIRST colon and
                // require that both parts are non-empty NCNames with no
                // additional colons. This rejects ":name", "name:", and
                // "a:b:c" forms (covers wild038 / wild039).
                let (prefix, local) = match s.split_once(':') {
                    Some((p, l)) => (Some(p), l),
                    None => (None, s),
                };
                if local.is_empty() || local.contains(':') {
                    return Err(SchemaError::structural(
                        "w-props-correct",
                        format!("Invalid QName '{}' in notQName", s),
                        None,
                    ));
                }
                if let Some(p) = prefix {
                    if p.is_empty() {
                        return Err(SchemaError::structural(
                            "w-props-correct",
                            format!("Invalid QName '{}' in notQName", s),
                            None,
                        ));
                    }
                }

                let namespace = if let Some(p) = prefix {
                    // Reject undeclared prefixes (covers wild036 / wild037).
                    let prefix_id = match name_table.get(p) {
                        Some(id) => id,
                        None => {
                            return Err(SchemaError::structural(
                                "w-props-correct",
                                format!(
                                    "Undeclared prefix '{}' in notQName entry '{}'",
                                    p, s
                                ),
                                None,
                            ));
                        }
                    };
                    match ns_snapshot.resolve_prefix(prefix_id) {
                        Some(ns) => Some(ns),
                        None => {
                            return Err(SchemaError::structural(
                                "w-props-correct",
                                format!(
                                    "Undeclared prefix '{}' in notQName entry '{}'",
                                    p, s
                                ),
                                None,
                            ));
                        }
                    }
                } else {
                    // Unprefixed: per the QName datatype, unprefixed names
                    // resolve to the default namespace (if one is declared)
                    // or otherwise to the absent namespace.
                    ns_snapshot.default_namespace()
                };

                let local_name = name_table.add(local);
                items.push(NotQNameItem::QName { namespace, local_name });
            }
        }
    }
    Ok(items)
}

/// Parse an XSD nonNegativeInteger/positiveInteger facet value.
///
/// Applies whitespace collapse (the schema-for-schemas types these attributes
/// as `nonNegativeInteger`/`positiveInteger`) and strips an optional leading `+`.
fn parse_nonneg_integer<T: std::str::FromStr>(value: &str, facet_name: &str) -> SchemaResult<T> {
    let trimmed = value.trim();
    // An `xs:nonNegativeInteger`: zero "may be preceded by a positive ("+") or
    // a negative ("-") sign" (XSD 1.0 §3.3.20.1, Datatypes 1.1 §3.4.20.1).
    let trimmed = crate::types::validators::unsigned_lexical(trimmed);
    let text = trimmed.strip_prefix('+').unwrap_or(trimmed);
    text.parse().map_err(|_| {
        SchemaError::structural(
            "st-props-correct",
            format!("Invalid {} value '{}': must be a nonNegativeInteger", facet_name, value),
            None,
        )
    })
}

/// Apply a facet to a facet set
fn apply_facet(facets: &mut FacetSet, facet: FacetResult) -> SchemaResult<()> {
    use crate::types::facets::{normalize_whitespace, FacetFixed, WhitespaceMode};

    let fixed = if facet.fixed {
        FacetFixed::Fixed
    } else {
        FacetFixed::Default
    };

    let dup = |name: &str| {
        SchemaError::structural(
            "st-props-correct",
            format!("Facet '{}' must not appear more than once in a restriction", name),
            None,
        )
    };

    match facet.kind {
        FacetKind::Enumeration => {
            facets.add_enumeration_with_ns(facet.value, facet.ns_snapshot, facet.source);
        }
        FacetKind::Pattern => {
            // Use unchecked to defer pattern compilation to validation phase
            facets.add_pattern_unchecked(facet.value, facet.source);
        }
        FacetKind::MinLength => {
            if facets.min_length.is_some() { return Err(dup("minLength")); }
            facets.set_min_length(parse_nonneg_integer(&facet.value, "minLength")?, fixed, facet.source);
        }
        FacetKind::MaxLength => {
            if facets.max_length.is_some() { return Err(dup("maxLength")); }
            facets.set_max_length(parse_nonneg_integer(&facet.value, "maxLength")?, fixed, facet.source);
        }
        FacetKind::Length => {
            if facets.length.is_some() { return Err(dup("length")); }
            facets.set_length(parse_nonneg_integer(&facet.value, "length")?, fixed, facet.source);
        }
        FacetKind::MinInclusive => {
            if facets.min_inclusive.is_some() { return Err(dup("minInclusive")); }
            facets.set_min_inclusive(facet.value, fixed, facet.source);
        }
        FacetKind::MaxInclusive => {
            if facets.max_inclusive.is_some() { return Err(dup("maxInclusive")); }
            facets.set_max_inclusive(facet.value, fixed, facet.source);
        }
        FacetKind::MinExclusive => {
            if facets.min_exclusive.is_some() { return Err(dup("minExclusive")); }
            facets.set_min_exclusive(facet.value, fixed, facet.source);
        }
        FacetKind::MaxExclusive => {
            if facets.max_exclusive.is_some() { return Err(dup("maxExclusive")); }
            facets.set_max_exclusive(facet.value, fixed, facet.source);
        }
        FacetKind::TotalDigits => {
            if facets.total_digits.is_some() { return Err(dup("totalDigits")); }
            let v: u32 = parse_nonneg_integer(&facet.value, "totalDigits")?;
            if v == 0 {
                return Err(SchemaError::structural(
                    "st-props-correct",
                    "Invalid totalDigits value '0': must be a positiveInteger (> 0)",
                    None,
                ));
            }
            facets.set_total_digits(v, fixed, facet.source);
        }
        FacetKind::FractionDigits => {
            if facets.fraction_digits.is_some() { return Err(dup("fractionDigits")); }
            facets.set_fraction_digits(parse_nonneg_integer(&facet.value, "fractionDigits")?, fixed, facet.source);
        }
        FacetKind::WhiteSpace => {
            if facets.whitespace.is_some() { return Err(dup("whiteSpace")); }
            // The `value` is an `xs:NMTOKEN` in the schema for schemas
            // (Datatypes Appendix A), so it is read after whitespace collapse;
            // §4.3.6 (XSD 1.0 alike): "The value of whiteSpace must be one of
            // {preserve, replace, collapse}".
            let mode = match normalize_whitespace(&facet.value, WhitespaceMode::Collapse).as_ref() {
                "preserve" => WhitespaceMode::Preserve,
                "replace" => WhitespaceMode::Replace,
                "collapse" => WhitespaceMode::Collapse,
                other => {
                    return Err(SchemaError::structural(
                        "st-props-correct",
                        format!(
                            "Invalid whiteSpace value '{}': expected 'preserve', 'replace', or 'collapse'",
                            other
                        ),
                        None,
                    ));
                }
            };
            facets.set_whitespace(mode, fixed, facet.source);
        }
        FacetKind::Assertion => {
            // XSD 1.1: assertion facet - the value is the XPath test expression
            facets.add_assertion(
                facet.value,
                facet.xpath_default_namespace,
                facet.ns_snapshot.unwrap_or_default(),
                facet.source,
            );
        }
        FacetKind::ExplicitTimezone => {
            if facets.explicit_timezone.is_some() { return Err(dup("explicitTimezone")); }
            // XSD 1.1 §4.3.16: explicitTimezone value must be one of required/prohibited/optional
            // (an `xs:NMTOKEN`, read after whitespace collapse, as for whiteSpace).
            let mode = match normalize_whitespace(&facet.value, WhitespaceMode::Collapse).as_ref() {
                "required" => ExplicitTimezone::Required,
                "prohibited" => ExplicitTimezone::Prohibited,
                "optional" => ExplicitTimezone::Optional,
                other => {
                    return Err(SchemaError::structural(
                        "st-props-correct",
                        format!(
                            "Invalid explicitTimezone value '{}': expected 'required', 'prohibited', or 'optional'",
                            other
                        ),
                        None,
                    ));
                }
            };
            facets.set_explicit_timezone(mode, fixed, facet.source);
        }
    }

    Ok(())
}

