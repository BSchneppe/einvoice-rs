//! Strict grammar for the pinned Schematron/xslt2 source dialect.
use crate::{source, xpath, Error, Result};
use roxmltree::Node;
use std::collections::{BTreeMap, BTreeSet};
use validation_ir::*;
pub const SCH: &str = "http://purl.oclc.org/dsdl/schematron";
pub const XSL: &str = "http://www.w3.org/1999/XSL/Transform";
type ScopeMap = BTreeMap<String, String>;
pub fn check(node: Node<'_, '_>, ns: &str, local: &str, attrs: &[&str]) -> Result<()> {
    if node.tag_name().namespace() != Some(ns) || node.tag_name().name() != local {
        let p = node.document().text_pos_at(node.range().start);
        return Err(Error::new(format!(
            "XML element {}:{}: unexpected qualified element {:?}",
            p.row,
            p.col,
            node.tag_name()
        )));
    }
    for attr in node.attributes() {
        if attr.namespace().is_some() || !attrs.contains(&attr.name()) {
            let p = node.document().text_pos_at(attr.range().start);
            return Err(Error::new(format!(
                "XML attribute {}:{}: unknown attribute {} on {local}",
                p.row,
                p.col,
                attr.name()
            )));
        }
    }
    for child in node.children().filter(Node::is_text) {
        if !child.text().unwrap_or("").trim().is_empty()
            && !["title", "assert", "variable"].contains(&local)
        {
            return Err(Error::new(format!("unexpected text in {local}")));
        }
    }
    Ok(())
}
fn req<'a>(node: Node<'a, '_>, name: &str) -> Result<&'a str> {
    node.attribute(name)
        .ok_or_else(|| Error::new(format!("missing {name} on {}", node.tag_name().name())))
}
fn empty(node: Node<'_, '_>) -> Result<()> {
    if node
        .children()
        .any(|n| n.is_element() || n.is_text() && !n.text().unwrap_or("").trim().is_empty())
    {
        return Err(Error::new("expected empty instruction"));
    }
    Ok(())
}
fn id(node: Node<'_, '_>, resource: &str) -> String {
    format!("{resource}@{}", node.range().start)
}
fn expression(
    node: Node<'_, '_>,
    attr: &str,
    resource: &str,
    c: &xpath::Context,
    pattern: bool,
) -> Result<Expression> {
    let text = req(node, attr)?.to_owned();
    let source = source::location(node, resource, Some(attr));
    let tree = if pattern {
        xpath::parse_pattern(&text, c)
    } else {
        xpath::parse(&text, c)
    }
    .map_err(|e| e.at(&source))?;
    Ok(Expression { text, source, tree })
}
fn sequence_type(
    node: Node<'_, '_>,
    resource: &str,
    c: &xpath::Context,
) -> Result<Option<SequenceType>> {
    node.attribute("as")
        .map(|s| {
            xpath::parse_type(s, c).map_err(|e| e.at(&source::location(node, resource, Some("as"))))
        })
        .transpose()
}
fn binding(
    node: Node<'_, '_>,
    resource: &str,
    owner: &str,
    scope: Scope,
    c: &xpath::Context,
) -> Result<Binding> {
    let (ns, local, attrs) = match node.tag_name().name() {
        "let" => (SCH, "let", vec!["name", "value"]),
        "param" => (XSL, "param", vec!["name", "as"]),
        "variable" => (XSL, "variable", vec!["name", "as", "select"]),
        _ => return Err(Error::new("unknown binding construct")),
    };
    check(node, ns, local, &attrs)?;
    let name = c.name(req(node, "name")?, false)?;
    let value = if local == "param" {
        empty(node)?;
        BindingValue::Parameter
    } else if local == "let" || node.has_attribute("select") {
        empty(node)?;
        BindingValue::Select {
            expression: expression(
                node,
                if local == "let" { "value" } else { "select" },
                resource,
                c,
                false,
            )?,
        }
    } else {
        BindingValue::Content {
            body: instructions(node, resource, owner, c, true)?,
        }
    };
    Ok(Binding {
        id: id(node, resource),
        name,
        declared_owner: owner.into(),
        initialization_context: if scope == Scope::Xslt2SchemaGlobal {
            "document-root"
        } else {
            "lexical-dynamic-context"
        }
        .into(),
        effective_scope: scope,
        source: source::location(node, resource, None),
        sequence_type: sequence_type(node, resource, c)?,
        value,
    })
}
fn instructions(
    parent: Node<'_, '_>,
    resource: &str,
    owner: &str,
    c: &xpath::Context,
    allow_text: bool,
) -> Result<Vec<Instruction>> {
    let mut out = Vec::new();
    for node in parent.children() {
        if node.is_text() {
            let text = node.text().unwrap_or("");
            if !text.trim().is_empty() {
                if !allow_text {
                    return Err(Error::new("literal text outside variable constructor"));
                }
                out.push(Instruction::Text { text: text.into() });
            }
            continue;
        }
        if !node.is_element() {
            continue;
        }
        let instr = match node.tag_name().name() {
            "variable" => Instruction::Variable {
                binding: binding(node, resource, owner, Scope::HelperLocal, c)?,
            },
            "sequence" => {
                check(node, XSL, "sequence", &["select"])?;
                empty(node)?;
                Instruction::Sequence {
                    expression: expression(node, "select", resource, c, false)?,
                }
            }
            "choose" => choose_instruction(node, resource, owner, c)?,
            _ => {
                return Err(Error::new(format!(
                    "unsupported helper instruction {:?}",
                    node.tag_name()
                )))
            }
        };
        out.push(instr);
    }
    Ok(out)
}
fn helper(node: Node<'_, '_>, resource: &str, c: &xpath::Context) -> Result<Helper> {
    check(node, XSL, "function", &["name", "as"])?;
    let name = c.name(req(node, "name")?, true)?;
    let result_type = sequence_type(node, resource, c)?
        .ok_or_else(|| Error::new("helper requires result type"))?;
    let mut parameters = Vec::new();
    let mut body_started = false;
    for child in node.children().filter(Node::is_element) {
        if child.tag_name().namespace() == Some(XSL) && child.tag_name().name() == "param" {
            if body_started {
                return Err(Error::new("parameter after helper body"));
            }
            parameters.push(binding(
                child,
                resource,
                &name.expanded(),
                Scope::HelperParameter,
                c,
            )?);
        } else {
            body_started = true;
        }
    }
    // Parse body without permitting params in general sequence constructors.
    let mut body = Vec::new();
    for child in node.children().filter(|n| {
        n.is_element() && !(n.tag_name().namespace() == Some(XSL) && n.tag_name().name() == "param")
    }) {
        // Parse a single child by parsing its enclosing constructor, excluding preceding params.
        match child.tag_name().name() {
            "variable" => body.push(Instruction::Variable {
                binding: binding(child, resource, &name.expanded(), Scope::HelperLocal, c)?,
            }),
            "sequence" => {
                check(child, XSL, "sequence", &["select"])?;
                empty(child)?;
                body.push(Instruction::Sequence {
                    expression: expression(child, "select", resource, c, false)?,
                });
            }
            "choose" => {
                // Reuse constructor parser through an explicit single-instruction helper.
                body.push(choose_instruction(child, resource, &name.expanded(), c)?);
            }
            _ => return Err(Error::new("unsupported helper instruction")),
        }
    }
    Ok(Helper {
        name,
        source: source::location(node, resource, None),
        result_type,
        parameters,
        body,
    })
}
fn choose_instruction(
    node: Node<'_, '_>,
    resource: &str,
    owner: &str,
    c: &xpath::Context,
) -> Result<Instruction> {
    check(node, XSL, "choose", &[])?;
    let mut branches = Vec::new();
    let mut otherwise = None;
    for b in node.children().filter(Node::is_element) {
        match b.tag_name().name() {
            "when" if otherwise.is_none() => {
                check(b, XSL, "when", &["test"])?;
                branches.push(Branch {
                    test: expression(b, "test", resource, c, false)?,
                    body: instructions(b, resource, owner, c, false)?,
                });
            }
            "otherwise" if otherwise.is_none() => {
                check(b, XSL, "otherwise", &[])?;
                otherwise = Some(instructions(b, resource, owner, c, false)?);
            }
            _ => return Err(Error::new("misplaced choose branch")),
        }
    }
    if branches.is_empty() {
        return Err(Error::new("choose requires when"));
    }
    Ok(Instruction::Choose {
        source: source::location(node, resource, None),
        branches,
        otherwise: otherwise.unwrap_or_default(),
    })
}
fn severity(flag: &str) -> Severity {
    match flag {
        "warning" | "warn" => Severity::Warning,
        "information" | "info" => Severity::Information,
        _ => Severity::Error,
    }
}
fn pattern(node: Node<'_, '_>, resource: &str, c: &xpath::Context) -> Result<Pattern> {
    check(node, SCH, "pattern", &["id"])?;
    let name = req(node, "id")?.to_owned();
    let mut bindings = Vec::new();
    let mut rules = Vec::new();
    let mut rules_started = false;
    for child in node.children().filter(Node::is_element) {
        match child.tag_name().name() {
            "let" if !rules_started => bindings.push(binding(
                child,
                resource,
                &format!("pattern:{name}"),
                Scope::Xslt2SchemaGlobal,
                c,
            )?),
            "rule" => {
                rules_started = true;
                check(child, SCH, "rule", &["context", "flag"])?;
                let identity = id(child, resource);
                let mut locals = Vec::new();
                let mut sites = Vec::new();
                let mut sites_started = false;
                let context = expression(child, "context", resource, c, true)?;
                for item in child.children().filter(Node::is_element) {
                    match item.tag_name().name() {
                        "let" if !sites_started => {
                            locals.push(binding(item, resource, &identity, Scope::RuleLocal, c)?)
                        }
                        "assert" => {
                            sites_started = true;
                            check(item, SCH, "assert", &["id", "flag", "test"])?;
                            let raw_flag = req(item, "flag")?.to_owned();
                            let test = expression(item, "test", resource, c, false)?;
                            let mut message = Vec::new();
                            for part in item.children() {
                                if part.is_text() {
                                    message.push(MessagePart::Text {
                                        text: part.text().unwrap_or("").into(),
                                    });
                                } else if part.is_element() {
                                    match part.tag_name().name() {
                                        "name" => {
                                            check(part, SCH, "name", &[])?;
                                            empty(part)?;
                                            message.push(MessagePart::Name {
                                                source: source::location(part, resource, None),
                                            });
                                        }
                                        "value-of" => {
                                            check(part, SCH, "value-of", &["select"])?;
                                            empty(part)?;
                                            message.push(MessagePart::ValueOf {
                                                expression: expression(
                                                    part, "select", resource, c, false,
                                                )?,
                                            });
                                        }
                                        _ => return Err(Error::new("unknown message construct")),
                                    }
                                }
                            }
                            sites.push(Site {
                                identity: id(item, resource),
                                source: source::location(item, resource, None),
                                id: req(item, "id")?.into(),
                                base_severity: severity(&raw_flag),
                                effective_severity: severity(&raw_flag),
                                raw_flag,
                                test,
                                message,
                            });
                        }
                        _ => return Err(Error::new("unknown/misplaced rule construct")),
                    }
                }
                rules.push(Rule {
                    identity,
                    source: source::location(child, resource, None),
                    flag: child.attribute("flag").map(str::to_owned),
                    context,
                    bindings: locals,
                    sites,
                });
            }
            _ => return Err(Error::new("unknown/misplaced pattern construct")),
        }
    }
    Ok(Pattern {
        id: name,
        source: source::location(node, resource, None),
        bindings,
        rules,
        active: false,
        activation_reason: String::new(),
    })
}
/// Parse one layer from an explicit in-memory resource closure; only declared includes resolve.
pub fn parse(
    resources: &BTreeMap<String, String>,
    entry: &str,
    selection: Option<&str>,
) -> Result<Schema> {
    parse_inner(resources, entry, selection).map_err(|e| Error::new(format!("{entry}: {e}")))
}
fn parse_inner(
    resources: &BTreeMap<String, String>,
    entry: &str,
    selection: Option<&str>,
) -> Result<Schema> {
    let text = resources
        .get(entry)
        .ok_or_else(|| Error::new("missing Schematron entry"))?;
    let doc = source::xml(text)?;
    let root = doc.root_element();
    check(root, SCH, "schema", &["queryBinding", "defaultPhase"])?;
    if req(root, "queryBinding")? != "xslt2" {
        return Err(Error::new("unsupported queryBinding"));
    }
    let mut c = xpath::Context::xslt2();
    let mut declared_prefixes = BTreeSet::new();
    for node in root
        .children()
        .filter(Node::is_element)
        .filter(|n| n.tag_name().namespace() == Some(SCH) && n.tag_name().name() == "ns")
    {
        check(node, SCH, "ns", &["prefix", "uri"])?;
        empty(node)?;
        let prefix = req(node, "prefix")?;
        let uri = req(node, "uri")?;
        if prefix.is_empty() || uri.is_empty() || !declared_prefixes.insert(prefix) {
            return Err(Error::new("invalid/duplicate namespace declaration"));
        }
        if let Some(old) = c.namespaces.insert(prefix.into(), uri.into()) {
            if old != uri {
                return Err(Error::new("conflicting namespace binding"));
            }
        }
    }
    for node in root
        .children()
        .filter(Node::is_element)
        .filter(|n| n.tag_name().namespace() == Some(XSL) && n.tag_name().name() == "function")
    {
        // This bounded dialect shares sch:ns plus xslt2 defaults with helper bodies.
        // XML rebinding would change XSLT QName semantics. Reject it everywhere in
        // the helper subtree rather than silently resolve against the wrong map.
        for instruction in node.descendants().filter(Node::is_element) {
            for namespace in instruction.namespaces() {
                if let Some(prefix) = namespace.name() {
                    if c.namespaces
                        .get(prefix)
                        .is_some_and(|uri| uri != namespace.uri())
                    {
                        return Err(Error::new(format!(
                            "unsupported helper XML namespace rebinding for {prefix}"
                        ))
                        .at(&source::location(instruction, entry, None)));
                    }
                }
            }
        }
        let name = c.name(req(node, "name")?, true)?;
        if name.namespace.is_empty() || name.namespace == xpath::FN || name.namespace == xpath::XS {
            return Err(Error::new("reserved helper namespace"));
        }
        let arity = node
            .children()
            .filter(Node::is_element)
            .filter(|n| n.tag_name().namespace() == Some(XSL) && n.tag_name().name() == "param")
            .count();
        if !c.helpers.insert((name.expanded(), arity)) {
            return Err(Error::new("duplicate helper signature"));
        }
    }
    let mut schema = Schema {
        source: source::location(root, entry, None),
        title: String::new(),
        namespaces: c.namespaces.clone(),
        namespace_declarations: root
            .children()
            .filter(Node::is_element)
            .filter(|n| n.tag_name().namespace() == Some(SCH) && n.tag_name().name() == "ns")
            .map(|n| NamespaceDeclaration {
                prefix: n.attribute("prefix").unwrap().into(),
                uri: n.attribute("uri").unwrap().into(),
                source: source::location(n, entry, None),
            })
            .collect(),
        xpath_element_default_namespace: String::new(),
        xpath_function_default_namespace: xpath::FN.into(),
        declared_default_phase: root.attribute("defaultPhase").map(str::to_owned),
        selection: selection.unwrap_or("#ALL").into(),
        dispatch: Dispatch::DocumentTraversalFirstMatchingRulePerPatternInSourceOrder,
        phases: Vec::new(),
        includes: Vec::new(),
        globals: Vec::new(),
        helpers: Vec::new(),
        patterns: Vec::new(),
    };
    let allowed = resources.keys().cloned().collect();
    let mut title_seen = false;
    for node in root.children().filter(Node::is_element) {
        let result = match (node.tag_name().namespace(), node.tag_name().name()) {
            (Some(SCH), "title") => {
                if title_seen {
                    return Err(Error::new("duplicate title"));
                }
                title_seen = true;
                check(node, SCH, "title", &[])?;
                if node.children().any(|n| n.is_element()) {
                    return Err(Error::new("markup in title"));
                }
                schema.title = node.text().unwrap_or("").into();
                Ok(())
            }
            (Some(SCH), "ns") => Ok(()),
            (Some(SCH), "phase") => {
                check(node, SCH, "phase", &["id"])?;
                let mut active_patterns = Vec::new();
                for a in node.children().filter(Node::is_element) {
                    check(a, SCH, "active", &["pattern"])?;
                    empty(a)?;
                    active_patterns.push(req(a, "pattern")?.into());
                }
                schema.phases.push(Phase {
                    id: req(node, "id")?.into(),
                    active_patterns,
                    source: source::location(node, entry, None),
                });
                Ok(())
            }
            (Some(SCH), "let") => {
                schema.globals.push(binding(
                    node,
                    entry,
                    "schema",
                    Scope::Xslt2SchemaGlobal,
                    &c,
                )?);
                Ok(())
            }
            (Some(SCH), "pattern") => {
                schema.patterns.push(pattern(node, entry, &c)?);
                Ok(())
            }
            (Some(SCH), "include") => {
                check(node, SCH, "include", &["href"])?;
                empty(node)?;
                let to = source::relative(entry, req(node, "href")?, &allowed)?;
                if to == entry {
                    return Err(Error::new("include cycle"));
                }
                let included = source::xml(&resources[&to])?;
                schema
                    .patterns
                    .push(pattern(included.root_element(), &to, &c)?);
                schema.includes.push(Include {
                    from: source::location(node, entry, None),
                    to,
                });
                Ok(())
            }
            (Some(XSL), "function") => {
                schema.helpers.push(helper(node, entry, &c)?);
                Ok(())
            }
            _ => Err(Error::new("unknown qualified schema construct")),
        };
        result.map_err(|e| e.at(&source::location(node, entry, None)))?;
    }
    if let Some(default) = root.attribute("defaultPhase") {
        if default != "#ALL" && !schema.phases.iter().any(|p| p.id == default) {
            return Err(Error::new("unknown default phase"));
        }
    }
    let mut pattern_ids = BTreeSet::new();
    for p in &schema.patterns {
        if !pattern_ids.insert(p.id.clone()) {
            return Err(Error::new("duplicate pattern"));
        }
    }
    let mut phase_ids = BTreeSet::new();
    for phase in &schema.phases {
        if !phase_ids.insert(&phase.id) {
            return Err(Error::new("duplicate phase"));
        }
        for a in &phase.active_patterns {
            if !pattern_ids.contains(a) {
                return Err(Error::new("phase references missing pattern"));
            }
        }
    }
    let active = if schema.selection == "#ALL" {
        pattern_ids
    } else {
        schema
            .phases
            .iter()
            .find(|p| p.id == schema.selection)
            .ok_or_else(|| Error::new("unknown selected phase"))?
            .active_patterns
            .iter()
            .cloned()
            .collect()
    };
    for p in &mut schema.patterns {
        p.active = active.contains(&p.id);
        p.activation_reason = if p.active {
            "selected; predicates determine contextual applicability"
        } else {
            "inactive in selected phase; source retained"
        }
        .into();
    }
    resolve_schema(&mut schema)?;
    Ok(schema)
}
fn insert(scope: &mut ScopeMap, b: &Binding) -> Result<()> {
    if scope.insert(b.name.expanded(), b.id.clone()).is_some() {
        return Err(Error::new(format!(
            "duplicate binding {}",
            b.name.qualified
        )));
    }
    Ok(())
}
fn resolve_expression(e: &mut Expression, scope: &ScopeMap) -> Result<()> {
    let prefix = format!("{}@{}", e.source.resource, e.source.xml_byte_start);
    xpath::resolve(&mut e.tree, scope, &prefix).map_err(|err| err.at(&e.source))
}
fn resolve_value(b: &mut Binding, scope: &ScopeMap) -> Result<()> {
    match &mut b.value {
        BindingValue::Select { expression } => resolve_expression(expression, scope),
        BindingValue::Content { body } => resolve_body(body, scope),
        BindingValue::Parameter => Ok(()),
    }
}
fn resolve_body(body: &mut [Instruction], outer: &ScopeMap) -> Result<()> {
    let mut scope = outer.clone();
    let mut declared = BTreeSet::new();
    for instruction in body {
        match instruction {
            Instruction::Variable { binding } => {
                resolve_value(binding, &scope)?;
                if !declared.insert(binding.name.expanded()) {
                    return Err(Error::new("duplicate sequence constructor local"));
                }
                scope.insert(binding.name.expanded(), binding.id.clone());
            }
            Instruction::Sequence { expression } => resolve_expression(expression, &scope)?,
            Instruction::Choose {
                branches,
                otherwise,
                ..
            } => {
                for branch in branches {
                    resolve_expression(&mut branch.test, &scope)?;
                    resolve_body(&mut branch.body, &scope)?;
                }
                resolve_body(otherwise, &scope)?;
            }
            Instruction::Text { .. } => (),
        }
    }
    Ok(())
}
fn body_dependencies(
    body: &[Instruction],
    vars: &mut BTreeSet<String>,
    calls: &mut BTreeSet<String>,
) {
    for i in body {
        match i {
            Instruction::Variable { binding } => value_dependencies(&binding.value, vars, calls),
            Instruction::Sequence { expression } => {
                xpath::dependencies(&expression.tree, vars, calls)
            }
            Instruction::Choose {
                branches,
                otherwise,
                ..
            } => {
                for b in branches {
                    xpath::dependencies(&b.test.tree, vars, calls);
                    body_dependencies(&b.body, vars, calls);
                }
                body_dependencies(otherwise, vars, calls);
            }
            _ => (),
        }
    }
}
fn value_dependencies(v: &BindingValue, vars: &mut BTreeSet<String>, calls: &mut BTreeSet<String>) {
    match v {
        BindingValue::Select { expression } => xpath::dependencies(&expression.tree, vars, calls),
        BindingValue::Content { body } => body_dependencies(body, vars, calls),
        _ => (),
    }
}
fn resolve_schema(s: &mut Schema) -> Result<()> {
    let mut globals = ScopeMap::new();
    for b in s
        .globals
        .iter()
        .chain(s.patterns.iter().flat_map(|p| p.bindings.iter()))
    {
        insert(&mut globals, b)?;
    }
    for b in s
        .globals
        .iter_mut()
        .chain(s.patterns.iter_mut().flat_map(|p| p.bindings.iter_mut()))
    {
        resolve_value(b, &globals)?;
    }
    for h in &mut s.helpers {
        let mut scope = globals.clone();
        let mut params = BTreeSet::new();
        for p in &h.parameters {
            if !params.insert(p.name.expanded()) {
                return Err(Error::new("duplicate helper parameter"));
            }
            scope.insert(p.name.expanded(), p.id.clone());
        }
        resolve_body(&mut h.body, &scope)?;
    }
    for p in &mut s.patterns {
        for r in &mut p.rules {
            resolve_expression(&mut r.context, &globals)?;
            let mut scope = globals.clone();
            let mut declared = BTreeSet::new();
            for b in &mut r.bindings {
                resolve_value(b, &scope)?;
                if !declared.insert(b.name.expanded()) {
                    return Err(Error::new("duplicate rule local"));
                }
                scope.insert(b.name.expanded(), b.id.clone());
            }
            for site in &mut r.sites {
                resolve_expression(&mut site.test, &scope)?;
                for m in &mut site.message {
                    if let MessagePart::ValueOf { expression } = m {
                        resolve_expression(expression, &scope)?;
                    }
                }
            }
        }
    }
    // Function recursion is legal; compute a finite transitive dependency closure before global-cycle checks.
    let mut helpers = BTreeMap::new();
    for h in &s.helpers {
        let mut vars = BTreeSet::new();
        let mut calls = BTreeSet::new();
        body_dependencies(&h.body, &mut vars, &mut calls);
        helpers.insert(
            format!("{}/{}", h.name.expanded(), h.parameters.len()),
            (vars, calls),
        );
    }
    loop {
        let old = helpers.clone();
        for (vars, calls) in helpers.values_mut() {
            for call in calls.clone() {
                if let Some((hv, hc)) = old.get(&call) {
                    vars.extend(hv.clone());
                    calls.extend(hc.clone());
                }
            }
        }
        if helpers == old {
            break;
        }
    }
    let global_ids: BTreeSet<_> = globals.values().cloned().collect();
    let mut graph = BTreeMap::new();
    for b in s
        .globals
        .iter()
        .chain(s.patterns.iter().flat_map(|p| p.bindings.iter()))
    {
        let mut vars = BTreeSet::new();
        let mut calls = BTreeSet::new();
        value_dependencies(&b.value, &mut vars, &mut calls);
        for call in calls {
            if let Some((hv, _)) = helpers.get(&call) {
                vars.extend(hv.clone());
            }
        }
        vars.retain(|v| global_ids.contains(v));
        graph.insert(b.id.clone(), vars);
    }
    fn visit(
        id: &str,
        graph: &BTreeMap<String, BTreeSet<String>>,
        active: &mut BTreeSet<String>,
        done: &mut BTreeSet<String>,
    ) -> Result<()> {
        if done.contains(id) {
            return Ok(());
        }
        if !active.insert(id.into()) {
            return Err(Error::new(format!("global initialization cycle at {id}")));
        }
        for dep in &graph[id] {
            visit(dep, graph, active, done)?;
        }
        active.remove(id);
        done.insert(id.into());
        Ok(())
    }
    let mut active = BTreeSet::new();
    let mut done = BTreeSet::new();
    for id in graph.keys() {
        visit(id, &graph, &mut active, &mut done)?;
    }
    Ok(())
}
